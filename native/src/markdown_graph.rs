//! Authored Markdown relationships are candidates, never executable instructions.
use crate::{
    capture::Deadline,
    manifest_graph::{Builder, DerivedGraph},
    model::{Entity, Graph, Snapshot, SourceFile},
    store::Store,
};
use anyhow::{Result, ensure};
use pulldown_cmark::{Event, HeadingLevel, LinkType, Options, Parser, Tag, TagEnd};
use serde_json::json;
use std::{collections::BTreeMap, ops::Range};
use unicode_normalization::UnicodeNormalization;

pub(crate) const PRODUCER: &str = "yoke-markdown-graph-3";

fn decoded(value: &str) -> Option<String> {
    let raw = value.as_bytes();
    let mut bytes = Vec::with_capacity(raw.len());
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

fn relative(source: &str, target: &str) -> Option<String> {
    if target.contains([':', '\\', '\0'])
        || target.chars().any(char::is_control)
        || target.starts_with('/')
        || target.is_empty()
        || crate::paths::protected(target)
    {
        return None;
    }
    let mut parts: Vec<_> = source
        .rsplit_once('/')
        .map(|(parent, _)| parent.split('/').collect())
        .unwrap_or_default();
    for part in target.split('/') {
        match part {
            "" | "." => (),
            ".." => {
                parts.pop()?;
            }
            _ => parts.push(part),
        }
    }
    crate::paths::normalized(&parts.join("/")).ok()
}

fn basename(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

fn extension(path: &str) -> &str {
    basename(path).rsplit_once('.').map_or("", |(_, ext)| ext)
}

fn is_markdown(file: &SourceFile) -> bool {
    file.language == "markdown"
        || matches!(
            extension(&file.path).to_ascii_lowercase().as_str(),
            "md" | "mdx" | "markdown" | "qmd" | "skill"
        )
}

fn matches_suffix(path: &str, suffix: &str) -> bool {
    path == suffix
        || path
            .strip_suffix(suffix)
            .is_some_and(|prefix| prefix.ends_with('/'))
}

fn explicit_relative(path: &str) -> bool {
    path.split('/').any(|part| matches!(part, "." | ".."))
}

struct Documents<'a> {
    ids: BTreeMap<&'a str, String>,
    by_basename: BTreeMap<String, Vec<(&'a str, String)>>,
}

impl Documents<'_> {
    fn resolve(
        &self,
        source: &str,
        target: &str,
        wiki: bool,
        deadline: &Deadline,
    ) -> Result<Option<String>> {
        let literal = target.split(['#', '?']).next().unwrap_or("").trim();
        if !wiki
            && let Some(candidate) = relative(source, literal)
            && self.ids.contains_key(candidate.as_str())
        {
            return Ok(Some(candidate));
        }
        let converted;
        let requested = if wiki {
            literal
        } else {
            let Some(value) = decoded(literal) else {
                return Ok(None);
            };
            converted = value;
            &converted
        };
        let Some(local) = relative(source, requested) else {
            return Ok(None);
        };
        let completed = format!("{requested}.md");
        for candidate in [requested, completed.as_str()] {
            let Some(local_candidate) = relative(source, candidate) else {
                continue;
            };
            if self.ids.contains_key(local_candidate.as_str()) {
                return Ok(Some(local_candidate));
            }
            if !wiki {
                if !extension(requested).is_empty() {
                    break;
                }
                continue;
            }
            let basename: String = basename(candidate).nfc().collect();
            let expected: String = local_candidate.nfc().collect();
            let fallback = crate::paths::normalized(candidate)
                .ok()
                .filter(|_| !explicit_relative(candidate));
            let fallback = fallback.map(|path| path.nfc().collect::<String>());
            let mut winner = None;
            for (original, normalized) in self.by_basename.get(&basename).into_iter().flatten() {
                deadline.check()?;
                if *normalized == expected {
                    return Ok(Some((*original).to_owned()));
                }
                // Entries are sorted by depth and then by their normalized and raw paths.
                if winner.is_none()
                    && fallback
                        .as_deref()
                        .is_some_and(|suffix| matches_suffix(normalized, suffix))
                {
                    winner = Some(*original);
                }
            }
            if let Some(winner) = winner {
                return Ok(Some(winner.to_owned()));
            }
        }
        Ok(Some(if wiki && extension(requested).is_empty() {
            format!("{local}.md")
        } else {
            local
        }))
    }
}

fn emit_link(
    builder: &mut Builder<'_>,
    documents: &Documents<'_>,
    file: &SourceFile,
    text: &str,
    target: &str,
    wiki: bool,
    span: Range<usize>,
) -> Result<()> {
    let anchor = builder.anchor(file, text, span.start, span.end, "exact_authored_link")?;
    let root = &documents.ids[file.path.as_str()];
    if let Some(path) = documents.resolve(&file.path, target, wiki, builder.deadline)? {
        let target_id = documents.ids.get(path.as_str()).map(String::as_str);
        builder.edge(
            root,
            target_id,
            &path,
            "links_document",
            if target_id.is_some() {
                "authored_path"
            } else {
                "unresolved"
            },
            &anchor,
            json!({"fragment":target.split_once('#').map(|(_,suffix)|suffix)}),
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
    Ok(())
}

fn emit_frontmatter(
    builder: &mut Builder<'_>,
    documents: &Documents<'_>,
    file: &SourceFile,
    text: &str,
    span: Range<usize>,
) -> Result<()> {
    let mut offset = span.start;
    for (index, line) in text[span].split_inclusive('\n').enumerate() {
        builder.deadline.check()?;
        let start = offset;
        offset += line.len();
        if index == 0 {
            continue;
        }
        let trimmed = line.trim_start();
        if trimmed.starts_with('#') {
            continue;
        }
        if trimmed.len() == line.len()
            && let Some((key, _)) = line.split_once(':')
            && !key.trim().is_empty()
        {
            builder.node(file, text, "frontmatter_key", key.trim(), "markdown")?;
        }
        if !trimmed.contains('[') {
            continue;
        }
        let start = start + line.len() - trimmed.len();
        for (event, link_span) in
            Parser::new_ext(trimmed, Options::ENABLE_WIKILINKS).into_offset_iter()
        {
            builder.deadline.check()?;
            if let Event::Start(Tag::Link {
                dest_url,
                link_type,
                ..
            }) = event
            {
                emit_link(
                    builder,
                    documents,
                    file,
                    text,
                    &dest_url,
                    matches!(link_type, LinkType::WikiLink { .. }),
                    (start + link_span.start)..(start + link_span.end),
                )?;
            }
        }
    }
    Ok(())
}

fn headings(
    builder: &mut Builder<'_>,
    file: &SourceFile,
    text: &str,
    root: &str,
    options: Options,
) -> Result<Vec<(usize, String)>> {
    let mut section = None::<(HeadingLevel, Range<usize>, String)>;
    let mut parents = Vec::<(HeadingLevel, String)>::new();
    let mut result = Vec::new();
    for (event, span) in Parser::new_ext(text, options).into_offset_iter() {
        builder.deadline.check()?;
        match event {
            Event::Start(Tag::Heading { level, .. }) => {
                section = Some((level, span, String::new()))
            }
            Event::Text(label) | Event::Code(label) => {
                if let Some((_, _, name)) = &mut section {
                    ensure!(
                        name.len() + label.len() <= 4096,
                        "Derived node capacity exceeded"
                    );
                    name.push_str(&label);
                }
            }
            Event::SoftBreak | Event::HardBreak => {
                if let Some((_, _, name)) = &mut section {
                    name.push(' ');
                }
            }
            Event::End(TagEnd::Heading(_)) => {
                let (level, span, name) = section.take().expect("CommonMark pairs heading tags");
                let name = name.trim();
                let name = if name.is_empty() {
                    "(untitled heading)"
                } else {
                    name
                };
                let end = span.start + text[span.clone()].trim_end_matches(['\r', '\n']).len();
                let anchor =
                    builder.anchor(file, text, span.start, end, "exact_authored_heading")?;
                let id = builder.node_at_anchor(file, "heading", name, "markdown", &anchor)?;
                while parents.last().is_some_and(|(parent, _)| *parent >= level) {
                    parents.pop();
                }
                let parent = parents.last().map_or(root, |(_, id)| id.as_str());
                builder.edge(
                    parent,
                    Some(&id),
                    name,
                    "contains",
                    "authored_heading",
                    &anchor,
                    json!({}),
                )?;
                parents.push((level, id.clone()));
                result.push((span.start, id));
            }
            _ => (),
        }
    }
    Ok(result)
}

fn identifier(value: &str) -> bool {
    let mut chars = value.chars();
    chars.next().is_some_and(|c| c == '_' || c.is_alphabetic())
        && chars.all(|c| c == '_' || c.is_alphanumeric())
}

fn code_entity(node: &Entity) -> bool {
    node.kind != "file"
        && !matches!(
            node.language.as_str(),
            "markdown" | "text" | "config" | "json" | "toml" | "yaml"
        )
}

fn callable_or_type(node: &Entity) -> bool {
    matches!(
        node.kind.as_str(),
        "function"
            | "method"
            | "class"
            | "struct"
            | "enum"
            | "interface"
            | "trait"
            | "type"
            | "union"
    )
}

fn symbol_chain(node: &Entity) -> &str {
    node.qualified_name
        .strip_prefix(&node.path)
        .and_then(|name| name.strip_prefix('/'))
        .unwrap_or(&node.qualified_name)
}

struct Symbols<'a> {
    by_name: BTreeMap<&'a str, Vec<&'a Entity>>,
    files: BTreeMap<&'a str, Vec<&'a str>>,
}

impl<'a> Symbols<'a> {
    fn cited_file(
        &self,
        source: &str,
        target: &str,
        deadline: &Deadline,
    ) -> Result<Option<&'a str>> {
        let Some(local) = relative(source, target) else {
            return Ok(None);
        };
        let Some(options) = self.files.get(basename(&local)) else {
            return Ok(None);
        };
        if let Some(path) = options.iter().find(|path| **path == local) {
            return Ok(Some(path));
        }
        if explicit_relative(target) {
            return Ok(None);
        }
        if let Some(path) = options.iter().find(|path| **path == target) {
            return Ok(Some(path));
        }
        let mut matched = None;
        for path in options.iter().filter(|path| matches_suffix(path, target)) {
            deadline.check()?;
            if matched.replace(*path).is_some() {
                return Ok(None);
            }
        }
        Ok(matched)
    }

    fn candidates(&self, source: &str, label: &str, deadline: &Deadline) -> Result<Vec<&'a str>> {
        let name = label.strip_suffix("()").unwrap_or(label);
        let (file, name, separator) = if let Some((file, name)) = name.split_once("::") {
            let Some(path) = self.cited_file(source, file, deadline)? else {
                return Ok(vec![]);
            };
            (Some(path), name, "::")
        } else {
            (None, name, ".")
        };
        // Earlier Markdown citations also accepted the native slash-qualified spelling.
        let name = file
            .and_then(|path| name.strip_prefix(path)?.strip_prefix('/'))
            .unwrap_or(name);
        let separator = if file.is_some() && name.contains('/') {
            "/"
        } else {
            separator
        };
        let parts: Vec<_> = name.split(separator).collect();
        if !parts.iter().all(|part| identifier(part)) {
            return Ok(vec![]);
        }
        let (leaf, qualifiers) = parts.split_last().expect("split always emits a part");
        let chain = file.map(|_| parts.join("/"));
        let mut found = Vec::new();
        for node in self.by_name.get(leaf).into_iter().flatten() {
            deadline.check()?;
            let owners = symbol_chain(node);
            let matched = if let Some(file) = file {
                node.path == file
                    && (qualifiers.is_empty() || matches_suffix(owners, chain.as_deref().unwrap()))
            } else {
                callable_or_type(node)
                    && qualifiers.iter().all(|qualifier| {
                        owners.rsplit_once('/').is_some_and(|(parents, _)| {
                            parents.split('/').any(|part| part == *qualifier)
                        }) || node.path.split('/').any(|part| {
                            part == *qualifier
                                || part
                                    .rsplit_once('.')
                                    .is_some_and(|(stem, _)| stem == *qualifier)
                        })
                    })
            };
            if matched {
                found.push(node.id.as_str());
            }
        }
        Ok(found)
    }
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
    let mut builder = Builder::new(store, snapshot, graph, PRODUCER, deadline)?;
    let mut documents = Documents {
        ids: BTreeMap::new(),
        by_basename: BTreeMap::new(),
    };
    let mut sources = Vec::new();
    let mut symbols = Symbols {
        by_name: BTreeMap::new(),
        files: BTreeMap::new(),
    };
    for file in &snapshot.files {
        deadline.check()?;
        let markdown = is_markdown(file);
        if markdown
            || matches!(
                extension(&file.path).to_ascii_lowercase().as_str(),
                "rst" | "txt"
            )
        {
            let (text, _) = crate::query::source(store, snapshot, &file.path)?;
            documents.ids.insert(
                &file.path,
                builder.node(file, &text, "document", &file.path, "markdown")?,
            );
            let normalized: String = file.path.nfc().collect();
            documents
                .by_basename
                .entry(basename(&normalized).into())
                .or_default()
                .push((&file.path, normalized));
            if markdown {
                sources.push((file, text));
            }
        } else {
            symbols
                .files
                .entry(basename(&file.path))
                .or_default()
                .push(&file.path);
        }
    }
    for choices in documents.by_basename.values_mut() {
        deadline.check()?;
        choices.sort_by_cached_key(|(path, normalized)| {
            (
                normalized.bytes().filter(|b| *b == b'/').count(),
                normalized.clone(),
                *path,
            )
        });
    }
    for node in graph.entities.iter().filter(|node| code_entity(node)) {
        deadline.check()?;
        symbols.by_name.entry(&node.name).or_default().push(node);
    }

    let options = Options::ENABLE_WIKILINKS
        | Options::ENABLE_YAML_STYLE_METADATA_BLOCKS
        | Options::ENABLE_PLUSES_DELIMITED_METADATA_BLOCKS;
    for (file, text) in &sources {
        let root = &documents.ids[file.path.as_str()];
        let sections = headings(&mut builder, file, text, root, options)?;
        let mut sections = sections.iter().peekable();
        let mut current = root;
        let parser = Parser::new_ext(text, options);
        let mut definitions = parser
            .reference_definitions()
            .iter()
            .map(|(_, definition)| definition)
            .collect::<Vec<_>>();
        definitions.sort_by_key(|definition| definition.span.start);
        for definition in definitions {
            emit_link(
                &mut builder,
                &documents,
                file,
                text,
                &definition.dest,
                false,
                definition.span.clone(),
            )?;
        }
        for (event, span) in parser.into_offset_iter() {
            deadline.check()?;
            while sections
                .peek()
                .is_some_and(|(start, _)| *start <= span.start)
            {
                current = &sections.next().unwrap().1;
            }
            match event {
                Event::Start(Tag::MetadataBlock(_)) => {
                    emit_frontmatter(&mut builder, &documents, file, text, span)?;
                }
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
                    emit_link(
                        &mut builder,
                        &documents,
                        file,
                        text,
                        &dest_url,
                        matches!(link_type, LinkType::WikiLink { .. }),
                        span,
                    )?;
                }
                Event::Code(label) => {
                    let label = label.trim();
                    let found = symbols.candidates(&file.path, label, deadline)?;
                    if found.is_empty() && !label.contains("::") {
                        continue;
                    }
                    let anchor = builder.anchor(
                        file,
                        text,
                        span.start,
                        span.end,
                        "exact_authored_citation",
                    )?;
                    let (target, resolution) = match found.as_slice() {
                        [one] => (Some(*one), "unique_name_candidate"),
                        [] => (None, "unresolved"),
                        _ => (None, "ambiguous_name"),
                    };
                    builder.edge(
                        current,
                        target,
                        label,
                        "cites_symbol",
                        resolution,
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
