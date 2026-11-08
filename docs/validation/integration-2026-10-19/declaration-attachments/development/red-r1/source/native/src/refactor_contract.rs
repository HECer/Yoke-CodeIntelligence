//! Source-checked, crate-owned symbol move intent. Wire data cannot construct proof.
use crate::{
    Engine,
    capture::Deadline,
    model::{Entity, Graph, Snapshot, identity},
};
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use std::ops::Range;
use tree_sitter::{Node, Parser, Tree};

#[derive(Clone, Debug)]
pub(crate) struct CheckedFileBinding {
    snapshot_id: String,
    path: String,
    content_hash: String,
}
impl CheckedFileBinding {
    pub(crate) fn path(&self) -> &str {
        &self.path
    }
    pub(crate) fn content_hash(&self) -> &str {
        &self.content_hash
    }
    fn wire(&self) -> Value {
        json!({"snapshotId":self.snapshot_id,"path":self.path,"contentHash":self.content_hash})
    }
}

#[derive(Clone, Debug)]
pub(crate) struct CheckedSymbolBinding {
    file: CheckedFileBinding,
    entity_id: String,
    canonical_name_path: String,
    start_byte: usize,
    end_byte: usize,
    name_start_byte: usize,
    name_end_byte: usize,
}
impl CheckedSymbolBinding {
    pub(crate) fn file(&self) -> &CheckedFileBinding {
        &self.file
    }
    pub(crate) fn range(&self) -> Range<usize> {
        self.start_byte..self.end_byte
    }
    pub(crate) fn name_range(&self) -> Range<usize> {
        self.name_start_byte..self.name_end_byte
    }
    fn wire(&self) -> Value {
        json!({"snapshotId":self.file.snapshot_id,"path":self.file.path,"contentHash":self.file.content_hash,
            "entityId":self.entity_id,"canonicalNamePath":self.canonical_name_path,
            "startByte":self.start_byte,"endByte":self.end_byte,
            "nameStartByte":self.name_start_byte,"nameEndByte":self.name_end_byte})
    }
}

#[derive(Clone, Debug)]
pub(crate) enum CheckedMoveDestination {
    FileTopLevel {
        file: CheckedFileBinding,
        insertion_byte: usize,
    },
    ParentSymbol {
        parent: CheckedSymbolBinding,
        body_start_byte: usize,
        body_end_byte: usize,
    },
}
impl CheckedMoveDestination {
    pub(crate) fn file(&self) -> &CheckedFileBinding {
        match self {
            Self::FileTopLevel { file, .. } => file,
            Self::ParentSymbol { parent, .. } => parent.file(),
        }
    }
    pub(crate) fn insertion_byte(&self) -> usize {
        match self {
            Self::FileTopLevel { insertion_byte, .. } => *insertion_byte,
            Self::ParentSymbol { body_end_byte, .. } => *body_end_byte,
        }
    }
    fn wire(&self) -> Value {
        match self {
            Self::FileTopLevel { file, .. } => json!({"kind":"fileTopLevel","file":file.wire()}),
            Self::ParentSymbol {
                parent,
                body_start_byte,
                body_end_byte,
            } => {
                json!({"kind":"parentSymbol","parent":parent.wire(),"bodyStartByte":body_start_byte,"bodyEndByte":body_end_byte})
            }
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct CheckedMoveIntent {
    snapshot_id: String,
    workspace_id: String,
    manifest_hash: String,
    graph_hash: String,
    source: CheckedSymbolBinding,
    source_kind: &'static str,
    transfer: CheckedSourceTransfer,
    destination: CheckedMoveDestination,
}

#[derive(Clone, Debug)]
struct CheckedSourceTransfer {
    range: Range<usize>,
    attachments: Vec<(&'static str, Range<usize>)>,
}

fn source_transfer(
    syntax: &RustSyntax,
    declaration: Node<'_>,
    deadline: &Deadline,
) -> Result<CheckedSourceTransfer> {
    let parent = declaration
        .parent()
        .context("Declaration AST parent missing")?;
    let mut cursor = parent.walk();
    let mut start = None;
    let mut attachments = Vec::new();
    for node in parent.named_children(&mut cursor) {
        deadline.check()?;
        let range = syntax.range(node);
        if node == declaration {
            return Ok(CheckedSourceTransfer {
                range: start.unwrap_or(range.start)..range.end,
                attachments,
            });
        }
        let outer = node.kind() == "attribute_item"
            || matches!(node.kind(), "line_comment" | "block_comment")
                && node.child_by_field_name("outer").is_some();
        if outer {
            start.get_or_insert(range.start);
            attachments.push((node.kind(), range));
        } else if !matches!(node.kind(), "line_comment" | "block_comment") {
            start = None;
            attachments.clear();
        }
    }
    anyhow::bail!("Selected declaration is absent from its AST parent")
}
impl CheckedMoveIntent {
    #[allow(clippy::too_many_arguments)] // Explicit source and destination selectors share one immutable scope.
    pub(crate) fn resolve(
        engine: &Engine,
        snapshot: &Snapshot,
        graph: &Graph,
        source_path: &str,
        source_selector: &str,
        target_path: &str,
        target_parent_selector: Option<&str>,
        deadline: &Deadline,
    ) -> Result<Self> {
        deadline.check()?;
        crate::query_features::source_scope(&engine.store, snapshot, graph, deadline)?;
        let epoch = crate::capture::analyzer_epoch_for_snapshot_with_deadline(
            &engine.store,
            snapshot,
            deadline,
        )?;
        let scope = crate::capture::source_scope_policy(&engine.store, snapshot)?;
        for path in [source_path, target_path] {
            let file = scope.file(path)?;
            ensure!(
                file.size <= 4_000_000 && file.language == "rust",
                "Symbol move requires captured Rust files"
            );
            scope.check_host_write_scope(path)?;
            ensure!(
                !scope.path_policy(path)?.read_only,
                "Symbol move source is read-only"
            );
        }
        ensure!(
            !source_selector.is_empty(),
            "Symbol move source selector is empty"
        );
        let entity = crate::agent_repl::source_symbol(
            &engine.store,
            snapshot,
            graph,
            source_selector,
            source_path,
            deadline,
        )?;
        ensure!(
            entity.path == source_path
                && entity.language == "rust"
                && entity.producer == epoch.producer(),
            "Symbol move source binding differs"
        );
        let (source_text, source_hash) =
            crate::query::source(&engine.store, snapshot, source_path)?;
        let source_syntax = parse_rust(&source_text, deadline)?;
        let source_kind = match entity.kind.as_str() {
            "function" => "function_item",
            "struct" => "struct_item",
            "enum" => "enum_item",
            "interface" => "trait_item",
            "type" => "type_item",
            "constant" => "const_item",
            _ => anyhow::bail!("Selected symbol is not the required actual Rust AST declaration"),
        };
        let source_node = source_syntax.find(&entity, source_kind, deadline)?;
        let source_parent = source_node
            .parent()
            .context("Declaration AST parent missing")?;
        ensure!(
            is_item_container(source_parent),
            "Symbol move supports only root or inline-module {}",
            if source_kind == "function_item" {
                "functions"
            } else {
                "declarations"
            }
        );
        let transfer = source_transfer(&source_syntax, source_node, deadline)?;
        let source = symbol_binding(
            graph,
            snapshot,
            &entity,
            source_hash,
            &source_text,
            &source_syntax,
            source_node,
            deadline,
        )?;

        let (target_text, target_hash) =
            crate::query::source(&engine.store, snapshot, target_path)?;
        let target_syntax = parse_rust(&target_text, deadline)?;
        let destination = if let Some(selector) = target_parent_selector {
            ensure!(
                !selector.is_empty(),
                "Symbol move destination parent selector is empty"
            );
            let parent = crate::agent_repl::source_symbol(
                &engine.store,
                snapshot,
                graph,
                selector,
                target_path,
                deadline,
            )?;
            ensure!(
                parent.path == target_path
                    && parent.language == "rust"
                    && parent.producer == epoch.producer(),
                "Symbol move parent binding differs"
            );
            let parent_node = target_syntax.find(&parent, "mod_item", deadline)?;
            let body = parent_node
                .child_by_field_name("body")
                .filter(|n| n.kind() == "declaration_list")
                .context("Symbol move destination must be an actual inline module")?;
            check_container(body, deadline)?;
            let body_range = target_syntax.range(body);
            ensure!(
                body_range.end >= body_range.start + 2
                    && target_text.as_bytes()[body_range.start] == b'{'
                    && target_text.as_bytes()[body_range.end - 1] == b'}',
                "Inline module delimiters differ"
            );
            if source_path == target_path {
                ensure!(
                    source_syntax.range(source_parent) != body_range,
                    "Symbol already belongs to the destination parent"
                );
                ensure!(
                    parent.end_byte <= entity.start_byte || entity.end_byte <= parent.start_byte,
                    "Symbol move cannot target itself or its descendant"
                );
            }
            CheckedMoveDestination::ParentSymbol {
                parent: symbol_binding(
                    graph,
                    snapshot,
                    &parent,
                    target_hash,
                    &target_text,
                    &target_syntax,
                    parent_node,
                    deadline,
                )?,
                body_start_byte: body_range.start + 1,
                body_end_byte: body_range.end - 1,
            }
        } else {
            ensure!(
                source_path != target_path || source_parent.kind() != "source_file",
                "Symbol already belongs to the destination root"
            );
            CheckedMoveDestination::FileTopLevel {
                file: CheckedFileBinding {
                    snapshot_id: snapshot.id.clone(),
                    path: target_path.into(),
                    content_hash: target_hash,
                },
                insertion_byte: target_text.len(),
            }
        };
        deadline.check()?;
        Ok(Self {
            snapshot_id: snapshot.id.clone(),
            workspace_id: snapshot.workspace_id.clone(),
            manifest_hash: identity(snapshot),
            graph_hash: identity(graph),
            source,
            source_kind,
            transfer,
            destination,
        })
    }
    pub(crate) fn source_path(&self) -> &str {
        self.source.file.path()
    }
    pub(crate) fn destination_path(&self) -> &str {
        self.destination.file().path()
    }
    pub(crate) fn destination_kind(&self) -> &'static str {
        match self.destination {
            CheckedMoveDestination::FileTopLevel { .. } => "fileTopLevel",
            CheckedMoveDestination::ParentSymbol { .. } => "parentSymbol",
        }
    }
    pub(crate) fn source(&self) -> &CheckedSymbolBinding {
        &self.source
    }
    pub(crate) fn source_kind(&self) -> &'static str {
        self.source_kind
    }
    pub(crate) fn transfer_range(&self) -> Range<usize> {
        self.transfer.range.clone()
    }
    pub(crate) fn attachments(&self) -> &[(&'static str, Range<usize>)] {
        &self.transfer.attachments
    }
    pub(crate) fn destination(&self) -> &CheckedMoveDestination {
        &self.destination
    }
    pub(crate) fn wire_intent(&self, deadline: &Deadline) -> Result<Value> {
        deadline.check()?;
        let mut value = json!({"operation":"moveSymbol","source":self.source.wire(),"destination":self.destination.wire()});
        if !self.attachments().is_empty() {
            value["sourceTextRange"] =
                json!({"startByte":self.transfer.range.start,"endByte":self.transfer.range.end});
        }
        crate::json::encode_bounded(&value, 64000, deadline)?;
        Ok(value)
    }
    pub(crate) fn validate_binding(
        &self,
        snapshot: &Snapshot,
        graph: &Graph,
        deadline: &Deadline,
    ) -> Result<()> {
        deadline.check()?;
        ensure!(
            self.snapshot_id == snapshot.id
                && self.workspace_id == snapshot.workspace_id
                && graph.snapshot_id == snapshot.id
                && self.manifest_hash == identity(snapshot)
                && self.graph_hash == identity(graph),
            "Checked move intent source binding changed"
        );
        deadline.check()
    }
}

#[allow(clippy::too_many_arguments)] // Exact source, graph entity and AST node must agree before sealing.
fn symbol_binding(
    graph: &Graph,
    snapshot: &Snapshot,
    entity: &Entity,
    content_hash: String,
    text: &str,
    syntax: &RustSyntax,
    node: Node<'_>,
    deadline: &Deadline,
) -> Result<CheckedSymbolBinding> {
    let name = node
        .child_by_field_name("name")
        .context("Symbol AST name missing")?;
    let name_range = syntax.range(name);
    ensure!(
        entity.name_start_byte == Some(name_range.start)
            && entity.name_end_byte == Some(name_range.end)
            && text.get(name_range.clone()) == Some(entity.name.as_str()),
        "Symbol AST name range differs"
    );
    Ok(CheckedSymbolBinding {
        file: CheckedFileBinding {
            snapshot_id: snapshot.id.clone(),
            path: entity.path.clone(),
            content_hash,
        },
        entity_id: entity.id.clone(),
        canonical_name_path: crate::agent_repl::canonical_source_symbol_path(
            graph, entity, deadline,
        )?,
        start_byte: entity.start_byte,
        end_byte: entity.end_byte,
        name_start_byte: name_range.start,
        name_end_byte: name_range.end,
    })
}

pub(crate) struct RustSyntax {
    tree: Tree,
    offset: usize,
}
impl RustSyntax {
    pub(crate) fn root(&self) -> Node<'_> {
        self.tree.root_node()
    }
    pub(crate) fn range(&self, node: Node<'_>) -> Range<usize> {
        node.start_byte() + self.offset..node.end_byte() + self.offset
    }
    fn find(&self, entity: &Entity, kind: &str, deadline: &Deadline) -> Result<Node<'_>> {
        let mut stack = vec![self.root()];
        let mut found = None;
        let mut count = 0usize;
        while let Some(node) = stack.pop() {
            deadline.check()?;
            count += 1;
            ensure!(count <= 200_000, "Symbol move AST capacity exceeded");
            if self.range(node) == (entity.start_byte..entity.end_byte) && node.kind() == kind {
                ensure!(found.is_none(), "Ambiguous symbol AST range");
                found = Some(node);
            }
            let mut cursor = node.walk();
            stack.extend(node.named_children(&mut cursor));
        }
        found.context("Selected symbol is not the required actual Rust AST declaration")
    }
}
pub(crate) fn parse_rust(text: &str, deadline: &Deadline) -> Result<RustSyntax> {
    deadline.check()?;
    ensure!(
        text.len() <= 4_000_000 && !text.contains('\0'),
        "Symbol move Rust source capacity exceeded"
    );
    // Only the recognized leading UTF-8 BOM is excluded from parser input.
    // Staged content retains every original byte and all coordinates are rebased.
    let offset = usize::from(text.starts_with('\u{feff}')) * 3;
    let input = &text.as_bytes()[offset..];
    let mut parser = Parser::new();
    parser.set_language(&tree_sitter_rust::LANGUAGE.into())?;
    let mut cancelled = |_: &tree_sitter::ParseState| deadline.check().is_err();
    let tree = parser
        .parse_with_options(
            &mut |at, _| &input[at.min(input.len())..],
            None,
            Some(tree_sitter::ParseOptions::new().progress_callback(&mut cancelled)),
        )
        .context("Symbol move Rust parser cancelled")?;
    deadline.check()?;
    ensure!(
        !tree.root_node().has_error(),
        "Symbol move requires error-free Rust syntax"
    );
    let syntax = RustSyntax { tree, offset };
    let mut stack = vec![syntax.root()];
    let mut count = 0usize;
    while let Some(node) = stack.pop() {
        deadline.check()?;
        count += 1;
        ensure!(count <= 200_000, "Symbol move AST capacity exceeded");
        if is_item_container(node) {
            check_container(node, deadline)?;
        }
        let mut cursor = node.walk();
        stack.extend(node.named_children(&mut cursor));
    }
    Ok(syntax)
}
fn is_item_container(node: Node<'_>) -> bool {
    node.kind() == "source_file"
        || node.kind() == "declaration_list"
            && node.parent().is_some_and(|p| {
                p.kind() == "mod_item" && p.child_by_field_name("body") == Some(node)
            })
}
fn check_container(container: Node<'_>, deadline: &Deadline) -> Result<()> {
    let mut cursor = container.walk();
    let (mut item_seen, mut outer_pending) = (false, false);
    for node in container.named_children(&mut cursor) {
        deadline.check()?;
        match node.kind() {
            "shebang" => ensure!(
                container.kind() == "source_file" && !item_seen && !outer_pending,
                "Misplaced Rust shebang"
            ),
            "inner_attribute_item" => ensure!(
                !item_seen && !outer_pending,
                "Misplaced Rust inner preamble"
            ),
            "attribute_item" => outer_pending = true,
            "line_comment" | "block_comment" => {
                if node.child_by_field_name("inner").is_some() {
                    ensure!(
                        !item_seen && !outer_pending,
                        "Misplaced Rust inner documentation"
                    );
                } else if node.child_by_field_name("outer").is_some() {
                    outer_pending = true;
                }
            }
            _ => {
                item_seen = true;
                outer_pending = false;
            }
        }
    }
    ensure!(
        !outer_pending,
        "Dangling Rust outer attributes or documentation at destination boundary"
    );
    Ok(())
}
