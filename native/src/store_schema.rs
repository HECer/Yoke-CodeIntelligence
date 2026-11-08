//! Bootstrap all Store tables atomically without renewing the opening policy.
use crate::capture::Deadline;
use anyhow::Result;
use rusqlite::{Connection, Transaction, TransactionBehavior};

// These are the original schema statements. Connection PRAGMAs are configured
// by the caller before this transaction, with its busy/progress policy active.
const SCHEMA: &str = "CREATE TABLE IF NOT EXISTS meta(key TEXT PRIMARY KEY,value TEXT NOT NULL);
            CREATE TABLE IF NOT EXISTS snapshots(id TEXT PRIMARY KEY,value TEXT NOT NULL);
            CREATE TABLE IF NOT EXISTS graphs(id TEXT PRIMARY KEY,value TEXT NOT NULL);
            CREATE TABLE IF NOT EXISTS cache(key TEXT PRIMARY KEY,value TEXT NOT NULL);
            CREATE TABLE IF NOT EXISTS records(kind TEXT NOT NULL,id TEXT NOT NULL,value TEXT NOT NULL,PRIMARY KEY(kind,id));
            CREATE VIRTUAL TABLE IF NOT EXISTS symbols USING fts5(snapshot UNINDEXED,id UNINDEXED,name,path UNINDEXED,signature);
            CREATE VIRTUAL TABLE IF NOT EXISTS documents USING fts5(snapshot UNINDEXED,path UNINDEXED,text);";

pub(super) fn initialize(db: &mut Connection, deadline: &Deadline) -> Result<()> {
    deadline.check()?;
    // Keep existing-schema opens from reserving a writer before any DDL needs
    // to create a table. SQLite and the caller's original policy still govern
    // any write acquisition; no fresh busy timeout or retry is installed here.
    let tx = Transaction::new(db, TransactionBehavior::Deferred)?;
    tx.execute_batch(SCHEMA)?;
    deadline.check()?;
    tx.commit()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    };

    fn connection(root: &std::path::Path) -> Connection {
        let db = Connection::open(root.join("schema.sqlite")).unwrap();
        db.execute_batch(
            "PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON; PRAGMA synchronous=FULL;",
        )
        .unwrap();
        db
    }

    fn user_schema_objects(db: &Connection) -> i64 {
        db.query_row(
            "SELECT count(*) FROM sqlite_master WHERE name NOT LIKE 'sqlite_%'",
            [],
            |row| row.get(0),
        )
        .unwrap()
    }

    fn assert_schema_and_policy(db: &Connection) {
        let names: Vec<String> = db
            .prepare("SELECT name FROM sqlite_master WHERE name IN ('meta','snapshots','graphs','cache','records','symbols','documents') ORDER BY name")
            .unwrap()
            .query_map([], |row| row.get(0))
            .unwrap()
            .collect::<rusqlite::Result<_>>()
            .unwrap();
        assert_eq!(
            names,
            [
                "cache",
                "documents",
                "graphs",
                "meta",
                "records",
                "snapshots",
                "symbols"
            ]
        );
        let journal: String = db
            .query_row("PRAGMA journal_mode", [], |r| r.get(0))
            .unwrap();
        let synchronous: i64 = db
            .query_row("PRAGMA synchronous", [], |r| r.get(0))
            .unwrap();
        let foreign_keys: i64 = db
            .query_row("PRAGMA foreign_keys", [], |r| r.get(0))
            .unwrap();
        assert_eq!(journal, "wal");
        assert_eq!(synchronous, 2);
        assert_eq!(foreign_keys, 1);
    }

    #[test]
    fn bootstrap_has_one_commit_and_keeps_all_schema_and_durability_settings() {
        let root = tempfile::tempdir().unwrap();
        let mut db = connection(root.path());
        let commits = Arc::new(AtomicUsize::new(0));
        let observed = Arc::clone(&commits);
        db.commit_hook(Some(move || {
            observed.fetch_add(1, Ordering::SeqCst);
            false
        }));
        initialize(&mut db, &Deadline::new(10000)).unwrap();
        assert!(db.is_autocommit());
        assert_eq!(commits.load(Ordering::SeqCst), 1);
        assert_schema_and_policy(&db);
        // Reopening does not replace existing data or its immutable identifiers.
        db.execute("INSERT INTO meta VALUES('workspace','owned-identity')", [])
            .unwrap();
        initialize(&mut db, &Deadline::new(10000)).unwrap();
        let identity: String = db
            .query_row("SELECT value FROM meta WHERE key='workspace'", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(identity, "owned-identity");
    }

    #[test]
    fn refused_commit_rolls_back_every_table_and_allows_a_fresh_attempt() {
        let root = tempfile::tempdir().unwrap();
        let mut db = connection(root.path());
        let attempts = Arc::new(AtomicUsize::new(0));
        let observed = Arc::clone(&attempts);
        db.commit_hook(Some(move || {
            observed.fetch_add(1, Ordering::SeqCst);
            true
        }));
        assert!(initialize(&mut db, &Deadline::new(10000)).is_err());
        assert_eq!(attempts.load(Ordering::SeqCst), 1);
        assert!(db.is_autocommit());
        db.commit_hook(None::<fn() -> bool>);
        assert_eq!(user_schema_objects(&db), 0);
        initialize(&mut db, &Deadline::new(10000)).unwrap();
        assert_schema_and_policy(&db);
    }

    #[test]
    fn expired_budget_refuses_before_starting_a_schema_transaction() {
        let root = tempfile::tempdir().unwrap();
        let mut db = connection(root.path());
        let attempts = Arc::new(AtomicUsize::new(0));
        let observed = Arc::clone(&attempts);
        db.commit_hook(Some(move || {
            observed.fetch_add(1, Ordering::SeqCst);
            false
        }));
        assert!(initialize(&mut db, &Deadline::new(0)).is_err());
        assert_eq!(attempts.load(Ordering::SeqCst), 0);
        assert!(db.is_autocommit());
        assert_eq!(user_schema_objects(&db), 0);
    }

    #[test]
    fn initialized_schema_keeps_read_admission_while_another_connection_writes() {
        let root = tempfile::tempdir().unwrap();
        let mut writer = connection(root.path());
        initialize(&mut writer, &Deadline::new(10000)).unwrap();
        writer
            .execute("INSERT INTO meta VALUES('workspace','owned-identity')", [])
            .unwrap();
        let mut reader = connection(root.path());
        reader.busy_timeout(std::time::Duration::ZERO).unwrap();
        let transaction = writer
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .unwrap();
        transaction
            .execute("INSERT INTO meta VALUES('uncommitted','private')", [])
            .unwrap();
        initialize(&mut reader, &Deadline::new(10000)).unwrap();
        let identity: String = reader
            .query_row("SELECT value FROM meta WHERE key='workspace'", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(identity, "owned-identity");
        let hidden: i64 = reader
            .query_row(
                "SELECT count(*) FROM meta WHERE key='uncommitted'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(hidden, 0);
        assert!(reader.is_autocommit());
        transaction.rollback().unwrap();
    }

    #[test]
    fn inherited_cancellation_before_commit_discards_the_complete_schema() {
        let root = tempfile::tempdir().unwrap();
        let mut db = connection(root.path());
        let cancel = Arc::new(AtomicBool::new(false));
        let flag = Arc::clone(&cancel);
        let callbacks = Arc::new(AtomicUsize::new(0));
        let observed = Arc::clone(&callbacks);
        // Observe cancellation during real SQLite VM execution without making
        // SQLite interrupt it. The bootstrap must enforce its inherited policy
        // before publishing the transaction, not depend on this test callback.
        db.progress_handler(
            1,
            Some(move || {
                observed.fetch_add(1, Ordering::SeqCst);
                flag.store(true, Ordering::SeqCst);
                false
            }),
        );
        let result = Deadline::run_bounded(&Deadline::new(10000), Arc::clone(&cancel), || {
            initialize(&mut db, &Deadline::new(10000))
        });
        db.progress_handler(0, None::<fn() -> bool>);
        assert!(callbacks.load(Ordering::SeqCst) > 0);
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("cancelled by host")
        );
        assert!(db.is_autocommit());
        assert_eq!(user_schema_objects(&db), 0);
    }
}
