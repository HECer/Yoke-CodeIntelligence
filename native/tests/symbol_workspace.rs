use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
};
use yoke::{
    Engine, agent_repl,
    capture::Deadline,
    project_session::{HostProfile, HostProject, Session},
};
const SOURCE: &str = "// owned 🧭 e\u{301}\r\nfn first() {}\r\nfn second() {}\r\n";
fn d() -> Deadline {
    Deadline::new(30000)
}
fn profile(v: Value) -> HostProfile {
    serde_json::from_value(v).unwrap()
}
fn source(parent: &Path, name: &str) -> PathBuf {
    let p = parent.join(name);
    fs::create_dir(&p).unwrap();
    for name in ["src", "src-other", "é", "ignored"] {
        fs::create_dir(p.join(name)).unwrap();
        fs::write(p.join(name).join("main.rs"), SOURCE).unwrap();
    }
    fs::write(p.join("root.rs"), SOURCE).unwrap();
    fs::write(p.join(".gitignore"), "ignored/\n").unwrap();
    fs::write(
        p.join("project-config.txt"),
        "owned root configuration 🧭\r\n",
    )
    .unwrap();
    p
}
fn engine(root: &Path, log: &Path) -> Engine {
    let mut e = Engine::open(root).unwrap();
    e.allow_writes = true;
    e.register_configured_server(yoke::lsp::ConfiguredServer {
        server: yoke::lsp::Server {
            command: "node".into(),
            args: vec![
                Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("tests/fixtures/symbol-workspace-lsp.mjs")
                    .display()
                    .to_string(),
                log.display().to_string(),
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
    e
}
fn open(root: &Path, log: &Path, p: HostProfile, id: &str) -> Session {
    Session::open(engine(root, log), p, Some(id), &d()).unwrap()
}
fn call(s: &mut Session, op: &str, args: Value) -> Value {
    s.invoke(op, &args, &d()).unwrap()
}
fn data(v: Value) -> Value {
    assert_eq!(v["status"], "ok", "{v}");
    v["data"].clone()
}
fn find(s: &mut Session, args: Value, project: Option<&str>) -> Value {
    agent_repl::invoke_facade(s, "lsp.find_symbol", args, project, &d()).unwrap()
}
#[test]
fn absent_null_and_empty_policy_keep_raw_replies_bindings_and_immutable_source_graphs() {
    let r = tempfile::tempdir().unwrap();
    let root = source(r.path(), "source");
    let log = r.path().join("log");
    let base = HostProfile::default();
    let bytes = serde_json::to_vec(&base).unwrap();
    let mut s = open(&root, &log, base, "legacy");
    let config = call(&mut s, "configuration", json!({}));
    let (snapshot, graph, _, _) = s.engine.index(&d()).unwrap();
    let args = json!({"name_path_pattern":"*"});
    let old = find(&mut s, args.clone(), None);
    for policy in [
        Value::Null,
        json!({}),
        json!({"global":null,"projects":{"default":{"local":null}}}),
    ] {
        let p = profile(json!({"symbolWorkspaceFolders":policy}));
        assert_eq!(serde_json::to_vec(&p).unwrap(), bytes);
        let mut resumed = open(&root, &log, p, "legacy");
        assert_eq!(call(&mut resumed, "configuration", json!({})), config);
        assert_eq!(
            serde_json::to_vec(&find(&mut resumed, args.clone(), None)).unwrap(),
            serde_json::to_vec(&old).unwrap()
        );
        let (new_snapshot, new_graph, _, _) = resumed.engine.index(&d()).unwrap();
        assert_eq!(
            serde_json::to_vec(&new_snapshot).unwrap(),
            serde_json::to_vec(&snapshot).unwrap()
        );
        assert_eq!(
            serde_json::to_vec(&new_graph).unwrap(),
            serde_json::to_vec(&graph).unwrap()
        );
    }
    assert!(!log.exists());
    assert_eq!(
        fs::read(root.join("src/main.rs")).unwrap(),
        SOURCE.as_bytes()
    );
}
#[test]
fn symbol_index_filters_before_pagination_threshold_body_search_and_descendants_without_narrowing_reads_or_edits()
 {
    let r = tempfile::tempdir().unwrap();
    let root = source(r.path(), "source");
    let log = r.path().join("log");
    let mut plain = open(&root, &log, HostProfile::default(), "plain");
    let (before, graph, _, _) = plain.engine.index(&d()).unwrap();
    let mut s = open(
        &root,
        &log,
        profile(json!({"symbolWorkspaceFolders":{"global":["./é"]}})),
        "scoped",
    );
    let found = find(
        &mut s,
        json!({"name_path_pattern":"*","max_matches":2,"depth":2,"include_body":true}),
        None,
    );
    assert_eq!(found["items"].as_array().unwrap().len(), 2);
    assert_eq!(found["symbolIndexScope"]["folders"], json!(["é"]));
    assert_eq!(
        found["symbolIndexScope"]["baseGraphHash"],
        yoke::model::identity(&graph)
    );
    for item in found["items"].as_array().unwrap() {
        assert_eq!(item["path"], "é/main.rs");
        assert_eq!(item["contentHash"], yoke::model::hash(SOURCE));
        assert!(item["body"].is_string());
    }
    let page = data(call(
        &mut s,
        "symbols.find",
        json!({"namePath":"*","limit":1,"offset":1}),
    ));
    assert_eq!(page["total"], 2);
    assert_eq!(page["items"].as_array().unwrap().len(), 1);
    let body = data(call(
        &mut s,
        "symbols.find",
        json!({"query":"second","searchBody":true}),
    ));
    assert!(
        body["items"]
            .as_array()
            .unwrap()
            .iter()
            .all(|v| v["path"] == "é/main.rs")
    );
    let read = data(call(&mut s, "fs.read", json!({"path":"src-other/main.rs"})));
    assert_eq!(read["contentHash"], yoke::model::hash(SOURCE));
    let files = call(&mut s, "fs.list", json!({"recursive":true}));
    assert!(
        files["files"]
            .as_array()
            .unwrap()
            .iter()
            .any(|v| v["path"] == "src-other/main.rs")
    );
    let prepared = data(call(
        &mut s,
        "edit.prepare",
        json!({"edits":[{"kind":"create","path":"src-other/planned.rs","text":"fn planned() {}\n"}]}),
    ));
    assert!(prepared["plan"]["id"].is_string());
    assert!(!root.join("src-other/planned.rs").exists());
    let (after, new_graph, _, _) = s.engine.index(&d()).unwrap();
    assert_eq!(before.id, after.id);
    assert_eq!(
        yoke::model::identity(&new_graph),
        yoke::model::identity(&graph)
    );
    assert!(!log.exists());
}
#[test]
fn actual_provider_receives_selected_private_roots_and_full_root_configuration_even_for_direct_outside_hover()
 {
    let r = tempfile::tempdir().unwrap();
    let root = source(r.path(), "source");
    let log = r.path().join("log");
    let mut s = open(
        &root,
        &log,
        profile(json!({"symbolWorkspaceFolders":{"global":["src","é"]}})),
        "actual",
    );
    let value = s
        .engine
        .invoke(
            "ci_find",
            json!({"languageServer":{"path":"src-other/main.rs","offset":29,"operation":"hover"}}),
        )
        .unwrap();
    assert_eq!(value["status"], "ok", "{value}");
    let rows = fs::read_to_string(&log).unwrap();
    let observed: Value = serde_json::from_str(rows.lines().next().unwrap()).unwrap();
    assert_eq!(observed["folders"], json!(["src", "é"]));
    assert_eq!(observed["callbackVerified"], true);
    assert_eq!(observed["rootConfig"], "owned root configuration 🧭\r\n");
    assert_eq!(fs::read(root.join("root.rs")).unwrap(), SOURCE.as_bytes());
}
#[test]
fn named_projects_keep_independent_roots_activation_and_changed_host_bindings_refuse() {
    let r = tempfile::tempdir().unwrap();
    let root = source(r.path(), "primary");
    let secondary = source(r.path(), "secondary");
    let log = r.path().join("log");
    let mut p = profile(
        json!({"symbolWorkspaceFolders":{"global":["src"],"projects":{"second":{"project":["src-other"],"local":["é"]}}}}),
    );
    p.projects.push(HostProject {
        name: "second".into(),
        root: secondary,
        read_only: true,
        query_only: false,
    });
    let mut s = open(&root, &log, p.clone(), "projects");
    let args = json!({"name_path_pattern":"*"});
    assert_eq!(
        find(&mut s, args.clone(), None)["symbolIndexScope"]["folders"],
        json!(["src"])
    );
    assert_eq!(
        find(&mut s, args.clone(), Some("second"))["symbolIndexScope"]["folders"],
        json!(["é"])
    );
    let mut stale = open(&root, &log, p.clone(), "projects");
    call(&mut s, "activate", json!({"project":"second"}));
    assert!(stale.invoke("symbols.find", &json!({}), &d()).is_err());
    assert_eq!(
        find(&mut s, args, None)["symbolIndexScope"]["folders"],
        json!(["é"])
    );
    p.symbol_workspace_folders.as_mut().unwrap().global = Some(vec![".".into()]);
    assert!(Session::open(engine(&root, &log), p, Some("projects"), &d()).is_err());
    assert!(
        Session::open(
            engine(&root, &log),
            HostProfile::default(),
            Some("projects"),
            &d()
        )
        .is_err()
    );
    assert!(!log.exists());
}
#[test]
fn missing_ignored_and_file_roots_refuse_before_provider_or_silent_empty_results() {
    let r = tempfile::tempdir().unwrap();
    let root = source(r.path(), "source");
    let log = r.path().join("log");
    for folder in ["missing", "ignored", "src/main.rs"] {
        let mut s = open(
            &root,
            &log,
            profile(json!({"symbolWorkspaceFolders":{"global":[folder]}})),
            folder.replace('/', "_").as_str(),
        );
        let v = call(&mut s, "symbols.find", json!({}));
        assert_eq!(v["status"], "error", "{folder}: {v}");
        let v = s
            .engine
            .invoke(
                "ci_find",
                json!({"languageServer":{"path":"src/main.rs","offset":29,"operation":"hover"}}),
            )
            .unwrap();
        assert_eq!(v["status"], "error", "{v}");
        assert!(!log.exists());
    }
}
#[test]
fn strict_host_paths_schema_and_capacity_refuse_without_touching_source() {
    let r = tempfile::tempdir().unwrap();
    let root = source(r.path(), "source");
    let log = r.path().join("log");
    for value in [
        json!([]),
        json!(["../outside"]),
        json!(["C:/outside"]),
        json!([".git"]),
        json!([".env"]),
        json!(["src\\child"]),
        json!(["src", "./src"]),
        json!(["src//child"]),
        json!(["src\0"]),
        json!(false),
        json!([42]),
        json!(vec!["src"; 33]),
    ] {
        let parsed = serde_json::from_value::<HostProfile>(
            json!({"symbolWorkspaceFolders":{"global":value}}),
        );
        if let Ok(p) = parsed {
            assert!(Session::open(engine(&root, &log), p, Some("bad"), &d()).is_err());
        }
    }
    for policy in [
        json!({"unknown":[]}),
        json!({"projects":{"not-registered":{"local":["src"]}}}),
    ] {
        if let Ok(p) =
            serde_json::from_value::<HostProfile>(json!({"symbolWorkspaceFolders":policy}))
        {
            assert!(Session::open(engine(&root, &log), p, Some("bad"), &d()).is_err());
        }
    }
    assert!(!log.exists());
    assert_eq!(
        fs::read(root.join("src/main.rs")).unwrap(),
        SOURCE.as_bytes()
    );
}

#[test]
fn invalid_provider_callback_refuses_and_workspace_selection_does_not_grant_source_execution() {
    let r = tempfile::tempdir().unwrap();
    let root = source(r.path(), "source");
    let log = r.path().join("log");
    let p = profile(json!({"symbolWorkspaceFolders":{"global":["src"]}}));
    let mut e = engine(&root, &log);
    e.servers
        .get_mut("rust")
        .unwrap()
        .args
        .push("bad-callback".into());
    let mut s = Session::open(e, p.clone(), Some("callback"), &d()).unwrap();
    let request = json!({"languageServer":{"path":"src/main.rs","offset":20,"operation":"hover"}});
    let result = s.engine.invoke("ci_find", request.clone()).unwrap();
    assert_eq!(result["status"], "error", "{result}");
    assert!(
        result
            .to_string()
            .contains("Invalid workspace folders callback parameters")
    );
    let mut e = engine(&root, &log);
    e.server_configurations.clear();
    let mut s = Session::open(e, p, Some("no-runtime-grant"), &d()).unwrap();
    let result = s.engine.invoke("ci_find", request).unwrap();
    assert_eq!(result["status"], "error", "{result}");
    assert!(result.to_string().contains("Passive provider unavailable"));
    assert!(!log.exists());
    assert_eq!(
        fs::read(root.join("src/main.rs")).unwrap(),
        SOURCE.as_bytes()
    );
}

#[test]
fn aggregate_path_and_policy_byte_budgets_refuse_before_capturing_source() {
    let r = tempfile::tempdir().unwrap();
    let root = source(r.path(), "source");
    let log = r.path().join("log");
    let long_roots: Vec<_> = (0..32)
        .map(|i| format!("{}-{i}", "x".repeat(4090)))
        .collect();
    let medium_roots: Vec<_> = (0..32)
        .map(|i| format!("{}-{i}", "x".repeat(2000)))
        .collect();
    for policy in [
        json!({"global":long_roots}),
        json!({"global":medium_roots,"projects":{"default":{"project":medium_roots}}}),
    ] {
        let p = profile(json!({"symbolWorkspaceFolders":policy}));
        assert!(Session::open(engine(&root, &log), p, Some("too-large"), &d()).is_err());
    }
    assert!(!log.exists());
    assert_eq!(
        fs::read(root.join("src/main.rs")).unwrap(),
        SOURCE.as_bytes()
    );
}

#[test]
fn explicitly_captured_ignored_sources_remain_readable_without_becoming_index_roots_or_symbols() {
    let r = tempfile::tempdir().unwrap();
    let root = source(r.path(), "source");
    let log = r.path().join("log");
    for (id, folders) in [
        ("ignored-only", json!(["ignored"])),
        ("whole-root", json!(["."])),
    ] {
        let mut e = engine(&root, &log);
        e.capture_options
            .include_ignored_dirs
            .push("ignored".into());
        let p = profile(json!({"symbolWorkspaceFolders":{"global":folders}}));
        let mut s = Session::open(e, p, Some(id), &d()).unwrap();
        let (snapshot, _, _, _) = s.engine.index(&d()).unwrap();
        assert!(snapshot.files.iter().any(|f| f.path == "ignored/main.rs"));
        let read = data(call(&mut s, "fs.read", json!({"path":"ignored/main.rs"})));
        assert_eq!(read["contentHash"], yoke::model::hash(SOURCE));
        let found = call(&mut s, "symbols.find", json!({"namePath":"*"}));
        if id == "ignored-only" {
            assert_eq!(found["status"], "error", "{found}");
        } else {
            let found = data(found);
            assert_eq!(found["items"].as_array().unwrap().len(), 8);
            assert!(
                found["items"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .all(|i| i["path"] != "ignored/main.rs")
            );
            assert_eq!(
                found["symbolIndexScope"]["sourcesInScope"]
                    .as_u64()
                    .unwrap(),
                snapshot.files.len() as u64 - 1
            );
        }
    }
    assert!(!log.exists());
}
