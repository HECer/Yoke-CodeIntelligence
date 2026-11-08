//! Private comparison budget and deadline contracts over checked published archives.
//! Archive getters supply oracle data and do not grant additional source access.
//! Owned local fixture workers run only during explicit test setup.
#[allow(
    clippy::duplicate_mod,
    reason = "Each private test harness registers the shared redaction calibration."
)]
#[path = "../tests/support/comparison_error_redaction.rs"]
mod error_oracle;
#[allow(
    clippy::duplicate_mod,
    reason = "Private fixture harnesses independently qualify their own state oracle."
)]
#[path = "historical_fixture_state_internal.rs"]
mod oracle;
#[allow(
    clippy::duplicate_mod,
    reason = "Private fixture harnesses intentionally keep separate fixture type namespaces."
)]
#[path = "meaning_comparison_fixture_internal.rs"]
mod support;

use crate::{
    analysis::AnalyzerEpoch,
    capture::{Deadline, DeadlineFailure},
    graph_view::{self, BuildOptions, StructuralKind},
    graph_view_archive::{self as archive, CheckedMeaningArchive, Limits},
    meaning_build,
    model::{Snapshot, hash},
    view_comparison::{self, ComparisonOptions, SelectionPin},
};
use serde::Serialize;
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::Write,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

// Independent, literal contract envelope from INPUT_API.md. This counter does
// not call any future pair/accounting function, serialize the wire source map,
// or allocate a second proof-sized byte buffer.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ContractSideProof<'a> {
    profile: &'static str,
    snapshot: &'a Snapshot,
    archive: &'a super::ArchiveProof,
}
struct Count(usize);
impl Write for Count {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0 = self
            .0
            .checked_add(bytes.len())
            .ok_or_else(|| std::io::Error::other("oracle count overflow"))?;
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
fn proof_bytes(checked: &CheckedMeaningArchive) -> usize {
    let mut bytes = Count(0);
    serde_json::to_writer(
        &mut bytes,
        &ContractSideProof {
            profile: "yoke_view_comparison_input_1",
            snapshot: checked.snapshot(),
            archive: checked.proof(),
        },
    )
    .unwrap();
    bytes.0
}

fn source_union(sides: &[&CheckedMeaningArchive]) -> (usize, usize) {
    let mut union = BTreeMap::<String, usize>::new();
    for side in sides {
        let mut listed = BTreeSet::new();
        for file in side.source_files() {
            let text = side
                .source_objects()
                .get(&file.hash)
                .expect("Checked full manifest source is present");
            assert_eq!(hash(text), file.hash);
            assert_eq!(text.len(), file.size);
            listed.insert(file.hash.clone());
            if let Some(previous) = union.insert(file.hash.clone(), text.len()) {
                assert_eq!(previous, text.len());
            }
        }
        assert_eq!(listed, side.source_objects().keys().cloned().collect());
    }
    let bytes = union
        .values()
        .try_fold(0usize, |sum, value| sum.checked_add(*value))
        .unwrap();
    (bytes, union.len())
}

fn archives(f: &support::Fixture) -> (CheckedMeaningArchive, CheckedMeaningArchive) {
    let layer = f.view.meaning.as_ref().unwrap();
    let original_job = &layer.packets[0].provenance.original_job_id;
    let status = meaning_build::status(&f.engine.store, original_job, None, &support::d()).unwrap();
    assert_eq!(status["status"], "ready");
    let graph = f.engine.store.graph(&f.original.id).unwrap();
    let original = graph_view::build(
        &f.engine.store,
        &f.original,
        &graph,
        &BuildOptions {
            structural_kinds: vec![StructuralKind::Markdown],
            meaning_graph_ids: vec![status["graphId"].as_str().unwrap().into()],
            ..Default::default()
        },
        &support::d(),
    )
    .unwrap();
    let load = |view: &graph_view::View| {
        let prepared =
            archive::prepare_published(&f.engine.store, &view.id, Limits::default(), &support::d())
                .unwrap();
        assert!(
            prepared.bytes().len() <= 32_000_000,
            "Existing per-side wire admission remains separate"
        );
        archive::parse(prepared.bytes(), Limits::default(), &support::d()).unwrap()
    };
    (load(&original), load(&f.view))
}

fn compare(
    before: &CheckedMeaningArchive,
    after: &CheckedMeaningArchive,
    options: &ComparisonOptions,
    deadline: &Deadline,
) -> anyhow::Result<view_comparison::Comparison> {
    error_oracle::checked(view_comparison::compare_meaning_archives(
        before, after, options, deadline,
    ))
}
fn capacity(result: anyhow::Result<view_comparison::Comparison>) {
    let error = match result {
        Ok(value) => {
            drop(value);
            panic!("Exact one-below capacity was accepted")
        }
        Err(error) => error,
    };
    assert!(format!("{error:#}").contains("comparison_capacity_exceeded"));
}

#[test]
fn h15_same_checked_archive_counts_proof_twice_but_full_union_source_hashes_once() {
    let f = support::fixture(AnalyzerEpoch::V9);
    let (_, checked) = archives(&f);
    let one_proof = proof_bytes(&checked);
    let pair_proof = one_proof.checked_mul(2).unwrap();
    let (source_bytes, objects) = source_union(&[&checked, &checked]);
    assert_eq!(objects, checked.source_objects().len());
    assert_eq!(
        source_bytes,
        checked
            .source_objects()
            .values()
            .map(|text| text.len())
            .sum::<usize>()
    );
    assert!(pair_proof > 1 && source_bytes > 1);
    assert!(pair_proof < 64_000_000 && source_bytes < 64_000_000);
    // Counted input is NOT CheckedMeaningArchive.bytes(): that transport also
    // contains the source map, and its existing 32MB limit is still independent.
    assert!(checked.bytes().len() > one_proof);
    let baseline = oracle::state(&f.engine);
    let calls = fs::read(&f.marker).unwrap();
    let exact = ComparisonOptions {
        max_input_bytes: pair_proof,
        max_source_bytes: source_bytes,
        ..Default::default()
    };
    compare(&checked, &checked, &exact, &support::d()).unwrap();
    capacity(compare(
        &checked,
        &checked,
        &ComparisonOptions {
            max_input_bytes: pair_proof - 1,
            ..exact.clone()
        },
        &support::d(),
    ));
    capacity(compare(
        &checked,
        &checked,
        &ComparisonOptions {
            max_input_bytes: one_proof,
            ..exact.clone()
        },
        &support::d(),
    ));
    capacity(compare(
        &checked,
        &checked,
        &ComparisonOptions {
            max_source_bytes: source_bytes - 1,
            ..exact
        },
        &support::d(),
    ));
    assert_eq!(oracle::state(&f.engine), baseline);
    assert_eq!(fs::read(&f.marker).unwrap(), calls);
}

#[test]
fn h15_original_rebound_full_union_has_an_exact_distinct_source_boundary_after_store_removal() {
    let f = support::fixture(AnalyzerEpoch::V9);
    let (original, rebound) = archives(&f);
    let old_hash = hash(support::HISTORY_OLD);
    let new_hash = hash(support::HISTORY_NEW);
    assert_eq!(
        original.source_objects().get(&old_hash).unwrap(),
        support::HISTORY_OLD
    );
    assert!(!original.source_objects().contains_key(&new_hash));
    assert_eq!(
        rebound.source_objects().get(&old_hash).unwrap(),
        support::HISTORY_OLD
    );
    assert_eq!(
        rebound.source_objects().get(&new_hash).unwrap(),
        support::HISTORY_NEW
    );
    assert!(
        rebound
            .view()
            .meaning
            .as_ref()
            .unwrap()
            .packets
            .iter()
            .flat_map(|p| &p.evidence)
            .all(|e| e.path != "history.rs")
    );
    let (bytes, objects) = source_union(&[&original, &rebound]);
    let original_bytes: usize = original
        .source_objects()
        .values()
        .map(|text| text.len())
        .sum();
    assert_eq!(bytes, original_bytes + support::HISTORY_NEW.len());
    assert_eq!(objects, original.source_objects().len() + 1);
    assert!(
        support::A.len() > support::A.chars().count(),
        "UTF8 source byte oracle is not a character count"
    );
    let proof_bytes = proof_bytes(&original)
        .checked_add(proof_bytes(&rebound))
        .unwrap();
    let marker = f.marker.clone();
    let calls = fs::read(&marker).unwrap();
    let removed = f.engine.root.clone();
    let _retained = support::remove_source_workspace(f);
    assert!(!removed.exists());
    let exact = ComparisonOptions {
        max_input_bytes: proof_bytes,
        max_source_bytes: bytes,
        ..Default::default()
    };
    compare(&original, &rebound, &exact, &support::d()).unwrap();
    capacity(compare(
        &original,
        &rebound,
        &ComparisonOptions {
            max_input_bytes: proof_bytes - 1,
            ..exact.clone()
        },
        &support::d(),
    ));
    capacity(compare(
        &original,
        &rebound,
        &ComparisonOptions {
            max_source_bytes: bytes - 1,
            ..exact
        },
        &support::d(),
    ));
    assert_eq!(fs::read(marker).unwrap(), calls);
}

#[test]
fn h15_both_public_entrypoints_honor_real_inherited_cancellation_and_zero_deadline() {
    let f = support::fixture(AnalyzerEpoch::V9);
    let (_, checked) = archives(&f);
    let pin = SelectionPin::from_view(&f.view);
    let baseline = oracle::state(&f.engine);
    let calls = fs::read(&f.marker).unwrap();
    let published = |deadline: &Deadline| {
        error_oracle::checked(view_comparison::compare_published(
            &f.engine.store,
            &pin,
            &pin,
            &ComparisonOptions::default(),
            deadline,
        ))
    };
    let parent = Deadline::new(300_000);
    let flag = Arc::new(AtomicBool::new(false));
    Deadline::run_bounded(&parent, flag.clone(), || {
        let d = Deadline::new(30_000);
        assert_eq!(d.failure(), None);
        published(&d).unwrap();
        compare(&checked, &checked, &ComparisonOptions::default(), &d).unwrap();
        flag.store(true, Ordering::SeqCst);
        assert_eq!(d.failure(), Some(DeadlineFailure::Cancelled));
        assert!(published(&d).is_err());
        assert!(compare(&checked, &checked, &ComparisonOptions::default(), &d).is_err());
        assert_eq!(d.failure(), Some(DeadlineFailure::Cancelled));
        assert_eq!(oracle::state(&f.engine), baseline);
        assert_eq!(fs::read(&f.marker).unwrap(), calls);
        // Cleanup only after every refusal/side-effect oracle, so run_bounded's
        // final parent check does not mask the exact inner comparison result.
        flag.store(false, Ordering::SeqCst);
        Ok(())
    })
    .unwrap();
    let expired = Deadline::new(0);
    assert_eq!(expired.failure(), Some(DeadlineFailure::Expired));
    assert!(published(&expired).is_err());
    assert!(compare(&checked, &checked, &ComparisonOptions::default(), &expired).is_err());
    assert_eq!(expired.failure(), Some(DeadlineFailure::Expired));
    assert_eq!(oracle::state(&f.engine), baseline);
    assert_eq!(fs::read(&f.marker).unwrap(), calls);
}
