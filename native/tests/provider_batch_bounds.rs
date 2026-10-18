//! Allocation admission: caller-provided graph facts cannot expand a tiny host policy.
use std::{
    alloc::{GlobalAlloc, Layout, System},
    fs,
    sync::atomic::{AtomicBool, AtomicUsize, Ordering},
};
use yoke::{
    Engine,
    capture::Deadline,
    model_bedrock::{BedrockSpec, Dialect},
    provider_batches::{EmbeddingBatchPolicy, generate_embeddings_batched},
    providers::ProviderSpec,
};
struct Tracked;
static TRACK: AtomicBool = AtomicBool::new(false);
static LARGEST: AtomicUsize = AtomicUsize::new(0);
unsafe impl GlobalAlloc for Tracked {
    unsafe fn alloc(&self, l: Layout) -> *mut u8 {
        if TRACK.load(Ordering::Relaxed) {
            LARGEST.fetch_max(l.size(), Ordering::Relaxed);
        }
        unsafe { System.alloc(l) }
    }
    unsafe fn dealloc(&self, p: *mut u8, l: Layout) {
        unsafe { System.dealloc(p, l) }
    }
    unsafe fn realloc(&self, p: *mut u8, l: Layout, n: usize) -> *mut u8 {
        if TRACK.load(Ordering::Relaxed) {
            LARGEST.fetch_max(n, Ordering::Relaxed);
        }
        unsafe { System.realloc(p, l, n) }
    }
}
#[global_allocator]
static ALLOC: Tracked = Tracked;
#[test]
fn entity_limit_rejects_before_copying_entire_unverified_graph_selection() {
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("a.rs"), "pub fn one() {}\n").unwrap();
    let mut engine = Engine::open(root.path()).unwrap();
    let (snapshot, mut graph, _, _) = engine.index(&Deadline::new(5000)).unwrap();
    let entity = graph
        .entities
        .iter()
        .find(|e| e.kind != "file")
        .unwrap()
        .clone();
    graph.entities = vec![entity; 100_000];
    let spec = BedrockSpec {
        trusted_network: true,
        endpoint: "http://127.0.0.1:1/".into(),
        region: "us-east-1".into(),
        remote_model_id: "fixture".into(),
        model: "fixture".into(),
        model_version: "v1".into(),
        credential_name: "unused".into(),
        dialect: Dialect::TitanV2 {
            dimensions: 256,
            normalize: true,
        },
        allow_loopback_http: true,
        additional_root_certificates: vec![],
    };
    let policy = EmbeddingBatchPolicy::for_bedrock(&spec, 1, 1, 1000).unwrap();
    let provider = ProviderSpec {
        trusted: true,
        command: "must-not-launch".into(),
        args: vec![],
        model_id: "fixture".into(),
        model_version: "v1".into(),
        task_id: "bounds-fixture".into(),
        currency: "EUR".into(),
        max_cost_micros: 1,
        task_limit_micros: 1,
    };
    let deadline = Deadline::new(30_000);
    LARGEST.store(0, Ordering::SeqCst);
    TRACK.store(true, Ordering::SeqCst);
    let result = generate_embeddings_batched(
        &engine.store,
        &snapshot,
        &graph,
        &provider,
        &policy,
        &deadline,
    );
    TRACK.store(false, Ordering::SeqCst);
    assert!(result.is_err());
    assert!(
        LARGEST.load(Ordering::SeqCst) <= 65_536,
        "host entity admission happened after oversized allocation: {}",
        LARGEST.load(Ordering::SeqCst)
    );
}
