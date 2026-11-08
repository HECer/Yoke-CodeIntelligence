//! Independent expected bytes for source-bound native sibling preservation.
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
            &json!({"deleteSymbol":{"entityId":selected.id,"siblingDeclarators":"preserve"}}),
            &d(),
        )
        .unwrap_or_else(|error| panic!("{path} {source:?}: {error:#}"));
        let after = engine.store.snapshot(&plan.result_snapshot_id).unwrap();
        assert_eq!(
            query::source(&engine.store, &after, path).unwrap().0,
            expected,
            "{path} {source:?}"
        );
        assert_eq!(
            query::source(&engine.store, &snapshot, path).unwrap().0,
            source
        );
        assert_eq!(fs::read(root.path().join(path)).unwrap(), source.as_bytes());
        assert_eq!(serde_json::to_value(selected).unwrap(), original);
        assert!(
            engine
                .store
                .graph(&after.id)
                .unwrap()
                .diagnostics
                .is_empty()
        );
        assert_eq!(
            changes::load_checked_plan(&engine.store, &plan.id, &d())
                .unwrap()
                .diff,
            plan.diff
        );
    }
}
#[test]
fn first_middle_and_last_declarators_keep_all_unrelated_bytes() {
    for path in ["module.ts", "module.js", "module.tsx", "module.jsx"] {
        for keyword in ["const", "let", "var"] {
            for (source, expected) in [
                (
                    "deleteMe = () => 1, retained = () => 2",
                    " retained = () => 2",
                ),
                (
                    "first = () => 0, deleteMe = () => 1, last = () => 2",
                    "first = () => 0,  last = () => 2",
                ),
                (
                    "retained = () => 2, deleteMe = () => 1",
                    "retained = () => 2 ",
                ),
            ] {
                checked(
                    path,
                    &format!("\u{feff}// café 🦀\r\nexport {keyword} {source};\r\n"),
                    &format!("\u{feff}// café 🦀\r\nexport {keyword} {expected};\r\n"),
                );
            }
        }
    }
}
#[test]
fn nested_commas_and_owned_initializer_comments_are_not_separators() {
    checked(
        "module.ts",
        "export const deleteMe = (left: number, right: number) => { /* owned */ return [left,right]; }, kept = (value: number) => [value,2];\n",
        "export const  kept = (value: number) => [value,2];\n",
    );
    checked(
        "module.js",
        "function parent() { const kept = () => ',🦀,', deleteMe = function() { return [1,2]; }; return kept; }\n",
        "function parent() { const kept = () => ',🦀,' ; return kept; }\n",
    );
    checked("module.ts", "export const deleteMe = () => 1;\n", "\n");
}
