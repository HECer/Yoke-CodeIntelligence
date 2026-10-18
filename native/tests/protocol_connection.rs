use serde_json::{Value, json};
use yoke::{Engine, protocol::Connection};

fn call(connection: &mut Connection<'_>, message: Value) -> Option<Value> {
    connection
        .message(&serde_json::to_vec(&message).unwrap())
        .unwrap()
}

#[test]
fn separate_transport_messages_preserve_initialization_and_profile_state() {
    let root = tempfile::tempdir().unwrap();
    let mut engine = Engine::open(root.path()).unwrap();
    let mut connection = Connection::native(&mut engine);
    let uninitialized = call(
        &mut connection,
        json!({"jsonrpc":"2.0","id":1,"method":"tools/list"}),
    )
    .unwrap();
    assert_eq!(uninitialized["error"]["code"], -32002);
    let initialized = call(&mut connection, json!({"jsonrpc":"2.0","id":2,"method":"initialize","params":{"protocolVersion":"2025-11-25"}})).unwrap();
    assert_eq!(initialized["result"]["protocolVersion"], "2025-11-25");
    assert!(
        call(
            &mut connection,
            json!({"jsonrpc":"2.0","method":"notifications/initialized"})
        )
        .is_none()
    );
    let tools = call(
        &mut connection,
        json!({"jsonrpc":"2.0","id":3,"method":"tools/list"}),
    )
    .unwrap();
    assert_eq!(tools["result"]["tools"].as_array().unwrap().len(), 8);
    assert!(connection.message(b"{}\n{}").is_err());
    assert!(connection.message(&vec![b' '; 4_000_001]).is_err());
    let ping = call(
        &mut connection,
        json!({"jsonrpc":"2.0","id":4,"method":"ping"}),
    )
    .unwrap();
    assert_eq!(ping["result"], json!({}));
}

#[test]
fn host_selected_profiles_keep_their_public_surface_across_messages() {
    use yoke::{
        capture::Deadline,
        project_session::{HostProfile, Session},
    };
    let root = tempfile::tempdir().unwrap();
    let mut session = Session::open(
        Engine::open(root.path()).unwrap(),
        HostProfile::default(),
        Some("connection-profiles"),
        &Deadline::new(20000),
    )
    .unwrap();
    for (profile, count, prefix) in [
        ("session", 9, "ci_"),
        ("graphify", 10, ""),
        ("serena", 53, ""),
        ("trail", 6, "trail_"),
        ("graft", 6, "graft_"),
    ] {
        let mut connection = match profile {
            "session" => Connection::session(&mut session),
            "graphify" => Connection::graphify(&mut session),
            "serena" => Connection::serena(&mut session),
            "trail" => Connection::trail(&mut session),
            "graft" => Connection::graft(&mut session),
            _ => unreachable!(),
        };
        let initialized = call(
            &mut connection,
            json!({"jsonrpc":"2.0","id":1,"method":"initialize"}),
        )
        .unwrap();
        assert!(initialized.get("error").is_none(), "{profile}");
        let listed = call(
            &mut connection,
            json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}),
        )
        .unwrap();
        let tools = listed["result"]["tools"].as_array().unwrap();
        assert_eq!(tools.len(), count, "{profile}");
        assert!(
            tools
                .iter()
                .all(|tool| tool["name"].as_str().unwrap().starts_with(prefix)),
            "{profile}"
        );
    }
}

#[test]
fn incomplete_stdio_frames_are_rejected_before_dispatch_or_publication() {
    for method in ["initialize", "tools/call", "resources/read"] {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("main.rs"), "fn main() {}\n").unwrap();
        let mut engine = Engine::open(root.path()).unwrap();
        let artifact = engine.store.artifact(b"fixture data").unwrap();
        let message = match method {
            "initialize" => json!({"jsonrpc":"2.0","id":2,"method":method}),
            "tools/call" => {
                json!({"jsonrpc":"2.0","id":2,"method":method,"params":{"name":"ci_index","arguments":{}}})
            }
            _ => {
                json!({"jsonrpc":"2.0","id":2,"method":method,"params":{"uri":format!("ci://artifact/{artifact}")}})
            }
        };
        let input = if method == "initialize" {
            message.to_string()
        } else {
            json!({"jsonrpc":"2.0","id":1,"method":"initialize"}).to_string()
                + "\n"
                + &message.to_string()
        };
        let mut output = vec![];
        assert!(
            yoke::protocol::serve(&mut engine, std::io::Cursor::new(input), &mut output).is_err(),
            "{method}"
        );
        assert_eq!(
            engine.store.latest().unwrap(),
            None,
            "Incomplete {method} published source state"
        );
        assert_eq!(
            output.iter().filter(|&&byte| byte == b'\n').count(),
            usize::from(method != "initialize"),
            "Incomplete {method} emitted a response"
        );
    }
}

#[test]
fn legal_request_ids_and_escaped_source_fit_the_complete_response_envelope() {
    let root = tempfile::tempdir().unwrap();
    let source = "\"".repeat(1_500_000);
    std::fs::write(root.path().join("data.txt"), &source).unwrap();
    let mut engine = Engine::open(root.path()).unwrap();
    engine.index(&yoke::capture::Deadline::new(120000)).unwrap();
    let mut connection = Connection::native(&mut engine);
    call(
        &mut connection,
        json!({"jsonrpc":"2.0","id":1,"method":"initialize"}),
    );
    let identifier = "i".repeat(2_000_000);
    let response=call(&mut connection,json!({"jsonrpc":"2.0","id":identifier,"method":"tools/call","params":{"name":"ci_read","arguments":{"path":"data.txt","maxBytes":4_000_000}}})).unwrap();
    assert_eq!(response["id"].as_str().unwrap().len(), 2_000_000);
    let result: Value =
        serde_json::from_str(response["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
    assert!(
        result["data"]["text"].as_str() == Some(source.as_str()),
        "Read response keys: {:?}; data keys: {:?}",
        result
            .as_object()
            .map(|value| value.keys().collect::<Vec<_>>()),
        result["data"]
            .as_object()
            .map(|value| value.keys().collect::<Vec<_>>())
    );
}

#[test]
fn owned_transport_state_retains_initialization_and_refuses_profile_switches() {
    use yoke::{
        capture::Deadline,
        project_session::{HostProfile, Session},
        protocol::{ConnectionState, Profile},
    };
    let root = tempfile::tempdir().unwrap();
    let mut session = Session::open(
        Engine::open(root.path()).unwrap(),
        HostProfile::default(),
        Some("owned-transport-state"),
        &Deadline::new(20000),
    )
    .unwrap();
    let mut state = ConnectionState::default();
    let init = json!({"jsonrpc":"2.0","id":1,"method":"initialize"}).to_string();
    assert!(
        state
            .message_session(&mut session, Profile::Graphify, init.as_bytes())
            .unwrap()
            .unwrap()
            .get("error")
            .is_none()
    );
    let list = json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}).to_string();
    let result = state
        .message_session(&mut session, Profile::Graphify, list.as_bytes())
        .unwrap()
        .unwrap();
    assert_eq!(result["result"]["tools"].as_array().unwrap().len(), 10);
    assert!(
        state
            .message_session(&mut session, Profile::Serena, list.as_bytes())
            .is_err()
    );
}
