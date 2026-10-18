use crate::{
    capture::Deadline,
    model::{
        ANALYZER, Coverage, Diagnostic, Entity, Evidence, Extraction, Graph, Observation, RawEdge,
        Snapshot, SourceFile, identity,
    },
};
use anyhow::{Result, ensure};
use regex::Regex;
use std::collections::{BTreeMap, HashMap};
use tree_sitter::{Language, Node, ParseOptions, Parser};

pub use crate::languages::LANGUAGES;

#[path = "analysis_declarations.rs"]
pub(crate) mod declarations;

/// An explicit extraction policy; capture/classification proofs remain separate.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AnalyzerEpoch {
    V8,
    V9,
}
impl AnalyzerEpoch {
    pub fn parse(producer: &str) -> Result<Self> {
        match producer {
            "yoke-tree-sitter-8" => Ok(Self::V8),
            "yoke-tree-sitter-9" => Ok(Self::V9),
            _ => anyhow::bail!("Unknown extraction analyzer epoch"),
        }
    }
    pub fn active() -> Result<Self> {
        Self::parse(ANALYZER)
    }
    pub fn producer(self) -> &'static str {
        match self {
            Self::V8 => "yoke-tree-sitter-8",
            Self::V9 => "yoke-tree-sitter-9",
        }
    }
    pub fn cache_key(self, file: &SourceFile) -> String {
        identity(&(self.producer(), &file.language, &file.hash, &file.path))
    }
    pub fn derived_snapshot_id(self, base: &Snapshot, files: &[SourceFile]) -> String {
        match self {
            Self::V8 => identity(&(&base.id, "derived", files)),
            Self::V9 => identity(&(&base.id, "derived", self.producer(), files)),
        }
    }
}

pub fn grammar(language: &str) -> Option<Language> {
    crate::languages::grammar(language)
}
#[allow(clippy::too_many_arguments)]
fn entity(
    epoch: AnalyzerEpoch,
    file: &SourceFile,
    name: &str,
    kind: &str,
    start: usize,
    end: usize,
    text: &str,
    line_starts: &[usize],
    parent: Option<&Entity>,
    name_range: Option<(usize, usize)>,
) -> Entity {
    let key = format!("{}:{kind}:{start}", file.path);
    let signature = if kind == "file" {
        format!("{} [{}]", file.path, file.language)
    } else {
        text[start..end]
            .lines()
            .next()
            .unwrap_or("")
            .chars()
            .take(240)
            .collect()
    };
    Entity {
        id: String::new(),
        key,
        snapshot_id: String::new(),
        path: file.path.clone(),
        name: name.into(),
        qualified_name: parent
            .map_or_else(|| name.into(), |p| format!("{}/{}", p.qualified_name, name)),
        kind: kind.into(),
        language: file.language.clone(),
        signature,
        start_byte: start,
        end_byte: end,
        start_line: line_starts.partition_point(|offset| *offset <= start),
        end_line: line_starts.partition_point(|offset| *offset <= end.saturating_sub(1)),
        name_start_byte: name_range.map(|r| r.0),
        name_end_byte: name_range.map(|r| r.1),
        parent_key: parent.map(|p| p.key.clone()),
        producer: epoch.producer().into(),
    }
}
fn line(text: &str, offset: usize) -> usize {
    text.as_bytes()[..offset.min(text.len())]
        .iter()
        .filter(|b| **b == b'\n')
        .count()
        + 1
}
fn kind(node: Node<'_>) -> Option<&'static str> {
    match node.kind() {
        "function_item"
        | "local_function_statement"
        | "function_definition"
        | "function_declaration"
        | "function_expression"
        | "generator_function_declaration" => Some("function"),
        "method_definition"
        | "method_declaration"
        | "method_signature"
        | "abstract_method_signature"
        | "function_signature_item"
        | "method"
        | "singleton_method"
        | "constructor_declaration" => Some("method"),
        "class_definition" | "class_declaration" | "class" => Some("class"),
        "struct_item" | "struct_specifier" => Some("struct"),
        "enum_item" | "enum_declaration" | "enum_specifier" => Some("enum"),
        "trait_item" | "interface_declaration" => Some("interface"),
        "type_alias_declaration" | "type_item" | "type_spec" => Some("type"),
        "const_item" | "constant_declaration" => Some("constant"),
        "mod_item" | "module" | "namespace_definition" | "namespace_declaration" => Some("module"),
        "variable_declarator"
            if node
                .child_by_field_name("value")
                .is_some_and(|n| matches!(n.kind(), "arrow_function" | "function_expression")) =>
        {
            Some("function")
        }
        _ => None,
    }
}
fn name_node(node: Node<'_>) -> Option<Node<'_>> {
    if let Some(name) = node.child_by_field_name("name") {
        return name.is_named().then_some(name);
    }
    let mut declarator = node.child_by_field_name("declarator")?;
    for _ in 0..8 {
        if matches!(
            declarator.kind(),
            "identifier" | "field_identifier" | "operator_name"
        ) {
            return Some(declarator);
        }
        declarator = declarator.child_by_field_name("declarator")?;
    }
    None
}
pub(crate) fn symbol<'a>(
    epoch: AnalyzerEpoch,
    node: Node<'a>,
    text: &str,
    language: &str,
) -> Option<(&'static str, Node<'a>)> {
    if language == "commonlisp" && crate::languages::commonlisp_literal_data(node, text) {
        return None;
    }
    if epoch == AnalyzerEpoch::V9 {
        if language == "verilog" && node.kind() == "task_declaration" {
            return crate::languages::verilog_task_symbol(node);
        }
        if language == "commonlisp" && crate::languages::lisp_local_function(node, text).is_some() {
            return Some(("function", node.named_child(0)?));
        }
    }
    if let Some(symbol) = crate::languages::symbol(node, language, text) {
        return Some(symbol);
    }
    if language == "rust" && node.kind() == "impl_item" {
        return Some(("implementation", node.child_by_field_name("type")?));
    }
    if language == "dart" && node.kind() == "function_declaration" {
        return Some((
            "function",
            node.child_by_field_name("signature")?
                .child_by_field_name("name")?,
        ));
    }
    if language == "clojure" && node.kind() == "list_lit" {
        let first = node.named_child(0)?.utf8_text(text.as_bytes()).ok()?;
        if [
            "defn",
            "defn-",
            "defmacro",
            "def",
            "defrecord",
            "deftype",
            "defprotocol",
            "ns",
        ]
        .contains(&first)
        {
            return Some((
                if first == "ns" {
                    "module"
                } else if first == "def" {
                    "constant"
                } else {
                    "function"
                },
                node.named_child(1)?,
            ));
        }
    }
    if language == "r"
        && node.kind() == "binary_operator"
        && node
            .child_by_field_name("rhs")
            .is_some_and(|n| n.kind() == "function_definition")
    {
        return Some(("function", node.child_by_field_name("lhs")?));
    }
    if language == "nix" && node.kind() == "binding" {
        let attrpath = node.child_by_field_name("attrpath")?;
        let name = attrpath.named_child(0)?;
        let kind = if node
            .child_by_field_name("expression")
            .is_some_and(|n| n.kind() == "function_expression")
        {
            "function"
        } else {
            "constant"
        };
        return Some((kind, name));
    }
    if language == "ocaml" && node.kind() == "value_definition" {
        let binding = node.named_child(0)?;
        return Some(("function", binding.child_by_field_name("pattern")?));
    }
    if language == "elixir" && node.kind() == "call" {
        let target = node
            .child_by_field_name("target")?
            .utf8_text(text.as_bytes())
            .ok()?;
        if ["def", "defp", "defmacro", "defmodule", "defprotocol"].contains(&target) {
            let mut cursor = node.walk();
            let arguments = node
                .named_children(&mut cursor)
                .find(|n| n.kind() == "arguments")?;
            let first = arguments.named_child(0)?;
            let name = first.child_by_field_name("target").unwrap_or(first);
            return Some((
                if target == "defmodule" || target == "defprotocol" {
                    "module"
                } else {
                    "function"
                },
                name,
            ));
        }
    }
    Some((kind(node)?, name_node(node)?))
}
pub(crate) fn call_target<'a>(
    epoch: AnalyzerEpoch,
    node: Node<'a>,
    text: &str,
    language: &str,
) -> Option<Node<'a>> {
    if language == "commonlisp" {
        return crate::languages::commonlisp_call_target(node, text);
    }
    if let Some(target) = crate::languages::call_target(node, language) {
        return Some(target);
    }
    if !matches!(
        node.kind(),
        "call_expression"
            | "call"
            | "method_invocation"
            | "invocation_expression"
            | "function_call_expression"
            | "member_call_expression"
            | "function_call"
            | "application_expression"
            | "apply_expression"
            | "command"
            | "list_lit"
    ) {
        return None;
    }
    if language == "clojure" {
        if node.kind() != "list_lit" || symbol(epoch, node, text, language).is_some() {
            return None;
        }
        return node.named_child(0).filter(|n| n.kind() == "sym_lit");
    }
    if node.kind() == "list_lit" {
        return None;
    }
    if language == "elixir" {
        if symbol(epoch, node, text, language).is_some() {
            return None;
        }
        if let Some(arguments) = node.parent().filter(|parent| parent.kind() == "arguments")
            && arguments.named_child(0) == Some(node)
            && arguments
                .parent()
                .is_some_and(|parent| symbol(epoch, parent, text, language).is_some())
        {
            return None;
        }
    }
    node.child_by_field_name("function")
        .or_else(|| node.child_by_field_name("name"))
        .or_else(|| node.child_by_field_name("method"))
        .or_else(|| node.child_by_field_name("target"))
        .or_else(|| {
            matches!(language, "swift" | "kotlin")
                .then(|| node.named_child(0))
                .flatten()
        })
}
pub fn extract(file: &SourceFile, text: &str, deadline: &Deadline) -> Result<Extraction> {
    extract_at(AnalyzerEpoch::active()?, file, text, deadline)
}
pub fn extract_at(
    epoch: AnalyzerEpoch,
    file: &SourceFile,
    text: &str,
    deadline: &Deadline,
) -> Result<Extraction> {
    deadline.check()?;
    let mut line_starts = vec![0];
    line_starts.extend(
        text.bytes()
            .enumerate()
            .filter_map(|(offset, byte)| (byte == b'\n').then_some(offset + 1)),
    );
    let mut output = Extraction::default();
    output.entities.push(entity(
        epoch,
        file,
        &file.path,
        "file",
        0,
        text.len(),
        text,
        &line_starts,
        None,
        None,
    ));
    if let Some(units) = crate::languages::embedded_units(&file.language, text)? {
        output.resolution.scopes.push(crate::resolution::Scope {
            start: 0,
            end: text.len(),
            parent: None,
            function: false,
        });
        for unit in units {
            deadline.check()?;
            if let Some(message) = unit.unsupported {
                output.diagnostics.push(Diagnostic {
                    path: file.path.clone(),
                    start_byte: unit.start,
                    message,
                    severity: "warning".into(),
                    producer: epoch.producer().into(),
                });
                continue;
            }
            let mut selected = file.clone();
            selected.language = unit.language.into();
            let code = text
                .get(unit.start..unit.end)
                .ok_or_else(|| anyhow::anyhow!("Embedded source range invalid"))?;
            let parsed = extract_at(epoch, &selected, code, deadline)?;
            merge_embedded(
                &mut output,
                parsed,
                unit.start,
                line_starts.partition_point(|offset| *offset <= unit.start) - 1,
                deadline,
            )?;
            if output.resolution.language.is_empty()
                || unit.language == "typescript"
                || unit.language == "tsx"
            {
                output.resolution.language = unit.language.into();
            }
        }
        crate::contracts::extract_document(file, text, &mut output, deadline)?;
        return Ok(output);
    }
    let effective_language = file.language.as_str();
    let parse_text = text;
    if let Some(language) = grammar(effective_language) {
        let mut parser = Parser::new();
        parser.set_language(&language)?;
        let mut cancelled = |_: &tree_sitter::ParseState| deadline.check().is_err();
        let tree = parser
            .parse_with_options(
                &mut |offset, _| &parse_text.as_bytes()[offset.min(parse_text.len())..],
                None,
                Some(ParseOptions::new().progress_callback(&mut cancelled)),
            )
            .ok_or_else(|| anyhow::anyhow!("Parser cancelled or failed"))?;
        let mut stack = vec![(tree.root_node(), 0usize)];
        let mut count = 0usize;
        while let Some((node, owner)) = stack.pop() {
            count += 1;
            if count.is_multiple_of(512) {
                deadline.check()?;
            }
            ensure!(count <= 1_000_000, "AST node budget exceeded");
            let mut current = owner;
            if let Some((kind, name)) = symbol(epoch, node, text, effective_language) {
                let name_text = name.utf8_text(text.as_bytes())?;
                if !name_text.is_empty() && name_text.len() <= 1000 {
                    let parent = &output.entities[owner];
                    let e = entity(
                        epoch,
                        file,
                        name_text,
                        kind,
                        node.start_byte(),
                        node.end_byte(),
                        text,
                        &line_starts,
                        Some(parent),
                        Some((name.start_byte(), name.end_byte())),
                    );
                    output.edges.push(RawEdge {
                        source_key: parent.key.clone(),
                        target_label: e.key.clone(),
                        relation: "contains".into(),
                        start_byte: node.start_byte(),
                        end_byte: node.end_byte(),
                        start_line: node.start_position().row + 1,
                        end_line: node.end_position().row + 1,
                    });
                    current = output.entities.len();
                    output.entities.push(e);
                }
            }
            let raw = |label: String, relation: &str| RawEdge {
                source_key: output.entities[current].key.clone(),
                target_label: label,
                relation: relation.into(),
                start_byte: node.start_byte(),
                end_byte: node.end_byte(),
                start_line: node.start_position().row + 1,
                end_line: node.end_position().row + 1,
            };
            if let Some(target) = call_target(epoch, node, text, effective_language) {
                output
                    .edges
                    .push(raw(target.utf8_text(text.as_bytes())?.into(), "calls"));
            }
            if matches!(
                node.kind(),
                "import_statement"
                    | "import_from_statement"
                    | "import_declaration"
                    | "use_declaration"
                    | "import_spec"
                    | "include_directive"
                    | "preproc_include"
                    | "require"
            ) {
                let target = node
                    .child_by_field_name("source")
                    .or_else(|| node.child_by_field_name("module_name"))
                    .or_else(|| node.child_by_field_name("path"))
                    .or_else(|| node.child_by_field_name("argument"));
                let label = target
                    .unwrap_or(node)
                    .utf8_text(text.as_bytes())?
                    .trim()
                    .trim_matches(['\'', '"', '<', '>'])
                    .to_string();
                output.edges.push(raw(label, "imports"));
            }
            if matches!(
                node.kind(),
                "superclass"
                    | "base_list"
                    | "extends_type_clause"
                    | "class_heritage"
                    | "super_interfaces"
            ) {
                output
                    .edges
                    .push(raw(node.utf8_text(text.as_bytes())?.into(), "inherits"));
            }
            if (node.is_error() || node.is_missing()) && output.diagnostics.len() < 200 {
                output.diagnostics.push(Diagnostic {
                    path: file.path.clone(),
                    start_byte: node.start_byte(),
                    message: format!("Syntax error near {}", node.kind()),
                    severity: "error".into(),
                    producer: epoch.producer().into(),
                });
            }
            if node.kind().contains("comment") {
                extract_rationale(
                    epoch,
                    file,
                    text,
                    node.start_byte(),
                    node.end_byte(),
                    &output.entities[current].clone(),
                    &mut output,
                    &line_starts,
                );
            }
            crate::contracts::extract_code_node(
                file,
                text,
                node,
                &output.entities[current].clone(),
                &mut output,
            )?;
            let mut cursor = node.walk();
            let children: Vec<_> = node.children(&mut cursor).collect();
            for child in children.into_iter().rev() {
                stack.push((child, current));
            }
        }
        let mut semantic_file = file.clone();
        semantic_file.language = effective_language.into();
        crate::resolution::enrich_at(
            epoch,
            tree.root_node(),
            &semantic_file,
            text,
            &mut output,
            deadline,
        )?;
    } else if file.language == "markdown" {
        let mut offset = 0;
        for text_line in text.split_inclusive('\n') {
            deadline.check()?;
            if text_line.starts_with('#') {
                let name = text_line.trim_start_matches('#').trim();
                if !name.is_empty() {
                    let root = output.entities[0].clone();
                    output.entities.push(entity(
                        epoch,
                        file,
                        name,
                        "concept",
                        offset,
                        offset + text_line.len(),
                        text,
                        &line_starts,
                        Some(&root),
                        None,
                    ));
                }
            }
            offset += text_line.len();
        }
        let links = Regex::new(r"\[[^\]]*\]\(([^\s)#]+)(?:#[^)]*)?\)")?;
        for captures in links.captures_iter(text).take(10_000) {
            let matched = captures.get(0).unwrap();
            output.edges.push(RawEdge {
                source_key: output.entities[0].key.clone(),
                target_label: captures[1].to_string(),
                relation: "documents".into(),
                start_byte: matched.start(),
                end_byte: matched.end(),
                start_line: line(text, matched.start()),
                end_line: line(text, matched.end()),
            });
        }
    }
    for declaration in crate::languages::pattern_declarations(&file.language, text, deadline)? {
        deadline.check()?;
        let root = output.entities[0].clone();
        let mut symbol = entity(
            epoch,
            file,
            declaration.name,
            declaration.kind,
            declaration.start,
            declaration.end,
            text,
            &line_starts,
            Some(&root),
            Some((declaration.name_start, declaration.name_end)),
        );
        symbol.producer = "yoke-structural-patterns-1".into();
        output.edges.push(RawEdge {
            source_key: root.key.clone(),
            target_label: symbol.key.clone(),
            relation: "contains".into(),
            start_byte: declaration.start,
            end_byte: declaration.end,
            start_line: symbol.start_line,
            end_line: symbol.end_line,
        });
        output.entities.push(symbol);
    }
    crate::contracts::extract_document(file, text, &mut output, deadline)?;
    Ok(output)
}
fn merge_embedded(
    output: &mut Extraction,
    mut unit: Extraction,
    offset: usize,
    line_offset: usize,
    deadline: &Deadline,
) -> Result<()> {
    let root = &output.entities[0];
    let keys: HashMap<_, _> = unit
        .entities
        .iter()
        .map(|entity| {
            (
                entity.key.clone(),
                if entity.kind == "file" {
                    root.key.clone()
                } else {
                    format!(
                        "{}:{}:{}",
                        entity.path,
                        entity.kind,
                        entity.start_byte + offset
                    )
                },
            )
        })
        .collect();
    let remap = |key: &str| keys.get(key).cloned().unwrap_or_else(|| key.into());
    for mut entity in unit
        .entities
        .drain(..)
        .filter(|entity| entity.kind != "file")
    {
        deadline.check()?;
        entity.key = remap(&entity.key);
        entity.parent_key = entity.parent_key.as_deref().map(remap);
        entity.start_byte += offset;
        entity.end_byte += offset;
        entity.start_line += line_offset;
        entity.end_line += line_offset;
        entity.name_start_byte = entity.name_start_byte.map(|byte| byte + offset);
        entity.name_end_byte = entity.name_end_byte.map(|byte| byte + offset);
        output.entities.push(entity);
    }
    for mut edge in unit.edges {
        deadline.check()?;
        edge.source_key = remap(&edge.source_key);
        edge.target_label = remap(&edge.target_label);
        edge.start_byte += offset;
        edge.end_byte += offset;
        edge.start_line += line_offset;
        edge.end_line += line_offset;
        output.edges.push(edge);
    }
    for mut diagnostic in unit.diagnostics {
        diagnostic.start_byte += offset;
        output.diagnostics.push(diagnostic);
    }
    let first = output.resolution.scopes.len();
    let scope = |index: usize| if index == 0 { 0 } else { first + index - 1 };
    for mut value in unit.resolution.scopes.into_iter().skip(1) {
        value.start += offset;
        value.end += offset;
        value.parent = value.parent.map(scope);
        output.resolution.scopes.push(value);
    }
    for mut binding in unit.resolution.bindings {
        binding.scope = scope(binding.scope);
        binding.defined_at += offset;
        if let crate::resolution::BindingTarget::Entity(key) = &mut binding.target {
            *key = remap(key);
        }
        output.resolution.bindings.push(binding);
    }
    for mut usage in unit.resolution.uses {
        usage.scope = scope(usage.scope);
        usage.start += offset;
        usage.end += offset;
        usage.source_key = remap(&usage.source_key);
        output.resolution.uses.push(usage);
    }
    for mut arity in unit.resolution.callable_arities {
        arity.entity_key = remap(&arity.entity_key);
        output.resolution.callable_arities.push(arity);
    }
    ensure!(
        output.entities.len() <= 1_000_000
            && output.resolution.scopes.len() <= 100_000
            && output.resolution.uses.len() <= 200_000,
        "Embedded extraction aggregate budget exceeded"
    );
    Ok(())
}
#[allow(clippy::too_many_arguments)]
fn extract_rationale(
    epoch: AnalyzerEpoch,
    file: &SourceFile,
    text: &str,
    start: usize,
    end: usize,
    parent: &Entity,
    output: &mut Extraction,
    line_starts: &[usize],
) {
    let comment = &text[start..end];
    if ["WHY:", "NOTE:", "ADR-", "RFC-"]
        .iter()
        .any(|marker| comment.contains(marker))
    {
        let e = entity(
            epoch,
            file,
            comment.trim(),
            "rationale",
            start,
            end,
            text,
            line_starts,
            Some(parent),
            None,
        );
        output.edges.push(RawEdge {
            source_key: parent.key.clone(),
            target_label: e.key.clone(),
            relation: "rationale".into(),
            start_byte: start,
            end_byte: end,
            start_line: line(text, start),
            end_line: line(text, end),
        });
        output.entities.push(e);
    }
}
fn relative_target(from: &str, label: &str, files: &BTreeMap<String, String>) -> Option<String> {
    if label.contains("://") {
        return None;
    }
    let mut components: Vec<&str> = from.split('/').collect();
    components.pop();
    for part in label.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                components.pop()?;
            }
            _ => components.push(part),
        }
    }
    let joined = components.join("/");
    for suffix in [
        "",
        ".ts",
        ".tsx",
        ".js",
        ".jsx",
        ".py",
        ".rs",
        "/index.ts",
        "/index.js",
        "/mod.rs",
    ] {
        if let Some(key) = files.get(&format!("{joined}{suffix}")) {
            return Some(key.clone());
        }
    }
    files.get(label).cloned()
}
pub fn assemble(
    snapshot: &Snapshot,
    extractions: Vec<Extraction>,
    deadline: &Deadline,
) -> Result<Graph> {
    assemble_at(AnalyzerEpoch::active()?, snapshot, extractions, deadline)
}
pub fn assemble_at(
    epoch: AnalyzerEpoch,
    snapshot: &Snapshot,
    extractions: Vec<Extraction>,
    deadline: &Deadline,
) -> Result<Graph> {
    let mut graph=Graph{snapshot_id:snapshot.id.clone(),coverage:Coverage{source_files:snapshot.files.len(),indexed_files:snapshot.files.len(),omissions:snapshot.omissions.len(),exhaustive:false,semantic:false,limitations:vec!["Structural candidate resolution is not compiler binding or exhaustive runtime coverage.".into(),"Unsupported text formats have full-text search but no AST semantics. Binary/media files are explicitly omitted.".into()],..Coverage::default()},..Graph::default()};
    for file in &snapshot.files {
        *graph
            .coverage
            .languages
            .entry(file.language.clone())
            .or_default() += 1;
    }
    let mut raw_edges = Vec::new();
    let mut scoped_records = Vec::new();
    for extraction in extractions {
        deadline.check()?;
        ensure!(
            extraction.entities.iter().all(|entity| {
                !entity.producer.starts_with("yoke-tree-sitter-")
                    || entity.producer == epoch.producer()
            }) && extraction.diagnostics.iter().all(|diagnostic| {
                !diagnostic.producer.starts_with("yoke-tree-sitter-")
                    || diagnostic.producer == epoch.producer()
            }),
            "Extraction analyzer epoch does not match assembly policy"
        );
        if let Some(file) = extraction
            .entities
            .iter()
            .find(|entity| entity.kind == "file")
        {
            scoped_records.push((file.path.clone(), extraction.resolution));
        }
        graph.entities.extend(extraction.entities);
        raw_edges.extend(extraction.edges);
        graph.diagnostics.extend(extraction.diagnostics);
    }
    if graph
        .entities
        .iter()
        .any(|e| e.producer == "yoke-contracts-1")
    {
        graph.coverage.limitations.push("Contract declarations and literal call patterns are source-bound hints. Name matches remain heuristic; ambiguous or differently scoped matches stay unresolved. SQL dialect binding, framework routing, external schema refs and runtime event delivery are not proven.".into());
    }
    raw_edges.extend(crate::contracts::candidate_edges(
        &graph.entities,
        snapshot,
        deadline,
    )?);
    let mut keys = HashMap::new();
    let mut names: HashMap<String, Vec<(String, String)>> = HashMap::new();
    let mut files = BTreeMap::new();
    for e in &mut graph.entities {
        e.snapshot_id = snapshot.id.clone();
        e.id = identity(&(&snapshot.id, &e.key));
        keys.insert(e.key.clone(), e.id.clone());
        if e.kind == "file" {
            files.insert(e.path.clone(), e.id.clone());
        } else {
            names
                .entry(e.name.clone())
                .or_default()
                .push((e.id.clone(), e.path.clone()));
        }
    }
    let paths: HashMap<_, _> = graph
        .entities
        .iter()
        .map(|e| (e.key.clone(), e.path.clone()))
        .collect();
    let hashes: HashMap<_, _> = snapshot.files.iter().map(|f| (&f.path, &f.hash)).collect();
    let resolver = crate::resolution::Resolver::new(&graph.entities, scoped_records, deadline)?;
    graph.coverage.limitations.push("Lexical/import/receiver bindings are syntax-local candidates. Parallel targets preserve possible dispatch; dynamic assignment, dependencies and runtime targets remain nonexhaustive.".into());
    for (index, raw) in raw_edges.into_iter().enumerate() {
        deadline.check()?;
        let path = paths
            .get(&raw.source_key)
            .ok_or_else(|| anyhow::anyhow!("Unknown edge owner"))?;
        let mut resolution = "unresolved";
        let targets = if let Some(id) = keys.get(&raw.target_label) {
            resolution = if crate::contracts::candidate_relation(&raw.relation) {
                "heuristic"
            } else {
                "syntactic"
            };
            vec![Some(id.clone())]
        } else if let Some(candidates) = resolver.resolve(path, &raw)? {
            if candidates.is_empty() {
                vec![None]
            } else {
                resolution = "heuristic";
                candidates.into_iter().map(Some).collect()
            }
        } else if matches!(raw.relation.as_str(), "imports" | "documents") {
            let target = if raw.relation == "imports"
                && !raw.target_label.starts_with("./")
                && !raw.target_label.starts_with("../")
            {
                None
            } else {
                relative_target(path, &raw.target_label, &files)
            };
            if target.is_some() {
                resolution = "syntactic";
            }
            vec![target]
        } else if crate::contracts::candidate_relation(&raw.relation) {
            vec![None]
        } else {
            let candidates = names.get(&raw.target_label);
            let local: Vec<_> = candidates
                .into_iter()
                .flatten()
                .filter(|(_, p)| p == path)
                .collect();
            let found = if local.len() == 1 {
                local.first().copied()
            } else if local.is_empty() && candidates.is_some_and(|c| c.len() == 1) {
                candidates.and_then(|c| c.first())
            } else {
                None
            };
            if let Some((id, _)) = found {
                resolution = "heuristic";
                vec![Some(id.clone())]
            } else {
                vec![None]
            }
        };
        if targets.iter().all(Option::is_none) {
            graph.coverage.unresolved += 1;
        }
        let eid = identity(&(
            &snapshot.id,
            path,
            raw.start_byte,
            raw.end_byte,
            &raw.relation,
            index,
        ));
        graph.evidence.push(Evidence {
            id: eid.clone(),
            snapshot_id: snapshot.id.clone(),
            path: path.clone(),
            content_hash: hashes[path].to_string(),
            start_byte: raw.start_byte,
            end_byte: raw.end_byte,
            start_line: raw.start_line,
            end_line: raw.end_line,
            anchor: "exact".into(),
            trust: "source-data".into(),
            producer: epoch.producer().into(),
        });
        for target in targets {
            graph.observations.push(Observation {
                id: identity(&(&snapshot.id, &raw.source_key, &target, &eid)),
                snapshot_id: snapshot.id.clone(),
                source: keys[&raw.source_key].clone(),
                target,
                target_label: raw.target_label.clone(),
                relation: raw.relation.clone(),
                resolution: resolution.into(),
                evidence_ids: vec![eid.clone()],
                producer: epoch.producer().into(),
            });
        }
    }
    Ok(graph)
}
