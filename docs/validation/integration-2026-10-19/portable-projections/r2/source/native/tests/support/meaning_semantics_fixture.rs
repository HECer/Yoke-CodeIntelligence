//! Owned Meaning fixtures with independent generations and ordered participants.
//! Explicit fixture construction runs a local Node worker with literal responses.
//! Source citations and generation namespaces remain available to comparison tests.
use serde_json::Value;
use std::{fs, path::PathBuf};
use tempfile::TempDir;
use yoke::{
    Engine,
    analysis::AnalyzerEpoch,
    capture::Deadline,
    graph_view::{self, BuildOptions, View},
    meaning_build::{self, MeaningPolicy, MeaningSelection},
    model::{Graph, Snapshot, hash},
    providers::ProviderSpec,
};

pub const A: &str =
    "// __H12_SOURCE_A_9efc__ caf\u{e9}\r\npub fn alpha() -> &'static str { \"\u{fc}ber\" }\r\n";
pub const B: &str = "// __H12_SOURCE_B_432d__ \u{3b2}\r\npub fn beta() -> u32 { 2 }\r\n";
pub const DESCRIPTION: &str = "__H12_DESCRIPTION_NOT_OUTPUT_540b__";
pub const ALIAS: &str = "__H12_ALIAS_NOT_OUTPUT_6c7e__";
pub const SENTINELS: &[&str] = &[
    "__H12_SOURCE_A_9efc__",
    "__H12_SOURCE_B_432d__",
    DESCRIPTION,
    ALIAS,
];
pub fn d() -> Deadline {
    Deadline::new(30_000)
}
pub struct MeaningPair {
    pub dir: TempDir,
    pub engine: Engine,
    pub snapshot: Snapshot,
    pub graph: Graph,
    pub before: View,
    pub after: View,
    pub marker: PathBuf,
}

pub fn fixture() -> MeaningPair {
    let dir = TempDir::new().unwrap();
    let root = dir.path().join("workspace");
    fs::create_dir(&root).unwrap();
    fs::write(root.join("a.rs"), A).unwrap();
    fs::write(root.join("b.rs"), B).unwrap();
    let worker = dir.path().join("h12-semantics-worker.cjs");
    let marker = dir.path().join("h12-owned-calls.txt");
    fs::write(&worker, r#"
let raw='';process.stdin.on('data',b=>raw+=b);process.stdin.on('end',()=>{
 const r=JSON.parse(raw),fs=require('fs'),variant=process.argv[3];
 fs.appendFileSync(process.argv[2],variant+':'+r.requestId+'\n');
 const all=r.payload.sources.map(s=>({sourceId:s.anchor.sourceId,startByte:s.anchor.startByte,endByte:s.anchor.endByte}));
 const concepts=[{key:'one',label:'Alpha concept',description:'__H12_DESCRIPTION_NOT_OUTPUT_540b__',aliases:['__H12_ALIAS_NOT_OUTPUT_6c7e__'],support:all}];
 const relations=[],hyperedges=[];
 if(all.length===2){
  concepts.push({key:'two',label:'Beta concept',description:'__H12_DESCRIPTION_NOT_OUTPUT_540b__',aliases:['__H12_ALIAS_NOT_OUTPUT_6c7e__'],support:[all[1]]});
  concepts.push({key:'three',label:'Gamma concept',description:'__H12_DESCRIPTION_NOT_OUTPUT_540b__',aliases:['__H12_ALIAS_NOT_OUTPUT_6c7e__'],support:[all[0]]});
  const add=(source,target,ordinal)=>relations.push({source,target,relation:'uses',description:'__H12_DESCRIPTION_NOT_OUTPUT_540b__ '+source+' '+target+' '+ordinal,support:all});
  if(variant==='before'){add('one','two',0);add('one','two',1);add('two','one',0);add('one','one',0);}
  else if(variant==='after'){add('one','two',0);add('two','one',0);add('two','one',1);add('one','one',0);add('one','one',1);}
  else throw Error('Unknown explicitly owned fixture variant');
  const inputs=variant==='before'?['one','two']:['two','one'];
  hyperedges.push({relation:'coordinates',participants:[...inputs.map(concept=>({role:'input',concept})),{role:'context',concept:'one'},{role:'output',concept:'three'}],description:'__H12_DESCRIPTION_NOT_OUTPUT_540b__ repeated roles and concept under distinct roles',support:all});
 }
 process.stdout.write(JSON.stringify({schema:1,operation:r.operation,model:r.model,modelVersion:r.modelVersion,
 requestId:r.requestId,inputHash:r.inputHash,sourceBindings:r.payload.sources.map(s=>s.anchor),currency:r.currency,
 actualCostMicros:null,usage:null,data:{concepts,relations,hyperedges,
 coverage:r.payload.sources.map(s=>({sourceId:s.anchor.sourceId,outcome:'claims'}))}}));
});
"#).unwrap();
    let mut engine = Engine::open(&root).unwrap();
    engine.capture_epoch = AnalyzerEpoch::V9;
    let (snapshot, graph, _, _) = engine.index(&d()).unwrap();
    let build = |variant: &str| -> View {
        let provider = ProviderSpec {
            trusted: true,
            command: PathBuf::from("node"),
            args: vec![
                worker.to_str().unwrap().into(),
                marker.to_str().unwrap().into(),
                variant.into(),
            ],
            model_id: format!("r3-h12-owned-{variant}"),
            model_version: "v1".into(),
            task_id: format!("r3-h12-owned-task-{variant}"),
            currency: "EUR".into(),
            max_cost_micros: 10,
            task_limit_micros: 200,
        };
        let mut policy = MeaningPolicy::files(&snapshot, hash("r3-h12-owned-contract"));
        policy.generation = format!("r3-h12-generation-{variant}");
        policy.selection = MeaningSelection::Files(vec!["a.rs".into(), "b.rs".into()]);
        policy.context_groups = vec![vec!["a.rs".into(), "b.rs".into()]];
        let result: Value =
            meaning_build::build(&engine.store, &snapshot, &graph, &provider, &policy, &d())
                .unwrap();
        assert_eq!(
            result["status"], "ready",
            "Genuine fixture setup must succeed before comparison assertions"
        );
        assert_eq!(
            result["modelCallsThisRun"], 3,
            "Distinct explicit generations must not be an accidental cached/rebound response"
        );
        graph_view::build(
            &engine.store,
            &snapshot,
            &graph,
            &BuildOptions {
                structural_kinds: vec![],
                meaning_graph_ids: vec![result["graphId"].as_str().unwrap().into()],
                ..BuildOptions::default()
            },
            &d(),
        )
        .unwrap()
    };
    let before = build("before");
    let after = build("after");
    assert_ne!(before.id, after.id);
    assert_eq!(before.snapshot_id, after.snapshot_id);
    assert_eq!(fs::read_to_string(&marker).unwrap().lines().count(), 6);
    fs::remove_file(&worker).unwrap();
    MeaningPair {
        dir,
        engine,
        snapshot,
        graph,
        before,
        after,
        marker,
    }
}

pub fn remove_store_and_workspace(f: MeaningPair) -> TempDir {
    let root = f.engine.root.clone();
    assert!(root.is_absolute());
    assert!(root.starts_with(f.dir.path().canonicalize().unwrap()));
    assert_eq!(root.file_name().unwrap(), "workspace");
    drop(f.engine);
    fs::remove_dir_all(&root).unwrap();
    assert!(!root.exists());
    assert!(!f.dir.path().join("h12-semantics-worker.cjs").exists());
    f.dir
}
