use serde_json::{Value, json};
use std::{collections::BTreeSet, fs};
use tempfile::TempDir;
use yoke::{
    Engine,
    capture::Deadline,
    project_session::{HostProfile, Session},
    protocol::{ConnectionState, Profile},
};
fn root() -> TempDir {
    let r = TempDir::new().unwrap();
    fs::write(r.path().join("main.rs"), "// β\nfn source() {}\n").unwrap();
    r
}
fn host(v: Value) -> HostProfile {
    serde_json::from_value(v).unwrap()
}
fn open(r: &TempDir, h: HostProfile, id: &str) -> Session {
    Session::open(
        Engine::open(r.path()).unwrap(),
        h,
        Some(id),
        &Deadline::new(30000),
    )
    .unwrap()
}
fn call(s: &mut Session, op: &str, args: Value) -> Value {
    s.invoke(op, &args, &Deadline::new(30000)).unwrap()
}
fn message(
    c: &mut ConnectionState,
    s: &mut Session,
    p: Profile,
    method: &str,
    params: Value,
) -> Value {
    c.message_session(
        s,
        p,
        &serde_json::to_vec(&json!({"jsonrpc":"2.0","id":1,"method":method,"params":params}))
            .unwrap(),
    )
    .unwrap()
    .unwrap()
}
fn names(v: &Value) -> BTreeSet<&str> {
    v["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|x| x["name"].as_str().unwrap())
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
fn snapshots(s: &Session) -> u64 {
    s.engine
        .store
        .db
        .query_row("SELECT count(*) FROM snapshots", [], |r| r.get(0))
        .unwrap()
}

#[test]
fn fixed_base_preserves_exact_original_schemas_and_controls_real_calls_before_source_work() {
    let r = root();
    let mut s = open(
        &r,
        host(
            json!({"context":"desktop-app","steering":{"global":{"fixedTools":["get_current_config","list_memories"]}}}),
        ),
        "selected",
    );
    let mut c = ConnectionState::default();
    message(&mut c, &mut s, Profile::Serena, "initialize", json!({}));
    let list = message(&mut c, &mut s, Profile::Serena, "tools/list", json!({}));
    assert_eq!(
        names(&list),
        BTreeSet::from(["get_current_config", "list_memories"])
    );
    let expected: Vec<_> = yoke::compatibility::serena_tools()
        .as_array()
        .unwrap()
        .iter()
        .filter(|v| v["name"] == "get_current_config" || v["name"] == "list_memories")
        .cloned()
        .collect();
    assert_eq!(list["result"]["tools"], json!(expected));
    let cfg = message(
        &mut c,
        &mut s,
        Profile::Serena,
        "tools/call",
        json!({"name":"get_current_config","arguments":{}}),
    );
    assert_eq!(cfg["result"]["isError"], false);
    assert_eq!(snapshots(&s), 0);
    let before = stored(&s);
    let denied = message(
        &mut c,
        &mut s,
        Profile::Serena,
        "tools/call",
        json!({"name":"read_file","arguments":{"relative_path":"main.rs"}}),
    );
    assert_eq!(denied["result"]["isError"], true);
    assert!(
        denied
            .to_string()
            .contains("Tool excluded by session steering")
    );
    assert_eq!(stored(&s), before);
    assert_eq!(snapshots(&s), 0);
    let memory = message(
        &mut c,
        &mut s,
        Profile::Serena,
        "tools/call",
        json!({"name":"list_memories","arguments":{}}),
    );
    assert_eq!(memory["result"]["isError"], false);
    assert!(snapshots(&s) > 0);
}

#[test]
fn every_session_profile_and_original_query_alias_identity_obey_the_same_base() {
    let r = root();
    let mut s = open(
        &r,
        host(
            json!({"steering":{"global":{"fixedTools":["ci_status","get_current_config","graft_find_code","graph_stats"]}}}),
        ),
        "profiles",
    );
    for (p, expected) in [
        (Profile::Session, "ci_status"),
        (Profile::Serena, "get_current_config"),
        (Profile::Trail, "trail_find_code"),
        (Profile::Graft, "graft_find_code"),
        (Profile::Graphify, "graph_stats"),
    ] {
        let mut c = ConnectionState::default();
        message(&mut c, &mut s, p, "initialize", json!({}));
        let list = message(&mut c, &mut s, p, "tools/list", json!({}));
        assert_eq!(names(&list), BTreeSet::from([expected]));
    }
    assert!(s.tool_allowed("graft_ask") && s.tool_allowed("trail_ask"));
    assert!(!s.tool_allowed("graft_find_all") && !s.tool_allowed("trail_find_all"));
    let value = yoke::trail_adapter::invoke(
        &mut s,
        "graft_ask",
        &json!({"query":"source"}),
        &Deadline::new(30000),
    )
    .unwrap();
    assert!(!value.is_null());
    assert!(
        yoke::trail_adapter::invoke(
            &mut s,
            "trail_find_all",
            &json!({"query":"source"}),
            &Deadline::new(30000)
        )
        .unwrap_err()
        .to_string()
        .contains("Tool excluded")
    );
    assert!(
        yoke::graphify_adapter::invoke(&mut s, "query_graph", &json!({}), &Deadline::new(30000))
            .unwrap_err()
            .to_string()
            .contains("Tool excluded")
    );
}

#[test]
fn six_layer_base_precedence_provenance_and_cumulative_other_layer_exclusions() {
    let r = root();
    let mut h = json!({"context":"custom","modes":["selected"],"steering":{"global":{"fixedTools":["ci_status"]},"projects":{"default":{"project":{"fixedTools":["ci_find"]},"projectLocal":{"fixedTools":["ci_read"]}}},"contexts":{"custom":{"fixedTools":["ci_context"]}},"modes":{"selected":{"fixedTools":["list_memories"]}},"cli":{"fixedTools":["get_current_config"]}}});
    for (i, source, name) in [
        (0, "cli", "get_current_config"),
        (1, "mode:selected", "list_memories"),
        (2, "context:custom", "ci_context"),
        (3, "project-local", "ci_read"),
        (4, "project", "ci_find"),
        (5, "global", "ci_status"),
    ] {
        let mut s = open(&r, host(h.clone()), &format!("layer{i}"));
        let v = call(&mut s, "configuration", json!({}));
        assert_eq!(v["steering"]["fixedToolsSource"], source);
        assert_eq!(v["steering"]["fixedTools"], json!([name]));
        assert!(s.tool_allowed(name));
        match i {
            0 => h["steering"]["cli"]["fixedTools"] = json!([]),
            1 => h["steering"]["modes"]["selected"]["fixedTools"] = json!([]),
            2 => h["steering"]["contexts"]["custom"]["fixedTools"] = json!([]),
            3 => h["steering"]["projects"]["default"]["projectLocal"]["fixedTools"] = json!([]),
            4 => h["steering"]["projects"]["default"]["project"]["fixedTools"] = json!([]),
            _ => {}
        }
    }
    let mut s = open(
        &r,
        host(
            json!({"steering":{"global":{"fixedTools":["get_current_config","read_file"]},"cli":{"excludedTools":["read_file"]}}}),
        ),
        "excluded",
    );
    assert!(s.tool_allowed("get_current_config") && !s.tool_allowed("read_file"));
    assert_eq!(
        call(&mut s, "configuration", json!({}))["steering"]["fixedToolsSource"],
        "global"
    );
}

#[test]
fn invalid_conflicting_duplicate_and_forged_fixed_settings_refuse_before_publication() {
    for steering in [
        json!({"global":{"fixedTools":["not_a_tool"]}}),
        json!({"global":{"fixedTools":["*"]}}),
        json!({"global":{"fixedTools":["../read_file"]}}),
        json!({"global":{"fixedTools":["get_current_cоnfig"]}}),
        json!({"global":{"fixedTools":["get_current_config","get_current_config"]}}),
        json!({"global":{"fixedTools":["get_current_config"],"excludedTools":["read_file"]}}),
        json!({"modes":{"unused":{"fixedTools":["get_current_config"],"excludedTools":["read_file"]}}}),
        json!({"global":{"fixedToolsSource":"forged","fixedTools":["get_current_config"]}}),
        json!({"global":{"fixedTools":["get_current_config".repeat(20000)]}}),
    ] {
        let r = root();
        let parsed = serde_json::from_value::<HostProfile>(json!({"steering":steering}));
        if let Ok(h) = parsed {
            assert!(
                Session::open(
                    Engine::open(r.path()).unwrap(),
                    h,
                    Some("invalid"),
                    &Deadline::new(30000)
                )
                .is_err()
            );
            let e = Engine::open(r.path()).unwrap();
            let count: u64 = e
                .store
                .db
                .query_row(
                    "SELECT count(*) FROM records WHERE kind=?1",
                    [yoke::project_session::RECORD_KIND],
                    |r| r.get(0),
                )
                .unwrap();
            assert_eq!(count, 0);
        }
    }
    let r = root();
    let mut s = open(
        &r,
        host(json!({"steering":{"global":{"fixedTools":["get_current_config"]}}})),
        "caller",
    );
    let before = stored(&s);
    assert!(
        s.invoke(
            "switchModes",
            &json!({"modes":["editing"],"fixedTools":["read_file"]}),
            &Deadline::new(30000)
        )
        .is_err()
    );
    assert_eq!(stored(&s), before);
}

#[test]
fn real_mode_catalog_notifications_and_stale_parallel_sessions_refuse_without_empty_success() {
    let r = root();
    let h = host(
        json!({"steering":{"global":{"fixedTools":["ci_session","ci_status"]},"modes":{"tiny":{"fixedTools":["ci_session"]}}}}),
    );
    let mut s = open(&r, h.clone(), "live");
    let mut c = ConnectionState::default();
    message(&mut c, &mut s, Profile::Session, "initialize", json!({}));
    let before = message(&mut c, &mut s, Profile::Session, "tools/list", json!({}));
    assert_eq!(names(&before), BTreeSet::from(["ci_session", "ci_status"]));
    let switched = message(
        &mut c,
        &mut s,
        Profile::Session,
        "tools/call",
        json!({"name":"ci_session","arguments":{"operation":"switchModes","args":{"modes":["tiny"]}}}),
    );
    assert_eq!(switched["result"]["isError"], false);
    let notifications = c.take_notifications();
    assert_eq!(notifications.len(), 1);
    assert_eq!(
        notifications[0]["method"],
        "notifications/tools/list_changed"
    );
    assert_eq!(
        names(&message(
            &mut c,
            &mut s,
            Profile::Session,
            "tools/list",
            json!({})
        )),
        BTreeSet::from(["ci_session"])
    );
    let mut peer = open(&r, h.clone(), "live");
    call(&mut peer, "switchModes", json!({"modes":["editing"]}));
    let changed = stored(&peer);
    for (method, params) in [
        ("tools/list", json!({})),
        (
            "tools/call",
            json!({"name":"ci_session","arguments":{"operation":"configuration","args":{}}}),
        ),
    ] {
        let v = message(&mut c, &mut s, Profile::Session, method, params);
        assert_eq!(v["error"]["code"], -32001);
        assert!(
            v["error"]["message"]
                .as_str()
                .unwrap()
                .contains("concurrently")
        );
        assert!(v.get("result").is_none());
        assert_eq!(stored(&s), changed);
    }
    assert!(
        yoke::compatibility::invoke_serena(
            &mut s,
            "get_current_config",
            &json!({}),
            &Deadline::new(30000)
        )
        .unwrap_err()
        .to_string()
        .contains("concurrently")
    );
    assert!(
        s.invoke("configuration", &json!({}), &Deadline::new(30000))
            .unwrap_err()
            .to_string()
            .contains("concurrently")
    );
    assert_eq!(snapshots(&s), 0);
    let mut resumed = open(&r, h, "live");
    let mut fresh = ConnectionState::default();
    message(
        &mut fresh,
        &mut resumed,
        Profile::Session,
        "initialize",
        json!({}),
    );
    assert_eq!(
        names(&message(
            &mut fresh,
            &mut resumed,
            Profile::Session,
            "tools/list",
            json!({})
        )),
        BTreeSet::from(["ci_session", "ci_status"])
    );
}

#[test]
fn fixed_base_cannot_override_context_startup_or_source_write_authority() {
    let r = root();
    let mut locked = open(
        &r,
        host(
            json!({"context":"claude-code","startupProject":"default","steering":{"global":{"fixedTools":["activate_project","read_file","get_current_config"]}}}),
        ),
        "locked",
    );
    assert!(
        !locked.tool_allowed("activate_project")
            && !locked.tool_allowed("read_file")
            && locked.tool_allowed("get_current_config")
    );
    assert!(
        yoke::compatibility::invoke_serena(
            &mut locked,
            "activate_project",
            &json!({"project":"default"}),
            &Deadline::new(30000)
        )
        .is_err()
    );
    let mut s = open(
        &r,
        host(json!({"steering":{"global":{"fixedTools":["create_text_file"]}}})),
        "no-write",
    );
    assert!(s.tool_allowed("create_text_file"));
    let planned = yoke::compatibility::invoke_serena(
        &mut s,
        "create_text_file",
        &json!({"relative_path":"forbidden.rs","content":"fn forbidden() {}"}),
        &Deadline::new(30000),
    )
    .unwrap();
    assert!(planned["plan"].is_object());
    assert!(!r.path().join("forbidden.rs").exists());
    let target = root();
    let mut read_only = open(
        &r,
        host(
            json!({"projects":[{"name":"target","root":target.path(),"readOnly":true}],"startupProject":"target","steering":{"global":{"fixedTools":["create_text_file"]}}}),
        ),
        "read-only",
    );
    assert!(
        yoke::compatibility::invoke_serena(
            &mut read_only,
            "create_text_file",
            &json!({"relative_path":"forbidden.rs","content":"fn forbidden() {}"}),
            &Deadline::new(30000)
        )
        .is_err()
    );
    assert!(!target.path().join("forbidden.rs").exists());
    assert_eq!(
        fs::read(r.path().join("main.rs")).unwrap(),
        "// β\nfn source() {}\n".as_bytes()
    );
}

#[test]
fn empty_fixed_fields_preserve_serialized_binding_configuration_and_legacy_catalog() {
    let r = root();
    let absent = host(json!({"steering":{}}));
    let empty = host(json!({"steering":{"global":{"fixedTools":[]},"cli":{"fixedTools":[]}}}));
    assert_eq!(
        serde_json::to_vec(&absent).unwrap(),
        serde_json::to_vec(&empty).unwrap()
    );
    let mut s = open(&r, absent, "legacy");
    let before = stored(&s);
    let cfg = call(&mut s, "configuration", json!({}));
    assert!(
        cfg["steering"].get("fixedTools").is_none()
            && cfg["steering"].get("fixedToolsSource").is_none()
    );
    drop(s);
    let mut resumed = open(&r, empty, "legacy");
    assert_eq!(stored(&resumed), before);
    let mut c = ConnectionState::default();
    message(
        &mut c,
        &mut resumed,
        Profile::Serena,
        "initialize",
        json!({}),
    );
    assert_eq!(
        message(
            &mut c,
            &mut resumed,
            Profile::Serena,
            "tools/list",
            json!({})
        )["result"]["tools"],
        yoke::compatibility::serena_tools()
    );
}

#[test]
fn mcp_tool_base_does_not_grant_or_replace_independent_python_api_selection() {
    use yoke::agent_repl::{AgentRepl, ReplSpec};
    let r = root();
    let mut s = open(
        &r,
        host(json!({"steering":{"global":{"fixedTools":["get_current_config"]}}})),
        "api",
    );
    assert!(!s.tool_allowed("list_memories"));
    let output = std::process::Command::new("python")
        .args(["-I", "-c", "import sys;print(sys.executable)"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let spec = ReplSpec {
        python: std::path::PathBuf::from(String::from_utf8(output.stdout).unwrap().trim()),
        trust_python: true,
        max_output_bytes: 100000,
        shell: None,
    };
    let mut worker = AgentRepl::spawn(spec.clone(), &Deadline::new(30000)).unwrap();
    assert_eq!(
        worker
            .execute(
                &mut s,
                "len(s.mem.list_memories()['items'])",
                &Deadline::new(30000)
            )
            .unwrap()["result"],
        0
    );
    worker.close().unwrap();
    let mut limited = open(
        &r,
        host(
            json!({"steering":{"global":{"fixedTools":["get_current_config"],"excludedApis":["mem"]}}}),
        ),
        "limited-api",
    );
    assert!(limited.tool_allowed("get_current_config"));
    let mut worker = AgentRepl::spawn(spec, &Deadline::new(30000)).unwrap();
    assert_eq!(
        worker
            .execute(&mut limited, "hasattr(s,'mem')", &Deadline::new(30000))
            .unwrap()["result"],
        false
    );
    worker.close().unwrap();
}
