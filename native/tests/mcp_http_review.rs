use serde_json::{Value, json};
use std::{
    io::{Read, Write},
    net::{SocketAddr, TcpStream},
    time::{Duration, Instant},
};
use yoke::{
    mcp_http::{self, Config, Principal, Target},
    project_session::HostProfile,
    protocol::Profile,
};
use zeroize::Zeroizing;

fn frame(address: SocketAddr, sid: Option<&str>, value: Value) -> Vec<u8> {
    let body = serde_json::to_vec(&value).unwrap();
    let session = sid
        .map(|sid| format!("Mcp-Session-Id: {sid}\r\nMcp-Protocol-Version: 2025-11-25\r\n"))
        .unwrap_or_default();
    let mut bytes=format!("POST /mcp HTTP/1.1\r\nHost: {address}\r\nAuthorization: Bearer {}\r\nAccept: application/json, text/event-stream\r\nContent-Type: application/json\r\nContent-Length: {}\r\n{session}\r\n",hex::encode([47;32]),body.len()).into_bytes();
    bytes.extend(body);
    bytes
}
fn exchange(address: SocketAddr, bytes: &[u8]) -> (u16, String, Option<i64>) {
    let mut socket = TcpStream::connect(address).unwrap();
    socket
        .set_read_timeout(Some(Duration::from_secs(8)))
        .unwrap();
    socket.write_all(bytes).unwrap();
    let mut reply = Vec::new();
    loop {
        let mut chunk = [0; 8192];
        let count = socket.read(&mut chunk).unwrap();
        assert_ne!(count, 0, "HTTP peer closed without complete response");
        reply.extend_from_slice(&chunk[..count]);
        if let Some(end) = reply.windows(4).position(|p| p == b"\r\n\r\n") {
            let header = std::str::from_utf8(&reply[..end]).unwrap();
            let length: usize = header
                .lines()
                .find_map(|line| {
                    line.split_once(':')
                        .filter(|(key, _)| key.eq_ignore_ascii_case("content-length"))
                        .map(|(_, v)| v.trim().parse().unwrap())
                })
                .unwrap();
            assert!(length < 16_000_000);
            if reply.len() == end + 4 + length {
                // Scalar diagnostics only; never include response text or tokens.
                let rpc_error_code = serde_json::from_slice::<Value>(&reply[end + 4..])
                    .ok()
                    .and_then(|value| value["error"]["code"].as_i64());
                return (
                    header.split_whitespace().nth(1).unwrap().parse().unwrap(),
                    header.to_owned(),
                    rpc_error_code,
                );
            }
            assert!(reply.len() < end + 4 + length);
        }
    }
}

#[test]
fn single_worker_session_expiry_cancels_an_already_running_provider() {
    // Match the startup allowance of the already-started runtime fixture in
    // mcp_http.rs. The session lifetime still begins at initialize, and its
    // expiry must win well before the independent request deadline.
    session_expiry(4000, 8000, false);
}

#[test]
fn single_worker_short_session_expiry_bounds_provider_startup() {
    // Keep the short lifetime covered without claiming that startup must finish
    // inside it. This peer never completes its initialization handshake.
    session_expiry(1200, 5000, true);
}

fn session_expiry(session_ttl_ms: u64, request_timeout_ms: u64, stall_initialize: bool) {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("main.rs"), "fn target(){}\n").unwrap();
    let marker = root.path().join("hover.log");
    let mut target = Target::new(root.path(), Profile::Native, HostProfile::default());
    let mut args = vec![
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/mcp-cancel-lsp.mjs")
            .display()
            .to_string(),
        marker.display().to_string(),
    ];
    if stall_initialize {
        args.push("--stall-initialize".into());
    }
    target.servers.push(yoke::lsp::ConfiguredServer {
        server: yoke::lsp::Server {
            language: "rust".into(),
            command: "node".into(),
            args,
            trust: true,
        },
        configuration: yoke::lsp::HostConfiguration {
            passive_profile: Some(yoke::lsp::PassiveProfile::ContractFixture),
            ..Default::default()
        },
    });
    let principal = Principal::new("ttl-review", Zeroizing::new(vec![47; 32]), target).unwrap();
    let server = mcp_http::serve(
        Config {
            workers: 1,
            session_ttl_ms,
            request_timeout_ms,
            ..Default::default()
        },
        vec![principal],
    )
    .unwrap();
    let address = server.local_addr();
    let initializing = Instant::now();
    let (status, header, _) = exchange(
        address,
        &frame(
            address,
            None,
            json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25"}}),
        ),
    );
    let initialization_elapsed = initializing.elapsed();
    assert_eq!(status, 200);
    let sid = header
        .lines()
        .find_map(|line| {
            line.split_once(':')
                .filter(|(key, _)| key.eq_ignore_ascii_case("mcp-session-id"))
                .map(|(_, v)| v.trim())
        })
        .unwrap();
    let started = Instant::now();
    let (status, _, rpc_error_code) = exchange(
        address,
        &frame(
            address,
            Some(sid),
            json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"ci_find","arguments":{"languageServer":{"path":"main.rs","offset":3,"operation":"hover"},"timeoutMs":600000}}}),
        ),
    );
    let request_elapsed = started.elapsed();
    let session_elapsed = initializing.elapsed();
    let provider_started = marker.exists();
    let stopping = Instant::now();
    let shutdown = server.shutdown();
    let shutdown_elapsed = stopping.elapsed();
    let diagnostic = format!(
        "initialization_us={} request_us={} session_us={} http_status={status} rpc_error_code={rpc_error_code:?} marker_present={provider_started} session_ttl_ms={session_ttl_ms} request_timeout_ms={request_timeout_ms} stall_initialize={stall_initialize}",
        initialization_elapsed.as_micros(),
        request_elapsed.as_micros(),
        session_elapsed.as_micros(),
    );
    assert!(
        shutdown.is_ok(),
        "Shutdown failed: {shutdown:?}; {diagnostic}"
    );
    assert_eq!(status, 503, "{diagnostic}");
    assert_eq!(
        provider_started, !stall_initialize,
        "Only the running-provider case must reach hover before expiry; {diagnostic}"
    );
    // An early provider/setup failure or the longer request deadline cannot
    // stand in for expiry of the original, nonrenewed session lifetime.
    assert!(
        session_elapsed >= Duration::from_millis(session_ttl_ms),
        "Request failed before session expiry; {diagnostic}"
    );
    let expiry_bound = if stall_initialize {
        // Retain the original short-TTL response plus shutdown ceiling.
        Duration::from_secs(3)
    } else {
        Duration::from_millis(session_ttl_ms) + Duration::from_secs(2)
    };
    assert!(
        request_elapsed + shutdown_elapsed < expiry_bound,
        "Expired session retained in-flight authority; {diagnostic}"
    );
    assert!(shutdown_elapsed < Duration::from_secs(3), "{diagnostic}");
}
