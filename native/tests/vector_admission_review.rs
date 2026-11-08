//! Exact captured source and preallocation admission for vector publication.
use serde_json::{Value, json};
use std::{
    alloc::{GlobalAlloc, Layout, System},
    fs,
    sync::{
        Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
};
use tempfile::TempDir;
use yoke::{
    Engine,
    capture::Deadline,
    knowledge,
    model::{Graph, Snapshot},
};
struct Observe;
static ACTIVE: AtomicBool = AtomicBool::new(false);
static LARGEST: AtomicUsize = AtomicUsize::new(0);
static GUARD: Mutex<()> = Mutex::new(());
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
fn fixture(dependency: bool) -> (TempDir, Engine, Snapshot, Graph, Value) {
    let root = TempDir::new().unwrap();
    let path = if dependency {
        "node_modules/pkg/index.rs"
    } else {
        "main.rs"
    };
    fs::create_dir_all(root.path().join(path).parent().unwrap()).unwrap();
    fs::write(root.path().join(path), "pub fn vectorTarget() {}\n").unwrap();
    let mut engine = Engine::open(root.path()).unwrap();
    if dependency {
        engine.capture_options =
            serde_json::from_value(json!({"readOnlyDependencyDirs":["node_modules"]})).unwrap();
    }
    let (snapshot, graph, _, _) = engine.index(&Deadline::new(20000)).unwrap();
    let entity = graph
        .entities
        .iter()
        .find(|e| e.name == "vectorTarget")
        .unwrap();
    let file = snapshot.files.iter().find(|f| f.path == path).unwrap();
    let input = json!({"snapshotId":snapshot.id,"model":"review-model","items":[{"entityId":entity.id,"contentHash":file.hash,"vector":[3.0,4.0]}]});
    (root, engine, snapshot, graph, input)
}
#[test]
fn explicit_readonly_dependency_vectors_import_and_search_without_write_authority() {
    let _guard = GUARD.lock().unwrap_or_else(|p| p.into_inner());
    let (_root, engine, snapshot, graph, input) = fixture(true);
    let deadline = Deadline::new(10000);
    knowledge::import_vectors(&engine.store, &snapshot, &graph, input, &deadline).unwrap();
    let found = knowledge::search_vectors(
        &engine.store,
        &snapshot,
        &graph,
        "review-model",
        &[3.0, 4.0],
        5,
        &deadline,
    )
    .unwrap();
    assert_eq!(
        found["items"][0]["entity"]["path"],
        "node_modules/pkg/index.rs"
    );
    let local =
        knowledge::build_local_vectors(&engine.store, &snapshot, &graph, &deadline).unwrap();
    let found = knowledge::search_local_vectors(
        &engine.store,
        &snapshot,
        &graph,
        local["model"].as_str().unwrap(),
        "vector target",
        5,
        &deadline,
    )
    .unwrap();
    assert_eq!(
        found["items"][0]["entity"]["path"],
        "node_modules/pkg/index.rs"
    );
    assert!(yoke::paths::normalized("node_modules/pkg/index.rs").is_err());
}
#[test]
fn corrupt_source_blob_is_size_admitted_before_vector_reader_allocation() {
    let _guard = GUARD.lock().unwrap_or_else(|p| p.into_inner());
    let (_root, engine, snapshot, graph, input) = fixture(false);
    let path = engine.store.dir.join("blobs").join(&snapshot.files[0].hash);
    fs::OpenOptions::new()
        .write(true)
        .open(path)
        .unwrap()
        .set_len(16_000_000)
        .unwrap();
    LARGEST.store(0, Ordering::SeqCst);
    ACTIVE.store(true, Ordering::SeqCst);
    let result = knowledge::import_vectors(
        &engine.store,
        &snapshot,
        &graph,
        input,
        &Deadline::new(10000),
    );
    ACTIVE.store(false, Ordering::SeqCst);
    assert!(result.is_err());
    let largest = LARGEST.load(Ordering::SeqCst);
    assert!(
        largest <= 256_000,
        "Corrupt tiny captured source allocated {largest} bytes before rejection"
    );
}
#[test]
fn oversized_escaped_import_is_counted_before_allocating_encoded_json() {
    let _guard = GUARD.lock().unwrap_or_else(|p| p.into_inner());
    let (_root, engine, snapshot, graph, mut input) = fixture(false);
    input["unknown"] = Value::String("\u{1}".repeat(5_000_000));
    LARGEST.store(0, Ordering::SeqCst);
    ACTIVE.store(true, Ordering::SeqCst);
    let result = knowledge::import_vectors(
        &engine.store,
        &snapshot,
        &graph,
        input,
        &Deadline::new(10000),
    );
    ACTIVE.store(false, Ordering::SeqCst);
    assert!(result.is_err());
    let largest = LARGEST.load(Ordering::SeqCst);
    assert!(
        largest <= 256_000,
        "Rejected vector import allocated {largest} bytes before24MiB admission"
    );
}

#[test]
fn prepared_vector_admission_rejects_unpublishable_plans_before_vectors_exist() {
    let _guard = GUARD.lock().unwrap_or_else(|p| p.into_inner());
    let (_root, engine, snapshot, graph, input) = fixture(false);
    let ids = vec![input["items"][0]["entityId"].as_str().unwrap().to_owned()];
    let deadline = Deadline::new(10000);
    for (selected, dimensions, maximum) in [
        (ids.clone(), 0, 24 * 1024 * 1024),
        (ids.clone(), 4097, 24 * 1024 * 1024),
        (vec![], 2, 24 * 1024 * 1024),
        (vec![ids[0].clone(), ids[0].clone()], 2, 24 * 1024 * 1024),
        (vec!["a".repeat(64)], 2, 24 * 1024 * 1024),
        (ids.clone(), 4096, 4096),
    ] {
        assert!(
            knowledge::PreparedVectorAdmission::new(
                &engine.store,
                &snapshot,
                &graph,
                "batch-model",
                &selected,
                dimensions,
                maximum,
                &deadline
            )
            .is_err()
        );
    }
    let rows: usize = engine
        .store
        .db
        .query_row(
            "SELECT count(*) FROM records WHERE kind='native_vectors_v1'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(rows, 0);
    knowledge::import_vectors(&engine.store, &snapshot, &graph, input, &deadline).unwrap();
    assert!(
        knowledge::PreparedVectorAdmission::new(
            &engine.store,
            &snapshot,
            &graph,
            "review-model",
            &ids,
            2,
            24 * 1024 * 1024,
            &deadline
        )
        .is_err()
    );
}

#[test]
fn prepared_collection_preserves_legacy_vector_identity_and_serialized_bytes() {
    let _guard = GUARD.lock().unwrap_or_else(|p| p.into_inner());
    let (_root, engine, snapshot, graph, input) = fixture(true);
    let ids = vec![input["items"][0]["entityId"].as_str().unwrap().to_owned()];
    let deadline = Deadline::new(10000);
    let admitted = knowledge::PreparedVectorAdmission::new(
        &engine.store,
        &snapshot,
        &graph,
        "review-model",
        &ids,
        2,
        24 * 1024 * 1024,
        &deadline,
    )
    .unwrap();
    let prepared = admitted.prepare(input.clone(), &deadline).unwrap();
    assert_eq!(prepared.dimension(), 2);
    assert_eq!(prepared.count(), 1);
    let id = yoke::model::identity(&(snapshot.id.as_str(), "review-model"));
    assert_eq!(prepared.collection_id(), id);
    let rows: usize = engine
        .store
        .db
        .query_row(
            "SELECT count(*) FROM records WHERE kind='native_vectors_v1'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(rows, 0);
    knowledge::import_vectors(&engine.store, &snapshot, &graph, input, &deadline).unwrap();
    let raw: String = engine
        .store
        .db
        .query_row(
            "SELECT value FROM records WHERE kind='native_vectors_v1' AND id=?1",
            [id],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(
        prepared.serialized_hash(),
        yoke::model::hash(raw.as_bytes())
    );
    let parsed: Value = serde_json::from_str(&raw).unwrap();
    assert_eq!(parsed["items"][0]["vector"], json!([0.6, 0.8]));
}

#[test]
fn prepared_vector_admission_checks_global_capacity_without_publishing_a_record() {
    let _guard = GUARD.lock().unwrap_or_else(|p| p.into_inner());
    let (_root, engine, snapshot, graph, input) = fixture(false);
    let ids = vec![input["items"][0]["entityId"].as_str().unwrap().to_owned()];
    for i in 0..512 {
        engine
            .store
            .db
            .execute(
                "INSERT INTO records(kind,id,value) VALUES('native_vectors_v1',?1,'{}')",
                [yoke::model::hash(format!("existing-{i}"))],
            )
            .unwrap();
    }
    assert!(
        knowledge::PreparedVectorAdmission::new(
            &engine.store,
            &snapshot,
            &graph,
            "batch-model",
            &ids,
            2,
            24 * 1024 * 1024,
            &Deadline::new(10000)
        )
        .is_err()
    );
    let rows: usize = engine
        .store
        .db
        .query_row(
            "SELECT count(*) FROM records WHERE kind='native_vectors_v1'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(rows, 512);
}

#[test]
fn prepared_vectors_reject_changed_source_model_count_dimensions_and_zero_norm() {
    let _guard = GUARD.lock().unwrap_or_else(|p| p.into_inner());
    let (_root, engine, snapshot, graph, input) = fixture(false);
    let ids = vec![input["items"][0]["entityId"].as_str().unwrap().to_owned()];
    for mutation in ["model", "hash", "count", "dimension", "zero"] {
        let admitted = knowledge::PreparedVectorAdmission::new(
            &engine.store,
            &snapshot,
            &graph,
            "review-model",
            &ids,
            2,
            24 * 1024 * 1024,
            &Deadline::new(10000),
        )
        .unwrap();
        let mut bad = input.clone();
        match mutation {
            "model" => bad["model"] = json!("other"),
            "hash" => bad["items"][0]["contentHash"] = json!("a".repeat(64)),
            "count" => bad["items"] = json!([]),
            "dimension" => bad["items"][0]["vector"] = json!([1.0]),
            "zero" => bad["items"][0]["vector"] = json!([0.0, 0.0]),
            _ => unreachable!(),
        }
        assert!(
            admitted.prepare(bad, &Deadline::new(10000)).is_err(),
            "accepted{mutation}"
        );
    }
}

#[test]
fn oversized_persisted_vector_cell_is_bounded_before_string_copy() {
    let _guard = GUARD.lock().unwrap_or_else(|p| p.into_inner());
    let (_root, engine, snapshot, graph, input) = fixture(false);
    let deadline = Deadline::new(10000);
    knowledge::import_vectors(&engine.store, &snapshot, &graph, input, &deadline).unwrap();
    engine.store.db.execute("UPDATE records SET value=CAST(zeroblob(32000000) AS TEXT) WHERE kind='native_vectors_v1'",[]).unwrap();
    let sqlite_baseline = unsafe { rusqlite::ffi::sqlite3_memory_used() };
    unsafe {
        rusqlite::ffi::sqlite3_memory_highwater(1);
    }
    LARGEST.store(0, Ordering::SeqCst);
    ACTIVE.store(true, Ordering::SeqCst);
    let result = knowledge::search_vectors(
        &engine.store,
        &snapshot,
        &graph,
        "review-model",
        &[1.0, 0.0],
        5,
        &deadline,
    );
    ACTIVE.store(false, Ordering::SeqCst);
    let sqlite_peak = unsafe { rusqlite::ffi::sqlite3_memory_highwater(0) };
    assert!(result.is_err());
    let largest = LARGEST.load(Ordering::SeqCst);
    assert!(
        largest <= 256000,
        "Oversized vector record allocated{largest} bytes before24MiB admission"
    );
    assert!(
        sqlite_peak - sqlite_baseline < 28_000_000,
        "Oversized vector cell materialized{} SQLitebytes",
        sqlite_peak - sqlite_baseline
    );
}
