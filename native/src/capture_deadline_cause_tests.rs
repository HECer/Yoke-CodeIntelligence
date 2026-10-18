//! Typed observation for the existing deadline; root promotes tests before behavior.
use super::{Deadline, DeadlineFailure};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

#[test]
fn live_original_deadline_has_no_typed_failure_and_keeps_legacy_checks() {
    let deadline = Deadline::new(60_000);
    assert_eq!(deadline.failure(), None);
    assert!(deadline.check().is_ok());
    assert!(!deadline.remaining().is_zero());
}

#[test]
fn expired_original_deadline_reports_expired_without_reset() {
    let deadline = Deadline::new(0);
    assert_eq!(deadline.failure(), Some(DeadlineFailure::Expired));
    assert!(deadline.check().is_err());
    assert!(deadline.remaining().is_zero());
}

#[test]
fn inherited_cancellation_is_observed_on_the_same_borrow_and_wins_over_expiry() {
    let parent = Deadline::new(60_000);
    let cancellation = Arc::new(AtomicBool::new(false));
    let flag = Arc::clone(&cancellation);
    let result = Deadline::run_bounded(&parent, cancellation, || {
        let inherited = Deadline::new(60_000);
        let expired = Deadline::new(0);
        let address = std::ptr::from_ref(&inherited);
        assert_eq!(inherited.failure(), None);
        flag.store(true, Ordering::SeqCst);
        assert_eq!(inherited.failure(), Some(DeadlineFailure::Cancelled));
        assert_eq!(expired.failure(), Some(DeadlineFailure::Cancelled));
        assert_eq!(std::ptr::from_ref(&inherited), address);
        assert!(inherited.check().is_err());
        assert!(inherited.remaining().is_zero());
        Ok(())
    });
    assert!(
        result.is_err(),
        "The legacy host scope must still reject cancellation"
    );
    assert!(
        parent.check().is_ok(),
        "Host scope restoration must preserve the original parent"
    );
    let after_scope = Deadline::new(60_000);
    assert_eq!(after_scope.failure(), None);
    assert!(after_scope.check().is_ok());
    assert!(!after_scope.remaining().is_zero());
}

#[test]
fn history_cleanup_failure_survives_host_cancellation() {
    let parent = Deadline::new(60_000);
    let cancellation = Arc::new(AtomicBool::new(false));
    let flag = Arc::clone(&cancellation);
    let result: anyhow::Result<()> = Deadline::run_bounded(&parent, cancellation, || {
        flag.store(true, Ordering::SeqCst);
        Err(anyhow::Error::new(crate::process::CleanupUnproven::new(
            "owned group still active",
        ))
        .context("Nested history acquisition failed"))
    });
    assert!(
        crate::process::cleanup_unproven(&result.unwrap_err()),
        "Cancellation must not hide an unproven owned-process cleanup"
    );
    assert!(parent.check().is_ok());
}

#[test]
fn history_cleanup_failure_survives_host_deadline() {
    let parent = Deadline::new(20);
    let result: anyhow::Result<()> =
        Deadline::run_bounded(&parent, Arc::new(AtomicBool::new(false)), || {
            std::thread::sleep(std::time::Duration::from_millis(30));
            Err(anyhow::Error::new(crate::process::CleanupUnproven::new(
                "owned group still active",
            ))
            .context("Nested history acquisition failed"))
        });
    assert!(
        crate::process::cleanup_unproven(&result.unwrap_err()),
        "Expiry must not hide an unproven owned-process cleanup"
    );
    assert_eq!(parent.failure(), Some(DeadlineFailure::Expired));
}

#[test]
fn history_cleanup_failure_remains_typed_inside_a_spawn_io_error() {
    let error = anyhow::Error::new(std::io::Error::other(crate::process::CleanupUnproven::new(
        "owned root exit unproven",
    )))
    .context("Host executable unavailable");
    assert!(crate::process::cleanup_unproven(&error));
}
