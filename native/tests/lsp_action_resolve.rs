use serde_json::{Value, json};
use std::{fs, path::Path};
use yoke::{Engine, capture::Deadline, lsp};

const SOURCE: &str = "fn helper() {}\n";
fn fixture(mode: &str) -> (tempfile::TempDir, Engine, lsp::ConfiguredServer) {
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("main.rs"), SOURCE).unwrap();
    let engine = Engine::open(root.path()).unwrap();
    let server = lsp::ConfiguredServer {
        server: lsp::Server {
            command: "node".into(),
            args: vec![
                Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("tests/fixtures/resolve-lsp.mjs")
                    .display()
                    .to_string(),
                mode.into(),
            ],
            language: "rust".into(),
            trust: true,
        },
        configuration: lsp::HostConfiguration {
            passive_profile: Some(lsp::PassiveProfile::ContractFixture),
            ..Default::default()
        },
    };
    (root, engine, server)
}
fn invoke(engine: &mut Engine, server: &lsp::ConfiguredServer) -> anyhow::Result<Value> {
    let deadline = Deadline::new(20000);
    let (snapshot, graph, _, _) = engine.index(&deadline)?;
    lsp::run_at_configured(
        engine,
        server,
        &snapshot,
        &graph,
        lsp::SemanticRequest {
            path: "main.rs",
            offset: 3,
            operation: "codeActions",
            new_name: None,
        },
        &json!({"only":["refactor.inline"]}),
        &deadline,
    )
}
fn plans(engine: &Engine) -> i64 {
    engine
        .store
        .db
        .query_row(
            "SELECT count(*) FROM records WHERE kind='plan'",
            [],
            |row| row.get(0),
        )
        .unwrap()
}

#[test]
fn lazy_code_actions_preserve_opaque_identity_and_prepare_checked_edits_before_owned_shutdown() {
    for mode in ["", "metadata-only"] {
        let (root, mut engine, server) = fixture(mode);
        let value = invoke(&mut engine, &server).unwrap();
        assert_eq!(
            value["items"][0]["usable"], true,
            "Lazy WorkspaceEdit was not resolved: {value}"
        );
        assert_eq!(value["items"][0]["resolution"], "resolved");
        let plan = &value["items"][0]["plan"];
        let text = engine
            .invoke(
                "ci_read",
                json!({"snapshotId":plan["resultSnapshotId"],"path":"main.rs"}),
            )
            .unwrap();
        assert_eq!(text["data"]["text"], "fn renamed() {}\n");
        assert_eq!(
            fs::read_to_string(root.path().join("main.rs")).unwrap(),
            SOURCE
        );
        assert_eq!(plans(&engine), 1);
    }
}

#[test]
fn resolution_refuses_changed_action_identity_foreign_ranges_and_materialized_source_mutation() {
    for mode in [
        "changed-title",
        "changed-kind",
        "changed-data",
        "foreign-edit",
        "invalid-range",
        "source-mutation",
    ] {
        let (root, mut engine, server) = fixture(mode);
        let result = invoke(&mut engine, &server);
        assert!(
            result.is_err(),
            "Accepted invalid resolved action {mode}: {result:?}"
        );
        assert_eq!(
            plans(&engine),
            0,
            "Invalid resolved action published a plan: {mode}"
        );
        assert_eq!(
            fs::read_to_string(root.path().join("main.rs")).unwrap(),
            SOURCE
        );
    }
}

#[test]
fn unavailable_disabled_command_and_unrequested_actions_do_not_invoke_resolution() {
    for mode in [
        "unadvertised",
        "disabled",
        "command-only",
        "unrequested-kind",
    ] {
        let (_root, mut engine, server) = fixture(mode);
        let value = invoke(&mut engine, &server).unwrap();
        if mode == "unrequested-kind" || mode == "command-only" {
            assert!(value["items"].as_array().unwrap().is_empty());
        } else {
            assert_eq!(value["items"][0]["usable"], false);
        }
        assert_eq!(plans(&engine), 0);
    }
}

#[test]
fn deferred_edit_with_a_follow_up_command_is_resolved_but_cannot_prepare_a_partial_action() {
    let (_root, mut engine, server) = fixture("deferred-command");
    let value = invoke(&mut engine, &server).unwrap();
    assert_eq!(value["items"][0]["resolution"], "resolved");
    assert_eq!(value["items"][0]["hasWorkspaceEdit"], true);
    assert_eq!(value["items"][0]["usable"], false);
    assert_eq!(value["items"][0]["commandExecution"], "refused");
    assert!(value["items"][0].get("plan").is_none());
    assert_eq!(plans(&engine), 0);
}

#[test]
fn action_context_preserves_validated_provider_diagnostic_data_and_refuses_outside_ranges() {
    let (_root, mut engine, server) = fixture("diagnostic-context");
    let value = invoke(&mut engine, &server).unwrap();
    assert_eq!(value["items"][0]["usable"], true);
    assert_eq!(value["diagnosticContext"]["source"], "pull");
    assert_eq!(value["diagnosticContext"]["count"], 1);
    let (_root, mut engine, server) = fixture("diagnostic-context-invalid");
    assert!(invoke(&mut engine, &server).is_err());
    assert_eq!(plans(&engine), 0);
}

#[test]
#[ignore = "requires YOKE_TS_LSP selecting the actual installed TypeScript language server"]
fn actual_typescript_lazy_fix_all_preserves_the_unexecuted_auxiliary_command_boundary() {
    let cli = std::env::var("YOKE_TS_LSP").expect("Select actual TypeScript LSP CLI");
    assert!(Path::new(&cli).is_absolute() && Path::new(&cli).is_file());
    let root = tempfile::tempdir().unwrap();
    let source = "export function one() { await Promise.resolve(1); }\nexport function two() { await Promise.resolve(2); }\n";
    fs::write(root.path().join("main.ts"), source).unwrap();
    fs::write(
        root.path().join("tsconfig.json"),
        "{\"compilerOptions\":{\"strict\":true,\"target\":\"ES2020\"},\"include\":[\"*.ts\"]}",
    )
    .unwrap();
    let mut engine = Engine::open(root.path()).unwrap();
    let deadline = Deadline::new(120000);
    let (snapshot, graph, _, _) = engine.index(&deadline).unwrap();
    let configured = lsp::ConfiguredServer {
        server: lsp::Server {
            command: "node".into(),
            args: vec![cli, "--stdio".into()],
            language: "typescript".into(),
            trust: true,
        },
        configuration: lsp::HostConfiguration {
            passive_profile: Some(lsp::PassiveProfile::TypescriptLanguageServer),
            ..Default::default()
        },
    };
    let value = lsp::run_at_configured(
        &mut engine,
        &configured,
        &snapshot,
        &graph,
        lsp::SemanticRequest {
            path: "main.ts",
            offset: 0,
            operation: "codeActions",
            new_name: None,
        },
        &json!({"startByte":0,"endByte":source.len(),"only":["quickfix"]}),
        &deadline,
    )
    .unwrap();
    let actions = value["items"].as_array().unwrap();
    let resolved = actions
        .iter()
        .find(|item| item["resolution"] == "resolved" && item["hasWorkspaceEdit"] == true)
        .unwrap_or_else(|| {
            panic!(
                "Actual TypeScript server did not expose a genuinely resolved fix-all edit: {value}"
            )
        });
    assert_eq!(resolved["usable"], false);
    assert_eq!(resolved["commandExecution"], "refused");
    assert!(resolved.get("plan").is_none());
    assert_eq!(plans(&engine), 0);
    assert_eq!(value["snapshotId"], snapshot.id);
    assert!(
        value["serverProfileHash"]
            .as_str()
            .is_some_and(yoke::model::valid_id)
    );
    assert_eq!(
        fs::read_to_string(root.path().join("main.ts")).unwrap(),
        source
    );
}
