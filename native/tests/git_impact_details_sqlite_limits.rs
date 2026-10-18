use yoke as test_yoke;
#[path = "support/git_impact_fixture.rs"]
mod fixture;
use fixture::{Fixture, assert_static_error, state};
use rusqlite::{Transaction, TransactionBehavior, limits::Limit};
use yoke::{
    capture::Deadline,
    git_workflows::impact_details::{self, DetailOptions},
};

#[test]
fn caller_sqlite_limit_and_read_transaction_survive_success_and_both_refusal_classes() {
    let f = Fixture::moved();
    impact_details::derive(
        &f.engine.store,
        f.id(),
        &DetailOptions::default(),
        &Deadline::new(30_000),
    )
    .expect("genuine successful feature before caller-state regression");
    let db = &f.engine.store.db;
    let original = db.limit(Limit::SQLITE_LIMIT_LENGTH).unwrap();
    let requested = 96_000_123;
    assert_ne!(
        original, requested,
        "fixture supplies a genuine nondefault caller limit"
    );
    db.set_limit(Limit::SQLITE_LIMIT_LENGTH, requested).unwrap();
    assert_eq!(db.limit(Limit::SQLITE_LIMIT_LENGTH).unwrap(), requested);
    let read = Transaction::new_unchecked(db, TransactionBehavior::Deferred).unwrap();
    let before = state(&f.engine);

    impact_details::derive(
        &f.engine.store,
        f.id(),
        &DetailOptions::default(),
        &Deadline::new(30_000),
    )
    .unwrap();
    assert_eq!(db.limit(Limit::SQLITE_LIMIT_LENGTH).unwrap(), requested);
    assert!(
        !db.is_autocommit(),
        "producer retains the caller-owned transaction"
    );
    assert_eq!(state(&f.engine), before);

    assert_static_error(
        impact_details::derive(
            &f.engine.store,
            f.id(),
            &DetailOptions {
                max_rows: 0,
                ..Default::default()
            },
            &Deadline::new(30_000),
        )
        .unwrap_err(),
        "git_impact_details_capacity_exceeded",
    );
    assert_eq!(db.limit(Limit::SQLITE_LIMIT_LENGTH).unwrap(), requested);
    assert!(!db.is_autocommit());
    assert_eq!(state(&f.engine), before);

    assert_static_error(
        impact_details::derive(
            &f.engine.store,
            "invalid-report-id",
            &DetailOptions::default(),
            &Deadline::new(30_000),
        )
        .unwrap_err(),
        "git_impact_details_input_invalid",
    );
    assert_eq!(db.limit(Limit::SQLITE_LIMIT_LENGTH).unwrap(), requested);
    assert!(!db.is_autocommit());
    assert_eq!(state(&f.engine), before);

    read.rollback().unwrap();
    assert!(db.is_autocommit());
    db.set_limit(Limit::SQLITE_LIMIT_LENGTH, original).unwrap();
}
