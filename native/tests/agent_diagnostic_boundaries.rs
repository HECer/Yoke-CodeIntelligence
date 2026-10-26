use serde_json::{Value, json};
use std::{fs, path::Path, time::Instant};
use yoke::{
    Engine, agent_repl,
    capture::Deadline,
    project_session::{HostProfile, Session},
};

fn invoke_method(source: &str, method: &str, args: Value) -> Value {
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("main.rs"), source).unwrap();
    let mut engine = Engine::open(root.path()).unwrap();
    engine
        .register_configured_server(yoke::lsp::ConfiguredServer {
            server: yoke::lsp::Server {
                command: "node".into(),
                args: vec![
                    Path::new(env!("CARGO_MANIFEST_DIR"))
                        .join("tests/fixtures/diagnostic-boundaries-lsp.mjs")
                        .display()
                        .to_string(),
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
    let deadline = Deadline::new(20000);
    let started = Instant::now();
    let selected_lines = (
        args.get("start_line").and_then(Value::as_u64),
        args.get("end_line").and_then(Value::as_u64),
    );
    let mut session = Session::open(
        engine,
        HostProfile::default(),
        Some("diagnostic-eof"),
        &deadline,
    )
    .unwrap_or_else(|error| {
        panic!(
            "diagnostic fixture failed; phase=session-open; method={method}; selected_lines={selected_lines:?}; elapsed={:?}; remaining={:?}; error={error:#}",
            started.elapsed(),
            deadline.remaining()
        )
    });
    let opened_elapsed = started.elapsed();
    let opened_remaining = deadline.remaining();
    agent_repl::invoke_facade(&mut session, method, args, None, &deadline).unwrap_or_else(|error| {
        panic!(
            "diagnostic fixture failed; phase=facade; method={method}; selected_lines={selected_lines:?}; opened_elapsed={opened_elapsed:?}; opened_remaining={opened_remaining:?}; elapsed={:?}; remaining={:?}; error={error:#}",
            started.elapsed(),
            deadline.remaining()
        )
    })
}
fn invoke(source: &str, args: Value) -> Value {
    invoke_method(source, "lsp.get_diagnostics_for_file", args)
}

#[test]
fn whole_file_diagnostics_keep_valid_zero_width_eof_points_including_empty_source() {
    for source in ["", "fn selected() {}\nfn next() {}\n"] {
        let value = invoke(source, json!({"relative_path":"main.rs"}));
        assert!(
            value["items"]
                .as_array()
                .unwrap()
                .iter()
                .any(|item| item["startByte"] == source.len() && item["endByte"] == source.len()),
            "Valid EOF diagnostic lost for {} bytes: {value}",
            source.len()
        );
    }
}

#[test]
fn partial_line_diagnostics_do_not_admit_the_next_line_point() {
    let value = invoke(
        "fn selected() {}\nfn next() {}\n",
        json!({"relative_path":"main.rs","start_line":0,"end_line":0}),
    );
    assert!(
        value["items"].as_array().unwrap().is_empty(),
        "Next line diagnostic escaped selected line: {value}"
    );
}

#[test]
fn selecting_lines_before_the_trailing_empty_line_excludes_its_eof_point() {
    let value = invoke(
        "fn selected() {}\nfn next() {}\n",
        json!({"relative_path":"main.rs","start_line":0,"end_line":1}),
    );
    assert_eq!(value["items"].as_array().unwrap().len(), 1);
    assert_eq!(
        value["items"][0]["message"],
        "Next line declaration warning"
    );
}

#[test]
fn independent_inspection_line_ranges_exclude_the_next_line_but_keep_full_file_eof() {
    let source = "fn selected() {}\nfn next() {}\n";
    let partial = invoke_method(
        source,
        "jb.run_inspections",
        json!({"relative_path":"main.rs","start_line":0,"end_line":0}),
    );
    assert!(
        partial["items"].as_array().unwrap().is_empty(),
        "Next line inspection escaped scope: {partial}"
    );
    let full = invoke_method(
        source,
        "jb.run_inspections",
        json!({"relative_path":"main.rs"}),
    );
    assert_eq!(full["items"].as_array().unwrap().len(), 2);
}
