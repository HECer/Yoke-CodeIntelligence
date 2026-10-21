//! Immutable, source-bound derived graph reports. No source execution or model calls.
use crate::{
    capture::Deadline,
    model::{Entity, Graph, Observation, Snapshot, hash, identity, valid_id},
    paths::{is_link, utf8_range},
    store::Store,
};
use anyhow::{Result, ensure};
use rusqlite::OptionalExtension;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::Read,
};
#[path = "graph_analysis_policy.rs"]
mod policy;
pub use policy::{Category as AnalysisCategory, SourceCategoryPolicy};
#[path = "graph_community_policy.rs"]
mod community;
pub use community::CommunityPolicy;

pub const REPORT_KIND: &str = "native_graph_report_v1";
pub const VIEW_REPORT_KIND: &str = "native_graph_view_report_v1";
const VIEW_REPORT_PRODUCER: &str = "yoke-graph-view-report-1";
const CATEGORY_PRODUCER: &str = "yoke-source-category-report-1";
const CATEGORY_VIEW_PRODUCER: &str = "yoke-source-category-view-report-1";
const COMMUNITY_PRODUCER: &str = "yoke-community-policy-report-1";
const COMMUNITY_VIEW_PRODUCER: &str = "yoke-community-policy-view-report-1";

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AnalysisOptions {
    #[serde(default = "default_limit")]
    pub hub_limit: usize,
    #[serde(default = "default_limit")]
    pub surprise_limit: usize,
    #[serde(default = "default_limit")]
    pub question_limit: usize,
    #[serde(default = "default_resolution")]
    pub resolution: f64,
    #[serde(default)]
    pub exclude_hubs_percentile: Option<f64>,
    #[serde(default = "default_uncertainty")]
    pub uncertain_weight: f64,
    #[serde(default = "default_iterations")]
    pub max_iterations: usize,
    #[serde(default = "default_seed")]
    pub seed: u64,
    #[serde(default)]
    pub max_community_size: usize,
    #[serde(default)]
    pub previous_snapshot_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_category_policy: Option<SourceCategoryPolicy>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub community_policy: Option<CommunityPolicy>,
}
fn default_limit() -> usize {
    10
}
fn default_resolution() -> f64 {
    1.0
}
fn default_uncertainty() -> f64 {
    0.5
}
fn default_iterations() -> usize {
    10
}
fn default_seed() -> u64 {
    42
}
impl Default for AnalysisOptions {
    fn default() -> Self {
        Self {
            hub_limit: 10,
            surprise_limit: 10,
            question_limit: 10,
            resolution: 1.0,
            exclude_hubs_percentile: None,
            uncertain_weight: 0.5,
            max_iterations: 10,
            seed: 42,
            max_community_size: 0,
            previous_snapshot_id: None,
            source_category_policy: None,
            community_policy: None,
        }
    }
}

const PRODUCER: &str = "yoke-graph-report-v1";
const MAX_BYTES: usize = 64_000_000;
const MAX_NODES: usize = 5000;
const MAX_EDGES: usize = 50000;
const LIMITATIONS: &str = "Partial structural graph: not runtime or compiler proof. Rankings and suggested questions are heuristics, not probabilities or independently confirmed facts. Source snapshots and parallel directed observations remain unchanged.";

pub(crate) fn validate_options(options: &AnalysisOptions) -> Result<()> {
    if let Some(policy) = &options.source_category_policy {
        policy.validate()?;
    }
    ensure!(
        (1..=100).contains(&options.hub_limit)
            && (1..=100).contains(&options.surprise_limit)
            && (1..=100).contains(&options.question_limit),
        "Invalid report result limits"
    );
    ensure!(
        options.resolution.is_finite() && (0.0..=100.0).contains(&options.resolution),
        "Invalid Leiden resolution"
    );
    ensure!(
        options.uncertain_weight.is_finite() && (0.0..=1.0).contains(&options.uncertain_weight),
        "Invalid uncertain-edge weight"
    );
    ensure!(
        options
            .exclude_hubs_percentile
            .is_none_or(|p| p.is_finite() && (0.0..=100.0).contains(&p)),
        "Invalid hub exclusion percentile"
    );
    ensure!(
        (1..=50).contains(&options.max_iterations) && options.max_community_size <= MAX_NODES,
        "Invalid Leiden bounds"
    );
    ensure!(
        options
            .previous_snapshot_id
            .as_ref()
            .is_none_or(|id| valid_id(id)),
        "Invalid previous snapshot ID"
    );
    Ok(())
}
pub(crate) fn sources(
    store: &Store,
    snapshot: &Snapshot,
    graph: &Graph,
    deadline: &Deadline,
) -> Result<BTreeMap<String, String>> {
    deadline.check()?;
    store.set_deadline(deadline)?;
    ensure!(
        snapshot.workspace_id == store.workspace_id
            && graph.snapshot_id == snapshot.id
            && valid_id(&snapshot.id),
        "Invalid graph workspace or snapshot identity"
    );
    ensure!(
        graph.entities.len() <= MAX_NODES
            && graph.observations.len() <= MAX_EDGES
            && graph.evidence.len() <= 100000
            && snapshot.files.len() <= 20000,
        "Derived graph capacity exceeded"
    );
    ensure!(
        identity(&store.snapshot(&snapshot.id)?) == identity(snapshot),
        "Snapshot differs from immutable store"
    );
    ensure!(
        identity(&store.graph(&snapshot.id)?) == identity(graph),
        "Graph differs from immutable store"
    );
    let mut result = BTreeMap::new();
    let mut total = 0usize;
    let source_scope = crate::capture::source_scope_policy(store, snapshot)?;
    for source in &snapshot.files {
        deadline.check()?;
        source_scope.path_policy(&source.path)?;
        ensure!(
            valid_id(&source.hash) && source.size <= 4_000_000,
            "Invalid source binding"
        );
        total = total
            .checked_add(source.size)
            .ok_or_else(|| anyhow::anyhow!("Source capacity overflow"))?;
        ensure!(total <= MAX_BYTES, "Source validation capacity exceeded");
        let path = store.dir.join("blobs").join(&source.hash);
        let meta = fs::symlink_metadata(&path)?;
        ensure!(
            meta.is_file() && !is_link(&meta) && meta.len() == source.size as u64,
            "Source blob type or size mismatch"
        );
        let mut input = fs::File::open(path)?;
        let opened = input.metadata()?;
        ensure!(
            opened.is_file() && opened.len() == meta.len(),
            "Source changed before read"
        );
        let mut raw = Vec::with_capacity(source.size);
        let mut digest = Sha256::new();
        let mut chunk = [0u8; 65536];
        loop {
            deadline.check()?;
            let read = input.read(&mut chunk)?;
            if read == 0 {
                break;
            }
            ensure!(
                raw.len() + read <= source.size,
                "Source grew during validation"
            );
            digest.update(&chunk[..read]);
            raw.extend_from_slice(&chunk[..read]);
        }
        deadline.check()?;
        ensure!(
            raw.len() == source.size
                && hex::encode(digest.finalize()) == source.hash
                && input.metadata()?.len() == opened.len(),
            "Source hash or size changed"
        );
        ensure!(
            result
                .insert(source.path.clone(), String::from_utf8(raw)?)
                .is_none(),
            "Duplicate source path"
        );
    }
    let mut endpoints = BTreeMap::<String, BTreeSet<usize>>::new();
    for entity in &graph.entities {
        endpoints.entry(entity.path.clone()).or_default().extend([
            entity.start_byte,
            entity.end_byte,
            entity.end_byte.saturating_sub(1),
        ]);
    }
    for evidence in &graph.evidence {
        endpoints.entry(evidence.path.clone()).or_default().extend([
            evidence.start_byte,
            evidence.end_byte,
            evidence.end_byte.saturating_sub(1),
        ]);
    }
    let mut exact_lines = BTreeMap::<String, BTreeMap<usize, usize>>::new();
    for (path, points) in endpoints {
        let text = result
            .get(&path)
            .ok_or_else(|| anyhow::anyhow!("Anchor source outside snapshot"))?;
        let mut offset = 0usize;
        let mut line = 1usize;
        let mut lines = BTreeMap::new();
        for point in points {
            deadline.check()?;
            ensure!(point <= text.len(), "Anchor byte offset outside source");
            for chunk in text.as_bytes()[offset..point].chunks(65536) {
                deadline.check()?;
                line += chunk.iter().filter(|b| **b == b'\n').count();
            }
            lines.insert(point, line);
            offset = point;
        }
        exact_lines.insert(path, lines);
    }
    let files: BTreeMap<_, _> = snapshot
        .files
        .iter()
        .map(|f| (f.path.as_str(), f.hash.as_str()))
        .collect();
    let mut nodes = BTreeSet::new();
    let mut keys = BTreeSet::new();
    for entity in &graph.entities {
        deadline.check()?;
        ensure!(
            valid_id(&entity.id)
                && entity.snapshot_id == snapshot.id
                && nodes.insert(entity.id.as_str())
                && keys.insert(entity.key.as_str()),
            "Invalid or duplicate entity identity"
        );
        let text = result
            .get(&entity.path)
            .ok_or_else(|| anyhow::anyhow!("Entity source outside snapshot"))?;
        utf8_range(text, entity.start_byte, entity.end_byte)?;
        ensure!(
            entity.start_line == exact_lines[&entity.path][&entity.start_byte]
                && entity.end_line == exact_lines[&entity.path][&entity.end_byte.saturating_sub(1)],
            "Invalid entity line binding"
        );
    }
    let mut anchors = BTreeSet::new();
    for anchor in &graph.evidence {
        deadline.check()?;
        ensure!(
            valid_id(&anchor.id)
                && anchor.snapshot_id == snapshot.id
                && anchors.insert(anchor.id.as_str()),
            "Invalid or duplicate evidence identity"
        );
        ensure!(
            files.get(anchor.path.as_str()) == Some(&anchor.content_hash.as_str()),
            "Evidence source hash mismatch"
        );
        let text = result
            .get(&anchor.path)
            .ok_or_else(|| anyhow::anyhow!("Evidence source outside snapshot"))?;
        utf8_range(text, anchor.start_byte, anchor.end_byte)?;
        ensure!(
            anchor.anchor == "exact"
                && anchor.start_line == exact_lines[&anchor.path][&anchor.start_byte]
                && (anchor.end_line == exact_lines[&anchor.path][&anchor.end_byte]
                    || anchor.end_line
                        == exact_lines[&anchor.path][&anchor.end_byte.saturating_sub(1)]),
            "Unsupported evidence anchor"
        );
    }
    let mut observations = BTreeSet::new();
    for edge in &graph.observations {
        deadline.check()?;
        ensure!(
            valid_id(&edge.id)
                && edge.snapshot_id == snapshot.id
                && observations.insert(edge.id.as_str())
                && nodes.contains(edge.source.as_str())
                && edge
                    .target
                    .as_ref()
                    .is_none_or(|id| nodes.contains(id.as_str())),
            "Observation endpoint or identity mismatch"
        );
        ensure!(
            !edge.evidence_ids.is_empty()
                && edge
                    .evidence_ids
                    .iter()
                    .all(|id| anchors.contains(id.as_str())),
            "Observation evidence missing"
        );
    }
    deadline.check()?;
    Ok(result)
}
pub(crate) fn validate_scope(
    store: &Store,
    snapshot: &Snapshot,
    graph: &Graph,
    deadline: &Deadline,
) -> Result<()> {
    sources(store, snapshot, graph, deadline).map(|_| ())
}
fn anchor(snapshot: &Snapshot, entity: &Entity) -> Value {
    let file = snapshot.files.iter().find(|f| f.path == entity.path);
    json!({"path":entity.path,"contentHash":file.map(|f|f.hash.as_str()),"startByte":entity.start_byte,"endByte":entity.end_byte,"startLine":entity.start_line,"endLine":entity.end_line,"entityId":entity.id,"anchor":"exact"})
}
fn semantic_key(entity: &Entity, keys: &BTreeMap<&str, &Entity>) -> Result<String> {
    let mut context = Vec::new();
    let mut parent = entity.parent_key.as_deref();
    let mut visited = BTreeSet::new();
    while let Some(key) = parent {
        ensure!(
            visited.len() < 128 && visited.insert(key),
            "Entity parent cycle or depth exceeded"
        );
        let e = keys
            .get(key)
            .ok_or_else(|| anyhow::anyhow!("Entity parent missing"))?;
        context.push((e.kind.clone(), e.qualified_name.clone()));
        parent = e.parent_key.as_deref();
    }
    context.reverse();
    Ok(serde_json::to_string(&(
        &entity.path,
        &entity.kind,
        &entity.qualified_name,
        context,
    ))?)
}
struct Derived {
    communities: Vec<Value>,
    membership: BTreeMap<String, String>,
    degree: BTreeMap<String, usize>,
    excluded: BTreeSet<String>,
    quality: Option<f64>,
    community_details: Option<Value>,
}
fn edge_weight(edge: &Observation, options: &AnalysisOptions) -> f64 {
    if let Some(policy) = &options.source_category_policy {
        policy.weight(edge, options.uncertain_weight)
    } else if edge.resolution == "syntactic" {
        1.0
    } else {
        options.uncertain_weight
    }
}
fn derive(graph: &Graph, options: &AnalysisOptions, deadline: &Deadline) -> Result<Derived> {
    let keys: BTreeMap<_, _> = graph.entities.iter().map(|e| (e.key.as_str(), e)).collect();
    let mut stable_by_id = BTreeMap::new();
    let mut stable_counts = BTreeMap::<String, usize>::new();
    for entity in &graph.entities {
        deadline.check()?;
        let stable = semantic_key(entity, &keys)?;
        *stable_counts.entry(stable.clone()).or_default() += 1;
        stable_by_id.insert(entity.id.as_str(), stable);
    }
    let mut nodes: Vec<_> = graph.entities.iter().collect();
    nodes.sort_by(|a, b| {
        (&a.path, &a.qualified_name, &a.kind, a.start_byte).cmp(&(
            &b.path,
            &b.qualified_name,
            &b.kind,
            b.start_byte,
        ))
    });
    let index: BTreeMap<_, _> = nodes
        .iter()
        .enumerate()
        .map(|(i, e)| (e.id.as_str(), i))
        .collect();
    let mut degree: BTreeMap<_, usize> = nodes.iter().map(|e| (e.id.clone(), 0)).collect();
    let mut adjacency = vec![Vec::<(usize, f64)>::new(); nodes.len()];
    for edge in &graph.observations {
        deadline.check()?;
        if let Some(target) = &edge.target {
            let a = index[edge.source.as_str()];
            let b = index[target.as_str()];
            *degree.get_mut(&edge.source).unwrap() += 1;
            *degree.get_mut(target).unwrap() += 1;
            let weight = edge_weight(edge, options);
            if weight > 0.0 {
                adjacency[a].push((b, weight));
                if a != b {
                    adjacency[b].push((a, weight));
                }
            }
        }
    }
    let (groups, excluded, quality, community_details) = match options.community_policy {
        Some(CommunityPolicy::NeighborMajorityV1 {}) => {
            let ids: Vec<_> = nodes.iter().map(|node| node.id.as_str()).collect();
            let selected = community::partition(&ids, &adjacency, options, deadline)?;
            (
                selected.groups,
                selected.excluded,
                selected.quality,
                Some(selected.details),
            )
        }
        None => {
            let mut excluded = BTreeSet::new();
            if let Some(percentile) = options.exclude_hubs_percentile
                && !nodes.is_empty()
            {
                let mut counts: Vec<_> = degree.values().copied().collect();
                counts.sort();
                let position = if options.source_category_policy.is_some() {
                    ((counts.len() as f64 * percentile / 100.0).floor() as usize)
                        .saturating_sub(1)
                        .min(counts.len() - 1)
                } else {
                    ((counts.len() - 1) as f64 * percentile / 100.0).floor() as usize
                };
                let threshold = counts[position];
                excluded.extend(
                    degree
                        .iter()
                        .filter(|(_, n)| **n > threshold)
                        .map(|(id, _)| id.clone()),
                );
            }
            let active: Vec<_> = nodes
                .iter()
                .enumerate()
                .filter(|(_, n)| !excluded.contains(&n.id))
                .map(|(i, _)| i)
                .collect();
            let compact: BTreeMap<_, _> = active.iter().enumerate().map(|(a, b)| (*b, a)).collect();
            let mut builder = leiden_rs::GraphDataBuilder::new(active.len());
            for edge in &graph.observations {
                deadline.check()?;
                if let Some(target) = &edge.target {
                    let a = index[edge.source.as_str()];
                    let b = index[target.as_str()];
                    let weight = edge_weight(edge, options);
                    if weight > 0.0
                        && let (Some(a), Some(b)) = (compact.get(&a), compact.get(&b))
                    {
                        builder.add_edge(*a, *b, weight)?;
                    }
                }
            }
            deadline.check()?;
            let output = leiden_rs::Leiden::new(leiden_rs::LeidenConfig {
                resolution: options.resolution,
                max_iterations: options.max_iterations,
                seed: Some(options.seed),
                max_comm_size: options.max_community_size,
                ..Default::default()
            })
            .run(&builder.build()?)?;
            deadline.check()?;
            let mut groups: BTreeMap<usize, Vec<usize>> = output
                .partition
                .communities()
                .into_iter()
                .map(|(cid, members)| (cid, members.into_iter().map(|i| active[i]).collect()))
                .collect();
            let mut assigned: BTreeMap<usize, usize> = groups
                .iter()
                .flat_map(|(cid, members)| members.iter().map(move |node| (*node, *cid)))
                .collect();
            let mut next = groups.keys().next_back().map_or(0, |c| c + 1);
            for (i, node) in nodes.iter().enumerate() {
                deadline.check()?;
                if !excluded.contains(&node.id) {
                    continue;
                }
                let mut votes = BTreeMap::<usize, f64>::new();
                for (neighbor, weight) in &adjacency[i] {
                    if let Some(cid) = assigned.get(neighbor) {
                        *votes.entry(*cid).or_default() += weight;
                    }
                }
                let cid = votes
                    .into_iter()
                    .max_by(|(a, wa), (b, wb)| wa.total_cmp(wb).then_with(|| b.cmp(a)))
                    .map(|(cid, _)| cid)
                    .unwrap_or_else(|| {
                        let c = next;
                        next += 1;
                        c
                    });
                groups.entry(cid).or_default().push(i);
                assigned.insert(i, cid);
            }
            (
                groups,
                excluded,
                output.quality.is_finite().then_some(output.quality),
                None,
            )
        }
    };
    let mut communities = Vec::new();
    let mut membership = BTreeMap::new();
    for members in groups.values() {
        deadline.check()?;
        let mut stable_keys = Vec::new();
        let mut ids = Vec::new();
        let mut order = members.clone();
        order.sort_by(|a, b| {
            degree[&nodes[*b].id]
                .cmp(&degree[&nodes[*a].id])
                .then(nodes[*a].qualified_name.cmp(&nodes[*b].qualified_name))
                .then(nodes[*a].path.cmp(&nodes[*b].path))
        });
        for member in members {
            stable_keys.push(stable_by_id[nodes[*member].id.as_str()].clone());
            ids.push(nodes[*member].id.clone());
        }
        stable_keys.sort();
        ids.sort();
        let ambiguous_members = stable_keys.iter().any(|key| stable_counts[key] > 1);
        let cid = if ambiguous_members {
            identity(&("ambiguous_members", &graph.snapshot_id, &ids, &stable_keys))
        } else {
            identity(&stable_keys)
        };
        for id in &ids {
            membership.insert(id.clone(), cid.clone());
        }
        let mut labels: BTreeSet<String> = BTreeSet::new();
        let mut chosen = Vec::new();
        for i in &order {
            if nodes[*i].kind != "file" && labels.insert(nodes[*i].name.clone()) {
                chosen.push(nodes[*i].name.clone());
                if chosen.len() == 3 {
                    break;
                }
            }
        }
        if chosen.is_empty() {
            chosen.push(nodes[order[0]].name.clone());
        }
        let mut pairs = BTreeSet::new();
        let set: BTreeSet<_> = members.iter().copied().collect();
        for a in members {
            for (b, _) in &adjacency[*a] {
                if a != b && set.contains(b) {
                    pairs.insert(((*a).min(*b), (*a).max(*b)));
                }
            }
        }
        let cohesion = if members.len() < 2 {
            if options.community_policy.is_some() {
                1.0
            } else {
                0.0
            }
        } else {
            2.0 * pairs.len() as f64 / (members.len() * (members.len() - 1)) as f64
        };
        communities.push(json!({"id":cid,"members":ids,"stableMemberKeys":stable_keys,"identityMethod":if ambiguous_members{"ambiguous_members_snapshot_scoped"}else{"unique_structural_members"},"label":chosen.join(" · "),"labelProducer":"deterministic_source_hubs","cohesion":cohesion,"cohesionProjection":"unique_undirected_positive_weight_pairs","anchor":anchor_for_node(graph,nodes[order[0]])}));
    }
    communities.sort_by(|a, b| {
        b["members"]
            .as_array()
            .unwrap()
            .len()
            .cmp(&a["members"].as_array().unwrap().len())
            .then(a["id"].as_str().cmp(&b["id"].as_str()))
    });
    Ok(Derived {
        communities,
        membership,
        degree,
        excluded,
        quality,
        community_details,
    })
}
pub(crate) fn communities(graph: &Graph, deadline: &Deadline) -> Result<Vec<Value>> {
    Ok(derive(graph, &AnalysisOptions::default(), deadline)?.communities)
}
fn anchor_for_node(graph: &Graph, entity: &Entity) -> Value {
    json!({"entityId":entity.id,"path":entity.path,"startByte":entity.start_byte,"endByte":entity.end_byte,"startLine":entity.start_line,"endLine":entity.end_line,"snapshotId":graph.snapshot_id,"anchor":"exact"})
}
fn markdown_text(text: &str) -> String {
    text.chars()
        .flat_map(|c| {
            if matches!(
                c,
                '\\' | '`' | '*' | '_' | '[' | ']' | '(' | ')' | '#' | '!' | '|' | '~'
            ) {
                vec!['\\', c]
            } else if c == '<' {
                "&lt;".chars().collect()
            } else if c == '>' {
                "&gt;".chars().collect()
            } else if c == '&' {
                "&amp;".chars().collect()
            } else {
                vec![c]
            }
        })
        .collect()
}
pub(crate) fn markdown(report: &Value) -> String {
    let mut text = format!(
        "# Yoke graph report\n\n{}\n\nSnapshot `{}`\n\nGraph `{}`\n\n## Confidence and coverage audit\n\n{} observations; resolution categories are source/analysis labels, not probabilities.\n\n",
        markdown_text(LIMITATIONS),
        report["snapshotId"].as_str().unwrap_or(""),
        report["graphHash"].as_str().unwrap_or(""),
        report["audit"]["totalObservations"]
    );
    for section in ["godNodes", "communities", "surprises", "questions"] {
        text.push_str(&format!("## {}\n\n", section));
        if let Some(items) = report[section].as_array() {
            for item in items {
                let label = item["label"]
                    .as_str()
                    .or_else(|| item["question"].as_str())
                    .or_else(|| item["reason"].as_str())
                    .unwrap_or("Graph observation");
                text.push_str(&format!("- {}", markdown_text(label)));
                if let Some(path) = item["anchor"]["path"].as_str() {
                    text.push_str(&format!(
                        " · {}:{} · UTF-8 bytes {}–{}",
                        markdown_text(path),
                        item["anchor"]["startLine"],
                        item["anchor"]["startByte"],
                        item["anchor"]["endByte"]
                    ));
                }
                text.push('\n');
            }
        }
        text.push('\n');
    }
    if !report["diff"].is_null() {
        text.push_str("## Snapshot comparison\n\nMatched by source path, kind, qualified name and parent context. Duplicate semantic identities are reported as ambiguous. Location churn alone is not a source-body change.\n\n");
    }
    if let Some(view_id) = report["graphViewId"].as_str() {
        text.push_str(&format!("## Selected view provenance\n\nView `{view_id}`\n\nBase graph `{}`\n\nProjection `{}`\n\nOriginal binding precision follows; enclosing-file anchors are not exact symbol tokens. Provider attestations remain those of the selected overlays.\n\n", report["baseGraphHash"].as_str().unwrap(), report["projectionGraphHash"].as_str().unwrap()));
        for node in report["sourceBindings"].as_array().into_iter().flatten() {
            for binding in node["sourceBindings"].as_array().into_iter().flatten() {
                text.push_str(&format!(
                    "- `{}` · {} · UTF-8 bytes {}–{} · precision {}\n",
                    node["id"].as_str().unwrap(),
                    markdown_text(binding["path"].as_str().unwrap()),
                    binding["startByte"],
                    binding["endByte"],
                    markdown_text(binding["anchor"].as_str().unwrap())
                ));
            }
        }
    }
    text
}
pub fn analyze(
    store: &Store,
    snapshot: &Snapshot,
    graph: &Graph,
    options: &AnalysisOptions,
    deadline: &Deadline,
) -> Result<Value> {
    let mut report = reconstruct_report(store, snapshot, graph, options, deadline)?;
    let bytes = serde_json::to_vec(&report)?;
    ensure!(
        bytes.len() <= MAX_BYTES,
        "Report artifact capacity exceeded"
    );
    let markdown = markdown(&report);
    ensure!(
        markdown.len() <= MAX_BYTES,
        "Markdown report capacity exceeded"
    );
    store.set_deadline(deadline)?;
    let transaction =
        rusqlite::Transaction::new_unchecked(&store.db, rusqlite::TransactionBehavior::Immediate)?;
    deadline.check()?;
    let artifact = store.artifact(&bytes)?;
    let markdown_artifact = store.artifact(markdown.as_bytes())?;
    let record = json!({"snapshotId":snapshot.id,"workspaceId":snapshot.workspace_id,"graphHash":report["graphHash"],"previousSnapshotId":options.previous_snapshot_id,"artifactId":artifact,"markdownArtifactId":markdown_artifact,"options":options});
    store.db.execute(
        "INSERT INTO records(kind,id,value) VALUES(?1,?2,?3) ON CONFLICT(kind,id) DO NOTHING",
        rusqlite::params![REPORT_KIND, artifact, serde_json::to_string(&record)?],
    )?;
    ensure!(
        store.get_record::<Value>(REPORT_KIND, &artifact)? == record,
        "Stored report record differs"
    );
    deadline.check()?;
    transaction.commit()?;
    report["artifactId"] = json!(artifact);
    report["markdownArtifactId"] = json!(markdown_artifact);
    Ok(report)
}

/// Read-only canonical reconstruction for publication and dependency validation.
/// Producer v1 algorithms and serialized options remain immutable.
pub(crate) fn reconstruct_report(
    store: &Store,
    snapshot: &Snapshot,
    graph: &Graph,
    options: &AnalysisOptions,
    deadline: &Deadline,
) -> Result<Value> {
    validate_options(options)?;
    let _sources = sources(store, snapshot, graph, deadline)?;
    let previous = if let Some(id) = &options.previous_snapshot_id {
        let old = store.snapshot(id)?;
        let old_graph = store.graph(id)?;
        let diff = compare(store, &old, &old_graph, snapshot, graph, deadline)?;
        Some((old_graph, diff))
    } else {
        None
    };
    build_report(snapshot, graph, options, previous.as_ref(), deadline)
}

// Pure report construction: source-scope validation and publication belong to
// the canonical or checked-view entry point, never to the numerical projection.
fn build_report(
    snapshot: &Snapshot,
    graph: &Graph,
    options: &AnalysisOptions,
    previous: Option<&(Graph, Value)>,
    deadline: &Deadline,
) -> Result<Value> {
    let mut report = build_report_inner(snapshot, graph, options, previous, deadline)?;
    if let Some(policy) = &options.source_category_policy {
        report["producer"] = json!(CATEGORY_PRODUCER);
        report["analysisPolicy"] = json!({"kind":"native_source_categories_v1","configuration":policy,
            "classifications":"declared_projection_categories_not_source_or_execution_attestation",
            "originalResolutionPreserved":true,"baseWeights":"host_selected_relation_weights_default_one"});
        report["clustering"]["uncertainWeightAppliesTo"] = json!("policy_ambiguous_only");
        if options.community_policy.is_none() {
            report["clustering"]["hubPercentilePolicy"] =
                json!("floor_count_times_percentile_minus_one_clamped");
        }
        let mut unmapped = BTreeMap::<String, usize>::new();
        for edge in &graph.observations {
            deadline.check()?;
            if policy.category(edge) == AnalysisCategory::Unmapped {
                *unmapped.entry(edge.resolution.clone()).or_default() += 1;
            }
        }
        report["audit"]["unmappedPolicyResolutions"] = json!(unmapped);
    }
    report["producer"] = json!(canonical_producer(options));
    Ok(report)
}
fn canonical_producer(options: &AnalysisOptions) -> &'static str {
    match options.community_policy {
        Some(CommunityPolicy::NeighborMajorityV1 {}) => COMMUNITY_PRODUCER,
        None if options.source_category_policy.is_some() => CATEGORY_PRODUCER,
        None => PRODUCER,
    }
}
fn view_producer(options: &AnalysisOptions) -> &'static str {
    match options.community_policy {
        Some(CommunityPolicy::NeighborMajorityV1 {}) => COMMUNITY_VIEW_PRODUCER,
        None if options.source_category_policy.is_some() => CATEGORY_VIEW_PRODUCER,
        None => VIEW_REPORT_PRODUCER,
    }
}
fn build_report_inner(
    snapshot: &Snapshot,
    graph: &Graph,
    options: &AnalysisOptions,
    previous: Option<&(Graph, Value)>,
    deadline: &Deadline,
) -> Result<Value> {
    let mut derived = derive(graph, options, deadline)?;
    let nodes: BTreeMap<_, _> = graph.entities.iter().map(|e| (e.id.as_str(), e)).collect();
    let mut ranked: Vec<_> = graph
        .entities
        .iter()
        .filter(|e| {
            e.kind != "file"
                && !derived.excluded.contains(&e.id)
                && options
                    .source_category_policy
                    .as_ref()
                    .is_none_or(|p| p.eligible(e, derived.degree[&e.id]))
        })
        .collect();
    ranked.sort_by(|a, b| {
        derived.degree[&b.id]
            .cmp(&derived.degree[&a.id])
            .then(a.qualified_name.cmp(&b.qualified_name))
            .then(a.path.cmp(&b.path))
    });
    let gods:Vec<_>=ranked.iter().take(options.hub_limit).map(|e|json!({"entityId":e.id,"label":e.qualified_name,"degree":derived.degree[&e.id],"degreeDefinition":"resolved_parallel_in_plus_out_self_loop_twice","anchor":anchor(snapshot,e)})).collect();
    let mut by_resolution = BTreeMap::<String, usize>::new();
    let mut by_relation = BTreeMap::<String, usize>::new();
    let mut surprises = Vec::new();
    let mut unresolved = Vec::new();
    let mut inferred = Vec::new();
    let multi_source = graph
        .entities
        .iter()
        .filter(|e| {
            options
                .source_category_policy
                .as_ref()
                .is_none_or(|p| p.eligible(e, derived.degree[&e.id]))
        })
        .map(|e| e.path.as_str())
        .collect::<BTreeSet<_>>()
        .len()
        > 1;
    for edge in &graph.observations {
        deadline.check()?;
        *by_resolution.entry(edge.resolution.clone()).or_default() += 1;
        *by_relation.entry(edge.relation.clone()).or_default() += 1;
        let source = nodes[edge.source.as_str()];
        if let Some(target) = &edge.target {
            let target = nodes[target.as_str()];
            let separate = derived.membership[&source.id] != derived.membership[&target.id];
            let candidate = if let Some(policy) = &options.source_category_policy {
                !SourceCategoryPolicy::structural_relation(&edge.relation)
                    && policy.eligible(source, derived.degree[&source.id])
                    && policy.eligible(target, derived.degree[&target.id])
                    && if multi_source {
                        source.path != target.path
                    } else {
                        separate
                    }
            } else {
                edge.relation != "contains" && (source.path != target.path || separate)
            };
            if candidate {
                let reason = if source.path != target.path {
                    "cross_file_candidate"
                } else {
                    "cross_community_candidate"
                };
                let mut row = json!({"observationId":edge.id,"source":edge.source,"target":edge.target,"relation":edge.relation,"resolution":edge.resolution,"reason":reason,"rankingScore":if source.language!=target.language{3}else if edge.resolution!="syntactic"{2}else{1},"evidenceIds":edge.evidence_ids,"anchor":anchor(snapshot,source)});
                if let Some(policy) = &options.source_category_policy {
                    let (score, reasons) = policy.score(
                        edge,
                        source,
                        target,
                        separate,
                        derived.degree[&source.id],
                        derived.degree[&target.id],
                    );
                    row["rankingScore"] = json!(score);
                    row["rankingReasons"] = json!(reasons);
                    row["policyCategory"] = json!(policy.category(edge));
                }
                surprises.push(row);
            }
            let needs_confirmation = options.source_category_policy.as_ref().map_or(
                edge.resolution != "syntactic",
                |p| {
                    matches!(
                        p.category(edge),
                        AnalysisCategory::Inferred | AnalysisCategory::Ambiguous
                    )
                },
            );
            if needs_confirmation {
                let mut row = json!({"kind":"candidate","question":format!("What source or compiler evidence confirms {} → {} ({})?",source.qualified_name,target.qualified_name,edge.relation),"observationId":edge.id,"anchor":anchor(snapshot,source)});
                if let Some(policy) = &options.source_category_policy {
                    row["policyCategory"] = json!(policy.category(edge));
                    row["lowConfidence"] =
                        json!(policy.category(edge) == AnalysisCategory::Ambiguous);
                }
                inferred.push(row);
            }
        } else {
            unresolved.push(json!({"kind":"unresolved","question":format!("Where is {} resolved for {}?",edge.target_label,source.qualified_name),"observationId":edge.id,"anchor":anchor(snapshot,source)}));
        }
    }
    surprises.sort_by(|a, b| {
        b["rankingScore"]
            .as_u64()
            .cmp(&a["rankingScore"].as_u64())
            .then(
                a["observationId"]
                    .as_str()
                    .cmp(&b["observationId"].as_str()),
            )
    });
    surprises.truncate(options.surprise_limit);
    let bridges:Vec<_>=surprises.iter().map(|s|json!({"kind":"bridge","question":format!("Why does this {} relationship cross a source boundary?",s["relation"].as_str().unwrap_or("candidate")),"observationId":s["observationId"],"anchor":s["anchor"]})).collect();
    let isolated:Vec<_>=graph.entities.iter().filter(|e|e.kind!="file"&&derived.degree[&e.id]<=1).map(|e|json!({"kind":"isolation","question":format!("Is {} intentionally isolated from the captured graph?",e.qualified_name),"anchor":anchor(snapshot,e)})).collect();
    let mut buckets = [unresolved, inferred, bridges, isolated].map(|v| v.into_iter());
    let mut questions = Vec::new();
    while questions.len() < options.question_limit {
        let before = questions.len();
        for bucket in &mut buckets {
            if let Some(q) = bucket.next() {
                questions.push(q);
                if questions.len() == options.question_limit {
                    break;
                }
            }
        }
        if questions.len() == before {
            break;
        }
    }
    let mut diff = Value::Null;
    let mut previous_graph_hash = None;
    if let Some((old_graph, previous_diff)) = previous {
        diff = previous_diff.clone();
        previous_graph_hash = Some(identity(old_graph));
        let old_derived = derive(old_graph, options, deadline)?;
        let mut used = BTreeSet::new();
        for community in &mut derived.communities {
            let current: BTreeSet<_> = community["stableMemberKeys"]
                .as_array()
                .unwrap()
                .iter()
                .filter_map(Value::as_str)
                .collect();
            let mut candidates = Vec::new();
            for before in &old_derived.communities {
                let id = before["id"].as_str().unwrap();
                if used.contains(id) {
                    continue;
                }
                let prior: BTreeSet<_> = before["stableMemberKeys"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .filter_map(Value::as_str)
                    .collect();
                let intersection = current.intersection(&prior).count();
                let union = current.union(&prior).count();
                if intersection > 0 && union > 0 {
                    let score = intersection as f64 / union as f64;
                    if score >= 0.5 {
                        candidates.push((score, id.to_owned()));
                    }
                }
            }
            candidates.sort_by(|a, b| b.0.total_cmp(&a.0).then(a.1.cmp(&b.1)));
            if let Some((score, id)) = candidates.first() {
                if candidates.get(1).is_none_or(|next| next.0 < *score) {
                    used.insert(id.clone());
                    community["previousCommunityId"] = json!(id);
                    community["previousMemberOverlap"] = json!(score);
                } else {
                    community["previousMatchAmbiguous"] = json!(true);
                }
            }
        }
    }
    let bindings: Vec<_> = graph.entities.iter().map(|e| anchor(snapshot, e)).collect();
    let mut report = json!({"producer":PRODUCER,"workspaceId":snapshot.workspace_id,"snapshotId":snapshot.id,"graphHash":identity(graph),"previousSnapshotId":options.previous_snapshot_id,"previousGraphHash":previous_graph_hash,"options":options,"sourceBindings":bindings,"evidenceAnchors":graph.evidence,"audit":{"totalEntities":graph.entities.len(),"totalObservations":graph.observations.len(),"byResolution":by_resolution,"byRelation":by_relation,"unresolvedObservations":graph.observations.iter().filter(|e|e.target.is_none()).count(),"omissions":snapshot.omissions,"coverage":graph.coverage,"confidenceMeaning":"source_and_analysis_categories_not_probabilities"},"godNodes":gods,"communities":derived.communities,"clustering":{"algorithm":"leiden","producer":"leiden-rs-0.8.1","resolution":options.resolution,"seed":options.seed,"maxIterations":options.max_iterations,"maxCommunitySize":options.max_community_size,"uncertainWeight":options.uncertain_weight,"excludeHubsPercentile":options.exclude_hubs_percentile,"excludedHubs":derived.excluded,"quality":derived.quality,"projection":"undirected_weighted_resolved_observations_parallel_weight_retained","cancellation":"before_and_after_bounded_library_call_not_inside_leiden","hubReattachment":"weighted_majority_then_standalone_may_exceed_max_community_size"},"surprises":surprises,"questions":questions,"diff":diff,"modelCalls":0,"complete":false,"limitations":LIMITATIONS});
    if let Some(details) = derived.community_details {
        report["clustering"]["communityPolicy"] = details;
        report["clustering"]["projection"] =
            json!("undirected_sum_positive_resolved_observation_weights");
        report["clustering"]["hubReattachment"] =
            json!("distinct_neighbor_majority_then_stranded_then_bounded_split");
        report["clustering"]["hubPercentilePolicy"] =
            json!("floor_count_times_percentile_minus_one_clamped_unique_positive_degree");
    }
    Ok(report)
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ViewReportRecord {
    schema_version: u32,
    producer: String,
    graph_view_id: String,
    view_artifact_id: String,
    snapshot_id: String,
    workspace_id: String,
    base_graph_hash: String,
    projection_graph_hash: String,
    artifact_id: String,
    markdown_artifact_id: String,
    options: AnalysisOptions,
}

/// A prepared report has no durable effects. Its private source binding is
/// checked again at publication, including when used with another Store.
pub struct PreparedViewReport {
    view: crate::graph_view::View,
    report: Value,
    record: ViewReportRecord,
    bytes: Vec<u8>,
    markdown: Vec<u8>,
}

impl PreparedViewReport {
    pub fn report(&self) -> &Value {
        &self.report
    }
    pub fn publish(self, store: &Store, deadline: &Deadline) -> Result<Value> {
        store.set_deadline(deadline)?;
        let transaction = rusqlite::Transaction::new_unchecked(
            &store.db,
            rusqlite::TransactionBehavior::Immediate,
        )?;
        let graph = crate::graph_view::validated_projection(store, &self.view, deadline)?;
        ensure!(
            identity(&graph) == self.record.projection_graph_hash,
            "Prepared view report projection changed"
        );
        let record = serde_json::to_value(&self.record)?;
        if let Some(existing) = view_report_record_value(store, &self.record.artifact_id)? {
            ensure!(
                existing == record,
                "Stored view report registration differs"
            );
        }
        deadline.check()?;
        ensure!(
            store.artifact(&self.bytes)? == self.record.artifact_id,
            "View report artifact identity differs"
        );
        ensure!(
            store.artifact(&self.markdown)? == self.record.markdown_artifact_id,
            "View report Markdown identity differs"
        );
        store.db.execute(
            "INSERT INTO records(kind,id,value) VALUES(?1,?2,?3) ON CONFLICT(kind,id) DO NOTHING",
            rusqlite::params![
                VIEW_REPORT_KIND,
                self.record.artifact_id,
                serde_json::to_string(&record)?
            ],
        )?;
        ensure!(
            view_report_record_value(store, &self.record.artifact_id)? == Some(record),
            "Stored view report registration changed"
        );
        deadline.check()?;
        transaction.commit()?;
        Ok(self.report)
    }
}

fn view_report_record_value(store: &Store, id: &str) -> Result<Option<Value>> {
    ensure!(valid_id(id), "Invalid view report ID");
    let limit = rusqlite::limits::Limit::SQLITE_LIMIT_LENGTH;
    let previous = store
        .db
        .set_limit(limit, store.db.limit(limit)?.min(64000))?;
    let raw: rusqlite::Result<Option<String>> = store
        .db
        .query_row(
            "SELECT value FROM records WHERE kind=?1 AND id=?2",
            rusqlite::params![VIEW_REPORT_KIND, id],
            |row| row.get(0),
        )
        .optional();
    store.db.set_limit(limit, previous)?;
    raw?.map(|value| crate::json::parse(value.as_bytes(), 64000))
        .transpose()
}

fn view_report_anchors(
    report: &mut Value,
    view: &crate::graph_view::View,
    deadline: &Deadline,
) -> Result<()> {
    let nodes: BTreeMap<_, _> = view
        .nodes
        .iter()
        .map(|node| (node["id"].as_str().unwrap(), node))
        .collect();
    for section in ["godNodes", "communities", "surprises", "questions"] {
        for row in report[section].as_array_mut().unwrap() {
            deadline.check()?;
            let id = row["anchor"]["entityId"]
                .as_str()
                .ok_or_else(|| anyhow::anyhow!("View report anchor entity missing"))?
                .to_owned();
            let node = nodes
                .get(id.as_str())
                .ok_or_else(|| anyhow::anyhow!("View report anchor escaped selected nodes"))?;
            let bindings = node["sourceBindings"].as_array().unwrap();
            row["anchorEntityId"] = json!(id);
            row["anchor"] = if bindings.len() == 1 {
                bindings[0].clone()
            } else {
                Value::Null
            };
            row["anchorSelection"] = json!(if bindings.len() == 1 {
                "sole_source_binding"
            } else {
                "multiple_source_bindings"
            });
            row["sourceBindings"] = node["sourceBindings"].clone();
            row["origin"] = node["origin"].clone();
            row["sourceNode"] = (*node).clone();
        }
    }
    Ok(())
}

/// Prepare selected analysis without publishing artifacts or report records.
/// Graph projections cannot enter through the canonical report validator.
pub fn prepare_view_report(
    store: &Store,
    view: &crate::graph_view::View,
    options: &AnalysisOptions,
    deadline: &Deadline,
) -> Result<PreparedViewReport> {
    validate_options(options)?;
    ensure!(
        options.previous_snapshot_id.is_none(),
        "Selected view analysis does not support previousSnapshotId; checked view comparison is required"
    );
    let graph = crate::graph_view::validated_projection(store, view, deadline)?;
    ensure!(
        graph.entities.len() <= MAX_NODES
            && graph.observations.len() <= MAX_EDGES
            && graph.evidence.len() <= 100000,
        "Selected analysis capacity exceeded"
    );
    let snapshot = store.snapshot(&view.snapshot_id)?;
    let canonical = store.graph(&view.snapshot_id)?;
    let mut report = build_report(&snapshot, &graph, options, None, deadline)?;
    view_report_anchors(&mut report, view, deadline)?;
    report["producer"] = json!(view_producer(options));
    report["graphViewId"] = json!(view.id);
    report["viewArtifactId"] = json!(view.artifact_id);
    report["baseGraphHash"] = json!(view.base_graph_hash);
    report["projectionGraphHash"] = json!(identity(&graph));
    report["overlays"] = json!(view.overlays);
    report["sourceBindings"] = json!(view.nodes);
    report["observations"] = json!(view.observations);
    report["evidenceAnchors"] = json!(view.evidence);
    report["audit"]["coverage"] = json!({"scope":"selected_view","canonical":canonical.coverage,"selectedOverlays":view.overlays,"complete":false,"limitations":view.limitations});
    let mut by_origin = BTreeMap::<String, usize>::new();
    for observation in &view.observations {
        deadline.check()?;
        *by_origin
            .entry(observation["origin"].as_str().unwrap_or("unknown").into())
            .or_default() += 1;
    }
    report["audit"]["byOrigin"] = json!(by_origin);
    report["limitations"] = json!([
        LIMITATIONS,
        "Selected source bindings retain their original precision. Provider attestation is not established by graph analysis."
    ]);
    let bytes = serde_json::to_vec(&report)?;
    ensure!(
        bytes.len() <= MAX_BYTES,
        "Selected report artifact capacity exceeded"
    );
    let markdown = markdown(&report).into_bytes();
    ensure!(
        markdown.len() <= MAX_BYTES,
        "Selected report Markdown capacity exceeded"
    );
    let record = ViewReportRecord {
        schema_version: 1,
        producer: view_producer(options).into(),
        graph_view_id: view.id.clone(),
        view_artifact_id: view.artifact_id.clone(),
        snapshot_id: view.snapshot_id.clone(),
        workspace_id: view.workspace_id.clone(),
        base_graph_hash: view.base_graph_hash.clone(),
        projection_graph_hash: identity(&graph),
        artifact_id: hash(&bytes),
        markdown_artifact_id: hash(&markdown),
        options: options.clone(),
    };
    report["artifactId"] = json!(record.artifact_id);
    report["markdownArtifactId"] = json!(record.markdown_artifact_id);
    deadline.check()?;
    Ok(PreparedViewReport {
        view: view.clone(),
        report,
        record,
        bytes,
        markdown,
    })
}

pub fn analyze_view(
    store: &Store,
    view: &crate::graph_view::View,
    options: &AnalysisOptions,
    deadline: &Deadline,
) -> Result<Value> {
    prepare_view_report(store, view, options, deadline)?.publish(store, deadline)
}

/// Exact roots discovered by read-only reconstruction of a selected report.
pub struct ViewReportRoots {
    pub snapshot_id: String,
    pub graph_view_id: String,
    pub artifact_ids: Vec<String>,
}

pub fn validate_view_report_record(
    store: &Store,
    id: &str,
    value: &Value,
    deadline: &Deadline,
) -> Result<ViewReportRoots> {
    store.set_deadline(deadline)?;
    let record: ViewReportRecord = crate::json::decode(value.clone())?;
    ensure!(
        serde_json::to_value(&record)? == *value
            && record.schema_version == 1
            && record.producer == view_producer(&record.options)
            && record.artifact_id == id
            && record.workspace_id == store.workspace_id,
        "Invalid selected report registration"
    );
    ensure!(
        view_report_record_value(store, id)?.as_ref() == Some(value),
        "Selected report registration differs from store"
    );
    let view = crate::graph_view::load(store, &record.graph_view_id, deadline)?;
    let expected = prepare_view_report(store, &view, &record.options, deadline)?;
    ensure!(
        serde_json::to_value(&expected.record)? == *value,
        "Selected report scope, configuration or identity mismatch"
    );
    ensure!(
        store.read_artifact_limited(id, MAX_BYTES)? == expected.bytes,
        "Selected report bytes differ from checked reconstruction"
    );
    ensure!(
        store.read_artifact_limited(&record.markdown_artifact_id, MAX_BYTES)? == expected.markdown,
        "Selected report Markdown differs from checked reconstruction"
    );
    let mut artifacts = vec![
        record.artifact_id,
        record.markdown_artifact_id,
        record.view_artifact_id,
    ];
    artifacts.extend(
        view.overlays
            .iter()
            .filter_map(|overlay| overlay["artifactId"].as_str().map(str::to_owned)),
    );
    artifacts.sort();
    artifacts.dedup();
    deadline.check()?;
    Ok(ViewReportRoots {
        snapshot_id: record.snapshot_id,
        graph_view_id: record.graph_view_id,
        artifact_ids: artifacts,
    })
}

fn entity_groups<'a>(
    graph: &'a Graph,
    deadline: &Deadline,
) -> Result<BTreeMap<String, Vec<&'a Entity>>> {
    let mut groups = BTreeMap::<String, Vec<&Entity>>::new();
    let keys: BTreeMap<_, _> = graph.entities.iter().map(|e| (e.key.as_str(), e)).collect();
    for entity in &graph.entities {
        deadline.check()?;
        groups
            .entry(semantic_key(entity, &keys)?)
            .or_default()
            .push(entity);
    }
    Ok(groups)
}
fn entity_change(snapshot: &Snapshot, entity: &Entity) -> Value {
    json!({"name":entity.name,"qualifiedName":entity.qualified_name,"kind":entity.kind,"entityId":entity.id,"anchor":anchor(snapshot,entity)})
}
type ObservationGroups<'a> = (BTreeMap<String, Vec<&'a Observation>>, Vec<String>);
fn edge_groups<'a>(
    graph: &'a Graph,
    groups: &BTreeMap<String, Vec<&Entity>>,
    sources: &BTreeMap<String, String>,
    deadline: &Deadline,
) -> Result<ObservationGroups<'a>> {
    let unique: BTreeMap<_, _> = groups
        .iter()
        .filter(|(_, v)| v.len() == 1)
        .map(|(key, entities)| (entities[0].id.as_str(), key.as_str()))
        .collect();
    let evidence: BTreeMap<_, _> = graph.evidence.iter().map(|e| (e.id.as_str(), e)).collect();
    let mut result = BTreeMap::<String, Vec<&Observation>>::new();
    let mut ambiguous = Vec::new();
    for edge in &graph.observations {
        deadline.check()?;
        let Some(source) = unique.get(edge.source.as_str()) else {
            ambiguous.push(edge.id.clone());
            continue;
        };
        let target = if let Some(target) = &edge.target {
            let Some(key) = unique.get(target.as_str()) else {
                ambiguous.push(edge.id.clone());
                continue;
            };
            (*key).to_owned()
        } else {
            format!("unresolved:{}", edge.target_label)
        };
        let mut snippets = Vec::new();
        for id in &edge.evidence_ids {
            let anchor = evidence[id.as_str()];
            snippets.push(hash(utf8_range(
                &sources[&anchor.path],
                anchor.start_byte,
                anchor.end_byte,
            )?));
        }
        snippets.sort();
        let key =
            serde_json::to_string(&(source, target, &edge.relation, &edge.resolution, snippets))?;
        result.entry(key).or_default().push(edge);
    }
    for values in result.values_mut() {
        values.sort_by_key(|e| &e.id);
    }
    Ok((result, ambiguous))
}
pub fn compare(
    store: &Store,
    before: &Snapshot,
    before_graph: &Graph,
    after: &Snapshot,
    after_graph: &Graph,
    deadline: &Deadline,
) -> Result<Value> {
    ensure!(
        before.workspace_id == after.workspace_id,
        "Cross-workspace snapshot comparison refused"
    );
    let old_sources = sources(store, before, before_graph, deadline)?;
    let new_sources = sources(store, after, after_graph, deadline)?;
    let old = entity_groups(before_graph, deadline)?;
    let new = entity_groups(after_graph, deadline)?;
    let keys: BTreeSet<_> = old.keys().chain(new.keys()).cloned().collect();
    let mut added = Vec::new();
    let mut removed = Vec::new();
    let mut modified = Vec::new();
    let mut relocated = Vec::new();
    let mut ambiguous = Vec::new();
    let mut unchanged = 0usize;
    for key in keys {
        deadline.check()?;
        let a = old.get(&key).map(Vec::as_slice).unwrap_or(&[]);
        let b = new.get(&key).map(Vec::as_slice).unwrap_or(&[]);
        if a.len() > 1 || b.len() > 1 {
            ambiguous.push(json!({"semanticKey":key,"before":a.iter().map(|e|entity_change(before,e)).collect::<Vec<_>>(),"after":b.iter().map(|e|entity_change(after,e)).collect::<Vec<_>>() }));
            continue;
        }
        match (a.first(), b.first()) {
            (None, Some(e)) => added.push(entity_change(after, e)),
            (Some(e), None) => removed.push(entity_change(before, e)),
            (Some(a), Some(b)) => {
                let before_hash =
                    hash(utf8_range(&old_sources[&a.path], a.start_byte, a.end_byte)?);
                let after_hash = hash(utf8_range(&new_sources[&b.path], b.start_byte, b.end_byte)?);
                if before_hash != after_hash {
                    let mut change = entity_change(after, b);
                    change["beforeAnchor"] = anchor(before, a);
                    change["beforeBodyHash"] = json!(before_hash);
                    change["afterBodyHash"] = json!(after_hash);
                    modified.push(change);
                } else {
                    unchanged += 1;
                    if a.start_byte != b.start_byte
                        || a.end_byte != b.end_byte
                        || a.start_line != b.start_line
                        || a.end_line != b.end_line
                    {
                        let mut change = entity_change(after, b);
                        change["beforeAnchor"] = anchor(before, a);
                        relocated.push(change);
                    }
                }
            }
            _ => (),
        }
    }
    let (old_edges, old_ambiguous) = edge_groups(before_graph, &old, &old_sources, deadline)?;
    let (new_edges, new_ambiguous) = edge_groups(after_graph, &new, &new_sources, deadline)?;
    let edge_keys: BTreeSet<_> = old_edges.keys().chain(new_edges.keys()).cloned().collect();
    let mut added_edges = Vec::new();
    let mut removed_edges = Vec::new();
    for key in edge_keys {
        deadline.check()?;
        let a = old_edges.get(&key).map(Vec::as_slice).unwrap_or(&[]);
        let b = new_edges.get(&key).map(Vec::as_slice).unwrap_or(&[]);
        if b.len() > a.len() {
            added_edges.extend(b[a.len()..].iter().map(|edge|json!({"observation":edge,"evidenceAnchors":after_graph.evidence.iter().filter(|e|edge.evidence_ids.contains(&e.id)).collect::<Vec<_>>()})));
        } else if a.len() > b.len() {
            removed_edges.extend(a[b.len()..].iter().map(|edge|json!({"observation":edge,"evidenceAnchors":before_graph.evidence.iter().filter(|e|edge.evidence_ids.contains(&e.id)).collect::<Vec<_>>()})));
        }
    }
    deadline.check()?;
    Ok(
        json!({"workspaceId":after.workspace_id,"beforeSnapshotId":before.id,"afterSnapshotId":after.id,"beforeGraphHash":identity(before_graph),"afterGraphHash":identity(after_graph),"identityMethod":"path_kind_qualified_name_parent_context_unique_only","observationMethod":"semantic_endpoints_relation_resolution_exact_evidence_snippet_hash_multiset","addedEntities":added,"removedEntities":removed,"modifiedEntities":modified,"relocatedEntities":relocated,"ambiguousEntityGroups":ambiguous,"unchangedEntities":unchanged,"addedObservations":added_edges,"removedObservations":removed_edges,"ambiguousObservations":{"before":old_ambiguous,"after":new_ambiguous},"complete":false,"limitations":"Semantic keys are structural labels, not compiler identity; rename/move matching across paths is not inferred. Duplicate semantic keys remain ambiguous. Parallel observation multiplicity is preserved."}),
    )
}
