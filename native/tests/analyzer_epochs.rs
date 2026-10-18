use serde_json::{Value, json};
use yoke::{
    Engine,
    analysis::{self, AnalyzerEpoch},
    capture::Deadline,
    model::{Extraction, Snapshot, SourceFile, hash, identity},
};

fn bundle() -> Value {
    serde_json::from_str(include_str!("fixtures/analyzer8/authentic-analyzer8.json")).unwrap()
}
fn source(case: &Value, file: &SourceFile) -> String {
    let text = case["sources"][&file.hash].as_str().unwrap();
    assert_eq!(hash(text), file.hash);
    assert_eq!(text.len(), file.size);
    text.into()
}
fn snapshot(case: &Value, kind: &str) -> Snapshot {
    case["snapshots"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| serde_json::from_str::<Snapshot>(r["value"].as_str().unwrap()).unwrap())
        .find(|s| s.kind == kind)
        .unwrap()
}
fn restore(case: &Value) -> (tempfile::TempDir, Engine) {
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
    for (expected, text) in case["sources"].as_object().unwrap() {
        assert_eq!(
            engine
                .store
                .blob(text.as_str().unwrap().as_bytes())
                .unwrap(),
            *expected
        );
    }
    (root, engine)
}

#[test]
fn authentic_v8_extractions_and_entire_graphs_replay_byte_identically() {
    let deadline = Deadline::new(20000);
    for case in bundle()["cases"].as_array().unwrap() {
        for cache_row in case["cache"].as_array().unwrap() {
            let extraction: Extraction =
                serde_json::from_str(cache_row["value"].as_str().unwrap()).unwrap();
            let root = extraction
                .entities
                .iter()
                .find(|e| e.kind == "file")
                .unwrap();
            let file = case["snapshots"]
                .as_array()
                .unwrap()
                .iter()
                .find_map(|row| {
                    let snapshot: Snapshot =
                        serde_json::from_str(row["value"].as_str().unwrap()).unwrap();
                    snapshot.files.into_iter().find(|f| {
                        f.path == root.path
                            && AnalyzerEpoch::V8.cache_key(f) == cache_row["id"].as_str().unwrap()
                    })
                });
            // Cache identity includes source path/hash/language, not snapshot ID.
            let file =
                file.unwrap_or_else(|| panic!("authentic cache member missing: {}", root.path));
            let actual =
                analysis::extract_at(AnalyzerEpoch::V8, &file, &source(case, &file), &deadline)
                    .unwrap();
            assert_eq!(
                serde_json::to_string(&actual).unwrap(),
                cache_row["value"].as_str().unwrap()
            );
        }
        for row in case["snapshots"].as_array().unwrap() {
            let snapshot: Snapshot = serde_json::from_str(row["value"].as_str().unwrap()).unwrap();
            let extractions = snapshot
                .files
                .iter()
                .map(|f| {
                    analysis::extract_at(AnalyzerEpoch::V8, f, &source(case, f), &deadline).unwrap()
                })
                .collect();
            let actual =
                analysis::assemble_at(AnalyzerEpoch::V8, &snapshot, extractions, &deadline)
                    .unwrap();
            let saved = case["graphs"]
                .as_array()
                .unwrap()
                .iter()
                .find(|r| r["id"] == snapshot.id)
                .unwrap();
            assert_eq!(
                serde_json::to_string(&actual).unwrap(),
                saved["value"].as_str().unwrap(),
                "entire V8 graph changed: {} {}",
                case["name"],
                snapshot.kind
            );
        }
    }
}

#[test]
fn explicit_v9_policy_reaches_embedded_symbols_scoped_bindings_and_evidence() {
    let data = bundle();
    let case = &data["cases"][0];
    let mut snapshot = snapshot(case, "disk");
    snapshot.id = identity(&("explicit-v9-fixture", &snapshot.id));
    let deadline = Deadline::new(20000);
    let extracted: Vec<_> = snapshot
        .files
        .iter()
        .map(|f| analysis::extract_at(AnalyzerEpoch::V9, f, &source(case, f), &deadline).unwrap())
        .collect();
    let rust = extracted
        .iter()
        .find(|e| e.entities[0].path == "main.rs")
        .unwrap();
    assert!(
        rust.edges
            .iter()
            .any(|e| e.relation == "references" && e.target_label == "target")
    );
    assert_eq!(
        rust.entities
            .iter()
            .find(|e| e.name == "target")
            .unwrap()
            .signature,
        "fn target(value: i32)"
    );
    let graph = analysis::assemble_at(AnalyzerEpoch::V9, &snapshot, extracted, &deadline).unwrap();
    assert!(
        graph
            .entities
            .iter()
            .any(|e| e.path == "main.vue" && e.name == "target")
    );
    assert!(
        graph
            .entities
            .iter()
            .filter(|e| e.producer.starts_with("yoke-tree-sitter-"))
            .all(|e| e.producer == "yoke-tree-sitter-9")
    );
    assert!(
        graph
            .evidence
            .iter()
            .all(|e| e.producer == "yoke-tree-sitter-9")
    );
    assert!(
        graph
            .observations
            .iter()
            .all(|e| e.producer == "yoke-tree-sitter-9")
    );
}

#[test]
fn v8_cache_cannot_be_relabelled_as_a_v9_assembly() {
    let data = bundle();
    let case = &data["cases"][0];
    let snapshot = snapshot(case, "disk");
    let file = &snapshot.files[0];
    assert_eq!(
        AnalyzerEpoch::V8.cache_key(file),
        identity(&("yoke-tree-sitter-8", &file.language, &file.hash, &file.path))
    );
    let deadline = Deadline::new(20000);
    let extracted = snapshot
        .files
        .iter()
        .map(|f| analysis::extract_at(AnalyzerEpoch::V8, f, &source(case, f), &deadline).unwrap())
        .collect();
    assert!(analysis::assemble_at(AnalyzerEpoch::V9, &snapshot, extracted, &deadline).is_err());
    assert!(AnalyzerEpoch::parse("yoke-tree-sitter-10").is_err());
    assert!(AnalyzerEpoch::parse("yoke-tree-sitter-08").is_err());
}

#[test]
fn extraction_cache_keys_are_separated_by_epoch() {
    let data = bundle();
    let snapshot = snapshot(&data["cases"][0], "disk");
    assert_ne!(
        AnalyzerEpoch::V8.cache_key(&snapshot.files[0]),
        AnalyzerEpoch::V9.cache_key(&snapshot.files[0])
    );
}

#[test]
fn identical_patch_gets_a_new_v9_identity_without_republishing_v8_graph_bytes() {
    let data = bundle();
    let case = &data["cases"][0];
    let (_root, mut engine) = restore(case);
    let snapshot = snapshot(case, "disk");
    let graph = engine.store.graph(&snapshot.id).unwrap();
    let file = snapshot.files.iter().find(|f| f.path == "main.rs").unwrap();
    let args = json!({"edits":[{"kind":"replace","path":"main.rs","expectedHash":file.hash,
        "startByte":0,"endByte":0,"text":"// derived fixture\n"}]});
    let before: Vec<_> = case["graphs"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| {
            (
                r["id"].as_str().unwrap().to_owned(),
                r["value"].as_str().unwrap().to_owned(),
            )
        })
        .collect();
    let deadline = Deadline::new(20000);
    let old = yoke::changes::prepare_at(
        AnalyzerEpoch::V8,
        &mut engine.store,
        &snapshot,
        &graph,
        &args,
        &deadline,
    )
    .unwrap();
    let new = yoke::changes::prepare_at(
        AnalyzerEpoch::V9,
        &mut engine.store,
        &snapshot,
        &graph,
        &args,
        &deadline,
    )
    .unwrap();
    let expected: Snapshot = case["snapshots"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| serde_json::from_str::<Snapshot>(r["value"].as_str().unwrap()).unwrap())
        .find(|s| s.kind == "derived")
        .unwrap();
    assert_eq!(old.result_snapshot_id, expected.id);
    assert_ne!(
        new.result_snapshot_id, old.result_snapshot_id,
        "new extraction epoch must not collide with the released derived ID"
    );
    let new_graph = engine.store.graph(&new.result_snapshot_id).unwrap();
    assert!(
        new_graph
            .evidence
            .iter()
            .all(|e| e.producer == "yoke-tree-sitter-9")
    );
    for (id, expected) in before {
        let actual: String = engine
            .store
            .db
            .query_row("SELECT value FROM graphs WHERE id=?1", [id], |r| r.get(0))
            .unwrap();
        assert_eq!(actual, expected);
    }
}

#[test]
fn analyzer_selection_follows_exact_native_and_derived_ids_instead_of_the_active_epoch() {
    let data = bundle();
    for case in data["cases"].as_array().unwrap() {
        let (_root, mut engine) = restore(case);
        for row in case["snapshots"].as_array().unwrap() {
            let snapshot: Snapshot = serde_json::from_str(row["value"].as_str().unwrap()).unwrap();
            assert_eq!(
                yoke::capture::analyzer_epoch_for_snapshot(&engine.store, &snapshot).unwrap(),
                AnalyzerEpoch::V8
            );
        }
        let base = snapshot(case, "disk");
        let graph = engine.store.graph(&base.id).unwrap();
        let file = base.files.iter().find(|f| f.path == "main.rs").unwrap();
        let args = json!({"edits":[{"kind":"replace","path":"main.rs","expectedHash":file.hash,
            "startByte":0,"endByte":0,"text":"// explicit9 replay identity\n"}]});
        let plan = yoke::changes::prepare_at(
            AnalyzerEpoch::V9,
            &mut engine.store,
            &base,
            &graph,
            &args,
            &Deadline::new(20000),
        )
        .unwrap();
        let result = engine.store.snapshot(&plan.result_snapshot_id).unwrap();
        assert_eq!(
            yoke::capture::analyzer_epoch_for_snapshot(&engine.store, &result).unwrap(),
            AnalyzerEpoch::V9
        );
    }
}

#[test]
fn unknown_or_resealed_capture_ids_never_select_the_current_analyzer_as_a_fallback() {
    let data = bundle();
    let (_root, engine) = restore(&data["cases"][0]);
    let mut snapshot = snapshot(&data["cases"][0], "disk");
    snapshot.id = identity(&(
        &snapshot.workspace_id,
        "disk",
        "yoke-tree-sitter-10",
        &snapshot.files,
        &snapshot.omissions,
    ));
    assert!(yoke::capture::analyzer_epoch_for_snapshot(&engine.store, &snapshot).is_err());
}

#[test]
fn explicit_editor9_constructor_reuses_its_exact_graph_without_relabelling_active8() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("main.rs"), "fn saved() {}\n").unwrap();
    let mut engine = Engine::open(root.path()).unwrap();
    let deadline = Deadline::new(20000);
    let session = yoke::editor::open(&mut engine, &deadline).unwrap();
    yoke::editor::update(
        &engine,
        &session.id,
        &json!({"path":"main.rs","version":1,"text":"fn unsaved() {}\n"}),
        &deadline,
    )
    .unwrap();
    let first =
        yoke::editor::capture_at(&mut engine, &session.id, AnalyzerEpoch::V9, &deadline).unwrap();
    let before = serde_json::to_string(&engine.store.graph(&first.id).unwrap()).unwrap();
    assert!(before.contains("yoke-tree-sitter-9"));
    let second =
        yoke::editor::capture_at(&mut engine, &session.id, AnalyzerEpoch::V9, &deadline).unwrap();
    assert_eq!(second.id, first.id);
    assert_eq!(
        serde_json::to_string(&engine.store.graph(&first.id).unwrap()).unwrap(),
        before
    );
    assert_eq!(
        std::fs::read_to_string(root.path().join("main.rs")).unwrap(),
        "fn saved() {}\n"
    );
}

#[test]
fn frozen_v8_classifier_matches_every_extension_captured_by_the_original_executable() {
    let data: Value = serde_json::from_str(include_str!(
        "fixtures/analyzer8/authentic-language-classifier8.json"
    ))
    .unwrap();
    let original: Snapshot = serde_json::from_str(data["snapshotRaw"].as_str().unwrap()).unwrap();
    assert_eq!(original.files.len(), 172);
    for file in &original.files {
        assert_eq!(
            yoke::capture::language_at(AnalyzerEpoch::V8, &file.path),
            file.language,
            "{}",
            file.path
        );
    }
}

#[test]
fn authentic_v8_view_archives_stay_self_contained_without_original_store() {
    let data = bundle();
    let deadline = Deadline::new(20000);
    for case in data["cases"].as_array().unwrap() {
        let central = tempfile::tempdir().unwrap();
        let registry = yoke::graph_collections::Registry::open(central.path()).unwrap();
        let db = rusqlite::Connection::open(central.path().join("yoke-collection.sqlite")).unwrap();
        for row in case["registrations"].as_array().unwrap() {
            db.execute(
                "INSERT INTO registrations(name,id,payload) VALUES(?1,?2,?3)",
                rusqlite::params![
                    row["name"].as_str().unwrap(),
                    row["id"].as_str().unwrap(),
                    row["payload"].as_str().unwrap()
                ],
            )
            .unwrap();
        }
        for (hash, text) in case["sources"].as_object().unwrap() {
            db.execute(
                "INSERT INTO source_blobs(hash,bytes) VALUES(?1,?2)",
                rusqlite::params![hash, text.as_str().unwrap().as_bytes()],
            )
            .unwrap();
        }
        for row in case["registrations"].as_array().unwrap() {
            let joined = registry
                .join(&[row["name"].as_str().unwrap().into()], &deadline)
                .unwrap();
            assert!(!joined["nodes"].as_array().unwrap().is_empty());
            let payload: String = db
                .query_row(
                    "SELECT payload FROM registrations WHERE name=?1",
                    [row["name"].as_str().unwrap()],
                    |r| r.get(0),
                )
                .unwrap();
            assert_eq!(payload, row["payload"].as_str().unwrap());
        }
    }
}
