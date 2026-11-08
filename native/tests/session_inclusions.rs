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
        "// owned inclusion source\nfn source() {}\n",
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
fn all_six_layers_include_exact_tools_and_individual_api_exceptions_with_provenance() {
    let r = root();
    for (i, path, source) in [
        (0, "/steering/global", "global"),
        (1, "/steering/projects/default/project", "project"),
        (
            2,
            "/steering/projects/default/projectLocal",
            "project-local",
        ),
        (3, "/steering/contexts/custom", "context:custom"),
        (4, "/steering/modes/custom", "mode:custom"),
        (5, "/steering/cli", "cli"),
    ] {
        let mut h = json!({"context":"custom","modes":["custom"],"steering":{"global":{"excludedTools":["read_file"],"excludedApis":["fs"]},"projects":{"default":{"project":{},"projectLocal":{}}},"contexts":{"custom":{}},"modes":{"custom":{}},"cli":{}}});
        let layer = h.pointer_mut(path).unwrap();
        layer["includedOptionalTools"] = json!(["read_file"]);
        layer["includedApis"] = json!(["fs.read_file"]);
        let mut s = open(&r, host(h), &format!("layer{i}"));
        assert!(s.tool_allowed("read_file"));
        assert!(s.require_repl_api("fs.read_file").is_ok());
        assert!(s.require_repl_api("fs.list_dir").is_err());
        let cfg = call(&mut s, "configuration", json!({}));
        assert_eq!(
            cfg["steering"]["inclusions"],
            json!([{"source":source,"tools":["read_file"],"apis":["fs.read_file"]}])
        );
        assert!(
            cfg["steering"]["excludedApis"]
                .as_array()
                .unwrap()
                .iter()
                .all(|v| v != "fs" && v != "fs.read_file")
        );
    }
}

#[test]
fn inclusions_extend_an_earlier_fixed_base_and_query_aliases_share_later_exclusions() {
    let r = root();
    let h = json!({"context":"desktop-app","modes":["grant","deny"],"steering":{"global":{"fixedTools":["get_current_config"]},"projects":{"default":{"project":{"includedOptionalTools":["graft_ask","read_file"]},"projectLocal":{"excludedTools":["read_file"]}}},"modes":{"grant":{"includedOptionalTools":["read_file"]},"deny":{"excludedTools":["trail_find_code","read_file"]}},"cli":{"includedOptionalTools":["graft_find_code"]}}});
    let mut s = open(&r, host(h), "base");
    assert!(s.tool_allowed("get_current_config") && !s.tool_allowed("read_file"));
    for name in ["graft_ask", "graft_find_code", "trail_find_code"] {
        assert!(s.tool_allowed(name));
    }
    assert!(!s.tool_allowed("graft_find_all") && !s.tool_allowed("list_dir"));
    let cfg = call(&mut s, "configuration", json!({}));
    assert_eq!(cfg["steering"]["fixedToolsSource"], "global");
    assert_eq!(cfg["steering"]["inclusions"].as_array().unwrap().len(), 3);
    call(&mut s, "switchModes", json!({"modes":["deny","grant"]}));
    assert!(s.tool_allowed("read_file"));
    assert!(!s.tool_allowed("list_dir"));
}

#[test]
fn invalid_duplicate_path_unicode_budget_and_fixed_incremental_conflicts_refuse_before_publication()
{
    let r = root();
    for (i, steering) in [
        json!({"global":{"includedOptionalTools":["unknown"]}}),
        json!({"cli":{"includedOptionalTools":["read_file","read_file"]}}),
        json!({"global":{"includedOptionalTools":["../read_file"]}}),
        json!({"global":{"includedOptionalTools":["read_f\u{0456}le"]}}),
        json!({"global":{"includedOptionalTools":["x".repeat(262145)]}}),
        json!({"global":{"includedApis":["fs.*"]}}),
        json!({"global":{"includedApis":["fs.read_file","fs.read_file"]}}),
        json!({"global":{"includedApis":["fs.read_f\u{0456}le"]}}),
        json!({"global":{"includedApis":vec!["fs";100]}}),
    ]
    .into_iter()
    .enumerate()
    {
        let h = host(json!({"steering":steering}));
        assert!(
            Session::open(
                Engine::open(r.path()).unwrap(),
                h,
                Some(&format!("bad{i}")),
                &Deadline::new(30000)
            )
            .is_err()
        );
    }
    for path in [
        "/steering/global",
        "/steering/projects/default/project",
        "/steering/projects/default/projectLocal",
        "/steering/contexts/custom",
        "/steering/modes/custom",
        "/steering/cli",
    ] {
        let mut h = json!({"context":"custom","modes":["custom"],"steering":{"global":{},"projects":{"default":{"project":{},"projectLocal":{}}},"contexts":{"custom":{}},"modes":{"custom":{}},"cli":{}}});
        *h.pointer_mut(path).unwrap() =
            json!({"fixedTools":["get_current_config"],"includedOptionalTools":["read_file"]});
        let err = Session::open(
            Engine::open(r.path()).unwrap(),
            host(h),
            Some("conflict"),
            &Deadline::new(30000),
        )
        .err()
        .unwrap();
        assert!(err.to_string().contains("Fixed tools"), "{err}");
    }
    for v in [
        json!({"steering":{"global":{"includedApis":true}}}),
        json!({"steering":{"global":{"includedOptionalTools":"read_file"}}}),
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
            |v| v.get(0),
        )
        .unwrap();
    assert_eq!(count, 0);
}

#[test]
fn empty_inclusions_preserve_old_bindings_and_original53_tools52_and_repl6_views() {
    let r = root();
    let old = host(json!({"context":"desktop-app","steering":{}}));
    let empty = host(
        json!({"context":"desktop-app","steering":{"global":{"includedOptionalTools":[],"includedApis":[]},"cli":{"includedOptionalTools":[],"includedApis":[]}}}),
    );
    assert_eq!(
        serde_json::to_vec(&old).unwrap(),
        serde_json::to_vec(&empty).unwrap()
    );
    let s = open(&r, old, "legacy");
    let before = stored(&s);
    drop(s);
    let mut s = open(&r, empty, "legacy");
    assert_eq!(stored(&s), before);
    assert!(
        call(&mut s, "configuration", json!({}))["steering"]
            .get("inclusions")
            .is_none()
    );
    for (label, setting, count) in [
        ("legacy", Value::Null, 53),
        ("tools", json!("tools"), 52),
        ("repl", json!("repl"), 6),
    ] {
        let mut s = open(
            &r,
            host(json!({"context":"desktop-app","agentInterface":setting,"steering":{}})),
            label,
        );
        let mut c = ConnectionState::default();
        message(&mut c, &mut s, Profile::Serena, "initialize", json!({}));
        let list = message(&mut c, &mut s, Profile::Serena, "tools/list", json!({}));
        assert_eq!(names(&list).len(), count);
        if setting.is_null() {
            assert_eq!(list["result"]["tools"], yoke::compatibility::serena_tools());
        }
    }
    let changed = host(
        json!({"context":"desktop-app","steering":{"cli":{"includedOptionalTools":["read_file"]}}}),
    );
    assert!(
        Session::open(
            Engine::open(r.path()).unwrap(),
            changed,
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
fn real_cached_python_callbacks_observe_in_cell_inclusions_and_later_denials() {
    let r = root();
    let mut s = open(
        &r,
        host(
            json!({"steering":{"modes":{"blocked":{"excludedApis":["fs","mem"]},"grant":{"includedApis":["fs.read_file"]}}}}),
        ),
        "cells",
    );
    let mut worker = AgentRepl::spawn(spec(), &Deadline::new(30000)).unwrap();
    let v=worker.execute(&mut s,"answer=42\nsaved_read=s.fs.read_file\nsaved_list=s.fs.list_dir\nsaved_native=s.invoke\ns.invoke('switchModes',{'modes':['blocked','grant']})\nsaved_read('main.rs')['text']",&Deadline::new(30000)).unwrap();
    assert!(v["error"].is_null(), "{v}");
    assert_eq!(v["result"], "// owned inclusion source\nfn source() {}\n");
    let v=worker.execute(&mut s,"{'answer':answer,'hasRead':hasattr(s.fs,'read_file'),'hasList':hasattr(s.fs,'list_dir'),'hasMemory':hasattr(s,'mem'),'hasNative':hasattr(s,'invoke')}",&Deadline::new(30000)).unwrap();
    assert_eq!(
        v["result"],
        json!({"answer":42,"hasRead":true,"hasList":false,"hasMemory":false,"hasNative":false})
    );
    for code in ["saved_list('.')", "saved_native('configuration')"] {
        let v = worker.execute(&mut s, code, &Deadline::new(30000)).unwrap();
        assert!(!v["error"].is_null(), "{v}");
        assert!(worker.is_alive());
    }
    call(&mut s, "switchModes", json!({"modes":["grant","blocked"]}));
    let v = worker
        .execute(&mut s, "saved_read('main.rs')", &Deadline::new(30000))
        .unwrap();
    assert!(!v["error"].is_null());
    assert_eq!(
        worker
            .execute(&mut s, "answer", &Deadline::new(30000))
            .unwrap()["result"],
        42
    );
    worker.close().unwrap();
}

#[test]
fn inclusion_cannot_grant_source_application_startup_activation_or_caller_policy_changes() {
    let r = root();
    let mut s = open(
        &r,
        host(
            json!({"steering":{"global":{"excludedTools":["create_text_file"]},"cli":{"includedOptionalTools":["create_text_file"]}}}),
        ),
        "plans",
    );
    let plan = yoke::compatibility::invoke_serena(
        &mut s,
        "create_text_file",
        &json!({"relative_path":"new.rs","content":"fn new() {}"}),
        &Deadline::new(30000),
    )
    .unwrap();
    assert!(plan.is_object() && !r.path().join("new.rs").exists());
    let before = stored(&s);
    assert!(
        s.invoke(
            "switchModes",
            &json!({"modes":["editing"],"includedApis":["fs"]}),
            &Deadline::new(30000)
        )
        .is_err()
    );
    assert_eq!(stored(&s), before);
    let mut locked = open(
        &r,
        host(
            json!({"context":"claude-code","startupProject":"default","steering":{"cli":{"includedOptionalTools":["activate_project","read_file"]}}}),
        ),
        "locked",
    );
    assert!(!locked.tool_allowed("activate_project") && locked.tool_allowed("read_file"));
    assert!(
        locked
            .invoke(
                "activate",
                &json!({"project":"default"}),
                &Deadline::new(30000)
            )
            .is_err()
    );
    let reference = root();
    let mut readonly = open(
        &r,
        host(
            json!({"startupProject":"reference","projects":[{"name":"reference","root":reference.path(),"readOnly":true}],"steering":{"cli":{"includedOptionalTools":["create_text_file"]}}}),
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
    assert!(!reference.path().join("new.rs").exists());
}

#[test]
fn mcp_catalog_dispatch_notifications_and_real_python_use_the_effective_inclusions() {
    let r = root();
    let mut s = open(
        &r,
        host(
            json!({"steering":{"global":{"fixedTools":["ci_session"]},"modes":{"grant":{"includedOptionalTools":["ci_status"]}}}}),
        ),
        "catalog",
    );
    let mut c = ConnectionState::default();
    message(&mut c, &mut s, Profile::Session, "initialize", json!({}));
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
    let v = message(
        &mut c,
        &mut s,
        Profile::Session,
        "tools/call",
        json!({"name":"ci_session","arguments":{"operation":"switchModes","args":{"modes":["grant"]}}}),
    );
    assert_eq!(v["result"]["isError"], false);
    assert_eq!(c.take_notifications().len(), 1);
    assert_eq!(
        names(&message(
            &mut c,
            &mut s,
            Profile::Session,
            "tools/list",
            json!({})
        )),
        BTreeSet::from(["ci_session", "ci_status"])
    );
    let mut s = open(
        &r,
        host(
            json!({"context":"desktop-app","steering":{"global":{"excludedTools":["read_file"],"excludedApis":["fs","mem"]},"cli":{"includedOptionalTools":["read_file"],"includedApis":["fs.read_file"]}}}),
        ),
        "mcp",
    );
    let mut worker = AgentRepl::spawn(spec(), &Deadline::new(30000)).unwrap();
    let mut c = ConnectionState::default();
    runtime_message(&mut c, &mut s, &mut worker, "initialize", json!({}));
    assert_eq!(
        runtime_message(&mut c, &mut s, &mut worker, "tools/list", json!({}))["result"]["tools"],
        yoke::compatibility::serena_tools()
    );
    let v = runtime_message(
        &mut c,
        &mut s,
        &mut worker,
        "tools/call",
        json!({"name":"read_file","arguments":{"relative_path":"main.rs"}}),
    );
    assert_eq!(v["result"]["isError"], false, "{v}");
    let v = runtime_message(
        &mut c,
        &mut s,
        &mut worker,
        "tools/call",
        json!({"name":"serena_repl","arguments":{"session_id":"mcp","code":"{'answer':40+2,'hasList':hasattr(s.fs,'list_dir'),'hasMemory':hasattr(s,'mem'),'text':s.fs.read_file('main.rs')['text']}"}}),
    );
    assert_eq!(v["result"]["isError"], false, "{v}");
    assert_eq!(
        payload(&v)["data"]["result"],
        json!({"answer":42,"hasList":false,"hasMemory":false,"text":"// owned inclusion source\nfn source() {}\n"})
    );
    worker.close().unwrap();
}

#[test]
fn registered_unused_inclusions_require_current_state_before_catalogs_adapters_and_cells() {
    let r = root();
    let h = host(
        json!({"steering":{"modes":{"grant":{"includedOptionalTools":["read_file"],"includedApis":["fs.read_file"]}}}}),
    );
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
    call(&mut peer, "switchModes", json!({"modes":["grant"]}));
    let current = stored(&peer);
    let stale = message(&mut c, &mut s, Profile::Serena, "tools/list", json!({}));
    assert_eq!(stale["error"]["code"], -32001);
    assert!(stale.get("result").is_none());
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
    let output = TempDir::new().unwrap();
    let marker = output.path().join("must-not-run.txt");
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
    assert_eq!(stored(&s), current);
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
