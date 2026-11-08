use serde_json::json;
use std::fs;
use yoke::{Engine, analysis::AnalyzerEpoch, capture::Deadline, changes, query};

fn checked_after(path: &str, source: &str, name: &str, expected: &str) {
    for epoch in [AnalyzerEpoch::V8, AnalyzerEpoch::V9] {
        let root = tempfile::tempdir().unwrap();
        fs::write(root.path().join(path), source).unwrap();
        let mut engine = Engine::open(root.path()).unwrap();
        engine.capture_epoch = epoch;
        let (snapshot, graph, _, _) = engine.index(&Deadline::new(30_000)).unwrap();
        let selected = graph.entities.iter().find(|e| e.name == name).unwrap();
        let original = serde_json::to_value(selected).unwrap();
        let plan = changes::prepare(
            &mut engine.store,
            &snapshot,
            &graph,
            &json!({"deleteSymbol":{"entityId":selected.id}}),
            &Deadline::new(30_000),
        )
        .unwrap_or_else(|error| panic!("{path} {source:?}: {error:#}"));
        let derived = engine.store.snapshot(&plan.result_snapshot_id).unwrap();
        assert_eq!(
            query::source(&engine.store, &derived, path).unwrap().0,
            expected,
            "{source:?}"
        );
        assert_eq!(
            query::source(&engine.store, &snapshot, path).unwrap().0,
            source
        );
        assert_eq!(serde_json::to_value(selected).unwrap(), original);
        assert_eq!(fs::read(root.path().join(path)).unwrap(), source.as_bytes());
        assert_eq!(
            changes::load_checked_plan(&engine.store, &plan.id, &Deadline::new(30_000))
                .unwrap()
                .diff,
            plan.diff
        );
    }
}

#[test]
fn exported_functions_and_default_functions_preserve_unicode_crlf_and_neighbor_bytes() {
    for path in ["module.ts", "module.js", "module.tsx", "module.jsx"] {
        for export in [
            "export",
            "export default",
            "export /* owned wrapper comment */",
        ] {
            let source = format!(
                "\u{feff}// café 🦀\r\n{export} function deleteMe() {{ return '🦀'; }}\r\nexport const retained = 2;\r\n"
            );
            checked_after(
                path,
                &source,
                "deleteMe",
                "\u{feff}// café 🦀\r\n\r\nexport const retained = 2;\r\n",
            );
        }
    }
}

#[test]
fn declaration_ownership_handles_generators_classes_types_and_single_function_variables() {
    for declaration in [
        "export function* deleteMe() { yield 1; }",
        "export class deleteMe {}",
        "export default class deleteMe {}",
        "export interface deleteMe { kept: number; }",
        "export type deleteMe = number;",
        "export enum deleteMe { One }",
        "export const deleteMe = () => 1;",
        "const deleteMe = function () { return 1; };",
        "export declare class deleteMe {}",
        "declare class deleteMe {}",
    ] {
        checked_after(
            "module.ts",
            &format!("{declaration}\nexport const retained = 2;\n"),
            "deleteMe",
            "\nexport const retained = 2;\n",
        );
    }
}

#[test]
fn nested_declarations_do_not_consume_their_parent_or_adjacent_comments() {
    checked_after(
        "module.js",
        "function parent() {\n// retain this comment\nfunction deleteMe() {}\nreturn 7;\n}\n",
        "deleteMe",
        "function parent() {\n// retain this comment\n\nreturn 7;\n}\n",
    );
    checked_after(
        "module.ts",
        "export class Parent { deleteMe() {} retained() {} }\n",
        "deleteMe",
        "export class Parent {  retained() {} }\n",
    );
}

#[test]
fn multi_declarators_named_expressions_object_methods_and_loop_bindings_refuse_before_plan() {
    for source in [
        "export const deleteMe = () => 1, retained = () => 2;\n",
        "const retained = function deleteMe() {};\n",
        "const retained = { deleteMe() {}, other() {} };\n",
        "for (let deleteMe = () => 1; false;) {}\n",
        "class Parent { @decorate deleteMe() {} retained() {} }\n",
        "export function deleteMe() {}\n/* malformed",
    ] {
        let root = tempfile::tempdir().unwrap();
        fs::write(root.path().join("module.ts"), source).unwrap();
        let mut engine = Engine::open(root.path()).unwrap();
        let (snapshot, graph, _, _) = engine.index(&Deadline::new(30_000)).unwrap();
        let selected = graph
            .entities
            .iter()
            .find(|e| e.name == "deleteMe")
            .unwrap();
        let result = changes::prepare(
            &mut engine.store,
            &snapshot,
            &graph,
            &json!({"deleteSymbol":{"entityId":selected.id}}),
            &Deadline::new(30_000),
        );
        assert!(result.is_err(), "{source:?}");
        let count: usize = engine
            .store
            .db
            .query_row(
                "SELECT COUNT(*) FROM records WHERE kind='plan'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 0);
        assert_eq!(
            fs::read_to_string(root.path().join("module.ts")).unwrap(),
            source
        );
    }
}

#[test]
fn deletion_spans_do_not_change_insert_or_replace_entity_coordinates() {
    let source = "export function deleteMe() {}\n";
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("module.ts"), source).unwrap();
    let mut engine = Engine::open(root.path()).unwrap();
    let (snapshot, graph, _, _) = engine.index(&Deadline::new(30_000)).unwrap();
    let selected = graph
        .entities
        .iter()
        .find(|e| e.name == "deleteMe")
        .unwrap();
    for (operation, expected) in [
        ("replaceSymbol", "export replacement\n"),
        ("insertBefore", "export replacementfunction deleteMe() {}\n"),
        ("insertAfter", "export function deleteMe() {}replacement\n"),
    ] {
        let plan = changes::prepare(
            &mut engine.store,
            &snapshot,
            &graph,
            &json!({(operation):{"entityId":selected.id,"text":"replacement"}}),
            &Deadline::new(30_000),
        )
        .unwrap();
        let derived = engine.store.snapshot(&plan.result_snapshot_id).unwrap();
        assert_eq!(
            query::source(&engine.store, &derived, "module.ts")
                .unwrap()
                .0,
            expected
        );
    }
}
