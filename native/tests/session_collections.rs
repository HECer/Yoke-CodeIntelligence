use serde_json::json;
#[path = "support/registry_coordination.rs"]
mod coordination;
use std::{collections::BTreeMap, fs};
use yoke::{
    Engine,
    capture::Deadline,
    graph_collections::Registry,
    project_session::{HostCollection, HostProfile, Session},
};
fn host(root: &std::path::Path, write: bool) -> HostProfile {
    HostProfile {
        collections: vec![HostCollection {
            name: "library".into(),
            registry_root: root.into(),
            allowed_members: vec!["archived".into(), "next".into()],
            allow_registration: write,
            allow_registry_coordination: cfg!(unix),
            receiver_project: None,
        }],
        ..Default::default()
    }
}
fn bytes(root: &std::path::Path) -> BTreeMap<String, Vec<u8>> {
    fs::read_dir(root)
        .unwrap()
        .map(|entry| {
            let e = entry.unwrap();
            (
                e.file_name().to_string_lossy().into_owned(),
                fs::read(e.path()).unwrap(),
            )
        })
        .collect()
}
#[test]
fn empty_collections_preserve_legacy_host_serialization() {
    let value = serde_json::to_value(HostProfile::default()).unwrap();
    assert!(value.get("collections").is_none());
    let old: HostProfile = serde_json::from_value(value.clone()).unwrap();
    assert_eq!(serde_json::to_value(old).unwrap(), value);
}
#[test]
fn native_archived_queries_never_open_original_or_active_source() {
    let original = tempfile::tempdir().unwrap();
    let central = tempfile::tempdir().unwrap();
    let active = tempfile::tempdir().unwrap();
    let d = Deadline::new(30000);
    fs::write(original.path().join("main.rs"), "fn historic(){}\n").unwrap();
    let mut engine = Engine::open(original.path()).unwrap();
    let (s, g, _, _) = engine.index(&d).unwrap();
    let registry = Registry::open(central.path()).unwrap();
    registry
        .register("archived", &engine.store, &s, &g, &d)
        .unwrap();
    drop(registry);
    drop(engine);
    original.close().unwrap();
    let before = bytes(central.path());
    let mut session = Session::open(
        Engine::open(active.path()).unwrap(),
        host(central.path(), false),
        Some("archive-reader"),
        &d,
    )
    .unwrap();
    let configuration = session.invoke("configuration", &json!({}), &d).unwrap();
    let list = session
        .invoke("collections.list", &json!({"collection":"library"}), &d)
        .unwrap();
    assert_eq!(list["items"].as_array().unwrap().len(), 1);
    assert_eq!(
        session
            .invoke(
                "collections.describe",
                &json!({"collection":"library","member":"archived"}),
                &d
            )
            .unwrap()["kind"],
        "native"
    );
    let found=session.invoke("collections.query",&json!({"collection":"library","members":["archived"],"arguments":{"operation":"find","query":"historic","kind":"function"}}),&d).unwrap();
    assert_eq!(found["items"][0]["name"], "historic");
    let read=session.invoke("collections.read",&json!({"collection":"library","member":"archived","path":"main.rs","startByte":0,"endByte":16}),&d).unwrap();
    assert_eq!(read["text"], "fn historic(){}\n");
    assert_eq!(
        session.invoke("configuration", &json!({}), &d).unwrap(),
        configuration
    );
    let snapshots: i64 = session
        .engine
        .store
        .db
        .query_row("SELECT COUNT(*) FROM snapshots", [], |r| r.get(0))
        .unwrap();
    assert_eq!(snapshots, 0);
    coordination::assert_read_footprint(central.path(), before);
    for args in [
        json!({"collection":"missing"}),
        json!({"collection":"library","registryRoot":active.path()}),
        json!({"collection":"library","allowRegistryCoordination":true}),
        json!({"collection":"library","member":"unregistered"}),
    ] {
        assert!(session.invoke("collections.describe", &args, &d).is_err());
    }
}
#[test]
fn registration_requires_host_write_authority_and_explicit_binding_expectation() {
    let central = tempfile::tempdir().unwrap();
    let active = tempfile::tempdir().unwrap();
    let d = Deadline::new(30000);
    fs::write(active.path().join("main.rs"), "fn current(){}\n").unwrap();
    let mut readonly = Session::open(
        Engine::open(active.path()).unwrap(),
        host(central.path(), false),
        Some("readonly"),
        &d,
    )
    .unwrap();
    let args = json!({"collection":"library","member":"next","kind":"native","project":"default","expectedRegistration":null});
    assert!(readonly.invoke("collections.register", &args, &d).is_err());
    assert!(bytes(central.path()).is_empty());
    drop(readonly);
    let mut session = Session::open(
        Engine::open(active.path()).unwrap(),
        host(central.path(), true),
        Some("writer"),
        &d,
    )
    .unwrap();
    let registered = session.invoke("collections.register", &args, &d).unwrap();
    let id = registered["registrationId"].as_str().unwrap();
    assert!(session.invoke("collections.register", &args, &d).is_err());
    assert!(session.invoke("collections.remove",&json!({"collection":"library","member":"next","expectedRegistration":"0".repeat(64)}),&d).is_err());
    session
        .invoke(
            "collections.remove",
            &json!({"collection":"library","member":"next","expectedRegistration":id}),
            &d,
        )
        .unwrap();
    assert_eq!(
        session
            .invoke("collections.list", &json!({"collection":"library"}), &d)
            .unwrap()["count"],
        0
    );
}
#[test]
fn native_registry_read_wrapper_uses_exact_archived_membership() {
    let source = tempfile::tempdir().unwrap();
    let central = tempfile::tempdir().unwrap();
    let d = Deadline::new(30000);
    fs::write(source.path().join("main.rs"), "fn archive(){}\n").unwrap();
    let mut engine = Engine::open(source.path()).unwrap();
    let (s, g, _, _) = engine.index(&d).unwrap();
    let registry = Registry::open(central.path()).unwrap();
    registry
        .register("archived", &engine.store, &s, &g, &d)
        .unwrap();
    let read = registry
        .read_source("archived", "main.rs", 0..15, &d)
        .unwrap();
    assert_eq!(read["text"], "fn archive(){}\n");
    assert!(
        registry
            .read_source("archived", "../outside", 0..0, &d)
            .is_err()
    );
}

#[test]
fn configured_query_only_project_can_register_current_checked_view_without_activation() {
    use yoke::project_session::HostProject;
    let source = tempfile::tempdir().unwrap();
    let central = tempfile::tempdir().unwrap();
    let active = tempfile::tempdir().unwrap();
    let d = Deadline::new(30000);
    fs::write(source.path().join("main.rs"), "fn viewed(){}\n").unwrap();
    let mut engine = Engine::open(source.path()).unwrap();
    let (s, g, _, _) = engine.index(&d).unwrap();
    let view = yoke::graph_view::build(&engine.store, &s, &g, &Default::default(), &d).unwrap();
    drop(engine);
    let mut profile = host(central.path(), true);
    profile.projects.push(HostProject {
        name: "reference".into(),
        root: source.path().into(),
        read_only: true,
        query_only: true,
    });
    let mut session = Session::open(
        Engine::open(active.path()).unwrap(),
        profile,
        Some("view-writer"),
        &d,
    )
    .unwrap();
    let registered=session.invoke("collections.register",&json!({"collection":"library","member":"next","kind":"view","project":"reference","viewId":view.id,"expectedRegistration":null}),&d).unwrap();
    assert_eq!(registered["kind"], "view");
    assert_eq!(
        session.invoke("configuration", &json!({}), &d).unwrap()["activeProject"],
        "default"
    );
    assert_eq!(
        session
            .invoke(
                "collections.describe",
                &json!({"collection":"library","member":"next"}),
                &d
            )
            .unwrap()["kind"],
        "view"
    );
    let joined = session
        .invoke(
            "collections.join",
            &json!({"collection":"library","members":["next"]}),
            &d,
        )
        .unwrap();
    assert!(
        joined["nodes"]
            .as_array()
            .unwrap()
            .iter()
            .any(|n| n["name"] == "viewed")
    );
    assert!(session.invoke("collections.register",&json!({"collection":"library","member":"archived","kind":"native","project":source.path(),"expectedRegistration":null}),&d).is_err());
    session
        .invoke("removeProject", &json!({"project":"reference"}), &d)
        .unwrap();
    assert!(session.invoke("collections.register",&json!({"collection":"library","member":"archived","kind":"view","project":"reference","viewId":view.id,"expectedRegistration":null}),&d).is_err());
}

#[test]
fn transient_current_foreign_grants_authorize_only_the_registered_receiver_and_call() {
    use ed25519_dalek::SigningKey;
    use yoke::{
        federation::{self, ExportScope, ImportGrant, Scope},
        project_session::CollectionInvocationAccess,
    };
    let source = tempfile::tempdir().unwrap();
    let receiver_root = tempfile::tempdir().unwrap();
    let central = tempfile::tempdir().unwrap();
    let d = Deadline::new(60000);
    fs::write(source.path().join("main.rs"), "fn foreign(){}\n").unwrap();
    let mut sender = Engine::open(source.path()).unwrap();
    let receiver = Engine::open(receiver_root.path()).unwrap();
    let (s, _, _, _) = sender.index(&d).unwrap();
    let secret = [63; 32];
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
    let cap = CollectionInvocationAccess::new("library", "default", &receiver, &grants).unwrap();
    let mut profile = host(central.path(), true);
    profile.collections[0].receiver_project = Some("default".into());
    let mut session = Session::open(
        Engine::open(receiver_root.path()).unwrap(),
        profile,
        Some("foreign"),
        &d,
    )
    .unwrap();
    let args = json!({"collection":"library","member":"next","kind":"federatedSources","project":"default","grantId":"live","remoteId":imported["remoteId"],"expectedRegistration":null});
    assert!(session.invoke("collections.register", &args, &d).is_err());
    assert!(bytes(central.path()).is_empty());
    let registered = session
        .invoke_collection_with_access("collections.register", &args, &cap, &d)
        .unwrap();
    assert_eq!(registered["kind"], "federatedSources");
    assert!(
        session
            .invoke(
                "collections.describe",
                &json!({"collection":"library","member":"next"}),
                &d
            )
            .is_err()
    );
    assert_eq!(
        session
            .invoke_collection_with_access(
                "collections.describe",
                &json!({"collection":"library","member":"next"}),
                &cap,
                &d
            )
            .unwrap()["nodeCount"],
        0
    );
    let read=session.invoke_collection_with_access("collections.read",&json!({"collection":"library","member":"next","path":"main.rs","startByte":0,"endByte":15}),&cap,&d).unwrap();
    assert_eq!(read["text"], "fn foreign(){}\n");
    let canonical = json!({"collection":"library","member":"archived","kind":"federatedCanonical","project":"default","grantId":"live","remoteId":imported["remoteId"],"expectedRegistration":null});
    let canonical = session
        .invoke_collection_with_access("collections.register", &canonical, &cap, &d)
        .unwrap();
    assert_eq!(canonical["kind"], "federatedCanonical");
    let found=session.invoke_collection_with_access("collections.query",&json!({"collection":"library","members":["archived"],"arguments":{"operation":"find","query":"foreign","kind":"function"}}),&cap,&d).unwrap();
    assert_eq!(found["items"][0]["name"], "foreign");
    let wrong_root = tempfile::tempdir().unwrap();
    let wrong = Engine::open(wrong_root.path()).unwrap();
    let mismatched =
        CollectionInvocationAccess::new("library", "default", &wrong, &grants).unwrap();
    assert!(
        session
            .invoke_collection_with_access(
                "collections.describe",
                &json!({"collection":"library","member":"next"}),
                &mismatched,
                &d
            )
            .is_err()
    );
    assert!(
        session
            .invoke_collection_with_access(
                "collections.describe",
                &json!({"collection":"library","member":"next","publicKey":"forged"}),
                &cap,
                &d
            )
            .is_err()
    );
    grants.clear();
    let revoked =
        CollectionInvocationAccess::new("library", "default", &receiver, &grants).unwrap();
    assert!(session.invoke_collection_with_access("collections.read",&json!({"collection":"library","member":"next","path":"main.rs","startByte":0,"endByte":15}),&revoked,&d).is_err());
    let row: String = session
        .engine
        .store
        .db
        .query_row(
            "SELECT value FROM records WHERE kind='native_agent_session_v1'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert!(!row.contains("live") && !row.contains("publicKey") && !row.contains("grantId"));
}

#[test]
fn collection_policy_changes_invalidate_resume_but_empty_policy_keeps_original_fingerprint() {
    let active = tempfile::tempdir().unwrap();
    let central = tempfile::tempdir().unwrap();
    let d = Deadline::new(30000);
    let mut engine = Engine::open(active.path()).unwrap();
    let profile = HostProfile::default();
    let expected = yoke::model::identity(&(
        &engine.store.workspace_id,
        Vec::<(String, std::path::PathBuf, bool, bool)>::new(),
        &profile.context,
        &profile.modes,
        &profile.global_notes_root,
        &profile.notes_policy.read_only_patterns,
        &profile.notes_policy.ignored_patterns,
        profile.notes_policy.writable,
        engine.allow_writes,
        &engine.capture_options,
        Vec::<(String, String, Vec<String>, bool)>::new(),
        &engine.server_configurations,
    ));
    let session = Session::open(engine, profile, Some("old-fingerprint"), &d).unwrap();
    let value: String = session
        .engine
        .store
        .db
        .query_row(
            "SELECT value FROM records WHERE kind='native_agent_session_v1'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&value).unwrap()["hostFingerprint"],
        expected
    );
    drop(session);
    engine = Engine::open(active.path()).unwrap();
    let profile = host(central.path(), false);
    let session = Session::open(engine, profile.clone(), Some("policy"), &d).unwrap();
    drop(session);
    let mut changed = profile;
    changed.collections[0].allow_registration = true;
    assert!(
        Session::open(
            Engine::open(active.path()).unwrap(),
            changed,
            Some("policy"),
            &d
        )
        .is_err()
    );
}

#[test]
fn registry_coordination_is_host_policy_and_defaults_preserve_existing_profile_bytes() {
    let active = tempfile::tempdir().unwrap();
    let central = tempfile::tempdir().unwrap();
    let d = Deadline::new(30000);
    let mut profile = host(central.path(), false);
    profile.collections[0].allow_registry_coordination = false;
    let legacy = serde_json::to_value(&profile).unwrap();
    assert!(
        legacy["collections"][0]
            .get("allowRegistryCoordination")
            .is_none()
    );
    let decoded: HostProfile = serde_json::from_value(legacy.clone()).unwrap();
    assert!(!decoded.collections[0].allow_registry_coordination);
    assert_eq!(serde_json::to_value(decoded).unwrap(), legacy);
    drop(
        Session::open(
            Engine::open(active.path()).unwrap(),
            profile.clone(),
            Some("coordination"),
            &d,
        )
        .unwrap(),
    );
    profile.collections[0].allow_registry_coordination = true;
    assert!(
        Session::open(
            Engine::open(active.path()).unwrap(),
            profile,
            Some("coordination"),
            &d
        )
        .is_err()
    );
}
