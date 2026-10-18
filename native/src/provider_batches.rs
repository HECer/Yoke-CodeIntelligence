//! Parent-managed source-bound embedding attempts. Legacy single requests are unchanged.
use crate::{
    capture::Deadline,
    json::encode_bounded,
    ledger::{AttemptState, AttemptStatus, Ledger, ReservedAttempt, Usage, validate_cost},
    model::{Graph, Snapshot, hash, identity, valid_id},
    model_bedrock::{self, BedrockSpec, Dialect},
    process::{CaptureLimits, run_capture},
    providers::{self, Binding, ProviderSpec},
    store::Store,
};
use anyhow::{Context, Result, ensure};
use rusqlite::{Transaction, params};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    process::Command,
};
const KIND: &str = "native_model_batch_v1";
const RESPONSE_KIND: &str = "native_model_batch_response_v1";
const MAX_RECORD: usize = 4_000_000;
const MAX_REQUEST: usize = 16_000_000;
const MAX_RESPONSE: usize = 32_000_000;
const MAX_VECTOR_RECORD: usize = 24 * 1024 * 1024;
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct PolicyRecord {
    dialect: String,
    dimensions: usize,
    chunk_items: usize,
    per_source_bytes: usize,
    max_calls: usize,
    max_entities: usize,
    max_total_source_bytes: usize,
    model: String,
}
impl PolicyRecord {
    fn validate(&self) -> Result<()> {
        ensure!(
            (1..=256).contains(&self.max_calls)
                && (1..=2000).contains(&self.max_entities)
                && (1..=10_000_000).contains(&self.max_total_source_bytes),
            "Invalid host embedding batch limits"
        );
        let (chunk_items, per_source_bytes) = match self.dialect.as_str() {
            "titanV1" => {
                ensure!(self.dimensions == 1536, "Invalid Titan G1 dimensions");
                (1, 5000)
            }
            "titanV2" => {
                ensure!(
                    [256, 512, 1024].contains(&self.dimensions),
                    "Invalid Titan V2 dimensions"
                );
                (1, 5000)
            }
            "cohereV3" => {
                ensure!(self.dimensions == 1024, "Invalid Cohere V3 dimensions");
                (96, 512)
            }
            "cohereV4" => {
                ensure!(
                    [256, 512, 1024, 1536].contains(&self.dimensions),
                    "Invalid Cohere V4 dimensions"
                );
                (96, 5000)
            }
            _ => anyhow::bail!("Unsupported embedding batch dialect"),
        };
        ensure!(
            self.chunk_items == chunk_items && self.per_source_bytes == per_source_bytes,
            "Embedding batch wire policy mismatch"
        );
        crate::model_protocol::bounded_text(&self.model, 200)?;
        Ok(())
    }
}
/// Explicit trusted host limits; never deserialized from source or model output.
pub struct EmbeddingBatchPolicy {
    record: PolicyRecord,
}
impl EmbeddingBatchPolicy {
    pub fn for_bedrock(
        spec: &BedrockSpec,
        max_calls: usize,
        max_entities: usize,
        max_total_source_bytes: usize,
    ) -> Result<Self> {
        model_bedrock::validate_spec(spec)?;
        let (dialect, dimensions, chunk_items, per_source_bytes) = match spec.dialect {
            Dialect::TitanV1 => ("titanV1", 1536, 1, 5000),
            Dialect::TitanV2 { dimensions, .. } => ("titanV2", dimensions, 1, 5000),
            Dialect::CohereV3 => ("cohereV3", 1024, 96, 512),
            Dialect::CohereV4 { dimensions } => ("cohereV4", dimensions, 96, 5000),
            _ => anyhow::bail!("Converse is not an embedding batch dialect"),
        };
        let record = PolicyRecord {
            dialect: dialect.into(),
            dimensions,
            chunk_items,
            per_source_bytes,
            max_calls,
            max_entities,
            max_total_source_bytes,
            model: format!("{}@{}", spec.model, spec.model_version),
        };
        record.validate()?;
        Ok(Self { record })
    }
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SelectedSource {
    anchor: Binding,
    omitted_bytes: usize,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Chunk {
    index: usize,
    input_hash: String,
    request_id: String,
    attempt_id: String,
    source_ids: Vec<String>,
    response_artifact_id: Option<String>,
}
#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum BatchState {
    Reserved,
    Running,
    Failed,
    Complete,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct BatchRecord {
    schema_version: u8,
    batch_id: String,
    batch_nonce: String,
    workspace_id: String,
    snapshot_id: String,
    graph_hash: String,
    model: String,
    model_id: String,
    model_version: String,
    task_id: String,
    currency: String,
    reservation_per_attempt: u64,
    task_limit_micros: u64,
    provider_fingerprint: String,
    policy: PolicyRecord,
    policy_hash: String,
    sources: Vec<SelectedSource>,
    chunks: Vec<Chunk>,
    state: BatchState,
    failure: Option<String>,
    vector_collection_id: Option<String>,
    vector_collection_hash: Option<String>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct VectorItem {
    source_id: String,
    vector: Vec<f64>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Vectors {
    items: Vec<VectorItem>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Reply {
    schema: u8,
    operation: String,
    model: String,
    model_version: String,
    request_id: String,
    input_hash: String,
    source_bindings: Vec<Binding>,
    currency: String,
    actual_cost_micros: Option<u64>,
    usage: Option<Usage>,
    data: Option<Value>,
    error: Option<String>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ResponseRecord {
    schema_version: u8,
    kind: String,
    batch_id: String,
    index: usize,
    attempt_id: String,
    request_id: String,
    input_hash: String,
    model: String,
    source_bindings: Vec<Binding>,
    currency: String,
    actual_cost_micros: Option<u64>,
    usage: Option<Usage>,
    outcome: String,
    vectors: Option<Vectors>,
}
struct PlannedChunk {
    payload: Value,
    input_hash: String,
    source_ids: Vec<String>,
    bindings: Vec<Binding>,
}
fn request_id(attempt: &str, record: &BatchRecord, input_hash: &str) -> String {
    identity(&(
        attempt,
        "embed",
        &record.model_id,
        &record.model_version,
        input_hash,
    ))
}
fn batch_id(record: &BatchRecord) -> String {
    identity(&(
        KIND,
        &record.workspace_id,
        &record.snapshot_id,
        &record.graph_hash,
        &record.model,
        &record.policy_hash,
        &record.batch_nonce,
        record
            .chunks
            .iter()
            .map(|c| &c.input_hash)
            .collect::<Vec<_>>(),
    ))
}
fn write_new(tx: &Transaction<'_>, record: &BatchRecord, deadline: &Deadline) -> Result<()> {
    let bytes = encode_bounded(record, MAX_RECORD, deadline)?;
    let raw = std::str::from_utf8(&bytes)?;
    tx.execute(
        "INSERT INTO records(kind,id,value) VALUES(?1,?2,?3)",
        params![KIND, record.batch_id, raw],
    )?;
    Ok(())
}
fn read_raw(db: &rusqlite::Connection, kind: &str, id: &str, maximum: usize) -> Result<String> {
    let limit = rusqlite::limits::Limit::SQLITE_LIMIT_LENGTH;
    let prior = db.set_limit(limit, db.limit(limit)?.min(i32::try_from(maximum)?))?;
    let result = db.query_row(
        "SELECT value FROM records WHERE kind=?1 AND id=?2",
        params![kind, id],
        |row| row.get(0),
    );
    db.set_limit(limit, prior)?;
    Ok(result?)
}
fn write_progress(store: &Store, record: &BatchRecord, deadline: &Deadline) -> Result<()> {
    let bytes = encode_bounded(record, MAX_RECORD, deadline)?;
    store.set_deadline(deadline)?;
    let changed = store.db.execute(
        "UPDATE records SET value=?3 WHERE kind=?1 AND id=?2",
        params![KIND, record.batch_id, std::str::from_utf8(&bytes)?],
    )?;
    ensure!(changed == 1, "Embedding batch disappeared");
    Ok(())
}
fn validate_vectors(value: &Value, expected: &[String], dimension: usize) -> Result<Vectors> {
    let vectors: Vectors =
        serde_json::from_value(value.clone()).context("Invalid native embedding schema")?;
    ensure!(
        vectors.items.len() == expected.len(),
        "Embedding chunk count mismatch"
    );
    let expected: BTreeSet<_> = expected.iter().map(String::as_str).collect();
    let mut seen = BTreeSet::new();
    for item in &vectors.items {
        ensure!(
            expected.contains(item.source_id.as_str())
                && seen.insert(item.source_id.as_str())
                && item.vector.len() == dimension,
            "Embedding chunk identity or dimension mismatch"
        );
        let mut norm = 0.0;
        for x in &item.vector {
            ensure!(x.is_finite(), "Nonfinite embedding coordinate");
            norm += x * x;
        }
        ensure!(norm > 0.0 && norm.is_finite(), "Invalid embedding norm");
    }
    Ok(vectors)
}
fn admitted_request(
    provider: &ProviderSpec,
    request_id: &str,
    payload: &Value,
    input_hash: &str,
    deadline: &Deadline,
) -> Result<Vec<u8>> {
    encode_bounded(
        &json!({"schema":1,"operation":"embed","model":provider.model_id,"modelVersion":provider.model_version,"requestId":request_id,"inputHash":input_hash,"currency":provider.currency,"taskId":provider.task_id,"costLimitMicros":provider.max_cost_micros,"payload":payload}),
        MAX_REQUEST,
        deadline,
    )
}
fn prepare_sources(
    store: &Store,
    snapshot: &Snapshot,
    graph: &Graph,
    policy: &PolicyRecord,
    provider: &ProviderSpec,
    deadline: &Deadline,
) -> Result<(Vec<SelectedSource>, Vec<PlannedChunk>)> {
    let ids = selected_ids(graph, policy.max_entities, deadline)?;
    ensure!(
        !ids.is_empty() && ids.len() <= policy.max_entities,
        "Complete embedding selection exceeds explicit entity budget"
    );
    ensure!(
        ids.len().div_ceil(policy.chunk_items) <= policy.max_calls,
        "Embedding batch exceeds explicit attempt budget"
    );
    let source = providers::sources_with_total_limit(
        store,
        snapshot,
        graph,
        &ids,
        (
            policy.max_entities,
            policy.per_source_bytes,
            policy.max_total_source_bytes,
        ),
        deadline,
    )?;
    let selected = source
        .iter()
        .map(|source| SelectedSource {
            anchor: source.anchor.clone(),
            omitted_bytes: source.omitted_bytes,
        })
        .collect();
    let mut source = source.into_iter();
    let mut chunks = Vec::new();
    loop {
        let items = source.by_ref().take(policy.chunk_items).collect::<Vec<_>>();
        if items.is_empty() {
            break;
        }
        let bindings = items.iter().map(|s| s.anchor.clone()).collect::<Vec<_>>();
        let source_ids = bindings
            .iter()
            .map(|a| a.source_id.clone())
            .collect::<Vec<_>>();
        let payload = json!({"snapshotId":snapshot.id,"sources":items});
        let input_hash = identity(&payload);
        // Real request IDs have this same exact 64-byte JSON representation.
        admitted_request(provider, &"0".repeat(64), &payload, &input_hash, deadline)?;
        chunks.push(PlannedChunk {
            payload,
            input_hash,
            source_ids,
            bindings,
        });
    }
    Ok((selected, chunks))
}
fn selected_ids(graph: &Graph, maximum: usize, deadline: &Deadline) -> Result<Vec<String>> {
    let mut ids = Vec::with_capacity(graph.entities.len().min(maximum));
    for entity in &graph.entities {
        deadline.check()?;
        if entity.kind != "file" {
            ensure!(
                ids.len() < maximum,
                "Complete embedding selection exceeds explicit entity budget"
            );
            ids.push(entity.id.clone());
        }
    }
    ids.sort();
    Ok(ids)
}
fn validate_bound_reply(
    reply: &Reply,
    record: &BatchRecord,
    chunk: &Chunk,
    expected: &PlannedChunk,
) -> Result<()> {
    ensure!(
        reply.schema == 1
            && reply.operation == "embed"
            && reply.model == record.model_id
            && reply.model_version == record.model_version
            && reply.request_id == chunk.request_id
            && reply.input_hash == chunk.input_hash
            && reply.source_bindings == expected.bindings
            && reply.currency == record.currency,
        "Embedding reply identity, currency, or source binding mismatch"
    );
    if let Some(cost) = reply.actual_cost_micros {
        validate_cost(cost)?;
    }
    if let Some(usage) = &reply.usage {
        usage.validate()?;
    }
    if let Some(error) = &reply.error {
        crate::model_protocol::bounded_text(error, 1000)?;
    }
    ensure!(
        reply.data.is_some() || reply.error.is_some(),
        "Embedding reply lacks data and error"
    );
    Ok(())
}
fn dispatch(
    store: &Store,
    record: &BatchRecord,
    chunk: &Chunk,
    planned: &PlannedChunk,
    provider: &ProviderSpec,
    token: ReservedAttempt,
    deadline: &Deadline,
) -> Result<ResponseRecord> {
    let directory = tempfile::Builder::new()
        .prefix("yoke-model-batch-")
        .tempdir()?;
    let input = admitted_request(
        provider,
        &chunk.request_id,
        &planned.payload,
        &chunk.input_hash,
        deadline,
    )?;
    let mut command = Command::new(&provider.command);
    command.args(&provider.args).current_dir(directory.path());
    let ledger = Ledger::new(store, deadline);
    let sent_id = ledger.send_reserved(token)?;
    ensure!(
        sent_id == chunk.attempt_id,
        "Reserved embedding attempt identity changed"
    );
    let captured = run_capture(
        command,
        &input,
        deadline,
        CaptureLimits {
            stdin_bytes: MAX_REQUEST,
            stdout_bytes: MAX_RESPONSE,
            stderr_bytes: 256 * 1024,
        },
    )?;
    ensure!(
        captured.stdin_complete
            && captured.output_complete
            && !captured.timed_out
            && !captured.output_exceeded
            && !captured.io_error,
        "Embedding worker output incomplete or deadline expired"
    );
    deadline.check()?;
    let value = crate::json::parse(&captured.stdout, MAX_RESPONSE)?;
    let reply: Reply = serde_json::from_value(value).context("Malformed embedding reply")?;
    validate_bound_reply(&reply, record, chunk, planned)?;
    // A fully bound response may report a bill even when its vectors or exit
    // status are unusable. Preserve that report before data validation.
    if let Some(usage) = &reply.usage {
        ledger.usage(&chunk.attempt_id, "response", usage.clone())?;
    }
    ledger.settle(&chunk.attempt_id, reply.actual_cost_micros)?;
    let validated = reply
        .data
        .as_ref()
        .map(|data| validate_vectors(data, &chunk.source_ids, record.policy.dimensions));
    let (outcome, vectors) = if captured.exit_code != Some(0) {
        ("process_failure", None)
    } else if reply.error.is_some() {
        ("provider_failure", None)
    } else {
        match validated {
            Some(Ok(vectors)) => ("complete", Some(vectors)),
            _ => ("invalid_vectors", None),
        }
    };
    Ok(ResponseRecord {
        schema_version: 1,
        kind: RESPONSE_KIND.into(),
        batch_id: record.batch_id.clone(),
        index: chunk.index,
        attempt_id: chunk.attempt_id.clone(),
        request_id: chunk.request_id.clone(),
        input_hash: chunk.input_hash.clone(),
        model: record.model.clone(),
        source_bindings: planned.bindings.clone(),
        currency: record.currency.clone(),
        actual_cost_micros: reply.actual_cost_micros,
        usage: reply.usage,
        outcome: outcome.into(),
        vectors,
    })
}
fn persist_response(
    store: &Store,
    response: &ResponseRecord,
    deadline: &Deadline,
) -> Result<String> {
    let bytes = encode_bounded(response, MAX_VECTOR_RECORD, deadline)?;
    deadline.check()?;
    store.artifact(&bytes)
}
fn cleanup_unsent(store: &Store, record: &BatchRecord, deadline: &Deadline) {
    let ledger = Ledger::new(store, deadline);
    for chunk in &record.chunks {
        let Ok(status) = ledger.attempt_status(&chunk.attempt_id) else {
            continue;
        };
        match status.state {
            AttemptState::Reserved => {
                let _ = ledger.cancel_before_send(&chunk.attempt_id);
            }
            AttemptState::Sent => {
                let _ = ledger.settle(&chunk.attempt_id, None);
            }
            _ => {}
        }
    }
}
/// A complete validated batch, sealed by the orchestrator before vector publication.
pub(crate) struct CheckedBatchCompletion {
    record: BatchRecord,
}
impl CheckedBatchCompletion {
    pub(crate) fn persist_in_tx(
        &self,
        tx: &Transaction<'_>,
        vector_id: &str,
        vector_hash: &str,
        dimension: usize,
        count: usize,
        deadline: &Deadline,
    ) -> Result<()> {
        let record = &self.record;
        ensure!(
            record.state == BatchState::Running
                && dimension == record.policy.dimensions
                && count == record.sources.len()
                && vector_id == identity(&(record.snapshot_id.as_str(), record.model.as_str()))
                && valid_id(vector_hash),
            "Embedding batch vector publication differs from admitted scope"
        );
        let raw = read_raw(tx, KIND, &record.batch_id, MAX_RECORD)?;
        let current: BatchRecord =
            serde_json::from_value(crate::json::parse(raw.as_bytes(), MAX_RECORD)?)?;
        ensure!(
            identity(&current) == identity(record),
            "Embedding batch changed before vector publication"
        );
        let mut completed = record.clone();
        completed.state = BatchState::Complete;
        completed.vector_collection_id = Some(vector_id.into());
        completed.vector_collection_hash = Some(vector_hash.into());
        let bytes = encode_bounded(&completed, MAX_RECORD, deadline)?;
        deadline.check()?;
        let changed = tx.execute(
            "UPDATE records SET value=?3 WHERE kind=?1 AND id=?2 AND value=?4",
            params![KIND, record.batch_id, std::str::from_utf8(&bytes)?, raw],
        )?;
        ensure!(
            changed == 1,
            "Embedding batch completion precondition failed"
        );
        Ok(())
    }
}
/// Sequential explicitly reserved chunks. Every source, sink, shape and worst-case
/// budget is admitted before the first worker. No automatic retry or resume.
pub fn generate_embeddings_batched(
    store: &Store,
    snapshot: &Snapshot,
    graph: &Graph,
    provider: &ProviderSpec,
    policy: &EmbeddingBatchPolicy,
    deadline: &Deadline,
) -> Result<Value> {
    provider.validate()?;
    policy.record.validate()?;
    ensure!(
        provider.model_key()? == policy.record.model,
        "Embedding policy model differs from selected provider"
    );
    store.with_exclusive_client(deadline,||{
        let (sources,planned)=prepare_sources(store,snapshot,graph,&policy.record,provider,deadline)?;
        let ids=sources.iter().map(|s|s.anchor.source_id.clone()).collect::<Vec<_>>();
        let admission=crate::knowledge::PreparedVectorAdmission::new(store,snapshot,graph,&provider.model_key()?,&ids,policy.record.dimensions,MAX_VECTOR_RECORD,deadline)?;
        let total=provider.max_cost_micros.checked_mul(planned.len() as u64).context("Batch cost overflow")?;validate_cost(total)?;ensure!(total<=provider.task_limit_micros,"Full embedding batch exceeds task cost budget");
        let mut nonce=[0u8;32];getrandom::fill(&mut nonce)?;
        let mut record=BatchRecord{schema_version:1,batch_id:String::new(),batch_nonce:hex::encode(nonce),workspace_id:store.workspace_id.clone(),snapshot_id:snapshot.id.clone(),graph_hash:identity(graph),model:provider.model_key()?,model_id:provider.model_id.clone(),model_version:provider.model_version.clone(),task_id:provider.task_id.clone(),currency:provider.currency.clone(),reservation_per_attempt:provider.max_cost_micros,task_limit_micros:provider.task_limit_micros,provider_fingerprint:identity(&(&provider.command,&provider.args)),policy:policy.record.clone(),policy_hash:identity(&policy.record),sources,chunks:planned.iter().enumerate().map(|(index,p)|Chunk{index,input_hash:p.input_hash.clone(),request_id:"0".repeat(64),attempt_id:"0".repeat(64),source_ids:p.source_ids.clone(),response_artifact_id:None}).collect(),state:BatchState::Reserved,failure:None,vector_collection_id:None,vector_collection_hash:None};
        record.batch_id=batch_id(&record);encode_bounded(&record,MAX_RECORD,deadline)?;
        let ledger=Ledger::new(store,deadline);
        let tokens=ledger.reserve_many_registered(&provider.task_id,&vec![provider.max_cost_micros;planned.len()],provider.task_limit_micros,&provider.currency,&provider.model_key()?,|tx,tokens|{
            for (index,token) in tokens.iter().enumerate(){let id=token.id().to_owned();record.chunks[index].request_id=request_id(&id,&record,&record.chunks[index].input_hash);record.chunks[index].attempt_id=id;}
            write_new(tx,&record,deadline)
        })?;
        let result=(||->Result<Value>{
            record.state=BatchState::Running;write_progress(store,&record,deadline)?;
            let mut vectors=Vec::with_capacity(record.sources.len());
            for (index,(token,planned)) in tokens.into_iter().zip(&planned).enumerate(){
                deadline.check()?;let response=dispatch(store,&record,&record.chunks[index],planned,provider,token,deadline)?;
                let artifact=persist_response(store,&response,deadline)?;record.chunks[index].response_artifact_id=Some(artifact);write_progress(store,&record,deadline)?;
                ensure!(response.outcome=="complete","Embedding worker failed after accounting");
                for item in response.vectors.context("Validated embedding vectors missing")?.items {
                    let anchor=planned.bindings.iter().find(|a|a.source_id==item.source_id).context("Embedding output anchor missing")?;
                    vectors.push(json!({"entityId":item.source_id,"contentHash":anchor.content_hash,"vector":item.vector}));
                }
            }
            vectors.sort_by(|left,right|left["entityId"].as_str().cmp(&right["entityId"].as_str()));
            let prepared=admission.prepare(json!({"snapshotId":snapshot.id,"model":record.model,"items":vectors}),deadline)?;
            // Validate durable ledger/response/source links before sealing a completion.
            validate_record(store,&record,deadline)?;
            let completion=CheckedBatchCompletion{record:record.clone()};prepared.publish(&completion,deadline)
        })();
        match result {Ok(_)=>describe_embedding_batch(store,&record.batch_id,deadline),Err(error)=>{
            cleanup_unsent(store,&record,deadline);record.state=BatchState::Failed;record.failure=Some("batch_incomplete_or_publication_failed".into());let _=write_progress(store,&record,deadline);
            Err(error.context(format!("Embedding batch {} incomplete; inspect durable attempts and unreleased reservations",record.batch_id)))
        }}
    })
}
fn read_response(store: &Store, id: &str, deadline: &Deadline) -> Result<ResponseRecord> {
    ensure!(valid_id(id), "Invalid batch response artifact identity");
    deadline.check()?;
    let size = store.artifact_size(id)?;
    ensure!(
        size <= MAX_VECTOR_RECORD,
        "Embedding response artifact exceeds admission"
    );
    let bytes = store.read_artifact_limited(id, MAX_VECTOR_RECORD)?;
    ensure!(
        bytes.len() == size && hash(&bytes) == id,
        "Embedding response artifact hash mismatch"
    );
    let value = crate::json::parse(&bytes, MAX_VECTOR_RECORD)?;
    Ok(serde_json::from_value(value)?)
}
fn status_for(record: &BatchRecord, status: &AttemptStatus) -> Result<()> {
    ensure!(
        status.task_id == record.task_id
            && status.provider == record.model
            && status.currency == record.currency
            && status.reserved_micros == record.reservation_per_attempt,
        "Embedding batch accounting scope mismatch"
    );
    Ok(())
}
fn validate_record(
    store: &Store,
    record: &BatchRecord,
    deadline: &Deadline,
) -> Result<Vec<String>> {
    deadline.check()?;
    store.set_deadline(deadline)?;
    record.policy.validate()?;
    ensure!(
        record.schema_version == 1
            && record.workspace_id == store.workspace_id
            && valid_id(&record.batch_nonce)
            && record.batch_id == batch_id(record)
            && record.policy_hash == identity(&record.policy)
            && record.policy.model == record.model
            && record.model == format!("{}@{}", record.model_id, record.model_version)
            && valid_id(&record.provider_fingerprint),
        "Invalid embedding batch identity or policy"
    );
    crate::model_protocol::bounded_text(&record.model_id, 100)?;
    crate::model_protocol::bounded_text(&record.model_version, 100)?;
    ensure!(
        !record.model_id.contains('@') && !record.model_version.contains('@'),
        "Invalid embedding batch native namespace"
    );
    validate_cost(record.reservation_per_attempt)?;
    validate_cost(record.task_limit_micros)?;
    ensure!(
        !record.chunks.is_empty()
            && record.chunks.len() <= record.policy.max_calls
            && !record.sources.is_empty()
            && record.sources.len() <= record.policy.max_entities,
        "Embedding batch record exceeds selected limits"
    );
    let snapshot = store.snapshot(&record.snapshot_id)?;
    let graph = store.graph(&record.snapshot_id)?;
    ensure!(
        snapshot.workspace_id == record.workspace_id && identity(&graph) == record.graph_hash,
        "Embedding batch graph or snapshot mismatch"
    );
    let ids = record
        .sources
        .iter()
        .map(|s| s.anchor.source_id.clone())
        .collect::<Vec<_>>();
    let complete_ids = selected_ids(&graph, record.policy.max_entities, deadline)?;
    ensure!(
        ids == complete_ids,
        "Embedding batch omitted source entities from complete selection"
    );
    let sorted = ids.iter().collect::<BTreeSet<_>>();
    ensure!(
        sorted.len() == ids.len() && sorted.into_iter().cloned().collect::<Vec<_>>() == ids,
        "Embedding batch source order is not canonical"
    );
    let sources = providers::sources_with_total_limit(
        store,
        &snapshot,
        &graph,
        &ids,
        (
            record.policy.max_entities,
            record.policy.per_source_bytes,
            record.policy.max_total_source_bytes,
        ),
        deadline,
    )?;
    let reconstructed = sources
        .iter()
        .map(|s| SelectedSource {
            anchor: s.anchor.clone(),
            omitted_bytes: s.omitted_bytes,
        })
        .collect::<Vec<_>>();
    ensure!(
        identity(&reconstructed) == identity(&record.sources),
        "Embedding batch source replay mismatch"
    );
    ensure!(
        record.chunks.len() == ids.len().div_ceil(record.policy.chunk_items),
        "Embedding batch chunk count mismatch"
    );
    let ledger = Ledger::new(store, deadline);
    let totals = ledger.total(&record.task_id)?;
    ensure!(
        totals.currency.as_deref() == Some(record.currency.as_str())
            && totals.task_limit_micros == Some(record.task_limit_micros),
        "Embedding batch task policy mismatch"
    );
    let mut seen = BTreeSet::new();
    let mut refs = Vec::new();
    let mut all_vectors = Vec::new();
    for (index, (chunk, sources)) in record
        .chunks
        .iter()
        .zip(sources.chunks(record.policy.chunk_items))
        .enumerate()
    {
        deadline.check()?;
        let bindings = sources.iter().map(|s| s.anchor.clone()).collect::<Vec<_>>();
        let ids = sources
            .iter()
            .map(|s| s.anchor.source_id.clone())
            .collect::<Vec<_>>();
        ensure!(
            chunk.index == index
                && chunk.source_ids == ids
                && valid_id(&chunk.attempt_id)
                && seen.insert(&chunk.attempt_id)
                && chunk.input_hash
                    == identity(&json!({"snapshotId":record.snapshot_id,"sources":sources}))
                && chunk.request_id == request_id(&chunk.attempt_id, record, &chunk.input_hash),
            "Embedding batch chunk identity mismatch"
        );
        let status = ledger.attempt_status(&chunk.attempt_id)?;
        status_for(record, &status)?;
        if let Some(id) = &chunk.response_artifact_id {
            let response = read_response(store, id, deadline)?;
            ensure!(
                response.schema_version == 1
                    && response.kind == RESPONSE_KIND
                    && response.batch_id == record.batch_id
                    && response.index == index
                    && response.attempt_id == chunk.attempt_id
                    && response.request_id == chunk.request_id
                    && response.input_hash == chunk.input_hash
                    && response.model == record.model
                    && response.source_bindings == bindings
                    && response.currency == record.currency
                    && response
                        .actual_cost_micros
                        .is_none_or(|cost| status.actual_cost_micros == Some(cost))
                    && response
                        .usage
                        .as_ref()
                        .is_none_or(|usage| status.response_usage.as_ref() == Some(usage))
                    && matches!(status.state, AttemptState::Unknown | AttemptState::Settled),
                "Embedding response artifact or ledger binding mismatch"
            );
            ensure!(
                [
                    "complete",
                    "process_failure",
                    "provider_failure",
                    "invalid_vectors"
                ]
                .contains(&response.outcome.as_str()),
                "Unknown embedding response outcome"
            );
            if response.outcome == "complete" {
                let vectors = response
                    .vectors
                    .context("Complete embedding response missing vectors")?;
                validate_vectors(
                    &serde_json::to_value(&vectors)?,
                    &ids,
                    record.policy.dimensions,
                )?;
                all_vectors.extend(vectors.items);
            } else {
                ensure!(
                    response.vectors.is_none(),
                    "Failed embedding response carries accepted vectors"
                );
                ensure!(
                    record.state != BatchState::Complete,
                    "Complete batch contains failed response"
                );
            }
            refs.push(id.clone());
        } else {
            ensure!(
                record.state != BatchState::Complete,
                "Complete batch missing chunk response"
            );
        }
    }
    match record.state {
        BatchState::Complete => {
            ensure!(
                record.failure.is_none() && all_vectors.len() == record.sources.len(),
                "Incomplete published embedding batch"
            );
            let collection_id = identity(&(record.snapshot_id.as_str(), record.model.as_str()));
            ensure!(
                record.vector_collection_id.as_deref() == Some(&collection_id),
                "Embedding collection namespace mismatch"
            );
            let raw = read_raw(
                &store.db,
                "native_vectors_v1",
                &collection_id,
                MAX_VECTOR_RECORD,
            )?;
            ensure!(
                raw.len() <= MAX_VECTOR_RECORD
                    && record.vector_collection_hash.as_deref()
                        == Some(hash(raw.as_bytes()).as_str()),
                "Published embedding collection hash mismatch"
            );
            let mut by_id = BTreeMap::new();
            for item in all_vectors {
                let norm = item.vector.iter().map(|v| v * v).sum::<f64>().sqrt();
                by_id.insert(
                    item.source_id,
                    item.vector.iter().map(|v| v / norm).collect::<Vec<_>>(),
                );
            }
            let mut items = Vec::new();
            for source in &record.sources {
                items.push(json!({"entityId":source.anchor.source_id,"contentHash":source.anchor.content_hash,"vector":by_id.get(&source.anchor.source_id).context("Published embedding entity absent")?}));
            }
            let expected = json!({"schema":1,"snapshotId":record.snapshot_id,"workspaceId":record.workspace_id,"model":record.model,"dimension":record.policy.dimensions,"items":items});
            ensure!(
                crate::json::parse(raw.as_bytes(), MAX_VECTOR_RECORD)? == expected,
                "Published vector corpus differs from checked chunk results"
            );
        }
        _ => ensure!(
            record.vector_collection_id.is_none() && record.vector_collection_hash.is_none(),
            "Incomplete embedding batch claims published collection"
        ),
    }
    Ok(refs)
}
/// Strict retention replay, returning only validated response CAS dependencies.
pub(crate) fn validate_retained_batch(
    store: &Store,
    id: &str,
    value: &Value,
    deadline: &Deadline,
) -> Result<Vec<String>> {
    let bytes = encode_bounded(value, MAX_RECORD, deadline)?;
    let record: BatchRecord = serde_json::from_slice(&bytes)?;
    ensure!(
        record.batch_id == id,
        "Retained embedding batch key mismatch"
    );
    validate_record(store, &record, deadline)
}
/// Inspect a durable batch without executing, retrying, or refreshing any provider.
pub fn describe_embedding_batch(store: &Store, id: &str, deadline: &Deadline) -> Result<Value> {
    ensure!(valid_id(id), "Invalid embedding batch identity");
    store.set_deadline(deadline)?;
    let raw = read_raw(&store.db, KIND, id, MAX_RECORD)?;
    let record: BatchRecord =
        serde_json::from_value(crate::json::parse(raw.as_bytes(), MAX_RECORD)?)?;
    ensure!(record.batch_id == id, "Embedding batch key mismatch");
    validate_record(store, &record, deadline)?;
    let ledger = Ledger::new(store, deadline);
    let mut attempts = Vec::new();
    let mut known_cost = 0u64;
    let mut unknown_cost = 0usize;
    // At most 256 individually valid i64 counters fit comfortably in u128.
    // Numeric JSON aggregates may still exceed u64; retain exact decimal totals
    // alongside the original per-attempt reports instead of failing publication.
    let mut usage_totals = [0u128; 3];
    let mut unknown_usage = 0usize;
    let mut dispatched = 0usize;
    let mut uncertain = Vec::new();
    let mut unreleased = Vec::new();
    let mut confirmed = 0usize;
    for chunk in &record.chunks {
        let status = ledger.attempt_status(&chunk.attempt_id)?;
        if matches!(
            status.state,
            AttemptState::Sent | AttemptState::Unknown | AttemptState::Settled
        ) {
            dispatched += 1;
        }
        if status.state == AttemptState::Reserved {
            unreleased.push(status.attempt_id.clone());
        }
        if matches!(status.state, AttemptState::Sent | AttemptState::Unknown)
            && chunk.response_artifact_id.is_none()
        {
            uncertain.push(status.attempt_id.clone());
        }
        if chunk.response_artifact_id.is_some() {
            confirmed += 1;
        }
        if let Some(cost) = status.actual_cost_micros {
            known_cost = known_cost
                .checked_add(cost)
                .context("Batch cost total overflow")?;
        } else {
            unknown_cost += 1;
        }
        if let Some(usage) = &status.reported_usage {
            usage_totals[0] += u128::from(usage.input_tokens);
            usage_totals[1] += u128::from(usage.output_tokens);
            usage_totals[2] += u128::from(usage.cached_input_tokens);
        } else {
            unknown_usage += 1;
        }
        attempts.push(status);
    }
    let complete = record.state == BatchState::Complete;
    let known_usage = match usage_totals.map(u64::try_from) {
        [Ok(input_tokens), Ok(output_tokens), Ok(cached_input_tokens)] => Some(Usage {
            input_tokens,
            output_tokens,
            cached_input_tokens,
        }),
        _ => None,
    };
    let usage_overflow = known_usage.is_none();
    let usage_decimal = if usage_overflow {
        Some(
            json!({"inputTokens":usage_totals[0].to_string(),"outputTokens":usage_totals[1].to_string(),"cachedInputTokens":usage_totals[2].to_string()}),
        )
    } else {
        None
    };
    let calls = if complete {
        Some(record.chunks.len())
    } else {
        None
    };
    Ok(
        json!({"kind":"model_embedding_batch_generation","schemaVersion":1,"batchId":record.batch_id,"snapshotId":record.snapshot_id,"model":record.model,"status":if complete{"complete"}else if record.state==BatchState::Failed{"failed"}else{"incomplete"},"dimension":record.policy.dimensions,"count":if complete{record.sources.len()}else{0},"plannedCount":record.sources.len(),"attemptIds":record.chunks.iter().map(|c|&c.attempt_id).collect::<Vec<_>>(),"attempts":attempts,"modelCalls":calls,"dispatchedAttempts":dispatched,"confirmedResponseCount":confirmed,"uncertainAttemptIds":uncertain,"unreleasedUnsentAttemptIds":unreleased,"providerReportedCostMicros":if unknown_cost==0{Some(known_cost)}else{None},"knownReportedCostMicros":known_cost,"unknownCostAttempts":unknown_cost,"providerReportedUsage":if unknown_usage==0{known_usage.as_ref()}else{None},"knownReportedUsage":known_usage,"unknownUsageAttempts":unknown_usage,"reportedUsageOverflow":usage_overflow,"knownReportedUsageDecimal":usage_decimal,"currency":record.currency,"sourceBindings":record.sources.iter().map(|s|&s.anchor).collect::<Vec<_>>(),"sourceOmissions":record.sources.iter().filter(|s|s.omitted_bytes>0).map(|s|json!({"sourceId":s.anchor.source_id,"omittedBytes":s.omitted_bytes})).collect::<Vec<_>>(),"omittedEntities":0,"truncated":false,"accounting":"provider_reported_unverified_or_unknown","networkIsolation":"not_enforced_for_trusted_provider","sourceWritten":false,"localOnly":false,"vectorCollectionId":record.vector_collection_id,"vectorCollectionHash":record.vector_collection_hash}),
    )
}
