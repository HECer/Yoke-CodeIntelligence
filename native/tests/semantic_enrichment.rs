use serde_json::json;
use std::fs;
use tempfile::TempDir;
use yoke::{Engine, capture::Deadline, model::identity, semantic_enrichment};

#[test]
fn rehashed_semantic_payloads_revalidate_selection_and_canonical_binding_claims() {
    let root = TempDir::new().unwrap();
    fs::write(root.path().join("main.rs"), "fn helper() {}\n").unwrap();
    let mut engine = Engine::open(root.path()).unwrap();
    let deadline = Deadline::new(20000);
    let (snapshot, graph, _, _) = engine.index(&deadline).unwrap();
    let file = &snapshot.files[0];
    let node = json!({"snapshotId":snapshot.id,"name":"helper","kind":12,"path":"main.rs","contentHash":file.hash,"startByte":0,"endByte":14,"selectionStartByte":3,"selectionEndByte":9});
    let result = semantic_enrichment::persist(&engine.store, &snapshot, &graph, &json!({"snapshotId":snapshot.id,"operation":"outgoingCalls","producer":"owned-selection-fixture","serverProfileHash":"d".repeat(64),"items":[{"root":node,"to":node,"fromRanges":[]}]}), &deadline).unwrap();
    let original = semantic_enrichment::load(
        &engine.store,
        result["overlayId"].as_str().unwrap(),
        &deadline,
    )
    .unwrap();
    let mut accepted = vec![];
    for defect in ["selection", "canonicalBinding", "schema"] {
        let mut raw = original.clone();
        raw.as_object_mut().unwrap().remove("overlayId");
        match defect {
            "selection" => raw["nodes"][0]["selectionEndByte"] = json!(99999),
            "canonicalBinding" => raw["nodes"][0]["baseEntityIds"] = json!(["e".repeat(64)]),
            _ => raw["futureSourceDependencies"] = json!(["unrecognized"]),
        }
        let id = identity(&raw);
        raw["overlayId"] = json!(id);
        let artifact = engine
            .store
            .artifact(&serde_json::to_vec(&raw).unwrap())
            .unwrap();
        let record = json!({"overlayId":id,"artifactId":artifact,"snapshotId":snapshot.id,"baseGraphHash":identity(&graph),"sourceBindings":raw["sourceBindings"]});
        engine
            .store
            .record(semantic_enrichment::RECORD_KIND, &id, &record)
            .unwrap();
        if semantic_enrichment::load(&engine.store, &id, &deadline).is_ok() {
            accepted.push(defect);
        }
    }
    assert!(
        accepted.is_empty(),
        "semantic load accepted rehashed invalid source claims: {accepted:?}"
    );
}

#[test]
fn semantic_overlay_keeps_parallel_calls_and_never_mutates_the_published_graph() {
    let root = TempDir::new().unwrap();
    fs::write(
        root.path().join("main.rs"),
        "fn helper() {}\nfn main() { helper(); }\n",
    )
    .unwrap();
    let mut engine = Engine::open(root.path()).unwrap();
    let deadline = Deadline::new(20000);
    let (snapshot, graph, _, _) = engine.index(&deadline).unwrap();
    let before = identity(&graph);
    let hash = &snapshot.files[0].hash;
    let helper = json!({"snapshotId":snapshot.id,"name":"helper","kind":12,"path":"main.rs","contentHash":hash,"startByte":0,"endByte":14,"selectionStartByte":3,"selectionEndByte":9});
    let caller = json!({"snapshotId":snapshot.id,"name":"main","kind":12,"path":"main.rs","contentHash":hash,"startByte":15,"endByte":38,"selectionStartByte":18,"selectionEndByte":22});
    let range = json!({"snapshotId":snapshot.id,"path":"main.rs","contentHash":hash,"startByte":27,"endByte":33});
    let input = json!({"snapshotId":snapshot.id,"operation":"incomingCalls","producer":"owned-lsp-fixture","serverProfileHash":"a".repeat(64),"items":[{"root":helper,"from":caller,"fromRanges":[range,range]}],"complete":false});
    let result =
        semantic_enrichment::persist(&engine.store, &snapshot, &graph, &input, &deadline).unwrap();
    let overlay = semantic_enrichment::load(
        &engine.store,
        result["overlayId"].as_str().unwrap(),
        &deadline,
    )
    .unwrap();
    assert_eq!(overlay["observations"].as_array().unwrap().len(), 2);
    assert_ne!(
        overlay["observations"][0]["id"],
        overlay["observations"][1]["id"]
    );
    assert_eq!(overlay["evidence"][0]["startByte"], 27);
    assert_eq!(overlay["evidence"][0]["contentHash"], hash.as_str());
    let mut another_profile = input.clone();
    another_profile["serverProfileHash"] = json!("c".repeat(64));
    let another = semantic_enrichment::persist(
        &engine.store,
        &snapshot,
        &graph,
        &another_profile,
        &deadline,
    )
    .unwrap();
    let another = semantic_enrichment::load(
        &engine.store,
        another["overlayId"].as_str().unwrap(),
        &deadline,
    )
    .unwrap();
    assert_ne!(
        overlay["nodes"][0]["id"], another["nodes"][0]["id"],
        "Provider contexts collapsed into one semantic node identity"
    );
    assert_eq!(identity(&engine.store.graph(&snapshot.id).unwrap()), before);
    let mut hostile = input.clone();
    hostile["items"][0]["from"]["contentHash"] = json!("0".repeat(64));
    assert!(
        semantic_enrichment::persist(&engine.store, &snapshot, &graph, &hostile, &deadline)
            .is_err()
    );
    let mut rebound = snapshot.clone();
    rebound.files[0].hash = "0".repeat(64);
    assert!(
        semantic_enrichment::persist(&engine.store, &rebound, &graph, &input, &deadline).is_err()
    );
}

#[test]
fn overlay_publication_is_atomic_and_existing_records_are_immutable() {
    let root = TempDir::new().unwrap();
    fs::write(root.path().join("main.rs"), "fn main() {}\n").unwrap();
    let mut engine = Engine::open(root.path()).unwrap();
    let deadline = Deadline::new(20000);
    let (snapshot, graph, _, _) = engine.index(&deadline).unwrap();
    let input = json!({"snapshotId":snapshot.id,"operation":"incomingCalls","producer":"owned-fixture","serverProfileHash":"b".repeat(64),"items":[],"complete":false});
    engine.store.db.execute_batch("CREATE TRIGGER reject_overlay BEFORE INSERT ON records WHEN NEW.kind='native_semantic_overlay_v1' BEGIN SELECT RAISE(ABORT,'owned injected publication failure'); END;").unwrap();
    let before: i64 = engine
        .store
        .db
        .query_row(
            "SELECT count(*) FROM records WHERE kind='artifact'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert!(
        semantic_enrichment::persist(&engine.store, &snapshot, &graph, &input, &deadline).is_err()
    );
    let after: i64 = engine
        .store
        .db
        .query_row(
            "SELECT count(*) FROM records WHERE kind='artifact'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(
        after, before,
        "Failed publication registered a durable orphan artifact"
    );
    engine
        .store
        .db
        .execute_batch("DROP TRIGGER reject_overlay;")
        .unwrap();
    let value =
        semantic_enrichment::persist(&engine.store, &snapshot, &graph, &input, &deadline).unwrap();
    let id = value["overlayId"].as_str().unwrap();
    engine
        .store
        .db
        .execute(
            "UPDATE records SET value='{}' WHERE kind=?1 AND id=?2",
            rusqlite::params![semantic_enrichment::RECORD_KIND, id],
        )
        .unwrap();
    assert!(
        semantic_enrichment::persist(&engine.store, &snapshot, &graph, &input, &deadline).is_err(),
        "Republication overwrote a colliding immutable record"
    );
    let raw: String = engine
        .store
        .db
        .query_row(
            "SELECT value FROM records WHERE kind=?1 AND id=?2",
            rusqlite::params![semantic_enrichment::RECORD_KIND, id],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(raw, "{}");
}
