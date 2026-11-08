use super::*;
use crate::{
    meaning_graph::{self, Binding, Fragment, MeaningInput, Source},
    model::{hash, identity},
};
use serde_json::json;

fn graph() -> Value {
    let workspace = hash("group-test-workspace");
    let text = "€ original source\r\n";
    let mut anchor = Binding {
        source_id: String::new(),
        path: "one.rs".into(),
        content_hash: hash(text),
        start_byte: 7,
        end_byte: 7 + text.len(),
    };
    anchor.source_id = meaning_graph::unit_id(&workspace, &anchor, text);
    let sources = vec![Source {
        anchor,
        text: text.into(),
        omitted_bytes: 0,
    }];
    let input = MeaningInput {
        workspace_id: workspace.clone(),
        unit_set_hash: identity(&serde_json::to_value(&sources).unwrap()),
        contract_hash: hash("group-contract"),
        sources,
    };
    let support =
        json!([{"sourceId":input.sources[0].anchor.source_id,"startByte":7,"endByte":10}]);
    let fragment: Fragment = serde_json::from_value(json!({
        "concepts":[
            {"key":"a","label":"Account","description":"Separate A inference","aliases":[],"support":support},
            {"key":"b","label":"Billing account","description":"Separate B inference","aliases":[],"support":support},
            {"key":"c","label":"Receipt","description":"Separate C inference","aliases":[],"support":support}
        ],
        "relations":[
            {"source":"a","target":"b","relation":"uses","description":"First directed observation","support":support},
            {"source":"a","target":"b","relation":"uses","description":"Parallel directed observation","support":support},
            {"source":"b","target":"a","relation":"returns","description":"Reverse observation","support":support}
        ],
        "hyperedges":[{"relation":"records","description":"Ordered roles","participants":[
            {"concept":"b","role":"actor"},{"concept":"a","role":"target"},{"concept":"c","role":"witness"}
        ],"support":support}],
        "coverage":[{"sourceId":input.sources[0].anchor.source_id,"outcome":"claims"}]
    })).unwrap();
    let meaning = meaning_graph::materialize(&input, &fragment, &Deadline::new(5000)).unwrap();
    json!({"schemaVersion":1,"graphId":hash("group-test-graph"),"workspaceId":workspace,
        "snapshotId":hash("group-test-snapshot"),"packets":[{"index":0,"meaning":meaning}]})
}

fn reference(g: &Value, packet_index: usize, label: &str) -> ConceptRef {
    let meaning = &g["packets"][packet_index]["meaning"];
    let id = meaning["concepts"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["label"] == label)
        .unwrap()["id"]
        .as_str()
        .unwrap();
    ConceptRef {
        packet_index,
        concept_id: id.into(),
    }
}

fn review(g: &Value) -> Review {
    let a = reference(g, 0, "Account");
    let b = reference(g, 0, "Billing account");
    Review {
        schema_version: 1,
        graph_id: g["graphId"].as_str().unwrap().into(),
        groups: vec![Group {
            members: vec![b, a.clone()],
            representative: a,
            rationale: "Host-selected grouping; € and Σ remain verbatim".into(),
        }],
    }
}

fn run(g: &Value, review: &Review, options: &Options) -> Result<Value> {
    report(g, review, options, &Deadline::new(5000))
}

#[test]
fn grouping_partitions_every_original_concept_without_changing_claims_or_evidence() {
    let g = graph();
    let before = g.clone();
    let r = run(&g, &review(&g), &Options::default()).unwrap();
    assert_eq!(r["kind"], "meaning_group_projection_v1");
    assert_eq!(r["nodes"].as_array().unwrap().len(), 2);
    let mut actual = BTreeMap::new();
    for node in r["nodes"].as_array().unwrap() {
        for member in node["members"].as_array().unwrap() {
            let key = serde_json::to_string(&member["reference"]).unwrap();
            assert!(actual.insert(key, &member["record"]).is_none());
        }
    }
    for original in g["packets"][0]["meaning"]["concepts"].as_array().unwrap() {
        let key =
            serde_json::to_string(&json!({"packetIndex":0,"conceptId":original["id"]})).unwrap();
        assert_eq!(actual[&key], original);
    }
    assert_eq!(actual.len(), 3);
    assert_eq!(
        r["evidence"][0]["record"],
        g["packets"][0]["meaning"]["evidence"][0]
    );
    assert_eq!(r["evidence"][0]["record"]["text"], "€");
    assert_eq!(r["groupingAuthority"], "explicit_host_assertion");
    assert_eq!(r["semanticIdentityEstablished"], false);
    assert_eq!(r["providerAttestation"], "unverified");
    assert_eq!(r["originalConceptMergesApplied"], 0);
    assert_eq!(r["modelCalls"], 0);
    assert_eq!(r["complete"], false);
    assert_eq!(g, before);
}

#[test]
fn grouping_keeps_parallel_directed_relations_as_original_observations_and_selfloops() {
    let g = graph();
    let r = run(&g, &review(&g), &Options::default()).unwrap();
    assert_eq!(r["relations"].as_array().unwrap().len(), 3);
    let originals = g["packets"][0]["meaning"]["relations"].as_array().unwrap();
    for row in r["relations"].as_array().unwrap() {
        assert_eq!(row["reference"]["packetIndex"], 0);
        assert!(originals.contains(&row["record"]));
        assert_eq!(row["reference"]["relationId"], row["record"]["id"]);
        assert_eq!(row["source"], row["target"]);
    }
    assert_eq!(
        r["relations"]
            .as_array()
            .unwrap()
            .iter()
            .map(|x| x["record"]["id"].as_str().unwrap())
            .collect::<BTreeSet<_>>()
            .len(),
        3
    );
}

#[test]
fn grouping_keeps_every_hyperedge_slot_role_position_and_original_reference() {
    let g = graph();
    let r = run(&g, &review(&g), &Options::default()).unwrap();
    let original = &g["packets"][0]["meaning"]["hyperedges"][0];
    let row = &r["hyperedges"][0];
    assert_eq!(row["record"], *original);
    assert_eq!(row["reference"]["packetIndex"], 0);
    assert_eq!(row["reference"]["hyperedgeId"], original["id"]);
    let slots = row["participants"].as_array().unwrap();
    assert_eq!(slots.len(), 3);
    for (position, slot) in slots.iter().enumerate() {
        assert_eq!(slot["position"], position);
        assert_eq!(slot["role"], original["participants"][position]["role"]);
        assert_eq!(slot["original"]["packetIndex"], 0);
        assert_eq!(
            slot["original"]["conceptId"],
            original["participants"][position]["concept"]
        );
    }
    assert_eq!(slots[0]["nodeId"], slots[1]["nodeId"]);
    assert_ne!(slots[0]["nodeId"], slots[2]["nodeId"]);
}

#[test]
fn grouping_packet_qualifies_colliding_concepts_relations_hyperedges_and_evidence() {
    let mut g = graph();
    let mut packet = g["packets"][0].clone();
    packet["index"] = json!(1);
    g["packets"].as_array_mut().unwrap().push(packet);
    let mut request = review(&g);
    request.groups[0].members[0] = reference(&g, 1, "Account");
    let r = run(&g, &request, &Options::default()).unwrap();
    assert_eq!(r["nodes"].as_array().unwrap().len(), 5);
    assert_eq!(r["relations"].as_array().unwrap().len(), 6);
    assert_eq!(r["hyperedges"].as_array().unwrap().len(), 2);
    assert_eq!(r["evidence"].as_array().unwrap().len(), 2);
    for kind in ["relations", "hyperedges", "evidence"] {
        let packets = r[kind]
            .as_array()
            .unwrap()
            .iter()
            .map(|row| {
                row.get("reference").unwrap_or(row)["packetIndex"]
                    .as_u64()
                    .unwrap()
            })
            .collect::<BTreeSet<_>>();
        assert_eq!(packets, BTreeSet::from([0, 1]));
    }
}

#[test]
fn grouping_refuses_duplicate_overlap_absent_refs_and_nonmember_representatives() {
    let g = graph();
    for case in 0..6 {
        let mut request = review(&g);
        match case {
            0 => {
                let duplicate = request.groups[0].members[0].clone();
                request.groups[0].members.push(duplicate);
            }
            1 => request.groups.push(request.groups[0].clone()),
            2 => request.groups[0].members[0].packet_index = 99,
            3 => request.groups[0].members[0].concept_id = hash("missing"),
            4 => request.groups[0].representative = reference(&g, 0, "Receipt"),
            _ => {
                request.groups[0].members.pop();
            }
        }
        assert!(
            run(&g, &request, &Options::default()).is_err(),
            "case {case}"
        );
    }
}

#[test]
fn grouping_canonicalizes_review_order_and_separates_policy_from_capacity() {
    let g = graph();
    let mut request = review(&g);
    let first = run(&g, &request, &Options::default()).unwrap();
    request.groups[0].members.reverse();
    let second = run(&g, &request, &Options::default()).unwrap();
    assert_eq!(first, second);
    let bigger = Options {
        max_concepts: Options::default().max_concepts + 1,
        ..Options::default()
    };
    let third = run(&g, &request, &bigger).unwrap();
    assert_eq!(first["projectionId"], third["projectionId"]);
    assert_eq!(first["policyHash"], third["policyHash"]);
    assert_eq!(first["nodes"], third["nodes"]);
    request.groups[0].rationale.push('!');
    let fourth = run(&g, &request, &Options::default()).unwrap();
    assert_ne!(first["policyHash"], fourth["policyHash"]);
    assert_ne!(first["projectionId"], fourth["projectionId"]);
    assert_eq!(first["nodes"], fourth["nodes"]);
}

#[test]
fn grouping_counts_original_records_and_slots_before_any_collapse() {
    let g = graph();
    let request = review(&g);
    for case in 0..8 {
        let mut options = Options::default();
        match case {
            0 => options.max_groups = 0,
            1 => options.max_members = 1,
            2 => options.max_concepts = 2,
            3 => options.max_relations = 2,
            4 => options.max_hyperedges = 0,
            5 => options.max_participants = 2,
            6 => options.max_evidence = 0,
            _ => options.max_output_bytes = 1023,
        }
        assert!(run(&g, &request, &options).is_err(), "case {case}");
    }
    let exact = Options {
        max_groups: 1,
        max_members: 2,
        max_concepts: 3,
        max_relations: 3,
        max_hyperedges: 1,
        max_participants: 3,
        max_evidence: 1,
        ..Options::default()
    };
    assert!(run(&g, &request, &exact).is_ok());
}

#[test]
fn grouping_zero_limits_admit_a_truly_empty_graph_and_review_only() {
    let mut g = graph();
    for field in ["concepts", "relations", "hyperedges", "evidence"] {
        g["packets"][0]["meaning"][field] = json!([]);
    }
    g["packets"][0]["meaning"]["coverage"][0]["outcome"] = json!("no_claims");
    let request = Review {
        schema_version: 1,
        graph_id: g["graphId"].as_str().unwrap().into(),
        groups: vec![],
    };
    let zero = Options {
        max_groups: 0,
        max_members: 0,
        max_concepts: 0,
        max_relations: 0,
        max_hyperedges: 0,
        max_participants: 0,
        max_evidence: 0,
        ..Options::default()
    };
    let r = run(&g, &request, &zero).unwrap();
    for name in ["nodes", "relations", "hyperedges", "evidence"] {
        assert!(r[name].as_array().unwrap().is_empty());
    }
    let nonempty = graph();
    let request = Review {
        schema_version: 1,
        graph_id: nonempty["graphId"].as_str().unwrap().into(),
        groups: vec![],
    };
    assert!(run(&nonempty, &request, &zero).is_err());
}

#[test]
fn grouping_refuses_invalid_version_graph_binding_and_rationale_without_echoing_values() {
    let g = graph();
    for case in 0..6 {
        let mut request = review(&g);
        match case {
            0 => request.schema_version = 2,
            1 => request.graph_id = hash("another-graph"),
            2 => request.groups[0].rationale = " \r\n ".into(),
            3 => request.groups[0].rationale = "secret-marker".repeat(300),
            4 => request.groups[0].rationale = "secret-marker\0".into(),
            _ => request.groups[0].members[0].concept_id = "secret-marker".into(),
        }
        let error = run(&g, &request, &Options::default()).unwrap_err();
        assert!(
            !format!("{error:#}").contains("secret-marker"),
            "case {case}"
        );
    }
}

#[test]
fn grouping_expired_deadline_refuses_before_projection() {
    let g = graph();
    assert!(report(&g, &review(&g), &Options::default(), &Deadline::new(0)).is_err());
}

#[test]
fn grouping_shared_host_cancellation_is_inherited_by_nested_deadlines() {
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };
    let g = graph();
    let host = Deadline::new(5000);
    let cancel = Arc::new(AtomicBool::new(false));
    let active = cancel.clone();
    let error = Deadline::run_bounded(&host, cancel, || {
        let nested = Deadline::new(5000);
        active.store(true, Ordering::SeqCst);
        let inner_error = report(&g, &review(&g), &Options::default(), &nested).unwrap_err();
        assert!(
            format!("{inner_error:#}").contains("cancelled"),
            "{inner_error:#}"
        );
        Err::<Value, _>(inner_error)
    })
    .unwrap_err();
    assert!(format!("{error:#}").contains("cancelled"), "{error:#}");
}

#[test]
fn grouping_two_disjoint_groups_have_canonical_order_independent_of_review_order() {
    let mut g = graph();
    let mut packet = g["packets"][0].clone();
    packet["index"] = json!(1);
    g["packets"].as_array_mut().unwrap().push(packet);
    let mut request = review(&g);
    let mut second = request.groups[0].clone();
    for reference in &mut second.members {
        reference.packet_index = 1;
    }
    second.representative.packet_index = 1;
    second.rationale = "Second explicit host assertion".into();
    request.groups.push(second);
    let original = run(&g, &request, &Options::default()).unwrap();
    request.groups.reverse();
    for group in &mut request.groups {
        group.members.reverse();
    }
    assert_eq!(run(&g, &request, &Options::default()).unwrap(), original);
}

#[test]
fn grouping_representative_changes_policy_and_presentation_but_not_node_membership_ids() {
    let g = graph();
    let mut request = review(&g);
    let original = run(&g, &request, &Options::default()).unwrap();
    request.groups[0].representative = reference(&g, 0, "Billing account");
    let changed = run(&g, &request, &Options::default()).unwrap();
    assert_ne!(original["policyHash"], changed["policyHash"]);
    assert_ne!(original["projectionId"], changed["projectionId"]);
    let nodes = |value: &Value| {
        value["nodes"]
            .as_array()
            .unwrap()
            .iter()
            .map(|node| node["id"].as_str().unwrap().to_owned())
            .collect::<Vec<_>>()
    };
    assert_eq!(nodes(&original), nodes(&changed));
    let chosen = changed["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|node| node["members"].as_array().unwrap().len() == 2)
        .unwrap();
    assert_eq!(
        chosen["representative"],
        serde_json::to_value(&request.groups[0].representative).unwrap()
    );
}

#[test]
fn grouping_tagged_tuple_identities_follow_the_declared_byte_recipes() {
    let g = graph();
    let mut request = review(&g);
    let value = run(&g, &request, &Options::default()).unwrap();
    request.groups[0].members.sort();
    fn reference_json(reference: &ConceptRef) -> String {
        format!(
            r#"{{"packetIndex":{},"conceptId":"{}"}}"#,
            reference.packet_index, reference.concept_id
        )
    }
    let members = request.groups[0]
        .members
        .iter()
        .map(reference_json)
        .collect::<Vec<_>>()
        .join(",");
    let representative = reference_json(&request.groups[0].representative);
    let rationale = serde_json::to_string(&request.groups[0].rationale).unwrap();
    let canonical_review = format!(
        r#"{{"schemaVersion":1,"graphId":"{}","groups":[{{"members":[{}],"representative":{},"rationale":{}}}]}}"#,
        request.graph_id, members, representative, rationale
    );
    let policy = hash(format!(
        r#"["meaning-group-policy-v1",{}]"#,
        canonical_review
    ));
    assert_eq!(value["policyHash"], policy);
    assert_eq!(
        value["projectionId"],
        hash(format!(
            r#"["meaning-group-projection-v1","{}","{}"]"#,
            request.graph_id, policy
        ))
    );
    let grouped = value["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|node| node["members"].as_array().unwrap().len() == 2)
        .unwrap();
    assert_eq!(
        grouped["id"],
        hash(format!(
            r#"["meaning-group-node-v1","{}",[{}]]"#,
            request.graph_id, members
        ))
    );
}

#[test]
fn grouping_output_capacity_is_exact_including_a_real_zero_limit() {
    let g = graph();
    let request = review(&g);
    let original = run(&g, &request, &Options::default()).unwrap();
    let bytes = serde_json::to_vec(&original).unwrap().len();
    let mut options = Options {
        max_output_bytes: bytes,
        ..Options::default()
    };
    assert_eq!(run(&g, &request, &options).unwrap(), original);
    options.max_output_bytes -= 1;
    assert!(run(&g, &request, &options).is_err());
    options.max_output_bytes = 0;
    let error = run(&g, &request, &options).unwrap_err();
    assert!(format!("{error:#}").contains("capacity"), "{error:#}");
}
