//! Scripted startup/owner channels exercise the actual startup admission path.
//! They do not replace real Python/process-ownership integration acceptance.
use super::*;

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
