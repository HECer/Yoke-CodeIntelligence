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
    engine: Engine,
    plan: Plan,
    path: PathBuf,
    root: tempfile::TempDir,
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

#[test]
fn released_epoch_plan_ids_review_bytes_apply_and_recovery_remain_compatible() {
    for epoch in [AnalyzerEpoch::V8, AnalyzerEpoch::V9] {
        let f = fixture("src/café.rs", epoch);
        let serialized = serde_json::to_vec(&f.plan).unwrap();
        assert_eq!(
            f.plan.id,
            yoke::model::identity(&(
                &f.plan.base_snapshot_id,
                &f.plan.result_snapshot_id,
                &f.plan.files,
                f.plan.created_at
            ))
        );
        let checked = changes::load_checked_plan(&f.engine.store, &f.plan.id, &d()).unwrap();
        assert_eq!(serde_json::to_vec(&checked).unwrap(), serialized);
        assert!(checked.diff.contains("café 🦀"));
        host_prerequisite_fixture(&f);
        let receipt = apply(&f, "valid-released-plan").unwrap();
        assert_eq!(fs::read(&f.path).unwrap(), REVIEWED.as_bytes());
        assert_eq!(apply(&f, "valid-released-plan").unwrap().id, receipt.id);
        changes::recover(&f.engine.store, f.root.path(), &receipt.id, true).unwrap();
        assert_eq!(fs::read(&f.path).unwrap(), BEFORE.as_bytes());
    }
}

#[test]
fn ambiguous_unknown_and_oversized_saved_json_cannot_publish_approval() {
    let f = fixture("src/ordinary.rs", AnalyzerEpoch::V9);
    let valid = serde_json::to_string(&f.plan).unwrap();
    for raw in [
        format!("{{\"id\":\"{}\",{}", f.plan.id, &valid[1..]),
        format!("{{\"hostAuthority\":true,{}", &valid[1..]),
        valid.replacen("\"mode\":", "\"unknown\":true,\"mode\":", 1),
        format!("{valid} false"),
    ] {
        f.engine
            .store
            .db
            .execute(
                "UPDATE records SET value=?1 WHERE kind='plan' AND id=?2",
                rusqlite::params![raw, f.plan.id],
            )
            .unwrap();
        assert!(changes::approve(&f.engine.store, &f.plan.id, true).is_err());
        assert_eq!(count(&f, "approval"), 0);
    }
    f.engine
        .store
        .db
        .execute(
            "UPDATE records SET value=printf('%.*c',64000001,'x') WHERE kind='plan' AND id=?1",
            [&f.plan.id],
        )
        .unwrap();
    assert_eq!(
        changes::load_checked_plan(&f.engine.store, &f.plan.id, &d())
            .err()
            .unwrap()
            .to_string(),
        "changes_plan_capacity_exceeded"
    );
    assert_eq!(count(&f, "approval"), 0);
    assert_eq!(fs::read(&f.path).unwrap(), BEFORE.as_bytes());
}

#[test]
fn captured_result_metadata_and_complete_file_delta_are_replayed() {
    let f = fixture("src/ordinary.rs", AnalyzerEpoch::V9);
    let result = f.engine.store.snapshot(&f.plan.result_snapshot_id).unwrap();
    for mutation in ["size", "mode", "omissions", "path"] {
        let mut changed = result.clone();
        match mutation {
            "size" => changed.files[0].size += 1,
            "mode" => changed.files[0].mode ^= 0o111,
            "omissions" => changed.omissions.push(yoke::model::Omission {
                path: "outside.rs".into(),
                reason: "injected".into(),
            }),
            _ => changed.files[0].path = "../outside.rs".into(),
        }
        f.engine
            .store
            .db
            .execute(
                "UPDATE snapshots SET value=?1 WHERE id=?2",
                rusqlite::params![serde_json::to_string(&changed).unwrap(), result.id],
            )
            .unwrap();
        assert!(
            changes::load_checked_plan(&f.engine.store, &f.plan.id, &d()).is_err(),
            "{mutation}"
        );
    }
    f.engine
        .store
        .db
        .execute(
            "UPDATE snapshots SET value=?1 WHERE id=?2",
            rusqlite::params![serde_json::to_string(&result).unwrap(), result.id],
        )
        .unwrap();
    // Recomputing the legacy ID alone cannot disguise a delta omitted from files.
    let mut changed = f.plan.clone();
    changed.files.clear();
    changed.id = yoke::model::identity(&(
        &changed.base_snapshot_id,
        &changed.result_snapshot_id,
        &changed.files,
        changed.created_at,
    ));
    f.engine
        .store
        .record("plan", &changed.id, &changed)
        .unwrap();
    assert!(changes::load_checked_plan(&f.engine.store, &changed.id, &d()).is_err());
    assert!(changes::load_checked_plan(&f.engine.store, &f.plan.id, &d()).is_ok());
    assert_eq!(fs::read(&f.path).unwrap(), BEFORE.as_bytes());
}

#[test]
fn plan_replay_restores_sql_policy_and_preserves_the_callers_transaction() {
    use rusqlite::limits::Limit;
    let f = fixture("src/ordinary.rs", AnalyzerEpoch::V8);
    let db = &f.engine.store.db;
    db.busy_timeout(std::time::Duration::from_millis(137))
        .unwrap();
    db.set_limit(Limit::SQLITE_LIMIT_LENGTH, 8_000_000).unwrap();
    db.execute_batch(
        "BEGIN; INSERT INTO records(kind,id,value) VALUES('owned-fixture','pending','{}')",
    )
    .unwrap();
    let changes = db.total_changes();
    for id in [&f.plan.id, &"7".repeat(64)] {
        let _ = changes::load_checked_plan(&f.engine.store, id, &d());
        assert!(!db.is_autocommit());
        assert_eq!(db.total_changes(), changes);
        assert_eq!(db.limit(Limit::SQLITE_LIMIT_LENGTH).unwrap(), 8_000_000);
        assert_eq!(
            db.query_row("PRAGMA busy_timeout", [], |r| r.get::<_, u64>(0))
                .unwrap(),
            137
        );
    }
    assert!(changes::load_checked_plan(&f.engine.store, &f.plan.id, &d()).is_ok());
    db.execute_batch("ROLLBACK").unwrap();
    assert_eq!(
        db.query_row(
            "SELECT COUNT(*) FROM records WHERE kind='owned-fixture'",
            [],
            |r| r.get::<_, u64>(0)
        )
        .unwrap(),
        0
    );
    assert!(changes::load_checked_plan(&f.engine.store, &f.plan.id, &d()).is_ok());
    assert!(db.is_autocommit());
}

#[test]
fn expired_or_cancelled_host_requests_cannot_be_extended_by_plan_replay() {
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };
    let f = fixture("src/ordinary.rs", AnalyzerEpoch::V9);
    assert!(changes::approve_at(&f.engine.store, &f.plan.id, false, &Deadline::new(0)).is_err());
    assert_eq!(count(&f, "approval"), 0);
    for cancel_prior in [false, true] {
        let prior_flag = Arc::new(AtomicBool::new(false));
        let prior = Deadline::run_bounded(&d(), Arc::clone(&prior_flag), || Ok(d())).unwrap();
        let request_flag = Arc::new(AtomicBool::new(false));
        let request = Deadline::run_bounded(&d(), Arc::clone(&request_flag), || Ok(d())).unwrap();
        f.engine.store.set_deadline(&prior).unwrap();
        if cancel_prior {
            prior_flag.store(true, Ordering::SeqCst);
        } else {
            request_flag.store(true, Ordering::SeqCst);
        }
        assert!(changes::approve_at(&f.engine.store, &f.plan.id, false, &request).is_err());
        if cancel_prior {
            assert!(
                f.engine
                    .store
                    .record("owned-fixture", "should-not-write", &json!({}))
                    .is_err()
            );
        } else {
            f.engine
                .store
                .record("owned-fixture", "prior-restored", &json!({}))
                .unwrap();
        }
        f.engine.store.clear_deadline();
        assert_eq!(count(&f, "approval"), 0);
    }
    assert!(changes::approve_at(&f.engine.store, &f.plan.id, false, &d()).is_ok());
}

#[test]
fn invalid_plan_is_rejected_before_an_explicit_owned_verifier_can_start() {
    let f = fixture("src/ordinary.rs", AnalyzerEpoch::V9);
    let marker = f.root.path().join("verifier-started");
    let mut changed = f.plan.clone();
    changed.diff.clear();
    f.engine.store.record("plan", &f.plan.id, &changed).unwrap();
    let command = format!(
        "require('fs').writeFileSync({},'started')",
        serde_json::to_string(&marker).unwrap()
    );
    assert!(
        yoke::verify::execute(
            &f.engine,
            &f.plan.id,
            "node",
            &["-e".into(), command],
            true,
            &d()
        )
        .is_err()
    );
    assert!(!marker.exists());
    assert_eq!(count(&f, "verification"), 0);
    assert!(changes::verify_syntax(&f.engine.store, &f.plan).is_err());
    assert_eq!(count(&f, "verification"), 0);
}

#[test]
fn identity_replay_and_expiry_admission_remain_separate() {
    let f = fixture("src/ordinary.rs", AnalyzerEpoch::V8);
    let mut expired = f.plan.clone();
    expired.created_at -= 100_000;
    expired.expires_at = expired.created_at + 86_400;
    expired.id = yoke::model::identity(&(
        &expired.base_snapshot_id,
        &expired.result_snapshot_id,
        &expired.files,
        expired.created_at,
    ));
    f.engine
        .store
        .record("plan", &expired.id, &expired)
        .unwrap();
    // Recovery may need a valid historical plan after its approval window.
    assert!(changes::load_checked_plan(&f.engine.store, &expired.id, &d()).is_ok());
    assert!(changes::approve(&f.engine.store, &expired.id, false).is_err());
    assert_eq!(count(&f, "approval"), 0);
}

#[test]
fn preparation_refuses_escaped_plan_overflow_before_publishing_a_derived_result() {
    let mut f = fixture("src/ordinary.rs", AnalyzerEpoch::V9);
    let (snapshot, graph, _, _) = f.engine.index(&d()).unwrap();
    let snapshots: u64 = f
        .engine
        .store
        .db
        .query_row("SELECT COUNT(*) FROM snapshots", [], |r| r.get(0))
        .unwrap();
    let plans: u64 = f
        .engine
        .store
        .db
        .query_row("SELECT COUNT(*) FROM records WHERE kind='plan'", [], |r| {
            r.get(0)
        })
        .unwrap();
    // Four admitted <=4MB source files produce a <=16MB diff, but JSON escaping
    // makes the saved review exceed its independent 64MB reader contract.
    let text = "\u{1}".repeat(3_000_000);
    let edits: Vec<_> = (0..4)
        .map(|i| json!({"kind":"create","path":format!("owned-{i}.txt"),"text":text}))
        .collect();
    let error = changes::prepare(
        &mut f.engine.store,
        &snapshot,
        &graph,
        &json!({"edits":edits}),
        &d(),
    )
    .err()
    .unwrap();
    assert_eq!(error.to_string(), "changes_plan_capacity_exceeded");
    assert_eq!(
        f.engine
            .store
            .db
            .query_row("SELECT COUNT(*) FROM snapshots", [], |r| r.get::<_, u64>(0))
            .unwrap(),
        snapshots
    );
    assert_eq!(
        f.engine
            .store
            .db
            .query_row("SELECT COUNT(*) FROM records WHERE kind='plan'", [], |r| {
                r.get::<_, u64>(0)
            })
            .unwrap(),
        plans
    );
    assert_eq!(fs::read(&f.path).unwrap(), BEFORE.as_bytes());
}
