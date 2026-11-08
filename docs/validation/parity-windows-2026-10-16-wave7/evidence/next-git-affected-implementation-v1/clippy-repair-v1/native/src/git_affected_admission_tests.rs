//! Private admission tests for complete scopes, retained read views and caller transactions.
//! Intended as native/src/git_affected_admission_tests.rs, a cfg(test) child of
//! git_workflows::impact_details. See README for the one additional-denial seam.
use super::{
    GraphInputLimits, is_checked_input_capacity, with_checked_member_policy,
    with_checked_report_inputs,
};
use crate as test_yoke;
#[path = "../tests/support/git_impact_fixture.rs"]
mod fixture;
use crate::{
    analysis::AnalyzerEpoch,
    capture::{CaptureOptions, Deadline},
    git_workflows::{self, HostGit, WorkflowOptions},
    model::{Snapshot, identity},
    store::Store,
};
use fixture::{Fixture, state};
use rusqlite::{Connection, limits::Limit};
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    io::{self, Write},
    rc::Rc,
};

const CHANGED: &str = "src/changed.rs";
const UNREACHED: &str = "src/unreached.rs";
const SMALL: &str = "fn changed() { helper(); }\nfn helper() {}\n";
const LARGE: &str =
    "fn changed() { helper(); let n = 1; }\nfn helper() {}\nfn extra() { changed(); changed(); }\n";
const UNCHANGED: &str = "fn untouched() {}\n";

fn fixture(scoped: bool) -> Fixture {
    Fixture::new_with_options(
        &[(CHANGED, SMALL), (UNREACHED, UNCHANGED)],
        &[(CHANGED, LARGE), (UNREACHED, UNCHANGED)],
        CaptureOptions {
            only_dirs: if scoped { vec!["src".into()] } else { vec![] },
            ..Default::default()
        },
    )
}

fn limits(e: usize, o: usize, v: usize) -> GraphInputLimits {
    GraphInputLimits::new(e, o, v).unwrap()
}

fn maximum() -> GraphInputLimits {
    limits(5_000, 50_000, 100_000)
}

fn snapshot(f: &Fixture, side: &str) -> Snapshot {
    f.engine
        .store
        .snapshot(f.report[format!("{side}SnapshotId")].as_str().unwrap())
        .unwrap()
}

fn qualify(f: &Fixture) {
    with_checked_report_inputs(
        &f.engine.store,
        f.id(),
        maximum(),
        &Deadline::new(30_000),
        |inputs| {
            assert_eq!(inputs.report_artifact_id(), f.id());
            assert_eq!(inputs.workspace_id(), f.engine.store.workspace_id);
            assert_eq!(
                inputs.changed_symbols(),
                f.report["changedSymbols"].as_array().unwrap()
            );
            assert_eq!(inputs.impact(), f.report["impact"].as_array().unwrap());
            assert_eq!(
                inputs.impact_observations(),
                f.report["impactObservations"].as_array().unwrap()
            );
            for (label, side) in [("base", inputs.base()), ("head", inputs.head())] {
                assert_eq!(side.snapshot().id, f.report[format!("{label}SnapshotId")]);
                assert_eq!(side.graph_hash(), identity(side.graph()));
                for file in &side.snapshot().files {
                    assert_eq!(side.file(&file.path)?, file);
                    let bytes = side.read_source(&file.path)?;
                    assert_eq!(bytes.len(), file.size);
                    assert_eq!(crate::model::hash(&bytes), file.hash);
                }
                assert_eq!(side.read_source(UNREACHED)?, UNCHANGED.as_bytes());
            }
            Ok(())
        },
    )
    .expect("genuine fully admitted positive before every destructive tripwire");
}

fn expected_members(f: &Fixture) -> Vec<(String, String)> {
    ["base", "head"]
        .into_iter()
        .flat_map(|label| {
            snapshot(f, label)
                .files
                .into_iter()
                .map(move |file| (label.into(), file.path))
        })
        .collect()
}

fn break_first_base_source(f: &Fixture) {
    let base = snapshot(f, "base");
    let file = base.files.first().expect("genuine nonempty manifest");
    std::fs::remove_file(f.engine.store.dir.join("blobs").join(&file.hash)).unwrap();
}

fn host_git() -> HostGit {
    let executable = std::env::split_paths(&std::env::var_os("PATH").expect("host PATH"))
        .map(|p| p.join(if cfg!(windows) { "git.exe" } else { "git" }))
        .find(|p| p.is_absolute() && p.is_file())
        .expect("real host Git")
        .canonicalize()
        .unwrap();
    HostGit { executable }
}

#[test]
fn graph_input_limits_keep_exact_zero_and_reject_only_values_above_fixed_maxima() {
    assert!(GraphInputLimits::new(0, 0, 0).is_ok());
    assert!(GraphInputLimits::new(5_000, 50_000, 100_000).is_ok());
    for (e, o, v) in [(5_001, 0, 0), (0, 50_001, 0), (0, 0, 100_001)] {
        assert!(GraphInputLimits::new(e, o, v).is_err());
    }
}

fn deny_unchanged_head_before_source_load(f: &Fixture) {
    qualify(f);
    assert_eq!(f.base[UNREACHED], f.head[UNREACHED]);
    for field in ["changedSymbols", "impact"] {
        assert!(
            f.report[field]
                .as_array()
                .unwrap()
                .iter()
                .all(|row| row["path"] != UNREACHED)
        );
    }
    assert!(
        f.report["impactObservations"]
            .as_array()
            .unwrap()
            .iter()
            .all(|row| {
                row["evidence"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .all(|anchor| anchor["path"] != UNREACHED)
            })
    );
    let expected = expected_members(f);
    let seen = Rc::new(RefCell::new(Vec::new()));
    let positive_seen = Rc::clone(&seen);
    with_checked_member_policy(
        move |side, path| {
            positive_seen.borrow_mut().push((side.into(), path.into()));
            Ok(())
        },
        || qualify(f),
    );
    assert_eq!(
        *seen.borrow(),
        expected,
        "every real manifest member, once per logical side"
    );
    break_first_base_source(f);
    let before = state(&f.engine);
    seen.borrow_mut().clear();
    let denied_seen = Rc::clone(&seen);
    let consumed = Cell::new(false);
    let result: anyhow::Result<()> = with_checked_member_policy(
        move |side, path| {
            denied_seen.borrow_mut().push((side.into(), path.into()));
            if side == "head" && path == UNREACHED {
                anyhow::bail!("R5B_ADDITIONAL_HEAD_MEMBER_DENIAL");
            }
            Ok(())
        },
        || {
            with_checked_report_inputs(
                &f.engine.store,
                f.id(),
                maximum(),
                &Deadline::new(30_000),
                |_| {
                    consumed.set(true);
                    Ok(())
                },
            )
        },
    );
    assert_eq!(
        result.unwrap_err().to_string(),
        "R5B_ADDITIONAL_HEAD_MEMBER_DENIAL"
    );
    let stop = expected
        .iter()
        .position(|row| row == &("head".into(), UNREACHED.into()))
        .unwrap();
    assert_eq!(*seen.borrow(), expected[..=stop]);
    assert!(!consumed.get());
    assert_eq!(state(&f.engine), before);
}

#[test]
fn unchanged_unreached_head_additional_denial_precedes_inherited_source_load() {
    let f = fixture(true);
    assert_ne!(f.report["baseSnapshotId"], f.report["headSnapshotId"]);
    deny_unchanged_head_before_source_load(&f);
}

#[test]
fn equal_genuine_commit_snapshot_ids_keep_both_logical_side_member_obligations() {
    let mut f = Fixture::new(
        &[(CHANGED, SMALL), (UNREACHED, UNCHANGED)],
        &[(CHANGED, SMALL), (UNREACHED, UNCHANGED)],
    );
    f.report = git_workflows::analyze(
        &mut f.engine,
        &host_git(),
        &WorkflowOptions {
            base_ref: "HEAD".into(),
            head_ref: Some("HEAD".into()),
            merge_base: false,
            history_limit: 1,
            branch_limit: 0,
            ..Default::default()
        },
        &Deadline::new(30_000),
    )
    .expect("second genuine analysis of the same committed side, without fabricated publication");
    assert_eq!(f.report["headMode"], "commit");
    assert_eq!(f.report["baseSnapshotId"], f.report["headSnapshotId"]);
    assert_eq!(f.report["baseGraphHash"], f.report["headGraphHash"]);
    deny_unchanged_head_before_source_load(&f);
}

#[test]
fn real_capture_binding_corruption_refuses_before_additional_policy_and_graph_capacity() {
    let f = fixture(true);
    qualify(&f);
    let head = f.report["headSnapshotId"].as_str().unwrap();
    let mut record: Value = f
        .engine
        .store
        .get_record(crate::capture::OPTIONS_RECORD_KIND, head)
        .unwrap();
    record["optionsHash"] = json!("f".repeat(64));
    f.engine
        .store
        .db
        .execute(
            "UPDATE records SET value=?1 WHERE kind=?2 AND id=?3",
            rusqlite::params![
                record.to_string(),
                crate::capture::OPTIONS_RECORD_KIND,
                head
            ],
        )
        .unwrap();
    break_first_base_source(&f);
    let before = state(&f.engine);
    let restrictions = Rc::new(Cell::new(0));
    let observed = Rc::clone(&restrictions);
    let consumed = Cell::new(false);
    let result: anyhow::Result<()> = with_checked_member_policy(
        move |_, _| {
            observed.set(observed.get() + 1);
            Ok(())
        },
        || {
            with_checked_report_inputs(
                &f.engine.store,
                f.id(),
                limits(0, 0, 0),
                &Deadline::new(30_000),
                |_| {
                    consumed.set(true);
                    Ok(())
                },
            )
        },
    );
    let error = result.unwrap_err();
    assert!(!is_checked_input_capacity(&error));
    assert_eq!(error.to_string(), "Capture options identity mismatch");
    assert_eq!(restrictions.get(), 0);
    assert!(!consumed.get());
    // The historical base can inherit this head origin. This proves actual
    // captured binding refusal, not that a particular logical side failed first.
    assert_eq!(state(&f.engine), before);
}

#[test]
fn each_whole_graph_vector_on_either_side_is_capacity_before_broken_early_source() {
    for larger_side in ["base", "head"] {
        let (base, head) = if larger_side == "base" {
            (LARGE, SMALL)
        } else {
            (SMALL, LARGE)
        };
        let f = Fixture::new(
            &[(CHANGED, base), (UNREACHED, UNCHANGED)],
            &[(CHANGED, head), (UNREACHED, UNCHANGED)],
        );
        qualify(&f);
        let mut counts = Vec::new();
        for side in ["base", "head"] {
            let graph = f.engine.store.graph(&snapshot(&f, side).id).unwrap();
            counts.push([
                graph.entities.len(),
                graph.observations.len(),
                graph.evidence.len(),
            ]);
        }
        let large = usize::from(larger_side == "head");
        let small = 1 - large;
        for metric in 0..3 {
            assert!(
                counts[large][metric] > counts[small][metric],
                "genuine asymmetric vector {metric}"
            );
            let mut exact = [5_000, 50_000, 100_000];
            exact[metric] = counts[large][metric];
            with_checked_report_inputs(
                &f.engine.store,
                f.id(),
                limits(exact[0], exact[1], exact[2]),
                &Deadline::new(30_000),
                |_| Ok(()),
            )
            .expect("exact genuine whole-graph count is admitted");
        }
        break_first_base_source(&f);
        let before = state(&f.engine);
        for metric in 0..3 {
            let mut below = [5_000, 50_000, 100_000];
            below[metric] = counts[large][metric] - 1;
            assert!(counts[small][metric] <= below[metric]);
            let consumed = Cell::new(false);
            let result: anyhow::Result<()> = with_checked_report_inputs(
                &f.engine.store,
                f.id(),
                limits(below[0], below[1], below[2]),
                &Deadline::new(30_000),
                |_| {
                    consumed.set(true);
                    Ok(())
                },
            );
            assert!(
                is_checked_input_capacity(&result.unwrap_err()),
                "capacity must precede source failure for {larger_side}/{metric}"
            );
            assert!(!consumed.get());
            assert_eq!(state(&f.engine), before);
        }
    }
}

fn record_text(db: &Connection, id: &str) -> String {
    db.query_row(
        "SELECT value FROM records WHERE kind=?1 AND id=?2",
        rusqlite::params![git_workflows::REPORT_KIND, id],
        |row| row.get(0),
    )
    .unwrap()
}

struct EncodingProbe<'a> {
    db: &'a Connection,
    limit: i32,
    report_id: &'a str,
    original_record: &'a str,
    bytes: Vec<u8>,
    writes: usize,
    fail: bool,
}
impl EncodingProbe<'_> {
    fn check(&self) {
        assert!(
            !self.db.is_autocommit(),
            "read view remains active through encoding"
        );
        assert_eq!(
            self.db.limit(Limit::SQLITE_LIMIT_LENGTH).unwrap(),
            self.limit
        );
        assert_eq!(record_text(self.db, self.report_id), self.original_record);
    }
}
impl Write for EncodingProbe<'_> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.check();
        self.writes += 1;
        if self.fail {
            return Err(io::Error::other("R5B_CONSUMER_ENCODING_FAILURE"));
        }
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        self.check();
        Ok(())
    }
}

#[test]
fn owned_read_view_and_exact_sql_limit_survive_late_encoding_success_and_failure() {
    let f = fixture(false);
    qualify(&f);
    let writer = Store::open(f.dir.path()).unwrap();
    let db = &f.engine.store.db;
    let original_limit = db.limit(Limit::SQLITE_LIMIT_LENGTH).unwrap();
    let original_record = record_text(db, f.id());
    let mut changed_record: Value = serde_json::from_str(&original_record).unwrap();
    changed_record["workspaceId"] = json!("f".repeat(64));
    let changed_record = changed_record.to_string();
    let before = state(&f.engine);
    for prior in [128_000_000, 1_000_000] {
        db.set_limit(Limit::SQLITE_LIMIT_LENGTH, prior).unwrap();
        assert_eq!(db.limit(Limit::SQLITE_LIMIT_LENGTH).unwrap(), prior);
        for fail in [false, true] {
            let result: anyhow::Result<Vec<u8>> = with_checked_report_inputs(
                &f.engine.store,
                f.id(),
                maximum(),
                &Deadline::new(30_000),
                |inputs| {
                    assert!(!db.is_autocommit());
                    writer.db.execute(
                        "UPDATE records SET value=?1 WHERE kind=?2 AND id=?3",
                        rusqlite::params![changed_record, git_workflows::REPORT_KIND, f.id()],
                    )?;
                    assert_eq!(record_text(&writer.db, f.id()), changed_record);
                    assert_eq!(
                        record_text(db, f.id()),
                        original_record,
                        "retained SQL snapshot, not only owned graph values"
                    );
                    let value = json!({"reportArtifactId":inputs.report_artifact_id(),
                        "baseSnapshotId":inputs.base().snapshot().id,
                        "headSnapshotId":inputs.head().snapshot().id,
                        "changedSymbols":inputs.changed_symbols(),
                        "impact":inputs.impact(), "impactObservations":inputs.impact_observations()});
                    let mut output = EncodingProbe {
                        db,
                        limit: prior.min(64_004_096),
                        report_id: f.id(),
                        original_record: &original_record,
                        bytes: vec![],
                        writes: 0,
                        fail,
                    };
                    let encoded = serde_json::to_writer(&mut output, &value);
                    output.check();
                    assert!(output.writes > 0);
                    encoded?;
                    Ok(output.bytes)
                },
            );
            if fail {
                let error = result.unwrap_err();
                assert!(error.to_string().contains("R5B_CONSUMER_ENCODING_FAILURE"));
                assert!(
                    !is_checked_input_capacity(&error),
                    "consumer error is not a shared capacity cause"
                );
            } else {
                assert!(!result.unwrap().is_empty());
            }
            assert!(
                db.is_autocommit(),
                "only the hook-owned deferred view was dropped"
            );
            assert_eq!(db.limit(Limit::SQLITE_LIMIT_LENGTH).unwrap(), prior);
            assert_eq!(
                record_text(db, f.id()),
                changed_record,
                "fresh view sees the deliberate second-connection change"
            );
            writer
                .db
                .execute(
                    "UPDATE records SET value=?1 WHERE kind=?2 AND id=?3",
                    rusqlite::params![original_record, git_workflows::REPORT_KIND, f.id()],
                )
                .unwrap();
            assert_eq!(state(&f.engine), before);
        }
    }
    db.set_limit(Limit::SQLITE_LIMIT_LENGTH, original_limit)
        .unwrap();
}

#[test]
fn consumer_encoding_failure_preserves_caller_transaction_savepoint_marker_and_exact_limit() {
    let f = fixture(false);
    qualify(&f);
    let db = &f.engine.store.db;
    let before = state(&f.engine);
    let original_limit = db.limit(Limit::SQLITE_LIMIT_LENGTH).unwrap();
    let prior = 128_000_000;
    db.set_limit(Limit::SQLITE_LIMIT_LENGTH, prior).unwrap();
    db.execute_batch("BEGIN DEFERRED; INSERT INTO meta(key,value) VALUES('R5B_CALLER_MARKER','owned'); SAVEPOINT r5b_caller_boundary;").unwrap();
    let caller_before = state(&f.engine);
    assert_eq!(
        caller_before["totalChanges"].as_u64(),
        before["totalChanges"]
            .as_u64()
            .and_then(|n| n.checked_add(1))
    );
    let original_record = record_text(db, f.id());
    let result: anyhow::Result<Vec<u8>> = with_checked_report_inputs(
        &f.engine.store,
        f.id(),
        maximum(),
        &Deadline::new(30_000),
        |inputs| {
            let mut output = EncodingProbe {
                db,
                limit: prior.min(64_004_096),
                report_id: f.id(),
                original_record: &original_record,
                bytes: vec![],
                writes: 0,
                fail: true,
            };
            let result = serde_json::to_writer(&mut output, inputs.changed_symbols());
            output.check();
            assert!(output.writes > 0);
            result?;
            Ok(output.bytes)
        },
    );
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("R5B_CONSUMER_ENCODING_FAILURE")
    );
    assert!(!db.is_autocommit());
    assert_eq!(db.limit(Limit::SQLITE_LIMIT_LENGTH).unwrap(), prior);
    assert_eq!(
        state(&f.engine),
        caller_before,
        "the producer preserves the complete caller-owned state"
    );
    db.execute_batch(
        "ROLLBACK TO SAVEPOINT r5b_caller_boundary; RELEASE SAVEPOINT r5b_caller_boundary;",
    )
    .unwrap();
    let marker: String = db
        .query_row(
            "SELECT value FROM meta WHERE key='R5B_CALLER_MARKER'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(marker, "owned");
    db.execute_batch("ROLLBACK").unwrap();
    assert!(db.is_autocommit());
    db.set_limit(Limit::SQLITE_LIMIT_LENGTH, original_limit)
        .unwrap();
    // SQLite's total_changes includes the caller's successful INSERT even after
    // its ROLLBACK. The exact pre-consumer comparison above detects any producer
    // writes; only this known caller-owned increment belongs in the final oracle.
    let mut after_caller_rollback = before;
    after_caller_rollback["totalChanges"] = caller_before["totalChanges"].clone();
    assert_eq!(state(&f.engine), after_caller_rollback);
}

#[test]
fn checked_sides_retain_proven_git_and_captured_working_tree_epochs_after_host_switch() {
    for (epoch, other) in [
        (AnalyzerEpoch::V8, AnalyzerEpoch::V9),
        (AnalyzerEpoch::V9, AnalyzerEpoch::V8),
    ] {
        let mut f = Fixture::new_with_profile(
            &[(CHANGED, SMALL), (UNREACHED, UNCHANGED)],
            &[(CHANGED, LARGE), (UNREACHED, UNCHANGED)],
            CaptureOptions {
                only_dirs: vec!["src".into()],
                ..Default::default()
            },
            epoch,
        );
        qualify(&f);
        assert_eq!(f.report["headMode"], "captured_working_tree");
        let base = snapshot(&f, "base");
        let head = snapshot(&f, "head");
        assert_eq!(base.kind, "derived");
        assert_eq!(head.kind, "disk");
        assert!(git_workflows::has_source_proof(&f.engine.store, &base.id).unwrap());
        assert!(!git_workflows::has_source_proof(&f.engine.store, &head.id).unwrap());
        let capture: crate::capture::CaptureOptionsRecord = f
            .engine
            .store
            .get_record(crate::capture::OPTIONS_RECORD_KIND, &head.id)
            .unwrap();
        assert_eq!(capture.analyzer, epoch.producer());
        assert_eq!(
            git_workflows::validate_source(&f.engine.store, &base.id, &Deadline::new(30_000))
                .unwrap()["analyzer"],
            epoch.producer()
        );
        let original_r5a = super::derive(
            &f.engine.store,
            f.id(),
            &super::DetailOptions::default(),
            &Deadline::new(30_000),
        )
        .unwrap()
        .canonical_bytes()
        .to_vec();
        f.engine.capture_epoch = other;
        f.engine.index(&Deadline::new(30_000)).unwrap();
        assert_ne!(f.engine.store.latest().unwrap().unwrap(), head.id);
        let before = state(&f.engine);
        with_checked_report_inputs(
            &f.engine.store,
            f.id(),
            maximum(),
            &Deadline::new(30_000),
            |inputs| {
                for side in [inputs.base(), inputs.head()] {
                    assert_eq!(side.analyzer_epoch(), epoch);
                    assert_eq!(side.analyzer_epoch().producer(), epoch.producer());
                    assert_eq!(
                        crate::capture::analyzer_epoch_for_snapshot_with_deadline(
                            &f.engine.store,
                            side.snapshot(),
                            &Deadline::new(30_000)
                        )?,
                        epoch
                    );
                }
                assert_eq!(inputs.base().snapshot().id, base.id);
                assert_eq!(inputs.head().snapshot().id, head.id);
                Ok(())
            },
        )
        .unwrap();
        assert_eq!(
            super::derive(
                &f.engine.store,
                f.id(),
                &super::DetailOptions::default(),
                &Deadline::new(30_000)
            )
            .unwrap()
            .canonical_bytes(),
            original_r5a
        );
        assert_eq!(state(&f.engine), before);
        assert_eq!(
            std::fs::read(f.dir.path().join(".git/index")).unwrap(),
            f.index_bytes
        );
    }
}
