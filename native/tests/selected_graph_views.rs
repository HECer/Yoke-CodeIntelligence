use serde_json::{Value, json};
use std::fs;
use tempfile::TempDir;
use yoke::{
    Engine,
    capture::Deadline,
    graph_view::{self, BuildOptions, View},
    model::{Graph, Snapshot, identity},
};

fn fixture() -> (TempDir, Engine, Snapshot, Graph, View) {
    let dir = TempDir::new().unwrap();
    fs::write(
        dir.path().join("main.rs"),
        "fn helper() {}\nfn caller(){helper();helper();}\n",
    )
    .unwrap();
    fs::write(
        dir.path().join("package.json"),
        r#"{"name":"example_app","dependencies":{"library":"^1"}}"#,
    )
    .unwrap();
    fs::write(
        dir.path().join("guide.md"),
        "# Guide\n`main.rs::helper`\nRésumé 🦀\n",
    )
    .unwrap();
    let mut engine = Engine::open(dir.path()).unwrap();
    let (snapshot, graph, _, _) = engine.index(&Deadline::new(10000)).unwrap();
    let view = graph_view::build(
        &engine.store,
        &snapshot,
        &graph,
        &BuildOptions::default(),
        &Deadline::new(10000),
    )
    .unwrap();
    (dir, engine, snapshot, graph, view)
}

fn invoke(engine: &mut Engine, tool: &str, args: Value) -> Value {
    let result = engine.invoke(tool, args).unwrap();
    assert_eq!(result["status"], "ok", "{result}");
    assert_eq!(result["metrics"]["modelCalls"], 0);
    assert_eq!(result["metrics"]["languageServerAttempts"], 0);
    assert_eq!(result["metrics"]["localOnly"], true);
    result
}

#[test]
fn ordinary_tools_select_one_published_code_markdown_and_manifest_view() {
    // Dropping the selector or routing it to canonical symbol search loses dependencies/citations.
    let (_dir, mut engine, snapshot, graph, view) = fixture();
    let before = identity(&engine.store.graph(&snapshot.id).unwrap());
    let package = invoke(
        &mut engine,
        "ci_find",
        json!({"graphViewId":view.id,"query":"example_app","kind":"package","maxBytes":256000}),
    );
    assert_eq!(package["snapshotId"], snapshot.id);
    assert_eq!(package["data"]["items"].as_array().unwrap().len(), 1);
    assert_eq!(package["data"]["items"][0]["origin"], "structural_overlay");
    assert_eq!(package["data"]["graphView"]["baseGraphHash"], before);
    assert_eq!(package["data"]["graphView"]["complete"], false);
    assert!(
        !package["data"]["graphView"]["overlays"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert!(!package["data"]["graphView"]["coverage"].is_null());
    let dependencies = invoke(
        &mut engine,
        "ci_trace",
        json!({"graphViewId":view.id,"entityIds":[package["data"]["items"][0]["id"]],"relations":["depends_on"],"maxBytes":256000}),
    );
    assert!(
        dependencies["data"]["observations"]
            .as_array()
            .unwrap()
            .iter()
            .any(|o| o["targetLabel"] == "library")
    );
    let helper = graph
        .entities
        .iter()
        .find(|node| node.name == "helper")
        .unwrap();
    let references = invoke(
        &mut engine,
        "ci_trace",
        json!({"graphViewId":view.id,"snapshotId":snapshot.id,"entityIds":[helper.id],"direction":"in","relations":["cites_symbol"],"maxBytes":256000}),
    );
    assert!(
        references["data"]["observations"]
            .as_array()
            .unwrap()
            .iter()
            .any(|o| o["relation"] == "cites_symbol" && o["target"] == helper.id)
    );
    let document = invoke(
        &mut engine,
        "ci_find",
        json!({"graphViewId":view.id,"query":"Guide","path":"guide.md","maxBytes":256000}),
    );
    assert!(!document["data"]["items"].as_array().unwrap().is_empty());
    let code = invoke(
        &mut engine,
        "ci_find",
        json!({"graphViewId":view.id,"query":"helper","maxBytes":256000}),
    );
    assert!(
        code["data"]["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|node| node["id"] == helper.id && node["origin"] == "canonical")
    );
    assert_eq!(identity(&engine.store.graph(&snapshot.id).unwrap()), before);
    assert_eq!(
        engine.store.latest().unwrap().as_deref(),
        Some(snapshot.id.as_str())
    );
}

#[test]
fn selected_context_and_entity_read_use_historic_cas_without_capturing_live_source() {
    // Accidentally calling Engine::select/index replaces the historical source with live bytes.
    let (dir, mut engine, old, _, view) = fixture();
    fs::write(dir.path().join("package.json"), r#"{"name":"new_app"}"#).unwrap();
    let (new, _, _, _) = engine.index(&Deadline::new(10000)).unwrap();
    assert_ne!(old.id, new.id);
    let context = invoke(
        &mut engine,
        "ci_context",
        json!({"graphViewId":view.id,"query":"example_app","kind":"package","maxBytes":256000}),
    );
    assert_eq!(context["snapshotId"], old.id);
    assert_eq!(context["data"]["graphViewId"], view.id);
    assert!(
        context["data"]["sources"]
            .as_array()
            .unwrap()
            .iter()
            .any(|s| s["text"].as_str().unwrap().contains("\"library\":\"^1\""))
    );
    let selected = &context["data"]["selected"][0];
    let read = invoke(
        &mut engine,
        "ci_read",
        json!({"graphViewId":view.id,"entityId":selected["id"],"maxBytes":256000}),
    );
    assert_eq!(read["snapshotId"], old.id);
    assert_eq!(read["data"]["path"], "package.json");
    assert_eq!(
        read["data"]["contentHash"],
        selected["sourceBindings"][0]["contentHash"]
    );
    assert_eq!(
        read["data"]["declarationAnchor"],
        selected["sourceBindings"][0]
    );
    assert!(
        read["data"]["text"]
            .as_str()
            .unwrap()
            .contains("example_app")
    );
    assert_eq!(
        engine.store.latest().unwrap().as_deref(),
        Some(new.id.as_str())
    );
    let plain = invoke(
        &mut engine,
        "ci_find",
        json!({"snapshotId":new.id,"query":"helper"}),
    );
    assert!(plain["data"].get("graphViewId").is_none());
}

#[test]
fn selected_queries_reject_scope_mismatch_unknown_fields_and_canonical_only_modes() {
    let (_dir, mut engine, snapshot, _, view) = fixture();
    let bad = [
        (
            "ci_find",
            json!({"graphViewId":view.id,"query":"helper","snapshotId":"f".repeat(64)}),
        ),
        (
            "ci_find",
            json!({"graphViewId":"f".repeat(64),"query":"helper"}),
        ),
        (
            "ci_find",
            json!({"graphViewId":view.id,"query":"helper","regex":false}),
        ),
        (
            "ci_find",
            json!({"graphViewId":view.id,"query":"helper","kind":"all"}),
        ),
        (
            "ci_find",
            json!({"graphViewId":view.id,"query":"helper","kind":"semantic","model":"local"}),
        ),
        (
            "ci_find",
            json!({"graphViewId":view.id,"query":"helper","unknown":true}),
        ),
        (
            "ci_find",
            json!({"graphViewId":view.id,"languageServer":{"operation":"definition","path":"main.rs","offset":3}}),
        ),
        (
            "ci_context",
            json!({"graphViewId":view.id,"plan":[{"op":"find","query":"helper"}]}),
        ),
        (
            "ci_context",
            json!({"graphViewId":view.id,"query":"helper","includeMemory":true}),
        ),
        (
            "ci_read",
            json!({"graphViewId":view.id,"artifactId":view.artifact_id}),
        ),
        (
            "ci_read",
            json!({"graphViewId":view.id,"entityId":"f".repeat(64)}),
        ),
        (
            "ci_read",
            json!({"graphViewId":view.id,"path":"../main.rs"}),
        ),
        ("ci_status", json!({"graphViewId":view.id,"mode":"index"})),
        (
            "ci_change_prepare",
            json!({"graphViewId":view.id,"edits":[]}),
        ),
        (
            "ci_find",
            json!({"graphViewId":"not_an_id","query":"helper"}),
        ),
        ("ci_context", json!({"query":"helper","maxSourceBytes":200})),
    ];
    for (tool, args) in bad {
        let result = engine.invoke(tool, args.clone()).unwrap();
        assert_eq!(
            result["status"], "error",
            "accepted {tool}: {args}: {result}"
        );
        assert_eq!(result["metrics"]["languageServerAttempts"], 0);
    }
    assert_eq!(
        engine.store.latest().unwrap().as_deref(),
        Some(snapshot.id.as_str())
    );
}

#[test]
fn selected_query_replays_published_view_and_rejects_forged_registry_scope() {
    // Trusting a decoded record without graph_view::load admits another workspace's claims.
    let (_dir, mut engine, snapshot, _, view) = fixture();
    engine.store.db.execute("UPDATE records SET value=json_set(value,'$.workspaceId','foreign') WHERE kind=?1 AND id=?2", rusqlite::params![graph_view::RECORD_KIND,view.id]).unwrap();
    for (tool, mut args) in [
        ("ci_find", json!({"query":"helper"})),
        ("ci_context", json!({"query":"helper"})),
        ("ci_read", json!({"path":"main.rs"})),
        ("ci_trace", json!({"entityIds":[view.nodes[0]["id"]]})),
    ] {
        args["graphViewId"] = json!(view.id);
        let result = engine.invoke(tool, args).unwrap();
        assert_eq!(result["status"], "error", "{tool}: {result}");
    }
    assert_eq!(
        engine.store.latest().unwrap().as_deref(),
        Some(snapshot.id.as_str())
    );
}

#[test]
fn native_mcp_keeps_eight_tools_and_exercises_published_view_selection() {
    let (_dir, mut engine, snapshot, _, view) = fixture();
    let requests = [
        json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}),
        json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}),
        json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"ci_find","arguments":{"graphViewId":view.id,"query":"example_app","kind":"package","maxBytes":256000}}}),
    ];
    let input = requests
        .iter()
        .map(|v| format!("{v}\n"))
        .collect::<String>();
    let mut output = Vec::new();
    yoke::protocol::serve(&mut engine, std::io::Cursor::new(input), &mut output).unwrap();
    let responses: Vec<Value> = String::from_utf8(output)
        .unwrap()
        .lines()
        .map(|s| serde_json::from_str(s).unwrap())
        .collect();
    let tools = responses[1]["result"]["tools"].as_array().unwrap();
    assert_eq!(tools.len(), 8);
    for tool in tools {
        assert_eq!(
            tool["inputSchema"]["properties"]
                .get("graphViewId")
                .is_some(),
            matches!(
                tool["name"].as_str(),
                Some("ci_find" | "ci_trace" | "ci_context" | "ci_read")
            )
        );
    }
    let result: Value = serde_json::from_str(
        responses[2]["result"]["content"][0]["text"]
            .as_str()
            .unwrap(),
    )
    .unwrap();
    assert_eq!(result["status"], "ok", "{result}");
    assert_eq!(result["snapshotId"], snapshot.id);
    assert_eq!(result["data"]["items"][0]["name"], "example_app");
}

#[test]
fn selected_semantic_claims_keep_distinct_nodes_parallel_edges_and_provider_lineage() {
    // Flattening semantic overlays into canonical nodes destroys identity and parallel evidence.
    let (_dir, mut engine, snapshot, graph, _) = fixture();
    let file = snapshot.files.iter().find(|f| f.path == "main.rs").unwrap();
    let helper = graph.entities.iter().find(|n| n.name == "helper").unwrap();
    let caller = graph.entities.iter().find(|n| n.name == "caller").unwrap();
    let item = |node: &yoke::model::Entity| json!({"snapshotId":snapshot.id,"contentHash":file.hash,"path":"main.rs","startByte":node.start_byte,"endByte":node.end_byte,"selectionStartByte":node.name_start_byte.unwrap(),"selectionEndByte":node.name_end_byte.unwrap(),"name":node.name,"kind":12});
    let source = "fn helper() {}\nfn caller(){helper();helper();}\n";
    let at = source[caller.start_byte..].find("helper").unwrap() + caller.start_byte;
    let anchor = json!({"snapshotId":snapshot.id,"contentHash":file.hash,"path":"main.rs","startByte":at,"endByte":at+6});
    let overlay = yoke::semantic_enrichment::persist(&engine.store, &snapshot, &graph, &json!({"snapshotId":snapshot.id,"operation":"incomingCalls","producer":"labeled_selected_view_fixture","serverProfileHash":"a".repeat(64),"items":[{"root":item(helper),"from":item(caller),"fromRanges":[anchor,anchor]}]}), &Deadline::new(10000)).unwrap();
    let view = graph_view::build(
        &engine.store,
        &snapshot,
        &graph,
        &BuildOptions {
            semantic_overlay_ids: vec![overlay["overlayId"].as_str().unwrap().into()],
            ..BuildOptions::default()
        },
        &Deadline::new(10000),
    )
    .unwrap();
    let found = invoke(
        &mut engine,
        "ci_find",
        json!({"graphViewId":view.id,"query":"helper","maxBytes":256000}),
    );
    let items = found["data"]["items"].as_array().unwrap();
    assert!(
        items
            .iter()
            .any(|node| node["id"] == helper.id && node["origin"] == "canonical")
    );
    assert!(
        items
            .iter()
            .any(|node| node["id"] != helper.id && node["origin"] == "semantic_overlay")
    );
    let semantic = view
        .nodes
        .iter()
        .find(|node| node["origin"] == "semantic_overlay" && node["name"] == "caller")
        .unwrap();
    let traced = invoke(
        &mut engine,
        "ci_trace",
        json!({"graphViewId":view.id,"entityIds":[semantic["id"]],"relations":["calls"],"depth":1,"maxBytes":256000}),
    );
    let observations = traced["data"]["observations"].as_array().unwrap();
    assert_eq!(observations.len(), 2);
    assert_ne!(observations[0]["id"], observations[1]["id"]);
    assert!(
        observations
            .iter()
            .all(|o| o["origin"] == "semantic_overlay")
    );
    assert!(
        traced["data"]["graphView"]["overlays"]
            .as_array()
            .unwrap()
            .iter()
            .any(|overlay| overlay["providerAttestation"] == "unverified")
    );
}

#[test]
fn bounded_selected_response_pages_complete_lineage_without_claiming_generic_artifact_scope() {
    let (_dir, mut engine, snapshot, _, view) = fixture();
    let result = engine
        .invoke(
            "ci_find",
            json!({"graphViewId":view.id,"query":"helper","maxBytes":1024}),
        )
        .unwrap();
    assert_eq!(result["status"], "partial", "{result}");
    assert!(serde_json::to_vec(&result).unwrap().len() <= 1024);
    let paged = invoke(
        &mut engine,
        "ci_read",
        json!({"artifactId":result["artifactId"],"maxBytes":256000}),
    );
    assert!(paged["snapshotId"].is_null());
    assert!(paged["data"]["nextCursor"].is_null());
    let complete: Value = serde_json::from_str(paged["data"]["chunk"].as_str().unwrap()).unwrap();
    assert_eq!(complete["snapshotId"], snapshot.id);
    assert_eq!(complete["data"]["graphViewId"], view.id);
    assert_eq!(
        complete["data"]["graphView"]["artifactId"],
        view.artifact_id
    );
    assert!(!complete["data"]["items"].as_array().unwrap().is_empty());
}

#[test]
fn selected_source_reads_preserve_utf8_boundaries_and_reject_ambiguous_source_selectors() {
    let (_dir, mut engine, _, graph, view) = fixture();
    let helper = graph
        .entities
        .iter()
        .find(|node| node.name == "helper")
        .unwrap();
    let source = "# Guide\n`main.rs::helper`\nRésumé 🦀\n";
    let crab = source.find('🦀').unwrap();
    let read = invoke(
        &mut engine,
        "ci_read",
        json!({"graphViewId":view.id,"path":"guide.md","startByte":crab,"endByte":crab+4,"maxBytes":256000}),
    );
    assert_eq!(read["data"]["text"], "🦀");
    for args in [
        json!({"graphViewId":view.id,"path":"guide.md","startByte":crab+1,"endByte":crab+4}),
        json!({"graphViewId":view.id,"entityId":helper.id,"path":"guide.md"}),
        json!({"graphViewId":view.id,"entityId":helper.id,"startByte":0,"endLine":1}),
    ] {
        let result = engine.invoke("ci_read", args).unwrap();
        assert_eq!(result["status"], "error", "{result}");
    }
}
