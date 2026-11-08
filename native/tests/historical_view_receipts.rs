//! Historical Meaning receipt reuse and exact original report provenance tests.
//! Owned local Node workers construct the fixtures before comparison calls.
//! Receipt metadata, unknown values and independent claim namespaces stay distinct.
#[path = "support/historical_fixture_state.rs"]
mod fixture_state;
#[path = "support/historical_schema_oracle.rs"]
#[allow(dead_code)] // The shared semantics target also uses the anchor assertion helper.
mod schema;
#[path = "support/meaning_receipt_fixture.rs"]
mod support;

use serde_json::{Value, json};
use std::fs;
use yoke::{
    graph_view::View,
    graph_view_archive::{self as archive, Limits},
    model::identity,
    view_comparison::{self, Comparison, ComparisonOptions, SelectionPin},
};

// New comparison digests only. Keep complete original Values until after
// this recursive object ordering; array order/null/absence remain unchanged.
fn canonical_value_digest(mut value: Value) -> String {
    value.sort_all_objects();
    identity(&value)
}

fn rows(value: &Value) -> &[Value] {
    value.as_array().expect("closed result array")
}
fn assert_contract(comparison: &Comparison) {
    schema::validate(comparison.value(), support::SENTINELS).unwrap();
    assert_eq!(
        comparison.canonical_bytes(),
        serde_json::to_vec(comparison.value()).unwrap()
    );
    let mut body = comparison.value().clone();
    body.as_object_mut()
        .unwrap()
        .remove("comparisonId")
        .unwrap();
    assert_eq!(
        comparison.value()["comparisonId"],
        canonical_value_digest(body)
    );
}

fn assert_receipt(row: &Value, view: &View, side: &str) {
    let layer = view.meaning.as_ref().unwrap();
    assert_eq!(layer.packets.len(), 1);
    let packet = &layer.packets[0];
    let original = serde_json::to_value(&packet.provenance).unwrap();
    assert_eq!(row["side"], side);
    assert_eq!(
        row["originalRowDigest"],
        canonical_value_digest(original.clone()),
        "Digest the FULL original ReceiptProvenance only, before adding descriptors, bindings or coverage"
    );
    assert_eq!(row["digestMethod"], "sha256_canonical_original_json_v1");
    for (key, value) in original.as_object().unwrap() {
        assert_eq!(&row[key], value, "Original receipt field {key}");
    }
    assert_eq!(row["selectionIndex"], packet.selection_index);
    assert_eq!(row["packetIndex"], packet.index);
    assert_eq!(row["graphId"], layer.selections[0].graph_id);
    assert_eq!(row["jobId"], layer.selections[0].job_id);
    assert_eq!(
        row["coverage"],
        serde_json::to_value(&packet.coverage).unwrap()
    );
    assert_eq!(row["textOmitted"], true);
    assert_eq!(rows(&row["sourceBindings"]).len(), 1);
    let binding = serde_json::to_value(&packet.source_bindings[0]).unwrap();
    let redacted = &row["sourceBindings"][0];
    assert_eq!(
        redacted["originalRowDigest"],
        canonical_value_digest(binding.clone()),
        "Binding digest is independent of ReceiptProvenance"
    );
    assert_eq!(
        redacted["digestMethod"],
        "sha256_canonical_original_json_v1"
    );
    assert_eq!(redacted["textOmitted"], true);
    for (key, value) in binding.as_object().unwrap() {
        assert_eq!(&redacted[key], value, "Original unit binding {key}");
    }
    for invented in ["snapshotId", "anchor", "startLine", "endLine"] {
        assert!(redacted.get(invented).is_none());
    }
}

fn assert_pair(
    comparison: &Comparison,
    before: &View,
    after: &View,
    unknown_cost: usize,
    unknown_usage: usize,
    receipt_reused: bool,
    literal_claims_per_side: usize,
) {
    assert_contract(comparison);
    let output = comparison.value();
    for (side, view) in [("before", before), ("after", after)] {
        assert_eq!(
            output[side]["meaningCapturePolicyHash"],
            view.meaning.as_ref().unwrap().scope.capture_policy_hash,
            "Keep original source authority hash separate from comparison policy"
        );
        let actual = rows(&output["meaning"]["provenance"][format!("{side}Rows")]);
        assert_eq!(
            actual.len(),
            1,
            "No-claims packets still retain original receipts and unit coverage"
        );
        assert_receipt(&actual[0], view, side);
        assert_eq!(
            output["meaning"]["provenance"][format!("{side}UnknownCostPackets")],
            unknown_cost
        );
        assert_eq!(
            output["meaning"]["provenance"][format!("{side}UnknownUsagePackets")],
            unknown_usage
        );
    }
    let reuse = rows(&output["meaning"]["receiptReuse"]);
    assert_eq!(reuse.len(), usize::from(receipt_reused));
    if receipt_reused {
        let original = &before.meaning.as_ref().unwrap().packets[0].provenance;
        for (key, value) in [
            ("originalJobId", &original.original_job_id),
            ("responseArtifactId", &original.response_artifact_id),
            ("attemptId", &original.attempt_id),
            ("requestId", &original.request_id),
        ] {
            assert_eq!(reuse[0][key], *value);
        }
        assert_eq!(
            reuse[0]["provenance"],
            "original_receipt_reuse_not_claim_identity"
        );
        for (side, view) in [("before", before), ("after", after)] {
            let copies = rows(&reuse[0][format!("{side}Rows")]);
            assert_eq!(copies.len(), 1);
            assert_receipt(&copies[0], view, side);
            assert_eq!(
                copies[0],
                output["meaning"]["provenance"][format!("{side}Rows")][0]
            );
        }
    }
    assert!(
        rows(&output["meaning"]["sharedClaimIds"]).is_empty(),
        "Receipt reuse never proves selected claim identity"
    );
    assert_eq!(
        rows(&output["meaning"]["removedClaimIds"]).len(),
        literal_claims_per_side
    );
    assert_eq!(
        rows(&output["meaning"]["addedClaimIds"]).len(),
        literal_claims_per_side
    );
    assert_eq!(
        rows(&output["meaning"]["claimChanges"]).len(),
        2 * literal_claims_per_side
    );
    for (side, view, kind) in [("before", before, "removed"), ("after", after, "added")] {
        let claims = &view.meaning.as_ref().unwrap().packets[0].concepts;
        assert_eq!(claims.len(), literal_claims_per_side);
        if let Some(claim) = claims.first() {
            assert_eq!(
                output["meaning"][format!(
                    "{}ClaimIds",
                    if side == "before" { "removed" } else { "added" }
                )],
                json!([claim.id])
            );
            let changes = rows(&output["meaning"]["claimChanges"])
                .iter()
                .filter(|row| row["changeKind"] == kind)
                .collect::<Vec<_>>();
            assert_eq!(changes.len(), 1);
            assert_eq!(changes[0][side]["originalClaimId"], claim.id);
            assert_eq!(changes[0][side]["materializedClaimId"], claim.claim_id);
            assert_eq!(
                changes[0][side]["originalRowDigest"],
                canonical_value_digest(serde_json::to_value(claim).unwrap())
            );
            assert_eq!(changes[0]["verifiedIdentity"], false);
        }
    }
}

#[test]
fn h11_published_comparison_preserves_null_zero_independent_usage_and_receipt_reuse() {
    // Break caught: fabricating zero for unknown, combining independent fields,
    // hashing a projected packet instead of provenance, or identifying claims
    // by a reused original receipt/materialized claim ID.
    let fixture = support::fixture();
    let baseline = fixture_state::state(&fixture.engine);
    let calls = fs::read(&fixture.marker).unwrap();
    for (variant, unknown_cost, unknown_usage, claims) in [
        (support::Variant::Known, 0, 0, 1),
        (support::Variant::Zero, 0, 0, 1),
        (support::Variant::Unknown, 1, 1, 1),
        (support::Variant::CostOnly, 0, 1, 1),
        (support::Variant::UsageOnly, 1, 0, 1),
        (support::Variant::NoClaims, 1, 1, 0),
    ] {
        let case = fixture.case(variant);
        support::assert_original_inputs(&fixture, case);
        let result = view_comparison::compare_published(
            &fixture.engine.store,
            &SelectionPin::from_view(&case.original),
            &SelectionPin::from_view(&case.rebound),
            &ComparisonOptions::default(),
            &support::d(),
        )
        .unwrap();
        assert_pair(
            &result,
            &case.original,
            &case.rebound,
            unknown_cost,
            unknown_usage,
            true,
            claims,
        );
        support::assert_original_inputs(&fixture, case);
        assert_eq!(fixture_state::state(&fixture.engine), baseline);
        assert_eq!(fs::read(&fixture.marker).unwrap(), calls);
    }
}

#[test]
fn h11_different_currencies_and_versions_remain_original_nonaggregated_receipts() {
    // Break caught: summing money across currencies, copying one side's version,
    // or matching equal source-backed materialized concepts across generations.
    let fixture = support::fixture();
    let before = &fixture.case(support::Variant::Known).original;
    let after = &fixture.case(support::Variant::Zero).original;
    assert_eq!(
        before.meaning.as_ref().unwrap().packets[0].concepts[0].claim_id,
        after.meaning.as_ref().unwrap().packets[0].concepts[0].claim_id
    );
    assert_ne!(
        before.meaning.as_ref().unwrap().packets[0].concepts[0].id,
        after.meaning.as_ref().unwrap().packets[0].concepts[0].id
    );
    let baseline = fixture_state::state(&fixture.engine);
    let calls = fs::read(&fixture.marker).unwrap();
    let result = view_comparison::compare_published(
        &fixture.engine.store,
        &SelectionPin::from_view(before),
        &SelectionPin::from_view(after),
        &ComparisonOptions::default(),
        &support::d(),
    )
    .unwrap();
    assert_pair(&result, before, after, 0, 0, false, 1);
    let provenance = &result.value()["meaning"]["provenance"];
    assert_eq!(provenance["beforeRows"][0]["currency"], "EUR");
    assert_eq!(provenance["beforeRows"][0]["providerReportedCostMicros"], 7);
    assert_eq!(provenance["beforeRows"][0]["modelVersion"], "h11-v1");
    assert_eq!(provenance["afterRows"][0]["currency"], "USD");
    assert_eq!(provenance["afterRows"][0]["providerReportedCostMicros"], 0);
    assert_eq!(provenance["afterRows"][0]["modelVersion"], "h11-v2");
    for case in &fixture.cases {
        support::assert_original_inputs(&fixture, case);
    }
    assert_eq!(fixture_state::state(&fixture.engine), baseline);
    assert_eq!(fs::read(&fixture.marker).unwrap(), calls);
}

#[test]
fn h11_checked_archives_keep_original_reports_after_later_ledger_reconciliation_and_store_removal()
{
    // Break caught: deriving receipt values from current ledger totals, hidden
    // original workspace/provider reads, or changing source policy during replay.
    let fixture = support::fixture();
    support::add_later_host_accounting(&fixture);
    let baseline = fixture_state::state(&fixture.engine);
    let calls = fs::read(&fixture.marker).unwrap();
    let marker = fixture.marker.clone();
    let mut pairs = Vec::new();
    for (variant, unknown_cost, unknown_usage, claims) in [
        (support::Variant::Known, 0, 0, 1),
        (support::Variant::Zero, 0, 0, 1),
        (support::Variant::Unknown, 1, 1, 1),
        (support::Variant::CostOnly, 0, 1, 1),
        (support::Variant::UsageOnly, 1, 0, 1),
        (support::Variant::NoClaims, 1, 1, 0),
    ] {
        let case = fixture.case(variant);
        support::assert_original_inputs(&fixture, case);
        let left = archive::prepare_published(
            &fixture.engine.store,
            &case.original.id,
            Limits::default(),
            &support::d(),
        )
        .unwrap();
        let right = archive::prepare_published(
            &fixture.engine.store,
            &case.rebound.id,
            Limits::default(),
            &support::d(),
        )
        .unwrap();
        pairs.push((
            left.bytes().to_vec(),
            right.bytes().to_vec(),
            case.original.clone(),
            case.rebound.clone(),
            unknown_cost,
            unknown_usage,
            claims,
        ));
    }
    assert_eq!(fixture_state::state(&fixture.engine), baseline);
    assert_eq!(fs::read(&marker).unwrap(), calls);
    let root = fixture.engine.root.clone();
    let _retained = support::remove_store_and_workspace(fixture);
    assert!(!root.exists());
    for (left, right, before, after, unknown_cost, unknown_usage, claims) in pairs {
        let left = archive::parse(&left, Limits::default(), &support::d()).unwrap();
        let right = archive::parse(&right, Limits::default(), &support::d()).unwrap();
        let result = view_comparison::compare_meaning_archives(
            &left,
            &right,
            &ComparisonOptions::default(),
            &support::d(),
        )
        .unwrap();
        assert_pair(
            &result,
            &before,
            &after,
            unknown_cost,
            unknown_usage,
            true,
            claims,
        );
    }
    assert_eq!(fs::read(marker).unwrap(), calls);
}

#[test]
fn h11_new_digest_oracle_sorts_nested_objects_without_reordering_arrays_or_erasing_nulls() {
    // Break caught: hashing insertion order, sorting arrays, dropping nulls,
    // or returning a single constant for all complete original Values.
    for (literal, expected) in [
        (
            r#"{"z":{"b":null,"a":0},"a":[{"y":2,"x":1},{"x":3}]}"#,
            "1537cd4c25f6ab5f51de90e8e54a7133d39400406691c2085e0497049b416758",
        ),
        (
            r#"{"a":[{"x":1,"y":2},{"x":3}],"z":{"a":0,"b":null}}"#,
            "1537cd4c25f6ab5f51de90e8e54a7133d39400406691c2085e0497049b416758",
        ),
        (
            r#"{"a":[{"x":3},{"x":1,"y":2}],"z":{"a":0,"b":null}}"#,
            "7bcdbe9a7b3ff6596324d9d7d3b04b220e90b7cf63835e5788e048ad0ad436dd",
        ),
        (
            r#"{"a":[{"x":1,"y":2},{"x":3}],"z":{"a":0}}"#,
            "3d4bdc8be849fbd073feaf901cfdd6b5c9741d4a98f393570e45ada6122a4338",
        ),
    ] {
        let complete: Value = serde_json::from_str(literal).unwrap();
        assert_eq!(canonical_value_digest(complete), expected);
    }
}
