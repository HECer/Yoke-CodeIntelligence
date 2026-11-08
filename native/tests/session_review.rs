use serde_json::{Value, json};
use std::{fs, io::Cursor};
use tempfile::TempDir;
use yoke::{
    Engine,
    capture::Deadline,
    project_session::{HostProfile, Session},
};

#[test]
fn nested_native_failure_is_an_mcp_tool_error() {
    let root = TempDir::new().unwrap();
    fs::write(root.path().join("main.rs"), "fn actual() {}\n").unwrap();
    let mut session = Session::open(
        Engine::open(root.path()).unwrap(),
        HostProfile::default(),
        None,
        &Deadline::new(20000),
    )
    .unwrap();
    let requests = [
        json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18"}}),
        json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"ci_session","arguments":{"operation":"fs.read","args":{"path":"missing.rs"}}}}),
    ];
    let input = requests
        .iter()
        .map(|request| format!("{request}\n"))
        .collect::<String>();
    let mut output = Vec::new();
    yoke::protocol::serve_session(&mut session, Cursor::new(input), &mut output).unwrap();
    let response: Value =
        serde_json::from_str(String::from_utf8(output).unwrap().lines().nth(1).unwrap()).unwrap();
    assert_eq!(response["result"]["isError"], true, "{response}");
    let envelope: Value =
        serde_json::from_str(response["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(envelope["status"], "error");
}
