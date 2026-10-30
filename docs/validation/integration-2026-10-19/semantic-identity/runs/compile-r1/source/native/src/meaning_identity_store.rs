//! Bounded SQL/CAS admission and exact provider-free source/receipt replay.
use super::*;
use rusqlite::limits::Limit;
pub(super) fn read_transaction(store: &Store) -> Result<Option<Transaction<'_>>> {
    if store.db.is_autocommit() {
        Ok(Some(Transaction::new_unchecked(
            &store.db,
            TransactionBehavior::Deferred,
        )?))
    } else {
        Ok(None)
    }
}
struct LengthLimit<'a> {
    store: &'a Store,
    prior: i32,
}
impl Drop for LengthLimit<'_> {
    fn drop(&mut self) {
        let _ = self
            .store
            .db
            .set_limit(Limit::SQLITE_LIMIT_LENGTH, self.prior);
    }
}
fn limited<T>(store: &Store, f: impl FnOnce() -> Result<T>) -> Result<T> {
    let prior = store.db.limit(Limit::SQLITE_LIMIT_LENGTH)?;
    store.db.set_limit(
        Limit::SQLITE_LIMIT_LENGTH,
        prior.min((MAX_JOB + 65_536) as i32),
    )?;
    let _guard = LengthLimit { store, prior };
    f()
}
#[derive(Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Artifact {
    artifact_id: String,
    total_bytes: usize,
}
pub(super) struct Context<'a> {
    store: &'a Store,
    o: &'a Options,
    d: &'a Deadline,
    input_bytes: usize,
    rows: BTreeMap<(String, String), String>,
    artifacts: BTreeMap<String, usize>,
}
impl<'a> Context<'a> {
    fn new(store: &'a Store, o: &'a Options, d: &'a Deadline) -> Result<Self> {
        validate_options(o, d)?;
        store.set_deadline(d)?;
        Ok(Self {
            store,
            o,
            d,
            input_bytes: 0,
            rows: BTreeMap::new(),
            artifacts: BTreeMap::new(),
        })
    }
    fn charge(&mut self, n: usize) -> Result<()> {
        self.input_bytes = self
            .input_bytes
            .checked_add(n)
            .ok_or_else(|| fail(CAPACITY))?;
        require(self.input_bytes <= self.o.max_input_bytes, CAPACITY)
    }
    fn raw(&mut self, kind: &str, id: &str, max: usize) -> Result<String> {
        self.d.check()?;
        require(valid_id(id), INPUT)?;
        let key = (kind.into(), id.into());
        if let Some(v) = self.rows.get(&key) {
            return Ok(v.clone());
        }
        let v = limited(self.store, || {
            let size: Option<usize> = self
                .store
                .db
                .query_row(
                    "SELECT length(CAST(value AS BLOB)) FROM records WHERE kind=?1 AND id=?2",
                    params![kind, id],
                    |r| r.get(0),
                )
                .optional()?;
            let size = size.ok_or_else(|| fail(BINDING))?;
            require(size <= max, CAPACITY)?;
            self.charge(size)?;
            let raw:String=self.store.db.query_row("SELECT value FROM records WHERE kind=?1 AND id=?2 AND length(CAST(value AS BLOB))=?3",params![kind,id,size],|r|r.get(0))?;
            require(raw.len() == size, BINDING)?;
            Ok(raw)
        })?;
        self.rows.insert(key, v.clone());
        Ok(v)
    }
    fn source(&mut self, j: &Job) -> Result<Prepared> {
        let o = Options {
            max_input_bytes: self.o.max_input_bytes.saturating_sub(self.input_bytes),
            ..self.o.clone()
        };
        let p = source::prepare(self.store, &j.request, &o, self.d)?;
        self.charge(p.admitted_bytes)?;
        require(p.binding == j.binding, BINDING)?;
        Ok(p)
    }
    fn admit(&mut self, id: &str, max: usize) -> Result<()> {
        let raw = self.raw("artifact", id, 64_000)?;
        let a: Artifact = decode(raw.as_bytes(), 64_000)?;
        require(a.artifact_id == id, BINDING)?;
        require(a.total_bytes <= max, CAPACITY)?;
        if let Some(old) = self.artifacts.get(id) {
            return require(*old == a.total_bytes, BINDING);
        }
        self.charge(a.total_bytes)?;
        self.artifacts.insert(id.into(), a.total_bytes);
        Ok(())
    }
    fn artifact(&self, id: &str, max: usize) -> Result<Vec<u8>> {
        self.d.check()?;
        self.store
            .read_blob_exact(
                id,
                *self.artifacts.get(id).ok_or_else(|| fail(BINDING))?,
                max,
            )
            .map_err(|_| fail(BINDING))
    }
}
pub(super) fn ids(store: &Store, maximum: usize, d: &Deadline) -> Result<Vec<String>> {
    d.check()?;
    limited(store, || {
        let (count,bytes,bad):(usize,usize,usize)=store.db.query_row("SELECT count(*),coalesce(sum(length(CAST(value AS BLOB))),0),coalesce(sum(CASE WHEN length(CAST(id AS BLOB))!=64 OR length(CAST(value AS BLOB))>?2 THEN 1 ELSE 0 END),0) FROM records WHERE kind=?1",params![JOB_KIND,MAX_JOB],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?)))?;
        require(
            count <= maximum && bytes <= 32_000_000 && bad == 0,
            CAPACITY,
        )?;
        let mut s = store
            .db
            .prepare("SELECT id FROM records WHERE kind=?1 ORDER BY id")?;
        let result = s
            .query_map([JOB_KIND], |r| r.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        require(
            result.len() == count && result.iter().all(|id| valid_id(id)),
            BINDING,
        )?;
        Ok(result)
    })
}
pub(super) fn register_blob(
    tx: &Transaction<'_>,
    store: &Store,
    bytes: &[u8],
    d: &Deadline,
) -> Result<String> {
    d.check()?;
    let id = store.blob(bytes)?;
    let value = Artifact {
        artifact_id: id.clone(),
        total_bytes: bytes.len(),
    };
    let prior = limited(store, || {
        let raw:Option<Vec<u8>>=tx.query_row("SELECT substr(CAST(value AS BLOB),1,64001) FROM records WHERE kind='artifact' AND id=?1",[&id],|r|r.get(0)).optional()?;
        Ok(raw)
    })?;
    if let Some(raw) = prior {
        require(decode::<Artifact>(&raw, 64_000)? == value, BINDING)?;
    } else {
        tx.execute(
            "INSERT INTO records VALUES('artifact',?1,?2)",
            params![id, String::from_utf8(encode(&value, 64_000, d)?)?],
        )?;
    }
    Ok(id)
}
pub(super) fn insert(tx: &Transaction<'_>, j: &Job, d: &Deadline) -> Result<String> {
    let raw = String::from_utf8(encode(j, MAX_JOB, d)?)?;
    tx.execute(
        "INSERT INTO records VALUES(?1,?2,?3)",
        params![JOB_KIND, j.id, raw],
    )?;
    Ok(raw)
}
pub(super) fn update(store: &Store, old: &str, j: &Job, d: &Deadline) -> Result<String> {
    d.check()?;
    let raw = String::from_utf8(encode(j, MAX_JOB, d)?)?;
    require(
        store.db.execute(
            "UPDATE records SET value=?3 WHERE kind=?1 AND id=?2 AND value=?4",
            params![JOB_KIND, j.id, raw, old],
        )? == 1,
        BINDING,
    )?;
    d.check()?;
    Ok(raw)
}
fn validate_job(j: &Job, store: &Store, d: &Deadline) -> Result<()> {
    validate_request(&j.request, d)?;
    require(
        j.schema_version == 1
            && j.request.selection.workspace_id == store.workspace_id
            && j.request.selection == j.binding.selection
            && valid_id(&j.nonce)
            && valid_id(&j.binding.source_hash)
            && valid_id(&j.binding.source_projection_id)
            && valid_id(&j.binding.candidate_set_id)
            && valid_id(&j.payload_id)
            && valid_id(&j.contract_hash)
            && valid_id(&j.attempt_id)
            && valid_id(&j.provider.fingerprint)
            && j.id == job_id(j)
            && j.request_id == request_id(j)
            && (REQUIRED_RESULT..=MAX_RESULT).contains(&j.max_result_bytes),
        BINDING,
    )?;
    label(&j.provider.model, 100)?;
    label(&j.provider.model_version, 100)?;
    label(&j.provider.task_id, 200)?;
    require(
        !j.provider.model.contains('@')
            && !j.provider.model_version.contains('@')
            && j.provider.model_key == format!("{}@{}", j.provider.model, j.provider.model_version)
            && j.provider.currency.len() == 3
            && j.provider.currency.bytes().all(|b| b.is_ascii_uppercase()),
        BINDING,
    )?;
    validate_cost(j.provider.reservation).map_err(|_| fail(BINDING))?;
    validate_cost(j.provider.task_limit).map_err(|_| fail(BINDING))?;
    require(
        j.response_id.as_ref().is_none_or(|id| valid_id(id))
            && j.result_id.as_ref().is_none_or(|id| valid_id(id))
            && (j.response_id.is_some() || j.exit_code.is_none()),
        BINDING,
    )?;
    require(
        match j.state {
            State::Reserved | State::Sent => {
                j.response_id.is_none() && j.result_id.is_none() && j.failure.is_none()
            }
            State::Responded => {
                j.response_id.is_some() && j.result_id.is_none() && j.failure.is_none()
            }
            State::Complete => {
                j.response_id.is_some()
                    && j.result_id.is_some()
                    && j.exit_code == Some(0)
                    && j.failure.is_none()
            }
            State::Failed => {
                j.result_id.is_none()
                    && j.failure.as_deref().is_some_and(|s| {
                        matches!(
                            s,
                            "meaning_identity_bound_response_rejected"
                                | "meaning_identity_unbound_or_interrupted"
                                | "meaning_identity_publication_failed"
                        )
                    })
            }
        },
        BINDING,
    )
}
fn read_job(c: &mut Context<'_>, id: &str) -> Result<Job> {
    let raw = c.raw(JOB_KIND, id, MAX_JOB)?;
    let j: Job = decode(raw.as_bytes(), MAX_JOB)?;
    require(j.id == id, BINDING)?;
    validate_job(&j, c.store, c.d)?;
    Ok(j)
}
fn attempt(c: &mut Context<'_>, j: &Job) -> Result<AttemptStatus> {
    let a = c.raw("native_model_attempt_v1", &j.attempt_id, 64_000)?;
    let task = identity(&j.provider.task_id);
    let t = c.raw("native_model_task_v1", &task, 64_000)?;
    let av = crate::json::parse(a.as_bytes(), 64_000).map_err(|_| fail(BINDING))?;
    let tv = crate::json::parse(t.as_bytes(), 64_000).map_err(|_| fail(BINDING))?;
    let a = crate::ledger::checked_archived_attempt(&j.attempt_id, &av, &task, &tv, c.d)
        .map_err(|_| fail(BINDING))?;
    require(
        a.task_id == j.provider.task_id
            && a.provider == j.provider.model_key
            && a.currency == j.provider.currency
            && a.reserved_micros == j.provider.reservation
            && tv["limitMicros"].as_u64() == Some(j.provider.task_limit),
        BINDING,
    )?;
    Ok(a)
}
fn checked(c: &mut Context<'_>, j: &Job) -> Result<(AttemptStatus, Option<Reply>, Option<Value>)> {
    let p = c.source(j)?;
    let expected = encode(
        &source::payload(&p, &j.contract_hash, c.d)?,
        MAX_PAYLOAD,
        c.d,
    )?;
    require(hash(&expected) == j.payload_id, BINDING)?;
    let a = attempt(c, j)?;
    c.admit(&j.payload_id, MAX_PAYLOAD)?;
    if let Some(id) = &j.response_id {
        c.admit(id, MAX_REPLY)?;
    }
    if let Some(id) = &j.result_id {
        c.admit(id, j.max_result_bytes)?;
    }
    require(c.artifact(&j.payload_id, MAX_PAYLOAD)? == expected, BINDING)?;
    let r = if let Some(id) = &j.response_id {
        let raw = c.artifact(id, MAX_REPLY)?;
        let r: Reply = decode(&raw, MAX_REPLY)?;
        envelope(&r, j, c.d)?;
        require(
            matches!(a.state, AttemptState::Unknown | AttemptState::Settled)
                && r.actual_cost_micros
                    .is_none_or(|cost| a.actual_cost_micros == Some(cost)),
            BINDING,
        )?;
        if let Ok(Some(u)) = usage(&r) {
            require(a.response_usage.as_ref() == Some(&u), BINDING)?;
        }
        Some(r)
    } else {
        None
    };
    let value = if let Some(id) = &j.result_id {
        require(j.state == State::Complete, BINDING)?;
        let raw = c.artifact(id, j.max_result_bytes)?;
        let v = result(&p, j, r.as_ref().ok_or_else(|| fail(BINDING))?, c.d)?;
        require(raw == encode(&v, j.max_result_bytes, c.d)?, BINDING)?;
        Some(v)
    } else {
        None
    };
    Ok((a, r, value))
}
pub(super) fn reference(j: &Job) -> Value {
    json!({"schemaVersion":1,"kind":"meaning_identity_job_reference_v1","jobId":j.id,"state":j.state,"attemptId":j.attempt_id,"responseArtifactId":j.response_id,"resultArtifactId":j.result_id,
    "failure":j.failure,"automaticRetryAvailable":false,"sourceWritten":false,"semanticIdentityEstablished":false})
}
pub(super) fn load(store: &Store, id: &str, o: &Options, d: &Deadline) -> Result<CheckedGraph> {
    let mut c = Context::new(store, o, d)?;
    let _read = read_transaction(store)?;
    let j = read_job(&mut c, id)?;
    let (_, _, value) = checked(&mut c, &j)?;
    require(
        j.state == State::Complete,
        "meaning_identity_job_incomplete",
    )?;
    let value = value.ok_or_else(|| fail(BINDING))?;
    encode(&value, o.max_output_bytes, d)?;
    Ok(CheckedGraph { value })
}
pub(super) fn status(store: &Store, id: &str, o: &Options, d: &Deadline) -> Result<Value> {
    let mut c = Context::new(store, o, d)?;
    let _read = read_transaction(store)?;
    let j = read_job(&mut c, id)?;
    let (a, r, _) = checked(&mut c, &j)?;
    let mut v = reference(&j);
    v["sourceBinding"] = json!(j.binding);
    v["providerCallsThisRead"] = json!(0);
    v["originalExitCode"] = json!(j.exit_code);
    v["currentAttemptObservation"] = json!(a);
    v["currentAttemptScope"] = json!("current_ledger_observation_not_original_response_billing");
    v["originalResponseReceipt"]=r.map_or(Value::Null,|r|json!({"providerReportedCostMicros":r.actual_cost_micros,"providerReportedUsage":usage(&r).ok().flatten(),"providerReportedUsageValid":usage(&r).is_ok(),"providerErrorPresent":r.error.is_some()}));
    encode(&v, o.max_output_bytes, d)?;
    Ok(v)
}
pub(super) fn jobs(
    store: &Store,
    task: &str,
    offset: usize,
    limit: usize,
    o: &Options,
    d: &Deadline,
) -> Result<Value> {
    let mut c = Context::new(store, o, d)?;
    label(task, 200)?;
    require((1..=100).contains(&limit), INPUT)?;
    let _read = read_transaction(store)?;
    let mut selected = Vec::new();
    for id in ids(store, MAX_JOBS, d)? {
        let j = read_job(&mut c, &id)?;
        if j.provider.task_id == task {
            selected.push(reference(&j));
        }
    }
    let total = selected.len();
    let items = selected
        .into_iter()
        .skip(offset)
        .take(limit)
        .collect::<Vec<_>>();
    let v = json!({"kind":"meaning_identity_jobs_v1","taskId":task,"totalJobs":total,"offset":offset,"limit":limit,"returnedJobs":items.len(),"items":items,"allJobHeadersChecked":true,"dependencyReplayPerformed":false,"providerCallsThisRead":0,"complete":false});
    encode(&v, o.max_output_bytes, d)?;
    Ok(v)
}
pub(super) fn retained(
    store: &Store,
    id: &str,
    v: &Value,
    d: &Deadline,
) -> Result<crate::meaning_build::MeaningDependencies> {
    let o = Options {
        max_similarity_work: 128_000_000,
        max_input_bytes: 256_000_000,
        max_output_bytes: MAX_RESULT,
        ..Default::default()
    };
    let mut c = Context::new(store, &o, d)?;
    let _read = read_transaction(store)?;
    let j = read_job(&mut c, id)?;
    require(
        crate::json::parse(c.rows[&(JOB_KIND.into(), id.into())].as_bytes(), MAX_JOB)
            .map_err(|_| fail(BINDING))?
            == *v,
        BINDING,
    )?;
    checked(&mut c, &j)?;
    Ok(crate::meaning_build::MeaningDependencies {
        snapshots: vec![j.request.selection.snapshot_id],
        artifacts: c.artifacts.into_keys().collect(),
        records: j
            .request
            .selection
            .graph_ids
            .into_iter()
            .map(|id| (crate::meaning_build::GRAPH_KIND.into(), id))
            .collect(),
    })
}
