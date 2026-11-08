//! Explicit Python pass edits preserve exact declaration/source ownership.
use serde_json::json;
use std::fs;
use yoke::{Engine, analysis::AnalyzerEpoch, capture::Deadline, changes, query};

fn d() -> Deadline {
    Deadline::new(30_000)
}
fn checked(source: &str, expected: &str) {
    for epoch in [AnalyzerEpoch::V8, AnalyzerEpoch::V9] {
        let root = tempfile::tempdir().unwrap();
        fs::write(root.path().join("module.py"), source).unwrap();
        let mut engine = Engine::open(root.path()).unwrap();
        engine.capture_epoch = epoch;
        let (snapshot, graph, _, _) = engine.index(&d()).unwrap();
        let selected = graph
            .entities
            .iter()
            .find(|e| e.name == "delete_me")
            .unwrap();
        let original = serde_json::to_value(selected).unwrap();
        let plan = changes::prepare(
            &mut engine.store,
            &snapshot,
            &graph,
            &json!({"deleteSymbol":{"entityId":selected.id,"emptyPythonBody":"pass"}}),
            &d(),
        )
        .unwrap_or_else(|error| panic!("{source:?}: {error:#}"));
        let derived = engine.store.snapshot(&plan.result_snapshot_id).unwrap();
        assert_eq!(
            query::source(&engine.store, &derived, "module.py")
                .unwrap()
                .0,
            expected,
            "{source:?}"
        );
        assert_eq!(
            query::source(&engine.store, &snapshot, "module.py")
                .unwrap()
                .0,
            source
        );
        assert_eq!(serde_json::to_value(selected).unwrap(), original);
        assert_eq!(
            fs::read(root.path().join("module.py")).unwrap(),
            source.as_bytes()
        );
        let checked = changes::load_checked_plan(&engine.store, &plan.id, &d()).unwrap();
        assert_eq!(checked.diff, plan.diff);
        assert!(
            engine
                .store
                .graph(&derived.id)
                .unwrap()
                .diagnostics
                .is_empty()
        );
        assert_eq!(engine.store.db.query_row("SELECT count(*) FROM records WHERE kind IN ('approval','verification','receipt','journal')",[],|r|r.get::<_,usize>(0)).unwrap(),0);
    }
}
#[test]
fn only_class_method_owns_decorators_without_changing_bom_unicode_or_crlf() {
    for bom in ["", "\u{feff}"] {
        for declaration in [
            "def delete_me(self):\r\n        return 1",
            "async def delete_me(self):\r\n        return 1",
        ] {
            checked(
                &format!(
                    "{bom}# café 🦀\r\n@parent_decorator\r\nclass Parent:\r\n    @outer(\r\n        '🦀'\r\n    )\r\n    @inner\r\n    {declaration}\r\n"
                ),
                &format!("{bom}# café 🦀\r\n@parent_decorator\r\nclass Parent:\r\n    pass\r\n"),
            );
        }
    }
}
#[test]
fn nested_function_class_and_control_flow_suites_receive_exactly_one_pass() {
    for header in [
        "def parent():",
        "async def parent():",
        "class Parent:",
        "if condition:",
        "for value in values:",
        "while condition:",
        "with manager:",
    ] {
        for declaration in [
            "def delete_me():\n        return 1",
            "class delete_me:\n        pass",
        ] {
            checked(
                &format!("{header}\n    {declaration}\n"),
                &format!("{header}\n    pass\n"),
            );
        }
    }
    checked(
        "class Parent:\n\t@decorator\n\tdef delete_me(self):\n\t\treturn 1\n",
        "class Parent:\n\tpass\n",
    );
    checked(
        "try:\n    def delete_me():\n        return 1\nexcept Exception:\n    retained()\n",
        "try:\n    pass\nexcept Exception:\n    retained()\n",
    );
    checked(
        "match value:\n    case 1:\n        def delete_me():\n            return 1\n",
        "match value:\n    case 1:\n        pass\n",
    );
}
#[test]
fn comments_remain_owned_and_existing_statements_or_module_need_no_pass() {
    checked(
        "class Parent:\n    # preceding café\n    def delete_me(self):\n        return 1\n    # trailing 🦀\n",
        "class Parent:\n    # preceding café\n    pass\n    # trailing 🦀\n",
    );
    checked(
        "class Parent:\n    \"\"\"retained docstring\"\"\"\n    def delete_me(self):\n        return 1\n",
        "class Parent:\n    \"\"\"retained docstring\"\"\"\n    \n",
    );
    checked(
        "class Parent:\n    def delete_me(self): return 1\n    retained = 2\n",
        "class Parent:\n    \n    retained = 2\n",
    );
    checked("@decorator\ndef delete_me():\n    return 1\n", "\n");
}
#[test]
fn absence_and_explicit_reject_retain_the_released_empty_suite_refusal() {
    let root = tempfile::tempdir().unwrap();
    let source = "class Parent:\n    def delete_me(self):\n        return 1\n";
    fs::write(root.path().join("module.py"), source).unwrap();
    let mut engine = Engine::open(root.path()).unwrap();
    let (snapshot, graph, _, _) = engine.index(&d()).unwrap();
    let selected = graph
        .entities
        .iter()
        .find(|e| e.name == "delete_me")
        .unwrap();
    for operation in [
        json!({"entityId":selected.id}),
        json!({"entityId":selected.id,"emptyPythonBody":"reject"}),
    ] {
        let args = json!({"deleteSymbol":operation});
        assert!(changes::prepare(&mut engine.store, &snapshot, &graph, &args, &d()).is_err());
    }
    assert_eq!(engine.store.db.query_row("SELECT count(*) FROM records WHERE kind IN ('plan','approval','receipt','journal')",[],|r|r.get::<_,usize>(0)).unwrap(),0);
    assert_eq!(
        fs::read(root.path().join("module.py")).unwrap(),
        source.as_bytes()
    );
}
#[test]
fn closed_protocol_and_mixed_intents_refuse_before_new_capture_or_plan_publication() {
    let root = tempfile::tempdir().unwrap();
    fs::write(
        root.path().join("module.py"),
        "class Parent:\n    def delete_me(self): pass\n",
    )
    .unwrap();
    let mut engine = Engine::open(root.path()).unwrap();
    let (snapshot, graph, _, _) = engine.index(&d()).unwrap();
    let selected = graph
        .entities
        .iter()
        .find(|e| e.name == "delete_me")
        .unwrap();
    let rows: usize = engine
        .store
        .db
        .query_row("SELECT count(*) FROM snapshots", [], |r| r.get(0))
        .unwrap();
    for policy in [
        json!(true),
        json!(null),
        json!("execute"),
        json!({"pass":true}),
        json!(1),
    ] {
        let args = json!({"deleteSymbol":{"entityId":selected.id,"emptyPythonBody":policy}});
        assert!(yoke::validate("ci_change_prepare", &args).is_err());
        assert_eq!(
            engine.invoke("ci_change_prepare", args).unwrap()["status"],
            "error"
        );
    }
    for field in [
        "edits",
        "replaceSymbol",
        "insertBefore",
        "insertAfter",
        "rename",
        "bulkReplace",
    ] {
        let mut args = json!({"deleteSymbol":{"entityId":selected.id,"emptyPythonBody":"pass"}});
        args[field] = json!([]);
        assert_eq!(
            engine.invoke("ci_change_prepare", args.clone()).unwrap()["status"],
            "error"
        );
        assert!(changes::prepare(&mut engine.store, &snapshot, &graph, &args, &d()).is_err());
    }
    for field in ["replaceSymbol", "insertBefore", "insertAfter", "rename"] {
        let mut args = json!({});
        args[field] = json!({"entityId":selected.id,"text":"pass","emptyPythonBody":"pass"});
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
        rows
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
fn policy_cannot_be_applied_to_another_language_or_grant_write_authority() {
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("module.rs"), "fn delete_me() {}\n").unwrap();
    let mut engine = Engine::open(root.path()).unwrap();
    let (snapshot, graph, _, _) = engine.index(&d()).unwrap();
    let selected = graph
        .entities
        .iter()
        .find(|e| e.name == "delete_me")
        .unwrap();
    for policy in ["reject", "pass"] {
        assert!(
            changes::prepare(
                &mut engine.store,
                &snapshot,
                &graph,
                &json!({"deleteSymbol":{"entityId":selected.id,"emptyPythonBody":policy}}),
                &d()
            )
            .is_err()
        );
    }
    let tools = yoke::protocol::tools();
    let list = tools.as_array().unwrap();
    assert_eq!(list.len(), 8);
    let prepare = list
        .iter()
        .find(|t| t["name"] == "ci_change_prepare")
        .unwrap();
    assert_eq!(
        prepare["inputSchema"]["properties"]["deleteSymbol"]["properties"]["emptyPythonBody"]["enum"],
        json!(["reject", "pass"])
    );
    assert_eq!(
        prepare["inputSchema"]["properties"]["deleteSymbol"]["additionalProperties"],
        false
    );
}
#[test]
fn explicit_policy_still_checks_forged_bindings_paths_and_the_shared_deadline() {
    let root = tempfile::tempdir().unwrap();
    let source = "class Parent:\n    def delete_me(self): pass\n";
    fs::write(root.path().join("module.py"), source).unwrap();
    let mut engine = Engine::open(root.path()).unwrap();
    let (snapshot, graph, _, _) = engine.index(&d()).unwrap();
    let index = graph
        .entities
        .iter()
        .position(|e| e.name == "delete_me")
        .unwrap();
    let id = graph.entities[index].id.clone();
    let args = json!({"deleteSymbol":{"entityId":id,"emptyPythonBody":"pass"}});
    for case in 0..4 {
        let mut forged = graph.clone();
        match case {
            0 => forged.entities[index].name = "another".into(),
            1 => forged.entities[index].start_byte += 1,
            2 => forged.entities[index].path = "../outside.py".into(),
            3 => forged.entities[index].kind = "class".into(),
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
        fs::read(root.path().join("module.py")).unwrap(),
        source.as_bytes()
    );
}

#[test]
fn native_mcp_prepares_the_exact_opt_in_edit_without_apply_or_execution_authority() {
    let root = tempfile::tempdir().unwrap();
    let source = "class Parent:\n    @not_executed\n    def delete_me(self): pass\n";
    fs::write(root.path().join("module.py"), source).unwrap();
    let mut engine = Engine::open(root.path()).unwrap();
    let (snapshot, graph, _, _) = engine.index(&d()).unwrap();
    let selected = graph
        .entities
        .iter()
        .find(|e| e.name == "delete_me")
        .unwrap();
    let call = json!({"jsonrpc":"2.0","id":2,"method":"tools/call", "params":{
        "name":"ci_change_prepare","arguments":{"snapshotId":snapshot.id,
        "deleteSymbol":{"entityId":selected.id,"emptyPythonBody":"pass"}}}});
    let response = {
        let mut connection = yoke::protocol::Connection::native(&mut engine);
        connection
            .message(br#"{"jsonrpc":"2.0","id":1,"method":"initialize"}"#)
            .unwrap();
        connection
            .message(br#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#)
            .unwrap();
        connection
            .message(&serde_json::to_vec(&call).unwrap())
            .unwrap()
            .unwrap()
    };
    assert_eq!(response["result"]["isError"], false, "{response}");
    let envelope: serde_json::Value =
        serde_json::from_str(response["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(envelope["status"], "ok");
    assert_eq!(envelope["metrics"]["modelCalls"], 0);
    let plan: changes::Plan = serde_json::from_value(envelope["data"]["plan"].clone()).unwrap();
    let derived = engine.store.snapshot(&plan.result_snapshot_id).unwrap();
    assert_eq!(
        query::source(&engine.store, &derived, "module.py")
            .unwrap()
            .0,
        "class Parent:\n    pass\n"
    );
    assert_eq!(
        fs::read(root.path().join("module.py")).unwrap(),
        source.as_bytes()
    );
    assert!(
        changes::apply(
            &engine.store,
            root.path(),
            &json!({"planId":plan.id,
        "idempotencyKey":"mcp-pass-no-grants","mode":"workspace"}),
            false,
            &d()
        )
        .is_err()
    );
    assert_eq!(engine.store.db.query_row("SELECT count(*) FROM records WHERE kind IN ('approval','verification','receipt','journal')",[],|r|r.get::<_,usize>(0)).unwrap(),0);
}

#[test]
#[ignore = "requires YOKE_PYTHON selecting actual Python for stdlib AST-only verification"]
fn actual_python_ast_and_guarded_apply_recovery_validate_the_planned_pass() {
    let compiler = std::env::var("YOKE_PYTHON").expect("Select actual Python");
    assert!(std::path::Path::new(&compiler).is_absolute());
    let root = tempfile::tempdir().unwrap();
    let source = "\u{feff}# café 🦀\r\n@not_executed\r\nclass Parent:\r\n    @also_not_executed\r\n    async def delete_me(self):\r\n        return 1\r\n";
    let expected = "\u{feff}# café 🦀\r\n@not_executed\r\nclass Parent:\r\n    pass\r\n";
    fs::write(root.path().join("module.py"), source).unwrap();
    let mut engine = Engine::open(root.path()).unwrap();
    let (snapshot, graph, _, _) = engine.index(&d()).unwrap();
    let selected = graph
        .entities
        .iter()
        .find(|e| e.name == "delete_me")
        .unwrap();
    let plan = changes::prepare(
        &mut engine.store,
        &snapshot,
        &graph,
        &json!({"deleteSymbol":{"entityId":selected.id,"emptyPythonBody":"pass"}}),
        &d(),
    )
    .unwrap();
    let args = json!({"planId":plan.id,"idempotencyKey":"actual-python-pass","mode":"workspace"});
    assert!(changes::apply(&engine.store, root.path(), &args, true, &d()).is_err());
    let code = "import ast,pathlib; tree=ast.parse(pathlib.Path('module.py').read_bytes(),filename='module.py'); parent=tree.body[0]; assert isinstance(parent,ast.ClassDef); assert len(parent.body)==1 and isinstance(parent.body[0],ast.Pass); assert len(parent.decorator_list)==1 and parent.decorator_list[0].id=='not_executed'";
    let verification = yoke::verify::execute(
        &engine,
        &plan.id,
        &compiler,
        &["-I".into(), "-S".into(), "-c".into(), code.into()],
        true,
        &d(),
    )
    .unwrap();
    println!("actual-python-pass-verification={verification}");
    assert_eq!(verification["status"], "passed");
    assert!(changes::apply(&engine.store, root.path(), &args, true, &d()).is_err());
    changes::approve(&engine.store, &plan.id, false).unwrap();
    assert!(changes::apply(&engine.store, root.path(), &args, false, &d()).is_err());
    let receipt = changes::apply(&engine.store, root.path(), &args, true, &d()).unwrap();
    assert_eq!(
        fs::read(root.path().join("module.py")).unwrap(),
        expected.as_bytes()
    );
    changes::recover(&engine.store, root.path(), &receipt.id, true).unwrap();
    assert_eq!(
        fs::read(root.path().join("module.py")).unwrap(),
        source.as_bytes()
    );
    assert_eq!(
        query::source(&engine.store, &snapshot, "module.py")
            .unwrap()
            .0,
        source
    );
    println!(
        "actual-python-pass-plan={}",
        serde_json::to_value(&plan).unwrap()
    );
}
