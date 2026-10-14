use serde_json::{Value, json};
use std::{
    fs,
    io::Write,
    process::{Command, Stdio},
};
use tempfile::TempDir;
use yoke::{Engine, capture::Deadline};

#[test]
fn sqlite_sidecar_links_cannot_rewrite_files_outside_state() {
    for suffix in ["-wal", "-shm", "-journal"] {
        let dir = TempDir::new().unwrap();
        fs::create_dir(dir.path().join(".yoke")).unwrap();
        let target = dir.path().join("sentinel.bin");
        let bytes = vec![b'x'; 53000];
        fs::write(&target, &bytes).unwrap();
        let link = dir.path().join(format!(".yoke/index.sqlite{suffix}"));
        #[cfg(windows)]
        std::os::windows::fs::symlink_file(&target, &link).unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(&target, &link).unwrap();
        let result = Engine::open(dir.path());
        assert!(result.is_err(), "SQLite sidecar link accepted: {suffix}");
        assert_eq!(fs::read(&target).unwrap(), bytes);
        fs::remove_file(&link).unwrap();
    }
}

#[test]
fn protected_package_credentials_and_obvious_tokens_never_enter_snapshots() {
    let dir = TempDir::new().unwrap();
    let token = format!("ghp_{}", "a".repeat(36));
    fs::write(dir.path().join(".npmrc"), format!("_authToken={token}")).unwrap();
    fs::write(dir.path().join(".pypirc"), "password = test").unwrap();
    fs::write(
        dir.path().join("config.json"),
        format!("{{\"token\":\"{token}\"}}"),
    )
    .unwrap();
    fs::write(dir.path().join("safe.rs"), "fn safe() {}\n").unwrap();
    let mut engine = Engine::open(dir.path()).unwrap();
    let (snapshot, _, _, _) = engine.index(&Deadline::new(10000)).unwrap();
    assert_eq!(snapshot.files.len(), 1);
    assert_eq!(snapshot.files[0].path, "safe.rs");
    assert!(
        snapshot
            .omissions
            .iter()
            .any(|o| o.path == "config.json" && o.reason == "obvious-secret")
    );
}

#[test]
fn private_ec_keys_and_credential_file_variants_are_excluded() {
    let dir = TempDir::new().unwrap();
    for path in [
        "credentials.toml",
        "credentials.yaml",
        "vault.pfx",
        "vault.keystore",
    ] {
        fs::write(dir.path().join(path), "synthetic sensitive bytes").unwrap();
    }
    fs::write(
        dir.path().join("ec.txt"),
        "-----BEGIN EC PRIVATE KEY-----\nsynthetic\n-----END EC PRIVATE KEY-----",
    )
    .unwrap();
    fs::write(dir.path().join("safe.rs"), "fn safe() {}\n").unwrap();
    let mut engine = Engine::open(dir.path()).unwrap();
    let (snapshot, _, _, _) = engine.index(&Deadline::new(10000)).unwrap();
    assert_eq!(
        snapshot
            .files
            .iter()
            .map(|file| file.path.as_str())
            .collect::<Vec<_>>(),
        vec!["safe.rs"]
    );
    assert!(
        snapshot
            .omissions
            .iter()
            .any(|o| o.path == "ec.txt" && o.reason == "obvious-secret")
    );
}

#[test]
fn windows_device_names_and_path_aliases_are_never_edit_targets() {
    for path in [
        "NUL",
        "CON.txt",
        "a/aux.rs",
        "LPT1.log",
        "COM9",
        "name. ",
        "name.",
        "a/name ",
        "dir/<file>",
    ] {
        assert!(
            yoke::paths::normalized(path).is_err(),
            "Unsafe portable path accepted: {path}"
        );
    }
    assert!(yoke::paths::normalized("src/control.rs").is_ok());
}

#[test]
fn huge_unicode_argument_errors_cannot_abort_the_mcp_session() {
    let dir = TempDir::new().unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_yoke"))
        .args(["--root", dir.path().to_str().unwrap(), "serve"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut args = json!({"maxBytes":1024});
    args["🌍".repeat(500)] = json!(true);
    let messages = [
        json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}),
        json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"ci_status","arguments":args}}),
        json!({"jsonrpc":"2.0","id":3,"method":"ping"}),
    ];
    let mut input = child.stdin.take().unwrap();
    for message in messages {
        writeln!(input, "{message}").unwrap();
    }
    drop(input);
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let values: Vec<Value> = String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(values.len(), 3);
    assert_eq!(values[1]["result"]["isError"], true);
    assert_eq!(values[2]["result"], json!({}));
}

#[test]
fn bare_package_imports_do_not_become_syntactic_sibling_file_bindings() {
    let dir = TempDir::new().unwrap();
    fs::write(
        dir.path().join("consumer.ts"),
        "import { helper } from 'lib';\n",
    )
    .unwrap();
    fs::write(dir.path().join("lib.ts"), "export function helper() {}\n").unwrap();
    let mut engine = Engine::open(dir.path()).unwrap();
    let (_, graph, _, _) = engine.index(&Deadline::new(10000)).unwrap();
    let imported = graph
        .observations
        .iter()
        .find(|o| o.relation == "imports")
        .unwrap();
    assert!(imported.target.is_none());
    assert_eq!(imported.resolution, "unresolved");
}

#[test]
fn unchanged_index_republishes_the_disk_snapshot_after_patch_preparation() {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("main.rs"), "fn main() {}\n").unwrap();
    let mut engine = Engine::open(dir.path()).unwrap();
    let read = engine.invoke("ci_read", json!({"path":"main.rs"})).unwrap();
    let plan=engine.invoke("ci_change_prepare",json!({"edits":[{"kind":"replace","path":"main.rs","expectedHash":read["data"]["contentHash"],"startByte":0,"endByte":0,"text":"// added\n"}]})).unwrap();
    assert_eq!(plan["status"], "ok");
    let indexed = engine.invoke("ci_status", json!({"mode":"index"})).unwrap();
    let status = engine.invoke("ci_status", json!({})).unwrap();
    assert_eq!(indexed["snapshotId"], read["snapshotId"]);
    assert_eq!(status["data"]["latestSnapshotId"], read["snapshotId"]);
}

#[test]
fn missing_anonymous_tokens_are_reported_as_syntax_diagnostics() {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("main.rs"), "fn main() { let x = 1 }\n").unwrap();
    let mut engine = Engine::open(dir.path()).unwrap();
    let (_, graph, _, _) = engine.index(&Deadline::new(10000)).unwrap();
    assert!(!graph.diagnostics.is_empty());
}

#[test]
fn syntax_delta_is_multiplicity_aware_and_test_paths_are_case_insensitive() {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("main.rs"), "fn old() { @ }\n").unwrap();
    fs::write(dir.path().join("UnitTests.cs"), "class UnitTests {}\n").unwrap();
    let mut engine = Engine::open(dir.path()).unwrap();
    let read = engine.invoke("ci_read", json!({"path":"main.rs"})).unwrap();
    let plan=engine.invoke("ci_change_prepare",json!({"edits":[{"kind":"replace","path":"main.rs","expectedHash":read["data"]["contentHash"],"startByte":0,"endByte":0,"text":"fn new() { @ }\n"}]})).unwrap();
    let verification = engine
        .invoke("ci_verify", json!({"planId":plan["data"]["plan"]["id"]}))
        .unwrap();
    assert_eq!(verification["data"]["verification"]["status"], "failed");
    let read = engine
        .invoke("ci_read", json!({"path":"UnitTests.cs"}))
        .unwrap();
    let prepared=engine.invoke("ci_change_prepare",json!({"edits":[{"kind":"delete","path":"UnitTests.cs","expectedHash":read["data"]["contentHash"]}]})).unwrap();
    let id = prepared["data"]["plan"]["id"].as_str().unwrap();
    assert!(engine.approve(id, false).is_err());
}

#[test]
fn derived_source_cannot_introduce_obvious_credentials_into_state() {
    let dir = TempDir::new().unwrap();
    let mut engine = Engine::open(dir.path()).unwrap();
    let secret = format!("ghp_{}", "b".repeat(36));
    let result = engine
        .invoke(
            "ci_change_prepare",
            json!({"edits":[{"kind":"create","path":"config.json","text":secret}]}),
        )
        .unwrap();
    assert_eq!(result["status"], "error");
}

#[test]
fn conservative_token_budget_bounds_the_serialized_result() {
    let dir = TempDir::new().unwrap();
    let mut engine = Engine::open(dir.path()).unwrap();
    let result = engine
        .invoke("ci_status", json!({"tokenBudget":300}))
        .unwrap();
    assert!(result["metrics"]["estimatedTokens"].as_u64().unwrap() <= 300);
}

#[test]
fn insufficient_workspace_response_budget_is_rejected_before_a_write() {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("main.rs"), "fn main(){}\n").unwrap();
    let mut engine = Engine::open(dir.path()).unwrap();
    engine.allow_writes = true;
    let result = engine.invoke("ci_change_apply", json!({"planId":"a".repeat(64),"mode":"workspace","idempotencyKey":"small","tokenBudget":200}));
    assert!(result.unwrap_err().to_string().contains("recovery"));
    assert_eq!(
        fs::read_to_string(dir.path().join("main.rs")).unwrap(),
        "fn main(){}\n"
    );
}

#[test]
fn cli_trace_honors_a_requested_snapshot_after_disk_changes() {
    let dir = TempDir::new().unwrap();
    fs::write(
        dir.path().join("main.rs"),
        "fn helper(){} fn main(){helper();}\n",
    )
    .unwrap();
    let mut engine = Engine::open(dir.path()).unwrap();
    let (snapshot, _, _, _) = engine.index(&Deadline::new(10000)).unwrap();
    fs::write(dir.path().join("main.rs"), "fn unrelated(){}\n").unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_yoke"))
        .args([
            "--root",
            dir.path().to_str().unwrap(),
            "--snapshot",
            &snapshot.id,
            "trace",
            "main",
            "--relation",
            "calls",
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let result: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(result["snapshotId"], snapshot.id);
    assert_eq!(result["data"]["observations"].as_array().unwrap().len(), 1);
}

#[test]
fn sqlite_writer_contention_respects_the_cli_deadline() {
    let dir = TempDir::new().unwrap();
    let _engine = Engine::open(dir.path()).unwrap();
    fs::write(dir.path().join("new.rs"), "fn example() {}\n").unwrap();
    let blocker = rusqlite::Connection::open(dir.path().join(".yoke/index.sqlite")).unwrap();
    blocker.execute_batch("BEGIN IMMEDIATE;").unwrap();
    let started = std::time::Instant::now();
    let output = Command::new(env!("CARGO_BIN_EXE_yoke"))
        .args([
            "--root",
            dir.path().to_str().unwrap(),
            "--timeout",
            "200",
            "index",
        ])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(
        started.elapsed() < std::time::Duration::from_secs(2),
        "SQLite outlived deadline: {:?}",
        started.elapsed()
    );
    blocker.execute_batch("ROLLBACK;").unwrap();
}
