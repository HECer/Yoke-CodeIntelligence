use std::{
    alloc::{GlobalAlloc, Layout, System},
    sync::atomic::{AtomicBool, AtomicUsize, Ordering},
};
struct Observe;
static ACTIVE: AtomicBool = AtomicBool::new(false);
static LARGEST: AtomicUsize = AtomicUsize::new(0);
#[global_allocator]
static ALLOCATOR: Observe = Observe;
unsafe impl GlobalAlloc for Observe {
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
#[test]
fn oversized_collection_invocation_is_rejected_before_serialization_allocation() {
    let root = tempfile::tempdir().unwrap();
    let d = yoke::capture::Deadline::new(30000);
    let mut session = yoke::project_session::Session::open(
        yoke::Engine::open(root.path()).unwrap(),
        Default::default(),
        Some("admission"),
        &d,
    )
    .unwrap();
    let args = serde_json::json!({"collection":"unknown","ignored":"a".repeat(4_100_000)});
    LARGEST.store(0, Ordering::SeqCst);
    ACTIVE.store(true, Ordering::SeqCst);
    let result = session.invoke("collections.list", &args, &d);
    ACTIVE.store(false, Ordering::SeqCst);
    let largest = LARGEST.load(Ordering::SeqCst);
    assert!(result.is_err());
    assert!(
        largest < 1_000_000,
        "Rejected collection input allocated {largest} bytes before admission"
    );
}
