use std::{
    alloc::{GlobalAlloc, Layout, System},
    sync::{
        Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
};
use yoke::{Engine, capture::Deadline, config_graph};

struct Observe;
static ACTIVE: AtomicBool = AtomicBool::new(false);
static TOTAL: AtomicUsize = AtomicUsize::new(0);
static SERIAL: Mutex<()> = Mutex::new(());
#[global_allocator]
static ALLOCATOR: Observe = Observe;
unsafe impl GlobalAlloc for Observe {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if ACTIVE.load(Ordering::Relaxed) {
            TOTAL.fetch_add(layout.size(), Ordering::Relaxed);
        }
        unsafe { System.alloc(layout) }
    }
    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        unsafe { System.dealloc(pointer, layout) }
    }
    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        if ACTIVE.load(Ordering::Relaxed) {
            TOTAL.fetch_add(size, Ordering::Relaxed);
        }
        unsafe { System.realloc(pointer, layout, size) }
    }
}
fn refuses_before_expansion(text: String) {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("config.yaml"), text).unwrap();
    let mut engine = Engine::open(root.path()).unwrap();
    let deadline = Deadline::new(30_000);
    let (snapshot, graph, _, _) = engine.index(&deadline).unwrap();
    TOTAL.store(0, Ordering::SeqCst);
    ACTIVE.store(true, Ordering::SeqCst);
    let result = config_graph::extract(&engine.store, &snapshot, &graph, &deadline);
    ACTIVE.store(false, Ordering::SeqCst);
    let total = TOTAL.load(Ordering::SeqCst);
    assert!(
        result.is_err() && total < 16 * 1024 * 1024,
        "Expanded scalar bytes must be refused before loader cloning: accepted={}, allocated={total}",
        result.is_ok()
    );
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("YAML expanded scalar byte capacity exceeded")
    );
}
#[test]
fn scalar_aliases_are_admitted_before_total_clone_allocation() {
    let mut text = format!("value: &value {}\nuses:\n", "a".repeat(65_536));
    for _ in 0..256 {
        text.push_str("  - *value\n");
    }
    refuses_before_expansion(text);
}
#[test]
fn container_aliases_include_nested_scalar_bytes() {
    let mut text = format!(
        "value: &value {}\npack: &pack [*value, *value, *value, *value]\nuses:\n",
        "a".repeat(65_536)
    );
    for _ in 0..64 {
        text.push_str("  - *pack\n");
    }
    refuses_before_expansion(text);
}
#[test]
fn expanded_scalar_byte_budget_is_shared_across_documents() {
    let mut text = String::new();
    for _ in 0..4 {
        text.push_str(&format!(
            "---\nvalue: &value {}\nuses:\n",
            "a".repeat(65_536)
        ));
        for _ in 0..40 {
            text.push_str("  - *value\n");
        }
    }
    refuses_before_expansion(text);
}

#[test]
fn retained_nested_anchor_copies_are_charged_without_aliases() {
    let mut text = String::from("value: ");
    for depth in 0..40 {
        text.push_str(&format!("&a{depth} ["));
    }
    text.push_str(&"a".repeat(262_144));
    text.push_str(&"]".repeat(40));
    text.push('\n');
    refuses_before_expansion(text);
}

#[test]
fn small_unicode_aliases_and_independent_documents_remain_supported() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("config.yaml"),
        "value: &word 'Grüße 世界'\ncopy: *word\npack: &pack [*word, *word]\ncopyPack: *pack\n---\nvalue: &word 'Καλημέρα'\ncopy: *word\n").unwrap();
    let mut engine = Engine::open(root.path()).unwrap();
    let deadline = Deadline::new(30_000);
    let (snapshot, graph, _, _) = engine.index(&deadline).unwrap();
    let first = config_graph::extract(&engine.store, &snapshot, &graph, &deadline).unwrap();
    let again = config_graph::extract(&engine.store, &snapshot, &graph, &deadline).unwrap();
    assert_eq!(
        serde_json::to_value(&first).unwrap(),
        serde_json::to_value(&again).unwrap()
    );
    assert!(first.nodes.iter().any(|node| {
        node["name"]
            .as_str()
            .is_some_and(|name| name.ends_with("copyPack"))
    }));
}
