//! Explicit host-selected source federation transport; secrets remain in RAM.
use crate::{
    Engine,
    capture::Deadline,
    federation::{self, ExportScope, ImportGrant, Scope},
    model::valid_id,
};
use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    io::{Read, Write},
    net::{Shutdown, SocketAddr, TcpListener, TcpStream},
    path::Path,
    sync::{
        Arc, Mutex, RwLock,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread,
    time::Duration,
};
use subtle::ConstantTimeEq;
use zeroize::Zeroizing;

pub struct TlsMaterial {
    pub certificates: Vec<Vec<u8>>,
    pub private_key: Zeroizing<Vec<u8>>,
}
pub struct ImportAccess {
    pub id: String,
    pub grant: ImportGrant,
}
pub struct ExportAccess {
    pub id: String,
    pub configuration: ExportScope,
    pub private_key: Zeroizing<[u8; 32]>,
}
pub struct Config {
    pub bind: SocketAddr,
    pub tls: Option<TlsMaterial>,
    pub allow_remote_bind: bool,
    pub server_names: Vec<String>,
    pub request_timeout_ms: u64,
    pub max_body_bytes: usize,
    pub max_response_bytes: usize,
    pub imports: Vec<ImportAccess>,
    pub exports: Vec<ExportAccess>,
}
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    Read,
    Import,
    Admin,
}
#[derive(Clone)]
pub struct Principal {
    id: String,
    token_hash: [u8; 32],
    roles: BTreeSet<Role>,
    imports: BTreeSet<String>,
    exports: BTreeSet<String>,
}
impl Principal {
    pub fn new(
        id: &str,
        token: &[u8; 32],
        roles: &[Role],
        import_grant_ids: &[String],
        export_grant_ids: &[String],
    ) -> Result<Self> {
        identifier(id)?;
        ensure!(
            !roles.is_empty()
                && roles.len() <= 3
                && roles.iter().collect::<BTreeSet<_>>().len() == roles.len(),
            "Invalid host principal roles"
        );
        ensure!(
            import_grant_ids.len() <= 256 && export_grant_ids.len() <= 256,
            "Host principal grant capacity exceeded"
        );
        for id in import_grant_ids.iter().chain(export_grant_ids) {
            identifier(id)?;
        }
        Ok(Self {
            id: id.into(),
            token_hash: Sha256::digest(token).into(),
            roles: roles.iter().copied().collect(),
            imports: import_grant_ids.iter().cloned().collect(),
            exports: export_grant_ids.iter().cloned().collect(),
        })
    }
}
struct Access {
    imports: BTreeMap<String, ImportGrant>,
    exports: BTreeMap<String, ExportAccess>,
}
struct Policy {
    principals: BTreeMap<String, Principal>,
    import_ids: BTreeSet<String>,
    export_ids: BTreeSet<String>,
}
impl Policy {
    fn validate(&self, p: &Principal) -> Result<()> {
        ensure!(
            p.imports.is_subset(&self.import_ids) && p.exports.is_subset(&self.export_ids),
            "Principal references a grant outside fixed host authority"
        );
        Ok(())
    }
    fn authenticate(&self, hash: &[u8; 32]) -> Option<&Principal> {
        self.principals
            .values()
            .find(|p| bool::from(p.token_hash.ct_eq(hash)))
    }
}
pub struct ServerHandle {
    address: SocketAddr,
    policy: Arc<RwLock<Policy>>,
    stop: Arc<AtomicBool>,
    active: Arc<Mutex<Option<TcpStream>>>,
    worker: Option<thread::JoinHandle<()>>,
    completed: mpsc::Receiver<()>,
    finish_attempted: bool,
}
impl ServerHandle {
    pub fn local_addr(&self) -> SocketAddr {
        self.address
    }
    pub fn revoke(&self, id: &str) -> Result<bool> {
        identifier(id)?;
        Ok(self
            .policy
            .write()
            .map_err(|_| anyhow::anyhow!("Host policy lock unavailable"))?
            .principals
            .remove(id)
            .is_some())
    }
    pub fn upsert(&self, principal: Principal) -> Result<()> {
        let mut policy = self
            .policy
            .write()
            .map_err(|_| anyhow::anyhow!("Host policy lock unavailable"))?;
        policy.validate(&principal)?;
        ensure!(
            policy.principals.len() < 256 || policy.principals.contains_key(&principal.id),
            "Host principal capacity exceeded"
        );
        ensure!(
            !policy
                .principals
                .values()
                .any(|p| p.id != principal.id
                    && bool::from(p.token_hash.ct_eq(&principal.token_hash))),
            "Duplicate host principal credential"
        );
        policy.principals.insert(principal.id.clone(), principal);
        Ok(())
    }
    pub fn shutdown(mut self) -> Result<()> {
        self.finish()
    }
    fn finish(&mut self) -> Result<()> {
        if self.worker.is_none() {
            return Ok(());
        }
        self.finish_attempted = true;
        self.stop.store(true, Ordering::SeqCst);
        if let Ok(active) = self.active.lock()
            && let Some(tcp) = active.as_ref()
        {
            let _ = tcp.shutdown(Shutdown::Both);
        }
        let mut wake = self.address;
        if wake.ip().is_unspecified() {
            wake.set_ip(if wake.is_ipv4() {
                "127.0.0.1".parse()?
            } else {
                "::1".parse()?
            });
        }
        let _ = TcpStream::connect_timeout(&wake, Duration::from_millis(50));
        ensure!(
            self.completed.recv_timeout(Duration::from_secs(5)).is_ok(),
            "Federation worker shutdown requires recovery; owned stop flag remains set"
        );
        if let Some(worker) = self.worker.take() {
            worker
                .join()
                .map_err(|_| anyhow::anyhow!("Federation worker failed"))?;
        }
        Ok(())
    }
}
impl Drop for ServerHandle {
    fn drop(&mut self) {
        if self.worker.is_some() && !self.finish_attempted && self.finish().is_err() {
            // Drop cannot return an error. Never silently claim that a worker
            // which exceeded the bounded join has been cleaned up.
            eprintln!("Federation worker cleanup could not be proven; host recovery required");
        }
    }
}
fn name_valid(name: &str) -> Result<()> {
    ensure!(
        !name.is_empty() && name.len() <= 253 && !name.contains('*'),
        "Invalid explicit TLS server name"
    );
    rustls::pki_types::ServerName::try_from(name.to_owned())
        .map_err(|_| anyhow::anyhow!("Invalid explicit TLS server name"))?;
    Ok(())
}
fn tls_server(material: TlsMaterial) -> Result<Arc<rustls::ServerConfig>> {
    ensure!(
        !material.certificates.is_empty()
            && material.certificates.len() <= 8
            && material
                .certificates
                .iter()
                .all(|c| !c.is_empty() && c.len() <= 32_000)
            && material.certificates.iter().map(Vec::len).sum::<usize>() <= 128_000
            && material.private_key.len() <= 64_000,
        "TLS material capacity exceeded"
    );
    let certificates = material
        .certificates
        .into_iter()
        .map(rustls::pki_types::CertificateDer::from)
        .collect();
    let key = rustls::pki_types::PrivateKeyDer::try_from(material.private_key.to_vec())
        .map_err(|_| anyhow::anyhow!("Invalid host TLS private key encoding"))?;
    let mut config = rustls::ServerConfig::builder_with_provider(Arc::new(
        rustls::crypto::ring::default_provider(),
    ))
    .with_protocol_versions(&[&rustls::version::TLS13])?
    .with_no_client_auth()
    .with_single_cert(certificates, key)?;
    config.alpn_protocols = vec![b"http/1.1".to_vec()];
    Ok(Arc::new(config))
}
pub fn serve(root: &Path, mut config: Config, principals: Vec<Principal>) -> Result<ServerHandle> {
    ensure!(
        config.bind.ip().is_loopback() || (config.allow_remote_bind && config.tls.is_some()),
        "Nonloopback federation requires explicit host remote bind permission and TLS"
    );
    ensure!(
        (100..=30_000).contains(&config.request_timeout_ms)
            && (1..=32_000_000).contains(&config.max_body_bytes)
            && (1..=32_000_000).contains(&config.max_response_bytes),
        "Invalid federation transport budgets"
    );
    ensure!(
        config.server_names.len() <= 8,
        "TLS server name capacity exceeded"
    );
    for name in &config.server_names {
        name_valid(name)?;
    }
    ensure!(
        !principals.is_empty()
            && principals.len() <= 256
            && config.imports.len() <= 256
            && config.exports.len() <= 256,
        "Host authority capacity exceeded"
    );
    let mut access = Access {
        imports: BTreeMap::new(),
        exports: BTreeMap::new(),
    };
    for entry in config.imports.drain(..) {
        identifier(&entry.id)?;
        ensure!(
            access.imports.insert(entry.id, entry.grant).is_none(),
            "Duplicate fixed import grant"
        );
    }
    for entry in config.exports.drain(..) {
        identifier(&entry.id)?;
        ensure!(
            access.exports.insert(entry.id.clone(), entry).is_none(),
            "Duplicate fixed export grant"
        );
    }
    let mut policy = Policy {
        principals: BTreeMap::new(),
        import_ids: access.imports.keys().cloned().collect(),
        export_ids: access.exports.keys().cloned().collect(),
    };
    for principal in principals {
        policy.validate(&principal)?;
        ensure!(
            !policy
                .principals
                .values()
                .any(|p| bool::from(p.token_hash.ct_eq(&principal.token_hash))),
            "Duplicate host principal credential"
        );
        ensure!(
            policy
                .principals
                .insert(principal.id.clone(), principal)
                .is_none(),
            "Duplicate host principal ID"
        );
    }
    let tls = config.tls.take().map(tls_server).transpose()?;
    let root = root.canonicalize()?;
    ensure!(root.is_dir(), "Host source root is not a directory");
    // Open state before binding so invalid/unavailable source state never creates
    // a listener. The engine moves into its one bounded worker thread.
    let engine = Engine::open(&root)?;
    let listener = TcpListener::bind(config.bind)?;
    let address = listener.local_addr()?;
    listener.set_nonblocking(true)?;
    let policy = Arc::new(RwLock::new(policy));
    let stop = Arc::new(AtomicBool::new(false));
    let active = Arc::new(Mutex::new(None));
    let worker_policy = Arc::clone(&policy);
    let worker_stop = Arc::clone(&stop);
    let worker_active = Arc::clone(&active);
    let (done, completed) = mpsc::sync_channel(1);
    let worker = thread::Builder::new()
        .name("yoke-federation".into())
        .spawn(move || {
            while !worker_stop.load(Ordering::SeqCst) {
                match listener.accept() {
                    Ok((tcp, _)) => {
                        if worker_stop.load(Ordering::SeqCst) {
                            break;
                        }
                        if let Ok(clone) = tcp.try_clone()
                            && let Ok(mut current) = worker_active.lock()
                        {
                            *current = Some(clone);
                        }
                        let _ = connection(
                            &engine,
                            tcp,
                            Transport {
                                tls: tls.as_ref(),
                                address,
                            },
                            &config,
                            &access,
                            &worker_policy,
                            &worker_stop,
                        );
                        if let Ok(mut current) = worker_active.lock() {
                            *current = None;
                        }
                        engine.store.clear_deadline();
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(5))
                    }
                    Err(_) => break,
                }
            }
            drop(listener);
            drop(engine);
            let _ = done.send(());
        })?;
    Ok(ServerHandle {
        address,
        policy,
        stop,
        active,
        worker: Some(worker),
        completed,
        finish_attempted: false,
    })
}
fn identifier(id: &str) -> Result<()> {
    ensure!(
        !id.is_empty()
            && id.len() <= 128
            && id.as_bytes()[0].is_ascii_alphanumeric()
            && id
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || b"_.-".contains(&byte)),
        "Invalid host grant/principal identifier"
    );
    Ok(())
}
pub struct ClientConfig {
    pub address: SocketAddr,
    pub server_name: String,
    pub ca_certificates: Vec<Vec<u8>>,
    pub token: Zeroizing<Vec<u8>>,
    pub allow_plaintext_loopback: bool,
    pub max_body_bytes: usize,
    pub max_response_bytes: usize,
}
pub struct ClientResponse {
    pub status: u16,
    pub body: serde_json::Value,
}
/// Make one request to the literal host-selected endpoint; redirects and
/// credential-bearing command arguments are never involved.
pub fn request(
    config: &ClientConfig,
    method: &str,
    path: &str,
    body: Option<Value>,
    deadline: &Deadline,
) -> Result<ClientResponse> {
    deadline.check()?;
    ensure!(
        ["GET", "POST"].contains(&method)
            && path.starts_with('/')
            && !path.starts_with("//")
            && path.len() <= 4096
            && path
                .bytes()
                .all(|b| b.is_ascii() && !b.is_ascii_control() && !b.is_ascii_whitespace())
            && !path.contains('#'),
        "Invalid explicit HTTP request target"
    );
    ensure!(
        config.token.len() == 32
            && (1..=32_000_000).contains(&config.max_body_bytes)
            && (1..=32_000_000).contains(&config.max_response_bytes),
        "Invalid client credential or budgets"
    );
    ensure!(
        (method == "GET" && body.is_none()) || (method == "POST" && body.is_some()),
        "Invalid explicit HTTP request body"
    );
    let body = Zeroizing::new(
        body.map(|body| serde_json::to_vec(&body))
            .transpose()?
            .unwrap_or_default(),
    );
    ensure!(
        body.len() <= config.max_body_bytes,
        "Client body budget exceeded"
    );
    let tls = if config.ca_certificates.is_empty() {
        ensure!(
            config.allow_plaintext_loopback && config.address.ip().is_loopback(),
            "Plaintext requires explicit host loopback permission"
        );
        None
    } else {
        name_valid(&config.server_name)?;
        ensure!(
            config.ca_certificates.len() <= 8
                && config
                    .ca_certificates
                    .iter()
                    .all(|cert| !cert.is_empty() && cert.len() <= 32_000)
                && config.ca_certificates.iter().map(Vec::len).sum::<usize>() <= 128_000,
            "Client CA capacity exceeded"
        );
        let mut roots = rustls::RootCertStore::empty();
        for certificate in &config.ca_certificates {
            roots.add(rustls::pki_types::CertificateDer::from(certificate.clone()))?;
        }
        let mut tls = rustls::ClientConfig::builder_with_provider(Arc::new(
            rustls::crypto::ring::default_provider(),
        ))
        .with_protocol_versions(&[&rustls::version::TLS13])?
        .with_root_certificates(roots)
        .with_no_client_auth();
        tls.alpn_protocols = vec![b"http/1.1".to_vec()];
        Some(Arc::new(tls))
    };
    deadline.check()?;
    let tcp = TcpStream::connect_timeout(&config.address, deadline.remaining())?;
    let stop = AtomicBool::new(false);
    let socket = BoundSocket {
        socket: tcp,
        deadline,
        stop: &stop,
    };
    let mut io = if let Some(tls) = tls {
        Channel::Client(rustls::StreamOwned::new(
            rustls::ClientConnection::new(
                tls,
                rustls::pki_types::ServerName::try_from(config.server_name.clone())?,
            )?,
            socket,
        ))
    } else {
        Channel::Plain(socket)
    };
    let credential = Zeroizing::new(hex::encode(&*config.token));
    let authority = if config.ca_certificates.is_empty() {
        config.address.to_string()
    } else if config.server_name.parse::<std::net::Ipv6Addr>().is_ok() {
        format!("[{}]:{}", config.server_name, config.address.port())
    } else {
        format!("{}:{}", config.server_name, config.address.port())
    };
    let header = Zeroizing::new(format!(
        "{method} {path} HTTP/1.1\r\nHost: {}\r\nAuthorization: Bearer {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        authority,
        credential.as_str(),
        body.len()
    ));
    ensure!(header.len() <= 16_384, "Client header budget exceeded");
    write_all(&mut io, header.as_bytes(), deadline, &stop)
        .context("Sending bounded request headers")?;
    write_all(&mut io, &body, deadline, &stop).context("Sending bounded request body")?;
    let mut bytes = Zeroizing::new(Vec::new());
    let mut chunk = [0u8; 8192];
    let parsed;
    loop {
        let mut headers = [httparse::EMPTY_HEADER; 32];
        let mut response = httparse::Response::new(&mut headers);
        match response.parse(&bytes) {
            Ok(httparse::Status::Complete(end)) => {
                ensure!(
                    end <= 16_384 && response.version == Some(1),
                    "Invalid response HTTP version or framing"
                );
                let status = response.code.context("Response status absent")?;
                ensure!((200..=599).contains(&status), "Unsupported response status");
                let mut fields = BTreeMap::new();
                for field in response.headers.iter() {
                    ensure!(
                        fields
                            .insert(
                                field.name.to_ascii_lowercase(),
                                std::str::from_utf8(field.value)?.trim().to_owned()
                            )
                            .is_none(),
                        "Duplicate response header refused"
                    );
                }
                ensure!(
                    !fields.contains_key("transfer-encoding") && !fields.contains_key("upgrade"),
                    "Unsupported response framing"
                );
                let length = fields
                    .get("content-length")
                    .context("Response content length absent")?;
                ensure!(
                    !length.is_empty() && length.bytes().all(|b| b.is_ascii_digit()),
                    "Invalid response content length"
                );
                let length = length.parse::<usize>()?;
                ensure!(
                    length <= config.max_response_bytes
                        && length
                            .checked_add(body.len() + header.len() + end)
                            .is_some_and(|total| total <= 64_000_000),
                    "Client response budget exceeded"
                );
                ensure!(
                    length == 0
                        || fields.get("content-type").map(String::as_str)
                            == Some("application/json"),
                    "Response is not bounded JSON"
                );
                parsed = (status, end, length);
                break;
            }
            Ok(httparse::Status::Partial) => ensure!(
                bytes.len() < 16_384,
                "Client response header budget exceeded"
            ),
            Err(_) => bail!("Malformed HTTP response"),
        }
        let count = read_some(&mut io, &mut chunk, deadline, &stop)?;
        ensure!(count > 0, "Truncated response headers");
        bytes.extend_from_slice(&chunk[..count]);
    }
    let (status, end, length) = parsed;
    let expected = end + length;
    while bytes.len() < expected {
        let want = (expected - bytes.len()).min(chunk.len());
        let count = read_some(&mut io, &mut chunk[..want], deadline, &stop)?;
        ensure!(count > 0, "Truncated response body");
        bytes.extend_from_slice(&chunk[..count]);
    }
    ensure!(
        bytes.len() == expected,
        "Unexpected trailing response bytes"
    );
    // One request owns its connection. Demand the agreed close boundary so a
    // second response or an unterminated TLS stream cannot masquerade as proof.
    let count = read_some(&mut io, &mut chunk[..1], deadline, &stop)
        .context("Completing bounded response close")?;
    ensure!(count == 0, "Unexpected trailing response bytes");
    deadline.check()?;
    let body = if length == 0 {
        Value::Null
    } else {
        serde_json::from_slice(&bytes[end..])?
    };
    deadline.check()?;
    Ok(ClientResponse { status, body })
}

// Rustls can perform many reads/writes inside one StreamOwned call while
// assembling a TLS record or handshake. Enforce the same deadline and stop flag
// underneath that loop, including after each successful physical operation.
struct BoundSocket<'a> {
    socket: TcpStream,
    deadline: &'a Deadline,
    stop: &'a AtomicBool,
}
impl BoundSocket<'_> {
    fn budget(&self) -> std::io::Result<Duration> {
        if self.stop.load(Ordering::SeqCst) {
            return Err(std::io::Error::new(
                std::io::ErrorKind::ConnectionAborted,
                "Federation physical I/O cancelled",
            ));
        }
        let remaining = self.deadline.remaining();
        if remaining.is_zero() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::TimedOut,
                "Federation physical I/O deadline exceeded",
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
    fn write_vectored(&mut self, bytes: &[std::io::IoSlice<'_>]) -> std::io::Result<usize> {
        self.socket.set_write_timeout(Some(self.budget()?))?;
        let result = self.socket.write_vectored(bytes);
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
    Server(rustls::StreamOwned<rustls::ServerConnection, BoundSocket<'a>>),
    Client(rustls::StreamOwned<rustls::ClientConnection, BoundSocket<'a>>),
}
impl Read for Channel<'_> {
    fn read(&mut self, bytes: &mut [u8]) -> std::io::Result<usize> {
        match self {
            Self::Plain(s) => s.read(bytes),
            Self::Server(s) => s.read(bytes),
            Self::Client(s) => s.read(bytes),
        }
    }
}
impl Write for Channel<'_> {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        match self {
            Self::Plain(s) => s.write(bytes),
            Self::Server(s) => s.write(bytes),
            Self::Client(s) => s.write(bytes),
        }
    }
    fn flush(&mut self) -> std::io::Result<()> {
        match self {
            Self::Plain(s) => s.flush(),
            Self::Server(s) => s.flush(),
            Self::Client(s) => s.flush(),
        }
    }
}
fn wait_io(error: &std::io::Error) -> bool {
    matches!(
        error.kind(),
        std::io::ErrorKind::WouldBlock
            | std::io::ErrorKind::TimedOut
            | std::io::ErrorKind::Interrupted
    )
}
fn check(deadline: &Deadline, stop: &AtomicBool) -> Result<()> {
    deadline.check()?;
    ensure!(
        !stop.load(Ordering::SeqCst),
        "Federation transport cancelled"
    );
    Ok(())
}
fn read_some(
    io: &mut Channel<'_>,
    bytes: &mut [u8],
    deadline: &Deadline,
    stop: &AtomicBool,
) -> Result<usize> {
    loop {
        check(deadline, stop)?;
        match io.read(bytes) {
            Ok(count) => {
                check(deadline, stop)?;
                return Ok(count);
            }
            Err(error) if wait_io(&error) => {}
            Err(error) => return Err(error.into()),
        }
    }
}
fn write_all(
    io: &mut Channel<'_>,
    mut bytes: &[u8],
    deadline: &Deadline,
    stop: &AtomicBool,
) -> Result<()> {
    while !bytes.is_empty() {
        check(deadline, stop)?;
        match io.write(bytes) {
            Ok(0) => bail!("Transport closed during response"),
            Ok(count) => {
                check(deadline, stop)?;
                bytes = &bytes[count..]
            }
            Err(error) if wait_io(&error) => {}
            Err(error) => return Err(error.into()),
        }
    }
    loop {
        check(deadline, stop)?;
        match io.flush() {
            Ok(()) => {
                check(deadline, stop)?;
                return Ok(());
            }
            Err(error) if wait_io(&error) => {}
            Err(error) => return Err(error.into()),
        }
    }
}
#[derive(Clone, Copy)]
struct Fault(u16, &'static str);
type HttpResult<T> = std::result::Result<T, Fault>;
struct Incoming {
    method: String,
    target: String,
    host: String,
    token_hash: [u8; 32],
    body: Zeroizing<Vec<u8>>,
    total_bytes: usize,
}
fn token_hash(value: &str) -> HttpResult<[u8; 32]> {
    let hex = value
        .strip_prefix("Bearer ")
        .ok_or(Fault(401, "Unauthorized"))?;
    if !valid_id(hex) {
        return Err(Fault(401, "Unauthorized"));
    }
    let mut token = Zeroizing::new([0u8; 32]);
    hex::decode_to_slice(hex, &mut *token).map_err(|_| Fault(401, "Unauthorized"))?;
    Ok(Sha256::digest(token.as_slice()).into())
}
fn incoming(
    io: &mut Channel<'_>,
    config: &Config,
    address: SocketAddr,
    policy: &RwLock<Policy>,
    deadline: &Deadline,
    stop: &AtomicBool,
) -> HttpResult<Incoming> {
    let mut bytes = Zeroizing::new(Vec::new());
    let mut chunk = [0u8; 8192];
    let parsed;
    loop {
        if bytes.len() >= 16_384 {
            return Err(Fault(431, "Header budget exceeded"));
        }
        let mut headers = [httparse::EMPTY_HEADER; 32];
        let mut request = httparse::Request::new(&mut headers);
        match request.parse(&bytes) {
            Ok(httparse::Status::Complete(end)) => {
                if end > 16_384 || request.version != Some(1) {
                    return Err(Fault(400, "Invalid HTTP version or framing"));
                }
                let method = request
                    .method
                    .ok_or(Fault(400, "Method absent"))?
                    .to_owned();
                let target = request.path.ok_or(Fault(400, "Target absent"))?.to_owned();
                if !["GET", "POST"].contains(&method.as_str())
                    || !target.starts_with('/')
                    || target.len() > 4096
                    || target.contains(['#', '\r', '\n', '\0'])
                {
                    return Err(Fault(400, "Invalid request target"));
                }
                let mut fields = BTreeMap::new();
                for header in request.headers.iter() {
                    let name = header.name.to_ascii_lowercase();
                    if fields
                        .insert(
                            name,
                            Zeroizing::new(
                                std::str::from_utf8(header.value)
                                    .map_err(|_| Fault(400, "Invalid header encoding"))?
                                    .trim()
                                    .to_owned(),
                            ),
                        )
                        .is_some()
                    {
                        return Err(Fault(400, "Duplicate header refused"));
                    }
                }
                if fields.contains_key("origin") {
                    return Err(Fault(403, "Browser origins are not permitted"));
                }
                if fields.contains_key("transfer-encoding")
                    || fields.contains_key("upgrade")
                    || fields.contains_key("expect")
                {
                    return Err(Fault(400, "Unsupported HTTP framing"));
                }
                let host = fields
                    .get("host")
                    .ok_or(Fault(400, "Host absent"))?
                    .to_string();
                let content_length = if let Some(value) = fields.get("content-length") {
                    if value.is_empty() || !value.bytes().all(|b| b.is_ascii_digit()) {
                        return Err(Fault(400, "Invalid content length"));
                    }
                    value
                        .parse::<usize>()
                        .map_err(|_| Fault(400, "Invalid content length"))?
                } else {
                    0
                };
                if content_length > config.max_body_bytes {
                    return Err(Fault(413, "Body budget exceeded"));
                }
                if method == "GET" && content_length > 0
                    || method == "POST"
                        && (content_length == 0
                            || fields.get("content-type").map(|value| value.as_str())
                                != Some("application/json"))
                {
                    return Err(Fault(400, "Invalid request body framing"));
                }
                let token_hash = token_hash(
                    fields
                        .get("authorization")
                        .ok_or(Fault(401, "Unauthorized"))?,
                )?;
                parsed = (method, target, host, token_hash, end, content_length);
                break;
            }
            Ok(httparse::Status::Partial) => {}
            Err(_) => return Err(Fault(400, "Malformed HTTP request")),
        }
        let count = read_some(io, &mut chunk, deadline, stop)
            .map_err(|_| Fault(408, "Request deadline exceeded or transport closed"))?;
        if count == 0 {
            return Err(Fault(400, "Truncated request"));
        }
        bytes.extend_from_slice(&chunk[..count]);
    }
    let (method, target, host, token_hash, end, size) = parsed;
    let metadata = Incoming {
        method: method.clone(),
        target: target.clone(),
        host: host.clone(),
        token_hash,
        body: Zeroizing::new(Vec::new()),
        total_bytes: end,
    };
    if !host_allowed(&host, address, &config.server_names) {
        return Err(Fault(403, "Host not authorized"));
    }
    if matches!(&io,Channel::Server(stream) if stream.conn.server_name().is_some_and(|name|!config.server_names.iter().any(|allowed|name.eq_ignore_ascii_case(allowed))))
    {
        return Err(Fault(403, "TLS server name not authorized"));
    }
    preflight(&metadata, policy)?;
    let expected = end
        .checked_add(size)
        .ok_or(Fault(413, "Body budget exceeded"))?;
    while bytes.len() < expected {
        let want = (expected - bytes.len()).min(chunk.len());
        let count = read_some(io, &mut chunk[..want], deadline, stop)
            .map_err(|_| Fault(408, "Request deadline exceeded or transport closed"))?;
        if count == 0 {
            return Err(Fault(400, "Truncated body"));
        }
        bytes.extend_from_slice(&chunk[..count]);
    }
    if bytes.len() != expected {
        return Err(Fault(400, "Pipelined request bytes refused"));
    }
    Ok(Incoming {
        method,
        target,
        host,
        token_hash,
        body: Zeroizing::new(bytes[end..].to_vec()),
        total_bytes: expected,
    })
}
fn host_allowed(host: &str, address: SocketAddr, names: &[String]) -> bool {
    if host == address.to_string() {
        return true;
    }
    names.iter().any(|name| {
        let authority = if name.parse::<std::net::Ipv6Addr>().is_ok() {
            format!("[{name}]:{}", address.port())
        } else {
            format!("{name}:{}", address.port())
        };
        host.eq_ignore_ascii_case(&authority)
    })
}
enum Route {
    Status,
    Sources,
    Read(String, String, String),
    Graph(String, String),
    Export(String, String),
    Import(String),
    Revoke,
}
fn preflight(request: &Incoming, policy: &RwLock<Policy>) -> HttpResult<()> {
    let policy = policy
        .read()
        .map_err(|_| Fault(503, "Policy unavailable"))?;
    let principal = policy
        .authenticate(&request.token_hash)
        .ok_or(Fault(401, "Unauthorized"))?;
    let allowed = match route(request)? {
        Route::Status => true,
        Route::Sources => principal.roles.contains(&Role::Read),
        Route::Read(grant, _, _) | Route::Graph(grant, _) => {
            principal.roles.contains(&Role::Read) && principal.imports.contains(&grant)
        }
        Route::Export(grant, _) => {
            principal.roles.contains(&Role::Read) && principal.exports.contains(&grant)
        }
        Route::Import(grant) => {
            principal.roles.contains(&Role::Import) && principal.imports.contains(&grant)
        }
        Route::Revoke => principal.roles.contains(&Role::Admin),
    };
    if !allowed {
        return Err(Fault(403, "Host role and grant required"));
    }
    Ok(())
}
fn query_path(text: &str) -> HttpResult<String> {
    if !text.starts_with("path=") || text.contains('&') {
        return Err(Fault(400, "Exactly one path query is required"));
    }
    let raw = &text[5..];
    let bytes = raw.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            if i + 2 >= bytes.len()
                || !bytes[i + 1].is_ascii_hexdigit()
                || !bytes[i + 2].is_ascii_hexdigit()
            {
                return Err(Fault(400, "Invalid path encoding"));
            }
            i += 3;
        } else {
            i += 1;
        }
    }
    let mut decoded = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'%' => {
                let value = u8::from_str_radix(&raw[i + 1..i + 3], 16)
                    .map_err(|_| Fault(400, "Invalid path encoding"))?;
                decoded.push(value);
                i += 3;
            }
            b'+' => {
                decoded.push(b' ');
                i += 1;
            }
            value => {
                decoded.push(value);
                i += 1;
            }
        }
    }
    let path = String::from_utf8(decoded).map_err(|_| Fault(400, "Path is not UTF-8"))?;
    if path.contains(['\r', '\n', '\0']) {
        return Err(Fault(400, "Invalid path"));
    }
    Ok(path)
}
fn route(request: &Incoming) -> HttpResult<Route> {
    let (mut path, query) = (request.target.as_str(), None);
    let query = if let Some((base, query)) = path.split_once('?') {
        path = base;
        Some(query)
    } else {
        query
    };
    let parts: Vec<_> = path.split('/').collect();
    let method = request.method.as_str();
    match parts.as_slice() {
        ["", "v1", "status"] if method == "GET" && query.is_none() => Ok(Route::Status),
        ["", "v1", "sources"] if method == "GET" && query.is_none() => Ok(Route::Sources),
        ["", "v1", "admin", "revoke"] if method == "POST" && query.is_none() => Ok(Route::Revoke),
        ["", "v1", "import", grant] if method == "POST" && query.is_none() => {
            identifier(grant).map_err(|_| Fault(400, "Invalid grant ID"))?;
            Ok(Route::Import(grant.to_string()))
        }
        ["", "v1", kind, grant, id] if method == "GET" => {
            identifier(grant).map_err(|_| Fault(400, "Invalid grant ID"))?;
            if !valid_id(id) {
                return Err(Fault(400, "Invalid source ID"));
            }
            match *kind {
                "read" => Ok(Route::Read(
                    grant.to_string(),
                    id.to_string(),
                    query_path(query.ok_or(Fault(400, "Path absent"))?)?,
                )),
                "graph" if query.is_none() => Ok(Route::Graph(grant.to_string(), id.to_string())),
                "export" if query.is_none() => Ok(Route::Export(grant.to_string(), id.to_string())),
                _ => Err(Fault(400, "Invalid source request")),
            }
        }
        _ => Err(Fault(404, "Unknown endpoint")),
    }
}
fn dispatch(
    engine: &Engine,
    request: &Incoming,
    access: &Access,
    policy: &RwLock<Policy>,
    deadline: &Deadline,
) -> HttpResult<Value> {
    let route = route(request)?;
    if matches!(route, Route::Revoke) {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase", deny_unknown_fields)]
        struct Revoke {
            principal_id: String,
        }
        let action: Revoke = serde_json::from_slice(&request.body)
            .map_err(|_| Fault(400, "Invalid revoke request"))?;
        identifier(&action.principal_id).map_err(|_| Fault(400, "Invalid principal ID"))?;
        let mut policy = policy
            .write()
            .map_err(|_| Fault(503, "Policy unavailable"))?;
        let principal = policy
            .authenticate(&request.token_hash)
            .ok_or(Fault(401, "Unauthorized"))?;
        if !principal.roles.contains(&Role::Admin) {
            return Err(Fault(403, "Admin role required"));
        }
        return Ok(
            json!({"principalId":action.principal_id,"revoked":policy.principals.remove(&action.principal_id).is_some()}),
        );
    }
    let policy = policy
        .read()
        .map_err(|_| Fault(503, "Policy unavailable"))?;
    let principal = policy
        .authenticate(&request.token_hash)
        .ok_or(Fault(401, "Unauthorized"))?;
    let read = principal.roles.contains(&Role::Read);
    match route {
        Route::Status => Ok(
            json!({"principalId":principal.id,"roles":principal.roles,"runtime":"rust","sourceAuthority":"fixed_host_grants","executionAuthority":false}),
        ),
        Route::Sources => {
            if !read {
                return Err(Fault(403, "Read role required"));
            }
            let list = federation::list(&engine.store, deadline)
                .map_err(|_| Fault(400, "Source listing unavailable"))?;
            let mut items = Vec::new();
            for item in list["items"]
                .as_array()
                .ok_or(Fault(400, "Source listing unavailable"))?
            {
                let scope: Scope = serde_json::from_value(item["scope"].clone())
                    .map_err(|_| Fault(400, "Source metadata unavailable"))?;
                for grant_id in &principal.imports {
                    let grant = &access.imports[grant_id];
                    if grant.scope == scope
                        && item["senderWorkspaceId"] == grant.sender_workspace_id
                    {
                        let id = item["remoteId"]
                            .as_str()
                            .ok_or(Fault(400, "Source metadata unavailable"))?;
                        federation::snapshot(engine, id, grant, deadline).map_err(|_| {
                            Fault(
                                400,
                                "Retained scoped source requires recovery or current host grant",
                            )
                        })?;
                        let mut visible = item.clone();
                        visible["grantId"] = json!(grant_id);
                        items.push(visible);
                        break;
                    }
                }
            }
            Ok(
                json!({"namespace":"federated","items":items,"truncated":list["truncated"].as_bool().unwrap_or(false)}),
            )
        }
        Route::Import(grant_id) => {
            if !principal.roles.contains(&Role::Import) || !principal.imports.contains(&grant_id) {
                return Err(Fault(403, "Import grant required"));
            }
            let input = serde_json::from_slice(&request.body)
                .map_err(|_| Fault(400, "Invalid signed envelope"))?;
            federation::import_snapshot(engine, input, &access.imports[&grant_id], deadline)
                .map_err(|_| Fault(400, "Signed source import refused"))
        }
        Route::Read(grant_id, id, path) => {
            if !read || !principal.imports.contains(&grant_id) {
                return Err(Fault(403, "Read grant required"));
            }
            federation::read(engine, &id, &path, &access.imports[&grant_id], deadline)
                .map_err(|_| Fault(400, "Scoped source read refused"))
        }
        Route::Graph(grant_id, id) => {
            if !read || !principal.imports.contains(&grant_id) {
                return Err(Fault(403, "Read grant required"));
            }
            serde_json::to_value(
                federation::graph(engine, &id, &access.imports[&grant_id], deadline)
                    .map_err(|_| Fault(400, "Scoped graph read refused"))?,
            )
            .map_err(|_| Fault(500, "Graph response unavailable"))
        }
        Route::Export(grant_id, id) => {
            if !read || !principal.exports.contains(&grant_id) {
                return Err(Fault(403, "Export grant required"));
            }
            let entry = &access.exports[&grant_id];
            let snapshot = engine
                .store
                .snapshot(&id)
                .map_err(|_| Fault(400, "Local snapshot unavailable"))?;
            federation::export_snapshot(
                &engine.store,
                &snapshot,
                &entry.configuration,
                &entry.private_key,
                deadline,
            )
            .map_err(|_| Fault(400, "Source export refused"))
        }
        Route::Revoke => unreachable!(),
    }
}
struct Transport<'a> {
    tls: Option<&'a Arc<rustls::ServerConfig>>,
    address: SocketAddr,
}
fn connection(
    engine: &Engine,
    tcp: TcpStream,
    transport: Transport<'_>,
    config: &Config,
    access: &Access,
    policy: &RwLock<Policy>,
    stop: &AtomicBool,
) -> Result<()> {
    let Transport { tls, address } = transport;
    let deadline = Deadline::new(config.request_timeout_ms);
    let socket = BoundSocket {
        socket: tcp,
        deadline: &deadline,
        stop,
    };
    let mut io = if let Some(tls) = tls {
        Channel::Server(rustls::StreamOwned::new(
            rustls::ServerConnection::new(Arc::clone(tls))?,
            socket,
        ))
    } else {
        Channel::Plain(socket)
    };
    let request = incoming(&mut io, config, address, policy, &deadline, stop);
    let mut total = 0;
    let result = match request {
        Ok(request) => {
            total = request.total_bytes;
            if !host_allowed(&request.host, address, &config.server_names) {
                Err(Fault(403, "Host not authorized"))
            } else if matches!(&io,Channel::Server(stream) if stream.conn.server_name().is_some_and(|name|!config.server_names.iter().any(|allowed|name.eq_ignore_ascii_case(allowed))))
            {
                Err(Fault(403, "TLS server name not authorized"))
            } else {
                // Core loops and SQLite contention share this smaller budget,
                // leaving the owned worker enough time to join on shutdown.
                let operation_deadline =
                    Deadline::new(deadline.remaining().as_millis().min(4_000) as u64);
                engine
                    .store
                    .set_deadline(&operation_deadline)
                    .map_err(|_| Fault(408, "Core operation deadline exceeded"))
                    .and_then(|_| dispatch(engine, &request, access, policy, &operation_deadline))
            }
        }
        Err(error) => Err(error),
    };
    let (status, value) = match result {
        Ok(value) => (200, value),
        Err(Fault(status, message)) => (status, json!({"error":message,"complete":false})),
    };
    let mut body = serde_json::to_vec(&value)?;
    let mut status = status;
    if body.len() > config.max_response_bytes
        || body
            .len()
            .checked_add(total)
            .is_none_or(|bytes| bytes + 512 > 64_000_000)
    {
        status = 413;
        body = b"{\"error\":\"budget\"}".to_vec();
        if body.len() > config.max_response_bytes {
            body.clear();
        }
    }
    let header = format!(
        "HTTP/1.1 {status} Response\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\nCache-Control: no-store\r\nX-Content-Type-Options: nosniff\r\n\r\n",
        body.len()
    );
    write_all(&mut io, header.as_bytes(), &deadline, stop)?;
    write_all(&mut io, &body, &deadline, stop)?;
    if let Channel::Server(stream) = &mut io {
        stream.conn.send_close_notify();
    }
    write_all(&mut io, &[], &deadline, stop)?;
    if status >= 400 {
        // Closing a TCP socket with unread upload bytes can discard its already
        // authenticated denial response through an OS reset. Publish FIN first,
        // then drain only immediately arriving encrypted/socket bytes under a
        // separate50ms/64KiB cleanup budget. No body parsing or operation is
        // allowed; larger continuing uploads may still receive a reset.
        let socket = match &mut io {
            Channel::Plain(socket) => &mut socket.socket,
            Channel::Server(stream) => &mut stream.sock.socket,
            Channel::Client(_) => unreachable!(),
        };
        let _ = socket.shutdown(Shutdown::Write);
        socket.set_nonblocking(true)?;
        let cleanup = Deadline::new(50);
        let mut scratch = Zeroizing::new([0u8; 8192]);
        let mut drained = 0usize;
        while cleanup.check().is_ok() && !stop.load(Ordering::SeqCst) && drained < 65_536 {
            let want = (65_536 - drained).min(scratch.len());
            match socket.read(&mut scratch[..want]) {
                Ok(0) => break,
                Ok(count) => drained += count,
                Err(error) if wait_io(&error) => thread::sleep(Duration::from_millis(1)),
                Err(_) => break,
            }
        }
    }
    Ok(())
}
