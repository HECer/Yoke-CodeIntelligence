use serde_json::json;
use std::fs;
use tempfile::TempDir;
use yoke::{
    Engine,
    analysis::AnalyzerEpoch,
    capture::{self, Deadline},
    changes, editor,
};

const SOURCE: &str = "function Invoke-Work { }\ninvoke-work\n";

fn fixture(epoch: AnalyzerEpoch) -> (TempDir, Engine) {
    let root = TempDir::new().unwrap();
    fs::write(root.path().join("main.ps1"), SOURCE).unwrap();
    let mut engine = Engine::open(root.path()).unwrap();
    engine.capture_epoch = epoch;
    (root, engine)
}

fn append_args(hash: &str) -> serde_json::Value {
    json!({"edits":[{"kind":"replace","path":"main.ps1","expectedHash":hash,
        "startByte":SOURCE.len(),"endByte":SOURCE.len(),"text":"# Authored change\n"}]})
}

#[test]
fn ordinary_guarded_prepare_preserves_the_proven_base_epoch() {
    let (root, mut engine) = fixture(AnalyzerEpoch::V9);
    let d = Deadline::new(30_000);
    let (base, graph, _, _) = engine.index(&d).unwrap();
    let plan = changes::prepare(
        &mut engine.store,
        &base,
        &graph,
        &append_args(&base.files[0].hash),
        &d,
    )
    .unwrap();
    let result = engine.store.snapshot(&plan.result_snapshot_id).unwrap();
    assert_eq!(
        capture::analyzer_epoch_for_snapshot_with_deadline(&engine.store, &result, &d).unwrap(),
        AnalyzerEpoch::V9
    );
    let derived = engine.store.graph(&result.id).unwrap();
    assert!(derived.observations.iter().any(|arc| {
        arc.relation == "calls" && arc.target_label == "invoke-work" && arc.target.is_some()
    }));
    assert_eq!(
        fs::read_to_string(root.path().join("main.ps1")).unwrap(),
        SOURCE
    );
}

#[test]
fn mcp_prepare_uses_historical_source_policy_instead_of_new_capture_policy() {
    let (_root, mut engine) = fixture(AnalyzerEpoch::V8);
    let d = Deadline::new(30_000);
    let (old, _, _, _) = engine.index(&d).unwrap();
    engine.capture_epoch = AnalyzerEpoch::V9;
    engine.index(&d).unwrap();
    let mut args = append_args(&old.files[0].hash);
    args["snapshotId"] = json!(old.id);
    let response = engine.invoke("ci_change_prepare", args).unwrap();
    assert_eq!(response["status"], "ok", "{response}");
    let id = response["data"]["plan"]["resultSnapshotId"]
        .as_str()
        .unwrap();
    let result = engine.store.snapshot(id).unwrap();
    assert_eq!(
        capture::analyzer_epoch_for_snapshot_with_deadline(&engine.store, &result, &d).unwrap(),
        AnalyzerEpoch::V8
    );
}

#[test]
fn ordinary_editor_capture_uses_the_explicit_host_epoch() {
    let (root, mut engine) = fixture(AnalyzerEpoch::V9);
    let d = Deadline::new(30_000);
    let session = editor::open(&mut engine, &d).unwrap();
    editor::update(
        &engine,
        &session.id,
        &json!({"path":"main.ps1","version":1,"text":format!("{SOURCE}# Buffer\n")}),
        &d,
    )
    .unwrap();
    let overlay = editor::capture(&mut engine, &session.id, &d).unwrap();
    assert_eq!(
        capture::analyzer_epoch_for_snapshot_with_deadline(&engine.store, &overlay, &d).unwrap(),
        AnalyzerEpoch::V9
    );
    assert_eq!(
        fs::read_to_string(root.path().join("main.ps1")).unwrap(),
        SOURCE
    );
}
