//! Test support using only an explicitly owned local protocol worker.
use serde_json::{Value, json};
use std::{fs, path::PathBuf};
use tempfile::TempDir;
use yoke::{
    Engine,
    analysis::AnalyzerEpoch,
    capture::Deadline,
    graph_view::{self, BuildOptions, StructuralKind, View},
    meaning_build::{self, MeaningPolicy, MeaningSelection},
    model::{Graph, Snapshot, hash},
    providers::ProviderSpec,
};

pub const A: &str = "// exact café evidence\npub fn alpha() -> &'static str { \"über\" }\n";
pub const B: &str = "pub fn beta() -> u32 { 2 }\n";
pub const HISTORY_OLD: &str = "pub fn history() -> u32 { 0 }\n";
pub const HISTORY_NEW: &str = "pub fn history() -> u32 { 9 }\n";
pub fn d() -> Deadline {
    Deadline::new(30000)
}

pub struct Fixture {
    pub dir: TempDir,
    pub engine: Engine,
    #[allow(dead_code)] // Used by the portable archive target sharing this fixture module.
    pub original: Snapshot,
    pub snapshot: Snapshot,
    pub graph: Graph,
    pub view: View,
    pub marker: PathBuf,
    #[allow(dead_code)] // Used by the portable archive target sharing this fixture module.
    pub provider: ProviderSpec,
}
pub fn fixture(epoch: AnalyzerEpoch) -> Fixture {
    let dir = TempDir::new().unwrap();
    let root = dir.path().join("workspace");
    fs::create_dir(&root).unwrap();
    for (path, text) in [
        ("a.rs", A),
        ("b.rs", B),
        ("history.rs", HISTORY_OLD),
        ("GUIDE.md", "# Archive\n`a.rs::alpha`\n"),
    ] {
        fs::write(root.join(path), text).unwrap();
    }
    fs::write(root.join("opaque.bin"), [0, 255, 0]).unwrap();
    let worker = dir.path().join("archive-worker.cjs");
    let marker = dir.path().join("provider-calls.txt");
    fs::write(&worker, r#"
let raw='';process.stdin.on('data',b=>raw+=b);process.stdin.on('end',()=>{
 const r=JSON.parse(raw),fs=require('fs');fs.appendFileSync(process.argv[2],r.requestId+'\n');
 const all=r.payload.sources.map(s=>({sourceId:s.anchor.sourceId,startByte:s.anchor.startByte,endByte:s.anchor.endByte}));
 const concepts=[{key:'one',label:'Storage policy',description:'Owned fixture',aliases:['Archive'],support:all}];
 const relations=[],hyperedges=[];
 if(all.length===2){
  concepts.push({key:'two',label:'Read path',description:'Owned fixture',aliases:[],support:[all[1]]});
  concepts.push({key:'three',label:'Write path',description:'Owned fixture',aliases:[],support:[all[0]]});
  relations.push({source:'one',target:'two',relation:'uses',description:'Forward',support:all},
   {source:'one',target:'two',relation:'uses',description:'Parallel',support:all},
   {source:'two',target:'one',relation:'uses',description:'Reverse',support:all});
  hyperedges.push({relation:'coordinates',participants:[{role:'output',concept:'three'},
   {role:'input',concept:'one'},{role:'input',concept:'two'}],description:'Ordered repeated roles',support:all});
 }
 process.stdout.write(JSON.stringify({schema:1,operation:r.operation,model:r.model,modelVersion:r.modelVersion,
 requestId:r.requestId,inputHash:r.inputHash,sourceBindings:r.payload.sources.map(s=>s.anchor),currency:r.currency,
 actualCostMicros:null,usage:null,data:{concepts,relations,hyperedges,
 coverage:r.payload.sources.map(s=>({sourceId:s.anchor.sourceId,outcome:'claims'}))}}));
});
"#).unwrap();
    let mut engine = Engine::open(&root).unwrap();
    engine.capture_epoch = epoch;
    let (original, original_graph, _, _) = engine.index(&d()).unwrap();
    let provider = ProviderSpec {
        trusted: true,
        command: PathBuf::from("node"),
        args: vec![
            worker.to_str().unwrap().into(),
            marker.to_str().unwrap().into(),
        ],
        model_id: "portable-meaning-owned-fixture".into(),
        model_version: "v1".into(),
        task_id: "portable-meaning-owned-fixture".into(),
        currency: "EUR".into(),
        max_cost_micros: 10,
        task_limit_micros: 200,
    };
    let policy = |snapshot: &Snapshot| {
        let mut p = MeaningPolicy::files(snapshot, hash("portable-meaning-owned-contract"));
        p.selection = MeaningSelection::Files(vec!["a.rs".into(), "b.rs".into()]);
        p.context_groups = vec![vec!["a.rs".into(), "b.rs".into()]];
        p
    };
    let first = meaning_build::build(
        &engine.store,
        &original,
        &original_graph,
        &provider,
        &policy(&original),
        &d(),
    )
    .unwrap();
    assert_eq!(first["status"], "ready");
    fs::write(root.join("history.rs"), HISTORY_NEW).unwrap();
    let (snapshot, graph, _, _) = engine.index(&d()).unwrap();
    let second = meaning_build::build(
        &engine.store,
        &snapshot,
        &graph,
        &provider,
        &policy(&snapshot),
        &d(),
    )
    .unwrap();
    assert_eq!(second["status"], "ready");
    assert_eq!(
        second["modelCallsThisRun"], 0,
        "This fixture must exercise an original receipt rebound"
    );
    let options = BuildOptions {
        structural_kinds: vec![StructuralKind::Markdown],
        meaning_graph_ids: vec![second["graphId"].as_str().unwrap().into()],
        ..BuildOptions::default()
    };
    let view = graph_view::build(&engine.store, &snapshot, &graph, &options, &d()).unwrap();
    assert!(!view.overlays.is_empty());
    assert!(
        view.meaning
            .as_ref()
            .unwrap()
            .packets
            .iter()
            .all(|p| p.provenance.rebound)
    );
    assert_eq!(fs::read_to_string(&marker).unwrap().lines().count(), 3);
    fs::remove_file(worker).unwrap();
    Fixture {
        dir,
        engine,
        original,
        snapshot,
        graph,
        view,
        marker,
        provider,
    }
}

pub fn store_state(f: &Fixture) -> Value {
    let count = |table: &str| {
        f.engine
            .store
            .db
            .query_row(&format!("SELECT count(*) FROM {table}"), [], |row| {
                row.get::<_, i64>(0)
            })
            .unwrap()
    };
    let total: i64 = f
        .engine
        .store
        .db
        .query_row("SELECT total_changes()", [], |row| row.get(0))
        .unwrap();
    json!([
        count("snapshots"),
        count("graphs"),
        count("records"),
        total,
        fs::read(&f.marker).unwrap(),
        blob_inventory(&f.engine.store)
    ])
}

pub fn blob_inventory(store: &yoke::store::Store) -> Value {
    // Unregistered Store::blob writes do not alter SQL counters or artifact registrations.
    let mut entries = std::collections::BTreeMap::new();
    for entry in fs::read_dir(store.dir.join("blobs")).unwrap() {
        let entry = entry.unwrap();
        let metadata = fs::symlink_metadata(entry.path()).unwrap();
        assert!(metadata.is_file(), "Unexpected non-file in fixture CAS");
        let bytes = fs::read(entry.path()).unwrap();
        entries.insert(
            entry.file_name().to_str().unwrap().to_owned(),
            json!({"size":metadata.len(),"hash":hash(&bytes)}),
        );
    }
    serde_json::to_value(entries).unwrap()
}

#[allow(dead_code)] // Used by the collection target sharing this fixture module.
pub fn registry_state(directory: &std::path::Path) -> Value {
    let db = rusqlite::Connection::open_with_flags(
        directory.join("yoke-collection.sqlite"),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .unwrap();
    let mut stmt = db
        .prepare("SELECT name,id,payload FROM registrations ORDER BY name")
        .unwrap();
    let registrations = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    let mut stmt = db
        .prepare("SELECT hash,hex(bytes) FROM source_blobs ORDER BY hash")
        .unwrap();
    let blobs = stmt
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    json!({"registrations":registrations,"blobs":blobs})
}

pub fn remove_source_workspace(f: Fixture) -> TempDir {
    let root = f.engine.root.clone();
    assert!(root.is_absolute());
    assert!(root.starts_with(f.dir.path().canonicalize().unwrap()));
    assert_eq!(root.file_name().unwrap(), "workspace");
    drop(f.engine);
    fs::remove_dir_all(&root).unwrap();
    assert!(!root.exists());
    f.dir
}

#[allow(dead_code)] // Used by the collection target sharing this fixture module.
pub fn foreign_only_source(text: &str) -> (TempDir, Engine, Snapshot) {
    // An actual signed import leaves bytes in CAS without making its foreign source a local read grant.
    use ed25519_dalek::SigningKey;
    use yoke::federation::{self, ExportScope, ImportGrant, Scope};
    let directory = TempDir::new().unwrap();
    let sender_root = directory.path().join("sender");
    let receiver_root = directory.path().join("receiver");
    fs::create_dir(&sender_root).unwrap();
    fs::create_dir(&receiver_root).unwrap();
    fs::write(sender_root.join("a.rs"), text).unwrap();
    let mut sender = Engine::open(&sender_root).unwrap();
    let (source, _, _, _) = sender.index(&d()).unwrap();
    let receiver = Engine::open(&receiver_root).unwrap();
    let seed = [87; 32];
    let scope = Scope {
        tenant_id: "archive-test".into(),
        project_id: "foreign-only".into(),
        sender_id: "fixture-sender".into(),
        key_id: "fixture-key".into(),
    };
    let grant = ImportGrant {
        scope: scope.clone(),
        public_key: hex::encode(SigningKey::from_bytes(&seed).verifying_key().to_bytes()),
        sender_workspace_id: source.workspace_id.clone(),
        exclude: vec![],
    };
    let envelope = federation::export_snapshot(
        &sender.store,
        &source,
        &ExportScope {
            scope,
            ttl_ms: 60_000,
        },
        &seed,
        &d(),
    )
    .unwrap();
    let imported = federation::import_snapshot(&receiver, envelope, &grant, &d()).unwrap();
    let snapshot = federation::snapshot(
        &receiver,
        imported["remoteId"].as_str().unwrap(),
        &grant,
        &d(),
    )
    .unwrap();
    drop(sender);
    // No ImportGrant/FederationArchiveAccess escapes this fixture.
    (directory, receiver, snapshot)
}
