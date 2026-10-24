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

fn host_file(path: &Path, maximum: u64, d: &Deadline) -> Result<(std::path::PathBuf, String)> {
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
        if node.start_byte() == start && node.end_byte() == end && node.kind() == kind {
            return Ok(node);
        }
        if node.start_byte() <= start && node.end_byte() >= end {
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
    let effect: Effect = crate::json::decode(report["inline"].clone())?;
    let (edits, result) = check_effect(&snapshot, &symbol, &source, &tree, node, keep, &effect, d)?;
    let _ = parse(&file.language, &result, d)?;
    let plan = changes::prepare(
        &mut engine.store,
        &snapshot,
        &graph,
        &json!({"edits":edits}),
        d,
    )?;
    Ok(
        json!({"snapshotId":snapshot.id,"actions":[{"kind":effect.action_kind,"usable":true,"plan":plan}],"provider":"host_selected_typescript_compiler","compiler":"5.8.3","compilerHash":report["compilerHash"],"hostBinding":binding,"standardLibraryHashes":report["standardLibraryHashes"],"compilerContextId":effect.context_id,"compilerConfig":effect.config,"intent":intent,"sourceBindings":report["sourceBindings"],"sourceHash":hash,"effectDigest":identity(&report["inline"]),"beforeDiagnostics":effect.before_diagnostics,"afterDiagnostics":effect.after_diagnostics,"limitations":report["limitations"],"equivalent":"independent_compiler_variable_inline","complete":false,"commandsExecuted":false,"sourceExecuted":false,"sourceTreeWritten":false,"verificationRequired":true}),
    )
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
            && effect.edits.len() == effect.references.len() + 1
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
            "identifier",
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
    let mut removed = 0;
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
        if edit.start_byte <= symbol.start_byte && edit.end_byte >= symbol.end_byte {
            removal(node, edit, source, d)?;
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
        removed == 1 && refs.is_empty() && crate::model::hash(&result) == effect.result_hash,
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
}
