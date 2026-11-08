use serde_json::{Value, json};
use std::{fs, io::Cursor};
use tempfile::TempDir;
use yoke::{
    Engine,
    capture::Deadline,
    project_session::{HostProfile, Session},
};
#[test]
fn session_graph_tool_builds_an_explicit_source_bound_view() {
    let root = TempDir::new().unwrap();
    fs::write(
        root.path().join("Cargo.toml"),
        "[package]\nname='example'\nversion='0.1.0'\n[dependencies]\nserde='1'\n",
    )
    .unwrap();
    let mut session = Session::open(
        Engine::open(root.path()).unwrap(),
        HostProfile::default(),
        Some("graph"),
        &Deadline::new(20000),
    )
    .unwrap();
    let input = format!(
        "{}\n{}\n",
        json!({"jsonrpc":"2.0","id":1,"method":"initialize"}),
        json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"ci_graph","arguments":{"operation":"build","options":{}}}})
    );
    let mut output = Vec::new();
    yoke::protocol::serve_session(&mut session, Cursor::new(input), &mut output).unwrap();
    let responses: Vec<Value> = String::from_utf8(output)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    let result: Value = serde_json::from_str(
        responses[1]["result"]["content"][0]["text"]
            .as_str()
            .unwrap(),
    )
    .unwrap();
    assert_eq!(responses[1]["result"]["isError"], false, "{result}");
    assert!(
        result["data"]["viewId"]
            .as_str()
            .is_some_and(yoke::model::valid_id)
    );
    assert!(result["data"]["nodes"].as_u64().unwrap() > 1);
}
#[test]
fn session_mcp_retrieval_uses_exact_source_bodies_and_rejects_authority_options() {
    let root = TempDir::new().unwrap();
    fs::write(
        root.path().join("main.rs"),
        "fn process(){let payment=1;}\n",
    )
    .unwrap();
    let mut session = Session::open(
        Engine::open(root.path()).unwrap(),
        HostProfile::default(),
        Some("retrieval"),
        &Deadline::new(20000),
    )
    .unwrap();
    let messages = [
        json!({"jsonrpc":"2.0","id":1,"method":"initialize"}),
        json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"ci_retrieve","arguments":{"query":"payment","includeBody":true}}}),
        json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"ci_retrieve","arguments":{"query":"payment","execute":true}}}),
    ];
    let input = messages
        .iter()
        .map(|m| format!("{m}\n"))
        .collect::<String>();
    let mut output = Vec::new();
    yoke::protocol::serve_session(&mut session, Cursor::new(input), &mut output).unwrap();
    let responses: Vec<Value> = String::from_utf8(output)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    let result: Value = serde_json::from_str(
        responses[1]["result"]["content"][0]["text"]
            .as_str()
            .unwrap(),
    )
    .unwrap();
    assert_eq!(responses[1]["result"]["isError"], false, "{result}");
    assert_eq!(result["data"]["items"][0]["name"], "process");
    assert!(
        result["data"]["items"][0]["body"]
            .as_str()
            .unwrap()
            .contains("payment")
    );
    assert_eq!(responses[2]["result"]["isError"], true);
}
#[test]
fn session_profile_exposes_persistent_steering_without_host_authority_arguments() {
    let root = TempDir::new().unwrap();
    fs::write(root.path().join("main.rs"), "fn a() {}\n").unwrap();
    let mut session = Session::open(
        Engine::open(root.path()).unwrap(),
        HostProfile::default(),
        Some("mcp"),
        &Deadline::new(20000),
    )
    .unwrap();
    let messages = [
        json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18"}}),
        json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}),
        json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"ci_session","arguments":{"operation":"switchModes","args":{"modes":["planning"]}}}}),
        json!({"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"ci_session","arguments":{"operation":"configuration","args":{}}}}),
        json!({"jsonrpc":"2.0","id":5,"method":"tools/call","params":{"name":"ci_session","arguments":{"operation":"activate","args":{"project":"default","root":"C:/"}}}}),
    ];
    let input = messages
        .iter()
        .map(|m| format!("{m}\n"))
        .collect::<String>();
    let mut output = Vec::new();
    yoke::protocol::serve_session(&mut session, Cursor::new(input), &mut output).unwrap();
    let responses: Vec<Value> = String::from_utf8(output)
        .unwrap()
        .lines()
        .map(|s| serde_json::from_str(s).unwrap())
        .collect();
    assert!(
        responses[1]["result"]["tools"]
            .as_array()
            .unwrap()
            .iter()
            .any(|t| t["name"] == "ci_session")
    );
    assert!(
        !responses[1]["result"]["tools"]
            .as_array()
            .unwrap()
            .iter()
            .any(|t| t["name"] == "ci_change_apply")
    );
    let config: Value = serde_json::from_str(
        responses[3]["result"]["content"][0]["text"]
            .as_str()
            .unwrap(),
    )
    .unwrap();
    assert_eq!(config["data"]["modes"], json!(["planning"]));
    assert_eq!(responses[4]["result"]["isError"], true);
}
