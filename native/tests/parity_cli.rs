use serde_json::Value;
use std::{fs, process::Command};
use tempfile::TempDir;
#[test]
fn public_serena_stdio_cli_preserves_cells_and_reports_errors_under_host_authority() {
    use std::{io::Write, process::Stdio};
    let root = TempDir::new().unwrap();
    fs::write(root.path().join("main.rs"), "fn a() {}\n").unwrap();
    let python = Command::new("python")
        .args(["-I", "-c", "import sys;print(sys.executable)"])
        .output()
        .unwrap();
    assert!(python.status.success());
    let python = String::from_utf8(python.stdout).unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_yoke"))
        .arg("--root")
        .arg(root.path())
        .args([
            "serve-session",
            "--id",
            "public",
            "--profile",
            "serena",
            "--python",
            python.trim(),
            "--trust-python",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let messages = [
        serde_json::json!({"jsonrpc":"2.0","id":1,"method":"initialize"}),
        serde_json::json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"serena_repl","arguments":{"session_id":"public","code":"counter=41\ncounter"}}}),
        serde_json::json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"serena_repl","arguments":{"session_id":"public","code":"raise ValueError('test failure')"}}}),
        serde_json::json!({"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"serena_repl","arguments":{"session_id":"public","code":"counter+1"}}}),
    ];
    {
        let mut input = child.stdin.take().unwrap();
        for message in messages {
            writeln!(input, "{message}").unwrap();
        }
    }
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let rows: Vec<Value> = String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(rows[1]["result"]["isError"], false);
    assert_eq!(rows[2]["result"]["isError"], true);
    assert_eq!(rows[3]["result"]["isError"], false);
    let data: Value =
        serde_json::from_str(rows[3]["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(data["data"]["result"], 42);
}
#[test]
fn public_session_stdio_cli_serves_the_extended_profile_and_exits_on_eof() {
    use std::io::Write;
    use std::process::Stdio;
    let root = TempDir::new().unwrap();
    fs::write(root.path().join("main.rs"), "fn a() {}\n").unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_yoke"))
        .arg("--root")
        .arg(root.path())
        .args(["serve-session", "--id", "stdio-cli"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    {
        let mut input = child.stdin.take().unwrap();
        input.write_all(b"{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"initialize\"}\n{\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"tools/list\"}\n").unwrap();
    }
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let responses: Vec<Value> = String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(responses.len(), 2);
    assert!(
        responses[1]["result"]["tools"]
            .as_array()
            .unwrap()
            .iter()
            .any(|t| t["name"] == "ci_session")
    );
}
#[test]
fn public_graph_view_cli_combines_manifest_nodes_and_reads_historical_view() {
    let root = TempDir::new().unwrap();
    fs::write(
        root.path().join("Cargo.toml"),
        "[package]\nname='example'\nversion='0.1.0'\n[dependencies]\nserde='1'\n",
    )
    .unwrap();
    fs::write(root.path().join("main.rs"), "fn process(){}\n").unwrap();
    let built = run(root.path(), &["graph", "build"]);
    let id = built["viewId"].as_str().unwrap();
    fs::write(
        root.path().join("Cargo.toml"),
        "[package]\nname='changed'\nversion='0.1.0'\n",
    )
    .unwrap();
    let found = run(
        root.path(),
        &[
            "graph",
            "view",
            id,
            "find",
            "--args",
            r#"{"query":"serde"}"#,
        ],
    );
    assert!(!found["items"].as_array().unwrap().is_empty(), "{found}");
    assert_eq!(found["graphViewId"], id);
}
#[test]
fn public_retrieval_cli_ranks_task_words_in_source_bodies() {
    let root = TempDir::new().unwrap();
    fs::write(
        root.path().join("main.rs"),
        "fn process(){ let payment=1; }\n",
    )
    .unwrap();
    let result = run(root.path(), &["retrieve", "payment"]);
    assert_eq!(result["items"][0]["name"], "process");
    assert_eq!(result["ranking"], "source_body_bm25_personalized_graph");
}
#[test]
fn public_agent_repl_cli_executes_persistent_cells_only_with_host_trust() {
    let root = TempDir::new().unwrap();
    let input = TempDir::new().unwrap();
    fs::write(root.path().join("main.rs"), "fn a() {}\n").unwrap();
    let cells = input.path().join("cells.json");
    fs::write(&cells,r#"["counter=40\ndef add(x):\n return x+2\ncounter","counter=add(counter)\n(counter,s.fs.list_dir('',recursive=True)['files'][0]['path'])"]"#).unwrap();
    let python = Command::new("python")
        .args(["-I", "-c", "import sys; print(sys.executable)"])
        .output()
        .unwrap();
    assert!(python.status.success());
    let python = String::from_utf8(python.stdout).unwrap();
    let python = python.trim();
    let result = run(
        root.path(),
        &[
            "agent-repl",
            "--python",
            python,
            "--trust-python",
            "--cells",
            cells.to_str().unwrap(),
        ],
    );
    assert_eq!(result["cells"][0]["result"], 40);
    assert_eq!(
        result["cells"][1]["result"],
        serde_json::json!([42, "main.rs"])
    );
    let denied = Command::new(env!("CARGO_BIN_EXE_yoke"))
        .arg("--root")
        .arg(root.path())
        .args([
            "agent-repl",
            "--python",
            python,
            "--cells",
            cells.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(!denied.status.success());
    assert_eq!(
        fs::read_to_string(root.path().join("main.rs")).unwrap(),
        "fn a() {}\n"
    );
}
#[test]
fn public_session_cli_resumes_host_registered_project_and_notes() {
    let root = TempDir::new().unwrap();
    let secondary = TempDir::new().unwrap();
    let inputs = TempDir::new().unwrap();
    fs::write(root.path().join("main.rs"), "fn primary() {}\n").unwrap();
    fs::write(secondary.path().join("main.rs"), "fn secondary() {}\n").unwrap();
    let profile = inputs.path().join("host.json");
    let requests = inputs.path().join("requests.json");
    fs::write(&profile,serde_json::to_vec(&serde_json::json!({"projects":[{"name":"secondary","root":secondary.path(),"readOnly":true}]})).unwrap()).unwrap();
    fs::write(&requests,r#"[{"operation":"activate","args":{"project":"secondary"}},{"operation":"notes.write","args":{"name":"choice","content":"Rust"}}]"#).unwrap();
    let first = run(
        root.path(),
        &[
            "session",
            "--id",
            "cli-session",
            "--host-profile",
            profile.to_str().unwrap(),
            "--requests",
            requests.to_str().unwrap(),
        ],
    );
    assert_eq!(first["results"][0]["activeProject"], "secondary");
    fs::write(&requests,r#"[{"operation":"configuration","args":{}},{"operation":"notes.read","args":{"name":"choice"}},{"operation":"fs.read","args":{"path":"main.rs"}}]"#).unwrap();
    let second = run(
        root.path(),
        &[
            "session",
            "--id",
            "cli-session",
            "--host-profile",
            profile.to_str().unwrap(),
            "--requests",
            requests.to_str().unwrap(),
        ],
    );
    assert_eq!(second["results"][0]["activeProject"], "secondary");
    assert_eq!(second["results"][1]["note"]["content"], "Rust");
    assert!(
        second["results"][2]["data"]["text"]
            .as_str()
            .unwrap()
            .contains("secondary")
    );
}
fn run(root: &std::path::Path, args: &[&str]) -> Value {
    let output = Command::new(env!("CARGO_BIN_EXE_yoke"))
        .arg("--root")
        .arg(root)
        .args(args)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}
#[test]
fn public_graph_cli_analyzes_and_exports_without_overwriting_human_cards() {
    let root = TempDir::new().unwrap();
    fs::write(root.path().join("main.rs"), "fn a(){b();}\nfn b(){}\n").unwrap();
    let report = run(root.path(), &["graph", "analyze"]);
    assert!(report["artifactId"].as_str().is_some());
    let parent = TempDir::new().unwrap();
    let output = parent.path().join("cards");
    let result = run(
        root.path(),
        &[
            "graph",
            "export",
            "--format",
            "obsidian",
            "--output",
            output.to_str().unwrap(),
        ],
    );
    assert_eq!(result["format"], "obsidian");
    assert!(output.is_dir());
    fs::write(output.join("human.md"), "Human notes").unwrap();
    let failed = Command::new(env!("CARGO_BIN_EXE_yoke"))
        .arg("--root")
        .arg(root.path())
        .args([
            "graph",
            "export",
            "--format",
            "obsidian",
            "--output",
            output.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(!failed.status.success());
    assert_eq!(
        fs::read_to_string(output.join("human.md")).unwrap(),
        "Human notes"
    );
    assert_eq!(
        fs::read_to_string(root.path().join("main.rs")).unwrap(),
        "fn a(){b();}\nfn b(){}\n"
    );
}
#[test]
fn public_named_notes_cli_is_persistent_and_global_store_is_explicit() {
    let root = TempDir::new().unwrap();
    fs::write(root.path().join("main.rs"), "fn a(){}\n").unwrap();
    let wrote = run(
        root.path(),
        &[
            "notes",
            "write",
            "--args",
            r#"{"name":"topic/choice","content":"Rust"}"#,
        ],
    );
    assert_eq!(wrote["note"]["content"], "Rust");
    let read = run(
        root.path(),
        &["notes", "read", "--args", r#"{"name":"topic/choice"}"#],
    );
    assert_eq!(read["note"]["id"], wrote["note"]["id"]);
    let list = run(root.path(), &["notes", "list"]);
    assert_eq!(list["items"].as_array().unwrap().len(), 1);
    let catalog = run(root.path(), &["server-catalog"]);
    assert_eq!(catalog["declaredAdapters"], 80);
    let adapters = catalog["adapters"].as_array().unwrap();
    assert_eq!(adapters.len(), 80);
    for id in ["vbnet", "typescript_tsgo", "graphql"] {
        assert_eq!(adapters.iter().filter(|row| row["id"] == id).count(), 1);
    }
    assert_eq!(catalog["liveCertifiedAdapters"], Value::Null);
}
