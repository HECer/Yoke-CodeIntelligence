//! Authored runtime qualification; never label these executed from source checks.
use serde_json::{Value, json};
use std::{
    fs,
    io::{Read, Write},
    net::{Shutdown, SocketAddr, TcpStream},
    process::Command,
    time::Duration,
};
use yoke::{
    Engine,
    capture::Deadline,
    project_session::{HostProfile, Session},
    session_config::{self, Source, Target},
    tool_usage::Estimator,
};
const SOURCE: &str = "// owned 💡 e\u{301}\r\nfn visible() {}\r\n";
fn d() -> Deadline {
    Deadline::new(30000)
}
fn root() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("main.rs"), SOURCE).unwrap();
    dir
}
fn host(estimator: Estimator) -> HostProfile {
    serde_json::from_value(json!({"tokenCountEstimator":estimator})).unwrap()
}
fn session(root: &std::path::Path, profile: HostProfile, label: &str) -> Session {
    Session::open(Engine::open(root).unwrap(), profile, Some(label), &d()).unwrap()
}
fn stats(s: &mut Session) -> Value {
    s.invoke("toolUsage", &json!({}), &d()).unwrap()
}
fn call(s: &mut Session, name: &str, args: Value) -> anyhow::Result<Value> {
    yoke::compatibility::invoke_serena(s, name, &args, &d())
}

#[test]
fn global_yaml_all_estimators_clear_unknown_scope_and_host_authority_are_distinct() {
    for name in ["CHAR_COUNT", "TIKTOKEN_GPT4O", "ANTHROPIC_CLAUDE_SONNET_4"] {
        let bytes = format!("token_count_estimator: {name}\n");
        let hash = yoke::model::hash(&bytes);
        let p = session_config::project(
            &HostProfile::default(),
            &[Source {
                target: &Target::Global,
                expected_sha256: &hash,
                bytes: bytes.as_bytes(),
            }],
            &d(),
        )
        .unwrap();
        assert_eq!(
            serde_json::to_value(p.profile()).unwrap()["tokenCountEstimator"],
            name
        );
        assert!(p.profile().token_count_runtime.is_none());
        assert_eq!(
            p.plan()["sources"][0]["tokenCountEstimatorDeclaration"]["networkAuthority"],
            false
        );
        let clear = b"token_count_estimator: null\n";
        let hash = yoke::model::hash(clear);
        let cleared = session_config::project(
            p.profile(),
            &[Source {
                target: &Target::Global,
                expected_sha256: &hash,
                bytes: clear,
            }],
            &d(),
        )
        .unwrap();
        assert_eq!(
            serde_json::to_value(cleared.profile()).unwrap(),
            serde_json::to_value(HostProfile::default()).unwrap()
        );
    }
    for text in [
        "token_count_estimator: true\n",
        "token_count_estimator: 1\n",
        "token_count_estimator: unknown\n",
        "token_count_estimator: {}\n",
    ] {
        let hash = yoke::model::hash(text);
        assert!(
            session_config::project(
                &HostProfile::default(),
                &[Source {
                    target: &Target::Global,
                    expected_sha256: &hash,
                    bytes: text.as_bytes()
                }],
                &d()
            )
            .is_err()
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
        let text = b"token_count_estimator: CHAR_COUNT\nprompt: '{{ data }}'\n";
        let hash = yoke::model::hash(text);
        let mut calls = 0;
        let mut render = |_: &str, _: &Deadline| {
            calls += 1;
            Ok(session_config::RenderedPrompt {
                text: "literal".into(),
                evidence: json!({}),
            })
        };
        assert!(
            session_config::project_with_prompt_renderer(
                &HostProfile::default(),
                &[Source {
                    target: &target,
                    expected_sha256: &hash,
                    bytes: text
                }],
                &d(),
                &mut render
            )
            .is_err()
        );
        assert_eq!(calls, 0);
    }
    let r = root();
    assert!(
        Session::open(
            Engine::open(r.path()).unwrap(),
            host(Estimator::AnthropicClaudeSonnet4),
            Some("no-grant"),
            &d()
        )
        .is_err()
    );
    if let Ok(profile) = serde_json::from_value::<HostProfile>(
        json!({"tokenCountEstimator":"ANTHROPIC_CLAUDE_SONNET_4","tokenCountRuntime":{"allowed":true}}),
    ) {
        assert!(profile.token_count_runtime.is_none());
        assert!(
            Session::open(
                Engine::open(r.path()).unwrap(),
                profile,
                Some("forged"),
                &d()
            )
            .is_err()
        );
    }
}
#[test]
fn absent_null_policy_preserves_legacy_bytes_binding_metrics_and_output() {
    let r = root();
    let mut original = session(r.path(), HostProfile::default(), "legacy");
    let before = original.invoke("configuration", &json!({}), &d()).unwrap();
    drop(original);
    let profile: HostProfile = serde_json::from_value(json!({"tokenCountEstimator":null})).unwrap();
    assert_eq!(
        serde_json::to_value(&profile).unwrap(),
        serde_json::to_value(HostProfile::default()).unwrap()
    );
    let mut same = session(r.path(), profile, "legacy");
    assert_eq!(
        same.invoke("configuration", &json!({}), &d()).unwrap(),
        before
    );
    assert!(same.invoke("toolUsage", &json!({}), &d()).is_err());
    assert!(
        call(&mut same, "get_current_config", json!({}))
            .unwrap()
            .get("toolUsageStatistics")
            .is_none()
    );
    let response = same.engine.invoke("ci_status", json!({})).unwrap();
    assert_eq!(response["metrics"]["tokenMethod"], "utf8_bytes_div_3");
    assert_eq!(response["metrics"]["tokenCount"], "estimated");
}
#[test]
fn successful_public_call_counts_once_without_storing_text_failed_calls_do_not_increment() {
    let r = root();
    let mut s = session(r.path(), host(Estimator::CharCount), "counts");
    let args = json!({"relative_path":"main.rs"});
    let result = call(&mut s, "read_file", args.clone()).unwrap();
    let current = stats(&mut s);
    assert_eq!(current["tools"]["read_file"]["numTimesCalled"], 1);
    assert_eq!(
        current["tools"]["read_file"]["inputTokens"],
        serde_json::to_string(&args).unwrap().chars().count() / 4
    );
    assert_eq!(
        current["tools"]["read_file"]["outputTokens"],
        serde_json::to_string(&result).unwrap().chars().count() / 4
    );
    assert!(!current.to_string().contains("visible"));
    assert!(call(&mut s, "read_file", json!({"relative_path":"missing.rs"})).is_err());
    assert_eq!(stats(&mut s), current);
    assert!(
        s.invoke("toolUsage", &json!({"source":"foreign"}), &d())
            .is_err()
    );
    assert!(
        s.invoke("clearToolUsage", &json!({"source":"foreign"}), &d())
            .is_err()
    );
    let revision = s.invoke("configuration", &json!({}), &d()).unwrap()["revision"].clone();
    s.invoke("clearToolUsage", &json!({}), &d()).unwrap();
    assert!(stats(&mut s)["tools"].as_object().unwrap().is_empty());
    assert_eq!(
        s.invoke("configuration", &json!({}), &d()).unwrap()["revision"],
        revision
    );
    assert_eq!(
        fs::read(r.path().join("main.rs")).unwrap(),
        SOURCE.as_bytes()
    );
}
#[test]
fn repl_fallback_counts_one_public_operation_and_local_tiktoken_reaches_real_dispatch() {
    let r = root();
    let mut s = session(r.path(), host(Estimator::TiktokenGpt4o), "repl");
    let python = Command::new("python")
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
    let mut runtime = yoke::agent_repl::AgentRepl::spawn(spec, &d()).unwrap();
    yoke::compatibility::invoke_serena_with_repl(
        &mut s,
        &mut runtime,
        "read_file",
        &json!({"relative_path":"main.rs"}),
        &d(),
    )
    .unwrap();
    assert_eq!(stats(&mut s)["tools"]["read_file"]["numTimesCalled"], 1);
    assert_eq!(stats(&mut s)["estimator"], "TIKTOKEN_GPT4O");
    runtime.close().unwrap();
    assert_eq!(
        fs::read(r.path().join("main.rs")).unwrap(),
        SOURCE.as_bytes()
    );
}
#[test]
fn policy_binding_and_concurrent_session_updates_refuse_without_statistics_mutation() {
    let r = root();
    let profile = host(Estimator::CharCount);
    let mut current = session(r.path(), profile.clone(), "stale");
    let mut stale = session(r.path(), profile, "stale");
    current
        .invoke("switchModes", &json!({"modes":["planning"]}), &d())
        .unwrap();
    assert!(call(&mut stale, "read_file", json!({"relative_path":"main.rs"})).is_err());
    assert!(stale.invoke("toolUsage", &json!({}), &d()).is_err());
    drop(stale);
    drop(current);
    assert!(
        Session::open(
            Engine::open(r.path()).unwrap(),
            HostProfile::default(),
            Some("stale"),
            &d()
        )
        .is_err()
    );
    let mut resumed = session(r.path(), host(Estimator::CharCount), "stale");
    assert!(stats(&mut resumed)["tools"].as_object().unwrap().is_empty());
}
#[test]
fn stats_only_cli_inspection_cannot_enter_activation_or_publish_session_state() {
    let r = root();
    let policy = r.path().join("host.json");
    let requests = r.path().join("requests.json");
    fs::write(&policy,serde_json::to_vec(&json!({"tokenCountEstimator":"CHAR_COUNT","activationCommands":{"global":{"activation_command":"OWNED_CANARY_MUST_NOT_EXECUTE"}}})).unwrap()).unwrap();
    fs::write(
        &requests,
        serde_json::to_vec(
            &json!([{"operation":"toolUsage","args":{}},{"operation":"clearToolUsage","args":{}}]),
        )
        .unwrap(),
    )
    .unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_yoke"))
        .arg("--root")
        .arg(r.path())
        .args(["session", "--id", "passive-stats", "--host-profile"])
        .arg(policy)
        .arg("--requests")
        .arg(requests)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let engine = Engine::open(r.path()).unwrap();
    let rows: u64 = engine
        .store
        .db
        .query_row(
            "SELECT count(*) FROM records WHERE kind=?1",
            [yoke::project_session::RECORD_KIND],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(rows, 0);
    assert_eq!(
        fs::read(r.path().join("main.rs")).unwrap(),
        SOURCE.as_bytes()
    );
}
#[test]
fn real_stdio_reports_prior_completed_calls_without_counter_recursion() {
    let r = root();
    let mut s = session(r.path(), host(Estimator::CharCount), "stdio");
    let input=[json!({"jsonrpc":"2.0","id":1,"method":"initialize"}),json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"read_file","arguments":{"relative_path":"main.rs"}}}),json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"get_current_config","arguments":{}}})].iter().map(|v|v.to_string()+"\n").collect::<String>();
    let mut output = Vec::new();
    yoke::protocol::serve_serena(&mut s, std::io::Cursor::new(input), &mut output).unwrap();
    let rows: Vec<Value> = String::from_utf8(output)
        .unwrap()
        .lines()
        .map(|s| serde_json::from_str(s).unwrap())
        .collect();
    assert_eq!(rows[1]["result"]["isError"], false);
    let config: Value =
        serde_json::from_str(rows[2]["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(config["status"], "ok");
    assert_eq!(
        config["data"]["toolUsageStatistics"]["tools"]["read_file"]["numTimesCalled"],
        1
    );
    assert!(
        config["data"]["toolUsageStatistics"]["tools"]
            .get("get_current_config")
            .is_none()
    );
    let current = stats(&mut s);
    assert_eq!(current["tools"]["get_current_config"]["numTimesCalled"], 1);
}

fn dashboard_state(info: &Value) -> Value {
    let url = url::Url::parse(info["url"].as_str().unwrap()).unwrap();
    let address: SocketAddr = url.socket_addrs(|| None).unwrap()[0];
    let token = url.fragment().unwrap();
    let mut stream = TcpStream::connect_timeout(&address, Duration::from_secs(3)).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(7)))
        .unwrap();
    write!(
        stream,
        "GET /api/state HTTP/1.1\r\nHost: {address}\r\nAuthorization: Bearer {token}\r\n\r\n"
    )
    .unwrap();
    stream.shutdown(Shutdown::Write).unwrap();
    let mut bytes = Vec::new();
    if let Err(error) = stream.take(400_001).read_to_end(&mut bytes) {
        assert_eq!(error.kind(), std::io::ErrorKind::ConnectionReset);
    }
    let end = bytes.windows(4).position(|s| s == b"\r\n\r\n").unwrap();
    let header = std::str::from_utf8(&bytes[..end]).unwrap();
    assert!(header.starts_with("HTTP/1.1 200 "));
    let length = header
        .lines()
        .find_map(|line| line.strip_prefix("Content-Length: "))
        .unwrap()
        .parse::<usize>()
        .unwrap();
    assert_eq!(bytes.len() - end - 4, length);
    serde_json::from_slice(&bytes[end + 4..]).unwrap()
}
#[test]
fn dashboard_state_immediately_includes_the_first_completed_call_without_another_session_action() {
    let r = root();
    let mut profile = host(Estimator::CharCount);
    profile.dashboard = Some(Default::default());
    let mut s = session(r.path(), profile, "dashboard");
    let info = s.invoke("openDashboard", &json!({}), &d()).unwrap();
    call(&mut s, "read_file", json!({"relative_path":"main.rs"})).unwrap();
    let view = dashboard_state(&info);
    assert_eq!(
        view["view"]["toolUsageStatistics"]["tools"]["read_file"]["numTimesCalled"],
        1
    );
    let stats = &view["view"]["toolUsageStatistics"];
    assert!(!stats.to_string().contains("visible"));
}

#[test]
fn real_authenticated_http_counts_once_and_reports_prior_tool_output() {
    use yoke::mcp_http::{self, Config, Principal, Target};
    let r = root();
    let target = Target::new(
        r.path(),
        yoke::protocol::Profile::Serena,
        host(Estimator::CharCount),
    );
    let principal = Principal::new(
        "owned-counts",
        zeroize::Zeroizing::new(vec![51; 32]),
        target,
    )
    .unwrap();
    let server = mcp_http::serve(Config::default(), vec![principal]).unwrap();
    let address = server.local_addr();
    let (status, id, _) = post(
        address,
        None,
        json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"owned-counts","version":"1"}}}),
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
    let (status, _, result) = post(
        address,
        Some(&id),
        json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"read_file","arguments":{"relative_path":"main.rs"}}}),
    );
    assert_eq!(status, 200);
    assert_eq!(result["result"]["isError"], false);
    let (_, _, result) = post(
        address,
        Some(&id),
        json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"get_current_config","arguments":{}}}),
    );
    let view: Value =
        serde_json::from_str(result["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(view["status"], "ok");
    assert_eq!(
        view["data"]["toolUsageStatistics"]["tools"]["read_file"]["numTimesCalled"],
        1
    );
    server.revoke("owned-counts").unwrap();
    assert_eq!(post(address,Some(&id),json!({"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"read_file","arguments":{"relative_path":"main.rs"}}})).0,401);
    server.shutdown().unwrap();
    assert_eq!(
        fs::read(r.path().join("main.rs")).unwrap(),
        SOURCE.as_bytes()
    );
}
#[test]
fn cli_remote_estimator_needs_separate_flags_before_session_publication() {
    let r = root();
    let profile = r.path().join("remote.json");
    let requests = r.path().join("query.json");
    fs::write(
        &profile,
        serde_json::to_vec(&json!({"tokenCountEstimator":"ANTHROPIC_CLAUDE_SONNET_4"})).unwrap(),
    )
    .unwrap();
    fs::write(
        &requests,
        serde_json::to_vec(&json!([{"operation":"toolUsage","args":{}}])).unwrap(),
    )
    .unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_yoke"))
        .arg("--root")
        .arg(r.path())
        .args(["session", "--id", "ungranted", "--host-profile"])
        .arg(&profile)
        .arg("--requests")
        .arg(&requests)
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("tool_usage_independent_text_disclosure_grant_required")
    );
    let missing_flag = Command::new(env!("CARGO_BIN_EXE_yoke"))
        .args([
            "--token-count-credential",
            "owned-placeholder",
            "session",
            "--host-profile",
        ])
        .arg(&profile)
        .arg("--requests")
        .arg(&requests)
        .output()
        .unwrap();
    assert!(!missing_flag.status.success());
    assert!(String::from_utf8_lossy(&missing_flag.stderr).contains("allow-tool-token-disclosure"));
    let engine = Engine::open(r.path()).unwrap();
    let rows: u64 = engine
        .store
        .db
        .query_row(
            "SELECT count(*) FROM records WHERE kind=?1",
            [yoke::project_session::RECORD_KIND],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(rows, 0);
}

// The bounded MCP request helper below is copied exactly from global_tool_budgets.rs.
fn count_service(
    statuses: Vec<u16>,
) -> (
    yoke::tool_usage::http::Host,
    std::sync::mpsc::Receiver<Value>,
    std::thread::JoinHandle<()>,
) {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let address = listener.local_addr().unwrap();
    let (sender, receiver) = std::sync::mpsc::channel();
    let worker = std::thread::spawn(move || {
        for status in statuses {
            let until = std::time::Instant::now() + Duration::from_secs(10);
            let (mut stream, _) = loop {
                match listener.accept() {
                    Ok(peer) => break peer,
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        assert!(std::time::Instant::now() < until);
                        std::thread::sleep(Duration::from_millis(1));
                    }
                    Err(error) => panic!("owned accept failed: {error}"),
                }
            };
            stream.set_nonblocking(false).unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            let mut bytes = Vec::new();
            let (end, length) = loop {
                let mut block = [0; 4096];
                let n = stream.read(&mut block).unwrap();
                assert_ne!(n, 0);
                bytes.extend_from_slice(&block[..n]);
                assert!(bytes.len() < 8_010_000);
                if let Some(end) = bytes.windows(4).position(|b| b == b"\r\n\r\n") {
                    let header = std::str::from_utf8(&bytes[..end])
                        .unwrap()
                        .to_ascii_lowercase();
                    assert!(header.starts_with("post /v1/messages/count_tokens http/1.1\r\n"));
                    let length = header
                        .lines()
                        .find_map(|line| line.strip_prefix("content-length:"))
                        .unwrap()
                        .trim()
                        .parse::<usize>()
                        .unwrap();
                    if bytes.len() >= end + 4 + length {
                        break (end, length);
                    }
                }
            };
            sender
                .send(serde_json::from_slice(&bytes[end + 4..end + 4 + length]).unwrap())
                .unwrap();
            let body = r#"{"input_tokens":7}"#;
            let _ = write!(
                stream,
                "HTTP/1.1 {status} Owned\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
        }
    });
    let runtime = yoke::tool_usage::http::Host::new(
        yoke::tool_usage::http::Spec {
            endpoint: format!("http://{address}/v1/messages/count_tokens"),
            credential_name: "owned".into(),
            allow_loopback_http: true,
            additional_root_certificates: vec![],
        },
        true,
        std::sync::Arc::new(|_| Ok(zeroize::Zeroizing::new("owned-placeholder".into()))),
    )
    .unwrap();
    (runtime, receiver, worker)
}
#[test]
fn remote_counting_marks_real_native_envelopes_nonlocal_without_replacing_native_estimates() {
    let r = root();
    let (runtime, requests, worker) = count_service(vec![200, 200]);
    let mut profile = host(Estimator::AnthropicClaudeSonnet4);
    profile.token_count_runtime = Some(runtime);
    let mut s = session(r.path(), profile, "native-metrics");
    let result = call(
        &mut s,
        "query_project",
        json!({"project_name":"default","tool_name":"ci_status","tool_params_json":"{}"}),
    )
    .unwrap();
    assert_eq!(result["schemaVersion"], 1);
    assert_eq!(result["metrics"]["localOnly"], false);
    assert_eq!(result["metrics"]["externalUsage"], "unknown");
    assert_eq!(result["metrics"]["tokenMethod"], "utf8_bytes_div_3");
    assert_eq!(result["metrics"]["modelCalls"], 0);
    let size = result["metrics"]["serializedBytes"].as_u64().unwrap();
    assert_eq!(result["metrics"]["estimatedTokens"], size.div_ceil(3));
    assert_eq!(result["metrics"]["toolTokenCounting"]["requests"], 2);
    assert_eq!(
        result["metrics"]["toolTokenCounting"]["nativeSizeMetricsScope"],
        "underlying_native_result_before_tool_token_counting_metadata"
    );
    let _input = requests.recv().unwrap();
    let output = requests.recv().unwrap();
    worker.join().unwrap();
    let counted: Value =
        serde_json::from_str(output["messages"][0]["content"].as_str().unwrap()).unwrap();
    assert_eq!(counted, result);
    assert_eq!(stats(&mut s)["tools"]["query_project"]["numTimesCalled"], 1);
}
#[test]
fn failed_remote_attempt_is_visible_on_dashboard_immediately_without_fake_tool_counts() {
    let r = root();
    let (runtime, requests, worker) = count_service(vec![429]);
    let mut profile = host(Estimator::AnthropicClaudeSonnet4);
    profile.token_count_runtime = Some(runtime);
    profile.dashboard = Some(Default::default());
    let mut s = session(r.path(), profile, "failed-counter");
    let info = s.invoke("openDashboard", &json!({}), &d()).unwrap();
    let error = call(&mut s, "read_file", json!({"relative_path":"main.rs"})).unwrap_err();
    assert!(error.to_string().contains("after_operation_no_rollback"));
    assert!(requests.recv().is_ok());
    worker.join().unwrap();
    let view = dashboard_state(&info);
    let stats = &view["view"]["toolUsageStatistics"];
    assert_eq!(stats["hostGrantRemoteAttempts"], 1);
    assert!(stats["tools"].as_object().unwrap().is_empty());
    assert_eq!(
        fs::read(r.path().join("main.rs")).unwrap(),
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
