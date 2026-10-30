use serde_json::{Value, json};
use std::{fs, path::Path, process::Command};
use yoke::{
    Engine, agent_repl,
    capture::{self, CaptureOptions, Deadline},
    changes::{self, Plan},
    lsp,
    project_session::{HostProfile, HostProject, Session},
};
const IGNORED: &str = "// owned \u{1f4a1} e\u{301}\r\nfn ignored_nested() {}\r\n";
fn d() -> Deadline {
    Deadline::new(30000)
}
fn root() -> tempfile::TempDir {
    let root = tempfile::tempdir().unwrap();
    for dir in ["ignored", "target", "node_modules"] {
        fs::create_dir(root.path().join(dir)).unwrap();
    }
    for (name, bytes) in [
        ("main.rs", b"fn visible() {}\n".as_slice()),
        ("ignored/\u{e9}.rs", IGNORED.as_bytes()),
        ("root-ignored.rs", b"fn ignored_root() {}\n"),
        ("ordinary-ignore.rs", b"fn ordinary_filtered() {}\n"),
        (".gitignore", b"ignored/\nroot-ignored.rs\n"),
        (".ignore", b"ordinary-ignore.rs\n"),
        (".env", b"OWNED_FIXTURE=not-a-credential\n"),
        ("target/hard.rs", b"fn hard_target() {}\n"),
        ("node_modules/hard.rs", b"fn hard_dependency() {}\n"),
    ] {
        fs::write(root.path().join(name), bytes).unwrap();
    }
    root
}
fn profile(value: Value) -> HostProfile {
    serde_json::from_value(value).unwrap()
}
fn open(path: &Path, p: HostProfile, name: &str) -> Session {
    let mut engine = Engine::open(path).unwrap();
    engine.allow_writes = true;
    Session::open(engine, p, Some(name), &d()).unwrap()
}
fn selected() -> HostProfile {
    profile(json!({"gitIgnore":{"global":false}}))
}
fn symbols(s: &mut Session) -> Value {
    agent_repl::invoke_facade(
        s,
        "lsp.find_symbol",
        json!({"name_path_pattern":"*"}),
        None,
        &d(),
    )
    .unwrap()
}
fn prepare(s: &mut Session, path: &str) -> Plan {
    let value = s
        .invoke(
            "edit.prepare",
            &json!({"edits":[{"kind":"create","path":path,"text":"fn planned() {}\n"}]}),
            &d(),
        )
        .unwrap();
    assert_eq!(value["status"], "ok", "{value}");
    serde_json::from_value(value["data"]["plan"].clone()).unwrap()
}
fn verify(engine: &Engine, plan: &Plan, path: &str) {
    let script = format!(
        "process.exit(require('node:fs').readFileSync({},'utf8')==='fn planned() {{}}\\n'?0:1)",
        serde_json::to_string(path).unwrap()
    );
    let reply = yoke::verify::execute(engine, &plan.id, "node", &["-e".into(), script], true, &d())
        .unwrap();
    assert_eq!(reply["status"], "passed", "{reply}");
}
#[test]
fn absent_null_and_empty_keep_exact_legacy_profile_capture_and_query_bytes() {
    let r = root();
    let base = HostProfile::default();
    let bytes = serde_json::to_vec(&base).unwrap();
    let mut s = open(r.path(), base, "legacy");
    let config = s.invoke("configuration", &json!({}), &d()).unwrap();
    let old = symbols(&mut s);
    let (snapshot, _, _, _) = s.engine.index(&d()).unwrap();
    drop(s);
    assert!(
        serde_json::to_value(CaptureOptions::default())
            .unwrap()
            .get("includeGitIgnored")
            .is_none()
    );
    for policy in [
        Value::Null,
        json!({}),
        json!({"global":null,"projects":{"default":{"local":null}}}),
    ] {
        let host = profile(json!({"gitIgnore":policy}));
        assert_eq!(serde_json::to_vec(&host).unwrap(), bytes);
        let mut s = open(r.path(), host, "legacy");
        assert_eq!(s.invoke("configuration", &json!({}), &d()).unwrap(), config);
        assert_eq!(
            serde_json::to_vec(&symbols(&mut s)).unwrap(),
            serde_json::to_vec(&old).unwrap()
        );
        assert_eq!(
            serde_json::to_vec(&s.engine.index(&d()).unwrap().0).unwrap(),
            serde_json::to_vec(&snapshot).unwrap()
        );
    }
    let mut explicit = open(
        r.path(),
        profile(json!({"gitIgnore":{"global":true}})),
        "explicit-true",
    );
    assert_eq!(
        serde_json::to_vec(&explicit.engine.index(&d()).unwrap().0).unwrap(),
        serde_json::to_vec(&snapshot).unwrap()
    );
}
#[test]
fn real_capture_and_workspace_index_include_git_ignored_sources_without_other_read_grants() {
    let r = root();
    let mut s = open(
        r.path(),
        profile(
            json!({"gitIgnore":{"global":false},"symbolWorkspaceFolders":{"global":["ignored"]}}),
        ),
        "selected",
    );
    let (snapshot, _, _, _) = s.engine.index(&d()).unwrap();
    let scope = capture::source_scope_policy(&s.engine.store, &snapshot).unwrap();
    for path in ["ignored/\u{e9}.rs", "root-ignored.rs"] {
        assert!(scope.ignored_at_capture(path).unwrap());
        assert!(scope.path_policy(path).unwrap().read_only);
    }
    assert_eq!(
        scope.read_source("ignored/\u{e9}.rs").unwrap(),
        IGNORED.as_bytes()
    );
    assert!(!scope.path_policy("main.rs").unwrap().read_only);
    for path in [
        "ordinary-ignore.rs",
        ".env",
        "target/hard.rs",
        "node_modules/hard.rs",
    ] {
        assert!(scope.file(path).is_err(), "{path}");
    }
    assert!(
        capture::options_for_snapshot(&s.engine.store, &snapshot)
            .unwrap()
            .include_git_ignored
    );
    drop(scope);
    let value = symbols(&mut s);
    let items = value["items"].as_array().unwrap();
    assert_eq!(items.len(), 1, "{value}");
    assert_eq!(items[0]["name"], "ignored_nested");
    assert_eq!(value["symbolIndexScope"]["sourcesInScope"], 1);
    let mut e = Engine::open(r.path()).unwrap();
    e.capture_options.include_git_ignored = true;
    e.capture_options.source_exclusions = Some(yoke::source_exclusions::Policy::GitignoreV1 {
        groups: vec![yoke::source_exclusions::Group {
            patterns: vec!["ignored/**".into()],
            case_insensitive: false,
        }],
    });
    assert!(
        !e.index(&d())
            .unwrap()
            .0
            .files
            .iter()
            .any(|file| file.path.starts_with("ignored/"))
    );
}
#[test]
fn project_local_null_inheritance_independent_engines_and_stale_sessions_preserve_selection() {
    let primary = root();
    let secondary = root();
    let mut host = profile(
        json!({"gitIgnore":{"global":false,"projects":{"second":{"project":true,"local":null}}}}),
    );
    host.projects.push(HostProject {
        name: "second".into(),
        root: secondary.path().into(),
        read_only: false,
        query_only: false,
    });
    let mut s = open(primary.path(), host.clone(), "projects");
    let mut stale = open(primary.path(), host.clone(), "projects");
    assert!(
        s.engine
            .index(&d())
            .unwrap()
            .0
            .files
            .iter()
            .any(|file| file.path == "root-ignored.rs")
    );
    s.invoke("activate", &json!({"project":"second"}), &d())
        .unwrap();
    let value = symbols(&mut s);
    assert!(
        !value["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["name"] == "ignored_root")
    );
    assert_eq!(
        s.invoke("configuration", &json!({}), &d()).unwrap()["gitIgnoreResolved"],
        true
    );
    assert!(stale.invoke("configuration", &json!({}), &d()).is_err());
    drop(stale);
    drop(s);
    host.git_ignore
        .as_mut()
        .unwrap()
        .projects
        .get_mut("second")
        .unwrap()
        .local = Some(false);
    assert!(
        Session::open(
            Engine::open(primary.path()).unwrap(),
            host.clone(),
            Some("projects"),
            &d()
        )
        .is_err()
    );
    let mut changed = open(primary.path(), host, "new-projects");
    changed
        .invoke("activate", &json!({"project":"second"}), &d())
        .unwrap();
    assert!(
        symbols(&mut changed)["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["name"] == "ignored_root")
    );
}
#[test]
fn ignored_reads_stay_readonly_and_current_rules_refuse_cached_plans_before_journal() {
    let r = root();
    let mut old = open(r.path(), HostProfile::default(), "old");
    let plan = prepare(&mut old, "new.rs");
    changes::approve(&old.engine.store, &plan.id, false).unwrap();
    verify(&old.engine, &plan, "new.rs");
    let original = serde_json::to_vec(&plan).unwrap();
    drop(old);
    fs::write(
        r.path().join(".gitignore"),
        "ignored/\nroot-ignored.rs\nnew.rs\n",
    )
    .unwrap();
    let mut s = open(r.path(), selected(), "selected");
    let (snapshot, _, _, _) = s.engine.index(&d()).unwrap();
    let scope = capture::source_scope_policy(&s.engine.store, &snapshot).unwrap();
    for path in ["ignored/\u{e9}.rs", "root-ignored.rs"] {
        assert!(scope.check_host_write_scope(path).is_err(), "{path}");
    }
    scope.check_host_write_scope("allowed.rs").unwrap();
    drop(scope);
    for path in ["ignored/new.rs", "new.rs"] {
        let reply = s
            .invoke(
                "edit.prepare",
                &json!({"edits":[{"kind":"create","path":path,"text":"fn denied() {}\n"}]}),
                &d(),
            )
            .unwrap();
        assert_eq!(reply["status"], "error", "{reply}");
        assert!(
            reply
                .to_string()
                .contains("git_ignored_analysis_source_is_read_only"),
            "{reply}"
        );
    }
    assert_eq!(
        serde_json::to_vec(&changes::load_checked_plan(&s.engine.store, &plan.id, &d()).unwrap())
            .unwrap(),
        original
    );
    let before = s.engine.store.db.total_changes();
    let error = changes::apply(
        &s.engine.store,
        r.path(),
        &json!({"planId":plan.id,"idempotencyKey":"denied","mode":"workspace"}),
        true,
        &d(),
    )
    .err()
    .unwrap();
    assert!(
        error
            .to_string()
            .contains("git_ignored_analysis_source_is_read_only"),
        "{error}"
    );
    assert_eq!(s.engine.store.db.total_changes(), before);
    assert!(!r.path().join("new.rs").exists());
    assert_eq!(
        fs::read(r.path().join("ignored/\u{e9}.rs")).unwrap(),
        IGNORED.as_bytes()
    );
    let allowed = prepare(&mut s, "allowed.rs");
    changes::approve(&s.engine.store, &allowed.id, false).unwrap();
    verify(&s.engine, &allowed, "allowed.rs");
    let receipt = changes::apply(
        &s.engine.store,
        r.path(),
        &json!({"planId":allowed.id,"idempotencyKey":"allowed","mode":"workspace"}),
        true,
        &d(),
    )
    .unwrap();
    assert_eq!(
        fs::read(r.path().join("allowed.rs")).unwrap(),
        b"fn planned() {}\n"
    );
    changes::recover(&s.engine.store, r.path(), &receipt.id, true).unwrap();
    assert!(!r.path().join("allowed.rs").exists());
}
#[test]
fn types_paths_policy_bounds_and_incompatible_read_only_override_refuse_explicitly() {
    let r = root();
    for value in [
        json!(false),
        json!({"global":"false"}),
        json!({"global":0}),
        json!({"projects":{"unknown":{"local":false}}}),
        json!({"command":"owned"}),
    ] {
        if let Ok(host) = serde_json::from_value::<HostProfile>(json!({"gitIgnore":value})) {
            assert!(
                Session::open(Engine::open(r.path()).unwrap(), host, Some("bad"), &d()).is_err()
            );
        }
    }
    for path in ["../elsewhere", "/absolute", "C:/outside", "a/../b"] {
        assert!(
            CaptureOptions {
                include_git_ignored: true,
                only_dirs: vec![path.into()],
                ..Default::default()
            }
            .validated()
            .is_err()
        );
    }
    assert!(
        CaptureOptions {
            include_git_ignored: true,
            include_ignored_dirs: vec!["ignored".into()],
            ..Default::default()
        }
        .validated()
        .is_err()
    );
    let mut huge = yoke::git_ignore_policy::HostPolicy::default();
    for index in 0..102 {
        huge.projects
            .insert(format!("p{index}"), Default::default());
    }
    assert!(
        Session::open(
            Engine::open(r.path()).unwrap(),
            profile(json!({"gitIgnore":huge})),
            Some("huge"),
            &d()
        )
        .is_err()
    );
    let mut s = open(r.path(), selected(), "budget");
    s.engine.index(&d()).unwrap();
    fs::write(r.path().join(".gitignore"), "x".repeat(4097)).unwrap();
    let reply = s
        .invoke(
            "edit.prepare",
            &json!({"edits":[{"kind":"create","path":"new.rs","text":"fn owned() {}\n"}]}),
            &d(),
        )
        .unwrap();
    assert_eq!(reply["status"], "error", "{reply}");
    assert!(
        reply
            .to_string()
            .contains("git_ignore_write_policy_rule_capacity"),
        "{reply}"
    );
    assert!(!r.path().join("new.rs").exists());
}
#[test]
fn explicitly_trusted_lsp_can_read_selected_ignored_source_with_exact_original_anchor() {
    let r = root();
    let node = Command::new("node")
        .args(["-p", "process.execPath"])
        .output()
        .unwrap();
    assert!(node.status.success());
    let command = String::from_utf8(node.stdout).unwrap().trim().to_owned();
    let logs = tempfile::tempdir().unwrap();
    let log = logs.path().join("owned-log.jsonl");
    let mut e = Engine::open(r.path()).unwrap();
    e.register_configured_server(lsp::ConfiguredServer {
        server: lsp::Server {
            language: "rust".into(),
            command,
            args: vec![
                Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("tests/fixtures/server-selection-lsp.mjs")
                    .display()
                    .to_string(),
                log.display().to_string(),
                "owned".into(),
            ],
            trust: true,
        },
        configuration: lsp::HostConfiguration {
            passive_profile: Some(lsp::PassiveProfile::ContractFixture),
            ..Default::default()
        },
    })
    .unwrap();
    let mut s = Session::open(e, selected(), Some("provider"), &d()).unwrap();
    let reply=s.engine.invoke("ci_find",json!({"languageServer":{"path":"ignored/\u{e9}.rs","offset":IGNORED.find("fn").unwrap(),"operation":"hover"}})).unwrap();
    assert_eq!(reply["status"], "ok", "{reply}");
    let info = &reply["data"];
    assert_eq!(info["sourceExecutionAuthorized"], false);
    assert_eq!(info["contentHash"], yoke::model::hash(IGNORED.as_bytes()));
    assert_eq!(
        serde_json::from_str::<Value>(info["response"]["contents"]["value"].as_str().unwrap())
            .unwrap()["actor"],
        "owned"
    );
    assert_eq!(
        fs::read(r.path().join("ignored/\u{e9}.rs")).unwrap(),
        IGNORED.as_bytes()
    );
}

#[test]
fn pattern_search_defaults_follow_project_analysis_selection_and_explicit_skip_remains_available() {
    let primary = root();
    let secondary = root();
    let mut host =
        profile(json!({"gitIgnore":{"global":false,"projects":{"second":{"project":true}}}}));
    host.projects.push(HostProject {
        name: "second".into(),
        root: secondary.path().into(),
        read_only: true,
        query_only: false,
    });
    let mut s = open(primary.path(), host, "patterns");
    for code in [false, true] {
        let args =
            json!({"substring_pattern":"ignored_nested","restrict_search_to_code_files":code});
        let found =
            agent_repl::invoke_facade(&mut s, "fs.search_for_pattern", args.clone(), None, &d())
                .unwrap();
        assert_eq!(found["total"], 1, "{found}");
        let mut explicit = args;
        explicit["skip_ignored_files"] = json!(true);
        let filtered =
            agent_repl::invoke_facade(&mut s, "fs.search_for_pattern", explicit, None, &d())
                .unwrap();
        assert_eq!(filtered["total"], 0, "{filtered}");
    }
    s.invoke("activate", &json!({"project":"second"}), &d())
        .unwrap();
    let value = agent_repl::invoke_facade(
        &mut s,
        "fs.search_for_pattern",
        json!({"substring_pattern":"ignored_nested"}),
        None,
        &d(),
    )
    .unwrap();
    assert_eq!(value["total"], 0, "{value}");
}

#[test]
fn public_tools_external_queries_and_persistent_runtime_keep_implicit_selection_distinct() {
    let primary = root();
    let secondary = root();
    let mut host =
        profile(json!({"gitIgnore":{"global":true,"projects":{"second":{"local":false}}}}));
    host.projects.push(HostProject {
        name: "second".into(),
        root: secondary.path().into(),
        read_only: true,
        query_only: false,
    });
    let mut s = open(primary.path(), host, "public-patterns");
    let value = yoke::compatibility::invoke_serena(
        &mut s,
        "search_for_pattern",
        &json!({"substring_pattern":"ignored_nested"}),
        &d(),
    )
    .unwrap();
    assert_eq!(value["total"], 0, "{value}");
    let ordinary = yoke::compatibility::invoke_serena(
        &mut s,
        "search_for_pattern",
        &json!({"substring_pattern":"visible"}),
        &d(),
    )
    .unwrap();
    assert_eq!(ordinary["total"], 1, "{ordinary}");
    let nested = json!({"substring_pattern":"ignored_nested"});
    let value=yoke::compatibility::invoke_serena(&mut s,"query_project",&json!({"project_name":"second","tool_name":"search_for_pattern","tool_params_json":nested.to_string()}),&d()).unwrap();
    assert_eq!(value["total"], 1, "{value}");
    s.invoke("activate", &json!({"project":"second"}), &d())
        .unwrap();
    let value =
        yoke::compatibility::invoke_serena(&mut s, "search_for_pattern", &nested, &d()).unwrap();
    assert_eq!(value["total"], 1, "{value}");
    let python = Command::new("python")
        .args(["-I", "-c", "import sys;print(sys.executable)"])
        .output()
        .unwrap();
    assert!(python.status.success());
    let mut runtime = agent_repl::AgentRepl::spawn(
        agent_repl::ReplSpec {
            python: String::from_utf8(python.stdout).unwrap().trim().into(),
            trust_python: true,
            max_output_bytes: 100000,
            shell: None,
        },
        &d(),
    )
    .unwrap();
    let value = yoke::compatibility::invoke_serena_with_repl(
        &mut s,
        &mut runtime,
        "search_for_pattern",
        &nested,
        &d(),
    )
    .unwrap();
    assert_eq!(value["total"], 1, "{value}");
    let value = runtime
        .execute(
            &mut s,
            "s.fs.search_for_pattern('ignored_nested')['total']",
            &d(),
        )
        .unwrap();
    assert!(value["error"].is_null(), "{value}");
    assert_eq!(value["result"], 1, "{value}");
    let value = yoke::compatibility::invoke_serena_with_repl(
        &mut s,
        &mut runtime,
        "search_for_pattern",
        &json!({"substring_pattern":"ignored_nested","skip_ignored_files":true}),
        &d(),
    )
    .unwrap();
    assert_eq!(value["total"], 0, "{value}");
    runtime.close().unwrap();
}
