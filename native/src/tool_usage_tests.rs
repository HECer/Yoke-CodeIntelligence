use super::*;
fn d() -> Deadline {
    Deadline::new(20000)
}
#[test]
fn character_estimation_counts_unicode_scalars_and_floors_each_text_independently() {
    let stats = Stats::new(Estimator::CharCount, None, &d()).unwrap();
    for (text, count) in [
        ("", 0),
        ("abc", 0),
        ("abcd", 1),
        ("\u{1f4a1}\u{1f4a1}\u{1f4a1}\u{1f4a1}", 1),
        ("e\u{301}e\u{301}", 1),
        ("a\r\nb", 1),
    ] {
        assert_eq!(stats.count(text, &d()).unwrap(), count);
    }
}
#[test]
fn actual_o200k_matches_official_numeric_vectors_and_special_literals_stay_ordinary() {
    // Numeric facts from OpenAI cookbook blob19ce7689214e0868bf6c8ed8e15c051e3ab18eac.
    let stats = Stats::new(Estimator::TiktokenGpt4o, None, &d()).unwrap();
    for (text, count) in [
        ("", 0),
        ("antidisestablishmentarianism", 6),
        ("2 + 2 = 4", 7),
        ("お誕生日おめでとう", 8),
    ] {
        assert_eq!(stats.count(text, &d()).unwrap(), count);
    }
    for text in ["<|endoftext|>", "<|endofprompt|>"] {
        assert!(stats.count(text, &d()).unwrap() > 1);
    }
    let other = Stats::new(Estimator::TiktokenGpt4o, None, &d()).unwrap();
    if let (Counter::Local(a), Counter::Local(b)) = (&stats.counter, &other.counter) {
        assert!(Arc::ptr_eq(a, b));
    } else {
        panic!("compiled local tokenizer required");
    }
}
#[test]
fn complete_native_representations_produce_only_ephemeral_atomic_counts() {
    let mut stats = Stats::new(Estimator::CharCount, None, &d()).unwrap();
    let measured = stats
        .measure(
            "read_file",
            &json!({"text":"💡💡💡💡"}),
            &json!("abcd"),
            &d(),
        )
        .unwrap();
    assert!(stats.view()["tools"].as_object().unwrap().is_empty());
    stats.commit(measured, &d()).unwrap();
    assert_eq!(
        stats.view()["tools"]["read_file"],
        json!({"numTimesCalled":1,"inputTokens":3,"outputTokens":1})
    );
    let serialized = stats.view().to_string();
    assert!(!serialized.contains("💡") && !serialized.contains("abcd"));
    assert_eq!(stats.view()["nativeByteEstimateChanged"], false);
    assert_eq!(stats.view()["providerBillingEstablished"], false);
}
#[test]
fn overflow_of_any_counter_preserves_the_entire_previous_entry() {
    for field in 0..3 {
        let mut stats = Stats::new(Estimator::CharCount, None, &d()).unwrap();
        let old = Entry {
            num_times_called: if field == 0 { u64::MAX } else { 4 },
            input_tokens: if field == 1 { u64::MAX } else { 2 },
            output_tokens: if field == 2 { u64::MAX } else { 3 },
        };
        stats.entries.insert("read_file".into(), old.clone());
        assert!(
            stats
                .commit(
                    Measured {
                        name: "read_file".into(),
                        input: 1,
                        output: 1
                    },
                    &d()
                )
                .is_err()
        );
        assert_eq!(stats.entries["read_file"], old);
    }
}
#[test]
fn names_tool_capacity_and_both_payload_bounds_refuse_without_partial_updates() {
    let mut stats = Stats::new(Estimator::CharCount, None, &d()).unwrap();
    for name in ["", "../secret", "fs.read", "a\0b"] {
        assert!(stats.preflight(name, &json!({}), &d()).is_err());
    }
    for n in 0..MAX_TOOLS {
        stats
            .commit(
                Measured {
                    name: format!("owned_{n}"),
                    input: 0,
                    output: 0,
                },
                &d(),
            )
            .unwrap();
    }
    let before = stats.view();
    assert!(stats.preflight("new_tool", &json!({}), &d()).is_err());
    stats.preflight("owned_0", &json!({}), &d()).unwrap();
    let too_large = json!("x".repeat(MAX_TEXT_BYTES));
    assert!(
        stats
            .measure("owned_0", &json!({}), &too_large, &d())
            .is_err()
    );
    assert!(
        stats
            .measure("owned_0", &too_large, &json!({}), &d())
            .is_err()
    );
    assert_eq!(stats.view(), before);
}
#[test]
fn expired_deadline_cannot_measure_or_commit_and_clear_is_instance_local() {
    let mut a = Stats::new(Estimator::CharCount, None, &d()).unwrap();
    let mut b = Stats::new(Estimator::CharCount, None, &d()).unwrap();
    let expired = Deadline::new(0);
    assert!(
        a.measure("read_file", &json!({}), &json!({}), &expired)
            .is_err()
    );
    assert!(
        a.commit(
            Measured {
                name: "read_file".into(),
                input: 1,
                output: 1
            },
            &expired
        )
        .is_err()
    );
    for stats in [&mut a, &mut b] {
        stats
            .commit(
                Measured {
                    name: "read_file".into(),
                    input: 1,
                    output: 1,
                },
                &d(),
            )
            .unwrap();
    }
    a.clear(&d()).unwrap();
    assert!(a.entries.is_empty());
    assert_eq!(b.entries["read_file"].num_times_called, 1);
}
#[test]
fn selecting_remote_name_never_constructs_network_or_credential_authority() {
    assert!(Stats::new(Estimator::AnthropicClaudeSonnet4, None, &d()).is_err());
    for value in [
        json!(true),
        json!("unknown"),
        json!({"endpoint":"http://127.0.0.1"}),
    ] {
        assert!(serde_json::from_value::<Estimator>(value).is_err());
    }
}
#[test]
fn both_payloads_are_admitted_before_remote_credentials_or_network_attempts() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    let reads = Arc::new(AtomicUsize::new(0));
    let counter = reads.clone();
    let host = http::Host::new(
        http::Spec {
            endpoint: "https://api.anthropic.com/v1/messages/count_tokens".into(),
            credential_name: "owned".into(),
            allow_loopback_http: false,
            additional_root_certificates: vec![],
        },
        true,
        Arc::new(move |_| {
            counter.fetch_add(1, Ordering::SeqCst);
            Ok(Zeroizing::new("owned-placeholder".into()))
        }),
    )
    .unwrap();
    let stats = Stats::new(Estimator::AnthropicClaudeSonnet4, Some(&host), &d()).unwrap();
    assert!(
        stats
            .measure(
                "read_file",
                &json!({}),
                &json!("x".repeat(MAX_TEXT_BYTES)),
                &d()
            )
            .is_err()
    );
    assert_eq!(reads.load(Ordering::SeqCst), 0);
    assert_eq!(host.attempts(), 0);
    host.revoke();
    assert!(stats.preflight("read_file", &json!({}), &d()).is_err());
    assert_eq!(reads.load(Ordering::SeqCst), 0);
}
#[test]
fn remote_accounting_cannot_label_success_or_partial_native_envelopes_as_local() {
    let host = http::Host::new(
        http::Spec {
            endpoint: "https://api.anthropic.com/v1/messages/count_tokens".into(),
            credential_name: "owned".into(),
            allow_loopback_http: false,
            additional_root_certificates: vec![],
        },
        true,
        Arc::new(|_| anyhow::bail!("owned_reader_must_not_run")),
    )
    .unwrap();
    let remote = Stats::new(Estimator::AnthropicClaudeSonnet4, Some(&host), &d()).unwrap();
    let local = Stats::new(Estimator::CharCount, None, &d()).unwrap();
    for status in ["ok", "partial"] {
        let original = json!({"schemaVersion":1,"status":status,"data":{"source":"owned"},"warnings":["kept"],"metrics":{"serializedBytes":120,"estimatedTokens":40,"tokenCount":"estimated","tokenMethod":"utf8_bytes_div_3","localOnly":true,"externalUsage":"none","modelCalls":0}});
        let mut unchanged = original.clone();
        assert!(!local.annotate_native_result(&mut unchanged));
        assert_eq!(unchanged, original);
        let mut selected = original.clone();
        assert!(remote.annotate_native_result(&mut selected));
        assert_eq!(selected["metrics"]["localOnly"], false);
        assert_eq!(selected["metrics"]["externalUsage"], "unknown");
        for key in [
            "serializedBytes",
            "estimatedTokens",
            "tokenCount",
            "tokenMethod",
            "modelCalls",
        ] {
            assert_eq!(selected["metrics"][key], original["metrics"][key]);
        }
        assert_eq!(selected["metrics"]["toolTokenCounting"]["requests"], 2);
        assert_eq!(
            selected["metrics"]["toolTokenCounting"]["generationCalls"],
            0
        );
        assert_eq!(
            selected["metrics"]["toolTokenCounting"]["nativeSizeMetricsScope"],
            "underlying_native_result_before_tool_token_counting_metadata"
        );
        assert_eq!(selected["data"], original["data"]);
        assert_eq!(selected["warnings"], original["warnings"]);
    }
    let mut error = json!({"schemaVersion":1,"status":"error","metrics":{"localOnly":true}});
    let before = error.clone();
    assert!(!remote.annotate_native_result(&mut error));
    assert_eq!(error, before);
    assert_eq!(host.attempts(), 0);
}
