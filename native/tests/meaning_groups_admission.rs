//! Public pure-admission contracts; no Store, worker or live source is opened.
use yoke::{
    capture::Deadline,
    meaning_groups::{self, ConceptRef, Group, Options, Review},
    model::hash,
};

fn reference(index: usize) -> ConceptRef {
    ConceptRef {
        packet_index: 0,
        concept_id: hash(format!("admission-concept-{index}")),
    }
}
fn request() -> Review {
    Review {
        schema_version: 1,
        graph_id: hash("admission-graph"),
        groups: vec![Group {
            members: vec![reference(0), reference(1)],
            representative: reference(0),
            rationale: "Explicit host assertion".into(),
        }],
    }
}

#[test]
fn meaning_group_admission_zero_output_is_a_valid_capacity() {
    let options = Options {
        max_output_bytes: 0,
        ..Options::default()
    };
    assert!(meaning_groups::validate_options(&options).is_ok());
    assert!(meaning_groups::validate_review(&request(), &options, &Deadline::new(5000)).is_ok());
}

#[test]
fn meaning_group_admission_oversized_blank_rationale_hits_byte_guard_before_trimming() {
    let mut review = request();
    review.groups[0].rationale = " ".repeat(2049);
    let error = meaning_groups::validate_review(&review, &Options::default(), &Deadline::new(5000))
        .unwrap_err();
    assert!(format!("{error:#}").contains("byte capacity"), "{error:#}");
}

#[test]
fn meaning_group_admission_rationale_limit_counts_exact_utf8_bytes() {
    let mut review = request();
    review.groups[0].rationale = "€".repeat(682) + "ab";
    assert_eq!(review.groups[0].rationale.len(), 2048);
    meaning_groups::validate_review(&review, &Options::default(), &Deadline::new(5000)).unwrap();
    review.groups[0].rationale.push('Σ');
    assert!(
        meaning_groups::validate_review(&review, &Options::default(), &Deadline::new(5000))
            .is_err()
    );
}

#[test]
fn meaning_group_admission_every_hard_limit_and_all_zero_capacities_are_checked() {
    for case in 0..8 {
        let mut options = Options::default();
        match case {
            0 => options.max_groups = 513,
            1 => options.max_members = 4097,
            2 => options.max_concepts = 4097,
            3 => options.max_relations = 32769,
            4 => options.max_hyperedges = 8193,
            5 => options.max_participants = 65537,
            6 => options.max_evidence = 65537,
            _ => options.max_output_bytes = 4_000_001,
        }
        assert!(
            meaning_groups::validate_options(&options).is_err(),
            "case {case}"
        );
    }
    let zero = Options {
        max_groups: 0,
        max_members: 0,
        max_concepts: 0,
        max_relations: 0,
        max_hyperedges: 0,
        max_participants: 0,
        max_evidence: 0,
        max_output_bytes: 0,
    };
    meaning_groups::validate_options(&zero).unwrap();
    let empty = Review {
        groups: vec![],
        ..request()
    };
    meaning_groups::validate_review(&empty, &zero, &Deadline::new(5000)).unwrap();
    assert!(meaning_groups::validate_review(&request(), &zero, &Deadline::new(5000)).is_err());
}

#[test]
fn meaning_group_admission_library_serialized_review_is_bounded_before_parent_loading() {
    let mut review = request();
    review.groups = (0..32)
        .map(|i| Group {
            members: vec![reference(i * 2), reference(i * 2 + 1)],
            representative: reference(i * 2),
            rationale: "€".repeat(682) + "ab",
        })
        .collect();
    meaning_groups::validate_review(&review, &Options::default(), &Deadline::new(5000)).unwrap();
    review.groups.extend((32..64).map(|i| Group {
        members: vec![reference(i * 2), reference(i * 2 + 1)],
        representative: reference(i * 2),
        rationale: "€".repeat(682) + "ab",
    }));
    let error = meaning_groups::validate_review(&review, &Options::default(), &Deadline::new(5000))
        .unwrap_err();
    assert!(format!("{error:#}").contains("byte capacity"), "{error:#}");
}

#[test]
fn meaning_group_admission_per_group_member_limit_is_independent_of_total_capacity() {
    let mut review = request();
    review.groups[0].members = (0..128).map(reference).collect();
    meaning_groups::validate_review(&review, &Options::default(), &Deadline::new(5000)).unwrap();
    review.groups[0].members.push(reference(128));
    assert!(
        meaning_groups::validate_review(&review, &Options::default(), &Deadline::new(5000))
            .is_err()
    );
}

#[test]
fn meaning_group_admission_nested_unknown_fields_and_duplicate_json_keys_refuse() {
    for case in 0..4 {
        let mut value = serde_json::to_value(request()).unwrap();
        match case {
            0 => value["groups"][0]["unexpected-secret-marker"] = true.into(),
            1 => value["groups"][0]["members"][0]["unexpected-secret-marker"] = true.into(),
            2 => value["groups"][0]["representative"]["unexpected-secret-marker"] = true.into(),
            _ => value["unexpected-secret-marker"] = true.into(),
        }
        let error = yoke::json::decode::<Review>(value).unwrap_err();
        assert!(!format!("{error:#}").contains("secret-marker"));
    }
    let raw = br#"{"groups":[{"members":[{"packetIndex":0,"packetIndex":1}]}]}"#;
    assert!(
        yoke::json::parse(raw, meaning_groups::MAX_REVIEW_BYTES)
            .unwrap_err()
            .to_string()
            .contains("Duplicate JSON key")
    );
}
