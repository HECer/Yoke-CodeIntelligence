use serde_json::{Value, json};
use std::{
    collections::BTreeSet,
    fs,
    io::{Read, Write},
    net::TcpStream,
    time::Duration,
};
use tempfile::TempDir;
use yoke::{
    Engine,
    capture::Deadline,
    project_session::{HostProfile, Session},
    protocol::{ConnectionState, Profile},
};

fn root() -> TempDir {
    let r = TempDir::new().unwrap();
    fs::write(r.path().join("main.rs"), "// 🌍\nfn source() {}\n").unwrap();
    r
}
fn profile(v: Value) -> HostProfile {
    serde_json::from_value(v).unwrap()
}
fn open(root: &TempDir, host: HostProfile, id: &str) -> Session {
    Session::open(
        Engine::open(root.path()).unwrap(),
        host,
        Some(id),
        &Deadline::new(20000),
    )
    .unwrap()
}
fn invoke(s: &mut Session, op: &str, args: Value) -> Value {
    s.invoke(op, &args, &Deadline::new(20000)).unwrap()
}
fn message(
    state: &mut ConnectionState,
    s: &mut Session,
    p: Profile,
    id: u64,
    method: &str,
    params: Value,
) -> Value {
    state
        .message_session(
            s,
            p,
            &serde_json::to_vec(&json!({"jsonrpc":"2.0","id":id,"method":method,"params":params}))
                .unwrap(),
        )
        .unwrap()
        .unwrap()
}
fn names(v: &Value) -> BTreeSet<String> {
    v["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap().into())
        .collect()
}
fn stored(s: &Session) -> String {
    s.engine
        .store
        .db
        .query_row(
            "SELECT value FROM records WHERE kind=?1",
            [yoke::project_session::RECORD_KIND],
            |r| r.get(0),
        )
        .unwrap()
}

#[test]
fn contexts_change_actual_instructions_catalog_and_direct_dispatch() {
    let r = root();
    let mut desktop = open(&r, profile(json!({"context":"desktop-app"})), "desktop");
    let mut claude = open(&r, profile(json!({"context":"claude-code"})), "claude");
    let mut a = ConnectionState::default();
    let mut b = ConnectionState::default();
    let init_a = message(
        &mut a,
        &mut desktop,
        Profile::Serena,
        1,
        "initialize",
        json!({}),
    );
    let init_b = message(
        &mut b,
        &mut claude,
        Profile::Serena,
        1,
        "initialize",
        json!({}),
    );
    assert_ne!(
        init_a["result"]["instructions"],
        init_b["result"]["instructions"]
    );
    assert_eq!(
        init_b["result"]["capabilities"]["tools"]["listChanged"],
        true
    );
    let full = names(&message(
        &mut a,
        &mut desktop,
        Profile::Serena,
        2,
        "tools/list",
        json!({}),
    ));
    let selected = names(&message(
        &mut b,
        &mut claude,
        Profile::Serena,
        2,
        "tools/list",
        json!({}),
    ));
    assert_eq!(full.len(), 53);
    assert_eq!(selected.len(), 42);
    assert!(
        full.contains("read_file")
            && !selected.contains("read_file")
            && selected.contains("find_symbol")
    );
    let denied = message(
        &mut b,
        &mut claude,
        Profile::Serena,
        3,
        "tools/call",
        json!({"name":"read_file","arguments":{"relative_path":"main.rs"}}),
    );
    assert_eq!(denied["result"]["isError"], true);
    assert!(
        denied
            .to_string()
            .contains("Tool excluded by session steering")
    );
    assert!(
        yoke::compatibility::invoke_serena(
            &mut claude,
            "execute_shell_command",
            &json!({"command":"not-executed"}),
            &Deadline::new(20000)
        )
        .is_err()
    );
    assert_eq!(
        fs::read_to_string(r.path().join("main.rs")).unwrap(),
        "// 🌍\nfn source() {}\n"
    );
}

#[test]
fn layered_precedence_base_added_modes_and_literal_prompt_provenance() {
    let r = root();
    let host = profile(json!({"steering":{
        "baseModes":["interactive"],
        "global":{"defaultModes":["planning"],"addedModes":["no-memories"],"instructions":"GLOBAL","excludedTools":["ci_find"]},
        "projects":{"default":{"project":{"defaultModes":["editing"],"instructions":"PROJECT"},"projectLocal":{"defaultModes":["one-shot"],"addedModes":["no-onboarding"],"instructions":"LOCAL"}}},
        "cli":{"defaultModes":["review"],"addedModes":["interactive","benchmark"],"instructions":"{{ embed_memory('private') }} 🌍","excludedTools":["ci_trace"]},
        "modes":{"review":{"instructions":"CUSTOM REVIEW","excludedTools":["ci_read"]}}
    }}));
    let mut s = open(&r, host, "layered");
    let c = invoke(&mut s, "configuration", json!({}));
    assert_eq!(
        c["modes"],
        json!([
            "interactive",
            "review",
            "no-memories",
            "no-onboarding",
            "benchmark"
        ])
    );
    assert_eq!(c["steering"]["selectionSource"], "cli");
    assert_eq!(
        c["steering"]["excludedTools"],
        json!([
            "ci_find",
            "ci_read",
            "ci_trace",
            "delete_memory",
            "edit_memory",
            "list_memories",
            "onboarding",
            "read_memory",
            "rename_memory",
            "write_memory"
        ])
    );
    let text = s.instructions();
    let ordered = [
        "GLOBAL",
        "PROJECT",
        "LOCAL",
        "Native context agent.",
        "CUSTOM REVIEW",
        "{{ embed_memory('private') }} 🌍",
    ];
    let positions: Vec<_> = ordered.iter().map(|s| text.find(s).unwrap()).collect();
    assert!(positions.windows(2).all(|p| p[0] < p[1]));
    assert_eq!(
        c["steering"]["authority"],
        "advisory_instructions_and_tool_restrictions_only"
    );
    assert!(
        s.invoke("notes.list", &json!({}), &Deadline::new(20000))
            .is_err()
    );
    assert!(
        s.invoke("onboarding", &json!({}), &Deadline::new(20000))
            .is_err()
    );
    assert!(!s.engine.allow_writes);
}

#[test]
fn project_defaults_runtime_override_reset_resume_and_removal_are_persistent() {
    let a = root();
    let b = root();
    let host = profile(
        json!({"projects":[{"name":"other","root":b.path()}],"steering":{"baseModes":["interactive"],"global":{"defaultModes":["editing"]},"projects":{"other":{"project":{"defaultModes":["planning"]},"projectLocal":{"addedModes":["no-memories"]}}}}}),
    );
    let mut s = open(&a, host.clone(), "lifecycle");
    assert_eq!(
        invoke(&mut s, "activate", json!({"project":"other"}))["modes"],
        json!(["interactive", "planning", "no-memories"])
    );
    assert_eq!(
        invoke(&mut s, "switchModes", json!({"modes":["one-shot"]}))["modes"],
        json!(["interactive", "one-shot", "no-memories"])
    );
    assert_eq!(
        invoke(&mut s, "activate", json!({"project":"default"}))["modes"],
        json!(["interactive", "one-shot"])
    );
    let before = stored(&s);
    drop(s);
    let mut s = open(&a, host, "lifecycle");
    assert_eq!(stored(&s), before);
    assert_eq!(
        invoke(&mut s, "switchModes", json!({"reset":true}))["modes"],
        json!(["interactive", "editing"])
    );
    invoke(&mut s, "activate", json!({"project":"other"}));
    invoke(&mut s, "removeProject", json!({"project":"other"}));
    let c = invoke(&mut s, "configuration", json!({}));
    assert_eq!(c["activeProject"], "default");
    assert_eq!(c["modes"], json!(["interactive", "editing"]));
    assert!(
        s.invoke(
            "activate",
            &json!({"project":"other"}),
            &Deadline::new(20000)
        )
        .is_err()
    );
}

#[test]
fn mode_changes_emit_only_real_catalog_notifications_and_queue_is_bounded() {
    let r = root();
    let host = profile(json!({"steering":{"modes":{"quiet":{"excludedTools":["ci_find"]}}}}));
    let mut s = open(&r, host.clone(), "notifications");
    let mut state = ConnectionState::default();
    message(
        &mut state,
        &mut s,
        Profile::Session,
        1,
        "initialize",
        json!({}),
    );
    for (id, modes, changed) in [
        (2, json!(["quiet"]), true),
        (3, json!(["quiet", "interactive"]), false),
        (4, json!(["editing"]), true),
    ] {
        let response = message(
            &mut state,
            &mut s,
            Profile::Session,
            id,
            "tools/call",
            json!({"name":"ci_session","arguments":{"operation":"switchModes","args":{"modes":modes}}}),
        );
        assert_eq!(response["result"]["isError"], false);
        let events = state.take_notifications();
        assert_eq!(events.len(), usize::from(changed));
        if changed {
            assert_eq!(
                events[0],
                json!({"jsonrpc":"2.0","method":"notifications/tools/list_changed"})
            );
        }
        assert!(state.take_notifications().is_empty());
    }
    let mut raw = open(&r, host, "stdio-notifications");
    let mut out = Vec::new();
    let input = [json!({"jsonrpc":"2.0","id":1,"method":"initialize"}),json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"ci_session","arguments":{"operation":"switchModes","args":{"modes":["quiet"]}}}})].iter().map(|v| format!("{v}\n")).collect::<String>();
    yoke::protocol::serve_session(&mut raw, std::io::Cursor::new(input), &mut out).unwrap();
    let values: Vec<Value> = std::str::from_utf8(&out)
        .unwrap()
        .lines()
        .map(|s| serde_json::from_str(s).unwrap())
        .collect();
    assert_eq!(values.len(), 3);
    assert_eq!(values[2]["method"], "notifications/tools/list_changed");
}

#[test]
fn all_session_protocol_profiles_enforce_host_exclusions_before_dispatch() {
    for (p, name) in [
        (Profile::Session, "ci_find"),
        (Profile::Serena, "find_symbol"),
        (Profile::Graphify, "graphify_query"),
        (Profile::Trail, "find_code"),
        (Profile::Graft, "find_code"),
    ] {
        // Derive the real names for adapters whose public catalogs have prefixes.
        let catalog = match p {
            Profile::Graphify => yoke::graphify_adapter::tools(),
            Profile::Trail => yoke::trail_adapter::tools("trail"),
            Profile::Graft => yoke::trail_adapter::tools("graft"),
            _ => json!([]),
        };
        let name = catalog
            .as_array()
            .unwrap()
            .first()
            .and_then(|v| v["name"].as_str())
            .unwrap_or(name);
        let r = root();
        let mut s = open(
            &r,
            profile(json!({"steering":{"global":{"excludedTools":[name]}}})),
            "profile",
        );
        let mut state = ConnectionState::default();
        message(&mut state, &mut s, p, 1, "initialize", json!({}));
        assert!(!names(&message(&mut state, &mut s, p, 2, "tools/list", json!({}))).contains(name));
        let denied = message(
            &mut state,
            &mut s,
            p,
            3,
            "tools/call",
            json!({"name":name,"arguments":{}}),
        );
        assert_eq!(denied["result"]["isError"], true);
        assert!(
            denied
                .to_string()
                .contains("Tool excluded by session steering")
        );
    }
}

#[test]
fn every_trail_graft_alias_shares_catalog_and_direct_adapter_exclusion_identity() {
    for (canonical, alias) in [
        ("find_code", "ask"),
        ("find_all", "grep"),
        ("trace_calls", "callers"),
        ("file_api", "skeleton"),
        ("repo_map", "map"),
        ("check_freshness", "check"),
    ] {
        let r = root();
        for excluded in [format!("graft_{canonical}"), format!("trail_{alias}")] {
            let mut s = open(
                &r,
                profile(json!({"steering":{"global":{"excludedTools":[excluded]}}})),
                &excluded,
            );
            let before = s
                .engine
                .store
                .db
                .query_row::<i64, _, _>("SELECT COUNT(*) FROM snapshots", [], |r| r.get(0))
                .unwrap();
            for prefix in ["graft", "trail"] {
                for suffix in [canonical, alias] {
                    let name = format!("{prefix}_{suffix}");
                    assert!(!s.tool_allowed(&name));
                    let error = yoke::trail_adapter::invoke(
                        &mut s,
                        &name,
                        &json!({}),
                        &Deadline::new(20000),
                    )
                    .unwrap_err();
                    assert!(
                        error
                            .to_string()
                            .contains("Tool excluded by session steering")
                    );
                }
            }
            assert!(s.tool_allowed("ci_find"));
            let mut state = ConnectionState::default();
            message(
                &mut state,
                &mut s,
                Profile::Trail,
                1,
                "initialize",
                json!({}),
            );
            let tools = names(&message(
                &mut state,
                &mut s,
                Profile::Trail,
                2,
                "tools/list",
                json!({}),
            ));
            assert!(!tools.contains(&format!("trail_{canonical}")));
            assert_eq!(tools.len(), 5);
            let response = message(
                &mut state,
                &mut s,
                Profile::Trail,
                3,
                "tools/call",
                json!({"name":format!("graft_{alias}"),"arguments":{}}),
            );
            assert_eq!(response["result"]["isError"], true);
            assert!(
                response
                    .to_string()
                    .contains("Tool excluded by session steering")
            );
            assert_eq!(
                s.engine
                    .store
                    .db
                    .query_row::<i64, _, _>("SELECT COUNT(*) FROM snapshots", [], |r| r.get(0))
                    .unwrap(),
                before
            );
        }
    }
}

#[test]
fn stale_host_cas_and_forged_modes_refuse_without_changing_original_record() {
    let r = root();
    let host = profile(json!({"steering":{}}));
    let mut first = open(&r, host.clone(), "cas");
    let mut stale = open(&r, host.clone(), "cas");
    invoke(&mut first, "switchModes", json!({"modes":["planning"]}));
    let expected = stored(&first);
    assert!(
        stale
            .invoke(
                "switchModes",
                &json!({"modes":["editing"]}),
                &Deadline::new(20000)
            )
            .is_err()
    );
    assert_eq!(stored(&first), expected);
    let changed = profile(json!({"steering":{"global":{"instructions":"changed"}}}));
    assert!(
        Session::open(
            Engine::open(r.path()).unwrap(),
            changed,
            Some("cas"),
            &Deadline::new(20000)
        )
        .is_err()
    );
    assert_eq!(stored(&first), expected);
    let mut forged: Value = serde_json::from_str(&expected).unwrap();
    forged["modes"] = json!(["editing"]);
    first
        .engine
        .store
        .db
        .execute(
            "UPDATE records SET value=?1 WHERE kind=?2",
            rusqlite::params![forged.to_string(), yoke::project_session::RECORD_KIND],
        )
        .unwrap();
    assert!(
        Session::open(
            Engine::open(r.path()).unwrap(),
            host,
            Some("cas"),
            &Deadline::new(20000)
        )
        .is_err()
    );
    assert_eq!(stored(&first), forged.to_string());
}

#[test]
fn adversarial_schema_names_unicode_and_budgets_are_rejected_before_session_insert() {
    let cases = [
        json!({"steering":{"global":{"activationCommand":"never-run"}}}),
        json!({"steering":{"modes":{"planning":{}}}}),
        json!({"steering":{"contexts":{"agent":{}}}}),
        json!({"steering":{"global":{"excludedTools":["ci_fіnd"]}}}),
        json!({"steering":{"global":{"excludedTools":["ci_find","ci_find"]}}}),
        json!({"steering":{"baseModes":["unknown"]}}),
        json!({"steering":{"modes":{"réview":{}}}}),
        json!({"steering":{"projects":{"../../foreign":{}}}}),
        json!({"steering":{"global":{"instructions":"x".repeat(8193)}}}),
        json!({"steering":{"cli":{"instructions":"bad\0text"}}}),
        json!({"steering":{"global":{"defaultModes":[]}}}),
        json!({"context":"unregistered","steering":{}}),
    ];
    for value in cases {
        let r = root();
        let engine = Engine::open(r.path()).unwrap();
        if let Ok(host) = serde_json::from_value::<HostProfile>(value) {
            assert!(Session::open(engine, host, Some("refused"), &Deadline::new(20000)).is_err());
        }
        let engine = Engine::open(r.path()).unwrap();
        assert_eq!(
            engine
                .store
                .db
                .query_row::<i64, _, _>(
                    "SELECT COUNT(*) FROM records WHERE kind=?1",
                    [yoke::project_session::RECORD_KIND],
                    |r| r.get(0)
                )
                .unwrap(),
            0
        );
    }
    assert!(yoke::json::parse(br#"{"steering":{"global":{},"global":{}}}"#, 262144).is_err());
}

#[test]
fn custom_modes_and_contexts_remain_host_owned_and_do_not_grant_writes_or_execute_templates() {
    let r = root();
    fs::create_dir(r.path().join(".serena")).unwrap();
    fs::write(
        r.path().join(".serena/project.yml"),
        "activation_command: touch PWNED\ndefault_modes: [editing]\n",
    )
    .unwrap();
    let host = profile(
        json!({"context":"review-client","modes":["editing","review"],"steering":{"contexts":{"review-client":{"instructions":"$(touch PWNED) {{ 7*7 }} 🌍","excludedTools":["execute_shell_command"]}},"modes":{"review":{"instructions":"Review exact bytes."}}}}),
    );
    let mut s = open(&r, host, "custom");
    assert!(s.instructions().contains("$(touch PWNED) {{ 7*7 }} 🌍"));
    assert!(s.require_edit_permission().is_ok());
    assert!(!s.engine.allow_writes);
    assert!(!r.path().join("PWNED").exists());
    assert!(
        s.invoke(
            "switchModes",
            &json!({"modes":["review"],"reset":true}),
            &Deadline::new(20000)
        )
        .is_err()
    );
    let before = stored(&s);
    assert!(
        s.invoke(
            "switchModes",
            &json!({"modes":["not-registered"]}),
            &Deadline::new(20000)
        )
        .is_err()
    );
    assert_eq!(stored(&s), before);
    assert!(!invoke(&mut s, "configuration", json!({}))["steering"].is_null());
}

#[test]
fn legacy_agent_profile_and_record_fingerprint_and_wire_shape_are_unchanged() {
    let r = root();
    let host = HostProfile::default();
    assert!(
        serde_json::to_value(&host)
            .unwrap()
            .get("steering")
            .is_none()
    );
    let engine = Engine::open(r.path()).unwrap();
    let fingerprint = yoke::model::identity(&(
        &engine.store.workspace_id,
        Vec::<(String, String, bool, bool)>::new(),
        &host.context,
        &host.modes,
        &host.global_notes_root,
        &host.notes_policy.read_only_patterns,
        &host.notes_policy.ignored_patterns,
        host.notes_policy.writable,
        engine.allow_writes,
        &engine.capture_options,
        Vec::<(String, String, Vec<String>, bool)>::new(),
        &engine.server_configurations,
    ));
    let mut s = Session::open(engine, host, Some("legacy"), &Deadline::new(20000)).unwrap();
    let value: Value = serde_json::from_str(&stored(&s)).unwrap();
    assert_eq!(value["hostFingerprint"], fingerprint);
    assert!(value.get("modeOverride").is_none());
    assert_eq!(
        s.instructions(),
        yoke::session_steering::LEGACY_INSTRUCTIONS
    );
    assert!(
        invoke(&mut s, "configuration", json!({}))
            .get("steering")
            .is_none()
    );
    let mut state = ConnectionState::default();
    assert_eq!(
        message(
            &mut state,
            &mut s,
            Profile::Serena,
            1,
            "initialize",
            json!({})
        )["result"]["capabilities"]["tools"]["listChanged"],
        false
    );
    assert_eq!(
        names(&message(
            &mut state,
            &mut s,
            Profile::Serena,
            2,
            "tools/list",
            json!({})
        ))
        .len(),
        53
    );
}

#[test]
fn expanded_prompt_budget_and_mode_capacity_fail_without_a_partial_mode_update() {
    let r = root();
    let defs: serde_json::Map<String, Value> = (0..17)
        .map(|n| (format!("mode{n}"), json!({"instructions":"x".repeat(8192)})))
        .collect();
    let host = profile(json!({"steering":{"modes":defs}}));
    let mut s = open(&r, host, "expanded");
    let before = stored(&s);
    let modes: Vec<_> = (0..17).map(|n| format!("mode{n}")).collect();
    assert!(
        s.invoke(
            "switchModes",
            &json!({"modes":modes}),
            &Deadline::new(20000)
        )
        .unwrap_err()
        .to_string()
        .contains("Expanded steering instruction capacity")
    );
    assert_eq!(stored(&s), before);
    assert!(
        s.invoke(
            "switchModes",
            &json!({"modes":vec!["editing";42]}),
            &Deadline::new(20000)
        )
        .is_err()
    );
    assert_eq!(stored(&s), before);
}

#[test]
fn real_http_mode_change_delivers_bounded_sse_notification_then_response() {
    use yoke::mcp_http::{self, Config, Principal, Target};
    let r = root();
    let host = profile(json!({"steering":{"modes":{"quiet":{"excludedTools":["ci_find"]}}}}));
    let principal = Principal::new(
        "steering-test",
        zeroize::Zeroizing::new(vec![61; 32]),
        Target::new(r.path(), Profile::Session, host),
    )
    .unwrap();
    let server = mcp_http::serve(Config::default(), vec![principal]).unwrap();
    let address = server.local_addr();
    let exchange = |sid: Option<&str>, body: Value| {
        let body = body.to_string();
        let mut stream = TcpStream::connect(address).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(15)))
            .unwrap();
        let session = sid
            .map(|s| format!("Mcp-Session-Id: {s}\r\nMcp-Protocol-Version: 2025-11-25\r\n"))
            .unwrap_or_default();
        write!(stream,"POST /mcp HTTP/1.1\r\nHost: {address}\r\nAuthorization: Bearer {}\r\nAccept: application/json, text/event-stream\r\nContent-Type: application/json\r\nContent-Length: {}\r\n{session}\r\n{body}",hex::encode([61;32]),body.len()).unwrap();
        let mut bytes = Vec::new();
        stream.read_to_end(&mut bytes).unwrap();
        let text = String::from_utf8(bytes).unwrap();
        let (header, body) = text.split_once("\r\n\r\n").unwrap();
        assert!(header.starts_with("HTTP/1.1 200 "));
        (header.to_owned(), body.to_owned())
    };
    let (header, body) = exchange(
        None,
        json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25"}}),
    );
    assert_eq!(
        serde_json::from_str::<Value>(&body).unwrap()["result"]["capabilities"]["tools"]["listChanged"],
        true
    );
    let sid = header
        .lines()
        .find_map(|s| {
            s.split_once(':')
                .filter(|(k, _)| k.eq_ignore_ascii_case("mcp-session-id"))
                .map(|(_, v)| v.trim())
        })
        .unwrap();
    let (header, body) = exchange(
        Some(sid),
        json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"ci_session","arguments":{"operation":"switchModes","args":{"modes":["quiet"]}}}}),
    );
    assert!(
        header.contains("Content-Type: text/event-stream\r\n")
            || header.ends_with("Content-Type: text/event-stream")
    );
    let events: Vec<Value> = body
        .lines()
        .filter_map(|l| l.strip_prefix("data: "))
        .map(|s| serde_json::from_str(s).unwrap())
        .collect();
    assert_eq!(events.len(), 2);
    assert_eq!(events[0]["method"], "notifications/tools/list_changed");
    assert_eq!(events[1]["id"], 2);
    assert_eq!(events[1]["result"]["isError"], false);
    let (header, body) = exchange(
        Some(sid),
        json!({"jsonrpc":"2.0","id":3,"method":"tools/list"}),
    );
    assert!(header.contains("Content-Type: application/json"));
    assert!(!names(&serde_json::from_str(&body).unwrap()).contains("ci_find"));
    server.shutdown().unwrap();
    assert!(TcpStream::connect(address).is_err());
}
