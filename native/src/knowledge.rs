// Source-bound knowledge and vectors are implemented in the current migration phase.
//! Authored source knowledge and statistical retrieval, never execution authority.
//! These project records are distinct from the coding agents' central AgentMemory.
use crate::{
    capture::Deadline,
    model::{Entity, Graph, Snapshot, identity, valid_id},
    paths::{normalized, obvious_secret, utf8_range},
    store::Store,
};
use anyhow::{Context, Result, ensure};
use rusqlite::{OptionalExtension, TransactionBehavior, params};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use unicode_normalization::UnicodeNormalization;

const MEMORY_KIND: &str = "native_knowledge_v1";
#[path = "knowledge_feedback.rs"]
pub mod feedback;
const VECTOR_KIND: &str = "native_vectors_v1";
const LOCAL_KIND: &str = "native_local_tfidf_v1";
const TRUST: &str = "authored_data_not_execution_authority";
const MAX_RECORD_BYTES: usize = 24 * 1024 * 1024;
const MAX_MEMORY_TOTAL: usize = 32 * 1024 * 1024;
const MAX_VECTOR_TOTAL: usize = 128 * 1024 * 1024;
const MAX_DOCUMENTS: usize = 2_000;
const MAX_TERMS: usize = 20_000;
const MAX_COORDINATES: usize = 200_000;

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum MemoryKind {
    Hypothesis,
    Confirmed,
    DeadEnd,
    Correction,
    Rule,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RememberInput {
    kind: MemoryKind,
    text: String,
    paths: Vec<String>,
    actor: String,
    supersedes: Option<String>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Anchor {
    path: String,
    content_hash: String,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Memory {
    id: String,
    snapshot_id: String,
    workspace_id: String,
    kind: MemoryKind,
    text: String,
    anchors: Vec<Anchor>,
    actor: String,
    created_at_ms: u64,
    created_at_nonce: String,
    supersedes: Option<String>,
}

fn bounded_text(value: &str, max: usize, allow_empty: bool) -> Result<()> {
    ensure!(
        value.len() <= max && (allow_empty || !value.trim().is_empty()),
        "Invalid text size"
    );
    ensure!(
        !value
            .chars()
            .any(|c| c.is_control() && c != '\n' && c != '\r' && c != '\t'),
        "Unsupported control character"
    );
    ensure!(!obvious_secret(value), "Obvious secret content refused");
    Ok(())
}
fn scope(store: &Store, snapshot: &Snapshot, deadline: &Deadline) -> Result<()> {
    deadline.check()?;
    store.set_deadline(deadline)?;
    ensure!(
        snapshot.workspace_id == store.workspace_id && valid_id(&snapshot.id),
        "Foreign workspace or invalid snapshot"
    );
    let saved = store.snapshot(&snapshot.id)?;
    ensure!(
        identity(&saved) == identity(snapshot),
        "Snapshot differs from immutable store"
    );
    Ok(())
}
fn graph_scope(
    store: &Store,
    snapshot: &Snapshot,
    graph: &Graph,
    deadline: &Deadline,
) -> Result<()> {
    scope(store, snapshot, deadline)?;
    ensure!(graph.snapshot_id == snapshot.id, "Graph snapshot mismatch");
    ensure!(
        identity(&store.graph(&snapshot.id)?) == identity(graph),
        "Graph differs from immutable store"
    );
    Ok(())
}
fn database_deadline(store: &Store, deadline: &Deadline) -> Result<()> {
    deadline.check()?;
    store
        .db
        .busy_timeout(deadline.remaining().min(Duration::from_secs(10)))?;
    Ok(())
}
fn record_raw(store: &Store, kind: &str, id: &str) -> Result<Option<String>> {
    let limit = rusqlite::limits::Limit::SQLITE_LIMIT_LENGTH;
    let prior = store
        .db
        .set_limit(limit, (MAX_RECORD_BYTES + 65536) as i32)?;
    let result = (|| {
        let size: Option<usize> = store
            .db
            .query_row(
                "SELECT length(CAST(value AS BLOB)) FROM records WHERE kind=?1 AND id=?2",
                params![kind, id],
                |row| row.get(0),
            )
            .optional()?;
        if let Some(size) = size {
            ensure!(
                size <= MAX_RECORD_BYTES,
                "Persisted record exceeds capacity"
            );
        }
        let raw: Option<String> = store.db.query_row(
            "SELECT value FROM records WHERE kind=?1 AND id=?2 AND length(CAST(value AS BLOB))<=?3",
            params![kind,id,MAX_RECORD_BYTES], |row| row.get(0),
        ).optional()?;
        ensure!(
            size.is_none() == raw.is_none(),
            "Persisted record changed during admission"
        );
        if let Some(raw) = &raw {
            ensure!(
                raw.len() <= MAX_RECORD_BYTES,
                "Persisted record exceeds capacity"
            );
        }
        Ok(raw)
    })();
    store.db.set_limit(limit, prior)?;
    result
}

/// The actor is an authored attribution label, not an authenticated principal.
pub fn remember(
    store: &Store,
    snapshot: &Snapshot,
    input: Value,
    deadline: &Deadline,
) -> Result<Value> {
    scope(store, snapshot, deadline)?;
    ensure!(
        serde_json::to_vec(&input)?.len() <= 64_000,
        "Knowledge input exceeds capacity"
    );
    let input: RememberInput = serde_json::from_value(input)?;
    bounded_text(&input.text, 20_000, false)?;
    bounded_text(&input.actor, 200, false)?;
    ensure!(
        !input.actor.chars().any(char::is_control),
        "Actor must be a single label"
    );
    ensure!(
        !input.paths.is_empty() && input.paths.len() <= 100,
        "Knowledge needs 1–100 source paths"
    );
    let mut seen = BTreeSet::new();
    let mut anchors = Vec::new();
    for path in input.paths {
        deadline.check()?;
        let path = normalized(&path)?;
        ensure!(seen.insert(path.clone()), "Duplicate knowledge path");
        let file = snapshot
            .files
            .iter()
            .find(|file| file.path == path)
            .context("Knowledge source is not in this snapshot")?;
        store.read_blob(&file.hash)?;
        anchors.push(Anchor {
            path,
            content_hash: file.hash.clone(),
        });
    }
    anchors.sort_by(|a, b| a.path.cmp(&b.path));
    database_deadline(store, deadline)?;
    let tx = rusqlite::Transaction::new_unchecked(&store.db, TransactionBehavior::Immediate)?;
    let (count, bytes): (usize, usize) = tx.query_row(
        "SELECT count(*),coalesce(sum(length(CAST(value AS BLOB))),0) FROM records WHERE kind=?1",
        [MEMORY_KIND],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    ensure!(
        count < 10_000 && bytes < MAX_MEMORY_TOTAL,
        "Knowledge capacity reached"
    );
    if let Some(id) = &input.supersedes {
        ensure!(valid_id(id), "Invalid superseded knowledge ID");
        let raw: String = tx
            .query_row(
                "SELECT value FROM records WHERE kind=?1 AND id=?2",
                params![MEMORY_KIND, id],
                |row| row.get(0),
            )
            .context("Superseded knowledge not found")?;
        let old: Memory = serde_json::from_str(&raw)?;
        ensure!(
            old.workspace_id == store.workspace_id
                && anchors
                    .iter()
                    .any(|a| old.anchors.iter().any(|b| a.path == b.path)),
            "Supersession requires overlapping source scope"
        );
        let replaced: usize = tx.query_row(
            "SELECT count(*) FROM records WHERE kind=?1 AND json_extract(value,'$.supersedes')=?2",
            params![MEMORY_KIND, id],
            |row| row.get(0),
        )?;
        ensure!(replaced == 0, "Knowledge is already superseded");
    }
    let now = SystemTime::now().duration_since(UNIX_EPOCH)?;
    let mut memory = Memory {
        id: String::new(),
        snapshot_id: snapshot.id.clone(),
        workspace_id: store.workspace_id.clone(),
        kind: input.kind,
        text: input.text,
        anchors,
        actor: input.actor,
        created_at_ms: u64::try_from(now.as_millis())?,
        created_at_nonce: now.as_nanos().to_string(),
        supersedes: input.supersedes,
    };
    memory.id = identity(&memory);
    let serialized = serde_json::to_string(&memory)?;
    ensure!(
        serialized.len() <= 64_000,
        "Expanded knowledge record exceeds per-record capacity"
    );
    ensure!(
        bytes.saturating_add(serialized.len()) <= MAX_MEMORY_TOTAL,
        "Knowledge capacity reached"
    );
    deadline.check()?;
    tx.execute(
        "INSERT INTO records VALUES(?1,?2,?3)",
        params![MEMORY_KIND, memory.id, serialized],
    )?;
    deadline.check()?;
    tx.commit()?;
    let mut value = serde_json::to_value(memory)?;
    value["trust"] = json!(TRUST);
    Ok(value)
}

pub fn recall(
    store: &Store,
    snapshot: &Snapshot,
    query: &str,
    limit: usize,
    deadline: &Deadline,
) -> Result<Value> {
    scope(store, snapshot, deadline)?;
    bounded_text(query, 4_000, true)?;
    ensure!((1..=100).contains(&limit), "Recall limit must be 1–100");
    let mut statement = store.db.prepare("SELECT value FROM records WHERE kind=?1 ORDER BY json_extract(value,'$.createdAtMs') DESC,id DESC LIMIT 10001")?;
    let rows = statement.query_map([MEMORY_KIND], |row| row.get::<_, String>(0))?;
    let mut memories = Vec::new();
    let mut bytes = 0usize;
    for row in rows {
        deadline.check()?;
        let raw = row?;
        bytes = bytes.saturating_add(raw.len());
        ensure!(
            raw.len() <= 64_000 && bytes <= MAX_MEMORY_TOTAL && memories.len() < 10_000,
            "Knowledge store exceeds capacity"
        );
        let memory: Memory = serde_json::from_str(&raw)?;
        ensure!(
            memory.workspace_id == store.workspace_id
                && valid_id(&memory.id)
                && !memory.anchors.is_empty()
                && memory.anchors.len() <= 100,
            "Corrupt knowledge scope"
        );
        bounded_text(&memory.text, 20_000, false)?;
        memories.push(memory);
    }
    let superseded: BTreeSet<_> = memories
        .iter()
        .filter_map(|m| m.supersedes.as_deref())
        .collect();
    let query_terms = terms(query);
    let current: BTreeMap<_, _> = snapshot
        .files
        .iter()
        .map(|f| (f.path.as_str(), f.hash.as_str()))
        .collect();
    let mut items = Vec::new();
    for memory in &memories {
        deadline.check()?;
        let document_terms = terms(&memory.text);
        let score = query_terms
            .keys()
            .filter(|term| document_terms.contains_key(*term))
            .count();
        if !query.trim().is_empty() && score == 0 {
            continue;
        }
        let anchors: Vec<_> = memory.anchors.iter().map(|anchor| -> Result<Value> {
            normalized(&anchor.path)?;
            ensure!(valid_id(&anchor.content_hash), "Corrupt knowledge source hash");
            let state = match current.get(anchor.path.as_str()) {
                Some(hash) if *hash == anchor.content_hash => "current",
                Some(_) => "changed",
                None if snapshot.omissions.iter().any(|o| o.path == anchor.path) => "excluded",
                None => "deleted",
            };
            Ok(json!({"path":anchor.path,"contentHash":anchor.content_hash,"currentHash":current.get(anchor.path.as_str()),"state":state}))
        }).collect::<Result<_>>()?;
        let stale = anchors.iter().any(|a| a["state"] != "current");
        let mut value = serde_json::to_value(memory)?;
        value["anchors"] = json!(anchors);
        value["stale"] = json!(stale);
        value["superseded"] = json!(superseded.contains(memory.id.as_str()));
        value["score"] = json!(score);
        value["trust"] = json!(TRUST);
        items.push(value);
    }
    items.sort_by(|a, b| {
        b["score"]
            .as_u64()
            .cmp(&a["score"].as_u64())
            .then_with(|| b["createdAtMs"].as_u64().cmp(&a["createdAtMs"].as_u64()))
            .then_with(|| a["id"].as_str().cmp(&b["id"].as_str()))
    });
    let matched = items.len();
    items.truncate(limit);
    Ok(
        json!({"snapshotId":snapshot.id,"items":items,"matched":matched,"truncated":matched>limit,"trust":TRUST,"limitations":["Authored kinds are assertions, not independently verified facts.","Deleted means absent from the selected source snapshot; recorded exclusions remain distinct."]}),
    )
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct DenseItem {
    entity_id: String,
    content_hash: String,
    vector: Vec<f64>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct DenseInput {
    snapshot_id: String,
    model: String,
    items: Vec<DenseItem>,
}
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct DenseCollection {
    schema: u8,
    snapshot_id: String,
    workspace_id: String,
    model: String,
    dimension: usize,
    items: Vec<DenseItem>,
}

/// A source-checked destination plan, never constructible from model JSON.
/// The batch orchestrator holds Store::with_exclusive_client across admission,
/// billed attempts and final publication; no SQLite transaction spans the calls.
pub struct PreparedVectorAdmission<'a> {
    store: &'a Store,
    snapshot: &'a Snapshot,
    graph: &'a Graph,
    model: String,
    ordered: Vec<(String, String)>,
    dimension: usize,
    maximum: usize,
    worst_bytes: usize,
}

/// Exact normalized legacy collection bytes, sealed after source-plan validation.
pub struct PreparedVectorCollection<'a> {
    store: &'a Store,
    id: String,
    serialized: String,
    serialized_hash: String,
    model: String,
    snapshot_id: String,
    dimension: usize,
    count: usize,
}

fn vector_namespace_absent(store: &Store, id: &str) -> Result<()> {
    let exists: bool = store.db.query_row(
        "SELECT EXISTS(SELECT 1 FROM records WHERE kind=?1 AND id=?2)",
        params![VECTOR_KIND, id],
        |row| row.get(0),
    )?;
    ensure!(
        !exists,
        "Vector model namespace already exists; choose a versioned model identifier"
    );
    Ok(())
}

fn vector_capacity(store: &Store, additional: usize, deadline: &Deadline) -> Result<()> {
    database_deadline(store, deadline)?;
    let limit = rusqlite::limits::Limit::SQLITE_LIMIT_LENGTH;
    let prior = store
        .db
        .set_limit(limit, (MAX_RECORD_BYTES + 65536) as i32)?;
    let result = (|| {
        let (count, bytes): (usize,usize) = store.db.query_row(
            "SELECT count(*),coalesce(sum(length(CAST(value AS BLOB))),0) FROM records WHERE kind IN (?1,?2)",
            params![VECTOR_KIND, LOCAL_KIND], |row| Ok((row.get(0)?,row.get(1)?)),
        )?;
        ensure!(
            count < 512
                && bytes
                    .checked_add(additional)
                    .is_some_and(|n| n <= MAX_VECTOR_TOTAL),
            "Vector store capacity reached"
        );
        Ok(())
    })();
    store.db.set_limit(limit, prior)?;
    result
}

impl<'a> PreparedVectorAdmission<'a> {
    #[allow(clippy::too_many_arguments)] // Independent host bounds and immutable source selections.
    pub fn new(
        store: &'a Store,
        snapshot: &'a Snapshot,
        graph: &'a Graph,
        model: &str,
        ordered_ids: &[String],
        dimensions: usize,
        max_serialized_bytes: usize,
        deadline: &Deadline,
    ) -> Result<Self> {
        graph_scope(store, snapshot, graph, deadline)?;
        model_label(model)?;
        ensure!(
            !model.starts_with("yoke-local-tfidf-v1:"),
            "Local vector model namespace is reserved"
        );
        ensure!(
            !ordered_ids.is_empty()
                && ordered_ids.len() <= 10000
                && (1..=4096).contains(&dimensions),
            "Invalid prepared vector count or dimensions"
        );
        ensure!(
            ordered_ids
                .len()
                .checked_mul(dimensions)
                .is_some_and(|n| n <= 2_000_000),
            "Dense vector coordinate capacity exceeded"
        );
        ensure!(
            (1..=MAX_RECORD_BYTES).contains(&max_serialized_bytes),
            "Invalid prepared vector serialized capacity"
        );
        let id = identity(&(snapshot.id.as_str(), model));
        vector_namespace_absent(store, &id)?;
        let mut anchors = AnchorReader::new(store, snapshot, graph)?;
        let files: BTreeMap<_, _> = snapshot
            .files
            .iter()
            .map(|file| (file.path.as_str(), file.hash.as_str()))
            .collect();
        let entities: BTreeMap<_, _> = graph
            .entities
            .iter()
            .map(|entity| (entity.id.as_str(), entity))
            .collect();
        let mut seen = BTreeSet::new();
        let mut ordered = Vec::with_capacity(ordered_ids.len());
        let mut skeleton = Vec::with_capacity(ordered_ids.len());
        for entity_id in ordered_ids {
            deadline.check()?;
            ensure!(
                valid_id(entity_id) && seen.insert(entity_id),
                "Invalid or duplicate prepared vector entity"
            );
            let entity = entities
                .get(entity_id.as_str())
                .context("Prepared vector entity is not in this snapshot")?;
            let hash = *files
                .get(entity.path.as_str())
                .context("Prepared vector source absent")?;
            anchors.entity(entity_id, hash)?;
            ordered.push((entity_id.clone(), hash.into()));
            skeleton.push(DenseItem {
                entity_id: entity_id.clone(),
                content_hash: hash.into(),
                vector: vec![],
            });
        }
        let skeleton = DenseCollection {
            schema: 1,
            snapshot_id: snapshot.id.clone(),
            workspace_id: store.workspace_id.clone(),
            model: model.into(),
            dimension: dimensions,
            items: skeleton,
        };
        let base = bounded_serialized_size(&skeleton, max_serialized_bytes, deadline)?;
        // A finite f64 JSON number needs at most a sign, 17 significant decimal
        // digits, decimal separator, exponent marker/sign and three exponent
        // digits. Reserve 32 bytes plus its separator, conservatively beyond
        // that representation; output values are never assumed to be short.
        let coordinate_bytes = ordered_ids
            .len()
            .checked_mul(dimensions)
            .and_then(|n| n.checked_mul(33))
            .context("Prepared vector byte overflow")?;
        let worst_bytes = base
            .checked_add(coordinate_bytes)
            .context("Prepared vector byte overflow")?;
        ensure!(
            worst_bytes <= max_serialized_bytes,
            "Worst-case finite vector serialization exceeds capacity before model calls"
        );
        vector_capacity(store, worst_bytes, deadline)?;
        Ok(Self {
            store,
            snapshot,
            graph,
            model: model.into(),
            ordered,
            dimension: dimensions,
            maximum: max_serialized_bytes,
            worst_bytes,
        })
    }

    pub fn prepare(
        self,
        input: Value,
        deadline: &Deadline,
    ) -> Result<PreparedVectorCollection<'a>> {
        graph_scope(self.store, self.snapshot, self.graph, deadline)?;
        bounded_serialized_size(&input, self.maximum, deadline)?;
        let mut input: DenseInput = crate::json::decode(input)?;
        ensure!(
            input.snapshot_id == self.snapshot.id
                && input.model == self.model
                && input.items.len() == self.ordered.len(),
            "Prepared vector response scope/count mismatch"
        );
        let mut anchors = AnchorReader::new(self.store, self.snapshot, self.graph)?;
        for (item, (entity_id, hash)) in input.items.iter_mut().zip(&self.ordered) {
            deadline.check()?;
            ensure!(
                item.entity_id == *entity_id
                    && item.content_hash == *hash
                    && item.vector.len() == self.dimension,
                "Prepared vector response order/source/dimension mismatch"
            );
            anchors.entity(entity_id, hash)?;
            item.vector = normalized_vector(&item.vector)?;
        }
        input.items.sort_by(|a, b| a.entity_id.cmp(&b.entity_id));
        let count = input.items.len();
        let collection = DenseCollection {
            schema: 1,
            snapshot_id: self.snapshot.id.clone(),
            workspace_id: self.store.workspace_id.clone(),
            model: self.model.clone(),
            dimension: self.dimension,
            items: input.items,
        };
        let serialized = String::from_utf8(crate::json::encode_bounded(
            &collection,
            self.worst_bytes,
            deadline,
        )?)?;
        let serialized_hash = crate::model::hash(serialized.as_bytes());
        Ok(PreparedVectorCollection {
            store: self.store,
            id: identity(&(self.snapshot.id.as_str(), self.model.as_str())),
            serialized,
            serialized_hash,
            model: self.model,
            snapshot_id: self.snapshot.id.clone(),
            dimension: self.dimension,
            count,
        })
    }
}

impl PreparedVectorCollection<'_> {
    pub fn collection_id(&self) -> &str {
        &self.id
    }
    pub fn serialized_hash(&self) -> &str {
        &self.serialized_hash
    }
    pub fn dimension(&self) -> usize {
        self.dimension
    }
    pub fn count(&self) -> usize {
        self.count
    }

    pub(crate) fn publish(
        self,
        completion: &crate::provider_batches::CheckedBatchCompletion,
        deadline: &Deadline,
    ) -> Result<Value> {
        database_deadline(self.store, deadline)?;
        let tx =
            rusqlite::Transaction::new_unchecked(&self.store.db, TransactionBehavior::Immediate)?;
        vector_namespace_absent(self.store, &self.id)?;
        vector_capacity(self.store, self.serialized.len(), deadline)?;
        tx.execute(
            "INSERT INTO records(kind,id,value) VALUES(?1,?2,?3)",
            params![VECTOR_KIND, self.id, self.serialized],
        )?;
        completion.persist_in_tx(
            &tx,
            &self.id,
            &self.serialized_hash,
            self.dimension,
            self.count,
            deadline,
        )?;
        deadline.check()?;
        tx.commit()?;
        Ok(
            json!({"snapshotId":self.snapshot_id,"model":self.model,"dimension":self.dimension,"count":self.count,"normalized":true,"trust":"source_hash_binding_not_provider_attestation","collectionId":self.id,"collectionHash":self.serialized_hash}),
        )
    }
}

fn model_label(model: &str) -> Result<()> {
    bounded_text(model, 200, false)?;
    ensure!(
        !model.chars().any(char::is_control),
        "Model identifier must be a single label"
    );
    Ok(())
}
fn bounded_serialized_size(
    value: &impl Serialize,
    maximum: usize,
    deadline: &Deadline,
) -> Result<usize> {
    struct Count<'a> {
        bytes: usize,
        maximum: usize,
        deadline: &'a Deadline,
    }
    impl std::io::Write for Count<'_> {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.deadline.check().map_err(std::io::Error::other)?;
            if bytes.len() > self.maximum.saturating_sub(self.bytes) {
                return Err(std::io::Error::other("Vector JSON exceeds byte capacity"));
            }
            self.bytes += bytes.len();
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut count = Count {
        bytes: 0,
        maximum,
        deadline,
    };
    serde_json::to_writer(&mut count, value)?;
    Ok(count.bytes)
}
fn normalized_vector(vector: &[f64]) -> Result<Vec<f64>> {
    ensure!(
        !vector.is_empty() && vector.len() <= 4096 && vector.iter().all(|v| v.is_finite()),
        "Vector requires 1–4096 finite coordinates"
    );
    let norm = vector.iter().map(|v| v * v).sum::<f64>().sqrt();
    ensure!(
        norm > 0.0 && norm.is_finite(),
        "Zero, underflowed or overflowed vector norm refused"
    );
    Ok(vector.iter().map(|v| v / norm).collect())
}
struct SourceReader<'a> {
    scope: crate::capture::CapturedSourceScope<'a>,
    texts: BTreeMap<String, String>,
    cached_bytes: usize,
}
impl<'a> SourceReader<'a> {
    fn new(store: &'a Store, snapshot: &'a Snapshot) -> Result<Self> {
        let scope = crate::capture::source_scope_policy(store, snapshot)?;
        scope.require_legacy_text("knowledge source proofs")?;
        Ok(Self {
            scope,
            texts: BTreeMap::new(),
            cached_bytes: 0,
        })
    }
    fn read(&mut self, path: &str, hash: &str) -> Result<&str> {
        let file = self.scope.file(path)?;
        ensure!(
            valid_id(hash) && file.hash == hash,
            "Imported source hash mismatch"
        );
        if !self.texts.contains_key(hash) {
            let text = String::from_utf8(self.scope.read_source(path)?)?;
            ensure!(
                text.len() <= 4 * 1024 * 1024,
                "Source blob exceeds per-file capacity"
            );
            if self.cached_bytes.saturating_add(text.len()) > 8 * 1024 * 1024 {
                self.texts.clear();
                self.cached_bytes = 0;
            }
            self.cached_bytes += text.len();
            self.texts.insert(hash.to_string(), text);
        }
        ensure!(
            self.texts[hash].len() == file.size,
            "Captured source size differs from manifest"
        );
        Ok(&self.texts[hash])
    }
}
struct AnchorReader<'a> {
    snapshot_id: &'a str,
    entities: BTreeMap<&'a str, &'a Entity>,
    sources: SourceReader<'a>,
}
impl<'a> AnchorReader<'a> {
    fn new(store: &'a Store, snapshot: &'a Snapshot, graph: &'a Graph) -> Result<Self> {
        let entities: BTreeMap<_, _> = graph
            .entities
            .iter()
            .map(|entity| (entity.id.as_str(), entity))
            .collect();
        ensure!(
            entities.len() == graph.entities.len(),
            "Duplicate entity ID in graph"
        );
        Ok(Self {
            snapshot_id: &snapshot.id,
            entities,
            sources: SourceReader::new(store, snapshot)?,
        })
    }
    fn entity(&mut self, id: &str, hash: &str) -> Result<&'a Entity> {
        ensure!(valid_id(id), "Invalid vector entity ID");
        let entity = *self
            .entities
            .get(id)
            .context("Vector entity is not in this snapshot")?;
        ensure!(
            entity.snapshot_id == self.snapshot_id,
            "Entity snapshot mismatch"
        );
        let text = self.sources.read(&entity.path, hash)?;
        utf8_range(text, entity.start_byte, entity.end_byte)?;
        Ok(entity)
    }
}
fn persist_collection(
    store: &Store,
    kind: &str,
    id: &str,
    value: &impl Serialize,
    deadline: &Deadline,
) -> Result<()> {
    let serialized = String::from_utf8(crate::json::encode_bounded(
        value,
        MAX_RECORD_BYTES,
        deadline,
    )?)?;
    database_deadline(store, deadline)?;
    let tx = rusqlite::Transaction::new_unchecked(&store.db, TransactionBehavior::Immediate)?;
    let existing = record_raw(store, kind, id)?;
    if let Some(existing) = existing {
        ensure!(
            existing == serialized,
            "This model already has a different immutable collection; choose a versioned model identifier"
        );
    } else {
        vector_capacity(store, serialized.len(), deadline)?;
        deadline.check()?;
        tx.execute(
            "INSERT INTO records VALUES(?1,?2,?3)",
            params![kind, id, serialized],
        )?;
    }
    deadline.check()?;
    tx.commit()?;
    Ok(())
}

pub fn import_vectors(
    store: &Store,
    snapshot: &Snapshot,
    graph: &Graph,
    input: Value,
    deadline: &Deadline,
) -> Result<Value> {
    graph_scope(store, snapshot, graph, deadline)?;
    bounded_serialized_size(&input, MAX_RECORD_BYTES, deadline)?;
    let mut input: DenseInput = serde_json::from_value(input)?;
    ensure!(
        input.snapshot_id == snapshot.id,
        "Vector import snapshot mismatch"
    );
    model_label(&input.model)?;
    ensure!(
        !input.model.starts_with("yoke-local-tfidf-v1:"),
        "Local vector model namespace is reserved"
    );
    ensure!(
        !input.items.is_empty() && input.items.len() <= 10_000,
        "Import requires 1–10000 vector items"
    );
    let dimension = input.items[0].vector.len();
    ensure!(
        input.items.len().saturating_mul(dimension) <= 2_000_000,
        "Dense vector coordinate capacity exceeded"
    );
    let mut seen = BTreeSet::new();
    let mut anchors = AnchorReader::new(store, snapshot, graph)?;
    for item in &mut input.items {
        deadline.check()?;
        ensure!(
            seen.insert(item.entity_id.clone()),
            "Duplicate vector entity"
        );
        ensure!(item.vector.len() == dimension, "Mixed vector dimensions");
        anchors.entity(&item.entity_id, &item.content_hash)?;
        item.vector = normalized_vector(&item.vector)?;
    }
    input.items.sort_by(|a, b| a.entity_id.cmp(&b.entity_id));
    let collection = DenseCollection {
        schema: 1,
        snapshot_id: snapshot.id.clone(),
        workspace_id: store.workspace_id.clone(),
        model: input.model.clone(),
        dimension,
        items: input.items,
    };
    let id = identity(&(snapshot.id.as_str(), input.model.as_str()));
    persist_collection(store, VECTOR_KIND, &id, &collection, deadline)?;
    Ok(
        json!({"snapshotId":snapshot.id,"model":input.model,"dimension":dimension,"count":collection.items.len(),"normalized":true,"trust":"source_hash_binding_not_provider_attestation","modelCalls":0}),
    )
}

pub fn search_vectors(
    store: &Store,
    snapshot: &Snapshot,
    graph: &Graph,
    model: &str,
    vector: &[f64],
    limit: usize,
    deadline: &Deadline,
) -> Result<Value> {
    graph_scope(store, snapshot, graph, deadline)?;
    model_label(model)?;
    ensure!(
        (1..=100).contains(&limit),
        "Vector result limit must be 1–100"
    );
    let query = normalized_vector(vector)?;
    let raw = record_raw(
        store,
        VECTOR_KIND,
        &identity(&(snapshot.id.as_str(), model)),
    )?
    .context("Vector model is not indexed for this snapshot")?;
    let collection: DenseCollection = serde_json::from_str(&raw)?;
    ensure!(
        collection.schema == 1
            && collection.snapshot_id == snapshot.id
            && collection.workspace_id == store.workspace_id
            && collection.model == model,
        "Corrupt vector scope"
    );
    ensure!(
        collection.dimension == query.len()
            && !collection.items.is_empty()
            && collection.items.len() <= 10_000
            && collection.items.len().saturating_mul(collection.dimension) <= 2_000_000,
        "Query dimension or stored vector capacity mismatch"
    );
    let mut seen = BTreeSet::new();
    let mut items = Vec::new();
    let mut anchors = AnchorReader::new(store, snapshot, graph)?;
    for item in collection.items {
        deadline.check()?;
        ensure!(
            seen.insert(item.entity_id.clone()) && item.vector.len() == collection.dimension,
            "Corrupt vector dimensions or duplicate entity"
        );
        let entity = anchors.entity(&item.entity_id, &item.content_hash)?;
        let normalized = normalized_vector(&item.vector)?;
        let norm = item.vector.iter().map(|v| v * v).sum::<f64>().sqrt();
        ensure!(
            (norm - 1.0).abs() <= 1e-9,
            "Stored vector is not normalized"
        );
        let score = query
            .iter()
            .zip(normalized)
            .map(|(a, b)| a * b)
            .sum::<f64>()
            .clamp(-1.0, 1.0);
        items.push(json!({"entity":entity,"contentHash":item.content_hash,"score":score}));
    }
    sort_scores(&mut items);
    let matched = items.len();
    items.truncate(limit);
    Ok(
        json!({"snapshotId":snapshot.id,"model":model,"items":items,"matched":matched,"truncated":matched>limit,"kind":"external_vector_cosine_ranking","trust":"ranking_not_evidence","limitations":["Source hashes bind the supplied vectors to local bytes; they do not attest that a provider inspected the bytes.","No provider or model is executed by import or retrieval."]}),
    )
}

fn sort_scores(items: &mut [Value]) {
    items.sort_by(|a, b| {
        b["score"]
            .as_f64()
            .partial_cmp(&a["score"].as_f64())
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a["entity"]["id"].as_str().cmp(&b["entity"]["id"].as_str()))
    });
}
/// Split identifier casing, then tokenize Unicode letters/numbers into lexical terms.
fn terms(value: &str) -> BTreeMap<String, usize> {
    let mut expanded = String::with_capacity(value.len());
    let chars: Vec<_> = value.nfc().collect();
    for (i, &c) in chars.iter().enumerate() {
        if i > 0
            && c.is_uppercase()
            && (chars[i - 1].is_lowercase()
                || (chars[i - 1].is_uppercase()
                    && chars.get(i + 1).is_some_and(|n| n.is_lowercase())))
        {
            expanded.push(' ');
        }
        expanded.extend(c.to_lowercase());
    }
    let mut counts = BTreeMap::new();
    for term in expanded.split(|c: char| !c.is_alphanumeric()) {
        if term.chars().count() >= 2 && term.len() <= 128 {
            *counts.entry(term.to_string()).or_insert(0) += 1;
        }
    }
    counts
}
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SparseItem {
    entity_id: String,
    content_hash: String,
    weights: BTreeMap<String, f64>,
}
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct LocalCollection {
    schema: u8,
    snapshot_id: String,
    workspace_id: String,
    model: String,
    idf: BTreeMap<String, f64>,
    items: Vec<SparseItem>,
    candidate_entities: usize,
    omitted_term_occurrences: usize,
}
fn sparse_vector(
    counts: &BTreeMap<String, usize>,
    idf: &BTreeMap<String, f64>,
) -> Result<BTreeMap<String, f64>> {
    let mut vector: BTreeMap<_, _> = counts
        .iter()
        .filter_map(|(term, &count)| {
            idf.get(term)
                .map(|weight| (term.clone(), (1.0 + (count as f64).ln()) * weight))
        })
        .collect();
    if vector.is_empty() {
        return Ok(vector);
    }
    ensure!(
        vector
            .values()
            .all(|value| value.is_finite() && *value > 0.0),
        "Corrupt local vector weights"
    );
    let norm = vector
        .values()
        .map(|weight| weight * weight)
        .sum::<f64>()
        .sqrt();
    ensure!(
        norm.is_finite() && norm > 0.0,
        "Corrupt local vector normalization"
    );
    vector.values_mut().for_each(|weight| *weight /= norm);
    Ok(vector)
}

pub fn build_local_vectors(
    store: &Store,
    snapshot: &Snapshot,
    graph: &Graph,
    deadline: &Deadline,
) -> Result<Value> {
    graph_scope(store, snapshot, graph, deadline)?;
    let candidates: Vec<_> = graph.entities.iter().filter(|e| e.kind != "file").collect();
    ensure!(
        !candidates.is_empty(),
        "No symbol or concept documents to vectorize"
    );
    let candidate_entities = candidates.len();
    let mut sources = SourceReader::new(store, snapshot)?;
    let mut documents = Vec::new();
    let mut df = BTreeMap::<String, usize>::new();
    let mut coordinate_count = 0usize;
    let mut omitted_term_occurrences = 0usize;
    for entity in candidates.into_iter().take(MAX_DOCUMENTS) {
        deadline.check()?;
        let file = snapshot
            .files
            .iter()
            .find(|f| f.path == entity.path)
            .context("Entity source missing")?;
        ensure!(
            entity.snapshot_id == snapshot.id,
            "Entity snapshot mismatch"
        );
        let source = sources.read(&entity.path, &file.hash)?;
        utf8_range(source, entity.start_byte, entity.end_byte)?;
        let mut begin = entity.start_byte.saturating_sub(1024);
        while !source.is_char_boundary(begin) {
            begin += 1;
        }
        let mut end = entity.end_byte.min(begin.saturating_add(8192));
        while !source.is_char_boundary(end) {
            end -= 1;
        }
        let mut counts = terms(&format!(
            "{}\n{}\n{}\n{}",
            entity.path,
            entity.qualified_name,
            entity.signature,
            &source[begin..end]
        ));
        let document_term_count = counts.len();
        // Vocabulary overflow is an explicit partial corpus, never an unbounded allocation.
        counts.retain(|term, _| df.contains_key(term) || df.len() < MAX_TERMS);
        if counts.len() > 2048 {
            counts = counts.into_iter().take(2048).collect();
        }
        if coordinate_count.saturating_add(counts.len()) > MAX_COORDINATES {
            break;
        }
        coordinate_count += counts.len();
        for term in counts.keys() {
            if df.contains_key(term) || df.len() < MAX_TERMS {
                *df.entry(term.clone()).or_insert(0) += 1;
            }
        }
        counts.retain(|term, _| df.contains_key(term));
        omitted_term_occurrences += document_term_count - counts.len();
        documents.push((entity.id.clone(), file.hash.clone(), counts));
    }
    ensure!(
        !documents.is_empty() && !df.is_empty(),
        "No lexical documents within capacity"
    );
    let count = documents.len() as f64;
    let idf: BTreeMap<_, _> = df
        .into_iter()
        .map(|(term, n)| (term, ((count + 1.0) / (n as f64 + 1.0)).ln() + 1.0))
        .collect();
    let items: Vec<_> = documents
        .into_iter()
        .map(|(entity_id, content_hash, counts)| -> Result<_> {
            let weights = sparse_vector(&counts, &idf)?;
            Ok((!weights.is_empty()).then_some(SparseItem {
                entity_id,
                content_hash,
                weights,
            }))
        })
        .collect::<Result<Vec<_>>>()?
        .into_iter()
        .flatten()
        .collect();
    ensure!(!items.is_empty(), "No nonzero lexical vectors");
    let model = format!(
        "yoke-local-tfidf-v1:{}",
        identity(&(
            snapshot.id.as_str(),
            &idf,
            items
                .iter()
                .map(|item| (&item.entity_id, &item.content_hash))
                .collect::<Vec<_>>()
        ))
    );
    let collection = LocalCollection {
        schema: 1,
        snapshot_id: snapshot.id.clone(),
        workspace_id: store.workspace_id.clone(),
        model: model.clone(),
        idf,
        items,
        candidate_entities,
        omitted_term_occurrences,
    };
    let id = identity(&(snapshot.id.as_str(), model.as_str()));
    persist_collection(store, LOCAL_KIND, &id, &collection, deadline)?;
    Ok(
        json!({"snapshotId":snapshot.id,"model":model,"count":collection.items.len(),"vocabularyTerms":collection.idf.len(),"omittedEntities":candidate_entities-collection.items.len(),"omittedTermOccurrences":omitted_term_occurrences,"truncated":candidate_entities>collection.items.len() || omitted_term_occurrences>0,"kind":"statistical_lexical_tfidf","modelCalls":0,"limitations":["Unicode lexical TF-IDF with cosine ranking; no learned semantic or multilingual model.","Bounded documents include up to 1024 preceding source bytes for nearby comments; ranking does not establish a relationship."]}),
    )
}

pub fn search_local_vectors(
    store: &Store,
    snapshot: &Snapshot,
    graph: &Graph,
    model: &str,
    query: &str,
    limit: usize,
    deadline: &Deadline,
) -> Result<Value> {
    graph_scope(store, snapshot, graph, deadline)?;
    model_label(model)?;
    bounded_text(query, 4000, false)?;
    ensure!(
        (1..=100).contains(&limit),
        "Vector result limit must be 1–100"
    );
    let raw = record_raw(store, LOCAL_KIND, &identity(&(snapshot.id.as_str(), model)))?
        .context("Local vector model is not indexed for this snapshot")?;
    let collection: LocalCollection = serde_json::from_str(&raw)?;
    ensure!(
        collection.schema == 1
            && collection.snapshot_id == snapshot.id
            && collection.workspace_id == store.workspace_id
            && collection.model == model,
        "Corrupt local vector scope"
    );
    ensure!(
        !collection.items.is_empty()
            && collection.items.len() <= MAX_DOCUMENTS
            && collection.candidate_entities >= collection.items.len()
            && !collection.idf.is_empty()
            && collection.idf.len() <= MAX_TERMS
            && collection.idf.values().all(|v| v.is_finite()
                && *v >= 1.0
                && *v <= (MAX_DOCUMENTS as f64 + 1.0).ln() + 1.0),
        "Corrupt local vectorizer"
    );
    let query = sparse_vector(&terms(query), &collection.idf)?;
    let mut seen = BTreeSet::new();
    let mut coordinates = 0usize;
    let mut items = Vec::new();
    let mut anchors = AnchorReader::new(store, snapshot, graph)?;
    for item in collection.items {
        deadline.check()?;
        coordinates = coordinates.saturating_add(item.weights.len());
        ensure!(
            seen.insert(item.entity_id.clone())
                && !item.weights.is_empty()
                && item.weights.len() <= 2048
                && coordinates <= MAX_COORDINATES,
            "Corrupt sparse vector capacity"
        );
        ensure!(
            item.weights
                .iter()
                .all(|(term, v)| collection.idf.contains_key(term) && v.is_finite() && *v > 0.0),
            "Corrupt sparse coordinates"
        );
        let norm = item.weights.values().map(|v| v * v).sum::<f64>().sqrt();
        ensure!(
            (norm - 1.0).abs() <= 1e-9,
            "Stored sparse vector is not normalized"
        );
        let entity = anchors.entity(&item.entity_id, &item.content_hash)?;
        let score = query
            .iter()
            .filter_map(|(term, weight)| item.weights.get(term).map(|v| v * weight))
            .sum::<f64>()
            .clamp(0.0, 1.0);
        if score > 0.0 {
            items.push(json!({"entity":entity,"contentHash":item.content_hash,"score":score}));
        }
    }
    sort_scores(&mut items);
    let matched = items.len();
    items.truncate(limit);
    Ok(
        json!({"snapshotId":snapshot.id,"model":model,"items":items,"matched":matched,"truncated":matched>limit,"corpusOmittedEntities":collection.candidate_entities-seen.len(),"corpusOmittedTermOccurrences":collection.omitted_term_occurrences,"kind":"statistical_lexical_tfidf","modelCalls":0,"trust":"ranking_not_evidence"}),
    )
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct LcovInput {
    snapshot_id: String,
    hashes: BTreeMap<String, String>,
    lcov: String,
}
#[derive(Default)]
struct CoverageFile {
    path: String,
    test_name: String,
    lines: BTreeMap<usize, u64>,
    declared_lines: Option<usize>,
    declared_hits: Option<usize>,
}

fn validate_hashes(snapshot: &Snapshot, hashes: &BTreeMap<String, String>) -> Result<()> {
    ensure!(
        !hashes.is_empty() && hashes.len() <= 10_000,
        "Source hash map requires 1–10000 paths"
    );
    for (path, hash) in hashes {
        normalized(path)?;
        ensure!(
            snapshot
                .files
                .iter()
                .any(|f| f.path == *path && f.hash == *hash),
            "Imported source hash mismatch"
        );
    }
    Ok(())
}
fn artifact(store: &Store, snapshot: &Snapshot, data: Value, deadline: &Deadline) -> Result<Value> {
    deadline.check()?;
    let bytes = serde_json::to_vec(&data)?;
    ensure!(
        bytes.len() <= MAX_RECORD_BYTES,
        "Imported artifact exceeds byte capacity"
    );
    let artifact_id = store.artifact(&bytes)?;
    deadline.check()?;
    Ok(
        json!({"snapshotId":snapshot.id,"artifactId":artifact_id,"format":data["format"],"trust":data["trust"],"modelCalls":0,"executesProjectCode":false}),
    )
}
fn flush_coverage(
    sources: &mut SourceReader<'_>,
    hashes: &BTreeMap<String, String>,
    record: CoverageFile,
) -> Result<Value> {
    let hash = hashes
        .get(&record.path)
        .context("LCOV requires a matching hash for every source file")?;
    let source = sources.read(&record.path, hash)?;
    let line_count = crate::paths::line_starts(source).len();
    ensure!(
        !record.lines.is_empty()
            && record
                .lines
                .keys()
                .all(|&line| line > 0 && line <= line_count),
        "LCOV requires valid source line records"
    );
    if let Some(lines) = record.declared_lines {
        ensure!(
            lines == record.lines.len(),
            "LCOV declared line count differs from supplied records"
        );
    }
    if let Some(hits) = record.declared_hits {
        ensure!(
            hits == record.lines.values().filter(|&&v| v > 0).count(),
            "LCOV declared hit count differs from supplied records"
        );
    }
    Ok(
        json!({"path":record.path,"contentHash":hash,"testName":record.test_name,"lines":record.lines}),
    )
}

pub fn import_lcov(
    store: &Store,
    snapshot: &Snapshot,
    input: Value,
    deadline: &Deadline,
) -> Result<Value> {
    scope(store, snapshot, deadline)?;
    ensure!(
        serde_json::to_vec(&input)?.len() <= MAX_RECORD_BYTES,
        "LCOV input exceeds capacity"
    );
    let input: LcovInput = serde_json::from_value(input)?;
    ensure!(input.snapshot_id == snapshot.id, "LCOV snapshot mismatch");
    validate_hashes(snapshot, &input.hashes)?;
    bounded_text(&input.lcov, 20_000_000, false)?;
    let mut current: Option<CoverageFile> = None;
    let mut test_name = String::new();
    let mut files = Vec::new();
    let mut total_lines = 0usize;
    let mut ignored_record_kinds = BTreeSet::new();
    let mut sources = SourceReader::new(store, snapshot)?;
    for line in input.lcov.lines() {
        deadline.check()?;
        if line.is_empty() {
            continue;
        }
        if let Some(name) = line.strip_prefix("TN:") {
            ensure!(
                current.is_none(),
                "LCOV test name must precede its source record"
            );
            bounded_text(name, 200, true)?;
            test_name = name.to_string();
        } else if let Some(path) = line.strip_prefix("SF:") {
            ensure!(
                current.is_none() && files.len() < 10_000,
                "LCOV source record is incomplete or capacity was exceeded"
            );
            current = Some(CoverageFile {
                path: normalized(path)?,
                test_name: test_name.clone(),
                ..CoverageFile::default()
            });
        } else if let Some(data) = line.strip_prefix("DA:") {
            let record = current
                .as_mut()
                .context("LCOV line record needs a source file")?;
            let fields: Vec<_> = data.split(',').collect();
            ensure!(
                fields.len() == 2 || fields.len() == 3,
                "Malformed LCOV line record"
            );
            let line = fields[0].parse::<usize>()?;
            let hits = fields[1].parse::<u64>()?;
            ensure!(
                record.lines.insert(line, hits).is_none(),
                "Duplicate LCOV source line"
            );
            total_lines += 1;
            ensure!(total_lines <= 200_000, "LCOV line capacity exceeded");
        } else if let Some(data) = line.strip_prefix("LF:") {
            let record = current
                .as_mut()
                .context("LCOV summary needs a source file")?;
            ensure!(
                record.declared_lines.replace(data.parse()?).is_none(),
                "Duplicate LCOV line summary"
            );
        } else if let Some(data) = line.strip_prefix("LH:") {
            let record = current
                .as_mut()
                .context("LCOV summary needs a source file")?;
            ensure!(
                record.declared_hits.replace(data.parse()?).is_none(),
                "Duplicate LCOV hit summary"
            );
        } else if line == "end_of_record" {
            let record = current
                .take()
                .context("LCOV record end needs a source file")?;
            files.push(flush_coverage(&mut sources, &input.hashes, record)?);
        } else {
            let kind = line.split(':').next().unwrap_or("");
            ensure!(
                ["FN", "FNDA", "FNF", "FNH", "BRDA", "BRF", "BRH", "VER"].contains(&kind)
                    && current.is_some(),
                "Unsupported LCOV record"
            );
            ignored_record_kinds.insert(kind.to_string());
        }
    }
    ensure!(
        current.is_none() && !files.is_empty(),
        "Incomplete or empty LCOV artifact"
    );
    let count = files.len();
    let mut result = artifact(
        store,
        snapshot,
        json!({"format":"lcov","snapshotId":snapshot.id,"files":files,"ignoredRecordKinds":ignored_record_kinds,"trust":"declared_runtime_artifact","limitations":["Only supplied line hits are imported; function and branch records are listed as ignored.","LCOV test-name labels and source hashes do not independently attest actual test execution or complete path coverage."]}),
        deadline,
    )?;
    result["files"] = json!(count);
    result["lines"] = json!(total_lines);
    Ok(result)
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RuntimeSpan {
    id: String,
    parent_id: Option<String>,
    path: String,
    content_hash: String,
    line: usize,
    start_byte: Option<usize>,
    end_byte: Option<usize>,
    name: Option<String>,
    start_time_ns: Option<u64>,
    duration_ns: Option<u64>,
    attributes: Option<BTreeMap<String, String>>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RuntimeInput {
    snapshot_id: String,
    spans: Vec<RuntimeSpan>,
}
pub fn import_runtime(
    store: &Store,
    snapshot: &Snapshot,
    input: Value,
    deadline: &Deadline,
) -> Result<Value> {
    scope(store, snapshot, deadline)?;
    ensure!(
        serde_json::to_vec(&input)?.len() <= MAX_RECORD_BYTES,
        "Runtime input exceeds capacity"
    );
    let input: RuntimeInput = serde_json::from_value(input)?;
    ensure!(
        input.snapshot_id == snapshot.id && !input.spans.is_empty() && input.spans.len() <= 10_000,
        "Runtime snapshot mismatch or span capacity exceeded"
    );
    let mut parents = BTreeMap::new();
    let mut sources = SourceReader::new(store, snapshot)?;
    for span in &input.spans {
        deadline.check()?;
        bounded_text(&span.id, 128, false)?;
        ensure!(
            !span.id.chars().any(char::is_control),
            "Span ID must be a single label"
        );
        ensure!(
            parents
                .insert(span.id.as_str(), span.parent_id.as_deref())
                .is_none(),
            "Duplicate runtime span"
        );
        if let Some(parent) = &span.parent_id {
            bounded_text(parent, 128, false)?;
        }
        if let Some(name) = &span.name {
            bounded_text(name, 512, false)?;
        }
        if let Some(attributes) = &span.attributes {
            ensure!(
                attributes.len() <= 32,
                "Runtime attribute capacity exceeded"
            );
            for (key, value) in attributes {
                bounded_text(key, 128, false)?;
                bounded_text(value, 1024, true)?;
            }
        }
        let text = sources.read(&span.path, &span.content_hash)?;
        let lines = crate::paths::line_starts(text);
        ensure!(
            span.line > 0 && span.line <= lines.len(),
            "Runtime line is outside source"
        );
        match (span.start_byte, span.end_byte) {
            (Some(start), Some(end)) => {
                utf8_range(text, start, end)?;
                ensure!(
                    lines.partition_point(|&p| p <= start) == span.line,
                    "Runtime range does not start on its declared line"
                );
            }
            (None, None) => {}
            _ => anyhow::bail!("Runtime byte range requires both boundaries"),
        }
        if let (Some(start), Some(duration)) = (span.start_time_ns, span.duration_ns) {
            ensure!(
                start.checked_add(duration).is_some(),
                "Runtime timestamp overflow"
            );
        }
    }
    let mut finished = BTreeSet::new();
    for &id in parents.keys() {
        let mut cursor = Some(id);
        let mut chain = BTreeSet::new();
        while let Some(node) = cursor {
            deadline.check()?;
            if finished.contains(node) {
                break;
            }
            ensure!(chain.insert(node), "Runtime parent graph contains a cycle");
            cursor = *parents
                .get(node)
                .context("Runtime parent span is missing")?;
        }
        finished.extend(chain);
    }
    let count = input.spans.len();
    let mut result = artifact(
        store,
        snapshot,
        json!({"format":"ci-runtime-v1","snapshotId":snapshot.id,"spans":input.spans,"relation":"observed_execution","scope":"this_recorded_execution_only","trust":"declared_runtime_artifact","limitations":["Source hashes bind locations to bytes; supplied spans do not independently attest execution.","Imported executions do not prove unobserved runtime paths or authorize source changes."]}),
        deadline,
    )?;
    result["spans"] = json!(count);
    Ok(result)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SarifInput {
    snapshot_id: String,
    hashes: BTreeMap<String, String>,
    sarif: Value,
}
fn decoded_source_uri(uri: &str) -> Result<String> {
    ensure!(
        uri.len() <= 4096 && !uri.contains(['?', '#']),
        "SARIF source URI must be a relative file path"
    );
    let mut bytes = Vec::with_capacity(uri.len());
    let mut cursor = 0usize;
    while cursor < uri.len() {
        let byte = uri.as_bytes()[cursor];
        if byte == b'%' {
            ensure!(cursor + 2 < uri.len(), "Invalid escaped URI");
            let a = (uri.as_bytes()[cursor + 1] as char)
                .to_digit(16)
                .context("Invalid escaped URI")?;
            let b = (uri.as_bytes()[cursor + 2] as char)
                .to_digit(16)
                .context("Invalid escaped URI")?;
            bytes.push((a * 16 + b) as u8);
            cursor += 3;
        } else {
            bytes.push(byte);
            cursor += 1;
        }
    }
    normalized(&String::from_utf8(bytes)?)
}
fn source_position(
    text: &str,
    line: usize,
    column: Option<usize>,
    column_kind: &str,
    end: bool,
) -> Result<usize> {
    let lines = crate::paths::line_starts(text);
    ensure!(
        line > 0 && line <= lines.len(),
        "SARIF line is outside source"
    );
    let start = lines[line - 1];
    let mut stop = lines.get(line).copied().unwrap_or(text.len());
    while stop > start && matches!(text.as_bytes()[stop - 1], b'\r' | b'\n') {
        stop -= 1;
    }
    if column.is_none() {
        return Ok(if end { stop } else { start });
    }
    let target = column.context("Missing column")?;
    ensure!(target > 0, "SARIF columns are one-based");
    let mut units = 1usize;
    for (byte, c) in text[start..stop].char_indices() {
        if units == target {
            return Ok(start + byte);
        }
        units += if column_kind == "utf16CodeUnits" {
            c.len_utf16()
        } else {
            1
        };
        ensure!(
            units <= target,
            "SARIF position splits a UTF-16 surrogate pair"
        );
    }
    ensure!(units == target, "SARIF column is outside source");
    Ok(stop)
}

pub fn import_sarif(
    store: &Store,
    snapshot: &Snapshot,
    input: Value,
    deadline: &Deadline,
) -> Result<Value> {
    scope(store, snapshot, deadline)?;
    ensure!(
        serde_json::to_vec(&input)?.len() <= MAX_RECORD_BYTES,
        "SARIF input exceeds capacity"
    );
    let input: SarifInput = serde_json::from_value(input)?;
    ensure!(
        input.snapshot_id == snapshot.id && input.sarif["version"] == "2.1.0",
        "SARIF snapshot or version mismatch"
    );
    validate_hashes(snapshot, &input.hashes)?;
    let runs = input.sarif["runs"]
        .as_array()
        .context("SARIF runs required")?;
    ensure!(
        !runs.is_empty() && runs.len() <= 100,
        "SARIF run capacity exceeded"
    );
    let mut findings = Vec::new();
    let mut sources = SourceReader::new(store, snapshot)?;
    let mut declared_results = 0usize;
    for run in runs {
        deadline.check()?;
        let producer = run["tool"]["driver"]["name"]
            .as_str()
            .context("SARIF tool name required")?;
        bounded_text(producer, 200, false)?;
        let column_kind = run
            .get("columnKind")
            .map(|value| value.as_str().context("SARIF columnKind must be a string"))
            .transpose()?
            .unwrap_or("utf16CodeUnits");
        ensure!(
            ["utf16CodeUnits", "unicodeCodePoints"].contains(&column_kind),
            "Unsupported SARIF column kind"
        );
        let results = run
            .get("results")
            .map(|r| r.as_array().context("SARIF results must be an array"))
            .transpose()?;
        for result in results.into_iter().flatten() {
            deadline.check()?;
            declared_results += 1;
            ensure!(declared_results <= 10_000, "SARIF result capacity exceeded");
            let message = result["message"]["text"]
                .as_str()
                .or_else(|| result["message"]["markdown"].as_str())
                .context("SARIF message text required")?;
            bounded_text(message, 8_000, false)?;
            let rule = result
                .get("ruleId")
                .map(|value| value.as_str().context("SARIF ruleId must be a string"))
                .transpose()?;
            if let Some(rule) = rule {
                bounded_text(rule, 512, false)?;
            }
            let level = result
                .get("level")
                .map(|value| value.as_str().context("SARIF level must be a string"))
                .transpose()?
                .unwrap_or("warning");
            ensure!(
                ["none", "note", "warning", "error"].contains(&level),
                "Unsupported SARIF result level"
            );
            let locations = result["locations"]
                .as_array()
                .context("SARIF result requires source locations")?;
            ensure!(
                !locations.is_empty() && locations.len() <= 100,
                "SARIF source location capacity exceeded"
            );
            for location in locations {
                deadline.check()?;
                ensure!(findings.len() < 20_000, "SARIF finding capacity exceeded");
                let physical = location
                    .get("physicalLocation")
                    .context("SARIF logical-only locations are unsupported")?;
                let artifact_location = &physical["artifactLocation"];
                ensure!(
                    artifact_location.get("uriBaseId").is_none()
                        && artifact_location.get("index").is_none(),
                    "SARIF base/index locations require prior explicit resolution"
                );
                let path = decoded_source_uri(
                    artifact_location["uri"]
                        .as_str()
                        .context("SARIF source URI required")?,
                )?;
                let hash = input
                    .hashes
                    .get(&path)
                    .context("SARIF needs a matching source hash for every location")?;
                let text = sources.read(&path, hash)?;
                let (start, end) = if let Some(region) = physical.get("region") {
                    ensure!(
                        region.get("charOffset").is_none()
                            && region.get("charLength").is_none()
                            && region.get("byteOffset").is_none()
                            && region.get("byteLength").is_none(),
                        "SARIF offset regions are unsupported; use source lines and columns"
                    );
                    let start_line = region["startLine"]
                        .as_u64()
                        .context("SARIF startLine required")?
                        as usize;
                    let end_line = region
                        .get("endLine")
                        .map(|v| v.as_u64().context("Invalid SARIF endLine"))
                        .transpose()?
                        .unwrap_or(start_line as u64) as usize;
                    let start_column = region
                        .get("startColumn")
                        .map(|v| v.as_u64().context("Invalid SARIF startColumn"))
                        .transpose()?
                        .map(|v| v as usize);
                    let end_column = region
                        .get("endColumn")
                        .map(|v| v.as_u64().context("Invalid SARIF endColumn"))
                        .transpose()?
                        .map(|v| v as usize);
                    (
                        source_position(text, start_line, start_column, column_kind, false)?,
                        source_position(text, end_line, end_column, column_kind, true)?,
                    )
                } else {
                    (0, text.len())
                };
                utf8_range(text, start, end)?;
                findings.push(json!({"path":path,"contentHash":hash,"startByte":start,"endByte":end,"ruleId":rule,"message":message,"level":level,"producer":producer,"classification":"static_analysis_finding_not_formal_proof"}));
            }
        }
    }
    let count = findings.len();
    let mut result = artifact(
        store,
        snapshot,
        json!({"format":"sarif","snapshotId":snapshot.id,"findings":findings,"declaredResults":declared_results,"trust":"declared_static_analysis_artifact","limitations":["Supplied results are source-bound imported findings, not independent analyzer execution or formal proof.","Relative physical source locations with Unicode or UTF-16 line/column regions are supported; unresolved URI bases, indexes and logical-only locations fail explicitly."]}),
        deadline,
    )?;
    result["findings"] = json!(count);
    result["declaredResults"] = json!(declared_results);
    Ok(result)
}
