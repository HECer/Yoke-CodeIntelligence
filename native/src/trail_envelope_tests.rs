use super::{EnvelopeCounter, LogicalEnvelope, logical_envelope_size};
use crate::{capture::Deadline, retrieval_scopes::RetrievalHardError};
use serde_json::json;
use std::{
    io::Write,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

#[test]
fn logical_envelope_counter_matches_escaped_utf8_at_the_exact_boundary() {
    let data = json!({"body":"café\n\"\\\u{0000}","items":[true,null,123]});
    let expected = serde_json::to_vec(&LogicalEnvelope {
        status: "ok",
        data: &data,
    })
    .unwrap()
    .len();
    let deadline = Deadline::new(30000);
    assert_eq!(
        logical_envelope_size(&data, expected, &deadline).unwrap(),
        expected
    );
    assert_eq!(
        logical_envelope_size(&data, expected - 1, &deadline).unwrap_err(),
        RetrievalHardError::OutputCapacity("compatibility_envelope")
    );
}

#[test]
fn logical_envelope_counter_refuses_a_large_body_without_exposing_content() {
    let data = json!({"body":"private_definition_".repeat(300000)});
    let error = logical_envelope_size(&data, 4000000, &Deadline::new(30000)).unwrap_err();
    assert_eq!(
        error,
        RetrievalHardError::OutputCapacity("compatibility_envelope")
    );
    assert_eq!(
        error.to_string(),
        "Manifest-balanced retrieval output_capacity: compatibility_envelope"
    );
}

#[test]
fn logical_envelope_counter_uses_the_original_expired_deadline() {
    assert_eq!(
        logical_envelope_size(&json!({"items":[]}), 4000000, &Deadline::new(0)).unwrap_err(),
        RetrievalHardError::Deadline("expired")
    );
}

#[test]
fn logical_envelope_counter_observes_cancellation_between_writes() {
    let cancel = Arc::new(AtomicBool::new(false));
    Deadline::run_bounded(&Deadline::new(30000), Arc::clone(&cancel), || {
        let deadline = Deadline::new(30000);
        let mut counter = EnvelopeCounter {
            deadline: &deadline,
            limit: 100,
            size: 0,
            failure: None,
        };
        counter.write_all(b"first").unwrap();
        cancel.store(true, Ordering::SeqCst);
        assert!(counter.write_all(b"second").is_err());
        assert_eq!(counter.size, 5);
        assert_eq!(
            counter.failure,
            Some(RetrievalHardError::Cancelled("host_cancelled"))
        );
        cancel.store(false, Ordering::SeqCst);
        Ok(())
    })
    .unwrap();
}
