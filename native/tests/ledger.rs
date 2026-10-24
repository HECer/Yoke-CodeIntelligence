use tempfile::TempDir;
use yoke::{
    capture::Deadline,
    ledger::{Ledger, Usage},
    store::Store,
};

// Removing reservation persistence or the inclusive budget check must break this.
#[test]
fn reservations_survive_reopen_and_refuse_overspend() {
    let directory = TempDir::new().unwrap();
    {
        let store = Store::open(directory.path()).unwrap();
        Ledger::new(&store, &Deadline::new(10_000))
            .reserve("release", 60, 100, "EUR", "fixture-v1")
            .unwrap();
    }
    let store = Store::open(directory.path()).unwrap();
    let deadline = Deadline::new(10_000);
    let ledger = Ledger::new(&store, &deadline);
    assert!(
        ledger
            .reserve("release", 41, 100, "EUR", "fixture-v1")
            .is_err()
    );
    ledger
        .reserve("release", 40, 100, "EUR", "fixture-v1")
        .unwrap();
    let totals = serde_json::to_value(ledger.total("release").unwrap()).unwrap();
    assert_eq!(totals["currency"], "EUR");
    assert_eq!(totals["taskLimitMicros"], 100);
    let missing = serde_json::to_value(ledger.total("absent").unwrap()).unwrap();
    assert!(missing["currency"].is_null());
    assert!(missing["taskLimitMicros"].is_null());
}

// Releasing a sent unknown attempt as zero must break this.
#[test]
fn unknown_billing_retains_reservation_and_only_unsent_attempts_cancel() {
    let directory = TempDir::new().unwrap();
    let store = Store::open(directory.path()).unwrap();
    let deadline = Deadline::new(10_000);
    let ledger = Ledger::new(&store, &deadline);
    let first = ledger
        .reserve("unknown", 80, 100, "USD", "fixture-v1")
        .unwrap();
    ledger.mark_sent(&first).unwrap();
    ledger.settle(&first, None).unwrap();
    assert!(ledger.cancel_before_send(&first).is_err());
    assert_eq!(ledger.total("unknown").unwrap().encumbered_micros, 80);
    assert_eq!(ledger.total("unknown").unwrap().unknown, 1);
    let second = ledger
        .reserve("unknown", 20, 100, "USD", "fixture-v1")
        .unwrap();
    ledger.cancel_before_send(&second).unwrap();
    ledger
        .reserve("unknown", 20, 100, "USD", "fixture-v1")
        .unwrap();
    assert!(
        ledger
            .reserve("unknown", 1, 100, "USD", "fixture-v1")
            .is_err()
    );
    ledger.settle(&first, Some(30)).unwrap();
    assert_eq!(ledger.total("unknown").unwrap().encumbered_micros, 50);
}

// Clamping an overreported cost or allowing another attempt after overreservation must break this.
#[test]
fn reported_cost_above_reservation_stays_charged_and_blocks_task() {
    let directory = TempDir::new().unwrap();
    let store = Store::open(directory.path()).unwrap();
    let deadline = Deadline::new(10_000);
    let ledger = Ledger::new(&store, &deadline);
    let id = ledger
        .reserve("overrun", 10, 100, "EUR", "fixture-v1")
        .unwrap();
    ledger.mark_sent(&id).unwrap();
    ledger.settle(&id, Some(60)).unwrap();
    ledger.settle(&id, Some(60)).unwrap();
    assert!(ledger.settle(&id, Some(10)).is_err());
    let total = ledger.total("overrun").unwrap();
    assert_eq!(total.actual_micros, 60);
    assert_eq!(total.encumbered_micros, 60);
    assert!(total.reservation_exceeded);
    assert!(
        ledger
            .reserve("overrun", 1, 100, "EUR", "fixture-v1")
            .is_err()
    );
}

// Lowering cumulative usage, silently accepting conflicting event IDs, or changing currency must break this.
#[test]
fn usage_is_monotone_deduplicated_and_policy_is_immutable() {
    let directory = TempDir::new().unwrap();
    let store = Store::open(directory.path()).unwrap();
    let deadline = Deadline::new(10_000);
    let ledger = Ledger::new(&store, &deadline);
    let id = ledger
        .reserve("usage", 10, 100, "EUR", "fixture-v1")
        .unwrap();
    ledger.mark_sent(&id).unwrap();
    let reported = Usage {
        input_tokens: 100,
        output_tokens: 20,
        cached_input_tokens: 5,
    };
    ledger.usage(&id, "response", reported.clone()).unwrap();
    ledger.usage(&id, "response", reported).unwrap();
    assert!(ledger.usage(&id, "response", Usage::default()).is_err());
    ledger
        .usage(
            &id,
            "later",
            Usage {
                input_tokens: 80,
                output_tokens: 30,
                cached_input_tokens: 1,
            },
        )
        .unwrap();
    let attempt: serde_json::Value = store.get_record("native_model_attempt_v1", &id).unwrap();
    assert_eq!(attempt["usage"]["inputTokens"], 100);
    assert_eq!(attempt["usage"]["outputTokens"], 30);
    assert_eq!(attempt["usage"]["cachedInputTokens"], 5);
    assert!(
        ledger
            .usage(
                &id,
                "bad",
                Usage {
                    input_tokens: 1,
                    output_tokens: 0,
                    cached_input_tokens: 2
                }
            )
            .is_err()
    );
    assert!(
        ledger
            .reserve("usage", 1, 101, "EUR", "fixture-v1")
            .is_err()
    );
    assert!(
        ledger
            .reserve("usage", 1, 100, "USD", "fixture-v1")
            .is_err()
    );
}

// Checking a budget outside the insertion transaction must break this concurrent admission test.
#[test]
fn concurrent_reservations_cannot_overspend() {
    let directory = TempDir::new().unwrap();
    drop(Store::open(directory.path()).unwrap());
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(12));
    let workers: Vec<_> = (0..12)
        .map(|_| {
            let root = directory.path().to_path_buf();
            let barrier = barrier.clone();
            std::thread::spawn(move || {
                let store = Store::open(&root).unwrap();
                barrier.wait();
                Ledger::new(&store, &Deadline::new(10_000))
                    .reserve("parallel", 11, 100, "EUR", "fixture-v1")
                    .is_ok()
            })
        })
        .collect();
    let admitted = workers
        .into_iter()
        .map(|worker| worker.join().unwrap())
        .filter(|ok| *ok)
        .count();
    assert_eq!(admitted, 9);
    let store = Store::open(directory.path()).unwrap();
    assert_eq!(
        Ledger::new(&store, &Deadline::new(10_000))
            .total("parallel")
            .unwrap()
            .encumbered_micros,
        99
    );
}
