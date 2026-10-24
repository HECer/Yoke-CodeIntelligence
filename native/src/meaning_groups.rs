//! Read-only explicit host grouping of replayed inferred meaning concepts.
use crate::{capture::Deadline, store::Store};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ConceptRef {
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
    pub graph_id: String,
    pub groups: Vec<Group>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Options {
    pub max_groups: usize,
    pub max_members: usize,
    pub max_concepts: usize,
    pub max_relations: usize,
    pub max_hyperedges: usize,
    pub max_participants: usize,
    pub max_evidence: usize,
    pub max_output_bytes: usize,
}
impl Default for Options {
    fn default() -> Self {
        Self {
            max_groups: 256,
            max_members: 4096,
            max_concepts: 1024,
            max_relations: 8192,
            max_hyperedges: 2048,
            max_participants: 16384,
            max_evidence: 16384,
            max_output_bytes: 4_000_000,
        }
    }
}
pub const MAX_REVIEW_BYTES: usize = 128_000;

pub fn validate_options(options: &Options) -> Result<()> {
    ensure!(
        options.max_groups <= 512
            && options.max_members <= 4096
            && options.max_concepts <= 4096
            && options.max_relations <= 32768
            && options.max_hyperedges <= 8192
            && options.max_participants <= 65536
            && options.max_evidence <= 65536
            && options.max_output_bytes <= 4_000_000,
        "Invalid meaning group limits"
    );
    Ok(())
}

/// Pure host admission; reference membership still requires the replayed parent.
pub fn validate_review(review: &Review, options: &Options, d: &Deadline) -> Result<()> {
    d.check()?;
    validate_options(options)?;
    ensure!(
        review.schema_version == 1 && crate::model::valid_id(&review.graph_id),
        "Invalid meaning group review"
    );
    ensure!(
        review.groups.len() <= options.max_groups,
        "Meaning group capacity exceeded"
    );
    let mut members = 0usize;
    for group in &review.groups {
        d.check()?;
        ensure!(
            (2..=128).contains(&group.members.len()),
            "Invalid meaning group member count"
        );
        members = members
            .checked_add(group.members.len())
            .context("Meaning group capacity exceeded")?;
        ensure!(
            members <= options.max_members,
            "Meaning group member capacity exceeded"
        );
        ensure!(
            group.rationale.len() <= 2048,
            "Meaning group rationale byte capacity exceeded"
        );
        ensure!(
            !group.rationale.trim().is_empty(),
            "Invalid meaning group rationale"
        );
        for ch in group.rationale.chars() {
            d.check()?;
            ensure!(
                !ch.is_control() || matches!(ch, '\n' | '\r' | '\t'),
                "Invalid meaning group rationale"
            );
        }
        for reference in &group.members {
            d.check()?;
            ensure!(
                reference.packet_index < 64 && crate::model::valid_id(&reference.concept_id),
                "Invalid meaning group concept reference"
            );
        }
        ensure!(
            group.members.contains(&group.representative),
            "Meaning group representative is not a member"
        );
    }
    // Admit serialized review bytes before cloning strings or constructing a set.
    crate::json::encode_bounded(review, MAX_REVIEW_BYTES, d)
        .context("Meaning group review exceeds byte capacity")?;
    let mut seen = BTreeSet::new();
    for group in &review.groups {
        for reference in &group.members {
            d.check()?;
            ensure!(
                seen.insert(reference),
                "Duplicate or overlapping meaning group member"
            );
        }
    }
    d.check()
}

struct AdmittedReview {
    review: Review,
    policy_hash: String,
}
fn digest<T: Serialize>(value: &T, maximum: usize, d: &Deadline) -> Result<String> {
    let bytes = crate::json::encode_bounded(value, maximum, d)?;
    let hash = crate::model::hash(bytes);
    d.check()?;
    Ok(hash)
}
fn admit(review: &Review, options: &Options, d: &Deadline) -> Result<AdmittedReview> {
    validate_review(review, options, d)?;
    let mut review = review.clone();
    for group in &mut review.groups {
        d.check()?;
        group.members.sort();
    }
    review.groups.sort_by(|a, b| a.members.cmp(&b.members));
    let policy_hash = digest(
        &("meaning-group-policy-v1", &review),
        MAX_REVIEW_BYTES + 128,
        d,
    )?;
    Ok(AdmittedReview {
        review,
        policy_hash,
    })
}

/// Replay the full parent before projecting any host-selected concept reference.
/// This does not capture source, execute providers, or persist a projection.
pub fn project(store: &Store, review: &Review, options: &Options, d: &Deadline) -> Result<Value> {
    let admitted = admit(review, options, d)?;
    let graph = crate::meaning_build::load(store, &admitted.review.graph_id, d)?;
    render(&graph, &admitted, options, d)
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "camelCase")]
struct Reference<'a> {
    packet_index: usize,
    concept_id: &'a str,
}
impl<'a> From<&'a ConceptRef> for Reference<'a> {
    fn from(reference: &'a ConceptRef) -> Self {
        Self {
            packet_index: reference.packet_index,
            concept_id: &reference.concept_id,
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
struct RelationReference<'a> {
    packet_index: usize,
    relation_id: &'a str,
}
#[derive(Serialize)]
struct Relation<'a> {
    reference: RelationReference<'a>,
    record: &'a Value,
    source: &'a str,
    target: &'a str,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct HyperedgeReference<'a> {
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
    reference: HyperedgeReference<'a>,
    record: &'a Value,
    participants: Vec<Slot<'a>>,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Evidence<'a> {
    packet_index: usize,
    record: &'a Value,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PacketProof<'a> {
    packet_index: usize,
    input_hash: &'a Value,
    provenance: &'a Value,
    source_bindings: &'a Value,
    unit_set_hash: &'a Value,
    contract_hash: &'a Value,
    coverage: &'a Value,
}
#[derive(Default, Serialize)]
#[serde(rename_all = "camelCase")]
struct Statistics {
    packet_count: usize,
    original_concepts: usize,
    original_relations: usize,
    original_hyperedges: usize,
    original_participants: usize,
    original_evidence: usize,
    group_count: usize,
    submitted_members: usize,
    projected_nodes: usize,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Report<'a> {
    schema_version: u8,
    kind: &'static str,
    projection_id: String,
    policy_hash: &'a str,
    graph_id: &'a str,
    workspace_id: &'a Value,
    snapshot_id: &'a Value,
    job_id: &'a Value,
    graph_hash: &'a Value,
    parent_coverage: &'a Value,
    review: &'a Review,
    statistics: Statistics,
    nodes: &'a [Node<'a>],
    relations: Vec<Relation<'a>>,
    hyperedges: Vec<Hyperedge<'a>>,
    evidence: Vec<Evidence<'a>>,
    packets: Vec<PacketProof<'a>>,
    grouping_authority: &'static str,
    binding_validation: &'static str,
    provider_attestation: &'static str,
    semantic_identity_established: bool,
    original_concept_merges_applied: u8,
    model_calls: u8,
    complete: bool,
    limitations: [&'static str; 3],
}

fn rows<'a>(value: &'a Value, key: &str) -> Result<&'a [Value]> {
    value[key]
        .as_array()
        .map(Vec::as_slice)
        .context("Invalid meaning group parent records")
}
fn text<'a>(value: &'a Value, key: &str) -> Result<&'a str> {
    value[key]
        .as_str()
        .context("Invalid meaning group parent field")
}
fn packet_index(packet: &Value) -> Result<usize> {
    let index = packet["index"]
        .as_u64()
        .context("Invalid meaning group packet index")?;
    ensure!(index < 64, "Invalid meaning group packet index");
    Ok(index as usize)
}
fn charge(current: &mut usize, amount: usize, maximum: usize) -> Result<()> {
    *current = current
        .checked_add(amount)
        .context("Meaning group capacity exceeded")?;
    ensure!(*current <= maximum, "Meaning group capacity exceeded");
    Ok(())
}
fn counts(graph: &Value, options: &Options, d: &Deadline) -> Result<Statistics> {
    let packets = rows(graph, "packets")?;
    ensure!(
        packets.len() <= 64,
        "Meaning group packet capacity exceeded"
    );
    let mut result = Statistics {
        packet_count: packets.len(),
        ..Statistics::default()
    };
    for packet in packets {
        d.check()?;
        let meaning = &packet["meaning"];
        charge(
            &mut result.original_concepts,
            rows(meaning, "concepts")?.len(),
            options.max_concepts,
        )?;
        charge(
            &mut result.original_relations,
            rows(meaning, "relations")?.len(),
            options.max_relations,
        )?;
        charge(
            &mut result.original_hyperedges,
            rows(meaning, "hyperedges")?.len(),
            options.max_hyperedges,
        )?;
        charge(
            &mut result.original_evidence,
            rows(meaning, "evidence")?.len(),
            options.max_evidence,
        )?;
        for edge in rows(meaning, "hyperedges")? {
            d.check()?;
            charge(
                &mut result.original_participants,
                rows(edge, "participants")?.len(),
                options.max_participants,
            )?;
        }
    }
    Ok(result)
}

fn render(
    graph: &Value,
    admitted: &AdmittedReview,
    options: &Options,
    d: &Deadline,
) -> Result<Value> {
    d.check()?;
    let review = &admitted.review;
    ensure!(
        graph["graphId"].as_str() == Some(review.graph_id.as_str()),
        "Meaning group parent identity mismatch"
    );
    // Charge every original record and slot before any grouping or proof copying.
    let mut statistics = counts(graph, options, d)?;
    statistics.group_count = review.groups.len();
    statistics.submitted_members = review.groups.iter().map(|g| g.members.len()).sum();
    let mut concepts = BTreeMap::new();
    let mut packet_indices = BTreeSet::new();
    for packet in rows(graph, "packets")? {
        d.check()?;
        let packet_index = packet_index(packet)?;
        ensure!(
            packet_indices.insert(packet_index),
            "Duplicate meaning group parent packet"
        );
        for record in rows(&packet["meaning"], "concepts")? {
            d.check()?;
            let reference = Reference {
                packet_index,
                concept_id: text(record, "id")?,
            };
            ensure!(
                concepts.insert(reference, record).is_none(),
                "Duplicate meaning group parent concept"
            );
        }
    }
    let mut nodes = Vec::new();
    let mut grouped = BTreeSet::new();
    for group in &review.groups {
        d.check()?;
        let mut members = Vec::new();
        let mut references = Vec::new();
        for reference in &group.members {
            d.check()?;
            let (original, record) = concepts
                .get_key_value(&Reference::from(reference))
                .context("Meaning group concept is absent from parent")?;
            references.push(*original);
            members.push(Member {
                reference: *original,
                record,
            });
            ensure!(grouped.insert(*original), "Duplicate meaning group member");
        }
        nodes.push(Node {
            id: digest(
                &("meaning-group-node-v1", &review.graph_id, references),
                MAX_REVIEW_BYTES + 128,
                d,
            )?,
            representative: Reference::from(&group.representative),
            members,
        });
    }
    for (reference, record) in &concepts {
        d.check()?;
        if !grouped.contains(reference) {
            nodes.push(Node {
                id: digest(
                    &("meaning-group-node-v1", &review.graph_id, [reference]),
                    512,
                    d,
                )?,
                representative: *reference,
                members: vec![Member {
                    reference: *reference,
                    record,
                }],
            });
        }
    }
    nodes.sort_by(|a, b| a.id.cmp(&b.id));
    d.check()?;
    ensure!(
        nodes.windows(2).all(|pair| pair[0].id != pair[1].id),
        "Meaning group node identity collision"
    );
    statistics.projected_nodes = nodes.len();
    let mut mapping = BTreeMap::new();
    for node in &nodes {
        for member in &node.members {
            d.check()?;
            ensure!(
                mapping.insert(member.reference, node.id.as_str()).is_none(),
                "Meaning group partition overlaps"
            );
        }
    }
    ensure!(
        mapping.len() == concepts.len(),
        "Meaning group partition is incomplete"
    );
    let mapped = |reference: Reference<'_>| -> Result<&str> {
        mapping
            .get(&reference)
            .copied()
            .context("Meaning group endpoint absent from parent")
    };
    let mut relations = Vec::new();
    let mut hyperedges = Vec::new();
    let mut evidence = BTreeMap::new();
    let mut packets = Vec::new();
    let mut relation_keys = BTreeSet::new();
    let mut hyperedge_keys = BTreeSet::new();
    for packet in rows(graph, "packets")? {
        d.check()?;
        let packet_index = packet_index(packet)?;
        let meaning = &packet["meaning"];
        for record in rows(meaning, "relations")? {
            d.check()?;
            let id = text(record, "id")?;
            ensure!(
                relation_keys.insert((packet_index, id)),
                "Duplicate meaning group parent relation"
            );
            relations.push(Relation {
                reference: RelationReference {
                    packet_index,
                    relation_id: id,
                },
                record,
                source: mapped(Reference {
                    packet_index,
                    concept_id: text(record, "source")?,
                })?,
                target: mapped(Reference {
                    packet_index,
                    concept_id: text(record, "target")?,
                })?,
            });
        }
        for record in rows(meaning, "hyperedges")? {
            d.check()?;
            let id = text(record, "id")?;
            ensure!(
                hyperedge_keys.insert((packet_index, id)),
                "Duplicate meaning group parent hyperedge"
            );
            let mut participants = Vec::new();
            for (position, slot) in rows(record, "participants")?.iter().enumerate() {
                d.check()?;
                let original = Reference {
                    packet_index,
                    concept_id: text(slot, "concept")?,
                };
                participants.push(Slot {
                    position,
                    role: text(slot, "role")?,
                    original,
                    node_id: mapped(original)?,
                });
            }
            hyperedges.push(Hyperedge {
                reference: HyperedgeReference {
                    packet_index,
                    hyperedge_id: id,
                },
                record,
                participants,
            });
        }
        for record in rows(meaning, "evidence")? {
            d.check()?;
            ensure!(
                evidence
                    .insert((packet_index, text(record, "id")?), record)
                    .is_none(),
                "Duplicate meaning group parent evidence"
            );
        }
        packets.push(PacketProof {
            packet_index,
            input_hash: &packet["inputHash"],
            provenance: &packet["provenance"],
            source_bindings: &meaning["sourceBindings"],
            unit_set_hash: &meaning["unitSetHash"],
            contract_hash: &meaning["contractHash"],
            coverage: &meaning["coverage"],
        });
    }
    relations.sort_by_key(|r| (r.reference.packet_index, r.reference.relation_id));
    hyperedges.sort_by_key(|r| (r.reference.packet_index, r.reference.hyperedge_id));
    packets.sort_by_key(|p| p.packet_index);
    d.check()?;
    let result = Report {
        schema_version: 1,
        kind: "meaning_group_projection_v1",
        projection_id: digest(
            &(
                "meaning-group-projection-v1",
                &review.graph_id,
                &admitted.policy_hash,
            ),
            512,
            d,
        )?,
        policy_hash: &admitted.policy_hash,
        graph_id: &review.graph_id,
        workspace_id: &graph["workspaceId"],
        snapshot_id: &graph["snapshotId"],
        job_id: &graph["jobId"],
        graph_hash: &graph["graphHash"],
        parent_coverage: &graph["coverage"],
        review,
        statistics,
        nodes: &nodes,
        relations,
        hyperedges,
        packets,
        evidence: evidence
            .into_iter()
            .map(|((packet_index, _), record)| Evidence {
                packet_index,
                record,
            })
            .collect(),
        grouping_authority: "explicit_host_assertion",
        binding_validation: "exact_captured_source_replayed",
        provider_attestation: "unverified",
        semantic_identity_established: false,
        original_concept_merges_applied: 0,
        model_calls: 0,
        complete: false,
        limitations: [
            "Host-selected groups and rationales are assertions, not authenticated human review or independent semantic identity proof.",
            "The separate projection retains all original claims, parallel directed relations, group-induced selfloops and ordered hyperedge slots; the parent is unchanged.",
            "Only one explicitly selected graph participates. No provider, capture, persistence, automatic grouping or cross-graph matching occurs.",
        ],
    };
    // Original records remain borrowed until the complete output has been counted.
    let bytes = crate::json::encode_bounded(&result, options.max_output_bytes, d)
        .context("Meaning group output exceeds capacity")?;
    let value = crate::json::parse(&bytes, options.max_output_bytes)?;
    d.check()?;
    Ok(value)
}

#[cfg(test)]
fn report(graph: &Value, review: &Review, options: &Options, d: &Deadline) -> Result<Value> {
    let admitted = admit(review, options, d)?;
    render(graph, &admitted, options, d)
}
#[cfg(test)]
#[path = "meaning_groups_tests.rs"]
mod tests;
