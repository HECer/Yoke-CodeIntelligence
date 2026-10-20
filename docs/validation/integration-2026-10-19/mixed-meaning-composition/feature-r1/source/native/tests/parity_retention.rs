use serde_json::json;
use std::fs;
use tempfile::TempDir;
use yoke::{
    Engine,
    capture::{self, CaptureOptions, Deadline},
    notes::{self, Scope},
    project_session::{HostProfile, Session},
    retention,
};

fn fixture() -> (TempDir, Session, String) {
    let dir = TempDir::new().unwrap();
    fs::create_dir(dir.path().join("src")).unwrap();
    fs::write(dir.path().join("src/main.rs"), "fn initial() {}\n").unwrap();
    let mut engine = Engine::open(dir.path()).unwrap();
    engine.capture_options = CaptureOptions {
        only_dirs: vec!["src".into()],
        ..Default::default()
    };
    let deadline = Deadline::new(20000);
    let snapshot = engine.index(&deadline).unwrap().0;
    notes::invoke(
        &engine.store,
        &engine.store,
        &snapshot,
        Scope::Project,
        "write",
        &json!({"name":"decision","content":"retain source","paths":["src/main.rs"]}),
        &notes::Policy {
            writable: true,
            ..Default::default()
        },
        &deadline,
    )
    .unwrap();
    let session =
        Session::open(engine, HostProfile::default(), Some("retention"), &deadline).unwrap();
    (dir, session, snapshot.id)
}

#[test]
fn valid_capture_notes_session_and_unpublished_capture_do_not_disable_retention() {
    let (dir, mut session, before) = fixture();
    fs::write(dir.path().join("src/main.rs"), "fn current() {}\n").unwrap();
    let deadline = Deadline::new(20000);
    let observed = capture::capture_with_options(
        dir.path(),
        &session.engine.store,
        &session.engine.capture_options,
        &deadline,
    )
    .unwrap();
    assert!(session.engine.store.snapshot(&observed.id).is_err());
    let report = retention::plan(
        &session.engine.store,
        &retention::Policy::default(),
        &deadline,
    )
    .unwrap();
    assert_eq!(report["unknownDependencyKinds"], json!([]));
    assert_eq!(report["snapshotIds"], json!([]));
    session.engine.index(&deadline).unwrap();
    let report = retention::plan(
        &session.engine.store,
        &retention::Policy::default(),
        &deadline,
    )
    .unwrap();
    assert_eq!(report["unknownDependencyKinds"], json!([]));
    assert_eq!(
        report["snapshotIds"],
        json!([]),
        "note anchor must retain prior snapshot {before}"
    );
    let purge = retention::purge_plan(&session.engine.store, 0, &deadline).unwrap();
    assert_eq!(purge["unknownDependencyKinds"], json!([]));
}

#[test]
fn parity_record_future_fields_and_corrupt_bindings_never_grant_purge() {
    for corrupt in [false, true] {
        let (_dir, session, snapshot_id) = fixture();
        let mut value: serde_json::Value = session
            .engine
            .store
            .get_record(capture::OPTIONS_RECORD_KIND, &snapshot_id)
            .unwrap();
        if corrupt {
            value["workspaceId"] = json!("0".repeat(64));
        } else {
            value["futureSourceDependencies"] = json!(["unrecognized"]);
        }
        session
            .engine
            .store
            .db
            .execute(
                "UPDATE records SET value=?1 WHERE kind=?2 AND id=?3",
                rusqlite::params![value.to_string(), capture::OPTIONS_RECORD_KIND, snapshot_id],
            )
            .unwrap();
        let result = retention::purge_plan(&session.engine.store, 0, &Deadline::new(20000));
        assert!(
            result.is_err()
                || !result.unwrap()["unknownDependencyKinds"]
                    .as_array()
                    .unwrap()
                    .is_empty()
        );
    }
}
#[test]
fn source_bound_reports_exports_and_markdown_artifacts_are_known_retention_dependencies() {
    let (_dir, mut session, _) = fixture();
    let deadline = Deadline::new(20000);
    let (snapshot, graph, _, _) = session.engine.index(&deadline).unwrap();
    yoke::graph_extensions::analyze(
        &session.engine.store,
        &snapshot,
        &graph,
        &Default::default(),
        &deadline,
    )
    .unwrap();
    yoke::graph_exports::generate(
        &session.engine.store,
        &snapshot,
        &graph,
        &yoke::graph_exports::ExportOptions::new(yoke::graph_exports::ExportFormat::Svg),
        &deadline,
    )
    .unwrap();
    let plan = retention::purge_plan(&session.engine.store, 0, &deadline).unwrap();
    assert_eq!(plan["unknownDependencyKinds"], json!([]));
}
#[test]
fn structural_semantic_and_view_artifacts_preserve_exact_source_roots() {
    let (dir, mut session, _) = fixture();
    fs::write(
        dir.path().join("src/package.json"),
        r#"{"name":"app","dependencies":{"library":"^1"}}"#,
    )
    .unwrap();
    fs::write(
        dir.path().join("src/config.json"),
        r##"{"service":{"$ref":"#/definitions/User"},"definitions":{"User":{"type":"object"}}}"##,
    )
    .unwrap();
    fs::write(
        dir.path().join("src/guide.md"),
        "# Guide\n`src/main.rs::initial`\n",
    )
    .unwrap();
    let deadline = Deadline::new(20000);
    let (snapshot, graph, _, _) = session.engine.index(&deadline).unwrap();
    let file = snapshot
        .files
        .iter()
        .find(|file| file.path == "src/main.rs")
        .unwrap();
    let overlay = yoke::semantic_enrichment::persist(&session.engine.store, &snapshot, &graph, &json!({"snapshotId":snapshot.id,"operation":"outgoingCalls","producer":"owned-retention-fixture","serverProfileHash":"a".repeat(64),"items":[{"root":{"snapshotId":snapshot.id,"name":"initial","kind":12,"path":file.path,"contentHash":file.hash,"startByte":0,"endByte":15,"selectionStartByte":3,"selectionEndByte":10},"to":{"snapshotId":snapshot.id,"name":"initial","kind":12,"path":file.path,"contentHash":file.hash,"startByte":0,"endByte":15,"selectionStartByte":3,"selectionEndByte":10},"fromRanges":[]}],"complete":false}), &deadline).unwrap();
    let options = yoke::graph_view::BuildOptions {
        semantic_overlay_ids: vec![overlay["overlayId"].as_str().unwrap().into()],
        ..Default::default()
    };
    let view = yoke::graph_view::build(
        &session.engine.store,
        &snapshot,
        &graph,
        &options,
        &deadline,
    )
    .unwrap();
    let plan = retention::purge_plan(&session.engine.store, 0, &deadline).unwrap();
    assert_eq!(plan["unknownDependencyKinds"], json!([]));
    assert!(
        plan["items"]
            .as_array()
            .unwrap()
            .iter()
            .all(|item| item["blobId"] != view.artifact_id
                && item["blobId"] != overlay["artifactId"])
    );
    assert!(
        session
            .engine
            .store
            .read_artifact(&view.artifact_id)
            .is_ok()
    );
}

#[test]
fn rehashed_report_with_unknown_nested_schema_is_refused() {
    let (_dir, mut session, _) = fixture();
    let deadline = Deadline::new(20000);
    let (snapshot, graph, _, _) = session.engine.index(&deadline).unwrap();
    let report = yoke::graph_extensions::analyze(
        &session.engine.store,
        &snapshot,
        &graph,
        &Default::default(),
        &deadline,
    )
    .unwrap();
    let old = report["artifactId"].as_str().unwrap();
    let mut raw = yoke::json::parse(
        &session.engine.store.read_artifact(old).unwrap(),
        64_000_000,
    )
    .unwrap();
    raw["clustering"]["futureDependencies"] = json!(["encoded elsewhere"]);
    let id = session
        .engine
        .store
        .artifact(&serde_json::to_vec(&raw).unwrap())
        .unwrap();
    let mut record: serde_json::Value = session
        .engine
        .store
        .get_record(yoke::graph_extensions::REPORT_KIND, old)
        .unwrap();
    record["artifactId"] = json!(id);
    session
        .engine
        .store
        .record(yoke::graph_extensions::REPORT_KIND, &id, &record)
        .unwrap();
    assert!(retention::purge_plan(&session.engine.store, 0, &deadline).is_err());
}
#[test]
fn duplicate_provenance_fields_are_refused_before_dependency_interpretation() {
    let (dir, session, _) = fixture();
    fs::write(dir.path().join("src/main.rs"), "fn observed() {}\n").unwrap();
    let snapshot_id = capture::capture_with_options(
        dir.path(),
        &session.engine.store,
        &session.engine.capture_options,
        &Deadline::new(20000),
    )
    .unwrap()
    .id;
    assert!(session.engine.store.snapshot(&snapshot_id).is_err());
    let raw: String = session
        .engine
        .store
        .db
        .query_row(
            "SELECT value FROM records WHERE kind=?1 AND id=?2",
            rusqlite::params![capture::OPTIONS_RECORD_KIND, snapshot_id],
            |row| row.get(0),
        )
        .unwrap();
    let duplicated = format!(
        "{{\"workspaceId\":\"opaque future dependency\",{}",
        &raw[1..]
    );
    session
        .engine
        .store
        .db
        .execute(
            "UPDATE records SET value=?1 WHERE kind=?2 AND id=?3",
            rusqlite::params![duplicated, capture::OPTIONS_RECORD_KIND, snapshot_id],
        )
        .unwrap();
    assert!(retention::purge_plan(&session.engine.store, 0, &Deadline::new(20000)).is_err());
}
#[test]
fn released_analyzer_four_extraction_cache_is_readable_without_rewriting_old_payloads() {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("old.ps1"), "function Original { }\n").unwrap();
    let mut engine = Engine::open(dir.path()).unwrap();
    let deadline = Deadline::new(20000);
    let mut archived = capture::capture(dir.path(), &engine.store, &deadline).unwrap();
    archived.files[0].language = "text".into();
    archived.id = yoke::model::identity(&(
        &engine.store.workspace_id,
        "disk",
        "yoke-tree-sitter-4",
        &archived.files,
        &archived.omissions,
    ));
    let file = &archived.files[0];
    let mut extraction =
        yoke::analysis::extract(file, "function Original { }\n", &deadline).unwrap();
    for entity in &mut extraction.entities {
        entity.producer = "yoke-tree-sitter-4".into();
    }
    // HEAD's released Extraction is exactly entities/edges/diagnostics; all
    // three nested typed schemas are unchanged. It has no resolution field.
    let legacy = json!({"entities":extraction.entities,"edges":extraction.edges,"diagnostics":extraction.diagnostics});
    let key =
        yoke::model::identity(&("yoke-tree-sitter-4", &file.language, &file.hash, &file.path));
    let raw = legacy.to_string();
    engine
        .store
        .db
        .execute(
            "INSERT INTO cache VALUES(?1,?2)",
            rusqlite::params![key, raw],
        )
        .unwrap();
    // This compatibility fixture checks the released cache schema and capture
    // identity. Its empty archived graph claims no replay of extractor four.
    let graph = yoke::model::Graph {
        snapshot_id: archived.id.clone(),
        ..Default::default()
    };
    engine.store.publish(&archived, &graph).unwrap();
    assert_eq!(
        capture::validate_language(&engine.store, &archived, "old.ps1").unwrap(),
        "text"
    );
    let plan = retention::purge_plan(&engine.store, 0, &deadline).unwrap();
    assert_eq!(plan["unknownDependencyKinds"], json!([]));
    let preserved: String = engine
        .store
        .db
        .query_row("SELECT value FROM cache WHERE key=?1", [&key], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(preserved, raw);
    let mut future = legacy;
    future["futureDependencies"] = json!(["encoded elsewhere"]);
    engine
        .store
        .db
        .execute(
            "UPDATE cache SET value=?1 WHERE key=?2",
            rusqlite::params![future.to_string(), key],
        )
        .unwrap();
    let plan = retention::purge_plan(&engine.store, 0, &deadline).unwrap();
    assert_eq!(
        plan["unknownDependencyKinds"],
        json!(["cache:unknown-shape"])
    );
}
#[test]
fn development_six_cache_missing_only_embedded_language_keeps_exact_old_schema() {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("main.rs"), "fn original() {}\n").unwrap();
    let mut engine = Engine::open(dir.path()).unwrap();
    let deadline = Deadline::new(20000);
    let (snapshot, _, _, _) = engine.index(&deadline).unwrap();
    let file = &snapshot.files[0];
    let extraction = yoke::analysis::extract(file, "fn original() {}\n", &deadline).unwrap();
    let mut old = serde_json::to_value(extraction).unwrap();
    old["resolution"]
        .as_object_mut()
        .unwrap()
        .remove("language");
    for entity in old["entities"].as_array_mut().unwrap() {
        entity["producer"] = json!("yoke-tree-sitter-6");
    }
    let key =
        yoke::model::identity(&("yoke-tree-sitter-6", &file.language, &file.hash, &file.path));
    let raw = old.to_string();
    engine
        .store
        .db
        .execute(
            "INSERT INTO cache VALUES(?1,?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value",
            rusqlite::params![key, raw],
        )
        .unwrap();
    assert_eq!(
        retention::purge_plan(&engine.store, 0, &deadline).unwrap()["unknownDependencyKinds"],
        json!([])
    );
    let unchanged: String = engine
        .store
        .db
        .query_row("SELECT value FROM cache WHERE key=?1", [&key], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(unchanged, raw);
    old["resolution"]["futureDependencies"] = json!(["encoded elsewhere"]);
    engine
        .store
        .db
        .execute(
            "UPDATE cache SET value=?1 WHERE key=?2",
            rusqlite::params![old.to_string(), key],
        )
        .unwrap();
    assert_eq!(
        retention::purge_plan(&engine.store, 0, &deadline).unwrap()["unknownDependencyKinds"],
        json!(["cache:unknown-shape"])
    );
}
