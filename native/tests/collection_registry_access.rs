use std::{collections::BTreeMap, fs};
use yoke::{Engine, capture::Deadline, graph_collections::Registry};
fn files(root: &std::path::Path) -> BTreeMap<String, Vec<u8>> {
    fs::read_dir(root)
        .unwrap()
        .map(|entry| {
            let entry = entry.unwrap();
            (
                entry.file_name().to_string_lossy().into_owned(),
                fs::read(entry.path()).unwrap(),
            )
        })
        .collect()
}
#[test]
fn unknown_registry_open_does_not_create_state() {
    let root = tempfile::tempdir().unwrap();
    let missing = root.path().join("missing");
    assert!(Registry::open_existing(&missing).is_err());
    assert!(!missing.exists());
    assert!(Registry::open_existing(root.path()).is_err());
    assert!(files(root.path()).is_empty());
}

#[cfg(windows)]
#[test]
fn readonly_guard_excludes_independent_writers_and_refuses_uncheckpointed_wal() {
    let root = tempfile::tempdir().unwrap();
    let registry = Registry::open(root.path()).unwrap();
    let before = files(root.path());
    assert!(
        Registry::open_existing(root.path()).is_err(),
        "An already-open SQLite writer must prevent immutable admission"
    );
    assert_eq!(files(root.path()), before);
    // Preserve an actual SQLite-generated uncheckpointed transaction in an
    // independent directory, rather than inventing a synthetic WAL header.
    let writer = rusqlite::Connection::open(root.path().join("yoke-collection.sqlite")).unwrap();
    writer.execute_batch("PRAGMA user_version=17").unwrap();
    let crash = tempfile::tempdir().unwrap();
    let pending = files(root.path());
    assert!(pending["yoke-collection.sqlite-wal"].len() > 32);
    for (name, bytes) in &pending {
        fs::write(crash.path().join(name), bytes).unwrap();
    }
    assert!(
        Registry::open_existing(crash.path()).is_err(),
        "Immutable access must never ignore actual committed WAL data"
    );
    assert_eq!(files(crash.path()), pending);
    drop(writer);
    drop(registry);
    let before = files(root.path());
    let reader = Registry::open_existing(root.path()).unwrap();
    let attempted = rusqlite::Connection::open(root.path().join("yoke-collection.sqlite"));
    if let Ok(writer) = attempted {
        assert!(writer.execute_batch("PRAGMA user_version=99").is_err());
    }
    drop(reader);
    assert_eq!(files(root.path()), before);
}
#[test]
fn read_only_native_archive_survives_original_source_deletion_without_writes() {
    let source = tempfile::tempdir().unwrap();
    let central = tempfile::tempdir().unwrap();
    fs::write(
        source.path().join("main.rs"),
        "fn archive(){ target(); target(); }\nfn target(){}\n",
    )
    .unwrap();
    let mut engine = Engine::open(source.path()).unwrap();
    let d = Deadline::new(30000);
    let (s, g, _, _) = engine.index(&d).unwrap();
    let registry = Registry::open(central.path()).unwrap();
    let id = registry
        .register("native", &engine.store, &s, &g, &d)
        .unwrap();
    drop(registry);
    drop(engine);
    source.close().unwrap();
    let before = files(central.path());
    let registry = Registry::open_existing(central.path()).unwrap();
    assert_eq!(registry.list(&d).unwrap()["items"][0]["registrationId"], id);
    let metadata = registry.describe("native", &d).unwrap();
    assert_eq!(metadata["kind"], "native");
    assert_eq!(metadata["snapshotId"], s.id);
    let found = registry
        .query(
            &["native".into()],
            &serde_json::json!({"operation":"find","query":"archive","kind":"function"}),
            &d,
        )
        .unwrap();
    let read = registry
        .query(
            &["native".into()],
            &serde_json::json!({"operation":"read","entityIds":[found["items"][0]["id"]]}),
            &d,
        )
        .unwrap();
    assert!(read["text"].as_str().unwrap().contains("fn archive"));
    assert!(registry.remove("native", &id, &d).is_err());
    drop(registry);
    assert_eq!(files(central.path()), before);
}
#[test]
fn stale_and_absent_cas_never_replace_existing_binding() {
    let source = tempfile::tempdir().unwrap();
    let central = tempfile::tempdir().unwrap();
    fs::write(source.path().join("main.rs"), "fn old(){}\n").unwrap();
    let mut engine = Engine::open(source.path()).unwrap();
    let d = Deadline::new(30000);
    let (s, g, _, _) = engine.index(&d).unwrap();
    let registry = Registry::open(central.path()).unwrap();
    let first = registry
        .register_cas("project", &engine.store, &s, &g, None, &d)
        .unwrap();
    fs::write(source.path().join("main.rs"), "fn replacement(){}\n").unwrap();
    let (s2, g2, _, _) = engine.index(&d).unwrap();
    let before = registry.join(&["project".into()], &d).unwrap();
    assert!(
        registry
            .register_cas("project", &engine.store, &s2, &g2, None, &d)
            .is_err()
    );
    assert!(
        registry
            .register_cas(
                "project",
                &engine.store,
                &s2,
                &g2,
                Some(&"0".repeat(64)),
                &d
            )
            .is_err()
    );
    assert_eq!(registry.join(&["project".into()], &d).unwrap(), before);
    let second = registry
        .register_cas("project", &engine.store, &s2, &g2, Some(&first), &d)
        .unwrap();
    assert_ne!(first, second);
    assert_eq!(
        registry.describe("project", &d).unwrap()["registrationId"],
        second
    );
    let independent = Registry::open(central.path()).unwrap();
    assert!(
        independent
            .register_cas("project", &engine.store, &s, &g, Some(&first), &d)
            .is_err()
    );
    let view = yoke::graph_view::build(&engine.store, &s2, &g2, &Default::default(), &d).unwrap();
    assert!(
        registry
            .register_view_cas("project", &engine.store, &view, None, &d)
            .is_err()
    );
    let viewed = registry
        .register_view_cas("project", &engine.store, &view, Some(&second), &d)
        .unwrap();
    assert_ne!(viewed, second);
    assert_eq!(independent.describe("project", &d).unwrap()["kind"], "view");
}

#[test]
fn native_and_checked_view_metadata_keep_distinct_kinds_and_reject_unsafe_legacy_presentation() {
    let source = tempfile::tempdir().unwrap();
    let central = tempfile::tempdir().unwrap();
    fs::write(source.path().join("main.rs"), "fn checked(){}\n").unwrap();
    let mut engine = Engine::open(source.path()).unwrap();
    let d = Deadline::new(30000);
    let (s, g, _, _) = engine.index(&d).unwrap();
    let registry = Registry::open(central.path()).unwrap();
    registry
        .register("native", &engine.store, &s, &g, &d)
        .unwrap();
    let view = yoke::graph_view::build(&engine.store, &s, &g, &Default::default(), &d).unwrap();
    registry
        .register_view("view", &engine.store, &view, &d)
        .unwrap();
    assert_eq!(registry.describe("view", &d).unwrap()["kind"], "view");
    assert_eq!(
        registry.list(&d).unwrap()["items"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    let db = rusqlite::Connection::open(central.path().join("yoke-collection.sqlite")).unwrap();
    let payload: String = db
        .query_row(
            "SELECT payload FROM registrations WHERE name='native'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    let mut value: serde_json::Value = serde_json::from_str(&payload).unwrap();
    let nodes = value["graph"]["entities"].as_array_mut().unwrap();
    nodes
        .iter_mut()
        .find(|node| node["kind"] == "file")
        .unwrap()["signature"] = serde_json::json!("PASSWORD = PRIVATE_OLD_VALUE");
    // Native historical payloads have no optional sourceAuthority/archive fields.
    // Preserve typed registration field ordering when resealing this corruption.
    #[derive(serde::Serialize, serde::Deserialize)]
    struct Native {
        name: String,
        snapshot: yoke::model::Snapshot,
        graph: yoke::model::Graph,
        #[serde(rename = "captureScope")]
        capture_scope: serde_json::Value,
    }
    let forged: Native = serde_json::from_value(value).unwrap();
    db.execute(
        "UPDATE registrations SET id=?1,payload=?2 WHERE name='native'",
        rusqlite::params![
            yoke::model::identity(&forged),
            serde_json::to_string(&forged).unwrap()
        ],
    )
    .unwrap();
    assert!(registry.describe("native", &d).is_err());
    assert!(registry.join(&["native".into()], &d).is_err());
}

#[test]
fn foreign_metadata_requires_fresh_current_grants_for_each_inspection() {
    use ed25519_dalek::SigningKey;
    use yoke::{
        federation::{self, ExportScope, ImportGrant, Scope},
        federation_archive::FederationArchiveAccess,
    };
    let source = tempfile::tempdir().unwrap();
    let receiver_root = tempfile::tempdir().unwrap();
    let central = tempfile::tempdir().unwrap();
    fs::write(source.path().join("main.rs"), "fn foreign(){}\n").unwrap();
    let mut sender = Engine::open(source.path()).unwrap();
    let receiver = Engine::open(receiver_root.path()).unwrap();
    let d = Deadline::new(60000);
    let (s, _, _, _) = sender.index(&d).unwrap();
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
        sender_workspace_id: s.workspace_id.clone(),
        exclude: vec![],
    };
    let wire = federation::export_snapshot(
        &sender.store,
        &s,
        &ExportScope {
            scope,
            ttl_ms: 60000,
        },
        &secret,
        &d,
    )
    .unwrap();
    let imported = federation::import_snapshot(&receiver, wire, &grant, &d).unwrap();
    let mut grants = BTreeMap::from([("live".into(), grant)]);
    let access = FederationArchiveAccess::new(&receiver, &grants);
    let registry = Registry::open(central.path()).unwrap();
    registry
        .register_federated(
            "canonical",
            &access,
            "live",
            imported["remoteId"].as_str().unwrap(),
            &d,
        )
        .unwrap();
    registry
        .register_federated_sources(
            "sources",
            &access,
            "live",
            imported["remoteId"].as_str().unwrap(),
            &d,
        )
        .unwrap();
    assert!(registry.list(&d).is_err());
    assert!(registry.describe("canonical", &d).is_err());
    assert_eq!(
        registry
            .describe_authorized("canonical", &access, &d)
            .unwrap()["kind"],
        "federatedCanonical"
    );
    assert_eq!(
        registry
            .describe_authorized("sources", &access, &d)
            .unwrap()["kind"],
        "federatedSources"
    );
    assert_eq!(
        registry.list_authorized(&access, &d).unwrap()["items"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    let prior = registry
        .describe_authorized("canonical", &access, &d)
        .unwrap()["registrationId"]
        .as_str()
        .unwrap()
        .to_owned();
    assert!(
        registry
            .register_federated_cas(
                "canonical",
                &access,
                "live",
                imported["remoteId"].as_str().unwrap(),
                None,
                &d
            )
            .is_err()
    );
    let next = registry
        .register_federated_sources_cas(
            "canonical",
            &access,
            "live",
            imported["remoteId"].as_str().unwrap(),
            Some(&prior),
            &d,
        )
        .unwrap();
    assert_ne!(prior, next);
    assert_eq!(
        registry
            .describe_authorized("canonical", &access, &d)
            .unwrap()["kind"],
        "federatedSources"
    );
    grants.clear();
    let revoked = FederationArchiveAccess::new(&receiver, &grants);
    assert!(
        registry
            .describe_authorized("canonical", &revoked, &d)
            .is_err()
    );
    assert!(registry.list_authorized(&revoked, &d).is_err());
}

#[cfg(windows)]
#[test]
fn readonly_uri_preserves_unicode_spaces_and_literal_uri_characters() {
    let root = tempfile::tempdir().unwrap();
    let directory = root.path().join("registry Ω # %");
    fs::create_dir(&directory).unwrap();
    let writer = Registry::open(&directory).unwrap();
    drop(writer);
    let before = files(&directory);
    let reader = Registry::open_existing(&directory).unwrap();
    assert_eq!(reader.list(&Deadline::new(10000)).unwrap()["count"], 0);
    drop(reader);
    assert_eq!(files(&directory), before);
}
