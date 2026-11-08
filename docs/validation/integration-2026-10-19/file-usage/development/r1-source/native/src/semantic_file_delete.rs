//! Conservative captured ES-module file usage admission, before one guarded deletion plan.
use super::provider;
use crate::{
    Engine, analysis,
    capture::Deadline,
    changes,
    model::{Entity, Graph, Snapshot},
};
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

const CAPACITY: &str = "file_usage_proof_capacity";
const UNSUPPORTED: &str = "Safe file usage proof requires a captured TypeScript/JavaScript ES module containing only named function declarations";
const MAX_FILES: usize = 2000;
const MAX_BYTES: usize = 16_000_000;
const MAX_DECLARATIONS: usize = 8;

fn provider_context(value: &Value) -> Result<Value> {
    ensure!(
        value["serverProfileHash"].is_string() && value["providerSourceOmissions"].is_array(),
        "File usage proof provider/source context absent"
    );
    Ok(
        json!({"serverProfileHash":value["serverProfileHash"],"serverProfileBinding":value["serverProfileBinding"],"providerSourceOmissions":value["providerSourceOmissions"],"providerSourcePolicy":value["providerSourcePolicy"],"sourceExecutionAuthorized":value["sourceExecutionAuthorized"]}),
    )
}

fn targets(
    graph: &Graph,
    language: &str,
    path: &str,
    source: &str,
    d: &Deadline,
) -> Result<Vec<Entity>> {
    ensure!(source.len() <= 256_000, CAPACITY);
    let mut parser = tree_sitter::Parser::new();
    parser.set_language(&analysis::grammar(language).context(UNSUPPORTED)?)?;
    let mut cancelled = |_: &tree_sitter::ParseState| d.check().is_err();
    let tree = parser
        .parse_with_options(
            &mut |offset, _| &source.as_bytes()[offset.min(source.len())..],
            None,
            Some(tree_sitter::ParseOptions::new().progress_callback(&mut cancelled)),
        )
        .context("File usage proof parser cancelled")?;
    d.check()?;
    ensure!(!tree.root_node().has_error(), UNSUPPORTED);
    let mut result = Vec::new();
    let mut exported = false;
    let mut cursor = tree.root_node().walk();
    for child in tree.root_node().named_children(&mut cursor) {
        d.check()?;
        if child.kind() == "comment" {
            continue;
        }
        let declaration = if child.kind() == "export_statement" {
            exported = true;
            child
                .child_by_field_name("declaration")
                .context(UNSUPPORTED)?
        } else {
            child
        };
        ensure!(
            declaration.kind() == "function_declaration"
                && declaration.child_by_field_name("body").is_some(),
            UNSUPPORTED
        );
        let name = declaration
            .child_by_field_name("name")
            .context(UNSUPPORTED)?;
        let matches: Vec<_> = graph
            .entities
            .iter()
            .filter(|entity| {
                entity.path == path
                    && entity.kind == "function"
                    && entity.start_byte == declaration.start_byte()
                    && entity.end_byte == declaration.end_byte()
                    && entity.name_start_byte == Some(name.start_byte())
                    && entity.name_end_byte == Some(name.end_byte())
                    && entity.name == source[name.byte_range()]
            })
            .take(2)
            .collect();
        ensure!(
            matches.len() == 1,
            "File usage proof declaration/source binding unavailable"
        );
        ensure!(result.len() < MAX_DECLARATIONS, CAPACITY);
        result.push((*matches[0]).clone());
    }
    ensure!(exported && !result.is_empty(), UNSUPPORTED);
    Ok(result)
}

pub(super) fn prepare(
    engine: &mut Engine,
    snapshot: &Snapshot,
    graph: &Graph,
    args: &Value,
    d: &Deadline,
) -> Result<Value> {
    d.check()?;
    ensure!(
        args["propagate"] != true,
        "File deletion propagation is unavailable"
    );
    ensure!(
        args["deleteEvenIfUsed"] != true,
        "Forced file deletion uses the explicit source-policy facade path"
    );
    let path = crate::paths::normalized(
        args["relativePath"]
            .as_str()
            .context("File deletion source required")?,
    )?;
    let file = {
        let scope = crate::capture::source_scope_policy(&engine.store, snapshot)?;
        let file = scope.file(&path).context(UNSUPPORTED)?;
        scope.check_host_write_scope(&path)?;
        file.clone()
    };
    ensure!(
        matches!(file.language.as_str(), "typescript" | "javascript")
            && (path.ends_with(".ts") || path.ends_with(".js")),
        UNSUPPORTED
    );
    let host = crate::paths::scoped(&engine.root, &path)?;
    let metadata = std::fs::symlink_metadata(&host)?;
    ensure!(
        metadata.is_file() && !crate::paths::is_link(&metadata),
        UNSUPPORTED
    );
    ensure!(
        snapshot.files.len() <= MAX_FILES && file.size <= 256_000,
        CAPACITY
    );
    let total = snapshot.files.iter().try_fold(0usize, |bytes, next| {
        d.check()?;
        let bytes = bytes.checked_add(next.size).context(CAPACITY)?;
        ensure!(bytes <= MAX_BYTES, CAPACITY);
        Ok::<_, anyhow::Error>(bytes)
    })?;
    ensure!(
        crate::verify::file_integrity(&file, &host, d)?,
        "File deletion source is stale"
    );
    let (source, hash) = crate::query::source(&engine.store, snapshot, &path)?;
    let declarations = targets(graph, &file.language, &path, &source, d)?;
    let server = engine
        .servers
        .get(&file.language)
        .context("File usage proof requires a host-registered semantic provider")?;
    ensure!(
        server.trust,
        "File usage proof requires explicit provider trust"
    );
    // Empty answers from a provider with no source binding cannot prove absence.
    // Captured path/name candidates are independently conservative denials.
    let ids: BTreeSet<_> = graph
        .entities
        .iter()
        .filter(|entity| entity.path == path)
        .map(|entity| entity.id.as_str())
        .collect();
    let entities: BTreeMap<_, _> = graph
        .entities
        .iter()
        .map(|entity| (entity.id.as_str(), entity))
        .collect();
    let basename = path.rsplit('/').next().context(UNSUPPORTED)?;
    let stem = basename.rsplit_once('.').context(UNSUPPORTED)?.0;
    let mut candidates: BTreeSet<String> = declarations
        .iter()
        .map(|entity| entity.name.to_lowercase())
        .collect();
    candidates.insert(stem.to_lowercase());
    candidates.insert(path.to_lowercase());
    candidates.insert(basename.to_lowercase());
    for observation in &graph.observations {
        d.check()?;
        if observation.relation == "contains" {
            continue;
        }
        let origin = entities
            .get(observation.source.as_str())
            .context("File usage observation origin missing")?;
        if origin.path == path {
            continue;
        }
        ensure!(
            !observation
                .target
                .as_deref()
                .is_some_and(|id| ids.contains(id))
                && !candidates.contains(&observation.target_label.to_lowercase()),
            "Captured exact, candidate or unresolved references prevent file deletion"
        );
    }
    let mut bindings = BTreeMap::new();
    for member in &snapshot.files {
        d.check()?;
        let (text, hash) = crate::query::source(&engine.store, snapshot, &member.path)?;
        bindings.insert(member.path.clone(), hash);
        if member.path == path {
            continue;
        }
        let lowered = text.to_lowercase();
        ensure!(
            !candidates
                .iter()
                .any(|candidate| lowered.contains(candidate)),
            "Captured path/name occurrence prevents file deletion"
        );
    }
    let mut checked = Vec::new();
    let mut context: Option<Value> = None;
    for declaration in &declarations {
        d.check()?;
        let definition = provider(
            engine,
            snapshot,
            graph,
            declaration,
            "definition",
            &json!({"maxItems":32}),
            d,
        )?;
        let definitions = definition["locations"]
            .as_array()
            .context("File usage proof definition capability unavailable")?;
        let start = declaration.name_start_byte.context(UNSUPPORTED)?;
        let end = declaration.name_end_byte.context(UNSUPPORTED)?;
        ensure!(
            definition["truncated"] != true
                && definitions.len() == 1
                && definitions[0]["path"] == path
                && definitions[0]["contentHash"] == hash
                && definitions[0]["startByte"] == start
                && definitions[0]["endByte"] == end
                && definitions[0]["text"] == declaration.name,
            "File usage proof requires one exact provider-bound declaration"
        );
        let references = provider(
            engine,
            snapshot,
            graph,
            declaration,
            "references",
            &json!({"includeDeclaration":true,"maxItems":2000}),
            d,
        )?;
        let locations = references["locations"]
            .as_array()
            .context("File usage proof reference capability unavailable")?;
        ensure!(
            references["truncated"] != true && !locations.is_empty() && locations.len() <= 2000,
            "File usage proof requires complete bounded responses including the selected declaration"
        );
        ensure!(
            locations.iter().any(|location| location["path"] == path
                && location["startByte"] == start
                && location["endByte"] == end),
            "File usage proof reference response omitted the selected declaration"
        );
        for location in locations {
            d.check()?;
            ensure!(
                location["path"] == path && location["contentHash"] == hash,
                "Registered provider reports an external file usage"
            );
            let begin = usize::try_from(
                location["startByte"]
                    .as_u64()
                    .context("File usage start missing")?,
            )?;
            let finish = usize::try_from(
                location["endByte"]
                    .as_u64()
                    .context("File usage end missing")?,
            )?;
            ensure!(
                crate::paths::utf8_range(&source, begin, finish)? == declaration.name
                    && location["text"] == declaration.name,
                "File usage proof reference/source binding mismatch"
            );
        }
        let bound = provider_context(&definition)?;
        ensure!(
            provider_context(&references)? == bound
                && context.as_ref().is_none_or(|previous| previous == &bound),
            "File usage proof provider/source policy changed"
        );
        context = Some(bound);
        checked.push(json!({"entityId":declaration.id,"path":path,"contentHash":hash,"name":declaration.name,"startByte":start,"endByte":end,"inFileReferenceCount":locations.len()}));
    }
    d.check()?;
    ensure!(
        crate::verify::file_integrity(&file, &host, d)?,
        "File deletion source became stale during usage checks"
    );
    let context = context.context("File usage proof provider context absent")?;
    let plan = changes::prepare(
        &mut engine.store,
        snapshot,
        graph,
        &json!({"edits":[{"kind":"delete","path":path,"expectedHash":hash}]}),
        d,
    )?;
    Ok(
        json!({"snapshotId":snapshot.id,"plan":plan,"sourceBinding":{"snapshotId":snapshot.id,"path":path,"contentHash":hash},"provider":"host_selected_lsp","equivalent":"independent_captured_es_module_file_usage_check","forced":false,"propagate":false,"referencesRewritten":false,"remainingUsagesMayBreak":true,"providerSourceOmissions":context["providerSourceOmissions"],"providerSourcePolicy":context["providerSourcePolicy"],"usageCheck":{"performed":true,"policy":"captured_es_module_functions_v1","declarations":checked,"providerRequestCount":declarations.len()*2,"serverProfileHash":context["serverProfileHash"],"providerContext":context,"sourceBindings":bindings,"sourceFileCount":snapshot.files.len(),"sourceBytes":total,"exhaustiveCompilerProof":false},"coverage":{"omissions":snapshot.omissions,"exhaustive":false,"limitations":["Only captured conservative path/name candidates and selected provider references are checked; dynamic, external or omitted consumers remain unknown."]},"complete":false,"commandsExecuted":false}),
    )
}
