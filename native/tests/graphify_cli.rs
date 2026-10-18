use serde_json::{Value, json};
use std::{
    io::Write,
    process::{Command, Stdio},
};

#[test]
fn public_graphify_cli_exposes_ten_tools_six_resources_and_real_graph_stats() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(
        root.path().join("main.rs"),
        "fn helper() {}\nfn main() { helper(); }\n",
    )
    .unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_yoke"))
        .arg("--root")
        .arg(root.path())
        .args([
            "serve-session",
            "--id",
            "graphify-cli",
            "--profile",
            "graphify",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    {
        let mut input = child.stdin.take().unwrap();
        for message in [
            json!({"jsonrpc":"2.0","id":1,"method":"initialize"}),
            json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}),
            json!({"jsonrpc":"2.0","id":3,"method":"resources/list"}),
            json!({"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"graph_stats","arguments":{}}}),
            json!({"jsonrpc":"2.0","id":5,"method":"resources/read","params":{"uri":"graphify://audit"}}),
        ] {
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
        .map(|row| serde_json::from_str(row).unwrap())
        .collect();
    assert_eq!(rows[1]["result"]["tools"].as_array().unwrap().len(), 10);
    assert_eq!(rows[2]["result"]["resources"].as_array().unwrap().len(), 6);
    assert_eq!(rows[3]["result"]["isError"], false, "{}", rows[3]);
    assert!(
        rows[4]["result"]["contents"][0]["text"]
            .as_str()
            .unwrap()
            .len()
            > 10
    );
}
