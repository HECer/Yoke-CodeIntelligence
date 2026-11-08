//! Checked query capabilities retain exact replay and original observation positions.
use serde_json::{Value, json};
use std::{fs, io::Read, path::PathBuf};
use yoke::{
    capture::Deadline,
    mixed_view::{
        self, MeaningInputRequest, Query,
        provider::{self, Policy, SourceSelection},
    },
    model::hash,
    providers::ProviderSpec,
};
#[allow(dead_code)]
#[path = "support/mixed_pdf_fixture.rs"]
mod support;
use support::*;

fn inputs(f: &Fixture, d: &Deadline) -> MeaningInputRequest {
    let view = mixed_view::load_with_pdf(
        &f.engine.store,
        &f.selection,
        &worker(),
        &Default::default(),
        d,
    )
    .unwrap();
    let query: Query =
        serde_json::from_value(json!({"operation":"find","query":"storage","ignoreCase":true}))
            .unwrap();
    let result = view.query(&query, 4_000_000, d).unwrap();
    let mut units: Vec<_> = result["items"]
        .as_array()
        .unwrap()
        .iter()
        .enumerate()
        .map(|(n, item)| json!({"label":format!("u{n}"),"source":item["source"]}))
        .collect();
    units.push(json!({"label":"unicode","source":{"kind":"sourceUtf8","viewId":f.selection.source.view_id,"snapshotId":f.selection.source.snapshot_id,
        "path":"billing.rs","contentHash":hash(SOURCE),"startByte":0,"endByte":SOURCE.len()}}));
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(
        fs::read(f.dir.path().join("office.docx")).unwrap(),
    ))
    .unwrap();
    let mut xml = String::new();
    zip.by_name("word/document.xml")
        .unwrap()
        .read_to_string(&mut xml)
        .unwrap();
    units.push(json!({"label":"xml","source":{"kind":"documentPartXml","documentGraphId":f.selection.documents[0].document_graph_id,
        "part":"word/document.xml","contentHash":hash(&xml),"startByte":0,"endByte":xml.len()}}));
    serde_json::from_value(json!({"schemaVersion":1,"units":units,
        "contextGroups":[{"label":"ordered","units":["u2","u0","u1","u2"]}]}))
    .unwrap()
}
fn configured(f: &Fixture, mode: &str) -> (ProviderSpec, PathBuf, PathBuf) {
    let script = f.dir.path().join("owned-mixed-worker.cjs");
    let marker = f.dir.path().join("provider-calls.log");
    fs::write(&script,r#"
let raw='';process.stdin.setEncoding('utf8');process.stdin.on('data',b=>raw+=b);process.stdin.on('end',()=>{
 const r=JSON.parse(raw),fs=require('fs'),mode=process.argv[3];fs.appendFileSync(process.argv[2],r.requestId+'\n');
 const units=r.payload.units;
 const supports=units.map(u=>{const p=u.text.indexOf('🌍'),start=u.source.startByte+(p<0?0:Buffer.byteLength(u.text.slice(0,p),'utf8'));
 return {sourceId:u.sourceId,startByte:start,endByte:start+(p<0?1:4)};});
 const concepts=[{key:'a',label:'Storage € 🌍',description:'Provider interpretation A',aliases:['same label'],support:[supports[0]]},
 {key:'c',label:'Hyper only',description:'Hyperedge participation',aliases:[],support:[supports[0]]},
 {key:'d',label:'Isolated',description:'No relation or hyperedge',aliases:[],support:[supports[0]]},
 {key:'b',label:'Storage',description:'Provider interpretation B',aliases:['same label'],support:[supports[1]]}];
 const edge={source:'a',target:'b',relation:'relates',description:'Unverified directed observation',support:supports};
 const data={concepts,relations:[edge,edge,{source:'b',target:'a',relation:'reverse',description:'Reverse observation',support:supports},{...edge,target:'a'}],
 hyperedges:[{relation:'joint',participants:[{role:'input',concept:'a'},{role:'output',concept:'b'},{role:'aux',concept:'a'},{role:'other',concept:'c'}],description:'Ordered role observation',support:supports}],
 coverage:units.map(u=>({sourceId:u.sourceId,outcome:'claims'}))};
 if(mode==='invalid-fragment')data.concepts[0].support[0]={...supports[0],sourceId:'0'.repeat(64)};
 if(mode==='split-utf8')supports.find(s=>s.endByte-s.startByte===4).startByte++;
 const reply={schema:1,operation:r.operation,model:r.model,modelVersion:r.modelVersion,requestId:r.requestId,inputHash:r.inputHash,
 sourceBindings:units.map(u=>({sourceId:u.sourceId,source:u.source})),currency:r.currency,actualCostMicros:4,
 usage:{inputTokens:12,outputTokens:7,cachedInputTokens:0},data};
 if(mode==='unbound')reply.inputHash='0'.repeat(64);
 if(mode==='unknown-cost')reply.actualCostMicros=null;
 if(mode==='invalid-usage')reply.usage.cachedInputTokens=99;
 if(mode==='overrun')reply.actualCostMicros=11;
 if(mode==='provider-error')reply.error='NEVER_ECHO_PROVIDER_ERROR_SENTINEL';
 if(mode==='invalid-utf8')process.stdout.write(Buffer.from([255]));
 else if(mode==='duplicate-envelope')process.stdout.write(JSON.stringify(reply).replace('"schema":1','"schema":1,"schema":1'));
 else process.stdout.write(JSON.stringify(reply));
});
"#).unwrap();
    (
        ProviderSpec {
            trusted: true,
            command: PathBuf::from("node"),
            args: vec![
                script.to_str().unwrap().into(),
                marker.to_str().unwrap().into(),
                mode.into(),
            ],
            model_id: "owned-mixed-provider".into(),
            model_version: "v1".into(),
            task_id: "owned-mixed-provider-task".into(),
            currency: "EUR".into(),
            max_cost_micros: 10,
            task_limit_micros: 100,
        },
        script,
        marker,
    )
}
fn selection(f: &Fixture) -> SourceSelection {
    SourceSelection::Pdf {
        selection: f.selection.clone(),
    }
}
fn policy() -> Policy {
    Policy::new(hash("owned mixed interpretation contract"))
}

fn checked() -> (Fixture, provider::CheckedGraph, PathBuf) {
    let f = fixture();
    let d = Deadline::new(60000);
    let (spec, script, marker) = configured(&f, "success");
    let result = provider::build(
        &f.engine.store,
        &selection(&f),
        &inputs(&f, &d),
        Some(&worker()),
        &spec,
        &policy(),
        &d,
    )
    .unwrap();
    let id = result["jobId"].as_str().unwrap();
    refused(
        provider::load_checked(&f.engine.store, id, None, &d).map(|_| ()),
        "mixed_view_pdf_permission_required",
    );
    let graph = provider::load_checked(&f.engine.store, id, Some(&worker()), &d).unwrap();
    assert_eq!(
        graph.export(16_000_000, &d).unwrap(),
        provider::load(&f.engine.store, id, Some(&worker()), &d).unwrap()
    );
    fs::remove_file(script).unwrap();
    (f, graph, marker)
}
fn run(g: &provider::CheckedGraph, q: Value) -> Value {
    let q = yoke::json::decode(q).unwrap();
    g.query(&q, 16_000_000, &Deadline::new(60000)).unwrap()
}
fn reference(g: &provider::CheckedGraph, key: &str) -> Value {
    json!({"graphId":g.id(),"key":key})
}
fn keys(result: &Value) -> Vec<&str> {
    result["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|n| n["reference"]["key"].as_str().unwrap())
        .collect()
}
fn indices(result: &Value) -> Vec<u64> {
    result["relations"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["index"].as_u64().unwrap())
        .collect()
}
#[test]
fn degrees_preserve_parallel_rows_self_loops_and_ordered_repeated_hyperedge_slots() {
    let (f, g, marker) = checked();
    let before = state(&f.engine);
    let result = run(&g, json!({"operation":"degree"}));
    let a = result["degrees"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["reference"]["key"] == "a")
        .unwrap();
    assert_eq!(a["inDegree"], 2);
    assert_eq!(a["outDegree"], 3);
    assert_eq!(a["hyperedgeSlots"], 2);
    assert_eq!(indices(&result), vec![0, 1, 2, 3]);
    assert_eq!(
        result["relations"][0]["claim"],
        result["relations"][1]["claim"]
    );
    assert_ne!(
        result["relations"][0]["index"],
        result["relations"][1]["index"]
    );
    assert_eq!(
        result["hyperedges"][0]["claim"]["participants"],
        json!([
        {"role":"input","concept":"a"},{"role":"output","concept":"b"},
        {"role":"aux","concept":"a"},{"role":"other","concept":"c"}])
    );
    let d = result["degrees"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["reference"]["key"] == "d")
        .unwrap();
    assert_eq!(d["inDegree"], 0);
    assert_eq!(d["outDegree"], 0);
    assert_eq!(d["hyperedgeSlots"], 0);
    assert_eq!(
        result["originalGraph"],
        g.export(16_000_000, &Deadline::new(60000)).unwrap()
    );
    assert_eq!(result["originalGraph"]["modelCallsThisRun"], 1);
    assert_eq!(result["providerCallsThisRead"], 0);
    assert_eq!(result["complete"], false);
    assert_eq!(state(&f.engine), before);
    assert_eq!(
        fs::read(marker)
            .unwrap()
            .iter()
            .filter(|b| **b == b'\n')
            .count(),
        1
    );
}
#[test]
fn trace_obeys_direction_depth_and_capacity_without_inventing_hyperedge_flow() {
    let (_f, g, _marker) = checked();
    for (direction, want) in [
        ("outgoing", vec![0, 1, 3]),
        ("incoming", vec![2, 3]),
        ("both", vec![0, 1, 2, 3]),
    ] {
        let r = run(
            &g,
            json!({"operation":"trace","roots":[reference(&g,"a")],"direction":direction,"depth":1,"maxVisited":2}),
        );
        assert_eq!(keys(&r), vec!["a", "b"]);
        assert_eq!(indices(&r), want);
    }
    let zero = run(
        &g,
        json!({"operation":"trace","roots":[reference(&g,"a")],"direction":"both","depth":0,"maxVisited":1}),
    );
    assert_eq!(keys(&zero), vec!["a"]);
    assert!(indices(&zero).is_empty());
    let hyper = run(
        &g,
        json!({"operation":"trace","roots":[reference(&g,"c")],"direction":"both","depth":8,"maxVisited":1}),
    );
    assert_eq!(keys(&hyper), vec!["c"]);
    assert!(indices(&hyper).is_empty());
    assert_eq!(hyper["hyperedges"].as_array().unwrap().len(), 1);
    let q=yoke::json::decode(json!({"operation":"trace","roots":[reference(&g,"a")],"direction":"outgoing","depth":1,"maxVisited":1})).unwrap();
    refused(
        g.query(&q, 16_000_000, &Deadline::new(60000)),
        "mixed_view_capacity_exceeded",
    );
}
#[test]
fn literal_unicode_search_keeps_ambiguous_labels_separate_and_node_references_graph_qualified() {
    let (_f, g, _marker) = checked();
    assert_eq!(
        keys(&run(&g, json!({"operation":"find","query":"🌍"}))),
        vec!["a"]
    );
    assert_eq!(
        keys(&run(&g, json!({"operation":"find","query":"same label"}))),
        vec!["a", "b"]
    );
    let r = run(&g, json!({"operation":"find","query":"storage"}));
    assert!(keys(&r).is_empty());
    assert_eq!(r["selectionComplete"], true);
    assert_eq!(r["complete"], false);
    let r = run(
        &g,
        json!({"operation":"node","reference":reference(&g,"a")}),
    );
    assert_eq!(keys(&r), vec!["a"]);
    assert_eq!(indices(&r), vec![0, 1, 2, 3]);
    let q = yoke::json::decode(
        json!({"operation":"node","reference":{"graphId":hash("another graph"),"key":"a"}}),
    )
    .unwrap();
    refused(
        g.query(&q, 16_000_000, &Deadline::new(60000)),
        "mixed_meaning_query_invalid",
    );
}
#[test]
fn checked_graph_remains_frozen_after_metadata_corruption_and_entire_store_loss() {
    let (f, g, marker) = checked();
    let original = run(&g, json!({"operation":"degree"}));
    let graph = g.export(16_000_000, &Deadline::new(60000)).unwrap();
    f.engine
        .store
        .db
        .execute(
            "UPDATE records SET value='{}' WHERE kind=?1 AND id=?2",
            rusqlite::params![provider::JOB_KIND, graph["jobId"].as_str().unwrap()],
        )
        .unwrap();
    assert!(
        provider::load_checked(
            &f.engine.store,
            graph["jobId"].as_str().unwrap(),
            Some(&worker()),
            &Deadline::new(60000)
        )
        .is_err()
    );
    assert_eq!(run(&g, json!({"operation":"degree"})), original);
    assert_eq!(
        fs::read(marker)
            .unwrap()
            .iter()
            .filter(|b| **b == b'\n')
            .count(),
        1
    );
    drop(f.engine);
    fs::remove_dir_all(f.dir.path()).unwrap();
    assert!(!f.dir.path().exists());
    assert_eq!(run(&g, json!({"operation":"degree"})), original);
}
#[test]
fn closed_queries_reject_invalid_keys_unknown_fields_duplicate_members_and_out_of_bounds_requests()
{
    use provider::query::{Query, validate};
    let d = Deadline::new(60000);
    for value in [
        json!({"operation":"degree","extra":true}),
        json!({"operation":"node","reference":{"graphId":hash("g"),"key":"a","path":"x"}}),
    ] {
        assert!(yoke::json::decode::<Query>(value).is_err());
    }
    assert!(yoke::json::parse(br#"{"operation":"degree","operation":"degree"}"#, 64000).is_err());
    let r = json!({"graphId":hash("g"),"key":"a"});
    for value in [
        json!({"operation":"find","query":""}),
        json!({"operation":"find","query":"x".repeat(1025)}),
        json!({"operation":"find","query":"line\nbreak"}),
        json!({"operation":"node","reference":{"graphId":hash("g"),"key":"../a"}}),
        json!({"operation":"trace","roots":[],"direction":"both","depth":1,"maxVisited":128}),
        json!({"operation":"trace","roots":[r.clone(),r.clone()],"direction":"both","depth":1,"maxVisited":128}),
        json!({"operation":"trace","roots":[r.clone()],"direction":"both","depth":9,"maxVisited":128}),
        json!({"operation":"trace","roots":[r],"direction":"both","depth":1,"maxVisited":0}),
    ] {
        let q: Query = yoke::json::decode(value).unwrap();
        refused(validate(&q, &d), "mixed_meaning_query_invalid");
    }
}
#[test]
fn query_output_budget_and_absolute_deadline_fail_without_partial_result_or_writes() {
    let (f, g, marker) = checked();
    let before = state(&f.engine);
    let calls = fs::read(&marker).unwrap();
    let q = yoke::json::decode(json!({"operation":"degree"})).unwrap();
    refused(
        g.query(&q, 128, &Deadline::new(60000)),
        "mixed_view_capacity_exceeded",
    );
    refused(
        g.query(&q, 0, &Deadline::new(60000)),
        "mixed_view_capacity_exceeded",
    );
    refused(
        g.query(&q, 64_000_001, &Deadline::new(60000)),
        "mixed_view_input_invalid",
    );
    refused(
        g.query(&q, 16_000_000, &Deadline::new(0)),
        "mixed_view_deadline_exceeded",
    );
    assert_eq!(before, state(&f.engine));
    assert_eq!(calls, fs::read(marker).unwrap());
}
#[test]
fn cli_query_replays_with_separate_pdf_permission_and_no_provider_configuration() {
    use std::process::Command;
    let (f, g, marker) = checked();
    let graph = g.export(16_000_000, &Deadline::new(60000)).unwrap();
    let root = f.engine.root.clone();
    let id = graph["jobId"].as_str().unwrap();
    let path = f.dir.path().join("query.json");
    fs::write(&path, br#"{"operation":"degree"}"#).unwrap();
    let calls = fs::read(&marker).unwrap();
    drop(f.engine);
    let cli = |pdf: bool, bytes: &str| {
        let mut c = Command::new(env!("CARGO_BIN_EXE_yoke"));
        c.arg("--root")
            .arg(&root)
            .args([
                "--timeout",
                "60000",
                "--max-bytes",
                bytes,
                "mixed-meaning",
                "query",
                id,
                "--query",
            ])
            .arg(&path);
        if pdf {
            c.arg("--native-pdf-text");
        }
        c.output().unwrap()
    };
    let denied = cli(false, "4000000");
    assert!(!denied.status.success());
    assert!(denied.stdout.is_empty());
    assert!(String::from_utf8_lossy(&denied.stderr).contains("mixed_view_pdf_permission_required"));
    let result = cli(true, "4000000");
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let result: Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(result, run(&g, json!({"operation":"degree"})));
    let small = cli(true, "1024");
    assert!(!small.status.success());
    assert!(small.stdout.is_empty());
    assert_eq!(calls, fs::read(marker).unwrap());
}

#[test]
fn malformed_cli_query_is_private_and_refuses_before_missing_store_initialization() {
    let dir = tempfile::tempdir().unwrap();
    let query = dir.path().join("query.json");
    fs::write(
        &query,
        br#"{"operation":"degree","NEVER_ECHO_QUERY_KEY_SENTINEL":true}"#,
    )
    .unwrap();
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_yoke"))
        .arg("--root")
        .arg(dir.path())
        .args(["mixed-meaning", "query", &hash("missing job"), "--query"])
        .arg(query)
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    let error = String::from_utf8_lossy(&output.stderr);
    assert!(error.contains("mixed_meaning_query_invalid"), "{error}");
    assert!(!error.contains("NEVER_ECHO_QUERY_KEY_SENTINEL"));
    assert!(!dir.path().join(".yoke").exists());
}
