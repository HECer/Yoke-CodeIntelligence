//! Host-selected source-bound model meaning extraction.
use crate::{
    capture::Deadline,
    json::encode_bounded,
    ledger::{AttemptState, Ledger, ReservedAttempt, Usage, validate_cost},
    meaning_graph::{self, Binding, Fragment, MeaningInput, Source},
    model::{Graph, Snapshot, hash, identity, valid_id},
    providers::ProviderSpec,
    store::Store,
};
use anyhow::{Context, Result, ensure};
use rusqlite::{Connection, OptionalExtension, Transaction, TransactionBehavior, params};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    io::Read,
    path::{Path, PathBuf},
    process::Command,
};
/// Frozen source bytes supplied by a caller whose captured state may be borrowed.
pub type FrozenSourceReader<'a> =
    dyn FnMut(&Snapshot, &crate::model::SourceFile, &Deadline) -> Result<Vec<u8>> + 'a;
/// Complete immutable source union. Membership is sealed separately from any Store read grant.
#[derive(Debug)]
pub(crate) struct FrozenSourceIndex {
    scopes: BTreeMap<String, IndexedFrozenScope>,
    unique_files: BTreeMap<String, crate::model::SourceFile>,
    file_entries: usize,
    unique_bytes: usize,
}
#[derive(Debug)]
struct IndexedFrozenScope {
    snapshot: std::sync::Arc<Snapshot>,
    paths: BTreeMap<String, usize>,
}
pub(crate) struct BoundFrozenScope<'a> {
    scope: &'a IndexedFrozenScope,
}
#[derive(Clone, Debug)]
pub(crate) struct FrozenSourceMember {
    snapshot: std::sync::Arc<Snapshot>,
    position: usize,
}
pub(crate) type FrozenMemberReader<'a> =
    dyn FnMut(&FrozenSourceMember, &Deadline) -> Result<Vec<u8>> + 'a;

// Exhaustive field destructuring makes adding Snapshot metadata require an explicit equality decision.
fn same_frozen_snapshot(left: &Snapshot, right: &Snapshot) -> bool {
    let Snapshot {
        id,
        workspace_id,
        kind,
        files,
        omissions,
        capture_consistency,
    } = left;
    id == &right.id
        && workspace_id == &right.workspace_id
        && kind == &right.kind
        && files == &right.files
        && omissions == &right.omissions
        && capture_consistency == &right.capture_consistency
}
impl FrozenSourceIndex {
    pub(crate) fn new<'a>(
        scopes: impl IntoIterator<Item = &'a Snapshot>,
        max_source_bytes: usize,
        deadline: &Deadline,
    ) -> Result<Self> {
        ensure!(
            (1..=64_000_000).contains(&max_source_bytes),
            "Invalid frozen source capacity"
        );
        let mut index = Self {
            scopes: BTreeMap::new(),
            unique_files: BTreeMap::new(),
            file_entries: 0,
            unique_bytes: 0,
        };
        let mut workspace = None;
        for scope in scopes {
            deadline.check()?;
            ensure!(
                valid_id(&scope.id) && valid_id(&scope.workspace_id),
                "Invalid frozen source scope identity"
            );
            if let Some(expected) = &workspace {
                ensure!(
                    expected == &scope.workspace_id,
                    "Frozen source workspace mismatch"
                );
            } else {
                workspace = Some(scope.workspace_id.clone());
            }
            if let Some(prior) = index.scopes.get(&scope.id) {
                ensure!(
                    same_frozen_snapshot(&prior.snapshot, scope),
                    "Frozen source duplicate scope differs"
                );
                continue;
            }
            ensure!(
                index.scopes.len() < 4096,
                "Frozen source scope capacity exceeded"
            );
            let mut paths = BTreeMap::new();
            for (position, file) in scope.files.iter().enumerate() {
                deadline.check()?;
                index.file_entries = index
                    .file_entries
                    .checked_add(1)
                    .context("Frozen source entry overflow")?;
                ensure!(
                    index.file_entries <= 100_000
                        && file.size <= 4_000_000
                        && valid_id(&file.hash)
                        && crate::paths::normalized_syntax(&file.path)? == file.path
                        && paths.insert(file.path.clone(), position).is_none(),
                    "Invalid frozen source membership"
                );
                match index.unique_files.entry(file.hash.clone()) {
                    std::collections::btree_map::Entry::Occupied(prior) => {
                        ensure!(
                            prior.get().size == file.size,
                            "Frozen source hash size conflict"
                        );
                    }
                    std::collections::btree_map::Entry::Vacant(entry) => {
                        index.unique_bytes = index
                            .unique_bytes
                            .checked_add(file.size)
                            .context("Frozen source byte overflow")?;
                        ensure!(
                            index.unique_bytes <= max_source_bytes,
                            "Frozen source byte capacity exceeded"
                        );
                        entry.insert(file.clone());
                    }
                }
            }
            index.scopes.insert(
                scope.id.clone(),
                IndexedFrozenScope {
                    snapshot: std::sync::Arc::new(scope.clone()),
                    paths,
                },
            );
        }
        Ok(index)
    }
    pub(crate) fn bind<'a>(
        &'a self,
        snapshot: &Snapshot,
        deadline: &Deadline,
    ) -> Result<BoundFrozenScope<'a>> {
        deadline.check()?;
        let scope = self
            .scopes
            .get(&snapshot.id)
            .context("Frozen source scope absent")?;
        ensure!(
            same_frozen_snapshot(&scope.snapshot, snapshot),
            "Frozen source scope differs from complete admitted manifest"
        );
        Ok(BoundFrozenScope { scope })
    }
    /// Select only an already admitted immutable scope; no Store discovery.
    pub(crate) fn scope_named(
        &self,
        id: &str,
        deadline: &Deadline,
    ) -> Result<BoundFrozenScope<'_>> {
        deadline.check()?;
        let scope = self.scopes.get(id).context("Frozen source scope absent")?;
        Ok(BoundFrozenScope { scope })
    }
    pub(crate) fn scopes(&self) -> impl Iterator<Item = &Snapshot> {
        self.scopes.values().map(|s| s.snapshot.as_ref())
    }
    pub(crate) fn unique_files(&self) -> impl Iterator<Item = &crate::model::SourceFile> {
        self.unique_files.values()
    }
    pub(crate) fn scope_count(&self) -> usize {
        self.scopes.len()
    }
    pub(crate) fn file_entries(&self) -> usize {
        self.file_entries
    }
    pub(crate) fn unique_bytes(&self) -> usize {
        self.unique_bytes
    }
}
impl BoundFrozenScope<'_> {
    pub(crate) fn snapshot(&self) -> &Snapshot {
        self.scope.snapshot.as_ref()
    }
    pub(crate) fn member_named(
        &self,
        path: &str,
        deadline: &Deadline,
    ) -> Result<FrozenSourceMember> {
        deadline.check()?;
        let position = *self
            .scope
            .paths
            .get(path)
            .context("Frozen source member absent")?;
        Ok(FrozenSourceMember {
            snapshot: self.scope.snapshot.clone(),
            position,
        })
    }
    pub(crate) fn member(
        &self,
        file: &crate::model::SourceFile,
        deadline: &Deadline,
    ) -> Result<FrozenSourceMember> {
        deadline.check()?;
        let position = *self
            .scope
            .paths
            .get(&file.path)
            .context("Frozen source member absent")?;
        ensure!(
            &self.scope.snapshot.files[position] == file,
            "Frozen source file differs from complete admitted membership"
        );
        Ok(FrozenSourceMember {
            snapshot: self.scope.snapshot.clone(),
            position,
        })
    }
    pub(crate) fn members(&self) -> impl Iterator<Item = FrozenSourceMember> + '_ {
        (0..self.scope.snapshot.files.len()).map(|position| FrozenSourceMember {
            snapshot: self.scope.snapshot.clone(),
            position,
        })
    }
}
impl FrozenSourceMember {
    pub(crate) fn snapshot(&self) -> &Snapshot {
        self.snapshot.as_ref()
    }
    pub(crate) fn file(&self) -> &crate::model::SourceFile {
        &self.snapshot.files[self.position]
    }
}

/// Both raw rows share one budget and are admitted before decoding either row.
pub(crate) fn archive_initial_scope(
    store: &Store,
    snapshot_id: &str,
    max_metadata_bytes: usize,
    deadline: &Deadline,
) -> Result<(Snapshot, Graph)> {
    deadline.check()?;
    store.set_deadline(deadline)?;
    let maximum = max_metadata_bytes.min(VIEW_INPUT_BYTES);
    ensure!(maximum > 0, "Initial archive metadata capacity exceeded");
    let _read = if store.db.is_autocommit() {
        Some(Transaction::new_unchecked(
            &store.db,
            TransactionBehavior::Deferred,
        )?)
    } else {
        None
    };
    let raw_snapshot = raw_db_row(store, &table_key("snapshots", snapshot_id), maximum)?
        .context("Archive snapshot absent")?;
    let remaining = maximum
        .checked_sub(raw_snapshot.len())
        .context("Initial archive metadata capacity exceeded")?;
    let raw_graph = raw_db_row(store, &table_key("graphs", snapshot_id), remaining)?
        .context("Archive canonical graph absent")?;
    deadline.check()?;
    let snapshot_value = crate::json::parse(raw_snapshot.as_bytes(), maximum)?;
    let graph_value = crate::json::parse(raw_graph.as_bytes(), maximum)?;
    let snapshot: Snapshot = crate::json::decode(snapshot_value.clone())?;
    let graph: Graph = crate::json::decode(graph_value.clone())?;
    ensure!(
        serde_json::to_value(&snapshot)? == snapshot_value
            && serde_json::to_value(&graph)? == graph_value,
        "Unknown or lossy archive initial scope schema"
    );
    ensure!(
        snapshot.id == snapshot_id
            && snapshot.workspace_id == store.workspace_id
            && graph.snapshot_id == snapshot_id
            && graph.entities.iter().all(|e| e.snapshot_id == snapshot_id)
            && graph
                .observations
                .iter()
                .all(|e| e.snapshot_id == snapshot_id)
            && graph.evidence.iter().all(|e| e.snapshot_id == snapshot_id),
        "Archive initial scope identity mismatch"
    );
    Ok((snapshot, graph))
}

#[derive(Clone, Copy)]
pub(crate) struct ArchivePreparationLimits {
    pub remaining_input_bytes: usize,
    pub max_archive_bytes: usize,
    pub max_source_bytes: usize,
}
pub(crate) type FrozenScopePreflight<'a> = dyn FnMut(&Snapshot, &Deadline) -> Result<()> + 'a;

pub(crate) fn checked_published_set_for_archive(
    store: &Store,
    snapshot: &Snapshot,
    base: &Graph,
    graph_ids: &[String],
    limits: ArchivePreparationLimits,
    preflight: &mut FrozenScopePreflight<'_>,
    deadline: &Deadline,
) -> Result<CheckedPublishedSet> {
    ensure!(
        (1..=VIEW_INPUT_BYTES).contains(&limits.max_archive_bytes)
            && (1..=64_000_000).contains(&limits.max_source_bytes),
        "Invalid Meaning archive preparation limits"
    );
    checked_published_set_impl(
        store,
        snapshot,
        base,
        graph_ids,
        limits,
        Some(preflight),
        deadline,
    )
}

#[cfg(all(test, windows))]
#[path = "meaning_archive_order_tests.rs"]
mod archive_preparation_order_tests;

#[cfg(test)]
#[path = "meaning_archive_scope_tests.rs"]
mod archive_scope_tests;

pub const JOB_KIND: &str = "native_meaning_job_v1";
pub const CACHE_KIND: &str = "native_meaning_cache_v1";
pub const GRAPH_KIND: &str = "native_meaning_graph_v1";
const RESPONSE_KIND: &str = "native_meaning_response_v1";
const MAX_JOB: usize = 8_000_000;
const MAX_RESPONSE: usize = 2_000_000;
const MAX_CACHE: usize = 2_000_000;
const MAX_GRAPH: usize = 64_000_000;
const MAX_ROWS: usize = 8192;
const MAX_RECORD_BYTES: usize = 512_000_000;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub enum MeaningSelection {
    Files(Vec<String>),
    Bodies(Vec<String>),
}
#[derive(Clone, Debug)]
pub struct MeaningPolicy {
    pub contract_hash: String,
    pub generation: String,
    pub selection: MeaningSelection,
    pub context_groups: Vec<Vec<String>>,
    pub provider_input_files: Vec<std::path::PathBuf>,
    pub max_packets: usize,
    pub max_total_source_bytes: usize,
    pub max_graph_bytes: usize,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct PolicyRecord {
    contract_hash: String,
    generation: String,
    selection: MeaningSelection,
    context_groups: Vec<Vec<String>>,
    max_packets: usize,
    max_total_source_bytes: usize,
    max_graph_bytes: usize,
}
impl From<&MeaningPolicy> for PolicyRecord {
    fn from(p: &MeaningPolicy) -> Self {
        Self {
            contract_hash: p.contract_hash.clone(),
            generation: p.generation.clone(),
            selection: p.selection.clone(),
            context_groups: p.context_groups.clone(),
            max_packets: p.max_packets,
            max_total_source_bytes: p.max_total_source_bytes,
            max_graph_bytes: p.max_graph_bytes,
        }
    }
}
impl PolicyRecord {
    fn validate(&self) -> Result<()> {
        ensure!(
            valid_id(&self.contract_hash),
            "Invalid host meaning contract hash"
        );
        crate::model_protocol::bounded_text(&self.generation, 160)?;
        ensure!(
            (1..=64).contains(&self.max_packets)
                && (1..=4_000_000).contains(&self.max_total_source_bytes)
                && (1..=MAX_GRAPH).contains(&self.max_graph_bytes),
            "Invalid host meaning capacity policy"
        );
        let keys = match &self.selection {
            MeaningSelection::Files(k) | MeaningSelection::Bodies(k) => k,
        };
        ensure!(
            !keys.is_empty() && keys.len() <= 256 && self.context_groups.len() <= 64,
            "Empty or oversized complete meaning selection"
        );
        let selected: BTreeSet<_> = keys.iter().collect();
        ensure!(selected.len() == keys.len(), "Duplicate meaning selection");
        for group in &self.context_groups {
            let seen: BTreeSet<_> = group.iter().collect();
            ensure!(
                (2..=16).contains(&group.len())
                    && seen.len() == group.len()
                    && seen.iter().all(|k| selected.contains(k)),
                "Context packet references duplicate or unselected units"
            );
        }
        Ok(())
    }
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ProviderRecord {
    model_id: String,
    model_version: String,
    model: String,
    fingerprint: String,
    task_id: String,
    currency: String,
    reservation: u64,
    task_limit: u64,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Packet {
    input: MeaningInput,
    cache_key: String,
    attempt_id: Option<String>,
    request_id: Option<String>,
    response_artifact_id: Option<String>,
    origin_job_id: Option<String>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Job {
    schema_version: u8,
    job_id: String,
    nonce: String,
    workspace_id: String,
    snapshot_id: String,
    graph_hash: String,
    analyzer_epoch: String,
    capture_policy_hash: String,
    provider: ProviderRecord,
    policy: PolicyRecord,
    packets: Vec<Packet>,
    state: String,
    failure: Option<String>,
    graph_id: Option<String>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Response {
    schema_version: u8,
    kind: String,
    job_id: String,
    packet_index: usize,
    attempt_id: String,
    request_id: String,
    input_hash: String,
    model: String,
    model_version: String,
    source_bindings: Vec<Binding>,
    currency: String,
    actual_cost_micros: Option<u64>,
    usage: Option<Usage>,
    outcome: String,
    fragment: Option<Fragment>,
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
struct Cache {
    schema_version: u8,
    cache_key: String,
    workspace_id: String,
    origin_job_id: String,
    origin_snapshot_id: String,
    origin_graph_hash: String,
    packet_index: usize,
    response_artifact_id: String,
    input: MeaningInput,
    provider_fingerprint: String,
    model: String,
    analyzer_epoch: String,
    capture_policy_hash: String,
    generation: String,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct GraphRecord {
    schema_version: u8,
    graph_id: String,
    job_id: String,
    workspace_id: String,
    snapshot_id: String,
    artifact_id: String,
    total_bytes: usize,
}
#[derive(Clone)]
struct Resolved {
    response_id: String,
    response: Response,
    origin_snapshot: String,
    origin_job: String,
    cache: Cache,
}

fn read_optional_raw(
    db: &Connection,
    kind: &str,
    id: &str,
    maximum: usize,
) -> Result<Option<String>> {
    let size: Option<usize> = db
        .query_row(
            "SELECT length(CAST(value AS BLOB)) FROM records WHERE kind=?1 AND id=?2",
            params![kind, id],
            |r| r.get(0),
        )
        .optional()?;
    let Some(size) = size else { return Ok(None) };
    ensure!(size <= maximum, "Meaning record exceeds capacity");
    let raw: String = db.query_row(
        "SELECT value FROM records WHERE kind=?1 AND id=?2 AND length(CAST(value AS BLOB))<=?3",
        params![kind, id, maximum],
        |r| r.get(0),
    )?;
    ensure!(raw.len() == size, "Meaning record changed while reading");
    Ok(Some(raw))
}
fn typed<T: DeserializeOwned>(raw: &str, maximum: usize) -> Result<T> {
    crate::json::decode(crate::json::parse(raw.as_bytes(), maximum)?)
}
fn read_record<T: DeserializeOwned>(
    store: &Store,
    kind: &str,
    id: &str,
    maximum: usize,
) -> Result<T> {
    typed(
        &read_optional_raw(&store.db, kind, id, maximum)?.context("Meaning record missing")?,
        maximum,
    )
}
fn insert_record<T: Serialize>(
    tx: &Transaction<'_>,
    kind: &str,
    id: &str,
    value: &T,
    maximum: usize,
    d: &Deadline,
) -> Result<()> {
    let bytes = encode_bounded(value, maximum, d)?;
    tx.execute(
        "INSERT INTO records(kind,id,value) VALUES(?1,?2,?3)",
        params![kind, id, std::str::from_utf8(&bytes)?],
    )?;
    Ok(())
}
fn job_identity(job: &Job) -> String {
    identity(&(
        JOB_KIND,
        &job.workspace_id,
        &job.snapshot_id,
        &job.graph_hash,
        &job.analyzer_epoch,
        &job.capture_policy_hash,
        identity(&job.policy),
        &job.provider.fingerprint,
        &job.nonce,
        job.packets.iter().map(|p| &p.cache_key).collect::<Vec<_>>(),
    ))
}
fn graph_id(job: &Job) -> String {
    identity(&(GRAPH_KIND, &job.job_id))
}
fn cache_key(input: &MeaningInput, job: &Job) -> String {
    identity(&(
        CACHE_KIND,
        input,
        &job.provider.model,
        &job.provider.currency,
        &job.provider.fingerprint,
        &job.analyzer_epoch,
        &job.capture_policy_hash,
        &job.policy.generation,
    ))
}
fn request_id(attempt: &str, provider: &ProviderRecord, input: &MeaningInput) -> String {
    identity(&(
        attempt,
        meaning_graph::OPERATION,
        &provider.model_id,
        &provider.model_version,
        identity(&serde_json::to_value(input).expect("meaning input serializable")),
    ))
}
fn bindings(input: &MeaningInput) -> Vec<Binding> {
    input.sources.iter().map(|s| s.anchor.clone()).collect()
}
fn request(
    provider: &ProviderSpec,
    id: &str,
    input: &MeaningInput,
    d: &Deadline,
) -> Result<Vec<u8>> {
    let payload = serde_json::to_value(input)?;
    encode_bounded(
        &json!({"schema":1,"operation":meaning_graph::OPERATION,"model":provider.model_id,"modelVersion":provider.model_version,"requestId":id,"inputHash":identity(&payload),"currency":provider.currency,"taskId":provider.task_id,"costLimitMicros":provider.max_cost_micros,"payload":payload}),
        meaning_graph::MAX_INPUT_BYTES + 16_384,
        d,
    )
}
fn source_scope(
    store: &Store,
    snapshot: &Snapshot,
    graph: &Graph,
    d: &Deadline,
) -> Result<(String, String)> {
    d.check()?;
    store.set_deadline(d)?;
    ensure!(
        valid_id(&snapshot.id)
            && snapshot.workspace_id == store.workspace_id
            && graph.snapshot_id == snapshot.id,
        "Meaning snapshot workspace mismatch"
    );
    ensure!(
        identity(&store.snapshot(&snapshot.id)?) == identity(snapshot)
            && identity(&store.graph(&snapshot.id)?) == identity(graph),
        "Meaning inputs differ from immutable source store"
    );
    let epoch = crate::capture::analyzer_epoch_for_snapshot_with_deadline(store, snapshot, d)?
        .producer()
        .to_owned();
    // Scope-policy construction verifies capture/dependency grants before source reads.
    let _scope = crate::capture::source_scope_policy(store, snapshot)?;
    let capture = identity(&(
        &snapshot.capture_consistency,
        &snapshot.kind,
        &snapshot.omissions,
        crate::capture::options_for_snapshot(store, snapshot)?,
    ));
    Ok((epoch, capture))
}
fn prepare(
    store: &Store,
    snapshot: &Snapshot,
    graph: &Graph,
    p: &PolicyRecord,
    d: &Deadline,
) -> Result<Vec<MeaningInput>> {
    p.validate()?;
    source_scope(store, snapshot, graph, d)?;
    let selected = match &p.selection {
        MeaningSelection::Files(v) | MeaningSelection::Bodies(v) => v,
    };
    let scope = crate::capture::source_scope_policy(store, snapshot)?;
    scope.require_legacy_text("Meaning source selection")?;
    let mut units = BTreeMap::new();
    let mut total = 0usize;
    for key in selected {
        d.check()?;
        let (path, start, end) = match &p.selection {
            MeaningSelection::Files(_) => {
                crate::paths::normalized_syntax(key)?;
                let file = scope.file(key)?;
                (key.as_str(), 0, file.size)
            }
            MeaningSelection::Bodies(_) => {
                ensure!(valid_id(key), "Invalid meaning body identity");
                let e = graph
                    .entities
                    .iter()
                    .find(|e| &e.id == key && e.kind != "file")
                    .context("Selected meaning body absent")?;
                ensure!(
                    e.snapshot_id == snapshot.id,
                    "Meaning body snapshot mismatch"
                );
                (e.path.as_str(), e.start_byte, e.end_byte)
            }
        };
        ensure!(
            end >= start && end - start <= 16_384,
            "Complete meaning unit exceeds source byte capacity"
        );
        let file = scope.file(path)?;
        ensure!(
            file.size <= 4_000_000,
            "Meaning source file exceeds capacity"
        );
        let bytes = scope.read_source(path)?;
        let original = std::str::from_utf8(&bytes)?;
        let text = crate::paths::utf8_range(original, start, end)?.to_owned();
        total = total
            .checked_add(text.len())
            .context("Meaning source count overflow")?;
        ensure!(
            total <= p.max_total_source_bytes,
            "Complete meaning source selection exceeds budget"
        );
        let mut anchor = Binding {
            source_id: String::new(),
            path: path.to_owned(),
            content_hash: file.hash.clone(),
            start_byte: start,
            end_byte: end,
        };
        anchor.source_id = meaning_graph::unit_id(&snapshot.workspace_id, &anchor, &text);
        units.insert(
            key.clone(),
            Source {
                anchor,
                text,
                omitted_bytes: 0,
            },
        );
    }
    let mut groups = selected.iter().map(|s| vec![s.clone()]).collect::<Vec<_>>();
    groups.extend(p.context_groups.clone());
    ensure!(
        groups.len() <= p.max_packets,
        "Complete meaning packet count exceeds budget"
    );
    let mut seen = BTreeSet::new();
    let mut packets = Vec::new();
    for group in groups {
        d.check()?;
        let sources = group
            .iter()
            .map(|key| {
                units
                    .get(key)
                    .cloned()
                    .context("Missing selected meaning context")
            })
            .collect::<Result<Vec<_>>>()?;
        let input = MeaningInput {
            workspace_id: snapshot.workspace_id.clone(),
            unit_set_hash: identity(&serde_json::to_value(&sources)?),
            contract_hash: identity(&(
                &p.contract_hash,
                crate::model_protocol::meaning_contract_fingerprint(),
            )),
            sources,
        };
        meaning_graph::validate_input(&input, d)?;
        ensure!(
            seen.insert(identity(&input)),
            "Duplicate meaning request packet"
        );
        encode_bounded(&input, meaning_graph::MAX_INPUT_BYTES, d)?;
        packets.push(input);
    }
    Ok(packets)
}
fn capacity(
    store: &Store,
    packets: usize,
    misses: usize,
    p: &PolicyRecord,
    d: &Deadline,
) -> Result<()> {
    d.check()?;
    let worst = packets
        .checked_mul(
            meaning_graph::MAX_MATERIALIZED_BYTES
                + meaning_graph::MAX_INPUT_BYTES
                + meaning_graph::MAX_FRAGMENT_BYTES,
        )
        .and_then(|v| v.checked_add(256_000))
        .context("Meaning graph capacity overflow")?;
    ensure!(
        worst <= p.max_graph_bytes,
        "Entire meaning graph worst-case capacity exceeds host budget"
    );
    let(rows,bytes):(usize,usize)=store.db.query_row("SELECT count(*),coalesce(sum(length(CAST(value AS BLOB))),0) FROM records WHERE kind IN (?1,?2,?3)",params![JOB_KIND,CACHE_KIND,GRAPH_KIND],|r|Ok((r.get(0)?,r.get(1)?)))?;
    ensure!(
        rows.checked_add(misses + 2).is_some_and(|n| n <= MAX_ROWS)
            && bytes
                .checked_add(MAX_JOB + misses * MAX_CACHE + 64_000)
                .is_some_and(|n| n <= MAX_RECORD_BYTES),
        "Meaning record sink capacity exceeded"
    );
    Ok(())
}
fn validate_header(store: &Store, job: &Job, d: &Deadline) -> Result<()> {
    d.check()?;
    job.policy.validate()?;
    ensure!(
        job.schema_version == 1
            && job.workspace_id == store.workspace_id
            && valid_id(&job.nonce)
            && job.job_id == job_identity(job)
            && valid_id(&job.provider.fingerprint),
        "Meaning job identity mismatch"
    );
    let p = &job.provider;
    crate::model_protocol::bounded_text(&p.model_id, 100)?;
    crate::model_protocol::bounded_text(&p.model_version, 100)?;
    crate::model_protocol::bounded_text(&p.task_id, 200)?;
    ensure!(
        !p.model_id.contains('@')
            && !p.model_version.contains('@')
            && p.model == format!("{}@{}", p.model_id, p.model_version)
            && p.currency.len() == 3
            && p.currency.bytes().all(|b| b.is_ascii_uppercase()),
        "Meaning provider policy mismatch"
    );
    validate_cost(p.reservation)?;
    validate_cost(p.task_limit)?;
    ensure!(
        ["reserved", "running", "failed", "complete"].contains(&job.state.as_str())
            && job
                .failure
                .as_ref()
                .is_none_or(|f| f == "meaning_incomplete_or_publication_failed"),
        "Unknown meaning job state"
    );
    ensure!(
        (job.state == "complete") == job.graph_id.is_some()
            && (job.state == "failed") == job.failure.is_some(),
        "Meaning job state fields disagree"
    );
    if let Some(id) = &job.graph_id {
        ensure!(id == &graph_id(job), "Meaning job graph namespace mismatch");
    }
    let snapshot = store.snapshot(&job.snapshot_id)?;
    let graph = store.graph(&job.snapshot_id)?;
    let (epoch, policy) = source_scope(store, &snapshot, &graph, d)?;
    ensure!(
        job.graph_hash == identity(&graph)
            && job.analyzer_epoch == epoch
            && job.capture_policy_hash == policy,
        "Meaning source graph or capture epoch mismatch"
    );
    let inputs = prepare(store, &snapshot, &graph, &job.policy, d)?;
    ensure!(
        inputs.len() == job.packets.len(),
        "Meaning packet count differs from complete selection"
    );
    let mut attempts = BTreeSet::new();
    for (p, input) in job.packets.iter().zip(inputs) {
        ensure!(
            p.input == input && p.cache_key == cache_key(&input, job),
            "Meaning packet source replay mismatch"
        );
        ensure!(
            p.attempt_id.is_some() == p.request_id.is_some(),
            "Meaning packet attempt identity incomplete"
        );
        if let Some(attempt) = &p.attempt_id {
            ensure!(
                valid_id(attempt)
                    && attempts.insert(attempt)
                    && p.request_id.as_deref()
                        == Some(&request_id(attempt, &job.provider, &p.input)),
                "Meaning request identity mismatch"
            );
        } else {
            ensure!(
                p.origin_job_id.as_deref().is_some_and(valid_id)
                    && p.response_artifact_id.as_deref().is_some_and(valid_id),
                "Cache-hit packet missing immutable origin"
            );
        }
        if let Some(origin) = &p.origin_job_id {
            ensure!(
                p.attempt_id.is_none() && origin != &job.job_id,
                "Meaning packet has conflicting provider/cache provenance"
            );
        }
    }
    Ok(())
}
fn read_response(store: &Store, id: &str, d: &Deadline) -> Result<Response> {
    ensure!(valid_id(id), "Invalid meaning response artifact ID");
    d.check()?;
    let size = store.artifact_size(id)?;
    ensure!(size <= MAX_RESPONSE, "Meaning receipt exceeds capacity");
    let bytes = store.read_artifact_limited(id, MAX_RESPONSE)?;
    ensure!(
        bytes.len() == size && hash(&bytes) == id,
        "Meaning receipt hash mismatch"
    );
    crate::json::decode(crate::json::parse(&bytes, MAX_RESPONSE)?)
}
fn checked_response(
    store: &Store,
    job: &Job,
    index: usize,
    id: &str,
    d: &Deadline,
) -> Result<Response> {
    let p = job
        .packets
        .get(index)
        .context("Meaning response packet index absent")?;
    let r = read_response(store, id, d)?;
    ensure!(
        r.schema_version == 1
            && r.kind == RESPONSE_KIND
            && r.job_id == job.job_id
            && r.packet_index == index
            && Some(r.attempt_id.as_str()) == p.attempt_id.as_deref()
            && Some(r.request_id.as_str()) == p.request_id.as_deref()
            && r.request_id == request_id(&r.attempt_id, &job.provider, &p.input)
            && r.input_hash == identity(&serde_json::to_value(&p.input)?)
            && r.model == job.provider.model_id
            && r.model_version == job.provider.model_version
            && r.source_bindings == bindings(&p.input)
            && r.currency == job.provider.currency,
        "Meaning receipt identity/source mismatch"
    );
    let a = Ledger::new(store, d).attempt_status(&r.attempt_id)?;
    ensure!(
        a.task_id == job.provider.task_id
            && a.provider == job.provider.model
            && a.currency == r.currency
            && a.reserved_micros == job.provider.reservation
            && matches!(a.state, AttemptState::Unknown | AttemptState::Settled)
            && r.actual_cost_micros
                .is_none_or(|v| a.actual_cost_micros == Some(v))
            && r.usage
                .as_ref()
                .is_none_or(|u| a.response_usage.as_ref() == Some(u)),
        "Meaning response ledger binding mismatch"
    );
    ensure!(
        [
            "complete",
            "process_failure",
            "provider_failure",
            "invalid_fragment"
        ]
        .contains(&r.outcome.as_str()),
        "Unknown meaning response outcome"
    );
    if r.outcome == "complete" {
        let f = r
            .fragment
            .as_ref()
            .context("Complete meaning receipt lacks fragment")?;
        // Preserve provider-local keys, duplicate claims and submitted order in
        // the immutable receipt. Deterministic normalization is a derived view.
        meaning_graph::normalize(&p.input, f, d)?;
    } else {
        ensure!(
            r.fragment.is_none() && job.state != "complete",
            "Incomplete meaning receipt carries accepted claims"
        );
    }
    Ok(r)
}
fn make_cache(job: &Job, index: usize, response_id: &str) -> Cache {
    let p = &job.packets[index];
    Cache {
        schema_version: 1,
        cache_key: p.cache_key.clone(),
        workspace_id: job.workspace_id.clone(),
        origin_job_id: job.job_id.clone(),
        origin_snapshot_id: job.snapshot_id.clone(),
        origin_graph_hash: job.graph_hash.clone(),
        packet_index: index,
        response_artifact_id: response_id.to_owned(),
        input: p.input.clone(),
        provider_fingerprint: job.provider.fingerprint.clone(),
        model: job.provider.model.clone(),
        analyzer_epoch: job.analyzer_epoch.clone(),
        capture_policy_hash: job.capture_policy_hash.clone(),
        generation: job.policy.generation.clone(),
    }
}
fn checked_cache(
    store: &Store,
    key: &str,
    expected: &MeaningInput,
    job: &Job,
    d: &Deadline,
) -> Result<Resolved> {
    let c: Cache = read_record(store, CACHE_KIND, key, MAX_CACHE)?;
    ensure!(
        c.schema_version == 1
            && c.cache_key == key
            && key == cache_key(expected, job)
            && c.workspace_id == store.workspace_id
            && &c.input == expected
            && c.provider_fingerprint == job.provider.fingerprint
            && c.model == job.provider.model
            && c.analyzer_epoch == job.analyzer_epoch
            && c.capture_policy_hash == job.capture_policy_hash
            && c.generation == job.policy.generation,
        "Meaning cache input/configuration binding mismatch"
    );
    let origin: Job = read_record(store, JOB_KIND, &c.origin_job_id, MAX_JOB)?;
    validate_header(store, &origin, d)?;
    ensure!(
        origin.state == "complete"
            && origin.job_id == c.origin_job_id
            && origin.snapshot_id == c.origin_snapshot_id
            && origin.graph_hash == c.origin_graph_hash,
        "Meaning cache original source scope mismatch"
    );
    let packet = origin
        .packets
        .get(c.packet_index)
        .context("Meaning cache original packet absent")?;
    ensure!(
        packet.attempt_id.is_some()
            && packet.origin_job_id.is_none()
            && packet.input == c.input
            && packet.cache_key == key
            && packet.response_artifact_id.as_deref() == Some(&c.response_artifact_id),
        "Meaning cache original response linkage mismatch"
    );
    let expected_cache = make_cache(&origin, c.packet_index, &c.response_artifact_id);
    ensure!(
        identity(&expected_cache) == identity(&c),
        "Meaning cache original extraction metadata mismatch"
    );
    let response = checked_response(store, &origin, c.packet_index, &c.response_artifact_id, d)?;
    ensure!(
        response.outcome == "complete",
        "Meaning cache references unsuccessful response"
    );
    let resolved = Resolved {
        response_id: c.response_artifact_id.clone(),
        response,
        origin_snapshot: c.origin_snapshot_id.clone(),
        origin_job: c.origin_job_id.clone(),
        cache: c,
    };
    check_origin_graph(store, &origin, &resolved, d)?;
    Ok(resolved)
}
fn materialized_packet(job: &Job, index: usize, r: &Resolved, d: &Deadline) -> Result<Value> {
    let p = job.packets.get(index).context("Meaning packet absent")?;
    ensure!(
        r.response.outcome == "complete",
        "Cannot materialize unsuccessful meaning receipt"
    );
    let fragment = r
        .response
        .fragment
        .as_ref()
        .context("Meaning fragment absent")?;
    Ok(
        json!({"index":index,"inputHash":identity(&serde_json::to_value(&p.input)?),"meaning":meaning_graph::materialize(&p.input,fragment,d)?,"provenance":{"snapshotId":job.snapshot_id,"originalSnapshotId":r.origin_snapshot,"originalJobId":r.origin_job,"responseArtifactId":r.response_id,"attemptId":r.response.attempt_id,"requestId":r.response.request_id,"currency":r.response.currency,"providerReportedCostMicros":r.response.actual_cost_micros,"providerReportedUsage":r.response.usage,"rebound":r.origin_snapshot!=job.snapshot_id,"bindingValidation":"exact_captured_source_replayed"}}),
    )
}
// Verify only the directly referenced original packet. Full graph loading
// validates every packet; this bounded check avoids recursive cache/job cycles.
fn check_origin_graph(
    store: &Store,
    origin: &Job,
    r: &Resolved,
    d: &Deadline,
) -> Result<GraphRecord> {
    let id = origin
        .graph_id
        .as_deref()
        .context("Meaning cache origin graph absent")?;
    let record: GraphRecord = read_record(store, GRAPH_KIND, id, 64_000)?;
    ensure!(
        record.schema_version == 1
            && record.graph_id == id
            && id == graph_id(origin)
            && record.job_id == origin.job_id
            && record.workspace_id == store.workspace_id
            && record.snapshot_id == origin.snapshot_id
            && valid_id(&record.artifact_id)
            && record.total_bytes <= origin.policy.max_graph_bytes,
        "Meaning cache original graph registration mismatch"
    );
    ensure!(
        store.artifact_size(&record.artifact_id)? == record.total_bytes,
        "Meaning cache original graph size mismatch"
    );
    let bytes = store.read_artifact_limited(&record.artifact_id, MAX_GRAPH)?;
    ensure!(
        bytes.len() == record.total_bytes && hash(&bytes) == record.artifact_id,
        "Meaning cache original graph artifact hash mismatch"
    );
    let graph = crate::json::parse(&bytes, MAX_GRAPH)?;
    ensure!(
        graph["schemaVersion"] == 1
            && graph["kind"] == GRAPH_KIND
            && graph["graphId"] == id
            && graph["jobId"] == origin.job_id
            && graph["workspaceId"] == origin.workspace_id
            && graph["snapshotId"] == origin.snapshot_id
            && graph["graphHash"] == origin.graph_hash
            && graph["packets"]
                .as_array()
                .is_some_and(|p| p.len() == origin.packets.len())
            && graph["packets"][r.cache.packet_index]
                == materialized_packet(origin, r.cache.packet_index, r, d)?,
        "Meaning cache original graph packet differs from receipt"
    );
    Ok(record)
}
fn checked_job(store: &Store, job: &Job, d: &Deadline) -> Result<Vec<Option<Resolved>>> {
    validate_header(store, job, d)?;
    let mut resolved = Vec::new();
    for (index, p) in job.packets.iter().enumerate() {
        d.check()?;
        if p.origin_job_id.is_some() {
            let r = checked_cache(store, &p.cache_key, &p.input, job, d)?;
            ensure!(
                p.origin_job_id.as_deref() == Some(&r.origin_job)
                    && p.response_artifact_id.as_deref() == Some(&r.response_id),
                "Meaning job reused origin mismatch"
            );
            resolved.push(Some(r));
        } else if let Some(id) = &p.response_artifact_id {
            let r = checked_response(store, job, index, id, d)?;
            let c = make_cache(job, index, id);
            resolved.push(Some(Resolved {
                response_id: id.clone(),
                response: r,
                origin_snapshot: job.snapshot_id.clone(),
                origin_job: job.job_id.clone(),
                cache: c,
            }));
        } else {
            ensure!(
                job.state != "complete",
                "Published meaning job lacks required response"
            );
            let a = Ledger::new(store, d).attempt_status(
                p.attempt_id
                    .as_ref()
                    .context("Missing meaning reserved attempt")?,
            )?;
            ensure!(
                a.task_id == job.provider.task_id
                    && a.provider == job.provider.model
                    && a.currency == job.provider.currency
                    && a.reserved_micros == job.provider.reservation,
                "Meaning reserved attempt scope mismatch"
            );
            resolved.push(None);
        }
    }
    if job.state == "complete" {
        ensure!(
            resolved
                .iter()
                .all(|r| r.as_ref().is_some_and(|r| r.response.outcome == "complete")),
            "Published meaning job contains incomplete packet"
        );
    }
    Ok(resolved)
}
fn assembled(job: &Job, resolved: &[Option<Resolved>], d: &Deadline) -> Result<Value> {
    let mut packets = Vec::new();
    for (index, r) in resolved.iter().enumerate() {
        d.check()?;
        let r = r.as_ref().context("Missing complete meaning packet")?;
        ensure!(
            r.response.outcome == "complete",
            "Cannot materialize incomplete meaning packet"
        );
        packets.push(materialized_packet(job, index, r, d)?);
    }
    let snapshot_coverage = json!({"selectedUnits":match &job.policy.selection{MeaningSelection::Files(k)|MeaningSelection::Bodies(k)=>k.len()},"packets":packets.len(),"crossFilePackets":job.policy.context_groups.len(),"semanticExhaustive":false});
    Ok(
        json!({"schemaVersion":1,"kind":GRAPH_KIND,"graphId":graph_id(job),"jobId":job.job_id,"workspaceId":job.workspace_id,"snapshotId":job.snapshot_id,"graphHash":job.graph_hash,"resolution":"inferred_concept","providerAttestation":"unverified","coverage":snapshot_coverage,"packets":packets}),
    )
}
fn progress(store: &Store, job: &Job, d: &Deadline) -> Result<()> {
    let raw = encode_bounded(job, MAX_JOB, d)?;
    store.set_deadline(d)?;
    ensure!(
        store.db.execute(
            "UPDATE records SET value=?3 WHERE kind=?1 AND id=?2",
            params![JOB_KIND, job.job_id, std::str::from_utf8(&raw)?]
        )? == 1,
        "Meaning job disappeared"
    );
    Ok(())
}
fn cleanup(store: &Store, job: &Job, d: &Deadline) {
    let ledger = Ledger::new(store, d);
    for p in &job.packets {
        if let Some(id) = &p.attempt_id
            && let Ok(a) = ledger.attempt_status(id)
        {
            match a.state {
                AttemptState::Reserved => {
                    let _ = ledger.cancel_before_send(id);
                }
                AttemptState::Sent => {
                    let _ = ledger.settle(id, None);
                }
                _ => {}
            }
        }
    }
}
fn dispatch(
    store: &Store,
    job: &Job,
    index: usize,
    provider: &ProviderSpec,
    token: ReservedAttempt,
    d: &Deadline,
) -> Result<Response> {
    let p = &job.packets[index];
    let temp = tempfile::Builder::new().prefix("yoke-meaning-").tempdir()?;
    let input = request(
        provider,
        p.request_id
            .as_deref()
            .context("Missing meaning request ID")?,
        &p.input,
        d,
    )?;
    let mut cmd = Command::new(&provider.command);
    cmd.args(&provider.args).current_dir(temp.path());
    let ledger = Ledger::new(store, d);
    let id = ledger.send_reserved(token)?;
    ensure!(
        Some(id.as_str()) == p.attempt_id.as_deref(),
        "Meaning reserved send mismatch"
    );
    let out = crate::process::run_capture(
        cmd,
        &input,
        d,
        crate::process::CaptureLimits {
            stdin_bytes: meaning_graph::MAX_INPUT_BYTES + 16_384,
            stdout_bytes: MAX_RESPONSE,
            stderr_bytes: 256_000,
        },
    )?;
    ensure!(
        out.stdin_complete
            && out.output_complete
            && !out.timed_out
            && !out.output_exceeded
            && !out.io_error,
        "Meaning provider output incomplete or expired"
    );
    let reply: Reply = crate::json::decode(crate::json::parse(&out.stdout, MAX_RESPONSE)?)?;
    ensure!(
        reply.schema == 1
            && reply.operation == meaning_graph::OPERATION
            && reply.model == job.provider.model_id
            && reply.model_version == job.provider.model_version
            && Some(reply.request_id.as_str()) == p.request_id.as_deref()
            && reply.input_hash == identity(&serde_json::to_value(&p.input)?)
            && reply.source_bindings == bindings(&p.input)
            && reply.currency == job.provider.currency,
        "Meaning reply identity or source bindings mismatch"
    );
    if let Some(cost) = reply.actual_cost_micros {
        validate_cost(cost)?;
    }
    if let Some(usage) = &reply.usage {
        usage.validate()?;
    }
    // Billing is accepted only after complete envelope identity validation, and
    // before domain interpretation or a nonzero process exit can reject output.
    if let Some(usage) = &reply.usage {
        ledger.usage(&id, "response", usage.clone())?;
    }
    ledger.settle(&id, reply.actual_cost_micros)?;
    let validated = (|| -> Result<Fragment> {
        let data = reply
            .data
            .as_ref()
            .context("Meaning reply has no fragment")?;
        encode_bounded(data, meaning_graph::MAX_FRAGMENT_BYTES, d)?;
        let f: Fragment = crate::json::decode(data.clone())?;
        meaning_graph::normalize(&p.input, &f, d)?;
        Ok(f)
    })();
    let (outcome, fragment) = if out.exit_code != Some(0) {
        ("process_failure", None)
    } else if reply.error.is_some() {
        ("provider_failure", None)
    } else {
        match validated {
            Ok(f) => ("complete", Some(f)),
            Err(_) => ("invalid_fragment", None),
        }
    };
    Ok(Response {
        schema_version: 1,
        kind: RESPONSE_KIND.into(),
        job_id: job.job_id.clone(),
        packet_index: index,
        attempt_id: id,
        request_id: reply.request_id,
        input_hash: reply.input_hash,
        model: reply.model,
        model_version: reply.model_version,
        source_bindings: reply.source_bindings,
        currency: reply.currency,
        actual_cost_micros: reply.actual_cost_micros,
        usage: reply.usage,
        outcome: outcome.into(),
        fragment,
    })
}
fn resolve_executable(store: &Store, command: &Path) -> Result<PathBuf> {
    if command.is_absolute() {
        let path = command.canonicalize()?;
        ensure!(path.is_file(), "Meaning provider executable is not a file");
        return Ok(path);
    }
    ensure!(
        command.components().count() == 1,
        "Relative provider executable paths require an absolute host path"
    );
    let workspace = store
        .dir
        .parent()
        .context("Missing source workspace")?
        .canonicalize()?;
    for directory in std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default()) {
        if !directory.is_absolute() {
            continue;
        }
        let Ok(directory) = directory.canonicalize() else {
            continue;
        };
        if directory.starts_with(&workspace) {
            continue;
        }
        let names = vec![command.to_path_buf()];
        #[cfg(windows)]
        let names = {
            let mut names = names;
            if command.extension().is_none() {
                names.insert(0, command.with_extension("exe"));
            }
            names
        };
        for name in names {
            if let Ok(path) = directory.join(name).canonicalize()
                && path.is_file()
                && !path.starts_with(&workspace)
            {
                return Ok(path);
            }
        }
    }
    anyhow::bail!("Host meaning provider executable unavailable outside source workspace")
}
fn hash_host_file(path: &Path, d: &Deadline) -> Result<String> {
    use sha2::{Digest, Sha256};
    let mut file = std::fs::File::open(path)?;
    let initial = file.metadata()?;
    ensure!(
        initial.is_file() && initial.len() <= 256_000_000,
        "Provider implementation/configuration file exceeds hash capacity"
    );
    let mut h = Sha256::new();
    let mut total = 0u64;
    let mut buffer = [0; 65536];
    loop {
        d.check()?;
        let n = file.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        total = total
            .checked_add(n as u64)
            .context("Provider file length overflow")?;
        ensure!(total <= initial.len(), "Provider file grew while hashing");
        h.update(&buffer[..n]);
    }
    let final_meta = file.metadata()?;
    ensure!(
        total == initial.len()
            && final_meta.len() == initial.len()
            && final_meta.modified().ok() == initial.modified().ok(),
        "Provider implementation changed while hashing"
    );
    Ok(hex::encode(h.finalize()))
}
fn fingerprint(
    store: &Store,
    provider: &ProviderSpec,
    policy: &MeaningPolicy,
    d: &Deadline,
) -> Result<(String, PathBuf)> {
    fingerprint_host_provider(store, provider, &policy.provider_input_files, d)
}
/// Shared trusted-host implementation identity; it grants no source capability
/// and requires no fabricated native snapshot or selection policy.
pub(crate) fn fingerprint_host_provider(
    store: &Store,
    provider: &ProviderSpec,
    provider_input_files: &[PathBuf],
    d: &Deadline,
) -> Result<(String, PathBuf)> {
    ensure!(
        provider_input_files.len() <= 64,
        "Too many host-selected provider input files"
    );
    let exe = resolve_executable(store, &provider.command)?;
    let mut files = BTreeSet::new();
    files.insert(exe.clone());
    for arg in &provider.args {
        let p = Path::new(arg);
        if p.is_absolute()
            && p.extension().and_then(|s| s.to_str()).is_some_and(|s| {
                [
                    "js", "cjs", "mjs", "py", "ps1", "json", "toml", "yaml", "yml", "exe", "dll",
                    "wasm", "crt", "pem",
                ]
                .contains(&s.to_ascii_lowercase().as_str())
            })
        {
            files.insert(
                p.canonicalize()
                    .context("Provider implementation/config argument file unavailable")?,
            );
        }
    }
    for file in provider_input_files {
        ensure!(
            file.is_absolute(),
            "Provider input files require explicit absolute host paths"
        );
        files.insert(file.canonicalize()?);
    }
    let mut hashed = Vec::new();
    for file in files {
        d.check()?;
        hashed.push((file.clone(), hash_host_file(&file, d)?));
    }
    Ok((identity(&(&provider.command, &provider.args, hashed)), exe))
}
impl MeaningPolicy {
    pub fn files(snapshot: &Snapshot, contract_hash: String) -> Self {
        Self {
            contract_hash,
            generation: "default".into(),
            selection: MeaningSelection::Files(
                snapshot.files.iter().map(|f| f.path.clone()).collect(),
            ),
            context_groups: vec![],
            provider_input_files: vec![],
            max_packets: 64,
            max_total_source_bytes: 4_000_000,
            max_graph_bytes: 64_000_000,
        }
    }
}
/// Validate trusted host shapes without opening a store or reading any file.
pub fn validate_host_configuration(provider: &ProviderSpec, policy: &MeaningPolicy) -> Result<()> {
    provider.validate()?;
    PolicyRecord::from(policy).validate()?;
    ensure!(
        policy.provider_input_files.len() <= 64
            && policy.provider_input_files.iter().all(|p| p.is_absolute()),
        "Provider input files require bounded explicit absolute host paths"
    );
    Ok(())
}
/// Execute only the fixed, fully admitted host selection. There is no retry or resume.
pub fn build(
    store: &Store,
    snapshot: &Snapshot,
    graph: &Graph,
    provider: &ProviderSpec,
    policy: &MeaningPolicy,
    d: &Deadline,
) -> Result<Value> {
    validate_host_configuration(provider, policy)?;
    let policy_record = PolicyRecord::from(policy);
    store.with_exclusive_client(d,||{
        let inputs=prepare(store,snapshot,graph,&policy_record,d)?;let(epoch,capture_policy)=source_scope(store,snapshot,graph,d)?;
        let(fp,executable)=fingerprint(store,provider,policy,d)?;let mut pinned=provider.clone();pinned.command=executable;
        let mut nonce=[0u8;32];getrandom::fill(&mut nonce)?;
        let mut job=Job{schema_version:1,job_id:String::new(),nonce:hex::encode(nonce),workspace_id:store.workspace_id.clone(),snapshot_id:snapshot.id.clone(),graph_hash:identity(graph),analyzer_epoch:epoch,capture_policy_hash:capture_policy,provider:ProviderRecord{model_id:provider.model_id.clone(),model_version:provider.model_version.clone(),model:provider.model_key()?,fingerprint:fp,task_id:provider.task_id.clone(),currency:provider.currency.clone(),reservation:provider.max_cost_micros,task_limit:provider.task_limit_micros},policy:policy_record,packets:vec![],state:"reserved".into(),failure:None,graph_id:None};
        for input in inputs {let key=cache_key(&input,&job);job.packets.push(Packet{input,cache_key:key,attempt_id:Some("0".repeat(64)),request_id:Some("0".repeat(64)),response_artifact_id:None,origin_job_id:None});}
        job.job_id=job_identity(&job);let mut admitted_cache=BTreeMap::new();let mut misses=Vec::new();
        for index in 0..job.packets.len() {
            d.check()?;let packet=&job.packets[index];request(&pinned,&"0".repeat(64),&packet.input,d)?;
            let raw=read_optional_raw(&store.db,CACHE_KIND,&packet.cache_key,MAX_CACHE)?;
            if raw.is_some(){let r=checked_cache(store,&packet.cache_key,&packet.input,&job,d)?;let p=&mut job.packets[index];p.attempt_id=None;p.request_id=None;p.response_artifact_id=Some(r.response_id);p.origin_job_id=Some(r.origin_job);}else{misses.push(index);}
            admitted_cache.insert(job.packets[index].cache_key.clone(),raw);
        }
        capacity(store,job.packets.len(),misses.len(),&job.policy,d)?;
        let total=provider.max_cost_micros.checked_mul(misses.len() as u64).context("Meaning total reservation overflow")?;validate_cost(total)?;ensure!(total<=provider.task_limit_micros,"Entire meaning extraction exceeds task cost budget");
        ensure!(read_optional_raw(&store.db,JOB_KIND,&job.job_id,MAX_JOB)?.is_none()&&read_optional_raw(&store.db,GRAPH_KIND,&graph_id(&job),64_000)?.is_none(),"Meaning immutable publication namespace already exists");encode_bounded(&job,MAX_JOB,d)?;
        let ledger=Ledger::new(store,d);
        let tokens=if misses.is_empty(){let tx=Transaction::new_unchecked(&store.db,TransactionBehavior::Immediate)?;insert_record(&tx,JOB_KIND,&job.job_id,&job,MAX_JOB,d)?;d.check()?;tx.commit()?;vec![]}
        else{ledger.reserve_many_registered(&provider.task_id,&vec![provider.max_cost_micros;misses.len()],provider.task_limit_micros,&provider.currency,&provider.model_key()?,|tx,tokens|{
            for(index,token)in misses.iter().zip(tokens){let p=&mut job.packets[*index];p.attempt_id=Some(token.id().to_owned());p.request_id=Some(request_id(token.id(),&job.provider,&p.input));}
            insert_record(tx,JOB_KIND,&job.job_id,&job,MAX_JOB,d)
        })?};
        let result=(||->Result<Value>{
            job.state="running".into();progress(store,&job,d)?;
            for(index,token)in misses.iter().copied().zip(tokens){
                d.check()?;let response=dispatch(store,&job,index,&pinned,token,d)?;let bytes=encode_bounded(&response,MAX_RESPONSE,d)?;let id=store.artifact(&bytes)?;job.packets[index].response_artifact_id=Some(id);progress(store,&job,d)?;ensure!(response.outcome=="complete","Meaning worker failed after durable accounting");
            }
            let resolved=checked_job(store,&job,d)?;let graph_value=assembled(&job,&resolved,d)?;let bytes=encode_bounded(&graph_value,job.policy.max_graph_bytes,d)?;
            // Staging is CAS-only. No complete graph artifact registration is
            // visible until its graph/cache/job transaction commits below.
            let artifact_id=store.blob(&bytes)?;let record=GraphRecord{schema_version:1,graph_id:graph_id(&job),job_id:job.job_id.clone(),workspace_id:job.workspace_id.clone(),snapshot_id:job.snapshot_id.clone(),artifact_id:artifact_id.clone(),total_bytes:bytes.len()};
            let expected=serde_json::to_string(&job)?;let mut completed=job.clone();completed.state="complete".into();completed.graph_id=Some(record.graph_id.clone());
            let tx=Transaction::new_unchecked(&store.db,TransactionBehavior::Immediate)?;
            ensure!(read_optional_raw(&tx,JOB_KIND,&job.job_id,MAX_JOB)?.as_deref()==Some(expected.as_str()),"Meaning job changed before publication");
            for p in &job.packets {ensure!(read_optional_raw(&tx,CACHE_KIND,&p.cache_key,MAX_CACHE)?==admitted_cache[&p.cache_key],"Meaning cache namespace changed before publication");}
            for index in &misses {let r=resolved[*index].as_ref().context("Missing admitted meaning receipt")?;insert_record(&tx,CACHE_KIND,&job.packets[*index].cache_key,&r.cache,MAX_CACHE,d)?;}
            insert_record(&tx,GRAPH_KIND,&record.graph_id,&record,64_000,d)?;
            insert_record(&tx,"artifact",&artifact_id,&json!({"artifactId":artifact_id,"totalBytes":bytes.len()}),64_000,d)?;
            let completed_bytes=encode_bounded(&completed,MAX_JOB,d)?;ensure!(tx.execute("UPDATE records SET value=?3 WHERE kind=?1 AND id=?2 AND value=?4",params![JOB_KIND,job.job_id,std::str::from_utf8(&completed_bytes)?,expected])?==1,"Meaning completion precondition changed");d.check()?;tx.commit()?;job=completed;
            status_impl(store,&job,None,d)
        })();
        match result {
            Ok(value)=>Ok(value),
            Err(error) if job.state=="complete"=>Err(error.context(format!(
                "Meaning publication committed: job {}, graph {}; status unavailable; inspect the durable result with a fresh deadline",
                job.job_id,graph_id(&job)
            ))),
            Err(error)=>{
                cleanup(store,&job,d);
                job.state="failed".into();
                job.failure=Some("meaning_incomplete_or_publication_failed".into());
                let _=progress(store,&job,d);
                Err(error.context(format!("Meaning job {} incomplete; inspect durable accounting; retries are explicit new builds",job.job_id)))
            }
        }
    })
}
fn load_impl(
    store: &Store,
    id: &str,
    d: &Deadline,
) -> Result<(GraphRecord, Job, Vec<Option<Resolved>>, Value)> {
    ensure!(valid_id(id), "Invalid meaning graph ID");
    store.set_deadline(d)?;
    let record: GraphRecord = read_record(store, GRAPH_KIND, id, 64_000)?;
    ensure!(
        record.schema_version == 1
            && record.graph_id == id
            && record.workspace_id == store.workspace_id
            && valid_id(&record.artifact_id)
            && record.total_bytes <= MAX_GRAPH,
        "Meaning graph registration identity mismatch"
    );
    let job: Job = read_record(store, JOB_KIND, &record.job_id, MAX_JOB)?;
    ensure!(
        job.state == "complete"
            && job.graph_id.as_deref() == Some(id)
            && job.snapshot_id == record.snapshot_id,
        "Meaning graph completed job mismatch"
    );
    let resolved = checked_job(store, &job, d)?;
    let expected = assembled(&job, &resolved, d)?;
    let size = store.artifact_size(&record.artifact_id)?;
    ensure!(
        size == record.total_bytes,
        "Meaning graph registered size mismatch"
    );
    let bytes = store.read_artifact_limited(&record.artifact_id, MAX_GRAPH)?;
    ensure!(
        bytes.len() == record.total_bytes
            && hash(&bytes) == record.artifact_id
            && crate::json::parse(&bytes, MAX_GRAPH)? == expected,
        "Meaning graph artifact differs from source-bound receipts"
    );
    // Complete jobs must still point at every immutable reusable origin cache.
    for p in &job.packets {
        checked_cache(store, &p.cache_key, &p.input, &job, d)?;
    }
    Ok((record, job, resolved, expected))
}
/// Read and replay a published graph without executing or consulting a provider.
pub fn load(store: &Store, id: &str, d: &Deadline) -> Result<Value> {
    Ok(load_impl(store, id, d)?.3)
}
fn status_impl(
    store: &Store,
    job: &Job,
    comparison_snapshot: Option<&str>,
    d: &Deadline,
) -> Result<Value> {
    let resolved = checked_job(store, job, d)?;
    if let Some(id) = &job.graph_id {
        let _ = load_impl(store, id, d)?;
    }
    let mut freshness = "current";
    let mut changed = BTreeSet::new();
    let mut missing = BTreeSet::new();
    if let Some(id) = comparison_snapshot {
        ensure!(valid_id(id), "Invalid meaning comparison snapshot");
        let comparison = store.snapshot(id)?;
        ensure!(
            comparison.workspace_id == store.workspace_id,
            "Meaning comparison workspace mismatch"
        );
        for p in &job.packets {
            for source in &p.input.sources {
                d.check()?;
                match comparison
                    .files
                    .iter()
                    .find(|f| f.path == source.anchor.path)
                {
                    None => {
                        missing.insert(source.anchor.path.clone());
                    }
                    Some(file) if file.hash != source.anchor.content_hash => {
                        changed.insert(source.anchor.path.clone());
                    }
                    _ => {}
                }
            }
        }
        if !missing.is_empty() {
            freshness = "missing";
        } else if !changed.is_empty() {
            freshness = "changed";
        }
    }
    let mut cost = 0u128;
    let mut usage = [0u128; 3];
    let mut unknown_cost = 0usize;
    let mut unknown_usage = 0usize;
    let mut no_claims = 0usize;
    let mut claims = 0usize;
    let mut original_attempts = Vec::new();
    for r in resolved.iter().flatten() {
        match r.response.actual_cost_micros {
            Some(v) => cost += u128::from(v),
            None => unknown_cost += 1,
        }
        if let Some(u) = &r.response.usage {
            usage[0] += u128::from(u.input_tokens);
            usage[1] += u128::from(u.output_tokens);
            usage[2] += u128::from(u.cached_input_tokens);
        } else {
            unknown_usage += 1
        }
        original_attempts.push(r.response.attempt_id.clone());
        if let Some(f) = &r.response.fragment {
            for unit in &f.coverage {
                if unit.outcome == "no_claims" {
                    no_claims += 1
                } else {
                    claims += 1
                }
            }
        }
    }
    let ledger = Ledger::new(store, d);
    let mut dispatched = 0usize;
    let mut confirmed = 0usize;
    let mut unreleased = Vec::new();
    let mut canceled = Vec::new();
    let mut this_run_cost = 0u128;
    let mut this_run_unknown = 0usize;
    for (index, p) in job.packets.iter().enumerate() {
        let Some(id) = &p.attempt_id else { continue };
        let status = ledger.attempt_status(id)?;
        match status.state {
            AttemptState::Reserved => unreleased.push(id.clone()),
            AttemptState::NotSent => canceled.push(id.clone()),
            AttemptState::Sent | AttemptState::Unknown | AttemptState::Settled => {
                dispatched += 1;
                if let Some(r) = &resolved[index] {
                    confirmed += 1;
                    match r.response.actual_cost_micros {
                        Some(v) => this_run_cost += u128::from(v),
                        None => this_run_unknown += 1,
                    }
                } else {
                    // A worker may fail before a replayable receipt is stored.
                    // Its durable sent/unknown reservation still belongs here.
                    original_attempts.push(id.clone());
                    match status.actual_cost_micros {
                        Some(v) => {
                            cost += u128::from(v);
                            this_run_cost += u128::from(v);
                        }
                        None => {
                            unknown_cost += 1;
                            this_run_unknown += 1;
                        }
                    }
                    if let Some(u) = status.response_usage {
                        usage[0] += u128::from(u.input_tokens);
                        usage[1] += u128::from(u.output_tokens);
                        usage[2] += u128::from(u.cached_input_tokens);
                    } else {
                        unknown_usage += 1;
                    }
                }
            }
        }
    }
    let calls = if job.state == "complete" {
        Some(dispatched)
    } else {
        None
    };
    let hits = job
        .packets
        .iter()
        .filter(|p| p.origin_job_id.is_some())
        .count();
    let exact_usage = json!({"inputTokens":usage[0].to_string(),"outputTokens":usage[1].to_string(),"cachedInputTokens":usage[2].to_string()});
    let known_native_usage = if usage.iter().all(|v| *v <= u128::from(u64::MAX)) {
        json!({"inputTokens":usage[0] as u64,"outputTokens":usage[1] as u64,"cachedInputTokens":usage[2] as u64})
    } else {
        Value::Null
    };
    let mut result = json!({"schemaVersion":1,"jobId":job.job_id,"graphId":job.graph_id,"snapshotId":job.snapshot_id,"status":if job.state=="complete"{"ready"}else if job.state=="failed"{"failed"}else{"pending"},"availability":if job.state=="complete"{"ready"}else if job.state=="failed"{"failed"}else{"pending"},"state":job.state,"freshness":freshness,"comparisonSnapshotId":comparison_snapshot,"changedPaths":changed,"missingPaths":missing,"modelCallsThisRun":calls,"dispatchedAttempts":dispatched,"confirmedResponseCount":confirmed,"unreleasedReservations":unreleased,"canceledBeforeSend":canceled,"providerInternalCalls":"unverified","cacheHits":hits,"attemptIds":original_attempts,"accountingScope":"original_receipts_referenced_by_graph","knownReportedCostMicros":u64::try_from(cost).ok(),"knownReportedCostMicrosExact":cost.to_string(),"providerReportedCostMicros":Value::Null,"providerReportedCostMicrosExact":Value::Null,"unknownCosts":unknown_cost,"providerReportedUsage":Value::Null,"providerReportedUsageExact":Value::Null,"knownReportedUsage":known_native_usage,"knownReportedUsageExact":exact_usage,"unknownUsage":unknown_usage,"thisRunKnownReportedCostMicrosExact":this_run_cost.to_string(),"thisRunUnknownCosts":this_run_unknown,"thisRunProviderReportedCostMicros":Value::Null,"coverage":{"claims":claims,"noClaims":no_claims,"semanticExhaustive":false},"resolution":"inferred_concept","providerAttestation":"unverified","automaticRetry":false});
    result["currency"] = json!(job.provider.currency);
    if unknown_cost == 0 {
        result["providerReportedCostMicros"] = json!(u64::try_from(cost).ok());
        result["providerReportedCostMicrosExact"] = json!(cost.to_string());
    }
    if unknown_usage == 0 {
        result["providerReportedUsage"] = result["knownReportedUsage"].clone();
        result["providerReportedUsageExact"] = result["knownReportedUsageExact"].clone();
    }
    if this_run_unknown == 0 {
        result["thisRunProviderReportedCostMicros"] = json!(u64::try_from(this_run_cost).ok());
    }
    Ok(result)
}
pub fn status(
    store: &Store,
    job_id: &str,
    comparison_snapshot: Option<&str>,
    d: &Deadline,
) -> Result<Value> {
    ensure!(valid_id(job_id), "Invalid meaning job ID");
    store.set_deadline(d)?;
    let job: Job = read_record(store, JOB_KIND, job_id, MAX_JOB)?;
    ensure!(job.job_id == job_id, "Meaning job lookup key mismatch");
    status_impl(store, &job, comparison_snapshot, d)
}
pub fn query(store: &Store, id: &str, term: &str, d: &Deadline) -> Result<Value> {
    crate::model_protocol::bounded_text(term, 1000)?;
    let mut graph = load(store, id, d)?;
    let term = term.to_lowercase();
    let packets = graph["packets"]
        .as_array_mut()
        .context("Meaning graph packets absent")?;
    packets.retain(|p| {
        p["meaning"]["concepts"].as_array().is_some_and(|concepts| {
            concepts.iter().any(|c| {
                ["label", "description"].iter().any(|field| {
                    c[*field]
                        .as_str()
                        .is_some_and(|s| s.to_lowercase().contains(&term))
                }) || c["aliases"].as_array().is_some_and(|aliases| {
                    aliases.iter().any(|alias| {
                        alias
                            .as_str()
                            .is_some_and(|s| s.to_lowercase().contains(&term))
                    })
                })
            })
        })
    });
    graph["query"] = json!(term);
    graph["queryCoverage"] = json!("matched_packets_preserve_native_claims_and_support");
    Ok(graph)
}
#[derive(Clone, Debug, Default)]
pub struct MeaningDependencies {
    pub snapshots: Vec<String>,
    pub artifacts: Vec<String>,
    pub records: Vec<(String, String)>,
}

// The view/archive boundary admits one complete immutable dependency closure.
// These internal proof bytes cannot authorize provider or filesystem execution.
const VIEW_INPUT_BYTES: usize = 32_000_000;
const VIEW_KEYS: usize = 4096;
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct FrozenKey {
    table: String,
    kind: String,
    id: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct FrozenRow {
    key: FrozenKey,
    payload: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct FrozenArtifact {
    artifact_id: String,
    total_bytes: usize,
    payload: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct MeaningArchiveProof {
    schema_version: u8,
    selected_graph_ids: Vec<String>,
    rows: Vec<FrozenRow>,
    artifacts: Vec<FrozenArtifact>,
    source_manifests: Vec<Snapshot>,
}
impl MeaningArchiveProof {
    pub(crate) fn source_scopes(&self) -> impl Iterator<Item = &Snapshot> {
        self.source_manifests.iter()
    }
}
#[derive(Clone, Debug)]
pub(crate) struct MeaningSourceScope {
    pub snapshot: Snapshot,
    pub canonical_graph: Graph,
    pub capture_scope: Value,
}
#[derive(Clone, Debug)]
pub(crate) struct CheckedPublishedGraph {
    pub graph_id: String,
    pub job_id: String,
    pub artifact_id: String,
    pub total_bytes: usize,
    pub workspace_id: String,
    pub snapshot_id: String,
    pub base_graph_hash: String,
    pub model: String,
    pub model_version: String,
    pub analyzer_epoch: String,
    pub capture_policy_hash: String,
    pub value: Value,
}
pub(crate) struct CheckedPublishedSet {
    pub graphs: Vec<CheckedPublishedGraph>,
    pub dependencies: MeaningDependencies,
    pub proof: MeaningArchiveProof,
    pub admitted_bytes: usize,
    source_paths: BTreeMap<(String, String), String>,
    source_texts: BTreeMap<String, String>,
}
impl CheckedPublishedSet {
    pub(crate) fn graphs(&self) -> &[CheckedPublishedGraph] {
        &self.graphs
    }
    pub(crate) fn dependencies(&self) -> &MeaningDependencies {
        &self.dependencies
    }
    pub(crate) fn archive(&self) -> &MeaningArchiveProof {
        &self.proof
    }
    pub(crate) fn admitted_bytes(&self) -> usize {
        self.admitted_bytes
    }
    pub(crate) fn source(&self, snapshot_id: &str, path: &str) -> Result<&str> {
        let hash = self
            .source_paths
            .get(&(snapshot_id.to_owned(), path.to_owned()))
            .context("Meaning source path outside frozen scope")?;
        Ok(self
            .source_texts
            .get(hash)
            .context("Frozen meaning source bytes absent")?)
    }
}
fn record_key(kind: &str, id: &str) -> FrozenKey {
    FrozenKey {
        table: "records".into(),
        kind: kind.into(),
        id: id.into(),
    }
}
fn table_key(table: &str, id: &str) -> FrozenKey {
    FrozenKey {
        table: table.into(),
        kind: String::new(),
        id: id.into(),
    }
}
fn row_limit(key: &FrozenKey) -> Result<usize> {
    ensure!(
        valid_id(&key.id),
        "Invalid frozen meaning dependency identity"
    );
    match (key.table.as_str(), key.kind.as_str()) {
        ("snapshots" | "graphs", "") => Ok(VIEW_INPUT_BYTES),
        ("records", JOB_KIND) => Ok(MAX_JOB),
        ("records", CACHE_KIND) => Ok(MAX_CACHE),
        (
            "records",
            GRAPH_KIND | "artifact" | "native_model_attempt_v1" | "native_model_task_v1",
        ) => Ok(64_000),
        ("records", kind)
            if [
                crate::capture::OPTIONS_RECORD_KIND,
                crate::capture::IGNORED_RECORD_KIND,
                crate::capture::DERIVED_OPTIONS_RECORD_KIND,
                crate::git_workflows::SOURCE_KIND,
                "overlay-snapshot",
            ]
            .contains(&kind) =>
        {
            Ok(16_000_000)
        }
        _ => anyhow::bail!("Unknown frozen meaning dependency kind"),
    }
}
fn selected_ids_checked(ids: &[String]) -> Result<Vec<String>> {
    ensure!(
        !ids.is_empty() && ids.len() <= 29,
        "Meaning selection capacity exceeded"
    );
    let selected: BTreeSet<_> = ids.iter().cloned().collect();
    ensure!(
        selected.len() == ids.len() && selected.iter().all(|id| valid_id(id)),
        "Invalid or duplicate published meaning selection"
    );
    Ok(selected.into_iter().collect())
}
fn raw_db_row(store: &Store, key: &FrozenKey, maximum: usize) -> Result<Option<String>> {
    row_limit(key)?;
    let size: Option<usize> = match key.table.as_str() {
        "records" => store
            .db
            .query_row(
                "SELECT length(CAST(value AS BLOB)) FROM records WHERE kind=?1 AND id=?2",
                params![key.kind, key.id],
                |r| r.get(0),
            )
            .optional()?,
        "snapshots" => store
            .db
            .query_row(
                "SELECT length(CAST(value AS BLOB)) FROM snapshots WHERE id=?1",
                [&key.id],
                |r| r.get(0),
            )
            .optional()?,
        "graphs" => store
            .db
            .query_row(
                "SELECT length(CAST(value AS BLOB)) FROM graphs WHERE id=?1",
                [&key.id],
                |r| r.get(0),
            )
            .optional()?,
        _ => anyhow::bail!("Unknown frozen meaning table"),
    };
    let Some(size) = size else { return Ok(None) };
    ensure!(
        size <= maximum,
        "Meaning dependency exceeds remaining input capacity"
    );
    let raw: String = match key.table.as_str() {
        "records" => store.db.query_row(
            "SELECT value FROM records WHERE kind=?1 AND id=?2 AND length(CAST(value AS BLOB))<=?3",
            params![key.kind, key.id, maximum],
            |r| r.get(0),
        )?,
        "snapshots" => store.db.query_row(
            "SELECT value FROM snapshots WHERE id=?1 AND length(CAST(value AS BLOB))<=?2",
            params![key.id, maximum],
            |r| r.get(0),
        )?,
        "graphs" => store.db.query_row(
            "SELECT value FROM graphs WHERE id=?1 AND length(CAST(value AS BLOB))<=?2",
            params![key.id, maximum],
            |r| r.get(0),
        )?,
        _ => unreachable!(),
    };
    ensure!(raw.len() == size, "Frozen meaning row changed during read");
    Ok(Some(raw))
}
struct FrozenAdmission {
    rows: BTreeMap<FrozenKey, String>,
    sizes: BTreeMap<String, usize>,
    artifact_ids: BTreeSet<String>,
    snapshots: BTreeMap<String, Snapshot>,
    used: usize,
    source_bytes: usize,
    source_hashes: BTreeSet<String>,
    key_count: usize,
}
impl FrozenAdmission {
    fn charge(&mut self, size: usize, limit: usize) -> Result<()> {
        self.used = self
            .used
            .checked_add(size)
            .context("Meaning input byte overflow")?;
        ensure!(
            self.used <= limit,
            "Meaning dependency closure exceeds remaining input capacity"
        );
        Ok(())
    }
    fn key(&mut self) -> Result<()> {
        self.key_count += 1;
        ensure!(
            self.key_count <= VIEW_KEYS,
            "Meaning dependency closure key capacity exceeded"
        );
        Ok(())
    }
    fn cas(
        &mut self,
        id: &str,
        size: usize,
        source: bool,
        limit: usize,
        max_sources: usize,
    ) -> Result<()> {
        ensure!(
            valid_id(id) && size <= VIEW_INPUT_BYTES,
            "Invalid meaning CAS dependency"
        );
        if let Some(prior) = self.sizes.get(id) {
            ensure!(*prior == size, "Meaning CAS dependency size conflict");
        } else {
            self.key()?;
            self.charge(size, limit)?;
            self.sizes.insert(id.into(), size);
        }
        if source && self.source_hashes.insert(id.into()) {
            self.source_bytes = self
                .source_bytes
                .checked_add(size)
                .context("Meaning archive source byte overflow")?;
            ensure!(
                self.source_bytes <= max_sources,
                "Meaning archive source capacity exceeded"
            );
        }
        Ok(())
    }
    fn value(&self, key: &FrozenKey) -> Result<Value> {
        let raw = self
            .rows
            .get(key)
            .context("Frozen meaning dependency missing")?;
        crate::json::parse(raw.as_bytes(), row_limit(key)?)
    }
    fn record<T: DeserializeOwned>(&self, kind: &str, id: &str) -> Result<T> {
        crate::json::decode(self.value(&record_key(kind, id))?)
    }
}
fn discover_frozen(
    ids: &[String],
    workspace: &str,
    remaining: usize,
    max_sources: usize,
    fetch: &mut dyn FnMut(&FrozenKey, usize) -> Result<Option<String>>,
    d: &Deadline,
) -> Result<FrozenAdmission> {
    ensure!(
        remaining <= VIEW_INPUT_BYTES && max_sources <= 64_000_000,
        "Invalid meaning closure budgets"
    );
    let mut a = FrozenAdmission {
        rows: BTreeMap::new(),
        sizes: BTreeMap::new(),
        artifact_ids: BTreeSet::new(),
        snapshots: BTreeMap::new(),
        used: 0,
        source_bytes: 0,
        source_hashes: BTreeSet::new(),
        key_count: 0,
    };
    let mut pending = ids
        .iter()
        .map(|id| (record_key(GRAPH_KIND, id), true))
        .collect::<VecDeque<_>>();
    let mut absent = BTreeSet::new();
    while let Some((key, required)) = pending.pop_front() {
        d.check()?;
        if a.rows.contains_key(&key) {
            continue;
        }
        if absent.contains(&key) {
            ensure!(!required, "Required meaning dependency missing");
            continue;
        }
        let maximum = row_limit(&key)?.min(remaining.saturating_sub(a.used));
        let raw = fetch(&key, maximum)?;
        let Some(raw) = raw else {
            ensure!(!required, "Required meaning dependency missing");
            absent.insert(key);
            continue;
        };
        ensure!(
            raw.len() <= maximum,
            "Meaning dependency exceeds admitted input bytes"
        );
        a.key()?;
        // Mutable ledger reports may grow after publication. Reserve their full
        // bounded row capacity so a sealed layer keeps an immutable admission.
        let admitted_row_bytes = if key.table == "records"
            && matches!(
                key.kind.as_str(),
                "native_model_attempt_v1" | "native_model_task_v1"
            ) {
            64_000
        } else {
            raw.len()
        };
        a.charge(admitted_row_bytes, remaining)?;
        // Canonical graph rows are opaque until every reachable size is admitted.
        if key.table == "graphs" {
            a.rows.insert(key, raw);
            continue;
        }
        let value = crate::json::parse(raw.as_bytes(), maximum)?;
        a.rows.insert(key.clone(), raw);
        if key.table == "snapshots" {
            let snapshot: Snapshot = crate::json::decode(value.clone())?;
            ensure!(
                serde_json::to_value(&snapshot)? == value
                    && snapshot.id == key.id
                    && snapshot.workspace_id == workspace
                    && snapshot.files.len() <= 100_000,
                "Frozen meaning snapshot scope/capacity mismatch"
            );
            let mut paths = BTreeSet::new();
            for file in &snapshot.files {
                d.check()?;
                ensure!(
                    file.size <= 4_000_000
                        && paths.insert(&file.path)
                        && crate::paths::normalized_syntax(&file.path)? == file.path,
                    "Invalid frozen meaning source manifest"
                );
                a.cas(&file.hash, file.size, true, remaining, max_sources)?;
            }
            pending.push_back((table_key("graphs", &snapshot.id), true));
            match snapshot.kind.as_str() {
                "disk" => {
                    pending.push_back((
                        record_key(crate::capture::IGNORED_RECORD_KIND, &snapshot.id),
                        false,
                    ));
                    pending.push_back((
                        record_key(crate::capture::OPTIONS_RECORD_KIND, &snapshot.id),
                        false,
                    ));
                }
                "derived" => {
                    pending.push_back((
                        record_key(crate::git_workflows::SOURCE_KIND, &snapshot.id),
                        false,
                    ));
                    pending.push_back((
                        record_key(crate::capture::DERIVED_OPTIONS_RECORD_KIND, &snapshot.id),
                        true,
                    ));
                }
                "overlay" => {
                    pending.push_back((record_key("overlay-snapshot", &snapshot.id), true))
                }
                _ => anyhow::bail!("Meaning source has no supported native capture lineage"),
            }
            a.snapshots.insert(snapshot.id.clone(), snapshot);
            continue;
        }
        match key.kind.as_str() {
            GRAPH_KIND => {
                let graph: GraphRecord = crate::json::decode(value)?;
                ensure!(
                    graph.graph_id == key.id && graph.workspace_id == workspace,
                    "Selected meaning graph registration scope mismatch"
                );
                pending.push_back((record_key(JOB_KIND, &graph.job_id), true));
                pending.push_back((record_key("artifact", &graph.artifact_id), true));
            }
            JOB_KIND => {
                let job: Job = crate::json::decode(value)?;
                ensure!(
                    job.job_id == key.id
                        && job.workspace_id == workspace
                        && job.state == "complete"
                        && !job.packets.is_empty()
                        && job.packets.len() <= 64,
                    "Incomplete or oversized meaning dependency job"
                );
                pending.push_back((table_key("snapshots", &job.snapshot_id), true));
                pending.push_back((
                    record_key(
                        GRAPH_KIND,
                        job.graph_id
                            .as_deref()
                            .context("Published meaning graph missing")?,
                    ),
                    true,
                ));
                for packet in &job.packets {
                    pending.push_back((record_key(CACHE_KIND, &packet.cache_key), true));
                    pending.push_back((
                        record_key(
                            "artifact",
                            packet
                                .response_artifact_id
                                .as_deref()
                                .context("Published meaning response missing")?,
                        ),
                        true,
                    ));
                    if let Some(origin) = &packet.origin_job_id {
                        pending.push_back((record_key(JOB_KIND, origin), true));
                    }
                    if let Some(attempt) = &packet.attempt_id {
                        pending.push_back((record_key("native_model_attempt_v1", attempt), true));
                        pending.push_back((
                            record_key("native_model_task_v1", &identity(&job.provider.task_id)),
                            true,
                        ));
                    }
                }
            }
            CACHE_KIND => {
                let cache: Cache = crate::json::decode(value)?;
                ensure!(
                    cache.cache_key == key.id && cache.workspace_id == workspace,
                    "Frozen meaning cache scope mismatch"
                );
                pending.push_back((record_key(JOB_KIND, &cache.origin_job_id), true));
                pending.push_back((table_key("snapshots", &cache.origin_snapshot_id), true));
                pending.push_back((record_key("artifact", &cache.response_artifact_id), true));
            }
            "artifact" => {
                #[derive(Deserialize)]
                #[serde(rename_all = "camelCase", deny_unknown_fields)]
                struct Registration {
                    artifact_id: String,
                    total_bytes: usize,
                }
                let registered: Registration = crate::json::decode(value)?;
                ensure!(
                    registered.artifact_id == key.id,
                    "Frozen meaning artifact registration mismatch"
                );
                a.cas(
                    &key.id,
                    registered.total_bytes,
                    false,
                    remaining,
                    max_sources,
                )?;
                a.artifact_ids.insert(key.id);
            }
            "native_model_attempt_v1" => {
                let label = value["task"]
                    .as_str()
                    .context("Frozen meaning attempt task missing")?;
                crate::model_protocol::bounded_text(label, 200)?;
                pending.push_back((record_key("native_model_task_v1", &identity(&label)), true));
            }
            "native_model_task_v1" => {}
            kind if kind == crate::capture::DERIVED_OPTIONS_RECORD_KIND => {
                let origin = value["originSnapshotId"]
                    .as_str()
                    .context("Frozen capture origin missing")?;
                pending.push_back((table_key("snapshots", origin), true));
            }
            "overlay-snapshot" => {
                let origin = value["baseSnapshotId"]
                    .as_str()
                    .context("Frozen overlay origin missing")?;
                pending.push_back((table_key("snapshots", origin), true));
            }
            kind if kind == crate::git_workflows::SOURCE_KIND => {
                let origin = value["originSnapshotId"]
                    .as_str()
                    .context("Frozen Git origin missing")?;
                let commit = value["commitArtifactId"]
                    .as_str()
                    .context("Frozen Git commit proof missing")?;
                pending.push_back((table_key("snapshots", origin), true));
                pending.push_back((record_key("artifact", commit), true));
            }
            kind if [
                crate::capture::OPTIONS_RECORD_KIND,
                crate::capture::IGNORED_RECORD_KIND,
            ]
            .contains(&kind) => {}
            _ => anyhow::bail!("Unknown meaning dependency schema"),
        }
        ensure!(
            pending.len() <= VIEW_KEYS * 64,
            "Meaning dependency fanout capacity exceeded"
        );
    }
    Ok(a)
}
fn text_at<'a>(
    texts: &'a BTreeMap<String, String>,
    file: &crate::model::SourceFile,
) -> Result<&'a str> {
    let text = texts
        .get(&file.hash)
        .context("Frozen source bytes absent")?;
    ensure!(
        text.len() == file.size,
        "Frozen source size differs from manifest"
    );
    Ok(text)
}
fn serialized_bytes<T: Serialize + ?Sized>(
    value: &T,
    maximum: usize,
    d: &Deadline,
) -> Result<usize> {
    struct Counter<'a> {
        used: usize,
        maximum: usize,
        d: &'a Deadline,
    }
    impl std::io::Write for Counter<'_> {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.d
                .check()
                .map_err(|_| std::io::Error::other("Meaning serialization deadline exceeded"))?;
            self.used = self
                .used
                .checked_add(bytes.len())
                .ok_or_else(|| std::io::Error::other("Meaning serialization size overflow"))?;
            if self.used > self.maximum {
                return Err(std::io::Error::other(
                    "Meaning derived proof capacity exceeded",
                ));
            }
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut counter = Counter {
        used: 0,
        maximum,
        d,
    };
    serde_json::to_writer(&mut counter, value)
        .context("Meaning proof serialization exceeds capacity")?;
    Ok(counter.used)
}
fn frozen_capture_scope(
    a: &FrozenAdmission,
    texts: &BTreeMap<String, String>,
    id: &str,
    derived: &mut usize,
    d: &Deadline,
) -> Result<Value> {
    let mut current = id.to_owned();
    let mut visited = BTreeSet::new();
    let mut nodes = Vec::new();
    let mut estimate = 1024usize;
    loop {
        d.check()?;
        ensure!(
            nodes.len() < 16 && visited.insert(current.clone()),
            "Frozen capture lineage exceeds capacity or cycles"
        );
        let snapshot = a
            .snapshots
            .get(&current)
            .context("Frozen capture origin manifest absent")?;
        estimate = estimate
            .checked_add(serialized_bytes(snapshot, VIEW_INPUT_BYTES, d)? + 512)
            .context("Meaning capture proof size overflow")?;
        let (proof, origin) = match snapshot.kind.as_str() {
            "disk" => {
                if a.rows
                    .contains_key(&record_key(crate::capture::IGNORED_RECORD_KIND, &current))
                {
                    let raw =
                        a.value(&record_key(crate::capture::IGNORED_RECORD_KIND, &current))?;
                    estimate = estimate
                        .checked_add(serialized_bytes(&raw, VIEW_INPUT_BYTES, d)?)
                        .context("Meaning capture proof size overflow")?;
                    (json!({"kind":"ignoredNative","record":raw}), None)
                } else {
                    let raw = a
                        .rows
                        .get(&record_key(crate::capture::OPTIONS_RECORD_KIND, &current))
                        .map(|r| crate::json::parse(r.as_bytes(), 16_000_000))
                        .transpose()?;
                    estimate = estimate
                        .checked_add(serialized_bytes(&raw, VIEW_INPUT_BYTES, d)?)
                        .context("Meaning capture proof size overflow")?;
                    (json!({"kind":"native","options":raw}), None)
                }
            }
            "derived"
                if a.rows
                    .contains_key(&record_key(crate::git_workflows::SOURCE_KIND, &current)) =>
            {
                let receipt = a.value(&record_key(crate::git_workflows::SOURCE_KIND, &current))?;
                let inheritance = a.value(&record_key(
                    crate::capture::DERIVED_OPTIONS_RECORD_KIND,
                    &current,
                ))?;
                let origin = receipt["originSnapshotId"]
                    .as_str()
                    .context("Frozen Git origin absent")?
                    .to_owned();
                let commit_id = receipt["commitArtifactId"]
                    .as_str()
                    .context("Frozen Git commit absent")?;
                let commit = texts
                    .get(commit_id)
                    .context("Frozen Git commit bytes absent")?;
                let sources = snapshot
                    .files
                    .iter()
                    .map(|file| Ok((file.path.as_str(), text_at(texts, file)?)))
                    .collect::<Result<BTreeMap<_, _>>>()?;
                let added = serialized_bytes(&receipt, VIEW_INPUT_BYTES, d)?
                    + serialized_bytes(&inheritance, VIEW_INPUT_BYTES, d)?
                    + serialized_bytes(commit, VIEW_INPUT_BYTES, d)?
                    + serialized_bytes(&sources, VIEW_INPUT_BYTES, d)?;
                estimate = estimate
                    .checked_add(added + 512)
                    .context("Meaning capture proof size overflow")?;
                ensure!(
                    estimate <= VIEW_INPUT_BYTES.saturating_sub(*derived),
                    "Meaning capture proof aggregate capacity exceeded"
                );
                (
                    json!({"kind":"git","record":{"receipt":receipt,"inheritance":inheritance,"commitText":commit,"sources":sources}}),
                    Some(origin),
                )
            }
            "derived" => {
                let raw = a.value(&record_key(
                    crate::capture::DERIVED_OPTIONS_RECORD_KIND,
                    &current,
                ))?;
                let origin = raw["originSnapshotId"]
                    .as_str()
                    .context("Frozen derived origin absent")?
                    .to_owned();
                estimate = estimate
                    .checked_add(serialized_bytes(&raw, VIEW_INPUT_BYTES, d)?)
                    .context("Meaning capture proof size overflow")?;
                (json!({"kind":"derived","record":raw}), Some(origin))
            }
            "overlay" => {
                let raw = a.value(&record_key("overlay-snapshot", &current))?;
                let origin = raw["baseSnapshotId"]
                    .as_str()
                    .context("Frozen overlay origin absent")?
                    .to_owned();
                estimate = estimate
                    .checked_add(serialized_bytes(&raw, VIEW_INPUT_BYTES, d)?)
                    .context("Meaning capture proof size overflow")?;
                (json!({"kind":"overlay","record":raw}), Some(origin))
            }
            _ => anyhow::bail!("Unknown frozen capture lineage"),
        };
        ensure!(
            estimate <= VIEW_INPUT_BYTES.saturating_sub(*derived),
            "Meaning capture proof aggregate capacity exceeded"
        );
        nodes.push(json!({"snapshot":snapshot,"proof":proof}));
        if let Some(origin) = origin {
            current = origin
        } else {
            break;
        }
    }
    let value = json!({"schemaVersion":1,"nodes":nodes});
    let count = serialized_bytes(&value, VIEW_INPUT_BYTES.saturating_sub(*derived), d)?;
    *derived += count;
    Ok(value)
}
fn prepared_frozen_inputs(
    snapshot: &Snapshot,
    graph: &Graph,
    p: &PolicyRecord,
    texts: &BTreeMap<String, String>,
    d: &Deadline,
) -> Result<Vec<MeaningInput>> {
    p.validate()?;
    let selected = match &p.selection {
        MeaningSelection::Files(v) | MeaningSelection::Bodies(v) => v,
    };
    let mut units = BTreeMap::new();
    let mut total = 0usize;
    for key in selected {
        d.check()?;
        let (path, start, end) = match &p.selection {
            MeaningSelection::Files(_) => {
                let file = snapshot
                    .files
                    .iter()
                    .find(|f| &f.path == key)
                    .context("Frozen selected source file absent")?;
                (file.path.as_str(), 0, file.size)
            }
            MeaningSelection::Bodies(_) => {
                let entity = graph
                    .entities
                    .iter()
                    .find(|e| &e.id == key && e.kind != "file")
                    .context("Frozen selected source body absent")?;
                ensure!(
                    entity.snapshot_id == snapshot.id,
                    "Frozen source body snapshot mismatch"
                );
                (entity.path.as_str(), entity.start_byte, entity.end_byte)
            }
        };
        ensure!(
            end >= start && end - start <= 16_384,
            "Frozen complete meaning unit exceeds capacity"
        );
        let file = snapshot
            .files
            .iter()
            .find(|f| f.path == path)
            .context("Frozen source file membership absent")?;
        let text = crate::paths::utf8_range(text_at(texts, file)?, start, end)?;
        total = total
            .checked_add(text.len())
            .context("Frozen source byte overflow")?;
        ensure!(
            total <= p.max_total_source_bytes,
            "Frozen meaning source budget exceeded"
        );
        let mut anchor = Binding {
            source_id: String::new(),
            path: path.into(),
            content_hash: file.hash.clone(),
            start_byte: start,
            end_byte: end,
        };
        anchor.source_id = meaning_graph::unit_id(&snapshot.workspace_id, &anchor, text);
        units.insert(
            key.clone(),
            Source {
                anchor,
                text: text.into(),
                omitted_bytes: 0,
            },
        );
    }
    let mut groups = selected.iter().map(|s| vec![s.clone()]).collect::<Vec<_>>();
    groups.extend(p.context_groups.clone());
    ensure!(
        groups.len() <= p.max_packets,
        "Frozen packet capacity exceeded"
    );
    let mut seen = BTreeSet::new();
    let mut inputs = Vec::new();
    for group in groups {
        let sources = group
            .iter()
            .map(|key| {
                units
                    .get(key)
                    .cloned()
                    .context("Frozen context source absent")
            })
            .collect::<Result<Vec<_>>>()?;
        let input = MeaningInput {
            workspace_id: snapshot.workspace_id.clone(),
            unit_set_hash: identity(&serde_json::to_value(&sources)?),
            contract_hash: identity(&(
                &p.contract_hash,
                crate::model_protocol::meaning_contract_fingerprint(),
            )),
            sources,
        };
        meaning_graph::validate_input(&input, d)?;
        ensure!(
            seen.insert(identity(&input)),
            "Duplicate frozen meaning packet"
        );
        inputs.push(input);
    }
    Ok(inputs)
}
fn frozen_job_header(
    job: &Job,
    scope: &MeaningSourceScope,
    texts: &BTreeMap<String, String>,
    d: &Deadline,
) -> Result<()> {
    job.policy.validate()?;
    let p = &job.provider;
    ensure!(
        job.schema_version == 1
            && job.job_id == job_identity(job)
            && valid_id(&job.nonce)
            && job.state == "complete"
            && job.failure.is_none()
            && job.graph_id.as_deref() == Some(graph_id(job).as_str())
            && valid_id(&p.fingerprint),
        "Frozen meaning job identity/state mismatch"
    );
    crate::model_protocol::bounded_text(&p.model_id, 100)?;
    crate::model_protocol::bounded_text(&p.model_version, 100)?;
    crate::model_protocol::bounded_text(&p.task_id, 200)?;
    validate_cost(p.reservation)?;
    validate_cost(p.task_limit)?;
    ensure!(
        !p.model_id.contains('@')
            && !p.model_version.contains('@')
            && p.model == format!("{}@{}", p.model_id, p.model_version)
            && p.currency.len() == 3
            && p.currency.bytes().all(|b| b.is_ascii_uppercase()),
        "Frozen meaning provider namespace mismatch"
    );
    let epoch = crate::capture::archived_analyzer_epoch(&scope.snapshot, &scope.capture_scope, d)?
        .context("Meaning source extraction epoch unavailable")?;
    let options =
        crate::capture::archived_capture_options(&scope.snapshot, &scope.capture_scope, d)?;
    let capture_hash = identity(&(
        &scope.snapshot.capture_consistency,
        &scope.snapshot.kind,
        &scope.snapshot.omissions,
        options,
    ));
    ensure!(
        job.workspace_id == scope.snapshot.workspace_id
            && job.snapshot_id == scope.snapshot.id
            && job.graph_hash == identity(&scope.canonical_graph)
            && job.analyzer_epoch == epoch.producer()
            && job.capture_policy_hash == capture_hash,
        "Frozen meaning source/epoch/capture binding mismatch"
    );
    let inputs = prepared_frozen_inputs(
        &scope.snapshot,
        &scope.canonical_graph,
        &job.policy,
        texts,
        d,
    )?;
    ensure!(
        inputs.len() == job.packets.len(),
        "Frozen meaning packet count mismatch"
    );
    let mut attempts = BTreeSet::new();
    for (packet, input) in job.packets.iter().zip(inputs) {
        d.check()?;
        ensure!(
            packet.input == input && packet.cache_key == cache_key(&input, job),
            "Frozen meaning packet source replay mismatch"
        );
        match (
            &packet.attempt_id,
            &packet.request_id,
            &packet.origin_job_id,
        ) {
            (Some(attempt), Some(request), None) => ensure!(
                valid_id(attempt)
                    && attempts.insert(attempt)
                    && request == &request_id(attempt, &job.provider, &packet.input),
                "Frozen meaning request identity mismatch"
            ),
            (None, None, Some(origin)) => ensure!(
                valid_id(origin) && origin != &job.job_id,
                "Frozen meaning cache origin cycle"
            ),
            _ => anyhow::bail!("Frozen meaning packet authority fields conflict"),
        }
        ensure!(
            packet.response_artifact_id.as_deref().is_some_and(valid_id),
            "Frozen meaning response identity absent"
        );
    }
    Ok(())
}
fn frozen_response(
    a: &FrozenAdmission,
    texts: &BTreeMap<String, String>,
    job: &Job,
    index: usize,
    d: &Deadline,
) -> Result<Response> {
    let packet = job
        .packets
        .get(index)
        .context("Frozen response packet absent")?;
    let id = packet
        .response_artifact_id
        .as_deref()
        .context("Frozen response identity absent")?;
    ensure!(
        a.artifact_ids.contains(id),
        "Frozen response has no artifact registration"
    );
    let raw = texts.get(id).context("Frozen response CAS absent")?;
    ensure!(
        raw.len() <= MAX_RESPONSE,
        "Frozen response exceeds byte capacity"
    );
    let r: Response = crate::json::decode(crate::json::parse(raw.as_bytes(), MAX_RESPONSE)?)?;
    ensure!(
        r.schema_version == 1
            && r.kind == RESPONSE_KIND
            && r.job_id == job.job_id
            && r.packet_index == index
            && Some(r.attempt_id.as_str()) == packet.attempt_id.as_deref()
            && Some(r.request_id.as_str()) == packet.request_id.as_deref()
            && r.request_id == request_id(&r.attempt_id, &job.provider, &packet.input)
            && r.input_hash == identity(&serde_json::to_value(&packet.input)?)
            && r.model == job.provider.model_id
            && r.model_version == job.provider.model_version
            && r.source_bindings == bindings(&packet.input)
            && r.currency == job.provider.currency
            && r.outcome == "complete",
        "Frozen meaning response binding/outcome mismatch"
    );
    let task_id = identity(&job.provider.task_id);
    let attempt = a.value(&record_key("native_model_attempt_v1", &r.attempt_id))?;
    let task = a.value(&record_key("native_model_task_v1", &task_id))?;
    let status =
        crate::ledger::checked_archived_attempt(&r.attempt_id, &attempt, &task_id, &task, d)?;
    ensure!(
        status.task_id == job.provider.task_id
            && status.provider == job.provider.model
            && status.currency == r.currency
            && status.reserved_micros == job.provider.reservation
            && matches!(status.state, AttemptState::Unknown | AttemptState::Settled)
            && r.actual_cost_micros
                .is_none_or(|v| status.actual_cost_micros == Some(v))
            && r.usage
                .as_ref()
                .is_none_or(|u| status.response_usage.as_ref() == Some(u))
            && task["limitMicros"].as_u64() == Some(job.provider.task_limit),
        "Frozen meaning response ledger scope mismatch"
    );
    meaning_graph::normalize(
        &packet.input,
        r.fragment
            .as_ref()
            .context("Frozen complete response has no fragment")?,
        d,
    )?;
    Ok(r)
}
fn finish_frozen(
    a: FrozenAdmission,
    texts: BTreeMap<String, String>,
    proof: MeaningArchiveProof,
    snapshot: &Snapshot,
    base: &Graph,
    d: &Deadline,
) -> Result<CheckedPublishedSet> {
    d.check()?;
    let mut paths = BTreeMap::new();
    let mut scopes = BTreeMap::new();
    let mut derived = 0usize;
    for (id, source) in &a.snapshots {
        d.check()?;
        let raw = a
            .rows
            .get(&table_key("graphs", id))
            .context("Frozen canonical graph absent")?;
        let value = crate::json::parse(raw.as_bytes(), VIEW_INPUT_BYTES)?;
        let canonical: Graph = crate::json::decode(value.clone())?;
        ensure!(
            serde_json::to_value(&canonical)? == value && canonical.snapshot_id == *id,
            "Frozen canonical graph schema/snapshot mismatch"
        );
        let capture = frozen_capture_scope(&a, &texts, id, &mut derived, d)?;
        let epoch = crate::capture::archived_analyzer_epoch(source, &capture, d)?
            .context("Frozen source extractor is unavailable")?;
        crate::capture::archived_capture_options(source, &capture, d)?;
        let mut extracted = Vec::new();
        for file in &source.files {
            d.check()?;
            let text = text_at(&texts, file)?;
            paths.insert((id.clone(), file.path.clone()), file.hash.clone());
            extracted.push(crate::analysis::extract_at(epoch, file, text, d)?);
        }
        let expected = crate::analysis::assemble_at(epoch, source, extracted, d)?;
        ensure!(
            identity(&expected) == identity(&canonical),
            "Frozen canonical graph differs from exact captured bytes"
        );
        scopes.insert(
            id.clone(),
            MeaningSourceScope {
                snapshot: source.clone(),
                canonical_graph: canonical,
                capture_scope: capture,
            },
        );
    }
    let current = scopes
        .get(&snapshot.id)
        .context("Selected meaning source snapshot differs from current view")?;
    ensure!(
        identity(&current.snapshot) == identity(snapshot)
            && base.snapshot_id == snapshot.id
            && identity(&current.canonical_graph) == identity(base),
        "Selected meaning canonical source scope mismatch"
    );
    let mut jobs = BTreeMap::new();
    let mut caches = BTreeMap::new();
    let mut registrations = BTreeMap::new();
    for key in a.rows.keys() {
        d.check()?;
        if key.table != "records" {
            continue;
        }
        match key.kind.as_str() {
            JOB_KIND => {
                let job: Job = a.record(JOB_KIND, &key.id)?;
                ensure!(job.job_id == key.id, "Frozen meaning job key mismatch");
                let source = scopes
                    .get(&job.snapshot_id)
                    .context("Frozen meaning job source scope absent")?;
                frozen_job_header(&job, source, &texts, d)?;
                jobs.insert(key.id.clone(), job);
            }
            CACHE_KIND => {
                let cache: Cache = a.record(CACHE_KIND, &key.id)?;
                ensure!(
                    cache.schema_version == 1 && cache.cache_key == key.id,
                    "Frozen meaning cache identity mismatch"
                );
                caches.insert(key.id.clone(), cache);
            }
            GRAPH_KIND => {
                let registered: GraphRecord = a.record(GRAPH_KIND, &key.id)?;
                ensure!(
                    registered.schema_version == 1
                        && registered.graph_id == key.id
                        && registered.workspace_id == snapshot.workspace_id
                        && registered.total_bytes <= MAX_GRAPH
                        && a.artifact_ids.contains(&registered.artifact_id)
                        && a.sizes.get(&registered.artifact_id) == Some(&registered.total_bytes),
                    "Frozen meaning graph registration mismatch"
                );
                registrations.insert(key.id.clone(), registered);
            }
            _ => {}
        }
    }
    let mut received = BTreeMap::new();
    for job in jobs.values() {
        for (index, packet) in job.packets.iter().enumerate() {
            if packet.attempt_id.is_some() {
                received.insert(
                    (job.job_id.clone(), index),
                    frozen_response(&a, &texts, job, index, d)?,
                );
            }
        }
    }
    for cache in caches.values() {
        d.check()?;
        let origin = jobs
            .get(&cache.origin_job_id)
            .context("Frozen meaning cache original job absent")?;
        let packet = origin
            .packets
            .get(cache.packet_index)
            .context("Frozen meaning cache original packet absent")?;
        ensure!(
            packet.attempt_id.is_some()
                && packet.origin_job_id.is_none()
                && packet.response_artifact_id.as_deref()
                    == Some(cache.response_artifact_id.as_str())
                && packet.cache_key == cache.cache_key
                && packet.input == cache.input
                && identity(&make_cache(
                    origin,
                    cache.packet_index,
                    &cache.response_artifact_id
                )) == identity(cache)
                && received.contains_key(&(origin.job_id.clone(), cache.packet_index)),
            "Frozen meaning cache original response metadata mismatch"
        );
    }
    let mut graph_values = BTreeMap::new();
    for job in jobs.values() {
        let mut resolved = Vec::new();
        for (index, packet) in job.packets.iter().enumerate() {
            d.check()?;
            let cache = caches
                .get(&packet.cache_key)
                .context("Frozen complete packet cache absent")?;
            ensure!(
                packet.cache_key == cache_key(&packet.input, job)
                    && cache.workspace_id == job.workspace_id
                    && cache.input == packet.input
                    && cache.model == job.provider.model
                    && cache.provider_fingerprint == job.provider.fingerprint
                    && cache.analyzer_epoch == job.analyzer_epoch
                    && cache.capture_policy_hash == job.capture_policy_hash
                    && cache.generation == job.policy.generation,
                "Frozen cache/current packet binding mismatch"
            );
            let response = if packet.attempt_id.is_some() {
                ensure!(
                    identity(cache)
                        == identity(&make_cache(
                            job,
                            index,
                            packet
                                .response_artifact_id
                                .as_deref()
                                .context("Frozen packet response absent")?
                        )),
                    "Frozen owned cache metadata mismatch"
                );
                received
                    .get(&(job.job_id.clone(), index))
                    .context("Frozen owned response absent")?
            } else {
                ensure!(
                    packet.origin_job_id.as_deref() == Some(cache.origin_job_id.as_str())
                        && packet.response_artifact_id.as_deref()
                            == Some(cache.response_artifact_id.as_str()),
                    "Frozen cached packet original job/response mismatch"
                );
                received
                    .get(&(cache.origin_job_id.clone(), cache.packet_index))
                    .context("Frozen cached original response absent")?
            };
            resolved.push(Some(Resolved {
                response_id: cache.response_artifact_id.clone(),
                response: response.clone(),
                origin_snapshot: cache.origin_snapshot_id.clone(),
                origin_job: cache.origin_job_id.clone(),
                cache: cache.clone(),
            }));
        }
        let id = graph_id(job);
        let registration = registrations
            .get(&id)
            .context("Frozen complete job graph registration absent")?;
        ensure!(
            registration.job_id == job.job_id
                && registration.snapshot_id == job.snapshot_id
                && job.graph_id.as_deref() == Some(id.as_str()),
            "Frozen job/graph publication link mismatch"
        );
        let raw = texts
            .get(&registration.artifact_id)
            .context("Frozen graph CAS absent")?;
        let actual = crate::json::parse(raw.as_bytes(), MAX_GRAPH)?;
        let expected = assembled(job, &resolved, d)?;
        ensure!(
            actual == expected,
            "Frozen published graph differs from original typed receipts"
        );
        graph_values.insert(id, actual);
    }
    let mut published = Vec::new();
    for id in &proof.selected_graph_ids {
        let registration = registrations
            .get(id)
            .context("Frozen selected graph absent")?;
        let job = jobs
            .get(&registration.job_id)
            .context("Frozen selected job absent")?;
        ensure!(
            job.snapshot_id == snapshot.id
                && registration.snapshot_id == snapshot.id
                && job.graph_hash == identity(base)
                && job.workspace_id == snapshot.workspace_id,
            "Published meaning selection is not bound to the exact current source graph"
        );
        published.push(CheckedPublishedGraph {
            graph_id: id.clone(),
            job_id: job.job_id.clone(),
            artifact_id: registration.artifact_id.clone(),
            total_bytes: registration.total_bytes,
            workspace_id: job.workspace_id.clone(),
            snapshot_id: job.snapshot_id.clone(),
            base_graph_hash: job.graph_hash.clone(),
            model: job.provider.model_id.clone(),
            model_version: job.provider.model_version.clone(),
            analyzer_epoch: job.analyzer_epoch.clone(),
            capture_policy_hash: job.capture_policy_hash.clone(),
            value: graph_values
                .remove(id)
                .context("Frozen selected graph materialization absent")?,
        });
    }
    let mut dependencies = MeaningDependencies {
        snapshots: a.snapshots.keys().cloned().collect(),
        artifacts: a.artifact_ids.iter().cloned().collect(),
        records: a
            .rows
            .keys()
            .filter(|k| k.table == "records")
            .map(|k| (k.kind.clone(), k.id.clone()))
            .collect(),
    };
    dependencies.records.sort();
    dependencies.records.dedup();
    Ok(CheckedPublishedSet {
        graphs: published,
        dependencies,
        proof,
        admitted_bytes: a.used,
        source_paths: paths,
        source_texts: texts,
    })
}
/// Admit the complete selected closure before reading any graph/response CAS.
/// A read transaction freezes mutable accounting rows; no provider is resolved.
pub(crate) fn checked_published_set(
    store: &Store,
    snapshot: &Snapshot,
    base: &Graph,
    graph_ids: &[String],
    remaining_input_bytes: usize,
    d: &Deadline,
) -> Result<CheckedPublishedSet> {
    checked_published_set_impl(
        store,
        snapshot,
        base,
        graph_ids,
        ArchivePreparationLimits {
            remaining_input_bytes,
            max_archive_bytes: VIEW_INPUT_BYTES,
            max_source_bytes: 64_000_000,
        },
        None,
        d,
    )
}
fn checked_published_set_impl(
    store: &Store,
    snapshot: &Snapshot,
    base: &Graph,
    graph_ids: &[String],
    limits: ArchivePreparationLimits,
    preflight: Option<&mut FrozenScopePreflight<'_>>,
    d: &Deadline,
) -> Result<CheckedPublishedSet> {
    let ids = selected_ids_checked(graph_ids)?;
    d.check()?;
    store.set_deadline(d)?;
    ensure!(
        snapshot.workspace_id == store.workspace_id && base.snapshot_id == snapshot.id,
        "Meaning view workspace/snapshot mismatch"
    );
    let _read = if store.db.is_autocommit() {
        Some(Transaction::new_unchecked(
            &store.db,
            TransactionBehavior::Deferred,
        )?)
    } else {
        None
    };
    let mut fetch = |key: &FrozenKey, maximum: usize| raw_db_row(store, key, maximum);
    let a = discover_frozen(
        &ids,
        &snapshot.workspace_id,
        limits.remaining_input_bytes,
        limits.max_source_bytes,
        &mut fetch,
        d,
    )?;
    if let Some(preflight) = preflight {
        // A later denied historical scope must prevent every earlier CAS read.
        // The existing frozen read transaction keeps all discovered rows stable.
        for source in a.snapshots.values() {
            d.check()?;
            preflight(source, d)?;
        }
    }
    for id in &a.artifact_ids {
        ensure!(
            store.artifact_size(id)? == a.sizes[id],
            "Meaning artifact registration changed after admission"
        );
    }
    let mut texts = BTreeMap::new();
    for (id, size) in &a.sizes {
        d.check()?;
        let bytes = store.read_blob_exact(id, *size, VIEW_INPUT_BYTES)?;
        texts.insert(
            id.clone(),
            String::from_utf8(bytes).context("Frozen meaning dependency is not UTF-8")?,
        );
    }
    let proof = MeaningArchiveProof {
        schema_version: 1,
        selected_graph_ids: ids,
        rows: a
            .rows
            .iter()
            .map(|(key, payload)| FrozenRow {
                key: key.clone(),
                payload: payload.clone(),
            })
            .collect(),
        artifacts: a
            .artifact_ids
            .iter()
            .map(|id| FrozenArtifact {
                artifact_id: id.clone(),
                total_bytes: a.sizes[id],
                payload: texts[id].clone(),
            })
            .collect(),
        source_manifests: a.snapshots.values().cloned().collect(),
    };
    finish_frozen(a, texts, proof, snapshot, base, d)
}
/// Pure replay uses only the bounded proof and explicitly supplied frozen bytes.
pub(crate) fn replay_published_set_members(
    snapshot: &Snapshot,
    base: &Graph,
    proof: &MeaningArchiveProof,
    read_source: &mut FrozenMemberReader<'_>,
    remaining_input_bytes: usize,
    max_source_bytes: usize,
    d: &Deadline,
) -> Result<CheckedPublishedSet> {
    d.check()?;
    ensure!(
        proof.schema_version == 1
            && proof.rows.len() <= VIEW_KEYS
            && proof.artifacts.len() <= VIEW_KEYS
            && proof.source_manifests.len() <= VIEW_KEYS,
        "Unknown or oversized meaning archive proof"
    );
    let ids = selected_ids_checked(&proof.selected_graph_ids)?;
    ensure!(
        ids == proof.selected_graph_ids,
        "Meaning archive selected order is not canonical"
    );
    let mut rows = BTreeMap::new();
    for row in &proof.rows {
        d.check()?;
        ensure!(
            row.payload.len() <= row_limit(&row.key)?
                && rows.insert(row.key.clone(), row.payload.as_str()).is_none(),
            "Duplicate/oversized meaning archive row"
        );
    }
    let mut fetch = |key: &FrozenKey, maximum: usize| -> Result<Option<String>> {
        let Some(raw) = rows.get(key) else {
            return Ok(None);
        };
        ensure!(
            raw.len() <= maximum,
            "Meaning archive dependency exceeds remaining input capacity"
        );
        Ok(Some((*raw).to_owned()))
    };
    let a = discover_frozen(
        &ids,
        &snapshot.workspace_id,
        remaining_input_bytes,
        max_source_bytes,
        &mut fetch,
        d,
    )?;
    ensure!(
        a.rows.len() == rows.len() && a.rows.keys().all(|k| rows.contains_key(k)),
        "Meaning archive contains unreachable dependencies"
    );
    ensure!(
        proof.source_manifests.len() == a.snapshots.len()
            && proof
                .source_manifests
                .iter()
                .zip(a.snapshots.values())
                .all(|(p, s)| identity(p) == identity(s)),
        "Meaning archive source manifest closure mismatch"
    );
    let mut texts = BTreeMap::new();
    for artifact in &proof.artifacts {
        d.check()?;
        ensure!(
            a.artifact_ids.contains(&artifact.artifact_id)
                && a.sizes.get(&artifact.artifact_id) == Some(&artifact.total_bytes)
                && artifact.payload.len() == artifact.total_bytes
                && hash(artifact.payload.as_bytes()) == artifact.artifact_id
                && texts
                    .insert(artifact.artifact_id.clone(), artifact.payload.clone())
                    .is_none(),
            "Meaning archive artifact identity/size/hash mismatch"
        );
    }
    ensure!(
        texts.len() == a.artifact_ids.len(),
        "Meaning archive registered artifact bytes missing"
    );
    let index = FrozenSourceIndex::new(a.snapshots.values(), max_source_bytes, d)?;
    for source in a.snapshots.values() {
        let bound = index.bind(source, d)?;
        for file in &source.files {
            let member = bound.member(file, d)?;
            d.check()?;
            if !texts.contains_key(&file.hash) {
                let bytes = read_source(&member, d)?;
                ensure!(
                    bytes.len() == file.size && hash(&bytes) == file.hash,
                    "Meaning frozen source callback hash/size mismatch"
                );
                texts.insert(
                    file.hash.clone(),
                    String::from_utf8(bytes)
                        .context("Meaning frozen source callback is not UTF-8")?,
                );
            } else {
                ensure!(
                    texts[&file.hash].len() == file.size,
                    "Meaning reused frozen source size mismatch"
                );
            }
        }
    }
    finish_frozen(a, texts, proof.clone(), snapshot, base, d)
}
pub fn validate_retained_record(
    store: &Store,
    kind: &str,
    id: &str,
    value: &Value,
    d: &Deadline,
) -> Result<MeaningDependencies> {
    ensure!(valid_id(id), "Invalid retained meaning key");
    store.set_deadline(d)?;
    let maximum = match kind {
        GRAPH_KIND => 64_000,
        JOB_KIND => MAX_JOB,
        CACHE_KIND => MAX_CACHE,
        _ => anyhow::bail!("Unknown retained meaning record schema"),
    };
    encode_bounded(value, maximum, d)?;
    let stored = read_optional_raw(&store.db, kind, id, maximum)?
        .context("Retained meaning record missing")?;
    ensure!(
        crate::json::parse(stored.as_bytes(), maximum)? == *value,
        "Retained meaning value differs from immutable registered row"
    );
    let mut deps = MeaningDependencies::default();
    let (job, resolved) = match kind {
        GRAPH_KIND => {
            let (record, job, resolved, _) = load_impl(store, id, d)?;
            ensure!(
                serde_json::to_value(&record)? == *value,
                "Retained meaning graph record changed"
            );
            deps.artifacts.push(record.artifact_id);
            deps.records.push((JOB_KIND.into(), job.job_id.clone()));
            (job, resolved)
        }
        JOB_KIND => {
            let job: Job = crate::json::decode(value.clone())?;
            ensure!(job.job_id == id, "Retained meaning job key mismatch");
            encode_bounded(&job, MAX_JOB, d)?;
            let resolved = checked_job(store, &job, d)?;
            if let Some(graph) = &job.graph_id {
                let (record, _, _, _) = load_impl(store, graph, d)?;
                deps.artifacts.push(record.artifact_id);
                deps.records.push((GRAPH_KIND.into(), graph.clone()));
            }
            (job, resolved)
        }
        CACHE_KIND => {
            let cache: Cache = crate::json::decode(value.clone())?;
            ensure!(cache.cache_key == id, "Retained meaning cache key mismatch");
            let job: Job = read_record(store, JOB_KIND, &cache.origin_job_id, MAX_JOB)?;
            let resolved = checked_cache(store, id, &cache.input, &job, d)?;
            deps.records.push((JOB_KIND.into(), job.job_id.clone()));
            (job, vec![Some(resolved)])
        }
        _ => anyhow::bail!("Unknown retained meaning record schema"),
    };
    deps.snapshots.push(job.snapshot_id.clone());
    for (index, p) in job.packets.iter().enumerate() {
        d.check()?;
        if let Some(attempt) = &p.attempt_id {
            deps.records
                .push(("native_model_attempt_v1".into(), attempt.clone()));
        }
        if job.state == "complete" {
            deps.records.push((CACHE_KIND.into(), p.cache_key.clone()));
        }
        if let Some(Some(r)) = resolved.get(index) {
            deps.snapshots.push(r.origin_snapshot.clone());
            deps.artifacts.push(r.response_id.clone());
            deps.records.push((JOB_KIND.into(), r.origin_job.clone()));
            deps.records.push((
                "native_model_attempt_v1".into(),
                r.response.attempt_id.clone(),
            ));
        }
    }
    // Cache-only validation may originate from a packet other than index zero.
    for r in resolved.iter().flatten() {
        deps.snapshots.push(r.origin_snapshot.clone());
        deps.artifacts.push(r.response_id.clone());
        deps.records.push((JOB_KIND.into(), r.origin_job.clone()));
    }
    deps.snapshots.sort();
    deps.snapshots.dedup();
    deps.artifacts.sort();
    deps.artifacts.dedup();
    deps.records.sort();
    deps.records.dedup();
    Ok(deps)
}
