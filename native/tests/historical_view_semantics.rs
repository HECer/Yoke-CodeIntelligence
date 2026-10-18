//! Historical comparison schema, source precision and ambiguity contracts.
//! Directed occurrences and independent Meaning generations retain original evidence.
//! Owned local workers run during explicit Meaning fixture setup only.
#[path = "support/historical_fixture_state.rs"]
mod fixture_state;
#[path = "support/meaning_semantics_fixture.rs"]
mod meaning_fixture;
#[path = "support/historical_schema_oracle.rs"]
mod schema;

use serde_json::{Value, json};
use std::{collections::BTreeSet, fs};
use tempfile::TempDir;
use yoke::{
    Engine,
    analysis::AnalyzerEpoch,
    capture::Deadline,
    graph_view::{self, BuildOptions, StructuralKind, View},
    graph_view_archive::{self, Limits},
    model::{Snapshot, identity},
    view_comparison::{self, ComparisonOptions, DeclaredFileContinuity, SelectionPin},
};

fn d() -> Deadline {
    Deadline::new(30_000)
}
fn assert_result_contract(comparison: &view_comparison::Comparison, sentinels: &[&str]) {
    schema::validate(comparison.value(), sentinels).unwrap();
    assert_eq!(
        comparison.canonical_bytes(),
        serde_json::to_vec(comparison.value()).unwrap()
    );
    let mut without_id = comparison.value().clone();
    without_id
        .as_object_mut()
        .unwrap()
        .remove("comparisonId")
        .unwrap();
    assert_eq!(comparison.value()["comparisonId"], identity(&without_id));
}
fn rows(value: &Value) -> &[Value] {
    value.as_array().expect("closed output array")
}
fn id(value: &Value, key: &str) -> String {
    value[key].as_str().expect("original identity").into()
}
fn source_pair(before: &str, after: &str) -> (TempDir, Engine, View, View) {
    let root = TempDir::new().unwrap();
    fs::write(root.path().join("main.rs"), before).unwrap();
    let mut engine = Engine::open(root.path()).unwrap();
    engine.capture_epoch = AnalyzerEpoch::V9;
    let (snapshot, graph, _, _) = engine.index(&d()).unwrap();
    let options = BuildOptions {
        structural_kinds: vec![],
        ..Default::default()
    };
    let left = graph_view::build(&engine.store, &snapshot, &graph, &options, &d()).unwrap();
    fs::write(root.path().join("main.rs"), after).unwrap();
    let (snapshot, graph, _, _) = engine.index(&d()).unwrap();
    let right = graph_view::build(&engine.store, &snapshot, &graph, &options, &d()).unwrap();
    (root, engine, left, right)
}
fn function_id(view: &View, name: &str) -> String {
    let found = view
        .nodes
        .iter()
        .filter(|n| n["kind"] == "function" && n["name"] == name)
        .collect::<Vec<_>>();
    assert_eq!(
        found.len(),
        1,
        "Genuine fixture requires one function {name}"
    );
    id(found[0], "id")
}
fn source_descriptors<'a>(value: &'a Value, side: &str) -> Vec<&'a Value> {
    let mut found = Vec::new();
    for change in rows(&value["source"]["nodeChanges"]) {
        if !change[side].is_null() {
            found.push(&change[side]);
        }
    }
    for group in rows(&value["source"]["ambiguousNodes"]) {
        found.extend(rows(&group[format!("{side}Rows")]));
    }
    found
}
fn assert_node_rows(value: &Value, view: &View, side: &str) {
    let actual = source_descriptors(value, side);
    assert_eq!(
        actual.len(),
        view.nodes.len(),
        "Every selected node descriptor occurs once per side"
    );
    let mut seen = BTreeSet::new();
    for descriptor in actual {
        let original = view
            .nodes
            .iter()
            .find(|n| n["id"] == descriptor["originalNodeId"])
            .expect("Only original selected node IDs");
        assert!(
            seen.insert(id(descriptor, "originalNodeId")),
            "Duplicate output node descriptor"
        );
        assert_eq!(descriptor["side"], side);
        assert_eq!(
            descriptor["originalRowDigest"],
            identity(original),
            "Digest FULL original row, including signature and any fields omitted from output"
        );
        assert_eq!(
            descriptor["digestMethod"],
            "sha256_canonical_original_json_v1"
        );
        assert_eq!(descriptor["graphViewId"], view.id);
        assert_eq!(descriptor["workspaceId"], view.workspace_id);
        assert_eq!(descriptor["snapshotId"], view.snapshot_id);
        for key in ["path", "name", "kind", "origin", "producer"] {
            assert_eq!(descriptor[key], original[key]);
        }
        for key in ["qualifiedName", "trust", "providerAttestation"] {
            assert_eq!(
                descriptor.get(key),
                original.get(key),
                "Original optional {key}"
            );
        }
        assert_eq!(descriptor["textOmitted"], true);
        assert!(descriptor.get("signature").is_none());
        if let Some(signature) = original.get("signature") {
            assert_eq!(descriptor["signatureDigest"], identity(signature));
        } else {
            assert!(descriptor.get("signatureDigest").is_none());
        }
        let original_bindings = rows(&original["sourceBindings"]);
        let bindings = rows(&descriptor["sourceBindings"]);
        assert_eq!(bindings.len(), original_bindings.len());
        for (binding, expected) in bindings.iter().zip(original_bindings) {
            schema::assert_original_anchor(binding, expected);
        }
        if bindings.len() == 1 {
            assert_eq!(descriptor["representativeAnchor"], bindings[0]);
        } else {
            assert!(
                descriptor["representativeAnchor"].is_null(),
                "No fabricated representative for multiple bindings"
            );
        }
    }
}
fn assert_occurrence_rows(
    actual: &Value,
    view: &View,
    source: &str,
    target: &str,
    literal_count: usize,
) {
    let original = view
        .observations
        .iter()
        .filter(|r| r["relation"] == "calls" && r["source"] == source && r["target"] == target)
        .collect::<Vec<_>>();
    assert_eq!(
        original.len(),
        literal_count,
        "Independent genuine captured-call control"
    );
    assert_eq!(rows(actual).len(), literal_count);
    let mut seen = BTreeSet::new();
    for occurrence in rows(actual) {
        let row = original
            .iter()
            .find(|r| r["id"] == occurrence["originalObservationId"])
            .expect("No fabricated occurrence IDs");
        assert!(
            seen.insert(id(occurrence, "originalObservationId")),
            "Parallel occurrence must not be duplicated"
        );
        assert_eq!(occurrence["originalRowDigest"], identity(*row));
        assert_eq!(occurrence["sourceId"], source);
        assert_eq!(occurrence["targetId"], target);
        assert_eq!(occurrence["snapshotId"], row["snapshotId"]);
        assert_eq!(occurrence["evidenceIds"], row["evidenceIds"]);
        assert_eq!(
            occurrence.get("sourceBindings").is_some(),
            row.get("sourceBindings").is_some(),
            "Do not synthesize canonical observation sourceBindings"
        );
        assert_eq!(
            rows(&occurrence["evidence"]).len(),
            rows(&row["evidenceIds"]).len()
        );
        for (redacted, evidence_id) in rows(&occurrence["evidence"])
            .iter()
            .zip(rows(&row["evidenceIds"]))
        {
            let evidence = view
                .evidence
                .iter()
                .find(|e| e["id"] == *evidence_id)
                .unwrap();
            assert_eq!(redacted["originalEvidenceId"], *evidence_id);
            assert_eq!(redacted["originalRowDigest"], identity(evidence));
            schema::assert_original_anchor(redacted, evidence);
        }
    }
}

#[test]
fn h10_position_closed_schema_preserves_unicode_crlf_file_and_multiple_bindings() {
    const BEFORE: &str = "// __H10_BEFORE_1ce8__ caf\u{e9}\r\nfn unicode() -> &'static str { \"\u{fc}ber\" }\r\nfn caller(){unicode();}\r\n";
    const AFTER: &str = "// __H10_AFTER_70ad__ caf\u{e9}\r\nfn unicode() -> &'static str { \"\u{fc}ber!\" }\r\nfn caller(){unicode();}\r\n";
    const PACKAGE: &str = "{\r\n  \"name\":\"demo\",\r\n  \"scripts\":{\"echo\":\"demo\"},\r\n  \"dependencies\":{\"unique-widget\":\"*\"}\r\n}\r\n";
    const GUIDE: &str =
        "# Guide\r\n<!-- __H10_MARKDOWN_58e9__ caf\u{e9} -->\r\n`main.rs::unicode`\r\n";
    assert_eq!(PACKAGE.match_indices("demo").count(), 2);
    assert_eq!(PACKAGE.match_indices("unique-widget").count(), 1);
    let root = TempDir::new().unwrap();
    fs::write(root.path().join("main.rs"), BEFORE).unwrap();
    fs::write(root.path().join("package.json"), PACKAGE).unwrap();
    fs::write(root.path().join("GUIDE.md"), GUIDE).unwrap();
    let mut engine = Engine::open(root.path()).unwrap();
    engine.capture_epoch = AnalyzerEpoch::V9;
    let (snapshot, graph, _, _) = engine.index(&d()).unwrap();
    let before = graph_view::build(
        &engine.store,
        &snapshot,
        &graph,
        &BuildOptions {
            structural_kinds: vec![StructuralKind::Manifest, StructuralKind::Markdown],
            ..Default::default()
        },
        &d(),
    )
    .unwrap();
    fs::write(root.path().join("main.rs"), AFTER).unwrap();
    let (snapshot, graph, _, _) = engine.index(&d()).unwrap();
    // Explicit PUBLIC source-bound host overlay claim with two genuine evidence
    // bindings. This is not automatic extraction or compiler/provider identity,
    // and changes no archive proof or source grant (H09 has separate tamper tests).
    let mut host_claim =
        yoke::manifest_graph::extract(&engine.store, &snapshot, &graph, &d()).unwrap();
    assert_eq!(host_claim.nodes.len(), 1);
    assert_eq!(host_claim.evidence.len(), 2);
    assert_ne!(host_claim.evidence[0]["id"], host_claim.evidence[1]["id"]);
    assert_eq!(
        host_claim.evidence[1]["anchor"],
        "unique_literal_occurrence"
    );
    assert_eq!(
        host_claim.evidence[0]["anchor"], "file",
        "Repeated package label must stay file precision"
    );
    host_claim.nodes[0]["evidenceIds"] =
        json!([host_claim.evidence[0]["id"], host_claim.evidence[1]["id"]]);
    host_claim.artifact_id.clear();
    let artifact = engine
        .store
        .artifact(&serde_json::to_vec(&host_claim).unwrap())
        .unwrap();
    engine.store.record(yoke::manifest_graph::RECORD_KIND, &artifact, &json!({"snapshotId":snapshot.id,"workspaceId":snapshot.workspace_id,"graphHash":identity(&graph),"artifactId":artifact,"producer":host_claim.producer})).unwrap();
    let after = graph_view::build(
        &engine.store,
        &snapshot,
        &graph,
        &BuildOptions {
            structural_kinds: vec![StructuralKind::Markdown],
            structural_overlay_ids: vec![artifact],
            ..Default::default()
        },
        &d(),
    )
    .unwrap();
    assert_eq!(
        graph_view::load(&engine.store, &after.id, &d()).unwrap().id,
        after.id,
        "Published host claim passes current full grant/proof validation"
    );
    let before_package = before
        .nodes
        .iter()
        .find(|n| n["kind"] == "package")
        .unwrap();
    let after_package = after.nodes.iter().find(|n| n["kind"] == "package").unwrap();
    assert_eq!(rows(&before_package["sourceBindings"]).len(), 1);
    assert_eq!(before_package["sourceBindings"][0]["anchor"], "file");
    assert_eq!(before_package["sourceBindings"][0]["startByte"], 0);
    assert_eq!(
        before_package["sourceBindings"][0]["endByte"],
        PACKAGE.len()
    );
    assert_eq!(rows(&after_package["sourceBindings"]).len(), 2);
    let unicode = after
        .nodes
        .iter()
        .find(|n| n["kind"] == "function" && n["name"] == "unicode")
        .unwrap();
    let start = unicode["sourceBindings"][0]["startByte"].as_u64().unwrap() as usize;
    let end = unicode["sourceBindings"][0]["endByte"].as_u64().unwrap() as usize;
    assert!(AFTER.get(start..end).unwrap().contains("\"\u{fc}ber!\""));
    assert!(
        start > AFTER[..start].chars().count(),
        "Actual multibyte prefix proves byte/scalar offsets differ"
    );
    assert!(AFTER.contains("\r\n"));
    let baseline = fixture_state::state(&engine);
    let comparison = view_comparison::compare_published(
        &engine.store,
        &SelectionPin::from_view(&before),
        &SelectionPin::from_view(&after),
        &ComparisonOptions::default(),
        &d(),
    )
    .unwrap();
    let output = comparison.value();
    let sentinels = [
        "__H10_BEFORE_1ce8__",
        "__H10_AFTER_70ad__",
        "__H10_MARKDOWN_58e9__",
        "\"\u{fc}ber!\"",
    ];
    assert_result_contract(&comparison, &sentinels);
    assert_node_rows(output, &before, "before");
    assert_node_rows(output, &after, "after");
    let unicode_change = rows(&output["source"]["nodeChanges"])
        .iter()
        .find(|c| c["after"]["originalNodeId"] == unicode["id"])
        .unwrap();
    assert_eq!(
        unicode_change["changeKind"], "content_and_range_changed",
        "The UTF-8 body changes, and the unequal comment-prefix byte lengths also move its original range"
    );
    let redacted_package = source_descriptors(output, "after")
        .into_iter()
        .find(|n| n["originalNodeId"] == after_package["id"])
        .unwrap();
    assert!(redacted_package["representativeAnchor"].is_null());
    assert_eq!(redacted_package["sourceBindings"][0]["anchor"], "file");
    // Wrong-position keys are globally legitimate elsewhere. A UNION allowlist
    // would accept these; the recursive per-position oracle must refuse them.
    for (pointer, key, value) in [
        ("/before/coverage", "participants", json!([])),
        ("/rowBudget", "sourceId", json!(before.id)),
        ("/before/coverage", "sourceFiles", json!("1")),
    ] {
        let mut forged = output.clone();
        forged
            .pointer_mut(pointer)
            .unwrap()
            .as_object_mut()
            .unwrap()
            .insert(key.into(), value);
        assert!(
            schema::validate(&forged, &sentinels).is_err(),
            "Position/type calibration {pointer}/{key}"
        );
    }
    let mut inflated_limit = output.clone();
    inflated_limit["rowBudget"]["limit"] =
        json!(output["options"]["maxResultRows"].as_u64().unwrap() + 1);
    assert!(
        schema::validate(&inflated_limit, &sentinels)
            .unwrap_err()
            .contains("options.maxResultRows"),
        "A response cannot inflate its self-reported row cap"
    );
    let mut oversized = output.clone();
    oversized["options"]["maxOutputBytes"] = json!(1);
    assert!(
        schema::validate(&oversized, &sentinels)
            .unwrap_err()
            .contains("whole canonical Value"),
        "All canonical response bytes must fit the requested cap"
    );
    let mut leaked = output.clone();
    leaked["before"]["overlays"][0]["producer"] = json!("__H10_BEFORE_1ce8__");
    assert!(
        schema::validate(&leaked, &sentinels)
            .unwrap_err()
            .contains("evidence-only sentinel")
    );
    assert_eq!(fixture_state::state(&engine), baseline);
}

#[test]
fn h12_directed_parallel_and_self_loop_occurrences_use_original_ids_and_literal_counts() {
    const BEFORE: &str = "fn looped(){looped();}\nfn left(){right();right();}\nfn right(){}\n";
    const AFTER: &str =
        "fn looped(){looped();looped();}\nfn left(){right();}\nfn right(){left();}\n";
    let (_dir, engine, before, after) = source_pair(BEFORE, AFTER);
    let baseline = fixture_state::state(&engine);
    let comparison = view_comparison::compare_published(
        &engine.store,
        &SelectionPin::from_view(&before),
        &SelectionPin::from_view(&after),
        &ComparisonOptions::default(),
        &d(),
    )
    .unwrap();
    let output = comparison.value();
    assert_result_contract(&comparison, &[]);
    assert_eq!(
        output["source"]["totals"],
        json!({"beforeCalls":3,"afterCalls":4,"beforeSelfLoops":1,"afterSelfLoops":2})
    );
    for (source_name, target_name, left_count, right_count, delta) in [
        ("left", "right", 2, 1, -1),
        ("right", "left", 0, 1, 1),
        ("looped", "looped", 1, 2, 1),
    ] {
        let left_source = function_id(&before, source_name);
        let left_target = function_id(&before, target_name);
        let right_source = function_id(&after, source_name);
        let right_target = function_id(&after, target_name);
        let groups = rows(&output["source"]["observationChanges"])
            .iter()
            .filter(|group| {
                group["relation"] == "calls"
                    && (rows(&group["beforeRows"])
                        .iter()
                        .any(|r| r["sourceId"] == left_source && r["targetId"] == left_target)
                        || rows(&group["afterRows"]).iter().any(|r| {
                            r["sourceId"] == right_source && r["targetId"] == right_target
                        }))
            })
            .collect::<Vec<_>>();
        assert_eq!(
            groups.len(),
            1,
            "One evidence-qualified directed fixture group {source_name}->{target_name}"
        );
        let group = groups[0];
        assert_eq!(group["beforeCount"], left_count);
        assert_eq!(group["afterCount"], right_count);
        assert_eq!(group["delta"], delta);
        assert_eq!(group["occurrenceIdentity"], "not_inferred");
        assert!(group["sourceCorrespondenceId"].is_string());
        assert!(group["targetCorrespondenceId"].is_string());
        assert_occurrence_rows(
            &group["beforeRows"],
            &before,
            &left_source,
            &left_target,
            left_count,
        );
        assert_occurrence_rows(
            &group["afterRows"],
            &after,
            &right_source,
            &right_target,
            right_count,
        );
    }
    assert_eq!(fixture_state::state(&engine), baseline);
}

fn claim_rows(view: &View) -> Vec<(String, usize, usize, Value)> {
    let layer = view.meaning.as_ref().unwrap();
    let mut result = Vec::new();
    for packet in &layer.packets {
        for (kind, claims) in [
            (
                "concept",
                packet
                    .concepts
                    .iter()
                    .map(|r| serde_json::to_value(r).unwrap())
                    .collect::<Vec<_>>(),
            ),
            (
                "relation",
                packet
                    .relations
                    .iter()
                    .map(|r| serde_json::to_value(r).unwrap())
                    .collect::<Vec<_>>(),
            ),
            (
                "hyperedge",
                packet
                    .hyperedges
                    .iter()
                    .map(|r| serde_json::to_value(r).unwrap())
                    .collect::<Vec<_>>(),
            ),
        ] {
            for row in claims {
                result.push((kind.into(), packet.selection_index, packet.index, row));
            }
        }
    }
    result
}
fn assert_meaning_rows(output: &Value, view: &View, side: &str) {
    let expected = claim_rows(view);
    let mut seen = BTreeSet::new();
    let layer = view.meaning.as_ref().unwrap();
    for change in rows(&output["meaning"]["claimChanges"]) {
        let actual = &change[side];
        if actual.is_null() {
            continue;
        }
        let (kind, selection_index, packet_index, original) = expected
            .iter()
            .find(|(_, _, _, row)| row["id"] == actual["originalClaimId"])
            .unwrap();
        assert!(
            seen.insert(id(actual, "originalClaimId")),
            "Selected claim must be retained exactly once"
        );
        assert_eq!(actual["kind"], *kind);
        assert_eq!(actual["side"], side);
        assert_eq!(
            actual["materializedClaimId"], original["claimId"],
            "Do not replace the materialized identity with selected View ID"
        );
        assert_ne!(actual["originalClaimId"], actual["materializedClaimId"]);
        assert_eq!(
            actual["originalRowDigest"],
            identity(original),
            "Digest selected FULL row, including descriptions/aliases/roles before redaction"
        );
        assert_eq!(actual["selectionIndex"], *selection_index);
        assert_eq!(actual["packetIndex"], *packet_index);
        assert_eq!(actual["graphViewId"], view.id);
        assert_eq!(actual["snapshotId"], view.snapshot_id);
        let selected = &layer.selections[*selection_index];
        assert_eq!(actual["graphId"], selected.graph_id);
        assert_eq!(actual["jobId"], selected.job_id);
        assert_eq!(actual["evidenceIds"], original["evidenceIds"]);
        assert_eq!(actual["resolution"], "inferred_concept");
        assert_eq!(actual["providerAttestation"], "unverified");
        for key in [
            "label",
            "aliases",
            "description",
            "path",
            "anchor",
            "representativeAnchor",
            "sourceBindings",
        ] {
            assert!(
                actual.get(key).is_none(),
                "Not a concept declaration or raw provider field: {key}"
            );
        }
        match kind.as_str() {
            "relation" => {
                assert_eq!(actual["sourceId"], original["source"]);
                assert_eq!(actual["targetId"], original["target"]);
            }
            "hyperedge" => {
                assert_eq!(actual["participants"], original["participants"]);
                assert!(actual.get("sourceId").is_none());
                assert!(actual.get("targetId").is_none());
            }
            "concept" => {
                assert!(actual.get("sourceId").is_none());
                assert!(actual.get("targetId").is_none());
                assert!(actual.get("participants").is_none());
            }
            _ => unreachable!(),
        }
        let packet = layer
            .packets
            .iter()
            .find(|p| p.selection_index == *selection_index && p.index == *packet_index)
            .unwrap();
        assert_eq!(
            rows(&actual["evidence"]).len(),
            rows(&original["evidenceIds"]).len()
        );
        for (evidence, evidence_id) in rows(&actual["evidence"])
            .iter()
            .zip(rows(&original["evidenceIds"]))
        {
            let original = packet
                .evidence
                .iter()
                .find(|e| e.id == evidence_id.as_str().unwrap())
                .unwrap();
            let original_value = serde_json::to_value(original).unwrap();
            assert_eq!(evidence["originalEvidenceId"], original.id);
            assert_eq!(evidence["baseEvidenceId"], original.base_evidence_id);
            assert_eq!(evidence["originalRowDigest"], identity(&original_value));
            for key in [
                "sourceId",
                "snapshotId",
                "path",
                "contentHash",
                "startByte",
                "endByte",
                "startLine",
                "endLine",
            ] {
                assert_eq!(evidence[key], original_value[key]);
            }
            assert_eq!(evidence["precision"], "captured_utf8_quote_range");
            assert_eq!(
                evidence["bindingDerivation"],
                "checked_meaning_evidence_quote_range_not_declaration"
            );
            assert!(
                evidence.get("anchor").is_none(),
                "There was no original MeaningEvidence.anchor"
            );
            assert!(evidence.get("text").is_none());
        }
    }
    assert_eq!(seen.len(), expected.len());
    let receipts = rows(&output["meaning"]["provenance"][format!("{side}Rows")]);
    assert_eq!(receipts.len(), layer.packets.len());
    for packet in &layer.packets {
        let receipt = receipts
            .iter()
            .find(|r| {
                r["selectionIndex"] == packet.selection_index && r["packetIndex"] == packet.index
            })
            .unwrap();
        let original = serde_json::to_value(&packet.provenance).unwrap();
        assert_eq!(receipt["originalRowDigest"], identity(&original));
        for (key, value) in original.as_object().unwrap() {
            assert_eq!(
                receipt[key], *value,
                "Original receipt {key}, not current task totals"
            );
        }
        assert_eq!(receipt["providerReportedCostMicros"], Value::Null);
        assert_eq!(receipt["providerReportedUsage"], Value::Null);
        assert_eq!(
            rows(&receipt["sourceBindings"]).len(),
            packet.source_bindings.len()
        );
        for (binding, original) in rows(&receipt["sourceBindings"])
            .iter()
            .zip(&packet.source_bindings)
        {
            let original = serde_json::to_value(original).unwrap();
            assert_eq!(binding["originalRowDigest"], identity(&original));
            for (key, value) in original.as_object().unwrap() {
                assert_eq!(binding[key], *value);
            }
            for key in ["snapshotId", "startLine", "endLine", "anchor"] {
                assert!(
                    binding.get(key).is_none(),
                    "Absent original unit binding field {key}"
                );
            }
        }
        assert_eq!(
            receipt["coverage"],
            serde_json::to_value(&packet.coverage).unwrap()
        );
    }
}

#[test]
fn h12_independent_meaning_generations_keep_order_repetition_direction_and_exact_namespaces_storelessly()
 {
    let fixture = meaning_fixture::fixture();
    assert_eq!(fixture.graph.snapshot_id, fixture.snapshot.id);
    assert_eq!(fixture.before.meaning.as_ref().unwrap().packets.len(), 3);
    assert_eq!(fixture.after.meaning.as_ref().unwrap().packets.len(), 3);
    let before_claims = claim_rows(&fixture.before);
    let after_claims = claim_rows(&fixture.after);
    assert_eq!(before_claims.len(), 10);
    assert_eq!(after_claims.len(), 11);
    for (view, forward, reverse, self_loops, expected_order) in [
        (
            &fixture.before,
            2,
            1,
            1,
            vec![
                "Alpha concept",
                "Beta concept",
                "Alpha concept",
                "Gamma concept",
            ],
        ),
        (
            &fixture.after,
            1,
            2,
            2,
            vec![
                "Beta concept",
                "Alpha concept",
                "Alpha concept",
                "Gamma concept",
            ],
        ),
    ] {
        let joint = view
            .meaning
            .as_ref()
            .unwrap()
            .packets
            .iter()
            .find(|p| p.source_bindings.len() == 2)
            .unwrap();
        let concept = |label: &str| &joint.concepts.iter().find(|c| c.label == label).unwrap().id;
        let a = concept("Alpha concept");
        let b = concept("Beta concept");
        assert_eq!(
            joint
                .relations
                .iter()
                .filter(|r| &r.source == a && &r.target == b)
                .count(),
            forward
        );
        assert_eq!(
            joint
                .relations
                .iter()
                .filter(|r| &r.source == b && &r.target == a)
                .count(),
            reverse
        );
        assert_eq!(
            joint
                .relations
                .iter()
                .filter(|r| &r.source == a && &r.target == a)
                .count(),
            self_loops
        );
        assert_eq!(joint.hyperedges.len(), 1);
        let edge = &joint.hyperedges[0];
        assert_eq!(
            edge.participants
                .iter()
                .map(|p| p.role.as_str())
                .collect::<Vec<_>>(),
            ["input", "input", "context", "output"]
        );
        let labels = edge
            .participants
            .iter()
            .map(|p| {
                joint
                    .concepts
                    .iter()
                    .find(|c| c.id == p.concept)
                    .unwrap()
                    .label
                    .as_str()
            })
            .collect::<Vec<_>>();
        assert_eq!(labels, expected_order);
        assert_eq!(
            edge.participants.iter().filter(|p| &p.concept == a).count(),
            2,
            "Actual repeated concept under DISTINCT roles, not a forbidden duplicate (role,concept) pair"
        );
        assert_eq!(
            edge.participants
                .iter()
                .map(|p| (&p.role, &p.concept))
                .collect::<BTreeSet<_>>()
                .len(),
            4,
            "Current model requires each role/concept pair unique"
        );
        assert!(
            joint
                .evidence
                .iter()
                .any(|e| e.path == "a.rs" && e.text.contains("caf\u{e9}\r\n"))
        );
        assert!(
            joint
                .evidence
                .iter()
                .any(|e| e.path == "b.rs" && e.text.contains("\u{3b2}\r\n"))
        );
        assert!(
            joint
                .evidence
                .iter()
                .all(|e| e.start_byte < e.end_byte && e.snapshot_id == view.snapshot_id)
        );
    }
    let before_ids = before_claims
        .iter()
        .map(|(_, _, _, r)| id(r, "id"))
        .collect::<BTreeSet<_>>();
    let after_ids = after_claims
        .iter()
        .map(|(_, _, _, r)| id(r, "id"))
        .collect::<BTreeSet<_>>();
    assert!(
        before_ids.is_disjoint(&after_ids),
        "Exact selected namespaces differ despite unchanged labels and aliases"
    );
    let materialized_before = before_claims
        .iter()
        .map(|(_, _, _, r)| id(r, "claimId"))
        .collect::<BTreeSet<_>>();
    let materialized_after = after_claims
        .iter()
        .map(|(_, _, _, r)| id(r, "claimId"))
        .collect::<BTreeSet<_>>();
    assert!(
        !materialized_before.is_disjoint(&materialized_after),
        "Positive control: some original materialized claims really are equal, but namespace identity is still distinct"
    );
    let baseline = fixture_state::state(&fixture.engine);
    let left = graph_view_archive::prepare(
        &fixture.engine.store,
        &fixture.before,
        Limits::default(),
        &d(),
    )
    .unwrap();
    let right = graph_view_archive::prepare(
        &fixture.engine.store,
        &fixture.after,
        Limits::default(),
        &d(),
    )
    .unwrap();
    let left = graph_view_archive::parse(left.bytes(), Limits::default(), &d()).unwrap();
    let right = graph_view_archive::parse(right.bytes(), Limits::default(), &d()).unwrap();
    assert_eq!(fixture_state::state(&fixture.engine), baseline);
    let marker = fixture.marker.clone();
    let calls = fs::read(&marker).unwrap();
    let before = fixture.before.clone();
    let after = fixture.after.clone();
    let dir = meaning_fixture::remove_store_and_workspace(fixture);
    let output = view_comparison::compare_meaning_archives(
        &left,
        &right,
        &ComparisonOptions::default(),
        &d(),
    )
    .unwrap();
    assert_result_contract(&output, meaning_fixture::SENTINELS);
    assert_eq!(rows(&output.value()["meaning"]["sharedClaimIds"]).len(), 0);
    assert_eq!(
        rows(&output.value()["meaning"]["removedClaimIds"]).len(),
        10
    );
    assert_eq!(rows(&output.value()["meaning"]["addedClaimIds"]).len(), 11);
    assert_eq!(rows(&output.value()["meaning"]["claimChanges"]).len(), 21);
    assert!(
        rows(&output.value()["meaning"]["receiptReuse"]).is_empty(),
        "Two fresh original generations must not fabricate receipt reuse"
    );
    let actual_before = rows(&output.value()["meaning"]["removedClaimIds"])
        .iter()
        .map(|v| v.as_str().unwrap().to_owned())
        .collect::<BTreeSet<_>>();
    let actual_after = rows(&output.value()["meaning"]["addedClaimIds"])
        .iter()
        .map(|v| v.as_str().unwrap().to_owned())
        .collect::<BTreeSet<_>>();
    assert_eq!(actual_before, before_ids);
    assert_eq!(actual_after, after_ids);
    assert_eq!(
        output.value()["before"]["meaningCapturePolicyHash"],
        before.meaning.as_ref().unwrap().scope.capture_policy_hash
    );
    assert_eq!(
        output.value()["after"]["meaningCapturePolicyHash"],
        after.meaning.as_ref().unwrap().scope.capture_policy_hash
    );
    assert_meaning_rows(output.value(), &before, "before");
    assert_meaning_rows(output.value(), &after, "after");
    assert_eq!(
        output.value()["meaning"]["provenance"]["beforeUnknownCostPackets"],
        3
    );
    assert_eq!(
        output.value()["meaning"]["provenance"]["afterUnknownCostPackets"],
        3
    );
    assert_eq!(
        output.value()["meaning"]["provenance"]["beforeUnknownUsagePackets"],
        3
    );
    assert_eq!(
        output.value()["meaning"]["provenance"]["afterUnknownUsagePackets"],
        3
    );
    assert_eq!(fs::read(&marker).unwrap(), calls);
    assert!(!dir.path().join("workspace").exists());
}

#[test]
fn h16_duplicate_structural_candidates_and_their_calls_stay_ambiguous_even_with_host_continuity() {
    const BEFORE: &str = "fn twin(){sink();}\nfn twin(){sink();}\nfn sink(){}\n";
    const AFTER: &str = "fn twin(){sink();}\nfn twin(){sink();}\nfn twin(){sink();}\nfn sink(){}\n";
    let (_dir, engine, before, after) = source_pair(BEFORE, AFTER);
    let twins = |view: &View| {
        view.nodes
            .iter()
            .filter(|r| r["kind"] == "function" && r["name"] == "twin")
            .map(|r| id(r, "id"))
            .collect::<BTreeSet<_>>()
    };
    let left_twins = twins(&before);
    let right_twins = twins(&after);
    assert_eq!(left_twins.len(), 2);
    assert_eq!(right_twins.len(), 3);
    let left_snapshot: Snapshot = engine.store.snapshot(&before.snapshot_id).unwrap();
    let right_snapshot: Snapshot = engine.store.snapshot(&after.snapshot_id).unwrap();
    let left_file = left_snapshot
        .files
        .iter()
        .find(|f| f.path == "main.rs")
        .unwrap();
    let right_file = right_snapshot
        .files
        .iter()
        .find(|f| f.path == "main.rs")
        .unwrap();
    let baseline = fixture_state::state(&engine);
    for declarations in [
        vec![],
        vec![DeclaredFileContinuity {
            before_path: "main.rs".into(),
            before_hash: left_file.hash.clone(),
            after_path: "main.rs".into(),
            after_hash: right_file.hash.clone(),
        }],
    ] {
        let options = ComparisonOptions {
            declared_file_continuity: declarations,
            ..Default::default()
        };
        let comparison = view_comparison::compare_published(
            &engine.store,
            &SelectionPin::from_view(&before),
            &SelectionPin::from_view(&after),
            &options,
            &d(),
        )
        .unwrap();
        let output = comparison.value();
        assert_result_contract(&comparison, &[]);
        assert_node_rows(output, &before, "before");
        assert_node_rows(output, &after, "after");
        let groups = rows(&output["source"]["ambiguousNodes"])
            .iter()
            .filter(|group| {
                rows(&group["beforeRows"])
                    .iter()
                    .any(|r| r["name"] == "twin")
                    || rows(&group["afterRows"])
                        .iter()
                        .any(|r| r["name"] == "twin")
            })
            .collect::<Vec<_>>();
        assert_eq!(groups.len(), 1);
        let group = groups[0];
        assert_eq!(group["beforeCount"], 2);
        assert_eq!(group["afterCount"], 3);
        assert_eq!(group["verifiedIdentity"], false);
        assert_eq!(
            group["identityBasis"],
            "ambiguous_structural_label_candidates"
        );
        assert_eq!(
            rows(&group["beforeRows"])
                .iter()
                .map(|r| id(r, "originalNodeId"))
                .collect::<BTreeSet<_>>(),
            left_twins
        );
        assert_eq!(
            rows(&group["afterRows"])
                .iter()
                .map(|r| id(r, "originalNodeId"))
                .collect::<BTreeSet<_>>(),
            right_twins
        );
        for node_change in rows(&output["source"]["nodeChanges"]) {
            assert!(
                !left_twins.contains(
                    node_change["before"]["originalNodeId"]
                        .as_str()
                        .unwrap_or("")
                )
            );
            assert!(
                !right_twins.contains(
                    node_change["after"]["originalNodeId"]
                        .as_str()
                        .unwrap_or("")
                ),
                "No arbitrary pair/added/removed promotion of ambiguous declarations"
            );
        }
        let ambiguous = rows(&output["source"]["ambiguousObservations"])
            .iter()
            .filter(|g| g["relation"] == "calls")
            .collect::<Vec<_>>();
        let actual_left = ambiguous
            .iter()
            .flat_map(|g| rows(&g["beforeRows"]))
            .collect::<Vec<_>>();
        let actual_right = ambiguous
            .iter()
            .flat_map(|g| rows(&g["afterRows"]))
            .collect::<Vec<_>>();
        assert_eq!(actual_left.len(), 2);
        assert_eq!(actual_right.len(), 3);
        for (view, candidates, actual, literal) in [
            (&before, &left_twins, actual_left, 2),
            (&after, &right_twins, actual_right, 3),
        ] {
            let sink = function_id(view, "sink");
            let expected = view
                .observations
                .iter()
                .filter(|r| {
                    r["relation"] == "calls" && candidates.contains(r["source"].as_str().unwrap())
                })
                .collect::<Vec<_>>();
            assert_eq!(expected.len(), literal);
            assert_eq!(
                expected
                    .iter()
                    .map(|r| id(r, "id"))
                    .collect::<BTreeSet<_>>(),
                actual
                    .iter()
                    .map(|r| id(r, "originalObservationId"))
                    .collect::<BTreeSet<_>>()
            );
            for occurrence in actual {
                let original = expected
                    .iter()
                    .find(|r| r["id"] == occurrence["originalObservationId"])
                    .unwrap();
                assert_eq!(occurrence["originalRowDigest"], identity(*original));
                assert_eq!(occurrence["sourceId"], original["source"]);
                assert_eq!(occurrence["targetId"], sink);
                assert_eq!(occurrence["evidenceIds"], original["evidenceIds"]);
            }
        }
        for g in ambiguous {
            assert_eq!(g["verifiedIdentity"], false);
            assert!(g["sourceCorrespondenceId"].is_null());
            assert!(
                g["targetCorrespondenceId"].is_string(),
                "A unique sink may be a candidate without identifying ambiguous callers"
            );
        }
        assert!(
            rows(&output["source"]["observationChanges"])
                .iter()
                .all(|g| g["relation"] != "calls"),
            "Do not cancel or arbitrarily pair ambiguous directed occurrences"
        );
        assert_eq!(
            output["declaredFileContinuity"].as_array().unwrap().len(),
            options.declared_file_continuity.len()
        );
        for declaration in rows(&output["declaredFileContinuity"]) {
            assert_eq!(declaration["verifiedIdentity"], false);
        }
        assert_eq!(fixture_state::state(&engine), baseline);
    }
}

#[test]
fn h12_owned_fixture_qualifies_two_legal_independent_generations_before_comparison() {
    let fixture = meaning_fixture::fixture();
    assert_eq!(claim_rows(&fixture.before).len(), 10);
    assert_eq!(claim_rows(&fixture.after).len(), 11);
    for view in [&fixture.before, &fixture.after] {
        let layer = view.meaning.as_ref().unwrap();
        assert_eq!(layer.packets.len(), 3);
        assert!(layer.packets.iter().all(|p| !p.provenance.rebound));
        let joint = layer
            .packets
            .iter()
            .find(|p| p.source_bindings.len() == 2)
            .unwrap();
        assert_eq!(joint.hyperedges.len(), 1);
        let participants = &joint.hyperedges[0].participants;
        assert_eq!(participants.len(), 4);
        assert_eq!(
            participants
                .iter()
                .map(|p| p.role.as_str())
                .collect::<Vec<_>>(),
            ["input", "input", "context", "output"]
        );
        assert_eq!(
            participants
                .iter()
                .map(|p| (&p.role, &p.concept))
                .collect::<BTreeSet<_>>()
                .len(),
            4
        );
        assert_eq!(
            joint
                .evidence
                .iter()
                .map(|e| e.path.as_str())
                .collect::<BTreeSet<_>>(),
            BTreeSet::from(["a.rs", "b.rs"])
        );
        let archive = graph_view_archive::prepare_published(
            &fixture.engine.store,
            &view.id,
            Limits::default(),
            &d(),
        )
        .unwrap();
        assert_eq!(
            archive.view().id,
            view.id,
            "Current guarded published archive must qualify the genuine fixture independently of any new comparison API"
        );
    }
    assert_eq!(
        fs::read_to_string(&fixture.marker).unwrap().lines().count(),
        6
    );
}

#[test]
fn h16_declared_moved_file_pairs_only_that_file_and_keeps_duplicate_descendants_ambiguous() {
    const SOURCE: &str = "fn twin(){sink();}\nfn twin(){sink();}\nfn sink(){}\n";
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("old.rs"), SOURCE).unwrap();
    let mut engine = Engine::open(dir.path()).unwrap();
    engine.capture_epoch = AnalyzerEpoch::V9;
    let options = BuildOptions {
        structural_kinds: vec![],
        ..Default::default()
    };
    let (before_snapshot, graph, _, _) = engine.index(&d()).unwrap();
    let before =
        graph_view::build(&engine.store, &before_snapshot, &graph, &options, &d()).unwrap();
    fs::rename(dir.path().join("old.rs"), dir.path().join("new.rs")).unwrap();
    let (after_snapshot, graph, _, _) = engine.index(&d()).unwrap();
    let after = graph_view::build(&engine.store, &after_snapshot, &graph, &options, &d()).unwrap();
    let before_file = before_snapshot
        .files
        .iter()
        .find(|f| f.path == "old.rs")
        .unwrap();
    let after_file = after_snapshot
        .files
        .iter()
        .find(|f| f.path == "new.rs")
        .unwrap();
    assert_eq!(before_file.hash, after_file.hash);
    let before_file_node = before
        .nodes
        .iter()
        .find(|n| n["kind"] == "file" && n["path"] == "old.rs")
        .unwrap();
    let after_file_node = after
        .nodes
        .iter()
        .find(|n| n["kind"] == "file" && n["path"] == "new.rs")
        .unwrap();
    let options = ComparisonOptions {
        declared_file_continuity: vec![DeclaredFileContinuity {
            before_path: "old.rs".into(),
            before_hash: before_file.hash.clone(),
            after_path: "new.rs".into(),
            after_hash: after_file.hash.clone(),
        }],
        ..Default::default()
    };
    let baseline = fixture_state::state(&engine);
    let result = view_comparison::compare_published(
        &engine.store,
        &SelectionPin::from_view(&before),
        &SelectionPin::from_view(&after),
        &options,
        &d(),
    )
    .unwrap();
    assert_result_contract(&result, &[]);
    let output = result.value();
    assert_eq!(output["renameProofAvailable"], false);
    assert_node_rows(output, &before, "before");
    assert_node_rows(output, &after, "after");
    let file_pairs = rows(&output["source"]["nodeChanges"])
        .iter()
        .filter(|r| {
            r["before"]["originalNodeId"] == before_file_node["id"]
                && r["after"]["originalNodeId"] == after_file_node["id"]
        })
        .collect::<Vec<_>>();
    assert_eq!(
        file_pairs.len(),
        1,
        "Explicit primary association actually pairs file rows with different path/name labels"
    );
    assert_eq!(
        file_pairs[0]["identityBasis"],
        "host_declared_file_continuity_not_proven_rename"
    );
    assert_eq!(file_pairs[0]["verifiedIdentity"], false);
    assert!(file_pairs[0]["correspondenceId"].is_string());
    assert_eq!(rows(&output["declaredFileContinuity"]).len(), 1);
    assert_eq!(
        output["declaredFileContinuity"][0]["beforeGraphViewId"],
        before.id
    );
    assert_eq!(
        output["declaredFileContinuity"][0]["afterGraphViewId"],
        after.id
    );
    assert_eq!(
        output["declaredFileContinuity"][0]["verifiedIdentity"],
        false
    );
    let old_sink = function_id(&before, "sink");
    let new_sink = function_id(&after, "sink");
    assert_ne!(
        old_sink, new_sink,
        "Moved descendant IDs retain their original paths"
    );
    let removed_sink = rows(&output["source"]["nodeChanges"])
        .iter()
        .filter(|r| r["before"]["originalNodeId"] == old_sink)
        .collect::<Vec<_>>();
    let added_sink = rows(&output["source"]["nodeChanges"])
        .iter()
        .filter(|r| r["after"]["originalNodeId"] == new_sink)
        .collect::<Vec<_>>();
    assert_eq!(removed_sink.len(), 1);
    assert_eq!(added_sink.len(), 1);
    assert_eq!(removed_sink[0]["changeKind"], "removed");
    assert!(removed_sink[0]["after"].is_null());
    assert!(removed_sink[0]["correspondenceId"].is_null());
    assert_eq!(added_sink[0]["changeKind"], "added");
    assert!(added_sink[0]["before"].is_null());
    assert!(added_sink[0]["correspondenceId"].is_null());
    let twin_groups = rows(&output["source"]["ambiguousNodes"])
        .iter()
        .filter(|g| {
            rows(&g["beforeRows"]).iter().any(|n| n["name"] == "twin")
                || rows(&g["afterRows"]).iter().any(|n| n["name"] == "twin")
        })
        .collect::<Vec<_>>();
    assert_eq!(twin_groups.len(), 2);
    let old_group = twin_groups
        .iter()
        .find(|g| rows(&g["beforeRows"]).iter().any(|n| n["path"] == "old.rs"))
        .unwrap();
    let new_group = twin_groups
        .iter()
        .find(|g| rows(&g["afterRows"]).iter().any(|n| n["path"] == "new.rs"))
        .unwrap();
    assert_eq!(old_group["beforeCount"], 2);
    assert_eq!(old_group["afterCount"], 0);
    assert!(rows(&old_group["afterRows"]).is_empty());
    assert_eq!(new_group["beforeCount"], 0);
    assert_eq!(new_group["afterCount"], 2);
    assert!(rows(&new_group["beforeRows"]).is_empty());
    assert_ne!(old_group["semanticKey"], new_group["semanticKey"]);
    for (view, side, path) in [(&before, "before", "old.rs"), (&after, "after", "new.rs")] {
        let ambiguous = rows(&output["source"]["ambiguousNodes"])
            .iter()
            .flat_map(|g| rows(&g[format!("{side}Rows")]))
            .filter(|n| n["name"] == "twin")
            .collect::<Vec<_>>();
        assert_eq!(ambiguous.len(), 2);
        assert!(ambiguous.iter().all(|n| n["path"] == path));
        let expected = view
            .nodes
            .iter()
            .filter(|n| n["kind"] == "function" && n["name"] == "twin")
            .map(|n| id(n, "id"))
            .collect::<BTreeSet<_>>();
        assert_eq!(
            ambiguous
                .iter()
                .map(|n| id(n, "originalNodeId"))
                .collect::<BTreeSet<_>>(),
            expected
        );
    }
    for row in rows(&output["source"]["nodeChanges"]) {
        assert_ne!(row["before"]["name"], "twin");
        assert_ne!(
            row["after"]["name"], "twin",
            "Host file continuity must not upgrade duplicate symbols or rewrite their structural key paths"
        );
    }
    assert_eq!(fixture_state::state(&engine), baseline);
}
