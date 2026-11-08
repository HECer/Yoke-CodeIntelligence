use yoke as test_yoke;
#[path = "support/git_impact_fixture.rs"]
mod fixture;
use fixture::{Fixture, MOVED_BASE, MOVED_HEAD, all_quotes, assert_static_error, state};
use serde_json::{Value, json};
use yoke::{
    capture::Deadline,
    git_workflows::{
        self,
        impact_details::{self, DetailOptions, ImpactDetails},
    },
};

fn details(fixture: &Fixture, options: &DetailOptions) -> ImpactDetails {
    impact_details::derive(
        &fixture.engine.store,
        fixture.id(),
        options,
        &Deadline::new(30_000),
    )
    .expect("source-bound Git impact details feature")
}

fn candidates(details: &Value) -> Vec<(&str, &str, u64)> {
    details["renameCandidates"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| {
            (
                row["base"]["path"].as_str().unwrap(),
                row["head"]["path"].as_str().unwrap(),
                row["score"].as_u64().unwrap(),
            )
        })
        .collect()
}

// Catches lost cross-path attribution, altered score inputs, quote-side mixing,
// changed original claims, dropped parallel calls, and new persistence.
#[test]
fn modified_move_keeps_all_original_claims_parallel_ids_and_exact_two_side_quotes() {
    let f = Fixture::moved();
    let before = state(&f.engine);
    let output = details(&f, &DetailOptions::default());
    let value = output.value();
    assert_eq!(candidates(value), [("src/alt-ü.rs", "src/neu-λ.rs", 6666)]);
    let candidate = &value["renameCandidates"][0];
    assert_eq!(candidate["kind"], "modified_content_candidate");
    assert_eq!(candidate["baseLineCount"], 6);
    assert_eq!(candidate["headLineCount"], 6);
    assert_eq!(candidate["matchedLineCount"], 4);
    assert_eq!(candidate["identityEstablished"], false);
    assert_eq!(candidate["ambiguous"], false);
    assert_eq!(value["candidateSummary"]["pairsConsidered"], 1);
    assert_eq!(value["candidateSummary"]["similarityWork"], 12);
    assert_eq!(value["candidateSummary"]["similarityLines"], 12);
    for key in [
        "workspaceId",
        "baseSnapshotId",
        "headSnapshotId",
        "baseGraphHash",
        "headGraphHash",
    ] {
        assert_eq!(value[key], f.report[key]);
    }
    assert_eq!(value["reportArtifactId"], f.report["artifactId"]);
    fixture::assert_originals_and_complete_quotes(&f, value);
    let occurrences = value["impactObservations"].as_array().unwrap();
    assert_eq!(
        occurrences
            .iter()
            .filter(|row| row["original"]["relation"] == "calls")
            .count(),
        4
    );
    assert_eq!(value["coverage"]["quotesComplete"], true);
    assert_eq!(value["complete"], false);
    assert_eq!(state(&f.engine), before);
    assert_eq!(
        std::fs::read(f.dir.path().join(".git/index")).unwrap(),
        f.index_bytes
    );
}

#[test]
fn unique_identical_content_including_empty_files_does_not_prove_rename_identity() {
    for text in ["fn same() {}\n", ""] {
        let f = Fixture::new(&[("old.rs", text)], &[("new.rs", text)]);
        let output = details(&f, &DetailOptions::default());
        let row = &output.value()["renameCandidates"][0];
        assert_eq!(candidates(output.value()), [("old.rs", "new.rs", 10000)]);
        assert_eq!(row["kind"], "unique_identical_content");
        assert_eq!(row["baseIdenticalCount"], 1);
        assert_eq!(row["headIdenticalCount"], 1);
        assert_eq!(row["identityEstablished"], false);
        assert_eq!(row["ambiguous"], false);
    }
}

#[test]
fn duplicate_identical_groups_retain_the_complete_ambiguous_cross_product() {
    let text = "fn same() {}\n";
    let f = Fixture::new(
        &[("a.rs", text), ("b.rs", text)],
        &[("c.rs", text), ("d.rs", text)],
    );
    let output = details(&f, &DetailOptions::default());
    assert_eq!(
        candidates(output.value()),
        [
            ("a.rs", "c.rs", 10000),
            ("a.rs", "d.rs", 10000),
            ("b.rs", "c.rs", 10000),
            ("b.rs", "d.rs", 10000)
        ]
    );
    for row in output.value()["renameCandidates"].as_array().unwrap() {
        assert_eq!(row["kind"], "identical_content_candidate");
        assert_eq!(row["baseCandidateCount"], 2);
        assert_eq!(row["headCandidateCount"], 2);
        assert_eq!(row["baseIdenticalCount"], 2);
        assert_eq!(row["headIdenticalCount"], 2);
        assert_eq!(row["ambiguous"], true);
        assert_eq!(row["identityEstablished"], false);
    }
}

#[test]
fn an_exact_content_pair_stays_ambiguous_when_a_modified_partner_also_qualifies() {
    let f = Fixture::new(
        &[("old.rs", MOVED_BASE)],
        &[("exact.rs", MOVED_BASE), ("modified.rs", MOVED_HEAD)],
    );
    let output = details(&f, &DetailOptions::default());
    assert_eq!(
        candidates(output.value()),
        [
            ("old.rs", "exact.rs", 10000),
            ("old.rs", "modified.rs", 6666)
        ]
    );
    for row in output.value()["renameCandidates"].as_array().unwrap() {
        assert_eq!(row["ambiguous"], true);
        assert_eq!(row["baseCandidateCount"], 2);
    }
    assert_eq!(
        output.value()["renameCandidates"][0]["kind"],
        "unique_identical_content"
    );
}

#[test]
fn tied_modified_candidates_are_all_visible_in_stable_path_order() {
    let f = Fixture::new(
        &[("old.rs", MOVED_BASE)],
        &[("z.rs", MOVED_HEAD), ("a.rs", MOVED_HEAD)],
    );
    let output = details(&f, &DetailOptions::default());
    assert_eq!(
        candidates(output.value()),
        [("old.rs", "a.rs", 6666), ("old.rs", "z.rs", 6666)]
    );
    assert!(
        output.value()["renameCandidates"]
            .as_array()
            .unwrap()
            .iter()
            .all(|row| row["ambiguous"] == true)
    );
}

#[test]
fn similarity_counts_repeated_normalized_lines_with_multiplicity() {
    let f = Fixture::new(
        &[("old.rs", "// a\n// a\n// b\n")],
        &[("new.rs", "// a\n// b\n")],
    );
    let output = details(&f, &DetailOptions::default());
    assert_eq!(candidates(output.value()), [("old.rs", "new.rs", 8000)]);
    let row = &output.value()["renameCandidates"][0];
    assert_eq!(row["baseLineCount"], 3);
    assert_eq!(row["headLineCount"], 2);
    assert_eq!(row["matchedLineCount"], 2);
}

#[test]
fn exact_similarity_threshold_is_inclusive_and_lower_nonzero_scores_are_rejected() {
    let f = Fixture::new(
        &[("old.rs", "// a\n// b\n// c\n// d\n// e\n")],
        &[
            ("boundary.rs", "// a\n// b\n// c\n// f\n// g\n"),
            ("below.rs", "// a\n// b\n// u\n// v\n// w\n"),
        ],
    );
    let output = details(&f, &DetailOptions::default());
    assert_eq!(
        candidates(output.value()),
        [("old.rs", "boundary.rs", 6000)]
    );
    assert_eq!(output.value()["candidateSummary"]["qualifiedPairs"], 1);
    assert_eq!(output.value()["candidateSummary"]["belowThresholdPairs"], 1);
    assert_eq!(output.value()["candidateSummary"]["noSignalPairs"], 0);
}

#[test]
fn unequal_whitespace_only_sources_report_no_signal_instead_of_a_rename() {
    let f = Fixture::new(&[("old.rs", " \r\n")], &[("new.rs", "\t\n")]);
    let output = details(&f, &DetailOptions::default());
    assert!(candidates(output.value()).is_empty());
    assert_eq!(output.value()["candidateSummary"]["pairsConsidered"], 1);
    assert_eq!(output.value()["candidateSummary"]["noSignalPairs"], 1);
    assert_eq!(output.value()["candidateSummary"]["belowThresholdPairs"], 0);
}

#[test]
fn same_path_edits_are_excluded_and_unrelated_added_deleted_sources_stay_unpaired() {
    let f = Fixture::new(
        &[
            ("gone.rs", "fn alpha() {}\n"),
            ("same.rs", "fn same() {}\n"),
        ],
        &[
            ("new.rs", "fn beta() { let z = 9; }\n"),
            ("same.rs", "fn same() { let x = 1; }\n"),
        ],
    );
    let output = details(&f, &DetailOptions::default());
    assert!(candidates(output.value()).is_empty());
    assert_eq!(output.value()["candidateSummary"]["pairsConsidered"], 1);
    assert_eq!(output.value()["candidateSummary"]["belowThresholdPairs"], 1);
    assert_eq!(output.value()["changes"].as_array().unwrap().len(), 3);
    fixture::assert_originals_and_complete_quotes(&f, output.value());
}

#[test]
fn score_normalizes_only_horizontal_ascii_edges_and_crlf_not_quote_bytes() {
    let before = " \tfn a() {}\r\n\tfn b() {}  \r\n";
    let after = "fn a() {}\nfn b() {}\n";
    let f = Fixture::new(&[("old.rs", before)], &[("new.rs", after)]);
    let output = details(&f, &DetailOptions::default());
    assert_eq!(candidates(output.value()), [("old.rs", "new.rs", 10000)]);
    assert_eq!(
        output.value()["renameCandidates"][0]["kind"],
        "modified_content_candidate"
    );
    fixture::assert_originals_and_complete_quotes(&f, output.value());
    assert!(
        all_quotes(output.value())
            .iter()
            .any(|quote| quote["text"] == before)
    );
    assert!(
        all_quotes(output.value())
            .iter()
            .any(|quote| quote["text"] == after)
    );
}

#[test]
fn similarity_preserves_unicode_case_composition_and_unterminated_carriage_return() {
    for (before, after, expected) in [
        ("// café\n// x\n// y\n", "// CAFÉ\n// x\n// y\n", Some(6666)),
        (
            "// café\n// x\n// y\n",
            "// cafe\u{301}\n// x\n// y\n",
            Some(6666),
        ),
        ("// a\r", "// a", None),
    ] {
        let f = Fixture::new(&[("old.rs", before)], &[("new.rs", after)]);
        let output = details(&f, &DetailOptions::default());
        match expected {
            Some(score) => assert_eq!(candidates(output.value()), [("old.rs", "new.rs", score)]),
            None => assert!(candidates(output.value()).is_empty()),
        }
        fixture::assert_originals_and_complete_quotes(&f, output.value());
    }
}

#[test]
fn insertion_empty_ranges_and_absent_sides_remain_distinct_exact_source_values() {
    let f = Fixture::new(
        &[("gone.rs", "fn gone() {}\n"), ("same.rs", "fn one() {}\n")],
        &[
            ("new.rs", "fn new() {}\n"),
            ("same.rs", "fn one() {}\nfn two() {}\n"),
        ],
    );
    let output = details(&f, &DetailOptions::default());
    fixture::assert_originals_and_complete_quotes(&f, output.value());
    assert!(
        all_quotes(output.value())
            .iter()
            .any(|quote| quote["text"] == "" && quote["status"] == "complete")
    );
    assert!(
        output.value()["changes"]
            .as_array()
            .unwrap()
            .iter()
            .any(|row| row["original"]["status"] == "deleted" && row["hunks"][0]["head"].is_null())
    );
}

#[test]
fn quote_prefixes_stop_on_utf8_boundaries_and_report_actual_excerpt_ranges() {
    let f = Fixture::new(
        &[("old.rs", "// é🙂\nfn changed() {}\n")],
        &[("new.rs", "// é🙂\nfn changed() { let a = 1; }\n")],
    );
    let output = details(
        &f,
        &DetailOptions {
            max_quote_bytes: 4,
            ..Default::default()
        },
    );
    let quotes = all_quotes(output.value());
    assert_eq!(quotes[0]["text"], "// ");
    let mut emitted = 0usize;
    for quote in quotes {
        let source = f.source_for_quote(quote);
        let start = quote["requested"]["startByte"].as_u64().unwrap() as usize;
        let requested_end = quote["requested"]["endByte"].as_u64().unwrap() as usize;
        let mut end = requested_end.min(start + 4);
        while !source.is_char_boundary(end) {
            end -= 1;
        }
        if end == start && requested_end != start {
            assert_eq!(quote["status"], "omitted");
            assert!(quote["range"].is_null() && quote["text"].is_null());
        } else {
            assert_eq!(quote["text"], &source[start..end]);
            assert_eq!(quote["range"], fixture::expected_range(source, start, end));
            emitted += end - start;
        }
        if end < requested_end {
            assert_eq!(quote["reason"], "per_quote_byte_limit");
        }
    }
    assert_eq!(output.value()["coverage"]["emittedQuoteBytes"], emitted);
    assert_eq!(output.value()["coverage"]["quotesComplete"], false);
}

#[test]
fn zero_quote_caps_omit_nonempty_ranges_but_keep_genuine_empty_ranges_complete() {
    let f = Fixture::new(
        &[("same.rs", "fn one() {}\n")],
        &[("same.rs", "fn one() {}\nfn two() {}\n")],
    );
    for options in [
        DetailOptions {
            max_quote_bytes: 0,
            ..Default::default()
        },
        DetailOptions {
            max_total_quote_bytes: 0,
            ..Default::default()
        },
    ] {
        let output = details(&f, &options);
        let quotes = all_quotes(output.value());
        assert!(
            quotes
                .iter()
                .any(|quote| quote["text"] == "" && quote["status"] == "complete")
        );
        for quote in quotes {
            if quote["requested"]["startByte"] != quote["requested"]["endByte"] {
                assert_eq!(quote["status"], "omitted");
                assert!(quote["text"].is_null() && quote["range"].is_null());
                assert_eq!(
                    quote["reason"],
                    if options.max_quote_bytes == 0 {
                        "per_quote_byte_limit"
                    } else {
                        "aggregate_quote_byte_limit"
                    }
                );
            }
        }
        assert_eq!(output.value()["coverage"]["quotesComplete"], false);
        assert_eq!(output.value()["coverage"]["emittedQuoteBytes"], 0);
    }
}

#[test]
fn a_positive_aggregate_quote_cap_is_consumed_in_documented_occurrence_order() {
    let f = Fixture::moved();
    let limit = MOVED_BASE.len() + 2;
    let output = details(
        &f,
        &DetailOptions {
            max_total_quote_bytes: limit,
            ..Default::default()
        },
    );
    let quotes = all_quotes(output.value());
    assert_eq!(quotes[0]["text"], MOVED_BASE);
    assert_eq!(quotes[0]["status"], "complete");
    assert_eq!(quotes[1]["text"], "//");
    assert_eq!(quotes[1]["status"], "excerpt");
    assert_eq!(quotes[1]["reason"], "aggregate_quote_byte_limit");
    assert_eq!(
        quotes[1]["range"],
        fixture::expected_range(MOVED_HEAD, 0, 2)
    );
    for quote in &quotes[2..] {
        assert_eq!(quote["status"], "omitted");
        assert_eq!(quote["reason"], "aggregate_quote_byte_limit");
        assert!(quote["range"].is_null() && quote["text"].is_null());
    }
    assert_eq!(output.value()["coverage"]["emittedQuoteBytes"], limit);
    assert_eq!(output.value()["coverage"]["completeQuotes"], 1);
    assert_eq!(output.value()["coverage"]["excerptedQuotes"], 1);
    assert_eq!(
        output.value()["coverage"]["omittedQuotes"],
        quotes.len() - 2
    );
    assert_eq!(output.value()["coverage"]["quotesComplete"], false);
}

#[test]
fn admitted_control_character_source_is_omitted_without_fabricated_safe_quote_text() {
    let before = "// \u{1b}R5_UNSAFE_SOURCE\nfn changed() {}\n";
    let after = "// \u{1b}R5_UNSAFE_SOURCE\nfn changed() { let a = 1; }\n";
    let f = Fixture::new(&[("old.rs", before)], &[("new.rs", after)]);
    assert_eq!(
        git_workflows::load(&f.engine.store, f.id(), &Deadline::new(30_000)).unwrap(),
        f.report
    );
    let output = details(&f, &DetailOptions::default());
    for quote in all_quotes(output.value()) {
        assert_eq!(quote["status"], "omitted");
        assert_eq!(quote["reason"], "unsafe_source_excerpt");
        assert!(quote["text"].is_null() && quote["range"].is_null());
    }
    assert_eq!(output.value()["coverage"]["quotesComplete"], false);
    assert!(
        !String::from_utf8(output.canonical_bytes().to_vec())
            .unwrap()
            .contains("R5_UNSAFE_SOURCE")
    );
}

#[test]
fn exact_pair_line_work_source_and_row_budgets_succeed_and_one_below_refuses() {
    let f = Fixture::moved();
    let baseline = details(&f, &DetailOptions::default());
    let required_rows = fixture::rows(baseline.value());
    assert_eq!(baseline.value()["rowUnits"], required_rows);
    let required_source = MOVED_BASE.len() + MOVED_HEAD.len();
    assert_eq!(baseline.value()["sourceBytes"], required_source);
    let exact = DetailOptions {
        max_candidate_pairs: 1,
        max_similarity_lines: 12,
        max_similarity_work: 12,
        max_source_bytes: required_source,
        max_rows: required_rows,
        ..Default::default()
    };
    details(&f, &exact);
    let before = state(&f.engine);
    for dimension in 0..5 {
        let mut options = exact.clone();
        match dimension {
            0 => options.max_candidate_pairs -= 1,
            1 => options.max_similarity_lines -= 1,
            2 => options.max_similarity_work -= 1,
            3 => options.max_source_bytes -= 1,
            _ => options.max_rows -= 1,
        }
        assert_static_error(
            impact_details::derive(&f.engine.store, f.id(), &options, &Deadline::new(30_000))
                .unwrap_err(),
            "git_impact_details_capacity_exceeded",
        );
        assert_eq!(state(&f.engine), before);
    }
}

#[test]
fn identical_source_objects_are_charged_once_per_hash_across_both_scoped_sides() {
    let text = "fn same() {}\n";
    let f = Fixture::new(
        &[("a.rs", text), ("b.rs", text)],
        &[("c.rs", text), ("d.rs", text)],
    );
    let options = DetailOptions {
        max_source_bytes: text.len(),
        ..Default::default()
    };
    let output = details(&f, &options);
    assert_eq!(output.value()["sourceBytes"], text.len());
    assert_eq!(output.value()["candidateSummary"]["similarityLines"], 4);
    assert_eq!(output.value()["candidateSummary"]["similarityWork"], 8);
    assert_eq!(candidates(output.value()).len(), 4);
    assert_static_error(
        impact_details::derive(
            &f.engine.store,
            f.id(),
            &DetailOptions {
                max_source_bytes: text.len() - 1,
                ..options
            },
            &Deadline::new(30_000),
        )
        .unwrap_err(),
        "git_impact_details_capacity_exceeded",
    );
}

#[test]
fn zero_hard_budgets_are_real_limits_and_never_restore_defaults() {
    let f = Fixture::moved();
    details(&f, &DetailOptions::default());
    let before = state(&f.engine);
    for dimension in 0..6 {
        let mut options = DetailOptions::default();
        match dimension {
            0 => options.max_candidate_pairs = 0,
            1 => options.max_similarity_lines = 0,
            2 => options.max_similarity_work = 0,
            3 => options.max_source_bytes = 0,
            4 => options.max_rows = 0,
            _ => options.max_output_bytes = 0,
        }
        assert_static_error(
            impact_details::derive(&f.engine.store, f.id(), &options, &Deadline::new(30_000))
                .unwrap_err(),
            "git_impact_details_capacity_exceeded",
        );
        assert_eq!(state(&f.engine), before);
    }
}

#[test]
fn zero_charge_dimensions_succeed_instead_of_becoming_unconditional_refusals() {
    let same_path = Fixture::new(
        &[("same.rs", "fn one() {}\n")],
        &[("same.rs", "fn one() {}\nfn two() {}\n")],
    );
    let options = DetailOptions {
        max_candidate_pairs: 0,
        max_similarity_lines: 0,
        max_similarity_work: 0,
        ..Default::default()
    };
    let output = details(&same_path, &options);
    assert_eq!(output.value()["candidateSummary"]["pairsConsidered"], 0);
    let empty = Fixture::new(&[("old.rs", "")], &[("new.rs", "")]);
    let output = details(
        &empty,
        &DetailOptions {
            max_candidate_pairs: 1,
            max_source_bytes: 0,
            max_similarity_lines: 0,
            max_similarity_work: 0,
            ..Default::default()
        },
    );
    assert_eq!(output.value()["sourceBytes"], 0);
    assert_eq!(candidates(output.value()), [("old.rs", "new.rs", 10000)]);
}

#[test]
fn canonical_full_result_id_row_counter_and_exact_wire_limit_are_independent() {
    let f = Fixture::moved();
    let mut options = DetailOptions::default();
    let mut output = details(&f, &options);
    for _ in 0..8 {
        let actual = serde_json::to_vec(&fixture::sorted(output.value())).unwrap();
        assert_eq!(output.canonical_bytes(), actual);
        assert_eq!(output.value()["wireBytes"], actual.len());
        assert_eq!(output.value()["rowUnits"], fixture::rows(output.value()));
        let mut identity = fixture::sorted(output.value());
        identity.as_object_mut().unwrap().remove("resultId");
        assert_eq!(output.value()["resultId"], yoke::model::identity(&identity));
        if options.max_output_bytes == actual.len() {
            break;
        }
        options.max_output_bytes = actual.len();
        output = details(&f, &options);
    }
    assert_eq!(
        options.max_output_bytes,
        output.canonical_bytes().len(),
        "stable decimal-width exact boundary"
    );
    options.max_output_bytes -= 1;
    assert_static_error(
        impact_details::derive(&f.engine.store, f.id(), &options, &Deadline::new(30_000))
            .unwrap_err(),
        "git_impact_details_capacity_exceeded",
    );
}

#[test]
fn expired_deadline_refuses_without_sql_cas_or_transaction_mutation() {
    let f = Fixture::moved();
    details(&f, &DetailOptions::default());
    let before = state(&f.engine);
    assert_static_error(
        impact_details::derive(
            &f.engine.store,
            f.id(),
            &DetailOptions::default(),
            &Deadline::new(0),
        )
        .unwrap_err(),
        "git_impact_details_deadline_exceeded",
    );
    f.engine.store.clear_deadline();
    assert_eq!(state(&f.engine), before);
}

#[test]
fn replay_uses_report_snapshots_after_live_files_and_latest_selection_change() {
    let mut f = Fixture::moved();
    let original = details(&f, &DetailOptions::default())
        .canonical_bytes()
        .to_vec();
    std::fs::remove_file(f.dir.path().join("src/neu-λ.rs")).unwrap();
    std::fs::write(f.dir.path().join("live.rs"), "fn unrelated() {}\n").unwrap();
    f.engine.index(&Deadline::new(30_000)).unwrap();
    assert_ne!(
        f.engine.store.latest().unwrap().unwrap(),
        f.report["headSnapshotId"]
    );
    let before = state(&f.engine);
    assert_eq!(
        details(&f, &DetailOptions::default()).canonical_bytes(),
        original
    );
    assert_eq!(state(&f.engine), before);
}

#[test]
fn historical_epoch_eight_and_nine_reports_keep_their_bindings_after_host_epoch_changes() {
    use yoke::analysis::AnalyzerEpoch;
    for (epoch, other) in [
        (AnalyzerEpoch::V8, AnalyzerEpoch::V9),
        (AnalyzerEpoch::V9, AnalyzerEpoch::V8),
    ] {
        let mut f = Fixture::new_with_profile(
            &[("src/alt-ü.rs", MOVED_BASE)],
            &[("src/neu-λ.rs", MOVED_HEAD)],
            yoke::capture::CaptureOptions::default(),
            epoch,
        );
        let old_bytes = details(&f, &DetailOptions::default())
            .canonical_bytes()
            .to_vec();
        for side in ["base", "head"] {
            let snapshot = f
                .engine
                .store
                .snapshot(f.report[format!("{side}SnapshotId")].as_str().unwrap())
                .unwrap();
            assert_eq!(
                yoke::capture::analyzer_epoch_for_snapshot_with_deadline(
                    &f.engine.store,
                    &snapshot,
                    &Deadline::new(30_000)
                )
                .unwrap(),
                epoch
            );
        }
        f.engine.capture_epoch = other;
        f.engine.index(&Deadline::new(30_000)).unwrap();
        assert_ne!(
            f.engine.store.latest().unwrap().unwrap(),
            f.report["headSnapshotId"]
        );
        let before = state(&f.engine);
        assert_eq!(
            details(&f, &DetailOptions::default()).canonical_bytes(),
            old_bytes
        );
        assert_eq!(state(&f.engine), before);
    }
}

#[test]
fn report_id_authority_rejects_missing_cross_workspace_and_resealed_fabricated_reports() {
    let f = Fixture::moved();
    details(&f, &DetailOptions::default());
    let foreign = tempfile::TempDir::new().unwrap();
    let other = yoke::Engine::open(foreign.path()).unwrap();
    assert_static_error(
        impact_details::derive(
            &other.store,
            f.id(),
            &DetailOptions::default(),
            &Deadline::new(30_000),
        )
        .unwrap_err(),
        "git_impact_details_input_invalid",
    );
    assert_static_error(
        impact_details::derive(
            &f.engine.store,
            &"f".repeat(64),
            &DetailOptions::default(),
            &Deadline::new(30_000),
        )
        .unwrap_err(),
        "git_impact_details_input_invalid",
    );
    let mut forged = f.report.clone();
    forged.as_object_mut().unwrap().remove("artifactId");
    forged["changes"] = json!([]);
    let id = f
        .engine
        .store
        .artifact(&serde_json::to_vec(&forged).unwrap())
        .unwrap();
    let mut registration: Value = f
        .engine
        .store
        .get_record(git_workflows::REPORT_KIND, f.id())
        .unwrap();
    registration["artifactId"] = json!(id);
    f.engine
        .store
        .record(git_workflows::REPORT_KIND, &id, &registration)
        .unwrap();
    let before = state(&f.engine);
    assert_static_error(
        impact_details::derive(
            &f.engine.store,
            &id,
            &DetailOptions::default(),
            &Deadline::new(30_000),
        )
        .unwrap_err(),
        "git_impact_details_input_invalid",
    );
    assert_eq!(state(&f.engine), before);
}

#[test]
fn changed_source_cas_refuses_with_a_static_error_and_no_live_source_fallback() {
    let f = Fixture::moved();
    details(&f, &DetailOptions::default());
    let hash = yoke::model::hash(MOVED_BASE);
    let mut altered = MOVED_BASE.as_bytes().to_vec();
    altered[0] = b'!';
    std::fs::write(f.engine.store.dir.join("blobs").join(hash), altered).unwrap();
    let before = state(&f.engine);
    assert_static_error(
        impact_details::derive(
            &f.engine.store,
            f.id(),
            &DetailOptions::default(),
            &Deadline::new(30_000),
        )
        .unwrap_err(),
        "git_impact_details_input_invalid",
    );
    assert_eq!(state(&f.engine), before);
}

#[test]
fn changed_or_missing_captured_graph_refuses_without_reanalysis_or_publication() {
    let f = Fixture::moved();
    details(&f, &DetailOptions::default());
    let snapshot = f.report["baseSnapshotId"].as_str().unwrap();
    let original: String = f
        .engine
        .store
        .db
        .query_row("SELECT value FROM graphs WHERE id=?1", [snapshot], |row| {
            row.get(0)
        })
        .unwrap();
    let mut changed: Value = serde_json::from_str(&original).unwrap();
    changed["observations"][0]["relation"] = json!("R5_TAMPER_SENTINEL");
    f.engine
        .store
        .db
        .execute(
            "UPDATE graphs SET value=?1 WHERE id=?2",
            rusqlite::params![serde_json::to_string(&changed).unwrap(), snapshot],
        )
        .unwrap();
    let before = state(&f.engine);
    assert_static_error(
        impact_details::derive(
            &f.engine.store,
            f.id(),
            &DetailOptions::default(),
            &Deadline::new(30_000),
        )
        .unwrap_err(),
        "git_impact_details_input_invalid",
    );
    assert_eq!(state(&f.engine), before);
    f.engine
        .store
        .db
        .execute(
            "UPDATE graphs SET value=?1 WHERE id=?2",
            rusqlite::params![original, snapshot],
        )
        .unwrap();
    f.engine
        .store
        .db
        .execute("DELETE FROM graphs WHERE id=?1", [snapshot])
        .unwrap();
    let before = state(&f.engine);
    assert_static_error(
        impact_details::derive(
            &f.engine.store,
            f.id(),
            &DetailOptions::default(),
            &Deadline::new(30_000),
        )
        .unwrap_err(),
        "git_impact_details_input_invalid",
    );
    assert_eq!(state(&f.engine), before);
}

#[test]
fn caller_owned_read_transaction_survives_success_and_refusal() {
    let f = Fixture::moved();
    details(&f, &DetailOptions::default());
    let read = rusqlite::Transaction::new_unchecked(
        &f.engine.store.db,
        rusqlite::TransactionBehavior::Deferred,
    )
    .unwrap();
    let before = state(&f.engine);
    details(&f, &DetailOptions::default());
    assert!(!f.engine.store.db.is_autocommit());
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
    assert_eq!(state(&f.engine), before);
    read.rollback().unwrap();
    assert!(f.engine.store.db.is_autocommit());
}

#[test]
fn html_escapes_captured_source_markup_without_changing_structured_quote_bytes() {
    let f = Fixture::moved();
    let output = details(&f, &DetailOptions::default());
    let bytes = output.canonical_bytes().to_vec();
    assert!(all_quotes(output.value()).iter().any(|quote| {
        quote["text"]
            .as_str()
            .is_some_and(|text| text.contains("<tag>&"))
    }));
    let html = impact_details::render_html(&output, &Deadline::new(30_000)).unwrap();
    assert!(!html.contains("<tag>"));
    assert!(html.contains("&lt;tag&gt;&amp;"));
    assert_eq!(output.canonical_bytes(), bytes);
}
