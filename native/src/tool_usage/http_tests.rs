use super::*;
use serde_json::Value;
use std::{io::Write, net::TcpListener, sync::mpsc, thread};

fn fixture(
    responses: Vec<(u16, &'static str, String, &'static str)>,
) -> (Host, mpsc::Receiver<Value>, thread::JoinHandle<()>) {
    fixture_changing_current(responses, None)
}
fn fixture_changing_current(
    responses: Vec<(u16, &'static str, String, &'static str)>,
    current: Option<Arc<AtomicBool>>,
) -> (Host, mpsc::Receiver<Value>, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let address = listener.local_addr().unwrap();
    let (sender, receiver) = mpsc::channel();
    let worker = thread::spawn(move || {
        for (status, media, body, extra) in responses {
            let until = std::time::Instant::now() + std::time::Duration::from_secs(10);
            let (mut stream, _) = loop {
                match listener.accept() {
                    Ok(peer) => break peer,
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        assert!(
                            std::time::Instant::now() < until,
                            "owned count request did not arrive"
                        );
                        thread::sleep(std::time::Duration::from_millis(1));
                    }
                    Err(error) => panic!("owned count accept failed: {error}"),
                }
            };
            stream.set_nonblocking(false).unwrap();
            stream
                .set_read_timeout(Some(std::time::Duration::from_secs(5)))
                .unwrap();
            stream
                .set_write_timeout(Some(std::time::Duration::from_secs(5)))
                .unwrap();
            let mut raw = Vec::new();
            let (end, length) = loop {
                let mut block = [0; 4096];
                let n = stream.read(&mut block).unwrap();
                assert_ne!(n, 0);
                raw.extend_from_slice(&block[..n]);
                assert!(raw.len() <= 8_010_000);
                if let Some(end) = raw.windows(4).position(|b| b == b"\r\n\r\n") {
                    let header = std::str::from_utf8(&raw[..end]).unwrap();
                    assert!(header.starts_with("POST /v1/messages/count_tokens HTTP/1.1\r\n"));
                    let header = header.to_ascii_lowercase();
                    assert!(header.contains("x-api-key: owned-placeholder\r\n"));
                    assert!(header.contains("anthropic-version: 2023-06-01\r\n"));
                    let length = header
                        .lines()
                        .find_map(|line| line.strip_prefix("content-length:"))
                        .unwrap()
                        .trim()
                        .parse::<usize>()
                        .unwrap();
                    if raw.len() >= end + 4 + length {
                        break (end, length);
                    }
                }
            };
            let request = crate::json::parse(&raw[end + 4..end + 4 + length], 8_001_024).unwrap();
            assert_eq!(request["model"], MODEL);
            assert_eq!(request["messages"].as_array().unwrap().len(), 1);
            assert_eq!(request["messages"][0]["role"], "user");
            assert_eq!(request.as_object().unwrap().len(), 2);
            sender.send(request).unwrap();
            if let Some(current) = &current {
                current.store(false, Ordering::Release);
            }
            if let Err(error) = write!(
                stream,
                "HTTP/1.1 {status} Owned\r\nContent-Type: {media}\r\nContent-Length: {}\r\nConnection: close\r\n{extra}\r\n{body}",
                body.len()
            ) {
                assert!(matches!(
                    error.kind(),
                    std::io::ErrorKind::BrokenPipe
                        | std::io::ErrorKind::ConnectionReset
                        | std::io::ErrorKind::ConnectionAborted
                ));
            }
        }
    });
    let host = Host::new(
        Spec {
            endpoint: format!("http://{address}{PATH}"),
            credential_name: "owned".into(),
            allow_loopback_http: true,
            additional_root_certificates: vec![],
        },
        true,
        Arc::new(|_| Ok(Zeroizing::new("owned-placeholder".into()))),
    )
    .unwrap();
    (host, receiver, worker)
}
#[test]
fn changed_session_during_first_count_prevents_the_second_disclosure_and_all_counters() {
    use super::super::{Estimator, Stats};
    let current = Arc::new(AtomicBool::new(true));
    let (host, requests, worker) = fixture_changing_current(
        vec![(200, "application/json", r#"{"input_tokens":7}"#.into(), "")],
        Some(current.clone()),
    );
    let stats = Stats::new(
        Estimator::AnthropicClaudeSonnet4,
        Some(&host),
        &Deadline::new(10000),
    )
    .unwrap();
    let still_current = || {
        ensure!(current.load(Ordering::Acquire), "owned_session_changed");
        Ok(())
    };
    assert!(
        stats
            .measure_checked(
                "read_file",
                &json!({"input":"owned"}),
                &json!({"output":"owned"}),
                &Deadline::new(10000),
                &still_current
            )
            .is_err()
    );
    assert!(requests.recv().is_ok());
    worker.join().unwrap();
    assert_eq!(host.attempts(), 1);
    assert!(stats.view()["tools"].as_object().unwrap().is_empty());
}
#[test]
fn owned_http_sends_exact_user_text_to_pinned_count_route_without_generation_fields() {
    let (host, requests, worker) = fixture(vec![(
        200,
        "application/json",
        r#"{"input_tokens":7}"#.into(),
        "",
    )]);
    let text = "Owned \u{1f4a1} e\u{301}\r\n<|endoftext|>";
    assert_eq!(host.count(text, &Deadline::new(10000)).unwrap(), 7);
    assert_eq!(requests.recv().unwrap()["messages"][0]["content"], text);
    worker.join().unwrap();
    assert_eq!(host.attempts(), 1);
}
#[test]
fn malformed_ambiguous_or_out_of_budget_response_never_becomes_a_zero_count() {
    for body in [
        r#"{"input_tokens":true}"#,
        r#"{"input_tokens":-1}"#,
        r#"{"input_tokens":1.5}"#,
        r#"{"input_tokens":"7"}"#,
        r#"{"input_tokens":100000001}"#,
        r#"{"input_tokens":1,"input_tokens":2}"#,
        r#"{"input_tokens":1,"other":0}"#,
        r#"{}"#,
        "not json",
    ] {
        let (host, requests, worker) = fixture(vec![(200, "application/json", body.into(), "")]);
        let error = host
            .count("owned", &Deadline::new(10000))
            .unwrap_err()
            .to_string();
        assert!(
            [
                "tool_usage_http_json_invalid",
                "tool_usage_http_count_schema_invalid",
                "tool_usage_http_count_invalid"
            ]
            .contains(&error.as_str()),
            "{error}"
        );
        assert!(requests.recv().is_ok());
        worker.join().unwrap();
        assert_eq!(host.attempts(), 1);
    }
}
#[test]
fn redirect_status_media_encoding_and_response_capacity_refuse_without_retry() {
    for (status, media, body, extra) in [
        (
            302,
            "application/json",
            "{}".into(),
            "Location: http://127.0.0.1:1/foreign\r\n",
        ),
        (429, "application/json", "{}".into(), ""),
        (200, "text/plain", r#"{"input_tokens":7}"#.into(), ""),
        (
            200,
            "application/json",
            r#"{"input_tokens":7}"#.into(),
            "Content-Encoding: gzip\r\n",
        ),
        (200, "application/json", "x".repeat(MAX_RESPONSE + 1), ""),
    ] {
        let (host, requests, worker) = fixture(vec![(status, media, body, extra)]);
        assert!(host.count("owned", &Deadline::new(10000)).is_err());
        assert!(requests.recv().is_ok());
        worker.join().unwrap();
        assert_eq!(host.attempts(), 1);
    }
}
#[test]
fn missing_grant_or_invalid_endpoint_refuses_before_reading_any_credential() {
    use std::sync::atomic::AtomicUsize;
    let reads = Arc::new(AtomicUsize::new(0));
    let counted = reads.clone();
    let credentials: Arc<Credentials> = Arc::new(move |_| {
        counted.fetch_add(1, Ordering::SeqCst);
        Ok(Zeroizing::new("owned-placeholder".into()))
    });
    let spec = Spec {
        endpoint: format!("https://api.anthropic.com{PATH}"),
        credential_name: "owned".into(),
        allow_loopback_http: false,
        additional_root_certificates: vec![],
    };
    assert!(Host::new(spec.clone(), false, credentials.clone()).is_err());
    for endpoint in [
        "http://localhost/v1/messages/count_tokens",
        "http://192.0.2.1/v1/messages/count_tokens",
        "https://user:password@example.test/v1/messages/count_tokens",
        "https://example.test/v1/messages/count_tokens?secret=owned",
        "https://example.test/v1/messages/count_tokens#fragment",
        "https://example.test/v1/messages",
    ] {
        let mut invalid = spec.clone();
        invalid.endpoint = endpoint.into();
        invalid.allow_loopback_http = true;
        assert!(Host::new(invalid, true, credentials.clone()).is_err());
    }
    assert_eq!(reads.load(Ordering::SeqCst), 0);
}
#[test]
fn revoked_and_expired_grants_never_reach_credentials_or_network() {
    use std::sync::atomic::AtomicUsize;
    let reads = Arc::new(AtomicUsize::new(0));
    let counted = reads.clone();
    let host = Host::new(
        Spec {
            endpoint: format!("https://api.anthropic.com{PATH}"),
            credential_name: "owned".into(),
            allow_loopback_http: false,
            additional_root_certificates: vec![],
        },
        true,
        Arc::new(move |_| {
            counted.fetch_add(1, Ordering::SeqCst);
            Ok(Zeroizing::new("owned-placeholder".into()))
        }),
    )
    .unwrap();
    assert!(host.count("owned", &Deadline::new(0)).is_err());
    host.revoke();
    assert!(host.count("owned", &Deadline::new(10000)).is_err());
    assert_eq!(reads.load(Ordering::SeqCst), 0);
    assert_eq!(host.attempts(), 0);
}
#[test]
fn second_remote_failure_does_not_publish_partial_tool_statistics() {
    use super::super::{Estimator, Stats};
    let (host, requests, worker) = fixture(vec![
        (200, "application/json", r#"{"input_tokens":7}"#.into(), ""),
        (429, "application/json", "{}".into(), ""),
    ]);
    let stats = Stats::new(
        Estimator::AnthropicClaudeSonnet4,
        Some(&host),
        &Deadline::new(10000),
    )
    .unwrap();
    assert!(
        stats
            .measure(
                "read_file",
                &json!({"owned":"input"}),
                &json!({"owned":"output"}),
                &Deadline::new(10000)
            )
            .is_err()
    );
    for _ in 0..2 {
        assert!(requests.recv().is_ok());
    }
    worker.join().unwrap();
    assert!(stats.view()["tools"].as_object().unwrap().is_empty());
    assert_eq!(host.attempts(), 2);
}
