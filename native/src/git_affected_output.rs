//! Canonical, bounded projection of the private affected-walk arenas.

use super::index::Index;
use super::source::SourceCache;
use super::walk::WalkResult;
use super::{AffectedOptions, Budget, Eligibility, LIMITATIONS, Side, SourceAccess, capacity};
use anyhow::{Result, ensure};
use serde::Serialize;
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};
use std::cmp::Ordering;
use std::collections::BTreeSet;
use std::io::{self, Write};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Range {
    start_byte: usize,
    end_byte: usize,
    start_line: usize,
    end_line: usize,
}
impl From<(usize, usize, usize, usize)> for Range {
    fn from((start_byte, end_byte, start_line, end_line): (usize, usize, usize, usize)) -> Self {
        Self {
            start_byte,
            end_byte,
            start_line,
            end_line,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct EntityRow<'a> {
    side: &'a str,
    entity_id: &'a str,
    snapshot_id: &'a str,
    path: &'a str,
    content_hash: &'a str,
    name: &'a str,
    kind: &'a str,
    language: &'a str,
    producer: &'a str,
    parent_key: Option<&'a str>,
    range: Range,
    name_range: Option<Range>,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ObservationRow<'a> {
    side: &'a str,
    observation_id: &'a str,
    snapshot_id: &'a str,
    source_entity_id: &'a str,
    target_entity_id: Option<&'a str>,
    target_label: &'a str,
    relation: &'a str,
    resolution: &'a str,
    producer: &'a str,
    evidence_ids: &'a [String],
    relation_class: &'a str,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct EvidenceRow<'a> {
    side: &'a str,
    evidence_id: &'a str,
    snapshot_id: &'a str,
    path: &'a str,
    content_hash: &'a str,
    range: Range,
    anchor: &'a str,
    trust: &'a str,
    producer: &'a str,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct OriginalEligibility<'a> {
    status: &'a str,
    reason: Option<&'a str>,
    owner_entity_id: Option<&'a str>,
}
#[derive(Serialize)]
struct MemberEligibility<'a> {
    status: &'a str,
    reason: Option<&'a str>,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct OriginalRow<'a> {
    side: &'a str,
    entity_id: &'a str,
    changed_row_indices: &'a [usize],
    eligibility: OriginalEligibility<'a>,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct EntityRef<'a> {
    side: &'a str,
    entity_id: &'a str,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ImpactRef<'a> {
    original_row_index: usize,
    side: &'a str,
    entity_id: &'a str,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ImpactObservationRef<'a> {
    original_row_index: usize,
    side: &'a str,
    observation_id: &'a str,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct HierarchyRow<'a> {
    side: &'a str,
    root_entity_id: &'a str,
    direction: &'a str,
    entity_id: &'a str,
    distance: usize,
    witness_observation_ids: &'a [&'a str],
    boundary: bool,
    unexamined_adjacency_count: usize,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct MemberRow<'a> {
    side: &'a str,
    root_entity_id: &'a str,
    direction: &'a str,
    type_entity_id: &'a str,
    member_entity_id: &'a str,
    hierarchy_state_index: usize,
    eligibility: MemberEligibility<'a>,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct WalkRow<'a> {
    side: &'a str,
    root_entity_id: &'a str,
    entity_id: &'a str,
    origin: &'a str,
    member_reason_indices: &'a [usize],
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct IncomingRow<'a> {
    side: &'a str,
    root_entity_id: &'a str,
    walk_seed_entity_id: &'a str,
    entity_id: &'a str,
    distance: usize,
    witness_observation_ids: &'a [&'a str],
    boundary: bool,
    unexamined_adjacency_count: usize,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ProofRow<'a> {
    state_index: usize,
    observation_id: &'a str,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Input<'a> {
    snapshot_id: &'a str,
    graph_hash: &'a str,
    analyzer_epoch: &'a str,
}
#[derive(Serialize)]
struct Sides<T> {
    base: T,
    head: T,
}
#[derive(Serialize)]
struct GraphCounts {
    entities: usize,
    observations: usize,
    evidence: usize,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Consumed {
    graphs: Sides<GraphCounts>,
    original_seed_rows: usize,
    unique_expanded_seeds: usize,
    hierarchy_states: usize,
    incoming_states: usize,
    states: usize,
    index_observation_scans: usize,
    hierarchy_adjacency_visits: usize,
    incoming_adjacency_visits: usize,
    edge_visits: usize,
    new_evidence_source_bytes: usize,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct HierarchyCoverage {
    unresolved_observations: usize,
    unsupported_observations: usize,
    boundary_states: usize,
    unexamined_adjacency: usize,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct IncomingCoverage {
    unresolved_observations: usize,
    boundary_states: usize,
    unexamined_adjacency: usize,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Coverage {
    within_policy_complete: bool,
    hierarchy: HierarchyCoverage,
    incoming: IncomingCoverage,
    unsupported_original_seeds: usize,
    unsupported_member_reasons: usize,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Metadata<'a> {
    schema_version: usize,
    kind: &'a str,
    algorithm_version: &'a str,
    policy: &'a str,
    result_id: &'a str,
    report_artifact_id: &'a str,
    workspace_id: &'a str,
    inputs: Sides<Input<'a>>,
    options: &'a AffectedOptions,
    coverage: Coverage,
    consumed: Consumed,
    row_units: usize,
    wire_bytes: usize,
    inference_class: &'a str,
    complete: bool,
    compiler_binding_established: bool,
    runtime_dispatch_established: bool,
    limitations: &'a [&'a str],
}

fn total(values: impl IntoIterator<Item = usize>) -> Result<usize> {
    values
        .into_iter()
        .try_fold(0usize, |a, b| a.checked_add(b).ok_or_else(capacity))
}

/// A fallible bottom-up merge sort checks cancellation during comparisons.
fn ordered<T>(
    values: &[T],
    budget: &Budget<'_>,
    compare: impl Fn(&T, &T) -> Ordering,
) -> Result<Vec<usize>> {
    budget.check()?;
    let mut order: Vec<_> = (0..values.len()).collect();
    let mut scratch = vec![0; values.len()];
    let mut width = 1usize;
    while width < values.len() {
        let step = width.checked_mul(2).ok_or_else(capacity)?;
        for start in (0..values.len()).step_by(step) {
            let middle = start.saturating_add(width).min(values.len());
            let end = start.saturating_add(step).min(values.len());
            let (mut left, mut right) = (start, middle);
            for slot in &mut scratch[start..end] {
                budget.check()?;
                if left < middle
                    && (right == end
                        || compare(&values[order[left]], &values[order[right]])
                            != Ordering::Greater)
                {
                    *slot = order[left];
                    left += 1;
                } else {
                    *slot = order[right];
                    right += 1;
                }
                #[cfg(test)]
                super::observe_work(super::WorkPhase::OutputSort);
            }
        }
        std::mem::swap(&mut order, &mut scratch);
        width = step;
    }
    budget.check()?;
    Ok(order)
}

fn remap(order: &[usize], budget: &Budget<'_>) -> Result<Vec<usize>> {
    let mut result = vec![0; order.len()];
    for (new, old) in order.iter().copied().enumerate() {
        budget.check()?;
        result[old] = new;
    }
    Ok(result)
}
fn append<T: Serialize>(table: &mut Vec<Value>, row: &T, budget: &mut Budget<'_>) -> Result<()> {
    // Only this row's disjoint final bytes are reserved, before copying strings.
    budget.reserve_serialized(row)?;
    table.push(serde_json::to_value(row)?);
    budget.check()
}
fn entity_id<'a>(indexes: &'a [Index<'_>; 2], side: Side, entity: usize) -> &'a str {
    &indexes[side.index()].graph.entities[entity].id
}
fn observation_id<'a>(indexes: &'a [Index<'_>; 2], side: Side, observation: usize) -> &'a str {
    &indexes[side.index()].graph.observations[observation].id
}
fn row_binding(row: &Value) -> Result<(Side, &str)> {
    let side = Side::parse(&row["side"])?;
    let id = row["id"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("Invalid original row identity"))?;
    Ok((side, id))
}

#[derive(Default)]
struct Required {
    entities: BTreeSet<(Side, usize)>,
    observations: BTreeSet<(Side, usize)>,
    evidence: BTreeSet<(Side, usize)>,
}
impl Required {
    fn entity(
        &mut self,
        indexes: &[Index<'_>; 2],
        side: Side,
        entity: usize,
        budget: &mut Budget<'_>,
    ) -> Result<()> {
        ensure!(
            entity < indexes[side.index()].graph.entities.len(),
            "Invalid affected entity index"
        );
        if !self.entities.contains(&(side, entity)) {
            budget.charge_rows(1)?;
            self.entities.insert((side, entity));
        }
        Ok(())
    }
    fn observation(
        &mut self,
        indexes: &[Index<'_>; 2],
        side: Side,
        observation: usize,
        budget: &mut Budget<'_>,
    ) -> Result<()> {
        let index = &indexes[side.index()];
        let row = index
            .graph
            .observations
            .get(observation)
            .ok_or_else(|| anyhow::anyhow!("Invalid affected proof index"))?;
        if self.observations.contains(&(side, observation)) {
            return Ok(());
        }
        budget.charge_rows(row.evidence_ids.len().checked_add(1).ok_or_else(capacity)?)?;
        self.observations.insert((side, observation));
        let source = *index
            .entities
            .get(row.source.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing proof source"))?;
        self.entity(indexes, side, source, budget)?;
        if let Some(target) = &row.target {
            let target = *index
                .entities
                .get(target.as_str())
                .ok_or_else(|| anyhow::anyhow!("Missing proof target"))?;
            self.entity(indexes, side, target, budget)?;
        }
        for id in &row.evidence_ids {
            budget.check()?;
            let evidence = *index
                .evidence
                .get(id.as_str())
                .ok_or_else(|| anyhow::anyhow!("Missing proof evidence"))?;
            if !self.evidence.contains(&(side, evidence)) {
                budget.charge_rows(1)?;
                self.evidence.insert((side, evidence));
            }
        }
        Ok(())
    }
}

pub(super) fn finish(
    access: &dyn SourceAccess,
    indexes: &[Index<'_>; 2],
    sources: &mut SourceCache<'_>,
    walk: &WalkResult,
    budget: &mut Budget<'_>,
) -> Result<(Value, Vec<u8>)> {
    budget.check()?;
    let mut required = Required::default();
    for root in &walk.originals {
        required.entity(indexes, root.side, root.entity, budget)?;
    }
    for row in access.impact() {
        budget.check()?;
        let (side, id) = row_binding(row)?;
        let entity = *indexes[side.index()]
            .entities
            .get(id)
            .ok_or_else(|| anyhow::anyhow!("Missing baseline impact entity"))?;
        required.entity(indexes, side, entity, budget)?;
    }
    for state in &walk.hierarchy {
        for entity in [state.root, state.entity] {
            required.entity(indexes, state.side, entity, budget)?;
        }
        for observation in &state.witness {
            required.observation(indexes, state.side, *observation, budget)?;
        }
    }
    for reason in &walk.members {
        for entity in [reason.root, reason.type_entity, reason.member] {
            required.entity(indexes, reason.side, entity, budget)?;
        }
    }
    for seed in &walk.walk_seeds {
        for entity in [seed.root, seed.entity] {
            required.entity(indexes, seed.side, entity, budget)?;
        }
    }
    for state in &walk.incoming {
        for entity in [state.root, state.seed, state.entity] {
            required.entity(indexes, state.side, entity, budget)?;
        }
        for observation in &state.witness {
            required.observation(indexes, state.side, *observation, budget)?;
        }
    }
    for proof in &walk.hierarchy_proofs {
        let state = walk
            .hierarchy
            .get(proof.state)
            .ok_or_else(|| anyhow::anyhow!("Invalid hierarchy proof state"))?;
        required.observation(indexes, state.side, proof.observation, budget)?;
    }
    for proof in &walk.incoming_proofs {
        let state = walk
            .incoming
            .get(proof.state)
            .ok_or_else(|| anyhow::anyhow!("Invalid incoming proof state"))?;
        required.observation(indexes, state.side, proof.observation, budget)?;
    }
    for &(side, entity) in &walk.expanded {
        required.entity(indexes, side, entity, budget)?;
    }

    let mut entities = Vec::new();
    let needed_entities: Vec<_> = required.entities.into_iter().collect();
    for position in ordered(&needed_entities, budget, |a, b| {
        (a.0, entity_id(indexes, a.0, a.1)).cmp(&(b.0, entity_id(indexes, b.0, b.1)))
    })? {
        let (side, entity) = needed_entities[position];
        let index = &indexes[side.index()];
        sources.validate_entity(index, entity, budget)?;
        let row = &index.graph.entities[entity];
        // Project canonical lines from captured byte intervals, including legacy
        // admitted evidence whose stored endLine used the alternate LF boundary.
        let range = sources
            .range(index, &row.path, row.start_byte, row.end_byte, budget)?
            .into();
        let name_range = sources.name_range(index, entity, budget)?.map(Range::from);
        let file = access.file(side, &row.path)?;
        append(
            &mut entities,
            &EntityRow {
                side: side.label(),
                entity_id: &row.id,
                snapshot_id: &row.snapshot_id,
                path: &row.path,
                content_hash: &file.hash,
                name: &row.name,
                kind: &row.kind,
                language: &row.language,
                producer: &row.producer,
                parent_key: row.parent_key.as_deref(),
                range,
                name_range,
            },
            budget,
        )?;
    }
    let mut observations = Vec::new();
    let needed_observations: Vec<_> = required.observations.into_iter().collect();
    for position in ordered(&needed_observations, budget, |a, b| {
        (a.0, observation_id(indexes, a.0, a.1)).cmp(&(b.0, observation_id(indexes, b.0, b.1)))
    })? {
        let (side, observation) = needed_observations[position];
        let row = &indexes[side.index()].graph.observations[observation];
        let relation_class = match row.relation.as_str() {
            "implements" => "contract_membership",
            "extends" | "inherits" => "structural_inheritance",
            "calls" | "references" => "incoming_candidate",
            _ => anyhow::bail!("Invalid affected proof relation"),
        };
        append(
            &mut observations,
            &ObservationRow {
                side: side.label(),
                observation_id: &row.id,
                snapshot_id: &row.snapshot_id,
                source_entity_id: &row.source,
                target_entity_id: row.target.as_deref(),
                target_label: &row.target_label,
                relation: &row.relation,
                resolution: &row.resolution,
                producer: &row.producer,
                evidence_ids: &row.evidence_ids,
                relation_class,
            },
            budget,
        )?;
    }
    let mut evidence = Vec::new();
    let needed_evidence: Vec<_> = required.evidence.into_iter().collect();
    for position in ordered(&needed_evidence, budget, |a, b| {
        (a.0, &indexes[a.0.index()].graph.evidence[a.1].id)
            .cmp(&(b.0, &indexes[b.0.index()].graph.evidence[b.1].id))
    })? {
        let (side, evidence_index) = needed_evidence[position];
        let index = &indexes[side.index()];
        sources.validate_evidence(index, evidence_index, budget)?;
        let row = &index.graph.evidence[evidence_index];
        let range = sources
            .range(index, &row.path, row.start_byte, row.end_byte, budget)?
            .into();
        append(
            &mut evidence,
            &EvidenceRow {
                side: side.label(),
                evidence_id: &row.id,
                snapshot_id: &row.snapshot_id,
                path: &row.path,
                content_hash: &row.content_hash,
                range,
                anchor: &row.anchor,
                trust: &row.trust,
                producer: &row.producer,
            },
            budget,
        )?;
    }

    let mut original_seeds = Vec::new();
    for position in ordered(&walk.originals, budget, |a, b| {
        (a.side, entity_id(indexes, a.side, a.entity))
            .cmp(&(b.side, entity_id(indexes, b.side, b.entity)))
    })? {
        let root = &walk.originals[position];
        let changed_order = ordered(&root.changed_rows, budget, usize::cmp)?;
        let changed_rows: Vec<_> = changed_order
            .into_iter()
            .map(|i| root.changed_rows[i])
            .collect();
        let eligibility = match root.eligibility {
            Eligibility::Expandable { owner } => OriginalEligibility {
                status: "expandable",
                reason: None,
                owner_entity_id: Some(entity_id(indexes, root.side, owner)),
            },
            Eligibility::Unsupported(reason) => OriginalEligibility {
                status: "unsupported",
                reason: Some(reason),
                owner_entity_id: None,
            },
        };
        append(
            &mut original_seeds,
            &OriginalRow {
                side: root.side.label(),
                entity_id: entity_id(indexes, root.side, root.entity),
                changed_row_indices: &changed_rows,
                eligibility,
            },
            budget,
        )?;
    }
    let mut original_impact_refs = Vec::new();
    for (original_row_index, row) in access.impact().iter().enumerate() {
        let (side, id) = row_binding(row)?;
        budget.charge_rows(1)?;
        append(
            &mut original_impact_refs,
            &ImpactRef {
                original_row_index,
                side: side.label(),
                entity_id: id,
            },
            budget,
        )?;
    }
    let mut original_impact_observation_refs = Vec::new();
    for (original_row_index, row) in access.impact_observations().iter().enumerate() {
        let (side, id) = row_binding(row)?;
        ensure!(
            indexes[side.index()].observations.contains_key(id),
            "Missing baseline impact observation"
        );
        budget.charge_rows(1)?;
        append(
            &mut original_impact_observation_refs,
            &ImpactObservationRef {
                original_row_index,
                side: side.label(),
                observation_id: id,
            },
            budget,
        )?;
    }
    let mut expanded_seeds = Vec::new();
    let expanded: Vec<_> = walk.expanded.iter().copied().collect();
    for position in ordered(&expanded, budget, |a, b| {
        (a.0, entity_id(indexes, a.0, a.1)).cmp(&(b.0, entity_id(indexes, b.0, b.1)))
    })? {
        let (side, entity) = expanded[position];
        append(
            &mut expanded_seeds,
            &EntityRef {
                side: side.label(),
                entity_id: entity_id(indexes, side, entity),
            },
            budget,
        )?;
    }

    let hierarchy_order = ordered(&walk.hierarchy, budget, |a, b| {
        (
            a.side,
            entity_id(indexes, a.side, a.root),
            a.direction,
            a.distance,
            entity_id(indexes, a.side, a.entity),
        )
            .cmp(&(
                b.side,
                entity_id(indexes, b.side, b.root),
                b.direction,
                b.distance,
                entity_id(indexes, b.side, b.entity),
            ))
    })?;
    let hierarchy_remap = remap(&hierarchy_order, budget)?;
    let mut hierarchy_states = Vec::new();
    for &position in &hierarchy_order {
        let state = &walk.hierarchy[position];
        let witness: Vec<_> = state
            .witness
            .iter()
            .map(|i| observation_id(indexes, state.side, *i))
            .collect();
        append(
            &mut hierarchy_states,
            &HierarchyRow {
                side: state.side.label(),
                root_entity_id: entity_id(indexes, state.side, state.root),
                direction: state.direction.label(),
                entity_id: entity_id(indexes, state.side, state.entity),
                distance: state.distance,
                witness_observation_ids: &witness,
                boundary: state.boundary,
                unexamined_adjacency_count: state.unexamined,
            },
            budget,
        )?;
    }
    let mut hierarchy_proof_refs = Vec::new();
    for position in ordered(&walk.hierarchy_proofs, budget, |a, b| {
        let a_side = walk.hierarchy[a.state].side;
        let b_side = walk.hierarchy[b.state].side;
        (
            hierarchy_remap[a.state],
            observation_id(indexes, a_side, a.observation),
        )
            .cmp(&(
                hierarchy_remap[b.state],
                observation_id(indexes, b_side, b.observation),
            ))
    })? {
        let proof = &walk.hierarchy_proofs[position];
        append(
            &mut hierarchy_proof_refs,
            &ProofRow {
                state_index: hierarchy_remap[proof.state],
                observation_id: observation_id(
                    indexes,
                    walk.hierarchy[proof.state].side,
                    proof.observation,
                ),
            },
            budget,
        )?;
    }
    let member_order = ordered(&walk.members, budget, |a, b| {
        (
            a.side,
            entity_id(indexes, a.side, a.root),
            a.direction,
            walk.hierarchy[a.state].distance,
            entity_id(indexes, a.side, a.type_entity),
            entity_id(indexes, a.side, a.member),
        )
            .cmp(&(
                b.side,
                entity_id(indexes, b.side, b.root),
                b.direction,
                walk.hierarchy[b.state].distance,
                entity_id(indexes, b.side, b.type_entity),
                entity_id(indexes, b.side, b.member),
            ))
    })?;
    let member_remap = remap(&member_order, budget)?;
    let mut member_reasons = Vec::new();
    for position in member_order {
        let reason = &walk.members[position];
        let eligibility = match reason.eligibility {
            Eligibility::Expandable { .. } => MemberEligibility {
                status: "eligible",
                reason: None,
            },
            Eligibility::Unsupported(reason) => MemberEligibility {
                status: "unsupported",
                reason: Some(reason),
            },
        };
        append(
            &mut member_reasons,
            &MemberRow {
                side: reason.side.label(),
                root_entity_id: entity_id(indexes, reason.side, reason.root),
                direction: reason.direction.label(),
                type_entity_id: entity_id(indexes, reason.side, reason.type_entity),
                member_entity_id: entity_id(indexes, reason.side, reason.member),
                hierarchy_state_index: hierarchy_remap[reason.state],
                eligibility,
            },
            budget,
        )?;
    }
    let mut walk_seeds = Vec::new();
    for position in ordered(&walk.walk_seeds, budget, |a, b| {
        (
            a.side,
            entity_id(indexes, a.side, a.root),
            entity_id(indexes, a.side, a.entity),
        )
            .cmp(&(
                b.side,
                entity_id(indexes, b.side, b.root),
                entity_id(indexes, b.side, b.entity),
            ))
    })? {
        let seed = &walk.walk_seeds[position];
        let mapped: Vec<_> = seed.reasons.iter().map(|i| member_remap[*i]).collect();
        let order = ordered(&mapped, budget, usize::cmp)?;
        let reasons: Vec<_> = order.into_iter().map(|i| mapped[i]).collect();
        append(
            &mut walk_seeds,
            &WalkRow {
                side: seed.side.label(),
                root_entity_id: entity_id(indexes, seed.side, seed.root),
                entity_id: entity_id(indexes, seed.side, seed.entity),
                origin: if seed.original {
                    "original"
                } else {
                    "hierarchy"
                },
                member_reason_indices: &reasons,
            },
            budget,
        )?;
    }
    let incoming_order = ordered(&walk.incoming, budget, |a, b| {
        (
            a.side,
            entity_id(indexes, a.side, a.root),
            entity_id(indexes, a.side, a.seed),
            a.distance,
            entity_id(indexes, a.side, a.entity),
        )
            .cmp(&(
                b.side,
                entity_id(indexes, b.side, b.root),
                entity_id(indexes, b.side, b.seed),
                b.distance,
                entity_id(indexes, b.side, b.entity),
            ))
    })?;
    let incoming_remap = remap(&incoming_order, budget)?;
    let mut incoming_states = Vec::new();
    for position in incoming_order {
        let state = &walk.incoming[position];
        let witness: Vec<_> = state
            .witness
            .iter()
            .map(|i| observation_id(indexes, state.side, *i))
            .collect();
        append(
            &mut incoming_states,
            &IncomingRow {
                side: state.side.label(),
                root_entity_id: entity_id(indexes, state.side, state.root),
                walk_seed_entity_id: entity_id(indexes, state.side, state.seed),
                entity_id: entity_id(indexes, state.side, state.entity),
                distance: state.distance,
                witness_observation_ids: &witness,
                boundary: state.boundary,
                unexamined_adjacency_count: state.unexamined,
            },
            budget,
        )?;
    }
    let mut incoming_proof_refs = Vec::new();
    for position in ordered(&walk.incoming_proofs, budget, |a, b| {
        let a_side = walk.incoming[a.state].side;
        let b_side = walk.incoming[b.state].side;
        (
            incoming_remap[a.state],
            observation_id(indexes, a_side, a.observation),
        )
            .cmp(&(
                incoming_remap[b.state],
                observation_id(indexes, b_side, b.observation),
            ))
    })? {
        let proof = &walk.incoming_proofs[position];
        append(
            &mut incoming_proof_refs,
            &ProofRow {
                state_index: incoming_remap[proof.state],
                observation_id: observation_id(
                    indexes,
                    walk.incoming[proof.state].side,
                    proof.observation,
                ),
            },
            budget,
        )?;
    }

    ensure!(
        budget.counts.hierarchy_states == walk.hierarchy.len()
            && budget.counts.incoming_states == walk.incoming.len(),
        "Affected state accounting mismatch"
    );
    ensure!(
        budget.counts.hierarchy_adjacency_visits == walk.hierarchy_proofs.len()
            && budget.counts.incoming_adjacency_visits == walk.incoming_proofs.len(),
        "Affected proof accounting mismatch"
    );
    budget.charge_rows(LIMITATIONS.len())?;
    let graph_counts = |side: Side| {
        let graph = access.graph(side);
        GraphCounts {
            entities: graph.entities.len(),
            observations: graph.observations.len(),
            evidence: graph.evidence.len(),
        }
    };
    let input = |side: Side| Input {
        snapshot_id: &access.snapshot(side).id,
        graph_hash: access.graph_hash(side),
        analyzer_epoch: access.analyzer_epoch(side).producer(),
    };
    let counts = &budget.counts;
    let options = budget.options;
    let metadata = Metadata {
        schema_version: 1,
        kind: "git_affected_details_v1",
        algorithm_version: "yoke_git_affected_details_1",
        policy: &options.policy,
        result_id: "0000000000000000000000000000000000000000000000000000000000000000",
        report_artifact_id: access.report_artifact_id(),
        workspace_id: access.workspace_id(),
        inputs: Sides {
            base: input(Side::Base),
            head: input(Side::Head),
        },
        options,
        coverage: Coverage {
            within_policy_complete: true,
            hierarchy: HierarchyCoverage {
                unresolved_observations: total(indexes.iter().map(|i| i.hierarchy_unresolved))?,
                unsupported_observations: total(indexes.iter().map(|i| i.hierarchy_unsupported))?,
                boundary_states: walk.hierarchy.iter().filter(|s| s.boundary).count(),
                unexamined_adjacency: total(walk.hierarchy.iter().map(|s| s.unexamined))?,
            },
            incoming: IncomingCoverage {
                unresolved_observations: total(indexes.iter().map(|i| i.incoming_unresolved))?,
                boundary_states: walk.incoming.iter().filter(|s| s.boundary).count(),
                unexamined_adjacency: total(walk.incoming.iter().map(|s| s.unexamined))?,
            },
            unsupported_original_seeds: walk
                .originals
                .iter()
                .filter(|s| matches!(s.eligibility, Eligibility::Unsupported(_)))
                .count(),
            unsupported_member_reasons: walk
                .members
                .iter()
                .filter(|s| matches!(s.eligibility, Eligibility::Unsupported(_)))
                .count(),
        },
        consumed: Consumed {
            graphs: Sides {
                base: graph_counts(Side::Base),
                head: graph_counts(Side::Head),
            },
            original_seed_rows: counts.original_seed_rows,
            unique_expanded_seeds: counts.unique_expanded_seeds,
            hierarchy_states: counts.hierarchy_states,
            incoming_states: counts.incoming_states,
            states: total([counts.hierarchy_states, counts.incoming_states])?,
            index_observation_scans: counts.index_observation_scans,
            hierarchy_adjacency_visits: counts.hierarchy_adjacency_visits,
            incoming_adjacency_visits: counts.incoming_adjacency_visits,
            edge_visits: total([
                counts.index_observation_scans,
                counts.hierarchy_adjacency_visits,
                counts.incoming_adjacency_visits,
            ])?,
            new_evidence_source_bytes: counts.new_evidence_source_bytes,
        },
        row_units: budget.rows,
        wire_bytes: 0,
        inference_class: "source_bound_structural_candidate",
        complete: false,
        compiler_binding_established: false,
        runtime_dispatch_established: false,
        limitations: &LIMITATIONS,
    };
    budget.reserve_serialized(&metadata)?;
    let mut value = serde_json::to_value(metadata)?;
    let object = value
        .as_object_mut()
        .ok_or_else(|| anyhow::anyhow!("Invalid affected metadata"))?;
    for (name, rows) in [
        ("entities", entities),
        ("observations", observations),
        ("evidence", evidence),
        ("originalSeeds", original_seeds),
        ("originalImpactRefs", original_impact_refs),
        (
            "originalImpactObservationRefs",
            original_impact_observation_refs,
        ),
        ("expandedSeeds", expanded_seeds),
        ("hierarchyStates", hierarchy_states),
        ("hierarchyProofRefs", hierarchy_proof_refs),
        ("memberReasons", member_reasons),
        ("walkSeeds", walk_seeds),
        ("incomingStates", incoming_states),
        ("incomingProofRefs", incoming_proof_refs),
    ] {
        object.insert(name.to_owned(), Value::Array(rows));
    }
    ensure!(
        recursive_rows(&value, budget)? == budget.rows,
        "Affected recursive row accounting mismatch"
    );
    seal(value, budget)
}

fn recursive_rows(value: &Value, budget: &Budget<'_>) -> Result<usize> {
    budget.check()?;
    match value {
        Value::Array(rows) => rows.iter().try_fold(rows.len(), |count, row| {
            count
                .checked_add(recursive_rows(row, budget)?)
                .ok_or_else(capacity)
        }),
        Value::Object(fields) => fields.values().try_fold(0usize, |count, row| {
            count
                .checked_add(recursive_rows(row, budget)?)
                .ok_or_else(capacity)
        }),
        _ => Ok(0),
    }
}

struct BoundedWriter<'a, 'b, W> {
    inner: W,
    budget: &'a Budget<'b>,
    written: usize,
    exceeded: bool,
}
impl<W: Write> Write for BoundedWriter<'_, '_, W> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.budget
            .check()
            .map_err(|_| io::Error::other("deadline"))?;
        let Some(next) = self.written.checked_add(bytes.len()) else {
            self.exceeded = true;
            return Err(io::Error::other("capacity"));
        };
        if next > self.budget.options.max_output_bytes {
            self.exceeded = true;
            return Err(io::Error::other("capacity"));
        }
        self.inner.write_all(bytes)?;
        self.written = next;
        #[cfg(test)]
        super::observe_work(super::WorkPhase::CanonicalEncoding);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}
fn canonical<W: Write>(
    value: &Value,
    out: &mut W,
    budget: &Budget<'_>,
    omit_result_id: bool,
) -> Result<()> {
    budget.check()?;
    match value {
        Value::Object(fields) => {
            out.write_all(b"{")?;
            canonical_fields(fields, out, budget, omit_result_id)?;
            out.write_all(b"}")?;
        }
        Value::Array(rows) => {
            out.write_all(b"[")?;
            for (i, row) in rows.iter().enumerate() {
                budget.check()?;
                if i != 0 {
                    out.write_all(b",")?;
                }
                canonical(row, out, budget, false)?;
            }
            out.write_all(b"]")?;
        }
        Value::Number(number) => {
            ensure!(
                number.is_u64() || number.is_i64(),
                "Noninteger affected output"
            );
            serde_json::to_writer(out, number)?;
        }
        _ => serde_json::to_writer(out, value)?,
    }
    Ok(())
}
fn canonical_fields<W: Write>(
    fields: &Map<String, Value>,
    out: &mut W,
    budget: &Budget<'_>,
    omit_result_id: bool,
) -> Result<()> {
    let keys: Vec<_> = fields
        .keys()
        .filter(|key| !(omit_result_id && key.as_str() == "resultId"))
        .collect();
    let mut first = true;
    for position in ordered(&keys, budget, |a, b| a.as_bytes().cmp(b.as_bytes()))? {
        budget.check()?;
        let key = keys[position];
        if !first {
            out.write_all(b",")?;
        }
        first = false;
        serde_json::to_writer(&mut *out, key)?;
        out.write_all(b":")?;
        canonical(&fields[key], out, budget, false)?;
    }
    Ok(())
}
fn stream<W: Write>(
    value: &Value,
    inner: W,
    budget: &Budget<'_>,
    omit_result_id: bool,
) -> Result<(W, usize)> {
    let mut writer = BoundedWriter {
        inner,
        budget,
        written: 0,
        exceeded: false,
    };
    if let Err(error) = canonical(value, &mut writer, budget, omit_result_id) {
        budget.check()?;
        if writer.exceeded {
            return Err(capacity());
        }
        return Err(error);
    }
    budget.ensure_wire(writer.written)?;
    Ok((writer.inner, writer.written))
}
struct HashWriter(Sha256);
impl Write for HashWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.0.update(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
fn seal(mut value: Value, budget: &Budget<'_>) -> Result<(Value, Vec<u8>)> {
    let mut settled = None;
    for _ in 0..32 {
        let (_, length) = stream(&value, io::sink(), budget, false)?;
        if value["wireBytes"].as_u64() == Some(length as u64) {
            settled = Some(length);
            break;
        }
        value["wireBytes"] = Value::from(length);
    }
    let length = settled.ok_or_else(capacity)?;
    let mut digest = Sha256::new();
    digest.update(b"yoke_git_affected_details_1\0");
    let (digest, _) = stream(&value, HashWriter(digest), budget, true)?;
    value["resultId"] = Value::String(hex::encode(digest.0.finalize()));
    budget.ensure_wire(length)?;
    let (bytes, written) = stream(&value, Vec::with_capacity(length), budget, false)?;
    ensure!(
        written == length && bytes.len() == length,
        "Affected wire fixed point changed"
    );
    budget.check()?;
    Ok((value, bytes))
}
