//! Exact source effects for concrete class methods with owning decorators.
use serde_json::json;
use std::fs;
use yoke::{Engine, analysis::AnalyzerEpoch, capture::Deadline, changes, query};
fn d() -> Deadline {
    Deadline::new(30_000)
}
fn checked(path: &str, source: &str, expected: &str) {
    for epoch in [AnalyzerEpoch::V8, AnalyzerEpoch::V9] {
        let root = tempfile::tempdir().unwrap();
        fs::write(root.path().join(path), source).unwrap();
        let mut engine = Engine::open(root.path()).unwrap();
        engine.capture_epoch = epoch;
        let (snapshot, graph, _, _) = engine.index(&d()).unwrap();
        let selected = graph
            .entities
            .iter()
            .find(|e| e.name == "deleteMe")
            .unwrap();
        let original = serde_json::to_value(selected).unwrap();
        let plan = changes::prepare(
            &mut engine.store,
            &snapshot,
            &graph,
            &json!({"deleteSymbol":{"entityId":selected.id}}),
            &d(),
        )
        .unwrap_or_else(|error| panic!("{path} {source:?}: {error:#}"));
        let after = engine.store.snapshot(&plan.result_snapshot_id).unwrap();
        assert_eq!(
            query::source(&engine.store, &after, path).unwrap().0,
            expected
        );
        assert_eq!(
            query::source(&engine.store, &snapshot, path).unwrap().0,
            source
        );
        assert_eq!(serde_json::to_value(selected).unwrap(), original);
        assert_eq!(fs::read(root.path().join(path)).unwrap(), source.as_bytes());
        assert_eq!(
            changes::load_checked_plan(&engine.store, &plan.id, &d())
                .unwrap()
                .diff,
            plan.diff
        );
    }
}
#[test]
fn concrete_method_decorators_belong_to_the_selected_member_not_its_neighbor() {
    checked(
        "module.ts",
        "\u{feff}// café 🦀\r\n@parent\r\nclass Parent {\r\n    // retained preceding\r\n    @first('🦀')\r\n    // owned bundle comment\r\n    @second\r\n    deleteMe() {}\r\n    // retained neighbor\r\n    kept() {}\r\n}\r\n",
        "\u{feff}// café 🦀\r\n@parent\r\nclass Parent {\r\n    // retained preceding\r\n    \r\n    // retained neighbor\r\n    kept() {}\r\n}\r\n",
    );
}
