use serde_json::json;
use std::fs;
use tempfile::TempDir;
use yoke::{
    Engine,
    capture::Deadline,
    project_session::{HostProfile, Session},
};
#[test]
fn public_debug_information_topic_is_explicit_and_does_not_execute_a_backend() {
    let root = TempDir::new().unwrap();
    fs::write(root.path().join("main.rs"), "fn a(){}\n").unwrap();
    let d = Deadline::new(20000);
    let mut session = Session::open(
        Engine::open(root.path()).unwrap(),
        HostProfile::default(),
        Some("info"),
        &d,
    )
    .unwrap();
    let info = yoke::compatibility::invoke_serena(
        &mut session,
        "serena_info",
        &json!({"topic":"jet_brains_debug_repl"}),
        &d,
    )
    .unwrap();
    assert_eq!(info["topic"], "jet_brains_debug_repl");
    assert_eq!(info["executionStarted"], false);
    assert!(
        yoke::compatibility::invoke_serena(
            &mut session,
            "serena_info",
            &json!({"topic":"unknown"}),
            &d
        )
        .is_err()
    );
}
#[test]
fn serena_profile_inventories_all_public_tools_and_routes_authored_notes() {
    let tools = yoke::compatibility::serena_tools();
    assert_eq!(tools.as_array().unwrap().len(), 53);
    assert!(
        tools
            .as_array()
            .unwrap()
            .iter()
            .any(|tool| tool["name"] == "serena_repl")
    );
    let root = TempDir::new().unwrap();
    fs::write(root.path().join("main.rs"), "fn alpha() {}\n").unwrap();
    let d = Deadline::new(20000);
    let mut session = Session::open(
        Engine::open(root.path()).unwrap(),
        HostProfile::default(),
        Some("compat"),
        &d,
    )
    .unwrap();
    yoke::compatibility::invoke_serena(
        &mut session,
        "write_memory",
        &json!({"memory_name":"choice","content":"Rust"}),
        &d,
    )
    .unwrap();
    let note = yoke::compatibility::invoke_serena(
        &mut session,
        "read_memory",
        &json!({"memory_name":"choice"}),
        &d,
    )
    .unwrap();
    assert_eq!(note["note"]["content"], "Rust");
    assert!(
        yoke::compatibility::invoke_serena(
            &mut session,
            "write_memory",
            &json!({"memory_name":"bad","content":"data","root":"C:/"}),
            &d
        )
        .is_err()
    );
}
#[test]
fn compatibility_session_ids_cannot_select_another_host_session() {
    let root = TempDir::new().unwrap();
    fs::write(root.path().join("main.rs"), "fn a(){}\n").unwrap();
    let d = Deadline::new(20000);
    let mut session = Session::open(
        Engine::open(root.path()).unwrap(),
        HostProfile::default(),
        Some("owned"),
        &d,
    )
    .unwrap();
    assert!(
        yoke::compatibility::invoke_serena(
            &mut session,
            "activate_project",
            &json!({"project":"default","session_id":"foreign"}),
            &d
        )
        .is_err()
    );
    let selected = yoke::compatibility::invoke_serena(
        &mut session,
        "activate_project",
        &json!({"project":"default","session_id":"owned"}),
        &d,
    )
    .unwrap();
    assert_eq!(selected["activeProject"], "default");
}
#[test]
fn public_serena_mcp_profile_lists_contracts_and_routes_without_native_write_tools() {
    let root = TempDir::new().unwrap();
    fs::write(root.path().join("main.rs"), "fn a(){}\n").unwrap();
    let d = Deadline::new(20000);
    let mut session = Session::open(
        Engine::open(root.path()).unwrap(),
        HostProfile::default(),
        Some("mcp"),
        &d,
    )
    .unwrap();
    let messages = [
        json!({"jsonrpc":"2.0","id":1,"method":"initialize"}),
        json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}),
        json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"get_current_config","arguments":{}}}),
        json!({"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"activate_project","arguments":{"project":"default","session_id":"other"}}}),
    ];
    let input = messages
        .iter()
        .map(|m| m.to_string() + "\n")
        .collect::<String>();
    let mut out = Vec::new();
    yoke::protocol::serve_serena(&mut session, std::io::Cursor::new(input), &mut out).unwrap();
    let rows: Vec<serde_json::Value> = String::from_utf8(out)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(rows[1]["result"]["tools"].as_array().unwrap().len(), 53);
    assert_eq!(rows[2]["result"]["isError"], false);
    assert_eq!(rows[3]["result"]["isError"], true);
}
#[test]
fn public_contract_types_defaults_and_owned_python_runtime_are_enforced() {
    let root = TempDir::new().unwrap();
    fs::write(root.path().join("main.rs"), "fn a(){}\n").unwrap();
    let d = Deadline::new(20000);
    let mut session = Session::open(
        Engine::open(root.path()).unwrap(),
        HostProfile::default(),
        Some("python"),
        &d,
    )
    .unwrap();
    assert!(
        yoke::compatibility::invoke_serena(
            &mut session,
            "write_memory",
            &json!({"memory_name":"bad","content":4}),
            &d
        )
        .is_err()
    );
    assert!(
        yoke::compatibility::invoke_serena(
            &mut session,
            "list_dir",
            &json!({"relative_path":"","recursive":"yes"}),
            &d
        )
        .is_err()
    );
    assert!(
        yoke::compatibility::invoke_serena(
            &mut session,
            "write_memory",
            &json!({"memory_name":"bad"}),
            &d
        )
        .is_err()
    );
    let python = std::process::Command::new("python")
        .args(["-I", "-c", "import sys;print(sys.executable)"])
        .output()
        .unwrap();
    assert!(python.status.success());
    let spec = yoke::agent_repl::ReplSpec {
        python: String::from_utf8(python.stdout).unwrap().trim().into(),
        trust_python: true,
        max_output_bytes: 1000000,
        shell: None,
    };
    let mut runtime = yoke::agent_repl::AgentRepl::spawn(spec, &d).unwrap();
    let first = yoke::compatibility::invoke_serena_with_repl(
        &mut session,
        &mut runtime,
        "serena_repl",
        &json!({"session_id":"python","code":"counter=40\ncounter"}),
        &d,
    )
    .unwrap();
    assert_eq!(first["result"], 40);
    let next = yoke::compatibility::invoke_serena_with_repl(
        &mut session,
        &mut runtime,
        "serena_repl",
        &json!({"session_id":"python","code":"counter+=2\ncounter"}),
        &d,
    )
    .unwrap();
    assert_eq!(next["result"], 42);
    assert!(
        yoke::compatibility::invoke_serena_with_repl(
            &mut session,
            &mut runtime,
            "serena_repl",
            &json!({"session_id":"foreign","code":"counter=0"}),
            &d
        )
        .is_err()
    );
    runtime.close().unwrap();
}
#[test]
fn public_external_project_queries_route_readonly_aliases_without_activation_or_edits() {
    let root = TempDir::new().unwrap();
    let external = TempDir::new().unwrap();
    fs::write(root.path().join("main.rs"), "fn local() {}\n").unwrap();
    fs::write(external.path().join("main.rs"), "fn externalAlpha() {}\n").unwrap();
    let d = Deadline::new(20000);
    let host = HostProfile {
        projects: vec![yoke::project_session::HostProject {
            name: "reference".into(),
            root: external.path().into(),
            read_only: true,
            query_only: true,
        }],
        ..HostProfile::default()
    };
    let mut session =
        Session::open(Engine::open(root.path()).unwrap(), host, Some("owned"), &d).unwrap();
    let found=yoke::compatibility::invoke_serena(&mut session,"query_project",&json!({"project_name":"reference","tool_name":"find_symbol","tool_params_json":"{\"name_path_pattern\":\"externalAlpha\",\"relative_path\":\"main.rs\"}"}),&d).unwrap();
    assert_eq!(found["items"][0]["name"], "externalAlpha");
    assert!(yoke::compatibility::invoke_serena(&mut session,"query_project",&json!({"project_name":"reference","tool_name":"create_text_file","tool_params_json":"{\"relative_path\":\"bad.rs\",\"content\":\"bad\"}"}),&d).is_err());
    assert_eq!(
        session.invoke("configuration", &json!({}), &d).unwrap()["activeProject"],
        "default"
    );
    assert!(!external.path().join("bad.rs").exists());
}
