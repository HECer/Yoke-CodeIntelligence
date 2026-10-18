use serde_json::json;
use std::fs;
use tempfile::TempDir;
use yoke::{
    Engine,
    capture::Deadline,
    graph_view::{self, BuildOptions, View},
    model::{Graph, Snapshot, identity},
    semantic_enrichment,
};
#[test]
fn newly_derived_view_omits_legacy_config_file_signature_values_without_rewriting_history() {
    let root = TempDir::new().unwrap();
    let engine = Engine::open(root.path()).unwrap();
    let source = "PASSWORD = PRIVATE_RUNTIME_VALUE\n";
    let file = yoke::model::SourceFile {
        path: "app.yaml".into(),
        hash: engine.store.blob(source.as_bytes()).unwrap(),
        size: source.len(),
        language: "config".into(),
        mode: 0o644,
    };
    let mut snapshot = Snapshot {
        id: String::new(),
        workspace_id: engine.store.workspace_id.clone(),
        kind: "disk".into(),
        files: vec![file.clone()],
        omissions: vec![],
        capture_consistency: "stable-double-scan".into(),
    };
    snapshot.id = identity(&(
        &snapshot.workspace_id,
        "disk",
        "yoke-tree-sitter-4",
        &snapshot.files,
        &snapshot.omissions,
    ));
    let key = "app.yaml:file:0".to_owned();
    let graph = Graph {
        snapshot_id: snapshot.id.clone(),
        entities: vec![yoke::model::Entity {
            id: identity(&(&snapshot.id, &key)),
            key,
            snapshot_id: snapshot.id.clone(),
            path: file.path.clone(),
            name: file.path.clone(),
            qualified_name: file.path.clone(),
            kind: "file".into(),
            language: "config".into(),
            signature: source.trim_end().into(),
            start_byte: 0,
            end_byte: source.len(),
            start_line: 1,
            end_line: 1,
            name_start_byte: None,
            name_end_byte: None,
            parent_key: None,
            producer: "yoke-tree-sitter-4".into(),
        }],
        ..Graph::default()
    };
    let original = serde_json::to_string(&graph).unwrap();
    engine
        .store
        .db
        .execute(
            "INSERT INTO snapshots VALUES(?1,?2)",
            rusqlite::params![snapshot.id, serde_json::to_string(&snapshot).unwrap()],
        )
        .unwrap();
    engine
        .store
        .db
        .execute(
            "INSERT INTO graphs VALUES(?1,?2)",
            rusqlite::params![snapshot.id, original],
        )
        .unwrap();
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
    let exported = graph_view::export(&engine.store, &view, "json", &Deadline::new(10000)).unwrap();
    assert!(!exported.to_string().contains("PRIVATE_RUNTIME_VALUE"));
    assert_eq!(
        serde_json::to_string(&engine.store.graph(&snapshot.id).unwrap()).unwrap(),
        original
    );
    assert_eq!(
        yoke::query::source(&engine.store, &snapshot, &file.path)
            .unwrap()
            .0,
        source
    );
    for format in [
        "svg", "wiki", "tree", "callflow", "cypher", "canvas", "obsidian",
    ] {
        let value =
            graph_view::export(&engine.store, &view, format, &Deadline::new(10000)).unwrap();
        assert!(
            !value.to_string().contains("PRIVATE_RUNTIME_VALUE"),
            "{format}"
        );
    }
    let mut legacy = view.clone();
    legacy.artifact_id.clear();
    legacy.id.clear();
    legacy.nodes[0]["signature"] = json!(source.trim_end());
    legacy.id = identity(&legacy);
    let bytes = serde_json::to_vec(&legacy).unwrap();
    let artifact = engine.store.artifact(&bytes).unwrap();
    engine.store.record(graph_view::RECORD_KIND,&legacy.id,&json!({"viewId":legacy.id,"artifactId":artifact,"snapshotId":snapshot.id,"workspaceId":snapshot.workspace_id,"baseGraphHash":identity(&graph),"overlayIds":[]})).unwrap();
    let error = graph_view::load(&engine.store, &legacy.id, &Deadline::new(10000)).unwrap_err();
    assert!(
        error
            .to_string()
            .contains("Legacy presentation repair required")
    );
    assert!(!error.to_string().contains("PRIVATE_RUNTIME_VALUE"));
    assert_ne!(legacy.id, view.id);
    assert_eq!(engine.store.read_artifact(&artifact).unwrap(), bytes);
}
fn fixture() -> (TempDir, Engine, Snapshot, Graph) {
    let dir = TempDir::new().unwrap();
    fs::write(
        dir.path().join("main.rs"),
        "fn helper() {}\nfn caller(){helper();helper();}\n",
    )
    .unwrap();
    fs::write(
        dir.path().join("package.json"),
        r#"{"name":"app","dependencies":{"library":"^1"}}"#,
    )
    .unwrap();
    fs::write(
        dir.path().join("config.json"),
        r##"{"service":{"$ref":"#/definitions/User"},"definitions":{"User":{"type":"object"}}}"##,
    )
    .unwrap();
    fs::write(dir.path().join("guide.md"), "# Guide\n`main.rs::helper`\n").unwrap();
    let mut engine = Engine::open(dir.path()).unwrap();
    let (s, g, _, _) = engine.index(&Deadline::new(10000)).unwrap();
    (dir, engine, s, g)
}
fn view(engine: &Engine, s: &Snapshot, g: &Graph) -> View {
    graph_view::build(
        &engine.store,
        s,
        g,
        &BuildOptions::default(),
        &Deadline::new(10000),
    )
    .unwrap()
}
#[test]
fn selected_structural_artifacts_reject_forged_node_scope_lines_and_schema() {
    let (_dir, e, s, g) = fixture();
    let derived = yoke::manifest_graph::extract(&e.store, &s, &g, &Deadline::new(10000)).unwrap();
    let mut accepted = vec![];
    for defect in ["snapshot", "line", "schema"] {
        let mut raw = serde_json::to_value(&derived).unwrap();
        raw["artifactId"] = json!("");
        match defect {
            "snapshot" => raw["nodes"][0]["snapshotId"] = json!("f".repeat(64)),
            "line" => raw["evidence"][0]["startLine"] = json!(999),
            _ => raw["unexpectedSecret"] = json!("must reject unknown payload"),
        }
        let id = e
            .store
            .artifact(&serde_json::to_vec(&raw).unwrap())
            .unwrap();
        let record = json!({"snapshotId":s.id,"workspaceId":s.workspace_id,"graphHash":identity(&g),"producer":derived.producer,"artifactId":id});
        e.store
            .db
            .execute(
                "INSERT INTO records(kind,id,value) VALUES(?1,?2,?3)",
                rusqlite::params![yoke::manifest_graph::RECORD_KIND, id, record.to_string()],
            )
            .unwrap();
        let options = BuildOptions {
            structural_kinds: vec![],
            structural_overlay_ids: vec![id],
            semantic_overlay_ids: vec![],
            ..Default::default()
        };
        if graph_view::build(&e.store, &s, &g, &options, &Deadline::new(10000)).is_ok() {
            accepted.push(defect);
        }
    }
    assert!(accepted.is_empty(), "accepted {accepted:?}");
}
#[test]
fn selected_overlay_inputs_have_an_aggregate_byte_budget() {
    let (_dir, e, s, g) = fixture();
    let derived = yoke::manifest_graph::extract(&e.store, &s, &g, &Deadline::new(10000)).unwrap();
    let mut selected = vec![];
    for suffix in ["one", "two"] {
        let mut raw = serde_json::to_value(&derived).unwrap();
        raw["artifactId"] = json!("");
        raw["limitations"] = json!([format!("{}{}", "x".repeat(16_100_000), suffix)]);
        let id = e
            .store
            .artifact(&serde_json::to_vec(&raw).unwrap())
            .unwrap();
        let record = json!({"snapshotId":s.id,"workspaceId":s.workspace_id,"graphHash":identity(&g),"producer":derived.producer,"artifactId":id});
        e.store
            .db
            .execute(
                "INSERT INTO records(kind,id,value) VALUES(?1,?2,?3)",
                rusqlite::params![yoke::manifest_graph::RECORD_KIND, id, record.to_string()],
            )
            .unwrap();
        selected.push(id);
    }
    let options = BuildOptions {
        structural_kinds: vec![],
        structural_overlay_ids: selected,
        semantic_overlay_ids: vec![],
        ..Default::default()
    };
    assert!(
        graph_view::build(&e.store, &s, &g, &options, &Deadline::new(10000)).is_err(),
        "aggregate overlay capacity must be checked before second artifact allocation"
    );
}
#[test]
fn registered_view_rejects_corrupt_cas_and_foreign_overlay_scope() {
    let (_dir, e, s, g) = fixture();
    let v = view(&e, &s, &g);
    let blob = e.store.dir.join("blobs").join(&v.artifact_id);
    let original = fs::read(&blob).unwrap();
    let mut corrupt = original.clone();
    corrupt[0] ^= 1;
    fs::write(&blob, corrupt).unwrap();
    assert!(graph_view::load(&e.store, &v.id, &Deadline::new(10000)).is_err());
    fs::write(&blob, original).unwrap();
    assert!(graph_view::load(&e.store, &v.id, &Deadline::new(10000)).is_ok());
    let derived = yoke::manifest_graph::extract(&e.store, &s, &g, &Deadline::new(10000)).unwrap();
    e.store
        .db
        .execute(
            "UPDATE records SET value=json_set(value,'$.snapshotId',?1) WHERE kind=?2 AND id=?3",
            rusqlite::params![
                "f".repeat(64),
                yoke::manifest_graph::RECORD_KIND,
                derived.artifact_id
            ],
        )
        .unwrap();
    let options = BuildOptions {
        structural_kinds: vec![],
        structural_overlay_ids: vec![derived.artifact_id],
        semantic_overlay_ids: vec![],
        ..Default::default()
    };
    assert!(graph_view::build(&e.store, &s, &g, &options, &Deadline::new(10000)).is_err());
}
#[test]
fn context_omits_environment_values_and_clips_unicode_at_byte_budget() {
    let (dir, mut e, _, _) = fixture();
    fs::write(dir.path().join("mcp.json"),r#"{"mcpServers":{"local":{"command":"node","env":{"NONSECRET_SETTING":"private-example-value"}}}}"#).unwrap();
    fs::write(
        dir.path().join("unicode.md"),
        format!("# Unicode\n{}", "🦀".repeat(100)),
    )
    .unwrap();
    let (s, g, _, _) = e.index(&Deadline::new(10000)).unwrap();
    let v = view(&e, &s, &g);
    let private = graph_view::context(
        &e.store,
        &v,
        &json!({"query":"NONSECRET_SETTING","kind":"environment_name"}),
        &Deadline::new(10000),
    )
    .unwrap();
    assert!(
        private["sources"].as_array().unwrap().is_empty(),
        "raw config context may expose environment values"
    );
    assert!(!private["sourceOmissions"].as_array().unwrap().is_empty());
    let unicode = graph_view::context(
        &e.store,
        &v,
        &json!({"query":"unicode.md","kind":"document","limit":1,"maxSourceBytes":100}),
        &Deadline::new(10000),
    )
    .unwrap();
    assert!(unicode["sourceBytes"].as_u64().unwrap() <= 100);
    assert_eq!(unicode["sources"][0]["bodyTruncated"], true);
    assert_eq!(unicode["sources"][0]["anchor"], "source_range");
}
#[test]
fn source_bound_view_exports_all_seven_formats() {
    let (_dir, e, s, g) = fixture();
    let v = view(&e, &s, &g);
    for format in [
        "svg", "wiki", "obsidian", "tree", "callflow", "cypher", "canvas",
    ] {
        let result = graph_view::export(&e.store, &v, format, &Deadline::new(10000)).unwrap();
        assert_eq!(result["graphViewId"], v.id);
        assert_eq!(result["baseGraphHash"], identity(&g));
        let bundle: yoke::graph_exports::ExportBundle =
            serde_json::from_value(result["bundle"].clone()).unwrap();
        assert!(!bundle.files.is_empty());
        assert!(
            bundle
                .files
                .iter()
                .any(|file| file.content.contains("library")),
            "{format} omits derived dependencies"
        );
        assert!(
            e.store
                .dir
                .join("blobs")
                .join(&bundle.artifact_id)
                .is_file()
        );
        assert!(
            bundle.files.iter().any(|file| file.content.contains(&v.id)
                && file.content.contains("unique_literal_occurrence")),
            "{format} must retain original view and anchor precision"
        );
    }
}
#[test]
fn captured_readonly_dependency_proof_allows_view_and_collection_reads() {
    let (dir, mut e, _, _) = fixture();
    // Keep the fixture independent of ancestor user-level dependency ignores.
    fs::write(dir.path().join(".gitignore"), "!node_modules/\n").unwrap();
    fs::create_dir_all(dir.path().join("node_modules/sdk")).unwrap();
    fs::write(
        dir.path().join("node_modules/sdk/index.ts"),
        "export class External {}\n",
    )
    .unwrap();
    e.capture_options.read_only_dependency_dirs = vec!["node_modules".into()];
    let (s, g, _, _) = e.index(&Deadline::new(10000)).unwrap();
    assert!(
        yoke::capture::source_path_policy(&e.store, &s, "node_modules/sdk/index.ts")
            .unwrap()
            .read_only
    );
    let v = view(&e, &s, &g);
    let found = graph_view::find(
        &e.store,
        &v,
        &json!({"query":"External"}),
        &Deadline::new(10000),
    )
    .unwrap();
    assert!(
        found["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|node| node["path"] == "node_modules/sdk/index.ts")
    );
    let registry_dir = TempDir::new().unwrap();
    let registry = yoke::graph_collections::Registry::open(registry_dir.path()).unwrap();
    registry
        .register("library", &e.store, &s, &g, &Deadline::new(10000))
        .unwrap();
    let collection = registry
        .join(&["library".into()], &Deadline::new(10000))
        .unwrap();
    assert!(
        collection["nodes"]
            .as_array()
            .unwrap()
            .iter()
            .any(|node| node["path"] == "node_modules/sdk/index.ts")
    );
}
#[test]
fn default_view_exposes_structural_relationships_without_replacing_canonical_graph() {
    let (_dir, e, s, g) = fixture();
    let v = view(&e, &s, &g);
    assert_eq!(v.overlays.len(), 3);
    assert!(
        v.observations
            .iter()
            .any(|o| o["relation"] == "depends_on" && o["targetLabel"] == "library")
    );
    let symbol = g.entities.iter().find(|n| n.name == "helper").unwrap();
    assert!(
        v.observations
            .iter()
            .any(|o| o["relation"] == "cites_symbol" && o["target"] == symbol.id)
    );
    assert_eq!(identity(&e.store.graph(&s.id).unwrap()), identity(&g));
    assert!(!v.complete);
    let loaded = graph_view::load(&e.store, &v.id, &Deadline::new(10000)).unwrap();
    assert_eq!(
        serde_json::to_value(loaded).unwrap(),
        serde_json::to_value(v).unwrap()
    );
}
#[test]
fn view_find_trace_context_and_export_preserve_source_and_parallel_calls() {
    let (_dir, e, s, g) = fixture();
    let v = view(&e, &s, &g);
    let found = graph_view::find(
        &e.store,
        &v,
        &json!({"query":"app","kind":"package"}),
        &Deadline::new(10000),
    )
    .unwrap();
    assert_eq!(found["items"].as_array().unwrap().len(), 1);
    let package = &found["items"][0];
    assert_eq!(
        package["sourceBindings"][0]["contentHash"],
        s.files
            .iter()
            .find(|f| f.path == "package.json")
            .unwrap()
            .hash
    );
    let caller = g.entities.iter().find(|n| n.name == "caller").unwrap();
    let result = graph_view::trace(
        &e.store,
        &v,
        &json!({"entityIds":[caller.id],"relations":["calls"],"depth":1}),
        &Deadline::new(10000),
    )
    .unwrap();
    assert_eq!(result["observations"].as_array().unwrap().len(), 2);
    let context = graph_view::context(
        &e.store,
        &v,
        &json!({"query":"app","limit":1}),
        &Deadline::new(10000),
    )
    .unwrap();
    assert!(
        context["sources"]
            .as_array()
            .unwrap()
            .iter()
            .any(|source| source["text"].as_str().unwrap().contains("library"))
    );
    let exported = graph_view::export(&e.store, &v, "json", &Deadline::new(10000)).unwrap();
    assert_eq!(exported["graphViewId"], v.id);
    assert_eq!(exported["nodes"].as_array().unwrap().len(), v.nodes.len());
}
#[test]
fn source_view_remains_historical_and_rejects_forged_scope_options_and_deadlines() {
    let (dir, mut e, s, g) = fixture();
    let v = view(&e, &s, &g);
    fs::write(dir.path().join("package.json"), r#"{"name":"changed"}"#).unwrap();
    e.index(&Deadline::new(10000)).unwrap();
    assert!(
        graph_view::find(
            &e.store,
            &v,
            &json!({"query":"app","kind":"package"}),
            &Deadline::new(10000)
        )
        .is_ok()
    );
    let mut forged = v.clone();
    forged.nodes[0]["name"] = json!("forged");
    assert!(
        graph_view::find(
            &e.store,
            &forged,
            &json!({"query":"forged"}),
            &Deadline::new(10000)
        )
        .is_err()
    );
    assert!(
        graph_view::find(
            &e.store,
            &v,
            &json!({"query":"app","allowWrites":true}),
            &Deadline::new(10000)
        )
        .is_err()
    );
    assert!(graph_view::load(&e.store, &v.id, &Deadline::new(0)).is_err());
    assert!(
        serde_json::from_value::<BuildOptions>(json!({"structuralKinds":["unsupported"]})).is_err()
    );
}
#[test]
fn semantic_and_structural_overlays_have_distinct_namespaces_and_provider_trust() {
    let (_dir, e, s, g) = fixture();
    let f = s.files.iter().find(|f| f.path == "main.rs").unwrap();
    let source = fs::read_to_string(_dir.path().join("main.rs")).unwrap();
    let helper = g.entities.iter().find(|n| n.name == "helper").unwrap();
    let caller = g.entities.iter().find(|n| n.name == "caller").unwrap();
    let h = json!({"snapshotId":s.id,"contentHash":f.hash,"path":"main.rs","startByte":helper.start_byte,"endByte":helper.end_byte,"selectionStartByte":helper.name_start_byte.unwrap(),"selectionEndByte":helper.name_end_byte.unwrap(),"name":"helper","kind":12});
    let c = json!({"snapshotId":s.id,"contentHash":f.hash,"path":"main.rs","startByte":caller.start_byte,"endByte":caller.end_byte,"selectionStartByte":caller.name_start_byte.unwrap(),"selectionEndByte":caller.name_end_byte.unwrap(),"name":"caller","kind":12});
    let at = source[caller.start_byte..].find("helper").unwrap() + caller.start_byte;
    let anchor = json!({"snapshotId":s.id,"contentHash":f.hash,"path":"main.rs","startByte":at,"endByte":at+6});
    let semantic=semantic_enrichment::persist(&e.store,&s,&g,&json!({"snapshotId":s.id,"operation":"incomingCalls","producer":"labeled_review_provider","serverProfileHash":"a".repeat(64),"items":[{"root":h,"from":c,"fromRanges":[anchor,anchor]}]}),&Deadline::new(10000)).unwrap();
    let options = BuildOptions {
        semantic_overlay_ids: vec![semantic["overlayId"].as_str().unwrap().into()],
        ..Default::default()
    };
    let v = graph_view::build(&e.store, &s, &g, &options, &Deadline::new(10000)).unwrap();
    assert_eq!(
        v.nodes
            .iter()
            .filter(|n| n["origin"] == "semantic_overlay")
            .count(),
        2
    );
    assert_eq!(
        v.observations
            .iter()
            .filter(|o| o["origin"] == "semantic_overlay" && o["relation"] == "calls")
            .count(),
        2
    );
    assert!(
        v.overlays
            .iter()
            .any(|o| o["providerAttestation"] == "unverified")
    );
    let ids: std::collections::BTreeSet<_> =
        v.nodes.iter().map(|n| n["id"].as_str().unwrap()).collect();
    assert_eq!(ids.len(), v.nodes.len());
}
