//! Real native permission and budget ordering without host keys or source discovery.
use std::{path::Path, process::Command};
fn refused(root: &Path, args: &[&str], marker: &str) {
    let output = Command::new(env!("CARGO_BIN_EXE_yoke"))
        .args(args)
        .current_dir(root)
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    assert!(
        String::from_utf8_lossy(&output.stderr).contains(marker),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(std::fs::read_dir(root).unwrap().count(), 0);
}
#[test]
fn worker_mutation_permissions_precede_missing_request_profiles_and_source() {
    let root = tempfile::tempdir().unwrap();
    for args in [
        vec!["--root", "absent-source", "github-app", "worker", "claim"],
        vec![
            "--root",
            "absent-source",
            "github-app",
            "--allow-webhooks",
            "worker",
            "claim",
        ],
        vec![
            "--allow-writes",
            "--root",
            "absent-source",
            "github-app",
            "worker",
            "claim",
        ],
        vec!["github-app", "worker", "status"],
    ] {
        refused(root.path(), &args, "github_app_permission_required");
    }
    refused(
        root.path(),
        &[
            "--allow-writes",
            "github-app",
            "--allow-webhooks",
            "worker",
            "run",
        ],
        "github_app_network_permission_required",
    );
}
#[test]
fn worker_zero_history_and_output_limits_precede_all_host_files() {
    let root = tempfile::tempdir().unwrap();
    for flag in [
        "--max-records",
        "--max-worker-store-bytes",
        "--max-lease-seconds",
    ] {
        refused(
            root.path(),
            &[
                "--allow-writes",
                "github-app",
                "--allow-webhooks",
                "worker",
                flag,
                "0",
                "claim",
            ],
            "github_app_worker_capacity_exceeded",
        );
    }
    refused(
        root.path(),
        &[
            "--max-bytes",
            "0",
            "--allow-writes",
            "github-app",
            "--allow-webhooks",
            "worker",
            "claim",
        ],
        "github_app_worker_capacity_exceeded",
    );
}
#[test]
fn worker_zero_remote_limits_precede_auth_files_credentials_and_source() {
    let root = tempfile::tempdir().unwrap();
    for flag in [
        "--max-requests",
        "--max-response-bytes",
        "--max-total-bytes",
        "--max-report-bytes",
        "--max-files",
    ] {
        refused(
            root.path(),
            &[
                "--allow-writes",
                "github-app",
                "--allow-webhooks",
                "--allow-github",
                "worker",
                "run",
                flag,
                "0",
            ],
            "github_app_worker_capacity_exceeded",
        );
    }
}
#[test]
fn read_only_worker_commands_refuse_network_and_write_authority() {
    let root = tempfile::tempdir().unwrap();
    for args in [
        vec![
            "github-app",
            "--allow-webhooks",
            "--allow-github",
            "worker",
            "status",
        ],
        vec![
            "--allow-writes",
            "github-app",
            "--allow-webhooks",
            "worker",
            "status",
        ],
    ] {
        refused(root.path(), &args, "github_app_worker_input_invalid");
    }
}

#[test]
fn worker_inherited_zero_queue_limits_precede_missing_host_files_and_keys() {
    let root = tempfile::tempdir().unwrap();
    for flag in [
        "--max-payload-bytes",
        "--max-deliveries",
        "--max-jobs",
        "--max-transitions",
        "--max-store-bytes",
    ] {
        refused(
            root.path(),
            &[
                "--allow-writes",
                "github-app",
                "--allow-webhooks",
                flag,
                "0",
                "worker",
                "claim",
            ],
            "github_app_worker_capacity_exceeded",
        );
    }
}
