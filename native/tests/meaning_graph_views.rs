//! Explicit published meaning selection, exercised through existing View/MCP entry points.
use serde::Serialize;
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::PathBuf,
};
use tempfile::TempDir;
use yoke::{
    Engine,
    capture::Deadline,
    graph_view::{self, BuildOptions, View},
    meaning_build::{self, MeaningPolicy},
    model::{Graph, Snapshot, hash, identity},
    providers::ProviderSpec,
};

const A: &str = "// Historical supporting bytes remain a citation with their original context and deliberately exceed the small excerpt budget.\npub fn alpha() -> &'static str { \"über\" }\n";
const B: &str = "pub fn beta() -> u32 { 2 }\n";

struct Fixture {
    _dir: TempDir,
    engine: Engine,
    snapshot: Snapshot,
    graph: Graph,
    meaning_id: String,
    second_meaning_id: String,
    marker: PathBuf,
}

fn d() -> Deadline {
    Deadline::new(30000)
}

fn fixture() -> Fixture {
    let dir = TempDir::new().unwrap();
    let root = dir.path().join("workspace");
    fs::create_dir(&root).unwrap();
    fs::write(root.join("a.rs"), A).unwrap();
    fs::write(root.join("b.rs"), B).unwrap();
    let worker = dir.path().join("meaning-view-worker.cjs");
    let marker = dir.path().join("provider-calls.txt");
    fs::write(
        &worker,
        r#"
let raw='';process.stdin.on('data',b=>raw+=b);process.stdin.on('end',()=>{
 const r=JSON.parse(raw),fs=require('fs');fs.appendFileSync(process.argv[2],r.requestId+'\n');
 const support=s=>[{sourceId:s.anchor.sourceId,startByte:s.anchor.startByte,endByte:s.anchor.endByte}];
 const sources=r.payload.sources;
 const data={concepts:sources.map((s,i)=>({key:'c'+i,label:'Route '+i,description:'Owned fixture inference',aliases:['Pipeline'],support:support(s)})),relations:[],hyperedges:[],coverage:sources.map(s=>({sourceId:s.anchor.sourceId,outcome:'claims'}))};
 if(sources.length===2){
  const both=[...support(sources[0]),...support(sources[1])];
  data.relations=[{source:'c0',target:'c1',relation:'transfers',description:'Directed fixture claim',support:both}];
  data.hyperedges=[{relation:'coordinates',participants:[{role:'input',concept:'c0'},{role:'output',concept:'c1'}],description:'Ordered roles, no inferred direction',support:both}];
 }
 process.stdout.write(JSON.stringify({schema:1,operation:r.operation,model:r.model,modelVersion:r.modelVersion,requestId:r.requestId,inputHash:r.inputHash,sourceBindings:sources.map(s=>s.anchor),currency:r.currency,actualCostMicros:2,usage:{inputTokens:4,outputTokens:3,cachedInputTokens:0},data}));
});
"#,
    )
    .unwrap();
    let mut engine = Engine::open(&root).unwrap();
    let (snapshot, graph, _, _) = engine.index(&d()).unwrap();
    let mut policy = MeaningPolicy::files(&snapshot, hash("meaning-view-owned-test-contract"));
    policy.context_groups = vec![vec!["a.rs".into(), "b.rs".into()]];
    let provider = ProviderSpec {
        trusted: true,
        command: PathBuf::from("node"),
        args: vec![
            worker.to_str().unwrap().into(),
            marker.to_str().unwrap().into(),
        ],
        model_id: "meaning-view-owned-fixture".into(),
        model_version: "v1".into(),
        task_id: "meaning-view-owned-fixture".into(),
        currency: "EUR".into(),
        max_cost_micros: 3,
        task_limit_micros: 30,
    };
    let result =
        meaning_build::build(&engine.store, &snapshot, &graph, &provider, &policy, &d()).unwrap();
    assert_eq!(result["status"], "ready");
    let second =
        meaning_build::build(&engine.store, &snapshot, &graph, &provider, &policy, &d()).unwrap();
    assert_eq!(second["modelCallsThisRun"], 0);
    assert_eq!(fs::read_to_string(&marker).unwrap().lines().count(), 3);
    // Building/selecting/querying a published View must not resolve this missing executable input.
    fs::rename(&worker, worker.with_extension("disabled")).unwrap();
    Fixture {
        _dir: dir,
        engine,
        snapshot,
        graph,
        meaning_id: result["graphId"].as_str().unwrap().into(),
        second_meaning_id: second["graphId"].as_str().unwrap().into(),
        marker,
    }
}

fn selected(f: &Fixture) -> View {
    let input = json!({"structuralKinds":[],"meaningGraphIds":[f.meaning_id]});
    let decoded = serde_json::from_value::<BuildOptions>(input);
    assert!(
        decoded.is_ok(),
        "An explicit already-published meaning selection is rejected: {decoded:?}"
    );
    graph_view::build(
        &f.engine.store,
        &f.snapshot,
        &f.graph,
        &decoded.unwrap(),
        &d(),
    )
    .unwrap()
}

fn invoke(f: &mut Fixture, tool: &str, args: Value) -> Value {
    let response = f.engine.invoke(tool, args).unwrap();
    assert_eq!(response["status"], "ok", "{response}");
    assert_eq!(response["metrics"]["modelCalls"], 0);
    assert_eq!(response["metrics"]["languageServerAttempts"], 0);
    assert_eq!(fs::read_to_string(&f.marker).unwrap().lines().count(), 3);
    response["data"].clone()
}

#[test]
fn unselected_views_keep_the_released_schema_identity_and_artifact_bytes() {
    // This released schema is an independent serialization oracle for absent optional layers.
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct LegacyView<'a> {
        id: &'a str,
        artifact_id: &'a str,
        snapshot_id: &'a str,
        workspace_id: &'a str,
        base_graph_hash: &'a str,
        overlays: &'a [Value],
        nodes: &'a [Value],
        observations: &'a [Value],
        evidence: &'a [Value],
        limitations: &'a [String],
        complete: bool,
    }
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("a.rs"), A).unwrap();
    let mut engine = Engine::open(dir.path()).unwrap();
    let (snapshot, graph, _, _) = engine.index(&d()).unwrap();
    assert_eq!(
        serde_json::to_value(BuildOptions::default()).unwrap(),
        json!({"structuralKinds":["manifest","config","markdown"],"structuralOverlayIds":[],"semanticOverlayIds":[]})
    );
    let view = graph_view::build(
        &engine.store,
        &snapshot,
        &graph,
        &BuildOptions::default(),
        &d(),
    )
    .unwrap();
    assert!(
        serde_json::to_value(&view)
            .unwrap()
            .get("meaning")
            .is_none()
    );
    let mut legacy = LegacyView {
        id: "",
        artifact_id: "",
        snapshot_id: &view.snapshot_id,
        workspace_id: &view.workspace_id,
        base_graph_hash: &view.base_graph_hash,
        overlays: &view.overlays,
        nodes: &view.nodes,
        observations: &view.observations,
        evidence: &view.evidence,
        limitations: &view.limitations,
        complete: false,
    };
    assert_eq!(identity(&legacy), view.id);
    legacy.id = &view.id;
    assert_eq!(hash(serde_json::to_vec(&legacy).unwrap()), view.artifact_id);
    let record: Value = engine
        .store
        .get_record(graph_view::RECORD_KIND, &view.id)
        .unwrap();
    assert_eq!(
        record,
        json!({"viewId":view.id,"artifactId":view.artifact_id,"snapshotId":view.snapshot_id,"workspaceId":view.workspace_id,"baseGraphHash":view.base_graph_hash,"overlayIds":view.overlays.iter().map(|o|o["artifactId"].clone()).collect::<Vec<_>>()})
    );
    let empty: BuildOptions = serde_json::from_value(json!({"meaningGraphIds":[]})).unwrap();
    assert!(
        serde_json::to_value(&empty)
            .unwrap()
            .get("meaningGraphIds")
            .is_none()
    );
    let explicit_empty = graph_view::build(&engine.store, &snapshot, &graph, &empty, &d()).unwrap();
    assert_eq!(explicit_empty.id, view.id);
    assert_eq!(explicit_empty.artifact_id, view.artifact_id);
    let report = yoke::graph_extensions::prepare_view_report(
        &engine.store,
        &view,
        &Default::default(),
        &d(),
    )
    .unwrap();
    let empty_report = yoke::graph_extensions::prepare_view_report(
        &engine.store,
        &explicit_empty,
        &Default::default(),
        &d(),
    )
    .unwrap();
    assert_eq!(report.report(), empty_report.report());
    let first_dir = TempDir::new().unwrap();
    let second_dir = TempDir::new().unwrap();
    let first = yoke::graph_collections::Registry::open(first_dir.path()).unwrap();
    let second = yoke::graph_collections::Registry::open(second_dir.path()).unwrap();
    let first_id = first
        .register_view_cas("legacy", &engine.store, &view, None, &d())
        .unwrap();
    let second_id = second
        .register_view_cas("legacy", &engine.store, &explicit_empty, None, &d())
        .unwrap();
    assert_eq!(first_id, second_id);
    let db = rusqlite::Connection::open_with_flags(
        first_dir.path().join("yoke-collection.sqlite"),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .unwrap();
    let payload: String = db
        .query_row(
            "SELECT payload FROM registrations WHERE name='legacy'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    let archive: Value = serde_json::from_str(&payload).unwrap();
    assert!(archive["archive"].get("meaningArchive").is_none());
    assert!(archive["archive"]["view"].get("meaning").is_none());
}

#[test]
fn explicit_meaning_view_finds_alias_candidates_and_source_without_merging_claims() {
    let mut f = fixture();
    let view = selected(&f);
    let source_only = graph_view::build(
        &f.engine.store,
        &f.snapshot,
        &f.graph,
        &BuildOptions {
            structural_kinds: vec![],
            ..BuildOptions::default()
        },
        &d(),
    )
    .unwrap();
    assert_eq!(view.nodes, source_only.nodes);
    assert_eq!(view.observations, source_only.observations);
    assert_eq!(view.evidence, source_only.evidence);
    assert_eq!(view.overlays, source_only.overlays);
    assert_eq!(view.limitations, source_only.limitations);
    assert_eq!(view.base_graph_hash, source_only.base_graph_hash);
    assert_eq!(view.snapshot_id, source_only.snapshot_id);
    let found = invoke(
        &mut f,
        "ci_find",
        json!({"graphViewId":view.id,"query":"Pipeline","kind":"meaning_concept","limit":20,"maxBytes":256000}),
    );
    let items = found["items"].as_array().unwrap();
    // Two independent single-file packets plus two concepts in the fixed cross-file packet.
    assert_eq!(
        items.len(),
        4,
        "Alias candidates must retain every packet/claim identity: {found}"
    );
    let mut ids = BTreeSet::new();
    for item in items {
        assert!(ids.insert(item["id"].as_str().unwrap()));
        assert_eq!(item["kind"], "meaning_concept");
        assert_eq!(item["origin"], "meaning_graph");
        assert_eq!(item["resolution"], "inferred_concept");
        assert_eq!(item["providerAttestation"], "unverified");
        assert!(
            item["aliases"]
                .as_array()
                .unwrap()
                .iter()
                .any(|a| a == "Pipeline")
        );
        for field in ["path", "startByte", "endByte", "declarationAnchor"] {
            assert!(
                item.get(field).is_none(),
                "Inference acquired a fake code field: {item}"
            );
        }
    }
    let source = invoke(
        &mut f,
        "ci_find",
        json!({"graphViewId":view.id,"query":"alpha","kind":"function","maxBytes":256000}),
    );
    assert_eq!(source["items"].as_array().unwrap().len(), 1);
    assert_eq!(source["items"][0]["origin"], "canonical");
    assert_eq!(
        identity(&f.engine.store.graph(&f.snapshot.id).unwrap()),
        view.base_graph_hash
    );
}

#[test]
fn meaning_read_and_context_return_historic_quote_citations_without_declaration_anchors() {
    let mut f = fixture();
    let view = selected(&f);
    let found = invoke(
        &mut f,
        "ci_find",
        json!({"graphViewId":view.id,"query":"Pipeline","kind":"meaning_concept","path":"a.rs","maxBytes":256000}),
    );
    let id = found["items"][0]["id"].clone();
    fs::write(
        f.engine.root.join("a.rs"),
        "pub fn alpha() -> u32 { 999 }\n",
    )
    .unwrap();
    let (new, _, _, _) = f.engine.index(&d()).unwrap();
    assert_ne!(new.id, f.snapshot.id);
    let read = invoke(
        &mut f,
        "ci_read",
        json!({"graphViewId":view.id,"entityId":id,"maxBytes":256000}),
    );
    assert_eq!(read["item"]["id"], id);
    assert!(read.get("declarationAnchor").is_none());
    assert!(read["item"].get("declarationAnchor").is_none());
    let quotes = read["citations"].as_array().unwrap();
    assert_eq!(quotes.len(), 1);
    assert_eq!(quotes[0]["snapshotId"], f.snapshot.id);
    assert_eq!(quotes[0]["path"], "a.rs");
    assert_eq!(quotes[0]["contentHash"], hash(A));
    assert_eq!(quotes[0]["text"], A);
    assert_eq!(quotes[0]["startByte"], 0);
    assert_eq!(quotes[0]["endByte"], A.len());
    let context = invoke(
        &mut f,
        "ci_context",
        json!({"graphViewId":view.id,"query":"Pipeline","kind":"meaning_concept","path":"a.rs","limit":1,"depth":0,"maxSourceBytes":1000,"maxBytes":256000}),
    );
    let sources = context["sources"].as_array().unwrap();
    assert_eq!(sources.len(), 1);
    assert_eq!(sources[0]["snapshotId"], f.snapshot.id);
    assert_eq!(sources[0]["text"], A);
    assert!(sources[0].get("citationAnchor").is_some());
    assert!(sources[0].get("declarationAnchor").is_none());
    assert_eq!(f.engine.store.latest().unwrap(), Some(new.id));
}

#[test]
fn meaning_trace_preserves_direction_and_ordered_hyperedge_incidence_without_a_clique() {
    let mut f = fixture();
    let view = selected(&f);
    let value = serde_json::to_value(&view).unwrap();
    let packet = value["meaning"]["packets"]
        .as_array()
        .unwrap()
        .iter()
        .find(|packet| packet["relations"].as_array().unwrap().len() == 1)
        .unwrap();
    let relation = &packet["relations"][0];
    let left = relation["source"].clone();
    let right = relation["target"].clone();
    let hyperedge = &packet["hyperedges"][0];
    let outgoing = invoke(
        &mut f,
        "ci_trace",
        json!({"graphViewId":view.id,"entityIds":[left],"direction":"out","relations":["transfers"],"depth":1,"maxBytes":256000}),
    );
    assert_eq!(
        outgoing["meaning"]["relations"].as_array().unwrap().len(),
        1
    );
    assert!(
        outgoing["entities"]
            .as_array()
            .unwrap()
            .iter()
            .any(|n| n["id"] == right)
    );
    let incoming = invoke(
        &mut f,
        "ci_trace",
        json!({"graphViewId":view.id,"entityIds":[left],"direction":"in","relations":["transfers"],"depth":1,"maxBytes":256000}),
    );
    assert!(
        !incoming["entities"]
            .as_array()
            .unwrap()
            .iter()
            .any(|n| n["id"] == right)
    );
    let incident = invoke(
        &mut f,
        "ci_trace",
        json!({"graphViewId":view.id,"entityIds":[left],"direction":"both","relations":["coordinates"],"depth":1,"maxBytes":256000}),
    );
    assert_eq!(
        incident["meaning"]["hyperedges"][0]["participants"],
        json!([{"role":"input","concept":left},{"role":"output","concept":right}])
    );
    assert!(
        incident["entities"]
            .as_array()
            .unwrap()
            .iter()
            .any(|n| n["id"] == hyperedge["id"])
    );
    assert!(
        !incident["entities"]
            .as_array()
            .unwrap()
            .iter()
            .any(|n| n["id"] == right),
        "Hyperedge traversal silently created a one-hop clique: {incident}"
    );
    let expanded = invoke(
        &mut f,
        "ci_trace",
        json!({"graphViewId":view.id,"entityIds":[left],"direction":"both","relations":["coordinates"],"depth":2,"maxBytes":256000}),
    );
    assert!(
        expanded["entities"]
            .as_array()
            .unwrap()
            .iter()
            .any(|n| n["id"] == right)
    );
    let file = f
        .graph
        .entities
        .iter()
        .find(|n| n.kind == "file" && n.path == "a.rs")
        .unwrap()
        .id
        .clone();
    let mixed = invoke(
        &mut f,
        "ci_trace",
        json!({"graphViewId":view.id,"entityIds":[file],"direction":"both","relations":["cites_source"],"depth":1,"maxBytes":256000}),
    );
    assert!(
        mixed["entities"]
            .as_array()
            .unwrap()
            .iter()
            .any(|n| n["id"] == left)
    );
    assert!(
        !mixed["meaning"]["citationLinks"]
            .as_array()
            .unwrap()
            .is_empty()
    );
}

#[test]
fn source_only_projection_consumers_refuse_meaning_before_report_or_archive_publication() {
    let f = fixture();
    let view = selected(&f);
    let before: i64 = f
        .engine
        .store
        .db
        .query_row(
            "SELECT count(*) FROM records WHERE kind IN ('artifact','native_graph_view_report_v1')",
            [],
            |row| row.get(0),
        )
        .unwrap();
    for format in ["json", "svg", "canvas"] {
        assert!(
            graph_view::export(&f.engine.store, &view, format, &d()).is_err(),
            "Untyped {format} projection silently discarded a selected meaning layer"
        );
    }
    assert!(
        yoke::graph_extensions::analyze_view(&f.engine.store, &view, &Default::default(), &d())
            .is_err(),
        "Source-only analysis published a report that omitted its selected meaning layer"
    );
    let after: i64 = f
        .engine
        .store
        .db
        .query_row(
            "SELECT count(*) FROM records WHERE kind IN ('artifact','native_graph_view_report_v1')",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(after, before);
    let directory = TempDir::new().unwrap();
    let registry = yoke::graph_collections::Registry::open(directory.path()).unwrap();
    assert!(
        registry
            .register_view("meaning", &f.engine.store, &view, &d())
            .is_err(),
        "Legacy single-snapshot archive silently omitted meaning or its historical sources"
    );
    assert_eq!(registry.list(&d()).unwrap()["count"], 0);
}

#[test]
fn two_published_meaning_graphs_keep_equal_local_claims_in_separate_namespaces() {
    let mut f = fixture();
    assert_ne!(f.meaning_id, f.second_meaning_id);
    let options: BuildOptions = serde_json::from_value(
        json!({"structuralKinds":[],"meaningGraphIds":[f.second_meaning_id,f.meaning_id]}),
    )
    .unwrap();
    let view = graph_view::build(&f.engine.store, &f.snapshot, &f.graph, &options, &d()).unwrap();
    let found = invoke(
        &mut f,
        "ci_find",
        json!({"graphViewId":view.id,"query":"Pipeline","kind":"meaning_concept","limit":20,"maxBytes":256000}),
    );
    let ids: BTreeSet<_> = found["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item["id"].as_str().unwrap())
        .collect();
    assert_eq!(
        ids.len(),
        8,
        "Published graph namespaces collapsed equal packet claims: {found}"
    );
    let again: BuildOptions = serde_json::from_value(
        json!({"structuralKinds":[],"meaningGraphIds":[f.meaning_id,f.second_meaning_id]}),
    )
    .unwrap();
    let reordered =
        graph_view::build(&f.engine.store, &f.snapshot, &f.graph, &again, &d()).unwrap();
    assert_eq!(reordered.id, view.id);
    assert_eq!(reordered.artifact_id, view.artifact_id);
}

#[test]
fn multi_file_relation_and_hyperedge_reads_preserve_all_citations_and_shared_context_budget() {
    let mut f = fixture();
    let view = selected(&f);
    let value = serde_json::to_value(&view).unwrap();
    let packet = value["meaning"]["packets"]
        .as_array()
        .unwrap()
        .iter()
        .find(|packet| packet["relations"].as_array().unwrap().len() == 1)
        .unwrap();
    for id in [
        packet["relations"][0]["id"].clone(),
        packet["hyperedges"][0]["id"].clone(),
    ] {
        let read = invoke(
            &mut f,
            "ci_read",
            json!({"graphViewId":view.id,"entityId":id,"maxBytes":256000}),
        );
        let citations = read["citations"].as_array().unwrap();
        assert_eq!(
            citations.len(),
            2,
            "Multi-file claim lost its own supporting source: {read}"
        );
        for (path, text) in [("a.rs", A), ("b.rs", B)] {
            let citation = citations
                .iter()
                .find(|citation| citation["path"] == path)
                .unwrap();
            assert_eq!(citation["text"], text);
            assert_eq!(citation["contentHash"], hash(text));
            assert_eq!(citation["snapshotId"], f.snapshot.id);
        }
        assert!(read.get("declarationAnchor").is_none());
        for mut conflict in [
            json!({"path":"a.rs"}),
            json!({"startByte":0,"endByte":1}),
            json!({"startLine":1,"endLine":1}),
            json!({"path":"b.rs","startByte":0,"endLine":1}),
        ] {
            conflict["entityId"] = id.clone();
            assert!(
                graph_view::read_meaning(&f.engine.store, &view, &conflict, &d()).is_err(),
                "A path/range selector redefined a typed meaning citation read: {conflict}"
            );
            conflict["graphViewId"] = json!(view.id);
            let refused = f.engine.invoke("ci_read", conflict).unwrap();
            assert_eq!(refused["status"], "error", "{refused}");
            assert_eq!(refused["metrics"]["modelCalls"], 0);
            assert_eq!(refused["metrics"]["languageServerAttempts"], 0);
        }
    }
    let context = invoke(
        &mut f,
        "ci_context",
        json!({"graphViewId":view.id,"query":"coordinates","kind":"meaning_hyperedge","limit":1,"depth":0,"maxSourceBytes":100,"maxBytes":256000}),
    );
    assert_eq!(context["selected"].as_array().unwrap().len(), 1);
    assert!(context["sourceBytes"].as_u64().unwrap() <= 100);
    assert_eq!(context["sourceTruncated"], true);
    let bytes: usize = context["sources"]
        .as_array()
        .unwrap()
        .iter()
        .map(|source| source["text"].as_str().unwrap().len())
        .sum();
    assert_eq!(context["sourceBytes"], bytes);
    for source in context["sources"].as_array().unwrap() {
        assert!(source.get("citationAnchor").is_some());
        assert!(source.get("declarationAnchor").is_none());
    }
    // Context may expose bounded evidence metadata; it must not repeat unbudgeted full quotations.
    for evidence in context["neighborhood"]["meaning"]["evidence"]
        .as_array()
        .unwrap()
    {
        assert!(
            evidence.get("text").is_none(),
            "Quote bypassed maxSourceBytes in a second field: {context}"
        );
    }
}

#[test]
fn inferred_support_and_hyperedge_roles_never_become_directed_or_exact_bindings() {
    let mut f = fixture();
    let view = selected(&f);
    let value = serde_json::to_value(&view).unwrap();
    let packet = value["meaning"]["packets"]
        .as_array()
        .unwrap()
        .iter()
        .find(|packet| packet["hyperedges"].as_array().unwrap().len() == 1)
        .unwrap();
    let left = packet["hyperedges"][0]["participants"][0]["concept"].clone();
    let right = packet["hyperedges"][0]["participants"][1]["concept"].clone();
    for direction in ["in", "out"] {
        let result = invoke(
            &mut f,
            "ci_trace",
            json!({"graphViewId":view.id,"entityIds":[left],"direction":direction,"relations":["coordinates","cites_source"],"depth":8,"maxBytes":256000}),
        );
        assert!(
            !result["entities"]
                .as_array()
                .unwrap()
                .iter()
                .any(|node| node["id"] == right)
        );
        assert!(
            !result["entities"]
                .as_array()
                .unwrap()
                .iter()
                .any(|node| node["kind"] == "file")
        );
    }
    let exact = invoke(
        &mut f,
        "ci_trace",
        json!({"graphViewId":view.id,"entityIds":[left],"direction":"both","resolutions":["exact_binding"],"depth":8,"maxBytes":256000}),
    );
    assert_eq!(exact["entities"].as_array().unwrap().len(), 1);
    assert!(exact["meaning"]["relations"].as_array().unwrap().is_empty());
    assert!(
        exact["meaning"]["hyperedges"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert!(
        exact["meaning"]["citationLinks"]
            .as_array()
            .unwrap()
            .is_empty()
    );
}

#[test]
fn invalid_meaning_selection_is_admitted_before_default_structural_artifact_publication() {
    let mut f = fixture();
    fs::write(f.engine.root.join("new.md"), "# A new captured source\n").unwrap();
    let (new_snapshot, new_graph, _, _) = f.engine.index(&d()).unwrap();
    let before: i64 = f
        .engine
        .store
        .db
        .query_row("SELECT count(*) FROM records", [], |row| row.get(0))
        .unwrap();
    for (snapshot, graph, ids) in [
        (&new_snapshot, &new_graph, vec![f.meaning_id.clone()]),
        (
            &f.snapshot,
            &f.graph,
            vec![f.meaning_id.clone(), f.meaning_id.clone()],
        ),
        (&f.snapshot, &f.graph, vec!["0".repeat(64)]),
    ] {
        // Default structural kinds intentionally remain enabled to detect early publication.
        let options: BuildOptions = serde_json::from_value(json!({"meaningGraphIds":ids})).unwrap();
        assert!(graph_view::build(&f.engine.store, snapshot, graph, &options, &d()).is_err());
        let after: i64 = f
            .engine
            .store
            .db
            .query_row("SELECT count(*) FROM records", [], |row| row.get(0))
            .unwrap();
        assert_eq!(
            after, before,
            "Rejected meaning selection published default overlay or View state"
        );
    }
    assert_eq!(fs::read_to_string(&f.marker).unwrap().lines().count(), 3);
}

#[test]
fn published_view_queries_and_archived_meaning_keep_identity_after_later_ledger_changes() {
    let mut f = fixture();
    let view = selected(&f);
    let found = invoke(
        &mut f,
        "ci_find",
        json!({"graphViewId":view.id,"query":"Pipeline","kind":"meaning_concept","limit":20,"maxBytes":256000}),
    );
    let id = found["items"][0]["id"].clone();
    let read = invoke(
        &mut f,
        "ci_read",
        json!({"graphViewId":view.id,"entityId":id,"maxBytes":256000}),
    );
    let layer = view.meaning.as_ref().unwrap();
    let original = &layer.packets[0].provenance;
    assert_eq!(original.provider_reported_cost_micros, Some(2));
    assert_eq!(
        original
            .provider_reported_usage
            .as_ref()
            .unwrap()
            .input_tokens,
        4
    );
    let deadline = d();
    let ledger = yoke::ledger::Ledger::new(&f.engine.store, &deadline);
    let later = ledger
        .reserve(
            "meaning-view-owned-fixture",
            20,
            30,
            "EUR",
            "meaning-view-owned-fixture@v1",
        )
        .unwrap();
    ledger.mark_sent(&later).unwrap();
    ledger.settle(&later, Some(20)).unwrap();
    ledger
        .usage(
            &original.attempt_id,
            "later-host-token-reconciliation",
            yoke::ledger::Usage {
                input_tokens: 12345,
                output_tokens: 678,
                cached_input_tokens: 12,
            },
        )
        .unwrap();
    let reread = graph_view::load(&f.engine.store, &view.id, &d())
        .expect("Later task/attempt row sizes invalidated a published immutable View");
    assert_eq!(
        serde_json::to_value(&reread).unwrap(),
        serde_json::to_value(&view).unwrap()
    );
    let found_after = invoke(
        &mut f,
        "ci_find",
        json!({"graphViewId":view.id,"query":"Pipeline","kind":"meaning_concept","limit":20,"maxBytes":256000}),
    );
    assert_eq!(found_after, found);
    let read_after = invoke(
        &mut f,
        "ci_read",
        json!({"graphViewId":view.id,"entityId":id,"maxBytes":256000}),
    );
    assert_eq!(read_after, read);
    // The typed proof archive is supported independently of the source-only collection facade.
    let archive = yoke::meaning_view::archive_selected(
        &f.engine.store,
        &f.snapshot,
        &f.graph,
        layer,
        32_000_000,
        64_000_000,
        &d(),
    )
    .unwrap();
    let mut source = |_: &Snapshot, file: &yoke::model::SourceFile, _: &Deadline| {
        f.engine
            .store
            .read_blob_exact(&file.hash, file.size, 4_000_000)
    };
    let replayed = yoke::meaning_view::replay_archive(
        &f.snapshot,
        &f.graph,
        &archive,
        &mut source,
        32_000_000,
        64_000_000,
        &d(),
    )
    .unwrap();
    assert_eq!(&replayed, layer);
    assert_eq!(fs::read_to_string(&f.marker).unwrap().lines().count(), 3);
}

#[test]
fn every_untyped_graphify_tool_and_resource_refuses_a_genuine_meaning_view_without_side_effects() {
    let f = fixture();
    let view = selected(&f);
    assert!(view.meaning.is_some());
    let marker = fs::read(&f.marker).unwrap();
    let mut session = yoke::project_session::Session::open(
        f.engine,
        yoke::project_session::HostProfile::default(),
        Some("meaning-untyped-adapter-refusal"),
        &d(),
    )
    .unwrap();
    let counts = |session: &yoke::project_session::Session| {
        let count = |sql: &str| {
            session
                .engine
                .store
                .db
                .query_row(sql, [], |row| row.get::<_, i64>(0))
                .unwrap()
        };
        (
            count("SELECT count(*) FROM snapshots"),
            count("SELECT count(*) FROM graphs"),
            count("SELECT count(*) FROM records"),
            count("SELECT count(*) FROM records WHERE kind='artifact'"),
        )
    };
    let before = counts(&session);
    fs::write(
        session.engine.root.join("a.rs"),
        "pub fn live_changed() {}\n",
    )
    .unwrap();
    let mut accepted = Vec::new();
    for (name, mut args) in [
        (
            "query_graph",
            json!({"question":"alpha","depth":1,"token_budget":65536}),
        ),
        ("get_node", json!({"label":"alpha"})),
        (
            "get_neighbors",
            json!({"label":"alpha","token_budget":65536}),
        ),
        (
            "get_community",
            json!({"community_id":0,"token_budget":65536}),
        ),
        ("god_nodes", json!({"top_n":10})),
        ("graph_stats", json!({})),
        (
            "shortest_path",
            json!({"source":"alpha","target":"beta","max_hops":8}),
        ),
    ] {
        args["graphViewId"] = json!(view.id);
        if yoke::graphify_adapter::invoke(&mut session, name, &args, &d()).is_ok() {
            accepted.push(name.to_owned());
        }
    }
    for name in [
        "report",
        "stats",
        "god-nodes",
        "surprises",
        "audit",
        "questions",
    ] {
        let uri = format!("graphify://{name}?graphViewId={}", view.id);
        if yoke::graphify_adapter::resource(&mut session, &uri, &d()).is_ok() {
            accepted.push(uri);
        }
    }
    assert_eq!(
        counts(&session),
        before,
        "Refused adapter reads published state"
    );
    assert_eq!(fs::read(&f.marker).unwrap(), marker);
    assert!(
        accepted.is_empty(),
        "Untyped Graphify consumers silently discarded a selected meaning layer: {accepted:?}"
    );
}

#[test]
fn later_semantic_selection_failure_leaves_prepared_defaults_unpublished() {
    let f = fixture();
    assert!(selected(&f).meaning.is_some());
    let state = |f: &Fixture| {
        let db = &f.engine.store.db;
        let names: Vec<String> = db
            .prepare("SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%' ORDER BY name")
            .unwrap()
            .query_map([], |row| row.get(0))
            .unwrap()
            .collect::<rusqlite::Result<_>>()
            .unwrap();
        let tables: BTreeMap<_, _> = names
            .into_iter()
            .map(|name| {
                let count: i64 = db
                    .query_row(&format!("SELECT count(*) FROM {name}"), [], |row| {
                        row.get(0)
                    })
                    .unwrap();
                (name, count)
            })
            .collect();
        let records: Vec<(String, String, String)> = db
            .prepare("SELECT kind,id,value FROM records ORDER BY kind,id")
            .unwrap()
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))
            .unwrap()
            .collect::<rusqlite::Result<_>>()
            .unwrap();
        let blobs: BTreeMap<_, _> = fs::read_dir(f.engine.store.dir.join("blobs"))
            .unwrap()
            .map(|entry| {
                let entry = entry.unwrap();
                (
                    entry.file_name().into_string().unwrap(),
                    entry.metadata().unwrap().len(),
                )
            })
            .collect();
        (tables, identity(&records), blobs)
    };
    let before = state(&f);
    let marker = fs::read(&f.marker).unwrap();
    let error = graph_view::build(
        &f.engine.store,
        &f.snapshot,
        &f.graph,
        &BuildOptions {
            meaning_graph_ids: vec![f.meaning_id.clone()],
            semantic_overlay_ids: vec!["0".repeat(64)],
            ..BuildOptions::default()
        },
        &d(),
    )
    .unwrap_err();
    assert!(
        format!("{error:#}").contains("Semantic overlay record absent or ambiguous"),
        "The fixture must reach later semantic selection after all default preparation: {error:#}"
    );
    assert_eq!(state(&f), before);
    assert_eq!(fs::read(&f.marker).unwrap(), marker);
}

#[test]
fn conflicting_view_registration_is_refused_before_any_prepared_or_view_cas_write() {
    let actual_dir = TempDir::new().unwrap();
    let probe_dir = TempDir::new().unwrap();
    let make = |root: &std::path::Path| {
        for (path, text) in [
            ("main.rs", "fn alpha() {}\n"),
            ("package.json", r#"{"name":"conflict-fixture"}"#),
            ("settings.json", r#"{"mode":"local"}"#),
            ("guide.md", "# Scope\n"),
        ] {
            fs::write(root.join(path), text).unwrap();
        }
        let mut engine = Engine::open(root).unwrap();
        // A shared explicit identity before capture makes both normal captured scopes equal.
        // The tested Store never receives the probe's derived records or CAS blobs.
        let workspace = hash("view-registration-conflict-owned-scope");
        engine
            .store
            .db
            .execute(
                "UPDATE meta SET value=?1 WHERE key='workspace'",
                [&workspace],
            )
            .unwrap();
        engine.store.workspace_id = workspace;
        engine.capture_epoch = yoke::analysis::AnalyzerEpoch::V9;
        let (snapshot, graph, _, _) = engine.index(&d()).unwrap();
        (engine, snapshot, graph)
    };
    let (probe, probe_snapshot, probe_graph) = make(probe_dir.path());
    let wanted = graph_view::build(
        &probe.store,
        &probe_snapshot,
        &probe_graph,
        &BuildOptions::default(),
        &d(),
    )
    .unwrap();
    let mut conflict: Value = probe
        .store
        .get_record(graph_view::RECORD_KIND, &wanted.id)
        .unwrap();
    conflict["artifactId"] = json!("0".repeat(64));
    let (actual, snapshot, graph) = make(actual_dir.path());
    assert_eq!(snapshot.id, probe_snapshot.id);
    assert_eq!(identity(&graph), identity(&probe_graph));
    for artifact in std::iter::once(wanted.artifact_id.as_str()).chain(
        wanted
            .overlays
            .iter()
            .map(|overlay| overlay["artifactId"].as_str().unwrap()),
    ) {
        assert!(
            !actual.store.dir.join("blobs").join(artifact).exists(),
            "The actual Store already contains a probe-derived artifact"
        );
    }
    actual
        .store
        .record(graph_view::RECORD_KIND, &wanted.id, &conflict)
        .unwrap();
    let state = |engine: &Engine| {
        let db = &engine.store.db;
        let names: Vec<String> = db
            .prepare("SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%' ORDER BY name")
            .unwrap()
            .query_map([], |row| row.get(0))
            .unwrap()
            .collect::<rusqlite::Result<_>>()
            .unwrap();
        let tables: BTreeMap<_, _> = names
            .into_iter()
            .map(|name| {
                let count: i64 = db
                    .query_row(&format!("SELECT count(*) FROM {name}"), [], |row| {
                        row.get(0)
                    })
                    .unwrap();
                (name, count)
            })
            .collect();
        let records: Vec<(String, String, String)> = db
            .prepare("SELECT kind,id,value FROM records ORDER BY kind,id")
            .unwrap()
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))
            .unwrap()
            .collect::<rusqlite::Result<_>>()
            .unwrap();
        let blobs: BTreeMap<_, _> = fs::read_dir(engine.store.dir.join("blobs"))
            .unwrap()
            .map(|entry| {
                let entry = entry.unwrap();
                (
                    entry.file_name().into_string().unwrap(),
                    entry.metadata().unwrap().len(),
                )
            })
            .collect();
        (tables, identity(&records), blobs)
    };
    let before = state(&actual);
    let error = graph_view::build(
        &actual.store,
        &snapshot,
        &graph,
        &BuildOptions::default(),
        &d(),
    )
    .unwrap_err();
    assert!(
        format!("{error:#}").contains("Stored graph view record differs"),
        "The fixture must reach its known View registration conflict: {error:#}"
    );
    assert_eq!(
        state(&actual),
        before,
        "A predictable View registration conflict left prepared or View CAS blobs"
    );
}
