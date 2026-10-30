//! Finite, source-bound hierarchy and incoming walks over admitted captured rows.

use super::index::Index;
use super::source::SourceCache;
use super::{Budget, Eligibility, Side, SourceAccess};
use anyhow::{Context, Result, bail};
use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};

const INVALID: &str = "git_affected_details_input_invalid";

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum Direction {
    Ancestor,
    Descendant,
}

impl Direction {
    pub(super) fn label(self) -> &'static str {
        match self {
            Self::Ancestor => "ancestor",
            Self::Descendant => "descendant",
        }
    }
}

pub(super) struct OriginalSeed {
    pub(super) side: Side,
    pub(super) entity: usize,
    pub(super) changed_rows: Vec<usize>,
    pub(super) eligibility: Eligibility,
}

pub(super) struct HierarchyState {
    pub(super) side: Side,
    pub(super) root: usize,
    pub(super) direction: Direction,
    pub(super) entity: usize,
    pub(super) distance: usize,
    pub(super) witness: Vec<usize>,
    pub(super) boundary: bool,
    pub(super) unexamined: usize,
}

pub(super) struct ProofRef {
    pub(super) state: usize,
    pub(super) observation: usize,
}

pub(super) struct MemberReason {
    pub(super) side: Side,
    pub(super) root: usize,
    pub(super) direction: Direction,
    pub(super) type_entity: usize,
    pub(super) member: usize,
    pub(super) state: usize,
    pub(super) eligibility: Eligibility,
}

pub(super) struct WalkSeed {
    pub(super) side: Side,
    pub(super) root: usize,
    pub(super) entity: usize,
    pub(super) original: bool,
    pub(super) reasons: Vec<usize>,
}

pub(super) struct IncomingState {
    pub(super) side: Side,
    pub(super) root: usize,
    pub(super) seed: usize,
    pub(super) entity: usize,
    pub(super) distance: usize,
    pub(super) witness: Vec<usize>,
    pub(super) boundary: bool,
    pub(super) unexamined: usize,
}

#[derive(Default)]
pub(super) struct WalkResult {
    pub(super) originals: Vec<OriginalSeed>,
    pub(super) expanded: BTreeSet<(Side, usize)>,
    pub(super) hierarchy: Vec<HierarchyState>,
    pub(super) hierarchy_proofs: Vec<ProofRef>,
    pub(super) members: Vec<MemberReason>,
    pub(super) walk_seeds: Vec<WalkSeed>,
    pub(super) incoming: Vec<IncomingState>,
    pub(super) incoming_proofs: Vec<ProofRef>,
}

fn hierarchy_adjacency<'a>(
    index: &'a Index<'_>,
    direction: Direction,
    entity: usize,
) -> &'a [usize] {
    match direction {
        Direction::Ancestor => index.ancestors.get(&entity),
        Direction::Descendant => index.descendants.get(&entity),
    }
    .map_or(&[], Vec::as_slice)
}

fn incoming_adjacency<'a>(index: &'a Index<'_>, entity: usize) -> &'a [usize] {
    index.incoming.get(&entity).map_or(&[], Vec::as_slice)
}

/// Compare the complete proposed path without allocating a competing path.
fn candidate_precedes(
    index: &Index<'_>,
    prefix: &[usize],
    observation: usize,
    previous: &[usize],
    budget: &Budget<'_>,
) -> Result<bool> {
    for (left, right) in prefix
        .iter()
        .chain(std::iter::once(&observation))
        .zip(previous)
    {
        budget.check()?;
        let left = &index.graph.observations[*left].id;
        let right = &index.graph.observations[*right].id;
        let comparison = left.as_bytes().cmp(right.as_bytes());
        #[cfg(test)]
        super::observe_work(super::WorkPhase::WitnessComparison);
        match comparison {
            Ordering::Less => return Ok(true),
            Ordering::Greater => return Ok(false),
            Ordering::Equal => {}
        }
    }
    Ok(false)
}

fn extended_witness(prefix: &[usize], observation: usize) -> Vec<usize> {
    let mut witness = Vec::with_capacity(prefix.len() + 1);
    witness.extend_from_slice(prefix);
    witness.push(observation);
    witness
}

/// Candidate credit is reserved on its first discovery, before path allocation.
/// Replacement paths have exactly the same length and reuse that reservation.
fn consider_candidate(
    index: &Index<'_>,
    candidates: &mut BTreeMap<usize, Vec<usize>>,
    entity: usize,
    prefix: &[usize],
    observation: usize,
    hierarchy: bool,
    budget: &mut Budget<'_>,
) -> Result<()> {
    if let Some(previous) = candidates.get_mut(&entity) {
        if candidate_precedes(index, prefix, observation, previous, budget)? {
            *previous = extended_witness(prefix, observation);
        }
    } else {
        budget.charge_state(hierarchy)?;
        let rows = prefix.len().checked_add(2).ok_or_else(super::capacity)?;
        budget.charge_rows(rows)?;
        candidates.insert(entity, extended_witness(prefix, observation));
    }
    Ok(())
}

/// New proof claims validate both endpoints and every captured evidence ref.
/// Storage deduplication and repeated source hashes do not erase these checks.
fn validate_proof(
    index: &Index<'_>,
    observation: usize,
    sources: &mut SourceCache<'_>,
    budget: &mut Budget<'_>,
) -> Result<(usize, usize)> {
    let observation = index.graph.observations.get(observation).context(INVALID)?;
    let source = *index
        .entities
        .get(observation.source.as_str())
        .context(INVALID)?;
    let target = *index
        .entities
        .get(observation.target.as_deref().context(INVALID)?)
        .context(INVALID)?;
    sources.validate_entity(index, source, budget)?;
    sources.validate_entity(index, target, budget)?;
    for id in &observation.evidence_ids {
        budget.check()?;
        let evidence = *index.evidence.get(id.as_str()).context(INVALID)?;
        sources.validate_evidence(index, evidence, budget)?;
    }
    Ok((source, target))
}

fn walk_hierarchy(
    index: &Index<'_>,
    root: usize,
    owner: usize,
    direction: Direction,
    sources: &mut SourceCache<'_>,
    budget: &mut Budget<'_>,
    result: &mut WalkResult,
) -> Result<()> {
    let depth = budget.options.max_hierarchy_depth;
    if depth == 0 {
        return Ok(());
    }
    budget.charge_state(true)?;
    budget.charge_rows(1)?;
    let initial = result.hierarchy.len();
    result.hierarchy.push(HierarchyState {
        side: index.side,
        root,
        direction,
        entity: owner,
        distance: 0,
        witness: Vec::new(),
        boundary: false,
        unexamined: 0,
    });
    let mut visited = BTreeSet::from([owner]);
    let mut frontier = vec![initial];
    while !frontier.is_empty() {
        budget.check()?;
        let mut candidates = BTreeMap::new();
        for state_index in frontier {
            budget.check()?;
            let state = &result.hierarchy[state_index];
            if state.boundary {
                continue;
            }
            for &observation in hierarchy_adjacency(index, direction, state.entity) {
                budget.check()?;
                budget.charge_adjacency(true)?;
                budget.charge_rows(1)?;
                let (source, target) = validate_proof(index, observation, sources, budget)?;
                result.hierarchy_proofs.push(ProofRef {
                    state: state_index,
                    observation,
                });
                #[cfg(test)]
                super::observe_work(super::WorkPhase::HierarchyAdjacency);
                let next = match direction {
                    Direction::Ancestor => target,
                    Direction::Descendant => source,
                };
                if !visited.contains(&next) {
                    consider_candidate(
                        index,
                        &mut candidates,
                        next,
                        &state.witness,
                        observation,
                        true,
                        budget,
                    )?;
                }
            }
        }
        // All parents at this distance have competed before a state is settled.
        frontier = Vec::new();
        for (entity, witness) in candidates {
            budget.check()?;
            let distance = witness.len();
            let boundary = distance == depth;
            let unexamined = if boundary {
                hierarchy_adjacency(index, direction, entity).len()
            } else {
                0
            };
            visited.insert(entity);
            let state = result.hierarchy.len();
            result.hierarchy.push(HierarchyState {
                side: index.side,
                root,
                direction,
                entity,
                distance,
                witness,
                boundary,
                unexamined,
            });
            frontier.push(state);
        }
    }
    Ok(())
}

fn walk_incoming(
    index: &Index<'_>,
    root: usize,
    seed: usize,
    sources: &mut SourceCache<'_>,
    budget: &mut Budget<'_>,
    result: &mut WalkResult,
) -> Result<()> {
    let depth = budget.options.max_incoming_depth;
    budget.charge_state(false)?;
    budget.charge_rows(1)?;
    let initial = result.incoming.len();
    result.incoming.push(IncomingState {
        side: index.side,
        root,
        seed,
        entity: seed,
        distance: 0,
        witness: Vec::new(),
        boundary: depth == 0,
        unexamined: if depth == 0 {
            incoming_adjacency(index, seed).len()
        } else {
            0
        },
    });
    let mut visited = BTreeSet::from([seed]);
    let mut frontier = vec![initial];
    while !frontier.is_empty() {
        budget.check()?;
        let mut candidates = BTreeMap::new();
        for state_index in frontier {
            budget.check()?;
            let state = &result.incoming[state_index];
            if state.boundary {
                continue;
            }
            for &observation in incoming_adjacency(index, state.entity) {
                budget.check()?;
                budget.charge_adjacency(false)?;
                budget.charge_rows(1)?;
                let (source, _) = validate_proof(index, observation, sources, budget)?;
                result.incoming_proofs.push(ProofRef {
                    state: state_index,
                    observation,
                });
                #[cfg(test)]
                super::observe_work(super::WorkPhase::IncomingAdjacency);
                if !visited.contains(&source) {
                    consider_candidate(
                        index,
                        &mut candidates,
                        source,
                        &state.witness,
                        observation,
                        false,
                        budget,
                    )?;
                }
            }
        }
        frontier = Vec::new();
        for (entity, witness) in candidates {
            budget.check()?;
            let distance = witness.len();
            let boundary = distance == depth;
            let unexamined = if boundary {
                incoming_adjacency(index, entity).len()
            } else {
                0
            };
            visited.insert(entity);
            let state = result.incoming.len();
            result.incoming.push(IncomingState {
                side: index.side,
                root,
                seed,
                entity,
                distance,
                witness,
                boundary,
                unexamined,
            });
            frontier.push(state);
        }
    }
    Ok(())
}

pub(super) fn derive(
    access: &dyn SourceAccess,
    indexes: &[Index<'_>; 2],
    sources: &mut SourceCache<'_>,
    budget: &mut Budget<'_>,
) -> Result<WalkResult> {
    let mut result = WalkResult::default();
    let mut originals = BTreeMap::<(Side, usize), usize>::new();
    for (row_index, row) in access.changed_symbols().iter().enumerate() {
        budget.check()?;
        budget.charge_original(1)?;
        let side = match row["side"].as_str() {
            Some("base") => Side::Base,
            Some("head") => Side::Head,
            _ => bail!(INVALID),
        };
        let index = &indexes[side.index()];
        let entity = *index
            .entities
            .get(row["id"].as_str().context(INVALID)?)
            .context(INVALID)?;
        if let Some(&original) = originals.get(&(side, entity)) {
            budget.charge_rows(1)?;
            result.originals[original].changed_rows.push(row_index);
        } else {
            budget.charge_rows(2)?;
            let eligibility = sources.eligibility(index, entity, budget)?;
            let original = result.originals.len();
            result.originals.push(OriginalSeed {
                side,
                entity,
                changed_rows: vec![row_index],
                eligibility,
            });
            originals.insert((side, entity), original);
        }
    }

    // Fix the complete original set before any related member is considered.
    // A member that is another original root still receives its reason/walk.
    let mut walk_seeds = BTreeMap::<(Side, usize, usize), usize>::new();
    for original in &result.originals {
        budget.check()?;
        budget.charge_rows(1)?;
        let seed = result.walk_seeds.len();
        result.walk_seeds.push(WalkSeed {
            side: original.side,
            root: original.entity,
            entity: original.entity,
            original: true,
            reasons: Vec::new(),
        });
        walk_seeds.insert((original.side, original.entity, original.entity), seed);
    }

    for original_index in 0..result.originals.len() {
        budget.check()?;
        let original = &result.originals[original_index];
        let (side, root) = (original.side, original.entity);
        let Eligibility::Expandable { owner } = original.eligibility else {
            continue;
        };
        let index = &indexes[side.index()];
        for direction in [Direction::Ancestor, Direction::Descendant] {
            walk_hierarchy(index, root, owner, direction, sources, budget, &mut result)?;
        }
    }

    for state_index in 0..result.hierarchy.len() {
        budget.check()?;
        let state = &result.hierarchy[state_index];
        let index = &indexes[state.side.index()];
        let owner = match result.originals
            [*originals.get(&(state.side, state.root)).context(INVALID)?]
        .eligibility
        {
            Eligibility::Expandable { owner } => owner,
            Eligibility::Unsupported(_) => bail!(INVALID),
        };
        if state.distance == 0 || state.entity == owner {
            continue;
        }
        let member_indices = index
            .members
            .get(index.graph.entities[state.entity].key.as_str())
            .map_or(&[][..], Vec::as_slice);
        for &member in member_indices {
            budget.check()?;
            if index.graph.entities[member].name != index.graph.entities[state.root].name {
                continue;
            }
            let eligibility = sources.eligibility(index, member, budget)?;
            if let Eligibility::Expandable {
                owner: member_owner,
            } = eligibility
            {
                if member_owner != state.entity {
                    bail!(INVALID);
                }
            }
            budget.charge_rows(1)?;
            let reason = result.members.len();
            result.members.push(MemberReason {
                side: state.side,
                root: state.root,
                direction: state.direction,
                type_entity: state.entity,
                member,
                state: state_index,
                eligibility,
            });
            if !matches!(eligibility, Eligibility::Expandable { .. }) {
                continue;
            }
            let pair = (state.side, member);
            if !originals.contains_key(&pair) && !result.expanded.contains(&pair) {
                budget.charge_expanded()?;
                budget.charge_rows(1)?;
                result.expanded.insert(pair);
            }
            let key = (state.side, state.root, member);
            if let Some(&seed) = walk_seeds.get(&key) {
                // A root's own owner is never expanded, so this is a hierarchy seed.
                if result.walk_seeds[seed].original {
                    bail!(INVALID);
                }
                budget.charge_rows(1)?;
                result.walk_seeds[seed].reasons.push(reason);
            } else {
                budget.charge_rows(2)?;
                let seed = result.walk_seeds.len();
                result.walk_seeds.push(WalkSeed {
                    side: state.side,
                    root: state.root,
                    entity: member,
                    original: false,
                    reasons: vec![reason],
                });
                walk_seeds.insert(key, seed);
            }
        }
    }

    for seed_index in 0..result.walk_seeds.len() {
        budget.check()?;
        let seed = &result.walk_seeds[seed_index];
        let (side, root, entity) = (seed.side, seed.root, seed.entity);
        walk_incoming(
            &indexes[side.index()],
            root,
            entity,
            sources,
            budget,
            &mut result,
        )?;
    }
    Ok(result)
}
