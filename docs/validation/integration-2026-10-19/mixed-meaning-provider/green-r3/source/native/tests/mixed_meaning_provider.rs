//! Owned protocol processes exercise accounting and replay, not model quality.
use serde_json::{Value, json};
use std::{fs, io::Read, path::PathBuf};
use yoke::{
    capture::Deadline,
    ledger::{Ledger, Usage},
    mixed_view::{
        self, MeaningInputRequest, Query,
        provider::{self, Policy, SourceSelection},
    },
    model::hash,
    providers::ProviderSpec,
    retention,
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
let raw='';process.stdin.on('data',b=>raw+=b);process.stdin.on('end',()=>{
 const r=JSON.parse(raw),fs=require('fs'),mode=process.argv[3];fs.appendFileSync(process.argv[2],r.requestId+'\n');
 const units=r.payload.units;
 const supports=units.map(u=>{const p=u.text.indexOf('🌍'),start=u.source.startByte+(p<0?0:Buffer.byteLength(u.text.slice(0,p),'utf8'));
 return {sourceId:u.sourceId,startByte:start,endByte:start+(p<0?1:4)};});
 const concepts=[{key:'a',label:'Storage € 🌍',description:'Provider interpretation A',aliases:['same label'],support:[supports[0]]},
 {key:'b',label:'Storage',description:'Provider interpretation B',aliases:['same label'],support:[supports[1]]}];
 const edge={source:'a',target:'b',relation:'relates',description:'Unverified directed observation',support:supports};
 const data={concepts,relations:[edge,edge,{source:'b',target:'a',relation:'reverse',description:'Reverse observation',support:supports}],
 hyperedges:[{relation:'joint',participants:[{role:'input',concept:'a'},{role:'output',concept:'b'},{role:'aux',concept:'a'}],description:'Ordered role observation',support:supports}],
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
fn job(f: &Fixture) -> Value {
    let raw: String = f
        .engine
        .store
        .db
        .query_row(
            "SELECT value FROM records WHERE kind=?1 ORDER BY id LIMIT 1",
            [provider::JOB_KIND],
            |r| r.get(0),
        )
        .unwrap();
    serde_json::from_str(&raw).unwrap()
}
#[test]
fn explicit_mixed_provider_preserves_original_claims_and_replays_without_provider_or_live_files() {
    let f = fixture();
    let d = Deadline::new(60000);
    let inputs = inputs(&f, &d);
    let (spec, script, marker) = configured(&f, "success");
    let result = provider::build(
        &f.engine.store,
        &selection(&f),
        &inputs,
        Some(&worker()),
        &spec,
        &policy(),
        &d,
    )
    .unwrap();
    assert_eq!(result["state"], "complete");
    let id = result["jobId"].as_str().unwrap();
    let graph = provider::load(&f.engine.store, id, Some(&worker()), &d).unwrap();
    assert_eq!(graph["modelCallsThisRun"], 1);
    assert_eq!(graph["providerReportedCostMicros"], 4);
    assert_eq!(graph["providerAttestation"], "unverified");
    assert_eq!(graph["canonicalSourceAuthority"], false);
    let quote_spaces: std::collections::BTreeSet<_> = graph["sourceValidation"]["supportQuotes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|q| q["item"]["source"]["kind"].as_str().unwrap())
        .collect();
    assert_eq!(quote_spaces.len(), 4);
    assert!(
        graph["sourceValidation"]["supportQuotes"]
            .as_array()
            .unwrap()
            .iter()
            .any(|q| q["item"]["text"] == "🌍")
    );
    assert_eq!(
        graph["sourceValidation"]["originalFragment"]["relations"][0],
        graph["sourceValidation"]["originalFragment"]["relations"][1]
    );
    assert_eq!(
        graph["sourceValidation"]["originalFragment"]["hyperedges"][0]["participants"][2]["role"],
        "aux"
    );
    let calls = fs::read(&marker).unwrap();
    assert_eq!(calls.iter().filter(|b| **b == b'\n').count(), 1);
    for path in ["billing.rs", "office.docx", "notes.pdf"] {
        fs::remove_file(f.dir.path().join(path)).unwrap();
    }
    fs::remove_file(script).unwrap();
    refused(
        provider::load(&f.engine.store, id, None, &d),
        "mixed_view_pdf_permission_required",
    );
    assert_eq!(
        provider::load(&f.engine.store, id, Some(&worker()), &d).unwrap(),
        graph
    );
    let attempt = graph["attemptId"].as_str().unwrap();
    Ledger::new(&f.engine.store, &d)
        .usage(
            attempt,
            "later",
            Usage {
                input_tokens: 99,
                output_tokens: 99,
                cached_input_tokens: 0,
            },
        )
        .unwrap();
    let status = provider::status(&f.engine.store, id, &d).unwrap();
    assert_eq!(status["originalResponseUsage"]["inputTokens"], 12);
    assert_eq!(status["currentLedgerUsage"]["inputTokens"], 99);
    assert_eq!(
        provider::load(&f.engine.store, id, Some(&worker()), &d).unwrap(),
        graph
    );
    retention::plan(&f.engine.store, &Default::default(), &d).unwrap();
    assert_eq!(fs::read(marker).unwrap(), calls);
}
#[test]
fn bound_invalid_fragment_error_or_usage_retains_reported_bill_without_graph_or_secret_echo() {
    for mode in [
        "invalid-fragment",
        "provider-error",
        "invalid-usage",
        "split-utf8",
        "overrun",
    ] {
        let f = fixture();
        let d = Deadline::new(60000);
        let inputs = inputs(&f, &d);
        let (spec, _, marker) = configured(&f, mode);
        let error = provider::build(
            &f.engine.store,
            &selection(&f),
            &inputs,
            Some(&worker()),
            &spec,
            &policy(),
            &d,
        )
        .unwrap_err();
        assert!(!format!("{error:#}").contains("NEVER_ECHO_PROVIDER_ERROR_SENTINEL"));
        let row = job(&f);
        let status = provider::status(&f.engine.store, row["jobId"].as_str().unwrap(), &d).unwrap();
        assert_eq!(status["state"], "failed");
        let expected_cost = if mode == "overrun" { 11 } else { 4 };
        assert_eq!(status["originalResponseCostMicros"], expected_cost);
        assert_eq!(status["modelCallsThisRun"], 1);
        assert!(status["graphArtifactId"].is_null());
        let totals = Ledger::new(&f.engine.store, &d)
            .total(&spec.task_id)
            .unwrap();
        assert_eq!(totals.actual_micros, expected_cost);
        assert_eq!(totals.reserved_micros, 0);
        let calls = fs::read(&marker).unwrap();
        retention::plan(&f.engine.store, &Default::default(), &d).unwrap();
        if mode == "overrun" {
            let before = state(&f.engine);
            assert!(
                provider::build(
                    &f.engine.store,
                    &selection(&f),
                    &inputs,
                    Some(&worker()),
                    &spec,
                    &policy(),
                    &d
                )
                .is_err()
            );
            assert_eq!(state(&f.engine), before);
            assert_eq!(
                Ledger::new(&f.engine.store, &d)
                    .total(&spec.task_id)
                    .unwrap()
                    .attempts,
                1
            );
        }
        assert_eq!(fs::read(marker).unwrap(), calls);
    }
}
#[test]
fn unbound_reply_keeps_unknown_reservation_and_status_performs_no_retry() {
    for mode in ["unbound", "invalid-utf8", "duplicate-envelope"] {
        let f = fixture();
        let d = Deadline::new(60000);
        let inputs = inputs(&f, &d);
        let (spec, script, marker) = configured(&f, mode);
        assert!(
            provider::build(
                &f.engine.store,
                &selection(&f),
                &inputs,
                Some(&worker()),
                &spec,
                &policy(),
                &d
            )
            .is_err()
        );
        let row = job(&f);
        let id = row["jobId"].as_str().unwrap();
        let calls = fs::read(&marker).unwrap();
        fs::remove_file(script).unwrap();
        for _ in 0..2 {
            let status = provider::status(&f.engine.store, id, &d).unwrap();
            assert!(status["modelCallsThisRun"].is_null());
            assert!(status["originalResponseCostMicros"].is_null());
        }
        let totals = Ledger::new(&f.engine.store, &d)
            .total(&spec.task_id)
            .unwrap();
        assert_eq!(totals.actual_micros, 0);
        assert_eq!(totals.reserved_micros, 10);
        assert_eq!(totals.unknown, 1);
        assert_eq!(fs::read(marker).unwrap(), calls);
    }
}
#[test]
fn trust_source_permission_and_capacity_refuse_before_accounting_or_process() {
    let f = fixture();
    let d = Deadline::new(60000);
    let inputs = inputs(&f, &d);
    let (mut spec, _, marker) = configured(&f, "success");
    let before = state(&f.engine);
    spec.trusted = false;
    assert!(
        provider::build(
            &f.engine.store,
            &selection(&f),
            &inputs,
            Some(&worker()),
            &spec,
            &policy(),
            &d
        )
        .is_err()
    );
    spec.trusted = true;
    let mut small = policy();
    small.max_graph_bytes = 9_999_999;
    assert!(
        provider::build(
            &f.engine.store,
            &selection(&f),
            &inputs,
            Some(&worker()),
            &spec,
            &small,
            &d
        )
        .is_err()
    );
    refused(
        provider::build(
            &f.engine.store,
            &selection(&f),
            &inputs,
            None,
            &spec,
            &policy(),
            &d,
        ),
        "mixed_view_pdf_permission_required",
    );
    let mut bad = inputs.clone();
    bad.units[0].label = "../wrong".into();
    assert!(
        provider::build(
            &f.engine.store,
            &selection(&f),
            &bad,
            Some(&worker()),
            &spec,
            &policy(),
            &d
        )
        .is_err()
    );
    assert_eq!(state(&f.engine), before);
    assert!(!marker.exists());
}
#[test]
fn saved_quote_tampering_refuses_both_source_load_and_retention_before_history_changes() {
    let f = fixture();
    let d = Deadline::new(60000);
    let inputs = inputs(&f, &d);
    let (spec, _, _) = configured(&f, "success");
    let result = provider::build(
        &f.engine.store,
        &selection(&f),
        &inputs,
        Some(&worker()),
        &spec,
        &policy(),
        &d,
    )
    .unwrap();
    let id = result["jobId"].as_str().unwrap();
    let original_artifact = result["graphArtifactId"].as_str().unwrap();
    let mut graph: Value =
        serde_json::from_slice(&f.engine.store.read_artifact(original_artifact).unwrap()).unwrap();
    graph["sourceValidation"]["supportQuotes"][0]["item"]["text"] = json!("forged quotation");
    let artifact = f
        .engine
        .store
        .artifact(&serde_json::to_vec(&graph).unwrap())
        .unwrap();
    let mut row = job(&f);
    row["graphArtifactId"] = json!(artifact);
    f.engine
        .store
        .db
        .execute(
            "UPDATE records SET value=?2 WHERE kind=?1 AND id=?3",
            rusqlite::params![provider::JOB_KIND, row.to_string(), id],
        )
        .unwrap();
    let before = state(&f.engine);
    assert!(provider::load(&f.engine.store, id, Some(&worker()), &d).is_err());
    assert!(retention::plan(&f.engine.store, &Default::default(), &d).is_err());
    assert_eq!(state(&f.engine), before);
}

#[test]
fn unknown_original_cost_is_not_upgraded_by_later_ledger_reconciliation() {
    let f = fixture();
    let d = Deadline::new(60000);
    let inputs = inputs(&f, &d);
    let (spec, _, marker) = configured(&f, "unknown-cost");
    let result = provider::build(
        &f.engine.store,
        &selection(&f),
        &inputs,
        Some(&worker()),
        &spec,
        &policy(),
        &d,
    )
    .unwrap();
    let id = result["jobId"].as_str().unwrap();
    let graph = provider::load(&f.engine.store, id, Some(&worker()), &d).unwrap();
    assert!(graph["providerReportedCostMicros"].is_null());
    assert_eq!(
        graph["modelCallsScope"],
        "original_bound_host_build_attempt"
    );
    assert_eq!(graph["providerCallsThisRead"], 0);
    let attempt = graph["attemptId"].as_str().unwrap();
    Ledger::new(&f.engine.store, &d)
        .settle(attempt, Some(6))
        .unwrap();
    let status = provider::status(&f.engine.store, id, &d).unwrap();
    assert!(status["originalResponseCostMicros"].is_null());
    assert_eq!(status["currentLedgerCostMicros"], 6);
    assert_eq!(
        provider::load(&f.engine.store, id, Some(&worker()), &d).unwrap(),
        graph
    );
    assert_eq!(fs::read_to_string(marker).unwrap().lines().count(), 1);
}
#[test]
fn status_preserves_caller_read_transaction_sqlite_limit_and_never_spawns_a_provider() {
    let f = fixture();
    let d = Deadline::new(60000);
    let inputs = inputs(&f, &d);
    let (spec, _, marker) = configured(&f, "success");
    let result = provider::build(
        &f.engine.store,
        &selection(&f),
        &inputs,
        Some(&worker()),
        &spec,
        &policy(),
        &d,
    )
    .unwrap();
    let id = result["jobId"].as_str().unwrap();
    let calls = fs::read(&marker).unwrap();
    let db = &f.engine.store.db;
    let limit = rusqlite::limits::Limit::SQLITE_LIMIT_LENGTH;
    let original = db.set_limit(limit, 110_000).unwrap();
    db.execute_batch("BEGIN DEFERRED").unwrap();
    let status = provider::status(&f.engine.store, id, &d).unwrap();
    assert_eq!(status["providerCallsThisRead"], 0);
    assert_eq!(status["requiresNativePdfText"], true);
    assert!(!db.is_autocommit());
    assert_eq!(db.limit(limit).unwrap(), 110_000);
    assert!(
        provider::build(
            &f.engine.store,
            &selection(&f),
            &inputs,
            Some(&worker()),
            &spec,
            &policy(),
            &d
        )
        .is_err()
    );
    assert!(!db.is_autocommit());
    assert_eq!(db.limit(limit).unwrap(), 110_000);
    db.execute_batch("ROLLBACK").unwrap();
    db.set_limit(limit, original).unwrap();
    assert_eq!(fs::read(marker).unwrap(), calls);
}
#[test]
fn cli_build_requires_separate_trust_and_reads_replay_without_provider_configuration() {
    use std::process::Command;
    let f = fixture();
    let d = Deadline::new(60000);
    let inputs = inputs(&f, &d);
    let (spec, script, marker) = configured(&f, "success");
    let selection_file = f.dir.path().join("selection.json");
    let input_file = f.dir.path().join("inputs.json");
    let config = f.dir.path().join("host-provider.json");
    let policy_file = f.dir.path().join("host-policy.json");
    fs::write(&selection_file, serde_json::to_vec(&f.selection).unwrap()).unwrap();
    fs::write(&input_file, serde_json::to_vec(&inputs).unwrap()).unwrap();
    fs::write(&config,serde_json::to_vec(&json!({"command":spec.command,"args":spec.args,"modelId":spec.model_id,"modelVersion":spec.model_version,
        "taskId":spec.task_id,"currency":spec.currency,"maxCostMicros":spec.max_cost_micros,"taskLimitMicros":spec.task_limit_micros})).unwrap()).unwrap();
    fs::write(
        &policy_file,
        serde_json::to_vec(&json!({"contractHash":policy().contract_hash})).unwrap(),
    )
    .unwrap();
    let root = f.engine.root.clone();
    let run = |trust: bool| {
        let mut c = Command::new(env!("CARGO_BIN_EXE_yoke"));
        c.arg("--root")
            .arg(&root)
            .args([
                "--timeout",
                "60000",
                "--max-bytes",
                "4000000",
                "mixed-meaning",
                "build",
                "--selection",
            ])
            .arg(&selection_file)
            .arg("--inputs")
            .arg(&input_file)
            .arg("--config")
            .arg(&config)
            .arg("--policy")
            .arg(&policy_file)
            .arg("--native-pdf-text");
        if trust {
            c.arg("--trust-provider");
        }
        c.output().unwrap()
    };
    let before = state(&f.engine);
    let denied = run(false);
    assert!(!denied.status.success());
    assert!(denied.stdout.is_empty());
    assert!(!marker.exists());
    assert_eq!(state(&f.engine), before);
    drop(f.engine);
    let output = run(true);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let result: Value = serde_json::from_slice(&output.stdout).unwrap();
    let id = result["jobId"].as_str().unwrap();
    let calls = fs::read(&marker).unwrap();
    fs::remove_file(script).unwrap();
    fs::remove_file(config).unwrap();
    for op in ["status", "load"] {
        let mut c = Command::new(env!("CARGO_BIN_EXE_yoke"));
        c.arg("--root").arg(&root).args([
            "--timeout",
            "60000",
            "--max-bytes",
            "4000000",
            "mixed-meaning",
            op,
            id,
        ]);
        if op == "load" {
            c.arg("--native-pdf-text");
        }
        let output = c.output().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let read: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(read["providerCallsThisRead"], 0);
    }
    assert_eq!(fs::read(marker).unwrap(), calls);
}

#[test]
fn metadata_jobs_recover_identities_without_provider_files_source_replay_or_writes() {
    let f = fixture();
    let d = Deadline::new(60000);
    let inputs = inputs(&f, &d);
    let (spec, script, marker) = configured(&f, "success");
    let result = provider::build(
        &f.engine.store,
        &selection(&f),
        &inputs,
        Some(&worker()),
        &spec,
        &policy(),
        &d,
    )
    .unwrap();
    let id = result["jobId"].as_str().unwrap();
    let calls = fs::read(&marker).unwrap();
    fs::remove_file(script).unwrap();
    let before = state(&f.engine);
    let jobs = provider::jobs(&f.engine.store, &spec.task_id, 0, 1, &d).unwrap();
    assert_eq!(jobs["total"], 1);
    assert_eq!(jobs["jobs"][0]["jobId"], id);
    assert_eq!(jobs["jobs"][0]["recordedState"], "complete");
    assert_eq!(jobs["sourceReplay"], "not_performed");
    assert_eq!(jobs["metadataOnly"], true);
    assert_eq!(jobs["providerCallsThisRead"], 0);
    let empty = provider::jobs(&f.engine.store, "unregistered-task", 0, 1, &d).unwrap();
    assert_eq!(empty["jobs"], json!([]));
    assert_eq!(empty["total"], 0);
    let later = provider::jobs(&f.engine.store, &spec.task_id, 1, 1, &d).unwrap();
    assert_eq!(later["jobs"], json!([]));
    assert_eq!(later["total"], 1);
    assert!(provider::jobs(&f.engine.store, &spec.task_id, 0, 0, &d).is_err());
    assert_eq!(state(&f.engine), before);
    assert_eq!(fs::read(marker).unwrap(), calls);
}
