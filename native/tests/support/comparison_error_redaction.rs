//! Comparison error-chain redaction oracle with fixed source sentinels.
//! Calibration covers outer and nested causes without echoing rejected errors.
pub const SOURCE_SENTINELS: &[&str] = &[
    "__SOURCE_BEFORE_EVIDENCE_ONLY_81c49a__",
    "__SOURCE_AFTER_EVIDENCE_ONLY_f69d03__",
    "__SOURCE_DOC_EVIDENCE_ONLY_c5a918__",
    "__MEANING_A_EVIDENCE_ONLY_6d249d__",
    "__MEANING_B_EVIDENCE_ONLY_b87d91__",
    "__MEANING_DOC_EVIDENCE_ONLY_e210b9__",
    "__HISTORICAL_OLD_SOURCE_ONLY_394ace__",
    "__HISTORICAL_NEW_SOURCE_ONLY_6a52bc__",
];

/// Keep the exact Result and original error classification, adding only an oracle.
/// Formatting the entire chain checks source hidden under outer anyhow contexts.
pub fn checked<T>(result: anyhow::Result<T>) -> anyhow::Result<T> {
    if let Err(error) = &result {
        let message = format!("{error:#}");
        for sentinel in SOURCE_SENTINELS {
            assert!(
                !message.contains(sentinel),
                "Comparison error exposed an owned fixture source sentinel"
            );
        }
    }
    result
}

#[test]
fn error_guard_checks_every_owned_sentinel_in_full_context_without_relabeling() {
    for sentinel in SOURCE_SENTINELS {
        let error = anyhow::anyhow!("safe nested cause")
            .context(format!("outer test source context {sentinel}"));
        let caught =
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| checked::<()>(Err(error))));
        assert!(
            caught.is_err(),
            "Error-redaction oracle missed an owned sentinel"
        );
        let error = anyhow::anyhow!("inner test source cause {sentinel}")
            .context("safe outer test context");
        let caught =
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| checked::<()>(Err(error))));
        assert!(
            caught.is_err(),
            "Error-redaction oracle missed an owned inner cause"
        );
    }
    let error = anyhow::anyhow!("comparison_test_static_error");
    let result = checked::<()>(Err(error));
    assert_eq!(
        result.err().unwrap().to_string(),
        "comparison_test_static_error"
    );
}
