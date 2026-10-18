use super::*;
use crate::model::{Omission, SourceFile};

fn scope() -> Snapshot {
    Snapshot {
        id: hash("index-snapshot"),
        workspace_id: hash("index-workspace"),
        kind: "filesystem".into(),
        capture_consistency: "best_effort".into(),
        omissions: vec![],
        files: vec![SourceFile {
            path: "a.rs".into(),
            hash: hash("a"),
            size: 1,
            language: "rust".into(),
            mode: 0o100644,
        }],
    }
}
fn read_member(
    index: &FrozenSourceIndex,
    snapshot: &Snapshot,
    file: &SourceFile,
    calls: &mut usize,
    d: &Deadline,
) -> Result<Vec<u8>> {
    let bound = index.bind(snapshot, d)?;
    let member = bound.member(file, d)?;
    let mut reader = |member: &FrozenSourceMember, _: &Deadline| {
        *calls += 1;
        ensure!(
            member.snapshot().id == snapshot.id && member.file().path == file.path,
            "Wrong admitted member"
        );
        Ok(b"a".to_vec())
    };
    let reader: &mut FrozenMemberReader<'_> = &mut reader;
    reader(&member, d)
}
#[test]
fn frozen_source_index_valid_member_and_duplicate_scope_are_admitted() {
    let s = scope();
    let d = Deadline::new(30000);
    let index = FrozenSourceIndex::new([&s, &s], 64_000_000, &d).unwrap();
    assert_eq!(index.scope_count(), 1);
    assert_eq!(index.file_entries(), 1);
    assert_eq!(index.unique_bytes(), 1);
    assert_eq!(index.unique_files().count(), 1);
    assert_eq!(index.scopes().next().unwrap().id, s.id);
    let mut calls = 0;
    assert_eq!(
        read_member(&index, &s, &s.files[0], &mut calls, &d).unwrap(),
        b"a"
    );
    assert_eq!(calls, 1);
}
#[test]
fn frozen_source_index_rechecks_complete_same_id_scope_on_every_bind() {
    let s = scope();
    let d = Deadline::new(30000);
    let index = FrozenSourceIndex::new([&s], 64_000_000, &d).unwrap();
    let mut calls = 0;
    read_member(&index, &s, &s.files[0], &mut calls, &d).unwrap();
    assert_eq!(calls, 1);
    let mut forged = s.clone();
    forged.omissions.push(Omission {
        path: "private.rs".into(),
        reason: "forged".into(),
    });
    forged.capture_consistency = "forged-same-id-consistency".into();
    calls = 0;
    let result = read_member(&index, &forged, &s.files[0], &mut calls, &d);
    assert!(
        result.is_err(),
        "A valid earlier bind must not authorize different metadata with the same snapshot ID"
    );
    assert_eq!(calls, 0, "Forged scope reached the frozen source reader");
}
#[test]
fn frozen_source_index_refuses_forged_same_path_hash_and_size_before_reader() {
    let s = scope();
    let d = Deadline::new(30000);
    let index = FrozenSourceIndex::new([&s], 64_000_000, &d).unwrap();
    for file in [
        SourceFile {
            hash: hash("forged"),
            ..s.files[0].clone()
        },
        SourceFile {
            size: 2,
            ..s.files[0].clone()
        },
    ] {
        let mut calls = 0;
        let result = read_member(&index, &s, &file, &mut calls, &d);
        assert!(
            result.is_err(),
            "A path alone does not authorize changed file metadata: {file:?}"
        );
        assert_eq!(calls, 0, "Forged file reached the frozen source reader");
    }
}
#[test]
fn frozen_source_index_refuses_conflicting_same_id_union_scope() {
    let s = scope();
    let d = Deadline::new(30000);
    let mut forged = s.clone();
    forged.files[0].hash = hash("changed");
    assert!(
        FrozenSourceIndex::new([&s, &forged], 64_000_000, &d).is_err(),
        "Complete union admission must reject conflicting manifests sharing an ID"
    );
}
#[test]
fn frozen_source_index_many_members_reuse_one_bound_scope_without_timing_claim() {
    let mut s = scope();
    let d = Deadline::new(30000);
    s.files = (0..8192)
        .map(|i| SourceFile {
            path: format!("files/{i:05}.rs"),
            ..s.files[0].clone()
        })
        .collect();
    let index = FrozenSourceIndex::new([&s], 64_000_000, &d).unwrap();
    let bound = index.bind(&s, &d).unwrap();
    let mut count = 0;
    for (member, file) in bound.members().zip(&s.files) {
        assert_eq!(member.file(), file);
        assert_eq!(member.snapshot().id, s.id);
        count += 1;
    }
    assert_eq!(count, 8192);
    assert_eq!(index.file_entries(), 8192);
    assert_eq!(index.unique_bytes(), 1);
}

fn initial_scope_fixture() -> (tempfile::TempDir, crate::Engine, Snapshot) {
    let directory = tempfile::tempdir().unwrap();
    std::fs::write(directory.path().join("main.rs"), "fn initial() {}\n").unwrap();
    let mut engine = crate::Engine::open(directory.path()).unwrap();
    engine.capture_epoch = crate::analysis::AnalyzerEpoch::V9;
    let (snapshot, _, _, _) = engine.index(&Deadline::new(30000)).unwrap();
    (directory, engine, snapshot)
}
fn capacity_error(store: &Store, id: &str, cap: usize) {
    let before: i64 = store
        .db
        .query_row("SELECT total_changes()", [], |r| r.get(0))
        .unwrap();
    let error = archive_initial_scope(store, id, cap, &Deadline::new(30000)).unwrap_err();
    assert!(
        format!("{error:#}").contains("capacity"),
        "Initial scope capacity must be admitted before malformed JSON decoding: {error:#}"
    );
    let after: i64 = store
        .db
        .query_row("SELECT total_changes()", [], |r| r.get(0))
        .unwrap();
    assert_eq!(after, before);
}
#[test]
fn archive_initial_snapshot_row_capacity_precedes_json_decode() {
    let (_directory, engine, snapshot) = initial_scope_fixture();
    let malformed = format!("{{\"unterminated\":\"{}", "x".repeat(4096));
    engine
        .store
        .db
        .execute(
            "UPDATE snapshots SET value=?1 WHERE id=?2",
            params![malformed, snapshot.id],
        )
        .unwrap();
    capacity_error(&engine.store, &snapshot.id, 2048);
}
#[test]
fn archive_initial_graph_row_capacity_precedes_json_decode() {
    let (_directory, engine, snapshot) = initial_scope_fixture();
    let malformed = format!("{{\"unterminated\":\"{}", "x".repeat(4096));
    engine
        .store
        .db
        .execute(
            "UPDATE graphs SET value=?1 WHERE id=?2",
            params![malformed, snapshot.id],
        )
        .unwrap();
    capacity_error(&engine.store, &snapshot.id, 2048);
}
#[test]
fn archive_initial_snapshot_and_graph_share_one_capacity_before_decode() {
    let (_directory, engine, snapshot) = initial_scope_fixture();
    let raw: String = engine
        .store
        .db
        .query_row(
            "SELECT value FROM snapshots WHERE id=?1",
            [&snapshot.id],
            |r| r.get(0),
        )
        .unwrap();
    let cap = raw.len() + 64;
    let malformed = format!("{{\"unterminated\":\"{}", "x".repeat(96));
    assert!(raw.len() < cap && malformed.len() < cap && raw.len() + malformed.len() > cap);
    engine
        .store
        .db
        .execute(
            "UPDATE graphs SET value=?1 WHERE id=?2",
            params![malformed, snapshot.id],
        )
        .unwrap();
    capacity_error(&engine.store, &snapshot.id, cap);
}
