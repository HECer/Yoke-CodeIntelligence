use serde_json::json;
use std::fs;
use tempfile::TempDir;
use yoke::{Engine, capture::Deadline};

fn fixture() -> (TempDir, Engine, String) {
    let root = TempDir::new().unwrap();
    fs::write(
        root.path().join("main.rs"),
        "fn helper() {}\nfn caller() { helper(); }\n",
    )
    .unwrap();
    let mut engine = Engine::open(root.path()).unwrap();
    let (snapshot, _, _, _) = engine.index(&Deadline::new(20_000)).unwrap();
    (root, engine, snapshot.id)
}

#[test]
fn a_corrupted_snapshot_payload_cannot_alias_a_different_lookup_identity() {
    let (root, mut engine, first) = fixture();
    let changed = "fn newer_source() {}\n";
    fs::write(root.path().join("main.rs"), changed).unwrap();
    let (second, _, _, _) = engine.index(&Deadline::new(20_000)).unwrap();
    engine
        .store
        .db
        .execute(
            "UPDATE snapshots SET value=json_set(value,'$.id',?1) WHERE id=?2",
            [&second.id, &first],
        )
        .unwrap();
    assert!(
        engine.store.snapshot(&first).is_err(),
        "Corrupted payload silently changed the lookup identity"
    );
    let response = engine
        .invoke("ci_read", json!({"snapshotId":first,"path":"main.rs"}))
        .unwrap();
    assert_eq!(response["status"], "error", "{response}");
    assert_eq!(response["snapshotId"], first);
    assert_eq!(engine.store.snapshot(&second.id).unwrap().id, second.id);
    assert_eq!(
        fs::read_to_string(root.path().join("main.rs")).unwrap(),
        changed
    );
}

#[test]
fn snapshot_workspace_mismatch_is_rejected_before_source_retrieval() {
    let (_root, mut engine, id) = fixture();
    engine
        .store
        .db
        .execute(
            "UPDATE snapshots SET value=json_set(value,'$.workspaceId',?1) WHERE id=?2",
            [&"f".repeat(64), &id],
        )
        .unwrap();
    assert!(
        engine.store.snapshot(&id).is_err(),
        "Foreign workspace attribution was accepted"
    );
    let response = engine
        .invoke("ci_read", json!({"snapshotId":id,"path":"main.rs"}))
        .unwrap();
    assert_eq!(response["status"], "error", "{response}");
}

#[test]
fn graph_lookup_and_every_evidence_member_keep_the_same_snapshot_identity() {
    let (_root, engine, id) = fixture();
    let graph = engine.store.graph(&id).unwrap();
    assert!(!graph.entities.is_empty());
    assert!(!graph.observations.is_empty());
    assert!(!graph.evidence.is_empty());
    let original: String = engine
        .store
        .db
        .query_row("SELECT value FROM graphs WHERE id=?1", [&id], |row| {
            row.get(0)
        })
        .unwrap();
    for field in [
        "$.snapshotId",
        "$.entities[0].snapshotId",
        "$.observations[0].snapshotId",
        "$.evidence[0].snapshotId",
    ] {
        engine
            .store
            .db
            .execute("UPDATE graphs SET value=?1 WHERE id=?2", [&original, &id])
            .unwrap();
        engine
            .store
            .db
            .execute(
                "UPDATE graphs SET value=json_set(value,?1,?2) WHERE id=?3",
                [field, &"f".repeat(64), &id],
            )
            .unwrap();
        assert!(
            engine.store.graph(&id).is_err(),
            "Mixed graph snapshot scope accepted: {field}"
        );
    }
    engine
        .store
        .db
        .execute("UPDATE graphs SET value=?1 WHERE id=?2", [&original, &id])
        .unwrap();
    assert_eq!(engine.store.graph(&id).unwrap().snapshot_id, id);
}

#[test]
fn valid_disk_and_derived_snapshot_identities_remain_readable() {
    let (root, mut engine, id) = fixture();
    let snapshot = engine.store.snapshot(&id).unwrap();
    let response = engine.invoke("ci_change_prepare", json!({"snapshotId":id,"edits":[{"kind":"replace","path":"main.rs","expectedHash":snapshot.files[0].hash,"startByte":3,"endByte":9,"text":"renamed"}]})).unwrap();
    assert_eq!(response["status"], "ok", "{response}");
    let derived = response["data"]["plan"]["resultSnapshotId"]
        .as_str()
        .unwrap();
    let result = engine.store.snapshot(derived).unwrap();
    assert_eq!(result.id, derived);
    assert_eq!(result.workspace_id, engine.store.workspace_id);
    assert_eq!(result.kind, "derived");
    let graph = engine.store.graph(derived).unwrap();
    assert_eq!(graph.snapshot_id, derived);
    assert!(
        graph
            .entities
            .iter()
            .all(|entity| entity.snapshot_id == derived)
    );
    assert!(
        graph
            .observations
            .iter()
            .all(|observation| observation.snapshot_id == derived)
    );
    assert!(
        graph
            .evidence
            .iter()
            .all(|evidence| evidence.snapshot_id == derived)
    );
    assert_eq!(
        fs::read_to_string(root.path().join("main.rs")).unwrap(),
        "fn helper() {}\nfn caller() { helper(); }\n"
    );
}
