//! Explicit installed-provider/compiler validation of owned propagation fixtures.
use serde_json::json;
use std::{collections::BTreeSet, fs, path::Path};
use yoke::{Engine, capture::Deadline, changes, lsp};

#[test]
#[ignore = "requires YOKE_TS_LSP and YOKE_TSC selecting actual installed TypeScript tools"]
fn actual_typescript_and_javascript_propagation_removes_uses_and_newly_unused_dependencies() {
    let cli =
        std::env::var("YOKE_TS_LSP").expect("Select the actual TypeScript language-server CLI");
    let compiler = std::env::var("YOKE_TSC").expect("Select the actual TypeScript compiler CLI");
    for path in [&cli, &compiler] {
        assert!(Path::new(path).is_absolute() && Path::new(path).is_file());
    }
    for (language, extension) in [("typescript", "ts"), ("javascript", "js")] {
        let root = tempfile::tempdir().unwrap();
        let path = format!("module.{extension}");
        let source = "// owned café 🦀\r\nfunction leaf() { return 1; }\r\nfunction dependency() { return leaf(); }\r\nexport function removeMe() { return dependency(); }\r\nexport function keep() { removeMe(); return 2; }\r\n";
        let expected = "// owned café 🦀\r\n\r\n\r\n\r\nexport function keep() {  return 2; }\r\n";
        fs::write(root.path().join(&path), source).unwrap();
        fs::write(root.path().join("tsconfig.json"), serde_json::to_vec(&json!({"compilerOptions":{"strict":true,"module":"esnext","target":"es2022","allowJs":language=="javascript","checkJs":language=="javascript"},"include":[path]})).unwrap()).unwrap();
        let mut engine = Engine::open(root.path()).unwrap();
        engine
            .register_configured_server(lsp::ConfiguredServer {
                server: lsp::Server {
                    command: "node".into(),
                    args: vec![cli.clone(), "--stdio".into()],
                    language: language.into(),
                    trust: true,
                },
                configuration: lsp::HostConfiguration {
                    passive_profile: Some(lsp::PassiveProfile::TypescriptLanguageServer),
                    ..Default::default()
                },
            })
            .unwrap();
        let value = yoke::semantic_refactor::invoke(
            &mut engine,
            "safeDelete",
            &json!({"relativePath":path,"namePath":"removeMe","propagate":true}),
            &Deadline::new(90_000),
        )
        .unwrap_or_else(|error| panic!("Actual {language} propagated deletion: {error:#}"));
        println!("actual-{language}-propagated-delete-plan={value}");
        assert_eq!(value["complete"], false);
        assert_eq!(value["remainingUsagesMayBreak"], true);
        assert_eq!(value["propagation"]["performed"], true);
        assert_eq!(value["propagation"]["removedUsageCount"], 1);
        let deleted: BTreeSet<_> = value["propagation"]["deletedDeclarations"]
            .as_array()
            .unwrap()
            .iter()
            .map(|d| d["name"].as_str().unwrap())
            .collect();
        assert_eq!(deleted, BTreeSet::from(["removeMe", "dependency", "leaf"]));
        assert_eq!(
            fs::read(root.path().join(&path)).unwrap(),
            source.as_bytes()
        );
        let id = value["plan"]["id"].as_str().unwrap();
        let verification = yoke::verify::execute(
            &engine,
            id,
            "node",
            &[
                compiler.clone(),
                "--noEmit".into(),
                "--project".into(),
                "tsconfig.json".into(),
            ],
            true,
            &Deadline::new(60_000),
        )
        .unwrap();
        println!("actual-{language}-propagated-delete-verification={verification}");
        assert_eq!(verification["status"], "passed", "{verification}");
        changes::approve(&engine.store, id, false).unwrap();
        let receipt = changes::apply(
            &engine.store,
            root.path(),
            &json!({"planId":id,"idempotencyKey":"actual-propagated-delete","mode":"workspace"}),
            true,
            &Deadline::new(30_000),
        )
        .unwrap();
        assert_eq!(
            fs::read(root.path().join(&path)).unwrap(),
            expected.as_bytes()
        );
        changes::recover(&engine.store, root.path(), &receipt.id, true).unwrap();
        assert_eq!(
            fs::read(root.path().join(&path)).unwrap(),
            source.as_bytes()
        );
    }
}

#[test]
#[ignore = "requires YOKE_NODE, YOKE_TS_LSP and YOKE_TSC selecting actual installed tools"]
fn actual_wrapped_propagation_preserves_compiler_approval_write_and_recovery_gates() {
    let node = std::env::var("YOKE_NODE").unwrap();
    let cli = std::env::var("YOKE_TS_LSP").unwrap();
    let compiler = std::env::var("YOKE_TSC").unwrap();
    for path in [&node, &cli, &compiler] {
        assert!(Path::new(path).is_absolute() && Path::new(path).is_file());
    }
    for (language, extension) in [("typescript", "ts"), ("javascript", "js")] {
        let root = tempfile::tempdir().unwrap();
        let path = format!("module.{extension}");
        let source = "\u{feff}// owned café 🦀\r\nfunction leaf() { throw new Error('analyzed source must not execute'); }\r\nexport function removeMe() { return leaf(); }\r\nexport async function keep() { ((removeMe())); await (removeMe()); void (await removeMe()); return 2; }\r\n";
        let expected =
            "\u{feff}// owned café 🦀\r\n\r\n\r\nexport async function keep() {    return 2; }\r\n";
        fs::write(root.path().join(&path), source).unwrap();
        fs::write(root.path().join("tsconfig.json"), serde_json::to_vec(&json!({"compilerOptions":{"strict":true,"module":"esnext","target":"es2022","allowJs":language=="javascript","checkJs":language=="javascript"},"include":[path]})).unwrap()).unwrap();
        let mut engine = Engine::open(root.path()).unwrap();
        engine
            .register_configured_server(lsp::ConfiguredServer {
                server: lsp::Server {
                    command: node.clone(),
                    args: vec![cli.clone(), "--stdio".into()],
                    language: language.into(),
                    trust: true,
                },
                configuration: lsp::HostConfiguration {
                    passive_profile: Some(lsp::PassiveProfile::TypescriptLanguageServer),
                    ..Default::default()
                },
            })
            .unwrap();
        let value = yoke::semantic_refactor::invoke(&mut engine,"safeDelete",
            &json!({"relativePath":path,"namePath":"removeMe","propagate":true,"propagationPolicy":"captured_es_function_propagation_v2"}),
            &Deadline::new(90_000)).unwrap_or_else(|error|panic!("Actual {language} wrapped propagation: {error:#}"));
        println!("actual-{language}-wrapped-propagation-plan={value}");
        assert_eq!(
            value["propagation"]["policy"],
            "captured_es_function_propagation_v2"
        );
        assert_eq!(value["propagation"]["removedUsageCount"], 3);
        assert_eq!(value["propagation"]["exhaustiveCompilerProof"], false);
        assert_eq!(value["complete"], false);
        assert_eq!(value["remainingUsagesMayBreak"], true);
        let id = value["plan"]["id"].as_str().unwrap();
        let apply =
            json!({"planId":id,"idempotencyKey":"actual-wrapped-propagation","mode":"workspace"});
        assert!(
            changes::apply(
                &engine.store,
                root.path(),
                &apply,
                true,
                &Deadline::new(30_000)
            )
            .is_err()
        );
        let command = vec![
            compiler.clone(),
            "--noEmit".into(),
            "--project".into(),
            "tsconfig.json".into(),
        ];
        assert!(
            yoke::verify::execute(&engine, id, &node, &command, false, &Deadline::new(60_000))
                .is_err()
        );
        assert_eq!(
            fs::read(root.path().join(&path)).unwrap(),
            source.as_bytes()
        );
        let verification =
            yoke::verify::execute(&engine, id, &node, &command, true, &Deadline::new(60_000))
                .unwrap();
        println!("actual-{language}-wrapped-propagation-verification={verification}");
        assert_eq!(verification["status"], "passed", "{verification}");
        assert!(
            changes::apply(
                &engine.store,
                root.path(),
                &apply,
                true,
                &Deadline::new(30_000)
            )
            .is_err()
        );
        changes::approve(&engine.store, id, false).unwrap();
        assert!(
            changes::apply(
                &engine.store,
                root.path(),
                &apply,
                false,
                &Deadline::new(30_000)
            )
            .is_err()
        );
        let receipt = changes::apply(
            &engine.store,
            root.path(),
            &apply,
            true,
            &Deadline::new(30_000),
        )
        .unwrap();
        assert_eq!(
            fs::read(root.path().join(&path)).unwrap(),
            expected.as_bytes()
        );
        assert!(changes::recover(&engine.store, root.path(), &receipt.id, false).is_err());
        changes::recover(&engine.store, root.path(), &receipt.id, true).unwrap();
        assert_eq!(
            fs::read(root.path().join(&path)).unwrap(),
            source.as_bytes()
        );
    }
}
