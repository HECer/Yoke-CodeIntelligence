//! Owned fixtures only: no Google account, document, network or analyzed program.
use serde_json::{Value, json};
use std::{
    fs,
    path::Path,
    process::{Command, Output},
};
use yoke::{
    capture::Deadline,
    google_workspace::{ExporterClaim, Limits, ReceivedExport, Shortcut},
};

fn invoke(root: &Path, args: &[&str]) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_yoke"));
    command.args(args).current_dir(root);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000);
    }
    command.output().unwrap()
}
fn received(root: &Path) -> std::path::PathBuf {
    let deadline = Deadline::new(30000);
    let pointer = Shortcut::parse("notes.gdoc", br#"{"doc_id":"owned_123"}"#, &deadline).unwrap();
    let source = ReceivedExport::from_received_data(
        pointer.metadata().clone(),
        ExporterClaim {
            executable_sha256: "a".repeat(64),
            version: "0.22.5".into(),
        },
        "A💡B\r\n".as_bytes(),
        &Limits::default(),
        &deadline,
    )
    .unwrap();
    let input = root.join("received.json");
    fs::write(&input, source.archive(&deadline).unwrap()).unwrap();
    input
}
#[test]
fn replay_retains_exact_unicode_bytes_without_initializing_state_or_a_client() {
    let root = tempfile::tempdir().unwrap();
    let input = received(root.path());
    let before = fs::read(&input).unwrap();
    for (start, end, utf8) in [(1, 5, json!("💡")), (2, 4, Value::Null), (0, 0, json!(""))] {
        let output = invoke(
            root.path(),
            &[
                "source-acquisition",
                "google-read",
                "--input",
                input.to_str().unwrap(),
                "--start-byte",
                &start.to_string(),
                "--end-byte",
                &end.to_string(),
            ],
        );
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let value: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(value["utf8"], utf8);
        assert_eq!(value["startByte"], start);
        assert_eq!(value["endByte"], end);
        assert_eq!(value["sourceSpace"], "received_export_body_bytes");
        assert_eq!(value["executionAuthority"], "none");
    }
    let output = invoke(
        root.path(),
        &[
            "source-acquisition",
            "google-read",
            "--input",
            input.to_str().unwrap(),
        ],
    );
    assert!(output.status.success());
    let value: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["googleIdentityVerified"], false);
    assert_eq!(value["exporterClaimVerified"], false);
    assert_eq!(value["modelCalls"], 0);
    assert_eq!(fs::read(input).unwrap(), before);
    assert!(!root.path().join(".yoke").exists());
}
#[test]
fn replay_rejects_source_flags_invalid_ranges_and_forged_archives() {
    let root = tempfile::tempdir().unwrap();
    let input = received(root.path());
    for extras in [
        vec!["--allow-network"],
        vec!["--office-text"],
        vec!["--start-byte", "0"],
        vec!["--start-byte", "2", "--end-byte", "1"],
        vec!["--start-byte", "0", "--end-byte", "99999"],
        vec!["--office-text", "--start-byte", "0", "--end-byte", "1"],
    ] {
        let mut args = vec![
            "source-acquisition",
            "google-read",
            "--input",
            input.to_str().unwrap(),
        ];
        args.extend(extras);
        assert!(!invoke(root.path(), &args).status.success());
    }
    let mut value: Value = serde_json::from_slice(&fs::read(&input).unwrap()).unwrap();
    value["allowNetwork"] = json!(true);
    fs::write(&input, serde_json::to_vec(&value).unwrap()).unwrap();
    assert!(
        !invoke(
            root.path(),
            &[
                "source-acquisition",
                "google-read",
                "--input",
                input.to_str().unwrap()
            ]
        )
        .status
        .success()
    );
    assert!(!root.path().join(".yoke").exists());
}
#[test]
fn capture_permissions_and_fresh_destination_precede_any_client_effect() {
    let root = tempfile::tempdir().unwrap();
    let config = root.path().join("missing.json");
    let output = root.path().join("output.json");
    let missing = invoke(
        root.path(),
        &[
            "source-acquisition",
            "google-capture",
            "--config",
            config.to_str().unwrap(),
            "--source-root",
            root.path().to_str().unwrap(),
            "--output",
            output.to_str().unwrap(),
        ],
    );
    assert!(!missing.status.success());
    assert!(
        String::from_utf8_lossy(&missing.stderr)
            .contains("google_workspace_host_permission_required")
    );
    let pointer = br#"{"doc_id":"owned_123","allowNetwork":true}"#;
    fs::write(root.path().join("notes.gdoc"), pointer).unwrap();
    let absent = root.path().join("missing.exe");
    fs::write(
        &config,
        serde_json::to_vec(&json!({"shortcutPath":"notes.gdoc",
        "expectedShortcutSha256":yoke::model::hash(pointer),"expectedFileId":"owned_123",
        "client":{"executable":absent,"executableSha256":"a".repeat(64),"version":"0.22.5",
        "configurationDirectory":root.path()}}))
        .unwrap(),
    )
    .unwrap();
    fs::write(&output, b"foreign bytes").unwrap();
    let args = [
        "source-acquisition",
        "google-capture",
        "--config",
        config.to_str().unwrap(),
        "--source-root",
        root.path().to_str().unwrap(),
        "--output",
        output.to_str().unwrap(),
        "--allow-shortcut-read",
        "--allow-network",
        "--allow-exporter-execution",
        "--allow-client-state",
        "--allow-output",
    ];
    let refused = invoke(root.path(), &args);
    assert!(!refused.status.success());
    assert!(String::from_utf8_lossy(&refused.stderr).contains("fresh file"));
    assert_eq!(fs::read(&output).unwrap(), b"foreign bytes");
    fs::remove_file(&output).unwrap();
    let mut value: Value = serde_json::from_slice(&fs::read(&config).unwrap()).unwrap();
    value["expectedShortcutSha256"] = json!("f".repeat(64));
    fs::write(&config, serde_json::to_vec(&value).unwrap()).unwrap();
    let stale = invoke(root.path(), &args);
    assert!(!stale.status.success());
    assert!(
        String::from_utf8_lossy(&stale.stderr)
            .contains("google_workspace_shortcut_precondition_changed")
    );
    assert!(!output.exists());
    assert!(!root.path().join(".yoke").exists());
}
