use serde_json::{Value, json};
use std::{collections::BTreeSet, fs};
use tempfile::TempDir;
use yoke::{
    Engine,
    agent_repl::{AgentRepl, ReplSpec},
    capture::Deadline,
    project_session::{HostProfile, Session},
    protocol::{ConnectionState, Profile},
};
fn root() -> TempDir {
    let r = TempDir::new().unwrap();
    fs::write(
        r.path().join("main.rs"),
        "// owned interface source\nfn source() {}\n",
    )
    .unwrap();
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
fn runtime_message(
    c: &mut ConnectionState,
    s: &mut Session,
    r: &mut AgentRepl,
    method: &str,
    params: Value,
) -> Value {
    c.message_serena_with_repl(
        s,
        r,
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
fn payload(v: &Value) -> Value {
    serde_json::from_str(v["result"]["content"][0]["text"].as_str().unwrap()).unwrap()
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
fn spec() -> ReplSpec {
    let out = std::process::Command::new("python")
        .args(["-I", "-c", "import sys;print(sys.executable)"])
        .output()
        .unwrap();
    assert!(out.status.success());
    ReplSpec {
        python: std::path::PathBuf::from(String::from_utf8(out.stdout).unwrap().trim()),
        trust_python: true,
        max_output_bytes: 100000,
        shell: None,
    }
}

#[test]
fn tools_and_repl_views_are_exact_original_subsets_and_repl_selection_grants_no_runtime() {
    let r = root();
    let original = yoke::compatibility::serena_tools();
    let controls = BTreeSet::from([
        "activate_project",
        "remove_project",
        "query_project",
        "serena_repl",
        "initial_instructions",
        "serena_info",
    ]);
    for (label, interface, count) in [("tools", "tools", 52), ("repl", "repl", 6)] {
        let mut s = open(
            &r,
            host(json!({"context":"desktop-app","agentInterface":interface})),
            label,
        );
        let mut c = ConnectionState::default();
        message(&mut c, &mut s, Profile::Serena, "initialize", json!({}));
        let list = message(&mut c, &mut s, Profile::Serena, "tools/list", json!({}));
        assert_eq!(names(&list).len(), count);
        let expected: Vec<_> = original
            .as_array()
            .unwrap()
            .iter()
            .filter(|t| {
                if interface == "tools" {
                    t["name"] != "serena_repl"
                } else {
                    controls.contains(t["name"].as_str().unwrap())
                }
            })
            .cloned()
            .collect();
        assert_eq!(list["result"]["tools"], json!(expected));
        let cfg = call(&mut s, "configuration", json!({}));
        assert_eq!(cfg["steering"]["agentInterface"], interface);
        assert_eq!(
            cfg["steering"]["agentInterfaceSource"],
            "host.agentInterface"
        );
        if interface == "repl" {
            let info = message(
                &mut c,
                &mut s,
                Profile::Serena,
                "tools/call",
                json!({"name":"serena_info","arguments":{"topic":"jet_brains_debug_repl"}}),
            );
            assert_eq!(info["result"]["isError"], false);
        }
        let marker = r.path().join("must-not-execute.txt");
        let code = format!(
            "open({},'w').write('unexpected')",
            serde_json::to_string(marker.to_str().unwrap()).unwrap()
        );
        let denied = message(
            &mut c,
            &mut s,
            Profile::Serena,
            "tools/call",
            json!({"name":"serena_repl","arguments":{"session_id":label,"code":code}}),
        );
        assert_eq!(denied["result"]["isError"], true);
        assert!(!marker.exists());
    }
}

#[test]
fn repl_control_set_ignores_tool_steering_but_preserves_fixed_startup_admission() {
    let r = root();
    let mut s = open(
        &r,
        host(
            json!({"context":"desktop-app","agentInterface":"repl","steering":{"global":{"fixedTools":["read_file"]},"cli":{"excludedTools":["serena_repl","activate_project","serena_info"]}}}),
        ),
        "repl",
    );
    assert!(!s.tool_allowed("serena_repl"));
    assert!(
        s.serena_tool_allowed("serena_repl")
            && s.serena_tool_allowed("activate_project")
            && s.serena_tool_allowed("serena_info")
    );
    assert!(!s.serena_tool_allowed("read_file"));
    assert!(
        yoke::compatibility::invoke_serena(
            &mut s,
            "serena_info",
            &json!({"topic":"jet_brains_debug_repl"}),
            &Deadline::new(30000)
        )
        .is_ok()
    );
    assert!(
        yoke::compatibility::invoke_serena(
            &mut s,
            "read_file",
            &json!({"relative_path":"main.rs"}),
            &Deadline::new(30000)
        )
        .is_err()
    );
    let mut locked = open(
        &r,
        host(
            json!({"context":"claude-code","startupProject":"default","agentInterface":"repl","steering":{"cli":{"excludedTools":["serena_repl"]}}}),
        ),
        "locked",
    );
    let mut c = ConnectionState::default();
    message(
        &mut c,
        &mut locked,
        Profile::Serena,
        "initialize",
        json!({}),
    );
    let list = message(
        &mut c,
        &mut locked,
        Profile::Serena,
        "tools/list",
        json!({}),
    );
    assert_eq!(names(&list).len(), 5);
    assert!(!names(&list).contains("activate_project") && names(&list).contains("serena_repl"));
    assert!(
        locked
            .invoke(
                "activate",
                &json!({"project":"default"}),
                &Deadline::new(30000)
            )
            .is_err()
    );
}

#[test]
fn startup_project_and_local_override_cli_but_runtime_activation_and_modes_keep_interface_fixed() {
    let r = root();
    let first = root();
    let second = root();
    let mut h = json!({"agentInterface":"tools","startupProject":"first","projects":[{"name":"first","root":first.path()},{"name":"second","root":second.path()}],"steering":{"global":{"agentInterface":"repl"},"cli":{"agentInterface":"tools"},"projects":{"first":{"project":{"agentInterface":"repl"},"projectLocal":{"agentInterface":"tools"}},"second":{"project":{"agentInterface":"repl"}}}}});
    let mut s = open(&r, host(h.clone()), "pinned");
    let initial = call(&mut s, "configuration", json!({}));
    assert_eq!(initial["steering"]["agentInterface"], "tools");
    assert_eq!(initial["steering"]["agentInterfaceSource"], "project-local");
    assert_eq!(initial["steering"]["agentInterfaceProject"], "first");
    call(&mut s, "activate", json!({"project":"second"}));
    let changed = call(&mut s, "switchModes", json!({"modes":["planning"]}));
    assert_eq!(changed["activeProject"], "second");
    assert_eq!(changed["steering"]["agentInterface"], "tools");
    assert_eq!(changed["steering"]["agentInterfaceProject"], "first");
    drop(s);
    let mut resumed = open(&r, host(h.clone()), "pinned");
    assert_eq!(
        call(&mut resumed, "configuration", json!({}))["steering"]["agentInterface"],
        "tools"
    );
    for (i, value, source) in [
        (0, "tools", "project-local"),
        (1, "repl", "project"),
        (2, "tools", "cli"),
        (3, "repl", "global"),
        (4, "tools", "host.agentInterface"),
    ] {
        let mut s = open(&r, host(h.clone()), &format!("precedence{i}"));
        let c = call(&mut s, "configuration", json!({}));
        assert_eq!(c["steering"]["agentInterface"], value);
        assert_eq!(c["steering"]["agentInterfaceSource"], source);
        match i {
            0 => h["steering"]["projects"]["first"]["projectLocal"]["agentInterface"] = Value::Null,
            1 => h["steering"]["projects"]["first"]["project"]["agentInterface"] = Value::Null,
            2 => h["steering"]["cli"]["agentInterface"] = Value::Null,
            3 => h["steering"]["global"]["agentInterface"] = Value::Null,
            _ => {}
        }
    }
    h["startupProject"] = json!("second");
    let mut s = open(&r, host(h), "second-start");
    assert_eq!(
        call(&mut s, "configuration", json!({}))["steering"]["agentInterface"],
        "repl"
    );
}

#[test]
fn unknown_values_context_mode_settings_and_caller_interface_forgery_refuse() {
    for v in [
        json!({"agentInterface":"auto"}),
        json!({"agentInterface":"REPL"}),
        json!({"agentInterface":"rеpl"}),
        json!({"agentInterface":true}),
        json!({"steering":{"global":{"agentInterface":[]}}}),
        json!({"steering":{"contexts":{"custom":{"agentInterface":"tools"}}}}),
        json!({"steering":{"modes":{"custom":{"agentInterface":"repl"}}}}),
    ] {
        assert!(serde_json::from_value::<HostProfile>(v).is_err());
    }
    let r = root();
    let mut s = open(&r, host(json!({"agentInterface":"tools"})), "forgery");
    let before = stored(&s);
    assert!(
        s.invoke(
            "switchModes",
            &json!({"modes":["editing"],"agentInterface":"repl"}),
            &Deadline::new(30000)
        )
        .is_err()
    );
    assert_eq!(stored(&s), before);
    assert!(
        s.invoke(
            "configuration",
            &json!({"agentInterface":"repl"}),
            &Deadline::new(30000)
        )
        .is_err()
    );
    assert_eq!(stored(&s), before);
}

#[test]
fn none_null_and_empty_interface_settings_preserve_legacy_profile_binding_and_all53_schemas() {
    let r = root();
    let absent = host(json!({"context":"desktop-app","steering":{}}));
    let explicit = host(
        json!({"context":"desktop-app","agentInterface":null,"steering":{"global":{"agentInterface":null},"cli":{"agentInterface":null}}}),
    );
    assert_eq!(
        serde_json::to_vec(&absent).unwrap(),
        serde_json::to_vec(&explicit).unwrap()
    );
    let mut s = open(&r, absent, "legacy");
    let before = stored(&s);
    assert!(
        call(&mut s, "configuration", json!({}))["steering"]
            .get("agentInterface")
            .is_none()
    );
    drop(s);
    let mut resumed = open(&r, explicit, "legacy");
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
    let h = host(json!({"context":"desktop-app","agentInterface":"tools","steering":{}}));
    assert!(
        Session::open(
            Engine::open(r.path()).unwrap(),
            h,
            Some("legacy"),
            &Deadline::new(30000)
        )
        .err()
        .unwrap()
        .to_string()
        .contains("host binding changed")
    );
}

#[test]
fn real_mcp_repl_requires_explicit_host_runtime_and_keeps_api_selection_independent() {
    let r = root();
    let mut s = open(
        &r,
        host(
            json!({"agentInterface":"repl","steering":{"global":{"fixedTools":["read_file"],"excludedApis":["mem"]},"cli":{"excludedTools":["serena_repl"]}}}),
        ),
        "runtime",
    );
    let mut worker = AgentRepl::spawn(spec(), &Deadline::new(30000)).unwrap();
    let mut c = ConnectionState::default();
    runtime_message(&mut c, &mut s, &mut worker, "initialize", json!({}));
    let list = runtime_message(&mut c, &mut s, &mut worker, "tools/list", json!({}));
    assert_eq!(names(&list).len(), 6);
    let result = runtime_message(
        &mut c,
        &mut s,
        &mut worker,
        "tools/call",
        json!({"name":"serena_repl","arguments":{"session_id":"runtime","code":"{'answer':40+2,'hasMemory':hasattr(s,'mem'),'text':s.fs.read_file('main.rs')['text']}"}}),
    );
    assert_eq!(result["result"]["isError"], false, "{result}");
    let data = payload(&result);
    assert_eq!(data["data"]["result"]["answer"], 42);
    assert_eq!(data["data"]["result"]["hasMemory"], false);
    assert_eq!(
        data["data"]["result"]["text"],
        "// owned interface source\nfn source() {}\n"
    );
    let denied = runtime_message(
        &mut c,
        &mut s,
        &mut worker,
        "tools/call",
        json!({"name":"read_file","arguments":{"relative_path":"main.rs"}}),
    );
    assert_eq!(denied["result"]["isError"], true);
    worker.close().unwrap();
    let mut untrusted = spec();
    untrusted.trust_python = false;
    assert!(AgentRepl::spawn(untrusted, &Deadline::new(30000)).is_err());
    assert_eq!(
        fs::read(r.path().join("main.rs")).unwrap(),
        b"// owned interface source\nfn source() {}\n"
    );
}

#[test]
fn opted_interface_sessions_reject_external_stale_catalogs_adapters_and_cells_before_execution() {
    let r = root();
    let h = host(json!({"agentInterface":"repl"}));
    let mut s = open(&r, h.clone(), "live");
    let mut c = ConnectionState::default();
    message(&mut c, &mut s, Profile::Serena, "initialize", json!({}));
    let selected = spec();
    let mut worker = AgentRepl::spawn(selected.clone(), &Deadline::new(30000)).unwrap();
    worker
        .execute(&mut s, "answer=42", &Deadline::new(30000))
        .unwrap();
    let mut peer = open(&r, h.clone(), "live");
    call(&mut peer, "switchModes", json!({"modes":["planning"]}));
    let state = stored(&peer);
    let stale = message(&mut c, &mut s, Profile::Serena, "tools/list", json!({}));
    assert_eq!(stale["error"]["code"], -32001);
    assert!(stale.get("result").is_none());
    assert!(
        yoke::compatibility::invoke_serena(
            &mut s,
            "serena_info",
            &json!({"topic":"jet_brains_debug_repl"}),
            &Deadline::new(30000)
        )
        .unwrap_err()
        .to_string()
        .contains("concurrently")
    );
    let output = TempDir::new().unwrap();
    let marker = output.path().join("must-not-run.txt");
    let code = format!(
        "open({},'w').write('unexpected')",
        serde_json::to_string(marker.to_str().unwrap()).unwrap()
    );
    assert!(
        worker
            .execute(&mut s, &code, &Deadline::new(30000))
            .unwrap_err()
            .to_string()
            .contains("concurrently")
    );
    assert!(!marker.exists() && !worker.is_alive());
    worker.close().unwrap();
    assert_eq!(stored(&s), state);
    let mut resumed = open(&r, h, "live");
    let mut fresh = AgentRepl::spawn(selected, &Deadline::new(30000)).unwrap();
    assert_eq!(
        fresh
            .execute(&mut resumed, "40+2", &Deadline::new(30000))
            .unwrap()["result"],
        42
    );
    fresh.close().unwrap();
}

#[test]
fn other_session_profiles_and_independent_native8_keep_their_original_schema_sets() {
    let r = root();
    let mut ordinary = open(
        &r,
        host(json!({"context":"desktop-app","steering":{}})),
        "ordinary",
    );
    let mut repl = open(
        &r,
        host(json!({"context":"desktop-app","agentInterface":"repl","steering":{}})),
        "repl",
    );
    for p in [
        Profile::Native,
        Profile::Session,
        Profile::Graphify,
        Profile::Trail,
        Profile::Graft,
    ] {
        let mut a = ConnectionState::default();
        let mut b = ConnectionState::default();
        message(&mut a, &mut ordinary, p, "initialize", json!({}));
        message(&mut b, &mut repl, p, "initialize", json!({}));
        let old = message(&mut a, &mut ordinary, p, "tools/list", json!({}));
        let new = message(&mut b, &mut repl, p, "tools/list", json!({}));
        assert_eq!(old["result"]["tools"], new["result"]["tools"]);
        if matches!(p, Profile::Native) {
            assert_eq!(names(&new).len(), 8);
        }
    }
}
