use serde_json::{Value, json};
use std::{fs, process::Command};
use yoke::{Engine, capture::Deadline, graph_collections::Registry};

#[test]
fn archived_collection_cli_queries_without_opening_a_source_project() {
    let source = tempfile::tempdir().unwrap();
    let registry_root = tempfile::tempdir().unwrap();
    let empty = tempfile::tempdir().unwrap();
    fs::write(
        source.path().join("invoice.rs"),
        "pub fn invoice() -> u32 { 17 }\n",
    )
    .unwrap();
    let snapshot = {
        let mut engine = Engine::open(source.path()).unwrap();
        let deadline = Deadline::new(5000);
        let (snapshot, graph, _, _) = engine.index(&deadline).unwrap();
        let registry = Registry::open(registry_root.path()).unwrap();
        registry
            .register_cas("billing", &engine.store, &snapshot, &graph, None, &deadline)
            .unwrap();
        snapshot.id
    };
    source.close().unwrap();
    let list = Command::new(env!("CARGO_BIN_EXE_yoke"))
        .arg("--root")
        .arg(empty.path().join("never-open-this"))
        .args(["collections", "--registry"])
        .arg(registry_root.path())
        .arg("list")
        .output()
        .unwrap();
    assert!(
        list.status.success(),
        "{}",
        String::from_utf8_lossy(&list.stderr)
    );
    let list: Value = serde_json::from_slice(&list.stdout).unwrap();
    assert_eq!(list["items"][0]["snapshotId"], snapshot);
    assert_eq!(list["items"][0]["kind"], "native");
    let found = Command::new(env!("CARGO_BIN_EXE_yoke"))
        .arg("--root")
        .arg(empty.path().join("never-open-this"))
        .args(["collections", "--registry"])
        .arg(registry_root.path())
        .args([
            "query",
            "--member",
            "billing",
            "--args",
            &json!({"operation":"find","query":"invoice","kind":"function"}).to_string(),
        ])
        .output()
        .unwrap();
    assert!(
        found.status.success(),
        "{}",
        String::from_utf8_lossy(&found.stderr)
    );
    let found: Value = serde_json::from_slice(&found.stdout).unwrap();
    assert_eq!(found["items"][0]["name"], "invoice");
    assert!(!empty.path().join("never-open-this").exists());
}

#[test]
fn collection_registration_requires_separate_authority_before_creating_any_state() {
    let source = tempfile::tempdir().unwrap();
    let registry_root = tempfile::tempdir().unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_yoke"))
        .arg("--root")
        .arg(source.path())
        .args(["collections", "--registry"])
        .arg(registry_root.path())
        .args(["register", "billing", "--expect-absent"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("requires --allow-registration"),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!source.path().join(".yoke").exists());
    assert_eq!(fs::read_dir(registry_root.path()).unwrap().count(), 0);
}

#[test]
fn collection_cli_registration_is_create_only_or_exact_compare_and_swap() {
    let source = tempfile::tempdir().unwrap();
    let registry_root = tempfile::tempdir().unwrap();
    fs::write(source.path().join("main.rs"), "fn before() {}\n").unwrap();
    let register = |extra: &[&str]| {
        Command::new(env!("CARGO_BIN_EXE_yoke"))
            .arg("--root")
            .arg(source.path())
            .args(["collections", "--registry"])
            .arg(registry_root.path())
            .args(["--allow-registration", "register", "project"])
            .args(extra)
            .output()
            .unwrap()
    };
    let first = register(&["--expect-absent"]);
    assert!(
        first.status.success(),
        "{}",
        String::from_utf8_lossy(&first.stderr)
    );
    let first: Value = serde_json::from_slice(&first.stdout).unwrap();
    let id = first["registrationId"].as_str().unwrap();
    fs::write(source.path().join("main.rs"), "fn after() {}\n").unwrap();
    assert!(!register(&["--expect-absent"]).status.success());
    assert!(!register(&["--expected", &"0".repeat(64)]).status.success());
    {
        let registry = Registry::open_existing(registry_root.path()).unwrap();
        assert_eq!(
            registry.describe("project", &Deadline::new(5000)).unwrap()["registrationId"],
            id
        );
    }
    let second = register(&["--expected", id]);
    assert!(
        second.status.success(),
        "{}",
        String::from_utf8_lossy(&second.stderr)
    );
    let second: Value = serde_json::from_slice(&second.stdout).unwrap();
    assert_ne!(second["registrationId"], id);
    let removed = Command::new(env!("CARGO_BIN_EXE_yoke"))
        .arg("--root")
        .arg(source.path().join("unused"))
        .args(["collections", "--registry"])
        .arg(registry_root.path())
        .args([
            "--allow-registration",
            "remove",
            "project",
            "--expected",
            second["registrationId"].as_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(
        removed.status.success(),
        "{}",
        String::from_utf8_lossy(&removed.stderr)
    );
    assert!(!source.path().join("unused").exists());
    let registry = Registry::open_existing(registry_root.path()).unwrap();
    assert_eq!(registry.list(&Deadline::new(5000)).unwrap()["count"], 0);
}

#[test]
fn unknown_collection_cli_registry_does_not_create_files() {
    let root = tempfile::tempdir().unwrap();
    let registry = root.path().join("unregistered");
    let output = Command::new(env!("CARGO_BIN_EXE_yoke"))
        .arg("--root")
        .arg(root.path())
        .args(["collections", "--registry"])
        .arg(&registry)
        .arg("list")
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(!registry.exists());
    assert!(!root.path().join(".yoke").exists());
}

#[test]
fn foreign_collection_cli_reloads_current_host_grants_and_keeps_source_only_distinct() {
    use yoke::federation::{self, ExportScope, ImportGrant, Scope};
    let sender_root = tempfile::tempdir().unwrap();
    let receiver_root = tempfile::tempdir().unwrap();
    let central = tempfile::tempdir().unwrap();
    let profiles = tempfile::tempdir().unwrap();
    let text = "fn foreign_target() {}\nfn caller() { foreign_target(); foreign_target(); }\n";
    fs::write(sender_root.path().join("main.rs"), text).unwrap();
    let deadline = Deadline::new(20000);
    let mut sender = Engine::open(sender_root.path()).unwrap();
    let (snapshot, _, _, _) = sender.index(&deadline).unwrap();
    let receiver = Engine::open(receiver_root.path()).unwrap();
    let scope = Scope {
        tenant_id: "fixture".into(),
        project_id: "fixture".into(),
        sender_id: "fixture".into(),
        key_id: "fixture".into(),
    };
    let key = ed25519_dalek::SigningKey::from_bytes(&[47; 32]);
    let grant = ImportGrant {
        scope: scope.clone(),
        public_key: hex::encode(key.verifying_key().to_bytes()),
        sender_workspace_id: snapshot.workspace_id.clone(),
        exclude: vec![],
    };
    let wire = federation::export_snapshot(
        &sender.store,
        &snapshot,
        &ExportScope {
            scope,
            ttl_ms: 60000,
        },
        &key.to_bytes(),
        &deadline,
    )
    .unwrap();
    let imported = federation::import_snapshot(&receiver, wire, &grant, &deadline).unwrap();
    let remote = imported["remoteId"].as_str().unwrap().to_owned();
    drop(receiver);
    drop(sender);
    sender_root.close().unwrap();
    let profile_path = profiles.path().join("federation.json");
    let profile =
        json!({"receiverRoot":receiver_root.path(),"imports":[{"id":"live","grant":grant}]});
    fs::write(&profile_path, profile.to_string()).unwrap();
    let run = |args: &[&str], authorized: bool| {
        let mut command = Command::new(env!("CARGO_BIN_EXE_yoke"));
        command
            .args(["collections", "--registry"])
            .arg(central.path());
        if authorized {
            command
                .arg("--federation-profile")
                .arg(&profile_path)
                .arg("--trust-grant");
        }
        command.args(args).output().unwrap()
    };
    for (name, source_only) in [("foreign", false), ("sources", true)] {
        let mut args = vec![
            "--allow-registration",
            "register-federated",
            name,
            "--grant-id",
            "live",
            "--remote-id",
            &remote,
            "--expect-absent",
        ];
        if source_only {
            args.push("--source-only");
        }
        let registered = run(&args, true);
        assert!(
            registered.status.success(),
            "{}",
            String::from_utf8_lossy(&registered.stderr)
        );
    }
    assert!(!run(&["describe", "foreign"], false).status.success());
    for (name, kind) in [
        ("foreign", "federatedCanonical"),
        ("sources", "federatedSources"),
    ] {
        let output = run(&["describe", name], true);
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let output: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(output["kind"], kind);
        assert_eq!(output["authorization"], "current_host_context_rechecked");
    }
    let read = run(
        &[
            "read-source",
            "sources",
            "main.rs",
            "--start-byte",
            "0",
            "--end-byte",
            &text.len().to_string(),
        ],
        true,
    );
    assert!(
        read.status.success(),
        "{}",
        String::from_utf8_lossy(&read.stderr)
    );
    let read: Value = serde_json::from_slice(&read.stdout).unwrap();
    assert_eq!(read["text"], text);
    let query = json!({"operation":"find","query":"foreign_target","kind":"function"}).to_string();
    let found = run(&["query", "--member", "foreign", "--args", &query], true);
    assert!(
        found.status.success(),
        "{}",
        String::from_utf8_lossy(&found.stderr)
    );
    assert_eq!(
        serde_json::from_slice::<Value>(&found.stdout).unwrap()["items"][0]["name"],
        "foreign_target"
    );
    let mut revoked = profile;
    revoked["imports"] = json!([]);
    fs::write(&profile_path, revoked.to_string()).unwrap();
    assert!(!run(&["describe", "foreign"], true).status.success());
    assert!(
        !run(
            &[
                "read-source",
                "sources",
                "main.rs",
                "--start-byte",
                "0",
                "--end-byte",
                "2"
            ],
            true
        )
        .status
        .success()
    );
}
