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

#[test]
fn concrete_method_variants_and_four_languages_keep_their_original_entity_coordinates() {
    for path in ["module.ts", "module.tsx", "module.js", "module.jsx"] {
        for method in [
            "deleteMe() {}",
            "async deleteMe() {}",
            "*deleteMe() { yield 1; }",
            "static deleteMe() {}",
            "get deleteMe() { return 1; }",
            "set deleteMe(value) {}",
        ] {
            checked(
                path,
                &format!(
                    "class Parent {{\n    @first\n    @second\n    {method}\n    kept() {{}}\n}}\n"
                ),
                "class Parent {\n    \n    kept() {}\n}\n",
            );
        }
    }
}
#[test]
fn decorator_chains_on_parent_previous_and_next_members_stay_separate() {
    checked(
        "module.ts",
        "@parent\nclass Parent {\n    @previous\n    before() {}\n    // retained before selected bundle\n    @selected\n    deleteMe() {}\n    @next\n    kept() {}\n}\n",
        "@parent\nclass Parent {\n    @previous\n    before() {}\n    // retained before selected bundle\n    \n    @next\n    kept() {}\n}\n",
    );
    checked(
        "module.ts",
        "class Parent {\n    // retained old comment\n    ; deleteMe() {}\n    kept() {}\n}\n",
        "class Parent {\n    // retained old comment\n    ; \n    kept() {}\n}\n",
    );
}
#[test]
fn decorator_capacity_admits64_and_refuses65_without_a_plan() {
    let admitted = format!(
        "class Parent {{\n{}    deleteMe() {{}}\n}}\n",
        "    @decorator\n".repeat(64)
    );
    checked("module.ts", &admitted, "class Parent {\n    \n}\n");
    let root = tempfile::tempdir().unwrap();
    let refused = format!(
        "class Parent {{\n{}    deleteMe() {{}}\n}}\n",
        "    @decorator\n".repeat(65)
    );
    fs::write(root.path().join("module.ts"), &refused).unwrap();
    let mut engine = Engine::open(root.path()).unwrap();
    let (snapshot, graph, _, _) = engine.index(&d()).unwrap();
    let selected = graph
        .entities
        .iter()
        .find(|e| e.name == "deleteMe")
        .unwrap();
    assert!(
        changes::prepare(
            &mut engine.store,
            &snapshot,
            &graph,
            &json!({"deleteSymbol":{"entityId":selected.id}}),
            &d()
        )
        .is_err()
    );
    assert_eq!(
        engine
            .store
            .db
            .query_row("SELECT count(*) FROM records WHERE kind='plan'", [], |r| {
                r.get::<_, usize>(0)
            })
            .unwrap(),
        0
    );
    assert_eq!(
        fs::read(root.path().join("module.ts")).unwrap(),
        refused.as_bytes()
    );
}
#[test]
fn malformed_gaps_and_nonconcrete_or_nonsimple_decorated_members_refuse() {
    for source in [
        "class Parent { @first; deleteMe() {} }\n",
        "abstract class Parent { @first abstract deleteMe(): void; }\n",
        "class Parent { @first [deleteMe]() {} }\n",
        "class Parent { @first 'deleteMe'() {} }\n",
        "class Parent { @first #deleteMe() {} }\n",
        "class Parent { @first deleteMe() {} }\n/* malformed",
    ] {
        let root = tempfile::tempdir().unwrap();
        fs::write(root.path().join("module.ts"), source).unwrap();
        let mut engine = Engine::open(root.path()).unwrap();
        let (snapshot, graph, _, _) = engine.index(&d()).unwrap();
        let selected = graph
            .entities
            .iter()
            .find(|e| e.kind == "method" && e.name.contains("deleteMe"))
            .unwrap();
        assert!(
            changes::prepare(
                &mut engine.store,
                &snapshot,
                &graph,
                &json!({"deleteSymbol":{"entityId":selected.id}}),
                &d()
            )
            .is_err(),
            "{source:?}"
        );
        assert_eq!(
            engine
                .store
                .db
                .query_row("SELECT count(*) FROM records WHERE kind='plan'", [], |r| {
                    r.get::<_, usize>(0)
                })
                .unwrap(),
            0
        );
        assert_eq!(
            fs::read(root.path().join("module.ts")).unwrap(),
            source.as_bytes()
        );
    }
}
#[test]
fn replacing_a_method_keeps_its_original_range_and_existing_decorators() {
    let root = tempfile::tempdir().unwrap();
    let source = "class Parent { @decorator deleteMe() {} kept() {} }\n";
    fs::write(root.path().join("module.ts"), source).unwrap();
    let mut engine = Engine::open(root.path()).unwrap();
    let (snapshot, graph, _, _) = engine.index(&d()).unwrap();
    let selected = graph
        .entities
        .iter()
        .find(|e| e.name == "deleteMe")
        .unwrap();
    let plan = changes::prepare(
        &mut engine.store,
        &snapshot,
        &graph,
        &json!({"replaceSymbol":{"entityId":selected.id,"text":"deleteMe() { return 2; }"}}),
        &d(),
    )
    .unwrap();
    let after = engine.store.snapshot(&plan.result_snapshot_id).unwrap();
    assert_eq!(
        query::source(&engine.store, &after, "module.ts").unwrap().0,
        "class Parent { @decorator deleteMe() { return 2; } kept() {} }\n"
    );
    assert_eq!(
        fs::read(root.path().join("module.ts")).unwrap(),
        source.as_bytes()
    );
}
