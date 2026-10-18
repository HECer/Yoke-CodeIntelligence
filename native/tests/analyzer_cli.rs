use serde_json::Value;
use std::{fs, path::Path, process::Command};
use tempfile::TempDir;
use yoke::{Engine, model::Graph};

fn run(root: &Path, args: &[&str]) -> Value {
    let output = Command::new(env!("CARGO_BIN_EXE_yoke"))
        .arg("--root")
        .arg(root)
        .args(args)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

fn snapshot_id(value: &Value) -> &str {
    value["snapshotId"].as_str().unwrap()
}

fn graph(root: &Path, snapshot: &str) -> Graph {
    Engine::open(root).unwrap().store.graph(snapshot).unwrap()
}

#[test]
fn explicit_cli_epoch_changes_new_capture_semantics_and_default_eight_is_stable() {
    let root = TempDir::new().unwrap();
    fs::write(
        root.path().join("sample.ps1"),
        "function Invoke-Work { }\ninvoke-work\n",
    )
    .unwrap();
    let original = run(root.path(), &["index"]);
    let nine = run(root.path(), &["--analyzer-epoch", "9", "index"]);
    let again = run(root.path(), &["index"]);
    assert_eq!(snapshot_id(&original), snapshot_id(&again));
    assert_ne!(snapshot_id(&original), snapshot_id(&nine));
    let old = graph(root.path(), snapshot_id(&original));
    let new = graph(root.path(), snapshot_id(&nine));
    assert!(
        old.observations
            .iter()
            .filter(|arc| arc.relation == "calls" && arc.target_label == "invoke-work")
            .all(|arc| arc.target.is_none())
    );
    assert!(new.observations.iter().any(|arc| {
        arc.relation == "calls" && arc.target_label == "invoke-work" && arc.target.is_some()
    }));
    assert_eq!(new.entities.len(), old.entities.len());
    let historical = run(
        root.path(),
        &[
            "--analyzer-epoch",
            "8",
            "--snapshot",
            snapshot_id(&nine),
            "find",
            "Invoke-Work",
        ],
    );
    assert_eq!(historical["snapshotId"], snapshot_id(&nine));
    assert_eq!(
        yoke::model::identity(&graph(root.path(), snapshot_id(&nine))),
        yoke::model::identity(&new)
    );
}

#[test]
fn invalid_cli_epoch_fails_before_creating_source_state() {
    let root = TempDir::new().unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_yoke"))
        .arg("--root")
        .arg(root.path())
        .args(["--analyzer-epoch", "10", "index"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(!root.path().join(".yoke").exists());
}

#[test]
fn analyzer_epoch_is_host_configuration_and_not_an_mcp_argument() {
    let root = TempDir::new().unwrap();
    let mut engine = Engine::open(root.path()).unwrap();
    let response = engine
        .invoke(
            "ci_find",
            serde_json::json!({"query":"x","analyzerEpoch":9}),
        )
        .unwrap();
    assert_eq!(response["status"], "error");
    assert!(engine.store.latest().unwrap().is_none());
}

#[test]
fn explicit_cli_session_epoch_survives_restart_and_rejects_an_unpinned_resume() {
    let root = TempDir::new().unwrap();
    let input = TempDir::new().unwrap();
    fs::write(
        root.path().join("sample.ps1"),
        "function Invoke-Work { }\ninvoke-work\n",
    )
    .unwrap();
    let requests = input.path().join("requests.json");
    fs::write(
        &requests,
        br#"[{"operation":"symbols.find","args":{"query":"Invoke-Work"}}]"#,
    )
    .unwrap();
    let requests = requests.to_str().unwrap();
    for _ in 0..2 {
        run(
            root.path(),
            &[
                "--analyzer-epoch",
                "9",
                "session",
                "--id",
                "epoch-cli",
                "--requests",
                requests,
            ],
        );
    }
    let engine = Engine::open(root.path()).unwrap();
    let latest = engine.store.latest().unwrap().unwrap();
    let source = engine.store.snapshot(&latest).unwrap();
    assert_eq!(
        yoke::capture::analyzer_epoch_for_snapshot_with_deadline(
            &engine.store,
            &source,
            &yoke::capture::Deadline::new(30_000)
        )
        .unwrap(),
        yoke::analysis::AnalyzerEpoch::V9
    );
    let before: Vec<String> = engine
        .store
        .db
        .prepare("SELECT value FROM records ORDER BY kind,id")
        .unwrap()
        .query_map([], |row| row.get(0))
        .unwrap()
        .collect::<rusqlite::Result<_>>()
        .unwrap();
    drop(engine);
    let refused = Command::new(env!("CARGO_BIN_EXE_yoke"))
        .arg("--root")
        .arg(root.path())
        .args(["session", "--id", "epoch-cli", "--requests", requests])
        .output()
        .unwrap();
    assert!(!refused.status.success());
    let engine = Engine::open(root.path()).unwrap();
    let after: Vec<String> = engine
        .store
        .db
        .prepare("SELECT value FROM records ORDER BY kind,id")
        .unwrap()
        .query_map([], |row| row.get(0))
        .unwrap()
        .collect::<rusqlite::Result<_>>()
        .unwrap();
    assert_eq!(before, after);
}
