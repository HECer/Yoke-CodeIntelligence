//! Retrieval-only selection/cache guards; no large graph is invented as trusted input.
use super::Engine;
use crate::{
    analysis::AnalyzerEpoch,
    capture::Deadline,
    model::{Graph, Snapshot, identity},
    retrieval_scopes::RetrievalHardError,
};
use serde_json::json;
use std::fs;

fn fixture(epoch: AnalyzerEpoch) -> (tempfile::TempDir, Engine, Snapshot, Graph) {
    let directory = tempfile::tempdir().unwrap();
    fs::write(directory.path().join("main.rs"), "fn payment() {}\n").unwrap();
    let mut engine = Engine::open(directory.path()).unwrap();
    engine.capture_epoch = epoch;
    let (snapshot, graph, parsed, reused) = engine.index(&Deadline::new(60_000)).unwrap();
    assert_eq!((parsed, reused), (1, 0));
    assert_eq!(snapshot.files.len(), 1);
    (directory, engine, snapshot, graph)
}

fn oversized_saved_graph(engine: &Engine, id: &str) {
    // Invalid JSON deliberately distinguishes length admission from a decoder
    // error or the legacy cached-read fallback into recomputation.
    let raw = " ".repeat(64_000_001);
    let changed = engine
        .store
        .db
        .execute(
            "UPDATE graphs SET value=?1 WHERE id=?2",
            rusqlite::params![raw, id],
        )
        .unwrap();
    assert_eq!(changed, 1);
    let bytes: usize = engine
        .store
        .db
        .query_row(
            "SELECT length(CAST(value AS BLOB)) FROM graphs WHERE id=?1",
            [id],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(bytes, 64_000_001);
}

fn assert_capacity(error: anyhow::Error) {
    assert!(
        matches!(
            error.downcast_ref::<RetrievalHardError>(),
            Some(RetrievalHardError::Capacity(_))
        ),
        "Graph byte admission must fail with typed Capacity, not decoding, unavailability or reanalysis: {error}"
    );
}

#[test]
fn bounded_saved_and_cached_selection_keep_real_epoch8_and_epoch9_inputs() {
    for epoch in [AnalyzerEpoch::V8, AnalyzerEpoch::V9] {
        let (_directory, mut engine, snapshot, graph) = fixture(epoch);
        let deadline = Deadline::new(60_000);
        let (selected, selected_graph) = engine
            .select_retrieval_inputs_at(&json!({"snapshotId":snapshot.id}), &deadline)
            .unwrap();
        assert_eq!(identity(&selected), identity(&snapshot));
        assert_eq!(identity(&selected_graph), identity(&graph));
        let removed = engine
            .store
            .db
            .execute(
                "DELETE FROM meta WHERE key IN ('latest', ?1)",
                [crate::freshness::BASELINE_KEY],
            )
            .unwrap();
        assert_eq!(
            removed, 2,
            "The owned fixture must clear both previous index effects"
        );
        let (cached, cached_graph, parsed, reused) = engine.index_retrieval_at(&deadline).unwrap();
        assert_eq!(identity(&cached), identity(&snapshot));
        assert_eq!(identity(&cached_graph), identity(&graph));
        assert_eq!((parsed, reused), (0, 1));
        let latest: String = engine
            .store
            .db
            .query_row("SELECT value FROM meta WHERE key='latest'", [], |r| {
                r.get(0)
            })
            .unwrap();
        let baseline: String = engine
            .store
            .db
            .query_row(
                "SELECT value FROM meta WHERE key=?1",
                [crate::freshness::BASELINE_KEY],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(latest, snapshot.id);
        assert_eq!(baseline, snapshot.id);
    }
}

#[test]
fn saved_retrieval_selection_rejects_oversized_graph_before_general_decode() {
    let (_directory, mut engine, snapshot, _) = fixture(AnalyzerEpoch::V9);
    oversized_saved_graph(&engine, &snapshot.id);
    let error = engine
        .select_retrieval_inputs_at(&json!({"snapshotId":snapshot.id}), &Deadline::new(60_000))
        .unwrap_err();
    assert_capacity(error);
}

#[test]
fn cached_retrieval_index_propagates_oversized_graph_instead_of_reanalysis() {
    let (_directory, mut engine, snapshot, _) = fixture(AnalyzerEpoch::V9);
    let (same, _, parsed, reused) = engine.index(&Deadline::new(60_000)).unwrap();
    assert_eq!(
        same.id, snapshot.id,
        "The real unchanged capture must hit the saved row"
    );
    assert_eq!((parsed, reused), (0, 1));
    oversized_saved_graph(&engine, &snapshot.id);
    let error = engine
        .index_retrieval_at(&Deadline::new(60_000))
        .unwrap_err();
    assert_capacity(error);
    let bytes: usize = engine
        .store
        .db
        .query_row(
            "SELECT length(CAST(value AS BLOB)) FROM graphs WHERE id=?1",
            [&snapshot.id],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(
        bytes, 64_000_001,
        "The rejected original row must not be replaced"
    );
}
