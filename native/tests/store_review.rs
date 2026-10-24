use std::fs;
use yoke::{
    Engine,
    capture::{self, Deadline},
    model::{Snapshot, SourceFile},
};

#[test]
fn source_blob_artifact_fallback_cannot_bypass_captured_path_authority() {
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("main.rs"), "fn visible() {}\n").unwrap();
    let mut engine = Engine::open(root.path()).unwrap();
    let deadline = Deadline::new(20000);
    let ordinary = engine.index(&deadline).unwrap().0;
    assert_eq!(
        engine.store.read_artifact(&ordinary.files[0].hash).unwrap(),
        b"fn visible() {}\n"
    );
    let content = b"private outside captured authority";
    let hash = engine.store.blob(content).unwrap();
    assert!(engine.store.read_artifact(&hash).is_err());
    let malformed = Snapshot {
        id: "e".repeat(64),
        workspace_id: engine.store.workspace_id.clone(),
        kind: "disk".into(),
        files: vec![SourceFile {
            path: ".env".into(),
            language: "text".into(),
            hash: hash.clone(),
            size: content.len(),
            mode: 0o644,
        }],
        omissions: vec![],
        capture_consistency: "stable".into(),
    };
    engine
        .store
        .db
        .execute(
            "INSERT INTO snapshots VALUES(?1,?2)",
            rusqlite::params![malformed.id, serde_json::to_string(&malformed).unwrap()],
        )
        .unwrap();
    assert!(capture::source_path_policy(&engine.store, &malformed, ".env").is_err());
    assert!(
        engine.store.read_artifact(&hash).is_err(),
        "Hash-only artifact fallback granted a protected source whose captured proof rejects it"
    );
}
