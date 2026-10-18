//! Independent batch ledger capability and immutable response-event review.
use serde_json::json;
use yoke::{
    capture::Deadline,
    ledger::{AttemptState, Ledger, Usage},
    model::identity,
    store::Store,
};

#[test]
fn sealed_reservation_rejects_changed_durable_bindings_without_ledger_side_effects() {
    for (field, value) in [
        ("provider", json!("other")),
        ("task", json!("other")),
        ("currency", json!("EUR")),
        ("reservedMicros", json!(11)),
    ] {
        let root = tempfile::tempdir().unwrap();
        let store = Store::open(root.path()).unwrap();
        let d = Deadline::new(5000);
        let ledger = Ledger::new(&store, &d);
        let token = ledger
            .reserve_many("task", &[10], 100, "USD", "fixture")
            .unwrap()
            .pop()
            .unwrap();
        let id = token.id().to_owned();
        let original: String = store
            .db
            .query_row(
                "SELECT value FROM records WHERE kind='native_model_attempt_v1' AND id=?1",
                [&id],
                |r| r.get(0),
            )
            .unwrap();
        let mut changed: serde_json::Value = serde_json::from_str(&original).unwrap();
        changed[field] = value;
        store
            .db
            .execute(
                "UPDATE records SET value=?2 WHERE kind='native_model_attempt_v1' AND id=?1",
                rusqlite::params![id, changed.to_string()],
            )
            .unwrap();
        assert!(
            ledger.send_reserved(token).is_err(),
            "accepted changed {field}"
        );
        store
            .db
            .execute(
                "UPDATE records SET value=?2 WHERE kind='native_model_attempt_v1' AND id=?1",
                rusqlite::params![id, original],
            )
            .unwrap();
        assert_eq!(
            ledger.attempt_status(&id).unwrap().state,
            AttemptState::Reserved
        );
        let totals = ledger.total("task").unwrap();
        assert_eq!(totals.attempts, 1);
        assert_eq!(totals.unknown, 0);
        assert_eq!(totals.reserved_micros, 10);
        assert_eq!(totals.encumbered_micros, 10);
    }
}

#[test]
fn original_response_usage_stays_distinct_from_later_maxima_and_reconciled_cost() {
    let root = tempfile::tempdir().unwrap();
    let store = Store::open(root.path()).unwrap();
    let d = Deadline::new(5000);
    let ledger = Ledger::new(&store, &d);
    let token = ledger
        .reserve_many("task", &[10], 100, "USD", "fixture")
        .unwrap()
        .pop()
        .unwrap();
    let id = ledger.send_reserved(token).unwrap();
    let response = Usage {
        input_tokens: 10,
        output_tokens: 3,
        cached_input_tokens: 2,
    };
    ledger.usage(&id, "response", response.clone()).unwrap();
    ledger.settle(&id, None).unwrap();
    ledger
        .usage(
            &id,
            "host-later-observation",
            Usage {
                input_tokens: 20,
                output_tokens: 4,
                cached_input_tokens: 3,
            },
        )
        .unwrap();
    ledger.settle(&id, Some(5)).unwrap();
    let status = ledger.attempt_status(&id).unwrap();
    assert_eq!(status.state, AttemptState::Settled);
    assert_eq!(status.actual_cost_micros, Some(5));
    assert_eq!(status.response_usage, Some(response.clone()));
    assert_eq!(status.reported_usage.unwrap().input_tokens, 20);
    ledger.usage(&id, "response", response).unwrap();
    assert!(
        ledger
            .usage(
                &id,
                "response",
                Usage {
                    input_tokens: 11,
                    output_tokens: 3,
                    cached_input_tokens: 2
                }
            )
            .is_err()
    );
    assert_eq!(
        ledger
            .attempt_status(&id)
            .unwrap()
            .response_usage
            .unwrap()
            .input_tokens,
        10
    );
    assert_eq!(ledger.total("task").unwrap().encumbered_micros, 5);
}

#[test]
fn late_batch_insert_failure_preserves_an_existing_task_and_all_old_reservations() {
    let root = tempfile::tempdir().unwrap();
    let store = Store::open(root.path()).unwrap();
    let d = Deadline::new(5000);
    let ledger = Ledger::new(&store, &d);
    let old = ledger
        .reserve_many("task", &[7], 100, "USD", "fixture")
        .unwrap()
        .pop()
        .unwrap();
    let old_id = old.id().to_owned();
    let task_id = identity(&"task");
    let before: String = store
        .db
        .query_row(
            "SELECT value FROM records WHERE kind='native_model_task_v1' AND id=?1",
            [&task_id],
            |r| r.get(0),
        )
        .unwrap();
    store.db.execute_batch("CREATE TRIGGER review_second_insert BEFORE INSERT ON records WHEN NEW.kind='native_model_attempt_v1' AND (SELECT count(*) FROM records WHERE kind='native_model_attempt_v1')=2 BEGIN SELECT RAISE(ABORT,'owned second batch row failure'); END;").unwrap();
    assert!(
        ledger
            .reserve_many("task", &[10, 10, 10], 100, "USD", "fixture")
            .is_err()
    );
    let after: String = store
        .db
        .query_row(
            "SELECT value FROM records WHERE kind='native_model_task_v1' AND id=?1",
            [&task_id],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(after, before);
    assert_eq!(ledger.total("task").unwrap().attempts, 1);
    assert_eq!(
        ledger.attempt_status(&old_id).unwrap().state,
        AttemptState::Reserved
    );
    assert_eq!(ledger.send_reserved(old).unwrap(), old_id);
}
