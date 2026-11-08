//! Explicit model-adjudicated identity policy over checked native Meaning claims.
//! Equivalence is an unverified model inference; original observations are immutable.
use crate::{
    capture::Deadline,
    meaning_join::{self, ConceptRef},
    model::{hash, identity, valid_id},
    store::Store,
};
use crate::{
    ledger::{AttemptState, AttemptStatus, Usage, validate_cost},
    providers::ProviderSpec,
};
use anyhow::Result;
use rusqlite::{OptionalExtension, Transaction, TransactionBehavior, params};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
    path::PathBuf,
};

#[path = "meaning_identity_build.rs"]
mod execution;
#[path = "meaning_identity_lexical.rs"]
mod lexical;
#[path = "meaning_identity_projection.rs"]
mod projection;
#[path = "meaning_identity_store.rs"]
mod saved;
#[path = "meaning_identity_input.rs"]
mod source;

pub const OPERATION: &str = "adjudicate_meaning_identity_v1";
pub const JOB_KIND: &str = "native_meaning_identity_job_v1";
const MAX_SOURCE: usize = 4_000_000;
const MAX_PAYLOAD: usize = 8_000_000;
const MAX_REPLY: usize = 2_000_000;
const MAX_RESULT: usize = 64_000_000;
const REQUIRED_RESULT: usize = 16_000_000;
const MAX_JOB: usize = 256_000;
const MAX_JOBS: usize = 1000;
const INPUT: &str = "meaning_identity_input_invalid";
const CAPACITY: &str = "meaning_identity_capacity_exceeded";
const BINDING: &str = "meaning_identity_binding_invalid";
#[derive(Debug)]
struct Failure(&'static str);
impl fmt::Display for Failure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.0)
    }
}
impl std::error::Error for Failure {}
fn fail(code: &'static str) -> anyhow::Error {
    Failure(code).into()
}
fn require(ok: bool, code: &'static str) -> Result<()> {
    if ok { Ok(()) } else { Err(fail(code)) }
}
fn sanitized(e: anyhow::Error, d: &Deadline) -> anyhow::Error {
    if e.downcast_ref::<execution::BuildFailure>().is_some() {
        return e;
    }
    if let Err(e) = d.check() {
        return e;
    }
    for c in e.chain() {
        if let Some(e) = c.downcast_ref::<Failure>() {
            return fail(e.0);
        }
    }
    fail(BINDING)
}
fn encode<T: Serialize>(v: &T, max: usize, d: &Deadline) -> Result<Vec<u8>> {
    d.check()?;
    let b = crate::json::encode_bounded(v, max, d)
        .map_err(|_| d.check().err().unwrap_or_else(|| fail(CAPACITY)))?;
    d.check()?;
    Ok(b)
}
fn decode<T: serde::de::DeserializeOwned>(b: &[u8], max: usize) -> Result<T> {
    crate::json::parse(b, max)
        .and_then(crate::json::decode)
        .map_err(|_| fail(BINDING))
}
fn rows<'a>(v: &'a Value, k: &str) -> Result<&'a [Value]> {
    v[k].as_array()
        .map(Vec::as_slice)
        .ok_or_else(|| fail(BINDING))
}
fn text<'a>(v: &'a Value, k: &str) -> Result<&'a str> {
    v[k].as_str().ok_or_else(|| fail(BINDING))
}
fn label(v: &str, max: usize) -> Result<()> {
    require(
        !v.trim().is_empty()
            && v.len() <= max
            && !v.chars().any(char::is_control)
            && !crate::paths::obvious_secret(v),
        BINDING,
    )
}
#[derive(Clone, Debug)]
pub struct Options {
    pub max_concepts: usize,
    pub max_pairs: usize,
    pub max_candidates: usize,
    pub max_similarity_work: u64,
    pub max_input_bytes: usize,
    pub max_output_bytes: usize,
}
impl Default for Options {
    fn default() -> Self {
        Self {
            max_concepts: 128,
            max_pairs: 8128,
            max_candidates: 256,
            max_similarity_work: 64_000_000,
            max_input_bytes: 64_000_000,
            max_output_bytes: 16_000_000,
        }
    }
}
pub fn validate_options(o: &Options, d: &Deadline) -> Result<()> {
    d.check()?;
    require(
        o.max_concepts <= 128
            && o.max_pairs <= 8128
            && o.max_candidates <= 256
            && o.max_similarity_work <= 128_000_000
            && o.max_input_bytes <= 256_000_000
            && o.max_output_bytes <= MAX_RESULT,
        INPUT,
    )
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Selection {
    pub workspace_id: String,
    pub snapshot_id: String,
    pub base_graph_hash: String,
    pub graph_ids: Vec<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Pair {
    pub left: ConceptRef,
    pub right: ConceptRef,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CandidatePolicy {
    pub min_jaro_winkler_bps: u16,
    pub min_minhash_bps: u16,
}
impl Default for CandidatePolicy {
    fn default() -> Self {
        Self {
            min_jaro_winkler_bps: 9000,
            min_minhash_bps: 5000,
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Request {
    pub schema_version: u8,
    pub selection: Selection,
    pub candidate_policy: CandidatePolicy,
    #[serde(default)]
    pub additional_pairs: Vec<Pair>,
}
pub fn validate_request(r: &Request, d: &Deadline) -> Result<()> {
    d.check()?;
    require(
        r.schema_version == 1
            && valid_id(&r.selection.workspace_id)
            && valid_id(&r.selection.snapshot_id)
            && valid_id(&r.selection.base_graph_hash)
            && (1..=4).contains(&r.selection.graph_ids.len())
            && r.selection.graph_ids.iter().all(|id| valid_id(id))
            && r.selection.graph_ids.windows(2).all(|w| w[0] < w[1])
            && (7000..=10000).contains(&r.candidate_policy.min_jaro_winkler_bps)
            && r.candidate_policy.min_minhash_bps <= 10000
            && r.additional_pairs.len() <= 256,
        INPUT,
    )?;
    let mut pairs = BTreeSet::new();
    for p in &r.additional_pairs {
        d.check()?;
        require(
            reference_valid(&p.left)
                && reference_valid(&p.right)
                && p.left < p.right
                && r.selection.graph_ids.contains(&p.left.graph_id)
                && r.selection.graph_ids.contains(&p.right.graph_id)
                && pairs.insert((&p.left, &p.right)),
            INPUT,
        )?;
    }
    Ok(())
}
fn reference_valid(r: &ConceptRef) -> bool {
    valid_id(&r.graph_id) && valid_id(&r.concept_id) && r.packet_index < 64
}
/// Host-only execution policy, never constructed from analyzed text.
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
    require(p.trusted, "meaning_identity_provider_permission_required")?;
    d.check()?;
    p.validate().map_err(|_| fail(INPUT))?;
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
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SourceBinding {
    pub selection: Selection,
    pub source_projection_id: String,
    pub source_hash: String,
    pub candidate_set_id: String,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Candidate {
    id: String,
    left: ConceptRef,
    right: ConceptRef,
    lexical: Option<lexical::Match>,
    explicit_host_candidate: bool,
}
struct Prepared {
    request: Request,
    binding: SourceBinding,
    source: meaning_join::JoinedGraph,
    candidates: Vec<Candidate>,
    admitted_bytes: usize,
    similarity_work: u64,
}
pub struct CheckedCandidates {
    value: Value,
}
impl CheckedCandidates {
    pub fn export(&self, max: usize, d: &Deadline) -> Result<Value> {
        require(max <= MAX_RESULT, CAPACITY)?;
        encode(&self.value, max, d)?;
        let v = self.value.clone();
        d.check()?;
        Ok(v)
    }
}
pub fn candidates(
    store: &Store,
    r: &Request,
    o: &Options,
    d: &Deadline,
) -> Result<CheckedCandidates> {
    (|| {let p=source::prepare(store,r,o,d)?;let value=json!({"schemaVersion":1,"kind":"meaning_identity_candidates_v1","sourceBinding":p.binding,
        "candidatePolicy":r.candidate_policy,"algorithm":"native_nfkc_context_lower_minhash64_lsh16x4_jaro_v1","source":p.source.as_value(),
        "candidates":p.candidates,"similarityWork":p.similarity_work,"allSelectedPairsConsidered":true,"providerCallsThisRead":0,
        "semanticIdentityEstablished":false,"mergesApplied":0,"selectionComplete":true,"complete":false});
        encode(&value,o.max_output_bytes,d)?;Ok(CheckedCandidates{value})})().map_err(|e|sanitized(e,d))
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    SameEntity,
    DifferentEntities,
    Uncertain,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Support {
    pub reference: ConceptRef,
    pub evidence_id: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Decision {
    pub candidate_id: String,
    pub outcome: Outcome,
    pub rationale: String,
    pub support: Vec<Support>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Fragment {
    pub decisions: Vec<Decision>,
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
    request: Request,
    binding: SourceBinding,
    payload_id: String,
    contract_hash: String,
    provider: ProviderRecord,
    max_result_bytes: usize,
    attempt_id: String,
    request_id: String,
    state: State,
    response_id: Option<String>,
    result_id: Option<String>,
    exit_code: Option<i32>,
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
    source_binding: SourceBinding,
    actual_cost_micros: Option<u64>,
    usage: Option<Value>,
    data: Option<Value>,
    error: Option<Value>,
}
fn job_id(j: &Job) -> String {
    identity(&(
        JOB_KIND,
        &j.nonce,
        &j.request,
        &j.binding,
        &j.payload_id,
        &j.contract_hash,
        &j.provider,
        j.max_result_bytes,
    ))
}
fn request_id(j: &Job) -> String {
    identity(&(OPERATION, &j.id, &j.attempt_id, &j.payload_id))
}
fn wire(j: &Job, payload: &Value, d: &Deadline) -> Result<Vec<u8>> {
    encode(
        &json!({"schema":1,"operation":OPERATION,"model":j.provider.model,"modelVersion":j.provider.model_version,
    "requestId":j.request_id,"inputHash":j.payload_id,"currency":j.provider.currency,"taskId":j.provider.task_id,"costLimitMicros":j.provider.reservation,"payload":payload}),
        MAX_PAYLOAD + 16_384,
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
            && r.source_binding == j.binding,
        BINDING,
    )?;
    if let Some(cost) = r.actual_cost_micros {
        validate_cost(cost).map_err(|_| fail(BINDING))?;
    }
    Ok(())
}
fn usage(r: &Reply) -> Result<Option<Usage>> {
    r.usage
        .as_ref()
        .map(|v| {
            let u: Usage = crate::json::decode(v.clone()).map_err(|_| fail(BINDING))?;
            u.validate().map_err(|_| fail(BINDING))?;
            Ok(u)
        })
        .transpose()
}
fn result(p: &Prepared, j: &Job, r: &Reply, d: &Deadline) -> Result<Value> {
    require(
        j.exit_code == Some(0)
            && r.error.is_none()
            && r.actual_cost_micros
                .is_none_or(|v| v <= j.provider.reservation),
        BINDING,
    )?;
    let reported_usage = usage(r)?;
    let data = r.data.as_ref().ok_or_else(|| fail(BINDING))?;
    encode(data, 1_000_000, d)?;
    let fragment: Fragment = crate::json::decode(data.clone()).map_err(|_| fail(BINDING))?;
    let mut v = projection::project(p, &fragment, d)?;
    v["jobId"] = json!(j.id);
    v["attemptId"] = json!(j.attempt_id);
    v["payloadArtifactId"] = json!(j.payload_id);
    v["responseArtifactId"] = json!(j.response_id);
    v["requestId"] = json!(j.request_id);
    v["model"] = json!(j.provider.model);
    v["modelVersion"] = json!(j.provider.model_version);
    v["providerFingerprint"] = json!(j.provider.fingerprint);
    v["currency"] = json!(j.provider.currency);
    v["providerReportedCostMicros"] = json!(r.actual_cost_micros);
    v["providerReportedUsage"] = json!(reported_usage);
    v["accounting"] = json!("original_bound_response_provider_reported_unverified_or_unknown");
    v["providerAttestation"] = json!("unverified");
    v["modelCallsOriginalBuild"] = json!(1);
    v["modelCallsScope"] = json!("one_original_bound_host_attempt_not_provider_internal_calls");
    v["sourceWritten"] = json!(false);
    v["sourceWriteScope"] = json!("native_parent_only");
    v["providerFilesystemIsolation"] = json!("not_enforced_for_explicitly_trusted_provider");
    encode(&v, j.max_result_bytes, d)?;
    Ok(v)
}
/// Source/response-replayed immutable quotient. Serialized JSON cannot construct it.
pub struct CheckedGraph {
    value: Value,
}
impl CheckedGraph {
    pub fn export(&self, max: usize, d: &Deadline) -> Result<Value> {
        require(max <= MAX_RESULT, CAPACITY)?;
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
pub fn load(store: &Store, id: &str, o: &Options, d: &Deadline) -> Result<CheckedGraph> {
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
pub fn validate_retained_record(
    store: &Store,
    id: &str,
    v: &Value,
    d: &Deadline,
) -> Result<crate::meaning_build::MeaningDependencies> {
    saved::retained(store, id, v, d).map_err(|e| sanitized(e, d))
}
