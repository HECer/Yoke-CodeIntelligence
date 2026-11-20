use anyhow::{Context, Result, ensure};
use clap::Args;
use serde::Serialize;
use serde_json::{Map, Value, json};
use std::{
    borrow::Cow,
    collections::{BTreeMap, HashSet},
    io::{self, Write},
    path::{Path, PathBuf},
};
use yoke::{capture::Deadline, model::hash};

pub(super) const INPUT: &str = "graph_diagnostics_input_invalid";
const GRAPH_BYTES: usize = 32_000_000;
const PRODUCER_BYTES: usize = 8_000_000;

#[derive(Args)]
pub(super) struct Arguments {
    /// Host-selected Graphify graph or raw extraction JSON.
    #[arg(long)]
    input: PathBuf,
    /// Optional host-selected extractor source for a heuristic suppression scan.
    #[arg(long)]
    producer_source: Option<PathBuf>,
    /// Maximum number of examples in each list. Counts always cover the full input.
    #[arg(long, default_value_t = 5)]
    examples: usize,
}

impl Arguments {
    pub(super) fn run(&self, timeout_ms: u64, max_bytes: u64) -> Result<bool> {
        ensure!(
            (1..=600_000).contains(&timeout_ms)
                && (2..=4_000_000).contains(&max_bytes)
                && self.examples <= 50,
            INPUT
        );
        let deadline = Deadline::new(timeout_ms);
        let bytes = read_selected(&self.input, GRAPH_BYTES, &deadline)?;
        let mut graph = yoke::json::parse(&bytes, GRAPH_BYTES)?;
        graph.sort_all_objects();
        let (counts, examples) = audit(&graph, self.examples, &deadline)?;
        let producer = self
            .producer_source
            .as_ref()
            .map(|path| scan_producer(path, self.examples, &deadline))
            .transpose()?;
        let result = json!({
            "schemaVersion": 1,
            "inputSha256": hash(&bytes),
            "declaredDirected": graph["directed"].as_bool(),
            "counts": counts,
            "parallelExamples": examples,
            "producerSuppression": producer,
            "comparisonScope": "supplied_edges_before_hypothetical_endpoint_collapse"
        });
        let output = yoke::json::encode_bounded(&result, max_bytes as usize - 1, &deadline)?;
        deadline.check()?;
        let mut stdout = io::stdout().lock();
        stdout.write_all(&output)?;
        stdout.write_all(b"\n")?;
        stdout.flush()?;
        Ok(true)
    }
}

fn read_selected(path: &Path, cap: usize, deadline: &Deadline) -> Result<Vec<u8>> {
    let absolute = std::path::absolute(path)?;
    super::meaning_archive_cli::read_input(&absolute, cap, deadline)
        .context("Graph diagnostics could not read the selected regular host file")
}

pub(super) fn node_id(value: &Value) -> Option<Cow<'_, str>> {
    match value {
        Value::String(id) if !id.is_empty() => Some(Cow::Borrowed(id)),
        Value::Number(id) if id.is_i64() || id.is_u64() => Some(Cow::Owned(id.to_string())),
        _ => None,
    }
}

fn endpoint<'a>(edge: &'a Map<String, Value>, name: &str, alias: &str) -> Option<Cow<'a, str>> {
    match (edge.get(name), edge.get(alias)) {
        (Some(primary), Some(legacy)) if node_id(primary)? == node_id(legacy)? => node_id(primary),
        (Some(value), None) | (None, Some(value)) => node_id(value),
        _ => None,
    }
}

pub(super) fn directed_endpoints(
    edge: &Map<String, Value>,
) -> Option<(Cow<'_, str>, Cow<'_, str>)> {
    let source = endpoint(edge, "source", "from")?;
    let target = endpoint(edge, "target", "to")?;
    match (edge.get("_src"), edge.get("_tgt")) {
        (None, None) => Some((source, target)),
        (Some(original_source), Some(original_target)) => {
            let original_source = node_id(original_source)?;
            let original_target = node_id(original_target)?;
            ((original_source == source && original_target == target)
                || (original_source == target && original_target == source))
                .then_some((original_source, original_target))
        }
        _ => None,
    }
}

fn edge_digest(edge: &Map<String, Value>) -> Result<String> {
    let mut normalized: BTreeMap<_, _> = edge
        .iter()
        .filter(|(key, _)| key.as_str() != "from" && key.as_str() != "to")
        .map(|(key, value)| (key.as_str(), value))
        .collect();
    for (name, alias) in [("source", "from"), ("target", "to")] {
        if let Some(value) = edge.get(alias) {
            normalized.entry(name).or_insert(value);
        }
    }
    Ok(hash(serde_json::to_vec(&normalized)?))
}

#[derive(Default, Serialize)]
#[serde(rename_all = "camelCase")]
struct Counts {
    node_rows: usize,
    unique_nodes: usize,
    invalid_node_rows: usize,
    duplicate_node_ids: usize,
    unverified_node_rows: usize,
    edge_rows: usize,
    non_object_edges: usize,
    invalid_endpoint_edges: usize,
    dangling_endpoint_edges: usize,
    external_reference_edges: usize,
    bound_edges: usize,
    exact_duplicate_edges: usize,
    self_loops: usize,
    directed_endpoint_pairs: usize,
    directed_parallel_groups: usize,
    directed_collapse_loss: usize,
    reverse_direction_groups: usize,
    undirected_endpoint_pairs: usize,
    undirected_collapse_loss: usize,
    relation_variant_groups: usize,
    source_file_variant_groups: usize,
    source_location_variant_groups: usize,
    context_variant_groups: usize,
}

#[derive(Default)]
struct Pair<'a> {
    observations: usize,
    by_relation: BTreeMap<String, Relation<'a>>,
}

struct Relation<'a> {
    first_fields: [Option<&'a Value>; 3],
    varied: [bool; 3],
}

impl<'a> Pair<'a> {
    fn record(&mut self, edge: &'a Map<String, Value>) {
        self.observations += 1;
        let name = edge
            .get("relation")
            .map(Value::to_string)
            .unwrap_or_default();
        let fields = ["source_file", "source_location", "context"].map(|key| edge.get(key));
        let relation = self.by_relation.entry(name).or_insert(Relation {
            first_fields: fields,
            varied: [false; 3],
        });
        for (index, field) in fields.into_iter().enumerate() {
            relation.varied[index] |= field != relation.first_fields[index];
        }
    }
}

fn audit(graph: &Value, max_examples: usize, deadline: &Deadline) -> Result<(Counts, Vec<Value>)> {
    let nodes = graph["nodes"].as_array().context(INPUT)?;
    let edges = if graph["edges"].is_null() {
        &graph["links"]
    } else {
        &graph["edges"]
    }
    .as_array()
    .context(INPUT)?;
    let mut counts = Counts {
        node_rows: nodes.len(),
        edge_rows: edges.len(),
        ..Counts::default()
    };
    let mut known = HashSet::new();
    for node in nodes {
        deadline.check()?;
        if let Some(id) = node_id(&node["id"]) {
            counts.unverified_node_rows += usize::from(node["verification"] == "unverified");
            counts.duplicate_node_ids += usize::from(!known.insert(id));
        } else {
            counts.invalid_node_rows += 1;
        }
    }
    counts.unique_nodes = known.len();
    let mut seen = HashSet::new();
    let mut directed: BTreeMap<_, Pair<'_>> = BTreeMap::new();
    for edge in edges {
        deadline.check()?;
        let Some(edge) = edge.as_object() else {
            counts.non_object_edges += 1;
            continue;
        };
        let Some((source, target)) = directed_endpoints(edge) else {
            counts.invalid_endpoint_edges += 1;
            continue;
        };
        counts.exact_duplicate_edges += usize::from(!seen.insert(edge_digest(edge)?));
        let source_known = known.contains(&source);
        if source_known && known.contains(&target) {
            counts.bound_edges += 1;
            counts.self_loops += usize::from(source == target);
            directed.entry((source, target)).or_default().record(edge);
        } else if source_known && target.starts_with("ref_") {
            counts.external_reference_edges += 1;
        } else {
            counts.dangling_endpoint_edges += 1;
        }
    }

    counts.directed_endpoint_pairs = directed.len();
    counts.directed_collapse_loss = counts.bound_edges - directed.len();
    let mut parallel = Vec::new();
    for (endpoints, group) in &directed {
        deadline.check()?;
        let (source, target) = endpoints;
        if source < target && directed.contains_key(&(target.clone(), source.clone())) {
            counts.reverse_direction_groups += 1;
        }
        if group.observations > 1 {
            counts.directed_parallel_groups += 1;
            if max_examples > 0 {
                parallel.push((endpoints, group.observations));
            }
        }
        counts.relation_variant_groups += usize::from(group.by_relation.len() > 1);
        for relation in group.by_relation.values() {
            counts.source_file_variant_groups += usize::from(relation.varied[0]);
            counts.source_location_variant_groups += usize::from(relation.varied[1]);
            counts.context_variant_groups += usize::from(relation.varied[2]);
        }
    }
    counts.undirected_endpoint_pairs = directed.len() - counts.reverse_direction_groups;
    counts.undirected_collapse_loss = counts.bound_edges - counts.undirected_endpoint_pairs;
    parallel.sort_unstable_by(|(left, a), (right, b)| b.cmp(a).then_with(|| left.cmp(right)));
    deadline.check()?;
    let examples = parallel
        .into_iter()
        .take(max_examples)
        .map(|((source, target), observations)| {
            json!({"source": source, "target": target, "observations": observations})
        })
        .collect();
    Ok((counts, examples))
}

fn scan_producer(path: &Path, max_examples: usize, deadline: &Deadline) -> Result<Value> {
    let bytes = read_selected(path, PRODUCER_BYTES, deadline)?;
    let text =
        std::str::from_utf8(&bytes).context("Graph diagnostics producer source is not UTF-8")?;
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let mut count = 0;
    let mut examples = Vec::new();
    for (index, line) in text.lines().enumerate() {
        deadline.check()?;
        let line = line.trim_start();
        if !line.starts_with("seen_") {
            continue;
        }
        let end = line
            .bytes()
            .take_while(|byte| byte.is_ascii_alphanumeric() || *byte == b'_')
            .count();
        if end > "seen_".len() && line[end..].trim_start().starts_with([':', '=']) {
            count += 1;
            if examples.len() < max_examples {
                examples.push(json!({"line": index + 1, "name": &line[..end]}));
            }
        }
    }
    Ok(json!({
        "sourceSha256": hash(&bytes),
        "matchingLines": count,
        "examples": examples,
        "kind": "heuristic_seen_declarations"
    }))
}
