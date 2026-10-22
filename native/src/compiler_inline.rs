//! Host-selected compiler inlining with unpublished, original-byte effect checks.
use crate::{
    Engine, analysis,
    capture::Deadline,
    changes,
    model::{Entity, Snapshot, identity},
    projects::CompilerSpec,
};
use anyhow::{Context, Result, ensure};
use serde::Deserialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{collections::BTreeSet, fs, io::Read, path::Path};
use tree_sitter::{Node, Tree};
#[path = "compiler_inline_body.rs"]
mod body;

pub(crate) fn host_file(
    path: &Path,
    maximum: u64,
    d: &Deadline,
) -> Result<(std::path::PathBuf, String)> {
    ensure!(
        path.is_absolute(),
        "Inline compiler paths must be absolute host paths"
    );
    let path = path
        .canonicalize()
        .context("Inline compiler host file unavailable")?;
    let mut file = fs::File::open(&path)?;
    let expected = file.metadata()?.len();
    ensure!(
        file.metadata()?.is_file() && expected <= maximum,
        "Inline compiler host file capacity exceeded"
    );
    let mut sha = Sha256::new();
    let mut bytes = 0u64;
    let mut buffer = [0u8; 64_000];
    loop {
        d.check()?;
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        bytes += count as u64;
        ensure!(
            bytes <= expected,
            "Inline compiler host file grew during binding"
        );
        sha.update(&buffer[..count]);
    }
    ensure!(
        bytes == expected,
        "Inline compiler host file changed during binding"
    );
    Ok((path, hex::encode(sha.finalize())))
}
/// Reading and hashing trusted host files starts no process. This identity is
/// included only for explicitly registered compiler sessions.
pub fn host_binding(spec: &CompilerSpec, d: &Deadline) -> Result<String> {
    ensure!(spec.trusted, "Inline compiler requires explicit host trust");
    Ok(identity(&(
        "host_inline_compiler_v1",
        host_file(&spec.node, 128_000_000, d)?,
        host_file(&spec.typescript, 16_000_000, d)?,
    )))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Span {
    path: String,
    start_byte: usize,
    end_byte: usize,
    text: String,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Effect {
    schema: u32,
    path: String,
    keep_definition: bool,
    declaration: Span,
    references: Vec<Span>,
    edits: Vec<Span>,
    config: String,
    context_id: String,
    action_kind: String,
    before_diagnostics: Vec<Value>,
    after_diagnostics: Vec<Value>,
    result_hash: String,
}
fn parse(language: &str, source: &str, d: &Deadline) -> Result<Tree> {
    ensure!(
        source.len() <= 4_000_000 && !source.contains('\0'),
        "Inline source/result capacity or NUL refusal"
    );
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&analysis::grammar(language).context("Inline source syntax unavailable")?)?;
    let mut cancelled = |_: &tree_sitter::ParseState| d.check().is_err();
    let tree = parser
        .parse_with_options(
            &mut |offset, _| &source.as_bytes()[offset.min(source.len())..],
            None,
            Some(tree_sitter::ParseOptions::new().progress_callback(&mut cancelled)),
        )
        .context("Inline syntax parser cancelled")?;
    d.check()?;
    ensure!(
        !tree.root_node().has_error(),
        "Inline requires error-free source/result syntax"
    );
    Ok(tree)
}
fn exact_node<'a>(
    tree: &'a Tree,
    start: usize,
    end: usize,
    kind: &str,
    d: &Deadline,
) -> Result<Node<'a>> {
    let mut stack = vec![tree.root_node()];
    let mut count = 0usize;
    while let Some(node) = stack.pop() {
        d.check()?;
        count += 1;
        ensure!(count <= 200_000, "Inline AST capacity exceeded");
        if node.start_byte() == start
            && node.end_byte() == end
            && (node.kind() == kind
                || kind == "inline_reference"
                    && matches!(node.kind(), "identifier" | "shorthand_property_identifier"))
        {
            return Ok(node);
        }
        if node.start_byte() <= start && node.end_byte() >= end {
            ensure!(
                stack.len().saturating_add(node.named_child_count()) <= 200_000,
                "Inline AST frontier capacity exceeded"
            );
            let mut cursor = node.walk();
            stack.extend(node.named_children(&mut cursor));
        }
    }
    anyhow::bail!("Inline exact AST anchor absent")
}
fn selected<'a>(tree: &'a Tree, entity: &Entity, source: &str, d: &Deadline) -> Result<Node<'a>> {
    let node = exact_node(
        tree,
        entity.start_byte,
        entity.end_byte,
        "variable_declarator",
        d,
    )?;
    let name = node
        .child_by_field_name("name")
        .context("Inline name syntax absent")?;
    let value = node
        .child_by_field_name("value")
        .context("Inline initializer syntax absent")?;
    ensure!(
        name.kind() == "identifier"
            && Some(name.start_byte()) == entity.name_start_byte
            && Some(name.end_byte()) == entity.name_end_byte
            && name.utf8_text(source.as_bytes())? == entity.name
            && matches!(value.kind(), "arrow_function" | "function_expression"),
        "Inline captured declaration/name binding mismatch"
    );
    let parent = node.parent().context("Inline statement absent")?;
    ensure!(
        matches!(
            parent.kind(),
            "lexical_declaration" | "variable_declaration"
        ) && parent
            .parent()
            .is_some_and(|n| matches!(n.kind(), "program" | "statement_block")),
        "Inline requires a non-exported statement binding"
    );
    Ok(node)
}
fn whitespace(text: &str) -> bool {
    text.chars().all(char::is_whitespace) && !text.contains('\u{feff}')
}
fn removal(node: Node<'_>, edit: &Span, source: &str, d: &Deadline) -> Result<()> {
    ensure!(
        edit.text.is_empty()
            && edit.start_byte <= node.start_byte()
            && edit.end_byte >= node.end_byte(),
        "Inline declaration removal mismatch"
    );
    let parent = node.parent().context("Inline statement absent")?;
    let mut cursor = parent.walk();
    let declarations: Vec<_> = parent
        .named_children(&mut cursor)
        .filter(|n| n.kind() != "comment")
        .collect();
    ensure!(
        declarations
            .iter()
            .all(|n| n.kind() == "variable_declarator"),
        "Inline statement shape unsupported"
    );
    if declarations.len() == 1 {
        ensure!(
            edit.start_byte <= parent.start_byte()
                && edit.end_byte >= parent.end_byte()
                && whitespace(crate::paths::utf8_range(
                    source,
                    edit.start_byte,
                    parent.start_byte()
                )?)
                && whitespace(crate::paths::utf8_range(
                    source,
                    parent.end_byte(),
                    edit.end_byte
                )?),
            "Inline removal changes text outside its complete declaration"
        );
    } else {
        ensure!(
            edit.start_byte >= parent.start_byte() && edit.end_byte <= parent.end_byte(),
            "Inline sibling removal escapes its statement"
        );
        let remaining = format!(
            "{}{}",
            crate::paths::utf8_range(source, edit.start_byte, node.start_byte())?,
            crate::paths::utf8_range(source, node.end_byte(), edit.end_byte)?
        );
        ensure!(
            remaining.chars().filter(|c| !c.is_whitespace()).eq([',']),
            "Inline sibling removal must own exactly one comma and whitespace"
        );
        let mut cursor = parent.walk();
        let commas: Vec<_> = parent
            .children(&mut cursor)
            .filter(|n| {
                n.kind() == ","
                    && n.start_byte() >= edit.start_byte
                    && n.end_byte() <= edit.end_byte
            })
            .collect();
        ensure!(
            commas.len() == 1,
            "Inline sibling comma AST binding mismatch"
        );
        for sibling in declarations {
            d.check()?;
            ensure!(
                sibling == node
                    || sibling.end_byte() <= edit.start_byte
                    || sibling.start_byte() >= edit.end_byte,
                "Inline removal touches another declarator"
            );
        }
    }
    Ok(())
}
fn checked_span(span: &Span, path: &str, source: &str) -> Result<()> {
    ensure!(
        span.path == path
            && crate::paths::utf8_range(source, span.start_byte, span.end_byte)? == span.text,
        "Inline compiler span differs from captured source"
    );
    Ok(())
}

// Compare the original and copied initializer structurally. Compiler formatting
// may insert parameter parentheses and statement semicolons; grouping is erased
// only after parsing, so operator precedence and statement boundaries survive.
fn syntax_signature(node: Node<'_>, source: &str, d: &Deadline) -> Result<String> {
    let mut digest = Sha256::new();
    let mut stack = vec![(node, false, 0usize)];
    let mut nodes = 0usize;
    while let Some((node, closing, depth)) = stack.pop() {
        d.check()?;
        if closing {
            digest.update(b")");
            continue;
        }
        nodes += 1;
        ensure!(
            nodes <= 200_000 && depth <= 512,
            "Inline copied syntax capacity exceeded"
        );
        if node.kind() == "comment"
            || node.kind() == ";"
                && node
                    .parent()
                    .is_none_or(|parent| parent.kind() != "for_statement")
        {
            continue;
        }
        if node.kind() == "parenthesized_expression" {
            ensure!(
                node.named_child_count() <= 200_000,
                "Inline grouping capacity exceeded"
            );
            let mut cursor = node.walk();
            let children: Vec<_> = node
                .named_children(&mut cursor)
                .filter(|n| n.kind() != "comment")
                .collect();
            ensure!(children.len() == 1, "Inline grouping shape unsupported");
            stack.push((children[0], false, depth + 1));
            continue;
        }
        let flatten = matches!(node.kind(), "formal_parameters" | "required_parameter");
        if !flatten {
            digest.update(b"(");
            digest.update(node.kind().as_bytes());
            digest.update(b":");
            stack.push((node, true, depth));
        }
        if node.child_count() == 0 {
            let text = node.utf8_text(source.as_bytes())?;
            digest.update(text.len().to_le_bytes());
            digest.update(text.as_bytes());
        } else {
            ensure!(
                stack.len().saturating_add(node.child_count()) <= 200_000,
                "Inline syntax frontier capacity exceeded"
            );
            let mut cursor = node.walk();
            let children: Vec<_> = node.children(&mut cursor).collect();
            for child in children.into_iter().rev() {
                if node.kind() == "formal_parameters" && matches!(child.kind(), "(" | ")") {
                    continue;
                }
                stack.push((child, false, depth + 1));
            }
        }
    }
    Ok(hex::encode(digest.finalize()))
}
fn replacement_signature(language: &str, text: &str, d: &Deadline) -> Result<String> {
    ensure!(
        text.len() <= 3_999_000,
        "Inline replacement expression capacity exceeded"
    );
    let wrapped = format!("const __yoke_inline = {text};\n");
    let tree = parse(language, &wrapped, d)?;
    let root = tree.root_node();
    ensure!(
        root.named_child_count() == 1,
        "Inline replacement adds unrelated statements"
    );
    let statement = root
        .named_child(0)
        .context("Inline replacement statement absent")?;
    ensure!(
        statement.kind() == "lexical_declaration" && statement.named_child_count() == 1,
        "Inline replacement adds unrelated declarations"
    );
    let declaration = statement
        .named_child(0)
        .context("Inline replacement declaration absent")?;
    ensure!(
        declaration.kind() == "variable_declarator",
        "Inline replacement is not an initializer"
    );
    let value = declaration
        .child_by_field_name("value")
        .context("Inline replacement expression absent")?;
    syntax_signature(value, &wrapped, d)
}

fn shorthand_signature(language: &str, name: &str, text: &str, d: &Deadline) -> Result<String> {
    ensure!(
        text.len() <= 3_999_000,
        "Inline shorthand capacity exceeded"
    );
    let wrapped = format!("const __yoke_inline = {{{text}}};\n");
    let tree = parse(language, &wrapped, d)?;
    let root = tree.root_node();
    ensure!(
        root.named_child_count() == 1,
        "Inline shorthand adds statements"
    );
    let statement = root
        .named_child(0)
        .context("Inline shorthand statement absent")?;
    ensure!(
        statement.kind() == "lexical_declaration" && statement.named_child_count() == 1,
        "Inline shorthand adds declarations"
    );
    let object = statement
        .named_child(0)
        .and_then(|n| n.child_by_field_name("value"))
        .context("Inline shorthand object absent")?;
    ensure!(
        object.kind() == "object" && object.named_child_count() == 1,
        "Inline shorthand changes other properties"
    );
    let pair = object
        .named_child(0)
        .context("Inline shorthand pair absent")?;
    let key = pair
        .child_by_field_name("key")
        .context("Inline shorthand property key absent")?;
    ensure!(
        pair.kind() == "pair"
            && key.utf8_text(wrapped.as_bytes())? == name
            && key.kind() == "property_identifier",
        "Inline shorthand changes its original key"
    );
    syntax_signature(
        pair.child_by_field_name("value")
            .context("Inline shorthand function absent")?,
        &wrapped,
        d,
    )
}

fn diagnostic_spans(
    engine: &Engine,
    snapshot: &Snapshot,
    path: &str,
    source: &str,
    config: &str,
    diagnostics: &[Value],
    d: &Deadline,
) -> Result<()> {
    let mut sources = std::collections::BTreeMap::from([(path.to_owned(), source.to_owned())]);
    let mut bytes = source.len();
    for diagnostic in diagnostics {
        d.check()?;
        ensure!(
            diagnostic["severity"] == "warning"
                && diagnostic["config"] == config
                && diagnostic["code"].as_u64().is_some()
                && diagnostic["message"]
                    .as_str()
                    .is_some_and(|s| s.len() <= 32_000),
            "Inline diagnostic metadata mismatch"
        );
        if let Some(text) = diagnostic["text"].as_str() {
            let path = diagnostic["path"]
                .as_str()
                .context("Inline diagnostic source path absent")?;
            if !sources.contains_key(path) {
                let file = snapshot
                    .files
                    .iter()
                    .find(|file| file.path == path)
                    .context("Inline diagnostic source outside capture")?;
                bytes = bytes.saturating_add(file.size);
                ensure!(
                    bytes <= 14_000_000,
                    "Inline diagnostic source capacity exceeded"
                );
                sources.insert(
                    path.into(),
                    String::from_utf8(
                        engine
                            .store
                            .read_blob_exact(&file.hash, file.size, 10_000_000)?,
                    )?,
                );
            }
            let start = diagnostic["startByte"]
                .as_u64()
                .context("Inline diagnostic start absent")? as usize;
            let end = diagnostic["endByte"]
                .as_u64()
                .context("Inline diagnostic end absent")? as usize;
            ensure!(
                crate::paths::utf8_range(&sources[path], start, end)? == text,
                "Inline diagnostic source binding mismatch"
            );
        } else {
            ensure!(
                diagnostic["path"] == config,
                "Inline unanchored diagnostic path mismatch"
            );
        }
    }
    Ok(())
}

pub fn prepare(
    engine: &mut Engine,
    spec: &CompilerSpec,
    args: &Value,
    d: &Deadline,
) -> Result<Value> {
    d.check()?;
    ensure!(
        args.as_object().is_some_and(|v| v.keys().all(|k| [
            "snapshotId",
            "relativePath",
            "namePath",
            "keepDefinition"
        ]
        .contains(&k.as_str())))
            && serde_json::to_vec(args)?.len() <= 64_000
            && args.get("keepDefinition").is_none_or(Value::is_boolean),
        "Invalid compiler inline arguments"
    );
    let path = args["relativePath"]
        .as_str()
        .context("Inline source path required")?;
    ensure!(
        crate::paths::normalized(path)? == path && !crate::paths::protected(path),
        "Inline canonical source path required"
    );
    let name = args["namePath"]
        .as_str()
        .context("Inline name path required")?;
    let keep = args["keepDefinition"] == true;
    let binding = host_binding(spec, d)?;
    for tool in [&spec.node, &spec.typescript] {
        ensure!(
            !tool
                .canonicalize()?
                .starts_with(engine.root.canonicalize()?),
            "Inline compiler tools must be outside analyzed workspace"
        );
    }
    let (snapshot, graph, _, _) = engine.index(d)?;
    ensure!(
        args.get("snapshotId").is_none_or(|id| id == &snapshot.id),
        "Inline source snapshot is stale"
    );
    let scope = crate::capture::source_scope_policy(&engine.store, &snapshot)?;
    scope.check_host_write_scope(path)?;
    let file = scope.file(path).context("Inline source is not captured")?;
    ensure!(
        matches!(
            file.language.as_str(),
            "typescript" | "tsx" | "javascript" | "jsx"
        ) && file.size <= 4_000_000,
        "Inline compiler requires captured TS/JS source within capacity"
    );
    let symbol = crate::agent_repl::source_symbol(&engine.store, &snapshot, &graph, name, path, d)?;
    let epoch =
        crate::capture::analyzer_epoch_for_snapshot_with_deadline(&engine.store, &snapshot, d)?;
    ensure!(
        symbol.kind == "function"
            && symbol.language == file.language
            && symbol.producer == epoch.producer(),
        "Inline requires an original captured function binding"
    );
    let (source, hash) = crate::query::source(&engine.store, &snapshot, path)?;
    let tree = parse(&file.language, &source, d)?;
    if let Some(declaration) = body::classic(&tree, &symbol, &source, d)? {
        return body::prepare(
            engine,
            spec,
            &snapshot,
            &graph,
            &symbol,
            &source,
            &tree,
            declaration,
            keep,
            &binding,
            d,
        );
    }
    let node = selected(&tree, &symbol, &source, d)?;
    let intent = json!({"path":path,"name":symbol.name,"startByte":symbol.start_byte,"endByte":symbol.end_byte,"nameStartByte":symbol.name_start_byte,"nameEndByte":symbol.name_end_byte,"keepDefinition":keep});
    let report = crate::projects::execute(
        &engine.store,
        &snapshot,
        spec,
        "inline",
        Some(intent.clone()),
        d,
    )?;
    ensure!(
        host_binding(spec, d)? == binding,
        "Inline compiler host binding changed during execution"
    );
    ensure!(
        report["compilerHash"] == host_file(&spec.typescript, 16_000_000, d)?.1,
        "Inline compiler response hash differs from the selected host compiler"
    );
    let effect: Effect = crate::json::decode(report["inline"].clone())?;
    let (edits, result) = check_effect(&snapshot, &symbol, &source, &tree, node, keep, &effect, d)?;
    let _ = parse(&file.language, &result, d)?;
    ensure!(
        effect.config == "[inferred]"
            || snapshot
                .files
                .iter()
                .any(|file| file.path == effect.config && file.path.ends_with(".json")),
        "Inline compiler context is not captured"
    );
    diagnostic_spans(
        engine,
        &snapshot,
        path,
        &source,
        &effect.config,
        &effect.before_diagnostics,
        d,
    )?;
    diagnostic_spans(
        engine,
        &snapshot,
        path,
        &result,
        &effect.config,
        &effect.after_diagnostics,
        d,
    )?;
    let plan = changes::prepare(
        &mut engine.store,
        &snapshot,
        &graph,
        &json!({"edits":edits}),
        d,
    )?;
    Ok(crate::source_line_endings::qualify_provider_result(
        &plan,
        json!({"snapshotId":snapshot.id,"actions":[{"kind":effect.action_kind,"usable":true,"plan":plan}],"provider":"host_selected_typescript_compiler","compiler":"5.8.3","compilerHash":report["compilerHash"],"hostBinding":binding,"standardLibraryHashes":report["standardLibraryHashes"],"compilerContextId":effect.context_id,"compilerConfig":effect.config,"intent":intent,"sourceBindings":report["sourceBindings"],"sourceHash":hash,"effectDigest":identity(&report["inline"]),"beforeDiagnostics":effect.before_diagnostics,"afterDiagnostics":effect.after_diagnostics,"limitations":report["limitations"],"equivalent":"independent_compiler_variable_inline","complete":false,"commandsExecuted":false,"sourceExecuted":false,"sourceTreeWritten":false,"verificationRequired":true}),
    ))
}

#[allow(clippy::too_many_arguments)]
fn check_effect(
    snapshot: &Snapshot,
    symbol: &Entity,
    source: &str,
    tree: &Tree,
    node: Node<'_>,
    keep: bool,
    effect: &Effect,
    d: &Deadline,
) -> Result<(Vec<changes::Edit>, String)> {
    ensure!(
        effect.schema == 1
            && effect.path == symbol.path
            && effect.keep_definition == keep
            && effect.action_kind == "refactor.inline.variable"
            && crate::model::valid_id(&effect.context_id)
            && crate::model::valid_id(&effect.result_hash)
            && effect.config.len() <= 4096
            && !effect.references.is_empty()
            && effect.references.len() <= 1999
            && (effect.references.len() + 1..=effect.references.len() + 2)
                .contains(&effect.edits.len())
            && effect.edits.len() <= 2000
            && effect.before_diagnostics.len() <= 20_000
            && effect.after_diagnostics.len() <= 20_000
            && effect
                .before_diagnostics
                .iter()
                .chain(&effect.after_diagnostics)
                .all(|v| v["severity"] != "error"),
        "Inline compiler effect envelope mismatch"
    );
    checked_span(&effect.declaration, &symbol.path, source)?;
    ensure!(
        effect.declaration.start_byte == symbol.start_byte
            && effect.declaration.end_byte == symbol.end_byte,
        "Inline compiler selected declaration mismatch"
    );
    let mut refs = BTreeSet::new();
    for reference in &effect.references {
        d.check()?;
        checked_span(reference, &symbol.path, source)?;
        ensure!(
            reference.text == symbol.name
                && (reference.end_byte <= symbol.start_byte
                    || reference.start_byte >= symbol.end_byte)
                && refs.insert((reference.start_byte, reference.end_byte)),
            "Inline duplicate or recursive reference"
        );
        exact_node(
            tree,
            reference.start_byte,
            reference.end_byte,
            "inline_reference",
            d,
        )?;
    }
    let mut ordered: Vec<_> = effect.edits.iter().collect();
    ordered.sort_by_key(|e| (e.start_byte, e.end_byte));
    for pair in ordered.windows(2) {
        ensure!(
            pair[0].end_byte <= pair[1].start_byte && pair[0].start_byte != pair[1].start_byte,
            "Inline overlapping edits"
        );
    }
    let deletions: Vec<_> = ordered
        .iter()
        .copied()
        .filter(|edit| edit.text.is_empty())
        .collect();
    ensure!(
        (1..=2).contains(&deletions.len())
            && deletions
                .iter()
                .all(|edit| edit.path == symbol.path && edit.start_byte < edit.end_byte),
        "Inline removal bundle mismatch"
    );
    for pair in deletions.windows(2) {
        ensure!(
            pair[0].end_byte == pair[1].start_byte,
            "Inline split removal must be contiguous"
        );
    }
    let bundle = Span {
        path: symbol.path.clone(),
        start_byte: deletions[0].start_byte,
        end_byte: deletions.last().context("Inline removal absent")?.end_byte,
        text: String::new(),
    };
    removal(node, &bundle, source, d)?;
    let mut removed = 0;
    let initializer = node
        .child_by_field_name("value")
        .context("Inline source initializer absent")?;
    let expected_initializer = syntax_signature(initializer, source, d)?;
    let mut edits = Vec::new();
    let mut result = source.to_owned();
    let hash = snapshot
        .files
        .iter()
        .find(|f| f.path == symbol.path)
        .context("Inline source manifest absent")?
        .hash
        .clone();
    for edit in ordered.iter().rev() {
        d.check()?;
        let original = crate::paths::utf8_range(source, edit.start_byte, edit.end_byte)?;
        ensure!(
            edit.path == symbol.path && edit.text.len() <= 4_000_000 && !edit.text.contains('\0'),
            "Inline edited path/text capacity mismatch"
        );
        if edit.text.is_empty() {
            removed += 1;
            if keep {
                continue;
            }
        } else {
            ensure!(
                refs.remove(&(edit.start_byte, edit.end_byte))
                    && !edit.text.is_empty()
                    && edit.text != original,
                "Inline edit is not one exact compiler-bound consumer replacement"
            );
            let reference =
                exact_node(tree, edit.start_byte, edit.end_byte, "inline_reference", d)?;
            let signature = if reference.kind() == "shorthand_property_identifier" {
                shorthand_signature(&symbol.language, &symbol.name, &edit.text, d)?
            } else {
                replacement_signature(&symbol.language, &edit.text, d)?
            };
            ensure!(
                signature == expected_initializer,
                "Inline replacement changes the original function initializer syntax"
            );
        }
        ensure!(
            result
                .len()
                .saturating_sub(original.len())
                .saturating_add(edit.text.len())
                <= 4_000_000,
            "Inline result capacity exceeded"
        );
        result.replace_range(edit.start_byte..edit.end_byte, &edit.text);
        edits.push(changes::Edit::Replace {
            path: symbol.path.clone(),
            expected_hash: hash.clone(),
            start_byte: edit.start_byte,
            end_byte: edit.end_byte,
            text: edit.text.clone(),
        });
    }
    ensure!(
        removed == deletions.len()
            && refs.is_empty()
            && crate::model::hash(&result) == effect.result_hash,
        "Inline removal/reference/result integrity mismatch"
    );
    Ok((edits, result))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn hostile_effects_cannot_widen_paths_ranges_references_or_removal() {
        let root = tempfile::tempdir().unwrap();
        let source = "// 🌍\nconst helper=()=>7;\nexport const answer=helper();\n";
        fs::write(root.path().join("main.ts"), source).unwrap();
        let mut engine = Engine::open(root.path()).unwrap();
        let d = Deadline::new(20_000);
        let (snapshot, graph, _, _) = engine.index(&d).unwrap();
        let symbol = crate::agent_repl::source_symbol(
            &engine.store,
            &snapshot,
            &graph,
            "helper",
            "main.ts",
            &d,
        )
        .unwrap();
        let tree = parse("typescript", source, &d).unwrap();
        let node = selected(&tree, &symbol, source, &d).unwrap();
        let statement = node.parent().unwrap();
        let reference = source.rfind("helper").unwrap();
        let removal = json!({"path":"main.ts","startByte":statement.start_byte(),"endByte":statement.end_byte(),"text":""});
        let replacement =
            json!({"path":"main.ts","startByte":reference,"endByte":reference+6,"text":"(()=>7)"});
        let mut result = source.to_owned();
        result.replace_range(reference..reference + 6, "(()=>7)");
        result.replace_range(statement.byte_range(), "");
        let baseline = json!({"schema":1,"path":"main.ts","keepDefinition":false,"declaration":{"path":"main.ts","startByte":symbol.start_byte,"endByte":symbol.end_byte,"text":&source[symbol.start_byte..symbol.end_byte]},"references":[{"path":"main.ts","startByte":reference,"endByte":reference+6,"text":"helper"}],"edits":[removal,replacement],"config":"tsconfig.json","contextId":"a".repeat(64),"actionKind":"refactor.inline.variable","beforeDiagnostics":[],"afterDiagnostics":[],"resultHash":crate::model::hash(&result)});
        let effect: Effect = crate::json::decode(baseline.clone()).unwrap();
        assert_eq!(
            check_effect(&snapshot, &symbol, source, &tree, node, false, &effect, &d)
                .unwrap()
                .1,
            result
        );
        for (pointer, bad) in [
            ("/path", json!("../main.ts")),
            ("/references/0/path", json!("other.ts")),
            ("/references/0/text", json!("answer")),
            ("/references/0/startByte", json!(reference + 1)),
            ("/edits/1/path", json!("other.ts")),
            ("/edits/1/startByte", json!(2)),
            ("/edits/0/startByte", json!(0)),
            ("/edits/0/endByte", json!(source.len())),
            ("/edits/0/text", json!("// hidden replacement")),
            ("/resultHash", json!("b".repeat(64))),
            ("/keepDefinition", json!(true)),
            ("/actionKind", json!("refactor.move")),
            ("/afterDiagnostics", json!([{"severity":"error"}])),
        ] {
            let mut bad_effect = baseline.clone();
            *bad_effect.pointer_mut(pointer).unwrap() = bad;
            let effect: Effect = crate::json::decode(bad_effect).unwrap();
            assert!(
                check_effect(&snapshot, &symbol, source, &tree, node, false, &effect, &d).is_err(),
                "{pointer}"
            );
        }
        for key in ["command", "unsupported", "authority"] {
            let mut malformed = baseline.clone();
            malformed[key] = json!(true);
            assert!(crate::json::decode::<Effect>(malformed).is_err());
        }
        // A coherent replacement/result hash still cannot substitute another
        // expression for the original initializer.
        let mut substituted = baseline.clone();
        substituted["edits"][1]["text"] = json!("(()=>8)");
        substituted["resultHash"] = json!(crate::model::hash(result.replace("()=>7", "()=>8")));
        let effect: Effect = crate::json::decode(substituted).unwrap();
        assert!(check_effect(&snapshot, &symbol, source, &tree, node, false, &effect, &d).is_err());
        assert_eq!(
            engine
                .store
                .db
                .query_row("SELECT COUNT(*) FROM records WHERE kind='plan'", [], |r| {
                    r.get::<_, usize>(0)
                })
                .unwrap(),
            0
        );
    }

    #[test]
    fn copied_syntax_checks_preserve_precedence_statements_literals_and_operators() {
        let d = Deadline::new(10_000);
        for (a, b, equal) in [
            ("n=>n+1", "((n) => n + 1)", true),
            (
                "(n:number)=>{return n+1}",
                "((n: number) => { return n + 1; })",
                true,
            ),
            ("()=> (1+2)*3", "()=> 1+2*3", false),
            ("()=> {return 1;}", "()=> {return; 1;}", false),
            ("()=> 'one'", "()=> 'two'", false),
            ("(n:number)=>n+1", "(n:number)=>n-1", false),
            ("()=>{for(;x;y){}}", "()=>{for(x;;y){}}", false),
            ("()=>{for(;x;y)x()}", "()=>{for (;x;y) x();}", true),
        ] {
            assert_eq!(
                replacement_signature("typescript", a, &d).unwrap()
                    == replacement_signature("typescript", b, &d).unwrap(),
                equal,
                "{a} / {b}"
            );
        }
        assert!(replacement_signature("typescript", "()=>1; const unrelated=2", &d).is_err());
        assert_eq!(
            shorthand_signature("typescript", "helper", "helper: (()=>1)", &d).unwrap(),
            replacement_signature("typescript", "()=>1", &d).unwrap()
        );
        for bad in [
            "other: (()=>1)",
            "helper: (()=>1), other: 2",
            "['helper']: (()=>1)",
        ] {
            assert!(
                shorthand_signature("typescript", "helper", bad, &d).is_err(),
                "{bad}"
            );
        }
    }
}
