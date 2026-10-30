use ed25519_dalek::{Signer, SigningKey};
use serde_json::{Value, json};
use std::fs;
use std::time::{Duration, Instant};
use tempfile::TempDir;
use yoke::{
    Engine,
    capture::Deadline,
    editor,
    federation::{self, ExportScope, ImportGrant, Scope},
    model::Snapshot,
};

fn fixture(name: &str) -> (TempDir, Engine, Snapshot) {
    let dir = TempDir::new().unwrap();
    fs::write(
        dir.path().join("main.rs"),
        format!("// Grüße 🌍\nfn {name}() {{}}\n"),
    )
    .unwrap();
    fs::write(dir.path().join("README.md"), "# Invoice design\n").unwrap();
    let mut engine = Engine::open(dir.path()).unwrap();
    let (snapshot, _, _, _) = engine.index(&Deadline::new(10_000)).unwrap();
    (dir, engine, snapshot)
}
fn scope() -> ExportScope {
    ExportScope {
        scope: Scope {
            tenant_id: "tenant-one".into(),
            project_id: "billing".into(),
            sender_id: "source-host".into(),
            key_id: "ephemeral-test-key".into(),
        },
        ttl_ms: 60_000,
    }
}
fn key() -> [u8; 32] {
    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes).unwrap();
    bytes
}
fn grant(snapshot: &Snapshot, secret: &[u8; 32]) -> ImportGrant {
    ImportGrant {
        scope: scope().scope,
        public_key: hex::encode(SigningKey::from_bytes(secret).verifying_key().to_bytes()),
        sender_workspace_id: snapshot.workspace_id.clone(),
        exclude: Vec::new(),
    }
}

#[test]
fn signed_sources_are_separate_immutable_and_locally_reindexed_without_source_writes() {
    let (_sender_root, sender, source) = fixture("remote_invoice");
    let (receiver_root, receiver, local) = fixture("local_main");
    let secret = key();
    let policy = grant(&source, &secret);
    let deadline = Deadline::new(20_000);
    let envelope =
        federation::export_snapshot(&sender.store, &source, &scope(), &secret, &deadline).unwrap();
    let result =
        federation::import_snapshot(&receiver, envelope.clone(), &policy, &deadline).unwrap();
    let id = result["remoteId"].as_str().unwrap();
    assert_eq!(result["sourceWritten"], false);
    assert_eq!(receiver.store.latest().unwrap(), Some(local.id));
    assert!(
        !fs::read_to_string(receiver_root.path().join("main.rs"))
            .unwrap()
            .contains("remote_invoice")
    );
    assert!(
        receiver.store.snapshot(id).is_err(),
        "Remote source must not enter the ordinary local snapshot namespace"
    );
    let read = federation::read(&receiver, id, "main.rs", &policy, &deadline).unwrap();
    assert!(read["text"].as_str().unwrap().contains("Grüße 🌍"));
    assert!(read["text"].as_str().unwrap().contains("remote_invoice"));
    let graph = federation::graph(&receiver, id, &policy, &deadline).unwrap();
    assert!(graph.entities.iter().any(|e| e.name == "remote_invoice"));
    assert!(!graph.coverage.exhaustive);
    let repeated = federation::import_snapshot(&receiver, envelope, &policy, &deadline).unwrap();
    assert_eq!(repeated["remoteId"], id);
    assert_eq!(repeated["idempotent"], true);
    let list = federation::list(&receiver.store, &deadline).unwrap();
    assert_eq!(list["items"].as_array().unwrap().len(), 1);
    assert_eq!(list["items"][0]["scope"]["tenantId"], "tenant-one");
}

#[test]
fn granted_foreign_search_reuses_verified_sources_without_native_publication() {
    let (_sender_root, sender, source) = fixture("remote_invoice");
    let (receiver_root, receiver, local) = fixture("local_main");
    let secret = key();
    let policy = grant(&source, &secret);
    let deadline = Deadline::new(20_000);
    let envelope =
        federation::export_snapshot(&sender.store, &source, &scope(), &secret, &deadline).unwrap();
    let imported = federation::import_snapshot(&receiver, envelope, &policy, &deadline).unwrap();
    let id = imported["remoteId"].as_str().unwrap();
    for kind in ["symbol", "all", "text"] {
        let mut args = json!({
            "kind":kind,"query":"remote_invoice","skipIgnored":true,
            "pathGlob":"**/*.rs","limit":10
        });
        if kind == "symbol" {
            args["includeBody"] = json!(true);
        }
        let found = federation::find(&receiver, id, &policy, &args, &deadline).unwrap();
        assert!(
            !found["items"].as_array().unwrap().is_empty(),
            "{kind}: {found}"
        );
        assert_eq!(found["items"][0]["path"], "main.rs");
        assert_eq!(
            found["items"][0]["contentHash"],
            source
                .files
                .iter()
                .find(|file| file.path == "main.rs")
                .unwrap()
                .hash
        );
        if kind == "symbol" {
            assert!(
                found["items"][0]["body"]
                    .as_str()
                    .unwrap()
                    .contains("remote_invoice")
            );
        }
    }
    let foreign = federation::snapshot(&receiver, id, &policy, &deadline).unwrap();
    let graph = federation::graph(&receiver, id, &policy, &deadline).unwrap();
    assert!(
        yoke::query_features::symbols_find(
            &receiver.store,
            &foreign,
            &graph,
            &json!({"query":"remote_invoice"}),
            &deadline
        )
        .is_err()
    );
    let mut rotated = policy.clone();
    rotated.public_key = hex::encode(SigningKey::from_bytes(&key()).verifying_key().to_bytes());
    assert!(
        federation::find(
            &receiver,
            id,
            &rotated,
            &json!({"query":"remote_invoice"}),
            &deadline
        )
        .is_err()
    );
    let mut excluded = policy.clone();
    excluded.exclude.push("main.rs".into());
    assert!(
        federation::find(
            &receiver,
            id,
            &excluded,
            &json!({"query":"remote_invoice"}),
            &deadline
        )
        .is_err()
    );
    fs::write(receiver_root.path().join(".gitignore"), "main.rs\n").unwrap();
    assert!(
        federation::find(
            &receiver,
            id,
            &policy,
            &json!({"kind":"text","query":"remote_invoice"}),
            &deadline
        )
        .is_err()
    );
    assert_eq!(receiver.store.latest().unwrap(), Some(local.id));
    assert!(receiver.store.snapshot(id).is_err());
    assert!(receiver.store.graph(id).is_err());
}

#[test]
fn canonical_editor_overlays_round_trip_without_saved_source_or_reexport_authority() {
    let (sender_root, mut sender, _) = fixture("saved_invoice");
    let (receiver_root, receiver, local) = fixture("local_main");
    let deadline = Deadline::new(20_000);
    let session = editor::open(&mut sender, &deadline).unwrap();
    let text = "// Grüße 🌍\nfn unsaved_invoice() {}\n";
    editor::update(
        &sender,
        &session.id,
        &json!({"path":"main.rs","version":1,"text":text}),
        &deadline,
    )
    .unwrap();
    let overlay = editor::capture(&mut sender, &session.id, &deadline).unwrap();
    assert_eq!(overlay.kind, "overlay");
    let secret = key();
    let policy = grant(&overlay, &secret);
    let envelope =
        federation::export_snapshot(&sender.store, &overlay, &scope(), &secret, &deadline).unwrap();
    assert_eq!(envelope["manifest"]["sourceKind"], "overlay");
    let imported = federation::import_snapshot(&receiver, envelope, &policy, &deadline).unwrap();
    let id = imported["remoteId"].as_str().unwrap();
    let read = federation::read(&receiver, id, "main.rs", &policy, &deadline).unwrap();
    assert_eq!(read["text"], text);
    let graph = federation::graph(&receiver, id, &policy, &deadline).unwrap();
    assert!(
        graph
            .entities
            .iter()
            .any(|entity| entity.name == "unsaved_invoice")
    );
    assert_eq!(receiver.store.latest().unwrap(), Some(local.id));
    for (root, name) in [
        (&sender_root, "saved_invoice"),
        (&receiver_root, "local_main"),
    ] {
        assert_eq!(
            fs::read_to_string(root.path().join("main.rs")).unwrap(),
            format!("// Grüße 🌍\nfn {name}() {{}}\n")
        );
    }
    let mut forged = overlay.clone();
    forged.files[0].mode ^= 0o100;
    assert!(
        federation::export_snapshot(&sender.store, &forged, &scope(), &secret, &deadline).is_err(),
        "Overlay export must match the canonical stored snapshot exactly"
    );
    let remote = federation::snapshot(&receiver, id, &policy, &deadline).unwrap();
    assert!(
        federation::export_snapshot(&receiver.store, &remote, &scope(), &secret, &deadline)
            .is_err(),
        "Accepted overlay source must retain its separate remote namespace"
    );
}

#[test]
fn tampered_bytes_signatures_scope_and_workspace_are_rejected_before_persistence() {
    let (_sender_root, sender, source) = fixture("remote_invoice");
    let (_receiver_root, receiver, _) = fixture("local_main");
    let secret = key();
    let policy = grant(&source, &secret);
    let deadline = Deadline::new(20_000);
    let envelope =
        federation::export_snapshot(&sender.store, &source, &scope(), &secret, &deadline).unwrap();
    for field in ["projectId", "tenantId", "senderId", "keyId"] {
        let mut bad = envelope.clone();
        bad["manifest"]["scope"][field] = json!("attacker");
        assert!(federation::import_snapshot(&receiver, bad, &policy, &deadline).is_err());
    }
    let mut bad = envelope.clone();
    bad["manifest"]["files"][0]["contentHash"] = json!("0".repeat(64));
    assert!(federation::import_snapshot(&receiver, bad, &policy, &deadline).is_err());
    let mut bad = envelope.clone();
    bad["contents"][0]["base64"] = json!("dGFtcGVyZWQ=");
    assert!(federation::import_snapshot(&receiver, bad, &policy, &deadline).is_err());
    let mut bad = envelope.clone();
    bad["signature"] = json!("0".repeat(128));
    assert!(federation::import_snapshot(&receiver, bad, &policy, &deadline).is_err());
    let mut denied = policy.clone();
    denied.sender_workspace_id = "0".repeat(64);
    assert!(federation::import_snapshot(&receiver, envelope.clone(), &denied, &deadline).is_err());
    let mut denied = policy;
    denied.public_key = hex::encode(SigningKey::from_bytes(&key()).verifying_key().to_bytes());
    assert!(federation::import_snapshot(&receiver, envelope, &denied, &deadline).is_err());
    let list = federation::list(&receiver.store, &deadline).unwrap();
    assert!(list["items"].as_array().unwrap().is_empty());
}

#[test]
fn receiver_ignore_excludes_links_and_remote_policy_text_cannot_change_host_authority() {
    let (sender_root, mut sender, _) = fixture("remote_invoice");
    fs::write(
        sender_root.path().join("private.rs"),
        "fn private_source() {}\n",
    )
    .unwrap();
    fs::write(sender_root.path().join(".gitignore"), "!private.rs\n").unwrap();
    let (source, _, _, _) = sender.index(&Deadline::new(10_000)).unwrap();
    let (receiver_root, receiver, _) = fixture("local_main");
    fs::write(receiver_root.path().join(".gitignore"), "private.rs\n").unwrap();
    let secret = key();
    let policy = grant(&source, &secret);
    let deadline = Deadline::new(20_000);
    let envelope =
        federation::export_snapshot(&sender.store, &source, &scope(), &secret, &deadline).unwrap();
    assert!(federation::import_snapshot(&receiver, envelope.clone(), &policy, &deadline).is_err());
    fs::remove_file(receiver_root.path().join(".gitignore")).unwrap();
    let mut excluded = policy.clone();
    excluded.exclude = vec!["private.rs".into()];
    assert!(
        federation::import_snapshot(&receiver, envelope.clone(), &excluded, &deadline).is_err()
    );
    let outside = TempDir::new().unwrap();
    fs::write(outside.path().join("other.rs"), "fn outsider() {}\n").unwrap();
    #[cfg(windows)]
    std::os::windows::fs::symlink_file(
        outside.path().join("other.rs"),
        receiver_root.path().join("private.rs"),
    )
    .unwrap();
    #[cfg(unix)]
    std::os::unix::fs::symlink(
        outside.path().join("other.rs"),
        receiver_root.path().join("private.rs"),
    )
    .unwrap();
    assert!(federation::import_snapshot(&receiver, envelope, &policy, &deadline).is_err());
}

#[test]
fn valid_signature_does_not_authorize_protected_secret_binary_or_colliding_source() {
    let (_sender_root, sender, source) = fixture("remote_invoice");
    let (_receiver_root, receiver, _) = fixture("local_main");
    let secret = key();
    let policy = grant(&source, &secret);
    let deadline = Deadline::new(20_000);
    let envelope =
        federation::export_snapshot(&sender.store, &source, &scope(), &secret, &deadline).unwrap();
    let mut forged = source.clone();
    forged.files[0].path = ".env".into();
    assert!(
        federation::export_snapshot(&sender.store, &forged, &scope(), &secret, &deadline).is_err()
    );
    let mut extra = envelope.clone();
    extra["allowWrites"] = json!(true);
    assert!(federation::import_snapshot(&receiver, extra, &policy, &deadline).is_err());
    assert!(federation::import_snapshot(&receiver, envelope, &policy, &Deadline::new(0)).is_err());
}

fn resign(mut envelope: Value, secret: &[u8; 32]) -> Value {
    // Independently serialized protocol signing bytes; no production serializer.
    let mut message = b"YOKE-SOURCE-FEDERATION-V1\0".to_vec();
    message.extend(serde_json::to_vec(&envelope["manifest"]).unwrap());
    envelope["signature"] = json!(hex::encode(
        SigningKey::from_bytes(secret).sign(&message).to_bytes()
    ));
    envelope
}

#[test]
fn correctly_signed_malicious_sources_still_fail_receiver_safety_policy() {
    use base64::{Engine as _, engine::general_purpose::STANDARD};
    let (_sender_root, sender, source) = fixture("remote_invoice");
    let (_receiver_root, receiver, _) = fixture("local_main");
    let secret = key();
    let policy = grant(&source, &secret);
    let deadline = Deadline::new(20_000);
    let envelope =
        federation::export_snapshot(&sender.store, &source, &scope(), &secret, &deadline).unwrap();
    let mut protected = envelope.clone();
    protected["manifest"]["files"][0]["path"] = json!(".env");
    protected["contents"][0]["path"] = json!(".env");
    assert!(
        federation::import_snapshot(&receiver, resign(protected, &secret), &policy, &deadline)
            .is_err()
    );
    for payload in [
        vec![0u8, 255, 10],
        format!("token='ghp_{}'\n", "A".repeat(36)).into_bytes(),
    ] {
        let mut malicious = envelope.clone();
        malicious["manifest"]["files"][0]["contentHash"] = json!(yoke::model::hash(&payload));
        malicious["manifest"]["files"][0]["size"] = json!(payload.len());
        malicious["contents"][0]["base64"] = json!(STANDARD.encode(payload));
        assert!(
            federation::import_snapshot(&receiver, resign(malicious, &secret), &policy, &deadline)
                .is_err()
        );
    }
    let mut duplicate = envelope.clone();
    let mut upper = duplicate["manifest"]["files"][0].clone();
    let mut lower = upper.clone();
    upper["path"] = json!("MAIN.rs");
    lower["path"] = json!("main.rs");
    duplicate["manifest"]["files"] = json!([upper, lower]);
    let mut upper = duplicate["contents"][0].clone();
    let mut lower = upper.clone();
    upper["path"] = json!("MAIN.rs");
    lower["path"] = json!("main.rs");
    duplicate["contents"] = json!([upper, lower]);
    assert!(
        federation::import_snapshot(&receiver, resign(duplicate, &secret), &policy, &deadline)
            .is_err()
    );
    assert!(
        federation::list(&receiver.store, &deadline).unwrap()["items"]
            .as_array()
            .unwrap()
            .is_empty()
    );
}

#[test]
fn nonce_reuse_expired_import_and_current_grant_revocation_are_distinct() {
    let (_sender_root, sender, source) = fixture("remote_invoice");
    let (receiver_root, receiver, _) = fixture("local_main");
    let secret = key();
    let policy = grant(&source, &secret);
    let deadline = Deadline::new(20_000);
    let envelope =
        federation::export_snapshot(&sender.store, &source, &scope(), &secret, &deadline).unwrap();
    let accepted =
        federation::import_snapshot(&receiver, envelope.clone(), &policy, &deadline).unwrap();
    let mut reused = envelope.clone();
    reused["manifest"]["sourceSnapshotId"] = json!("1".repeat(64));
    assert!(
        federation::import_snapshot(&receiver, resign(reused, &secret), &policy, &deadline)
            .is_err()
    );
    let mut expired = envelope;
    expired["manifest"]["issuedAtMs"] = json!(1);
    expired["manifest"]["expiresAtMs"] = json!(2);
    assert!(
        federation::import_snapshot(&receiver, resign(expired, &secret), &policy, &deadline)
            .is_err()
    );
    let mut revoked = policy.clone();
    revoked.public_key = hex::encode(SigningKey::from_bytes(&key()).verifying_key().to_bytes());
    assert!(
        federation::read(
            &receiver,
            accepted["remoteId"].as_str().unwrap(),
            "main.rs",
            &revoked,
            &deadline
        )
        .is_err()
    );
    fs::write(receiver_root.path().join(".gitignore"), "main.rs\n").unwrap();
    assert!(
        federation::read(
            &receiver,
            accepted["remoteId"].as_str().unwrap(),
            "main.rs",
            &policy,
            &deadline
        )
        .is_err()
    );
}

#[test]
fn source_namespace_capacity_accounts_for_every_file_in_prior_receipts() {
    let (_sender_root, sender, source) = fixture("remote_invoice");
    let (_receiver_root, receiver, _) = fixture("local_main");
    let secret = key();
    let policy = grant(&source, &secret);
    let deadline = Deadline::new(20_000);
    let envelope =
        federation::export_snapshot(&sender.store, &source, &scope(), &secret, &deadline).unwrap();
    federation::import_snapshot(&receiver, envelope, &policy, &deadline).unwrap();
    receiver.store.db.execute("UPDATE records SET value=json_set(value,'$.snapshot.files[0].size',512000000) WHERE kind='native_federation_receipt_v1'",[]).unwrap();
    let second =
        federation::export_snapshot(&sender.store, &source, &scope(), &secret, &deadline).unwrap();
    assert!(
        federation::import_snapshot(&receiver, second, &policy, &deadline).is_err(),
        "Only the last source file was counted toward namespace capacity"
    );
}

#[test]
fn corrupt_remote_graph_cannot_be_returned_as_a_successful_empty_index() {
    let (_sender_root, sender, source) = fixture("remote_invoice");
    let (_receiver_root, receiver, _) = fixture("local_main");
    let secret = key();
    let policy = grant(&source, &secret);
    let deadline = Deadline::new(20_000);
    let envelope =
        federation::export_snapshot(&sender.store, &source, &scope(), &secret, &deadline).unwrap();
    let result = federation::import_snapshot(&receiver, envelope, &policy, &deadline).unwrap();
    receiver.store.db.execute("UPDATE records SET value=json_set(value,'$.entities',json('[]')) WHERE kind='native_federation_graph_v1'",[]).unwrap();
    assert!(
        federation::graph(
            &receiver,
            result["remoteId"].as_str().unwrap(),
            &policy,
            &deadline
        )
        .is_err(),
        "Corrupted graph was accepted as an empty index"
    );
}

#[test]
fn host_exclude_patterns_cannot_be_bypassed_with_path_case_aliases() {
    let (sender_root, mut sender, _) = fixture("remote_invoice");
    fs::write(
        sender_root.path().join("Private.rs"),
        "fn confidential() {}\n",
    )
    .unwrap();
    let deadline = Deadline::new(20_000);
    let (source, _, _, _) = sender.index(&deadline).unwrap();
    let (_receiver_root, receiver, _) = fixture("local_main");
    let secret = key();
    let mut policy = grant(&source, &secret);
    policy.exclude = vec!["private.rs".into()];
    let envelope =
        federation::export_snapshot(&sender.store, &source, &scope(), &secret, &deadline).unwrap();
    assert!(
        federation::import_snapshot(&receiver, envelope, &policy, &deadline).is_err(),
        "Case alias bypassed explicit host deny pattern"
    );
}

#[test]
fn receiver_effective_ignore_precedence_handles_virtual_paths_and_whitelists() {
    let (sender_root, mut sender, _) = fixture("remote_invoice");
    fs::remove_file(sender_root.path().join("main.rs")).unwrap();
    fs::create_dir(sender_root.path().join("sub")).unwrap();
    fs::write(
        sender_root.path().join("sub/visible.rs"),
        "fn visible() {}\n",
    )
    .unwrap();
    let deadline = Deadline::new(20_000);
    let (source, _, _, _) = sender.index(&deadline).unwrap();
    let (receiver_root, receiver, _) = fixture("local_main");
    fs::create_dir(receiver_root.path().join("sub")).unwrap();
    fs::write(receiver_root.path().join(".ignore"), "*.rs\n").unwrap();
    fs::write(receiver_root.path().join("sub/.gitignore"), "!visible.rs\n").unwrap();
    let secret = key();
    let policy = grant(&source, &secret);
    let envelope =
        federation::export_snapshot(&sender.store, &source, &scope(), &secret, &deadline).unwrap();
    assert!(
        federation::import_snapshot(&receiver, envelope, &policy, &deadline).is_err(),
        "Nested git whitelist overrode higher-priority .ignore policy"
    );
    fs::remove_file(receiver_root.path().join(".ignore")).unwrap();
    fs::write(receiver_root.path().join(".gitignore"), "*.rs\n").unwrap();
    let envelope =
        federation::export_snapshot(&sender.store, &source, &scope(), &secret, &deadline).unwrap();
    let imported = federation::import_snapshot(&receiver, envelope, &policy, &deadline).unwrap();
    assert!(
        federation::read(
            &receiver,
            imported["remoteId"].as_str().unwrap(),
            "sub/visible.rs",
            &policy,
            &deadline
        )
        .unwrap()["text"]
            .as_str()
            .unwrap()
            .contains("visible")
    );
    fs::write(receiver_root.path().join(".gitignore"), "sub/\n").unwrap();
    let envelope =
        federation::export_snapshot(&sender.store, &source, &scope(), &secret, &deadline).unwrap();
    assert!(
        federation::import_snapshot(&receiver, envelope, &policy, &deadline).is_err(),
        "Ignored parent directory was re-enabled by a leaf whitelist"
    );
}

#[test]
fn sqlite_writer_contention_consumes_only_the_remaining_import_deadline() {
    let (sender_root, mut sender, _) = fixture("remote_invoice");
    let text = format!(
        "{}\nfn remote_invoice() {{}}\n",
        "// source padding for elapsed analysis\n".repeat(12_000)
    );
    fs::write(sender_root.path().join("main.rs"), &text).unwrap();
    let (source, _, _, _) = sender.index(&Deadline::new(20_000)).unwrap();
    let (receiver_root, receiver, local) = fixture("local_main");
    let secret = key();
    let policy = grant(&source, &secret);
    let initial = federation::export_snapshot(
        &sender.store,
        &source,
        &scope(),
        &secret,
        &Deadline::new(20_000),
    )
    .unwrap();
    let started = Instant::now();
    federation::import_snapshot(&receiver, initial, &policy, &Deadline::new(20_000)).unwrap();
    let preparation = started.elapsed();
    // Scale to actual preprocessing speed while leaving enough time to reach
    // transaction admission. The lock stays held throughout the failed import.
    let budget_ms = (preparation.as_millis() as u64 * 3).max(500);
    let next = federation::export_snapshot(
        &sender.store,
        &source,
        &scope(),
        &secret,
        &Deadline::new(20_000),
    )
    .unwrap();
    let foreign = rusqlite::Connection::open(receiver.store.dir.join("index.sqlite")).unwrap();
    foreign.execute_batch("BEGIN IMMEDIATE").unwrap();
    let started = Instant::now();
    let error = federation::import_snapshot(&receiver, next, &policy, &Deadline::new(budget_ms))
        .unwrap_err();
    let elapsed = started.elapsed();
    foreign.execute_batch("ROLLBACK").unwrap();
    receiver.store.clear_deadline();
    // Allow operating-system scheduling slack without the additional full
    // preprocessing interval reproduced by the stale SQLite busy timeout.
    assert!(
        elapsed < Duration::from_millis(budget_ms + 250),
        "Import reused the initial busy timeout after preprocessing: budget={budget_ms}ms elapsed={elapsed:?} preparation={preparation:?} error={error}"
    );
    assert_eq!(receiver.store.latest().unwrap(), Some(local.id));
    assert_eq!(
        federation::list(&receiver.store, &Deadline::new(10_000)).unwrap()["items"]
            .as_array()
            .unwrap()
            .len(),
        1,
        "Contended import must leave no partial receipt"
    );
    assert_eq!(
        fs::read_to_string(receiver_root.path().join("main.rs")).unwrap(),
        "// Grüße 🌍\nfn local_main() {}\n"
    );
}
