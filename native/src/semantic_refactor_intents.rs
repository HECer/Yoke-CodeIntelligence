//! Independent, bounded projections of explicit provider edits over exact source.
use crate::{
    Engine,
    capture::Deadline,
    lsp::{CheckedEditOperation, CheckedTextEdit, CheckedWorkspaceEdit},
    model::{Entity, Graph, Snapshot},
};
use anyhow::{Context, Result, ensure};
use serde::Serialize;
use serde_json::{Value, json};
use std::collections::BTreeMap;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct SourceTarget {
    snapshot_id: String,
    path: String,
    content_hash: String,
    entity_id: String,
    name_path: String,
    start_byte: usize,
    end_byte: usize,
    name_start_byte: usize,
    name_end_byte: usize,
}
impl SourceTarget {
    pub(super) fn checked(
        engine: &Engine,
        snapshot: &Snapshot,
        symbol: &Entity,
        name_path: &str,
    ) -> Result<Self> {
        let (source, hash) = crate::query::source(&engine.store, snapshot, &symbol.path)?;
        let start = symbol
            .name_start_byte
            .context("Refactoring exact name anchor absent")?;
        let end = symbol
            .name_end_byte
            .context("Refactoring exact name anchor absent")?;
        ensure!(
            symbol.snapshot_id == snapshot.id
                && symbol.start_byte <= start
                && end <= symbol.end_byte,
            "Refactoring selected symbol source binding mismatch"
        );
        crate::paths::utf8_range(&source, symbol.start_byte, symbol.end_byte)?;
        ensure!(
            crate::paths::utf8_range(&source, start, end)? == symbol.name,
            "Refactoring exact symbol name mismatch"
        );
        Ok(Self {
            snapshot_id: snapshot.id.clone(),
            path: symbol.path.clone(),
            content_hash: hash,
            entity_id: symbol.id.clone(),
            name_path: name_path.into(),
            start_byte: symbol.start_byte,
            end_byte: symbol.end_byte,
            name_start_byte: start,
            name_end_byte: end,
        })
    }
}

#[derive(Serialize)]
#[serde(
    tag = "operation",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub(super) enum RefactorIntent {
    Rename {
        source: SourceTarget,
        new_name: String,
        rename_in_comments: bool,
        rename_in_text_occurrences: bool,
    },
    Inline {
        source: SourceTarget,
        keep_definition: bool,
    },
}

pub(super) fn operations_digest(
    snapshot: &Snapshot,
    server_profile: &Value,
    operations: &[CheckedEditOperation],
) -> String {
    crate::model::identity(&(
        "checked_refactor_operations_v1",
        &snapshot.workspace_id,
        &snapshot.id,
        crate::model::identity(snapshot),
        server_profile,
        operations,
    ))
}

/// Projection is deliberately limited to original-coordinate text edits. A
/// resource operation or second virtual version needs a separate typed contract.
fn original_edits(
    engine: &Engine,
    snapshot: &Snapshot,
    checked: &CheckedWorkspaceEdit,
    deadline: &Deadline,
) -> Result<BTreeMap<String, Vec<CheckedTextEdit>>> {
    let mut result = BTreeMap::new();
    for operation in &checked.operations {
        deadline.check()?;
        let CheckedEditOperation::TextDocument {
            path,
            source_hash,
            original_source,
            version,
            edits,
        } = operation
        else {
            anyhow::bail!("Native intent projection does not support provider resource operations");
        };
        let file = snapshot
            .files
            .iter()
            .find(|file| file.path == *path)
            .context("Intent source outside captured manifest")?;
        ensure!(
            *original_source
                && source_hash == &file.hash
                && version.is_none_or(|version| version == 1),
            "Intent projection requires exact original source coordinates"
        );
        ensure!(
            result.insert(path.clone(), edits.clone()).is_none(),
            "Intent projection refuses repeated virtual document edits"
        );
        crate::query::source(&engine.store, snapshot, path)?;
    }
    Ok(result)
}

fn rebuild(
    engine: &Engine,
    snapshot: &Snapshot,
    checked: &mut CheckedWorkspaceEdit,
    mut edits: BTreeMap<String, Vec<CheckedTextEdit>>,
    deadline: &Deadline,
) -> Result<()> {
    let scope = crate::capture::source_scope_policy(&engine.store, snapshot)?;
    let mut files = BTreeMap::new();
    let mut operations = Vec::new();
    let mut bytes = 0usize;
    for (path, replacements) in &mut edits {
        deadline.check()?;
        scope.check_host_write_scope(path)?;
        let (mut source, hash) = crate::query::source(&engine.store, snapshot, path)?;
        replacements.sort_by_key(|edit| (edit.start, edit.end));
        replacements.dedup_by(|a, b| a.start == b.start && a.end == b.end && a.text == b.text);
        ensure!(
            replacements.len() <= 2000,
            "Intent projection edit capacity exceeded"
        );
        for pair in replacements.windows(2) {
            ensure!(
                pair[0].end <= pair[1].start && pair[0].start != pair[1].start,
                "Intent projection conflicts with provider edits"
            );
        }
        for edit in replacements.iter().rev() {
            crate::paths::utf8_range(&source, edit.start, edit.end)?;
            source.replace_range(edit.start..edit.end, &edit.text);
            ensure!(
                source.len() <= 4_000_000,
                "Intent projection file capacity exceeded"
            );
        }
        bytes = bytes.saturating_add(source.len());
        ensure!(
            bytes <= 32_000_000 && files.len() < 2000,
            "Intent projection source capacity exceeded"
        );
        operations.push(CheckedEditOperation::TextDocument {
            path: path.clone(),
            source_hash: hash,
            original_source: true,
            version: Some(1),
            edits: replacements.clone(),
        });
        files.insert(path.clone(), Some(source));
    }
    checked.operations = operations;
    checked.files = files;
    Ok(())
}

pub(super) fn keep_definition(
    engine: &Engine,
    snapshot: &Snapshot,
    graph: &Graph,
    symbol: &Entity,
    checked: &mut CheckedWorkspaceEdit,
    deadline: &Deadline,
) -> Result<Value> {
    let mut edits = original_edits(engine, snapshot, checked, deadline)?;
    let declaration = edits
        .get_mut(&symbol.path)
        .context("Inline provider did not edit the selected declaration")?;
    let mut removed = 0usize;
    declaration.retain(|edit| {
        if edit.start == symbol.start_byte && edit.end == symbol.end_byte && edit.text.is_empty() {
            removed += 1;
            false
        } else {
            true
        }
    });
    ensure!(
        removed == 1,
        "Keeping a definition requires one isolated exact declaration removal"
    );
    let mut effective = 0usize;
    for (path, replacements) in &edits {
        let (source, _) = crate::query::source(&engine.store, snapshot, path)?;
        for edit in replacements {
            deadline.check()?;
            ensure!(
                path != &symbol.path
                    || edit.end <= symbol.start_byte
                    || edit.start >= symbol.end_byte,
                "Inline provider also changes the retained declaration"
            );
            if crate::paths::utf8_range(&source, edit.start, edit.end)? != edit.text {
                effective += 1;
            }
        }
    }
    ensure!(
        effective > 0,
        "Keeping a definition requires a separate effective inline replacement"
    );
    let mut needed = std::collections::BTreeSet::new();
    for observation in &graph.observations {
        deadline.check()?;
        if observation.relation == "calls" && observation.target.as_deref() == Some(&symbol.id) {
            ensure!(
                observation.snapshot_id == snapshot.id,
                "Inline call observation snapshot mismatch"
            );
            needed.extend(observation.evidence_ids.iter().map(String::as_str));
            ensure!(
                needed.len() <= 2000,
                "Inline call evidence capacity exceeded"
            );
        }
    }
    let mut call_replacements = Vec::new();
    for evidence in &graph.evidence {
        deadline.check()?;
        if needed.contains(evidence.id.as_str())
            && let Some(replacements) = edits.get(&evidence.path)
        {
            let (source, hash) = crate::query::source(&engine.store, snapshot, &evidence.path)?;
            ensure!(
                evidence.snapshot_id == snapshot.id && hash == evidence.content_hash,
                "Inline call exact source binding mismatch"
            );
            let before = crate::paths::utf8_range(&source, evidence.start_byte, evidence.end_byte)?;
            if replacements.iter().any(|edit| {
                edit.start == evidence.start_byte
                    && edit.end == evidence.end_byte
                    && !edit.text.is_empty()
                    && edit.text != before
            }) {
                call_replacements.push(json!({"evidenceId":evidence.id,"path":evidence.path,"contentHash":hash,"startByte":evidence.start_byte,"endByte":evidence.end_byte}));
            }
        }
    }
    ensure!(
        !call_replacements.is_empty(),
        "Keeping a definition requires a source-bound call replacement for the selected symbol"
    );
    rebuild(engine, snapshot, checked, edits, deadline)?;
    let (source, _) = crate::query::source(&engine.store, snapshot, &symbol.path)?;
    let retained = checked.files[&symbol.path]
        .as_ref()
        .context("Retained declaration file absent")?;
    // No edit before the declaration is allowed to shift its exact byte anchor.
    // This conservative restriction avoids interpreting offsets as symbol proof.
    ensure!(
        checked.operations.iter().all(
            |op| !matches!(op, CheckedEditOperation::TextDocument{path,edits,..}
        if path==&symbol.path && edits.iter().any(|edit|edit.start<symbol.end_byte))
        ),
        "Keeping a definition refuses edits before its exact source anchor"
    );
    ensure!(
        crate::paths::utf8_range(retained, symbol.start_byte, symbol.end_byte)?
            == crate::paths::utf8_range(&source, symbol.start_byte, symbol.end_byte)?,
        "Retained declaration bytes changed"
    );
    Ok(
        json!({"semantics":"isolated_provider_declaration_removal_omitted","replacementCount":effective,"sourceBoundCallReplacements":call_replacements,"callBindingSemantics":"provider_inline_action_and_captured_call_candidate_not_exhaustive_compiler_proof","retainedDeclarationHash":crate::model::hash(&source.as_bytes()[symbol.start_byte..symbol.end_byte])}),
    )
}

/// Text occurrence flags cover captured writable Rust comment nodes and plain
/// string literal nodes. They do not assert semantic binding of prose mentions.
pub(super) fn rename_occurrences(
    engine: &Engine,
    snapshot: &Snapshot,
    symbol: &Entity,
    new_name: &str,
    occurrences: (bool, bool),
    checked: &mut CheckedWorkspaceEdit,
    deadline: &Deadline,
) -> Result<Value> {
    let (comments, text) = occurrences;
    ensure!(
        symbol.language == "rust",
        "Native comment/text rename currently requires Rust source syntax"
    );
    let identifier = regex::Regex::new(r"\A[_\p{XID_Start}][_\p{XID_Continue}]*\z")?;
    ensure!(
        identifier.is_match(&symbol.name) && identifier.is_match(new_name),
        "Native occurrence rename requires plain Unicode identifiers"
    );
    let word = regex::Regex::new(&format!(r"\b{}\b", regex::escape(&symbol.name)))?;
    let mut edits = original_edits(engine, snapshot, checked, deadline)?;
    let name_start = symbol.name_start_byte.context("Rename name start absent")?;
    let name_end = symbol.name_end_byte.context("Rename name end absent")?;
    ensure!(
        edits.get(&symbol.path).is_some_and(|edits| edits
            .iter()
            .any(|edit| edit.start == name_start && edit.end == name_end && edit.text == new_name)),
        "Rename intent requires an exact provider replacement of the selected source name"
    );
    for path in edits.keys() {
        ensure!(
            crate::capture::validate_language(&engine.store, snapshot, path)? == "rust",
            "Native occurrence flags cannot classify provider edits outside Rust source syntax"
        );
    }
    let scope = crate::capture::source_scope_policy(&engine.store, snapshot)?;
    let mut bindings = BTreeMap::new();
    let mut excluded = Vec::new();
    let mut bytes = 0usize;
    let mut count = 0usize;
    for file in &snapshot.files {
        deadline.check()?;
        if crate::capture::validate_language(&engine.store, snapshot, &file.path)? != "rust"
            || scope.path_policy(&file.path)?.read_only
        {
            excluded.push(json!({"path":file.path,"contentHash":file.hash,"reason":"outside_native_writable_rust_occurrence_scope"}));
            continue;
        }
        bytes = bytes.saturating_add(file.size);
        ensure!(
            bytes <= 16_000_000 && bindings.len() < 2000,
            "Native occurrence source capacity exceeded"
        );
        let (source, hash) = crate::query::source(&engine.store, snapshot, &file.path)?;
        bindings.insert(file.path.clone(), hash);
        let mut parser = tree_sitter::Parser::new();
        parser.set_language(&tree_sitter_rust::LANGUAGE.into())?;
        let mut cancelled = |_: &tree_sitter::ParseState| deadline.check().is_err();
        let tree = parser
            .parse_with_options(
                &mut |offset, _| &source.as_bytes()[offset.min(source.len())..],
                None,
                Some(tree_sitter::ParseOptions::new().progress_callback(&mut cancelled)),
            )
            .context("Native occurrence parser cancelled")?;
        deadline.check()?;
        ensure!(
            !tree.root_node().has_error(),
            "Native occurrence classification requires error-free Rust syntax"
        );
        let mut stack = vec![tree.root_node()];
        let mut nodes = 0usize;
        while let Some(node) = stack.pop() {
            deadline.check()?;
            nodes += 1;
            ensure!(
                nodes <= 200000,
                "Native occurrence syntax capacity exceeded"
            );
            let is_comment = matches!(node.kind(), "line_comment" | "block_comment");
            let is_string = matches!(node.kind(), "string_literal" | "raw_string_literal");
            if is_comment || is_string {
                if is_comment && !comments || is_string && !text {
                    let violates = edits.get(&file.path).is_some_and(|replacements| {
                        replacements.iter().any(|edit| {
                            let touches = if edit.start == edit.end {
                                node.start_byte() <= edit.start && edit.start <= node.end_byte()
                            } else {
                                edit.start < node.end_byte() && node.start_byte() < edit.end
                            };
                            touches
                                && source
                                    .get(edit.start..edit.end)
                                    .is_none_or(|before| before != edit.text)
                        })
                    });
                    ensure!(
                        !violates,
                        "Provider rename changes a disabled comment/text occurrence class"
                    );
                }
                if is_comment && comments || is_string && text {
                    let value =
                        crate::paths::utf8_range(&source, node.start_byte(), node.end_byte())?;
                    let mut cursor = node.walk();
                    let content_ranges: Vec<_> = node
                        .named_children(&mut cursor)
                        .filter(|child| child.kind() == "string_content")
                        .map(|child| (child.start_byte(), child.end_byte()))
                        .collect();
                    for occurrence in word.find_iter(value) {
                        let start = node.start_byte() + occurrence.start();
                        let end = node.start_byte() + occurrence.end();
                        if is_string
                            && !content_ranges.iter().any(|&(content_start, content_end)| {
                                content_start <= start && end <= content_end
                            })
                        {
                            continue;
                        }
                        ensure!(
                            !is_string || !value.starts_with('b') || new_name.is_ascii(),
                            "Native Unicode occurrence rename cannot create non-ASCII Rust byte-string contents"
                        );
                        ensure!(
                            !is_string || !value.contains(['{', '}']),
                            "Native text occurrence in a potentially interpolated string requires a provider intent contract"
                        );
                        count += 1;
                        ensure!(count <= 2000, "Native occurrence edit capacity exceeded");
                        edits
                            .entry(file.path.clone())
                            .or_default()
                            .push(CheckedTextEdit {
                                start,
                                end,
                                text: new_name.into(),
                            });
                    }
                }
            } else {
                let mut cursor = node.walk();
                for child in node.named_children(&mut cursor) {
                    stack.push(child);
                }
            }
        }
    }
    rebuild(engine, snapshot, checked, edits, deadline)?;
    Ok(
        json!({"semantics":"literal_identifier_occurrences_in_captured_writable_rust_comments_and_plain_strings","semanticBindingOfTextOccurrences":false,"supplementalOccurrenceCount":count,"sourceBindings":bindings,"excludedSources":excluded}),
    )
}
