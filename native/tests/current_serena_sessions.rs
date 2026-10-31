use serde_json::{Value, json};
use std::fs;
use yoke::{
    Engine,
    capture::Deadline,
    project_session::{HostProfile, Session},
};

const SOURCE: &str = "// owned \u{1f4a1} e\u{301}\r\nfn visible() {}\r\n";

fn setup(id: &str, deadline: &Deadline) -> (tempfile::TempDir, Session) {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("owned \u{3c0} e\u{301}");
    fs::create_dir(&root).unwrap();
    fs::write(root.join("main.rs"), SOURCE.as_bytes()).unwrap();
    let session = Session::open(
        Engine::open(&root).unwrap(),
        HostProfile::default(),
        Some(id),
        deadline,
    )
    .unwrap();
    (temp, session)
}

#[test]
fn current_discovery_adds_exactly_two_session_unions_and_preserves_the_other_51_tools() {
    let historical: Value =
        serde_json::from_str(include_str!("../src/compatibility_contracts.json")).unwrap();
    let tools = yoke::compatibility::serena_tools();
    assert_eq!(tools.as_array().unwrap().len(), 53);
    let mut changed = 0;
    for (old, current) in historical["tools"]
        .as_array()
        .unwrap()
        .iter()
        .zip(tools.as_array().unwrap())
    {
        assert_eq!(current["name"], old["name"]);
        assert_eq!(current["description"], old["description"]);
        if matches!(
            old["name"].as_str(),
            Some("activate_project" | "serena_repl")
        ) {
            changed += 1;
            let schema = &current["inputSchema"];
            assert_eq!(schema["properties"]["session"], json!({"type":"string"}));
            assert_eq!(schema["properties"]["session_id"]["deprecated"], true);
            assert_eq!(
                schema["oneOf"],
                json!([{"required":["session"]},{"required":["session_id"]}])
            );
            assert_eq!(schema["additionalProperties"], false);
        } else {
            assert_eq!(current["inputSchema"], old["inputSchema"]);
        }
    }
    assert_eq!(changed, 2);
}

#[test]
fn both_spellings_require_the_same_host_session_and_do_not_create_a_python_runtime() {
    let deadline = Deadline::new(20000);
    let (_temp, mut session) = setup("owned", &deadline);
    for key in ["session", "session_id"] {
        let mut activation = json!({"project":"default"});
        activation[key] = json!("foreign");
        assert!(
            yoke::compatibility::invoke_serena(
                &mut session,
                "activate_project",
                &activation,
                &deadline
            )
            .is_err()
        );
        activation[key] = json!("owned");
        let value = yoke::compatibility::invoke_serena(
            &mut session,
            "activate_project",
            &activation,
            &deadline,
        )
        .unwrap();
        assert_eq!(value["activeProject"], "default");
        let mut repl = json!({"code":"raise RuntimeError('must not execute')"});
        repl[key] = json!("owned");
        let error =
            yoke::compatibility::invoke_serena(&mut session, "serena_repl", &repl, &deadline)
                .unwrap_err();
        assert!(
            error
                .to_string()
                .contains("Persistent Python runtime must be configured explicitly by the host"),
            "{error:#}"
        );
    }
    for args in [
        json!({"project":"default","session":"owned","session_id":"owned"}),
        json!({"project":"default","session":"owned","session_id":"foreign"}),
        json!({"project":"default","session":null}),
        json!({"project":"default","session":true}),
        json!({"project":"default","session":["owned"]}),
        json!({"project":"default","session":"owned","root":"outside"}),
        json!({"project":"default","\u{455}ession":"owned"}),
        json!({"project":"default"}),
    ] {
        assert!(
            yoke::compatibility::invoke_serena(&mut session, "activate_project", &args, &deadline)
                .is_err(),
            "{args}"
        );
    }
    assert_eq!(
        fs::read(session.engine.root.join("main.rs")).unwrap(),
        SOURCE.as_bytes()
    );
}

#[test]
fn real_serena_stdio_discovery_and_both_session_spellings_use_the_owned_session() {
    let deadline = Deadline::new(20000);
    let (_temp, mut session) = setup("wire", &deadline);
    let messages = [
        json!({"jsonrpc":"2.0","id":1,"method":"initialize"}),
        json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}),
        json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"activate_project","arguments":{"project":"default","session":"wire"}}}),
        json!({"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"activate_project","arguments":{"project":"default","session_id":"wire"}}}),
        json!({"jsonrpc":"2.0","id":5,"method":"tools/call","params":{"name":"activate_project","arguments":{"project":"default","session":"foreign"}}}),
        json!({"jsonrpc":"2.0","id":6,"method":"tools/call","params":{"name":"activate_project","arguments":{"project":"default","session":"wire","session_id":"wire"}}}),
    ];
    let input: String = messages.iter().map(|row| row.to_string() + "\n").collect();
    let mut output = Vec::new();
    yoke::protocol::serve_serena(&mut session, std::io::Cursor::new(input), &mut output).unwrap();
    let rows: Vec<Value> = String::from_utf8(output)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(rows.len(), 6);
    let tool = rows[1]["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["name"] == "activate_project")
        .unwrap();
    assert_eq!(
        tool["inputSchema"]["properties"]["session"]["type"],
        "string"
    );
    for row in &rows[2..4] {
        assert_eq!(row["result"]["isError"], false, "{row}");
        let value: Value =
            serde_json::from_str(row["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
        assert_eq!(value["activeProject"], "default");
    }
    for row in &rows[4..] {
        assert_eq!(row["result"]["isError"], true, "{row}");
    }
    assert_eq!(
        fs::read(session.engine.root.join("main.rs")).unwrap(),
        SOURCE.as_bytes()
    );
}

#[test]
fn current_and_legacy_parameters_share_one_explicit_persistent_runtime_without_foreign_execution() {
    let deadline = Deadline::new(20000);
    let (_temp, mut session) = setup("python", &deadline);
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
    let mut runtime = yoke::agent_repl::AgentRepl::spawn(spec, &deadline).unwrap();
    let first = yoke::compatibility::invoke_serena_with_repl(
        &mut session,
        &mut runtime,
        "serena_repl",
        &json!({"session":"python","code":"counter=40\ncounter"}),
        &deadline,
    )
    .unwrap();
    assert_eq!(first["result"], 40);
    for bad in [
        json!({"session":"foreign","code":"counter=0"}),
        json!({"session":"python","session_id":"python","code":"counter=0"}),
        json!({"session":false,"code":"counter=0"}),
    ] {
        assert!(
            yoke::compatibility::invoke_serena_with_repl(
                &mut session,
                &mut runtime,
                "serena_repl",
                &bad,
                &deadline
            )
            .is_err()
        );
    }
    let next = yoke::compatibility::invoke_serena_with_repl(
        &mut session,
        &mut runtime,
        "serena_repl",
        &json!({"session_id":"python","code":"counter+=2\ncounter"}),
        &deadline,
    )
    .unwrap();
    assert_eq!(next["result"], 42);
    runtime.close().unwrap();
    assert_eq!(
        fs::read(session.engine.root.join("main.rs")).unwrap(),
        SOURCE.as_bytes()
    );
}
