use serde_json::json;
use std::fs;
use tempfile::TempDir;
use yoke::{
    Engine,
    capture::Deadline,
    project_session::{HostProfile, Session},
};
#[test]
fn public_trail_mcp_profile_lists_six_tools_and_accepts_legacy_query_aliases() {
    let root = TempDir::new().unwrap();
    fs::write(root.path().join("main.rs"), "fn alpha(){}\n").unwrap();
    let d = Deadline::new(20000);
    let mut session = Session::open(
        Engine::open(root.path()).unwrap(),
        HostProfile::default(),
        Some("mcp"),
        &d,
    )
    .unwrap();
    let rows = [
        json!({"jsonrpc":"2.0","id":1,"method":"initialize"}),
        json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}),
        json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"graft_skeleton","arguments":{"file":"main.rs"}}}),
        json!({"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"trail_check_freshness","arguments":{"refresh":true}}}),
    ];
    let input = rows
        .iter()
        .map(|value| value.to_string() + "\n")
        .collect::<String>();
    let mut output = Vec::new();
    yoke::protocol::serve_trail(
        &mut session,
        "trail",
        std::io::Cursor::new(input),
        &mut output,
    )
    .unwrap();
    let rows: Vec<serde_json::Value> = String::from_utf8(output)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(rows[1]["result"]["tools"].as_array().unwrap().len(), 6);
    assert_eq!(rows[1]["result"]["tools"][0]["name"], "trail_find_code");
    assert_eq!(rows[2]["result"]["isError"], false);
    assert_eq!(rows[3]["result"]["isError"], true);
}
#[test]
fn six_public_trail_tools_preserve_freshness_and_exact_file_api_contracts() {
    let root = TempDir::new().unwrap();
    fs::write(
        root.path().join("main.rs"),
        "fn alpha(\n input: usize\n) -> usize { input + 1 }\n",
    )
    .unwrap();
    let d = Deadline::new(20000);
    let mut session = Session::open(
        Engine::open(root.path()).unwrap(),
        HostProfile::default(),
        Some("trail"),
        &d,
    )
    .unwrap();
    assert_eq!(
        yoke::trail_adapter::tools("trail")
            .as_array()
            .unwrap()
            .len(),
        6
    );
    let status =
        yoke::trail_adapter::invoke(&mut session, "graft_check_freshness", &json!({}), &d).unwrap();
    assert_eq!(status["indexed"], false);
    let api = yoke::trail_adapter::invoke(
        &mut session,
        "trail_file_api",
        &json!({"file":"main.rs"}),
        &d,
    )
    .unwrap();
    assert!(
        api["items"][0]["signature"]
            .as_str()
            .unwrap()
            .contains("input: usize")
    );
    assert!(
        yoke::trail_adapter::invoke(
            &mut session,
            "graft_find_code",
            &json!({"query":"alpha","command":"bad"}),
            &d
        )
        .is_err()
    );
}
#[test]
fn task_retrieval_regex_hits_and_repo_orientation_use_captured_source() {
    let root = TempDir::new().unwrap();
    fs::create_dir_all(root.path().join("src")).unwrap();
    fs::write(
        root.path().join("src/main.rs"),
        "fn alpha(){ let invoice_payment = 1; }\nfn beta(){ alpha(); }\n",
    )
    .unwrap();
    let d = Deadline::new(20000);
    let mut session = Session::open(
        Engine::open(root.path()).unwrap(),
        HostProfile::default(),
        Some("trail"),
        &d,
    )
    .unwrap();
    let code = yoke::trail_adapter::invoke(
        &mut session,
        "graft_find_code",
        &json!({"query":"invoice payment","in":"src"}),
        &d,
    )
    .unwrap();
    assert!(!code["items"].as_array().unwrap().is_empty());
    let hits = yoke::trail_adapter::invoke(
        &mut session,
        "graft_find_all",
        &json!({"pattern":"ALPHA","ignore_case":true,"fixed":true}),
        &d,
    )
    .unwrap();
    assert!(hits["items"].as_array().unwrap().len() >= 2);
    let map =
        yoke::trail_adapter::invoke(&mut session, "graft_repo_map", &json!({"max_dirs":1}), &d)
            .unwrap();
    assert_eq!(map["directories"][0]["path"], "src");
}
#[test]
fn named_call_trace_accepts_full_closure_without_the_native_depth_eight_limit() {
    let root = TempDir::new().unwrap();
    let mut source = String::new();
    for i in 0..12 {
        source.push_str(&format!("fn step{i}(){{ step{}(); }}\n", i + 1));
    }
    source.push_str("fn step12(){}\n");
    fs::write(root.path().join("main.rs"), source).unwrap();
    let d = Deadline::new(20000);
    let mut session = Session::open(
        Engine::open(root.path()).unwrap(),
        HostProfile::default(),
        Some("trail"),
        &d,
    )
    .unwrap();
    let trace = yoke::trail_adapter::invoke(
        &mut session,
        "graft_trace_calls",
        &json!({"symbol":"step0","direction":"out","depth":"all"}),
        &d,
    )
    .unwrap();
    assert!(
        trace["entities"]
            .as_array()
            .unwrap()
            .iter()
            .any(|entity| entity["name"] == "step12")
    );
    assert_eq!(trace["truncated"], false);
}
