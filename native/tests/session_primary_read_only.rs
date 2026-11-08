use serde_json::{Value, json};
use std::{fs, path::Path, process::Command};
use tempfile::TempDir;
use yoke::{
    Engine,
    capture::Deadline,
    project_session::{HostProfile, HostProject, Session},
    protocol::{ConnectionState, Profile},
};
fn owned_root() -> TempDir {
    let root = TempDir::new().unwrap();
    fs::write(
        root.path().join("main.rs"),
        "// owned \u{1f4a1} e\u{301}\r\nfn source() {}\r\n",
    )
    .unwrap();
    root
}
fn deadline() -> Deadline {
    Deadline::new(30000)
}
fn engine(path: &Path, writes: bool) -> Engine {
    let mut engine = Engine::open(path).unwrap();
    engine.allow_writes = writes;
    engine
}
fn open(root: &TempDir, profile: HostProfile, id: &str, writes: bool) -> Session {
    Session::open(engine(root.path(), writes), profile, Some(id), &deadline()).unwrap()
}
fn profile(read_only: bool) -> HostProfile {
    HostProfile {
        primary_read_only: read_only,
        ..Default::default()
    }
}
fn edit() -> Value {
    json!({"edits":[{"kind":"create","path":"planned.rs","text":"fn planned() {}\n"}]})
}
fn message(
    connection: &mut ConnectionState,
    session: &mut Session,
    profile: Profile,
    method: &str,
    params: Value,
) -> Value {
    connection
        .message_session(
            session,
            profile,
            &serde_json::to_vec(&json!({"jsonrpc":"2.0","id":1,"method":method,"params":params}))
                .unwrap(),
        )
        .unwrap()
        .unwrap()
}
fn changes(session: &Session) -> u64 {
    session.engine.store.db.total_changes()
}
fn verify_plan(engine: &Engine, id: &str) {
    let args=vec!["-e".into(),"const fs=require('node:fs');process.exit(fs.readFileSync('planned.rs','utf8')==='fn planned() {}\\n'?0:1)".into()];
    let result = yoke::verify::execute(engine, id, "node", &args, true, &deadline()).unwrap();
    assert_eq!(result["status"], "passed", "{result}");
}

#[test]
fn absent_and_explicit_false_keep_profile_bytes_existing_binding_and_preparation() {
    let root = owned_root();
    let absent = HostProfile::default();
    let explicit: HostProfile = serde_json::from_value(json!({"primaryReadOnly":false})).unwrap();
    assert_eq!(
        serde_json::to_vec(&absent).unwrap(),
        serde_json::to_vec(&explicit).unwrap()
    );
    assert!(
        serde_json::to_value(&absent)
            .unwrap()
            .get("primaryReadOnly")
            .is_none()
    );
    let mut old = open(&root, absent, "legacy", false);
    let config = old
        .invoke("configuration", &json!({}), &deadline())
        .unwrap();
    let old_row: String = old
        .engine
        .store
        .db
        .query_row(
            "SELECT value FROM records WHERE kind=?1",
            [yoke::project_session::RECORD_KIND],
            |r| r.get(0),
        )
        .unwrap();
    assert!(old.invoke("edit.prepare", &edit(), &deadline()).is_ok());
    drop(old);
    let mut resumed = open(&root, explicit, "legacy", false);
    assert_eq!(
        resumed
            .invoke("configuration", &json!({}), &deadline())
            .unwrap(),
        config
    );
    let row: String = resumed
        .engine
        .store
        .db
        .query_row(
            "SELECT value FROM records WHERE kind=?1",
            [yoke::project_session::RECORD_KIND],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(row, old_row);
    assert!(!root.path().join("planned.rs").exists());
}

#[test]
fn primary_read_only_denies_genuine_plans_without_source_or_store_mutation() {
    let root = owned_root();
    let original = fs::read(root.path().join("main.rs")).unwrap();
    let mut session = open(&root, profile(true), "readonly", true);
    assert!(!session.engine.allow_writes);
    let before = changes(&session);
    let error = session
        .invoke("edit.prepare", &edit(), &deadline())
        .unwrap_err();
    assert!(error.to_string().contains("read-only"));
    assert_eq!(changes(&session), before);
    assert!(!root.path().join("planned.rs").exists());
    assert_eq!(fs::read(root.path().join("main.rs")).unwrap(), original);
    let config = session
        .invoke("configuration", &json!({}), &deadline())
        .unwrap();
    assert_eq!(config["primaryReadOnly"], true);
    assert_eq!(
        session
            .invoke("listProjects", &json!({}), &deadline())
            .unwrap()["items"][0]["readOnly"],
        true
    );
    assert!(
        session
            .invoke(
                "configuration",
                &json!({"primaryReadOnly":false}),
                &deadline()
            )
            .is_err()
    );
}

#[test]
fn primary_constraint_survives_modes_and_switches_without_revoking_secondary_grants() {
    let root = owned_root();
    let secondary = owned_root();
    let locked = owned_root();
    let host = HostProfile {
        primary_read_only: true,
        projects: vec![
            HostProject {
                name: "secondary".into(),
                root: secondary.path().into(),
                read_only: false,
                query_only: false,
            },
            HostProject {
                name: "locked".into(),
                root: locked.path().into(),
                read_only: true,
                query_only: false,
            },
        ],
        ..Default::default()
    };
    let mut session = open(&root, host, "projects", true);
    let listed = session
        .invoke("listProjects", &json!({}), &deadline())
        .unwrap();
    assert_eq!(
        listed["items"]
            .as_array()
            .unwrap()
            .iter()
            .find(|v| v["name"] == "secondary")
            .unwrap()["readOnly"],
        false
    );
    session
        .invoke("activate", &json!({"project":"secondary"}), &deadline())
        .unwrap();
    assert!(session.active_engine_at(&deadline()).unwrap().allow_writes);
    let prepared = session
        .invoke("edit.prepare", &edit(), &deadline())
        .unwrap();
    assert_eq!(prepared["status"], "ok", "{prepared}");
    let id = prepared["data"]["plan"]["id"].as_str().unwrap();
    let active = session.active_engine_at(&deadline()).unwrap();
    yoke::changes::approve_at(&active.store, id, false, &deadline()).unwrap();
    verify_plan(active, id);
    let applied = active
        .invoke(
            "ci_change_apply",
            json!({"planId":id,"idempotencyKey":"owned-secondary","mode":"workspace"}),
        )
        .unwrap();
    assert_eq!(applied["status"], "ok", "{applied}");
    assert_eq!(
        fs::read(secondary.path().join("planned.rs")).unwrap(),
        b"fn planned() {}\n"
    );
    assert!(!root.path().join("planned.rs").exists());
    session
        .invoke("activate", &json!({"project":"locked"}), &deadline())
        .unwrap();
    assert!(!session.active_engine_at(&deadline()).unwrap().allow_writes);
    assert!(session.require_edit_permission().is_err());
    session
        .invoke("activate", &json!({"project":"default"}), &deadline())
        .unwrap();
    for modes in [json!(["planning"]), json!(["editing"])] {
        session
            .invoke("switchModes", &json!({"modes":modes}), &deadline())
            .unwrap();
        assert!(session.require_edit_permission().is_err());
    }
    assert!(!session.active_engine_at(&deadline()).unwrap().allow_writes);
    let alias = HostProfile {
        primary_read_only: true,
        projects: vec![HostProject {
            name: "alias".into(),
            root: root.path().join("."),
            read_only: false,
            query_only: false,
        }],
        ..Default::default()
    };
    let error = Session::open(engine(root.path(), true), alias, Some("alias"), &deadline())
        .err()
        .unwrap();
    assert!(error.to_string().contains("writable registered project"));
    let nested = root.path().join("nested");
    fs::create_dir(&nested).unwrap();
    fs::write(nested.join("main.rs"), b"fn nested() {}\n").unwrap();
    for (primary, registered) in [
        (root.path(), nested.as_path()),
        (nested.as_path(), root.path()),
    ] {
        let alias = HostProfile {
            primary_read_only: true,
            projects: vec![HostProject {
                name: "overlap".into(),
                root: registered.into(),
                read_only: false,
                query_only: false,
            }],
            ..Default::default()
        };
        let error = Session::open(engine(primary, true), alias, Some("overlap"), &deadline())
            .err()
            .unwrap();
        assert!(error.to_string().contains("writable registered project"));
    }
}

#[test]
fn true_false_binding_changes_refuse_and_original_state_records_stay_exact() {
    let root = owned_root();
    let old = open(&root, profile(false), "legacy", true);
    let before: String = old
        .engine
        .store
        .db
        .query_row(
            "SELECT value FROM records WHERE kind=?1",
            [yoke::project_session::RECORD_KIND],
            |r| r.get(0),
        )
        .unwrap();
    drop(old);
    assert!(
        Session::open(
            engine(root.path(), true),
            profile(true),
            Some("legacy"),
            &deadline()
        )
        .is_err()
    );
    let resumed = open(&root, profile(false), "legacy", true);
    let after: String = resumed
        .engine
        .store
        .db
        .query_row(
            "SELECT value FROM records WHERE kind=?1",
            [yoke::project_session::RECORD_KIND],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(before, after);
    drop(resumed);
    drop(open(&root, profile(true), "restricted", true));
    assert!(
        Session::open(
            engine(root.path(), true),
            profile(false),
            Some("restricted"),
            &deadline()
        )
        .is_err()
    );
    let mut resumed = open(&root, profile(true), "restricted", true);
    assert!(
        resumed
            .invoke("edit.prepare", &edit(), &deadline())
            .is_err()
    );
}

#[test]
fn yaml_project_and_local_flags_only_tighten_and_invalid_host_types_refuse() {
    use yoke::session_config::{Source, Target, project};
    let target = Target::Project {
        project: "default".into(),
    };
    let local = Target::ProjectLocal {
        project: "default".into(),
    };
    let yes = b"read_only: true\r\n";
    let no = b"read_only: false\r\n";
    let hy = yoke::model::hash(yes);
    let hn = yoke::model::hash(no);
    for sources in [
        [
            Source {
                target: &target,
                expected_sha256: &hy,
                bytes: yes,
            },
            Source {
                target: &local,
                expected_sha256: &hn,
                bytes: no,
            },
        ],
        [
            Source {
                target: &target,
                expected_sha256: &hn,
                bytes: no,
            },
            Source {
                target: &local,
                expected_sha256: &hy,
                bytes: yes,
            },
        ],
    ] {
        assert!(
            project(&profile(false), &sources, &deadline())
                .unwrap()
                .profile()
                .primary_read_only
        );
    }
    assert!(
        project(
            &profile(true),
            &[Source {
                target: &target,
                expected_sha256: &hn,
                bytes: no
            }],
            &deadline()
        )
        .unwrap()
        .profile()
        .primary_read_only
    );
    let unchanged = project(
        &profile(false),
        &[Source {
            target: &target,
            expected_sha256: &hn,
            bytes: no,
        }],
        &deadline(),
    )
    .unwrap();
    assert_eq!(
        unchanged.profile_bytes(&deadline()).unwrap(),
        serde_json::to_vec(&profile(false)).unwrap()
    );
    for value in [Value::Null, json!("false"), json!(0), json!([])] {
        assert!(serde_json::from_value::<HostProfile>(json!({"primaryReadOnly":value})).is_err());
    }
}

#[test]
fn native_and_serena_mcp_deny_writes_and_semantic_plan_carriers_but_allow_reads() {
    let root = owned_root();
    let mut baseline = open(&root, profile(false), "positive", true);
    let mut positive = ConnectionState::default();
    message(
        &mut positive,
        &mut baseline,
        Profile::Serena,
        "initialize",
        json!({}),
    );
    let call = json!({"name":"create_text_file","arguments":{"relative_path":"planned.rs","content":"fn planned() {}\n"}});
    let prepared = message(
        &mut positive,
        &mut baseline,
        Profile::Serena,
        "tools/call",
        call.clone(),
    );
    assert_eq!(prepared["result"]["isError"], false, "{prepared}");
    drop(baseline);
    let mut session = open(&root, profile(true), "mcp", true);
    let mut connection = ConnectionState::default();
    message(
        &mut connection,
        &mut session,
        Profile::Serena,
        "initialize",
        json!({}),
    );
    let read = message(
        &mut connection,
        &mut session,
        Profile::Serena,
        "tools/call",
        json!({"name":"read_file","arguments":{"relative_path":"main.rs"}}),
    );
    assert_eq!(read["result"]["isError"], false, "{read}");
    let denied = message(
        &mut connection,
        &mut session,
        Profile::Serena,
        "tools/call",
        call,
    );
    assert_eq!(denied["result"]["isError"], true);
    assert!(denied.to_string().contains("read-only"));
    let mut native = ConnectionState::default();
    message(
        &mut native,
        &mut session,
        Profile::Session,
        "initialize",
        json!({}),
    );
    let args = json!({"query":"source","languageServer":{"path":"main.rs","offset":0,"operation":"codeActions"}});
    let denied = message(
        &mut native,
        &mut session,
        Profile::Session,
        "tools/call",
        json!({"name":"ci_find","arguments":args.clone()}),
    );
    assert_eq!(denied["result"]["isError"], true);
    assert!(denied.to_string().contains("read-only"));
    let error = session
        .invoke("symbols.find", &args, &deadline())
        .unwrap_err();
    assert!(error.to_string().contains("read-only"));
    assert!(!root.path().join("planned.rs").exists());
}

#[test]
fn previously_approved_workspace_plan_cannot_bypass_the_primary_engine_restriction() {
    let root = owned_root();
    let original = fs::read(root.path().join("main.rs")).unwrap();
    let mut writable = engine(root.path(), true);
    let prepared = writable.invoke("ci_change_prepare", edit()).unwrap();
    assert_eq!(prepared["status"], "ok", "{prepared}");
    let id = prepared["data"]["plan"]["id"].as_str().unwrap().to_owned();
    yoke::changes::approve_at(&writable.store, &id, false, &deadline()).unwrap();
    verify_plan(&writable, &id);
    drop(writable);
    let mut session = open(&root, profile(true), "apply-denied", true);
    let result = session.engine.invoke(
        "ci_change_apply",
        json!({"planId":id,"idempotencyKey":"owned-readonly","mode":"workspace"}),
    );
    let error = match result {
        Ok(value) => {
            assert_eq!(value["status"], "error", "{value}");
            value.to_string()
        }
        Err(error) => error.to_string(),
    };
    assert!(error.contains("allow-writes"), "{error}");
    assert!(!root.path().join("planned.rs").exists());
    assert_eq!(fs::read(root.path().join("main.rs")).unwrap(), original);
}

#[test]
fn real_python_cached_edit_callbacks_cannot_escape_primary_read_only() {
    use yoke::agent_repl::{AgentRepl, ReplSpec};
    let mut command = Command::new("python");
    command.args(["-I", "-c", "import sys;print(sys.executable)"]);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000);
    }
    let selected = command.output().unwrap();
    assert!(selected.status.success());
    let python = std::path::PathBuf::from(String::from_utf8(selected.stdout).unwrap().trim());
    let root = owned_root();
    let mut session = open(&root, profile(true), "python", true);
    let mut worker = AgentRepl::spawn(
        ReplSpec {
            python,
            trust_python: true,
            max_output_bytes: 100000,
            shell: None,
        },
        &deadline(),
    )
    .unwrap();
    let read = worker
        .execute(
            &mut session,
            "saved=s.fs.create_text_file\ns.fs.read_file('main.rs')",
            &deadline(),
        )
        .unwrap();
    assert!(read["error"].is_null(), "{read}");
    assert!(read["result"].to_string().contains("source"));
    for modes in [json!(["planning"]), json!(["editing"])] {
        session
            .invoke("switchModes", &json!({"modes":modes}), &deadline())
            .unwrap();
        let denied = worker
            .execute(
                &mut session,
                "saved('planned.rs','fn planned() {}')",
                &deadline(),
            )
            .unwrap();
        assert!(!denied["error"].is_null(), "{denied}");
    }
    assert!(!root.path().join("planned.rs").exists());
    worker.close().unwrap();
}
