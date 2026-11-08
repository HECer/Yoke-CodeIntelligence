//! Real owned sockets/SQLite/source captures; no external service or credentials.
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    io::{Read, Write},
    net::{SocketAddr, TcpListener},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::Duration,
};
use yoke::{
    Engine,
    capture::Deadline,
    github::{self, discussions::Capture},
    github_app::{
        Host, HostProfile, SigningKey,
        digest::{self, discussions, service},
    },
    model::{hash, identity},
};

#[derive(Clone)]
struct Call {
    method: String,
    path: String,
    headers: String,
    body: Vec<u8>,
}
struct Fixture {
    endpoint: SocketAddr,
    calls: Arc<Mutex<Vec<Call>>>,
    mode: Arc<Mutex<String>>,
    stop: Arc<AtomicBool>,
    worker: Option<thread::JoinHandle<()>>,
}
impl Fixture {
    fn new() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let endpoint = listener.local_addr().unwrap();
        let calls = Arc::new(Mutex::new(vec![]));
        let seen = Arc::clone(&calls);
        let mode = Arc::new(Mutex::new("normal".to_owned()));
        let selected = Arc::clone(&mode);
        let stop = Arc::new(AtomicBool::new(false));
        let done = Arc::clone(&stop);
        let worker = thread::spawn(move || {
            let mut receipts = BTreeMap::new();
            while !done.load(Ordering::SeqCst) {
                let (mut stream, _) = match listener.accept() {
                    Ok(v) => v,
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(2));
                        continue;
                    }
                    Err(e) => panic!("fixture accept: {e}"),
                };
                stream.set_nonblocking(false).unwrap();
                stream
                    .set_read_timeout(Some(Duration::from_secs(2)))
                    .unwrap();
                stream
                    .set_write_timeout(Some(Duration::from_secs(2)))
                    .unwrap();
                let mut header = vec![];
                let mut byte = [0];
                while header.len() < 16384 && !header.ends_with(b"\r\n\r\n") {
                    if stream.read(&mut byte).unwrap_or(0) == 0 {
                        break;
                    }
                    header.push(byte[0]);
                }
                let headers = String::from_utf8(header).unwrap();
                let mut parts = headers.lines().next().unwrap_or("").split_whitespace();
                let method = parts.next().unwrap_or("").to_owned();
                let path = parts.next().unwrap_or("").to_owned();
                let length = headers
                    .lines()
                    .filter_map(|l| l.split_once(':'))
                    .find(|(k, _)| k.eq_ignore_ascii_case("content-length"))
                    .map(|(_, v)| v.trim().parse::<usize>().unwrap())
                    .unwrap_or(0);
                assert!(length <= 4_000_000);
                let mut body = vec![0; length];
                if stream.read_exact(&mut body).is_err() {
                    continue;
                }
                seen.lock().unwrap().push(Call {
                    method: method.clone(),
                    path: path.clone(),
                    headers,
                    body: body.clone(),
                });
                let mode = selected.lock().unwrap().clone();
                let mut status = "200 OK";
                let mut extra = String::new();
                let mut output = if path.split('?').next() == Some("/repos/org/repo") {
                    json!({"id":9,"full_name":"org/repo"})
                } else if path.starts_with("/repos/org/repo/pulls?") {
                    json!([])
                } else if let Some(id) = path.strip_prefix("/v1/digests/") {
                    if method == "PUT" {
                        assert!(yoke::model::valid_id(id));
                        let receipt = json!({"schemaVersion":1,"proposalId":id,"payloadHash":hash(&body),"receiptId":hash(format!("owned-receiver:{id}")),"accepted":true});
                        if mode != "absent" {
                            receipts.insert(id.to_owned(), receipt.clone());
                        }
                        if mode == "lost" || mode == "absent" {
                            continue;
                        }
                        if mode == "delay" {
                            thread::sleep(Duration::from_millis(200));
                        }
                        receipt
                    } else if method == "GET" {
                        if let Some(r) = receipts.get(id) {
                            r.clone()
                        } else {
                            status = "404 Not Found";
                            json!({})
                        }
                    } else {
                        panic!("unexpected method")
                    }
                } else {
                    panic!("unexpected path {path}")
                };
                if path.starts_with("/v1/") && method == "PUT" {
                    match mode.as_str() {
                        "wrong" => output["payloadHash"] = json!("0".repeat(64)),
                        "unknown-key" => output["owned-secret-marker"] = json!("must-not-echo"),
                        "redirect" => {
                            status = "307 Temporary Redirect";
                            extra = format!("Location: http://{endpoint}/forbidden\r\n");
                        }
                        "oversized" => output["extra"] = json!("x".repeat(70_000)),
                        _ => {}
                    }
                }
                let raw = serde_json::to_vec(&output).unwrap();
                let _ = write!(
                    stream,
                    "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\n{extra}Connection: close\r\n\r\n",
                    raw.len()
                );
                let _ = stream.write_all(&raw);
            }
        });
        Self {
            endpoint,
            calls,
            mode,
            stop,
            worker: Some(worker),
        }
    }
    fn capture(&self) -> Capture {
        let profile = github::discussions::Profile {
            allow_discussion_text: true,
            read: github::HostReadProfile {
                allow_network: true,
                allowed_repos: vec!["org/repo".into()],
                ..Default::default()
            },
            ..Default::default()
        };
        let d = Deadline::new(20000);
        let client = github::ReadClient::for_fixture(
            &profile.read,
            &github::HostTestProfile::loopback(self.endpoint).unwrap(),
            &d,
        )
        .unwrap();
        github::discussions::capture(
            &client,
            &profile,
            &github::discussions::Request {
                schema_version: 1,
                repository: "org/repo".into(),
                repository_id: 9,
                pull_requests: vec![],
            },
            &d,
        )
        .unwrap()
    }
    fn writes(&self) -> Vec<Call> {
        self.calls
            .lock()
            .unwrap()
            .iter()
            .filter(|c| c.method == "PUT")
            .cloned()
            .collect()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(worker) = self.worker.take() {
            let joined = worker.join();
            if !thread::panicking() {
                joined.unwrap();
            }
        }
    }
}
struct Harness {
    root: tempfile::TempDir,
    engine: Engine,
    app: HostProfile,
    host: Host,
    request: digest::PreviewRequest,
    capture: Capture,
    profile: service::Profile,
    options: service::Options,
    source_options: digest::Options,
    permission: discussions::Permission,
}
impl Harness {
    fn new(f: &Fixture) -> Self {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(
            root.path().join("main.rs"),
            "pub fn café(x:usize)->usize { x + 987654321 }\r\npub const FLAG:usize=123456789;\r\n",
        )
        .unwrap();
        let mut engine = Engine::open(root.path()).unwrap();
        let snapshot = engine.index(&Deadline::new(20000)).unwrap().0;
        let app = HostProfile::parse(&serde_json::to_vec(&app_document()).unwrap()).unwrap();
        let host = Host::with_signing_key(
            app.clone(),
            SigningKey::from_host_bytes(b"owned-digest-service-signing-key").unwrap(),
        )
        .unwrap();
        let request = digest::PreviewRequest {
            schema_version: 1,
            snapshot_id: snapshot.id,
            installation_id: 42,
            repository_id: 9,
            repository: "org/repo".into(),
        };
        let profile = service::Profile {
            schema_version: 1,
            endpoint: format!("http://{}/", f.endpoint),
            credential_name: None,
            grants: vec![yoke::github_app::Grant {
                installation_id: 42,
                repository_id: 9,
                repository: "org/repo".into(),
            }],
            max_payload_bytes: 4_000_000,
            max_response_bytes: 64_000,
            max_journal_records: 1000,
            max_journal_bytes: 8_000_000,
        };
        Self {
            root,
            engine,
            app,
            host,
            request,
            capture: f.capture(),
            profile,
            options: service::Options {
                allow_service: true,
                allow_network: true,
                allow_writes: true,
                allow_fixture: true,
                ..Default::default()
            },
            source_options: Default::default(),
            permission: discussions::Permission {
                allow_discussions: true,
                allow_fixture: true,
                handoff_preview: true,
            },
        }
    }
    fn source(&self) -> discussions::Context<'_> {
        discussions::Context {
            store: &self.engine.store,
            profile: &self.app,
            options: &self.source_options,
            permission: &self.permission,
            source: discussions::Source::Current,
        }
    }
    fn context(&self) -> service::Context<'_> {
        service::Context {
            store: &self.engine.store,
            host: &self.host,
            profile: &self.profile,
            options: &self.options,
        }
    }
    fn approval(&self) -> service::Approval {
        let p = service::propose(
            &self.source(),
            &self.request,
            &self.capture,
            &self.profile,
            &Deadline::new(20000),
        )
        .unwrap()
        .value(&Deadline::new(20000))
        .unwrap();
        service::Approval {
            schema_version: 1,
            proposal_id: p["proposalId"].as_str().unwrap().into(),
            payload_hash: p["payloadHash"].as_str().unwrap().into(),
            request_id: "01234567-89ab-cdef-0123-456789abcdef".into(),
            approval: "publish_exact_digest".into(),
        }
    }
    fn client(&self, f: &Fixture) -> service::Client {
        service::Client::for_fixture(&self.context(), f.endpoint, &Deadline::new(20000)).unwrap()
    }
    fn send(&self, f: &Fixture, a: &service::Approval) -> anyhow::Result<Value> {
        let client =
            service::Client::for_fixture(&self.context(), f.endpoint, &Deadline::new(20000))?;
        service::send(
            &self.context(),
            &self.source(),
            &self.request,
            &self.capture,
            a,
            &client,
            &Deadline::new(20000),
        )
    }
    fn rows(&self) -> usize {
        self.engine
            .store
            .db
            .query_row(
                "SELECT count(*) FROM records WHERE kind=?1",
                [service::RECORD_KIND],
                |r| r.get(0),
            )
            .unwrap()
    }
}
fn app_document() -> Value {
    json!({"schemaVersion":1,"appId":123,"secretCredential":"must-not-read-service-tests","grants":[{"installationId":42,"repositoryId":9,"repository":"org/repo"}]})
}

#[test]
fn exact_approved_bytes_use_one_put_and_keep_captured_headers_and_receipt_scope() {
    let f = Fixture::new();
    let h = Harness::new(&f);
    let a = h.approval();
    std::fs::write(h.root.path().join("main.rs"), "pub fn live_changed(){}\n").unwrap();
    let v = h.send(&f, &a).unwrap();
    assert_eq!(v["status"], "accepted");
    assert_eq!(v["serviceAccepted"], true);
    assert_eq!(v["processingComplete"], false);
    let writes = f.writes();
    assert_eq!(writes.len(), 1);
    assert_eq!(hash(&writes[0].body), a.payload_hash);
    assert!(writes[0].path.ends_with(&a.proposal_id));
    assert!(
        !writes[0]
            .headers
            .to_ascii_lowercase()
            .contains("authorization:")
    );
    let body = String::from_utf8(writes[0].body.clone()).unwrap();
    assert!(body.contains("café"));
    for absent in ["987654321", "123456789", "live_changed"] {
        assert!(!body.contains(absent));
    }
    assert_eq!(h.rows(), 2);
    assert_eq!(h.send(&f, &a).unwrap()["historical"], true);
    assert_eq!(f.writes().len(), 1);
    assert_eq!(
        service::status(&h.context(), &a.request_id, &Deadline::new(20000)).unwrap()["networkRequests"],
        0
    );
}
#[test]
fn lost_response_stays_unknown_blocks_retries_and_recovers_with_get_only() {
    let f = Fixture::new();
    let h = Harness::new(&f);
    let a = h.approval();
    *f.mode.lock().unwrap() = "lost".into();
    assert!(
        h.send(&f, &a)
            .unwrap_err()
            .to_string()
            .contains("outcome_unknown")
    );
    assert_eq!(h.rows(), 1);
    assert_eq!(
        service::status(&h.context(), &a.request_id, &Deadline::new(20000)).unwrap()["status"],
        "unknown"
    );
    assert!(h.send(&f, &a).is_err());
    let mut duplicate = a.clone();
    duplicate.request_id = "11234567-89ab-cdef-0123-456789abcdef".into();
    assert!(h.send(&f, &duplicate).is_err());
    assert_eq!(f.writes().len(), 1);
    let recovered = service::recover(
        &h.context(),
        &a.request_id,
        &h.client(&f),
        &Deadline::new(20000),
    )
    .unwrap();
    assert_eq!(recovered["recovered"], true);
    assert_eq!(recovered["status"], "accepted");
    assert_eq!(h.rows(), 2);
    let calls = f.calls.lock().unwrap().len();
    assert_eq!(
        service::recover(
            &h.context(),
            &a.request_id,
            &h.client(&f),
            &Deadline::new(20000)
        )
        .unwrap()["historical"],
        true
    );
    assert_eq!(f.calls.lock().unwrap().len(), calls);
}
#[test]
fn remote_absence_does_not_erase_intent_or_authorize_another_send() {
    let f = Fixture::new();
    let h = Harness::new(&f);
    let a = h.approval();
    *f.mode.lock().unwrap() = "absent".into();
    assert!(h.send(&f, &a).is_err());
    let result = service::recover(
        &h.context(),
        &a.request_id,
        &h.client(&f),
        &Deadline::new(20000),
    )
    .unwrap();
    assert_eq!(result["status"], "unknown");
    assert_eq!(result["retryAllowed"], false);
    assert_eq!(h.rows(), 1);
    assert!(h.send(&f, &a).is_err());
    assert_eq!(f.writes().len(), 1);
}
#[test]
fn malformed_wrong_oversized_and_redirect_receipts_leave_durable_unknown() {
    for mode in ["wrong", "unknown-key", "oversized", "redirect"] {
        let f = Fixture::new();
        let h = Harness::new(&f);
        let a = h.approval();
        *f.mode.lock().unwrap() = mode.into();
        let error = h.send(&f, &a).unwrap_err().to_string();
        assert!(!error.contains("must-not-echo"));
        assert!(!error.contains("owned-secret-marker"));
        assert_eq!(h.rows(), 1);
        assert_eq!(f.writes().len(), 1);
        assert!(
            !f.calls
                .lock()
                .unwrap()
                .iter()
                .any(|c| c.path == "/forbidden")
        );
        assert_eq!(
            service::recover(
                &h.context(),
                &a.request_id,
                &h.client(&f),
                &Deadline::new(20000)
            )
            .unwrap()["status"],
            "accepted"
        );
    }
}
#[test]
fn host_authority_fixture_scope_and_stale_approval_refuse_before_intent_or_http() {
    let f = Fixture::new();
    let mut h = Harness::new(&f);
    let a = h.approval();
    for flag in 0..4 {
        let mut options = service::Options { ..h.options };
        match flag {
            0 => options.allow_service = false,
            1 => options.allow_network = false,
            2 => options.allow_writes = false,
            _ => options.allow_fixture = false,
        };
        let client = h.client(&f);
        let c = service::Context {
            options: &options,
            ..h.context()
        };
        assert!(
            service::send(
                &c,
                &h.source(),
                &h.request,
                &h.capture,
                &a,
                &client,
                &Deadline::new(20000)
            )
            .is_err()
        );
        assert_eq!(h.rows(), 0);
    }
    let mut stale = a.clone();
    stale.payload_hash = "0".repeat(64);
    assert!(h.send(&f, &stale).is_err());
    assert_eq!(h.rows(), 0);
    h.profile.grants[0].repository_id = 10;
    assert!(h.send(&f, &a).is_err());
    h.profile.grants[0].repository_id = 9;
    let mut revoked = app_document();
    revoked["grants"][0]["repositoryId"] = json!(10);
    h.host = Host::with_signing_key(
        HostProfile::parse(&serde_json::to_vec(&revoked).unwrap()).unwrap(),
        SigningKey::from_host_bytes(b"owned-digest-service-signing-key").unwrap(),
    )
    .unwrap();
    assert!(h.send(&f, &a).is_err());
    assert!(f.writes().is_empty());
    assert_eq!(h.rows(), 0);
}
#[test]
fn corrupt_captured_source_never_uses_approved_preview_as_source_authority() {
    let f = Fixture::new();
    let h = Harness::new(&f);
    let a = h.approval();
    let snapshot = h.engine.store.snapshot(&h.request.snapshot_id).unwrap();
    let blob = h
        .engine
        .store
        .dir
        .join("blobs")
        .join(&snapshot.files[0].hash);
    assert!(blob.exists());
    std::fs::write(&blob, b"corrupted source").unwrap();
    assert!(h.send(&f, &a).is_err());
    assert_eq!(h.rows(), 0);
    assert!(f.writes().is_empty());
}
#[test]
fn receiver_success_followed_by_sql_failure_remains_recoverable_without_resend() {
    let f = Fixture::new();
    let h = Harness::new(&f);
    let a = h.approval();
    h.engine.store.db.execute_batch("CREATE TRIGGER refuse_digest_accept BEFORE INSERT ON records WHEN NEW.kind='github_app_digest_service_v1' AND instr(NEW.value,'\"type\":\"accepted\"')>0 BEGIN SELECT RAISE(ABORT,'owned receipt persistence failure'); END;").unwrap();
    assert!(h.send(&f, &a).is_err());
    assert_eq!(h.rows(), 1);
    assert_eq!(f.writes().len(), 1);
    h.engine
        .store
        .db
        .execute_batch("DROP TRIGGER refuse_digest_accept")
        .unwrap();
    assert_eq!(
        service::recover(
            &h.context(),
            &a.request_id,
            &h.client(&f),
            &Deadline::new(20000)
        )
        .unwrap()["status"],
        "accepted"
    );
    assert_eq!(f.writes().len(), 1);
}
#[test]
fn output_and_payload_and_journal_reservation_refuse_before_send() {
    let f = Fixture::new();
    let mut h = Harness::new(&f);
    let a = h.approval();
    h.options.max_output_bytes = 1;
    assert!(h.send(&f, &a).is_err());
    assert_eq!(h.rows(), 0);
    h.options.max_output_bytes = 32000;
    h.profile.max_payload_bytes = 64;
    assert!(h.send(&f, &a).is_err());
    assert_eq!(h.rows(), 0);
    h.profile.max_payload_bytes = 4_000_000;
    h.profile.max_journal_bytes = 8192;
    assert!(h.send(&f, &a).is_err());
    assert_eq!(h.rows(), 0);
    assert!(f.writes().is_empty());
}
#[test]
fn unknown_intent_retains_its_source_but_does_not_turn_payload_hashes_into_roots() {
    let f = Fixture::new();
    let mut h = Harness::new(&f);
    let a = h.approval();
    *f.mode.lock().unwrap() = "lost".into();
    assert!(h.send(&f, &a).is_err());
    std::fs::write(h.root.path().join("main.rs"), "pub fn next_version(){}\n").unwrap();
    h.engine.index(&Deadline::new(20000)).unwrap();
    let plan =
        yoke::retention::plan(&h.engine.store, &Default::default(), &Deadline::new(20000)).unwrap();
    assert_eq!(plan["snapshotIds"], json!([]));
    assert_eq!(h.rows(), 1);
}
#[test]
fn mutated_or_rekeyed_journal_is_refused_before_remote_recovery() {
    let f = Fixture::new();
    let h = Harness::new(&f);
    let a = h.approval();
    *f.mode.lock().unwrap() = "lost".into();
    assert!(h.send(&f, &a).is_err());
    let (id, raw): (String, String) = h
        .engine
        .store
        .db
        .query_row(
            "SELECT id,value FROM records WHERE kind=?1",
            [service::RECORD_KIND],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    let mut record: Value = serde_json::from_str(&raw).unwrap();
    record["event"]["data"]["digestId"] = json!("0".repeat(64));
    h.engine
        .store
        .db
        .execute(
            "DELETE FROM records WHERE kind=?1 AND id=?2",
            rusqlite::params![service::RECORD_KIND, id],
        )
        .unwrap();
    h.engine
        .store
        .record(service::RECORD_KIND, &identity(&record), &record)
        .unwrap();
    assert!(service::status(&h.context(), &a.request_id, &Deadline::new(20000)).is_err());
    assert!(
        service::recover(
            &h.context(),
            &a.request_id,
            &h.client(&f),
            &Deadline::new(20000)
        )
        .is_err()
    );
    assert_eq!(f.writes().len(), 1);
}
#[test]
fn caller_transaction_and_weak_sql_durability_never_create_an_intent() {
    let f = Fixture::new();
    let h = Harness::new(&f);
    let a = h.approval();
    h.engine.store.db.execute_batch("BEGIN").unwrap();
    assert!(h.send(&f, &a).is_err());
    assert!(!h.engine.store.db.is_autocommit());
    h.engine
        .store
        .db
        .execute_batch("ROLLBACK; PRAGMA synchronous=NORMAL")
        .unwrap();
    assert!(h.send(&f, &a).is_err());
    assert_eq!(h.rows(), 0);
    assert!(f.writes().is_empty());
}
#[test]
fn strict_profiles_and_approval_do_not_echo_unknown_keys_or_accept_ambient_urls() {
    let f = Fixture::new();
    let h = Harness::new(&f);
    for endpoint in [
        "http://localhost:1234/",
        "https://example.invalid/path",
        "https://user:password@example.invalid/",
        "https://example.invalid/?x=y",
        "https://EXAMPLE.invalid/",
    ] {
        let mut profile = serde_json::to_value(&h.profile).unwrap();
        profile["endpoint"] = json!(endpoint);
        profile["credentialName"] = json!("owned");
        assert!(service::Profile::parse(&serde_json::to_vec(&profile).unwrap()).is_err());
    }
    let mut a = serde_json::to_value(h.approval()).unwrap();
    a["unknown-sensitive-key"] = json!("secret-value");
    let err = service::Approval::parse(&serde_json::to_vec(&a).unwrap())
        .err()
        .unwrap()
        .to_string();
    assert!(!err.contains("sensitive"));
    assert!(!err.contains("secret-value"));
}
#[test]
fn actual_cli_proposal_is_offline_and_denials_precede_host_files_and_state_creation() {
    let f = Fixture::new();
    let h = Harness::new(&f);
    let host = tempfile::tempdir().unwrap();
    for (name, value) in [
        ("app.json", app_document()),
        ("service.json", serde_json::to_value(&h.profile).unwrap()),
        ("capture.json", serde_json::to_value(&h.capture).unwrap()),
        ("request.json", serde_json::to_value(&h.request).unwrap()),
    ] {
        std::fs::write(host.path().join(name), serde_json::to_vec(&value).unwrap()).unwrap();
    }
    let before = h.engine.store.db.total_changes();
    let calls = f.calls.lock().unwrap().len();
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_yoke"))
        .args(["--root"])
        .arg(h.root.path())
        .args(["--max-bytes", "4000000", "github-app", "--profile"])
        .arg(host.path().join("app.json"))
        .args([
            "digest",
            "--allow-digest",
            "--allow-discussions",
            "--allow-fixture-discussions",
            "--discussion-capture",
        ])
        .arg(host.path().join("capture.json"))
        .args(["--allow-service", "--service-profile"])
        .arg(host.path().join("service.json"))
        .args(["preview", "--request"])
        .arg(host.path().join("request.json"))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let proposal: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(proposal["kind"], "native_digest_service_proposal");
    assert_eq!(proposal["payloadHash"], h.approval().payload_hash);
    assert_eq!(h.engine.store.db.total_changes(), before);
    assert_eq!(f.calls.lock().unwrap().len(), calls);
    assert_eq!(h.rows(), 0);
    let mut stale = h.approval();
    stale.payload_hash = "0".repeat(64);
    std::fs::write(
        host.path().join("stale-approval.json"),
        serde_json::to_vec(&stale).unwrap(),
    )
    .unwrap();
    let stale_output = std::process::Command::new(env!("CARGO_BIN_EXE_yoke"))
        .arg("--root")
        .arg(h.root.path())
        .args(["--allow-writes", "github-app", "--profile"])
        .arg(host.path().join("app.json"))
        .args([
            "digest",
            "--allow-digest",
            "--allow-discussions",
            "--allow-fixture-discussions",
            "--discussion-capture",
        ])
        .arg(host.path().join("capture.json"))
        .args([
            "--allow-service",
            "--allow-service-network",
            "--allow-fixture-service",
            "--service-profile",
        ])
        .arg(host.path().join("service.json"))
        .arg("--service-approval")
        .arg(host.path().join("stale-approval.json"))
        .args(["preview", "--request"])
        .arg(host.path().join("request.json"))
        .output()
        .unwrap();
    assert!(!stale_output.status.success());
    assert!(String::from_utf8_lossy(&stale_output.stderr).contains("stale_approval"));
    assert!(stale_output.stdout.is_empty());
    assert_eq!(h.rows(), 0);
    assert_eq!(f.calls.lock().unwrap().len(), calls);
    let absent = tempfile::tempdir().unwrap();
    let denied = std::process::Command::new(env!("CARGO_BIN_EXE_yoke"))
        .args(["--root"])
        .arg(absent.path())
        .args([
            "github-app",
            "digest",
            "--allow-digest",
            "--service-profile",
            "C:/missing.json",
            "service-recover",
            "--request-id",
            "01234567-89ab-cdef-0123-456789abcdef",
        ])
        .output()
        .unwrap();
    assert!(!denied.status.success());
    assert!(denied.stdout.is_empty());
    assert!(!absent.path().join(".yoke").exists());
}

#[test]
fn exact_output_boundary_is_admitted_before_the_first_put() {
    let f = Fixture::new();
    let h = Harness::new(&f);
    let a = h.approval();
    let value = h.send(&f, &a).unwrap();
    let bound = serde_json::to_vec(&value).unwrap().len() + 1;
    let mut one_below = Harness::new(&f);
    let rejected = one_below.approval();
    one_below.options.max_output_bytes = bound - 1;
    assert!(one_below.send(&f, &rejected).is_err());
    assert_eq!(one_below.rows(), 0);
    assert_eq!(f.writes().len(), 1);
    let mut exact = Harness::new(&f);
    let accepted = exact.approval();
    exact.options.max_output_bytes = bound;
    assert_eq!(exact.send(&f, &accepted).unwrap()["status"], "accepted");
    assert_eq!(f.writes().len(), 2);
}

#[test]
fn held_journal_lock_obeys_original_deadline_without_writing_an_intent() {
    let f = Fixture::new();
    let h = Harness::new(&f);
    let a = h.approval();
    let client = h.client(&f);
    let lock = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(h.engine.store.dir.join("digest-service.lock"))
        .unwrap();
    fs2::FileExt::lock_exclusive(&lock).unwrap();
    let start = std::time::Instant::now();
    let result = service::send(
        &h.context(),
        &h.source(),
        &h.request,
        &h.capture,
        &a,
        &client,
        &Deadline::new(30),
    );
    assert!(result.is_err());
    assert!(start.elapsed() < Duration::from_secs(1));
    assert_eq!(h.rows(), 0);
    assert!(f.writes().is_empty());
    fs2::FileExt::unlock(&lock).unwrap();
    assert_eq!(h.send(&f, &a).unwrap()["status"], "accepted");
}

#[test]
fn sql_contention_preserves_policy_and_deadline_without_network_or_late_intent() {
    let f = Fixture::new();
    let h = Harness::new(&f);
    let a = h.approval();
    let client = h.client(&f);
    h.engine
        .store
        .db
        .busy_timeout(Duration::from_millis(777))
        .unwrap();
    let other = rusqlite::Connection::open(h.engine.store.dir.join("index.sqlite")).unwrap();
    other.execute_batch("BEGIN IMMEDIATE").unwrap();
    let start = std::time::Instant::now();
    let result = service::send(
        &h.context(),
        &h.source(),
        &h.request,
        &h.capture,
        &a,
        &client,
        &Deadline::new(60),
    );
    assert!(result.is_err());
    assert!(start.elapsed() < Duration::from_secs(1));
    assert!(f.writes().is_empty());
    assert_eq!(h.rows(), 0);
    let busy: u64 = h
        .engine
        .store
        .db
        .query_row("PRAGMA busy_timeout", [], |r| r.get(0))
        .unwrap();
    assert_eq!(busy, 777);
    other.execute_batch("ROLLBACK").unwrap();
    assert_eq!(h.send(&f, &a).unwrap()["status"], "accepted");
}

#[test]
fn pending_repository_blocks_a_different_payload_and_retargeted_approval_refuses() {
    let f = Fixture::new();
    let mut h = Harness::new(&f);
    let a = h.approval();
    *f.mode.lock().unwrap() = "lost".into();
    assert!(h.send(&f, &a).is_err());
    h.source_options.max_symbols = 0;
    let mut different = h.approval();
    different.request_id = "11234567-89ab-cdef-0123-456789abcdef".into();
    assert_ne!(different.proposal_id, a.proposal_id);
    assert!(
        h.send(&f, &different)
            .unwrap_err()
            .to_string()
            .contains("outcome_unknown")
    );
    assert_eq!(f.writes().len(), 1);
    h.source_options.max_symbols = digest::Options::default().max_symbols;
    let other = Fixture::new();
    h.profile.endpoint = format!("http://{}/", other.endpoint);
    let retargeted = h.approval();
    assert_eq!(retargeted.payload_hash, a.payload_hash);
    assert_ne!(retargeted.proposal_id, a.proposal_id);
    let error = service::send(
        &h.context(),
        &h.source(),
        &h.request,
        &h.capture,
        &a,
        &h.client(&other),
        &Deadline::new(20000),
    )
    .unwrap_err();
    assert!(error.to_string().contains("stale_approval"));
    assert!(other.writes().is_empty());
}

#[cfg(windows)]
#[test]
#[ignore = "Requires actual Windows per-user Credential Manager; creates and removes one unique signing credential"]
fn actual_cli_send_status_and_lost_response_recovery_use_os_key_without_retry() {
    struct Created(String);
    impl Drop for Created {
        fn drop(&mut self) {
            #[link(name = "Advapi32")]
            unsafe extern "system" {
                fn CredDeleteW(target: *const u16, kind: u32, flags: u32) -> i32;
            }
            let wide: Vec<_> = self.0.encode_utf16().chain(Some(0)).collect();
            assert_ne!(
                unsafe { CredDeleteW(wide.as_ptr(), 1, 0) },
                0,
                "owned credential cleanup failed"
            );
        }
    }
    let f = Fixture::new();
    let h = Harness::new(&f);
    let host = tempfile::tempdir().unwrap();
    let mut nonce = [0_u8; 16];
    getrandom::fill(&mut nonce).unwrap();
    let name = format!("test-digest-{}", hex::encode(nonce));
    yoke::host_credentials::import_github_webhook_secret(
        &name,
        b"owned-digest-service-signing-key",
    )
    .unwrap();
    let _created = Created(format!(
        "yoke-code-intelligence/service/github-webhook/{name}"
    ));
    let mut app = app_document();
    app["secretCredential"] = json!(name);
    let approval = h.approval();
    for (name, value) in [
        ("app.json", app),
        ("service.json", serde_json::to_value(&h.profile).unwrap()),
        ("capture.json", serde_json::to_value(&h.capture).unwrap()),
        ("request.json", serde_json::to_value(&h.request).unwrap()),
        ("approval.json", serde_json::to_value(&approval).unwrap()),
    ] {
        std::fs::write(host.path().join(name), serde_json::to_vec(&value).unwrap()).unwrap();
    }
    let common = |writes: bool| {
        let mut c = std::process::Command::new(env!("CARGO_BIN_EXE_yoke"));
        c.arg("--root")
            .arg(h.root.path())
            .args(["--max-bytes", "4000000"]);
        if writes {
            c.arg("--allow-writes");
        }
        c.args(["github-app", "--profile"])
            .arg(host.path().join("app.json"))
            .args([
                "digest",
                "--allow-digest",
                "--allow-service",
                "--allow-fixture-service",
                "--service-profile",
            ])
            .arg(host.path().join("service.json"));
        c
    };
    let send = || {
        let mut c = common(true);
        c.args([
            "--allow-service-network",
            "--allow-discussions",
            "--allow-fixture-discussions",
            "--discussion-capture",
        ])
        .arg(host.path().join("capture.json"))
        .arg("--service-approval")
        .arg(host.path().join("approval.json"))
        .args(["preview", "--request"])
        .arg(host.path().join("request.json"));
        c.output().unwrap()
    };
    *f.mode.lock().unwrap() = "lost".into();
    let lost = send();
    assert!(!lost.status.success());
    assert_eq!(f.writes().len(), 1);
    let mut status = common(false);
    status.args(["service-status", "--request-id", &approval.request_id]);
    let out = status.output().unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(
        serde_json::from_slice::<Value>(&out.stdout).unwrap()["status"],
        "unknown"
    );
    let mut recover = common(true);
    recover.args([
        "--allow-service-network",
        "service-recover",
        "--request-id",
        &approval.request_id,
    ]);
    let out = recover.output().unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(
        serde_json::from_slice::<Value>(&out.stdout).unwrap()["recovered"],
        true
    );
    let terminal = send();
    assert!(
        terminal.status.success(),
        "{}",
        String::from_utf8_lossy(&terminal.stderr)
    );
    assert_eq!(f.writes().len(), 1);
    assert_eq!(h.rows(), 2);
}
