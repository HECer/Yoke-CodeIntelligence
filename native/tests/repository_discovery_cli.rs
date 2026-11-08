use serde_json::Value;
use std::{fs, process::Command};

#[test]
fn discovery_cli_selects_only_reported_candidates_without_creating_state() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("source π '");
    fs::create_dir_all(path.join("child/.git")).unwrap();
    let call = |extra: &[&str]| {
        Command::new(env!("CARGO_BIN_EXE_yoke"))
            .arg("--root")
            .arg(&path)
            .args(["repositories", "discover"])
            .args(extra)
            .output()
            .unwrap()
    };
    let output = call(&[]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["candidates"][0]["path"], "child");
    assert_eq!(report["complete"], false);
    assert_eq!(report["registrationGranted"], false);
    let selected = call(&["--select", "child"]);
    assert!(selected.status.success());
    let selected: Value = serde_json::from_slice(&selected.stdout).unwrap();
    assert_eq!(selected["candidate"], report["candidates"][0]);
    assert_eq!(selected["sourceAccessGranted"], false);
    for args in [
        vec!["--select", "missing"],
        vec!["--select", "../outside"],
        vec!["--max-repositories", "0"],
        vec!["--max-bytes", "1"],
        vec!["--timeout", "0"],
        vec!["--max-depth", "33"],
        vec!["--max-depth", "1", "--max-depth", "2"],
    ] {
        let output = call(&args);
        assert!(!output.status.success(), "{args:?}");
        assert!(output.stdout.is_empty());
    }
    assert!(!path.join(".yoke").exists());
    assert!(!path.join("child/.yoke").exists());
}

#[test]
fn discovery_rejects_source_provider_and_write_flags_before_opening_store() {
    let root = tempfile::tempdir().unwrap();
    for flags in [
        vec!["--allow-writes"],
        vec!["--trust-servers"],
        vec!["--snapshot", "missing"],
        vec!["--capture-config", "missing.json"],
        vec!["--language-server-config", "missing.json"],
        vec!["--include-ignored-dir", "secret"],
        vec!["--analyzer-epoch", "9"],
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_yoke"))
            .arg("--root")
            .arg(root.path())
            .args(&flags)
            .args(["repositories", "discover"])
            .output()
            .unwrap();
        assert!(!output.status.success(), "{flags:?}");
        assert!(output.stdout.is_empty());
        assert!(!root.path().join(".yoke").exists());
    }
    let output = Command::new(env!("CARGO_BIN_EXE_yoke"))
        .args(["repositories", "discover"])
        .current_dir(root.path())
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(!root.path().join(".yoke").exists());
}
