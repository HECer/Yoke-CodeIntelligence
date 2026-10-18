//! Adversarial API admission of a caller manifest against a warm native extraction cache.
use std::fs;
use yoke::{Engine, analysis::AnalyzerEpoch, capture::Deadline, model::identity};

#[test]
fn cached_extraction_cannot_publish_past_the_exact_declared_source_size() {
    let root = tempfile::tempdir().unwrap();
    let text = "pub fn source_bound() {}\n";
    fs::write(root.path().join("main.rs"), text).unwrap();
    let mut engine = Engine::open(root.path()).unwrap();
    let epoch = AnalyzerEpoch::active().unwrap();
    let deadline = Deadline::new(10000);
    let (mut snapshot, _, parsed, _) = engine.index(&deadline).unwrap();
    assert_eq!(parsed, 1);
    let key = epoch.cache_key(&snapshot.files[0]);
    assert!(engine.store.cache(&key).unwrap().is_some());
    snapshot.files[0].size = 1;
    snapshot.id = identity(&(
        &snapshot.workspace_id,
        "disk",
        epoch.producer(),
        &snapshot.files,
        &snapshot.omissions,
    ));
    assert_eq!(epoch.cache_key(&snapshot.files[0]), key);
    let id = snapshot.id.clone();
    let result = engine.analyze_snapshot(snapshot, &deadline);
    assert!(
        result.is_err(),
        "Warm extraction cache must not publish graph spans past a one-byte manifest; result={result:?}"
    );
    assert!(engine.store.graph(&id).is_err());
}
