//! Actual host CLI with owned local worker fixtures; no external model calls.
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Output, Stdio},
};
use yoke::{Engine, capture::Deadline, model::hash};

struct Fixture {
    _dir: tempfile::TempDir,
    root: PathBuf,
    config: PathBuf,
    policy: PathBuf,
    worker: PathBuf,
    marker: PathBuf,
}
fn fixture(mode: &str) -> Fixture {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("source");
    fs::create_dir(&root).unwrap();
    fs::write(root.join("a.rs"), "pub fn alpha() -> u32 { 1 }\n").unwrap();
    fs::write(root.join("b.rs"), "pub fn beta() -> u32 { 2 }\n").unwrap();
    let worker = dir.path().join("worker.cjs");
    let marker = dir.path().join("calls.txt");
    fs::write(&worker, r#"
let raw='';process.stdin.on('data',b=>raw+=b);process.stdin.on('end',()=>{
 const r=JSON.parse(raw),fs=require('fs'),mode=process.argv[2],marker=process.argv[3];
 fs.appendFileSync(marker,r.requestId+'\n');const n=fs.readFileSync(marker,'utf8').trim().split('\n').length;
 const support=s=>[{sourceId:s.anchor.sourceId,startByte:s.anchor.startByte,endByte:s.anchor.endByte}];
 const data={concepts:r.payload.sources.map((s,i)=>({key:'c'+i,label:'Storage policy',description:'Owned fixture claim',aliases:['Immutable bytes'],support:support(s)})),relations:[],hyperedges:[],coverage:r.payload.sources.map(s=>({sourceId:s.anchor.sourceId,outcome:'claims'}))};
 if(mode==='second-invalid'&&n===2)data.concepts[0].support[0].startByte=999999;
 process.stdout.write(JSON.stringify({schema:1,operation:r.operation,model:r.model,modelVersion:r.modelVersion,requestId:r.requestId,inputHash:r.inputHash,sourceBindings:r.payload.sources.map(s=>s.anchor),currency:r.currency,actualCostMicros:7,usage:{inputTokens:10,outputTokens:3,cachedInputTokens:2},data}));
});
"#).unwrap();
    let config = dir.path().join("provider.json");
    write_json(
        &config,
        &json!({"command":"node","args":[worker,mode,marker],"modelId":"owned-cli-fixture","modelVersion":"v1","taskId":"meaning-cli-fixture","currency":"EUR","maxCostMicros":10,"taskLimitMicros":100}),
    );
    let policy = dir.path().join("policy.json");
    write_json(
        &policy,
        &json!({"contractHash":hash("owned host extraction contract"),"generation":"fixture-v1","selection":{"files":["a.rs","b.rs"]},"contextGroups":[],"providerInputFiles":[],"maxPackets":64,"maxTotalSourceBytes":4000000,"maxGraphBytes":64000000}),
    );
    Fixture {
        _dir: dir,
        root,
        config,
        policy,
        worker,
        marker,
    }
}
fn write_json(path: &Path, value: &Value) {
    fs::write(path, serde_json::to_vec(value).unwrap()).unwrap();
}
fn command(f: &Fixture, operation: &str) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_yoke"));
    command
        .arg("--root")
        .arg(&f.root)
        .arg("--timeout")
        .arg("30000")
        .arg("meaning")
        .arg(operation);
    command
}
fn build(f: &Fixture, extra: &[&str]) -> Output {
    command(f, "build")
        .arg("--config")
        .arg(&f.config)
        .arg("--policy")
        .arg(&f.policy)
        .arg("--trust-provider")
        .args(extra)
        .output()
        .unwrap()
}
fn success(output: Output) -> Value {
    assert!(
        output.status.success(),
        "stderr: {}\nstdout: {}",
        String::from_utf8_lossy(&output.stderr),
        String::from_utf8_lossy(&output.stdout)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}
fn calls(f: &Fixture) -> usize {
    fs::read_to_string(&f.marker)
        .map(|s| s.lines().count())
        .unwrap_or(0)
}
fn records(f: &Fixture, kind: &str) -> Vec<Value> {
    Engine::open(&f.root).unwrap().store.records(kind).unwrap()
}

#[test]
fn meaning_cli_builds_queries_and_reuses_immutable_receipts_without_read_time_execution() {
    let f = fixture("ok");
    let first = success(build(&f, &[]));
    assert_eq!(first["status"], "ready");
    assert_eq!(first["modelCallsThisRun"], 2);
    assert_eq!(calls(&f), 2);
    let again = success(build(&f, &[]));
    assert_eq!(again["modelCallsThisRun"], 0);
    assert_eq!(again["cacheHits"], 2);
    fs::rename(&f.worker, f.worker.with_extension("disabled")).unwrap();
    let id = again["graphId"].as_str().unwrap();
    let loaded = success(command(&f, "load").arg(id).output().unwrap());
    assert_eq!(loaded["resolution"], "inferred_concept");
    assert_eq!(loaded["providerAttestation"], "unverified");
    assert_eq!(loaded["packets"].as_array().unwrap().len(), 2);
    let found = success(
        command(&f, "query")
            .args([id, "Immutable bytes"])
            .output()
            .unwrap(),
    );
    assert_eq!(found["packets"].as_array().unwrap().len(), 2);
    let job = again["jobId"].as_str().unwrap();
    let current = success(command(&f, "status").arg(job).output().unwrap());
    assert_eq!(current["status"], "ready");
    fs::write(f.root.join("a.rs"), "pub fn alpha() -> u32 { 9 }\n").unwrap();
    let (snapshot, _, _, _) = Engine::open(&f.root)
        .unwrap()
        .index(&Deadline::new(10000))
        .unwrap();
    let stale = success(
        command(&f, "status")
            .arg(job)
            .arg("--compare-snapshot")
            .arg(&snapshot.id)
            .output()
            .unwrap(),
    );
    assert_eq!(stale["freshness"], "changed");
    assert_eq!(stale["changedPaths"], json!(["a.rs"]));
    assert_eq!(calls(&f), 2);
}

#[test]
fn meaning_cli_requires_explicit_provider_permission_before_opening_source_state() {
    let f = fixture("ok");
    let output = command(&f, "build")
        .arg("--config")
        .arg("missing.json")
        .arg("--policy")
        .arg("also-missing.json")
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("--trust-provider"));
    assert!(!f.root.join(".yoke").exists());
    assert_eq!(calls(&f), 0);
}

#[test]
fn meaning_cli_rejects_ambiguous_host_json_and_invalid_output_budget_before_state() {
    for mode in 0..4 {
        let f = fixture("ok");
        match mode {
            0 => fs::write(&f.config, r#"{"command":"node","command":"other"}"#).unwrap(),
            1 => fs::write(&f.policy, r#"{"generation":"one","generation":"two"}"#).unwrap(),
            2 => fs::write(&f.policy, "x".repeat(128001)).unwrap(),
            _ => {}
        }
        let output = build(
            &f,
            if mode == 3 {
                &["--max-bytes", "128"]
            } else {
                &[]
            },
        );
        assert!(!output.status.success());
        let error = String::from_utf8_lossy(&output.stderr);
        assert!(
            if mode < 2 {
                error.contains("Duplicate JSON key")
            } else if mode == 2 {
                error.contains("capacity")
            } else {
                error.contains("output budget")
            },
            "{mode}: {error}"
        );
        assert!(!f.root.join(".yoke").exists(), "mode {mode}");
        assert_eq!(calls(&f), 0);
    }
}

#[test]
fn meaning_cli_read_commands_require_existing_store_and_reject_execution_configuration() {
    for operation in ["load", "query", "status", "aliases"] {
        let f = fixture("ok");
        let mut request = command(&f, operation);
        request.arg("a".repeat(64));
        if operation == "query" {
            request.arg("Storage");
        }
        let output = request.output().unwrap();
        assert!(!output.status.success());
        assert!(String::from_utf8_lossy(&output.stderr).contains("existing"));
        assert!(!f.root.join(".yoke").exists());
        let mut request = command(&f, operation);
        request.arg("a".repeat(64));
        if operation == "query" {
            request.arg("Storage");
        }
        let output = request
            .args([
                "--language-server-config",
                "missing.json",
                "--trust-servers",
            ])
            .output()
            .unwrap();
        assert!(!output.status.success());
        assert!(String::from_utf8_lossy(&output.stderr).contains("language server"));
        assert!(!f.root.join(".yoke").exists());
        assert_eq!(calls(&f), 0);
    }
}

#[test]
fn meaning_alias_cli_replays_exact_graph_without_provider_source_capture_or_writes() {
    let f = fixture("ok");
    let built = success(build(&f, &[]));
    let id = built["graphId"].as_str().unwrap();
    fs::rename(&f.worker, f.worker.with_extension("disabled")).unwrap();
    fs::remove_file(f.root.join("a.rs")).unwrap();
    fs::remove_file(f.root.join("b.rs")).unwrap();
    let inventory = || {
        let engine = Engine::open(&f.root).unwrap();
        [
            "SELECT count(*) FROM records",
            "SELECT count(*) FROM records WHERE kind='artifact'",
            "SELECT count(*) FROM snapshots",
        ]
        .map(|sql| {
            engine
                .store
                .db
                .query_row(sql, [], |r| r.get::<_, u64>(0))
                .unwrap()
        })
    };
    let before = inventory();
    let output = command(&f, "aliases")
        .args([id, "--max-candidates", "1"])
        .output()
        .unwrap();
    let r = success(output);
    assert_eq!(r["kind"], "meaning_alias_candidates_v1");
    assert_eq!(r["graphId"], id);
    assert_eq!(r["bindingValidation"], "exact_captured_source_replayed");
    assert_eq!(r["candidates"].as_array().unwrap().len(), 1);
    assert_eq!(r["candidates"][0]["scoreBps"], 10000);
    assert_eq!(r["semanticIdentityEstablished"], false);
    assert_eq!(r["concepts"].as_array().unwrap().len(), 2);
    assert_eq!(r["evidence"].as_array().unwrap().len(), 2);
    for (option, value) in [
        ("--max-pairs", "0"),
        ("--max-candidates", "0"),
        ("--max-similarity-work", "0"),
        ("--max-bytes", "1024"),
    ] {
        let output = command(&f, "aliases")
            .args([id, option, value])
            .output()
            .unwrap();
        assert!(!output.status.success(), "{option}");
        assert!(output.stdout.is_empty());
    }
    assert_eq!(inventory(), before);
    assert_eq!(calls(&f), 2);
    assert!(!f.root.join("a.rs").exists());
    assert!(!f.root.join("b.rs").exists());
    let engine = Engine::open(&f.root).unwrap();
    engine.store.db.execute(
        "UPDATE records SET value=json_set(value,'$.snapshotId',?1) WHERE kind='native_meaning_graph_v1' AND id=?2",
        [&"a".repeat(64), id],
    ).unwrap();
    drop(engine);
    let output = command(&f, "aliases").arg(id).output().unwrap();
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    assert_eq!(calls(&f), 2);
}

#[test]
fn meaning_alias_limits_fail_before_creating_state() {
    for (option, value) in [
        ("--min-score-bps", "0"),
        ("--min-score-bps", "10001"),
        ("--max-concepts", "1025"),
        ("--max-pairs", "262145"),
        ("--max-candidates", "8193"),
        ("--max-similarity-work", "64000001"),
    ] {
        let f = fixture("ok");
        let output = command(&f, "aliases")
            .args([&"a".repeat(64), option, value])
            .output()
            .unwrap();
        assert!(!output.status.success());
        assert!(
            String::from_utf8_lossy(&output.stderr).contains("alias"),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(!f.root.join(".yoke").exists());
        assert_eq!(calls(&f), 0);
    }
}

#[test]
fn meaning_cli_build_reference_is_bounded_and_graph_reads_never_create_overflow_artifacts() {
    let f = fixture("ok");
    let output = build(&f, &["--max-bytes", "1024"]);
    assert!(output.stdout.len() <= 1024);
    let reference = success(output);
    assert_eq!(reference["status"], "partial");
    assert_eq!(reference["buildStatus"], "ready");
    let id = reference["graphId"].as_str().unwrap();
    let before = records(&f, "native_meaning_graph_v1");
    let engine = Engine::open(&f.root).unwrap();
    let artifact_count: u64 = engine
        .store
        .db
        .query_row(
            "SELECT count(*) FROM records WHERE kind='artifact'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    drop(engine);
    for operation in ["load", "query"] {
        let mut request = command(&f, operation);
        request.arg(id);
        if operation == "query" {
            request.arg("Storage");
        }
        let output = request.args(["--max-bytes", "1024"]).output().unwrap();
        assert!(!output.status.success());
        assert!(String::from_utf8_lossy(&output.stderr).contains("output budget"));
    }
    assert_eq!(records(&f, "native_meaning_graph_v1"), before);
    let engine = Engine::open(&f.root).unwrap();
    let after: u64 = engine
        .store
        .db
        .query_row(
            "SELECT count(*) FROM records WHERE kind='artifact'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(artifact_count, after);
    assert_eq!(calls(&f), 2);
}

#[test]
fn meaning_cli_admits_the_entire_selection_and_cost_before_first_worker() {
    for mode in 0..3 {
        let f = fixture("ok");
        if mode == 0 {
            let mut config: Value = serde_json::from_slice(&fs::read(&f.config).unwrap()).unwrap();
            config["taskLimitMicros"] = json!(15);
            write_json(&f.config, &config);
        } else {
            let mut policy: Value = serde_json::from_slice(&fs::read(&f.policy).unwrap()).unwrap();
            policy[if mode == 1 {
                "maxPackets"
            } else {
                "maxGraphBytes"
            }] = json!(1);
            write_json(&f.policy, &policy);
        }
        assert!(!build(&f, &[]).status.success());
        assert_eq!(calls(&f), 0);
        assert!(records(&f, "native_meaning_job_v1").is_empty());
    }
}

#[test]
fn meaning_cli_failed_later_claim_keeps_billing_and_no_partial_publication() {
    let f = fixture("second-invalid");
    let output = build(&f, &[]);
    assert!(!output.status.success());
    let jobs = records(&f, "native_meaning_job_v1");
    assert_eq!(jobs.len(), 1);
    assert_eq!(jobs[0]["state"], "failed");
    let id = jobs[0]["jobId"].as_str().unwrap();
    assert!(String::from_utf8_lossy(&output.stderr).contains(id));
    assert!(records(&f, "native_meaning_graph_v1").is_empty());
    assert!(records(&f, "native_meaning_cache_v1").is_empty());
    let status = success(command(&f, "status").arg(id).output().unwrap());
    assert_eq!(status["status"], "failed");
    assert_eq!(status["knownReportedCostMicros"], 14);
    assert_eq!(calls(&f), 2);
}

#[test]
fn meaning_cli_output_pipe_loss_reports_the_already_durable_job_and_graph() {
    let f = fixture("ok");
    let mut child = command(&f, "build")
        .arg("--config")
        .arg(&f.config)
        .arg("--policy")
        .arg(&f.policy)
        .arg("--trust-provider")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    drop(child.stdout.take());
    let output = child.wait_with_output().unwrap();
    assert!(!output.status.success());
    let graphs = records(&f, "native_meaning_graph_v1");
    assert_eq!(graphs.len(), 1);
    let error = String::from_utf8_lossy(&output.stderr);
    assert!(
        error.contains(graphs[0]["graphId"].as_str().unwrap()),
        "{error}"
    );
    assert!(
        error.contains(graphs[0]["jobId"].as_str().unwrap()),
        "{error}"
    );
    assert_eq!(calls(&f), 2);
}

fn meaning_group_store_state(f: &Fixture) -> Value {
    let engine = Engine::open(&f.root).unwrap();
    let mut statement = engine.store.db.prepare("SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%' ORDER BY name").unwrap();
    let tables = statement
        .query_map([], |row| row.get::<_, String>(0))
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    let mut sql = std::collections::BTreeMap::new();
    for table in tables {
        let query = format!("SELECT * FROM \"{}\"", table.replace('"', "\"\""));
        let mut statement = engine.store.db.prepare(&query).unwrap();
        let count = statement.column_count();
        let mut rows = statement
            .query_map([], |row| {
                (0..count)
                    .map(|i| Ok(format!("{:?}", row.get_ref(i)?)))
                    .collect::<rusqlite::Result<Vec<_>>>()
            })
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        rows.sort();
        sql.insert(table, rows);
    }
    fn files(root: &Path, path: &Path, rows: &mut std::collections::BTreeMap<String, String>) {
        for entry in fs::read_dir(path).unwrap() {
            let path = entry.unwrap().path();
            let metadata = fs::symlink_metadata(&path).unwrap();
            assert!(!metadata.file_type().is_symlink());
            if metadata.is_dir() {
                files(root, &path, rows);
            } else {
                rows.insert(
                    path.strip_prefix(root)
                        .unwrap()
                        .to_string_lossy()
                        .into_owned(),
                    hash(fs::read(&path).unwrap()),
                );
            }
        }
    }
    let mut cas = std::collections::BTreeMap::new();
    for name in ["blobs", "trees"] {
        files(&engine.store.dir, &engine.store.dir.join(name), &mut cas);
    }
    json!({"sql":sql,"cas":cas})
}

#[test]
fn meaning_group_cli_replays_parent_and_preserves_all_sql_cas_and_original_claims() {
    let f = fixture("ok");
    let built = success(build(&f, &[]));
    let id = built["graphId"].as_str().unwrap();
    let loaded = success(command(&f, "load").arg(id).output().unwrap());
    let members = loaded["packets"].as_array().unwrap().iter().map(|packet|
        json!({"packetIndex":packet["index"],"conceptId":packet["meaning"]["concepts"][0]["id"]})).collect::<Vec<_>>();
    let review = f._dir.path().join("group-review.json");
    write_json(
        &review,
        &json!({"schemaVersion":1,"graphId":id,"groups":[{
        "members":members,"representative":members[0],"rationale":"Explicit owned test selection"}]}),
    );
    fs::rename(&f.worker, f.worker.with_extension("disabled")).unwrap();
    fs::remove_file(f.root.join("a.rs")).unwrap();
    fs::remove_file(f.root.join("b.rs")).unwrap();
    let before = meaning_group_store_state(&f);
    let projected = success(
        command(&f, "group")
            .arg("--review")
            .arg(&review)
            .output()
            .unwrap(),
    );
    assert_eq!(projected["kind"], "meaning_group_projection_v1");
    assert_eq!(projected["nodes"].as_array().unwrap().len(), 1);
    assert_eq!(
        projected["nodes"][0]["members"].as_array().unwrap().len(),
        2
    );
    assert_eq!(projected["evidence"].as_array().unwrap().len(), 2);
    assert_eq!(
        projected["bindingValidation"],
        "exact_captured_source_replayed"
    );
    assert_eq!(projected["semanticIdentityEstablished"], false);
    assert_eq!(
        projected["packets"].as_array().unwrap().len(),
        loaded["packets"].as_array().unwrap().len()
    );
    for (projected_packet, original_packet) in projected["packets"]
        .as_array()
        .unwrap()
        .iter()
        .zip(loaded["packets"].as_array().unwrap())
    {
        assert_eq!(projected_packet["packetIndex"], original_packet["index"]);
        assert_eq!(projected_packet["inputHash"], original_packet["inputHash"]);
        assert_eq!(
            projected_packet["provenance"],
            original_packet["provenance"]
        );
        for key in ["sourceBindings", "unitSetHash", "contractHash", "coverage"] {
            assert_eq!(projected_packet[key], original_packet["meaning"][key]);
        }
    }
    for (option, value) in [
        ("--max-concepts", "1"),
        ("--max-members", "1"),
        ("--max-groups", "0"),
        ("--max-bytes", "1024"),
    ] {
        let output = command(&f, "group")
            .arg("--review")
            .arg(&review)
            .args([option, value])
            .output()
            .unwrap();
        assert!(!output.status.success(), "{option}");
        assert!(output.stdout.is_empty());
    }
    assert_eq!(meaning_group_store_state(&f), before);
    assert_eq!(calls(&f), 2);
    assert!(!f.root.join("a.rs").exists());
    let engine = Engine::open(&f.root).unwrap();
    engine.store.db.execute("UPDATE records SET value=json_set(value,'$.snapshotId',?1) WHERE kind='native_meaning_graph_v1' AND id=?2", [&hash("tampered-group-parent"),id]).unwrap();
    drop(engine);
    let tampered = meaning_group_store_state(&f);
    let output = command(&f, "group")
        .arg("--review")
        .arg(&review)
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    assert_eq!(meaning_group_store_state(&f), tampered);
    assert_eq!(calls(&f), 2);
}

#[test]
fn meaning_group_cli_rejects_ambiguous_host_review_and_limits_before_store_creation() {
    for case in 0..7 {
        let f = fixture("ok");
        let review = f._dir.path().join("bad-group-review.json");
        let mut request = json!({"schemaVersion":1,"graphId":hash("unknown"),"groups":[]});
        match case {
            0 => {
                request["unexpected"] = json!(true);
                write_json(&review, &request);
            }
            1 => fs::write(&review, r#"{"schemaVersion":1,"schemaVersion":1}"#).unwrap(),
            2 => fs::write(&review, "x".repeat(128001)).unwrap(),
            3 => {
                request["schemaVersion"] = json!(2);
                write_json(&review, &request);
            }
            _ => write_json(&review, &request),
        }
        let mut command = command(&f, "group");
        command.arg("--review").arg(&review);
        if case == 4 {
            command.args(["--max-concepts", "4097"]);
        }
        if case == 5 {
            command.args(["--max-members", "4097"]);
        }
        if case == 6 {
            command.args(["--max-bytes", "1023"]);
        }
        let output = command.output().unwrap();
        assert!(!output.status.success(), "case {case}");
        let error = String::from_utf8_lossy(&output.stderr);
        assert!(
            match case {
                0 => error.contains("schema"),
                1 => error.contains("Duplicate JSON key"),
                2 => error.contains("capacity"),
                3 => error.contains("group"),
                4 | 5 => error.contains("group"),
                _ => error.contains("output budget"),
            },
            "case {case}: {error}"
        );
        assert!(output.stdout.is_empty());
        assert!(!f.root.join(".yoke").exists(), "case {case}");
        assert_eq!(calls(&f), 0);
    }
}

#[test]
fn meaning_group_cli_requires_existing_store_and_refuses_capture_or_execution_flags() {
    let f = fixture("ok");
    let review = f._dir.path().join("group-review.json");
    write_json(
        &review,
        &json!({"schemaVersion":1,"graphId":hash("unknown"),"groups":[]}),
    );
    let output = command(&f, "group")
        .arg("--review")
        .arg(&review)
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("existing"));
    for flags in [
        vec!["--capture-config", "missing.json"],
        vec!["--language-server-config", "missing.json"],
        vec!["--allow-writes"],
        vec!["--snapshot", "invalid"],
    ] {
        let output = command(&f, "group")
            .arg("--review")
            .arg(&review)
            .args(flags)
            .output()
            .unwrap();
        assert!(!output.status.success());
        assert!(output.stdout.is_empty());
        assert!(!f.root.join(".yoke").exists());
        assert_eq!(calls(&f), 0);
    }
}
