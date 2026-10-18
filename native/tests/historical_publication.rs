use std::fs;
use tempfile::TempDir;
use yoke::{
    Engine,
    capture::{CaptureOptions, Deadline},
};
#[test]
fn historical_publication_is_atomic_without_repointing_latest_or_disk_checkpoint() {
    let root = TempDir::new().unwrap();
    fs::write(root.path().join("main.rs"), "fn before() {}\n").unwrap();
    let mut engine = Engine::open(root.path()).unwrap();
    let deadline = Deadline::new(20000);
    let (before, _, _, _) = engine.index(&deadline).unwrap();
    fs::write(root.path().join("main.rs"), "fn after() {}\n").unwrap();
    let after = yoke::capture::capture_with_options(
        root.path(),
        &engine.store,
        &CaptureOptions::default(),
        &deadline,
    )
    .unwrap();
    let mut extracted = Vec::new();
    for file in &after.files {
        let (text, _) = yoke::query::source(&engine.store, &after, &file.path).unwrap();
        extracted.push(yoke::analysis::extract(file, &text, &deadline).unwrap());
    }
    let graph = yoke::analysis::assemble(&after, extracted, &deadline).unwrap();
    engine.store.publish_historical(&after, &graph).unwrap();
    assert_eq!(engine.store.latest().unwrap(), Some(before.id.clone()));
    let checkpoint: String = engine
        .store
        .db
        .query_row(
            "SELECT value FROM meta WHERE key=?1",
            [yoke::freshness::BASELINE_KEY],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(checkpoint, before.id);
    assert_eq!(engine.store.snapshot(&after.id).unwrap().id, after.id);
    assert_eq!(
        engine.store.graph(&after.id).unwrap().entities.len(),
        graph.entities.len()
    );
}
