//! Checked presentation joins over jointly replayed native meaning selections.
use crate::{
    capture::Deadline,
    model::{Graph, Snapshot, valid_id},
    store::Store,
};
use anyhow::Result;
use rusqlite::{Connection, OptionalExtension, limits::Limit};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
};

#[path = "meaning_join_query.rs"]
mod query;
pub use query::{Direction, Query, validate_query};

pub const MAX_REVIEW_BYTES: usize = 128_000;
const MAX_OUTPUT: usize = 4_000_000;
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ConceptRef {
    pub graph_id: String,
    pub packet_index: usize,
    pub concept_id: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Group {
    pub members: Vec<ConceptRef>,
    pub representative: ConceptRef,
    pub rationale: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Review {
    pub schema_version: u8,
    pub workspace_id: String,
    pub snapshot_id: String,
    pub base_graph_hash: String,
    pub graph_ids: Vec<String>,
    pub groups: Vec<Group>,
}
#[derive(Clone, Debug)]
pub struct Options {
    pub max_graphs: usize,
    pub max_groups: usize,
    pub max_members: usize,
    pub max_concepts: usize,
    pub max_relations: usize,
    pub max_hyperedges: usize,
    pub max_participants: usize,
    pub max_evidence: usize,
    pub max_input_bytes: usize,
    pub max_output_bytes: usize,
}
impl Default for Options {
    fn default() -> Self {
        Self {
            max_graphs: 4,
            max_groups: 256,
            max_members: 4096,
            max_concepts: 1024,
            max_relations: 8192,
            max_hyperedges: 2048,
            max_participants: 16384,
            max_evidence: 16384,
            max_input_bytes: 32_000_000,
            max_output_bytes: MAX_OUTPUT,
        }
    }
}
#[derive(Debug)]
struct Refusal(&'static str);
impl fmt::Display for Refusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.0)
    }
}
impl std::error::Error for Refusal {}
fn failure(code: &'static str) -> anyhow::Error {
    Refusal(code).into()
}
fn input(ok: bool) -> Result<()> {
    if ok {
        Ok(())
    } else {
        Err(failure("meaning_join_input_invalid"))
    }
}
fn capacity(ok: bool) -> Result<()> {
    if ok {
        Ok(())
    } else {
        Err(failure("meaning_join_capacity_exceeded"))
    }
}
fn parent(ok: bool) -> Result<()> {
    if ok {
        Ok(())
    } else {
        Err(failure("meaning_join_parent_refused"))
    }
}
fn check(d: &Deadline) -> Result<()> {
    d.check()
        .map_err(|_| failure("meaning_join_deadline_exceeded"))
}
fn sanitized(error: anyhow::Error, d: &Deadline) -> anyhow::Error {
    for cause in error.chain() {
        if let Some(code) = cause.downcast_ref::<Refusal>() {
            return failure(code.0);
        }
        if let Some(rusqlite::Error::SqliteFailure(code, _)) =
            cause.downcast_ref::<rusqlite::Error>()
            && code.code == rusqlite::ErrorCode::TooBig
        {
            return failure("meaning_join_capacity_exceeded");
        }
        if matches!(
            cause.to_string().as_str(),
            "Meaning record exceeds capacity"
                | "Meaning dependency exceeds remaining input capacity"
                | "Meaning dependency closure exceeds remaining input capacity"
                | "Frozen source byte capacity exceeded"
                | "Frozen source scope capacity exceeded"
                | "JSON input exceeds byte capacity"
                | "JSON output exceeds byte capacity"
        ) {
            return failure("meaning_join_capacity_exceeded");
        }
    }
    if check(d).is_err() {
        failure("meaning_join_deadline_exceeded")
    } else {
        failure("meaning_join_parent_refused")
    }
}
fn reference_valid(r: &ConceptRef) -> bool {
    valid_id(&r.graph_id) && r.packet_index < 64 && valid_id(&r.concept_id)
}
pub fn validate_options(o: &Options) -> Result<()> {
    input(
        o.max_graphs <= 8
            && o.max_groups <= 512
            && o.max_members <= 4096
            && o.max_concepts <= 4096
            && o.max_relations <= 32768
            && o.max_hyperedges <= 8192
            && o.max_participants <= 65536
            && o.max_evidence <= 65536
            && o.max_input_bytes <= 32_000_000
            && o.max_output_bytes <= MAX_OUTPUT,
    )
}
pub fn validate_review(r: &Review, o: &Options, d: &Deadline) -> Result<()> {
    validate_options(o)?;
    check(d)?;
    input(
        r.schema_version == 1
            && valid_id(&r.workspace_id)
            && valid_id(&r.snapshot_id)
            && valid_id(&r.base_graph_hash)
            && !r.graph_ids.is_empty(),
    )?;
    capacity(r.graph_ids.len() <= o.max_graphs && r.groups.len() <= o.max_groups)?;
    for id in &r.graph_ids {
        check(d)?;
        input(valid_id(id))?;
    }
    let mut members = 0usize;
    for group in &r.groups {
        check(d)?;
        input((2..=128).contains(&group.members.len()) && reference_valid(&group.representative))?;
        members = members
            .checked_add(group.members.len())
            .ok_or_else(|| failure("meaning_join_capacity_exceeded"))?;
        capacity(members <= o.max_members)?;
        input(group.rationale.len() <= 2048)?;
        input(
            !group.rationale.trim().is_empty()
                && !crate::paths::obvious_secret(&group.rationale)
                && !group
                    .rationale
                    .chars()
                    .any(|c| c.is_control() && !matches!(c, '\n' | '\r' | '\t')),
        )?;
        for member in &group.members {
            check(d)?;
            input(reference_valid(member) && r.graph_ids.contains(&member.graph_id))?;
        }
        input(group.members.contains(&group.representative))?;
    }
    encode(r, MAX_REVIEW_BYTES, d)?;
    let graphs: BTreeSet<_> = r.graph_ids.iter().collect();
    input(graphs.len() == r.graph_ids.len())?;
    let mut seen = BTreeSet::new();
    for group in &r.groups {
        for member in &group.members {
            check(d)?;
            input(seen.insert(member))?;
        }
    }
    check(d)
}
fn encode<T: Serialize>(value: &T, limit: usize, d: &Deadline) -> Result<Vec<u8>> {
    match crate::json::encode_bounded(value, limit, d) {
        Ok(bytes) => {
            check(d)?;
            Ok(bytes)
        }
        Err(_) => {
            check(d)?;
            Err(failure("meaning_join_capacity_exceeded"))
        }
    }
}
fn digest<T: Serialize>(value: &T, limit: usize, d: &Deadline) -> Result<String> {
    let result = crate::model::hash(encode(value, limit, d)?);
    check(d)?;
    Ok(result)
}
fn rows<'a>(value: &'a Value, key: &str) -> Result<&'a [Value]> {
    value[key]
        .as_array()
        .map(Vec::as_slice)
        .ok_or_else(|| failure("meaning_join_parent_refused"))
}
fn text<'a>(value: &'a Value, key: &str) -> Result<&'a str> {
    value[key]
        .as_str()
        .ok_or_else(|| failure("meaning_join_parent_refused"))
}
fn packet_index(packet: &Value) -> Result<usize> {
    let n = packet["index"]
        .as_u64()
        .ok_or_else(|| failure("meaning_join_parent_refused"))?;
    parent(n < 64)?;
    Ok(n as usize)
}
fn charge(total: &mut usize, amount: usize, limit: usize) -> Result<()> {
    *total = total
        .checked_add(amount)
        .ok_or_else(|| failure("meaning_join_capacity_exceeded"))?;
    capacity(*total <= limit)
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
fn scope(store: &Store, r: &Review, o: &Options, d: &Deadline) -> Result<(Snapshot, Graph)> {
    let original = store.db.set_limit(
        Limit::SQLITE_LIMIT_LENGTH,
        store
            .db
            .limit(Limit::SQLITE_LIMIT_LENGTH)?
            .min((o.max_input_bytes + 65536) as i32),
    )?;
    let _limit = LengthLimit {
        db: &store.db,
        original,
    };
    let mut remaining = o.max_input_bytes;
    let mut read = |table: &str| -> Result<Value> {
        check(d)?;
        let size: Option<usize> = store
            .db
            .query_row(
                &format!("SELECT length(CAST(value AS BLOB)) FROM {table} WHERE id=?1"),
                [&r.snapshot_id],
                |row| row.get(0),
            )
            .optional()?;
        let size = size.ok_or_else(|| failure("meaning_join_parent_refused"))?;
        capacity(size <= remaining)?;
        let raw: String = store.db.query_row(
            &format!("SELECT value FROM {table} WHERE id=?1 AND length(CAST(value AS BLOB))=?2"),
            rusqlite::params![r.snapshot_id, size],
            |row| row.get(0),
        )?;
        parent(raw.len() == size)?;
        remaining -= size;
        let value = crate::json::parse(raw.as_bytes(), size)?;
        check(d)?;
        Ok(value)
    };
    let raw = read("snapshots")?;
    let snapshot: Snapshot = crate::json::decode(raw.clone())?;
    parent(
        serde_json::to_value(&snapshot)? == raw
            && snapshot.id == r.snapshot_id
            && snapshot.workspace_id == r.workspace_id
            && snapshot.workspace_id == store.workspace_id,
    )?;
    let raw = read("graphs")?;
    let graph: Graph = crate::json::decode(raw.clone())?;
    parent(
        serde_json::to_value(&graph)? == raw
            && graph.snapshot_id == snapshot.id
            && digest(&graph, o.max_input_bytes, d)? == r.base_graph_hash,
    )?;
    Ok((snapshot, graph))
}

/// Only this constructor can create the immutable checked presentation value.
/// Serialized projection JSON cannot be admitted as a query capability.
pub struct JoinedGraph {
    value: Value,
    graph_ids: Vec<String>,
}
/// Internal engine adapter. Only the separate source/response-checked identity
/// capability exposes this quotient query path to external callers.
pub(crate) fn query_identity_projection(
    value: &Value,
    graph_ids: &[String],
    q: &Query,
    limit: usize,
    d: &Deadline,
) -> Result<Value> {
    input(value["kind"] == "meaning_identity_quotient_v1")?;
    query::run_projection(
        value,
        graph_ids,
        q,
        limit,
        64_000_000,
        "meaning_identity_query_v1",
        d,
    )
    .map_err(|e| sanitized(e, d))
}
impl JoinedGraph {
    pub fn as_value(&self) -> &Value {
        &self.value
    }
    pub fn query(&self, request: &Query, max_output_bytes: usize, d: &Deadline) -> Result<Value> {
        query::run(self, request, max_output_bytes, d).map_err(|e| sanitized(e, d))
    }
}
pub fn join(
    store: &Store,
    review: &Review,
    options: &Options,
    d: &Deadline,
) -> Result<JoinedGraph> {
    let run = || -> Result<JoinedGraph> {
        validate_review(review, options, d)?;
        capacity(options.max_input_bytes > 0 && options.max_output_bytes > 0)?;
        let mut review = review.clone();
        review.graph_ids.sort();
        for group in &mut review.groups {
            group.members.sort();
        }
        review.groups.sort_by(|a, b| a.members.cmp(&b.members));
        let policy_hash = digest(
            &("meaning-join-policy-v1", &review),
            MAX_REVIEW_BYTES + 128,
            d,
        )?;
        store.set_deadline(d)?;
        let transaction = if store.db.is_autocommit() {
            Some(store.db.unchecked_transaction()?)
        } else {
            None
        };
        let (snapshot, graph) = scope(store, &review, options, d)?;
        let checked = crate::meaning_build::checked_published_set(
            store,
            &snapshot,
            &graph,
            &review.graph_ids,
            options.max_input_bytes,
            d,
        )?;
        let value = render(&checked, &review, &policy_hash, options, d)?;
        if let Some(transaction) = transaction {
            transaction.commit()?;
        }
        check(d)?;
        Ok(JoinedGraph {
            value,
            graph_ids: review.graph_ids,
        })
    };
    run().map_err(|e| sanitized(e, d))
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "camelCase")]
struct Reference<'a> {
    graph_id: &'a str,
    packet_index: usize,
    concept_id: &'a str,
}
impl<'a> From<&'a ConceptRef> for Reference<'a> {
    fn from(r: &'a ConceptRef) -> Self {
        Self {
            graph_id: &r.graph_id,
            packet_index: r.packet_index,
            concept_id: &r.concept_id,
        }
    }
}
#[derive(Serialize)]
struct Member<'a> {
    reference: Reference<'a>,
    record: &'a Value,
}
#[derive(Serialize)]
struct Node<'a> {
    id: String,
    representative: Reference<'a>,
    members: Vec<Member<'a>>,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct RelationRef<'a> {
    graph_id: &'a str,
    packet_index: usize,
    relation_id: &'a str,
}
#[derive(Serialize)]
struct Relation<'a> {
    reference: RelationRef<'a>,
    record: &'a Value,
    source: &'a str,
    target: &'a str,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct HyperedgeRef<'a> {
    graph_id: &'a str,
    packet_index: usize,
    hyperedge_id: &'a str,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Slot<'a> {
    position: usize,
    role: &'a str,
    original: Reference<'a>,
    node_id: &'a str,
}
#[derive(Serialize)]
struct Hyperedge<'a> {
    reference: HyperedgeRef<'a>,
    record: &'a Value,
    participants: Vec<Slot<'a>>,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Evidence<'a> {
    graph_id: &'a str,
    packet_index: usize,
    record: &'a Value,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Packet<'a> {
    packet_index: usize,
    input_hash: &'a Value,
    provenance: &'a Value,
    source_bindings: &'a Value,
    unit_set_hash: &'a Value,
    contract_hash: &'a Value,
    coverage: &'a Value,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Parent<'a> {
    graph_id: &'a str,
    snapshot_id: &'a str,
    job_id: &'a str,
    graph_hash: &'a str,
    artifact_id: &'a str,
    total_bytes: usize,
    model: &'a str,
    model_version: &'a str,
    analyzer_epoch: &'a str,
    capture_policy_hash: &'a str,
    coverage: &'a Value,
    packets: Vec<Packet<'a>>,
}
#[derive(Default, Serialize)]
#[serde(rename_all = "camelCase")]
struct Statistics {
    graphs: usize,
    packets: usize,
    original_concepts: usize,
    original_relations: usize,
    original_hyperedges: usize,
    original_participants: usize,
    original_evidence: usize,
    groups: usize,
    submitted_members: usize,
    projected_nodes: usize,
    admitted_input_bytes: usize,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Report<'a> {
    schema_version: u8,
    kind: &'static str,
    projection_id: String,
    policy_hash: &'a str,
    review: &'a Review,
    statistics: Statistics,
    nodes: &'a [Node<'a>],
    relations: Vec<Relation<'a>>,
    hyperedges: Vec<Hyperedge<'a>>,
    evidence: Vec<Evidence<'a>>,
    parents: Vec<Parent<'a>>,
    grouping_authority: &'static str,
    binding_validation: &'static str,
    provider_attestation: &'static str,
    semantic_identity_established: bool,
    original_concept_merges_applied: u8,
    model_calls: u8,
    complete: bool,
    limitations: [&'static str; 3],
}
fn render(
    checked: &crate::meaning_build::CheckedPublishedSet,
    review: &Review,
    policy_hash: &str,
    o: &Options,
    d: &Deadline,
) -> Result<Value> {
    let mut statistics = Statistics {
        graphs: checked.graphs().len(),
        groups: review.groups.len(),
        submitted_members: review.groups.iter().map(|g| g.members.len()).sum(),
        admitted_input_bytes: checked.admitted_bytes(),
        ..Default::default()
    };
    let mut concepts = BTreeMap::new();
    for graph in checked.graphs() {
        let mut indices = BTreeSet::new();
        for packet in rows(&graph.value, "packets")? {
            check(d)?;
            let index = packet_index(packet)?;
            parent(indices.insert(index))?;
            statistics.packets += 1;
            let meaning = &packet["meaning"];
            charge(
                &mut statistics.original_concepts,
                rows(meaning, "concepts")?.len(),
                o.max_concepts,
            )?;
            charge(
                &mut statistics.original_relations,
                rows(meaning, "relations")?.len(),
                o.max_relations,
            )?;
            charge(
                &mut statistics.original_hyperedges,
                rows(meaning, "hyperedges")?.len(),
                o.max_hyperedges,
            )?;
            charge(
                &mut statistics.original_evidence,
                rows(meaning, "evidence")?.len(),
                o.max_evidence,
            )?;
            for edge in rows(meaning, "hyperedges")? {
                check(d)?;
                charge(
                    &mut statistics.original_participants,
                    rows(edge, "participants")?.len(),
                    o.max_participants,
                )?;
            }
            for record in rows(meaning, "concepts")? {
                check(d)?;
                parent(
                    concepts
                        .insert(
                            Reference {
                                graph_id: &graph.graph_id,
                                packet_index: index,
                                concept_id: text(record, "id")?,
                            },
                            record,
                        )
                        .is_none(),
                )?;
            }
        }
    }
    let mut nodes = Vec::new();
    let mut grouped = BTreeSet::new();
    for group in &review.groups {
        let mut members = Vec::new();
        let mut references = Vec::new();
        for reference in &group.members {
            check(d)?;
            let (original, record) = concepts
                .get_key_value(&Reference::from(reference))
                .ok_or_else(|| failure("meaning_join_input_invalid"))?;
            references.push(*original);
            members.push(Member {
                reference: *original,
                record,
            });
            input(grouped.insert(*original))?;
        }
        nodes.push(Node {
            id: digest(
                &("meaning-join-node-v1", references),
                MAX_REVIEW_BYTES + 128,
                d,
            )?,
            representative: Reference::from(&group.representative),
            members,
        });
    }
    for (reference, record) in &concepts {
        check(d)?;
        if !grouped.contains(reference) {
            nodes.push(Node {
                id: digest(&("meaning-join-node-v1", [reference]), 1024, d)?,
                representative: *reference,
                members: vec![Member {
                    reference: *reference,
                    record,
                }],
            });
        }
    }
    nodes.sort_by(|a, b| a.id.cmp(&b.id));
    statistics.projected_nodes = nodes.len();
    let mut targets = BTreeMap::new();
    for node in &nodes {
        for member in &node.members {
            check(d)?;
            parent(targets.insert(member.reference, node.id.as_str()).is_none())?;
        }
    }
    let mut relations = Vec::new();
    let mut hyperedges = Vec::new();
    let mut evidence = BTreeMap::new();
    let mut parents = Vec::new();
    for graph in checked.graphs() {
        let mut packets = Vec::new();
        for packet in rows(&graph.value, "packets")? {
            check(d)?;
            let index = packet_index(packet)?;
            let meaning = &packet["meaning"];
            let endpoint = |id: &str| -> Result<&str> {
                targets
                    .get(&Reference {
                        graph_id: &graph.graph_id,
                        packet_index: index,
                        concept_id: id,
                    })
                    .copied()
                    .ok_or_else(|| failure("meaning_join_parent_refused"))
            };
            let mut relation_ids = BTreeSet::new();
            let mut hyperedge_ids = BTreeSet::new();
            for record in rows(meaning, "relations")? {
                check(d)?;
                let id = text(record, "id")?;
                parent(relation_ids.insert(id))?;
                relations.push(Relation {
                    reference: RelationRef {
                        graph_id: &graph.graph_id,
                        packet_index: index,
                        relation_id: id,
                    },
                    record,
                    source: endpoint(text(record, "source")?)?,
                    target: endpoint(text(record, "target")?)?,
                });
            }
            for record in rows(meaning, "hyperedges")? {
                check(d)?;
                let id = text(record, "id")?;
                parent(hyperedge_ids.insert(id))?;
                let mut participants = Vec::new();
                for (position, slot) in rows(record, "participants")?.iter().enumerate() {
                    check(d)?;
                    let concept_id = text(slot, "concept")?;
                    participants.push(Slot {
                        position,
                        role: text(slot, "role")?,
                        original: Reference {
                            graph_id: &graph.graph_id,
                            packet_index: index,
                            concept_id,
                        },
                        node_id: endpoint(concept_id)?,
                    });
                }
                hyperedges.push(Hyperedge {
                    reference: HyperedgeRef {
                        graph_id: &graph.graph_id,
                        packet_index: index,
                        hyperedge_id: id,
                    },
                    record,
                    participants,
                });
            }
            for record in rows(meaning, "evidence")? {
                check(d)?;
                parent(
                    evidence
                        .insert(
                            (graph.graph_id.as_str(), index, text(record, "id")?),
                            record,
                        )
                        .is_none(),
                )?;
            }
            packets.push(Packet {
                packet_index: index,
                input_hash: &packet["inputHash"],
                provenance: &packet["provenance"],
                source_bindings: &meaning["sourceBindings"],
                unit_set_hash: &meaning["unitSetHash"],
                contract_hash: &meaning["contractHash"],
                coverage: &meaning["coverage"],
            });
        }
        packets.sort_by_key(|p| p.packet_index);
        parents.push(Parent {
            graph_id: &graph.graph_id,
            snapshot_id: &graph.snapshot_id,
            job_id: &graph.job_id,
            graph_hash: &graph.base_graph_hash,
            artifact_id: &graph.artifact_id,
            total_bytes: graph.total_bytes,
            model: &graph.model,
            model_version: &graph.model_version,
            analyzer_epoch: &graph.analyzer_epoch,
            capture_policy_hash: &graph.capture_policy_hash,
            coverage: &graph.value["coverage"],
            packets,
        });
    }
    relations.sort_by_key(|r| {
        (
            r.reference.graph_id,
            r.reference.packet_index,
            r.reference.relation_id,
        )
    });
    hyperedges.sort_by_key(|r| {
        (
            r.reference.graph_id,
            r.reference.packet_index,
            r.reference.hyperedge_id,
        )
    });
    parents.sort_by_key(|p| p.graph_id);
    let report = Report {
        schema_version: 1,
        kind: "meaning_join_projection_v1",
        projection_id: digest(&("meaning-join-projection-v1", policy_hash), 1024, d)?,
        policy_hash,
        review,
        statistics,
        nodes: &nodes,
        relations,
        hyperedges,
        evidence: evidence
            .into_iter()
            .map(|((graph_id, packet_index, _), record)| Evidence {
                graph_id,
                packet_index,
                record,
            })
            .collect(),
        parents,
        grouping_authority: "explicit_host_assertion",
        binding_validation: "joint_frozen_captured_source_replay",
        provider_attestation: "unverified",
        semantic_identity_established: false,
        original_concept_merges_applied: 0,
        model_calls: 0,
        complete: false,
        limitations: [
            "Host groups are presentation assertions, not authenticated review or established semantic identity.",
            "Original inferred claims and receipts, parallel directed observations, selfloops and every ordered role slot remain distinct; no parent is changed.",
            "Selections share one explicit native workspace/canonical snapshot. No provider, capture, persistence, cross-history/workspace join or mixed-document source space is implied.",
        ],
    };
    let bytes = encode(&report, o.max_output_bytes, d)?;
    let value = crate::json::parse(&bytes, o.max_output_bytes)?;
    check(d)?;
    Ok(value)
}
