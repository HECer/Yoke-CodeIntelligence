//! A valid Meaning closure must not expose structural artifacts after late admission failure.
use std::{collections::BTreeMap, fs, path::PathBuf};
use tempfile::TempDir;
use yoke::{
    Engine,
    analysis::AnalyzerEpoch,
    capture::Deadline,
    graph_view::{self, BuildOptions},
    meaning_build::{self, MeaningPolicy, MeaningSelection},
    model::{Graph, Snapshot, hash, identity},
    providers::ProviderSpec,
};

const CAP: usize = 32_000_000;
const SOURCE: &str = "fn alpha() {}\n";

struct Fixture {
    engine: Engine,
    snapshot: Snapshot,
    graph: Graph,
    _dir: TempDir,
}

fn deadline() -> Deadline {
    Deadline::new(180000)
}

fn fixture(settings: &str) -> Fixture {
    let dir = TempDir::new().unwrap();
    let root = dir.path().join("workspace");
    fs::create_dir(&root).unwrap();
    fs::write(root.join("main.rs"), SOURCE).unwrap();
    fs::write(root.join("settings.json"), settings).unwrap();
    let mut engine = Engine::open(&root).unwrap();
    engine.capture_epoch = AnalyzerEpoch::V9;
    let (snapshot, graph, _, _) = engine.index(&deadline()).unwrap();
    Fixture {
        engine,
        snapshot,
        graph,
        _dir: dir,
    }
}

#[derive(Debug, PartialEq, Eq)]
struct State {
    tables: BTreeMap<String, i64>,
    records: String,
    blobs: BTreeMap<String, u64>,
}

fn state(f: &Fixture) -> State {
    let db = &f.engine.store.db;
    let names: Vec<String> = db.prepare("SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%' ORDER BY name").unwrap()
        .query_map([], |r| r.get(0)).unwrap().collect::<rusqlite::Result<_>>().unwrap();
    let tables = names
        .into_iter()
        .map(|name| {
            let n = db
                .query_row(&format!("SELECT count(*) FROM {name}"), [], |r| r.get(0))
                .unwrap();
            (name, n)
        })
        .collect();
    let records: Vec<(String, String, String)> = db
        .prepare("SELECT kind,id,value FROM records ORDER BY kind,id")
        .unwrap()
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
        .unwrap()
        .collect::<rusqlite::Result<_>>()
        .unwrap();
    let blobs = fs::read_dir(f.engine.store.dir.join("blobs"))
        .unwrap()
        .map(|e| {
            let e = e.unwrap();
            (
                e.file_name().into_string().unwrap(),
                e.metadata().unwrap().len(),
            )
        })
        .collect();
    State {
        tables,
        records: identity(&records),
        blobs,
    }
}

#[test]
fn valid_meaning_plus_default_structural_budget_failure_publishes_no_records_or_cas() {
    // These keys have real bounded source positions. No records or CAS metadata are padded.
    let settings: BTreeMap<_, _> = (0..600)
        .map(|i| (format!("setting_{i:04}_{}", "x".repeat(1500)), "local"))
        .collect();
    let settings = serde_json::to_string(&settings).unwrap();
    let probe = fixture(&settings);
    let mut structural_bytes = 0usize;
    for extract in [
        yoke::manifest_graph::extract,
        yoke::config_graph::extract,
        yoke::markdown_graph::extract,
    ] {
        // Probe a separate workspace using the released API. The tested workspace
        // below must still have no previously published default derived artifacts.
        let output = extract(
            &probe.engine.store,
            &probe.snapshot,
            &probe.graph,
            &deadline(),
        )
        .unwrap();
        structural_bytes += probe
            .engine
            .store
            .read_artifact(&output.artifact_id)
            .unwrap()
            .len();
    }
    assert!(
        structural_bytes > 2_000_000 && structural_bytes < 6_000_000,
        "Unexpected genuine structural fixture size: {structural_bytes}"
    );
    drop(probe);

    let f = fixture(&settings);
    let worker = f._dir.path().join("meaning-budget-worker.cjs");
    let marker = f._dir.path().join("provider-calls.log");
    fs::write(&worker, r#"
let raw='';process.stdin.on('data',b=>raw+=b);process.stdin.on('end',()=>{
 const r=JSON.parse(raw),fs=require('fs');fs.appendFileSync(process.argv[2],r.requestId+'\n');
 const s=r.payload.sources[0],support=[{sourceId:s.anchor.sourceId,startByte:s.anchor.startByte,endByte:s.anchor.startByte+1}];
 const description='d'.repeat(1000);
 const concepts=Array.from({length:128},(_,i)=>({key:'c'+i,label:'Concept '+String(i).padStart(3,'0')+' '+'l'.repeat(144),description,aliases:Array.from({length:8},(_,j)=>'alias'+j+' '+'a'.repeat(150)),support}));
 const relations=Array.from({length:256},(_,i)=>({source:'c0',target:'c1',relation:'r'+i,description,support}));
 const hyperedges=Array.from({length:64},(_,i)=>({relation:'h'+i,participants:[{role:'input',concept:'c0'},{role:'output',concept:'c1'}],description,support}));
 const data={concepts,relations,hyperedges,coverage:r.payload.sources.map(s=>({sourceId:s.anchor.sourceId,outcome:'claims'}))};
 process.stdout.write(JSON.stringify({schema:1,operation:r.operation,model:r.model,modelVersion:r.modelVersion,requestId:r.requestId,inputHash:r.inputHash,sourceBindings:r.payload.sources.map(s=>s.anchor),currency:r.currency,actualCostMicros:0,usage:{inputTokens:0,outputTokens:0,cachedInputTokens:0},data}));
});
"#).unwrap();
    let provider = ProviderSpec {
        trusted: true,
        command: PathBuf::from("node"),
        args: vec![
            worker.to_str().unwrap().into(),
            marker.to_str().unwrap().into(),
        ],
        model_id: "owned-meaning-budget-fixture".into(),
        model_version: "v1".into(),
        task_id: "owned-meaning-budget-fixture".into(),
        currency: "EUR".into(),
        max_cost_micros: 1,
        task_limit_micros: 128,
    };
    let mut ids = Vec::new();
    let mut charged = 0usize;
    let mut check_at = 2usize;
    for generation in 0..29 {
        let mut policy = MeaningPolicy::files(&f.snapshot, hash("owned-meaning-budget-contract"));
        policy.selection = MeaningSelection::Files(vec!["main.rs".into()]);
        policy.generation = format!("mixed-budget-{generation}");
        let result = meaning_build::build(
            &f.engine.store,
            &f.snapshot,
            &f.graph,
            &provider,
            &policy,
            &deadline(),
        )
        .unwrap();
        assert_eq!(
            result["status"], "ready",
            "Owned bounded provider fixture failed: {result}"
        );
        ids.push(result["graphId"].as_str().unwrap().to_owned());
        if ids.len() > 2 && ids.len() < check_at {
            continue;
        }
        let prior_charge = charged;
        let layer = yoke::meaning_view::load_selected(
            &f.engine.store,
            &f.snapshot,
            &f.graph,
            &ids,
            CAP,
            &deadline(),
        )
        .expect("The selected closure must remain genuinely valid below the limit")
        .unwrap();
        charged = yoke::meaning_view::charged_input_bytes(&layer).unwrap();
        if charged + structural_bytes > CAP {
            break;
        }
        if ids.len() == 2 {
            // Two genuine checked prefixes choose when to recheck. The final
            // capacity proof below always uses the complete real closure.
            let increment = charged.checked_sub(prior_charge).unwrap();
            assert!(increment > 0 && increment < structural_bytes);
            check_at = (2 + (CAP - structural_bytes - charged) / increment + 1).min(29);
        }
    }
    assert!(
        (26_000_000..=CAP).contains(&charged) && charged + structural_bytes > CAP,
        "Fixture did not reach the intended combined boundary: meaning={charged}, structural={structural_bytes}, graphs={}",
        ids.len()
    );
    let meaning_only = BuildOptions {
        structural_kinds: vec![],
        meaning_graph_ids: ids.clone(),
        ..BuildOptions::default()
    };
    let valid = graph_view::build(
        &f.engine.store,
        &f.snapshot,
        &f.graph,
        &meaning_only,
        &deadline(),
    )
    .expect("A genuine Meaning-only View must succeed before testing mixed admission");
    assert!(valid.meaning.is_some());
    assert!(serde_json::to_vec(&valid).unwrap().len() < CAP);
    let calls = fs::read_to_string(&marker).unwrap();
    assert_eq!(calls.lines().count(), ids.len());
    fs::rename(&worker, worker.with_extension("disabled")).unwrap();
    let before = state(&f);
    let combined = BuildOptions {
        meaning_graph_ids: ids,
        ..BuildOptions::default()
    };
    let error = graph_view::build(
        &f.engine.store,
        &f.snapshot,
        &f.graph,
        &combined,
        &deadline(),
    )
    .expect_err("Combined selected input must be refused before publishing derived artifacts");
    assert!(
        format!("{error:#}").contains("Selected overlay aggregate byte capacity exceeded"),
        "The test must reach the combined input admission boundary: {error:#}"
    );
    assert_eq!(
        state(&f),
        before,
        "Rejected combined selection published structural records or CAS"
    );
    assert_eq!(fs::read_to_string(&marker).unwrap(), calls);
}
