//! Public collection archives must outlive the original host input and Store.
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::fs;
use tempfile::TempDir;
use yoke::{
    Engine,
    capture::Deadline,
    graph_collections::Registry,
    graph_view::{self, BuildOptions, View},
    model::{Graph, Snapshot, hash, identity},
};

const SOURCE: &str = "// Ω e\u{301}\r\nfn entry(){ target(); target(); }\r\nfn target(){}\r\n";

// Match the released serialization order so the negative controls reseal the
// outer registration, rather than failing only its first integrity check.
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Input {
    artifact_id: String,
    payload: String,
}
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Archive {
    view: View,
    canonical_graph: Graph,
    inputs: Vec<Input>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    capture_scope: Option<Value>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Registration {
    name: String,
    snapshot: Snapshot,
    graph: Graph,
    archive: Archive,
}
struct Fixture {
    central: TempDir,
    registry: Registry,
    joined: Value,
    caller: String,
    view_id: String,
}
fn fixture() -> Fixture {
    let central = TempDir::new().unwrap();
    let project = TempDir::new().unwrap();
    let host = TempDir::new().unwrap();
    fs::write(project.path().join("main.rs"), SOURCE).unwrap();
    let d = Deadline::new(30000);
    let mut engine = Engine::open(project.path()).unwrap();
    let (snapshot, graph, _, _) = engine.index(&d).unwrap();
    let calls: Vec<_> = graph
        .observations
        .iter()
        .filter(|e| e.relation == "calls")
        .collect();
    assert_eq!(calls.len(), 2);
    let claims: Vec<_> = calls
        .iter()
        .map(|e| {
            json!({
                "observationId":e.id,"source":e.source,"target":e.target,
                "evidenceIds":e.evidence_ids,
                "attributes":{"confidence":"INFERRED","confidence_score":0.125,
                    "also_seen":[{"confidence":"INFERRED","confidence_score":0.0625}]}
            })
        })
        .collect();
    let input = host.path().join("selected.json");
    fs::write(
        &input,
        serde_json::to_vec(&json!({
            "schemaVersion":1,"snapshotId":snapshot.id,"workspaceId":snapshot.workspace_id,
            "baseGraphHash":identity(&graph),"claims":claims
        }))
        .unwrap(),
    )
    .unwrap();
    let receipt =
        graph_view::source_claims::import_file(&engine.store, &snapshot, &graph, &input, &d)
            .unwrap();
    let view = graph_view::build(
        &engine.store,
        &snapshot,
        &graph,
        &BuildOptions {
            structural_kinds: vec![],
            source_claim_overlay_ids: vec![receipt["artifactId"].as_str().unwrap().into()],
            ..Default::default()
        },
        &d,
    )
    .unwrap();
    let registry = Registry::open(central.path()).unwrap();
    registry
        .register_view("selected", &engine.store, &view, &d)
        .unwrap();
    let joined = registry.join(&["selected".into()], &d).unwrap();
    let caller = joined["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|n| n["name"] == "entry")
        .unwrap()["id"]
        .as_str()
        .unwrap()
        .to_owned();
    assert_eq!(joined["lineage"][0]["viewId"], view.id);
    assert_eq!(
        joined["observations"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|e| e["relation"] == "calls")
            .count(),
        2
    );
    let project_path = project.path().to_owned();
    let host_path = host.path().to_owned();
    drop(engine);
    project.close().unwrap();
    host.close().unwrap();
    assert!(!project_path.exists() && !host_path.exists());
    Fixture {
        central,
        registry,
        joined,
        caller,
        view_id: view.id,
    }
}
impl Fixture {
    fn db(&self) -> rusqlite::Connection {
        rusqlite::Connection::open(self.central.path().join("yoke-collection.sqlite")).unwrap()
    }
    fn registration(&self) -> (String, String) {
        self.db()
            .query_row(
                "SELECT id,payload FROM registrations WHERE name='selected'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap()
    }
}

#[test]
fn public_collection_replays_source_claim_view_after_entire_original_store_loss() {
    let f = fixture();
    let d = Deadline::new(30000);
    assert_eq!(f.registry.join(&["selected".into()], &d).unwrap(), f.joined);
    assert_eq!(f.joined["lineage"][0]["viewId"], f.view_id);
    let read = f
        .registry
        .query(
            &["selected".into()],
            &json!({
                "operation":"read","entityIds":[f.caller],"startByte":0,"endByte":SOURCE.len()
            }),
            &d,
        )
        .unwrap();
    assert_eq!(read["text"], SOURCE);
    assert_eq!(read["contentHash"], hash(SOURCE.as_bytes()));
    let archived: Registration = serde_json::from_str(&f.registration().1).unwrap();
    assert_eq!(archived.archive.inputs.len(), 1);
    assert_eq!(
        hash(archived.archive.inputs[0].payload.as_bytes()),
        archived.archive.inputs[0].artifact_id
    );
}

#[test]
fn public_collection_refuses_resealed_corrupt_or_missing_original_claim_input() {
    for mode in ["changed-original-bytes", "missing-input", "duplicate-input"] {
        let f = fixture();
        let (original_id, payload) = f.registration();
        let mut data: Registration = serde_json::from_str(&payload).unwrap();
        assert_eq!(
            identity(&data),
            original_id,
            "test mirror must preserve original serialization"
        );
        assert_eq!(data.archive.inputs.len(), 1);
        match mode {
            "changed-original-bytes" => data.archive.inputs[0].payload.push(' '),
            "missing-input" => data.archive.inputs.clear(),
            "duplicate-input" => data.archive.inputs.push(Input {
                artifact_id: data.archive.inputs[0].artifact_id.clone(),
                payload: data.archive.inputs[0].payload.clone(),
            }),
            _ => unreachable!(),
        }
        let resealed = (identity(&data), serde_json::to_string(&data).unwrap());
        f.db()
            .execute(
                "UPDATE registrations SET id=?1,payload=?2 WHERE name='selected'",
                rusqlite::params![resealed.0, resealed.1],
            )
            .unwrap();
        assert!(
            f.registry
                .join(&["selected".into()], &Deadline::new(30000))
                .is_err(),
            "accepted {mode}"
        );
        assert_eq!(
            f.registration(),
            resealed,
            "refusal must not repair or publish registration rows"
        );
        f.db()
            .execute(
                "UPDATE registrations SET id=?1,payload=?2 WHERE name='selected'",
                rusqlite::params![original_id, payload],
            )
            .unwrap();
        assert_eq!(
            f.registry
                .join(&["selected".into()], &Deadline::new(30000))
                .unwrap(),
            f.joined
        );
    }
}

#[test]
fn archived_source_claim_view_refuses_corrupt_captured_source_without_repair() {
    let f = fixture();
    let before = f.registration();
    let original: Vec<u8> = f
        .db()
        .query_row(
            "SELECT bytes FROM source_blobs WHERE hash=?1",
            [hash(SOURCE.as_bytes())],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(original, SOURCE.as_bytes());
    let corrupt = vec![b'x'; original.len()];
    f.db()
        .execute(
            "UPDATE source_blobs SET bytes=?1 WHERE hash=?2",
            rusqlite::params![corrupt, hash(SOURCE.as_bytes())],
        )
        .unwrap();
    assert!(
        f.registry
            .join(&["selected".into()], &Deadline::new(30000))
            .is_err()
    );
    assert_eq!(f.registration(), before);
    let retained: Vec<u8> = f
        .db()
        .query_row(
            "SELECT bytes FROM source_blobs WHERE hash=?1",
            [hash(SOURCE.as_bytes())],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(retained, corrupt);
    f.db()
        .execute(
            "UPDATE source_blobs SET bytes=?1 WHERE hash=?2",
            rusqlite::params![original, hash(SOURCE.as_bytes())],
        )
        .unwrap();
    assert_eq!(
        f.registry
            .join(&["selected".into()], &Deadline::new(30000))
            .unwrap(),
        f.joined
    );
}
