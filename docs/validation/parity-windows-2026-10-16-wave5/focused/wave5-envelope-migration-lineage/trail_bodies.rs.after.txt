use serde_json::json;
use yoke::{
    Engine,
    capture::Deadline,
    project_session::{HostProfile, Session},
    trail_adapter,
};

#[test]
fn full_definitions_exceed_old_excerpt_limit_and_default_crux_has_eight_lines() {
    let root = tempfile::tempdir().unwrap();
    let source = format!(
        "fn payment() {{\n{}    let payment_tail_marker = 42;\n}}\n",
        "    // ordinary filler line with enough bytes\n".repeat(500)
    );
    std::fs::write(root.path().join("main.rs"), &source).unwrap();
    let deadline = Deadline::new(20000);
    let mut session = Session::open(
        Engine::open(root.path()).unwrap(),
        HostProfile::default(),
        Some("bodies"),
        &deadline,
    )
    .unwrap();
    let full = trail_adapter::invoke(
        &mut session,
        "graft_find_code",
        &json!({"query":"payment_tail_marker","full":true}),
        &deadline,
    )
    .unwrap();
    let item = &full["items"][0];
    assert!(
        item["body"]
            .as_str()
            .unwrap()
            .contains("payment_tail_marker = 42")
    );
    assert_eq!(item["bodyTruncated"], false);
    assert_eq!(item["bodyStartByte"], item["startByte"]);
    assert_eq!(item["bodyEndByte"], item["endByte"]);
    let crux = trail_adapter::invoke(
        &mut session,
        "trail_find_code",
        &json!({"query":"payment_tail_marker"}),
        &deadline,
    )
    .unwrap();
    let item = &crux["items"][0];
    let body = item["body"].as_str().unwrap();
    assert!(body.lines().count() <= 8);
    assert!(body.contains("payment_tail_marker = 42"));
    let start = item["bodyStartByte"].as_u64().unwrap() as usize;
    let end = item["bodyEndByte"].as_u64().unwrap() as usize;
    assert_eq!(&source[start..end], body);
    let conceptual = trail_adapter::invoke(
        &mut session,
        "trail_find_code",
        &json!({"query":"payment tail marker"}),
        &deadline,
    )
    .unwrap();
    assert!(
        conceptual["items"][0]["body"]
            .as_str()
            .unwrap()
            .contains("payment_tail_marker = 42")
    );
}

#[test]
fn find_all_returns_more_than_two_hundred_source_occurrences() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(
        root.path().join("main.rs"),
        format!(
            "fn payment() {{\n{}\n}}",
            "    let needle = 42;\n".repeat(350)
        ),
    )
    .unwrap();
    let deadline = Deadline::new(20000);
    let mut session = Session::open(
        Engine::open(root.path()).unwrap(),
        HostProfile::default(),
        Some("all"),
        &deadline,
    )
    .unwrap();
    let result = trail_adapter::invoke(
        &mut session,
        "graft_find_all",
        &json!({"pattern":"needle","fixed":true}),
        &deadline,
    )
    .unwrap();
    assert_eq!(result["items"].as_array().unwrap().len(), 350);
    assert_eq!(result["total"], 350);
    assert_eq!(result["truncated"], false);
}

#[test]
fn public_file_api_returns_all_three_hundred_and_fifty_headers_in_one_projection() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(
        root.path().join("main.rs"),
        (0..350)
            .map(|n| format!("pub fn item{n}(value: usize) -> usize {{ value + 1 }}\n"))
            .collect::<String>(),
    )
    .unwrap();
    let deadline = Deadline::new(20000);
    let mut session = Session::open(
        Engine::open(root.path()).unwrap(),
        HostProfile::default(),
        Some("headers"),
        &deadline,
    )
    .unwrap();
    let result = trail_adapter::invoke(
        &mut session,
        "graft_file_api",
        &json!({"file":"main.rs"}),
        &deadline,
    )
    .unwrap();
    assert_eq!(result["items"].as_array().unwrap().len(), 350);
    assert_eq!(result["truncated"], false);
    assert!(
        result["items"]
            .as_array()
            .unwrap()
            .iter()
            .all(|row| row["signatureAvailable"] == true
                && !row["signature"].as_str().unwrap().contains("value + 1"))
    );
}

#[test]
fn large_full_definition_results_refuse_capacity_without_publishing_artifacts() {
    let root = tempfile::tempdir().unwrap();
    for n in 0..3 {
        let source = format!(
            "fn payment{n}() {{\n/*{}*/\n}}",
            "payment filler ".repeat(100000)
        );
        std::fs::write(root.path().join(format!("part{n}.rs")), source).unwrap();
    }
    let deadline = Deadline::new(30000);
    let mut session = Session::open(
        Engine::open(root.path()).unwrap(),
        HostProfile::default(),
        Some("large"),
        &deadline,
    )
    .unwrap();
    for n in 0..3 {
        let path = format!("part{n}.rs");
        let source = std::fs::read_to_string(root.path().join(&path)).unwrap();
        let full = trail_adapter::invoke(
            &mut session,
            "graft_find_code",
            &json!({"query":"payment","full":true,"limit":1,"in":path}),
            &deadline,
        )
        .unwrap();
        let items = full["items"].as_array().unwrap();
        assert_eq!(items.len(), 1);
        let item = &items[0];
        assert_eq!(item["path"], path);
        assert_eq!(item["name"], format!("payment{n}"));
        assert_eq!(item["bodyTruncated"], false);
        assert_eq!(item["bodyMode"], "full_definition");
        assert_eq!(item["contentHash"], yoke::model::hash(source.as_bytes()));
        assert_eq!(item["bodyStartByte"], item["startByte"]);
        assert_eq!(item["bodyEndByte"], item["endByte"]);
        let start = item["bodyStartByte"].as_u64().unwrap() as usize;
        let end = item["bodyEndByte"].as_u64().unwrap() as usize;
        let body = item["body"].as_str().unwrap();
        assert_eq!((start, end), (0, source.len()));
        assert_eq!(body, source);
        assert_eq!(&source[start..end], body);
        assert!(body.len() > 1_400_000);
        assert_eq!(full["retrievalScope"]["effectiveMode"], "global");
    }
    let artifact_count = |session: &Session| {
        session
            .engine
            .store
            .db
            .query_row(
                "SELECT COUNT(*) FROM records WHERE kind='artifact'",
                [],
                |row| row.get::<_, i64>(0),
            )
            .unwrap()
    };
    let before = artifact_count(&session);
    let input = format!(
        "{}\n{}\n",
        json!({"jsonrpc":"2.0","id":1,"method":"initialize"}),
        json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"graft_find_code","arguments":{"query":"payment","full":true}}})
    );
    let mut output = vec![];
    yoke::protocol::serve_trail(
        &mut session,
        "graft",
        std::io::Cursor::new(input),
        &mut output,
    )
    .unwrap();
    let response: serde_json::Value =
        serde_json::from_str(String::from_utf8(output).unwrap().lines().nth(1).unwrap()).unwrap();
    assert_eq!(response["result"]["isError"], true, "{response}");
    let envelope: serde_json::Value =
        serde_json::from_str(response["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(
        envelope,
        json!({"status":"error","data":{},"error":{
            "message":"Manifest-balanced retrieval output_capacity: compatibility_envelope"
        }})
    );
    assert_eq!(artifact_count(&session), before);
}

#[test]
fn large_find_all_results_remain_complete_readable_artifacts() {
    let root = tempfile::tempdir().unwrap();
    let source = format!("fn payment() {{\n{}}}\n", "    // needle\n".repeat(12_000));
    std::fs::write(root.path().join("main.rs"), &source).unwrap();
    let deadline = Deadline::new(120000);
    let mut session = Session::open(
        Engine::open(root.path()).unwrap(),
        HostProfile::default(),
        Some("large-occurrences"),
        &deadline,
    )
    .unwrap();
    let input = format!(
        "{}\n{}\n",
        json!({"jsonrpc":"2.0","id":1,"method":"initialize"}),
        json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{
            "name":"graft_find_all","arguments":{"pattern":"needle","fixed":true}
        }})
    );
    let mut output = vec![];
    yoke::protocol::serve_trail(
        &mut session,
        "graft",
        std::io::Cursor::new(input),
        &mut output,
    )
    .unwrap();
    let response: serde_json::Value =
        serde_json::from_str(String::from_utf8(output).unwrap().lines().nth(1).unwrap()).unwrap();
    assert_eq!(response["result"]["isError"], false, "{response}");
    let envelope: serde_json::Value =
        serde_json::from_str(response["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
    let data = &envelope["data"];
    let id = data["artifactId"].as_str().unwrap();
    let bytes = session.engine.store.read_artifact(id).unwrap();
    assert!(bytes.len() > 4_000_000);
    assert_eq!(data["totalBytes"], bytes.len());
    assert_eq!(data["resultTransport"], "immutable_paged_artifact");
    assert_eq!(
        data["resourceUri"],
        format!("ci://artifact/{id}?cursor=0&maxBytes=32000&encoding=utf8")
    );
    let full: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(full["total"], 12_000);
    assert_eq!(full["truncated"], false);
    let items = full["items"].as_array().unwrap();
    assert_eq!(items.len(), 12_000);
    for item in items {
        assert_eq!(item["path"], "main.rs");
        assert_eq!(item["contentHash"], yoke::model::hash(source.as_bytes()));
        let start = item["startByte"].as_u64().unwrap() as usize;
        let end = item["endByte"].as_u64().unwrap() as usize;
        assert_eq!(&source[start..end], "needle");
    }
}
