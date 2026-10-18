use yoke::{
    capture::Deadline,
    ledger::{AttemptState, Ledger},
    store::Store,
};

#[test]
fn batch_admission_is_atomic_and_preserves_individual_unknown_billing() {
    let root = tempfile::tempdir().unwrap();
    let store = Store::open(root.path()).unwrap();
    let d = Deadline::new(10000);
    let ledger = Ledger::new(&store, &d);
    assert!(
        ledger
            .reserve_many("task", &[60, 41], 100, "USD", "fixture")
            .is_err()
    );
    assert_eq!(ledger.total("task").unwrap().attempts, 0);
    let mut reservations = ledger
        .reserve_many("task", &[60, 40], 100, "USD", "fixture")
        .expect("the complete admitted batch must reserve atomically");
    assert_eq!(reservations.len(), 2);
    assert_ne!(reservations[0].id(), reservations[1].id());
    let unsent = reservations.pop().unwrap();
    let unsent_id = unsent.id().to_owned();
    let sent = ledger.send_reserved(reservations.pop().unwrap()).unwrap();
    ledger
        .usage(
            &sent,
            "response",
            yoke::ledger::Usage {
                input_tokens: 7,
                output_tokens: 0,
                cached_input_tokens: 2,
            },
        )
        .unwrap();
    ledger.settle(&sent, None).unwrap();
    assert!(ledger.cancel_before_send(&sent).is_err());
    ledger.cancel_before_send(&unsent_id).unwrap();
    let status = ledger.attempt_status(&sent).unwrap();
    assert_eq!(status.state, AttemptState::Unknown);
    assert_eq!(status.reported_usage.unwrap().input_tokens, 7);
    assert_eq!(
        ledger.attempt_status(&unsent_id).unwrap().state,
        AttemptState::NotSent
    );
    let total = ledger.total("task").unwrap();
    assert_eq!(total.attempts, 2);
    assert_eq!(total.unknown, 1);
    assert_eq!(total.encumbered_micros, 60);
}

#[test]
fn first_overrun_blocks_a_previously_reserved_later_send_without_releasing_sent_cost() {
    let root = tempfile::tempdir().unwrap();
    let store = Store::open(root.path()).unwrap();
    let d = Deadline::new(10000);
    let ledger = Ledger::new(&store, &d);
    let mut attempts = ledger
        .reserve_many("task", &[10, 10], 100, "USD", "fixture")
        .unwrap();
    let later = attempts.pop().unwrap();
    let later_id = later.id().to_owned();
    let first = ledger.send_reserved(attempts.pop().unwrap()).unwrap();
    ledger.settle(&first, Some(15)).unwrap();
    assert!(ledger.send_reserved(later).is_err());
    assert_eq!(
        ledger.attempt_status(&later_id).unwrap().state,
        AttemptState::Reserved
    );
    ledger.cancel_before_send(&later_id).unwrap();
    assert_eq!(ledger.total("task").unwrap().actual_micros, 15);
    assert!(ledger.total("task").unwrap().reservation_exceeded);
}

#[test]
fn reservation_tokens_are_workspace_bound_and_cannot_send_an_already_sent_attempt() {
    let a = tempfile::tempdir().unwrap();
    let b = tempfile::tempdir().unwrap();
    let a = Store::open(a.path()).unwrap();
    let b = Store::open(b.path()).unwrap();
    let d = Deadline::new(10000);
    let left = Ledger::new(&a, &d);
    let right = Ledger::new(&b, &d);
    let mut tokens = left
        .reserve_many("task", &[1, 1], 10, "USD", "fixture")
        .unwrap();
    let foreign = tokens.pop().unwrap();
    let foreign_id = foreign.id().to_owned();
    assert!(right.send_reserved(foreign).is_err());
    assert_eq!(
        left.attempt_status(&foreign_id).unwrap().state,
        AttemptState::Reserved
    );
    let repeated = tokens.pop().unwrap();
    let repeated_id = repeated.id().to_owned();
    left.mark_sent(&repeated_id).unwrap();
    assert!(left.send_reserved(repeated).is_err());
    assert_eq!(left.total("task").unwrap().unknown, 1);
}

#[test]
fn a_late_sqlite_insert_failure_rolls_back_every_batch_reservation_and_task() {
    let root = tempfile::tempdir().unwrap();
    let store = Store::open(root.path()).unwrap();
    let d = Deadline::new(10000);
    let ledger = Ledger::new(&store, &d);
    store.db.execute_batch("CREATE TRIGGER owned_batch_failure BEFORE INSERT ON records WHEN NEW.kind='native_model_attempt_v1' AND (SELECT COUNT(*) FROM records WHERE kind='native_model_attempt_v1')=1 BEGIN SELECT RAISE(ABORT,'owned late insert failure'); END;").unwrap();
    assert!(
        ledger
            .reserve_many("task", &[1, 1, 1], 10, "USD", "fixture")
            .is_err()
    );
    let count:i64=store.db.query_row("SELECT count(*) FROM records WHERE kind IN ('native_model_attempt_v1','native_model_task_v1')",[],|r|r.get(0)).unwrap();
    assert_eq!(count, 0);
    assert_eq!(ledger.total("task").unwrap().attempts, 0);
}

#[test]
fn competing_whole_batches_cannot_spend_partial_or_overlapping_budgets() {
    use std::sync::{Arc, Barrier};
    let root = tempfile::tempdir().unwrap();
    drop(Store::open(root.path()).unwrap());
    let barrier = Arc::new(Barrier::new(2));
    let mut handles = Vec::new();
    for _ in 0..2 {
        let path = root.path().to_owned();
        let barrier = Arc::clone(&barrier);
        handles.push(std::thread::spawn(move || {
            let opened = Store::open(&path);
            barrier.wait();
            let store = opened.unwrap();
            let d = Deadline::new(10000);
            let ledger = Ledger::new(&store, &d);
            ledger
                .reserve_many("task", &[30, 30], 100, "USD", "fixture")
                .is_ok()
        }));
    }
    assert_eq!(
        handles
            .into_iter()
            .map(|h| h.join().unwrap() as usize)
            .sum::<usize>(),
        1
    );
    let store = Store::open(root.path()).unwrap();
    let d = Deadline::new(10000);
    let totals = Ledger::new(&store, &d).total("task").unwrap();
    assert_eq!(totals.attempts, 2);
    assert_eq!(totals.reserved_micros, 60);
}

#[test]
fn batch_status_can_be_validated_inside_the_existing_retention_transaction() {
    let root = tempfile::tempdir().unwrap();
    let store = Store::open(root.path()).unwrap();
    let d = Deadline::new(10000);
    let ledger = Ledger::new(&store, &d);
    let token = ledger
        .reserve_many("task", &[1], 10, "USD", "fixture")
        .unwrap()
        .pop()
        .unwrap();
    let transaction =
        rusqlite::Transaction::new_unchecked(&store.db, rusqlite::TransactionBehavior::Immediate)
            .unwrap();
    let status = ledger
        .attempt_status(token.id())
        .expect("Retention already owns the SQLite transaction; status must borrow it");
    assert_eq!(status.state, AttemptState::Reserved);
    transaction.rollback().unwrap();
    assert_eq!(ledger.total("task").unwrap().reserved_micros, 1);
}
