use serde_json::{Value, json};
use std::{fs, process::Command};
use tempfile::TempDir;
use yoke::Engine;
fn setup() -> (TempDir, Engine, String) {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("main.rs"), "pub fn value() -> u32 { 1 }\n").unwrap();
    let mut engine = Engine::open(dir.path()).unwrap();
    let read = engine.invoke("ci_read", json!({"path":"main.rs"})).unwrap();
    let plan=engine.invoke("ci_change_prepare",json!({"snapshotId":read["snapshotId"],"edits":[{"kind":"replace","path":"main.rs","expectedHash":read["data"]["contentHash"],"startByte":0,"endByte":0,"text":"// new\n"}]})).unwrap();
    assert_eq!(plan["status"], "ok");
    let id = plan["data"]["plan"]["id"].as_str().unwrap().to_string();
    (dir, engine, id)
}
#[test]
fn real_rustc_verification_approval_apply_and_rollback_preserve_original() {
    let (dir, mut engine, id) = setup();
    let output = Command::new(env!("CARGO_BIN_EXE_yoke"))
        .args([
            "--root",
            dir.path().to_str().unwrap(),
            "verify",
            "--plan",
            &id,
            "--trust-project",
            "--command",
            "rustc",
            "--arg=--crate-type=lib",
            "--arg=--emit=metadata",
            "--arg=main.rs",
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let verified: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(verified["status"], "passed");
    assert_eq!(verified["level"], "executed");
    engine.allow_writes = true;
    engine.approve(&id, false).unwrap();
    let applied = engine
        .invoke(
            "ci_change_apply",
            json!({"planId":id,"idempotencyKey":"workspace-test","mode":"workspace"}),
        )
        .unwrap();
    assert_eq!(applied["status"], "ok", "{applied}");
    assert!(
        fs::read_to_string(dir.path().join("main.rs"))
            .unwrap()
            .starts_with("// new")
    );
    let receipt = applied["data"]["receipt"]["id"].as_str().unwrap();
    yoke::changes::recover(&engine.store, &engine.root, receipt, true).unwrap();
    assert_eq!(
        fs::read_to_string(dir.path().join("main.rs")).unwrap(),
        "pub fn value() -> u32 { 1 }\n"
    );
}
#[test]
fn syntax_only_verification_does_not_authorize_workspace_writes() {
    let (_dir, mut engine, id) = setup();
    engine.allow_writes = true;
    engine.approve(&id, false).unwrap();
    let verify = engine.invoke("ci_verify", json!({"planId":id})).unwrap();
    assert_eq!(verify["data"]["verification"]["level"], "syntax-only");
    let result = engine
        .invoke(
            "ci_change_apply",
            json!({"planId":id,"idempotencyKey":"denied","mode":"workspace"}),
        )
        .unwrap();
    assert_eq!(result["status"], "error");
}

#[test]
fn rollback_restores_deleted_sources_and_keeps_foreign_writes() {
    let (dir, mut engine, _) = setup();
    let read = engine.invoke("ci_read", json!({"path":"main.rs"})).unwrap();
    let prepared=engine.invoke("ci_change_prepare",json!({"edits":[{"kind":"delete","path":"main.rs","expectedHash":read["data"]["contentHash"]}]})).unwrap();
    let id = prepared["data"]["plan"]["id"].as_str().unwrap();
    engine.store.record("verification",id,&json!({"status":"passed","level":"executed","snapshotId":prepared["data"]["plan"]["resultSnapshotId"]})).unwrap();
    engine.approve(id, false).unwrap();
    engine.allow_writes = true;
    let applied = engine
        .invoke(
            "ci_change_apply",
            json!({"planId":id,"idempotencyKey":"delete","mode":"workspace"}),
        )
        .unwrap();
    assert_eq!(applied["status"], "ok");
    assert!(!dir.path().join("main.rs").exists());
    let journal = applied["data"]["receipt"]["id"].as_str().unwrap();
    yoke::changes::recover(&engine.store, &engine.root, journal, true).unwrap();
    assert!(dir.path().join("main.rs").exists());
}

#[test]
fn executed_verification_deadline_includes_descendants_holding_output_pipes() {
    let (dir, _engine, id) = setup();
    let code = "import subprocess,sys; subprocess.Popen([sys.executable,'-c','import time; time.sleep(3)'])";
    let started = std::time::Instant::now();
    let output = Command::new(env!("CARGO_BIN_EXE_yoke"))
        .args([
            "--root",
            dir.path().to_str().unwrap(),
            "--timeout",
            "300",
            "verify",
            "--plan",
            &id,
            "--trust-project",
            "--command",
            "python",
            "--arg=-c",
            &format!("--arg={code}"),
        ])
        .output()
        .unwrap();
    assert!(
        started.elapsed() < std::time::Duration::from_secs(2),
        "Verification outlived deadline: {:?}",
        started.elapsed()
    );
    assert!(
        !output.status.success(),
        "Inherited output handles cannot establish completed verification"
    );
}
