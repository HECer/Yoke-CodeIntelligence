//! Isolated allocator observation; fixtures are allocated before the admission call.
use std::{
    alloc::{GlobalAlloc, Layout, System},
    sync::atomic::{AtomicBool, AtomicUsize, Ordering},
};
use yoke::{capture::Deadline, json::encode_bounded};
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
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) }
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        if ACTIVE.load(Ordering::Relaxed) {
            LARGEST.fetch_max(size, Ordering::Relaxed);
        }
        unsafe { System.realloc(ptr, layout, size) }
    }
}
#[test]
fn escaped_json_is_admitted_before_allocation_and_preserves_exact_bytes() {
    let oversized = serde_json::json!({"capturedText":"\u{1}\"\\\n".repeat(400_000)});
    let accepted = serde_json::json!({"text":"世界\"\\\n", "numbers":[i64::MIN,u64::MAX],"float":-0.0,"nested":[null,true,false]});
    let expected = serde_json::to_vec(&accepted).unwrap();
    let deadline = Deadline::new(5000);
    LARGEST.store(0, Ordering::SeqCst);
    ACTIVE.store(true, Ordering::SeqCst);
    let rejected = encode_bounded(&oversized, 1024, &deadline);
    ACTIVE.store(false, Ordering::SeqCst);
    let largest = LARGEST.load(Ordering::SeqCst);
    assert!(rejected.is_err());
    assert!(
        largest <= 64 * 1024,
        "Output budget rejection allocated {largest} bytes first"
    );
    let bytes = encode_bounded(&accepted, expected.len(), &Deadline::new(5000)).unwrap();
    assert_eq!(bytes, expected);
    assert_eq!(bytes.capacity(), bytes.len());
    assert!(encode_bounded(&accepted, expected.len() - 1, &Deadline::new(5000)).is_err());
    assert!(encode_bounded(&oversized, usize::MAX, &Deadline::new(0)).is_err());
    struct Changing {
        calls: std::cell::Cell<usize>,
        later: String,
    }
    impl serde::Serialize for Changing {
        fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
            let call = self.calls.get();
            self.calls.set(call + 1);
            serializer.serialize_str(if call == 0 { "first" } else { &self.later })
        }
    }
    let changing = Changing {
        calls: std::cell::Cell::new(0),
        later: "x".repeat(100_000),
    };
    LARGEST.store(0, Ordering::SeqCst);
    ACTIVE.store(true, Ordering::SeqCst);
    let rejected = encode_bounded(&changing, 200_000, &Deadline::new(5000));
    ACTIVE.store(false, Ordering::SeqCst);
    assert!(
        rejected.is_err(),
        "A serializer cannot expand after count admission"
    );
    assert!(LARGEST.load(Ordering::SeqCst) <= 64 * 1024);
}
