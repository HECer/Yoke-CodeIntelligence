use super::*;
use crate::{
    Engine,
    analysis::AnalyzerEpoch,
    meaning_build::{self, MeaningPolicy, MeaningSelection},
    providers::ProviderSpec,
};
use std::{fs, fs::OpenOptions, os::windows::fs::OpenOptionsExt, path::PathBuf};

#[test]
fn archive_by_id_preflight_covers_every_manifest_before_any_cas_read() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().join("workspace");
    fs::create_dir(&root).unwrap();
    fs::write(root.join("main.rs"), "fn alpha() {}\n").unwrap();
    fs::write(root.join("history.rs"), "fn history() -> u32 { 1 }\n").unwrap();
    let worker = directory.path().join("scope-order-worker.cjs");
    let marker = directory.path().join("provider-calls.log");
    fs::write(
        &worker,
        r#"
let raw='';process.stdin.on('data',b=>raw+=b);process.stdin.on('end',()=>{
 const r=JSON.parse(raw),fs=require('fs');fs.appendFileSync(process.argv[2],r.requestId+'\n');
 const s=r.payload.sources[0],support=[{sourceId:s.anchor.sourceId,startByte:s.anchor.startByte,endByte:s.anchor.endByte}];
 process.stdout.write(JSON.stringify({schema:1,operation:r.operation,model:r.model,modelVersion:r.modelVersion,
 requestId:r.requestId,inputHash:r.inputHash,sourceBindings:r.payload.sources.map(s=>s.anchor),currency:r.currency,
 actualCostMicros:0,usage:null,data:{concepts:[{key:'one',label:'Archive scope',description:'Owned ordering fixture',aliases:[],support}],
 relations:[],hyperedges:[],coverage:[{sourceId:s.anchor.sourceId,outcome:'claims'}]}}));
});
"#,
    )
    .unwrap();
    let mut engine = Engine::open(&root).unwrap();
    engine.capture_epoch = AnalyzerEpoch::V9;
    let deadline = Deadline::new(30000);
    let (original, original_graph, _, _) = engine.index(&deadline).unwrap();
    let provider = ProviderSpec {
        trusted: true,
        command: PathBuf::from("node"),
        args: vec![
            worker.to_str().unwrap().into(),
            marker.to_str().unwrap().into(),
        ],
        model_id: "owned-archive-scope-order".into(),
        model_version: "v1".into(),
        task_id: "owned-archive-scope-order".into(),
        currency: "EUR".into(),
        max_cost_micros: 1,
        task_limit_micros: 10,
    };
    let policy = |snapshot: &Snapshot| {
        let mut policy = MeaningPolicy::files(snapshot, hash("archive-scope-order-contract"));
        policy.selection = MeaningSelection::Files(vec!["main.rs".into()]);
        policy
    };
    let first = meaning_build::build(
        &engine.store,
        &original,
        &original_graph,
        &provider,
        &policy(&original),
        &deadline,
    )
    .unwrap();
    assert_eq!(first["status"], "ready", "{first}");
    fs::write(root.join("history.rs"), "fn history() -> u32 { 2 }\n").unwrap();
    let (snapshot, graph, _, _) = engine.index(&deadline).unwrap();
    assert_ne!(original.id, snapshot.id);
    let rebound = meaning_build::build(
        &engine.store,
        &snapshot,
        &graph,
        &provider,
        &policy(&snapshot),
        &deadline,
    )
    .unwrap();
    assert_eq!(rebound["status"], "ready", "{rebound}");
    assert_eq!(rebound["modelCallsThisRun"], 0, "{rebound}");
    let ids = vec![rebound["graphId"].as_str().unwrap().to_owned()];
    let checked = meaning_build::checked_published_set(
        &engine.store,
        &snapshot,
        &graph,
        &ids,
        MAX_BYTES,
        &deadline,
    )
    .expect("The complete same-Store original/rebound closure must be valid before locking CAS");
    let expected = checked
        .archive()
        .source_scopes()
        .map(|scope| scope.id.clone())
        .collect::<Vec<_>>();
    let mut literal = vec![original.id, snapshot.id.clone()];
    literal.sort();
    assert_eq!(expected, literal);
    drop(checked);
    let view = build(
        &engine.store,
        &snapshot,
        &graph,
        &BuildOptions {
            structural_kinds: vec![],
            meaning_graph_ids: ids.clone(),
            ..Default::default()
        },
        &deadline,
    )
    .expect("The published typed View must be genuine before locking CAS");
    assert!(view.meaning.is_some());
    let positive =
        archive_meaning_view_by_id(&engine.store, &view.id, MAX_BYTES, 64_000_000, &deadline)
            .expect("The public by-ID preparation path must succeed before locking CAS");
    assert_eq!(positive.view().id, view.id);
    drop(positive);
    let calls = fs::read(&marker).unwrap();
    assert_eq!(String::from_utf8(calls.clone()).unwrap().lines().count(), 1);
    fs::remove_file(worker).unwrap();
    let before: i64 = engine
        .store
        .db
        .query_row("SELECT total_changes()", [], |row| row.get(0))
        .unwrap();
    // Any premature graph/response/source CAS open now produces a real sharing violation.
    let locks = fs::read_dir(engine.store.dir.join("blobs"))
        .unwrap()
        .map(|entry| {
            OpenOptions::new()
                .read(true)
                .share_mode(0)
                .open(entry.unwrap().path())
                .unwrap()
        })
        .collect::<Vec<_>>();
    assert!(!locks.is_empty());
    let sentinel = "owned-policy-denial-before-cas";
    let mut seen = Vec::new();
    let mut preflight = |scope: &Snapshot, _: &Deadline| {
        seen.push(scope.id.clone());
        if Some(&scope.id) == expected.last() {
            anyhow::bail!(sentinel);
        }
        Ok(())
    };
    let error = archive_meaning_view_by_id_guarded(
        &engine.store,
        &view.id,
        MAX_BYTES,
        64_000_000,
        &mut preflight,
        &deadline,
    )
    .expect_err("The final source scope deliberately denies this archive");
    drop(locks);
    assert!(
        format!("{error:#}").contains(sentinel),
        "A CAS read preceded complete source-policy preflight: {error:#}"
    );
    assert_eq!(
        seen, expected,
        "A base-only scope check omitted historical source"
    );
    let after: i64 = engine
        .store
        .db
        .query_row("SELECT total_changes()", [], |row| row.get(0))
        .unwrap();
    assert_eq!(after, before);
    assert_eq!(fs::read(marker).unwrap(), calls);
}
