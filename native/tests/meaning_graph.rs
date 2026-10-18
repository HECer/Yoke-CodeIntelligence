use serde_json::{Value, json};
use yoke::{
    capture::Deadline,
    meaning_graph::{self, Binding, Fragment, MeaningInput, Source},
    model::{hash, identity},
};

fn input() -> MeaningInput {
    let workspace = "a".repeat(64);
    let body = "pub fn invoice() { let currency = \"€\"; }\n";
    let file_hash = hash(body);
    let mut anchor = Binding {
        source_id: String::new(),
        path: "billing.rs".into(),
        content_hash: file_hash,
        start_byte: 0,
        end_byte: body.len(),
    };
    anchor.source_id = identity(&(
        "meaning-unit-v1",
        &workspace,
        &anchor.path,
        &anchor.content_hash,
        anchor.start_byte,
        anchor.end_byte,
        hash(body),
    ));
    let sources = vec![Source {
        anchor,
        text: body.into(),
        omitted_bytes: 0,
    }];
    MeaningInput {
        workspace_id: workspace,
        unit_set_hash: identity(&serde_json::to_value(&sources).unwrap()),
        contract_hash: "b".repeat(64),
        sources,
    }
}
fn fragment(input: &MeaningInput) -> Value {
    let support =
        json!([{"sourceId":input.sources[0].anchor.source_id,"startByte":7,"endByte":14}]);
    json!({"concepts":[
        {"key":"billing","label":"Billing","description":"Issues invoices.","aliases":["Invoices"],"support":support},
        {"key":"payment","label":"Payment","description":"Takes payment.","aliases":["Billing"],"support":support}],
        "relations":[{"source":"billing","target":"payment","relation":"depends_on","description":"An inferred dependency.","support":support}],
        "hyperedges":[{"relation":"workflow","participants":[{"role":"producer","concept":"billing"},{"role":"consumer","concept":"payment"},{"role":"audit","concept":"billing"}],"description":"An inferred workflow.","support":support}],
        "coverage":[{"sourceId":input.sources[0].anchor.source_id,"outcome":"claims"}]})
}

#[test]
fn model_concepts_relations_and_ordered_roles_keep_exact_source_and_inference_labels() {
    let input = input();
    let f: Fragment = serde_json::from_value(fragment(&input)).unwrap();
    let graph = meaning_graph::materialize(&input, &f, &Deadline::new(5000)).unwrap();
    assert_eq!(graph["resolution"], "inferred_concept");
    assert_eq!(graph["providerAttestation"], "unverified");
    assert_eq!(graph["concepts"].as_array().unwrap().len(), 2);
    assert_eq!(
        graph["hyperedges"][0]["participants"][0]["role"],
        "producer"
    );
    assert_eq!(graph["hyperedges"][0]["participants"][2]["role"], "audit");
    assert_eq!(graph["evidence"][0]["text"], "invoice");
    assert_eq!(
        graph["evidence"][0]["contentHash"],
        input.sources[0].anchor.content_hash
    );
}

#[test]
fn exact_claim_dedup_remaps_endpoints_but_aliases_do_not_merge_distinct_concepts() {
    let input = input();
    let mut raw = fragment(&input);
    let mut duplicate = raw["concepts"][0].clone();
    duplicate["key"] = json!("duplicate");
    raw["concepts"].as_array_mut().unwrap().push(duplicate);
    raw["relations"][0]["source"] = json!("duplicate");
    let f: Fragment = serde_json::from_value(raw).unwrap();
    let d = Deadline::new(5000);
    let normalized = meaning_graph::normalize(&input, &f, &d).unwrap();
    assert_eq!(normalized.concepts.len(), 2);
    assert!(
        normalized
            .concepts
            .iter()
            .any(|c| c.key == normalized.relations[0].source)
    );
    assert_eq!(
        meaning_graph::normalize(&input, &normalized, &d).unwrap(),
        normalized
    );
}

#[test]
fn utf8_support_escape_forged_source_identity_and_false_coverage_fail_closed() {
    let original = input();
    let mut euro = fragment(&original);
    let at = original.sources[0].text.find('€').unwrap();
    euro["concepts"][0]["support"][0]["startByte"] = json!(at + 1);
    euro["concepts"][0]["support"][0]["endByte"] = json!(at + 2);
    assert!(
        meaning_graph::normalize(
            &original,
            &serde_json::from_value(euro).unwrap(),
            &Deadline::new(5000)
        )
        .is_err()
    );
    let mut forged = original.clone();
    forged.sources[0].anchor.content_hash = "c".repeat(64);
    assert!(meaning_graph::validate_input(&forged, &Deadline::new(5000)).is_err());
    let mut false_coverage = fragment(&original);
    false_coverage["coverage"][0]["outcome"] = json!("no_claims");
    assert!(
        meaning_graph::normalize(
            &original,
            &serde_json::from_value(false_coverage).unwrap(),
            &Deadline::new(5000)
        )
        .is_err()
    );
    let mut bad_relation = fragment(&original);
    bad_relation["relations"][0]["target"] = json!("invented");
    assert!(
        meaning_graph::normalize(
            &original,
            &serde_json::from_value(bad_relation).unwrap(),
            &Deadline::new(5000)
        )
        .is_err()
    );
}

#[test]
fn empty_extraction_records_no_claims_without_claiming_semantic_completeness() {
    let input = input();
    let f:Fragment=serde_json::from_value(json!({"concepts":[],"relations":[],"hyperedges":[],"coverage":[{"sourceId":input.sources[0].anchor.source_id,"outcome":"no_claims"}]})).unwrap();
    let graph = meaning_graph::materialize(&input, &f, &Deadline::new(5000)).unwrap();
    assert_eq!(graph["concepts"], json!([]));
    assert_eq!(graph["complete"], false);
    assert_eq!(graph["coverage"][0]["outcome"], "no_claims");
}

#[test]
fn nonzero_excerpts_and_identical_quotes_in_distinct_files_keep_their_own_bindings() {
    let mut input = input();
    input.sources.clear();
    for path in ["one.rs", "two.rs"] {
        let mut anchor = Binding {
            source_id: String::new(),
            path: path.into(),
            content_hash: hash(format!("prefix invoice suffix {path}")),
            start_byte: 7,
            end_byte: 14,
        };
        anchor.source_id = identity(&(
            "meaning-unit-v1",
            &input.workspace_id,
            &anchor.path,
            &anchor.content_hash,
            7usize,
            14usize,
            hash("invoice"),
        ));
        input.sources.push(Source {
            anchor,
            text: "invoice".into(),
            omitted_bytes: 0,
        });
    }
    input.unit_set_hash = identity(&serde_json::to_value(&input.sources).unwrap());
    let mut raw = fragment(&input);
    raw["concepts"][1]["support"][0]["sourceId"] = json!(input.sources[1].anchor.source_id);
    raw["coverage"]
        .as_array_mut()
        .unwrap()
        .push(json!({"sourceId":input.sources[1].anchor.source_id,"outcome":"claims"}));
    let graph = meaning_graph::materialize(
        &input,
        &serde_json::from_value(raw).unwrap(),
        &Deadline::new(5000),
    )
    .unwrap();
    let evidence = graph["evidence"].as_array().unwrap();
    assert_eq!(evidence.len(), 2);
    for item in evidence {
        assert_eq!(item["text"], "invoice");
        assert_eq!(item["startByte"], 7);
        assert_eq!(item["endByte"], 14);
    }
    assert_ne!(evidence[0]["sourceId"], evidence[1]["sourceId"]);
    assert_eq!(graph["bindingValidation"], "submitted_bytes_only");
}

#[test]
fn ambiguous_keys_roles_coverage_and_capacity_are_not_accepted_as_inference() {
    let input = input();
    let cases = [
        "duplicate-key",
        "collapsed-hyperedge",
        "participant",
        "duplicate-participant",
        "missing-coverage",
        "duplicate-coverage",
        "foreign-coverage",
        "description",
        "aliases",
        "concept-count",
    ];
    for case in cases {
        let mut raw = fragment(&input);
        match case {
            "duplicate-key" => raw["concepts"][1]["key"] = raw["concepts"][0]["key"].clone(),
            "collapsed-hyperedge" => {
                let mut duplicate = raw["concepts"][0].clone();
                duplicate["key"] = json!("payment");
                raw["concepts"][1] = duplicate;
            }
            "participant" => raw["hyperedges"][0]["participants"][0]["concept"] = json!("absent"),
            "duplicate-participant" => {
                raw["hyperedges"][0]["participants"][1] =
                    raw["hyperedges"][0]["participants"][0].clone()
            }
            "missing-coverage" => raw["coverage"] = json!([]),
            "duplicate-coverage" => {
                let c = raw["coverage"][0].clone();
                raw["coverage"].as_array_mut().unwrap().push(c);
            }
            "foreign-coverage" => raw["coverage"][0]["sourceId"] = json!("c".repeat(64)),
            "description" => raw["concepts"][0]["description"] = json!("x".repeat(1001)),
            "aliases" => {
                raw["concepts"][0]["aliases"] = json!(["1", "2", "3", "4", "5", "6", "7", "8", "9"])
            }
            "concept-count" => {
                let c = raw["concepts"][0].clone();
                for i in 0..128 {
                    let mut c = c.clone();
                    c["key"] = json!(format!("extra-{i}"));
                    raw["concepts"].as_array_mut().unwrap().push(c);
                }
            }
            _ => unreachable!(),
        }
        assert!(
            meaning_graph::normalize(
                &input,
                &serde_json::from_value(raw).unwrap(),
                &Deadline::new(5000)
            )
            .is_err(),
            "{case}"
        );
    }
    assert!(
        meaning_graph::materialize(
            &input,
            &serde_json::from_value(fragment(&input)).unwrap(),
            &Deadline::new(0)
        )
        .is_err()
    );
}

#[test]
fn overlapping_support_cannot_expand_small_inputs_into_unbounded_quotation_artifacts() {
    let mut input = input();
    let body = "source words ".repeat(1000);
    let anchor = &mut input.sources[0].anchor;
    anchor.content_hash = hash(&body);
    anchor.end_byte = body.len();
    anchor.source_id = identity(&(
        "meaning-unit-v1",
        &input.workspace_id,
        &anchor.path,
        &anchor.content_hash,
        anchor.start_byte,
        anchor.end_byte,
        hash(&body),
    ));
    input.sources[0].text = body;
    input.unit_set_hash = identity(&serde_json::to_value(&input.sources).unwrap());
    for (count, width) in [(9usize, 2000usize), (33, 1)] {
        let concepts=(0..count).map(|i|json!({"key":format!("claim-{i}"),"label":format!("Concept {i}"),"description":"An inference.","aliases":[],"support":(0..8).map(|j|json!({"sourceId":input.sources[0].anchor.source_id,"startByte":i*8+j,"endByte":i*8+j+width})).collect::<Vec<_>>()})).collect::<Vec<_>>();
        let fragment:Fragment=serde_json::from_value(json!({"concepts":concepts,"relations":[],"hyperedges":[],"coverage":[{"sourceId":input.sources[0].anchor.source_id,"outcome":"claims"}]})).unwrap();
        assert!(
            meaning_graph::materialize(&input, &fragment, &Deadline::new(5000)).is_err(),
            "Accepted support expansion count={count} width={width}"
        );
    }
}
