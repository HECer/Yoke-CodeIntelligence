//! Derived graph exports. Generation never overwrites source or human cards.
use crate::{
    capture::Deadline,
    graph_extensions::validate_scope,
    model::{Entity, Graph, Observation, Snapshot, hash, identity, valid_id},
    paths::{is_link, normalized},
    store::Store,
};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::Write,
    path::{Component, Path},
};

pub const EXPORT_KIND: &str = "native_graph_export_v1";
pub const VIEW_EXPORT_KIND: &str = "native_graph_view_export_v1";

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ExportFormat {
    Cypher,
    Svg,
    Canvas,
    Wiki,
    Obsidian,
    Tree,
    Callflow,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ExportOptions {
    pub format: ExportFormat,
    #[serde(default = "default_nodes")]
    pub max_nodes: usize,
    #[serde(default = "default_edges")]
    pub max_edges: usize,
}
fn default_nodes() -> usize {
    1000
}
fn default_edges() -> usize {
    10000
}
impl ExportOptions {
    pub fn new(format: ExportFormat) -> Self {
        Self {
            format,
            max_nodes: 1000,
            max_edges: 10000,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ExportFile {
    pub path: String,
    pub media_type: String,
    pub content: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ExportBundle {
    pub snapshot_id: String,
    pub workspace_id: String,
    pub graph_hash: String,
    pub format: ExportFormat,
    pub files: Vec<ExportFile>,
    pub omitted_nodes: usize,
    pub omitted_observations: usize,
    pub complete: bool,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub artifact_id: String,
}
const MAX_BYTES: usize = 64_000_000;
const LIMIT_NOTE: &str = "Partial structural graph: not runtime or compiler proof. Directions and parallel observations are retained. Unresolved targets remain explicit.";

struct Projection<'a> {
    source_view_metadata: Option<Value>,
    nodes: Vec<&'a Entity>,
    edges: Vec<&'a Observation>,
    extra: BTreeMap<String, String>,
    index: BTreeMap<String, usize>,
    graph_hash: String,
    relationships: BTreeMap<String, Vec<usize>>,
}
impl Projection<'_> {
    fn target(&self, edge: &Observation) -> String {
        edge.target
            .clone()
            .unwrap_or_else(|| format!("unresolved_{}", edge.id))
    }
    fn label(&self, id: &str) -> String {
        self.index
            .get(id)
            .and_then(|i| self.nodes.get(*i))
            .map(|e| e.qualified_name.clone())
            .or_else(|| self.extra.get(id).cloned())
            .unwrap_or_else(|| "Unresolved target".into())
    }
    fn position(&self, id: &str) -> (i64, i64) {
        let index = self.index.get(id).copied().unwrap_or(0);
        (
            ((index % 5) * 240 + 100) as i64,
            ((index / 5) * 160 + 90) as i64,
        )
    }
}
fn projection<'a>(
    graph: &'a Graph,
    options: &ExportOptions,
    deadline: &Deadline,
) -> Result<Projection<'a>> {
    let mut nodes: Vec<_> = graph.entities.iter().collect();
    nodes.sort_by(|a, b| {
        (&a.path, &a.qualified_name, &a.kind, a.start_byte, &a.id).cmp(&(
            &b.path,
            &b.qualified_name,
            &b.kind,
            b.start_byte,
            &b.id,
        ))
    });
    nodes.truncate(options.max_nodes);
    let selected: BTreeSet<_> = nodes.iter().map(|n| n.id.as_str()).collect();
    let mut edges = Vec::new();
    let mut extra = BTreeMap::new();
    for edge in &graph.observations {
        deadline.check()?;
        if edges.len() == options.max_edges {
            break;
        }
        if selected.contains(edge.source.as_str())
            && edge
                .target
                .as_ref()
                .is_none_or(|id| selected.contains(id.as_str()))
        {
            if edge.target.is_none() {
                extra.insert(
                    format!("unresolved_{}", edge.id),
                    format!("Unresolved: {}", edge.target_label),
                );
            }
            edges.push(edge);
        }
    }
    edges.sort_by_key(|e| &e.id);
    let index = nodes
        .iter()
        .map(|n| n.id.clone())
        .chain(extra.keys().cloned())
        .enumerate()
        .map(|(i, id)| (id, i))
        .collect();
    let mut relationships = BTreeMap::<String, Vec<usize>>::new();
    for (index, edge) in edges.iter().enumerate() {
        relationships
            .entry(edge.source.clone())
            .or_default()
            .push(index);
        if let Some(target) = &edge.target
            && target != &edge.source
        {
            relationships.entry(target.clone()).or_default().push(index);
        }
    }
    Ok(Projection {
        source_view_metadata: None,
        nodes,
        edges,
        extra,
        index,
        graph_hash: identity(graph),
        relationships,
    })
}
fn xml(text: &str) -> String {
    let valid: String = text
        .chars()
        .map(|c| {
            if c == '\t'
                || c == '\n'
                || c == '\r'
                || (('\u{20}'..='\u{d7ff}').contains(&c))
                || (('\u{e000}'..='\u{fffd}').contains(&c))
                || c >= '\u{10000}'
            {
                c
            } else {
                '\u{fffd}'
            }
        })
        .collect();
    valid
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}
fn md(text: &str) -> String {
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
fn json_string(text: &str) -> Result<String> {
    Ok(serde_json::to_string(text)?)
}
fn code_id(id: &str) -> String {
    format!("n{id}")
}
fn metadata(snapshot: &Snapshot, graph: &Graph, view: &Projection<'_>) -> Value {
    if let Some(value) = &view.source_view_metadata {
        return value.clone();
    }
    let files: BTreeMap<_, _> = snapshot
        .files
        .iter()
        .map(|f| (f.path.as_str(), f.hash.as_str()))
        .collect();
    let source_bindings:Vec<_>=view.nodes.iter().map(|n|json!({"entityId":n.id,"path":n.path,"contentHash":files[n.path.as_str()],"startByte":n.start_byte,"endByte":n.end_byte,"startLine":n.start_line,"endLine":n.end_line,"anchor":"exact"})).collect();
    let evidence_ids: BTreeSet<_> = view
        .edges
        .iter()
        .flat_map(|edge| edge.evidence_ids.iter().map(String::as_str))
        .collect();
    let evidence: Vec<_> = graph
        .evidence
        .iter()
        .filter(|a| evidence_ids.contains(a.id.as_str()))
        .collect();
    json!({"snapshotId":snapshot.id,"workspaceId":snapshot.workspace_id,"graphHash":view.graph_hash,"producer":"yoke-graph-exports-v1","complete":false,"omittedNodes":graph.entities.len()-view.nodes.len(),"omittedObservations":graph.observations.len()-view.edges.len(),"sourceBindings":source_bindings,"evidenceAnchors":evidence,"displaySanitization":"markup_escaped_XML10_forbidden_controls_replaced_canonical_source_unchanged","limitations":LIMIT_NOTE})
}
fn cypher(
    snapshot: &Snapshot,
    graph: &Graph,
    view: &Projection<'_>,
    deadline: &Deadline,
) -> Result<String> {
    let mut text = format!(
        "// {LIMIT_NOTE}\n// snapshot {} graph {}\n",
        snapshot.id, view.graph_hash
    );
    if view.source_view_metadata.is_some() {
        text.push_str(&format!(
            "// sourceView {}\n",
            serde_json::to_string(&metadata(snapshot, graph, view))?
        ));
    }
    let source_hashes: BTreeMap<_, _> = snapshot
        .files
        .iter()
        .map(|f| (f.path.as_str(), f.hash.as_str()))
        .collect();
    let evidence: BTreeMap<_, _> = graph
        .evidence
        .iter()
        .map(|anchor| (anchor.id.as_str(), anchor))
        .collect();
    for node in &view.nodes {
        deadline.check()?;
        let source_hash = source_hashes[node.path.as_str()];
        text.push_str(&format!("MERGE (n:YokeEntity {{id: {}}}) SET n.snapshotId = {}, n.workspaceId = {}, n.label = {}, n.kind = {}, n.path = {}, n.startByte = {}, n.endByte = {}, n.contentHash = {};\n",json_string(&node.id)?,json_string(&snapshot.id)?,json_string(&snapshot.workspace_id)?,json_string(&node.qualified_name)?,json_string(&node.kind)?,json_string(&node.path)?,node.start_byte,node.end_byte,json_string(source_hash)?));
    }
    for (id, label) in &view.extra {
        text.push_str(&format!(
            "MERGE (n:YokeUnresolved {{id: {}}}) SET n.label = {};\n",
            json_string(id)?,
            json_string(label)?
        ));
    }
    for edge in &view.edges {
        deadline.check()?;
        let anchors: Vec<_> = edge
            .evidence_ids
            .iter()
            .map(|id| evidence[id.as_str()])
            .collect();
        text.push_str(&format!("MATCH (s {{id: {}}}), (t {{id: {}}}) MERGE (s)-[r:YOKE_OBSERVATION {{id: {}}}]->(t) SET r.relation = {}, r.resolution = {}, r.evidenceIds = {}, r.snapshotId = {}, r.evidenceAnchorsJson = {};\n",json_string(&edge.source)?,json_string(&view.target(edge))?,json_string(&edge.id)?,json_string(&edge.relation)?,json_string(&edge.resolution)?,serde_json::to_string(&edge.evidence_ids)?,json_string(&snapshot.id)?,json_string(&serde_json::to_string(&anchors)?)?));
    }
    Ok(text)
}
fn canvas(
    snapshot: &Snapshot,
    graph: &Graph,
    view: &Projection<'_>,
    deadline: &Deadline,
) -> Result<String> {
    let mut nodes = Vec::new();
    for node in &view.nodes {
        deadline.check()?;
        let (x, y) = view.position(&node.id);
        nodes.push(json!({"id":node.id,"type":"text","x":x,"y":y,"width":220,"height":120,"text":format!("{}\n{}:{}\nUTF-8 bytes {}–{}\n{}",md(&node.qualified_name),md(&node.path),node.start_line,node.start_byte,node.end_byte,LIMIT_NOTE)}));
    }
    for (id, label) in &view.extra {
        let (x, y) = view.position(id);
        nodes.push(
            json!({"id":id,"type":"text","x":x,"y":y,"width":220,"height":120,"text":md(label)}),
        );
    }
    let edges:Vec<_>=view.edges.iter().map(|e|json!({"id":e.id,"fromNode":e.source,"fromSide":"right","fromEnd":"none","toNode":view.target(e),"toSide":"left","toEnd":"arrow","label":format!("{} · {}",e.relation,e.resolution),"yokeEvidenceIds":e.evidence_ids})).collect();
    Ok(serde_json::to_string_pretty(
        &json!({"nodes":nodes,"edges":edges,"yoke":metadata(snapshot,graph,view)}),
    )?)
}
fn svg(
    snapshot: &Snapshot,
    graph: &Graph,
    view: &Projection<'_>,
    deadline: &Deadline,
) -> Result<String> {
    let height = ((view.index.len().max(1).div_ceil(5)) * 160 + 70).max(300);
    let mut text = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"1250\" height=\"{height}\" viewBox=\"0 0 1250 {height}\" role=\"img\"><title>Yoke source graph</title><desc>{}</desc><metadata>{}</metadata><defs><marker id=\"arrow\" markerWidth=\"8\" markerHeight=\"8\" refX=\"7\" refY=\"4\" orient=\"auto\"><path d=\"M0 0L8 4L0 8Z\" fill=\"#587d91\"/></marker></defs><rect width=\"100%\" height=\"100%\" fill=\"#f5f9fc\"/>",
        xml(LIMIT_NOTE),
        xml(&serde_json::to_string(&metadata(snapshot, graph, view))?)
    );
    let mut parallel: BTreeMap<(String, String), usize> = BTreeMap::new();
    for edge in &view.edges {
        deadline.check()?;
        let target = view.target(edge);
        let (x1, y1) = view.position(&edge.source);
        let (x2, y2) = view.position(&target);
        let ordinal = parallel.entry((edge.source.clone(), target)).or_default();
        let bend = 20 + ((*ordinal % 9) as i64) * 12;
        *ordinal += 1;
        let path = if x1 == x2 && y1 == y2 {
            format!(
                "M{} {}C{} {},{} {},{} {}",
                x1 + 12,
                y1 - 12,
                x1 + 100,
                y1 - 100,
                x1 - 100,
                y1 - 100,
                x1 - 12,
                y1 - 12
            )
        } else {
            format!(
                "M{x1} {y1}Q{} {},{x2} {y2}",
                (x1 + x2) / 2,
                (y1 + y2) / 2 - bend
            )
        };
        text.push_str(&format!("<path id=\"e{}\" d=\"{path}\" fill=\"none\" stroke=\"#587d91\" marker-end=\"url(#arrow)\"><title>{}</title></path>",edge.id,xml(&format!("{} → {}: {} · {} · evidence {}",view.label(&edge.source),view.label(&view.target(edge)),edge.relation,edge.resolution,edge.evidence_ids.join(",")))));
    }
    for (id, index) in &view.index {
        deadline.check()?;
        let (x, y) = view.position(id);
        let node = view.nodes.get(*index);
        let title = node
            .map(|n| {
                format!(
                    "{} {}:{} UTF-8 bytes {}–{}",
                    n.qualified_name, n.path, n.start_line, n.start_byte, n.end_byte
                )
            })
            .unwrap_or_else(|| view.label(id));
        let label = view.label(id).chars().take(28).collect::<String>();
        text.push_str(&format!("<g id=\"{}\"><title>{}</title><circle cx=\"{x}\" cy=\"{y}\" r=\"12\" fill=\"{}\"/><text x=\"{}\" y=\"{}\" font-size=\"12\" fill=\"#132f42\">{}</text></g>",code_id(id),xml(&title),if node.is_some(){"#15697a"}else{"#9b623d"},x-85,y+30,xml(&label)));
    }
    text.push_str("</svg>");
    Ok(text)
}
fn cards(
    snapshot: &Snapshot,
    graph: &Graph,
    view: &Projection<'_>,
    format: ExportFormat,
    deadline: &Deadline,
) -> Result<Vec<ExportFile>> {
    let mut files = Vec::new();
    let graph_hash = &view.graph_hash;
    let mut index = format!(
        "# Yoke source graph\n\n{}\n\nSnapshot `{}`\n\nGraph `{}`\n\n",
        md(LIMIT_NOTE),
        snapshot.id,
        graph_hash
    );
    if view.source_view_metadata.is_some() {
        index.push_str(&format!(
            "## Immutable view provenance\n\n    {}\n\n",
            serde_json::to_string(&metadata(snapshot, graph, view))?
        ));
    }
    for node in &view.nodes {
        deadline.check()?;
        let filename = format!("nodes/{}.md", node.id);
        let mut content = format!(
            "---\nyoke_snapshot: {}\nyoke_entity: {}\nyoke_graph: {}\n---\n\n# {}\n\n{}\n\nSource: {}:{}\n\nUTF-8 bytes {}–{}\n\nKind: {}\n\n## Relationships\n\n",
            snapshot.id,
            node.id,
            graph_hash,
            md(&node.qualified_name),
            md(LIMIT_NOTE),
            md(&node.path),
            node.start_line,
            node.start_byte,
            node.end_byte,
            md(&node.kind)
        );
        for index in view.relationships.get(&node.id).into_iter().flatten() {
            deadline.check()?;
            let edge = view.edges[*index];
            let outgoing = edge.source == node.id;
            let other = if outgoing {
                view.target(edge)
            } else {
                edge.source.clone()
            };
            let link = if view.extra.contains_key(&other) {
                md(&view.label(&other))
            } else if format == ExportFormat::Obsidian {
                format!("[[{}|{}]]", other, md(&view.label(&other)))
            } else {
                format!("[{}]({}.md)", md(&view.label(&other)), other)
            };
            content.push_str(&format!(
                "- {} {} · {} · {} · observation `{}` · evidence {}\n",
                if outgoing { "→" } else { "←" },
                md(&edge.relation),
                md(&edge.resolution),
                link,
                edge.id,
                edge.evidence_ids
                    .iter()
                    .map(|id| format!("`{id}`"))
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }
        content.push_str("\n## Human notes\n\nThis fresh generated card may be edited. Re-export to a new directory; existing notes are never overwritten.\n");
        index.push_str(&format!("- [{}]({filename})\n", md(&node.qualified_name)));
        files.push(ExportFile {
            path: filename,
            media_type: "text/markdown".into(),
            content,
        });
    }
    if format == ExportFormat::Wiki {
        let chosen: BTreeSet<_> = view.nodes.iter().map(|e| e.id.as_str()).collect();
        let by_id: BTreeMap<_, _> = view.nodes.iter().map(|e| (e.id.as_str(), *e)).collect();
        for community in crate::graph_extensions::communities(graph, deadline)? {
            deadline.check()?;
            let members: Vec<_> = community["members"]
                .as_array()
                .unwrap()
                .iter()
                .filter_map(Value::as_str)
                .filter(|id| chosen.contains(id))
                .collect();
            if members.is_empty() {
                continue;
            }
            let id = community["id"].as_str().unwrap();
            let label = community["label"].as_str().unwrap();
            let mut content = format!(
                "# {}\n\n{}\n\nSnapshot `{}`\n\nGraph `{}`\n\nCommunity derived with Leiden; unique undirected positive-weight cohesion {}. This article contains {} selected members.\n\n",
                md(label),
                md(LIMIT_NOTE),
                snapshot.id,
                graph_hash,
                community["cohesion"],
                members.len()
            );
            for member in members {
                let node = by_id[member];
                content.push_str(&format!(
                    "- [{}](../nodes/{}.md) · {}:{} · UTF-8 bytes {}–{}\n",
                    md(&node.qualified_name),
                    member,
                    md(&node.path),
                    node.start_line,
                    node.start_byte,
                    node.end_byte
                ));
            }
            content.push_str("\n## Human notes\n\nRe-export to a new directory; existing notes are never overwritten.\n");
            let path = format!("communities/{id}.md");
            index.push_str(&format!("- Community [{}]({path})\n", md(label)));
            files.push(ExportFile {
                path,
                media_type: "text/markdown".into(),
                content,
            });
        }
    }
    files.insert(
        0,
        ExportFile {
            path: "index.md".into(),
            media_type: "text/markdown".into(),
            content: index,
        },
    );
    if format == ExportFormat::Obsidian {
        files.push(ExportFile {
            path: "graph.canvas".into(),
            media_type: "application/json".into(),
            content: canvas(snapshot, graph, view, deadline)?,
        });
    }
    Ok(files)
}
fn html(
    snapshot: &Snapshot,
    graph: &Graph,
    view: &Projection<'_>,
    format: ExportFormat,
    deadline: &Deadline,
) -> Result<String> {
    let nodes:Vec<_>=view.nodes.iter().map(|n|json!({"id":n.id,"name":n.qualified_name,"path":n.path,"line":n.start_line,"startByte":n.start_byte,"endByte":n.end_byte,"kind":n.kind})).collect();
    let edges:Vec<_>=view.edges.iter().map(|e|json!({"id":e.id,"source":e.source,"target":view.target(e),"relation":e.relation,"resolution":e.resolution,"evidenceIds":e.evidence_ids})).collect();
    let data = serde_json::to_string(
        &json!({"nodes":nodes,"edges":edges,"metadata":metadata(snapshot,graph,view)}),
    )?
    .replace('&', "\\u0026")
    .replace('<', "\\u003c")
    .replace('>', "\\u003e")
    .replace('\u{2028}', "\\u2028")
    .replace('\u{2029}', "\\u2029");
    let mut text = format!(
        "<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width,initial-scale=1\"><meta http-equiv=\"Content-Security-Policy\" content=\"default-src 'none'; script-src 'unsafe-inline'; style-src 'unsafe-inline'; connect-src 'none'; base-uri 'none'; form-action 'none'\"><title>Yoke {}</title><style>body{{font:16px system-ui;background:#f5f9fc;color:#132f42;margin:24px}}details{{margin:8px 0 8px 16px}}summary{{cursor:pointer}}pre{{white-space:pre-wrap;overflow-wrap:anywhere}}svg{{width:100%;height:auto}}input{{font:inherit;padding:8px}}button{{font:inherit;margin:4px;padding:6px}}</style></head><body><h1>Yoke {}</h1><p>{}</p><p>Snapshot <code>{}</code></p>",
        if format == ExportFormat::Tree {
            "source tree"
        } else {
            "architecture and callflow"
        },
        if format == ExportFormat::Tree {
            "source tree"
        } else {
            "architecture and callflow"
        },
        xml(LIMIT_NOTE),
        snapshot.id
    );
    if format == ExportFormat::Callflow {
        text.push_str(&svg(snapshot, graph, view, deadline)?);
        let mut mermaid = String::from("flowchart LR\n");
        for id in view.index.keys() {
            let label = view
                .label(id)
                .replace('&', "&amp;")
                .replace('"', "#quot;")
                .replace('<', "#lt;")
                .replace('>', "#gt;")
                .replace('[', "#91;")
                .replace(']', "#93;")
                .replace(['\n', '\r'], " ");
            mermaid.push_str(&format!("  {}[\"{}\"]\n", code_id(id), label));
        }
        for edge in &view.edges {
            mermaid.push_str(&format!(
                "  {} --> {}\n",
                code_id(&edge.source),
                code_id(&view.target(edge))
            ));
        }
        text.push_str(&format!("<details><summary>Editable Mermaid source; native SVG above works offline</summary><pre>{}</pre></details>",xml(&mermaid)));
    }
    text.push_str("<label>Filter source tree <input id=\"search\" type=\"search\"></label><div id=\"tree\"></div><h2>Selected source and observations</h2><pre id=\"detail\"></pre><script id=\"data\" type=\"application/json\">");
    text.push_str(&data);
    text.push_str(r#"</script><script>'use strict';const data=JSON.parse(document.getElementById('data').textContent),tree=document.getElementById('tree'),search=document.getElementById('search'),detail=document.getElementById('detail');function make(tag,text){const e=document.createElement(tag);e.textContent=text;return e;}function render(){tree.replaceChildren();const groups=new Map();for(const n of data.nodes){if(!(n.name+' '+n.path).toLowerCase().includes(search.value.toLowerCase()))continue;let parent=tree,key='';for(const part of n.path.split('/')){key+='/'+part;if(!groups.has(key)){const d=document.createElement('details');d.append(make('summary',part));parent.append(d);groups.set(key,d);}parent=groups.get(key);}const b=make('button',n.name+' · '+n.kind);b.addEventListener('click',()=>{detail.textContent=n.name+'\n'+n.path+':'+n.line+'\nUTF-8 bytes '+n.startByte+'–'+n.endByte+'\n'+JSON.stringify(data.edges.filter(e=>e.source===n.id||e.target===n.id),null,2);});parent.append(b);}}search.addEventListener('input',render);render();</script></body></html>"#);
    Ok(text)
}
pub(crate) fn validate_bundle(bundle: &ExportBundle, deadline: &Deadline) -> Result<()> {
    deadline.check()?;
    ensure!(
        valid_id(&bundle.snapshot_id)
            && valid_id(&bundle.workspace_id)
            && valid_id(&bundle.graph_hash)
            && !bundle.complete,
        "Invalid export scope"
    );
    ensure!(
        !bundle.files.is_empty() && bundle.files.len() <= 10002,
        "Export file capacity exceeded"
    );
    let mut paths = BTreeSet::new();
    let mut total = 0usize;
    for file in &bundle.files {
        deadline.check()?;
        normalized(&file.path)?;
        ensure!(
            file.path.len() <= 200 && paths.insert(file.path.to_ascii_lowercase()),
            "Duplicate or invalid export file path"
        );
        ensure!(
            file.path == "index.md"
                || file.path == "graph.canvas"
                || file.path == "graph.cypher"
                || file.path == "graph.svg"
                || file.path == "tree.html"
                || file.path == "callflow.html"
                || ["nodes/", "communities/"].iter().any(|prefix| file
                    .path
                    .strip_prefix(prefix)
                    .and_then(|s| s.strip_suffix(".md"))
                    .is_some_and(valid_id)),
            "Unsupported export file path"
        );
        total = total
            .checked_add(file.content.len())
            .ok_or_else(|| anyhow::anyhow!("Export capacity overflow"))?;
        ensure!(total <= MAX_BYTES, "Export byte capacity exceeded");
    }
    let mut unsealed = bundle.clone();
    unsealed.artifact_id.clear();
    ensure!(
        valid_id(&bundle.artifact_id) && hash(serde_json::to_vec(&unsealed)?) == bundle.artifact_id,
        "Export bundle hash mismatch"
    );
    Ok(())
}
pub fn generate(
    store: &Store,
    snapshot: &Snapshot,
    graph: &Graph,
    options: &ExportOptions,
    deadline: &Deadline,
) -> Result<ExportBundle> {
    validate_scope(store, snapshot, graph, deadline)?;
    generate_inner(store, snapshot, graph, options, deadline, None)
}
/// The validated view is the only entry point for rendering a derived projection.
/// Canonical `generate` continues to require the exact published base graph.
pub(crate) fn generate_verified(
    store: &Store,
    view: &crate::graph_view::View,
    options: &ExportOptions,
    deadline: &Deadline,
) -> Result<ExportBundle> {
    let graph = crate::graph_view::validated_projection(store, view, deadline)?;
    let snapshot = store.snapshot(&view.snapshot_id)?;
    generate_inner(store, &snapshot, &graph, options, deadline, Some(view))
}
fn generate_inner(
    store: &Store,
    snapshot: &Snapshot,
    graph: &Graph,
    options: &ExportOptions,
    deadline: &Deadline,
    graph_view: Option<&crate::graph_view::View>,
) -> Result<ExportBundle> {
    ensure!(
        (1..=5000).contains(&options.max_nodes) && (1..=50000).contains(&options.max_edges),
        "Invalid export projection limits"
    );
    let mut view = projection(graph, options, deadline)?;
    if let Some(source_view) = graph_view {
        let selected: BTreeSet<_> = view.nodes.iter().map(|node| node.id.as_str()).collect();
        view.source_view_metadata = Some(
            json!({"graphViewId":source_view.id,"viewArtifactId":source_view.artifact_id,"snapshotId":snapshot.id,"workspaceId":snapshot.workspace_id,"baseGraphHash":source_view.base_graph_hash,"projectionGraphHash":identity(graph),"overlays":source_view.overlays,"sourceBindings":source_view.nodes.iter().filter(|node|node["id"].as_str().is_some_and(|id|selected.contains(id))).collect::<Vec<_>>(),"evidenceAnchors":source_view.evidence,"complete":false,"omittedNodes":graph.entities.len()-view.nodes.len(),"omittedObservations":graph.observations.len()-view.edges.len(),"limitations":source_view.limitations}),
        );
    }
    let files = match options.format {
        ExportFormat::Cypher => vec![ExportFile {
            path: "graph.cypher".into(),
            media_type: "text/plain".into(),
            content: cypher(snapshot, graph, &view, deadline)?,
        }],
        ExportFormat::Svg => vec![ExportFile {
            path: "graph.svg".into(),
            media_type: "image/svg+xml".into(),
            content: svg(snapshot, graph, &view, deadline)?,
        }],
        ExportFormat::Canvas => vec![ExportFile {
            path: "graph.canvas".into(),
            media_type: "application/json".into(),
            content: canvas(snapshot, graph, &view, deadline)?,
        }],
        ExportFormat::Wiki | ExportFormat::Obsidian => {
            cards(snapshot, graph, &view, options.format, deadline)?
        }
        ExportFormat::Tree | ExportFormat::Callflow => vec![ExportFile {
            path: if options.format == ExportFormat::Tree {
                "tree.html"
            } else {
                "callflow.html"
            }
            .into(),
            media_type: "text/html".into(),
            content: html(snapshot, graph, &view, options.format, deadline)?,
        }],
    };
    let mut bundle = ExportBundle {
        snapshot_id: snapshot.id.clone(),
        workspace_id: snapshot.workspace_id.clone(),
        graph_hash: identity(graph),
        format: options.format,
        files,
        omitted_nodes: graph.entities.len() - view.nodes.len(),
        omitted_observations: graph.observations.len() - view.edges.len(),
        complete: false,
        artifact_id: String::new(),
    };
    let bytes = serde_json::to_vec(&bundle)?;
    ensure!(
        bytes.len() <= MAX_BYTES,
        "Export artifact capacity exceeded"
    );
    bundle.artifact_id = hash(&bytes);
    validate_bundle(&bundle, deadline)?;
    store.set_deadline(deadline)?;
    let transaction =
        rusqlite::Transaction::new_unchecked(&store.db, rusqlite::TransactionBehavior::Immediate)?;
    deadline.check()?;
    let artifact = store.artifact(&bytes)?;
    ensure!(
        artifact == bundle.artifact_id,
        "Export artifact identity mismatch"
    );
    let mut record = json!({"snapshotId":snapshot.id,"workspaceId":snapshot.workspace_id,"graphHash":bundle.graph_hash,"artifactId":artifact,"format":options.format,"options":options});
    let record_kind = if let Some(graph_view) = graph_view {
        record["graphViewId"] = json!(graph_view.id);
        record["baseGraphHash"] = json!(graph_view.base_graph_hash);
        VIEW_EXPORT_KIND
    } else {
        EXPORT_KIND
    };
    store.db.execute(
        "INSERT INTO records(kind,id,value) VALUES(?1,?2,?3) ON CONFLICT(kind,id) DO NOTHING",
        rusqlite::params![record_kind, artifact, serde_json::to_string(&record)?],
    )?;
    ensure!(
        store.get_record::<Value>(record_kind, &artifact)? == record,
        "Stored export record differs"
    );
    deadline.check()?;
    transaction.commit()?;
    Ok(bundle)
}
pub fn write_new_bundle(
    bundle: &ExportBundle,
    destination: &Path,
    deadline: &Deadline,
) -> Result<()> {
    validate_bundle(bundle, deadline)?;
    ensure!(
        destination.is_absolute(),
        "Export destination must be an absolute host-selected path"
    );
    for component in destination.components() {
        ensure!(
            !matches!(component, Component::ParentDir | Component::CurDir),
            "Export destination is noncanonical"
        );
        #[cfg(windows)]
        if let Component::Prefix(prefix) = component {
            ensure!(
                matches!(
                    prefix.kind(),
                    std::path::Prefix::Disk(_) | std::path::Prefix::VerbatimDisk(_)
                ),
                "Network export paths refused"
            );
        }
    }
    let parent = destination
        .parent()
        .ok_or_else(|| anyhow::anyhow!("Export destination needs a parent"))?;
    for ancestor in parent.ancestors() {
        let meta = fs::symlink_metadata(ancestor)?;
        ensure!(
            meta.is_dir() && !is_link(&meta),
            "Export ancestor link or non-directory refused"
        );
    }
    ensure!(
        fs::symlink_metadata(destination)
            .is_err_and(|error| error.kind() == std::io::ErrorKind::NotFound),
        "Export destination already exists or is unavailable; use a new directory to preserve human notes"
    );
    deadline.check()?;
    fs::create_dir(destination)?;
    for file in &bundle.files {
        deadline.check()?;
        let path = destination.join(&file.path);
        if let Some(parent) = path.parent()
            && parent != destination
        {
            match fs::create_dir(parent) {
                Ok(()) => (),
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                    let meta = fs::symlink_metadata(parent)?;
                    ensure!(
                        meta.is_dir() && !is_link(&meta),
                        "Export child directory replaced"
                    );
                }
                Err(error) => return Err(error.into()),
            }
        }
        let mut output = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)?;
        for chunk in file.content.as_bytes().chunks(65536) {
            deadline.check()?;
            output.write_all(chunk)?;
        }
        output.sync_all()?;
    }
    let manifest = json!({"producer":"yoke-graph-exports-v1","snapshotId":bundle.snapshot_id,"graphHash":bundle.graph_hash,"artifactId":bundle.artifact_id,"humanNotesPolicy":"create_only_new_directory_no_overwrite","complete":false,"files":bundle.files.iter().map(|file|json!({"path":file.path,"sha256":hash(file.content.as_bytes()),"bytes":file.content.len()})).collect::<Vec<_>>()});
    deadline.check()?;
    let mut output = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(destination.join("yoke-export-manifest.json"))?;
    output.write_all(&serde_json::to_vec_pretty(&manifest)?)?;
    output.sync_all()?;
    deadline.check()?;
    Ok(())
}
