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
const SOURCE: &str = "// owned 🌍 e\u{301}\r\nfn helper() {}\r\nfn main() { helper(); }\r\n";
fn d() -> Deadline {
    Deadline::new(30000)
}
fn profile(v: Value) -> HostProfile {
    serde_json::from_value(v).unwrap()
}
fn source(parent: &Path, name: &str) -> PathBuf {
    let p = parent.join(name);
    fs::create_dir(&p).unwrap();
    fs::write(p.join("main.rs"), SOURCE).unwrap();
    p
}
fn engine(root: &Path, log: &Path, delay: u64, mode: &str) -> Engine {
    let mut e = Engine::open(root).unwrap();
    e.register_configured_server(yoke::lsp::ConfiguredServer {
        server: yoke::lsp::Server {
            command: "node".into(),
            args: vec![
                Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("tests/fixtures/symbol-info-budget-lsp.mjs")
                    .display()
                    .to_string(),
                log.display().to_string(),
                delay.to_string(),
                mode.into(),
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
fn session(root: &Path, log: &Path, p: HostProfile, id: &str) -> Session {
    Session::open(engine(root, log, 100, ""), p, Some(id), &d()).unwrap()
}
fn invoke(
    s: &mut Session,
    method: &str,
    args: Value,
    project: Option<&str>,
) -> anyhow::Result<Value> {
    agent_repl::invoke_facade(s, method, args, project, &d())
}
fn found(s: &mut Session, project: Option<&str>) -> Value {
    invoke(
        s,
        "lsp.find_symbol",
        json!({"name_path_pattern":"*","relative_path":"main.rs","include_info":true}),
        project,
    )
    .unwrap()
}
fn hovers(log: &Path) -> usize {
    fs::read_to_string(log)
        .unwrap_or_default()
        .lines()
        .filter_map(|l| serde_json::from_str::<Value>(l).ok())
        .filter(|v| v["method"] == "textDocument/hover")
        .count()
}
#[test]
fn absent_null_and_empty_keep_original_host_bindings_source_ids_and_raw_replies() {
    let r = tempfile::tempdir().unwrap();
    let root = source(r.path(), "source");
    let log = r.path().join("log");
    let base = HostProfile::default();
    let bytes = serde_json::to_vec(&base).unwrap();
    let mut original = session(&root, &log, base, "legacy");
    let config = original.invoke("configuration", &json!({}), &d()).unwrap();
    let (snapshot, _, _, _) = original.engine.index(&d()).unwrap();
    let reply = found(&mut original, None);
    assert!(reply.get("symbolInfo").is_none());
    assert_eq!(reply["items"].as_array().unwrap().len(), 2);
    for policy in [
        Value::Null,
        json!({}),
        json!({"globalMs":null,"projects":{"default":{"localMs":null}}}),
    ] {
        let host = profile(json!({"symbolInfoBudgets":policy}));
        assert_eq!(serde_json::to_vec(&host).unwrap(), bytes);
        let mut resumed = session(&root, &log, host, "legacy");
        assert_eq!(
            resumed.invoke("configuration", &json!({}), &d()).unwrap(),
            config
        );
        assert_eq!(resumed.engine.index(&d()).unwrap().0.id, snapshot.id);
        assert_eq!(
            serde_json::to_vec(&found(&mut resumed, None)).unwrap(),
            serde_json::to_vec(&reply).unwrap()
        );
    }
    assert_eq!(fs::read(root.join("main.rs")).unwrap(), SOURCE.as_bytes());
}
#[test]
fn delayed_hover_stops_only_supplementary_requests_and_zero_is_unlimited() {
    for (budget, expected) in [(1, 1), (0, 2), (600000, 2)] {
        let r = tempfile::tempdir().unwrap();
        let root = source(r.path(), "source");
        let log = r.path().join("log");
        let mut s = session(
            &root,
            &log,
            profile(json!({"symbolInfoBudgets":{"globalMs":budget}})),
            "selected",
        );
        let value = found(&mut s, None);
        let items = value["items"].as_array().unwrap();
        assert_eq!(items.len(), 2);
        assert_eq!(hovers(&log), expected);
        assert_eq!(value["symbolInfo"]["retrieved"], expected);
        assert_eq!(value["symbolInfo"]["omitted"], 2 - expected);
        assert_eq!(value["symbolInfo"]["earlyStopped"], expected == 1);
        assert_eq!(value["symbolInfo"]["inFlightRequestsInterrupted"], false);
        assert_eq!(
            items.iter().filter(|i| i.get("info").is_some()).count(),
            expected
        );
        for item in items {
            assert_eq!(item["contentHash"], yoke::model::hash(SOURCE));
            if item.get("info").is_none() {
                assert_eq!(item["infoOmitted"]["reason"], "symbol_info_budget_elapsed");
            }
        }
        let before = hovers(&log);
        let bodies=invoke(&mut s,"lsp.find_symbol",json!({"name_path_pattern":"*","relative_path":"main.rs","include_info":true,"include_body":true}),None).unwrap();
        assert_eq!(hovers(&log), before);
        assert!(bodies.get("symbolInfo").is_none());
        assert!(
            bodies["items"]
                .as_array()
                .unwrap()
                .iter()
                .all(|i| i["body"].is_string() && i.get("info").is_none())
        );
        let jb = invoke(
            &mut s,
            "jb.find_symbol",
            json!({"name_path_pattern":"h*","relative_path":"main.rs","include_info":true}),
            None,
        )
        .unwrap();
        assert!(jb.get("symbolInfo").is_none());
        assert_eq!(hovers(&log), before + 1);
        assert_eq!(fs::read(root.join("main.rs")).unwrap(), SOURCE.as_bytes());
    }
}
#[test]
fn implementation_metadata_is_partial_but_provider_failures_are_not_budget_success() {
    let r = tempfile::tempdir().unwrap();
    let root = source(r.path(), "source");
    let log = r.path().join("log");
    let host = profile(json!({"symbolInfoBudgets":{"globalMs":1}}));
    let mut s = session(&root, &log, host.clone(), "implementations");
    let v = invoke(
        &mut s,
        "lsp.find_implementations",
        json!({"name_path":"helper","relative_path":"main.rs","include_info":true}),
        None,
    )
    .unwrap();
    assert_eq!(v["locations"].as_array().unwrap().len(), 2);
    assert_eq!(v["symbolInfo"]["retrieved"], 1);
    assert_eq!(v["symbolInfo"]["omitted"], 1);
    assert_eq!(hovers(&log), 1);
    let v = invoke(
        &mut s,
        "lsp.find_implementations",
        json!({"name_path":"helper","relative_path":"main.rs","include_info":false}),
        None,
    )
    .unwrap();
    assert_eq!(hovers(&log), 1);
    assert!(v.get("symbolInfo").is_none());
    let mut broken = Session::open(
        engine(&root, &log, 100, "error"),
        host,
        Some("provider-error"),
        &d(),
    )
    .unwrap();
    let err = invoke(
        &mut broken,
        "lsp.find_symbol",
        json!({"name_path_pattern":"*","relative_path":"main.rs","include_info":true}),
        None,
    )
    .unwrap_err();
    assert!(
        format!("{err:#}").contains("Owned hover refusal"),
        "{err:#}"
    );
}
#[test]
fn secondary_projects_resolve_their_own_overrides_and_activation_uses_current_project() {
    let r = tempfile::tempdir().unwrap();
    let root = source(r.path(), "primary");
    let other = source(r.path(), "other");
    let log = r.path().join("log");
    let mut host = profile(
        json!({"context":"desktop-app","modes":["editing","interactive","query-projects"],
        "symbolInfoBudgets":{"globalMs":600000,"projects":{"default":{"projectMs":1},"other":{"localMs":0}}}}),
    );
    host.projects.push(HostProject {
        name: "other".into(),
        root: other.clone(),
        read_only: true,
        query_only: false,
    });
    let mut s = session(&root, &log, host, "projects");
    assert_eq!(found(&mut s, None)["symbolInfo"]["retrieved"], 1);
    let v = found(&mut s, Some("other"));
    assert_eq!(v["symbolInfo"]["retrieved"], 2);
    assert_eq!(v["symbolInfo"]["budgetMs"], 0);
    s.invoke("activate", &json!({"project":"other"}), &d())
        .unwrap();
    assert_eq!(
        s.invoke("configuration", &json!({}), &d()).unwrap()["symbolInfoBudgetMs"],
        0
    );
    assert_eq!(found(&mut s, None)["symbolInfo"]["retrieved"], 2);
    s.invoke("activate", &json!({"project":"default"}), &d())
        .unwrap();
    assert_eq!(found(&mut s, None)["symbolInfo"]["retrieved"], 1);
    assert_eq!(fs::read(root.join("main.rs")).unwrap(), SOURCE.as_bytes());
    assert_eq!(fs::read(other.join("main.rs")).unwrap(), SOURCE.as_bytes());
}
#[test]
fn invalid_profiles_refuse_and_selected_host_bindings_cannot_upgrade_or_downgrade() {
    for v in [
        json!({"globalMs":-1}),
        json!({"globalMs":600001}),
        json!({"globalMs":0.5}),
        json!({"globalMs":false}),
        json!({"globalMs":1,"trust":true}),
    ] {
        let decoded = serde_json::from_value::<HostProfile>(json!({"symbolInfoBudgets":v}));
        if let Ok(host) = decoded {
            let r = tempfile::tempdir().unwrap();
            let root = source(r.path(), "source");
            assert!(
                Session::open(
                    engine(&root, &r.path().join("log"), 0, ""),
                    host,
                    Some("invalid"),
                    &d()
                )
                .is_err()
            );
        }
    }
    let r = tempfile::tempdir().unwrap();
    let root = source(r.path(), "source");
    let log = r.path().join("log");
    let host = profile(json!({"symbolInfoBudgets":{"globalMs":1}}));
    let mut original = session(&root, &log, host.clone(), "selected");
    let config = original.invoke("configuration", &json!({}), &d()).unwrap();
    let mut resumed = session(&root, &log, host, "selected");
    assert_eq!(
        resumed.invoke("configuration", &json!({}), &d()).unwrap(),
        config
    );
    for value in [
        Value::Null,
        json!({"globalMs":0}),
        json!({"projects":{"unregistered":{}}}),
    ] {
        let host = profile(json!({"symbolInfoBudgets":value}));
        assert!(Session::open(engine(&root, &log, 100, ""), host, Some("selected"), &d()).is_err());
    }
    let _legacy = session(&root, &log, HostProfile::default(), "legacy");
    assert!(
        Session::open(
            engine(&root, &log, 100, ""),
            profile(json!({"symbolInfoBudgets":{"globalMs":1}})),
            Some("legacy"),
            &d()
        )
        .is_err()
    );
    assert_eq!(hovers(&log), 0);
}
fn python() -> agent_repl::ReplSpec {
    let output = std::process::Command::new("python")
        .args(["-I", "-c", "import sys;print(sys.executable)"])
        .output()
        .unwrap();
    assert!(output.status.success());
    agent_repl::ReplSpec {
        python: PathBuf::from(String::from_utf8(output.stdout).unwrap().trim()),
        trust_python: true,
        max_output_bytes: 100000,
        shell: None,
    }
}
#[test]
fn persistent_declaration_bridge_preserves_lsp_and_jb_origins_and_actual_hover() {
    let r = tempfile::tempdir().unwrap();
    let root = source(r.path(), "source");
    let log = r.path().join("log");
    let mut s = session(
        &root,
        &log,
        profile(json!({"symbolInfoBudgets":{"globalMs":1}})),
        "declarations",
    );
    let mut worker = agent_repl::AgentRepl::spawn(python(), &d()).unwrap();
    let code = "a=s.lsp.find_declaration('main.rs',r'(?<=\\{ )(helper)(?=\\(\\))',include_info=True)\nb=s.jb.find_declaration('main.rs',r'(?<=\\{ )(helper)(?=\\(\\))')\n(a['symbolInfo']['retrieved'],a['info']['response']['contents']['value'],'symbolInfo' in b,'info' in b)";
    let value = worker.execute(&mut s, code, &d()).unwrap();
    assert_eq!(
        value["result"],
        json!([1, "Owned supplementary information 🧭", false, false]),
        "{value}"
    );
    assert_eq!(hovers(&log), 1);
    worker.close().unwrap();
}
#[test]
fn stale_sessions_and_host_cancellation_do_not_become_partial_information_success() {
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };
    let r = tempfile::tempdir().unwrap();
    let root = source(r.path(), "source");
    let log = r.path().join("log");
    let host = profile(json!({"symbolInfoBudgets":{"globalMs":1}}));
    let mut old = session(&root, &log, host.clone(), "stale");
    let mut current = session(&root, &log, host.clone(), "stale");
    current
        .invoke(
            "switchModes",
            &json!({"modes":["interactive","planning"]}),
            &d(),
        )
        .unwrap();
    assert!(
        invoke(
            &mut old,
            "lsp.find_symbol",
            json!({"name_path_pattern":"*","include_info":true}),
            None
        )
        .is_err()
    );
    assert_eq!(hovers(&log), 0);
    let mut s = Session::open(engine(&root, &log, 15000, ""), host, Some("cancel"), &d()).unwrap();
    let cancel = Arc::new(AtomicBool::new(false));
    let flag = Arc::clone(&cancel);
    let watched = log.clone();
    let observer = std::thread::spawn(move || {
        let end = std::time::Instant::now() + std::time::Duration::from_secs(15);
        while std::time::Instant::now() < end {
            if hovers(&watched) > 0 {
                let now = std::time::Instant::now();
                flag.store(true, Ordering::SeqCst);
                return Some(now);
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        flag.store(true, Ordering::SeqCst);
        None
    });
    let result = Deadline::run_bounded(&d(), cancel, || {
        invoke(
            &mut s,
            "lsp.find_symbol",
            json!({"name_path_pattern":"*","relative_path":"main.rs","include_info":true}),
            None,
        )
    });
    let observed = observer
        .join()
        .unwrap()
        .expect("Owned hover must actually start before cancellation");
    assert!(observed.elapsed() < std::time::Duration::from_secs(8));
    let error = result.unwrap_err();
    assert!(
        format!("{error:#}").to_lowercase().contains("cancel"),
        "{error:#}"
    );
    assert_eq!(hovers(&log), 1);
}
