use std::fs;
use tempfile::TempDir;
use yoke::{Engine, analysis::AnalyzerEpoch, capture::Deadline, freshness};

#[test]
fn freshness_reuses_the_proven_disk_epoch_and_does_not_publish_new_source() {
    let root = TempDir::new().unwrap();
    fs::write(root.path().join("main.rs"), "fn before() {}\n").unwrap();
    let mut engine = Engine::open(root.path()).unwrap();
    engine.capture_epoch = AnalyzerEpoch::V9;
    let d = Deadline::new(30_000);
    let (base, _, _, _) = engine.index(&d).unwrap();
    let same = freshness::inspect(&engine, &d).unwrap();
    assert_eq!(same["fresh"], true);
    assert_eq!(same["observedCaptureId"], base.id);
    fs::write(root.path().join("main.rs"), "fn after() {}\n").unwrap();
    let changed = freshness::inspect(&engine, &d).unwrap();
    assert_eq!(changed["fresh"], false);
    assert_eq!(changed["baselineSnapshotId"], base.id);
    assert_eq!(changed["indexPublished"], false);
    assert_eq!(engine.store.latest().unwrap().unwrap(), base.id);
    assert!(
        engine
            .store
            .snapshot(changed["observedCaptureId"].as_str().unwrap())
            .is_err()
    );
}

#[test]
fn freshness_refuses_a_new_host_epoch_until_it_is_explicitly_indexed() {
    let root = TempDir::new().unwrap();
    fs::write(root.path().join("main.rs"), "fn same() {}\n").unwrap();
    let mut engine = Engine::open(root.path()).unwrap();
    let d = Deadline::new(30_000);
    let (base, _, _, _) = engine.index(&d).unwrap();
    engine.capture_epoch = AnalyzerEpoch::V9;
    assert!(freshness::inspect(&engine, &d).is_err());
    assert_eq!(engine.store.latest().unwrap().unwrap(), base.id);
    let (new, _, _, _) = engine.index(&d).unwrap();
    assert_ne!(base.id, new.id);
    assert_eq!(
        freshness::inspect(&engine, &d).unwrap()["observedCaptureId"],
        new.id
    );
}
