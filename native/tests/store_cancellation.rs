use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};
use yoke::{capture::Deadline, store::Store};

#[test]
fn active_sql_is_interrupted_by_inherited_host_cancellation() {
    let root = tempfile::tempdir().unwrap();
    let store = Store::open(root.path()).unwrap();
    let cancel = Arc::new(AtomicBool::new(false));
    let flag = Arc::clone(&cancel);
    let started = Instant::now();
    let result = Deadline::run_bounded(&Deadline::new(3000), cancel, || {
        store.set_deadline(&Deadline::new(3000))?;
        let mut query=store.db.prepare("WITH RECURSIVE work(n) AS (VALUES(1) UNION ALL SELECT n+1 FROM work WHERE n<1000000000) SELECT sum(n) FROM work")?;
        let cancelling = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(80));
            flag.store(true, Ordering::SeqCst);
        });
        let result = query.query_row([], |row| row.get::<_, i64>(0));
        cancelling.join().unwrap();
        assert!(
            matches!(result,Err(rusqlite::Error::SqliteFailure(error,_)) if error.code==rusqlite::ErrorCode::OperationInterrupted)
        );
        Ok(())
    });
    assert!(result.is_err());
    assert!(
        started.elapsed() < Duration::from_secs(1),
        "Cancelled SQL continued until its time deadline: {:?}",
        started.elapsed()
    );
    store.clear_deadline();
    assert_eq!(
        store
            .db
            .query_row("SELECT 7", [], |row| row.get::<_, i64>(0))
            .unwrap(),
        7
    );
}

#[test]
fn cancelled_store_refuses_new_publication_until_deadline_is_replaced() {
    let root = tempfile::tempdir().unwrap();
    let store = Store::open(root.path()).unwrap();
    let cancel = Arc::new(AtomicBool::new(false));
    let flag = Arc::clone(&cancel);
    let _ = Deadline::run_bounded(&Deadline::new(3000), cancel, || {
        store.set_deadline(&Deadline::new(3000))?;
        flag.store(true, Ordering::SeqCst);
        assert!(
            store
                .record(
                    "cancel_probe",
                    "probe",
                    &serde_json::json!({"written":true})
                )
                .is_err(),
            "Cancelled Store accepted a new publication"
        );
        Ok(())
    });
    store.set_deadline(&Deadline::new(3000)).unwrap();
    assert!(
        store
            .get_record::<serde_json::Value>("cancel_probe", "probe")
            .is_err()
    );
    store
        .record(
            "cancel_probe",
            "probe",
            &serde_json::json!({"written":true}),
        )
        .unwrap();
}

#[test]
fn sqlite_lock_contention_retains_a_finite_timeout_and_atomic_recovery() {
    let root = tempfile::tempdir().unwrap();
    let store = Store::open(root.path()).unwrap();
    let holder = rusqlite::Connection::open(store.dir.join("index.sqlite")).unwrap();
    holder.execute_batch("BEGIN IMMEDIATE").unwrap();
    let cancel = Arc::new(AtomicBool::new(false));
    let flag = Arc::clone(&cancel);
    let started = Instant::now();
    let result = Deadline::run_bounded(&Deadline::new(350), cancel, || {
        store.set_deadline(&Deadline::new(350))?;
        let cancelling = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(60));
            flag.store(true, Ordering::SeqCst);
        });
        let result = store.record(
            "cancel_probe",
            "locked",
            &serde_json::json!({"written":true}),
        );
        cancelling.join().unwrap();
        result
    });
    assert!(result.is_err());
    eprintln!(
        "SQLite busy-wait elapsed after cancellation-at60ms: {:?}",
        started.elapsed()
    );
    assert!(started.elapsed() < Duration::from_secs(1));
    holder.execute_batch("ROLLBACK").unwrap();
    store.clear_deadline();
    assert!(
        store
            .get_record::<serde_json::Value>("cancel_probe", "locked")
            .is_err()
    );
    store
        .record(
            "cancel_probe",
            "locked",
            &serde_json::json!({"written":true}),
        )
        .unwrap();
}
