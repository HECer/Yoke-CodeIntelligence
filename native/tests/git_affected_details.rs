//! Public affected-detail behavior, provenance and deterministic boundary contracts.
//! Requires the separately drafted genuine fixture and the future opaque API.
use yoke as test_yoke;
#[path = "support/git_affected_fixture.rs"]
mod fixture;
use fixture::{Edit, Fixture, Oracle, PATH, expected_range, hierarchy, oracle, side, state};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};
use yoke::{
    capture::Deadline,
    git_workflows::{
        self,
        affected_details::{self, AffectedDetails, AffectedOptions},
        impact_details::{self, DetailOptions},
    },
    model::{ANALYZER, hash},
};

const INPUT: &str = "git_affected_details_input_invalid";
const CAPACITY: &str = "git_affected_details_capacity_exceeded";
const DEADLINE: &str = "git_affected_details_deadline_exceeded";

fn deadline() -> Deadline {
    Deadline::new(30_000)
}
fn derive(f: &Fixture, options: &AffectedOptions) -> AffectedDetails {
    affected_details::derive(&f.engine.store, f.id(), options, &deadline())
        .expect("qualified genuine affected result")
}
fn refusal(f: &Fixture, options: &AffectedOptions, marker: &str) {
    let before = state(&f.engine);
    let error =
        affected_details::derive(&f.engine.store, f.id(), options, &deadline()).unwrap_err();
    assert_eq!(error.to_string(), marker);
    f.engine.store.clear_deadline();
    assert_eq!(state(&f.engine), before);
}
fn rows<'a>(value: &'a Value, name: &str) -> &'a Vec<Value> {
    value[name].as_array().unwrap()
}
fn string(value: &Value) -> String {
    value.as_str().unwrap().to_owned()
}
fn strings(value: &Value) -> Vec<String> {
    value.as_array().unwrap().iter().map(string).collect()
}
fn keys(value: &Value, expected: &[&str]) {
    assert_eq!(
        value
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect::<BTreeSet<_>>(),
        expected.iter().copied().collect()
    );
}
fn assert_references_closed(v: &Value) {
    let entities: BTreeSet<_> = rows(v, "entities")
        .iter()
        .map(|r| (string(&r["side"]), string(&r["entityId"])))
        .collect();
    let evidence: BTreeSet<_> = rows(v, "evidence")
        .iter()
        .map(|r| (string(&r["side"]), string(&r["evidenceId"])))
        .collect();
    let observations: BTreeSet<_> = rows(v, "observations")
        .iter()
        .map(|r| (string(&r["side"]), string(&r["observationId"])))
        .collect();
    assert_eq!(entities.len(), rows(v, "entities").len());
    assert_eq!(evidence.len(), rows(v, "evidence").len());
    assert_eq!(observations.len(), rows(v, "observations").len());
    for r in rows(v, "observations") {
        let side = string(&r["side"]);
        assert!(entities.contains(&(side.clone(), string(&r["sourceEntityId"]))));
        if !r["targetEntityId"].is_null() {
            assert!(entities.contains(&(side.clone(), string(&r["targetEntityId"]))));
        }
        for id in strings(&r["evidenceIds"]) {
            assert!(evidence.contains(&(side.clone(), id)));
        }
    }
    for (states, proofs) in [
        ("hierarchyStates", "hierarchyProofRefs"),
        ("incomingStates", "incomingProofRefs"),
    ] {
        for s in rows(v, states) {
            let label = string(&s["side"]);
            assert!(entities.contains(&(label.clone(), string(&s["entityId"]))));
            for id in strings(&s["witnessObservationIds"]) {
                assert!(observations.contains(&(label.clone(), id)));
            }
        }
        for p in rows(v, proofs) {
            let s = &rows(v, states)[p["stateIndex"].as_u64().unwrap() as usize];
            assert!(observations.contains(&(string(&s["side"]), string(&p["observationId"]))));
        }
    }
    for reason in rows(v, "memberReasons") {
        let s =
            &rows(v, "hierarchyStates")[reason["hierarchyStateIndex"].as_u64().unwrap() as usize];
        assert_eq!(reason["side"], s["side"]);
        assert_eq!(reason["rootEntityId"], s["rootEntityId"]);
        assert_eq!(reason["direction"], s["direction"]);
        assert_eq!(reason["typeEntityId"], s["entityId"]);
    }
    for seed in rows(v, "walkSeeds") {
        for index in rows(seed, "memberReasonIndices") {
            let r = &rows(v, "memberReasons")[index.as_u64().unwrap() as usize];
            assert_eq!(seed["side"], r["side"]);
            assert_eq!(seed["rootEntityId"], r["rootEntityId"]);
            assert_eq!(seed["entityId"], r["memberEntityId"]);
            assert_eq!(r["eligibility"]["status"], "eligible");
        }
    }
}
fn count_rows(value: &Value) -> usize {
    match value {
        Value::Array(items) => items.len() + items.iter().map(count_rows).sum::<usize>(),
        Value::Object(fields) => fields.values().map(count_rows).sum(),
        _ => 0,
    }
}
fn sorted(value: &Value) -> Value {
    match value {
        Value::Array(items) => Value::Array(items.iter().map(sorted).collect()),
        Value::Object(fields) => {
            let ordered: BTreeMap<_, _> =
                fields.iter().map(|(k, v)| (k.clone(), sorted(v))).collect();
            Value::Object(ordered.into_iter().collect())
        }
        _ => value.clone(),
    }
}
// Independent documented wire/identity oracle. It calls no affected serializer,
// expander, budget helper, sorter or public result constructor.
fn sealed(mut value: Value) -> Vec<u8> {
    value["resultId"] = json!("0".repeat(64));
    value["wireBytes"] = json!(0);
    let mut settled = false;
    for _ in 0..32 {
        let length = serde_json::to_vec(&sorted(&value)).unwrap().len();
        if value["wireBytes"].as_u64() == Some(length as u64) {
            settled = true;
            break;
        }
        value["wireBytes"] = json!(length);
    }
    assert!(settled);
    let mut identity_free = value.clone();
    identity_free.as_object_mut().unwrap().remove("resultId");
    let mut identity_bytes = b"yoke_git_affected_details_1\0".to_vec();
    identity_bytes.extend(serde_json::to_vec(&sorted(&identity_free)).unwrap());
    value["resultId"] = json!(hash(identity_bytes));
    let bytes = serde_json::to_vec(&sorted(&value)).unwrap();
    assert_eq!(value["wireBytes"].as_u64(), Some(bytes.len() as u64));
    bytes
}
fn root_rows<'a>(value: &'a Value, table: &str, label: &str, root: &str) -> Vec<&'a Value> {
    rows(value, table)
        .iter()
        .filter(|r| r["side"] == label && r["rootEntityId"] == root)
        .collect()
}
fn hierarchy_edge(o: &Oracle, source: &str, target: &str) -> String {
    let found: Vec<_> = o
        .hierarchy
        .iter()
        .filter(|e| e.source == source && e.target == Some(target))
        .collect();
    assert_eq!(found.len(), 1, "qualified literal relation tuple");
    found[0].id.clone()
}
fn members(value: &Value, label: &str, root: &str) -> BTreeSet<(String, String)> {
    root_rows(value, "memberReasons", label, root)
        .into_iter()
        .map(|r| {
            assert_eq!(r["eligibility"], json!({"status":"eligible","reason":null}));
            (string(&r["direction"]), string(&r["memberEntityId"]))
        })
        .collect()
}
fn assert_original_refs(f: &Fixture, value: &Value) {
    let mut actual = Vec::new();
    for seed in rows(value, "originalSeeds") {
        for index in rows(seed, "changedRowIndices") {
            actual.push((
                index.as_u64().unwrap() as usize,
                string(&seed["side"]),
                string(&seed["entityId"]),
            ));
        }
    }
    actual.sort();
    let expected: Vec<_> = rows(&f.report, "changedSymbols")
        .iter()
        .enumerate()
        .map(|(i, r)| (i, string(&r["side"]), string(&r["id"])))
        .collect();
    assert_eq!(actual, expected);
    for (report_field, output_field, id_field) in [
        ("impact", "originalImpactRefs", "entityId"),
        (
            "impactObservations",
            "originalImpactObservationRefs",
            "observationId",
        ),
    ] {
        let expected: Vec<_> = rows(&f.report, report_field)
            .iter()
            .enumerate()
            .map(|(i, r)| {
                let mut row = json!({"originalRowIndex":i,"side":r["side"]});
                row[id_field] = r["id"].clone();
                row
            })
            .collect();
        assert_eq!(rows(value, output_field), &expected);
    }
}
fn compatibility(f: &Fixture) -> Value {
    let loaded = git_workflows::load(&f.engine.store, f.id(), &deadline()).unwrap();
    let rendered = git_workflows::render(&f.engine.store, &f.report, "json", &deadline()).unwrap();
    let mut digests = BTreeMap::new();
    for kind in ["report", "history", "file"] {
        digests.insert(
            kind,
            git_workflows::digest(
                &f.engine.store,
                &f.report,
                kind,
                if kind == "file" { Some(PATH) } else { None },
                &deadline(),
            )
            .unwrap(),
        );
    }
    let impact = impact_details::derive(
        &f.engine.store,
        f.id(),
        &DetailOptions::default(),
        &deadline(),
    )
    .unwrap();
    json!({"load":loaded,"render":rendered,"digests":digests,"r5aBytes":impact.canonical_bytes()})
}

#[test]
fn default_options_are_closed_thirteen_fields_and_direct_callers_cannot_raise_maxima() {
    let f = hierarchy(Edit::Derived, false);
    let good = derive(&f, &AffectedOptions::default());
    assert_eq!(good.value()["policy"], "captured_hierarchy_v1");
    let expected = json!({"policy":"captured_hierarchy_v1","maxGraphEntities":5000,"maxGraphObservations":50000,
        "maxGraphEvidence":100000,"maxOriginalSeeds":2000,"maxExpandedSeeds":4000,"maxHierarchyDepth":16,
        "maxIncomingDepth":8,"maxStates":20000,"maxEdgeVisits":200000,"maxRows":20000,
        "maxSourceBytes":64000000,"maxOutputBytes":4000000});
    assert_eq!(
        serde_json::to_value(AffectedOptions::default()).unwrap(),
        expected
    );
    assert_eq!(expected.as_object().unwrap().len(), 13);
    let valid = serde_json::to_string(&expected).unwrap();
    for key in expected.as_object().unwrap().keys() {
        let mut missing = expected.clone();
        missing.as_object_mut().unwrap().remove(key);
        assert!(serde_json::from_str::<AffectedOptions>(&missing.to_string()).is_err());
        if key != "policy" {
            let mut excessive = expected.clone();
            excessive[key] = json!(expected[key].as_u64().unwrap() + 1);
            refusal(&f, &serde_json::from_value(excessive).unwrap(), INPUT);
        }
    }
    let duplicate = valid.replacen(
        "\"maxStates\":20000",
        "\"maxStates\":20000,\"maxStates\":1",
        1,
    );
    assert_ne!(valid, duplicate);
    for malformed in [
        duplicate,
        valid.replacen("\"maxStates\":20000", "\"maxStates\":-1", 1),
        valid.replacen("\"maxStates\":20000", "\"maxStates\":1.5", 1),
        valid.replacen(
            "\"maxStates\":20000",
            "\"maxStates\":184467440737095516160",
            1,
        ),
    ] {
        assert!(serde_json::from_str::<AffectedOptions>(&malformed).is_err());
    }
    let mut unknown = expected.clone();
    unknown["extra"] = json!(true);
    assert!(serde_json::from_value::<AffectedOptions>(unknown).is_err());
    refusal(
        &f,
        &AffectedOptions {
            policy: "other".into(),
            ..AffectedOptions::default()
        },
        INPUT,
    );
    refusal(
        &f,
        &AffectedOptions {
            max_states: usize::MAX,
            ..AffectedOptions::default()
        },
        INPUT,
    );
}

#[test]
fn derived_root_has_exact_ancestor_members_full_witnesses_and_no_sibling_shortcut() {
    let f = hierarchy(Edit::Derived, false);
    let result = derive(&f, &AffectedOptions::default());
    let v = result.value();
    assert_original_refs(&f, v);
    for label in ["base", "head"] {
        let o = oracle(&f, label);
        let root = o.id("Derived.save");
        let first = hierarchy_edge(&o, "Derived", "Base");
        let second = hierarchy_edge(&o, "Base", "Service");
        assert_eq!(
            members(v, label, root),
            [
                ("ancestor".into(), o.id("Base.save").into()),
                ("ancestor".into(), o.id("Service.save").into())
            ]
            .into()
        );
        let mut actual: Vec<_> = root_rows(v, "hierarchyStates", label, root)
            .into_iter()
            .map(|s| {
                (
                    string(&s["direction"]),
                    s["distance"].as_u64().unwrap(),
                    string(&s["entityId"]),
                    strings(&s["witnessObservationIds"]),
                )
            })
            .collect();
        actual.sort();
        let mut expected: Vec<(String, u64, String, Vec<String>)> = vec![
            ("ancestor".into(), 0, o.id("Derived").into(), vec![]),
            (
                "ancestor".into(),
                1,
                o.id("Base").into(),
                vec![first.clone()],
            ),
            (
                "ancestor".into(),
                2,
                o.id("Service").into(),
                vec![first, second],
            ),
            ("descendant".into(), 0, o.id("Derived").into(), vec![]),
        ];
        expected.sort();
        assert_eq!(actual, expected);
        let mut actual_proof = Vec::new();
        for p in rows(v, "hierarchyProofRefs") {
            let s = &rows(v, "hierarchyStates")[p["stateIndex"].as_u64().unwrap() as usize];
            if s["side"] == label && s["rootEntityId"] == root {
                actual_proof.push((
                    string(&s["direction"]),
                    string(&s["entityId"]),
                    string(&p["observationId"]),
                ));
            }
        }
        let mut expected_proof = vec![
            (
                "ancestor".to_owned(),
                o.id("Derived").to_owned(),
                hierarchy_edge(&o, "Derived", "Base"),
            ),
            (
                "ancestor".to_owned(),
                o.id("Base").to_owned(),
                hierarchy_edge(&o, "Base", "Service"),
            ),
        ];
        actual_proof.sort();
        expected_proof.sort();
        assert_eq!(actual_proof, expected_proof);
        let actual_walks: BTreeSet<_> = root_rows(v, "walkSeeds", label, root)
            .into_iter()
            .map(|w| string(&w["entityId"]))
            .collect();
        assert_eq!(
            actual_walks,
            [
                root.into(),
                o.id("Base.save").into(),
                o.id("Service.save").into()
            ]
            .into()
        );
        assert!(o.derived_excluded_member_ids().is_disjoint(&actual_walks));
        let class = rows(v, "originalSeeds")
            .iter()
            .find(|s| s["side"] == label && s["entityId"] == o.id("Derived"))
            .unwrap();
        assert_eq!(
            class["eligibility"],
            json!({"status":"unsupported","reason":"unsupported_entity_kind","ownerEntityId":null})
        );
    }
}

#[test]
fn base_root_keeps_both_descendant_candidates_and_contract_ancestor() {
    let f = hierarchy(Edit::Base, false);
    let result = derive(&f, &AffectedOptions::default());
    for label in ["base", "head"] {
        let o = oracle(&f, label);
        let root = o.id("Base.save");
        let expected = [
            ("ancestor".into(), o.id("Service.save").into()),
            ("descendant".into(), o.id("Derived.save").into()),
            ("descendant".into(), o.id("Sibling.save").into()),
        ]
        .into();
        assert_eq!(members(result.value(), label, root), expected);
        assert!(
            !root_rows(result.value(), "memberReasons", label, root)
                .iter()
                .any(|r| r["memberEntityId"] == root
                    || r["memberEntityId"] == o.id("Unrelated.save"))
        );
        for r in root_rows(result.value(), "memberReasons", label, root) {
            let s = &rows(result.value(), "hierarchyStates")
                [r["hierarchyStateIndex"].as_u64().unwrap() as usize];
            assert_eq!(s["distance"], 1);
            assert_eq!(s["side"], label);
            assert_eq!(s["rootEntityId"], root);
        }
    }
}

#[test]
fn root_and_walk_seed_states_retain_parallel_calls_and_independent_incoming_distances() {
    let f = hierarchy(Edit::Derived, false);
    let result = derive(&f, &AffectedOptions::default());
    let v = result.value();
    for label in ["base", "head"] {
        let o = oracle(&f, label);
        let root = o.id("Derived.save");
        let mut expected_states: Vec<(String, u64, String, Vec<String>)> = Vec::new();
        let mut expected_proof = Vec::new();
        for (seed, caller) in [
            ("Derived.save", "viaDerived"),
            ("Base.save", "viaBase"),
            ("Service.save", "viaContract"),
        ] {
            let calls = o.calls_to(seed);
            assert_eq!(calls.len(), 2);
            let first = calls.iter().map(|e| e.id.clone()).min().unwrap();
            expected_states.push((o.id(seed).to_owned(), 0, o.id(seed).to_owned(), vec![]));
            expected_states.push((
                o.id(seed).to_owned(),
                1,
                o.id(caller).to_owned(),
                vec![first.clone()],
            ));
            expected_proof.extend(
                calls
                    .iter()
                    .map(|e| (o.id(seed).to_owned(), o.id(seed).to_owned(), e.id.clone())),
            );
            if seed == "Base.save" {
                let indirect = o.calls_to("viaBase");
                assert_eq!(indirect.len(), 1);
                expected_states.push((
                    o.id(seed).to_owned(),
                    2,
                    o.id("indirectly").to_owned(),
                    vec![first, indirect[0].id.clone()],
                ));
                expected_proof.push((
                    o.id(seed).to_owned(),
                    o.id("viaBase").to_owned(),
                    indirect[0].id.clone(),
                ));
            }
        }
        let mut actual_states: Vec<_> = root_rows(v, "incomingStates", label, root)
            .into_iter()
            .map(|s| {
                (
                    string(&s["walkSeedEntityId"]),
                    s["distance"].as_u64().unwrap(),
                    string(&s["entityId"]),
                    strings(&s["witnessObservationIds"]),
                )
            })
            .collect();
        let mut actual_proof = Vec::new();
        for proof in rows(v, "incomingProofRefs") {
            let s = &rows(v, "incomingStates")[proof["stateIndex"].as_u64().unwrap() as usize];
            if s["side"] == label && s["rootEntityId"] == root {
                actual_proof.push((
                    string(&s["walkSeedEntityId"]),
                    string(&s["entityId"]),
                    string(&proof["observationId"]),
                ));
            }
        }
        expected_states.sort();
        actual_states.sort();
        expected_proof.sort();
        actual_proof.sort();
        assert_eq!(actual_states, expected_states);
        assert_eq!(actual_proof, expected_proof);
    }
}

#[test]
fn unicode_crlf_tables_bind_exact_historical_entities_observations_and_evidence_vectors() {
    let f = hierarchy(Edit::Derived, true);
    let result = derive(&f, &AffectedOptions::default());
    let v = result.value();
    assert_references_closed(v);
    for label in ["base", "head"] {
        let o = oracle(&f, label);
        let (_, g) = side(&f, label);
        assert!(
            o.text.contains('\r') && o.text.contains('\u{1f30d}') && o.text.contains("e\u{301}")
        );
        for r in rows(v, "entities").iter().filter(|r| r["side"] == label) {
            keys(
                r,
                &[
                    "side",
                    "entityId",
                    "snapshotId",
                    "path",
                    "contentHash",
                    "name",
                    "kind",
                    "language",
                    "producer",
                    "parentKey",
                    "range",
                    "nameRange",
                ],
            );
            let candidates: Vec<_> = g
                .entities
                .iter()
                .filter(|e| r["entityId"] == e.id)
                .collect();
            assert_eq!(candidates.len(), 1);
            let e = candidates[0];
            assert_eq!(r["snapshotId"], o.snapshot_id);
            assert_eq!(r["path"], PATH);
            assert_eq!(r["contentHash"], hash(&o.text));
            assert_eq!(
                r["range"],
                expected_range(&o.text, e.start_byte, e.end_byte)
            );
            assert_eq!(r["name"], e.name);
            assert_eq!(r["kind"], e.kind);
            assert_eq!(r["language"], e.language);
            assert_eq!(r["producer"], e.producer);
            assert_eq!(r["parentKey"], json!(e.parent_key));
            if let (Some(a), Some(b)) = (e.name_start_byte, e.name_end_byte) {
                assert_eq!(r["nameRange"], expected_range(&o.text, a, b));
                assert_eq!(&o.text[a..b], e.name);
            } else {
                assert!(r["nameRange"].is_null());
            }
        }
        for r in rows(v, "observations")
            .iter()
            .filter(|r| r["side"] == label)
        {
            keys(
                r,
                &[
                    "side",
                    "observationId",
                    "snapshotId",
                    "sourceEntityId",
                    "targetEntityId",
                    "targetLabel",
                    "relation",
                    "resolution",
                    "producer",
                    "evidenceIds",
                    "relationClass",
                ],
            );
            let e = g
                .observations
                .iter()
                .find(|e| r["observationId"] == e.id)
                .unwrap();
            assert_eq!(r["snapshotId"], o.snapshot_id);
            assert_eq!(r["sourceEntityId"], e.source);
            assert_eq!(r["targetEntityId"], json!(e.target));
            assert_eq!(r["relation"], e.relation);
            assert_eq!(r["resolution"], e.resolution);
            assert_eq!(r["targetLabel"], e.target_label);
            assert_eq!(r["evidenceIds"], json!(e.evidence_ids));
            assert_eq!(r["producer"], e.producer);
            assert_eq!(
                r["relationClass"],
                if e.relation == "implements" {
                    "contract_membership"
                } else if ["extends", "inherits"].contains(&e.relation.as_str()) {
                    "structural_inheritance"
                } else {
                    "incoming_candidate"
                }
            );
        }
        for r in rows(v, "evidence").iter().filter(|r| r["side"] == label) {
            keys(
                r,
                &[
                    "side",
                    "evidenceId",
                    "snapshotId",
                    "path",
                    "contentHash",
                    "range",
                    "anchor",
                    "trust",
                    "producer",
                ],
            );
            let e = g.evidence.iter().find(|e| r["evidenceId"] == e.id).unwrap();
            assert_eq!(r["snapshotId"], o.snapshot_id);
            assert_eq!(r["path"], PATH);
            assert_eq!(r["contentHash"], hash(&o.text));
            assert_eq!(
                r["range"],
                expected_range(&o.text, e.start_byte, e.end_byte)
            );
            assert!(o.text.is_char_boundary(e.start_byte) && o.text.is_char_boundary(e.end_byte));
            assert_eq!(r["anchor"], e.anchor);
            assert_eq!(r["trust"], e.trust);
            assert_eq!(r["producer"], e.producer);
        }
    }
    fn no_source_copies(value: &Value) {
        match value {
            Value::Object(m) => {
                for (k, v) in m {
                    assert!(!["text", "signature", "qualifiedName"].contains(&k.as_str()));
                    no_source_copies(v)
                }
            }
            Value::Array(a) => {
                for v in a {
                    no_source_copies(v)
                }
            }
            _ => {}
        }
    }
    no_source_copies(v);
}

#[test]
fn historical_replay_needs_no_live_git_and_preserves_report_r5a_and_store_state() {
    let f = hierarchy(Edit::Derived, false);
    let before_result = derive(&f, &AffectedOptions::default());
    let old = compatibility(&f);
    fs::remove_dir_all(f.dir.path().join("src")).unwrap();
    fs::rename(f.dir.path().join(".git"), f.dir.path().join("git-withheld")).unwrap();
    let before = state(&f.engine);
    let after = derive(&f, &AffectedOptions::default());
    assert_eq!(after.canonical_bytes(), before_result.canonical_bytes());
    assert_eq!(compatibility(&f), old);
    assert_eq!(state(&f.engine), before);
    assert_eq!(
        fs::read(f.dir.path().join("git-withheld/index")).unwrap(),
        f.index_bytes
    );
    assert!(!f.dir.path().join("src").exists() && !f.dir.path().join(".git").exists());
}

#[test]
fn no_change_still_charges_every_index_observation_and_allows_true_zero_derived_work() {
    let f = hierarchy(Edit::None, false);
    assert!(rows(&f.report, "changedSymbols").is_empty());
    let observations = ["base", "head"]
        .into_iter()
        .map(|s| side(&f, s).1.observations.len())
        .sum::<usize>();
    assert!(observations > 0);
    let options = AffectedOptions {
        max_original_seeds: 0,
        max_expanded_seeds: 0,
        max_states: 0,
        max_source_bytes: 0,
        max_hierarchy_depth: 0,
        max_incoming_depth: 0,
        max_edge_visits: observations,
        ..AffectedOptions::default()
    };
    let result = derive(&f, &options);
    let v = result.value();
    for table in [
        "originalSeeds",
        "expandedSeeds",
        "hierarchyStates",
        "hierarchyProofRefs",
        "memberReasons",
        "walkSeeds",
        "incomingStates",
        "incomingProofRefs",
        "entities",
        "observations",
        "evidence",
    ] {
        assert!(rows(v, table).is_empty());
    }
    assert_eq!(v["consumed"]["indexObservationScans"], observations);
    assert_eq!(v["consumed"]["edgeVisits"], observations);
    assert_eq!(v["consumed"]["newEvidenceSourceBytes"], 0);
    assert_eq!(v["consumed"]["states"], 0);
    refusal(
        &f,
        &AffectedOptions {
            max_edge_visits: observations - 1,
            ..options
        },
        CAPACITY,
    );
}

#[test]
fn exact_graph_original_row_and_seed_state_budgets_succeed_one_less_refuses() {
    let f = hierarchy(Edit::Derived, false);
    let b = side(&f, "base").1;
    let h = side(&f, "head").1;
    let originals = rows(&f.report, "changedSymbols").len();
    let roots: BTreeSet<_> = rows(&f.report, "changedSymbols")
        .iter()
        .map(|r| (string(&r["side"]), string(&r["id"])))
        .collect();
    let options = AffectedOptions {
        max_graph_entities: b.entities.len().max(h.entities.len()),
        max_graph_observations: b.observations.len().max(h.observations.len()),
        max_graph_evidence: b.evidence.len().max(h.evidence.len()),
        max_original_seeds: originals,
        max_expanded_seeds: 0,
        max_states: roots.len(),
        max_hierarchy_depth: 0,
        max_incoming_depth: 0,
        ..AffectedOptions::default()
    };
    let result = derive(&f, &options);
    assert_eq!(result.value()["consumed"]["originalSeedRows"], originals);
    assert_eq!(result.value()["consumed"]["states"], roots.len());
    assert_eq!(rows(result.value(), "incomingStates").len(), roots.len());
    assert!(rows(result.value(), "hierarchyStates").is_empty());
    let serialized = serde_json::to_value(&options).unwrap();
    for key in [
        "maxGraphEntities",
        "maxGraphObservations",
        "maxGraphEvidence",
        "maxOriginalSeeds",
        "maxStates",
    ] {
        let mut lower = serialized.clone();
        let n = lower[key].as_u64().unwrap();
        assert!(n > 0);
        lower[key] = json!(n - 1);
        refusal(&f, &serde_json::from_value(lower).unwrap(), CAPACITY);
    }
}

#[test]
fn selected_depths_have_explicit_boundaries_and_capacity_never_drops_member_candidates() {
    let f = hierarchy(Edit::Derived, false);
    let options = AffectedOptions {
        max_hierarchy_depth: 1,
        max_incoming_depth: 0,
        ..AffectedOptions::default()
    };
    let result = derive(&f, &options);
    for label in ["base", "head"] {
        let o = oracle(&f, label);
        let root = o.id("Derived.save");
        assert_eq!(
            members(result.value(), label, root),
            [("ancestor".into(), o.id("Base.save").into())].into()
        );
        let boundary = root_rows(result.value(), "hierarchyStates", label, root)
            .into_iter()
            .find(|s| s["direction"] == "ancestor" && s["entityId"] == o.id("Base"))
            .unwrap();
        assert_eq!(boundary["boundary"], true);
        assert_eq!(boundary["unexaminedAdjacencyCount"], 1);
        assert!(
            root_rows(result.value(), "incomingStates", label, root)
                .iter()
                .all(|s| s["distance"] == 0 && s["boundary"] == true)
        );
    }
    assert!(rows(result.value(), "incomingProofRefs").is_empty());
    let exact = AffectedOptions {
        max_expanded_seeds: 4,
        ..AffectedOptions::default()
    };
    let all = derive(&f, &exact);
    assert_eq!(rows(all.value(), "expandedSeeds").len(), 4);
    assert_eq!(all.value()["consumed"]["uniqueExpandedSeeds"], 4);
    refusal(
        &f,
        &AffectedOptions {
            max_expanded_seeds: 3,
            ..exact
        },
        CAPACITY,
    );
}

#[test]
fn recursive_rows_new_source_bytes_and_wire_boundaries_are_independently_measured() {
    let f = hierarchy(Edit::Derived, true);
    let baseline = derive(&f, &AffectedOptions::default());
    let value = baseline.value();
    let mut index_scans = 0;
    let mut hierarchy_unresolved = 0;
    let mut incoming_unresolved = 0;
    for label in ["base", "head"] {
        let graph = side(&f, label).1;
        assert_eq!(
            value["consumed"]["graphs"][label],
            json!({"entities":graph.entities.len(),"observations":graph.observations.len(),"evidence":graph.evidence.len()})
        );
        index_scans += graph.observations.len();
        hierarchy_unresolved += graph
            .observations
            .iter()
            .filter(|e| {
                e.target.is_none()
                    && ["extends", "implements", "inherits"].contains(&e.relation.as_str())
            })
            .count();
        incoming_unresolved += graph
            .observations
            .iter()
            .filter(|e| {
                e.target.is_none() && ["calls", "references"].contains(&e.relation.as_str())
            })
            .count();
    }
    assert_eq!(value["consumed"]["indexObservationScans"], index_scans);
    assert_eq!(
        value["consumed"]["hierarchyStates"],
        rows(value, "hierarchyStates").len()
    );
    assert_eq!(
        value["consumed"]["incomingStates"],
        rows(value, "incomingStates").len()
    );
    assert_eq!(
        value["consumed"]["states"],
        rows(value, "hierarchyStates").len() + rows(value, "incomingStates").len()
    );
    assert_eq!(
        value["consumed"]["hierarchyAdjacencyVisits"],
        rows(value, "hierarchyProofRefs").len()
    );
    assert_eq!(
        value["consumed"]["incomingAdjacencyVisits"],
        rows(value, "incomingProofRefs").len()
    );
    assert_eq!(
        value["consumed"]["edgeVisits"],
        index_scans
            + rows(value, "hierarchyProofRefs").len()
            + rows(value, "incomingProofRefs").len()
    );
    assert_eq!(
        value["coverage"]["hierarchy"]["unresolvedObservations"],
        hierarchy_unresolved
    );
    assert_eq!(
        value["coverage"]["incoming"]["unresolvedObservations"],
        incoming_unresolved
    );
    let count = count_rows(value);
    assert_eq!(value["rowUnits"], count);
    assert!(count > 0);
    derive(
        &f,
        &AffectedOptions {
            max_rows: count,
            ..AffectedOptions::default()
        },
    );
    refusal(
        &f,
        &AffectedOptions {
            max_rows: count - 1,
            ..AffectedOptions::default()
        },
        CAPACITY,
    );
    let hashes: BTreeMap<_, _> = ["base", "head"]
        .into_iter()
        .map(|s| {
            let t = f.text(s, PATH);
            (hash(t), t.len())
        })
        .collect();
    let source_bytes = hashes.values().sum::<usize>();
    assert_eq!(value["consumed"]["newEvidenceSourceBytes"], source_bytes);
    derive(
        &f,
        &AffectedOptions {
            max_source_bytes: source_bytes,
            ..AffectedOptions::default()
        },
    );
    refusal(
        &f,
        &AffectedOptions {
            max_source_bytes: source_bytes - 1,
            ..AffectedOptions::default()
        },
        CAPACITY,
    );
    let mut cap = baseline.canonical_bytes().len();
    let mut expected = value.clone();
    let mut matched = None;
    for _ in 0..32 {
        expected["options"]["maxOutputBytes"] = json!(cap);
        let bytes = sealed(expected.clone());
        if bytes.len() == cap {
            matched = Some(bytes);
            break;
        }
        cap = bytes.len();
    }
    let expected = matched.expect("independent option/wire cap fixed point");
    let exact = AffectedOptions {
        max_output_bytes: cap,
        ..AffectedOptions::default()
    };
    assert_eq!(derive(&f, &exact).canonical_bytes(), expected);
    let mut lower = value.clone();
    lower["options"]["maxOutputBytes"] = json!(cap - 1);
    assert!(sealed(lower).len() > cap - 1);
    refusal(
        &f,
        &AffectedOptions {
            max_output_bytes: cap - 1,
            ..AffectedOptions::default()
        },
        CAPACITY,
    );
}

#[test]
fn canonical_identity_binds_every_option_input_and_false_inference_field() {
    let f = hierarchy(Edit::Derived, false);
    let a = derive(&f, &AffectedOptions::default());
    let b = derive(&f, &AffectedOptions::default());
    keys(
        a.value(),
        &[
            "schemaVersion",
            "kind",
            "algorithmVersion",
            "policy",
            "resultId",
            "reportArtifactId",
            "workspaceId",
            "inputs",
            "options",
            "entities",
            "observations",
            "evidence",
            "originalSeeds",
            "originalImpactRefs",
            "originalImpactObservationRefs",
            "expandedSeeds",
            "hierarchyStates",
            "hierarchyProofRefs",
            "memberReasons",
            "walkSeeds",
            "incomingStates",
            "incomingProofRefs",
            "coverage",
            "consumed",
            "rowUnits",
            "wireBytes",
            "inferenceClass",
            "complete",
            "compilerBindingEstablished",
            "runtimeDispatchEstablished",
            "limitations",
        ],
    );
    assert_eq!(a.value()["schemaVersion"], 1);
    assert_eq!(a.value()["reportArtifactId"], f.id());
    assert_eq!(a.value()["workspaceId"], f.report["workspaceId"]);
    assert_eq!(a.canonical_bytes(), b.canonical_bytes());
    assert_eq!(a.canonical_bytes(), sealed(a.value().clone()));
    assert_eq!(serde_json::to_value(&a).unwrap(), *a.value());
    assert_eq!(
        a.value()["wireBytes"].as_u64(),
        Some(a.canonical_bytes().len() as u64)
    );
    assert_eq!(a.value()["kind"], "git_affected_details_v1");
    assert_eq!(a.value()["algorithmVersion"], "yoke_git_affected_details_1");
    assert_eq!(a.value()["complete"], false);
    assert_eq!(
        a.value()["inferenceClass"],
        "source_bound_structural_candidate"
    );
    assert_eq!(a.value()["compilerBindingEstablished"], false);
    assert_eq!(a.value()["runtimeDispatchEstablished"], false);
    for label in ["base", "head"] {
        assert_eq!(
            a.value()["inputs"][label]["snapshotId"],
            f.report[format!("{label}SnapshotId")]
        );
        assert_eq!(
            a.value()["inputs"][label]["graphHash"],
            f.report[format!("{label}GraphHash")]
        );
        assert_eq!(a.value()["inputs"][label]["analyzerEpoch"], ANALYZER);
    }
    let smaller = derive(
        &f,
        &AffectedOptions {
            max_incoming_depth: 7,
            ..AffectedOptions::default()
        },
    );
    assert_ne!(a.value()["resultId"], smaller.value()["resultId"]);
    assert_eq!(smaller.canonical_bytes(), sealed(smaller.value().clone()));
    let mut changed = a.value().clone();
    changed["runtimeDispatchEstablished"] = json!(true);
    assert_ne!(sealed(changed), a.canonical_bytes());
}

#[test]
fn caller_transaction_sql_limit_and_state_survive_success_and_all_failure_classes() {
    let f = hierarchy(Edit::Derived, false);
    let db = &f.engine.store.db;
    let which = rusqlite::limits::Limit::SQLITE_LIMIT_LENGTH;
    let prior = db.limit(which).unwrap();
    let chosen = prior.min(1_000_000);
    db.set_limit(which, chosen).unwrap();
    db.execute_batch(
        "BEGIN DEFERRED; INSERT INTO meta(key,value) VALUES('R5B_CALLER_MARKER','owned');",
    )
    .unwrap();
    let before = state(&f.engine);
    derive(&f, &AffectedOptions::default());
    for (options, marker) in [
        (
            AffectedOptions {
                max_rows: 0,
                ..AffectedOptions::default()
            },
            CAPACITY,
        ),
        (
            AffectedOptions {
                policy: "other".into(),
                ..AffectedOptions::default()
            },
            INPUT,
        ),
    ] {
        assert_eq!(
            affected_details::derive(&f.engine.store, f.id(), &options, &deadline())
                .unwrap_err()
                .to_string(),
            marker
        );
        f.engine.store.clear_deadline();
        assert_eq!(db.limit(which).unwrap(), chosen);
        assert!(!db.is_autocommit());
        assert_eq!(state(&f.engine), before);
    }
    assert_eq!(
        affected_details::derive(
            &f.engine.store,
            f.id(),
            &AffectedOptions::default(),
            &Deadline::new(0)
        )
        .unwrap_err()
        .to_string(),
        DEADLINE
    );
    f.engine.store.clear_deadline();
    let cancel = Arc::new(AtomicBool::new(false));
    let mut entered = false;
    let outer = Deadline::run_bounded(&deadline(), cancel.clone(), || {
        entered = true;
        let child = deadline();
        cancel.store(true, Ordering::SeqCst);
        assert_eq!(
            affected_details::derive(&f.engine.store, f.id(), &AffectedOptions::default(), &child)
                .unwrap_err()
                .to_string(),
            DEADLINE
        );
        Ok(())
    });
    assert!(entered && outer.is_err());
    f.engine.store.clear_deadline();
    assert_eq!(db.limit(which).unwrap(), chosen);
    assert!(!db.is_autocommit());
    assert_eq!(state(&f.engine), before);
    assert_eq!(
        db.query_row(
            "SELECT value FROM meta WHERE key='R5B_CALLER_MARKER'",
            [],
            |r| r.get::<_, String>(0)
        )
        .unwrap(),
        "owned"
    );
    db.execute_batch("ROLLBACK").unwrap();
    assert!(db.is_autocommit());
    db.set_limit(which, prior).unwrap();
}
