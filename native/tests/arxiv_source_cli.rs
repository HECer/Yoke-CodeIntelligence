use serde_json::{Value, json};
use std::{
    fs,
    path::Path,
    process::{Command, Output},
};
use yoke::{capture::Deadline, source_acquisition::ReceivedSource};
fn invoke(root: &Path, args: &[&str]) -> Output {
    let mut c = Command::new(env!("CARGO_BIN_EXE_yoke"));
    c.args(args).current_dir(root);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        c.creation_flags(0x0800_0000);
    }
    c.output().unwrap()
}
#[test]
fn offline_arxiv_metadata_replay_has_no_store_and_conflicting_read_modes_refuse() {
    let root = tempfile::tempdir().unwrap();
    let input = root.path().join("owned-source.json");
    let source=ReceivedSource::from_response(&json!({"schemaVersion":1,"format":"html","requestedUrl":"https://arxiv.org/abs/2301.00001","httpStatus":200,"contentType":"text/html","etag":null,"lastModified":null}),b"<head><meta name=citation_title content=owned><meta name=citation_arxiv_id content=2301.00001></head>",&Deadline::new(30000)).unwrap();
    let bytes = source.encode_archive(&Deadline::new(30000)).unwrap();
    fs::write(&input, &bytes).unwrap();
    let args = [
        "source-acquisition",
        "read",
        "--input",
        input.to_str().unwrap(),
        "--arxiv-metadata",
    ];
    let out = invoke(root.path(), &args);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["networkConnections"], 0);
    assert_eq!(v["sourceAuthority"], false);
    assert_eq!(fs::read(&input).unwrap(), bytes);
    assert!(!root.path().join(".yoke").exists());
    let mut conflicts = args.to_vec();
    conflicts.push("--pdf-text");
    assert!(!invoke(root.path(), &conflicts).status.success());
}
#[test]
fn capture_grants_unknown_config_existing_output_and_recovery_grant_precede_effects() {
    let root = tempfile::tempdir().unwrap();
    let config = root.path().join("config.json");
    let output = root.path().join("output.json");
    let gate = root.path().join("arxiv-rate-v1.bin");
    let mut args = vec![
        "source-acquisition",
        "arxiv-capture",
        "--config",
        config.to_str().unwrap(),
        "--output",
        output.to_str().unwrap(),
    ];
    for flag in [
        "--allow-network",
        "--allow-output",
        "--allow-rate-coordination",
    ] {
        let out = invoke(root.path(), &args);
        assert!(!out.status.success());
        assert!(
            String::from_utf8_lossy(&out.stderr).contains("arxiv_source_host_permission_required")
        );
        assert!(!gate.exists() && !output.exists());
        args.push(flag);
    }
    let spec = json!({"id":"2301.00001","address":"127.0.0.1:9","coordinationFile":gate,"useSystemRoots":false});
    fs::write(&config, serde_json::to_vec(&spec).unwrap()).unwrap();
    fs::write(&output, b"foreign bytes").unwrap();
    let out = invoke(root.path(), &args);
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("fresh"));
    assert!(!gate.exists());
    assert_eq!(fs::read(&output).unwrap(), b"foreign bytes");
    fs::remove_file(&output).unwrap();
    let mut hostile = spec;
    hostile["allowNetwork"] = json!(true);
    fs::write(&config, serde_json::to_vec(&hostile).unwrap()).unwrap();
    assert!(!invoke(root.path(), &args).status.success());
    assert!(!gate.exists() && !output.exists());
    let out = invoke(
        root.path(),
        &[
            "source-acquisition",
            "arxiv-rate-recover",
            "--coordination-file",
            gate.to_str().unwrap(),
            "--expected-hash",
            &"f".repeat(64),
        ],
    );
    assert!(!out.status.success());
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("arxiv_rate_recovery_permission_required")
    );
    assert!(!gate.exists());
    let out = invoke(
        root.path(),
        &[
            "source-acquisition",
            "arxiv-rate-status",
            "--coordination-file",
            gate.to_str().unwrap(),
        ],
    );
    assert!(!out.status.success());
    assert!(!gate.exists());
    assert!(!root.path().join(".yoke").exists());
}
#[test]
fn recovery_cli_checks_exact_journal_and_retains_completed_identity_after_lost_stdout() {
    use sha2::{Digest, Sha256};
    use std::process::Stdio;
    let root = tempfile::tempdir().unwrap();
    let gate = root.path().join("arxiv-rate-v1.bin");
    let mut pending = [0u8; 64];
    pending[..8].copy_from_slice(b"YARATE1\0");
    pending[8..16].copy_from_slice(&1u64.to_le_bytes());
    pending[16..24].copy_from_slice(&1u64.to_le_bytes());
    pending[24] = 1;
    let checksum: [u8; 32] = Sha256::digest(&pending[..32]).into();
    pending[32..].copy_from_slice(&checksum);
    fs::write(&gate, pending).unwrap();
    let wrong = "f".repeat(64);
    let out = invoke(
        root.path(),
        &[
            "source-acquisition",
            "arxiv-rate-recover",
            "--coordination-file",
            gate.to_str().unwrap(),
            "--expected-hash",
            &wrong,
            "--allow-rate-recovery",
        ],
    );
    assert!(!out.status.success());
    assert_eq!(fs::read(&gate).unwrap(), pending);
    let hash = yoke::model::hash(pending);
    let mut command = Command::new(env!("CARGO_BIN_EXE_yoke"));
    command
        .args([
            "source-acquisition",
            "arxiv-rate-recover",
            "--coordination-file",
            gate.to_str().unwrap(),
            "--expected-hash",
            &hash,
            "--allow-rate-recovery",
        ])
        .current_dir(root.path())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000);
    }
    let mut child = command.spawn().unwrap();
    drop(child.stdout.take());
    let output = child.wait_with_output().unwrap();
    assert!(!output.status.success());
    let current = fs::read(&gate).unwrap();
    let current_hash = yoke::model::hash(&current);
    let error = String::from_utf8_lossy(&output.stderr);
    assert!(
        error.contains("arxiv_rate_recovery_completed_delivery_failed")
            && error.contains(&current_hash),
        "{error}"
    );
    let out = invoke(
        root.path(),
        &[
            "source-acquisition",
            "arxiv-rate-status",
            "--coordination-file",
            gate.to_str().unwrap(),
        ],
    );
    assert!(out.status.success());
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["phase"], "completed");
    assert_eq!(v["sequence"], 2);
    assert_eq!(v["journalHash"], current_hash);
    assert!(!root.path().join(".yoke").exists());
}
