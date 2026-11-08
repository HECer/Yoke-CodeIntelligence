//! Actual Windows OS credential chunks and owned TCP; no AWS service calls.
#![cfg(windows)]

use serde_json::json;
use std::{fs, process::Command};
fn profile() -> serde_json::Value {
    json!({"endpoint":"http://127.0.0.1:1/","region":"us-east-1","remoteModelId":"fixture-model","modelId":"fixture-model","modelVersion":"fixture-v1","credentialName":"unused-fixture","dialect":{"kind":"converse","output":"jsonSchema","maxOutputTokens":256,"vision":false},"taskId":"model-cli-fixture","currency":"USD","maxCostMicros":10,"taskLimitMicros":100})
}
#[cfg(windows)]
#[test]
#[ignore = "Actual Windows Credential Manager and owned loopback HTTP; creates and removes one unique fake AWS credential generation"]
fn actual_cli_bedrock_summary_uses_chunked_session_and_one_accounted_attempt() {
    actual_cli(false);
}
#[cfg(windows)]
#[test]
#[ignore = "Actual Windows credentials and three owned TCP Bedrock attempts; cleans only fake generation"]
fn actual_cli_bedrock_batch_has_three_accounted_attempts_and_one_complete_corpus() {
    actual_cli(true);
}
#[cfg(windows)]
fn actual_cli(batch: bool) {
    use std::{
        io::{Read, Write},
        net::TcpListener,
        process::Stdio,
        time::{Duration, Instant},
    };
    let root = tempfile::tempdir().unwrap();
    fs::write(
        root.path().join("invoice.rs"),
        if batch { "pub fn invoice() -> u32 { 17 }\npub fn second() -> u32 { 18 }\npub fn third() -> u32 { 19 }\n" } else { "pub fn invoice() -> u32 { 17 }\n" },
    )
    .unwrap();
    let (snapshot_id, entity_id, expected_ids) = {
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
            graph
                .entities
                .iter()
                .filter(|e| e.kind != "file")
                .map(|e| e.id.clone())
                .collect::<std::collections::BTreeSet<_>>(),
        )
    };
    let mut nonce = [0; 16];
    getrandom::fill(&mut nonce).unwrap();
    let name = format!("aws-cli-test-{}", hex::encode(nonce));
    let secret = zeroize::Zeroizing::new(format!("synthetic-{}", hex::encode(nonce)));
    struct Created(Vec<String>);
    impl Drop for Created {
        fn drop(&mut self) {
            #[link(name = "Advapi32")]
            unsafe extern "system" {
                fn CredDeleteW(target: *const u16, kind: u32, flags: u32) -> i32;
            }
            for target in &self.0 {
                assert!(
                    target.starts_with("yoke-code-intelligence/service/aws-session/aws-cli-test-")
                );
                let wide: Vec<_> = target.encode_utf16().chain(Some(0)).collect();
                let result = unsafe { CredDeleteW(wide.as_ptr(), 1, 0) };
                if result == 0 && !std::thread::panicking() {
                    panic!("Owned fake AWS credential cleanup failed");
                }
            }
        }
    }
    fn created_targets(base: &str) -> Vec<String> {
        use std::{ffi::c_void, ptr};
        #[repr(C)]
        struct FileTime {
            low: u32,
            high: u32,
        }
        #[repr(C)]
        struct Credential {
            flags: u32,
            kind: u32,
            target: *mut u16,
            comment: *mut u16,
            last_written: FileTime,
            blob_size: u32,
            blob: *mut u8,
            persist: u32,
            attribute_count: u32,
            attributes: *mut c_void,
            target_alias: *mut u16,
            username: *mut u16,
        }
        #[link(name = "Advapi32")]
        unsafe extern "system" {
            fn CredReadW(
                target: *const u16,
                kind: u32,
                flags: u32,
                value: *mut *mut Credential,
            ) -> i32;
            fn CredFree(buffer: *mut c_void);
        }
        let wide: Vec<_> = base.encode_utf16().chain(Some(0)).collect();
        let mut value = ptr::null_mut();
        assert_ne!(
            unsafe { CredReadW(wide.as_ptr(), 1, 0, &mut value) },
            0,
            "Owned fake AWS manifest read failed"
        );
        assert!(!value.is_null());
        let manifest = unsafe { &*value };
        assert_eq!(manifest.blob_size, 67);
        let bytes = unsafe { std::slice::from_raw_parts(manifest.blob, 67) }.to_vec();
        unsafe { CredFree(value.cast()) };
        assert_eq!(&bytes[..5], b"YAWS1");
        let count = u16::from_be_bytes(bytes[21..23].try_into().unwrap());
        assert!((2..=28).contains(&count));
        let generation = hex::encode(&bytes[5..21]);
        let mut targets = (0..count)
            .map(|index| format!("{base}/chunk/{generation}/{index:02}"))
            .collect::<Vec<_>>();
        targets.push(base.into());
        targets
    }
    let session_token =
        zeroize::Zeroizing::new(format!("synthetic-{}", hex::encode(nonce)).repeat(100));
    assert!(session_token.len() > 4096);
    let expiration = u64::try_from(
        (std::time::SystemTime::now() + Duration::from_secs(3600))
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis(),
    )
    .unwrap();
    let private_input=zeroize::Zeroizing::new(json!({"accessKeyId":"AKIDEXAMPLE","secretAccessKey":secret.as_str(),"sessionToken":session_token.as_str(),"expiresAtUnixMs":expiration}).to_string());
    let mut import = Command::new(env!("CARGO_BIN_EXE_yoke"))
        .args(["credentials", "aws-import", &name])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    import
        .stdin
        .take()
        .unwrap()
        .write_all(private_input.as_bytes())
        .unwrap();
    let output = import.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "Model credential fixture import failed"
    );
    let _created = Created(created_targets(&format!(
        "yoke-code-intelligence/service/aws-session/{name}"
    )));
    assert!(!String::from_utf8_lossy(&output.stdout).contains(secret.as_str()));
    assert!(!String::from_utf8_lossy(&output.stderr).contains(secret.as_str()));
    assert!(
        yoke::host_credentials::import_aws_session_json(
            &name,
            private_input.as_bytes(),
            &yoke::capture::Deadline::new(5000)
        )
        .is_err()
    );
    assert!(
        yoke::host_credentials::read_aws_session(&name, &yoke::capture::Deadline::new(5000))
            .is_ok()
    );
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let address = listener.local_addr().unwrap();
    let expected_entity = entity_id.clone();
    let expected_secret = session_token.clone();
    let worker = std::thread::spawn(move || {
        let mut observed = Vec::new();
        for _ in 0..if batch { 3 } else { 1 } {
            let until = Instant::now() + Duration::from_secs(8);
            let mut stream = loop {
                match listener.accept() {
                    Ok((stream, _)) => break stream,
                    Err(error)
                        if error.kind() == std::io::ErrorKind::WouldBlock
                            && Instant::now() < until =>
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
            assert!(head.starts_with(if batch {
                "POST /v1/model/fixture-model/invoke HTTP/1.1\r\n"
            } else {
                "POST /v1/model/fixture-model/converse HTTP/1.1\r\n"
            }));
            assert!(head.to_ascii_lowercase().contains(&format!(
                "x-amz-security-token: {}",
                expected_secret.as_str()
            )));
            assert!(
                head.to_ascii_lowercase()
                    .contains("authorization: aws4-hmac-sha256 credential=akidexample/")
            );
            assert!(head.contains("/us-east-1/bedrock/aws4_request"));
            let response = if batch {
                let text = body["inputText"].as_str().unwrap();
                assert!(
                    [
                        "pub fn invoice() -> u32 { 17 }",
                        "pub fn second() -> u32 { 18 }",
                        "pub fn third() -> u32 { 19 }"
                    ]
                    .contains(&text)
                );
                observed.push(text.to_owned());
                assert_eq!(body["dimensions"], 256);
                json!({"embedding":std::iter::once(1.0).chain(std::iter::repeat_n(0.0,255)).collect::<Vec<_>>(),"inputTextTokenCount":10}).to_string()
            } else {
                let data: serde_json::Value = serde_json::from_str(
                    body["messages"][0]["content"][0]["text"].as_str().unwrap(),
                )
                .unwrap();
                assert_eq!(data["sources"][0]["anchor"]["sourceId"], expected_entity);
                assert!(
                    data["sources"][0]["text"]
                        .as_str()
                        .unwrap()
                        .contains("pub fn invoice()")
                );
                let content=json!({"statements":[{"text":"The source defines invoice.","evidenceEntityIds":[expected_entity]}]}).to_string();
                json!({"output":{"message":{"role":"assistant","content":[{"text":content}]}},"stopReason":"end_turn","usage":{"inputTokens":10,"outputTokens":3,"cacheReadInputTokens":2,"cacheWriteInputTokens":1}}).to_string()
            };
            write!(stream,"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",response.len(),response).unwrap();
        }
        observed
    });
    let config_root = tempfile::tempdir().unwrap();
    let config_path = config_root.path().join("profile.json");
    let mut config = profile();
    config["endpoint"] = json!(format!("http://{address}/v1/"));
    config["credentialName"] = json!(name);
    if batch {
        config["dialect"] = json!({"kind":"titanV2","dimensions":256,"normalize":true});
        config["embeddingBatch"] =
            json!({"maxCalls":3,"maxEntities":3,"maxTotalSourceBytes":15000});
    }
    fs::write(&config_path, config.to_string()).unwrap();
    let mut command = Command::new(env!("CARGO_BIN_EXE_yoke"));
    command
        .arg("--root")
        .arg(root.path())
        .args([
            "--snapshot",
            &snapshot_id,
            "--timeout",
            "5000",
            "--max-bytes",
            "500000",
            "models-bedrock",
            "--config",
        ])
        .arg(&config_path)
        .args(["--trust-network", "--allow-loopback-http"]);
    if batch {
        command.arg("embeddings");
    } else {
        command.args(["summarize", "--entity", &entity_id]);
    }
    let result = command.output().unwrap();
    let observed = worker.join().unwrap();
    if batch {
        assert_eq!(observed.len(), 3);
        assert_eq!(
            observed
                .into_iter()
                .collect::<std::collections::BTreeSet<_>>()
                .len(),
            3
        );
    }
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert!(!String::from_utf8_lossy(&result.stdout).contains(secret.as_str()));
    assert!(!String::from_utf8_lossy(&result.stderr).contains(secret.as_str()));
    let value: serde_json::Value = serde_json::from_slice(&result.stdout).unwrap();
    if batch {
        assert_eq!(value["status"], "complete");
        assert_eq!(value["count"], 3);
        assert_eq!(value["dimension"], 256);
        assert_eq!(value["modelCalls"], 3);
        assert_eq!(value["attemptIds"].as_array().unwrap().len(), 3);
        assert_eq!(value["providerReportedUsage"]["inputTokens"], 30);
    } else {
        assert_eq!(value["statements"][0]["evidenceEntityIds"][0], entity_id);
        assert_eq!(value["truth"], "model_inference_not_verified_fact");
        assert_eq!(value["providerReportedUsage"]["cachedInputTokens"], 2);
        assert_eq!(value["providerReportedUsage"]["inputTokens"], 13);
    }
    assert!(value["providerReportedCostMicros"].is_null());
    let engine = yoke::Engine::open(root.path()).unwrap();
    let totals = yoke::ledger::Ledger::new(&engine.store, &yoke::capture::Deadline::new(5000))
        .total("model-cli-fixture")
        .unwrap();
    assert_eq!(totals.attempts, if batch { 3 } else { 1 });
    assert_eq!(totals.unknown, if batch { 3 } else { 1 });
    assert_eq!(totals.encumbered_micros, if batch { 30 } else { 10 });
    if batch {
        let collections: Vec<serde_json::Value> =
            engine.store.records("native_vectors_v1").unwrap();
        assert_eq!(collections.len(), 1);
        assert_eq!(
            collections[0]["items"]
                .as_array()
                .unwrap()
                .iter()
                .map(|i| i["entityId"].as_str().unwrap().to_owned())
                .collect::<std::collections::BTreeSet<_>>(),
            expected_ids
        );
        let rows: Vec<serde_json::Value> = engine.store.records("native_model_batch_v1").unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0]["state"], "complete");
    }
}
