//! Windows comparison input policy-ordering and guarded-replay tests.
//! Share-mode CAS locks distinguish policy denial from premature source reads.
//! Owned local workers belong to fixture setup, outside comparison calls.
#[path = "../tests/support/comparison_error_redaction.rs"]
mod error_redaction;
use super::*;
#[path = "meaning_comparison_fixture_internal.rs"]
mod fixture;
#[path = "historical_fixture_state_internal.rs"]
mod oracle;

use crate::analysis::AnalyzerEpoch;
use std::{fs, fs::OpenOptions, os::windows::fs::OpenOptionsExt};

#[test]
fn h07_valid_current_scope_cannot_hide_a_late_original_scope_policy_denial_before_cas() {
    let f = fixture::fixture(AnalyzerEpoch::V9);
    let checked = archive_meaning_view_by_id(
        &f.engine.store,
        &f.view.id,
        MAX_BYTES,
        64_000_000,
        &fixture::d(),
    )
    .expect("Genuine original/rebound closure must prepare before any denial or lock");
    let scopes = checked
        .proof()
        .source_scopes()
        .map(|scope| scope.id.clone())
        .collect::<BTreeSet<_>>();
    assert!(scopes.contains(&f.original.id) && scopes.contains(&f.snapshot.id));
    assert_eq!(checked.view().id, f.view.id);
    drop(checked);
    // Prove the current published scope's actual native policy accepts first.
    // Source-manifest iteration is hash order; no invented callback order is asserted.
    preflight_native_archive_scope(&f.engine.store, &f.snapshot, &fixture::d()).unwrap();
    let baseline = oracle::state(&f.engine);
    let calls = fs::read(&f.marker).unwrap();
    let locks = fs::read_dir(f.engine.store.dir.join("blobs"))
        .unwrap()
        .map(|entry| {
            OpenOptions::new()
                .read(true)
                .share_mode(0)
                .open(entry.unwrap().path())
                .unwrap()
        })
        .collect::<Vec<_>>();
    assert!(!locks.is_empty());
    let denied = "comparison_test_late_original_policy_denied";
    let mut seen = Vec::new();
    let mut preflight = |scope: &Snapshot, deadline: &Deadline| {
        seen.push(scope.id.clone());
        if scope.id == f.original.id {
            anyhow::bail!(denied);
        }
        preflight_native_archive_scope(&f.engine.store, scope, deadline)
    };
    let error = match error_redaction::checked(archive_meaning_view_by_id_guarded(
        &f.engine.store,
        &f.view.id,
        MAX_BYTES,
        64_000_000,
        &mut preflight,
        &fixture::d(),
    )) {
        Ok(value) => {
            drop(value);
            panic!("Original scope deliberately denies after the current-policy positive control")
        }
        Err(error) => error,
    };
    drop(locks);
    assert!(
        format!("{error:#}").contains(denied),
        "A policy denial must precede any locked CAS open"
    );
    assert_eq!(seen.last(), Some(&f.original.id));
    assert!(seen.iter().all(|id| scopes.contains(id)));
    assert_eq!(oracle::state(&f.engine), baseline);
    assert_eq!(fs::read(&f.marker).unwrap(), calls);
}

#[test]
fn h07_pair_allows_the_first_authorized_side_then_denies_after_side_history_before_its_cas() {
    use crate::view_comparison::{ComparisonOptions, SelectionPin};
    let f = fixture::fixture(AnalyzerEpoch::V9);
    let job = &f.view.meaning.as_ref().unwrap().packets[0]
        .provenance
        .original_job_id;
    let status = crate::meaning_build::status(&f.engine.store, job, None, &fixture::d()).unwrap();
    assert_eq!(status["status"], "ready");
    let graph = f.engine.store.graph(&f.original.id).unwrap();
    let original_view = build(
        &f.engine.store,
        &f.original,
        &graph,
        &BuildOptions {
            structural_kinds: vec![StructuralKind::Markdown],
            meaning_graph_ids: vec![status["graphId"].as_str().unwrap().into()],
            ..Default::default()
        },
        &fixture::d(),
    )
    .unwrap();
    assert!(
        original_view
            .meaning
            .as_ref()
            .unwrap()
            .packets
            .iter()
            .all(|packet| !packet.provenance.rebound)
    );
    assert!(
        f.view
            .meaning
            .as_ref()
            .unwrap()
            .packets
            .iter()
            .all(|packet| packet.provenance.rebound)
    );
    assert_ne!(original_view.artifact_id, f.view.artifact_id);
    let before = SelectionPin::from_view(&original_view);
    let after = SelectionPin::from_view(&f.view);
    let options = ComparisonOptions::default();
    let mut seen = Vec::new();
    let mut allow = |scope: &Snapshot, _: &Deadline| {
        seen.push(scope.id.clone());
        Ok(())
    };
    let pair = error_redaction::checked(comparison_input::prepare_published_pair_guarded(
        &f.engine.store,
        &before,
        &after,
        &options,
        &mut allow,
        &fixture::d(),
    ))
    .expect("Both real published sides must materialize before the late historical denial");
    assert_eq!(pair.before().view().id, original_view.id);
    assert_eq!(pair.after().view().id, f.view.id);
    drop(pair);
    // Frozen private hook contract: once per complete scope PER side, before side first.
    assert_eq!(
        seen.iter()
            .filter(|id| id.as_str() == f.original.id.as_str())
            .count(),
        2
    );
    assert_eq!(
        seen.iter()
            .filter(|id| id.as_str() == f.snapshot.id.as_str())
            .count(),
        1
    );
    assert_eq!(seen.len(), 3);
    preflight_native_archive_scope(&f.engine.store, &f.snapshot, &fixture::d()).unwrap();
    let baseline = oracle::state(&f.engine);
    let calls = fs::read(&f.marker).unwrap();
    let new_hash = &f
        .snapshot
        .files
        .iter()
        .find(|file| file.path == "history.rs")
        .unwrap()
        .hash;
    assert!(!f.original.files.iter().any(|file| &file.hash == new_hash));
    // Only after-side unique CAS is locked. Earlier authorized before-side reads are permitted.
    let locks = [&f.view.artifact_id, new_hash]
        .into_iter()
        .map(|id| {
            OpenOptions::new()
                .read(true)
                .share_mode(0)
                .open(f.engine.store.dir.join("blobs").join(id))
                .unwrap()
        })
        .collect::<Vec<_>>();
    let denied = "comparison_test_after_side_original_policy_denied";
    let mut originals = 0;
    let mut seen = Vec::new();
    let mut deny_late = |scope: &Snapshot, _: &Deadline| {
        seen.push(scope.id.clone());
        if scope.id == f.original.id {
            originals += 1;
            if originals == 2 {
                anyhow::bail!(denied);
            }
        }
        Ok(())
    };
    let error = match error_redaction::checked(comparison_input::prepare_published_pair_guarded(
        &f.engine.store,
        &before,
        &after,
        &options,
        &mut deny_late,
        &fixture::d(),
    )) {
        Ok(value) => {
            drop(value);
            panic!("The after side's historical policy deliberately denies this pair")
        }
        Err(error) => error,
    };
    drop(locks);
    assert!(
        format!("{error:#}").contains(denied),
        "After-side CAS must not precede its complete policy preflight"
    );
    assert_eq!(originals, 2);
    assert_eq!(seen.first(), Some(&f.original.id));
    assert_eq!(seen.last(), Some(&f.original.id));
    assert_eq!(oracle::state(&f.engine), baseline);
    assert_eq!(fs::read(&f.marker).unwrap(), calls);
}
