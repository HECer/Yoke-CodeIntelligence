//! Owned Windows provider/source fixtures; no real installation or project execution.
#![cfg(windows)]
use base64::{Engine as _, prelude::BASE64_STANDARD};
use hmac::{Hmac, Mac};
use serde_json::{Value, json};
use std::{
    collections::BTreeSet,
    io::{Read, Write},
    net::{SocketAddr, TcpListener},
    process::Command,
    sync::{
        Arc, Mutex, OnceLock,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    thread,
    time::Duration,
};
use yoke::{
    Engine,
    capture::Deadline,
    github_app::{self as app, worker},
    github_app_auth as auth,
};
use zeroize::Zeroizing;

const KEY: &[u8] = b"public owned deterministic worker execution fixture key";
const NOW: u64 = 1_900_000_000;
const TOKEN: &str = "owned-opaque-worker-installation-token-canary_2026";
const SOURCE: &str =
    "// café\r\npub fn target() -> u32 { 2 }\r\npub fn caller() -> u32 { target() }\r\n";
const PATH: &str = "café.rs";
fn host() -> app::Host {
    app::Host::with_signing_key(
        app::HostProfile::parse(
            &serde_json::to_vec(&json!({
        "schemaVersion":1,"appId":123,"secretCredential":"not-read-by-fixture",
        "grants":[{"installationId":42,"repositoryId":9,"repository":"acme/demo"}]}))
            .unwrap(),
        )
        .unwrap(),
        app::SigningKey::from_host_bytes(KEY).unwrap(),
    )
    .unwrap()
}
fn raw_event() -> Vec<u8> {
    serde_json::to_vec(
        &json!({"action":"opened","number":7,"installation":{"id":42},
        "repository":{"id":9,"full_name":"acme/demo"},"pull_request":{"number":7,"state":"open",
        "draft":false,"title":"Owned source fixture","head":{"sha":"a".repeat(40)},
        "base":{"sha":"b".repeat(40),"repo":{"id":9,"full_name":"acme/demo"}}}}),
    )
    .unwrap()
}
fn headers(raw: &[u8]) -> app::Headers {
    let mut mac = Hmac::<sha2::Sha256>::new_from_slice(KEY).unwrap();
    mac.update(raw);
    app::Headers::parse(
        &serde_json::to_vec(&json!({"schemaVersion":1,"event":"pull_request",
        "deliveryId":"01234567-89ab-4cde-8000-000000000001",
        "signature256":format!("sha256={}",hex::encode(mac.finalize().into_bytes()))}))
        .unwrap(),
    )
    .unwrap()
}
fn blob_sha(raw: &[u8]) -> String {
    use sha1::{Digest, Sha1};
    let mut digest = Sha1::new();
    digest.update(format!("blob {}\0", raw.len()));
    digest.update(raw);
    hex::encode(digest.finalize())
}
fn key() -> auth::AppKey {
    static KEY: OnceLock<Zeroizing<Vec<u8>>> = OnceLock::new();
    let raw=KEY.get_or_init(|| {
        // Private child stdout stays in memory; no OS credential entry is created.
        let output=Command::new("pwsh.exe").args(["-NoProfile","-NonInteractive","-Command",
            "$rsa=[System.Security.Cryptography.RSA]::Create(2048); try { [Convert]::ToBase64String($rsa.ExportRSAPrivateKey()) } finally { $rsa.Dispose() }"])
            .output().expect("owned fixture RSA generation");
        assert!(output.status.success(),"owned fixture generation failed");
        let stdout=Zeroizing::new(output.stdout);
        Zeroizing::new(BASE64_STANDARD.decode(std::str::from_utf8(&stdout).unwrap().trim()).unwrap())
    });
    auth::AppKey::from_host_bytes(raw).unwrap()
}
struct Fixture {
    engine: Engine,
    host: app::Host,
    options: worker::Options,
    clock: worker::Clock,
    time: Arc<AtomicU64>,
    d: Deadline,
    claim: worker::Claim,
    snapshot: String,
    root: tempfile::TempDir,
}
impl Fixture {
    fn new() -> Self {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join(PATH), SOURCE.as_bytes()).unwrap();
        let d = Deadline::new(30_000);
        let mut engine = Engine::open(root.path()).unwrap();
        let snapshot = engine.index(&d).unwrap().0.id;
        let host = host();
        let options = worker::Options {
            queue: app::Options {
                allow_writes: true,
                ..Default::default()
            },
            ..Default::default()
        };
        let raw = raw_event();
        let admitted = app::ingest(
            &engine.store,
            &host,
            &headers(&raw),
            &raw,
            &options.queue,
            &d,
        )
        .unwrap();
        let job = admitted["jobId"].as_str().unwrap();
        app::select(&engine.store, &host, job, 1, &options.queue, &d).unwrap();
        let time = Arc::new(AtomicU64::new(NOW));
        let clock = worker::Clock::for_fixture(time.clone());
        let request = worker::ClaimRequest {
            schema_version: 1,
            job_id: job.into(),
            expected_queue_revision: 2,
            worker: "owned-native-worker".into(),
            request_id: "89abcdef-0123-4abc-8000-000000000001".into(),
            lease_seconds: 120,
        };
        let claim = worker::claim(&engine.store, &host, &request, &options, &clock, &d).unwrap();
        Self {
            engine,
            host,
            options,
            clock,
            time,
            d,
            claim,
            snapshot,
            root,
        }
    }
    fn request(&self) -> worker::RunRequest {
        worker::RunRequest {
            schema_version: 1,
            claim_id: self.claim.evidence()["claimId"].as_str().unwrap().into(),
            head_snapshot_id: self.snapshot.clone(),
            base_snapshot_id: None,
            request_id: "89abcdef-0123-4abc-8000-000000000002".into(),
        }
    }
    fn context(&self) -> worker::RunContext<'_> {
        worker::RunContext {
            store: &self.engine.store,
            host: &self.host,
            options: &self.options,
            clock: &self.clock,
        }
    }
    fn rows(&self) -> usize {
        self.engine
            .store
            .db
            .query_row(
                "SELECT count(*) FROM records WHERE kind=?1",
                [worker::RECORD_KIND],
                |r| r.get(0),
            )
            .unwrap()
    }
    fn blobs(&self) -> BTreeSet<String> {
        std::fs::read_dir(self.engine.store.dir.join("blobs"))
            .unwrap()
            .map(|entry| entry.unwrap().file_name().into_string().unwrap())
            .collect()
    }
    fn client(&self, provider: &Provider) -> auth::Client {
        fixture_client(
            provider.endpoint,
            self.time.clone(),
            &self.d,
            json!({"contents":"read","pull_requests":"read"}),
        )
    }
}
fn fixture_client(
    endpoint: SocketAddr,
    time: Arc<AtomicU64>,
    d: &Deadline,
    permissions: Value,
) -> auth::Client {
    let profile=auth::AuthProfile::parse(&serde_json::to_vec(&json!({"schemaVersion":1,"appId":123,
            "keyCredential":"not-read-by-fixture","grants":[{"installationId":42,"repositoryId":9,"repository":"acme/demo"}],
            "permissions":permissions})).unwrap()).unwrap();
    auth::Client::for_fixture(
        profile,
        key(),
        auth::FixtureProfile::loopback_with_clock(endpoint, time).unwrap(),
        auth::Options {
            allow_network: true,
            ..Default::default()
        },
        d,
    )
    .unwrap()
}
#[derive(Clone, Copy)]
enum Mode {
    Exact,
    FinalHeadChanged,
    BlobMismatch,
    Minimal,
    FinalCountChanged,
    FinalForkChanged,
    MissingDraft,
    ShortFiles,
    UnsafeLink,
    ReflectedToken,
    Removed,
    Paginated,
    PaginatedUnsafe,
    StalledFiles,
}
struct Seen {
    path: String,
    installation_token: bool,
}
struct Provider {
    endpoint: SocketAddr,
    seen: Arc<Mutex<Vec<Seen>>>,
    stop: Arc<AtomicBool>,
    worker: Option<thread::JoinHandle<()>>,
}
impl Provider {
    fn new(mode: Mode) -> Self {
        Self::with_hook(mode, None)
    }
    fn with_hook(mode: Mode, mut hook: Option<Box<dyn FnOnce() + Send>>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let endpoint = listener.local_addr().unwrap();
        let seen = Arc::new(Mutex::new(Vec::new()));
        let target = seen.clone();
        let stop = Arc::new(AtomicBool::new(false));
        let stopped = stop.clone();
        let worker = thread::spawn(move || {
            let mut pr_reads = 0;
            while !stopped.load(Ordering::SeqCst) {
                match listener.accept() {
                    Ok((mut stream, _)) => {
                        stream.set_nonblocking(false).unwrap();
                        stream
                            .set_read_timeout(Some(Duration::from_secs(2)))
                            .unwrap();
                        stream
                            .set_write_timeout(Some(Duration::from_secs(2)))
                            .unwrap();
                        let mut raw = Zeroizing::new(Vec::new());
                        let mut byte = [0];
                        while raw.len() < 65536 && !raw.ends_with(b"\r\n\r\n") {
                            match stream.read(&mut byte) {
                                Ok(1) => raw.push(byte[0]),
                                _ => break,
                            }
                        }
                        if !raw.ends_with(b"\r\n\r\n") {
                            continue;
                        }
                        let head =
                            Zeroizing::new(String::from_utf8(std::mem::take(&mut *raw)).unwrap());
                        let path = head
                            .lines()
                            .next()
                            .unwrap()
                            .split_whitespace()
                            .nth(1)
                            .unwrap()
                            .to_owned();
                        let length = head
                            .lines()
                            .find_map(|line| {
                                line.split_once(':')
                                    .filter(|(k, _)| k.eq_ignore_ascii_case("content-length"))
                                    .map(|(_, v)| v.trim().parse::<usize>().unwrap())
                            })
                            .unwrap_or(0);
                        assert!(length < 10000);
                        let mut body = Zeroizing::new(vec![0; length]);
                        stream.read_exact(&mut body).unwrap();
                        let installation_token = head.lines().any(|line| {
                            line.split_once(':').is_some_and(|(k, v)| {
                                k.eq_ignore_ascii_case("authorization")
                                    && v.trim() == format!("Bearer {TOKEN}")
                            })
                        });
                        target.lock().unwrap().push(Seen {
                            path: path.clone(),
                            installation_token,
                        });
                        let permissions = if matches!(mode, Mode::Minimal) {
                            json!({"pull_requests":"read"})
                        } else {
                            json!({"contents":"read","pull_requests":"read"})
                        };
                        let repository = json!({"id":9,"full_name":"acme/demo","archived":false});
                        let (status, value) = match path.as_str() {
                            "/app" => (200, json!({"id":123})),
                            "/app/installations/42" => (
                                200,
                                json!({"id":42,"app_id":123,"suspended_at":null,"suspended_by":null,"permissions":permissions}),
                            ),
                            "/app/installations/42/access_tokens" => {
                                let request: Value = serde_json::from_slice(&body).unwrap();
                                assert_eq!(request["repository_ids"], json!([9]));
                                assert_eq!(request["permissions"], permissions);
                                let date =
                                    time::OffsetDateTime::from_unix_timestamp((NOW + 3600) as i64)
                                        .unwrap();
                                let expires = format!(
                                    "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
                                    date.year(),
                                    u8::from(date.month()),
                                    date.day(),
                                    date.hour(),
                                    date.minute(),
                                    date.second()
                                );
                                (
                                    201,
                                    json!({"token":TOKEN,"expires_at":expires,"permissions":permissions,"repository_selection":"selected","repositories":[repository]}),
                                )
                            }
                            "/installation/repositories?per_page=100&page=1" => {
                                (200, json!({"total_count":1,"repositories":[repository]}))
                            }
                            "/repos/acme/demo" => (200, repository.clone()),
                            "/repos/acme/demo/pulls/7" => {
                                pr_reads += 1;
                                if pr_reads > 1
                                    && let Some(hook) = hook.take()
                                {
                                    hook();
                                }
                                let changed =
                                    matches!(mode, Mode::FinalHeadChanged) && pr_reads > 1;
                                let mut head_repository = repository.clone();
                                if matches!(mode, Mode::FinalForkChanged) && pr_reads > 1 {
                                    head_repository["id"] = json!(10);
                                    head_repository["full_name"] = json!("fork/demo");
                                }
                                (
                                    200,
                                    json!({"id":77,"number":7,"state":"open","draft":if matches!(mode,Mode::MissingDraft) {Value::Null} else {json!(false)},"changed_files":if matches!(mode,Mode::Paginated|Mode::PaginatedUnsafe) {101} else if matches!(mode,Mode::FinalCountChanged) && pr_reads>1 {2} else {1},
                                    "head":{"sha":if changed {"c".repeat(40)} else {"a".repeat(40)},"repo":head_repository},
                                    "base":{"sha":"b".repeat(40),"repo":repository}}),
                                )
                            }
                            route
                                if matches!(mode, Mode::Paginated | Mode::PaginatedUnsafe)
                                    && route.starts_with(
                                        "/repos/acme/demo/pulls/7/files?per_page=100&page=",
                                    ) =>
                            {
                                let page =
                                    route.rsplit('=').next().unwrap().parse::<usize>().unwrap();
                                let rows: Vec<_> = if (1..=2).contains(&page) {
                                    ((page-1)*100..(page*100).min(101)).map(|n|json!({
                                    "sha":blob_sha(SOURCE.as_bytes()),"filename":if n==0 {PATH.to_owned()} else {format!("extra-{n}.rs")},"status":"modified",
                                    "additions":1,"deletions":1,"changes":2,"patch":"@@ -2 +2 @@\n-pub fn target() -> u32 { 1 }\r\n+pub fn target() -> u32 { 2 }\r"})).collect()
                                } else {
                                    vec![]
                                };
                                (200, json!(rows))
                            }
                            "/repos/acme/demo/pulls/7/files?per_page=100&page=1"
                                if matches!(mode, Mode::ShortFiles) =>
                            {
                                (200, json!([]))
                            }
                            "/repos/acme/demo/pulls/7/files?per_page=100&page=1" => (
                                200,
                                json!([{
                                "sha":if matches!(mode,Mode::BlobMismatch) {"0".repeat(40)} else {blob_sha(SOURCE.as_bytes())},
                                "filename":PATH,"status":if matches!(mode,Mode::Removed) {"removed"} else {"modified"},"additions":if matches!(mode,Mode::Removed) {0} else {1},"deletions":if matches!(mode,Mode::Removed) {3} else {1},"changes":if matches!(mode,Mode::Removed) {3} else {2},
                                "extra":if matches!(mode,Mode::ReflectedToken) {TOKEN} else {"inert"},
                                "patch":if matches!(mode,Mode::Removed) {"@@ -1,3 +0,0 @@\n-// café\r\n-pub fn target() -> u32 { 2 }\r\n-pub fn caller() -> u32 { target() }\r"} else {"@@ -2 +2 @@\n-pub fn target() -> u32 { 1 }\r\n+pub fn target() -> u32 { 2 }\r"}}]),
                            ),
                            _ => (404, json!({})),
                        };
                        let value = value.to_string();
                        let link = if matches!(mode, Mode::Paginated | Mode::PaginatedUnsafe)
                            && path.ends_with("files?per_page=100&page=1")
                        {
                            let origin = if matches!(mode, Mode::PaginatedUnsafe) {
                                "https://untrusted.invalid".into()
                            } else {
                                format!("http://{endpoint}")
                            };
                            format!(
                                "Link: <{origin}/repos/acme/demo/pulls/7/files?per_page=100&page=2>; rel=\"next\"\r\n"
                            )
                        } else if matches!(mode, Mode::UnsafeLink)
                            && path.ends_with("files?per_page=100&page=1")
                        {
                            "Link: <https://untrusted.invalid/steal>; rel=\"next\"\r\n".into()
                        } else {
                            String::new()
                        };
                        let response = format!(
                            "HTTP/1.1 {status} Fixture\r\n{link}Content-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{value}",
                            value.len()
                        );
                        if matches!(mode, Mode::StalledFiles) && path.contains("/files?") {
                            let header_end = response.find("\r\n\r\n").unwrap() + 4;
                            stream
                                .write_all(&response.as_bytes()[..header_end + 1])
                                .unwrap();
                            hook.take().expect("owned mid-body cancellation hook")();
                            let mut byte = [0u8; 1];
                            assert_eq!(
                                stream.read(&mut byte).unwrap(),
                                0,
                                "cancelled owned response must close its connection"
                            );
                            continue;
                        }
                        let _ = stream.write_all(response.as_bytes());
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(2))
                    }
                    Err(_) => break,
                }
            }
        });
        Self {
            endpoint,
            seen,
            stop,
            worker: Some(worker),
        }
    }
}
impl Drop for Provider {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(worker) = self.worker.take() {
            worker.join().unwrap();
        }
    }
}
fn remote() -> worker::RunOptions {
    worker::RunOptions {
        allow_network: true,
        ..Default::default()
    }
}

#[test]
fn native_run_binds_exact_crlf_utf8_source_and_finishes_without_intermediate_artifacts() {
    let f = Fixture::new();
    let provider = Provider::new(Mode::Exact);
    let mut client = f.client(&provider);
    let before = f.blobs();
    let result = worker::run(&f.context(), &mut client, &f.request(), &remote(), &f.d).unwrap();
    assert_eq!(result["status"], "completed");
    assert_eq!(result["workerExecutionVerified"], true);
    assert_eq!(result["installationTokenVerified"], true);
    assert_eq!(result["currentHeadVerified"], true);
    assert_eq!(result["sourceWritten"], false);
    assert_eq!(result["published"], false);
    assert_eq!(result["complete"], false);
    assert_eq!(result["modelCalls"], 0);
    assert_eq!(result["networkRequests"], 8);
    assert_eq!(result["review"]["headSnapshotId"], f.snapshot);
    assert_eq!(
        result["review"]["providerReads"]["initialObservedAtUnix"],
        NOW
    );
    assert_eq!(
        result["review"]["providerReads"]["finalObservedAtUnix"],
        NOW
    );
    let file = &result["review"]["impact"]["files"][0];
    assert_eq!(file["hunksVerified"], true);
    assert_eq!(file["hunks"][0]["anchor"]["startByte"], "// café\r\n".len());
    assert_eq!(
        file["sourceBinding"]["contentHash"],
        yoke::model::hash(SOURCE.as_bytes())
    );
    assert_eq!(f.blobs(), before);
    assert_eq!(f.rows(), 2);
    assert_eq!(
        std::fs::read(f.root.path().join(PATH)).unwrap(),
        SOURCE.as_bytes()
    );
    assert!(!result.to_string().contains(TOKEN));
    let seen = provider.seen.lock().unwrap();
    assert_eq!(seen.len(), 8);
    assert!(
        seen.iter()
            .skip(3)
            .all(|request| request.installation_token)
    );
    assert_eq!(
        seen.iter()
            .filter(|request| request.path == "/repos/acme/demo/pulls/7")
            .count(),
        2
    );
}

#[test]
fn completed_request_retry_is_historical_after_expiry_and_changed_source_cannot_rebind_it() {
    let f = Fixture::new();
    let provider = Provider::new(Mode::Exact);
    let mut client = f.client(&provider);
    let request = f.request();
    let first = worker::run(&f.context(), &mut client, &request, &remote(), &f.d).unwrap();
    f.time.store(NOW + 121, Ordering::SeqCst);
    let repeat = worker::run(&f.context(), &mut client, &request, &remote(), &f.d).unwrap();
    assert_eq!(repeat["completionId"], first["completionId"]);
    assert_eq!(repeat["historical"], true);
    assert_eq!(provider.seen.lock().unwrap().len(), 8);
    assert_eq!(f.rows(), 2);
    let mut changed = request;
    changed.head_snapshot_id = "f".repeat(64);
    let error = worker::run(&f.context(), &mut client, &changed, &remote(), &f.d).unwrap_err();
    assert_eq!(error.to_string(), "github_app_worker_request_conflict");
    assert_eq!(provider.seen.lock().unwrap().len(), 8);
}

#[test]
fn completed_job_is_terminal_and_a_fresh_claim_cannot_erase_its_completion() {
    let f = Fixture::new();
    let provider = Provider::new(Mode::Exact);
    let mut client = f.client(&provider);
    let result = worker::run(&f.context(), &mut client, &f.request(), &remote(), &f.d).unwrap();
    let request = worker::ClaimRequest {
        schema_version: 1,
        job_id: f.claim.evidence()["jobId"].as_str().unwrap().into(),
        expected_queue_revision: 2,
        worker: "second-worker".into(),
        request_id: "89abcdef-0123-4abc-8000-000000000003".into(),
        lease_seconds: 120,
    };
    let error = worker::claim(
        &f.engine.store,
        &f.host,
        &request,
        &f.options,
        &f.clock,
        &f.d,
    )
    .err()
    .expect("completed job must refuse a new claim");
    assert_eq!(error.to_string(), "github_app_worker_job_completed");
    let status = worker::status(
        &f.engine.store,
        &f.host,
        &worker::StatusRequest::default(),
        &f.options,
        &f.clock,
        &f.d,
    )
    .unwrap();
    assert_eq!(status["jobs"][0]["status"], "completed");
    assert_eq!(status["jobs"][0]["completionId"], result["completionId"]);
    assert_eq!(f.rows(), 2);
}

#[test]
fn racing_identical_run_keeps_one_completion_and_reports_the_attempts_real_requests() {
    let f = Fixture::new();
    let root = f.root.path().to_owned();
    let request = f.request();
    let second_request = request.clone();
    let time = f.time.clone();
    let completed = Arc::new(Mutex::new(None));
    let observed = completed.clone();
    let provider = Provider::with_hook(
        Mode::Exact,
        Some(Box::new(move || {
            let store = yoke::store::Store::open(&root).unwrap();
            let host = host();
            let options = worker::Options {
                queue: app::Options {
                    allow_writes: true,
                    ..Default::default()
                },
                ..Default::default()
            };
            let clock = worker::Clock::for_fixture(time.clone());
            let d = Deadline::new(10_000);
            let second_provider = Provider::new(Mode::Exact);
            let mut client = fixture_client(
                second_provider.endpoint,
                time,
                &d,
                json!({"contents":"read","pull_requests":"read"}),
            );
            let context = worker::RunContext {
                store: &store,
                host: &host,
                options: &options,
                clock: &clock,
            };
            let value = worker::run(&context, &mut client, &second_request, &remote(), &d).unwrap();
            assert_eq!(second_provider.seen.lock().unwrap().len(), 8);
            *observed.lock().unwrap() = Some(value);
        })),
    );
    let mut client = f.client(&provider);
    let result = worker::run(&f.context(), &mut client, &request, &remote(), &f.d).unwrap();
    let other = completed.lock().unwrap().take().unwrap();
    assert_eq!(result["completionId"], other["completionId"]);
    assert_eq!(result["historical"], true);
    assert_eq!(result["networkRequests"], 8);
    assert_eq!(provider.seen.lock().unwrap().len(), 8);
    assert_eq!(f.rows(), 2);
}

#[test]
fn pull_request_read_permission_alone_can_finish_existing_source_analysis() {
    let f = Fixture::new();
    let provider = Provider::new(Mode::Minimal);
    let mut client = fixture_client(
        provider.endpoint,
        f.time.clone(),
        &f.d,
        json!({"pull_requests":"read"}),
    );
    assert_eq!(
        worker::run(&f.context(), &mut client, &f.request(), &remote(), &f.d).unwrap()["status"],
        "completed"
    );
}

#[test]
fn provider_count_fork_draft_coverage_links_and_token_reflections_refuse() {
    for mode in [
        Mode::FinalCountChanged,
        Mode::FinalForkChanged,
        Mode::MissingDraft,
        Mode::ShortFiles,
        Mode::UnsafeLink,
        Mode::ReflectedToken,
    ] {
        let f = Fixture::new();
        let provider = Provider::new(mode);
        let mut client = f.client(&provider);
        let result = worker::run(&f.context(), &mut client, &f.request(), &remote(), &f.d);
        assert!(result.is_err());
        assert!(!result.err().unwrap().to_string().contains(TOKEN));
        assert_eq!(f.rows(), 1);
    }
}

#[test]
fn removed_files_use_only_an_explicit_base_and_label_the_base_commit() {
    let f = Fixture::new();
    let provider = Provider::new(Mode::Removed);
    let mut client = f.client(&provider);
    let mut request = f.request();
    assert_eq!(
        worker::run(&f.context(), &mut client, &request, &remote(), &f.d)
            .unwrap_err()
            .to_string(),
        "github_app_worker_source_unavailable"
    );
    request.base_snapshot_id = Some(f.snapshot.clone());
    let value = worker::run(&f.context(), &mut client, &request, &remote(), &f.d).unwrap();
    let impact = &value["review"]["baseImpact"];
    assert_eq!(impact["reportedBaseSha"], "b".repeat(40));
    assert!(impact.get("reportedHeadSha").is_none());
    assert_eq!(impact["files"][0]["reportedBaseSha"], "b".repeat(40));
    assert_eq!(impact["files"][0]["hunksVerified"], true);
}

#[test]
fn final_source_seals_detect_changes_to_graph_and_absent_scope_rows() {
    for add_absent in [false, true] {
        let f = Fixture::new();
        let db_path = f.engine.store.dir.join("index.sqlite");
        let snapshot = f.snapshot.clone();
        let provider = Provider::with_hook(
            Mode::Exact,
            Some(Box::new(move || {
                let db = rusqlite::Connection::open(db_path).unwrap();
                db.busy_timeout(Duration::from_millis(50)).unwrap();
                if add_absent {
                    db.execute("INSERT INTO records(kind,id,value) VALUES('native_capture_scope_derived_v1',?1,'{}')",[snapshot]).unwrap();
                } else {
                    db.execute("UPDATE graphs SET value=value||' ' WHERE id=?1", [snapshot])
                        .unwrap();
                }
            })),
        );
        let mut client = f.client(&provider);
        assert_eq!(
            worker::run(&f.context(), &mut client, &f.request(), &remote(), &f.d)
                .unwrap_err()
                .to_string(),
            "github_app_worker_source_unavailable"
        );
        assert_eq!(f.rows(), 1);
        assert_eq!(provider.seen.lock().unwrap().len(), 8);
    }
}

#[test]
fn lease_expiry_and_new_queue_revision_during_http_prevent_completion() {
    for expire in [false, true] {
        let f = Fixture::new();
        let root = f.root.path().to_owned();
        let time = f.time.clone();
        let provider = Provider::with_hook(
            Mode::Exact,
            Some(Box::new(move || {
                if expire {
                    time.store(NOW + 120, Ordering::SeqCst);
                } else {
                    let store = yoke::store::Store::open(&root).unwrap();
                    let mut raw: Value = serde_json::from_slice(&raw_event()).unwrap();
                    raw["pull_request"]["head"]["sha"] = json!("c".repeat(40));
                    let raw = serde_json::to_vec(&raw).unwrap();
                    let mut value = serde_json::to_value(headers(&raw)).unwrap();
                    value["deliveryId"] = json!("01234567-89ab-4cde-8000-000000000002");
                    let headers =
                        app::Headers::parse(&serde_json::to_vec(&value).unwrap()).unwrap();
                    app::ingest(
                        &store,
                        &host(),
                        &headers,
                        &raw,
                        &app::Options {
                            allow_writes: true,
                            ..Default::default()
                        },
                        &Deadline::new(5000),
                    )
                    .unwrap();
                }
            })),
        );
        let mut client = f.client(&provider);
        let error =
            worker::run(&f.context(), &mut client, &f.request(), &remote(), &f.d).unwrap_err();
        assert_eq!(
            error.to_string(),
            if expire {
                "github_app_worker_lease_stale"
            } else {
                "github_app_worker_queue_stale"
            }
        );
        assert_eq!(f.rows(), 1);
    }
}

#[test]
fn historical_result_checks_source_again_but_status_remains_historical() {
    let f = Fixture::new();
    let provider = Provider::new(Mode::Exact);
    let mut client = f.client(&provider);
    worker::run(&f.context(), &mut client, &f.request(), &remote(), &f.d).unwrap();
    let path = f
        .engine
        .store
        .dir
        .join("blobs")
        .join(yoke::model::hash(SOURCE));
    std::fs::write(path, b"corrupt").unwrap();
    assert_eq!(
        worker::run(&f.context(), &mut client, &f.request(), &remote(), &f.d)
            .unwrap_err()
            .to_string(),
        "github_app_worker_source_unavailable"
    );
    assert_eq!(provider.seen.lock().unwrap().len(), 8);
    let value = worker::status(
        &f.engine.store,
        &f.host,
        &worker::StatusRequest::default(),
        &f.options,
        &f.clock,
        &f.d,
    )
    .unwrap();
    assert_eq!(value["jobs"][0]["status"], "completed");
    assert_eq!(value["currentHeadVerified"], false);
}

#[test]
fn report_and_output_refusals_leave_no_completion() {
    for report in [false, true] {
        let f = Fixture::new();
        let provider = Provider::new(Mode::Exact);
        let mut client = f.client(&provider);
        let mut limits = remote();
        let mut options = f.options.clone();
        if report {
            limits.max_report_bytes = 1;
        } else {
            options.max_output_bytes = 1;
        }
        let context = worker::RunContext {
            options: &options,
            ..f.context()
        };
        assert_eq!(
            worker::run(&context, &mut client, &f.request(), &limits, &f.d)
                .unwrap_err()
                .to_string(),
            "github_app_worker_capacity_exceeded"
        );
        assert_eq!(f.rows(), 1);
    }
}

#[test]
fn exact_historical_report_obeys_the_current_report_byte_budget_without_http() {
    let f = Fixture::new();
    let provider = Provider::new(Mode::Exact);
    let mut client = f.client(&provider);
    let value = worker::run(&f.context(), &mut client, &f.request(), &remote(), &f.d).unwrap();
    let bytes = serde_json::to_vec(&value["review"]).unwrap().len();
    let mut limits = remote();
    limits.max_report_bytes = bytes;
    assert_eq!(
        worker::run(&f.context(), &mut client, &f.request(), &limits, &f.d).unwrap()["completionId"],
        value["completionId"]
    );
    limits.max_report_bytes -= 1;
    assert_eq!(
        worker::run(&f.context(), &mut client, &f.request(), &limits, &f.d)
            .unwrap_err()
            .to_string(),
        "github_app_worker_capacity_exceeded"
    );
    assert_eq!(provider.seen.lock().unwrap().len(), 8);
    assert_eq!(f.rows(), 2);
}

#[test]
fn lease_expiry_observed_during_source_preparation_refuses_before_token_minting() {
    use rusqlite::hooks::{AuthAction, AuthContext, Authorization};
    let f = Fixture::new();
    let time = f.time.clone();
    let observed = Arc::new(AtomicBool::new(false));
    let mark = observed.clone();
    f.engine
        .store
        .db
        .authorizer(Some(move |context: AuthContext<'_>| {
            if matches!(
                context.action,
                AuthAction::Read {
                    table_name: "graphs",
                    ..
                }
            ) {
                mark.store(true, Ordering::SeqCst);
                time.store(NOW + 120, Ordering::SeqCst);
            }
            Authorization::Allow
        }));
    let provider = Provider::new(Mode::Exact);
    let mut client = f.client(&provider);
    assert_eq!(
        worker::run(&f.context(), &mut client, &f.request(), &remote(), &f.d)
            .unwrap_err()
            .to_string(),
        "github_app_worker_lease_stale"
    );
    assert!(observed.load(Ordering::SeqCst));
    assert!(
        provider.seen.lock().unwrap().is_empty(),
        "stale source prelude must not mint a token or start remote reads"
    );
    assert_eq!(f.rows(), 1);
}

#[test]
fn typed_pagination_covers_all_files_and_reserves_the_final_observation() {
    let f = Fixture::new();
    let provider = Provider::new(Mode::Paginated);
    let mut client = f.client(&provider);
    let value = worker::run(&f.context(), &mut client, &f.request(), &remote(), &f.d).unwrap();
    assert_eq!(value["networkRequests"], 9);
    assert_eq!(value["review"]["provider"]["changedFiles"], 101);
    assert_eq!(
        value["review"]["impact"]["files"].as_array().unwrap().len(),
        101
    );
    assert_eq!(
        provider
            .seen
            .lock()
            .unwrap()
            .iter()
            .filter(|v| v.path.contains("/files?"))
            .count(),
        2
    );

    let f = Fixture::new();
    let provider = Provider::new(Mode::Paginated);
    let mut client = f.client(&provider);
    let mut limits = remote();
    limits.max_requests = 4;
    assert_eq!(
        worker::run(&f.context(), &mut client, &f.request(), &limits, &f.d)
            .unwrap_err()
            .to_string(),
        "github_app_auth_capacity_exceeded"
    );
    assert_eq!(provider.seen.lock().unwrap().len(), 6);
    assert_eq!(f.rows(), 1);

    let f = Fixture::new();
    let provider = Provider::new(Mode::PaginatedUnsafe);
    let mut client = f.client(&provider);
    assert!(worker::run(&f.context(), &mut client, &f.request(), &remote(), &f.d).is_err());
    assert_eq!(provider.seen.lock().unwrap().len(), 7);
    assert_eq!(f.rows(), 1);
}

#[test]
fn final_completion_writer_conflict_is_causal_bounded_and_restores_sql_policy() {
    let f = Fixture::new();
    let db_path = f.engine.store.dir.join("index.sqlite");
    let held = Arc::new(Mutex::new(None));
    let holder = held.clone();
    let started = Arc::new(Mutex::new(None));
    let observed = started.clone();
    let provider = Provider::with_hook(
        Mode::Exact,
        Some(Box::new(move || {
            let db = rusqlite::Connection::open(db_path).unwrap();
            db.busy_timeout(Duration::from_millis(50)).unwrap();
            db.execute_batch("BEGIN IMMEDIATE").unwrap();
            *holder.lock().unwrap() = Some(db);
            *observed.lock().unwrap() = Some(std::time::Instant::now());
        })),
    );
    f.engine
        .store
        .db
        .busy_timeout(Duration::from_millis(137))
        .unwrap();
    let mut client = f.client(&provider);
    let error = worker::run(&f.context(), &mut client, &f.request(), &remote(), &f.d).unwrap_err();
    assert!(
        matches!(error.downcast_ref::<rusqlite::Error>(),Some(rusqlite::Error::SqliteFailure(code,_)) if code.code==rusqlite::ErrorCode::DatabaseBusy)
    );
    assert!(started.lock().unwrap().unwrap().elapsed() < Duration::from_millis(750));
    assert_eq!(
        f.engine
            .store
            .db
            .query_row("PRAGMA busy_timeout", [], |r| r.get::<_, u64>(0))
            .unwrap(),
        137
    );
    assert!(f.engine.store.db.is_autocommit());
    held.lock()
        .unwrap()
        .take()
        .unwrap()
        .execute_batch("ROLLBACK")
        .unwrap();
    assert_eq!(f.rows(), 1);
}

#[test]
fn execution_restores_the_callers_actual_limits_and_deadline_handler() {
    use rusqlite::limits::Limit;
    for success in [false, true] {
        let f = Fixture::new();
        let stop = Arc::new(AtomicBool::new(false));
        let prior =
            Deadline::run_bounded(&f.d, stop.clone(), || Ok(Deadline::new(30_000))).unwrap();
        f.engine.store.set_deadline(&prior).unwrap();
        f.engine
            .store
            .db
            .busy_timeout(Duration::from_millis(137))
            .unwrap();
        f.engine
            .store
            .db
            .set_limit(Limit::SQLITE_LIMIT_LENGTH, 12_000_000)
            .unwrap();
        let provider = Provider::new(if success {
            Mode::Exact
        } else {
            Mode::BlobMismatch
        });
        let mut client = f.client(&provider);
        assert_eq!(
            worker::run(&f.context(), &mut client, &f.request(), &remote(), &f.d).is_ok(),
            success
        );
        assert_eq!(
            f.engine
                .store
                .db
                .query_row("PRAGMA busy_timeout", [], |r| r.get::<_, u64>(0))
                .unwrap(),
            137
        );
        assert_eq!(
            f.engine.store.db.limit(Limit::SQLITE_LIMIT_LENGTH).unwrap(),
            12_000_000
        );
        stop.store(true, Ordering::SeqCst);
        let result=f.engine.store.db.query_row("WITH RECURSIVE w(n) AS (SELECT 1 UNION ALL SELECT n+1 FROM w WHERE n<100000) SELECT sum(n) FROM w",[],|r|r.get::<_,u64>(0));
        assert!(
            matches!(result,Err(rusqlite::Error::SqliteFailure(code,_)) if code.code==rusqlite::ErrorCode::OperationInterrupted)
        );
        f.engine.store.clear_deadline();
    }
}

#[test]
fn source_seals_allow_graph_rows_larger_than_the_worker_record_limit() {
    let f = Fixture::new();
    f.engine
        .store
        .db
        .execute(
            "UPDATE graphs SET value=value||?1 WHERE id=?2",
            rusqlite::params![" ".repeat(5_000_000), f.snapshot],
        )
        .unwrap();
    let provider = Provider::new(Mode::Exact);
    let mut client = f.client(&provider);
    let value = worker::run(&f.context(), &mut client, &f.request(), &remote(), &f.d).unwrap();
    assert_eq!(value["status"], "completed");
    assert!(
        value["review"]["headProof"]["seals"][0]["bytes"]
            .as_u64()
            .unwrap()
            > 5_000_000
    );
    assert_eq!(
        worker::run(&f.context(), &mut client, &f.request(), &remote(), &f.d).unwrap()["completionId"],
        value["completionId"]
    );
}

fn change_completion(f: &Fixture, change: impl FnOnce(&mut Value)) {
    let (old,raw):(String,String)=f.engine.store.db.query_row("SELECT id,value FROM records WHERE kind=?1 AND json_extract(value,'$.event.type')='complete'",[worker::RECORD_KIND],|r|Ok((r.get(0)?,r.get(1)?))).unwrap();
    let mut value: Value = serde_json::from_str(&raw).unwrap();
    change(&mut value);
    value.as_object_mut().unwrap().remove("signature256");
    let mut mac = Hmac::<sha2::Sha256>::new_from_slice(KEY).unwrap();
    mac.update(b"yoke-code-intelligence/github-app-worker/v1\n");
    mac.update(&serde_json::to_vec(&value).unwrap());
    value["signature256"] = json!(format!(
        "sha256={}",
        hex::encode(mac.finalize().into_bytes())
    ));
    let id = yoke::model::identity(&value);
    f.engine
        .store
        .db
        .execute(
            "UPDATE records SET id=?1,value=?2 WHERE kind=?3 AND id=?4",
            rusqlite::params![id, value.to_string(), worker::RECORD_KIND, old],
        )
        .unwrap();
}

#[test]
fn retention_preserves_typed_completion_roots_without_walking_inert_report_ids() {
    let mut f = Fixture::new();
    std::fs::write(f.root.path().join(PATH), "fn abandoned() {}\n").unwrap();
    let abandoned = f.engine.index(&f.d).unwrap().0.id;
    std::fs::write(f.root.path().join(PATH), SOURCE).unwrap();
    assert_eq!(f.engine.index(&f.d).unwrap().0.id, f.snapshot);
    let provider = Provider::new(Mode::Exact);
    let mut client = f.client(&provider);
    worker::run(&f.context(), &mut client, &f.request(), &remote(), &f.d).unwrap();
    change_completion(&f, |value| {
        let review = &mut value["event"]["review"];
        review["impact"]["ownedInertFixtureId"] = json!(abandoned);
        review["impactHash"] = json!(yoke::model::identity(&review["impact"]));
    });
    // This is an authenticated closed report, but its inert JSON is not root authority.
    worker::status(
        &f.engine.store,
        &f.host,
        &worker::StatusRequest::default(),
        &f.options,
        &f.clock,
        &f.d,
    )
    .unwrap();
    std::fs::write(f.root.path().join(PATH), "fn current() {}\n").unwrap();
    f.engine.index(&f.d).unwrap();
    let value =
        yoke::retention::plan(&f.engine.store, &yoke::retention::Policy::default(), &f.d).unwrap();
    let ids = value["snapshotIds"].as_array().unwrap();
    assert!(ids.contains(&json!(abandoned)));
    assert!(!ids.contains(&json!(f.snapshot)));
}

#[test]
fn closed_completion_binding_corruption_refuses_retention_even_with_valid_fixture_mac() {
    let f = Fixture::new();
    let provider = Provider::new(Mode::Exact);
    let mut client = f.client(&provider);
    worker::run(&f.context(), &mut client, &f.request(), &remote(), &f.d).unwrap();
    change_completion(&f, |v| {
        v["event"]["review"]["bindings"][0]["endByte"] = json!(SOURCE.len() - 1)
    });
    assert!(
        yoke::retention::plan(&f.engine.store, &yoke::retention::Policy::default(), &f.d).is_err()
    );
}

#[test]
fn a_client_created_outside_tokio_cannot_run_a_worker_inside_it() {
    let f = Fixture::new();
    let provider = Provider::new(Mode::Exact);
    let mut client = f.client(&provider);
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(async {
        assert_eq!(
            worker::run(&f.context(), &mut client, &f.request(), &remote(), &f.d)
                .unwrap_err()
                .to_string(),
            "github_app_requires_synchronous_host"
        );
    });
    assert!(provider.seen.lock().unwrap().is_empty());
    assert_eq!(f.rows(), 1);
}

#[test]
fn mid_body_file_cancellation_preserves_the_cause_closes_http_and_leaves_no_completion() {
    let f = Fixture::new();
    let stop = Arc::new(AtomicBool::new(false));
    let signal = stop.clone();
    let provider = Provider::with_hook(
        Mode::StalledFiles,
        Some(Box::new(move || signal.store(true, Ordering::SeqCst))),
    );
    let mut client = f.client(&provider);
    let result = Deadline::run_bounded(&f.d, stop.clone(), || {
        let result = worker::run(
            &f.context(),
            &mut client,
            &f.request(),
            &remote(),
            &Deadline::new(30_000),
        );
        assert!(
            result
                .as_ref()
                .expect_err("direct worker cancellation must refuse")
                .to_string()
                .to_ascii_lowercase()
                .contains("cancel")
        );
        result
    });
    assert!(stop.load(Ordering::SeqCst));
    assert!(
        result
            .unwrap_err()
            .to_string()
            .to_ascii_lowercase()
            .contains("cancel")
    );
    assert_eq!(provider.seen.lock().unwrap().len(), 7);
    assert_eq!(f.rows(), 1);
    assert!(f.engine.store.db.is_autocommit());
}

#[test]
fn a_deleted_source_blob_at_final_observation_is_named_unavailable() {
    let f = Fixture::new();
    let blob = f
        .engine
        .store
        .dir
        .join("blobs")
        .join(yoke::model::hash(SOURCE));
    let provider = Provider::with_hook(
        Mode::Exact,
        Some(Box::new(move || std::fs::remove_file(blob).unwrap())),
    );
    let mut client = f.client(&provider);
    assert_eq!(
        worker::run(&f.context(), &mut client, &f.request(), &remote(), &f.d)
            .unwrap_err()
            .to_string(),
        "github_app_worker_source_unavailable"
    );
    assert_eq!(f.rows(), 1);
}

#[test]
fn provider_head_change_at_the_final_metadata_read_cannot_complete_work() {
    let f = Fixture::new();
    let provider = Provider::new(Mode::FinalHeadChanged);
    let mut client = f.client(&provider);
    let before = f.blobs();
    let error = worker::run(&f.context(), &mut client, &f.request(), &remote(), &f.d).unwrap_err();
    assert_eq!(error.to_string(), "github_app_worker_pull_request_stale");
    assert_eq!(provider.seen.lock().unwrap().len(), 8);
    assert_eq!(f.rows(), 1);
    assert_eq!(f.blobs(), before);
}

#[test]
fn local_source_binding_on_a_blob_mismatch_is_not_a_checked_remote_blob_match() {
    let f = Fixture::new();
    let provider = Provider::new(Mode::BlobMismatch);
    let mut client = f.client(&provider);
    let before = f.blobs();
    let error = worker::run(&f.context(), &mut client, &f.request(), &remote(), &f.d).unwrap_err();
    assert_eq!(error.to_string(), "github_app_worker_source_unavailable");
    assert_eq!(f.rows(), 1);
    assert_eq!(f.blobs(), before);
}

#[test]
fn run_refuses_a_caller_transaction_before_any_http_and_preserves_it() {
    let f = Fixture::new();
    let provider = Provider::new(Mode::Exact);
    let mut client = f.client(&provider);
    f.engine.store.db.execute_batch("BEGIN").unwrap();
    let result = worker::run(&f.context(), &mut client, &f.request(), &remote(), &f.d);
    let preserved = !f.engine.store.db.is_autocommit();
    f.engine.store.db.execute_batch("ROLLBACK").unwrap();
    assert_eq!(
        result.unwrap_err().to_string(),
        "github_app_worker_caller_transaction_refused"
    );
    assert!(preserved);
    assert!(provider.seen.lock().unwrap().is_empty());
    assert_eq!(f.rows(), 1);
}

#[test]
fn execution_permissions_and_zero_budgets_precede_sql_and_http() {
    use rusqlite::hooks::{AuthContext, Authorization};
    let f = Fixture::new();
    let provider = Provider::new(Mode::Exact);
    let mut client = f.client(&provider);
    f.engine
        .store
        .db
        .authorizer(Some(|_: AuthContext<'_>| Authorization::Deny));
    let mut options = f.options.clone();
    options.queue.allow_writes = false;
    let context = worker::RunContext {
        options: &options,
        ..f.context()
    };
    let error = worker::run(&context, &mut client, &f.request(), &remote(), &f.d).unwrap_err();
    assert_eq!(error.to_string(), "github_app_permission_required");
    let error = worker::run(
        &f.context(),
        &mut client,
        &f.request(),
        &Default::default(),
        &f.d,
    )
    .unwrap_err();
    assert_eq!(error.to_string(), "github_app_network_permission_required");
    for field in 0..5 {
        let mut options = remote();
        match field {
            0 => options.max_requests = 0,
            1 => options.max_response_bytes = 0,
            2 => options.max_total_bytes = 0,
            3 => options.max_report_bytes = 0,
            _ => options.max_files = 0,
        }
        let error =
            worker::run(&f.context(), &mut client, &f.request(), &options, &f.d).unwrap_err();
        assert_eq!(error.to_string(), "github_app_worker_capacity_exceeded");
    }
    f.engine
        .store
        .db
        .authorizer(None::<fn(AuthContext<'_>) -> Authorization>);
    assert!(provider.seen.lock().unwrap().is_empty());
    assert_eq!(f.rows(), 1);
}

#[test]
fn execution_request_is_closed_duplicate_safe_and_has_no_endpoint_or_clock_selector() {
    let f = Fixture::new();
    let good = serde_json::to_value(f.request()).unwrap();
    assert!(worker::RunRequest::parse(&serde_json::to_vec(&good).unwrap()).is_ok());
    for (field, value) in [
        ("schemaVersion", json!(2)),
        ("claimId", json!("bad")),
        ("headSnapshotId", json!("0")),
        ("baseSnapshotId", json!("bad")),
        ("requestId", json!("BAD-ID")),
        ("endpoint", json!("http://127.0.0.1")),
        ("now", json!(NOW)),
    ] {
        let mut input = good.clone();
        input[field] = value;
        assert!(
            worker::RunRequest::parse(&serde_json::to_vec(&input).unwrap()).is_err(),
            "invalid {field}"
        );
    }
    let duplicate = good.to_string().replacen('{', "{\"schemaVersion\":1,", 1);
    assert!(worker::RunRequest::parse(duplicate.as_bytes()).is_err());
}

#[test]
fn unknown_explicit_snapshot_refuses_without_capture_or_http() {
    let f = Fixture::new();
    let provider = Provider::new(Mode::Exact);
    let mut client = f.client(&provider);
    let before = f.engine.store.latest().unwrap();
    let mut request = f.request();
    request.head_snapshot_id = "f".repeat(64);
    let error = worker::run(&f.context(), &mut client, &request, &remote(), &f.d).unwrap_err();
    assert_eq!(error.to_string(), "github_app_worker_source_unavailable");
    assert!(provider.seen.lock().unwrap().is_empty());
    assert_eq!(f.engine.store.latest().unwrap(), before);
    assert_eq!(f.rows(), 1);
}
