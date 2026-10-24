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
fn exchange(address: SocketAddr, bytes: &[u8]) -> (u16, String) {
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
                return (
                    header.split_whitespace().nth(1).unwrap().parse().unwrap(),
                    header.to_owned(),
                );
            }
            assert!(reply.len() < end + 4 + length);
        }
    }
}

#[test]
fn single_worker_session_expiry_cancels_an_already_running_provider() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("main.rs"), "fn target(){}\n").unwrap();
    let marker = root.path().join("hover.log");
    let mut target = Target::new(root.path(), Profile::Native, HostProfile::default());
    target.servers.push(yoke::lsp::ConfiguredServer {
        server: yoke::lsp::Server {
            language: "rust".into(),
            command: "node".into(),
            args: vec![
                std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("tests/fixtures/mcp-cancel-lsp.mjs")
                    .display()
                    .to_string(),
                marker.display().to_string(),
            ],
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
            session_ttl_ms: 1200,
            request_timeout_ms: 5000,
            ..Default::default()
        },
        vec![principal],
    )
    .unwrap();
    let address = server.local_addr();
    let (status, header) = exchange(
        address,
        &frame(
            address,
            None,
            json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25"}}),
        ),
    );
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
    let (status, _) = exchange(
        address,
        &frame(
            address,
            Some(sid),
            json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"ci_find","arguments":{"languageServer":{"path":"main.rs","offset":3,"operation":"hover"},"timeoutMs":600000}}}),
        ),
    );
    assert!(
        marker.exists(),
        "The provider must actually run before session expiration"
    );
    server.shutdown().unwrap();
    assert_eq!(status, 503);
    assert!(
        started.elapsed() < Duration::from_secs(3),
        "Expired session retained in-flight authority for {:?}",
        started.elapsed()
    );
}
