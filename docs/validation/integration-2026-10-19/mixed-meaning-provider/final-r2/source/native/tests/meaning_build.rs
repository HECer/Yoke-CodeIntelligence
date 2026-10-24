//! Owned local child-process fixtures. These tests make no model-quality claims.
use serde_json::Value;
use std::{fs, path::PathBuf};
use yoke::{
    Engine,
    capture::Deadline,
    ledger::Ledger,
    meaning_build::{self, MeaningPolicy, MeaningSelection},
    model::{Graph, Snapshot, hash},
    providers::ProviderSpec,
};
struct Fixture {
    _dir: tempfile::TempDir,
    engine: Engine,
    snapshot: Snapshot,
    graph: Graph,
    provider: ProviderSpec,
    marker: PathBuf,
}
fn fixture(mode: &str) -> Fixture {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("workspace");
    fs::create_dir(&root).unwrap();
    fs::write(root.join("a.rs"), "pub fn alpha() -> u32 { 1 }\n").unwrap();
    fs::write(root.join("b.rs"), "pub fn beta() -> u32 { 2 }\n").unwrap();
    let script = dir.path().join("worker.cjs");
    let marker = dir.path().join("calls.txt");
    fs::write(&script,r#"
let raw='';process.stdin.on('data',b=>raw+=b);process.stdin.on('end',()=>{
 const r=JSON.parse(raw),fs=require('fs'),mode=process.argv[2],marker=process.argv[3];
 fs.appendFileSync(marker,r.requestId+'\n');const n=fs.readFileSync(marker,'utf8').trim().split('\n').length;
 const support=s=>[{sourceId:s.anchor.sourceId,startByte:s.anchor.startByte,endByte:s.anchor.endByte}];
 let data={concepts:r.payload.sources.map((s,i)=>({key:'c'+i,label:'Data flow',description:'Owned fixture source concept',aliases:['Flow'],support:support(s)})),relations:[],hyperedges:[],coverage:r.payload.sources.map(s=>({sourceId:s.anchor.sourceId,outcome:'claims'}))};
 if(r.payload.sources.length>1){data.relations=[{source:'c0',target:'c1',relation:'uses',description:'Owned cross-file fixture',support:[...support(r.payload.sources[0]),...support(r.payload.sources[1])]}];data.hyperedges=[{relation:'coordinates',participants:[{role:'input',concept:'c0'},{role:'output',concept:'c1'}],description:'Role-bearing fixture',support:[...support(r.payload.sources[0]),...support(r.payload.sources[1])]}];}
 if(mode==='no-claims')data={concepts:[],relations:[],hyperedges:[],coverage:r.payload.sources.map(s=>({sourceId:s.anchor.sourceId,outcome:'no_claims'}))};
 if(mode==='literal-query'){data.concepts[0].label='a"b';data.concepts[0].aliases=['C:\\src'];}
 if(mode==='second-invalid'&&n===2)data.concepts[0].support[0].startByte=999999;
 if(mode==='outside-support')data.concepts[0].support[0].sourceId='0'.repeat(64);
 let reply={schema:1,operation:r.operation,model:r.model,modelVersion:r.modelVersion,requestId:r.requestId,inputHash:r.inputHash,sourceBindings:r.payload.sources.map(s=>s.anchor),currency:r.currency,actualCostMicros:mode==='unknown'?null:7,usage:{inputTokens:10,outputTokens:3,cachedInputTokens:2},data};
 if(mode==='wrong-binding')reply.inputHash='0'.repeat(64);
 if(mode==='unsafe-error')reply.error='\u0001untrusted-control-error';
 if(mode==='bad-cost-schema')reply.actualCostMicros='PRIVATE_PROVIDER_FIELD';
 if(mode==='overrun')reply.actualCostMicros=60;
 if(mode==='broken')return process.stdout.write('{broken');
 if(mode==='duplicate')return process.stdout.write(JSON.stringify(reply).replace('"schema":1','"schema":1,"schema":1'));
 process.stdout.write(JSON.stringify(reply));if(mode==='exit-error')process.exitCode=1;
});"#).unwrap();
    let mut engine = Engine::open(&root).unwrap();
    let (snapshot, graph, _, _) = engine.index(&Deadline::new(5000)).unwrap();
    let provider = ProviderSpec {
        trusted: true,
        command: PathBuf::from("node"),
        args: vec![
            script.to_str().unwrap().into(),
            mode.into(),
            marker.to_str().unwrap().into(),
        ],
        model_id: "owned-meaning-fixture".into(),
        model_version: "v1".into(),
        task_id: "meaning-fixture".into(),
        currency: "EUR".into(),
        max_cost_micros: 10,
        task_limit_micros: 100,
    };
    Fixture {
        _dir: dir,
        engine,
        snapshot,
        graph,
        provider,
        marker,
    }
}
fn policy(f: &Fixture) -> MeaningPolicy {
    MeaningPolicy::files(&f.snapshot, hash("owned prompt and policy"))
}
fn run(f: &Fixture, p: &MeaningPolicy) -> anyhow::Result<Value> {
    meaning_build::build(
        &f.engine.store,
        &f.snapshot,
        &f.graph,
        &f.provider,
        p,
        &Deadline::new(15000),
    )
}
fn calls(f: &Fixture) -> usize {
    fs::read_to_string(&f.marker)
        .map(|s| s.lines().count())
        .unwrap_or(0)
}
fn count(f: &Fixture, kind: &str) -> usize {
    f.engine
        .store
        .db
        .query_row("SELECT count(*) FROM records WHERE kind=?1", [kind], |r| {
            r.get(0)
        })
        .unwrap()
}
fn totals(f: &Fixture) -> yoke::ledger::Totals {
    Ledger::new(&f.engine.store, &Deadline::new(5000))
        .total(&f.provider.task_id)
        .unwrap()
}
#[test]
fn publishes_source_bound_claims_then_reuses_receipts_with_zero_calls_or_reservations() {
    let f = fixture("ok");
    let p = policy(&f);
    let first = run(&f, &p).expect("meaning extraction");
    assert_eq!(first["status"], "ready");
    assert_eq!(calls(&f), 2);
    assert_eq!(first["modelCallsThisRun"], 2);
    assert_eq!(count(&f, "native_meaning_cache_v1"), 2);
    let loaded = meaning_build::load(
        &f.engine.store,
        first["graphId"].as_str().unwrap(),
        &Deadline::new(5000),
    )
    .unwrap();
    assert_eq!(loaded["resolution"], "inferred_concept");
    assert_eq!(loaded["providerAttestation"], "unverified");
    assert_eq!(loaded["packets"].as_array().unwrap().len(), 2);
    let second = run(&f, &p).expect("all cache hits");
    assert_eq!(calls(&f), 2);
    assert_eq!(second["modelCallsThisRun"], 0);
    assert_eq!(second["cacheHits"], 2);
    assert_eq!(totals(&f).attempts, 2);
    assert_eq!(totals(&f).actual_micros, 14);
    assert!(
        !meaning_build::query(
            &f.engine.store,
            second["graphId"].as_str().unwrap(),
            "Flow",
            &Deadline::new(5000)
        )
        .unwrap()["packets"]
            .as_array()
            .unwrap()
            .is_empty()
    );
}
#[test]
fn every_selected_source_budget_and_graph_sink_is_admitted_before_first_child() {
    for mode in 0..6 {
        let mut f = fixture("ok");
        let mut p = policy(&f);
        match mode {
            0 => f.provider.task_limit_micros = 15,
            1 => p.max_packets = 1,
            2 => p.max_total_source_bytes = 1,
            3 => p.max_graph_bytes = 1,
            4 => p.selection = MeaningSelection::Files(vec!["a.rs".into(), "missing.rs".into()]),
            _ => {
                f.snapshot.files[1].hash = "0".repeat(64);
            }
        }
        assert!(run(&f, &p).is_err(), "admission mode {mode}");
        assert_eq!(calls(&f), 0);
        assert_eq!(totals(&f).attempts, 0);
        assert_eq!(count(&f, "native_meaning_job_v1"), 0);
    }
}
#[test]
fn later_invalid_claim_accounts_bill_but_publishes_no_partial_graph_or_cache() {
    let f = fixture("second-invalid");
    assert!(run(&f, &policy(&f)).is_err());
    assert_eq!(calls(&f), 2);
    assert_eq!(totals(&f).actual_micros, 14);
    assert_eq!(count(&f, "native_meaning_cache_v1"), 0);
    assert_eq!(count(&f, "native_meaning_graph_v1"), 0);
    let jobs: Vec<Value> = f.engine.store.records("native_meaning_job_v1").unwrap();
    assert_eq!(jobs.len(), 1);
    assert_eq!(jobs[0]["state"], "failed");
}
#[test]
fn malformed_or_unbound_replies_keep_sent_hold_and_cancel_only_unsent() {
    for mode in ["wrong-binding", "broken", "duplicate"] {
        let f = fixture(mode);
        assert!(run(&f, &policy(&f)).is_err());
        assert_eq!(calls(&f), 1);
        let t = totals(&f);
        assert_eq!(t.attempts, 2);
        assert_eq!(t.unknown, 1);
        assert_eq!(t.reserved_micros, 10);
        assert_eq!(count(&f, "native_meaning_cache_v1"), 0);
    }
}
#[test]
fn accounted_overrun_blocks_later_reserved_send_and_never_publishes() {
    let f = fixture("overrun");
    assert!(run(&f, &policy(&f)).is_err());
    assert_eq!(calls(&f), 1);
    assert_eq!(totals(&f).actual_micros, 60);
    assert_eq!(totals(&f).reserved_micros, 0);
    assert_eq!(count(&f, "native_meaning_graph_v1"), 0);
}
#[test]
fn unknown_billing_remains_reserved_after_valid_graph_publication() {
    let f = fixture("unknown");
    let r = run(&f, &policy(&f)).unwrap();
    assert_eq!(r["status"], "ready");
    assert!(r["providerReportedCostMicros"].is_null());
    assert_eq!(r["knownReportedCostMicros"], 0);
    let t = totals(&f);
    assert_eq!(t.unknown, 2);
    assert_eq!(t.reserved_micros, 20);
    assert_eq!(t.actual_micros, 0);
}
#[test]
fn no_claims_is_explicit_successful_coverage() {
    let f = fixture("no-claims");
    let r = run(&f, &policy(&f)).unwrap();
    let s = meaning_build::status(
        &f.engine.store,
        r["jobId"].as_str().unwrap(),
        None,
        &Deadline::new(5000),
    )
    .unwrap();
    assert_eq!(s["availability"], "ready");
    assert_eq!(s["coverage"]["noClaims"], 2);
}
#[test]
fn fixed_cross_file_packet_preserves_relations_and_hyperedge_roles() {
    let f = fixture("ok");
    let mut p = policy(&f);
    p.context_groups = vec![vec!["a.rs".into(), "b.rs".into()]];
    let r = run(&f, &p).unwrap();
    assert_eq!(calls(&f), 3);
    let g = meaning_build::load(
        &f.engine.store,
        r["graphId"].as_str().unwrap(),
        &Deadline::new(5000),
    )
    .unwrap();
    let cross = &g["packets"][2]["meaning"];
    assert_eq!(cross["relations"].as_array().unwrap().len(), 1);
    assert_eq!(cross["hyperedges"][0]["participants"][0]["role"], "input");
    assert_eq!(cross["hyperedges"][0]["participants"][1]["role"], "output");
}
#[test]
fn unchanged_body_cache_rebinds_new_snapshot_and_changed_body_invalidates_it() {
    let mut f = fixture("ok");
    let p = policy(&f);
    let first = run(&f, &p).unwrap();
    fs::write(f.engine.root.join("c.txt"), "unrelated source\n").unwrap();
    let (s, g, _, _) = f.engine.index(&Deadline::new(5000)).unwrap();
    f.snapshot = s;
    f.graph = g;
    let rebound = run(&f, &p).unwrap();
    assert_eq!(rebound["modelCallsThisRun"], 0);
    assert_ne!(rebound["graphId"], first["graphId"]);
    assert_eq!(calls(&f), 2);
    let stale = meaning_build::status(
        &f.engine.store,
        first["jobId"].as_str().unwrap(),
        Some(&f.snapshot.id),
        &Deadline::new(5000),
    )
    .unwrap();
    assert_eq!(stale["freshness"], "current");
    fs::write(f.engine.root.join("a.rs"), "pub fn alpha() -> u32 { 3 }\n").unwrap();
    let (s, g, _, _) = f.engine.index(&Deadline::new(5000)).unwrap();
    f.snapshot = s;
    f.graph = g;
    let changed = run(&f, &p).unwrap();
    assert_eq!(changed["modelCallsThisRun"], 1);
    assert_eq!(calls(&f), 3);
    let stale = meaning_build::status(
        &f.engine.store,
        first["jobId"].as_str().unwrap(),
        Some(&f.snapshot.id),
        &Deadline::new(5000),
    )
    .unwrap();
    assert_eq!(stale["freshness"], "changed");
}
#[test]
fn altered_contract_and_provider_version_do_not_reuse_old_claims() {
    let mut f = fixture("ok");
    let mut p = policy(&f);
    run(&f, &p).unwrap();
    p.contract_hash = hash("changed policy");
    run(&f, &p).unwrap();
    assert_eq!(calls(&f), 4);
    f.provider.model_version = "v2".into();
    run(&f, &p).unwrap();
    assert_eq!(calls(&f), 6);
}
#[test]
fn forged_cache_origin_is_rejected_without_starting_worker() {
    let f = fixture("ok");
    let p = policy(&f);
    run(&f, &p).unwrap();
    f.engine.store.db.execute("UPDATE records SET value=json_set(value,'$.responseArtifactId',?1) WHERE kind='native_meaning_cache_v1'",["0".repeat(64)]).unwrap();
    assert!(run(&f, &p).is_err());
    assert_eq!(calls(&f), 2);
    assert_eq!(totals(&f).attempts, 2);
}
#[test]
fn publication_failure_rolls_back_every_new_cache_pointer() {
    let f = fixture("ok");
    f.engine.store.db.execute_batch("CREATE TRIGGER reject_meaning_graph BEFORE INSERT ON records WHEN NEW.kind='native_meaning_graph_v1' BEGIN SELECT RAISE(ABORT,'owned publication fault'); END;").unwrap();
    assert!(run(&f, &policy(&f)).is_err());
    assert_eq!(calls(&f), 2);
    assert_eq!(totals(&f).actual_micros, 14);
    assert_eq!(count(&f, "native_meaning_cache_v1"), 0);
    assert_eq!(count(&f, "native_meaning_graph_v1"), 0);
}
#[test]
fn body_selection_uses_full_body_and_invalidates_unchanged_signature_changes() {
    let mut f = fixture("ok");
    let entity = f
        .graph
        .entities
        .iter()
        .find(|e| e.name == "alpha" && e.kind != "file")
        .unwrap();
    let mut p = policy(&f);
    p.selection = MeaningSelection::Bodies(vec![entity.id.clone()]);
    run(&f, &p).unwrap();
    assert_eq!(calls(&f), 1);
    fs::write(f.engine.root.join("a.rs"), "pub fn alpha() -> u32 { 4 }\n").unwrap();
    let (s, g, _, _) = f.engine.index(&Deadline::new(5000)).unwrap();
    f.snapshot = s;
    f.graph = g;
    let entity = f
        .graph
        .entities
        .iter()
        .find(|e| e.name == "alpha" && e.kind != "file")
        .unwrap();
    p.selection = MeaningSelection::Bodies(vec![entity.id.clone()]);
    let changed = run(&f, &p).unwrap();
    assert_eq!(changed["modelCallsThisRun"], 1);
    assert_eq!(calls(&f), 2);
}
#[test]
fn oversized_source_is_rejected_as_complete_selection_before_worker() {
    let mut f = fixture("ok");
    fs::write(
        f.engine.root.join("a.rs"),
        format!("pub fn alpha() {{ /*{}*/ }}\n", "x".repeat(17000)),
    )
    .unwrap();
    let (s, g, _, _) = f.engine.index(&Deadline::new(5000)).unwrap();
    f.snapshot = s;
    f.graph = g;
    assert!(run(&f, &policy(&f)).is_err());
    assert_eq!(calls(&f), 0);
    assert_eq!(totals(&f).attempts, 0);
}
#[test]
fn retained_graph_validates_original_receipt_and_rebound_snapshot_dependencies() {
    let f = fixture("ok");
    let r = run(&f, &policy(&f)).unwrap();
    let id = r["graphId"].as_str().unwrap();
    let value: Value = f
        .engine
        .store
        .get_record("native_meaning_graph_v1", id)
        .unwrap();
    let deps = meaning_build::validate_retained_record(
        &f.engine.store,
        "native_meaning_graph_v1",
        id,
        &value,
        &Deadline::new(5000),
    )
    .unwrap();
    assert!(deps.snapshots.contains(&f.snapshot.id));
    assert!(deps.artifacts.len() >= 3);
    let attempt: Value = f
        .engine
        .store
        .records::<Value>("native_model_attempt_v1")
        .unwrap()
        .remove(0);
    f.engine
        .store
        .db
        .execute(
            "DELETE FROM records WHERE kind='native_model_attempt_v1' AND id=?1",
            [attempt["id"].as_str().unwrap()],
        )
        .unwrap();
    assert!(meaning_build::load(&f.engine.store, id, &Deadline::new(5000)).is_err());
    assert_eq!(calls(&f), 2);
}
#[test]
fn in_place_provider_script_change_invalidates_cache_even_when_command_is_unchanged() {
    let f = fixture("ok");
    let p = policy(&f);
    run(&f, &p).unwrap();
    let script = PathBuf::from(&f.provider.args[0]);
    let mut text = fs::read_to_string(&script).unwrap();
    text.push_str("\n// Host worker implementation changed.\n");
    fs::write(script, text).unwrap();
    let r = run(&f, &p).unwrap();
    assert_eq!(r["modelCallsThisRun"], 2);
    assert_eq!(calls(&f), 4);
}
#[test]
fn exact_snapshot_analyzer_epochs_have_separate_cache_namespaces() {
    let mut f = fixture("ok");
    let p = policy(&f);
    run(&f, &p).unwrap();
    f.engine.capture_epoch = yoke::analysis::AnalyzerEpoch::V9;
    let (s, g, _, _) = f.engine.index(&Deadline::new(5000)).unwrap();
    f.snapshot = s;
    f.graph = g;
    let r = run(&f, &p).unwrap();
    assert_eq!(r["modelCallsThisRun"], 2);
    assert_eq!(calls(&f), 4);
}
#[test]
fn valid_bound_bill_is_preserved_when_worker_exits_unsuccessfully() {
    let f = fixture("exit-error");
    assert!(run(&f, &policy(&f)).is_err());
    assert_eq!(calls(&f), 1);
    assert_eq!(totals(&f).actual_micros, 7);
    assert_eq!(totals(&f).reserved_micros, 0);
    assert_eq!(count(&f, "native_meaning_cache_v1"), 0);
}
#[test]
fn cache_reuse_requires_the_original_completed_graph_registration() {
    let f = fixture("ok");
    let p = policy(&f);
    let first = run(&f, &p).unwrap();
    f.engine
        .store
        .db
        .execute(
            "DELETE FROM records WHERE kind='native_meaning_graph_v1' AND id=?1",
            [first["graphId"].as_str().unwrap()],
        )
        .unwrap();
    assert!(run(&f, &p).is_err());
    assert_eq!(calls(&f), 2);
}
#[test]
fn failed_status_counts_only_dispatched_attempts_and_reports_missing_unknown_receipts() {
    let f = fixture("wrong-binding");
    assert!(run(&f, &policy(&f)).is_err());
    let jobs: Vec<Value> = f.engine.store.records("native_meaning_job_v1").unwrap();
    let status = meaning_build::status(
        &f.engine.store,
        jobs[0]["jobId"].as_str().unwrap(),
        None,
        &Deadline::new(5000),
    )
    .unwrap();
    assert!(status["modelCallsThisRun"].is_null());
    assert_eq!(status["dispatchedAttempts"], 1);
    assert_eq!(status["unknownCosts"], 1);
    assert_eq!(
        status["unreleasedReservations"].as_array().unwrap().len(),
        0
    );
}
#[test]
fn immutable_response_preserves_provider_local_claim_keys_before_normalization() {
    let f = fixture("ok");
    let first = run(&f, &policy(&f)).unwrap();
    let job: Value = f
        .engine
        .store
        .get_record("native_meaning_job_v1", first["jobId"].as_str().unwrap())
        .unwrap();
    let bytes = f
        .engine
        .store
        .read_artifact(job["packets"][0]["responseArtifactId"].as_str().unwrap())
        .unwrap();
    let response: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(response["fragment"]["concepts"][0]["key"], "c0");
}
#[test]
fn query_matches_literal_quotes_and_backslashes_in_displayed_claim_text() {
    let f = fixture("literal-query");
    let first = run(&f, &policy(&f)).unwrap();
    for term in ["a\"b", "C:\\src"] {
        let result = meaning_build::query(
            &f.engine.store,
            first["graphId"].as_str().unwrap(),
            term,
            &Deadline::new(5000),
        )
        .unwrap();
        assert!(
            !result["packets"].as_array().unwrap().is_empty(),
            "literal query {term:?}"
        );
    }
}
#[test]
fn fully_bound_bill_is_accounted_even_when_provider_error_text_is_unsafe() {
    let f = fixture("unsafe-error");
    let error = run(&f, &policy(&f)).unwrap_err();
    assert_eq!(calls(&f), 1);
    assert_eq!(totals(&f).actual_micros, 7);
    assert_eq!(totals(&f).unknown, 0);
    assert!(!format!("{error:#}").contains("untrusted-control-error"));
}
#[test]
fn malformed_provider_schema_does_not_echo_untrusted_field_values() {
    let f = fixture("bad-cost-schema");
    let error = run(&f, &policy(&f)).unwrap_err();
    assert!(!format!("{error:#}").contains("PRIVATE_PROVIDER_FIELD"));
    assert_eq!(totals(&f).unknown, 1);
}
#[test]
fn retained_cache_value_must_match_the_validated_immutable_row() {
    let f = fixture("ok");
    run(&f, &policy(&f)).unwrap();
    let mut value: Value = f
        .engine
        .store
        .records::<Value>("native_meaning_cache_v1")
        .unwrap()
        .remove(0);
    let id = value["cacheKey"].as_str().unwrap().to_owned();
    value["generation"] = serde_json::json!("forged-generation");
    assert!(
        meaning_build::validate_retained_record(
            &f.engine.store,
            "native_meaning_cache_v1",
            &id,
            &value,
            &Deadline::new(5000)
        )
        .is_err()
    );
}
#[test]
fn currency_scoped_cache_prevents_mixing_old_and_new_monetary_units() {
    let mut f = fixture("ok");
    run(&f, &policy(&f)).unwrap();
    fs::write(f.engine.root.join("c.rs"), "pub fn gamma() -> u32 { 3 }\n").unwrap();
    let (s, g, _, _) = f.engine.index(&Deadline::new(5000)).unwrap();
    f.snapshot = s;
    f.graph = g;
    f.provider.currency = "USD".into();
    f.provider.task_id = "meaning-usd".into();
    let result = run(&f, &policy(&f)).unwrap();
    assert_eq!(result["modelCallsThisRun"], 3);
    assert_eq!(result["cacheHits"], 0);
    assert_eq!(result["currency"], "USD");
    assert_eq!(calls(&f), 5);
    assert_eq!(result["thisRunProviderReportedCostMicros"], 21);
}
