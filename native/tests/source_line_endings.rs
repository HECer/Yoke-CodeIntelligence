use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
};
use yoke::{
    Engine,
    capture::Deadline,
    changes::{self, Plan},
    model::{hash, identity},
    project_session::{HostProfile, HostProject, Session},
    source_line_endings::ResolvedLineEnding,
};

fn d() -> Deadline {
    Deadline::new(30000)
}
fn root(text: &str) -> tempfile::TempDir {
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("main.rs"), text).unwrap();
    fs::write(
        root.path().join("untouched.rs"),
        "// unchanged\r\nfn untouched() {}\n",
    )
    .unwrap();
    root
}
fn profile(value: Value) -> HostProfile {
    serde_json::from_value(value).unwrap()
}
fn open(path: &Path, host: HostProfile, id: &str) -> Session {
    let mut engine = Engine::open(path).unwrap();
    engine.allow_writes = true;
    Session::open(engine, host, Some(id), &d()).unwrap()
}
fn prepare(session: &mut Session, edits: Value) -> Plan {
    serde_json::from_value(
        session
            .invoke("edit.prepare", &json!({"edits":edits}), &d())
            .unwrap()["data"]["plan"]
            .clone(),
    )
    .unwrap()
}
fn create(session: &mut Session, path: &str, text: &str) -> Plan {
    prepare(session, json!([{"kind":"create","path":path,"text":text}]))
}
fn after(engine: &Engine, plan: &Plan, path: &str) -> Vec<u8> {
    let snapshot = engine.store.snapshot(&plan.result_snapshot_id).unwrap();
    let file = snapshot.files.iter().find(|f| f.path == path).unwrap();
    engine.store.read_blob(&file.hash).unwrap()
}
fn reseal(plan: &Plan, ending: ResolvedLineEnding) -> String {
    let old = identity(&(
        &plan.base_snapshot_id,
        &plan.result_snapshot_id,
        &plan.files,
        plan.created_at,
    ));
    identity(&(old, "source-line-ending-v1", ending))
}

#[test]
fn absent_and_null_keep_legacy_profile_session_plan_identity_and_mixed_source_bytes() {
    let root = root("fn original() {}\r\n");
    let absent = HostProfile::default();
    let null = profile(json!({"sourceLineEndings":null}));
    assert_eq!(
        serde_json::to_vec(&absent).unwrap(),
        serde_json::to_vec(&null).unwrap()
    );
    let mut session = open(root.path(), absent, "legacy");
    let config = session.invoke("configuration", &json!({}), &d()).unwrap();
    let plan = create(
        &mut session,
        "created.rs",
        "// \u{1f4a1}\r\nfn created() {}\n",
    );
    assert_eq!(plan.source_line_ending, None);
    assert_eq!(
        plan.id,
        identity(&(
            &plan.base_snapshot_id,
            &plan.result_snapshot_id,
            &plan.files,
            plan.created_at
        ))
    );
    assert!(
        serde_json::to_value(&plan)
            .unwrap()
            .get("sourceLineEnding")
            .is_none()
    );
    assert_eq!(
        after(&session.engine, &plan, "created.rs"),
        "// \u{1f4a1}\r\nfn created() {}\n".as_bytes()
    );
    let raw: String = session
        .engine
        .store
        .db
        .query_row(
            "select value from records where kind='plan' and id=?1",
            [&plan.id],
            |r| r.get(0),
        )
        .unwrap();
    drop(session);
    let mut resumed = open(root.path(), null, "legacy");
    assert_eq!(
        resumed.invoke("configuration", &json!({}), &d()).unwrap(),
        config
    );
    let loaded = changes::load_checked_plan(&resumed.engine.store, &plan.id, &d()).unwrap();
    assert_eq!(serde_json::to_string(&loaded).unwrap(), raw);
    assert!(!root.path().join("created.rs").exists());
}

#[test]
fn original_unicode_offsets_and_parallel_edits_precede_formatting_only_written_sources() {
    let before = "// \u{1f4a1} e\u{301}\r\nfn alpha() {}\r\nfn omega() {}\n";
    let root = root(before);
    let mut session = open(
        root.path(),
        profile(json!({"sourceLineEndings":{"global":"lf"}})),
        "lf",
    );
    let a = before.find("alpha").unwrap();
    let z = before.find("omega").unwrap();
    let plan = prepare(
        &mut session,
        json!([
            {"kind":"replace","path":"main.rs","expectedHash":hash(before),"startByte":a,"endByte":a+5,"text":"beta"},
            {"kind":"replace","path":"main.rs","expectedHash":hash(before),"startByte":z,"endByte":z+5,"text":"last"},
            {"kind":"create","path":"new.rs","text":"// \u{feff}e\u{301}\r\nfn new() {}\r"}
        ]),
    );
    assert_eq!(plan.source_line_ending, Some(ResolvedLineEnding::Lf));
    assert_eq!(
        after(&session.engine, &plan, "main.rs"),
        "// \u{1f4a1} e\u{301}\nfn beta() {}\nfn last() {}\n".as_bytes()
    );
    assert_eq!(
        after(&session.engine, &plan, "new.rs"),
        "// \u{feff}e\u{301}\nfn new() {}\n".as_bytes()
    );
    assert_eq!(
        after(&session.engine, &plan, "untouched.rs"),
        fs::read(root.path().join("untouched.rs")).unwrap()
    );
    assert_eq!(
        fs::read(root.path().join("main.rs")).unwrap(),
        before.as_bytes()
    );
    assert!(plan.diff.contains("beta") && plan.diff.contains("last"));
    changes::load_checked_plan(&session.engine.store, &plan.id, &d()).unwrap();
    let error=session.invoke("edit.prepare",&json!({"edits":[{"kind":"replace","path":"main.rs","expectedHash":hash(before),"startByte":4,"endByte":5,"text":"x"}]}),&d()).unwrap();
    assert_eq!(error["status"], "error");
    assert!(
        error["error"]["message"]
            .as_str()
            .unwrap()
            .contains("UTF-8"),
        "{error}"
    );
}

#[test]
fn project_overrides_native_resolution_modes_and_resume_bind_concrete_host_policy() {
    let primary = root("fn primary() {}\n");
    let secondary = root("fn secondary() {}\n");
    let inherited = root("fn inherited() {}\n");
    let mut host = profile(
        json!({"sourceLineEndings":{"global":"crlf","projects":{"default":{"project":"crlf","local":"lf"},"secondary":{"project":"native"}}}}),
    );
    host.projects = vec![
        HostProject {
            name: "secondary".into(),
            root: secondary.path().to_path_buf(),
            read_only: false,
            query_only: false,
        },
        HostProject {
            name: "inherited".into(),
            root: inherited.path().to_path_buf(),
            read_only: false,
            query_only: false,
        },
    ];
    let mut session = open(primary.path(), host.clone(), "selected");
    let plan = create(&mut session, "created.rs", "fn created() {}\n");
    assert_eq!(plan.source_line_ending, Some(ResolvedLineEnding::Lf));
    session
        .invoke("activate", &json!({"project":"secondary"}), &d())
        .unwrap();
    let plan = create(&mut session, "created.rs", "fn created() {}\n");
    assert_eq!(
        plan.source_line_ending,
        Some(if cfg!(windows) {
            ResolvedLineEnding::Crlf
        } else {
            ResolvedLineEnding::Lf
        })
    );
    session
        .invoke("activate", &json!({"project":"inherited"}), &d())
        .unwrap();
    let plan = create(&mut session, "created.rs", "fn created() {}\n");
    assert_eq!(plan.source_line_ending, Some(ResolvedLineEnding::Crlf));
    session
        .invoke("activate", &json!({"project":"default"}), &d())
        .unwrap();
    session
        .invoke("switchModes", &json!({"modes":["planning"]}), &d())
        .unwrap();
    assert!(
        session
            .invoke(
                "edit.prepare",
                &json!({"edits":[{"kind":"create","path":"blocked.rs","text":"x\n"}]}),
                &d()
            )
            .is_err()
    );
    drop(session);
    let mut resumed = open(primary.path(), host.clone(), "selected");
    assert_eq!(
        resumed.invoke("configuration", &json!({}), &d()).unwrap()["sourceLineEnding"],
        "lf"
    );
    drop(resumed);
    host.source_line_endings.as_mut().unwrap().global =
        Some(yoke::source_line_endings::LineEnding::Lf);
    let mut rebound = Engine::open(primary.path()).unwrap();
    rebound.allow_writes = true;
    assert!(Session::open(rebound, host, Some("selected"), &d()).is_err());
}

#[test]
fn strict_policy_schema_registration_and_primary_read_only_keep_authority_separate() {
    for value in [
        json!({"global":"LF"}),
        json!({"global":true}),
        json!({"global":3}),
        json!({"global":"preserve"}),
        json!({"global":"lf","allowWrites":true}),
        json!({"projects":{"default":{"local":"cr"}}}),
    ] {
        assert!(serde_json::from_value::<HostProfile>(json!({"sourceLineEndings":value})).is_err());
    }
    let root = root("fn original() {}\n");
    assert!(
        Session::open(
            Engine::open(root.path()).unwrap(),
            profile(json!({"sourceLineEndings":{"projects":{"foreign":{"project":"lf"}}}})),
            Some("bad"),
            &d()
        )
        .is_err()
    );
    let mut session = open(
        root.path(),
        profile(json!({"primaryReadOnly":true,"sourceLineEndings":{"global":"crlf"}})),
        "readonly",
    );
    let count = session.engine.store.db.total_changes();
    assert!(
        session
            .invoke(
                "edit.prepare",
                &json!({"edits":[{"kind":"create","path":"denied.rs","text":"fn denied() {}\n"}]}),
                &d()
            )
            .is_err()
    );
    assert_eq!(count, session.engine.store.db.total_changes());
    assert!(!root.path().join("denied.rs").exists());
}

#[test]
fn selected_plan_identity_replay_rejects_changed_stripped_and_resealed_false_conventions() {
    let root = root("fn original() {}\r\n");
    let mut session = open(
        root.path(),
        profile(json!({"sourceLineEndings":{"global":"lf"}})),
        "lf",
    );
    let plan = create(&mut session, "created.rs", "fn created() {}\r\n");
    let mut changed = plan.clone();
    changed.source_line_ending = Some(ResolvedLineEnding::Crlf);
    session
        .engine
        .store
        .record("plan", &plan.id, &changed)
        .unwrap();
    assert!(changes::load_checked_plan(&session.engine.store, &plan.id, &d()).is_err());
    changed.id = reseal(&changed, ResolvedLineEnding::Crlf);
    session
        .engine
        .store
        .record("plan", &changed.id, &changed)
        .unwrap();
    assert!(changes::load_checked_plan(&session.engine.store, &changed.id, &d()).is_err());
    changed = plan.clone();
    changed.source_line_ending = None;
    session
        .engine
        .store
        .record("plan", &plan.id, &changed)
        .unwrap();
    assert!(changes::load_checked_plan(&session.engine.store, &plan.id, &d()).is_err());
    session
        .engine
        .store
        .record("plan", &plan.id, &plan)
        .unwrap();
    changes::load_checked_plan(&session.engine.store, &plan.id, &d()).unwrap();
}

#[test]
fn workspace_apply_uses_verified_exact_bytes_and_recovery_restores_original_mixed_bytes() {
    let before = "fn original() {}\r\n";
    let root = root(before);
    let mut session = open(
        root.path(),
        profile(json!({"sourceLineEndings":{"global":"crlf"}})),
        "crlf",
    );
    let plan = prepare(
        &mut session,
        json!([{"kind":"replace","path":"main.rs","expectedHash":hash(before),"startByte":0,"endByte":before.len(),"text":"fn changed() {}\n"}]),
    );
    let stored = serde_json::to_vec(&plan).unwrap();
    let expected = "fn changed() {}\r\n";
    changes::approve(&session.engine.store, &plan.id, false).unwrap();
    let script = format!(
        "const fs=require('node:fs');process.exit(fs.readFileSync('main.rs','utf8')==={} ? 0 : 1)",
        serde_json::to_string(expected).unwrap()
    );
    let verification = yoke::verify::execute(
        &session.engine,
        &plan.id,
        "node",
        &["-e".into(), script],
        true,
        &d(),
    )
    .unwrap();
    assert_eq!(verification["status"], "passed");
    let receipt = changes::apply(
        &session.engine.store,
        root.path(),
        &json!({"planId":plan.id,"idempotencyKey":"exact","mode":"workspace"}),
        true,
        &d(),
    )
    .unwrap();
    assert_eq!(
        fs::read(root.path().join("main.rs")).unwrap(),
        expected.as_bytes()
    );
    assert_eq!(
        serde_json::to_vec(
            &changes::load_checked_plan(&session.engine.store, &plan.id, &d()).unwrap()
        )
        .unwrap(),
        stored
    );
    changes::recover(&session.engine.store, root.path(), &receipt.id, true).unwrap();
    assert_eq!(
        fs::read(root.path().join("main.rs")).unwrap(),
        before.as_bytes()
    );
}

#[test]
fn old_approved_plan_is_not_reformatted_and_conflicting_host_apply_refuses_without_journal() {
    let root = root("fn original() {}\n");
    let mut old = open(root.path(), HostProfile::default(), "old");
    let plan = create(&mut old, "created.rs", "fn created() {}\n");
    changes::approve(&old.engine.store, &plan.id, false).unwrap();
    let script = "const fs=require('node:fs');process.exit(fs.readFileSync('created.rs','utf8')==='fn created() {}\\n'?0:1)";
    assert_eq!(
        yoke::verify::execute(
            &old.engine,
            &plan.id,
            "node",
            &["-e".into(), script.into()],
            true,
            &d()
        )
        .unwrap()["status"],
        "passed"
    );
    drop(old);
    let selected = open(
        root.path(),
        profile(json!({"sourceLineEndings":{"global":"crlf"}})),
        "new",
    );
    let loaded = changes::load_checked_plan(&selected.engine.store, &plan.id, &d()).unwrap();
    assert_eq!(
        after(&selected.engine, &loaded, "created.rs"),
        b"fn created() {}\n"
    );
    let count = selected.engine.store.db.total_changes();
    let error = changes::apply(
        &selected.engine.store,
        root.path(),
        &json!({"planId":plan.id,"idempotencyKey":"conflicting","mode":"workspace"}),
        true,
        &d(),
    )
    .err()
    .unwrap();
    assert!(
        error
            .to_string()
            .contains("changes_plan_line_ending_mismatch"),
        "{error}"
    );
    assert_eq!(count, selected.engine.store.db.total_changes());
    assert!(!root.path().join("created.rs").exists());
    let isolated = changes::apply(
        &selected.engine.store,
        root.path(),
        &json!({"planId":plan.id,"idempotencyKey":"old-isolated","mode":"isolated"}),
        false,
        &d(),
    )
    .unwrap();
    assert_eq!(
        fs::read(Path::new(isolated.sandbox_path.as_ref().unwrap()).join("created.rs")).unwrap(),
        b"fn created() {}\n"
    );
}

#[test]
fn notes_and_config_are_outside_source_formatting_and_null_yaml_preserves_old_profile() {
    let root = root("fn original() {}\n");
    let mut session = open(
        root.path(),
        profile(json!({"sourceLineEndings":{"global":"crlf"}})),
        "notes",
    );
    let text = "owned\nnotes\r\nexact";
    session
        .invoke(
            "notes.write",
            &json!({"name":"format","content":text}),
            &d(),
        )
        .unwrap();
    let read = session
        .invoke("notes.read", &json!({"name":"format"}), &d())
        .unwrap();
    assert!(
        serde_json::to_string(&read)
            .unwrap()
            .contains("owned\\nnotes\\r\\nexact"),
        "{read}"
    );
    let target = yoke::session_config::Target::Project {
        project: "default".into(),
    };
    let bytes = b"line_ending: null\r\n";
    let sha = hash(bytes);
    let projected = yoke::session_config::project(
        &HostProfile::default(),
        &[yoke::session_config::Source {
            target: &target,
            expected_sha256: &sha,
            bytes,
        }],
        &d(),
    )
    .unwrap();
    assert_eq!(
        projected.profile_bytes(&d()).unwrap(),
        serde_json::to_vec(&HostProfile::default()).unwrap()
    );
    assert_eq!(bytes, b"line_ending: null\r\n");
}

#[test]
fn real_lsp_resolved_actions_and_real_typescript_compiler_share_selected_source_formatting() {
    let root = root("fn helper() {}\n");
    let mut session = open(
        root.path(),
        profile(json!({"sourceLineEndings":{"global":"crlf"}})),
        "lsp",
    );
    let server = yoke::lsp::ConfiguredServer {
        server: yoke::lsp::Server {
            command: "node".into(),
            args: vec![
                Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("tests/fixtures/resolve-lsp.mjs")
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
    };
    let engine = session.active_engine().unwrap();
    let (s, g, _, _) = engine.index(&d()).unwrap();
    let value = yoke::lsp::run_at_configured(
        engine,
        &server,
        &s,
        &g,
        yoke::lsp::SemanticRequest {
            path: "main.rs",
            offset: 3,
            operation: "codeActions",
            new_name: None,
        },
        &json!({"only":["refactor.inline"]}),
        &d(),
    )
    .unwrap();
    let plan: Plan = serde_json::from_value(value["items"][0]["plan"].clone()).unwrap();
    assert_eq!(after(engine, &plan, "main.rs"), b"fn renamed() {}\r\n");
    changes::load_checked_plan(&engine.store, &plan.id, &d()).unwrap();
    let ts = tempfile::tempdir().unwrap();
    let source =
        "// \u{1f4a1}\r\nconst helper = (n:number) => n+1;\r\nexport const result=helper(2);\r\n";
    fs::write(ts.path().join("main.ts"), source).unwrap();
    fs::write(ts.path().join("tsconfig.json"),r#"{"compilerOptions":{"strict":true,"target":"ES2022","module":"CommonJS","noEmit":true},"files":["main.ts"]}"#).unwrap();
    let node = std::env::split_paths(&std::env::var_os("PATH").unwrap())
        .map(|p| p.join(if cfg!(windows) { "node.exe" } else { "node" }))
        .find(|p| p.is_file())
        .unwrap()
        .canonicalize()
        .unwrap();
    let mut host = profile(json!({"sourceLineEndings":{"global":"lf"}}));
    host.inline_compiler = Some(yoke::projects::CompilerSpec {
        node,
        typescript: PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../node_modules/typescript/lib/typescript.js")
            .canonicalize()
            .unwrap(),
        trusted: true,
    });
    let mut session = open(ts.path(), host, "compiler");
    let value=session.invoke("refactor.prepare",&json!({"operation":"inline","arguments":{"relativePath":"main.ts","namePath":"helper","keepDefinition":false}}),&d()).unwrap();
    assert_eq!(
        value["sourceFormatting"]["diagnosticsScope"],
        "provider_effect_before_host_source_formatting"
    );
    assert_eq!(
        value["providerEffectEquivalent"],
        "independent_compiler_variable_inline"
    );
    let plan: Plan = serde_json::from_value(value["actions"][0]["plan"].clone()).unwrap();
    let actual = String::from_utf8(after(&session.engine, &plan, "main.ts")).unwrap();
    assert!(
        !actual.contains('\r') && !actual.contains("helper"),
        "{actual}"
    );
    assert!(actual.starts_with("// \u{1f4a1}\n"));
    changes::load_checked_plan(&session.engine.store, &plan.id, &d()).unwrap();
    assert_eq!(
        fs::read(ts.path().join("main.ts")).unwrap(),
        source.as_bytes()
    );
}

#[test]
fn selected_formatting_cannot_change_path_scope_or_accept_stale_source_preconditions() {
    let before = "fn original() {}\r\n";
    let root = root(before);
    let mut session = open(
        root.path(),
        profile(json!({"sourceLineEndings":{"global":"lf"}})),
        "scope",
    );
    for path in [
        "../outside.rs",
        ".yoke/owned.rs",
        "C:/outside.rs",
        "safe/../outside.rs",
        "nul.rs\0",
    ] {
        assert!(
            session
                .invoke(
                    "edit.prepare",
                    &json!({"edits":[{"kind":"create","path":path,"text":"fn denied() {}\r\n"}]}),
                    &d()
                )
                .unwrap()["status"]
                == "error",
            "{path}"
        );
    }
    assert_eq!(session.invoke("edit.prepare",&json!({"edits":[{"kind":"replace","path":"main.rs","expectedHash":hash("other"),"startByte":0,"endByte":before.len(),"text":"fn denied() {}\r\n"}]}),&d()).unwrap()["status"],"error");
    assert_eq!(
        fs::read(root.path().join("main.rs")).unwrap(),
        before.as_bytes()
    );
    assert_eq!(
        session
            .engine
            .store
            .db
            .query_row("select count(*) from records where kind='plan'", [], |r| {
                r.get::<_, usize>(0)
            })
            .unwrap(),
        0
    );
}
