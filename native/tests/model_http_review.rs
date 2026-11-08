use serde_json::{Value, json};
use std::{
    io::{Read, Write},
    net::TcpListener,
    thread,
    time::{Duration, Instant},
};
use yoke::{
    capture::Deadline,
    model::{hash, identity},
    model_http::{self, AuthMode, Capabilities, Dialect, HttpProviderSpec, JsonMode, TokenLimit},
};
use zeroize::Zeroizing;

fn reply(tools: Value, content: String, credential: &str) -> Value {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let endpoint = format!("http://{}/", listener.local_addr().unwrap());
    let response = json!({"model":"remote","choices":[{"index":0,"finish_reason":"stop","message":{"role":"assistant","tool_calls":tools,"content":content}}],"usage":{"prompt_tokens":10,"completion_tokens":5}});
    let server = thread::spawn(move || {
        let stop = Instant::now() + Duration::from_secs(5);
        let mut stream = loop {
            match listener.accept() {
                Ok((stream, _)) => break stream,
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock && Instant::now() < stop => {
                    thread::sleep(Duration::from_millis(5))
                }
                Err(e) => panic!("fixture accept: {e}"),
            }
        };
        stream.set_nonblocking(false).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        stream
            .set_write_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        let mut input = Vec::new();
        let mut chunk = [0; 4096];
        loop {
            let n = stream.read(&mut chunk).unwrap();
            assert!(n > 0);
            input.extend_from_slice(&chunk[..n]);
            assert!(input.len() < 64000);
            if let Some(end) = input
                .windows(4)
                .position(|s| s == b"\r\n\r\n")
                .map(|n| n + 4)
            {
                let headers = String::from_utf8_lossy(&input[..end]);
                let length = headers
                    .lines()
                    .find_map(|line| {
                        line.split_once(':')
                            .filter(|(key, _)| key.eq_ignore_ascii_case("content-length"))
                            .map(|(_, v)| v.trim().parse::<usize>().unwrap())
                    })
                    .unwrap();
                if input.len() >= end + length {
                    break;
                }
            }
        }
        let body = serde_json::to_vec(&response).unwrap();
        write!(stream,"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",body.len()).unwrap();
        stream.write_all(&body).unwrap();
        stream.flush().unwrap();
    });
    let spec = HttpProviderSpec {
        trusted_network: true,
        endpoint,
        model: "fixture".into(),
        model_version: "v1".into(),
        expected_remote_model: "remote".into(),
        credential_name: "owned-fixture".into(),
        dialect: Dialect::OpenAiCompatible,
        auth: AuthMode::Bearer,
        json_mode: JsonMode::JsonSchema,
        token_limit: TokenLimit::MaxCompletionTokens,
        capabilities: Capabilities::default(),
        max_output_tokens: 32,
        allow_loopback_http: true,
        additional_root_certificates: vec![],
    };
    let text = "fn source(){}";
    let payload = json!({"snapshotId":hash(b"snapshot"),"instructions":"Summarize the supplied source.","sources":[{"anchor":{"sourceId":hash(b"entity"),"path":"main.rs","contentHash":hash(text),"startByte":0,"endByte":text.len()},"text":text,"omittedBytes":0}]});
    let input = json!({"schema":1,"operation":"summarize","model":"fixture","modelVersion":"v1","requestId":hash(b"request"),"inputHash":identity(&payload),"taskId":"review","currency":"EUR","costLimitMicros":10,"payload":payload});
    let result = model_http::provider_reply(
        &spec,
        &serde_json::to_vec(&input).unwrap(),
        &|_| Ok(Zeroizing::new(credential.to_owned())),
        &Deadline::new(5000),
    );
    server.join().unwrap();
    result.unwrap()
}
fn statement(text: &str) -> String {
    serde_json::to_string(
        &json!({"statements":[{"text":text,"evidenceEntityIds":[hash(b"entity")]}]}),
    )
    .unwrap()
}
#[test]
fn present_malformed_tool_calls_cannot_be_treated_as_absent_tools() {
    for tools in [
        json!({"id":"unrequested","type":"function","function":{"name":"host_action","arguments":"{}"}}),
        json!("host_action"),
        json!(true),
    ] {
        let reply = reply(
            tools,
            statement("A source-bound observation."),
            "synthetic-fixture-only",
        );
        assert!(
            reply.get("error").is_some() && reply.get("data").is_none(),
            "Malformed present tool_calls must fail closed: {reply}"
        );
        assert_eq!(reply["usage"]["inputTokens"], 10);
    }
}

#[test]
fn unicode_escaped_credential_echo_is_rejected_after_decoding_model_text() {
    let secret = "synthetic-fixture-only";
    let content =
        statement(secret).replace("synthetic-fixture-only", r"\u0073ynthetic-fixture-only");
    assert!(!content.contains(secret));
    let reply = reply(Value::Null, content, secret);
    assert!(
        reply.get("error").is_some() && reply.get("data").is_none(),
        "Decoded exact credential echo must fail closed: {reply}"
    );
    assert_eq!(reply["usage"]["inputTokens"], 10);
}

#[test]
fn credential_with_quote_and_backslash_is_rejected_after_json_unescaping() {
    let secret = r#"synthetic-"quote\-fixture"#;
    let content = statement(secret);
    assert!(!content.contains(secret));
    let reply = reply(Value::Null, content, secret);
    assert!(
        reply.get("error").is_some() && reply.get("data").is_none(),
        "JSON-unescaped exact credential echo must fail closed: {reply}"
    );
    assert_eq!(reply["usage"]["inputTokens"], 10);
}
