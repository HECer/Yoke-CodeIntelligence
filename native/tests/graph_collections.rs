use std::fs;
// SQLite allocator observations are process-wide; isolate fixtures.
static TEST_GUARD: std::sync::Mutex<()> = std::sync::Mutex::new(());
use tempfile::TempDir;
use yoke::{Engine, capture::Deadline, graph_collections::Registry};
#[derive(serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct TestInput {
    artifact_id: String,
    payload: String,
}
#[derive(serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct TestArchive {
    view: yoke::graph_view::View,
    canonical_graph: yoke::model::Graph,
    inputs: Vec<TestInput>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    capture_scope: Option<serde_json::Value>,
}
#[derive(serde::Serialize, serde::Deserialize)]
struct TestRegistration {
    name: String,
    snapshot: yoke::model::Snapshot,
    graph: yoke::model::Graph,
    archive: TestArchive,
}
#[derive(serde::Serialize, serde::Deserialize)]
struct TestCanonicalRegistration {
    name: String,
    snapshot: yoke::model::Snapshot,
    graph: yoke::model::Graph,
    #[serde(
        rename = "captureScope",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    capture_scope: Option<serde_json::Value>,
}
#[test]
fn canonical_collection_also_rejects_resealed_protected_source_after_registration() {
    let _guard = TEST_GUARD
        .lock()
        .unwrap_or_else(|poison| poison.into_inner());
    let project = TempDir::new().unwrap();
    let central = TempDir::new().unwrap();
    fs::write(project.path().join("main.rs"), "fn original(){}\n").unwrap();
    let d = Deadline::new(20000);
    let mut engine = Engine::open(project.path()).unwrap();
    let (snapshot, graph, _, _) = engine.index(&d).unwrap();
    let registry = Registry::open(central.path()).unwrap();
    registry
        .register("canonical", &engine.store, &snapshot, &graph, &d)
        .unwrap();
    let db = rusqlite::Connection::open(central.path().join("yoke-collection.sqlite")).unwrap();
    let payload: String = db
        .query_row(
            "SELECT payload FROM registrations WHERE name='canonical'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    let data: TestCanonicalRegistration =
        serde_json::from_str(&payload.replace("main.rs", ".env")).unwrap();
    db.execute(
        "UPDATE registrations SET id=?1,payload=?2 WHERE name='canonical'",
        rusqlite::params![
            yoke::model::identity(&data),
            serde_json::to_string(&data).unwrap()
        ],
    )
    .unwrap();
    assert!(
        registry.join(&["canonical".into()], &d).is_err(),
        "Canonical archives require original captured path authority too"
    );
}
#[test]
fn archived_capture_scope_keeps_configured_readonly_dependencies_and_rejects_missing_proof() {
    let _guard = TEST_GUARD
        .lock()
        .unwrap_or_else(|poison| poison.into_inner());
    let project = TempDir::new().unwrap();
    let central = TempDir::new().unwrap();
    fs::create_dir_all(project.path().join("node_modules/pkg")).unwrap();
    fs::write(
        project.path().join("node_modules/pkg/index.js"),
        "export function dep(){}\n",
    )
    .unwrap();
    fs::write(project.path().join("main.rs"), "fn normal(){}\n").unwrap();
    let d = Deadline::new(20000);
    let mut engine = Engine::open(project.path()).unwrap();
    engine.capture_options =
        serde_json::from_value(serde_json::json!({"readOnlyDependencyDirs":["node_modules"]}))
            .unwrap();
    let (snapshot, graph, _, _) = engine.index(&d).unwrap();
    let view = yoke::graph_view::build(
        &engine.store,
        &snapshot,
        &graph,
        &yoke::graph_view::BuildOptions::default(),
        &d,
    )
    .unwrap();
    let registry = Registry::open(central.path()).unwrap();
    registry
        .register_view("deps", &engine.store, &view, &d)
        .unwrap();
    drop(engine);
    project.close().unwrap();
    let joined = registry.join(&["deps".into()], &d).unwrap();
    let node = joined["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|n| n["name"] == "dep")
        .unwrap();
    let read = registry
        .query(
            &["deps".into()],
            &serde_json::json!({"operation":"read","entityIds":[node["id"]]}),
            &d,
        )
        .unwrap();
    assert!(read["text"].as_str().unwrap().contains("function dep"));
    // Archived scope proves reads only; original live write protection remains.
    assert!(yoke::paths::normalized("node_modules/pkg/index.js").is_err());
    let db = rusqlite::Connection::open(central.path().join("yoke-collection.sqlite")).unwrap();
    let payload: String = db
        .query_row(
            "SELECT payload FROM registrations WHERE name='deps'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    let mut data: TestRegistration = serde_json::from_str(&payload).unwrap();
    assert!(data.archive.capture_scope.is_some());
    data.archive.capture_scope = None;
    db.execute(
        "UPDATE registrations SET id=?1,payload=?2 WHERE name='deps'",
        rusqlite::params![
            yoke::model::identity(&data),
            serde_json::to_string(&data).unwrap()
        ],
    )
    .unwrap();
    assert!(
        registry.join(&["deps".into()], &d).is_err(),
        "Configured dependency authority cannot be inferred from a missing proof"
    );
}
#[test]
fn legacy_default_native_archive_without_scope_sidecar_retains_original_identity() {
    let _guard = TEST_GUARD
        .lock()
        .unwrap_or_else(|poison| poison.into_inner());
    let project = TempDir::new().unwrap();
    let central = TempDir::new().unwrap();
    fs::write(project.path().join("main.rs"), "fn historical(){}\n").unwrap();
    let d = Deadline::new(20000);
    let mut engine = Engine::open(project.path()).unwrap();
    let (mut snapshot, _, _, _) = engine.index(&d).unwrap();
    snapshot.id = yoke::model::identity(&(
        &snapshot.workspace_id,
        "disk",
        "yoke-tree-sitter-4",
        &snapshot.files,
        &snapshot.omissions,
    ));
    // Synthetic compatibility shape: a native4 capture ID with explicitly
    // assembled V8 facts. This is not an authentic historical4 extractor replay.
    // Missing historical graphs must never make Engine infer a modern epoch.
    let extraction = yoke::analysis::extract_at(
        yoke::analysis::AnalyzerEpoch::V8,
        &snapshot.files[0],
        "fn historical(){}\n",
        &d,
    )
    .unwrap();
    let graph = yoke::analysis::assemble_at(
        yoke::analysis::AnalyzerEpoch::V8,
        &snapshot,
        vec![extraction],
        &d,
    )
    .unwrap();
    engine.store.publish_historical(&snapshot, &graph).unwrap();
    let view = yoke::graph_view::build(
        &engine.store,
        &snapshot,
        &graph,
        &yoke::graph_view::BuildOptions {
            structural_kinds: vec![],
            ..Default::default()
        },
        &d,
    )
    .unwrap();
    let registry = Registry::open(central.path()).unwrap();
    registry
        .register_view("legacy", &engine.store, &view, &d)
        .unwrap();
    let db = rusqlite::Connection::open(central.path().join("yoke-collection.sqlite")).unwrap();
    let payload: String = db
        .query_row(
            "SELECT payload FROM registrations WHERE name='legacy'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    let mut data: TestRegistration = serde_json::from_str(&payload).unwrap();
    data.archive.capture_scope = None;
    let id = yoke::model::identity(&data);
    let payload = serde_json::to_string(&data).unwrap();
    assert!(!payload.contains("captureScope"));
    db.execute(
        "UPDATE registrations SET id=?1,payload=?2 WHERE name='legacy'",
        rusqlite::params![id, payload],
    )
    .unwrap();
    drop(engine);
    project.close().unwrap();
    let joined = registry.join(&["legacy".into()], &d).unwrap();
    assert_eq!(joined["lineage"][0]["registrationId"], id);
    assert!(
        joined["nodes"]
            .as_array()
            .unwrap()
            .iter()
            .any(|n| n["name"] == "historical")
    );
    data.archive.capture_scope = Some(serde_json::json!({"schemaVersion":99,"nodes":[]}));
    db.execute(
        "UPDATE registrations SET id=?1,payload=?2 WHERE name='legacy'",
        rusqlite::params![
            yoke::model::identity(&data),
            serde_json::to_string(&data).unwrap()
        ],
    )
    .unwrap();
    assert!(
        registry.join(&["legacy".into()], &d).is_err(),
        "Invalid non-null proof cannot fall back to native historical identity"
    );
}
#[test]
fn view_archive_rejects_resealed_invented_rows_corrupt_inputs_and_foreign_view() {
    let _guard = TEST_GUARD
        .lock()
        .unwrap_or_else(|poison| poison.into_inner());
    use yoke::{
        graph_view,
        model::{hash, identity},
    };
    let central = TempDir::new().unwrap();
    let project = TempDir::new().unwrap();
    fs::write(
        project.path().join("main.rs"),
        "fn callee(){}\nfn caller(){callee();callee();}\n",
    )
    .unwrap();
    let mut engine = Engine::open(project.path()).unwrap();
    let (snapshot, graph, _, _) = engine.index(&Deadline::new(10000)).unwrap();
    let view = graph_view::build(
        &engine.store,
        &snapshot,
        &graph,
        &graph_view::BuildOptions::default(),
        &Deadline::new(10000),
    )
    .unwrap();
    let registry = Registry::open(central.path()).unwrap();
    let id = registry
        .register_view("selected", &engine.store, &view, &Deadline::new(10000))
        .unwrap();
    let mut forged = view.clone();
    forged.nodes[0]["name"] = serde_json::json!("invented");
    assert!(
        registry
            .register_view("forged", &engine.store, &forged, &Deadline::new(10000))
            .is_err()
    );
    let other = TempDir::new().unwrap();
    let foreign = Engine::open(other.path()).unwrap();
    assert!(
        registry
            .register_view("foreign", &foreign.store, &view, &Deadline::new(10000))
            .is_err()
    );
    let db = rusqlite::Connection::open(central.path().join("yoke-collection.sqlite")).unwrap();
    let original: String = db
        .query_row(
            "SELECT payload FROM registrations WHERE name='selected'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    for mode in ["invented", "corrupt"] {
        let mut data: TestRegistration = serde_json::from_str(&original).unwrap();
        assert_eq!(
            identity(&data),
            id,
            "independent resealing fixture must match native serialization"
        );
        if mode == "invented" {
            let entity = &mut data.graph.entities[0];
            entity.name = "invented".into();
            let row = data
                .archive
                .view
                .nodes
                .iter_mut()
                .find(|node| node["id"] == entity.id)
                .unwrap();
            row["name"] = serde_json::json!("invented");
            data.archive.view.id.clear();
            data.archive.view.artifact_id.clear();
            data.archive.view.id = identity(&data.archive.view);
            data.archive.view.artifact_id = hash(serde_json::to_vec(&data.archive.view).unwrap());
        } else {
            data.archive.inputs[0].payload.push(' ');
        }
        let forged_id = identity(&data);
        db.execute(
            "UPDATE registrations SET id=?1,payload=?2 WHERE name='selected'",
            rusqlite::params![forged_id, serde_json::to_string(&data).unwrap()],
        )
        .unwrap();
        assert!(
            registry
                .join(&["selected".into()], &Deadline::new(10000))
                .is_err(),
            "accepted resealed {mode}"
        );
    }
}
#[test]
fn selected_view_collection_preserves_origins_namespaces_and_source_after_retirement() {
    let _guard = TEST_GUARD
        .lock()
        .unwrap_or_else(|poison| poison.into_inner());
    use yoke::{graph_view, maintenance, retention};
    let central = TempDir::new().unwrap();
    let project = TempDir::new().unwrap();
    let source = "// 🌍\nfn callee(){}\nfn caller(){callee();callee();}\n";
    fs::write(project.path().join("main.rs"), source).unwrap();
    fs::create_dir(project.path().join(".cargo")).unwrap();
    fs::write(
        project.path().join(".cargo/config.toml"),
        "[build]\nrustc='source-selected.exe'\n",
    )
    .unwrap();
    fs::write(
        project.path().join("Cargo.toml"),
        "[package]\nname='demo'\nversion='0.1.0'\n[dependencies]\nserde='1'\n",
    )
    .unwrap();
    fs::write(project.path().join("mcp.json"),r#"{"mcpServers":{"tools":{"command":"npx","args":["@org/worker"],"env":{"TOKEN":"private-fixture-value"}}}}"#).unwrap();
    let mut engine = Engine::open(project.path()).unwrap();
    let (snapshot, graph, _, _) = engine.index(&Deadline::new(10000)).unwrap();
    let policy = snapshot
        .files
        .iter()
        .find(|file| file.path == ".cargo/config.toml")
        .unwrap();
    let metadata = serde_json::json!({"snapshotId":snapshot.id,"operation":"incomingCalls","producer":"archive-context-fixture","serverProfileHash":"c".repeat(64),"items":[],"sourceExecutionAuthorized":false,"providerSourcePolicy":"passive_rust_source_selectors_and_inherited_cargo_config_excluded","providerSourceOmissions":[{"path":policy.path,"contentHash":policy.hash,"reason":"workspace_execution_selector_omitted_from_private_provider_tree"}]});
    let semantic = yoke::semantic_enrichment::persist(
        &engine.store,
        &snapshot,
        &graph,
        &metadata,
        &Deadline::new(10000),
    )
    .unwrap();
    let options = graph_view::BuildOptions {
        semantic_overlay_ids: vec![semantic["overlayId"].as_str().unwrap().into()],
        ..graph_view::BuildOptions::default()
    };
    let view = graph_view::build(
        &engine.store,
        &snapshot,
        &graph,
        &options,
        &Deadline::new(10000),
    )
    .unwrap();
    let registry = Registry::open(central.path()).unwrap();
    registry
        .register(
            "canonical",
            &engine.store,
            &snapshot,
            &graph,
            &Deadline::new(10000),
        )
        .unwrap();
    registry
        .register_view("selected", &engine.store, &view, &Deadline::new(10000))
        .unwrap();
    let names = vec!["canonical".into(), "selected".into()];
    let joined = registry.join(&names, &Deadline::new(10000)).unwrap();
    assert_eq!(joined["lineage"][1]["viewId"], view.id);
    let producer = joined["lineage"][1]["selectedOrigins"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["origin"] == "semantic_overlay")
        .unwrap();
    assert_eq!(
        producer["providerContext"]["sourceExecutionAuthorized"],
        false
    );
    assert_eq!(
        producer["providerContext"]["omissions"][0]["contentHash"],
        policy.hash
    );
    assert!(!joined.to_string().contains("private-fixture-value"));
    let nodes = joined["nodes"].as_array().unwrap();
    let dependency = nodes
        .iter()
        .find(|node| node["registration"] == "selected" && node["name"] == "demo")
        .unwrap();
    assert_eq!(dependency["origin"], "structural_overlay");
    assert!(dependency["trust"].as_str().unwrap().contains("structural"));
    assert!(
        joined["observations"]
            .as_array()
            .unwrap()
            .iter()
            .any(|arc| arc["targetLabel"] == "serde" && arc["target"].is_null())
    );
    let caller = nodes
        .iter()
        .find(|node| node["registration"] == "selected" && node["name"] == "caller")
        .unwrap();
    let caller_id = caller["id"].as_str().unwrap().to_owned();
    let traced=registry.query(&names,&serde_json::json!({"operation":"trace","entityIds":[caller_id],"direction":"out","depth":1}),&Deadline::new(10000)).unwrap();
    assert_eq!(
        traced["observations"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|arc| arc["relation"] == "calls")
            .count(),
        2
    );
    fs::write(project.path().join("main.rs"), "fn current(){}\n").unwrap();
    engine.index(&Deadline::new(10000)).unwrap();
    engine
        .store
        .db
        .execute(
            "DELETE FROM records WHERE kind=?1 OR kind=?2",
            rusqlite::params![graph_view::RECORD_KIND, yoke::manifest_graph::RECORD_KIND],
        )
        .unwrap();
    engine
        .store
        .db
        .execute(
            "DELETE FROM records WHERE kind=?1 OR kind=?2",
            rusqlite::params![
                yoke::semantic_enrichment::RECORD_KIND,
                yoke::semantic_enrichment::RECORD_KIND_V2
            ],
        )
        .unwrap();
    for artifact in std::iter::once(&view.artifact_id).chain(
        view.overlays
            .iter()
            .map(|item| item["artifactId"].as_str().unwrap())
            .map(String::from)
            .collect::<Vec<_>>()
            .iter(),
    ) {
        engine
            .store
            .db
            .execute(
                "DELETE FROM records WHERE kind='artifact' AND id=?1",
                [artifact],
            )
            .unwrap();
    }
    let retirement = retention::plan(
        &engine.store,
        &retention::Policy::default(),
        &Deadline::new(20000),
    )
    .unwrap();
    let id = retirement["planId"].as_str().unwrap();
    retention::apply(&engine.store, id, id, true, &Deadline::new(20000)).unwrap();
    maintenance::collect(
        &engine.store,
        &maintenance::Policy {
            apply: true,
            minimum_age_ms: 0,
            purge_quarantine: false,
        },
        &Deadline::new(20000),
    )
    .unwrap();
    let purge = retention::purge_plan(&engine.store, 0, &Deadline::new(20000)).unwrap();
    let id = purge["planId"].as_str().unwrap();
    retention::purge(&engine.store, id, id, true, &Deadline::new(20000)).unwrap();
    assert!(engine.store.snapshot(&snapshot.id).is_err());
    let old_hash = &snapshot
        .files
        .iter()
        .find(|file| file.path == "main.rs")
        .unwrap()
        .hash;
    assert!(!engine.store.dir.join("blobs").join(old_hash).exists());
    let read=registry.query(&["selected".into()],&serde_json::json!({"operation":"read","entityIds":[caller_id],"startByte":0,"endByte":source.len()}),&Deadline::new(10000)).unwrap();
    assert_eq!(read["text"], source);
    assert_eq!(read["contentHash"], *old_hash);
    assert_eq!(
        registry.join(&names, &Deadline::new(10000)).unwrap(),
        joined
    );
}
#[test]
fn registry_payload_sqlite_cell_is_capped_before_length_admission() {
    let _guard = TEST_GUARD
        .lock()
        .unwrap_or_else(|poison| poison.into_inner());
    let registry_dir = TempDir::new().unwrap();
    let project = TempDir::new().unwrap();
    fs::write(project.path().join("main.rs"), "fn a(){}\n").unwrap();
    let mut engine = Engine::open(project.path()).unwrap();
    let (snapshot, graph, _, _) = engine.index(&Deadline::new(10000)).unwrap();
    let registry = Registry::open(registry_dir.path()).unwrap();
    registry
        .register(
            "project",
            &engine.store,
            &snapshot,
            &graph,
            &Deadline::new(10000),
        )
        .unwrap();
    let db =
        rusqlite::Connection::open(registry_dir.path().join("yoke-collection.sqlite")).unwrap();
    db.execute(
        "UPDATE registrations SET payload=CAST(zeroblob(64000000) AS TEXT)",
        [],
    )
    .unwrap();
    let baseline = unsafe { rusqlite::ffi::sqlite3_memory_used() };
    unsafe {
        rusqlite::ffi::sqlite3_memory_highwater(1);
    }
    let result = registry.join(&["project".into()], &Deadline::new(10000));
    let peak = unsafe { rusqlite::ffi::sqlite3_memory_highwater(0) };
    assert!(result.is_err());
    assert!(
        peak - baseline < 40_000_000,
        "allocated {} SQLite bytes before registry rejection",
        peak - baseline
    );
}
#[test]
fn registry_payload_capacity_is_checked_before_decoding_corrupt_registration() {
    let _guard = TEST_GUARD
        .lock()
        .unwrap_or_else(|poison| poison.into_inner());
    let registry_dir = TempDir::new().unwrap();
    let project = TempDir::new().unwrap();
    fs::write(project.path().join("main.rs"), "fn a(){}\n").unwrap();
    let mut engine = Engine::open(project.path()).unwrap();
    let (snapshot, graph, _, _) = engine.index(&Deadline::new(10000)).unwrap();
    let registry = Registry::open(registry_dir.path()).unwrap();
    registry
        .register(
            "project",
            &engine.store,
            &snapshot,
            &graph,
            &Deadline::new(10000),
        )
        .unwrap();
    let db =
        rusqlite::Connection::open(registry_dir.path().join("yoke-collection.sqlite")).unwrap();
    db.execute(
        "UPDATE registrations SET payload=CAST(zeroblob(32000001) AS TEXT) WHERE name='project'",
        [],
    )
    .unwrap();
    let error = registry
        .join(&["project".into()], &Deadline::new(10000))
        .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("Collection registration payload exceeds capacity"),
        "{error}"
    );
}
#[test]
fn collection_sqlite_sidecars_cannot_write_through_links() {
    let _guard = TEST_GUARD
        .lock()
        .unwrap_or_else(|poison| poison.into_inner());
    for suffix in ["-wal", "-shm", "-journal"] {
        let d = TempDir::new().unwrap();
        let target = d.path().join("sentinel");
        let bytes = vec![b'x'; 53000];
        fs::write(&target, &bytes).unwrap();
        let link = d.path().join(format!("yoke-collection.sqlite{suffix}"));
        #[cfg(windows)]
        std::os::windows::fs::symlink_file(&target, &link).unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(&target, &link).unwrap();
        assert!(
            Registry::open(d.path()).is_err(),
            "SQLite sidecar link accepted: {suffix}"
        );
        assert_eq!(fs::read(&target).unwrap(), bytes);
        fs::remove_file(link).unwrap();
    }
}
#[test]
fn collection_historical_source_survives_original_history_retirement_and_blob_purge() {
    let _guard = TEST_GUARD
        .lock()
        .unwrap_or_else(|poison| poison.into_inner());
    use yoke::{maintenance, retention};
    let c = TempDir::new().unwrap();
    let d = TempDir::new().unwrap();
    let original = "// 🌍\nfn historic(){}\n";
    fs::write(d.path().join("main.rs"), original).unwrap();
    let mut e = Engine::open(d.path()).unwrap();
    let (s, g, _, _) = e.index(&Deadline::new(10000)).unwrap();
    let r = Registry::open(c.path()).unwrap();
    r.register("project", &e.store, &s, &g, &Deadline::new(10000))
        .unwrap();
    let view = r.join(&["project".into()], &Deadline::new(10000)).unwrap();
    let node = view["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|n| n["name"] == "historic")
        .unwrap();
    let id = node["id"].as_str().unwrap();
    fs::write(d.path().join("main.rs"), "fn current(){}\n").unwrap();
    e.index(&Deadline::new(10000)).unwrap();
    let retirement = retention::plan(
        &e.store,
        &retention::Policy::default(),
        &Deadline::new(10000),
    )
    .unwrap();
    let plan = retirement["planId"].as_str().unwrap();
    retention::apply(&e.store, plan, plan, true, &Deadline::new(10000)).unwrap();
    maintenance::collect(
        &e.store,
        &maintenance::Policy {
            apply: true,
            minimum_age_ms: 0,
            purge_quarantine: false,
        },
        &Deadline::new(10000),
    )
    .unwrap();
    let purge = retention::purge_plan(&e.store, 0, &Deadline::new(10000)).unwrap();
    let plan = purge["planId"].as_str().unwrap();
    retention::purge(&e.store, plan, plan, true, &Deadline::new(10000)).unwrap();
    assert!(e.store.snapshot(&s.id).is_err());
    assert!(!e.store.dir.join("blobs").join(&s.files[0].hash).exists());
    let read=r.query(&["project".into()],&serde_json::json!({"operation":"read","entityIds":[id],"startByte":0,"endByte":original.len()}),&Deadline::new(10000)).unwrap();
    assert_eq!(read["text"], original);
    assert_eq!(read["contentHash"], s.files[0].hash);
    assert_eq!(read["snapshotId"], s.id);
}
#[test]
fn collection_queries_use_namespaced_entities_and_exact_original_call_anchors() {
    let _guard = TEST_GUARD
        .lock()
        .unwrap_or_else(|poison| poison.into_inner());
    let c = TempDir::new().unwrap();
    let d = TempDir::new().unwrap();
    fs::write(d.path().join("main.rs"), "fn a(){b();b();}\nfn b(){}\n").unwrap();
    let mut e = Engine::open(d.path()).unwrap();
    let (s, g, _, _) = e.index(&Deadline::new(10000)).unwrap();
    let r = Registry::open(c.path()).unwrap();
    r.register("project", &e.store, &s, &g, &Deadline::new(10000))
        .unwrap();
    let names = ["project".into()];
    let found = r
        .query(
            &names,
            &serde_json::json!({"operation":"find","query":"a","kind":"function"}),
            &Deadline::new(10000),
        )
        .unwrap();
    assert_eq!(found["items"].as_array().unwrap().len(), 1);
    let id = found["items"][0]["id"].as_str().unwrap();
    let traced = r
        .query(
            &names,
            &serde_json::json!({"operation":"trace","entityIds":[id],"relations":["calls"]}),
            &Deadline::new(10000),
        )
        .unwrap();
    assert_eq!(traced["observations"].as_array().unwrap().len(), 2);
    for anchor in traced["evidence"].as_array().unwrap() {
        assert_eq!(anchor["contentHash"], s.files[0].hash);
    }
    assert!(
        r.query(
            &names,
            &serde_json::json!({"operation":"trace","entityIds":[g.entities[0].id]}),
            &Deadline::new(10000)
        )
        .is_err()
    );
    assert!(
        r.query(
            &names,
            &serde_json::json!({"operation":"find","query":"a","allowWrites":true}),
            &Deadline::new(10000)
        )
        .is_err()
    );
}
#[test]
fn registered_collection_namespaces_equal_symbols_and_preserves_parallel_edges() {
    let _guard = TEST_GUARD
        .lock()
        .unwrap_or_else(|poison| poison.into_inner());
    let central = TempDir::new().unwrap();
    let left = TempDir::new().unwrap();
    let right = TempDir::new().unwrap();
    fs::write(
        left.path().join("main.rs"),
        "struct Shared {}\nfn a(){b();b();}\nfn b(){}\n",
    )
    .unwrap();
    fs::write(
        right.path().join("main.rs"),
        "struct Shared {}\nfn a(){b();}\nfn b(){}\n",
    )
    .unwrap();
    let mut a = Engine::open(left.path()).unwrap();
    let mut b = Engine::open(right.path()).unwrap();
    let (sa, ga, _, _) = a.index(&Deadline::new(10000)).unwrap();
    let (sb, gb, _, _) = b.index(&Deadline::new(10000)).unwrap();
    let registry = Registry::open(central.path()).unwrap();
    let id = registry
        .register("left", &a.store, &sa, &ga, &Deadline::new(10000))
        .unwrap();
    registry
        .register("right", &b.store, &sb, &gb, &Deadline::new(10000))
        .unwrap();
    let joined = registry
        .join(&["left".into(), "right".into()], &Deadline::new(10000))
        .unwrap();
    assert_eq!(
        joined["nodes"].as_array().unwrap().len(),
        ga.entities.len() + gb.entities.len()
    );
    assert_eq!(
        joined["observations"].as_array().unwrap().len(),
        ga.observations.len() + gb.observations.len()
    );
    let ids: std::collections::BTreeSet<_> = joined["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|n| n["id"].as_str().unwrap())
        .collect();
    assert_eq!(ids.len(), ga.entities.len() + gb.entities.len());
    let keys: std::collections::BTreeSet<_> = joined["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|n| n["key"].as_str().unwrap())
        .collect();
    assert_eq!(
        keys.len(),
        ga.entities.len() + gb.entities.len(),
        "Collection internal keys must be namespaced as well as public entity IDs"
    );
    assert!(
        joined["candidates"]
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e["relation"] == "shared_type_candidate" && e["resolution"] == "heuristic")
    );
    assert!(
        registry
            .remove("left", "invalid", &Deadline::new(10000))
            .is_err()
    );
    registry.remove("left", &id, &Deadline::new(10000)).unwrap();
    assert!(
        registry
            .join(&["left".into()], &Deadline::new(10000))
            .is_err()
    );
}
#[test]
fn registration_is_immutable_and_rejects_forged_graph_or_unregistered_scope() {
    let _guard = TEST_GUARD
        .lock()
        .unwrap_or_else(|poison| poison.into_inner());
    let central = TempDir::new().unwrap();
    let project = TempDir::new().unwrap();
    fs::write(project.path().join("main.rs"), "fn first(){}\n").unwrap();
    let mut engine = Engine::open(project.path()).unwrap();
    let (s, g, _, _) = engine.index(&Deadline::new(10000)).unwrap();
    let registry = Registry::open(central.path()).unwrap();
    registry
        .register("project", &engine.store, &s, &g, &Deadline::new(10000))
        .unwrap();
    let before = registry
        .join(&["project".into()], &Deadline::new(10000))
        .unwrap();
    fs::write(project.path().join("main.rs"), "fn changed(){}\n").unwrap();
    engine.index(&Deadline::new(10000)).unwrap();
    let reopened = Registry::open(central.path()).unwrap();
    assert_eq!(
        before,
        reopened
            .join(&["project".into()], &Deadline::new(10000))
            .unwrap()
    );
    let mut fake = g.clone();
    fake.entities[0].name = "forged".into();
    assert!(
        registry
            .register("forged", &engine.store, &s, &fake, &Deadline::new(10000))
            .is_err()
    );
    assert!(
        registry
            .register("../outside", &engine.store, &s, &g, &Deadline::new(10000))
            .is_err()
    );
    assert!(
        registry
            .join(&["missing".into()], &Deadline::new(10000))
            .is_err()
    );
    assert!(
        registry
            .join(&["project".into(), "project".into()], &Deadline::new(10000))
            .is_err()
    );
    assert!(
        registry
            .join(&["project".into()], &Deadline::new(0))
            .is_err()
    );
}
