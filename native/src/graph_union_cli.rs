use super::{
    graph_diagnostics_cli::{directed_endpoints, node_id},
    meaning_archive_cli::{directory_guards, read_input},
};
use anyhow::{Context, Result, bail, ensure};
use clap::Args;
use serde_json::{Map, Value, json};
use std::{
    collections::{BTreeMap, HashMap},
    io::{self, Write},
    path::{Path, PathBuf},
};
use yoke::capture::Deadline;

pub(super) const INPUT: &str = "graph_union_input_invalid";
const FILE_CAPACITY: usize = 50 * 1024 * 1024;
const NODE_CAPACITY: usize = 100_000;
const EDGE_CAPACITY: usize = 1_000_000;
const VERSION_FIELD: &str = "yokeGraphUnionVersion";
const ORIGINAL_KEY: &str = "yokeOriginalEdgeKey";

#[derive(Args)]
pub(super) struct Arguments {
    /// Git %O (the common ancestor). An empty ancestor is allowed.
    base: PathBuf,
    /// Git %A (our graph). The only file eligible for replacement.
    current: PathBuf,
    /// Git %B (the other graph).
    other: PathBuf,
    /// Replace the current Git scratch file instead of writing a preview to stdout.
    #[arg(long)]
    write_git_current: bool,
}

impl Arguments {
    pub(super) fn run(&self, timeout: u64, max_bytes: u64) -> Result<bool> {
        ensure!((1..=600_000).contains(&timeout) && max_bytes > 1, INPUT);
        let deadline = Deadline::new(timeout);
        if self.write_git_current {
            let base = scratch_name(&self.base)?;
            let current = scratch_name(&self.current)?;
            let other = scratch_name(&self.other)?;
            ensure!(base != current && base != other && current != other, INPUT);
        }
        let base = std::path::absolute(&self.base)?;
        let current = std::path::absolute(&self.current)?;
        let other = std::path::absolute(&self.other)?;
        read_input(&base, FILE_CAPACITY, &deadline)?;
        let before = read_input(&current, FILE_CAPACITY, &deadline)?;
        let left = read_graph(&before)?;
        let right = read_graph(&read_input(&other, FILE_CAPACITY, &deadline)?)?;
        let graph = union(left, right, &deadline)?;
        let size = max_bytes.min(FILE_CAPACITY as u64) as usize;
        if self.write_git_current {
            let output = yoke::json::encode_bounded(&graph, FILE_CAPACITY - 1, &deadline)?;
            replace_git_scratch(&current, &before, &output, &deadline)?;
        } else {
            let output = yoke::json::encode_bounded(&graph, size - 1, &deadline)?;
            let mut stdout = io::stdout().lock();
            deadline.check()?;
            stdout.write_all(&output)?;
            stdout.write_all(b"\n")?;
            stdout.flush()?;
        }
        Ok(true)
    }
}

fn scratch_name(path: &Path) -> Result<String> {
    let label = path.to_str().context(INPUT)?;
    let suffix = label.strip_prefix(".merge_file_").context(INPUT)?;
    ensure!(
        suffix.len() == 6 && suffix.bytes().all(|byte| byte.is_ascii_alphanumeric()),
        "Graph union writes only to Git merge scratch files"
    );
    Ok(label.to_ascii_lowercase())
}

fn replace_git_scratch(
    path: &Path,
    original: &[u8],
    output: &[u8],
    deadline: &Deadline,
) -> Result<()> {
    let parent = path.parent().context(INPUT)?;
    let _held = directory_guards(parent, deadline)?;
    let mut pending = tempfile::NamedTempFile::new_in(parent)?;
    for chunk in output.chunks(65536) {
        deadline.check()?;
        pending.write_all(chunk)?;
    }
    pending.write_all(b"\n")?;
    pending.as_file().sync_all()?;
    ensure!(
        read_input(path, FILE_CAPACITY, deadline)? == original,
        "Git merge scratch changed before publication"
    );
    deadline.check()?;
    pending.persist(path).map_err(|error| error.error)?;
    Ok(())
}

struct Graph {
    fields: Map<String, Value>,
    graph: Map<String, Value>,
    nodes: Vec<Value>,
    links: Vec<Value>,
    hyperedges: Option<Vec<Value>>,
    directed: Option<bool>,
    generated_keys: bool,
}

fn read_graph(bytes: &[u8]) -> Result<Graph> {
    let mut input = yoke::json::parse(bytes, FILE_CAPACITY)?;
    input.sort_all_objects();
    let Value::Object(mut fields) = input else {
        bail!(INPUT)
    };
    let nodes = take_rows(fields.remove("nodes"), NODE_CAPACITY)?.context(INPUT)?;
    let edges = take_rows(fields.remove("edges"), EDGE_CAPACITY)?;
    let links = take_rows(fields.remove("links"), EDGE_CAPACITY)?;
    let links = match (edges, links) {
        (Some(edges), None) => edges,
        (None, Some(links)) => links,
        _ => bail!("Graph union requires exactly one edges or links array"),
    };
    let hyperedges = take_rows(fields.remove("hyperedges"), EDGE_CAPACITY)?;
    let directed = take_flag(fields.remove("directed"))?;
    take_flag(fields.remove("multigraph"))?;
    let mut graph = match fields.remove("graph") {
        Some(Value::Object(graph)) => graph,
        None | Some(Value::Null) => Map::new(),
        _ => bail!(INPUT),
    };
    let generated_keys = match graph.remove(VERSION_FIELD) {
        None => false,
        Some(version) if version == 1 => true,
        _ => bail!(INPUT),
    };
    Ok(Graph {
        fields,
        graph,
        nodes,
        links,
        hyperedges,
        directed,
        generated_keys,
    })
}

fn take_rows(value: Option<Value>, max: usize) -> Result<Option<Vec<Value>>> {
    match value {
        Some(Value::Array(rows)) if rows.len() <= max => Ok(Some(rows)),
        None | Some(Value::Null) => Ok(None),
        _ => bail!(INPUT),
    }
}

fn take_flag(value: Option<Value>) -> Result<Option<bool>> {
    match value {
        Some(Value::Bool(flag)) => Ok(Some(flag)),
        None | Some(Value::Null) => Ok(None),
        _ => bail!(INPUT),
    }
}

fn add_nodes(
    nodes: &mut BTreeMap<String, Map<String, Value>>,
    side: Vec<Value>,
    deadline: &Deadline,
) -> Result<()> {
    let mut side_ids = std::collections::HashSet::new();
    for row in side {
        deadline.check()?;
        let Value::Object(row) = row else {
            bail!(INPUT)
        };
        let id = node_id(row.get("id").context(INPUT)?)
            .context(INPUT)?
            .into_owned();
        ensure!(
            side_ids.insert(id.clone()),
            "Graph union found duplicate nodes on one side"
        );
        nodes.entry(id).or_default().extend(row);
        ensure!(
            nodes.len() <= NODE_CAPACITY,
            "Graph union node capacity exceeded"
        );
    }
    Ok(())
}

fn normalize_links(
    side: Vec<Value>,
    generated_keys: bool,
    nodes: &BTreeMap<String, Map<String, Value>>,
    deadline: &Deadline,
) -> Result<Vec<Value>> {
    side.into_iter()
        .map(|row| {
            deadline.check()?;
            let Value::Object(mut row) = row else {
                bail!(INPUT)
            };
            let (source, target) = directed_endpoints(&row).context(INPUT)?;
            let lookup = |id: &str| {
                nodes
                    .get(id)
                    .and_then(|node| node.get("id"))
                    .cloned()
                    .unwrap_or_else(|| json!(id))
            };
            let (source, target) = (lookup(&source), lookup(&target));
            let key = row.remove("key");
            if generated_keys {
                ensure!(key.as_ref().and_then(Value::as_u64).is_some(), INPUT);
            } else {
                ensure!(!row.contains_key(ORIGINAL_KEY), INPUT);
                if let Some(key) = key {
                    row.insert(ORIGINAL_KEY.to_owned(), key);
                }
            }
            row.remove("from");
            row.remove("to");
            row.insert("_src".into(), source.clone());
            row.insert("_tgt".into(), target.clone());
            row.insert("source".into(), source);
            row.insert("target".into(), target);
            let mut normalized = Value::Object(row);
            normalized.sort_all_objects();
            Ok(normalized)
        })
        .collect()
}

fn union_rows(mut left: Vec<Value>, right: Vec<Value>, deadline: &Deadline) -> Result<Vec<Value>> {
    // Compare complete normalized rows. Matching occurrences consume distinct
    // copies on the other side, so a branch's repeated observations survive.
    let mut remaining = HashMap::<Vec<u8>, usize>::new();
    for row in &left {
        deadline.check()?;
        *remaining.entry(serde_json::to_vec(row)?).or_default() += 1;
    }
    for row in right {
        deadline.check()?;
        let signature = serde_json::to_vec(&row)?;
        match remaining.get_mut(&signature) {
            Some(count) if *count > 0 => *count -= 1,
            _ => {
                ensure!(
                    left.len() < EDGE_CAPACITY,
                    "Graph union row capacity exceeded"
                );
                left.push(row);
            }
        }
    }
    Ok(left)
}

fn union(mut left: Graph, right: Graph, deadline: &Deadline) -> Result<Value> {
    if let (Some(a), Some(b)) = (left.directed, right.directed) {
        ensure!(a == b, "Graph union found conflicting directedness");
    }
    let directed = left.directed.or(right.directed).unwrap_or(false);
    let mut nodes = BTreeMap::new();
    add_nodes(&mut nodes, left.nodes, deadline)?;
    add_nodes(&mut nodes, right.nodes, deadline)?;
    let left_links = normalize_links(left.links, left.generated_keys, &nodes, deadline)?;
    let right_links = normalize_links(right.links, right.generated_keys, &nodes, deadline)?;
    let mut links = union_rows(left_links, right_links, deadline)?;
    for (key, row) in links.iter_mut().enumerate() {
        deadline.check()?;
        row["key"] = json!(key);
    }
    if left.hyperedges.is_some() || right.hyperedges.is_some() {
        let hyperedges = union_rows(
            left.hyperedges.unwrap_or_default(),
            right.hyperedges.unwrap_or_default(),
            deadline,
        )?;
        left.fields
            .insert("hyperedges".into(), Value::Array(hyperedges));
    }
    left.graph.extend(right.graph);
    left.graph.insert(VERSION_FIELD.into(), json!(1));
    left.fields.extend(right.fields);
    left.fields
        .insert("graph".into(), Value::Object(left.graph));
    left.fields.insert("directed".into(), json!(directed));
    left.fields.insert("multigraph".into(), json!(true));
    left.fields.insert(
        "nodes".into(),
        Value::Array(nodes.into_values().map(Value::Object).collect()),
    );
    left.fields.insert("links".into(), Value::Array(links));
    Ok(Value::Object(left.fields))
}
