//! Retrieval-only bounded saved Graph reads in the caller's pinned transaction.

use crate::{
    capture::Deadline,
    model::{Graph, valid_id},
    retrieval_capacity::{MAX_GRAPH_BYTES, bounded_graph_fingerprint},
    retrieval_scopes::{RetrievalHardError, RetrievalResult, check_deadline},
    store::Store,
};
use rusqlite::{Connection, OptionalExtension, Transaction, limits::Limit, types::ValueRef};

#[path = "retrieval_saved_graph_decode.rs"]
mod decode;

fn database<T>(result: rusqlite::Result<T>, deadline: &Deadline) -> RetrievalResult<T> {
    // Callbacks may set the inherited cancellation while preparing or stepping.
    // Preserve that typed cause instead of classifying the resulting SQLite error.
    check_deadline(deadline)?;
    result.map_err(|error| match error {
        rusqlite::Error::SqliteFailure(cause, _) if cause.code == rusqlite::ErrorCode::TooBig => {
            RetrievalHardError::Capacity("graph_json_bytes")
        }
        _ => RetrievalHardError::Internal("saved_graph_read"),
    })
}

struct LengthLimit<'a> {
    connection: &'a Connection,
    previous: i32,
    active: bool,
}
impl<'a> LengthLimit<'a> {
    fn enter(connection: &'a Connection) -> RetrievalResult<Self> {
        let previous = connection
            .limit(Limit::SQLITE_LIMIT_LENGTH)
            .map_err(|_| RetrievalHardError::Internal("saved_graph_limit"))?;
        // Construct the guard before mutating the connection, so unwinding from
        // any subsequent operation also attempts exact restoration.
        let guard = Self {
            connection,
            previous,
            active: true,
        };
        let lowered = previous.min((MAX_GRAPH_BYTES + 4096) as i32);
        connection
            .set_limit(Limit::SQLITE_LIMIT_LENGTH, lowered)
            .map_err(|_| RetrievalHardError::Internal("saved_graph_limit"))?;
        if connection
            .limit(Limit::SQLITE_LIMIT_LENGTH)
            .map_err(|_| RetrievalHardError::Internal("saved_graph_limit"))?
            != lowered
        {
            return Err(RetrievalHardError::Internal("saved_graph_limit"));
        }
        Ok(guard)
    }
    fn restore(&mut self) -> RetrievalResult<()> {
        if !self.active {
            return Ok(());
        }
        let restored = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            self.connection
                .set_limit(Limit::SQLITE_LIMIT_LENGTH, self.previous)?;
            self.connection.limit(Limit::SQLITE_LIMIT_LENGTH)
        }));
        if !matches!(restored, Ok(Ok(value)) if value == self.previous) {
            return Err(RetrievalHardError::Internal("saved_graph_limit_restore"));
        }
        self.active = false;
        Ok(())
    }
}
impl Drop for LengthLimit<'_> {
    fn drop(&mut self) {
        // Explicit result paths report failure. Drop must not cause a second
        // panic during unwinding, but still attempts the same exact restoration.
        let _ = self.restore();
    }
}

// None means only an absent saved row. Returned model data grants no source
// authority. The exact Store connection must own the borrowed transaction.
pub(crate) fn read_graph_in(
    store: &Store,
    read: &Transaction<'_>,
    snapshot_id: &str,
    deadline: &Deadline,
) -> RetrievalResult<Option<Graph>> {
    // A Transaction from an equal workspace on another connection is not this
    // Store's authority. Check before touching either connection's host limits.
    if !std::ptr::eq(&store.db, &**read) || store.db.is_autocommit() {
        return Err(RetrievalHardError::Authority("saved_graph_transaction"));
    }
    check_deadline(deadline)?;
    if !valid_id(snapshot_id) {
        return Err(RetrievalHardError::Authority("snapshot_id"));
    }
    let mut limit = LengthLimit::enter(&store.db)?;
    let result = read_bounded(read, snapshot_id, deadline);
    // All statements and row borrows have ended before this exact restoration.
    // A restoration failure must never masquerade as successful verification.
    limit.restore()?;
    result
}

fn read_bounded(
    read: &Transaction<'_>,
    snapshot_id: &str,
    deadline: &Deadline,
) -> RetrievalResult<Option<Graph>> {
    check_deadline(deadline)?;
    let metadata = database(
        read.query_row(
            "SELECT length(CAST(value AS BLOB)),typeof(value) FROM graphs WHERE id=?1",
            [snapshot_id],
            |row| Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?)),
        )
        .optional(),
        deadline,
    )?;
    let Some((bytes, storage_type)) = metadata else {
        return Ok(None);
    };
    if bytes < 0 {
        return Err(RetrievalHardError::SourceIntegrity("saved_graph_size"));
    }
    if bytes > MAX_GRAPH_BYTES as i64 {
        return Err(RetrievalHardError::Capacity("graph_json_bytes"));
    }
    if storage_type != "text" {
        return Err(RetrievalHardError::SourceIntegrity("saved_graph_type"));
    }
    check_deadline(deadline)?;
    // Keep a separate payload query so metadata bounds are enforced before its
    // prepare/step. Both queries see the same explicit WAL read transaction.
    let mut statement = database(
        read.prepare("SELECT value FROM graphs WHERE id=?1"),
        deadline,
    )?;
    let mut rows = database(statement.query([snapshot_id]), deadline)?;
    let row = database(rows.next(), deadline)?.ok_or(RetrievalHardError::SourceIntegrity(
        "saved_graph_disappeared",
    ))?;
    let payload = database(row.get_ref(0), deadline)?;
    let ValueRef::Text(payload) = payload else {
        return Err(RetrievalHardError::SourceIntegrity("saved_graph_type"));
    };
    if payload.len() > MAX_GRAPH_BYTES {
        return Err(RetrievalHardError::Capacity("graph_json_bytes"));
    }
    if payload.len() != bytes as usize {
        return Err(RetrievalHardError::SourceIntegrity("saved_graph_size"));
    }
    // Borrow SQLite's bounded row bytes only for decoding; no full Rust String
    // or duplicate Value tree is allocated. The returned Graph is owned data.
    let graph = decode::decode(payload, deadline)?;
    if graph.snapshot_id != snapshot_id {
        return Err(RetrievalHardError::Authority("saved_graph_snapshot"));
    }
    check_deadline(deadline)?;
    Ok(Some(graph))
}

// Short read-transaction convenience wrapper for retrieval-only selection.
pub(crate) fn read_graph(
    store: &Store,
    snapshot_id: &str,
    deadline: &Deadline,
) -> RetrievalResult<Option<Graph>> {
    check_deadline(deadline)?;
    let read = database(store.db.unchecked_transaction(), deadline)?;
    let result = read_graph_in(store, &read, snapshot_id, deadline);
    let closed = database(read.rollback(), deadline);
    // A failed transaction close is a hard state/read failure even if the row
    // itself was valid. This wrapper never commits caller-owned state.
    closed?;
    result
}

// The expected fingerprint is derived from the admitted supplied Graph stream,
// never request JSON. Verification constructs no run or source capability.
pub(crate) fn verify_saved_graph_in(
    store: &Store,
    read: &Transaction<'_>,
    snapshot_id: &str,
    expected_graph_fingerprint: &str,
    deadline: &Deadline,
) -> RetrievalResult<()> {
    let graph = read_graph_in(store, read, snapshot_id, deadline)?
        .ok_or(RetrievalHardError::SourceIntegrity("saved_graph_missing"))?;
    if bounded_graph_fingerprint(&graph, deadline)? != expected_graph_fingerprint {
        return Err(RetrievalHardError::Authority("saved_graph_fingerprint"));
    }
    check_deadline(deadline)
}

#[cfg(test)]
#[path = "retrieval_saved_graph_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "retrieval_saved_graph_schema_tests.rs"]
mod schema_tests;
