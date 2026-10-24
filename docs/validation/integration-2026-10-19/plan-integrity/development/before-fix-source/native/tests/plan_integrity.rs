//! Owned fault injection into saved plans; approval prerequisites are host fixtures.
//! No external verifier execution is claimed by the prerequisite records.
use serde_json::json;
use std::{fs, path::PathBuf};
use yoke::{
    Engine,
    analysis::AnalyzerEpoch,
    capture::Deadline,
    changes::{self, Plan},
    model::hash,
};

const BEFORE: &str = "pub fn original(){ /* café 🦀 */ }\r\n";
const REVIEWED: &str = "pub fn reviewed(){ /* café 🦀 */ }\r\n";
const OTHER: &str = "pub fn outside_the_reviewed_effect(){}\r\n";
fn d() -> Deadline {
    Deadline::new(30_000)
}
struct Fixture {
    root: tempfile::TempDir,
    engine: Engine,
    plan: Plan,
    path: PathBuf,
}
fn fixture(name: &str, epoch: AnalyzerEpoch) -> Fixture {
    let root = tempfile::tempdir().unwrap();
    fs::create_dir(root.path().join("src")).unwrap();
    let path = root.path().join(name);
    fs::write(&path, BEFORE).unwrap();
    let mut engine = Engine::open(root.path()).unwrap();
    engine.capture_epoch = epoch;
    let (s, g, _, _) = engine.index(&d()).unwrap();
    let plan = changes::prepare(
        &mut engine.store,
        &s,
        &g,
        &json!({"edits":[{"kind":"replace","path":name,"expectedHash":hash(BEFORE),
            "startByte":0,"endByte":BEFORE.len(),"text":REVIEWED}]}),
        &d(),
    )
    .unwrap();
    Fixture {
        root,
        engine,
        plan,
        path,
    }
}
fn host_prerequisite_fixture(f: &Fixture) {
    changes::approve(&f.engine.store, &f.plan.id, true).unwrap();
    f.engine.store.record("verification",&f.plan.id,&json!({
        "status":"passed","level":"executed","snapshotId":f.plan.result_snapshot_id,
        "planId":f.plan.id,
        "fixtureProvenance":"Explicit host prerequisite fixture; no compiler execution is asserted"
    })).unwrap();
}
fn apply(f: &Fixture, key: &str) -> anyhow::Result<changes::Receipt> {
    changes::apply(
        &f.engine.store,
        f.root.path(),
        &json!({"planId":f.plan.id,"idempotencyKey":key,"mode":"workspace"}),
        true,
        &d(),
    )
}
fn count(f: &Fixture, kind: &str) -> usize {
    f.engine
        .store
        .db
        .query_row(
            "SELECT COUNT(*) FROM records WHERE kind=?1 AND id=?2",
            rusqlite::params![kind, f.plan.id],
            |r| r.get(0),
        )
        .unwrap()
}
#[test]
fn approved_plan_cannot_change_applied_bytes_after_its_record_is_modified() {
    for epoch in [AnalyzerEpoch::V8, AnalyzerEpoch::V9] {
        let f = fixture("src/café.rs", epoch);
        host_prerequisite_fixture(&f);
        let mut changed = f.plan.clone();
        changed.files[0].after_hash = Some(f.engine.store.blob(OTHER.as_bytes()).unwrap());
        f.engine.store.record("plan", &f.plan.id, &changed).unwrap();
        let result = apply(&f, "mutated-approved-effect");
        let original_unchanged = fs::read(&f.path).unwrap() == BEFORE.as_bytes();
        assert!(
            result.is_err() && original_unchanged,
            "Changed saved effect reused approval: accepted={}, original_unchanged={original_unchanged}",
            result.is_ok()
        );
    }
}
#[test]
fn clearing_saved_test_warnings_cannot_remove_the_host_test_change_gate() {
    let f = fixture("src/spec.rs", AnalyzerEpoch::V9);
    assert!(!f.plan.warnings.is_empty());
    assert!(changes::approve(&f.engine.store, &f.plan.id, false).is_err());
    let mut changed = f.plan.clone();
    changed.warnings.clear();
    f.engine.store.record("plan", &f.plan.id, &changed).unwrap();
    let result = changes::approve(&f.engine.store, &f.plan.id, false);
    assert!(
        result.is_err(),
        "Cleared warning bypassed exact test-change approval"
    );
    assert_eq!(count(&f, "approval"), 0);
    assert_eq!(fs::read(&f.path).unwrap(), BEFORE.as_bytes());
}
#[test]
fn saved_review_text_must_describe_the_exact_captured_effect_before_approval() {
    let f = fixture("src/ordinary.rs", AnalyzerEpoch::V8);
    let mut changed = f.plan.clone();
    changed.diff = "This review text no longer describes the captured change.".into();
    f.engine.store.record("plan", &f.plan.id, &changed).unwrap();
    assert!(
        changes::approve(&f.engine.store, &f.plan.id, false).is_err(),
        "Unbound review text was accepted"
    );
    assert_eq!(count(&f, "approval"), 0);
}
#[test]
fn record_key_and_fixed_expiry_cannot_be_changed_without_changing_the_plan_identity() {
    for field in ["id", "expiry"] {
        let f = fixture("src/ordinary.rs", AnalyzerEpoch::V9);
        let mut changed = f.plan.clone();
        if field == "id" {
            changed.id = "7".repeat(64);
        } else {
            changed.expires_at += 3600;
        }
        f.engine.store.record("plan", &f.plan.id, &changed).unwrap();
        assert!(
            changes::approve(&f.engine.store, &f.plan.id, false).is_err(),
            "Unbound plan {field} was accepted"
        );
        assert_eq!(count(&f, "approval"), 0);
    }
}
#[test]
fn recovery_cannot_restore_different_bytes_from_a_modified_saved_plan() {
    let f = fixture("src/ordinary.rs", AnalyzerEpoch::V8);
    host_prerequisite_fixture(&f);
    let applied = apply(&f, "recovery-plan-binding").unwrap();
    assert_eq!(fs::read(&f.path).unwrap(), REVIEWED.as_bytes());
    let mut changed = f.plan.clone();
    changed.files[0].before_hash = Some(f.engine.store.blob(OTHER.as_bytes()).unwrap());
    f.engine.store.record("plan", &f.plan.id, &changed).unwrap();
    let result = changes::recover(&f.engine.store, f.root.path(), &applied.id, true);
    let reviewed_unchanged = fs::read(&f.path).unwrap() == REVIEWED.as_bytes();
    assert!(
        result.is_err() && reviewed_unchanged,
        "Recovery consumed changed plan: accepted={}, reviewed_unchanged={reviewed_unchanged}",
        result.is_ok()
    );
}
