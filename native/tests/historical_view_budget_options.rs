//! Caller-supplied comparison budget admission and zero-capacity contracts.
//! Source-only captures provide positive work without a provider or worker.
#[path = "support/historical_fixture_state.rs"]
mod oracle;
#[path = "support/meaning_comparison_fixture.rs"]
mod support;

use serde_json::json;
use yoke::{
    Engine,
    analysis::AnalyzerEpoch,
    graph_view::{self, BuildOptions, View},
    view_comparison::{ComparisonOptions, SelectionPin, compare_published},
};

fn published() -> (tempfile::TempDir, Engine, View) {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("a.rs"), support::A).unwrap();
    let mut engine = Engine::open(root.path()).unwrap();
    engine.capture_epoch = AnalyzerEpoch::V9;
    let (snapshot, graph, _, _) = engine.index(&support::d()).unwrap();
    let view = graph_view::build(
        &engine.store,
        &snapshot,
        &graph,
        &BuildOptions::default(),
        &support::d(),
    )
    .unwrap();
    assert!(!view.nodes.is_empty());
    (root, engine, view)
}

#[test]
fn explicit_budgets_above_defaults_are_admitted_and_reported_without_clamping() {
    let (_root, engine, view) = published();
    let pin = SelectionPin::from_view(&view);
    compare_published(
        &engine.store,
        &pin,
        &pin,
        &ComparisonOptions::default(),
        &support::d(),
    )
    .unwrap();
    let baseline = oracle::state(&engine);
    let options = ComparisonOptions {
        max_result_rows: 10_001,
        max_output_bytes: 4_000_001,
        max_input_bytes: 64_000_001,
        max_source_bytes: 64_000_001,
        ..ComparisonOptions::default()
    };
    let result = compare_published(&engine.store, &pin, &pin, &options, &support::d()).unwrap();
    assert_eq!(
        result.value()["options"],
        json!({
            "maxResultRows":10_001,"maxOutputBytes":4_000_001,
            "maxInputBytes":64_000_001,"maxSourceBytes":64_000_001,
            "declaredFileContinuity":[]
        })
    );
    assert_eq!(result.value()["rowBudget"]["limit"], 10_001);
    assert!(result.value()["rowBudget"]["used"].as_u64().unwrap() > 0);
    assert_eq!(oracle::state(&engine), baseline);
}

#[test]
fn zero_budgets_refuse_positive_work_instead_of_meaning_unlimited() {
    let (_root, engine, view) = published();
    let pin = SelectionPin::from_view(&view);
    let mut requests = vec![];
    for field in 0..4 {
        let mut options = ComparisonOptions::default();
        match field {
            0 => options.max_result_rows = 0,
            1 => options.max_output_bytes = 0,
            2 => options.max_input_bytes = 0,
            3 => options.max_source_bytes = 0,
            _ => unreachable!(),
        }
        requests.push(options);
    }
    for options in requests {
        compare_published(
            &engine.store,
            &pin,
            &pin,
            &ComparisonOptions::default(),
            &support::d(),
        )
        .unwrap();
        let baseline = oracle::state(&engine);
        let error =
            compare_published(&engine.store, &pin, &pin, &options, &support::d()).unwrap_err();
        assert!(
            error.to_string().contains("comparison_capacity_exceeded"),
            "Zero capacity must be refused by the budget contract"
        );
        assert_eq!(oracle::state(&engine), baseline);
    }
}
