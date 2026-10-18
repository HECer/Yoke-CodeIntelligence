use serde_json::{Value, json};
use std::{collections::BTreeSet, fs};
use tempfile::TempDir;
use yoke::{
    Engine,
    capture::Deadline,
    graph_view::{self, BuildOptions, View},
    project_session::{HostProfile, Session},
};

fn fixture() -> (TempDir, Session, View) {
    let root = TempDir::new().unwrap();
    fs::write(
        root.path().join("main.rs"),
        "fn helper(){helper();}\nfn caller(){helper();helper();}\n",
    )
    .unwrap();
    fs::write(
        root.path().join("guide.md"),
        "# Guide\n`main.rs::helper`\n`main.rs::helper`\n",
    )
    .unwrap();
    fs::write(
        root.path().join("package.json"),
        r#"{"name":"app","description":"app","dependencies":{"external":"^1"}}"#,
    )
    .unwrap();
    let deadline = Deadline::new(30000);
    let mut engine = Engine::open(root.path()).unwrap();
    let (snapshot, graph, _, _) = engine.index(&deadline).unwrap();
    let view = graph_view::build(
        &engine.store,
        &snapshot,
        &graph,
        &BuildOptions::default(),
        &deadline,
    )
    .unwrap();
    let session =
        Session::open(engine, HostProfile::default(), Some("graphify"), &deadline).unwrap();
    (root, session, view)
}

fn invoke(session: &mut Session, name: &str, args: Value) -> Value {
    yoke::graphify_adapter::invoke(session, name, &args, &Deadline::new(30000)).unwrap()
}

#[test]
fn existing_graphify_community_tools_assign_default_view_overlay_nodes() {
    // Canonical-only reports omit real manifest/Markdown nodes already returned by get_node.
    let (_root, mut session, view) = fixture();
    let expected: BTreeSet<_> = view
        .nodes
        .iter()
        .map(|node| node["id"].as_str().unwrap().to_owned())
        .collect();
    let mut actual = BTreeSet::new();
    for community_id in 0..view.nodes.len() {
        let community = invoke(
            &mut session,
            "get_community",
            json!({"community_id":community_id,"token_budget":65536}),
        );
        if community["found"] == false {
            break;
        }
        for node in community["nodes"].as_array().unwrap() {
            assert!(
                actual.insert(node["id"].as_str().unwrap().to_owned()),
                "A selected node was assigned to multiple communities"
            );
        }
    }
    let package = view
        .nodes
        .iter()
        .find(|node| node["kind"] == "package" && node["name"] == "app")
        .unwrap();
    let document = view
        .nodes
        .iter()
        .find(|node| node["kind"] == "document")
        .unwrap();
    assert!(
        actual.contains(package["id"].as_str().unwrap()),
        "Published package node was omitted from every community"
    );
    assert!(
        actual.contains(document["id"].as_str().unwrap()),
        "Published Markdown node was omitted from every community"
    );
    assert_eq!(actual, expected);
}

#[test]
fn canonical_report_bytes_and_registration_match_frozen_golden() {
    let root = TempDir::new().unwrap();
    fs::write(
        root.path().join("main.rs"),
        "fn alpha(){beta();beta();}\nfn beta(){}\n",
    )
    .unwrap();
    let mut engine = Engine::open(root.path()).unwrap();
    let workspace = "b".repeat(64);
    engine
        .store
        .db
        .execute(
            "UPDATE meta SET value=?1 WHERE key='workspace'",
            [&workspace],
        )
        .unwrap();
    engine.store.workspace_id = workspace;
    let deadline = Deadline::new(30000);
    let (snapshot, graph, _, _) = engine.index(&deadline).unwrap();
    let report = yoke::graph_extensions::analyze(
        &engine.store,
        &snapshot,
        &graph,
        &Default::default(),
        &deadline,
    )
    .unwrap();
    let artifact = report["artifactId"].as_str().unwrap();
    let record: Value = engine
        .store
        .get_record(yoke::graph_extensions::REPORT_KIND, artifact)
        .unwrap();
    let hashes = json!({"report":yoke::model::hash(engine.store.read_artifact(artifact).unwrap()),"record":yoke::model::identity(&record),"markdown":report["markdownArtifactId"]});
    assert_eq!(
        hashes,
        json!({"report":"d8f121c54b0a3ecbd898d035da3293dfbe08d79047600d7f8a21cb2a121852d0","record":"a49f27cbc779a406c1f03fa53966d7e629ef4b3f66459af3a269f821341508be","markdown":"ddc366379924b1ee7715fb66b2612265f5fa5eb65c773a7babd4c047ac9fa2b4"}),
        "Canonical report bytes must match the pre-refactor capture"
    );
}

#[test]
fn existing_graphify_hubs_include_parallel_citations_and_count_self_loops_twice() {
    // helper: enclosing-file contains=1, caller calls=2, recursive call=2, Markdown citations=2.
    let (_root, mut session, view) = fixture();
    let helper = view
        .nodes
        .iter()
        .find(|node| node["origin"] == "canonical" && node["name"] == "helper")
        .unwrap();
    let hubs = invoke(&mut session, "god_nodes", json!({"top_n":100}));
    let helper_hub = hubs["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["entityId"] == helper["id"])
        .unwrap();
    assert_eq!(
        helper_hub["degree"], 7,
        "Selected parallel/citation degree must include all captured resolved observations: {hubs}"
    );
    let document = view
        .nodes
        .iter()
        .find(|node| node["kind"] == "document")
        .unwrap();
    let document_hub = hubs["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["entityId"] == document["id"])
        .expect("Selected document must appear in the hub candidate set");
    assert_eq!(document_hub["degree"], 2);
}

#[test]
fn existing_graphify_hubs_preserve_file_level_manifest_source_precision() {
    let (_root, mut session, view) = fixture();
    let package = view
        .nodes
        .iter()
        .find(|node| node["kind"] == "package" && node["name"] == "app")
        .unwrap();
    // Repeated name prevents unique-token evidence; source is the enclosing manifest file.
    assert_eq!(package["sourceBindings"][0]["anchor"], "file");
    let hubs = invoke(&mut session, "god_nodes", json!({"top_n":100}));
    let package_hub = hubs["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["entityId"] == package["id"])
        .expect("Selected package must appear in the hub candidate set");
    assert_eq!(package_hub["anchor"], package["sourceBindings"][0]);
    assert_eq!(package_hub["sourceBindings"], package["sourceBindings"]);
    assert_eq!(package_hub["origin"], "structural_overlay");
}

#[test]
fn selected_report_preserves_provider_claims_and_every_original_anchor_family() {
    let (_root, mut session, initial) = fixture();
    let deadline = Deadline::new(30000);
    let engine = session.active_engine_at(&deadline).unwrap();
    let snapshot = engine.store.snapshot(&initial.snapshot_id).unwrap();
    let graph = engine.store.graph(&snapshot.id).unwrap();
    let file = snapshot.files.iter().find(|f| f.path == "main.rs").unwrap();
    let helper = graph.entities.iter().find(|n| n.name == "helper").unwrap();
    let caller = graph.entities.iter().find(|n| n.name == "caller").unwrap();
    let item = |node: &yoke::model::Entity| json!({"snapshotId":snapshot.id,"contentHash":file.hash,"path":"main.rs","startByte":node.start_byte,"endByte":node.end_byte,"selectionStartByte":node.name_start_byte.unwrap(),"selectionEndByte":node.name_end_byte.unwrap(),"name":node.name,"kind":12});
    let at = caller.start_byte + "fn caller(){".len();
    let anchor = json!({"snapshotId":snapshot.id,"contentHash":file.hash,"path":"main.rs","startByte":at,"endByte":at+6});
    let semantic = yoke::semantic_enrichment::persist(&engine.store, &snapshot, &graph, &json!({"snapshotId":snapshot.id,"operation":"incomingCalls","producer":"labeled_analytics_fixture","serverProfileHash":"a".repeat(64),"items":[{"root":item(helper),"from":item(caller),"fromRanges":[anchor,anchor]}]}), &deadline).unwrap();
    let view = graph_view::build(
        &engine.store,
        &snapshot,
        &graph,
        &BuildOptions {
            semantic_overlay_ids: vec![semantic["overlayId"].as_str().unwrap().into()],
            ..BuildOptions::default()
        },
        &deadline,
    )
    .unwrap();
    let report =
        yoke::graph_extensions::analyze_view(&engine.store, &view, &Default::default(), &deadline)
            .unwrap();
    assert_eq!(report["sourceBindings"], json!(view.nodes));
    assert_eq!(report["observations"], json!(view.observations));
    assert_eq!(report["evidenceAnchors"], json!(view.evidence));
    assert_eq!(report["overlays"], json!(view.overlays));
    assert!(
        report["overlays"]
            .as_array()
            .unwrap()
            .iter()
            .any(|o| o["providerAttestation"] == "unverified")
    );
    assert_eq!(
        report["audit"]["coverage"]["canonical"],
        json!(graph.coverage)
    );
    assert_eq!(report["audit"]["coverage"]["complete"], false);
    // Canonical: 2 function contains + 3 calls; the Guide concept has no contains arc.
    // Structural: 2 citations, 1 unresolved dependency, 1 dependencies -> external key arc.
    // Semantic: 2 parallel caller -> helper provider calls, in the overlay namespace.
    assert_eq!(
        report["audit"]["byRelation"],
        json!({"contains":2,"calls":5,"cites_symbol":2,"depends_on":1,"contains_key":1}),
        "Actual selected observations: {}",
        report["observations"]
    );
    assert_eq!(report["audit"]["totalObservations"], 11);
    assert_eq!(report["audit"]["unresolvedObservations"], 1);
    for family in ["godNodes", "communities", "surprises", "questions"] {
        assert!(!report[family].as_array().unwrap().is_empty());
        for row in report[family].as_array().unwrap() {
            let node = view
                .nodes
                .iter()
                .find(|n| n["id"] == row["anchorEntityId"])
                .unwrap();
            assert_eq!(row["sourceBindings"], node["sourceBindings"], "{family}");
            assert_eq!(row["sourceNode"], *node, "{family}");
            assert_eq!(row["anchor"], node["sourceBindings"][0], "{family}");
        }
    }
    let caller_hub = report["godNodes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["entityId"] == caller.id)
        .unwrap();
    assert_eq!(caller_hub["degree"], 3);
    let repeat =
        yoke::graph_extensions::analyze_view(&engine.store, &view, &Default::default(), &deadline)
            .unwrap();
    assert_eq!(repeat, report);
    assert_eq!(
        yoke::model::identity(&engine.store.graph(&snapshot.id).unwrap()),
        initial.base_graph_hash
    );
}

#[test]
fn selected_report_preparation_validation_and_admission_have_no_publication_side_effects() {
    let (_root, mut session, view) = fixture();
    let deadline = Deadline::new(30000);
    let engine = session.active_engine_at(&deadline).unwrap();
    let artifacts_before: usize = engine
        .store
        .db
        .query_row(
            "SELECT count(*) FROM records WHERE kind='artifact'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    let prepared = yoke::graph_extensions::prepare_view_report(
        &engine.store,
        &view,
        &Default::default(),
        &deadline,
    )
    .unwrap();
    assert!(
        engine
            .store
            .records::<Value>(yoke::graph_extensions::VIEW_REPORT_KIND)
            .unwrap()
            .is_empty()
    );
    let artifacts_after: usize = engine
        .store
        .db
        .query_row(
            "SELECT count(*) FROM records WHERE kind='artifact'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(artifacts_before, artifacts_after);
    let foreign = TempDir::new().unwrap();
    let other = Engine::open(foreign.path()).unwrap();
    assert!(prepared.publish(&other.store, &deadline).is_err());
    assert!(
        other
            .store
            .records::<Value>(yoke::graph_extensions::VIEW_REPORT_KIND)
            .unwrap()
            .is_empty()
    );
    assert!(
        yoke::graphify_adapter::invoke(
            &mut session,
            "get_community",
            &json!({"community_id":0,"token_budget":128}),
            &deadline
        )
        .is_err()
    );
    assert!(
        yoke::graphify_adapter::invoke(&mut session, "god_nodes", &json!({"top_n":0}), &deadline)
            .is_err()
    );
    let engine = session.active_engine_at(&deadline).unwrap();
    assert!(
        engine
            .store
            .records::<Value>(yoke::graph_extensions::VIEW_REPORT_KIND)
            .unwrap()
            .is_empty()
    );
    let artifacts_after: usize = engine
        .store
        .db
        .query_row(
            "SELECT count(*) FROM records WHERE kind='artifact'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(artifacts_before, artifacts_after);
}

#[test]
fn selected_report_retention_replays_exact_bytes_and_rejects_scope_and_cas_tampering() {
    let (_root, mut session, view) = fixture();
    let deadline = Deadline::new(30000);
    let engine = session.active_engine_at(&deadline).unwrap();
    let report =
        yoke::graph_extensions::analyze_view(&engine.store, &view, &Default::default(), &deadline)
            .unwrap();
    let id = report["artifactId"].as_str().unwrap();
    let record: Value = engine
        .store
        .get_record(yoke::graph_extensions::VIEW_REPORT_KIND, id)
        .unwrap();
    let artifacts_before: usize = engine
        .store
        .db
        .query_row(
            "SELECT count(*) FROM records WHERE kind='artifact'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    let roots =
        yoke::graph_extensions::validate_view_report_record(&engine.store, id, &record, &deadline)
            .unwrap();
    assert_eq!(roots.snapshot_id, view.snapshot_id);
    assert_eq!(roots.graph_view_id, view.id);
    assert!(roots.artifact_ids.contains(&view.artifact_id));
    let artifacts_after: usize = engine
        .store
        .db
        .query_row(
            "SELECT count(*) FROM records WHERE kind='artifact'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(artifacts_before, artifacts_after);
    let mut forged = record.clone();
    forged["baseGraphHash"] = json!("f".repeat(64));
    engine
        .store
        .db
        .execute(
            "UPDATE records SET value=?1 WHERE kind=?2 AND id=?3",
            rusqlite::params![
                forged.to_string(),
                yoke::graph_extensions::VIEW_REPORT_KIND,
                id
            ],
        )
        .unwrap();
    assert!(
        yoke::graph_extensions::validate_view_report_record(&engine.store, id, &forged, &deadline)
            .is_err()
    );
    engine
        .store
        .db
        .execute(
            "UPDATE records SET value=?1 WHERE kind=?2 AND id=?3",
            rusqlite::params![
                record.to_string(),
                yoke::graph_extensions::VIEW_REPORT_KIND,
                id
            ],
        )
        .unwrap();
    let path = engine.store.dir.join("blobs").join(id);
    let mut bytes = fs::read(&path).unwrap();
    bytes[0] = b' ';
    fs::write(&path, bytes).unwrap();
    assert!(
        yoke::graph_extensions::validate_view_report_record(&engine.store, id, &record, &deadline)
            .is_err()
    );
}

#[test]
fn selected_report_keeps_multiple_source_bindings_without_choosing_a_fabricated_exact_anchor() {
    let (_root, mut session, initial) = fixture();
    let deadline = Deadline::new(30000);
    let engine = session.active_engine_at(&deadline).unwrap();
    let snapshot = engine.store.snapshot(&initial.snapshot_id).unwrap();
    let graph = engine.store.graph(&snapshot.id).unwrap();
    let mut source =
        yoke::manifest_graph::extract(&engine.store, &snapshot, &graph, &deadline).unwrap();
    assert_eq!(source.nodes.len(), 1);
    assert_eq!(source.evidence.len(), 2);
    source.nodes[0]["evidenceIds"] = json!([source.evidence[0]["id"], source.evidence[1]["id"]]);
    source.artifact_id.clear();
    let artifact = engine
        .store
        .artifact(&serde_json::to_vec(&source).unwrap())
        .unwrap();
    engine.store.record(yoke::manifest_graph::RECORD_KIND,&artifact,&json!({"snapshotId":snapshot.id,"workspaceId":snapshot.workspace_id,"graphHash":yoke::model::identity(&graph),"artifactId":artifact,"producer":source.producer})).unwrap();
    let view = graph_view::build(
        &engine.store,
        &snapshot,
        &graph,
        &BuildOptions {
            structural_kinds: vec![],
            structural_overlay_ids: vec![artifact],
            semantic_overlay_ids: vec![],
            ..Default::default()
        },
        &deadline,
    )
    .unwrap();
    let package = view
        .nodes
        .iter()
        .find(|node| node["kind"] == "package")
        .unwrap();
    assert_eq!(package["sourceBindings"].as_array().unwrap().len(), 2);
    let report =
        yoke::graph_extensions::analyze_view(&engine.store, &view, &Default::default(), &deadline)
            .unwrap();
    let package_hub = report["godNodes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|hub| hub["entityId"] == package["id"])
        .unwrap();
    assert!(package_hub["anchor"].is_null());
    assert_eq!(package_hub["anchorSelection"], "multiple_source_bindings");
    assert_eq!(package_hub["sourceBindings"], package["sourceBindings"]);
}

#[test]
fn selected_report_publication_rechecks_cas_after_preparation() {
    let (_root, mut session, view) = fixture();
    let deadline = Deadline::new(30000);
    let engine = session.active_engine_at(&deadline).unwrap();
    let prepared = yoke::graph_extensions::prepare_view_report(
        &engine.store,
        &view,
        &Default::default(),
        &deadline,
    )
    .unwrap();
    let before = engine.store.records::<Value>("artifact").unwrap().len();
    let path = engine.store.dir.join("blobs").join(&view.artifact_id);
    let mut bytes = fs::read(&path).unwrap();
    bytes[0] = b' ';
    fs::write(&path, bytes).unwrap();
    assert!(prepared.publish(&engine.store, &deadline).is_err());
    assert!(
        engine
            .store
            .records::<Value>(yoke::graph_extensions::VIEW_REPORT_KIND)
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        engine.store.records::<Value>("artifact").unwrap().len(),
        before
    );
}

#[test]
fn selected_report_resource_admission_precedes_publication_of_a_large_formatted_body() {
    let root = TempDir::new().unwrap();
    let source = (0..3000)
        .map(|i| format!("fn member_{i}(){{}}\n"))
        .collect::<String>();
    fs::write(root.path().join("main.rs"), source).unwrap();
    let deadline = Deadline::new(120000);
    let mut engine = Engine::open(root.path()).unwrap();
    let (snapshot, graph, _, _) = engine.index(&deadline).unwrap();
    let view = graph_view::build(
        &engine.store,
        &snapshot,
        &graph,
        &BuildOptions::default(),
        &deadline,
    )
    .unwrap();
    let before = engine.store.records::<Value>("artifact").unwrap().len();
    let mut session =
        Session::open(engine, HostProfile::default(), Some("graphify"), &deadline).unwrap();
    let error = yoke::graphify_adapter::resource(
        &mut session,
        &format!("graphify://report?graphViewId={}", view.id),
        &deadline,
    )
    .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("resource exceeds output capacity"),
        "{error}"
    );
    let engine = session.active_engine_at(&deadline).unwrap();
    assert!(
        engine
            .store
            .records::<Value>(yoke::graph_extensions::VIEW_REPORT_KIND)
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        engine.store.records::<Value>("artifact").unwrap().len(),
        before
    );
}

#[test]
fn explicit_selected_analysis_and_resources_replay_historic_views_and_reject_unsupported_scope() {
    let (root, mut session, view) = fixture();
    let deadline = Deadline::new(30000);
    fs::write(root.path().join("package.json"), r#"{"name":"changed"}"#).unwrap();
    let engine = session.active_engine_at(&deadline).unwrap();
    let (new, _, _, _) = engine.index(&deadline).unwrap();
    let previous = yoke::graph_extensions::AnalysisOptions {
        previous_snapshot_id: Some(view.snapshot_id.clone()),
        ..Default::default()
    };
    assert!(
        yoke::graph_extensions::analyze_view(&engine.store, &view, &previous, &deadline).is_err()
    );
    let mut forged = view.clone();
    forged.nodes[0]["name"] = json!("forged");
    assert!(
        yoke::graph_extensions::analyze_view(
            &engine.store,
            &forged,
            &Default::default(),
            &deadline
        )
        .is_err()
    );
    assert!(
        yoke::graph_extensions::analyze_view(
            &engine.store,
            &view,
            &Default::default(),
            &Deadline::new(0)
        )
        .is_err()
    );
    let stats = invoke(&mut session, "graph_stats", json!({"graphViewId":view.id}));
    assert_eq!(stats["snapshotId"], view.snapshot_id);
    assert_eq!(stats["graphViewId"], view.id);
    assert_eq!(stats["nodeCount"], view.nodes.len());
    for name in [
        "report",
        "stats",
        "god-nodes",
        "audit",
        "surprises",
        "questions",
    ] {
        let (_, body) = yoke::graphify_adapter::resource(
            &mut session,
            &format!("graphify://{name}?graphViewId={}", view.id),
            &deadline,
        )
        .unwrap();
        assert!(body.contains(&view.id), "{name}");
        if matches!(name, "audit" | "surprises" | "questions") {
            let resource: Value = serde_json::from_str(&body).unwrap();
            assert_eq!(resource["graphHash"], stats["projectionGraphHash"]);
            assert_eq!(resource["graphHash"], resource["projectionGraphHash"]);
            assert_eq!(resource["baseGraphHash"], view.base_graph_hash);
            assert_eq!(resource["analysisScope"], "selected_view");
        }
    }
    for uri in [
        format!(
            "graphify://report?graphViewId={}&graphViewId={}",
            view.id, view.id
        ),
        "graphify://report?unknown=true".into(),
        "graphify://report?graphViewId=bad".into(),
    ] {
        assert!(yoke::graphify_adapter::resource(&mut session, &uri, &deadline).is_err());
    }
    assert!(
        yoke::graphify_adapter::invoke(
            &mut session,
            "graph_stats",
            &json!({"graphViewId":"f".repeat(64)}),
            &deadline
        )
        .is_err()
    );
    assert!(
        yoke::graphify_adapter::invoke(
            &mut session,
            "god_nodes",
            &json!({"graphViewId":view.id,"previousSnapshotId":view.snapshot_id}),
            &deadline
        )
        .is_err()
    );
    assert_eq!(
        session
            .active_engine_at(&deadline)
            .unwrap()
            .store
            .latest()
            .unwrap(),
        Some(new.id)
    );
}
