use base64::Engine as _;
use serde_json::json;
use yoke::Engine;
#[test]
fn binary_artifacts_page_with_explicit_base64_without_loss_or_budget_overflow() {
    let root = tempfile::tempdir().unwrap();
    let mut engine = Engine::open(root.path()).unwrap();
    let bytes = (0..=255).cycle().take(12000).collect::<Vec<u8>>();
    let id = engine.store.artifact(&bytes).unwrap();
    let mut cursor = "0".to_string();
    let mut restored = Vec::new();
    loop {
        let result = engine
            .invoke(
                "ci_read",
                json!({"artifactId":id,"encoding":"base64","cursor":cursor,"maxBytes":1024}),
            )
            .unwrap();
        assert_eq!(result["status"], "ok", "{result}");
        assert!(serde_json::to_vec(&result).unwrap().len() <= 1024);
        restored.extend(
            base64::prelude::BASE64_STANDARD
                .decode(result["data"]["chunk"].as_str().unwrap())
                .unwrap(),
        );
        if let Some(next) = result["data"]["nextCursor"].as_str() {
            cursor = next.to_string();
        } else {
            break;
        }
    }
    assert_eq!(restored, bytes);
    assert_eq!(
        engine.invoke("ci_read", json!({"artifactId":id})).unwrap()["status"],
        "error"
    );
}

#[test]
fn cas_artifact_pages_cannot_claim_an_unrelated_snapshot() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("main.rs"), "fn old() {}\n").unwrap();
    let mut engine = Engine::open(root.path()).unwrap();
    let old = engine.invoke("ci_read", json!({"path":"main.rs"})).unwrap();
    std::fs::write(root.path().join("main.rs"), "fn changed() {}\n").unwrap();
    let fresh = engine.invoke("ci_read", json!({"path":"main.rs"})).unwrap();
    for encoding in ["utf8", "base64"] {
        let page = engine.invoke("ci_read", json!({"snapshotId":old["snapshotId"],"artifactId":fresh["data"]["contentHash"],"encoding":encoding})).unwrap();
        assert_eq!(page["status"], "ok", "{page}");
        assert!(
            page["snapshotId"].is_null(),
            "Generic CAS paging must not claim unrelated source scope: {page}"
        );
    }
}

#[test]
fn published_response_artifacts_remain_roots_during_explicit_maintenance() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(
        root.path().join("main.rs"),
        format!("// {}\nfn main(){{}}\n", "x".repeat(10000)),
    )
    .unwrap();
    let mut engine = Engine::open(root.path()).unwrap();
    let result = engine
        .invoke("ci_read", json!({"path":"main.rs","maxBytes":1024}))
        .unwrap();
    assert_eq!(result["status"], "partial");
    let id = result["artifactId"].as_str().unwrap();
    let before = engine.store.read_blob(id).unwrap();
    let report = yoke::maintenance::collect(
        &engine.store,
        &yoke::maintenance::Policy {
            apply: true,
            minimum_age_ms: 0,
            purge_quarantine: false,
        },
        &yoke::capture::Deadline::new(10000),
    )
    .unwrap();
    assert_eq!(
        engine.store.read_blob(id).unwrap(),
        before,
        "Issued response artifact was quarantined: {report}"
    );
}

#[test]
fn foreign_source_and_unpublished_blobs_cannot_bypass_explicit_read_grants() {
    use yoke::{
        capture::Deadline,
        federation::{self, ExportScope, ImportGrant, Scope},
    };
    let sender_root = tempfile::tempdir().unwrap();
    let receiver_root = tempfile::tempdir().unwrap();
    std::fs::write(
        sender_root.path().join("private.rs"),
        "fn remote_private(){}\n",
    )
    .unwrap();
    let mut sender = Engine::open(sender_root.path()).unwrap();
    let mut receiver = Engine::open(receiver_root.path()).unwrap();
    let deadline = Deadline::new(10000);
    let (snapshot, _, _, _) = sender.index(&deadline).unwrap();
    let scope = Scope {
        tenant_id: "fixture".into(),
        project_id: "private".into(),
        sender_id: "sender".into(),
        key_id: "ephemeral".into(),
    };
    let seed = zeroize::Zeroizing::new([42u8; 32]);
    let mut grant = ImportGrant {
        scope: scope.clone(),
        public_key: hex::encode(
            ed25519_dalek::SigningKey::from_bytes(&seed)
                .verifying_key()
                .to_bytes(),
        ),
        sender_workspace_id: snapshot.workspace_id.clone(),
        exclude: vec![],
    };
    let envelope = federation::export_snapshot(
        &sender.store,
        &snapshot,
        &ExportScope {
            scope,
            ttl_ms: 60000,
        },
        &seed,
        &deadline,
    )
    .unwrap();
    let accepted = federation::import_snapshot(&receiver, envelope, &grant, &deadline).unwrap();
    grant.exclude.push("private.rs".into());
    assert!(
        federation::read(
            &receiver,
            accepted["remoteId"].as_str().unwrap(),
            "private.rs",
            &grant,
            &deadline
        )
        .is_err()
    );
    let orphan = receiver.store.blob(b"unpublished private fixture").unwrap();
    for id in [&snapshot.files[0].hash, &orphan] {
        assert_eq!(
            receiver
                .invoke("ci_read", json!({"artifactId":id}))
                .unwrap()["status"],
            "error",
            "Unregistered foreign/unpublished CAS source was exposed"
        );
        let input = format!(
            "{}\n",
            json!({"jsonrpc":"2.0","id":1,"method":"resources/read","params":{"uri":format!("ci://artifact/{id}")}})
        );
        let mut output = Vec::new();
        yoke::protocol::serve(&mut receiver, std::io::Cursor::new(input), &mut output).unwrap();
        let reply: serde_json::Value = serde_json::from_slice(&output).unwrap();
        assert!(
            reply.get("error").is_some(),
            "MCP resource exposed denied source: {reply}"
        );
    }
}
