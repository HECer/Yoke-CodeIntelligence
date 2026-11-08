//! Windows comparison pin admission and retained Store read-view contracts.
//! Covers both selected registrations, requested pin fields and CAS tripwires.
//! Owned local fixture workers are explicit test setup only.
#[allow(
    clippy::duplicate_mod,
    reason = "Each private test harness registers the shared redaction calibration."
)]
#[path = "../tests/support/comparison_error_redaction.rs"]
mod error_redaction;
#[allow(
    clippy::duplicate_mod,
    reason = "Private fixture harnesses intentionally keep separate fixture type namespaces."
)]
#[path = "meaning_comparison_fixture_internal.rs"]
mod meaning_fixture;
#[allow(
    clippy::duplicate_mod,
    reason = "Private fixture harnesses independently qualify their own state oracle."
)]
#[path = "historical_fixture_state_internal.rs"]
mod oracle;

use super::*;
use crate::{
    Engine,
    analysis::AnalyzerEpoch,
    view_comparison::{ComparisonOptions, SelectionPin},
};
use rusqlite::hooks::{AuthAction, AuthContext, Authorization};
use std::{
    fs,
    fs::File,
    fs::OpenOptions,
    os::windows::fs::OpenOptionsExt,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};
use tempfile::TempDir;

const BEFORE: &str = "// __SOURCE_BEFORE_EVIDENCE_ONLY_81c49a__\nfn helper(){helper();}\nfn caller(){helper();helper();}\n";
const AFTER: &str = "// __SOURCE_AFTER_EVIDENCE_ONLY_f69d03__\nfn helper(){helper();caller();}\nfn caller(){helper();}\n";
const GUIDE: &str = "# Guide\n<!-- __SOURCE_DOC_EVIDENCE_ONLY_c5a918__ -->\n`main.rs::helper`\n";

fn d() -> Deadline {
    Deadline::new(30_000)
}

struct SourcePair {
    _root: TempDir,
    engine: Engine,
    before: View,
    after: View,
    alternate_before: View,
    alternate_after: View,
}

fn source_pair() -> SourcePair {
    let root = TempDir::new().unwrap();
    fs::write(root.path().join("main.rs"), BEFORE).unwrap();
    fs::write(root.path().join("guide.md"), GUIDE).unwrap();
    let mut engine = Engine::open(root.path()).unwrap();
    engine.capture_epoch = AnalyzerEpoch::V9;
    let mut selected = Vec::new();
    for text in [BEFORE, AFTER] {
        fs::write(root.path().join("main.rs"), text).unwrap();
        let (snapshot, graph, _, _) = engine.index(&d()).unwrap();
        let ordinary = build(
            &engine.store,
            &snapshot,
            &graph,
            &BuildOptions::default(),
            &d(),
        )
        .unwrap();
        let alternate = build(
            &engine.store,
            &snapshot,
            &graph,
            &BuildOptions {
                structural_kinds: vec![],
                ..Default::default()
            },
            &d(),
        )
        .unwrap();
        assert_eq!(ordinary.workspace_id, alternate.workspace_id);
        assert_eq!(ordinary.snapshot_id, alternate.snapshot_id);
        assert_eq!(ordinary.base_graph_hash, alternate.base_graph_hash);
        assert_ne!(ordinary.id, alternate.id);
        assert_ne!(ordinary.artifact_id, alternate.artifact_id);
        assert!(!ordinary.overlays.is_empty() && alternate.overlays.is_empty());
        selected.push((ordinary, alternate));
    }
    let (after, alternate_after) = selected.pop().unwrap();
    let (before, alternate_before) = selected.pop().unwrap();
    assert_ne!(before.snapshot_id, after.snapshot_id);
    assert_ne!(before.base_graph_hash, after.base_graph_hash);
    SourcePair {
        _root: root,
        engine,
        before,
        after,
        alternate_before,
        alternate_after,
    }
}

fn positive(f: &SourcePair) {
    let pair = error_redaction::checked(comparison_input::prepare_published_pair(
        &f.engine.store,
        &SelectionPin::from_view(&f.before),
        &SelectionPin::from_view(&f.after),
        &ComparisonOptions::default(),
        &d(),
    ))
    .expect("Both genuine complete selected sides must materialize before pin negatives");
    assert_eq!(pair.before().view().id, f.before.id);
    assert_eq!(pair.after().view().id, f.after.id);
    assert_eq!(pair.before().view().artifact_id, f.before.artifact_id);
    assert_eq!(pair.after().view().artifact_id, f.after.artifact_id);
    assert_eq!(pair.before().primary_snapshot().id, f.before.snapshot_id);
    assert_eq!(pair.after().primary_snapshot().id, f.after.snapshot_id);
}

fn lock_all_cas(engine: &Engine) -> Vec<File> {
    let files = fs::read_dir(engine.store.dir.join("blobs"))
        .unwrap()
        .map(|entry| {
            let entry = entry.unwrap();
            assert!(entry.file_type().unwrap().is_file());
            OpenOptions::new()
                .read(true)
                .share_mode(0)
                .open(entry.path())
                .unwrap()
        })
        .collect::<Vec<_>>();
    assert!(!files.is_empty());
    files
}

fn deny_registration_reads(engine: &Engine) -> Arc<AtomicUsize> {
    let count = Arc::new(AtomicUsize::new(0));
    let observed = count.clone();
    engine
        .store
        .db
        .authorizer(Some(move |context: AuthContext<'_>| {
            if let AuthAction::Read {
                table_name: "records",
                ..
            } = context.action
            {
                observed.fetch_add(1, Ordering::SeqCst);
                Authorization::Deny
            } else {
                Authorization::Allow
            }
        }));
    count
}

fn clear_authorizer(engine: &Engine) {
    engine
        .store
        .db
        .authorizer(None::<fn(AuthContext<'_>) -> Authorization>);
}

fn static_refusal<T>(result: anyhow::Result<T>) -> String {
    let error = match error_redaction::checked(result) {
        Ok(value) => {
            drop(value);
            panic!("Selected input must refuse")
        }
        Err(error) => error,
    };
    let message = format!("{error:#}");
    assert!(
        message.len() <= 256 && message.is_ascii(),
        "Refusal must remain bounded static metadata"
    );
    assert!(
        message.starts_with("comparison_"),
        "Registration admission must precede OS/SQL details"
    );
    assert!(
        !message.contains("implementation_unavailable"),
        "An unavailable constructor is not a guard proof"
    );
    message
}

fn pin_refusal<T>(result: anyhow::Result<T>, marker: &str) {
    let message = static_refusal(result);
    assert!(
        message.contains("comparison_pin_mismatch"),
        "Pin refusal classification is required"
    );
    assert!(
        message.contains(marker),
        "Pin refusal must identify the actual differing binding"
    );
}

#[test]
fn h03_existing_source_views_state_oracle_and_windows_cas_tripwire_qualify_without_comparison() {
    let f = source_pair();
    for view in [&f.before, &f.after, &f.alternate_before, &f.alternate_after] {
        let loaded = load(&f.engine.store, &view.id, &d()).unwrap();
        assert_eq!(
            serde_json::to_value(loaded).unwrap(),
            serde_json::to_value(view).unwrap()
        );
    }
    oracle::calibrate_foreign_connection_mutation_oracle(&f.engine);
    let baseline = oracle::state(&f.engine);
    let reads = deny_registration_reads(&f.engine);
    assert!(
        f.engine
            .store
            .db
            .query_row(
                "SELECT value FROM records WHERE kind=?1 AND id=?2",
                rusqlite::params![RECORD_KIND, &f.before.id],
                |row| row.get::<_, String>(0)
            )
            .is_err()
    );
    assert!(
        reads.load(Ordering::SeqCst) > 0,
        "The records authorizer tripwire must actually fire"
    );
    clear_authorizer(&f.engine);
    let path = f.engine.store.dir.join("blobs").join(&f.before.artifact_id);
    assert!(!fs::read(&path).unwrap().is_empty());
    let locks = lock_all_cas(&f.engine);
    let error = match fs::read(&path) {
        Ok(value) => {
            drop(value);
            panic!("Actual Windows deny-share lock must reject a CAS read")
        }
        Err(error) => error,
    };
    assert_eq!(error.raw_os_error(), Some(32));
    drop(locks);
    assert_eq!(oracle::state(&f.engine), baseline);
}

#[test]
fn h03_all_five_requested_pin_fields_are_admitted_on_both_sides_before_any_cas() {
    let f = source_pair();
    let foreign = source_pair();
    positive(&f);
    positive(&foreign);
    assert_ne!(
        f.engine.store.workspace_id,
        foreign.engine.store.workspace_id
    );
    let baseline = oracle::state(&f.engine);
    let foreign_baseline = oracle::state(&foreign.engine);
    for side in 0..2 {
        for field in 0..5 {
            let mut before = SelectionPin::from_view(&f.before);
            let mut after = SelectionPin::from_view(&f.after);
            let stale = if side == 0 { &mut before } else { &mut after };
            let opposite = if side == 0 { &f.after } else { &f.before };
            let alternate = if side == 0 {
                &f.alternate_before
            } else {
                &f.alternate_after
            };
            let marker = match field {
                0 => {
                    stale.workspace_id = foreign.engine.store.workspace_id.clone();
                    "workspace_id"
                }
                1 => {
                    stale.snapshot_id = opposite.snapshot_id.clone();
                    "snapshotId"
                }
                2 => {
                    // view_id is a lookup key, not an external expected identity.
                    // This REAL same-snapshot alternate differs only in View/artifact selection.
                    stale.view_id = alternate.id.clone();
                    "viewArtifactId"
                }
                3 => {
                    stale.view_artifact_id = alternate.artifact_id.clone();
                    "viewArtifactId"
                }
                _ => {
                    stale.base_graph_hash = opposite.base_graph_hash.clone();
                    "baseGraphHash"
                }
            };
            assert!(
                [
                    &stale.workspace_id,
                    &stale.snapshot_id,
                    &stale.view_id,
                    &stale.view_artifact_id,
                    &stale.base_graph_hash
                ]
                .into_iter()
                .all(|id| valid_id(id))
            );
            let locks = lock_all_cas(&f.engine);
            let result = comparison_input::prepare_published_pair(
                &f.engine.store,
                &before,
                &after,
                &ComparisonOptions::default(),
                &d(),
            );
            drop(locks);
            pin_refusal(result, marker);
            assert_eq!(oracle::state(&f.engine), baseline);
            assert_eq!(oracle::state(&foreign.engine), foreign_baseline);
        }
    }
}

#[test]
fn h03_matching_foreign_pins_wrong_store_and_mixed_workspaces_use_early_workspace_pin_refusal() {
    let f = source_pair();
    let other = source_pair();
    positive(&f);
    positive(&other);
    let a = (
        SelectionPin::from_view(&f.before),
        SelectionPin::from_view(&f.after),
    );
    let b = (
        SelectionPin::from_view(&other.before),
        SelectionPin::from_view(&other.after),
    );
    let baseline = oracle::state(&f.engine);
    let other_baseline = oracle::state(&other.engine);
    for (engine, before, after) in [
        (&f.engine, &b.0, &b.1),
        (&other.engine, &a.0, &a.1),
        (&f.engine, &a.0, &b.1),
        (&f.engine, &b.0, &a.1),
    ] {
        let reads = deny_registration_reads(&f.engine);
        let other_reads = deny_registration_reads(&other.engine);
        let locks = lock_all_cas(&f.engine);
        let other_locks = lock_all_cas(&other.engine);
        let result = comparison_input::prepare_published_pair(
            &engine.store,
            before,
            after,
            &ComparisonOptions::default(),
            &d(),
        );
        drop((locks, other_locks));
        clear_authorizer(&f.engine);
        clear_authorizer(&other.engine);
        pin_refusal(result, "workspace_id");
        assert_eq!(
            reads.load(Ordering::SeqCst),
            0,
            "Foreign workspace pins refuse before registration reads"
        );
        assert_eq!(other_reads.load(Ordering::SeqCst), 0);
        assert_eq!(oracle::state(&f.engine), baseline);
        assert_eq!(oracle::state(&other.engine), other_baseline);
    }
}

#[test]
fn h03_missing_registration_on_either_side_refuses_before_cas_without_lookup_fallback() {
    let f = source_pair();
    positive(&f);
    let missing = hash("owned_h03_missing_registration_lookup_key");
    let count: i64 = f
        .engine
        .store
        .db
        .query_row(
            "SELECT count(*) FROM records WHERE kind=?1 AND id=?2",
            rusqlite::params![RECORD_KIND, &missing],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(count, 0);
    let baseline = oracle::state(&f.engine);
    for side in 0..2 {
        let mut before = SelectionPin::from_view(&f.before);
        let mut after = SelectionPin::from_view(&f.after);
        if side == 0 {
            before.view_id = missing.clone();
        } else {
            after.view_id = missing.clone();
        }
        let locks = lock_all_cas(&f.engine);
        let result = comparison_input::prepare_published_pair(
            &f.engine.store,
            &before,
            &after,
            &ComparisonOptions::default(),
            &d(),
        );
        drop(locks);
        static_refusal(result);
        assert_eq!(oracle::state(&f.engine), baseline);
    }
}

#[test]
fn h03_registration_internal_view_id_binding_is_checked_separately_from_the_lookup_key() {
    let f = source_pair();
    positive(&f);
    let baseline = oracle::state(&f.engine);
    let writer = rusqlite::Connection::open(oracle::database_path(&f.engine)).unwrap();
    for view in [&f.before, &f.after] {
        let raw: String = writer
            .query_row(
                "SELECT value FROM records WHERE kind=?1 AND id=?2",
                rusqlite::params![RECORD_KIND, &view.id],
                |row| row.get(0),
            )
            .unwrap();
        let mut corrupt: Value = serde_json::from_str(&raw).unwrap();
        corrupt["viewId"] = json!(hash("owned_h03_wrong_internal_registration_identity"));
        assert_eq!(
            writer
                .execute(
                    "UPDATE records SET value=?1 WHERE kind=?2 AND id=?3",
                    rusqlite::params![
                        serde_json::to_string(&corrupt).unwrap(),
                        RECORD_KIND,
                        &view.id
                    ]
                )
                .unwrap(),
            1
        );
        // This is the fixture writer's intentional mutation, not an API effect.
        let expected_writer_state = oracle::state(&f.engine);
        assert_ne!(expected_writer_state, baseline);
        let locks = lock_all_cas(&f.engine);
        let result = comparison_input::prepare_published_pair(
            &f.engine.store,
            &SelectionPin::from_view(&f.before),
            &SelectionPin::from_view(&f.after),
            &ComparisonOptions::default(),
            &d(),
        );
        drop(locks);
        // No new spelling/class for corrupt registration field names is frozen here.
        static_refusal(result);
        assert_eq!(oracle::state(&f.engine), expected_writer_state);
        assert_eq!(
            writer
                .execute(
                    "UPDATE records SET value=?1 WHERE kind=?2 AND id=?3",
                    rusqlite::params![&raw, RECORD_KIND, &view.id]
                )
                .unwrap(),
            1
        );
        assert_eq!(oracle::state(&f.engine), baseline);
    }
}

#[test]
fn h03_one_supplied_store_read_view_survives_a_foreign_second_registration_commit() {
    let f = meaning_fixture::fixture(AnalyzerEpoch::V9);
    let job = &f.view.meaning.as_ref().unwrap().packets[0]
        .provenance
        .original_job_id;
    let status = crate::meaning_build::status(&f.engine.store, job, None, &d()).unwrap();
    assert_eq!(status["status"], "ready");
    let graph = f.engine.store.graph(&f.original.id).unwrap();
    let original = build(
        &f.engine.store,
        &f.original,
        &graph,
        &BuildOptions {
            structural_kinds: vec![StructuralKind::Markdown],
            meaning_graph_ids: vec![status["graphId"].as_str().unwrap().into()],
            ..Default::default()
        },
        &d(),
    )
    .unwrap();
    let before = SelectionPin::from_view(&original);
    let after = SelectionPin::from_view(&f.view);
    let mut allow = |_: &Snapshot, _: &Deadline| Ok(());
    let pair = error_redaction::checked(comparison_input::prepare_published_pair_guarded(
        &f.engine.store,
        &before,
        &after,
        &ComparisonOptions::default(),
        &mut allow,
        &d(),
    ))
    .expect("A genuine distinct original/rebound pair must work before the controlled writer");
    assert_eq!(pair.before().view().id, original.id);
    assert_eq!(pair.after().view().id, f.view.id);
    drop(pair);
    assert!(f.engine.store.db.is_autocommit());
    oracle::calibrate_foreign_connection_mutation_oracle(&f.engine);
    let baseline = oracle::state(&f.engine);
    let calls = fs::read(&f.marker).unwrap();
    let local_changes: i64 = f
        .engine
        .store
        .db
        .query_row("SELECT total_changes()", [], |row| row.get(0))
        .unwrap();
    let writer = rusqlite::Connection::open(oracle::database_path(&f.engine)).unwrap();
    writer.busy_timeout(Duration::from_secs(2)).unwrap();
    let old: String = writer
        .query_row(
            "SELECT value FROM records WHERE kind=?1 AND id=?2",
            rusqlite::params![RECORD_KIND, &f.view.id],
            |row| row.get(0),
        )
        .unwrap();
    let mut changed: Value = serde_json::from_str(&old).unwrap();
    changed["artifactId"] = json!(original.artifact_id);
    let changed = serde_json::to_string(&changed).unwrap();
    assert_ne!(old, changed);
    let mut mutated = false;
    let mut callback_count = 0;
    let mut expected_writer_state = None;
    let mut observe = |_: &Snapshot, _: &Deadline| {
        callback_count += 1;
        assert!(
            !f.engine.store.db.is_autocommit(),
            "The supplied connection must retain its read transaction"
        );
        if !mutated {
            // WAL writer changes only a second-side registration after pair admission.
            // This committed mutation is intentional test setup and MUST remain visible
            // to a fresh reader; it is not attributed to comparison code.
            assert_eq!(
                writer
                    .execute(
                        "UPDATE records SET value=?1 WHERE kind=?2 AND id=?3",
                        rusqlite::params![&changed, RECORD_KIND, &f.view.id]
                    )
                    .unwrap(),
                1
            );
            assert!(writer.is_autocommit());
            let fresh: String = writer
                .query_row(
                    "SELECT value FROM records WHERE kind=?1 AND id=?2",
                    rusqlite::params![RECORD_KIND, &f.view.id],
                    |row| row.get(0),
                )
                .unwrap();
            assert_eq!(fresh, changed);
            expected_writer_state = Some(oracle::state(&f.engine));
            assert_ne!(expected_writer_state.as_ref().unwrap(), &baseline);
            mutated = true;
        }
        let retained: String = f
            .engine
            .store
            .db
            .query_row(
                "SELECT value FROM records WHERE kind=?1 AND id=?2",
                rusqlite::params![RECORD_KIND, &f.view.id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(
            retained, old,
            "Both sides must see the same earlier registration bytes"
        );
        Ok(())
    };
    let pair = error_redaction::checked(comparison_input::prepare_published_pair_guarded(
        &f.engine.store,
        &before,
        &after,
        &ComparisonOptions::default(),
        &mut observe,
        &d(),
    ))
    .expect("The pair retains its admitted registration view despite the later foreign commit");
    assert_eq!(pair.before().view().id, original.id);
    assert_eq!(pair.after().view().id, f.view.id);
    assert_eq!(pair.after().view().artifact_id, f.view.artifact_id);
    drop(pair);
    assert!(mutated && callback_count == 3);
    assert!(f.engine.store.db.is_autocommit());
    let fresh: String = f
        .engine
        .store
        .db
        .query_row(
            "SELECT value FROM records WHERE kind=?1 AND id=?2",
            rusqlite::params![RECORD_KIND, &f.view.id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(
        fresh, changed,
        "Ending the API read view reveals the writer's committed row"
    );
    assert_eq!(oracle::state(&f.engine), expected_writer_state.unwrap());
    assert_eq!(
        f.engine
            .store
            .db
            .query_row("SELECT total_changes()", [], |row| row.get::<_, i64>(0))
            .unwrap(),
        local_changes
    );
    assert_eq!(fs::read(&f.marker).unwrap(), calls);
    assert_eq!(
        writer
            .execute(
                "UPDATE records SET value=?1 WHERE kind=?2 AND id=?3",
                rusqlite::params![&old, RECORD_KIND, &f.view.id]
            )
            .unwrap(),
        1
    );
    assert_eq!(oracle::state(&f.engine), baseline);
}
