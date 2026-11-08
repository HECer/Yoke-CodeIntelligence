use serde_json::json;
use std::{fs, path::Path, sync::Mutex};
use tempfile::TempDir;
use yoke::{Engine, capture::Deadline, lsp::Server};

static FIXTURE: Mutex<()> = Mutex::new(());
const SOURCE: &str = "// 🌍\nfn helper() {}\nfn main() { helper(); }\n";
fn server(adversary: &str) -> Server {
    Server {
        command: "node".into(),
        args: vec![
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("tests/fixtures/advanced-lsp.mjs")
                .display()
                .to_string(),
            adversary.into(),
        ],
        language: "rust".into(),
        trust: true,
    }
}
fn invoke(operation: &str, adversary: &str) -> anyhow::Result<serde_json::Value> {
    let root = TempDir::new()?;
    fs::write(root.path().join("main.rs"), SOURCE)?;
    let mut engine = Engine::open(root.path())?;
    let value = lsp::run(
        &mut engine,
        &server(adversary),
        "main.rs",
        44,
        operation,
        None,
        &Deadline::new(20000),
    )?;
    assert_eq!(fs::read_to_string(root.path().join("main.rs"))?, SOURCE);
    Ok(value)
}

#[test]
fn canonical_symbols_have_checked_anchors_and_refuse_escaped_selections() {
    let _fixture = FIXTURE.lock().unwrap_or_else(|e| e.into_inner());
    let value = invoke("symbols", "").unwrap();
    assert_eq!(value["items"][0]["selectionStartByte"], 11);
    assert!(invoke("symbols", "selection-outside").is_err());
}

#[test]
fn completion_insert_replace_edits_require_nested_ranges_and_string_replacements() {
    let _fixture = FIXTURE.lock().unwrap_or_else(|e| e.into_inner());
    for adversary in [
        "insert-replace-start",
        "insert-replace-end",
        "completion-invalid-text",
    ] {
        assert!(
            invoke("completion", adversary).is_err(),
            "Accepted {adversary}"
        );
    }
}

#[test]
fn diagnostic_notifications_have_a_cumulative_retention_budget() {
    let _fixture = FIXTURE.lock().unwrap_or_else(|e| e.into_inner());
    let error = invoke("typeDefinition", "diagnostic-flood").unwrap_err();
    assert!(
        error
            .to_string()
            .contains("Diagnostic notification byte budget"),
        "{error:#}"
    );
}

#[test]
fn configuration_callbacks_are_bounded_before_host_settings_are_replicated() {
    let _fixture = FIXTURE.lock().unwrap_or_else(|e| e.into_inner());
    let root = TempDir::new().unwrap();
    fs::write(root.path().join("main.rs"), SOURCE).unwrap();
    let mut engine = Engine::open(root.path()).unwrap();
    let deadline = Deadline::new(20000);
    let (snapshot, graph, _, _) = engine.index(&deadline).unwrap();
    let configured = lsp::ConfiguredServer {
        server: server("configuration-flood"),
        configuration: lsp::HostConfiguration {
            passive_profile: Some(lsp::PassiveProfile::ContractFixture),
            settings: json!({"payload":"x".repeat(60000)}),
            ..Default::default()
        },
    };
    let error = lsp::run_at_configured(
        &mut engine,
        &configured,
        &snapshot,
        &graph,
        lsp::SemanticRequest {
            path: "main.rs",
            offset: 44,
            operation: "typeDefinition",
            new_name: None,
        },
        &json!({}),
        &deadline,
    )
    .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("Configuration callback item budget"),
        "{error:#}"
    );
}

#[test]
fn semantic_profiles_distinguish_host_settings_for_identical_source_results() {
    let _fixture = FIXTURE.lock().unwrap_or_else(|e| e.into_inner());
    let root = TempDir::new().unwrap();
    fs::write(root.path().join("main.rs"), SOURCE).unwrap();
    let mut engine = Engine::open(root.path()).unwrap();
    let deadline = Deadline::new(20000);
    let (snapshot, graph, _, _) = engine.index(&deadline).unwrap();
    let mut hashes = Vec::new();
    for mode in ["first", "second"] {
        let configured = lsp::ConfiguredServer {
            server: server(""),
            configuration: lsp::HostConfiguration {
                passive_profile: Some(lsp::PassiveProfile::ContractFixture),
                settings: json!({"mode":mode}),
                ..Default::default()
            },
        };
        let value = lsp::run_at_configured(
            &mut engine,
            &configured,
            &snapshot,
            &graph,
            lsp::SemanticRequest {
                path: "main.rs",
                offset: 44,
                operation: "incomingCalls",
                new_name: None,
            },
            &json!({}),
            &deadline,
        )
        .unwrap();
        let hash = value["serverProfileHash"]
            .as_str()
            .expect("Missing semantic provider context identity");
        assert_eq!(hash.len(), 64);
        hashes.push(hash.to_owned());
    }
    assert_ne!(hashes[0], hashes[1]);
}

#[test]
fn type_definition_binds_the_requested_snapshot_and_utf8_name() {
    let _fixture = FIXTURE.lock().unwrap_or_else(|e| e.into_inner());
    let result = invoke("typeDefinition", "").unwrap();
    assert_eq!(result["locations"][0]["startByte"], 11);
    assert_eq!(result["locations"][0]["endByte"], 17);
    assert_eq!(result["locations"][0]["text"], "helper");
    assert!(invoke("typeDefinition", "unsupported").is_err());
}

#[test]
fn incoming_calls_preserve_parallel_call_evidence_and_opaque_provider_data() {
    let _fixture = FIXTURE.lock().unwrap_or_else(|e| e.into_inner());
    let result = invoke("incomingCalls", "").unwrap();
    assert_eq!(result["items"][0]["from"]["name"], "main");
    let ranges = result["items"][0]["fromRanges"].as_array().unwrap();
    assert_eq!(ranges.len(), 2);
    assert_eq!(ranges[0]["startByte"], 35);
    assert_eq!(ranges[0]["endByte"], 41);
    assert_eq!(
        ranges[0]["contentHash"],
        result["items"][0]["from"]["contentHash"]
    );
    assert_eq!(result["complete"], false);
    assert!(invoke("incomingCalls", "outside").is_err());
}

#[test]
fn type_hierarchies_return_checked_source_ranges() {
    let _fixture = FIXTURE.lock().unwrap_or_else(|e| e.into_inner());
    for operation in ["supertypes", "subtypes"] {
        let result = invoke(operation, "").unwrap();
        assert_eq!(result["items"][0]["name"], "main");
        assert_eq!(result["items"][0]["startByte"], 23);
        assert_eq!(result["items"][0]["selectionStartByte"], 26);
    }
}

#[test]
fn normalized_symbols_refuse_selection_ranges_outside_the_symbol() {
    let _fixture = FIXTURE.lock().unwrap_or_else(|e| e.into_inner());
    let result = invoke("symbolInfo", "").unwrap();
    assert_eq!(result["items"][0]["namePath"], "helper");
    assert_eq!(result["items"][0]["selectionStartByte"], 11);
    assert_eq!(
        result["items"][0]["contentHash"].as_str().unwrap().len(),
        64
    );
    assert!(invoke("symbolInfo", "selection-outside").is_err());
}

#[test]
fn diagnostic_ranges_and_completion_edits_are_source_checked() {
    let _fixture = FIXTURE.lock().unwrap_or_else(|e| e.into_inner());
    let diagnostics = invoke("diagnostics", "").unwrap();
    assert_eq!(diagnostics["items"][0]["startByte"], 35);
    assert_eq!(diagnostics["items"][0]["severity"], 2);
    let completion = invoke("completion", "").unwrap();
    assert_eq!(completion["items"][0]["textEdit"]["startByte"], 35);
    assert!(invoke("completion", "completion-outside").is_err());
}

#[test]
fn prepare_rename_validates_the_providers_exact_anchor() {
    let _fixture = FIXTURE.lock().unwrap_or_else(|e| e.into_inner());
    let result = invoke("prepareRename", "").unwrap();
    assert_eq!(result["range"]["startByte"], 11);
    assert_eq!(result["placeholder"], "helper");
}

#[test]
fn code_actions_prepare_immutable_plans_and_do_not_execute_server_commands() {
    let _fixture = FIXTURE.lock().unwrap_or_else(|e| e.into_inner());
    let result = invoke("codeActions", "").unwrap();
    assert_eq!(result["items"][0]["usable"], true);
    assert_eq!(result["items"][0]["plan"]["files"][0]["path"], "main.rs");
    let command = invoke("codeActions", "command-only").unwrap();
    assert_eq!(command["items"][0]["usable"], false);
    assert_eq!(command["items"][0]["commandExecution"], "refused");
}

#[test]
fn resource_rename_creates_a_guarded_delete_and_create_plan() {
    let _fixture = FIXTURE.lock().unwrap_or_else(|e| e.into_inner());
    let result = invoke("codeActions", "resource-rename").unwrap();
    let files = result["items"][0]["plan"]["files"].as_array().unwrap();
    assert_eq!(files.len(), 2);
    assert_eq!(files[0]["path"], "main.rs");
    assert_eq!(files[1]["path"], "renamed.rs");
}

#[test]
fn semantic_options_reject_unknown_authority_and_configured_settings_reach_the_server() {
    let _fixture = FIXTURE.lock().unwrap_or_else(|e| e.into_inner());
    let root = TempDir::new().unwrap();
    fs::write(root.path().join("main.rs"), SOURCE).unwrap();
    let mut engine = Engine::open(root.path()).unwrap();
    let deadline = Deadline::new(20000);
    let (snapshot, graph, _, _) = engine.index(&deadline).unwrap();
    let result = lsp::run_at_with_options(
        &mut engine,
        &server(""),
        &snapshot,
        &graph,
        lsp::SemanticRequest {
            path: "main.rs",
            offset: 44,
            operation: "typeDefinition",
            new_name: None,
        },
        &json!({"trustServer":true}),
        &deadline,
    );
    assert!(result.is_err());
    let configured = lsp::ConfiguredServer {
        server: server("host-settings"),
        configuration: lsp::HostConfiguration {
            passive_profile: Some(lsp::PassiveProfile::ContractFixture),
            settings: json!({"feature":{"enabled":true}}),
            ..Default::default()
        },
    };
    let result = lsp::run_at_configured(
        &mut engine,
        &configured,
        &snapshot,
        &graph,
        lsp::SemanticRequest {
            path: "main.rs",
            offset: 44,
            operation: "typeDefinition",
            new_name: None,
        },
        &json!({}),
        &deadline,
    )
    .unwrap();
    assert_eq!(result["locations"][0]["text"], "helper");
}

#[test]
fn push_only_diagnostics_preserve_actual_publication_and_reject_stale_versions() {
    let _fixture = FIXTURE.lock().unwrap_or_else(|e| e.into_inner());
    let result = invoke("diagnostics", "push-valid").unwrap();
    assert_eq!(result["diagnosticMode"], "push");
    assert_eq!(result["items"][0]["message"], "Published fixture warning");
    assert_eq!(result["items"][0]["startByte"], 35);
    assert!(invoke("diagnostics", "push-stale").is_err());
}

#[test]
fn hierarchy_rounds_retain_each_providers_data_and_stop_cycles() {
    let _fixture = FIXTURE.lock().unwrap_or_else(|e| e.into_inner());
    let root = TempDir::new().unwrap();
    fs::write(root.path().join("main.rs"), SOURCE).unwrap();
    let mut engine = Engine::open(root.path()).unwrap();
    let deadline = Deadline::new(20000);
    let (snapshot, graph, _, _) = engine.index(&deadline).unwrap();
    for (adversary, depth, count) in [("", 2, 1), ("hierarchy-cycle", 8, 2)] {
        let result = lsp::run_at_with_options(
            &mut engine,
            &server(adversary),
            &snapshot,
            &graph,
            lsp::SemanticRequest {
                path: "main.rs",
                offset: 44,
                operation: "incomingCalls",
                new_name: None,
            },
            &json!({"depth":depth}),
            &deadline,
        )
        .unwrap();
        assert_eq!(result["items"].as_array().unwrap().len(), count);
    }
    assert_eq!(
        fs::read_to_string(root.path().join("main.rs")).unwrap(),
        SOURCE
    );
}

#[test]
fn existing_loopback_tcp_provider_is_used_without_launching_or_stopping_its_process() {
    use std::{
        io::{BufReader, Write},
        net::TcpListener,
    };
    let _fixture = FIXTURE.lock().unwrap_or_else(|e| e.into_inner());
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let configuration: lsp::HostConfiguration = serde_json::from_value(
        json!({"tcpEndpoint":address.to_string(),"passiveProfile":"contract_fixture"}),
    )
    .unwrap();
    let provider = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(std::time::Duration::from_secs(20)))
            .unwrap();
        let mut reader = BufReader::new(stream.try_clone().unwrap());
        let mut document = serde_json::Value::Null;
        let mut methods = Vec::new();
        while let Ok(message) = lsp::read_frame(&mut reader) {
            let method = message["method"].as_str().unwrap_or("");
            methods.push(method.to_owned());
            let result = match method {
                "initialize" => Some(
                    json!({"capabilities":{"positionEncoding":"utf-16","typeDefinitionProvider":true}}),
                ),
                "textDocument/didOpen" => {
                    document = message["params"]["textDocument"]["uri"].clone();
                    None
                }
                "textDocument/typeDefinition" => Some(
                    json!({"uri":document,"range":{"start":{"line":1,"character":3},"end":{"line":1,"character":9}}}),
                ),
                _ => None,
            };
            if let Some(result) = result {
                let bytes = serde_json::to_vec(
                    &json!({"jsonrpc":"2.0","id":message["id"],"result":result}),
                )
                .unwrap();
                write!(stream, "Content-Length: {}\r\n\r\n", bytes.len()).unwrap();
                stream.write_all(&bytes).unwrap();
                stream.flush().unwrap();
            }
        }
        methods
    });
    let root = TempDir::new().unwrap();
    fs::write(root.path().join("main.rs"), SOURCE).unwrap();
    let mut engine = Engine::open(root.path()).unwrap();
    let deadline = Deadline::new(20000);
    let (snapshot, graph, _, _) = engine.index(&deadline).unwrap();
    let configured = lsp::ConfiguredServer {
        server: Server {
            command: "must-not-spawn-a-provider.exe".into(),
            args: vec![],
            language: "rust".into(),
            trust: true,
        },
        configuration,
    };
    let result = lsp::run_at_configured(
        &mut engine,
        &configured,
        &snapshot,
        &graph,
        lsp::SemanticRequest {
            path: "main.rs",
            offset: 44,
            operation: "typeDefinition",
            new_name: None,
        },
        &json!({}),
        &deadline,
    )
    .unwrap();
    assert_eq!(result["locations"][0]["text"], "helper");
    let methods = provider.join().unwrap();
    assert!(
        !methods
            .iter()
            .any(|method| method == "shutdown" || method == "exit")
    );
    assert_eq!(
        fs::read_to_string(root.path().join("main.rs")).unwrap(),
        SOURCE
    );
}

#[cfg(windows)]
#[test]
fn published_diagnostic_uri_drive_spelling_preserves_the_captured_file_identity() {
    let _fixture = FIXTURE.lock().unwrap_or_else(|e| e.into_inner());
    let root = TempDir::new().unwrap();
    fs::write(root.path().join("main.rs"), SOURCE).unwrap();
    let mut engine = Engine::open(root.path()).unwrap();
    let result = lsp::run(
        &mut engine,
        &server("push-upper-drive"),
        "main.rs",
        44,
        "diagnostics",
        None,
        &Deadline::new(3000),
    )
    .unwrap();
    assert_eq!(result["items"][0]["path"], "main.rs");
    assert_eq!(result["items"][0]["startByte"], 35);
}

#[test]
fn semantic_rename_resource_operations_and_following_text_edits_share_one_guarded_plan() {
    let _fixture = FIXTURE.lock().unwrap_or_else(|e| e.into_inner());
    let root = TempDir::new().unwrap();
    fs::write(root.path().join("main.rs"), SOURCE).unwrap();
    let mut engine = Engine::open(root.path()).unwrap();
    let result = lsp::run(
        &mut engine,
        &server(""),
        "main.rs",
        11,
        "rename",
        Some("renamed"),
        &Deadline::new(20000),
    )
    .unwrap();
    assert_eq!(result["plan"]["files"].as_array().unwrap().len(), 2);
    let read = engine
        .invoke(
            "ci_read",
            json!({"snapshotId":result["plan"]["resultSnapshotId"],"path":"renamed.rs"}),
        )
        .unwrap();
    assert!(
        read["data"]["text"]
            .as_str()
            .unwrap()
            .contains("fn renamed")
    );
    assert_eq!(
        fs::read_to_string(root.path().join("main.rs")).unwrap(),
        SOURCE
    );
    assert!(!root.path().join("renamed.rs").exists());
}

#[path = "support/lsp_fixture.rs"]
mod lsp;
