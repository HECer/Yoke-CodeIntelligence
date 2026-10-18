use serde_json::{Value, json};
use std::fs;
use tempfile::TempDir;
use yoke::{
    Engine,
    capture::Deadline,
    graph_view::{self, BuildOptions, View},
    model::{Graph, Snapshot, identity},
};

fn fixture(files: &[(&str, &str)]) -> (TempDir, Engine, Snapshot, Graph, View) {
    let root = TempDir::new().unwrap();
    for (path, text) in files {
        fs::write(root.path().join(path), text).unwrap();
    }
    let mut engine = Engine::open(root.path()).unwrap();
    let (snapshot, graph, _, _) = engine.index(&Deadline::new(10000)).unwrap();
    let view = graph_view::build(
        &engine.store,
        &snapshot,
        &graph,
        &BuildOptions {
            structural_kinds: vec![],
            ..BuildOptions::default()
        },
        &Deadline::new(10000),
    )
    .unwrap();
    (root, engine, snapshot, graph, view)
}

fn selected_requests(view: &View, entity: &str) -> Vec<(&'static str, Value)> {
    vec![
        ("ci_find", json!({"graphViewId":view.id,"query":"helper"})),
        (
            "ci_trace",
            json!({"graphViewId":view.id,"entityIds":[entity]}),
        ),
        (
            "ci_context",
            json!({"graphViewId":view.id,"query":"helper"}),
        ),
        ("ci_read", json!({"graphViewId":view.id,"entityId":entity})),
    ]
}

fn invoke_ok(engine: &mut Engine, tool: &str, mut args: Value) -> Value {
    args["maxBytes"] = json!(256000);
    let response = engine.invoke(tool, args).unwrap();
    assert_eq!(response["status"], "ok", "{tool}: {response}");
    assert_eq!(response["metrics"]["modelCalls"], 0);
    assert_eq!(response["metrics"]["languageServerAttempts"], 0);
    assert_eq!(response["metrics"]["localOnly"], true);
    response
}

#[test]
fn selected_queries_reject_same_length_view_cas_corruption() {
    let (_root, mut engine, snapshot, graph, view) =
        fixture(&[("main.rs", "fn helper() {}\nfn caller(){helper();}\n")]);
    let helper = graph.entities.iter().find(|n| n.name == "helper").unwrap();
    let path = engine.store.dir.join("blobs").join(&view.artifact_id);
    let mut bytes = fs::read(&path).unwrap();
    let offset = bytes.windows(6).position(|b| b == b"helper").unwrap();
    bytes[offset] = b'H';
    fs::write(&path, bytes).unwrap();
    for (tool, args) in selected_requests(&view, &helper.id) {
        let response = engine.invoke(tool, args).unwrap();
        assert_eq!(response["status"], "error", "{tool}: {response}");
        assert_eq!(response["metrics"]["languageServerAttempts"], 0);
    }
    assert_eq!(
        identity(&engine.store.graph(&snapshot.id).unwrap()),
        identity(&graph)
    );
    assert_eq!(
        engine.store.latest().unwrap().as_deref(),
        Some(snapshot.id.as_str())
    );
}

#[test]
fn selected_queries_reject_replaced_base_graph_without_republishing() {
    let (_root, mut engine, snapshot, mut graph, view) =
        fixture(&[("main.rs", "fn helper() {}\nfn caller(){helper();}\n")]);
    let helper = graph
        .entities
        .iter()
        .find(|n| n.name == "helper")
        .unwrap()
        .id
        .clone();
    graph
        .entities
        .iter_mut()
        .find(|n| n.id == helper)
        .unwrap()
        .name = "forged".into();
    engine
        .store
        .db
        .execute(
            "UPDATE graphs SET value=?1 WHERE id=?2",
            rusqlite::params![serde_json::to_string(&graph).unwrap(), snapshot.id],
        )
        .unwrap();
    for (tool, args) in selected_requests(&view, &helper) {
        let response = engine.invoke(tool, args).unwrap();
        assert_eq!(response["status"], "error", "{tool}: {response}");
    }
}

#[test]
fn selected_queries_replay_exact_source_after_files_disappear_and_engine_reopens() {
    let original = "fn helper() { /* résumé 🦀 */ }\nfn caller(){helper();}\n";
    let (root, engine, snapshot, graph, view) = fixture(&[("main.rs", original)]);
    let helper = graph.entities.iter().find(|n| n.name == "helper").unwrap();
    fs::remove_file(root.path().join("main.rs")).unwrap();
    fs::write(root.path().join("live.rs"), "fn changed_live_source() {}\n").unwrap();
    drop(engine);
    let mut engine = Engine::open(root.path()).unwrap();
    for (tool, args) in selected_requests(&view, &helper.id) {
        let response = invoke_ok(&mut engine, tool, args);
        assert_eq!(response["snapshotId"], snapshot.id);
        assert_eq!(
            response["data"]["graphView"]["baseGraphHash"],
            identity(&graph)
        );
    }
    let read = invoke_ok(
        &mut engine,
        "ci_read",
        json!({"graphViewId":view.id,"path":"main.rs","startLine":1,"endLine":1}),
    );
    assert_eq!(
        read["data"]["text"],
        original.lines().next().unwrap().to_owned() + "\n"
    );
    assert_eq!(
        engine.store.latest().unwrap().as_deref(),
        Some(snapshot.id.as_str())
    );
    assert!(
        !engine
            .store
            .snapshot(&snapshot.id)
            .unwrap()
            .files
            .iter()
            .any(|f| f.path == "live.rs")
    );
}

#[test]
fn selected_library_queries_reject_mutated_in_memory_views() {
    let (_root, engine, _, _, mut view) = fixture(&[("main.rs", "fn helper() {}\n")]);
    view.nodes
        .iter_mut()
        .find(|node| node["name"] == "helper")
        .unwrap()["name"] = json!("forged");
    let deadline = Deadline::new(10000);
    assert!(graph_view::find(&engine.store, &view, &json!({"query":"forged"}), &deadline).is_err());
    assert!(
        graph_view::context(&engine.store, &view, &json!({"query":"forged"}), &deadline).is_err()
    );
    assert!(
        graph_view::trace(
            &engine.store,
            &view,
            &json!({"entityIds":[view.nodes[0]["id"]]}),
            &deadline
        )
        .is_err()
    );
}

#[test]
fn selected_trace_visit_budget_includes_initial_entities() {
    let (_root, mut engine, _, graph, view) =
        fixture(&[("main.rs", "fn helper() {}\nfn caller(){helper();}\n")]);
    let ids: Vec<_> = graph
        .entities
        .iter()
        .filter(|n| n.kind == "function")
        .map(|n| &n.id)
        .collect();
    assert_eq!(ids.len(), 2);
    let response = engine.invoke("ci_trace", json!({"graphViewId":view.id,"entityIds":ids,"depth":0,"maxVisited":1,"maxBytes":256000})).unwrap();
    assert!(
        response["status"] == "error"
            || response["data"]["entities"]
                .as_array()
                .is_some_and(|nodes| nodes.len() <= 1),
        "maxVisited=1 accepted two distinct starting entities: {response}"
    );
}

#[test]
fn selected_context_reports_wholly_omitted_source_when_budget_is_exhausted() {
    // Make the first declaration exactly 100 bytes so it has no partial-body marker.
    let prefix = "fn chunk_a() { /* ";
    let suffix = " */ }";
    let first = format!(
        "{prefix}{}{suffix}",
        "a".repeat(100 - prefix.len() - suffix.len())
    );
    let (_root, mut engine, _, graph, view) = fixture(&[
        ("a.rs", &first),
        ("b.rs", "fn chunk_b() { /* extra source */ }"),
    ]);
    let a = graph.entities.iter().find(|n| n.name == "chunk_a").unwrap();
    assert_eq!(a.end_byte - a.start_byte, 100);
    let response = invoke_ok(
        &mut engine,
        "ci_context",
        json!({"graphViewId":view.id,"query":"chunk_","kind":"function","depth":0,"maxSourceBytes":100}),
    );
    let data = &response["data"];
    let selected = data["selected"].as_array().unwrap();
    let sources = data["sources"].as_array().unwrap();
    let omissions = data["sourceOmissions"].as_array().unwrap();
    assert_eq!(selected.len(), 2);
    assert_eq!(sources.len(), 1);
    assert_eq!(data["sourceBytes"], 100);
    assert_eq!(sources[0]["bodyTruncated"], false);
    for node in selected {
        assert!(
            sources.iter().any(|s| s["entityId"] == node["id"])
                || omissions.iter().any(|s| s["entityId"] == node["id"]),
            "Selected source was dropped without a source omission record: {response}"
        );
    }
}
