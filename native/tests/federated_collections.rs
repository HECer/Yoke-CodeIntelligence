use ed25519_dalek::SigningKey;
use serde_json::json;
use std::collections::BTreeMap;
use yoke::{
    Engine,
    capture::Deadline,
    federation::{self, ExportScope, ImportGrant, Scope},
    federation_archive::FederationArchiveAccess,
    graph_collections::Registry,
};

fn fixture() -> (
    tempfile::TempDir,
    Engine,
    BTreeMap<String, ImportGrant>,
    String,
) {
    let sender_root = tempfile::tempdir().unwrap();
    std::fs::write(
        sender_root.path().join("main.rs"),
        "fn target() {}\nfn caller(){ target(); target(); }\n",
    )
    .unwrap();
    let mut sender = Engine::open(sender_root.path()).unwrap();
    let deadline = Deadline::new(60000);
    let (snapshot, _, _, _) = sender.index(&deadline).unwrap();
    let root = tempfile::tempdir().unwrap();
    let receiver = Engine::open(root.path()).unwrap();
    let secret = [39; 32];
    let scope = Scope {
        tenant_id: "tenant".into(),
        project_id: "project".into(),
        sender_id: "sender".into(),
        key_id: "key".into(),
    };
    let grant = ImportGrant {
        scope: scope.clone(),
        public_key: hex::encode(SigningKey::from_bytes(&secret).verifying_key().to_bytes()),
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
        &secret,
        &deadline,
    )
    .unwrap();
    let imported = federation::import_snapshot(&receiver, wire, &grant, &deadline).unwrap();
    (
        root,
        receiver,
        BTreeMap::from([("live".into(), grant)]),
        imported["remoteId"].as_str().unwrap().into(),
    )
}

#[test]
fn archived_foreign_collection_requires_current_host_context_for_every_operation() {
    let (_root, receiver, mut grants, remote) = fixture();
    let registry_root = tempfile::tempdir().unwrap();
    let registry = Registry::open(registry_root.path()).unwrap();
    let deadline = Deadline::new(60000);
    let names = vec!["foreign".into()];
    let access = FederationArchiveAccess::new(&receiver, &grants);
    let prepared = access.capture("live", &remote, &deadline).unwrap();
    assert!(
        registry
            .register(
                "bypass",
                &receiver.store,
                prepared.snapshot(),
                prepared.graph(),
                &deadline
            )
            .is_err()
    );
    registry
        .register_federated("foreign", &access, "live", &remote, &deadline)
        .unwrap();
    assert!(registry.join(&names, &deadline).is_err());
    assert!(
        registry
            .query(
                &names,
                &json!({"operation":"find","query":"target"}),
                &deadline
            )
            .is_err()
    );
    let joined = registry
        .join_authorized(&names, &access, &deadline)
        .unwrap();
    assert_eq!(
        joined["observations"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|edge| edge["relation"] == "calls")
            .count(),
        2
    );
    let found = registry
        .query_authorized(
            &names,
            &json!({"operation":"find","query":"target"}),
            &access,
            &deadline,
        )
        .unwrap();
    let id = found["items"][0]["id"].clone();
    let read = registry
        .query_authorized(
            &names,
            &json!({"operation":"read","entityIds":[id]}),
            &access,
            &deadline,
        )
        .unwrap();
    assert!(read["text"].as_str().unwrap().contains("fn target"));
    grants.clear();
    let revoked = FederationArchiveAccess::new(&receiver, &grants);
    assert!(
        registry
            .join_authorized(&names, &revoked, &deadline)
            .is_err()
    );
    assert!(
        registry
            .query_authorized(
                &names,
                &json!({"operation":"find","query":"target"}),
                &revoked,
                &deadline
            )
            .is_err()
    );
    assert!(
        registry
            .query_authorized(
                &names,
                &json!({"operation":"read","entityIds":[id]}),
                &revoked,
                &deadline
            )
            .is_err()
    );
}

#[test]
fn unknown_authority_kinds_and_nested_fields_are_not_silently_accepted() {
    let (_root, receiver, grants, remote) = fixture();
    let registry_root = tempfile::tempdir().unwrap();
    let registry = Registry::open(registry_root.path()).unwrap();
    let deadline = Deadline::new(60000);
    let access = FederationArchiveAccess::new(&receiver, &grants);
    registry
        .register_federated("foreign", &access, "live", &remote, &deadline)
        .unwrap();
    let db =
        rusqlite::Connection::open(registry_root.path().join("yoke-collection.sqlite")).unwrap();
    let original: String = db
        .query_row(
            "SELECT payload FROM registrations WHERE name='foreign'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    for mutation in ["kind", "field", "binding"] {
        let mut value: serde_json::Value = serde_json::from_str(&original).unwrap();
        match mutation {
            "kind" => value["sourceAuthority"]["kind"] = json!("futureAuthority"),
            "field" => value["sourceAuthority"]["allowExecute"] = json!(true),
            _ => value["sourceAuthority"]["binding"]["grant"] = json!({"allowAll":true}),
        }
        db.execute(
            "UPDATE registrations SET payload=?1 WHERE name='foreign'",
            [value.to_string()],
        )
        .unwrap();
        let error = registry
            .join_authorized(&["foreign".into()], &access, &deadline)
            .unwrap_err()
            .to_string();
        assert!(
            error.contains("unknown"),
            "Expected typed-schema rejection before integrity lookup: {error}"
        );
    }
}

#[test]
fn archived_sources_outlive_original_receipts_but_not_changed_receiver_policy() {
    let (root, receiver, mut grants, remote) = fixture();
    let registry_root = tempfile::tempdir().unwrap();
    let registry = Registry::open(registry_root.path()).unwrap();
    let deadline = Deadline::new(60000);
    let names = vec!["foreign".into()];
    registry
        .register_federated(
            "foreign",
            &FederationArchiveAccess::new(&receiver, &grants),
            "live",
            &remote,
            &deadline,
        )
        .unwrap();
    let captured = FederationArchiveAccess::new(&receiver, &grants)
        .capture("live", &remote, &deadline)
        .unwrap();
    for file in &captured.snapshot().files {
        std::fs::remove_file(receiver.store.dir.join("blobs").join(&file.hash)).unwrap();
    }
    receiver
        .store
        .db
        .execute("DELETE FROM records WHERE id=?1", [&remote])
        .unwrap();
    drop(receiver);
    let receiver = Engine::open(root.path()).unwrap();
    let access = FederationArchiveAccess::new(&receiver, &grants);
    let found = registry
        .query_authorized(
            &names,
            &json!({"operation":"find","query":"target"}),
            &access,
            &deadline,
        )
        .unwrap();
    let id = found["items"][0]["id"].clone();
    assert!(
        registry
            .query_authorized(
                &names,
                &json!({"operation":"read","entityIds":[id]}),
                &access,
                &deadline
            )
            .is_ok()
    );
    let other_root = tempfile::tempdir().unwrap();
    let other = Engine::open(other_root.path()).unwrap();
    assert!(
        registry
            .join_authorized(
                &names,
                &FederationArchiveAccess::new(&other, &grants),
                &deadline
            )
            .is_err()
    );
    grants
        .get_mut("live")
        .unwrap()
        .exclude
        .push("main.rs".into());
    assert!(
        registry
            .query_authorized(
                &names,
                &json!({"operation":"read","entityIds":[id]}),
                &FederationArchiveAccess::new(&receiver, &grants),
                &deadline
            )
            .is_err()
    );
}

#[test]
fn source_only_archive_does_not_invent_canonical_graph_authority() {
    let (_root, receiver, mut grants, remote) = fixture();
    let registry_root = tempfile::tempdir().unwrap();
    let registry = Registry::open(registry_root.path()).unwrap();
    let deadline = Deadline::new(60000);
    let access = FederationArchiveAccess::new(&receiver, &grants);
    registry
        .register_federated_sources("sources", &access, "live", &remote, &deadline)
        .unwrap();
    let joined = registry
        .join_authorized(&["sources".into()], &access, &deadline)
        .unwrap();
    assert!(joined["nodes"].as_array().unwrap().is_empty());
    assert!(joined["observations"].as_array().unwrap().is_empty());
    assert_eq!(
        joined["lineage"][0]["sourceAuthority"]["kind"],
        "federatedSources"
    );
    let read = registry
        .read_source_authorized("sources", "main.rs", 0..15, &access, &deadline)
        .unwrap();
    assert_eq!(read["text"], "fn target() {}\n");
    grants.clear();
    assert!(
        registry
            .read_source_authorized(
                "sources",
                "main.rs",
                0..15,
                &FederationArchiveAccess::new(&receiver, &grants),
                &deadline
            )
            .is_err()
    );
}
