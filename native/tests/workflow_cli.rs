use serde_json::{Value, json};
use std::{fs, process::Command};
use yoke::Engine;

fn runtime_response_boundary_fixture(
    source: &str,
    requests: Value,
    allow_evaluation: bool,
) -> (yoke::process::CapturedOutput, Vec<u8>) {
    let workspace = tempfile::tempdir().unwrap();
    let inputs = tempfile::tempdir().unwrap();
    let source_path = workspace.path().join("main.py");
    fs::write(&source_path, source).unwrap();
    let request_path = inputs.path().join("requests.json");
    fs::write(&request_path, serde_json::to_vec(&requests).unwrap()).unwrap();
    let executable = if cfg!(windows) {
        "python.exe"
    } else {
        "python3"
    };
    let python = std::env::split_paths(&std::env::var_os("PATH").unwrap())
        .filter(|path| path.is_absolute())
        .map(|path| path.join(executable))
        .find(|path| path.is_file())
        .expect("Actual host Python is required for runtime CLI acceptance")
        .canonicalize()
        .unwrap();
    let mut command = Command::new(env!("CARGO_BIN_EXE_yoke"));
    command
        .current_dir(workspace.path())
        .arg("--root")
        .arg(workspace.path())
        .args(["--timeout", "10000", "runtime", "main.py", "--python"])
        .arg(python)
        .arg("--requests")
        .arg(request_path)
        .arg("--trust-runtime");
    if allow_evaluation {
        command.arg("--allow-evaluation");
    }
    let output = yoke::process::run_capture(
        command,
        b"",
        &yoke::capture::Deadline::new(20_000),
        yoke::process::CaptureLimits {
            stdin_bytes: 0,
            stdout_bytes: 1_000_000,
            stderr_bytes: 128_000,
        },
    )
    .unwrap();
    let original = fs::read(&source_path).unwrap();
    // run_capture has completed checked root/group cleanup. Both temporary
    // namespaces are dropped before the caller makes acceptance assertions.
    (output, original)
}

#[test]
fn runtime_cli_empty_and_paused_batches_keep_only_original_observations() {
    let source = "raise RuntimeError('owned paused payload must remain inert')\n";
    for requests in [json!([]), json!([{"command": "stack"}])] {
        let expected = requests.as_array().unwrap().len() + 1;
        let (output, original) = runtime_response_boundary_fixture(source, requests, false);
        assert_eq!(
            output.exit_code,
            Some(0),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(output.stdin_complete && output.output_complete);
        assert!(!output.timed_out && !output.output_exceeded && !output.io_error);
        let report: Value = serde_json::from_slice(&output.stdout).unwrap();
        let events = report["events"].as_array().unwrap();
        assert_eq!(events.len(), expected);
        assert!(events.iter().all(|event| event["status"] == "paused"));
        assert_eq!(original, source.as_bytes());
    }
}

#[test]
fn runtime_cli_paused_shutdown_refusal_has_no_successful_batch_output() {
    let source = "value = 1\n";
    // Explicitly authorized evaluation changes only the owned worker's in-memory
    // protocol function. The user REPL response remains paused; only a later
    // Close refuses. Without checked host shutdown, the old CLI prints success.
    let install = "import sys\n_owned_worker = sys.modules['__main__']\n_owned_emit = _owned_worker.emit\ndef _owned_refuse_closed(event, data):\n if event == 'closed':\n  raise _owned_worker.WorkerFailure('owned close refusal')\n return _owned_emit(event, data)\n_owned_worker.emit = _owned_refuse_closed\n";
    let requests = json!([{"command": "repl", "code": install, "frame": 0}]);
    let (output, original) = runtime_response_boundary_fixture(source, requests, true);
    assert_eq!(output.exit_code, Some(1));
    assert!(output.stdin_complete && output.output_complete);
    assert!(!output.timed_out && !output.output_exceeded && !output.io_error);
    assert!(output.stdout.is_empty());
    assert!(String::from_utf8_lossy(&output.stderr).contains("shutdown was not confirmed"));
    assert_eq!(original, source.as_bytes());
}

#[test]
fn runtime_cli_terminal_and_explicit_close_preserve_requested_events() {
    let source = "print('owned terminal response')\n";
    for (requests, status, completed) in [
        (json!([{"command": "continue"}]), "finished", true),
        (json!([{"command": "close"}]), "closed", false),
    ] {
        let (output, original) = runtime_response_boundary_fixture(source, requests, false);
        assert_eq!(
            output.exit_code,
            Some(0),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(output.stdin_complete && output.output_complete);
        assert!(!output.timed_out && !output.output_exceeded && !output.io_error);
        let report: Value = serde_json::from_slice(&output.stdout).unwrap();
        let events = report["events"].as_array().unwrap();
        assert_eq!(events.len(), 2);
        assert_eq!(events[0]["status"], "paused");
        assert_eq!(events[1]["status"], status);
        assert_eq!(events[1]["programCompleted"], completed);
        assert_eq!(original, source.as_bytes());
    }
}

#[test]
fn malformed_host_profile_values_are_not_repeated_in_cli_errors() {
    let workspace = tempfile::tempdir().unwrap();
    let inputs = tempfile::tempdir().unwrap();
    let sentinel = "synthetic-private-profile-value-never-real-credential";
    let config = inputs.path().join("provider.json");
    fs::write(&config,serde_json::to_vec(&json!({"command":"unavailable-test-provider","modelId":"test","modelVersion":"1","taskId":"test","currency":"EUR","maxCostMicros":sentinel,"taskLimitMicros":100})).unwrap()).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_yoke"))
        .arg("--root")
        .arg(workspace.path())
        .arg("models")
        .arg("--config")
        .arg(config)
        .args(["--trust-provider", "embeddings"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(
        !String::from_utf8_lossy(&output.stderr).contains(sentinel),
        "Invalid host field values must not be disclosed by schema errors"
    );
}
#[test]
fn runtime_cli_preserves_failed_program_evidence_and_returns_failure() {
    let dir = tempfile::tempdir().unwrap();
    let inputs = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join("main.py"),
        "raise ValueError('observed failure')\n",
    )
    .unwrap();
    let requests = inputs.path().join("requests.json");
    fs::write(&requests, r#"[{"command":"continue"}]"#).unwrap();
    let binary = if cfg!(windows) {
        "python.exe"
    } else {
        "python3"
    };
    let python = std::env::split_paths(&std::env::var_os("PATH").unwrap())
        .map(|p| p.join(binary))
        .find(|p| p.is_file())
        .unwrap()
        .canonicalize()
        .unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_yoke"))
        .arg("--root")
        .arg(dir.path())
        .args(["runtime", "main.py", "--python"])
        .arg(python)
        .arg("--requests")
        .arg(requests)
        .arg("--trust-runtime")
        .output()
        .unwrap();
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["events"][1]["programCompleted"], false);
    assert!(
        !output.status.success(),
        "An observed failed program must return a failing CLI exit status"
    );
}
#[test]
fn ambiguous_json_rpc_ids_and_nested_lsp_results_are_refused() {
    let dir = tempfile::tempdir().unwrap();
    let mut engine = Engine::open(dir.path()).unwrap();
    let input =
        b"{\"jsonrpc\":\"2.0\",\"id\":1,\"id\":2,\"method\":\"initialize\",\"params\":{}}\n";
    let mut output = Vec::new();
    yoke::protocol::serve(&mut engine, &input[..], &mut output).unwrap();
    let message: Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(
        message["error"]["code"], -32700,
        "Ambiguous MCP identity must not receive a successful response"
    );
    let payload = r#"{"jsonrpc":"2.0","id":1,"result":{"range":{},"range":{}}}"#;
    let bytes = format!("Content-Length: {}\r\n\r\n{payload}", payload.len());
    assert!(
        yoke::lsp::read_frame(&mut std::io::Cursor::new(bytes)).is_err(),
        "Nested ambiguous LSP fields must fail"
    );
}
#[cfg(windows)]
#[test]
#[ignore = "Actual native CLI TLS service/client and Windows credential store; creates/removes only unique test credentials"]
fn actual_native_cli_tls_service_and_client_load_os_credentials() {
    use std::io::{BufRead, BufReader};
    struct Entries(Vec<String>);
    impl Drop for Entries {
        fn drop(&mut self) {
            #[link(name = "Advapi32")]
            unsafe extern "system" {
                fn CredDeleteW(target: *const u16, kind: u32, flags: u32) -> i32;
            }
            for target in &self.0 {
                assert!(target.starts_with("yoke-code-intelligence/service/"));
                let wide: Vec<_> = target.encode_utf16().chain(Some(0)).collect();
                unsafe {
                    CredDeleteW(wide.as_ptr(), 1, 0);
                }
            }
        }
    }
    struct Service(std::process::Child);
    impl Drop for Service {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
    let workspace = tempfile::tempdir().unwrap();
    let inputs = tempfile::tempdir().unwrap();
    let mut nonce = [0_u8; 16];
    getrandom::fill(&mut nonce).unwrap();
    let name = format!("test-cli-{}", hex::encode(nonce));
    let mut entries = Entries(vec![]);
    let generated = Command::new(env!("CARGO_BIN_EXE_yoke"))
        .args(["credentials", "tls", &name, "--hostname", "localhost"])
        .output()
        .unwrap();
    assert!(generated.status.success());
    entries
        .0
        .push(format!("yoke-code-intelligence/service/tls/{name}"));
    let identity: Value = serde_json::from_slice(&generated.stdout).unwrap();
    assert!(identity.get("privateKey").is_none());
    let generated = Command::new(env!("CARGO_BIN_EXE_yoke"))
        .args(["credentials", "token", &name])
        .output()
        .unwrap();
    assert!(generated.status.success());
    entries
        .0
        .push(format!("yoke-code-intelligence/service/token/{name}"));
    let token: Value = serde_json::from_slice(&generated.stdout).unwrap();
    assert!(token.get("token").is_none());
    let profile = inputs.path().join("service.json");
    fs::write(&profile,serde_json::to_vec(&json!({"bind":"127.0.0.1:0","tlsCredentialName":name,"serverNames":["localhost"],"requestTimeoutMs":2000,"maxBodyBytes":64000,"maxResponseBytes":64000,"imports":[],"exports":[],"principals":[{"id":"reader","tokenCredentialName":name,"roles":["read"],"importIds":[],"exportIds":[]}]})).unwrap()).unwrap();
    let mut service = Service(
        Command::new(env!("CARGO_BIN_EXE_yoke"))
            .arg("--root")
            .arg(workspace.path())
            .arg("federation-service")
            .arg("--profile")
            .arg(&profile)
            .args(["--trust-service", "--duration-ms", "5000"])
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .spawn()
            .unwrap(),
    );
    let stdout = service.0.stdout.take().unwrap();
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let mut line = String::new();
        let read = BufReader::new(stdout).read_line(&mut line);
        let _ = tx.send((read, line));
    });
    let (read, line) = rx
        .recv_timeout(std::time::Duration::from_secs(8))
        .expect("Bounded service startup");
    read.unwrap();
    let listening: Value = serde_json::from_str(&line).unwrap();
    assert_eq!(listening["status"], "listening");
    let client = inputs.path().join("client.json");
    fs::write(&client,serde_json::to_vec(&json!({"address":listening["address"],"serverName":"localhost","caCertificates":[identity["certificateDerBase64"]],"tokenCredentialName":name,"maxBodyBytes":64000,"maxResponseBytes":64000})).unwrap()).unwrap();
    let request = inputs.path().join("request.json");
    fs::write(&request, r#"{"method":"GET","path":"/v1/status"}"#).unwrap();
    let response = Command::new(env!("CARGO_BIN_EXE_yoke"))
        .arg("--root")
        .arg(workspace.path())
        .arg("federation-request")
        .arg("--profile")
        .arg(&client)
        .arg("--input")
        .arg(&request)
        .arg("--trust-transport")
        .output()
        .unwrap();
    assert!(
        response.status.success(),
        "{}",
        String::from_utf8_lossy(&response.stderr)
    );
    let report: Value = serde_json::from_slice(&response.stdout).unwrap();
    assert_eq!(report["statusCode"], 200);
    assert_eq!(report["body"]["principalId"], "reader");
    assert!(service.0.wait().unwrap().success());
}
#[test]
fn host_profiles_reject_duplicate_keys_instead_of_replacing_policy() {
    let dir = tempfile::tempdir().unwrap();
    let input = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("one.ts"), "export const value=1;\n").unwrap();
    let policy = input.path().join("policy.json");
    fs::write(
        &policy,
        r#"{"sources":[],"sinks":{},"sanitizers":[],"maxIterations":2,"maxIterations":3}"#,
    )
    .unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_yoke"))
        .arg("--root")
        .arg(dir.path())
        .arg("dataflow")
        .arg("--policy")
        .arg(&policy)
        .output()
        .unwrap();
    assert!(
        !output.status.success(),
        "Duplicate host policy fields were silently replaced"
    );
    assert!(String::from_utf8_lossy(&output.stderr).contains("Duplicate JSON key"));
}
#[test]
fn native_runtime_cli_preserves_state_across_authorized_repl_commands() {
    let dir = tempfile::tempdir().unwrap();
    let inputs = tempfile::tempdir().unwrap();
    let source = "counter = 1\ncounter += 1\nprint(counter)\n";
    fs::write(dir.path().join("main.py"), source).unwrap();
    let requests = inputs.path().join("requests.json");
    fs::write(&requests,serde_json::to_vec(&json!([{"command":"next"},{"command":"repl","code":"counter = 41","frame":0},{"command":"evaluate","expression":"counter + 1","frame":0},{"command":"continue"}])).unwrap()).unwrap();
    let binary = if cfg!(windows) {
        "python.exe"
    } else {
        "python3"
    };
    let python = std::env::split_paths(&std::env::var_os("PATH").unwrap())
        .map(|p| p.join(binary))
        .find(|p| p.is_file())
        .unwrap()
        .canonicalize()
        .unwrap();
    let run = |evaluation: bool| {
        let mut command = Command::new(env!("CARGO_BIN_EXE_yoke"));
        command
            .arg("--root")
            .arg(dir.path())
            .args(["runtime", "main.py", "--python"])
            .arg(&python)
            .arg("--requests")
            .arg(&requests)
            .arg("--trust-runtime");
        if evaluation {
            command.arg("--allow-evaluation");
        }
        command.output().unwrap()
    };
    assert!(
        !run(false).status.success(),
        "Source requests cannot enable evaluation"
    );
    let output = run(true);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["events"][3]["evaluation"]["display"], "42");
    assert_eq!(report["events"][4]["programCompleted"], true);
    assert_eq!(report["hostWroteOriginalSource"], false);
    assert_eq!(report["programHostEffects"], "not_monitored");
    assert_eq!(
        fs::read_to_string(dir.path().join("main.py")).unwrap(),
        source
    );
}
#[test]
fn model_worker_deadline_includes_an_input_pipe_that_never_closes() {
    let mut child = Command::new(env!("CARGO_BIN_EXE_yoke"))
        .args([
            "--timeout",
            "100",
            "model-worker",
            "--endpoint",
            "http://127.0.0.1:9",
            "--model",
            "fixture",
            "--digest",
            &"a".repeat(64),
        ])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .unwrap();
    let start = std::time::Instant::now();
    let mut exited = false;
    while start.elapsed() < std::time::Duration::from_secs(2) {
        if child.try_wait().unwrap().is_some() {
            exited = true;
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    if !exited {
        child.kill().unwrap();
    }
    child.wait().unwrap();
    assert!(
        exited,
        "Worker input transfer must share the host-selected deadline"
    );
}
#[test]
fn readonly_mcp_dataflow_and_native_cli_share_exact_snapshot_evidence() {
    let dir = tempfile::tempdir().unwrap();
    let source = "function run(req:any) { const value=req.query.q; eval(value); }\n";
    fs::write(dir.path().join("one.ts"), source).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_yoke"))
        .args(["--root", dir.path().to_str().unwrap(), "dataflow"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let cli: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(cli["findings"].as_array().unwrap().len(), 1);
    let mut engine = Engine::open(dir.path()).unwrap();
    let mcp = engine
        .invoke(
            "ci_status",
            json!({"snapshotId":cli["snapshotId"],"mode":"dataflow"}),
        )
        .unwrap();
    assert_eq!(mcp["status"], "ok", "{mcp}");
    assert_eq!(mcp["data"]["findings"], cli["findings"]);
    assert_eq!(mcp["metrics"]["modelCalls"], 0);
    assert_eq!(
        fs::read_to_string(dir.path().join("one.ts")).unwrap(),
        source
    );
}
#[test]
fn host_only_execution_and_retention_never_inherit_authority_from_config_json() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("one.ts"), "export const one=1;").unwrap();
    for args in [
        vec!["compiler", "--node", "missing", "--typescript", "missing"],
        vec!["models", "--config", "missing", "embeddings"],
        vec![
            "verify-container",
            "--plan",
            &"a".repeat(64),
            "--profile",
            "missing",
        ],
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_yoke"))
            .arg("--root")
            .arg(dir.path())
            .args(args)
            .output()
            .unwrap();
        assert!(!output.status.success());
        assert!(String::from_utf8_lossy(&output.stderr).contains("requires"));
    }
    let ledger = Command::new(env!("CARGO_BIN_EXE_yoke"))
        .arg("--root")
        .arg(dir.path())
        .args(["ledger", "no-attempts"])
        .output()
        .unwrap();
    assert!(ledger.status.success());
    let value: Value = serde_json::from_slice(&ledger.stdout).unwrap();
    assert_eq!(value["attempts"], 0);
}
