use super::{Budget, Side, SourceAccess, capacity};
use crate::model::{Graph, Snapshot, SourceFile, valid_id};
use anyhow::{Context, Result, ensure};
use std::collections::BTreeMap;

pub(super) struct Index<'a> {
    pub(super) side: Side,
    pub(super) snapshot: &'a Snapshot,
    pub(super) graph: &'a Graph,
    pub(super) entities: BTreeMap<&'a str, usize>,
    pub(super) keys: BTreeMap<&'a str, usize>,
    pub(super) observations: BTreeMap<&'a str, usize>,
    pub(super) evidence: BTreeMap<&'a str, usize>,
    pub(super) ancestors: BTreeMap<usize, Vec<usize>>,
    pub(super) descendants: BTreeMap<usize, Vec<usize>>,
    pub(super) incoming: BTreeMap<usize, Vec<usize>>,
    pub(super) members: BTreeMap<&'a str, Vec<usize>>,
    pub(super) hierarchy_unresolved: usize,
    pub(super) hierarchy_unsupported: usize,
    pub(super) incoming_unresolved: usize,
    pub(super) files: BTreeMap<&'a str, &'a SourceFile>,
}

fn is_type(graph: &Graph, entity: usize) -> bool {
    let entity = &graph.entities[entity];
    entity.language == "typescript" && matches!(entity.kind.as_str(), "class" | "interface")
}

fn sort_observations(
    adjacency: &mut BTreeMap<usize, Vec<usize>>,
    graph: &Graph,
    budget: &Budget<'_>,
) -> Result<()> {
    for entries in adjacency.values_mut() {
        budget.check()?;
        let mut refusal = None;
        entries.sort_unstable_by(|left, right| {
            if refusal.is_none() {
                refusal = budget.check().err();
            }
            graph.observations[*left]
                .id
                .cmp(&graph.observations[*right].id)
        });
        if let Some(error) = refusal {
            return Err(error);
        }
        budget.check()?;
    }
    Ok(())
}

impl<'a> Index<'a> {
    pub(super) fn build(
        side: Side,
        access: &'a dyn SourceAccess,
        budget: &mut Budget<'_>,
    ) -> Result<Self> {
        budget.check()?;
        let snapshot = access.snapshot(side);
        let graph = access.graph(side);
        ensure!(
            valid_id(&snapshot.id) && graph.snapshot_id == snapshot.id,
            "Invalid affected graph snapshot"
        );
        if graph.entities.len() > budget.options.max_graph_entities
            || graph.observations.len() > budget.options.max_graph_observations
            || graph.evidence.len() > budget.options.max_graph_evidence
        {
            return Err(capacity());
        }
        let mut result = Self {
            side,
            snapshot,
            graph,
            entities: BTreeMap::new(),
            keys: BTreeMap::new(),
            observations: BTreeMap::new(),
            evidence: BTreeMap::new(),
            ancestors: BTreeMap::new(),
            descendants: BTreeMap::new(),
            incoming: BTreeMap::new(),
            members: BTreeMap::new(),
            hierarchy_unresolved: 0,
            hierarchy_unsupported: 0,
            incoming_unresolved: 0,
            files: BTreeMap::new(),
        };
        for file in &snapshot.files {
            budget.check()?;
            ensure!(
                valid_id(&file.hash)
                    && crate::paths::normalized(&file.path)? == file.path
                    && result.files.insert(file.path.as_str(), file).is_none(),
                "Invalid affected snapshot file binding"
            );
        }
        for (position, entity) in graph.entities.iter().enumerate() {
            budget.check()?;
            let file = result
                .files
                .get(entity.path.as_str())
                .context("Affected entity outside snapshot")?;
            ensure!(
                valid_id(&entity.id)
                    && entity.snapshot_id == snapshot.id
                    && entity.start_byte <= entity.end_byte
                    && entity.end_byte <= file.size
                    && entity.start_line > 0
                    && entity.end_line > 0
                    && result
                        .entities
                        .insert(entity.id.as_str(), position)
                        .is_none()
                    && result.keys.insert(entity.key.as_str(), position).is_none(),
                "Invalid or duplicate affected entity"
            );
            if entity.kind == "method"
                && let Some(parent) = entity.parent_key.as_deref()
            {
                result.members.entry(parent).or_default().push(position);
            }
        }
        for (position, evidence) in graph.evidence.iter().enumerate() {
            budget.check()?;
            let file = result
                .files
                .get(evidence.path.as_str())
                .context("Affected evidence outside snapshot")?;
            ensure!(
                valid_id(&evidence.id)
                    && evidence.snapshot_id == snapshot.id
                    && evidence.content_hash == file.hash
                    && evidence.anchor == "exact"
                    && evidence.start_byte <= evidence.end_byte
                    && evidence.end_byte <= file.size
                    && evidence.start_line > 0
                    && evidence.end_line > 0
                    && result
                        .evidence
                        .insert(evidence.id.as_str(), position)
                        .is_none(),
                "Invalid or duplicate affected evidence"
            );
        }
        // One charged pass covers every captured observation, including facts
        // unrelated to the new walks and null-target coverage entries.
        for (position, observation) in graph.observations.iter().enumerate() {
            budget.charge_index_scan()?;
            ensure!(
                valid_id(&observation.id)
                    && observation.snapshot_id == snapshot.id
                    && result
                        .observations
                        .insert(observation.id.as_str(), position)
                        .is_none(),
                "Invalid or duplicate affected observation"
            );
            let source = *result
                .entities
                .get(observation.source.as_str())
                .context("Affected observation source missing")?;
            let target = observation
                .target
                .as_deref()
                .map(|id| {
                    result
                        .entities
                        .get(id)
                        .copied()
                        .context("Affected observation target missing")
                })
                .transpose()?;
            ensure!(
                !observation.evidence_ids.is_empty(),
                "Affected observation evidence missing"
            );
            for evidence in &observation.evidence_ids {
                budget.check()?;
                ensure!(
                    result.evidence.contains_key(evidence.as_str()),
                    "Affected observation evidence missing"
                );
            }
            match observation.relation.as_str() {
                "extends" | "implements" | "inherits" => match target {
                    None => result.hierarchy_unresolved += 1,
                    Some(target) if is_type(graph, source) && is_type(graph, target) => {
                        result.ancestors.entry(source).or_default().push(position);
                        result.descendants.entry(target).or_default().push(position);
                    }
                    Some(_) => result.hierarchy_unsupported += 1,
                },
                "calls" | "references" => match target {
                    None => result.incoming_unresolved += 1,
                    Some(target) => result.incoming.entry(target).or_default().push(position),
                },
                _ => {}
            }
            #[cfg(test)]
            super::observe_work(super::WorkPhase::IndexScan);
        }
        sort_observations(&mut result.ancestors, graph, budget)?;
        sort_observations(&mut result.descendants, graph, budget)?;
        sort_observations(&mut result.incoming, graph, budget)?;
        for members in result.members.values_mut() {
            budget.check()?;
            members.sort_unstable_by(|left, right| {
                graph.entities[*left].id.cmp(&graph.entities[*right].id)
            });
            budget.check()?;
        }
        budget.check()?;
        Ok(result)
    }
}
