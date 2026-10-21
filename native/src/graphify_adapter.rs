//! Independent source-bound adapters for the pinned Graphify public surface.
use crate::{
    capture::Deadline,
    graph_extensions::{self, AnalysisOptions},
    graph_view::{self, BuildOptions, View},
    project_session::Session,
};
use anyhow::{Context, Result, bail, ensure};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
const MAX_OUTPUT: usize = 256_000;
fn definitions() -> Vec<(&'static str, &'static str, Value, Vec<&'static str>)> {
    vec![
        (
            "query_graph",
            "Search checked captured graph context with bounded BFS or DFS.",
            json!({"question":{"type":"string"},"mode":{"type":"string","enum":["bfs","dfs"],"default":"bfs"},"depth":{"type":"integer","default":3},"token_budget":{"type":"integer","default":2000},"context_filter":{"type":"array","items":{"type":"string"}}}),
            vec!["question"],
        ),
        (
            "get_node",
            "Resolve a label or ID; ambiguous labels return source-bound candidates.",
            json!({"label":{"type":"string"},"node_id":{"type":"string"}}),
            vec![],
        ),
        (
            "get_neighbors",
            "Read incoming and outgoing parallel observations and exact evidence sites.",
            json!({"label":{"type":"string"},"node_id":{"type":"string"},"relation_filter":{"type":"string"},"token_budget":{"type":"integer","default":2000}}),
            vec![],
        ),
        (
            "get_community",
            "Read deterministic size-ranked Leiden communities over a checked selected view.",
            json!({"community_id":{"type":"integer"},"token_budget":{"type":"integer","default":2000}}),
            vec!["community_id"],
        ),
        (
            "god_nodes",
            "Rank resolved parallel degree, with optional percentile hub exclusion.",
            json!({"top_n":{"type":"integer","default":10},"exclude_hubs_percentile":{"type":"number"}}),
            vec![],
        ),
        (
            "graph_stats",
            "Count checked view nodes and observations; confidence categories are source claims.",
            json!({}),
            vec![],
        ),
        (
            "shortest_path",
            "Find a bounded directed shortest path; undirected is an explicit option.",
            json!({"source":{"type":"string"},"target":{"type":"string"},"max_hops":{"type":"integer","default":8},"undirected":{"type":"boolean","default":false}}),
            vec!["source", "target"],
        ),
        (
            "list_prs",
            "List actual hosted PR metadata when a host read provider is available.",
            json!({"base":{"type":"string"},"repo":{"type":"string"}}),
            vec![],
        ),
        (
            "get_pr_impact",
            "Read actual hosted PR files and source-bound impact when a host provider is available.",
            json!({"pr_number":{"type":"integer"},"repo":{"type":"string"}}),
            vec!["pr_number"],
        ),
        (
            "triage_prs",
            "Read actionable hosted PR metadata when a host provider is available.",
            json!({"base":{"type":"string"},"repo":{"type":"string"}}),
            vec![],
        ),
    ]
}
pub fn tools() -> Value {
    Value::Array(definitions().into_iter().map(|(name,description,mut properties,required)|{
 properties["project_path"]=json!({"type":"string","description":"Absolute canonical root already registered by the host; omission selects the active project."});
 if matches!(name,"query_graph"|"get_node"|"get_neighbors"|"get_community"|"god_nodes"|"graph_stats"|"shortest_path") {
 properties["graphViewId"]=json!({"type":"string","pattern":"^[a-f0-9]{64}$","description":"Already published immutable view; omission uses the default structural view."});
 }
 json!({"name":name,"description":description,"inputSchema":{"type":"object","properties":properties,"required":required,"additionalProperties":false},"annotations":{"readOnlyHint":true,"destructiveHint":false,"openWorldHint":matches!(name,"list_prs"|"get_pr_impact"|"triage_prs")}})
}).collect())
}
pub fn resources() -> Value {
    Value::Array([
 ("report","Graph Report","text/markdown"),("stats","Graph Stats","text/plain"),("god-nodes","God Nodes","text/plain"),("surprises","Surprising Connections","text/plain"),("audit","Confidence Audit","text/plain"),("questions","Suggested Questions","text/plain")
].into_iter().map(|(key,name,mime)|json!({"uri":format!("graphify://{key}"),"name":name,"mimeType":mime,"description":"Verified immutable source-bound graph analysis; heuristic claims retain their limitations."})).collect())
}
fn bounded(args: &Value, key: &str, min: usize, max: usize) -> Result<usize> {
    let n = args[key].as_u64().context("Bounded integer required")?;
    ensure!(
        (min as u64..=max as u64).contains(&n),
        "Argument {key} exceeds supported bounds"
    );
    Ok(n as usize)
}
fn text<'a>(args: &'a Value, key: &str) -> Result<&'a str> {
    let s = args[key].as_str().context("Text argument required")?;
    ensure!(
        !s.trim().is_empty() && s.len() <= 4096,
        "Text argument capacity exceeded"
    );
    Ok(s)
}
fn label(args: &Value) -> Result<&str> {
    match (args.get("label"), args.get("node_id")) {
        (Some(a), Some(b)) => {
            ensure!(a == b, "Conflicting label and node_id");
            text(args, "label")
        }
        (Some(_), None) => text(args, "label"),
        (None, Some(_)) => text(args, "node_id"),
        _ => bail!("Provide label or node_id"),
    }
}
fn id(v: &Value) -> &str {
    v["id"].as_str().unwrap_or("")
}
fn matches(view: &View, label: &str, deadline: &Deadline) -> Result<Vec<usize>> {
    let key = label.to_lowercase();
    let mut exact = vec![];
    let mut partial = vec![];
    for (i, n) in view.nodes.iter().enumerate() {
        deadline.check()?;
        let labels = [
            id(n),
            n["name"].as_str().unwrap_or(""),
            n["qualifiedName"].as_str().unwrap_or(""),
        ];
        if labels.iter().any(|s| s.to_lowercase() == key) {
            exact.push(i);
        } else if labels
            .iter()
            .any(|s| !s.is_empty() && s.to_lowercase().contains(&key))
        {
            partial.push(i);
        }
    }
    let hits = if exact.is_empty() { partial } else { exact };
    ensure!(
        hits.len() <= 100,
        "Node label matches too many candidates; use an exact ID"
    );
    Ok(hits)
}
fn identity_result(view: &View) -> Value {
    json!({"snapshotId":view.snapshot_id,"viewId":view.id,"baseGraphHash":view.base_graph_hash,"complete":false,"truncated":false})
}
fn ambiguity(view: &View, hits: &[usize]) -> Value {
    let mut out = identity_result(view);
    out["ambiguous"] = json!(hits.len() > 1);
    out["found"] = json!(!hits.is_empty());
    out["totalCandidates"] = json!(hits.len());
    out["candidates"] = json!(hits.iter().map(|i| &view.nodes[*i]).collect::<Vec<_>>());
    out
}
fn context(relation: &str) -> &str {
    match relation {
        "calls" | "call" => "call",
        "imports" | "import" | "depends_on" | "declares_package" | "references_project" => "import",
        "field" | "field_type" | "reads" | "writes" => "field",
        "parameter_type" => "parameter_type",
        "returns" | "return_type" => "return_type",
        "generic_arg" => "generic_arg",
        "attribute" | "decorator" => "attribute",
        "exports" => "export",
        other => other,
    }
}
fn context_name(value: &str) -> Result<String> {
    let key = value.trim().to_lowercase();
    let key = match key.as_str() {
        "calls" | "called" | "invoke" | "invocation" => "call",
        "fields" | "property" | "properties" | "member" | "members" => "field",
        "imports" | "imported" | "module" | "modules" => "import",
        "param" | "params" | "parameter" | "parameters" | "argument" | "arguments" | "arg"
        | "args" => "parameter_type",
        "return" | "returns" | "returned" => "return_type",
        "generic" | "generics" | "template" | "templates" => "generic_arg",
        "annotation" | "annotations" | "decorator" | "decorators" => "attribute",
        "exports" | "exported" => "export",
        _ => key.as_str(),
    };
    ensure!(
        [
            "call",
            "import",
            "field",
            "parameter_type",
            "return_type",
            "generic_arg",
            "attribute",
            "export",
            "references",
            "contains",
            "implements",
            "extends",
            "inherits",
            "config_reference",
            "markdown_reference",
            "wiki_reference",
            "symbol_citation"
        ]
        .contains(&key),
        "Unsupported edge context filter"
    );
    Ok(key.into())
}
fn filters(args: &Value) -> Result<BTreeSet<String>> {
    let mut out = BTreeSet::new();
    if let Some(values) = args["context_filter"].as_array() {
        ensure!(values.len() <= 32, "Too many context filters");
        for value in values {
            out.insert(context_name(
                value.as_str().context("Context string required")?,
            )?);
        }
    }
    Ok(out)
}
fn accepted(edge: &Value, filter: &BTreeSet<String>) -> bool {
    filter.is_empty() || filter.contains(context(edge["relation"].as_str().unwrap_or("")))
}
fn selected(view: &View, nodes: &[usize], edges: &[usize]) -> Value {
    let evidence: BTreeSet<_> = edges
        .iter()
        .flat_map(|i| {
            view.observations[*i]["evidenceIds"]
                .as_array()
                .into_iter()
                .flatten()
        })
        .filter_map(Value::as_str)
        .collect();
    let mut out = identity_result(view);
    out["nodes"] = json!(nodes.iter().map(|i| &view.nodes[*i]).collect::<Vec<_>>());
    out["observations"] = json!(
        edges
            .iter()
            .map(|i| &view.observations[*i])
            .collect::<Vec<_>>()
    );
    out["evidence"] = json!(
        view.evidence
            .iter()
            .filter(|e| evidence.contains(id(e)))
            .collect::<Vec<_>>()
    );
    out
}
fn prune(out: &mut Value) {
    let nodes: BTreeSet<String> = out["nodes"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|n| id(n).to_owned())
        .collect();
    if let Some(edges) = out["observations"].as_array_mut() {
        edges.retain(|e| {
            nodes.contains(e["source"].as_str().unwrap_or(""))
                && (e["target"].is_null() || nodes.contains(e["target"].as_str().unwrap_or("")))
        });
    }
    let ev: BTreeSet<String> = out["observations"]
        .as_array()
        .into_iter()
        .flatten()
        .flat_map(|e| e["evidenceIds"].as_array().into_iter().flatten())
        .filter_map(Value::as_str)
        .map(str::to_owned)
        .collect();
    if let Some(evidence) = out["evidence"].as_array_mut() {
        evidence.retain(|e| ev.contains(id(e)));
    }
}
fn budget(mut out: Value, tokens: usize, deadline: &Deadline) -> Result<Value> {
    // Pinned presentation uses approximately three characters per token. UTF-8
    // bytes bound character count more strictly; no model tokenizer is asserted.
    let cap = tokens
        .checked_mul(3)
        .context("Token budget overflow")?
        .min(MAX_OUTPUT);
    out["budget"] = json!({"maxUtf8Bytes":cap,"tokenEstimate":"utf8_bytes_divided_by_three_not_model_tokenization"});
    loop {
        deadline.check()?;
        if serde_json::to_vec(&out)?.len() <= cap {
            return Ok(out);
        }
        out["truncated"] = json!(true);
        let mut reduced = false;
        for key in ["nodes", "candidates", "observations", "items"] {
            if let Some(items) = out[key].as_array_mut()
                && !items.is_empty()
            {
                let next = if key == "nodes" && items.len() > 2 {
                    items.len().div_ceil(2)
                } else {
                    items.len() - 1
                };
                items.truncate(next);
                reduced = true;
                break;
            }
        }
        if reduced {
            prune(&mut out);
        } else if out["community"].get("detailsOmitted").is_none() && out["community"].is_object() {
            out["community"] = json!({"id":out["community"]["id"],"memberCount":out["community"]["memberCount"],"detailsOmitted":"response_budget_use_reportArtifactId"});
        } else {
            bail!("Output metadata exceeds token budget; raise token_budget");
        }
    }
}
fn walk(view: &View, args: &Value, deadline: &Deadline) -> Result<Value> {
    let depth = bounded(args, "depth", 1, 6)?;
    let mut filter = filters(args)?;
    let query = text(args, "question")?;
    let mut filter_source = if filter.is_empty() {
        Value::Null
    } else {
        json!("explicit")
    };
    if filter.is_empty() {
        let words: BTreeSet<_> = query
            .split(|c: char| !c.is_alphanumeric() && c != '_')
            .map(str::to_lowercase)
            .collect();
        for (category, hints) in [
            (
                "call",
                &[
                    "call", "calls", "called", "caller", "callers", "invoke", "invokes", "invoked",
                ][..],
            ),
            (
                "import",
                &["import", "imports", "imported", "module", "modules"][..],
            ),
            (
                "field",
                &[
                    "field",
                    "fields",
                    "member",
                    "members",
                    "property",
                    "properties",
                ][..],
            ),
            (
                "parameter_type",
                &[
                    "parameter",
                    "parameters",
                    "param",
                    "params",
                    "argument",
                    "arguments",
                ][..],
            ),
            ("return_type", &["return", "returns", "returned"][..]),
            (
                "generic_arg",
                &["generic", "generics", "template", "templates"][..],
            ),
        ] {
            if hints.iter().any(|word| words.contains(*word)) {
                filter.insert(category.to_owned());
            }
        }
        if !filter.is_empty() {
            filter_source = json!("heuristic");
        }
    }
    let mut seeds = matches(view, query, deadline)?;
    if seeds.is_empty() {
        let terms: Vec<_> = query
            .split(|c: char| !c.is_alphanumeric() && c != '_')
            .filter(|s| s.chars().count() > 1)
            .map(str::to_lowercase)
            .take(32)
            .collect();
        let mut ranked = vec![];
        for (i, n) in view.nodes.iter().enumerate() {
            deadline.check()?;
            let label = format!(
                "{} {}",
                n["qualifiedName"].as_str().unwrap_or(""),
                n["path"].as_str().unwrap_or("")
            )
            .to_lowercase();
            let score = terms.iter().filter(|s| label.contains(s.as_str())).count();
            if score > 0 {
                ranked.push((score, i));
            }
        }
        ranked.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
        seeds = ranked.into_iter().take(8).map(|(_, i)| i).collect();
    }
    let candidates = seeds.len();
    seeds.truncate(8);
    let index: BTreeMap<_, _> = view
        .nodes
        .iter()
        .enumerate()
        .map(|(i, n)| (id(n), i))
        .collect();
    let mut adjacency = vec![vec![]; view.nodes.len()];
    let mut degree = vec![0usize; view.nodes.len()];
    for edge in &view.observations {
        deadline.check()?;
        if let (Some(a), Some(b)) = (
            index.get(edge["source"].as_str().unwrap_or("")),
            index.get(edge["target"].as_str().unwrap_or("")),
        ) {
            degree[*a] += 1;
            degree[*b] += 1;
            if accepted(edge, &filter) {
                adjacency[*a].push(*b);
            }
        }
    }
    for adjacent in &mut adjacency {
        adjacent.sort();
        adjacent.dedup();
    }
    let mut distribution = degree.clone();
    distribution.sort();
    let hub = distribution
        .get(distribution.len() * 99 / 100)
        .copied()
        .unwrap_or(0)
        .max(50);
    let seedset: BTreeSet<_> = seeds.iter().copied().collect();
    let mut visited = BTreeSet::new();
    let mut order = vec![];
    let dfs = args["mode"] == "dfs";
    let mut pending: VecDeque<_> = seeds.iter().map(|i| (*i, 0)).collect();
    while let Some((i, hops)) = if dfs {
        pending.pop_back()
    } else {
        pending.pop_front()
    } {
        deadline.check()?;
        if hops > depth || !visited.insert(i) {
            continue;
        }
        order.push(i);
        if hops == depth || (!seedset.contains(&i) && degree[i] >= hub) {
            continue;
        }
        for next in &adjacency[i] {
            pending.push_back((*next, hops + 1));
        }
    }
    ensure!(
        order.len() <= 5000,
        "Traversal result exceeds node capacity"
    );
    let edges: Vec<_> = view
        .observations
        .iter()
        .enumerate()
        .filter(|(_, e)| {
            accepted(e, &filter)
                && index
                    .get(e["source"].as_str().unwrap_or(""))
                    .is_some_and(|i| visited.contains(i))
                && (e["target"].is_null()
                    || index
                        .get(e["target"].as_str().unwrap_or(""))
                        .is_some_and(|i| visited.contains(i)))
        })
        .map(|(i, _)| i)
        .collect();
    let mut out = selected(view, &order, &edges);
    out["mode"] = args["mode"].clone();
    out["depth"] = json!(depth);
    out["matchedSeeds"] = json!(candidates);
    out["seedSelectionTruncated"] = json!(candidates > 8);
    out["hubTransitThreshold"] = json!(hub);
    out["contextFilter"] = json!(filter);
    out["contextFilterSource"] = filter_source;
    out["traversalDirection"] = json!("stored_outgoing");
    out["totalSelectedNodes"] = json!(order.len());
    out["totalSelectedObservations"] = json!(edges.len());
    budget(out, bounded(args, "token_budget", 128, 65536)?, deadline)
}
fn shortest(view: &View, args: &Value, deadline: &Deadline) -> Result<Value> {
    let max = bounded(args, "max_hops", 1, 64)?;
    let sources = matches(view, text(args, "source")?, deadline)?;
    let targets = matches(view, text(args, "target")?, deadline)?;
    if sources.len() != 1 || targets.len() != 1 {
        let mut out = identity_result(view);
        out["ambiguous"] = json!(sources.len() > 1 || targets.len() > 1);
        out["found"] = json!(false);
        out["sourceCandidates"] =
            json!(sources.iter().map(|i| &view.nodes[*i]).collect::<Vec<_>>());
        out["targetCandidates"] =
            json!(targets.iter().map(|i| &view.nodes[*i]).collect::<Vec<_>>());
        return Ok(out);
    }
    let start = sources[0];
    let end = targets[0];
    let index: BTreeMap<_, _> = view
        .nodes
        .iter()
        .enumerate()
        .map(|(i, n)| (id(n), i))
        .collect();
    let mut adjacency = vec![vec![]; view.nodes.len()];
    for (i, e) in view.observations.iter().enumerate() {
        deadline.check()?;
        if let (Some(a), Some(b)) = (
            index.get(e["source"].as_str().unwrap_or("")),
            index.get(e["target"].as_str().unwrap_or("")),
        ) {
            adjacency[*a].push((*b, i));
            if args["undirected"] == true {
                adjacency[*b].push((*a, i));
            }
        }
    }
    let mut distances = BTreeMap::from([(start, 0usize)]);
    let mut parent = BTreeMap::new();
    let mut queue = VecDeque::from([start]);
    while let Some(i) = queue.pop_front() {
        deadline.check()?;
        if i == end {
            break;
        }
        let distance = distances[&i];
        if distance == max {
            continue;
        }
        for (next, edge) in &adjacency[i] {
            if !distances.contains_key(next) {
                distances.insert(*next, distance + 1);
                parent.insert(*next, (i, *edge));
                queue.push_back(*next);
            }
        }
    }
    let found = distances.contains_key(&end);
    let mut nodes = vec![];
    let mut edges = vec![];
    if found {
        let mut at = end;
        nodes.push(at);
        while at != start {
            let (p, e) = parent[&at];
            edges.push(e);
            nodes.push(p);
            at = p;
        }
        nodes.reverse();
        edges.reverse();
    }
    let mut out = selected(view, &nodes, &edges);
    out["found"] = json!(found);
    out["hops"] = if found {
        json!(edges.len())
    } else {
        Value::Null
    };
    out["undirected"] = args["undirected"].clone();
    out["maxHops"] = json!(max);
    out["pathObservationIds"] = json!(
        edges
            .iter()
            .map(|i| id(&view.observations[*i]))
            .collect::<Vec<_>>()
    );
    Ok(out)
}
fn stats(view: &View, report: &Value) -> Value {
    let mut by_resolution = BTreeMap::<String, usize>::new();
    let mut by_origin = BTreeMap::<String, usize>::new();
    for e in &view.observations {
        *by_resolution
            .entry(e["resolution"].as_str().unwrap_or("unknown").into())
            .or_default() += 1;
        *by_origin
            .entry(e["origin"].as_str().unwrap_or("unknown").into())
            .or_default() += 1;
    }
    let mut out = identity_result(view);
    out["nodeCount"] = json!(view.nodes.len());
    out["edgeCount"] = json!(view.observations.len());
    out["communityCount"] = json!(report["communities"].as_array().map_or(0, Vec::len));
    out["byResolution"] = json!(by_resolution);
    out["byOrigin"] = json!(by_origin);
    out["unresolvedObservations"] = json!(
        view.observations
            .iter()
            .filter(|e| e["target"].is_null())
            .count()
    );
    out["confidenceMeaning"] = json!("source_and_analysis_categories_not_probabilities");
    out["communityScope"] = json!("selected_view");
    report_identity(&mut out, report);
    out["builtAtCommit"] = Value::Null;
    out["commitMetadata"] = json!("unavailable_for_disk_snapshot_no_git_execution");
    out["limitations"] = json!(view.limitations);
    out
}
fn report_identity(out: &mut Value, report: &Value) {
    for key in [
        "graphViewId",
        "viewArtifactId",
        "baseGraphHash",
        "projectionGraphHash",
    ] {
        out[key] = report[key].clone();
    }
    out["reportArtifactId"] = report["artifactId"].clone();
}
fn hubs(view: &View, report: &Value) -> Value {
    let mut out = identity_result(view);
    out["items"] = report["godNodes"].clone();
    out["clustering"] = report["clustering"].clone();
    report_identity(&mut out, report);
    out["rankingScope"] = json!("selected_view");
    out
}
fn select_view(engine: &mut crate::Engine, id: Option<&str>, deadline: &Deadline) -> Result<View> {
    if let Some(id) = id {
        ensure!(
            crate::model::valid_id(id),
            "Invalid selected analysis view ID"
        );
        let view = graph_view::load(&engine.store, id, deadline)?;
        ensure!(
            view.meaning.is_none(),
            "Graphify source graph tools and resources do not support selected meaning layers"
        );
        return Ok(view);
    }
    let (snapshot, graph) = if let Some(id) = engine.store.latest()? {
        (engine.store.snapshot(&id)?, engine.store.graph(&id)?)
    } else {
        let (snapshot, graph, _, _) = engine.index(deadline)?;
        (snapshot, graph)
    };
    graph_view::build(
        &engine.store,
        &snapshot,
        &graph,
        &BuildOptions::default(),
        deadline,
    )
}
pub fn invoke(
    session: &mut Session,
    name: &str,
    args: &Value,
    deadline: &Deadline,
) -> Result<Value> {
    deadline.check()?;
    ensure!(
        session.tool_allowed(name),
        "Tool excluded by session steering"
    );
    let inventory = tools();
    let definition = inventory
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["name"] == name)
        .context("Unknown Graphify tool")?;
    let args = crate::compatibility::arguments(args, &definition["inputSchema"])?;
    // Bounds are independent of resolution success and precede source capture.
    match name {
        "shortest_path" => {
            bounded(&args, "max_hops", 1, 64)?;
            text(&args, "source")?;
            text(&args, "target")?;
        }
        "query_graph" => {
            bounded(&args, "depth", 1, 6)?;
            bounded(&args, "token_budget", 128, 65536)?;
            text(&args, "question")?;
            filters(&args)?;
        }
        "get_neighbors" => {
            bounded(&args, "token_budget", 128, 65536)?;
            label(&args)?;
        }
        "get_node" => {
            label(&args)?;
        }
        "get_community" => {
            bounded(&args, "community_id", 0, 5000)?;
            bounded(&args, "token_budget", 128, 65536)?;
        }
        "god_nodes" => {
            let options = AnalysisOptions {
                hub_limit: bounded(&args, "top_n", 1, 100)?,
                exclude_hubs_percentile: args["exclude_hubs_percentile"].as_f64(),
                ..Default::default()
            };
            graph_extensions::validate_options(&options)?;
        }
        _ => {}
    }
    let github_profile = session.github_host().cloned();
    let engine = session.query_engine_by_root_at(args["project_path"].as_str(), deadline)?;
    if matches!(name, "list_prs" | "get_pr_impact" | "triage_prs") {
        if name == "get_pr_impact" {
            bounded(&args, "pr_number", 1, 1_000_000)?;
        }
        let profile = github_profile
            .context("Hosted PR read provider unavailable; no remote query was performed")?;
        return crate::github::query(&profile, engine, name, &args, deadline);
    }
    let view = select_view(engine, args["graphViewId"].as_str(), deadline)?;
    let mut pending_report = None;
    let result = match name {
        "query_graph" => walk(&view, &args, deadline)?,
        "get_node" | "get_neighbors" => {
            let hits = matches(&view, label(&args)?, deadline)?;
            if hits.len() != 1 {
                let tokens = if name == "get_neighbors" {
                    bounded(&args, "token_budget", 128, 65536)?
                } else {
                    65536
                };
                budget(ambiguity(&view, &hits), tokens, deadline)?
            } else {
                let at = hits[0];
                if name == "get_node" {
                    let mut out = identity_result(&view);
                    out["node"] = view.nodes[at].clone();
                    out["found"] = json!(true);
                    out
                } else {
                    let filter = args["relation_filter"]
                        .as_str()
                        .unwrap_or("")
                        .to_lowercase();
                    ensure!(filter.len() <= 256, "Relation filter capacity exceeded");
                    let edges: Vec<_> = view
                        .observations
                        .iter()
                        .enumerate()
                        .filter(|(_, e)| {
                            (e["source"] == id(&view.nodes[at])
                                || e["target"] == id(&view.nodes[at]))
                                && (filter.is_empty()
                                    || e["relation"]
                                        .as_str()
                                        .unwrap_or("")
                                        .to_lowercase()
                                        .contains(&filter))
                        })
                        .map(|(i, _)| i)
                        .collect();
                    let endpoints: BTreeSet<_> = edges
                        .iter()
                        .flat_map(|i| {
                            [
                                view.observations[*i]["source"].as_str(),
                                view.observations[*i]["target"].as_str(),
                            ]
                        })
                        .flatten()
                        .collect();
                    let mut nodes = vec![at];
                    nodes.extend(
                        view.nodes
                            .iter()
                            .enumerate()
                            .filter(|(i, n)| *i != at && endpoints.contains(id(n)))
                            .map(|(i, _)| i),
                    );
                    let mut out = selected(&view, &nodes, &edges);
                    out["totalSelectedObservations"] = json!(edges.len());
                    budget(out, bounded(&args, "token_budget", 128, 65536)?, deadline)?
                }
            }
        }
        "shortest_path" => shortest(&view, &args, deadline)?,
        "get_community" | "god_nodes" | "graph_stats" => {
            let mut options = AnalysisOptions::default();
            if name == "god_nodes" {
                options.hub_limit = bounded(&args, "top_n", 1, 100)?;
                options.exclude_hubs_percentile = args["exclude_hubs_percentile"].as_f64();
            }
            let prepared =
                graph_extensions::prepare_view_report(&engine.store, &view, &options, deadline)?;
            let report = prepared.report();
            let out = match name {
                "get_community" => {
                    let community = bounded(&args, "community_id", 0, 5000)?;
                    let communities = report["communities"]
                        .as_array()
                        .context("Report communities missing")?;
                    let mut out = if let Some(c) = communities.get(community) {
                        let members: BTreeSet<_> = c["members"]
                            .as_array()
                            .into_iter()
                            .flatten()
                            .filter_map(Value::as_str)
                            .collect();
                        let nodes: Vec<_> = view
                            .nodes
                            .iter()
                            .enumerate()
                            .filter(|(_, n)| members.contains(id(n)))
                            .map(|(i, _)| i)
                            .collect();
                        let mut out = selected(&view, &nodes, &[]);
                        let mut descriptor = c.clone();
                        let count = descriptor["members"].as_array().map_or(0, Vec::len);
                        if let Some(fields) = descriptor.as_object_mut() {
                            fields.remove("members");
                            fields.remove("stableMemberKeys");
                        }
                        descriptor["memberCount"] = json!(count);
                        out["community"] = descriptor;
                        out["totalSelectedNodes"] = json!(nodes.len());
                        out["found"] = json!(true);
                        out
                    } else {
                        let mut out = identity_result(&view);
                        out["found"] = json!(false);
                        out
                    };
                    out["communityIndex"] = json!(community);
                    out["communityScope"] = json!("selected_view");
                    report_identity(&mut out, report);
                    budget(out, bounded(&args, "token_budget", 128, 65536)?, deadline)?
                }
                "god_nodes" => hubs(&view, report),
                _ => stats(&view, report),
            };
            pending_report = Some(prepared);
            out
        }
        _ => bail!("Unknown Graphify tool"),
    };
    deadline.check()?;
    ensure!(
        serde_json::to_vec(&result)?.len() <= MAX_OUTPUT,
        "Graphify output capacity exceeded; narrow the query"
    );
    if let Some(prepared) = pending_report {
        prepared.publish(&engine.store, deadline)?;
    }
    Ok(result)
}
pub fn resource(session: &mut Session, uri: &str, deadline: &Deadline) -> Result<(String, String)> {
    deadline.check()?;
    ensure!(uri.len() <= 4096, "Graphify resource URI capacity exceeded");
    let selector = uri
        .strip_prefix("graphify://")
        .context("Unknown Graphify resource URI")?;
    let (name, query) = selector.split_once('?').unwrap_or((selector, ""));
    let mut view_id = None;
    for (key, value) in url::form_urlencoded::parse(query.as_bytes()) {
        ensure!(
            key == "graphViewId" && view_id.is_none() && crate::model::valid_id(&value),
            "Unknown, duplicate or invalid Graphify resource selector"
        );
        view_id = Some(value.into_owned());
    }
    ensure!(
        [
            "report",
            "stats",
            "god-nodes",
            "surprises",
            "audit",
            "questions"
        ]
        .contains(&name),
        "Unknown Graphify resource URI"
    );
    let engine = session.query_engine_by_root_at(None, deadline)?;
    let view = select_view(engine, view_id.as_deref(), deadline)?;
    let prepared = graph_extensions::prepare_view_report(
        &engine.store,
        &view,
        &AnalysisOptions::default(),
        deadline,
    )?;
    let report = prepared.report();
    let (mime, body) = if name == "stats" || name == "god-nodes" {
        let value = if name == "stats" {
            stats(&view, report)
        } else {
            hubs(&view, report)
        };
        ("text/plain", serde_json::to_string_pretty(&value)?)
    } else if name == "report" {
        ("text/markdown", graph_extensions::markdown(report))
    } else {
        let key = match name {
            "surprises" => "surprises",
            "questions" => "questions",
            _ => "audit",
        };
        (
            "text/plain",
            serde_json::to_string_pretty(
                &json!({"snapshotId":view.snapshot_id,"graphViewId":view.id,"graphHash":report["graphHash"],"baseGraphHash":view.base_graph_hash,"projectionGraphHash":report["projectionGraphHash"],"analysisScope":"selected_view","reportArtifactId":report["artifactId"],"data":report[key],"limitations":report["limitations"]}),
            )?,
        )
    };
    ensure!(
        body.len() <= MAX_OUTPUT,
        "Graphify resource exceeds output capacity; use bounded tool queries"
    );
    deadline.check()?;
    prepared.publish(&engine.store, deadline)?;
    Ok((mime.into(), body))
}
