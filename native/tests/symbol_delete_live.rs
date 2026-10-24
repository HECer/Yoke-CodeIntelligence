//! Explicit installed compiler/provider checks; no fixture fallback.
use serde_json::json;
use std::{fs, path::Path};
use yoke::{Engine, capture::Deadline, changes, lsp};

#[test]
#[ignore = "requires YOKE_TS_LSP and YOKE_TSC selecting actual installed TypeScript tools"]
fn actual_typescript_and_javascript_exported_symbols_compile_after_delete_and_recover_exactly() {
    let cli =
        std::env::var("YOKE_TS_LSP").expect("Select the actual TypeScript language-server CLI");
    let compiler = std::env::var("YOKE_TSC").expect("Select the actual TypeScript compiler CLI");
    for path in [&cli, &compiler] {
        assert!(Path::new(path).is_absolute() && Path::new(path).is_file());
    }
    for (language, extension) in [("typescript", "ts"), ("javascript", "js")] {
        for export in ["export", "export default"] {
            let root = tempfile::tempdir().unwrap();
            let path = format!("module.{extension}");
            let source = format!(
                "// owned compiler fixture: café 🦀\r\n{export} function deleteMe() {{ return 1; }}\r\nexport const retained = 2;\r\n"
            );
            fs::write(root.path().join(&path), &source).unwrap();
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
                &json!({"relativePath":path,"namePath":"deleteMe"}),
                &Deadline::new(90_000),
            )
            .unwrap();
            println!("actual-{language}-{export}-symbol-delete-plan={value}");
            assert_eq!(
                fs::read(root.path().join(&path)).unwrap(),
                source.as_bytes()
            );
            assert_eq!(value["complete"], false);
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
            println!("actual-{language}-{export}-symbol-delete-verification={verification}");
            assert_eq!(
                verification["status"], "passed",
                "{language} {export}: {verification}"
            );
            changes::approve(&engine.store, id, false).unwrap();
            let receipt = changes::apply(
                &engine.store,
                root.path(),
                &json!({"planId":id,"idempotencyKey":"actual-symbol-delete","mode":"workspace"}),
                true,
                &Deadline::new(30_000),
            )
            .unwrap();
            assert_eq!(
                fs::read_to_string(root.path().join(&path)).unwrap(),
                "// owned compiler fixture: café 🦀\r\n\r\nexport const retained = 2;\r\n"
            );
            changes::recover(&engine.store, root.path(), &receipt.id, true).unwrap();
            assert_eq!(
                fs::read(root.path().join(&path)).unwrap(),
                source.as_bytes()
            );
        }
    }
}
