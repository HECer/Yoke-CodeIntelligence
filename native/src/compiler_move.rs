//! Actual host-selected compiler moves with independent source/effect admission.
use crate::{
    Engine,
    capture::Deadline,
    changes,
    model::{Entity, identity},
    projects::CompilerSpec,
};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
#[path = "compiler_move_ast.rs"]
mod ast;
#[path = "compiler_move_attributes.rs"]
mod attributes;
#[path = "compiler_move_bindings.rs"]
mod bindings;
#[path = "compiler_move_repairs.rs"]
mod repairs;
#[cfg(test)]
#[path = "compiler_move_tests.rs"]
mod tests;

// Scalar variables and namespaces are not invented as legacy graph entities.
// Their exact top-level AST selection has its own explicit provenance.
struct Declaration {
    graph_entity: Option<Entity>,
    name: String,
    path: String,
    language: String,
    start_byte: usize,
    end_byte: usize,
    name_start_byte: Option<usize>,
    name_end_byte: Option<usize>,
}
impl Declaration {
    fn from_graph(entity: Entity) -> Self {
        Self {
            name: entity.name.clone(),
            path: entity.path.clone(),
            language: entity.language.clone(),
            start_byte: entity.start_byte,
            end_byte: entity.end_byte,
            name_start_byte: entity.name_start_byte,
            name_end_byte: entity.name_end_byte,
            graph_entity: Some(entity),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Span {
    path: String,
    start_byte: usize,
    end_byte: usize,
    text: String,
}
impl Span {
    fn key(&self) -> (String, usize, usize) {
        (self.path.clone(), self.start_byte, self.end_byte)
    }
    fn check(&self, contents: &BTreeMap<String, String>) -> Result<()> {
        ensure!(
            crate::paths::utf8_range(
                contents
                    .get(&self.path)
                    .context("Move binding source outside capture")?,
                self.start_byte,
                self.end_byte
            )? == self.text,
            "Move exact source span mismatch"
        );
        Ok(())
    }
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
enum Target {
    Captured {
        span: Span,
    },
    Module {
        path: String,
    },
    Library {
        name: String,
        hash: String,
        start_byte: usize,
        end_byte: usize,
        text: String,
    },
}
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Binding {
    span: Span,
    resolution: String,
    targets: Vec<Target>,
}
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct NamedBinding {
    name: String,
    resolution: String,
    targets: Vec<Target>,
}
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct TargetSet {
    resolution: String,
    targets: Vec<Target>,
}
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Import {
    kind: String,
    span: Span,
    module: TargetSet,
    names: Vec<Binding>,
}
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Module {
    path: String,
    is_module: bool,
    exports: Vec<NamedBinding>,
    imports: Vec<Import>,
}
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Observations {
    bindings: Vec<Binding>,
    modules: Vec<Module>,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct TextEdit {
    path: String,
    start_byte: usize,
    end_byte: usize,
    text: String,
}
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RawEdit {
    path: String,
    start_byte: usize,
    end_byte: usize,
    text: String,
    ordinal: usize,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Effect {
    schema: u32,
    normalization: String,
    #[serde(default)]
    repairs: Vec<repairs::Repair>,
    #[serde(default)]
    composition: Option<String>,
    config: String,
    context_id: String,
    action_kind: String,
    before: Observations,
    after: Observations,
    raw_edits: Vec<RawEdit>,
    edits: Vec<TextEdit>,
    result_hashes: BTreeMap<String, String>,
    before_diagnostics: Vec<Value>,
    after_diagnostics: Vec<Value>,
}
type Contents = BTreeMap<String, String>;
type Key = (String, usize, usize);

fn normalize(raw: &[RawEdit], before: &Contents, d: &Deadline) -> Result<Vec<TextEdit>> {
    ensure!(
        !raw.is_empty() && raw.len() <= 2000,
        "Move raw edit capacity exceeded"
    );
    let mut result: Vec<TextEdit> = Vec::new();
    let mut seen = BTreeSet::new();
    let mut current = String::new();
    let mut ordinal = 0;
    for edit in raw {
        d.check()?;
        if edit.path != current {
            ensure!(
                seen.insert(edit.path.clone()),
                "Move repeated document batch"
            );
            current = edit.path.clone();
            ordinal = 0;
        }
        ensure!(edit.ordinal == ordinal, "Move edit ordinal mismatch");
        ordinal += 1;
        let text = before
            .get(&edit.path)
            .context("Move edited path outside capture")?;
        crate::paths::utf8_range(text, edit.start_byte, edit.end_byte)?;
        ensure!(
            edit.text.len() <= 4_000_000 && !edit.text.contains('\0'),
            "Move replacement text capacity/NUL refusal"
        );
        if let Some(previous) = result
            .last_mut()
            .filter(|previous| previous.path == edit.path)
        {
            if previous.start_byte == edit.start_byte {
                ensure!(
                    previous.start_byte == previous.end_byte,
                    "Move ambiguous same-offset edit order"
                );
                ensure!(
                    previous.text.len().saturating_add(edit.text.len()) <= 4_000_000,
                    "Move coalesced insertion capacity exceeded"
                );
                previous.end_byte = edit.end_byte;
                previous.text.push_str(&edit.text);
                continue;
            }
            ensure!(
                previous.end_byte <= edit.start_byte,
                "Move overlapping or unordered edits"
            );
        }
        result.push(TextEdit {
            path: edit.path.clone(),
            start_byte: edit.start_byte,
            end_byte: edit.end_byte,
            text: edit.text.clone(),
        });
    }
    // The compiler may delete an original leading BOM with its first declaration
    // or insert imports before it. Keep that original marker outside its edits.
    let mut admitted: Vec<TextEdit> = Vec::new();
    for mut edit in result {
        if before[&edit.path].starts_with('\u{feff}') && edit.start_byte == 0 {
            edit.start_byte = 3;
            edit.end_byte = edit.end_byte.max(3);
        }
        if let Some(previous) = admitted
            .last_mut()
            .filter(|p| p.path == edit.path && p.start_byte == edit.start_byte)
        {
            ensure!(
                previous.start_byte == previous.end_byte,
                "Move BOM normalization overlaps replacement"
            );
            ensure!(
                previous.text.len().saturating_add(edit.text.len()) <= 4_000_000,
                "Move BOM normalization capacity exceeded"
            );
            previous.end_byte = edit.end_byte;
            previous.text.push_str(&edit.text);
        } else {
            admitted.push(edit);
        }
    }
    Ok(admitted)
}
fn apply(edits: &[TextEdit], before: &Contents, d: &Deadline) -> Result<Contents> {
    let mut after = before.clone();
    let mut bytes = 0usize;
    let paths: BTreeSet<_> = edits.iter().map(|edit| edit.path.as_str()).collect();
    for path in paths {
        let text = after.get_mut(path).context("Move result file absent")?;
        for edit in edits.iter().filter(|edit| edit.path == path).rev() {
            d.check()?;
            crate::paths::utf8_range(text, edit.start_byte, edit.end_byte)?;
            ensure!(
                text.len()
                    .saturating_sub(edit.end_byte - edit.start_byte)
                    .saturating_add(edit.text.len())
                    <= 4_000_000,
                "Move result file capacity exceeded"
            );
            text.replace_range(edit.start_byte..edit.end_byte, &edit.text);
        }
        bytes = bytes.saturating_add(text.len());
        ensure!(bytes <= 16_000_000, "Move changed result capacity exceeded");
    }
    Ok(after)
}

pub fn prepare(
    engine: &mut Engine,
    spec: &CompilerSpec,
    args: &Value,
    d: &Deadline,
) -> Result<Value> {
    d.check()?;
    ensure!(
        args.as_object().is_some_and(|v| v.keys().all(|key| [
            "snapshotId",
            "relativePath",
            "namePath",
            "targetRelativePath",
            "targetParentNamePath"
        ]
        .contains(&key.as_str())))
            && serde_json::to_vec(args)?.len() <= 64_000,
        "Invalid compiler move arguments"
    );
    ensure!(
        args.get("targetParentNamePath").is_none_or(Value::is_null),
        "Compiler Move to file has no symbol-parent destination"
    );
    let path = args["relativePath"]
        .as_str()
        .context("Move source path required")?;
    let destination = args["targetRelativePath"]
        .as_str()
        .context("Move destination path required")?;
    let name = args["namePath"]
        .as_str()
        .filter(|v| !v.is_empty())
        .context("Move symbol selector required")?;
    for p in [path, destination] {
        ensure!(
            crate::paths::normalized(p)? == p && !crate::paths::protected(p),
            "Move requires canonical unprotected file paths"
        );
    }
    ensure!(
        path != destination,
        "Move source and destination must differ"
    );
    let host_binding = crate::compiler_inline::host_binding(spec, d)?;
    for tool in [&spec.node, &spec.typescript] {
        ensure!(
            !tool
                .canonicalize()?
                .starts_with(engine.root.canonicalize()?),
            "Move compiler must be outside analyzed workspace"
        );
    }
    let (snapshot, graph, _, _) = engine.index(d)?;
    crate::source_encoding::require_legacy(
        &engine.store,
        &snapshot,
        "compiler move raw-piece proof",
    )?;
    ensure!(
        args.get("snapshotId").is_none_or(|id| id == &snapshot.id),
        "Move source snapshot is stale"
    );
    let scope = crate::capture::source_scope_policy(&engine.store, &snapshot)?;
    for p in [path, destination] {
        scope.check_host_write_scope(p)?;
        let file = scope.file(p)?;
        ensure!(
            matches!(
                file.language.as_str(),
                "typescript" | "tsx" | "javascript" | "jsx"
            ) && file.size <= 4_000_000,
            "Move requires captured writable TS/JS files within capacity"
        );
    }
    let epoch =
        crate::capture::analyzer_epoch_for_snapshot_with_deadline(&engine.store, &snapshot, d)?;
    crate::query_features::source_scope(&engine.store, &snapshot, &graph, d)?;
    ensure!(
        snapshot.files.len() <= 20_000,
        "Move captured file capacity exceeded"
    );
    let mut before = Contents::new();
    let mut bytes = 0usize;
    for file in &snapshot.files {
        d.check()?;
        bytes = bytes.saturating_add(file.size);
        ensure!(bytes <= 10_000_000, "Move captured input capacity exceeded");
        before.insert(
            file.path.clone(),
            String::from_utf8(
                engine
                    .store
                    .read_blob_exact(&file.hash, file.size, 10_000_000)?,
            )?,
        );
    }
    let language = scope.file(path)?.language.clone();
    let source_tree = ast::parse(&language, &before[path], d)?;
    let symbol = if let Some(selected) = ast::named_statement(&source_tree, &before[path], name, d)?
    {
        if let Some(entity) = graph.entities.iter().find(|entity| {
            entity.path == path
                && entity.start_byte == selected.node.start_byte()
                && entity.end_byte == selected.node.end_byte()
                && entity.name == name
        }) {
            Declaration::from_graph(entity.clone())
        } else {
            ensure!(
                matches!(
                    selected.node.kind(),
                    "variable_declarator" | "internal_module"
                ),
                "Move declaration requires captured graph binding or explicit variable/namespace AST ownership"
            );
            let anchor = selected
                .node
                .child_by_field_name("name")
                .context("Move variable name absent")?;
            ensure!(
                anchor.kind() == "identifier",
                "Move requires one plain variable binding"
            );
            Declaration {
                graph_entity: None,
                name: name.into(),
                path: path.into(),
                language: language.clone(),
                start_byte: selected.node.start_byte(),
                end_byte: selected.node.end_byte(),
                name_start_byte: Some(anchor.start_byte()),
                name_end_byte: Some(anchor.end_byte()),
            }
        }
    } else {
        Declaration::from_graph(crate::agent_repl::source_symbol(
            &engine.store,
            &snapshot,
            &graph,
            name,
            path,
            d,
        )?)
    };
    if let Some(entity) = &symbol.graph_entity {
        ensure!(
            entity.producer == epoch.producer() && entity.language == language,
            "Move selected graph producer/language mismatch"
        );
    }
    let selected = ast::selection(&source_tree, &symbol, &before[path], epoch, d)?;
    let target_language = &scope.file(destination)?.language;
    let target_tree = ast::parse(target_language, &before[destination], d)?;
    ensure!(
        ast::named_statement(&target_tree, &before[destination], &symbol.name, d)?.is_none(),
        "Move destination already declares the selected name"
    );
    ensure!(
        ast::directives(&source_tree, &before[path], d)?
            == ast::directives(&target_tree, &before[destination], d)?,
        "Move source/destination directives differ"
    );
    let intent = json!({"path":path,"destination":destination,"name":symbol.name,"entityId":symbol.graph_entity.as_ref().map(|entity|&entity.id),"sourceSelectionOrigin":if symbol.graph_entity.is_some(){"captured_graph_and_exact_ast"}else{"independent_captured_variable_or_namespace_v1"},"startByte":selected.statement.start_byte(),"endByte":selected.statement.end_byte(),"nameStartByte":symbol.name_start_byte,"nameEndByte":symbol.name_end_byte});
    let report = crate::projects::execute(
        &engine.store,
        &snapshot,
        spec,
        "move",
        Some(intent.clone()),
        d,
    )?;
    ensure!(
        crate::compiler_inline::host_binding(spec, d)? == host_binding,
        "Move compiler host binding changed during execution"
    );
    ensure!(
        report["compilerHash"]
            == crate::compiler_inline::host_file(&spec.typescript, 16_000_000, d)?.1,
        "Move compiler module hash mismatch"
    );
    let effect: Effect = crate::json::decode(report["move"].clone())?;
    ensure!(
        effect.schema == 1
            && effect.normalization == "ordered_insertions_and_original_bom_v1"
            && effect.action_kind == "refactor.move.file"
            && crate::model::valid_id(&effect.context_id)
            && effect.config.len() <= 4096,
        "Move response intent/context mismatch"
    );
    ensure!(
        effect.config == "[inferred]"
            || before.contains_key(&effect.config) && effect.config.ends_with(".json"),
        "Move compiler configuration is outside captured source"
    );
    let original_edits = normalize(&effect.raw_edits, &before, d)?;
    let (edits, after) = repairs::admit(
        &before,
        &original_edits,
        &effect,
        &snapshot,
        &symbol,
        destination,
        d,
    )?;
    ensure!(
        edits == effect.edits,
        "Move normalized operations differ from original compiler changes"
    );
    let changed: BTreeSet<_> = edits.iter().map(|edit| edit.path.as_str()).collect();
    ensure!(
        changed.contains(path)
            && changed.contains(destination)
            && changed.len() == effect.result_hashes.len(),
        "Move effect does not change the selected file pair"
    );
    for p in &changed {
        scope.check_host_write_scope(p)?;
        let file = scope.file(p)?;
        ensure!(
            matches!(
                file.language.as_str(),
                "typescript" | "tsx" | "javascript" | "jsx"
            ),
            "Move cannot edit configuration or non-code resources"
        );
        ensure!(
            effect.result_hashes.get(*p) == Some(&crate::model::hash(&after[*p]))
                && before[*p] != after[*p],
            "Move result hash/no-op mismatch"
        );
    }
    let proof = bindings::check(
        engine,
        spec,
        &snapshot,
        &symbol,
        destination,
        &before,
        &after,
        &effect,
        &report,
        d,
    )?;
    let native_edits: Vec<_> = edits
        .iter()
        .map(|edit| changes::Edit::Replace {
            path: edit.path.clone(),
            expected_hash: snapshot
                .files
                .iter()
                .find(|f| f.path == edit.path)
                .expect("checked captured path")
                .hash
                .clone(),
            start_byte: edit.start_byte,
            end_byte: edit.end_byte,
            text: edit.text.clone(),
        })
        .collect();
    let plan = changes::prepare(
        &mut engine.store,
        &snapshot,
        &graph,
        &json!({"edits":native_edits}),
        d,
    )?;
    let mut result = json!({"snapshotId":snapshot.id,"plan":plan,"intent":intent,"provider":"host_selected_typescript_compiler","compiler":"5.8.3","hostBinding":host_binding,"compilerHash":report["compilerHash"],"compilerContextId":effect.context_id,"sourceBindings":report["sourceBindings"],"standardLibraryHashes":report["standardLibraryHashes"],"effectDigest":identity(&report["move"]),"bindingCheck":proof,"limitations":report["limitations"],"equivalent":"independent_compiler_move_to_existing_file","complete":false,"commandsExecuted":false,"sourceExecuted":false,"sourceTreeWritten":false,"verificationRequired":true});
    if !effect.repairs.is_empty() {
        result["sourceRepairs"] = serde_json::to_value(&effect.repairs)?;
        result["editComposition"] = json!(effect.composition);
    }
    Ok(crate::source_line_endings::qualify_provider_result(
        &plan, result,
    ))
}
