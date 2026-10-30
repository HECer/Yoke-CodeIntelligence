use serde_json::{Value, json};
use yoke::{
    Engine,
    analysis::AnalyzerEpoch,
    capture::Deadline,
    model::{Snapshot, identity},
};

fn restored() -> (tempfile::TempDir, Engine, Snapshot) {
    let bundle: Value =
        serde_json::from_str(include_str!("fixtures/analyzer8/authentic-analyzer8.json")).unwrap();
    let case = &bundle["cases"][0];
    let root = tempfile::tempdir().unwrap();
    let mut engine = Engine::open(root.path()).unwrap();
    engine.store.workspace_id = case["workspaceId"].as_str().unwrap().into();
    for table in ["snapshots", "graphs", "cache", "records"] {
        for row in case[table].as_array().unwrap() {
            if table == "records" {
                engine
                    .store
                    .db
                    .execute(
                        "INSERT INTO records(kind,id,value) VALUES(?1,?2,?3)",
                        rusqlite::params![
                            row["kind"].as_str().unwrap(),
                            row["id"].as_str().unwrap(),
                            row["value"].as_str().unwrap()
                        ],
                    )
                    .unwrap();
            } else {
                let column = if table == "cache" { "key" } else { "id" };
                engine
                    .store
                    .db
                    .execute(
                        &format!("INSERT INTO {table}({column},value) VALUES(?1,?2)"),
                        rusqlite::params![
                            row["id"].as_str().unwrap(),
                            row["value"].as_str().unwrap()
                        ],
                    )
                    .unwrap();
            }
        }
    }
    for (hash, text) in case["sources"].as_object().unwrap() {
        assert_eq!(
            engine
                .store
                .blob(text.as_str().unwrap().as_bytes())
                .unwrap(),
            *hash
        );
    }
    let snapshot = case["snapshots"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| serde_json::from_str::<Snapshot>(row["value"].as_str().unwrap()).unwrap())
        .find(|s| s.kind == "disk")
        .unwrap();
    (root, engine, snapshot)
}

#[test]
fn default_engine_reanalysis_uses_exact_proven_epoch_instead_of_active_epoch() {
    let (_root, mut engine, snapshot) = restored();
    let deadline = Deadline::new(20000);
    let old_graph = engine.store.graph(&snapshot.id).unwrap();
    let file = snapshot.files.iter().find(|f| f.path == "main.rs").unwrap();
    let edit = json!({"edits":[{"kind":"replace","path":file.path,"expectedHash":file.hash,"startByte":0,"endByte":0,"text":"// epoch fixture\n"}]});
    let result = yoke::changes::prepare_at(
        AnalyzerEpoch::V9,
        &mut engine.store,
        &snapshot,
        &old_graph,
        &edit,
        &deadline,
    )
    .unwrap();
    let id = result.result_snapshot_id.as_str();
    let derived = engine.store.snapshot(id).unwrap();
    let expected = identity(&engine.store.graph(id).unwrap());
    engine
        .store
        .db
        .execute("DELETE FROM graphs WHERE id=?1", [id])
        .unwrap();
    let (_, actual, _, _) = engine.analyze_snapshot(derived, &deadline).unwrap();
    assert_eq!(
        identity(&actual),
        expected,
        "A proven V9 snapshot must never be re-extracted under active V8 policy"
    );
    assert!(
        actual
            .entities
            .iter()
            .filter(|e| e.producer.starts_with("yoke-tree-sitter-"))
            .all(|e| e.producer == "yoke-tree-sitter-9")
    );
}

#[test]
fn default_engine_refuses_missing_graph_with_unproven_capture_epoch() {
    let (_root, mut engine, mut snapshot) = restored();
    snapshot.id = identity(&("unproven-epoch", &snapshot));
    engine
        .store
        .db
        .execute(
            "INSERT INTO snapshots(id,value) VALUES(?1,?2)",
            rusqlite::params![snapshot.id, serde_json::to_string(&snapshot).unwrap()],
        )
        .unwrap();
    let result = engine.analyze_snapshot(snapshot.clone(), &Deadline::new(20000));
    assert!(
        result.is_err(),
        "Unproven snapshot IDs must not receive active analyzer facts"
    );
    assert!(engine.store.graph(&snapshot.id).is_err());
}

#[test]
fn existing_graph_fastpath_rejects_a_different_manifest_under_the_same_id() {
    let (_root, mut engine, mut snapshot) = restored();
    let original = identity(&engine.store.graph(&snapshot.id).unwrap());
    snapshot.files[0].hash = yoke::model::hash(b"different caller supplied source");
    assert!(
        engine
            .analyze_snapshot(snapshot.clone(), &Deadline::new(20000))
            .is_err(),
        "A stored graph must not authenticate changed caller-provided source membership"
    );
    assert_eq!(
        identity(&engine.store.graph(&snapshot.id).unwrap()),
        original
    );
}
