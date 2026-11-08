use serde_json::{Value, json};
use std::{
    fs,
    path::Path,
    process::{Command, Output},
};
use yoke::{capture::Deadline, postgres_catalog::Catalog};
fn invoke(args: &[&str], cwd: &Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_yoke"))
        .args(args)
        .current_dir(cwd)
        .output()
        .unwrap()
}
fn artifact(root: &Path) -> std::path::PathBuf {
    let c = Catalog::from_response(
        include_bytes!("fixtures/postgres-catalog-response.jsonl"),
        &Deadline::new(30000),
    )
    .unwrap();
    let path = root.join("catalog.json");
    fs::write(&path, c.encode_archive(&Deadline::new(30000)).unwrap()).unwrap();
    path
}
#[test]
fn frozen_read_does_not_initialize_a_workspace_and_preserves_supplied_files() {
    let temp = tempfile::tempdir().unwrap();
    let path = artifact(temp.path());
    let before = fs::read(&path).unwrap();
    let p = invoke(
        &[
            "--max-bytes",
            "1000000",
            "postgres-catalog",
            "read",
            "--input",
            path.to_str().unwrap(),
        ],
        temp.path(),
    );
    assert!(p.status.success(), "{}", String::from_utf8_lossy(&p.stderr));
    let value: Value = serde_json::from_slice(&p.stdout).unwrap();
    assert_eq!(value["nodes"].as_array().unwrap().len(), 7);
    assert_eq!(value["networkConnections"], 0);
    assert_eq!(fs::read(&path).unwrap(), before);
    assert!(!temp.path().join(".yoke").exists());
    assert!(!temp.path().join(".code-intelligence").exists());
}
#[test]
fn permissions_and_source_flags_refuse_before_config_lookup_or_store_open() {
    let temp = tempfile::tempdir().unwrap();
    let missing = temp.path().join("missing-config.json");
    let output = temp.path().join("output.json");
    let p = invoke(
        &[
            "postgres-catalog",
            "capture",
            "--config",
            missing.to_str().unwrap(),
            "--output",
            output.to_str().unwrap(),
        ],
        temp.path(),
    );
    assert!(!p.status.success());
    assert!(
        String::from_utf8_lossy(&p.stderr).contains("postgres_catalog_host_permission_required")
    );
    assert!(!output.exists());
    let p = invoke(
        &[
            "--root",
            temp.path().to_str().unwrap(),
            "postgres-catalog",
            "read",
            "--input",
            missing.to_str().unwrap(),
        ],
        temp.path(),
    );
    assert!(!p.status.success());
    assert!(
        String::from_utf8_lossy(&p.stderr).contains("source/provider/write flags are unavailable")
    );
    assert!(!temp.path().join(".yoke").exists());
    let p = invoke(
        &[
            "postgres-catalog",
            "import-password",
            "--config",
            missing.to_str().unwrap(),
        ],
        temp.path(),
    );
    assert!(!p.status.success());
    assert!(
        String::from_utf8_lossy(&p.stderr)
            .contains("postgres_catalog_credential_permission_required")
    );
}
#[test]
fn existing_output_and_unsafe_paths_are_preserved_before_client_execution() {
    let temp = tempfile::tempdir().unwrap();
    let config = temp.path().join("config.json");
    let output = temp.path().join("existing.json");
    fs::write(&output, b"foreign bytes").unwrap();
    let spec = json!({"client":temp.path().join("absent-client.exe"),"clientSha256":"0".repeat(64),"address":"127.0.0.1","port":5432,"database":"postgres","user":"reader","security":{"kind":"loopback_trust"}});
    fs::write(&config, serde_json::to_vec(&spec).unwrap()).unwrap();
    let p = invoke(
        &[
            "postgres-catalog",
            "capture",
            "--config",
            config.to_str().unwrap(),
            "--output",
            output.to_str().unwrap(),
            "--trust-client",
            "--allow-network",
            "--allow-output",
            "--allow-loopback-trust",
        ],
        temp.path(),
    );
    assert!(!p.status.success());
    assert!(String::from_utf8_lossy(&p.stderr).contains("fresh file"));
    assert_eq!(fs::read(&output).unwrap(), b"foreign bytes");
    let p = invoke(
        &["postgres-catalog", "read", "--input", "relative.json"],
        temp.path(),
    );
    assert!(!p.status.success());
    assert!(!temp.path().join(".yoke").exists());
}
#[test]
fn closed_queries_and_small_output_budgets_cannot_report_partial_success() {
    let temp = tempfile::tempdir().unwrap();
    let path = artifact(temp.path());
    for query in [
        r#"{"operation":"inspect","complete":true}"#,
        r#"{"operation":"inspect","operation":"node"}"#,
    ] {
        let p = invoke(
            &[
                "postgres-catalog",
                "read",
                "--input",
                path.to_str().unwrap(),
                "--query",
                query,
            ],
            temp.path(),
        );
        assert!(!p.status.success());
        assert!(p.stdout.is_empty());
    }
    let p = invoke(
        &[
            "--max-bytes",
            "1024",
            "postgres-catalog",
            "read",
            "--input",
            path.to_str().unwrap(),
        ],
        temp.path(),
    );
    assert!(!p.status.success());
    assert!(p.stdout.is_empty());
}
