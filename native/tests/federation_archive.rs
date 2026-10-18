use ed25519_dalek::SigningKey;
use std::collections::BTreeMap;
use tempfile::TempDir;
use yoke::{
    Engine,
    capture::Deadline,
    federation::{self, ExportScope, ImportGrant, Scope},
    federation_archive::FederationArchiveAccess,
};

fn fixture() -> (TempDir, Engine, BTreeMap<String, ImportGrant>, String) {
    fixture_with_extra(false)
}

fn fixture_with_extra(extra: bool) -> (TempDir, Engine, BTreeMap<String, ImportGrant>, String) {
    let sender_root = TempDir::new().unwrap();
    std::fs::write(
        sender_root.path().join("main.rs"),
        "fn target() {}\nfn caller() { target(); }\n",
    )
    .unwrap();
    if extra {
        std::fs::write(sender_root.path().join("zz.rs"), "fn second() {}\n").unwrap();
    }
    let mut sender = Engine::open(sender_root.path()).unwrap();
    let deadline = Deadline::new(60_000);
    let (source, _, _, _) = sender.index(&deadline).unwrap();
    let receiver_root = TempDir::new().unwrap();
    let receiver = Engine::open(receiver_root.path()).unwrap();
    let secret = [71; 32];
    let scope = Scope {
        tenant_id: "t".into(),
        project_id: "p".into(),
        sender_id: "s".into(),
        key_id: "k".into(),
    };
    let grant = ImportGrant {
        scope: scope.clone(),
        public_key: hex::encode(SigningKey::from_bytes(&secret).verifying_key().to_bytes()),
        sender_workspace_id: source.workspace_id.clone(),
        exclude: vec![],
    };
    let envelope = federation::export_snapshot(
        &sender.store,
        &source,
        &ExportScope {
            scope,
            ttl_ms: 60_000,
        },
        &secret,
        &deadline,
    )
    .unwrap();
    let result = federation::import_snapshot(&receiver, envelope, &grant, &deadline).unwrap();
    // Sender and its store are already closed when the returned fixture is used.
    (
        receiver_root,
        receiver,
        BTreeMap::from([("source".into(), grant)]),
        result["remoteId"].as_str().unwrap().into(),
    )
}

#[test]
fn resealed_valid_range_graph_labels_and_endpoints_require_source_reconstruction() {
    let (_root, receiver, grants, id) = fixture();
    let deadline = Deadline::new(60_000);
    let access = FederationArchiveAccess::new(&receiver, &grants);
    let prepared = access.capture("source", &id, &deadline).unwrap();
    for mutation in ["label", "edge"] {
        let mut graph = prepared.graph().clone();
        if mutation == "label" {
            let target = graph
                .entities
                .iter_mut()
                .find(|entity| entity.name == "target")
                .unwrap();
            target.name = "invented_without_source".into();
            target.qualified_name = "invented_without_source".into();
        } else {
            let caller = graph
                .entities
                .iter()
                .find(|entity| entity.name == "caller")
                .unwrap()
                .id
                .clone();
            let edge = graph
                .observations
                .iter_mut()
                .find(|edge| edge.relation == "calls")
                .unwrap();
            edge.target = Some(caller);
        }
        let mut proof = serde_json::to_value(prepared.binding()).unwrap();
        proof["receipt"]["graphHash"] =
            yoke::model::hash(serde_json::to_vec(&graph).unwrap()).into();
        let binding = serde_json::from_value(proof).unwrap();
        assert!(
            access
                .authorize(
                    &binding,
                    prepared.snapshot(),
                    &graph,
                    &mut |file, _| Ok(prepared.sources()[&file.path].clone()),
                    &deadline
                )
                .is_err(),
            "Unsigned resealed {mutation} was accepted as canonical graph origin"
        );
    }
}

#[test]
fn every_current_receiver_path_is_preflighted_before_any_source_callback() {
    let (_root, receiver, mut grants, id) = fixture_with_extra(true);
    let deadline = Deadline::new(60_000);
    let prepared = FederationArchiveAccess::new(&receiver, &grants)
        .capture("source", &id, &deadline)
        .unwrap();
    grants
        .get_mut("source")
        .unwrap()
        .exclude
        .push("zz.rs".into());
    let mut reads = 0;
    assert!(
        FederationArchiveAccess::new(&receiver, &grants)
            .authorize(
                prepared.binding(),
                prepared.snapshot(),
                prepared.graph(),
                &mut |file, _| {
                    reads += 1;
                    Ok(prepared.sources()[&file.path].clone())
                },
                &deadline
            )
            .is_err()
    );
    assert_eq!(
        reads, 0,
        "An allowed first source was read before the later denied source policy was preflighted"
    );
}

#[test]
fn archive_source_survives_removed_receipt_rows_but_not_revoked_live_host_grants() {
    let (root, receiver, mut grants, id) = fixture();
    let deadline = Deadline::new(60_000);
    let prepared = FederationArchiveAccess::new(&receiver, &grants)
        .capture("source", &id, &deadline)
        .unwrap();
    let original = serde_json::to_vec(prepared.binding()).unwrap();
    assert_eq!(prepared.binding().grant_id(), "source");
    assert_eq!(
        prepared.binding().provenance(&deadline).unwrap()["receiverEpoch"],
        "not_recorded"
    );
    assert!(
        !String::from_utf8(original.clone())
            .unwrap()
            .contains("exclude")
    );
    receiver
        .store
        .db
        .execute("DELETE FROM records WHERE id=?1", [&id])
        .unwrap();
    let snapshot = prepared.snapshot().clone();
    let graph = prepared.graph().clone();
    let binding = prepared.binding().clone();
    let sources = prepared.sources().clone();
    drop(receiver);
    // Explicitly reestablish the original receiver workspace, rather than
    // substituting a different receiving root or treating a receipt as a grant.
    let receiver = Engine::open(root.path()).unwrap();
    let mut reads = 0;
    FederationArchiveAccess::new(&receiver, &grants)
        .authorize(
            &binding,
            &snapshot,
            &graph,
            &mut |file, _| {
                reads += 1;
                Ok(sources[&file.path].clone())
            },
            &deadline,
        )
        .unwrap();
    assert_eq!(reads, snapshot.files.len());
    let mut source_proof = serde_json::to_value(&binding).unwrap();
    source_proof["receipt"]["graphHash"] = "f".repeat(64).into();
    let source_binding = serde_json::from_value(source_proof).unwrap();
    FederationArchiveAccess::new(&receiver, &grants)
        .authorize_sources(
            &source_binding,
            &snapshot,
            &mut |file, _| Ok(sources[&file.path].clone()),
            &deadline,
        )
        .unwrap();
    grants.clear();
    let error = FederationArchiveAccess::new(&receiver, &grants)
        .authorize(
            &binding,
            &snapshot,
            &graph,
            &mut |_, _| panic!("revoked grants must reject before source reads"),
            &deadline,
        )
        .unwrap_err();
    assert!(error.to_string().contains("revoked"));
    assert!(
        FederationArchiveAccess::new(&receiver, &grants)
            .authorize_sources(
                &source_binding,
                &snapshot,
                &mut |_, _| panic!("revoked source-only grants must precede reads"),
                &deadline
            )
            .is_err()
    );
    assert_eq!(serde_json::to_vec(&binding).unwrap(), original);
}

#[test]
fn archive_rechecks_current_ignore_key_scope_sender_and_receiver_before_sources() {
    let (root, receiver, grants, id) = fixture();
    let deadline = Deadline::new(60_000);
    let prepared = FederationArchiveAccess::new(&receiver, &grants)
        .capture("source", &id, &deadline)
        .unwrap();
    for field in ["key", "scope", "sender", "exclude"] {
        let mut changed = grants.clone();
        let grant = changed.get_mut("source").unwrap();
        match field {
            "key" => grant.public_key = "a".repeat(64),
            "scope" => grant.scope.project_id = "foreign".into(),
            "sender" => grant.sender_workspace_id = "b".repeat(64),
            _ => grant.exclude.push("main.rs".into()),
        }
        assert!(
            FederationArchiveAccess::new(&receiver, &changed)
                .authorize(
                    prepared.binding(),
                    prepared.snapshot(),
                    prepared.graph(),
                    &mut |_, _| panic!("invalid live grant must precede source reads"),
                    &deadline
                )
                .is_err()
        );
    }
    std::fs::write(root.path().join(".gitignore"), "main.rs\n").unwrap();
    assert!(
        FederationArchiveAccess::new(&receiver, &grants)
            .authorize(
                prepared.binding(),
                prepared.snapshot(),
                prepared.graph(),
                &mut |_, _| panic!("new local ignores must precede source reads"),
                &deadline
            )
            .is_err()
    );
    let other_root = TempDir::new().unwrap();
    let other = Engine::open(other_root.path()).unwrap();
    assert!(
        FederationArchiveAccess::new(&other, &grants)
            .authorize(
                prepared.binding(),
                prepared.snapshot(),
                prepared.graph(),
                &mut |_, _| panic!("foreign receiver must precede source reads"),
                &deadline
            )
            .is_err()
    );
}

#[test]
fn archive_refuses_unknown_schema_source_changes_and_invalid_exact_ranges() {
    let (_root, receiver, grants, id) = fixture();
    let deadline = Deadline::new(60_000);
    let access = FederationArchiveAccess::new(&receiver, &grants);
    let prepared = access.capture("source", &id, &deadline).unwrap();
    let mut proof = serde_json::to_value(prepared.binding()).unwrap();
    proof["schemaVersion"] = 2.into();
    let future = serde_json::from_value(proof).unwrap();
    assert!(
        access
            .authorize(
                &future,
                prepared.snapshot(),
                prepared.graph(),
                &mut |_, _| panic!("future schema must precede sources"),
                &deadline
            )
            .is_err()
    );
    assert!(
        access
            .authorize(
                prepared.binding(),
                prepared.snapshot(),
                prepared.graph(),
                &mut |_, _| Ok(b"tampered".to_vec()),
                &deadline
            )
            .is_err()
    );
    let mut graph = prepared.graph().clone();
    graph.entities[0].end_byte = 999_999;
    // Receipt graph hash is original local provenance, not a substitute for
    // structural range checks after an untrusted archive has been resealed.
    let mut proof = serde_json::to_value(prepared.binding()).unwrap();
    proof["receipt"]["graphHash"] = yoke::model::hash(serde_json::to_vec(&graph).unwrap()).into();
    let resealed = serde_json::from_value(proof).unwrap();
    assert!(
        access
            .authorize(
                &resealed,
                prepared.snapshot(),
                &graph,
                &mut |file, _| Ok(prepared.sources()[&file.path].clone()),
                &deadline
            )
            .is_err()
    );
}

#[test]
fn unknown_preserved_receiver_classifier_never_grants_archived_source_access() {
    let (_root, receiver, grants, id) = fixture();
    let deadline = Deadline::new(60_000);
    let access = FederationArchiveAccess::new(&receiver, &grants);
    let prepared = access.capture("source", &id, &deadline).unwrap();
    let mut snapshot = prepared.snapshot().clone();
    snapshot.files[0].language = "unknown-legacy-classifier".into();
    let mut graph = prepared.graph().clone();
    for entity in &mut graph.entities {
        entity.language = snapshot.files[0].language.clone();
    }
    let mut proof = serde_json::to_value(prepared.binding()).unwrap();
    proof["receipt"]["snapshot"] = serde_json::to_value(&snapshot).unwrap();
    proof["receipt"]["graphHash"] = yoke::model::hash(serde_json::to_vec(&graph).unwrap()).into();
    let binding: yoke::federation_archive::FederationArchiveBinding =
        serde_json::from_value(proof).unwrap();
    assert_eq!(
        binding.provenance(&deadline).unwrap()["receiverClassifier"],
        "unknown_preserved"
    );
    assert_eq!(
        binding.provenance(&deadline).unwrap()["receiverEpoch"],
        "not_recorded"
    );
    let error = access
        .authorize(
            &binding,
            &snapshot,
            &graph,
            &mut |_, _| panic!("unknown classifier cannot authorize source"),
            &deadline,
        )
        .unwrap_err();
    assert!(error.to_string().contains("Historical"));
    let error = access
        .authorize_sources(
            &binding,
            &snapshot,
            &mut |_, _| panic!("unknown source classifier cannot authorize bytes"),
            &deadline,
        )
        .unwrap_err();
    assert!(error.to_string().contains("classifier"));
}
