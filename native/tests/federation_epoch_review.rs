use ed25519_dalek::SigningKey;
use serde_json::{Value, json};
use yoke::{
    Engine,
    analysis::AnalyzerEpoch,
    capture::Deadline,
    federation::{self, ExportScope, ImportGrant, Scope},
    model::{Graph, hash},
};

fn rejects_resealed_facts(epoch: AnalyzerEpoch, retarget: bool) {
    let source = tempfile::tempdir().unwrap();
    let root = tempfile::tempdir().unwrap();
    let d = Deadline::new(30000);
    std::fs::write(
        source.path().join("main.rs"),
        "fn target(){}\nfn other(){}\nfn caller(){target();}\n",
    )
    .unwrap();
    let mut sender = Engine::open(source.path()).unwrap();
    let receiver = Engine::open(root.path()).unwrap();
    let (snapshot, _, _, _) = sender.index(&d).unwrap();
    let secret = [43; 32];
    let scope = Scope {
        tenant_id: "review".into(),
        project_id: "origin".into(),
        sender_id: "sender".into(),
        key_id: "key".into(),
    };
    let grant = ImportGrant {
        scope: scope.clone(),
        public_key: hex::encode(SigningKey::from_bytes(&secret).verifying_key().to_bytes()),
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
        &secret,
        &d,
    )
    .unwrap();
    let imported = federation::import_snapshot_at(epoch, &receiver, envelope, &grant, &d).unwrap();
    let id = imported["remoteId"].as_str().unwrap();
    let mut graph = federation::graph(&receiver, id, &grant, &d).unwrap();
    if retarget {
        let other = graph
            .entities
            .iter()
            .find(|node| node.name == "other")
            .unwrap()
            .id
            .clone();
        let call = graph
            .observations
            .iter_mut()
            .find(|edge| edge.relation == "calls")
            .unwrap();
        assert_ne!(call.target.as_deref(), Some(other.as_str()));
        call.target = Some(other);
    } else {
        let node = graph
            .entities
            .iter_mut()
            .find(|node| node.name == "target")
            .unwrap();
        node.name = "invented".into();
        node.qualified_name = "invented".into();
    }
    let raw = serde_json::to_string(&graph).unwrap();
    let kind = match epoch {
        AnalyzerEpoch::V8 => "native_federation_receipt_v1",
        AnalyzerEpoch::V9 => "native_federation_receipt_v2",
    };
    let receipt: String = receiver
        .store
        .db
        .query_row(
            "SELECT value FROM records WHERE kind=?1 AND id=?2",
            rusqlite::params![kind, id],
            |r| r.get(0),
        )
        .unwrap();
    let mut receipt: Value = serde_json::from_str(&receipt).unwrap();
    if epoch == AnalyzerEpoch::V8 {
        receipt["graphHash"] = json!(hash(raw.as_bytes()));
    } else {
        receipt["receipt"]["graphHash"] = json!(hash(raw.as_bytes()));
    }
    receiver
        .store
        .db
        .execute(
            "UPDATE records SET value=?3 WHERE kind=?1 AND id=?2",
            rusqlite::params!["native_federation_graph_v1", id, raw],
        )
        .unwrap();
    receiver
        .store
        .db
        .execute(
            "UPDATE records SET value=?3 WHERE kind=?1 AND id=?2",
            rusqlite::params![kind, id, serde_json::to_string(&receipt).unwrap()],
        )
        .unwrap();
    // The sender signature, original source blobs and exact manifest have not
    // changed. A self-consistent local graph hash is no source-origin proof.
    let loaded: Result<Graph, _> = federation::graph(&receiver, id, &grant, &d);
    let found = federation::find(
        &receiver,
        id,
        &grant,
        &json!({"kind":"symbol","query":"invented"}),
        &d,
    );
    assert!(
        loaded.is_err() && found.is_err(),
        "Resealed {:?} graph accepted invented facts: retarget={retarget}, graphAccepted={}, findAccepted={}",
        epoch,
        loaded.is_ok(),
        found.is_ok()
    );
}
#[test]
fn v1_public_graph_rejects_resealed_invented_label() {
    rejects_resealed_facts(AnalyzerEpoch::V8, false);
}
#[test]
fn v2_public_graph_rejects_resealed_invented_label() {
    rejects_resealed_facts(AnalyzerEpoch::V9, false);
}
#[test]
fn v1_public_graph_rejects_resealed_retargeted_call() {
    rejects_resealed_facts(AnalyzerEpoch::V8, true);
}
#[test]
fn v2_public_graph_rejects_resealed_retargeted_call() {
    rejects_resealed_facts(AnalyzerEpoch::V9, true);
}
