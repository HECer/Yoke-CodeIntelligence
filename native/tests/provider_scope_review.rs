//! Real local stdio contract fixtures, not neural models or network providers.
use std::{
    alloc::{GlobalAlloc, Layout, System},
    fs,
    path::PathBuf,
    sync::{
        Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
};
use yoke::{
    Engine,
    capture::Deadline,
    model::{Graph, Snapshot},
    providers::{self, ProviderSpec},
};
struct Observe;
static ACTIVE: AtomicBool = AtomicBool::new(false);
static LARGEST: AtomicUsize = AtomicUsize::new(0);
static REVIEW: Mutex<()> = Mutex::new(());
#[global_allocator]
static ALLOCATOR: Observe = Observe;
unsafe impl GlobalAlloc for Observe {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if ACTIVE.load(Ordering::Relaxed) {
            LARGEST.fetch_max(layout.size(), Ordering::Relaxed);
        }
        unsafe { System.alloc(layout) }
    }
    unsafe fn dealloc(&self, p: *mut u8, layout: Layout) {
        unsafe { System.dealloc(p, layout) }
    }
    unsafe fn realloc(&self, p: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        if ACTIVE.load(Ordering::Relaxed) {
            LARGEST.fetch_max(size, Ordering::Relaxed);
        }
        unsafe { System.realloc(p, layout, size) }
    }
}
fn provider(root: &std::path::Path) -> (ProviderSpec, PathBuf) {
    let script = root.join("labeled-provider.cjs");
    let marker = root.join("provider-ran");
    fs::write(&script,r#"let input='';process.stdin.on('data',b=>input+=b);process.stdin.on('end',()=>{const r=JSON.parse(input);require('fs').writeFileSync(process.argv[2],'ran');const s=r.payload.sources;process.stdout.write(JSON.stringify({schema:1,operation:r.operation,model:r.model,modelVersion:r.modelVersion,requestId:r.requestId,inputHash:r.inputHash,sourceBindings:s.map(x=>x.anchor),currency:r.currency,actualCostMicros:1,usage:{inputTokens:1,outputTokens:1,cachedInputTokens:0},data:{statements:[{text:'A labeled source fixture.',evidenceEntityIds:[s[0].anchor.sourceId]}]}}));});"#).unwrap();
    (
        ProviderSpec {
            trusted: true,
            command: PathBuf::from("node"),
            args: vec![
                script.to_str().unwrap().into(),
                marker.to_str().unwrap().into(),
            ],
            model_id: "scope-fixture".into(),
            model_version: "v1".into(),
            task_id: "scope-review".into(),
            currency: "EUR".into(),
            max_cost_micros: 10,
            task_limit_micros: 100,
        },
        marker,
    )
}
fn ordinary(root: &std::path::Path) -> (Engine, Snapshot, Graph) {
    let workspace = root.join("workspace");
    fs::create_dir(&workspace).unwrap();
    fs::write(workspace.join("main.rs"), "pub fn original() {}\n").unwrap();
    let mut engine = Engine::open(&workspace).unwrap();
    let (s, g, _, _) = engine.index(&Deadline::new(60_000)).unwrap();
    (engine, s, g)
}
#[test]
fn explicitly_captured_readonly_dependencies_keep_exact_model_source_binding() {
    let _guard = REVIEW.lock().unwrap_or_else(|p| p.into_inner());
    let root = tempfile::tempdir().unwrap();
    let workspace = root.path().join("workspace");
    fs::create_dir_all(workspace.join("node_modules/pkg")).unwrap();
    fs::write(
        workspace.join("node_modules/pkg/index.js"),
        "export function dependencyAlpha() { return 'Grüße 🌍'; }\n",
    )
    .unwrap();
    let mut engine = Engine::open(&workspace).unwrap();
    engine.capture_options.read_only_dependency_dirs = vec!["node_modules".into()];
    let d = Deadline::new(60_000);
    let (snapshot, graph, _, _) = engine.index(&d).unwrap();
    let entity = graph
        .entities
        .iter()
        .find(|e| e.name == "dependencyAlpha")
        .unwrap();
    let (spec, marker) = provider(root.path());
    let result = providers::summarize_sources(
        &engine.store,
        &snapshot,
        &graph,
        std::slice::from_ref(&entity.id),
        &spec,
        &d,
    )
    .unwrap();
    assert!(marker.exists());
    let file = snapshot
        .files
        .iter()
        .find(|f| f.path == entity.path)
        .unwrap();
    assert_eq!(
        result["sourceBindings"],
        serde_json::json!([{
            "sourceId":entity.id,"path":entity.path,"contentHash":file.hash,
            "startByte":entity.start_byte,"endByte":entity.end_byte
        }])
    );
    let edit=engine.invoke("ci_change_prepare",serde_json::json!({"snapshotId":snapshot.id,"replaceSymbol":{"entityId":entity.id,"text":"export function altered(){}"}})).unwrap();
    assert_eq!(edit["status"], "error");
    assert!(
        fs::read_to_string(workspace.join("node_modules/pkg/index.js"))
            .unwrap()
            .contains("dependencyAlpha")
    );
}
#[test]
fn corrupt_cas_size_is_refused_before_source_allocation_or_provider_launch() {
    let _guard = REVIEW.lock().unwrap_or_else(|p| p.into_inner());
    let root = tempfile::tempdir().unwrap();
    let (engine, snapshot, graph) = ordinary(root.path());
    let (spec, marker) = provider(root.path());
    let file = &snapshot.files[0];
    fs::write(
        engine.store.dir.join("blobs").join(&file.hash),
        vec![b'x'; 16_000_000],
    )
    .unwrap();
    let id = graph
        .entities
        .iter()
        .find(|e| e.name == "original")
        .unwrap()
        .id
        .clone();
    LARGEST.store(0, Ordering::Relaxed);
    ACTIVE.store(true, Ordering::Relaxed);
    let result = providers::summarize_sources(
        &engine.store,
        &snapshot,
        &graph,
        &[id],
        &spec,
        &Deadline::new(60_000),
    );
    ACTIVE.store(false, Ordering::Relaxed);
    let largest = LARGEST.load(Ordering::Relaxed);
    assert!(result.is_err());
    assert!(!marker.exists());
    assert!(
        largest <= 4_000_000,
        "Oversized corrupt source allocated {largest} bytes before expected-size admission"
    );
}
#[test]
fn escaped_request_is_admitted_before_serialization_allocation_and_billing() {
    let _guard = REVIEW.lock().unwrap_or_else(|p| p.into_inner());
    let root = tempfile::tempdir().unwrap();
    let workspace = root.path().join("workspace");
    fs::create_dir(&workspace).unwrap();
    // A single comment token avoids making parser-node admission the failure.
    // Tabs are valid source text and each expands to two JSON bytes.
    let escaped = "\t".repeat(4900);
    for part in 0..4 {
        let mut source = String::new();
        for n in 0..500 {
            source.push_str(&format!(
                "export function f_{part}_{n}(){{/*{escaped}*/return 1;}}\n"
            ));
        }
        fs::write(workspace.join(format!("part{part}.js")), source).unwrap();
    }
    let mut engine = Engine::open(&workspace).unwrap();
    let d = Deadline::new(60_000);
    let (snapshot, graph, _, _) = engine.index(&d).unwrap();
    assert_eq!(
        graph.entities.iter().filter(|e| e.kind != "file").count(),
        2000
    );
    let (spec, marker) = provider(root.path());
    LARGEST.store(0, Ordering::Relaxed);
    ACTIVE.store(true, Ordering::Relaxed);
    let result = providers::generate_embeddings(&engine.store, &snapshot, &graph, &spec, &d);
    ACTIVE.store(false, Ordering::Relaxed);
    let largest = LARGEST.load(Ordering::Relaxed);
    assert!(result.is_err());
    assert!(!marker.exists());
    assert_eq!(
        yoke::ledger::Ledger::new(&engine.store, &d)
            .total("scope-review")
            .unwrap()
            .attempts,
        0
    );
    assert!(
        largest <= 8_000_000,
        "Rejected escaped request allocated {largest} bytes before16MB admission"
    );
}
#[test]
fn persisted_unknown_native_identity_does_not_grant_model_source_authority() {
    let _guard = REVIEW.lock().unwrap_or_else(|p| p.into_inner());
    let root = tempfile::tempdir().unwrap();
    let (mut engine, mut snapshot, mut graph) = ordinary(root.path());
    let (spec, marker) = provider(root.path());
    snapshot.id = yoke::model::hash(b"unproven native source");
    graph.snapshot_id = snapshot.id.clone();
    for e in &mut graph.entities {
        e.snapshot_id = snapshot.id.clone();
    }
    for e in &mut graph.evidence {
        e.snapshot_id = snapshot.id.clone();
    }
    engine.store.publish_historical(&snapshot, &graph).unwrap();
    let id = graph
        .entities
        .iter()
        .find(|e| e.name == "original")
        .unwrap()
        .id
        .clone();
    let result = providers::summarize_sources(
        &engine.store,
        &snapshot,
        &graph,
        &[id],
        &spec,
        &Deadline::new(60_000),
    );
    assert!(
        result.is_err(),
        "An unproven native identity authorized a model source disclosure"
    );
    assert!(!marker.exists());
}
