//! Native session/CLI semantic planning; owned protocol fixtures are not compilers.
use serde_json::{Value, json};
use std::{fs, path::Path};
use yoke::{
    Engine,
    analysis::AnalyzerEpoch,
    capture::Deadline,
    lsp,
    project_session::{HostProfile, Session},
};

const SOURCE: &str = "\u{feff}// owned café 🦀\r\nfunction dependency() { return 1; }\r\nexport function removeMe() { return dependency(); }\r\nexport function keep() { void (removeMe()); return 2; }\r\n";
const AFTER: &str = "\u{feff}// owned café 🦀\r\n\r\n\r\nexport function keep() {  return 2; }\r\n";

fn d() -> Deadline {
    Deadline::new(30_000)
}
struct Fixture {
    session: Session,
    root: tempfile::TempDir,
    inputs: tempfile::TempDir,
}
fn fixture(epoch: AnalyzerEpoch, host: HostProfile) -> Fixture {
    let root = tempfile::tempdir().unwrap();
    let inputs = tempfile::tempdir().unwrap();
    fs::write(root.path().join("module.ts"), SOURCE).unwrap();
    let mut engine = Engine::open(root.path()).unwrap();
    engine.capture_epoch = epoch;
    engine
        .register_configured_server(lsp::ConfiguredServer {
            server: lsp::Server {
                command: "node".into(),
                args: vec![
                    Path::new(env!("CARGO_MANIFEST_DIR"))
                        .join("tests/fixtures/propagated-delete-lsp.mjs")
                        .display()
                        .to_string(),
                    inputs.path().join("protocol.log").display().to_string(),
                    "ok".into(),
                    root.path().join("module.ts").display().to_string(),
                ],
                language: "typescript".into(),
                trust: true,
            },
            configuration: lsp::HostConfiguration {
                passive_profile: Some(lsp::PassiveProfile::ContractFixture),
                ..Default::default()
            },
        })
        .unwrap();
    Fixture {
        session: Session::open(engine, host, Some("native-refactor"), &d()).unwrap(),
        root,
        inputs,
    }
}
fn request() -> Value {
    json!({"operation":"safeDelete","arguments":{"relativePath":"module.ts","namePath":"removeMe","propagate":true,"propagationPolicy":"captured_es_function_propagation_v2"}})
}
fn transcript(f: &Fixture) -> String {
    fs::read_to_string(f.inputs.path().join("protocol.log")).unwrap_or_default()
}
fn unpublished(f: &Fixture) {
    assert_eq!(f.session.engine.store.db.query_row("SELECT COUNT(*) FROM records WHERE kind IN ('plan','approval','receipt','journal')",[],|row|row.get::<_,usize>(0)).unwrap(),0);
    assert_eq!(
        fs::read(f.root.path().join("module.ts")).unwrap(),
        SOURCE.as_bytes()
    );
}

#[test]
fn session_refactor_prepares_the_native_plan_without_steering_or_source_writes() {
    for epoch in [AnalyzerEpoch::V8, AnalyzerEpoch::V9] {
        let mut f = fixture(epoch, HostProfile::default());
        let before = f.session.invoke("configuration", &json!({}), &d()).unwrap();
        let value = f
            .session
            .invoke("refactor.prepare", &request(), &d())
            .unwrap();
        assert_eq!(
            value["propagation"]["policy"],
            "captured_es_function_propagation_v2"
        );
        assert_eq!(value["complete"], false);
        assert_eq!(value["commandsExecuted"], false);
        assert_eq!(
            f.session.invoke("configuration", &json!({}), &d()).unwrap(),
            before
        );
        let result = f
            .session
            .engine
            .store
            .snapshot(value["plan"]["resultSnapshotId"].as_str().unwrap())
            .unwrap();
        assert_eq!(
            yoke::query::source(&f.session.engine.store, &result, "module.ts")
                .unwrap()
                .0,
            AFTER
        );
        assert_eq!(
            fs::read(f.root.path().join("module.ts")).unwrap(),
            SOURCE.as_bytes()
        );
        assert!(transcript(&f).contains("textDocument/references"));
        assert_eq!(
            f.session
                .engine
                .store
                .db
                .query_row(
                    "SELECT COUNT(*) FROM records WHERE kind IN ('approval','receipt','journal')",
                    [],
                    |row| row.get::<_, usize>(0)
                )
                .unwrap(),
            0
        );
    }
}

#[test]
fn session_refactor_rejects_unknown_envelopes_operations_and_authority_fields() {
    for request in [
        Value::Null,
        json!([]),
        json!({}),
        json!({"operation":false,"arguments":{}}),
        json!({"operation":"apply","arguments":{}}),
        json!({"operation":"inspections","arguments":{}}),
        json!({"operation":"safeDelete","arguments":null}),
        json!({"operation":"safeDelete","arguments":[]}),
        json!({"operation":"safeDelete","arguments":{},"project":"outside"}),
        json!({"operation":"safeDelete","arguments":{},"modes":["editing"]}),
        json!({"operation":"safeDelete","arguments":{},"command":"untrusted"}),
    ] {
        let mut f = fixture(AnalyzerEpoch::V9, HostProfile::default());
        assert!(
            f.session
                .invoke("refactor.prepare", &request, &d())
                .is_err(),
            "{request}"
        );
        unpublished(&f);
        assert!(transcript(&f).is_empty());
    }
    for operation in ["safeDelete", "rename", "inline", "move"] {
        let mut f = fixture(AnalyzerEpoch::V9, HostProfile::default());
        let result = f.session.invoke(
            "refactor.prepare",
            &json!({"operation":operation,"arguments":{"allowWrites":true}}),
            &d(),
        );
        assert!(
            format!("{:#}", result.unwrap_err()).contains("Unknown semantic refactoring argument")
        );
        unpublished(&f);
        assert!(transcript(&f).is_empty());
    }
    for field in ["project", "modes", "command", "root", "allowWrites"] {
        let mut f = fixture(AnalyzerEpoch::V9, HostProfile::default());
        let mut call = request();
        call[field] = json!("untrusted override");
        assert!(f.session.invoke("refactor.prepare", &call, &d()).is_err());
        unpublished(&f);
        assert!(transcript(&f).is_empty());
    }
}

#[test]
fn session_refactor_checks_modes_read_only_projects_and_original_deadline() {
    for modes in [
        vec!["planning".into()],
        vec!["interactive".into()],
        vec!["editing".into(), "planning".into()],
    ] {
        let mut f = fixture(
            AnalyzerEpoch::V9,
            HostProfile {
                modes,
                ..Default::default()
            },
        );
        assert!(
            f.session
                .invoke("refactor.prepare", &request(), &d())
                .is_err()
        );
        unpublished(&f);
        assert!(transcript(&f).is_empty());
    }
    let secondary = tempfile::tempdir().unwrap();
    fs::write(secondary.path().join("module.ts"), SOURCE).unwrap();
    let mut f = fixture(
        AnalyzerEpoch::V9,
        HostProfile {
            projects: vec![yoke::project_session::HostProject {
                name: "reference".into(),
                root: secondary.path().into(),
                read_only: true,
                query_only: false,
            }],
            ..Default::default()
        },
    );
    f.session
        .invoke("activate", &json!({"project":"reference"}), &d())
        .unwrap();
    assert!(
        f.session
            .invoke("refactor.prepare", &request(), &d())
            .is_err()
    );
    unpublished(&f);
    assert!(transcript(&f).is_empty());
    assert_eq!(
        fs::read(secondary.path().join("module.ts")).unwrap(),
        SOURCE.as_bytes()
    );
    let mut f = fixture(AnalyzerEpoch::V9, HostProfile::default());
    assert!(
        f.session
            .invoke("refactor.prepare", &request(), &Deadline::new(0))
            .is_err()
    );
    unpublished(&f);
    assert!(transcript(&f).is_empty());
}

#[test]
fn session_refactor_refuses_a_session_changed_by_another_client_before_provider_start() {
    let mut f = fixture(AnalyzerEpoch::V9, HostProfile::default());
    let mut engine = Engine::open(f.root.path()).unwrap();
    engine.capture_epoch = AnalyzerEpoch::V9;
    engine.servers = f.session.engine.servers.clone();
    engine.server_configurations = f.session.engine.server_configurations.clone();
    let mut second = Session::open(
        engine,
        HostProfile::default(),
        Some("native-refactor"),
        &d(),
    )
    .unwrap();
    second
        .invoke("switchModes", &json!({"modes":["planning"]}), &d())
        .unwrap();
    drop(second);
    let result = f.session.invoke("refactor.prepare", &request(), &d());
    assert!(format!("{:#}", result.unwrap_err()).contains("Session state changed concurrently"));
    unpublished(&f);
    assert!(transcript(&f).is_empty());
}

#[test]
fn session_refactor_argument_budget_admits_64000_and_refuses_64001_without_clipping() {
    for size in [64_000, 64_001] {
        let mut f = fixture(AnalyzerEpoch::V9, HostProfile::default());
        let mut arguments = json!({"unknown":""});
        let overhead = serde_json::to_vec(&arguments).unwrap().len();
        arguments["unknown"] = json!("x".repeat(size - overhead));
        assert_eq!(serde_json::to_vec(&arguments).unwrap().len(), size);
        let result = f.session.invoke(
            "refactor.prepare",
            &json!({"operation":"safeDelete","arguments":arguments}),
            &d(),
        );
        let expected = if size == 64_000 {
            "Unknown semantic refactoring argument"
        } else {
            "Native refactoring input capacity exceeded"
        };
        assert!(format!("{:#}", result.unwrap_err()).contains(expected));
        unpublished(&f);
        assert!(transcript(&f).is_empty());
    }
}

#[test]
fn session_mcp_refactor_uses_the_same_guarded_native_result_and_rejects_extra_fields() {
    let mut f = fixture(AnalyzerEpoch::V9, HostProfile::default());
    let messages = [
        json!({"jsonrpc":"2.0","id":1,"method":"initialize"}),
        json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"ci_session","arguments":{"operation":"refactor.prepare","args":request()}}}),
        json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"ci_session","arguments":{"operation":"refactor.prepare","args":{"operation":"safeDelete","arguments":request()["arguments"],"project":"outside"}}}}),
        json!({"jsonrpc":"2.0","id":4,"method":"tools/list"}),
    ];
    let input = messages
        .iter()
        .map(|v| format!("{v}\n"))
        .collect::<String>();
    let mut output = Vec::new();
    yoke::protocol::serve_session(&mut f.session, std::io::Cursor::new(input), &mut output)
        .unwrap();
    let responses: Vec<Value> = String::from_utf8(output)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(responses[1]["result"]["isError"], false, "{}", responses[1]);
    let result: Value = serde_json::from_str(
        responses[1]["result"]["content"][0]["text"]
            .as_str()
            .unwrap(),
    )
    .unwrap();
    assert_eq!(
        result["data"]["propagation"]["policy"],
        "captured_es_function_propagation_v2"
    );
    assert_eq!(result["data"]["complete"], false);
    assert_eq!(responses[2]["result"]["isError"], true);
    let tools = responses[3]["result"]["tools"].as_array().unwrap();
    let session_tool = tools
        .iter()
        .find(|tool| tool["name"] == "ci_session")
        .unwrap();
    assert!(
        session_tool["description"]
            .as_str()
            .unwrap()
            .contains("refactor.prepare")
    );
    assert!(!tools.iter().any(|tool| tool["name"] == "ci_change_apply"));
    assert_eq!(
        fs::read(f.root.path().join("module.ts")).unwrap(),
        SOURCE.as_bytes()
    );
}

fn cli_inputs(
    inputs: &Path,
    language: &str,
    command: &str,
    args: Vec<String>,
    configuration: lsp::HostConfiguration,
    call: Value,
) {
    fs::write(inputs.join("servers.json"),serde_json::to_vec(&json!([{"language":language,"command":command,"args":args,"configuration":configuration}])).unwrap()).unwrap();
    fs::write(
        inputs.join("requests.json"),
        serde_json::to_vec(&json!([{"operation":"refactor.prepare","args":call}])).unwrap(),
    )
    .unwrap();
}
fn cli(root: &Path, inputs: &Path, trust: bool, host: bool) -> std::process::Output {
    let mut command = std::process::Command::new(env!("CARGO_BIN_EXE_yoke"));
    command
        .arg("--root")
        .arg(root)
        .args(["--timeout", "30000", "--language-server-config"])
        .arg(inputs.join("servers.json"));
    if trust {
        command.arg("--trust-servers");
    }
    command
        .args(["session", "--id", "native-refactor-cli", "--requests"])
        .arg(inputs.join("requests.json"));
    if host {
        command.arg("--host-profile").arg(inputs.join("host.json"));
    }
    command.output().unwrap()
}
fn owned_cli_inputs(root: &Path, inputs: &Path) {
    fs::write(root.join("module.ts"), SOURCE).unwrap();
    cli_inputs(
        inputs,
        "typescript",
        "node",
        vec![
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("tests/fixtures/propagated-delete-lsp.mjs")
                .display()
                .to_string(),
            inputs.join("protocol.log").display().to_string(),
            "ok".into(),
            root.join("module.ts").display().to_string(),
        ],
        lsp::HostConfiguration {
            passive_profile: Some(lsp::PassiveProfile::ContractFixture),
            ..Default::default()
        },
        request(),
    );
}

#[test]
fn native_session_cli_prepares_the_same_source_effect_without_write_or_apply_authority() {
    let root = tempfile::tempdir().unwrap();
    let inputs = tempfile::tempdir().unwrap();
    owned_cli_inputs(root.path(), inputs.path());
    let output = cli(root.path(), inputs.path(), true, false);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let result: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(result["sourceWritten"], false);
    assert_eq!(result["atomicBatch"], false);
    assert_eq!(
        result["results"][0]["propagation"]["policy"],
        "captured_es_function_propagation_v2"
    );
    let engine = Engine::open(root.path()).unwrap();
    let snapshot = engine
        .store
        .snapshot(
            result["results"][0]["plan"]["resultSnapshotId"]
                .as_str()
                .unwrap(),
        )
        .unwrap();
    assert_eq!(
        yoke::query::source(&engine.store, &snapshot, "module.ts")
            .unwrap()
            .0,
        AFTER
    );
    assert_eq!(
        engine
            .store
            .db
            .query_row(
                "SELECT COUNT(*) FROM records WHERE kind IN ('approval','receipt','journal')",
                [],
                |row| row.get::<_, usize>(0)
            )
            .unwrap(),
        0
    );
    assert_eq!(
        fs::read(root.path().join("module.ts")).unwrap(),
        SOURCE.as_bytes()
    );
}

#[test]
fn native_session_cli_requires_host_server_trust_modes_and_unambiguous_json() {
    for mode in ["untrusted", "planning", "duplicate"] {
        let root = tempfile::tempdir().unwrap();
        let inputs = tempfile::tempdir().unwrap();
        owned_cli_inputs(root.path(), inputs.path());
        if mode == "planning" {
            fs::write(
                inputs.path().join("host.json"),
                br#"{"modes":["planning"]}"#,
            )
            .unwrap();
        }
        if mode == "duplicate" {
            let path = inputs.path().join("requests.json");
            let raw = fs::read_to_string(&path).unwrap();
            let duplicated = raw.replacen(
                "\"operation\":\"refactor.prepare\"",
                "\"operation\":\"refactor.prepare\",\"operation\":\"refactor.prepare\"",
                1,
            );
            assert_ne!(duplicated, raw);
            fs::write(path, duplicated).unwrap();
        }
        let output = cli(
            root.path(),
            inputs.path(),
            mode != "untrusted",
            mode == "planning",
        );
        assert!(!output.status.success(), "{mode}");
        assert!(!inputs.path().join("protocol.log").exists(), "{mode}");
        assert_eq!(
            fs::read(root.path().join("module.ts")).unwrap(),
            SOURCE.as_bytes()
        );
        let engine = Engine::open(root.path()).unwrap();
        assert_eq!(engine.store.db.query_row("SELECT COUNT(*) FROM records WHERE kind IN ('plan','approval','receipt','journal')",[],|row|row.get::<_,usize>(0)).unwrap(),0);
    }
}

#[test]
fn session_refactor_preserves_native_opt_in_path_and_snapshot_preconditions() {
    let mut f = fixture(AnalyzerEpoch::V9, HostProfile::default());
    let mut call = request();
    call["arguments"]
        .as_object_mut()
        .unwrap()
        .remove("propagationPolicy");
    assert!(f.session.invoke("refactor.prepare", &call, &d()).is_err());
    unpublished(&f);
    assert!(!transcript(&f).is_empty());
    for path in [
        "../outside.ts",
        "/outside.ts",
        "C:\\outside.ts",
        "node_modules/module.ts",
    ] {
        let mut f = fixture(AnalyzerEpoch::V9, HostProfile::default());
        let mut call = request();
        call["arguments"]["relativePath"] = json!(path);
        assert!(f.session.invoke("refactor.prepare", &call, &d()).is_err());
        unpublished(&f);
        assert!(transcript(&f).is_empty());
    }
    let mut f = fixture(AnalyzerEpoch::V9, HostProfile::default());
    let snapshot = f.session.engine.index(&d()).unwrap().0;
    let mut call = request();
    call["arguments"]["snapshotId"] = json!(snapshot.id);
    let foreign = format!("// changed after the selected snapshot\n{SOURCE}");
    fs::write(f.root.path().join("module.ts"), &foreign).unwrap();
    let result = f.session.invoke("refactor.prepare", &call, &d());
    assert!(format!("{:#}", result.unwrap_err()).contains("snapshot is stale"));
    assert!(transcript(&f).is_empty());
    assert_eq!(
        fs::read(f.root.path().join("module.ts")).unwrap(),
        foreign.as_bytes()
    );
    assert_eq!(f.session.engine.store.db.query_row("SELECT COUNT(*) FROM records WHERE kind IN ('plan','approval','receipt','journal')",[],|row|row.get::<_,usize>(0)).unwrap(),0);
}

#[test]
#[ignore = "requires YOKE_NODE, YOKE_TS_LSP and YOKE_TSC selecting actual installed tools"]
fn actual_native_session_cli_preserves_compiler_approval_write_and_recovery_gates() {
    let node = std::env::var("YOKE_NODE").unwrap();
    let server = std::env::var("YOKE_TS_LSP").unwrap();
    let compiler = std::env::var("YOKE_TSC").unwrap();
    for path in [&node, &server, &compiler] {
        assert!(Path::new(path).is_absolute() && Path::new(path).is_file());
    }
    let executable = env!("CARGO_BIN_EXE_yoke");
    let executable_hash = yoke::model::hash(fs::read(executable).unwrap());
    println!("actual-native-session-cli-executable={executable} sha256={executable_hash}");
    for (language, extension) in [("typescript", "ts"), ("javascript", "js")] {
        let root = tempfile::tempdir().unwrap();
        let inputs = tempfile::tempdir().unwrap();
        let path = format!("module.{extension}");
        fs::write(root.path().join(&path), SOURCE).unwrap();
        fs::write(root.path().join("tsconfig.json"),serde_json::to_vec(&json!({"compilerOptions":{"strict":true,"module":"esnext","target":"es2022","allowJs":language=="javascript","checkJs":language=="javascript"},"include":[path]})).unwrap()).unwrap();
        let mut call = request();
        call["arguments"]["relativePath"] = json!(path);
        cli_inputs(
            inputs.path(),
            language,
            &node,
            vec![server.clone(), "--stdio".into()],
            lsp::HostConfiguration {
                passive_profile: Some(lsp::PassiveProfile::TypescriptLanguageServer),
                ..Default::default()
            },
            call,
        );
        let output = cli(root.path(), inputs.path(), true, false);
        assert!(
            output.status.success(),
            "{language}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let result: Value = serde_json::from_slice(&output.stdout).unwrap();
        println!("actual-native-session-{language}-plan={result}");
        assert_eq!(result["sourceWritten"], false);
        assert_eq!(result["results"][0]["complete"], false);
        assert_eq!(
            result["results"][0]["propagation"]["policy"],
            "captured_es_function_propagation_v2"
        );
        let id = result["results"][0]["plan"]["id"].as_str().unwrap();
        let engine = Engine::open(root.path()).unwrap();
        let apply = json!({"planId":id,"idempotencyKey":"actual-native-session-refactor","mode":"workspace"});
        assert!(yoke::changes::apply(&engine.store, root.path(), &apply, true, &d()).is_err());
        yoke::changes::approve(&engine.store, id, false).unwrap();
        assert!(yoke::changes::apply(&engine.store, root.path(), &apply, true, &d()).is_err());
        let command = vec![
            compiler.clone(),
            "--noEmit".into(),
            "--project".into(),
            "tsconfig.json".into(),
        ];
        assert!(yoke::verify::execute(&engine, id, &node, &command, false, &d()).is_err());
        let verification =
            yoke::verify::execute(&engine, id, &node, &command, true, &Deadline::new(60_000))
                .unwrap();
        println!("actual-native-session-{language}-verification={verification}");
        assert_eq!(verification["status"], "passed");
        assert!(yoke::changes::apply(&engine.store, root.path(), &apply, false, &d()).is_err());
        fs::write(root.path().join(&path), "// foreign modification\n").unwrap();
        assert!(yoke::changes::apply(&engine.store, root.path(), &apply, true, &d()).is_err());
        assert_eq!(
            fs::read_to_string(root.path().join(&path)).unwrap(),
            "// foreign modification\n"
        );
        fs::write(root.path().join(&path), SOURCE).unwrap();
        let receipt = yoke::changes::apply(&engine.store, root.path(), &apply, true, &d()).unwrap();
        assert_eq!(fs::read(root.path().join(&path)).unwrap(), AFTER.as_bytes());
        assert!(yoke::changes::recover(&engine.store, root.path(), &receipt.id, false).is_err());
        yoke::changes::recover(&engine.store, root.path(), &receipt.id, true).unwrap();
        assert_eq!(
            fs::read(root.path().join(&path)).unwrap(),
            SOURCE.as_bytes()
        );
        assert_eq!(
            yoke::model::hash(fs::read(executable).unwrap()),
            executable_hash
        );
    }
}

#[test]
#[ignore = "requires YOKE_NODE, YOKE_TS_LSP and YOKE_TSC selecting actual installed tools"]
fn actual_function_binding_policy_cli_compiles_ts_js_and_recovers_original_unicode_bytes() {
    let node = std::env::var("YOKE_NODE").unwrap();
    let server = std::env::var("YOKE_TS_LSP").unwrap();
    let compiler = std::env::var("YOKE_TSC").unwrap();
    for path in [&node, &server, &compiler] {
        assert!(Path::new(path).is_absolute() && Path::new(path).is_file());
    }
    let executable = env!("CARGO_BIN_EXE_yoke");
    let executable_hash = yoke::model::hash(fs::read(executable).unwrap());
    println!("actual-native-session-cli-executable={executable} sha256={executable_hash}");
    for (language, extension) in [("typescript", "ts"), ("javascript", "js")] {
        for (keyword, value) in [
            ("const", "() => dependency()"),
            ("let", "function() { return dependency(); }"),
            ("var", "async () => dependency()"),
        ] {
            let root = tempfile::tempdir().unwrap();
            let inputs = tempfile::tempdir().unwrap();
            let path = format!("module.{extension}");
            let (dependency, target) = if keyword == "var" {
                ("εξάρτηση", "διαγραφή")
            } else {
                ("dependency", "removeMe")
            };
            let source=format!("\u{feff}// owned café 🦀\r\n{keyword} dependency = () => 1;\r\nexport {keyword} removeMe = {value};\r\nexport {keyword} keep = async () => {{ void (await removeMe()); return 2; }};\r\n")
                .replace("dependency",dependency).replace("removeMe",target);
            let expected = format!(
                "\u{feff}// owned café 🦀\r\n\r\n\r\nexport {keyword} keep = async () => {{  return 2; }};\r\n"
            );
            fs::write(root.path().join(&path), &source).unwrap();
            fs::write(root.path().join("tsconfig.json"),serde_json::to_vec(&json!({"compilerOptions":{"strict":true,"module":"esnext","target":"es2022","allowJs":language=="javascript","checkJs":language=="javascript"},"include":[path]})).unwrap()).unwrap();
            let call = json!({"operation":"safeDelete","arguments":{"relativePath":path,"namePath":target,"propagate":true,"propagationPolicy":"captured_es_function_propagation_v3"}});
            cli_inputs(
                inputs.path(),
                language,
                &node,
                vec![server.clone(), "--stdio".into()],
                lsp::HostConfiguration {
                    passive_profile: Some(lsp::PassiveProfile::TypescriptLanguageServer),
                    ..Default::default()
                },
                call,
            );
            let output = cli(root.path(), inputs.path(), true, false);
            assert!(
                output.status.success(),
                "{language} {keyword}: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            let result: Value = serde_json::from_slice(&output.stdout).unwrap();
            println!("actual-v3-{language}-{keyword}-plan={result}");
            assert_eq!(result["sourceWritten"], false);
            let value = &result["results"][0];
            assert_eq!(
                value["propagation"]["policy"],
                "captured_es_function_propagation_v3"
            );
            assert_eq!(value["propagation"]["removedUsageCount"], 1);
            assert_eq!(value["complete"], false);
            let id = value["plan"]["id"].as_str().unwrap();
            let engine = Engine::open(root.path()).unwrap();
            let apply = json!({"planId":id,"idempotencyKey":"actual-v3-function-bindings","mode":"workspace"});
            assert!(yoke::changes::apply(&engine.store, root.path(), &apply, true, &d()).is_err());
            yoke::changes::approve(&engine.store, id, false).unwrap();
            assert!(yoke::changes::apply(&engine.store, root.path(), &apply, true, &d()).is_err());
            let command = vec![
                compiler.clone(),
                "--noEmit".into(),
                "--project".into(),
                "tsconfig.json".into(),
            ];
            assert!(yoke::verify::execute(&engine, id, &node, &command, false, &d()).is_err());
            let verification =
                yoke::verify::execute(&engine, id, &node, &command, true, &Deadline::new(60_000))
                    .unwrap();
            println!("actual-v3-{language}-{keyword}-verification={verification}");
            assert_eq!(verification["status"], "passed", "{verification}");
            assert!(yoke::changes::apply(&engine.store, root.path(), &apply, false, &d()).is_err());
            let receipt =
                yoke::changes::apply(&engine.store, root.path(), &apply, true, &d()).unwrap();
            assert_eq!(
                fs::read(root.path().join(&path)).unwrap(),
                expected.as_bytes()
            );
            assert!(
                yoke::changes::recover(&engine.store, root.path(), &receipt.id, false).is_err()
            );
            yoke::changes::recover(&engine.store, root.path(), &receipt.id, true).unwrap();
            assert_eq!(
                fs::read(root.path().join(&path)).unwrap(),
                source.as_bytes()
            );
            assert_eq!(
                yoke::model::hash(fs::read(executable).unwrap()),
                executable_hash
            );
        }
    }
}
