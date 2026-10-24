//! Owned source and model processes test policy/replay, never semantic accuracy.
#[allow(dead_code)]
#[path = "support/meaning_semantics_fixture.rs"]
mod support;
use serde_json::{Value, json};
use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
};
use support::{MeaningPair, fixture};
use yoke::{
    Engine,
    capture::Deadline,
    ledger::Ledger,
    meaning_identity as api,
    model::{hash, identity},
    retention,
};
thread_local! {
    // Other concurrently running tests must not contaminate this observation.
    static LARGEST:Cell<Option<usize>>=const {Cell::new(None)};
}
fn observe(size: usize) {
    let _ = LARGEST.try_with(|v| {
        if let Some(old) = v.get() {
            v.set(Some(old.max(size)));
        }
    });
}
struct Observed;
unsafe impl GlobalAlloc for Observed {
    unsafe fn alloc(&self, l: Layout) -> *mut u8 {
        observe(l.size());
        unsafe { System.alloc(l) }
    }
    unsafe fn dealloc(&self, p: *mut u8, l: Layout) {
        unsafe { System.dealloc(p, l) }
    }
    unsafe fn realloc(&self, p: *mut u8, l: Layout, n: usize) -> *mut u8 {
        observe(n);
        unsafe { System.realloc(p, l, n) }
    }
}
#[global_allocator]
static ALLOCATOR: Observed = Observed;
fn d() -> Deadline {
    Deadline::new(60000)
}
fn request(f: &MeaningPair) -> api::Request {
    let mut graph_ids = vec![
        f.before.meaning.as_ref().unwrap().selections[0]
            .graph_id
            .clone(),
        f.after.meaning.as_ref().unwrap().selections[0]
            .graph_id
            .clone(),
    ];
    graph_ids.sort();
    api::Request {
        schema_version: 1,
        selection: api::Selection {
            workspace_id: f.snapshot.workspace_id.clone(),
            snapshot_id: f.snapshot.id.clone(),
            base_graph_hash: identity(&f.graph),
            graph_ids,
        },
        candidate_policy: Default::default(),
        additional_pairs: vec![],
    }
}
fn configured(
    f: &MeaningPair,
    mode: &str,
    extra: &str,
) -> (yoke::providers::ProviderSpec, api::Policy, PathBuf, PathBuf) {
    let script = f.dir.path().join("identity-worker.cjs");
    let marker = f.dir.path().join("identity-calls.log");
    fs::write(&script,r#"
let raw='';process.stdin.setEncoding('utf8');process.stdin.on('data',b=>raw+=b);process.stdin.on('end',()=>{
 const r=JSON.parse(raw),fs=require('fs'),mode=process.argv[3],extra=process.argv[4];fs.appendFileSync(process.argv[2],r.requestId+'\n');
 if(mode==='delay'){setTimeout(()=>{},10000);return;}
 const source=r.payload.source,candidates=r.payload.candidates;
 const key=ref=>JSON.stringify(ref);const concepts=new Map(source.nodes.map(n=>[key(n.members[0].reference),n.members[0].record]));
 let decisions=candidates.map(c=>({candidateId:c.id,outcome:concepts.get(key(c.left)).label===concepts.get(key(c.right)).label?'same_entity':'different_entities',
   rationale:extra==='none'?'Unverified semantic identity interpretation.':extra,
   support:[c.left,c.right].map(reference=>({reference,evidenceId:concepts.get(key(reference)).evidenceIds[0]}))}));
 if(mode==='all-different')decisions.forEach(d=>d.outcome='different_entities');
 if(mode==='uncertain')decisions.forEach(d=>d.outcome='uncertain');
 if(mode==='all-same')decisions.forEach(d=>d.outcome='same_entity');
 if(mode==='reverse')decisions.reverse();
 if(mode==='missing')decisions.pop();
 if(mode==='duplicate')decisions[1].candidateId=decisions[0].candidateId;
 if(mode==='foreign-support')decisions[0].support[0].evidenceId='0'.repeat(64);
 if(mode==='one-sided')decisions[0].support.pop();
 if(mode==='non-clique'){
   const refs=[...concepts.keys()].sort().slice(0,3);decisions.forEach((d,i)=>{const c=candidates[i],a=key(c.left),b=key(c.right);
     d.outcome=((a===refs[0]&&b===refs[1])||(b===refs[0]&&a===refs[1])||(a===refs[1]&&b===refs[2])||(b===refs[1]&&a===refs[2]))?'same_entity':'different_entities';});
 }
 const reply={schema:1,operation:r.operation,model:r.model,modelVersion:r.modelVersion,requestId:r.requestId,inputHash:r.inputHash,currency:r.currency,
   sourceBinding:r.payload.sourceBinding,actualCostMicros:4,usage:{inputTokens:20,outputTokens:10,cachedInputTokens:0},data:{decisions}};
 if(mode==='unknown-cost'){reply.actualCostMicros=null;reply.usage=null;}
 if(mode==='unbound')reply.sourceBinding={...reply.sourceBinding,sourceHash:'0'.repeat(64)};
 if(mode==='bad-usage')reply.usage.cachedInputTokens=99;
 if(mode==='overrun')reply.actualCostMicros=11;
 if(mode==='error')reply.error='NEVER_ECHO_IDENTITY_PROVIDER_SENTINEL';
 if(mode==='invalid-utf8')process.stdout.write(Buffer.from([255]));
 else if(mode==='truncated')process.stdout.write(JSON.stringify(reply).slice(0,80));
 else if(mode==='duplicate-envelope')process.stdout.write(JSON.stringify(reply).replace('"schema":1','"schema":1,"schema":1'));
 else process.stdout.write(JSON.stringify(reply));
 if(mode==='nonzero')process.exitCode=9;
});
"#).unwrap();
    let p = yoke::providers::ProviderSpec {
        trusted: true,
        command: "node".into(),
        args: vec![
            script.to_str().unwrap().into(),
            marker.to_str().unwrap().into(),
            mode.into(),
            extra.into(),
        ],
        model_id: "owned-semantic-identity".into(),
        model_version: "v1".into(),
        task_id: "owned-identity-task".into(),
        currency: "EUR".into(),
        max_cost_micros: 10,
        task_limit_micros: 100,
    };
    let mut policy = api::Policy::new(hash("owned semantic identity contract"));
    policy.provider_input_files.push(script.clone());
    (p, policy, script, marker)
}
fn candidates(f: &MeaningPair, r: &api::Request) -> Value {
    api::candidates(&f.engine.store, r, &Default::default(), &d())
        .unwrap()
        .export(16_000_000, &d())
        .unwrap()
}
fn build(
    f: &MeaningPair,
    p: &yoke::providers::ProviderSpec,
    policy: &api::Policy,
) -> anyhow::Result<Value> {
    api::build(
        &f.engine.store,
        &request(f),
        p,
        policy,
        &Default::default(),
        &d(),
    )
}
fn report(f: &MeaningPair, id: &str) -> Value {
    api::load(&f.engine.store, id, &Default::default(), &d())
        .unwrap()
        .export(16_000_000, &d())
        .unwrap()
}
fn status(f: &MeaningPair, id: &str) -> Value {
    api::status(&f.engine.store, id, &Default::default(), &d()).unwrap()
}
fn rows(f: &MeaningPair) -> Vec<(String, String, String)> {
    let mut s = f
        .engine
        .store
        .db
        .prepare("SELECT kind,id,value FROM records ORDER BY kind,id")
        .unwrap();
    s.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
        .unwrap()
        .collect::<rusqlite::Result<_>>()
        .unwrap()
}
fn job(f: &MeaningPair) -> Value {
    let raw: String = f
        .engine
        .store
        .db
        .query_row(
            "SELECT value FROM records WHERE kind=?1 LIMIT 1",
            [api::JOB_KIND],
            |r| r.get(0),
        )
        .unwrap();
    serde_json::from_str(&raw).unwrap()
}
fn cli(root: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_yoke"))
        .arg("--root")
        .arg(root)
        .args(["--timeout", "60000", "--max-bytes", "16000000"])
        .args(args)
        .output()
        .unwrap()
}

#[test]
fn checked_candidates_retain_every_original_claim_quote_receipt_and_equal_alias_pair() {
    let f = fixture();
    let before = rows(&f);
    let v = candidates(&f, &request(&f));
    assert_eq!(rows(&f), before);
    assert_eq!(v["candidates"].as_array().unwrap().len(), 45);
    assert_eq!(v["source"]["nodes"].as_array().unwrap().len(), 10);
    assert_eq!(v["source"]["relations"].as_array().unwrap().len(), 9);
    assert_eq!(v["source"]["hyperedges"].as_array().unwrap().len(), 2);
    assert!(
        v["source"]["evidence"]
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e["record"]["text"].as_str().unwrap().contains("café\r\n"))
    );
    assert!(
        v["candidates"]
            .as_array()
            .unwrap()
            .iter()
            .any(|c| c["lexical"]["normalizedEqual"] == true
                && c["lexical"]["matchingLshBands"] == 16)
    );
    assert_eq!(v["providerCallsThisRead"], 0);
    assert_eq!(v["mergesApplied"], 0);
    assert_eq!(v["semanticIdentityEstablished"], false);
    assert_eq!(v["complete"], false);
    assert_eq!(fs::read_to_string(f.marker).unwrap().lines().count(), 6);
}

#[test]
fn no_candidates_are_inspectable_and_refuse_model_reservation_before_executable_lookup() {
    let f = fixture();
    let script = f.dir.path().join("single-source-worker.cjs");
    fs::write(&script,r#"let raw='';process.stdin.on('data',b=>raw+=b);process.stdin.on('end',()=>{const r=JSON.parse(raw),support=r.payload.sources.map(s=>({sourceId:s.anchor.sourceId,startByte:s.anchor.startByte,endByte:s.anchor.endByte}));process.stdout.write(JSON.stringify({schema:1,operation:r.operation,model:r.model,modelVersion:r.modelVersion,requestId:r.requestId,inputHash:r.inputHash,currency:r.currency,actualCostMicros:1,sourceBindings:r.payload.sources.map(s=>s.anchor),data:{concepts:[{key:'only',label:'One original concept',description:'Owned finite singleton',aliases:[],support}],relations:[],hyperedges:[],coverage:r.payload.sources.map(s=>({sourceId:s.anchor.sourceId,outcome:'claims'}))}}));});"#).unwrap();
    let p = yoke::providers::ProviderSpec {
        trusted: true,
        command: "node".into(),
        args: vec![script.to_str().unwrap().into()],
        model_id: "owned-single-source".into(),
        model_version: "v1".into(),
        task_id: "owned-single-source-task".into(),
        currency: "EUR".into(),
        max_cost_micros: 10,
        task_limit_micros: 100,
    };
    let mut policy =
        yoke::meaning_build::MeaningPolicy::files(&f.snapshot, hash("owned singleton contract"));
    policy.selection = yoke::meaning_build::MeaningSelection::Files(vec!["a.rs".into()]);
    let built =
        yoke::meaning_build::build(&f.engine.store, &f.snapshot, &f.graph, &p, &policy, &d())
            .unwrap();
    let mut r = request(&f);
    r.selection.graph_ids = vec![built["graphId"].as_str().unwrap().into()];
    let v = candidates(&f, &r);
    assert_eq!(v["candidates"], json!([]));
    assert_eq!(v["complete"], false);
    let (mut p, policy, _, marker) = configured(&f, "success", "none");
    p.command = f.dir.path().join("missing-provider.exe");
    let before = rows(&f);
    let e = api::build(&f.engine.store, &r, &p, &policy, &Default::default(), &d()).unwrap_err();
    assert_eq!(e.to_string(), "meaning_identity_no_candidates");
    assert_eq!(rows(&f), before);
    assert!(!marker.exists());
}

#[test]
fn explicit_model_cliques_project_nodes_without_dropping_parallel_edges_or_ordered_roles() {
    let f = fixture();
    let (p, policy, _, marker) = configured(&f, "success", "none");
    let r = build(&f, &p, &policy).unwrap();
    let v = report(&f, r["jobId"].as_str().unwrap());
    assert_eq!(v["originalConceptCount"], 10);
    assert_eq!(v["projectedNodeCount"], 3);
    assert_eq!(
        v["nodes"]
            .as_array()
            .unwrap()
            .iter()
            .map(|n| n["members"].as_array().unwrap().len())
            .sum::<usize>(),
        10
    );
    let mut sizes = v["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|n| n["members"].as_array().unwrap().len())
        .collect::<Vec<_>>();
    sizes.sort();
    assert_eq!(sizes, vec![2, 2, 6]);
    assert_eq!(v["relations"].as_array().unwrap().len(), 9);
    assert_eq!(v["hyperedges"].as_array().unwrap().len(), 2);
    for (a, b) in v["relations"]
        .as_array()
        .unwrap()
        .iter()
        .zip(v["originalSelection"]["relations"].as_array().unwrap())
    {
        assert_eq!(a["reference"], b["reference"]);
        assert_eq!(a["record"], b["record"]);
        assert_eq!(a["originalProjectedSource"], b["source"]);
    }
    for (a, b) in v["hyperedges"]
        .as_array()
        .unwrap()
        .iter()
        .zip(v["originalSelection"]["hyperedges"].as_array().unwrap())
    {
        assert_eq!(a["record"], b["record"]);
        assert_eq!(a["participants"].as_array().unwrap().len(), 4);
        for (x, y) in a["participants"]
            .as_array()
            .unwrap()
            .iter()
            .zip(b["participants"].as_array().unwrap())
        {
            assert_eq!(x["role"], y["role"]);
            assert_eq!(x["position"], y["position"]);
            assert_eq!(x["original"], y["original"]);
            assert_eq!(x["originalNodeId"], y["nodeId"]);
        }
    }
    assert_eq!(v["providerReportedCostMicros"], 4);
    assert_eq!(v["modelCallsOriginalBuild"], 1);
    assert_eq!(v["originalClaimsModified"], false);
    assert_eq!(v["semanticIdentityEstablished"], false);
    assert_eq!(v["canonicalSourceAuthority"], false);
    assert_eq!(v["executionObserved"], false);
    assert_eq!(fs::read_to_string(marker).unwrap().lines().count(), 1);
    retention::plan(&f.engine.store, &Default::default(), &d()).unwrap();
}

#[test]
fn frozen_node_degree_and_directed_trace_queries_preserve_original_members_and_observations() {
    let f = fixture();
    let (p, policy, script, marker) = configured(&f, "success", "none");
    let r = build(&f, &p, &policy).unwrap();
    let id = r["jobId"].as_str().unwrap();
    let graph = api::load(&f.engine.store, id, &Default::default(), &d()).unwrap();
    let v = graph.export(16_000_000, &d()).unwrap();
    let alpha = v["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|n| n["members"][0]["record"]["label"] == "Alpha concept")
        .unwrap();
    let reference: yoke::meaning_join::ConceptRef =
        serde_json::from_value(alpha["members"][0]["reference"].clone()).unwrap();
    let node = graph
        .query(
            &api::Query::Node {
                reference: reference.clone(),
            },
            16_000_000,
            &d(),
        )
        .unwrap();
    assert_eq!(node["kind"], "meaning_identity_query_v1");
    assert_eq!(node["nodes"].as_array().unwrap().len(), 1);
    assert_eq!(node["nodes"][0]["members"].as_array().unwrap().len(), 6);
    assert_eq!(node["relations"].as_array().unwrap().len(), 3);
    assert_eq!(node["originalFragment"], v["originalFragment"]);
    let degree = graph
        .query(&api::Query::Degree {}, 16_000_000, &d())
        .unwrap();
    let degree_rows = degree["rows"].as_array().unwrap();
    assert_eq!(degree_rows.len(), 3);
    assert_eq!(
        degree_rows
            .iter()
            .map(|r| r["inDegree"].as_u64().unwrap())
            .sum::<u64>(),
        9
    );
    assert_eq!(
        degree_rows
            .iter()
            .map(|r| r["outDegree"].as_u64().unwrap())
            .sum::<u64>(),
        9
    );
    assert_eq!(
        degree_rows
            .iter()
            .map(|r| r["hyperedgeSlots"].as_u64().unwrap())
            .sum::<u64>(),
        8
    );
    let a = degree_rows
        .iter()
        .find(|r| r["nodeId"] == alpha["id"])
        .unwrap();
    assert_eq!(a["inDegree"], 6);
    assert_eq!(a["outDegree"], 6);
    assert_eq!(a["hyperedgeSlots"], 4);
    let q = api::Query::Trace {
        roots: vec![reference.clone()],
        direction: api::Direction::Outgoing,
        depth: 1,
        max_visited: 2,
    };
    let trace = graph.query(&q, 16_000_000, &d()).unwrap();
    assert_eq!(trace["nodes"].as_array().unwrap().len(), 2);
    assert_eq!(trace["relations"].as_array().unwrap().len(), 9);
    assert_eq!(trace["hyperedges"], json!([]));
    assert_eq!(trace["providerCallsThisRead"], 0);
    assert_eq!(trace["semanticIdentityEstablished"], false);
    let q = api::Query::Trace {
        roots: vec![reference.clone()],
        direction: api::Direction::Both,
        depth: 1,
        max_visited: 1,
    };
    assert!(graph.query(&q, 16_000_000, &d()).is_err());
    let mut wrong = reference;
    wrong.graph_id = "0".repeat(64);
    assert!(
        graph
            .query(&api::Query::Node { reference: wrong }, 16_000_000, &d())
            .is_err()
    );
    fs::remove_file(script).unwrap();
    let before = rows(&f);
    assert_eq!(
        graph
            .query(&api::Query::Degree {}, 16_000_000, &d())
            .unwrap(),
        degree
    );
    assert_eq!(rows(&f), before);
    assert_eq!(fs::read_to_string(marker).unwrap().lines().count(), 1);
}

#[test]
fn lexical_equalities_never_override_negative_or_uncertain_model_decisions() {
    for mode in ["all-different", "uncertain"] {
        let f = fixture();
        let (p, policy, _, _) = configured(&f, mode, "none");
        let r = build(&f, &p, &policy).unwrap();
        let v = report(&f, r["jobId"].as_str().unwrap());
        assert_eq!(v["projectedNodeCount"], 10);
        assert_eq!(
            v["originalFragment"]["decisions"].as_array().unwrap().len(),
            45
        );
    }
    let f = fixture();
    let (p, policy, _, _) = configured(&f, "all-same", "none");
    let r = build(&f, &p, &policy).unwrap();
    let v = report(&f, r["jobId"].as_str().unwrap());
    assert_eq!(v["projectedNodeCount"], 1);
    for edge in v["hyperedges"].as_array().unwrap() {
        let slots = edge["participants"].as_array().unwrap();
        assert_eq!(slots.len(), 4);
        assert!(slots.iter().all(|s| s["nodeId"] == slots[0]["nodeId"]));
        assert_eq!(slots[0]["role"], "input");
        assert_eq!(slots[1]["role"], "input");
        assert_eq!(slots[2]["role"], "context");
        assert_eq!(slots[3]["role"], "output");
    }
}

#[test]
fn parent_accounting_reconciliation_does_not_rebind_original_source_or_identity_result() {
    let f = fixture();
    let (p, policy, _, _) = configured(&f, "success", "none");
    let r = build(&f, &p, &policy).unwrap();
    let id = r["jobId"].as_str().unwrap();
    let expected = report(&f, id);
    let original_attempt:String=f.engine.store.db.query_row("SELECT id FROM records WHERE kind='native_model_attempt_v1' AND id!=?1 ORDER BY id LIMIT 1",[r["attemptId"].as_str().unwrap()],|r|r.get(0)).unwrap();
    let deadline = d();
    let ledger = Ledger::new(&f.engine.store, &deadline);
    ledger.settle(&original_attempt, Some(7)).unwrap();
    ledger
        .usage(
            &original_attempt,
            "later-host-observation",
            yoke::ledger::Usage {
                input_tokens: 123,
                output_tokens: 9,
                cached_input_tokens: 0,
            },
        )
        .unwrap();
    assert_eq!(report(&f, id), expected);
    assert_eq!(status(&f, id)["state"], "complete");
    retention::plan(&f.engine.store, &Default::default(), &d()).unwrap();
}

#[test]
fn oversized_linked_sql_cells_refuse_before_rust_materialization_and_restore_limits() {
    let f = fixture();
    let (p, policy, _, _) = configured(&f, "success", "none");
    build(&f, &p, &policy).unwrap();
    let j = job(&f);
    let id = j["payloadId"].as_str().unwrap();
    f.engine.store.db.execute("UPDATE records SET value=CAST(zeroblob(16000000) AS TEXT) WHERE kind='artifact' AND id=?1",[id]).unwrap();
    let original = f
        .engine
        .store
        .db
        .limit(rusqlite::limits::Limit::SQLITE_LIMIT_LENGTH)
        .unwrap();
    LARGEST.set(Some(0));
    let result = retention::plan(&f.engine.store, &Default::default(), &d());
    let largest = LARGEST.replace(None).unwrap();
    assert!(result.is_err());
    assert!(largest < 1_000_000);
    assert_eq!(
        f.engine
            .store
            .db
            .limit(rusqlite::limits::Limit::SQLITE_LIMIT_LENGTH)
            .unwrap(),
        original
    );
}

#[test]
fn non_clique_missing_duplicate_foreign_or_one_sided_support_refuses_with_original_bill() {
    for mode in [
        "non-clique",
        "missing",
        "duplicate",
        "foreign-support",
        "one-sided",
    ] {
        let f = fixture();
        let (p, policy, _, marker) = configured(&f, mode, "none");
        let e = build(&f, &p, &policy).unwrap_err();
        let j = job(&f);
        let id = j["id"].as_str().unwrap();
        assert!(e.to_string().contains(id));
        let s = status(&f, id);
        assert_eq!(s["state"], "failed", "{mode}");
        assert_eq!(
            s["originalResponseReceipt"]["providerReportedCostMicros"],
            4
        );
        assert!(s["resultArtifactId"].is_null());
        assert!(api::load(&f.engine.store, id, &Default::default(), &d()).is_err());
        assert_eq!(
            Ledger::new(&f.engine.store, &d())
                .total(&p.task_id)
                .unwrap()
                .actual_micros,
            4
        );
        retention::plan(&f.engine.store, &Default::default(), &d()).unwrap();
        assert_eq!(fs::read_to_string(marker).unwrap().lines().count(), 1);
    }
}

#[test]
fn errors_invalid_usage_nonzero_and_overrun_keep_bound_receipts_without_echo_or_retry() {
    for mode in ["error", "bad-usage", "nonzero", "overrun"] {
        let f = fixture();
        let (p, policy, _, marker) = configured(&f, mode, "none");
        let e = build(&f, &p, &policy).unwrap_err();
        assert!(!format!("{e:#}").contains("NEVER_ECHO_IDENTITY_PROVIDER_SENTINEL"));
        let j = job(&f);
        let s = status(&f, j["id"].as_str().unwrap());
        assert_eq!(s["state"], "failed");
        assert_eq!(
            s["originalResponseReceipt"]["providerReportedCostMicros"],
            if mode == "overrun" { 11 } else { 4 }
        );
        assert_eq!(s["originalExitCode"], if mode == "nonzero" { 9 } else { 0 });
        if mode == "overrun" {
            let before = rows(&f);
            assert!(build(&f, &p, &policy).is_err());
            assert_eq!(rows(&f), before);
        }
        assert_eq!(fs::read_to_string(marker).unwrap().lines().count(), 1);
    }
}

#[test]
fn unbound_duplicate_truncated_and_invalid_utf8_responses_retain_unknown_reservations() {
    for mode in ["unbound", "duplicate-envelope", "truncated", "invalid-utf8"] {
        let f = fixture();
        let (p, policy, script, marker) = configured(&f, mode, "none");
        assert!(build(&f, &p, &policy).is_err());
        let j = job(&f);
        fs::remove_file(script).unwrap();
        let before = rows(&f);
        let s = status(&f, j["id"].as_str().unwrap());
        assert_eq!(s["providerCallsThisRead"], 0);
        assert!(s["originalResponseReceipt"].is_null());
        assert_eq!(rows(&f), before);
        let t = Ledger::new(&f.engine.store, &d())
            .total(&p.task_id)
            .unwrap();
        assert_eq!(t.unknown, 1);
        assert_eq!(t.reserved_micros, 10);
        assert_eq!(t.actual_micros, 0);
        assert_eq!(fs::read_to_string(marker).unwrap().lines().count(), 1);
    }
}

#[test]
fn trust_policy_source_and_all_host_limits_refuse_before_any_new_attempt() {
    let f = fixture();
    let (mut p, mut policy, _, marker) = configured(&f, "success", "none");
    let before = rows(&f);
    p.trusted = false;
    assert!(build(&f, &p, &policy).is_err());
    p.trusted = true;
    policy.max_result_bytes = 15_999_999;
    assert!(build(&f, &p, &policy).is_err());
    policy.max_result_bytes = 16_000_000;
    for o in [
        api::Options {
            max_concepts: 9,
            ..Default::default()
        },
        api::Options {
            max_pairs: 44,
            ..Default::default()
        },
        api::Options {
            max_candidates: 44,
            ..Default::default()
        },
        api::Options {
            max_similarity_work: 0,
            ..Default::default()
        },
        api::Options {
            max_input_bytes: 1_000_000,
            ..Default::default()
        },
        api::Options {
            max_output_bytes: 200,
            ..Default::default()
        },
    ] {
        assert!(api::build(&f.engine.store, &request(&f), &p, &policy, &o, &d()).is_err());
    }
    let mut r = request(&f);
    r.selection.base_graph_hash = "0".repeat(64);
    assert!(api::build(&f.engine.store, &r, &p, &policy, &Default::default(), &d()).is_err());
    assert_eq!(rows(&f), before);
    assert!(!marker.exists());
}

#[test]
fn original_unknown_billing_survives_reconciliation_and_model_decision_order_is_preserved() {
    let f = fixture();
    let (p, policy, _, _) = configured(&f, "unknown-cost", "none");
    let r = build(&f, &p, &policy).unwrap();
    let id = r["jobId"].as_str().unwrap();
    let v = report(&f, id);
    assert!(v["providerReportedCostMicros"].is_null());
    Ledger::new(&f.engine.store, &d())
        .settle(r["attemptId"].as_str().unwrap(), Some(6))
        .unwrap();
    assert_eq!(report(&f, id), v);
    assert_eq!(
        status(&f, id)["currentAttemptObservation"]["actualCostMicros"],
        6
    );
    assert!(status(&f, id)["originalResponseReceipt"]["providerReportedCostMicros"].is_null());
    let f = fixture();
    let (p, policy, _, _) = configured(&f, "reverse", "none");
    let r = build(&f, &p, &policy).unwrap();
    let v = report(&f, r["jobId"].as_str().unwrap());
    assert_eq!(
        v["originalFragment"]["decisions"][0]["candidateId"],
        v["candidates"].as_array().unwrap().last().unwrap()["id"]
    );
    assert_eq!(v["projectedNodeCount"], 3);
}

#[test]
fn provider_free_checked_export_and_candidate_export_survive_all_store_and_worker_loss() {
    let f = fixture();
    let (p, policy, script, marker) = configured(&f, "success", "none");
    let r = build(&f, &p, &policy).unwrap();
    let id = r["jobId"].as_str().unwrap();
    let checked = api::load(&f.engine.store, id, &Default::default(), &d()).unwrap();
    let candidate =
        api::candidates(&f.engine.store, &request(&f), &Default::default(), &d()).unwrap();
    let expected = checked.export(16_000_000, &d()).unwrap();
    fs::remove_file(script).unwrap();
    let before = rows(&f);
    assert_eq!(report(&f, id), expected);
    assert_eq!(status(&f, id)["providerCallsThisRead"], 0);
    let jobs = api::jobs(&f.engine.store, &p.task_id, 0, 1, &Default::default(), &d()).unwrap();
    assert_eq!(jobs["totalJobs"], 1);
    assert_eq!(rows(&f), before);
    let marker_bytes = fs::read(&marker).unwrap();
    assert_eq!(String::from_utf8_lossy(&marker_bytes).lines().count(), 1);
    let dir = support::remove_store_and_workspace(f);
    assert_eq!(checked.export(16_000_000, &d()).unwrap(), expected);
    assert_eq!(
        checked
            .query(&api::Query::Degree {}, 16_000_000, &d())
            .unwrap()["providerCallsThisRead"],
        0
    );
    assert_eq!(
        candidate.export(16_000_000, &d()).unwrap()["candidates"]
            .as_array()
            .unwrap()
            .len(),
        45
    );
    assert!(checked.export(1000, &d()).is_err());
    assert!(dir.path().exists());
}

#[test]
fn source_payload_response_rehashed_result_and_ledger_tampering_refuse_read_and_retention() {
    for target in ["source", "payloadId", "responseId", "resultId", "ledger"] {
        let f = fixture();
        let (p, policy, _, _) = configured(&f, "success", "none");
        let r = build(&f, &p, &policy).unwrap();
        let id = r["jobId"].as_str().unwrap();
        let mut j = job(&f);
        if target == "ledger" {
            let mut a: Value = f
                .engine
                .store
                .get_record("native_model_attempt_v1", r["attemptId"].as_str().unwrap())
                .unwrap();
            a["provider"] = json!("forged@v1");
            f.engine
                .store
                .db
                .execute(
                    "UPDATE records SET value=?1 WHERE kind='native_model_attempt_v1' AND id=?2",
                    rusqlite::params![a.to_string(), r["attemptId"].as_str().unwrap()],
                )
                .unwrap();
        } else if target == "resultId" {
            let mut v = report(&f, id);
            v["nodes"][0]["representative"]["conceptId"] = json!("0".repeat(64));
            j["resultId"] = json!(
                f.engine
                    .store
                    .artifact(&serde_json::to_vec(&v).unwrap())
                    .unwrap()
            );
            f.engine
                .store
                .db
                .execute(
                    "UPDATE records SET value=?1 WHERE kind=?2 AND id=?3",
                    rusqlite::params![j.to_string(), api::JOB_KIND, id],
                )
                .unwrap();
        } else {
            let blob = if target == "source" {
                hash(support::A)
            } else {
                j[target].as_str().unwrap().into()
            };
            let path = f.engine.store.dir.join("blobs").join(blob);
            let mut bytes = fs::read(&path).unwrap();
            bytes[0] ^= 1;
            fs::write(path, bytes).unwrap();
        }
        let before = rows(&f);
        assert!(
            api::load(&f.engine.store, id, &Default::default(), &d()).is_err(),
            "{target}"
        );
        assert!(api::status(&f.engine.store, id, &Default::default(), &d()).is_err());
        assert!(retention::plan(&f.engine.store, &Default::default(), &d()).is_err());
        assert_eq!(rows(&f), before);
    }
}

#[test]
fn sql_registration_send_and_publication_failures_preserve_the_right_accounting_phase() {
    for phase in ["register", "send", "publish"] {
        let f = fixture();
        let (p, policy, _, marker) = configured(&f, "success", "none");
        let sql = match phase {
            "register" => {
                "CREATE TRIGGER owned_failure BEFORE INSERT ON records WHEN NEW.kind='native_meaning_identity_job_v1' BEGIN SELECT RAISE(ABORT,'owned registration failure'); END;"
            }
            "send" => {
                "CREATE TRIGGER owned_failure BEFORE UPDATE ON records WHEN NEW.kind='native_model_attempt_v1' AND json_extract(NEW.value,'$.state')='sent' BEGIN SELECT RAISE(ABORT,'owned send failure'); END;"
            }
            _ => {
                "CREATE TRIGGER owned_failure BEFORE UPDATE ON records WHEN NEW.kind='native_meaning_identity_job_v1' AND json_extract(NEW.value,'$.state')='complete' BEGIN SELECT RAISE(ABORT,'owned publication failure'); END;"
            }
        };
        f.engine.store.db.execute_batch(sql).unwrap();
        assert!(build(&f, &p, &policy).is_err());
        let t = Ledger::new(&f.engine.store, &d())
            .total(&p.task_id)
            .unwrap();
        assert_eq!(t.reserved_micros, 0);
        if phase == "register" {
            assert_eq!(t.attempts, 0);
            assert!(!marker.exists());
        } else {
            let j = job(&f);
            let s = status(&f, j["id"].as_str().unwrap());
            if phase == "send" {
                assert_eq!(s["currentAttemptObservation"]["state"], "not_sent");
                assert!(!marker.exists());
            } else {
                assert_eq!(
                    s["originalResponseReceipt"]["providerReportedCostMicros"],
                    4
                );
                assert!(s["resultArtifactId"].is_null());
                assert_eq!(fs::read_to_string(marker).unwrap().lines().count(), 1);
            }
        }
        f.engine
            .store
            .db
            .execute_batch("DROP TRIGGER owned_failure")
            .unwrap();
        retention::plan(&f.engine.store, &Default::default(), &d()).unwrap();
    }
}

#[test]
fn interrupted_attempt_and_current_read_caps_never_turn_into_retry_or_clipped_results() {
    let f = fixture();
    let (p, policy, script, marker) = configured(&f, "delay", "none");
    assert!(
        api::build(
            &f.engine.store,
            &request(&f),
            &p,
            &policy,
            &Default::default(),
            &Deadline::new(2000)
        )
        .is_err()
    );
    let j = job(&f);
    fs::remove_file(script).unwrap();
    let s = status(&f, j["id"].as_str().unwrap());
    assert!(matches!(
        s["currentAttemptObservation"]["state"].as_str(),
        Some("sent" | "unknown")
    ));
    assert_eq!(s["automaticRetryAvailable"], false);
    assert_eq!(fs::read_to_string(marker).unwrap().lines().count(), 1);
    let f = fixture();
    let (p, policy, _, _) = configured(&f, "success", "none");
    let r = build(&f, &p, &policy).unwrap();
    let id = r["jobId"].as_str().unwrap();
    let before = rows(&f);
    for o in [
        api::Options {
            max_candidates: 44,
            ..Default::default()
        },
        api::Options {
            max_input_bytes: 1000,
            ..Default::default()
        },
        api::Options {
            max_output_bytes: 1000,
            ..Default::default()
        },
    ] {
        assert!(api::load(&f.engine.store, id, &o, &d()).is_err());
    }
    assert_eq!(rows(&f), before);
}

#[test]
fn model_rationale_hashes_do_not_grant_roots_to_unrelated_history() {
    let mut f = fixture();
    fs::write(f.engine.root.join("unrelated.rs"), "fn unrelated() {}\n").unwrap();
    let old = f.engine.index(&d()).unwrap().0.id;
    fs::remove_file(f.engine.root.join("unrelated.rs")).unwrap();
    let restored = f.engine.index(&d()).unwrap().0;
    assert_eq!(restored.id, f.snapshot.id);
    let before = retention::plan(&f.engine.store, &Default::default(), &d()).unwrap();
    assert!(
        before["snapshotIds"]
            .as_array()
            .unwrap()
            .iter()
            .any(|id| id == &old),
        "{before}"
    );
    let (p, policy, _, _) = configured(&f, "success", &old);
    let r = build(&f, &p, &policy).unwrap();
    let v = report(&f, r["jobId"].as_str().unwrap());
    assert_eq!(v["originalFragment"]["decisions"][0]["rationale"], old);
    let after = retention::plan(&f.engine.store, &Default::default(), &d()).unwrap();
    assert!(
        after["snapshotIds"]
            .as_array()
            .unwrap()
            .iter()
            .any(|id| id == &old)
    );
}

#[test]
fn actual_cli_permissions_closed_inputs_and_lost_stdout_recover_without_provider_config() {
    let empty = tempfile::tempdir().unwrap();
    let args = [
        "meaning-identity",
        "build",
        "--request",
        "missing.json",
        "--config",
        "missing.json",
        "--policy",
        "missing.json",
    ];
    let out = cli(empty.path(), &args);
    assert!(!out.status.success());
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("meaning_identity_write_permission_required")
    );
    assert!(!empty.path().join(".yoke").exists());
    let mut args = args.to_vec();
    args.push("--allow-writes");
    let out = cli(empty.path(), &args);
    assert!(
        String::from_utf8_lossy(&out.stderr)
            .contains("meaning_identity_provider_permission_required")
    );
    assert!(!empty.path().join(".yoke").exists());
    let f = fixture();
    let (p, policy, script, marker) = configured(&f, "success", "none");
    let r = request(&f);
    let root = f.engine.root.clone();
    let request_path = f.dir.path().join("request.json");
    let config = f.dir.path().join("config.json");
    let policy_path = f.dir.path().join("policy.json");
    fs::write(&request_path, serde_json::to_vec(&r).unwrap()).unwrap();
    fs::write(&config,json!({"command":p.command,"args":p.args,"modelId":p.model_id,"modelVersion":p.model_version,"taskId":p.task_id,"currency":p.currency,"maxCostMicros":10,"taskLimitMicros":100}).to_string()).unwrap();
    fs::write(&policy_path,json!({"contractHash":policy.contract_hash,"providerInputFiles":policy.provider_input_files}).to_string()).unwrap();
    drop(f.engine);
    let out = cli(
        &root,
        &[
            "meaning-identity",
            "candidates",
            "--request",
            request_path.to_str().unwrap(),
        ],
    );
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let raw = fs::read_to_string(&request_path).unwrap();
    fs::write(
        &request_path,
        raw.replacen(
            "\"schemaVersion\":1",
            "\"schemaVersion\":1,\"schemaVersion\":1",
            1,
        ),
    )
    .unwrap();
    let out = cli(
        &root,
        &[
            "--allow-writes",
            "meaning-identity",
            "build",
            "--trust-provider",
            "--request",
            request_path.to_str().unwrap(),
            "--config",
            config.to_str().unwrap(),
            "--policy",
            policy_path.to_str().unwrap(),
        ],
    );
    assert!(!out.status.success());
    assert!(!marker.exists());
    fs::write(&request_path, raw).unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_yoke"))
        .arg("--root")
        .arg(&root)
        .args([
            "--timeout",
            "60000",
            "--max-bytes",
            "16000000",
            "--allow-writes",
            "meaning-identity",
            "build",
            "--trust-provider",
            "--request",
        ])
        .arg(&request_path)
        .arg("--config")
        .arg(&config)
        .arg("--policy")
        .arg(&policy_path)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    drop(child.stdout.take());
    let lost = child.wait_with_output().unwrap();
    assert!(!lost.status.success());
    let engine = Engine::open(&root).unwrap();
    let jobs = api::jobs(
        &engine.store,
        "owned-identity-task",
        0,
        10,
        &Default::default(),
        &d(),
    )
    .unwrap();
    assert_eq!(jobs["totalJobs"], 1);
    let id = jobs["items"][0]["jobId"].as_str().unwrap().to_owned();
    assert_eq!(jobs["items"][0]["state"], "complete");
    assert!(String::from_utf8_lossy(&lost.stderr).contains(&id));
    drop(engine);
    for path in [request_path, config, policy_path, script] {
        fs::remove_file(path).unwrap();
    }
    for op in ["load", "status"] {
        let out = cli(&root, &["meaning-identity", op, "--job", &id]);
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
    let out = cli(
        &root,
        &[
            "meaning-identity",
            "load",
            "--job",
            &id,
            "--query",
            r#"{"operation":"degree"}"#,
        ],
    );
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let queried: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(queried["kind"], "meaning_identity_query_v1");
    assert_eq!(queried["rows"].as_array().unwrap().len(), 3);
    assert_eq!(fs::read_to_string(marker).unwrap().lines().count(), 1);
}
