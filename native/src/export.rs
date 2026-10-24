use crate::{
    capture::Deadline,
    model::{Graph, Snapshot},
    paths::is_link,
    store::Store,
};
use anyhow::{Result, bail, ensure};
use std::{fs, io::Write, path::Path};

pub fn serialize(graph: &Graph, format: &str) -> Result<Vec<u8>> {
    match format {
        "json" => Ok(serde_json::to_vec_pretty(graph)?),
        "graphml" => Ok(graphml(graph)?.into_bytes()),
        "html" => html(graph, &serde_json::Value::Null),
        _ => bail!("Export format must be json, graphml or html"),
    }
}
fn script_json(value: &impl serde::Serialize) -> Result<String> {
    Ok(serde_json::to_string(value)?
        .replace('&', "\\u0026")
        .replace('<', "\\u003c")
        .replace('>', "\\u003e")
        .replace('\u{2028}', "\\u2028")
        .replace('\u{2029}', "\\u2029"))
}
fn html(graph: &Graph, communities: &serde_json::Value) -> Result<Vec<u8>> {
    html_parts(&script_json(graph)?, &script_json(communities)?, false)
}
fn html_parts(graph_json: &str, community_json: &str, document: bool) -> Result<Vec<u8>> {
    // Split only the trusted template; captured labels never become template syntax.
    let template = if document {
        include_str!("explorer.html")
            .replace("Source graph explorer","Document graph explorer")
            .replace("SOURCE GRAPH","DOCUMENT GRAPH")
            .replace("Search symbols","Search document nodes")
            .replace("Every matching symbol","Every matching document node")
            .replace("Canonical JSON","Document JSON")
            .replace("Language<select id=\"language\">","Format<select id=\"language\">")
            .replace("All languages","All formats")
            .replace("Explore the source graph","Explore the Document graph")
            .replace("Source candidates and heuristic communities are not compiler or runtime proof.","Document relationships are structural claims. Decoded text is not rendered layout, OCR or evaluated formulas.")
            .replace("Select a symbol to inspect captured declaration metadata, source anchors, relationships and exact evidence identities.","Select a document node to inspect decoded UTF-8 text, part XML origins and its opaque container binding.")
            .replace("Full source file bytes are not embedded. Source paths, ranges and hashes describe the immutable captured graph.","Decoded text is embedded. Part XML and opaque container bytes are not embedded; their distinct hashes and exact enclosing-element anchors are recorded.")
    } else {
        include_str!("explorer.html").to_owned()
    };
    let (before_graph, remaining) = template
        .split_once("__YOKE_GRAPH_DATA__")
        .ok_or_else(|| anyhow::anyhow!("Missing graph template slot"))?;
    let (before_communities, after) = remaining
        .split_once("__YOKE_COMMUNITY_DATA__")
        .ok_or_else(|| anyhow::anyhow!("Missing community template slot"))?;
    Ok([
        before_graph,
        graph_json,
        before_communities,
        community_json,
        after,
    ]
    .concat()
    .into_bytes())
}
/// Only a source-checked document graph can select this presentation. It is
/// separate from canonical SourceSnapshot/Graph JSON and never fabricates one.
pub fn serialize_document_explorer(
    graph: &crate::document_graph::CheckedDocumentGraph,
    deadline: &Deadline,
) -> Result<Vec<u8>> {
    struct Bounded {
        bytes: Vec<u8>,
        limit: usize,
    }
    impl Write for Bounded {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            if bytes.len() > self.limit.saturating_sub(self.bytes.len()) {
                return Err(std::io::Error::other(
                    "Document explorer byte capacity exceeded",
                ));
            }
            self.bytes.extend_from_slice(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    deadline.check()?;
    let payload = crate::document_graph::explorer_payload(graph, deadline)?;
    let mut serialized = Bounded {
        bytes: Vec::new(),
        limit: 64_000_000,
    };
    serde_json::to_writer(&mut serialized, &payload)?;
    let serialized = String::from_utf8(serialized.bytes)?;
    let mut escaped = Bounded {
        bytes: Vec::new(),
        limit: 120_000_000,
    };
    for (index, character) in serialized.chars().enumerate() {
        if index % 4096 == 0 {
            deadline.check()?;
        }
        let escape = match character {
            '&' => Some("\\u0026"),
            '<' => Some("\\u003c"),
            '>' => Some("\\u003e"),
            '\u{2028}' => Some("\\u2028"),
            '\u{2029}' => Some("\\u2029"),
            _ => None,
        };
        if let Some(escape) = escape {
            escaped.write_all(escape.as_bytes())?;
        } else {
            let mut bytes = [0; 4];
            escaped.write_all(character.encode_utf8(&mut bytes).as_bytes())?;
        }
    }
    let escaped = String::from_utf8(escaped.bytes)?;
    let output = html_parts(&escaped, "null", true)?;
    deadline.check()?;
    Ok(output)
}
/// Compute a checked source-bound community overlay without changing the graph.
pub fn serialize_explorer(
    store: &Store,
    snapshot: &Snapshot,
    graph: &Graph,
    deadline: &Deadline,
) -> Result<Vec<u8>> {
    let report =
        crate::graph_extensions::analyze(store, snapshot, graph, &Default::default(), deadline)?;
    let metadata = serde_json::json!({"snapshotId":snapshot.id,"graphHash":crate::model::identity(graph),"reportArtifactId":report["artifactId"],"producer":report["producer"],"heuristic":true,"communities":report["communities"],"limitations":report["limitations"]});
    deadline.check()?;
    html(graph, &metadata)
}
/// Create an offline explorer only after canonical source and graph validation.
pub fn write_explorer(
    store: &Store,
    snapshot: &Snapshot,
    graph: &Graph,
    path: &Path,
    deadline: &Deadline,
) -> Result<()> {
    let bytes = serialize_explorer(store, snapshot, graph, deadline)?;
    write_bytes(&bytes, path, Some(deadline))
}

/// Explicit source-bearing offline export. The old metadata-only HTML is unchanged.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SourceExplorerOptions {
    pub max_source_bytes: usize,
    pub max_output_bytes: usize,
}
impl Default for SourceExplorerOptions {
    fn default() -> Self {
        Self {
            max_source_bytes: 16_000_000,
            max_output_bytes: 64_000_000,
        }
    }
}
pub fn serialize_source_explorer(
    store: &Store,
    snapshot: &Snapshot,
    graph: &Graph,
    options: &SourceExplorerOptions,
    deadline: &Deadline,
) -> Result<Vec<u8>> {
    deadline.check()?;
    ensure!(
        options.max_source_bytes <= 32_000_000
            && (1024..=128_000_000).contains(&options.max_output_bytes),
        "Invalid source explorer byte limits"
    );
    // Admit the complete source set before opening any blob. No partial export.
    let mut total = 0usize;
    for file in &snapshot.files {
        deadline.check()?;
        total = total
            .checked_add(file.size)
            .ok_or_else(|| anyhow::anyhow!("Source explorer size overflow"))?;
        ensure!(
            total <= options.max_source_bytes,
            "Source explorer source byte budget exceeded"
        );
    }
    let sources = crate::graph_extensions::sources(store, snapshot, graph, deadline)?;
    #[derive(serde::Serialize)]
    struct SourceEntry<'a> {
        binding: &'a crate::model::SourceFile,
        text: &'a str,
    }
    #[derive(serde::Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Payload<'a> {
        schema: &'static str,
        producer: &'static str,
        workspace_id: &'a str,
        snapshot_id: &'a str,
        graph_hash: String,
        files: Vec<SourceEntry<'a>>,
        omissions: &'a [crate::model::Omission],
        coverage: &'a crate::model::Coverage,
        source_bytes: usize,
        freshness: &'static str,
    }
    let files = snapshot
        .files
        .iter()
        .map(|file| SourceEntry {
            binding: file,
            text: &sources[&file.path],
        })
        .collect();
    let payload = Payload {
        schema: "yoke-source-explorer-v1",
        producer: "yoke-source-explorer-1",
        workspace_id: &snapshot.workspace_id,
        snapshot_id: &snapshot.id,
        graph_hash: crate::model::identity(graph),
        files,
        omissions: &snapshot.omissions,
        coverage: &graph.coverage,
        source_bytes: total,
        freshness: "immutable_capture_not_current_disk",
    };
    struct Bounded<'a> {
        bytes: Vec<u8>,
        limit: usize,
        deadline: &'a Deadline,
    }
    impl Write for Bounded<'_> {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.deadline.check().map_err(std::io::Error::other)?;
            if bytes.len() > self.limit.saturating_sub(self.bytes.len()) {
                return Err(std::io::Error::other(
                    "Source explorer output byte budget exceeded",
                ));
            }
            self.bytes.extend_from_slice(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut serialized = Bounded {
        bytes: Vec::new(),
        limit: options.max_output_bytes,
        deadline,
    };
    serde_json::to_writer(&mut serialized, &payload)?;
    let mut escaped = Bounded {
        bytes: Vec::new(),
        limit: options.max_output_bytes,
        deadline,
    };
    for character in std::str::from_utf8(&serialized.bytes)?.chars() {
        match character {
            '&' => escaped.write_all(b"\\u0026")?,
            '<' => escaped.write_all(b"\\u003c")?,
            '>' => escaped.write_all(b"\\u003e")?,
            '\u{2028}' => escaped.write_all(b"\\u2028")?,
            '\u{2029}' => escaped.write_all(b"\\u2029")?,
            _ => {
                let mut bytes = [0; 4];
                escaped.write_all(character.encode_utf8(&mut bytes).as_bytes())?;
            }
        }
    }
    let base = serialize_explorer(store, snapshot, graph, deadline)?;
    let base = std::str::from_utf8(&base)?;
    // Captured values were JSON-escaped; only the final trusted body terminator
    // is split. User text and template marker strings are never interpreted.
    let (before, after) = base
        .rsplit_once("</body>")
        .ok_or_else(|| anyhow::anyhow!("Missing explorer body terminator"))?;
    let mut output = Bounded {
        bytes: Vec::new(),
        limit: options.max_output_bytes,
        deadline,
    };
    for part in [
        before.as_bytes(),
        b"<script type=\"application/json\" id=\"sources\">",
        &escaped.bytes,
        b"</script>",
        include_bytes!("explorer_source.html"),
        b"</body>",
        after.as_bytes(),
    ] {
        output.write_all(part)?;
    }
    deadline.check()?;
    Ok(output.bytes)
}
pub fn write_source_explorer(
    store: &Store,
    snapshot: &Snapshot,
    graph: &Graph,
    path: &Path,
    options: &SourceExplorerOptions,
    deadline: &Deadline,
) -> Result<()> {
    let bytes = serialize_source_explorer(store, snapshot, graph, options, deadline)?;
    write_bytes(&bytes, path, Some(deadline))
}
pub fn write(graph: &Graph, format: &str, path: &Path) -> Result<()> {
    write_bytes(&serialize(graph, format)?, path, None)
}
fn check_destination(path: &Path) -> Result<()> {
    let absolute = if path.is_absolute() {
        path.to_owned()
    } else {
        std::env::current_dir()?.join(path)
    };
    let mut current = std::path::PathBuf::new();
    for component in absolute.components() {
        current.push(component.as_os_str());
        match fs::symlink_metadata(&current) {
            Ok(metadata) => ensure!(!is_link(&metadata), "Export link/reparse component refused"),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
    }
    Ok(())
}
fn write_bytes(bytes: &[u8], path: &Path, deadline: Option<&Deadline>) -> Result<()> {
    ensure!(bytes.len() <= 128_000_000, "Export byte budget exceeded");
    check_destination(path)?;
    if path.exists() {
        let mut file = fs::File::open(path)?;
        let metadata = file.metadata()?;
        ensure!(
            metadata.is_file() && !is_link(&metadata) && metadata.len() == bytes.len() as u64,
            "Export destination already contains different bytes; choose a new path"
        );
        let mut chunk = [0u8; 64 * 1024];
        let mut at = 0;
        loop {
            if let Some(deadline) = deadline {
                deadline.check()?;
            }
            let count = std::io::Read::read(&mut file, &mut chunk)?;
            if count == 0 {
                break;
            }
            ensure!(
                count <= bytes.len().saturating_sub(at) && bytes[at..at + count] == chunk[..count],
                "Export destination already contains different bytes; choose a new path"
            );
            at += count;
        }
        ensure!(
            at == bytes.len(),
            "Export destination changed during comparison"
        );
        return Ok(());
    }
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        fs::create_dir_all(parent)?;
    }
    check_destination(path)?;
    if let Some(deadline) = deadline {
        deadline.check()?;
    }
    let mut file = fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(path)?;
    for chunk in bytes.chunks(64 * 1024) {
        if let Some(deadline) = deadline {
            deadline.check()?;
        }
        file.write_all(chunk)?;
    }
    file.sync_all()?;
    Ok(())
}
fn xml(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}
fn xml_value(value: &str) -> Result<()> {
    ensure!(
        value.chars().all(|c| matches!(c, '\t' | '\n' | '\r')
            || ('\u{20}'..='\u{d7ff}').contains(&c)
            || ('\u{e000}'..='\u{fffd}').contains(&c)
            || ('\u{10000}'..='\u{10ffff}').contains(&c)),
        "GraphML cannot represent an XML 1.0 character; choose JSON or HTML to preserve the original value"
    );
    Ok(())
}
fn graphml(graph: &Graph) -> Result<String> {
    for entity in &graph.entities {
        for value in [&entity.id, &entity.qualified_name, &entity.kind] {
            xml_value(value)?;
        }
    }
    for edge in &graph.observations {
        for value in [&edge.id, &edge.source, &edge.relation, &edge.resolution] {
            xml_value(value)?;
        }
        if let Some(target) = &edge.target {
            xml_value(target)?;
        } else {
            xml_value(&edge.target_label)?;
        }
        for evidence in &edge.evidence_ids {
            xml_value(evidence)?;
        }
    }

    let mut result = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?><graphml xmlns=\"http://graphml.graphdrawing.org/xmlns\"><key id=\"label\" for=\"all\" attr.name=\"label\" attr.type=\"string\"/><key id=\"kind\" for=\"node\" attr.name=\"kind\" attr.type=\"string\"/><key id=\"resolution\" for=\"edge\" attr.name=\"resolution\" attr.type=\"string\"/><key id=\"evidence\" for=\"edge\" attr.name=\"evidence\" attr.type=\"string\"/><graph edgedefault=\"directed\">",
    );
    for entity in &graph.entities {
        result.push_str(&format!(
            "<node id=\"{}\"><data key=\"label\">{}</data><data key=\"kind\">{}</data></node>",
            xml(&entity.id),
            xml(&entity.qualified_name),
            xml(&entity.kind)
        ));
    }
    for edge in &graph.observations {
        let target = edge
            .target
            .clone()
            .unwrap_or_else(|| format!("unresolved_{}", edge.id));
        if edge.target.is_none() {
            result.push_str(&format!("<node id=\"{}\"><data key=\"label\">{}</data><data key=\"kind\">unresolved-placeholder</data></node>",xml(&target),xml(&edge.target_label)));
        }
        result.push_str(&format!("<edge id=\"{}\" source=\"{}\" target=\"{}\"><data key=\"label\">{}</data><data key=\"resolution\">{}</data><data key=\"evidence\">{}</data></edge>",xml(&edge.id),xml(&edge.source),xml(&target),xml(&edge.relation),xml(&edge.resolution),xml(&edge.evidence_ids.join(","))));
    }
    result.push_str("</graph></graphml>");
    Ok(result)
}
