//! Owned process fixtures only; no AWS endpoint, key store or real billing.
use serde_json::{Value, json};
use std::{fs, path::PathBuf, process::Command};
use yoke::{
    Engine,
    capture::Deadline,
    model_bedrock::{BedrockSpec, Dialect},
    provider_batches::{self, EmbeddingBatchPolicy},
    providers::ProviderSpec,
};

fn profile() -> Value {
    json!({"endpoint":"https://bedrock-runtime.us-east-1.amazonaws.com/","region":"us-east-1","remoteModelId":"amazon.titan-embed-text-v2:0","modelId":"fixture","modelVersion":"1","credentialName":"unused-owned-review","dialect":{"kind":"titanV2","dimensions":256,"normalize":true},"taskId":"batch-review","currency":"USD","maxCostMicros":10,"taskLimitMicros":100,"embeddingBatch":{"maxCalls":2,"maxEntities":2,"maxTotalSourceBytes":10000}})
}

#[test]
fn invalid_batch_output_budget_rejects_before_creating_source_state() {
    let inputs = tempfile::tempdir().unwrap();
    let root = inputs.path().join("absent-source");
    fs::create_dir(&root).unwrap();
    let config = inputs.path().join("host.json");
    fs::write(&config, profile().to_string()).unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_yoke"))
        .arg("--root")
        .arg(&root)
        .args([
            "--max-bytes",
            "1023",
            "models-bedrock",
            "--trust-network",
            "--config",
        ])
        .arg(config)
        .arg("embeddings")
        .output()
        .unwrap();
    assert!(!out.status.success());
    let error = String::from_utf8_lossy(&out.stderr);
    assert!(error.contains("output budget"), "{error}");
    assert!(
        !root.join(".yoke").exists(),
        "A pure invalid transport budget must not manufacture source state"
    );
}

#[test]
fn batch_status_does_not_initialize_an_empty_database_placeholder() {
    for unrelated in [false, true] {
        let root = tempfile::tempdir().unwrap();
        fs::create_dir(root.path().join(".yoke")).unwrap();
        let db = root.path().join(".yoke/index.sqlite");
        fs::write(&db, []).unwrap();
        if unrelated {
            let connection = rusqlite::Connection::open(&db).unwrap();
            connection
                .execute("CREATE TABLE unrelated(value TEXT)", [])
                .unwrap();
            connection
                .execute("INSERT INTO unrelated VALUES('preserve me')", [])
                .unwrap();
        }
        let original = fs::read(&db).unwrap();
        let out = Command::new(env!("CARGO_BIN_EXE_yoke"))
            .arg("--root")
            .arg(root.path())
            .args(["model-batch-status", &"a".repeat(64)])
            .output()
            .unwrap();
        assert!(!out.status.success());
        assert!(
            fs::read(&db).unwrap() == original,
            "Status modified an unrelated database (nonempty={unrelated})"
        );
        assert!(!root.path().join(".yoke/blobs").exists());
    }
}

#[test]
fn batch_status_unknown_root_or_invalid_identity_does_not_create_state() {
    let inputs = tempfile::tempdir().unwrap();
    let root = inputs.path().join("absent");
    for id in ["a".repeat(64), "invalid".into()] {
        let out = Command::new(env!("CARGO_BIN_EXE_yoke"))
            .arg("--root")
            .arg(&root)
            .args(["model-batch-status", &id])
            .output()
            .unwrap();
        assert!(!out.status.success());
        assert!(!root.exists());
    }
}

fn complete_batch() -> (tempfile::TempDir, Engine, Value, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("workspace");
    fs::create_dir(&root).unwrap();
    fs::write(root.join("source.rs"), "fn first() {}\nfn second() {}\n").unwrap();
    let script = dir.path().join("provider.cjs");
    let marker = dir.path().join("calls");
    fs::write(&script,r#"let b='';process.stdin.on('data',x=>b+=x);process.stdin.on('end',()=>{let r=JSON.parse(b);require('fs').appendFileSync(process.argv[2],'call\n');process.stdout.write(JSON.stringify({schema:1,operation:r.operation,model:r.model,modelVersion:r.modelVersion,requestId:r.requestId,inputHash:r.inputHash,sourceBindings:r.payload.sources.map(s=>s.anchor),currency:r.currency,actualCostMicros:7,usage:{inputTokens:10,outputTokens:0,cachedInputTokens:0},data:{items:r.payload.sources.map(s=>({sourceId:s.anchor.sourceId,vector:[1,...Array(255).fill(0)]}))}}));});"#).unwrap();
    let mut engine = Engine::open(&root).unwrap();
    let d = Deadline::new(20000);
    let (snapshot, graph, _, _) = engine.index(&d).unwrap();
    let provider = ProviderSpec {
        trusted: true,
        command: "node".into(),
        args: vec![
            script.to_str().unwrap().into(),
            marker.to_str().unwrap().into(),
        ],
        model_id: "review".into(),
        model_version: "v1".into(),
        task_id: "review-batch".into(),
        currency: "USD".into(),
        max_cost_micros: 10,
        task_limit_micros: 100,
    };
    let spec = BedrockSpec {
        trusted_network: true,
        endpoint: "http://127.0.0.1:1/".into(),
        region: "us-east-1".into(),
        remote_model_id: "fixture".into(),
        model: "review".into(),
        model_version: "v1".into(),
        credential_name: "unused".into(),
        dialect: Dialect::TitanV2 {
            dimensions: 256,
            normalize: true,
        },
        allow_loopback_http: true,
        additional_root_certificates: vec![],
    };
    let policy = EmbeddingBatchPolicy::for_bedrock(&spec, 2, 2, 10000).unwrap();
    let result = provider_batches::generate_embeddings_batched(
        &engine.store,
        &snapshot,
        &graph,
        &provider,
        &policy,
        &d,
    )
    .unwrap();
    (dir, engine, result, marker)
}

#[test]
fn small_cli_status_is_a_recoverable_typed_reference_without_new_artifacts_or_execution() {
    let (_dir, engine, result, marker) = complete_batch();
    let root = engine.root.clone();
    let id = result["batchId"].as_str().unwrap().to_owned();
    let before: usize = engine
        .store
        .db
        .query_row(
            "SELECT count(*) FROM records WHERE kind='artifact'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    let snapshots: usize = engine
        .store
        .db
        .query_row("SELECT count(*) FROM snapshots", [], |r| r.get(0))
        .unwrap();
    drop(engine);
    fs::remove_file(root.join("source.rs")).unwrap();
    let run = |limit: &str| {
        Command::new(env!("CARGO_BIN_EXE_yoke"))
            .arg("--root")
            .arg(&root)
            .args(["--max-bytes", limit, "model-batch-status", &id])
            .output()
            .unwrap()
    };
    let short = run("1024");
    assert!(
        short.status.success(),
        "{}",
        String::from_utf8_lossy(&short.stderr)
    );
    assert!(short.stdout.len() <= 1024);
    let reference: Value = serde_json::from_slice(&short.stdout).unwrap();
    assert_eq!(reference["kind"], "model_embedding_batch_reference");
    assert_eq!(reference["batchId"], id);
    assert_eq!(reference["readWith"]["command"], "model-batch-status");
    assert!(reference.get("artifactId").is_none());
    let full = run("4000000");
    assert!(
        full.status.success(),
        "{}",
        String::from_utf8_lossy(&full.stderr)
    );
    let recovered: Value = serde_json::from_slice(&full.stdout).unwrap();
    assert_eq!(recovered, result);
    assert_eq!(fs::read_to_string(marker).unwrap().lines().count(), 2);
    let engine = Engine::open(&root).unwrap();
    assert_eq!(
        engine
            .store
            .db
            .query_row::<usize, _, _>(
                "SELECT count(*) FROM records WHERE kind='artifact'",
                [],
                |r| r.get(0)
            )
            .unwrap(),
        before
    );
    assert_eq!(
        engine
            .store
            .db
            .query_row::<usize, _, _>("SELECT count(*) FROM snapshots", [], |r| r.get(0))
            .unwrap(),
        snapshots
    );
    assert!(!root.join("source.rs").exists());
}

#[test]
fn valid_batch_retention_is_typed_and_future_fields_or_response_rebinding_fail_closed() {
    for mode in 0..3 {
        let (_dir, engine, result, _marker) = complete_batch();
        let id = result["batchId"].as_str().unwrap();
        let d = Deadline::new(20000);
        let mut record: Value = engine
            .store
            .get_record("native_model_batch_v1", id)
            .unwrap();
        let baseline = yoke::retention::plan(&engine.store, &Default::default(), &d).unwrap();
        assert_eq!(baseline["unknownDependencyKinds"], json!([]), "{baseline}");
        match mode {
            0 => record["policy"]["unknownFutureAuthority"] = true.into(),
            1 => record["chunks"][0]["responseArtifactId"] = "0".repeat(64).into(),
            _ => record["chunks"][0]["inputHash"] = "0".repeat(64).into(),
        };
        engine
            .store
            .db
            .execute(
                "UPDATE records SET value=?1 WHERE kind='native_model_batch_v1' AND id=?2",
                rusqlite::params![record.to_string(), id],
            )
            .unwrap();
        assert!(
            yoke::retention::plan(&engine.store, &Default::default(), &Deadline::new(20000))
                .is_err(),
            "tampering {mode}"
        );
    }
}

#[test]
fn host_batch_authority_fields_and_aggregate_overbudget_reject_without_workers() {
    for variant in 0..4 {
        let root = tempfile::tempdir().unwrap();
        let config = root.path().join("host.json");
        let mut p = profile();
        match variant {
            0 => p["embeddingBatch"]["trusted"] = true.into(),
            1 => p["embeddingBatch"]["allowWrites"] = true.into(),
            2 => p["trustedNetwork"] = true.into(),
            _ => p["taskLimitMicros"] = 5.into(),
        };
        fs::write(&config, p.to_string()).unwrap();
        let source = root.path().join("source");
        if variant == 3 {
            fs::create_dir(&source).unwrap();
            fs::write(source.join("file.rs"), "fn a(){}\nfn b(){}\n").unwrap();
        }
        let out = Command::new(env!("CARGO_BIN_EXE_yoke"))
            .arg("--root")
            .arg(&source)
            .args(["models-bedrock", "--trust-network", "--config"])
            .arg(config)
            .arg("embeddings")
            .output()
            .unwrap();
        assert!(!out.status.success());
        if variant < 3 {
            assert!(!source.exists());
        } else {
            let engine = Engine::open(&source).unwrap();
            assert_eq!(
                yoke::ledger::Ledger::new(&engine.store, &Deadline::new(5000))
                    .total("batch-review")
                    .unwrap()
                    .attempts,
                0
            );
        }
    }
}
