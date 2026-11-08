use std::{
    alloc::{GlobalAlloc, Layout, System},
    sync::atomic::{AtomicBool, AtomicUsize, Ordering},
};
use tempfile::TempDir;
use yoke::{
    Engine,
    capture::Deadline,
    federation::{self, ImportGrant, Scope},
};
struct Observer;
static ACTIVE: AtomicBool = AtomicBool::new(false);
static LARGEST: AtomicUsize = AtomicUsize::new(0);
static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
unsafe impl GlobalAlloc for Observer {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if ACTIVE.load(Ordering::Relaxed) {
            LARGEST.fetch_max(layout.size(), Ordering::Relaxed);
        }
        unsafe { System.alloc(layout) }
    }
    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        unsafe { System.dealloc(pointer, layout) }
    }
    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        if ACTIVE.load(Ordering::Relaxed) {
            LARGEST.fetch_max(size, Ordering::Relaxed);
        }
        unsafe { System.realloc(pointer, layout, size) }
    }
}
#[global_allocator]
static ALLOCATOR: Observer = Observer;
#[test]
fn oversized_receipt_is_rejected_before_rust_allocation_and_restores_sql_limit() {
    let _lock = LOCK.lock().unwrap();
    let root = TempDir::new().unwrap();
    let receiver = Engine::open(root.path()).unwrap();
    let id = "a".repeat(64);
    receiver.store.db.execute("INSERT INTO records(kind,id,value) VALUES('native_federation_receipt_v1',?1,printf('%.*c',64000001,'x'))", [&id]).unwrap();
    let grant = ImportGrant {
        scope: Scope {
            tenant_id: "t".into(),
            project_id: "p".into(),
            sender_id: "s".into(),
            key_id: "k".into(),
        },
        public_key: "b".repeat(64),
        sender_workspace_id: "c".repeat(64),
        exclude: vec![],
    };
    let limit = rusqlite::limits::Limit::SQLITE_LIMIT_LENGTH;
    let original = receiver.store.db.limit(limit).unwrap();
    LARGEST.store(0, Ordering::Relaxed);
    ACTIVE.store(true, Ordering::Relaxed);
    let result = federation::snapshot(&receiver, &id, &grant, &Deadline::new(20_000));
    ACTIVE.store(false, Ordering::Relaxed);
    assert!(result.is_err());
    assert!(
        LARGEST.load(Ordering::Relaxed) < 1_000_000,
        "Rejected receipt allocated {} bytes",
        LARGEST.load(Ordering::Relaxed)
    );
    assert_eq!(receiver.store.db.limit(limit).unwrap(), original);
}

#[test]
fn source_and_graph_records_admit_size_before_allocating_rejected_payloads() {
    let _lock = LOCK.lock().unwrap();
    let sender_root = TempDir::new().unwrap();
    std::fs::write(sender_root.path().join("main.rs"), "fn source() {}\n").unwrap();
    let mut sender = Engine::open(sender_root.path()).unwrap();
    let deadline = Deadline::new(60_000);
    let (source, _, _, _) = sender.index(&deadline).unwrap();
    let root = TempDir::new().unwrap();
    let receiver = Engine::open(root.path()).unwrap();
    let secret = [47; 32];
    let scope = Scope {
        tenant_id: "t".into(),
        project_id: "p".into(),
        sender_id: "s".into(),
        key_id: "k".into(),
    };
    let grant = ImportGrant {
        scope: scope.clone(),
        public_key: hex::encode(
            ed25519_dalek::SigningKey::from_bytes(&secret)
                .verifying_key()
                .to_bytes(),
        ),
        sender_workspace_id: source.workspace_id.clone(),
        exclude: vec![],
    };
    let envelope = federation::export_snapshot(
        &sender.store,
        &source,
        &federation::ExportScope {
            scope,
            ttl_ms: 60_000,
        },
        &secret,
        &deadline,
    )
    .unwrap();
    let result = federation::import_snapshot(&receiver, envelope, &grant, &deadline).unwrap();
    let id = result["remoteId"].as_str().unwrap();
    let grants = std::collections::BTreeMap::from([("source".into(), grant.clone())]);
    let access = yoke::federation_archive::FederationArchiveAccess::new(&receiver, &grants);
    let prepared = access.capture("source", id, &deadline).unwrap();
    let mut oversized = prepared.snapshot().clone();
    let mut entry = oversized.files[0].clone();
    entry.path = "x".repeat(1024);
    oversized.files = vec![entry; 2001];
    LARGEST.store(0, Ordering::Relaxed);
    ACTIVE.store(true, Ordering::Relaxed);
    let result = access.authorize(
        prepared.binding(),
        &oversized,
        prepared.graph(),
        &mut |_, _| panic!("oversized snapshot must precede source read"),
        &deadline,
    );
    ACTIVE.store(false, Ordering::Relaxed);
    assert!(result.is_err());
    assert!(
        LARGEST.load(Ordering::Relaxed) < 1_000_000,
        "Rejected snapshot allocated {} bytes",
        LARGEST.load(Ordering::Relaxed)
    );
    let file = &source.files[0];
    let cas = receiver.store.dir.join("blobs").join(&file.hash);
    std::fs::OpenOptions::new()
        .write(true)
        .open(&cas)
        .unwrap()
        .set_len(64_000_000)
        .unwrap();
    LARGEST.store(0, Ordering::Relaxed);
    ACTIVE.store(true, Ordering::Relaxed);
    let result = federation::snapshot(&receiver, id, &grant, &deadline);
    ACTIVE.store(false, Ordering::Relaxed);
    assert!(result.is_err());
    assert!(
        LARGEST.load(Ordering::Relaxed) < 1_000_000,
        "Rejected source allocated {} bytes",
        LARGEST.load(Ordering::Relaxed)
    );
    std::fs::write(cas, "fn source() {}\n").unwrap();
    receiver.store.db.execute("UPDATE records SET value=printf('%.*c',64000001,'x') WHERE kind='native_federation_graph_v1' AND id=?1",[id]).unwrap();
    let limit = rusqlite::limits::Limit::SQLITE_LIMIT_LENGTH;
    let original = receiver.store.db.limit(limit).unwrap();
    LARGEST.store(0, Ordering::Relaxed);
    ACTIVE.store(true, Ordering::Relaxed);
    let result = federation::graph(&receiver, id, &grant, &deadline);
    ACTIVE.store(false, Ordering::Relaxed);
    assert!(result.is_err());
    assert!(
        LARGEST.load(Ordering::Relaxed) < 1_000_000,
        "Rejected graph allocated {} bytes",
        LARGEST.load(Ordering::Relaxed)
    );
    assert_eq!(receiver.store.db.limit(limit).unwrap(), original);
    receiver.store.db.set_limit(limit, 4096).unwrap();
    assert!(federation::graph(&receiver, id, &grant, &deadline).is_err());
    assert_eq!(receiver.store.db.limit(limit).unwrap(), 4096);
}
