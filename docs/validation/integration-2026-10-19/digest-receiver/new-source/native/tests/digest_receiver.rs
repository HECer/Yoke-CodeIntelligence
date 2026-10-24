//! Actual owned receiver/client sockets, SQLite, source and host credentials.
use serde_json::{Value, json};
use std::{
    io::{Read, Write},
    net::{SocketAddr, TcpStream},
    time::Duration,
};
use yoke::{
    Engine,
    capture::Deadline,
    github::discussions as provider,
    github_app::{
        Host, HostProfile, SigningKey,
        digest::{
            self, discussions,
            service::{self, receiver},
        },
    },
    model::{hash, identity},
    store::Store,
};
const TOKEN: [u8; 32] = [42; 32];
fn capture() -> provider::Capture {
    // Explicit unsigned fixture import; this is not live GitHub acquisition.
    let payload=provider::Payload {repository:"org/repo".into(),repository_id:9,requested_pulls:vec![],unacquired_pulls:vec![],pulls:vec![],
        discovery_coverage:provider::Coverage{complete:true,reason:"provider_reported_pages_complete".into(),pages:1},
        receipts:vec![provider::Receipt{url:"http://127.0.0.1:1/repos/org/repo".into(),response_hash:hash(b"owned metadata"),response_bytes:14,api_version:"2026-03-10".into(),body_retained:false},
            provider::Receipt{url:"http://127.0.0.1:1/repos/org/repo/pulls?per_page=100&page=1&state=closed&sort=updated&direction=desc".into(),response_hash:hash(b"[]"),response_bytes:2,api_version:"2026-03-10".into(),body_retained:false}],
        acquired_at_unix:1_700_000_000,atomic:false,transport:"explicit_host_loopback_fixture".into(),requests:2,response_bytes:16,text_bytes:0,observations:0,coverage_complete:true};
    let d = Deadline::new(20000);
    let c = provider::Capture {
        schema_version: 1,
        kind: "native_github_closed_pr_discussions".into(),
        id: hash(yoke::json::encode_bounded(&payload, 8_000_000, &d).unwrap()),
        payload,
    };
    c.validate(&d).unwrap();
    c
}
fn app_value() -> Value {
    json!({"schemaVersion":1,"appId":123,"secretCredential":"receiver-tests-no-os-read","grants":[{"installationId":42,"repositoryId":9,"repository":"org/repo"}]})
}
struct Source {
    root: tempfile::TempDir,
    engine: Engine,
    app: HostProfile,
    host: Host,
    request: digest::PreviewRequest,
    capture: provider::Capture,
    options: digest::Options,
    permission: discussions::Permission,
    service_options: service::Options,
}
impl Source {
    fn new() -> Self {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(
            root.path().join("main.rs"),
            "pub fn café(value:usize)->usize { value+123456789 }\r\n",
        )
        .unwrap();
        let mut engine = Engine::open(root.path()).unwrap();
        let snapshot = engine.index(&Deadline::new(20000)).unwrap().0;
        let app = HostProfile::parse(&serde_json::to_vec(&app_value()).unwrap()).unwrap();
        let host = Host::with_signing_key(
            app.clone(),
            SigningKey::from_host_bytes(b"owned-receiver-signing-key").unwrap(),
        )
        .unwrap();
        Self {
            root,
            engine,
            app,
            host,
            request: digest::PreviewRequest {
                schema_version: 1,
                snapshot_id: snapshot.id,
                installation_id: 42,
                repository_id: 9,
                repository: "org/repo".into(),
            },
            capture: capture(),
            options: Default::default(),
            permission: discussions::Permission {
                allow_discussions: true,
                allow_fixture: true,
                handoff_preview: true,
            },
            service_options: service::Options {
                allow_service: true,
                allow_network: true,
                allow_writes: true,
                allow_fixture: true,
                ..Default::default()
            },
        }
    }
    fn source(&self) -> discussions::Context<'_> {
        discussions::Context {
            store: &self.engine.store,
            profile: &self.app,
            options: &self.options,
            permission: &self.permission,
            source: discussions::Source::Current,
        }
    }
    fn client_profile(&self, endpoint: &str) -> service::Profile {
        service::Profile {
            schema_version: 1,
            endpoint: endpoint.into(),
            credential_name: if endpoint.starts_with("https:") {
                Some("receiver-test-token".into())
            } else {
                None
            },
            grants: vec![yoke::github_app::Grant {
                installation_id: 42,
                repository_id: 9,
                repository: "org/repo".into(),
            }],
            max_payload_bytes: 4_000_000,
            max_response_bytes: 64_000,
            max_journal_records: 1000,
            max_journal_bytes: 8_000_000,
        }
    }
    fn context<'a>(&'a self, p: &'a service::Profile) -> service::Context<'a> {
        service::Context {
            store: &self.engine.store,
            host: &self.host,
            profile: p,
            options: &self.service_options,
        }
    }
    fn proposal(&self, p: &service::Profile) -> Value {
        service::propose(
            &self.source(),
            &self.request,
            &self.capture,
            p,
            &Deadline::new(20000),
        )
        .unwrap()
        .value(&Deadline::new(20000))
        .unwrap()
    }
    fn approval(&self, p: &service::Profile) -> service::Approval {
        let v = self.proposal(p);
        service::Approval {
            schema_version: 1,
            proposal_id: v["proposalId"].as_str().unwrap().into(),
            payload_hash: v["payloadHash"].as_str().unwrap().into(),
            request_id: "01234567-89ab-cdef-0123-456789abcdef".into(),
            approval: "publish_exact_digest".into(),
        }
    }
    fn receiver_profile(&self) -> receiver::Profile {
        receiver::Profile {
            schema_version: 1,
            endpoint: "http://127.0.0.1:1/".into(),
            grants: vec![receiver::Access {
                id: "sender".into(),
                token_hash: hash(TOKEN),
                app_id: 123,
                sender_workspace_id: self.engine.store.workspace_id.clone(),
                installation_id: 42,
                repository_id: 9,
                repository: "org/repo".into(),
            }],
            max_payload_bytes: 4_000_000,
            max_records: 1000,
            max_stored_bytes: 32_000_000,
        }
    }
}
fn config() -> receiver::Config {
    receiver::Config {
        allow_receiver: true,
        allow_network: true,
        allow_writes: true,
        allow_fixture: true,
        lifetime_ms: 20000,
        ..Default::default()
    }
}
fn start(s: &Source) -> (tempfile::TempDir, receiver::ServerHandle) {
    let root = tempfile::tempdir().unwrap();
    let h = receiver::start(
        Store::open(root.path()).unwrap(),
        s.receiver_profile(),
        config(),
        &Deadline::new(20000),
    )
    .unwrap();
    (root, h)
}
fn count(root: &std::path::Path) -> usize {
    let db = rusqlite::Connection::open(root.join(".yoke/index.sqlite")).unwrap();
    db.query_row(
        "SELECT count(*) FROM records WHERE kind=?1",
        [receiver::RECORD_KIND],
        |r| r.get(0),
    )
    .unwrap()
}
fn body(proposal: &Value) -> Vec<u8> {
    yoke::json::encode_bounded(&proposal["body"], 4_000_000, &Deadline::new(20000)).unwrap()
}
fn request(
    h: &receiver::ServerHandle,
    method: &str,
    id: &str,
    token: &[u8; 32],
    body: &[u8],
) -> (u16, Value) {
    if rustls::crypto::CryptoProvider::get_default().is_none() {
        let _ = rustls::crypto::ring::default_provider().install_default();
    }
    let c = reqwest::blocking::Client::builder()
        .no_proxy()
        .build()
        .unwrap();
    let url = format!("{}v1/digests/{id}", h.endpoint());
    let mut call = if method == "PUT" {
        c.put(url)
            .header("Content-Type", "application/json")
            .header("Idempotency-Key", id)
            .body(body.to_vec())
    } else {
        c.get(url)
    };
    call = call
        .bearer_auth(hex::encode(token))
        .timeout(Duration::from_secs(5));
    let r = call.send().unwrap();
    let status = r.status().as_u16();
    let bytes = r.bytes().unwrap();
    (status, serde_json::from_slice(&bytes).unwrap())
}
#[test]
fn real_native_client_and_receiver_commit_one_intake_and_reconcile_local_failure() {
    let s = Source::new();
    let (root, h) = start(&s);
    let p = s.client_profile(h.endpoint());
    let a = s.approval(&p);
    let c = s.context(&p);
    let client =
        service::Client::for_fixture_token(&c, h.local_addr(), TOKEN, &Deadline::new(20000))
            .unwrap();
    s.engine.store.db.execute_batch("CREATE TRIGGER refuse_local_ack BEFORE INSERT ON records WHEN NEW.kind='github_app_digest_service_v1' AND instr(NEW.value,'\"type\":\"accepted\"')>0 BEGIN SELECT RAISE(ABORT,'owned local ack failure'); END;").unwrap();
    assert!(
        service::send(
            &c,
            &s.source(),
            &s.request,
            &s.capture,
            &a,
            &client,
            &Deadline::new(20000)
        )
        .is_err()
    );
    assert_eq!(count(root.path()), 1);
    assert_eq!(
        service::status(&c, &a.request_id, &Deadline::new(20000)).unwrap()["status"],
        "unknown"
    );
    s.engine
        .store
        .db
        .execute_batch("DROP TRIGGER refuse_local_ack")
        .unwrap();
    let ack = service::recover(&c, &a.request_id, &client, &Deadline::new(20000)).unwrap();
    assert_eq!(ack["status"], "accepted");
    assert_eq!(ack["recovered"], true);
    assert_eq!(
        service::send(
            &c,
            &s.source(),
            &s.request,
            &s.capture,
            &a,
            &client,
            &Deadline::new(20000)
        )
        .unwrap()["networkRequests"],
        0
    );
    assert_eq!(count(root.path()), 1);
    assert_eq!(h.shutdown().unwrap()["requests"], 2);
}
#[test]
fn receipt_is_immutable_for_exact_repeats_and_survives_receiver_restart() {
    let s = Source::new();
    let (root, h) = start(&s);
    let endpoint = h.endpoint().to_owned();
    let address = h.local_addr();
    let proposal = s.proposal(&s.client_profile(&endpoint));
    let id = proposal["proposalId"].as_str().unwrap();
    let raw = body(&proposal);
    let (status, first) = request(&h, "PUT", id, &TOKEN, &raw);
    assert_eq!(status, 201);
    assert_eq!(first["payloadHash"], hash(&raw));
    let (status, second) = request(&h, "PUT", id, &TOKEN, &raw);
    assert_eq!(status, 200);
    assert_eq!(first, second);
    assert_eq!(count(root.path()), 1);
    h.shutdown().unwrap();
    let mut profile = s.receiver_profile();
    profile.endpoint = endpoint;
    let mut cfg = config();
    cfg.listen = address;
    let h = receiver::start(
        Store::open(root.path()).unwrap(),
        profile,
        cfg,
        &Deadline::new(20000),
    )
    .unwrap();
    assert_eq!(request(&h, "GET", id, &TOKEN, &[]), (200, first));
    h.shutdown().unwrap();
}
#[test]
fn authentication_scope_revocation_and_absent_receipts_do_not_publish_or_disclose() {
    let s = Source::new();
    let (root, h) = start(&s);
    let p = s.proposal(&s.client_profile(h.endpoint()));
    let id = p["proposalId"].as_str().unwrap();
    let raw = body(&p);
    assert_eq!(request(&h, "PUT", id, &[43; 32], &raw).0, 401);
    assert_eq!(count(root.path()), 0);
    let mut foreign = p["body"].clone();
    foreign["workspaceId"] = json!("f".repeat(64));
    let raw_foreign = serde_json::to_vec(&foreign).unwrap();
    assert_eq!(request(&h, "PUT", id, &TOKEN, &raw_foreign).0, 403);
    assert_eq!(count(root.path()), 0);
    assert_eq!(request(&h, "GET", id, &TOKEN, &[]).0, 404);
    assert_eq!(request(&h, "PUT", id, &TOKEN, &raw).0, 201);
    h.revoke("sender", &Deadline::new(20000)).unwrap();
    assert_eq!(request(&h, "GET", id, &TOKEN, &[]).0, 401);
    assert_eq!(request(&h, "PUT", id, &TOKEN, &raw).0, 401);
    assert_eq!(count(root.path()), 1);
    h.shutdown().unwrap();
}
#[test]
fn changed_wire_binding_and_rehashed_text_fields_cannot_enter_the_closed_handoff() {
    let s = Source::new();
    let (root, h) = start(&s);
    let p = s.proposal(&s.client_profile(h.endpoint()));
    let id = p["proposalId"].as_str().unwrap();
    for field in ["body", "diffHunk", "author"] {
        let mut value = p["body"].clone();
        value["digest"]["payload"]["symbols"][0][field] = json!("owned raw-text marker");
        value["digest"]["id"] = json!(hash(
            yoke::json::encode_bounded(
                &value["digest"]["payload"],
                4_000_000,
                &Deadline::new(20000)
            )
            .unwrap()
        ));
        let raw = serde_json::to_vec(&value).unwrap();
        let new_id = identity(
            &json!({"schemaVersion":1,"workspaceId":s.engine.store.workspace_id,"appId":123,"endpoint":h.endpoint(),"payloadHash":hash(&raw)}),
        );
        let (status, response) = request(&h, "PUT", &new_id, &TOKEN, &raw);
        assert_eq!(status, 400);
        assert!(!response.to_string().contains("raw-text"));
    }
    let raw = body(&p);
    assert_eq!(request(&h, "PUT", &"a".repeat(64), &TOKEN, &raw).0, 400);
    assert_eq!(count(root.path()), 0);
    assert_eq!(request(&h, "PUT", id, &TOKEN, &raw).0, 201);
    h.shutdown().unwrap();
}
#[test]
fn receiver_capacity_and_sql_failure_never_return_a_false_acceptance() {
    for fault in ["capacity", "sql"] {
        let s = Source::new();
        let root = tempfile::tempdir().unwrap();
        let store = Store::open(root.path()).unwrap();
        if fault == "sql" {
            store.db.execute_batch("CREATE TRIGGER refuse_receiver BEFORE INSERT ON records WHEN NEW.kind='github_app_digest_receiver_v1' BEGIN SELECT RAISE(ABORT,'private database marker'); END;").unwrap();
        }
        let mut profile = s.receiver_profile();
        if fault == "capacity" {
            profile.max_stored_bytes = 1024;
        }
        let h = receiver::start(store, profile, config(), &Deadline::new(20000)).unwrap();
        let p = s.proposal(&s.client_profile(h.endpoint()));
        let (status, response) = request(
            &h,
            "PUT",
            p["proposalId"].as_str().unwrap(),
            &TOKEN,
            &body(&p),
        );
        assert!(status >= 400);
        assert!(!response.to_string().contains("private database"));
        assert_eq!(count(root.path()), 0);
        h.shutdown().unwrap();
    }
}
#[test]
fn received_snapshot_ids_never_pin_matching_local_capture_history() {
    let mut s = Source::new();
    let profile = s.receiver_profile();
    let mut cfg = config();
    cfg.max_requests = 1;
    let h = receiver::start(
        Store::open(s.root.path()).unwrap(),
        profile,
        cfg,
        &Deadline::new(20000),
    )
    .unwrap();
    let p = s.proposal(&s.client_profile(h.endpoint()));
    let old = s.request.snapshot_id.clone();
    std::fs::write(s.root.path().join("main.rs"), "pub fn next_version(){}\n").unwrap();
    s.engine.index(&Deadline::new(20000)).unwrap();
    assert_eq!(
        request(
            &h,
            "PUT",
            p["proposalId"].as_str().unwrap(),
            &TOKEN,
            &body(&p)
        )
        .0,
        201
    );
    h.wait().unwrap();
    let plan =
        yoke::retention::plan(&s.engine.store, &Default::default(), &Deadline::new(20000)).unwrap();
    assert!(
        plan["snapshotIds"]
            .as_array()
            .unwrap()
            .contains(&json!(old))
    );
}
fn raw(addr: SocketAddr, bytes: &[u8]) -> Vec<u8> {
    let mut stream = TcpStream::connect(addr).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(3)))
        .unwrap();
    stream.write_all(bytes).unwrap();
    let mut response = vec![];
    let _ = stream.read_to_end(&mut response);
    response
}
#[test]
fn ambiguous_framing_foreign_hosts_and_bearerless_puts_refuse_before_body_or_state() {
    let s = Source::new();
    let (root, h) = start(&s);
    let host = h.local_addr();
    let id = "a".repeat(64);
    let token = hex::encode(TOKEN);
    let heads = [
        format!(
            "Host: {host}\r\nAuthorization: Bearer {token}\r\nContent-Length: 1\r\nContent-Length: 1\r\n"
        ),
        format!("Host: {host}\r\nAuthorization: Bearer {token}\r\nTransfer-Encoding: chunked\r\n"),
        format!("Host: foreign.invalid\r\nAuthorization: Bearer {token}\r\nContent-Length: 1\r\n"),
        format!("Host: {host}\r\nContent-Length: 4000000\r\n"),
    ];
    for head in heads {
        let response = raw(
            host,
            format!("PUT /v1/digests/{id} HTTP/1.1\r\n{head}\r\n").as_bytes(),
        );
        assert!(response.starts_with(b"HTTP/1.1 400") || response.starts_with(b"HTTP/1.1 401"));
    }
    assert_eq!(count(root.path()), 0);
    h.shutdown().unwrap();
}
#[test]
fn slow_partial_request_shutdown_and_initial_permissions_remain_bounded() {
    let s = Source::new();
    let root = tempfile::tempdir().unwrap();
    for flag in 0..4 {
        let mut c = config();
        match flag {
            0 => c.allow_receiver = false,
            1 => c.allow_network = false,
            2 => c.allow_writes = false,
            _ => c.allow_fixture = false,
        };
        assert!(
            receiver::start(
                Store::open(root.path()).unwrap(),
                s.receiver_profile(),
                c,
                &Deadline::new(20000)
            )
            .is_err()
        );
    }
    let (root, h) = start(&s);
    let mut stream = TcpStream::connect(h.local_addr()).unwrap();
    stream.write_all(b"PUT /v1/").unwrap();
    std::thread::sleep(Duration::from_millis(30));
    let at = std::time::Instant::now();
    h.shutdown().unwrap();
    assert!(at.elapsed() < Duration::from_secs(1));
    assert_eq!(count(root.path()), 0);
}
#[test]
fn actual_cli_default_denial_creates_no_state_or_listener() {
    let root = tempfile::tempdir().unwrap();
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_yoke"))
        .arg("--root")
        .arg(root.path())
        .args(["digest-receiver", "--profile", "C:/missing-receiver.json"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    assert!(!root.path().join(".yoke").exists());
    assert!(String::from_utf8_lossy(&output.stderr).contains("permission_required"));
}

#[test]
fn detached_host_cancellation_remains_binding_after_start_returns() {
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };
    let s = Source::new();
    let root = tempfile::tempdir().unwrap();
    let cancel = Arc::new(AtomicBool::new(false));
    let d = Deadline::run_bounded(&Deadline::new(20000), Arc::clone(&cancel), || {
        Ok(Deadline::new(20000))
    })
    .unwrap();
    let h = receiver::start(
        Store::open(root.path()).unwrap(),
        s.receiver_profile(),
        config(),
        &d,
    )
    .unwrap();
    let mut stream = TcpStream::connect(h.local_addr()).unwrap();
    stream.write_all(b"PUT /v1/").unwrap();
    std::thread::sleep(Duration::from_millis(30));
    let at = std::time::Instant::now();
    cancel.store(true, Ordering::SeqCst);
    h.wait().unwrap();
    assert!(at.elapsed() < Duration::from_secs(1));
    assert_eq!(count(root.path()), 0);
}

#[test]
fn revocation_during_body_acquisition_precedes_any_record_commit() {
    let s = Source::new();
    let (root, h) = start(&s);
    let p = s.proposal(&s.client_profile(h.endpoint()));
    let id = p["proposalId"].as_str().unwrap();
    let payload = body(&p);
    let address = h.local_addr();
    let mut stream = TcpStream::connect(address).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    write!(stream,"PUT /v1/digests/{id} HTTP/1.1\r\nHost: {address}\r\nAuthorization: Bearer {}\r\nContent-Type: application/json\r\nIdempotency-Key: {id}\r\nContent-Length: {}\r\n\r\n",hex::encode(TOKEN),payload.len()).unwrap();
    stream.write_all(&payload[..1]).unwrap();
    std::thread::sleep(Duration::from_millis(50));
    h.revoke("sender", &Deadline::new(20000)).unwrap();
    stream.write_all(&payload[1..]).unwrap();
    let mut response = vec![];
    let _ = stream.read_to_end(&mut response);
    assert!(response.starts_with(b"HTTP/1.1 401") || response.starts_with(b"HTTP/1.1 403"));
    assert_eq!(count(root.path()), 0);
    h.shutdown().unwrap();
}

#[test]
fn caller_sql_durability_and_corrupted_accepted_bytes_refuse_before_binding() {
    let s = Source::new();
    let root = tempfile::tempdir().unwrap();
    let store = Store::open(root.path()).unwrap();
    store.db.execute_batch("PRAGMA synchronous=NORMAL").unwrap();
    assert!(receiver::start(store, s.receiver_profile(), config(), &Deadline::new(20000)).is_err());
    let h = receiver::start(
        Store::open(root.path()).unwrap(),
        s.receiver_profile(),
        config(),
        &Deadline::new(20000),
    )
    .unwrap();
    let endpoint = h.endpoint().to_owned();
    let address = h.local_addr();
    let proposal = s.proposal(&s.client_profile(&endpoint));
    assert_eq!(
        request(
            &h,
            "PUT",
            proposal["proposalId"].as_str().unwrap(),
            &TOKEN,
            &body(&proposal)
        )
        .0,
        201
    );
    h.shutdown().unwrap();
    let db = rusqlite::Connection::open(root.path().join(".yoke/index.sqlite")).unwrap();
    let raw: String = db
        .query_row(
            "SELECT value FROM records WHERE kind=?1",
            [receiver::RECORD_KIND],
            |r| r.get(0),
        )
        .unwrap();
    let mut value: Value = serde_json::from_str(&raw).unwrap();
    value["body"] = json!("changed stored bytes");
    db.execute(
        "UPDATE records SET value=?1 WHERE kind=?2",
        rusqlite::params![
            serde_json::to_string(&value).unwrap(),
            receiver::RECORD_KIND
        ],
    )
    .unwrap();
    drop(db);
    let mut profile = s.receiver_profile();
    profile.endpoint = endpoint;
    let mut cfg = config();
    cfg.listen = address;
    assert!(
        receiver::start(
            Store::open(root.path()).unwrap(),
            profile,
            cfg,
            &Deadline::new(20000)
        )
        .is_err()
    );
    assert!(std::net::TcpListener::bind(address).is_ok());
}

#[test]
fn payload_capacity_and_expired_partial_body_do_not_commit_late_bytes() {
    let s = Source::new();
    let root = tempfile::tempdir().unwrap();
    let mut profile = s.receiver_profile();
    profile.max_payload_bytes = 1;
    let h = receiver::start(
        Store::open(root.path()).unwrap(),
        profile,
        config(),
        &Deadline::new(20000),
    )
    .unwrap();
    let address = h.local_addr();
    let response=raw(address,format!("PUT /v1/digests/{} HTTP/1.1\r\nHost: {address}\r\nAuthorization: Bearer {}\r\nContent-Type: application/json\r\nIdempotency-Key: {}\r\nContent-Length: 2\r\n\r\n","a".repeat(64),hex::encode(TOKEN),"a".repeat(64)).as_bytes());
    assert!(response.starts_with(b"HTTP/1.1 507"));
    assert_eq!(count(root.path()), 0);
    h.shutdown().unwrap();
    let root = tempfile::tempdir().unwrap();
    let mut cfg = config();
    cfg.request_timeout_ms = 100;
    let h = receiver::start(
        Store::open(root.path()).unwrap(),
        s.receiver_profile(),
        cfg,
        &Deadline::new(20000),
    )
    .unwrap();
    let address = h.local_addr();
    let p = s.proposal(&s.client_profile(h.endpoint()));
    let id = p["proposalId"].as_str().unwrap();
    let payload = body(&p);
    let mut stream = TcpStream::connect(address).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(1)))
        .unwrap();
    write!(stream,"PUT /v1/digests/{id} HTTP/1.1\r\nHost: {address}\r\nAuthorization: Bearer {}\r\nContent-Type: application/json\r\nIdempotency-Key: {id}\r\nContent-Length: {}\r\n\r\n",hex::encode(TOKEN),payload.len()).unwrap();
    stream.write_all(&payload[..1]).unwrap();
    std::thread::sleep(Duration::from_millis(180));
    let _ = stream.write_all(&payload[1..]);
    let mut response = vec![];
    let _ = stream.read_to_end(&mut response);
    assert!(!response.starts_with(b"HTTP/1.1 2"));
    assert_eq!(count(root.path()), 0);
    h.shutdown().unwrap();
}

#[test]
fn real_tls_accepts_selected_ca_and_refuses_wrong_trust_or_name_before_intake() {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let s = Source::new();
    let root = tempfile::tempdir().unwrap();
    let key = rcgen::KeyPair::generate().unwrap();
    let params = rcgen::CertificateParams::new(vec!["127.0.0.1".into()]).unwrap();
    let cert = params.self_signed(&key).unwrap();
    let mut profile = s.receiver_profile();
    profile.endpoint = "https://127.0.0.1:0/".into();
    let mut cfg = config();
    cfg.tls = Some(yoke::federation_http::TlsMaterial {
        certificates: vec![cert.der().to_vec()],
        private_key: zeroize::Zeroizing::new(key.serialize_der()),
    });
    let h = receiver::start(
        Store::open(root.path()).unwrap(),
        profile,
        cfg,
        &Deadline::new(20000),
    )
    .unwrap();
    let p = s.proposal(&s.client_profile(h.endpoint()));
    let id = p["proposalId"].as_str().unwrap();
    let raw = body(&p);
    let untrusted = reqwest::blocking::Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(2))
        .build()
        .unwrap();
    assert!(
        untrusted
            .get(format!("{}v1/digests/{id}", h.endpoint()))
            .bearer_auth(hex::encode(TOKEN))
            .send()
            .is_err()
    );
    assert_eq!(count(root.path()), 0);
    let trusted = reqwest::blocking::Client::builder()
        .no_proxy()
        .add_root_certificate(reqwest::Certificate::from_der(cert.der()).unwrap())
        .timeout(Duration::from_secs(3))
        .build()
        .unwrap();
    assert!(
        trusted
            .get(format!(
                "https://localhost:{}/v1/digests/{id}",
                h.local_addr().port()
            ))
            .bearer_auth(hex::encode(TOKEN))
            .send()
            .is_err()
    );
    assert_eq!(count(root.path()), 0);
    let response = trusted
        .put(format!("{}v1/digests/{id}", h.endpoint()))
        .bearer_auth(hex::encode(TOKEN))
        .header("Content-Type", "application/json")
        .header("Idempotency-Key", id)
        .body(raw.clone())
        .send()
        .unwrap();
    assert_eq!(response.status().as_u16(), 201);
    let ack: Value = serde_json::from_slice(&response.bytes().unwrap()).unwrap();
    assert_eq!(ack["payloadHash"], hash(&raw));
    assert_eq!(count(root.path()), 1);
    h.shutdown().unwrap();
}

#[cfg(windows)]
#[test]
#[ignore = "Requires actual Windows Credential Manager; creates/removes three unique TLS/token/signing credentials and starts only owned loopback CLI processes"]
fn actual_tls_receiver_and_sender_cli_recover_using_os_credentials_and_selected_ca() {
    use std::{
        io::{BufRead, BufReader},
        process::{Command, Stdio},
    };
    struct Credentials(Vec<String>);
    impl Drop for Credentials {
        fn drop(&mut self) {
            #[link(name = "Advapi32")]
            unsafe extern "system" {
                fn CredDeleteW(target: *const u16, kind: u32, flags: u32) -> i32;
            }
            for target in &self.0 {
                let wide: Vec<_> = target.encode_utf16().chain(Some(0)).collect();
                let result = unsafe { CredDeleteW(wide.as_ptr(), 1, 0) };
                if !std::thread::panicking() {
                    assert_ne!(result, 0, "owned credential cleanup failed");
                }
            }
        }
    }
    struct Child(std::process::Child);
    impl Drop for Child {
        fn drop(&mut self) {
            if self.0.try_wait().ok().flatten().is_none() {
                let _ = self.0.kill();
                let _ = self.0.wait();
            }
        }
    }
    let s = Source::new();
    let receiver_root = tempfile::tempdir().unwrap();
    let host = tempfile::tempdir().unwrap();
    let mut nonce = [0_u8; 16];
    getrandom::fill(&mut nonce).unwrap();
    let name = format!("test-receiver-{}", hex::encode(nonce));
    let mut created = Credentials(vec![]);
    let token = yoke::host_credentials::generate_token(&name).unwrap();
    created
        .0
        .push(format!("yoke-code-intelligence/service/token/{name}"));
    let tls = yoke::host_credentials::generate_tls(&name, &["127.0.0.1".into()]).unwrap();
    created
        .0
        .push(format!("yoke-code-intelligence/service/tls/{name}"));
    yoke::host_credentials::import_github_webhook_secret(&name, b"owned-receiver-signing-key")
        .unwrap();
    created.0.push(format!(
        "yoke-code-intelligence/service/github-webhook/{name}"
    ));
    use base64::Engine as _;
    let ca = base64::prelude::BASE64_STANDARD
        .decode(tls["certificateDerBase64"].as_str().unwrap())
        .unwrap();
    std::fs::write(host.path().join("ca.der"), ca).unwrap();
    let mut profile = s.receiver_profile();
    profile.endpoint = "https://127.0.0.1:0/".into();
    profile.grants[0].token_hash = token["tokenHash"].as_str().unwrap().into();
    std::fs::write(
        host.path().join("receiver.json"),
        serde_json::to_vec(&profile).unwrap(),
    )
    .unwrap();
    let mut command = Command::new(env!("CARGO_BIN_EXE_yoke"));
    command
        .arg("--root")
        .arg(receiver_root.path())
        .args([
            "--allow-writes",
            "--timeout",
            "20000",
            "digest-receiver",
            "--allow-receiver",
            "--allow-network",
            "--allow-fixture",
            "--max-requests",
            "2",
            "--profile",
        ])
        .arg(host.path().join("receiver.json"))
        .arg("--tls-credential")
        .arg(&name)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    use std::os::windows::process::CommandExt;
    command.creation_flags(0x08000000);
    let mut child = Child(command.spawn().unwrap());
    let mut line = String::new();
    BufReader::new(child.0.stdout.take().unwrap())
        .read_line(&mut line)
        .unwrap();
    assert!(
        !line.is_empty(),
        "owned receiver CLI produced no announcement"
    );
    let announced: Value = serde_json::from_str(&line).unwrap();
    let mut service = s.client_profile(announced["endpoint"].as_str().unwrap());
    service.credential_name = Some(name.clone());
    let approval = s.approval(&service);
    let mut app = app_value();
    app["secretCredential"] = json!(name);
    for (file, value) in [
        ("app.json", app),
        ("service.json", serde_json::to_value(&service).unwrap()),
        ("approval.json", serde_json::to_value(&approval).unwrap()),
        ("source.json", serde_json::to_value(&s.request).unwrap()),
        ("capture.json", serde_json::to_value(&s.capture).unwrap()),
    ] {
        std::fs::write(host.path().join(file), serde_json::to_vec(&value).unwrap()).unwrap();
    }
    s.engine.store.db.execute_batch("CREATE TRIGGER refuse_os_ack BEFORE INSERT ON records WHEN NEW.kind='github_app_digest_service_v1' AND instr(NEW.value,'\"type\":\"accepted\"')>0 BEGIN SELECT RAISE(ABORT,'owned local failure'); END;").unwrap();
    let cli = || {
        let mut c = Command::new(env!("CARGO_BIN_EXE_yoke"));
        c.creation_flags(0x08000000);
        c.arg("--root")
            .arg(s.root.path())
            .args([
                "--allow-writes",
                "--max-bytes",
                "4000000",
                "github-app",
                "--profile",
            ])
            .arg(host.path().join("app.json"))
            .args([
                "digest",
                "--allow-digest",
                "--allow-service",
                "--allow-service-network",
                "--service-profile",
            ])
            .arg(host.path().join("service.json"))
            .arg("--service-ca")
            .arg(host.path().join("ca.der"));
        c
    };
    let mut send = cli();
    send.args([
        "--allow-discussions",
        "--allow-fixture-discussions",
        "--discussion-capture",
    ])
    .arg(host.path().join("capture.json"))
    .arg("--service-approval")
    .arg(host.path().join("approval.json"))
    .args(["preview", "--request"])
    .arg(host.path().join("source.json"));
    let sent = send.output().unwrap();
    assert!(!sent.status.success());
    assert!(sent.stdout.is_empty());
    assert_eq!(count(receiver_root.path()), 1);
    s.engine
        .store
        .db
        .execute_batch("DROP TRIGGER refuse_os_ack")
        .unwrap();
    let mut recover = cli();
    recover.args(["service-recover", "--request-id", &approval.request_id]);
    let recovered = recover.output().unwrap();
    assert!(
        recovered.status.success(),
        "{}",
        String::from_utf8_lossy(&recovered.stderr)
    );
    assert_eq!(
        serde_json::from_slice::<Value>(&recovered.stdout).unwrap()["recovered"],
        true
    );
    let end = std::time::Instant::now() + Duration::from_secs(3);
    let status = loop {
        if let Some(status) = child.0.try_wait().unwrap() {
            break status;
        }
        assert!(
            std::time::Instant::now() < end,
            "owned receiver did not finish after exactly two requests"
        );
        std::thread::sleep(Duration::from_millis(10));
    };
    assert!(status.success());
    assert_eq!(count(receiver_root.path()), 1);
}
