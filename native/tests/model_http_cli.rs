use serde_json::json;
use std::{fs, process::Command};

fn profile() -> serde_json::Value {
    json!({
        "endpoint":"http://127.0.0.1:1/",
        "dialect":"openAiCompatible", "auth":"bearer",
        "jsonMode":"jsonSchema", "tokenLimit":"maxCompletionTokens",
        "modelId":"fixture-model", "modelVersion":"fixture-v1",
        "expectedRemoteModel":"fixture-model", "credentialName":"unused-model-fixture",
        "maxOutputTokens":256, "taskId":"model-cli-fixture", "currency":"USD",
        "maxCostMicros":10, "taskLimitMicros":100
    })
}

#[test]
fn missing_http_authority_fails_before_profile_or_source_state_is_opened() {
    let root = tempfile::tempdir().unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_yoke"))
        .arg("--root")
        .arg(root.path())
        .args([
            "models-http",
            "--config",
            "does-not-exist.json",
            "summarize",
        ])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("requires --trust-network"),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!root.path().join(".yoke").exists());
}

#[test]
fn profile_authority_fields_and_inline_secrets_are_rejected_before_source_state() {
    let root = tempfile::tempdir().unwrap();
    let inputs = tempfile::tempdir().unwrap();
    for (key, value) in [
        ("trustedNetwork", json!(true)),
        ("allowLoopbackHttp", json!(true)),
        ("apiKey", json!("sentinel-private-value")),
    ] {
        let mut config = profile();
        config[key] = value;
        let path = inputs.path().join("profile.json");
        fs::write(&path, config.to_string()).unwrap();
        let output = Command::new(env!("CARGO_BIN_EXE_yoke"))
            .arg("--root")
            .arg(root.path())
            .args(["models-http", "--config"])
            .arg(path)
            .args(["--trust-network", "summarize"])
            .output()
            .unwrap();
        assert!(!output.status.success());
        assert!(!String::from_utf8_lossy(&output.stderr).contains("sentinel-private-value"));
        assert!(!root.path().join(".yoke").exists());
    }
}

#[test]
fn worker_rejects_changed_host_profile_before_waiting_for_stdin_or_reading_credentials() {
    let root = tempfile::tempdir().unwrap();
    let inputs = tempfile::tempdir().unwrap();
    let path = inputs.path().join("profile.json");
    fs::write(&path, profile().to_string()).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_yoke"))
        .arg("--root")
        .arg(root.path())
        .args(["model-http-worker", "--config"])
        .arg(path)
        .args([
            "--config-hash",
            &"0".repeat(64),
            "--trust-network",
            "--allow-loopback-http",
        ])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("HTTP provider profile changed"),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!root.path().join(".yoke").exists());
}

#[cfg(windows)]
#[test]
#[ignore = "Actual Windows Credential Manager and owned loopback HTTP; creates and removes one unique fake credential"]
fn actual_cli_http_summary_preserves_source_binding_and_unknown_billing() {
    use std::{
        io::{Read, Write},
        net::TcpListener,
        process::Stdio,
        time::{Duration, Instant},
    };
    let root = tempfile::tempdir().unwrap();
    fs::write(
        root.path().join("invoice.rs"),
        "pub fn invoice() -> u32 { 17 }\n",
    )
    .unwrap();
    let (snapshot_id, entity_id) = {
        let mut engine = yoke::Engine::open(root.path()).unwrap();
        let (snapshot, graph, _, _) = engine.index(&yoke::capture::Deadline::new(5000)).unwrap();
        (
            snapshot.id,
            graph
                .entities
                .iter()
                .find(|e| e.name == "invoice")
                .unwrap()
                .id
                .clone(),
        )
    };
    let mut nonce = [0; 16];
    getrandom::fill(&mut nonce).unwrap();
    let name = format!("model-cli-test-{}", hex::encode(nonce));
    let secret = zeroize::Zeroizing::new(format!("synthetic-{}", hex::encode(nonce)));
    struct Created(String);
    impl Drop for Created {
        fn drop(&mut self) {
            #[link(name = "Advapi32")]
            unsafe extern "system" {
                fn CredDeleteW(target: *const u16, kind: u32, flags: u32) -> i32;
            }
            assert!(
                self.0
                    .starts_with("yoke-code-intelligence/service/model/model-cli-test-")
            );
            let wide: Vec<_> = self.0.encode_utf16().chain(Some(0)).collect();
            unsafe {
                CredDeleteW(wide.as_ptr(), 1, 0);
            }
        }
    }
    let mut import = Command::new(env!("CARGO_BIN_EXE_yoke"))
        .args(["credentials", "model-import", &name])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    import
        .stdin
        .take()
        .unwrap()
        .write_all(secret.as_bytes())
        .unwrap();
    let output = import.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "Model credential fixture import failed"
    );
    let _created = Created(format!("yoke-code-intelligence/service/model/{name}"));
    assert!(!String::from_utf8_lossy(&output.stdout).contains(secret.as_str()));
    assert!(!String::from_utf8_lossy(&output.stderr).contains(secret.as_str()));
    assert!(yoke::host_credentials::import_model_token(&name, b"replacement-fixture").is_err());
    assert_eq!(
        yoke::host_credentials::read_model_token(&name)
            .unwrap()
            .as_str(),
        secret.as_str()
    );
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let address = listener.local_addr().unwrap();
    let expected_entity = entity_id.clone();
    let expected_secret = secret.clone();
    let worker = std::thread::spawn(move || {
        let until = Instant::now() + Duration::from_secs(8);
        let mut stream = loop {
            match listener.accept() {
                Ok((stream, _)) => break stream,
                Err(error)
                    if error.kind() == std::io::ErrorKind::WouldBlock && Instant::now() < until =>
                {
                    std::thread::sleep(Duration::from_millis(5))
                }
                _ => panic!("Owned HTTP fixture received no request"),
            }
        };
        stream.set_nonblocking(false).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        stream
            .set_write_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        let mut bytes = Vec::new();
        let (head, body) = loop {
            let mut chunk = [0; 4096];
            let n = stream.read(&mut chunk).unwrap();
            assert!(n > 0 && bytes.len() + n <= 1_000_000);
            bytes.extend_from_slice(&chunk[..n]);
            if let Some(end) = bytes
                .windows(4)
                .position(|b| b == b"\r\n\r\n")
                .map(|n| n + 4)
            {
                let head = String::from_utf8(bytes[..end].to_vec()).unwrap();
                let length: usize = head
                    .lines()
                    .find_map(|line| {
                        line.split_once(':')
                            .filter(|(key, _)| key.eq_ignore_ascii_case("content-length"))
                            .map(|(_, value)| value.trim().parse().unwrap())
                    })
                    .unwrap();
                if bytes.len() == end + length {
                    break (
                        head,
                        serde_json::from_slice::<serde_json::Value>(&bytes[end..]).unwrap(),
                    );
                }
            }
        };
        assert!(head.starts_with("POST /v1/chat/completions HTTP/1.1\r\n"));
        assert!(head.to_ascii_lowercase().contains(&format!(
            "authorization: bearer {}",
            expected_secret.as_str()
        )));
        let data: serde_json::Value =
            serde_json::from_str(body["messages"][1]["content"].as_str().unwrap()).unwrap();
        assert_eq!(data["sources"][0]["anchor"]["sourceId"], expected_entity);
        assert!(
            data["sources"][0]["text"]
                .as_str()
                .unwrap()
                .contains("pub fn invoice()")
        );
        let content = json!({"statements":[{"text":"The source defines invoice.","evidenceEntityIds":[expected_entity]}]}).to_string();
        let response = json!({"model":"fixture-model","choices":[{"index":0,"finish_reason":"stop","message":{"role":"assistant","content":content}}],"usage":{"prompt_tokens":10,"completion_tokens":3,"prompt_tokens_details":{"cached_tokens":2}}}).to_string();
        write!(stream,"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",response.len(),response).unwrap();
    });
    let config_root = tempfile::tempdir().unwrap();
    let config_path = config_root.path().join("profile.json");
    let mut config = profile();
    config["endpoint"] = json!(format!("http://{address}/v1/"));
    config["credentialName"] = json!(name);
    fs::write(&config_path, config.to_string()).unwrap();
    let result = Command::new(env!("CARGO_BIN_EXE_yoke"))
        .arg("--root")
        .arg(root.path())
        .args([
            "--snapshot",
            &snapshot_id,
            "--timeout",
            "5000",
            "--max-bytes",
            "500000",
            "models-http",
            "--config",
        ])
        .arg(&config_path)
        .args([
            "--trust-network",
            "--allow-loopback-http",
            "summarize",
            "--entity",
            &entity_id,
        ])
        .output()
        .unwrap();
    worker.join().unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert!(!String::from_utf8_lossy(&result.stdout).contains(secret.as_str()));
    assert!(!String::from_utf8_lossy(&result.stderr).contains(secret.as_str()));
    let value: serde_json::Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(value["statements"][0]["evidenceEntityIds"][0], entity_id);
    assert_eq!(value["truth"], "model_inference_not_verified_fact");
    assert!(value["providerReportedCostMicros"].is_null());
    assert_eq!(value["providerReportedUsage"]["cachedInputTokens"], 2);
    let engine = yoke::Engine::open(root.path()).unwrap();
    let totals = yoke::ledger::Ledger::new(&engine.store, &yoke::capture::Deadline::new(5000))
        .total("model-cli-fixture")
        .unwrap();
    assert_eq!(totals.attempts, 1);
    assert_eq!(totals.unknown, 1);
    assert_eq!(totals.encumbered_micros, 10);
}
