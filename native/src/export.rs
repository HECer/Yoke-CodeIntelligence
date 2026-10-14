use crate::{model::Graph, paths::is_link};
use anyhow::{Result, bail, ensure};
use std::{fs, io::Write, path::Path};

pub fn serialize(graph: &Graph, format: &str) -> Result<Vec<u8>> {
    match format {
        "json" => Ok(serde_json::to_vec_pretty(graph)?),
        "graphml" => Ok(graphml(graph).into_bytes()),
        "html" => {
            let data = serde_json::to_string(graph)?
                .replace('&', "\\u0026")
                .replace('<', "\\u003c")
                .replace('>', "\\u003e")
                .replace('\u{2028}', "\\u2028")
                .replace('\u{2029}', "\\u2029");
            Ok(include_str!("explorer.html")
                .replace("__YOKE_GRAPH_DATA__", &data)
                .into_bytes())
        }
        _ => bail!("Export format must be json, graphml or html"),
    }
}
pub fn write(graph: &Graph, format: &str, path: &Path) -> Result<()> {
    let bytes = serialize(graph, format)?;
    ensure!(bytes.len() <= 128_000_000, "Export byte budget exceeded");
    if path.exists() {
        ensure!(
            !is_link(&fs::symlink_metadata(path)?),
            "Export link refused"
        );
        ensure!(
            fs::read(path)? == bytes,
            "Export destination already contains different bytes; choose a new path"
        );
        return Ok(());
    }
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent)?;
    }
    let mut file = fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(path)?;
    file.write_all(&bytes)?;
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
fn graphml(graph: &Graph) -> String {
    let mut result = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?><graphml xmlns=\"http://graphml.graphdrawing.org/xmlns\"><key id=\"label\" for=\"all\" attr.name=\"label\" attr.type=\"string\"/><key id=\"kind\" for=\"node\" attr.name=\"kind\" attr.type=\"string\"/><key id=\"resolution\" for=\"edge\" attr.name=\"resolution\" attr.type=\"string\"/><key id=\"evidence\" for=\"edge\" attr.name=\"evidence\" attr.type=\"string\"/><graph edgedefault=\"directed\">",
    );
    for entity in &graph.entities {
        result.push_str(&format!(
            "<node id=\"{}\"><data key=\"label\">{}</data><data key=\"kind\">{}</data></node>",
            entity.id,
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
            result.push_str(&format!("<node id=\"{target}\"><data key=\"label\">{}</data><data key=\"kind\">unresolved-placeholder</data></node>",xml(&edge.target_label)));
        }
        result.push_str(&format!("<edge id=\"{}\" source=\"{}\" target=\"{}\"><data key=\"label\">{}</data><data key=\"resolution\">{}</data><data key=\"evidence\">{}</data></edge>",edge.id,edge.source,target,xml(&edge.relation),xml(&edge.resolution),xml(&edge.evidence_ids.join(","))));
    }
    result.push_str("</graph></graphml>");
    result
}
