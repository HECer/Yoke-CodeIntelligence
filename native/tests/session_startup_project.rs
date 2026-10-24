use serde_json::{Value, json};
use std::{collections::BTreeSet, fs};
use tempfile::TempDir;
use yoke::{
    Engine,
    capture::Deadline,
    project_session::{HostProfile, RECORD_KIND, Session},
    protocol::{ConnectionState, Profile},
};

fn roots() -> (TempDir, TempDir) {
    let primary = TempDir::new().unwrap();
    let target = TempDir::new().unwrap();
    fs::write(primary.path().join("main.rs"), "fn primary() {}\n").unwrap();
    fs::write(target.path().join("main.rs"), "// β\nfn selected() {}\n").unwrap();
    (primary, target)
}
fn host(target: &TempDir, context: &str, startup: Option<&str>) -> HostProfile {
    let mut v = json!({"context":context,"projects":[{"name":"target","root":target.path(),"readOnly":true}],"steering":{"projects":{"target":{"project":{"defaultModes":["planning"],"instructions":"OWN TARGET GUIDANCE","excludedTools":["find_symbol"]}}}}});
    if let Some(startup) = startup {
        v["startupProject"] = json!(startup);
    }
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
fn stored(s: &Session) -> String {
    s.engine
        .store
        .db
        .query_row(
            "SELECT value FROM records WHERE kind=?1",
            [RECORD_KIND],
            |r| r.get(0),
        )
        .unwrap()
}
fn message(s: &mut Session, profile: Profile, method: &str, params: Value) -> Value {
    let mut connection = ConnectionState::default();
    let initialized = connection
        .message_session(
            s,
            profile,
            br#"{"jsonrpc":"2.0","id":0,"method":"initialize"}"#,
        )
        .unwrap()
        .unwrap();
    assert!(initialized.get("result").is_some());
    connection
        .message_session(
            s,
            profile,
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
        .map(|t| t["name"].as_str().unwrap())
        .collect()
}

#[test]
fn three_contexts_select_concrete_project_before_catalog_or_first_operation() {
    for context in ["ide", "claude-code", "grok"] {
        let (r, t) = roots();
        let h = host(&t, context, Some("target"));
        let mut s = open(&r, h.clone(), "locked");
        let c = invoke(&mut s, "configuration", json!({}));
        assert_eq!(c["activeProject"], "target");
        assert_eq!(c["startupProject"], "target");
        assert_eq!(c["projectActivationLocked"], true);
        assert_eq!(c["modes"], json!(["planning"]));
        assert_eq!(c["revision"], 0);
        let tools = message(&mut s, Profile::Serena, "tools/list", json!({}));
        assert!(!names(&tools).contains("activate_project"));
        assert!(!names(&tools).contains("find_symbol"));
        assert!(names(&tools).contains("get_current_config"));
        let instructions = invoke(&mut s, "initialInstructions", json!({}));
        assert!(
            instructions["instructions"]
                .as_str()
                .unwrap()
                .contains("OWN TARGET GUIDANCE")
        );
        assert!(
            instructions["instructions"]
                .as_str()
                .unwrap()
                .contains("Project activation is disabled")
        );
        assert!(!t.path().join(".yoke").exists());
        let before = stored(&s);
        drop(s);
        let resumed = open(&r, h, "locked");
        assert_eq!(stored(&resumed), before);
    }
}

#[test]
fn direct_session_and_compatibility_activation_cannot_escape() {
    let (r, t) = roots();
    let mut s = open(&r, host(&t, "claude-code", Some("target")), "locked");
    let before = stored(&s);
    for project in ["default", "target", "unknown", "../outside"] {
        let error = s
            .invoke(
                "activate",
                &json!({"project":project}),
                &Deadline::new(20000),
            )
            .unwrap_err();
        assert!(error.to_string().contains("Project activation is disabled"));
        assert!(
            yoke::compatibility::invoke_serena(
                &mut s,
                "activate_project",
                &json!({"project":project}),
                &Deadline::new(20000)
            )
            .is_err()
        );
        assert_eq!(stored(&s), before);
    }
    assert!(
        s.invoke(
            "removeProject",
            &json!({"project":"target"}),
            &Deadline::new(20000)
        )
        .unwrap_err()
        .to_string()
        .contains("cannot be removed")
    );
    assert!(
        yoke::compatibility::invoke_serena(
            &mut s,
            "remove_project",
            &json!({"project_name":"target"}),
            &Deadline::new(20000)
        )
        .is_err()
    );
    for profile in [
        Profile::Session,
        Profile::Serena,
        Profile::Graphify,
        Profile::Trail,
        Profile::Graft,
    ] {
        let catalog = message(&mut s, profile, "tools/list", json!({}));
        assert!(!names(&catalog).contains("activate_project"));
    }
    for profile in [Profile::Session, Profile::Serena] {
        let result = message(
            &mut s,
            profile,
            "tools/call",
            json!({"name":if matches!(profile,Profile::Serena){"activate_project"}else{"ci_session"},"arguments":if matches!(profile,Profile::Serena){json!({"project":"default"})}else{json!({"operation":"activate","args":{"project":"default"}})}}),
        );
        assert_eq!(result["result"]["isError"], true);
        assert_eq!(stored(&s), before);
    }
    assert!(!t.path().join(".yoke").exists());
}

#[test]
fn real_authorized_python_generic_callbacks_cannot_change_or_remove_fixed_project() {
    use yoke::agent_repl::{AgentRepl, ReplSpec};
    let (r, t) = roots();
    let mut s = open(&r, host(&t, "claude-code", Some("target")), "python");
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
    let mut worker = AgentRepl::spawn(spec, &Deadline::new(20000)).unwrap();
    let before = stored(&s);
    let positive = worker
        .execute(
            &mut s,
            "answer=42\ns.invoke('configuration')['activeProject']",
            &Deadline::new(20000),
        )
        .unwrap();
    assert!(positive["error"].is_null());
    assert_eq!(positive["result"], "target");
    for code in [
        "s.invoke('activate', {'project':'default'})",
        "s.invoke('removeProject', {'project':'target'})",
    ] {
        let denied = worker.execute(&mut s, code, &Deadline::new(20000)).unwrap();
        assert_eq!(denied["error"]["type"], "RuntimeError");
        assert_eq!(denied["callbackCount"], 1);
        assert_eq!(denied["stateLost"], false);
        assert_eq!(stored(&s), before);
    }
    let survived = worker
        .execute(&mut s, "answer", &Deadline::new(20000))
        .unwrap();
    assert_eq!(survived["result"], 42);
    worker.close().unwrap();
    assert!(!t.path().join(".yoke").exists());
}

#[test]
fn other_contexts_start_selected_but_resume_current_project_without_reapplying_startup() {
    for context in ["desktop-app", "agent"] {
        let (r, t) = roots();
        let h = host(&t, context, Some("target"));
        let mut s = open(&r, h.clone(), "movable");
        assert_eq!(
            invoke(&mut s, "configuration", json!({}))["activeProject"],
            "target"
        );
        assert_eq!(
            invoke(&mut s, "configuration", json!({}))["projectActivationLocked"],
            false
        );
        assert!(s.tool_allowed("activate_project"));
        assert_eq!(
            invoke(&mut s, "activate", json!({"project":"default"}))["activeProject"],
            "default"
        );
        let before = stored(&s);
        drop(s);
        let mut resumed = open(&r, h, "movable");
        assert_eq!(stored(&resumed), before);
        assert_eq!(
            invoke(&mut resumed, "configuration", json!({}))["activeProject"],
            "default"
        );
    }
}

#[test]
fn absent_startup_selection_retains_legacy_profile_fields_and_project_switching() {
    let (r, t) = roots();
    let h = host(&t, "claude-code", None);
    assert!(
        serde_json::to_value(&h)
            .unwrap()
            .get("startupProject")
            .is_none()
    );
    let mut s = open(&r, h.clone(), "legacy");
    let c = invoke(&mut s, "configuration", json!({}));
    assert_eq!(c["activeProject"], "default");
    assert!(c.get("startupProject").is_none() && c.get("projectActivationLocked").is_none());
    assert!(s.tool_allowed("activate_project"));
    assert_eq!(
        invoke(&mut s, "activate", json!({"project":"target"}))["activeProject"],
        "target"
    );
    let before = stored(&s);
    drop(s);
    assert_eq!(stored(&open(&r, h, "legacy")), before);
}

#[test]
fn malformed_unknown_and_query_only_startup_selections_refuse_without_session_publication() {
    for name in [
        "",
        "../target",
        "/target",
        "tаrget",
        "unknown",
        &"x".repeat(101),
    ] {
        let (r, t) = roots();
        let engine = Engine::open(r.path()).unwrap();
        assert!(
            Session::open(
                engine,
                host(&t, "ide", Some(name)),
                Some("bad"),
                &Deadline::new(20000)
            )
            .is_err()
        );
        let engine = Engine::open(r.path()).unwrap();
        let count: u64 = engine
            .store
            .db
            .query_row(
                "SELECT count(*) FROM records WHERE kind=?1",
                [RECORD_KIND],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count, 0);
        assert!(!t.path().join(".yoke").exists());
    }
    let (r, t) = roots();
    let mut h = host(&t, "ide", Some("target"));
    h.projects[0].query_only = true;
    assert!(
        Session::open(
            Engine::open(r.path()).unwrap(),
            h,
            Some("bad"),
            &Deadline::new(20000)
        )
        .is_err()
    );
    assert!(!t.path().join(".yoke").exists());
}

#[test]
fn fixed_project_state_and_changed_host_selection_cannot_be_forged_on_resume() {
    let (r, t) = roots();
    let h = host(&t, "grok", Some("target"));
    let s = open(&r, h.clone(), "forged");
    let original = stored(&s);
    let mut row: Value = serde_json::from_str(&original).unwrap();
    row["activeProject"] = json!("default");
    let forged = serde_json::to_string(&row).unwrap();
    s.engine
        .store
        .db
        .execute(
            "UPDATE records SET value=?1 WHERE kind=?2",
            rusqlite::params![forged, RECORD_KIND],
        )
        .unwrap();
    drop(s);
    let error = Session::open(
        Engine::open(r.path()).unwrap(),
        h,
        Some("forged"),
        &Deadline::new(20000),
    )
    .err()
    .unwrap();
    assert!(error.to_string().contains("fixed startup project"));
    let engine = Engine::open(r.path()).unwrap();
    let retained: String = engine
        .store
        .db
        .query_row(
            "SELECT value FROM records WHERE kind=?1",
            [RECORD_KIND],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(retained, forged);
    let changed = host(&t, "grok", Some("default"));
    assert!(
        Session::open(engine, changed, Some("forged"), &Deadline::new(20000))
            .err()
            .unwrap()
            .to_string()
            .contains("host binding changed")
    );
}

#[test]
fn modes_cas_and_read_only_source_operations_retain_the_fixed_project() {
    let (r, t) = roots();
    let h = host(&t, "ide", Some("target"));
    let mut s = open(&r, h.clone(), "live");
    let mut stale = open(&r, h, "live");
    let c = invoke(&mut s, "switchModes", json!({"modes":["one-shot"]}));
    assert_eq!(c["activeProject"], "target");
    assert_eq!(c["projectActivationLocked"], true);
    assert!(
        stale
            .invoke(
                "switchModes",
                &json!({"modes":["editing"]}),
                &Deadline::new(20000)
            )
            .unwrap_err()
            .to_string()
            .contains("concurrently")
    );
    let c = invoke(&mut s, "switchModes", json!({"reset":true}));
    assert_eq!(c["modes"], json!(["planning"]));
    assert!(!s.tool_allowed("activate_project"));
    let read = yoke::agent_repl::invoke_facade(
        &mut s,
        "fs.read_file",
        json!({"relative_path":"main.rs"}),
        None,
        &Deadline::new(20000),
    )
    .unwrap();
    assert!(read.to_string().contains("selected"));
    assert!(!read.to_string().contains("primary"));
    assert!(
        yoke::agent_repl::invoke_facade(
            &mut s,
            "fs.create_text_file",
            json!({"relative_path":"forbidden.rs","content":"fn nope() {}"}),
            None,
            &Deadline::new(20000)
        )
        .is_err()
    );
    assert!(!t.path().join("forbidden.rs").exists());
    assert_eq!(
        fs::read(t.path().join("main.rs")).unwrap(),
        "// β\nfn selected() {}\n".as_bytes()
    );
    assert_eq!(
        fs::read(r.path().join("main.rs")).unwrap(),
        b"fn primary() {}\n"
    );
}
