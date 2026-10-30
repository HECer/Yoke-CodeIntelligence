use serde_json::{Value, json};
use std::{fs, path::PathBuf};
use tempfile::TempDir;
use yoke::{
    Engine,
    agent_repl::{AgentRepl, ReplSpec},
    capture::Deadline,
    project_session::{HostProfile, Session},
};

fn source() -> TempDir {
    let root = TempDir::new().unwrap();
    fs::write(
        root.path().join("main.rs"),
        "// 🌍\nfn helper() {}\nfn main() { helper(); }\n",
    )
    .unwrap();
    root
}
fn host(value: Value) -> HostProfile {
    serde_json::from_value(value).unwrap()
}
fn open(root: &TempDir, profile: HostProfile, id: &str) -> Session {
    Session::open(
        Engine::open(root.path()).unwrap(),
        profile,
        Some(id),
        &Deadline::new(90000),
    )
    .unwrap()
}
fn spec() -> ReplSpec {
    let output = std::process::Command::new("python")
        .args(["-I", "-c", "import sys;print(sys.executable)"])
        .output()
        .unwrap();
    assert!(output.status.success());
    ReplSpec {
        python: PathBuf::from(String::from_utf8(output.stdout).unwrap().trim()),
        trust_python: true,
        max_output_bytes: 100000,
        shell: None,
    }
}

#[test]
#[cfg(windows)]
fn invalid_windows_python_images_refuse_before_any_worker_launch() {
    let root = source();
    for (name, bytes) in [
        ("not-python.exe", b"Owned non-executable fixture".as_slice()),
        ("pýtĥon-🌍.EXE", b"MZ\0\0\0\0".as_slice()),
    ] {
        let executable = root.path().join(name);
        fs::write(&executable, bytes).unwrap();
        let selected = ReplSpec {
            python: executable,
            trust_python: true,
            max_output_bytes: 1000,
            shell: None,
        };
        // Pure host registration remains non-executing and can classify later.
        AgentRepl::validate_spec(&selected).unwrap();
        let started = std::time::Instant::now();
        let error = match AgentRepl::spawn(selected, &Deadline::new(3000)) {
            Ok(mut worker) => {
                worker.close().unwrap();
                panic!("Invalid image started a worker")
            }
            Err(error) => error,
        };
        assert!(
            error.to_string().contains("Windows PE executable"),
            "{error}"
        );
        assert!(started.elapsed() < std::time::Duration::from_secs(2));
    }
    assert!(!root.path().join(".yoke").exists());
}
fn run(worker: &mut AgentRepl, session: &mut Session, code: &str) -> Value {
    worker
        .execute(session, code, &Deadline::new(20000))
        .unwrap()
}
fn steer(session: &mut Session, args: Value) {
    session
        .invoke("switchModes", &args, &Deadline::new(20000))
        .unwrap();
}
fn snapshots(session: &Session) -> i64 {
    session
        .engine
        .store
        .db
        .query_row("SELECT COUNT(*) FROM snapshots", [], |r| r.get(0))
        .unwrap()
}

#[test]
fn mode_selection_hides_groups_and_cached_calls_are_checked_by_the_host() {
    let root = source();
    let profile =
        host(json!({"steering":{"modes":{"restricted":{"excludedApis":["mem","fs.read_file"]}}}}));
    let mut session = open(&root, profile, "cached");
    session
        .invoke(
            "notes.write",
            &json!({"name":"decision","content":"owned API note"}),
            &Deadline::new(20000),
        )
        .unwrap();
    let mut worker = AgentRepl::spawn(spec(), &Deadline::new(20000)).unwrap();
    let saved = run(
        &mut worker,
        &mut session,
        "saved_group=s.mem\nsaved_read=s.mem.read_memory\nsaved_native=s.invoke\nsaved_read('decision')",
    );
    assert!(saved["error"].is_null(), "{saved}");
    assert!(saved["result"].to_string().contains("owned API note"));
    steer(&mut session, json!({"modes":["restricted"]}));
    let discovery = run(
        &mut worker,
        &mut session,
        "{'groups':dir(s),'oldGroup':dir(saved_group),'fs':dir(s.fs),'hasMem':hasattr(s,'mem')}",
    );
    assert!(discovery["error"].is_null(), "{discovery}");
    assert_eq!(discovery["result"]["hasMem"], false);
    assert_eq!(discovery["result"]["oldGroup"], json!([]));
    assert!(
        !discovery["result"]["fs"]
            .as_array()
            .unwrap()
            .contains(&json!("read_file"))
    );
    let before = snapshots(&session);
    for code in [
        "saved_read('decision')",
        "import __main__ as w\nw._methods=w._all_methods\nsaved_read('decision')",
    ] {
        let denied = run(&mut worker, &mut session, code);
        assert!(
            denied["error"]["message"]
                .as_str()
                .unwrap()
                .contains("API excluded by session steering"),
            "{denied}"
        );
        assert_eq!(denied["callbackCount"], 1);
    }
    let denied = run(
        &mut worker,
        &mut session,
        "saved_native('notes.read',{'name':'decision'})",
    );
    assert!(
        denied["error"]["message"]
            .as_str()
            .unwrap()
            .contains("Generic native callbacks are unavailable")
    );
    assert_eq!(snapshots(&session), before);
    // Ordinary public tools have their separate tool selection, not the REPL selection.
    let tool = yoke::compatibility::invoke_serena(
        &mut session,
        "read_memory",
        &json!({"memory_name":"decision"}),
        &Deadline::new(20000),
    )
    .unwrap();
    assert!(tool.to_string().contains("owned API note"));
    steer(&mut session, json!({"reset":true}));
    let resumed = run(&mut worker, &mut session, "saved_read('decision')");
    assert!(resumed["error"].is_null(), "{resumed}");
    assert_eq!(resumed["result"], saved["result"]);
    worker.close().unwrap();
}

#[test]
fn all_six_layers_accumulate_exact_api_selectors_without_changing_tool_policy() {
    let root = source();
    let profile = host(
        json!({"context":"review-client","modes":["review"],"steering":{
            "global":{"excludedApis":["mem.read_memory"]},
            "projects":{"default":{"project":{"excludedApis":["jb.debug_eval"]},"projectLocal":{"excludedApis":["fs.create_text_file"]}}},
            "contexts":{"review-client":{"excludedApis":["shell"]}},
            "modes":{"review":{"excludedApis":["edit","mem.write_memory"]}},
            "cli":{"excludedApis":["lsp"]}
        }}),
    );
    let mut session = open(&root, profile, "layers");
    let configuration = session
        .invoke("configuration", &json!({}), &Deadline::new(20000))
        .unwrap();
    assert_eq!(
        configuration["steering"]["excludedApis"],
        json!([
            "edit",
            "fs.create_text_file",
            "jb.debug_eval",
            "lsp",
            "mem.read_memory",
            "mem.write_memory",
            "shell"
        ])
    );
    let selected = session.selected_repl_methods().unwrap().unwrap();
    assert!(selected.contains(&"fs.read_file".to_string()));
    for name in [
        "edit.replace_content",
        "mem.read_memory",
        "mem.write_memory",
        "lsp.find_symbol",
        "shell.execute_shell_command",
        "jb.debug_eval",
    ] {
        assert!(!selected.contains(&name.to_string()));
        assert!(session.require_repl_api(name).is_err());
    }
    assert!(session.tool_allowed("read_memory") && session.tool_allowed("execute_shell_command"));
    assert!(!session.engine.allow_writes);
}

#[test]
fn cached_project_context_cannot_escape_a_mode_change_inside_the_same_cell() {
    let root = source();
    let reference = source();
    let profile = host(
        json!({"projects":[{"name":"reference","root":reference.path(),"readOnly":true,"queryOnly":true}],"steering":{"modes":{"restricted":{"excludedApis":["ext.project_context"]}}}}),
    );
    let mut session = open(&root, profile, "project-context");
    let mut worker = AgentRepl::spawn(spec(), &Deadline::new(20000)).unwrap();
    let response = run(
        &mut worker,
        &mut session,
        "saved=s.ext.project_context('reference')\ns.invoke('switchModes',{'modes':['restricted']})\nwith saved:\n pass",
    );
    assert!(
        response["error"]["message"]
            .as_str()
            .unwrap()
            .contains("API excluded by session steering"),
        "{response}"
    );
    assert_eq!(response["callbackCount"], 2);
    assert!(!reference.path().join(".yoke").exists());
    assert_eq!(snapshots(&session), 0);
    worker.close().unwrap();
}

#[test]
fn qualified_declaration_bridges_keep_origin_and_tool_adapters_remain_independent() {
    let root = source();
    let d = Deadline::new(30000);
    let mut engine = Engine::open(root.path()).unwrap();
    engine
        .register_configured_server(yoke::lsp::ConfiguredServer {
            server: yoke::lsp::Server {
                command: "node".into(),
                args: vec![
                    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                        .join("tests/fixtures/advanced-lsp.mjs")
                        .display()
                        .to_string(),
                ],
                language: "rust".into(),
                trust: true,
            },
            configuration: yoke::lsp::HostConfiguration {
                passive_profile: Some(yoke::lsp::PassiveProfile::ContractFixture),
                ..Default::default()
            },
        })
        .unwrap();
    let mut session = Session::open(
        engine,
        host(json!({"steering":{"global":{"excludedApis":["lsp.find_declaration"]}}})),
        Some("declaration"),
        &d,
    )
    .unwrap();
    let mut worker = AgentRepl::spawn(spec(), &d).unwrap();
    let selected = run(
        &mut worker,
        &mut session,
        "s.jb.find_declaration('main.rs',r'(?<=\\{ )(helper)(?=\\(\\))')['locations'][0]['startByte']",
    );
    assert!(selected["error"].is_null(), "{selected}");
    assert_eq!(selected["result"], 11);
    let denied = run(
        &mut worker,
        &mut session,
        "import __main__ as w\nw._rpc('bridge.lsp.find_declaration.at',{},None)",
    );
    assert!(
        denied["error"]["message"]
            .as_str()
            .unwrap()
            .contains("API excluded by session steering")
    );
    let denied = run(
        &mut worker,
        &mut session,
        "w._rpc('bridge.declaration_source',{},None)",
    );
    assert!(
        denied["error"]["message"]
            .as_str()
            .unwrap()
            .contains("Unknown declared REPL API")
    );
    let tool = yoke::compatibility::invoke_serena_with_repl(
        &mut session,
        &mut worker,
        "find_declaration",
        &json!({"relative_path":"main.rs","regex":"(?<=\\{ )(helper)(?=\\(\\))"}),
        &d,
    )
    .unwrap();
    assert_eq!(tool["locations"][0]["startByte"], 11, "{tool}");
    let discovery = run(
        &mut worker,
        &mut session,
        "'find_declaration' in dir(s.lsp)",
    );
    assert_eq!(discovery["result"], false);
    worker.close().unwrap();
}

#[test]
fn empty_api_surface_is_explicit_and_does_not_claim_to_sandbox_trusted_python() {
    let root = source();
    let mut session = open(
        &root,
        host(
            json!({"steering":{"global":{"excludedApis":["cfg","edit","ext","fs","jb","lsp","mem","shell"]}}}),
        ),
        "empty",
    );
    assert_eq!(session.selected_repl_methods().unwrap(), Some(vec![]));
    let mut worker = AgentRepl::spawn(spec(), &Deadline::new(20000)).unwrap();
    let result = run(
        &mut worker,
        &mut session,
        "{'groups':[n for n in dir(s) if n in ['mem','fs','cfg','invoke']],'calculation':sum([1,2,3])}",
    );
    assert!(result["error"].is_null(), "{result}");
    assert_eq!(result["result"], json!({"groups":[],"calculation":6}));
    assert_eq!(result["callbackCount"], 0);
    let denied = run(
        &mut worker,
        &mut session,
        "import __main__ as w\nw._rpc('mem.list_memories',{})",
    );
    assert!(
        denied["error"]["message"]
            .as_str()
            .unwrap()
            .contains("API excluded by session steering")
    );
    worker.close().unwrap();
}

#[test]
fn stale_registered_policy_refuses_before_executing_a_cell() {
    let root = source();
    let profile = host(json!({"steering":{"modes":{"restricted":{"excludedApis":["mem"]}}}}));
    let mut stale = open(&root, profile.clone(), "concurrent");
    let mut current = open(&root, profile, "concurrent");
    let mut worker = AgentRepl::spawn(spec(), &Deadline::new(20000)).unwrap();
    steer(&mut current, json!({"modes":["restricted"]}));
    let marker = root.path().join("must-not-execute");
    let code = format!(
        "__import__('pathlib').Path({}).write_text('unexpected')",
        serde_json::to_string(marker.to_str().unwrap()).unwrap()
    );
    let error = worker
        .execute(&mut stale, &code, &Deadline::new(20000))
        .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("Session state changed concurrently")
    );
    assert!(!marker.exists());
    assert!(!worker.is_alive());
}

#[test]
fn api_schema_bounds_exact_names_and_legacy_serialization_are_preserved() {
    let root = source();
    for value in [
        json!({"steering":{"global":{"excludedApis":["mem.*"]}}}),
        json!({"steering":{"global":{"excludedApis":["mem","mem"]}}}),
        json!({"steering":{"global":{"excludedApis":["mеm"]}}}),
        json!({"steering":{"global":{"excludedApis":["native.notes.read"]}}}),
        json!({"steering":{"global":{"excludedApis":vec!["mem";59]}}}),
    ] {
        assert!(
            Session::open(
                Engine::open(root.path()).unwrap(),
                host(value),
                Some("refused"),
                &Deadline::new(20000)
            )
            .is_err()
        );
    }
    let legacy = host(json!({"steering":{}}));
    let serialized = serde_json::to_value(&legacy).unwrap();
    assert!(
        serialized["steering"]["global"]
            .get("excludedApis")
            .is_none()
    );
    let mut session = open(&root, legacy, "legacy-api");
    let before = session
        .invoke("configuration", &json!({}), &Deadline::new(20000))
        .unwrap();
    assert!(before["steering"].get("excludedApis").is_none());
    assert!(session.selected_repl_methods().unwrap().is_none());
    let mut worker = AgentRepl::spawn(spec(), &Deadline::new(20000)).unwrap();
    let old = run(
        &mut worker,
        &mut session,
        "(len(dir(s.mem)),s.invoke('configuration')['context'])",
    );
    assert_eq!(old["result"], json!([7, "agent"]), "{old}");
    worker.close().unwrap();
}
