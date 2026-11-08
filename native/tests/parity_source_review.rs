use std::{
    alloc::{GlobalAlloc, Layout, System},
    fs,
    sync::atomic::{AtomicBool, AtomicUsize, Ordering},
};
use yoke::{Engine, capture::Deadline, changes};
struct ObservedAllocator;
static TRACK: AtomicBool = AtomicBool::new(false);
static LARGEST: AtomicUsize = AtomicUsize::new(0);
unsafe impl GlobalAlloc for ObservedAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if TRACK.load(Ordering::Relaxed) {
            LARGEST.fetch_max(layout.size(), Ordering::Relaxed);
        }
        unsafe { System.alloc(layout) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) }
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        if TRACK.load(Ordering::Relaxed) {
            LARGEST.fetch_max(new_size, Ordering::Relaxed);
        }
        unsafe { System.realloc(ptr, layout, new_size) }
    }
}
#[global_allocator]
static ALLOCATOR: ObservedAllocator = ObservedAllocator;

#[test]
fn corrupted_oversized_cas_is_rejected_before_an_unbounded_source_buffer_allocation() {
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("main.rs"), "fn actual() {}\n").unwrap();
    let mut engine = Engine::open(root.path()).unwrap();
    let deadline = Deadline::new(20000);
    let snapshot = engine.index(&deadline).unwrap().0;
    let blob = engine.store.dir.join("blobs").join(&snapshot.files[0].hash);
    fs::OpenOptions::new()
        .write(true)
        .open(blob)
        .unwrap()
        .set_len(64_000_000)
        .unwrap();
    let destination = tempfile::tempdir().unwrap();
    LARGEST.store(0, Ordering::Relaxed);
    TRACK.store(true, Ordering::Relaxed);
    let result = changes::materialize(&engine.store, &snapshot, destination.path(), &deadline);
    TRACK.store(false, Ordering::Relaxed);
    assert!(result.is_err());
    assert!(
        LARGEST.load(Ordering::Relaxed) < 8_000_000,
        "source buffer admission allocated {} bytes before rejecting corrupt captured size",
        LARGEST.load(Ordering::Relaxed)
    );
}
