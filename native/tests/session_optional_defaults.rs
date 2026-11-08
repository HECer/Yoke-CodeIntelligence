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
        "// owned defaults source\nfn source() {}\n",
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
fn versioned_defaults_filter_original_schemas_and_require_explicit_optional_tool_selection() {
    let r = root();
    let original = yoke::compatibility::serena_tools();
    for (label, interface, count) in [
        ("classic", Value::Null, 29),
        ("tools", json!("tools"), 29),
        ("repl", json!("repl"), 6),
    ] {
        let mut s = open(
            &r,
            host(
                json!({"context":"desktop-app","optionalDefaults":"serena_optional_v1","agentInterface":interface}),
            ),
            label,
        );
        let mut c = ConnectionState::default();
        message(&mut c, &mut s, Profile::Serena, "initialize", json!({}));
        let list = message(&mut c, &mut s, Profile::Serena, "tools/list", json!({}));
        assert_eq!(names(&list).len(), count);
        for tool in list["result"]["tools"].as_array().unwrap() {
            assert!(original.as_array().unwrap().contains(tool));
        }
        let cfg = call(&mut s, "configuration", json!({}));
        assert_eq!(cfg["steering"]["optionalDefaults"], "serena_optional_v1");
        assert_eq!(s.selected_repl_methods().unwrap().unwrap().len(), 42);
        if interface != json!("repl") {
            for name in [
                "serena_info",
                "remove_project",
                "get_diagnostics_for_symbol",
                "jet_brains_find_symbol",
                "serena_repl",
            ] {
                assert!(!names(&list).contains(name));
            }
            let denied = message(
                &mut c,
                &mut s,
                Profile::Serena,
                "tools/call",
                json!({"name":"serena_info","arguments":{"topic":"jet_brains_debug_repl"}}),
            );
            assert_eq!(denied["result"]["isError"], true);
            assert!(
                yoke::compatibility::invoke_serena(
                    &mut s,
                    "serena_info",
                    &json!({"topic":"jet_brains_debug_repl"}),
                    &Deadline::new(30000)
                )
                .is_err()
            );
        } else {
            assert!(names(&list).contains("serena_info") && names(&list).contains("serena_repl"));
        }
        assert!(s.require_repl_api("fs.read_file").is_ok());
        assert!(
            s.require_repl_api("lsp.get_diagnostics_for_symbol")
                .is_err()
        );
        assert!(s.require_repl_api("ext.list_projects").is_err());
    }
}

#[test]
fn inclusions_and_fixed_bases_enable_optional_entries_without_changing_other_defaults() {
    let r = root();
    let mut s = open(
        &r,
        host(
            json!({"optionalDefaults":"serena_optional_v1","agentInterface":"tools","steering":{"modes":{"grant":{"includedOptionalTools":["serena_info","serena_repl"],"includedApis":["lsp.get_diagnostics_for_symbol"]},"deny":{"excludedTools":["serena_info"],"excludedApis":["lsp"]}}}}),
        ),
        "layers",
    );
    assert!(!s.serena_tool_allowed("serena_info") && !s.serena_tool_allowed("serena_repl"));
    call(&mut s, "switchModes", json!({"modes":["grant"]}));
    assert!(s.serena_tool_allowed("serena_info") && s.serena_tool_allowed("serena_repl"));
    assert!(s.require_repl_api("lsp.get_diagnostics_for_symbol").is_ok());
    assert!(s.require_repl_api("lsp.restart_language_server").is_err());
    call(&mut s, "switchModes", json!({"modes":["grant","deny"]}));
    assert!(!s.serena_tool_allowed("serena_info"));
    assert!(
        s.require_repl_api("lsp.get_diagnostics_for_symbol")
            .is_err()
    );
    let mut fixed = open(
        &r,
        host(
            json!({"optionalDefaults":"serena_optional_v1","steering":{"global":{"fixedTools":["serena_info"]}}}),
        ),
        "fixed",
    );
    assert!(fixed.serena_tool_allowed("serena_info") && !fixed.serena_tool_allowed("read_file"));
    assert!(
        yoke::compatibility::invoke_serena(
            &mut fixed,
            "serena_info",
            &json!({"topic":"jet_brains_debug_repl"}),
            &Deadline::new(30000)
        )
        .is_ok()
    );
}

#[test]
fn explicit_repl_controls_ignore_tool_rules_while_optional_api_defaults_still_apply() {
    let r = root();
    let mut s = open(
        &r,
        host(
            json!({"optionalDefaults":"serena_optional_v1","agentInterface":"repl","context":"desktop-app","steering":{"global":{"fixedTools":["read_file"]},"cli":{"excludedTools":["serena_info","serena_repl"]}}}),
        ),
        "controls",
    );
    assert!(
        s.serena_tool_allowed("serena_info")
            && s.serena_tool_allowed("serena_repl")
            && !s.serena_tool_allowed("read_file")
    );
    assert_eq!(s.selected_repl_methods().unwrap().unwrap().len(), 42);
    let mut c = ConnectionState::default();
    message(&mut c, &mut s, Profile::Serena, "initialize", json!({}));
    assert_eq!(
        names(&message(
            &mut c,
            &mut s,
            Profile::Serena,
            "tools/list",
            json!({})
        ))
        .len(),
        6
    );
    let no_runtime = message(
        &mut c,
        &mut s,
        Profile::Serena,
        "tools/call",
        json!({"name":"serena_repl","arguments":{"session_id":"controls","code":"40+2"}}),
    );
    assert_eq!(no_runtime["result"]["isError"], true);
    let mut locked = open(
        &r,
        host(
            json!({"optionalDefaults":"serena_optional_v1","agentInterface":"repl","context":"claude-code","startupProject":"default","steering":{"cli":{"includedOptionalTools":["activate_project"]}}}),
        ),
        "locked",
    );
    assert!(!locked.serena_tool_allowed("activate_project"));
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
fn real_worker_moves_from_legacy50_to_selected42_and_cached_optional_calls_follow_modes() {
    let r = root();
    let reference = root();
    let projects = json!([{"name":"reference","root":reference.path(),"queryOnly":true}]);
    let mut legacy = open(&r, host(json!({"projects":projects})), "legacy");
    let mut selected = open(
        &r,
        host(
            json!({"projects":projects,"optionalDefaults":"serena_optional_v1","steering":{"modes":{"grant":{"includedApis":["ext.list_projects"]},"deny":{"excludedApis":["ext"]}}}}),
        ),
        "selected",
    );
    let mut worker = AgentRepl::spawn(spec(), &Deadline::new(30000)).unwrap();
    let v = worker
        .execute(
            &mut legacy,
            "answer=42\nsaved_optional=s.ext.list_projects\ncallable(saved_optional)",
            &Deadline::new(30000),
        )
        .unwrap();
    assert_eq!(v["result"], true);
    let methods = yoke::agent_repl::facade_contract()["methods"]
        .as_object()
        .unwrap()
        .keys()
        .map(|m| m.split('.').map(str::to_owned).collect::<Vec<_>>())
        .collect::<Vec<_>>();
    let pairs = serde_json::to_string(&methods).unwrap();
    let code = format!(
        "{{'answer':answer,'visibleCount':sum(hasattr(getattr(s,g,None),m) for g,m in {pairs}),'hasExt':hasattr(s,'ext'),'text':s.fs.read_file('main.rs')['text']}}"
    );
    let v = worker
        .execute(&mut selected, &code, &Deadline::new(30000))
        .unwrap();
    assert!(v["error"].is_null(), "{v}");
    assert_eq!(
        v["result"],
        json!({"answer":42,"visibleCount":42,"hasExt":false,"text":"// owned defaults source\nfn source() {}\n"})
    );
    let v = worker
        .execute(&mut selected, "saved_optional()", &Deadline::new(30000))
        .unwrap();
    assert!(!v["error"].is_null() && worker.is_alive());
    call(&mut selected, "switchModes", json!({"modes":["grant"]}));
    let v = worker
        .execute(&mut selected, "saved_optional()", &Deadline::new(30000))
        .unwrap();
    assert!(v["error"].is_null(), "{v}");
    assert!(v["result"].to_string().contains("reference"));
    assert_eq!(selected.selected_repl_methods().unwrap().unwrap().len(), 43);
    call(
        &mut selected,
        "switchModes",
        json!({"modes":["grant","deny"]}),
    );
    assert!(
        !worker
            .execute(&mut selected, "saved_optional()", &Deadline::new(30000))
            .unwrap()["error"]
            .is_null()
    );
    assert_eq!(
        worker
            .execute(&mut selected, "answer", &Deadline::new(30000))
            .unwrap()["result"],
        42
    );
    worker.close().unwrap();
}

#[test]
fn absent_and_null_preserve_legacy_bindings_and_other_profiles_keep_all_original_schemas() {
    let r = root();
    let old = host(json!({"context":"desktop-app","steering":{}}));
    let null = host(json!({"context":"desktop-app","optionalDefaults":null,"steering":{}}));
    assert_eq!(
        serde_json::to_vec(&old).unwrap(),
        serde_json::to_vec(&null).unwrap()
    );
    let s = open(&r, old.clone(), "legacy");
    let before = stored(&s);
    drop(s);
    let mut ordinary = open(&r, null, "legacy");
    assert_eq!(stored(&ordinary), before);
    assert!(
        call(&mut ordinary, "configuration", json!({}))["steering"]
            .get("optionalDefaults")
            .is_none()
    );
    assert!(ordinary.selected_repl_methods().unwrap().is_none());
    let h = host(
        json!({"context":"desktop-app","optionalDefaults":"serena_optional_v1","steering":{}}),
    );
    assert!(
        Session::open(
            Engine::open(r.path()).unwrap(),
            h.clone(),
            Some("legacy"),
            &Deadline::new(30000)
        )
        .err()
        .unwrap()
        .to_string()
        .contains("host binding changed")
    );
    let mut selected = open(&r, h.clone(), "selected");
    for p in [
        Profile::Native,
        Profile::Session,
        Profile::Trail,
        Profile::Graft,
        Profile::Graphify,
    ] {
        let mut a = ConnectionState::default();
        let mut b = ConnectionState::default();
        message(&mut a, &mut ordinary, p, "initialize", json!({}));
        message(&mut b, &mut selected, p, "initialize", json!({}));
        let original = message(&mut a, &mut ordinary, p, "tools/list", json!({}));
        let new = message(&mut b, &mut selected, p, "tools/list", json!({}));
        assert_eq!(new["result"]["tools"], original["result"]["tools"]);
        if matches!(p, Profile::Native) {
            assert_eq!(names(&new).len(), 8);
        }
    }
    drop(selected);
    let mut resumed = open(&r, h, "selected");
    assert_eq!(
        call(&mut resumed, "configuration", json!({}))["steering"]["optionalDefaults"],
        "serena_optional_v1"
    );
    assert!(
        Session::open(
            Engine::open(r.path()).unwrap(),
            old,
            Some("selected"),
            &Deadline::new(30000)
        )
        .is_err()
    );
}

#[test]
fn unknown_type_unicode_and_nested_selectors_refuse_and_callers_cannot_change_the_policy() {
    let r = root();
    for value in [
        json!("unknown"),
        json!("SERENA_OPTIONAL_V1"),
        json!("serena_opt\u{0456}onal_v1"),
        json!(true),
        json!([]),
        json!({}),
    ] {
        assert!(serde_json::from_value::<HostProfile>(json!({"optionalDefaults":value})).is_err());
    }
    for v in [
        json!({"steering":{"global":{"optionalDefaults":"serena_optional_v1"}}}),
        json!({"steering":{"modes":{"custom":{"optionalDefaults":"serena_optional_v1"}}}}),
    ] {
        assert!(serde_json::from_value::<HostProfile>(v).is_err());
    }
    let e = Engine::open(r.path()).unwrap();
    let count: i64 = e
        .store
        .db
        .query_row(
            "SELECT COUNT(*) FROM records WHERE kind=?1",
            [yoke::project_session::RECORD_KIND],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(count, 0);
    drop(e);
    let mut s = open(
        &r,
        host(json!({"optionalDefaults":"serena_optional_v1"})),
        "forgery",
    );
    let before = stored(&s);
    assert!(
        s.invoke(
            "switchModes",
            &json!({"modes":["editing"],"optionalDefaults":null}),
            &Deadline::new(30000)
        )
        .is_err()
    );
    assert_eq!(stored(&s), before);
}

#[test]
fn selected_defaults_without_other_policies_reject_external_stale_catalogs_adapters_and_cells() {
    let r = root();
    let h = host(json!({"optionalDefaults":"serena_optional_v1"}));
    let mut s = open(&r, h.clone(), "live");
    let mut c = ConnectionState::default();
    message(&mut c, &mut s, Profile::Serena, "initialize", json!({}));
    let selected = spec();
    let mut worker = AgentRepl::spawn(selected.clone(), &Deadline::new(30000)).unwrap();
    assert_eq!(
        worker
            .execute(&mut s, "40+2", &Deadline::new(30000))
            .unwrap()["result"],
        42
    );
    let mut peer = open(&r, h.clone(), "live");
    call(&mut peer, "switchModes", json!({"modes":["planning"]}));
    let state = stored(&peer);
    let v = message(&mut c, &mut s, Profile::Serena, "tools/list", json!({}));
    assert_eq!(v["error"]["code"], -32001);
    assert!(v.get("result").is_none());
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
    let out = TempDir::new().unwrap();
    let marker = out.path().join("must-not-run.txt");
    let code = format!(
        "open({},'w').write('unexpected')",
        serde_json::to_string(marker.to_str().unwrap()).unwrap()
    );
    assert!(
        worker
            .execute(&mut s, &code, &Deadline::new(30000))
            .is_err()
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
fn explicitly_included_classic_repl_keeps_real_runtime_api_and_source_permission_gates() {
    let r = root();
    let mut s = open(
        &r,
        host(
            json!({"context":"desktop-app","optionalDefaults":"serena_optional_v1","agentInterface":"tools","steering":{"cli":{"includedOptionalTools":["serena_repl","serena_info"]}}}),
        ),
        "runtime",
    );
    let mut worker = AgentRepl::spawn(spec(), &Deadline::new(30000)).unwrap();
    let mut c = ConnectionState::default();
    runtime_message(&mut c, &mut s, &mut worker, "initialize", json!({}));
    let list = runtime_message(&mut c, &mut s, &mut worker, "tools/list", json!({}));
    assert_eq!(names(&list).len(), 31);
    assert!(names(&list).contains("serena_repl"));
    let v = runtime_message(
        &mut c,
        &mut s,
        &mut worker,
        "tools/call",
        json!({"name":"serena_repl","arguments":{"session_id":"runtime","code":"{'answer':42,'hasDiagnostic':hasattr(s.lsp,'get_diagnostics_for_symbol'),'text':s.fs.read_file('main.rs')['text']}"}}),
    );
    assert_eq!(v["result"]["isError"], false, "{v}");
    assert_eq!(
        payload(&v)["data"]["result"],
        json!({"answer":42,"hasDiagnostic":false,"text":"// owned defaults source\nfn source() {}\n"})
    );
    worker.close().unwrap();
    let plan = yoke::compatibility::invoke_serena(
        &mut s,
        "create_text_file",
        &json!({"relative_path":"new.rs","content":"fn new() {}"}),
        &Deadline::new(30000),
    )
    .unwrap();
    assert!(plan.is_object() && !r.path().join("new.rs").exists());
    let mut untrusted = spec();
    untrusted.trust_python = false;
    assert!(AgentRepl::spawn(untrusted, &Deadline::new(30000)).is_err());
    let target = root();
    let mut readonly = open(
        &r,
        host(
            json!({"optionalDefaults":"serena_optional_v1","startupProject":"reference","projects":[{"name":"reference","root":target.path(),"readOnly":true}]}),
        ),
        "readonly",
    );
    assert!(
        yoke::compatibility::invoke_serena(
            &mut readonly,
            "create_text_file",
            &json!({"relative_path":"new.rs","content":"fn new() {}"}),
            &Deadline::new(30000)
        )
        .is_err()
    );
    assert!(!target.path().join("new.rs").exists());
}
