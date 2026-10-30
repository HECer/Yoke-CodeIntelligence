use std::{
    alloc::{GlobalAlloc, Layout, System},
    sync::atomic::{AtomicBool, AtomicUsize, Ordering},
};
use yoke::model::{hash, identity};

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
fn large_identity_preserves_exact_released_json_bytes_without_a_second_corpus_buffer() {
    let value = serde_json::json!({
        "text": "世界\n\"\\\t captured source ".repeat(200_000),
        "nested": [null, true, false, {"min": i64::MIN, "max": u64::MAX, "float": -0.0}],
        "empty": []
    });
    // This is the released identity byte contract, independently materialized
    // before allocation observation. The production path must stream those exact bytes.
    let bytes = serde_json::to_vec(&value).unwrap();
    assert!(bytes.len() > 4_000_000);
    let expected = hash(&bytes);
    drop(bytes);
    LARGEST.store(0, Ordering::SeqCst);
    ACTIVE.store(true, Ordering::SeqCst);
    let actual = identity(&value);
    ACTIVE.store(false, Ordering::SeqCst);
    let largest = LARGEST.load(Ordering::SeqCst);
    assert_eq!(
        actual, expected,
        "Existing immutable identities must remain byte exact"
    );
    assert!(
        largest <= 64 * 1024,
        "Hashing an already allocated value requested a second {largest}-byte buffer"
    );
}
