use super::*;

#[test]
fn dashboard_strings_preserve_u64_counters_without_changing_native_numeric_view() {
    let mut stats = Stats::new(Estimator::CharCount, None, &Deadline::new(5000)).unwrap();
    stats.entries.insert(
        "read_file".into(),
        Entry {
            num_times_called: u64::MAX,
            input_tokens: 9_007_199_254_740_993,
            output_tokens: u64::MAX,
        },
    );
    let view = stats.dashboard_view(u64::MAX);
    assert_eq!(view["revision"], u64::MAX.to_string());
    assert_eq!(
        view["tools"]["read_file"]["numTimesCalled"],
        u64::MAX.to_string()
    );
    assert_eq!(
        view["tools"]["read_file"]["inputTokens"],
        "9007199254740993"
    );
    assert_eq!(
        stats.view()["tools"]["read_file"]["inputTokens"].as_u64(),
        Some(9_007_199_254_740_993)
    );
}

#[test]
fn largest_admitted_dashboard_statistics_remain_bounded_and_contain_no_source_text() {
    let mut stats = Stats::new(Estimator::CharCount, None, &Deadline::new(5000)).unwrap();
    for n in 0..MAX_TOOLS {
        stats.entries.insert(
            format!("{}_{n:02}", "t".repeat(96)),
            Entry {
                num_times_called: u64::MAX,
                input_tokens: u64::MAX,
                output_tokens: u64::MAX,
            },
        );
    }
    let view = stats.dashboard_view(7);
    assert_eq!(view["tools"].as_object().unwrap().len(), MAX_TOOLS);
    assert!(serde_json::to_vec(&view).unwrap().len() <= 32000);
    assert_eq!(view["hostGrantRemoteAttempts"], "0");
    assert!(view.get("source").is_none());
}
