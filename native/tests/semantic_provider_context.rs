use serde_json::json;
use std::fs;
use yoke::{Engine, capture::Deadline, semantic_enrichment};
#[test]
fn semantic_provider_source_context_is_immutable_and_does_not_rewrite_historical_unknown_context() {
    let root = tempfile::tempdir().unwrap();
    fs::create_dir(root.path().join(".cargo")).unwrap();
    fs::write(
        root.path().join(".cargo/config.toml"),
        "[build]\nrustc=\"source-selected.exe\"\n",
    )
    .unwrap();
    fs::write(root.path().join("main.rs"), "fn actual() {}\n").unwrap();
    let mut engine = Engine::open(root.path()).unwrap();
    let deadline = Deadline::new(20000);
    let (snapshot, graph, _, _) = engine.index(&deadline).unwrap();
    let old = json!({"snapshotId":snapshot.id,"operation":"incomingCalls","producer":"independent-context-fixture","serverProfileHash":"c".repeat(64),"items":[]});
    let historical =
        semantic_enrichment::persist(&engine.store, &snapshot, &graph, &old, &deadline).unwrap();
    let old_loaded = semantic_enrichment::load(
        &engine.store,
        historical["overlayId"].as_str().unwrap(),
        &deadline,
    )
    .unwrap();
    let config = snapshot
        .files
        .iter()
        .find(|file| file.path == ".cargo/config.toml")
        .unwrap();
    let mut current = old.clone();
    let source = snapshot
        .files
        .iter()
        .find(|file| file.path == "main.rs")
        .unwrap();
    let node = json!({"snapshotId":snapshot.id,"name":"actual","kind":12,"path":"main.rs","contentHash":source.hash,"startByte":0,"endByte":14,"selectionStartByte":3,"selectionEndByte":9});
    current["items"] = json!([{"root":node,"from":node,"fromRanges":[]}]);
    current["sourceExecutionAuthorized"] = json!(false);
    current["providerSourcePolicy"] =
        json!("passive_rust_source_selectors_and_inherited_cargo_config_excluded");
    current["providerSourceOmissions"] = json!([{"path":config.path,"contentHash":config.hash,"reason":"workspace_execution_selector_omitted_from_private_provider_tree"}]);
    let published =
        semantic_enrichment::persist(&engine.store, &snapshot, &graph, &current, &deadline)
            .unwrap();
    let loaded = semantic_enrichment::load(
        &engine.store,
        published["overlayId"].as_str().unwrap(),
        &deadline,
    )
    .unwrap();
    assert_eq!(
        loaded["schemaVersion"], 2,
        "current source context was discarded: {loaded}"
    );
    assert_eq!(
        loaded["providerContext"]["sourceExecutionAuthorized"],
        false
    );
    assert_eq!(
        loaded["providerContext"]["omissions"][0]["contentHash"],
        config.hash
    );
    assert_ne!(published["overlayId"], historical["overlayId"]);
    assert_eq!(
        semantic_enrichment::load(
            &engine.store,
            historical["overlayId"].as_str().unwrap(),
            &deadline
        )
        .unwrap(),
        old_loaded
    );
    assert!(
        old_loaded.get("providerContext").is_none(),
        "historical unknown context was rewritten"
    );
    let mut wrong = current.clone();
    wrong["providerSourceOmissions"][0]["contentHash"] = json!("0".repeat(64));
    assert!(
        semantic_enrichment::persist(&engine.store, &snapshot, &graph, &wrong, &deadline).is_err()
    );
    for defect in [
        "contextRebind",
        "unknownField",
        "futureSchema",
        "foreignOmission",
    ] {
        let mut forged = loaded.clone();
        forged.as_object_mut().unwrap().remove("overlayId");
        match defect {
            "contextRebind" => {
                forged["providerContext"] = json!({"sourceExecutionAuthorized":true,"sourcePolicy":"host_selected_provider_unattested_transitive_behavior","omissions":[]})
            }
            "unknownField" => forged["providerContext"]["implicitExecutionGrant"] = json!(true),
            "futureSchema" => forged["schemaVersion"] = json!(3),
            _ => forged["providerContext"]["omissions"][0]["path"] = json!("outside.rs"),
        }
        let id = yoke::model::identity(&forged);
        forged["overlayId"] = json!(id);
        let artifact = engine
            .store
            .artifact(&serde_json::to_vec(&forged).unwrap())
            .unwrap();
        engine.store.record(semantic_enrichment::RECORD_KIND_V2,&id,&json!({"overlayId":id,"artifactId":artifact,"snapshotId":snapshot.id,"baseGraphHash":yoke::model::identity(&graph),"sourceBindings":forged["sourceBindings"],"providerContextHash":yoke::model::identity(&(&forged["serverProfileHash"],&forged["providerContext"])),"serverProfileHash":forged["serverProfileHash"]})).unwrap();
        assert!(
            semantic_enrichment::load(&engine.store, &id, &deadline).is_err(),
            "accepted resealed {defect}"
        );
    }
}
#[test]
fn forged_source_context_is_rejected_before_any_overlay_publication() {
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("main.rs"), "fn actual() {}\n").unwrap();
    let mut engine = Engine::open(root.path()).unwrap();
    let deadline = Deadline::new(20000);
    let (snapshot, graph, _, _) = engine.index(&deadline).unwrap();
    let input = json!({"snapshotId":snapshot.id,"operation":"incomingCalls","producer":"independent-context-fixture","serverProfileHash":"d".repeat(64),"items":[],"sourceExecutionAuthorized":false,"providerSourcePolicy":"passive_rust_source_selectors_and_inherited_cargo_config_excluded","providerSourceOmissions":[{"path":"outside.rs","contentHash":"d".repeat(64),"reason":"workspace_execution_selector_omitted_from_private_provider_tree"}]});
    assert!(
        semantic_enrichment::persist(&engine.store, &snapshot, &graph, &input, &deadline).is_err(),
        "unknown source omission was silently ignored"
    );
    let records: i64 = engine
        .store
        .db
        .query_row(
            "SELECT count(*) FROM records WHERE kind LIKE 'native_semantic_overlay_%'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(records, 0);
}

#[test]
fn versioned_rust_json_omissions_preserve_legacy_overlay_bytes_and_exact_policy_sets() {
    let root = tempfile::tempdir().unwrap();
    fs::create_dir(root.path().join(".cargo")).unwrap();
    fs::create_dir(root.path().join("nested")).unwrap();
    for (path, bytes) in [
        ("main.rs", "fn actual() {}\n"),
        (".cargo/config.toml", "[build]\nrustc='owned-canary'\n"),
        ("rust-project.json", "{\"crates\":[]}"),
        ("nested/RuSt-PrOjEcT.JsOn", "{\"crates\":[]}"),
    ] {
        fs::write(root.path().join(path), bytes).unwrap();
    }
    let mut engine = Engine::open(root.path()).unwrap();
    let deadline = Deadline::new(20000);
    let (snapshot, graph, _, _) = engine.index(&deadline).unwrap();
    let omission = |path: &str| {
        let file = snapshot.files.iter().find(|f| f.path == path).unwrap();
        json!({"path":path,"contentHash":file.hash,"reason":"workspace_execution_selector_omitted_from_private_provider_tree"})
    };
    for (legacy_policy, json_policy) in [
        (
            "passive_rust_source_selectors_and_inherited_cargo_config_excluded",
            "passive_rust_json_project_selectors_excluded_v2",
        ),
        (
            "contract_fixture_rust_selector_broker_only",
            "contract_fixture_rust_json_project_selectors_excluded_v2",
        ),
    ] {
        let legacy = json!({"snapshotId":snapshot.id,"operation":"incomingCalls","producer":"independent-versioned-context-fixture","serverProfileHash":"b".repeat(64),"items":[],"sourceExecutionAuthorized":false,"providerSourcePolicy":legacy_policy,"providerSourceOmissions":[omission(".cargo/config.toml")]});
        let published =
            semantic_enrichment::persist(&engine.store, &snapshot, &graph, &legacy, &deadline)
                .unwrap();
        let old_id = published["overlayId"].as_str().unwrap();
        let old_value = semantic_enrichment::load(&engine.store, old_id, &deadline).unwrap();
        let old_bytes = serde_json::to_vec(&old_value).unwrap();
        let mut current = legacy.clone();
        current["providerSourcePolicy"] = json!(json_policy);
        current["providerSourceOmissions"] = json!([
            omission(".cargo/config.toml"),
            omission("nested/RuSt-PrOjEcT.JsOn"),
            omission("rust-project.json")
        ]);
        let new_result =
            semantic_enrichment::persist(&engine.store, &snapshot, &graph, &current, &deadline)
                .unwrap();
        assert_ne!(new_result["overlayId"], published["overlayId"]);
        let loaded = semantic_enrichment::load(
            &engine.store,
            new_result["overlayId"].as_str().unwrap(),
            &deadline,
        )
        .unwrap();
        assert_eq!(loaded["providerContext"]["sourcePolicy"], json_policy);
        assert_eq!(
            loaded["providerContext"]["omissions"]
                .as_array()
                .unwrap()
                .len(),
            3
        );
        assert_eq!(
            serde_json::to_vec(
                &semantic_enrichment::load(&engine.store, old_id, &deadline).unwrap()
            )
            .unwrap(),
            old_bytes
        );
        for defect in ["missing", "wrongHash", "legacySet", "permission"] {
            let mut invalid = current.clone();
            match defect {
                "missing" => {
                    invalid["providerSourceOmissions"]
                        .as_array_mut()
                        .unwrap()
                        .pop();
                }
                "wrongHash" => {
                    invalid["providerSourceOmissions"][1]["contentHash"] = json!("0".repeat(64))
                }
                "legacySet" => invalid["providerSourcePolicy"] = json!(legacy_policy),
                _ => invalid["sourceExecutionAuthorized"] = json!(true),
            }
            assert!(
                semantic_enrichment::persist(&engine.store, &snapshot, &graph, &invalid, &deadline)
                    .is_err(),
                "accepted {defect}"
            );
        }
        assert_eq!(
            fs::read_to_string(root.path().join("rust-project.json")).unwrap(),
            "{\"crates\":[]}"
        );
    }
}
