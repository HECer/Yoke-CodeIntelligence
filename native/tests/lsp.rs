use serde_json::json;
use std::{
    fs,
    path::Path,
    sync::{Mutex, MutexGuard},
};
use tempfile::TempDir;
use yoke::{
    Engine,
    capture::Deadline,
    lsp::{self, Server},
};
// Concurrent Windows Node/JobObject startup measured 1.5-1.85 seconds per
// fixture, versus 146-235 ms sequentially. Acquire before any measured phase.
static PROTOCOL_FIXTURE: Mutex<()> = Mutex::new(());
fn protocol_fixture() -> MutexGuard<'static, ()> {
    PROTOCOL_FIXTURE
        .lock()
        .unwrap_or_else(|error| error.into_inner())
}
fn server(adversary: &str) -> Server {
    Server {
        command: "node".into(),
        args: vec![
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("tests/fixtures/lsp.mjs")
                .display()
                .to_string(),
            adversary.into(),
        ],
        language: "rust".into(),
        trust: true,
    }
}
#[test]
fn protocol_fixture_locations_bind_to_exact_immutable_source() {
    let _fixture = protocol_fixture();
    let dir = TempDir::new().unwrap();
    fs::write(
        dir.path().join("main.rs"),
        "// 🌍\nfn helper() {}\nfn main() { helper(); }\n",
    )
    .unwrap();
    let mut engine = Engine::open(dir.path()).unwrap();
    let result = lsp::run(
        &mut engine,
        &server(""),
        "main.rs",
        44,
        "definition",
        None,
        &Deadline::new(20000),
    )
    .unwrap();
    assert_eq!(result["locations"][0]["path"], "main.rs");
    assert_eq!(result["locations"][0]["startByte"], 11);
    assert_eq!(result["locations"][0]["endByte"], 17);
}
#[test]
fn protocol_fixture_rename_prepares_a_plan_without_touching_source() {
    let _fixture = protocol_fixture();
    let dir = TempDir::new().unwrap();
    let text = "// 🌍\nfn helper() {}\nfn main() { helper(); }\n";
    fs::write(dir.path().join("main.rs"), text).unwrap();
    let mut engine = Engine::open(dir.path()).unwrap();
    let result = lsp::run(
        &mut engine,
        &server(""),
        "main.rs",
        44,
        "rename",
        Some("renamed"),
        &Deadline::new(20000),
    )
    .unwrap();
    assert_eq!(
        fs::read_to_string(dir.path().join("main.rs")).unwrap(),
        text
    );
    let read = engine
        .invoke(
            "ci_read",
            json!({"snapshotId":result["plan"]["resultSnapshotId"],"path":"main.rs"}),
        )
        .unwrap();
    assert!(
        read["data"]["text"]
            .as_str()
            .unwrap()
            .contains("fn renamed")
    );
}
#[test]
fn protocol_fixture_unsafe_workspace_edits_fail_closed() {
    let _fixture = protocol_fixture();
    for adversary in ["outside", "version"] {
        let dir = TempDir::new().unwrap();
        let text = "// 🌍\nfn helper() {}\nfn main() { helper(); }\n";
        fs::write(dir.path().join("main.rs"), text).unwrap();
        let mut engine = Engine::open(dir.path()).unwrap();
        assert!(
            lsp::run(
                &mut engine,
                &server(adversary),
                "main.rs",
                44,
                "rename",
                Some("renamed"),
                &Deadline::new(20000)
            )
            .is_err()
        );
        assert_eq!(
            fs::read_to_string(dir.path().join("main.rs")).unwrap(),
            text
        );
    }
}

#[test]
fn host_registered_semantic_rename_uses_the_requested_mcp_snapshot() {
    let _fixture = protocol_fixture();
    let dir = TempDir::new().unwrap();
    let text = "// 🌍\nfn helper() {}\nfn main() { helper(); }\n";
    fs::write(dir.path().join("main.rs"), text).unwrap();
    let mut engine = Engine::open(dir.path()).unwrap();
    engine.register_server(server("")).unwrap();
    let found = engine.invoke("ci_find", json!({"query":"helper"})).unwrap();
    let symbol = found["data"]["items"][0]["id"].clone();
    fs::write(dir.path().join("main.rs"), "fn completely_different() {}\n").unwrap();
    let plan=engine.invoke("ci_change_prepare",json!({"snapshotId":found["snapshotId"],"rename":{"entityId":symbol,"newName":"renamed"}})).unwrap();
    assert_eq!(plan["status"], "ok", "{plan}");
    assert_eq!(plan["snapshotId"], found["snapshotId"]);
    assert_eq!(
        fs::read_to_string(dir.path().join("main.rs")).unwrap(),
        "fn completely_different() {}\n"
    );
}

#[test]
fn lsp_deadline_includes_a_server_that_refuses_to_read_didopen() {
    let _fixture = protocol_fixture();
    let dir = TempDir::new().unwrap();
    fs::write(
        dir.path().join("main.rs"),
        format!("// {}\nfn main(){{}}", "x".repeat(200000)),
    )
    .unwrap();
    let mut engine = Engine::open(dir.path()).unwrap();
    let started = std::time::Instant::now();
    let result = lsp::run(
        &mut engine,
        &server("blocked"),
        "main.rs",
        0,
        "definition",
        None,
        &Deadline::new(300),
    );
    assert!(result.is_err());
    assert!(
        started.elapsed() < std::time::Duration::from_secs(2),
        "Blocked stdin outlived deadline: {:?}",
        started.elapsed()
    );
}

#[test]
fn lsp_shutdown_grace_uses_the_remaining_request_deadline() {
    let _fixture = protocol_fixture();
    let dir = TempDir::new().unwrap();
    fs::write(
        dir.path().join("main.rs"),
        "// 🌍\nfn helper() {}\nfn main() { helper(); }\n",
    )
    .unwrap();
    let mut engine = Engine::open(dir.path()).unwrap();
    let started = std::time::Instant::now();
    let result = lsp::run(
        &mut engine,
        &server("slow-exit"),
        "main.rs",
        44,
        "definition",
        None,
        &Deadline::new(800),
    );
    assert!(
        result.is_err(),
        "A server that exceeds the shared shutdown deadline cannot return completed evidence"
    );
    assert!(
        started.elapsed() < std::time::Duration::from_secs(2),
        "Shutdown grace outlived request budget: {:?}",
        started.elapsed()
    );
}

#[test]
fn semantic_positions_preserve_cr_crlf_lf_and_unicode_boundaries() {
    let text = "a🌍\rcafé\r\nlast\n";
    for (byte, line, character) in [
        (0, 0, 0),
        (1, 0, 1),
        (5, 0, 3),
        (6, 1, 0),
        (11, 1, 4),
        (13, 2, 0),
        (17, 2, 4),
        (18, 3, 0),
    ] {
        let position = json!({"line":line,"character":character});
        assert_eq!(lsp::byte_to_position(text, byte).unwrap(), position);
        assert_eq!(lsp::position_to_byte(text, &position).unwrap(), byte);
    }
    assert!(lsp::byte_to_position(text, 12).is_err());
    assert!(lsp::position_to_byte(text, &json!({"line":0,"character":2})).is_err());
}

#[test]
fn semantic_location_link_selection_must_be_inside_its_target_range() {
    let _fixture = protocol_fixture();
    let dir = TempDir::new().unwrap();
    fs::write(
        dir.path().join("main.rs"),
        "// 🌍\nfn helper() {}\nfn main() { helper(); }\n",
    )
    .unwrap();
    let mut engine = Engine::open(dir.path()).unwrap();
    let result = lsp::run(
        &mut engine,
        &server("outside-selection"),
        "main.rs",
        44,
        "definition",
        None,
        &Deadline::new(10000),
    );
    assert!(
        result.is_err(),
        "Contradictory LocationLink accepted: {result:?}"
    );
}

#[test]
fn repeated_large_lsp_locations_remain_exact_anchors_with_bounded_body_bytes() {
    let _fixture = protocol_fixture();
    let root = TempDir::new().unwrap();
    let text = format!("// {}\nfn main() {{}}\n", "x".repeat(100000));
    fs::write(root.path().join("main.rs"), &text).unwrap();
    let mut engine = Engine::open(root.path()).unwrap();
    let result = lsp::run(
        &mut engine,
        &server("many-large-locations"),
        "main.rs",
        0,
        "references",
        None,
        &Deadline::new(20000),
    )
    .unwrap();
    assert!(
        serde_json::to_vec(&result).unwrap().len() < 1_000_000,
        "Provider ranges amplified repeated source bodies"
    );
    let locations = result["locations"].as_array().unwrap();
    assert_eq!(locations.len(), 128);
    for item in locations {
        assert_eq!(item["startByte"], 0);
        assert_eq!(item["endByte"], 100004);
        assert_eq!(item["bodyOmitted"], true);
        assert!(item["text"].is_null());
    }
    let mut arguments = locations[0]["readWith"].clone();
    arguments.as_object_mut().unwrap().remove("tool");
    arguments["maxBytes"] = json!(120000);
    let read = engine.invoke("ci_read", arguments).unwrap();
    assert_eq!(read["status"], "ok");
    assert_eq!(read["snapshotId"], result["snapshotId"]);
    assert_eq!(read["data"]["contentHash"], locations[0]["contentHash"]);
    assert_eq!(read["data"]["text"], &text[..100004]);
}
