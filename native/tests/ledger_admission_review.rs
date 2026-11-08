use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
static ACTIVE: AtomicBool = AtomicBool::new(false);
static LARGEST: AtomicUsize = AtomicUsize::new(0);
struct Observed;
unsafe impl GlobalAlloc for Observed {
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
#[global_allocator]
static ALLOCATOR: Observed = Observed;
#[test]
fn oversized_accounting_cells_are_rejected_before_rust_materialization_and_restore_sql_limit() {
    use yoke::{capture::Deadline, ledger::Ledger, store::Store};
    let root = tempfile::tempdir().unwrap();
    let store = Store::open(root.path()).unwrap();
    let id = yoke::model::identity(&"oversized");
    store.db.execute("INSERT INTO records(kind,id,value) VALUES('native_model_task_v1',?1,CAST(zeroblob(16000000) AS TEXT))",[&id]).unwrap();
    let before = store
        .db
        .limit(rusqlite::limits::Limit::SQLITE_LIMIT_LENGTH)
        .unwrap();
    let d = Deadline::new(10000);
    let ledger = Ledger::new(&store, &d);
    LARGEST.store(0, Ordering::Relaxed);
    ACTIVE.store(true, Ordering::Relaxed);
    let rejected = ledger.total("oversized");
    ACTIVE.store(false, Ordering::Relaxed);
    assert!(rejected.is_err());
    let largest = LARGEST.load(Ordering::Relaxed);
    assert!(
        largest <= 70_000,
        "Oversized accounting rejection allocated {largest} bytes"
    );
    assert_eq!(
        store
            .db
            .limit(rusqlite::limits::Limit::SQLITE_LIMIT_LENGTH)
            .unwrap(),
        before
    );
    assert_eq!(ledger.total("missing").unwrap().attempts, 0);
}
