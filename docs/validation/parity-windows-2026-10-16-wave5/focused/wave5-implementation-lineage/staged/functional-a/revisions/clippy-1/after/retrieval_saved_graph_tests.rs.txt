//! Uncompiled/unexecuted drafts. Every corruption starts from a genuine small
//! Engine publication and changes only its owned test SQLite authority row.

use super::*;
use crate::{
    Engine,
    model::{Graph, Snapshot, identity},
};
use rusqlite::{
    Connection,
    hooks::{AuthAction, AuthContext, Authorization},
    limits::Limit,
};
use serde_json::json;
use std::{
    fs,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    time::Duration,
};
use tempfile::TempDir;

struct Fixture {
    engine: Engine,
    snapshot: Snapshot,
    graph: Graph,
    _root: TempDir,
}
impl Fixture {
    fn new() -> Self {
        let root = TempDir::new().unwrap();
        fs::write(root.path().join("lib.rs"), "fn f(){}\n").unwrap();
        let mut engine = Engine::open(root.path()).unwrap();
        let (snapshot, graph, _, _) = engine.index(&Deadline::new(30_000)).unwrap();
        assert!(serde_json::to_vec(&graph).unwrap().len() < 16_384);
        Self {
            engine,
            snapshot,
            graph,
            _root: root,
        }
    }
    fn check(&self, d: &Deadline) -> Result<(), RetrievalHardError> {
        let tx = self.engine.store.db.unchecked_transaction().unwrap();
        let result = verify_saved_graph_in(
            &self.engine.store,
            &tx,
            &self.snapshot.id,
            &identity(&self.graph),
            d,
        );
        drop(tx); // Read-only rollback; helper cannot commit its borrowed caller transaction.
        result
    }
    fn replace_text(&self, raw: &str) {
        self.engine
            .store
            .db
            .execute(
                "UPDATE graphs SET value=?1 WHERE id=?2",
                rusqlite::params![raw, self.snapshot.id],
            )
            .unwrap();
    }
}
fn capacity(result: Result<(), RetrievalHardError>) {
    assert!(
        matches!(result, Err(RetrievalHardError::Capacity(_))),
        "{result:?}"
    );
}
fn integrity(result: Result<(), RetrievalHardError>) {
    assert!(
        matches!(
            result,
            Err(RetrievalHardError::SourceIntegrity(_)) | Err(RetrievalHardError::Authority(_))
        ),
        "{result:?}"
    );
}
fn clear(store: &Store) {
    store
        .db
        .authorizer(None::<fn(AuthContext<'_>) -> Authorization>);
}

#[test]
fn valid_saved_graph_restores_the_exact_lower_host_sqlite_limit() {
    let f = Fixture::new();
    let kind = Limit::SQLITE_LIMIT_LENGTH;
    let original = f.engine.store.db.set_limit(kind, 16_384).unwrap();
    let before = f.engine.store.db.limit(kind).unwrap();
    f.check(&Deadline::new(10_000)).unwrap();
    assert_eq!(
        f.engine.store.db.limit(kind).unwrap(),
        before,
        "do not restore a generic default or raise host policy"
    );
    f.engine.store.db.set_limit(kind, original).unwrap();
}

#[test]
fn missing_saved_row_is_none_for_readers_and_source_integrity_for_verifier() {
    let f = Fixture::new();
    let d = Deadline::new(10_000);
    f.check(&d).unwrap(); // Qualify the genuine saved authority before deletion.
    let expected = identity(&f.graph);
    assert_eq!(
        f.engine
            .store
            .db
            .execute("DELETE FROM graphs WHERE id=?1", [&f.snapshot.id])
            .unwrap(),
        1,
        "delete only this fixture's genuine Graph row"
    );

    assert!(
        read_graph(&f.engine.store, &f.snapshot.id, &d)
            .unwrap()
            .is_none()
    );
    assert!(
        f.engine.store.db.is_autocommit(),
        "short reader must close its own read transaction"
    );

    let tx = f.engine.store.db.unchecked_transaction().unwrap();
    assert!(
        read_graph_in(&f.engine.store, &tx, &f.snapshot.id, &d)
            .unwrap()
            .is_none()
    );
    assert!(
        !f.engine.store.db.is_autocommit(),
        "missing-row read must retain the caller-owned transaction"
    );
    let result = verify_saved_graph_in(&f.engine.store, &tx, &f.snapshot.id, &expected, &d);
    assert!(
        !f.engine.store.db.is_autocommit(),
        "missing authority must not end the caller-owned transaction"
    );
    assert_eq!(
        result,
        Err(RetrievalHardError::SourceIntegrity("saved_graph_missing"))
    );
    drop(tx);
    assert!(f.engine.store.db.is_autocommit());
}

#[test]
fn wrong_storage_type_is_hard_and_restores_host_limit() {
    let f = Fixture::new();
    f.check(&Deadline::new(10_000)).unwrap();
    f.engine
        .store
        .db
        .execute(
            "UPDATE graphs SET value=zeroblob(2) WHERE id=?1",
            [&f.snapshot.id],
        )
        .unwrap();
    let kind = Limit::SQLITE_LIMIT_LENGTH;
    let original = f.engine.store.db.set_limit(kind, 16_384).unwrap();
    let storage: String = f
        .engine
        .store
        .db
        .query_row(
            "SELECT typeof(value) FROM graphs WHERE id=?1",
            [&f.snapshot.id],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(
        storage, "blob",
        "SQLite text affinity must not silently repair fixture type"
    );
    integrity(f.check(&Deadline::new(10_000)));
    assert_eq!(f.engine.store.db.limit(kind).unwrap(), 16_384);
    f.engine.store.db.set_limit(kind, original).unwrap();
}

#[test]
fn malformed_saved_authority_is_hard_without_parser_contents_or_auto_fallback() {
    let f = Fixture::new();
    f.check(&Deadline::new(10_000)).unwrap();
    f.replace_text(r#"{"private":"SAVED_GRAPH_SECRET_DO_NOT_ECHO","broken":"#);
    let before = f.engine.store.db.limit(Limit::SQLITE_LIMIT_LENGTH).unwrap();
    let error = f.check(&Deadline::new(10_000)).unwrap_err();
    assert!(
        matches!(
            error,
            RetrievalHardError::SourceIntegrity(_) | RetrievalHardError::Authority(_)
        ),
        "{error}"
    );
    assert!(!error.to_string().contains("SAVED_GRAPH_SECRET"));
    assert_eq!(
        f.engine.store.db.limit(Limit::SQLITE_LIMIT_LENGTH).unwrap(),
        before
    );
    let error = crate::retrieval::find_compatibility_auto(
        &f.engine.store,
        &f.snapshot,
        &f.graph,
        &json!({"query":"f"}),
        &Deadline::new(10_000),
        4_000_000,
    )
    .unwrap_err();
    assert!(
        matches!(
            error.downcast_ref::<RetrievalHardError>(),
            Some(RetrievalHardError::SourceIntegrity(_)) | Some(RetrievalHardError::Authority(_))
        ),
        "{error:#}"
    );
    assert!(!format!("{error:#}").contains("SAVED_GRAPH_SECRET"));
}

#[test]
fn denied_payload_read_restores_limit_on_error_and_stays_typed_hard() {
    let f = Fixture::new();
    f.check(&Deadline::new(10_000)).unwrap();
    let reads = Arc::new(AtomicUsize::new(0));
    let observed = reads.clone();
    // Required bounded-read pattern: length+typeof metadata reads the value
    // twice, then a separate raw-payload SELECT is the third value-column read.
    // This is an I/O sequencing oracle, not a source-authority grant.
    f.engine
        .store
        .db
        .authorizer(Some(move |ctx: AuthContext<'_>| {
            if let AuthAction::Read {
                table_name: "graphs",
                column_name: "value",
            } = ctx.action
                && observed.fetch_add(1, Ordering::SeqCst) >= 2
            {
                return Authorization::Deny;
            }
            Authorization::Allow
        }));
    let before = f.engine.store.db.limit(Limit::SQLITE_LIMIT_LENGTH).unwrap();
    let result = f.check(&Deadline::new(10_000));
    clear(&f.engine.store);
    assert!(
        reads.load(Ordering::SeqCst) >= 3,
        "must reach actual raw-payload read stage"
    );
    assert_eq!(
        result,
        Err(RetrievalHardError::Internal("saved_graph_read"))
    );
    assert_eq!(
        f.engine.store.db.limit(Limit::SQLITE_LIMIT_LENGTH).unwrap(),
        before
    );
}

#[test]
fn cancellation_after_metadata_restores_limit_without_resetting_the_deadline() {
    let f = Fixture::new();
    f.check(&Deadline::new(10_000)).unwrap();
    let parent = Deadline::new(30_000);
    let cancel = Arc::new(AtomicBool::new(false));
    Deadline::run_bounded(&parent, cancel.clone(), || {
        let d = Deadline::new(30_000);
        let flag = cancel.clone();
        let reads = Arc::new(AtomicUsize::new(0));
        let observed = reads.clone();
        f.engine
            .store
            .db
            .authorizer(Some(move |ctx: AuthContext<'_>| {
                if let AuthAction::Read {
                    table_name: "graphs",
                    column_name: "value",
                } = ctx.action
                    && observed.fetch_add(1, Ordering::SeqCst) >= 2
                {
                    flag.store(true, Ordering::SeqCst);
                }
                Authorization::Allow
            }));
        let before = f.engine.store.db.limit(Limit::SQLITE_LIMIT_LENGTH).unwrap();
        let result = f.check(&d);
        clear(&f.engine.store);
        assert!(reads.load(Ordering::SeqCst) >= 3);
        assert!(
            matches!(result, Err(RetrievalHardError::Cancelled(_))),
            "{result:?}"
        );
        assert_eq!(
            f.engine.store.db.limit(Limit::SQLITE_LIMIT_LENGTH).unwrap(),
            before
        );
        // Preserve all cancellation/limit assertions before clearing the shared
        // flag so run_bounded's own exit check can complete fixture cleanup.
        cancel.store(false, Ordering::SeqCst);
        Ok(())
    })
    .unwrap();
}

#[test]
fn transaction_from_another_store_connection_is_not_a_checked_read_for_this_store() {
    let f = Fixture::new();
    let d = Deadline::new(10_000);
    f.check(&d).unwrap();
    let other = Store::open(&f.engine.root).unwrap();
    assert_eq!(other.workspace_id, f.engine.store.workspace_id);
    let tx = other.db.unchecked_transaction().unwrap();
    assert!(matches!(
        verify_saved_graph_in(
            &f.engine.store,
            &tx,
            &f.snapshot.id,
            &identity(&f.graph),
            &d
        ),
        Err(RetrievalHardError::Authority(_))
    ));
}

#[test]
fn metadata_and_payload_share_the_same_wal_read_snapshot() {
    let f = Fixture::new();
    let d = Deadline::new(30_000);
    f.check(&d).unwrap();
    let mode: String = f
        .engine
        .store
        .db
        .query_row("PRAGMA journal_mode", [], |r| r.get(0))
        .unwrap();
    assert_eq!(
        mode.to_ascii_lowercase(),
        "wal",
        "real Store must qualify WAL fixture"
    );
    let writer = Connection::open(f.engine.store.db.path().unwrap()).unwrap();
    writer.busy_timeout(Duration::from_millis(250)).unwrap();
    let id = f.snapshot.id.clone();
    let changed = Arc::new(AtomicBool::new(false));
    let done = changed.clone();
    let reads = Arc::new(AtomicUsize::new(0));
    let observed = reads.clone();
    f.engine
        .store
        .db
        .authorizer(Some(move |ctx: AuthContext<'_>| {
            if let AuthAction::Read {
                table_name: "graphs",
                column_name: "value",
            } = ctx.action
                && observed.fetch_add(1, Ordering::SeqCst) == 2
            {
                writer
                    .execute("UPDATE graphs SET value='{bad' WHERE id=?1", [&id])
                    .unwrap();
                done.store(true, Ordering::SeqCst);
            }
            Authorization::Allow
        }));
    let result = f.check(&d);
    clear(&f.engine.store);
    assert!(
        changed.load(Ordering::SeqCst),
        "writer must commit BETWEEN metadata and payload SELECT"
    );
    assert!(reads.load(Ordering::SeqCst) >= 3);
    result.unwrap(); // Existing pinned read snapshot sees the original valid row.
    integrity(f.check(&d)); // A new transaction must observe the committed corruption.
}

#[test]
#[ignore = "qualified SQLite64MB row fixture:reserve>=512MB RAM/1GB disk; not executed"]
fn oversized_saved_row_refuses_before_raw_payload_read_and_auto_fallback() {
    let f = Fixture::new();
    let d = Deadline::new(60_000);
    f.check(&d).unwrap();
    // SQLite builds the explicit corrupt fixture; no64MB Rust String or fake
    // published Graph/source manifest is manufactured.
    f.engine
        .store
        .db
        .execute(
            "UPDATE graphs SET value=CAST(zeroblob(64000001) AS TEXT) WHERE id=?1",
            [&f.snapshot.id],
        )
        .unwrap();
    let raw_size: usize = f
        .engine
        .store
        .db
        .query_row(
            "SELECT length(CAST(value AS BLOB)) FROM graphs WHERE id=?1",
            [&f.snapshot.id],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(raw_size, 64_000_001);
    let reads = Arc::new(AtomicUsize::new(0));
    let observed = reads.clone();
    f.engine
        .store
        .db
        .authorizer(Some(move |ctx: AuthContext<'_>| {
            if let AuthAction::Read {
                table_name: "graphs",
                column_name: "value",
            } = ctx.action
                && observed.fetch_add(1, Ordering::SeqCst) >= 2
            {
                return Authorization::Deny;
            }
            Authorization::Allow
        }));
    let before = f.engine.store.db.limit(Limit::SQLITE_LIMIT_LENGTH).unwrap();
    let result = f.check(&d);
    clear(&f.engine.store);
    assert_eq!(
        result,
        Err(RetrievalHardError::Capacity("graph_json_bytes"))
    );
    assert!(
        reads.load(Ordering::SeqCst) <= 2,
        "raw payload SELECT must never be prepared"
    );
    assert_eq!(
        f.engine.store.db.limit(Limit::SQLITE_LIMIT_LENGTH).unwrap(),
        before
    );
    let error = crate::retrieval::find_compatibility_auto(
        &f.engine.store,
        &f.snapshot,
        &f.graph,
        &json!({"query":"f"}),
        &d,
        4_000_000,
    )
    .unwrap_err();
    assert_eq!(
        error.downcast_ref::<RetrievalHardError>(),
        Some(&RetrievalHardError::Capacity("graph_json_bytes"))
    );
    assert_eq!(
        f.engine.store.db.limit(Limit::SQLITE_LIMIT_LENGTH).unwrap(),
        before
    );
    // Preserve a stricter host connection limit as well, even when SQLite
    // itself refuses the corrupt large record before its metadata can be read.
    f.engine
        .store
        .db
        .set_limit(Limit::SQLITE_LIMIT_LENGTH, 16_384)
        .unwrap();
    capacity(f.check(&d));
    assert_eq!(
        f.engine.store.db.limit(Limit::SQLITE_LIMIT_LENGTH).unwrap(),
        16_384
    );
    f.engine
        .store
        .db
        .set_limit(Limit::SQLITE_LIMIT_LENGTH, before)
        .unwrap();
}
