//! Native owned TCP contracts; no static helper or external service.
use super::*;
use std::{net::TcpStream, time::Instant};
use yoke::github_app::ingress;

fn store_rows(store: &yoke::store::Store) -> Vec<(String, String, String)> {
    store
        .db
        .prepare("SELECT kind,id,value FROM records ORDER BY kind,id")
        .unwrap()
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
        .unwrap()
        .collect::<rusqlite::Result<Vec<_>>>()
        .unwrap()
}
fn blob_hashes(store: &yoke::store::Store) -> std::collections::BTreeMap<String, String> {
    std::fs::read_dir(store.dir.join("blobs"))
        .unwrap()
        .map(|r| {
            let p = r.unwrap().path();
            (
                p.file_name().unwrap().to_str().unwrap().into(),
                yoke::model::hash(std::fs::read(p).unwrap()),
            )
        })
        .collect()
}
struct Prepared {
    engine: Engine,
    host: app::Host,
    clock: Arc<worker::Clock>,
    time: Arc<AtomicU64>,
    queue: app::Options,
    page_options: pages::Options,
    root: tempfile::TempDir,
    before: Vec<(String, String, String)>,
    blobs: std::collections::BTreeMap<String, String>,
    html: String,
    path: String,
    page_id: String,
    workspace: String,
}
impl Prepared {
    fn new() -> Self {
        let f = Fixture::new();
        let done = completed(&f);
        let mut o = options(&f);
        let p = preview(&f, &done, &o);
        pages::store_page(&context(&f, &o), &request(&p), &f.d).unwrap();
        let before = rows(&f);
        let blobs = blob_hashes(&f.engine.store);
        let workspace = f.engine.store.workspace_id.clone();
        let page_id = id(&p).to_owned();
        let html = p["html"].as_str().unwrap().to_owned();
        let path = format!("/p/{page_id}?t={}", token(&p));
        let mut queue = f.options.queue.clone();
        queue.allow_writes = false;
        o.worker.queue.allow_writes = false;
        let Fixture {
            engine,
            host,
            clock,
            time,
            root,
            ..
        } = f;
        Self {
            engine,
            host,
            clock: Arc::new(clock),
            time,
            queue,
            page_options: o,
            root,
            before,
            blobs,
            html,
            path,
            page_id,
            workspace,
        }
    }
    fn start(
        mut self,
        config: ingress::Config,
        allow_serving: bool,
        allow_pages: bool,
    ) -> anyhow::Result<Running> {
        self.page_options.allow_pages = allow_pages;
        let server = ingress::start_pages(
            self.engine,
            self.host,
            self.queue,
            config,
            ingress::PageService {
                allow_serving,
                options: self.page_options,
                clock: self.clock,
            },
            &Deadline::new(300_000),
        )?;
        Ok(Running {
            server: Some(server),
            root: self.root,
            time: self.time,
            before: self.before,
            blobs: self.blobs,
            html: self.html,
            path: self.path,
            page_id: self.page_id,
            workspace: self.workspace,
        })
    }
}
struct Running {
    // Drop the owned server before its temporary Store, including assertion failures.
    server: Option<ingress::ServerHandle>,
    root: tempfile::TempDir,
    time: Arc<AtomicU64>,
    before: Vec<(String, String, String)>,
    blobs: std::collections::BTreeMap<String, String>,
    html: String,
    path: String,
    page_id: String,
    workspace: String,
}
impl Running {
    fn address(&self) -> SocketAddr {
        self.server.as_ref().unwrap().local_addr()
    }
    fn request(&self, method: &str, path: &str, host: Option<&str>) -> Vec<u8> {
        let address = self.address();
        let mut stream = TcpStream::connect_timeout(&address, Duration::from_secs(1)).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        stream
            .set_write_timeout(Some(Duration::from_secs(1)))
            .unwrap();
        let authority = address.to_string();
        let host = host.unwrap_or(&authority);
        write!(stream,"{method} {path} HTTP/1.1\r\nHost: {host}\r\nConnection: close\r\nContent-Length: 0\r\n\r\n").unwrap();
        let mut response = Vec::new();
        let deadline = Instant::now() + Duration::from_secs(2);
        stream
            .set_read_timeout(Some(Duration::from_millis(50)))
            .unwrap();
        let mut chunk = [0u8; 8192];
        loop {
            assert!(
                Instant::now() < deadline,
                "owned response exceeded its absolute test deadline"
            );
            match stream.read(&mut chunk) {
                Ok(0) => break,
                Ok(n) => {
                    assert!(
                        n <= 4_065_536usize.saturating_sub(response.len()),
                        "owned response exceeded fixed wire cap"
                    );
                    response.extend_from_slice(&chunk[..n]);
                }
                Err(e)
                    if matches!(
                        e.kind(),
                        std::io::ErrorKind::TimedOut
                            | std::io::ErrorKind::WouldBlock
                            | std::io::ErrorKind::Interrupted
                    ) =>
                {
                    continue;
                }
                Err(_) => panic!("owned native response transport failed"),
            }
        }
        response
    }
    fn finish(mut self) -> Value {
        let summary = self.server.take().unwrap().shutdown().unwrap();
        assert_eq!(summary["listenerClosed"], true);
        assert_eq!(summary["workersJoined"], true);
        assert_eq!(summary["durableAdmissions"], 0);
        assert_eq!(summary["sourceWritten"], false);
        let reopened = Engine::open(self.root.path()).unwrap();
        assert_eq!(store_rows(&reopened.store), self.before);
        assert_eq!(blob_hashes(&reopened.store), self.blobs);
        assert_eq!(
            std::fs::read(self.root.path().join(PATH)).unwrap(),
            SOURCE.as_bytes()
        );
        summary
    }
}
fn config() -> ingress::Config {
    ingress::Config {
        allow_webhooks: false,
        max_response_bytes: 4_065_536,
        request_timeout_ms: 1000,
        lifetime_ms: 5000,
        max_requests: 50,
        ..Default::default()
    }
}
fn status(packet: &[u8]) -> u16 {
    std::str::from_utf8(packet)
        .unwrap()
        .split_whitespace()
        .nth(1)
        .unwrap()
        .parse()
        .unwrap()
}
fn body(packet: &[u8]) -> &[u8] {
    let end = packet.windows(4).position(|s| s == b"\r\n\r\n").unwrap() + 4;
    &packet[end..]
}
fn headers(packet: &[u8]) -> String {
    let end = packet.windows(4).position(|s| s == b"\r\n\r\n").unwrap();
    std::str::from_utf8(&packet[..end])
        .unwrap()
        .to_ascii_lowercase()
}
fn missing_token(workspace: &str, page_id: &str) -> String {
    let mut mac = Hmac::<sha2::Sha256>::new_from_slice(KEY).unwrap();
    mac.update(b"yoke-code-intelligence/github-app-page-access/v1\n");
    mac.update(
        &serde_json::to_vec(&json!({"workspaceId":workspace,"appId":123,"pageId":page_id}))
            .unwrap(),
    );
    hex::encode(mac.finalize().into_bytes())
}

#[test]
fn native_healthz_reports_only_listener_liveness_without_page_or_source_details() {
    let running = Prepared::new().start(config(), true, true).unwrap();
    let response = running.request("GET", "/healthz", None);
    assert_eq!(status(&response), 200);
    assert_eq!(
        serde_json::from_slice::<Value>(body(&response)).unwrap(),
        json!({"ok":true,"pageServing":true,"webhookServing":false,"complete":false})
    );
    assert_eq!(
        status(&running.request("GET", "/healthz", Some("untrusted.invalid"))),
        403
    );
    assert_eq!(status(&running.request("POST", "/healthz", None)), 405);
    assert!((400..500).contains(&status(&running.request(
        "GET",
        "/healthz?token=not-a-capability",
        None
    ))));
    assert_eq!(status(&running.request("GET", &running.path, None)), 200);
    running.finish();
}

#[test]
fn native_get_serves_exact_approved_html_with_complete_headers_and_no_queue_write_consent() {
    let running = Prepared::new().start(config(), true, true).unwrap();
    let response = running.request("GET", &running.path, None);
    assert_eq!(status(&response), 200);
    assert_eq!(body(&response), running.html.as_bytes());
    let h = headers(&response);
    for required in [
        "content-type: text/html; charset=utf-8",
        "cache-control: no-store",
        "referrer-policy: no-referrer",
        "x-content-type-options: nosniff",
        "content-security-policy:",
        "frame-ancestors 'none'",
    ] {
        assert!(
            h.contains(required),
            "missing required response header: {required}"
        );
    }
    let end = response.windows(4).position(|w| w == b"\r\n\r\n").unwrap();
    let raw_headers = std::str::from_utf8(&response[..end]).unwrap();
    let policies: Vec<_> = raw_headers
        .lines()
        .filter_map(|line| line.split_once(':'))
        .filter(|(name, _)| name.eq_ignore_ascii_case("content-security-policy"))
        .map(|(_, value)| value.trim())
        .collect();
    assert_eq!(policies.len(), 1);
    let approved_policy = running
        .html
        .split_once("http-equiv=\"Content-Security-Policy\" content=\"")
        .unwrap()
        .1
        .split_once('"')
        .unwrap()
        .0;
    assert_eq!(
        policies[0], approved_policy,
        "hash values are case-sensitive and policy must exactly match approved native HTML"
    );
    running.finish();
}

#[test]
fn native_missing_wrong_bearer_and_expired_pages_have_identical_404_packets() {
    let running = Prepared::new().start(config(), true, true).unwrap();
    let bad = running.request(
        "GET",
        &format!("/p/{}?t={}", running.page_id, "0".repeat(64)),
        None,
    );
    let absent_id = "f".repeat(64);
    let absent = running.request(
        "GET",
        &format!(
            "/p/{absent_id}?t={}",
            missing_token(&running.workspace, &absent_id)
        ),
        None,
    );
    assert_eq!(status(&bad), 404);
    assert_eq!(absent, bad);
    running.time.store(NOW + 60, Ordering::SeqCst);
    let expired = running.request("GET", &running.path, None);
    assert_eq!(expired, bad);
    running.finish();
}

#[test]
fn native_page_route_refuses_wrong_authority_encoded_paths_extra_queries_and_methods() {
    let running = Prepared::new().start(config(), true, true).unwrap();
    assert_eq!(
        status(&running.request("GET", &running.path, Some("untrusted.invalid"))),
        403
    );
    for path in [
        "/p/../../outside".to_owned(),
        "/p/%2e%2e%2foutside".into(),
        format!("{}&t={}", running.path, "a".repeat(64)),
        format!("{}&extra=1", running.path),
        format!("{}#fragment", running.path),
    ] {
        let response = running.request("GET", &path, None);
        assert!((400..500).contains(&status(&response)));
        assert!(!String::from_utf8_lossy(body(&response)).contains(&path));
    }
    assert_eq!(status(&running.request("POST", &running.path, None)), 405);
    assert_eq!(status(&running.request("GET", &running.path, None)), 200);
    running.finish();
}

#[test]
fn native_page_serving_requires_its_own_consent_and_page_read_permission() {
    fail_marker(
        Prepared::new().start(config(), false, true),
        "github_app_pages_serving_permission_required",
    );
    fail_marker(
        Prepared::new().start(config(), true, false),
        "github_app_pages_permission_required",
    );
    let mut remote = config();
    remote.listen = "0.0.0.0:0".parse().unwrap();
    remote.allow_remote = true;
    fail_marker(
        Prepared::new().start(remote, true, true),
        "github_app_remote_tls_permission_required",
    );
}

#[test]
fn native_html_wire_budget_refuses_without_partial_html_or_state_changes() {
    let mut c = config();
    c.max_response_bytes = 1000;
    let running = Prepared::new().start(c, true, true).unwrap();
    let response = running.request("GET", &running.path, None);
    assert_eq!(status(&response), 507);
    assert!(
        response.len() <= 1000,
        "the complete error packet must also fit the wire budget"
    );
    assert_eq!(
        serde_json::from_slice::<Value>(body(&response)).unwrap(),
        json!({"accepted":false,"error":"github_app_ingress_capacity_exceeded","complete":false})
    );
    assert!(!body(&response).starts_with(b"<!doctype html>"));
    running.finish();
}

#[test]
fn native_page_listener_shutdown_joins_a_stalled_owned_connection() {
    let mut c = config();
    c.max_connections = 1;
    c.request_timeout_ms = 3000;
    let running = Prepared::new().start(c, true, true).unwrap();
    let address = running.address();
    let started = Instant::now();
    let mut stalled = TcpStream::connect_timeout(&address, Duration::from_secs(1)).unwrap();
    stalled.write_all(b"GET /p/").unwrap();
    assert_eq!(
        status(&running.request("GET", &running.path, None)),
        503,
        "the stalled connection must occupy the sole native worker slot"
    );
    running.finish();
    assert!(started.elapsed() < Duration::from_secs(2));
    assert!(TcpStream::connect_timeout(&address, Duration::from_millis(200)).is_err());
}

#[test]
fn native_page_tls_checks_sni_and_host_before_releasing_approved_html() {
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
        private_key: Zeroizing::new(generated.key_pair.serialize_der()),
    });
    let running = Prepared::new().start(c, true, true).unwrap();
    let address = running.address();
    let client = reqwest::blocking::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(2))
        .resolve("allowed.fixture", address)
        .resolve("other.fixture", address)
        .add_root_certificate(reqwest::Certificate::from_der(&der).unwrap())
        .build()
        .unwrap();
    let send = |name: &str, host: &str| {
        client
            .get(format!("https://{name}:{}{}", address.port(), running.path))
            .header("Host", format!("{host}:{}", address.port()))
            .send()
            .unwrap()
    };
    for (name, host) in [
        ("other.fixture", "allowed.fixture"),
        ("allowed.fixture", "other.fixture"),
    ] {
        let response = send(name, host);
        assert_eq!(response.status().as_u16(), 403);
        assert!(
            !response
                .bytes()
                .unwrap()
                .windows(15)
                .any(|w| w == b"<!doctype html>")
        );
    }
    let response = send("allowed.fixture", "allowed.fixture");
    assert_eq!(response.status().as_u16(), 200);
    assert!(
        response
            .headers()
            .get("content-security-policy")
            .unwrap()
            .to_str()
            .unwrap()
            .contains("frame-ancestors 'none'")
    );
    assert_eq!(response.bytes().unwrap().as_ref(), running.html.as_bytes());
    running.finish();
}

#[test]
fn native_key_rotation_revokes_old_url_while_prior_key_only_authenticates_history() {
    const NEXT_KEY: &[u8] = b"public owned replacement page access key";
    let mut prepared = Prepared::new();
    prepared.host = app::Host::with_signing_keys(
        app::HostProfile::parse(
            &serde_json::to_vec(&json!({"schemaVersion":1,"appId":123,
            "secretCredential":"current-not-read","priorSecretCredentials":["prior-not-read"],
            "grants":[{"installationId":42,"repositoryId":9,"repository":"acme/demo"}]}))
            .unwrap(),
        )
        .unwrap(),
        vec![
            app::SigningKey::from_host_bytes(NEXT_KEY).unwrap(),
            app::SigningKey::from_host_bytes(KEY).unwrap(),
        ],
    )
    .unwrap();
    let mut mac = Hmac::<sha2::Sha256>::new_from_slice(NEXT_KEY).unwrap();
    mac.update(b"yoke-code-intelligence/github-app-page-access/v1\n");
    mac.update(
        &serde_json::to_vec(
            &json!({"workspaceId":prepared.workspace,"appId":123,"pageId":prepared.page_id}),
        )
        .unwrap(),
    );
    let fresh_path = format!(
        "/p/{}?t={}",
        prepared.page_id,
        hex::encode(mac.finalize().into_bytes())
    );
    let running = prepared.start(config(), true, true).unwrap();
    assert_eq!(status(&running.request("GET", &running.path, None)), 404);
    let fresh = running.request("GET", &fresh_path, None);
    assert_eq!(status(&fresh), 200);
    assert_eq!(body(&fresh), running.html.as_bytes());
    running.finish();
}

#[test]
fn native_read_rechecks_repository_grant_and_cannot_release_stored_html_after_revocation() {
    let mut prepared = Prepared::new();
    prepared.host=app::Host::with_signing_key(
        app::HostProfile::parse(&serde_json::to_vec(&json!({"schemaVersion":1,"appId":123,
            "secretCredential":"not-read","grants":[{"installationId":42,"repositoryId":10,"repository":"acme/other"}]})).unwrap()).unwrap(),
        app::SigningKey::from_host_bytes(KEY).unwrap()).unwrap();
    let running = prepared.start(config(), true, true).unwrap();
    let response = running.request("GET", &running.path, None);
    assert_eq!(status(&response), 503);
    assert_eq!(
        serde_json::from_slice::<Value>(body(&response)).unwrap(),
        json!({"accepted":false,"error":"github_app_page_unavailable","complete":false})
    );
    running.finish();
}

#[test]
#[ignore = "Explicit owned native listener for the T3 browser oracle; bounded by a stop marker and lifetime"]
fn native_listener_for_owned_browser_oracle() {
    let ready = std::path::PathBuf::from(
        std::env::var_os("YOKE_PAGES_NATIVE_BROWSER_READY").expect("explicit owned ready path"),
    );
    let stop = std::path::PathBuf::from(
        std::env::var_os("YOKE_PAGES_NATIVE_BROWSER_STOP").expect("explicit owned stop path"),
    );
    assert!(ready.is_absolute() && stop.is_absolute() && ready.parent() == stop.parent());
    assert!(!ready.exists() && !stop.exists());
    let mut c = config();
    c.lifetime_ms = 240_000;
    c.request_timeout_ms = 5000;
    c.max_requests = 100;
    let running = Prepared::new().start(c, true, true).unwrap();
    let metadata = json!({"oracle":"native_pages_http_browser_v1","fixtureOnly":true,
        "url":format!("http://{}{}",running.address(),running.path),
        "htmlSha256":yoke::model::hash(running.html.as_bytes()),
        "sourceSha256":yoke::model::hash(SOURCE.as_bytes()),
        "pageId":running.page_id,"sourceBytes":SOURCE.len(),"casFilesHashed":running.blobs.len(),
        "nativeListener":true,"staticHelper":false,"liveGitHubValidated":false});
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&ready)
        .unwrap();
    file.write_all(&serde_json::to_vec_pretty(&metadata).unwrap())
        .unwrap();
    file.sync_all().unwrap();
    let started = Instant::now();
    while !stop.exists() && started.elapsed() < Duration::from_secs(230) {
        std::thread::sleep(Duration::from_millis(25));
    }
    let stopped_by_marker = stop.exists();
    running.finish();
    assert!(
        stopped_by_marker,
        "browser oracle ended without its explicit owned stop marker"
    );
    println!(
        "{}",
        json!({"oracle":"native_pages_http_browser_v1","fixtureOnly":true,
        "nativeListener":true,"staticHelper":false,"stoppedByMarker":true,
        "recordsUnchanged":true,"casBytesUnchanged":true,"sourceBytesUnchanged":true,
        "htmlSha256":metadata["htmlSha256"],"liveGitHubValidated":false})
    );
}
