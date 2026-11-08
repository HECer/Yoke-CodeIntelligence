use ed25519_dalek::SigningKey;
use serde_json::Value;
use tempfile::TempDir;
use yoke::{
    Engine,
    capture::Deadline,
    federation::{self, ExportScope, ImportGrant, Scope},
};

#[test]
fn released4_receiver_labels_survive_a_newer_sender_without_receipt_rewriting() {
    let sender_root = TempDir::new().unwrap();
    std::fs::write(
        sender_root.path().join("component.vue"),
        "<template>Hello</template>\n",
    )
    .unwrap();
    std::fs::write(
        sender_root.path().join("script.ps1"),
        "Write-Output 'Hello'\n",
    )
    .unwrap();
    let mut sender = Engine::open(sender_root.path()).unwrap();
    let deadline = Deadline::new(20_000);
    let (source, _, _, _) = sender.index(&deadline).unwrap();
    let receiver_root = TempDir::new().unwrap();
    let receiver = Engine::open(receiver_root.path()).unwrap();
    let secret = [43; 32];
    let scope = Scope {
        tenant_id: "tenant".into(),
        project_id: "project".into(),
        sender_id: "sender".into(),
        key_id: "key".into(),
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
    assert_eq!(envelope["manifest"]["analyzer"], "yoke-tree-sitter-8");
    let imported = federation::import_snapshot(&receiver, envelope, &grant, &deadline).unwrap();
    let id = imported["remoteId"].as_str().unwrap();
    let raw: String = receiver
        .store
        .db
        .query_row(
            "SELECT value FROM records WHERE kind='native_federation_receipt_v1' AND id=?1",
            [id],
            |row| row.get(0),
        )
        .unwrap();
    let mut receipt: Value = serde_json::from_str(&raw).unwrap();
    // The original v1 importer did not record its receiver analyzer. Released4
    // classified both paths as text, independently of the signed sender analyzer.
    for file in receipt["snapshot"]["files"].as_array_mut().unwrap() {
        file["language"] = "text".into();
    }
    let historical_snapshot: yoke::model::Snapshot =
        serde_json::from_value(receipt["snapshot"].clone()).unwrap();
    let extractions = historical_snapshot
        .files
        .iter()
        .map(|file| {
            yoke::analysis::extract(
                file,
                &std::fs::read_to_string(sender_root.path().join(&file.path)).unwrap(),
                &deadline,
            )
            .unwrap()
        })
        .collect();
    let mut graph = yoke::analysis::assemble(&historical_snapshot, extractions, &deadline).unwrap();
    for entity in &mut graph.entities {
        entity.producer = "yoke-tree-sitter-4".into();
        entity.signature = std::fs::read_to_string(sender_root.path().join(&entity.path))
            .unwrap()
            .lines()
            .next()
            .unwrap()
            .into();
    }
    let graph_raw = serde_json::to_string(&graph).unwrap();
    receipt["graphHash"] = yoke::model::hash(&graph_raw).into();
    receiver
        .store
        .db
        .execute(
            "UPDATE records SET value=?1 WHERE kind='native_federation_graph_v1' AND id=?2",
            rusqlite::params![graph_raw, id],
        )
        .unwrap();
    let historical = serde_json::to_string(&receipt).unwrap();
    receiver
        .store
        .db
        .execute(
            "UPDATE records SET value=?1 WHERE kind='native_federation_receipt_v1' AND id=?2",
            rusqlite::params![historical, id],
        )
        .unwrap();
    let snapshot = federation::snapshot(&receiver, id, &grant, &deadline).unwrap();
    assert!(snapshot.files.iter().all(|file| file.language == "text"));
    assert_eq!(
        federation::read(&receiver, id, "script.ps1", &grant, &deadline).unwrap()["text"],
        "Write-Output 'Hello'\n"
    );
    let stored: String = receiver
        .store
        .db
        .query_row(
            "SELECT value FROM records WHERE kind='native_federation_receipt_v1' AND id=?1",
            [id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(stored, historical);
    let grants = std::collections::BTreeMap::from([("source".into(), grant)]);
    let access = yoke::federation_archive::FederationArchiveAccess::new(&receiver, &grants);
    let prepared = access.capture_sources("source", id, &deadline).unwrap();
    assert!(
        prepared
            .snapshot()
            .files
            .iter()
            .all(|file| file.language == "text")
    );
    assert_eq!(
        prepared.binding().provenance(&deadline).unwrap()["receiverEpoch"],
        "not_recorded"
    );
    assert!(
        access.capture("source", id, &deadline).is_err(),
        "Historical local graph is archival data, not current source reconstruction"
    );
    let stored_graph: String = receiver
        .store
        .db
        .query_row(
            "SELECT value FROM records WHERE kind='native_federation_graph_v1' AND id=?1",
            [id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(stored_graph, graph_raw);
}
