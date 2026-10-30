//! Real command admission checks without credentials, network or source discovery.
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
fn preview_permission_precedes_host_files_and_source_store() {
    let root = tempfile::tempdir().unwrap();
    refused(
        root.path(),
        &[
            "--root",
            "absent-source",
            "github-app",
            "publication",
            "preview",
        ],
        "github_app_permission_required",
    );
}
#[test]
fn preview_refuses_network_and_write_authority_before_host_files() {
    let root = tempfile::tempdir().unwrap();
    for args in [
        vec![
            "github-app",
            "--allow-webhooks",
            "--allow-github",
            "publication",
            "preview",
        ],
        vec![
            "--allow-writes",
            "github-app",
            "--allow-webhooks",
            "publication",
            "preview",
        ],
    ] {
        refused(root.path(), &args, "github_app_publication_input_invalid");
    }
}
#[test]
fn preview_zero_limits_precede_profiles_credentials_and_state() {
    let root = tempfile::tempdir().unwrap();
    for flag in [
        "--max-body-bytes",
        "--max-records",
        "--max-worker-store-bytes",
    ] {
        refused(
            root.path(),
            &[
                "github-app",
                "--allow-webhooks",
                "publication",
                flag,
                "0",
                "preview",
            ],
            "github_app_publication_capacity_exceeded",
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
                "publication",
                "preview",
            ],
            "github_app_publication_capacity_exceeded",
        );
    }
    refused(
        root.path(),
        &[
            "--max-bytes",
            "0",
            "github-app",
            "--allow-webhooks",
            "publication",
            "preview",
        ],
        "github_app_publication_capacity_exceeded",
    );
}
#[test]
fn preview_missing_host_input_never_captures_a_repository() {
    let root = tempfile::tempdir().unwrap();
    refused(
        root.path(),
        &[
            "--root",
            "absent-source",
            "github-app",
            "--allow-webhooks",
            "publication",
            "preview",
        ],
        "github_app_publication_input_invalid",
    );
}

#[test]
fn preview_parses_request_before_credentials_and_source_discovery() {
    let root = tempfile::tempdir().unwrap();
    let profile = root.path().join("host-profile.json");
    let request = root.path().join("request.json");
    std::fs::write(
        &profile,
        serde_json::to_vec(&serde_json::json!({
        "schemaVersion":1,"appId":123,"secretCredential":"absent-owned-preview-fixture",
        "grants":[{"installationId":42,"repositoryId":9,"repository":"acme/demo"}]}))
        .unwrap(),
    )
    .unwrap();
    std::fs::write(&request, format!("{{\"schemaVersion\":1,\"completionId\":\"{}\",\"expectedQueueRevision\":2,\"expectedQueueRevision\":2}}","a".repeat(64))).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_yoke"))
        .args([
            "--root",
            "absent-source",
            "github-app",
            "--allow-webhooks",
            "--profile",
            profile.to_str().unwrap(),
            "publication",
            "preview",
            "--request",
            request.to_str().unwrap(),
        ])
        .current_dir(root.path())
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("github_app_publication_input_invalid"),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!root.path().join("absent-source").exists());
    assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 2);
}

#[test]
fn publish_requires_distinct_publication_write_and_network_consent_before_inputs() {
    let root = tempfile::tempdir().unwrap();
    refused(
        root.path(),
        &[
            "--allow-writes",
            "github-app",
            "--allow-webhooks",
            "--allow-github",
            "publication",
            "publish",
        ],
        "github_app_publication_permission_required",
    );
    refused(
        root.path(),
        &[
            "github-app",
            "--allow-webhooks",
            "--allow-github",
            "publication",
            "--allow-publication",
            "publish",
        ],
        "github_app_permission_required",
    );
    refused(
        root.path(),
        &[
            "--allow-writes",
            "github-app",
            "--allow-webhooks",
            "publication",
            "--allow-publication",
            "publish",
        ],
        "github_app_network_permission_required",
    );
}
#[test]
fn recover_requires_journal_and_network_authority_but_no_post_patch_consent() {
    let root = tempfile::tempdir().unwrap();
    refused(
        root.path(),
        &[
            "github-app",
            "--allow-webhooks",
            "--allow-github",
            "publication",
            "recover",
        ],
        "github_app_permission_required",
    );
    refused(
        root.path(),
        &[
            "--allow-writes",
            "github-app",
            "--allow-webhooks",
            "publication",
            "recover",
        ],
        "github_app_network_permission_required",
    );
}
#[test]
fn status_remains_read_only_and_refuses_unneeded_authority() {
    let root = tempfile::tempdir().unwrap();
    for args in [
        vec![
            "--allow-writes",
            "github-app",
            "--allow-webhooks",
            "publication",
            "status",
        ],
        vec![
            "github-app",
            "--allow-webhooks",
            "--allow-github",
            "publication",
            "status",
        ],
        vec![
            "github-app",
            "--allow-webhooks",
            "publication",
            "--allow-publication",
            "status",
        ],
    ] {
        refused(root.path(), &args, "github_app_publication_input_invalid");
    }
}
#[test]
fn journal_and_remote_zero_caps_precede_all_host_documents() {
    let root = tempfile::tempdir().unwrap();
    for flag in ["--max-journal-records", "--max-journal-bytes"] {
        refused(
            root.path(),
            &[
                "github-app",
                "--allow-webhooks",
                "publication",
                flag,
                "0",
                "preview",
            ],
            "github_app_publication_capacity_exceeded",
        );
    }
    for flag in [
        "--max-requests",
        "--max-response-bytes",
        "--max-total-bytes",
    ] {
        refused(
            root.path(),
            &[
                "--allow-writes",
                "github-app",
                "--allow-webhooks",
                "--allow-github",
                "publication",
                "--allow-publication",
                "publish",
                flag,
                "0",
            ],
            "github_app_publication_capacity_exceeded",
        );
    }
}
#[test]
fn new_operations_parse_request_before_either_credential_or_source_store() {
    let root = tempfile::tempdir().unwrap();
    let profile = root.path().join("profile.json");
    let request = root.path().join("request.json");
    std::fs::write(&profile,serde_json::to_vec(&serde_json::json!({"schemaVersion":1,"appId":123,"secretCredential":"absent-owned-publication-fixture",
        "grants":[{"installationId":42,"repositoryId":9,"repository":"acme/demo"}]})).unwrap()).unwrap();
    for operation in ["publish", "recover", "status"] {
        std::fs::write(&request, b"{\"schemaVersion\":1,\"schemaVersion\":1}").unwrap();
        let mut args = vec!["--root", "absent-source"];
        if operation != "status" {
            args.push("--allow-writes");
        }
        args.extend([
            "github-app",
            "--allow-webhooks",
            "--profile",
            profile.to_str().unwrap(),
        ]);
        if operation != "status" {
            args.push("--allow-github");
        }
        args.push("publication");
        if operation == "publish" {
            args.push("--allow-publication");
        }
        args.extend([operation, "--request", request.to_str().unwrap()]);
        if operation != "status" {
            args.extend(["--auth-profile", "absent-auth.json"]);
        }
        let output = Command::new(env!("CARGO_BIN_EXE_yoke"))
            .args(args)
            .current_dir(root.path())
            .output()
            .unwrap();
        assert!(!output.status.success());
        assert!(output.stdout.is_empty());
        assert!(
            String::from_utf8_lossy(&output.stderr)
                .contains("github_app_publication_input_invalid"),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(!root.path().join("absent-source").exists());
        assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 2);
    }
}

#[test]
fn comment_profiles_match_app_and_least_role_before_either_credential() {
    use serde_json::json;
    let root = tempfile::tempdir().unwrap();
    let profile = root.path().join("profile.json");
    let request = root.path().join("request.json");
    let auth = root.path().join("auth.json");
    std::fs::write(&profile, serde_json::to_vec(&json!({
        "schemaVersion":1,"appId":123,"secretCredential":"absent-owned-publication-cli-admission",
        "grants":[{"installationId":42,"repositoryId":9,"repository":"acme/demo"}]
    })).unwrap()).unwrap();
    for (operation, app_id, permissions, marker) in [
        (
            "publish",
            123,
            json!({"pull_requests":"write"}),
            "github_app_credential_unavailable",
        ),
        (
            "recover",
            123,
            json!({"pull_requests":"read"}),
            "github_app_credential_unavailable",
        ),
        (
            "publish",
            123,
            json!({"pull_requests":"read"}),
            "github_app_publication_input_invalid",
        ),
        (
            "recover",
            124,
            json!({"pull_requests":"read"}),
            "github_app_publication_input_invalid",
        ),
        (
            "publish",
            123,
            json!({"pull_requests":"write","issues":"read"}),
            "github_app_publication_input_invalid",
        ),
        (
            "recover",
            123,
            json!({"contents":"read"}),
            "github_app_publication_input_invalid",
        ),
    ] {
        let body = if operation == "publish" {
            json!({"schemaVersion":1,"completionId":"a".repeat(64),"expectedQueueRevision":2,
                "proposalId":"b".repeat(64),"bodyHash":"c".repeat(64),
                "requestId":"11111111-2222-4333-8444-555555555555","approval":"publish_exact_comment"})
        } else {
            json!({"schemaVersion":1,"intentId":"a".repeat(64),"commentId":501,"installationId":42})
        };
        std::fs::write(&request, serde_json::to_vec(&body).unwrap()).unwrap();
        std::fs::write(
            &auth,
            serde_json::to_vec(&json!({
                "schemaVersion":1,"appId":app_id,"keyCredential":"absent-owned-publication-cli-key",
                "grants":[{"installationId":42,"repositoryId":9,"repository":"acme/demo"}],
                "permissions":permissions
            }))
            .unwrap(),
        )
        .unwrap();
        let mut args = vec![
            "--root",
            "absent-source",
            "--allow-writes",
            "github-app",
            "--allow-webhooks",
            "--allow-github",
            "--profile",
            profile.to_str().unwrap(),
            "publication",
        ];
        if operation == "publish" {
            args.push("--allow-publication");
        }
        args.extend([
            operation,
            "--request",
            request.to_str().unwrap(),
            "--auth-profile",
            auth.to_str().unwrap(),
        ]);
        let output = Command::new(env!("CARGO_BIN_EXE_yoke"))
            .args(args)
            .current_dir(root.path())
            .output()
            .unwrap();
        assert!(!output.status.success());
        assert!(output.stdout.is_empty());
        assert!(
            String::from_utf8_lossy(&output.stderr).contains(marker),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(!root.path().join("absent-source").exists());
        assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 3);
    }
}

#[test]
fn explicit_page_link_preview_flag_reaches_bounded_input_admission() {
    let root = tempfile::tempdir().unwrap();
    refused(
        root.path(),
        &[
            "github-app",
            "--allow-webhooks",
            "publication",
            "--allow-page-links",
            "preview",
        ],
        "github_app_publication_input_invalid",
    );
}

#[test]
fn historical_read_and_recovery_refuse_unused_page_link_authority_before_credentials() {
    use serde_json::json;
    let root = tempfile::tempdir().unwrap();
    let profile = root.path().join("profile.json");
    let request = root.path().join("request.json");
    let auth = root.path().join("auth.json");
    std::fs::write(&profile,serde_json::to_vec(&json!({
        "schemaVersion":1,"appId":123,"secretCredential":"absent-owned-historical-page-cli-fixture",
        "grants":[{"installationId":42,"repositoryId":9,"repository":"acme/demo"}]
    })).unwrap()).unwrap();
    std::fs::write(
        &auth,
        serde_json::to_vec(&json!({
            "schemaVersion":1,"appId":123,"keyCredential":"absent-owned-historical-page-cli-key",
            "grants":[{"installationId":42,"repositoryId":9,"repository":"acme/demo"}],
            "permissions":{"pull_requests":"read"}
        }))
        .unwrap(),
    )
    .unwrap();
    for operation in ["status", "recover"] {
        let body = if operation == "status" {
            json!({"schemaVersion":1,"requestId":"11111111-2222-4333-8444-555555555555"})
        } else {
            json!({"schemaVersion":1,"intentId":"a".repeat(64),"commentId":501,"installationId":42})
        };
        std::fs::write(&request, serde_json::to_vec(&body).unwrap()).unwrap();
        for allow in [false, true] {
            let mut args = vec!["--root", "absent-source"];
            if operation == "recover" {
                args.push("--allow-writes");
            }
            args.extend([
                "github-app",
                "--allow-webhooks",
                "--profile",
                profile.to_str().unwrap(),
            ]);
            if operation == "recover" {
                args.push("--allow-github");
            }
            args.push("publication");
            if allow {
                args.push("--allow-page-links");
            }
            args.extend([operation, "--request", request.to_str().unwrap()]);
            if operation == "recover" {
                args.extend(["--auth-profile", auth.to_str().unwrap()]);
            }
            let output = Command::new(env!("CARGO_BIN_EXE_yoke"))
                .args(args)
                .current_dir(root.path())
                .output()
                .unwrap();
            assert!(!output.status.success() && output.stdout.is_empty());
            let marker = if allow {
                "github_app_publication_input_invalid"
            } else {
                "github_app_credential_unavailable"
            };
            assert!(
                String::from_utf8_lossy(&output.stderr).contains(marker),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            assert!(!root.path().join("absent-source").exists());
            assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 3);
        }
    }
}

#[test]
fn linked_preview_checks_distinct_consent_before_credentials_or_source_store() {
    use serde_json::json;
    let root = tempfile::tempdir().unwrap();
    let profile = root.path().join("profile.json");
    let request = root.path().join("request.json");
    std::fs::write(
        &profile,
        serde_json::to_vec(&json!({
            "schemaVersion":1,"appId":123,"secretCredential":"absent-owned-linked-cli-fixture",
            "grants":[{"installationId":42,"repositoryId":9,"repository":"acme/demo"}]
        }))
        .unwrap(),
    )
    .unwrap();
    std::fs::write(
        &request,
        serde_json::to_vec(&json!({
            "schemaVersion":1,"completionId":"a".repeat(64),"expectedQueueRevision":2,
            "page":{"pageId":"b".repeat(64),"expectedPageVersion":1}
        }))
        .unwrap(),
    )
    .unwrap();
    for allow in [false, true] {
        let mut args = vec![
            "--root",
            "absent-source",
            "github-app",
            "--allow-webhooks",
            "--profile",
            profile.to_str().unwrap(),
            "publication",
        ];
        if allow {
            args.push("--allow-page-links");
        }
        args.extend(["preview", "--request", request.to_str().unwrap()]);
        let output = Command::new(env!("CARGO_BIN_EXE_yoke"))
            .args(args)
            .current_dir(root.path())
            .output()
            .unwrap();
        assert!(!output.status.success() && output.stdout.is_empty());
        let marker = if allow {
            "github_app_credential_unavailable"
        } else {
            "github_app_page_links_permission_required"
        };
        assert!(
            String::from_utf8_lossy(&output.stderr).contains(marker),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(!root.path().join("absent-source").exists());
        assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 2);
    }
}
