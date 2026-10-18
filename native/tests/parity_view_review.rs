use serde_json::json;
use std::fs;
use tempfile::TempDir;
use yoke::{
    Engine,
    capture::Deadline,
    graph_view::{self, BuildOptions, View},
    model::identity,
};

#[test]
fn resealed_view_cannot_invent_canonical_labels_or_parent_claims() {
    let root = TempDir::new().unwrap();
    fs::write(
        root.path().join("main.rs"),
        "fn helper() {}\nfn caller(){helper();}\n",
    )
    .unwrap();
    let mut engine = Engine::open(root.path()).unwrap();
    let deadline = Deadline::new(20000);
    let (snapshot, graph, _, _) = engine.index(&deadline).unwrap();
    let mut view = graph_view::build(
        &engine.store,
        &snapshot,
        &graph,
        &BuildOptions {
            structural_kinds: vec![],
            ..Default::default()
        },
        &deadline,
    )
    .unwrap();
    let helper = view
        .nodes
        .iter_mut()
        .find(|node| node["name"] == "helper")
        .unwrap();
    helper["name"] = json!("invented-canonical-symbol");
    helper["qualifiedName"] = json!("invented-canonical-symbol");
    helper["parentKey"] = json!("invented-parent-key");
    publish_resealed(&engine, &mut view);
    let loaded = graph_view::load(&engine.store, &view.id, &deadline);
    assert!(
        loaded.is_err(),
        "a matching source hash and CAS hash do not prove canonical labels/parents: {:?}",
        loaded.map(|v| v.nodes)
    );
}

fn publish_resealed(engine: &Engine, view: &mut View) {
    view.id.clear();
    view.artifact_id.clear();
    view.id = identity(&*view);
    let artifact_id = engine
        .store
        .artifact(&serde_json::to_vec(view).unwrap())
        .unwrap();
    let record = json!({"viewId":view.id,"artifactId":artifact_id,"snapshotId":view.snapshot_id,"workspaceId":view.workspace_id,"baseGraphHash":view.base_graph_hash,"overlayIds":view.overlays.iter().map(|overlay|overlay["artifactId"].clone()).collect::<Vec<_>>()});
    engine
        .store
        .db
        .execute(
            "INSERT INTO records(kind,id,value) VALUES(?1,?2,?3)",
            rusqlite::params![graph_view::RECORD_KIND, view.id, record.to_string()],
        )
        .unwrap();
    view.artifact_id = artifact_id;
}

#[test]
fn resealed_view_cannot_remove_canonical_observations() {
    let root = TempDir::new().unwrap();
    fs::write(
        root.path().join("main.rs"),
        "fn helper() {}\nfn caller(){helper();helper();}\n",
    )
    .unwrap();
    let mut engine = Engine::open(root.path()).unwrap();
    let deadline = Deadline::new(20000);
    let (snapshot, graph, _, _) = engine.index(&deadline).unwrap();
    let mut view = graph_view::build(
        &engine.store,
        &snapshot,
        &graph,
        &BuildOptions::default(),
        &deadline,
    )
    .unwrap();
    let at = view
        .observations
        .iter()
        .position(|arc| arc["relation"] == "calls")
        .unwrap();
    view.observations.remove(at);
    publish_resealed(&engine, &mut view);
    assert!(
        graph_view::load(&engine.store, &view.id, &deadline).is_err(),
        "resealing must not erase captured parallel observations"
    );
}

#[test]
fn resealed_view_cannot_relabel_a_selected_overlay_or_upgrade_its_precision() {
    let root = TempDir::new().unwrap();
    fs::write(
        root.path().join("package.json"),
        r#"{"name":"app","dependencies":{"library":"^1"}}"#,
    )
    .unwrap();
    let mut engine = Engine::open(root.path()).unwrap();
    let deadline = Deadline::new(20000);
    let (snapshot, graph, _, _) = engine.index(&deadline).unwrap();
    let original = graph_view::build(
        &engine.store,
        &snapshot,
        &graph,
        &BuildOptions::default(),
        &deadline,
    )
    .unwrap();
    for defect in ["label", "precision"] {
        let mut view = original.clone();
        if defect == "label" {
            let node = view
                .nodes
                .iter_mut()
                .find(|node| node["origin"] == "structural_overlay")
                .unwrap();
            node["name"] = json!("invented-overlay-claim");
        } else {
            let anchor = view
                .evidence
                .iter_mut()
                .find(|anchor| {
                    anchor["origin"] == "structural_overlay" && anchor["anchor"] != "exact"
                })
                .unwrap();
            anchor["anchor"] = json!("exact");
        }
        publish_resealed(&engine, &mut view);
        assert!(
            graph_view::load(&engine.store, &view.id, &deadline).is_err(),
            "resealed {defect} must match the selected registered overlay"
        );
    }
}
