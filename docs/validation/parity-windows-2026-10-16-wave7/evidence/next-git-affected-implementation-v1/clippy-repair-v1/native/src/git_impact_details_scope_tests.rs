use super::{DetailOptions, derive, with_additional_scope_policy, with_candidate_pair_observer};
use crate as test_yoke;
#[path = "../tests/support/git_impact_fixture.rs"]
mod fixture;
use crate::capture::Deadline;
use fixture::{Fixture, MOVED_BASE, state};
use std::{cell::RefCell, rc::Rc};

// The callback is a test-only extra denial AFTER each real side-wide policy.
// Corrupting the first side's source makes a load-before-policy regression fail
// before it reaches the expected second-side callback. A genuine positive runs first.
#[test]
fn both_real_side_policies_precede_inherited_load_source_cas_reads() {
    let f = Fixture::moved();
    derive(
        &f.engine.store,
        f.id(),
        &DetailOptions::default(),
        &Deadline::new(30_000),
    )
    .expect("genuine admitted feature before the ordering tripwire");
    let hash = crate::model::hash(MOVED_BASE);
    let blob = f.engine.store.dir.join("blobs").join(hash);
    let mut altered = MOVED_BASE.as_bytes().to_vec();
    altered[0] = b'!';
    std::fs::write(blob, altered).unwrap();
    let before = state(&f.engine);
    let calls = Rc::new(RefCell::new(Vec::new()));
    let observed = Rc::clone(&calls);
    let result = with_additional_scope_policy(
        move |side| {
            observed.borrow_mut().push(side.to_owned());
            if side == "head" {
                anyhow::bail!("R5_SCOPE_DENIAL_SENTINEL");
            }
            Ok(())
        },
        || {
            derive(
                &f.engine.store,
                f.id(),
                &DetailOptions::default(),
                &Deadline::new(30_000),
            )
        },
    );
    assert_eq!(*calls.borrow(), ["base", "head"]);
    fixture::assert_static_error(result.unwrap_err(), "git_impact_details_input_invalid");
    assert_eq!(state(&f.engine), before);
}

#[test]
fn actual_invalid_head_capture_policy_refuses_before_extra_policy_observer() {
    let f = Fixture::moved_scoped();
    derive(
        &f.engine.store,
        f.id(),
        &DetailOptions::default(),
        &Deadline::new(30_000),
    )
    .expect("genuine admitted feature before the policy corruption");
    let head = f.report["headSnapshotId"].as_str().unwrap();
    let mut policy: serde_json::Value = f
        .engine
        .store
        .get_record(crate::capture::OPTIONS_RECORD_KIND, head)
        .unwrap();
    policy["optionsHash"] = serde_json::json!("f".repeat(64));
    f.engine
        .store
        .db
        .execute(
            "UPDATE records SET value=?1 WHERE kind=?2 AND id=?3",
            rusqlite::params![
                serde_json::to_string(&policy).unwrap(),
                crate::capture::OPTIONS_RECORD_KIND,
                head
            ],
        )
        .unwrap();
    let before = state(&f.engine);
    let calls = Rc::new(RefCell::new(Vec::new()));
    let observed = Rc::clone(&calls);
    let result = with_additional_scope_policy(
        move |side| {
            observed.borrow_mut().push(side.to_owned());
            Ok(())
        },
        || {
            derive(
                &f.engine.store,
                f.id(),
                &DetailOptions::default(),
                &Deadline::new(30_000),
            )
        },
    );
    // Historical base policy may resolve its origin's policy too. Neither that
    // early denial nor the head's own denial can authorize a head callback.
    assert!(!calls.borrow().iter().any(|side| side == "head"));
    fixture::assert_static_error(result.unwrap_err(), "git_impact_details_input_invalid");
    assert_eq!(state(&f.engine), before);
}

#[test]
fn candidate_capacity_is_admitted_before_any_large_cross_product_expansion() {
    let base: Vec<_> = (0..40)
        .map(|i| (format!("base-{i:02}.rs"), "fn same() {}\n".to_owned()))
        .collect();
    let head: Vec<_> = (0..40)
        .map(|i| (format!("head-{i:02}.rs"), "fn same() {}\n".to_owned()))
        .collect();
    let base_refs: Vec<_> = base.iter().map(|(p, t)| (p.as_str(), t.as_str())).collect();
    let head_refs: Vec<_> = head.iter().map(|(p, t)| (p.as_str(), t.as_str())).collect();
    let f = Fixture::new(&base_refs, &head_refs);
    let observed = Rc::new(RefCell::new(0usize));
    let pass_counter = Rc::clone(&observed);
    let output = with_candidate_pair_observer(
        move || *pass_counter.borrow_mut() += 1,
        || {
            derive(
                &f.engine.store,
                f.id(),
                &DetailOptions {
                    max_candidate_pairs: 1600,
                    ..Default::default()
                },
                &Deadline::new(30_000),
            )
        },
    )
    .expect("genuine complete 40 by 40 candidate qualification");
    assert_eq!(*observed.borrow(), 1600);
    assert_eq!(
        output.value()["renameCandidates"].as_array().unwrap().len(),
        1600
    );
    *observed.borrow_mut() = 0;
    let refused_counter = Rc::clone(&observed);
    let before = state(&f.engine);
    let result = with_candidate_pair_observer(
        move || *refused_counter.borrow_mut() += 1,
        || {
            derive(
                &f.engine.store,
                f.id(),
                &DetailOptions {
                    max_candidate_pairs: 1599,
                    ..Default::default()
                },
                &Deadline::new(30_000),
            )
        },
    );
    assert_eq!(
        *observed.borrow(),
        0,
        "no pair expansion before capacity admission"
    );
    fixture::assert_static_error(result.unwrap_err(), "git_impact_details_capacity_exceeded");
    assert_eq!(state(&f.engine), before);
}

#[test]
fn shared_host_cancellation_stops_pair_work_and_preserves_read_transaction_state() {
    let f = Fixture::new(
        &[("a.rs", "fn same() {}\n"), ("b.rs", "fn same() {}\n")],
        &[("c.rs", "fn same() {}\n"), ("d.rs", "fn same() {}\n")],
    );
    derive(
        &f.engine.store,
        f.id(),
        &DetailOptions::default(),
        &Deadline::new(30_000),
    )
    .expect("genuine feature before cancellation");
    let cancelled = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let callback_flag = std::sync::Arc::clone(&cancelled);
    let count = Rc::new(RefCell::new(0usize));
    let callback_count = Rc::clone(&count);
    let before = state(&f.engine);
    let mut inner_result = None;
    let outer_result = Deadline::run_bounded(&Deadline::new(30_000), cancelled, || {
        inner_result = Some(with_candidate_pair_observer(
            move || {
                *callback_count.borrow_mut() += 1;
                callback_flag.store(true, std::sync::atomic::Ordering::SeqCst);
            },
            || {
                derive(
                    &f.engine.store,
                    f.id(),
                    &DetailOptions::default(),
                    &Deadline::new(30_000),
                )
            },
        ));
        Ok(())
    });
    assert_eq!(*count.borrow(), 1);
    // The host wrapper independently rechecks cancellation after its closure.
    // Assert the producer's refusal before that outer wrapper replaces it.
    assert!(outer_result.is_err());
    fixture::assert_static_error(
        inner_result.expect("producer called").unwrap_err(),
        "git_impact_details_deadline_exceeded",
    );
    f.engine.store.clear_deadline();
    assert_eq!(state(&f.engine), before);
}
