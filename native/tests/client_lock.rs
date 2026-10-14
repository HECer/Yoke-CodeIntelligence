use std::{
    cell::Cell,
    time::{Duration, Instant},
};
use yoke::{capture::Deadline, store::Store};

#[test]
fn maintenance_cannot_run_while_another_client_holds_state_open() {
    let root = tempfile::tempdir().unwrap();
    let first = Store::open(root.path()).unwrap();
    let second = Store::open(root.path()).unwrap();
    let called = Cell::new(false);
    assert!(
        first
            .with_exclusive_client(&Deadline::new(200), || {
                called.set(true);
                Ok(())
            })
            .is_err()
    );
    assert!(
        !called.get(),
        "Exclusive work ran with another client present"
    );
    drop(second);
    first
        .with_exclusive_client(&Deadline::new(200), || {
            called.set(true);
            Ok(())
        })
        .unwrap();
    assert!(called.get());
    let reopened = Store::open(root.path()).unwrap();
    assert_eq!(first.workspace_id, reopened.workspace_id);
}
#[test]
fn exclusive_maintenance_denies_new_clients_and_restores_its_lease_after_error() {
    let root = tempfile::tempdir().unwrap();
    let store = Store::open(root.path()).unwrap();
    let result: anyhow::Result<()> = store.with_exclusive_client(&Deadline::new(1000), || {
        let started = Instant::now();
        assert!(Store::open_with_timeout(root.path(), Duration::from_millis(50)).is_err());
        assert!(started.elapsed() < Duration::from_millis(500));
        anyhow::bail!("Fixture maintenance failure")
    });
    assert!(result.is_err());
    let second = Store::open(root.path()).unwrap();
    assert!(
        store
            .with_exclusive_client(&Deadline::new(200), || Ok(()))
            .is_err()
    );
    drop(second);
    store
        .with_exclusive_client(&Deadline::new(200), || Ok(()))
        .unwrap();
}
