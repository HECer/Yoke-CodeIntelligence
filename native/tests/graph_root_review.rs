use serde_json::{Value, json};
use yoke::Engine;

#[test]
fn artifact_resource_page_respects_byte_budget_after_json_control_escaping() {
    let root = tempfile::tempdir().unwrap();
    let mut engine = Engine::open(root.path()).unwrap();
    let id = engine.store.artifact(&vec![1u8; 4000]).unwrap();
    let uri = format!("ci://artifact/{id}?cursor=0&maxBytes=1024&encoding=utf8");
    let input = format!(
        "{}\n{}\n",
        json!({"jsonrpc":"2.0","id":1,"method":"initialize"}),
        json!({"jsonrpc":"2.0","id":2,"method":"resources/read","params":{"uri":uri}})
    );
    let mut output = vec![];
    yoke::protocol::serve(&mut engine, std::io::Cursor::new(input), &mut output).unwrap();
    let rows: Vec<Value> = String::from_utf8(output)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert!(
        rows[1].get("error").is_none(),
        "valid UTF8 control data should be readable"
    );
    let page = rows[1]["result"]["contents"][0]["text"].as_str().unwrap();
    assert!(
        page.len() <= 1024,
        "resource page JSON exceeded1024bytebudget after control escaping: {}",
        page.len()
    );
}
#[test]
fn trail_exact_file_selector_can_trace_more_than_two_hundred_declarations() {
    use yoke::{
        capture::Deadline,
        project_session::{HostProfile, Session},
        trail_adapter,
    };
    let root = tempfile::tempdir().unwrap();
    let source = (0..300)
        .map(|n| format!("pub fn item{n}(){{}}\n"))
        .collect::<String>();
    std::fs::write(root.path().join("api.rs"), source).unwrap();
    let deadline = Deadline::new(20000);
    let mut session = Session::open(
        Engine::open(root.path()).unwrap(),
        HostProfile::default(),
        Some("graph-review"),
        &deadline,
    )
    .unwrap();
    let result = trail_adapter::invoke(
        &mut session,
        "graft_trace_calls",
        &json!({"symbol":"api.rs","depth":"all"}),
        &deadline,
    )
    .unwrap();
    assert!(result["matchedRoots"].as_array().unwrap().len() >= 300);
}
