use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use yoke::capture::Deadline;

#[test]
fn nested_deadlines_inherit_host_ceiling_cancellation_and_restore_after_scope() {
    let parent = Deadline::new(500);
    let cancel = Arc::new(AtomicBool::new(false));
    let child = Deadline::run_bounded(&parent, Arc::clone(&cancel), || {
        let child = Deadline::new(600_000);
        assert!(parent.remaining() >= child.remaining());
        Deadline::run_bounded(
            &Deadline::new(600_000),
            Arc::new(AtomicBool::new(false)),
            || {
                assert!(parent.remaining() >= Deadline::new(600_000).remaining());
                Ok(())
            },
        )?;
        Ok(child)
    })
    .unwrap();
    cancel.store(true, Ordering::SeqCst);
    assert!(child.check().is_err());
    assert!(child.remaining().is_zero());
    assert!(Deadline::new(600_000).remaining().as_secs() > 500);
}

#[test]
fn scoped_deadline_restores_after_panic_and_does_not_leak_to_other_threads() {
    let cancel = Arc::new(AtomicBool::new(false));
    let panic = std::panic::catch_unwind(|| {
        let _ = Deadline::run_bounded(
            &Deadline::new(500),
            Arc::clone(&cancel),
            || -> anyhow::Result<()> {
                let inherited = Deadline::new(600_000);
                let other = std::thread::spawn(move || {
                    assert!(Deadline::new(600_000).remaining().as_secs() > 500);
                    inherited.check().unwrap();
                });
                other.join().unwrap();
                panic!("test scoped restoration")
            },
        );
    });
    assert!(panic.is_err());
    assert!(Deadline::new(600_000).remaining().as_secs() > 500);
}
