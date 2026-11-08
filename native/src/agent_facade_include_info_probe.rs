//! Same include-info contract with parent-side phases available only in unit builds.
use crate::{
    Engine, agent_repl,
    capture::Deadline,
    project_session::{HostProfile, Session},
};
use serde_json::{Value, json};
use std::{fs, path::Path, time::Instant};

const SOURCE: &str = "// 🌍\nfn helper() {}\nfn main() { helper(); }\n";

fn invoke(session: &mut Session, method: &str, args: Value) -> Value {
    let deadline = Deadline::new(20000);
    let _observations = crate::lsp::test_startup_phases::Guard::begin();
    let started = Instant::now();
    agent_repl::invoke_facade(session, method, args, None, &deadline).unwrap_or_else(|error| {
        let elapsed = started.elapsed();
        let remaining = deadline.remaining();
        let phases = crate::lsp::test_startup_phases::snapshot();
        panic!(
            "include-info parent probe failed; elapsed={elapsed:?}; remaining={remaining:?}; native_phases={phases:?}; error={error:#}"
        );
    })
}

#[test]
fn include_info_keeps_the_original_hover_and_document_anchors_with_parent_phases() {
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("main.rs"), SOURCE).unwrap();
    let mut engine = Engine::open(root.path()).unwrap();
    engine
        .register_configured_server(crate::lsp::ConfiguredServer {
            server: crate::lsp::Server {
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
            configuration: crate::lsp::HostConfiguration {
                passive_profile: Some(crate::lsp::PassiveProfile::ContractFixture),
                ..Default::default()
            },
        })
        .unwrap();
    let mut session = Session::open(
        engine,
        HostProfile::default(),
        Some("facade-options"),
        &Deadline::new(20000),
    )
    .unwrap();
    let value = invoke(
        &mut session,
        "lsp.find_symbol",
        json!({"name_path_pattern":"helper","relative_path":"main.rs","include_info":true}),
    );
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
        crate::model::hash(SOURCE)
    );
    let overview = invoke(
        &mut session,
        "jb.get_symbols_overview",
        json!({"relative_path":"main.rs","include_file_documentation":true}),
    );
    assert_eq!(overview["fileDocumentation"]["text"], "// 🌍");
    assert_eq!(overview["fileDocumentation"]["startByte"], 0);
    assert_eq!(overview["fileDocumentation"]["endByte"], 7);
    assert_eq!(
        overview["fileDocumentation"]["contentHash"],
        crate::model::hash(SOURCE)
    );
}
