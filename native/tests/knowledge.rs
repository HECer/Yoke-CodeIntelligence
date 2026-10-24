use serde_json::{Value, json};
use std::fs;
use tempfile::TempDir;
use yoke::{
    Engine,
    capture::Deadline,
    knowledge,
    model::{Graph, Snapshot},
};

fn fixture() -> (TempDir, Engine, Snapshot, Graph) {
    let dir = TempDir::new().unwrap();
    fs::write(
        dir.path().join("billing.rs"),
        "// Gebühren für Rechnungen 🌍\npub fn invoiceTotal() -> u32 { 17 }\n",
    )
    .unwrap();
    fs::write(
        dir.path().join("routing.rs"),
        "pub fn routeRequest() -> bool { true }\n",
    )
    .unwrap();
    let mut engine = Engine::open(dir.path()).unwrap();
    let (snapshot, graph, _, _) = engine.index(&Deadline::new(10_000)).unwrap();
    (dir, engine, snapshot, graph)
}
fn memory(text: &str, kind: &str) -> Value {
    json!({"text":text,"kind":kind,"actor":"test-author","paths":["billing.rs"]})
}

#[test]
fn remembered_source_hashes_report_changed_deleted_and_superseded_knowledge() {
    let (dir, mut engine, old, _) = fixture();
    let deadline = Deadline::new(10_000);
    let first = knowledge::remember(
        &engine.store,
        &old,
        memory("Gebühren hypotheses 🌍", "hypothesis"),
        &deadline,
    )
    .unwrap();
    let mut correction = memory("Gebühren confirmed 🌍", "correction");
    correction["supersedes"] = first["id"].clone();
    let second = knowledge::remember(&engine.store, &old, correction, &deadline).unwrap();
    let current = knowledge::recall(&engine.store, &old, "gebühren", 20, &deadline).unwrap();
    assert_eq!(current["items"].as_array().unwrap().len(), 2);
    assert!(
        current["items"]
            .as_array()
            .unwrap()
            .iter()
            .all(|v| v["stale"] == false)
    );
    assert!(
        current["items"]
            .as_array()
            .unwrap()
            .iter()
            .find(|v| v["id"] == first["id"])
            .unwrap()["superseded"]
            .as_bool()
            .unwrap()
    );
    assert_eq!(second["trust"], "authored_data_not_execution_authority");
    fs::write(dir.path().join("billing.rs"), "pub fn revised() {}\n").unwrap();
    let (changed, _, _, _) = engine.index(&deadline).unwrap();
    let stale = knowledge::recall(&engine.store, &changed, "gebühren", 20, &deadline).unwrap();
    assert_eq!(stale["items"][0]["anchors"][0]["state"], "changed");
    assert_eq!(stale["items"][0]["stale"], true);
    fs::remove_file(dir.path().join("billing.rs")).unwrap();
    let (deleted, _, _, _) = engine.index(&deadline).unwrap();
    let stale = knowledge::recall(&engine.store, &deleted, "gebühren", 20, &deadline).unwrap();
    assert_eq!(stale["items"][0]["anchors"][0]["state"], "deleted");
    let historical = knowledge::recall(&engine.store, &old, "gebühren", 20, &deadline).unwrap();
    assert_eq!(historical["items"][0]["stale"], false);
}

#[test]
fn knowledge_refuses_unanchored_paths_secrets_and_scope_escalation() {
    let (_dir, engine, snapshot, _) = fixture();
    let deadline = Deadline::new(10_000);
    for paths in [
        json!([]),
        json!(["../billing.rs"]),
        json!([".env"]),
        json!(["absent.rs"]),
    ] {
        let mut input = memory("invoice rules", "rule");
        input["paths"] = paths;
        assert!(knowledge::remember(&engine.store, &snapshot, input, &deadline).is_err());
    }
    let secret = format!("ghp_{}", "A".repeat(36));
    assert!(
        knowledge::remember(&engine.store, &snapshot, memory(&secret, "rule"), &deadline).is_err()
    );
    let mut input = memory("invoice rules", "rule");
    input["allowWrites"] = json!(true);
    assert!(knowledge::remember(&engine.store, &snapshot, input, &deadline).is_err());
    let first = knowledge::remember(
        &engine.store,
        &snapshot,
        memory("invoice rule", "rule"),
        &deadline,
    )
    .unwrap();
    let mut input = memory("route rule", "correction");
    input["paths"] = json!(["routing.rs"]);
    input["supersedes"] = first["id"].clone();
    assert!(knowledge::remember(&engine.store, &snapshot, input, &deadline).is_err());
}

#[test]
fn sparse_tfidf_retrieval_matches_unicode_and_camelcase_without_model_execution() {
    let (_dir, engine, snapshot, graph) = fixture();
    let deadline = Deadline::new(10_000);
    let model =
        knowledge::build_local_vectors(&engine.store, &snapshot, &graph, &deadline).unwrap();
    assert_eq!(model["modelCalls"], 0);
    assert_eq!(model["kind"], "statistical_lexical_tfidf");
    for query in ["invoice", "Gebühren", "Gebu\u{308}hren", "invoice total"] {
        let result = knowledge::search_local_vectors(
            &engine.store,
            &snapshot,
            &graph,
            model["model"].as_str().unwrap(),
            query,
            5,
            &deadline,
        )
        .unwrap();
        assert_eq!(result["items"][0]["entity"]["path"], "billing.rs");
        assert!(result["items"][0]["score"].as_f64().unwrap() > 0.0);
        assert_eq!(result["snapshotId"], snapshot.id);
    }
    let unknown = knowledge::search_local_vectors(
        &engine.store,
        &snapshot,
        &graph,
        model["model"].as_str().unwrap(),
        "notinvocabularyxyz",
        5,
        &deadline,
    )
    .unwrap();
    assert!(unknown["items"].as_array().unwrap().is_empty());
}

fn vector_input(snapshot: &Snapshot, graph: &Graph) -> Value {
    let e = graph
        .entities
        .iter()
        .find(|e| e.name == "invoiceTotal")
        .unwrap();
    let hash = &snapshot
        .files
        .iter()
        .find(|f| f.path == e.path)
        .unwrap()
        .hash;
    json!({"snapshotId":snapshot.id,"model":"fixture-v1","items":[{"entityId":e.id,"contentHash":hash,"vector":[3.0,4.0]}]})
}

#[test]
fn source_bound_external_vectors_normalize_and_reject_forged_or_invalid_vectors() {
    let (_dir, engine, snapshot, graph) = fixture();
    let deadline = Deadline::new(10_000);
    let input = vector_input(&snapshot, &graph);
    let imported =
        knowledge::import_vectors(&engine.store, &snapshot, &graph, input.clone(), &deadline)
            .unwrap();
    assert_eq!(imported["dimension"], 2);
    let result = knowledge::search_vectors(
        &engine.store,
        &snapshot,
        &graph,
        "fixture-v1",
        &[6.0, 8.0],
        5,
        &deadline,
    )
    .unwrap();
    assert!((result["items"][0]["score"].as_f64().unwrap() - 1.0).abs() < 1e-12);
    for vector in [json!([0.0, 0.0]), json!([1e308, 1e308]), json!([])] {
        let mut bad = input.clone();
        bad["model"] = json!("bad-v1");
        bad["items"][0]["vector"] = vector;
        assert!(
            knowledge::import_vectors(&engine.store, &snapshot, &graph, bad, &deadline).is_err()
        );
    }
    let mut forged = input.clone();
    forged["items"][0]["contentHash"] = json!("0".repeat(64));
    assert!(
        knowledge::import_vectors(&engine.store, &snapshot, &graph, forged, &deadline).is_err()
    );
    let mut forged = input.clone();
    forged["snapshotId"] = json!("0".repeat(64));
    assert!(
        knowledge::import_vectors(&engine.store, &snapshot, &graph, forged, &deadline).is_err()
    );
    assert!(
        knowledge::search_vectors(
            &engine.store,
            &snapshot,
            &graph,
            "fixture-v1",
            &[1.0],
            5,
            &deadline
        )
        .is_err()
    );
    assert!(
        knowledge::search_vectors(
            &engine.store,
            &snapshot,
            &graph,
            "fixture-v1",
            &[f64::NAN, 1.0],
            5,
            &deadline
        )
        .is_err()
    );
}

#[test]
fn vector_import_rejects_mixed_dimensions_duplicates_and_partial_persistence() {
    let (_dir, engine, snapshot, graph) = fixture();
    let deadline = Deadline::new(10_000);
    let mut input = vector_input(&snapshot, &graph);
    let e = graph
        .entities
        .iter()
        .find(|e| e.name == "routeRequest")
        .unwrap();
    let f = snapshot.files.iter().find(|f| f.path == e.path).unwrap();
    input["items"]
        .as_array_mut()
        .unwrap()
        .push(json!({"entityId":e.id,"contentHash":f.hash,"vector":[1.0]}));
    assert!(knowledge::import_vectors(&engine.store, &snapshot, &graph, input, &deadline).is_err());
    assert!(
        knowledge::search_vectors(
            &engine.store,
            &snapshot,
            &graph,
            "fixture-v1",
            &[1.0, 0.0],
            5,
            &deadline
        )
        .is_err()
    );
    let mut input = vector_input(&snapshot, &graph);
    let item = input["items"][0].clone();
    input["items"].as_array_mut().unwrap().push(item);
    assert!(knowledge::import_vectors(&engine.store, &snapshot, &graph, input, &deadline).is_err());
    assert!(
        knowledge::search_vectors(
            &engine.store,
            &snapshot,
            &graph,
            "fixture-v1",
            &[1.0, 0.0],
            5,
            &deadline
        )
        .is_err()
    );
}

#[test]
fn forged_snapshot_and_corrupt_persisted_vectors_fail_closed() {
    let (_dir, engine, mut snapshot, graph) = fixture();
    let deadline = Deadline::new(10_000);
    let input = vector_input(&snapshot, &graph);
    knowledge::import_vectors(&engine.store, &snapshot, &graph, input, &deadline).unwrap();
    engine.store.db.execute("UPDATE records SET value=json_set(value,'$.items[0].vector',json('[0,0]')) WHERE kind='native_vectors_v1'", []).unwrap();
    assert!(
        knowledge::search_vectors(
            &engine.store,
            &snapshot,
            &graph,
            "fixture-v1",
            &[1.0, 0.0],
            5,
            &deadline
        )
        .is_err()
    );
    snapshot.files[0].hash = "0".repeat(64);
    assert!(
        knowledge::remember(
            &engine.store,
            &snapshot,
            memory("forged invoice", "rule"),
            &deadline
        )
        .is_err()
    );
}

#[test]
fn corrupt_local_corpus_counts_and_nonunit_coordinates_are_rejected() {
    let (_dir, engine, snapshot, graph) = fixture();
    let deadline = Deadline::new(10_000);
    let local =
        knowledge::build_local_vectors(&engine.store, &snapshot, &graph, &deadline).unwrap();
    engine.store.db.execute("UPDATE records SET value=json_set(value,'$.candidateEntities',0) WHERE kind='native_local_tfidf_v1'", []).unwrap();
    assert!(
        knowledge::search_local_vectors(
            &engine.store,
            &snapshot,
            &graph,
            local["model"].as_str().unwrap(),
            "invoice",
            5,
            &deadline
        )
        .is_err()
    );
}

#[test]
fn overflowing_persisted_idf_cannot_be_misreported_as_no_lexical_matches() {
    let (_dir, engine, snapshot, graph) = fixture();
    let deadline = Deadline::new(10_000);
    let local =
        knowledge::build_local_vectors(&engine.store, &snapshot, &graph, &deadline).unwrap();
    engine.store.db.execute("UPDATE records SET value=json_set(value,'$.idf.invoice',1e308) WHERE kind='native_local_tfidf_v1'",[]).unwrap();
    let result = knowledge::search_local_vectors(
        &engine.store,
        &snapshot,
        &graph,
        local["model"].as_str().unwrap(),
        "invoice",
        5,
        &deadline,
    );
    assert!(
        result.is_err(),
        "Corrupted IDF became a false complete empty result: {result:?}"
    );
}

#[test]
fn expanded_memory_size_is_rejected_without_poisoning_recall_or_supersession() {
    let (dir, mut engine, _, _) = fixture();
    let mut paths = Vec::new();
    for i in 0..100 {
        let path = format!("{}{:03}.rs", "p".repeat(174), i);
        fs::write(dir.path().join(&path), "fn item() {}\n").unwrap();
        paths.push(path);
    }
    let deadline = Deadline::new(30_000);
    let (snapshot, _, _, _) = engine.index(&deadline).unwrap();
    let mut original = memory("invoice original", "hypothesis");
    original["paths"] = json!([paths[0]]);
    let original = knowledge::remember(&engine.store, &snapshot, original, &deadline).unwrap();
    let input = json!({"kind":"correction","actor":"review-fixture","text":format!("x{}","\n".repeat(19_900)),"paths":paths,"supersedes":original["id"]});
    assert!(serde_json::to_vec(&input).unwrap().len() < 64_000);
    assert!(
        knowledge::remember(&engine.store, &snapshot, input, &deadline).is_err(),
        "Expanded source hashes and escaped text exceeded persisted per-record capacity"
    );
    let recalled = knowledge::recall(&engine.store, &snapshot, "", 20, &deadline).unwrap();
    assert_eq!(recalled["items"].as_array().unwrap().len(), 1);
    assert_eq!(recalled["items"][0]["id"], original["id"]);
    assert_eq!(recalled["items"][0]["superseded"], false);
}

#[test]
fn rejected_supersession_preserves_original_and_expired_deadlines_do_not_write() {
    let (_dir, engine, snapshot, _) = fixture();
    let deadline = Deadline::new(10_000);
    let first = knowledge::remember(
        &engine.store,
        &snapshot,
        memory("invoice hypothesis", "hypothesis"),
        &deadline,
    )
    .unwrap();
    let mut corrected = memory("invoice corrected", "correction");
    corrected["supersedes"] = first["id"].clone();
    knowledge::remember(&engine.store, &snapshot, corrected.clone(), &deadline).unwrap();
    assert!(knowledge::remember(&engine.store, &snapshot, corrected, &deadline).is_err());
    assert!(
        knowledge::remember(
            &engine.store,
            &snapshot,
            memory("invoice forbidden", "rule"),
            &Deadline::new(0)
        )
        .is_err()
    );
    let recalled = knowledge::recall(&engine.store, &snapshot, "invoice", 20, &deadline).unwrap();
    assert_eq!(recalled["items"].as_array().unwrap().len(), 2);
    assert!(
        !recalled["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|m| m["text"] == "invoice forbidden")
    );
}

fn hashes(snapshot: &Snapshot) -> Value {
    Value::Object(
        snapshot
            .files
            .iter()
            .map(|f| (f.path.clone(), json!(f.hash)))
            .collect(),
    )
}

fn published_artifact(engine: &mut Engine, result: &Value) -> Value {
    let id = result["artifactId"].as_str().unwrap();
    let mut bytes = Vec::new();
    let mut cursor = "0".to_string();
    let mut pages = 0;
    loop {
        let page = engine
            .invoke(
                "ci_read",
                json!({"artifactId":id,"cursor":cursor,"maxBytes":1024}),
            )
            .unwrap();
        assert_eq!(page["status"], "ok", "{page}");
        assert_eq!(page["data"]["artifactId"], id);
        assert_eq!(page["data"]["cursor"], cursor);
        bytes.extend_from_slice(page["data"]["chunk"].as_str().unwrap().as_bytes());
        pages += 1;
        if let Some(next) = page["data"]["nextCursor"].as_str() {
            assert_eq!(next.parse::<usize>().unwrap(), bytes.len());
            assert!(next.parse::<usize>().unwrap() > cursor.parse::<usize>().unwrap());
            cursor = next.to_string();
        } else {
            assert_eq!(page["data"]["totalBytes"], bytes.len());
            break;
        }
    }
    assert!(pages > 1, "Fixture must exercise continuation pages");
    assert_eq!(yoke::model::hash(&bytes), id);
    serde_json::from_slice(&bytes).unwrap()
}

#[test]
fn lcov_import_binds_line_hits_and_rejects_incomplete_or_forged_records() {
    let (_dir, mut engine, snapshot, _) = fixture();
    let deadline = Deadline::new(10_000);
    let input = json!({"snapshotId":snapshot.id,"hashes":hashes(&snapshot),"lcov":"TN:invoice\nSF:billing.rs\nDA:2,7\nLF:1\nLH:1\nend_of_record\n"});
    let result =
        knowledge::import_lcov(&engine.store, &snapshot, input.clone(), &deadline).unwrap();
    let data = published_artifact(&mut engine, &result);
    assert_eq!(data["files"][0]["lines"]["2"], 7);
    assert_eq!(data["files"][0]["testName"], "invoice");
    for lcov in [
        "SF:billing.rs\nDA:2,7\n",
        "SF:billing.rs\nDA:99,7\nend_of_record\n",
        "DA:2,7\nend_of_record\n",
        "SF:../billing.rs\nDA:2,7\nend_of_record\n",
    ] {
        let mut bad = input.clone();
        bad["lcov"] = json!(lcov);
        assert!(knowledge::import_lcov(&engine.store, &snapshot, bad, &deadline).is_err());
    }
    let mut bad = input;
    bad["hashes"]["billing.rs"] = json!("0".repeat(64));
    assert!(knowledge::import_lcov(&engine.store, &snapshot, bad, &deadline).is_err());
}

#[test]
fn runtime_import_preserves_one_execution_and_refuses_parent_cycles() {
    let (_dir, mut engine, snapshot, _) = fixture();
    let deadline = Deadline::new(10_000);
    let f = snapshot
        .files
        .iter()
        .find(|f| f.path == "billing.rs")
        .unwrap();
    let input = json!({"snapshotId":snapshot.id,"spans":[{"id":"root","path":f.path,"contentHash":f.hash,"line":2},{"id":"call","parentId":"root","path":f.path,"contentHash":f.hash,"line":2}]});
    let result =
        knowledge::import_runtime(&engine.store, &snapshot, input.clone(), &deadline).unwrap();
    let data = published_artifact(&mut engine, &result);
    assert_eq!(data["scope"], "this_recorded_execution_only");
    assert_eq!(data["spans"][1]["parentId"], "root");
    let mut cycle = input.clone();
    cycle["spans"][0]["parentId"] = json!("call");
    assert!(knowledge::import_runtime(&engine.store, &snapshot, cycle, &deadline).is_err());
    let mut forged = input.clone();
    forged["spans"][0]["contentHash"] = json!("0".repeat(64));
    assert!(knowledge::import_runtime(&engine.store, &snapshot, forged, &deadline).is_err());
    let mut unsafe_range = input;
    unsafe_range["spans"][0]["startByte"] = json!(3);
    unsafe_range["spans"][0]["endByte"] = json!(4);
    assert!(knowledge::import_runtime(&engine.store, &snapshot, unsafe_range, &deadline).is_err());
}

#[test]
fn sarif_import_maps_utf16_unicode_regions_to_source_bytes_and_refuses_escape() {
    let (_dir, mut engine, snapshot, _) = fixture();
    let deadline = Deadline::new(10_000);
    let source = String::from_utf8(
        engine
            .store
            .read_blob(
                &snapshot
                    .files
                    .iter()
                    .find(|f| f.path == "billing.rs")
                    .unwrap()
                    .hash,
            )
            .unwrap(),
    )
    .unwrap();
    let emoji_byte = source.find('🌍').unwrap();
    let emoji_column = source[..emoji_byte].encode_utf16().count() + 1;
    let input = json!({"snapshotId":snapshot.id,"hashes":hashes(&snapshot),"sarif":{"version":"2.1.0","runs":[{"tool":{"driver":{"name":"fixture-checker"}},"results":[{"ruleId":"RULE1","message":{"text":"Inspect Unicode symbol"},"locations":[{"physicalLocation":{"artifactLocation":{"uri":"billing.rs"},"region":{"startLine":1,"startColumn":emoji_column,"endLine":1,"endColumn":emoji_column+2}}}]}]}]}});
    let result =
        knowledge::import_sarif(&engine.store, &snapshot, input.clone(), &deadline).unwrap();
    let data = published_artifact(&mut engine, &result);
    assert_eq!(data["findings"][0]["startByte"], emoji_byte);
    assert_eq!(data["findings"][0]["endByte"], emoji_byte + 4);
    let mut surrogate = input.clone();
    surrogate["sarif"]["runs"][0]["results"][0]["locations"][0]["physicalLocation"]["region"]["endColumn"] =
        json!(emoji_column + 1);
    assert!(knowledge::import_sarif(&engine.store, &snapshot, surrogate, &deadline).is_err());
    let mut malformed_column_kind = input.clone();
    malformed_column_kind["sarif"]["runs"][0]["columnKind"] = json!(42);
    assert!(
        knowledge::import_sarif(&engine.store, &snapshot, malformed_column_kind, &deadline)
            .is_err()
    );
    let mut unsafe_uri = input;
    unsafe_uri["sarif"]["runs"][0]["results"][0]["locations"][0]["physicalLocation"]["artifactLocation"]
        ["uri"] = json!("%2e%2e/billing.rs");
    assert!(knowledge::import_sarif(&engine.store, &snapshot, unsafe_uri, &deadline).is_err());
}
