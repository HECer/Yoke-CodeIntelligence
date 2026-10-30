//! Real CLI admission must precede host inputs, credentials and Store discovery.
use std::{path::Path, process::Command};

fn refused(root: &Path, args: &[&str], marker: &str, documents: usize) {
    let output = Command::new(env!("CARGO_BIN_EXE_yoke"))
        .args(["--root", "absent-source"])
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
    assert!(!root.join("absent-source").exists());
    assert_eq!(std::fs::read_dir(root).unwrap().count(), documents);
}

#[test]
fn preview_requires_source_read_consent_before_inputs() {
    let root = tempfile::tempdir().unwrap();
    refused(
        root.path(),
        &["github-app", "pages", "preview"],
        "github_app_permission_required",
        0,
    );
}

#[test]
fn preview_refuses_network_writes_or_page_mutation_consent_before_inputs() {
    let root = tempfile::tempdir().unwrap();
    for args in [
        vec![
            "--allow-writes",
            "github-app",
            "--allow-webhooks",
            "pages",
            "preview",
        ],
        vec![
            "github-app",
            "--allow-webhooks",
            "--allow-github",
            "pages",
            "preview",
        ],
        vec![
            "github-app",
            "--allow-webhooks",
            "pages",
            "--allow-pages",
            "preview",
        ],
        vec![
            "github-app",
            "--allow-webhooks",
            "pages",
            "--allow-page-serving",
            "preview",
        ],
    ] {
        refused(root.path(), &args, "github_app_pages_input_invalid", 0);
    }
}

#[test]
fn store_and_remove_require_distinct_page_and_store_write_consent() {
    let root = tempfile::tempdir().unwrap();
    for operation in ["store", "remove"] {
        refused(
            root.path(),
            &[
                "--allow-writes",
                "github-app",
                "--allow-webhooks",
                "pages",
                operation,
            ],
            "github_app_pages_permission_required",
            0,
        );
        refused(
            root.path(),
            &[
                "github-app",
                "--allow-webhooks",
                "pages",
                "--allow-pages",
                operation,
            ],
            "github_app_permission_required",
            0,
        );
    }
}

#[test]
fn readonly_service_requires_serving_and_page_consent_without_queue_write_consent() {
    let root = tempfile::tempdir().unwrap();
    refused(
        root.path(),
        &["github-app", "pages", "--allow-pages", "serve"],
        "github_app_pages_serving_permission_required",
        0,
    );
    refused(
        root.path(),
        &["github-app", "pages", "--allow-page-serving", "serve"],
        "github_app_pages_permission_required",
        0,
    );
    refused(
        root.path(),
        &[
            "github-app",
            "pages",
            "--allow-pages",
            "--allow-page-serving",
            "serve",
        ],
        "github_app_pages_input_invalid",
        0,
    );
    refused(
        root.path(),
        &[
            "--allow-writes",
            "github-app",
            "pages",
            "--allow-pages",
            "--allow-page-serving",
            "serve",
        ],
        "github_app_pages_input_invalid",
        0,
    );
    refused(
        root.path(),
        &[
            "github-app",
            "--allow-webhooks",
            "pages",
            "--allow-pages",
            "--allow-page-serving",
            "serve",
        ],
        "github_app_permission_required",
        0,
    );
}

#[test]
fn zero_page_and_source_limits_are_admitted_before_profiles_and_credentials() {
    let root = tempfile::tempdir().unwrap();
    for flag in [
        "--ttl-seconds",
        "--max-pages",
        "--max-page-store-bytes",
        "--max-html-bytes",
        "--max-nodes",
        "--max-observations",
        "--max-records",
        "--max-worker-store-bytes",
    ] {
        refused(
            root.path(),
            &[
                "github-app",
                "--allow-webhooks",
                "pages",
                flag,
                "0",
                "preview",
            ],
            "github_app_pages_capacity_exceeded",
            0,
        );
    }
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
                "github-app",
                "--allow-webhooks",
                flag,
                "0",
                "pages",
                "preview",
            ],
            "github_app_pages_capacity_exceeded",
            0,
        );
    }
    refused(
        root.path(),
        &[
            "--max-bytes",
            "0",
            "github-app",
            "--allow-webhooks",
            "pages",
            "preview",
        ],
        "github_app_pages_capacity_exceeded",
        0,
    );
}

#[test]
fn serving_refuses_plain_remote_and_zero_wire_limit_before_host_inputs() {
    let root = tempfile::tempdir().unwrap();
    refused(
        root.path(),
        &[
            "github-app",
            "pages",
            "--allow-pages",
            "--allow-page-serving",
            "serve",
            "--listen",
            "0.0.0.0:0",
            "--allow-remote-pages",
        ],
        "github_app_remote_tls_permission_required",
        0,
    );
    refused(
        root.path(),
        &[
            "github-app",
            "pages",
            "--allow-pages",
            "--allow-page-serving",
            "serve",
            "--max-response-bytes",
            "0",
        ],
        "github_app_ingress_capacity_exceeded",
        0,
    );
}

#[test]
fn strict_page_requests_are_parsed_before_os_credentials_or_store_opening() {
    let root = tempfile::tempdir().unwrap();
    let profile = root.path().join("profile.json");
    let request = root.path().join("request.json");
    std::fs::write(&profile, br#"{"schemaVersion":1,"appId":123,"secretCredential":"absent-owned-pages-key","grants":[{"installationId":42,"repositoryId":9,"repository":"acme/demo"}]}"#).unwrap();
    let profile = profile.to_str().unwrap();
    for operation in ["preview", "store", "remove"] {
        std::fs::write(
            &request,
            br#"{"schemaVersion":1,"schemaVersion":1,"credential":"incoming-not-authority"}"#,
        )
        .unwrap();
        let mut args = vec![
            "github-app",
            "--allow-webhooks",
            "--profile",
            profile,
            "pages",
        ];
        if operation != "preview" {
            args.insert(0, "--allow-writes");
            args.push("--allow-pages");
        }
        args.extend([operation, "--request", request.to_str().unwrap()]);
        refused(root.path(), &args, "github_app_pages_input_invalid", 2);
    }
}
