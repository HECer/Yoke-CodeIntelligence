//! Explicit host-owned MCP Streamable HTTP. JSON replies, no unsolicited SSE.
pub use crate::federation_http::TlsMaterial;
use crate::{
    Engine,
    capture::{CaptureOptions, Deadline},
    project_session::{HostProfile, Session},
    protocol::{ConnectionState, Profile},
};
use anyhow::{Context, Result, bail, ensure};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    io::{Read, Write},
    net::{Shutdown, SocketAddr, TcpListener, TcpStream},
    path::PathBuf,
    sync::{
        Arc, Mutex, RwLock,
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc,
    },
    thread,
    time::{Duration, Instant},
};
use subtle::ConstantTimeEq;
use zeroize::Zeroizing;

const MAX_RESPONSE: usize = 16_000_000;
#[cfg(test)]
#[path = "mcp_http_rejection_tests.rs"]
mod rejection_tests;
const VERSIONS: [&str; 3] = ["2025-11-25", "2025-06-18", "2025-03-26"];

/// Trusted host input, never reconstructed from an MCP message.
#[derive(Clone)]
pub struct Target {
    pub root: PathBuf,
    pub profile: Profile,
    pub host: HostProfile,
    pub capture_options: CaptureOptions,
    pub servers: Vec<crate::lsp::ConfiguredServer>,
    pub allow_writes: bool,
    pub agent_repl: Option<crate::agent_repl::ReplSpec>,
    /// Current principal's host-selected grants, grouped by registered collection.
    /// These remain in policy, never in a session, queued job or client message.
    pub collection_grants: BTreeMap<String, BTreeMap<String, crate::federation::ImportGrant>>,
}
impl Target {
    pub fn new(root: impl Into<PathBuf>, profile: Profile, host: HostProfile) -> Self {
        Self {
            root: root.into(),
            profile,
            host,
            capture_options: CaptureOptions::default(),
            servers: Vec::new(),
            allow_writes: false,
            agent_repl: None,
            collection_grants: BTreeMap::new(),
        }
    }
    fn open(&self, deadline: &Deadline) -> Result<Session> {
        deadline.check()?;
        let mut engine = Engine::open_with_timeout(&self.root, deadline.remaining())?;
        engine.allow_writes = self.allow_writes;
        engine.capture_options = self.capture_options.validated()?;
        for server in &self.servers {
            deadline.check()?;
            engine.register_configured_server(server.clone())?;
        }
        // This independent random label is not the transport session secret.
        Session::open(
            engine,
            self.host.clone(),
            Some(&format!("http-{}", random_id()?)),
            deadline,
        )
    }
}

#[derive(Clone)]
pub struct Principal {
    id: String,
    token_hash: [u8; 32],
    generation: [u8; 32],
    target: Target,
}
impl Principal {
    pub fn new(id: &str, token: Zeroizing<Vec<u8>>, mut target: Target) -> Result<Self> {
        identifier(id)?;
        ensure!(
            token.len() == 32,
            "MCP host credential must contain 32 bytes"
        );
        target.root = target.root.canonicalize()?;
        ensure!(
            target.root.is_dir() && target.servers.len() <= 64 && target.host.projects.len() <= 100,
            "Invalid MCP host target or capacity"
        );
        ensure!(
            serde_json::to_vec(&target.host)?.len() <= 1_000_000,
            "MCP host profile capacity exceeded"
        );
        ensure!(
            !target.allow_writes || target.host.modes.iter().any(|mode| mode == "editing"),
            "Host write authority requires editing mode"
        );
        target.capture_options = target.capture_options.validated()?;
        ensure!(
            target.collection_grants.is_empty() || target.profile == Profile::Session,
            "Collection grants require the Session profile"
        );
        ensure!(
            target.collection_grants.len() <= 100,
            "Collection policy capacity exceeded"
        );
        for (name, grants) in &target.collection_grants {
            let collection = target
                .host
                .collections
                .iter()
                .find(|c| &c.name == name)
                .context("Collection grant policy is not host-registered")?;
            ensure!(
                collection.receiver_project.is_some() && grants.len() <= 100,
                "Collection grant policy requires a registered receiver and bounded grants"
            );
            for id in grants.keys() {
                crate::graph_collections::validate_registration_name(id)?;
            }
        }
        crate::json::encode_bounded(&target.collection_grants, 1_000_000, &Deadline::new(30_000))?;
        if let Some(spec) = &target.agent_repl {
            ensure!(
                target.profile == Profile::Serena,
                "Persistent Python is available only in the Serena profile"
            );
            crate::agent_repl::AgentRepl::validate_spec(spec)?;
        }
        // Validate source/session host authority before a listener can be opened.
        drop(target.open(&Deadline::new(30_000))?);
        Ok(Self {
            id: id.into(),
            token_hash: Sha256::digest(token.as_slice()).into(),
            generation: random_bytes()?,
            target,
        })
    }
}

pub struct Config {
    pub bind: SocketAddr,
    pub tls: Option<TlsMaterial>,
    pub allow_remote_bind: bool,
    pub server_names: Vec<String>,
    pub allowed_origins: Vec<String>,
    pub endpoint: String,
    pub request_timeout_ms: u64,
    pub max_body_bytes: usize,
    pub max_connections: usize,
    pub workers: usize,
    pub max_sessions: usize,
    pub max_sessions_per_principal: usize,
    pub session_ttl_ms: u64,
    pub max_requests_per_session: u64,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            bind: "127.0.0.1:0".parse().expect("literal address"),
            tls: None,
            allow_remote_bind: false,
            server_names: vec![],
            allowed_origins: vec![],
            endpoint: "/mcp".into(),
            request_timeout_ms: 15_000,
            max_body_bytes: 4_000_000,
            max_connections: 16,
            workers: 2,
            max_sessions: 64,
            max_sessions_per_principal: 8,
            session_ttl_ms: 1_800_000,
            max_requests_per_session: 10_000,
        }
    }
}
struct Policy {
    principals: BTreeMap<String, Principal>,
}
impl Policy {
    fn authenticate(&self, hash: &[u8; 32]) -> Option<&Principal> {
        self.principals
            .values()
            .find(|principal| bool::from(principal.token_hash.ct_eq(hash)))
    }
    fn insert(&mut self, mut principal: Principal) -> Result<()> {
        ensure!(
            self.principals.len() < 256 || self.principals.contains_key(&principal.id),
            "MCP principal capacity exceeded"
        );
        ensure!(
            !self
                .principals
                .values()
                .any(|other| other.id != principal.id
                    && bool::from(other.token_hash.ct_eq(&principal.token_hash))),
            "Duplicate MCP host credential"
        );
        principal.generation = random_bytes()?;
        self.principals.insert(principal.id.clone(), principal);
        Ok(())
    }
}
struct Meta {
    principal: String,
    generation: [u8; 32],
    worker: usize,
    born: Instant,
    lifetime: Deadline,
    closed: Arc<AtomicBool>,
    version: Mutex<Option<String>>,
    requests: Mutex<BTreeMap<String, Arc<AtomicBool>>>,
    count: AtomicU64,
}
struct Shared {
    policy: RwLock<Policy>,
    sessions: Mutex<BTreeMap<String, Arc<Meta>>>,
    stop: Arc<AtomicBool>,
    active: Mutex<BTreeMap<u64, TcpStream>>,
    cleanup_failed: AtomicBool,
}
impl Shared {
    fn prune(&self, config: &Config) -> Result<()> {
        let policy = self
            .policy
            .read()
            .map_err(|_| anyhow::anyhow!("MCP policy lock unavailable"))?;
        self.sessions
            .lock()
            .map_err(|_| anyhow::anyhow!("MCP session lock unavailable"))?
            .retain(|_, meta| {
                let keep = !meta.closed.load(Ordering::SeqCst)
                    && meta.born.elapsed() < Duration::from_millis(config.session_ttl_ms)
                    && policy
                        .principals
                        .get(&meta.principal)
                        .is_some_and(|principal| principal.generation == meta.generation);
                if !keep {
                    meta.closed.store(true, Ordering::SeqCst);
                }
                keep
            });
        Ok(())
    }
    fn remove(&self, id: &str) {
        if let Ok(mut sessions) = self.sessions.lock()
            && let Some(meta) = sessions.remove(id)
        {
            meta.closed.store(true, Ordering::SeqCst);
        }
    }
}
pub struct ServerHandle {
    address: SocketAddr,
    shared: Arc<Shared>,
    worker: Option<thread::JoinHandle<()>>,
    completed: mpsc::Receiver<bool>,
    finish_attempted: bool,
}
/// Cloneable trusted-host control; never exposed through MCP arguments.
#[derive(Clone)]
pub struct Control {
    shared: Arc<Shared>,
}
impl ServerHandle {
    pub fn local_addr(&self) -> SocketAddr {
        self.address
    }
    pub fn controller(&self) -> Control {
        Control {
            shared: Arc::clone(&self.shared),
        }
    }
    pub fn session_count(&self) -> Result<usize> {
        self.controller().session_count()
    }
    pub fn revoke(&self, id: &str) -> Result<bool> {
        self.controller().revoke(id)
    }
    pub fn upsert(&self, principal: Principal) -> Result<()> {
        self.controller().upsert(principal)
    }
}
impl Control {
    pub fn pending_requests(&self) -> Result<usize> {
        let sessions = self
            .shared
            .sessions
            .lock()
            .map_err(|_| anyhow::anyhow!("MCP session lock unavailable"))?;
        let mut count = 0;
        for meta in sessions.values() {
            count += meta
                .requests
                .lock()
                .map_err(|_| anyhow::anyhow!("MCP request lock unavailable"))?
                .len();
        }
        Ok(count)
    }
    pub fn session_count(&self) -> Result<usize> {
        Ok(self
            .shared
            .sessions
            .lock()
            .map_err(|_| anyhow::anyhow!("MCP session lock unavailable"))?
            .len())
    }
    pub fn revoke(&self, id: &str) -> Result<bool> {
        identifier(id)?;
        // Cancel queued/in-flight work before waiting for a dispatcher read lease.
        // Recheck after removal also covers sessions admitted during that wait.
        self.invalidate(id);
        let removed = self
            .shared
            .policy
            .write()
            .map_err(|_| anyhow::anyhow!("MCP policy lock unavailable"))?
            .principals
            .remove(id)
            .is_some();
        self.invalidate(id);
        Ok(removed)
    }
    pub fn upsert(&self, principal: Principal) -> Result<()> {
        let id = principal.id.clone();
        // Cancel current readers before waiting for their policy lease, as in
        // revoke. Otherwise a superseded operation can delay replacement until
        // its old request deadline. Dispatch still rechecks the new generation.
        self.invalidate(&id);
        self.shared
            .policy
            .write()
            .map_err(|_| anyhow::anyhow!("MCP policy lock unavailable"))?
            .insert(principal)?;
        self.invalidate(&id);
        Ok(())
    }
    fn invalidate(&self, id: &str) {
        if let Ok(mut sessions) = self.shared.sessions.lock() {
            sessions.retain(|_, meta| {
                if meta.principal == id {
                    meta.closed.store(true, Ordering::SeqCst);
                    false
                } else {
                    true
                }
            });
        }
    }
}
impl ServerHandle {
    pub fn shutdown(mut self) -> Result<()> {
        self.finish()
    }
    fn finish(&mut self) -> Result<()> {
        if self.worker.is_none() {
            return Ok(());
        }
        self.finish_attempted = true;
        self.shared.stop.store(true, Ordering::SeqCst);
        if let Ok(active) = self.shared.active.lock() {
            for socket in active.values() {
                let _ = socket.shutdown(Shutdown::Both);
            }
        }
        if let Ok(sessions) = self.shared.sessions.lock() {
            for meta in sessions.values() {
                meta.closed.store(true, Ordering::SeqCst);
            }
        }
        let clean = self.completed.recv_timeout(Duration::from_secs(5)).context(
            "MCP worker cleanup could not be proven; host recovery required (cooperative cancellation only)"
        )?;
        if let Some(worker) = self.worker.take() {
            worker
                .join()
                .map_err(|_| anyhow::anyhow!("MCP listener worker failed"))?;
        }
        ensure!(
            clean,
            "MCP worker failed; cleanup completed but host recovery required"
        );
        Ok(())
    }
}
impl Drop for ServerHandle {
    fn drop(&mut self) {
        if self.worker.is_some() && !self.finish_attempted && self.finish().is_err() {
            eprintln!("MCP worker cleanup could not be proven; host recovery required");
        }
    }
}
fn identifier(value: &str) -> Result<()> {
    ensure!(
        !value.is_empty()
            && value.len() <= 128
            && value
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || b"_.-".contains(&byte)),
        "Invalid MCP host identifier"
    );
    Ok(())
}
fn random_bytes() -> Result<[u8; 32]> {
    let mut bytes = [0; 32];
    getrandom::fill(&mut bytes)
        .map_err(|_| anyhow::anyhow!("MCP secure randomness unavailable"))?;
    Ok(bytes)
}
fn random_id() -> Result<String> {
    Ok(hex::encode(random_bytes()?))
}

#[derive(Clone, Copy)]
struct Fault(u16, &'static str);
type HttpResult<T> = std::result::Result<T, Fault>;
struct Reply {
    status: u16,
    body: Vec<u8>,
    session: Option<String>,
    origin: Option<String>,
}
impl Reply {
    fn empty(status: u16) -> Self {
        Self {
            status,
            body: vec![],
            session: None,
            origin: None,
        }
    }
    fn fault(fault: Fault) -> Self {
        Self {
            status: fault.0,
            body: serde_json::to_vec(&json!({"error":fault.1})).expect("static error"),
            session: None,
            origin: None,
        }
    }
}

struct BoundSocket<'a> {
    socket: TcpStream,
    deadline: &'a Deadline,
    stop: &'a AtomicBool,
}
impl Drop for BoundSocket<'_> {
    fn drop(&mut self) {
        // Closing this endpoint must not depend on the lifetime of cloned or
        // inherited Winsock handles retained by an explicitly started worker.
        let _ = self.socket.shutdown(Shutdown::Both);
    }
}
impl BoundSocket<'_> {
    fn finish_rejection(&mut self) {
        // A close with unread TCP bytes can reset the connection and discard
        // the framed refusal on Windows. Half-close after sending the response,
        // then discard a bounded tail without parsing or admitting another request.
        let _ = self.socket.shutdown(Shutdown::Write);
        let end = Instant::now() + self.deadline.remaining().min(Duration::from_millis(50));
        let mut bytes = Zeroizing::new([0u8; 4096]);
        let mut remaining_bytes = 16_384;
        while remaining_bytes > 0 && !self.stop.load(Ordering::SeqCst) {
            let remaining = end
                .saturating_duration_since(Instant::now())
                .min(self.deadline.remaining());
            if remaining.is_zero() || self.socket.set_read_timeout(Some(remaining)).is_err() {
                break;
            }
            let want = remaining_bytes.min(bytes.len());
            match self.socket.read(&mut bytes[..want]) {
                Ok(0) => break,
                Ok(count) => remaining_bytes -= count,
                Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(_) => break,
            }
        }
    }
    fn budget(&self) -> std::io::Result<Duration> {
        if self.stop.load(Ordering::SeqCst) {
            return Err(std::io::Error::new(
                std::io::ErrorKind::ConnectionAborted,
                "MCP host shutdown",
            ));
        }
        let remaining = self.deadline.remaining();
        if remaining.is_zero() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::TimedOut,
                "MCP I/O deadline",
            ));
        }
        Ok(remaining.min(Duration::from_millis(50)))
    }
}
impl Read for BoundSocket<'_> {
    fn read(&mut self, bytes: &mut [u8]) -> std::io::Result<usize> {
        self.socket.set_read_timeout(Some(self.budget()?))?;
        let result = self.socket.read(bytes);
        self.budget()?;
        result
    }
}
impl Write for BoundSocket<'_> {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.socket.set_write_timeout(Some(self.budget()?))?;
        let result = self.socket.write(bytes);
        self.budget()?;
        result
    }
    fn flush(&mut self) -> std::io::Result<()> {
        self.budget()?;
        let result = self.socket.flush();
        self.budget()?;
        result
    }
}
enum Channel<'a> {
    Plain(BoundSocket<'a>),
    Tls(Box<rustls::StreamOwned<rustls::ServerConnection, BoundSocket<'a>>>),
}
impl Read for Channel<'_> {
    fn read(&mut self, bytes: &mut [u8]) -> std::io::Result<usize> {
        match self {
            Self::Plain(socket) => socket.read(bytes),
            Self::Tls(socket) => socket.read(bytes),
        }
    }
}
impl Write for Channel<'_> {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        match self {
            Self::Plain(socket) => socket.write(bytes),
            Self::Tls(socket) => socket.write(bytes),
        }
    }
    fn flush(&mut self) -> std::io::Result<()> {
        match self {
            Self::Plain(socket) => socket.flush(),
            Self::Tls(socket) => socket.flush(),
        }
    }
}
fn retry(error: &std::io::Error) -> bool {
    matches!(
        error.kind(),
        std::io::ErrorKind::WouldBlock
            | std::io::ErrorKind::TimedOut
            | std::io::ErrorKind::Interrupted
    )
}
fn physical_read(
    io: &mut Channel<'_>,
    buffer: &mut [u8],
    deadline: &Deadline,
    stop: &AtomicBool,
) -> Result<usize> {
    loop {
        deadline.check()?;
        ensure!(!stop.load(Ordering::SeqCst), "MCP host shutdown");
        match io.read(buffer) {
            Ok(size) => return Ok(size),
            Err(error) if retry(&error) => {}
            Err(error) => return Err(error.into()),
        }
    }
}
fn physical_write(
    io: &mut Channel<'_>,
    mut bytes: &[u8],
    deadline: &Deadline,
    stop: &AtomicBool,
) -> Result<()> {
    while !bytes.is_empty() {
        deadline.check()?;
        ensure!(!stop.load(Ordering::SeqCst), "MCP host shutdown");
        match io.write(bytes) {
            Ok(0) => bail!("MCP output closed"),
            Ok(size) => bytes = &bytes[size..],
            Err(error) if retry(&error) => {}
            Err(error) => return Err(error.into()),
        }
    }
    loop {
        deadline.check()?;
        match io.flush() {
            Ok(()) => return Ok(()),
            Err(error) if retry(&error) => {}
            Err(error) => return Err(error.into()),
        }
    }
}

struct Incoming {
    method: String,
    headers: BTreeMap<String, Zeroizing<String>>,
    token_hash: Option<[u8; 32]>,
    body: Zeroizing<Vec<u8>>,
}
fn authority(name: &str, port: u16) -> String {
    if name.parse::<std::net::Ipv6Addr>().is_ok() {
        format!("[{name}]:{port}")
    } else {
        format!("{name}:{port}")
    }
}
fn host_allowed(host: &str, address: SocketAddr, config: &Config) -> bool {
    host == address.to_string()
        || config
            .server_names
            .iter()
            .any(|name| host.eq_ignore_ascii_case(&authority(name, address.port())))
}
fn credential(value: &str) -> HttpResult<[u8; 32]> {
    let text = value
        .strip_prefix("Bearer ")
        .ok_or(Fault(401, "Unauthorized"))?;
    if !crate::model::valid_id(text) {
        return Err(Fault(401, "Unauthorized"));
    }
    let mut bytes = Zeroizing::new([0; 32]);
    hex::decode_to_slice(text, &mut *bytes).map_err(|_| Fault(401, "Unauthorized"))?;
    Ok(Sha256::digest(bytes.as_slice()).into())
}
fn accepts(value: &str, mime: &str) -> bool {
    value.split(',').any(|part| {
        let mut fields = part.trim().split(';');
        let same = fields
            .next()
            .is_some_and(|value| value.trim().eq_ignore_ascii_case(mime));
        same && fields.all(|field| {
            let Some((name, value)) = field.trim().split_once('=') else {
                return false;
            };
            if name.eq_ignore_ascii_case("q") {
                value.parse::<f64>().is_ok_and(|q| q > 0.0 && q <= 1.0)
            } else {
                true
            }
        })
    })
}
fn incoming(
    io: &mut Channel<'_>,
    config: &Config,
    address: SocketAddr,
    shared: &Shared,
    deadline: &Deadline,
    response_origin: &mut Option<String>,
) -> HttpResult<Incoming> {
    let mut raw = Zeroizing::new(Vec::new());
    let mut buffer = [0; 8192];
    let parsed;
    loop {
        let mut headers = [httparse::EMPTY_HEADER; 48];
        let mut request = httparse::Request::new(&mut headers);
        match request.parse(&raw) {
            Ok(httparse::Status::Complete(end)) => {
                if end > 16_384
                    || request.version != Some(1)
                    || raw[..end]
                        .iter()
                        .enumerate()
                        .any(|(i, b)| *b == b'\n' && (i == 0 || raw[i - 1] != b'\r'))
                {
                    return Err(Fault(400, "Invalid HTTP framing"));
                }
                let method = request
                    .method
                    .ok_or(Fault(400, "Method absent"))?
                    .to_owned();
                if request.path != Some(config.endpoint.as_str()) {
                    return Err(Fault(404, "MCP endpoint not found"));
                }
                let mut fields = BTreeMap::new();
                for header in request.headers.iter() {
                    let value = std::str::from_utf8(header.value)
                        .map_err(|_| Fault(400, "Invalid header encoding"))?;
                    if value
                        .bytes()
                        .any(|byte| byte < 32 && byte != b'\t' || byte == 127)
                        || fields
                            .insert(
                                header.name.to_ascii_lowercase(),
                                Zeroizing::new(value.trim().to_owned()),
                            )
                            .is_some()
                    {
                        return Err(Fault(400, "Invalid or duplicate header"));
                    }
                }
                let host = fields.get("host").ok_or(Fault(400, "Host absent"))?;
                if !host_allowed(host, address, config) {
                    return Err(Fault(403, "Host not authorized"));
                }
                if let Some(origin) = fields.get("origin") {
                    if !config
                        .allowed_origins
                        .iter()
                        .any(|allowed| allowed == origin.as_str())
                    {
                        return Err(Fault(403, "Origin not authorized"));
                    }
                    *response_origin = Some(origin.to_string());
                }
                if matches!(io,Channel::Tls(stream)if stream.conn.server_name().is_some_and(|name|!config.server_names.iter().any(|allowed|allowed.eq_ignore_ascii_case(name))))
                {
                    return Err(Fault(403, "TLS server name not authorized"));
                }
                if ["transfer-encoding", "expect", "upgrade", "trailer"]
                    .iter()
                    .any(|key| fields.contains_key(*key))
                {
                    return Err(Fault(400, "Unsupported HTTP framing"));
                }
                if fields.get("connection").is_some_and(|value| {
                    !value.eq_ignore_ascii_case("close")
                        && !value.eq_ignore_ascii_case("keep-alive")
                }) {
                    return Err(Fault(400, "Unsupported HTTP connection mode"));
                }
                if let Some(version) = fields.get("mcp-protocol-version")
                    && !VERSIONS.contains(&version.as_str())
                {
                    return Err(Fault(400, "Unsupported MCP protocol version"));
                }
                let length = match fields.get("content-length") {
                    Some(value)
                        if !value.is_empty() && value.bytes().all(|b| b.is_ascii_digit()) =>
                    {
                        value
                            .parse::<usize>()
                            .map_err(|_| Fault(400, "Invalid content length"))?
                    }
                    Some(_) => return Err(Fault(400, "Invalid content length")),
                    None => 0,
                };
                if length > config.max_body_bytes {
                    return Err(Fault(413, "Request body capacity exceeded"));
                }
                if method == "POST" {
                    let content = fields
                        .get("content-type")
                        .ok_or(Fault(415, "JSON content type required"))?;
                    let mut parts = content.split(';');
                    if !parts
                        .next()
                        .is_some_and(|value| value.trim().eq_ignore_ascii_case("application/json"))
                        || !parts.all(|value| value.trim().eq_ignore_ascii_case("charset=utf-8"))
                    {
                        return Err(Fault(415, "UTF-8 JSON content type required"));
                    }
                    if length == 0 {
                        return Err(Fault(400, "JSON body required"));
                    }
                    if !fields.get("accept").is_some_and(|value| {
                        accepts(value, "application/json") && accepts(value, "text/event-stream")
                    }) {
                        return Err(Fault(406, "Accept must support JSON and event streams"));
                    }
                } else if length != 0 {
                    return Err(Fault(400, "Unexpected request body"));
                }
                let token_hash = if method == "OPTIONS" {
                    None
                } else {
                    let hash = credential(
                        fields
                            .get("authorization")
                            .ok_or(Fault(401, "Unauthorized"))?,
                    )?;
                    if shared
                        .policy
                        .read()
                        .map_err(|_| Fault(503, "Host policy unavailable"))?
                        .authenticate(&hash)
                        .is_none()
                    {
                        return Err(Fault(401, "Unauthorized"));
                    }
                    Some(hash)
                };
                parsed = (method, fields, token_hash, end, length);
                break;
            }
            Ok(httparse::Status::Partial) => {}
            Err(_) => return Err(Fault(400, "Malformed HTTP request")),
        }
        if raw.len() >= 16_384 {
            return Err(Fault(431, "Header capacity exceeded"));
        }
        let want = (16_384 - raw.len()).min(buffer.len());
        let count = physical_read(io, &mut buffer[..want], deadline, &shared.stop)
            .map_err(|_| Fault(408, "Request deadline or transport closed"))?;
        if count == 0 {
            return Err(Fault(400, "Truncated HTTP request"));
        }
        raw.extend_from_slice(&buffer[..count]);
    }
    let (method, headers, token_hash, end, length) = parsed;
    let expected = end
        .checked_add(length)
        .ok_or(Fault(413, "Body capacity exceeded"))?;
    if raw.len() > expected {
        return Err(Fault(400, "Pipelined request bytes refused"));
    }
    while raw.len() < expected {
        let want = (expected - raw.len()).min(buffer.len());
        let count = physical_read(io, &mut buffer[..want], deadline, &shared.stop)
            .map_err(|_| Fault(408, "Request deadline or transport closed"))?;
        if count == 0 {
            return Err(Fault(400, "Truncated request body"));
        }
        raw.extend_from_slice(&buffer[..count]);
    }
    Ok(Incoming {
        method,
        headers,
        token_hash,
        body: Zeroizing::new(raw[end..].to_vec()),
    })
}

fn send_reply(
    io: &mut Channel<'_>,
    reply: Reply,
    deadline: &Deadline,
    stop: &AtomicBool,
) -> Result<()> {
    ensure!(
        reply.body.len() <= MAX_RESPONSE,
        "MCP response capacity exceeded"
    );
    let reason = match reply.status {
        200 => "OK",
        202 => "Accepted",
        204 => "No Content",
        400 => "Bad Request",
        401 => "Unauthorized",
        403 => "Forbidden",
        404 => "Not Found",
        405 => "Method Not Allowed",
        406 => "Not Acceptable",
        408 => "Request Timeout",
        409 => "Conflict",
        413 => "Content Too Large",
        415 => "Unsupported Media Type",
        429 => "Too Many Requests",
        431 => "Request Header Fields Too Large",
        _ => "Service Unavailable",
    };
    let mut header = format!(
        "HTTP/1.1 {} {reason}\r\nContent-Length: {}\r\nConnection: close\r\nCache-Control: no-store\r\nX-Content-Type-Options: nosniff\r\n",
        reply.status,
        reply.body.len()
    );
    if !reply.body.is_empty() {
        header.push_str("Content-Type: application/json\r\n");
    }
    if reply.status == 405 {
        header.push_str("Allow: POST, DELETE, OPTIONS\r\n");
    }
    if reply.status == 401 {
        header.push_str("WWW-Authenticate: Bearer\r\n");
    }
    if let Some(id) = reply.session {
        header.push_str(&format!("Mcp-Session-Id: {id}\r\n"));
    }
    if let Some(origin) = reply.origin {
        header.push_str(&format!("Access-Control-Allow-Origin: {origin}\r\nVary: Origin\r\nAccess-Control-Expose-Headers: Mcp-Session-Id\r\n"));
        if reply.status == 204 {
            header.push_str("Access-Control-Allow-Methods: POST, GET, DELETE\r\nAccess-Control-Allow-Headers: Authorization, Content-Type, Mcp-Session-Id, Mcp-Protocol-Version\r\n");
        }
    }
    header.push_str("\r\n");
    physical_write(io, header.as_bytes(), deadline, stop)?;
    physical_write(io, &reply.body, deadline, stop)
}

struct Job {
    id: String,
    meta: Arc<Meta>,
    token_hash: [u8; 32],
    body: Zeroizing<Vec<u8>>,
    initialize: bool,
    deadline: Deadline,
    cancel: Arc<AtomicBool>,
    reply: mpsc::SyncSender<Reply>,
}
struct LocalSession {
    session: Session,
    state: ConnectionState,
    meta: Arc<Meta>,
    runtime: Option<crate::agent_repl::AgentRepl>,
}
fn close_local(sessions: &mut BTreeMap<String, LocalSession>, id: &str, shared: &Shared) {
    if let Some(mut session) = sessions.remove(id) {
        let session_closed = session.session.close_owned_processes();
        let runtime_closed = session
            .runtime
            .take()
            .map(|mut runtime| runtime.close())
            .transpose();
        if session_closed.is_err() || runtime_closed.is_err() || session.session.cleanup_unproven()
        {
            shared.cleanup_failed.store(true, Ordering::SeqCst);
        }
    }
}
fn actor(receiver: mpsc::Receiver<Job>, shared: Arc<Shared>, config: Arc<Config>) {
    // Session owns non-Send process resources; it is constructed and destroyed
    // exclusively on this actor, never smuggled across a thread boundary.
    let mut sessions = BTreeMap::<String, LocalSession>::new();
    while !shared.stop.load(Ordering::SeqCst) {
        let _ = shared.prune(&config);
        let closed: Vec<_> = sessions
            .iter()
            .filter(|(_, session)| session.meta.closed.load(Ordering::SeqCst))
            .map(|(id, _)| id.clone())
            .collect();
        for id in closed {
            close_local(&mut sessions, &id, &shared);
        }
        let job = match receiver.recv_timeout(Duration::from_millis(25)) {
            Ok(job) => job,
            Err(mpsc::RecvTimeoutError::Timeout) => continue,
            Err(_) => break,
        };
        let result = (|| -> Result<Reply> {
            let policy = shared
                .policy
                .read()
                .map_err(|_| anyhow::anyhow!("MCP host policy unavailable"))?;
            let principal = policy
                .authenticate(&job.token_hash)
                .context("MCP principal revoked")?;
            ensure!(
                principal.id == job.meta.principal
                    && principal.generation == job.meta.generation
                    && !job.meta.closed.load(Ordering::SeqCst),
                "MCP session authority changed"
            );
            Deadline::run_bounded(&job.deadline, Arc::clone(&shared.stop), || {
                Deadline::run_bounded(&job.meta.lifetime, Arc::clone(&job.meta.closed), || {
                    Deadline::run_bounded(&job.deadline, Arc::clone(&job.cancel), || {
                        // Materialize the installed ceiling/flags before passing
                        // an explicit deadline into initialization helpers.
                        let operation_deadline = Deadline::new(600_000);
                        if job.initialize {
                            ensure!(!sessions.contains_key(&job.id), "Duplicate MCP session");
                            sessions.insert(
                                job.id.clone(),
                                LocalSession {
                                    session: principal.target.open(&operation_deadline)?,
                                    state: ConnectionState::default(),
                                    meta: Arc::clone(&job.meta),
                                    runtime: principal
                                        .target
                                        .agent_repl
                                        .as_ref()
                                        .map(|spec| {
                                            crate::agent_repl::AgentRepl::spawn(
                                                spec.clone(),
                                                &operation_deadline,
                                            )
                                        })
                                        .transpose()?,
                                },
                            );
                        }
                        let session = sessions
                            .get_mut(&job.id)
                            .context("MCP session unavailable")?;
                        let value = match &mut session.runtime {
                            Some(runtime) => session.state.message_serena_with_repl(
                                &mut session.session,
                                runtime,
                                &job.body,
                            )?,
                            None if principal.target.profile == Profile::Session => {
                                session.state.message_session_with_collection_grants(
                                    &mut session.session,
                                    &principal.target.collection_grants,
                                    &job.body,
                                )?
                            }
                            None => session.state.message_session(
                                &mut session.session,
                                principal.target.profile,
                                &job.body,
                            )?,
                        };
                        if session.session.cleanup_unproven() {
                            shared.cleanup_failed.store(true, Ordering::SeqCst);
                            return Err(crate::process::CleanupUnproven::new(
                                "Host debugger cleanup failed",
                            )
                            .into());
                        }
                        ensure!(
                            session
                                .runtime
                                .as_ref()
                                .is_none_or(|runtime| runtime.is_alive()),
                            "Agent REPL state was lost; open a new session"
                        );
                        if job.initialize {
                            let response = value.as_ref().context("Initialize response missing")?;
                            ensure!(response.get("error").is_none(), "MCP initialization failed");
                            let version = response["result"]["protocolVersion"]
                                .as_str()
                                .filter(|version| VERSIONS.contains(version))
                                .context("Unsupported negotiated transport version")?;
                            *job.meta
                                .version
                                .lock()
                                .map_err(|_| anyhow::anyhow!("MCP version state unavailable"))? =
                                Some(version.into());
                        }
                        let mut reply = match value {
                            Some(value) => {
                                let body = serde_json::to_vec(&value)?;
                                ensure!(
                                    body.len() <= MAX_RESPONSE,
                                    "MCP response exceeds transport capacity"
                                );
                                Reply {
                                    status: 200,
                                    body,
                                    session: None,
                                    origin: None,
                                }
                            }
                            None => Reply::empty(202),
                        };
                        if job.initialize {
                            reply.session = Some(job.id.clone());
                        }
                        Ok(reply)
                    })
                })
            })
        })();
        let response = match result {
            Ok(reply) => reply,
            Err(error) => {
                if crate::process::cleanup_unproven(&error) {
                    shared.cleanup_failed.store(true, Ordering::SeqCst);
                }
                if job.initialize
                    || sessions
                        .get(&job.id)
                        .is_some_and(|session| session.runtime.is_some())
                {
                    shared.remove(&job.id);
                    close_local(&mut sessions, &job.id, &shared);
                }
                Reply::fault(Fault(503, "MCP request cancelled, expired or unavailable"))
            }
        };
        let _ = job.reply.send(response);
        let _ = shared.prune(&config);
    }
    let ids: Vec<_> = sessions.keys().cloned().collect();
    for id in ids {
        close_local(&mut sessions, &id, &shared);
    }
}

fn lookup(
    shared: &Shared,
    id: &str,
    principal: &Principal,
    version: Option<&str>,
) -> HttpResult<Arc<Meta>> {
    if !crate::model::valid_id(id) {
        return Err(Fault(404, "MCP session unavailable"));
    }
    let meta = shared
        .sessions
        .lock()
        .map_err(|_| Fault(503, "MCP session state unavailable"))?
        .get(id)
        .cloned()
        .ok_or(Fault(404, "MCP session unavailable"))?;
    if meta.principal != principal.id
        || meta.generation != principal.generation
        || meta.closed.load(Ordering::SeqCst)
    {
        return Err(Fault(404, "MCP session unavailable"));
    }
    let negotiated = meta
        .version
        .lock()
        .map_err(|_| Fault(503, "MCP protocol state unavailable"))?;
    if negotiated.is_none() {
        return Err(Fault(404, "MCP session unavailable"));
    }
    if version.is_some_and(|version| negotiated.as_deref() != Some(version)) {
        return Err(Fault(400, "MCP version differs from negotiated session"));
    }
    drop(negotiated);
    Ok(meta)
}
struct Ticket {
    meta: Arc<Meta>,
    id: Option<String>,
}
impl Drop for Ticket {
    fn drop(&mut self) {
        if let Some(id) = &self.id
            && let Ok(mut requests) = self.meta.requests.lock()
        {
            requests.remove(id);
        }
    }
}
fn route(
    request: Incoming,
    shared: &Shared,
    config: &Config,
    senders: &[mpsc::SyncSender<Job>],
    next: &AtomicU64,
    deadline: &Deadline,
) -> HttpResult<Reply> {
    shared
        .prune(config)
        .map_err(|_| Fault(503, "MCP session state unavailable"))?;
    if request.method == "OPTIONS" {
        if !request.headers.contains_key("origin")
            || !request
                .headers
                .get("access-control-request-method")
                .is_some_and(|value| ["POST", "GET", "DELETE"].contains(&value.as_str()))
        {
            return Err(Fault(405, "Unsupported MCP preflight"));
        }
        if request
            .headers
            .get("access-control-request-headers")
            .is_some_and(|value| {
                value.split(',').any(|name| {
                    ![
                        "authorization",
                        "content-type",
                        "mcp-session-id",
                        "mcp-protocol-version",
                        "accept",
                    ]
                    .contains(&name.trim().to_ascii_lowercase().as_str())
                })
            })
        {
            return Err(Fault(403, "Preflight header not authorized"));
        }
        return Ok(Reply::empty(204));
    }
    let token_hash = request.token_hash.ok_or(Fault(401, "Unauthorized"))?;
    let principal = shared
        .policy
        .read()
        .map_err(|_| Fault(503, "MCP policy unavailable"))?
        .authenticate(&token_hash)
        .cloned()
        .ok_or(Fault(401, "Unauthorized"))?;
    let version = request
        .headers
        .get("mcp-protocol-version")
        .map(|value| value.as_str());
    let session_id = request
        .headers
        .get("mcp-session-id")
        .map(|value| value.as_str());
    if request.method == "GET" {
        if !request
            .headers
            .get("accept")
            .is_some_and(|value| accepts(value, "text/event-stream"))
        {
            return Err(Fault(406, "Event stream Accept required"));
        }
        if let Some(id) = session_id {
            lookup(shared, id, &principal, version)?;
        }
        return Ok(Reply::empty(405));
    }
    if request.method == "DELETE" {
        let id = session_id.ok_or(Fault(400, "MCP session ID required"))?;
        lookup(shared, id, &principal, version)?;
        shared.remove(id);
        return Ok(Reply::empty(204));
    }
    if request.method != "POST" {
        return Err(Fault(405, "Unsupported MCP method"));
    }
    let value = crate::json::parse(&request.body, config.max_body_bytes)
        .map_err(|_| Fault(400, "Invalid bounded JSON-RPC object"))?;
    if !value.is_object()
        || value["jsonrpc"] != "2.0"
        || !value["method"].is_string()
        || value
            .get("id")
            .is_some_and(|id| !id.is_string() && !id.is_i64() && !id.is_u64())
    {
        return Err(Fault(400, "One JSON-RPC request or notification required"));
    }
    let method = value["method"]
        .as_str()
        .ok_or(Fault(400, "MCP method required"))?;
    let initialize = method == "initialize";
    if initialize
        && (session_id.is_some()
            || value.get("id").is_none()
            || value["params"]["protocolVersion"] == "2024-11-05")
    {
        return Err(Fault(400, "Invalid Streamable HTTP initialization"));
    }
    let (id, meta) = if initialize {
        let id = random_id().map_err(|_| Fault(503, "MCP secure randomness unavailable"))?;
        let mut sessions = shared
            .sessions
            .lock()
            .map_err(|_| Fault(503, "MCP session state unavailable"))?;
        if sessions.len() >= config.max_sessions
            || sessions
                .values()
                .filter(|meta| meta.principal == principal.id)
                .count()
                >= config.max_sessions_per_principal
        {
            return Err(Fault(429, "MCP session capacity exceeded"));
        }
        let worker = next.fetch_add(1, Ordering::Relaxed) as usize % senders.len();
        let meta = Arc::new(Meta {
            principal: principal.id.clone(),
            generation: principal.generation,
            worker,
            born: Instant::now(),
            lifetime: Deadline::new(config.session_ttl_ms),
            closed: Arc::new(AtomicBool::new(false)),
            version: Mutex::new(None),
            requests: Mutex::new(BTreeMap::new()),
            count: AtomicU64::new(0),
        });
        sessions.insert(id.clone(), Arc::clone(&meta));
        (id, meta)
    } else {
        let id = session_id.ok_or(Fault(400, "MCP session ID required"))?;
        (id.to_owned(), lookup(shared, id, &principal, version)?)
    };
    if meta.count.fetch_add(1, Ordering::SeqCst) >= config.max_requests_per_session {
        shared.remove(&id);
        return Err(Fault(429, "MCP session request capacity exceeded"));
    }
    if method == "notifications/cancelled" && value.get("id").is_none() {
        let request_id = value["params"]
            .get("requestId")
            .filter(|id| id.is_string() || id.is_i64() || id.is_u64())
            .ok_or(Fault(400, "Cancellation request ID required"))?;
        let key = crate::model::identity(request_id);
        if let Some(cancel) = meta
            .requests
            .lock()
            .map_err(|_| Fault(503, "MCP request state unavailable"))?
            .get(&key)
        {
            cancel.store(true, Ordering::SeqCst);
        }
        return Ok(Reply::empty(202));
    }
    let cancel = Arc::new(AtomicBool::new(false));
    let request_key = value.get("id").map(crate::model::identity);
    if let Some(key) = &request_key {
        let mut pending = meta
            .requests
            .lock()
            .map_err(|_| Fault(503, "MCP request state unavailable"))?;
        if pending.contains_key(key) {
            return Err(Fault(409, "MCP request ID already pending"));
        }
        pending.insert(key.clone(), Arc::clone(&cancel));
    }
    let _ticket = Ticket {
        meta: Arc::clone(&meta),
        id: request_key,
    };
    let (reply_tx, reply_rx) = mpsc::sync_channel(1);
    let job = Job {
        id: id.clone(),
        meta: Arc::clone(&meta),
        token_hash,
        body: request.body,
        initialize,
        deadline: deadline.clone(),
        cancel: Arc::clone(&cancel),
        reply: reply_tx,
    };
    if senders[meta.worker].try_send(job).is_err() {
        if initialize {
            shared.remove(&id);
        }
        return Err(Fault(503, "MCP worker capacity exceeded"));
    }
    loop {
        if deadline.check().is_err()
            || meta.lifetime.check().is_err()
            || meta.closed.load(Ordering::SeqCst)
            || shared.stop.load(Ordering::SeqCst)
        {
            cancel.store(true, Ordering::SeqCst);
            if initialize {
                shared.remove(&id);
            }
            return Err(Fault(503, "MCP request deadline or shutdown"));
        }
        match reply_rx.recv_timeout(deadline.remaining().min(Duration::from_millis(25))) {
            Ok(reply) => return Ok(reply),
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(_) => {
                if initialize {
                    shared.remove(&id);
                }
                return Err(Fault(503, "MCP worker unavailable"));
            }
        }
    }
}

fn tls_config(material: TlsMaterial) -> Result<Arc<rustls::ServerConfig>> {
    ensure!(
        !material.certificates.is_empty()
            && material.certificates.len() <= 8
            && material.certificates.iter().map(Vec::len).sum::<usize>() <= 128_000
            && material.private_key.len() <= 64_000,
        "MCP TLS identity capacity exceeded"
    );
    let certificates = material
        .certificates
        .into_iter()
        .map(rustls::pki_types::CertificateDer::from)
        .collect();
    let key = rustls::pki_types::PrivateKeyDer::try_from(material.private_key.to_vec())
        .map_err(|_| anyhow::anyhow!("Invalid MCP host TLS private key"))?;
    let mut tls = rustls::ServerConfig::builder_with_provider(Arc::new(
        rustls::crypto::ring::default_provider(),
    ))
    .with_protocol_versions(&[&rustls::version::TLS13])?
    .with_no_client_auth()
    .with_single_cert(certificates, key)?;
    tls.alpn_protocols = vec![b"http/1.1".to_vec()];
    Ok(Arc::new(tls))
}
fn validate_config(config: &Config, principals: &[Principal]) -> Result<()> {
    ensure!(
        config.bind.ip().is_loopback() || (config.allow_remote_bind && config.tls.is_some()),
        "Remote MCP binding requires explicit host permission and TLS"
    );
    ensure!(
        (250..=30_000).contains(&config.request_timeout_ms)
            && (1..=4_000_000).contains(&config.max_body_bytes)
            && (1..=64).contains(&config.max_connections)
            && (1..=8).contains(&config.workers),
        "Invalid MCP transport budgets"
    );
    ensure!(
        (1..=256).contains(&config.max_sessions)
            && (1..=32).contains(&config.max_sessions_per_principal)
            && config.max_sessions_per_principal <= config.max_sessions
            && (100..=86_400_000).contains(&config.session_ttl_ms)
            && (1..=100_000).contains(&config.max_requests_per_session),
        "Invalid MCP session budgets"
    );
    ensure!(
        !principals.is_empty()
            && principals.len() <= 256
            && config.server_names.len() <= 8
            && config.allowed_origins.len() <= 32,
        "Invalid MCP host authority capacity"
    );
    ensure!(
        config.endpoint.starts_with('/')
            && !config.endpoint.starts_with("//")
            && config.endpoint.len() <= 256
            && !config.endpoint.contains("..")
            && config
                .endpoint
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || b"/_.-".contains(&byte)),
        "Invalid explicit MCP endpoint"
    );
    let mut names = BTreeSet::new();
    for name in &config.server_names {
        ensure!(
            !name.is_empty()
                && name.len() <= 253
                && !name.contains('*')
                && names.insert(name.to_ascii_lowercase()),
            "Invalid or duplicate MCP host name"
        );
        rustls::pki_types::ServerName::try_from(name.clone())
            .map_err(|_| anyhow::anyhow!("Invalid MCP host name"))?;
    }
    if config.tls.is_some() {
        ensure!(
            !config.server_names.is_empty(),
            "TLS requires explicit MCP server names"
        );
    }
    let mut origins = BTreeSet::new();
    for origin in &config.allowed_origins {
        ensure!(
            origin.len() <= 512 && origins.insert(origin),
            "Invalid or duplicate MCP origin"
        );
        let url = url::Url::parse(origin)?;
        ensure!(
            matches!(url.scheme(), "http" | "https")
                && url.host_str().is_some()
                && url.username().is_empty()
                && url.password().is_none()
                && url.query().is_none()
                && url.fragment().is_none()
                && url.path() == "/"
                && url.origin().ascii_serialization() == *origin,
            "Supply an exact HTTP origin, without credentials, path or query"
        );
    }
    Ok(())
}

/// No listener exists until this explicit trusted-host call. Tokens/host roots,
/// profiles and execution choices cannot be changed through MCP arguments.
pub fn serve(mut config: Config, principals: Vec<Principal>) -> Result<ServerHandle> {
    validate_config(&config, &principals)?;
    let mut policy = Policy {
        principals: BTreeMap::new(),
    };
    for principal in principals {
        ensure!(
            !policy.principals.contains_key(&principal.id),
            "Duplicate MCP principal ID"
        );
        policy.insert(principal)?;
    }
    let tls = config.tls.take().map(tls_config).transpose()?;
    let listener = TcpListener::bind(config.bind)?;
    let address = listener.local_addr()?;
    listener.set_nonblocking(true)?;
    let config = Arc::new(config);
    let shared = Arc::new(Shared {
        policy: RwLock::new(policy),
        sessions: Mutex::new(BTreeMap::new()),
        stop: Arc::new(AtomicBool::new(false)),
        active: Mutex::new(BTreeMap::new()),
        cleanup_failed: AtomicBool::new(false),
    });
    let mut senders = Vec::new();
    let mut actors = Vec::new();
    for index in 0..config.workers {
        let (tx, rx) = mpsc::sync_channel(config.max_connections);
        let shared = Arc::clone(&shared);
        let config = Arc::clone(&config);
        actors.push(
            thread::Builder::new()
                .name(format!("yoke-mcp-backend-{index}"))
                .spawn(move || actor(rx, shared, config))?,
        );
        senders.push(tx);
    }
    let worker_shared = Arc::clone(&shared);
    let (done, completed) = mpsc::sync_channel(1);
    let worker = thread::Builder::new()
        .name("yoke-mcp-http".into())
        .spawn(move || {
            let next = Arc::new(AtomicU64::new(0));
            let mut network = Vec::<thread::JoinHandle<()>>::new();
            let mut sequence = 0u64;
            let mut clean = true;
            while !worker_shared.stop.load(Ordering::SeqCst) {
                for index in (0..network.len()).rev() {
                    if network[index].is_finished() {
                        clean &= network.swap_remove(index).join().is_ok();
                    }
                }
                match listener.accept() {
                    Ok((tcp, _)) => {
                        if worker_shared.stop.load(Ordering::SeqCst) {
                            break;
                        }
                        if network.len() >= config.max_connections {
                            let _ = tcp.shutdown(Shutdown::Both);
                            continue;
                        }
                        // Accepted-socket mode inheritance differs by platform.
                        // BoundSocket uses short blocking timeouts, never a busy spin.
                        if tcp.set_nonblocking(false).is_err() {
                            clean = false;
                            continue;
                        }
                        sequence = sequence.wrapping_add(1);
                        let key = sequence;
                        if let Ok(clone) = tcp.try_clone()
                            && let Ok(mut sockets) = worker_shared.active.lock()
                        {
                            sockets.insert(key, clone);
                        }
                        let shared = Arc::clone(&worker_shared);
                        let config = Arc::clone(&config);
                        let tls = tls.clone();
                        let senders = senders.clone();
                        let next = Arc::clone(&next);
                        let handler = thread::Builder::new()
                            .name("yoke-mcp-http-connection".into())
                            .spawn(move || {
                                let deadline = Deadline::new(config.request_timeout_ms);
                                // Reserve part of the same host budget to report expiry.
                                let work_deadline = Deadline::new(config.request_timeout_ms - 100);
                                let socket = BoundSocket {
                                    socket: tcp,
                                    deadline: &deadline,
                                    stop: &shared.stop,
                                };
                                let channel = match tls {
                                    Some(tls) => rustls::ServerConnection::new(tls).map(|conn| {
                                        Channel::Tls(Box::new(rustls::StreamOwned::new(
                                            conn, socket,
                                        )))
                                    }),
                                    None => Ok(Channel::Plain(socket)),
                                };
                                if let Ok(mut io) = channel {
                                    let mut origin = None;
                                    let (mut reply, rejected) = match incoming(
                                        &mut io,
                                        &config,
                                        address,
                                        &shared,
                                        &deadline,
                                        &mut origin,
                                    ) {
                                        Ok(request) => (
                                            route(
                                                request,
                                                &shared,
                                                &config,
                                                &senders,
                                                &next,
                                                &work_deadline,
                                            )
                                            .unwrap_or_else(Reply::fault),
                                            false,
                                        ),
                                        Err(fault) => (Reply::fault(fault), true),
                                    };
                                    reply.origin = origin;
                                    let replied =
                                        send_reply(&mut io, reply, &deadline, &shared.stop);
                                    if let Channel::Tls(stream) = &mut io {
                                        stream.conn.send_close_notify();
                                        // Use the original physical budget; never extend a
                                        // timed-out request merely to finish TLS shutdown.
                                        let _ =
                                            physical_write(&mut io, &[], &deadline, &shared.stop);
                                    }
                                    if rejected && replied.is_ok() {
                                        match &mut io {
                                            Channel::Plain(socket) => socket.finish_rejection(),
                                            Channel::Tls(stream) => stream.sock.finish_rejection(),
                                        }
                                    }
                                }
                                if let Ok(mut sockets) = shared.active.lock() {
                                    sockets.remove(&key);
                                }
                            });
                        match handler {
                            Ok(handler) => network.push(handler),
                            Err(_) => {
                                clean = false;
                                if let Ok(mut sockets) = worker_shared.active.lock() {
                                    sockets.remove(&key);
                                }
                            }
                        }
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(5))
                    }
                    Err(_) => {
                        clean = false;
                        break;
                    }
                }
            }
            drop(listener);
            worker_shared.stop.store(true, Ordering::SeqCst);
            if let Ok(sockets) = worker_shared.active.lock() {
                for socket in sockets.values() {
                    let _ = socket.shutdown(Shutdown::Both);
                }
            }
            for handler in network {
                clean &= handler.join().is_ok();
            }
            drop(senders);
            for actor in actors {
                clean &= actor.join().is_ok();
            }
            let _ = done.send(clean && !worker_shared.cleanup_failed.load(Ordering::SeqCst));
        })?;
    Ok(ServerHandle {
        address,
        shared,
        worker: Some(worker),
        completed,
        finish_attempted: false,
    })
}
