//! Explicit composition preserves independently checked parent identities and host assertion provenance.
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

use provider::composition::{self, Composition, Review};
fn r(g: &provider::CheckedGraph, key: &str) -> Value {
    json!({"graphId":g.id(),"key":key})
}
fn review(a: &provider::CheckedGraph, b: &provider::CheckedGraph) -> Review {
    yoke::json::decode(json!({"schemaVersion":1,"graphIds":[a.id(),b.id()],"links":[
        {"source":r(a,"a"),"target":r(b,"c"),"relation":"candidate","rationale":"Explicit unverified host link"},
        {"source":r(a,"a"),"target":r(b,"c"),"relation":"candidate","rationale":"Explicit unverified host link"},
        {"source":r(b,"c"),"target":r(a,"a"),"relation":"reverse","rationale":"Reverse assertion"},
        {"source":r(a,"a"),"target":r(a,"a"),"relation":"self","rationale":"Host self observation"}
    ]})).unwrap()
}
fn composed() -> (Fixture, Fixture, Composition, Vec<PathBuf>) {
    let (a, ga, ma) = checked();
    let (b, gb, mb) = checked();
    let review = review(&ga, &gb);
    let c = composition::compose(vec![ga, gb], &review, 64_000_000, &Deadline::new(60000)).unwrap();
    (a, b, c, vec![ma, mb])
}
fn query(c: &Composition, q: Value, host: bool) -> Value {
    c.query(
        &yoke::json::decode(q).unwrap(),
        host,
        64_000_000,
        &Deadline::new(60000),
    )
    .unwrap()
}
fn node_refs(value: &Value) -> Vec<Value> {
    value["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|n| n["reference"].clone())
        .collect()
}
fn trace(root: &Value, direction: &str, depth: usize, max: usize) -> Value {
    json!({"operation":"trace","roots":[root],"direction":direction,"depth":depth,"maxVisited":max})
}
#[test]
fn independent_workspaces_keep_qualified_concepts_exact_original_provenance_and_parallel_role_observations()
 {
    let (a, b, c, markers) = composed();
    let d = Deadline::new(60000);
    assert_ne!(a.engine.store.workspace_id, b.engine.store.workspace_id);
    let before = [state(&a.engine), state(&b.engine)];
    let result = c.export(64_000_000, &d).unwrap();
    assert_eq!(result["nodes"].as_array().unwrap().len(), 8);
    assert_eq!(result["relations"].as_array().unwrap().len(), 8);
    assert_eq!(result["hyperedges"].as_array().unwrap().len(), 2);
    assert_eq!(
        result["relations"][0]["claim"],
        result["relations"][1]["claim"]
    );
    assert_ne!(
        result["relations"][0]["reference"],
        result["relations"][1]["reference"]
    );
    assert_ne!(
        result["relations"][0]["reference"]["graphId"],
        result["relations"][4]["reference"]["graphId"]
    );
    for original in result["originalGraphs"].as_array().unwrap() {
        assert_eq!(
            original["sourceValidation"]["supportQuotes"]
                .as_array()
                .unwrap()
                .iter()
                .map(|q| q["item"]["source"]["kind"].as_str().unwrap())
                .collect::<std::collections::BTreeSet<_>>()
                .len(),
            4
        );
        assert_eq!(original["modelCallsThisRun"], 1);
        assert_eq!(original["providerReportedCostMicros"], 4);
        assert_eq!(original["providerAttestation"], "unverified");
    }
    assert_eq!(
        result["hyperedges"][0]["claim"]["participants"],
        result["originalGraphs"][0]["sourceValidation"]["originalFragment"]["hyperedges"][0]["participants"]
    );
    assert_eq!(result["hostLinks"].as_array().unwrap().len(), 4);
    assert_eq!(
        result["hostLinks"][0]["claim"],
        result["hostLinks"][1]["claim"]
    );
    assert_eq!(result["semanticIdentityEstablished"], false);
    assert_eq!(result["complete"], false);
    assert_eq!(result["providerCallsThisRead"], 0);
    assert_eq!(before, [state(&a.engine), state(&b.engine)]);
    for marker in markers {
        assert_eq!(
            fs::read(marker)
                .unwrap()
                .iter()
                .filter(|b| **b == b'\n')
                .count(),
            1
        );
    }
}
#[test]
fn explicit_host_link_traversal_is_separate_from_original_provider_relations_and_hyperedge_roles() {
    let (_a, _b, c, _) = composed();
    let exported = c.export(64_000_000, &Deadline::new(60000)).unwrap();
    let root = exported["nodes"][0]["reference"].clone();
    let other = exported["hostLinks"][0]["claim"]["target"].clone();
    let ordinary = query(&c, trace(&root, "outgoing", 1, 3), false);
    assert_eq!(ordinary["nodes"].as_array().unwrap().len(), 2);
    assert!(!node_refs(&ordinary).contains(&other));
    assert!(ordinary["hostLinks"].as_array().unwrap().is_empty());
    let cross = query(&c, trace(&root, "outgoing", 1, 3), true);
    assert_eq!(cross["nodes"].as_array().unwrap().len(), 3);
    assert!(node_refs(&cross).contains(&other));
    assert_eq!(cross["relations"].as_array().unwrap().len(), 3); // two parallel A->B plus A self-loop
    assert_eq!(
        cross["hostLinks"]
            .as_array()
            .unwrap()
            .iter()
            .map(|l| l["index"].as_u64().unwrap())
            .collect::<Vec<_>>(),
        vec![0, 1, 3]
    );
    assert_eq!(
        query(&c, trace(&root, "incoming", 1, 3), true)["hostLinks"]
            .as_array()
            .unwrap()
            .iter()
            .map(|l| l["index"].as_u64().unwrap())
            .collect::<Vec<_>>(),
        vec![2, 3]
    );
    let zero = query(&c, trace(&root, "both", 0, 1), true);
    assert_eq!(node_refs(&zero), vec![root]);
    assert!(zero["hostLinks"].as_array().unwrap().is_empty());
    let hyper = query(&c, trace(&other, "outgoing", 8, 1), false);
    assert_eq!(node_refs(&hyper), vec![other]);
    assert!(hyper["relations"].as_array().unwrap().is_empty());
}
#[test]
fn degrees_split_authored_links_from_parallel_original_relations_and_literals_do_not_merge_aliases()
{
    let (_a, _b, c, _) = composed();
    let result = query(&c, json!({"operation":"degree"}), true);
    let root = result["nodes"][0]["reference"].clone();
    let row = result["degrees"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["reference"] == root)
        .unwrap();
    assert_eq!(row["originalRelationInDegree"], 2);
    assert_eq!(row["originalRelationOutDegree"], 3);
    assert_eq!(row["hostLinkInDegree"], 2);
    assert_eq!(row["hostLinkOutDegree"], 3);
    assert_eq!(row["hyperedgeSlots"], 2);
    let default = query(&c, json!({"operation":"degree"}), false);
    assert!(
        default["degrees"]
            .as_array()
            .unwrap()
            .iter()
            .all(|r| r["hostLinkInDegree"] == 0 && r["hostLinkOutDegree"] == 0)
    );
    let aliases = query(&c, json!({"operation":"find","query":"same label"}), false);
    assert_eq!(aliases["nodes"].as_array().unwrap().len(), 4);
    let unicode = query(&c, json!({"operation":"find","query":"🌍"}), false);
    assert_eq!(unicode["nodes"].as_array().unwrap().len(), 2);
    let node = query(&c, json!({"operation":"node","reference":root}), false);
    assert_eq!(node["nodes"].as_array().unwrap().len(), 1);
}
#[test]
fn composition_survives_both_store_losses_and_never_upgrades_currentness_or_host_assertion_attestation()
 {
    let (a, b, c, markers) = composed();
    let original = query(&c, json!({"operation":"degree"}), true);
    for marker in markers {
        assert_eq!(
            fs::read(marker)
                .unwrap()
                .iter()
                .filter(|b| **b == b'\n')
                .count(),
            1
        );
    }
    drop(a.engine);
    drop(b.engine);
    fs::remove_dir_all(a.dir.path()).unwrap();
    fs::remove_dir_all(b.dir.path()).unwrap();
    assert_eq!(query(&c, json!({"operation":"degree"}), true), original);
    assert_eq!(original["hostLinks"][0]["attestation"], "unverified");
    assert_eq!(original["canonicalSourceAuthority"], false);
}
#[test]
fn actual_parent_order_and_all_link_endpoints_are_rechecked_without_source_or_accounting_writes() {
    for bad_endpoint in [false, true] {
        let (a, ga, ma) = checked();
        let (b, gb, mb) = checked();
        let mut r = review(&ga, &gb);
        if bad_endpoint {
            r.links[0].target.key = "unknown".into();
        } else {
            r.graph_ids.reverse();
        }
        let before = [state(&a.engine), state(&b.engine)];
        refused(
            composition::compose(vec![ga, gb], &r, 64_000_000, &Deadline::new(60000)).map(|_| ()),
            "mixed_meaning_composition_invalid",
        );
        assert_eq!(before, [state(&a.engine), state(&b.engine)]);
        for marker in [ma, mb] {
            assert_eq!(
                fs::read(marker)
                    .unwrap()
                    .iter()
                    .filter(|b| **b == b'\n')
                    .count(),
                1
            );
        }
    }
}
#[test]
fn review_shapes_and_input_output_traversal_deadline_budgets_refuse_whole_results() {
    let (a, ga, _) = checked();
    let (b, gb, _) = checked();
    let review = review(&ga, &gb);
    let before = [state(&a.engine), state(&b.engine)];
    refused(
        composition::compose(vec![ga, gb], &review, 1, &Deadline::new(60000)).map(|_| ()),
        "mixed_view_capacity_exceeded",
    );
    assert_eq!(before, [state(&a.engine), state(&b.engine)]);
    for mutate in 0..5 {
        let mut r = review.clone();
        match mutate {
            0 => r.graph_ids[1] = r.graph_ids[0].clone(),
            1 => r.links[0].relation = "../path".into(),
            2 => r.links[0].source.graph_id = hash("foreign"),
            3 => r.links[0].rationale = "x".repeat(1025),
            _ => r.links = r.links.repeat(65),
        }
        assert!(composition::validate_review(&r, &Deadline::new(60000)).is_err());
    }
    assert!(
        yoke::json::decode::<Review>(
            json!({"schemaVersion":1,"graphIds":[hash("a"),hash("b")],"links":[],"unknown":true})
        )
        .is_err()
    );
    let (_a, _b, c, _) = composed();
    let e = c.export(64_000_000, &Deadline::new(60000)).unwrap();
    let root = &e["nodes"][0]["reference"];
    let q = yoke::json::decode(trace(root, "outgoing", 1, 2)).unwrap();
    refused(
        c.query(&q, true, 64_000_000, &Deadline::new(60000)),
        "mixed_view_capacity_exceeded",
    );
    refused(
        c.export(128, &Deadline::new(60000)),
        "mixed_view_capacity_exceeded",
    );
    refused(
        c.export(64_000_000, &Deadline::new(0)),
        "mixed_view_deadline_exceeded",
    );
}
#[test]
fn malformed_composition_cli_review_refuses_privately_before_missing_store_initialization() {
    let dir = tempfile::tempdir().unwrap();
    let review = dir.path().join("review.json");
    fs::write(
        &review,
        br#"{"schemaVersion":1,"NEVER_ECHO_REVIEW_KEY_SENTINEL":true}"#,
    )
    .unwrap();
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_yoke"))
        .arg("--root")
        .arg(dir.path())
        .args([
            "mixed-meaning",
            "compose",
            "--job-id",
            &hash("jobA"),
            "--job-id",
            &hash("jobB"),
            "--review",
        ])
        .arg(review)
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    let error = String::from_utf8_lossy(&output.stderr);
    assert!(
        error.contains("mixed_meaning_composition_invalid"),
        "{error}"
    );
    assert!(!error.contains("NEVER_ECHO_REVIEW_KEY_SENTINEL"));
    assert!(!dir.path().join(".yoke").exists());
}
#[test]
fn cli_composes_pdf_and_office_jobs_under_one_store_without_provider_configuration() {
    let (f, ga, marker) = checked();
    let d = Deadline::new(60000);
    let mut request = inputs(&f, &d);
    request
        .units
        .retain(|u| !matches!(u.source, mixed_view::SourceRef::PdfPageText { .. }));
    request.context_groups.clear();
    let office = SourceSelection::Office {
        selection: mixed_view::Selection {
            schema_version: 1,
            source: f.selection.source.clone(),
            documents: f.selection.documents.clone(),
        },
    };
    let (spec, script, _) = configured(&f, "success");
    let result = provider::build(
        &f.engine.store,
        &office,
        &request,
        None,
        &spec,
        &policy(),
        &d,
    )
    .unwrap();
    let gb = provider::load_checked(&f.engine.store, result["jobId"].as_str().unwrap(), None, &d)
        .unwrap();
    fs::remove_file(script).unwrap();
    let originals = [
        ga.export(16_000_000, &d).unwrap(),
        gb.export(16_000_000, &d).unwrap(),
    ];
    let review = review(&ga, &gb);
    let c = composition::compose(vec![ga, gb], &review, 64_000_000, &d).unwrap();
    let query_file = f.dir.path().join("query.json");
    let review_file = f.dir.path().join("review.json");
    fs::write(&query_file, br#"{"operation":"degree"}"#).unwrap();
    fs::write(&review_file, serde_json::to_vec(&review).unwrap()).unwrap();
    let calls = fs::read(&marker).unwrap();
    let root = f.engine.root.clone();
    drop(f.engine);
    let run = |pdf: bool| {
        let mut cmd = std::process::Command::new(env!("CARGO_BIN_EXE_yoke"));
        cmd.arg("--root")
            .arg(&root)
            .args([
                "--timeout",
                "60000",
                "--max-bytes",
                "64000000",
                "mixed-meaning",
                "compose",
                "--job-id",
                originals[0]["jobId"].as_str().unwrap(),
                "--job-id",
                originals[1]["jobId"].as_str().unwrap(),
                "--review",
            ])
            .arg(&review_file)
            .arg("--query")
            .arg(&query_file)
            .arg("--include-host-links");
        if pdf {
            cmd.arg("--native-pdf-text");
        }
        cmd.output().unwrap()
    };
    let denied = run(false);
    assert!(!denied.status.success());
    assert!(denied.stdout.is_empty());
    let output = run(true);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let value: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value, query(&c, json!({"operation":"degree"}), true));
    assert_eq!(calls, fs::read(marker).unwrap());
    assert_eq!(calls.iter().filter(|b| **b == b'\n').count(), 2);
}
