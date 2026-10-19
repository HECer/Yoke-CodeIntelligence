use std::{
    fs,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};
use yoke::{capture::Deadline, store::Store};

fn existing() -> tempfile::TempDir {
    let root = tempfile::tempdir().unwrap();
    drop(Store::open(root.path()).unwrap());
    root
}
fn excluded(root: &std::path::Path) -> rusqlite::Connection {
    let db = rusqlite::Connection::open(root.join(".yoke/index.sqlite")).unwrap();
    let mode: String = db
        .query_row("PRAGMA journal_mode=DELETE", [], |r| r.get(0))
        .unwrap();
    assert_eq!(mode, "delete");
    db.execute_batch("BEGIN EXCLUSIVE").unwrap();
    db
}

#[test]
fn zero_and_unrepresentable_open_budgets_refuse_before_creating_state() {
    for budget in [Duration::ZERO, Duration::MAX] {
        let root = tempfile::tempdir().unwrap();
        let result = std::panic::catch_unwind(|| Store::open_with_timeout(root.path(), budget));
        assert!(
            result.is_ok(),
            "Opening budget must refuse without panicking"
        );
        assert!(
            result.unwrap().is_err(),
            "Invalid opening budget was admitted"
        );
        assert!(!root.path().join(".yoke").exists());
    }
}

#[test]
fn cancelled_open_refuses_before_creating_state() {
    let root = tempfile::tempdir().unwrap();
    let cancel = Arc::new(AtomicBool::new(false));
    let flag = Arc::clone(&cancel);
    let result = Deadline::run_bounded(&Deadline::new(2000), cancel, || {
        flag.store(true, Ordering::SeqCst);
        assert!(Store::open(root.path()).is_err());
        Ok(())
    });
    assert!(result.is_err());
    assert!(!root.path().join(".yoke").exists());
}

#[test]
fn opening_schema_wait_uses_the_inherited_absolute_deadline() {
    let root = existing();
    let holder = excluded(root.path());
    let started = Instant::now();
    let result = Deadline::run_bounded(
        &Deadline::new(120),
        Arc::new(AtomicBool::new(false)),
        || {
            assert!(Store::open_with_timeout(root.path(), Duration::from_secs(2)).is_err());
            Ok(())
        },
    );
    assert!(result.is_err());
    assert!(
        started.elapsed() < Duration::from_millis(500),
        "Store opening ignored its parent ceiling: {:?}",
        started.elapsed()
    );
    holder.execute_batch("ROLLBACK").unwrap();
    let mode: String = holder
        .query_row("PRAGMA journal_mode", [], |r| r.get(0))
        .unwrap();
    assert_eq!(mode, "delete");
    drop(holder);
    drop(Store::open(root.path()).unwrap());
}

#[test]
fn opening_schema_wait_observes_host_cancellation() {
    let root = existing();
    let holder = excluded(root.path());
    let cancel = Arc::new(AtomicBool::new(false));
    let flag = Arc::clone(&cancel);
    let result = Deadline::run_bounded(&Deadline::new(3000), cancel, || {
        let cancelling = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(60));
            let at = Instant::now();
            flag.store(true, Ordering::SeqCst);
            at
        });
        let opened = Store::open_with_timeout(root.path(), Duration::from_secs(2));
        let returned = Instant::now();
        let cancelled = cancelling.join().unwrap();
        assert!(opened.is_err());
        assert!(
            returned.saturating_duration_since(cancelled) < Duration::from_millis(300),
            "Schema lock ignored cancellation: {:?}",
            returned.saturating_duration_since(cancelled)
        );
        Ok(())
    });
    assert!(result.is_err());
    holder.execute_batch("ROLLBACK").unwrap();
}

#[test]
fn opening_client_admission_and_lease_waits_observe_host_cancellation() {
    for name in ["maintenance.lock", "clients.lock"] {
        let root = existing();
        let holder = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(root.path().join(".yoke").join(name))
            .unwrap();
        fs2::FileExt::try_lock_exclusive(&holder).unwrap();
        let cancel = Arc::new(AtomicBool::new(false));
        let flag = Arc::clone(&cancel);
        let result = Deadline::run_bounded(&Deadline::new(3000), cancel, || {
            let cancelling = std::thread::spawn(move || {
                std::thread::sleep(Duration::from_millis(60));
                let at = Instant::now();
                flag.store(true, Ordering::SeqCst);
                at
            });
            let opened = Store::open_with_timeout(root.path(), Duration::from_secs(2));
            let returned = Instant::now();
            let cancelled = cancelling.join().unwrap();
            assert!(opened.is_err());
            assert!(
                returned.saturating_duration_since(cancelled) < Duration::from_millis(300),
                "{name} ignored cancellation: {:?}",
                returned.saturating_duration_since(cancelled)
            );
            Ok(())
        });
        assert!(result.is_err());
        fs2::FileExt::unlock(&holder).unwrap();
        drop(holder);
        drop(Store::open(root.path()).unwrap());
    }
}

#[test]
fn successful_open_does_not_leave_its_setup_deadline_on_later_operations() {
    let root = tempfile::tempdir().unwrap();
    let store = Store::open_with_timeout(root.path(), Duration::from_millis(500)).unwrap();
    std::thread::sleep(Duration::from_millis(550));
    store.record("owned", "after-opening", &true).unwrap();
    assert!(store.get_record::<bool>("owned", "after-opening").unwrap());
}

#[test]
fn opening_succeeds_after_brief_schema_contention_without_replacing_identity_or_data() {
    let root = tempfile::tempdir().unwrap();
    let store = Store::open(root.path()).unwrap();
    let workspace = store.workspace_id.clone();
    let source = "owned exact bytes 🌍\r\n";
    store.record("owned", "source", &source).unwrap();
    drop(store);
    let holder = excluded(root.path());
    let release = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(80));
        holder.execute_batch("ROLLBACK").unwrap();
    });
    let reopened = Store::open_with_timeout(root.path(), Duration::from_secs(2)).unwrap();
    release.join().unwrap();
    assert_eq!(reopened.workspace_id, workspace);
    assert_eq!(
        reopened.get_record::<String>("owned", "source").unwrap(),
        source
    );
}
