use serde_json::{Value, json};
use std::{collections::BTreeMap, fs};
use yoke::{
    Engine,
    analysis::{self, AnalyzerEpoch},
    capture::{self, CaptureOptions, Deadline},
    model::{Graph, Snapshot, identity},
};

use yoke::capture::capture_with_options_at as selected_capture;

fn publish(engine: &mut Engine, epoch: AnalyzerEpoch, snapshot: &Snapshot) -> Graph {
    let deadline = Deadline::new(20000);
    let extracted = snapshot
        .files
        .iter()
        .map(|file| {
            let source = String::from_utf8(engine.store.read_blob(&file.hash).unwrap()).unwrap();
            analysis::extract_at(epoch, file, &source, &deadline).unwrap()
        })
        .collect();
    let graph = analysis::assemble_at(epoch, snapshot, extracted, &deadline).unwrap();
    engine.store.publish(snapshot, &graph).unwrap();
    graph
}

fn options_cases() -> Vec<CaptureOptions> {
    vec![
        CaptureOptions::default(),
        CaptureOptions {
            language_overrides: BTreeMap::from([("fixture".into(), "rust".into())]),
            read_only_dependency_dirs: vec!["node_modules".into()],
            ..Default::default()
        },
        CaptureOptions {
            include_ignored_dirs: vec!["ignored".into()],
            ..Default::default()
        },
    ]
}

#[test]
fn selected_v9_capture_has_distinct_proofs_and_replays_through_default_readers() {
    for options in options_cases() {
        let root = tempfile::tempdir().unwrap();
        fs::create_dir_all(root.path().join("ignored")).unwrap();
        fs::create_dir_all(root.path().join("node_modules/pkg")).unwrap();
        for (path, source) in [
            ("main.ps1", "function Invoke-Target {}\ninvoke-target\n"),
            ("custom.fixture", "fn custom_target() {}\n"),
            (".gitignore", "ignored/\n!node_modules/\n"),
            ("ignored/hidden.rs", "fn hidden() {}\n"),
            ("node_modules/pkg/main.rs", "fn dependency() {}\n"),
        ] {
            fs::write(root.path().join(path), source).unwrap();
        }
        let mut engine = Engine::open(root.path()).unwrap();
        let deadline = Deadline::new(20000);
        let old = selected_capture(
            AnalyzerEpoch::V8,
            root.path(),
            &engine.store,
            &options,
            &deadline,
        )
        .unwrap();
        let new = selected_capture(
            AnalyzerEpoch::V9,
            root.path(),
            &engine.store,
            &options,
            &deadline,
        )
        .unwrap();
        assert_ne!(
            old.id, new.id,
            "explicit9 must not reuse the analyzer8 source identity"
        );
        assert_eq!(old.files, new.files);
        if options == CaptureOptions::default() {
            assert_eq!(
                new.id,
                identity(&(
                    &new.workspace_id,
                    "disk",
                    "yoke-tree-sitter-9",
                    &new.files,
                    &new.omissions,
                ))
            );
        } else {
            let kind = if options.include_ignored_dirs.is_empty() {
                capture::OPTIONS_RECORD_KIND
            } else {
                capture::IGNORED_RECORD_KIND
            };
            let proof: Value = engine.store.get_record(kind, &new.id).unwrap();
            assert_eq!(proof["analyzer"], "yoke-tree-sitter-9");
        }
        let expected = publish(&mut engine, AnalyzerEpoch::V9, &new);
        assert_eq!(
            capture::options_for_snapshot(&engine.store, &new).unwrap(),
            options
        );
        assert_eq!(
            capture::analyzer_epoch_for_snapshot(&engine.store, &new).unwrap(),
            AnalyzerEpoch::V9
        );
        assert!(
            !capture::source_path_policy(&engine.store, &new, "main.ps1")
                .unwrap()
                .read_only
        );
        if !options.include_ignored_dirs.is_empty() {
            assert!(
                capture::source_path_policy(&engine.store, &new, "ignored/hidden.rs")
                    .unwrap()
                    .read_only
            );
        }
        if !options.read_only_dependency_dirs.is_empty() {
            assert!(
                capture::source_path_policy(&engine.store, &new, "node_modules/pkg/main.rs")
                    .unwrap()
                    .read_only
            );
        }
        let view = yoke::graph_view::build(
            &engine.store,
            &new,
            &expected,
            &Default::default(),
            &deadline,
        )
        .unwrap();
        let registry_dir = tempfile::tempdir().unwrap();
        let registry = yoke::graph_collections::Registry::open(registry_dir.path()).unwrap();
        registry
            .register_view("selected9", &engine.store, &view, &deadline)
            .unwrap();
        engine
            .store
            .db
            .execute("DELETE FROM graphs WHERE id=?1", [&new.id])
            .unwrap();
        let (_, replayed, _, _) = engine.analyze_snapshot(new.clone(), &deadline).unwrap();
        assert_eq!(identity(&replayed), identity(&expected));
        assert!(
            replayed
                .evidence
                .iter()
                .all(|e| e.producer == "yoke-tree-sitter-9")
        );
        let retention =
            yoke::retention::plan(&engine.store, &Default::default(), &deadline).unwrap();
        assert_eq!(retention["unknownDependencyKinds"], json!([]));
    }
}

#[test]
fn typed_v8_disk_capture_matches_authentic_snapshot_and_receipt_bytes() {
    let bundle: Value =
        serde_json::from_str(include_str!("fixtures/analyzer8/authentic-analyzer8.json")).unwrap();
    for case in bundle["cases"].as_array().unwrap() {
        let snapshot_row = case["snapshots"]
            .as_array()
            .unwrap()
            .iter()
            .find(|row| {
                serde_json::from_str::<Snapshot>(row["value"].as_str().unwrap())
                    .unwrap()
                    .kind
                    == "disk"
            })
            .unwrap();
        let expected: Snapshot =
            serde_json::from_str(snapshot_row["value"].as_str().unwrap()).unwrap();
        let record_row = case["records"].as_array().unwrap().iter().find(|row| {
            row["id"] == expected.id
                && matches!(
                    row["kind"].as_str().unwrap(),
                    capture::OPTIONS_RECORD_KIND | capture::IGNORED_RECORD_KIND
                )
        });
        let options = record_row
            .map(|row| {
                let record: Value = serde_json::from_str(row["value"].as_str().unwrap()).unwrap();
                serde_json::from_value::<CaptureOptions>(record["options"].clone()).unwrap()
            })
            .unwrap_or_default();
        let root = tempfile::tempdir().unwrap();
        let mut engine = Engine::open(root.path()).unwrap();
        engine.store.workspace_id = expected.workspace_id.clone();
        for file in &expected.files {
            let path = root.path().join(&file.path);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, case["sources"][&file.hash].as_str().unwrap()).unwrap();
        }
        let deadline = Deadline::new(20000);
        let selected = selected_capture(
            AnalyzerEpoch::V8,
            root.path(),
            &engine.store,
            &options,
            &deadline,
        )
        .unwrap();
        assert_eq!(
            serde_json::to_string(&selected).unwrap(),
            snapshot_row["value"].as_str().unwrap()
        );
        let default =
            capture::capture_with_options(root.path(), &engine.store, &options, &deadline).unwrap();
        assert_eq!(identity(&selected), identity(&default));
        if let Some(row) = record_row {
            let actual: String = engine
                .store
                .db
                .query_row(
                    "SELECT value FROM records WHERE kind=?1 AND id=?2",
                    rusqlite::params![row["kind"].as_str().unwrap(), selected.id],
                    |r| r.get(0),
                )
                .unwrap();
            assert_eq!(actual, row["value"].as_str().unwrap());
        }
    }
}

#[test]
fn resealed_unknown_epoch_and_changed_v9_proofs_fail_closed() {
    for options in options_cases() {
        let root = tempfile::tempdir().unwrap();
        fs::write(
            root.path().join("main.ps1"),
            "function Invoke-Target {}\ninvoke-target\n",
        )
        .unwrap();
        let mut engine = Engine::open(root.path()).unwrap();
        let deadline = Deadline::new(20000);
        let snapshot = selected_capture(
            AnalyzerEpoch::V9,
            root.path(),
            &engine.store,
            &options,
            &deadline,
        )
        .unwrap();
        publish(&mut engine, AnalyzerEpoch::V9, &snapshot);
        assert_eq!(
            capture::analyzer_epoch_for_snapshot(&engine.store, &snapshot).unwrap(),
            AnalyzerEpoch::V9
        );
        if options == CaptureOptions::default() {
            let mut unknown = snapshot.clone();
            unknown.id = identity(&(
                &unknown.workspace_id,
                "disk",
                "yoke-tree-sitter-10",
                &unknown.files,
                &unknown.omissions,
            ));
            engine
                .store
                .db
                .execute(
                    "INSERT INTO snapshots(id,value) VALUES(?1,?2)",
                    rusqlite::params![unknown.id, serde_json::to_string(&unknown).unwrap()],
                )
                .unwrap();
            assert!(capture::analyzer_epoch_for_snapshot(&engine.store, &unknown).is_err());
            continue;
        }
        let kind = if options.include_ignored_dirs.is_empty() {
            capture::OPTIONS_RECORD_KIND
        } else {
            capture::IGNORED_RECORD_KIND
        };
        let mut record: Value = engine.store.get_record(kind, &snapshot.id).unwrap();
        record["analyzer"] = json!("yoke-tree-sitter-8");
        engine
            .store
            .db
            .execute(
                "UPDATE records SET value=?1 WHERE kind=?2 AND id=?3",
                rusqlite::params![serde_json::to_string(&record).unwrap(), kind, snapshot.id],
            )
            .unwrap();
        assert!(capture::analyzer_epoch_for_snapshot(&engine.store, &snapshot).is_err());
        assert!(yoke::retention::plan(&engine.store, &Default::default(), &deadline).is_err());
    }
}
