use serde_json::{Value, json};
use std::{
    fs,
    io::{Read, Write},
    net::SocketAddr,
};
use yoke::{
    Engine,
    capture::Deadline,
    project_session::{HostProfile, Session},
    session_config::{self, Source, Target},
};

const SOURCE: &str = "// owned \u{1f4a1} e\u{301}\r\nfn visible() {}\r\n";

fn host(chars: Option<u64>, timeout: Option<u64>) -> HostProfile {
    let mut limits = json!({});
    if let Some(n) = chars {
        limits["defaultAnswerChars"] = json!(n);
    }
    if let Some(n) = timeout {
        limits["timeoutMs"] = json!(n);
    }
    serde_json::from_value(json!({"toolBudgets": limits})).unwrap()
}
fn setup(profile: HostProfile, id: &str, d: &Deadline) -> (tempfile::TempDir, Session) {
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("main.rs"), SOURCE).unwrap();
    let session = Session::open(Engine::open(root.path()).unwrap(), profile, Some(id), d).unwrap();
    (root, session)
}
fn projection(
    base: &HostProfile,
    target: &Target,
    bytes: &[u8],
    d: &Deadline,
) -> anyhow::Result<HostProfile> {
    let hash = yoke::model::hash(bytes);
    Ok(session_config::project(
        base,
        &[Source {
            target,
            expected_sha256: &hash,
            bytes,
        }],
        d,
    )?
    .profile()
    .clone())
}

#[test]
fn global_yaml_projects_independent_limits_rounds_positive_time_up_and_clears_explicitly() {
    let d = Deadline::new(20000);
    let base = HostProfile::default();
    let selected = projection(
        &base,
        &Target::Global,
        b"tool_timeout: 0.00001\ndefault_max_tool_answer_chars: 0\n",
        &d,
    )
    .unwrap();
    assert_eq!(
        serde_json::to_value(&selected).unwrap()["toolBudgets"],
        json!({"timeoutMs":1,"defaultAnswerChars":0})
    );
    let cleared = projection(
        &selected,
        &Target::Global,
        b"tool_timeout: null\ndefault_max_tool_answer_chars: null\n",
        &d,
    )
    .unwrap();
    assert_eq!(
        serde_json::to_value(cleared).unwrap(),
        serde_json::to_value(base).unwrap()
    );
    let projected = projection(
        &host(Some(150000), None),
        &Target::Global,
        b"tool_timeout: 240\n",
        &d,
    )
    .unwrap();
    assert_eq!(
        serde_json::to_value(projected).unwrap()["toolBudgets"],
        json!({"timeoutMs":240000,"defaultAnswerChars":150000})
    );
}

#[test]
fn invalid_scope_type_capacity_and_underflow_refuse_before_selected_prompt_renderer() {
    let d = Deadline::new(20000);
    for bytes in [
        "tool_timeout: 0\n",
        "tool_timeout: -1\n",
        "tool_timeout: 600.001\n",
        "tool_timeout: true\n",
        "tool_timeout: '1'\n",
        "tool_timeout: 1e-999\n",
        "tool_timeout: .inf\n",
        "default_max_tool_answer_chars: -1\n",
        "default_max_tool_answer_chars: 4000001\n",
        "default_max_tool_answer_chars: true\n",
        "default_max_tool_answer_chars: 1.5\n",
    ] {
        assert!(
            projection(
                &HostProfile::default(),
                &Target::Global,
                bytes.as_bytes(),
                &d
            )
            .is_err(),
            "{bytes}"
        );
    }
    for target in [
        Target::Project {
            project: "default".into(),
        },
        Target::ProjectLocal {
            project: "default".into(),
        },
        Target::Context {
            name: "owned".into(),
        },
        Target::Mode {
            name: "owned".into(),
        },
    ] {
        for key in ["tool_timeout", "default_max_tool_answer_chars"] {
            let text = format!("{key}: 1\nprompt: '{{{{ value }}}}'\n");
            let hash = yoke::model::hash(&text);
            let mut calls = 0;
            let mut renderer = |_: &str, _: &Deadline| {
                calls += 1;
                Ok(session_config::RenderedPrompt {
                    text: "rendered".into(),
                    evidence: json!({}),
                })
            };
            assert!(
                session_config::project_with_prompt_renderer(
                    &HostProfile::default(),
                    &[Source {
                        target: &target,
                        expected_sha256: &hash,
                        bytes: text.as_bytes()
                    }],
                    &d,
                    &mut renderer
                )
                .is_err()
            );
            assert_eq!(calls, 0);
        }
    }
    for value in [
        json!({"timeoutMs":0}),
        json!({"timeoutMs":600001}),
        json!({"defaultAnswerChars":4000001}),
        json!({"timeoutMs":true}),
        json!({"unknown":1}),
    ] {
        let value = json!({"toolBudgets":value});
        if let Ok(host) = serde_json::from_value::<HostProfile>(value) {
            let root = tempfile::tempdir().unwrap();
            assert!(
                Session::open(
                    Engine::open(root.path()).unwrap(),
                    host,
                    Some("invalid"),
                    &d
                )
                .is_err()
            );
        }
    }
}

#[test]
fn selected_policy_binds_reopening_while_empty_selection_preserves_old_host_and_session() {
    let d = Deadline::new(20000);
    assert_eq!(
        serde_json::to_value(host(None, None)).unwrap(),
        serde_json::to_value(HostProfile::default()).unwrap()
    );
    let (root, session) = setup(HostProfile::default(), "legacy", &d);
    drop(session);
    let old = Session::open(
        Engine::open(root.path()).unwrap(),
        host(None, None),
        Some("legacy"),
        &d,
    )
    .unwrap();
    drop(old);
    assert!(
        Session::open(
            Engine::open(root.path()).unwrap(),
            host(Some(1000), None),
            Some("legacy"),
            &d
        )
        .is_err()
    );
    let (root, mut session) = setup(host(Some(1000), Some(240000)), "selected", &d);
    assert_eq!(
        session.invoke("configuration", &json!({}), &d).unwrap()["toolBudgets"],
        json!({"timeoutMs":240000,"defaultAnswerChars":1000})
    );
    drop(session);
    assert!(
        Session::open(
            Engine::open(root.path()).unwrap(),
            host(Some(1001), Some(240000)),
            Some("selected"),
            &d
        )
        .is_err()
    );
    assert_eq!(
        fs::read(root.path().join("main.rs")).unwrap(),
        SOURCE.as_bytes()
    );
}

#[test]
fn facade_and_public_tools_resolve_missing_and_minus_one_but_keep_exact_per_call_overrides() {
    let d = Deadline::new(20000);
    let (_legacy_root, mut legacy) = setup(HostProfile::default(), "legacy-read", &d);
    let args = json!({"relative_path":"main.rs","max_answer_chars":-1});
    yoke::agent_repl::invoke_facade(&mut legacy, "fs.read_file", args.clone(), None, &d).unwrap();
    let (root, mut selected) = setup(host(Some(0), None), "selected-read", &d);
    let expected = yoke::agent_repl::invoke_facade(
        &mut selected,
        "fs.read_file",
        json!({"relative_path":"main.rs","max_answer_chars":4000000}),
        None,
        &d,
    )
    .unwrap();
    let length = serde_json::to_string(&expected).unwrap().chars().count() as u64;
    for request in [
        json!({"relative_path":"main.rs"}),
        args.clone(),
        json!({"relative_path":"main.rs","max_answer_chars":0}),
    ] {
        let error = yoke::compatibility::invoke_serena(&mut selected, "read_file", &request, &d)
            .unwrap_err();
        assert!(
            error.to_string().contains("max_answer_chars=0"),
            "{error:#}"
        );
    }
    for public in [false, true] {
        let request = json!({"relative_path":"main.rs","max_answer_chars":length});
        let value = if public {
            yoke::compatibility::invoke_serena(&mut selected, "read_file", &request, &d)
        } else {
            yoke::agent_repl::invoke_facade(&mut selected, "fs.read_file", request, None, &d)
        }
        .unwrap();
        assert_eq!(value, expected);
    }
    assert!(
        yoke::compatibility::invoke_serena(
            &mut selected,
            "read_file",
            &json!({"relative_path":"main.rs","max_answer_chars":length-1}),
            &d
        )
        .is_err()
    );
    assert!(
        yoke::compatibility::invoke_serena(&mut selected, "get_current_config", &json!({}), &d)
            .is_ok()
    );
    assert_eq!(
        fs::read(root.path().join("main.rs")).unwrap(),
        SOURCE.as_bytes()
    );
}

#[test]
fn real_stdio_retains_typed_error_and_allows_the_declared_answer_override() {
    let d = Deadline::new(20000);
    let (_root, mut session) = setup(host(Some(0), Some(240000)), "stdio-limits", &d);
    let rows = [
        json!({"jsonrpc":"2.0","id":1,"method":"initialize"}),
        json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"read_file","arguments":{"relative_path":"main.rs"}}}),
        json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"read_file","arguments":{"relative_path":"main.rs","max_answer_chars":4000000}}}),
    ];
    let input: String = rows.iter().map(|r| r.to_string() + "\n").collect();
    let mut output = Vec::new();
    yoke::protocol::serve_serena(&mut session, std::io::Cursor::new(input), &mut output).unwrap();
    let values: Vec<Value> = String::from_utf8(output)
        .unwrap()
        .lines()
        .map(|s| serde_json::from_str(s).unwrap())
        .collect();
    assert_eq!(values[1]["result"]["isError"], true);
    assert!(
        values[1]["result"]["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains("max_answer_chars=0")
    );
    assert_eq!(values[2]["result"]["isError"], false);
}

#[test]
fn trusted_persistent_callback_uses_selected_default_and_whole_cell_deadline_loses_worker() {
    let d = Deadline::new(30000);
    let (root, mut session) = setup(host(Some(0), Some(5000)), "repl-limits", &d);
    let python = std::process::Command::new("python")
        .args(["-I", "-c", "import sys;print(sys.executable)"])
        .output()
        .unwrap();
    assert!(python.status.success());
    let spec = yoke::agent_repl::ReplSpec {
        python: String::from_utf8(python.stdout).unwrap().trim().into(),
        trust_python: true,
        max_output_bytes: 1000000,
        shell: None,
    };
    let mut runtime = yoke::agent_repl::AgentRepl::spawn(spec, &d).unwrap();
    let response = runtime
        .execute(&mut session, "s.fs.read_file(relative_path='main.rs')", &d)
        .unwrap();
    assert!(
        response["error"]
            .as_str()
            .unwrap()
            .contains("max_answer_chars=0")
    );
    let response = runtime
        .execute(
            &mut session,
            "s.fs.read_file(relative_path='main.rs',max_answer_chars=4000000)",
            &d,
        )
        .unwrap();
    assert!(response["error"].is_null());
    let started = std::time::Instant::now();
    assert!(
        runtime
            .execute(&mut session, "import time\ntime.sleep(10)\n42", &d)
            .is_err()
    );
    assert!(started.elapsed() < std::time::Duration::from_secs(9));
    assert!(runtime.execute(&mut session, "42", &d).is_err());
    runtime.close().unwrap();
    assert_eq!(
        fs::read(root.path().join("main.rs")).unwrap(),
        SOURCE.as_bytes()
    );
}

fn post(address: SocketAddr, session: Option<&str>, body: Value) -> (u16, Option<String>, Value) {
    let bytes = serde_json::to_vec(&body).unwrap();
    let session = session
        .map(|id| format!("Mcp-Session-Id: {id}\r\nMcp-Protocol-Version: 2025-11-25\r\n"))
        .unwrap_or_default();
    let mut stream = std::net::TcpStream::connect(address).unwrap();
    stream
        .set_read_timeout(Some(std::time::Duration::from_secs(10)))
        .unwrap();
    write!(stream,"POST /mcp HTTP/1.1\r\nHost: {address}\r\nAuthorization: Bearer {}\r\n{session}Accept: application/json, text/event-stream\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n", hex::encode([51;32]), bytes.len()).unwrap();
    stream.write_all(&bytes).unwrap();
    let mut output = Vec::new();
    loop {
        let mut block = [0; 8192];
        let n = stream.read(&mut block).unwrap();
        assert_ne!(n, 0);
        output.extend_from_slice(&block[..n]);
        assert!(output.len() <= 1_000_000);
        if let Some(end) = output.windows(4).position(|b| b == b"\r\n\r\n") {
            let header = std::str::from_utf8(&output[..end]).unwrap();
            let length: usize = header
                .lines()
                .filter_map(|s| s.split_once(':'))
                .find(|(k, _)| k.eq_ignore_ascii_case("content-length"))
                .unwrap()
                .1
                .trim()
                .parse()
                .unwrap();
            if output.len() >= end + 4 + length {
                let id = header
                    .lines()
                    .filter_map(|s| s.split_once(':'))
                    .find(|(k, _)| k.eq_ignore_ascii_case("mcp-session-id"))
                    .map(|(_, v)| v.trim().to_owned());
                return (
                    header.split_whitespace().nth(1).unwrap().parse().unwrap(),
                    id,
                    if length == 0 {
                        Value::Null
                    } else {
                        serde_json::from_slice(&output[end + 4..end + 4 + length]).unwrap()
                    },
                );
            }
        }
    }
}

#[test]
fn real_authenticated_http_preserves_selected_default_and_per_call_override() {
    use yoke::mcp_http::{self, Config, Principal, Target};
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("main.rs"), SOURCE).unwrap();
    let target = Target::new(
        root.path(),
        yoke::protocol::Profile::Serena,
        host(Some(0), Some(240000)),
    );
    let principal = Principal::new(
        "owned-limits",
        zeroize::Zeroizing::new(vec![51; 32]),
        target,
    )
    .unwrap();
    let server = mcp_http::serve(Config::default(), vec![principal]).unwrap();
    let address = server.local_addr();
    let (status, id, _) = post(
        address,
        None,
        json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"owned-limits","version":"1"}}}),
    );
    assert_eq!(status, 200);
    let id = id.unwrap();
    assert_eq!(
        post(
            address,
            Some(&id),
            json!({"jsonrpc":"2.0","method":"notifications/initialized"})
        )
        .0,
        202
    );
    for (sequence, override_chars, error) in [
        (2, None, true),
        (3, Some(-1), true),
        (4, Some(4000000), false),
    ] {
        let mut args = json!({"relative_path":"main.rs"});
        if let Some(n) = override_chars {
            args["max_answer_chars"] = json!(n);
        }
        let (status, _, response) = post(
            address,
            Some(&id),
            json!({"jsonrpc":"2.0","id":sequence,"method":"tools/call","params":{"name":"read_file","arguments":args}}),
        );
        assert_eq!(status, 200);
        assert_eq!(response["result"]["isError"], error, "{response}");
        if error {
            assert!(
                response["result"]["content"][0]["text"]
                    .as_str()
                    .unwrap()
                    .contains("max_answer_chars=0")
            );
        }
    }
    server.shutdown().unwrap();
    assert_eq!(
        fs::read(root.path().join("main.rs")).unwrap(),
        SOURCE.as_bytes()
    );
}
