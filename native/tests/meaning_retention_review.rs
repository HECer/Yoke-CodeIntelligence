//! Independent lifecycle checks using an owned local worker, without a real model or network.
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::PathBuf,
};
use yoke::{
    Engine,
    capture::Deadline,
    maintenance,
    meaning_build::{self, MeaningPolicy, MeaningSelection},
    model::{Snapshot, hash},
    providers::ProviderSpec,
    retention,
};

struct Fixture {
    engine: Engine,
    unrelated: Snapshot,
    original: Snapshot,
    rebound: Snapshot,
    latest: Snapshot,
    original_result: Value,
    rebound_result: Value,
    marker: PathBuf,
    _directory: tempfile::TempDir,
}

fn deadline() -> Deadline {
    Deadline::new(30_000)
}

fn fixture() -> Fixture {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().join("workspace");
    fs::create_dir(&root).unwrap();
    fs::write(root.join("a.rs"), "pub fn unrelated_history() {}\n").unwrap();
    let mut engine = Engine::open(&root).unwrap();
    let unrelated = engine.index(&deadline()).unwrap().0;
    fs::write(root.join("a.rs"), "pub fn captured_original() {}\n").unwrap();
    let (original, graph, _, _) = engine.index(&deadline()).unwrap();

    let script = directory.path().join("meaning-retention.cjs");
    let marker = directory.path().join("worker-calls.log");
    fs::write(&script, r#"
let input='';process.stdin.on('data',part=>input+=part);process.stdin.on('end',()=>{
 const r=JSON.parse(input),fs=require('fs');fs.appendFileSync(process.argv[2],r.requestId+'\n');
 const data={concepts:[],relations:[],hyperedges:[],coverage:r.payload.sources.map(s=>({sourceId:s.anchor.sourceId,outcome:'no_claims'}))};
 process.stdout.write(JSON.stringify({schema:1,operation:r.operation,model:r.model,modelVersion:r.modelVersion,requestId:r.requestId,inputHash:r.inputHash,sourceBindings:r.payload.sources.map(s=>s.anchor),currency:r.currency,actualCostMicros:1,usage:{inputTokens:2,outputTokens:1,cachedInputTokens:0},data}));
});
"#).unwrap();
    let provider = ProviderSpec {
        trusted: true,
        command: PathBuf::from("node"),
        args: vec![
            script.to_str().unwrap().into(),
            marker.to_str().unwrap().into(),
        ],
        model_id: "owned-meaning-retention-fixture".into(),
        model_version: "v1".into(),
        task_id: "meaning-retention-review".into(),
        currency: "EUR".into(),
        max_cost_micros: 2,
        task_limit_micros: 20,
    };
    let mut policy = MeaningPolicy::files(&original, hash("owned retention review contract"));
    policy.selection = MeaningSelection::Files(vec!["a.rs".into()]);
    let original_result = meaning_build::build(
        &engine.store,
        &original,
        &graph,
        &provider,
        &policy,
        &deadline(),
    )
    .unwrap();
    assert_eq!(original_result["modelCallsThisRun"], 1);

    fs::write(root.join("b.rs"), "pub fn rebound_only_context() {}\n").unwrap();
    let (rebound, graph, _, _) = engine.index(&deadline()).unwrap();
    let rebound_result = meaning_build::build(
        &engine.store,
        &rebound,
        &graph,
        &provider,
        &policy,
        &deadline(),
    )
    .unwrap();
    assert_eq!(rebound_result["modelCallsThisRun"], 0);
    assert_eq!(rebound_result["cacheHits"], 1);
    fs::write(root.join("a.rs"), "pub fn current_source() {}\n").unwrap();
    fs::write(root.join("b.rs"), "pub fn current_context() {}\n").unwrap();
    let latest = engine.index(&deadline()).unwrap().0;
    Fixture {
        engine,
        unrelated,
        original,
        rebound,
        latest,
        original_result,
        rebound_result,
        marker,
        _directory: directory,
    }
}

fn protected_content(f: &Fixture) -> BTreeMap<String, Vec<u8>> {
    let mut ids = BTreeSet::new();
    for snapshot in [&f.original, &f.rebound] {
        ids.extend(snapshot.files.iter().map(|file| file.hash.clone()));
    }
    for result in [&f.original_result, &f.rebound_result] {
        let graph: Value = f
            .engine
            .store
            .get_record(
                meaning_build::GRAPH_KIND,
                result["graphId"].as_str().unwrap(),
            )
            .unwrap();
        ids.insert(graph["artifactId"].as_str().unwrap().to_owned());
        let job: Value = f
            .engine
            .store
            .get_record(meaning_build::JOB_KIND, result["jobId"].as_str().unwrap())
            .unwrap();
        for packet in job["packets"].as_array().unwrap() {
            ids.insert(packet["responseArtifactId"].as_str().unwrap().to_owned());
        }
    }
    ids.into_iter()
        .map(|id| {
            let bytes = f.engine.store.read_blob(&id).unwrap();
            (id, bytes)
        })
        .collect()
}

#[test]
fn original_and_rebound_meaning_roots_survive_retirement_quarantine_and_purge_with_controls() {
    let f = fixture();
    let protected = protected_content(&f);
    let orphan_bytes = b"unrelated orphan control for meaning retention";
    let orphan = f.engine.store.blob(orphan_bytes).unwrap();
    let plan =
        retention::plan(&f.engine.store, &retention::Policy::default(), &deadline()).unwrap();
    assert_eq!(
        plan["unknownDependencyKinds"],
        json!([]),
        "Meaning records need typed retention, not an opaque stop-all fallback"
    );
    assert_eq!(
        plan["snapshotIds"],
        json!([f.unrelated.id]),
        "An unrelated historical snapshot must remain eligible while both meaning sources are rooted"
    );
    let id = plan["planId"].as_str().unwrap();
    let retired = retention::apply(&f.engine.store, id, id, true, &deadline()).unwrap();
    assert_eq!(retired["retiredSnapshots"], 1);
    assert!(f.engine.store.snapshot(&f.unrelated.id).is_err());
    for snapshot in [&f.original, &f.rebound, &f.latest] {
        assert!(f.engine.store.snapshot(&snapshot.id).is_ok());
    }

    let moved = maintenance::collect(
        &f.engine.store,
        &maintenance::Policy {
            apply: true,
            minimum_age_ms: 0,
            purge_quarantine: false,
        },
        &deadline(),
    )
    .unwrap();
    let quarantine = PathBuf::from(moved["quarantinePath"].as_str().unwrap());
    assert!(moved["quarantinedBlobs"].as_u64().unwrap() >= 1);
    assert_eq!(fs::read(quarantine.join(&orphan)).unwrap(), orphan_bytes);
    for (id, bytes) in &protected {
        assert!(
            !quarantine.join(id).exists(),
            "A meaning dependency entered quarantine: {id}"
        );
        assert_eq!(f.engine.store.read_blob(id).unwrap(), *bytes);
    }
    let purge = retention::purge_plan(&f.engine.store, 0, &deadline()).unwrap();
    assert_eq!(purge["unknownDependencyKinds"], json!([]));
    assert!(purge["eligibleBytes"].as_u64().unwrap() >= orphan_bytes.len() as u64);
    let id = purge["planId"].as_str().unwrap();
    assert_eq!(
        retention::purge(&f.engine.store, id, id, true, &deadline()).unwrap()["complete"],
        true
    );
    assert!(!quarantine.join(&orphan).exists());
    for (id, bytes) in protected {
        assert_eq!(f.engine.store.read_blob(&id).unwrap(), bytes);
    }
    for result in [&f.original_result, &f.rebound_result] {
        let loaded = meaning_build::load(
            &f.engine.store,
            result["graphId"].as_str().unwrap(),
            &deadline(),
        )
        .unwrap();
        assert_eq!(loaded["resolution"], "inferred_concept");
        let status = meaning_build::status(
            &f.engine.store,
            result["jobId"].as_str().unwrap(),
            Some(&f.latest.id),
            &deadline(),
        )
        .unwrap();
        assert_eq!(status["freshness"], "changed");
    }
    assert_eq!(
        fs::read_to_string(&f.marker).unwrap().lines().count(),
        1,
        "Retirement, replay and purge must never invoke a provider"
    );
    assert_eq!(
        fs::read_to_string(f.engine.root.join("a.rs")).unwrap(),
        "pub fn current_source() {}\n"
    );
}

fn forged(kind: &str, mut value: Value) -> Value {
    if kind == meaning_build::JOB_KIND {
        value["policy"]["generation"] = json!("forged-review-generation");
    } else if kind == meaning_build::CACHE_KIND {
        value["generation"] = json!("forged-review-generation");
    } else {
        value["workspaceId"] = json!("f".repeat(64));
    }
    value
}

#[test]
fn supplied_job_cache_and_graph_metadata_cannot_override_the_persisted_retention_proof() {
    let f = fixture();
    for kind in [
        meaning_build::JOB_KIND,
        meaning_build::CACHE_KIND,
        meaning_build::GRAPH_KIND,
    ] {
        let (id, raw): (String, String) = f
            .engine
            .store
            .db
            .query_row(
                "SELECT id,value FROM records WHERE kind=?1 ORDER BY id LIMIT 1",
                [kind],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        let original: Value = serde_json::from_str(&raw).unwrap();
        assert!(
            meaning_build::validate_retained_record(
                &f.engine.store,
                kind,
                &id,
                &original,
                &deadline()
            )
            .is_ok(),
            "Valid {kind} fixture must be accepted before the adversarial assertion"
        );
        assert!(
            meaning_build::validate_retained_record(
                &f.engine.store,
                kind,
                &id,
                &forged(kind, original),
                &deadline()
            )
            .is_err(),
            "Supplied {kind} metadata replaced its immutable proof"
        );
        let saved: String = f
            .engine
            .store
            .db
            .query_row(
                "SELECT value FROM records WHERE kind=?1 AND id=?2",
                [kind, &id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(saved, raw);
    }
}

#[test]
fn corrupted_job_cache_and_graph_rows_refuse_retirement_and_purge_before_deleting_history() {
    let f = fixture();
    for kind in [
        meaning_build::JOB_KIND,
        meaning_build::CACHE_KIND,
        meaning_build::GRAPH_KIND,
    ] {
        let (id, raw): (String, String) = f
            .engine
            .store
            .db
            .query_row(
                "SELECT id,value FROM records WHERE kind=?1 ORDER BY id LIMIT 1",
                [kind],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        let corrupted = forged(kind, serde_json::from_str(&raw).unwrap()).to_string();
        f.engine
            .store
            .db
            .execute(
                "UPDATE records SET value=?1 WHERE kind=?2 AND id=?3",
                rusqlite::params![corrupted, kind, id],
            )
            .unwrap();
        assert!(
            retention::plan(&f.engine.store, &retention::Policy::default(), &deadline()).is_err(),
            "Corrupt {kind} admitted a history retirement plan"
        );
        assert!(
            retention::purge_plan(&f.engine.store, 0, &deadline()).is_err(),
            "Corrupt {kind} admitted a purge plan"
        );
        for snapshot in [&f.unrelated, &f.original, &f.rebound, &f.latest] {
            assert!(f.engine.store.snapshot(&snapshot.id).is_ok());
        }
        f.engine
            .store
            .db
            .execute(
                "UPDATE records SET value=?1 WHERE kind=?2 AND id=?3",
                rusqlite::params![raw, kind, id],
            )
            .unwrap();
    }
}
