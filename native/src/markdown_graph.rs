//! Authored Markdown relationships are candidates, never executable instructions.
use crate::{
    capture::Deadline,
    manifest_graph::{Builder, DerivedGraph},
    model::{Graph, Snapshot},
    store::Store,
};
use anyhow::Result;
use serde_json::json;
use std::collections::BTreeMap;
fn decoded(value: &str) -> Option<String> {
    let raw = value.as_bytes();
    let mut bytes = Vec::new();
    let mut at = 0;
    while at < raw.len() {
        if raw[at] == b'%' {
            let hi = (*raw.get(at + 1)? as char).to_digit(16)?;
            let lo = (*raw.get(at + 2)? as char).to_digit(16)?;
            bytes.push((hi * 16 + lo) as u8);
            at += 3;
        } else {
            bytes.push(raw[at]);
            at += 1;
        }
    }
    String::from_utf8(bytes).ok()
}
fn local_path(source: &str, target: &str, wiki: bool) -> Option<String> {
    let clean = decoded(target.split('#').next()?.split('?').next()?)?;
    if clean.contains([':', '\\', '\0']) || clean.starts_with('/') || clean.is_empty() {
        return None;
    }
    let mut parts: Vec<_> = source
        .rsplit_once('/')
        .map(|(p, _)| p.split('/').collect())
        .unwrap_or_default();
    for part in clean.split('/') {
        match part {
            "" | "." => (),
            ".." => {
                parts.pop()?;
            }
            _ => parts.push(part),
        }
    }
    let mut path = parts.join("/");
    if wiki && !path.ends_with(".md") {
        path.push_str(".md");
    }
    crate::paths::normalized(&path).ok()
}
pub fn extract(
    store: &Store,
    snapshot: &Snapshot,
    graph: &Graph,
    deadline: &Deadline,
) -> Result<DerivedGraph> {
    prepare(store, snapshot, graph, deadline)?.publish(deadline)
}
/// Derive exact immutable output bytes without writing CAS or database records.
pub fn prepare<'a>(
    store: &'a Store,
    snapshot: &'a Snapshot,
    graph: &Graph,
    deadline: &'a Deadline,
) -> Result<crate::manifest_graph::PreparedDerivedGraph<'a>> {
    use pulldown_cmark::{Event, LinkType, Options, Parser, Tag};
    let mut builder = Builder::new(store, snapshot, graph, "yoke-markdown-graph-2", deadline)?;
    let mut docs = BTreeMap::new();
    let mut sources = BTreeMap::new();
    let mut files = BTreeMap::new();
    for file in &snapshot.files {
        files.insert(file.path.clone(), file);
        if file.path.ends_with(".md") || file.path.ends_with(".mdx") {
            let (text, _) = crate::query::source(store, snapshot, &file.path)?;
            docs.insert(
                file.path.clone(),
                builder.node(file, &text, "document", &file.path, "markdown")?,
            );
            sources.insert(file.path.clone(), text);
        }
    }
    let mut symbols = BTreeMap::<String, Vec<String>>::new();
    for node in &graph.entities {
        if node.kind != "file" {
            for label in [
                format!("{}::{}", node.path, node.qualified_name),
                format!("{}::{}", node.path, node.name),
                node.qualified_name.clone(),
            ] {
                let ids = symbols.entry(label).or_default();
                if !ids.contains(&node.id) {
                    ids.push(node.id.clone());
                }
            }
        }
    }
    let options = Options::ENABLE_WIKILINKS
        | Options::ENABLE_YAML_STYLE_METADATA_BLOCKS
        | Options::ENABLE_PLUSES_DELIMITED_METADATA_BLOCKS;
    for (path, text) in &sources {
        let file = files[path];
        let root = &docs[path];
        if text.starts_with("---\n") || text.starts_with("---\r\n") {
            for line in text.lines().skip(1) {
                deadline.check()?;
                if line.trim() == "---" {
                    break;
                }
                if let Some((key, _)) = line.split_once(':') {
                    let key = key.trim();
                    if !key.is_empty() {
                        builder.node(file, text, "frontmatter_key", key, "markdown")?;
                    }
                }
            }
        }
        for (event, span) in Parser::new_ext(text, options).into_offset_iter() {
            deadline.check()?;
            match event {
                Event::Start(Tag::Link {
                    dest_url,
                    link_type,
                    ..
                })
                | Event::Start(Tag::Image {
                    dest_url,
                    link_type,
                    ..
                }) => {
                    let target = dest_url.as_ref();
                    let anchor =
                        builder.anchor(file, text, span.start, span.end, "exact_authored_link")?;
                    let wiki = matches!(link_type, LinkType::WikiLink { .. });
                    if let Some(target_path) = local_path(path, target, wiki) {
                        let target_id = docs.get(&target_path);
                        builder.edge(
                            root,
                            target_id.map(String::as_str),
                            &target_path,
                            "links_document",
                            if target_id.is_some() {
                                "authored_path"
                            } else {
                                "unresolved"
                            },
                            &anchor,
                            json!({"fragment":target.split_once('#').map(|(_,s)|s)}),
                        )?;
                    } else {
                        builder.edge(
                            root,
                            None,
                            target,
                            "links_external",
                            "authored_link_not_fetched",
                            &anchor,
                            json!({}),
                        )?;
                    }
                }
                Event::Code(label) if label.contains("::") => {
                    let found = symbols
                        .get(label.as_ref())
                        .map(Vec::as_slice)
                        .unwrap_or(&[]);
                    let anchor = builder.anchor(
                        file,
                        text,
                        span.start,
                        span.end,
                        "exact_authored_citation",
                    )?;
                    builder.edge(
                        root,
                        if found.len() == 1 {
                            Some(found[0].as_str())
                        } else {
                            None
                        },
                        &label,
                        "cites_symbol",
                        if found.len() == 1 {
                            "unique_name_candidate"
                        } else if found.len() > 1 {
                            "ambiguous_name"
                        } else {
                            "unresolved"
                        },
                        &anchor,
                        json!({"candidateEntityIds":found,"targetNamespace":"canonical_graph"}),
                    )?;
                }
                _ => (),
            }
        }
    }
    builder.finish()
}
