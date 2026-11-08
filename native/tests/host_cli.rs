use serde_json::{Value, json};
use std::{fs, path::Path, process::Command};
use yoke::{
    Engine,
    capture::Deadline,
    federation::{self, ExportScope, ImportGrant, Scope},
};

fn command(root: &Path, args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_yoke"))
        .arg("--root")
        .arg(root)
        .args(args)
        .output()
        .unwrap()
}
#[test]
fn signed_source_cli_requires_a_host_grant_and_queries_a_separate_namespace() {
    let sender = tempfile::tempdir().unwrap();
    let receiver = tempfile::tempdir().unwrap();
    let inputs = tempfile::tempdir().unwrap();
    fs::write(
        sender.path().join("main.rs"),
        "// Signed fixture 🌍\nfn remote_invoice(){}\n",
    )
    .unwrap();
    let mut source = Engine::open(sender.path()).unwrap();
    let (snapshot, _, _, _) = source.index(&Deadline::new(10000)).unwrap();
    let seed = zeroize::Zeroizing::new([42u8; 32]);
    let scope = Scope {
        tenant_id: "fixture".into(),
        project_id: "billing".into(),
        sender_id: "sender".into(),
        key_id: "test".into(),
    };
    let public_key = hex::encode(
        ed25519_dalek::SigningKey::from_bytes(&seed)
            .verifying_key()
            .to_bytes(),
    );
    let bundle = federation::export_snapshot(
        &source.store,
        &snapshot,
        &ExportScope {
            scope: scope.clone(),
            ttl_ms: 60000,
        },
        &seed,
        &Deadline::new(10000),
    )
    .unwrap();
    let grant = ImportGrant {
        scope,
        public_key,
        sender_workspace_id: source.store.workspace_id.clone(),
        exclude: vec![],
    };
    let bundle_file = inputs.path().join("bundle.json");
    let grant_file = inputs.path().join("grant.json");
    fs::write(&bundle_file, serde_json::to_vec(&bundle).unwrap()).unwrap();
    fs::write(&grant_file, serde_json::to_vec(&grant).unwrap()).unwrap();
    let bundle_path = bundle_file.to_str().unwrap();
    let grant_path = grant_file.to_str().unwrap();
    assert!(
        !command(
            receiver.path(),
            &[
                "federation",
                "import",
                "--input",
                bundle_path,
                "--grant",
                grant_path
            ]
        )
        .status
        .success()
    );
    let imported = command(
        receiver.path(),
        &[
            "federation",
            "import",
            "--input",
            bundle_path,
            "--grant",
            grant_path,
            "--trust-grant",
        ],
    );
    assert!(
        imported.status.success(),
        "{}",
        String::from_utf8_lossy(&imported.stderr)
    );
    let imported: Value = serde_json::from_slice(&imported.stdout).unwrap();
    let id = imported["remoteId"].as_str().unwrap();
    let found = command(
        receiver.path(),
        &[
            "federation",
            "find",
            id,
            "remote_invoice",
            "--grant",
            grant_path,
            "--trust-grant",
        ],
    );
    assert!(
        found.status.success(),
        "{}",
        String::from_utf8_lossy(&found.stderr)
    );
    let found: Value = serde_json::from_slice(&found.stdout).unwrap();
    assert_eq!(found["data"]["items"][0]["name"], "remote_invoice");
    let read = command(
        receiver.path(),
        &[
            "federation",
            "read",
            id,
            "main.rs",
            "--grant",
            grant_path,
            "--trust-grant",
        ],
    );
    assert!(read.status.success());
    let read: Value = serde_json::from_slice(&read.stdout).unwrap();
    assert!(read["text"].as_str().unwrap().contains("🌍"));
    assert!(!receiver.path().join("main.rs").exists());
    let mut destination = Engine::open(receiver.path()).unwrap();
    assert!(destination.store.latest().unwrap().is_none());
    assert!(destination.store.snapshot(id).is_err());
    assert_eq!(
        destination
            .invoke("ci_read", json!({"snapshotId":id,"path":"main.rs"}))
            .unwrap()["status"],
        "error"
    );
}
#[test]
fn explicit_maintenance_cli_preserves_issued_artifacts_and_changes_no_source() {
    let root = tempfile::tempdir().unwrap();
    let original = format!("// {}\nfn main(){{}}\n", "x".repeat(10000));
    fs::write(root.path().join("main.rs"), &original).unwrap();
    let (artifact, orphan) = {
        let mut engine = Engine::open(root.path()).unwrap();
        let response = engine
            .invoke("ci_read", json!({"path":"main.rs","maxBytes":1024}))
            .unwrap();
        (
            response["artifactId"].as_str().unwrap().to_owned(),
            engine.store.blob(b"unreferenced fixture").unwrap(),
        )
    };
    let dry = command(
        root.path(),
        &["maintenance", "collect", "--minimum-age-ms", "0"],
    );
    assert!(dry.status.success());
    let dry: Value = serde_json::from_slice(&dry.stdout).unwrap();
    assert_eq!(dry["eligibleUnreferencedBlobs"], 1);
    assert_eq!(dry["applied"], false);
    let applied = command(
        root.path(),
        &["maintenance", "collect", "--minimum-age-ms", "0", "--apply"],
    );
    assert!(
        applied.status.success(),
        "{}",
        String::from_utf8_lossy(&applied.stderr)
    );
    let applied: Value = serde_json::from_slice(&applied.stdout).unwrap();
    assert_eq!(applied["quarantinedBlobs"], 1);
    assert_eq!(applied["reclaimedBytes"], 0);
    assert!(root.path().join(".yoke/blobs").join(&artifact).is_file());
    assert!(!root.path().join(".yoke/blobs").join(&orphan).exists());
    assert_eq!(
        fs::read_to_string(root.path().join("main.rs")).unwrap(),
        original
    );
}
