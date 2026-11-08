// Owned local Git + loopback App transport. No real GitHub/OS credential lookup.
use base64::{Engine as _, prelude::BASE64_STANDARD};
use hmac::{Hmac, Mac};
use serde_json::{Value, json};
use std::{io::{Read, Write}, process::Command, sync::{Arc, OnceLock, atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering}}, thread, time::Duration};
use yoke::{Engine, capture::Deadline, git_workflows::{self, HostGit}, github_app::{self as app, worker::{self, history}, digest::{self, history_jobs as jobs}}, github_app_auth as auth};
use zeroize::Zeroizing;
const KEY: &[u8] = b"owned public app history signing fixture key";
const TOKEN: &str = "owned-app-history-token-canary_2026";
const NOW: u64 = 1_900_000_000;
const PATH: &str = "caf\u{e9}.rs";
const SOURCE: &str = "// caf\u{e9}\r\npub fn target() -> u32 { 2 }\r\npub fn caller() -> u32 { target() }\r\n";
fn host(key: &[u8]) -> app::Host {
    app::Host::with_signing_key(app::HostProfile::parse(br#"{"schemaVersion":1,"appId":123,"secretCredential":"not-read-by-fixture","grants":[{"installationId":42,"repositoryId":9,"repository":"acme/demo"}]}"#).unwrap(), app::SigningKey::from_host_bytes(key).unwrap()).unwrap()
}
fn git(g: &HostGit, root: &std::path::Path, args: &[&str]) -> String {
    let output = Command::new(&g.executable).current_dir(root)
        .env("GIT_CONFIG_NOSYSTEM", "1").env("GIT_CONFIG_GLOBAL", "NUL").env("GIT_CONFIG_SYSTEM", "NUL")
        .env("GIT_AUTHOR_DATE", "1800000000 +0000").env("GIT_COMMITTER_DATE", "1800000000 +0000")
        .args(["-c", "init.templateDir=", "-c", "core.hooksPath=NUL", "-c", "core.autocrlf=false", "-c", "commit.gpgsign=false", "-c", "user.name=App History Fixture", "-c", "user.email=fixture@example.invalid"])
        .args(args).output().unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    String::from_utf8(output.stdout).unwrap().trim().into()
}
fn app_key() -> auth::AppKey {
    static KEY: OnceLock<Zeroizing<Vec<u8>>> = OnceLock::new();
    let bytes = KEY.get_or_init(|| {
        let output = Command::new("pwsh.exe").args(["-NoProfile", "-NonInteractive", "-Command", "$rsa=[System.Security.Cryptography.RSA]::Create(2048); try { [Convert]::ToBase64String($rsa.ExportRSAPrivateKey()) } finally { $rsa.Dispose() }"]).output().unwrap();
        assert!(output.status.success());
        let raw = Zeroizing::new(output.stdout);
        Zeroizing::new(BASE64_STANDARD.decode(std::str::from_utf8(&raw).unwrap().trim()).unwrap())
    });
    auth::AppKey::from_host_bytes(bytes).unwrap()
}
struct Fixture {
    root: tempfile::TempDir, engine: Engine, git: HostGit, host: app::Host,
    worker: worker::Options, clock: worker::Clock, time: Arc<AtomicU64>,
    request: worker::RunRequest, head: String, base: String,
}
impl Fixture {
    fn new(captured: bool) -> Self {
        let root = tempfile::tempdir().unwrap(); let git_tool = HostGit::discover().unwrap();
        git(&git_tool, root.path(), &["init", "-q"]);
        std::fs::write(root.path().join(".gitignore"), b".yoke/\n").unwrap();
        std::fs::write(root.path().join(PATH), SOURCE.replace("{ 2 }", "{ 1 }")).unwrap();
        git(&git_tool, root.path(), &["add", "--all"]); git(&git_tool, root.path(), &["commit", "-q", "-m", "base"]);
        let base = git(&git_tool, root.path(), &["rev-parse", "HEAD"]);
        std::fs::write(root.path().join(PATH), SOURCE).unwrap();
        git(&git_tool, root.path(), &["add", "--all"]); git(&git_tool, root.path(), &["commit", "-q", "-m", "head"]);
        let head = git(&git_tool, root.path(), &["rev-parse", "HEAD"]);
        let d = Deadline::new(30_000); let mut engine = Engine::open(root.path()).unwrap();
        engine.capture_epoch = yoke::analysis::AnalyzerEpoch::V8;
        let origin = engine.index(&d).unwrap().0;
        let snapshot = if captured { git_workflows::capture_commit_at(engine.capture_epoch, &mut engine, &git_tool, &origin, &head, &d).unwrap().0.id } else { origin.id };
        let host = host(KEY);
        let worker = worker::Options { queue: app::Options { allow_writes:true, ..Default::default() }, ..Default::default() };
        let event = serde_json::to_vec(&json!({"action":"opened","number":7,"installation":{"id":42},"repository":{"id":9,"full_name":"acme/demo"},"pull_request":{"number":7,"state":"open","draft":false,"title":"Owned history fixture","head":{"sha":head},"base":{"sha":base,"repo":{"id":9,"full_name":"acme/demo"}}}})).unwrap();
        let mut mac = Hmac::<sha2::Sha256>::new_from_slice(KEY).unwrap(); mac.update(&event);
        let headers = app::Headers::parse(&serde_json::to_vec(&json!({"schemaVersion":1,"event":"pull_request","deliveryId":"01234567-89ab-4cde-8000-000000000001","signature256":format!("sha256={}",hex::encode(mac.finalize().into_bytes()))})).unwrap()).unwrap();
        let admission = app::ingest(&engine.store, &host, &headers, &event, &worker.queue, &d).unwrap();
        let job = admission["jobId"].as_str().unwrap();
        app::select(&engine.store, &host, job, 1, &worker.queue, &d).unwrap();
        let time = Arc::new(AtomicU64::new(NOW)); let clock = worker::Clock::for_fixture(time.clone());
        let claim = worker::claim(&engine.store, &host, &worker::ClaimRequest { schema_version:1, job_id:job.into(), expected_queue_revision:2, worker:"history-fixture".into(), request_id:"89abcdef-0123-4abc-8000-000000000001".into(), lease_seconds:120 }, &worker, &clock, &d).unwrap();
        let request = worker::RunRequest { schema_version:1, claim_id:claim.evidence()["claimId"].as_str().unwrap().into(), head_snapshot_id:snapshot, base_snapshot_id:None, request_id:"89abcdef-0123-4abc-8000-000000000002".into() };
        Self {root, engine, git:git_tool, host, worker, clock, time, request, head, base}
    }
    fn complete(&self) -> (history::Selection, Value) {
        let provider = Provider::new(self.head.clone(), self.base.clone()); let d=Deadline::new(30_000);
        let profile = auth::AuthProfile::parse(br#"{"schemaVersion":1,"appId":123,"keyCredential":"not-read-by-fixture","grants":[{"installationId":42,"repositoryId":9,"repository":"acme/demo"}],"permissions":{"contents":"read","pull_requests":"read"}}"#).unwrap();
        let mut client = auth::Client::for_fixture(profile, app_key(), auth::FixtureProfile::loopback_with_clock(provider.endpoint, self.time.clone()).unwrap(), auth::Options {allow_network:true, ..Default::default()}, &d).unwrap();
        let result=worker::run(&worker::RunContext {store:&self.engine.store, host:&self.host, options:&self.worker, clock:&self.clock}, &mut client, &self.request, &worker::RunOptions {allow_network:true, ..Default::default()}, &d).unwrap();
        assert_eq!(result["status"], "completed"); assert_eq!(provider.seen.load(Ordering::SeqCst), 8);
        (history::Selection { schema_version:1, completion_id:result["completionId"].as_str().unwrap().into(), expected_queue_revision:Some(2) }, result)
    }
    fn options(&self) -> history::Options {
        history::Options { allow_app_history:true, worker:self.worker.clone(), digest:digest::Options::default(), jobs:jobs::Options { allow_jobs:true, max_batch_visits:1, acquisition:digest::acquisition::Options {allow_capture:true, allow_writes:true, ..Default::default()}, ..Default::default() } }
    }
    fn read<'a>(&'a self, options:&'a history::Options) -> history::Context<'a> { history::Context {store:&self.engine.store, host:&self.host, options} }
    fn write<'a>(&'a self, options:&'a history::Options) -> history::WriteContext<'a> { history::WriteContext {engine:&self.engine, host:&self.host, options} }
    fn facts(&self) -> Vec<(String,String,String)> {
        self.engine.store.db.prepare("SELECT kind,id,value FROM records ORDER BY kind,id").unwrap().query_map([],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).unwrap().collect::<Result<_,_>>().unwrap()
    }
}
struct Provider { endpoint:std::net::SocketAddr, seen:Arc<AtomicUsize>, stop:Arc<AtomicBool>, thread:Option<thread::JoinHandle<()>> }
impl Provider {
    fn new(head:String, base:String) -> Self {
        use sha1::{Digest, Sha1};
        let blob=hex::encode(Sha1::digest([format!("blob {}\0",SOURCE.len()).as_bytes(), SOURCE.as_bytes()].concat()));
        let listener=std::net::TcpListener::bind("127.0.0.1:0").unwrap(); listener.set_nonblocking(true).unwrap();
        let endpoint=listener.local_addr().unwrap(); let seen=Arc::new(AtomicUsize::new(0)); let stop=Arc::new(AtomicBool::new(false));
        let counted=seen.clone();let stopped=stop.clone();
        let thread=thread::spawn(move || { while !stopped.load(Ordering::SeqCst) {
            let Ok((mut stream,_))=listener.accept() else { thread::sleep(Duration::from_millis(2)); continue };
            stream.set_read_timeout(Some(Duration::from_secs(2))).unwrap(); stream.set_write_timeout(Some(Duration::from_secs(2))).unwrap();
            let mut raw=Zeroizing::new(Vec::new()); let mut b=[0]; while raw.len()<65536 && !raw.ends_with(b"\r\n\r\n") { if stream.read(&mut b).unwrap_or(0)!=1 {break} raw.push(b[0]); }
            let header=Zeroizing::new(String::from_utf8(std::mem::take(&mut *raw)).unwrap());
            let route=header.lines().next().unwrap().split_whitespace().nth(1).unwrap();
            let length=header.lines().find_map(|l|l.split_once(':').filter(|(k,_)|k.eq_ignore_ascii_case("content-length")).map(|(_,v)|v.trim().parse::<usize>().unwrap())).unwrap_or(0);assert!(length<10000);
            let mut body=Zeroizing::new(vec![0;length]);stream.read_exact(&mut body).unwrap(); counted.fetch_add(1,Ordering::SeqCst);
            let permissions=json!({"contents":"read","pull_requests":"read"}); let repo=json!({"id":9,"full_name":"acme/demo","archived":false});
            let value=match route {
                "/app"=>json!({"id":123}),
                "/app/installations/42"=>json!({"id":42,"app_id":123,"suspended_at":null,"suspended_by":null,"permissions":permissions}),
                "/app/installations/42/access_tokens"=> {let date=time::OffsetDateTime::from_unix_timestamp((NOW+3600) as i64).unwrap(); let expires=format!("{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",date.year(),u8::from(date.month()),date.day(),date.hour(),date.minute(),date.second()); json!({"token":TOKEN,"expires_at":expires,"permissions":permissions,"repository_selection":"selected","repositories":[repo]})},
                "/installation/repositories?per_page=100&page=1"=>json!({"total_count":1,"repositories":[repo]}),
                "/repos/acme/demo"=>repo.clone(),
                "/repos/acme/demo/pulls/7"=>json!({"id":77,"number":7,"state":"open","draft":false,"changed_files":1,"head":{"sha":head,"repo":repo},"base":{"sha":base,"repo":repo}}),
                "/repos/acme/demo/pulls/7/files?per_page=100&page=1"=>json!([{"sha":blob,"filename":PATH,"status":"modified","additions":1,"deletions":1,"changes":2,"patch":"@@ -2 +2 @@\n-pub fn target() -> u32 { 1 }\r\n+pub fn target() -> u32 { 2 }\r"}]),
                _=>panic!("unexpected owned fixture route"),
            }.to_string();
            write!(stream,"HTTP/1.1 200 Fixture\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{value}",value.len()).unwrap();
        }});
        Self {endpoint,seen,stop,thread:Some(thread)}
    }
}
impl Drop for Provider {fn drop(&mut self) {self.stop.store(true,Ordering::SeqCst);self.thread.take().unwrap().join().unwrap();}}
