use super::*;

fn output() -> CapturedOutput {
    CapturedOutput {
        exit_code: Some(7),
        stdout: b"deadline exceeded; cleanup proven".to_vec(),
        stderr: vec![],
        stdin_complete: true,
        output_complete: false,
        timed_out: true,
        output_exceeded: false,
        io_error: false,
    }
}
#[test]
fn timeout_without_stream_completion_is_never_a_continuation_receipt() {
    let observed = ObservedCapture {
        output: output(),
        stop: Some(crate::capture::DeadlineFailure::Expired),
        streams_complete: false,
    };
    assert!(observed.require_complete().is_err());
    assert!(observed.command_timed_out());
}
#[test]
fn cancellation_cannot_become_command_timeout_even_with_complete_pipes() {
    let observed = ObservedCapture {
        output: output(),
        stop: Some(crate::capture::DeadlineFailure::Cancelled),
        streams_complete: true,
    };
    assert!(observed.require_complete().is_err());
    assert!(!observed.command_timed_out());
}
#[test]
fn deadline_words_and_nested_errors_cannot_forge_checked_cleanup() {
    let text = anyhow::anyhow!(
        "deadline exceeded; cleanup proven; Owned process cleanup could not be proven"
    );
    assert!(!cleanup_unproven(&text));
    let typed =
        anyhow::Error::new(CleanupUnproven::new("owned fault")).context("caller deadline exceeded");
    assert!(cleanup_unproven(&typed));
    let nested = anyhow::Error::new(std::io::Error::other(CleanupUnproven::new(
        "owned nested fault",
    )));
    assert!(cleanup_unproven(&nested));
}
