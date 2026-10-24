//! Actual CLI admission and delivery over two immutable registered selections.
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
};
use yoke::{
    Engine,
    capture::Deadline,
    graph_view::{self, BuildOptions, View},
};

struct Fixture {
    _dir: tempfile::TempDir,
    root: PathBuf,
    before: PathBuf,
    after: PathBuf,
    left: View,
    right: View,
    engine: Engine,
}

fn write_json(path: &Path, value: &Value) {
    fs::write(path, serde_json::to_vec(value).unwrap()).unwrap();
}

fn pin(view: &View) -> Value {
    json!({"schemaVersion":1,"workspaceId":view.workspace_id,"snapshotId":view.snapshot_id,
        "viewId":view.id,"viewArtifactId":view.artifact_id,"baseGraphHash":view.base_graph_hash})
}

fn fixture(rename: bool) -> Fixture {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("source");
    fs::create_dir(&root).unwrap();
    fs::write(
        root.join("old.rs"),
        "fn caller(){helper();helper();}\nfn helper(){helper();}\n",
    )
    .unwrap();
    let mut engine = Engine::open(&root).unwrap();
    let deadline = Deadline::new(30_000);
    let (snapshot, graph, _, _) = engine.index(&deadline).unwrap();
    let left = graph_view::build(
        &engine.store,
        &snapshot,
        &graph,
        &BuildOptions::default(),
        &deadline,
    )
    .unwrap();
    if rename {
        fs::rename(root.join("old.rs"), root.join("new.rs")).unwrap();
    } else {
        fs::write(
            root.join("old.rs"),
            "fn caller(){helper();}\nfn helper(){helper();caller();}\n",
        )
        .unwrap();
    }
    let (snapshot, graph, _, _) = engine.index(&deadline).unwrap();
    let right = graph_view::build(
        &engine.store,
        &snapshot,
        &graph,
        &BuildOptions::default(),
        &deadline,
    )
    .unwrap();
    let before = dir.path().join("before.json");
    let after = dir.path().join("after.json");
    write_json(&before, &pin(&left));
    write_json(&after, &pin(&right));
    Fixture {
        _dir: dir,
        root,
        before,
        after,
        left,
        right,
        engine,
    }
}

fn request(f: &Fixture) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_yoke"));
    command
        .arg("--root")
        .arg(&f.root)
        .args(["--timeout", "30000", "graph", "compare-views"])
        .arg("--before")
        .arg(&f.before)
        .arg("--after")
        .arg(&f.after);
    command
}

fn blank(root: &Path, before: &Path, after: &Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_yoke"));
    command
        .arg("--root")
        .arg(root)
        .args(["graph", "compare-views"])
        .arg("--before")
        .arg(before)
        .arg("--after")
        .arg(after);
    command
}

fn success(output: &Output) -> Value {
    assert!(
        output.status.success(),
        "stderr: {}\nstdout: {}",
        String::from_utf8_lossy(&output.stderr),
        String::from_utf8_lossy(&output.stdout)
    );
    assert!(output.stderr.is_empty());
    serde_json::from_slice(&output.stdout).unwrap()
}

fn refusal(output: Output, marker: &str) {
    assert!(!output.status.success());
    assert!(
        output.stdout.is_empty(),
        "A refused result must not be partially delivered"
    );
    assert_eq!(
        String::from_utf8_lossy(&output.stderr).trim(),
        format!("yoke: {marker}")
    );
}

fn state(engine: &Engine) -> Value {
    let mut result = json!({});
    for table in ["records", "snapshots", "graphs"] {
        let mut statement = engine
            .store
            .db
            .prepare(&format!("SELECT * FROM {table} ORDER BY 1,2"))
            .unwrap();
        let columns = statement.column_count();
        let rows: Vec<Vec<String>> = statement
            .query_map([], |row| {
                (0..columns)
                    .map(|column| row.get::<_, String>(column))
                    .collect()
            })
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        result[table] = json!(rows);
    }
    result
}

#[test]
fn compare_views_cli_replays_history_and_preserves_directed_parallel_changes_without_publication() {
    // Removing live source must not cause capture, default-view selection or empty success.
    let f = fixture(false);
    fs::remove_file(f.root.join("old.rs")).unwrap();
    let original = state(&f.engine);
    let output = request(&f).output().unwrap();
    let result = success(&output);
    assert_eq!(result["producer"], "yoke-view-comparison-1");
    assert_eq!(result["before"]["graphViewId"], f.left.id);
    assert_eq!(result["after"]["graphViewId"], f.right.id);
    assert_eq!(result["before"]["viewArtifactId"], f.left.artifact_id);
    assert_eq!(result["after"]["baseGraphHash"], f.right.base_graph_hash);
    assert_eq!(result["source"]["totals"]["beforeCalls"], 3);
    assert_eq!(result["source"]["totals"]["afterCalls"], 3);
    assert_eq!(result["source"]["totals"]["beforeSelfLoops"], 1);
    let calls: Vec<_> = result["source"]["observationChanges"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|row| row["relation"] == "calls")
        .collect();
    assert_eq!(calls.len(), 2);
    assert!(
        calls
            .iter()
            .any(|row| row["beforeCount"] == 2 && row["afterCount"] == 1 && row["delta"] == -1)
    );
    assert!(
        calls
            .iter()
            .any(|row| row["beforeCount"] == 0 && row["afterCount"] == 1 && row["delta"] == 1)
    );
    assert_eq!(result["complete"], false);
    assert_eq!(result["renameProofAvailable"], false);
    let again = request(&f).output().unwrap();
    success(&again);
    assert_eq!(output.stdout, again.stdout);
    assert_eq!(state(&f.engine), original);
    assert!(!f.root.join("old.rs").exists());
}

#[test]
fn compare_views_cli_keeps_all_five_input_pins_binding() {
    let f = fixture(false);
    success(&request(&f).output().unwrap());
    let original = state(&f.engine);
    for key in [
        "workspaceId",
        "snapshotId",
        "viewId",
        "viewArtifactId",
        "baseGraphHash",
    ] {
        let mut changed = pin(&f.right);
        changed[key] = json!("f".repeat(64));
        write_json(&f.after, &changed);
        refusal(
            request(&f).output().unwrap(),
            "view_comparison_input_invalid",
        );
        assert_eq!(state(&f.engine), original, "pin {key}");
    }
}

#[test]
fn compare_views_cli_rejects_bad_host_json_before_creating_source_state() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("source");
    fs::create_dir(&root).unwrap();
    let before = dir.path().join("before.json");
    let after = dir.path().join("after.json");
    let valid = json!({"schemaVersion":1,"workspaceId":"a".repeat(64),"snapshotId":"b".repeat(64),
        "viewId":"c".repeat(64),"viewArtifactId":"d".repeat(64),"baseGraphHash":"e".repeat(64)});
    write_json(&after, &valid);
    for mode in 0..8 {
        let mut selected = valid.clone();
        match mode {
            0 => {
                selected["workspaceId"] = json!("SOURCE_SECRET_DO_NOT_PRINT");
            }
            1 => {
                selected["unknown"] = json!("SOURCE_SECRET_DO_NOT_PRINT");
            }
            2 => {
                selected["schemaVersion"] = json!(2);
            }
            3 => {
                selected.as_object_mut().unwrap().remove("schemaVersion");
            }
            4 => {
                selected["viewId"] = Value::Null;
            }
            _ => {}
        }
        write_json(&before, &selected);
        if mode == 5 {
            fs::write(&before, r#"{"schemaVersion":1,"schemaVersion":1}"#).unwrap();
        }
        if mode == 6 {
            fs::write(&before, " ".repeat(4097)).unwrap();
        }
        if mode == 7 {
            fs::write(&before, [0xff, 0xfe]).unwrap();
        }
        refusal(
            blank(&root, &before, &after).output().unwrap(),
            "view_comparison_input_invalid",
        );
        assert!(!root.join(".yoke").exists());
    }
}

#[test]
fn compare_views_cli_rejects_execution_and_capture_options_before_host_file_access() {
    let f = fixture(false);
    success(&request(&f).output().unwrap());
    let original = state(&f.engine);
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("source");
    fs::create_dir(&root).unwrap();
    let absent = dir.path().join("SOURCE_SECRET_DO_NOT_PRINT");
    for flags in [
        vec!["--snapshot", "not-an-id"],
        vec!["--capture-config", "absent.json"],
        vec!["--include-ignored-dir", "secret"],
        vec!["--analyzer-epoch", "9"],
        vec!["--language-server-config", "absent.json"],
        vec!["--trust-servers"],
        vec!["--allow-writes"],
    ] {
        // Valid pins would succeed without the explicit flag rejection. Missing
        // files alone would produce the same INPUT marker and mask that bug.
        refusal(
            request(&f).args(&flags).output().unwrap(),
            "view_comparison_input_invalid",
        );
        assert_eq!(state(&f.engine), original);
        refusal(
            blank(&root, &absent, &absent).args(flags).output().unwrap(),
            "view_comparison_input_invalid",
        );
        assert!(!root.join(".yoke").exists());
    }
}

#[test]
fn compare_views_cli_zero_capacities_refuse_without_output_or_store_creation() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("source");
    fs::create_dir(&root).unwrap();
    let absent = dir.path().join("absent.json");
    for option in [
        "--max-bytes",
        "--max-rows",
        "--max-input-bytes",
        "--max-source-bytes",
    ] {
        refusal(
            blank(&root, &absent, &absent)
                .args([option, "0"])
                .output()
                .unwrap(),
            "view_comparison_capacity_exceeded",
        );
        assert!(!root.join(".yoke").exists());
    }
    refusal(
        blank(&root, &absent, &absent)
            .args(["--timeout", "0"])
            .output()
            .unwrap(),
        "view_comparison_deadline_exceeded",
    );
    assert!(!root.join(".yoke").exists());
}

#[test]
fn compare_views_cli_exact_output_limit_includes_its_single_newline() {
    let f = fixture(false);
    let first = request(&f).args(["--max-bytes", "65536"]).output().unwrap();
    success(&first);
    let exact = first.stdout.len();
    // The options carry decimal budgets; keeping the same width makes the
    // derived result length stable while the content hash may change.
    assert_eq!((exact - 1).to_string().len(), "65535".len());
    let accepted = request(&f)
        .args(["--max-bytes", &exact.to_string()])
        .output()
        .unwrap();
    let value = success(&accepted);
    assert_eq!(accepted.stdout.len(), exact);
    assert_eq!(accepted.stdout.last(), Some(&b'\n'));
    assert_eq!(value["options"]["maxOutputBytes"], exact - 1);
    refusal(
        request(&f)
            .args(["--max-bytes", &(exact - 1).to_string()])
            .output()
            .unwrap(),
        "view_comparison_capacity_exceeded",
    );
}

#[test]
fn compare_views_cli_whole_result_budgets_refuse_without_artifact_fallback() {
    let f = fixture(false);
    success(&request(&f).output().unwrap());
    let original = state(&f.engine);
    for option in ["--max-rows", "--max-input-bytes", "--max-source-bytes"] {
        refusal(
            request(&f).args([option, "1"]).output().unwrap(),
            "view_comparison_capacity_exceeded",
        );
        assert_eq!(state(&f.engine), original);
    }
}

#[test]
fn compare_views_cli_uses_only_explicit_one_to_one_primary_file_continuity() {
    let f = fixture(true);
    // A whole-file rename retains both sets of source rows. Select enough
    // output capacity for this complete fixture instead of changing defaults.
    let first = request(&f)
        .args(["--max-bytes", "400000"])
        .output()
        .unwrap();
    let without = success(&first);
    assert_eq!(without["declaredFileContinuity"], json!([]));
    let before = f.engine.store.snapshot(&f.left.snapshot_id).unwrap();
    let after = f.engine.store.snapshot(&f.right.snapshot_id).unwrap();
    let entry = json!({"beforePath":"old.rs","beforeHash":before.files[0].hash,
        "afterPath":"new.rs","afterHash":after.files[0].hash});
    let continuity = f._dir.path().join("continuity.json");
    write_json(
        &continuity,
        &json!({"schemaVersion":1,"declarations":[entry]}),
    );
    let original = state(&f.engine);
    let declared = request(&f)
        .args(["--max-bytes", "400000"])
        .arg("--continuity")
        .arg(&continuity)
        .output()
        .unwrap();
    let value = success(&declared);
    let rows = value["declaredFileContinuity"].as_array().unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["beforePath"], "old.rs");
    assert_eq!(rows[0]["afterPath"], "new.rs");
    assert_eq!(rows[0]["verifiedIdentity"], false);
    assert_eq!(value["renameProofAvailable"], false);
    assert_ne!(value["comparisonId"], without["comparisonId"]);
    for mode in 0..4 {
        let mut changed = entry.clone();
        let document = match mode {
            0 => {
                changed["beforeHash"] = json!("f".repeat(64));
                json!({"schemaVersion":1,"declarations":[changed]})
            }
            1 => json!({"schemaVersion":1,"declarations":[entry,entry]}),
            2 => {
                changed["beforePath"] = json!("../SOURCE_SECRET_DO_NOT_PRINT");
                json!({"schemaVersion":1,"declarations":[changed]})
            }
            _ => json!({"schemaVersion":1,"declarations":[],"automatic":true}),
        };
        write_json(&continuity, &document);
        refusal(
            request(&f)
                .arg("--continuity")
                .arg(&continuity)
                .output()
                .unwrap(),
            "view_comparison_input_invalid",
        );
        assert_eq!(state(&f.engine), original);
    }
}

#[test]
fn compare_views_cli_refuses_missing_or_incomplete_stores_without_initializing_them() {
    let f = fixture(false);
    let original = state(&f.engine);
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("source");
    fs::create_dir(&root).unwrap();
    refusal(
        blank(&root, &f.before, &f.after).output().unwrap(),
        "view_comparison_input_invalid",
    );
    assert!(!root.join(".yoke").exists());
    fs::create_dir(root.join(".yoke")).unwrap();
    let database = root.join(".yoke/index.sqlite");
    let raw = b"NOT_A_COMPLETE_STORE_SOURCE_SECRET_DO_NOT_PRINT";
    fs::write(&database, raw).unwrap();
    refusal(
        blank(&root, &f.before, &f.after).output().unwrap(),
        "view_comparison_input_invalid",
    );
    assert_eq!(fs::read(&database).unwrap(), raw);
    assert_eq!(fs::read_dir(root.join(".yoke")).unwrap().count(), 1);
    assert_eq!(state(&f.engine), original);
}

#[test]
fn compare_views_cli_reads_a_checkpointed_store_after_its_last_writer_closed() {
    let f = fixture(false);
    let Fixture {
        _dir,
        root,
        before,
        after,
        engine,
        ..
    } = f;
    drop(engine);
    assert!(!root.join(".yoke/index.sqlite-wal").exists());
    let output = blank(&root, &before, &after)
        .args(["--max-bytes", "400000"])
        .output()
        .unwrap();
    let result = success(&output);
    assert_eq!(result["source"]["totals"]["beforeCalls"], 3);
    assert_eq!(result["source"]["totals"]["afterCalls"], 3);
    assert_eq!(result["complete"], false);
}
