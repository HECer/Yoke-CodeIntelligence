//! Owned Windows provider/source fixtures; no real installation or project execution.
#![cfg(windows)]
#[path = "support/github_app_pages_lifecycle.rs"]
mod page_lifecycle;
#[path = "support/github_app_pages_preview.rs"]
mod protected_pages;
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
    github_app::{self as app, publication, worker},
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
    host_for(42)
}
fn host_for(installation: u64) -> app::Host {
    app::Host::with_signing_key(
        app::HostProfile::parse(
            &serde_json::to_vec(&json!({
        "schemaVersion":1,"appId":123,"secretCredential":"not-read-by-fixture",
        "grants":[{"installationId":installation,"repositoryId":9,"repository":"acme/demo"}]}))
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
    delivery_headers(raw, 1)
}
fn delivery_headers(raw: &[u8], delivery: u32) -> app::Headers {
    let mut mac = Hmac::<sha2::Sha256>::new_from_slice(KEY).unwrap();
    mac.update(raw);
    app::Headers::parse(
        &serde_json::to_vec(&json!({"schemaVersion":1,"event":"pull_request",
        "deliveryId":format!("01234567-89ab-4cde-8000-{delivery:012}"),
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
        Self::new_at(NOW)
    }
    fn new_at(now: u64) -> Self {
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
        let time = Arc::new(AtomicU64::new(now));
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
    #[allow(dead_code)]
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
    client_for_installation(endpoint, time, d, permissions, 42)
}
fn client_for_installation(
    endpoint: SocketAddr,
    time: Arc<AtomicU64>,
    d: &Deadline,
    permissions: Value,
    installation: u64,
) -> auth::Client {
    let profile=auth::AuthProfile::parse(&serde_json::to_vec(&json!({"schemaVersion":1,"appId":123,
            "keyCredential":"not-read-by-fixture","grants":[{"installationId":installation,"repositoryId":9,"repository":"acme/demo"}],
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
#[allow(dead_code)]
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
    Escaping,
    Publishing,
    LostReply,
    NextHead,
    PublishingNext,
    WrongCommentApp,
    RecoveryRead,
    PublishingFault(CommentFault),
    PreflightMutation,
    LostPatchReply,
}
#[derive(Clone, Copy)]
enum CommentFault {
    WrongApp,
    MissingApp,
    WrongActor,
    WrongBody,
    ZeroId,
    WrongUrl,
    TokenReflection,
    WrongStatus,
}
impl Mode {
    fn comments(self) -> bool {
        self.publication() || matches!(self, Self::RecoveryRead)
    }
    fn publication(self) -> bool {
        matches!(
            self,
            Self::Publishing
                | Self::LostReply
                | Self::PublishingNext
                | Self::WrongCommentApp
                | Self::PublishingFault(_)
                | Self::PreflightMutation
                | Self::LostPatchReply
        )
    }
}

#[allow(dead_code)]
struct Seen {
    method: String,
    path: String,
    installation_token: bool,
    comment_body: Option<String>,
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
        Self::configured(mode, hook.take(), 42, None, None)
    }
    fn configured(
        mode: Mode,
        hook: Option<Box<dyn FnOnce() + Send>>,
        installation: u64,
        comment_body: Option<String>,
        numeric_repository: Option<u64>,
    ) -> Self {
        Self::with_number(
            mode,
            hook,
            installation,
            comment_body,
            numeric_repository,
            7,
        )
    }
    fn with_number(
        mode: Mode,
        hook: Option<Box<dyn FnOnce() + Send>>,
        installation: u64,
        comment_body: Option<String>,
        numeric_repository: Option<u64>,
        number: u64,
    ) -> Self {
        Self::with_time(
            mode,
            hook,
            installation,
            comment_body,
            numeric_repository,
            number,
            NOW,
        )
    }
    fn with_time(
        mode: Mode,
        mut hook: Option<Box<dyn FnOnce() + Send>>,
        installation: u64,
        mut comment_body: Option<String>,
        numeric_repository: Option<u64>,
        number: u64,
        now: u64,
    ) -> Self {
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
                        let method = head
                            .lines()
                            .next()
                            .unwrap()
                            .split_whitespace()
                            .next()
                            .unwrap()
                            .to_owned();
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
                            method: method.clone(),
                            path: path.clone(),
                            installation_token,
                            comment_body: if matches!(method.as_str(), "POST" | "PATCH")
                                && path.starts_with("/repos/acme/demo/issues/")
                            {
                                serde_json::from_slice::<Value>(&body)
                                    .ok()
                                    .and_then(|v| v["body"].as_str().map(str::to_owned))
                            } else {
                                None
                            },
                        });
                        let permissions = if mode.publication() {
                            json!({"pull_requests":"write"})
                        } else if matches!(mode, Mode::Minimal | Mode::RecoveryRead) {
                            json!({"pull_requests":"read"})
                        } else {
                            json!({"contents":"read","pull_requests":"read"})
                        };
                        let repository = json!({"id":9,"full_name":"acme/demo","archived":false});
                        let (mut status, mut value) = match path.as_str() {
                            "/app" => (200, json!({"id":123})),
                            route if route == format!("/app/installations/{installation}") => (
                                200,
                                json!({"id":installation,"app_id":123,"suspended_at":null,"suspended_by":null,"permissions":permissions}),
                            ),
                            route
                                if route
                                    == format!(
                                        "/app/installations/{installation}/access_tokens"
                                    ) =>
                            {
                                let request: Value = serde_json::from_slice(&body).unwrap();
                                assert_eq!(request["repository_ids"], json!([9]));
                                assert_eq!(request["permissions"], permissions);
                                let date =
                                    time::OffsetDateTime::from_unix_timestamp((now + 3600) as i64)
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
                            route if route == format!("/repos/acme/demo/pulls/{number}") => {
                                pr_reads += 1;
                                if pr_reads > 1
                                    && (!mode.publication()
                                        || matches!(mode, Mode::PreflightMutation))
                                    && let Some(hook) = hook.take()
                                {
                                    hook();
                                }
                                let changed = matches!(
                                    mode,
                                    Mode::NextHead | Mode::PublishingNext | Mode::LostPatchReply
                                ) || matches!(mode, Mode::FinalHeadChanged)
                                    && pr_reads > 1;
                                let mut head_repository = repository.clone();
                                if matches!(mode, Mode::FinalForkChanged) && pr_reads > 1 {
                                    head_repository["id"] = json!(10);
                                    head_repository["full_name"] = json!("fork/demo");
                                }
                                (
                                    200,
                                    json!({"id":77,"number":number,"state":"open","draft":if matches!(mode,Mode::MissingDraft) {Value::Null} else {json!(false)},"changed_files":if matches!(mode,Mode::Paginated|Mode::PaginatedUnsafe) {101} else if matches!(mode,Mode::FinalCountChanged) && pr_reads>1 {2} else {1},
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
                            route
                                if route
                                    == format!(
                                        "/repos/acme/demo/pulls/{number}/files?per_page=100&page=1"
                                    ) =>
                            {
                                (
                                    200,
                                    json!([{
                                "sha":if matches!(mode,Mode::BlobMismatch) {"0".repeat(40)} else {blob_sha(SOURCE.as_bytes())},
                                "filename":if matches!(mode,Mode::Escaping) {"@team[click](example.com)&'`_.rs"} else {PATH},"status":if matches!(mode,Mode::Removed) {"removed"} else {"modified"},"additions":if matches!(mode,Mode::Removed) {0} else {1},"deletions":if matches!(mode,Mode::Removed) {3} else {1},"changes":if matches!(mode,Mode::Removed) {3} else {2},
                                "extra":if matches!(mode,Mode::ReflectedToken) {TOKEN} else {"inert"},
                                "patch":if matches!(mode,Mode::Removed) {"@@ -1,3 +0,0 @@\n-// café\r\n-pub fn target() -> u32 { 2 }\r\n-pub fn caller() -> u32 { target() }\r"} else {"@@ -2 +2 @@\n-pub fn target() -> u32 { 1 }\r\n+pub fn target() -> u32 { 2 }\r"}}]),
                                )
                            }
                            route
                                if mode.publication()
                                    && route
                                        == format!("/repos/acme/demo/issues/{number}/comments") =>
                            {
                                assert_eq!(method, "POST");
                                assert!(installation_token);
                                let request: Value = serde_json::from_slice(&body).unwrap();
                                assert_eq!(request.as_object().unwrap().len(), 1);
                                assert!(
                                    request["body"].as_str().unwrap().starts_with(
                                        "<!-- yoke-code-intelligence/pr-comment/v1 -->"
                                    )
                                );
                                if let Some(hook) = hook.take() {
                                    hook();
                                }
                                comment_body = Some(request["body"].as_str().unwrap().to_owned());
                                if matches!(mode, Mode::LostReply) {
                                    continue;
                                }
                                (
                                    201,
                                    comment(
                                        comment_body.as_deref().unwrap(),
                                        numeric_repository,
                                        number,
                                    ),
                                )
                            }
                            "/repos/acme/demo/issues/comments/501" if mode.comments() => {
                                assert!(installation_token);
                                if method == "PATCH" {
                                    let request: Value = serde_json::from_slice(&body).unwrap();
                                    assert_eq!(request.as_object().unwrap().len(), 1);
                                    comment_body =
                                        Some(request["body"].as_str().unwrap().to_owned());
                                    if matches!(mode, Mode::LostPatchReply) {
                                        continue;
                                    }
                                } else {
                                    assert_eq!(method, "GET");
                                }
                                (
                                    200,
                                    comment(
                                        comment_body.as_deref().expect("owned existing comment"),
                                        numeric_repository,
                                        number,
                                    ),
                                )
                            }
                            _ => (404, json!({})),
                        };
                        if matches!(mode, Mode::WrongCommentApp)
                            && path.contains("/issues/comments/")
                        {
                            value["performed_via_github_app"]["id"] = json!(999);
                        }
                        if method == "POST"
                            && path == format!("/repos/acme/demo/issues/{number}/comments")
                            && let Mode::PublishingFault(fault) = mode
                        {
                            match fault {
                                CommentFault::WrongApp => {
                                    value["performed_via_github_app"]["id"] = json!(999)
                                }
                                CommentFault::MissingApp => {
                                    value["performed_via_github_app"] = Value::Null
                                }
                                CommentFault::WrongActor => value["user"]["type"] = json!("User"),
                                CommentFault::WrongBody => {
                                    value["body"] = json!("foreign edited body")
                                }
                                CommentFault::ZeroId => value["id"] = json!(0),
                                CommentFault::WrongUrl => {
                                    value["url"] = json!("https://untrusted.invalid/steal")
                                }
                                CommentFault::TokenReflection => value["extra"] = json!(TOKEN),
                                CommentFault::WrongStatus => status = 200,
                            }
                        }
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
fn comment(body: &str, numeric_repository: Option<u64>, number: u64) -> Value {
    json!({"id":501,"body":body,"user":{"id":1234,"type":"Bot","login":"owned-yoke-fixture[bot]"},
        "performed_via_github_app":{"id":123},
        "issue_url":format!("https://api.github.com/repos/acme/demo/issues/{number}"),
        "url":numeric_repository.map_or_else(||"https://api.github.com/repos/acme/demo/issues/comments/501".into(), |id|format!("https://api.github.com/repositories/{id}/issues/comments/501")),
        "html_url":format!("https://github.com/acme/demo/pull/{number}#issuecomment-501")})
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

fn completed(f: &Fixture) -> Value {
    let provider = Provider::new(Mode::Exact);
    let result = worker::run(
        &f.context(),
        &mut f.client(&provider),
        &f.request(),
        &remote(),
        &f.d,
    )
    .unwrap();
    assert_eq!(provider.seen.lock().unwrap().len(), 8);
    result
}
fn preview_request(result: &Value) -> publication::PreviewRequest {
    publication::PreviewRequest {
        page: None,
        schema_version: 1,
        completion_id: result["completionId"].as_str().unwrap().into(),
        expected_queue_revision: 2,
    }
}
fn options(f: &Fixture) -> publication::Options {
    publication::Options {
        worker: f.options.clone(),
        ..Default::default()
    }
}
fn fail_marker<T>(result: anyhow::Result<T>, marker: &str) {
    let error = result.err().expect("operation must refuse").to_string();
    assert_eq!(error, marker);
}

// A missing replay/render implementation makes this fail; all source and provider work is real.
#[test]
fn preview_replays_completed_source_without_network_or_logical_writes() {
    let f = Fixture::new();
    let result = completed(&f);
    let rows = f.rows();
    let blobs = f.blobs();
    let request = preview_request(&result);
    let first =
        publication::preview(&f.engine.store, &f.host, &request, &options(&f), &f.d).unwrap();
    let second =
        publication::preview(&f.engine.store, &f.host, &request, &options(&f), &f.d).unwrap();
    assert_eq!(first, second);
    assert_eq!(first["completionId"], result["completionId"]);
    assert_eq!(first["jobId"], result["jobId"]);
    assert_eq!(first["queueRevision"], 2);
    assert_eq!(first["headSnapshotId"], f.snapshot);
    assert_eq!(first["networkRequests"], 0);
    assert_eq!(first["modelCalls"], 0);
    assert_eq!(first["published"], false);
    assert_eq!(first["publicationAuthority"], false);
    assert_eq!(first["currentHeadVerified"], false);
    assert_eq!(first["sourceWritten"], false);
    assert_eq!(first["complete"], false);
    let body = first["body"].as_str().unwrap();
    assert!(body.starts_with("<!-- yoke-code-intelligence/pr-comment/v1 -->\n"));
    assert!(body.contains("Structural candidates"));
    assert!(body.contains("Changed symbols: 1"));
    assert!(body.contains("Incoming candidates: 1"));
    assert!(body.contains("caf&#xE9;.rs"));
    assert!(body.contains(&format!("[0, {})", SOURCE.len())));
    assert!(body.contains("Whole-tree commit equality: not attested"));
    assert!(body.contains("Removed files: 0"));
    assert_eq!(first["bodyHash"], yoke::model::hash(body.as_bytes()));
    assert!(yoke::model::valid_id(first["proposalId"].as_str().unwrap()));
    assert_eq!(f.rows(), rows);
    assert_eq!(f.blobs(), blobs);
    assert_eq!(
        std::fs::read(f.root.path().join(PATH)).unwrap(),
        SOURCE.as_bytes()
    );
    assert!(!first.to_string().contains(TOKEN));
}

#[test]
fn preview_refuses_claim_id_and_current_revision_mismatch() {
    let f = Fixture::new();
    let result = completed(&f);
    let mut request = preview_request(&result);
    request.completion_id = f.claim.evidence()["claimId"].as_str().unwrap().into();
    fail_marker(
        publication::preview(&f.engine.store, &f.host, &request, &options(&f), &f.d),
        "github_app_publication_completion_unavailable",
    );
    request = preview_request(&result);
    request.expected_queue_revision = 1;
    fail_marker(
        publication::preview(&f.engine.store, &f.host, &request, &options(&f), &f.d),
        "github_app_publication_queue_stale",
    );
}

#[test]
fn preview_rechecks_selected_source_bytes_and_restores_sql_policy() {
    use rusqlite::limits::Limit;
    let f = Fixture::new();
    let result = completed(&f);
    let blob = f
        .engine
        .store
        .dir
        .join("blobs")
        .join(yoke::model::hash(SOURCE.as_bytes()));
    std::fs::remove_file(blob).unwrap();
    f.engine
        .store
        .db
        .busy_timeout(Duration::from_millis(873))
        .unwrap();
    let prior = f.engine.store.db.limit(Limit::SQLITE_LIMIT_LENGTH).unwrap();
    fail_marker(
        publication::preview(
            &f.engine.store,
            &f.host,
            &preview_request(&result),
            &options(&f),
            &f.d,
        ),
        "github_app_worker_source_unavailable",
    );
    let busy: u64 = f
        .engine
        .store
        .db
        .query_row("PRAGMA busy_timeout", [], |r| r.get(0))
        .unwrap();
    assert_eq!(busy, 873);
    assert_eq!(
        f.engine.store.db.limit(Limit::SQLITE_LIMIT_LENGTH).unwrap(),
        prior
    );
}

#[test]
fn preview_body_and_complete_stdout_limits_refuse_without_artifacts() {
    let f = Fixture::new();
    let result = completed(&f);
    let request = preview_request(&result);
    let rows = f.rows();
    let blobs = f.blobs();
    let mut o = options(&f);
    o.max_body_bytes = 100;
    fail_marker(
        publication::preview(&f.engine.store, &f.host, &request, &o, &f.d),
        "github_app_publication_capacity_exceeded",
    );
    o = options(&f);
    o.max_output_bytes = 100;
    fail_marker(
        publication::preview(&f.engine.store, &f.host, &request, &o, &f.d),
        "github_app_publication_capacity_exceeded",
    );
    assert_eq!(f.rows(), rows);
    assert_eq!(f.blobs(), blobs);
}

#[test]
fn preview_request_refuses_duplicates_unknowns_and_invalid_identities() {
    let valid = json!({"schemaVersion":1,"completionId":"a".repeat(64),"expectedQueueRevision":2});
    assert!(publication::PreviewRequest::parse(&serde_json::to_vec(&valid).unwrap()).is_ok());
    for (key, value) in [
        ("schemaVersion", json!(2)),
        ("completionId", json!("a".repeat(63))),
        ("expectedQueueRevision", json!(0)),
        ("expectedQueueRevision", json!(2001)),
        ("endpoint", json!("https://untrusted.invalid")),
    ] {
        let mut bad = valid.clone();
        bad[key] = value;
        assert!(publication::PreviewRequest::parse(&serde_json::to_vec(&bad).unwrap()).is_err());
    }
    let duplicate = format!(
        "{{\"schemaVersion\":1,\"completionId\":\"{}\",\"expectedQueueRevision\":2,\"expectedQueueRevision\":2}}",
        "a".repeat(64)
    );
    assert!(publication::PreviewRequest::parse(duplicate.as_bytes()).is_err());
}

#[test]
fn preview_refuses_caller_write_transaction_and_preserves_its_ownership() {
    let f = Fixture::new();
    let result = completed(&f);
    f.engine.store.db.execute_batch("BEGIN IMMEDIATE").unwrap();
    let preview = publication::preview(
        &f.engine.store,
        &f.host,
        &preview_request(&result),
        &options(&f),
        &f.d,
    );
    assert!(
        !f.engine.store.db.is_autocommit(),
        "caller transaction must remain open"
    );
    f.engine.store.db.execute_batch("ROLLBACK").unwrap();
    fail_marker(preview, "github_app_publication_caller_transaction_refused");
}

#[test]
fn preview_exact_body_and_stdout_budgets_include_the_final_newline() {
    let f = Fixture::new();
    let result = completed(&f);
    let request = preview_request(&result);
    let value =
        publication::preview(&f.engine.store, &f.host, &request, &options(&f), &f.d).unwrap();
    let body_len = value["body"].as_str().unwrap().len();
    let stdout_len = serde_json::to_vec(&value).unwrap().len() + 1;
    let mut o = options(&f);
    o.max_body_bytes = body_len;
    o.max_output_bytes = stdout_len;
    assert_eq!(
        publication::preview(&f.engine.store, &f.host, &request, &o, &f.d).unwrap(),
        value
    );
    o.max_output_bytes -= 1;
    fail_marker(
        publication::preview(&f.engine.store, &f.host, &request, &o, &f.d),
        "github_app_publication_capacity_exceeded",
    );
    o.max_output_bytes += 1;
    o.max_body_bytes -= 1;
    fail_marker(
        publication::preview(&f.engine.store, &f.host, &request, &o, &f.d),
        "github_app_publication_capacity_exceeded",
    );
}

#[test]
fn preview_displays_hostile_filename_as_inert_code_without_source_quotes() {
    let mut f = Fixture::new();
    let path = "@team[click](example.com)&'`_.rs";
    std::fs::write(f.root.path().join(path), SOURCE.as_bytes()).unwrap();
    f.snapshot = f.engine.index(&f.d).unwrap().0.id;
    let provider = Provider::new(Mode::Escaping);
    let result = worker::run(
        &f.context(),
        &mut f.client(&provider),
        &f.request(),
        &remote(),
        &f.d,
    )
    .unwrap();
    let value = publication::preview(
        &f.engine.store,
        &f.host,
        &preview_request(&result),
        &options(&f),
        &f.d,
    )
    .unwrap();
    let body = value["body"].as_str().unwrap();
    assert!(body.contains(
        "<code>&#x40;team&#x5B;click&#x5D;&#x28;example.com&#x29;&#x26;&#x27;&#x60;_.rs</code>"
    ));
    assert!(!body.contains("@team"));
    assert!(!body.contains("[click]"));
    assert!(!body.contains("pub fn target"));
    assert!(!body.contains("javascript:"));
}

#[test]
fn preview_rechecks_current_grant_and_authenticated_completion_body() {
    use rusqlite::params;
    let f = Fixture::new();
    let result = completed(&f);
    let profile = app::HostProfile::parse(&serde_json::to_vec(&json!({"schemaVersion":1,"appId":123,
        "secretCredential":"not-read-by-fixture","grants":[{"installationId":42,"repositoryId":10,"repository":"acme/other"}]})).unwrap()).unwrap();
    let revoked =
        app::Host::with_signing_key(profile, app::SigningKey::from_host_bytes(KEY).unwrap())
            .unwrap();
    fail_marker(
        publication::preview(
            &f.engine.store,
            &revoked,
            &preview_request(&result),
            &options(&f),
            &f.d,
        ),
        "github_app_worker_grant_denied",
    );
    let id = result["completionId"].as_str().unwrap();
    let raw: String = f
        .engine
        .store
        .db
        .query_row(
            "SELECT value FROM records WHERE kind=?1 AND id=?2",
            params![worker::RECORD_KIND, id],
            |r| r.get(0),
        )
        .unwrap();
    let mut corrupted: Value = serde_json::from_str(&raw).unwrap();
    corrupted["event"]["review"]["impact"]["changedSymbols"] = json!([]);
    corrupted["event"]["review"]["impactHash"] = json!(yoke::model::identity(
        &corrupted["event"]["review"]["impact"]
    ));
    let new_id = yoke::model::identity(&corrupted);
    // Even a newly canonical ID does not authenticate a modified signed report.
    f.engine
        .store
        .db
        .execute(
            "UPDATE records SET id=?1,value=?2 WHERE kind=?3 AND id=?4",
            params![new_id, corrupted.to_string(), worker::RECORD_KIND, id],
        )
        .unwrap();
    let mut tampered_request = preview_request(&result);
    tampered_request.completion_id = new_id;
    fail_marker(
        publication::preview(
            &f.engine.store,
            &f.host,
            &tampered_request,
            &options(&f),
            &f.d,
        ),
        "github_app_worker_binding_invalid",
    );
}

#[test]
fn proposal_identity_independently_binds_source_job_scope_revision_and_exact_body() {
    use sha2::{Digest, Sha256};
    let f = Fixture::new();
    let result = completed(&f);
    let value = publication::preview(
        &f.engine.store,
        &f.host,
        &preview_request(&result),
        &options(&f),
        &f.d,
    )
    .unwrap();
    // Literal contract values come from the authenticated worker/input, not the renderer's field map.
    let body = value["body"].as_str().unwrap();
    let review_hash = hex::encode(Sha256::digest(
        serde_json::to_vec(&result["review"]).unwrap(),
    ));
    let body_hash = hex::encode(Sha256::digest(body.as_bytes()));
    let contract = json!({"schemaVersion":1,"rendererVersion":"yoke-pr-comment-v1",
        "workspaceId":f.engine.store.workspace_id,"appId":123,"installationId":42,
        "repositoryId":9,"repository":"acme/demo","number":7,
        "jobId":result["jobId"],"completionId":result["completionId"],"queueRevision":2,
        "headSha":"a".repeat(40),"baseSha":"b".repeat(40),
        "headSnapshotId":f.snapshot,"baseSnapshotId":null,"reviewHash":review_hash,
        "bodyHash":body_hash,"body":body});
    let expected = hex::encode(Sha256::digest(serde_json::to_vec(&contract).unwrap()));
    assert_eq!(value["proposalId"], expected);
}

#[test]
fn preview_requires_selected_job_but_can_bind_explicit_reselection_at_new_revision() {
    let f = Fixture::new();
    let result = completed(&f);
    let first = publication::preview(
        &f.engine.store,
        &f.host,
        &preview_request(&result),
        &options(&f),
        &f.d,
    )
    .unwrap();
    let mut event: Value = serde_json::from_slice(&raw_event()).unwrap();
    event["action"] = json!("synchronize");
    event["pull_request"]["head"]["sha"] = json!("c".repeat(40));
    let raw = serde_json::to_vec(&event).unwrap();
    let mut mac = Hmac::<sha2::Sha256>::new_from_slice(KEY).unwrap();
    mac.update(&raw);
    let headers = app::Headers::parse(
        &serde_json::to_vec(&json!({"schemaVersion":1,"event":"pull_request",
        "deliveryId":"01234567-89ab-4cde-8000-000000000002",
        "signature256":format!("sha256={}",hex::encode(mac.finalize().into_bytes()))}))
        .unwrap(),
    )
    .unwrap();
    let admitted = app::ingest(
        &f.engine.store,
        &f.host,
        &headers,
        &raw,
        &f.options.queue,
        &f.d,
    )
    .unwrap();
    app::select(
        &f.engine.store,
        &f.host,
        admitted["jobId"].as_str().unwrap(),
        3,
        &f.options.queue,
        &f.d,
    )
    .unwrap();
    let mut request = preview_request(&result);
    request.expected_queue_revision = 4;
    fail_marker(
        publication::preview(&f.engine.store, &f.host, &request, &options(&f), &f.d),
        "github_app_publication_queue_stale",
    );
    app::select(
        &f.engine.store,
        &f.host,
        result["jobId"].as_str().unwrap(),
        4,
        &f.options.queue,
        &f.d,
    )
    .unwrap();
    request.expected_queue_revision = 5;
    let selected =
        publication::preview(&f.engine.store, &f.host, &request, &options(&f), &f.d).unwrap();
    assert_eq!(selected["queueRevision"], 5);
    assert_eq!(selected["bodyHash"], first["bodyHash"]);
    assert_ne!(selected["proposalId"], first["proposalId"]);
    assert_eq!(selected["publicationAuthority"], false);
}

fn publish_request(result: &Value, preview: &Value) -> publication::PublishRequest {
    publication::PublishRequest {
        page: None,
        schema_version: 1,
        completion_id: result["completionId"].as_str().unwrap().into(),
        expected_queue_revision: 2,
        proposal_id: preview["proposalId"].as_str().unwrap().into(),
        body_hash: preview["bodyHash"].as_str().unwrap().into(),
        request_id: "89abcdef-0123-4abc-8000-000000000007".into(),
        approval: "publish_exact_comment".into(),
    }
}
fn publication_context<'a>(
    f: &'a Fixture,
    o: &'a publication::Options,
) -> publication::Context<'a> {
    publication::Context {
        store: &f.engine.store,
        host: &f.host,
        options: o,
        clock: &f.clock,
    }
}
fn remote_publish() -> publication::PublishOptions {
    publication::PublishOptions {
        allow_network: true,
        ..Default::default()
    }
}
#[test]
fn publication_request_requires_closed_explicit_exact_approval() {
    let valid = json!({"schemaVersion":1,"completionId":"a".repeat(64),"expectedQueueRevision":2,
        "proposalId":"b".repeat(64),"bodyHash":"c".repeat(64),
        "requestId":"89abcdef-0123-4abc-8000-000000000007","approval":"publish_exact_comment"});
    assert!(publication::PublishRequest::parse(&serde_json::to_vec(&valid).unwrap()).is_ok());
    for (key, value) in [
        ("approval", json!("approved")),
        ("requestId", json!("89ABCDEF-0123-4abc-8000-000000000007")),
        ("bodyHash", json!("c".repeat(63))),
        ("expectedQueueRevision", json!(0)),
        ("endpoint", json!("https://untrusted.invalid")),
    ] {
        let mut bad = valid.clone();
        bad[key] = value;
        assert!(publication::PublishRequest::parse(&serde_json::to_vec(&bad).unwrap()).is_err());
    }
    let raw = serde_json::to_string(&valid).unwrap();
    let duplicate = raw.replacen('{', "{\"approval\":\"publish_exact_comment\",", 1);
    assert!(publication::PublishRequest::parse(duplicate.as_bytes()).is_err());
}
#[test]
fn publication_permissions_precede_history_and_any_authorization_request() {
    let f = Fixture::new();
    let provider = Provider::new(Mode::Publishing);
    let mut client = fixture_client(
        provider.endpoint,
        f.time.clone(),
        &f.d,
        json!({"pull_requests":"write"}),
    );
    let request = publication::PublishRequest {
        page: None,
        schema_version: 1,
        completion_id: "a".repeat(64),
        expected_queue_revision: 2,
        proposal_id: "b".repeat(64),
        body_hash: "c".repeat(64),
        request_id: "89abcdef-0123-4abc-8000-000000000007".into(),
        approval: "publish_exact_comment".into(),
    };
    let mut o = options(&f);
    fail_marker(
        publication::publish(
            &publication_context(&f, &o),
            &mut client,
            &request,
            &remote_publish(),
            &f.d,
        ),
        "github_app_publication_permission_required",
    );
    o.allow_publication = true;
    o.worker.queue.allow_writes = false;
    fail_marker(
        publication::publish(
            &publication_context(&f, &o),
            &mut client,
            &request,
            &remote_publish(),
            &f.d,
        ),
        "github_app_permission_required",
    );
    o.worker.queue.allow_writes = true;
    fail_marker(
        publication::publish(
            &publication_context(&f, &o),
            &mut client,
            &request,
            &publication::PublishOptions::default(),
            &f.d,
        ),
        "github_app_network_permission_required",
    );
    assert!(provider.seen.lock().unwrap().is_empty());
}
#[test]
fn publication_exact_approval_and_budgets_precede_authentication_or_journal_writes() {
    let f = Fixture::new();
    let result = completed(&f);
    let preview = publication::preview(
        &f.engine.store,
        &f.host,
        &preview_request(&result),
        &options(&f),
        &f.d,
    )
    .unwrap();
    let request = publish_request(&result, &preview);
    let mut o = options(&f);
    o.allow_publication = true;
    let provider = Provider::new(Mode::Publishing);
    let mut client = fixture_client(
        provider.endpoint,
        f.time.clone(),
        &f.d,
        json!({"pull_requests":"write"}),
    );
    for field in ["body", "proposal"] {
        let mut altered = request.clone();
        if field == "body" {
            altered.body_hash = "0".repeat(64);
        } else {
            altered.proposal_id = "0".repeat(64);
        }
        fail_marker(
            publication::publish(
                &publication_context(&f, &o),
                &mut client,
                &altered,
                &remote_publish(),
                &f.d,
            ),
            "github_app_publication_approval_mismatch",
        );
    }
    let mut remote = remote_publish();
    remote.max_requests = 0;
    fail_marker(
        publication::publish(
            &publication_context(&f, &o),
            &mut client,
            &request,
            &remote,
            &f.d,
        ),
        "github_app_publication_capacity_exceeded",
    );
    assert!(provider.seen.lock().unwrap().is_empty());
    let count: usize = f
        .engine
        .store
        .db
        .query_row(
            "SELECT count(*) FROM records WHERE kind=?1",
            [publication::RECORD_KIND],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(count, 0);
}
#[test]
fn approved_publication_commits_intent_before_one_post_and_exact_retry_is_historical() {
    let f = Fixture::new();
    let result = completed(&f);
    let preview = publication::preview(
        &f.engine.store,
        &f.host,
        &preview_request(&result),
        &options(&f),
        &f.d,
    )
    .unwrap();
    let request = publish_request(&result, &preview);
    let mut o = options(&f);
    o.allow_publication = true;
    let db_path = f.engine.store.db.path().unwrap().to_owned();
    let provider = Provider::with_hook(
        Mode::Publishing,
        Some(Box::new(move || {
            let db = rusqlite::Connection::open(db_path).unwrap();
            let count: usize = db
                .query_row(
                    "SELECT count(*) FROM records WHERE kind=?1",
                    [publication::RECORD_KIND],
                    |r| r.get(0),
                )
                .unwrap();
            assert_eq!(
                count, 1,
                "one durable intent must be visible before the POST completes"
            );
            db.execute_batch("BEGIN IMMEDIATE; ROLLBACK").unwrap();
        })),
    );
    let mut client = fixture_client(
        provider.endpoint,
        f.time.clone(),
        &f.d,
        json!({"pull_requests":"write"}),
    );
    let published = publication::publish(
        &publication_context(&f, &o),
        &mut client,
        &request,
        &remote_publish(),
        &f.d,
    )
    .unwrap();
    assert_eq!(published["status"], "published");
    assert_eq!(published["commentId"], 501);
    assert_eq!(published["historical"], false);
    assert_eq!(published["networkRequests"], 9);
    assert_eq!(published["bodyHash"], preview["bodyHash"]);
    let retry = publication::publish(
        &publication_context(&f, &o),
        &mut client,
        &request,
        &remote_publish(),
        &f.d,
    )
    .unwrap();
    assert_eq!(retry["commentId"], 501);
    assert_eq!(retry["historical"], true);
    assert_eq!(retry["networkRequests"], 0);
    let seen = provider.seen.lock().unwrap();
    assert_eq!(seen.len(), 9);
    assert_eq!(
        seen.iter()
            .filter(|r| r.method == "POST" && r.path == "/repos/acme/demo/issues/7/comments")
            .count(),
        1
    );
    assert!(seen.iter().skip(3).all(|r| r.installation_token));
    let count: usize = f
        .engine
        .store
        .db
        .query_row(
            "SELECT count(*) FROM records WHERE kind=?1",
            [publication::RECORD_KIND],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(count, 2);
    assert_eq!(
        std::fs::read(f.root.path().join(PATH)).unwrap(),
        SOURCE.as_bytes()
    );
    assert!(!published.to_string().contains(TOKEN));
}

fn publish_with(
    f: &Fixture,
    request: &publication::PublishRequest,
    provider: &Provider,
    installation: u64,
) -> anyhow::Result<Value> {
    let mut o = options(f);
    o.allow_publication = true;
    let mut client = client_for_installation(
        provider.endpoint,
        f.time.clone(),
        &f.d,
        json!({"pull_requests":"write"}),
        installation,
    );
    publication::publish(
        &publication_context(f, &o),
        &mut client,
        request,
        &remote_publish(),
        &f.d,
    )
}
fn published_fixture() -> (Fixture, Value, Value, publication::PublishRequest) {
    let f = Fixture::new();
    let result = completed(&f);
    let preview = publication::preview(
        &f.engine.store,
        &f.host,
        &preview_request(&result),
        &options(&f),
        &f.d,
    )
    .unwrap();
    let request = publish_request(&result, &preview);
    assert_eq!(
        publish_with(&f, &request, &Provider::new(Mode::Publishing), 42).unwrap()["status"],
        "published"
    );
    (f, result, preview, request)
}
fn reselected_request(f: &Fixture, result: &Value) -> publication::PublishRequest {
    app::select(
        &f.engine.store,
        &f.host,
        result["jobId"].as_str().unwrap(),
        2,
        &f.options.queue,
        &f.d,
    )
    .unwrap();
    let mut p = preview_request(result);
    p.expected_queue_revision = 3;
    let preview = publication::preview(&f.engine.store, &f.host, &p, &options(f), &f.d).unwrap();
    let mut request = publish_request(result, &preview);
    request.expected_queue_revision = 3;
    request.request_id = "89abcdef-0123-4abc-8000-000000000008".into();
    request
}
fn check_historical_retry_after_reinstallation(published: bool) {
    let (mut f, _, preview, request) = if published {
        published_fixture()
    } else {
        let (f, result, preview, request, _) = unknown_fixture();
        (f, result, preview, request)
    };
    f.host = host_for(43);
    let status = publication::status(
        &publication_context(&f, &options(&f)),
        &publication::StatusRequest {
            schema_version: 1,
            request_id: request.request_id.clone(),
        },
        &f.d,
    )
    .unwrap();
    assert_eq!(
        status["status"],
        if published { "published" } else { "unknown" }
    );
    let provider = Provider::configured(
        Mode::Publishing,
        None,
        43,
        Some(preview["body"].as_str().unwrap().into()),
        Some(9),
    );
    let retry = publish_with(&f, &request, &provider, 43).unwrap();
    assert_eq!(
        retry["status"],
        if published { "published" } else { "unknown" }
    );
    assert_eq!(retry["historical"], true);
    assert_eq!(retry["published"], published);
    assert_eq!(retry["networkRequests"], 0);
    assert_eq!(retry["publicationAuthority"], false);
    assert_eq!(retry["currentHeadVerified"], false);
    assert_eq!(retry["installationTokenVerified"], false);
    assert_eq!(
        retry["commentId"],
        if published { json!(501) } else { Value::Null }
    );
    let mut conflict = request.clone();
    conflict.body_hash = "0".repeat(64);
    fail_marker(
        publish_with(&f, &conflict, &provider, 43),
        "github_app_publication_request_conflict",
    );
    let mut fresh_send = request.clone();
    fresh_send.request_id = "89abcdef-0123-4abc-8000-000000000009".into();
    fail_marker(
        publish_with(&f, &fresh_send, &provider, 43),
        "github_app_worker_grant_denied",
    );
    for (repository_id, repository) in [(10, "acme/demo"), (9, "acme/other")] {
        f.host = app::Host::with_signing_key(app::HostProfile::parse(&serde_json::to_vec(&json!({
            "schemaVersion":1,"appId":123,"secretCredential":"not-read-by-fixture",
            "grants":[{"installationId":43,"repositoryId":repository_id,"repository":repository}]
        })).unwrap()).unwrap(), app::SigningKey::from_host_bytes(KEY).unwrap()).unwrap();
        fail_marker(
            publish_with(&f, &request, &provider, 43),
            "github_app_worker_grant_denied",
        );
    }
    f.host = host_for(43);
    std::fs::remove_file(
        f.engine
            .store
            .dir
            .join("blobs")
            .join(yoke::model::hash(SOURCE.as_bytes())),
    )
    .unwrap();
    fail_marker(
        publish_with(&f, &request, &provider, 43),
        "github_app_worker_source_unavailable",
    );
    assert!(provider.seen.lock().unwrap().is_empty());
    let count: usize = f
        .engine
        .store
        .db
        .query_row(
            "SELECT count(*) FROM records WHERE kind=?1",
            [publication::RECORD_KIND],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(count, if published { 2 } else { 1 });
}

#[test]
fn exact_published_retry_after_reinstallation_uses_current_repository_grant_without_http() {
    check_historical_retry_after_reinstallation(true);
}

#[test]
fn exact_unknown_retry_after_reinstallation_uses_current_repository_grant_without_http() {
    check_historical_retry_after_reinstallation(false);
}

#[test]
fn app_reinstallation_updates_the_same_owned_comment_without_another_post() {
    let (mut f, _, preview, _) = published_fixture();
    // The installation changes; App/repository/PR ownership of the comment persists.
    f.host = host_for(43);
    let mut event: Value = serde_json::from_slice(&raw_event()).unwrap();
    event["installation"]["id"] = json!(43);
    let raw = serde_json::to_vec(&event).unwrap();
    let admitted = app::ingest(
        &f.engine.store,
        &f.host,
        &delivery_headers(&raw, 2),
        &raw,
        &f.options.queue,
        &f.d,
    )
    .unwrap();
    let job = admitted["jobId"].as_str().unwrap();
    app::select(&f.engine.store, &f.host, job, 1, &f.options.queue, &f.d).unwrap();
    let claim = worker::ClaimRequest {
        schema_version: 1,
        job_id: job.into(),
        expected_queue_revision: 2,
        worker: "owned-native-worker".into(),
        request_id: "89abcdef-0123-4abc-8000-000000000003".into(),
        lease_seconds: 120,
    };
    f.claim = worker::claim(&f.engine.store, &f.host, &claim, &f.options, &f.clock, &f.d).unwrap();
    let provider = Provider::configured(Mode::Exact, None, 43, None, None);
    let mut client = client_for_installation(
        provider.endpoint,
        f.time.clone(),
        &f.d,
        json!({"contents":"read","pull_requests":"read"}),
        43,
    );
    let mut run = f.request();
    run.request_id = "89abcdef-0123-4abc-8000-000000000004".into();
    let result = worker::run(&f.context(), &mut client, &run, &remote(), &f.d).unwrap();
    let next = publication::preview(
        &f.engine.store,
        &f.host,
        &preview_request(&result),
        &options(&f),
        &f.d,
    )
    .unwrap();
    let mut request = publish_request(&result, &next);
    request.request_id = "89abcdef-0123-4abc-8000-000000000008".into();
    let provider = Provider::configured(
        Mode::Publishing,
        None,
        43,
        Some(preview["body"].as_str().unwrap().into()),
        None,
    );
    let value = publish_with(&f, &request, &provider, 43).unwrap();
    assert_eq!(value["commentId"], 501);
    let seen = provider.seen.lock().unwrap();
    assert_eq!(
        seen.iter()
            .filter(|r| r.method == "POST" && r.path == "/repos/acme/demo/issues/7/comments")
            .count(),
        0,
        "reinstallation must preserve the owned comment instead of creating a duplicate"
    );
    assert_eq!(
        seen.iter()
            .filter(|r| r.method == "GET" && r.path == "/repos/acme/demo/issues/comments/501")
            .count(),
        1
    );
    assert_eq!(
        seen.iter()
            .filter(|r| r.method == "PATCH" && r.path == "/repos/acme/demo/issues/comments/501")
            .count(),
        1
    );
    assert_eq!(value["networkRequests"], 10);
}
#[test]
fn publication_create_target_rejects_raw_extra_fields_even_with_original_id_and_mac() {
    let (f, _, _, request) = published_fixture();
    let (id,raw):(String,String)=f.engine.store.db.query_row("SELECT id,value FROM records WHERE kind=?1 AND json_extract(value,'$.event.type')='intent'",[publication::RECORD_KIND],|r|Ok((r.get(0)?,r.get(1)?))).unwrap();
    let mut value: Value = serde_json::from_str(&raw).unwrap();
    assert_eq!(value["event"]["target"], json!({"type":"create"}));
    value["event"]["target"]["commentId"] = json!(999);
    f.engine
        .store
        .db
        .execute(
            "UPDATE records SET value=?1 WHERE kind=?2 AND id=?3",
            rusqlite::params![value.to_string(), publication::RECORD_KIND, id],
        )
        .unwrap();
    let provider = Provider::new(Mode::Publishing);
    fail_marker(
        publish_with(&f, &request, &provider, 42),
        "github_app_publication_binding_invalid",
    );
    assert!(provider.seen.lock().unwrap().is_empty());
}
#[test]
fn official_numeric_repository_comment_url_is_accepted_after_create() {
    let f = Fixture::new();
    let result = completed(&f);
    let preview = publication::preview(
        &f.engine.store,
        &f.host,
        &preview_request(&result),
        &options(&f),
        &f.d,
    )
    .unwrap();
    let request = publish_request(&result, &preview);
    let provider = Provider::configured(Mode::Publishing, None, 42, None, Some(9));
    let value = publish_with(&f, &request, &provider, 42).unwrap();
    assert_eq!(value["status"], "published");
    assert_eq!(value["commentId"], 501);
    assert_eq!(provider.seen.lock().unwrap().len(), 9);
}
#[test]
fn official_numeric_repository_comment_url_is_accepted_for_owned_get_and_patch() {
    let (mut f, _, preview, _) = published_fixture();
    let result = complete_next_head(&mut f);
    let p = publication::PreviewRequest {
        page: None,
        schema_version: 1,
        completion_id: result["completionId"].as_str().unwrap().into(),
        expected_queue_revision: 4,
    };
    let next = publication::preview(&f.engine.store, &f.host, &p, &options(&f), &f.d).unwrap();
    let mut request = publish_request(&result, &next);
    request.expected_queue_revision = 4;
    request.request_id = "89abcdef-0123-4abc-8000-000000000008".into();
    let provider = Provider::configured(
        Mode::PublishingNext,
        None,
        42,
        Some(preview["body"].as_str().unwrap().into()),
        Some(9),
    );
    let value = publish_with(&f, &request, &provider, 42).unwrap();
    assert_eq!(value["status"], "published");
    assert_eq!(value["commentId"], 501);
    let seen = provider.seen.lock().unwrap();
    assert_eq!(seen.len(), 10);
    assert_eq!(seen.iter().filter(|r| r.method == "PATCH").count(), 1);
    assert!(
        !seen.iter().any(|r| r.path.starts_with("/repositories/")),
        "informational URLs never choose transport routes"
    );
}
#[test]
fn numeric_comment_url_for_another_repository_is_refused_without_a_patch() {
    let (f, result, preview, _) = published_fixture();
    let request = reselected_request(&f, &result);
    let provider = Provider::configured(
        Mode::Publishing,
        None,
        42,
        Some(preview["body"].as_str().unwrap().into()),
        Some(10),
    );
    fail_marker(
        publish_with(&f, &request, &provider, 42),
        "github_app_worker_provider_invalid",
    );
    assert!(
        !provider
            .seen
            .lock()
            .unwrap()
            .iter()
            .any(|r| r.method == "PATCH")
    );
    let count: usize = f
        .engine
        .store
        .db
        .query_row(
            "SELECT count(*) FROM records WHERE kind=?1",
            [publication::RECORD_KIND],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(
        count, 2,
        "invalid ownership evidence must be rejected before another intent"
    );
}
fn reseal_publication(value: &mut Value) -> String {
    value.as_object_mut().unwrap().remove("signature256");
    let mut mac = Hmac::<sha2::Sha256>::new_from_slice(KEY).unwrap();
    mac.update(b"yoke-code-intelligence/github-app-publication/v1\n");
    mac.update(&serde_json::to_vec(value).unwrap());
    value["signature256"] = json!(format!(
        "sha256={}",
        hex::encode(mac.finalize().into_bytes())
    ));
    yoke::model::identity(value)
}
#[test]
fn authenticated_impossible_intent_at_unselected_checkpoint_is_rejected_by_queue_factory() {
    let (f, _, mut preview, mut request) = published_fixture();
    let checkpoint: String = f
        .engine
        .store
        .db
        .query_row(
            "SELECT id FROM records WHERE kind=?1 AND json_extract(value,'$.sequence')=1",
            [app::RECORD_KIND],
            |r| r.get(0),
        )
        .unwrap();
    let mut rows: Vec<(String, Value)> = f
        .engine
        .store
        .db
        .prepare(
            "SELECT id,value FROM records WHERE kind=?1 ORDER BY json_extract(value,'$.sequence')",
        )
        .unwrap()
        .query_map([publication::RECORD_KIND], |r| {
            Ok((
                r.get(0)?,
                serde_json::from_str::<Value>(&r.get::<_, String>(1)?).unwrap(),
            ))
        })
        .unwrap()
        .map(Result::unwrap)
        .collect();
    for key in [
        "proposalId",
        "published",
        "publicationAuthority",
        "currentHeadVerified",
        "installationTokenVerified",
        "sourceWritten",
        "complete",
        "networkRequests",
        "modelCalls",
    ] {
        preview.as_object_mut().unwrap().remove(key);
    }
    preview["queueRevision"] = json!(1);
    let proposal = yoke::model::identity(&preview);
    request.expected_queue_revision = 1;
    request.proposal_id = proposal.clone();
    rows[0].1["event"]["data"]["queueCheckpoint"] = json!(checkpoint);
    rows[0].1["event"]["data"]["queueRevision"] = json!(1);
    rows[0].1["event"]["data"]["proposalId"] = json!(proposal);
    rows[0].1["event"]["request"] = serde_json::to_value(&request).unwrap();
    let intent = reseal_publication(&mut rows[0].1);
    rows[1].1["event"]["intentId"] = json!(intent);
    for (old, mut value) in rows {
        let id = reseal_publication(&mut value);
        f.engine
            .store
            .db
            .execute(
                "UPDATE records SET id=?1,value=?2 WHERE kind=?3 AND id=?4",
                rusqlite::params![id, value.to_string(), publication::RECORD_KIND, old],
            )
            .unwrap();
    }
    let provider = Provider::new(Mode::Publishing);
    fail_marker(
        publish_with(&f, &request, &provider, 42),
        "github_app_binding_invalid",
    );
    assert!(provider.seen.lock().unwrap().is_empty());
}

#[test]
fn publication_refuses_disabled_sqlite_sync_before_authorization_or_intent() {
    let f = Fixture::new();
    let result = completed(&f);
    let preview = publication::preview(
        &f.engine.store,
        &f.host,
        &preview_request(&result),
        &options(&f),
        &f.d,
    )
    .unwrap();
    f.engine
        .store
        .db
        .execute_batch("PRAGMA synchronous=OFF")
        .unwrap();
    let provider = Provider::new(Mode::Publishing);
    fail_marker(
        publish_with(&f, &publish_request(&result, &preview), &provider, 42),
        "github_app_publication_sql_durability_required",
    );
    assert!(provider.seen.lock().unwrap().is_empty());
    let count: usize = f
        .engine
        .store
        .db
        .query_row(
            "SELECT count(*) FROM records WHERE kind=?1",
            [publication::RECORD_KIND],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(count, 0);
    let synchronous: u64 = f
        .engine
        .store
        .db
        .query_row("PRAGMA synchronous", [], |r| r.get(0))
        .unwrap();
    assert_eq!(
        synchronous, 0,
        "refusal must not change caller-owned SQL policy"
    );
}

#[test]
fn publication_refuses_disabled_sqlite_journal_before_authorization_or_intent() {
    let f = Fixture::new();
    let result = completed(&f);
    let preview = publication::preview(
        &f.engine.store,
        &f.host,
        &preview_request(&result),
        &options(&f),
        &f.d,
    )
    .unwrap();
    f.engine
        .store
        .db
        .execute_batch("PRAGMA journal_mode=OFF")
        .unwrap();
    let provider = Provider::new(Mode::Publishing);
    fail_marker(
        publish_with(&f, &publish_request(&result, &preview), &provider, 42),
        "github_app_publication_sql_durability_required",
    );
    assert!(provider.seen.lock().unwrap().is_empty());
    let count: usize = f
        .engine
        .store
        .db
        .query_row(
            "SELECT count(*) FROM records WHERE kind=?1",
            [publication::RECORD_KIND],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(count, 0);
    let journal: String = f
        .engine
        .store
        .db
        .query_row("PRAGMA journal_mode", [], |r| r.get(0))
        .unwrap();
    assert_eq!(
        journal, "off",
        "refusal must not change caller-owned SQL policy"
    );
}

#[test]
fn publication_journal_owner_prevents_windows_lock_file_rename_during_send() {
    let f = Fixture::new();
    let result = completed(&f);
    let preview = publication::preview(
        &f.engine.store,
        &f.host,
        &preview_request(&result),
        &options(&f),
        &f.d,
    )
    .unwrap();
    let path = f.engine.store.dir.join("github-publication.lock");
    let replacement = path.with_extension("owned-fixture-moved");
    let renamed = Arc::new(AtomicBool::new(false));
    let observed = renamed.clone();
    let attempted = Arc::new(AtomicBool::new(false));
    let checked = attempted.clone();
    let provider = Provider::with_hook(
        Mode::Publishing,
        Some(Box::new(move || {
            assert!(path.is_file());
            assert!(!replacement.exists());
            checked.store(true, Ordering::SeqCst);
            if std::fs::rename(&path, &replacement).is_ok() {
                observed.store(true, Ordering::SeqCst);
                std::fs::rename(&replacement, &path).unwrap();
            }
        })),
    );
    assert_eq!(
        publish_with(&f, &publish_request(&result, &preview), &provider, 42).unwrap()["status"],
        "published"
    );
    assert!(attempted.load(Ordering::SeqCst));
    assert!(
        !renamed.load(Ordering::SeqCst),
        "an owned Windows sender lock must prevent rename/replacement until the sender and terminal persistence finish"
    );
}

fn complete_another_pr(f: &mut Fixture, number: u64) -> Value {
    let mut event: Value = serde_json::from_slice(&raw_event()).unwrap();
    event["number"] = json!(number);
    event["pull_request"]["number"] = json!(number);
    let raw = serde_json::to_vec(&event).unwrap();
    let admitted = app::ingest(
        &f.engine.store,
        &f.host,
        &delivery_headers(&raw, 2),
        &raw,
        &f.options.queue,
        &f.d,
    )
    .unwrap();
    let job = admitted["jobId"].as_str().unwrap();
    app::select(&f.engine.store, &f.host, job, 1, &f.options.queue, &f.d).unwrap();
    let request = worker::ClaimRequest {
        schema_version: 1,
        job_id: job.into(),
        expected_queue_revision: 2,
        worker: "owned-native-worker".into(),
        request_id: "89abcdef-0123-4abc-8000-000000000003".into(),
        lease_seconds: 120,
    };
    f.claim = worker::claim(
        &f.engine.store,
        &f.host,
        &request,
        &f.options,
        &f.clock,
        &f.d,
    )
    .unwrap();
    let provider = Provider::with_number(Mode::Exact, None, 42, None, None, number);
    let mut client = f.client(&provider);
    let mut request = f.request();
    request.request_id = "89abcdef-0123-4abc-8000-000000000004".into();
    worker::run(&f.context(), &mut client, &request, &remote(), &f.d).unwrap()
}

#[test]
fn lost_post_response_leaves_one_unknown_intent_and_never_blindly_resends() {
    let f = Fixture::new();
    let result = completed(&f);
    let preview = publication::preview(
        &f.engine.store,
        &f.host,
        &preview_request(&result),
        &options(&f),
        &f.d,
    )
    .unwrap();
    let request = publish_request(&result, &preview);
    let provider = Provider::new(Mode::LostReply);
    fail_marker(
        publish_with(&f, &request, &provider, 42),
        "github_app_transport_failed",
    );
    let first = publish_with(&f, &request, &provider, 42).unwrap();
    assert_eq!(first["status"], "unknown");
    assert_eq!(first["historical"], true);
    assert_eq!(first["networkRequests"], 0);
    assert_eq!(first["commentId"], Value::Null);
    assert_eq!(first["published"], false);
    assert_eq!(first["publicationAuthority"], false);
    assert_eq!(
        provider
            .seen
            .lock()
            .unwrap()
            .iter()
            .filter(|r| r.method == "POST" && r.path.ends_with("/issues/7/comments"))
            .count(),
        1
    );
    let mut different = request;
    different.request_id = "89abcdef-0123-4abc-8000-000000000008".into();
    fail_marker(
        publish_with(&f, &different, &provider, 42),
        "github_app_publication_outcome_unknown",
    );
    assert_eq!(provider.seen.lock().unwrap().len(), 9);
    let count: usize = f
        .engine
        .store
        .db
        .query_row(
            "SELECT count(*) FROM records WHERE kind=?1",
            [publication::RECORD_KIND],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(count, 1);
}

// Removing response identity validation or clearing a failed send's durable
// intent would turn one of these into a false success or a second POST.
#[test]
fn invalid_post_responses_preserve_one_unknown_intent_without_resending() {
    for (fault, marker) in [
        (CommentFault::WrongApp, "github_app_worker_provider_invalid"),
        (
            CommentFault::MissingApp,
            "github_app_worker_provider_invalid",
        ),
        (
            CommentFault::WrongActor,
            "github_app_worker_provider_invalid",
        ),
        (
            CommentFault::WrongBody,
            "github_app_worker_provider_invalid",
        ),
        (CommentFault::ZeroId, "github_app_worker_provider_invalid"),
        (CommentFault::WrongUrl, "github_app_worker_provider_invalid"),
        (
            CommentFault::TokenReflection,
            "github_app_installation_binding_invalid",
        ),
        (CommentFault::WrongStatus, "github_app_http_refused"),
    ] {
        let f = Fixture::new();
        let result = completed(&f);
        let preview = publication::preview(
            &f.engine.store,
            &f.host,
            &preview_request(&result),
            &options(&f),
            &f.d,
        )
        .unwrap();
        let request = publish_request(&result, &preview);
        let provider = Provider::new(Mode::PublishingFault(fault));
        fail_marker(publish_with(&f, &request, &provider, 42), marker);
        let repeat = publish_with(&f, &request, &provider, 42).unwrap();
        assert_eq!(repeat["status"], "unknown");
        assert_eq!(repeat["historical"], true);
        assert_eq!(repeat["published"], false);
        assert_eq!(repeat["networkRequests"], 0);
        let seen = provider.seen.lock().unwrap();
        assert_eq!(seen.len(), 9);
        assert_eq!(
            seen.iter()
                .filter(|r| r.method == "POST" && r.path == "/repos/acme/demo/issues/7/comments")
                .count(),
            1
        );
        assert!(!seen.iter().any(|r| r.path.contains("untrusted")));
        let count: usize = f
            .engine
            .store
            .db
            .query_row(
                "SELECT count(*) FROM records WHERE kind=?1",
                [publication::RECORD_KIND],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);
    }
}

#[test]
fn source_lost_during_provider_preflight_refuses_before_intent_or_post() {
    let f = Fixture::new();
    let result = completed(&f);
    let preview = publication::preview(
        &f.engine.store,
        &f.host,
        &preview_request(&result),
        &options(&f),
        &f.d,
    )
    .unwrap();
    let blob = f
        .engine
        .store
        .dir
        .join("blobs")
        .join(yoke::model::hash(SOURCE.as_bytes()));
    let provider = Provider::with_hook(
        Mode::PreflightMutation,
        Some(Box::new(move || std::fs::remove_file(blob).unwrap())),
    );
    fail_marker(
        publish_with(&f, &publish_request(&result, &preview), &provider, 42),
        "github_app_worker_source_unavailable",
    );
    let seen = provider.seen.lock().unwrap();
    assert_eq!(seen.len(), 8);
    assert!(
        !seen
            .iter()
            .any(|r| r.method == "PATCH" || r.path == "/repos/acme/demo/issues/7/comments")
    );
    let count: usize = f
        .engine
        .store
        .db
        .query_row(
            "SELECT count(*) FROM records WHERE kind=?1",
            [publication::RECORD_KIND],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(count, 0);
}

#[test]
fn publication_reserves_terminal_capacity_for_prior_unknown_prs() {
    let mut f = Fixture::new();
    let result = completed(&f);
    let preview = publication::preview(
        &f.engine.store,
        &f.host,
        &preview_request(&result),
        &options(&f),
        &f.d,
    )
    .unwrap();
    let request = publish_request(&result, &preview);
    let mut o = options(&f);
    o.allow_publication = true;
    o.max_journal_records = 3;
    let provider = Provider::new(Mode::LostReply);
    let mut client = fixture_client(
        provider.endpoint,
        f.time.clone(),
        &f.d,
        json!({"pull_requests":"write"}),
    );
    fail_marker(
        publication::publish(
            &publication_context(&f, &o),
            &mut client,
            &request,
            &remote_publish(),
            &f.d,
        ),
        "github_app_transport_failed",
    );
    let result = complete_another_pr(&mut f, 8);
    let preview = publication::preview(
        &f.engine.store,
        &f.host,
        &preview_request(&result),
        &options(&f),
        &f.d,
    )
    .unwrap();
    let mut request = publish_request(&result, &preview);
    request.request_id = "89abcdef-0123-4abc-8000-000000000008".into();
    let provider = Provider::with_number(Mode::Publishing, None, 42, None, None, 8);
    let mut client = fixture_client(
        provider.endpoint,
        f.time.clone(),
        &f.d,
        json!({"pull_requests":"write"}),
    );
    fail_marker(
        publication::publish(
            &publication_context(&f, &o),
            &mut client,
            &request,
            &remote_publish(),
            &f.d,
        ),
        "github_app_publication_capacity_exceeded",
    );
    assert!(
        provider.seen.lock().unwrap().is_empty(),
        "cumulative capacity refusal must precede authentication"
    );
    let count: usize = f
        .engine
        .store
        .db
        .query_row(
            "SELECT count(*) FROM records WHERE kind=?1",
            [publication::RECORD_KIND],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(count, 1, "the old unknown intent retains its terminal slot");
}

#[test]
fn publication_journal_zero_limits_are_refused() {
    let f = Fixture::new();
    for field in ["records", "bytes"] {
        let mut o = options(&f);
        if field == "records" {
            o.max_journal_records = 0;
        } else {
            o.max_journal_bytes = 0;
        }
        fail_marker(o.admit(&f.d), "github_app_publication_capacity_exceeded");
    }
}

fn unknown_fixture() -> (Fixture, Value, Value, publication::PublishRequest, Value) {
    let f = Fixture::new();
    let result = completed(&f);
    let preview = publication::preview(
        &f.engine.store,
        &f.host,
        &preview_request(&result),
        &options(&f),
        &f.d,
    )
    .unwrap();
    let request = publish_request(&result, &preview);
    let provider = Provider::new(Mode::LostReply);
    fail_marker(
        publish_with(&f, &request, &provider, 42),
        "github_app_transport_failed",
    );
    let unknown = publish_with(&f, &request, &provider, 42).unwrap();
    assert_eq!(unknown["status"], "unknown");
    (f, result, preview, request, unknown)
}
fn complete_next_head(f: &mut Fixture) -> Value {
    let mut event: Value = serde_json::from_slice(&raw_event()).unwrap();
    event["action"] = json!("synchronize");
    event["pull_request"]["head"]["sha"] = json!("c".repeat(40));
    let raw = serde_json::to_vec(&event).unwrap();
    let job = app::ingest(
        &f.engine.store,
        &f.host,
        &delivery_headers(&raw, 2),
        &raw,
        &f.options.queue,
        &f.d,
    )
    .unwrap();
    let job = job["jobId"].as_str().unwrap();
    app::select(&f.engine.store, &f.host, job, 3, &f.options.queue, &f.d).unwrap();
    let request = worker::ClaimRequest {
        schema_version: 1,
        job_id: job.into(),
        expected_queue_revision: 4,
        worker: "owned-native-worker".into(),
        request_id: "89abcdef-0123-4abc-8000-000000000003".into(),
        lease_seconds: 120,
    };
    f.claim = worker::claim(
        &f.engine.store,
        &f.host,
        &request,
        &f.options,
        &f.clock,
        &f.d,
    )
    .unwrap();
    let provider = Provider::new(Mode::NextHead);
    let mut request = f.request();
    request.request_id = "89abcdef-0123-4abc-8000-000000000004".into();
    worker::run(
        &f.context(),
        &mut f.client(&provider),
        &request,
        &remote(),
        &f.d,
    )
    .unwrap()
}
fn recover_with(
    f: &Fixture,
    request: &publication::RecoverRequest,
    provider: &Provider,
) -> anyhow::Result<Value> {
    let mut o = options(f);
    o.allow_publication = true;
    let mut client = client_for_installation(
        provider.endpoint,
        f.time.clone(),
        &f.d,
        json!({"pull_requests":"write"}),
        request.installation_id,
    );
    publication::recover(
        &publication_context(f, &o),
        &mut client,
        request,
        &remote_publish(),
        &f.d,
    )
}
fn recovery_request(unknown: &Value) -> publication::RecoverRequest {
    publication::RecoverRequest {
        schema_version: 1,
        intent_id: unknown["intentId"].as_str().unwrap().into(),
        comment_id: 501,
        installation_id: 42,
    }
}
#[test]
fn publication_status_observes_unknown_without_write_or_network_authority() {
    let (f, _, _, request, unknown) = unknown_fixture();
    let mut o = options(&f);
    o.worker.queue.allow_writes = false;
    let request = publication::StatusRequest {
        schema_version: 1,
        request_id: request.request_id,
    };
    let value = publication::status(&publication_context(&f, &o), &request, &f.d).unwrap();
    assert_eq!(value, unknown);
}
#[test]
fn recovery_confirms_old_unknown_after_new_selection_then_new_head_patches_same_comment() {
    let (mut f, _, old_preview, _, unknown) = unknown_fixture();
    let next = complete_next_head(&mut f);
    let provider = Provider::configured(
        Mode::Publishing,
        None,
        42,
        Some(old_preview["body"].as_str().unwrap().into()),
        Some(9),
    );
    let request = recovery_request(&unknown);
    let value = recover_with(&f, &request, &provider).unwrap();
    assert_eq!(value["status"], "published");
    assert_eq!(value["reconciled"], true);
    assert_eq!(value["commentId"], 501);
    assert_eq!(value["currentHeadVerified"], false);
    assert_eq!(value["publicationAuthority"], false);
    assert_eq!(value["networkRequests"], 6);
    let retry = recover_with(&f, &request, &provider).unwrap();
    assert_eq!(retry["historical"], true);
    assert_eq!(retry["networkRequests"], 0);
    assert!(
        provider
            .seen
            .lock()
            .unwrap()
            .iter()
            .all(|r| r.method == "GET" || r.path.ends_with("/access_tokens"))
    );
    assert_eq!(provider.seen.lock().unwrap().len(), 6);
    let p = publication::PreviewRequest {
        page: None,
        schema_version: 1,
        completion_id: next["completionId"].as_str().unwrap().into(),
        expected_queue_revision: 4,
    };
    let preview = publication::preview(&f.engine.store, &f.host, &p, &options(&f), &f.d).unwrap();
    let mut publish = publish_request(&next, &preview);
    publish.expected_queue_revision = 4;
    publish.request_id = "89abcdef-0123-4abc-8000-000000000008".into();
    let provider = Provider::configured(
        Mode::PublishingNext,
        None,
        42,
        Some(old_preview["body"].as_str().unwrap().into()),
        Some(9),
    );
    assert_eq!(
        publish_with(&f, &publish, &provider, 42).unwrap()["commentId"],
        501
    );
    let seen = provider.seen.lock().unwrap();
    assert_eq!(seen.iter().filter(|r| r.method == "PATCH").count(), 1);
    assert!(
        !seen
            .iter()
            .any(|r| r.path == "/repos/acme/demo/issues/7/comments")
    );
}

#[test]
fn lost_changed_body_patch_is_never_resent_and_recovery_confirms_the_same_owned_id() {
    let (mut f, _, old_preview, _) = published_fixture();
    let next = complete_next_head(&mut f);
    let preview = publication::preview(
        &f.engine.store,
        &f.host,
        &publication::PreviewRequest {
            page: None,
            schema_version: 1,
            completion_id: next["completionId"].as_str().unwrap().into(),
            expected_queue_revision: 4,
        },
        &options(&f),
        &f.d,
    )
    .unwrap();
    assert_ne!(preview["bodyHash"], old_preview["bodyHash"]);
    let mut request = publish_request(&next, &preview);
    request.expected_queue_revision = 4;
    request.request_id = "89abcdef-0123-4abc-8000-000000000008".into();
    let provider = Provider::configured(
        Mode::LostPatchReply,
        None,
        42,
        Some(old_preview["body"].as_str().unwrap().into()),
        Some(9),
    );
    fail_marker(
        publish_with(&f, &request, &provider, 42),
        "github_app_transport_failed",
    );
    let unknown = publish_with(&f, &request, &provider, 42).unwrap();
    assert_eq!(unknown["status"], "unknown");
    assert_eq!(unknown["historical"], true);
    assert_eq!(unknown["networkRequests"], 0);
    assert_eq!(provider.seen.lock().unwrap().len(), 10);
    let mut recovery = recovery_request(&unknown);
    recovery.comment_id = 502;
    fail_marker(
        recover_with(&f, &recovery, &provider),
        "github_app_publication_request_conflict",
    );
    assert_eq!(provider.seen.lock().unwrap().len(), 10);
    recovery.comment_id = 501;
    let reconciled = recover_with(&f, &recovery, &provider).unwrap();
    assert_eq!(reconciled["status"], "published");
    assert_eq!(reconciled["reconciled"], true);
    assert_eq!(reconciled["commentId"], 501);
    assert_eq!(reconciled["bodyHash"], preview["bodyHash"]);
    assert_eq!(reconciled["networkRequests"], 6);
    assert_eq!(reconciled["writeConfirmed"], false);
    assert_eq!(reconciled["currentHeadVerified"], false);
    let repeat = publish_with(&f, &request, &provider, 42).unwrap();
    assert_eq!(repeat["publicationId"], reconciled["publicationId"]);
    assert_eq!(repeat["historical"], true);
    assert_eq!(repeat["networkRequests"], 0);
    let seen = provider.seen.lock().unwrap();
    assert_eq!(seen.len(), 16);
    assert_eq!(seen.iter().filter(|r| r.method == "PATCH").count(), 1);
    assert!(
        !seen
            .iter()
            .any(|r| r.path == "/repos/acme/demo/issues/7/comments")
    );
    let count: usize = f
        .engine
        .store
        .db
        .query_row(
            "SELECT count(*) FROM records WHERE kind=?1",
            [publication::RECORD_KIND],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(count, 4);
}
#[test]
fn recovery_uses_current_repository_grant_after_app_reinstallation() {
    let (mut f, _, preview, _, unknown) = unknown_fixture();
    f.host = host_for(43);
    let mut request = recovery_request(&unknown);
    request.installation_id = 43;
    let provider = Provider::configured(
        Mode::Publishing,
        None,
        43,
        Some(preview["body"].as_str().unwrap().into()),
        Some(9),
    );
    let value = recover_with(&f, &request, &provider).unwrap();
    assert_eq!(value["status"], "published");
    assert_eq!(value["reconciled"], true);
    let seen = provider.seen.lock().unwrap();
    assert_eq!(seen.len(), 6);
    assert!(seen.iter().any(|r| r.path == "/app/installations/43"));
    assert!(!seen.iter().any(|r| r.path.contains("/installations/42")));
    assert!(
        !seen
            .iter()
            .any(|r| r.method == "PATCH" || r.path == "/repos/acme/demo/issues/7/comments")
    );
}
#[test]
fn recovery_refuses_another_apps_comment_and_preserves_unknown_intent() {
    let (f, _, preview, original, unknown) = unknown_fixture();
    let provider = Provider::configured(
        Mode::WrongCommentApp,
        None,
        42,
        Some(preview["body"].as_str().unwrap().into()),
        None,
    );
    fail_marker(
        recover_with(&f, &recovery_request(&unknown), &provider),
        "github_app_worker_provider_invalid",
    );
    assert_eq!(
        publish_with(&f, &original, &provider, 42).unwrap()["status"],
        "unknown"
    );
    let count: usize = f
        .engine
        .store
        .db
        .query_row(
            "SELECT count(*) FROM records WHERE kind=?1",
            [publication::RECORD_KIND],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(count, 1);
}

#[test]
fn reconciled_recovery_refuses_a_changed_comment_id_before_http() {
    let (f, _, preview, _, unknown) = unknown_fixture();
    let provider = Provider::configured(
        Mode::Publishing,
        None,
        42,
        Some(preview["body"].as_str().unwrap().into()),
        None,
    );
    let mut request = recovery_request(&unknown);
    assert_eq!(
        recover_with(&f, &request, &provider).unwrap()["reconciled"],
        true
    );
    assert_eq!(provider.seen.lock().unwrap().len(), 6);
    request.comment_id = 502;
    fail_marker(
        recover_with(&f, &request, &provider),
        "github_app_publication_request_conflict",
    );
    assert_eq!(provider.seen.lock().unwrap().len(), 6);
    let count: usize = f
        .engine
        .store
        .db
        .query_row(
            "SELECT count(*) FROM records WHERE kind=?1",
            [publication::RECORD_KIND],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(count, 2);
}

#[test]
fn publication_status_and_recovery_preserve_a_refused_caller_transaction() {
    let (f, _, preview, request, unknown) = unknown_fixture();
    let provider = Provider::configured(
        Mode::Publishing,
        None,
        42,
        Some(preview["body"].as_str().unwrap().into()),
        None,
    );
    f.engine.store.db.execute_batch("CREATE TEMP TABLE owned_tx(value INTEGER); BEGIN IMMEDIATE; INSERT INTO owned_tx VALUES(1)").unwrap();
    fail_marker(
        publish_with(&f, &request, &provider, 42),
        "github_app_publication_caller_transaction_refused",
    );
    fail_marker(
        recover_with(&f, &recovery_request(&unknown), &provider),
        "github_app_publication_caller_transaction_refused",
    );
    fail_marker(
        publication::status(
            &publication_context(&f, &options(&f)),
            &publication::StatusRequest {
                schema_version: 1,
                request_id: request.request_id,
            },
            &f.d,
        ),
        "github_app_publication_caller_transaction_refused",
    );
    assert!(!f.engine.store.db.is_autocommit());
    let count: usize = f
        .engine
        .store
        .db
        .query_row("SELECT count(*) FROM owned_tx", [], |r| r.get(0))
        .unwrap();
    assert_eq!(count, 1);
    assert!(provider.seen.lock().unwrap().is_empty());
    f.engine.store.db.execute_batch("ROLLBACK").unwrap();
    let count: usize = f
        .engine
        .store
        .db
        .query_row("SELECT count(*) FROM owned_tx", [], |r| r.get(0))
        .unwrap();
    assert_eq!(count, 0);
}

#[test]
#[ignore = "Run through the outer owned credential controller, outside the guarded native job"]
fn publication_cli_reads_completed_preview_and_historical_status_with_an_owned_os_key() {
    // The outer controller owns creation and verified deletion. It survives a
    // memory guard terminating this complete native process tree.
    let name = std::env::var("YOKE_PUBLICATION_ORACLE_CREDENTIAL")
        .expect("explicit outer fixture credential owner required");
    let nonce = name
        .strip_prefix("test-publication-")
        .expect("owned fixture name required");
    assert!(
        nonce.len() == 32
            && nonce
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    );
    let loaded = yoke::host_credentials::read_github_webhook_secret(&name).unwrap();
    assert!(
        loaded.as_slice() == KEY,
        "owned fixture credential mismatch"
    );
    drop(loaded);
    let (f, result, preview, request) = published_fixture();
    let documents = tempfile::tempdir().unwrap();
    let profile = documents.path().join("profile.json");
    let input = documents.path().join("request.json");
    let executable_hash = || {
        use sha2::Digest;
        let mut input = std::fs::File::open(env!("CARGO_BIN_EXE_yoke")).unwrap();
        let mut digest = sha2::Sha256::new();
        let mut bytes = [0u8; 65536];
        loop {
            let count = input.read(&mut bytes).unwrap();
            if count == 0 {
                break;
            }
            digest.update(&bytes[..count]);
        }
        hex::encode(digest.finalize())
    };
    let binary_hash = executable_hash();
    std::fs::write(
        &profile,
        serde_json::to_vec(&json!({
            "schemaVersion":1,"appId":123,"secretCredential":name,
            "grants":[{"installationId":42,"repositoryId":9,"repository":"acme/demo"}]
        }))
        .unwrap(),
    )
    .unwrap();
    let run = |operation: &str| {
        let mut command = Command::new(env!("CARGO_BIN_EXE_yoke"));
        command.current_dir(documents.path()).args([
            "--root",
            f.root.path().to_str().unwrap(),
            "--timeout",
            "30000",
            "--max-bytes",
            "128000",
            "github-app",
            "--allow-webhooks",
            "--profile",
            profile.to_str().unwrap(),
            "publication",
            operation,
            "--request",
            input.to_str().unwrap(),
        ]);
        let output = yoke::process::run_capture(
            command,
            &[],
            &Deadline::new(40_000),
            yoke::process::CaptureLimits {
                stdin_bytes: 0,
                stdout_bytes: 128_000,
                stderr_bytes: 64_000,
            },
        )
        .unwrap();
        assert!(
            !output.timed_out
                && !output.output_exceeded
                && !output.io_error
                && output.output_complete
        );
        assert!(!output.stdout.windows(KEY.len()).any(|s| s == KEY));
        assert!(!output.stderr.windows(KEY.len()).any(|s| s == KEY));
        output
    };
    let sql_state = || {
        use rusqlite::types::ValueRef;
        let db = &f.engine.store.db;
        let tables = db.prepare("SELECT name FROM sqlite_schema WHERE type='table' AND name NOT LIKE 'sqlite_%' ORDER BY name").unwrap()
            .query_map([], |r| r.get::<_, String>(0)).unwrap().collect::<rusqlite::Result<Vec<_>>>().unwrap();
        let mut state = std::collections::BTreeMap::new();
        for table in tables {
            let mut statement = db
                .prepare(&format!("SELECT * FROM \"{}\"", table.replace('"', "\"\"")))
                .unwrap();
            let columns = statement.column_count();
            let mut rows = statement.query([]).unwrap();
            let mut values = Vec::new();
            while let Some(row) = rows.next().unwrap() {
                let mut cells = Vec::new();
                for column in 0..columns {
                    cells.push(match row.get_ref(column).unwrap() {
                        ValueRef::Null => "null".into(),
                        ValueRef::Integer(n) => format!("integer:{n}"),
                        ValueRef::Real(n) => format!("real:{:016x}", n.to_bits()),
                        ValueRef::Text(raw) => format!("text:{}", hex::encode(raw)),
                        ValueRef::Blob(raw) => format!("blob:{}", hex::encode(raw)),
                    });
                }
                values.push(cells);
            }
            values.sort();
            state.insert(table, values);
        }
        state
    };
    let cas_state = || {
        f.blobs()
            .into_iter()
            .map(|name| {
                let bytes = std::fs::read(f.engine.store.dir.join("blobs").join(&name)).unwrap();
                (name, yoke::model::hash(bytes))
            })
            .collect::<std::collections::BTreeMap<_, _>>()
    };
    let before = sql_state();
    let blobs = cas_state();
    std::fs::write(
        &input,
        serde_json::to_vec(&preview_request(&result)).unwrap(),
    )
    .unwrap();
    let output = run("preview");
    assert!(
        output.exit_code == Some(0),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stderr.is_empty() && output.stdout.ends_with(b"\n"));
    let value: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value, preview);
    std::fs::write(
        &input,
        serde_json::to_vec(&publication::StatusRequest {
            schema_version: 1,
            request_id: request.request_id,
        })
        .unwrap(),
    )
    .unwrap();
    let output = run("status");
    assert!(
        output.exit_code == Some(0),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stderr.is_empty() && output.stdout.ends_with(b"\n"));
    let value: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["status"], "published");
    assert_eq!(value["historical"], true);
    assert_eq!(value["commentId"], 501);
    assert_eq!(value["networkRequests"], 0);
    assert_eq!(value["publicationAuthority"], false);
    assert_eq!(value["currentHeadVerified"], false);
    assert_eq!(value["installationTokenVerified"], false);
    assert_eq!(sql_state(), before);
    assert_eq!(cas_state(), blobs);
    assert_eq!(
        std::fs::read(f.root.path().join(PATH)).unwrap(),
        SOURCE.as_bytes()
    );
    std::fs::remove_file(
        f.engine
            .store
            .dir
            .join("blobs")
            .join(yoke::model::hash(SOURCE.as_bytes())),
    )
    .unwrap();
    let output = run("status");
    assert!(output.exit_code != Some(0) && output.stdout.is_empty());
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("github_app_worker_source_unavailable")
    );
    assert_eq!(sql_state(), before);
    assert_eq!(executable_hash(), binary_hash);
    println!(
        "{}",
        json!({"oracle":"owned_publication_cli_v1","binarySha256":binary_hash,
        "verifiedFixtureKeyReads":1,"cliCalls":3,
        "logicalTablesChecked":before.len(),"casFilesHashed":blobs.len(),
        "fixtureOnly":true,"liveGitHubValidated":false,"externallyPublished":false})
    );
}
#[test]
fn publication_status_and_recovery_inputs_are_strict_and_duplicate_safe() {
    let status = json!({"schemaVersion":1,"requestId":"89abcdef-0123-4abc-8000-000000000007"});
    let recover =
        json!({"schemaVersion":1,"intentId":"a".repeat(64),"commentId":501,"installationId":42});
    assert!(publication::StatusRequest::parse(&serde_json::to_vec(&status).unwrap()).is_ok());
    assert!(publication::RecoverRequest::parse(&serde_json::to_vec(&recover).unwrap()).is_ok());
    for (field, value) in [
        ("intentId", json!("a".repeat(63))),
        ("commentId", json!(0)),
        ("installationId", json!(0)),
        ("endpoint", json!("https://untrusted.invalid")),
    ] {
        let mut bad = recover.clone();
        bad[field] = value;
        assert!(publication::RecoverRequest::parse(&serde_json::to_vec(&bad).unwrap()).is_err());
    }
    for raw in [
        serde_json::to_string(&status)
            .unwrap()
            .replacen('{', "{\"schemaVersion\":1,", 1),
        "{\"schemaVersion\":1,\"requestId\":\"BAD\"}".into(),
    ] {
        assert!(publication::StatusRequest::parse(raw.as_bytes()).is_err());
    }
    let raw = serde_json::to_string(&recover)
        .unwrap()
        .replacen('{', "{\"commentId\":501,", 1);
    assert!(publication::RecoverRequest::parse(raw.as_bytes()).is_err());
}

#[test]
fn identical_body_is_observed_without_a_redundant_remote_patch() {
    let (f, result, preview, _) = published_fixture();
    let request = reselected_request(&f, &result);
    let provider = Provider::configured(
        Mode::Publishing,
        None,
        42,
        Some(preview["body"].as_str().unwrap().into()),
        Some(9),
    );
    let value = publish_with(&f, &request, &provider, 42).unwrap();
    let seen = provider.seen.lock().unwrap();
    assert!(
        !seen
            .iter()
            .any(|r| r.method == "PATCH" || r.path == "/repos/acme/demo/issues/7/comments"),
        "identical bodies must not create an ambiguous delayed remote write"
    );
    assert_eq!(value["status"], "published");
    assert_eq!(value["unchanged"], true);
    assert_eq!(value["writeConfirmed"], false);
    assert_eq!(value["networkRequests"], 9);
    assert_eq!(seen.len(), 9);
    drop(seen);
    let retry = publish_with(&f, &request, &provider, 42).unwrap();
    assert_eq!(retry["historical"], true);
    assert_eq!(retry["unchanged"], true);
    assert_eq!(provider.seen.lock().unwrap().len(), 9);
}

#[test]
fn unchanged_observation_uses_only_its_five_scoped_get_requests() {
    let (f, result, preview, _) = published_fixture();
    let request = reselected_request(&f, &result);
    let provider = Provider::configured(
        Mode::Publishing,
        None,
        42,
        Some(preview["body"].as_str().unwrap().into()),
        None,
    );
    let mut client = fixture_client(
        provider.endpoint,
        f.time.clone(),
        &f.d,
        json!({"pull_requests":"write"}),
    );
    let mut o = options(&f);
    o.allow_publication = true;
    let mut remote = remote_publish();
    remote.max_requests = 5;
    let value = publication::publish(
        &publication_context(&f, &o),
        &mut client,
        &request,
        &remote,
        &f.d,
    )
    .unwrap();
    assert_eq!(value["unchanged"], true);
    assert_eq!(provider.seen.lock().unwrap().len(), 9);
}

#[test]
fn publication_waits_for_owned_lock_contention_until_the_inherited_deadline() {
    let f = Fixture::new();
    let result = completed(&f);
    let preview = publication::preview(
        &f.engine.store,
        &f.host,
        &preview_request(&result),
        &options(&f),
        &f.d,
    )
    .unwrap();
    let file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(f.engine.store.dir.join("github-publication.lock"))
        .unwrap();
    fs2::FileExt::try_lock_exclusive(&file).unwrap();
    let provider = Provider::new(Mode::Publishing);
    let mut client = fixture_client(
        provider.endpoint,
        f.time.clone(),
        &f.d,
        json!({"pull_requests":"write"}),
    );
    let mut o = options(&f);
    o.allow_publication = true;
    let d = Deadline::new(500);
    let start = std::time::Instant::now();
    fail_marker(
        publication::publish(
            &publication_context(&f, &o),
            &mut client,
            &publish_request(&result, &preview),
            &remote_publish(),
            &d,
        ),
        "Request deadline exceeded",
    );
    assert!(start.elapsed() >= Duration::from_millis(400));
    assert!(provider.seen.lock().unwrap().is_empty());
    fs2::FileExt::unlock(&file).unwrap();
}

#[test]
fn recovery_cannot_clear_an_intent_while_its_live_publisher_still_owns_the_send() {
    use std::sync::mpsc;
    struct Resume(Option<mpsc::Sender<()>>);
    impl Drop for Resume {
        fn drop(&mut self) {
            if let Some(sender) = self.0.take() {
                let _ = sender.send(());
            }
        }
    }
    let f = Fixture::new();
    let completed = completed(&f);
    let preview = publication::preview(
        &f.engine.store,
        &f.host,
        &preview_request(&completed),
        &options(&f),
        &f.d,
    )
    .unwrap();
    let request = publish_request(&completed, &preview);
    let (entered, waiting) = mpsc::channel();
    let (resume, paused) = mpsc::channel();
    let provider = Provider::with_hook(
        Mode::Publishing,
        Some(Box::new(move || {
            entered.send(()).unwrap();
            paused
                .recv_timeout(Duration::from_secs(8))
                .expect("owned publisher must be released");
        })),
    );
    let root = f.root.path().to_owned();
    let time = f.time.clone();
    let endpoint = provider.endpoint;
    let mut o = options(&f);
    o.allow_publication = true;
    let id = request.request_id.clone();
    thread::scope(|scope| {
        let release = Resume(Some(resume));
        let publishing = scope.spawn(move || {
            let engine = Engine::open(&root).unwrap();
            let host = host();
            let clock = worker::Clock::for_fixture(time.clone());
            let d = Deadline::new(15_000);
            let mut client = fixture_client(endpoint, time, &d, json!({"pull_requests":"write"}));
            publication::publish(
                &publication::Context {
                    store: &engine.store,
                    host: &host,
                    options: &o,
                    clock: &clock,
                },
                &mut client,
                &request,
                &remote_publish(),
                &d,
            )
        });
        waiting
            .recv_timeout(Duration::from_secs(5))
            .expect("owned POST must reach its durable intent");
        let status = publication::StatusRequest {
            schema_version: 1,
            request_id: id,
        };
        let unknown =
            publication::status(&publication_context(&f, &options(&f)), &status, &f.d).unwrap();
        assert_eq!(unknown["status"], "unknown");
        let recovery = Provider::configured(
            Mode::RecoveryRead,
            None,
            42,
            Some(preview["body"].as_str().unwrap().into()),
            None,
        );
        let mut client = fixture_client(
            recovery.endpoint,
            f.time.clone(),
            &f.d,
            json!({"pull_requests":"read"}),
        );
        let d = Deadline::new(400);
        fail_marker(
            publication::recover(
                &publication_context(&f, &options(&f)),
                &mut client,
                &recovery_request(&unknown),
                &remote_publish(),
                &d,
            ),
            "Request deadline exceeded",
        );
        assert!(recovery.seen.lock().unwrap().is_empty());
        assert_eq!(
            publication::status(&publication_context(&f, &options(&f)), &status, &f.d).unwrap()["status"],
            "unknown"
        );
        drop(release);
        assert_eq!(publishing.join().unwrap().unwrap()["status"], "published");
        assert_eq!(
            publication::status(&publication_context(&f, &options(&f)), &status, &f.d).unwrap()["status"],
            "published"
        );
        let count: usize = f
            .engine
            .store
            .db
            .query_row(
                "SELECT count(*) FROM records WHERE kind=?1",
                [publication::RECORD_KIND],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count, 2);
    });
    assert_eq!(
        provider
            .seen
            .lock()
            .unwrap()
            .iter()
            .filter(|r| r.path == "/repos/acme/demo/issues/7/comments")
            .count(),
        1
    );
}

#[test]
fn recovery_works_with_pull_request_read_only_and_no_publication_consent() {
    let (f, _, preview, _, unknown) = unknown_fixture();
    let provider = Provider::configured(
        Mode::RecoveryRead,
        None,
        42,
        Some(preview["body"].as_str().unwrap().into()),
        Some(9),
    );
    let mut client = fixture_client(
        provider.endpoint,
        f.time.clone(),
        &f.d,
        json!({"pull_requests":"read"}),
    );
    let o = options(&f);
    assert!(!o.allow_publication);
    let value = publication::recover(
        &publication_context(&f, &o),
        &mut client,
        &recovery_request(&unknown),
        &remote_publish(),
        &f.d,
    )
    .unwrap();
    assert_eq!(value["reconciled"], true);
    assert_eq!(provider.seen.lock().unwrap().len(), 6);
    assert!(
        !provider
            .seen
            .lock()
            .unwrap()
            .iter()
            .any(|r| r.method == "PATCH" || r.path == "/repos/acme/demo/issues/7/comments")
    );
}

#[test]
fn recovery_journal_and_network_permissions_precede_history_or_http() {
    let f = Fixture::new();
    let provider = Provider::new(Mode::RecoveryRead);
    let mut client = fixture_client(
        provider.endpoint,
        f.time.clone(),
        &f.d,
        json!({"pull_requests":"read"}),
    );
    let request = publication::RecoverRequest {
        schema_version: 1,
        intent_id: "a".repeat(64),
        comment_id: 501,
        installation_id: 42,
    };
    let mut o = options(&f);
    o.worker.queue.allow_writes = false;
    fail_marker(
        publication::recover(
            &publication_context(&f, &o),
            &mut client,
            &request,
            &remote_publish(),
            &f.d,
        ),
        "github_app_permission_required",
    );
    o.worker.queue.allow_writes = true;
    fail_marker(
        publication::recover(
            &publication_context(&f, &o),
            &mut client,
            &request,
            &publication::PublishOptions::default(),
            &f.d,
        ),
        "github_app_network_permission_required",
    );
    assert!(provider.seen.lock().unwrap().is_empty());
}

fn cancel_on_commit(f: &Fixture, number: u64) -> Deadline {
    let cancel = Arc::new(AtomicBool::new(false));
    let signal = cancel.clone();
    let commits = Arc::new(AtomicU64::new(0));
    f.engine.store.db.commit_hook(Some(move || {
        if commits.fetch_add(1, Ordering::SeqCst) + 1 == number {
            signal.store(true, Ordering::SeqCst);
        }
        false
    }));
    // Capture inherited cancellation before the direct API call. An outer bounded
    // wrapper must not supply the missing final check for the operation under test.
    Deadline::run_bounded(&f.d, cancel, || Ok(Deadline::new(30_000))).unwrap()
}
#[test]
fn cancellation_at_publication_terminal_commit_is_reported_with_durable_historical_success() {
    let f = Fixture::new();
    let result = completed(&f);
    let preview = publication::preview(
        &f.engine.store,
        &f.host,
        &preview_request(&result),
        &options(&f),
        &f.d,
    )
    .unwrap();
    let request = publish_request(&result, &preview);
    let provider = Provider::new(Mode::Publishing);
    let mut client = fixture_client(
        provider.endpoint,
        f.time.clone(),
        &f.d,
        json!({"pull_requests":"write"}),
    );
    let mut o = options(&f);
    o.allow_publication = true;
    let d = cancel_on_commit(&f, 2);
    let value = publication::publish(
        &publication_context(&f, &o),
        &mut client,
        &request,
        &remote_publish(),
        &d,
    );
    f.engine.store.db.commit_hook(None::<fn() -> bool>);
    fail_marker(value, "Request cancelled by host");
    let historical = publish_with(&f, &request, &provider, 42).unwrap();
    assert_eq!(historical["status"], "published");
    assert_eq!(historical["historical"], true);
    assert_eq!(provider.seen.lock().unwrap().len(), 9);
}
#[test]
fn cancellation_at_recovery_commit_is_reported_and_status_exposes_durable_reconciliation() {
    let (f, _, preview, original, unknown) = unknown_fixture();
    let provider = Provider::configured(
        Mode::RecoveryRead,
        None,
        42,
        Some(preview["body"].as_str().unwrap().into()),
        None,
    );
    let mut client = fixture_client(
        provider.endpoint,
        f.time.clone(),
        &f.d,
        json!({"pull_requests":"read"}),
    );
    let o = options(&f);
    let request = recovery_request(&unknown);
    let d = cancel_on_commit(&f, 1);
    let value = publication::recover(
        &publication_context(&f, &o),
        &mut client,
        &request,
        &remote_publish(),
        &d,
    );
    f.engine.store.db.commit_hook(None::<fn() -> bool>);
    fail_marker(value, "Request cancelled by host");
    let historical = publication::status(
        &publication_context(&f, &o),
        &publication::StatusRequest {
            schema_version: 1,
            request_id: original.request_id,
        },
        &f.d,
    )
    .unwrap();
    assert_eq!(historical["reconciled"], true);
    assert_eq!(historical["historical"], true);
    assert_eq!(provider.seen.lock().unwrap().len(), 6);
}
#[test]
fn cancellation_at_unchanged_commit_reports_cancellation_after_atomic_observation() {
    let (f, result, preview, _) = published_fixture();
    let request = reselected_request(&f, &result);
    let provider = Provider::configured(
        Mode::Publishing,
        None,
        42,
        Some(preview["body"].as_str().unwrap().into()),
        None,
    );
    let mut client = fixture_client(
        provider.endpoint,
        f.time.clone(),
        &f.d,
        json!({"pull_requests":"write"}),
    );
    let mut o = options(&f);
    o.allow_publication = true;
    let d = cancel_on_commit(&f, 1);
    let value = publication::publish(
        &publication_context(&f, &o),
        &mut client,
        &request,
        &remote_publish(),
        &d,
    );
    f.engine.store.db.commit_hook(None::<fn() -> bool>);
    fail_marker(value, "Request cancelled by host");
    let historical = publish_with(&f, &request, &provider, 42).unwrap();
    assert_eq!(historical["unchanged"], true);
    assert_eq!(historical["historical"], true);
    assert_eq!(provider.seen.lock().unwrap().len(), 9);
}
#[test]
fn first_publication_refuses_a_host_clock_older_than_its_signed_completion_before_auth() {
    let f = Fixture::new();
    let result = completed(&f);
    let preview = publication::preview(
        &f.engine.store,
        &f.host,
        &preview_request(&result),
        &options(&f),
        &f.d,
    )
    .unwrap();
    let provider = Provider::new(Mode::Publishing);
    let mut client = fixture_client(
        provider.endpoint,
        f.time.clone(),
        &f.d,
        json!({"pull_requests":"write"}),
    );
    let mut o = options(&f);
    o.allow_publication = true;
    let clock = worker::Clock::for_fixture(Arc::new(AtomicU64::new(NOW - 1)));
    let c = publication::Context {
        store: &f.engine.store,
        host: &f.host,
        options: &o,
        clock: &clock,
    };
    fail_marker(
        publication::publish(
            &c,
            &mut client,
            &publish_request(&result, &preview),
            &remote_publish(),
            &f.d,
        ),
        "github_app_publication_clock_regressed",
    );
    assert!(provider.seen.lock().unwrap().is_empty());
}

#[test]
fn retention_recognizes_publication_history_and_keeps_only_typed_source_roots() {
    let (mut f, _, _, _) = published_fixture();
    std::fs::write(f.root.path().join(PATH), "fn abandoned() {}\n").unwrap();
    let abandoned = f.engine.index(&f.d).unwrap().0.id;
    std::fs::write(f.root.path().join(PATH), "fn current() {}\n").unwrap();
    f.engine.index(&f.d).unwrap();
    let value =
        yoke::retention::plan(&f.engine.store, &yoke::retention::Policy::default(), &f.d).unwrap();
    assert_eq!(value["unknownDependencyKinds"], json!([]));
    let ids = value["snapshotIds"].as_array().unwrap();
    assert!(ids.contains(&json!(abandoned)));
    assert!(!ids.contains(&json!(f.snapshot)));
}

fn rewrite_intent(f: &Fixture, change: impl FnOnce(&mut Value)) {
    let mut rows: Vec<(String, Value)> = f
        .engine
        .store
        .db
        .prepare(
            "SELECT id,value FROM records WHERE kind=?1 ORDER BY json_extract(value,'$.sequence')",
        )
        .unwrap()
        .query_map([publication::RECORD_KIND], |r| {
            Ok((
                r.get(0)?,
                serde_json::from_str::<Value>(&r.get::<_, String>(1)?).unwrap(),
            ))
        })
        .unwrap()
        .map(Result::unwrap)
        .collect();
    change(&mut rows[0].1["event"]["data"]);
    let data = &mut rows[0].1["event"]["data"];
    data["bodyHash"] = json!(yoke::model::hash(data["body"].as_str().unwrap().as_bytes()));
    let j = &data["job"];
    let contract = json!({"schemaVersion":1,"rendererVersion":"yoke-pr-comment-v1",
        "workspaceId":j["workspaceId"],"appId":j["appId"],"installationId":j["installationId"],
        "repositoryId":j["repositoryId"],"repository":j["repository"],"number":j["number"],
        "jobId":j["id"],"completionId":data["completionId"],"queueRevision":data["queueRevision"],
        "headSha":j["headSha"],"baseSha":j["baseSha"],"headSnapshotId":data["headSnapshotId"],
        "baseSnapshotId":data["baseSnapshotId"],"reviewHash":data["reviewHash"],"bodyHash":data["bodyHash"],"body":data["body"]});
    data["proposalId"] = json!(yoke::model::identity(&contract));
    let data = data.clone();
    let request = &mut rows[0].1["event"]["request"];
    request["completionId"] = data["completionId"].clone();
    request["bodyHash"] = data["bodyHash"].clone();
    request["proposalId"] = data["proposalId"].clone();
    let intent = reseal_publication(&mut rows[0].1);
    rows[1].1["event"]["intentId"] = json!(intent);
    rows[1].1["event"]["comment"]["bodyHash"] = data["bodyHash"].clone();
    for (old, mut value) in rows {
        let id = reseal_publication(&mut value);
        f.engine
            .store
            .db
            .execute(
                "UPDATE records SET id=?1,value=?2 WHERE kind=?3 AND id=?4",
                rusqlite::params![id, value.to_string(), publication::RECORD_KIND, old],
            )
            .unwrap();
    }
}
#[test]
fn retention_refuses_resealed_orphan_completion_and_body_not_derived_from_the_report() {
    for case in ["completion", "body"] {
        let (f, _, _, _) = published_fixture();
        rewrite_intent(&f, |data| {
            if case == "completion" {
                data["completionId"] = json!("f".repeat(64));
            } else {
                data["body"] = json!(format!(
                    "{}\ninert-unrelated-id:{}\n",
                    data["body"].as_str().unwrap(),
                    "e".repeat(64)
                ));
            }
        });
        assert!(
            yoke::retention::plan(&f.engine.store, &yoke::retention::Policy::default(), &f.d)
                .is_err(),
            "publication {case} must retain only the validated completed source contract"
        );
    }
}
