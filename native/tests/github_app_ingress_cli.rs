use std::{path::Path, process::Command};
fn refuses(root: &Path, args: &[&str], code: &str) {
    let result = Command::new(env!("CARGO_BIN_EXE_yoke"))
        .args(args)
        .current_dir(root)
        .output()
        .unwrap();
    assert!(!result.status.success());
    assert!(result.stdout.is_empty());
    assert!(String::from_utf8_lossy(&result.stderr).contains(code));
    assert_eq!(std::fs::read_dir(root).unwrap().count(), 0);
}
#[test]
fn serve_permissions_precede_missing_files_state_and_listener() {
    let root = tempfile::tempdir().unwrap();
    for args in [
        vec![
            "--root",
            "absent",
            "github-app",
            "serve",
            "--profile",
            "absent.json",
        ],
        vec![
            "--root",
            "absent",
            "github-app",
            "--allow-webhooks",
            "serve",
            "--profile",
            "absent.json",
        ],
    ] {
        refuses(root.path(), &args, "github_app_permission_required");
    }
}
#[test]
fn serve_zero_limits_precede_host_files_credentials_and_store() {
    let root = tempfile::tempdir().unwrap();
    for flag in [
        "--max-requests",
        "--max-connections",
        "--max-response-bytes",
        "--lifetime-ms",
        "--request-timeout-ms",
    ] {
        refuses(
            root.path(),
            &[
                "--root",
                "absent",
                "--allow-writes",
                "github-app",
                "--allow-webhooks",
                "serve",
                "--profile",
                "absent.json",
                flag,
                "0",
            ],
            "github_app_ingress_capacity_exceeded",
        );
    }
    refuses(
        root.path(),
        &[
            "--root",
            "absent",
            "--max-bytes",
            "0",
            "--allow-writes",
            "github-app",
            "--allow-webhooks",
            "serve",
            "--profile",
            "absent.json",
        ],
        "github_app_ingress_capacity_exceeded",
    );
}
#[test]
fn remote_plaintext_and_implicit_remote_authority_refuse_before_files() {
    let root = tempfile::tempdir().unwrap();
    for extra in [vec![], vec!["--allow-remote-webhooks"]] {
        let mut args = vec![
            "--root",
            "absent",
            "--allow-writes",
            "github-app",
            "--allow-webhooks",
            "serve",
            "--profile",
            "absent.json",
            "--listen",
            "0.0.0.0:0",
        ];
        args.extend(extra);
        refuses(
            root.path(),
            &args,
            "github_app_remote_tls_permission_required",
        );
    }
}
#[test]
fn serve_rejects_unrelated_provider_and_snapshot_authority() {
    let root = tempfile::tempdir().unwrap();
    refuses(
        root.path(),
        &[
            "--root",
            "absent",
            "--allow-writes",
            "github-app",
            "--allow-webhooks",
            "--allow-github",
            "serve",
            "--profile",
            "absent.json",
        ],
        "github_app_input_invalid",
    );
    refuses(
        root.path(),
        &[
            "--root",
            "absent",
            "--snapshot",
            "absent",
            "--allow-writes",
            "github-app",
            "--allow-webhooks",
            "serve",
            "--profile",
            "absent.json",
        ],
        "github_app_input_invalid",
    );
}
