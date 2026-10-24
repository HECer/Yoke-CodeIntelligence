//! Real CLI permission ordering without source, input or credential discovery.
use std::{fs, process::Command};
#[test]
fn webhook_and_write_permissions_precede_missing_profile_credentials_and_store() {
    let folder = tempfile::tempdir().unwrap();
    let root = folder.path().join("absent-root");
    for flags in [vec![], vec!["--allow-webhooks"], vec!["--allow-writes"]] {
        let mut command = Command::new(env!("CARGO_BIN_EXE_yoke"));
        command
            .args(["--root"])
            .arg(&root)
            .args(["github-app", "ingest", "--profile"])
            .arg(folder.path().join("absent-profile.json"))
            .args(["--delivery"])
            .arg(folder.path().join("absent-headers.json"))
            .args(["--payload"])
            .arg(folder.path().join("absent-body.json"))
            .args(flags);
        let result = command.output().unwrap();
        assert!(!result.status.success());
        assert!(result.stdout.is_empty());
        assert!(String::from_utf8_lossy(&result.stderr).contains("github_app_permission_required"));
        assert!(!root.exists());
    }
}
#[test]
fn secret_import_rejects_terminal_control_bytes_without_echoing_or_creating_source() {
    use std::io::Write;
    use std::process::Stdio;
    let folder = tempfile::tempdir().unwrap();
    let root = folder.path().join("not-a-source");
    let mut child = Command::new(env!("CARGO_BIN_EXE_yoke"))
        .arg("--root")
        .arg(&root)
        .args([
            "credentials",
            "github-webhook-import",
            "invalid-secret-fixture",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(b"sensitive-invalid-test-value\r\n")
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    let error = String::from_utf8_lossy(&output.stderr);
    assert!(!error.contains("sensitive"));
    assert!(error.contains("github_app_credential_invalid"));
    assert!(!root.exists());
    assert_eq!(fs::read_dir(folder.path()).unwrap().count(), 0);
}
