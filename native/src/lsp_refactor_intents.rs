//! Private, host-selected transport for the owned refactor-intent protocol.
//! A checked response is still unpublished and requires move-effect admission.
use super::*;
use crate::refactor_contract::CheckedMoveIntent;

fn build_owned_request(
    base: &Value,
    wire_intent: &Value,
    documents: Value,
    deadline: &Deadline,
) -> Result<(String, Value)> {
    let request_id = identity(&("yoke-owned-refactor-intent-v1", base, wire_intent));
    let params = json!({
        "schemaVersion":1,"requestId":request_id,"base":base,"intent":wire_intent,
        "documents":documents,
    });
    bounded_host_json(&params, 4_000_000, deadline)?;
    Ok((request_id, params))
}

fn checked_capability(
    capabilities: &Value,
    destination: &str,
    source_attachments: bool,
    deadline: &Deadline,
) -> Result<()> {
    let capability = &capabilities["experimental"]["yokeRefactorIntent"];
    bounded_host_json(capability, 16_000, deadline)?;
    let object = capability
        .as_object()
        .context("Server does not advertise the owned refactor-intent capability")?;
    ensure!(
        (3..=4).contains(&object.len())
            && object.keys().all(|key| matches!(
                key.as_str(),
                "schemaVersion" | "operations" | "destinations" | "sourceAttachments"
            ))
            && object.contains_key("schemaVersion")
            && object.contains_key("operations")
            && object.contains_key("destinations")
            && capability["schemaVersion"].as_u64() == Some(1),
        "Unsupported owned refactor-intent capability schema"
    );
    if let Some(attachments) = object.get("sourceAttachments") {
        ensure!(
            attachments.as_str() == Some("exact_text_v1"),
            "Unsupported owned source-attachment capability"
        );
    }
    ensure!(
        !source_attachments || capability["sourceAttachments"] == "exact_text_v1",
        "Owned source-attachment transfer requires explicit provider capability"
    );
    for (key, allowed, requested) in [
        ("operations", &["moveSymbol"][..], "moveSymbol"),
        (
            "destinations",
            &["fileTopLevel", "parentSymbol"][..],
            destination,
        ),
    ] {
        let values = capability[key]
            .as_array()
            .context("Owned refactor-intent capability registry must be an array")?;
        let mut seen = HashSet::new();
        ensure!(
            !values.is_empty() && values.len() <= allowed.len(),
            "Owned refactor-intent capability registry exceeds its schema"
        );
        for value in values {
            deadline.check()?;
            let value = value
                .as_str()
                .context("Owned refactor-intent capability registry must contain strings")?;
            ensure!(
                allowed.contains(&value) && seen.insert(value),
                "Unsupported or repeated owned refactor-intent capability"
            );
        }
        ensure!(
            seen.contains(requested),
            "Server does not advertise the requested owned refactor intent"
        );
    }
    Ok(())
}

fn checked_response<'a>(
    response: &'a Value,
    request_id: &str,
    deadline: &Deadline,
) -> Result<&'a Value> {
    bounded_host_json(response, 4_000_000, deadline)?;
    let object = response
        .as_object()
        .context("Owned refactor-intent response must be an envelope")?;
    ensure!(
        object.len() == 3
            && response["schemaVersion"].as_u64() == Some(1)
            && response["requestId"].as_str() == Some(request_id)
            && object.contains_key("edit") != object.contains_key("unsupported"),
        "Owned refactor-intent response schema or request binding mismatch"
    );
    if let Some(unsupported) = object.get("unsupported") {
        let unsupported = unsupported
            .as_object()
            .context("Invalid owned refactor-intent unsupported response")?;
        ensure!(
            unsupported.len() == 2
                && unsupported
                    .get("code")
                    .and_then(Value::as_str)
                    .is_some_and(|code| !code.is_empty() && code.len() <= 200)
                && unsupported
                    .get("message")
                    .and_then(Value::as_str)
                    .is_some_and(|message| message.len() <= 2000),
            "Invalid owned refactor-intent unsupported response"
        );
        // Provider-controlled messages are not host diagnostics or instructions.
        bail!("Selected provider does not support this owned refactor intent");
    }
    let edit = object
        .get("edit")
        .filter(|edit| edit.is_object())
        .context("Owned refactor-intent response requires an explicit WorkspaceEdit")?;
    ensure!(
        edit.as_object().is_some_and(|edit| {
            edit.keys()
                .all(|key| matches!(key.as_str(), "changes" | "documentChanges"))
        }),
        "Unsupported owned refactor-intent WorkspaceEdit fields"
    );
    validate_owned_workspace_edit_shape(edit, deadline)?;
    Ok(edit)
}

fn owned_fields<'a>(
    value: &'a Value,
    allowed: &[&str],
    required: &[&str],
    deadline: &Deadline,
) -> Result<&'a serde_json::Map<String, Value>> {
    deadline.check()?;
    let object = value
        .as_object()
        .context("Owned WorkspaceEdit field must be an object")?;
    ensure!(
        object.keys().all(|key| allowed.contains(&key.as_str()))
            && required.iter().all(|key| object.contains_key(*key)),
        "Unknown or missing owned WorkspaceEdit field"
    );
    Ok(object)
}

fn owned_position(position: &Value, deadline: &Deadline) -> Result<()> {
    let position = owned_fields(
        position,
        &["line", "character"],
        &["line", "character"],
        deadline,
    )?;
    ensure!(
        position.values().all(|value| value
            .as_u64()
            .is_some_and(|number| number <= i32::MAX as u64)),
        "Owned WorkspaceEdit position must contain unsigned LSP integers"
    );
    Ok(())
}

fn owned_text_edits(edits: &Value, deadline: &Deadline) -> Result<()> {
    let edits = edits
        .as_array()
        .context("Owned WorkspaceEdit edits must be an array")?;
    ensure!(
        edits.len() <= 10000,
        "Owned WorkspaceEdit edit budget exceeded"
    );
    for edit in edits {
        let edit = owned_fields(
            edit,
            &["range", "newText", "annotationId"],
            &["range", "newText"],
            deadline,
        )?;
        ensure!(
            edit["newText"].is_string() && edit.get("annotationId").is_none_or(Value::is_string),
            "Invalid owned WorkspaceEdit text or annotation identifier"
        );
        let range = owned_fields(
            &edit["range"],
            &["start", "end"],
            &["start", "end"],
            deadline,
        )?;
        owned_position(&range["start"], deadline)?;
        owned_position(&range["end"], deadline)?;
    }
    Ok(())
}

fn owned_document_change(document: &Value, deadline: &Deadline) -> Result<()> {
    if let Some(kind) = document.get("kind") {
        let (fields, required, options): (&[&str], &[&str], &[&str]) = match kind.as_str() {
            Some("create") => (
                &["kind", "uri", "options", "annotationId"],
                &["kind", "uri"],
                &["overwrite", "ignoreIfExists"],
            ),
            Some("rename") => (
                &["kind", "oldUri", "newUri", "options", "annotationId"],
                &["kind", "oldUri", "newUri"],
                &["overwrite", "ignoreIfExists"],
            ),
            Some("delete") => (
                &["kind", "uri", "options", "annotationId"],
                &["kind", "uri"],
                &["recursive", "ignoreIfNotExists"],
            ),
            _ => bail!("Unsupported owned WorkspaceEdit resource operation"),
        };
        let document = owned_fields(document, fields, required, deadline)?;
        ensure!(
            required.iter().all(|key| document[*key].is_string())
                && document.get("annotationId").is_none_or(Value::is_string),
            "Invalid owned WorkspaceEdit resource binding"
        );
        if let Some(value) = document.get("options") {
            let options = owned_fields(value, options, &[], deadline)?;
            ensure!(
                options.values().all(Value::is_boolean),
                "Owned WorkspaceEdit resource options must be booleans"
            );
        }
    } else {
        let document = owned_fields(
            document,
            &["textDocument", "edits"],
            &["textDocument", "edits"],
            deadline,
        )?;
        let identifier = owned_fields(
            &document["textDocument"],
            &["uri", "version"],
            &["uri"],
            deadline,
        )?;
        ensure!(
            identifier["uri"].is_string()
                && identifier.get("version").is_none_or(|version| {
                    version.is_null()
                        || version
                            .as_i64()
                            .is_some_and(|number| i32::try_from(number).is_ok())
                }),
            "Invalid owned WorkspaceEdit text document binding"
        );
        owned_text_edits(&document["edits"], deadline)?;
    }
    Ok(())
}

fn validate_owned_workspace_edit_shape(edit: &Value, deadline: &Deadline) -> Result<()> {
    bounded_host_json(edit, 4_000_000, deadline)?;
    let edit = owned_fields(
        edit,
        &["changes", "documentChanges", "changeAnnotations"],
        &[],
        deadline,
    )?;
    ensure!(
        edit.contains_key("changes") != edit.contains_key("documentChanges"),
        "Ambiguous or missing owned WorkspaceEdit"
    );
    if let Some(changes) = edit.get("changes") {
        let changes = changes
            .as_object()
            .context("Owned WorkspaceEdit changes must be a URI map")?;
        ensure!(
            changes.len() <= 2000,
            "Owned WorkspaceEdit file budget exceeded"
        );
        for edits in changes.values() {
            owned_text_edits(edits, deadline)?;
        }
    }
    if let Some(documents) = edit.get("documentChanges") {
        let documents = documents
            .as_array()
            .context("Owned WorkspaceEdit documentChanges must be an array")?;
        ensure!(
            documents.len() <= 2000,
            "Owned WorkspaceEdit operation budget exceeded"
        );
        for document in documents {
            owned_document_change(document, deadline)?;
        }
    }
    if let Some(annotations) = edit.get("changeAnnotations") {
        let annotations = annotations
            .as_object()
            .context("Owned WorkspaceEdit annotations must be an identifier map")?;
        ensure!(
            annotations.len() <= 2000,
            "Owned WorkspaceEdit annotation budget exceeded"
        );
        for annotation in annotations.values() {
            let annotation = owned_fields(
                annotation,
                &["label", "needsConfirmation", "description"],
                &["label"],
                deadline,
            )?;
            ensure!(
                annotation["label"].is_string()
                    && annotation
                        .get("needsConfirmation")
                        .is_none_or(Value::is_boolean)
                    && annotation.get("description").is_none_or(Value::is_string),
                "Invalid owned WorkspaceEdit annotation fields"
            );
        }
    }
    deadline.check()
}

pub(crate) fn stage_move_intent_at(
    engine: &mut Engine,
    host: (&Server, &HostConfiguration),
    snapshot: &Snapshot,
    graph: &Graph,
    intent: &CheckedMoveIntent,
    deadline: &Deadline,
) -> Result<StagedSemantic> {
    deadline.check()?;
    let selected = engine.selected_provider(intent.source_path())?;
    let host = selected
        .as_ref()
        .map(|(_, p)| (&p.server, &p.configuration))
        .unwrap_or(host);
    ensure!(
        host.1.refactor_intent_profile == Some(RefactorIntentProfile::OwnV1),
        "Owned refactor intents require explicit host refactorIntentProfile own_v1"
    );
    ensure!(host.0.trust, "Language server trust is required");
    let configuration = effective_host_configuration(engine, snapshot, Some(host.1), deadline)?
        .context("Owned refactor intent host configuration missing")?;
    let host = (host.0, &configuration);
    validate_passive_host(host.0, Some(host.1), deadline)?;
    intent.validate_binding(snapshot, graph, deadline)?;
    engine.store.set_deadline(deadline)?;
    ensure!(
        snapshot.workspace_id == engine.store.workspace_id
            && identity(&engine.store.snapshot(&snapshot.id)?) == identity(snapshot),
        "Semantic source manifest mismatch"
    );
    ensure!(
        graph.snapshot_id == snapshot.id
            && identity(&engine.store.graph(&snapshot.id)?) == identity(graph),
        "Semantic graph manifest mismatch"
    );
    let epoch = crate::capture::analyzer_epoch_for_snapshot_with_deadline(
        &engine.store,
        snapshot,
        deadline,
    )?;
    let profile = server_profile(host.0, Some(host.1), deadline)?;
    let profile_hash =
        selected_profile_hash(&profile, selected.as_ref().map(|(id, _)| id.as_str()));
    let base = json!({
        "workspaceId":snapshot.workspace_id,"snapshotId":snapshot.id,
        "manifestHash":identity(snapshot),"analyzerProducer":epoch.producer(),
        "serverProfileHash":profile_hash,
    });
    let wire_intent = intent.wire_intent(deadline)?;
    bounded_host_json(&wire_intent, 64_000, deadline)?;
    let (source, _) = query::source(&engine.store, snapshot, intent.source_path())?;
    let source_file = snapshot
        .files
        .iter()
        .find(|file| file.path == intent.source_path())
        .context("Owned refactor source absent from snapshot")?;
    let destination_file = snapshot
        .files
        .iter()
        .find(|file| file.path == intent.destination_path())
        .context("Owned refactor destination absent from snapshot")?;
    let exclusions = passive_source_exclusions(host.0, Some(host.1), snapshot);
    ensure!(
        source_file.language == "rust"
            && destination_file.language == "rust"
            && host.0.language == "rust"
            && !exclusions.contains(intent.destination_path()),
        "Owned move requires captured Rust documents in the selected provider scope"
    );
    let InitializedProvider {
        tree,
        mut session,
        capabilities,
        document_uri,
        exclusions,
    } = initialize_provider_at(
        engine,
        (host.0, Some(host.1)),
        snapshot,
        intent.source_path(),
        &source,
        &source_file.language,
        deadline,
    )?;
    let response = (|| -> Result<(String, Value)> {
        checked_capability(
            &capabilities,
            intent.destination_kind(),
            !intent.attachments().is_empty(),
            deadline,
        )?;
        let destination_uri = uri(&scoped(tree.path(), intent.destination_path())?)?;
        let (request_id, params) = build_owned_request(
            &base,
            &wire_intent,
            json!({"sourceUri":document_uri,"destinationUri":destination_uri}),
            deadline,
        )?;
        let response = session.request("yoke/refactorIntent", params, deadline)?;
        Ok((request_id, response))
    })();
    let shutdown = session.shutdown(deadline);
    drop(session);
    // Verify after provider completion, while private URI mappings still exist.
    check_copy_verified(&engine.store, snapshot, tree.path(), &exclusions, deadline)?;
    let (request_id, response) = response?;
    shutdown?;
    let edit = checked_response(&response, &request_id, deadline)?;
    let checked = check_workspace_edit(engine, snapshot, graph, tree.path(), edit, deadline)?;
    intent.validate_binding(snapshot, graph, deadline)?;
    deadline.check()?;
    let mut value = json!({
        "snapshotId":snapshot.id,"path":intent.source_path(),"operation":"moveSymbol",
        "refactorIntentProfile":"own_v1","requestId":request_id,
        "serverProfileHash":profile_hash,"serverProfileBinding":profile["commandBinding"],
        "providerSourceOmissions":snapshot.files.iter().filter(|file| exclusions.contains(&file.path)).map(|file| json!({"path":file.path,"contentHash":file.hash,"reason":"workspace_execution_selector_omitted_from_private_provider_tree"})).collect::<Vec<_>>(),
        "providerSourcePolicy":provider_source_policy(host.0, Some(host.1), snapshot),
        "sourceExecutionAuthorized":host.1.allow_source_execution,
        "stagedEditIndex":0,"commandsExecuted":false,"complete":false,
        "referenceCompleteness":"compiler_binding_and_exhaustive_reference_updates_unverified",
    });
    if let Some((id, _)) = &selected {
        qualify_selection(&mut value, id, intent.source_path());
    }
    Ok(StagedSemantic {
        value,
        edits: vec![checked],
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn workspace_edit_shapes() -> Value {
        json!({
            "documentChanges":[
                {"textDocument":{"uri":"file:///private/source.rs","version":1},
                 "edits":[{"range":{"start":{"line":0,"character":0},"end":{"line":0,"character":1}},"newText":"a","annotationId":"review"}]},
                {"kind":"create","uri":"file:///private/new.rs","options":{"overwrite":false,"ignoreIfExists":true},"annotationId":"review"},
                {"kind":"rename","oldUri":"file:///private/a.rs","newUri":"file:///private/b.rs","options":{"overwrite":false,"ignoreIfExists":true},"annotationId":"review"},
                {"kind":"delete","uri":"file:///private/old.rs","options":{"recursive":false,"ignoreIfNotExists":true},"annotationId":"review"},
            ],
            "changeAnnotations":{"review":{"label":"Review edit","needsConfirmation":true,"description":"Captured declaration move"}},
        })
    }

    #[test]
    fn owned_nested_schema_recognizes_existing_edit_and_resource_field_shapes() {
        // Shape admission does not grant support or authority: the existing edit
        // checker still refuses annotations and B still refuses resource moves.
        validate_owned_workspace_edit_shape(&workspace_edit_shapes(), &Deadline::new(1000))
            .unwrap();
        validate_owned_workspace_edit_shape(
            &json!({"changes":{"file:///private/source.rs":[{"range":{"start":{"line":0,"character":0},"end":{"line":0,"character":1}},"newText":"a"}]}}),
            &Deadline::new(1000),
        ).unwrap();
    }

    #[test]
    fn owned_nested_schema_refuses_unknown_fields_at_each_supported_shape() {
        for pointer in [
            "",
            "/documentChanges/0",
            "/documentChanges/0/textDocument",
            "/documentChanges/0/edits/0",
            "/documentChanges/0/edits/0/range",
            "/documentChanges/0/edits/0/range/start",
            "/documentChanges/0/edits/0/range/end",
            "/documentChanges/1",
            "/documentChanges/1/options",
            "/documentChanges/2",
            "/documentChanges/2/options",
            "/documentChanges/3",
            "/documentChanges/3/options",
            "/changeAnnotations/review",
        ] {
            let mut edit = workspace_edit_shapes();
            edit.pointer_mut(pointer)
                .unwrap()
                .as_object_mut()
                .unwrap()
                .insert("unexpectedCommand".into(), json!("never execute"));
            assert!(
                validate_owned_workspace_edit_shape(&edit, &Deadline::new(1000)).is_err(),
                "Unknown owned WorkspaceEdit field accepted at {pointer}"
            );
        }
    }

    #[test]
    fn owned_response_applies_nested_schema_guard_before_returning_an_edit() {
        let mut response = json!({"schemaVersion":1,"requestId":"request-a","edit":{"changes":{"file:///private/source.rs":[{"range":{"start":{"line":0,"character":0},"end":{"line":0,"character":1}},"newText":"a"}]}}});
        assert!(checked_response(&response, "request-a", &Deadline::new(1000)).is_ok());
        response["edit"]["changes"]["file:///private/source.rs"][0]["command"] =
            json!({"command":"never.execute"});
        assert!(
            checked_response(&response, "request-a", &Deadline::new(1000)).is_err(),
            "Owned response silently dropped a nested unknown command field"
        );
    }

    fn canonical_inputs() -> (Value, Value) {
        (
            json!({
                "workspaceId":"workspace-a","snapshotId":"snapshot-a",
                "manifestHash":"manifest-a","analyzerProducer":"yoke-tree-sitter-8",
                "serverProfileHash":"profile-a",
            }),
            json!({
                "operation":"moveSymbol",
                "source":{
                    "snapshotId":"snapshot-a","path":"src/source.rs",
                    "contentHash":"source-hash","entityId":"source-entity",
                    "canonicalNamePath":"helper","startByte":10,"endByte":40,
                    "nameStartByte":17,"nameEndByte":23,
                },
                "destination":{
                    "kind":"parentSymbol",
                    "parent":{
                        "snapshotId":"snapshot-a","path":"src/destination.rs",
                        "contentHash":"destination-hash","entityId":"destination-entity",
                        "canonicalNamePath":"bucket","startByte":0,"endByte":80,
                        "nameStartByte":8,"nameEndByte":14,
                    },
                    "bodyStartByte":16,"bodyEndByte":79,
                },
            }),
        )
    }

    #[test]
    fn request_id_has_a_canonical_golden_and_ignores_actual_private_document_uris() {
        let (base, intent) = canonical_inputs();
        let first_documents = json!({
            "sourceUri":"file:///C:/Temp/yoke-lsp-first/src/source.rs",
            "destinationUri":"file:///C:/Temp/yoke-lsp-first/src/destination.rs",
        });
        let second_documents = json!({
            "sourceUri":"file:///D:/Other/yoke-lsp-second/src/source.rs",
            "destinationUri":"file:///D:/Other/yoke-lsp-second/src/destination.rs",
        });
        let (first_id, first) = build_owned_request(
            &base,
            &intent,
            first_documents.clone(),
            &Deadline::new(1000),
        )
        .unwrap();
        let (second_id, second) = build_owned_request(
            &base,
            &intent,
            second_documents.clone(),
            &Deadline::new(1000),
        )
        .unwrap();
        // Independently computed with Python's compact, key-sorted JSON and SHA-256.
        let golden = "a737bbfa96232e97eaa65d096a1c3027a99caa41f39d1abca987e5d4ed459860";
        assert_eq!(first_id, golden);
        assert_eq!(second_id, golden);
        assert_eq!(first["requestId"], golden);
        assert_eq!(second["requestId"], golden);
        assert_eq!(first["documents"], first_documents);
        assert_eq!(second["documents"], second_documents);
        assert_ne!(first["documents"], second["documents"]);
        assert_eq!(first["schemaVersion"], 1);
        assert_eq!(first["base"], base);
        assert_eq!(first["intent"], intent);
    }

    #[test]
    fn request_id_changes_with_every_manifest_profile_analyzer_and_intent_binding() {
        let (base, intent) = canonical_inputs();
        let documents = json!({"sourceUri":"file:///private/source.rs","destinationUri":"file:///private/destination.rs"});
        let golden = "a737bbfa96232e97eaa65d096a1c3027a99caa41f39d1abca987e5d4ed459860";
        for key in [
            "workspaceId",
            "snapshotId",
            "manifestHash",
            "analyzerProducer",
            "serverProfileHash",
        ] {
            let mut changed = base.clone();
            changed[key] = json!("changed");
            let (id, params) =
                build_owned_request(&changed, &intent, documents.clone(), &Deadline::new(1000))
                    .unwrap();
            assert_ne!(id, golden, "Base binding ignored: {key}");
            assert_eq!(params["base"], changed);
        }
        for pointer in [
            "/operation",
            "/source/snapshotId",
            "/source/path",
            "/source/contentHash",
            "/source/entityId",
            "/source/canonicalNamePath",
            "/source/startByte",
            "/source/endByte",
            "/source/nameStartByte",
            "/source/nameEndByte",
            "/destination/kind",
            "/destination/parent/snapshotId",
            "/destination/parent/path",
            "/destination/parent/contentHash",
            "/destination/parent/entityId",
            "/destination/parent/canonicalNamePath",
            "/destination/parent/startByte",
            "/destination/parent/endByte",
            "/destination/parent/nameStartByte",
            "/destination/parent/nameEndByte",
            "/destination/bodyStartByte",
            "/destination/bodyEndByte",
        ] {
            let mut changed = intent.clone();
            let field = changed.pointer_mut(pointer).unwrap();
            *field = field
                .as_u64()
                .map_or_else(|| json!("changed"), |value| json!(value + 1));
            let (id, params) =
                build_owned_request(&base, &changed, documents.clone(), &Deadline::new(1000))
                    .unwrap();
            assert_ne!(id, golden, "Intent binding ignored: {pointer}");
            assert_eq!(params["intent"], changed);
        }
    }
}
