use std::{
    alloc::{GlobalAlloc, Layout, System},
    fs,
    sync::atomic::{AtomicBool, AtomicUsize, Ordering},
};
use tempfile::TempDir;
use yoke::Engine;
struct ObservedAllocator;
static OBSERVE: AtomicBool = AtomicBool::new(false);
static LARGEST: AtomicUsize = AtomicUsize::new(0);
unsafe impl GlobalAlloc for ObservedAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if OBSERVE.load(Ordering::Relaxed) {
            LARGEST.fetch_max(layout.size(), Ordering::Relaxed);
        }
        unsafe { System.alloc(layout) }
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        if OBSERVE.load(Ordering::Relaxed) {
            LARGEST.fetch_max(layout.size(), Ordering::Relaxed);
        }
        unsafe { System.alloc_zeroed(layout) }
    }
    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        if OBSERVE.load(Ordering::Relaxed) {
            LARGEST.fetch_max(new_size, Ordering::Relaxed);
        }
        unsafe { System.realloc(pointer, layout, new_size) }
    }
    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        unsafe { System.dealloc(pointer, layout) }
    }
}
#[global_allocator]
static ALLOCATOR: ObservedAllocator = ObservedAllocator;
#[test]
fn corrupted_tiny_cas_objects_are_rejected_before_large_allocation_on_write_and_artifact_read() {
    let root = TempDir::new().unwrap();
    let engine = Engine::open(root.path()).unwrap();
    let bytes = b"bounded original artifact";
    let id = engine.store.artifact(bytes).unwrap();
    let path = engine.store.dir.join("blobs").join(&id);
    fs::OpenOptions::new()
        .write(true)
        .open(path)
        .unwrap()
        .set_len(64_000_000)
        .unwrap();
    for artifact in [false, true] {
        LARGEST.store(0, Ordering::Relaxed);
        OBSERVE.store(true, Ordering::Relaxed);
        let result = if artifact {
            engine.store.read_artifact(&id)
        } else {
            engine.store.blob(bytes).map(|id| id.into_bytes())
        };
        OBSERVE.store(false, Ordering::Relaxed);
        assert!(result.is_err());
        let largest = LARGEST.load(Ordering::Relaxed);
        assert!(
            largest < 1_000_000,
            "Corrupt tiny object allocated {largest} bytes before rejection (artifact={artifact})"
        );
    }
}
