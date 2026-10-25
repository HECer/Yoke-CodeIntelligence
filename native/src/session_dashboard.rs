//! Bounded native session dashboard with separate host opt-in for management.
//! Private capabilities and call arguments/results never enter activity records.
use crate::{capture::Deadline, model::valid_id, paths, store::Store};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, VecDeque},
    io::{Read, Write},
    net::{Ipv4Addr, TcpListener, TcpStream},
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread::JoinHandle,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use subtle::ConstantTimeEq;

#[path = "session_dashboard_management.rs"]
pub(crate) mod management;
pub use management::Config as ManagementConfig;
pub use management::LanguageGrant;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields, default)]
pub struct Config {
    pub port: u16,
    pub request_timeout_ms: u64,
    pub lifetime_ms: u64,
    pub max_requests: usize,
    pub activity_capacity: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub management: Option<ManagementConfig>,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            port: 0,
            request_timeout_ms: 1000,
            lifetime_ms: 3_600_000,
            max_requests: 10000,
            activity_capacity: 200,
            management: None,
        }
    }
}
impl Config {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            (10..=5000).contains(&self.request_timeout_ms),
            "dashboard_request_timeout_capacity"
        );
        ensure!(
            (100..=86_400_000).contains(&self.lifetime_ms),
            "dashboard_lifetime_capacity"
        );
        ensure!(
            (1..=100000).contains(&self.max_requests),
            "dashboard_request_capacity"
        );
        ensure!(
            (1..=1000).contains(&self.activity_capacity),
            "dashboard_activity_capacity"
        );
        if let Some(management) = &self.management {
            management.validate()?;
        }
        Ok(())
    }
}
struct View {
    expected_state: String,
    value: Value,
    activity: VecDeque<Value>,
    next_id: u64,
    dropped: u64,
    completed: u64,
    failed: u64,
    running: u64,
}
pub(crate) struct Handle {
    host: String,
    token: String,
    state: Arc<Mutex<View>>,
    stop: Arc<AtomicBool>,
    alive: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
    capacity: usize,
    started: Instant,
    management: Option<ManagementConfig>,
}
pub(crate) struct Activity {
    id: u64,
    start: Instant,
}
pub(crate) struct Binding {
    pub root: PathBuf,
    pub workspace: String,
    pub key: String,
    pub expected_state: String,
    pub closed: Option<Arc<AtomicBool>>,
    pub management: Option<management::Client>,
}
fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(u64::MAX as u128) as u64
}
fn event_name(operation: &str) -> &'static str {
    // Unknown caller text is never logged, even when it resembles a tool name.
    match operation {
        "configuration" => "configuration",
        "listProjects" => "listProjects",
        "activate" => "activate",
        "removeProject" => "removeProject",
        "switchModes" => "switchModes",
        "onboarding" => "onboarding",
        "initialInstructions" => "initialInstructions",
        "queryProject" => "queryProject",
        "notes.list" => "notes.list",
        "notes.read" => "notes.read",
        "notes.write" => "notes.write",
        "notes.delete" => "notes.delete",
        "notes.rename" => "notes.rename",
        "notes.edit" => "notes.edit",
        "fs.list" => "fs.list",
        "fs.glob" => "fs.glob",
        "fs.read" => "fs.read",
        "fs.search" => "fs.search",
        "symbols.find" => "symbols.find",
        "symbols.references" => "symbols.references",
        "symbols.fileApi" => "symbols.fileApi",
        "edit.prepare" => "edit.prepare",
        "refactor.prepare" => "refactor.prepare",
        "openDashboard" => "openDashboard",
        "ci_status" => "ci_status",
        "ci_find" => "ci_find",
        "ci_read" => "ci_read",
        "ci_trace" => "ci_trace",
        "ci_context" => "ci_context",
        "dashboard.languages" => "dashboard.languages",
        "dashboard.languages.reset" => "dashboard.languages.reset",
        "dashboard.documents.read" => "dashboard.documents.read",
        "dashboard.documents.plan" => "dashboard.documents.plan",
        "dashboard.documents.apply" => "dashboard.documents.apply",
        "dashboard.documents.inspect" => "dashboard.documents.inspect",
        "dashboard.documents.pending" => "dashboard.documents.pending",
        "dashboard.documents.recover" => "dashboard.documents.recover",
        "dashboard.logs.read" => "dashboard.logs.read",
        "dashboard.shutdown" => "dashboard.shutdown",
        _ => "other_operation",
    }
}
impl Handle {
    pub(crate) fn start(
        config: &Config,
        binding: Binding,
        value: Value,
        deadline: &Deadline,
    ) -> Result<Self> {
        let Binding {
            root,
            workspace,
            key,
            expected_state,
            closed,
            management,
        } = binding;
        config.validate()?;
        deadline.check()?;
        ensure!(
            valid_id(&workspace) && valid_id(&key),
            "dashboard_session_binding_invalid"
        );
        ensure!(
            expected_state.len() <= 128000 && serde_json::to_vec(&value)?.len() <= 400000,
            "dashboard_view_capacity"
        );
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, config.port))?;
        listener.set_nonblocking(true)?;
        let host = listener.local_addr()?.to_string();
        let mut secret = [0u8; 32];
        getrandom::fill(&mut secret)
            .map_err(|_| anyhow::anyhow!("dashboard_capability_unavailable"))?;
        let token = hex::encode(secret);
        let state = Arc::new(Mutex::new(View {
            expected_state,
            value,
            activity: VecDeque::new(),
            next_id: 0,
            dropped: 0,
            completed: 0,
            failed: 0,
            running: 0,
        }));
        let stop = Arc::new(AtomicBool::new(false));
        let alive = Arc::new(AtomicBool::new(true));
        let (worker_state, worker_stop, worker_alive) =
            (state.clone(), stop.clone(), alive.clone());
        let capacity = config.activity_capacity;
        let management_config = config.management.clone();
        let (worker_host, worker_token, config) = (host.clone(), token.clone(), config.clone());
        let worker = std::thread::Builder::new()
            .name("native-session-dashboard".into())
            .spawn(move || {
                let start = Instant::now();
                let mut served = 0usize;
                while !worker_stop.load(Ordering::Acquire)
                    && !closed.as_ref().is_some_and(|v| v.load(Ordering::SeqCst))
                    && start.elapsed() < Duration::from_millis(config.lifetime_ms)
                    && served < config.max_requests
                {
                    match listener.accept() {
                        Ok((mut stream, peer)) => {
                            served += 1;
                            if !peer.ip().is_loopback() {
                                continue;
                            }
                            // Winsock inherits the listener's nonblocking mode.
                            // Each accepted request instead uses its original
                            // bounded read/write deadlines, including split input.
                            if stream.set_nonblocking(false).is_err() {
                                continue;
                            }
                            let request_deadline = Deadline::from_duration(
                                Duration::from_millis(config.request_timeout_ms).min(
                                    Duration::from_millis(config.lifetime_ms)
                                        .saturating_sub(start.elapsed()),
                                ),
                            );
                            let mut response_deadline=request_deadline.clone();
                            let mut management_status=200;
                            let mut completion=None;
                            let outcome = request(
                                &mut stream,
                                &worker_host,
                                &worker_token,
                                &request_deadline,
                                management.is_some(),
                            )
                            .and_then(|route| {
                                if closed.as_ref().is_some_and(|v| v.load(Ordering::SeqCst)) {
                                    return Err(Failure(410, "Dashboard access revoked"));
                                }
                                match route {
                                    Route::State => snapshot(&root,&workspace,&key,&worker_state,&request_deadline).map(|data|(data,"application/json; charset=utf-8")),
                                    Route::Bootstrap => Ok((include_bytes!("session_dashboard.html").to_vec(),"text/html; charset=utf-8")),
                                    Route::Manage(body) => {
                                        let selected=config.management.as_ref().ok_or(Failure(405,"Dashboard management unavailable"))?;
                                        let time=Duration::from_millis(selected.operation_timeout_ms).min(Duration::from_millis(config.lifetime_ms).saturating_sub(start.elapsed()));
                                        response_deadline=Deadline::from_duration(time);
                                        let request=crate::json::parse(&body,128000).and_then(crate::json::decode::<management::Request>).map_err(|_|Failure(400,"Invalid management request"))?;
                                        let client=management.as_ref().ok_or(Failure(405,"Dashboard management unavailable"))?;
                                        let mut delivery=client.call(request,response_deadline.clone(),worker_stop.clone(),closed.clone());
                                        completion=delivery.reply.completion.take();
                                        let (reply,bytes)=delivery.encode(&response_deadline).map_err(|_|Failure(408,"Management outcome unconfirmed; refresh before retrying"))?;
                                        management_status=reply.status;
                                        Ok((bytes,"application/json; charset=utf-8"))
                                    }
                                }
                            });
                            let outcome =
                                if closed.as_ref().is_some_and(|v| v.load(Ordering::SeqCst)) {
                                    Err(Failure(410, "Dashboard access revoked"))
                                } else {
                                    outcome
                                };
                            let (status, bytes, media) = match outcome {
                                Ok((bytes, media)) => (management_status, bytes, media),
                                Err(Failure(status, message)) => (
                                    status,
                                    message.as_bytes().to_vec(),
                                    "text/plain; charset=utf-8",
                                ),
                            };
                            let _ = respond(&mut stream, status, &bytes, media, &response_deadline);
                            if let Some(completion)=completion { completion.finish(); }
                        }
                        Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                            std::thread::sleep(Duration::from_millis(10))
                        }
                        Err(_) => break,
                    }
                }
                worker_alive.store(false, Ordering::Release);
            })?;
        let handle = Self {
            host,
            token,
            state,
            stop,
            alive,
            worker: Some(worker),
            capacity,
            started: Instant::now(),
            management: management_config,
        };
        deadline.check()?;
        Ok(handle)
    }
    pub(crate) fn alive(&self) -> bool {
        self.alive.load(Ordering::Acquire)
    }
    pub(crate) fn result(&self) -> Value {
        let mut value = json!({"url":format!("http://{}/#{}",self.host,self.token),"browserOpened":false,
            "browserOpening":"client_managed","access":"private_ipv4_loopback_capability",
            "scope":"read_only_native_session_and_structured_activity","ageMs":self.age_ms(),"complete":false,
            "capabilities":{"sessionOverview":true,"selectedTools":true,"structuredActivity":true,
                "rawLogs":false,"configurationEditing":false,"memoryEditing":false,"mcpShutdown":false}});
        if let Some(config) = &self.management {
            value["scope"] = json!("private_native_session_management");
            value["capabilities"]["sessionControls"] = json!(config.session_controls);
            value["capabilities"]["rawLogs"] = json!(config.raw_logs.is_some());
            if let Ok(state) = self.state.try_lock() {
                shutdown_capabilities(&mut value, &state.value["management"]["shutdown"]);
            }
            if config.language_servers.is_some() {
                value["capabilities"]["languageSelection"] = json!(config.session_controls);
            }
            value["capabilities"]["memoryEditing"] = json!(
                config.memories
                    && self.state.try_lock().is_ok_and(|state| {
                        state.value["management"]["memoryWritesGranted"] == true
                            && state.value["management"]["noMemoriesMode"] != true
                    })
            );
            if config.documents.is_some()
                && let Ok(state) = self.state.try_lock()
            {
                document_capabilities(&mut value, &state.value["management"]["configurationFiles"]);
            }
        }
        value
    }
    pub(crate) fn begin(&self, operation: &str) -> Option<Activity> {
        if !self.alive() {
            return None;
        }
        let mut s = self.state.lock().ok()?;
        let id = s.next_id.checked_add(1)?;
        s.next_id = id;
        while s.activity.len() >= self.capacity {
            s.activity.pop_front();
            s.dropped = s.dropped.saturating_add(1);
        }
        s.activity.push_back(json!({"id":id,"operation":event_name(operation),"startedAtMs":now_ms(),"status":"running"}));
        s.running = s.running.saturating_add(1);
        Some(Activity {
            id,
            start: Instant::now(),
        })
    }
    pub(crate) fn finish(
        &self,
        activity: Option<Activity>,
        failed: bool,
        state: String,
        view: Result<Value>,
    ) {
        let Ok(mut s) = self.state.lock() else {
            return;
        };
        if let Some(activity) = activity {
            s.running = s.running.saturating_sub(1);
            s.completed = s.completed.saturating_add(1);
            if failed {
                s.failed = s.failed.saturating_add(1);
            }
            if let Some(row) = s.activity.iter_mut().find(|row| row["id"] == activity.id) {
                row["status"] = json!(if failed { "failed" } else { "completed" });
                row["durationMs"] =
                    json!(activity.start.elapsed().as_millis().min(u64::MAX as u128) as u64);
            }
        }
        s.expected_state = state;
        s.value = view.unwrap_or_else(
            |_| json!({"unavailable":true,"reason":"Current dashboard view unavailable"}),
        );
    }
    pub(crate) fn age_ms(&self) -> u64 {
        self.started.elapsed().as_millis().min(u64::MAX as u128) as u64
    }
}
impl Drop for Handle {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}
pub(crate) struct Failure(u16, &'static str);
enum Route {
    Bootstrap,
    State,
    Manage(Vec<u8>),
}
type HttpResult<T> = std::result::Result<T, Failure>;
fn remaining(d: &Deadline) -> HttpResult<Duration> {
    d.check()
        .map_err(|_| Failure(408, "Request deadline exceeded"))?;
    let left = d.remaining();
    if left.is_zero() {
        Err(Failure(408, "Request deadline exceeded"))
    } else {
        Ok(left)
    }
}
fn request(
    stream: &mut TcpStream,
    host: &str,
    token: &str,
    d: &Deadline,
    can_manage: bool,
) -> HttpResult<Route> {
    let mut bytes = Vec::with_capacity(1024);
    while !bytes.ends_with(b"\r\n\r\n") {
        if bytes.len() >= 16384 {
            return Err(Failure(431, "HTTP header budget exceeded"));
        }
        stream
            .set_read_timeout(Some(remaining(d)?))
            .map_err(|_| Failure(400, "HTTP read unavailable"))?;
        let mut byte = [0];
        if stream
            .read(&mut byte)
            .map_err(|_| Failure(408, "Request deadline exceeded"))?
            != 1
        {
            return Err(Failure(400, "Truncated HTTP request"));
        }
        bytes.push(byte[0]);
        if byte[0] == b'\n' && (bytes.len() < 2 || bytes[bytes.len() - 2] != b'\r') {
            return Err(Failure(400, "HTTP requires CRLF framing"));
        }
    }
    let mut fields = [httparse::EMPTY_HEADER; 64];
    let mut p = httparse::Request::new(&mut fields);
    if !matches!(p.parse(&bytes), Ok(httparse::Status::Complete(end)) if end==bytes.len())
        || p.version != Some(1)
    {
        return Err(Failure(400, "Invalid HTTP request"));
    }
    let mut headers = BTreeMap::new();
    for field in p.headers {
        let value =
            std::str::from_utf8(field.value).map_err(|_| Failure(400, "Invalid header value"))?;
        if headers
            .insert(field.name.to_ascii_lowercase(), value)
            .is_some()
        {
            return Err(Failure(400, "Duplicate HTTP header"));
        }
    }
    if headers.get("host").copied() != Some(host)
        || headers
            .get("origin")
            .is_some_and(|v| **v != format!("http://{host}"))
    {
        return Err(Failure(403, "Host or Origin denied"));
    }
    let private = p.path != Some("/");
    if private {
        let supplied = headers.get("authorization").copied().unwrap_or("");
        if !bool::from(
            supplied
                .as_bytes()
                .ct_eq(format!("Bearer {token}").as_bytes()),
        ) {
            return Err(Failure(401, "Authorization required"));
        }
    }
    if headers.contains_key("transfer-encoding") || headers.contains_key("expect") {
        return Err(Failure(400, "Unsupported HTTP framing"));
    }
    if can_manage && p.method == Some("POST") && p.path == Some("/api/manage") {
        if headers.get("origin").copied() != Some(format!("http://{host}").as_str()) {
            return Err(Failure(403, "Management Origin required"));
        }
        if !matches!(
            headers.get("content-type").copied(),
            Some("application/json" | "application/json; charset=utf-8")
        ) {
            return Err(Failure(415, "JSON management body required"));
        }
        let raw = headers
            .get("content-length")
            .copied()
            .ok_or(Failure(411, "Content-Length required"))?;
        let length = raw
            .parse::<usize>()
            .map_err(|_| Failure(400, "Invalid body length"))?;
        if length == 0 || length > 128000 || raw != length.to_string() {
            return Err(Failure(413, "Management body capacity"));
        }
        let mut body = vec![0; length];
        let mut offset = 0;
        while offset < length {
            stream
                .set_read_timeout(Some(remaining(d)?))
                .map_err(|_| Failure(400, "HTTP read unavailable"))?;
            let count = stream
                .read(&mut body[offset..])
                .map_err(|_| Failure(408, "Request deadline exceeded"))?;
            if count == 0 {
                return Err(Failure(400, "Truncated management body"));
            }
            offset += count;
        }
        no_pipeline(stream, d)?;
        return Ok(Route::Manage(body));
    }
    if let Some(length) = headers.get("content-length")
        && (length.is_empty()
            || !length.bytes().all(|b| b.is_ascii_digit())
            || length.parse::<usize>().ok() != Some(0))
    {
        return Err(Failure(400, "GET body refused"));
    }
    if p.method != Some("GET") {
        return Err(Failure(405, "Read-only dashboard"));
    }
    if !matches!(p.path, Some("/" | "/api/state")) {
        return Err(Failure(404, "Endpoint unavailable"));
    }
    no_pipeline(stream, d)?;
    Ok(if private {
        Route::State
    } else {
        Route::Bootstrap
    })
}
fn no_pipeline(stream: &mut TcpStream, d: &Deadline) -> HttpResult<()> {
    stream
        .set_nonblocking(true)
        .map_err(|_| Failure(400, "HTTP read unavailable"))?;
    let extra = stream.peek(&mut [0]);
    stream
        .set_nonblocking(false)
        .map_err(|_| Failure(400, "HTTP read unavailable"))?;
    if extra.is_ok_and(|n| n > 0) {
        return Err(Failure(400, "HTTP pipelining refused"));
    }
    remaining(d)?;
    Ok(())
}

pub(crate) fn check_binding(
    root: &Path,
    workspace: &str,
    key: &str,
    expected_state: &str,
    d: &Deadline,
) -> HttpResult<()> {
    for ancestor in root.ancestors() {
        let metadata = std::fs::symlink_metadata(ancestor)
            .map_err(|_| Failure(409, "Session source unavailable"))?;
        if !metadata.is_dir() || paths::is_link(&metadata) {
            return Err(Failure(409, "Session source identity unavailable"));
        }
        remaining(d)?;
    }
    let store = Store::open_existing_read_only(root, d)
        .map_err(|_| Failure(409, "Session state unavailable"))?;
    if store.workspace_id != workspace {
        return Err(Failure(409, "Session workspace changed"));
    }
    let current: bool = store
        .db
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM records WHERE kind=?1 AND id=?2 AND value=?3)",
            rusqlite::params![crate::project_session::RECORD_KIND, key, expected_state],
            |r| r.get(0),
        )
        .map_err(|_| Failure(409, "Session state unavailable"))?;
    if !current {
        return Err(Failure(409, "Session changed; resume the host session"));
    }
    Ok(())
}
fn snapshot(
    root: &Path,
    workspace: &str,
    key: &str,
    state: &Mutex<View>,
    d: &Deadline,
) -> HttpResult<Vec<u8>> {
    let (expected_state, view, activity, statistics) = {
        let s = view_lock(state, d, || {})?;
        (
            s.expected_state.clone(),
            s.value.clone(),
            s.activity.clone(),
            json!({"completed":s.completed,"failed":s.failed,"running":s.running,"droppedRows":s.dropped}),
        )
    };
    check_binding(root, workspace, key, &expected_state, d)?;
    let managed = view.get("management").cloned();
    let mut data = json!({"schema":"native_session_dashboard_v1","view":view,"activity":activity,
        "statistics":statistics,
        "activityScope":"native_session_operations_since_dashboard_opened","argumentsRecorded":false,"resultsRecorded":false,
        "capabilities":{"rawLogs":false,"configurationEditing":false,"memoryEditing":false,"mcpShutdown":false},"complete":false});
    if let Some(config) = managed {
        data["capabilities"]["sessionControls"] = config["sessionControls"].clone();
        if config["languageSelection"].is_object() {
            data["capabilities"]["languageSelection"] = config["sessionControls"].clone();
        }
        document_capabilities(&mut data, &config["configurationFiles"]);
        data["capabilities"]["rawLogs"] = json!(config["rawLogs"].is_object());
        shutdown_capabilities(&mut data, &config["shutdown"]);
        data["capabilities"]["memoryEditing"] = serde_json::json!(
            config["memories"] == true
                && config["memoryWritesGranted"] == true
                && config["noMemoriesMode"] != true
        );
    }
    crate::json::encode_bounded(&data, 1_000_000, d)
        .map_err(|_| Failure(413, "Dashboard response budget exceeded"))
}

fn view_lock<'a>(
    state: &'a Mutex<View>,
    d: &Deadline,
    mut waiting: impl FnMut(),
) -> HttpResult<std::sync::MutexGuard<'a, View>> {
    loop {
        // Publishing activity briefly holds this mutex. Share the original
        // request deadline instead of treating ordinary contention as failure.
        remaining(d)?;
        match state.try_lock() {
            Ok(view) => return Ok(view),
            Err(std::sync::TryLockError::Poisoned(_)) => {
                return Err(Failure(503, "Dashboard view unavailable"));
            }
            Err(std::sync::TryLockError::WouldBlock) => {
                waiting();
                std::thread::sleep(remaining(d)?.min(Duration::from_millis(1)));
            }
        }
    }
}

fn shutdown_capabilities(value: &mut Value, shutdown: &Value) {
    if shutdown.is_object() {
        value["capabilities"]["mcpShutdown"] = json!(true);
        value["capabilities"]["shutdownScope"] = shutdown["scope"].clone();
        value["capabilities"]["serviceShutdown"] = json!(false);
    }
}
fn document_capabilities(value: &mut Value, documents: &Value) {
    if let Some(choices) = documents["choices"].as_array() {
        value["capabilities"]["configurationReading"] = json!(true);
        value["capabilities"]["configurationEditing"] =
            json!(choices.iter().any(|c| c["writable"] == true));
        value["capabilities"]["projectConfigurationEditing"] = json!(
            choices
                .iter()
                .any(|c| c["scope"]["kind"] == "project" && c["writable"] == true)
        );
    }
}
fn respond(
    stream: &mut TcpStream,
    status: u16,
    bytes: &[u8],
    media: &str,
    d: &Deadline,
) -> HttpResult<()> {
    let header = format!(
        "HTTP/1.1 {status} {}\r\nContent-Type: {media}\r\nContent-Length: {}\r\nConnection: close\r\nCache-Control: no-store\r\nReferrer-Policy: no-referrer\r\nX-Content-Type-Options: nosniff\r\nX-Frame-Options: DENY\r\nContent-Security-Policy: default-src 'none'; script-src 'unsafe-inline'; style-src 'unsafe-inline'; connect-src 'self'; base-uri 'none'; form-action 'none'; frame-ancestors 'none'\r\n\r\n",
        if status == 200 {
            "OK"
        } else {
            "Request Refused"
        },
        bytes.len()
    );
    if status == 408 {
        stream
            .set_nonblocking(true)
            .map_err(|_| Failure(408, "Response deadline exceeded"))?;
        let _ = stream.write(header.as_bytes());
        let _ = stream.write(bytes);
        return Ok(());
    }
    for part in [header.as_bytes(), bytes] {
        let mut at = 0;
        while at < part.len() {
            stream
                .set_write_timeout(Some(remaining(d)?))
                .map_err(|_| Failure(408, "Response deadline exceeded"))?;
            let n = stream
                .write(&part[at..])
                .map_err(|_| Failure(400, "Response unavailable"))?;
            if n == 0 {
                return Err(Failure(400, "Response closed"));
            }
            at += n;
        }
    }
    if status >= 400 {
        // Windows may reset a socket closed with unread rejected input and lose
        // its response. Drain only currently available, bounded rejected bytes.
        let _ = stream.shutdown(std::net::Shutdown::Write);
        stream
            .set_nonblocking(true)
            .map_err(|_| Failure(400, "Response unavailable"))?;
        let mut buffer = [0u8; 4096];
        for _ in 0..4 {
            if d.check().is_err() {
                break;
            }
            match stream.read(&mut buffer) {
                Ok(0) | Err(_) => break,
                Ok(_) => {}
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod view_lock_tests {
    use super::*;

    fn view() -> View {
        View {
            expected_state: "before".into(),
            value: json!({"revision":"before"}),
            activity: VecDeque::new(),
            next_id: 0,
            dropped: 0,
            completed: 0,
            failed: 0,
            running: 0,
        }
    }

    #[test]
    fn brief_publication_contention_returns_one_coherent_current_view() {
        let state = Arc::new(Mutex::new(view()));
        let mut publisher = state.lock().unwrap();
        let (blocked, observed) = std::sync::mpsc::sync_channel(1);
        let reader_state = state.clone();
        let reader = std::thread::spawn(move || {
            let mut blocked = Some(blocked);
            let deadline = Deadline::new(1000);
            match view_lock(&reader_state, &deadline, || {
                if let Some(sender) = blocked.take() {
                    let _ = sender.send(());
                }
            }) {
                Ok(view) => Some((
                    view.expected_state.clone(),
                    view.value.clone(),
                    view.running,
                )),
                Err(_) => None,
            }
        });
        let was_blocked = observed.recv_timeout(Duration::from_secs(2));
        publisher.expected_state = "after".into();
        publisher.value = json!({"revision":"after"});
        publisher.running = 1;
        // Release the writer and join before any assertion can panic.
        drop(publisher);
        let result = reader.join();
        assert!(
            was_blocked.is_ok(),
            "Reader never observed actual contention"
        );
        assert_eq!(
            result.unwrap(),
            Some(("after".into(), json!({"revision":"after"}), 1))
        );
    }

    #[test]
    fn expired_original_request_never_gets_a_new_snapshot_wait_budget() {
        let state = Mutex::new(view());
        let expired = Deadline::new(0);
        // Even an available mutex cannot revive an expired request.
        assert!(matches!(
            view_lock(&state, &expired, || panic!("Unexpected wait")),
            Err(Failure(408, _))
        ));
        let publisher = state.lock().unwrap();
        assert!(matches!(
            view_lock(&state, &expired, || panic!("Expired request waited")),
            Err(Failure(408, _))
        ));
        drop(publisher);
    }

    #[test]
    fn poisoned_view_refuses_without_waiting_or_recovering_its_contents() {
        let state = Arc::new(Mutex::new(view()));
        let publisher_state = state.clone();
        assert!(
            std::thread::spawn(move || {
                let _publisher = publisher_state.lock().unwrap();
                panic!("Owned publisher poison fixture");
            })
            .join()
            .is_err()
        );
        assert!(matches!(
            view_lock(&state, &Deadline::new(1000), || panic!(
                "Poisoned view waited"
            )),
            Err(Failure(503, _))
        ));
    }

    #[test]
    fn host_cancellation_while_contended_stops_the_existing_request() {
        let state = Mutex::new(view());
        let publisher = state.lock().unwrap();
        let cancelled = Arc::new(AtomicBool::new(false));
        let signal = cancelled.clone();
        let mut refused = false;
        let result = Deadline::run_bounded(&Deadline::new(1000), cancelled, || {
            let request = Deadline::new(1000);
            refused = matches!(
                view_lock(&state, &request, || signal.store(true, Ordering::SeqCst)),
                Err(Failure(408, _))
            );
            Ok(())
        });
        drop(publisher);
        assert!(refused, "Contended view ignored host cancellation");
        assert_eq!(result.unwrap_err().to_string(), "Request cancelled by host");
    }
}
