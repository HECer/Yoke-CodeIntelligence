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

#[test]
fn default_and_explicit_reject_keep_the_original_multi_declaration_refusal() {
    let root = tempfile::tempdir().unwrap();
    let source = "export const deleteMe = () => 1, kept = () => 2;\n";
    fs::write(root.path().join("module.ts"), source).unwrap();
    let mut engine = Engine::open(root.path()).unwrap();
    let (snapshot, graph, _, _) = engine.index(&d()).unwrap();
    let selected = graph
        .entities
        .iter()
        .find(|e| e.name == "deleteMe")
        .unwrap();
    for operation in [
        json!({"entityId":selected.id}),
        json!({"entityId":selected.id,"siblingDeclarators":"reject"}),
    ] {
        assert!(
            changes::prepare(
                &mut engine.store,
                &snapshot,
                &graph,
                &json!({"deleteSymbol":operation}),
                &d()
            )
            .is_err()
        );
    }
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
#[test]
fn ambiguous_comment_attachments_loop_bindings_and_named_expressions_refuse_without_publication() {
    for source in [
        "export const deleteMe = () => 1, /* unowned */ kept = () => 2;\n",
        "export const deleteMe = () => 1 /* ambiguous */, kept = () => 2;\n",
        "export const first = () => 1, /** @deprecated */ deleteMe = () => 2, last = () => 3;\n",
        "for (let deleteMe = () => 1, kept = () => 2; false;) {}\n",
        "const kept = function deleteMe() {};\n",
        "class Parent { deleteMe() {} kept() {} }\n",
        "export const deleteMe = () => 1, kept = () => 2;\n/* malformed",
    ] {
        let root = tempfile::tempdir().unwrap();
        fs::write(root.path().join("module.ts"), source).unwrap();
        let mut engine = Engine::open(root.path()).unwrap();
        let (snapshot, graph, _, _) = engine.index(&d()).unwrap();
        let selected = graph
            .entities
            .iter()
            .find(|e| e.name == "deleteMe")
            .unwrap();
        let count: usize = engine
            .store
            .db
            .query_row("SELECT count(*) FROM snapshots", [], |r| r.get(0))
            .unwrap();
        assert!(
            changes::prepare(
                &mut engine.store,
                &snapshot,
                &graph,
                &json!({"deleteSymbol":{"entityId":selected.id,"siblingDeclarators":"preserve"}}),
                &d()
            )
            .is_err(),
            "{source:?}"
        );
        assert_eq!(
            engine
                .store
                .db
                .query_row("SELECT count(*) FROM snapshots", [], |r| r
                    .get::<_, usize>(0))
                .unwrap(),
            count
        );
        assert_eq!(engine.store.db.query_row("SELECT count(*) FROM records WHERE kind IN ('plan','approval','verification','receipt','journal')",[],|r|r.get::<_,usize>(0)).unwrap(),0);
        assert_eq!(
            fs::read(root.path().join("module.ts")).unwrap(),
            source.as_bytes()
        );
    }
}
#[test]
fn closed_policy_types_and_combinations_fail_before_implicit_capture() {
    let root = tempfile::tempdir().unwrap();
    fs::write(
        root.path().join("module.ts"),
        "export const deleteMe = () => 1, kept = () => 2;\n",
    )
    .unwrap();
    let mut engine = Engine::open(root.path()).unwrap();
    let (_, graph, _, _) = engine.index(&d()).unwrap();
    let selected = graph
        .entities
        .iter()
        .find(|e| e.name == "deleteMe")
        .unwrap();
    let count: usize = engine
        .store
        .db
        .query_row("SELECT count(*) FROM snapshots", [], |r| r.get(0))
        .unwrap();
    let mut values = Vec::new();
    for policy in [
        json!(true),
        json!(null),
        json!("delete_all"),
        json!({"preserve":true}),
        json!(1),
    ] {
        values.push(json!({"deleteSymbol":{"entityId":selected.id,"siblingDeclarators":policy}}));
    }
    values.push(json!({"deleteSymbol":{"entityId":selected.id,"siblingDeclarators":"preserve","emptyPythonBody":"reject"}}));
    for field in [
        "edits",
        "replaceSymbol",
        "insertBefore",
        "insertAfter",
        "rename",
        "bulkReplace",
    ] {
        let mut args =
            json!({"deleteSymbol":{"entityId":selected.id,"siblingDeclarators":"preserve"}});
        args[field] = json!([]);
        values.push(args);
    }
    for field in ["replaceSymbol", "insertBefore", "insertAfter", "rename"] {
        let mut args = json!({});
        args[field] = json!({"entityId":selected.id,"text":"kept","siblingDeclarators":"preserve"});
        values.push(args);
    }
    for args in values {
        assert!(yoke::validate("ci_change_prepare", &args).is_err());
        assert_eq!(
            engine.invoke("ci_change_prepare", args).unwrap()["status"],
            "error"
        );
    }
    assert_eq!(
        engine
            .store
            .db
            .query_row("SELECT count(*) FROM snapshots", [], |r| r
                .get::<_, usize>(0))
            .unwrap(),
        count
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
}
#[test]
fn forged_paths_names_kinds_offsets_and_deadline_cannot_select_another_declaration() {
    let root = tempfile::tempdir().unwrap();
    let source = "// café 🦀\nexport const deleteMe = () => 1, kept = () => 2;\n";
    fs::write(root.path().join("module.ts"), source).unwrap();
    let mut engine = Engine::open(root.path()).unwrap();
    let (snapshot, graph, _, _) = engine.index(&d()).unwrap();
    let index = graph
        .entities
        .iter()
        .position(|e| e.name == "deleteMe")
        .unwrap();
    let id = graph.entities[index].id.clone();
    let args = json!({"deleteSymbol":{"entityId":id,"siblingDeclarators":"preserve"}});
    for case in 0..5 {
        let mut forged = graph.clone();
        match case {
            0 => forged.entities[index].path = "../outside.ts".into(),
            1 => forged.entities[index].name = "kept".into(),
            2 => forged.entities[index].start_byte = source.find('é').unwrap() + 1,
            3 => forged.entities[index].end_byte -= 1,
            4 => forged.entities[index].kind = "class".into(),
            _ => unreachable!(),
        }
        assert!(changes::prepare(&mut engine.store, &snapshot, &forged, &args, &d()).is_err());
    }
    assert!(
        changes::prepare(
            &mut engine.store,
            &snapshot,
            &graph,
            &args,
            &Deadline::new(0)
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
        source.as_bytes()
    );
}
#[test]
fn sibling_policy_is_language_scoped_and_cannot_grant_application() {
    for (path, source) in [
        ("module.py", "def deleteMe():\n    pass\n"),
        ("module.rs", "fn deleteMe() {}\n"),
    ] {
        let root = tempfile::tempdir().unwrap();
        fs::write(root.path().join(path), source).unwrap();
        let mut engine = Engine::open(root.path()).unwrap();
        let (snapshot, graph, _, _) = engine.index(&d()).unwrap();
        let selected = graph
            .entities
            .iter()
            .find(|e| e.name == "deleteMe")
            .unwrap();
        for policy in ["reject", "preserve"] {
            assert!(
                changes::prepare(
                    &mut engine.store,
                    &snapshot,
                    &graph,
                    &json!({"deleteSymbol":{"entityId":selected.id,"siblingDeclarators":policy}}),
                    &d()
                )
                .is_err()
            );
        }
        assert_eq!(fs::read(root.path().join(path)).unwrap(), source.as_bytes());
    }
    let root = tempfile::tempdir().unwrap();
    let source = "export const deleteMe = () => 1, kept = () => 2;\n";
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
        &json!({"deleteSymbol":{"entityId":selected.id,"siblingDeclarators":"preserve"}}),
        &d(),
    )
    .unwrap();
    assert!(
        changes::apply(
            &engine.store,
            root.path(),
            &json!({"planId":plan.id,"idempotencyKey":"no-grants","mode":"workspace"}),
            false,
            &d()
        )
        .is_err()
    );
    assert_eq!(
        fs::read(root.path().join("module.ts")).unwrap(),
        source.as_bytes()
    );
}
#[test]
fn native_mcp_advertises_and_prepares_the_same_checked_pair_of_byte_edits() {
    let tools = yoke::protocol::tools();
    assert_eq!(tools.as_array().unwrap().len(), 8);
    let prepare = tools
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["name"] == "ci_change_prepare")
        .unwrap();
    assert_eq!(
        prepare["inputSchema"]["properties"]["deleteSymbol"]["properties"]["siblingDeclarators"]["enum"],
        json!(["reject", "preserve"])
    );
    let root = tempfile::tempdir().unwrap();
    let source = "export const deleteMe = () => 1, kept = () => 2;\n";
    fs::write(root.path().join("module.ts"), source).unwrap();
    let mut engine = Engine::open(root.path()).unwrap();
    let (snapshot, graph, _, _) = engine.index(&d()).unwrap();
    let selected = graph
        .entities
        .iter()
        .find(|e| e.name == "deleteMe")
        .unwrap();
    let response = {
        let mut connection = yoke::protocol::Connection::native(&mut engine);
        connection
            .message(br#"{"jsonrpc":"2.0","id":1,"method":"initialize"}"#)
            .unwrap();
        connection
            .message(br#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#)
            .unwrap();
        connection.message(&serde_json::to_vec(&json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{
            "name":"ci_change_prepare","arguments":{"snapshotId":snapshot.id,"deleteSymbol":{"entityId":selected.id,"siblingDeclarators":"preserve"}}}})).unwrap()).unwrap().unwrap()
    };
    assert_eq!(response["result"]["isError"], false, "{response}");
    let envelope: serde_json::Value =
        serde_json::from_str(response["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(envelope["metrics"]["modelCalls"], 0);
    let plan: changes::Plan = serde_json::from_value(envelope["data"]["plan"].clone()).unwrap();
    let after = engine.store.snapshot(&plan.result_snapshot_id).unwrap();
    assert_eq!(
        query::source(&engine.store, &after, "module.ts").unwrap().0,
        "export const  kept = () => 2;\n"
    );
    assert_eq!(
        fs::read(root.path().join("module.ts")).unwrap(),
        source.as_bytes()
    );
}
