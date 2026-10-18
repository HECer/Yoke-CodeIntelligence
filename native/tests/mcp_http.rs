use serde_json::{Value, json};
use std::{
    io::{Read, Write},
    net::{SocketAddr, TcpStream},
    sync::Arc,
    time::{Duration, Instant},
};
use tempfile::TempDir;
use yoke::{
    mcp_http::{self, Config, Principal, Target, TlsMaterial},
    project_session::HostProfile,
    protocol::Profile,
};
use zeroize::Zeroizing;

static HTTP_TEST: std::sync::Mutex<()> = std::sync::Mutex::new(());

struct Response {
    status: u16,
    headers: std::collections::BTreeMap<String, String>,
    body: Vec<u8>,
}
fn parse(bytes: &[u8]) -> Response {
    let end = bytes
        .windows(4)
        .position(|part| part == b"\r\n\r\n")
        .unwrap()
        + 4;
    let header = std::str::from_utf8(&bytes[..end]).unwrap();
    let status = header.split_whitespace().nth(1).unwrap().parse().unwrap();
    let headers = header
        .split("\r\n")
        .skip(1)
        .filter_map(|line| line.split_once(':'))
        .map(|(key, value)| (key.to_ascii_lowercase(), value.trim().to_owned()))
        .collect();
    Response {
        status,
        headers,
        body: bytes[end..].to_vec(),
    }
}
fn exchange(address: SocketAddr, request: &[u8]) -> Response {
    let mut tcp = TcpStream::connect(address).unwrap();
    tcp.set_read_timeout(Some(Duration::from_secs(10))).unwrap();
    tcp.write_all(request).unwrap();
    let mut bytes = vec![];
    loop {
        let mut chunk = [0; 8192];
        let count = tcp
            .read(&mut chunk)
            .expect("HTTP response must arrive within the host deadline");
        assert_ne!(
            count, 0,
            "HTTP connection closed before a complete framed response"
        );
        bytes.extend_from_slice(&chunk[..count]);
        if let Some(end) = bytes.windows(4).position(|part| part == b"\r\n\r\n") {
            let header = std::str::from_utf8(&bytes[..end]).unwrap();
            let length: usize = header
                .split("\r\n")
                .find_map(|line| {
                    line.split_once(':')
                        .filter(|(name, _)| name.eq_ignore_ascii_case("content-length"))
                        .map(|(_, value)| value.trim().parse().unwrap())
                })
                .unwrap();
            assert!(length <= 16_000_000);
            if bytes.len() == end + 4 + length {
                break;
            }
            assert!(bytes.len() < end + 4 + length, "Extra response bytes");
        }
    }
    parse(&bytes)
}
fn frame(
    address: SocketAddr,
    token: u8,
    method: &str,
    session: Option<&str>,
    body: Option<Value>,
    extra: &str,
) -> Vec<u8> {
    let body = body
        .map(|value| serde_json::to_vec(&value).unwrap())
        .unwrap_or_default();
    let session = session
        .map(|session| format!("Mcp-Session-Id: {session}\r\nMcp-Protocol-Version: 2025-11-25\r\n"))
        .unwrap_or_default();
    let mut request=format!("{method} /mcp HTTP/1.1\r\nHost: {address}\r\nAuthorization: Bearer {}\r\nAccept: application/json, text/event-stream\r\nContent-Type: application/json\r\nContent-Length: {}\r\n{session}{extra}\r\n",hex::encode([token;32]),body.len()).into_bytes();
    request.extend_from_slice(&body);
    request
}
fn init() -> Value {
    json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"native-transport-test","version":"1"}}})
}
fn principal(root: &TempDir, id: &str, token: u8, profile: Profile) -> Principal {
    let host = HostProfile {
        modes: vec!["planning".into()],
        ..HostProfile::default()
    };
    Principal::new(
        id,
        Zeroizing::new(vec![token; 32]),
        Target::new(root.path(), profile, host),
    )
    .unwrap()
}
fn root() -> TempDir {
    let root = TempDir::new().unwrap();
    std::fs::write(root.path().join("main.rs"), "fn target() {}\n").unwrap();
    root
}

#[test]
fn real_tcp_session_preserves_profile_initialization_and_closes_without_sse() {
    let _gate = HTTP_TEST
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let root = root();
    let server = mcp_http::serve(
        Config::default(),
        vec![principal(&root, "reader", 11, Profile::Session)],
    )
    .unwrap();
    let address = server.local_addr();
    let response = exchange(address, &frame(address, 11, "POST", None, Some(init()), ""));
    assert_eq!(response.status, 200);
    let sid = &response.headers["mcp-session-id"];
    assert!(yoke::model::valid_id(sid));
    assert_eq!(
        serde_json::from_slice::<Value>(&response.body).unwrap()["result"]["protocolVersion"],
        "2025-11-25"
    );
    let notification = exchange(
        address,
        &frame(
            address,
            11,
            "POST",
            Some(sid),
            Some(json!({"jsonrpc":"2.0","method":"notifications/initialized"})),
            "",
        ),
    );
    assert_eq!(notification.status, 202);
    assert!(notification.body.is_empty());
    let tools = exchange(
        address,
        &frame(
            address,
            11,
            "POST",
            Some(sid),
            Some(json!({"jsonrpc":"2.0","id":2,"method":"tools/list"})),
            "",
        ),
    );
    assert_eq!(tools.status, 200);
    let value: Value = serde_json::from_slice(&tools.body).unwrap();
    assert!(
        value["result"]["tools"]
            .as_array()
            .unwrap()
            .iter()
            .any(|tool| tool["name"] == "ci_session")
    );
    assert_eq!(
        exchange(address, &frame(address, 11, "GET", Some(sid), None, "")).status,
        405
    );
    assert_eq!(
        exchange(address, &frame(address, 11, "DELETE", Some(sid), None, "")).status,
        204
    );
    assert_eq!(
        exchange(
            address,
            &frame(
                address,
                11,
                "POST",
                Some(sid),
                Some(json!({"jsonrpc":"2.0","id":3,"method":"ping"})),
                ""
            )
        )
        .status,
        404
    );
    server.shutdown().unwrap();
    assert!(TcpStream::connect(address).is_err());
}

#[test]
fn live_principal_rotation_and_host_profile_changes_invalidate_old_sessions() {
    let _gate = HTTP_TEST
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let root = root();
    let server = mcp_http::serve(
        Config::default(),
        vec![
            principal(&root, "one", 12, Profile::Native),
            principal(&root, "two", 13, Profile::Serena),
        ],
    )
    .unwrap();
    let address = server.local_addr();
    let initialized = exchange(address, &frame(address, 12, "POST", None, Some(init()), ""));
    let sid = &initialized.headers["mcp-session-id"];
    assert_eq!(
        exchange(
            address,
            &frame(
                address,
                13,
                "POST",
                Some(sid),
                Some(json!({"jsonrpc":"2.0","id":2,"method":"ping"})),
                ""
            )
        )
        .status,
        404
    );
    server
        .upsert(principal(&root, "one", 14, Profile::Graphify))
        .unwrap();
    assert_eq!(
        exchange(
            address,
            &frame(
                address,
                12,
                "POST",
                Some(sid),
                Some(json!({"jsonrpc":"2.0","id":2,"method":"ping"})),
                ""
            )
        )
        .status,
        401
    );
    assert_eq!(
        exchange(
            address,
            &frame(
                address,
                14,
                "POST",
                Some(sid),
                Some(json!({"jsonrpc":"2.0","id":2,"method":"ping"})),
                ""
            )
        )
        .status,
        404
    );
    let fresh = exchange(address, &frame(address, 14, "POST", None, Some(init()), ""));
    let sid = &fresh.headers["mcp-session-id"];
    let tools = exchange(
        address,
        &frame(
            address,
            14,
            "POST",
            Some(sid),
            Some(json!({"jsonrpc":"2.0","id":2,"method":"tools/list"})),
            "",
        ),
    );
    let value: Value = serde_json::from_slice(&tools.body).unwrap();
    assert!(
        value["result"]["tools"]
            .as_array()
            .unwrap()
            .iter()
            .any(|tool| tool["name"] == "query_graph")
    );
    assert!(server.revoke("one").unwrap());
    assert_eq!(
        exchange(address, &frame(address, 14, "GET", Some(sid), None, "")).status,
        401
    );
    server.shutdown().unwrap();
}

#[test]
fn hostile_http_framing_origin_host_versions_and_json_are_rejected_before_sessions() {
    let _gate = HTTP_TEST
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let root = root();
    let server = mcp_http::serve(
        Config::default(),
        vec![principal(&root, "reader", 15, Profile::Native)],
    )
    .unwrap();
    let address = server.local_addr();
    for (extra, status) in [
        ("Origin: https://attacker.invalid\r\n", 403),
        ("Transfer-Encoding: chunked\r\n", 400),
        ("Expect: 100-continue\r\n", 400),
        ("Mcp-Protocol-Version: future\r\n", 400),
        ("Content-Length: 1\r\n", 400),
    ] {
        assert_eq!(
            exchange(
                address,
                &frame(address, 15, "POST", None, Some(init()), extra)
            )
            .status,
            status
        );
    }
    let wrong_host = String::from_utf8(frame(address, 15, "POST", None, Some(init()), ""))
        .unwrap()
        .replace(&format!("Host: {address}"), "Host: attacker.invalid");
    assert_eq!(exchange(address, wrong_host.as_bytes()).status, 403);
    let bad_accept = String::from_utf8(frame(address, 15, "POST", None, Some(init()), ""))
        .unwrap()
        .replace(
            "Accept: application/json, text/event-stream",
            "Accept: application/json, text/event-stream;q=0",
        );
    assert_eq!(exchange(address, bad_accept.as_bytes()).status, 406);
    assert_eq!(
        exchange(
            address,
            &frame(address, 15, "POST", None, Some(json!([init(), init()])), "")
        )
        .status,
        400
    );
    let mut pipeline = frame(address, 15, "POST", None, Some(init()), "");
    pipeline.extend_from_slice(&frame(address, 15, "POST", None, Some(init()), ""));
    assert_eq!(exchange(address, &pipeline).status, 400);
    let lf = String::from_utf8(frame(address, 15, "POST", None, Some(init()), ""))
        .unwrap()
        .replace("\r\n", "\n");
    assert_eq!(exchange(address, lf.as_bytes()).status, 400);
    assert_eq!(server.session_count().unwrap(), 0);
    server.shutdown().unwrap();
}

#[test]
fn real_tls_requires_pinned_trust_and_remote_binding_requires_explicit_tls() {
    let _gate = HTTP_TEST
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let root = root();
    let certificate = rcgen::generate_simple_self_signed(vec!["localhost".into()]).unwrap();
    let der = certificate.cert.der().to_vec();
    let config = Config {
        tls: Some(TlsMaterial {
            certificates: vec![der.clone()],
            private_key: Zeroizing::new(certificate.key_pair.serialize_der()),
        }),
        server_names: vec!["localhost".into()],
        ..Config::default()
    };
    let server = mcp_http::serve(config, vec![repl_principal(&root, 16)]).unwrap();
    let address = server.local_addr();
    let mut roots = rustls::RootCertStore::empty();
    roots
        .add(rustls::pki_types::CertificateDer::from(der))
        .unwrap();
    let client = rustls::ClientConfig::builder_with_provider(Arc::new(
        rustls::crypto::ring::default_provider(),
    ))
    .with_protocol_versions(&[&rustls::version::TLS13])
    .unwrap()
    .with_root_certificates(roots)
    .with_no_client_auth();
    let connection = rustls::ClientConnection::new(
        Arc::new(client),
        rustls::pki_types::ServerName::try_from("localhost").unwrap(),
    )
    .unwrap();
    let tcp = TcpStream::connect(address).unwrap();
    tcp.set_read_timeout(Some(Duration::from_secs(3))).unwrap();
    let mut tls = rustls::StreamOwned::new(connection, tcp);
    tls.write_all(&frame(address, 16, "POST", None, Some(init()), ""))
        .unwrap();
    let mut bytes = vec![];
    let received = tls.read_to_end(&mut bytes);
    assert!(
        received.is_ok(),
        "TLS must send close_notify and EOF independently of retained Python: {received:?}; bytes={}",
        bytes.len()
    );
    assert_eq!(parse(&bytes).status, 200);
    for (trusted, name) in [(false, "localhost"), (true, "other.test")] {
        let mut roots = rustls::RootCertStore::empty();
        if trusted {
            roots.add(certificate.cert.der().clone()).unwrap();
        }
        let client = rustls::ClientConfig::builder_with_provider(Arc::new(
            rustls::crypto::ring::default_provider(),
        ))
        .with_protocol_versions(&[&rustls::version::TLS13])
        .unwrap()
        .with_root_certificates(roots)
        .with_no_client_auth();
        let connection = rustls::ClientConnection::new(
            Arc::new(client),
            rustls::pki_types::ServerName::try_from(name).unwrap(),
        )
        .unwrap();
        let tcp = TcpStream::connect(address).unwrap();
        tcp.set_read_timeout(Some(Duration::from_secs(3))).unwrap();
        let mut tls = rustls::StreamOwned::new(connection, tcp);
        let request = frame(address, 16, "POST", None, Some(init()), "");
        let result = tls.write_all(&request).and_then(|_| tls.read(&mut [0; 1]));
        assert!(
            result.is_err(),
            "Untrusted TLS or wrong hostname must fail handshake"
        );
    }
    let mut plain = TcpStream::connect(address).unwrap();
    plain
        .set_read_timeout(Some(Duration::from_secs(3)))
        .unwrap();
    plain
        .write_all(&frame(address, 16, "POST", None, Some(init()), ""))
        .unwrap();
    let mut bytes = vec![];
    let _ = plain.read_to_end(&mut bytes);
    assert!(!bytes.starts_with(b"HTTP/1.1 200"));
    assert_eq!(
        server.session_count().unwrap(),
        1,
        "Rejected TLS clients must not create sessions"
    );
    server.shutdown().unwrap();
    let remote = Config {
        bind: "0.0.0.0:0".parse().unwrap(),
        allow_remote_bind: true,
        ..Config::default()
    };
    assert!(
        mcp_http::serve(
            remote,
            vec![principal(&root, "reader", 16, Profile::Native)]
        )
        .is_err()
    );
}

#[test]
fn session_capacity_expiry_and_slow_connection_shutdown_are_bounded() {
    let _gate = HTTP_TEST
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let root = root();
    let config = Config {
        max_sessions: 1,
        max_sessions_per_principal: 1,
        session_ttl_ms: 100,
        ..Config::default()
    };
    let server = mcp_http::serve(
        config,
        vec![principal(&root, "reader", 17, Profile::Native)],
    )
    .unwrap();
    let address = server.local_addr();
    let response = exchange(address, &frame(address, 17, "POST", None, Some(init()), ""));
    let sid = &response.headers["mcp-session-id"];
    assert_eq!(
        exchange(address, &frame(address, 17, "POST", None, Some(init()), "")).status,
        429
    );
    std::thread::sleep(Duration::from_millis(150));
    assert_eq!(
        exchange(address, &frame(address, 17, "GET", Some(sid), None, "")).status,
        404
    );
    let mut slow = TcpStream::connect(address).unwrap();
    slow.write_all(b"POST /mcp HTTP/1.1\r\nHost: ").unwrap();
    let started = Instant::now();
    server.shutdown().unwrap();
    assert!(started.elapsed() < Duration::from_secs(3));
    assert!(TcpStream::connect(address).is_err());
}

#[test]
fn explicit_browser_origin_preflight_and_auth_errors_preserve_exact_cors_scope() {
    let _gate = HTTP_TEST
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let root = root();
    let config = Config {
        allowed_origins: vec!["https://client.test".into()],
        ..Config::default()
    };
    let server = mcp_http::serve(
        config,
        vec![principal(&root, "reader", 18, Profile::Native)],
    )
    .unwrap();
    let address = server.local_addr();
    let preflight=String::from_utf8(frame(address,18,"OPTIONS",None,None,"Origin: https://client.test\r\nAccess-Control-Request-Method: POST\r\nAccess-Control-Request-Headers: authorization,content-type,mcp-session-id\r\n")).unwrap().replace(&format!("Authorization: Bearer {}\r\n",hex::encode([18;32])),"");
    let response = exchange(address, preflight.as_bytes());
    assert_eq!(response.status, 204);
    assert_eq!(
        response.headers["access-control-allow-origin"],
        "https://client.test"
    );
    let denied = exchange(
        address,
        &frame(
            address,
            19,
            "POST",
            None,
            Some(init()),
            "Origin: https://client.test\r\n",
        ),
    );
    assert_eq!(denied.status, 401);
    assert_eq!(
        denied
            .headers
            .get("access-control-allow-origin")
            .map(String::as_str),
        Some("https://client.test"),
        "Authorized browser origins must be able to observe authentication failure"
    );
    let hostile = exchange(
        address,
        &frame(
            address,
            18,
            "POST",
            None,
            Some(init()),
            "Origin: https://attacker.test\r\n",
        ),
    );
    assert_eq!(hostile.status, 403);
    assert!(!hostile.headers.contains_key("access-control-allow-origin"));
    server.shutdown().unwrap();
}

fn stalled_principal(root: &TempDir, marker: &std::path::Path) -> Principal {
    let mut target = Target::new(root.path(), Profile::Native, HostProfile::default());
    target.servers.push(yoke::lsp::ConfiguredServer {
        server: yoke::lsp::Server {
            command: "node".into(),
            args: vec![
                std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("tests/fixtures/mcp-cancel-lsp.mjs")
                    .display()
                    .to_string(),
                marker.display().to_string(),
            ],
            language: "rust".into(),
            trust: true,
        },
        configuration: yoke::lsp::HostConfiguration {
            passive_profile: Some(yoke::lsp::PassiveProfile::ContractFixture),
            ..Default::default()
        },
    });
    Principal::new("reader", Zeroizing::new(vec![21; 32]), target).unwrap()
}
fn stalled_request(address: SocketAddr, sid: &str) -> Vec<u8> {
    frame(
        address,
        21,
        "POST",
        Some(sid),
        Some(
            json!({"jsonrpc":"2.0","id":41,"method":"tools/call","params":{"name":"ci_find","arguments":{"languageServer":{"path":"main.rs","offset":3,"operation":"hover"},"timeoutMs":600000}}}),
        ),
        "",
    )
}
fn wait_for(mut check: impl FnMut() -> bool) {
    let end = Instant::now() + Duration::from_secs(5);
    while !check() {
        assert!(
            Instant::now() < end,
            "Fixture did not reach running operation"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn running_query_obeys_host_deadline_despite_client_timeout_and_returns_http_error() {
    let _gate = HTTP_TEST
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let root = root();
    let marker = root.path().join("hover.log");
    let server = mcp_http::serve(
        Config {
            request_timeout_ms: 1800,
            ..Config::default()
        },
        vec![stalled_principal(&root, &marker)],
    )
    .unwrap();
    let address = server.local_addr();
    let initialized = exchange(address, &frame(address, 21, "POST", None, Some(init()), ""));
    let sid = &initialized.headers["mcp-session-id"];
    let started = Instant::now();
    let result = exchange(address, &stalled_request(address, sid));
    assert!(
        marker.exists(),
        "Actual provider operation must run before timeout"
    );
    assert_eq!(result.status, 503);
    assert!(started.elapsed() < Duration::from_secs(4));
    server.shutdown().unwrap();
}

#[test]
fn network_cancellation_interrupts_running_provider_and_shutdown_cleans_workers() {
    let _gate = HTTP_TEST
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let root = root();
    let marker = root.path().join("hover.log");
    let server =
        mcp_http::serve(Config::default(), vec![stalled_principal(&root, &marker)]).unwrap();
    let address = server.local_addr();
    let initialized = exchange(address, &frame(address, 21, "POST", None, Some(init()), ""));
    let sid = initialized.headers["mcp-session-id"].clone();
    let request = stalled_request(address, &sid);
    let running = std::thread::spawn(move || exchange(address, &request));
    wait_for(|| marker.exists());
    let started = Instant::now();
    let cancellation = exchange(
        address,
        &frame(
            address,
            21,
            "POST",
            Some(&sid),
            Some(
                json!({"jsonrpc":"2.0","method":"notifications/cancelled","params":{"requestId":41,"reason":"test"}}),
            ),
            "",
        ),
    );
    assert_eq!(cancellation.status, 202);
    assert_eq!(running.join().unwrap().status, 503);
    assert!(started.elapsed() < Duration::from_secs(3));
    std::fs::remove_file(&marker).unwrap();
    let request = stalled_request(address, &sid);
    let running = std::thread::spawn(move || {
        let mut tcp = TcpStream::connect(address).unwrap();
        tcp.write_all(&request).unwrap();
        let _ = tcp.read_to_end(&mut vec![]);
    });
    wait_for(|| marker.exists());
    let started = Instant::now();
    server.shutdown().unwrap();
    running.join().unwrap();
    assert!(started.elapsed() < Duration::from_secs(3));
}

#[test]
fn queued_request_is_rechecked_after_current_principal_revocation() {
    let _gate = HTTP_TEST
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let root = root();
    let marker = root.path().join("hover.log");
    let server = mcp_http::serve(
        Config {
            workers: 1,
            request_timeout_ms: 2000,
            ..Config::default()
        },
        vec![
            stalled_principal(&root, &marker),
            principal(&root, "queued", 22, Profile::Native),
        ],
    )
    .unwrap();
    let address = server.local_addr();
    let a = exchange(address, &frame(address, 21, "POST", None, Some(init()), ""));
    let a_sid = &a.headers["mcp-session-id"];
    let b = exchange(address, &frame(address, 22, "POST", None, Some(init()), ""));
    let b_sid = &b.headers["mcp-session-id"];
    let request = stalled_request(address, a_sid);
    let running = std::thread::spawn(move || exchange(address, &request));
    wait_for(|| marker.exists());
    let request = frame(
        address,
        22,
        "POST",
        Some(b_sid),
        Some(json!({"jsonrpc":"2.0","id":55,"method":"ping"})),
        "",
    );
    let queued = std::thread::spawn(move || exchange(address, &request));
    let control = server.controller();
    wait_for(|| control.pending_requests().unwrap() == 2);
    let revoked = std::thread::spawn(move || control.revoke("queued").unwrap());
    assert_eq!(running.join().unwrap().status, 503);
    assert!(revoked.join().unwrap());
    assert_eq!(queued.join().unwrap().status, 503);
    assert_eq!(
        exchange(address, &frame(address, 22, "POST", None, Some(init()), "")).status,
        401
    );
    server.shutdown().unwrap();
}

#[test]
fn admission_caps_reject_declared_bodies_headers_deep_json_and_session_version_changes() {
    let _gate = HTTP_TEST
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let root = root();
    let server = mcp_http::serve(
        Config {
            max_body_bytes: 4096,
            ..Config::default()
        },
        vec![principal(&root, "reader", 23, Profile::Native)],
    )
    .unwrap();
    let address = server.local_addr();
    let base = String::from_utf8(frame(address, 23, "POST", None, Some(init()), "")).unwrap();
    let oversized = base.replace(
        &format!(
            "Content-Length: {}",
            serde_json::to_vec(&init()).unwrap().len()
        ),
        "Content-Length: 4097",
    );
    assert_eq!(exchange(address, oversized.as_bytes()).status, 413);
    let many = (0..50)
        .map(|i| format!("X-Field-{i}: x\r\n"))
        .collect::<String>();
    assert_eq!(
        exchange(
            address,
            &frame(address, 23, "POST", None, Some(init()), &many)
        )
        .status,
        400
    );
    let huge = format!("X-Large: {}\r\n", "x".repeat(17000));
    assert_eq!(
        exchange(
            address,
            &frame(address, 23, "POST", None, Some(init()), &huge)
        )
        .status,
        431
    );
    let mut nested = json!({});
    for _ in 0..70 {
        nested = json!({"nested":nested});
    }
    assert_eq!(
        exchange(
            address,
            &frame(
                address,
                23,
                "POST",
                None,
                Some(json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":nested})),
                ""
            )
        )
        .status,
        400
    );
    assert_eq!(server.session_count().unwrap(), 0);
    let initialized = exchange(address, &frame(address, 23, "POST", None, Some(init()), ""));
    let sid = &initialized.headers["mcp-session-id"];
    let wrong = String::from_utf8(frame(
        address,
        23,
        "POST",
        Some(sid),
        Some(json!({"jsonrpc":"2.0","id":2,"method":"ping"})),
        "",
    ))
    .unwrap()
    .replace(
        "Mcp-Protocol-Version: 2025-11-25",
        "Mcp-Protocol-Version: 2025-06-18",
    );
    assert_eq!(exchange(address, wrong.as_bytes()).status, 400);
    let absent = String::from_utf8(frame(
        address,
        23,
        "POST",
        Some(sid),
        Some(json!({"jsonrpc":"2.0","id":2,"method":"ping"})),
        "",
    ))
    .unwrap()
    .replace("Mcp-Protocol-Version: 2025-11-25\r\n", "");
    assert_eq!(exchange(address, absent.as_bytes()).status, 200);
    server.shutdown().unwrap();
}

fn python_spec() -> yoke::agent_repl::ReplSpec {
    let output = std::process::Command::new("python")
        .args(["-I", "-c", "import sys;print(sys.executable)"])
        .output()
        .unwrap();
    assert!(output.status.success());
    yoke::agent_repl::ReplSpec {
        python: String::from_utf8(output.stdout).unwrap().trim().into(),
        trust_python: true,
        max_output_bytes: 100_000,
        shell: None,
    }
}
fn repl_principal(root: &TempDir, token: u8) -> Principal {
    let mut target = Target::new(root.path(), Profile::Serena, HostProfile::default());
    target.agent_repl = Some(python_spec());
    Principal::new("repl", Zeroizing::new(vec![token; 32]), target).unwrap()
}
fn public_tool(
    address: SocketAddr,
    token: u8,
    sid: &str,
    name: &str,
    arguments: Value,
) -> (bool, Value) {
    let response = exchange(
        address,
        &frame(
            address,
            token,
            "POST",
            Some(sid),
            Some(
                json!({"jsonrpc":"2.0","id":9,"method":"tools/call","params":{"name":name,"arguments":arguments}}),
            ),
            "",
        ),
    );
    assert_eq!(response.status, 200);
    let value: Value = serde_json::from_slice(&response.body).unwrap();
    let envelope =
        serde_json::from_str(value["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
    (value["result"]["isError"] == true, envelope)
}

#[test]
fn explicit_http_serena_runtime_persists_per_sid_and_rejects_foreign_session_labels() {
    let _gate = HTTP_TEST
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let root = root();
    let server = mcp_http::serve(Config::default(), vec![repl_principal(&root, 24)]).unwrap();
    let address = server.local_addr();
    let initialized = exchange(address, &frame(address, 24, "POST", None, Some(init()), ""));
    let sid = &initialized.headers["mcp-session-id"];
    let (_, config) = public_tool(address, 24, sid, "get_current_config", json!({}));
    let label = config["data"]["sessionId"].as_str().unwrap();
    let (error, first) = public_tool(
        address,
        24,
        sid,
        "serena_repl",
        json!({"session_id":label,"code":"import os\ncounter=41\n(counter,os.getpid(),s.cfg.get_current_config()['sessionId'])"}),
    );
    assert!(!error, "{first}");
    assert_eq!(first["data"]["result"][0], 41);
    assert_eq!(first["data"]["result"][2], label);
    let (_, second) = public_tool(
        address,
        24,
        sid,
        "serena_repl",
        json!({"session_id":label,"code":"counter+=1\ncounter"}),
    );
    assert_eq!(second["data"]["result"], 42);
    let initialized = exchange(address, &frame(address, 24, "POST", None, Some(init()), ""));
    let other_sid = &initialized.headers["mcp-session-id"];
    let (_, config) = public_tool(address, 24, other_sid, "get_current_config", json!({}));
    let other_label = config["data"]["sessionId"].as_str().unwrap();
    assert_ne!(label, other_label);
    let (_, isolated) = public_tool(
        address,
        24,
        other_sid,
        "serena_repl",
        json!({"session_id":other_label,"code":"globals().get('counter','missing')"}),
    );
    assert_eq!(isolated["data"]["result"], "missing");
    let (denied, _) = public_tool(
        address,
        24,
        other_sid,
        "serena_repl",
        json!({"session_id":label,"code":"counter=99"}),
    );
    assert!(denied);
    server.shutdown().unwrap();
}

#[cfg(windows)]
fn process_exited(pid: u32) -> bool {
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn OpenProcess(access: u32, inherit: i32, id: u32) -> *mut std::ffi::c_void;
        fn WaitForSingleObject(handle: *mut std::ffi::c_void, milliseconds: u32) -> u32;
        fn CloseHandle(handle: *mut std::ffi::c_void) -> i32;
    }
    unsafe {
        let handle = OpenProcess(0x0010_0000, 0, pid);
        if handle.is_null() {
            return true;
        }
        let exited = WaitForSingleObject(handle, 0) == 0;
        CloseHandle(handle);
        exited
    }
}
#[cfg(windows)]
fn runtime_session(address: SocketAddr, token: u8) -> (String, String, u32) {
    let initialized = exchange(
        address,
        &frame(address, token, "POST", None, Some(init()), ""),
    );
    let sid = initialized.headers["mcp-session-id"].clone();
    let (_, config) = public_tool(address, token, &sid, "get_current_config", json!({}));
    let label = config["data"]["sessionId"].as_str().unwrap().to_string();
    let (error, value) = public_tool(
        address,
        token,
        &sid,
        "serena_repl",
        json!({"session_id":label,"code":"__import__('os').getpid()"}),
    );
    assert!(!error, "{value}");
    (
        sid,
        label,
        value["data"]["result"]
            .as_u64()
            .unwrap()
            .try_into()
            .unwrap(),
    )
}

#[test]
#[cfg(windows)]
fn http_python_cancellation_invalidates_lost_namespace_and_proves_process_exit() {
    let _gate = HTTP_TEST
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let root = root();
    let marker = root.path().join("python.pid");
    let server = mcp_http::serve(
        Config {
            request_timeout_ms: 5000,
            ..Config::default()
        },
        vec![repl_principal(&root, 25)],
    )
    .unwrap();
    let address = server.local_addr();
    let (sid, label, pid) = runtime_session(address, 25);
    assert!(!process_exited(pid));
    let code = format!(
        "import os,time\nwith open({},'w') as output:\n output.write(str(os.getpid()))\ntime.sleep(30)",
        serde_json::to_string(&marker.to_string_lossy()).unwrap()
    );
    let request = frame(
        address,
        25,
        "POST",
        Some(&sid),
        Some(
            json!({"jsonrpc":"2.0","id":70,"method":"tools/call","params":{"name":"serena_repl","arguments":{"session_id":label,"code":code}}}),
        ),
        "",
    );
    let running = std::thread::spawn(move || exchange(address, &request));
    wait_for(|| {
        std::fs::read_to_string(&marker)
            .ok()
            .and_then(|text| text.parse::<u32>().ok())
            == Some(pid)
    });
    let started = Instant::now();
    assert_eq!(exchange(address,&frame(address,25,"POST",Some(&sid),Some(json!({"jsonrpc":"2.0","method":"notifications/cancelled","params":{"requestId":70}})),"")).status,202);
    assert_eq!(running.join().unwrap().status, 503);
    wait_for(|| process_exited(pid));
    assert!(started.elapsed() < Duration::from_secs(3));
    assert_eq!(
        exchange(address, &frame(address, 25, "GET", Some(&sid), None, "")).status,
        404
    );
    server.shutdown().unwrap();
}

#[test]
#[cfg(windows)]
fn delete_rotation_revocation_and_shutdown_prove_each_owned_python_exit() {
    let _gate = HTTP_TEST
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let root = root();
    let server = mcp_http::serve(Config::default(), vec![repl_principal(&root, 26)]).unwrap();
    let address = server.local_addr();
    let (sid, _, pid) = runtime_session(address, 26);
    assert_eq!(
        exchange(address, &frame(address, 26, "DELETE", Some(&sid), None, "")).status,
        204
    );
    wait_for(|| process_exited(pid));
    let (old_sid, _, old_pid) = runtime_session(address, 26);
    server.upsert(repl_principal(&root, 27)).unwrap();
    wait_for(|| process_exited(old_pid));
    assert_eq!(
        exchange(
            address,
            &frame(address, 26, "GET", Some(&old_sid), None, "")
        )
        .status,
        401
    );
    let (_, _, pid) = runtime_session(address, 27);
    assert!(server.revoke("repl").unwrap());
    wait_for(|| process_exited(pid));
    server.upsert(repl_principal(&root, 28)).unwrap();
    let (_, _, pid) = runtime_session(address, 28);
    server.shutdown().unwrap();
    assert!(process_exited(pid));
}

#[test]
fn runtime_host_preflight_never_executes_and_other_profiles_cannot_attach_python() {
    let _gate = HTTP_TEST
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let root = root();
    let executable = root.path().join("not-python.exe");
    std::fs::write(
        &executable,
        b"Owned deliberately non-executable preflight fixture",
    )
    .unwrap();
    let selected = yoke::agent_repl::ReplSpec {
        python: executable,
        trust_python: true,
        max_output_bytes: 1000,
        shell: None,
    };
    let mut target = Target::new(root.path(), Profile::Native, HostProfile::default());
    target.agent_repl = Some(selected.clone());
    assert!(Principal::new("invalid", Zeroizing::new(vec![29; 32]), target).is_err());
    let mut target = Target::new(root.path(), Profile::Serena, HostProfile::default());
    target.agent_repl = Some(selected);
    let principal = Principal::new("preflight", Zeroizing::new(vec![29; 32]), target)
        .expect("Pure preflight must not try to execute selected bytes");
    let server = mcp_http::serve(Config::default(), vec![principal]).unwrap();
    let address = server.local_addr();
    assert_eq!(
        exchange(address, &frame(address, 29, "POST", None, Some(init()), "")).status,
        503
    );
    assert_eq!(server.session_count().unwrap(), 0);
    server.shutdown().unwrap();
}

#[cfg(windows)]
struct StalledInitialization {
    marker: std::path::PathBuf,
    launcher: std::path::PathBuf,
    shim: std::path::PathBuf,
}
#[cfg(windows)]
impl StalledInitialization {
    fn new(root: &TempDir) -> Self {
        let marker = root.path().join("handshake.pid");
        let launcher = root.path().join("launcher.started");
        let worker = root.path().join("stalled.py");
        std::fs::write(&worker,format!("import os,time\nwith open({},'w') as output:\n output.write(str(os.getpid()))\ntime.sleep(30)\n",serde_json::to_string(&marker.to_string_lossy()).unwrap())).unwrap();
        let python = python_spec().python;
        let shim = root.path().join("host-stalled.cmd");
        std::fs::write(
            &shim,
            format!(
                "@echo off\r\n> \"{}\" echo started\r\n\"{}\" -I \"{}\"\r\n",
                launcher.display().to_string().replace('%', "%%"),
                python.display().to_string().replace('%', "%%"),
                worker.display().to_string().replace('%', "%%"),
            ),
        )
        .unwrap();
        Self {
            marker,
            launcher,
            shim,
        }
    }
    fn principal(&self, root: &TempDir, token: u8) -> Principal {
        let mut target = Target::new(root.path(), Profile::Serena, HostProfile::default());
        target.agent_repl = Some(yoke::agent_repl::ReplSpec {
            python: self.shim.clone(),
            trust_python: true,
            max_output_bytes: 1000,
            shell: None,
        });
        Principal::new("stalled", Zeroizing::new(vec![token; 32]), target).unwrap()
    }
    fn pid(&self) -> Option<u32> {
        std::fs::read_to_string(&self.marker).ok()?.parse().ok()
    }
    fn progress(&self) -> String {
        format!(
            "launcherStarted={}, pythonPid={:?}",
            self.launcher.exists(),
            self.pid()
        )
    }
    fn exit_before(&self, observed_pid: Option<u32>, end: Instant) -> Option<(u32, bool)> {
        let pid = observed_pid.or_else(|| self.pid())?;
        loop {
            if Instant::now() >= end {
                return Some((pid, false));
            }
            if process_exited(pid) {
                return Some((pid, Instant::now() < end));
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }
    fn assert_worker_exited(&self) {
        if let Some(pid) = self.pid() {
            assert!(
                process_exited(pid),
                "Owned stalled Python {pid} survived shutdown: {}",
                self.progress()
            );
        }
    }
}

#[cfg(windows)]
struct InitializationResponse {
    response: Response,
    elapsed: Duration,
}
#[cfg(windows)]
impl InitializationResponse {
    fn diagnostic(&self) -> String {
        format!(
            "status={}, elapsed={:?}, body={}",
            self.response.status,
            self.elapsed,
            String::from_utf8_lossy(&self.response.body)
        )
    }
}

#[cfg(windows)]
struct PendingInitialization {
    started: Instant,
    replies: std::sync::mpsc::Receiver<InitializationResponse>,
    completed: Option<InitializationResponse>,
    worker: Option<std::thread::JoinHandle<()>>,
}
#[cfg(windows)]
impl PendingInitialization {
    fn new(address: SocketAddr, token: u8) -> Self {
        let request = frame(address, token, "POST", None, Some(init()), "");
        let (sender, replies) = std::sync::mpsc::sync_channel(1);
        let started = Instant::now();
        let worker = std::thread::spawn(move || {
            let response = exchange(address, &request);
            let _ = sender.send(InitializationResponse {
                response,
                elapsed: started.elapsed(),
            });
        });
        Self {
            started,
            replies,
            completed: None,
            worker: Some(worker),
        }
    }
    fn observe_start(&mut self, fixture: &StalledInitialization) -> Option<u32> {
        let observation_end = self.started + Duration::from_secs(5);
        loop {
            if let Some(pid) = fixture.pid() {
                return Some(pid);
            }
            match self.replies.try_recv() {
                Ok(reply) => {
                    eprintln!(
                        "Initialization completed before Python PID: {}; {}",
                        reply.diagnostic(),
                        fixture.progress()
                    );
                    self.completed = Some(reply);
                    return None;
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    panic!(
                        "Initialization request ended without a response after {:?}; {}",
                        self.started.elapsed(),
                        fixture.progress()
                    );
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {}
            }
            assert!(
                Instant::now() < observation_end,
                "Initialization produced neither a response nor a Python PID after {:?}; {}",
                self.started.elapsed(),
                fixture.progress()
            );
            std::thread::sleep(Duration::from_millis(10));
        }
    }
    fn finish(mut self, fixture: &StalledInitialization) -> InitializationResponse {
        let response = self.completed.take().unwrap_or_else(|| {
            self.replies
                .recv_timeout(Duration::from_secs(9))
                .unwrap_or_else(|error| {
                    panic!(
                        "Initialization response unavailable ({error}) after {:?}; {}",
                        self.started.elapsed(),
                        fixture.progress()
                    );
                })
        });
        self.worker
            .take()
            .unwrap()
            .join()
            .expect("Initialization request thread failed");
        response
    }
}
#[cfg(windows)]
impl Drop for PendingInitialization {
    fn drop(&mut self) {
        // Join on assertion failure too; exchange has its own bounded socket read.
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

#[test]
#[cfg(windows)]
fn stalled_runtime_initialization_obeys_ttl_and_shutdown_cancellation() {
    let _gate = HTTP_TEST
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let root = root();
    let fixture = StalledInitialization::new(&root);
    let ttl = Duration::from_millis(500);
    let server = mcp_http::serve(
        Config {
            request_timeout_ms: 4000,
            session_ttl_ms: 500,
            ..Config::default()
        },
        vec![fixture.principal(&root, 30)],
    )
    .unwrap();
    let mut pending = PendingInitialization::new(server.local_addr(), 30);
    let admitted_start = pending.started;
    // Expiry can legitimately win before SQLite, the launcher or Python startup.
    // Observe an early response instead of waiting for an impossible PID marker.
    let observed_pid = pending.observe_start(&fixture);
    let reply = pending.finish(&fixture);
    let exited_before_shutdown =
        fixture.exit_before(observed_pid, admitted_start + ttl + Duration::from_secs(2));
    let sessions = server.session_count();
    let stopping = Instant::now();
    let shutdown = server.shutdown();
    let shutdown_elapsed = stopping.elapsed();
    let diagnostic = format!(
        "{}; observedPid={observed_pid:?}, exitedBeforeShutdown={exited_before_shutdown:?}; {}",
        reply.diagnostic(),
        fixture.progress()
    );
    assert!(
        shutdown.is_ok(),
        "Shutdown failed: {shutdown:?}; {diagnostic}"
    );
    assert_eq!(reply.response.status, 503, "{diagnostic}");
    assert!(
        reply.elapsed >= ttl,
        "Initialization failed before the admitted TTL: {diagnostic}"
    );
    assert!(
        reply.elapsed < ttl + Duration::from_secs(2),
        "Session expiry was not bounded independently of the 4000ms request timeout: {diagnostic}"
    );
    assert_eq!(
        sessions.unwrap(),
        0,
        "Expired initialization retained a session: {diagnostic}"
    );
    assert!(
        shutdown_elapsed < Duration::from_secs(2),
        "Initialization shutdown took {shutdown_elapsed:?}: {diagnostic}"
    );
    assert!(
        exited_before_shutdown.is_none_or(|(_, exited)| exited),
        "Observed runtime survived TTL cancellation until shutdown: {diagnostic}"
    );
    fixture.assert_worker_exited();
}

#[test]
#[cfg(windows)]
fn already_started_runtime_initialization_expires_at_the_admitted_session_ttl() {
    let _gate = HTTP_TEST
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let root = root();
    let fixture = StalledInitialization::new(&root);
    // Existing revoke/stop fixtures admit 4000ms for this same launcher. Give
    // startup that allowance here; keep the independent request timeout longer
    // so losing session-TTL enforcement cannot pass via the request deadline.
    let ttl = Duration::from_millis(4000);
    let server = mcp_http::serve(
        Config {
            request_timeout_ms: 8000,
            session_ttl_ms: 4000,
            ..Config::default()
        },
        vec![fixture.principal(&root, 32)],
    )
    .unwrap();
    let mut pending = PendingInitialization::new(server.local_addr(), 32);
    let admitted_start = pending.started;
    let observed_pid = pending.observe_start(&fixture);
    let running_when_observed = observed_pid.is_some_and(|pid| !process_exited(pid));
    let startup_elapsed = pending.started.elapsed();
    let reply = pending.finish(&fixture);
    let exited_before_shutdown =
        fixture.exit_before(observed_pid, admitted_start + ttl + Duration::from_secs(2));
    let sessions = server.session_count();
    let stopping = Instant::now();
    let shutdown = server.shutdown();
    let shutdown_elapsed = stopping.elapsed();
    let diagnostic = format!(
        "{}; startup={startup_elapsed:?}, observedPid={observed_pid:?}, exitedBeforeShutdown={exited_before_shutdown:?}; {}",
        reply.diagnostic(),
        fixture.progress()
    );
    assert!(
        shutdown.is_ok(),
        "Shutdown failed: {shutdown:?}; {diagnostic}"
    );
    assert!(
        running_when_observed,
        "Fixture did not observe a live stalled runtime: {diagnostic}"
    );
    assert_eq!(reply.response.status, 503, "{diagnostic}");
    // Count startup in the same admitted lifetime; never restart TTL at the PID.
    assert!(
        reply.elapsed >= ttl,
        "Started initialization failed before the admitted TTL: {diagnostic}"
    );
    assert!(
        reply.elapsed < ttl + Duration::from_secs(2),
        "Started runtime ignored session expiry and reached the 8000ms request timeout: {diagnostic}"
    );
    assert_eq!(
        sessions.unwrap(),
        0,
        "Expired initialization retained a session: {diagnostic}"
    );
    assert!(
        shutdown_elapsed < Duration::from_secs(2),
        "Started runtime shutdown took {shutdown_elapsed:?}: {diagnostic}"
    );
    assert!(
        matches!(exited_before_shutdown, Some((_, true))),
        "Started runtime survived TTL cancellation until shutdown: {diagnostic}"
    );
    fixture.assert_worker_exited();
}

#[test]
#[cfg(windows)]
fn revoke_and_stop_cancel_runtime_during_initial_handshake() {
    let _gate = HTTP_TEST
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    for action in ["revoke", "stop"] {
        let root = root();
        let marker = root.path().join("handshake.pid");
        let worker = root.path().join("stalled.py");
        std::fs::write(&worker,format!("import os,time\nwith open({},'w') as output:\n output.write(str(os.getpid()))\ntime.sleep(30)\n",serde_json::to_string(&marker.to_string_lossy()).unwrap())).unwrap();
        let python = python_spec().python;
        let shim = root.path().join("host-stalled.cmd");
        std::fs::write(
            &shim,
            format!(
                "@echo off\r\n\"{}\" -I \"{}\"\r\n",
                python.display(),
                worker.display()
            ),
        )
        .unwrap();
        let mut target = Target::new(root.path(), Profile::Serena, HostProfile::default());
        target.agent_repl = Some(yoke::agent_repl::ReplSpec {
            python: shim,
            trust_python: true,
            max_output_bytes: 1000,
            shell: None,
        });
        let principal = Principal::new("stalled", Zeroizing::new(vec![31; 32]), target).unwrap();
        let mut server = Some(
            mcp_http::serve(
                Config {
                    request_timeout_ms: 4000,
                    ..Config::default()
                },
                vec![principal],
            )
            .unwrap(),
        );
        let address = server.as_ref().unwrap().local_addr();
        let request = frame(address, 31, "POST", None, Some(init()), "");
        let running = std::thread::spawn(move || {
            if action == "stop" {
                let mut tcp = TcpStream::connect(address).unwrap();
                tcp.write_all(&request).unwrap();
                let _ = tcp.read_to_end(&mut vec![]);
                None
            } else {
                Some(exchange(address, &request))
            }
        });
        wait_for(|| {
            std::fs::read_to_string(&marker)
                .ok()
                .and_then(|text| text.parse::<u32>().ok())
                .is_some()
        });
        let pid = std::fs::read_to_string(&marker).unwrap().parse().unwrap();
        let started = Instant::now();
        if action == "revoke" {
            assert!(server.as_ref().unwrap().revoke("stalled").unwrap());
        } else {
            server.take().unwrap().shutdown().unwrap();
        }
        let response = running.join().unwrap();
        if let Some(response) = response {
            assert_eq!(response.status, 503);
        }
        assert!(
            started.elapsed() < Duration::from_secs(2),
            "{action} ignored initialization cancellation"
        );
        wait_for(|| process_exited(pid));
        if let Some(server) = server {
            server.shutdown().unwrap();
        }
    }
}

#[test]
fn connection_close_reaches_eof_while_initialized_python_runtime_is_still_live() {
    let _gate = HTTP_TEST
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let root = root();
    let server = mcp_http::serve(Config::default(), vec![repl_principal(&root, 32)]).unwrap();
    let address = server.local_addr();
    let mut socket = TcpStream::connect(address).unwrap();
    socket
        .set_read_timeout(Some(Duration::from_secs(3)))
        .unwrap();
    socket
        .write_all(&frame(address, 32, "POST", None, Some(init()), ""))
        .unwrap();
    let mut bytes = Vec::new();
    let read = socket.read_to_end(&mut bytes);
    assert!(
        read.is_ok(),
        "Connection: close must deliver EOF independently of retained Python: {read:?}; bytes={}",
        bytes.len()
    );
    assert_eq!(parse(&bytes).status, 200);
    assert_eq!(server.session_count().unwrap(), 1);
    server.shutdown().unwrap();
}
