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
        Some("facade-options"),
        &Deadline::new(20000),
    )
    .unwrap()
}
fn invoke(session: &mut Session, method: &str, args: Value) -> anyhow::Result<Value> {
    agent_repl::invoke_facade(session, method, args, None, &Deadline::new(20000))
}
#[test]
fn positive_answer_bounds_count_unicode_serialized_characters_and_preserve_full_small_answers() {
    let root = tempfile::tempdir().unwrap();
    let mut session = session(root.path());
    let value = invoke(
        &mut session,
        "fs.read_file",
        json!({"relative_path":"main.rs","max_answer_chars":10000}),
    )
    .unwrap();
    assert_eq!(value["text"], SOURCE);
    let exact = serde_json::to_string(&value).unwrap().chars().count();
    assert!(exact < serde_json::to_vec(&value).unwrap().len());
    assert_eq!(
        invoke(
            &mut session,
            "fs.read_file",
            json!({"relative_path":"main.rs","max_answer_chars":exact})
        )
        .unwrap(),
        value
    );
    let error = invoke(
        &mut session,
        "fs.read_file",
        json!({"relative_path":"main.rs","max_answer_chars":exact-1}),
    )
    .unwrap_err();
    assert!(
        error.to_string().contains("exceeds max_answer_chars"),
        "{error:#}"
    );
}
#[test]
fn numeric_symbol_kinds_filter_referencing_containers_and_never_the_referenced_target() {
    let root = tempfile::tempdir().unwrap();
    let mut session = session(root.path());
    assert!(
        invoke(
            &mut session,
            "lsp.find_symbol",
            json!({"name_path_pattern":"*","include_kinds":[5]})
        )
        .unwrap()["items"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    let functions = invoke(
        &mut session,
        "lsp.find_symbol",
        json!({"name_path_pattern":"*","include_kinds":[12]}),
    )
    .unwrap();
    assert_eq!(functions["items"].as_array().unwrap().len(), 2);
    let none = invoke(
        &mut session,
        "lsp.find_referencing_symbols",
        json!({"name_path":"helper","relative_path":"main.rs","include_kinds":[5]}),
    )
    .unwrap();
    assert!(none["observations"].as_array().unwrap().is_empty());
    let actual = invoke(
        &mut session,
        "lsp.find_referencing_symbols",
        json!({"name_path":"helper","relative_path":"main.rs","include_kinds":[12]}),
    )
    .unwrap();
    assert_eq!(actual["observations"].as_array().unwrap().len(), 1);
    assert_eq!(actual["referencingSymbols"][0]["name"], "main");
    assert_eq!(
        actual["referencingSymbols"][0]["contentHash"],
        yoke::model::hash(SOURCE)
    );
    assert!(
        invoke(
            &mut session,
            "lsp.find_symbol",
            json!({"name_path_pattern":"*","include_kinds":[27]})
        )
        .is_err()
    );
}
#[test]
fn include_info_uses_actual_hover_at_the_same_captured_symbol_and_file_documentation_has_exact_anchors()
 {
    let root = tempfile::tempdir().unwrap();
    let mut session = session(root.path());
    let value = invoke(
        &mut session,
        "lsp.find_symbol",
        json!({"name_path_pattern":"helper","relative_path":"main.rs","include_info":true}),
    )
    .unwrap();
    assert_eq!(
        value["items"][0]["info"]["response"]["contents"]["value"],
        "Independent fixture hover information"
    );
    assert_eq!(
        value["items"][0]["info"]["snapshotId"],
        value["items"][0]["snapshotId"]
    );
    assert_eq!(
        value["items"][0]["info"]["contentHash"],
        yoke::model::hash(SOURCE)
    );
    let overview = invoke(
        &mut session,
        "jb.get_symbols_overview",
        json!({"relative_path":"main.rs","include_file_documentation":true}),
    )
    .unwrap();
    assert_eq!(overview["fileDocumentation"]["text"], "// 🌍");
    assert_eq!(overview["fileDocumentation"]["startByte"], 0);
    assert_eq!(overview["fileDocumentation"]["endByte"], 7);
    assert_eq!(
        overview["fileDocumentation"]["contentHash"],
        yoke::model::hash(SOURCE)
    );
}
