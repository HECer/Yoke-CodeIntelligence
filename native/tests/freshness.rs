use serde_json::json;
use std::fs;
use tempfile::TempDir;
use yoke::{Engine, capture::Deadline};

#[test]
fn freshness_observes_drift_without_publishing_or_moving_latest() {
    let root = TempDir::new().unwrap();
    fs::write(root.path().join("changed.rs"), "fn before() {}\n").unwrap();
    fs::write(root.path().join("deleted.rs"), "fn removed() {}\n").unwrap();
    let mut engine = Engine::open(root.path()).unwrap();
    let (baseline, graph, _, _) = engine.index(&Deadline::new(20000)).unwrap();
    let count: i64 = engine
        .store
        .db
        .query_row("SELECT COUNT(*) FROM snapshots", [], |r| r.get(0))
        .unwrap();
    // A prepared branch may become latest; it must not redefine disk freshness.
    let plan = yoke::changes::prepare(
        &mut engine.store,
        &baseline,
        &graph,
        &json!({"edits":[{"kind":"create","path":"planned.rs","text":"fn planned() {}\n"}]}),
        &Deadline::new(20000),
    )
    .unwrap();
    engine.store.set_latest(&plan.result_snapshot_id).unwrap();
    let latest = engine.store.latest().unwrap();
    fs::write(root.path().join("changed.rs"), "fn after() {}\n").unwrap();
    fs::remove_file(root.path().join("deleted.rs")).unwrap();
    fs::write(root.path().join("added.rs"), "fn added() {}\n").unwrap();
    let count_after_plan: i64 = engine
        .store
        .db
        .query_row("SELECT COUNT(*) FROM snapshots", [], |r| r.get(0))
        .unwrap();
    assert_eq!(count_after_plan, count + 1);
    let response = engine
        .invoke("ci_status", json!({"mode":"freshness","timeoutMs":20000}))
        .unwrap();
    assert_eq!(response["status"], "ok", "{response}");
    let data = &response["data"];
    assert_eq!(data["baselineSnapshotId"], baseline.id);
    assert_eq!(data["fresh"], false);
    assert_eq!(data["added"][0]["path"], "added.rs");
    assert_eq!(data["deleted"][0]["path"], "deleted.rs");
    assert_eq!(data["changed"][0]["path"], "changed.rs");
    assert_eq!(engine.store.latest().unwrap(), latest);
    let after: i64 = engine
        .store
        .db
        .query_row("SELECT COUNT(*) FROM snapshots", [], |r| r.get(0))
        .unwrap();
    assert_eq!(after, count_after_plan);
}

#[test]
fn unindexed_freshness_is_explicit_and_does_not_create_index() {
    let root = TempDir::new().unwrap();
    fs::write(root.path().join("main.rs"), "fn main() {}\n").unwrap();
    let mut engine = Engine::open(root.path()).unwrap();
    let response = engine
        .invoke("ci_status", json!({"mode":"freshness"}))
        .unwrap();
    assert_eq!(response["status"], "ok", "{response}");
    assert_eq!(response["data"]["indexed"], false);
    assert!(response["data"]["fresh"].is_null());
    assert_eq!(engine.store.latest().unwrap(), None);
}
