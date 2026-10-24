use std::{fs, path::PathBuf};
use yoke::{
    Engine,
    capture::Deadline,
    model::Snapshot,
    runtime::{Breakpoint, Request, RuntimeSpec, Session},
};
fn spec(eval: bool) -> RuntimeSpec {
    RuntimeSpec {
        python: host_python(),
        trust_runtime: true,
        allow_evaluation: eval,
        max_output_bytes: 32_000,
    }
}
fn host_python() -> PathBuf {
    let names: &[&str] = if cfg!(windows) {
        &["python.exe", "python3.exe"]
    } else {
        &["python3", "python"]
    };
    let paths = std::env::var_os("PATH").expect("Actual host Python requires PATH");
    for directory in std::env::split_paths(&paths).filter(|p| p.is_absolute()) {
        for name in names {
            let candidate = directory.join(name);
            if candidate.is_file() {
                return candidate
                    .canonicalize()
                    .expect("Host Python canonical path");
            }
        }
    }
    panic!("Actual host Python 3.10+ is required for runtime tests");
}
fn fixture(text: &str) -> (tempfile::TempDir, Engine, Snapshot) {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("main.py"), text).unwrap();
    let mut engine = Engine::open(dir.path()).unwrap();
    let (s, _, _, _) = engine.index(&Deadline::new(10_000)).unwrap();
    (dir, engine, s)
}
#[test]
fn actual_debugger_pauses_steps_and_hits_hash_bound_unicode_breakpoints() {
    let text = "café = '🌍'\nvalue = 1\nvalue += 2\nprint(café, value)\n";
    let (dir, e, s) = fixture(text);
    let d = Deadline::new(15_000);
    let mut session = Session::start(&e.store, &s, "main.py", &spec(false), &d).unwrap();
    assert_eq!(session.state()["status"], "paused");
    assert_eq!(session.state()["frames"][0]["line"], 1);
    let f = s.files.iter().find(|f| f.path == "main.py").unwrap();
    session
        .request(Request::Breakpoints {
            items: vec![Breakpoint {
                path: f.path.clone(),
                content_hash: f.hash.clone(),
                line: 3,
            }],
        })
        .unwrap();
    let stopped = session.request(Request::Continue).unwrap();
    assert_eq!(stopped["status"], "paused");
    assert_eq!(stopped["frames"][0]["line"], 3);
    assert_eq!(stopped["frames"][0]["startByte"], 25);
    let vars = session.request(Request::Variables { frame: 0 }).unwrap();
    assert!(
        vars["variables"]
            .as_array()
            .unwrap()
            .iter()
            .any(|v| v["name"] == "café" && v["display"].as_str().unwrap().contains("🌍"))
    );
    let stepped = session.request(Request::Next).unwrap();
    assert_eq!(stepped["frames"][0]["line"], 4);
    let done = session.request(Request::Continue).unwrap();
    assert_eq!(done["status"], "finished");
    assert_eq!(done["programCompleted"], true);
    assert!(
        done["output"]
            .as_array()
            .unwrap()
            .iter()
            .any(|v| v["text"].as_str().unwrap().contains("🌍"))
    );
    assert_eq!(
        fs::read_to_string(dir.path().join("main.py")).unwrap(),
        text
    );
}
#[test]
fn runtime_and_evaluation_authority_are_distinct_and_source_cannot_grant_them() {
    let (_dir, e, s) = fixture("allow_evaluation = True\nvalue=1\n");
    let d = Deadline::new(10_000);
    let mut denied = spec(false);
    denied.trust_runtime = false;
    assert!(Session::start(&e.store, &s, "main.py", &denied, &d).is_err());
    let mut session = Session::start(&e.store, &s, "main.py", &spec(false), &d).unwrap();
    assert!(
        session
            .request(Request::Evaluate {
                expression: "1+1".into(),
                frame: 0
            })
            .is_err()
    );
    assert!(
        session
            .request(Request::Repl {
                code: "value=9".into(),
                frame: 0
            })
            .is_err()
    );
    assert!(serde_json::from_value::<Request>(serde_json::json!({"command":"evaluate","expression":"1+1","frame":0,"allowEvaluation":true})).is_err());
    session.request(Request::Close).unwrap();
}
#[test]
fn explicit_eval_and_repl_have_actual_module_global_semantics() {
    let (_dir, e, s) = fixture("value=2\nvalue+=1\n");
    let d = Deadline::new(10_000);
    let mut session = Session::start(&e.store, &s, "main.py", &spec(true), &d).unwrap();
    session.request(Request::Next).unwrap();
    let result = session
        .request(Request::Evaluate {
            expression: "value+3".into(),
            frame: 0,
        })
        .unwrap();
    assert_eq!(result["evaluation"]["display"], "5");
    let repl = session
        .request(Request::Repl {
            code: "extra='🌍'".into(),
            frame: 0,
        })
        .unwrap();
    assert_eq!(repl["replScope"], "selected_frame_module_globals");
    let result = session
        .request(Request::Evaluate {
            expression: "extra".into(),
            frame: 0,
        })
        .unwrap();
    assert!(
        result["evaluation"]["display"]
            .as_str()
            .unwrap()
            .contains("🌍")
    );
    session.request(Request::Close).unwrap();
}
#[test]
fn stale_breakpoints_forged_snapshots_and_relative_interpreters_are_denied() {
    let (_dir, e, s) = fixture("value=1\nvalue=2\n");
    let d = Deadline::new(10_000);
    let mut forged = s.clone();
    forged.files[0].hash = "0".repeat(64);
    assert!(Session::start(&e.store, &forged, "main.py", &spec(false), &d).is_err());
    let mut bad = spec(false);
    bad.python = PathBuf::from("python");
    assert!(Session::start(&e.store, &s, "main.py", &bad, &d).is_err());
    let mut session = Session::start(&e.store, &s, "main.py", &spec(false), &d).unwrap();
    for item in [
        Breakpoint {
            path: "main.py".into(),
            content_hash: "0".repeat(64),
            line: 1,
        },
        Breakpoint {
            path: "../escape.py".into(),
            content_hash: s.files[0].hash.clone(),
            line: 1,
        },
        Breakpoint {
            path: "main.py".into(),
            content_hash: s.files[0].hash.clone(),
            line: 99,
        },
    ] {
        assert!(
            session
                .request(Request::Breakpoints { items: vec![item] })
                .is_err()
        );
    }
    assert_eq!(session.request(Request::Stack).unwrap()["status"], "paused");
    session.request(Request::Close).unwrap();
}
#[test]
fn variable_inspection_does_not_execute_custom_repr() {
    let text = "class Dangerous:\n def __repr__(self):\n  open('side-effect','w').write('bad')\n  return 'bad'\nitem=Dangerous()\nvalue=2\n";
    let (_dir, e, s) = fixture(text);
    let d = Deadline::new(10_000);
    let mut session = Session::start(&e.store, &s, "main.py", &spec(false), &d).unwrap();
    let f = &s.files[0];
    session
        .request(Request::Breakpoints {
            items: vec![Breakpoint {
                path: "main.py".into(),
                content_hash: f.hash.clone(),
                line: 6,
            }],
        })
        .unwrap();
    session.request(Request::Continue).unwrap();
    let vars = session.request(Request::Variables { frame: 0 }).unwrap();
    assert!(
        vars["variables"]
            .as_array()
            .unwrap()
            .iter()
            .any(|v| v["name"] == "item" && v["kind"] == "opaque")
    );
    session.request(Request::Close).unwrap();
}
#[test]
fn deadline_and_output_bounds_terminate_sessions_without_source_writes() {
    for text in ["while True:\n pass\n", "print('X'*100000)\n"] {
        let (dir, e, s) = fixture(text);
        let d = Deadline::new(if text.starts_with("while") {
            5000
        } else {
            10000
        });
        let mut config = spec(false);
        config.max_output_bytes = 1024;
        let mut session = Session::start(&e.store, &s, "main.py", &config, &d).unwrap();
        assert!(session.request(Request::Continue).is_err());
        assert!(session.request(Request::Stack).is_err());
        assert_eq!(
            fs::read_to_string(dir.path().join("main.py")).unwrap(),
            text
        );
    }
}
#[test]
fn modifying_copied_source_invalidates_runtime_evidence() {
    let text = "open(__file__,'a').write('#changed')\nvalue=2\n";
    let (dir, e, s) = fixture(text);
    let d = Deadline::new(10_000);
    let mut session = Session::start(&e.store, &s, "main.py", &spec(false), &d).unwrap();
    assert!(session.request(Request::Next).is_err());
    assert!(session.request(Request::Stack).is_err());
    assert_eq!(
        fs::read_to_string(dir.path().join("main.py")).unwrap(),
        text
    );
}
#[test]
fn inspecting_a_metaclass_does_not_call_its_equality_hook() {
    let text = "class Meta(type):\n def __eq__(self, other):\n  open('side-effect','w').write('bad')\n  return False\nclass Dangerous(metaclass=Meta):\n pass\nitem=Dangerous()\nvalue=2\n";
    let (_dir, engine, snapshot) = fixture(text);
    let d = Deadline::new(15_000);
    let mut session =
        Session::start(&engine.store, &snapshot, "main.py", &spec(false), &d).unwrap();
    session
        .request(Request::Breakpoints {
            items: vec![Breakpoint {
                path: "main.py".into(),
                content_hash: snapshot.files[0].hash.clone(),
                line: 8,
            }],
        })
        .unwrap();
    session.request(Request::Continue).unwrap();
    let result = session.request(Request::Variables { frame: 0 }).unwrap();
    assert!(
        result["variables"]
            .as_array()
            .unwrap()
            .iter()
            .any(|v| v["name"] == "item" && v["kind"] == "opaque")
    );
    session.request(Request::Close).unwrap();
}
#[test]
fn program_exceptions_remain_program_outcomes_and_never_protocol_success() {
    let (_dir, engine, snapshot) = fixture("raise ValueError('expected program exception')\n");
    let d = Deadline::new(10_000);
    let mut session =
        Session::start(&engine.store, &snapshot, "main.py", &spec(false), &d).unwrap();
    let result = session.request(Request::Continue).unwrap();
    assert_eq!(result["status"], "finished");
    assert_eq!(result["programCompleted"], false);
    assert_eq!(result["errorKind"], "program_exception");
    assert_eq!(result["verificationAttestation"], false);
}

#[test]
fn variable_inspection_does_not_execute_equality_of_non_string_global_keys() {
    let text = "class Collision:\n def __hash__(self): return hash('item')\n def __eq__(self, other):\n  if globals().get('armed', False): open('side-effect','w').write('bad')\n  return False\nkey=Collision()\nglobals()[key]=123\nitem=42\narmed=True\nvalue=2\n";
    let (_dir, engine, snapshot) = fixture(text);
    let deadline = Deadline::new(15_000);
    let mut session =
        Session::start(&engine.store, &snapshot, "main.py", &spec(false), &deadline).unwrap();
    session
        .request(Request::Breakpoints {
            items: vec![Breakpoint {
                path: "main.py".into(),
                content_hash: snapshot.files[0].hash.clone(),
                line: 10,
            }],
        })
        .unwrap();
    session.request(Request::Continue).unwrap();
    // The source arms its equality hook only after populating globals. Passive
    // inspection must not trigger it through a dictionary collision lookup.
    let result = session.request(Request::Variables { frame: 0 }).unwrap();
    assert_eq!(result["sourceIntegrity"], true);
    assert!(result["variables"].as_array().unwrap().iter().any(|value| {
        value["name"] == "item" && value["kind"] == "int" && value["display"] == "42"
    }));
    session.request(Request::Close).unwrap();
}

#[test]
fn stack_metadata_does_not_execute_string_subclass_hooks() {
    let text = "class Name(str):\n def __getitem__(self, index):\n  open('side-effect','w').write('bad')\n  return super().__getitem__(index)\n def __len__(self):\n  open('side-effect','w').write('bad')\n  return super().__len__()\n def __str__(self):\n  open('side-effect','w').write('bad')\n  return super().__str__()\ndef target():\n value=1\n return value\ntarget.__code__=target.__code__.replace(co_name=Name('target'))\ntarget()\n";
    let (_dir, engine, snapshot) = fixture(text);
    let deadline = Deadline::new(15_000);
    let mut session =
        Session::start(&engine.store, &snapshot, "main.py", &spec(false), &deadline).unwrap();
    session
        .request(Request::Breakpoints {
            items: vec![Breakpoint {
                path: "main.py".into(),
                content_hash: snapshot.files[0].hash.clone(),
                line: 12,
            }],
        })
        .unwrap();
    let result = session.request(Request::Continue).unwrap();
    assert_eq!(result["frames"][0]["function"], "target");
    assert_eq!(result["frames"][0]["functionNameTruncated"], false);
    assert_eq!(result["sourceIntegrity"], true);
    assert_eq!(
        session.request(Request::Stack).unwrap()["sourceIntegrity"],
        true
    );
    session.request(Request::Close).unwrap();
}

#[test]
fn variable_inspection_bypasses_custom_dictionary_accessors() {
    let mut text = String::from(
        "class Locals(dict):\n def __getitem__(self, key):\n  open('side-effect','w').write('bad')\n  return super().__getitem__(key)\n def items(self):\n  open('side-effect','w').write('bad')\n  return super().items()\nsandbox=Locals()\ncode=compile('\\n'*16+'value=42\\nvalue+=1\\n', __file__, 'exec')\nexec(code, globals(), sandbox)\n",
    );
    while text.lines().count() < 18 {
        text.push_str("pass\n");
    }
    let (_dir, engine, snapshot) = fixture(&text);
    let deadline = Deadline::new(15_000);
    let mut session =
        Session::start(&engine.store, &snapshot, "main.py", &spec(false), &deadline).unwrap();
    session
        .request(Request::Breakpoints {
            items: vec![Breakpoint {
                path: "main.py".into(),
                content_hash: snapshot.files[0].hash.clone(),
                line: 18,
            }],
        })
        .unwrap();
    // Python permits custom locals dictionaries for exec. Inspect backing
    // storage without invoking their overridden lookup or items methods.
    session.request(Request::Continue).unwrap();
    let result = session.request(Request::Variables { frame: 0 }).unwrap();
    assert_eq!(result["sourceIntegrity"], true);
    assert_eq!(
        result["variableSemantics"],
        "dictionary_storage_no_mapping_hooks"
    );
    assert!(result["variables"].as_array().unwrap().iter().any(|value| {
        value["name"] == "value" && value["kind"] == "int" && value["display"] == "42"
    }));
    session.request(Request::Close).unwrap();
}

#[test]
fn ordinary_function_locals_use_intrinsic_frame_storage() {
    let (_dir, engine, snapshot) = fixture("def target():\n answer=42\n return answer\ntarget()\n");
    let deadline = Deadline::new(15_000);
    let mut session =
        Session::start(&engine.store, &snapshot, "main.py", &spec(false), &deadline).unwrap();
    session
        .request(Request::Breakpoints {
            items: vec![Breakpoint {
                path: "main.py".into(),
                content_hash: snapshot.files[0].hash.clone(),
                line: 3,
            }],
        })
        .unwrap();
    let paused = session.request(Request::Continue).unwrap();
    assert_eq!(paused["frames"][0]["function"], "target");
    let result = session.request(Request::Variables { frame: 0 }).unwrap();
    assert_eq!(result["inspectionComplete"], true);
    assert!(matches!(
        result["variableSemantics"].as_str(),
        Some("dictionary_storage_no_mapping_hooks" | "frame_local_storage_no_mapping_hooks")
    ));
    assert!(
        result["variables"]
            .as_array()
            .unwrap()
            .iter()
            .any(|value| { value["name"] == "answer" && value["display"] == "42" })
    );
    session.request(Request::Close).unwrap();
}

#[test]
fn unsupported_custom_locals_are_explicit_failed_operations_without_hooks() {
    let mut text = String::from(
        "from collections import UserDict\nclass Locals(UserDict):\n def items(self):\n  open('side-effect','w').write('bad')\n  return super().items()\nsandbox=Locals()\ncode=compile('\\n'*18+'value=42\\nvalue+=1\\n', __file__, 'exec')\nexec(code, globals(), sandbox)\n",
    );
    while text.lines().count() < 20 {
        text.push_str("pass\n");
    }
    let (_dir, engine, snapshot) = fixture(&text);
    let deadline = Deadline::new(15_000);
    let mut session =
        Session::start(&engine.store, &snapshot, "main.py", &spec(false), &deadline).unwrap();
    session
        .request(Request::Breakpoints {
            items: vec![Breakpoint {
                path: "main.py".into(),
                content_hash: snapshot.files[0].hash.clone(),
                line: 20,
            }],
        })
        .unwrap();
    session.request(Request::Continue).unwrap();
    let result = session.request(Request::Variables { frame: 0 }).unwrap();
    assert_eq!(
        result["variableSemantics"],
        "unsupported_custom_locals_mapping"
    );
    assert_eq!(result["inspectionComplete"], false);
    assert_eq!(result["operationCompleted"], false);
    assert_eq!(result["sourceIntegrity"], true);
    assert_eq!(session.request(Request::Stack).unwrap()["status"], "paused");
    session.request(Request::Close).unwrap();
}
