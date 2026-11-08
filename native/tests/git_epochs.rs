use serde_json::Value;
use yoke::{
    Engine, capture::Deadline, git_workflows, graph_collections::Registry, model::Snapshot,
};
fn bundle() -> Value {
    serde_json::from_str(include_str!("fixtures/analyzer8/authentic-git8.json")).unwrap()
}
fn restore(case: &Value) -> (tempfile::TempDir, Engine) {
    let root = tempfile::tempdir().unwrap();
    let engine = Engine::open(root.path()).unwrap();
    engine
        .store
        .db
        .execute(
            "UPDATE meta SET value=?1 WHERE key='workspace'",
            [case["workspaceId"].as_str().unwrap()],
        )
        .unwrap();
    drop(engine);
    let engine = Engine::open(root.path()).unwrap();
    for table in ["snapshots", "graphs", "cache", "records"] {
        for row in case["rows"][table].as_array().unwrap() {
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
                let key = if table == "cache" { "key" } else { "id" };
                engine
                    .store
                    .db
                    .execute(
                        &format!("INSERT INTO {table}({key},value) VALUES(?1,?2)"),
                        rusqlite::params![
                            row[key].as_str().unwrap(),
                            row["value"].as_str().unwrap()
                        ],
                    )
                    .unwrap();
            }
        }
    }
    for (hash, text) in case["blobs"].as_object().unwrap() {
        assert_eq!(
            engine
                .store
                .blob(text.as_str().unwrap().as_bytes())
                .unwrap(),
            *hash
        );
    }
    (root, engine)
}
fn git_snapshot(case: &Value) -> Snapshot {
    case["rows"]["snapshots"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| serde_json::from_str::<Snapshot>(r["value"].as_str().unwrap()).unwrap())
        .find(|s| s.capture_consistency == "immutable_git_objects_verified")
        .unwrap()
}

#[test]
fn authentic_git8_receipts_reports_and_graphs_remain_readable() {
    for case in bundle()["cases"].as_array().unwrap() {
        let (_root, engine) = restore(case);
        let s = git_snapshot(case);
        let d = Deadline::new(20000);
        assert_eq!(
            git_workflows::load(
                &engine.store,
                case["report"]["artifactId"].as_str().unwrap(),
                &d
            )
            .unwrap(),
            case["report"]
        );
        let proof = git_workflows::validate_source(&engine.store, &s.id, &d).unwrap();
        assert_eq!(proof["analyzer"], "yoke-tree-sitter-8");
    }
}
#[test]
fn missing_authentic_git8_graph_replays_the_original_extraction_epoch() {
    for case in bundle()["cases"].as_array().unwrap() {
        let (_root, mut engine) = restore(case);
        let s = git_snapshot(case);
        let raw: String = engine
            .store
            .db
            .query_row("SELECT value FROM graphs WHERE id=?1", [&s.id], |r| {
                r.get(0)
            })
            .unwrap();
        engine
            .store
            .db
            .execute("DELETE FROM graphs WHERE id=?1", [&s.id])
            .unwrap();
        let (_, graph, _, _) = engine.analyze_snapshot(s, &Deadline::new(20000)).unwrap();
        assert_eq!(serde_json::to_string(&graph).unwrap(), raw);
    }
}
#[test]
fn git8_registry_archive_replays_without_the_original_workspace() {
    for case in bundle()["cases"].as_array().unwrap() {
        let (root, engine) = restore(case);
        let s = git_snapshot(case);
        let graph = engine.store.graph(&s.id).unwrap();
        let registry_root = tempfile::tempdir().unwrap();
        let registry = Registry::open(registry_root.path()).unwrap();
        let d = Deadline::new(20000);
        registry
            .register("history", &engine.store, &s, &graph, &d)
            .unwrap();
        drop(engine);
        root.close().unwrap();
        let joined = registry.join(&["history".into()], &d).unwrap();
        assert!(!joined["nodes"].as_array().unwrap().is_empty());
        assert_eq!(
            joined["lineage"][0]["canonicalReplay"]["analyzer"],
            "yoke-tree-sitter-8"
        );
    }
}

#[test]
fn explicit_git9_capture_has_distinct_identity_facts_and_missing_graph_replay() {
    use std::process::Command;
    use yoke::analysis::AnalyzerEpoch;
    let root = tempfile::tempdir().unwrap();
    let git = |args: &[&str]| {
        let out = Command::new("git")
            .args([
                "-c",
                "core.hooksPath=",
                "-c",
                "commit.gpgsign=false",
                "-c",
                "user.name=Fixture",
                "-c",
                "user.email=fixture@example.invalid",
            ])
            .args(args)
            .current_dir(root.path())
            .env("GIT_CONFIG_GLOBAL", "NUL")
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        out.stdout
    };
    git(&["init", "--quiet"]);
    std::fs::write(
        root.path().join("local.lisp"),
        "(defun run () (labels ((local () 1)) (local)))\n",
    )
    .unwrap();
    std::fs::write(
        root.path().join("circuit.sv"),
        "module m; task work(); endtask initial work(); endmodule\n",
    )
    .unwrap();
    git(&["add", "--all"]);
    git(&["commit", "--quiet", "-m", "Fixture"]);
    let oid = String::from_utf8(git(&["rev-parse", "HEAD"])).unwrap();
    let mut engine = Engine::open(root.path()).unwrap();
    let d = Deadline::new(20000);
    let (origin, _, _, _) = engine.index(&d).unwrap();
    let host = git_workflows::HostGit::discover().unwrap();
    let (old, old_graph) = git_workflows::capture_commit_at(
        AnalyzerEpoch::V8,
        &mut engine,
        &host,
        &origin,
        oid.trim(),
        &d,
    )
    .unwrap();
    let (new, new_graph) = git_workflows::capture_commit_at(
        AnalyzerEpoch::V9,
        &mut engine,
        &host,
        &origin,
        oid.trim(),
        &d,
    )
    .unwrap();
    assert_ne!(old.id, new.id);
    assert_eq!(old.files, new.files);
    assert_eq!(yoke::model::ANALYZER, "yoke-tree-sitter-8");
    assert!(
        !old_graph
            .entities
            .iter()
            .any(|e| e.name == "local" || e.name == "work")
    );
    assert!(new_graph.entities.iter().any(|e| e.name == "local"));
    assert!(new_graph.entities.iter().any(|e| e.name == "work"));
    assert_eq!(
        git_workflows::validated_source_epoch(&engine.store, &old, &d).unwrap(),
        AnalyzerEpoch::V8
    );
    assert_eq!(
        git_workflows::validated_source_epoch(&engine.store, &new, &d).unwrap(),
        AnalyzerEpoch::V9
    );
    engine
        .store
        .db
        .execute("DELETE FROM graphs WHERE id=?1", [&new.id])
        .unwrap();
    let (_, replayed, _, _) = engine.analyze_snapshot(new.clone(), &d).unwrap();
    assert_eq!(
        yoke::model::identity(&replayed),
        yoke::model::identity(&new_graph)
    );
    let registry_root = tempfile::tempdir().unwrap();
    let registry = Registry::open(registry_root.path()).unwrap();
    registry
        .register("git9", &engine.store, &new, &new_graph, &d)
        .unwrap();
    drop(engine);
    root.close().unwrap();
    assert_eq!(
        registry.join(&["git9".into()], &d).unwrap()["lineage"][0]["canonicalReplay"]["analyzer"],
        "yoke-tree-sitter-9"
    );
}

#[test]
fn guarded_derivative_archive_keeps_older_git_source_proof_offline() {
    for case in bundle()["cases"].as_array().unwrap() {
        let (root, mut engine) = restore(case);
        let s = git_snapshot(case);
        let graph = engine.store.graph(&s.id).unwrap();
        let d = Deadline::new(20000);
        let file = s.files.iter().find(|f| f.path == "main.rs").unwrap();
        let plan=yoke::changes::prepare_at(yoke::analysis::AnalyzerEpoch::V9,&mut engine.store,&s,&graph,&serde_json::json!({"edits":[{"kind":"replace","path":"main.rs","expectedHash":file.hash,"startByte":0,"endByte":file.size,"text":"fn nested() {}\n"}]}),&d).unwrap();
        let derived = engine.store.snapshot(&plan.result_snapshot_id).unwrap();
        let result = engine.store.graph(&derived.id).unwrap();
        let registry_root = tempfile::tempdir().unwrap();
        let registry = Registry::open(registry_root.path()).unwrap();
        registry
            .register("nested", &engine.store, &derived, &result, &d)
            .unwrap();
        drop(engine);
        root.close().unwrap();
        let text = registry
            .read_source("nested", "main.rs", 0..15, &d)
            .unwrap();
        assert_eq!(text["text"], "fn nested() {}\n");
        assert!(registry.join(&["nested".into()], &d).is_ok());
    }
}

#[test]
fn git_epoch_selection_rejects_invalid_receipts_missing_proofs_and_expired_deadlines() {
    for variant in 0..7 {
        let data = bundle();
        let (_root, engine) = restore(&data["cases"][0]);
        let snapshot = git_snapshot(&data["cases"][0]);
        let d = Deadline::new(20000);
        let mut proof = git_workflows::validate_source(&engine.store, &snapshot.id, &d).unwrap();
        match variant {
            0 => proof["analyzer"] = "yoke-tree-sitter-10".into(),
            1 => proof["snapshotHash"] = "0".repeat(64).into(),
            2 => proof["commitArtifactId"] = "0".repeat(64).into(),
            3 => proof["treeId"] = "0".repeat(40).into(),
            4 => proof["blobs"][0]["gitObjectId"] = "0".repeat(40).into(),
            5 => proof["blobs"][0]["mode"] = "100755".into(),
            _ => proof["originSnapshotId"] = "0".repeat(64).into(),
        };
        engine
            .store
            .db
            .execute(
                "UPDATE records SET value=?1 WHERE kind=?2 AND id=?3",
                rusqlite::params![proof.to_string(), git_workflows::SOURCE_KIND, &snapshot.id],
            )
            .unwrap();
        assert!(
            git_workflows::validated_source_epoch(&engine.store, &snapshot, &d).is_err(),
            "variant {variant}"
        );
    }
    let data = bundle();
    let (_root, mut engine) = restore(&data["cases"][0]);
    let s = git_snapshot(&data["cases"][0]);
    assert!(git_workflows::validated_source_epoch(&engine.store, &s, &Deadline::new(0)).is_err());
    engine.store.clear_deadline();
    engine
        .store
        .db
        .execute("DELETE FROM graphs WHERE id=?1", [&s.id])
        .unwrap();
    engine
        .store
        .db
        .execute(
            "DELETE FROM records WHERE kind=?1 AND id=?2",
            rusqlite::params![git_workflows::SOURCE_KIND, &s.id],
        )
        .unwrap();
    assert!(engine.analyze_snapshot(s, &Deadline::new(20000)).is_err());
}

#[test]
fn resealed_offline_git_proof_cannot_invent_commit_or_source_bytes() {
    for field in ["commitText", "source", "mode"] {
        let data = bundle();
        let (_root, engine) = restore(&data["cases"][0]);
        let s = git_snapshot(&data["cases"][0]);
        let graph = engine.store.graph(&s.id).unwrap();
        let registry_root = tempfile::tempdir().unwrap();
        let registry = Registry::open(registry_root.path()).unwrap();
        let d = Deadline::new(20000);
        registry
            .register("history", &engine.store, &s, &graph, &d)
            .unwrap();
        let db = rusqlite::Connection::open(registry_root.path().join("yoke-collection.sqlite"))
            .unwrap();
        let raw: String = db
            .query_row(
                "SELECT payload FROM registrations WHERE name='history'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        let mut value: Value = serde_json::from_str(&raw).unwrap();
        let proof = &mut value["captureScope"]["nodes"][0]["proof"]["record"];
        match field {
            "commitText" => proof["commitText"] = "tree fake\n".into(),
            "source" => proof["sources"]["main.rs"] = "fn forged(){}\n".into(),
            _ => proof["receipt"]["blobs"][0]["mode"] = "100755".into(),
        };
        let id = yoke::model::identity(&value);
        db.execute(
            "UPDATE registrations SET id=?1,payload=?2 WHERE name='history'",
            rusqlite::params![id, value.to_string()],
        )
        .unwrap();
        assert!(
            registry.join(&["history".into()], &d).is_err(),
            "tampering {field}"
        );
    }
}

#[test]
fn synthetic_legacy_git_receipts_remain_readable_without_an_extractor_claim() {
    for analyzer in ["yoke-tree-sitter-4", "yoke-tree-sitter-6"] {
        // Compatibility shape only: no claim that these are authentic old parser outputs.
        let data = bundle();
        let (_root, engine) = restore(&data["cases"][0]);
        let old = git_snapshot(&data["cases"][0]);
        let d = Deadline::new(20000);
        let mut proof = git_workflows::validate_source(&engine.store, &old.id, &d).unwrap();
        let mut s = old.clone();
        s.id = yoke::model::identity(&(
            "native-git-source-1",
            &s.workspace_id,
            proof["commitId"].as_str().unwrap(),
            proof["originSnapshotId"].as_str().unwrap(),
            &s.files,
            &s.omissions,
            analyzer,
        ));
        proof["snapshotId"] = s.id.clone().into();
        proof["snapshotHash"] = yoke::model::identity(&s).into();
        proof["analyzer"] = analyzer.into();
        let mut inherited: Value = engine
            .store
            .get_record(yoke::capture::DERIVED_OPTIONS_RECORD_KIND, &old.id)
            .unwrap();
        inherited["snapshotId"] = s.id.clone().into();
        inherited["derivedSnapshotHash"] = yoke::model::identity(&s).into();
        let graph = yoke::model::Graph {
            snapshot_id: s.id.clone(),
            ..Default::default()
        };
        engine
            .store
            .db
            .execute(
                "INSERT INTO snapshots(id,value) VALUES(?1,?2)",
                rusqlite::params![&s.id, serde_json::to_string(&s).unwrap()],
            )
            .unwrap();
        engine
            .store
            .db
            .execute(
                "INSERT INTO graphs(id,value) VALUES(?1,?2)",
                rusqlite::params![&s.id, serde_json::to_string(&graph).unwrap()],
            )
            .unwrap();
        for (kind, value) in [
            (git_workflows::SOURCE_KIND, proof),
            (yoke::capture::DERIVED_OPTIONS_RECORD_KIND, inherited),
        ] {
            engine
                .store
                .db
                .execute(
                    "INSERT INTO records(kind,id,value) VALUES(?1,?2,?3)",
                    rusqlite::params![kind, &s.id, value.to_string()],
                )
                .unwrap();
        }
        assert!(git_workflows::validate_source(&engine.store, &s.id, &d).is_ok());
        assert!(git_workflows::validated_source_epoch(&engine.store, &s, &d).is_err());
        let registry_root = tempfile::tempdir().unwrap();
        let registry = Registry::open(registry_root.path()).unwrap();
        registry
            .register("legacy", &engine.store, &s, &graph, &d)
            .unwrap();
        let joined = registry.join(&["legacy".into()], &d).unwrap();
        assert_eq!(
            joined["lineage"][0]["canonicalReplay"]["status"],
            "historical_host_registered_unreplayed"
        );
    }
}

#[test]
fn git_epoch_accessors_install_the_new_callers_deadline() {
    let data = bundle();
    let (_root, engine) = restore(&data["cases"][0]);
    let snapshot = git_snapshot(&data["cases"][0]);
    assert!(engine.store.set_deadline(&Deadline::new(0)).is_err());
    assert_eq!(
        git_workflows::validated_source_epoch(&engine.store, &snapshot, &Deadline::new(20000))
            .unwrap(),
        yoke::analysis::AnalyzerEpoch::V8
    );
    assert!(engine.store.set_deadline(&Deadline::new(0)).is_err());
    assert_eq!(
        yoke::capture::analyzer_epoch_for_snapshot_with_deadline(
            &engine.store,
            &snapshot,
            &Deadline::new(20000)
        )
        .unwrap(),
        yoke::analysis::AnalyzerEpoch::V8
    );
}
