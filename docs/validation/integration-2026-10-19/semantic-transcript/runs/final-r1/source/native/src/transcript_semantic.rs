//! Explicit model-assisted candidates over imported transcript bytes. This
//! source space never becomes a native snapshot, code path or event authority.
use super::*;
use crate::{
    ledger::{AttemptState, AttemptStatus, Usage, validate_cost},
    providers::ProviderSpec,
};
use std::{fmt, path::PathBuf};

#[path = "transcript_semantic_build.rs"]
mod execution;
#[path = "transcript_semantic_store.rs"]
mod saved;
pub const JOB_KIND: &str = "native_transcript_semantic_job_v1";
pub const OPERATION: &str = "extract_transcript_feedback_v1";
const MAX_JOB: usize = 32_000;
const MAX_PAYLOAD: usize = 8_000_000;
const MAX_WIRE: usize = 8_016_384;
const MAX_REPLY: usize = 2_000_000;
const MAX_FRAGMENT: usize = 1_000_000;
const REQUIRED_RESULT: usize = 4_000_000;
const MAX_RESULT: usize = 16_000_000;
const MAX_JOBS: usize = 1000;
const INPUT: &str = "transcript_semantic_input_invalid";
const CAPACITY: &str = "transcript_semantic_capacity_exceeded";
const BINDING: &str = "transcript_semantic_binding_invalid";
const PERMISSION: &str = "transcript_semantic_provider_permission_required";
#[derive(Debug)]
struct Failure(&'static str);
impl fmt::Display for Failure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.0)
    }
}
impl std::error::Error for Failure {}
fn refuse(code: &'static str) -> anyhow::Error {
    Failure(code).into()
}
fn require(ok: bool, code: &'static str) -> Result<()> {
    if ok { Ok(()) } else { Err(refuse(code)) }
}
fn sanitized(error: anyhow::Error, d: &Deadline) -> anyhow::Error {
    if error.downcast_ref::<execution::BuildFailure>().is_some() {
        return error;
    }
    if let Err(e) = d.check() {
        return e;
    }
    for cause in error.chain() {
        if let Some(e) = cause.downcast_ref::<Failure>() {
            return refuse(e.0);
        }
    }
    refuse(BINDING)
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SourceKind {
    TranscriptUtf8,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Selection {
    pub kind: SourceKind,
    pub transcript_id: String,
    pub content_hash: String,
    pub start_byte: usize,
    pub end_byte: usize,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Request {
    pub schema_version: u8,
    pub source: Selection,
}
pub fn validate_request(r: &Request, d: &Deadline) -> Result<()> {
    d.check()?;
    require(
        r.schema_version == 1
            && valid_id(&r.source.transcript_id)
            && valid_id(&r.source.content_hash)
            && r.source.start_byte < r.source.end_byte
            && r.source.end_byte <= MAX_TRANSCRIPT_BYTES,
        INPUT,
    )
}
/// Host-only construction; neither the transcript nor provider JSON grants trust.
pub struct Policy {
    pub contract_hash: String,
    pub provider_input_files: Vec<PathBuf>,
    pub max_result_bytes: usize,
}
impl Policy {
    pub fn new(contract_hash: String) -> Self {
        Self {
            contract_hash,
            provider_input_files: vec![],
            max_result_bytes: REQUIRED_RESULT,
        }
    }
}
pub fn validate_host_configuration(p: &ProviderSpec, policy: &Policy, d: &Deadline) -> Result<()> {
    require(p.trusted, PERMISSION)?;
    d.check()?;
    p.validate().map_err(|_| refuse(INPUT))?;
    require(
        valid_id(&policy.contract_hash)
            && policy.provider_input_files.len() <= 64
            && policy.provider_input_files.iter().all(|p| p.is_absolute()),
        INPUT,
    )?;
    require(
        (REQUIRED_RESULT..=MAX_RESULT).contains(&policy.max_result_bytes),
        CAPACITY,
    )
}
fn options(o: &Options, d: &Deadline) -> Result<()> {
    validate_options(o).map_err(|_| refuse(INPUT))?;
    d.check()?;
    require(o.max_candidates > 0, CAPACITY)
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Candidate {
    pub outcome: Outcome,
    pub interpretation: String,
    pub source_evidence: Vec<TranscriptQuote>,
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Coverage {
    Candidates,
    NoCandidates,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Fragment {
    pub candidates: Vec<Candidate>,
    pub coverage: Coverage,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ProviderRecord {
    model: String,
    model_version: String,
    model_key: String,
    fingerprint: String,
    task_id: String,
    currency: String,
    reservation: u64,
    task_limit: u64,
}
#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum State {
    Reserved,
    Sent,
    Responded,
    Complete,
    Failed,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Job {
    schema_version: u8,
    id: String,
    nonce: String,
    workspace_id: String,
    request: Request,
    transcript_header_hash: String,
    payload_id: String,
    contract_hash: String,
    provider: ProviderRecord,
    candidate_limit: usize,
    max_result_bytes: usize,
    attempt_id: String,
    request_id: String,
    state: State,
    response_id: Option<String>,
    result_id: Option<String>,
    failure: Option<String>,
}
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Reply {
    schema: u8,
    operation: String,
    model: String,
    model_version: String,
    request_id: String,
    input_hash: String,
    currency: String,
    source_binding: Selection,
    actual_cost_micros: Option<u64>,
    usage: Option<Value>,
    data: Option<Value>,
    error: Option<Value>,
}
fn job_id(j: &Job) -> String {
    identity(&(
        JOB_KIND,
        &j.nonce,
        &j.workspace_id,
        &j.request,
        &j.transcript_header_hash,
        &j.payload_id,
        &j.contract_hash,
        &j.provider,
        j.candidate_limit,
        j.max_result_bytes,
    ))
}
fn request_id(j: &Job) -> String {
    identity(&(OPERATION, &j.id, &j.attempt_id, &j.payload_id))
}
fn encode<T: Serialize>(v: &T, max: usize, d: &Deadline) -> Result<Vec<u8>> {
    d.check()?;
    let bytes = crate::json::encode_bounded(v, max, d)
        .map_err(|_| d.check().err().unwrap_or_else(|| refuse(CAPACITY)))?;
    d.check()?;
    Ok(bytes)
}
fn decode<T: serde::de::DeserializeOwned>(b: &[u8], max: usize) -> Result<T> {
    crate::json::parse(b, max)
        .and_then(crate::json::decode)
        .map_err(|_| refuse(BINDING))
}
fn payload(c: &mut saved::Context<'_>, r: &Request, contract: &str, limit: usize) -> Result<Value> {
    validate_request(r, c.source.deadline)?;
    c.transcript(&r.source.transcript_id)?;
    let t = &c.source.transcripts[&r.source.transcript_id];
    require(t.header.source_hash == r.source.content_hash, BINDING)?;
    let selected = t
        .text
        .get(r.source.start_byte..r.source.end_byte)
        .ok_or_else(|| refuse(BINDING))?;
    Ok(
        json!({"kind":"transcript_semantic_input_v1","workspaceId":c.source.store.workspace_id,
        "source":r.source,"transcript":t.header,"text":selected,"contractHash":contract,"candidateLimit":limit,
        "omittedBytesWithinSelection":0,"unselectedTranscriptBytes":t.text.len()-selected.len()}),
    )
}
fn wire(j: &Job, payload: &Value, d: &Deadline) -> Result<Vec<u8>> {
    encode(
        &json!({"schema":1,"operation":OPERATION,"model":j.provider.model,"modelVersion":j.provider.model_version,
        "requestId":j.request_id,"inputHash":j.payload_id,"currency":j.provider.currency,"taskId":j.provider.task_id,
        "costLimitMicros":j.provider.reservation,"payload":payload}),
        MAX_WIRE,
        d,
    )
}
fn envelope(r: &Reply, j: &Job, d: &Deadline) -> Result<()> {
    d.check()?;
    require(
        r.schema == 1
            && r.operation == OPERATION
            && r.model == j.provider.model
            && r.model_version == j.provider.model_version
            && r.request_id == j.request_id
            && r.input_hash == j.payload_id
            && r.currency == j.provider.currency
            && r.source_binding == j.request.source,
        BINDING,
    )?;
    if let Some(cost) = r.actual_cost_micros {
        validate_cost(cost).map_err(|_| refuse(BINDING))?;
    }
    Ok(())
}
fn usage(r: &Reply) -> Result<Option<Usage>> {
    r.usage
        .as_ref()
        .map(|v| {
            let u: Usage = crate::json::decode(v.clone()).map_err(|_| refuse(BINDING))?;
            u.validate().map_err(|_| refuse(BINDING))?;
            Ok(u)
        })
        .transpose()
}
fn result(c: &saved::Context<'_>, j: &Job, r: &Reply, d: &Deadline) -> Result<Value> {
    require(
        r.error.is_none()
            && r.actual_cost_micros
                .is_none_or(|v| v <= j.provider.reservation),
        BINDING,
    )?;
    let reported_usage = usage(r)?;
    let data = r.data.as_ref().ok_or_else(|| refuse(BINDING))?;
    encode(data, MAX_FRAGMENT, d)?;
    let fragment: Fragment = crate::json::decode(data.clone()).map_err(|_| refuse(BINDING))?;
    require(
        fragment.candidates.len() <= j.candidate_limit
            && fragment.candidates.len() <= c.source.options.max_candidates,
        CAPACITY,
    )?;
    require(
        (fragment.candidates.is_empty()) == (fragment.coverage == Coverage::NoCandidates),
        BINDING,
    )?;
    let t = &c.source.transcripts[&j.request.source.transcript_id];
    let mut quoted = 0usize;
    let mut rows = Vec::new();
    for (ordinal, candidate) in fragment.candidates.iter().enumerate() {
        d.check()?;
        bounded_text(&candidate.interpretation, 2048, false).map_err(|_| refuse(BINDING))?;
        require((1..=8).contains(&candidate.source_evidence.len()), BINDING)?;
        for q in &candidate.source_evidence {
            d.check()?;
            require(
                q.transcript_id == j.request.source.transcript_id
                    && q.content_hash == j.request.source.content_hash
                    && j.request.source.start_byte <= q.start_byte
                    && q.start_byte < q.end_byte
                    && q.end_byte <= j.request.source.end_byte
                    && q.end_byte - q.start_byte == q.quote.len()
                    && t.text.get(q.start_byte..q.end_byte) == Some(q.quote.as_str()),
                BINDING,
            )?;
            bounded_text(&q.quote, 16_000, false).map_err(|_| refuse(BINDING))?;
            quoted = quoted
                .checked_add(q.quote.len())
                .ok_or_else(|| refuse(CAPACITY))?;
            require(quoted <= 128_000, CAPACITY)?;
        }
        rows.push(json!({"id":identity(&("transcript_semantic_candidate_v1",&j.id,&j.response_id,ordinal,candidate)),
            "originalOrdinal":ordinal,"candidate":candidate,"sourceKind":"transcriptUtf8","requiresHostSelection":true,
            "resolution":"inferred_outcome_candidate","providerAttestation":"unverified"}));
    }
    let v = json!({"schemaVersion":1,"kind":"native_transcript_semantic_candidates_v1","jobId":j.id,
        "workspaceId":j.workspace_id,"source":j.request.source,"transcript":t.header,
        "sourceValidation":"exact_registered_imported_utf8_bytes","payloadArtifactId":j.payload_id,"responseArtifactId":j.response_id,
        "requestId":j.request_id,"attemptId":j.attempt_id,"providerFingerprint":j.provider.fingerprint,
        "model":j.provider.model,"modelVersion":j.provider.model_version,"currency":j.provider.currency,
        "providerReportedCostMicros":r.actual_cost_micros,"providerReportedUsage":reported_usage,
        "accounting":"original_bound_response_provider_reported_unverified_or_unknown","originalFragment":fragment,"candidates":rows,
        "modelCallsOriginalBuild":1,"modelCallsScope":"one_original_bound_host_attempt_not_provider_internal_calls",
        "modelAssistedInterpretationPerformed":true,"semanticUnderstandingEstablished":false,"outcomeTruthEstablished":false,
        "independentCorroborationEstablished":false,"executionObserved":false,"sourceAuthority":false,"complete":false,
        "eventsCreated":0,"sourceWritten":false,"sourceWriteScope":"native_parent_only","requiresHostSelection":true,
        "providerAttestation":"unverified","providerFilesystemIsolation":"not_enforced_for_explicitly_trusted_provider",
        "unselectedTranscriptBytes":t.text.len()-(j.request.source.end_byte-j.request.source.start_byte),
        "limitation":"Transcript byte quotations are checked; model classifications and explanations remain inferences. No native code/source identity, event promotion or execution authority is granted."});
    encode(&v, j.max_result_bytes, d)?;
    Ok(v)
}
/// Immutable source/response-checked value. JSON cannot construct this capability.
pub struct CheckedCandidates {
    value: Value,
}
impl CheckedCandidates {
    pub fn export(&self, max: usize, d: &Deadline) -> Result<Value> {
        require((1..=MAX_RESULT).contains(&max), CAPACITY)?;
        encode(&self.value, max, d)?;
        let v = self.value.clone();
        d.check()?;
        Ok(v)
    }
}
pub fn build(
    store: &Store,
    r: &Request,
    p: &ProviderSpec,
    policy: &Policy,
    o: &Options,
    d: &Deadline,
) -> Result<Value> {
    execution::build(store, r, p, policy, o, d).map_err(|e| sanitized(e, d))
}
pub fn load(store: &Store, id: &str, o: &Options, d: &Deadline) -> Result<CheckedCandidates> {
    saved::load(store, id, o, d).map_err(|e| sanitized(e, d))
}
pub fn status(store: &Store, id: &str, o: &Options, d: &Deadline) -> Result<Value> {
    saved::status(store, id, o, d).map_err(|e| sanitized(e, d))
}
pub fn jobs(
    store: &Store,
    task: &str,
    offset: usize,
    limit: usize,
    o: &Options,
    d: &Deadline,
) -> Result<Value> {
    saved::jobs(store, task, offset, limit, o, d).map_err(|e| sanitized(e, d))
}
pub(super) fn retained_roots(store: &Store, d: &Deadline) -> Result<RetainedRoots> {
    saved::retained_roots(store, d).map_err(|e| sanitized(e, d))
}
