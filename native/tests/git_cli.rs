use serde_json::Value;
use std::{fs, process::Command};
use tempfile::TempDir;
fn git(root: &std::path::Path, args: &[&str]) {
    let result = Command::new("git")
        .args([
            "-c",
            "core.hooksPath=",
            "-c",
            "commit.gpgsign=false",
            "-c",
            "user.name=Test",
            "-c",
            "user.email=test@example.invalid",
        ])
        .args(args)
        .current_dir(root)
        .env(
            "GIT_CONFIG_GLOBAL",
            if cfg!(windows) { "NUL" } else { "/dev/null" },
        )
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
}
#[test]
fn public_git_cli_analyzes_and_reloads_immutable_reports_with_explicit_host_execution() {
    let root = TempDir::new().unwrap();
    git(root.path(), &["init", "--quiet"]);
    fs::write(root.path().join("main.rs"), "fn original() {}\n").unwrap();
    git(root.path(), &["add", "main.rs"]);
    git(root.path(), &["commit", "--quiet", "-m", "Initial"]);
    fs::write(root.path().join("main.rs"), "fn changed() {}\n").unwrap();
    let denied = Command::new(env!("CARGO_BIN_EXE_yoke"))
        .arg("--root")
        .arg(root.path())
        .args(["git", "analyze"])
        .output()
        .unwrap();
    assert!(!denied.status.success());
    let result = Command::new(env!("CARGO_BIN_EXE_yoke"))
        .arg("--root")
        .arg(root.path())
        .args(["git", "analyze", "--trust-git"])
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let report: Value = serde_json::from_slice(&result.stdout).unwrap();
    let id = report["artifactId"].as_str().unwrap();
    assert!(yoke::model::valid_id(id));
    assert!(!report["changes"].as_array().unwrap().is_empty());
    fs::write(root.path().join("main.rs"), "fn later(){}\n").unwrap();
    let loaded = Command::new(env!("CARGO_BIN_EXE_yoke"))
        .arg("--root")
        .arg(root.path())
        .args(["git", "load", id])
        .output()
        .unwrap();
    assert!(
        loaded.status.success(),
        "{}",
        String::from_utf8_lossy(&loaded.stderr)
    );
    assert_eq!(
        serde_json::from_slice::<Value>(&loaded.stdout).unwrap(),
        report
    );
}
