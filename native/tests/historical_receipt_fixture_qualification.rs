//! Existing-API receipt-fixture qualification and portable archive replay tests.
//! Explicit fixture construction uses an owned local Node worker.
//! Later accounting and storeless replay retain the original receipt reports.
#[path = "support/historical_fixture_state.rs"]
mod fixture_state;
#[path = "support/meaning_receipt_fixture.rs"]
mod support;

use serde_json::{Value, json};
use std::fs;
use yoke::{
    graph_view,
    graph_view_archive::{self as archive, Limits},
    model::hash,
};

#[test]
fn h11_owned_worker_publishes_literal_reports_and_reuses_only_original_receipts() {
    // Break caught: null/zero collapse, cost-derived usage, version/currency
    // rewriting, changed provider policy, or a fresh dispatch during rebound.
    let fixture = support::fixture();
    assert_ne!(fixture.original_snapshot.id, fixture.rebound_snapshot.id);
    for (variant, cost, usage, currency, version, claims) in [
        (
            support::Variant::Known,
            json!(7),
            json!({"inputTokens":101,"outputTokens":13,"cachedInputTokens":17}),
            "EUR",
            "h11-v1",
            1,
        ),
        (
            support::Variant::Zero,
            json!(0),
            json!({"inputTokens":0,"outputTokens":0,"cachedInputTokens":0}),
            "USD",
            "h11-v2",
            1,
        ),
        (
            support::Variant::Unknown,
            Value::Null,
            Value::Null,
            "USD",
            "h11-v3",
            1,
        ),
        (
            support::Variant::CostOnly,
            json!(5),
            Value::Null,
            "EUR",
            "h11-v4",
            1,
        ),
        (
            support::Variant::UsageOnly,
            Value::Null,
            json!({"inputTokens":11,"outputTokens":3,"cachedInputTokens":0}),
            "EUR",
            "h11-v5",
            1,
        ),
        (
            support::Variant::NoClaims,
            Value::Null,
            Value::Null,
            "USD",
            "h11-v6",
            0,
        ),
    ] {
        let case = fixture.case(variant);
        support::assert_original_inputs(&fixture, case);
        for view in [&case.original, &case.rebound] {
            let layer = view.meaning.as_ref().unwrap();
            let packet = &layer.packets[0];
            let provenance = serde_json::to_value(&packet.provenance).unwrap();
            assert_eq!(provenance["providerReportedCostMicros"], cost);
            assert_eq!(provenance["providerReportedUsage"], usage);
            assert_eq!(provenance["currency"], currency);
            assert_eq!(provenance["modelVersion"], version);
            assert_eq!(packet.concepts.len(), claims);
            assert!(packet.relations.is_empty() && packet.hyperedges.is_empty());
            assert_eq!(packet.coverage.len(), 1);
            assert_eq!(packet.source_bindings.len(), 1);
            assert_eq!(packet.source_bindings[0].path, "claim.rs");
            assert_eq!(
                packet.source_bindings[0].content_hash,
                hash(support::SOURCE)
            );
            assert_eq!(packet.source_bindings[0].start_byte, 0);
            assert_eq!(packet.source_bindings[0].end_byte, support::SOURCE.len());
        }
    }
    fixture_state::calibrate_foreign_connection_mutation_oracle(&fixture.engine);
    let baseline = fixture_state::state(&fixture.engine);
    let calls = fs::read(&fixture.marker).unwrap();
    for case in &fixture.cases {
        for expected in [&case.original, &case.rebound] {
            let actual =
                graph_view::load(&fixture.engine.store, &expected.id, &support::d()).unwrap();
            assert_eq!(
                serde_json::to_value(actual).unwrap(),
                serde_json::to_value(expected).unwrap()
            );
        }
    }
    assert_eq!(fixture_state::state(&fixture.engine), baseline);
    assert_eq!(fs::read(&fixture.marker).unwrap(), calls);
    assert_eq!(support::requests(&fixture).len(), 6);
}

#[test]
fn h11_owned_archives_replay_original_reports_after_later_host_accounting_and_store_removal() {
    // Break caught: replacing immutable unknown reports with later ledger
    // reconciliation, or a portable read requiring a worker/live source Store.
    let fixture = support::fixture();
    support::add_later_host_accounting(&fixture);
    let baseline = fixture_state::state(&fixture.engine);
    let calls = fs::read(&fixture.marker).unwrap();
    let marker = fixture.marker.clone();
    let mut transports = Vec::new();
    for case in &fixture.cases {
        support::assert_original_inputs(&fixture, case);
        for view in [&case.original, &case.rebound] {
            let prepared = archive::prepare_published(
                &fixture.engine.store,
                &view.id,
                Limits::default(),
                &support::d(),
            )
            .unwrap();
            assert_eq!(
                serde_json::to_value(prepared.view()).unwrap(),
                serde_json::to_value(view).unwrap()
            );
            let wire: Value = serde_json::from_slice(prepared.bytes()).unwrap();
            assert_eq!(
                wire["sourceObjects"][hash(support::SOURCE)],
                support::SOURCE
            );
            if view.snapshot_id == fixture.rebound_snapshot.id {
                assert_eq!(
                    wire["sourceObjects"][hash(support::HISTORY_OLD)],
                    support::HISTORY_OLD
                );
                assert_eq!(
                    wire["sourceObjects"][hash(support::HISTORY_NEW)],
                    support::HISTORY_NEW
                );
            }
            transports.push((
                prepared.bytes().to_vec(),
                serde_json::to_value(view).unwrap(),
            ));
        }
    }
    assert_eq!(fixture_state::state(&fixture.engine), baseline);
    assert_eq!(fs::read(&marker).unwrap(), calls);
    let root = fixture.engine.root.clone();
    let _retained = support::remove_store_and_workspace(fixture);
    assert!(!root.exists());
    assert_eq!(transports.len(), 12);
    for (bytes, expected) in transports {
        let checked = archive::parse(&bytes, Limits::default(), &support::d()).unwrap();
        assert_eq!(checked.archive_id(), hash(&bytes));
        assert_eq!(serde_json::to_value(checked.view()).unwrap(), expected);
    }
    assert_eq!(fs::read(marker).unwrap(), calls);
}
