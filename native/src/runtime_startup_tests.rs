//! Scripted startup/owner channels exercise the actual startup admission path.
//! They do not replace real Python/process-ownership integration acceptance.
use super::*;

type BeforeReceive = Box<dyn FnOnce()>;
thread_local! {
    static BEFORE_RECEIVE: std::cell::RefCell<Option<BeforeReceive>> =
        const { std::cell::RefCell::new(None) };
}

pub(super) fn before_receive() {
    let hook = BEFORE_RECEIVE.with(|slot| slot.borrow_mut().take());
    if let Some(hook) = hook {
        hook();
    }
}

struct WaitHook;
impl Drop for WaitHook {
    fn drop(&mut self) {
        BEFORE_RECEIVE.with(|slot| drop(slot.borrow_mut().take()));
    }
}

fn cancel_at_receive(flag: Arc<AtomicBool>) -> WaitHook {
    BEFORE_RECEIVE.with(|slot| {
        assert!(
            slot.replace(Some(Box::new(move || flag.store(true, Ordering::SeqCst))))
                .is_none()
        );
    });
    WaitHook
}

fn cancellable_deadline(flag: Arc<AtomicBool>) -> Deadline {
    Deadline::run_bounded(&Deadline::new(10_000), flag, || Ok(Deadline::new(10_000))).unwrap()
}

#[test]
fn successful_startup_preserves_the_live_owner_receipt_and_does_not_cancel() {
    let (startup, started) = mpsc::sync_channel(1);
    let (owner, stopped) = mpsc::sync_channel(1);
    startup.send(Ok(7usize)).unwrap();
    owner.send(Ok(())).unwrap();
    let cancel = AtomicBool::new(false);

    let value = receive_startup(&started, &stopped, &cancel, &Deadline::new(10_000)).unwrap();

    assert_eq!(value, 7);
    assert!(!cancel.load(Ordering::SeqCst));
    assert!(matches!(stopped.try_recv(), Ok(Ok(()))));
}

#[test]
fn reported_startup_failure_cancels_and_consumes_confirmed_owner_shutdown() {
    let (startup, started) = mpsc::sync_channel::<std::result::Result<usize, String>>(1);
    let (owner, stopped) = mpsc::sync_channel(1);
    startup.send(Err("owned startup refusal".into())).unwrap();
    owner.send(Ok(())).unwrap();
    let cancel = AtomicBool::new(false);

    let error = receive_startup(&started, &stopped, &cancel, &Deadline::new(10_000)).unwrap_err();

    assert_eq!(error.to_string(), "owned startup refusal");
    assert!(!crate::process::cleanup_unproven(&error));
    assert!(cancel.load(Ordering::SeqCst));
    assert!(matches!(stopped.try_recv(), Err(mpsc::TryRecvError::Empty)));
}

#[test]
fn expired_and_disconnected_startup_require_the_same_owner_receipt() {
    for disconnected in [false, true] {
        let (startup, started) = mpsc::sync_channel::<std::result::Result<usize, String>>(1);
        let startup = if disconnected {
            drop(startup);
            None
        } else {
            Some(startup)
        };
        let (owner, stopped) = mpsc::sync_channel(1);
        owner.send(Ok(())).unwrap();
        let cancel = AtomicBool::new(false);

        let error = receive_startup(
            &started,
            &stopped,
            &cancel,
            &Deadline::new(if disconnected { 10_000 } else { 0 }),
        )
        .unwrap_err();

        drop(startup);
        assert_eq!(error.to_string(), "Runtime startup deadline exceeded");
        assert!(!crate::process::cleanup_unproven(&error));
        assert!(cancel.load(Ordering::SeqCst));
        assert!(matches!(stopped.try_recv(), Err(mpsc::TryRecvError::Empty)));
    }
}

#[test]
fn refused_owner_cleanup_keeps_its_typed_cause_after_startup_failure_or_expiry() {
    for expired in [false, true] {
        let (startup, started) = mpsc::sync_channel::<std::result::Result<usize, String>>(1);
        if !expired {
            startup.send(Err("owned startup refusal".into())).unwrap();
        }
        let (owner, stopped) = mpsc::sync_channel(1);
        owner
            .send(Err(crate::process::CleanupUnproven::new(
                "owned cleanup refusal",
            )
            .into()))
            .unwrap();
        let cancel = AtomicBool::new(false);

        let error = receive_startup(
            &started,
            &stopped,
            &cancel,
            &Deadline::new(if expired { 0 } else { 10_000 }),
        )
        .unwrap_err();

        assert!(crate::process::cleanup_unproven(&error));
        assert!(format!("{error:#}").contains("owned cleanup refusal"));
        assert!(cancel.load(Ordering::SeqCst));
        assert!(matches!(stopped.try_recv(), Err(mpsc::TryRecvError::Empty)));
    }
}

#[test]
fn missing_owner_receipt_remains_typed_cleanup_uncertainty_after_startup_failure_or_expiry() {
    for expired in [false, true] {
        let (startup, started) = mpsc::sync_channel::<std::result::Result<usize, String>>(1);
        if !expired {
            startup.send(Err("owned startup refusal".into())).unwrap();
        }
        let (owner, stopped) = mpsc::sync_channel::<Result<()>>(1);
        drop(owner);
        let cancel = AtomicBool::new(false);

        let error = receive_startup(
            &started,
            &stopped,
            &cancel,
            &Deadline::new(if expired { 0 } else { 10_000 }),
        )
        .unwrap_err();

        assert!(crate::process::cleanup_unproven(&error));
        assert!(format!("{error:#}").contains("Runtime startup owner cleanup receipt unavailable"));
        assert!(cancel.load(Ordering::SeqCst));
    }
}

#[test]
fn owner_observation_failure_preserves_both_error_causes() {
    let (startup, started) = mpsc::sync_channel::<std::result::Result<usize, String>>(1);
    let (owner, stopped) = mpsc::sync_channel(1);
    startup.send(Err("owned startup refusal".into())).unwrap();
    owner
        .send(Err(
            std::io::Error::other("owned observation failure").into()
        ))
        .unwrap();
    let cancel = AtomicBool::new(false);

    let error = receive_startup(&started, &stopped, &cancel, &Deadline::new(10_000)).unwrap_err();

    assert_eq!(error.to_string(), "owned startup refusal");
    assert!(format!("{error:#}").contains("owned observation failure"));
    assert!(error.chain().any(|cause| cause.is::<std::io::Error>()));
    assert!(cancel.load(Ordering::SeqCst));
    assert!(matches!(stopped.try_recv(), Err(mpsc::TryRecvError::Empty)));
}

#[test]
fn cancellation_after_wait_budget_selection_does_not_consume_the_original_lifetime() {
    let (startup, started) = mpsc::sync_channel::<std::result::Result<usize, String>>(1);
    let (owner, stopped) = mpsc::sync_channel(1);
    owner.send(Ok(())).unwrap();
    let host_cancel = Arc::new(AtomicBool::new(false));
    let deadline = cancellable_deadline(host_cancel.clone());
    let _hook = cancel_at_receive(host_cancel.clone());
    let cancel = AtomicBool::new(false);
    let now = std::time::Instant::now();

    // The hook cancels after the wait duration was sampled, while this sender
    // remains open without delivering startup. The real channel wait must wake.
    let error = receive_startup(&started, &stopped, &cancel, &deadline).unwrap_err();
    let elapsed = now.elapsed();
    drop(startup);

    assert_eq!(error.to_string(), "Runtime startup cancelled by host");
    assert!(elapsed < Duration::from_secs(2), "elapsed={elapsed:?}");
    assert!(host_cancel.load(Ordering::SeqCst));
    assert!(cancel.load(Ordering::SeqCst));
    assert!(!crate::process::cleanup_unproven(&error));
    assert!(matches!(stopped.try_recv(), Err(mpsc::TryRecvError::Empty)));
}

#[test]
fn cancellation_alongside_successful_pipe_delivery_still_requires_owner_cleanup() {
    for refused in [false, true] {
        let (startup, started) = mpsc::sync_channel(1);
        startup.send(Ok(7usize)).unwrap();
        let (owner, stopped) = mpsc::sync_channel(1);
        owner
            .send(if refused {
                Err(crate::process::CleanupUnproven::new("owned cleanup refusal").into())
            } else {
                Ok(())
            })
            .unwrap();
        let host_cancel = Arc::new(AtomicBool::new(false));
        let deadline = cancellable_deadline(host_cancel.clone());
        let _hook = cancel_at_receive(host_cancel);
        let cancel = AtomicBool::new(false);

        let error = receive_startup(&started, &stopped, &cancel, &deadline).unwrap_err();

        assert_eq!(error.to_string(), "Runtime startup cancelled by host");
        assert_eq!(crate::process::cleanup_unproven(&error), refused);
        if refused {
            assert!(format!("{error:#}").contains("owned cleanup refusal"));
        }
        assert!(cancel.load(Ordering::SeqCst));
        assert!(matches!(stopped.try_recv(), Err(mpsc::TryRecvError::Empty)));
    }
}
