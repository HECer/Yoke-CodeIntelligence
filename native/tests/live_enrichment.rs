// Actual installed-server gates. There is no protocol-fixture fallback.
use serde_json::Value;
use std::{fs, path::Path};
use tempfile::TempDir;
use yoke::{
    Engine,
    capture::Deadline,
    lsp::{self, ConfiguredServer, HostConfiguration, PassiveProfile, SemanticRequest, Server},
};

fn query(
    engine: &mut Engine,
    server: &Server,
    path: &str,
    offset: usize,
    operation: &str,
) -> Value {
    // Each test below selects this exact installed provider, not a generic backend.
    let passive_profile = match server.language.as_str() {
        "rust" => PassiveProfile::RustAnalyzer,
        "typescript" => PassiveProfile::TypescriptLanguageServer,
        "python" => PassiveProfile::JediLanguageServer,
        _ => panic!("No actual passive provider gate declared for this language"),
    };
    let deadline = Deadline::new(120000);
    let (snapshot, graph, _, _) = engine.index(&deadline).unwrap();
    lsp::run_at_configured(
        engine,
        &ConfiguredServer {
            server: server.clone(),
            configuration: HostConfiguration {
                passive_profile: Some(passive_profile),
                ..Default::default()
            },
        },
        &snapshot,
        &graph,
        SemanticRequest {
            path,
            offset,
            operation,
            new_name: None,
        },
        &serde_json::json!({}),
        &deadline,
    )
    .unwrap_or_else(|error| panic!("Actual {operation} gate failed: {error:#}"))
}

#[test]
#[ignore = "requires an actual installed rust-analyzer"]
fn actual_rust_server_expanded_call_symbols_completion_signature_diagnostics_and_prepare_rename() {
    let root = TempDir::new().unwrap();
    fs::create_dir(root.path().join("src")).unwrap();
    fs::write(
        root.path().join("Cargo.toml"),
        "[package]\nname = \"yoke_expanded_live\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    )
    .unwrap();
    let text = "// 🌍 actual Rust gate\nfn helper(value: i32) -> i32 { value + 1 }\nfn caller() -> i32 { helper(2) }\nfn main() { let _ = caller(); }\n";
    fs::write(root.path().join("src/main.rs"), text).unwrap();
    let mut engine = Engine::open(root.path()).unwrap();
    let server = Server {
        command: "rust-analyzer".into(),
        args: vec![],
        language: "rust".into(),
        trust: true,
    };
    let helper = text.find("helper").unwrap();
    let caller = text.find("caller").unwrap();
    let call = text.rfind("helper(").unwrap();
    let incoming = query(&mut engine, &server, "src/main.rs", helper, "incomingCalls");
    assert!(
        incoming["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["from"]["name"] == "caller"),
        "{incoming}"
    );
    let outgoing = query(&mut engine, &server, "src/main.rs", caller, "outgoingCalls");
    assert!(
        outgoing["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["to"]["name"] == "helper"),
        "{outgoing}"
    );
    let symbols = query(&mut engine, &server, "src/main.rs", helper, "symbolInfo");
    assert!(
        symbols["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["name"] == "helper"),
        "{symbols}"
    );
    let completion = query(&mut engine, &server, "src/main.rs", call + 3, "completion");
    assert!(
        !completion["items"].as_array().unwrap().is_empty(),
        "{completion}"
    );
    let signature = query(
        &mut engine,
        &server,
        "src/main.rs",
        call + 7,
        "signatureHelp",
    );
    assert!(
        signature["response"]["signatures"]
            .as_array()
            .is_some_and(|items| items.iter().any(|item| item["label"]
                .as_str()
                .is_some_and(|label| label.contains("helper")))),
        "{signature}"
    );
    let prepare = query(&mut engine, &server, "src/main.rs", helper, "prepareRename");
    assert_eq!(prepare["range"]["text"], "helper", "{prepare}");
    let diagnostics = query(&mut engine, &server, "src/main.rs", helper, "diagnostics");
    assert!(diagnostics["items"].is_array(), "{diagnostics}");
    assert!(matches!(
        diagnostics["diagnosticMode"].as_str(),
        Some("push" | "pull")
    ));
    assert_eq!(
        fs::read_to_string(root.path().join("src/main.rs")).unwrap(),
        text
    );
}

#[test]
#[ignore = "requires YOKE_TS_LSP identifying the actual installed TypeScript language-server CLI"]
fn actual_typescript_server_expanded_type_call_symbols_signature_and_published_diagnostics() {
    let cli = std::env::var("YOKE_TS_LSP").expect("Set YOKE_TS_LSP to the actual server CLI");
    assert!(Path::new(&cli).is_absolute() && Path::new(&cli).is_file());
    let root = TempDir::new().unwrap();
    fs::write(
        root.path().join("tsconfig.json"),
        "{\"compilerOptions\":{\"strict\":true},\"include\":[\"*.ts\"]}",
    )
    .unwrap();
    let text = "// 🌍 actual TypeScript gate\nexport class Value {}\nexport function helper(value: Value): Value { return value; }\nexport function caller(): Value { return helper(new Value()); }\n";
    fs::write(root.path().join("main.ts"), text).unwrap();
    let mut engine = Engine::open(root.path()).unwrap();
    let server = Server {
        command: "node".into(),
        args: vec![cli, "--stdio".into()],
        language: "typescript".into(),
        trust: true,
    };
    let helper = text.find("helper").unwrap();
    let caller = text.find("caller").unwrap();
    let call = text.rfind("helper(").unwrap();
    let variable = text.find("value:").unwrap();
    let definition = query(&mut engine, &server, "main.ts", variable, "typeDefinition");
    assert!(
        definition["locations"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["text"] == "Value"),
        "{definition}"
    );
    let incoming = query(&mut engine, &server, "main.ts", helper, "incomingCalls");
    assert!(
        incoming["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["from"]["name"] == "caller"),
        "{incoming}"
    );
    let outgoing = query(&mut engine, &server, "main.ts", caller, "outgoingCalls");
    assert!(
        outgoing["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["to"]["name"] == "helper"),
        "{outgoing}"
    );
    let symbols = query(&mut engine, &server, "main.ts", helper, "symbolInfo");
    assert!(
        symbols["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["name"] == "helper"),
        "{symbols}"
    );
    let signature = query(&mut engine, &server, "main.ts", call + 7, "signatureHelp");
    assert!(
        signature["response"]["signatures"]
            .as_array()
            .is_some_and(|items| !items.is_empty()),
        "{signature}"
    );
    let diagnostics = query(&mut engine, &server, "main.ts", helper, "diagnostics");
    assert_eq!(diagnostics["diagnosticMode"], "push", "{diagnostics}");
    assert!(diagnostics["items"].is_array());
    assert_eq!(
        fs::read_to_string(root.path().join("main.ts")).unwrap(),
        text
    );
}

#[test]
#[ignore = "requires YOKE_PYTHON_LSP identifying the actual installed Jedi language-server"]
fn actual_python_server_normalized_symbols_and_signature_help() {
    let command =
        std::env::var("YOKE_PYTHON_LSP").expect("Set YOKE_PYTHON_LSP to the actual server");
    assert!(Path::new(&command).is_absolute() && Path::new(&command).is_file());
    let root = TempDir::new().unwrap();
    let text = "# 🌍 actual Python gate\ndef café(value: int) -> int:\n    return value + 1\n\nresult = café(2)\n";
    fs::write(root.path().join("main.py"), text).unwrap();
    let mut engine = Engine::open(root.path()).unwrap();
    let server = Server {
        command,
        args: vec![],
        language: "python".into(),
        trust: true,
    };
    let symbols = query(
        &mut engine,
        &server,
        "main.py",
        text.find("café").unwrap(),
        "symbolInfo",
    );
    assert!(
        symbols["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["name"] == "café"),
        "{symbols}"
    );
    let signature = query(
        &mut engine,
        &server,
        "main.py",
        text.rfind("café(").unwrap() + 6,
        "signatureHelp",
    );
    assert!(
        signature["response"]["signatures"]
            .as_array()
            .is_some_and(|items| !items.is_empty()),
        "{signature}"
    );
    assert_eq!(
        fs::read_to_string(root.path().join("main.py")).unwrap(),
        text
    );
}
