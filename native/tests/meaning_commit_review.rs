//! Deterministic host cancellation at an actual committed publication, without production hooks.
use serde_json::Value;
use std::{
    fs,
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};
use yoke::{
    Engine,
    capture::Deadline,
    meaning_build::{self, MeaningPolicy},
    model::hash,
    providers::ProviderSpec,
};

#[test]
fn a_failed_status_after_commit_reports_both_durable_identities_without_claiming_incomplete_work() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().join("workspace");
    fs::create_dir(&root).unwrap();
    fs::write(root.join("a.rs"), "pub fn original() {}\n").unwrap();
    let mut engine = Engine::open(&root).unwrap();
    let (snapshot, graph, _, _) = engine.index(&Deadline::new(30_000)).unwrap();
    let script = directory.path().join("owned-worker.cjs");
    fs::write(&script, r#"
let input='';process.stdin.on('data',part=>input+=part);process.stdin.on('end',()=>{
 const r=JSON.parse(input);
 const data={concepts:[],relations:[],hyperedges:[],coverage:r.payload.sources.map(s=>({sourceId:s.anchor.sourceId,outcome:'no_claims'}))};
 process.stdout.write(JSON.stringify({schema:1,operation:r.operation,model:r.model,modelVersion:r.modelVersion,requestId:r.requestId,inputHash:r.inputHash,sourceBindings:r.payload.sources.map(s=>s.anchor),currency:r.currency,actualCostMicros:1,usage:{inputTokens:2,outputTokens:1,cachedInputTokens:0},data}));
});
"#).unwrap();
    let provider = ProviderSpec {
        trusted: true,
        command: PathBuf::from("node"),
        args: vec![script.to_str().unwrap().into()],
        model_id: "owned-commit-review".into(),
        model_version: "v1".into(),
        task_id: "meaning-commit-review".into(),
        currency: "EUR".into(),
        max_cost_micros: 2,
        task_limit_micros: 10,
    };
    let policy = MeaningPolicy::files(&snapshot, hash("owned committed-status review"));
    engine.store.db.execute_batch("CREATE TABLE review_meaning_completion_tick(value INTEGER); CREATE TRIGGER review_arm_completion AFTER INSERT ON records WHEN NEW.kind='native_meaning_graph_v1' BEGIN INSERT INTO review_meaning_completion_tick VALUES(1); END;").unwrap();
    let armed = Arc::new(AtomicBool::new(false));
    let completed = Arc::new(AtomicBool::new(false));
    let cancellation = Arc::new(AtomicBool::new(false));
    let arm_on_insert = Arc::clone(&armed);
    engine
        .store
        .db
        .update_hook(Some(move |_, _: &str, table: &str, _| {
            if table == "review_meaning_completion_tick" {
                arm_on_insert.store(true, Ordering::SeqCst);
            }
        }));
    let cancel_at_commit = Arc::clone(&cancellation);
    let observed_commit = Arc::clone(&completed);
    engine.store.db.commit_hook(Some(move || {
        if armed.swap(false, Ordering::SeqCst) {
            observed_commit.store(true, Ordering::SeqCst);
            cancel_at_commit.store(true, Ordering::SeqCst);
        }
        false // The publication must commit; this hook never rolls it back.
    }));
    let mut returned = None;
    let outer = Deadline::run_bounded(&Deadline::new(30_000), cancellation, || {
        let scoped = Deadline::new(30_000);
        returned = Some(meaning_build::build(
            &engine.store,
            &snapshot,
            &graph,
            &provider,
            &policy,
            &scoped,
        ));
        Ok(())
    });
    assert!(
        outer.is_err(),
        "The injected host cancellation must be observed"
    );
    assert!(
        completed.load(Ordering::SeqCst),
        "Cancellation must occur at the graph publication commit, not during worker admission"
    );
    let error = returned
        .expect("Builder must return to its caller")
        .unwrap_err();
    // Fresh inspection is outside the canceled host scope; no provider is rerun.
    engine.store.set_deadline(&Deadline::new(30_000)).unwrap();
    let jobs: Vec<Value> = engine.store.records(meaning_build::JOB_KIND).unwrap();
    let graphs: Vec<Value> = engine.store.records(meaning_build::GRAPH_KIND).unwrap();
    assert_eq!(jobs.len(), 1);
    assert_eq!(graphs.len(), 1);
    assert_eq!(jobs[0]["state"], "complete");
    assert_eq!(jobs[0]["graphId"], graphs[0]["graphId"]);
    let job_id = jobs[0]["jobId"].as_str().unwrap();
    let graph_id = graphs[0]["graphId"].as_str().unwrap();
    let loaded = meaning_build::load(&engine.store, graph_id, &Deadline::new(30_000)).unwrap();
    assert_eq!(loaded["jobId"], job_id);
    assert_eq!(loaded["snapshotId"], snapshot.id);
    assert_eq!(
        loaded["packets"][0]["meaning"]["coverage"][0]["outcome"],
        "no_claims"
    );
    assert_eq!(
        fs::read_to_string(root.join("a.rs")).unwrap(),
        "pub fn original() {}\n"
    );
    let message = format!("{error:#}");
    assert!(
        message.contains(job_id),
        "Durable job identity was lost: {message}"
    );
    assert!(
        message.contains(graph_id),
        "Durable graph identity was lost: {message}"
    );
    assert!(
        !message.to_lowercase().contains("incomplete"),
        "A committed publication was incorrectly reported as incomplete: {message}"
    );
    assert!(
        message.to_lowercase().contains("durable") || message.to_lowercase().contains("committed"),
        "The caller needs explicit publication outcome: {message}"
    );
    assert_eq!(
        engine
            .store
            .records::<Value>("native_meaning_cache_v1")
            .unwrap()
            .len(),
        1
    );
}
