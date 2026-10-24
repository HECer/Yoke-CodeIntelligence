//! Permission and closed-input ordering; never resolves a credential or creates state.
use std::{path::Path, process::Command};
fn call(root: &Path, flags: &[&str], operation: &[&str], writes: bool) -> std::process::Output {
    let mut c = Command::new(env!("CARGO_BIN_EXE_yoke"));
    c.arg("--root").arg(root);
    if writes {
        c.arg("--allow-writes");
    }
    c.args(["app-history-job", "--profile"])
        .arg(root.join("profile.json"))
        .arg("--selection")
        .arg(root.join("selection.json"))
        .args(flags)
        .args(operation)
        .output()
        .unwrap()
}
fn refused(output: std::process::Output, marker: &str) {
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    let error = String::from_utf8_lossy(&output.stderr);
    assert!(error.contains(marker), "{error}");
}
#[test]
fn every_operation_requires_explicit_app_and_job_permission_before_host_files() {
    let root = tempfile::tempdir().unwrap();
    for operation in [
        vec!["create"],
        vec!["status"],
        vec!["step", "--revision", "0"],
        vec!["run", "--revision", "0"],
        vec!["pause", "--revision", "0"],
        vec!["resume", "--revision", "0"],
        vec!["cancel", "--revision", "0"],
    ] {
        for flags in [vec![], vec!["--allow-app-history"], vec!["--allow-jobs"]] {
            refused(
                call(root.path(), &flags, &operation, true),
                "permission_required",
            );
        }
    }
    refused(
        call(
            root.path(),
            &["--allow-app-history", "--allow-jobs", "--allow-capture"],
            &["step", "--revision", "0"],
            true,
        ),
        "permission_required",
    );
    assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 0);
}
#[test]
fn budgets_and_read_only_scope_refuse_before_profiles_keys_or_store() {
    let root = tempfile::tempdir().unwrap();
    let flags = ["--allow-app-history", "--allow-jobs", "--allow-capture"];
    for (flag, value) in [
        ("--max-batch-visits", "0"),
        ("--max-batch-objects", "65"),
        ("--max-batch-proof-bytes", "2000001"),
    ] {
        let mut f = flags.to_vec();
        f.extend([flag, value]);
        refused(call(root.path(), &f, &["create"], true), "input_invalid");
    }
    refused(
        call(
            root.path(),
            &["--allow-app-history", "--allow-jobs"],
            &["status"],
            true,
        ),
        "input_invalid",
    );
    refused(
        call(root.path(), &flags, &["status"], false),
        "input_invalid",
    );
    let mut f = flags.to_vec();
    f.push("--trust-git");
    refused(
        call(
            root.path(),
            &f,
            &["run", "--revision", "0", "--max-batches", "129"],
            true,
        ),
        "input_invalid",
    );
    assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 0);
}
#[test]
fn closed_selection_and_plan_errors_precede_credential_resolution_and_missing_source() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("profile.json"),br#"{"schemaVersion":1,"appId":123,"secretCredential":"never-resolved-owned-canary","grants":[{"installationId":42,"repositoryId":9,"repository":"acme/demo"}]}"#).unwrap();
    for raw in [
        format!(
            "{{\"schemaVersion\":1,\"completionId\":\"{}\",\"allowWrites\":true}}",
            "a".repeat(64)
        ),
        format!(
            "{{\"schemaVersion\":1,\"completionId\":\"{}\"}}",
            "a".repeat(64)
        ),
    ] {
        std::fs::write(root.path().join("selection.json"), raw).unwrap();
        refused(
            call(
                root.path(),
                &["--allow-app-history", "--allow-jobs", "--allow-capture"],
                &["create"],
                true,
            ),
            "input_invalid",
        );
        assert!(!root.path().join(".yoke").exists());
    }
    std::fs::write(
        root.path().join("selection.json"),
        format!(
            "{{\"schemaVersion\":1,\"completionId\":\"{}\",\"expectedQueueRevision\":2}}",
            "a".repeat(64)
        ),
    )
    .unwrap();
    std::fs::write(
        root.path().join("plan.json"),
        br#"{"privateFixture":"must not reflect"}"#,
    )
    .unwrap();
    let output = call(
        root.path(),
        &["--allow-app-history", "--allow-jobs", "--allow-capture"],
        &[
            "create",
            "--plan",
            root.path().join("plan.json").to_str().unwrap(),
        ],
        true,
    );
    assert!(!String::from_utf8_lossy(&output.stderr).contains("must not reflect"));
    refused(output, "input_invalid");
    assert!(!root.path().join(".yoke").exists());
}
