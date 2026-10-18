//! Actual existing Store leases, independent of SQLite lock contention.
use std::{
    fs::File,
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};
use yoke::{Engine, capture::Deadline, store::Store};

fn fixture() -> (tempfile::TempDir, String) {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("api.rs"), "pub fn api() {}\n").unwrap();
    let mut engine = Engine::open(root.path()).unwrap();
    let id = engine.index(&Deadline::new(30_000)).unwrap().0.id;
    drop(engine);
    let db = rusqlite::Connection::open(root.path().join(".yoke/index.sqlite")).unwrap();
    db.execute_batch("PRAGMA journal_mode=DELETE").unwrap();
    drop(db);
    (root, id)
}
fn held(root: &Path, name: &str) -> File {
    let file = File::options()
        .read(true)
        .write(true)
        .open(root.join(".yoke").join(name))
        .unwrap();
    fs2::FileExt::try_lock_exclusive(&file).unwrap();
    file
}
fn released(root: &Path) {
    for name in ["maintenance.lock", "clients.lock"] {
        let file = held(root, name);
        fs2::FileExt::unlock(&file).unwrap();
    }
}
fn wait_for_release(read_only: bool) {
    for name in ["maintenance.lock", "clients.lock"] {
        let (root, id) = fixture();
        let file = held(root.path(), name);
        let release = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(100));
            fs2::FileExt::unlock(&file).unwrap();
        });
        let result = if read_only {
            Store::open_existing_read_only(root.path(), &Deadline::new(2000))
        } else {
            Store::open_with_timeout(root.path(), Duration::from_secs(2))
        };
        release.join().unwrap();
        let store = result
            .unwrap_or_else(|error| panic!("Store readOnly={read_only}, lease={name}: {error:#}"));
        assert_eq!(store.snapshot(&id).unwrap().id, id);
        drop(store);
        released(root.path());
    }
}
#[test]
fn existing_read_only_store_waits_for_a_transient_exclusive_client_or_maintenance_lease() {
    wait_for_release(true);
}
#[test]
fn normal_store_waits_for_a_transient_exclusive_client_or_maintenance_lease() {
    wait_for_release(false);
}
#[test]
fn existing_read_only_lease_timeout_does_not_return_before_the_deadline_or_leak_a_lease() {
    for name in ["maintenance.lock", "clients.lock"] {
        let (root, _) = fixture();
        let file = held(root.path(), name);
        let d = Deadline::new(25);
        let start = Instant::now();
        let result = Store::open_existing_read_only(root.path(), &d);
        let expired_at_return = d.check().is_err();
        let elapsed = start.elapsed();
        let error = result.err().expect("Held lease must expire");
        assert!(
            expired_at_return && error.to_string().contains("deadline"),
            "{error:#}"
        );
        assert!(
            elapsed < Duration::from_millis(150),
            "Lease deadline took {elapsed:?}"
        );
        drop(file);
        released(root.path());
    }
}
#[test]
fn existing_read_only_lease_wait_observes_host_cancellation_and_releases_its_handles() {
    for name in ["maintenance.lock", "clients.lock"] {
        let (root, _) = fixture();
        let file = held(root.path(), name);
        let flag = Arc::new(AtomicBool::new(false));
        let mut observed = None;
        let result = Deadline::run_bounded(&Deadline::new(2000), Arc::clone(&flag), || {
            let d = Deadline::new(2000);
            let cancel = Arc::clone(&flag);
            let worker = std::thread::spawn(move || {
                std::thread::sleep(Duration::from_millis(25));
                cancel.store(true, Ordering::SeqCst);
            });
            let result = Store::open_existing_read_only(root.path(), &d);
            // Record the constructor's state before joining the cancellation
            // thread; the outer bounded wrapper cannot manufacture this proof.
            observed = Some((
                result.err().map(|error| error.to_string()),
                d.check().is_err(),
            ));
            worker.join().unwrap();
            Ok(())
        });
        assert!(result.unwrap_err().to_string().contains("cancelled"));
        let (error, cancelled_at_return) = observed.unwrap();
        assert!(cancelled_at_return && error.is_some_and(|error| error.contains("cancelled")));
        drop(file);
        released(root.path());
    }
}

#[test]
fn maintenance_waits_for_a_transient_admission_lease_and_restores_its_client() {
    let (root, _) = fixture();
    let store = Store::open(root.path()).unwrap();
    let file = held(root.path(), "maintenance.lock");
    let release = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(100));
        fs2::FileExt::unlock(&file).unwrap();
    });
    let mut called = false;
    let result = store.with_exclusive_client(&Deadline::new(2000), || {
        called = true;
        Ok(())
    });
    release.join().unwrap();
    result.unwrap();
    assert!(called);
    store
        .with_exclusive_client(&Deadline::new(2000), || Ok(()))
        .unwrap();
    drop(store);
    released(root.path());
}

#[test]
fn cancelled_maintenance_wait_never_invokes_the_callback_and_leaves_leases_reusable() {
    let (root, _) = fixture();
    let store = Store::open(root.path()).unwrap();
    let file = held(root.path(), "maintenance.lock");
    let flag = Arc::new(AtomicBool::new(false));
    let mut called = false;
    let mut observed = None;
    let result = Deadline::run_bounded(&Deadline::new(2000), Arc::clone(&flag), || {
        let d = Deadline::new(2000);
        let (returned, wait_return) = std::sync::mpsc::channel();
        let forced_release = Arc::new(AtomicBool::new(false));
        let worker_forced_release = Arc::clone(&forced_release);
        let worker = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(25));
            flag.store(true, Ordering::SeqCst);
            // The caller must return while this lease is still held. The
            // fail-safe prevents a broken implementation from hanging the test.
            if wait_return
                .recv_timeout(Duration::from_millis(500))
                .is_err()
            {
                worker_forced_release.store(true, Ordering::SeqCst);
            }
            fs2::FileExt::unlock(&file).unwrap();
        });
        let result = store.with_exclusive_client(&d, || {
            called = true;
            Ok(())
        });
        observed = Some((
            result.err().map(|error| error.to_string()),
            d.check().is_err(),
            forced_release.load(Ordering::SeqCst),
        ));
        let _ = returned.send(());
        worker.join().unwrap();
        Ok(())
    });
    assert!(result.unwrap_err().to_string().contains("cancelled"));
    let (error, cancelled_at_return, forced_release) = observed.unwrap();
    assert!(cancelled_at_return && error.is_some_and(|error| error.contains("cancelled")));
    assert!(
        !forced_release,
        "Maintenance waited for forced lease release"
    );
    assert!(!called, "Cancelled maintenance invoked its callback");
    store
        .with_exclusive_client(&Deadline::new(2000), || Ok(()))
        .unwrap();
    drop(store);
    released(root.path());
}
