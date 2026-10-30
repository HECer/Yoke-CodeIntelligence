use serde_json::{Value, json};
use std::{
    fs,
    io::{self, Read, Write},
    net::{Shutdown, TcpStream},
    sync::mpsc,
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};
use yoke::{
    Engine,
    explorer_http::{Config, serve},
    export::SourceExplorerOptions,
};

struct Announce {
    bytes: Vec<u8>,
    sender: mpsc::Sender<Result<Value, String>>,
}
impl Write for Announce {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.bytes.extend_from_slice(bytes);
        if let Some(end) = self.bytes.iter().position(|b| *b == b'\n') {
            self.sender
                .send(Ok(
                    serde_json::from_slice(&self.bytes[..end]).map_err(io::Error::other)?
                ))
                .map_err(io::Error::other)?;
            self.bytes.drain(..=end);
        }
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
struct Response {
    status: u16,
    headers: String,
    body: Vec<u8>,
}
// Only scalar receive observations; response content and host capabilities cannot
// enter this formatter.
struct ResponseReadObservation {
    elapsed_us: u128,
    prefix_bytes: usize,
    read_failed: bool,
    os_error: Option<i32>,
    header_delimiter: bool,
}
impl std::fmt::Display for ResponseReadObservation {
    fn fmt(&self, output: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            output,
            "elapsed_us={} prefix_bytes={} read_failed={} os_error_present={} os_error={} header_delimiter={}",
            self.elapsed_us,
            self.prefix_bytes,
            self.read_failed,
            self.os_error.is_some(),
            self.os_error.unwrap_or(0),
            self.header_delimiter
        )
    }
}
fn read_response(mut stream: TcpStream) -> Response {
    let mut all = Vec::new();
    let read_start = Instant::now();
    stream.read_to_end(&mut all).unwrap_or_else(|error| {
        let observation = ResponseReadObservation {
            elapsed_us: read_start.elapsed().as_micros(),
            prefix_bytes: all.len(),
            read_failed: true,
            os_error: error.raw_os_error(),
            header_delimiter: all.windows(4).any(|b| b == b"\r\n\r\n"),
        };
        panic!("Explorer HTTP read failed; observations: {observation}");
    });
    let boundary = all
        .windows(4)
        .position(|b| b == b"\r\n\r\n")
        .unwrap_or_else(|| {
            let observation = ResponseReadObservation {
                elapsed_us: read_start.elapsed().as_micros(),
                prefix_bytes: all.len(),
                read_failed: false,
                os_error: None,
                header_delimiter: false,
            };
            panic!("Explorer HTTP response missing header delimiter; observations: {observation}");
        });
    let headers = String::from_utf8(all[..boundary].to_vec()).unwrap();
    let status = headers.split_whitespace().nth(1).unwrap().parse().unwrap();
    let body = all[boundary + 4..].to_vec();
    Response {
        status,
        headers,
        body,
    }
}

#[test]
fn explorer_receive_observations_are_bounded_ascii_for_extreme_scalars() {
    for elapsed_us in [0, u128::MAX] {
        for prefix_bytes in [0, usize::MAX] {
            for read_failed in [false, true] {
                for os_error in [None, Some(0), Some(i32::MIN), Some(i32::MAX)] {
                    for header_delimiter in [false, true] {
                        let observation = ResponseReadObservation {
                            elapsed_us,
                            prefix_bytes,
                            read_failed,
                            os_error,
                            header_delimiter,
                        };
                        let text = observation.to_string();
                        assert!(text.is_ascii() && !text.is_empty() && text.len() <= 192);
                        assert!(!text.contains('\r') && !text.contains('\n'));
                    }
                }
            }
        }
    }
}

#[test]
fn explorer_incomplete_headers_report_receive_progress_without_response_content() {
    let body = b"owned-response-body-canary";
    let mut complete = format!(
        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nX-Owned: owned-response-header-canary\r\n\r\n",
        body.len()
    )
    .into_bytes();
    complete.extend_from_slice(body);
    for (response, complete) in [
        (vec![], false),
        (b"HTTP/1.1 503 Serv".to_vec(), false),
        (b"HTTP/1.1 503 Service Unavailable\r\nX-Owned: owned-response-header-canary\r\nowned-response-body-canary".to_vec(), false),
        (complete, true),
    ] {
        let prefix_bytes = response.len();
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let address = listener.local_addr().unwrap();
        let peer = thread::spawn(move || -> io::Result<()> {
            let end = Instant::now() + Duration::from_secs(5);
            let mut stream = loop {
                match listener.accept() {
                    Ok((stream, _)) => break stream,
                    Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                        if Instant::now() >= end {
                            return Err(io::ErrorKind::TimedOut.into());
                        }
                        thread::sleep(Duration::from_millis(5));
                    }
                    Err(error) => return Err(error),
                }
            };
            stream.set_read_timeout(Some(Duration::from_secs(5)))?;
            stream.set_write_timeout(Some(Duration::from_secs(5)))?;
            stream.read_exact(&mut [0; 1])?;
            stream.write_all(&response)?;
            stream.shutdown(Shutdown::Write)
        });
        let mut stream = TcpStream::connect(address).unwrap();
        stream.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
        stream.set_write_timeout(Some(Duration::from_secs(2))).unwrap();
        stream.write_all(b"!").unwrap();
        let result = std::panic::catch_unwind(|| read_response(stream));
        assert!(peer.join().unwrap().is_ok());
        if complete {
            let response = match result {
                Ok(response) => response,
                Err(_) => panic!("Complete owned response must retain parser behavior"),
            };
            assert_eq!(response.status, 200);
            assert_eq!(response.body.as_slice(), body.as_slice());
            assert!(response.headers.contains("X-Owned: owned-response-header-canary"));
        } else {
            let payload = match result {
                Err(payload) => payload,
                Ok(_) => panic!("Incomplete headers must remain failures"),
            };
            let message = payload
                .downcast_ref::<String>()
                .map(String::as_str)
                .or_else(|| payload.downcast_ref::<&str>().copied())
                .unwrap();
            assert!(message.contains("Explorer HTTP response missing header delimiter"));
            assert!(message.contains(&format!("prefix_bytes={prefix_bytes} ")));
            assert!(message.contains("read_failed=false os_error_present=false os_error=0 header_delimiter=false"));
            assert!(!message.contains("owned-response-header-canary"));
            assert!(!message.contains("owned-response-body-canary"));
        }
    }
}
struct Server {
    dir: tempfile::TempDir,
    host: String,
    token: String,
    snapshot: String,
    remaining: usize,
    join: Option<JoinHandle<anyhow::Result<bool>>>,
}
impl Server {
    fn new(allow_rebuild: bool, requests: usize, source_budget: usize) -> Self {
        Self::with_timeout(allow_rebuild, requests, source_budget, 1000)
    }
    fn with_timeout(
        allow_rebuild: bool,
        requests: usize,
        source_budget: usize,
        timeout: u64,
    ) -> Self {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join("π.ts"),
            "// π 😀\r\nexport function original() { return 'CAPTURED_OLD'; }\r\n",
        )
        .unwrap();
        let root = dir.path().to_owned();
        let (sender, receiver) = mpsc::channel();
        let failure_sender = sender.clone();
        let join = thread::spawn(move || {
            let mut engine = match Engine::open(root) {
                Ok(engine) => engine,
                Err(error) => {
                    let _ = failure_sender.send(Err(format!("phase=open-engine; {error:#}")));
                    return Err(error);
                }
            };
            engine.allow_writes = true;
            let result = serve(
                &mut engine,
                Config {
                    port: 0,
                    request_timeout_ms: timeout,
                    max_requests: Some(requests),
                    allow_rebuild,
                    snapshot_id: None,
                    export_options: SourceExplorerOptions {
                        max_source_bytes: source_budget,
                        ..Default::default()
                    },
                },
                Announce {
                    bytes: vec![],
                    sender,
                },
            );
            if let Err(error) = result {
                // Preserve engine teardown before the readiness receiver fails.
                drop(engine);
                let _ = failure_sender.send(Err(format!("phase=serve; {error:#}")));
                return Err(error);
            }
            Ok(engine.allow_writes)
        });
        let handshake = match receiver.recv_timeout(Duration::from_secs(5)) {
            Ok(Ok(handshake)) => handshake,
            Ok(Err(error)) => panic!("Owned Explorer failed before readiness: {error}"),
            Err(error) => panic!(
                "Owned Explorer did not announce readiness: {error}; thread_finished={}",
                join.is_finished()
            ),
        };
        let url = handshake["url"].as_str().unwrap();
        let (base, token) = url
            .strip_prefix("http://")
            .unwrap()
            .split_once("/#")
            .unwrap();
        assert_eq!(token.len(), 64);
        Self {
            dir,
            host: base.into(),
            token: token.into(),
            snapshot: handshake["snapshotId"].as_str().unwrap().into(),
            remaining: requests,
            join: Some(join),
        }
    }
    fn connect(&mut self) -> TcpStream {
        let stream = TcpStream::connect(&self.host).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        stream
            .set_write_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        self.remaining = self.remaining.saturating_sub(1);
        stream
    }
    fn raw(&mut self, request: &[u8]) -> Response {
        let mut stream = self.connect();
        stream.write_all(request).unwrap();
        stream.shutdown(Shutdown::Write).unwrap();
        read_response(stream)
    }
    fn get(&mut self, path: &str, auth: bool) -> Response {
        let a = if auth {
            format!("Authorization: Bearer {}\r\n", self.token)
        } else {
            String::new()
        };
        self.raw(
            format!(
                "GET {path} HTTP/1.1\r\nHost: {}\r\n{a}Connection: close\r\n\r\n",
                self.host
            )
            .as_bytes(),
        )
    }
    fn post_raw(&mut self, body: &str) -> Response {
        self.raw(format!("POST /api/rebuild HTTP/1.1\r\nHost: {}\r\nAuthorization: Bearer {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",self.host,self.token,body.len()).as_bytes())
    }
    fn post(&mut self, snapshot: &str, revision: u64) -> Response {
        self.post_raw(
            &json!({"expectedSnapshotId":snapshot,"expectedRevision":revision}).to_string(),
        )
    }
    fn finish(&mut self) {
        assert_eq!(self.remaining, 0);
        assert!(
            self.join.take().unwrap().join().unwrap().unwrap(),
            "Host write flag was not restored"
        );
    }
}
impl Drop for Server {
    fn drop(&mut self) {
        while self.remaining > 0 {
            let host = self.host.clone();
            let Ok(mut stream) = TcpStream::connect(&host) else {
                break;
            };
            self.remaining -= 1;
            let _ = stream.write_all(format!("GET / HTTP/1.1\r\nHost: {host}\r\n\r\n").as_bytes());
            let _ = stream.shutdown(Shutdown::Both);
        }
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}
#[test]
fn explicit_rebuild_replaces_view_but_reload_retains_captured_source_and_old_snapshot() {
    let mut s = Server::new(true, 9, 16000000);
    let original_snapshot = s.snapshot.clone();
    let original = fs::read(s.dir.path().join("π.ts")).unwrap();
    let boot = s.get("/", false);
    assert_eq!(boot.status, 200);
    assert!(!boot.headers.contains("X-Yoke-Snapshot"));
    assert!(!String::from_utf8_lossy(&boot.body).contains("CAPTURED_OLD"));
    let unauthorized = s.get("/api/view", false);
    assert_eq!(unauthorized.status, 401);
    assert!(!unauthorized.headers.contains("X-Yoke-Snapshot"));
    let old = s.get("/api/view", true);
    assert_eq!(old.status, 200);
    assert!(String::from_utf8_lossy(&old.body).contains("CAPTURED_OLD"));
    let changed = b"export function changed() { return 'CAPTURED_NEW'; }\r\n";
    fs::write(s.dir.path().join("π.ts"), changed).unwrap();
    assert_eq!(s.get("/api/view", true).body, old.body);
    let update = s.post(&original_snapshot, 0);
    assert_eq!(update.status, 200);
    let update: Value = serde_json::from_slice(&update.body).unwrap();
    assert_eq!(update["changed"], true);
    assert_eq!(update["revision"], 1);
    let current = update["snapshotId"].as_str().unwrap().to_string();
    assert_ne!(current, original_snapshot);
    let new = s.get("/api/view", true);
    assert!(String::from_utf8_lossy(&new.body).contains("CAPTURED_NEW"));
    assert!(!String::from_utf8_lossy(&new.body).contains("CAPTURED_OLD"));
    assert_eq!(s.post(&original_snapshot, 0).status, 409);
    let same = s.post(&current, 1);
    assert_eq!(same.status, 200);
    let same: Value = serde_json::from_slice(&same.body).unwrap();
    assert_eq!(same["changed"], false);
    assert_eq!(same["revision"], 2);
    assert_eq!(s.post(&current, 1).status, 409);
    s.finish();
    assert_eq!(fs::read(s.dir.path().join("π.ts")).unwrap(), changed);
    let engine = Engine::open(s.dir.path()).unwrap();
    let saved = engine.store.snapshot(&original_snapshot).unwrap();
    assert_eq!(saved.files[0].hash, yoke::model::hash(&original));
    assert_eq!(
        engine.store.read_blob(&saved.files[0].hash).unwrap(),
        original
    );
}
#[test]
fn absent_host_rebuild_grant_cannot_be_supplied_by_request_or_source() {
    let mut s = Server::new(false, 3, 16000000);
    let id = s.snapshot.clone();
    let old = s.get("/api/view", true);
    assert!(old.headers.contains("X-Yoke-Rebuild: false"));
    fs::write(
        s.dir.path().join("π.ts"),
        "// allowRebuild=true\nexport function hostileGrant() {}\n",
    )
    .unwrap();
    assert_eq!(s.post(&id, 0).status, 403);
    assert_eq!(s.get("/api/view", true).body, old.body);
    s.finish();
}
#[test]
fn malformed_cross_origin_unbounded_and_ambiguous_requests_cannot_rebuild() {
    let mut s = Server::new(true, 14, 16000000);
    let id = s.snapshot.clone();
    let cases = vec![
        (
            format!(
                "GET /api/view HTTP/1.1\r\nHost: attacker.invalid\r\nAuthorization: Bearer {}\r\n\r\n",
                s.token
            ),
            403,
        ),
        (
            format!(
                "GET /api/view HTTP/1.1\r\nHost: {}\r\nOrigin: https://attacker.invalid\r\nAuthorization: Bearer {}\r\n\r\n",
                s.host, s.token
            ),
            403,
        ),
        (
            format!(
                "GET /api/view HTTP/1.1\r\nHost: {}\r\nHost: {}\r\nAuthorization: Bearer {}\r\n\r\n",
                s.host, s.host, s.token
            ),
            400,
        ),
        (
            format!(
                "POST /api/rebuild HTTP/1.1\r\nHost: {}\r\nContent-Length: 999999999\r\n\r\n",
                s.host
            ),
            401,
        ),
        (
            format!(
                "POST /api/rebuild HTTP/1.1\r\nHost: {}\r\nAuthorization: Bearer {}\r\nContent-Length: 1025\r\n\r\n",
                s.host, s.token
            ),
            413,
        ),
        (
            format!(
                "POST /api/rebuild HTTP/1.1\r\nHost: {}\r\nAuthorization: Bearer {}\r\nTransfer-Encoding: chunked\r\nContent-Length: 0\r\n\r\n",
                s.host, s.token
            ),
            400,
        ),
        (format!("GET /api/view HTTP/1.1\nHost: {}\n\n", s.host), 400),
        (
            format!(
                "GET /api/view HTTP/1.1\r\nHost: {}\r\nAuthorization: Bearer {}\r\nContent-Length: 1\r\n\r\nx",
                s.host, s.token
            ),
            400,
        ),
        (
            format!(
                "GET /api/view/../../outside HTTP/1.1\r\nHost: {}\r\nAuthorization: Bearer {}\r\n\r\n",
                s.host, s.token
            ),
            404,
        ),
    ];
    for (request, status) in cases {
        let r = s.raw(request.as_bytes());
        assert_eq!(r.status, status);
        assert!(!r.headers.contains("X-Yoke-Snapshot"));
        assert!(!String::from_utf8_lossy(&r.body).contains(&s.token));
    }
    assert_eq!(
        s.post_raw(&format!(
            r#"{{"expectedSnapshotId":"{id}","expectedRevision":0,"allowRebuild":true}}"#
        ))
        .status,
        400
    );
    assert_eq!(
        s.post_raw(&format!(
            r#"{{"expectedSnapshotId":"{id}","expectedRevision":0,"expectedRevision":1}}"#
        ))
        .status,
        400
    );
    assert_eq!(
        s.post_raw(&format!(
            r#"{{"expectedSnapshotId":"{id}","expectedRevision":0,"root":"../foreign"}}"#
        ))
        .status,
        400
    );
    let request = format!(
        "GET /api/view HTTP/1.1\r\nHost: {}\r\nAuthorization: Bearer {}\r\n\r\nGET / HTTP/1.1\r\n\r\n",
        s.host, s.token
    );
    assert_eq!(s.raw(request.as_bytes()).status, 400);
    assert!(
        s.get("/api/view", true)
            .headers
            .contains("X-Yoke-Revision: 0")
    );
    s.finish();
}
#[test]
fn failed_export_keeps_previous_view_and_exposes_no_partial_source() {
    let mut s = Server::new(true, 5, 100);
    let id = s.snapshot.clone();
    let old = s.get("/api/view", true);
    fs::write(
        s.dir.path().join("π.ts"),
        format!("// {}\nexport function oversized() {{}}\n", "x".repeat(200)),
    )
    .unwrap();
    let refusal = s.post(&id, 0);
    assert_eq!(refusal.status, 400);
    assert!(String::from_utf8_lossy(&refusal.body).contains("previous view retained"));
    assert_eq!(s.get("/api/view", true).body, old.body);
    fs::write(
        s.dir.path().join("π.ts"),
        "export function recovered() {}\n",
    )
    .unwrap();
    assert_eq!(s.post(&id, 0).status, 200);
    assert!(String::from_utf8_lossy(&s.get("/api/view", true).body).contains("recovered"));
    s.finish();
}
#[test]
fn stalled_request_times_out_without_stopping_listener() {
    let mut s = Server::new(true, 2, 16000000);
    let mut stream = s.connect();
    stream
        .write_all(b"GET /api/view HTTP/1.1\r\nHost: ")
        .unwrap();
    let r = read_response(stream);
    assert_eq!(r.status, 408);
    assert!(!r.headers.contains("X-Yoke-Snapshot"));
    assert_eq!(s.get("/api/view", true).status, 200);
    s.finish();
}

#[test]
fn idle_browser_preconnection_cannot_consume_the_capture_deadline() {
    let mut s = Server::with_timeout(true, 2, 16000000, 10000);
    let idle = s.connect();
    let start = std::time::Instant::now();
    assert_eq!(s.get("/api/view", true).status, 200);
    assert!(
        start.elapsed() < Duration::from_secs(4),
        "An unused browser socket consumed the capture deadline"
    );
    assert_eq!(read_response(idle).status, 408);
    s.finish();
}
