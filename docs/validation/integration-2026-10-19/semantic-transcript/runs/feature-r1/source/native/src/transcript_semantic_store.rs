//! Bounded source/receipt replay. No provider lookup or execution in read paths.
use super::*;

#[derive(Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Artifact {
    artifact_id: String,
    total_bytes: usize,
}
pub(super) struct Context<'a> {
    pub source: storage::Context<'a>,
    artifacts: BTreeMap<String, usize>,
}
impl<'a> Context<'a> {
    pub fn new(store: &'a Store, o: &'a Options, d: &'a Deadline) -> Result<Self> {
        options(o, d)?;
        Ok(Self {
            source: storage::Context::new(store, o, d).map_err(|_| refuse(BINDING))?,
            artifacts: BTreeMap::new(),
        })
    }
    pub fn raw(&mut self, kind: &str, id: &str) -> Result<Option<String>> {
        self.source.raw(kind, id).map_err(|e| {
            if e.to_string() == "feedback_capacity_exceeded" {
                refuse(CAPACITY)
            } else {
                refuse(BINDING)
            }
        })
    }
    pub fn transcript(&mut self, id: &str) -> Result<()> {
        let raw = self
            .raw(TRANSCRIPT_KIND, id)?
            .ok_or_else(|| refuse(BINDING))?;
        let h: transcript::Header = decode(raw.as_bytes(), MAX_RECORD_BYTES)?;
        transcript::validate_header(&h, self.source.store).map_err(|_| refuse(BINDING))?;
        require(h.id == id, BINDING)?;
        if !self.source.transcripts.contains_key(id) {
            require(
                h.source_bytes
                    <= self
                        .source
                        .options
                        .max_source_bytes
                        .saturating_sub(self.source.source_bytes),
                CAPACITY,
            )?;
        }
        self.source.transcript(id).map_err(|_| refuse(BINDING))
    }
    pub fn admit_artifact(&mut self, id: &str, max: usize) -> Result<()> {
        let raw = self.raw("artifact", id)?.ok_or_else(|| refuse(BINDING))?;
        let a: Artifact = decode(raw.as_bytes(), 64_000)?;
        require(a.artifact_id == id, BINDING)?;
        require(a.total_bytes <= max, CAPACITY)?;
        if let Some(prior) = self.artifacts.get(id) {
            require(*prior == a.total_bytes, BINDING)?;
            return Ok(());
        }
        require(
            a.total_bytes
                <= self
                    .source
                    .options
                    .max_input_bytes
                    .saturating_sub(self.source.input_bytes),
            CAPACITY,
        )?;
        self.source.input_bytes += a.total_bytes;
        self.artifacts.insert(id.into(), a.total_bytes);
        Ok(())
    }
    pub fn artifact(&self, id: &str, max: usize) -> Result<Vec<u8>> {
        self.source.deadline.check()?;
        let size = *self.artifacts.get(id).ok_or_else(|| refuse(BINDING))?;
        self.source
            .store
            .read_blob_exact(id, size, max)
            .map_err(|_| refuse(BINDING))
    }
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
    let prior: Option<String> = storage::limited(store, || {
        Ok(tx.query_row("SELECT substr(CAST(value AS BLOB),1,64001) FROM records WHERE kind='artifact' AND id=?1", [&id], |r| r.get::<_,Vec<u8>>(0))
            .optional()?.map(String::from_utf8).transpose()?)
    })?;
    if let Some(raw) = prior {
        require(
            decode::<Artifact>(raw.as_bytes(), 64_000)? == value,
            BINDING,
        )?;
    } else {
        let raw = encode(&value, 64_000, d)?;
        tx.execute(
            "INSERT INTO records VALUES('artifact',?1,?2)",
            params![id, String::from_utf8(raw)?],
        )?;
    }
    Ok(id)
}
pub(super) fn insert_job(tx: &Transaction<'_>, j: &Job, d: &Deadline) -> Result<()> {
    let raw = encode(j, MAX_JOB, d)?;
    tx.execute(
        "INSERT INTO records VALUES(?1,?2,?3)",
        params![JOB_KIND, j.id, String::from_utf8(raw)?],
    )?;
    Ok(())
}
pub(super) fn update(store: &Store, previous: &str, j: &Job, d: &Deadline) -> Result<String> {
    d.check()?;
    let next = String::from_utf8(encode(j, MAX_JOB, d)?)?;
    require(
        store.db.execute(
            "UPDATE records SET value=?3 WHERE kind=?1 AND id=?2 AND value=?4",
            params![JOB_KIND, j.id, next, previous],
        )? == 1,
        BINDING,
    )?;
    d.check()?;
    Ok(next)
}
pub(super) fn validate_job(j: &Job, store: &Store, d: &Deadline) -> Result<()> {
    validate_request(&j.request, d)?;
    require(
        j.schema_version == 1
            && j.workspace_id == store.workspace_id
            && valid_id(&j.nonce)
            && valid_id(&j.transcript_header_hash)
            && valid_id(&j.payload_id)
            && valid_id(&j.contract_hash)
            && valid_id(&j.provider.fingerprint)
            && valid_id(&j.attempt_id)
            && j.id == job_id(j)
            && j.request_id == request_id(j)
            && (1..=128).contains(&j.candidate_limit)
            && (REQUIRED_RESULT..=MAX_RESULT).contains(&j.max_result_bytes),
        BINDING,
    )?;
    for (v, max) in [
        (&j.provider.model, 100),
        (&j.provider.model_version, 100),
        (&j.provider.task_id, 200),
    ] {
        bounded_text(v, max, true).map_err(|_| refuse(BINDING))?;
        require(!v.chars().any(char::is_control), BINDING)?;
    }
    require(
        !j.provider.model.contains('@')
            && !j.provider.model_version.contains('@')
            && j.provider.model_key == format!("{}@{}", j.provider.model, j.provider.model_version)
            && j.provider.currency.len() == 3
            && j.provider.currency.bytes().all(|b| b.is_ascii_uppercase()),
        BINDING,
    )?;
    validate_cost(j.provider.reservation).map_err(|_| refuse(BINDING))?;
    validate_cost(j.provider.task_limit).map_err(|_| refuse(BINDING))?;
    require(
        j.response_id.as_ref().is_none_or(|id| valid_id(id))
            && j.result_id.as_ref().is_none_or(|id| valid_id(id)),
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
                j.response_id.is_some() && j.result_id.is_some() && j.failure.is_none()
            }
            State::Failed => {
                j.result_id.is_none()
                    && j.failure.as_deref().is_some_and(|f| {
                        matches!(
                            f,
                            "transcript_semantic_unbound_or_interrupted"
                                | "transcript_semantic_bound_response_rejected"
                                | "transcript_semantic_publication_failed"
                        )
                    })
            }
        },
        BINDING,
    )?;
    Ok(())
}
pub(super) fn read_job(c: &mut Context<'_>, id: &str) -> Result<(Job, String)> {
    require(valid_id(id), INPUT)?;
    let raw = c.raw(JOB_KIND, id)?.ok_or_else(|| refuse(BINDING))?;
    let j: Job = decode(raw.as_bytes(), MAX_JOB)?;
    require(j.id == id, BINDING)?;
    validate_job(&j, c.source.store, c.source.deadline)?;
    Ok((j, raw))
}
fn attempt(c: &mut Context<'_>, j: &Job) -> Result<AttemptStatus> {
    let a = c
        .raw("native_model_attempt_v1", &j.attempt_id)?
        .ok_or_else(|| refuse(BINDING))?;
    let task_id = identity(&j.provider.task_id);
    let t = c
        .raw("native_model_task_v1", &task_id)?
        .ok_or_else(|| refuse(BINDING))?;
    require(a.len() <= 64_000 && t.len() <= 64_000, CAPACITY)?;
    let av = crate::json::parse(a.as_bytes(), 64_000).map_err(|_| refuse(BINDING))?;
    let tv = crate::json::parse(t.as_bytes(), 64_000).map_err(|_| refuse(BINDING))?;
    let status = crate::ledger::checked_archived_attempt(
        &j.attempt_id,
        &av,
        &task_id,
        &tv,
        c.source.deadline,
    )
    .map_err(|_| refuse(BINDING))?;
    require(
        status.task_id == j.provider.task_id
            && status.provider == j.provider.model_key
            && status.currency == j.provider.currency
            && status.reserved_micros == j.provider.reservation
            && tv["limitMicros"].as_u64() == Some(j.provider.task_limit),
        BINDING,
    )?;
    Ok(status)
}
pub(super) fn checked(
    c: &mut Context<'_>,
    j: &Job,
) -> Result<(AttemptStatus, Option<Reply>, Option<Value>)> {
    let d = c.source.deadline;
    c.transcript(&j.request.source.transcript_id)?;
    require(
        identity(&c.source.transcripts[&j.request.source.transcript_id].header)
            == j.transcript_header_hash,
        BINDING,
    )?;
    let expected = encode(
        &payload(c, &j.request, &j.contract_hash, j.candidate_limit)?,
        MAX_PAYLOAD,
        d,
    )?;
    require(hash(&expected) == j.payload_id, BINDING)?;
    let a = attempt(c, j)?;
    // Admit the entire artifact closure before reading any payload/reply/result CAS.
    c.admit_artifact(&j.payload_id, MAX_PAYLOAD)?;
    if let Some(id) = &j.response_id {
        c.admit_artifact(id, MAX_REPLY)?;
    }
    if let Some(id) = &j.result_id {
        c.admit_artifact(id, j.max_result_bytes)?;
    }
    require(c.artifact(&j.payload_id, MAX_PAYLOAD)? == expected, BINDING)?;
    let response = if let Some(id) = &j.response_id {
        let raw = c.artifact(id, MAX_REPLY)?;
        let r: Reply = decode(&raw, MAX_REPLY)?;
        envelope(&r, j, d)?;
        require(
            matches!(a.state, AttemptState::Unknown | AttemptState::Settled)
                && r.actual_cost_micros
                    .is_none_or(|v| a.actual_cost_micros == Some(v)),
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
        let expected = result(c, j, response.as_ref().ok_or_else(|| refuse(BINDING))?, d)?;
        require(raw == encode(&expected, j.max_result_bytes, d)?, BINDING)?;
        Some(expected)
    } else {
        None
    };
    Ok((a, response, value))
}
pub(super) fn reference(j: &Job) -> Value {
    json!({"schemaVersion":1,"kind":"transcript_semantic_job_reference_v1",
    "jobId":j.id,"state":j.state,"attemptId":j.attempt_id,"resultArtifactId":j.result_id,
    "responseArtifactId":j.response_id,"failure":j.failure,"eventsCreated":0,"sourceWritten":false,
    "automaticRetryAvailable":false,"candidateOnly":true})
}
pub(super) fn load(
    store: &Store,
    id: &str,
    o: &Options,
    d: &Deadline,
) -> Result<CheckedCandidates> {
    options(o, d)?;
    store.set_deadline(d)?;
    let _read = storage::read_transaction(store)?;
    let mut c = Context::new(store, o, d)?;
    let (j, _) = read_job(&mut c, id)?;
    let (_, _, v) = checked(&mut c, &j)?;
    require(
        j.state == State::Complete,
        "transcript_semantic_job_incomplete",
    )?;
    let value = v.ok_or_else(|| refuse(BINDING))?;
    encode(&value, o.max_output_bytes, d)?;
    Ok(CheckedCandidates { value })
}
pub(super) fn status(store: &Store, id: &str, o: &Options, d: &Deadline) -> Result<Value> {
    options(o, d)?;
    store.set_deadline(d)?;
    let _read = storage::read_transaction(store)?;
    let mut c = Context::new(store, o, d)?;
    let (j, _) = read_job(&mut c, id)?;
    let (a, r, _) = checked(&mut c, &j)?;
    let mut v = reference(&j);
    v["providerCallsThisRead"] = json!(0);
    v["source"] = json!(j.request.source);
    v["currentAttemptObservation"] = json!(a);
    v["currentAttemptScope"] = json!("current_ledger_observation_not_original_response_billing");
    v["originalResponseReceipt"]=r.as_ref().map_or(Value::Null,|r|json!({"providerReportedCostMicros":r.actual_cost_micros,
        "providerReportedUsage":usage(r).ok().flatten(),"providerReportedUsageValid":usage(r).is_ok(),"providerErrorPresent":r.error.is_some()}));
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
    options(o, d)?;
    bounded_text(task, 200, true).map_err(|_| refuse(INPUT))?;
    require((1..=100).contains(&limit), INPUT)?;
    store.set_deadline(d)?;
    let _read = storage::read_transaction(store)?;
    let mut c = Context::new(store, o, d)?;
    let ids = c
        .source
        .ids(JOB_KIND, MAX_JOBS.min(o.max_events))
        .map_err(|_| refuse(CAPACITY))?;
    let mut selected = Vec::new();
    for id in &ids {
        d.check()?;
        let (j, _) = read_job(&mut c, id)?;
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
    let v = json!({"schemaVersion":1,"taskId":task,"offset":offset,"limit":limit,"totalJobs":total,"returnedJobs":items.len(),"items":items,
        "allJobHeadersChecked":true,"dependencyReplayPerformed":false,"providerCallsThisRead":0,"eventsCreated":0,"sourceWritten":false,"complete":false});
    encode(&v, o.max_output_bytes, d)?;
    Ok(v)
}
pub(super) fn retained_roots(store: &Store, d: &Deadline) -> Result<RetainedRoots> {
    let o = Options::default();
    store.set_deadline(d)?;
    let _read = storage::read_transaction(store)?;
    let mut admission = Context::new(store, &o, d)?;
    let ids = admission
        .source
        .ids(JOB_KIND, MAX_JOBS)
        .map_err(|_| refuse(CAPACITY))?;
    let mut roots = RetainedRoots::default();
    for id in ids {
        d.check()?;
        let mut c = Context::new(store, &o, d)?;
        let (j, _) = read_job(&mut c, &id)?;
        checked(&mut c, &j)?;
        roots.blob_ids.insert(j.request.source.content_hash.clone());
        roots.blob_ids.extend(c.artifacts.keys().cloned());
    }
    Ok(roots)
}
