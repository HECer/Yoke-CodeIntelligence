use serde_json::{Value, json};
use std::{
    fs,
    path::Path,
    process::{Command, Output},
};

const SOURCE: &str = "pub mod storage;\n";

fn source() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("lib.rs"), SOURCE).unwrap();
    dir
}

fn draft(body: &str) -> Value {
    json!({"draft":{"key":"storage-policy","kind":"decision","title":"Keep source immutable","body":body,"author":"Ada","date":"2026-10-15","paths":["lib.rs"],"rules":[{"kind":"follows","path":"lib.rs","literal":"pub mod"}]}})
}

fn call(root: &Path, operation: &str, input: &Value, flags: &[&str]) -> Output {
    let host = tempfile::tempdir().unwrap();
    let input_path = host.path().join("input.json");
    fs::write(&input_path, serde_json::to_vec(input).unwrap()).unwrap();
    Command::new(env!("CARGO_BIN_EXE_yoke"))
        .arg("--root")
        .arg(root)
        .arg("decisions")
        .arg(operation)
        .arg("--input")
        .arg(&input_path)
        .args(flags)
        .output()
        .unwrap()
}

fn success(output: Output) -> Value {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

#[test]
fn decision_cli_creates_reads_and_conditionally_updates_immutable_revisions() {
    let dir = source();
    let first = success(call(
        dir.path(),
        "write",
        &draft("Keep captured source."),
        &["--allow-record-writes"],
    ));
    assert_eq!(first["revision"]["number"], 1);
    let old = first["revision"]["id"].as_str().unwrap();
    let read = success(call(
        dir.path(),
        "read",
        &json!({"key":"storage-policy"}),
        &[],
    ));
    assert_eq!(read["revision"]["id"], old);
    let mut replacement = draft("Keep captured source and immutable history.");
    replacement["expectedRevision"] = json!(old);
    let second = success(call(
        dir.path(),
        "write",
        &replacement,
        &["--allow-record-writes"],
    ));
    assert_eq!(second["revision"]["number"], 2);
    assert_eq!(second["revision"]["parentRevision"], old);
    let stale = call(
        dir.path(),
        "write",
        &replacement,
        &["--allow-record-writes"],
    );
    assert!(!stale.status.success());
    let read = success(call(
        dir.path(),
        "read",
        &json!({"key":"storage-policy"}),
        &[],
    ));
    assert_eq!(read["revision"]["id"], second["revision"]["id"]);
    assert_eq!(
        fs::read_to_string(dir.path().join("lib.rs")).unwrap(),
        SOURCE
    );
}

#[test]
fn decision_cli_write_and_import_require_host_permission_before_opening_state() {
    for operation in ["write", "import"] {
        let dir = source();
        let personal = tempfile::tempdir().unwrap();
        let output = call(
            dir.path(),
            operation,
            &json!({"allowRecordWrites":true}),
            &[
                "--allow-writes",
                "--personal-root",
                personal.path().to_str().unwrap(),
                "--host-id",
                "fixture-host",
            ],
        );
        assert!(!output.status.success());
        assert!(String::from_utf8_lossy(&output.stderr).contains("--allow-record-writes"));
        assert!(!dir.path().join(".yoke").exists());
        assert!(!personal.path().join(".yoke").exists());
    }
}

#[test]
fn decision_cli_rejects_provider_configuration_before_opening_source_state() {
    let dir = source();
    let output = call(
        dir.path(),
        "list",
        &json!({}),
        &[
            "--language-server-config",
            "missing-provider.json",
            "--trust-servers",
        ],
    );
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("language servers"));
    assert!(!dir.path().join(".yoke").exists());
}

#[test]
fn decision_cli_inline_arguments_are_strict_and_cannot_select_a_second_input() {
    let dir = source();
    let inline = Command::new(env!("CARGO_BIN_EXE_yoke"))
        .arg("--root")
        .arg(dir.path())
        .args(["decisions", "list", "--args", "{}"])
        .output()
        .unwrap();
    assert_eq!(success(inline)["records"], json!([]));
    let pristine = source();
    let duplicate = Command::new(env!("CARGO_BIN_EXE_yoke"))
        .arg("--root")
        .arg(pristine.path())
        .args(["decisions", "read", "--args", r#"{"key":"a","key":"b"}"#])
        .output()
        .unwrap();
    assert!(!duplicate.status.success());
    assert!(String::from_utf8_lossy(&duplicate.stderr).contains("Duplicate JSON key"));
    assert!(!pristine.path().join(".yoke").exists());
    let conflict = Command::new(env!("CARGO_BIN_EXE_yoke"))
        .arg("--root")
        .arg(pristine.path())
        .args([
            "decisions",
            "list",
            "--args",
            "{}",
            "--input",
            "missing.json",
        ])
        .output()
        .unwrap();
    assert!(!conflict.status.success());
    assert!(!pristine.path().join(".yoke").exists());
}

#[test]
fn decision_cli_rejects_ambiguous_and_oversized_json_before_opening_state() {
    for bytes in [
        r#"{"key":"storage-policy","key":"other"}"#.to_owned(),
        format!("{{\"key\":\"{}\"}}", "x".repeat(128_000)),
    ] {
        let dir = source();
        let host = tempfile::tempdir().unwrap();
        let path = host.path().join("input.json");
        fs::write(&path, bytes).unwrap();
        let output = Command::new(env!("CARGO_BIN_EXE_yoke"))
            .arg("--root")
            .arg(dir.path())
            .args(["decisions", "read", "--input"])
            .arg(path)
            .output()
            .unwrap();
        assert!(!output.status.success());
        assert!(
            String::from_utf8_lossy(&output.stderr).contains("Duplicate JSON key")
                || String::from_utf8_lossy(&output.stderr).contains("capacity")
        );
        assert!(!dir.path().join(".yoke").exists());
    }
}

#[test]
fn decision_cli_personal_scope_is_explicit_isolated_and_pinned_to_host() {
    let dir = source();
    let personal = tempfile::tempdir().unwrap();
    let flags = [
        "--personal-root",
        personal.path().to_str().unwrap(),
        "--host-id",
        "fixture-host",
        "--allow-record-writes",
    ];
    let written = success(call(
        dir.path(),
        "write",
        &draft("Personal preference."),
        &flags,
    ));
    let local = success(call(dir.path(), "list", &json!({}), &[]));
    assert_eq!(local["records"].as_array().unwrap().len(), 0);
    let personal_read = success(call(
        dir.path(),
        "read",
        &json!({"key":"storage-policy"}),
        &flags[..4],
    ));
    assert_eq!(personal_read["revision"]["id"], written["revision"]["id"]);
    let wrong_host = call(
        dir.path(),
        "list",
        &json!({}),
        &[
            "--personal-root",
            personal.path().to_str().unwrap(),
            "--host-id",
            "other-host",
        ],
    );
    assert!(!wrong_host.status.success());
    assert_eq!(
        fs::read_to_string(dir.path().join("lib.rs")).unwrap(),
        SOURCE
    );
}

#[test]
fn decision_cli_rejects_incomplete_or_unsafe_personal_scope_without_creating_state() {
    let dir = source();
    let personal = tempfile::tempdir().unwrap();
    for flags in [
        vec!["--personal-root", personal.path().to_str().unwrap()],
        vec!["--host-id", "fixture-host"],
        vec!["--personal-root", ".", "--host-id", "fixture-host"],
        vec![
            "--personal-root",
            dir.path().to_str().unwrap(),
            "--host-id",
            "fixture-host",
        ],
        vec![
            "--personal-root",
            personal.path().to_str().unwrap(),
            "--host-id",
            "bad\nhost",
        ],
    ] {
        assert!(
            !call(dir.path(), "list", &json!({}), &flags)
                .status
                .success()
        );
        assert!(!dir.path().join(".yoke").exists());
        assert!(!personal.path().join(".yoke").exists());
    }
}

#[test]
fn decision_cli_exports_fresh_bundles_and_imports_without_replacing_documents() {
    let dir = source();
    let host = tempfile::tempdir().unwrap();
    let personal = tempfile::tempdir().unwrap();
    let written = success(call(
        dir.path(),
        "write",
        &draft("Keep source immutable."),
        &["--allow-record-writes"],
    ));
    let bundle = host.path().join("bundle");
    success(call(
        dir.path(),
        "export",
        &json!({"destination":bundle}),
        &[],
    ));
    let original = fs::read(bundle.join("storage-policy.md")).unwrap();
    assert!(
        !call(dir.path(), "export", &json!({"destination":bundle}), &[])
            .status
            .success()
    );
    let flags = [
        "--personal-root",
        personal.path().to_str().unwrap(),
        "--host-id",
        "fixture-host",
        "--allow-record-writes",
    ];
    let imported = success(call(
        dir.path(),
        "import",
        &json!({"directory":bundle,"expectedHeads":{"storage-policy":null}}),
        &flags,
    ));
    assert_eq!(imported["importedHeads"], 1);
    let read = success(call(
        dir.path(),
        "read",
        &json!({"key":"storage-policy"}),
        &flags[..4],
    ));
    assert_eq!(read["revision"]["id"], written["revision"]["id"]);
    assert_eq!(
        fs::read(bundle.join("storage-policy.md")).unwrap(),
        original
    );
    assert_eq!(
        fs::read_to_string(dir.path().join("lib.rs")).unwrap(),
        SOURCE
    );
}

#[test]
fn decision_cli_checks_current_captured_source_and_preserves_historical_snapshot() {
    let dir = source();
    let written = success(call(
        dir.path(),
        "write",
        &draft("Keep the module public."),
        &["--allow-record-writes"],
    ));
    let snapshot = written["revision"]["snapshotId"].as_str().unwrap();
    let baseline = success(call(
        dir.path(),
        "check",
        &json!({"key":"storage-policy"}),
        &[],
    ));
    assert_eq!(baseline["rules"][0]["outcome"], "followed");
    fs::write(dir.path().join("lib.rs"), "mod storage;\n").unwrap();
    let current = success(call(
        dir.path(),
        "check",
        &json!({"key":"storage-policy"}),
        &[],
    ));
    assert_eq!(
        current["classification"],
        "literal_heuristic_not_semantic_proof"
    );
    assert_eq!(current["rules"][0]["outcome"], "violated");
    let old = success(call(
        dir.path(),
        "check",
        &json!({"key":"storage-policy"}),
        &["--snapshot", snapshot],
    ));
    assert_eq!(old["rules"][0]["outcome"], "followed");
    assert_eq!(
        fs::read_to_string(dir.path().join("lib.rs")).unwrap(),
        "mod storage;\n"
    );
}

#[test]
fn decision_cli_output_budget_retains_the_complete_result_as_an_immutable_artifact() {
    let dir = source();
    let body = "Authored storage guidance. ".repeat(400);
    let output = call(
        dir.path(),
        "write",
        &draft(&body),
        &["--allow-record-writes", "--max-bytes", "1024"],
    );
    assert!(output.stdout.len() <= 1024);
    let result = success(output);
    assert_eq!(result["status"], "partial");
    let engine = yoke::Engine::open(dir.path()).unwrap();
    let artifact = result["artifactId"].as_str().unwrap();
    let data = engine.store.read_blob(artifact).unwrap();
    let full: Value = serde_json::from_slice(&data).unwrap();
    assert_eq!(full["revision"]["draft"]["body"], body);
    assert_eq!(
        fs::read_to_string(dir.path().join("lib.rs")).unwrap(),
        SOURCE
    );
}
