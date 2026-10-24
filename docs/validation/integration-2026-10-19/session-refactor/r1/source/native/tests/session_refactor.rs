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
    assert_eq!(
        fs::read(f.root.path().join("module.ts")).unwrap(),
        SOURCE.as_bytes()
    );
}
