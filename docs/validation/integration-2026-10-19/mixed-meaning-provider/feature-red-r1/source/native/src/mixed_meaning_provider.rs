//! Explicit trusted-host mixed interpretation execution. Never a query default.
use super::*;
use crate::{
    ledger::{AttemptState, Ledger, Usage, validate_cost},
    meaning_graph::Fragment,
    model::{hash, identity},
    providers::ProviderSpec,
};
use anyhow::ensure;
use rusqlite::{
    Connection, OptionalExtension, Transaction, TransactionBehavior, limits::Limit, params,
};
use serde::de::DeserializeOwned;
use std::path::PathBuf;

pub const JOB_KIND: &str = "native_mixed_meaning_job_v1";
pub const OPERATION: &str = "extract_mixed_meaning_v1";
const MAX_JOB: usize = 256_000;
const MAX_WIRE: usize = 1_016_384;
const MAX_REPLY: usize = 2_000_000;
const MAX_GRAPH: usize = 64_000_000;
// Existing prepared export4MB + original fragment1MB + checked quote items4MB
// + bounded quote descriptors/receipt. No response-dependent admission is added.
const REQUIRED_GRAPH_CAPACITY: usize = 10_000_000;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
pub enum SourceSelection {
    Office { selection: Selection },
    Pdf { selection: SelectionV2 },
}
impl SourceSelection {
    fn source(&self) -> &SourcePin {
        match self {
            Self::Office { selection } => &selection.source,
            Self::Pdf { selection } => &selection.source,
        }
    }
    fn load(
        &self,
        store: &Store,
        worker: Option<&NativePdfWorker>,
        d: &Deadline,
    ) -> Result<CheckedMixedView> {
        match self {
            Self::Office { selection } => {
                input(worker.is_none())?;
                super::load(store, selection, &Options::default(), d)
            }
            Self::Pdf { selection } => super::load_with_pdf(
                store,
                selection,
                worker.ok_or_else(|| failure("mixed_view_pdf_permission_required"))?,
                &Options::default(),
                d,
            ),
        }
    }
}
/// Host authority is supplied separately from repository/provider JSON.
#[derive(Clone, Debug)]
pub struct Policy {
    pub contract_hash: String,
    pub provider_input_files: Vec<PathBuf>,
    pub max_graph_bytes: usize,
}
impl Policy {
    pub fn new(contract_hash: String) -> Self {
        Self {
            contract_hash,
            provider_input_files: Vec::new(),
            max_graph_bytes: 16_000_000,
        }
    }
}
pub fn validate_host_configuration(provider: &ProviderSpec, policy: &Policy) -> Result<()> {
    provider.validate()?;
    input(
        valid_id(&policy.contract_hash)
            && policy.provider_input_files.len() <= 64
            && policy.provider_input_files.iter().all(|p| p.is_absolute())
            && (REQUIRED_GRAPH_CAPACITY..=MAX_GRAPH).contains(&policy.max_graph_bytes),
    )
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SourceBinding {
    source_id: String,
    source: SourceRef,
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
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Job {
    schema_version: u8,
    job_id: String,
    nonce: String,
    workspace_id: String,
    selection: SourceSelection,
    request: MeaningInputRequest,
    prepared_input_hash: String,
    contract_hash: String,
    provider: ProviderRecord,
    max_graph_bytes: usize,
    input_artifact_id: String,
    payload_artifact_id: String,
    payload_hash: String,
    bindings: Vec<SourceBinding>,
    parent_snapshots: Vec<String>,
    parent_artifacts: Vec<String>,
    parent_records: Vec<(String, String)>,
    attempt_id: String,
    request_id: String,
    state: String,
    response_artifact_id: Option<String>,
    graph_artifact_id: Option<String>,
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
    source_bindings: Vec<SourceBinding>,
    currency: String,
    actual_cost_micros: Option<u64>,
    usage: Option<Usage>,
    data: Option<Value>,
    error: Option<String>,
}
struct LengthLimit<'a> {
    db: &'a Connection,
    original: i32,
}
impl Drop for LengthLimit<'_> {
    fn drop(&mut self) {
        let _ = self.db.set_limit(Limit::SQLITE_LIMIT_LENGTH, self.original);
    }
}
fn read_raw(db: &Connection, id: &str) -> Result<Option<String>> {
    input(valid_id(id))?;
    let bytes: Option<usize> = db
        .query_row(
            "SELECT length(CAST(value AS BLOB)) FROM records WHERE kind=?1 AND id=?2",
            params![JOB_KIND, id],
            |r| r.get(0),
        )
        .optional()?;
    let Some(bytes) = bytes else {
        return Ok(None);
    };
    capacity(bytes <= MAX_JOB)?;
    let original = db.set_limit(
        Limit::SQLITE_LIMIT_LENGTH,
        db.limit(Limit::SQLITE_LIMIT_LENGTH)?
            .min((MAX_JOB + 4096) as i32),
    )?;
    let _limit = LengthLimit { db, original };
    let raw: String = db.query_row(
        "SELECT value FROM records WHERE kind=?1 AND id=?2 AND length(CAST(value AS BLOB))=?3",
        params![JOB_KIND, id, bytes],
        |r| r.get(0),
    )?;
    input(raw.len() == bytes)?;
    Ok(Some(raw))
}
fn decode<T: DeserializeOwned>(bytes: &[u8], max: usize) -> Result<T> {
    crate::json::decode(crate::json::parse(bytes, max)?)
}
fn job_identity(job: &Job) -> String {
    identity(&(
        JOB_KIND,
        &job.workspace_id,
        &job.selection,
        &job.request,
        &job.prepared_input_hash,
        &job.contract_hash,
        &job.provider,
        &job.max_graph_bytes,
        &job.input_artifact_id,
        &job.payload_artifact_id,
        &job.payload_hash,
        &job.bindings,
        &job.parent_snapshots,
        &job.parent_artifacts,
        &job.parent_records,
        &job.nonce,
    ))
}
fn request_identity(job: &Job) -> String {
    identity(&(
        OPERATION,
        &job.job_id,
        &job.attempt_id,
        &job.payload_hash,
        &job.provider,
    ))
}
fn update_job(store: &Store, previous: &str, job: &Job, d: &Deadline) -> Result<String> {
    check(d)?;
    let raw = String::from_utf8(encode(job, MAX_JOB, d)?)?;
    ensure!(
        store.db.execute(
            "UPDATE records SET value=?3 WHERE kind=?1 AND id=?2 AND value=?4",
            params![JOB_KIND, job.job_id, raw, previous]
        )? == 1,
        "mixed_meaning_job_stale"
    );
    Ok(raw)
}
fn bindings(inputs: &CheckedMeaningInputs<'_>) -> Vec<SourceBinding> {
    inputs
        .sources()
        .map(|(id, source)| SourceBinding {
            source_id: id.into(),
            source: source.clone(),
        })
        .collect()
}
fn same<T: Serialize>(a: &T, b: &T, d: &Deadline) -> Result<bool> {
    Ok(encode(a, MAX_JOB, d)? == encode(b, MAX_JOB, d)?)
}
fn parent_roots(
    store: &Store,
    view: &CheckedMixedView,
    d: &Deadline,
) -> Result<crate::meaning_build::MeaningDependencies> {
    let side = view.source.before();
    let mut roots =
        crate::graph_view::meaning_dependencies(store, side.view(), d)?.unwrap_or_default();
    roots
        .snapshots
        .extend(side.source_scopes().map(|s| s.id.clone()));
    roots
        .artifacts
        .push(view.selection.source.view_artifact_id.clone());
    roots.records.push((
        crate::graph_view::RECORD_KIND.into(),
        view.selection.source.view_id.clone(),
    ));
    for graph in &view.documents {
        check(d)?;
        let header = crate::document_graph::mixed_input::header(graph);
        roots.records.push((
            crate::document_graph::RECORD_KIND.into(),
            header.document_graph_id.into(),
        ));
        roots
            .records
            .push(("native_media_asset_v1".into(), header.asset_id.into()));
    }
    for pdf in &view.pdfs {
        check(d)?;
        roots
            .records
            .push(("native_media_asset_v1".into(), pdf.asset.id.clone()));
        roots.artifacts.push(pdf.asset.blob_id.clone());
    }
    roots.snapshots.sort();
    roots.snapshots.dedup();
    roots.artifacts.sort();
    roots.artifacts.dedup();
    roots.records.sort();
    roots.records.dedup();
    Ok(roots)
}
fn wire(job: &Job, payload: &Value, d: &Deadline) -> Result<Vec<u8>> {
    encode(
        &serde_json::json!({"schema":1,"operation":OPERATION,"model":job.provider.model,
        "modelVersion":job.provider.model_version,"requestId":job.request_id,"inputHash":job.payload_hash,
        "currency":job.provider.currency,"taskId":job.provider.task_id,"costLimitMicros":job.provider.reservation,
        "payload":payload}),
        MAX_WIRE,
        d,
    )
}
fn validate_job(store: &Store, job: &Job, d: &Deadline) -> Result<()> {
    check(d)?;
    input(
        job.schema_version == 1
            && job.workspace_id == store.workspace_id
            && job.selection.source().workspace_id == store.workspace_id
            && valid_id(&job.nonce)
            && job.job_id == job_identity(job)
            && job.request_id == request_identity(job)
            && [
                job.input_artifact_id.as_str(),
                job.payload_artifact_id.as_str(),
                job.payload_hash.as_str(),
                job.prepared_input_hash.as_str(),
                job.contract_hash.as_str(),
                job.provider.fingerprint.as_str(),
                job.attempt_id.as_str(),
            ]
            .iter()
            .all(|id| valid_id(id))
            && job.parent_snapshots.len() <= 2048
            && job.parent_artifacts.len() <= 8192
            && job.parent_records.len() <= 8192
            && (REQUIRED_GRAPH_CAPACITY..=MAX_GRAPH).contains(&job.max_graph_bytes)
            && matches!(
                job.state.as_str(),
                "reserved" | "sent" | "failed" | "complete"
            ),
    )?;
    let options = Options::default();
    match &job.selection {
        SourceSelection::Office { selection } => {
            validate_selection(selection, &options, d)?;
            validate_meaning_inputs(&job.request, selection, d)?;
        }
        SourceSelection::Pdf { selection } => {
            validate_pdf_selection(selection, &options, d)?;
            validate_pdf_meaning_inputs(&job.request, selection, d)?;
        }
    }
    let attempt = Ledger::new(store, d).attempt_status(&job.attempt_id)?;
    input(
        attempt.task_id == job.provider.task_id
            && attempt.provider == job.provider.model_key
            && attempt.currency == job.provider.currency
            && attempt.reserved_micros == job.provider.reservation,
    )?;
    input(
        !job.provider.model.contains('@')
            && !job.provider.model_version.contains('@')
            && job.provider.model_key
                == format!("{}@{}", job.provider.model, job.provider.model_version)
            && job.provider.model_key.len() <= 200,
    )?;
    validate_cost(job.provider.reservation)?;
    validate_cost(job.provider.task_limit)?;
    let totals = Ledger::new(store, d).total(&job.provider.task_id)?;
    input(
        totals.task_limit_micros == Some(job.provider.task_limit)
            && totals.currency.as_deref() == Some(job.provider.currency.as_str()),
    )?;
    input(
        job.state != "complete"
            || (job.response_artifact_id.is_some()
                && job.graph_artifact_id.is_some()
                && job.failure.is_none()),
    )?;
    input(job.state == "complete" || job.graph_artifact_id.is_none())?;
    // A crash can occur after the single-use ledger send transition but before
    // the job's sent transition. Preserve that uncertainty instead of retrying.
    encode(job, MAX_JOB, d)?;
    Ok(())
}
fn load_job(store: &Store, id: &str, d: &Deadline) -> Result<Job> {
    check(d)?;
    store.set_deadline(d)?;
    let raw = read_raw(&store.db, id)?.ok_or_else(|| failure("mixed_meaning_job_missing"))?;
    let job: Job =
        decode(raw.as_bytes(), MAX_JOB).map_err(|_| failure("mixed_meaning_record_invalid"))?;
    input(job.job_id == id)?;
    validate_job(store, &job, d)?;
    Ok(job)
}
fn reply_for_job(store: &Store, job: &Job, d: &Deadline) -> Result<Option<Reply>> {
    let Some(id) = &job.response_artifact_id else {
        return Ok(None);
    };
    check(d)?;
    let raw = store.read_artifact_limited(id, MAX_REPLY)?;
    let reply: Reply =
        decode(&raw, MAX_REPLY).map_err(|_| failure("mixed_meaning_record_invalid"))?;
    validate_envelope(&reply, job, d)?;
    let attempt = Ledger::new(store, d).attempt_status(&job.attempt_id)?;
    if let Some(cost) = reply.actual_cost_micros {
        input(attempt.actual_cost_micros == Some(cost) && attempt.state == AttemptState::Settled)?;
    }
    if let Some(usage) = &reply.usage {
        if usage.validate().is_ok() {
            input(attempt.response_usage.as_ref() == Some(usage))?;
        }
    }
    Ok(Some(reply))
}
fn validate_envelope(reply: &Reply, job: &Job, d: &Deadline) -> Result<()> {
    input(
        reply.schema == 1
            && reply.operation == OPERATION
            && reply.model == job.provider.model
            && reply.model_version == job.provider.model_version
            && reply.request_id == job.request_id
            && reply.input_hash == job.payload_hash
            && reply.currency == job.provider.currency
            && same(&reply.source_bindings, &job.bindings, d)?,
    )?;
    if let Some(cost) = reply.actual_cost_micros {
        validate_cost(cost)?;
    }
    check(d)
}
fn graph_value(
    inputs: &CheckedMeaningInputs<'_>,
    job: &Job,
    reply: &Reply,
    d: &Deadline,
) -> Result<Value> {
    input(reply.error.is_none())?;
    if let Some(usage) = &reply.usage {
        usage.validate()?;
    }
    let fragment: Fragment = crate::json::decode(
        reply
            .data
            .clone()
            .ok_or_else(|| failure("mixed_meaning_fragment_invalid"))?,
    )?;
    let request = MixedMeaningClaimRequest {
        schema_version: 1,
        input_hash: job.prepared_input_hash.clone(),
        fragment,
    };
    let checked = inputs.check_meaning_claims(&request, d)?;
    let validation = checked.provider_validation_export(job.max_graph_bytes, d)?;
    graph_envelope(validation, job, reply, d)
}
fn graph_envelope(validation: Value, job: &Job, reply: &Reply, d: &Deadline) -> Result<Value> {
    let graph_id = identity(&(
        "mixed_meaning_provider_graph_v1",
        &job.job_id,
        &job.response_artifact_id,
    ));
    let value = serde_json::json!({"kind":"mixed_meaning_provider_graph_v1","id":graph_id,"jobId":job.job_id,
        "sourceValidation":validation,"responseArtifactId":job.response_artifact_id,
        "requestId":job.request_id,"inputHash":job.payload_hash,"preparedInputHash":job.prepared_input_hash,
        "providerFingerprint":job.provider.fingerprint,"model":job.provider.model,"modelVersion":job.provider.model_version,
        "attemptId":job.attempt_id,"currency":job.provider.currency,"providerReportedCostMicros":reply.actual_cost_micros,
        "providerReportedUsage":reply.usage,"accounting":"original_bound_response_provider_reported_unverified_or_unknown",
        "modelCallsThisRun":1,"providerInternalCalls":"unverified","providerAttestation":"unverified",
        "resolution":"inferred_concept","canonicalSourceAuthority":false,"complete":false,"sourceWritten":false,
        "networkIsolation":"not_enforced_for_explicitly_trusted_provider"});
    encode(&value, job.max_graph_bytes, d)?;
    Ok(value)
}
fn insert<T: Serialize>(
    tx: &Transaction<'_>,
    kind: &str,
    id: &str,
    value: &T,
    d: &Deadline,
) -> Result<()> {
    let raw = encode(value, MAX_JOB, d)?;
    tx.execute(
        "INSERT INTO records(kind,id,value) VALUES(?1,?2,?3)",
        params![kind, id, std::str::from_utf8(&raw)?],
    )?;
    Ok(())
}
fn register_blob(
    tx: &Transaction<'_>,
    store: &Store,
    bytes: &[u8],
    d: &Deadline,
) -> Result<String> {
    check(d)?;
    let id = store.blob(bytes)?;
    let value = serde_json::json!({"artifactId":id,"totalBytes":bytes.len()});
    let size: Option<usize> = tx
        .query_row(
            "SELECT length(CAST(value AS BLOB)) FROM records WHERE kind='artifact' AND id=?1",
            [&id],
            |r| r.get(0),
        )
        .optional()?;
    if let Some(size) = size {
        capacity(size <= 64_000)?;
        let original = store.db.set_limit(
            Limit::SQLITE_LIMIT_LENGTH,
            store.db.limit(Limit::SQLITE_LIMIT_LENGTH)?.min(65_536),
        )?;
        let _limit = LengthLimit {
            db: &store.db,
            original,
        };
        let raw:String = tx.query_row("SELECT value FROM records WHERE kind='artifact' AND id=?1 AND length(CAST(value AS BLOB))=?2",
            params![id,size],|r|r.get(0))?;
        input(raw.len() == size && crate::json::parse(raw.as_bytes(), 64_000)? == value)?;
    } else {
        insert(tx, "artifact", &id, &value, d)?;
    }
    Ok(id)
}
fn reference(job: &Job) -> Value {
    serde_json::json!({"kind":"mixed_meaning_build_reference_v1","jobId":job.job_id,
        "state":job.state,"graphArtifactId":job.graph_artifact_id,"preparedInputHash":job.prepared_input_hash,
        "resultTransport":"durable_mixed_meaning_records","readWith":{"command":"mixed-meaning status","jobId":job.job_id},
        "resolution":"inferred_concept","providerAttestation":"unverified","complete":false,"sourceWritten":false})
}
fn failure_with_job(code: &'static str, job: &Job) -> anyhow::Error {
    anyhow::anyhow!(
        "{code}; job={}; inspect durable status; no automatic retry",
        job.job_id
    )
}
/// One fixed request over a complete checked selection, with no cache or retry.
/// ProviderSpec and the optional PDF worker are explicit host capabilities.
pub fn build(
    store: &Store,
    selection: &SourceSelection,
    request: &MeaningInputRequest,
    pdf_worker: Option<&NativePdfWorker>,
    provider: &ProviderSpec,
    policy: &Policy,
    d: &Deadline,
) -> Result<Value> {
    validate_host_configuration(provider, policy)?;
    check(d)?;
    input(store.db.is_autocommit() && selection.source().workspace_id == store.workspace_id)?;
    store.with_exclusive_client(d, || {
        // All source reads, resource admission and fingerprint reads precede the
        // reservation transaction. No transaction spans a provider process.
        let view = selection.load(store, pdf_worker, d)?;
        let inputs = view.prepare_meaning_inputs(request, d)?;
        let input_bytes = encode(&inputs.export(MAX_OUTPUT_BYTES, d)?, MAX_OUTPUT_BYTES, d)?;
        let payload = inputs.provider_payload(&policy.contract_hash, d)?;
        let payload_bytes = encode(&payload, 1_000_000, d)?;
        let roots = parent_roots(store, &view, d)?;
        let (fingerprint, executable) = crate::meaning_build::fingerprint_host_provider(
            store,
            provider,
            &policy.provider_input_files,
            d,
        )?;
        let mut nonce = [0u8; 32];
        getrandom::fill(&mut nonce)?;
        let mut job = Job {
            schema_version: 1,
            job_id: String::new(),
            nonce: hex::encode(nonce),
            workspace_id: store.workspace_id.clone(),
            selection: selection.clone(),
            request: request.clone(),
            prepared_input_hash: inputs.input_hash().into(),
            contract_hash: policy.contract_hash.clone(),
            provider: ProviderRecord {
                model: provider.model_id.clone(),
                model_version: provider.model_version.clone(),
                model_key: provider.model_key()?,
                fingerprint,
                task_id: provider.task_id.clone(),
                currency: provider.currency.clone(),
                reservation: provider.max_cost_micros,
                task_limit: provider.task_limit_micros,
            },
            max_graph_bytes: policy.max_graph_bytes,
            input_artifact_id: hash(&input_bytes),
            payload_artifact_id: hash(&payload_bytes),
            payload_hash: hash(&payload_bytes),
            bindings: bindings(&inputs),
            parent_snapshots: roots.snapshots,
            parent_artifacts: roots.artifacts,
            parent_records: roots.records,
            attempt_id: "0".repeat(64),
            request_id: "0".repeat(64),
            state: "reserved".into(),
            response_artifact_id: None,
            graph_artifact_id: None,
            failure: None,
        };
        job.job_id = job_identity(&job);
        encode(&job, MAX_JOB, d)?;
        wire(&job, &payload, d)?;
        // The CLI can preflight this fixed-size durable reference before build.
        encode(&reference(&job), 16_384, d)?;
        let ledger = Ledger::new(store, d);
        let mut tokens = ledger.reserve_many_registered(
            &provider.task_id,
            &[provider.max_cost_micros],
            provider.task_limit_micros,
            &provider.currency,
            &provider.model_key()?,
            |tx, tokens| {
                job.attempt_id = tokens[0].id().into();
                job.request_id = request_identity(&job);
                input(register_blob(tx, store, &input_bytes, d)? == job.input_artifact_id)?;
                input(register_blob(tx, store, &payload_bytes, d)? == job.payload_artifact_id)?;
                insert(tx, JOB_KIND, &job.job_id, &job, d)
            },
        )?;
        let token = tokens
            .pop()
            .ok_or_else(|| failure("mixed_meaning_reservation_invalid"))?;
        let mut raw = String::from_utf8(encode(&job, MAX_JOB, d)?)?;
        let result = (|| -> Result<Value> {
            let stdin = wire(&job, &payload, d)?;
            let temp = tempfile::Builder::new()
                .prefix("yoke-mixed-meaning-")
                .tempdir()?;
            let mut command = std::process::Command::new(&executable);
            command.args(&provider.args).current_dir(temp.path());
            input(ledger.send_reserved(token)? == job.attempt_id)?;
            job.state = "sent".into();
            raw = update_job(store, &raw, &job, d)?;
            let output = crate::process::run_capture(
                command,
                &stdin,
                d,
                crate::process::CaptureLimits {
                    stdin_bytes: MAX_WIRE,
                    stdout_bytes: MAX_REPLY,
                    stderr_bytes: 256_000,
                },
            )?;
            input(
                output.stdin_complete
                    && output.output_complete
                    && !output.timed_out
                    && !output.output_exceeded
                    && !output.io_error,
            )?;
            let reply: Reply = decode(&output.stdout, MAX_REPLY)
                .map_err(|_| failure("mixed_meaning_reply_invalid"))?;
            validate_envelope(&reply, &job, d)?;
            let usage_valid = reply.usage.as_ref().is_none_or(|u| u.validate().is_ok());
            if usage_valid {
                if let Some(usage) = &reply.usage {
                    ledger.usage(&job.attempt_id, "response", usage.clone())?;
                }
            }
            // Identity/currency/cost are bound first. A rejected interpretation,
            // unsafe provider error or nonzero exit must not discard its bill.
            ledger.settle(&job.attempt_id, reply.actual_cost_micros)?;
            let mut next = job.clone();
            next.response_artifact_id = Some(hash(&output.stdout));
            let outcome = if output.exit_code != Some(0) {
                Err(failure("mixed_meaning_process_failure"))
            } else if reply.error.is_some() {
                Err(failure("mixed_meaning_provider_failure"))
            } else if !usage_valid {
                Err(failure("mixed_meaning_usage_invalid"))
            } else if ledger.total(&provider.task_id)?.reservation_exceeded {
                Err(failure("mixed_meaning_cost_overrun"))
            } else {
                graph_value(&inputs, &next, &reply, d)
            };
            let graph_bytes = match outcome {
                Ok(value) => Some(encode(&value, job.max_graph_bytes, d)?),
                Err(_) => {
                    next.failure = Some("mixed_meaning_bound_response_rejected".into());
                    None
                }
            };
            next.state = if graph_bytes.is_some() {
                "complete"
            } else {
                "failed"
            }
            .into();
            let tx = Transaction::new_unchecked(&store.db, TransactionBehavior::Immediate)?;
            input(read_raw(&tx, &job.job_id)?.as_deref() == Some(raw.as_str()))?;
            input(
                register_blob(&tx, store, &output.stdout, d)?
                    == *next.response_artifact_id.as_ref().unwrap(),
            )?;
            if let Some(bytes) = &graph_bytes {
                next.graph_artifact_id = Some(register_blob(&tx, store, bytes, d)?);
            }
            let next_raw = String::from_utf8(encode(&next, MAX_JOB, d)?)?;
            input(
                tx.execute(
                    "UPDATE records SET value=?3 WHERE kind=?1 AND id=?2 AND value=?4",
                    params![JOB_KIND, job.job_id, next_raw, raw],
                )? == 1,
            )?;
            check(d)?;
            tx.commit()?;
            raw = next_raw;
            job = next;
            if job.state != "complete" {
                return Err(failure("mixed_meaning_bound_response_rejected"));
            }
            check(d)?;
            Ok(reference(&job))
        })();
        match result {
            Ok(value) => Ok(value),
            Err(_) if job.state == "complete" => Err(failure_with_job(
                "mixed_meaning_publication_committed",
                &job,
            )),
            Err(_) => {
                if let Ok(attempt) = ledger.attempt_status(&job.attempt_id) {
                    if attempt.state == AttemptState::Reserved {
                        let _ = ledger.cancel_before_send(&job.attempt_id);
                    } else if attempt.state == AttemptState::Sent {
                        let _ = ledger.settle(&job.attempt_id, None);
                    }
                }
                if job.state != "failed" {
                    job.state = "failed".into();
                    job.failure = Some("mixed_meaning_incomplete_or_publication_failed".into());
                    let _ = update_job(store, &raw, &job, d);
                }
                Err(failure_with_job("mixed_meaning_job_incomplete", &job))
            }
        }
    })
}
fn artifacts(store: &Store, job: &Job, d: &Deadline) -> Result<(Value, Value)> {
    check(d)?;
    let input_bytes = store.read_artifact_limited(&job.input_artifact_id, MAX_OUTPUT_BYTES)?;
    let payload_bytes = store.read_artifact_limited(&job.payload_artifact_id, 1_000_000)?;
    input(hash(&payload_bytes) == job.payload_hash)?;
    let prepared = crate::json::parse(&input_bytes, MAX_OUTPUT_BYTES)?;
    let payload = crate::json::parse(&payload_bytes, 1_000_000)?;
    input(
        prepared["kind"] == "mixed_meaning_inputs_v1"
            && prepared["inputHash"] == job.prepared_input_hash
            && payload["kind"] == "mixed_meaning_provider_input_v1"
            && payload["inputHash"] == job.prepared_input_hash
            && payload["contractHash"] == job.contract_hash,
    )?;
    let units = payload["units"]
        .as_array()
        .ok_or_else(|| failure("mixed_meaning_record_invalid"))?;
    let original = prepared["units"]
        .as_array()
        .ok_or_else(|| failure("mixed_meaning_record_invalid"))?;
    input(
        units.len() == job.bindings.len()
            && original.len() == units.len()
            && units.len() == job.request.units.len(),
    )?;
    for ((unit, original), binding) in units.iter().zip(original).zip(&job.bindings) {
        check(d)?;
        input(
            unit["sourceId"] == binding.source_id
                && original["id"] == binding.source_id
                && unit["source"] == serde_json::to_value(&binding.source)?
                && original["item"]["source"] == unit["source"]
                && unit["text"] == original["item"]["text"]
                && unit["omittedBytes"] == 0,
        )?;
        let text = unit["text"]
            .as_str()
            .ok_or_else(|| failure("mixed_meaning_record_invalid"))?;
        input(!text.is_empty() && text.len() <= 16_384 && original["textBytes"] == text.len())?;
    }
    input(prepared["contextGroups"] == payload["contextGroups"])?;
    Ok((prepared, payload))
}
/// Read diagnostic progress and original bound-response accounting. This never
/// resolves a provider, runs a PDF worker, retries, or upgrades unknown receipts.
pub fn status(store: &Store, id: &str, d: &Deadline) -> Result<Value> {
    let job = load_job(store, id, d).map_err(|_| failure("mixed_meaning_record_invalid"))?;
    artifacts(store, &job, d)?;
    let reply = reply_for_job(store, &job, d)?;
    let attempt = Ledger::new(store, d).attempt_status(&job.attempt_id)?;
    let dispatched = !matches!(
        attempt.state,
        AttemptState::Reserved | AttemptState::NotSent
    );
    check(d)?;
    Ok(
        serde_json::json!({"kind":"mixed_meaning_job_status_v1","jobId":job.job_id,"state":job.state,"failure":job.failure,
        "preparedInputHash":job.prepared_input_hash,"inputHash":job.payload_hash,"attemptId":job.attempt_id,
        "attemptState":attempt.state,"dispatchedHostAttempts":usize::from(dispatched),
        "modelCallsThisRun":reply.as_ref().map(|_|1),"providerInternalCalls":"unverified",
        "originalResponseCostMicros":reply.as_ref().and_then(|r|r.actual_cost_micros),
        "originalResponseUsage":reply.as_ref().and_then(|r|r.usage.as_ref()).filter(|u|u.validate().is_ok()),
        "currentLedgerCostMicros":attempt.actual_cost_micros,"currentLedgerUsage":attempt.reported_usage,
        "currency":job.provider.currency,"providerFingerprint":job.provider.fingerprint,
        "model":job.provider.model,"modelVersion":job.provider.model_version,
        "responseArtifactId":job.response_artifact_id,"graphArtifactId":job.graph_artifact_id,
        "complete":false,"providerAttestation":"unverified","sourceWritten":false,"providerCallsThisRead":0}),
    )
}
/// Recheck original source and response without a provider. A PDF worker, when
/// required, is supplied separately by the host and never recovered from JSON.
pub fn load(
    store: &Store,
    id: &str,
    pdf_worker: Option<&NativePdfWorker>,
    d: &Deadline,
) -> Result<Value> {
    let job = load_job(store, id, d)?;
    input(job.state == "complete")?;
    let (saved, payload) = artifacts(store, &job, d)?;
    let reply =
        reply_for_job(store, &job, d)?.ok_or_else(|| failure("mixed_meaning_record_invalid"))?;
    let view = job.selection.load(store, pdf_worker, d)?;
    let inputs = view.prepare_meaning_inputs(&job.request, d)?;
    input(
        inputs.input_hash() == job.prepared_input_hash
            && inputs.export(MAX_OUTPUT_BYTES, d)? == saved
            && inputs.provider_payload(&job.contract_hash, d)? == payload
            && same(&bindings(&inputs), &job.bindings, d)?,
    )?;
    let roots = parent_roots(store, &view, d)?;
    input(
        roots.snapshots == job.parent_snapshots
            && roots.artifacts == job.parent_artifacts
            && roots.records == job.parent_records,
    )?;
    let expected = graph_value(&inputs, &job, &reply, d)?;
    let id = job
        .graph_artifact_id
        .as_ref()
        .ok_or_else(|| failure("mixed_meaning_record_invalid"))?;
    let actual = store.read_artifact_limited(id, job.max_graph_bytes)?;
    input(actual == encode(&expected, job.max_graph_bytes, d)?)?;
    check(d)?;
    Ok(expected)
}
/// Closed metadata/ledger/artifact dependency validation for retention. This
/// does not construct a source capability or interpret saved PDF page data.
pub fn validate_retained_record(
    store: &Store,
    kind: &str,
    id: &str,
    value: &Value,
    d: &Deadline,
) -> Result<crate::meaning_build::MeaningDependencies> {
    input(kind == JOB_KIND)?;
    let job = load_job(store, id, d)?;
    input(serde_json::to_value(&job)? == *value)?;
    artifacts(store, &job, d)?;
    let reply = reply_for_job(store, &job, d)?;
    let mut roots = crate::meaning_build::MeaningDependencies {
        snapshots: job.parent_snapshots.clone(),
        artifacts: job.parent_artifacts.clone(),
        records: job.parent_records.clone(),
    };
    for snapshot in &roots.snapshots {
        check(d)?;
        let snapshot = store.snapshot(snapshot)?;
        input(snapshot.workspace_id == store.workspace_id)?;
    }
    input(
        roots
            .snapshots
            .contains(&job.selection.source().snapshot_id),
    )?;
    input(
        roots
            .artifacts
            .contains(&job.selection.source().view_artifact_id),
    )?;
    input(roots.records.contains(&(
        crate::graph_view::RECORD_KIND.into(),
        job.selection.source().view_id.clone(),
    )))?;
    roots.artifacts.extend([
        job.input_artifact_id.clone(),
        job.payload_artifact_id.clone(),
    ]);
    roots
        .records
        .push(("native_model_attempt_v1".into(), job.attempt_id.clone()));
    if let Some(id) = &job.response_artifact_id {
        roots.artifacts.push(id.clone());
    }
    if let Some(id) = &job.graph_artifact_id {
        let bytes = store.read_artifact_limited(id, job.max_graph_bytes)?;
        let graph = crate::json::parse(&bytes, job.max_graph_bytes)?;
        input(
            graph["kind"] == "mixed_meaning_provider_graph_v1"
                && graph["jobId"] == job.job_id
                && graph["responseArtifactId"] == serde_json::to_value(&job.response_artifact_id)?
                && graph["inputHash"] == job.payload_hash
                && graph["preparedInputHash"] == job.prepared_input_hash
                && graph["attemptId"] == job.attempt_id
                && graph["providerFingerprint"] == job.provider.fingerprint
                && graph["providerAttestation"] == "unverified"
                && graph["canonicalSourceAuthority"] == false,
        )?;
        let reply = reply
            .as_ref()
            .ok_or_else(|| failure("mixed_meaning_record_invalid"))?;
        input(
            reply.error.is_none()
                && graph["sourceValidation"]["originalFragment"]
                    == reply.data.clone().unwrap_or(Value::Null),
        )?;
        roots.artifacts.push(id.clone());
    }
    check(d)?;
    Ok(roots)
}
