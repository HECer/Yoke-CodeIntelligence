//! Labeled child-process fixtures, not model quality or remote billing evidence.
use serde_json::{Value, json};
use std::{fs, path::PathBuf};
use yoke::{
    Engine,
    capture::Deadline,
    model::{Graph, Snapshot, identity},
    model_bedrock::{BedrockSpec, Dialect},
    provider_batches::{self, EmbeddingBatchPolicy},
    providers::ProviderSpec,
};
struct Fixture {
    _dir: tempfile::TempDir,
    engine: Engine,
    snapshot: Snapshot,
    graph: Graph,
    provider: ProviderSpec,
    spec: BedrockSpec,
    marker: PathBuf,
}
fn fixture(mode: &str) -> Fixture {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("workspace");
    fs::create_dir(&root).unwrap();
    fs::write(
        root.join("fixture.rs"),
        if mode=="huge-usage" { "pub fn first() -> u32 { 1 }\npub fn second() -> u32 { 2 }\npub fn third() -> u32 { 3 }\n" }else{ "pub fn first() -> u32 { 1 }\npub fn second() -> u32 { 2 }\n" },
    )
    .unwrap();
    let script = dir.path().join("provider.cjs");
    let marker = dir.path().join("calls.txt");
    fs::write(&script,r#"
let bytes='';process.stdin.on('data',b=>bytes+=b);process.stdin.on('end',()=>{
 const r=JSON.parse(bytes),fs=require('fs'),mode=process.argv[2],marker=process.argv[3];
 fs.appendFileSync(marker,r.requestId+'\n');const calls=fs.readFileSync(marker,'utf8').trim().split('\n').length;
 let reply={schema:1,operation:r.operation,model:r.model,modelVersion:r.modelVersion,requestId:r.requestId,inputHash:r.inputHash,sourceBindings:r.payload.sources.map(s=>s.anchor),currency:r.currency,actualCostMicros:7,usage:{inputTokens:10,outputTokens:3,cachedInputTokens:2},data:{items:r.payload.sources.map(s=>({sourceId:s.anchor.sourceId,vector:[1,...Array(mode==='reverse'?1023:255).fill(0)]}))}};
 if(mode==='second-fails'&&calls===2){reply.error='owned fixture failure';delete reply.data;}
 if(mode==='second-invalid'&&calls===2){reply.data.items[0].vector=[0];}
 if(mode==='wrong-binding'){reply.sourceBindings[0].contentHash='0'.repeat(64);}
 if(mode==='unknown'){reply.actualCostMicros=null;reply.usage=null;}
 if(mode==='overrun'){reply.actualCostMicros=60;}
 if(mode==='timeout'&&calls===1){return setTimeout(()=>process.stdout.write(JSON.stringify(reply)),10000);}
 if(mode==='reverse')reply.data.items.reverse();
 if(mode==='huge-usage'){reply.usage={inputTokens:'HUGE',outputTokens:0,cachedInputTokens:0};return process.stdout.write(JSON.stringify(reply).replace('"HUGE"','9223372036854775807'));}
 process.stdout.write(JSON.stringify(reply));
});
"#).unwrap();
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
        model_id: "fixture".into(),
        model_version: "v1".into(),
        task_id: "batch-fixture".into(),
        currency: "EUR".into(),
        max_cost_micros: 10,
        task_limit_micros: 100,
    };
    let spec = BedrockSpec {
        trusted_network: true,
        endpoint: "http://127.0.0.1:1/".into(),
        region: "us-east-1".into(),
        remote_model_id: "fixture".into(),
        model: "fixture".into(),
        model_version: "v1".into(),
        credential_name: "unused".into(),
        dialect: Dialect::TitanV2 {
            dimensions: 256,
            normalize: true,
        },
        allow_loopback_http: true,
        additional_root_certificates: vec![],
    };
    Fixture {
        _dir: dir,
        engine,
        snapshot,
        graph,
        provider,
        spec,
        marker,
    }
}
fn policy(f: &Fixture) -> EmbeddingBatchPolicy {
    EmbeddingBatchPolicy::for_bedrock(&f.spec, 10, 10, 100_000).expect("explicit batch policy")
}
fn run(f: &Fixture) -> anyhow::Result<Value> {
    provider_batches::generate_embeddings_batched(
        &f.engine.store,
        &f.snapshot,
        &f.graph,
        &f.provider,
        &policy(f),
        &Deadline::new(10000),
    )
}
fn batch_rows(f: &Fixture) -> Vec<Value> {
    f.engine.store.records("native_model_batch_v1").unwrap()
}
fn calls(f: &Fixture) -> usize {
    fs::read_to_string(&f.marker)
        .map(|s| s.lines().count())
        .unwrap_or(0)
}
#[test]
fn two_titan_chunks_have_separate_attempts_and_one_complete_legacy_vector_collection() {
    let f = fixture("ok");
    let result = run(&f).expect("parent-managed two-chunk embedding generation");
    assert_eq!(calls(&f), 2);
    assert_eq!(result["status"], "complete");
    assert_eq!(result["modelCalls"], 2);
    assert_eq!(result["count"], 2);
    assert_eq!(result["dimension"], 256);
    assert_eq!(result["providerReportedCostMicros"], 14);
    assert_eq!(
        result["providerReportedUsage"],
        json!({"inputTokens":20,"outputTokens":6,"cachedInputTokens":4})
    );
    let attempts = result["attemptIds"].as_array().unwrap();
    assert_eq!(attempts.len(), 2);
    assert_ne!(attempts[0], attempts[1]);
    let collection: Value = f
        .engine
        .store
        .get_record(
            "native_vectors_v1",
            &identity(&(f.snapshot.id.as_str(), "fixture@v1")),
        )
        .unwrap();
    assert_eq!(collection["items"].as_array().unwrap().len(), 2);
    assert_eq!(batch_rows(&f)[0]["state"], "complete");
    let total = yoke::ledger::Ledger::new(&f.engine.store, &Deadline::new(5000))
        .total("batch-fixture")
        .unwrap();
    assert_eq!(total.attempts, 2);
    assert_eq!(total.actual_micros, 14);
    assert_eq!(total.encumbered_micros, 14);
}
#[test]
fn budget_call_limits_source_limits_and_existing_namespace_reject_before_first_child() {
    for mode in 0..4 {
        let mut f = fixture("ok");
        let (calls_limit, source_limit) = match mode {
            0 => {
                f.provider.task_limit_micros = 15;
                (10, 100_000)
            }
            1 => (1, 100_000),
            2 => (10, 1),
            _ => {
                let entity = f.graph.entities.iter().find(|e| e.kind != "file").unwrap();
                let file = f
                    .snapshot
                    .files
                    .iter()
                    .find(|file| file.path == entity.path)
                    .unwrap();
                yoke::knowledge::import_vectors(&f.engine.store,&f.snapshot,&f.graph,json!({"snapshotId":f.snapshot.id,"model":"fixture@v1","items":[{"entityId":entity.id,"contentHash":file.hash,"vector":[1,0]}]}),&Deadline::new(5000)).unwrap();
                (10, 100_000)
            }
        };
        let p = EmbeddingBatchPolicy::for_bedrock(&f.spec, calls_limit, 10, source_limit).unwrap();
        assert!(
            provider_batches::generate_embeddings_batched(
                &f.engine.store,
                &f.snapshot,
                &f.graph,
                &f.provider,
                &p,
                &Deadline::new(5000)
            )
            .is_err()
        );
        assert_eq!(calls(&f), 0, "mode {mode}");
        let totals = yoke::ledger::Ledger::new(&f.engine.store, &Deadline::new(5000))
            .total("batch-fixture")
            .unwrap();
        assert_eq!(totals.attempts, 0, "mode {mode}");
        assert!(batch_rows(&f).is_empty());
    }
}
#[test]
fn partial_failure_preserves_each_reported_cost_and_never_publishes_partial_corpus() {
    for mode in ["second-fails", "second-invalid"] {
        let f = fixture(mode);
        assert!(run(&f).is_err());
        assert_eq!(calls(&f), 2);
        let count: usize = f
            .engine
            .store
            .db
            .query_row(
                "SELECT count(*) FROM records WHERE kind='native_vectors_v1' AND id=?1",
                [identity(&(f.snapshot.id.as_str(), "fixture@v1"))],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 0);
        let total = yoke::ledger::Ledger::new(&f.engine.store, &Deadline::new(5000))
            .total("batch-fixture")
            .unwrap();
        assert_eq!(total.actual_micros, 14);
        assert_eq!(total.attempts, 2);
        let rows = batch_rows(&f);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0]["state"], "failed");
        assert_eq!(
            rows[0]["chunks"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|c| c["responseArtifactId"].is_string())
                .count(),
            2
        );
    }
}
#[test]
fn first_cost_overrun_blocks_remaining_reserved_chunk_and_releases_only_unsent() {
    let f = fixture("overrun");
    assert!(run(&f).is_err());
    assert_eq!(calls(&f), 1);
    let total = yoke::ledger::Ledger::new(&f.engine.store, &Deadline::new(5000))
        .total("batch-fixture")
        .unwrap();
    assert_eq!(total.actual_micros, 60);
    assert_eq!(total.reserved_micros, 0);
    assert!(total.reservation_exceeded);
    assert_eq!(total.attempts, 2);
}
#[test]
fn unknown_costs_remain_null_and_retain_both_reservations() {
    let f = fixture("unknown");
    let result = run(&f).unwrap();
    assert!(result["providerReportedCostMicros"].is_null());
    assert!(result["providerReportedUsage"].is_null());
    assert_eq!(result["unknownCostAttempts"], 2);
    let total = yoke::ledger::Ledger::new(&f.engine.store, &Deadline::new(5000))
        .total("batch-fixture")
        .unwrap();
    assert_eq!(total.unknown, 2);
    assert_eq!(total.encumbered_micros, 20);
}
#[test]
fn initial_batch_registration_failure_rolls_back_all_reservations_before_launch() {
    let f = fixture("ok");
    f.engine.store.db.execute_batch("CREATE TRIGGER reject_batch BEFORE INSERT ON records WHEN NEW.kind='native_model_batch_v1' BEGIN SELECT RAISE(ABORT,'owned registration failure'); END;").unwrap();
    assert!(run(&f).is_err());
    assert_eq!(calls(&f), 0);
    assert!(batch_rows(&f).is_empty());
    let total = yoke::ledger::Ledger::new(&f.engine.store, &Deadline::new(5000))
        .total("batch-fixture")
        .unwrap();
    assert_eq!(total.attempts, 0);
    assert_eq!(total.encumbered_micros, 0);
}
#[test]
fn vector_and_complete_batch_publication_roll_back_together_after_billed_chunks() {
    let f = fixture("ok");
    f.engine.store.db.execute_batch("CREATE TRIGGER reject_completion BEFORE UPDATE ON records WHEN NEW.kind='native_model_batch_v1' AND json_extract(NEW.value,'$.state')='complete' BEGIN SELECT RAISE(ABORT,'owned publication failure'); END;").unwrap();
    assert!(run(&f).is_err());
    assert_eq!(calls(&f), 2);
    let count: usize = f
        .engine
        .store
        .db
        .query_row(
            "SELECT count(*) FROM records WHERE kind='native_vectors_v1'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(count, 0);
    let rows = batch_rows(&f);
    assert_eq!(rows[0]["state"], "failed");
    assert!(rows[0]["vectorCollectionId"].is_null());
    let totals = yoke::ledger::Ledger::new(&f.engine.store, &Deadline::new(5000))
        .total("batch-fixture")
        .unwrap();
    assert_eq!(totals.actual_micros, 14);
}
#[test]
fn another_state_client_blocks_batch_before_spending_and_restores_original_client() {
    let f = fixture("ok");
    let other = Engine::open(&f.engine.root).unwrap();
    assert!(run(&f).is_err());
    assert_eq!(calls(&f), 0);
    assert!(batch_rows(&f).is_empty());
    drop(other);
    assert!(run(&f).is_ok());
    assert_eq!(calls(&f), 2);
}
#[test]
fn deadline_preserves_dispatched_unknown_and_reports_unreleased_unsent_without_retry() {
    let f = fixture("timeout");
    let cancel = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let signal = cancel.clone();
    let marker = f.marker.clone();
    let observed = std::thread::spawn(move || {
        let until = std::time::Instant::now() + std::time::Duration::from_secs(8);
        while std::time::Instant::now() < until && !marker.exists() {
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        assert!(
            marker.exists(),
            "Owned slow child must announce dispatch before cancellation"
        );
        signal.store(true, std::sync::atomic::Ordering::SeqCst);
    });
    let result = Deadline::run_bounded(&Deadline::new(10000), cancel, || {
        provider_batches::generate_embeddings_batched(
            &f.engine.store,
            &f.snapshot,
            &f.graph,
            &f.provider,
            &policy(&f),
            &Deadline::new(10000),
        )
    });
    observed.join().unwrap();
    assert!(result.is_err());
    assert_eq!(calls(&f), 1);
    let rows = batch_rows(&f);
    assert_eq!(rows.len(), 1);
    let id = rows[0]["batchId"].as_str().unwrap();
    let described =
        provider_batches::describe_embedding_batch(&f.engine.store, id, &Deadline::new(5000))
            .unwrap();
    assert_eq!(described["status"], "incomplete");
    assert!(described["modelCalls"].is_null());
    assert_eq!(described["dispatchedAttempts"], 1);
    assert_eq!(
        described["uncertainAttemptIds"].as_array().unwrap().len(),
        1
    );
    assert_eq!(
        described["unreleasedUnsentAttemptIds"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(calls(&f), 1);
    let totals = yoke::ledger::Ledger::new(&f.engine.store, &Deadline::new(5000))
        .total("batch-fixture")
        .unwrap();
    assert_eq!(totals.encumbered_micros, 20);
    assert_eq!(totals.unknown, 1);
}
#[test]
fn completed_and_failed_batches_are_typed_retention_roots_not_unknown_records() {
    for mode in ["ok", "second-fails"] {
        let f = fixture(mode);
        let _ = run(&f);
        let report = yoke::retention::plan(
            &f.engine.store,
            &yoke::retention::Policy::default(),
            &Deadline::new(10000),
        )
        .unwrap();
        assert_eq!(
            report["unknownDependencyKinds"],
            json!([]),
            "{mode}: {report}"
        );
        assert!(report["snapshotIds"].as_array().unwrap().is_empty());
    }
}
#[test]
fn bound_unknown_cost_can_later_reconcile_without_invalidating_original_response_artifact() {
    let f = fixture("unknown");
    let result = run(&f).unwrap();
    let id = result["batchId"].as_str().unwrap();
    let attempt = result["attemptIds"][0].as_str().unwrap();
    let deadline = Deadline::new(5000);
    let ledger = yoke::ledger::Ledger::new(&f.engine.store, &deadline);
    ledger.settle(attempt, Some(4)).unwrap();
    ledger
        .usage(
            attempt,
            "later-host-observation",
            yoke::ledger::Usage {
                input_tokens: 20,
                output_tokens: 0,
                cached_input_tokens: 0,
            },
        )
        .unwrap();
    let described = provider_batches::describe_embedding_batch(&f.engine.store, id, &deadline)
        .expect("later reconciliation must preserve the checked source/vector receipt");
    assert_eq!(described["knownReportedCostMicros"], 4);
    assert_eq!(described["unknownCostAttempts"], 1);
}
#[test]
fn id_addressed_reversed_cohere_response_is_reordered_before_atomic_publication() {
    let mut f = fixture("reverse");
    f.spec.dialect = Dialect::CohereV3;
    let result = run(&f).unwrap();
    assert_eq!(calls(&f), 1);
    assert_eq!(result["count"], 2);
    assert_eq!(result["dimension"], 1024);
}
#[test]
fn valid_large_per_attempt_usage_never_overflows_or_invalidates_completed_corpus() {
    let f = fixture("huge-usage");
    let result = run(&f).expect("valid per-attempt counters must preserve completed publication");
    assert_eq!(result["status"], "complete");
    assert_eq!(calls(&f), 3);
    assert!(result["providerReportedUsage"].is_null());
    assert_eq!(result["reportedUsageOverflow"], true);
    assert_eq!(
        result["knownReportedUsageDecimal"]["inputTokens"],
        "27670116110564327421"
    );
    let rows = batch_rows(&f);
    assert_eq!(rows[0]["state"], "complete");
    assert!(
        provider_batches::describe_embedding_batch(
            &f.engine.store,
            rows[0]["batchId"].as_str().unwrap(),
            &Deadline::new(5000)
        )
        .is_ok()
    );
}
#[test]
fn initially_unknown_usage_can_receive_its_first_response_report_later() {
    let f = fixture("unknown");
    let result = run(&f).unwrap();
    let deadline = Deadline::new(5000);
    let ledger = yoke::ledger::Ledger::new(&f.engine.store, &deadline);
    let attempt = result["attemptIds"][0].as_str().unwrap();
    ledger
        .usage(
            attempt,
            "response",
            yoke::ledger::Usage {
                input_tokens: 11,
                output_tokens: 0,
                cached_input_tokens: 2,
            },
        )
        .unwrap();
    let described = provider_batches::describe_embedding_batch(
        &f.engine.store,
        result["batchId"].as_str().unwrap(),
        &deadline,
    )
    .expect("first late report must not rewrite or invalidate original unknown receipt");
    assert_eq!(described["knownReportedUsage"]["inputTokens"], 11);
    assert_eq!(described["unknownUsageAttempts"], 1);
}
