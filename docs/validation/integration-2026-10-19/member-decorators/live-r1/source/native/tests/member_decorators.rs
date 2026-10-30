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
        let id = graph
            .entities
            .iter()
            .find(|e| e.kind == "method" && e.name.contains("deleteMe"))
            .map(|entity| entity.id.clone())
            .unwrap_or_else(|| "7".repeat(64));
        assert!(
            changes::prepare(
                &mut engine.store,
                &snapshot,
                &graph,
                &json!({"deleteSymbol":{"entityId":id}}),
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

#[test]
fn forged_member_anchors_paths_names_and_deadline_never_expand_decorator_ownership() {
    let root = tempfile::tempdir().unwrap();
    let source = "// café 🦀\nclass Parent { @decorator deleteMe() {} kept() {} }\n";
    fs::write(root.path().join("module.ts"), source).unwrap();
    let mut engine = Engine::open(root.path()).unwrap();
    let (snapshot, graph, _, _) = engine.index(&d()).unwrap();
    let index = graph
        .entities
        .iter()
        .position(|e| e.name == "deleteMe")
        .unwrap();
    let args = json!({"deleteSymbol":{"entityId":graph.entities[index].id}});
    for case in 0..5 {
        let mut changed = graph.clone();
        match case {
            0 => changed.entities[index].start_byte = source.find('@').unwrap(),
            1 => changed.entities[index].end_byte -= 1,
            2 => changed.entities[index].name = "kept".into(),
            3 => changed.entities[index].path = "../outside.ts".into(),
            4 => changed.entities[index].kind = "function".into(),
            _ => unreachable!(),
        }
        assert!(changes::prepare(&mut engine.store, &snapshot, &changed, &args, &d()).is_err());
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
fn native_mcp_prepares_owned_decorators_without_write_or_execution_grants() {
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
    let response = {
        let mut connection = yoke::protocol::Connection::native(&mut engine);
        connection
            .message(br#"{"jsonrpc":"2.0","id":1,"method":"initialize"}"#)
            .unwrap();
        connection
            .message(br#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#)
            .unwrap();
        connection.message(&serde_json::to_vec(&json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{
            "name":"ci_change_prepare","arguments":{"snapshotId":snapshot.id,"deleteSymbol":{"entityId":selected.id}}}})).unwrap()).unwrap().unwrap()
    };
    assert_eq!(response["result"]["isError"], false, "{response}");
    let envelope: serde_json::Value =
        serde_json::from_str(response["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(envelope["metrics"]["modelCalls"], 0);
    let plan: changes::Plan = serde_json::from_value(envelope["data"]["plan"].clone()).unwrap();
    let after = engine.store.snapshot(&plan.result_snapshot_id).unwrap();
    assert_eq!(
        query::source(&engine.store, &after, "module.ts").unwrap().0,
        "class Parent {  kept() {} }\n"
    );
    assert_eq!(
        fs::read(root.path().join("module.ts")).unwrap(),
        source.as_bytes()
    );
    assert!(
        changes::apply(
            &engine.store,
            root.path(),
            &json!({"planId":plan.id,"idempotencyKey":"no-member-grants","mode":"workspace"}),
            false,
            &d()
        )
        .is_err()
    );
}

fn actual_cycle(source: &str, expected: &str, path: &str, through_provider: bool) {
    let node = std::env::var("YOKE_NODE").expect("Select actual Node executable");
    let compiler = std::env::var("YOKE_TSC").expect("Select actual TypeScript compiler");
    let library = std::path::Path::new(&compiler)
        .parent()
        .unwrap()
        .join("typescript.js");
    for selected in [
        std::path::Path::new(&node),
        std::path::Path::new(&compiler),
        library.as_path(),
    ] {
        assert!(selected.is_absolute() && selected.is_file());
    }
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join(path), source).unwrap();
    fs::write(root.path().join("tsconfig.json"),serde_json::to_vec(&json!({"compilerOptions":{
        "target":"es2022","module":"esnext","strict":true,"noEmit":true,"experimentalDecorators":true,
        "allowJs":true,"checkJs":true,"types":[],"skipLibCheck":true},"files":[path]})).unwrap()).unwrap();
    let mut engine = Engine::open(root.path()).unwrap();
    let (snapshot, graph, _, _) = engine.index(&d()).unwrap();
    let selected = graph
        .entities
        .iter()
        .find(|e| e.name == "deleteMe")
        .unwrap();
    let original = serde_json::to_value(selected).unwrap();
    let plan = if through_provider {
        let cli =
            std::env::var("YOKE_TS_LSP").expect("Select actual TypeScript language-server CLI");
        assert!(std::path::Path::new(&cli).is_absolute() && std::path::Path::new(&cli).is_file());
        engine
            .register_configured_server(yoke::lsp::ConfiguredServer {
                server: yoke::lsp::Server {
                    command: node.clone(),
                    args: vec![cli, "--stdio".into()],
                    language: "typescript".into(),
                    trust: true,
                },
                configuration: yoke::lsp::HostConfiguration {
                    passive_profile: Some(yoke::lsp::PassiveProfile::TypescriptLanguageServer),
                    ..Default::default()
                },
            })
            .unwrap();
        let value = yoke::semantic_refactor::invoke(
            &mut engine,
            "safeDelete",
            &json!({"relativePath":path,"namePath":"deleteMe"}),
            &Deadline::new(90_000),
        )
        .unwrap();
        println!("actual-member-safe-delete={value}");
        assert_eq!(value["complete"], false);
        serde_json::from_value(value["plan"].clone()).unwrap()
    } else {
        changes::prepare(
            &mut engine.store,
            &snapshot,
            &graph,
            &json!({"deleteSymbol":{"entityId":selected.id}}),
            &d(),
        )
        .unwrap()
    };
    let args =
        json!({"planId":plan.id,"idempotencyKey":"actual-member-decorators","mode":"workspace"});
    assert!(changes::apply(&engine.store, root.path(), &args, true, &d()).is_err());
    let compile = yoke::verify::execute(
        &engine,
        &plan.id,
        &node,
        &[
            compiler,
            "--noEmit".into(),
            "--project".into(),
            "tsconfig.json".into(),
        ],
        true,
        &Deadline::new(60_000),
    )
    .unwrap();
    println!("actual-member-{path}-compiler={compile}");
    assert_eq!(compile["status"], "passed", "{compile}");
    let ast_code = "const ts=require(process.argv[1]); const fs=require('fs'); const file=process.argv[2]; const kind=file.endsWith('.tsx')?ts.ScriptKind.TSX:file.endsWith('.jsx')?ts.ScriptKind.JSX:file.endsWith('.js')?ts.ScriptKind.JS:ts.ScriptKind.TS; const tree=ts.createSourceFile(file,fs.readFileSync(file,'utf8'),ts.ScriptTarget.Latest,true,kind); const cls=tree.statements.find(ts.isClassDeclaration); if(!cls)throw Error('class missing'); const methods=cls.members.filter(ts.isMethodDeclaration); if(methods.length!==1||methods[0].name.getText(tree)!=='kept')throw Error('method selection changed'); if((ts.getDecorators(methods[0])||[]).length!==0)throw Error('decorator reattached to neighbor'); const parent=ts.getDecorators(cls)||[]; if(parent.length!==1||parent[0].expression.getText(tree)!=='parentMark')throw Error('parent decorator changed'); console.log('checked-parent-and-neighbor-decorator-ownership')";
    let ast = yoke::verify::execute(
        &engine,
        &plan.id,
        &node,
        &[
            "-e".into(),
            ast_code.into(),
            library.to_str().unwrap().into(),
            path.into(),
        ],
        true,
        &d(),
    )
    .unwrap();
    println!("actual-member-{path}-ast={ast}");
    assert_eq!(ast["status"], "passed", "{ast}");
    assert!(changes::apply(&engine.store, root.path(), &args, true, &d()).is_err());
    changes::approve(&engine.store, &plan.id, false).unwrap();
    assert!(changes::apply(&engine.store, root.path(), &args, false, &d()).is_err());
    let receipt = changes::apply(&engine.store, root.path(), &args, true, &d()).unwrap();
    assert_eq!(
        fs::read(root.path().join(path)).unwrap(),
        expected.as_bytes()
    );
    changes::recover(&engine.store, root.path(), &receipt.id, true).unwrap();
    assert_eq!(fs::read(root.path().join(path)).unwrap(), source.as_bytes());
    assert_eq!(
        query::source(&engine.store, &snapshot, path).unwrap().0,
        source
    );
    assert_eq!(serde_json::to_value(selected).unwrap(), original);
}
#[test]
#[ignore = "requires YOKE_NODE and YOKE_TSC selecting actual installed compiler/AST tools"]
fn actual_typescript_compiler_and_ast_confirm_parent_and_neighbor_decorator_ownership() {
    for extension in ["ts", "tsx", "js", "jsx"] {
        let prefix = if matches!(extension, "ts" | "tsx") {
            "declare const parentMark: any, firstMark: any, secondMark: any;\r\n"
        } else {
            "/** @type {any} */ var parentMark; /** @type {any} */ var firstMark; /** @type {any} */ var secondMark;\r\n"
        };
        let source = format!(
            "\u{feff}// café 🦀\r\n{prefix}@parentMark\r\nexport class Parent {{\r\n    @firstMark\r\n    @secondMark\r\n    deleteMe() {{ return 1; }}\r\n    kept() {{ return 2; }}\r\n}}\r\n"
        );
        let expected = format!(
            "\u{feff}// café 🦀\r\n{prefix}@parentMark\r\nexport class Parent {{\r\n    \r\n    kept() {{ return 2; }}\r\n}}\r\n"
        );
        actual_cycle(&source, &expected, &format!("module.{extension}"), false);
    }
}
#[test]
#[ignore = "requires YOKE_NODE/YOKE_TS_LSP/YOKE_TSC selecting actual TypeScript language server and compiler"]
fn actual_language_server_safe_delete_uses_the_same_checked_member_decorator_plan() {
    let source = "\u{feff}// café 🦀\r\ndeclare const parentMark: any, firstMark: any, secondMark: any;\r\n@parentMark\r\nexport class Parent {\r\n    @firstMark\r\n    @secondMark\r\n    deleteMe() { return 1; }\r\n    kept() { return 2; }\r\n}\r\n";
    let expected = "\u{feff}// café 🦀\r\ndeclare const parentMark: any, firstMark: any, secondMark: any;\r\n@parentMark\r\nexport class Parent {\r\n    \r\n    kept() { return 2; }\r\n}\r\n";
    actual_cycle(source, expected, "module.ts", true);
}
