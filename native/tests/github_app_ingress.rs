//! Real owned loopback sockets; no real GitHub, OS credential or project execution.
use hmac::{Hmac, Mac};
use serde_json::{Value, json};
use std::{
    io::{Read, Write},
    net::{SocketAddr, TcpStream},
    time::{Duration, Instant},
};
use yoke::{
    Engine,
    capture::Deadline,
    github_app::{
        self as app,
        ingress::{self, Config, ServerHandle},
    },
};

const KEY: &[u8] = b"public deterministic owned ingress fixture key";
fn host() -> app::Host {
    app::Host::with_signing_key(
        app::HostProfile::parse(
            &serde_json::to_vec(&json!({
        "schemaVersion":1,"appId":123,"secretCredential":"never-read-by-fixture",
        "grants":[{"installationId":42,"repositoryId":9,"repository":"acme/demo"}]}))
            .unwrap(),
        )
        .unwrap(),
        app::SigningKey::from_host_bytes(KEY).unwrap(),
    )
    .unwrap()
}
fn options() -> app::Options {
    app::Options {
        allow_writes: true,
        ..Default::default()
    }
}
fn config() -> Config {
    Config {
        allow_webhooks: true,
        ..Default::default()
    }
}
fn payload() -> Vec<u8> {
    let mut raw=serde_json::to_vec_pretty(&json!({"action":"opened","number":7,"installation":{"id":42},
        "repository":{"id":9,"full_name":"Acme/Demo"},"pull_request":{"number":7,"state":"open","draft":false,
        "title":"Grüße 🌍","head":{"sha":"a".repeat(40)},
        "base":{"sha":"b".repeat(40),"repo":{"id":9,"full_name":"acme/demo"}}}})).unwrap();
    raw.extend_from_slice(b"\r\n");
    raw
}
fn signature(raw: &[u8]) -> String {
    let mut mac = Hmac::<sha2::Sha256>::new_from_slice(KEY).unwrap();
    mac.update(raw);
    format!("sha256={}", hex::encode(mac.finalize().into_bytes()))
}
fn request(address: SocketAddr, raw: &[u8], event: &str, n: u32) -> Vec<u8> {
    let mut data=format!("POST /github/webhook HTTP/1.1\r\nHost: {address}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nX-GitHub-Event: {event}\r\nX-GitHub-Delivery: 01234567-89ab-4cde-8000-{n:012x}\r\nX-Hub-Signature-256: {}\r\nConnection: close\r\n\r\n",raw.len(),signature(raw)).into_bytes();
    data.extend_from_slice(raw);
    data
}
fn exchange(address: SocketAddr, raw: &[u8]) -> Vec<u8> {
    let started = Instant::now();
    let mut socket = TcpStream::connect(address).unwrap();
    socket
        .set_read_timeout(Some(Duration::from_secs(3)))
        .unwrap();
    socket
        .set_write_timeout(Some(Duration::from_secs(3)))
        .unwrap();
    socket.write_all(raw).unwrap();
    let mut result = Vec::new();
    socket.read_to_end(&mut result).unwrap_or_else(|error| {
        let header_end = result
            .windows(4)
            .position(|bytes| bytes == b"\r\n\r\n")
            .map(|offset| offset + 4);
        panic!(
            "owned ingress response read failed; kind={:?}; os_error={:?}; elapsed={:?}; retained_bytes={}; has_503_status={}; header_end={header_end:?}",
            error.kind(),
            error.raw_os_error(),
            started.elapsed(),
            result.len(),
            result.starts_with(b"HTTP/1.1 503 ")
        )
    });
    result
}
fn status(raw: &[u8]) -> u16 {
    let line = std::str::from_utf8(raw)
        .unwrap()
        .lines()
        .next()
        .expect("HTTP response");
    line.split_whitespace().nth(1).unwrap().parse().unwrap()
}
fn response(raw: &[u8]) -> Value {
    let split = raw.windows(4).position(|b| b == b"\r\n\r\n").unwrap() + 4;
    serde_json::from_slice(&raw[split..]).unwrap()
}
struct Fixture {
    root: tempfile::TempDir,
    handle: Option<ServerHandle>,
}
impl Fixture {
    fn new(c: Config, o: app::Options) -> Self {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("untouched.rs"), b"// caf\xc3\xa9\r\n").unwrap();
        let engine = Engine::open(root.path()).unwrap();
        let handle = ingress::start(engine, host(), o, c, &Deadline::new(10000)).unwrap();
        Self {
            root,
            handle: Some(handle),
        }
    }
    fn address(&self) -> SocketAddr {
        self.handle.as_ref().unwrap().local_addr()
    }
    fn stop(&mut self) {
        self.handle.take().unwrap().shutdown().unwrap();
    }
    fn records(&self) -> Vec<Value> {
        let engine = Engine::open(self.root.path()).unwrap();
        let mut query = engine
            .store
            .db
            .prepare("SELECT value FROM records WHERE kind=?1 ORDER BY rowid")
            .unwrap();
        query
            .query_map([app::RECORD_KIND], |r| r.get::<_, String>(0))
            .unwrap()
            .map(|r| serde_json::from_str(&r.unwrap()).unwrap())
            .collect()
    }
}
#[test]
fn permission_and_zero_service_limits_refuse_before_listener() {
    for field in 0..6 {
        let root = tempfile::tempdir().unwrap();
        let mut c = config();
        let mut o = options();
        let reserved = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        c.listen = reserved.local_addr().unwrap();
        match field {
            0 => c.allow_webhooks = false,
            1 => o.allow_writes = false,
            2 => c.max_connections = 0,
            3 => c.max_requests = 0,
            4 => c.lifetime_ms = 0,
            _ => c.max_response_bytes = 0,
        }
        let error = ingress::start(
            Engine::open(root.path()).unwrap(),
            host(),
            o,
            c,
            &Deadline::new(10000),
        )
        .err()
        .expect("permission/capacity must refuse");
        assert_eq!(
            error.to_string(),
            if field <= 1 {
                "github_app_permission_required"
            } else {
                "github_app_ingress_capacity_exceeded"
            }
        );
        let engine = Engine::open(root.path()).unwrap();
        assert_eq!(
            engine
                .store
                .db
                .query_row(
                    "SELECT count(*) FROM records WHERE kind=?1",
                    [app::RECORD_KIND],
                    |r| r.get::<_, u64>(0)
                )
                .unwrap(),
            0
        );
    }
}
#[test]
fn plaintext_remote_bind_cannot_be_granted_without_explicit_tls() {
    let root = tempfile::tempdir().unwrap();
    let mut c = config();
    c.listen = "0.0.0.0:0".parse().unwrap();
    c.allow_remote = true;
    assert!(
        ingress::start(
            Engine::open(root.path()).unwrap(),
            host(),
            options(),
            c,
            &Deadline::new(10000)
        )
        .is_err()
    );
}
#[test]
fn accepted_response_means_exact_raw_delivery_is_already_durable() {
    let mut f = Fixture::new(config(), options());
    let raw = payload();
    let address = f.address();
    let wire = exchange(f.address(), &request(f.address(), &raw, "pull_request", 1));
    assert_eq!(status(&wire), 202);
    assert_eq!(response(&wire)["durable"], true);
    let records = f.records();
    assert_eq!(records.len(), 1);
    assert_eq!(
        records[0]["event"]["payload"].as_str().unwrap().as_bytes(),
        raw
    );
    assert_eq!(
        std::fs::read(f.root.path().join("untouched.rs")).unwrap(),
        b"// caf\xc3\xa9\r\n"
    );
    assert_eq!(response(&wire)["sourceWritten"], false);
    assert_eq!(response(&wire)["published"], false);
    assert!(!String::from_utf8_lossy(&wire).contains(std::str::from_utf8(KEY).unwrap()));
    f.stop();
    assert!(TcpStream::connect(address).is_err());
}
#[test]
fn redelivery_is_idempotent_and_conflicting_body_cannot_overwrite_history() {
    let mut f = Fixture::new(config(), options());
    let raw = payload();
    let bytes = request(f.address(), &raw, "pull_request", 1);
    assert_eq!(status(&exchange(f.address(), &bytes)), 202);
    assert_eq!(status(&exchange(f.address(), &bytes)), 202);
    assert_eq!(f.records().len(), 1);
    let mut changed: Value = serde_json::from_slice(&raw).unwrap();
    changed["pull_request"]["title"] = json!("different authenticated title");
    let changed = serde_json::to_vec(&changed).unwrap();
    assert_eq!(
        status(&exchange(
            f.address(),
            &request(f.address(), &changed, "pull_request", 1)
        )),
        409
    );
    assert_eq!(f.records().len(), 1);
    f.stop();
}
#[test]
fn raw_byte_mutation_and_wrong_repository_refuse_without_state() {
    let mut f = Fixture::new(config(), options());
    let raw = payload();
    let mut changed = request(f.address(), &raw, "pull_request", 1);
    changed.push(b' ');
    assert_ne!(status(&exchange(f.address(), &changed)), 202);
    let mut altered = raw.clone();
    let at = altered.iter().position(|b| *b == b'a').unwrap();
    altered[at] = b'z';
    let original = signature(&raw);
    let wire = request(f.address(), &altered, "pull_request", 2);
    let wire = String::from_utf8(wire)
        .unwrap()
        .replace(&signature(&altered), &original);
    assert_eq!(status(&exchange(f.address(), wire.as_bytes())), 401);
    let mut wrong: Value = serde_json::from_slice(&raw).unwrap();
    wrong["installation"]["id"] = json!(43);
    let wrong = serde_json::to_vec(&wrong).unwrap();
    assert_ne!(
        status(&exchange(
            f.address(),
            &request(f.address(), &wrong, "pull_request", 3)
        )),
        202
    );
    assert!(f.records().is_empty());
    f.stop();
}
#[test]
fn hostile_http_framing_headers_and_encoding_never_reach_queue() {
    let mut f = Fixture::new(config(), options());
    let raw = payload();
    let original = String::from_utf8(request(f.address(), &raw, "pull_request", 1)).unwrap();
    let host = format!("Host: {}\r\n", f.address());
    for wire in [
        original.replace("POST ", "GET "),
        original.replace("/github/webhook ", "/github/webhook?x=1 "),
        original.replace(&host, &format!("{host}{host}")),
        original.replace(&host, "Host: evil.example\r\n"),
        original.replace(
            "Content-Length:",
            &format!("Content-Length: {}\r\nContent-Length:", raw.len()),
        ),
        original.replace("Content-Type: application/json", "Content-Type: text/plain"),
        original.replace(
            "Connection: close",
            "Transfer-Encoding: chunked\r\nConnection: close",
        ),
        original.replace(
            "Connection: close",
            "Content-Encoding: gzip\r\nConnection: close",
        ),
        original.replace(
            "Connection: close",
            "Expect: 100-continue\r\nConnection: close",
        ),
        original.replace("\r\n", "\n"),
        original.replace(
            "X-GitHub-Event: pull_request",
            "X-GitHub-Event: pull_request\r\nx-github-event: pull_request",
        ),
        format!("{original}GET / HTTP/1.1\r\nHost: {}\r\n\r\n", f.address()),
    ] {
        assert_ne!(status(&exchange(f.address(), wire.as_bytes())), 202);
    }
    assert!(f.records().is_empty());
    f.stop();
}
#[test]
fn signed_ping_is_acknowledged_without_creating_a_review_job() {
    let mut f = Fixture::new(config(), options());
    let raw = br#"{"zen":"owned fixture","hook_id":1}"#;
    let wire = exchange(f.address(), &request(f.address(), raw, "ping", 1));
    assert_eq!(status(&wire), 200);
    assert_eq!(response(&wire)["enqueued"], false);
    assert_eq!(response(&wire)["authenticated"], true);
    assert!(f.records().is_empty());
    f.stop();
}
#[test]
fn owned_shutdown_interrupts_stalled_body_and_closes_the_actual_listener() {
    let mut f = Fixture::new(config(), options());
    let address = f.address();
    let raw = payload();
    let request = request(address, &raw, "pull_request", 1);
    let end = request.windows(4).position(|b| b == b"\r\n\r\n").unwrap() + 4;
    let mut stream = TcpStream::connect(address).unwrap();
    stream.write_all(&request[..end + 1]).unwrap();
    std::thread::sleep(Duration::from_millis(100));
    let start = Instant::now();
    f.stop();
    assert!(start.elapsed() < Duration::from_secs(1));
    assert!(TcpStream::connect(address).is_err());
    assert!(f.records().is_empty());
}
#[test]
fn body_budget_refuses_before_durable_admission() {
    let raw = payload();
    let mut o = options();
    o.max_payload_bytes = raw.len() - 1;
    let mut f = Fixture::new(config(), o);
    assert_eq!(
        status(&exchange(
            f.address(),
            &request(f.address(), &raw, "pull_request", 1)
        )),
        413
    );
    assert!(f.records().is_empty());
    f.stop();
}
#[test]
fn connection_capacity_does_not_queue_unbounded_stalled_workers() {
    let mut c = config();
    c.max_connections = 1;
    let mut f = Fixture::new(c, options());
    let address = f.address();
    let raw = payload();
    let first = request(address, &raw, "pull_request", 1);
    let end = first.windows(4).position(|b| b == b"\r\n\r\n").unwrap() + 4;
    let mut stalled = TcpStream::connect(address).unwrap();
    stalled.write_all(&first[..end + 1]).unwrap();
    std::thread::sleep(Duration::from_millis(100));
    let now = Instant::now();
    assert_eq!(
        status(&exchange(
            address,
            &request(address, &raw, "pull_request", 2)
        )),
        503
    );
    assert!(now.elapsed() < Duration::from_millis(750));
    assert!(f.records().is_empty());
    f.stop();
}
#[test]
fn host_selected_tls_accepts_trusted_server_certificate_and_client_refuses_untrusted_certificate() {
    if rustls::crypto::CryptoProvider::get_default().is_none() {
        let _ = rustls::crypto::ring::default_provider().install_default();
    }
    let generated = rcgen::generate_simple_self_signed(vec!["127.0.0.1".into()]).unwrap();
    let der = generated.cert.der().to_vec();
    let mut c = config();
    c.server_names = vec!["127.0.0.1".into()];
    c.tls = Some(yoke::federation_http::TlsMaterial {
        certificates: vec![der.clone()],
        private_key: zeroize::Zeroizing::new(generated.key_pair.serialize_der()),
    });
    let mut f = Fixture::new(c, options());
    let url = format!("https://{}/github/webhook", f.address());
    let raw = payload();
    let untrusted = reqwest::blocking::Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(2))
        .build()
        .unwrap();
    assert!(untrusted.post(&url).body(raw.clone()).send().is_err());
    assert!(f.records().is_empty());
    let client = reqwest::blocking::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(3))
        .add_root_certificate(reqwest::Certificate::from_der(&der).unwrap())
        .build()
        .unwrap();
    let reply = client
        .post(url)
        .header("Content-Type", "application/json")
        .header("X-GitHub-Event", "pull_request")
        .header("X-GitHub-Delivery", "01234567-89ab-4cde-8000-000000000001")
        .header("X-Hub-Signature-256", signature(&raw))
        .body(raw)
        .send()
        .unwrap();
    assert_eq!(reply.status().as_u16(), 202);
    assert_eq!(f.records().len(), 1);
    f.stop();
}
fn wait_closed(address: SocketAddr) {
    let started = Instant::now();
    loop {
        if TcpStream::connect_timeout(&address, Duration::from_millis(50)).is_err() {
            return;
        }
        assert!(
            started.elapsed() < Duration::from_millis(750),
            "listener remained open without an explicit shutdown request"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
}
#[test]
fn failed_sql_commit_cannot_produce_an_accepted_http_response() {
    use rusqlite::hooks::{AuthAction, AuthContext, Authorization};
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };
    let root = tempfile::tempdir().unwrap();
    let engine = Engine::open(root.path()).unwrap();
    let inserted = Arc::new(AtomicBool::new(false));
    let denied = Arc::new(AtomicBool::new(false));
    let observed = inserted.clone();
    engine
        .store
        .db
        .authorizer(Some(move |context: AuthContext<'_>| {
            if matches!(
                context.action,
                AuthAction::Insert {
                    table_name: "records"
                }
            ) {
                observed.store(true, Ordering::SeqCst);
            }
            Authorization::Allow
        }));
    let observed = denied.clone();
    engine.store.db.commit_hook(Some(move || {
        observed.store(true, Ordering::SeqCst);
        true
    }));
    let handle =
        ingress::start(engine, host(), options(), config(), &Deadline::new(10000)).unwrap();
    let mut f = Fixture {
        root,
        handle: Some(handle),
    };
    let raw = payload();
    let wire = exchange(f.address(), &request(f.address(), &raw, "pull_request", 1));
    assert_ne!(status(&wire), 202);
    assert!(inserted.load(Ordering::SeqCst));
    assert!(denied.load(Ordering::SeqCst));
    assert!(f.records().is_empty());
    f.stop();
}
#[test]
fn owned_shutdown_bounds_a_request_observed_waiting_for_an_external_sqlite_writer() {
    use rusqlite::hooks::{AuthAction, AuthContext, Authorization, TransactionOperation};
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };
    let root = tempfile::tempdir().unwrap();
    let engine = Engine::open(root.path()).unwrap();
    let blocker = rusqlite::Connection::open(engine.store.dir.join("index.sqlite")).unwrap();
    // Verify the independent connection targets this Store, rather than a new database.
    assert!(
        blocker
            .query_row("SELECT count(*) FROM records", [], |r| r.get::<_, u64>(0))
            .is_ok()
    );
    blocker.execute_batch("BEGIN IMMEDIATE").unwrap();
    let began = Arc::new(AtomicBool::new(false));
    let observed = began.clone();
    engine
        .store
        .db
        .authorizer(Some(move |context: AuthContext<'_>| {
            if matches!(
                context.action,
                AuthAction::Transaction {
                    operation: TransactionOperation::Begin
                }
            ) {
                observed.store(true, Ordering::SeqCst);
            }
            Authorization::Allow
        }));
    let handle =
        ingress::start(engine, host(), options(), config(), &Deadline::new(10000)).unwrap();
    let address = handle.local_addr();
    let mut socket = TcpStream::connect(address).unwrap();
    socket
        .set_read_timeout(Some(Duration::from_secs(7)))
        .unwrap();
    socket
        .write_all(&request(address, &payload(), "pull_request", 1))
        .unwrap();
    let observed_until = Instant::now();
    while !began.load(Ordering::SeqCst) {
        assert!(
            observed_until.elapsed() < Duration::from_secs(2),
            "request never reached the real SQL transaction"
        );
        std::thread::sleep(Duration::from_millis(1));
    }
    // Keep the external write lock held through shutdown and the wire assertions.
    let started = Instant::now();
    let report = handle.shutdown().unwrap();
    let elapsed = started.elapsed();
    let mut wire = Vec::new();
    socket.read_to_end(&mut wire).unwrap();
    assert!(!wire.starts_with(b"HTTP/1.1 202 "));
    assert_eq!(report["durableAdmissions"], 0);
    assert_eq!(report["workersJoined"], true);
    assert_eq!(
        blocker
            .query_row(
                "SELECT count(*) FROM records WHERE kind=?1",
                [app::RECORD_KIND],
                |r| r.get::<_, u64>(0)
            )
            .unwrap(),
        0
    );
    blocker.execute_batch("ROLLBACK").unwrap();
    assert!(
        elapsed < Duration::from_millis(750),
        "shutdown waited {elapsed:?} for an external SQL lock"
    );
    assert!(TcpStream::connect(address).is_err());
}
#[test]
fn insufficient_positive_wire_budget_prevents_a_valid_delivery_from_mutating_queue() {
    let mut c = config();
    c.max_response_bytes = 1;
    let mut f = Fixture::new(c, options());
    let raw = payload();
    let wire = exchange(f.address(), &request(f.address(), &raw, "pull_request", 1));
    assert!(!wire.starts_with(b"HTTP/1.1 202 "));
    assert!(wire.len() <= 1);
    assert!(f.records().is_empty());
    f.stop();
}
#[test]
fn declared_service_lifetime_closes_listener_before_explicit_shutdown() {
    let mut c = config();
    c.lifetime_ms = 40;
    let mut f = Fixture::new(c, options());
    wait_closed(f.address());
    f.stop();
    assert!(f.records().is_empty());
}
#[test]
fn inherited_host_cancellation_retires_listener_before_explicit_shutdown() {
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };
    let root = tempfile::tempdir().unwrap();
    let engine = Engine::open(root.path()).unwrap();
    let cancel = Arc::new(AtomicBool::new(false));
    let handle = Deadline::run_bounded(&Deadline::new(10000), cancel.clone(), || {
        ingress::start(engine, host(), options(), config(), &Deadline::new(10000))
    })
    .unwrap();
    let mut f = Fixture {
        root,
        handle: Some(handle),
    };
    cancel.store(true, Ordering::SeqCst);
    wait_closed(f.address());
    f.stop();
    assert!(f.records().is_empty());
}
fn wait_for_partial_header_tick(
    started: Instant,
    acceptance_bound: Duration,
    stopped: &std::sync::atomic::AtomicBool,
) -> bool {
    // A coalesced 25 ms sleep can return after the entire 120 ms request has
    // closed. Pace this small active peer by the monotonic clock instead. The
    // original shared acceptance ceiling bounds all spinning across attempts;
    // cancellation is observed while waiting, without a new timer or thread.
    let next = started.elapsed().saturating_add(Duration::from_millis(25));
    loop {
        if stopped.load(std::sync::atomic::Ordering::SeqCst) {
            return false;
        }
        let elapsed = started.elapsed();
        if elapsed >= acceptance_bound {
            return false;
        }
        if elapsed >= next {
            return true;
        }
        std::hint::spin_loop();
    }
}

#[test]
fn partial_header_pacing_refuses_stopped_and_expired_write_opportunities() {
    use std::sync::atomic::AtomicBool;
    let stopped = AtomicBool::new(true);
    assert!(!wait_for_partial_header_tick(
        Instant::now(),
        Duration::from_millis(750),
        &stopped,
    ));
    let running = AtomicBool::new(false);
    assert!(!wait_for_partial_header_tick(
        Instant::now().checked_sub(Duration::from_secs(1)).unwrap(),
        Duration::from_millis(750),
        &running,
    ));
    assert!(!wait_for_partial_header_tick(
        Instant::now(),
        Duration::ZERO,
        &running,
    ));
}

fn continuously_paced_partial_headers(
    successes: &[Option<Duration>; 60],
    connected_at: Duration,
    closed_at: Duration,
    maximum_gap: Duration,
) -> bool {
    let mut previous = connected_at;
    let mut count = 0;
    for &success in successes.iter().flatten() {
        if success > closed_at {
            break;
        }
        let Some(gap) = success.checked_sub(previous) else {
            return false;
        };
        if gap >= maximum_gap {
            return false;
        }
        previous = success;
        count += 1;
    }
    count >= 2
        && closed_at
            .checked_sub(previous)
            .is_some_and(|gap| gap < maximum_gap)
}

#[test]
fn partial_header_stimulus_requires_live_pacing_through_peer_close() {
    let samples = |times: &[u64]| {
        let mut result = [None; 60];
        for (slot, micros) in result.iter_mut().zip(times) {
            *slot = Some(Duration::from_micros(*micros));
        }
        result
    };
    let closed_at = Duration::from_micros(134_255);
    let gap = Duration::from_millis(60);
    assert!(continuously_paced_partial_headers(
        &samples(&[214, 25_000, 50_000, 75_000, 100_000]),
        Duration::from_micros(105),
        closed_at,
        gap,
    ));
    // Retained Darwin trace: the first 25 ms sleep woke at 168.9 ms, after
    // the peer had already closed at 134.3 ms. It proves no continuous peer.
    for times in [
        vec![],
        vec![214],
        vec![214, 168_899],
        vec![214, 25_000, 50_000],
        vec![214, 25_000, 100_000],
        vec![214, 25_000, 85_000, 100_000],
        vec![214, 50_000, 25_000, 100_000],
        vec![70_000, 100_000],
    ] {
        assert!(
            !continuously_paced_partial_headers(
                &samples(&times),
                Duration::from_micros(105),
                closed_at,
                gap,
            ),
            "unproved partial-header pacing: {times:?}"
        );
    }
}

#[test]
fn absolute_request_deadline_stops_a_peer_which_keeps_sending_partial_headers() {
    use std::sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    };
    let request_timeout = Duration::from_millis(120);
    let acceptance_bound = Duration::from_millis(750);
    let mut c = config();
    c.request_timeout_ms = 120;
    let mut f = Fixture::new(c, options());
    // One total acceptance timer covers every connection, writer and join.
    // It starts earlier than the old single-attempt timer and never resets.
    let now = Instant::now();
    let observed_from = now;
    let mut attempts = Vec::with_capacity(3);
    let mut proved = false;
    for attempt in 0..3 {
        let mut read = TcpStream::connect(f.address()).unwrap();
        let connected_at = observed_from.elapsed();
        let remaining = acceptance_bound.saturating_sub(now.elapsed());
        assert!(
            !remaining.is_zero(),
            "acceptance budget exhausted: {attempts:?}"
        );
        read.set_read_timeout(Some(remaining)).unwrap();
        let mut write = read.try_clone().unwrap();
        let stop = Arc::new(AtomicBool::new(false));
        let stopping = stop.clone();
        let sent = Arc::new(AtomicUsize::new(0));
        let count = sent.clone();
        let spawn_started_at = observed_from.elapsed();
        let drip = std::thread::spawn(move || {
            // Fixed thread-owned observations; no observer lock or I/O.
            let entered_at = observed_from.elapsed();
            let mut first_two_writes = [(None, None, None); 2];
            let mut successes = [None; 60];
            let mut write_error = None;
            let mut stop_observed = None;
            let mut last_wake_at = None;
            let mut pacing_stopped = None;
            for (write_attempt, success_slot) in successes.iter_mut().enumerate() {
                if stopping.load(Ordering::SeqCst) {
                    stop_observed = Some((write_attempt + 1, observed_from.elapsed()));
                    break;
                }
                if let Some((attempt_at, _, _)) = first_two_writes.get_mut(write_attempt) {
                    *attempt_at = Some(observed_from.elapsed());
                }
                if let Err(error) = write.write_all(b"P") {
                    write_error = Some((write_attempt + 1, observed_from.elapsed(), error.kind()));
                    break;
                }
                count.fetch_add(1, Ordering::SeqCst);
                let success_at = observed_from.elapsed();
                *success_slot = Some(success_at);
                if let Some((_, first_success_at, _)) = first_two_writes.get_mut(write_attempt) {
                    *first_success_at = Some(success_at);
                }
                let next_write_due =
                    wait_for_partial_header_tick(observed_from, acceptance_bound, &stopping);
                let woke_at = observed_from.elapsed();
                last_wake_at = Some(woke_at);
                if let Some((_, _, wake)) = first_two_writes.get_mut(write_attempt) {
                    *wake = Some(woke_at);
                }
                if !next_write_due {
                    pacing_stopped = Some((woke_at, stopping.load(Ordering::SeqCst)));
                    break;
                }
            }
            (
                successes,
                (
                    ("entry", entered_at),
                    ("first-two-attempt-success-wake", first_two_writes),
                    ("write-error-attempt-time-kind", write_error),
                    ("stop-attempt-time", stop_observed),
                    ("last-wake", last_wake_at),
                    ("pacing-stop-time-and-cancelled", pacing_stopped),
                    ("return", observed_from.elapsed()),
                ),
            )
        });
        let mut wire = Vec::new();
        let read_started_at = observed_from.elapsed();
        let read_outcome = read.read_to_end(&mut wire).map_err(|error| error.kind());
        let read_completed_at = observed_from.elapsed();
        stop.store(true, Ordering::SeqCst);
        let stop_requested_at = observed_from.elapsed();
        let join_started_at = observed_from.elapsed();
        let joined = drip.join();
        let join_completed_at = observed_from.elapsed();
        let client_phases = [
            ("connect-return", connected_at),
            ("spawn-entry", spawn_started_at),
            ("read-entry", read_started_at),
            ("read-return", read_completed_at),
            ("stop-request", stop_requested_at),
            ("join-entry", join_started_at),
            ("join-return", join_completed_at),
        ];
        assert!(
            joined.is_ok(),
            "writer panicked; elapsed={:?}; response_bytes={}; read_outcome={read_outcome:?}; client_phases={client_phases:?}; attempts={attempts:?}",
            now.elapsed(),
            wire.len()
        );
        let (successes, writer) = joined.unwrap();
        let sent = sent.load(Ordering::SeqCst);
        let stimulated = continuously_paced_partial_headers(
            &successes,
            connected_at,
            read_completed_at,
            request_timeout / 2,
        );
        let clean_close = read_outcome == Ok(0) && wire.is_empty();
        let request_age = read_completed_at.saturating_sub(connected_at);
        attempts.push((
            sent,
            read_outcome,
            wire.len(),
            client_phases,
            writer,
            successes,
        ));
        // Retry only a clean, non-early close whose writer failed to provide
        // continuous traffic. No functional failure or exhausted wall budget
        // can retry. At most one writer exists at a time; all joins count.
        if !stimulated
            && clean_close
            && request_age >= request_timeout
            && attempt < 2
            && now.elapsed() + request_timeout < acceptance_bound
        {
            continue;
        }
        assert!(clean_close, "partial-header close failed: {attempts:?}");
        assert!(sent >= 2, "insufficient writes: {attempts:?}");
        assert!(
            stimulated,
            "continuous peer was not established: {attempts:?}"
        );
        assert!(
            now.elapsed() < acceptance_bound,
            "elapsed={:?}; attempts={attempts:?}",
            now.elapsed()
        );
        proved = true;
        break;
    }
    assert!(proved, "no qualified partial-header attempt: {attempts:?}");
    assert!(
        f.records().is_empty(),
        "unexpected durable ingress: {attempts:?}"
    );
    f.stop();
}
#[test]
fn unsigned_event_label_cannot_turn_pr_body_into_ping_or_ping_into_pr() {
    let mut f = Fixture::new(config(), options());
    let raw = payload();
    assert_ne!(
        status(&exchange(
            f.address(),
            &request(f.address(), &raw, "ping", 1)
        )),
        200
    );
    let ping = br#"{"zen":"fixture","hook_id":1}"#;
    assert_ne!(
        status(&exchange(
            f.address(),
            &request(f.address(), ping, "pull_request", 2)
        )),
        202
    );
    let mut hybrid: Value = serde_json::from_slice(&raw).unwrap();
    hybrid["zen"] = json!("fixture");
    hybrid["hook_id"] = json!(1);
    let hybrid = serde_json::to_vec(&hybrid).unwrap();
    assert_ne!(
        status(&exchange(
            f.address(),
            &request(f.address(), &hybrid, "ping", 3)
        )),
        200
    );
    assert!(f.records().is_empty());
    f.stop();
}
#[test]
fn configured_tls_server_name_and_http_host_are_both_required() {
    if rustls::crypto::CryptoProvider::get_default().is_none() {
        let _ = rustls::crypto::ring::default_provider().install_default();
    }
    let generated =
        rcgen::generate_simple_self_signed(vec!["allowed.fixture".into(), "other.fixture".into()])
            .unwrap();
    let der = generated.cert.der().to_vec();
    let mut c = config();
    c.server_names = vec!["allowed.fixture".into()];
    c.tls = Some(yoke::federation_http::TlsMaterial {
        certificates: vec![der.clone()],
        private_key: zeroize::Zeroizing::new(generated.key_pair.serialize_der()),
    });
    let mut f = Fixture::new(c, options());
    let address = f.address();
    let raw = payload();
    let client = reqwest::blocking::Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(3))
        .resolve("allowed.fixture", address)
        .resolve("other.fixture", address)
        .add_root_certificate(reqwest::Certificate::from_der(&der).unwrap())
        .build()
        .unwrap();
    let send = |name: &str, host: &str| {
        client
            .post(format!("https://{name}:{}/github/webhook", address.port()))
            .header("Host", format!("{host}:{}", address.port()))
            .header("Content-Type", "application/json")
            .header("X-GitHub-Event", "pull_request")
            .header("X-GitHub-Delivery", "01234567-89ab-4cde-8000-000000000001")
            .header("X-Hub-Signature-256", signature(&raw))
            .body(raw.clone())
            .send()
            .unwrap()
    };
    assert_eq!(
        send("other.fixture", "allowed.fixture").status().as_u16(),
        403
    );
    assert_eq!(
        send("allowed.fixture", "other.fixture").status().as_u16(),
        403
    );
    assert!(f.records().is_empty());
    assert_eq!(
        send("allowed.fixture", "allowed.fixture").status().as_u16(),
        202
    );
    assert_eq!(f.records().len(), 1);
    f.stop();
}
#[test]
fn owned_shutdown_also_interrupts_a_stalled_tls_handshake() {
    let generated = rcgen::generate_simple_self_signed(vec!["127.0.0.1".into()]).unwrap();
    let mut c = config();
    c.server_names = vec!["127.0.0.1".into()];
    c.tls = Some(yoke::federation_http::TlsMaterial {
        certificates: vec![generated.cert.der().to_vec()],
        private_key: zeroize::Zeroizing::new(generated.key_pair.serialize_der()),
    });
    let mut f = Fixture::new(c, options());
    let address = f.address();
    let _socket = TcpStream::connect(address).unwrap();
    std::thread::sleep(Duration::from_millis(100));
    assert!(
        std::net::TcpListener::bind(address).is_err(),
        "active service must own the exact listening address"
    );
    let now = Instant::now();
    let report = f.handle.take().unwrap().shutdown().unwrap();
    assert!(now.elapsed() < Duration::from_secs(1));
    assert_eq!(report["workersJoined"], true);
    assert_eq!(report["listenerClosed"], true);
    // Hold the released address throughout verification so another fixture cannot reuse it.
    let _released = std::net::TcpListener::bind(address)
        .expect("shutdown must release its exact listening address");
    assert!(f.records().is_empty());
}
