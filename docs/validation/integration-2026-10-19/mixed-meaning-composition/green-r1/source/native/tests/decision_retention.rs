use serde_json::json;
use std::fs;
use tempfile::TempDir;
use yoke::{
    Engine,
    capture::Deadline,
    decision_records::{self, DecisionStore, Draft, RecordKind, Scope},
    retention,
};

fn draft() -> Draft {
    Draft {
        key: "snapshot-policy".into(),
        kind: RecordKind::Decision,
        title: "Keep source immutable".into(),
        body: "Historical decisions retain their source evidence.".into(),
        author: "Ada".into(),
        date: "2026-10-15".into(),
        paths: vec!["main.rs".into()],
        cost: None,
        rules: vec![],
    }
}

#[test]
fn authored_revisions_are_known_roots_and_retain_their_historical_source() {
    let root = TempDir::new().unwrap();
    fs::write(root.path().join("main.rs"), "fn before() {}\n").unwrap();
    let mut engine = Engine::open(root.path()).unwrap();
    let d = Deadline::new(30_000);
    let (before, _, _, _) = engine.index(&d).unwrap();
    DecisionStore::new(&engine.store, &engine.store, Scope::Repository, &d)
        .unwrap()
        .write(&before, draft(), None, &d)
        .unwrap();
    fs::write(root.path().join("main.rs"), "fn after() {}\n").unwrap();
    engine.index(&d).unwrap();
    let plan = retention::plan(&engine.store, &retention::Policy::default(), &d).unwrap();
    assert_eq!(plan["unknownDependencyKinds"], json!([]));
    assert!(
        !plan["snapshotIds"]
            .as_array()
            .unwrap()
            .iter()
            .any(|id| id == &before.id)
    );
    let purge = retention::purge_plan(&engine.store, 0, &d).unwrap();
    assert_eq!(purge["unknownDependencyKinds"], json!([]));
}

#[test]
fn widened_authored_revision_cannot_authorize_history_retirement_or_blob_purge() {
    let root = TempDir::new().unwrap();
    fs::write(root.path().join("main.rs"), "fn source() {}\n").unwrap();
    let mut engine = Engine::open(root.path()).unwrap();
    let d = Deadline::new(30_000);
    let (snapshot, _, _, _) = engine.index(&d).unwrap();
    let revision = DecisionStore::new(&engine.store, &engine.store, Scope::Repository, &d)
        .unwrap()
        .write(&snapshot, draft(), None, &d)
        .unwrap();
    let mut raw: serde_json::Value = engine
        .store
        .get_record(decision_records::REVISION_KIND, &revision.id)
        .unwrap();
    raw["unrecognizedDependencies"] = json!(["future"]);
    engine
        .store
        .db
        .execute(
            "UPDATE records SET value=?1 WHERE kind=?2 AND id=?3",
            rusqlite::params![
                raw.to_string(),
                decision_records::REVISION_KIND,
                revision.id
            ],
        )
        .unwrap();
    for result in [
        retention::plan(&engine.store, &retention::Policy::default(), &d),
        retention::purge_plan(&engine.store, 0, &d),
    ] {
        assert!(
            result.is_err()
                || !result.unwrap()["unknownDependencyKinds"]
                    .as_array()
                    .unwrap()
                    .is_empty()
        );
    }
}
