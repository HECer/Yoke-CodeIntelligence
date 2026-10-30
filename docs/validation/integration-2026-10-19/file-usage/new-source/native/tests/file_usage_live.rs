//! Explicit real TypeScript language-server and compiler gate, without fixture fallback.
use serde_json::json;
use std::{fs, path::Path};
use yoke::{Engine, capture::Deadline, changes, lsp};

#[test]
#[ignore = "requires YOKE_TS_LSP and YOKE_TSC selecting actual installed TypeScript tools"]
fn actual_typescript_and_javascript_unused_modules_are_checked_compiled_applied_and_recovered() {
    let cli =
        std::env::var("YOKE_TS_LSP").expect("Select the actual TypeScript language-server CLI");
    let compiler = std::env::var("YOKE_TSC").expect("Select the actual TypeScript compiler CLI");
    for path in [&cli, &compiler] {
        assert!(Path::new(path).is_absolute() && Path::new(path).is_file());
    }
    for (language, extension) in [("typescript", "ts"), ("javascript", "js")] {
        let selected = format!("spare_module.{extension}");
        let other = format!("other.{extension}");
        let root = tempfile::tempdir().unwrap();
        let source = if language == "typescript" {
            "// actual provider fixture: café 🦀\r\nexport function unusedCafé(): number { return 42; }\r\n"
        } else {
            "// actual provider fixture: café 🦀\r\nexport function unusedCafé() { return 42; }\r\n"
        };
        fs::write(root.path().join(&selected), source).unwrap();
        fs::write(root.path().join(&other), "export const untouched = 7;\r\n").unwrap();
        fs::write(root.path().join("tsconfig.json"),serde_json::to_vec(&json!({"compilerOptions":{"strict":true,"module":"esnext","target":"es2022","allowJs":language=="javascript","checkJs":language=="javascript"},"include":[format!("*.{extension}")]})).unwrap()).unwrap();
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
            &json!({"relativePath":selected}),
            &Deadline::new(90_000),
        )
        .unwrap_or_else(|error| panic!("Actual TypeScript file usage gate: {error:#}"));
        println!("actual-{language}-file-usage-plan={value}");
        assert_eq!(value["forced"], false);
        assert_eq!(value["usageCheck"]["performed"], true);
        assert_eq!(value["usageCheck"]["providerRequestCount"], 2);
        assert_eq!(
            value["usageCheck"]["declarations"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
        assert_eq!(value["complete"], false);
        assert_eq!(
            fs::read(root.path().join(&selected)).unwrap(),
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
        println!("actual-{language}-verification={verification}");
        assert_eq!(verification["status"], "passed", "{verification}");
        changes::approve(&engine.store, id, false).unwrap();
        let receipt = changes::apply(
        &engine.store,
        root.path(),
        &json!({"planId":id,"idempotencyKey":"actual-typescript-file-usage","mode":"workspace"}),
        true,
        &Deadline::new(30_000),
    )
    .unwrap();
        assert!(!root.path().join(&selected).exists());
        changes::recover(&engine.store, root.path(), &receipt.id, true).unwrap();
        assert_eq!(
            fs::read(root.path().join(&selected)).unwrap(),
            source.as_bytes()
        );
        let count: usize = engine
            .store
            .db
            .query_row(
                "SELECT COUNT(*) FROM records WHERE kind='plan'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        fs::write(
            root.path().join(&other),
            "import { unusedCafé } from './spare_module';\nexport const retained = unusedCafé();\n",
        )
        .unwrap();
        assert!(
            yoke::semantic_refactor::invoke(
                &mut engine,
                "safeDelete",
                &json!({"relativePath":selected}),
                &Deadline::new(30_000)
            )
            .is_err()
        );
        assert_eq!(
            engine
                .store
                .db
                .query_row(
                    "SELECT COUNT(*) FROM records WHERE kind='plan'",
                    [],
                    |row| row.get::<_, usize>(0)
                )
                .unwrap(),
            count
        );
        assert_eq!(
            fs::read(root.path().join(&selected)).unwrap(),
            source.as_bytes()
        );
    }
}
