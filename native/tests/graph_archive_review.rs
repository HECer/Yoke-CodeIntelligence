use yoke::{Engine, capture::Deadline, graph_collections::Registry};
static TEST_GUARD: std::sync::Mutex<()> = std::sync::Mutex::new(());
#[derive(serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct Input {
    artifact_id: String,
    payload: String,
}
#[derive(serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct Archive {
    view: yoke::graph_view::View,
    canonical_graph: yoke::model::Graph,
    inputs: Vec<Input>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    capture_scope: Option<serde_json::Value>,
}
#[derive(serde::Serialize, serde::Deserialize)]
struct Registration {
    name: String,
    snapshot: yoke::model::Snapshot,
    graph: yoke::model::Graph,
    archive: Archive,
}

#[test]
fn resealed_archives_cannot_reclassify_a_captured_file_as_protected_source() {
    let _guard = TEST_GUARD
        .lock()
        .unwrap_or_else(|poison| poison.into_inner());
    use yoke::{
        graph_view,
        model::{hash, identity},
    };
    let central = tempfile::tempdir().unwrap();
    let project = tempfile::tempdir().unwrap();
    std::fs::write(project.path().join("main.rs"), "fn original() {}\n").unwrap();
    let deadline = Deadline::new(20000);
    let mut engine = Engine::open(project.path()).unwrap();
    let (snapshot, graph, _, _) = engine.index(&deadline).unwrap();
    let view = graph_view::build(
        &engine.store,
        &snapshot,
        &graph,
        &graph_view::BuildOptions {
            structural_kinds: vec![],
            ..Default::default()
        },
        &deadline,
    )
    .unwrap();
    let registry = Registry::open(central.path()).unwrap();
    registry
        .register_view("project", &engine.store, &view, &deadline)
        .unwrap();
    let db = rusqlite::Connection::open(central.path().join("yoke-collection.sqlite")).unwrap();
    let payload: String = db
        .query_row(
            "SELECT payload FROM registrations WHERE name='project'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    let mut data: Registration = serde_json::from_str(&payload.replace("main.rs", ".env")).unwrap();
    data.archive.view.base_graph_hash = identity(&data.archive.canonical_graph);
    let mut sealed = data.archive.view.clone();
    sealed.id.clear();
    sealed.artifact_id.clear();
    data.archive.view.id = identity(&sealed);
    sealed.id = data.archive.view.id.clone();
    data.archive.view.artifact_id = hash(serde_json::to_vec(&sealed).unwrap());
    db.execute(
        "UPDATE registrations SET id=?1,payload=?2 WHERE name='project'",
        rusqlite::params![identity(&data), serde_json::to_string(&data).unwrap()],
    )
    .unwrap();
    assert!(
        registry.join(&["project".into()], &deadline).is_err(),
        "Resealed archive admitted protected source path"
    );
}

#[test]
fn collection_reregistration_caps_existing_source_sql_cells_before_length_reads() {
    let _guard = TEST_GUARD
        .lock()
        .unwrap_or_else(|poison| poison.into_inner());
    let central = tempfile::tempdir().unwrap();
    let project = tempfile::tempdir().unwrap();
    std::fs::write(project.path().join("main.rs"), "fn original() {}\n").unwrap();
    let deadline = Deadline::new(20000);
    let mut engine = Engine::open(project.path()).unwrap();
    let (snapshot, graph, _, _) = engine.index(&deadline).unwrap();
    let registry = Registry::open(central.path()).unwrap();
    registry
        .register("project", &engine.store, &snapshot, &graph, &deadline)
        .unwrap();
    let db = rusqlite::Connection::open(central.path().join("yoke-collection.sqlite")).unwrap();
    db.execute(
        "UPDATE source_blobs SET bytes=CAST(zeroblob(64000000) AS TEXT)",
        [],
    )
    .unwrap();
    let baseline = unsafe { rusqlite::ffi::sqlite3_memory_used() };
    unsafe {
        rusqlite::ffi::sqlite3_memory_highwater(1);
    }
    let result = registry.register("project", &engine.store, &snapshot, &graph, &deadline);
    let peak = unsafe { rusqlite::ffi::sqlite3_memory_highwater(0) };
    assert!(result.is_err());
    assert!(
        peak - baseline < 8_000_000,
        "Corrupt archive cell allocated {} SQLite bytes before admission",
        peak - baseline
    );
}
