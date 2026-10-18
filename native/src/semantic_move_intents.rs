//! Exact original-operation and AST placement admission before change publication.
use crate::{
    Engine,
    capture::Deadline,
    changes,
    lsp::{CheckedEditOperation, CheckedTextEdit, CheckedWorkspaceEdit},
    model::{Graph, Snapshot},
    refactor_contract::{CheckedMoveDestination, CheckedMoveIntent, RustSyntax, parse_rust},
};
use anyhow::{Context, Result, ensure};
use std::{
    collections::{BTreeMap, BTreeSet},
    ops::Range,
};
use tree_sitter::Node;

pub(crate) struct CheckedMoveEffect {
    edit: CheckedWorkspaceEdit,
    intent: CheckedMoveIntent,
}

pub(crate) fn check_move_effect(
    engine: &Engine,
    snapshot: &Snapshot,
    graph: &Graph,
    intent: &CheckedMoveIntent,
    edit: CheckedWorkspaceEdit,
    deadline: &Deadline,
) -> Result<CheckedMoveEffect> {
    intent.validate_binding(snapshot, graph, deadline)?;
    crate::query_features::source_scope(&engine.store, snapshot, graph, deadline)?;
    ensure!(
        edit.operations
            .iter()
            .all(|op| matches!(op, CheckedEditOperation::TextDocument { .. })),
        "Move effect rejected: resource operations are unsupported"
    );
    let scope = crate::capture::source_scope_policy(&engine.store, snapshot)?;
    for binding in [intent.source().file(), intent.destination().file()] {
        ensure!(
            scope.file(binding.path())?.hash == binding.content_hash(),
            "Move effect rejected: checked intent file binding differs"
        );
    }
    let mut documents = BTreeMap::new();
    let mut originals = BTreeMap::new();
    let mut source_bytes = 0usize;
    for operation in &edit.operations {
        deadline.check()?;
        let CheckedEditOperation::TextDocument {
            path,
            source_hash,
            original_source,
            edits,
            ..
        } = operation
        else {
            unreachable!("resource operations refused above")
        };
        ensure!(
            *original_source && documents.insert(path.as_str(), edits.as_slice()).is_none(),
            "Move effect rejected: only one original document per path"
        );
        scope.check_host_write_scope(path)?;
        let file = scope.file(path)?;
        ensure!(
            !scope.path_policy(path)?.read_only && file.hash == *source_hash,
            "Move effect rejected: original source binding differs"
        );
        source_bytes = source_bytes
            .checked_add(file.size)
            .context("Move effect source size overflow")?;
        ensure!(
            source_bytes <= 32_000_000,
            "Move effect source capacity exceeded"
        );
        let (source, hash) = crate::query::source(&engine.store, snapshot, path)?;
        ensure!(
            hash == *source_hash,
            "Move effect rejected: original source binding differs"
        );
        let mut after = source.clone();
        for replacement in edits.iter().rev() {
            deadline.check()?;
            crate::paths::utf8_range(&source, replacement.start, replacement.end)?;
            after.replace_range(replacement.start..replacement.end, &replacement.text);
            ensure!(
                after.len() <= 4_000_000,
                "Move effect output capacity exceeded"
            );
        }
        ensure!(
            edit.files.get(path).and_then(Option::as_deref) == Some(after.as_str()),
            "Move effect rejected: final content differs from original operations"
        );
        originals.insert(path.clone(), source);
    }
    ensure!(
        documents.len() == edit.files.len() && edit.files.values().all(Option::is_some),
        "Move effect rejected: only original text documents are supported"
    );
    let source = originals
        .get(intent.source_path())
        .context("Move effect rejected: exact source cut required")?;
    let selected = intent.transfer_range();
    let declaration = crate::paths::utf8_range(source, selected.start, selected.end)?;
    let core_range = intent.source().range();
    let core_declaration = crate::paths::utf8_range(source, core_range.start, core_range.end)?;
    let source_edits = documents[intent.source_path()];
    ensure!(
        source_edits
            .iter()
            .filter(|e| e.start == selected.start && e.end == selected.end && e.text.is_empty())
            .count()
            == 1,
        "Move effect rejected: exact source cut required"
    );
    let mut insertions = Vec::new();
    for (path, edits) in &documents {
        for replacement in *edits {
            deadline.check()?;
            if replacement.start == replacement.end
                && replacement.text.trim_matches([' ', '\t', '\r', '\n']) == declaration
            {
                insertions.push((*path, replacement));
            }
            if *path == intent.source_path()
                && !(replacement.start == selected.start
                    && replacement.end == selected.end
                    && replacement.text.is_empty())
            {
                ensure!(
                    !touches(replacement, &selected),
                    "Move effect rejected: source declaration changed outside its cut"
                );
            }
        }
    }
    ensure!(
        insertions.len() == 1,
        "Move effect rejected: exact declaration insertion required"
    );
    let (insert_path, insertion) = insertions[0];
    ensure!(
        insert_path == intent.destination_path()
            && insertion.start == intent.destination().insertion_byte(),
        "Move effect rejected: insertion destination differs"
    );
    let target_edits = documents[intent.destination_path()];
    if let CheckedMoveDestination::ParentSymbol {
        parent,
        body_start_byte,
        body_end_byte,
    } = intent.destination()
    {
        let header = parent.range().start..*body_start_byte;
        let closing = *body_end_byte..parent.range().end;
        for replacement in target_edits {
            deadline.check()?;
            ensure!(
                !touches(replacement, &header) && !touches(replacement, &closing),
                "Move effect rejected: destination parent boundary changed"
            );
        }
    }
    let prefix = insertion.text.len()
        - insertion
            .text
            .trim_start_matches([' ', '\t', '\r', '\n'])
            .len();
    let inserted_start = map_position(insertion.start, false, target_edits)?
        .checked_add(prefix)
        .context("Move insertion position overflow")?;
    let inserted_bundle = inserted_start
        ..inserted_start
            .checked_add(declaration.len())
            .context("Move insertion range overflow")?;
    let inserted = inserted_bundle.start + core_range.start - selected.start
        ..inserted_bundle.start + core_range.end - selected.start;
    for (path, after) in &edit.files {
        deadline.check()?;
        if scope.file(path)?.language != "rust" {
            continue;
        }
        let after = after
            .as_deref()
            .context("Move effect rejected: deleted text document")?;
        let syntax = parse_rust(after, deadline)
            .map_err(|_| anyhow::anyhow!("Move effect rejected: final Rust syntax invalid"))?;
        let before = &originals[path];
        let original_syntax = parse_rust(before, deadline)
            .map_err(|_| anyhow::anyhow!("Move effect rejected: original Rust syntax invalid"))?;
        let original_nodes = ast_nodes(&original_syntax, deadline)?;
        let final_nodes = IndexedNodes::new(&syntax, deadline)?;
        check_declaration_provenance(
            (&original_syntax, &original_nodes, before),
            (&syntax, &final_nodes.all, after),
            documents[path.as_str()],
            (path, intent, core_declaration, &inserted),
            deadline,
        )?;
        if path == intent.source_path() || path == intent.destination_path() {
            Preservation {
                before: &original_syntax,
                before_nodes: &original_nodes,
                before_text: before,
                after: &syntax,
                after_nodes: &final_nodes,
                after_text: after,
                edits: documents[path.as_str()],
                moved_source: (path == intent.source_path()).then(|| selected.clone()),
                deadline,
            }
            .check()?;
        }
        if path != intent.destination_path() {
            continue;
        }
        let declaration_node =
            find_node(&syntax, intent.source_kind(), inserted.clone(), deadline)?;
        let name = declaration_node
            .child_by_field_name("name")
            .context("Move effect rejected: inserted function name missing")?;
        let old_name = intent.source().name_range();
        ensure!(
            syntax.range(name)
                == (inserted.start + old_name.start - core_range.start
                    ..inserted.start + old_name.end - core_range.start),
            "Move effect rejected: inserted function name differs"
        );
        for (kind, original) in intent.attachments() {
            deadline.check()?;
            let range = inserted_bundle.start + original.start - selected.start
                ..inserted_bundle.start + original.end - selected.start;
            let attachment = find_node(&syntax, kind, range, deadline)?;
            ensure!(
                attachment_owner(attachment, deadline)? == declaration_node,
                "Move effect rejected: transferred attachment owner differs"
            );
        }
        let parent = declaration_node
            .parent()
            .context("Move effect rejected: inserted function parent missing")?;
        match intent.destination() {
            CheckedMoveDestination::FileTopLevel { .. } => ensure!(
                parent == syntax.root(),
                "Move effect rejected: inserted function has the wrong direct parent"
            ),
            CheckedMoveDestination::ParentSymbol {
                parent: expected,
                body_start_byte,
                body_end_byte,
            } => {
                ensure!(
                    parent.kind() == "declaration_list",
                    "Move effect rejected: inserted function has the wrong direct parent"
                );
                let module = parent
                    .parent()
                    .context("Move effect rejected: destination module missing")?;
                let expected_range = map_position(expected.range().start, true, target_edits)?
                    ..map_position(expected.range().end, false, target_edits)?;
                let expected_body = map_position(body_start_byte - 1, true, target_edits)?
                    ..map_position(body_end_byte + 1, false, target_edits)?;
                ensure!(
                    module.kind() == "mod_item"
                        && module.child_by_field_name("body") == Some(parent)
                        && syntax.range(module) == expected_range
                        && syntax.range(parent) == expected_body,
                    "Move effect rejected: inserted function has the wrong direct parent"
                );
                let name = module
                    .child_by_field_name("name")
                    .context("Move effect rejected: destination module name missing")?;
                let expected_name = map_position(expected.name_range().start, true, target_edits)?
                    ..map_position(expected.name_range().end, false, target_edits)?;
                ensure!(
                    syntax.range(name) == expected_name,
                    "Move effect rejected: destination parent boundary changed"
                );
            }
        }
    }
    deadline.check()?;
    // This capability privately retains the admitted batch until one preparation.
    Ok(CheckedMoveEffect {
        edit,
        intent: intent.clone(),
    })
}
// A boundary insertion is before the first byte; only a strict interior insertion
// modifies the protected range. Nonempty replacements overlap normally.
fn touches(edit: &CheckedTextEdit, range: &Range<usize>) -> bool {
    if edit.start == edit.end {
        range.start < edit.start && edit.start < range.end
    } else {
        edit.start < range.end && range.start < edit.end
    }
}
fn map_position(at: usize, include_insertion_at: bool, edits: &[CheckedTextEdit]) -> Result<usize> {
    let mut mapped = at as i128;
    for edit in edits {
        if edit.start == edit.end {
            if edit.start < at || edit.start == at && include_insertion_at {
                mapped += edit.text.len() as i128;
            }
        } else if edit.end <= at {
            mapped += edit.text.len() as i128 - (edit.end - edit.start) as i128;
        } else {
            ensure!(
                !(edit.start < at && at < edit.end),
                "Move effect rejected: destination parent boundary changed"
            );
        }
    }
    usize::try_from(mapped).context("Move effect mapped position overflow")
}
fn find_node<'a>(
    syntax: &'a RustSyntax,
    kind: &str,
    range: Range<usize>,
    deadline: &Deadline,
) -> Result<Node<'a>> {
    let mut stack = vec![syntax.root()];
    let (mut count, mut found) = (0usize, None);
    while let Some(node) = stack.pop() {
        deadline.check()?;
        count += 1;
        ensure!(count <= 200_000, "Move effect AST capacity exceeded");
        if node.kind() == kind && syntax.range(node) == range {
            ensure!(
                found.is_none(),
                "Move effect rejected: ambiguous inserted function"
            );
            found = Some(node);
        }
        let mut cursor = node.walk();
        stack.extend(node.named_children(&mut cursor));
    }
    found.context("Move effect rejected: exact inserted function AST missing")
}
fn ast_nodes<'a>(syntax: &'a RustSyntax, deadline: &Deadline) -> Result<Vec<Node<'a>>> {
    let mut stack = vec![syntax.root()];
    let mut all = Vec::new();
    while let Some(node) = stack.pop() {
        deadline.check()?;
        ensure!(all.len() < 200_000, "Move effect AST capacity exceeded");
        all.push(node);
        let mut cursor = node.walk();
        stack.extend(node.named_children(&mut cursor));
    }
    Ok(all)
}
struct IndexedNodes<'a> {
    all: Vec<Node<'a>>,
    ranges: BTreeMap<(String, usize, usize), Option<Node<'a>>>,
}
impl<'a> IndexedNodes<'a> {
    fn new(syntax: &'a RustSyntax, deadline: &Deadline) -> Result<Self> {
        let all = ast_nodes(syntax, deadline)?;
        let mut ranges = BTreeMap::new();
        for node in &all {
            deadline.check()?;
            let range = syntax.range(*node);
            ranges
                .entry((node.kind().to_owned(), range.start, range.end))
                .and_modify(|prior| *prior = None)
                .or_insert(Some(*node));
        }
        Ok(Self { all, ranges })
    }
    fn get(&self, kind: &str, range: Range<usize>, reason: &'static str) -> Result<Node<'a>> {
        self.ranges
            .get(&(kind.to_owned(), range.start, range.end))
            .copied()
            .flatten()
            .with_context(|| format!("Move effect rejected: {reason}"))
    }
}
fn mapped_range(range: Range<usize>, edits: &[CheckedTextEdit]) -> Result<Range<usize>> {
    Ok(map_position(range.start, true, edits)?..map_position(range.end, false, edits)?)
}
type OriginalAst<'a, 'tree> = (&'a RustSyntax, &'a [Node<'tree>], &'a str);
fn check_declaration_provenance(
    before: OriginalAst<'_, '_>,
    after: OriginalAst<'_, '_>,
    edits: &[CheckedTextEdit],
    selected: (&str, &CheckedMoveIntent, &str, &Range<usize>),
    deadline: &Deadline,
) -> Result<()> {
    let (path, intent, declaration, inserted) = selected;
    let mut allowed = BTreeSet::new();
    if path == intent.destination_path() {
        allowed.insert((inserted.start, inserted.end));
    }
    for node in before.1 {
        deadline.check()?;
        let range = before.0.range(*node);
        if node.kind() != intent.source_kind()
            || before.2.get(range.clone()) != Some(declaration)
            || path == intent.source_path() && range == intent.source().range()
        {
            continue;
        }
        // Each original declaration is tracked separately. A removed node has no
        // surviving nonempty range; it cannot authorize a newly wrapped copy.
        if let Ok(mapped) = mapped_range(range, edits)
            && mapped.start < mapped.end
        {
            allowed.insert((mapped.start, mapped.end));
        }
    }
    for node in after.1 {
        deadline.check()?;
        let range = after.0.range(*node);
        if node.kind() == intent.source_kind() && after.2.get(range.clone()) == Some(declaration) {
            ensure!(
                allowed.contains(&(range.start, range.end)),
                "Move effect rejected: extra {} copy",
                if intent.source_kind() == "function_item" {
                    "function"
                } else {
                    "declaration"
                }
            );
        }
    }
    Ok(())
}
fn is_container(node: Node<'_>) -> bool {
    node.kind() == "source_file"
        || node.kind() == "declaration_list"
            && node.parent().is_some_and(|parent| {
                parent.kind() == "mod_item" && parent.child_by_field_name("body") == Some(node)
            })
}
fn attachment_owner<'tree>(mut node: Node<'tree>, deadline: &Deadline) -> Result<Node<'tree>> {
    while let Some(next) = node.next_named_sibling() {
        deadline.check()?;
        if !matches!(
            next.kind(),
            "attribute_item" | "line_comment" | "block_comment"
        ) {
            return Ok(next);
        }
        node = next;
    }
    anyhow::bail!("Move effect rejected: existing Rust attachment changed")
}
struct Preservation<'a, 'before, 'after> {
    before: &'a RustSyntax,
    before_nodes: &'a [Node<'before>],
    before_text: &'a str,
    after: &'a RustSyntax,
    after_nodes: &'a IndexedNodes<'after>,
    after_text: &'a str,
    edits: &'a [CheckedTextEdit],
    moved_source: Option<Range<usize>>,
    deadline: &'a Deadline,
}
impl Preservation<'_, '_, '_> {
    fn inner_prefix(&self, container: Node<'_>) -> Result<Range<usize>> {
        let range = self.before.range(container);
        let root = container.kind() == "source_file";
        let start = if root { 0 } else { range.start + 1 };
        let mut end = if root {
            self.before_text.len()
        } else {
            range.end - 1
        };
        let mut cursor = container.walk();
        for node in container.named_children(&mut cursor) {
            self.deadline.check()?;
            let prefix = matches!(node.kind(), "shebang" | "inner_attribute_item")
                || matches!(node.kind(), "line_comment" | "block_comment")
                    && node.child_by_field_name("outer").is_none();
            if !prefix {
                end = self.before.range(node).start;
                break;
            }
        }
        Ok(start..end)
    }
    fn bytes(&self, range: Range<usize>, reason: &'static str) -> Result<Range<usize>> {
        let mapped = mapped_range(range.clone(), self.edits)
            .map_err(|_| anyhow::anyhow!("Move effect rejected: {reason}"))?;
        ensure!(
            self.after_text.get(mapped.clone()) == self.before_text.get(range),
            "Move effect rejected: {reason}"
        );
        Ok(mapped)
    }
    fn boundary_token<'tree>(&self, mut node: Node<'tree>, first: bool) -> Result<Node<'tree>> {
        loop {
            self.deadline.check()?;
            let mut selected = None;
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                self.deadline.check()?;
                if !child.is_extra() {
                    selected = Some(child);
                    if first {
                        break;
                    }
                }
            }
            match selected {
                Some(child) => node = child,
                None => return Ok(node),
            }
        }
    }
    fn item(&self, node: Node<'_>, reason: &'static str) -> Result<Node<'_>> {
        self.deadline.check()?;
        let mapped = mapped_range(self.before.range(node), self.edits)
            .map_err(|_| anyhow::anyhow!("Move effect rejected: {reason}"))?;
        let actual = self.after_nodes.get(node.kind(), mapped, reason)?;
        // Preserve the original declaration's syntax anchors rather than all
        // reference-bearing content. A whole-owner replacement has no surviving
        // token ranges; ordinary path edits retain these anchors and AST owner.
        for first in [true, false] {
            let original = self.boundary_token(node, first)?;
            let mapped = self.bytes(self.before.range(original), reason)?;
            let final_token = self.boundary_token(actual, first)?;
            ensure!(
                original.kind() == final_token.kind() && self.after.range(final_token) == mapped,
                "Move effect rejected: {reason}"
            );
        }
        if let Some(name) = node.child_by_field_name("name") {
            let mapped = self.bytes(self.before.range(name), reason)?;
            ensure!(
                actual
                    .child_by_field_name("name")
                    .is_some_and(|final_name| final_name.kind() == name.kind()
                        && self.after.range(final_name) == mapped),
                "Move effect rejected: {reason}"
            );
        }
        Ok(actual)
    }

    fn container(&self, node: Node<'_>, reason: &'static str) -> Result<Node<'_>> {
        if node.kind() == "source_file" {
            return Ok(self.after.root());
        }
        let module = node
            .parent()
            .context("Move effect rejected: protected Rust preamble changed")?;
        self.item(module, reason)?
            .child_by_field_name("body")
            .with_context(|| format!("Move effect rejected: {reason}"))
    }
    fn check(&self) -> Result<()> {
        const PREAMBLE: &str = "protected Rust preamble changed";
        const ATTACHMENT: &str = "existing Rust attachment changed";
        if self.before_text.starts_with('\u{feff}') {
            ensure!(
                self.after_text.starts_with('\u{feff}'),
                "Move effect rejected: {PREAMBLE}"
            );
            self.bytes(0..3, PREAMBLE)?;
        }
        let mut checked_prefixes = BTreeSet::new();
        for node in self.before_nodes {
            self.deadline.check()?;
            let range = self.before.range(*node);
            if self
                .moved_source
                .as_ref()
                .is_some_and(|moved| moved.start <= range.start && range.end <= moved.end)
            {
                // The complete original bundle is checked as one unchanged
                // insertion, including its actual declaration and attachment owners.
                continue;
            }
            let Some(parent) = node.parent().filter(|parent| is_container(*parent)) else {
                continue;
            };
            let inner = node.kind() == "inner_attribute_item"
                || node.kind() == "shebang"
                || matches!(node.kind(), "line_comment" | "block_comment")
                    && node.child_by_field_name("inner").is_some();
            let outer = node.kind() == "attribute_item"
                || matches!(node.kind(), "line_comment" | "block_comment")
                    && node.child_by_field_name("outer").is_some();
            if inner {
                let container = self.before.range(parent);
                if checked_prefixes.insert((container.start, container.end)) {
                    // Preserve original preamble trivia and line endings too,
                    // ending before the first complete item or outer attachment.
                    self.bytes(self.inner_prefix(parent)?, PREAMBLE)?;
                }
                let range = self.bytes(self.before.range(*node), PREAMBLE)?;
                let actual = self.after_nodes.get(node.kind(), range, PREAMBLE)?;
                let owner = self.container(parent, PREAMBLE)?;
                ensure!(
                    actual.parent() == Some(owner),
                    "Move effect rejected: {PREAMBLE}"
                );
                if node.kind() == "shebang" {
                    let leading = usize::from(self.after_text.starts_with('\u{feff}')) * 3;
                    ensure!(
                        self.after.range(actual).start == leading,
                        "Move effect rejected: {PREAMBLE}"
                    );
                }
            } else if outer {
                let range = self.bytes(self.before.range(*node), ATTACHMENT)?;
                let actual = self.after_nodes.get(node.kind(), range, ATTACHMENT)?;
                let owner = self.item(attachment_owner(*node, self.deadline)?, ATTACHMENT)?;
                ensure!(
                    attachment_owner(actual, self.deadline)? == owner,
                    "Move effect rejected: {ATTACHMENT}"
                );
            }
        }
        Ok(())
    }
}

impl CheckedMoveEffect {
    pub(crate) fn prepare(
        self,
        engine: &mut Engine,
        snapshot: &Snapshot,
        graph: &Graph,
        deadline: &Deadline,
    ) -> Result<changes::Plan> {
        self.intent.validate_binding(snapshot, graph, deadline)?;
        crate::query_features::source_scope(&engine.store, snapshot, graph, deadline)?;
        self.edit.prepare(engine, snapshot, graph, deadline)
    }
}
