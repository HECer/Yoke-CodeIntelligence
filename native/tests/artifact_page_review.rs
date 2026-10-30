use serde_json::json;
use std::{
    alloc::{GlobalAlloc, Layout, System},
    sync::atomic::{AtomicBool, AtomicUsize, Ordering},
};
use yoke::Engine;
struct ObservedAllocator;
static TRACK: AtomicBool = AtomicBool::new(false);
static LARGEST: AtomicUsize = AtomicUsize::new(0);
static SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());
unsafe impl GlobalAlloc for ObservedAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if TRACK.load(Ordering::Relaxed) {
            LARGEST.fetch_max(layout.size(), Ordering::Relaxed);
        }
        unsafe { System.alloc(layout) }
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        if TRACK.load(Ordering::Relaxed) {
            LARGEST.fetch_max(layout.size(), Ordering::Relaxed);
        }
        unsafe { System.alloc_zeroed(layout) }
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        if TRACK.load(Ordering::Relaxed) {
            LARGEST.fetch_max(size, Ordering::Relaxed);
        }
        unsafe { System.realloc(ptr, layout, size) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) }
    }
}
#[global_allocator]
static ALLOCATOR: ObservedAllocator = ObservedAllocator;

#[test]
fn oversized_resource_rejection_happens_before_allocating_the_artifact_body() {
    let _serial = SERIAL.lock().unwrap_or_else(|error| error.into_inner());
    let root = tempfile::tempdir().unwrap();
    let mut engine = Engine::open(root.path()).unwrap();
    let id = engine.store.artifact(&vec![b'x'; 16_000_000]).unwrap();
    let input = format!(
        "{}\n{}\n",
        json!({"jsonrpc":"2.0","id":1,"method":"initialize"}),
        json!({"jsonrpc":"2.0","id":2,"method":"resources/read","params":{"uri":format!("ci://artifact/{id}")}})
    );
    let mut output = vec![];
    LARGEST.store(0, Ordering::Relaxed);
    TRACK.store(true, Ordering::Relaxed);
    let result = yoke::protocol::serve(&mut engine, std::io::Cursor::new(input), &mut output);
    TRACK.store(false, Ordering::Relaxed);
    result.unwrap();
    let rows: Vec<serde_json::Value> = String::from_utf8(output)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert!(rows[1].get("error").is_some());
    assert!(
        LARGEST.load(Ordering::Relaxed) < 1_000_000,
        "32KB resource rejection allocated {} bytes from a16MB artifact",
        LARGEST.load(Ordering::Relaxed)
    );
}

#[test]
fn a_small_artifact_page_does_not_allocate_the_entire_valid_artifact() {
    let _serial = SERIAL.lock().unwrap_or_else(|error| error.into_inner());
    let root = tempfile::tempdir().unwrap();
    let mut engine = Engine::open(root.path()).unwrap();
    let id = engine.store.artifact(&vec![b'x'; 16_000_000]).unwrap();
    LARGEST.store(0, Ordering::Relaxed);
    TRACK.store(true, Ordering::Relaxed);
    let result = engine.invoke("ci_read", json!({"artifactId":id,"maxBytes":1024}));
    TRACK.store(false, Ordering::Relaxed);
    assert_eq!(result.unwrap()["status"], "ok");
    assert!(
        LARGEST.load(Ordering::Relaxed) < 1_000_000,
        "Small artifact page allocated {} bytes from a16MB artifact",
        LARGEST.load(Ordering::Relaxed)
    );
}
