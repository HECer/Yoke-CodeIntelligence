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
        changes::load_checked_plan(&engine.store, &plan.id, &Deadline::new(30_000)).unwrap();
    }
}

#[test]
fn rust_outer_attributes_docs_and_intervening_comments_keep_their_own_declaration() {
    checked_after(
        "module.rs",
        "\u{feff}//! retained inner café 🦀\r\n#![allow(dead_code)]\r\n// retained preceding comment\r\n/// selected docs\r\n#[inline]\r\n// selected bundle comment\r\npub fn delete_me() {}\r\n/// retained docs\r\npub const RETAINED: u32 = 2;\r\n",
        "delete_me",
        "\u{feff}//! retained inner café 🦀\r\n#![allow(dead_code)]\r\n// retained preceding comment\r\n\r\n/// retained docs\r\npub const RETAINED: u32 = 2;\r\n",
    );
}

#[test]
fn rust_declaration_kinds_reuse_the_owned_attachment_boundary() {
    for declaration in [
        "fn delete_me() {}",
        "struct delete_me;",
        "enum delete_me { One }",
        "trait delete_me {}",
        "type delete_me = u32;",
        "const delete_me: u32 = 1;",
        "mod delete_me {}",
    ] {
        checked_after(
            "module.rs",
            &format!("#[allow(dead_code)]\n{declaration}\nconst RETAINED: u32 = 2;\n"),
            "delete_me",
            "\nconst RETAINED: u32 = 2;\n",
        );
    }
}

#[test]
fn rust_nested_items_and_trait_or_impl_methods_preserve_the_parent_and_siblings() {
    for (source, expected) in [
        (
            "mod parent { #[inline] fn delete_me() {} fn retained() {} }\n",
            "mod parent {  fn retained() {} }\n",
        ),
        (
            "trait Parent { #[must_use] fn delete_me() -> u32; fn retained(); }\n",
            "trait Parent {  fn retained(); }\n",
        ),
        (
            "struct Parent; impl Parent { #[inline] fn delete_me() {} fn retained() {} }\n",
            "struct Parent; impl Parent {  fn retained() {} }\n",
        ),
        (
            "fn parent() { #[inline] fn delete_me() {} let retained = 1; }\n",
            "fn parent() {  let retained = 1; }\n",
        ),
    ] {
        checked_after("module.rs", source, "delete_me", expected);
    }
}

#[test]
fn python_stacked_decorators_function_class_bom_and_crlf_have_exact_ownership() {
    for declaration in [
        "def delete_me():\r\n    return 1",
        "async def delete_me():\r\n    return 1",
        "class delete_me:\r\n    pass",
    ] {
        for bom in ["", "\u{feff}"] {
            checked_after(
                "module.py",
                &format!(
                    "{bom}# retained café 🦀\r\n@outer(\r\n    '🦀'\r\n)\r\n@inner\r\n{declaration}\r\n\r\ndef retained():\r\n    return 2\r\n"
                ),
                "delete_me",
                &format!("{bom}# retained café 🦀\r\n\r\n\r\ndef retained():\r\n    return 2\r\n"),
            );
        }
    }
}

#[test]
fn python_nested_declarations_leave_existing_body_statements_and_parent_decorators() {
    checked_after(
        "module.py",
        "@parent_decorator\nclass Parent:\n    @selected_decorator\n    def delete_me(self):\n        return 1\n    # retained comment\n    def retained(self):\n        return 2\n",
        "delete_me",
        "@parent_decorator\nclass Parent:\n    \n    # retained comment\n    def retained(self):\n        return 2\n",
    );
    checked_after(
        "module.py",
        "def parent():\n    @selected_decorator\n    def delete_me():\n        return 1\n    pass\n",
        "delete_me",
        "def parent():\n    \n    pass\n",
    );
}

#[test]
fn python_empty_required_bodies_and_malformed_rust_attachments_refuse_without_publication() {
    for (path, source) in [
        (
            "module.py",
            "class Parent:\n    @decorator\n    def delete_me(self):\n        return 1\n    # only a comment remains\n",
        ),
        (
            "module.py",
            "def parent():\n    def delete_me():\n        return 1\n",
        ),
        ("module.rs", "fn delete_me() {}\n#[inline]\n"),
        ("module.rs", "fn delete_me() {}\n/* malformed"),
    ] {
        let root = tempfile::tempdir().unwrap();
        fs::write(root.path().join(path), source).unwrap();
        let mut engine = Engine::open(root.path()).unwrap();
        let (snapshot, graph, _, _) = engine.index(&Deadline::new(30_000)).unwrap();
        let selected = graph
            .entities
            .iter()
            .find(|e| e.name == "delete_me")
            .unwrap();
        assert!(
            changes::prepare(
                &mut engine.store,
                &snapshot,
                &graph,
                &json!({"deleteSymbol":{"entityId":selected.id}}),
                &Deadline::new(30_000)
            )
            .is_err(),
            "{source:?}"
        );
        assert_eq!(engine.store.db.query_row("SELECT COUNT(*) FROM records WHERE kind IN ('plan','approval','receipt','journal')", [], |r| r.get::<_, usize>(0)).unwrap(), 0);
        assert_eq!(fs::read_to_string(root.path().join(path)).unwrap(), source);
    }
}
