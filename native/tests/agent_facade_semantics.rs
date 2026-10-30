use serde_json::{Value, json};
use std::{fs, path::Path};
use yoke::{
    Engine, agent_repl,
    capture::Deadline,
    project_session::{HostProfile, Session},
};
const SOURCE: &str = "// 🌍\nfn helper() {}\nfn main() { helper(); }\n";
fn session(root: &Path) -> Session {
    fs::write(root.join("main.rs"), SOURCE).unwrap();
    let mut engine = Engine::open(root).unwrap();
    engine
        .register_configured_server(yoke::lsp::ConfiguredServer {
            server: yoke::lsp::Server {
                command: "node".into(),
                args: vec![
                    Path::new(env!("CARGO_MANIFEST_DIR"))
                        .join("tests/fixtures/advanced-lsp.mjs")
                        .display()
                        .to_string(),
                    "".into(),
                ],
                language: "rust".into(),
                trust: true,
            },
            configuration: yoke::lsp::HostConfiguration {
                passive_profile: Some(yoke::lsp::PassiveProfile::ContractFixture),
                ..Default::default()
            },
        })
        .unwrap();
    Session::open(
        engine,
        HostProfile::default(),
        Some("remaining-facade-options"),
        &Deadline::new(20000),
    )
    .unwrap()
}
fn invoke(session: &mut Session, method: &str, args: Value) -> anyhow::Result<Value> {
    agent_repl::invoke_facade(session, method, args, None, &Deadline::new(20000))
}
#[test]
fn implementation_kinds_and_info_use_the_implemented_source_symbol() {
    let root = tempfile::tempdir().unwrap();
    let mut session = session(root.path());
    let empty = invoke(
        &mut session,
        "lsp.find_implementations",
        json!({"name_path":"helper","relative_path":"main.rs","include_kinds":[5]}),
    )
    .unwrap();
    assert!(empty["locations"].as_array().unwrap().is_empty());
    let actual = invoke(&mut session, "lsp.find_implementations", json!({"name_path":"helper","relative_path":"main.rs","include_kinds":[12],"include_info":true})).unwrap();
    assert_eq!(actual["locations"].as_array().unwrap().len(), 1);
    assert_eq!(actual["locations"][0]["symbol"]["name"], "main");
    assert_eq!(
        actual["locations"][0]["symbol"]["contentHash"],
        yoke::model::hash(SOURCE)
    );
    assert_eq!(
        actual["locations"][0]["info"]["snapshotId"],
        actual["snapshotId"]
    );
}
#[test]
fn diagnostic_line_ranges_and_reference_scope_are_real_source_filters() {
    let root = tempfile::tempdir().unwrap();
    let mut session = session(root.path());
    let empty = invoke(
        &mut session,
        "lsp.get_diagnostics_for_file",
        json!({"relative_path":"main.rs","start_line":0,"end_line":1}),
    )
    .unwrap();
    assert!(empty["items"].as_array().unwrap().is_empty());
    let actual = invoke(
        &mut session,
        "lsp.get_diagnostics_for_file",
        json!({"relative_path":"main.rs","start_line":2,"end_line":2}),
    )
    .unwrap();
    assert_eq!(actual["items"].as_array().unwrap().len(), 1);
    assert_eq!(actual["items"][0]["startByte"], 35);
    assert!(
        invoke(
            &mut session,
            "lsp.get_diagnostics_for_file",
            json!({"relative_path":"main.rs","start_line":99})
        )
        .is_err()
    );
    let symbol = invoke(
        &mut session,
        "lsp.get_diagnostics_for_symbol",
        json!({"name_path":"helper","reference_file":"main.rs","check_symbol_references":true}),
    )
    .unwrap();
    assert_eq!(symbol["items"].as_array().unwrap().len(), 1);
    assert_eq!(symbol["items"][0]["contentHash"], yoke::model::hash(SOURCE));
    assert_eq!(symbol["referenceScope"], "captured_referencing_symbols");
}
#[test]
fn boolean_facade_options_reject_wrong_types_before_querying() {
    let root = tempfile::tempdir().unwrap();
    let mut session = session(root.path());
    assert!(
        invoke(
            &mut session,
            "lsp.find_symbol",
            json!({"name_path_pattern":"helper","include_info":"true"})
        )
        .is_err()
    );
    assert!(
        invoke(
            &mut session,
            "fs.read_file",
            json!({"relative_path":"main.rs","max_answer_chars":-2})
        )
        .is_err()
    );
}
#[test]
fn reference_diagnostics_include_the_whole_referencing_symbol_as_the_public_contract_requests() {
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("main.rs"), SOURCE).unwrap();
    let mut engine = Engine::open(root.path()).unwrap();
    engine
        .register_configured_server(yoke::lsp::ConfiguredServer {
            server: yoke::lsp::Server {
                command: "node".into(),
                args: vec![
                    Path::new(env!("CARGO_MANIFEST_DIR"))
                        .join("tests/fixtures/advanced-lsp.mjs")
                        .display()
                        .to_string(),
                    "reference-diagnostics".into(),
                ],
                language: "rust".into(),
                trust: true,
            },
            configuration: yoke::lsp::HostConfiguration {
                passive_profile: Some(yoke::lsp::PassiveProfile::ContractFixture),
                ..Default::default()
            },
        })
        .unwrap();
    let mut session = Session::open(
        engine,
        HostProfile::default(),
        Some("reference-site-precision"),
        &Deadline::new(20000),
    )
    .unwrap();
    let value = invoke(
        &mut session,
        "lsp.get_diagnostics_for_symbol",
        json!({"name_path":"helper","reference_file":"main.rs","check_symbol_references":true}),
    )
    .unwrap();
    assert_eq!(
        value["items"].as_array().unwrap().len(),
        2,
        "The requested referencing-symbol diagnostic context was narrowed to the call site: {value}"
    );
    assert_eq!(value["referenceScope"], "captured_referencing_symbols");
}
