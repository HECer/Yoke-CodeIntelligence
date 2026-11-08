//! Pure deterministic arithmetic over already admitted rows; no source authority.
use crate::{
    capture::Deadline,
    retrieval_scopes::{RetrievalHardError, check_deadline},
};
use std::collections::BTreeMap;

#[derive(Clone, Debug)]
pub(super) struct AdmissionUnit {
    pub scope_root: String,
    pub path: String,
    pub start_byte: usize,
    pub end_byte: usize,
    pub entity_id: String,
    pub cost: usize,
}
#[derive(Debug)]
pub(super) struct AdmissionPlan {
    pub allowances: BTreeMap<String, usize>,
    pub phase1: Vec<usize>,
    pub phase2: Vec<usize>,
    pub omitted: Vec<usize>,
    pub used: usize,
}
pub(super) fn schedule_whole_units(
    units: &[AdmissionUnit],
    budget: usize,
    deadline: &Deadline,
) -> Result<AdmissionPlan, RetrievalHardError> {
    check_deadline(deadline)?;
    let mut queues = BTreeMap::<&str, Vec<usize>>::new();
    for (index, unit) in units.iter().enumerate() {
        check_deadline(deadline)?;
        if unit.start_byte > unit.end_byte {
            return Err(RetrievalHardError::SourceIntegrity("invalid_unit_range"));
        }
        queues.entry(&unit.scope_root).or_default().push(index);
    }
    for queue in queues.values_mut() {
        check_deadline(deadline)?;
        queue.sort_by(|left, right| {
            let left = &units[*left];
            let right = &units[*right];
            (&left.path, left.start_byte, left.end_byte, &left.entity_id).cmp(&(
                &right.path,
                right.start_byte,
                right.end_byte,
                &right.entity_id,
            ))
        });
    }
    check_deadline(deadline)?;
    let mut plan = AdmissionPlan {
        allowances: BTreeMap::new(),
        phase1: Vec::new(),
        phase2: Vec::new(),
        omitted: Vec::new(),
        used: 0,
    };
    if queues.is_empty() {
        return Ok(plan);
    }
    let base = budget / queues.len();
    let remainder = budget % queues.len();
    let mut remaining = Vec::with_capacity(queues.len());
    for (ordinal, root) in queues.keys().enumerate() {
        check_deadline(deadline)?;
        let allowance = base + usize::from(ordinal < remainder);
        plan.allowances.insert((*root).to_owned(), allowance);
        remaining.push(allowance);
    }
    let queues = queues.into_values().collect::<Vec<_>>();
    let mut cursors = vec![0; queues.len()];
    let mut admitted = vec![false; units.len()];
    loop {
        let mut progress = false;
        for (scope, queue) in queues.iter().enumerate() {
            check_deadline(deadline)?;
            while let Some(&index) = queue.get(cursors[scope]) {
                check_deadline(deadline)?;
                cursors[scope] += 1;
                let cost = units[index].cost;
                if cost <= remaining[scope] {
                    remaining[scope] -= cost;
                    plan.used += cost;
                    admitted[index] = true;
                    plan.phase1.push(index);
                    progress = true;
                    break;
                }
            }
        }
        if !progress {
            break;
        }
    }
    // Restart each cursor at its canonical queue head. Only deferred units are
    // eligible, and a unit exceeding the monotonically shrinking pool cannot
    // become eligible later in this phase.
    cursors.fill(0);
    let mut pool = budget - plan.used;
    loop {
        let mut progress = false;
        for (scope, queue) in queues.iter().enumerate() {
            check_deadline(deadline)?;
            while let Some(&index) = queue.get(cursors[scope]) {
                check_deadline(deadline)?;
                cursors[scope] += 1;
                if !admitted[index] && units[index].cost <= pool {
                    pool -= units[index].cost;
                    plan.used += units[index].cost;
                    admitted[index] = true;
                    plan.phase2.push(index);
                    progress = true;
                    break;
                }
            }
        }
        if !progress {
            break;
        }
    }
    for queue in queues {
        for index in queue {
            check_deadline(deadline)?;
            if !admitted[index] {
                plan.omitted.push(index);
            }
        }
    }
    Ok(plan)
}

#[derive(Clone, Debug)]
pub(super) struct ScoredRow {
    pub entity_id: String,
    pub scope_root: String,
    pub path: String,
    pub score: f64,
    pub query_coverage_count: usize,
}
#[derive(Debug, PartialEq, Eq)]
pub(super) struct RankedIndex {
    pub index: usize,
    pub local_rank: usize,
}
#[derive(Debug)]
pub(super) struct FusionPlan {
    pub selected: Vec<RankedIndex>,
    pub positive_candidate_count: usize,
    pub per_file_rejected_count: usize,
    pub final_limit_omission_count: usize,
}
pub(super) fn fuse_rows(
    rows: &[ScoredRow],
    per_file_limit: usize,
    limit: usize,
    deadline: &Deadline,
) -> Result<FusionPlan, RetrievalHardError> {
    check_deadline(deadline)?;
    let mut scopes = BTreeMap::<&str, Vec<usize>>::new();
    let mut positive_candidate_count = 0;
    for (index, row) in rows.iter().enumerate() {
        check_deadline(deadline)?;
        finite(row.score)?;
        if row.score > 0.0 {
            positive_candidate_count += 1;
            scopes.entry(&row.scope_root).or_default().push(index);
        }
    }
    let mut selected = Vec::new();
    let mut per_file_rejected_count = 0;
    for local in scopes.values_mut() {
        check_deadline(deadline)?;
        local.sort_by(|left, right| {
            let left = &rows[*left];
            let right = &rows[*right];
            right
                .score
                .total_cmp(&left.score)
                .then_with(|| right.query_coverage_count.cmp(&left.query_coverage_count))
                .then_with(|| left.entity_id.cmp(&right.entity_id))
        });
        check_deadline(deadline)?;
        let mut per_file = BTreeMap::<&str, usize>::new();
        let mut local_rank = 0;
        for &index in local.iter() {
            check_deadline(deadline)?;
            let count = per_file.entry(&rows[index].path).or_default();
            if *count >= per_file_limit {
                per_file_rejected_count += 1;
            } else {
                *count += 1;
                local_rank += 1;
                selected.push(RankedIndex { index, local_rank });
            }
        }
    }
    check_deadline(deadline)?;
    selected.sort_by(|left, right| {
        let left_row = &rows[left.index];
        let right_row = &rows[right.index];
        left.local_rank
            .cmp(&right.local_rank)
            .then_with(|| {
                right_row
                    .query_coverage_count
                    .cmp(&left_row.query_coverage_count)
            })
            .then_with(|| right_row.score.total_cmp(&left_row.score))
            .then_with(|| left_row.scope_root.cmp(&right_row.scope_root))
            .then_with(|| left_row.entity_id.cmp(&right_row.entity_id))
    });
    check_deadline(deadline)?;
    let final_limit_omission_count = selected.len().saturating_sub(limit);
    selected.truncate(limit);
    Ok(FusionPlan {
        selected,
        positive_candidate_count,
        per_file_rejected_count,
        final_limit_omission_count,
    })
}

#[derive(Clone, Debug)]
pub(super) struct LexicalDocument {
    pub entity_id: String,
    pub weighted_length: usize,
    pub term_counts: Vec<u32>,
}
pub(super) fn scope_bm25(
    documents: &[LexicalDocument],
    deadline: &Deadline,
) -> Result<BTreeMap<String, f64>, RetrievalHardError> {
    check_deadline(deadline)?;
    let term_count = documents.first().map_or(0, |row| row.term_counts.len());
    let mut frequencies = vec![0usize; term_count];
    let mut total_length = 0usize;
    for document in documents {
        check_deadline(deadline)?;
        if document.term_counts.len() != term_count {
            return Err(RetrievalHardError::Internal("inconsistent_query_terms"));
        }
        total_length = total_length
            .checked_add(document.weighted_length)
            .ok_or(RetrievalHardError::Capacity("weighted_length_sum"))?;
        for (term, &count) in document.term_counts.iter().enumerate() {
            check_deadline(deadline)?;
            if count > 0 {
                frequencies[term] += 1;
            }
        }
    }
    let mut scores = BTreeMap::new();
    if documents.is_empty() {
        return Ok(scores);
    }
    let count = documents.len() as f64;
    let average = (total_length as f64 / count).max(1.0);
    for document in documents {
        check_deadline(deadline)?;
        let mut score = 0.0;
        for (term, &frequency) in document.term_counts.iter().enumerate() {
            check_deadline(deadline)?;
            if frequency == 0 {
                continue;
            }
            let frequency = frequency as f64;
            let df = frequencies[term] as f64;
            let idf = (1.0 + (count - df + 0.5) / (df + 0.5)).ln();
            score += idf * frequency * 2.2
                / (frequency + 1.2 * (0.25 + 0.75 * document.weighted_length as f64 / average));
            finite(score)?;
        }
        if scores.insert(document.entity_id.clone(), score).is_some() {
            return Err(RetrievalHardError::Internal("duplicate_lexical_entity"));
        }
    }
    Ok(scores)
}
#[derive(Clone, Debug)]
pub(super) struct LexicalMassRow {
    pub entity_id: String,
    pub scope_root: String,
    pub bm25: f64,
}
pub(super) fn equal_scope_seeds(
    rows: &[LexicalMassRow],
    deadline: &Deadline,
) -> Result<BTreeMap<String, f64>, RetrievalHardError> {
    check_deadline(deadline)?;
    let mut scopes = BTreeMap::<&str, Vec<&LexicalMassRow>>::new();
    for row in rows {
        check_deadline(deadline)?;
        finite(row.bm25)?;
        if row.bm25 < 0.0 {
            return Err(RetrievalHardError::Internal("negative_lexical_mass"));
        }
        scopes.entry(&row.scope_root).or_default().push(row);
    }
    let mut masses = BTreeMap::new();
    let mut positive_scope_count = 0usize;
    for (&root, local) in scopes.iter_mut() {
        check_deadline(deadline)?;
        local.sort_by(|left, right| left.entity_id.cmp(&right.entity_id));
        let mut mass = 0.0;
        for row in local {
            check_deadline(deadline)?;
            mass += row.bm25;
            finite(mass)?;
        }
        positive_scope_count += usize::from(mass > 0.0);
        masses.insert(root, mass);
    }
    let mut seeds = BTreeMap::new();
    for (root, local) in scopes {
        check_deadline(deadline)?;
        let mass = masses[root];
        for row in local {
            check_deadline(deadline)?;
            let seed = if mass > 0.0 {
                row.bm25 / mass / positive_scope_count as f64
            } else {
                0.0
            };
            finite(seed)?;
            if seeds.insert(row.entity_id.clone(), seed).is_some() {
                return Err(RetrievalHardError::Internal("duplicate_lexical_entity"));
            }
        }
    }
    Ok(seeds)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum Orientation {
    Forward,
    Reverse,
}
#[derive(Clone, Debug)]
pub(super) struct ResolvedObservation {
    pub observation_id: String,
    pub source: String,
    pub target: Option<String>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct CanonicalArc {
    pub source: String,
    pub target: String,
    pub observation_id: String,
    pub orientation: Orientation,
}
pub(super) fn canonical_arcs(
    vertices: &[String],
    observations: &[ResolvedObservation],
    deadline: &Deadline,
) -> Result<Vec<CanonicalArc>, RetrievalHardError> {
    check_deadline(deadline)?;
    let mut admitted = BTreeMap::new();
    for vertex in vertices {
        check_deadline(deadline)?;
        if admitted.insert(vertex.as_str(), ()).is_some() {
            return Err(RetrievalHardError::Internal("duplicate_graph_vertex"));
        }
    }
    let mut arcs = Vec::new();
    for observation in observations {
        check_deadline(deadline)?;
        let Some(target) = observation.target.as_deref() else {
            continue;
        };
        if admitted.contains_key(observation.source.as_str()) && admitted.contains_key(target) {
            arcs.push(CanonicalArc {
                source: observation.source.clone(),
                target: target.to_owned(),
                observation_id: observation.observation_id.clone(),
                orientation: Orientation::Forward,
            });
            arcs.push(CanonicalArc {
                source: target.to_owned(),
                target: observation.source.clone(),
                observation_id: observation.observation_id.clone(),
                orientation: Orientation::Reverse,
            });
        }
    }
    check_deadline(deadline)?;
    arcs.sort_by(|left, right| {
        (
            &left.source,
            &left.target,
            &left.observation_id,
            left.orientation,
        )
            .cmp(&(
                &right.source,
                &right.target,
                &right.observation_id,
                right.orientation,
            ))
    });
    check_deadline(deadline)?;
    Ok(arcs)
}
#[derive(Debug)]
pub(super) struct GraphRanks {
    pub ranks: BTreeMap<String, f64>,
    pub iterations: usize,
}
pub(super) fn propagate(
    vertices: &[String],
    arcs: &[CanonicalArc],
    seeds: &BTreeMap<String, f64>,
    enabled: bool,
    deadline: &Deadline,
) -> Result<GraphRanks, RetrievalHardError> {
    check_deadline(deadline)?;
    let mut indices = BTreeMap::new();
    for vertex in vertices {
        check_deadline(deadline)?;
        if indices.insert(vertex.as_str(), 0usize).is_some() {
            return Err(RetrievalHardError::Internal("duplicate_graph_vertex"));
        }
    }
    for (index, ordinal) in indices.values_mut().enumerate() {
        check_deadline(deadline)?;
        *ordinal = index;
    }
    let mut seed = vec![0.0; indices.len()];
    let mut seed_mass = 0.0;
    for (vertex, &value) in seeds {
        check_deadline(deadline)?;
        finite(value)?;
        if value < 0.0 {
            return Err(RetrievalHardError::Internal("negative_graph_seed"));
        }
        let index = *indices
            .get(vertex.as_str())
            .ok_or(RetrievalHardError::Internal("unknown_graph_seed"))?;
        seed[index] = value;
        seed_mass += value;
        finite(seed_mass)?;
    }
    let mut ranks = vec![0.0; indices.len()];
    let mut iterations = 0;
    if enabled && seed_mass > 0.0 {
        let mut canonical = arcs.iter().collect::<Vec<_>>();
        check_deadline(deadline)?;
        canonical.sort_by(|left, right| {
            (
                &left.source,
                &left.target,
                &left.observation_id,
                left.orientation,
            )
                .cmp(&(
                    &right.source,
                    &right.target,
                    &right.observation_id,
                    right.orientation,
                ))
        });
        check_deadline(deadline)?;
        let mut outgoing = vec![Vec::new(); indices.len()];
        for arc in canonical {
            check_deadline(deadline)?;
            let source = *indices
                .get(arc.source.as_str())
                .ok_or(RetrievalHardError::Internal("unknown_graph_endpoint"))?;
            let target = *indices
                .get(arc.target.as_str())
                .ok_or(RetrievalHardError::Internal("unknown_graph_endpoint"))?;
            outgoing[source].push(target);
        }
        ranks.clone_from(&seed);
        for iteration in 1..=24 {
            check_deadline(deadline)?;
            let mut next = seed.iter().map(|value| value * 0.15).collect::<Vec<_>>();
            let mut dangling = 0.0;
            for (source, targets) in outgoing.iter().enumerate() {
                check_deadline(deadline)?;
                if targets.is_empty() {
                    dangling += ranks[source] * 0.85;
                    finite(dangling)?;
                } else {
                    let share = (ranks[source] * 0.85) / targets.len() as f64;
                    finite(share)?;
                    for &target in targets {
                        check_deadline(deadline)?;
                        next[target] += share;
                        finite(next[target])?;
                    }
                }
            }
            let mut delta = 0.0;
            for (index, value) in next.iter_mut().enumerate() {
                check_deadline(deadline)?;
                *value += dangling * seed[index];
                finite(*value)?;
                delta += (*value - ranks[index]).abs();
                finite(delta)?;
            }
            ranks = next;
            iterations = iteration;
            if delta < 1e-9 {
                break;
            }
        }
    }
    let mut result = BTreeMap::new();
    for (vertex, index) in indices {
        check_deadline(deadline)?;
        result.insert(vertex.to_owned(), ranks[index]);
    }
    Ok(GraphRanks {
        ranks: result,
        iterations,
    })
}

/// Serialization-cost rows only, never source descriptors or access grants.
/// Caller reserves mandatory scope/result bytes and every omitted-count field.
#[derive(Debug)]
pub(super) struct MetadataCostRows {
    pub diagnostics: Vec<usize>,
    pub scopes: Vec<usize>,
    pub declaration_bindings: Vec<usize>,
    pub source_omissions: Vec<usize>,
}
#[derive(Debug)]
pub(super) struct MetadataPrefixPlan {
    pub kept: [usize; 4],
    pub omitted: [usize; 4],
    pub optional_bytes_used: usize,
}
pub(super) fn plan_optional_metadata(
    rows: &MetadataCostRows,
    available_optional_bytes: usize,
    deadline: &Deadline,
) -> Result<MetadataPrefixPlan, RetrievalHardError> {
    check_deadline(deadline)?;
    let arrays = [
        (&rows.diagnostics, 64),
        (&rows.scopes, 64),
        (&rows.declaration_bindings, 128),
        (&rows.source_omissions, 200),
    ];
    let mut plan = MetadataPrefixPlan {
        kept: [0; 4],
        omitted: [0; 4],
        optional_bytes_used: 0,
    };
    let mut remaining = available_optional_bytes.min(256_000);
    for (array, (costs, cap)) in arrays.into_iter().enumerate() {
        check_deadline(deadline)?;
        for &cost in costs.iter().take(cap) {
            check_deadline(deadline)?;
            if cost > remaining {
                break;
            }
            remaining -= cost;
            plan.optional_bytes_used += cost;
            plan.kept[array] += 1;
        }
        plan.omitted[array] = costs.len() - plan.kept[array];
    }
    Ok(plan)
}

fn finite(value: f64) -> Result<(), RetrievalHardError> {
    if value.is_finite() {
        Ok(())
    } else {
        Err(RetrievalHardError::Internal("nonfinite_score"))
    }
}

#[cfg(test)]
#[path = "retrieval_balanced_tests.rs"]
mod tests;
