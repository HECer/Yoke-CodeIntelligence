//! Owned TCP/TLS protocol fixtures. No external model or real credential is used.
use serde_json::{Value, json};
use std::{
    io::{Read, Write},
    net::TcpListener,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    thread,
    time::Duration,
};
use yoke::{
    capture::Deadline,
    model::{hash, identity},
    model_bedrock::{self, AwsSession, BedrockSpec, Dialect, Output},
};
use zeroize::Zeroizing;
trait FixtureIo: Read + Write {}
impl<T: Read + Write> FixtureIo for T {}
struct Server {
    endpoint: String,
    received: Arc<Mutex<Option<(String, Value)>>>,
    stop: Arc<AtomicBool>,
    worker: Option<thread::JoinHandle<()>>,
}
impl Server {
    fn new(response: Value) -> Self {
        Self::raw(
            serde_json::to_vec(&response).unwrap(),
            "200 OK",
            "",
            None,
            Duration::ZERO,
        )
    }
    fn raw(
        body: Vec<u8>,
        status: &str,
        extra: &str,
        tls: Option<Arc<rustls::ServerConfig>>,
        delay: Duration,
    ) -> Self {
        let status = status.to_owned();
        let extra = extra.to_owned();
        let scheme = if tls.is_some() { "https" } else { "http" };
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let endpoint = format!("{scheme}://{}/base/", listener.local_addr().unwrap());
        let received = Arc::new(Mutex::new(None));
        let capture = received.clone();
        let stop = Arc::new(AtomicBool::new(false));
        let flag = stop.clone();
        let worker = thread::spawn(move || {
            while !flag.load(Ordering::SeqCst) {
                match listener.accept() {
                    Ok((stream, _)) => {
                        stream.set_nonblocking(false).unwrap();
                        stream
                            .set_read_timeout(Some(Duration::from_millis(250)))
                            .unwrap();
                        stream
                            .set_write_timeout(Some(Duration::from_millis(250)))
                            .unwrap();
                        let mut stream: Box<dyn FixtureIo> = if let Some(config) = tls.as_ref() {
                            Box::new(rustls::StreamOwned::new(
                                rustls::ServerConnection::new(config.clone()).unwrap(),
                                stream,
                            ))
                        } else {
                            Box::new(stream)
                        };
                        let mut bytes = Vec::new();
                        let mut chunk = [0; 4096];
                        let mut expected = None;
                        while bytes.len() < 2_000_000 {
                            let count = match stream.read(&mut chunk) {
                                Ok(0) | Err(_) => return,
                                Ok(n) => n,
                            };
                            bytes.extend_from_slice(&chunk[..count]);
                            if let Some(end) = bytes
                                .windows(4)
                                .position(|v| v == b"\r\n\r\n")
                                .map(|n| n + 4)
                            {
                                let head = String::from_utf8(bytes[..end].to_vec()).unwrap();
                                let length = head
                                    .lines()
                                    .find_map(|l| {
                                        l.split_once(':')
                                            .filter(|(k, _)| {
                                                k.eq_ignore_ascii_case("content-length")
                                            })
                                            .map(|(_, v)| v.trim().parse::<usize>().unwrap())
                                    })
                                    .unwrap();
                                expected = Some((end, length, head));
                            }
                            if expected
                                .as_ref()
                                .is_some_and(|(at, n, _)| bytes.len() >= at + n)
                            {
                                break;
                            }
                        }
                        if let Some((at, n, head)) = expected {
                            if bytes.len() != at + n {
                                return;
                            }
                            let request_body: Value = serde_json::from_slice(&bytes[at..]).unwrap();
                            *capture.lock().unwrap() = Some((head, request_body));
                            thread::sleep(delay);
                            let head = format!(
                                "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n{extra}\r\n",
                                body.len()
                            );
                            let _ = stream
                                .write_all(head.as_bytes())
                                .and_then(|_| stream.write_all(&body))
                                .and_then(|_| stream.flush());
                        }
                        return;
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(5))
                    }
                    Err(_) => return,
                }
            }
        });
        Self {
            endpoint,
            received,
            stop,
            worker: Some(worker),
        }
    }
}
impl Drop for Server {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(worker) = self.worker.take() {
            worker.join().unwrap();
        }
    }
}
fn spec(endpoint: String, dialect: Dialect) -> BedrockSpec {
    BedrockSpec {
        trusted_network: true,
        endpoint,
        region: "us-east-1".into(),
        remote_model_id: "arn:aws:bedrock:us-east-1:123456789012:inference-profile/fixture".into(),
        model: "fixture-request-model".into(),
        model_version: "fixture-v1".into(),
        credential_name: "owned-fixture".into(),
        dialect,
        allow_loopback_http: true,
        additional_root_certificates: vec![],
    }
}
fn credentials(name: &str, deadline: &Deadline) -> anyhow::Result<AwsSession> {
    deadline.check()?;
    assert_eq!(name, "owned-fixture");
    AwsSession::new(
        Zeroizing::new("AKIDEXAMPLE".into()),
        Zeroizing::new("wJalrXUtnFEMI/K7MDENG+bPxRfiCYEXAMPLEKEY".into()),
        Some(Zeroizing::new("synthetic-session-not-real".into())),
        Some(std::time::SystemTime::now() + Duration::from_secs(3600)),
    )
}
fn request(operation: &str) -> Value {
    let text = "pub fn fixture() {}";
    let anchor = json!({"sourceId":hash(b"fixture entity"),"path":"node_modules/pkg/index.rs","contentHash":hash(text),"startByte":0,"endByte":text.len()});
    let mut payload = json!({"snapshotId":hash(b"snapshot"),"sources":[{"anchor":anchor,"text":text,"omittedBytes":0}]});
    if operation == "summarize" {
        payload["instructions"] = json!("Summarize supplied source.");
    }
    json!({"schema":1,"operation":operation,"model":"fixture-request-model","modelVersion":"fixture-v1","requestId":hash(b"request"),"inputHash":identity(&payload),"taskId":"fixture-task","currency":"EUR","costLimitMicros":10,"payload":payload})
}
fn response() -> Value {
    json!({"output":{"message":{"role":"assistant","content":[{"text":serde_json::to_string(&json!({"statements":[{"text":"Owned source fixture.","evidenceEntityIds":[hash(b"fixture entity")]}]})).unwrap()}]}},"stopReason":"end_turn","usage":{"inputTokens":100,"outputTokens":10,"cacheReadInputTokens":20,"cacheWriteInputTokens":30}})
}
fn invoke(spec: &BedrockSpec, input: &Value) -> anyhow::Result<Value> {
    model_bedrock::provider_reply(
        spec,
        &serde_json::to_vec(input).unwrap(),
        &credentials,
        &Deadline::new(5000),
    )
}
#[test]
fn converse_signs_one_source_bound_request_and_preserves_unknown_cost() {
    let server = Server::new(response());
    let selected = spec(
        server.endpoint.clone(),
        Dialect::Converse {
            output: Output::JsonSchema,
            max_output_tokens: 256,
            vision: false,
        },
    );
    let input = request("summarize");
    let reply = invoke(&selected, &input).expect("one native Bedrock request must work");
    assert!(reply.get("error").is_none(), "{reply}");
    assert_eq!(
        reply["data"]["statements"][0]["text"],
        "Owned source fixture."
    );
    assert_eq!(
        reply["sourceBindings"][0],
        input["payload"]["sources"][0]["anchor"]
    );
    assert_eq!(
        reply["usage"],
        json!({"inputTokens":150,"outputTokens":10,"cachedInputTokens":20})
    );
    assert!(reply["actualCostMicros"].is_null());
    let captured = server.received.lock().unwrap();
    let (headers, body) = captured.as_ref().unwrap();
    assert!(headers.starts_with("POST /base/model/arn%3Aaws%3Abedrock%3Aus-east-1%3A123456789012%3Ainference-profile%2Ffixture/converse HTTP/1.1\r\n"), "{headers}");
    assert!(headers.contains("Credential=AKIDEXAMPLE/"));
    assert!(headers.contains("/us-east-1/bedrock/aws4_request"));
    assert!(
        headers
            .to_lowercase()
            .contains("x-amz-security-token: synthetic-session-not-real")
    );
    assert_eq!(body["outputConfig"]["textFormat"]["type"], "json_schema");
    assert!(body["outputConfig"]["textFormat"]["structure"]["jsonSchema"]["schema"].is_string());
    assert_eq!(body["inferenceConfig"]["maxTokens"], 256);
    assert!(body.get("model").is_none());
}
#[test]
fn titan_and_cohere_each_use_exactly_one_wire_batch_and_ordered_float_vectors() {
    for (dialect, raw, dimensions) in [
        (
            Dialect::TitanV1,
            json!({"embedding":vec![0.25;1536],"inputTextTokenCount":7}),
            1536,
        ),
        (
            Dialect::TitanV2 {
                dimensions: 256,
                normalize: true,
            },
            json!({"embedding":vec![0.25;256],"inputTextTokenCount":7}),
            256,
        ),
        (
            Dialect::CohereV3,
            json!({"embeddings":[vec![0.25;1024]]}),
            1024,
        ),
        (
            Dialect::CohereV4 { dimensions: 256 },
            json!({"embeddings":{"float":[vec![0.25;256]]}}),
            256,
        ),
    ] {
        let server = Server::new(raw);
        let selected = spec(server.endpoint.clone(), dialect);
        let input = request("embed");
        let reply = invoke(&selected, &input).expect("one embedding call");
        assert!(reply.get("error").is_none(), "{reply}");
        assert_eq!(
            reply["data"]["items"][0]["vector"]
                .as_array()
                .unwrap()
                .len(),
            dimensions
        );
        assert_eq!(
            reply["data"]["items"][0]["sourceId"],
            hash(b"fixture entity")
        );
        assert!(reply["actualCostMicros"].is_null());
        if matches!(dialect, Dialect::CohereV3 | Dialect::CohereV4 { .. }) {
            assert!(reply["usage"].is_null());
        } else {
            assert_eq!(reply["usage"]["inputTokens"], 7);
        }
        let captured = server.received.lock().unwrap();
        let (head, body) = captured.as_ref().unwrap();
        assert!(head.lines().next().unwrap().ends_with("/invoke HTTP/1.1"));
        if matches!(dialect, Dialect::CohereV3 | Dialect::CohereV4 { .. }) {
            assert_eq!(body["input_type"], "search_document");
            assert_eq!(body["truncate"], "NONE");
            assert_eq!(body["texts"].as_array().unwrap().len(), 1);
        } else {
            assert_eq!(body["inputText"], "pub fn fixture() {}");
        }
    }
}
#[test]
fn oversized_titan_batch_and_invalid_authority_never_read_credentials() {
    let mut input = request("embed");
    let mut second = input["payload"]["sources"][0].clone();
    second["anchor"]["sourceId"] = json!(hash(b"second"));
    input["payload"]["sources"]
        .as_array_mut()
        .unwrap()
        .push(second);
    input["inputHash"] = json!(identity(&input["payload"]));
    let selected = spec("http://127.0.0.1:1/base/".into(), Dialect::TitanV1);
    let reads = AtomicUsize::new(0);
    let result = model_bedrock::provider_reply(
        &selected,
        &serde_json::to_vec(&input).unwrap(),
        &|_, _| {
            reads.fetch_add(1, Ordering::SeqCst);
            anyhow::bail!("must not read credentials")
        },
        &Deadline::new(1000),
    );
    assert!(result.is_err());
    assert_eq!(reads.load(Ordering::SeqCst), 0);
    for endpoint in [
        "http://example.com/",
        "http://localhost:1/",
        "https://user@example.com/",
        "https://example.com/a/../b",
        "https://example.com/?x=1",
    ] {
        let mut invalid = selected.clone();
        invalid.endpoint = endpoint.into();
        assert!(
            model_bedrock::validate_spec(&invalid).is_err(),
            "{endpoint}"
        );
    }
}
#[test]
fn refusals_preserve_valid_usage_and_decoded_secret_echo_is_never_returned() {
    let mut refusal = response();
    refusal["stopReason"] = json!("max_tokens");
    let mut escaped = response();
    escaped["output"]["message"]["content"][0]["text"] = json!(format!(
        "{{\"statements\":[{{\"text\":\"{}\",\"evidenceEntityIds\":[\"{}\"]}}]}}",
        "synthetic-session-not-real"
            .chars()
            .map(|c| format!("\\u{:04x}", c as u32))
            .collect::<String>(),
        hash(b"fixture entity")
    ));
    for raw in [refusal, escaped] {
        let server = Server::new(raw);
        let selected = spec(
            server.endpoint.clone(),
            Dialect::Converse {
                output: Output::TextJson,
                max_output_tokens: 256,
                vision: false,
            },
        );
        let reply = invoke(&selected, &request("summarize")).unwrap();
        assert!(reply.get("error").is_some(), "{reply}");
        assert!(reply.get("data").is_none());
        assert_eq!(reply["usage"]["inputTokens"], 150);
        assert!(!reply.to_string().contains("synthetic-session-not-real"));
    }
}
fn independent_hmac(key: &[u8], message: &[u8]) -> Vec<u8> {
    use sha2::{Digest, Sha256};
    let key = if key.len() > 64 {
        Sha256::digest(key).to_vec()
    } else {
        key.to_vec()
    };
    let mut inner = [0x36u8; 64];
    let mut outer = [0x5cu8; 64];
    for (i, b) in key.iter().enumerate() {
        inner[i] ^= b;
        outer[i] ^= b;
    }
    let mut h = Sha256::new();
    h.update(inner);
    h.update(message);
    let digest = h.finalize();
    let mut h = Sha256::new();
    h.update(outer);
    h.update(digest);
    h.finalize().to_vec()
}
// Independent verifier uses only SHA256 and an explicit RFC2104 HMAC, never AWS signing code.
fn verify_signature(head: &str, body: &Value) {
    let headers: std::collections::BTreeMap<_, _> = head
        .lines()
        .skip(1)
        .filter_map(|line| {
            line.split_once(':')
                .map(|(k, v)| (k.to_ascii_lowercase(), v.trim().to_owned()))
        })
        .collect();
    let authorization = &headers["authorization"];
    let scope = authorization
        .split("Credential=AKIDEXAMPLE/")
        .nth(1)
        .unwrap()
        .split(',')
        .next()
        .unwrap();
    let names = authorization
        .split("SignedHeaders=")
        .nth(1)
        .unwrap()
        .split(',')
        .next()
        .unwrap();
    for required in [
        "host",
        "content-type",
        "x-amz-date",
        "x-amz-content-sha256",
        "x-amz-security-token",
    ] {
        assert!(
            names.split(';').any(|n| n == required),
            "unsigned {required}"
        );
    }
    let rawpath = head.lines().next().unwrap().split(' ').nth(1).unwrap();
    let canonicalpath = rawpath.replace('%', "%25");
    let canonicalheaders = names
        .split(';')
        .map(|name| {
            format!(
                "{name}:{}\n",
                headers[name]
                    .split_whitespace()
                    .collect::<Vec<_>>()
                    .join(" ")
            )
        })
        .collect::<String>();
    let payload_hash = hash(serde_json::to_vec(body).unwrap());
    assert_eq!(headers["x-amz-content-sha256"], payload_hash);
    let canonical = format!("POST\n{canonicalpath}\n\n{canonicalheaders}\n{names}\n{payload_hash}");
    let date = &headers["x-amz-date"];
    let to_sign = format!("AWS4-HMAC-SHA256\n{date}\n{scope}\n{}", hash(canonical));
    let k_date = independent_hmac(
        b"AWS4wJalrXUtnFEMI/K7MDENG+bPxRfiCYEXAMPLEKEY",
        &date.as_bytes()[..8],
    );
    let k_region = independent_hmac(&k_date, b"us-east-1");
    let k_service = independent_hmac(&k_region, b"bedrock");
    let signing = independent_hmac(&k_service, b"aws4_request");
    let expected = hex::encode(independent_hmac(&signing, to_sign.as_bytes()));
    assert_eq!(authorization.split("Signature=").nth(1).unwrap(), expected);
}
#[test]
fn actual_wire_signature_binds_arn_body_host_port_date_region_and_session_token() {
    let server = Server::new(response());
    let selected = spec(
        server.endpoint.clone(),
        Dialect::Converse {
            output: Output::TextJson,
            max_output_tokens: 256,
            vision: false,
        },
    );
    let reply = invoke(&selected, &request("summarize")).unwrap();
    assert!(reply.get("error").is_none(), "{reply}");
    let captured = server.received.lock().unwrap();
    let (head, body) = captured.as_ref().unwrap();
    verify_signature(head, body);
    let mut mutated = body.clone();
    mutated["inferenceConfig"]["maxTokens"] = json!(999);
    assert_ne!(
        hash(serde_json::to_vec(&mutated).unwrap()),
        hash(serde_json::to_vec(body).unwrap())
    );
}
#[test]
fn tls_uses_owned_roots_and_never_accepts_untrusted_fixture_certificate() {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let certificate = rcgen::generate_simple_self_signed(vec!["127.0.0.1".into()]).unwrap();
    let der = certificate.cert.der().to_vec();
    let config = Arc::new(
        rustls::ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(
                vec![certificate.cert.der().clone()],
                rustls::pki_types::PrivatePkcs8KeyDer::from(certificate.key_pair.serialize_der())
                    .into(),
            )
            .unwrap(),
    );
    for trusted in [false, true] {
        let server = Server::raw(
            serde_json::to_vec(&response()).unwrap(),
            "200 OK",
            "",
            Some(config.clone()),
            Duration::ZERO,
        );
        let mut selected = spec(
            server.endpoint.clone(),
            Dialect::Converse {
                output: Output::TextJson,
                max_output_tokens: 256,
                vision: false,
            },
        );
        if trusted {
            selected.additional_root_certificates.push(der.clone());
        }
        let result = invoke(&selected, &request("summarize"));
        if trusted {
            let reply = result.unwrap();
            assert!(reply.get("error").is_none(), "{reply}");
            let captured = server.received.lock().unwrap();
            let (head, body) = captured.as_ref().unwrap();
            verify_signature(head, body);
        } else {
            assert!(result.is_err());
            assert!(server.received.lock().unwrap().is_none());
        }
    }
}
#[test]
fn independent_signature_oracle_matches_official_aws_post_vanilla_vector() {
    // AWS Apache-2.0 aws-signing-test-suite/v4/post-vanilla, bundled in
    // aws-sigv4 1.6.0. This validates the independent wire verifier's HMAC chain.
    let canonical = "POST\n/\n\nhost:example.amazonaws.com\nx-amz-date:20150830T123600Z\n\nhost;x-amz-date\ne3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";
    assert_eq!(
        hash(canonical),
        "553f88c9e4d10fc9e109e2aeb65f030801b70c2f6468faca261d401ae622fc87"
    );
    let text = format!(
        "AWS4-HMAC-SHA256\n20150830T123600Z\n20150830/us-east-1/service/aws4_request\n{}",
        hash(canonical)
    );
    let date = independent_hmac(b"AWS4wJalrXUtnFEMI/K7MDENG+bPxRfiCYEXAMPLEKEY", b"20150830");
    let region = independent_hmac(&date, b"us-east-1");
    let service = independent_hmac(&region, b"service");
    let key = independent_hmac(&service, b"aws4_request");
    assert_eq!(
        hex::encode(independent_hmac(&key, text.as_bytes())),
        "5da7c1a2acd57cee7505fc6676e4e544621c30862966e37dddb68e92efbe5d6b"
    );
}
#[test]
fn session_admission_refuses_expiry_header_injection_and_partial_temporary_identity() {
    let make = |token: Option<&str>, expiry| {
        AwsSession::new(
            Zeroizing::new("AKIDEXAMPLE".into()),
            Zeroizing::new("fixture-secret".into()),
            token.map(|t| Zeroizing::new(t.into())),
            expiry,
        )
    };
    assert!(make(Some("fixture-token"), None).is_err());
    assert!(
        make(
            None,
            Some(std::time::SystemTime::now() + Duration::from_secs(3600))
        )
        .is_err()
    );
    assert!(
        make(
            Some("fixture\r\ntoken"),
            Some(std::time::SystemTime::now() + Duration::from_secs(3600))
        )
        .is_err()
    );
    assert!(
        make(
            Some("fixture-token"),
            Some(std::time::SystemTime::UNIX_EPOCH)
        )
        .is_err()
    );
    // A session token larger than Windows' 2560-byte single-blob limit is valid
    // within the worker's separately documented local 64KiB admission.
    assert!(
        make(
            Some(&"x".repeat(4096)),
            Some(std::time::SystemTime::now() + Duration::from_secs(3600))
        )
        .is_ok()
    );
}
#[test]
fn converse_refuses_overlarge_image_before_credential_read() {
    use base64::Engine as _;
    let mut bytes = vec![0u8; 3_750_001];
    bytes[..8].copy_from_slice(b"\x89PNG\r\n\x1a\n");
    let id = hash(b"media");
    let digest = hash(&bytes);
    let payload = json!({"assetId":id,"contentHash":digest,"path":"fixture.png","instructions":"Describe captured image.","sources":[],"mediaBase64":base64::prelude::BASE64_STANDARD.encode(&bytes),"mediaAnchor":{"sourceId":id,"contentHash":digest,"path":"fixture.png","startByte":0,"endByte":bytes.len()}});
    let input = json!({"schema":1,"operation":"describe_media","model":"fixture-request-model","modelVersion":"fixture-v1","requestId":hash(b"request"),"inputHash":identity(&payload),"taskId":"fixture-task","currency":"EUR","costLimitMicros":10,"payload":payload});
    let selected = spec(
        "http://127.0.0.1:1/base/".into(),
        Dialect::Converse {
            output: Output::TextJson,
            max_output_tokens: 256,
            vision: true,
        },
    );
    let reads = AtomicUsize::new(0);
    let result = model_bedrock::provider_reply(
        &selected,
        &serde_json::to_vec(&input).unwrap(),
        &|_, _| {
            reads.fetch_add(1, Ordering::SeqCst);
            anyhow::bail!("must not read credentials")
        },
        &Deadline::new(10000),
    );
    assert!(result.is_err());
    assert_eq!(
        reads.load(Ordering::SeqCst),
        0,
        "Bedrock image admission must precede credential access"
    );
}
#[test]
fn malformed_vectors_usage_and_tool_blocks_never_become_success() {
    for raw in [
        json!({"embedding":vec![0.25;255],"inputTextTokenCount":7}),
        json!({"embedding":vec![0.0;256],"inputTextTokenCount":7}),
        json!({"embedding":vec![0.25;256],"inputTextTokenCount":-1}),
    ] {
        let server = Server::new(raw);
        let reply = invoke(
            &spec(
                server.endpoint.clone(),
                Dialect::TitanV2 {
                    dimensions: 256,
                    normalize: true,
                },
            ),
            &request("embed"),
        )
        .unwrap();
        assert!(reply.get("error").is_some(), "{reply}");
        assert!(reply.get("data").is_none());
        assert!(reply["actualCostMicros"].is_null());
    }
    let mut raw = response();
    raw["output"]["message"]["content"][0]["toolUse"] = json!({"name":"untrusted"});
    let server = Server::new(raw);
    let reply = invoke(
        &spec(
            server.endpoint.clone(),
            Dialect::Converse {
                output: Output::TextJson,
                max_output_tokens: 256,
                vision: false,
            },
        ),
        &request("summarize"),
    )
    .unwrap();
    assert!(reply.get("error").is_some());
    assert_eq!(reply["usage"]["inputTokens"], 150);
    let mut raw = response();
    raw["usage"]["cacheReadInputTokens"] = Value::Null;
    let server = Server::new(raw);
    let reply = invoke(
        &spec(
            server.endpoint.clone(),
            Dialect::Converse {
                output: Output::TextJson,
                max_output_tokens: 256,
                vision: false,
            },
        ),
        &request("summarize"),
    )
    .unwrap();
    assert!(reply.get("error").is_some());
    assert!(reply["usage"].is_null());
}
#[test]
fn network_authority_input_integrity_deadline_and_cohere_batch_precede_credentials() {
    let selected = spec("http://127.0.0.1:1/base/".into(), Dialect::CohereV3);
    let mut wronghash = request("embed");
    wronghash["inputHash"] = json!(hash(b"wrong"));
    let mut batch = request("embed");
    let original = batch["payload"]["sources"][0].clone();
    let rows = (0..97)
        .map(|i| {
            let mut row = original.clone();
            row["anchor"]["sourceId"] = json!(hash(format!("source-{i}")));
            row
        })
        .collect::<Vec<_>>();
    batch["payload"]["sources"] = json!(rows);
    batch["inputHash"] = json!(identity(&batch["payload"]));
    let mut untrusted = selected.clone();
    untrusted.trusted_network = false;
    let reads = AtomicUsize::new(0);
    for (host, input, ms) in [
        (&selected, wronghash, 1000),
        (&selected, batch, 1000),
        (&untrusted, request("embed"), 1000),
        (&selected, request("embed"), 0),
    ] {
        let result = model_bedrock::provider_reply(
            host,
            &serde_json::to_vec(&input).unwrap(),
            &|_, _| {
                reads.fetch_add(1, Ordering::SeqCst);
                anyhow::bail!("must not read")
            },
            &Deadline::new(ms),
        );
        assert!(result.is_err());
    }
    assert_eq!(reads.load(Ordering::SeqCst), 0);
}
#[test]
fn non_200_status_is_one_attempt_with_unknown_billing_and_no_service_body() {
    let server = Server::raw(
        b"synthetic-session-not-real".to_vec(),
        "429 Too Many Requests",
        "",
        None,
        Duration::ZERO,
    );
    let reply = invoke(
        &spec(
            server.endpoint.clone(),
            Dialect::Converse {
                output: Output::TextJson,
                max_output_tokens: 256,
                vision: false,
            },
        ),
        &request("summarize"),
    )
    .unwrap();
    assert!(reply.get("error").is_some());
    assert!(reply.get("data").is_none());
    assert!(reply["usage"].is_null());
    assert!(reply["actualCostMicros"].is_null());
    assert!(!reply.to_string().contains("synthetic-session-not-real"));
    assert!(server.received.lock().unwrap().is_some());
}
