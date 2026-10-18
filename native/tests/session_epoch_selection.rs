use serde_json::{Value, json};
use std::{collections::BTreeMap, fs, path::Path};
use tempfile::TempDir;
use yoke::{
    Engine,
    analysis::AnalyzerEpoch,
    capture::Deadline,
    model::identity,
    project_session::{HostCollection, HostProfile, HostProject, RECORD_KIND, Session},
};

fn source() -> TempDir {
    let root = tempfile::tempdir().unwrap();
    fs::write(
        root.path().join("main.ps1"),
        "function Invoke-Target {}\ninvoke-target\n",
    )
    .unwrap();
    root
}

fn engine(root: &Path, epoch: AnalyzerEpoch) -> Engine {
    let mut engine = Engine::open(root).unwrap();
    engine.capture_epoch = epoch;
    engine
}

fn session_bytes(engine: &Engine) -> Vec<(String, String)> {
    engine
        .store
        .db
        .prepare("SELECT id,value FROM records WHERE kind=?1 ORDER BY id")
        .unwrap()
        .query_map([RECORD_KIND], |row| Ok((row.get(0)?, row.get(1)?)))
        .unwrap()
        .map(Result::unwrap)
        .collect()
}

fn assert_v9(engine: &Engine, id: &str) {
    let snapshot = engine.store.snapshot(id).unwrap();
    assert_eq!(
        yoke::capture::analyzer_epoch_for_snapshot(&engine.store, &snapshot).unwrap(),
        AnalyzerEpoch::V9
    );
    let graph = engine.store.graph(id).unwrap();
    assert!(!graph.evidence.is_empty());
    assert!(
        graph
            .evidence
            .iter()
            .all(|row| row.producer == "yoke-tree-sitter-9")
    );
}

#[test]
fn default_v8_preserves_the_original_session_record_and_can_resume_saved_bytes() {
    // Fixed original-V8 constructor tuple, independently hashed before the
    // session epoch change. Both new creation and an old record must match.
    const ORIGINAL: &str = r#"{"id":"fef1b352603fd02de7283e926b7a3e1898d2e20c720f013b26e3ab64c512b886","label":"frozen8","workspaceId":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","hostFingerprint":"21933ca4764c45e0cdfa8e72721605fcf3516faee68a90f58c1db5e0139e62f9","activeProject":"default","context":"agent","modes":["interactive","editing"],"onboardedProjects":[],"removedProjects":[],"revision":0}"#;
    let d = Deadline::new(20000);
    for existing in [false, true] {
        let root = source();
        let mut engine = engine(root.path(), AnalyzerEpoch::V8);
        engine.store.workspace_id = "a".repeat(64);
        if existing {
            let record: Value = serde_json::from_str(ORIGINAL).unwrap();
            engine
                .store
                .db
                .execute(
                    "INSERT INTO records(kind,id,value) VALUES(?1,?2,?3)",
                    rusqlite::params![RECORD_KIND, record["id"].as_str().unwrap(), ORIGINAL],
                )
                .unwrap();
        }
        let session = Session::open(engine, HostProfile::default(), Some("frozen8"), &d).unwrap();
        assert_eq!(session_bytes(&session.engine)[0].1, ORIGINAL);
    }
}

#[test]
fn session_v9_propagates_to_default_registered_readonly_and_query_only_projects() {
    let root = source();
    let regular = source();
    let readonly = source();
    let query_only = source();
    let profile = HostProfile {
        projects: vec![
            HostProject {
                name: "regular".into(),
                root: regular.path().into(),
                read_only: false,
                query_only: false,
            },
            HostProject {
                name: "readonly".into(),
                root: readonly.path().into(),
                read_only: true,
                query_only: false,
            },
            HostProject {
                name: "query".into(),
                root: query_only.path().into(),
                read_only: true,
                query_only: true,
            },
        ],
        ..Default::default()
    };
    let d = Deadline::new(30000);
    let mut host_engine = engine(root.path(), AnalyzerEpoch::V9);
    host_engine.allow_writes = true;
    let mut session = Session::open(host_engine, profile, Some("projects9"), &d).unwrap();
    for project in ["default", "regular", "readonly"] {
        session
            .invoke("activate", &json!({"project":project}), &d)
            .unwrap();
        let active = session.active_engine_at(&d).unwrap();
        let snapshot = active.index(&d).unwrap().0;
        assert_v9(active, &snapshot.id);
        assert_eq!(active.allow_writes, project != "readonly");
    }
    assert!(
        session
            .invoke("edit.prepare", &json!({"edits":[]}), &d)
            .is_err()
    );
    assert!(
        session
            .invoke("activate", &json!({"project":"query"}), &d)
            .is_err()
    );
    let found = session
        .invoke(
            "queryProject",
            &json!({
                "project":"query", "tool":"ci_find", "arguments":{"query":"Invoke-Target"}
            }),
            &d,
        )
        .unwrap();
    assert_eq!(found["status"], "ok", "{found}");
    let query_engine = Engine::open(query_only.path()).unwrap();
    let id = query_engine.store.latest().unwrap().unwrap();
    assert_v9(&query_engine, &id);
    assert_eq!(
        session.invoke("configuration", &json!({}), &d).unwrap()["activeProject"],
        "readonly"
    );
}

#[test]
fn same_epoch_resume_keeps_project_selection_and_cross_epoch_resume_cannot_mutate_records() {
    let d = Deadline::new(30000);
    for original_epoch in [AnalyzerEpoch::V8, AnalyzerEpoch::V9] {
        let root = source();
        let secondary = source();
        let profile = HostProfile {
            projects: vec![HostProject {
                name: "secondary".into(),
                root: secondary.path().into(),
                read_only: true,
                query_only: false,
            }],
            ..Default::default()
        };
        let mut first = Session::open(
            engine(root.path(), original_epoch),
            profile.clone(),
            Some("resume"),
            &d,
        )
        .unwrap();
        first
            .invoke("activate", &json!({"project":"secondary"}), &d)
            .unwrap();
        let before = session_bytes(&first.engine);
        drop(first);
        let mut resumed = Session::open(
            engine(root.path(), original_epoch),
            profile.clone(),
            Some("resume"),
            &d,
        )
        .unwrap();
        assert_eq!(session_bytes(&resumed.engine), before);
        assert_eq!(
            resumed.invoke("configuration", &json!({}), &d).unwrap()["activeProject"],
            "secondary"
        );
        let active = resumed.active_engine_at(&d).unwrap();
        assert_eq!(active.capture_epoch, original_epoch);
        let snapshot = active.index(&d).unwrap().0;
        assert_eq!(
            yoke::capture::analyzer_epoch_for_snapshot(&active.store, &snapshot).unwrap(),
            original_epoch
        );
        drop(resumed);
        let other_epoch = if original_epoch == AnalyzerEpoch::V8 {
            AnalyzerEpoch::V9
        } else {
            AnalyzerEpoch::V8
        };
        assert!(
            Session::open(
                engine(root.path(), other_epoch),
                profile,
                Some("resume"),
                &d
            )
            .is_err(),
            "an existing session must reject a changed host extraction policy"
        );
        assert_eq!(session_bytes(&Engine::open(root.path()).unwrap()), before);
    }
}

#[test]
fn mutable_engine_handles_cannot_silently_change_a_pinned_session_epoch() {
    let root = source();
    let secondary = source();
    let profile = HostProfile {
        projects: vec![HostProject {
            name: "secondary".into(),
            root: secondary.path().into(),
            read_only: false,
            query_only: false,
        }],
        ..Default::default()
    };
    let d = Deadline::new(20000);
    let mut session = Session::open(
        engine(root.path(), AnalyzerEpoch::V9),
        profile,
        Some("pinned"),
        &d,
    )
    .unwrap();
    let before = session_bytes(&session.engine);
    session.engine.capture_epoch = AnalyzerEpoch::V8;
    assert!(
        session
            .invoke("symbols.find", &json!({"query":"Invoke-Target"}), &d)
            .is_err()
    );
    assert_eq!(session_bytes(&session.engine), before);
    session.engine.capture_epoch = AnalyzerEpoch::V9;
    session
        .invoke("activate", &json!({"project":"secondary"}), &d)
        .unwrap();
    let before = session_bytes(&session.engine);
    session.active_engine_at(&d).unwrap().capture_epoch = AnalyzerEpoch::V8;
    assert!(
        session
            .invoke("symbols.find", &json!({"query":"Invoke-Target"}), &d)
            .is_err()
    );
    assert_eq!(session_bytes(&session.engine), before);
}

#[test]
fn session_v9_reads_old_native_archives_without_recapture_or_registration_authority() {
    let original = source();
    let archive = tempfile::tempdir().unwrap();
    let active = source();
    let d = Deadline::new(30000);
    let mut old_engine = engine(original.path(), AnalyzerEpoch::V8);
    let (snapshot, graph, _, _) = old_engine.index(&d).unwrap();
    let registry = yoke::graph_collections::Registry::open(archive.path()).unwrap();
    registry
        .register("old8", &old_engine.store, &snapshot, &graph, &d)
        .unwrap();
    drop(registry);
    drop(old_engine);
    original.close().unwrap();
    let profile = HostProfile {
        collections: vec![HostCollection {
            name: "library".into(),
            registry_root: archive.path().into(),
            allowed_members: vec!["old8".into()],
            allow_registration: false,
            receiver_project: None,
        }],
        ..Default::default()
    };
    let mut session = Session::open(
        engine(active.path(), AnalyzerEpoch::V9),
        profile,
        Some("archive9"),
        &d,
    )
    .unwrap();
    let before = session_bytes(&session.engine);
    let found = session.invoke("collections.query", &json!({
        "collection":"library", "members":["old8"], "arguments":{"operation":"find", "query":"Invoke-Target", "kind":"function"}
    }), &d).unwrap();
    assert!(
        found["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|row| row["name"] == "Invoke-Target")
    );
    assert!(session.invoke("collections.register", &json!({
        "collection":"library", "member":"old8", "kind":"native", "project":"default", "expectedRegistration":null
    }), &d).is_err());
    let count: usize = session
        .engine
        .store
        .db
        .query_row("SELECT COUNT(*) FROM snapshots", [], |r| r.get(0))
        .unwrap();
    assert_eq!(count, 0, "archive reads must not index the active project");
    assert_eq!(session_bytes(&session.engine), before);
}

#[test]
fn session_v9_keeps_federation_receiver_receipts_and_current_grants_separate() {
    use ed25519_dalek::SigningKey;
    use yoke::federation::{self, ExportScope, ImportGrant, Scope};
    let sender_root = source();
    let receiver_root = tempfile::tempdir().unwrap();
    let archive = tempfile::tempdir().unwrap();
    let d = Deadline::new(30000);
    let mut sender = engine(sender_root.path(), AnalyzerEpoch::V8);
    let receiver = engine(receiver_root.path(), AnalyzerEpoch::V8);
    let (snapshot, _, _, _) = sender.index(&d).unwrap();
    let secret = [75; 32];
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
        &d,
    )
    .unwrap();
    let imported = federation::import_snapshot(&receiver, wire, &grant, &d).unwrap();
    let remote = imported["remoteId"].as_str().unwrap();
    let original_graph = identity(&federation::graph(&receiver, remote, &grant, &d).unwrap());
    let receipt: String = receiver
        .store
        .db
        .query_row(
            "SELECT value FROM records WHERE kind='native_federation_receipt_v1' AND id=?1",
            [remote],
            |r| r.get(0),
        )
        .unwrap();
    let profile = HostProfile {
        collections: vec![HostCollection {
            name: "library".into(),
            registry_root: archive.path().into(),
            allowed_members: vec!["foreign8".into()],
            allow_registration: true,
            receiver_project: Some("default".into()),
        }],
        ..Default::default()
    };
    let mut session = Session::open(
        engine(receiver_root.path(), AnalyzerEpoch::V9),
        profile,
        Some("foreign9"),
        &d,
    )
    .unwrap();
    let grants = BTreeMap::from([("live".into(), grant.clone())]);
    session.invoke_collection_with_grants("collections.register", &json!({
        "collection":"library", "member":"foreign8", "kind":"federatedCanonical", "project":"default",
        "grantId":"live", "remoteId":remote, "expectedRegistration":null
    }), &grants, &d).unwrap();
    let query = json!({"collection":"library", "members":["foreign8"],
        "arguments":{"operation":"find", "query":"Invoke-Target", "kind":"function"}});
    let found = session
        .invoke_collection_with_grants("collections.query", &query, &grants, &d)
        .unwrap();
    assert!(!found["items"].as_array().unwrap().is_empty());
    assert!(session.invoke("collections.query", &query, &d).is_err());
    assert!(
        session
            .invoke_collection_with_grants("collections.query", &query, &BTreeMap::new(), &d)
            .is_err()
    );
    assert_eq!(
        identity(&federation::graph(&session.engine, remote, &grant, &d).unwrap()),
        original_graph
    );
    let after: String = session
        .engine
        .store
        .db
        .query_row(
            "SELECT value FROM records WHERE kind='native_federation_receipt_v1' AND id=?1",
            [remote],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(after, receipt);
    assert!(!session_bytes(&session.engine)[0].1.contains("grantId"));
}
