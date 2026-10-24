//! Independent owned TCP boundary review. Every credential is synthetic.
use serde_json::{Value, json};
use std::{
    io::{Read, Write},
    net::TcpListener,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    thread,
    time::{Duration, SystemTime},
};
use yoke::{
    capture::Deadline,
    model::{hash, identity},
    model_bedrock::{self, AwsSession, BedrockSpec, Dialect, Output},
};
use zeroize::Zeroizing;

const TOKEN: &str = "owned-review-session-token";
struct Server {
    endpoint: String,
    captured: Arc<Mutex<Vec<Vec<u8>>>>,
    stop: Arc<AtomicBool>,
    worker: Option<thread::JoinHandle<()>>,
}
impl Server {
    fn new(response: Value) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let endpoint = format!("http://{}/base/", listener.local_addr().unwrap());
        let captured = Arc::new(Mutex::new(Vec::new()));
        let output = captured.clone();
        let stop = Arc::new(AtomicBool::new(false));
        let flag = stop.clone();
        let worker = thread::spawn(move || {
            while !flag.load(Ordering::SeqCst) {
                let mut stream = match listener.accept() {
                    Ok((stream, _)) => stream,
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(2));
                        continue;
                    }
                    Err(error) => panic!("{error}"),
                };
                stream.set_nonblocking(false).unwrap();
                stream
                    .set_read_timeout(Some(Duration::from_secs(2)))
                    .unwrap();
                stream
                    .set_write_timeout(Some(Duration::from_secs(2)))
                    .unwrap();
                let mut bytes = Vec::new();
                let mut chunk = [0; 4096];
                loop {
                    let count = stream.read(&mut chunk).unwrap();
                    assert!(count > 0);
                    bytes.extend_from_slice(&chunk[..count]);
                    if let Some(at) = bytes
                        .windows(4)
                        .position(|w| w == b"\r\n\r\n")
                        .map(|n| n + 4)
                    {
                        let head = std::str::from_utf8(&bytes[..at]).unwrap();
                        let size: usize = head
                            .lines()
                            .find_map(|line| {
                                line.split_once(':')
                                    .filter(|(key, _)| key.eq_ignore_ascii_case("content-length"))
                                    .map(|(_, value)| value.trim().parse().unwrap())
                            })
                            .unwrap();
                        if bytes.len() == at + size {
                            break;
                        }
                    }
                    assert!(bytes.len() < 1_000_000);
                }
                output.lock().unwrap().push(bytes);
                let body = serde_json::to_vec(&response).unwrap();
                let headers = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                );
                stream.write_all(headers.as_bytes()).unwrap();
                stream.write_all(&body).unwrap();
            }
        });
        Self {
            endpoint,
            captured,
            stop,
            worker: Some(worker),
        }
    }
}
impl Drop for Server {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        self.worker.take().unwrap().join().unwrap();
    }
}
fn session(expiration: SystemTime) -> anyhow::Result<AwsSession> {
    AwsSession::new(
        Zeroizing::new("AKIDREVIEWEXAMPLE".into()),
        Zeroizing::new("synthetic-review-secret-never-real".into()),
        Some(Zeroizing::new(TOKEN.into())),
        Some(expiration),
    )
}
fn spec(endpoint: String) -> BedrockSpec {
    BedrockSpec {
        trusted_network: true,
        endpoint,
        region: "eu-central-1".into(),
        remote_model_id: "arn:aws:bedrock:eu-central-1:123456789012:inference-profile/a/../b"
            .into(),
        model: "review-model".into(),
        model_version: "review-version".into(),
        credential_name: "review-fixture".into(),
        dialect: Dialect::Converse {
            output: Output::TextJson,
            max_output_tokens: 128,
            vision: false,
        },
        allow_loopback_http: true,
        additional_root_certificates: vec![],
    }
}
fn request(operation: &str) -> Value {
    let text = "fn review() {}";
    let anchor = json!({"sourceId":hash("entity"),"path":"main.rs","contentHash":hash(text),"startByte":0,"endByte":text.len()});
    let mut payload = json!({"snapshotId":hash("snapshot"),"sources":[{"anchor":anchor,"text":text,"omittedBytes":0}]});
    if operation == "summarize" {
        payload["instructions"] = json!("Briefly summarize captured source.");
    }
    if operation == "rerank" {
        payload["query"] = json!("review");
    }
    json!({"schema":1,"operation":operation,"model":"review-model","modelVersion":"review-version","requestId":hash("request"),"inputHash":identity(&payload),"taskId":"review-task","currency":"EUR","costLimitMicros":10,"payload":payload})
}
fn response(data: Value) -> Value {
    json!({"output":{"message":{"role":"assistant","content":[{"text":serde_json::to_string(&data).unwrap()}]}},"stopReason":"end_turn"})
}
fn summary() -> Value {
    json!({"statements":[{"text":"A captured function.","evidenceEntityIds":[hash("entity")]}]})
}
fn invoke(host: &BedrockSpec, input: &Value) -> Value {
    model_bedrock::provider_reply(
        host,
        &serde_json::to_vec(input).unwrap(),
        &|_, _| session(SystemTime::now() + Duration::from_secs(3600)),
        &Deadline::new(5000),
    )
    .unwrap()
}

#[test]
fn missing_usage_stays_unknown_and_reported_zero_remains_known() {
    for (usage, expected) in [
        (None, Value::Null),
        (
            Some(json!({"inputTokens":0,"outputTokens":0})),
            json!({"inputTokens":0,"outputTokens":0,"cachedInputTokens":0}),
        ),
    ] {
        let mut raw = response(summary());
        if let Some(usage) = usage {
            raw["usage"] = usage;
        }
        let server = Server::new(raw);
        let reply = invoke(&spec(server.endpoint.clone()), &request("summarize"));
        assert!(reply.get("error").is_none(), "{reply}");
        assert_eq!(reply["usage"], expected);
        assert!(reply["actualCostMicros"].is_null());
        assert_eq!(server.captured.lock().unwrap().len(), 1);
    }
}
#[test]
fn unicode_escaped_inner_object_keys_never_echo_temporary_credentials() {
    // A valid schema key is deliberately also the synthetic session token.
    // Removing the decoded-key guard would accept this response and echo it;
    // an unknown schema key would merely test schema rejection instead.
    let token = "statements";
    let encoded = token
        .chars()
        .map(|ch| format!("\\u{:04x}", ch as u32))
        .collect::<String>();
    let mut raw = response(summary());
    raw["output"]["message"]["content"][0]["text"] = json!(format!(
        "{{\"{encoded}\":[{{\"text\":\"A captured function.\",\"evidenceEntityIds\":[\"{}\"]}}]}}",
        hash("entity")
    ));
    raw["usage"] = json!({"inputTokens":4,"outputTokens":2});
    assert!(!raw.to_string().contains(token));
    let server = Server::new(raw);
    let reply = model_bedrock::provider_reply(
        &spec(server.endpoint.clone()),
        &serde_json::to_vec(&request("summarize")).unwrap(),
        &|_, _| {
            AwsSession::new(
                Zeroizing::new("AKIDREVIEWEXAMPLE".into()),
                Zeroizing::new("synthetic-review-secret-never-real".into()),
                Some(Zeroizing::new(token.into())),
                Some(SystemTime::now() + Duration::from_secs(3600)),
            )
        },
        &Deadline::new(5000),
    )
    .unwrap();
    assert!(reply.get("error").is_some());
    assert!(reply.get("data").is_none());
    assert!(!reply.to_string().contains(token));
    assert_eq!(reply["usage"]["inputTokens"], 4);
}
#[test]
fn session_expiring_inside_credential_callback_never_reaches_wire() {
    let server = Server::new(response(summary()));
    let host = spec(server.endpoint.clone());
    let reads = AtomicUsize::new(0);
    let result = model_bedrock::provider_reply(
        &host,
        &serde_json::to_vec(&request("summarize")).unwrap(),
        &|_, _| {
            reads.fetch_add(1, Ordering::SeqCst);
            let session = session(SystemTime::now() + Duration::from_millis(100))?;
            thread::sleep(Duration::from_millis(150));
            Ok(session)
        },
        &Deadline::new(5000),
    );
    assert!(result.is_err());
    assert_eq!(reads.load(Ordering::SeqCst), 1);
    assert!(server.captured.lock().unwrap().is_empty());
}
#[test]
fn unsupported_dialect_operation_and_wrong_source_scope_precede_credentials() {
    let reads = AtomicUsize::new(0);
    let mut host = spec("http://127.0.0.1:1/base/".into());
    let mut wrong = request("summarize");
    wrong["payload"]["sources"][0]["anchor"]["endByte"] = json!(999);
    wrong["inputHash"] = json!(identity(&wrong["payload"]));
    for (dialect, input) in [
        (Dialect::TitanV1, request("summarize")),
        (host.dialect, wrong),
    ] {
        host.dialect = dialect;
        let result = model_bedrock::provider_reply(
            &host,
            &serde_json::to_vec(&input).unwrap(),
            &|_, _| {
                reads.fetch_add(1, Ordering::SeqCst);
                anyhow::bail!("must not read")
            },
            &Deadline::new(1000),
        );
        assert!(result.is_err());
    }
    assert_eq!(reads.load(Ordering::SeqCst), 0);
}
#[test]
fn rerank_rejects_foreign_missing_duplicate_and_out_of_range_results() {
    for items in [
        json!([]),
        json!([{"entityId":hash("foreign"),"score":0.5}]),
        json!([{"entityId":hash("entity"),"score":1.1}]),
        json!([{"entityId":hash("entity"),"score":0.5},{"entityId":hash("entity"),"score":0.4}]),
    ] {
        let mut raw = response(json!({"items":items}));
        raw["usage"] = json!({"inputTokens":2,"outputTokens":1});
        let server = Server::new(raw);
        let reply = invoke(&spec(server.endpoint.clone()), &request("rerank"));
        assert!(reply.get("error").is_some(), "{reply}");
        assert!(reply.get("data").is_none());
        assert_eq!(reply["usage"]["inputTokens"], 2);
        assert_eq!(server.captured.lock().unwrap().len(), 1);
    }
}
#[test]
fn arn_slashes_and_dot_segments_stay_inside_one_signed_model_segment() {
    let server = Server::new(response(summary()));
    let reply = invoke(&spec(server.endpoint.clone()), &request("summarize"));
    assert!(reply.get("error").is_none());
    let captured = server.captured.lock().unwrap();
    let wire = std::str::from_utf8(&captured[0]).unwrap();
    assert!(wire.starts_with("POST /base/model/arn%3Aaws%3Abedrock%3Aeu-central-1%3A123456789012%3Ainference-profile%2Fa%2F..%2Fb/converse HTTP/1.1\r\n"), "unexpected request path");
    assert!(wire.contains("/eu-central-1/bedrock/aws4_request"));
    let auth = wire
        .lines()
        .find(|line| line.to_ascii_lowercase().starts_with("authorization:"))
        .unwrap();
    for signed in [
        "host",
        "content-type",
        "x-amz-content-sha256",
        "x-amz-security-token",
        "x-amz-date",
    ] {
        assert!(auth.contains(signed));
    }
}

#[test]
fn cohere_maps_two_ordered_vectors_to_exact_supplied_source_ids() {
    let server = Server::new(json!({"embeddings":{"float":[vec![0.25;256],vec![0.75;256]]}}));
    let mut host = spec(server.endpoint.clone());
    host.dialect = Dialect::CohereV4 { dimensions: 256 };
    let mut input = request("embed");
    let mut second = input["payload"]["sources"][0].clone();
    second["anchor"]["sourceId"] = json!(hash("second entity"));
    input["payload"]["sources"]
        .as_array_mut()
        .unwrap()
        .push(second);
    input["inputHash"] = json!(identity(&input["payload"]));
    let reply = invoke(&host, &input);
    assert!(reply.get("error").is_none(), "{reply}");
    assert_eq!(reply["data"]["items"][0]["sourceId"], hash("entity"));
    assert_eq!(reply["data"]["items"][0]["vector"][0], 0.25);
    assert_eq!(reply["data"]["items"][1]["sourceId"], hash("second entity"));
    assert_eq!(reply["data"]["items"][1]["vector"][0], 0.75);
    assert!(reply["usage"].is_null());
    assert!(reply["actualCostMicros"].is_null());
    assert_eq!(server.captured.lock().unwrap().len(), 1);
}
