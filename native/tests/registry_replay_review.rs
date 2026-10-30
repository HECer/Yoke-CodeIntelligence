//! Archive canonical claims must be rederived, even when every unsigned seal agrees.
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::fs;
use tempfile::TempDir;
use yoke::{
    Engine,
    capture::Deadline,
    graph_collections::Registry,
    graph_view,
    model::{Graph, Snapshot, hash, identity},
};

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Input {
    artifact_id: String,
    payload: String,
}
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Archive {
    view: graph_view::View,
    canonical_graph: Graph,
    inputs: Vec<Input>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    capture_scope: Option<Value>,
}
#[derive(Serialize, Deserialize)]
struct Registration {
    name: String,
    snapshot: Snapshot,
    graph: Graph,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    archive: Option<Archive>,
    #[serde(
        rename = "captureScope",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    capture_scope: Option<Value>,
    #[serde(
        rename = "sourceAuthority",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    source_authority: Option<Value>,
}

fn mutate(graph: &mut Graph, relabel: bool) -> String {
    if relabel {
        let entity = graph
            .entities
            .iter_mut()
            .find(|e| e.name == "callee")
            .unwrap();
        entity.name = "invented".into();
        entity.qualified_name = "invented".into();
        entity.id.clone()
    } else {
        let target = graph
            .entities
            .iter()
            .find(|e| e.name == "other")
            .unwrap()
            .id
            .clone();
        let arc = graph
            .observations
            .iter_mut()
            .find(|o| o.relation == "calls" && o.target.is_some())
            .unwrap();
        arc.target = Some(target);
        arc.id.clone()
    }
}
fn fixture(view: bool, relabel: Option<bool>) -> (TempDir, Deadline) {
    let project = TempDir::new().unwrap();
    let central = TempDir::new().unwrap();
    fs::write(
        project.path().join("main.rs"),
        "fn callee(){}\nfn other(){}\nfn caller(){callee();callee();}\n",
    )
    .unwrap();
    let deadline = Deadline::new(30000);
    let mut engine = Engine::open(project.path()).unwrap();
    let (snapshot, graph, _, _) = engine.index(&deadline).unwrap();
    let registry = Registry::open(central.path()).unwrap();
    if view {
        let selected = graph_view::build(
            &engine.store,
            &snapshot,
            &graph,
            &graph_view::BuildOptions {
                structural_kinds: vec![],
                ..Default::default()
            },
            &deadline,
        )
        .unwrap();
        registry
            .register_view("member", &engine.store, &selected, &deadline)
            .unwrap();
    } else {
        registry
            .register("member", &engine.store, &snapshot, &graph, &deadline)
            .unwrap();
    }
    drop(registry);
    drop(engine);
    project.close().unwrap();
    if let Some(relabel) = relabel {
        let db = rusqlite::Connection::open(central.path().join("yoke-collection.sqlite")).unwrap();
        let payload: String = db
            .query_row(
                "SELECT payload FROM registrations WHERE name='member'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        let mut data: Registration = serde_json::from_str(&payload).unwrap();
        // Confirm the test preserves the original serialization and identity before tampering.
        let original: String = db
            .query_row(
                "SELECT id FROM registrations WHERE name='member'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(identity(&data), original);
        let changed = mutate(&mut data.graph, relabel);
        if let Some(archive) = &mut data.archive {
            assert_eq!(mutate(&mut archive.canonical_graph, relabel), changed);
            if relabel {
                let row = archive
                    .view
                    .nodes
                    .iter_mut()
                    .find(|row| row["id"] == changed)
                    .unwrap();
                row["name"] = json!("invented");
                row["qualifiedName"] = json!("invented");
            } else {
                let target = data
                    .graph
                    .observations
                    .iter()
                    .find(|arc| arc.id == changed)
                    .unwrap()
                    .target
                    .as_ref()
                    .unwrap();
                let row = archive
                    .view
                    .observations
                    .iter_mut()
                    .find(|row| row["id"] == changed)
                    .unwrap();
                row["target"] = json!(target);
            }
            archive.view.base_graph_hash = identity(&archive.canonical_graph);
            archive.view.id.clear();
            archive.view.artifact_id.clear();
            archive.view.id = identity(&archive.view);
            archive.view.artifact_id = hash(serde_json::to_vec(&archive.view).unwrap());
        }
        db.execute(
            "UPDATE registrations SET id=?1,payload=?2 WHERE name='member'",
            rusqlite::params![identity(&data), serde_json::to_string(&data).unwrap()],
        )
        .unwrap();
    }
    (central, deadline)
}
fn reject(view: bool, relabel: bool) {
    let (central, deadline) = fixture(view, Some(relabel));
    let registry = Registry::open_existing_for_host(
        central.path(),
        cfg!(unix),
        &yoke::capture::Deadline::new(30_000),
    )
    .unwrap();
    assert!(
        registry.join(&["member".into()], &deadline).is_err(),
        "accepted fully resealed view={view} relabel={relabel}"
    );
    assert!(registry.describe("member", &deadline).is_err());
    assert!(
        registry
            .read_source("member", "main.rs", 0..14, &deadline)
            .is_err()
    );
    assert!(
        registry
            .query(
                &["member".into()],
                &json!({"operation":"find", "query":"invented"}),
                &deadline
            )
            .is_err()
    );
}
#[test]
fn native8_relabel_is_rejected_after_origin_deleted() {
    reject(false, true);
}
#[test]
fn native8_retarget_is_rejected_after_origin_deleted() {
    reject(false, false);
}
#[test]
fn view8_relabel_is_rejected_after_origin_deleted() {
    reject(true, true);
}
#[test]
fn view8_retarget_is_rejected_after_origin_deleted() {
    reject(true, false);
}
#[test]
fn unchanged_native8_and_view8_remain_standalone() {
    for view in [false, true] {
        let (central, deadline) = fixture(view, None);
        let registry = Registry::open_existing_for_host(
            central.path(),
            cfg!(unix),
            &yoke::capture::Deadline::new(30_000),
        )
        .unwrap();
        let joined = registry.join(&["member".into()], &deadline).unwrap();
        assert_eq!(
            joined["observations"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|arc| arc["relation"] == "calls")
                .count(),
            2
        );
        assert!(
            joined["nodes"]
                .as_array()
                .unwrap()
                .iter()
                .any(|node| node["name"] == "callee")
        );
        let read = registry
            .read_source("member", "main.rs", 0..14, &deadline)
            .unwrap();
        assert_eq!(read["text"], "fn callee(){}\n");
    }
}
#[test]
fn native9_archive_selects_proven_epoch_without_global_activation() {
    let (central, deadline) = fixture(false, None);
    let db = rusqlite::Connection::open(central.path().join("yoke-collection.sqlite")).unwrap();
    let payload: String = db
        .query_row(
            "SELECT payload FROM registrations WHERE name='member'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    let mut data: Registration = serde_json::from_str(&payload).unwrap();
    data.snapshot.id = identity(&(
        &data.snapshot.workspace_id,
        "disk",
        "yoke-tree-sitter-9",
        &data.snapshot.files,
        &data.snapshot.omissions,
    ));
    let extraction = yoke::analysis::extract_at(
        yoke::analysis::AnalyzerEpoch::V9,
        &data.snapshot.files[0],
        "fn callee(){}\nfn other(){}\nfn caller(){callee();callee();}\n",
        &deadline,
    )
    .unwrap();
    data.graph = yoke::analysis::assemble_at(
        yoke::analysis::AnalyzerEpoch::V9,
        &data.snapshot,
        vec![extraction],
        &deadline,
    )
    .unwrap();
    data.capture_scope.as_mut().unwrap()["nodes"][0]["snapshot"] =
        serde_json::to_value(&data.snapshot).unwrap();
    db.execute(
        "UPDATE registrations SET id=?1,payload=?2 WHERE name='member'",
        rusqlite::params![identity(&data), serde_json::to_string(&data).unwrap()],
    )
    .unwrap();
    drop(db);
    let registry = Registry::open_existing_for_host(
        central.path(),
        cfg!(unix),
        &yoke::capture::Deadline::new(30_000),
    )
    .unwrap();
    let joined = registry.join(&["member".into()], &deadline).unwrap();
    assert!(
        joined["nodes"]
            .as_array()
            .unwrap()
            .iter()
            .any(|node| node["name"] == "callee")
    );
    assert_eq!(
        registry.describe("member", &deadline).unwrap()["canonicalReplay"]["analyzer"],
        "yoke-tree-sitter-9"
    );
}

#[test]
fn historical4_keeps_its_identity_and_reports_unreplayed_claims() {
    let (central, deadline) = fixture(false, None);
    let db = rusqlite::Connection::open(central.path().join("yoke-collection.sqlite")).unwrap();
    let payload: String = db
        .query_row(
            "SELECT payload FROM registrations WHERE name='member'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    let mut data: Registration = serde_json::from_str(&payload).unwrap();
    data.snapshot.id = identity(&(
        &data.snapshot.workspace_id,
        "disk",
        "yoke-tree-sitter-4",
        &data.snapshot.files,
        &data.snapshot.omissions,
    ));
    // Synthetic historical compatibility shape, not an original epoch4 parser result.
    let extraction = yoke::analysis::extract_at(
        yoke::analysis::AnalyzerEpoch::V8,
        &data.snapshot.files[0],
        "fn callee(){}\nfn other(){}\nfn caller(){callee();callee();}\n",
        &deadline,
    )
    .unwrap();
    data.graph = yoke::analysis::assemble_at(
        yoke::analysis::AnalyzerEpoch::V8,
        &data.snapshot,
        vec![extraction],
        &deadline,
    )
    .unwrap();
    data.capture_scope = None;
    let id = identity(&data);
    db.execute(
        "UPDATE registrations SET id=?1,payload=?2 WHERE name='member'",
        rusqlite::params![id, serde_json::to_string(&data).unwrap()],
    )
    .unwrap();
    drop(db);
    let registry = Registry::open_existing_for_host(
        central.path(),
        cfg!(unix),
        &yoke::capture::Deadline::new(30_000),
    )
    .unwrap();
    let metadata = registry.describe("member", &deadline).unwrap();
    assert_eq!(metadata["registrationId"], id);
    assert_eq!(
        metadata["canonicalReplay"],
        json!({"status":"historical_host_registered_unreplayed","analyzer":null})
    );
    assert_eq!(
        registry.join(&["member".into()], &deadline).unwrap()["lineage"][0]["canonicalReplay"],
        metadata["canonicalReplay"]
    );
}

#[test]
fn producer_labels_cannot_change_a_proven_capture_epoch() {
    for producer in ["yoke-tree-sitter-9", "unknown-analyzer"] {
        let (central, deadline) = fixture(false, None);
        let db = rusqlite::Connection::open(central.path().join("yoke-collection.sqlite")).unwrap();
        let payload: String = db
            .query_row(
                "SELECT payload FROM registrations WHERE name='member'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        let mut data: Registration = serde_json::from_str(&payload).unwrap();
        for entity in &mut data.graph.entities {
            entity.producer = producer.into();
        }
        for observation in &mut data.graph.observations {
            observation.producer = producer.into();
        }
        for evidence in &mut data.graph.evidence {
            evidence.producer = producer.into();
        }
        db.execute(
            "UPDATE registrations SET id=?1,payload=?2 WHERE name='member'",
            rusqlite::params![identity(&data), serde_json::to_string(&data).unwrap()],
        )
        .unwrap();
        drop(db);
        let registry = Registry::open_existing_for_host(
            central.path(),
            cfg!(unix),
            &yoke::capture::Deadline::new(30_000),
        )
        .unwrap();
        assert!(
            registry.join(&["member".into()], &deadline).is_err(),
            "graph label {producer} selected a replay policy"
        );
    }
}
