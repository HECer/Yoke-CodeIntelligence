//! Independent trusted subprocess fixtures; no real model or network call.
use serde_json::{Value, json};
use std::{fs, path::PathBuf};
use yoke::{
    Engine,
    capture::Deadline,
    ledger::{AttemptState, Ledger},
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
fn fixture(mode: &str, cohere: bool) -> Fixture {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("project");
    fs::create_dir(&root).unwrap();
    fs::write(root.join("main.rs"), "fn alpha(){}\n").unwrap();
    fs::write(root.join("second.rs"), "fn beta(){}\n").unwrap();
    let script = dir.path().join("review.cjs");
    let marker = dir.path().join("calls");
    fs::write(&script,r#"let input='';process.stdin.on('data',b=>input+=b);process.stdin.on('end',()=>{const r=JSON.parse(input),fs=require('fs'),mode=process.argv[2],marker=process.argv[3];fs.appendFileSync(marker,r.requestId+'\n');const n=fs.readFileSync(marker,'utf8').trim().split('\n').length;if(mode==='sqlite-free'){const {DatabaseSync}=require('node:sqlite');const db=new DatabaseSync(process.argv[4]);db.exec('BEGIN IMMEDIATE;ROLLBACK;');db.close();}let items=r.payload.sources.map((s,i)=>({sourceId:s.anchor.sourceId,vector:[i+1,...Array(255).fill(0)]}));if(mode==='reverse')items.reverse();let response={schema:1,operation:r.operation,model:r.model,modelVersion:r.modelVersion,requestId:r.requestId,inputHash:r.inputHash,sourceBindings:r.payload.sources.map(s=>s.anchor),currency:r.currency,actualCostMicros:7,usage:{inputTokens:4,outputTokens:1,cachedInputTokens:0},data:{items}};if(mode==='wrong-binding')response.sourceBindings[0].contentHash='0'.repeat(64);if(mode==='second-fails'&&n===2){delete response.data;response.error='owned failure';}if(mode==='nonzero')process.exitCode=7;process.stdout.write(JSON.stringify(response));});"#).unwrap();
    let mut engine = Engine::open(&root).unwrap();
    let (snapshot, graph, _, _) = engine.index(&Deadline::new(10000)).unwrap();
    let provider = ProviderSpec {
        trusted: true,
        command: PathBuf::from("node"),
        args: vec![
            script.to_str().unwrap().into(),
            mode.into(),
            marker.to_str().unwrap().into(),
            engine
                .store
                .dir
                .join("index.sqlite")
                .to_str()
                .unwrap()
                .into(),
        ],
        model_id: "review".into(),
        model_version: "v1".into(),
        task_id: "review-task".into(),
        currency: "EUR".into(),
        max_cost_micros: 10,
        task_limit_micros: 100,
    };
    let spec = BedrockSpec {
        trusted_network: true,
        endpoint: "http://127.0.0.1:1/".into(),
        region: "eu-central-1".into(),
        remote_model_id: "review".into(),
        model: "review".into(),
        model_version: "v1".into(),
        credential_name: "unused".into(),
        dialect: if cohere {
            Dialect::CohereV4 { dimensions: 256 }
        } else {
            Dialect::TitanV2 {
                dimensions: 256,
                normalize: true,
            }
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
fn run(f: &Fixture) -> anyhow::Result<Value> {
    provider_batches::generate_embeddings_batched(
        &f.engine.store,
        &f.snapshot,
        &f.graph,
        &f.provider,
        &EmbeddingBatchPolicy::for_bedrock(&f.spec, 10, 10, 100000)?,
        &Deadline::new(10000),
    )
}
fn calls(f: &Fixture) -> usize {
    fs::read_to_string(&f.marker)
        .map(|s| s.lines().count())
        .unwrap_or(0)
}
fn batch(f: &Fixture) -> Value {
    f.engine
        .store
        .records("native_model_batch_v1")
        .unwrap()
        .pop()
        .unwrap()
}

#[test]
fn legal_reversed_cohere_items_are_reassembled_by_id_before_publication() {
    let f = fixture("reverse", true);
    let result = run(&f).expect("the native protocol binds vector IDs, not response array order");
    assert_eq!(calls(&f), 1);
    assert_eq!(result["count"], 2);
    assert_eq!(result["status"], "complete");
}
#[test]
fn model_child_can_acquire_sqlite_write_transaction_while_parent_holds_only_client_lease() {
    let f = fixture("sqlite-free", false);
    let result = run(&f).expect("no SQLite transaction may span a child/model call");
    assert_eq!(result["status"], "complete");
    assert_eq!(calls(&f), 2);
}
#[test]
fn unbound_reported_cost_remains_unknown_and_only_never_sent_attempt_is_released() {
    let f = fixture("wrong-binding", false);
    assert!(run(&f).is_err());
    assert_eq!(calls(&f), 1);
    let d = Deadline::new(5000);
    let ledger = Ledger::new(&f.engine.store, &d);
    let totals = ledger.total("review-task").unwrap();
    assert_eq!(totals.actual_micros, 0);
    assert_eq!(totals.encumbered_micros, 10);
    assert_eq!(totals.unknown, 1);
    let b = batch(&f);
    let statuses = b["chunks"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| {
            ledger
                .attempt_status(c["attemptId"].as_str().unwrap())
                .unwrap()
                .state
        })
        .collect::<Vec<_>>();
    assert_eq!(statuses, vec![AttemptState::Unknown, AttemptState::NotSent]);
    let result = provider_batches::describe_embedding_batch(
        &f.engine.store,
        b["batchId"].as_str().unwrap(),
        &d,
    )
    .unwrap();
    assert!(result["modelCalls"].is_null());
    assert_eq!(result["confirmedResponseCount"], 0);
}
#[test]
fn complete_bound_reply_with_nonzero_exit_still_preserves_cost_and_usage() {
    let f = fixture("nonzero", false);
    assert!(run(&f).is_err());
    assert_eq!(calls(&f), 1);
    let d = Deadline::new(5000);
    let totals = Ledger::new(&f.engine.store, &d)
        .total("review-task")
        .unwrap();
    assert_eq!(totals.actual_micros, 7);
    assert_eq!(totals.encumbered_micros, 7);
    let b = batch(&f);
    let described = provider_batches::describe_embedding_batch(
        &f.engine.store,
        b["batchId"].as_str().unwrap(),
        &d,
    )
    .unwrap();
    // Failed batches conservatively avoid claiming an exact external model-call
    // count. The bound response and its known usage remain separately visible.
    assert!(described["modelCalls"].is_null());
    assert_eq!(described["dispatchedAttempts"], 1);
    assert_eq!(described["confirmedResponseCount"], 1);
    assert_eq!(described["knownReportedUsage"]["inputTokens"], 4);
    assert_eq!(b["state"], "failed");
}
#[test]
fn failed_new_batch_leaves_existing_other_model_corpus_byte_identical() {
    let f = fixture("second-fails", false);
    let entity = f.graph.entities.iter().find(|e| e.kind != "file").unwrap();
    let file = f
        .snapshot
        .files
        .iter()
        .find(|file| file.path == entity.path)
        .unwrap();
    yoke::knowledge::import_vectors(&f.engine.store,&f.snapshot,&f.graph,json!({"snapshotId":f.snapshot.id,"model":"previous-model","items":[{"entityId":entity.id,"contentHash":file.hash,"vector":[3.0,4.0]}]}),&Deadline::new(5000)).unwrap();
    let id = identity(&(f.snapshot.id.as_str(), "previous-model"));
    let before: String = f
        .engine
        .store
        .db
        .query_row(
            "SELECT value FROM records WHERE kind='native_vectors_v1' AND id=?1",
            [&id],
            |r| r.get(0),
        )
        .unwrap();
    assert!(run(&f).is_err());
    let after: String = f
        .engine
        .store
        .db
        .query_row(
            "SELECT value FROM records WHERE kind='native_vectors_v1' AND id=?1",
            [&id],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(after, before);
    assert_eq!(calls(&f), 2);
}
#[test]
fn retained_batch_rejects_unknown_nested_fields_and_rebound_request_identity() {
    for mode in ["unknown", "request"] {
        let f = fixture("ok", false);
        run(&f).unwrap();
        let mut b = batch(&f);
        if mode == "unknown" {
            b["chunks"][0]["extraAuthority"] = json!(true);
        } else {
            b["chunks"][0]["requestId"] = json!("0".repeat(64));
        }
        f.engine
            .store
            .db
            .execute(
                "UPDATE records SET value=?2 WHERE kind='native_model_batch_v1' AND id=?1",
                rusqlite::params![
                    b["batchId"].as_str().unwrap(),
                    serde_json::to_string(&b).unwrap()
                ],
            )
            .unwrap();
        assert!(
            yoke::retention::plan(&f.engine.store, &Default::default(), &Deadline::new(10000))
                .is_err(),
            "retention accepted {mode}"
        );
    }
}

#[test]
fn every_source_is_verified_before_first_chunk_reservation_or_launch() {
    let f = fixture("ok", false);
    let file = f
        .snapshot
        .files
        .iter()
        .find(|file| file.path == "second.rs")
        .unwrap();
    fs::OpenOptions::new()
        .write(true)
        .open(f.engine.store.dir.join("blobs").join(&file.hash))
        .unwrap()
        .set_len(16000000)
        .unwrap();
    assert!(run(&f).is_err());
    assert_eq!(calls(&f), 0);
    assert_eq!(
        Ledger::new(&f.engine.store, &Deadline::new(5000))
            .total("review-task")
            .unwrap()
            .attempts,
        0
    );
    assert!(
        f.engine
            .store
            .records::<Value>("native_model_batch_v1")
            .unwrap()
            .is_empty()
    );
}

#[test]
fn full_vector_store_capacity_is_rejected_before_any_reservation_or_child() {
    let f = fixture("ok", false);
    for i in 0..512 {
        f.engine
            .store
            .db
            .execute(
                "INSERT INTO records(kind,id,value) VALUES('native_vectors_v1',?1,'{}')",
                [identity(&format!("existing-{i}"))],
            )
            .unwrap();
    }
    assert!(run(&f).is_err());
    assert_eq!(calls(&f), 0);
    assert_eq!(
        Ledger::new(&f.engine.store, &Deadline::new(5000))
            .total("review-task")
            .unwrap()
            .attempts,
        0
    );
    assert!(
        f.engine
            .store
            .records::<Value>("native_model_batch_v1")
            .unwrap()
            .is_empty()
    );
}

#[test]
fn worst_finite_float_serialization_rejects_entire_batch_before_billing() {
    let mut f = fixture("ok", true);
    fs::remove_file(f.engine.root.join("second.rs")).unwrap();
    let source = (0..512)
        .map(|i| format!("fn entity{i}(){{}}\n"))
        .collect::<String>();
    fs::write(f.engine.root.join("main.rs"), source).unwrap();
    let (snapshot, graph, _, _) = f.engine.index(&Deadline::new(20000)).unwrap();
    f.snapshot = snapshot;
    f.graph = graph;
    f.spec.dialect = Dialect::CohereV4 { dimensions: 1536 };
    let policy = EmbeddingBatchPolicy::for_bedrock(&f.spec, 10, 1024, 100000).unwrap();
    let error = provider_batches::generate_embeddings_batched(
        &f.engine.store,
        &f.snapshot,
        &f.graph,
        &f.provider,
        &policy,
        &Deadline::new(10000),
    )
    .unwrap_err();
    assert!(error.to_string().contains("Worst-case"), "{error:#}");
    assert_eq!(calls(&f), 0);
    assert_eq!(
        Ledger::new(&f.engine.store, &Deadline::new(5000))
            .total("review-task")
            .unwrap()
            .attempts,
        0
    );
}

#[test]
fn each_chunk_keeps_native_protocol_one_and_legacy_attempt_bound_request_identity() {
    let f = fixture("ok", false);
    let script_path = &f.provider.args[0];
    let script = fs::read_to_string(script_path).unwrap();
    let capture = script.replace(
        "fs.appendFileSync(marker,r.requestId+'\\n')",
        "fs.appendFileSync(marker,JSON.stringify(r)+'\\n')",
    );
    assert_ne!(capture, script);
    fs::write(script_path, capture).unwrap();
    let result = run(&f).unwrap();
    let record = batch(&f);
    let raw = fs::read_to_string(&f.marker).unwrap();
    let requests = raw
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(requests.len(), 2);
    for (request, chunk) in requests.iter().zip(record["chunks"].as_array().unwrap()) {
        assert_eq!(request.as_object().unwrap().len(), 10);
        assert_eq!(request["schema"], 1);
        assert_eq!(request["operation"], "embed");
        assert_eq!(request["model"], "review");
        assert_eq!(request["modelVersion"], "v1");
        assert_eq!(request["inputHash"], identity(&request["payload"]));
        let expected = identity(&(
            chunk["attemptId"].as_str().unwrap(),
            "embed",
            "review",
            "v1",
            request["inputHash"].as_str().unwrap(),
        ));
        assert_eq!(request["requestId"], expected);
        assert_eq!(chunk["requestId"], expected);
        assert_eq!(request["costLimitMicros"], 10);
    }
    assert_eq!(
        result["vectorCollectionId"],
        identity(&(f.snapshot.id.as_str(), "review@v1"))
    );
}
