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

#[test]
fn sqlite_busy_wait_observes_cancellation_before_the_long_operation_deadline() {
    let root = tempfile::tempdir().unwrap();
    let store = Store::open(root.path()).unwrap();
    let holder = rusqlite::Connection::open(store.dir.join("index.sqlite")).unwrap();
    holder.execute_batch("BEGIN IMMEDIATE").unwrap();
    let cancel = Arc::new(AtomicBool::new(false));
    let flag = Arc::clone(&cancel);
    let result = Deadline::run_bounded(&Deadline::new(2000), cancel, || {
        store.set_deadline(&Deadline::new(2000))?;
        let cancelling = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(60));
            let cancelled_at = Instant::now();
            flag.store(true, Ordering::SeqCst);
            cancelled_at
        });
        let error = store
            .record(
                "cancel_probe",
                "long_locked",
                &serde_json::json!({"written":true}),
            )
            .unwrap_err();
        let returned_at = Instant::now();
        let cancelled_at = cancelling.join().unwrap();
        assert!(error.downcast_ref::<rusqlite::Error>().is_some());
        assert!(
            returned_at.saturating_duration_since(cancelled_at) < Duration::from_millis(300),
            "SQLite retained the long busy timeout after actual cancellation: {:?}",
            returned_at.saturating_duration_since(cancelled_at)
        );
        Ok(())
    });
    assert!(result.is_err());
    holder.execute_batch("ROLLBACK").unwrap();
    store.clear_deadline();
    assert!(
        store
            .get_record::<serde_json::Value>("cancel_probe", "long_locked")
            .is_err()
    );
    store.record("cancel_probe", "recovered", &true).unwrap();
}

#[test]
fn sqlite_busy_wait_does_not_restart_the_budget_after_serialization() {
    struct SlowValue(Arc<std::sync::Mutex<Option<Instant>>>);
    impl serde::Serialize for SlowValue {
        fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
            std::thread::sleep(Duration::from_millis(650));
            *self.0.lock().unwrap() = Some(Instant::now());
            serializer.serialize_bool(true)
        }
    }
    let root = tempfile::tempdir().unwrap();
    let store = Store::open(root.path()).unwrap();
    let holder = rusqlite::Connection::open(store.dir.join("index.sqlite")).unwrap();
    holder.execute_batch("BEGIN IMMEDIATE").unwrap();
    store.set_deadline(&Deadline::new(500)).unwrap();
    let serialized = Arc::new(std::sync::Mutex::new(None));
    assert!(
        store
            .record(
                "cancel_probe",
                "serialized_locked",
                &SlowValue(Arc::clone(&serialized))
            )
            .is_err()
    );
    let after_serialization = serialized.lock().unwrap().unwrap().elapsed();
    assert!(
        after_serialization < Duration::from_millis(200),
        "Expired serialization was followed by a fresh SQLite busy wait: {after_serialization:?}"
    );
    holder.execute_batch("ROLLBACK").unwrap();
    store.clear_deadline();
    assert!(
        store
            .get_record::<serde_json::Value>("cancel_probe", "serialized_locked")
            .is_err()
    );
}

#[test]
fn serialization_cancellation_cannot_publish_uncontended_record_mutations() {
    struct CancellingValue {
        flag: Arc<AtomicBool>,
        value: bool,
    }
    impl serde::Serialize for CancellingValue {
        fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
            self.flag.store(true, Ordering::SeqCst);
            serializer.serialize_bool(self.value)
        }
    }
    for operation in ["record", "replace", "delete"] {
        let root = tempfile::tempdir().unwrap();
        let store = Store::open(root.path()).unwrap();
        store
            .record("serialization_probe", "original", &true)
            .unwrap();
        let cancel = Arc::new(AtomicBool::new(false));
        let flag = Arc::clone(&cancel);
        let result = Deadline::run_bounded(&Deadline::new(2000), cancel, || {
            store.set_deadline(&Deadline::new(2000))?;
            let expected = CancellingValue {
                flag: Arc::clone(&flag),
                value: true,
            };
            let next = CancellingValue {
                flag: Arc::clone(&flag),
                value: false,
            };
            let result = match operation {
                "record" => store.record("serialization_probe", "created", &next),
                "replace" => store
                    .replace_record("serialization_probe", "original", &expected, &next)
                    .map(|_| ()),
                "delete" => store
                    .delete_record("serialization_probe", "original", &expected)
                    .map(|_| ()),
                _ => unreachable!(),
            };
            assert!(
                result.is_err(),
                "{operation} published after serialization cancelled the host request"
            );
            Ok(())
        });
        assert!(result.is_err());
        store.clear_deadline();
        assert!(
            store
                .get_record::<bool>("serialization_probe", "original")
                .unwrap()
        );
        assert!(
            store
                .get_record::<bool>("serialization_probe", "created")
                .is_err()
        );
    }
}

#[test]
fn ledger_lock_wait_keeps_the_same_host_cancellation_policy() {
    let root = tempfile::tempdir().unwrap();
    let store = Store::open(root.path()).unwrap();
    let holder = rusqlite::Connection::open(store.dir.join("index.sqlite")).unwrap();
    holder.execute_batch("BEGIN IMMEDIATE").unwrap();
    let cancel = Arc::new(AtomicBool::new(false));
    let flag = Arc::clone(&cancel);
    let result = Deadline::run_bounded(&Deadline::new(2000), cancel, || {
        let deadline = Deadline::new(2000);
        let cancelling = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(60));
            let cancelled_at = Instant::now();
            flag.store(true, Ordering::SeqCst);
            cancelled_at
        });
        let result = yoke::ledger::Ledger::new(&store, &deadline).reserve(
            "cancelled-lock",
            10,
            100,
            "EUR",
            "owned-fixture",
        );
        let returned_at = Instant::now();
        let cancelled_at = cancelling.join().unwrap();
        assert!(result.is_err());
        assert!(returned_at.saturating_duration_since(cancelled_at) < Duration::from_millis(300));
        Ok(())
    });
    assert!(result.is_err());
    holder.execute_batch("ROLLBACK").unwrap();
    store.clear_deadline();
    let deadline = Deadline::new(2000);
    let ledger = yoke::ledger::Ledger::new(&store, &deadline);
    assert_eq!(ledger.total("cancelled-lock").unwrap().encumbered_micros, 0);
    ledger
        .reserve("cancelled-lock", 10, 100, "EUR", "owned-fixture")
        .unwrap();
}

#[test]
fn brief_writer_contention_still_allows_atomic_publication() {
    let root = tempfile::tempdir().unwrap();
    let store = Store::open(root.path()).unwrap();
    let path = store.dir.join("index.sqlite");
    let (ready, blocked) = std::sync::mpsc::channel();
    let holder = std::thread::spawn(move || {
        let db = rusqlite::Connection::open(path).unwrap();
        db.execute_batch("BEGIN IMMEDIATE").unwrap();
        ready.send(()).unwrap();
        std::thread::sleep(Duration::from_millis(80));
        db.execute_batch("ROLLBACK").unwrap();
    });
    blocked.recv_timeout(Duration::from_secs(2)).unwrap();
    store.set_deadline(&Deadline::new(2000)).unwrap();
    store
        .record(
            "cancel_probe",
            "brief_lock",
            &serde_json::json!({"exact":"🌍\r\n"}),
        )
        .unwrap();
    holder.join().unwrap();
    assert_eq!(
        store
            .get_record::<serde_json::Value>("cancel_probe", "brief_lock")
            .unwrap(),
        serde_json::json!({"exact":"🌍\r\n"})
    );
}

#[test]
fn busy_policy_belongs_to_the_connection_after_its_store_is_dropped() {
    let root = tempfile::tempdir().unwrap();
    let other_root = tempfile::tempdir().unwrap();
    let db = {
        let store = Store::open(root.path()).unwrap();
        store.set_deadline(&Deadline::new(350)).unwrap();
        store.db
    };
    let holder = rusqlite::Connection::open(root.path().join(".yoke/index.sqlite")).unwrap();
    holder.execute_batch("BEGIN IMMEDIATE").unwrap();
    let other = Store::open(other_root.path()).unwrap();
    other.set_deadline(&Deadline::new(3000)).unwrap();
    let started = Instant::now();
    let operation = std::thread::spawn(move || {
        let result = db.execute("INSERT INTO records VALUES('owned','moved','true')", []);
        assert!(
            matches!(result, Err(rusqlite::Error::SqliteFailure(error, _)) if error.code == rusqlite::ErrorCode::DatabaseBusy)
        );
        // Closing the moved connection also owns its callback-state destruction.
        db.close().unwrap();
    });
    operation.join().unwrap();
    assert!(
        started.elapsed() < Duration::from_millis(900),
        "Another Store replaced the moved connection's deadline"
    );
    holder.execute_batch("ROLLBACK").unwrap();
    other.record("cancel_probe", "independent", &true).unwrap();
}
