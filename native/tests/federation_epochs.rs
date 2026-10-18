use ed25519_dalek::SigningKey;
use serde_json::{Value, json};
use std::{collections::BTreeMap, fs};
use yoke::{
    Engine,
    analysis::AnalyzerEpoch,
    capture::Deadline,
    federation::{self, ExportScope, ImportGrant, Scope},
    federation_archive::{FederationArchiveAccess, FederationArchiveBinding},
    model::{Graph, Snapshot, hash},
};

fn historical() -> Value {
    serde_json::from_str(include_str!(
        "fixtures/analyzer8/authentic-federation-receiver8.json"
    ))
    .unwrap()
}
fn envelope() -> (tempfile::TempDir, Engine, ImportGrant, Value) {
    let source = tempfile::tempdir().unwrap();
    fs::write(
        source.path().join("main.rs"),
        "fn target() {}\nfn caller() { let f = target; target(); }\n",
    )
    .unwrap();
    let mut sender = Engine::open(source.path()).unwrap();
    let deadline = Deadline::new(20000);
    let (snapshot, _, _, _) = sender.index(&deadline).unwrap();
    let scope = Scope {
        tenant_id: "migration".into(),
        project_id: "epoch".into(),
        sender_id: "sender".into(),
        key_id: "key".into(),
    };
    let secret = [54u8; 32];
    let grant = ImportGrant {
        scope: scope.clone(),
        public_key: hex::encode(SigningKey::from_bytes(&secret).verifying_key().to_bytes()),
        sender_workspace_id: snapshot.workspace_id.clone(),
        exclude: vec![],
    };
    let value = federation::export_snapshot(
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
    (source, sender, grant, value)
}

#[test]
fn authentic_receiver8_v1_receipt_graph_and_archived_provenance_stay_exact() {
    let data = historical();
    let root = tempfile::tempdir().unwrap();
    let mut receiver = Engine::open(root.path()).unwrap();
    receiver.store.workspace_id = data["receiverWorkspaceId"].as_str().unwrap().into();
    for row in data["records"].as_array().unwrap() {
        receiver
            .store
            .db
            .execute(
                "INSERT INTO records(kind,id,value) VALUES(?1,?2,?3)",
                rusqlite::params![
                    row["kind"].as_str().unwrap(),
                    row["id"].as_str().unwrap(),
                    row["value"].as_str().unwrap()
                ],
            )
            .unwrap();
    }
    let snapshot: Snapshot = serde_json::from_value(data["snapshot"].clone()).unwrap();
    let graph: Graph = serde_json::from_str(data["graphRaw"].as_str().unwrap()).unwrap();
    let binding: FederationArchiveBinding =
        serde_json::from_str(data["bindingRaw"].as_str().unwrap()).unwrap();
    let grant: ImportGrant = serde_json::from_value(data["grant"].clone()).unwrap();
    for file in &snapshot.files {
        let text = data["sources"][&file.path].as_str().unwrap();
        assert_eq!(receiver.store.blob(text.as_bytes()).unwrap(), file.hash);
    }
    let grants = BTreeMap::from([("source".to_owned(), grant)]);
    let deadline = Deadline::new(20000);
    let access = FederationArchiveAccess::new(&receiver, &grants);
    assert_eq!(
        serde_json::to_string(&binding).unwrap(),
        data["bindingRaw"].as_str().unwrap()
    );
    assert_eq!(
        serde_json::to_string(&binding.provenance(&deadline).unwrap()).unwrap(),
        data["provenanceRaw"].as_str().unwrap()
    );
    access
        .authorize(
            &binding,
            &snapshot,
            &graph,
            &mut |f, _| {
                Ok(data["sources"][&f.path]
                    .as_str()
                    .unwrap()
                    .as_bytes()
                    .to_vec())
            },
            &deadline,
        )
        .unwrap();
    let loaded = federation::graph(&receiver, &snapshot.id, &grants["source"], &deadline).unwrap();
    assert_eq!(
        serde_json::to_string(&loaded).unwrap(),
        data["graphRaw"].as_str().unwrap()
    );
    assert!(
        receiver.store.snapshot(&snapshot.id).is_err(),
        "foreign receipt never becomes native authority"
    );
}

#[test]
fn explicit_receiver9_creates_an_exact_v2_receipt_and_epoch9_source_graph() {
    let (_source, _sender, grant, envelope) = envelope();
    let root = tempfile::tempdir().unwrap();
    let receiver = Engine::open(root.path()).unwrap();
    let deadline = Deadline::new(20000);
    let result =
        federation::import_snapshot_at(AnalyzerEpoch::V9, &receiver, envelope, &grant, &deadline)
            .unwrap();
    let id = result["remoteId"].as_str().unwrap();
    let receipt: Option<String> = receiver
        .store
        .db
        .query_row(
            "SELECT value FROM records WHERE kind='native_federation_receipt_v2' AND id=?1",
            [id],
            |r| r.get(0),
        )
        .ok();
    assert!(
        receipt.is_some(),
        "new receiver epoch requires a separate exact V2 record"
    );
    let receipt: Value = serde_json::from_str(&receipt.unwrap()).unwrap();
    assert_eq!(receipt["schemaVersion"], 2);
    assert_eq!(receipt["receiverAnalyzer"], "yoke-tree-sitter-9");
    let graph = federation::graph(&receiver, id, &grant, &deadline).unwrap();
    assert!(
        graph
            .entities
            .iter()
            .all(|e| e.producer == "yoke-tree-sitter-9")
    );
    let grants = BTreeMap::from([("source".to_owned(), grant)]);
    let prepared = FederationArchiveAccess::new(&receiver, &grants)
        .capture("source", id, &deadline)
        .unwrap();
    assert_eq!(
        prepared.binding().provenance(&deadline).unwrap()["receiverEpoch"],
        "yoke-tree-sitter-9"
    );
}

#[test]
fn retrying_an_existing_v1_envelope_with_receiver9_preserves_old_id_graph_and_receipt() {
    let (_source, _sender, grant, envelope) = envelope();
    let root = tempfile::tempdir().unwrap();
    let receiver = Engine::open(root.path()).unwrap();
    let deadline = Deadline::new(20000);
    let first =
        federation::import_snapshot(&receiver, envelope.clone(), &grant, &deadline).unwrap();
    let id = first["remoteId"].as_str().unwrap();
    let original: String = receiver
        .store
        .db
        .query_row(
            "SELECT value FROM records WHERE kind='native_federation_receipt_v1' AND id=?1",
            [id],
            |r| r.get(0),
        )
        .unwrap();
    let graph = federation::graph(&receiver, id, &grant, &deadline).unwrap();
    let again =
        federation::import_snapshot_at(AnalyzerEpoch::V9, &receiver, envelope, &grant, &deadline)
            .unwrap();
    assert_eq!(again["remoteId"], id);
    assert_eq!(again["idempotent"], true);
    let retained: String = receiver
        .store
        .db
        .query_row(
            "SELECT value FROM records WHERE kind='native_federation_receipt_v1' AND id=?1",
            [id],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(retained, original);
    assert_eq!(
        hash(
            serde_json::to_vec(&federation::graph(&receiver, id, &grant, &deadline).unwrap())
                .unwrap()
        ),
        hash(serde_json::to_vec(&graph).unwrap())
    );
    let new_count: usize = receiver
        .store
        .db
        .query_row(
            "SELECT count(*) FROM records WHERE kind='native_federation_receipt_v2' AND id=?1",
            [id],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(new_count, 0);
}

#[test]
fn unknown_receipt_epochs_and_added_v1_epoch_fields_fail_before_source_callbacks() {
    let data = historical();
    let root = tempfile::tempdir().unwrap();
    let mut receiver = Engine::open(root.path()).unwrap();
    receiver.store.workspace_id = data["receiverWorkspaceId"].as_str().unwrap().into();
    let snapshot: Snapshot = serde_json::from_value(data["snapshot"].clone()).unwrap();
    let graph: Graph = serde_json::from_str(data["graphRaw"].as_str().unwrap()).unwrap();
    let grant: ImportGrant = serde_json::from_value(data["grant"].clone()).unwrap();
    let grants = BTreeMap::from([("source".to_owned(), grant)]);
    let original: Value = serde_json::from_str(data["bindingRaw"].as_str().unwrap()).unwrap();
    let mut widened_v1 = original.clone();
    widened_v1["receipt"]["receiverAnalyzer"] = json!("yoke-tree-sitter-9");
    let mut future_v2 = original.clone();
    future_v2["receipt"] = json!({"schemaVersion":2,"receiverAnalyzer":"yoke-tree-sitter-10","receipt":original["receipt"]});
    for value in [widened_v1, future_v2] {
        let binding: FederationArchiveBinding = serde_json::from_value(value).unwrap();
        let mut reads = 0;
        assert!(
            FederationArchiveAccess::new(&receiver, &grants)
                .authorize(
                    &binding,
                    &snapshot,
                    &graph,
                    &mut |_, _| {
                        reads += 1;
                        Ok(vec![])
                    },
                    &Deadline::new(20000)
                )
                .is_err()
        );
        assert_eq!(reads, 0);
    }
}

#[test]
fn declared_v2_epoch_cannot_be_downgraded_under_the_same_foreign_identity() {
    let (_source, _sender, grant, envelope) = envelope();
    let root = tempfile::tempdir().unwrap();
    let receiver = Engine::open(root.path()).unwrap();
    let deadline = Deadline::new(20000);
    let imported =
        federation::import_snapshot_at(AnalyzerEpoch::V9, &receiver, envelope, &grant, &deadline)
            .unwrap();
    let id = imported["remoteId"].as_str().unwrap();
    let raw: String = receiver
        .store
        .db
        .query_row(
            "SELECT value FROM records WHERE kind='native_federation_receipt_v2' AND id=?1",
            [id],
            |r| r.get(0),
        )
        .unwrap();
    let mut receipt: Value = serde_json::from_str(&raw).unwrap();
    receipt["receiverAnalyzer"] = json!("yoke-tree-sitter-8");
    let snapshot = federation::snapshot(&receiver, id, &grant, &deadline).unwrap();
    let binding: FederationArchiveBinding =
        serde_json::from_value(json!({"schemaVersion":1,"grantId":"source","receipt":receipt}))
            .unwrap();
    let grants = BTreeMap::from([("source".to_owned(), grant)]);
    let mut reads = 0;
    let result = FederationArchiveAccess::new(&receiver, &grants).authorize_sources(
        &binding,
        &snapshot,
        &mut |_, _| {
            reads += 1;
            Ok(b"fn target() {}\nfn caller() { let f = target; target(); }\n".to_vec())
        },
        &deadline,
    );
    assert!(
        result.is_err(),
        "declared receiver epoch was mutable without changing the foreign source identity"
    );
    assert_eq!(
        reads, 0,
        "epoch/identity mismatch must fail before any source callback"
    );
}

#[test]
fn exact_v2_receipt_is_recognized_by_retention_without_an_opaque_kind_allowance() {
    let (_source, _sender, grant, envelope) = envelope();
    let root = tempfile::tempdir().unwrap();
    let receiver = Engine::open(root.path()).unwrap();
    let deadline = Deadline::new(20000);
    federation::import_snapshot_at(AnalyzerEpoch::V9, &receiver, envelope, &grant, &deadline)
        .unwrap();
    let plan = yoke::retention::plan(
        &receiver.store,
        &yoke::retention::Policy::default(),
        &deadline,
    )
    .unwrap();
    assert_eq!(
        plan["unknownDependencyKinds"],
        json!([]),
        "exact V2 typed receipt must participate in retention preflight"
    );
}

#[test]
fn signed_v1_source_namespace_is_preserved_by_the_same_typed_retention_preflight() {
    let (_source, _sender, grant, envelope) = envelope();
    let root = tempfile::tempdir().unwrap();
    let receiver = Engine::open(root.path()).unwrap();
    let deadline = Deadline::new(20000);
    federation::import_snapshot(&receiver, envelope, &grant, &deadline).unwrap();
    let plan = yoke::retention::plan(
        &receiver.store,
        &yoke::retention::Policy::default(),
        &deadline,
    )
    .unwrap();
    assert_eq!(plan["unknownDependencyKinds"], json!([]));
}
