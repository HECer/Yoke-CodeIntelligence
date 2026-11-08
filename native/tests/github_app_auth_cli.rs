use std::{path::Path, process::Command};

fn refused(root: &Path, args: &[&str], marker: &str) {
    let result = Command::new(env!("CARGO_BIN_EXE_yoke"))
        .args(args)
        .current_dir(root)
        .output()
        .unwrap();
    assert!(!result.status.success());
    assert!(result.stdout.is_empty());
    assert!(String::from_utf8_lossy(&result.stderr).contains(marker));
    assert_eq!(std::fs::read_dir(root).unwrap().count(), 0);
}
#[test]
fn authorize_network_permission_precedes_ids_profile_and_source() {
    let root = tempfile::tempdir().unwrap();
    refused(
        root.path(),
        &[
            "--root",
            "absent-source",
            "github-app",
            "authorize",
            "--profile",
            "absent.json",
        ],
        "github_app_network_permission_required",
    );
    refused(
        root.path(),
        &["github-app", "--allow-webhooks", "authorize"],
        "github_app_network_permission_required",
    );
}
#[test]
fn authorize_does_not_accept_source_or_other_write_authority() {
    let root = tempfile::tempdir().unwrap();
    for args in [
        vec![
            "--root",
            "absent-source",
            "github-app",
            "--allow-github",
            "authorize",
        ],
        vec![
            "--allow-writes",
            "github-app",
            "--allow-github",
            "authorize",
        ],
        vec![
            "github-app",
            "--allow-github",
            "--allow-webhooks",
            "authorize",
        ],
    ] {
        refused(root.path(), &args, "github_app_auth_input_invalid");
    }
}
#[test]
fn authorize_zero_and_insufficient_request_budget_precede_missing_host_files() {
    let root = tempfile::tempdir().unwrap();
    for budget in ["0", "3"] {
        refused(
            root.path(),
            &[
                "github-app",
                "--allow-github",
                "authorize",
                "--installation",
                "31",
                "--repository",
                "47",
                "--profile",
                "absent.json",
                "--max-requests",
                budget,
            ],
            "github_app_auth_capacity_exceeded",
        );
    }
    refused(
        root.path(),
        &[
            "--max-bytes",
            "0",
            "github-app",
            "--allow-github",
            "authorize",
            "--profile",
            "absent.json",
        ],
        "github_app_auth_capacity_exceeded",
    );
}
#[test]
fn private_key_import_rejects_malformed_input_without_opening_state() {
    use std::io::Write;
    let root = tempfile::tempdir().unwrap();
    let name = "never-created-malformed-app-key";
    let mut child = Command::new(env!("CARGO_BIN_EXE_yoke"))
        .args([
            "--root",
            "absent-source",
            "credentials",
            "github-app-key-import",
            name,
        ])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .current_dir(root.path())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(b"private-key-canary-malformed")
        .unwrap();
    let result = child.wait_with_output().unwrap();
    assert!(!result.status.success());
    assert!(result.stdout.is_empty());
    let error = String::from_utf8_lossy(&result.stderr);
    assert!(error.contains("github_app_key_invalid"));
    assert!(!error.contains("private-key-canary-malformed"));
    assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 0);
}
