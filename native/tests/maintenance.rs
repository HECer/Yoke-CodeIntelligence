use serde_json::json;
use std::{
    fs,
    path::Path,
    time::{Duration, Instant},
};
use tempfile::TempDir;
use yoke::{
    Engine,
    capture::Deadline,
    maintenance::{self, Policy},
    model::hash,
};
fn fixture() -> (TempDir, Engine) {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("a.rs"), "fn retained() {}\n").unwrap();
    let mut engine = Engine::open(dir.path()).unwrap();
    engine.index(&Deadline::new(10000)).unwrap();
    (dir, engine)
}
fn policy(apply: bool) -> Policy {
    Policy {
        apply,
        minimum_age_ms: 0,
        purge_quarantine: false,
    }
}

#[test]
fn dry_run_counts_orphans_without_mutating_source_or_state_files() {
    let (dir, engine) = fixture();
    let orphan = engine.store.blob(b"unreferenced artifact").unwrap();
    let report =
        maintenance::collect(&engine.store, &policy(false), &Deadline::new(10000)).unwrap();
    assert_eq!(report["applied"], false);
    assert_eq!(report["unreferencedBlobs"], 1);
    assert_eq!(report["retiredSnapshots"], 0);
    assert_eq!(report["reclaimedBytes"], 0);
    assert_eq!(
        engine.store.read_blob(&orphan).unwrap(),
        b"unreferenced artifact"
    );
    assert!(!dir.path().join(".yoke/quarantine").exists());
}

#[test]
fn all_generic_records_and_history_preserve_workflow_editor_vector_and_media_roots() {
    let (dir, mut engine) = fixture();
    let first = engine.store.latest().unwrap().unwrap();
    maintenance::pin(
        &engine.store,
        &first,
        "keep-first",
        "source review",
        &Deadline::new(10000),
    )
    .unwrap();
    let mut protected = Vec::new();
    for kind in [
        "plan",
        "journal",
        "editor-session",
        "knowledge",
        "vectors",
        "media-artifact",
        "future-unknown-kind",
    ] {
        let id = engine.store.blob(kind.as_bytes()).unwrap();
        engine
            .store
            .record(
                kind,
                "fixture",
                &json!({"expiresAt":"2000-01-01","nested":[{"hash":id}]}),
            )
            .unwrap();
        protected.push(id);
    }
    fs::write(dir.path().join("a.rs"), "fn current() {}\n").unwrap();
    engine.index(&Deadline::new(10000)).unwrap();
    let report = maintenance::collect(&engine.store, &policy(true), &Deadline::new(10000)).unwrap();
    assert_eq!(report["unreferencedBlobs"], 0);
    assert!(engine.store.snapshot(&first).is_ok());
    for id in protected {
        assert!(engine.store.read_blob(&id).is_ok());
    }
    maintenance::unpin(&engine.store, "keep-first", &Deadline::new(10000)).unwrap();
    assert!(engine.store.snapshot(&first).is_ok());
}

#[test]
fn foreign_store_client_blocks_mutation_until_its_lifecycle_lease_closes() {
    let (dir, engine) = fixture();
    let orphan = engine.store.blob(b"orphan").unwrap();
    let foreign = Engine::open(dir.path()).unwrap();
    assert!(maintenance::collect(&engine.store, &policy(true), &Deadline::new(1000)).is_err());
    assert!(engine.store.read_blob(&orphan).is_ok());
    drop(foreign);
    let report = maintenance::collect(&engine.store, &policy(true), &Deadline::new(10000)).unwrap();
    assert_eq!(report["quarantinedBlobs"], 1);
    assert!(engine.store.read_blob(&orphan).is_err());
    let path = Path::new(report["quarantinePath"].as_str().unwrap()).join(&orphan);
    assert_eq!(fs::read(path).unwrap(), b"orphan");
}

#[test]
fn corrupt_unknown_record_refuses_collection_before_any_blob_moves() {
    let (_dir, engine) = fixture();
    let orphan = engine.store.blob(b"orphan").unwrap();
    engine
        .store
        .db
        .execute(
            "INSERT INTO records VALUES('unknown','fixture','not valid JSON')",
            [],
        )
        .unwrap();
    assert!(maintenance::collect(&engine.store, &policy(true), &Deadline::new(10000)).is_err());
    assert_eq!(engine.store.read_blob(&orphan).unwrap(), b"orphan");
}

#[test]
fn interrupted_quarantine_restores_originals_before_a_new_mark() {
    let (_dir, engine) = fixture();
    let id = engine.store.blob(b"recoverable orphan").unwrap();
    let run_id = hash(b"fixture maintenance run");
    let run = engine.store.dir.join("quarantine").join(&run_id);
    fs::create_dir_all(&run).unwrap();
    fs::write(run.join("manifest.json"),serde_json::to_vec(&json!({"schemaVersion":1,"id":run_id,"workspaceId":engine.store.workspace_id,"createdMs":0,"blobIds":[id]})).unwrap()).unwrap();
    fs::rename(engine.store.dir.join("blobs").join(&id), run.join(&id)).unwrap();
    let retention = Policy {
        apply: true,
        minimum_age_ms: 365 * 86_400_000,
        purge_quarantine: false,
    };
    let report = maintenance::collect(&engine.store, &retention, &Deadline::new(10000)).unwrap();
    assert_eq!(report["restoredBlobs"], 1);
    assert_eq!(engine.store.read_blob(&id).unwrap(), b"recoverable orphan");
}

#[test]
fn expired_deadlines_and_irreversible_purge_requests_leave_orphans_intact() {
    let (_dir, engine) = fixture();
    let orphan = engine.store.blob(b"keep").unwrap();
    let deadline = Deadline::new(0);
    assert!(maintenance::collect(&engine.store, &policy(true), &deadline).is_err());
    assert!(
        maintenance::collect(
            &engine.store,
            &Policy {
                purge_quarantine: true,
                ..policy(true)
            },
            &Deadline::new(10000)
        )
        .is_err()
    );
    assert_eq!(engine.store.read_blob(&orphan).unwrap(), b"keep");
}

#[test]
fn later_blob_hash_failure_restores_earlier_moves_and_rolls_back_metadata() {
    let (_dir, engine) = fixture();
    let a = engine.store.blob(b"first payload").unwrap();
    let b = engine.store.blob(b"second payload").unwrap();
    let mut ids = [a, b];
    ids.sort();
    let first = engine.store.read_blob(&ids[0]).unwrap();
    fs::write(
        engine.store.dir.join("blobs").join(&ids[1]),
        b"corrupt second candidate",
    )
    .unwrap();
    assert!(maintenance::collect(&engine.store, &policy(true), &Deadline::new(10000)).is_err());
    assert_eq!(engine.store.read_blob(&ids[0]).unwrap(), first);
    let count: i64 = engine
        .store
        .db
        .query_row(
            "SELECT count(*) FROM records WHERE kind='maintenance-committed'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(count, 0);
    assert!(
        engine.store.dir.join("quarantine").exists(),
        "Fixture must reach quarantine before the later failure"
    );
}

#[test]
fn malformed_snapshot_shape_is_refused_before_an_orphan_is_moved() {
    let (_dir, engine) = fixture();
    let orphan = engine.store.blob(b"leave intact").unwrap();
    let id = engine.store.latest().unwrap().unwrap();
    engine
        .store
        .db
        .execute("UPDATE snapshots SET value='{}' WHERE id=?1", [id])
        .unwrap();
    assert!(maintenance::collect(&engine.store, &policy(true), &Deadline::new(10000)).is_err());
    assert!(engine.store.read_blob(&orphan).is_ok());
}

#[test]
fn blob_links_are_refused_and_external_targets_are_preserved() {
    let (dir, engine) = fixture();
    let target = dir.path().join("outside.txt");
    fs::write(&target, b"external target").unwrap();
    let link = engine.store.dir.join("blobs").join(hash(b"linked blob"));
    #[cfg(windows)]
    std::os::windows::fs::symlink_file(&target, &link).unwrap();
    #[cfg(unix)]
    std::os::unix::fs::symlink(&target, &link).unwrap();
    assert!(maintenance::collect(&engine.store, &policy(true), &Deadline::new(10000)).is_err());
    assert_eq!(fs::read(target).unwrap(), b"external target");
}

#[test]
fn dry_run_distinguishes_unreferenced_blobs_from_age_eligible_candidates() {
    let (_dir, engine) = fixture();
    engine.store.blob(b"young orphan").unwrap();
    let report =
        maintenance::collect(&engine.store, &Policy::default(), &Deadline::new(10000)).unwrap();
    assert_eq!(report["unreferencedBlobs"], 1);
    assert_eq!(report["eligibleUnreferencedBlobs"], 0);
    assert_eq!(report["deferredYoungBlobs"], 1);
}

#[test]
fn opaque_record_ids_and_map_keys_are_conservative_blob_roots() {
    let (_dir, engine) = fixture();
    let id_root = engine.store.blob(b"record id root").unwrap();
    let key_root = engine.store.blob(b"dictionary root").unwrap();
    engine
        .store
        .record(
            "future-artifact",
            &id_root,
            &json!({"label":"retained metadata"}),
        )
        .unwrap();
    engine
        .store
        .record(
            "future-map",
            "fixture",
            &json!({key_root.clone():{"description":"retained key"}}),
        )
        .unwrap();
    let report = maintenance::collect(&engine.store, &policy(true), &Deadline::new(10000)).unwrap();
    assert_eq!(report["unreferencedBlobs"], 0);
    assert!(engine.store.read_blob(&id_root).is_ok());
    assert!(engine.store.read_blob(&key_root).is_ok());
}

#[test]
fn raw_sqlite_writer_contention_does_not_extend_failed_maintenance_cleanup() {
    let (_dir, engine) = fixture();
    let orphan = engine.store.blob(b"contended orphan").unwrap();
    let foreign = rusqlite::Connection::open(engine.store.dir.join("index.sqlite")).unwrap();
    foreign.execute_batch("BEGIN IMMEDIATE").unwrap();
    let started = Instant::now();
    let error =
        maintenance::collect(&engine.store, &policy(true), &Deadline::new(200)).unwrap_err();
    let elapsed = started.elapsed();
    assert!(
        elapsed < Duration::from_millis(450),
        "Cleanup repeated the busy wait: {elapsed:?}"
    );
    // Recovery inspection is a new read after the measured request has expired.
    engine.store.clear_deadline();
    assert_eq!(
        engine.store.read_blob(&orphan).unwrap(),
        b"contended orphan"
    );
    let prepared: Vec<_> = fs::read_dir(engine.store.dir.join("quarantine"))
        .unwrap()
        .map(|item| item.unwrap().path())
        .collect();
    assert_eq!(prepared.len(), 1);
    let manifest: serde_json::Value =
        serde_json::from_slice(&fs::read(prepared[0].join("manifest.json")).unwrap()).unwrap();
    assert_eq!(manifest["blobIds"], json!([orphan]));
    assert!(
        error.to_string().contains(manifest["id"].as_str().unwrap()),
        "Failure must identify the prepared recovery run"
    );
    foreign.execute_batch("ROLLBACK").unwrap();
    let committed: i64 = engine
        .store
        .db
        .query_row(
            "SELECT count(*) FROM records WHERE kind='maintenance-committed'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(committed, 0);
    let recovery = Policy {
        minimum_age_ms: 365 * 86_400_000,
        ..policy(true)
    };
    let report = maintenance::collect(&engine.store, &recovery, &Deadline::new(10000)).unwrap();
    assert_eq!(report["pendingRecoveryRuns"], 1);
    assert_eq!(report["quarantinedBlobs"], 0);
    assert_eq!(
        engine.store.read_blob(&orphan).unwrap(),
        b"contended orphan"
    );
}
