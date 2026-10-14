use serde_json::Value;
use std::fs;
use yoke::{Engine, capture::Deadline};

#[test]
fn first_publication_time_is_durable_and_never_refreshed_by_republication() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("main.rs"), "fn main() {}\n").unwrap();
    let mut engine = Engine::open(dir.path()).unwrap();
    let deadline = Deadline::new(10_000);
    let (snapshot, graph, _, _) = engine.index(&deadline).unwrap();
    let first: Value = engine
        .store
        .get_record("snapshot-published", &snapshot.id)
        .expect("new snapshots need a durable age receipt");
    assert_eq!(first["snapshotId"], snapshot.id);
    assert_eq!(first["workspaceId"], snapshot.workspace_id);
    assert!(first["publishedAtMs"].as_u64().unwrap() > 1_700_000_000_000);
    engine.store.publish(&snapshot, &graph).unwrap();
    let second: Value = engine
        .store
        .get_record("snapshot-published", &snapshot.id)
        .unwrap();
    assert_eq!(first, second);
    engine
        .store
        .db
        .execute("DELETE FROM records WHERE kind='snapshot-published'", [])
        .unwrap();
    engine.store.publish(&snapshot, &graph).unwrap();
    assert!(
        engine
            .store
            .get_record::<Value>("snapshot-published", &snapshot.id)
            .is_err(),
        "legacy age must remain unknown"
    );
}
