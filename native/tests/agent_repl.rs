use serde_json::json;
use std::{fs, path::PathBuf};
use yoke::{
    Engine,
    agent_repl::{AgentRepl, ReplSpec},
    capture::Deadline,
    project_session::{HostProfile, HostProject, Session},
};
fn python() -> PathBuf {
    let output = std::process::Command::new("python")
        .args(["-I", "-c", "import sys;print(sys.executable)"])
        .output()
        .unwrap();
    assert!(output.status.success());
    PathBuf::from(String::from_utf8(output.stdout).unwrap().trim())
}
fn spec() -> ReplSpec {
    ReplSpec {
        python: python(),
        trust_python: true,
        max_output_bytes: 100000,
        shell: None,
    }
}
fn session(root: &std::path::Path, host: HostProfile, deadline: &Deadline) -> Session {
    Session::open(
        Engine::open(root).unwrap(),
        host,
        Some("owned-agent-repl"),
        deadline,
    )
    .unwrap()
}
#[test]
fn persistent_cells_compose_real_source_queries_and_source_bound_notes() {
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("main.rs"), "fn alpha() {}\n").unwrap();
    let d = Deadline::new(20000);
    let mut s = session(root.path(), HostProfile::default(), &d);
    let mut repl = AgentRepl::spawn(spec(), &d).unwrap();
    let first=repl.execute(&mut s,"counter=40\ndef increase(x):\n return x+2\nfiles=s.fs.list_dir('',recursive=True)\nlen(files['files'])",&d).unwrap();
    assert_eq!(first["result"], 1);
    let second=repl.execute(&mut s,"counter=increase(counter)\ns.mem.write_memory('design/decision','counter='+str(counter))\n(counter,s.mem.read_memory('design/decision')['note']['content'])",&d).unwrap();
    assert_eq!(second["result"], json!([42, "counter=42"]), "{second}");
    let third = repl
        .execute(
            &mut s,
            "s.lsp.find_symbol('alpha',relative_path='main.rs')",
            &d,
        )
        .unwrap();
    assert_eq!(third["result"]["items"][0]["name"], "alpha");
    assert_eq!(
        fs::read_to_string(root.path().join("main.rs")).unwrap(),
        "fn alpha() {}\n"
    );
    repl.close().unwrap();
}
#[test]
fn external_project_context_composes_read_queries_and_cannot_prepare_edits() {
    let root = tempfile::tempdir().unwrap();
    let external = tempfile::tempdir().unwrap();
    fs::write(root.path().join("main.rs"), "fn alpha() {}\n").unwrap();
    fs::write(external.path().join("main.rs"), "fn beta() {}\n").unwrap();
    let d = Deadline::new(20000);
    let host = HostProfile {
        projects: vec![HostProject {
            name: "external".into(),
            root: external.path().into(),
            read_only: true,
            query_only: true,
        }],
        ..Default::default()
    };
    let mut s = session(root.path(), host, &d);
    let mut repl = AgentRepl::spawn(spec(), &d).unwrap();
    let value=repl.execute(&mut s,"with s.ext.project_context('external') as p:\n found=p.lsp.find_symbol('beta',relative_path='main.rs')\n try:\n  p.fs.create_text_file('extra.rs','fn unsafe_write(){}')\n except RuntimeError:\n  refused=True\n(found['items'][0]['name'],refused,s.cfg.get_current_config()['activeProject'])",&d).unwrap();
    assert_eq!(value["result"], json!(["beta", true, "default"]));
    assert!(!external.path().join("extra.rs").exists());
}
#[test]
fn edits_are_guarded_plans_shell_is_separate_and_results_do_not_execute_repr() {
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("main.rs"), "fn alpha() {}\n").unwrap();
    let d = Deadline::new(20000);
    let mut s = session(root.path(), HostProfile::default(), &d);
    let mut repl = AgentRepl::spawn(spec(), &d).unwrap();
    let result=repl.execute(&mut s,"plan=s.fs.create_text_file('created.rs','fn created(){}')\ntry:\n s.shell.execute_shell_command('echo not-authorized')\nexcept RuntimeError:\n refused=True\nclass Dangerous:\n def __repr__(self):\n  raise RuntimeError('repr must not be executed by result serializer')\n(plan['plan']['id'],refused,Dangerous())",&d).unwrap();
    assert!(result["result"][0].as_str().is_some());
    assert_eq!(result["result"][1], true);
    assert_eq!(result["result"][2]["omitted"], "unsupported_object");
    assert!(!root.path().join("created.rs").exists());
    assert!(
        repl.execute(
            &mut s,
            "s.invoke('activate',{'project':'outside-host-registry'})",
            &d
        )
        .unwrap()["error"]
            .is_object()
    );
}
#[test]
fn trust_output_and_deadline_failures_are_explicit_and_discard_worker_state() {
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("main.rs"), "fn alpha() {}\n").unwrap();
    let d = Deadline::new(20000);
    let mut s = session(root.path(), HostProfile::default(), &d);
    let mut denied = spec();
    denied.trust_python = false;
    assert!(AgentRepl::spawn(denied, &d).is_err());
    let mut small = spec();
    small.max_output_bytes = 100;
    let mut repl = AgentRepl::spawn(small, &d).unwrap();
    assert!(repl.execute(&mut s, "print('x'*10000)", &d).is_err());
    assert!(repl.execute(&mut s, "42", &d).is_err());
    let mut repl = AgentRepl::spawn(spec(), &d).unwrap();
    let timeout = Deadline::new(300);
    assert!(repl.execute(&mut s, "while True: pass", &timeout).is_err());
    assert!(repl.execute(&mut s, "42", &d).is_err());
}

#[test]
fn line_bulk_and_symbol_edits_are_source_checked_independent_immutable_plans() {
    let root = tempfile::tempdir().unwrap();
    let source = "fn alpha() {}\nfn caller(){ alpha(); }\n";
    fs::write(root.path().join("main.rs"), source).unwrap();
    let d = Deadline::new(20000);
    let mut s = session(root.path(), HostProfile::default(), &d);
    let mut repl = AgentRepl::spawn(spec(), &d).unwrap();
    let value=repl.execute(&mut s,"line=s.edit.replace_lines('main.rs',0,0,'fn renamed() {}\\n')\nbulk=s.edit.replace_content('main.rs','alpha','renamed','literal',allow_multiple_occurrences=True)\nbody=s.edit.replace_symbol_body('alpha','main.rs','fn changed() {}')\n(line['plan']['baseSnapshotId'],bulk['plan']['baseSnapshotId'],body['plan']['baseSnapshotId'])",&d).unwrap();
    let items = value["result"]
        .as_array()
        .unwrap_or_else(|| panic!("{value}"));
    assert_eq!(items.len(), 3);
    assert!(items.iter().all(|id| id == &items[0]));
    assert_eq!(
        fs::read_to_string(root.path().join("main.rs")).unwrap(),
        source
    );
}

#[test]
fn symbol_references_and_semantic_diagnostics_use_registered_source_provider() {
    let root = tempfile::tempdir().unwrap();
    let source = "// 🌍\nfn helper() {}\nfn main() { helper(); }\n";
    fs::write(root.path().join("main.rs"), source).unwrap();
    let d = Deadline::new(20000);
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
                    "".into(),
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
    let mut s =
        Session::open(engine, HostProfile::default(), Some("owned-agent-repl"), &d).unwrap();
    let mut repl = AgentRepl::spawn(spec(), &d).unwrap();
    let value=repl.execute(&mut s,"references=s.lsp.find_referencing_symbols('helper','main.rs')\ndiagnostics=s.lsp.get_diagnostics_for_file('main.rs')\n(len(references['observations']),diagnostics['items'][0]['message'],diagnostics['items'][0]['startByte'])",&d).unwrap();
    let items = value["result"]
        .as_array()
        .unwrap_or_else(|| panic!("{value}"));
    assert!(items[0].as_u64().unwrap() > 0);
    assert_eq!(items[1], "A fixture warning 🌍");
    assert_eq!(items[2], 35);
    assert_eq!(
        fs::read_to_string(root.path().join("main.rs")).unwrap(),
        source
    );
}

#[test]
fn provider_hierarchies_and_symbol_diagnostics_keep_exact_source_scope() {
    let root = tempfile::tempdir().unwrap();
    let source = "// 🌍\nfn helper() {}\nfn main() { helper(); }\n";
    fs::write(root.path().join("main.rs"), source).unwrap();
    let d = Deadline::new(20000);
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
                    "".into(),
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
    let mut s =
        Session::open(engine, HostProfile::default(), Some("owned-agent-repl"), &d).unwrap();
    let mut repl = AgentRepl::spawn(spec(), &d).unwrap();
    let value=repl.execute(&mut s,"hierarchy=s.jb.get_type_hierarchy('helper','main.rs',hierarchy_type='both')\nhelper=s.lsp.get_diagnostics_for_symbol('helper',reference_file='main.rs')\nmain=s.lsp.get_diagnostics_for_symbol('main',reference_file='main.rs')\n(hierarchy['supertypes']['items'][0]['selectionStartByte'],len(helper['items']),main['items'][0]['startByte'])",&d).unwrap();
    assert_eq!(value["result"], json!([26, 0, 35]), "{value}");
}

#[test]
fn explicitly_selected_shell_runs_and_cell_errors_preserve_namespace() {
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("main.rs"), "fn alpha() {}\n").unwrap();
    let setup = Deadline::new(60000);
    let mut s = session(root.path(), HostProfile::default(), &setup);
    let mut config = spec();
    config.shell = Some(if cfg!(windows) {
        yoke::agent_repl::HostShell {
            command: PathBuf::from(std::env::var_os("SystemRoot").unwrap())
                .join("System32/WindowsPowerShell/v1.0/powershell.exe"),
            arguments: vec![
                "-NoProfile".into(),
                "-NonInteractive".into(),
                "-Command".into(),
            ],
        }
    } else {
        yoke::agent_repl::HostShell {
            command: PathBuf::from("/bin/sh").canonicalize().unwrap(),
            arguments: vec!["-c".into()],
        }
    });
    let mut repl = AgentRepl::spawn(config, &setup).unwrap();
    let cell = if cfg!(windows) {
        "remembered=17\ns.shell.execute_shell_command('Write-Output owned-shell-test')['stdout'].strip()"
    } else {
        "remembered=17\ns.shell.execute_shell_command('printf owned-shell-test')['stdout'].strip()"
    };
    // Independent calls retain a 20s operation deadline after host/Python setup.
    let value = repl.execute(&mut s, cell, &Deadline::new(20000)).unwrap();
    assert_eq!(value["result"], "owned-shell-test", "{value}");
    assert!(
        repl.execute(
            &mut s,
            "raise ValueError('owned error')",
            &Deadline::new(20000)
        )
        .unwrap()["error"]
            .is_object()
    );
    assert_eq!(
        repl.execute(&mut s, "remembered", &Deadline::new(20000))
            .unwrap()["result"],
        17
    );
    let value=repl.execute(&mut s,"try:\n s.invoke('edit.prepare',{'trustServer':True,'edits':[]})\nexcept RuntimeError:\n rejected=True\nrejected",&Deadline::new(20000)).unwrap();
    assert_eq!(value["result"], true);
}

#[test]
fn python_regex_declaration_and_semantic_implementations_preserve_snapshot_anchors() {
    let root = tempfile::tempdir().unwrap();
    let source = "// 🌍\nfn helper() {}\nfn main() { helper(); }\n";
    fs::write(root.path().join("main.rs"), source).unwrap();
    let d = Deadline::new(20000);
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
                    "".into(),
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
    let mut s =
        Session::open(engine, HostProfile::default(), Some("owned-agent-repl"), &d).unwrap();
    let mut repl = AgentRepl::spawn(spec(), &d).unwrap();
    let value=repl.execute(&mut s,"declaration=s.lsp.find_declaration('main.rs',r'(?<=\\{ )(?P<symbol>helper)(?=\\(\\))',containing_symbol_name_path='main')\nimplementations=s.lsp.find_implementations('helper','main.rs')\ntry:\n s.lsp.find_declaration('main.rs',r'(helper)')\nexcept RuntimeError:\n ambiguous=True\n(declaration['locations'][0]['startByte'],implementations['locations'][0]['startByte'],ambiguous)",&d).unwrap();
    assert_eq!(value["result"], json!([11, 26, true]), "{value}");
    let info=repl.execute(&mut s,"s.lsp.find_declaration('main.rs',r'(?<=\\{ )(?P<symbol>helper)(?=\\(\\))',containing_symbol_name_path='main',include_info=True)['info']['response']['contents']['value']",&d).unwrap();
    assert_eq!(
        info["result"], "Independent fixture hover information",
        "{info}"
    );
    let body=repl.execute(&mut s,"s.lsp.find_declaration('main.rs',r'(?<=\\{ )(helper)(?=\\(\\))',include_body=True)['locations'][0]['symbol']['body']",&d).unwrap();
    assert_eq!(
        body["result"], "fn helper() {}",
        "Requested declaration body was omitted or replaced with a name-only location: {body}"
    );
}

#[test]
fn independent_jb_declaration_uses_the_same_python_regex_and_guarded_provider_bridge() {
    let root = tempfile::tempdir().unwrap();
    fs::write(
        root.path().join("main.rs"),
        "// 🌍\nfn helper() {}\nfn main() { helper(); }\n",
    )
    .unwrap();
    let deadline = Deadline::new(20000);
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
        HostProfile::default(),
        Some("independent-jb-declaration"),
        &deadline,
    )
    .unwrap();
    let mut repl = AgentRepl::spawn(spec(), &deadline).unwrap();
    let value=repl.execute(&mut session,"s.jb.find_declaration('main.rs',r'(?<=\\{ )(?P<symbol>helper)(?=\\(\\))')['locations'][0]['startByte']",&deadline).unwrap();
    assert_eq!(
        value["result"], 11,
        "Independent declaration facade did not route to its actual provider: {value}"
    );
}

#[test]
fn exception_rendering_never_reads_user_overridden_exception_properties() {
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("main.rs"), "fn alpha() {}\n").unwrap();
    let d = Deadline::new(20000);
    let mut s = session(root.path(), HostProfile::default(), &d);
    let mut repl = AgentRepl::spawn(spec(), &d).unwrap();
    let value=repl.execute(&mut s,"descriptor_called=False\nclass DangerousError(Exception):\n @property\n def args(self):\n  global descriptor_called\n  descriptor_called=True\n  return ('do not read this property',)\nraise DangerousError('owned error')",&d).unwrap();
    assert!(value["error"].is_object());
    assert_eq!(
        repl.execute(&mut s, "descriptor_called", &d).unwrap()["result"],
        false
    );
}

#[test]
fn semantic_rename_facade_only_prepares_registered_provider_guarded_plans() {
    let root = tempfile::tempdir().unwrap();
    let source = "// 🌍\nfn helper() {}\nfn main() { helper(); }\n";
    fs::write(root.path().join("main.rs"), source).unwrap();
    let d = Deadline::new(20000);
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
                    "".into(),
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
    let mut s =
        Session::open(engine, HostProfile::default(), Some("owned-agent-repl"), &d).unwrap();
    let mut repl = AgentRepl::spawn(spec(), &d).unwrap();
    let value=repl.execute(&mut s,"rename=s.lsp.rename_symbol('helper','main.rs','renamed')\nlen(rename['plan']['files'])",&d).unwrap();
    assert_eq!(value["result"], 2, "{value}");
    assert_eq!(
        fs::read_to_string(root.path().join("main.rs")).unwrap(),
        source
    );
    assert!(!root.path().join("renamed.rs").exists());
}

#[test]
fn forged_worker_callback_authority_keys_are_rejected_and_worker_state_is_lost() {
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("main.rs"), "fn alpha() {}\n").unwrap();
    let d = Deadline::new(20000);
    let mut s = session(root.path(), HostProfile::default(), &d);
    let mut repl = AgentRepl::spawn(spec(), &d).unwrap();
    let error=repl.execute(&mut s,"import sys,json\nsys.__stdout__.write(json.dumps({'event':'call','requestId':1,'callId':1,'method':'native.configuration','arguments':{},'project':None,'trustServer':True})+'\\n')\nsys.__stdout__.flush()",&d).unwrap_err();
    assert!(error.to_string().contains("unknown field"), "{error:#}");
    assert!(repl.execute(&mut s, "42", &d).is_err());
}

#[test]
fn composite_rename_selects_the_captured_parent_file_provider_not_the_logical_ast_language() {
    let root = tempfile::tempdir().unwrap();
    let source = "<script>\nfunction helper() {}\nfunction main(){helper();}\n</script>\n";
    fs::write(root.path().join("main.vue"), source).unwrap();
    let d = Deadline::new(20000);
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
                    "vue-renamer".into(),
                ],
                language: "vue".into(),
                trust: true,
            },
            configuration: yoke::lsp::HostConfiguration {
                passive_profile: Some(yoke::lsp::PassiveProfile::ContractFixture),
                ..Default::default()
            },
        })
        .unwrap();
    let (snapshot, graph, _, _) = engine.index(&d).unwrap();
    let entity = graph
        .entities
        .iter()
        .find(|entity| entity.name == "helper")
        .unwrap();
    assert_eq!(snapshot.files[0].language, "vue");
    assert!(["javascript", "typescript"].contains(&entity.language.as_str()));
    let value = engine
        .invoke(
            "ci_change_prepare",
            json!({"snapshotId":snapshot.id,"rename":{"entityId":entity.id,"newName":"renamed"}}),
        )
        .unwrap();
    assert_eq!(value["status"], "ok", "{value}");
    assert!(value["data"]["plan"].is_object(), "{value}");
    assert_eq!(
        fs::read_to_string(root.path().join("main.vue")).unwrap(),
        source
    );
}

#[test]
fn facade_requested_options_are_enforced_or_rejected_without_silent_dropping() {
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("main.rs"), "fn alpha() {}\n").unwrap();
    let d = Deadline::new(20000);
    let mut s = session(root.path(), HostProfile::default(), &d);
    let mut repl = AgentRepl::spawn(spec(), &d).unwrap();
    for code in [
        "s.lsp.find_symbol('alpha',include_info=True)",
        "s.lsp.find_symbol('alpha',max_answer_chars=1)",
        "s.mem.write_memory('too-long','long content',max_chars=1)",
    ] {
        let value = repl.execute(&mut s, code, &d).unwrap();
        assert!(
            value["error"].is_object(),
            "Silently dropped requested option: {code}: {value}"
        );
    }
    let overview = repl
        .execute(
            &mut s,
            "s.jb.get_symbols_overview('main.rs',include_file_documentation=True)",
            &d,
        )
        .unwrap();
    assert!(overview["error"].is_null(), "{overview}");
    assert!(overview["result"]["fileDocumentation"].is_null());
    let value = repl
        .execute(&mut s, "s.mem.list_memories()['items']", &d)
        .unwrap();
    assert_eq!(value["result"], json!([]));
}

#[test]
fn surrogate_dict_keys_are_normalized_without_losing_the_persistent_worker() {
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("main.rs"), "fn alpha() {}\n").unwrap();
    let d = Deadline::new(20000);
    let mut s = session(root.path(), HostProfile::default(), &d);
    let mut repl = AgentRepl::spawn(spec(), &d).unwrap();
    let value = repl.execute(&mut s, "{'\\ud800': 1}", &d).unwrap();
    assert!(value["result"].is_object());
    assert_eq!(repl.execute(&mut s, "42", &d).unwrap()["result"], 42);
}

#[test]
fn pure_facade_has_no_python_shell_or_caller_selected_host_authority() {
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("main.rs"), "fn alpha() {}\n").unwrap();
    let d = Deadline::new(20000);
    let mut s = session(root.path(), HostProfile::default(), &d);
    let value = yoke::agent_repl::invoke_facade(
        &mut s,
        "fs.read_file",
        json!({"relative_path":"main.rs"}),
        None,
        &d,
    )
    .unwrap();
    assert_eq!(value["text"], "fn alpha() {}\n");
    for (method, args) in [
        (
            "lsp.find_declaration",
            json!({"relative_path":"main.rs","regex":"(alpha)"}),
        ),
        (
            "shell.execute_shell_command",
            json!({"command":"echo prohibited"}),
        ),
        (
            "fs.read_file",
            json!({"relative_path":"main.rs","python":"host-injection"}),
        ),
    ] {
        assert!(yoke::agent_repl::invoke_facade(&mut s, method, args, None, &d).is_err());
    }
}

#[test]
fn memory_facade_global_namespace_uses_only_the_registered_global_store() {
    let root = tempfile::tempdir().unwrap();
    let global = tempfile::tempdir().unwrap();
    fs::write(root.path().join("main.rs"), "fn alpha() {}\n").unwrap();
    let d = Deadline::new(20000);
    let host = HostProfile {
        global_notes_root: Some(global.path().into()),
        ..Default::default()
    };
    let mut s = session(root.path(), host, &d);
    let mut repl = AgentRepl::spawn(spec(), &d).unwrap();
    let value=repl.execute(&mut s,"s.mem.write_memory('topic/local','local')\ns.mem.write_memory('global/topic/shared','shared')\ns.mem.rename_memory('global/topic/shared','global/topic/renamed')\n(s.mem.read_memory('global/topic/renamed')['note']['scope'],len(s.mem.list_memories(topic='global/topic')['items']),s.mem.read_memory('topic/local')['note']['scope'])",&d).unwrap();
    assert_eq!(value["result"], json!(["global", 1, "project"]), "{value}");
}

#[test]
fn semantic_refactoring_facades_check_modes_before_provider_queries_or_durable_plans() {
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("main.rs"), "fn helper() {}\n").unwrap();
    let deadline = Deadline::new(20000);
    let mut engine = Engine::open(root.path()).unwrap();
    engine
        .register_configured_server(yoke::lsp::ConfiguredServer {
            server: yoke::lsp::Server {
                command: "node".into(),
                args: vec![
                    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                        .join("tests/fixtures/refactor-lsp.mjs")
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
        HostProfile::default(),
        Some("refactor-modes"),
        &deadline,
    )
    .unwrap();
    session
        .invoke("switchModes", &json!({"modes":["planning"]}), &deadline)
        .unwrap();
    let error = yoke::agent_repl::invoke_facade(
        &mut session,
        "lsp.safe_delete_symbol",
        json!({"name_path_pattern":"helper","relative_path":"main.rs"}),
        None,
        &deadline,
    )
    .unwrap_err()
    .to_string();
    assert!(
        error.to_lowercase().contains("editing") || error.to_lowercase().contains("planning"),
        "missing permission preflight: {error}"
    );
    let count: i64 = session
        .engine
        .store
        .db
        .query_row(
            "SELECT count(*) FROM records WHERE kind='plan'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(count, 0);
    session
        .invoke("switchModes", &json!({"modes":["editing"]}), &deadline)
        .unwrap();
    let value = yoke::agent_repl::invoke_facade(
        &mut session,
        "lsp.safe_delete_symbol",
        json!({"name_path_pattern":"helper","relative_path":"main.rs"}),
        None,
        &deadline,
    )
    .unwrap();
    assert!(value["plan"]["id"].is_string(), "{value}");
    assert_eq!(
        fs::read_to_string(root.path().join("main.rs")).unwrap(),
        "fn helper() {}\n"
    );
}

#[test]
fn inspection_facades_are_source_bound_reads_in_planning_mode_with_honest_kind_registry() {
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("main.rs"), "fn helper() {}\n").unwrap();
    let deadline = Deadline::new(20000);
    let mut engine = Engine::open(root.path()).unwrap();
    engine
        .register_configured_server(yoke::lsp::ConfiguredServer {
            server: yoke::lsp::Server {
                command: "node".into(),
                args: vec![
                    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                        .join("tests/fixtures/refactor-lsp.mjs")
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
        HostProfile::default(),
        Some("inspection-facade"),
        &deadline,
    )
    .unwrap();
    session
        .invoke("switchModes", &json!({"modes":["planning"]}), &deadline)
        .unwrap();
    let result = yoke::agent_repl::invoke_facade(
        &mut session,
        "jb.run_inspections",
        json!({"relative_path":"main.rs","min_severity":"warning","inspection_names":["unused"]}),
        None,
        &deadline,
    )
    .unwrap();
    assert_eq!(result["items"].as_array().unwrap().len(), 1, "{result}");
    let registry = yoke::agent_repl::invoke_facade(
        &mut session,
        "jb.list_inspections",
        json!({"language":"rust","group_path_contains":"inline"}),
        None,
        &deadline,
    )
    .unwrap();
    assert_eq!(registry["items"][0]["kind"], "refactor.inline");
    assert_eq!(
        registry["registrySemantics"],
        "standard_lsp_declared_code_action_kinds_not_proprietary_inspection_catalog"
    );
    let count: i64 = session
        .engine
        .store
        .db
        .query_row(
            "SELECT count(*) FROM records WHERE kind='plan'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(count, 0);
}

#[test]
fn external_inspection_facades_query_only_registered_source_without_activating_or_preparing_plans()
{
    let root = tempfile::tempdir().unwrap();
    let external = tempfile::tempdir().unwrap();
    fs::write(root.path().join("main.rs"), "fn default_function() {}\n").unwrap();
    fs::write(external.path().join("main.rs"), "fn helper() {}\n").unwrap();
    let deadline = Deadline::new(20000);
    let mut engine = Engine::open(root.path()).unwrap();
    engine
        .register_configured_server(yoke::lsp::ConfiguredServer {
            server: yoke::lsp::Server {
                command: "node".into(),
                args: vec![
                    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                        .join("tests/fixtures/refactor-lsp.mjs")
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
            name: "external".into(),
            root: external.path().into(),
            read_only: true,
            query_only: true,
        }],
        ..Default::default()
    };
    let mut session = Session::open(engine, host, Some("external-inspection"), &deadline).unwrap();
    let result = yoke::agent_repl::invoke_facade(
        &mut session,
        "jb.run_inspections",
        json!({"relative_path":"main.rs"}),
        Some("external"),
        &deadline,
    )
    .unwrap();
    assert_eq!(
        result["items"][0]["contentHash"],
        yoke::model::hash("fn helper() {}\n")
    );
    assert_eq!(
        session
            .invoke("configuration", &json!({}), &deadline)
            .unwrap()["activeProject"],
        "default"
    );
    assert!(
        yoke::agent_repl::invoke_facade(
            &mut session,
            "jb.run_inspections",
            json!({"relative_path":"main.rs"}),
            Some("unregistered"),
            &deadline
        )
        .is_err()
    );
    let external_engine = Engine::open(external.path()).unwrap();
    let count: i64 = external_engine
        .store
        .db
        .query_row(
            "SELECT count(*) FROM records WHERE kind='plan'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(count, 0);
}
