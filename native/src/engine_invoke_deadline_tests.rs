//! Deterministic inherited-deadline admission; no provider process is required.
use super::*;
use std::{
    cell::RefCell,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

type AfterDeadline = Box<dyn FnOnce()>;
thread_local! {
    static AFTER_DEADLINE: RefCell<Option<AfterDeadline>> = const { RefCell::new(None) };
}

pub(super) fn after_deadline() {
    if let Some(hook) = AFTER_DEADLINE.with(|slot| slot.borrow_mut().take()) {
        hook();
    }
}

pub(super) struct Hook;
impl Drop for Hook {
    fn drop(&mut self) {
        AFTER_DEADLINE.with(|slot| drop(slot.borrow_mut().take()));
    }
}

pub(super) fn cancelling_hook(cancel: Arc<AtomicBool>) -> Hook {
    AFTER_DEADLINE.with(|slot| {
        assert!(
            slot.replace(Some(Box::new(move || cancel.store(true, Ordering::SeqCst))))
                .is_none()
        );
    });
    Hook
}

fn fixture() -> (tempfile::TempDir, Engine, model::Snapshot) {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("main.rs"), "fn target() {}\n").unwrap();
    let mut engine = Engine::open(root.path()).unwrap();
    let (snapshot, _, _, _) = engine.index(&Deadline::new(20_000)).unwrap();
    (root, engine, snapshot)
}

fn semantic(snapshot: &model::Snapshot, timeout: u64) -> Value {
    json!({"snapshotId":snapshot.id,"timeoutMs":timeout,
        "languageServer":{"path":"main.rs","offset":3,"operation":"implementation","options":{}}})
}

fn cancellable(cancel: Arc<AtomicBool>) -> Deadline {
    // Return the flagged Deadline, then leave the ambient host scope. The
    // explicit invoke_at argument must carry cancellation on its own.
    Deadline::run_bounded(&Deadline::new(20_000), cancel, || Ok(Deadline::new(20_000))).unwrap()
}

#[test]
fn expired_inherited_deadline_refuses_before_provider_selection() {
    let (_root, mut engine, snapshot) = fixture();
    for timeout in [10, 20_000, 120_000] {
        let error = engine
            .invoke_at("ci_find", semantic(&snapshot, timeout), &Deadline::new(0))
            .unwrap_err();
        assert_eq!(error.to_string(), "Request deadline exceeded");
    }
}

#[test]
fn captured_and_live_cancellation_reach_engine_admission_without_an_ambient_scope() {
    let (_root, mut engine, snapshot) = fixture();
    let cancel = Arc::new(AtomicBool::new(false));
    let deadline = cancellable(Arc::clone(&cancel));
    cancel.store(true, Ordering::SeqCst);
    assert_eq!(
        engine
            .invoke_at("ci_find", semantic(&snapshot, 120_000), &deadline)
            .unwrap_err()
            .to_string(),
        "Request cancelled by host"
    );

    let cancel = Arc::new(AtomicBool::new(false));
    let deadline = cancellable(Arc::clone(&cancel));
    let _hook = cancelling_hook(cancel);
    let value = engine
        .invoke_at("ci_find", semantic(&snapshot, 120_000), &deadline)
        .unwrap();
    assert_eq!(value["status"], "error", "{value}");
    assert_eq!(value["error"]["message"], "Request cancelled by host");
    assert_eq!(value["metrics"]["languageServerAttempts"], 0);
    // With the inherited flags removed, the same request reaches the absent
    // provider instead. That refusal must remain distinguishable from cancel.
    let ordinary = engine
        .invoke("ci_find", semantic(&snapshot, 120_000))
        .unwrap();
    assert_eq!(ordinary["status"], "error");
    assert_eq!(
        ordinary["error"]["message"],
        "No host-authorized semantic server registered for this language"
    );
}

#[test]
fn ordinary_timeout_validation_and_default_remain_unchanged_without_an_inherited_deadline() {
    let (_root, mut engine, _) = fixture();
    for timeout in [0, 9, 600_001] {
        let value = engine
            .invoke("ci_status", json!({"timeoutMs":timeout}))
            .unwrap();
        assert_eq!(value["status"], "error", "{value}");
        assert_eq!(
            value["error"]["message"],
            "Invalid numeric range: timeoutMs"
        );
    }
    assert_eq!(
        engine.invoke("ci_status", json!({})).unwrap()["status"],
        "ok"
    );
}
