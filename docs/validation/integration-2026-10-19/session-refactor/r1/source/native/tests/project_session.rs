use serde_json::json;
use std::fs;
use tempfile::TempDir;
#[test]
fn external_queries_cannot_publish_semantic_plans_through_find() {
    let root = source("primary");
    let secondary = TempDir::new().unwrap();
    fs::write(
        secondary.path().join("main.rs"),
        "// 🌍\nfn helper() {}\nfn main() { helper(); }\n",
    )
    .unwrap();
    let mut engine = Engine::open(root.path()).unwrap();
    engine
        .register_configured_server(yoke::lsp::ConfiguredServer {
            server: yoke::lsp::Server {
                command: "node".into(),
                args: vec![
                    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
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
    let host = HostProfile {
        projects: vec![HostProject {
            name: "reference".into(),
            root: secondary.path().to_path_buf(),
            read_only: true,
            query_only: true,
        }],
        ..Default::default()
    };
    let deadline = Deadline::new(20000);
    let mut session = Session::open(engine, host, Some("no-query-plans"), &deadline).unwrap();
    for operation in ["codeActions", "willRenameFiles"] {
        assert!(session.invoke("queryProject",&json!({"project":"reference","tool":"ci_find","arguments":{"languageServer":{"path":"main.rs","offset":11,"operation":operation}}}),&deadline).is_err(),"External query accepted plan operation {operation}");
    }
}
#[test]
fn global_note_host_identity_is_bound_to_canonical_directory() {
    let root = source("primary");
    let global = TempDir::new().unwrap();
    let d = Deadline::new(20000);
    let lexical = HostProfile {
        global_notes_root: Some(global.path().join(".")),
        ..Default::default()
    };
    let mut first = Session::open(
        Engine::open(root.path()).unwrap(),
        lexical,
        Some("canonical-global"),
        &d,
    )
    .unwrap();
    first
        .invoke(
            "notes.write",
            &json!({"scope":"global","name":"choice","content":"Rust"}),
            &d,
        )
        .unwrap();
    drop(first);
    let canonical = HostProfile {
        global_notes_root: Some(global.path().canonicalize().unwrap()),
        ..Default::default()
    };
    let mut resumed = Session::open(
        Engine::open(root.path()).unwrap(),
        canonical,
        Some("canonical-global"),
        &d,
    )
    .unwrap();
    let note = resumed
        .invoke("notes.read", &json!({"scope":"global","name":"choice"}), &d)
        .unwrap();
    assert_eq!(note["note"]["content"], "Rust");
}
#[test]
fn registered_projects_inherit_explicit_host_capture_and_provider_defaults() {
    let a = source("alpha");
    let b = source("beta");
    let d = Deadline::new(20000);
    fs::write(b.path().join("extension.mine"), "fn extended() {}\n").unwrap();
    let mut engine = Engine::open(a.path()).unwrap();
    engine
        .capture_options
        .language_overrides
        .insert("mine".into(), "rust".into());
    engine
        .register_server(yoke::lsp::Server {
            command: "node".into(),
            args: vec![],
            language: "rust".into(),
            trust: true,
        })
        .unwrap();
    let host = HostProfile {
        projects: vec![HostProject {
            name: "secondary".into(),
            root: b.path().to_path_buf(),
            read_only: true,
            query_only: false,
        }],
        ..Default::default()
    };
    let mut session = Session::open(engine, host, Some("defaults"), &d).unwrap();
    session
        .invoke("activate", &json!({"project":"secondary"}), &d)
        .unwrap();
    let engine = session.active_engine_at(&d).unwrap();
    assert!(engine.servers.contains_key("rust"));
    let (snapshot, _, _, _) = engine.index(&d).unwrap();
    assert_eq!(
        snapshot
            .files
            .iter()
            .find(|f| f.path == "extension.mine")
            .unwrap()
            .language,
        "rust"
    );
    let list = session
        .invoke("fs.list", &json!({"relativePath":"."}), &d)
        .unwrap();
    assert_eq!(list["files"].as_array().unwrap().len(), 2);
}
use yoke::{
    Engine,
    capture::Deadline,
    project_session::{HostProfile, HostProject, Session},
};
fn source(name: &str) -> TempDir {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("main.rs"), format!("fn {name}() {{}}\n")).unwrap();
    dir
}
#[test]
fn session_activation_and_resume_use_only_host_registered_projects() {
    let a = source("alpha");
    let b = source("beta");
    let d = Deadline::new(20000);
    let host = HostProfile {
        projects: vec![HostProject {
            name: "secondary".into(),
            root: b.path().to_path_buf(),
            read_only: true,
            query_only: false,
        }],
        ..Default::default()
    };
    let mut session = Session::open(
        Engine::open(a.path()).unwrap(),
        host.clone(),
        Some("owned-session"),
        &d,
    )
    .unwrap();
    let activated = session
        .invoke("activate", &json!({"project":"secondary"}), &d)
        .unwrap();
    assert_eq!(activated["activeProject"], "secondary");
    assert!(
        session
            .invoke(
                "activate",
                &json!({"project":a.path().display().to_string()}),
                &d
            )
            .is_err()
    );
    drop(session);
    let mut resumed = Session::open(
        Engine::open(a.path()).unwrap(),
        host,
        Some("owned-session"),
        &d,
    )
    .unwrap();
    let config = resumed.invoke("configuration", &json!({}), &d).unwrap();
    assert_eq!(config["activeProject"], "secondary");
    let read = resumed
        .invoke("fs.read", &json!({"path":"main.rs"}), &d)
        .unwrap();
    assert!(read["data"]["text"].as_str().unwrap().contains("beta"));
    assert!(!resumed.active_engine().unwrap().allow_writes);
}
#[test]
fn modes_onboarding_and_named_notes_are_persistent_without_granting_execution() {
    let a = source("alpha");
    let d = Deadline::new(20000);
    let mut session = Session::open(
        Engine::open(a.path()).unwrap(),
        HostProfile::default(),
        Some("onboarding"),
        &d,
    )
    .unwrap();
    assert_eq!(
        session.invoke("checkOnboarding", &json!({}), &d).unwrap()["onboarded"],
        false
    );
    let onboard = session.invoke("onboarding", &json!({}), &d).unwrap();
    assert_eq!(onboard["onboarded"], true);
    session
        .invoke(
            "notes.write",
            &json!({"name":"design","content":"Use Rust","paths":["main.rs"]}),
            &d,
        )
        .unwrap();
    session
        .invoke(
            "switchModes",
            &json!({"modes":["planning","no-memories"]}),
            &d,
        )
        .unwrap();
    assert!(
        session
            .invoke("notes.read", &json!({"name":"design"}), &d)
            .is_err()
    );
    assert!(
        session
            .invoke(
                "edit.prepare",
                &json!({"edits":[{"kind":"create","path":"bad.rs","text":""}]}),
                &d
            )
            .is_err()
    );
    assert!(
        session
            .invoke(
                "switchModes",
                &json!({"modes":["editing"],"trust":true}),
                &d
            )
            .is_err()
    );
    assert!(
        session
            .invoke("shell.execute", &json!({"command":"node"}), &d)
            .is_err()
    );
    assert_eq!(
        fs::read_to_string(a.path().join("main.rs")).unwrap(),
        "fn alpha() {}\n"
    );
}
#[test]
fn external_project_queries_cannot_mutate_or_activate_query_only_roots() {
    let a = source("alpha");
    let b = source("beta");
    let d = Deadline::new(20000);
    let host = HostProfile {
        projects: vec![HostProject {
            name: "library".into(),
            root: b.path().to_path_buf(),
            read_only: true,
            query_only: true,
        }],
        ..Default::default()
    };
    let mut session = Session::open(Engine::open(a.path()).unwrap(), host, None, &d).unwrap();
    let found = session
        .invoke(
            "queryProject",
            &json!({"project":"library","tool":"ci_find","arguments":{"query":"beta"}}),
            &d,
        )
        .unwrap();
    assert_eq!(found["data"]["items"][0]["name"], "beta");
    assert!(
        session
            .invoke(
                "queryProject",
                &json!({"project":"library","tool":"ci_change_prepare","arguments":{"edits":[]}}),
                &d
            )
            .is_err()
    );
    assert!(
        session
            .invoke("activate", &json!({"project":"library"}), &d)
            .is_err()
    );
    assert_eq!(
        session.invoke("configuration", &json!({}), &d).unwrap()["activeProject"],
        "default"
    );
}
