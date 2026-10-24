use serde_json::{Value, json};
use std::{
    fs,
    io::{self, Read, Write},
    net::{Shutdown, TcpStream},
    sync::{
        Arc,
        atomic::{AtomicU8, Ordering},
        mpsc,
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};
use tempfile::TempDir;
use yoke::{
    Engine,
    editor_http::{Config, serve},
};

struct HandshakeWriter {
    bytes: Vec<u8>,
    sender: mpsc::Sender<Value>,
    startup_stage: Arc<AtomicU8>,
}
impl Write for HandshakeWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.startup_stage.store(3, Ordering::Release);
        self.bytes.extend_from_slice(bytes);
        if let Some(end) = self.bytes.iter().position(|b| *b == b'\n') {
            let value = serde_json::from_slice(&self.bytes[..end]).map_err(io::Error::other)?;
            self.sender.send(value).map_err(io::Error::other)?;
            self.startup_stage.store(4, Ordering::Release);
            self.bytes.drain(..=end);
        }
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
struct Server {
    _dir: TempDir,
    endpoint: String,
    token: String,
    remaining: usize,
    join: Option<JoinHandle<anyhow::Result<()>>>,
}
impl Server {
    fn new(requests: usize, timeout: u64) -> Self {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("app.rs"), "fn saved() {}\n").unwrap();
        let root = dir.path().to_owned();
        let (sender, receiver) = mpsc::channel();
        let startup_stage = Arc::new(AtomicU8::new(0));
        let worker_stage = Arc::clone(&startup_stage);
        let startup_started = Instant::now();
        let join = thread::spawn(move || {
            worker_stage.store(1, Ordering::Release);
            let mut engine = Engine::open(root)?;
            worker_stage.store(2, Ordering::Release);
            // Transport must remain read-only even when the owning host enables writes.
            engine.allow_writes = true;
            serve(
                &mut engine,
                Config {
                    port: 0,
                    request_timeout_ms: timeout,
                    max_requests: Some(requests),
                },
                HandshakeWriter {
                    bytes: Vec::new(),
                    sender,
                    startup_stage: worker_stage,
                },
            )
        });
        let handshake = receiver
            .recv_timeout(Duration::from_secs(3))
            .unwrap_or_else(|error| {
                let stage = match startup_stage.load(Ordering::Acquire) {
                    0 => "worker_not_entered",
                    1 => "engine_open_entered",
                    2 => "engine_open_completed",
                    3 => "announcement_write_entered",
                    4 => "announcement_sent",
                    _ => "invalid_fixture_stage",
                };
                panic!(
                    "Editor service must announce its loopback endpoint: {error:?}; stage={stage}; elapsed={:?}; worker_finished={}",
                    startup_started.elapsed(),
                    join.is_finished()
                );
            });
        let endpoint = handshake["endpoint"].as_str().unwrap().to_string();
        let token = handshake["token"].as_str().unwrap().to_string();
        assert!(endpoint.starts_with("http://127.0.0.1:") && endpoint.ends_with("/editor"));
        assert!(token.len() == 64 && token.bytes().all(|b| b.is_ascii_hexdigit()));
        Self {
            _dir: dir,
            endpoint,
            token,
            remaining: requests,
            join: Some(join),
        }
    }
    fn host(&self) -> &str {
        self.endpoint
            .strip_prefix("http://")
            .unwrap()
            .strip_suffix("/editor")
            .unwrap()
    }
    fn connect(&mut self) -> TcpStream {
        let stream = TcpStream::connect(self.host()).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(10)))
            .unwrap();
        stream
            .set_write_timeout(Some(Duration::from_secs(4)))
            .unwrap();
        self.remaining = self.remaining.saturating_sub(1);
        stream
    }
    fn request(&mut self, value: Value) -> (u16, Value) {
        let body = serde_json::to_vec(&value).unwrap();
        let headers = format!(
            "POST /editor HTTP/1.1\r\nHost: {}\r\nAuthorization: Bearer {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            self.host(),
            self.token,
            body.len()
        );
        let mut request = headers.into_bytes();
        request.extend(body);
        self.raw(&request)
    }
    fn raw(&mut self, bytes: &[u8]) -> (u16, Value) {
        let mut stream = self.connect();
        stream.write_all(bytes).unwrap();
        stream.shutdown(Shutdown::Write).unwrap();
        read_response(stream)
    }
    fn finish(mut self) {
        assert_eq!(self.remaining, 0);
        assert!(
            self.join.take().unwrap().join().unwrap().is_ok(),
            "Editor service failed"
        );
    }
}
impl Drop for Server {
    fn drop(&mut self) {
        while self.remaining > 0 {
            let host = self.host().to_string();
            let Ok(mut stream) = TcpStream::connect(&host) else {
                break;
            };
            self.remaining -= 1;
            let _ = stream.write_all(
                format!("POST /editor HTTP/1.1\r\nHost: {host}\r\nContent-Length: 0\r\n\r\n")
                    .as_bytes(),
            );
            let _ = stream.shutdown(Shutdown::Both);
        }
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}
fn read_response(mut stream: TcpStream) -> (u16, Value) {
    let mut bytes = Vec::new();
    let mut byte = [0u8; 1];
    while !bytes.ends_with(b"\r\n\r\n") {
        stream
            .read_exact(&mut byte)
            .expect("Complete HTTP response headers");
        bytes.push(byte[0]);
        assert!(bytes.len() < 16384);
    }
    let header = std::str::from_utf8(&bytes).unwrap();
    let status = header.split_whitespace().nth(1).unwrap().parse().unwrap();
    assert!(header.to_ascii_lowercase().contains("connection: close"));
    assert!(
        header
            .to_ascii_lowercase()
            .contains("cache-control: no-store")
    );
    let length = header
        .lines()
        .find_map(|line| line.strip_prefix("Content-Length: "))
        .unwrap()
        .parse::<usize>()
        .unwrap();
    let mut body = vec![0u8; length];
    stream
        .read_exact(&mut body)
        .expect("Complete HTTP response body");
    (status, serde_json::from_slice(&body).unwrap())
}

#[cfg(windows)]
fn constrain_receive_window(stream: &TcpStream) {
    use std::os::windows::io::AsRawSocket;
    #[link(name = "Ws2_32")]
    unsafe extern "system" {
        fn setsockopt(socket: usize, level: i32, option: i32, value: *const i8, length: i32)
        -> i32;
    }
    let size = 8192i32;
    // The test owns a live TCP socket. Set a small receive buffer; the dedicated
    // write test observes actual WouldBlock rather than assuming this stalls TCP.
    let result = unsafe {
        setsockopt(
            stream.as_raw_socket() as usize,
            0xffff,
            0x1002,
            (&size as *const i32).cast(),
            4,
        )
    };
    assert_eq!(result, 0, "Cannot constrain test receive window");
}
#[cfg(unix)]
fn constrain_receive_window(stream: &TcpStream) {
    use std::os::fd::AsRawFd;
    unsafe extern "C" {
        fn setsockopt(
            socket: i32,
            level: i32,
            option: i32,
            value: *const std::ffi::c_void,
            length: u32,
        ) -> i32;
    }
    #[cfg(target_os = "linux")]
    let (level, option) = (1, 8);
    #[cfg(not(target_os = "linux"))]
    let (level, option) = (0xffff, 0x1002);
    let size = 8192i32;
    // BSD/macOS and Linux use different SOL_SOCKET/SO_RCVBUF values.
    let result = unsafe {
        setsockopt(
            stream.as_raw_fd(),
            level,
            option,
            (&size as *const i32).cast(),
            4,
        )
    };
    assert_eq!(result, 0, "Cannot constrain test receive window");
}

#[test]
fn real_editor_requests_read_unsaved_source_without_workspace_write_authority() {
    let mut server = Server::new(7, 10000);
    let (status, opened) = server.request(json!({"operation":"open"}));
    assert_eq!(status, 200);
    assert_eq!(opened["ok"], true);
    let id = opened["data"]["id"].as_str().unwrap().to_string();
    let (status,updated)=server.request(json!({"operation":"buffer","sessionId":id,"buffer":{"path":"app.rs","version":1,"text":"fn unsaved() {}\n"}}));
    assert_eq!(status, 200);
    assert_eq!(updated["ok"], true);
    let (status, read) = server.request(
        json!({"operation":"tool","sessionId":id,"tool":"ci_read","arguments":{"path":"app.rs"}}),
    );
    assert_eq!(status, 200);
    assert_eq!(read["data"]["data"]["text"], "fn unsaved() {}\n");
    let (status, capabilities) = server
        .request(json!({"operation":"tool","sessionId":id,"tool":"ci_status","arguments":{}}));
    assert_eq!(status, 200);
    assert_eq!(
        capabilities["data"]["data"]["capabilities"]["workspaceWrites"],
        false
    );
    let (status,_)=server.request(json!({"operation":"tool","sessionId":id,"tool":"ci_change_apply","arguments":{"mode":"workspace"}}));
    assert_eq!(status, 403);
    let (status,_)=server.request(json!({"operation":"tool","sessionId":id,"tool":"ci_read","arguments":{"path":"app.rs","allowWrites":true}}));
    assert_eq!(status, 400);
    let (status, _) = server.request(json!({"operation":"close","sessionId":id}));
    assert_eq!(status, 200);
    assert_eq!(
        fs::read_to_string(server._dir.path().join("app.rs")).unwrap(),
        "fn saved() {}\n"
    );
    server.finish();
}

#[test]
fn host_origin_and_auth_are_checked_before_waiting_for_declared_body() {
    let mut server = Server::new(4, 1000);
    let host = server.host().to_string();
    let requests = [
        (
            format!("POST /editor HTTP/1.1\r\nHost: {host}\r\nContent-Length: 10000000\r\n\r\n"),
            401,
        ),
        (
            format!(
                "POST /editor HTTP/1.1\r\nHost: evil.invalid\r\nAuthorization: Bearer {}\r\nContent-Length: 10000000\r\n\r\n",
                server.token
            ),
            403,
        ),
        (
            format!(
                "POST /editor HTTP/1.1\r\nHost: {host}\r\nOrigin: https://evil.invalid\r\nAuthorization: Bearer {}\r\nContent-Length: 10000000\r\n\r\n",
                server.token
            ),
            403,
        ),
        (
            format!(
                "POST /editor HTTP/1.1\r\nHost: {host}\r\nAuthorization: Bearer {}x\r\nContent-Length: 10000000\r\n\r\n",
                server.token
            ),
            401,
        ),
    ];
    for (request, expected) in requests {
        let started = Instant::now();
        let (status, body) = server.raw(request.as_bytes());
        assert_eq!(status, expected);
        assert!(!body.to_string().contains(&server.token));
        assert!(started.elapsed() < Duration::from_millis(800));
    }
    server.finish();
}

#[test]
fn framing_rejects_duplicate_lengths_transfer_encoding_and_invalid_payloads() {
    let mut server = Server::new(5, 1000);
    let host = server.host().to_string();
    let auth = format!("Authorization: Bearer {}\r\n", server.token);
    let malformed: [String; 5] = [
        "Content-Length: 2\r\nContent-Length: 2\r\n\r\n{}".into(),
        "Content-Length: 2\r\nTransfer-Encoding: chunked\r\n\r\n{}".into(),
        "Content-Length: -1\r\n\r\n".into(),
        "Content-Length: 999999999\r\n\r\n".into(),
        "Content-Length: 1\r\n\r\n{".into(),
    ];
    for (index, tail) in malformed.iter().enumerate() {
        let request = format!(
            "POST /editor HTTP/1.1\r\nHost: {host}\r\n{auth}Content-Type: application/json\r\n{tail}"
        );
        let (status, body) = server.raw(request.as_bytes());
        assert_eq!(status, if index == 3 { 413 } else { 400 });
        assert!(!body.to_string().contains(&server.token));
    }
    server.finish();
}

#[test]
fn stale_buffer_and_close_versions_cannot_replace_newer_editor_source() {
    let mut server = Server::new(7, 10000);
    let (_, opened) = server.request(json!({"operation":"open"}));
    let id = opened["data"]["id"].clone();
    assert_eq!(server.request(json!({"operation":"buffer","sessionId":id,"buffer":{"path":"app.rs","version":2,"text":"fn newer() {}"}})).0,200);
    assert_eq!(server.request(json!({"operation":"buffer","sessionId":id,"buffer":{"path":"app.rs","version":1,"text":"fn stale() {}"}})).0,409);
    assert_eq!(
        server
            .request(json!({"operation":"closeFile","sessionId":id,"path":"app.rs","version":1}))
            .0,
        409
    );
    let (_, read) = server.request(
        json!({"operation":"tool","sessionId":id,"tool":"ci_read","arguments":{"path":"app.rs"}}),
    );
    assert_eq!(read["data"]["data"]["text"], "fn newer() {}");
    assert_eq!(
        server
            .request(json!({"operation":"closeFile","sessionId":id,"path":"app.rs","version":2}))
            .0,
        200
    );
    let (_, read) = server.request(
        json!({"operation":"tool","sessionId":id,"tool":"ci_read","arguments":{"path":"app.rs"}}),
    );
    assert_eq!(read["data"]["data"]["text"], "fn saved() {}\n");
    server.finish();
}

#[test]
fn slow_header_bytes_cannot_reset_the_connection_deadline() {
    let mut server = Server::new(1, 200);
    let mut stream = server.connect();
    let started = Instant::now();
    stream.write_all(b"P").unwrap();
    thread::sleep(Duration::from_millis(70));
    let _ = stream.write_all(b"O");
    thread::sleep(Duration::from_millis(70));
    let _ = stream.write_all(b"S");
    thread::sleep(Duration::from_millis(70));
    let (status, body) = read_response(stream);
    assert_eq!(status, 408);
    assert!(!body.to_string().contains(&server.token));
    assert!(started.elapsed() < Duration::from_millis(800));
    server.finish();
}

#[test]
fn pipelining_and_duplicate_authorization_cannot_dispatch_editor_operations() {
    let mut server = Server::new(3, 1000);
    let host = server.host().to_string();
    let body = b"{\"operation\":\"open\"}";
    let prefix = format!(
        "POST /editor HTTP/1.1\r\nHost: {host}\r\nAuthorization: Bearer {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n",
        server.token,
        body.len()
    );
    let mut pipelined = prefix.clone().into_bytes();
    pipelined.extend(body);
    pipelined.extend(prefix.as_bytes());
    pipelined.extend(body);
    assert_eq!(server.raw(&pipelined).0, 400);
    let duplicate = prefix.replace(
        "Content-Type:",
        &format!("Authorization: Bearer {}\r\nContent-Type:", server.token),
    );
    let mut request = duplicate.into_bytes();
    request.extend(body);
    assert_eq!(server.raw(&request).0, 400);
    assert_eq!(server.request(json!({"operation":"open"})).0, 200);
    let engine = Engine::open(server._dir.path()).unwrap();
    let count: i64 = engine
        .store
        .db
        .query_row(
            "SELECT count(*) FROM records WHERE kind='editor-session'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(count, 1);
    server.finish();
}

#[test]
fn slow_body_chunks_share_the_header_deadline() {
    let mut server = Server::new(1, 250);
    let host = server.host().to_string();
    let header = format!(
        "POST /editor HTTP/1.1\r\nHost: {host}\r\nAuthorization: Bearer {}\r\nContent-Type: application/json\r\nContent-Length: 30\r\n\r\n",
        server.token
    );
    let mut stream = server.connect();
    let started = Instant::now();
    stream.write_all(header.as_bytes()).unwrap();
    stream.write_all(b"{").unwrap();
    thread::sleep(Duration::from_millis(90));
    let _ = stream.write_all(b"\"");
    thread::sleep(Duration::from_millis(90));
    let _ = stream.write_all(b"o");
    thread::sleep(Duration::from_millis(90));
    assert_eq!(read_response(stream).0, 408);
    assert!(started.elapsed() < Duration::from_millis(900));
    server.finish();
}

#[test]
fn excessive_headers_and_unknown_operation_fields_are_rejected() {
    let mut server = Server::new(3, 1000);
    let host = server.host().to_string();
    let header = format!(
        "POST /editor HTTP/1.1\r\nHost: {host}\r\nX-Large: {}\r\n\r\n",
        "x".repeat(17000)
    );
    assert_eq!(server.raw(header.as_bytes()).0, 431);
    assert_eq!(
        server
            .request(json!({"operation":"open","allowWrites":true}))
            .0,
        400
    );
    assert_eq!(server.request(json!({"operation":"unknown"})).0, 400);
    server.finish();
}

#[test]
fn maximally_escaped_valid_buffer_fits_the_transport_without_relaxing_source_limit() {
    let mut server = Server::new(3, 10000);
    let (_, opened) = server.request(json!({"operation":"open"}));
    let id = opened["data"]["id"].clone();
    let text = "\u{1}".repeat(4_000_000);
    let (status,value)=server.request(json!({"operation":"buffer","sessionId":id,"buffer":{"path":"escaped.md","version":1,"text":text}}));
    assert_eq!(status, 200);
    assert_eq!(value["ok"], true);
    let oversized = "\u{1}".repeat(4_000_001);
    assert_eq!(server.request(json!({"operation":"buffer","sessionId":id,"buffer":{"path":"too-large.md","version":1,"text":oversized}})).0,400);
    server.finish();
}

#[test]
fn nonreading_client_does_not_block_the_next_authenticated_request() {
    let mut server = Server::new(2, 5000);
    fs::write(server._dir.path().join("large.md"), "x".repeat(3_600_000)).unwrap();
    let id = {
        let mut engine = Engine::open(server._dir.path()).unwrap();
        yoke::editor::open(&mut engine, &yoke::capture::Deadline::new(10000))
            .unwrap()
            .id
    };
    let body=serde_json::to_vec(&json!({"operation":"tool","sessionId":id,"tool":"ci_read","arguments":{"path":"large.md","maxBytes":4_000_000}})).unwrap();
    let headers = format!(
        "POST /editor HTTP/1.1\r\nHost: {}\r\nAuthorization: Bearer {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n",
        server.host(),
        server.token,
        body.len()
    );
    let mut stalled = server.connect();
    constrain_receive_window(&stalled);
    stalled.write_all(headers.as_bytes()).unwrap();
    stalled.write_all(&body).unwrap();
    let started = Instant::now();
    let (status, _) = server.request(json!({"operation":"status","sessionId":id}));
    assert_eq!(status, 200);
    assert!(started.elapsed() < Duration::from_secs(6));
    drop(stalled);
    server.finish();
}
