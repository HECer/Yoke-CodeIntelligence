//! One explicit attempt, durable phase recovery, no transaction across a process.
use super::*;
use crate::ledger::Ledger;
#[derive(Debug)]
pub(super) struct BuildFailure {
    code: &'static str,
    job_id: String,
}
impl fmt::Display for BuildFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}: job {}; inspect meaning-identity status with a fresh deadline; never retry a send automatically",
            self.code, self.job_id
        )
    }
}
impl std::error::Error for BuildFailure {}
fn failed(code: &'static str, j: &Job) -> anyhow::Error {
    BuildFailure {
        code,
        job_id: j.id.clone(),
    }
    .into()
}
pub(super) fn build(
    store: &Store,
    r: &Request,
    p: &ProviderSpec,
    policy: &Policy,
    o: &Options,
    d: &Deadline,
) -> Result<Value> {
    validate_host_configuration(p, policy, d)?;
    validate_request(r, d)?;
    validate_options(o, d)?;
    require(
        store.db.is_autocommit(),
        "meaning_identity_transaction_unavailable",
    )?;
    let mut registered = None;
    let mut committed = None;
    let outcome = store.with_exclusive_client(d, || {
        let prepared = {
            let _read = saved::read_transaction(store)?;
            saved::admit_new_job(store, d)?;
            source::prepare(store, r, o, d)?
        };
        require(
            !prepared.candidates.is_empty(),
            "meaning_identity_no_candidates",
        )?;
        let contract = identity(&(
            OPERATION,
            &policy.contract_hash,
            "native_nfkc_context_lower_minhash64_lsh16x4_jaro_v1",
            "complete_same_entity_clique_or_refuse",
        ));
        let payload = source::payload(&prepared, &contract, d)?;
        let bytes = encode(&payload, MAX_PAYLOAD, d)?;
        let required = prepared
            .admitted_bytes
            .checked_add(bytes.len())
            .and_then(|n| n.checked_add(MAX_REPLY + MAX_JOB + 2 * 64_000 + 16_000))
            .and_then(|n| n.checked_add(policy.max_result_bytes))
            .ok_or_else(|| fail(CAPACITY))?;
        require(required <= o.max_input_bytes, CAPACITY)?;
        let (fingerprint, executable) = crate::meaning_build::fingerprint_host_provider(
            store,
            p,
            &policy.provider_input_files,
            d,
        )?;
        let mut nonce = [0u8; 32];
        getrandom::fill(&mut nonce)?;
        let mut j = Job {
            schema_version: 1,
            id: String::new(),
            nonce: hex::encode(nonce),
            request: r.clone(),
            binding: prepared.binding,
            payload_id: hash(&bytes),
            contract_hash: contract,
            provider: ProviderRecord {
                model: p.model_id.clone(),
                model_version: p.model_version.clone(),
                model_key: p.model_key()?,
                fingerprint,
                task_id: p.task_id.clone(),
                currency: p.currency.clone(),
                reservation: p.max_cost_micros,
                task_limit: p.task_limit_micros,
            },
            max_result_bytes: policy.max_result_bytes,
            attempt_id: "0".repeat(64),
            request_id: "0".repeat(64),
            state: State::Reserved,
            response_id: None,
            result_id: None,
            exit_code: None,
            failure: None,
        };
        j.id = job_id(&j);
        j.request_id = request_id(&j);
        // Reserve phase-field headroom before any irreversible provider send.
        encode(&j, MAX_JOB - 2048, d)?;
        wire(&j, &payload, d)?;
        encode(&saved::reference(&j), o.max_output_bytes, d)?;
        let ledger = Ledger::new(store, d);
        let mut previous = String::new();
        let mut tokens = ledger.reserve_many_registered(
            &p.task_id,
            &[p.max_cost_micros],
            p.task_limit_micros,
            &p.currency,
            &p.model_key()?,
            |tx, tokens| {
                j.attempt_id = tokens[0].id().into();
                j.request_id = request_id(&j);
                require(
                    saved::register_blob(tx, store, &bytes, d)? == j.payload_id,
                    BINDING,
                )?;
                previous = saved::insert(tx, &j, d)?;
                Ok(())
            },
        )?;
        registered = Some(j.id.clone());
        let token = tokens
            .pop()
            .ok_or_else(|| failed("meaning_identity_incomplete", &j))?;
        let run = (|| -> Result<Value> {
            let stdin = wire(&j, &payload, d)?;
            let temp = tempfile::Builder::new()
                .prefix("yoke-meaning-identity-")
                .tempdir()?;
            let mut command = std::process::Command::new(&executable);
            command.args(&p.args).current_dir(temp.path());
            require(ledger.send_reserved(token)? == j.attempt_id, BINDING)?;
            j.state = State::Sent;
            previous = saved::update(store, &previous, &j, d)?;
            let out = crate::process::run_capture(
                command,
                &stdin,
                d,
                crate::process::CaptureLimits {
                    stdin_bytes: MAX_PAYLOAD + 16_384,
                    stdout_bytes: MAX_REPLY,
                    stderr_bytes: 256_000,
                },
            )?;
            require(
                out.stdin_complete
                    && out.output_complete
                    && !out.timed_out
                    && !out.output_exceeded
                    && !out.io_error,
                BINDING,
            )?;
            let reply: Reply = decode(&out.stdout, MAX_REPLY)?;
            envelope(&reply, &j, d)?;
            let valid_usage = usage(&reply);
            ledger.settle(&j.attempt_id, reply.actual_cost_micros)?;
            if let Ok(Some(u)) = &valid_usage {
                ledger.usage(&j.attempt_id, "response", u.clone())?;
            }
            let mut responded = j.clone();
            responded.response_id = Some(hash(&out.stdout));
            responded.exit_code = out.exit_code;
            responded.state = State::Responded;
            {
                let tx = Transaction::new_unchecked(&store.db, TransactionBehavior::Immediate)?;
                require(
                    saved::register_blob(&tx, store, &out.stdout, d)?
                        == *responded.response_id.as_ref().unwrap(),
                    BINDING,
                )?;
                let next = saved::update(store, &previous, &responded, d)?;
                d.check()?;
                tx.commit()?;
                previous = next;
                j = responded;
            }
            let interpreted = (|| -> Result<Value> {
                require(valid_usage.is_ok(), BINDING)?;
                let _read = saved::read_transaction(store)?;
                let prepared = source::prepare(store, r, o, d)?;
                require(
                    prepared.binding == j.binding
                        && hash(encode(
                            &source::payload(&prepared, &j.contract_hash, d)?,
                            MAX_PAYLOAD,
                            d,
                        )?) == j.payload_id,
                    BINDING,
                )?;
                result(&prepared, &j, &reply, d)
            })();
            let mut completed = j.clone();
            let bytes = match interpreted {
                Ok(v) => Some(encode(&v, j.max_result_bytes, d)?),
                Err(_) => {
                    completed.state = State::Failed;
                    completed.failure = Some("meaning_identity_bound_response_rejected".into());
                    None
                }
            };
            {
                let tx = Transaction::new_unchecked(&store.db, TransactionBehavior::Immediate)?;
                if let Some(bytes) = &bytes {
                    completed.result_id = Some(saved::register_blob(&tx, store, bytes, d)?);
                    completed.state = State::Complete;
                }
                let next = saved::update(store, &previous, &completed, d)?;
                d.check()?;
                tx.commit()?;
                previous = next;
                j = completed;
            }
            if j.state != State::Complete {
                return Err(failed("meaning_identity_bound_response_rejected", &j));
            }
            committed = Some(j.id.clone());
            d.check()?;
            Ok(saved::reference(&j))
        })();
        match run {
            Ok(v) => Ok(v),
            Err(_) if j.state == State::Complete => {
                Err(failed("meaning_identity_publication_committed", &j))
            }
            Err(error) => {
                if let Ok(a) = ledger.attempt_status(&j.attempt_id) {
                    if a.state == AttemptState::Reserved {
                        let _ = ledger.cancel_before_send(&j.attempt_id);
                    } else if a.state == AttemptState::Sent {
                        let _ = ledger.settle(&j.attempt_id, None);
                    }
                }
                if j.state != State::Failed {
                    j.state = State::Failed;
                    j.failure = Some(
                        if j.response_id.is_some() {
                            "meaning_identity_publication_failed"
                        } else {
                            "meaning_identity_unbound_or_interrupted"
                        }
                        .into(),
                    );
                    let _ = saved::update(store, &previous, &j, d);
                }
                if error.downcast_ref::<BuildFailure>().is_some() {
                    Err(error)
                } else {
                    Err(failed("meaning_identity_incomplete", &j))
                }
            }
        }
    });
    match outcome {
        Err(_) if committed.is_some() => Err(BuildFailure {
            code: "meaning_identity_publication_committed",
            job_id: committed.unwrap(),
        }
        .into()),
        Err(e) if registered.is_some() && e.downcast_ref::<BuildFailure>().is_none() => {
            Err(BuildFailure {
                code: "meaning_identity_incomplete",
                job_id: registered.unwrap(),
            }
            .into())
        }
        other => other,
    }
}
