#![cfg(windows)]
use serde_json::{Value, json};
use std::{
    fs,
    path::Path,
    process::{Command, Output, Stdio},
};
fn invoke(root: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_yoke"))
        .args(args)
        .current_dir(root)
        .output()
        .unwrap()
}
fn selection(root: &Path) -> std::path::PathBuf {
    for name in ["config", "source", "state"] {
        fs::create_dir(root.join(name)).unwrap();
    }
    fs::write(root.join("owned.exe"), b"never executed fixture").unwrap();
    fs::write(root.join("policy.json"), b"{}").unwrap();
    let path = root.join("selection.json");
    fs::write(&path,serde_json::to_vec(&json!({"installation":"cli-owned","targetRoots":[root.join("config")],"executable":root.join("owned.exe"),"policy":root.join("policy.json"),"stateRoot":root.join("state"),"adapter":{"kind":"codex_mcp_v1","sourceRoot":root.join("source")}})).unwrap()).unwrap();
    path
}
#[test]
fn branded_write_grants_and_foreign_source_flags_refuse_before_state_or_configuration() {
    let dir = tempfile::tempdir().unwrap();
    let missing = dir.path().join("missing.json");
    let out = invoke(
        dir.path(),
        &[
            "host-install",
            "apply",
            "--selection",
            missing.to_str().unwrap(),
            "--plan",
            missing.to_str().unwrap(),
        ],
    );
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("--allow-host-writes"));
    assert!(out.stdout.is_empty());
    let out = invoke(
        dir.path(),
        &[
            "--root",
            dir.path().to_str().unwrap(),
            "host-install",
            "status",
            "--selection",
            missing.to_str().unwrap(),
        ],
    );
    assert!(!out.status.success());
    assert!(!dir.path().join(".yoke").exists());
    assert!(!dir.path().join("config.toml").exists());
}
#[test]
fn closed_adapter_selection_refuses_unknown_profile_extra_grants_and_duplicate_roots() {
    let dir = tempfile::tempdir().unwrap();
    let path = selection(dir.path());
    let initial = fs::read(&path).unwrap();
    let original: Value = serde_json::from_slice(&initial).unwrap();
    let mut unknown = original.clone();
    unknown["adapter"]["kind"] = json!("automatic-client");
    let mut grant = original;
    grant["adapter"]["allowHostWrites"] = json!(true);
    let duplicate = String::from_utf8(initial)
        .unwrap()
        .replacen(
            "\"sourceRoot\":",
            "\"sourceRoot\":\"ignored\",\"sourceRoot\":",
            1,
        )
        .into_bytes();
    for bytes in [
        serde_json::to_vec(&unknown).unwrap(),
        serde_json::to_vec(&grant).unwrap(),
        duplicate,
    ] {
        fs::write(&path, bytes).unwrap();
        let out = invoke(
            dir.path(),
            &[
                "host-install",
                "plan",
                "--selection",
                path.to_str().unwrap(),
            ],
        );
        assert!(!out.status.success());
        assert!(out.stdout.is_empty());
        assert!(!dir.path().join("config/config.toml").exists());
        assert!(
            fs::read_dir(dir.path().join("state"))
                .unwrap()
                .next()
                .is_none()
        );
        assert!(!dir.path().join("source/.yoke").exists());
    }
}
#[test]
fn lost_stdout_retains_codex_receipt_identity_and_installed_configuration() {
    let dir = tempfile::tempdir().unwrap();
    let path = selection(dir.path());
    let config = dir.path().join("config/config.toml");
    fs::write(&config, b"# human retained\r\n").unwrap();
    let output = invoke(
        dir.path(),
        &[
            "--max-bytes",
            "200000",
            "host-install",
            "plan",
            "--selection",
            path.to_str().unwrap(),
        ],
    );
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let plan: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(plan["profile"], "codex-mcp-v1");
    let saved = dir.path().join("approved.json");
    fs::write(&saved, &output.stdout).unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_yoke"))
        .args([
            "host-install",
            "apply",
            "--selection",
            path.to_str().unwrap(),
            "--plan",
            saved.to_str().unwrap(),
            "--allow-host-writes",
        ])
        .current_dir(dir.path())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    drop(child.stdout.take());
    let out = child.wait_with_output().unwrap();
    assert!(!out.status.success());
    let error = String::from_utf8_lossy(&out.stderr);
    assert!(error.contains("durable") && error.contains(plan["id"].as_str().unwrap()));
    assert!(
        fs::read(&config)
            .unwrap()
            .starts_with(b"# human retained\r\n")
    );
    let out = invoke(
        dir.path(),
        &[
            "--max-bytes",
            "200000",
            "host-install",
            "status",
            "--selection",
            path.to_str().unwrap(),
        ],
    );
    assert!(out.status.success());
    let status: yoke::host_install::Status = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(status.state, "installed");
    let receipt_id = status.receipt_id.unwrap();
    assert!(yoke::model::valid_id(&receipt_id));
    let receipt: yoke::host_install::Receipt = serde_json::from_slice(
        &fs::read(
            dir.path()
                .join("state")
                .join(yoke::host_install::STATE_DIRECTORY)
                .join("receipts")
                .join(format!("{receipt_id}.json")),
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(receipt.id, receipt_id);
    assert_eq!(receipt.plan_id, plan["id"]);
    assert_eq!(receipt.profile, "codex-mcp-v1");
    assert_eq!(receipt.files[0].path, "config.toml");
    assert!(!dir.path().join("source/.yoke").exists());
}
