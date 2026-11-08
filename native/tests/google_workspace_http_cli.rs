//! Owned source and private-pipe fixtures; never contacts Google or reads credentials.
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::Read,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::OnceLock,
    time::{Duration, Instant},
};
fn image_hash() -> String {
    static HASH: OnceLock<String> = OnceLock::new();
    HASH.get_or_init(|| {
        let mut file = fs::File::open(env!("CARGO_BIN_EXE_yoke")).unwrap();
        let mut digest = Sha256::new();
        let mut chunk = [0; 65536];
        loop {
            let n = file.read(&mut chunk).unwrap();
            if n == 0 {
                break;
            }
            digest.update(&chunk[..n]);
        }
        hex::encode(digest.finalize())
    })
    .clone()
}
fn command(root: &Path) -> Command {
    let mut c = Command::new(env!("CARGO_BIN_EXE_yoke"));
    c.current_dir(root);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        c.creation_flags(0x0800_0000);
    }
    c
}
fn fixture() -> (tempfile::TempDir, PathBuf, PathBuf, Vec<u8>) {
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("source");
    fs::create_dir(&source).unwrap();
    let bytes = br#"{"doc_id":"owned_123","resourceKey":"owned_private_resource_key"}"#.to_vec();
    fs::write(source.join("notes.gdoc"), &bytes).unwrap();
    let config = root.path().join("http.json");
    fs::write(&config,serde_json::to_vec(&json!({"shortcutPath":"notes.gdoc","expectedShortcutSha256":yoke::model::hash(&bytes),"expectedFileId":"owned_123","address":"142.250.0.1:443","expectedExporterSha256":image_hash()})).unwrap()).unwrap();
    (root, source, config, bytes)
}
fn flags<'a>(config: &'a Path, source: &'a Path, output: &'a Path) -> Vec<&'a str> {
    vec![
        "--timeout",
        "5000",
        "source-acquisition",
        "google-http-capture",
        "--config",
        config.to_str().unwrap(),
        "--source-root",
        source.to_str().unwrap(),
        "--output",
        output.to_str().unwrap(),
        "--allow-shortcut-read",
        "--allow-network",
        "--allow-private-credential",
        "--trust-native-exporter",
        "--access-token-stdin",
        "--allow-output",
    ]
}

#[test]
fn actual_native_plan_has_no_token_socket_store_output_or_private_key_material() {
    let (root, source, config, bytes) = fixture();
    let result = command(root.path())
        .args([
            "source-acquisition",
            "google-http-plan",
            "--config",
            config.to_str().unwrap(),
            "--source-root",
            source.to_str().unwrap(),
            "--allow-shortcut-read",
            "--trust-native-exporter",
        ])
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let value: Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(value["resourceKeyPresent"], true);
    assert_eq!(value["requests"], 0);
    assert_eq!(value["credentialsRead"], false);
    assert_eq!(value["outputCreated"], false);
    assert_eq!(value["method"], "GET");
    assert!(!String::from_utf8_lossy(&result.stdout).contains("owned_private_resource_key"));
    assert_eq!(fs::read(source.join("notes.gdoc")).unwrap(), bytes);
    assert!(!source.join(".yoke").exists());
}

#[test]
fn missing_grants_existing_outputs_and_invalid_specs_refuse_before_private_input() {
    let (root, source, config, bytes) = fixture();
    let output = root.path().join("export.json");
    let arguments = flags(&config, &source, &output);
    for omitted in [
        "--allow-shortcut-read",
        "--allow-network",
        "--allow-private-credential",
        "--trust-native-exporter",
        "--access-token-stdin",
        "--allow-output",
    ] {
        let args = arguments
            .iter()
            .filter(|a| **a != omitted)
            .copied()
            .collect::<Vec<_>>();
        let result = command(root.path()).args(args).output().unwrap();
        assert!(!result.status.success() && result.stdout.is_empty());
        assert!(
            String::from_utf8_lossy(&result.stderr)
                .contains("google_workspace_http_host_permission_required")
        );
    }
    fs::write(&output, b"foreign output").unwrap();
    let result = command(root.path()).args(&arguments).output().unwrap();
    assert!(!result.status.success() && result.stdout.is_empty());
    assert_eq!(fs::read(&output).unwrap(), b"foreign output");
    fs::remove_file(&output).unwrap();
    let mut spec: Value = serde_json::from_slice(&fs::read(&config).unwrap()).unwrap();
    spec["accessToken"] = json!("owned_private_token_canary");
    fs::write(&config, serde_json::to_vec(&spec).unwrap()).unwrap();
    let result = command(root.path()).args(&arguments).output().unwrap();
    assert!(!result.status.success() && result.stdout.is_empty());
    assert!(!String::from_utf8_lossy(&result.stderr).contains("owned_private_token_canary"));
    assert!(!output.exists());
    assert_eq!(fs::read(source.join("notes.gdoc")).unwrap(), bytes);
}

#[test]
fn ambient_tls_overrides_changed_executable_and_changed_shortcut_are_explicit_refusals() {
    let (root, source, config, bytes) = fixture();
    let args = [
        "source-acquisition",
        "google-http-plan",
        "--config",
        config.to_str().unwrap(),
        "--source-root",
        source.to_str().unwrap(),
        "--allow-shortcut-read",
        "--trust-native-exporter",
    ];
    for name in ["SSL_CERT_FILE", "SSL_CERT_DIR", "SSLKEYLOGFILE"] {
        let result = command(root.path())
            .args(args)
            .env(name, "owned-not-selected")
            .output()
            .unwrap();
        assert!(!result.status.success());
        assert!(
            String::from_utf8_lossy(&result.stderr)
                .contains("google_workspace_http_ambient_tls_settings_refused")
        );
    }
    let original = fs::read(&config).unwrap();
    let mut spec: Value = serde_json::from_slice(&original).unwrap();
    spec["expectedExporterSha256"] = json!("0".repeat(64));
    fs::write(&config, serde_json::to_vec(&spec).unwrap()).unwrap();
    let result = command(root.path()).args(args).output().unwrap();
    assert!(!result.status.success());
    assert!(
        String::from_utf8_lossy(&result.stderr)
            .contains("google_workspace_native_exporter_changed")
    );
    fs::write(&config, &original).unwrap();
    fs::write(source.join("notes.gdoc"), b"changed").unwrap();
    assert!(
        !command(root.path())
            .args(args)
            .output()
            .unwrap()
            .status
            .success()
    );
    fs::write(source.join("notes.gdoc"), bytes).unwrap();
    assert!(!source.join(".yoke").exists());
}

#[test]
fn an_open_empty_private_pipe_obeys_the_deadline_without_network_or_publication() {
    let (root, source, config, bytes) = fixture();
    let output = root.path().join("export.json");
    let start = Instant::now();
    let mut child = command(root.path())
        .args(flags(&config, &source, &output))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let held = child.stdin.take().unwrap();
    loop {
        if child.try_wait().unwrap().is_some() {
            break;
        }
        if start.elapsed() > Duration::from_secs(9) {
            let _ = child.kill();
            let _ = child.wait();
            panic!("Owned private-input deadline did not stop child");
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    drop(held);
    let result = child.wait_with_output().unwrap();
    assert!(!result.status.success() && result.stdout.is_empty());
    assert!(
        String::from_utf8_lossy(&result.stderr).contains("deadline"),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert!(start.elapsed() < Duration::from_secs(9));
    assert!(!output.exists());
    assert_eq!(fs::read(source.join("notes.gdoc")).unwrap(), bytes);
}
