use super::*;
use crate::model;
use sha2::{Digest, Sha256};
use std::{
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    sync::{
        Arc, Mutex, OnceLock,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    thread,
    time::Instant,
};

const TOKEN: &str = "owned_access_token_canary_0123456789";
const KEY: &str = "owned_resource_key_canary";
fn deadline() -> Deadline {
    Deadline::new(30000)
}
fn image_hash() -> String {
    static HASH: OnceLock<String> = OnceLock::new();
    HASH.get_or_init(|| {
        let mut file = File::open(std::env::current_exe().unwrap()).unwrap();
        let mut hash = Sha256::new();
        let mut buffer = [0; 65536];
        loop {
            let count = file.read(&mut buffer).unwrap();
            if count == 0 {
                break;
            }
            hash.update(&buffer[..count]);
        }
        hex::encode(hash.finalize())
    })
    .clone()
}
fn pointer() -> Vec<u8> {
    serde_json::to_vec(&json!({"doc_id":"owned_123","resourceKey":KEY})).unwrap()
}
fn spec(bytes: &[u8]) -> HttpSpec {
    HttpSpec {
        shortcut_path: "notes.gdoc".into(),
        expected_shortcut_sha256: model::hash(bytes),
        expected_file_id: "owned_123".into(),
        address: "142.250.0.1:443".parse().unwrap(),
        expected_exporter_sha256: image_hash(),
    }
}
fn prepared(root: &Path) -> Prepared {
    let bytes = pointer();
    prepare(root, &bytes, &spec(&bytes), &Limits::default(), &deadline()).unwrap()
}
struct Peer {
    address: SocketAddr,
    request: Arc<Mutex<Vec<u8>>>,
    count: Arc<AtomicUsize>,
    stop: Arc<AtomicBool>,
    thread: Option<thread::JoinHandle<()>>,
}
impl Peer {
    fn new(response: Vec<u8>, hold: bool) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let address = listener.local_addr().unwrap();
        let request = Arc::new(Mutex::new(Vec::new()));
        let observed = Arc::clone(&request);
        let count = Arc::new(AtomicUsize::new(0));
        let calls = Arc::clone(&count);
        let stop = Arc::new(AtomicBool::new(false));
        let stopped = Arc::clone(&stop);
        let handle = thread::spawn(move || {
            let end = Instant::now() + Duration::from_secs(5);
            while !stopped.load(Ordering::SeqCst) && Instant::now() < end {
                match listener.accept() {
                    Ok((mut stream, _)) => {
                        calls.fetch_add(1, Ordering::SeqCst);
                        stream
                            .set_read_timeout(Some(Duration::from_millis(20)))
                            .unwrap();
                        stream
                            .set_write_timeout(Some(Duration::from_millis(300)))
                            .unwrap();
                        read_request(&mut stream, &observed, &stopped, end);
                        if hold {
                            while !stopped.load(Ordering::SeqCst) && Instant::now() < end {
                                thread::sleep(Duration::from_millis(2));
                            }
                        } else {
                            let _ = stream.write_all(&response);
                        }
                        break;
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(2))
                    }
                    Err(e) => panic!("Owned listener failed: {e}"),
                }
            }
        });
        Self {
            address,
            request,
            count,
            stop,
            thread: Some(handle),
        }
    }
    fn url(&self, p: &Prepared) -> url::Url {
        let mut url = p.url.clone();
        url.set_scheme("http").unwrap();
        url.set_host(Some("127.0.0.1")).unwrap();
        url.set_port(Some(self.address.port())).unwrap();
        url
    }
    fn wire(&self) -> Vec<u8> {
        self.request.lock().unwrap().clone()
    }
}
impl Drop for Peer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(handle) = self.thread.take() {
            handle.join().unwrap();
        }
    }
}
fn read_request(stream: &mut TcpStream, request: &Mutex<Vec<u8>>, stop: &AtomicBool, end: Instant) {
    let mut bytes = Vec::new();
    let mut chunk = [0; 1024];
    while bytes.len() < 16384 && Instant::now() < end && !stop.load(Ordering::SeqCst) {
        match stream.read(&mut chunk) {
            Ok(0) => break,
            Ok(n) => {
                bytes.extend_from_slice(&chunk[..n]);
                if bytes.first() == Some(&22) || bytes.windows(4).any(|v| v == b"\r\n\r\n") {
                    break;
                }
            }
            Err(e)
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) => {}
            Err(_) => break,
        }
    }
    *request.lock().unwrap() = bytes;
}
fn reply(status: &str, content_type: &str, body: &[u8], extras: &str) -> Vec<u8> {
    let mut bytes=format!("HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n{extras}\r\n",body.len()).into_bytes();
    bytes.extend_from_slice(body);
    bytes
}
fn receive_from(
    peer: &Peer,
    p: &Prepared,
    limits: &Limits,
    deadline: &Deadline,
) -> Result<ReceivedExport> {
    receive(
        p,
        peer.address,
        &HostAccessToken::from_host_input(TOKEN.as_bytes()).unwrap(),
        limits,
        deadline,
        &peer.url(p),
    )
}

#[test]
fn private_token_is_bounded_ascii_with_one_optional_line_ending_and_no_source_schema() {
    for raw in [
        b"owned_token".as_slice(),
        b"owned_token\n",
        b"owned_token\r\n",
    ] {
        assert_eq!(
            HostAccessToken::from_host_input(raw).unwrap().0.as_str(),
            "owned_token"
        );
    }
    for raw in [
        b"".as_slice(),
        b"space token",
        b"token\r",
        b"token\nextra",
        b"token\0",
        b"token\r\nInjected:x",
        b"\xff",
    ] {
        assert!(HostAccessToken::from_host_input(raw).is_err());
    }
    assert!(HostAccessToken::from_host_input(&vec![b'a'; MAX_TOKEN + 1]).is_err());
    let raw = json!({"shortcutPath":"notes.gdoc","expectedShortcutSha256":"a".repeat(64),"expectedFileId":"owned_123","address":"142.250.0.1:443","expectedExporterSha256":"b".repeat(64),"accessToken":TOKEN});
    assert!(serde_json::from_value::<HttpSpec>(raw).is_err());
}

#[test]
fn local_plan_pins_native_image_source_mime_and_address_without_private_values_or_effects() {
    let root = tempfile::tempdir().unwrap();
    let bytes = pointer();
    let selected = spec(&bytes);
    let plan = plan_http(
        root.path(),
        &bytes,
        &selected,
        &Limits::default(),
        &deadline(),
    )
    .unwrap();
    let serialized = serde_json::to_string(&plan).unwrap();
    assert!(!serialized.contains(KEY) && !serialized.contains(TOKEN));
    assert_eq!(plan["resourceKeyPresent"], true);
    assert_eq!(plan["requests"], 0);
    assert_eq!(plan["credentialsRead"], false);
    assert_eq!(plan["captureClaimsAreAuthority"], false);
    let url = url::Url::parse(plan["requestUrl"].as_str().unwrap()).unwrap();
    assert_eq!(url.host_str(), Some(HOST));
    assert_eq!(url.path(), "/drive/v3/files/owned_123/export");
    assert_eq!(
        url.query_pairs().collect::<Vec<_>>(),
        vec![("mimeType".into(), "text/markdown".into())]
    );
    assert!(!root.path().join(".yoke").exists());
}

#[test]
fn all_host_grants_precede_source_io_and_admission_refuses_stale_and_unsafe_selections() {
    let root = tempfile::tempdir().unwrap();
    let bytes = pointer();
    let mut selected = spec(&bytes);
    let token = HostAccessToken::from_host_input(TOKEN.as_bytes()).unwrap();
    for bits in 0..7 {
        let grants = HttpPermissions {
            allow_network: bits & 1 != 0,
            allow_private_credential: bits & 2 != 0,
            trust_native_exporter: bits & 4 != 0,
        };
        let error = capture_http(
            &root.path().join("absent"),
            b"invalid",
            &selected,
            &grants,
            &token,
            &Limits::default(),
            &deadline(),
        )
        .err()
        .unwrap();
        assert_eq!(
            error.to_string(),
            "google_workspace_http_host_permission_required"
        );
    }
    for (field, value) in [
        ("file", "wrong"),
        ("source", "0"),
        ("image", "0"),
        ("path", "../notes.gdoc"),
        ("port", "80"),
    ] {
        selected = spec(&bytes);
        match field {
            "file" => selected.expected_file_id = value.into(),
            "source" => selected.expected_shortcut_sha256 = "0".repeat(64),
            "image" => selected.expected_exporter_sha256 = "0".repeat(64),
            "path" => selected.shortcut_path = value.into(),
            "port" => selected.address.set_port(80),
            _ => unreachable!(),
        };
        assert!(
            plan_http(
                root.path(),
                &bytes,
                &selected,
                &Limits::default(),
                &deadline()
            )
            .is_err()
        );
    }
    for address in ["0.0.0.0:443", "224.0.0.1:443", "[::]:443"] {
        selected = spec(&bytes);
        selected.address = address.parse().unwrap();
        assert!(
            plan_http(
                root.path(),
                &bytes,
                &selected,
                &Limits::default(),
                &deadline()
            )
            .is_err()
        );
    }
    assert!(
        plan_http(
            root.path(),
            &bytes,
            &spec(&bytes),
            &Limits {
                max_body_bytes: 32,
                max_archive_bytes: 1
            },
            &deadline()
        )
        .is_err()
    );
}

#[test]
fn actual_owned_wire_carries_only_selected_private_headers_and_replays_exact_received_bytes() {
    let root = tempfile::tempdir().unwrap();
    let p = prepared(root.path());
    let body = "A\u{1f4a1}B\r\n".as_bytes();
    let peer = Peer::new(
        reply("200 OK", "text/markdown; charset=UTF-8", body, ""),
        false,
    );
    let source = receive_from(&peer, &p, &Limits::default(), &deadline()).unwrap();
    assert_eq!(peer.count.load(Ordering::SeqCst), 1);
    let wire = String::from_utf8(peer.wire()).unwrap();
    assert!(
        wire.starts_with(
            "GET /drive/v3/files/owned_123/export?mimeType=text%2Fmarkdown HTTP/1.1\r\n"
        )
    );
    assert!(
        wire.to_ascii_lowercase()
            .contains(&format!("authorization: bearer {}", TOKEN).to_ascii_lowercase())
    );
    assert!(
        wire.to_ascii_lowercase()
            .contains(&format!("x-goog-drive-resource-keys: owned_123/{KEY}").to_ascii_lowercase())
    );
    assert!(!wire.split(" HTTP/").next().unwrap().contains(KEY));
    assert!(wire.ends_with("\r\n\r\n"));
    let archive = source.archive(&deadline()).unwrap();
    let text = String::from_utf8(archive.clone()).unwrap();
    assert!(!text.contains(KEY) && !text.contains(TOKEN));
    assert!(text.contains(VERSION));
    let loaded = ReceivedExport::from_archive(&archive, &Limits::default(), &deadline()).unwrap();
    assert_eq!(
        loaded.read(0, body.len(), &deadline()).unwrap()["dataBase64"],
        base64::Engine::encode(&base64::prelude::BASE64_STANDARD, body)
    );
    assert_eq!(loaded.summary()["googleIdentityVerified"], false);
    assert!(!root.path().join(".yoke").exists());
}

#[test]
fn plain_and_spreadsheet_representations_and_absent_keys_use_the_same_bounded_protocol() {
    let root = tempfile::tempdir().unwrap();
    for (path, mime, body) in [
        ("notes.gslides", "text/plain", b"owned slides".as_slice()),
        (
            "notes.gsheet",
            "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
            b"PK\x05\x06".as_slice(),
        ),
    ] {
        let bytes = br#"{"doc_id":"owned_123"}"#;
        let mut selected = spec(bytes);
        selected.shortcut_path = path.into();
        let p = prepare(
            root.path(),
            bytes,
            &selected,
            &Limits::default(),
            &deadline(),
        )
        .unwrap();
        let peer = Peer::new(reply("200 OK", mime, body, ""), false);
        let source = receive_from(&peer, &p, &Limits::default(), &deadline()).unwrap();
        let wire = String::from_utf8(peer.wire()).unwrap();
        assert!(
            !wire
                .to_ascii_lowercase()
                .contains("x-goog-drive-resource-keys")
        );
        assert_eq!(source.summary()["bodyBytes"], body.len());
    }
}

#[test]
fn redirects_and_failures_are_one_attempt_without_forwarding_or_private_error_body() {
    let root = tempfile::tempdir().unwrap();
    let p = prepared(root.path());
    let target = Peer::new(reply("200 OK", "text/markdown", b"foreign", ""), false);
    for status in ["302 Found", "401 Unauthorized", "503 Unavailable"] {
        let peer = Peer::new(
            reply(
                status,
                "text/plain",
                TOKEN.as_bytes(),
                &format!("Location: http://{}/foreign\r\n", target.address),
            ),
            false,
        );
        let error = receive_from(&peer, &p, &Limits::default(), &deadline())
            .err()
            .unwrap();
        assert_eq!(error.to_string(), "google_workspace_http_status_refused");
        assert!(!format!("{error:?}").contains(TOKEN));
        assert_eq!(peer.count.load(Ordering::SeqCst), 1);
    }
    assert_eq!(target.count.load(Ordering::SeqCst), 0);
}

#[test]
fn protocol_byte_budget_encoding_and_secret_reflection_fail_without_archives() {
    let root = tempfile::tempdir().unwrap();
    let p = prepared(root.path());
    let limits = Limits {
        max_body_bytes: 64,
        max_archive_bytes: 16384,
    };
    for response in [
        reply("200 OK", "application/json", b"{}", ""),
        reply(
            "200 OK",
            "text/markdown",
            b"content",
            "Content-Encoding: gzip\r\n",
        ),
        reply(
            "200 OK",
            "text/markdown",
            b"ok",
            "Content-Type: text/markdown\r\n",
        ),
        reply("200 OK", "text/markdown", TOKEN.as_bytes(), ""),
        reply("200 OK", "text/markdown", KEY.as_bytes(), ""),
        reply("200 OK", "text/markdown", &[b'x'; 65], ""),
        reply("200 OK", "text/markdown", b"\xff", ""),
    ] {
        let peer = Peer::new(response, false);
        let error = receive_from(&peer, &p, &limits, &deadline()).err().unwrap();
        let displayed = format!("{error:?}");
        assert!(!displayed.contains(TOKEN) && !displayed.contains(KEY));
    }
}

#[test]
fn deadlines_and_cancellation_stop_an_owned_waiting_peer_and_never_return_empty_success() {
    let root = tempfile::tempdir().unwrap();
    let p = prepared(root.path());
    let peer = Peer::new(Vec::new(), true);
    let start = Instant::now();
    assert!(receive_from(&peer, &p, &Limits::default(), &Deadline::new(80)).is_err());
    assert!(start.elapsed() < Duration::from_secs(2));
    drop(peer);
    let peer = Peer::new(Vec::new(), true);
    let cancel = Arc::new(AtomicBool::new(false));
    let trigger = Arc::clone(&cancel);
    let count = Arc::clone(&peer.count);
    let sender = thread::spawn(move || {
        let end = Instant::now() + Duration::from_secs(2);
        while count.load(Ordering::SeqCst) == 0 && Instant::now() < end {
            thread::sleep(Duration::from_millis(2));
        }
        trigger.store(true, Ordering::SeqCst);
    });
    let result = Deadline::run_bounded(&deadline(), cancel, || {
        receive_from(&peer, &p, &Limits::default(), &Deadline::new(5000))
    });
    assert!(result.is_err());
    sender.join().unwrap();
    assert_eq!(peer.count.load(Ordering::SeqCst), 1);
}

#[test]
fn reflection_scan_is_linear_bounded_and_cannot_expose_a_match_position() {
    assert!(reflected(b"owned matching secret", b"matching", &deadline()).unwrap());
    assert!(!reflected(b"owned matching secret", b"different", &deadline()).unwrap());
    let body = vec![b'a'; 1_000_000];
    let mut secret = vec![b'a'; MAX_TOKEN];
    secret[MAX_TOKEN - 1] = b'b';
    assert!(!reflected(&body, &secret, &deadline()).unwrap());
    assert!(reflected(&body, &secret, &Deadline::new(0)).is_err());
}

#[test]
fn https_never_sends_private_headers_to_an_owned_plaintext_peer() {
    let root = tempfile::tempdir().unwrap();
    let p = prepared(root.path());
    let peer = Peer::new(
        reply("200 OK", "text/markdown", b"untrusted plaintext", ""),
        false,
    );
    let mut url = peer.url(&p);
    url.set_scheme("https").unwrap();
    let error = receive(
        &p,
        peer.address,
        &HostAccessToken::from_host_input(TOKEN.as_bytes()).unwrap(),
        &Limits::default(),
        &Deadline::new(1000),
        &url,
    )
    .err()
    .unwrap();
    assert_eq!(error.to_string(), "google_workspace_http_transport_failed");
    assert_eq!(peer.count.load(Ordering::SeqCst), 1);
    let wire = peer.wire();
    assert!(!wire.windows(TOKEN.len()).any(|w| w == TOKEN.as_bytes()));
    assert!(!wire.windows(KEY.len()).any(|w| w == KEY.as_bytes()));
    assert!(!wire.starts_with(b"GET "));
}
