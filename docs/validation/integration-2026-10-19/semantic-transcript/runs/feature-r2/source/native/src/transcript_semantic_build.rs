//! One admitted host attempt. Durable source/attempt intent precedes dispatch;
//! no transaction spans a process, and no saved job authorizes another send.
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
            "{}: job {}; inspect feedback semantic-status with a fresh deadline; sends are never retried automatically",
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
    options(o, d)?;
    validate_request(r, d)?;
    require(
        store.db.is_autocommit(),
        "transcript_semantic_transaction_unavailable",
    )?;
    let mut committed = None;
    let mut registered = None;
    let run = store.with_exclusive_client(d, || {
        let (prepared, header_hash) = {
            let _read = storage::read_transaction(store)?;
            let mut c = saved::Context::new(store, o, d)?;
            let ids = c
                .source
                .ids(JOB_KIND, MAX_JOBS.min(o.max_events))
                .map_err(|_| refuse(CAPACITY))?;
            require(ids.len() < MAX_JOBS.min(o.max_events), CAPACITY)?;
            let contract = identity(&(
                OPERATION,
                &policy.contract_hash,
                "closed_transcript_utf8_candidates_v1",
            ));
            let v = payload(&mut c, r, &contract, o.max_candidates.min(128))?;
            let h = identity(&c.source.transcripts[&r.source.transcript_id].header);
            (v, h)
        };
        let payload_bytes = encode(&prepared, MAX_PAYLOAD, d)?;
        // Reserve enough read capacity for the complete future receipt/result
        // closure. A small host budget refuses before even reserving an attempt.
        let required = payload_bytes
            .len()
            .checked_add(MAX_REPLY)
            .and_then(|v| v.checked_add(policy.max_result_bytes))
            .and_then(|v| v.checked_add(MAX_JOB + 2 * 64_000 + 16_000))
            .ok_or_else(|| refuse(CAPACITY))?;
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
            workspace_id: store.workspace_id.clone(),
            request: r.clone(),
            transcript_header_hash: header_hash,
            payload_id: hash(&payload_bytes),
            contract_hash: prepared["contractHash"]
                .as_str()
                .ok_or_else(|| refuse(BINDING))?
                .into(),
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
            candidate_limit: o.max_candidates.min(128),
            max_result_bytes: policy.max_result_bytes,
            attempt_id: "0".repeat(64),
            request_id: "0".repeat(64),
            state: State::Reserved,
            response_id: None,
            result_id: None,
            failure: None,
        };
        j.id = job_id(&j);
        j.request_id = request_id(&j);
        encode(&j, MAX_JOB, d)?;
        wire(&j, &prepared, d)?;
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
                    saved::register_blob(tx, store, &payload_bytes, d)? == j.payload_id,
                    BINDING,
                )?;
                previous = saved::insert_job(tx, &j, d)?;
                Ok(())
            },
        )?;
        registered = Some(j.id.clone());
        let token = tokens
            .pop()
            .ok_or_else(|| failed("transcript_semantic_incomplete", &j))?;
        let outcome = (|| -> Result<Value> {
            let stdin = wire(&j, &prepared, d)?;
            let temp = tempfile::Builder::new()
                .prefix("yoke-transcript-semantic-")
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
                    stdin_bytes: MAX_WIRE,
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
            // A bound valid bill survives invalid usage/domain data, provider
            // errors, a nonzero exit and overrun. Never silently clamp a bill.
            ledger.settle(&j.attempt_id, reply.actual_cost_micros)?;
            if let Ok(Some(u)) = &valid_usage {
                ledger.usage(&j.attempt_id, "response", u.clone())?;
            }
            let mut responded = j.clone();
            responded.response_id = Some(hash(&out.stdout));
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
            // Recheck the immutable imported source after the process. The
            // original body/receipt remain durable even if later work refuses.
            let interpreted = (|| -> Result<Value> {
                require(
                    out.exit_code == Some(0) && reply.error.is_none() && valid_usage.is_ok(),
                    BINDING,
                )?;
                let _read = storage::read_transaction(store)?;
                let mut c = saved::Context::new(store, o, d)?;
                let expected = encode(
                    &payload(&mut c, &j.request, &j.contract_hash, j.candidate_limit)?,
                    MAX_PAYLOAD,
                    d,
                )?;
                require(
                    hash(&expected) == j.payload_id
                        && identity(&c.source.transcripts[&j.request.source.transcript_id].header)
                            == j.transcript_header_hash,
                    BINDING,
                )?;
                result(&c, &j, &reply, d)
            })();
            let mut completed = j.clone();
            let bytes = match interpreted {
                Ok(v) => Some(encode(&v, j.max_result_bytes, d)?),
                Err(_) => {
                    completed.state = State::Failed;
                    completed.failure = Some("transcript_semantic_bound_response_rejected".into());
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
                return Err(failed("transcript_semantic_bound_response_rejected", &j));
            }
            committed = Some(j.id.clone());
            d.check()?;
            Ok(saved::reference(&j))
        })();
        match outcome {
            Ok(v) => Ok(v),
            Err(_) if j.state == State::Complete => {
                Err(failed("transcript_semantic_publication_committed", &j))
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
                            "transcript_semantic_publication_failed"
                        } else {
                            "transcript_semantic_unbound_or_interrupted"
                        }
                        .into(),
                    );
                    let _ = saved::update(store, &previous, &j, d);
                }
                if error.downcast_ref::<BuildFailure>().is_some() {
                    Err(error)
                } else {
                    Err(failed("transcript_semantic_incomplete", &j))
                }
            }
        }
    });
    match run {
        Err(_) if committed.is_some() => Err(BuildFailure {
            code: "transcript_semantic_publication_committed",
            job_id: committed.unwrap(),
        }
        .into()),
        Err(error) if registered.is_some() && error.downcast_ref::<BuildFailure>().is_none() => {
            Err(BuildFailure {
                code: "transcript_semantic_incomplete",
                job_id: registered.unwrap(),
            }
            .into())
        }
        other => other,
    }
}
