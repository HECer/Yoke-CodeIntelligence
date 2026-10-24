use serde_json::json;
use std::fs;
use tempfile::TempDir;
use yoke::{
    Engine,
    capture::Deadline,
    maintenance,
    retention::{self, Policy},
};
fn history() -> (TempDir, Engine, String, String) {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("a.rs"), "fn old() {}\n").unwrap();
    let mut engine = Engine::open(dir.path()).unwrap();
    let old = engine.index(&Deadline::new(10_000)).unwrap().0.id;
    fs::write(dir.path().join("a.rs"), "fn current() {}\n").unwrap();
    let current = engine.index(&Deadline::new(10_000)).unwrap().0.id;
    (dir, engine, old, current)
}
#[test]
fn reviewed_history_retirement_removes_only_unrooted_rows_and_preserves_original_source() {
    let (dir, engine, old, current) = history();
    let report =
        retention::plan(&engine.store, &Policy::default(), &Deadline::new(10_000)).unwrap();
    assert_eq!(report["snapshotIds"], json!([old]));
    assert!(engine.store.snapshot(&old).is_ok());
    let id = report["planId"].as_str().unwrap();
    assert!(retention::apply(&engine.store, id, id, false, &Deadline::new(10_000)).is_err());
    assert!(retention::apply(&engine.store, id, "wrong", true, &Deadline::new(10_000)).is_err());
    let result = retention::apply(&engine.store, id, id, true, &Deadline::new(10_000)).unwrap();
    assert_eq!(result["retiredSnapshots"], 1);
    assert_eq!(result["reclaimedBytes"], 0);
    assert!(engine.store.snapshot(&old).is_err());
    assert!(engine.store.snapshot(&current).is_ok());
    assert_eq!(
        fs::read_to_string(dir.path().join("a.rs")).unwrap(),
        "fn current() {}\n"
    );
    assert_eq!(
        retention::apply(&engine.store, id, id, true, &Deadline::new(10_000)).unwrap()["retiredSnapshots"],
        1
    );
}
#[test]
fn pin_and_every_source_bound_workflow_record_protect_history() {
    for kind in [
        "snapshot-pin",
        "editor-session",
        "plan",
        "journal",
        "verification",
        "native_knowledge_v1",
        "artifact",
    ] {
        let (_dir, engine, old, _) = history();
        engine
            .store
            .record(kind, "fixture", &json!({"snapshotId":old}))
            .unwrap();
        let report =
            retention::plan(&engine.store, &Policy::default(), &Deadline::new(10_000)).unwrap();
        assert_eq!(report["snapshotIds"], json!([]), "{kind}");
    }
}
#[test]
fn dashboard_journal_provenance_does_not_retain_source_but_native_fields_still_do() {
    for native_reference in [false, true] {
        let (_dir, engine, old, current) = history();
        let mut row =
            json!({"dashboardDocument":{"accessBinding":old,"path":old,"documentId":old}});
        if native_reference {
            row["snapshotId"] = json!(old);
        }
        engine
            .store
            .record("journal", "owned-provenance-fixture", &row)
            .unwrap();
        let report =
            retention::plan(&engine.store, &Policy::default(), &Deadline::new(10_000)).unwrap();
        assert_eq!(
            report["snapshotIds"],
            if native_reference {
                json!([])
            } else {
                json!([old])
            }
        );
        assert!(engine.store.snapshot(&current).is_ok());
        assert!(engine.store.snapshot(&old).is_ok());
    }
}
#[test]
fn unknown_record_blocks_retirement_even_without_explicit_hash_fields() {
    let (_dir, engine, _, _) = history();
    engine
        .store
        .record(
            "future-opaque",
            "fixture",
            &json!({"dependency":"encoded elsewhere"}),
        )
        .unwrap();
    let report =
        retention::plan(&engine.store, &Policy::default(), &Deadline::new(10_000)).unwrap();
    assert_eq!(report["snapshotIds"], json!([]));
    assert_eq!(report["unknownDependencyKinds"], json!(["future-opaque"]));
}
#[test]
fn stale_plan_and_concurrent_client_leave_history_intact() {
    let (dir, engine, old, _) = history();
    let report =
        retention::plan(&engine.store, &Policy::default(), &Deadline::new(10_000)).unwrap();
    let id = report["planId"].as_str().unwrap();
    let other = Engine::open(dir.path()).unwrap();
    assert!(retention::apply(&engine.store, id, id, true, &Deadline::new(1000)).is_err());
    drop(other);
    maintenance::pin(
        &engine.store,
        &old,
        "new-root",
        "review",
        &Deadline::new(10_000),
    )
    .unwrap();
    assert!(retention::apply(&engine.store, id, id, true, &Deadline::new(10_000)).is_err());
    assert!(engine.store.snapshot(&old).is_ok());
}
#[test]
fn missing_history_age_and_corrupt_metadata_never_become_retirement_permission() {
    let (_dir, engine, old, _) = history();
    engine
        .store
        .db
        .execute(
            "DELETE FROM records WHERE kind='snapshot-published' AND id=?1",
            [&old],
        )
        .unwrap();
    let policy = Policy {
        minimum_age_ms: 1,
        ..Policy::default()
    };
    let report = retention::plan(&engine.store, &policy, &Deadline::new(10_000)).unwrap();
    assert_eq!(report["snapshotIds"], json!([]));
    assert_eq!(report["deferredUnknownAgeSnapshots"], 1);
    engine
        .store
        .db
        .execute(
            "INSERT INTO records VALUES('future-corrupt','fixture','bad JSON')",
            [],
        )
        .unwrap();
    assert!(retention::plan(&engine.store, &Policy::default(), &Deadline::new(10_000)).is_err());
    assert!(engine.store.snapshot(&old).is_ok());
}
#[test]
fn purge_reclaims_only_exact_committed_unreachable_quarantine_bytes_after_confirmation() {
    let (_dir, engine, _, _) = history();
    let orphan = engine.store.blob(b"elevenbytes").unwrap();
    let moved = maintenance::collect(
        &engine.store,
        &maintenance::Policy {
            apply: true,
            minimum_age_ms: 0,
            purge_quarantine: false,
        },
        &Deadline::new(10_000),
    )
    .unwrap();
    let path = std::path::Path::new(moved["quarantinePath"].as_str().unwrap()).join(&orphan);
    let report = retention::purge_plan(&engine.store, 0, &Deadline::new(10_000)).unwrap();
    assert_eq!(report["eligibleBytes"], 11);
    let id = report["planId"].as_str().unwrap();
    assert!(retention::purge(&engine.store, id, id, false, &Deadline::new(10_000)).is_err());
    assert!(path.is_file());
    let result = retention::purge(&engine.store, id, id, true, &Deadline::new(10_000)).unwrap();
    assert_eq!(result["reclaimedBytes"], 11);
    assert_eq!(result["complete"], true);
    assert!(!path.exists());
    assert!(path.parent().unwrap().join("manifest.json").exists());
}
#[test]
fn newly_rooted_quarantine_and_tampered_plan_refuse_purge_without_deleting_bytes() {
    let (_dir, engine, _, _) = history();
    let orphan = engine.store.blob(b"keep me").unwrap();
    let moved = maintenance::collect(
        &engine.store,
        &maintenance::Policy {
            apply: true,
            minimum_age_ms: 0,
            purge_quarantine: false,
        },
        &Deadline::new(10_000),
    )
    .unwrap();
    let report = retention::purge_plan(&engine.store, 0, &Deadline::new(10_000)).unwrap();
    let id = report["planId"].as_str().unwrap();
    engine
        .store
        .record(
            "artifact",
            &orphan,
            &json!({"artifactId":orphan,"totalBytes":7}),
        )
        .unwrap();
    assert!(retention::purge(&engine.store, id, id, true, &Deadline::new(10_000)).is_err());
    assert_eq!(
        fs::read(std::path::Path::new(moved["quarantinePath"].as_str().unwrap()).join(&orphan))
            .unwrap(),
        b"keep me"
    );
}

#[test]
fn source_bindings_inside_published_json_artifacts_protect_history() {
    let (_dir, engine, old, _) = history();
    engine
        .store
        .artifact(
            &serde_json::to_vec(&json!({"snapshotId":old,"sourceBindings":[{"snapshotId":old}]}))
                .unwrap(),
        )
        .unwrap();
    let report =
        retention::plan(&engine.store, &Policy::default(), &Deadline::new(10_000)).unwrap();
    assert_eq!(report["snapshotIds"], json!([]));
}
#[test]
fn opaque_published_artifacts_defer_history() {
    let (_dir, engine, _, _) = history();
    engine
        .store
        .artifact(b"opaque source-dependent export")
        .unwrap();
    let report =
        retention::plan(&engine.store, &Policy::default(), &Deadline::new(10_000)).unwrap();
    assert_eq!(report["snapshotIds"], json!([]));
    assert!(
        !report["unknownDependencyKinds"]
            .as_array()
            .unwrap()
            .is_empty()
    );
}
#[test]
fn future_opaque_json_artifact_schema_is_not_inferred_to_be_dependency_free() {
    let (_dir, engine, _, _) = history();
    engine
        .store
        .artifact(
            &serde_json::to_vec(
                &json!({"futureEncodedDependency":"opaque dependency representation"}),
            )
            .unwrap(),
        )
        .unwrap();
    let report =
        retention::plan(&engine.store, &Policy::default(), &Deadline::new(10_000)).unwrap();
    assert_eq!(report["snapshotIds"], json!([]));
    assert_eq!(
        report["unknownDependencyKinds"],
        json!(["artifact:unknown-shape"])
    );
}

#[test]
fn unknown_nested_snapshot_fields_defer_history() {
    let (_dir, engine, old, _) = history();
    let mut snapshot = serde_json::to_value(engine.store.snapshot(&old).unwrap()).unwrap();
    snapshot["files"][0]["futureDependencies"] = json!({"encoded":"opaque"});
    engine
        .store
        .db
        .execute(
            "UPDATE snapshots SET value=?1 WHERE id=?2",
            rusqlite::params![serde_json::to_string(&snapshot).unwrap(), old],
        )
        .unwrap();
    let report =
        retention::plan(&engine.store, &Policy::default(), &Deadline::new(10_000)).unwrap();
    assert_eq!(report["snapshotIds"], json!([]));
}
#[cfg(windows)]
#[test]
fn partial_purge_reports_actual_removed_bytes_and_can_resume_only_its_exact_intent() {
    use std::os::windows::fs::OpenOptionsExt;
    let (_dir, engine, _, _) = history();
    let a = engine.store.blob(b"first removable bytes").unwrap();
    let b = engine.store.blob(b"second protected bytes").unwrap();
    let moved = maintenance::collect(
        &engine.store,
        &maintenance::Policy {
            apply: true,
            minimum_age_ms: 0,
            purge_quarantine: false,
        },
        &Deadline::new(10_000),
    )
    .unwrap();
    let root = std::path::Path::new(moved["quarantinePath"].as_str().unwrap());
    let mut ids = [a, b];
    ids.sort();
    let first_size = fs::metadata(root.join(&ids[0])).unwrap().len();
    let lock = fs::OpenOptions::new()
        .read(true)
        .share_mode(3)
        .open(root.join(&ids[1]))
        .unwrap();
    let report = retention::purge_plan(&engine.store, 0, &Deadline::new(10_000)).unwrap();
    let id = report["planId"].as_str().unwrap();
    let result = retention::purge(&engine.store, id, id, true, &Deadline::new(10_000)).unwrap();
    assert_eq!(result["complete"], false);
    assert_eq!(result["reclaimedBytes"], first_size);
    assert!(!root.join(&ids[0]).exists());
    assert!(root.join(&ids[1]).exists());
    let pending = retention::purge_plan(&engine.store, 0, &Deadline::new(10_000)).unwrap();
    assert_eq!(
        pending["items"],
        json!([]),
        "Unfinished purge intent must retain remaining bytes for other plans"
    );
    drop(lock);
    let complete = retention::purge(&engine.store, id, id, true, &Deadline::new(10_000)).unwrap();
    assert_eq!(complete["complete"], true);
    assert!(!root.join(&ids[1]).exists());
}

#[test]
fn committed_retirement_can_feed_conservative_cas_quarantine_and_purge_without_losing_receipts() {
    let (_dir, engine, old, _) = history();
    let report =
        retention::plan(&engine.store, &Policy::default(), &Deadline::new(10_000)).unwrap();
    let id = report["planId"].as_str().unwrap();
    retention::apply(&engine.store, id, id, true, &Deadline::new(10_000)).unwrap();
    let quarantine = maintenance::collect(
        &engine.store,
        &maintenance::Policy {
            apply: true,
            minimum_age_ms: 0,
            purge_quarantine: false,
        },
        &Deadline::new(10_000),
    )
    .unwrap();
    assert_eq!(quarantine["quarantinedBlobs"], 1);
    let purge = retention::purge_plan(&engine.store, 0, &Deadline::new(10_000)).unwrap();
    let pid = purge["planId"].as_str().unwrap();
    assert_eq!(
        retention::purge(&engine.store, pid, pid, true, &Deadline::new(10_000)).unwrap()["complete"],
        true
    );
    assert!(engine.store.snapshot(&old).is_err());
    assert_eq!(
        retention::apply(&engine.store, id, id, true, &Deadline::new(10_000)).unwrap()["retiredSnapshots"],
        1
    );
}

#[test]
fn labeled_uncommitted_history_journal_restores_only_hash_bound_rows() {
    let (_dir, engine, old, _) = history();
    let report =
        retention::plan(&engine.store, &Policy::default(), &Deadline::new(10_000)).unwrap();
    let id = report["planId"].as_str().unwrap();
    let applied = retention::apply(&engine.store, id, id, true, &Deadline::new(10_000)).unwrap();
    // A labeled journal-recovery fixture, not a simulated power-loss guarantee.
    engine
        .store
        .db
        .execute(
            "DELETE FROM records WHERE kind='retention-committed' AND id=?1",
            [id],
        )
        .unwrap();
    let path =
        std::path::Path::new(applied["quarantinePath"].as_str().unwrap()).join("manifest.json");
    let mut manifest: serde_json::Value =
        serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    manifest["documents"][0][2] = json!("corrupt source search text");
    fs::write(&path, serde_json::to_vec(&manifest).unwrap()).unwrap();
    assert!(retention::recover(&engine.store, &Deadline::new(10_000)).is_err());
    assert!(engine.store.snapshot(&old).is_err());
}

#[test]
fn labeled_uncommitted_history_journal_restores_missing_rows_without_changing_current_source() {
    let (dir, engine, old, current) = history();
    let report =
        retention::plan(&engine.store, &Policy::default(), &Deadline::new(10_000)).unwrap();
    let id = report["planId"].as_str().unwrap();
    retention::apply(&engine.store, id, id, true, &Deadline::new(10_000)).unwrap();
    engine
        .store
        .db
        .execute(
            "DELETE FROM records WHERE kind='retention-committed' AND id=?1",
            [id],
        )
        .unwrap();
    let report = retention::recover(&engine.store, &Deadline::new(10_000)).unwrap();
    assert_eq!(report["restoredRows"], 2);
    assert!(engine.store.snapshot(&old).is_ok());
    assert_eq!(engine.store.latest().unwrap().unwrap(), current);
    assert_eq!(
        fs::read_to_string(dir.path().join("a.rs")).unwrap(),
        "fn current() {}\n"
    );
}

#[test]
fn multiply_linked_quarantine_content_is_unknown_physical_reclamation_and_is_preserved() {
    let (dir, engine, _, _) = history();
    let orphan = engine.store.blob(b"linked customer bytes").unwrap();
    fs::hard_link(
        engine.store.dir.join("blobs").join(&orphan),
        dir.path().join("customer-copy.txt"),
    )
    .unwrap();
    let moved = maintenance::collect(
        &engine.store,
        &maintenance::Policy {
            apply: true,
            minimum_age_ms: 0,
            purge_quarantine: false,
        },
        &Deadline::new(10_000),
    )
    .unwrap();
    assert!(retention::purge_plan(&engine.store, 0, &Deadline::new(10_000)).is_err());
    assert_eq!(
        fs::read(dir.path().join("customer-copy.txt")).unwrap(),
        b"linked customer bytes"
    );
    assert!(
        std::path::Path::new(moved["quarantinePath"].as_str().unwrap())
            .join(orphan)
            .is_file()
    );
}
