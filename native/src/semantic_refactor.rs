//! Independent source-bound semantic refactoring equivalents.
use crate::{
    Engine,
    capture::Deadline,
    changes, lsp,
    model::{Entity, Graph, Snapshot},
};
use anyhow::{Context, Result, bail, ensure};
use serde_json::{Value, json};
#[path = "semantic_directory_move.rs"]
mod directory_moves;
#[path = "semantic_file_delete.rs"]
mod file_delete;
#[path = "semantic_refactor_intents.rs"]
mod intents;
#[path = "semantic_move_intents.rs"]
mod move_intents;

pub fn invoke(
    engine: &mut Engine,
    operation: &str,
    args: &Value,
    deadline: &Deadline,
) -> Result<Value> {
    deadline.check()?;
    let allowed: &[&str] = match operation {
        "safeDelete" => &[
            "snapshotId",
            "namePath",
            "relativePath",
            "deleteEvenIfUsed",
            "propagate",
        ],
        "inline" => &["snapshotId", "namePath", "relativePath", "keepDefinition"],
        "rename" => &[
            "snapshotId",
            "namePath",
            "relativePath",
            "newName",
            "renameInComments",
            "renameInTextOccurrences",
        ],
        "move" => &[
            "snapshotId",
            "namePath",
            "relativePath",
            "targetRelativePath",
            "targetParentNamePath",
        ],
        "inspections" => &[
            "snapshotId",
            "relativePath",
            "minSeverity",
            "inspectionNames",
            "startLine",
            "endLine",
        ],
        "listInspections" => &["snapshotId", "language", "groupContains"],
        _ => bail!("Unknown semantic refactoring operation"),
    };
    ensure!(
        args.as_object()
            .is_some_and(|object| object.keys().all(|key| allowed.contains(&key.as_str()))),
        "Unknown semantic refactoring argument"
    );
    ensure!(
        serde_json::to_vec(args)?.len() <= 64000,
        "Semantic refactoring input capacity exceeded"
    );
    for key in [
        "deleteEvenIfUsed",
        "propagate",
        "keepDefinition",
        "renameInComments",
        "renameInTextOccurrences",
    ] {
        ensure!(
            args.get(key).is_none_or(Value::is_boolean),
            "Invalid semantic refactoring flag"
        );
    }
    let (snapshot, graph, _, _) = engine.index(deadline)?;
    ensure!(
        args.get("snapshotId").is_none_or(|id| id == &snapshot.id),
        "Refactoring source snapshot is stale"
    );
    if operation == "listInspections" {
        ensure!(
            args.get("language")
                .is_none_or(|value| value.is_null() || value.is_string())
                && args.get("groupContains").is_none_or(Value::is_string),
            "Invalid inspection registry filters"
        );
        let filter = args["language"].as_str();
        let group = args["groupContains"].as_str().unwrap_or("");
        ensure!(group.len() <= 200, "Inspection group capacity exceeded");
        let mut items = vec![];
        let mut seen = std::collections::BTreeSet::new();
        for file in &snapshot.files {
            deadline.check()?;
            if filter.is_some_and(|language| language != file.language)
                || !engine.servers.contains_key(&file.language)
                || !seen.insert(file.language.clone())
            {
                continue;
            }
            let entity = graph
                .entities
                .iter()
                .find(|entity| entity.path == file.path && entity.kind == "file")
                .context("Inspection registry source file absent")?;
            let capabilities = provider(
                engine,
                &snapshot,
                &graph,
                entity,
                "capabilities",
                &json!({}),
                deadline,
            )?;
            for kind in capabilities["codeActionKinds"]
                .as_array()
                .context("Declared provider kinds missing")?
            {
                let kind = kind.as_str().context("Provider kind must be text")?;
                if kind.contains(group) {
                    items.push(json!({"language":file.language,"kind":kind,"producer":capabilities["producer"],"serverProfileHash":capabilities["serverProfileHash"],"snapshotId":snapshot.id,"path":file.path,"contentHash":file.hash}));
                }
            }
        }
        return Ok(
            json!({"snapshotId":snapshot.id,"items":items,"provider":"host_selected_lsp","registrySemantics":"standard_lsp_declared_code_action_kinds_not_proprietary_inspection_catalog","complete":false}),
        );
    }
    if operation == "inspections" {
        let path = args["relativePath"]
            .as_str()
            .context("Inspection source required")?;
        let entity = graph
            .entities
            .iter()
            .find(|entity| entity.path == path && entity.kind == "file")
            .context("Inspection source is not a captured file")?;
        let min = args
            .get("minSeverity")
            .map(|value| {
                value
                    .as_u64()
                    .filter(|n| (1..=4).contains(n))
                    .context("Invalid inspection severity")
            })
            .transpose()?
            .unwrap_or(3);
        let names = args
            .get("inspectionNames")
            .map(|value| {
                value
                    .as_array()
                    .filter(|names| {
                        names.len() <= 100
                            && names
                                .iter()
                                .all(|name| name.as_str().is_some_and(|name| name.len() <= 200))
                    })
                    .context("Invalid inspection name filters")
            })
            .transpose()?;
        let (text, _) = crate::query::source(&engine.store, &snapshot, path)?;
        let starts = crate::paths::line_starts(&text);
        let first = args
            .get("startLine")
            .map(|value| value.as_u64().context("Invalid inspection start line"))
            .transpose()?
            .unwrap_or(0) as usize;
        let last = args
            .get("endLine")
            .map(|value| {
                value
                    .as_i64()
                    .filter(|n| *n >= -1)
                    .context("Invalid inspection end line")
            })
            .transpose()?
            .unwrap_or(-1);
        let last = if last < 0 {
            starts.len() - 1
        } else {
            last as usize
        };
        ensure!(
            first <= last && last < starts.len(),
            "Inspection line range outside captured source"
        );
        let begin = starts[first];
        let end = starts.get(last + 1).copied().unwrap_or(text.len());
        let mut value = provider(
            engine,
            &snapshot,
            &graph,
            entity,
            "diagnostics",
            &json!({"minSeverity":min,"maxItems":2000}),
            deadline,
        )?;
        let items: Vec<_> = value["items"]
            .as_array()
            .context("Provider diagnostics missing")?
            .iter()
            .filter(|item| {
                item["startByte"].as_u64().is_some_and(|at| {
                    at as usize >= begin
                        && ((at as usize) < end
                            || last + 1 == starts.len()
                                && at as usize == end
                                && item["endByte"].as_u64() == Some(at))
                }) && names.is_none_or(|names| {
                    names.is_empty()
                        || names.iter().any(|name| {
                            name == &item["code"]
                                || name.as_str()
                                    == Some(&format!(
                                        "{}:{}",
                                        item["source"].as_str().unwrap_or(""),
                                        item["code"].as_str().unwrap_or("")
                                    ))
                        })
                })
            })
            .cloned()
            .collect();
        value["items"] = json!(items);
        value["provider"] = json!("host_selected_lsp");
        value["equivalent"] = json!("registered_provider_diagnostics_not_proprietary_inspections");
        return Ok(value);
    }
    if operation == "move" && args.get("namePath").is_none_or(Value::is_null) {
        ensure!(
            args.get("targetParentNamePath").is_none(),
            "File move does not accept a destination symbol parent"
        );
        let path = args["relativePath"]
            .as_str()
            .context("File move source required")?;
        let destination = args["targetRelativePath"]
            .as_str()
            .context("File move destination required")?;
        let host_path = crate::paths::scoped(&engine.root, path)?;
        match std::fs::symlink_metadata(host_path) {
            Ok(metadata) if metadata.is_dir() && !crate::paths::is_link(&metadata) => {
                return directory_moves::prepare(
                    engine,
                    &snapshot,
                    &graph,
                    path,
                    destination,
                    deadline,
                );
            }
            Err(error) if error.kind() != std::io::ErrorKind::NotFound => return Err(error.into()),
            _ => {}
        }
        let file = graph
            .entities
            .iter()
            .find(|entity| entity.path == path && entity.kind == "file")
            .context("Move source is not a captured file")?;
        let value = provider(
            engine,
            &snapshot,
            &graph,
            file,
            "willRenameFiles",
            &json!({"renamePath":destination}),
            deadline,
        )?;
        return Ok(
            json!({"snapshotId":snapshot.id,"plan":value["plan"],"serverProfileHash":value["serverProfileHash"],"provider":"host_selected_lsp","equivalent":"independent_will_rename_files","complete":false,"commandsExecuted":false}),
        );
    }
    if operation == "move" {
        let parent = args
            .get("targetParentNamePath")
            .filter(|value| !value.is_null())
            .map(|value| {
                value
                    .as_str()
                    .context("Symbol move destination parent must be text")
            })
            .transpose()?;
        let intent = crate::refactor_contract::CheckedMoveIntent::resolve(
            engine,
            &snapshot,
            &graph,
            args["relativePath"]
                .as_str()
                .context("Symbol move source path required")?,
            args["namePath"]
                .as_str()
                .context("Symbol move source selector required")?,
            args["targetRelativePath"]
                .as_str()
                .context("Symbol move captured destination file required")?,
            parent,
            deadline,
        )?;
        let wire_intent = intent.wire_intent(deadline)?;
        let server = engine
            .servers
            .get("rust")
            .cloned()
            .context("No host-registered semantic provider for captured Rust source")?;
        let configuration = engine
            .server_configurations
            .get("rust")
            .cloned()
            .context("Symbol move requires an explicit owned host profile")?;
        let mut staged = lsp::stage_move_intent_at(
            engine,
            (&server, &configuration),
            &snapshot,
            &graph,
            &intent,
            deadline,
        )?;
        ensure!(
            staged.edits.len() == 1,
            "Symbol move requires exactly one unpublished provider edit"
        );
        let checked = staged.edits.pop().context("Symbol move edit absent")?;
        let effect =
            move_intents::check_move_effect(engine, &snapshot, &graph, &intent, checked, deadline)?;
        staged
            .value
            .as_object_mut()
            .context("Symbol move provider result absent")?
            .remove("stagedEditIndex");
        let plan = effect.prepare(engine, &snapshot, &graph, deadline)?;
        staged.value["plan"] = json!(plan);
        staged.value["intent"] = wire_intent;
        staged.value["equivalent"] = json!("independent_source_bound_owned_symbol_move");
        staged.value["referenceCompleteness"] = json!(
            "exact_declaration_and_ast_placement_only_not_exhaustive_compiler_reference_binding"
        );
        staged.value["complete"] = json!(false);
        staged.value["commandsExecuted"] = json!(false);
        return Ok(staged.value);
    }
    if operation == "safeDelete" && args.get("namePath").is_none_or(Value::is_null) {
        return file_delete::prepare(engine, &snapshot, &graph, args, deadline);
    }
    let symbol = symbol(engine, &snapshot, &graph, args, deadline)?;
    match operation {
        "rename" => {
            let new_name = args["newName"].as_str().context("Rename name required")?;
            let comments = args["renameInComments"] == true;
            let text = args["renameInTextOccurrences"] == true;
            let intent = intents::RefactorIntent::Rename {
                source: intents::SourceTarget::checked(
                    engine,
                    &snapshot,
                    &symbol,
                    args["namePath"]
                        .as_str()
                        .context("Source name path required")?,
                )?,
                new_name: new_name.into(),
                rename_in_comments: comments,
                rename_in_text_occurrences: text,
            };
            let mut staged = stage_provider(
                engine,
                &snapshot,
                &graph,
                &symbol,
                ("rename", Some(new_name), &json!({})),
                deadline,
            )?;
            ensure!(
                staged.edits.len() == 1,
                "Rename intent requires one explicit provider WorkspaceEdit"
            );
            let mut checked = staged.edits.pop().context("Rename edit absent")?;
            let provider_digest = intents::operations_digest(
                &snapshot,
                &staged.value["serverProfileHash"],
                &checked.operations,
            );
            let projection = intents::rename_occurrences(
                engine,
                &snapshot,
                &symbol,
                new_name,
                (comments, text),
                &mut checked,
                deadline,
            )?;
            let projected_digest = intents::operations_digest(
                &snapshot,
                &staged.value["serverProfileHash"],
                &checked.operations,
            );
            let plan = checked.prepare(engine, &snapshot, &graph, deadline)?;
            staged
                .value
                .as_object_mut()
                .context("Rename provider result missing")?
                .remove("stagedEditIndex");
            staged.value["plan"] = json!(plan);
            staged.value["intent"] = serde_json::to_value(intent)?;
            staged.value["intentProjection"] = projection;
            staged.value["providerOperationDigest"] = json!(provider_digest);
            staged.value["projectedOperationDigest"] = json!(projected_digest);
            staged.value["operationDigestSemantics"] = json!(
                "source_and_server_bound_audit_correlation_not_provider_attestation_or_replay"
            );
            staged.value["commandsExecuted"] = json!(false);
            Ok(staged.value)
        }
        "safeDelete" => {
            let forced = args["deleteEvenIfUsed"] == true;
            ensure!(
                args["propagate"] != true,
                "Propagating semantic deletion requires a provider refactoring action"
            );
            let observations: Vec<_> = graph
                .observations
                .iter()
                .filter(|observation| {
                    observation.relation != "contains"
                        && (observation.target.as_deref() == Some(&symbol.id)
                            || observation.target_label == symbol.name
                            || observation.target_label.rsplit(['.', ':', '/']).next()
                                == Some(symbol.name.as_str()))
                })
                .take(2001)
                .collect();
            ensure!(
                forced || observations.is_empty(),
                "Captured exact, candidate, or unresolved references prevent safe deletion"
            );
            ensure!(
                observations.len() <= 2000,
                "Semantic deletion usage proof exceeds bounded source scope"
            );
            let references = provider(
                engine,
                &snapshot,
                &graph,
                &symbol,
                "references",
                &json!({"includeDeclaration":false,"maxItems":2000}),
                deadline,
            )?;
            let locations = references["locations"]
                .as_array()
                .context("Registered provider references missing")?;
            ensure!(
                forced || locations.is_empty(),
                "Registered provider reports references to the selected symbol"
            );
            ensure!(
                locations.len() <= 2000 && references["truncated"] != true,
                "Semantic deletion provider usage proof exceeds bounded source scope"
            );
            let mut source_bindings = std::collections::BTreeMap::new();
            let mut source_cache = std::collections::BTreeMap::new();
            let mut source_bytes = 0usize;
            let mut needed = std::collections::BTreeSet::new();
            for observation in &observations {
                deadline.check()?;
                for id in &observation.evidence_ids {
                    ensure!(
                        needed.len() < 6000 || needed.contains(id.as_str()),
                        "Deletion usage evidence budget exceeded"
                    );
                    needed.insert(id.as_str());
                }
            }
            let mut evidence = std::collections::BTreeMap::new();
            if !needed.is_empty() {
                for (index, item) in graph.evidence.iter().enumerate() {
                    if index.is_multiple_of(64) {
                        deadline.check()?;
                    }
                    if needed.contains(item.id.as_str()) {
                        ensure!(
                            evidence.insert(item.id.as_str(), item).is_none(),
                            "Ambiguous deletion usage evidence identity"
                        );
                    }
                }
            }
            for observation in &observations {
                deadline.check()?;
                ensure!(
                    observation.snapshot_id == snapshot.id && !observation.evidence_ids.is_empty(),
                    "Deletion usage observation source proof missing"
                );
                for id in &observation.evidence_ids {
                    deadline.check()?;
                    let item = evidence
                        .get(id.as_str())
                        .context("Deletion usage evidence missing")?;
                    ensure!(
                        item.snapshot_id == snapshot.id,
                        "Deletion usage evidence snapshot mismatch"
                    );
                    deletion_usage_source(
                        engine,
                        &snapshot,
                        &item.path,
                        &item.content_hash,
                        item.start_byte,
                        item.end_byte,
                        &mut source_cache,
                        &mut source_bytes,
                        &mut source_bindings,
                    )?;
                }
            }
            for location in locations {
                deadline.check()?;
                deletion_usage_source(
                    engine,
                    &snapshot,
                    location["path"]
                        .as_str()
                        .context("Deletion usage path missing")?,
                    location["contentHash"]
                        .as_str()
                        .context("Deletion usage hash missing")?,
                    location["startByte"]
                        .as_u64()
                        .context("Deletion usage start missing")? as usize,
                    location["endByte"]
                        .as_u64()
                        .context("Deletion usage end missing")? as usize,
                    &mut source_cache,
                    &mut source_bytes,
                    &mut source_bindings,
                )?;
            }
            let plan = changes::prepare(
                &mut engine.store,
                &snapshot,
                &graph,
                &json!({"deleteSymbol":{"entityId":symbol.id}}),
                deadline,
            )?;
            Ok(
                json!({"snapshotId":snapshot.id,"plan":plan,"provider":"host_selected_lsp","equivalent":if forced{"independent_source_bound_forced_delete"}else{"independent_source_bound_safe_delete"},"forced":forced,"remainingUsagesMayBreak":forced && (!locations.is_empty() || !observations.is_empty()),"usageCheck":{"providerReferenceCount":locations.len(),"capturedReferenceCount":observations.len(),"sourceBindings":source_bindings,"serverProfileHash":references["serverProfileHash"],"exhaustiveCompilerProof":false},"complete":false,"commandsExecuted":false}),
            )
        }
        "inline" => {
            let keep = args["keepDefinition"] == true;
            let mut staged = stage_provider(
                engine,
                &snapshot,
                &graph,
                &symbol,
                (
                    "codeActions",
                    None,
                    &json!({"startByte":symbol.start_byte,"endByte":symbol.end_byte,"only":["refactor.inline"]}),
                ),
                deadline,
            )?;
            let mut actions: Vec<_> = staged.value["items"]
                .as_array()
                .context("Provider actions missing")?
                .iter()
                .filter(|action| {
                    action["usable"] == true
                        && action["kind"].as_str().is_some_and(|kind| {
                            kind == "refactor.inline" || kind.starts_with("refactor.inline.")
                        })
                })
                .cloned()
                .collect();
            ensure!(
                !actions.is_empty(),
                "Registered provider has no usable inline WorkspaceEdit"
            );
            let mut intent = Value::Null;
            let mut projection = Value::Null;
            let mut provider_digest = String::new();
            if keep {
                ensure!(
                    actions.len() == 1 && staged.edits.len() == 1,
                    "Keeping a definition requires one unambiguous provider candidate"
                );
                provider_digest = intents::operations_digest(
                    &snapshot,
                    &staged.value["serverProfileHash"],
                    &staged.edits[0].operations,
                );
                intent = serde_json::to_value(intents::RefactorIntent::Inline {
                    source: intents::SourceTarget::checked(
                        engine,
                        &snapshot,
                        &symbol,
                        args["namePath"]
                            .as_str()
                            .context("Source name path required")?,
                    )?,
                    keep_definition: true,
                })?;
                projection = intents::keep_definition(
                    engine,
                    &snapshot,
                    &graph,
                    &symbol,
                    &mut staged.edits[0],
                    deadline,
                )?;
            }
            // Candidate and intent admission finishes before the first plan.
            for action in &mut actions {
                let index = action["stagedEditIndex"]
                    .as_u64()
                    .context("Inline checked edit index absent")?
                    as usize;
                let checked = staged
                    .edits
                    .get(index)
                    .context("Inline checked edit index invalid")?;
                let projected_digest = if keep {
                    Some(intents::operations_digest(
                        &snapshot,
                        &staged.value["serverProfileHash"],
                        &checked.operations,
                    ))
                } else {
                    None
                };
                let plan = checked.prepare(engine, &snapshot, &graph, deadline)?;
                action
                    .as_object_mut()
                    .context("Inline action missing")?
                    .remove("stagedEditIndex");
                action["plan"] = json!(plan);
                if let Some(projected_digest) = projected_digest {
                    action["providerOperationDigest"] = json!(provider_digest);
                    action["projectedOperationDigest"] = json!(projected_digest);
                    action["operationDigestSemantics"] = json!(
                        "source_and_server_bound_audit_correlation_not_provider_attestation_or_replay"
                    );
                }
            }
            let mut result = json!({"snapshotId":snapshot.id,"actions":actions,"provider":"host_selected_lsp","serverProfileHash":staged.value["serverProfileHash"],"equivalent":"independent_provider_inline","complete":false,"commandsExecuted":false});
            if keep {
                result["intent"] = intent;
                result["intentProjection"] = projection;
            }
            Ok(result)
        }
        _ => bail!("Semantic refactoring operation is unavailable: {operation}"),
    }
}

#[allow(clippy::too_many_arguments)]
fn deletion_usage_source(
    engine: &Engine,
    snapshot: &Snapshot,
    path: &str,
    expected_hash: &str,
    start: usize,
    end: usize,
    cache: &mut std::collections::BTreeMap<String, String>,
    bytes: &mut usize,
    bindings: &mut std::collections::BTreeMap<String, String>,
) -> Result<()> {
    if !cache.contains_key(path) {
        let file = snapshot
            .files
            .iter()
            .find(|file| file.path == path)
            .context("Deletion usage outside captured manifest")?;
        ensure!(
            bytes.saturating_add(file.size) <= 16_000_000,
            "Deletion usage source byte budget exceeded"
        );
        let (source, hash) = crate::query::source(&engine.store, snapshot, path)?;
        *bytes += source.len();
        bindings.insert(path.into(), hash);
        cache.insert(path.into(), source);
    }
    ensure!(
        bindings.get(path).is_some_and(|hash| hash == expected_hash),
        "Deletion usage exact source hash mismatch"
    );
    crate::paths::utf8_range(&cache[path], start, end)?;
    Ok(())
}

fn symbol(
    engine: &Engine,
    snapshot: &Snapshot,
    graph: &Graph,
    args: &Value,
    deadline: &Deadline,
) -> Result<Entity> {
    let name = args["namePath"]
        .as_str()
        .filter(|name| !name.is_empty())
        .context("Symbol name path required")?;
    let path = args["relativePath"]
        .as_str()
        .context("Symbol source path required")?;
    crate::agent_repl::source_symbol(&engine.store, snapshot, graph, name, path, deadline)
}
fn provider(
    engine: &mut Engine,
    snapshot: &Snapshot,
    graph: &Graph,
    symbol: &Entity,
    operation: &str,
    options: &Value,
    deadline: &Deadline,
) -> Result<Value> {
    let language = crate::capture::validate_language(&engine.store, snapshot, &symbol.path)?;
    let server = engine
        .servers
        .get(&language)
        .cloned()
        .context("No host-registered semantic provider for captured parent language")?;
    let request = lsp::SemanticRequest {
        path: &symbol.path,
        offset: if matches!(
            operation,
            "willRenameFiles" | "diagnostics" | "capabilities"
        ) {
            0
        } else {
            symbol.name_start_byte.context("Symbol selection missing")?
        },
        operation,
        new_name: None,
    };
    if let Some(configuration) = engine.server_configurations.get(&language).cloned() {
        lsp::run_at_configured(
            engine,
            &lsp::ConfiguredServer {
                server,
                configuration,
            },
            snapshot,
            graph,
            request,
            options,
            deadline,
        )
    } else {
        lsp::run_at_with_options(engine, &server, snapshot, graph, request, options, deadline)
    }
}

fn stage_provider(
    engine: &mut Engine,
    snapshot: &Snapshot,
    graph: &Graph,
    symbol: &Entity,
    request: (&str, Option<&str>, &Value),
    deadline: &Deadline,
) -> Result<lsp::StagedSemantic> {
    let language = crate::capture::validate_language(&engine.store, snapshot, &symbol.path)?;
    let server = engine
        .servers
        .get(&language)
        .cloned()
        .context("No host-registered semantic provider for captured parent language")?;
    let configuration = engine.server_configurations.get(&language).cloned();
    lsp::stage_at(
        engine,
        (&server, configuration.as_ref()),
        snapshot,
        graph,
        lsp::SemanticRequest {
            path: &symbol.path,
            offset: symbol.name_start_byte.context("Symbol selection missing")?,
            operation: request.0,
            new_name: request.1,
        },
        request.2,
        deadline,
    )
}
