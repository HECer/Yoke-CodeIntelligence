use super::*;
use crate::{
    meaning_graph::{self, Binding, Fragment, MeaningInput, Source},
    model::{hash, identity},
};
use serde_json::json;

fn graph(labels: &[(&str, &[&str])]) -> Value {
    let workspace = hash("alias-test-workspace");
    let body = "prefix € source evidence\r\n";
    let mut sources = Vec::new();
    for path in ["one.rs", "two.rs"] {
        let mut anchor = Binding {
            source_id: String::new(),
            path: path.into(),
            content_hash: hash(format!("{path}{body}")),
            start_byte: 7,
            end_byte: 7 + body.len(),
        };
        anchor.source_id = meaning_graph::unit_id(&workspace, &anchor, body);
        sources.push(Source {
            anchor,
            text: body.into(),
            omitted_bytes: 0,
        });
    }
    let input = MeaningInput {
        workspace_id: workspace.clone(),
        unit_set_hash: identity(&serde_json::to_value(&sources).unwrap()),
        contract_hash: hash("alias-test-contract"),
        sources,
    };
    let support = |i: usize| json!([{"sourceId":input.sources[i % 2].anchor.source_id,"startByte":14,"endByte":17}]);
    let concepts = labels.iter().enumerate().map(|(i,(label,aliases))| json!({
        "key":format!("concept-{i}"),"label":label,"description":format!("Separate inference {i}"),"aliases":aliases,"support":support(i)
    })).collect::<Vec<_>>();
    let used = labels.len().min(2);
    let fragment: Fragment = serde_json::from_value(json!({"concepts":concepts,"relations":[],"hyperedges":[],
        "coverage":input.sources.iter().enumerate().map(|(i,s)|json!({"sourceId":s.anchor.source_id,"outcome":if i<used{"claims"}else{"no_claims"}})).collect::<Vec<_>>()
    })).unwrap();
    let meaning = meaning_graph::materialize(&input, &fragment, &Deadline::new(5000)).unwrap();
    json!({"schemaVersion":1,"workspaceId":workspace,"graphId":hash("alias-test-graph"),"snapshotId":hash("alias-test-snapshot"),
        "packets":[{"index":0,"meaning":meaning}]})
}
fn run(graph: &Value, options: &Options) -> Result<Value> {
    report(
        graph,
        graph["graphId"].as_str().unwrap(),
        options,
        &Deadline::new(5000),
    )
}

#[test]
fn normalized_aliases_propose_a_pair_without_merging_or_rewriting_source() {
    let g = graph(&[
        ("Billing Policy", &["Invoices"]),
        ("BILLING-policy", &["Receipts"]),
    ]);
    let before = g.clone();
    let r = run(&g, &Options::default()).unwrap();
    assert_eq!(r["candidates"].as_array().unwrap().len(), 1);
    assert_eq!(r["candidates"][0]["scoreBps"], 10_000);
    assert_eq!(r["candidates"][0]["basis"], "normalized_equal");
    assert_eq!(r["semanticIdentityEstablished"], false);
    assert_eq!(r["mergesApplied"], 0);
    assert_eq!(r["complete"], false);
    assert_eq!(r["concepts"].as_array().unwrap().len(), 2);
    assert_eq!(r["evidence"].as_array().unwrap().len(), 2);
    for evidence in r["evidence"].as_array().unwrap() {
        assert_eq!(evidence["record"]["startByte"], 14);
        assert_eq!(evidence["record"]["endByte"], 17);
        assert_eq!(evidence["record"]["text"], "€");
    }
    assert_ne!(
        r["evidence"][0]["record"]["path"],
        r["evidence"][1]["record"]["path"]
    );
    assert_eq!(g, before);
}

#[test]
fn explicit_alias_overlap_retains_which_provider_terms_matched() {
    let g = graph(&[
        ("Billing", &["Invoice service"]),
        ("Payments", &["INVOICE-service"]),
    ]);
    let r = run(&g, &Options::default()).unwrap();
    let candidate = &r["candidates"][0];
    assert_eq!(candidate["scoreBps"], 10000);
    assert_eq!(candidate["matchedTerms"]["leftKind"], "alias");
    assert_eq!(candidate["matchedTerms"]["rightKind"], "alias");
    let terms = [
        candidate["matchedTerms"]["left"].as_str().unwrap(),
        candidate["matchedTerms"]["right"].as_str().unwrap(),
    ];
    assert!(terms.contains(&"Invoice service"));
    assert!(terms.contains(&"INVOICE-service"));
}

#[test]
fn compatibility_normalization_preserves_accent_and_script_distinctions() {
    let options = Options {
        min_score_bps: 10000,
        ..Options::default()
    };
    for (a, b, want) in [
        ("Café", "Cafe\u{301}", 1),
        ("Ｂｉｌｌｉｎｇ", "billing", 1),
        ("Cafe", "Café", 0),
        ("PAY", "РАY", 0),
        ("ß", "ss", 0),
        ("का", "क", 0),
        ("İ", "i", 0),
        ("ΟΣ", "ος", 1),
    ] {
        let g = graph(&[(a, &[]), (b, &[])]);
        assert_eq!(
            run(&g, &options).unwrap()["candidates"]
                .as_array()
                .unwrap()
                .len(),
            want,
            "{a} {b}"
        );
    }
}

#[test]
fn trigram_similarity_is_separate_from_normalized_equality() {
    let g = graph(&[("abcdabcdabcd", &[]), ("bcdabcdabcda", &[])]);
    let r = run(&g, &Options::default()).unwrap();
    assert_eq!(r["candidates"][0]["basis"], "trigram_jaccard");
    assert_eq!(r["candidates"][0]["trigramJaccardBps"], 10000);
    assert_eq!(r["semanticIdentityEstablished"], false);
}

#[test]
fn transposed_letters_are_scored_without_becoming_identity_proof() {
    let g = graph(&[("MARTHA", &[]), ("MARHTA", &[])]);
    let r = run(&g, &Options::default()).unwrap();
    assert_eq!(r["candidates"][0]["basis"], "jaro_winkler");
    assert_eq!(r["candidates"][0]["scoreBps"], 9611);
    // Only "mar" is shared: one intersection over seven distinct trigrams.
    assert_eq!(r["candidates"][0]["trigramJaccardBps"], 1428);
    assert_eq!(r["semanticIdentityEstablished"], false);
    assert_eq!(
        run(
            &g,
            &Options {
                min_score_bps: 9612,
                ..Options::default()
            }
        )
        .unwrap()["candidates"],
        json!([])
    );
}

#[test]
fn short_unrelated_names_and_empty_normalizations_do_not_get_fuzzy_matches() {
    let g = graph(&[("cat", &[]), ("car", &[]), ("---", &[]), ("___", &[])]);
    let r = run(
        &g,
        &Options {
            min_score_bps: 7000,
            ..Options::default()
        },
    )
    .unwrap();
    assert_eq!(r["candidates"], json!([]));
    assert_eq!(r["statistics"]["unusableTerms"], 2);
}

#[test]
fn canonical_pair_order_is_independent_of_materialized_concept_order() {
    let mut g = graph(&[
        ("Billing", &["Ledger"]),
        ("BILLING", &["ledger"]),
        ("Ledger", &[]),
    ]);
    let first = run(&g, &Options::default()).unwrap();
    g["packets"][0]["meaning"]["concepts"]
        .as_array_mut()
        .unwrap()
        .reverse();
    g["packets"][0]["meaning"]["evidence"]
        .as_array_mut()
        .unwrap()
        .reverse();
    assert_eq!(run(&g, &Options::default()).unwrap(), first);
    assert_eq!(first["candidates"].as_array().unwrap().len(), 3);
}

#[test]
fn capacities_refuse_the_complete_operation_instead_of_returning_a_top_k() {
    let g = graph(&[("Policy", &[]), ("POLICY", &[]), ("policy", &[])]);
    for options in [
        Options {
            max_concepts: 2,
            ..Options::default()
        },
        Options {
            max_pairs: 2,
            ..Options::default()
        },
        Options {
            max_candidates: 2,
            ..Options::default()
        },
        Options {
            max_candidates: 0,
            ..Options::default()
        },
        Options {
            max_similarity_work: 0,
            ..Options::default()
        },
        Options {
            max_output_bytes: 1024,
            ..Options::default()
        },
    ] {
        assert!(run(&g, &options).is_err(), "{options:?}");
    }
    let empty = graph(&[]);
    let zero = Options {
        max_concepts: 0,
        max_pairs: 0,
        max_candidates: 0,
        max_similarity_work: 0,
        ..Options::default()
    };
    let r = run(&empty, &zero).unwrap();
    assert_eq!(r["candidates"], json!([]));
    assert_eq!(r["statistics"]["pairComparisons"], 0);
}

#[test]
fn invalid_host_limits_and_deadlines_are_refused() {
    for options in [
        Options {
            min_score_bps: 0,
            ..Options::default()
        },
        Options {
            min_score_bps: 10001,
            ..Options::default()
        },
        Options {
            max_concepts: usize::MAX,
            ..Options::default()
        },
        Options {
            max_pairs: usize::MAX,
            ..Options::default()
        },
        Options {
            max_candidates: usize::MAX,
            ..Options::default()
        },
        Options {
            max_similarity_work: u64::MAX,
            ..Options::default()
        },
        Options {
            max_output_bytes: 0,
            ..Options::default()
        },
    ] {
        assert!(validate_options(&options).is_err(), "{options:?}");
    }
    let g = graph(&[("one", &[]), ("ONE", &[])]);
    assert!(
        report(
            &g,
            g["graphId"].as_str().unwrap(),
            &Options::default(),
            &Deadline::new(0)
        )
        .is_err()
    );
}

#[test]
fn normalized_term_dedup_preserves_label_priority_and_exact_work_admission() {
    let g = graph(&[("Policy", &["POLICY", "ＰＯＬＩＣＹ"]), ("policy", &[])]);
    let options = Options {
        max_similarity_work: 36,
        ..Options::default()
    };
    let r = run(&g, &options).unwrap();
    assert_eq!(r["statistics"]["usableTerms"], 2);
    assert_eq!(r["statistics"]["similarityWork"], 36);
    assert_eq!(r["candidates"][0]["matchedTerms"]["leftKind"], "label");
    assert_eq!(r["candidates"][0]["matchedTerms"]["rightKind"], "label");
    assert!(
        run(
            &g,
            &Options {
                max_similarity_work: 35,
                ..options
            }
        )
        .is_err()
    );
}

#[test]
fn normalization_expansion_refuses_instead_of_truncating_a_label() {
    let expanded = "\u{fdfa}".repeat(30);
    let g = graph(&[(&expanded, &[]), ("Other concept", &[])]);
    assert!(
        run(&g, &Options::default())
            .unwrap_err()
            .to_string()
            .contains("normalization")
    );
}

#[test]
fn equal_native_ids_in_separate_packets_retain_qualified_proof() {
    let mut g = graph(&[("Policy", &[])]);
    let mut second = g["packets"][0].clone();
    second["index"] = json!(1);
    g["packets"].as_array_mut().unwrap().push(second);
    let r = run(&g, &Options::default()).unwrap();
    assert_eq!(r["candidates"].as_array().unwrap().len(), 1);
    assert_eq!(r["candidates"][0]["left"]["packetIndex"], 0);
    assert_eq!(r["candidates"][0]["right"]["packetIndex"], 1);
    assert_eq!(
        r["candidates"][0]["left"]["conceptId"],
        r["candidates"][0]["right"]["conceptId"]
    );
    assert_eq!(r["concepts"].as_array().unwrap().len(), 2);
    assert_eq!(r["evidence"].as_array().unwrap().len(), 2);
    assert_eq!(r["evidence"][0]["packetIndex"], 0);
    assert_eq!(r["evidence"][1]["packetIndex"], 1);
    assert_eq!(r["evidence"][0]["record"], r["evidence"][1]["record"]);
}
