//! Explicit host-permitted GitHub webhook ingress.
use super::*;
use crate::{Engine, federation_http::TlsMaterial};
use std::{
    net::{SocketAddr, TcpListener},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    thread::{self, JoinHandle},
    time::Duration,
};
#[path = "github_app_ingress_http.rs"]
mod http;
#[path = "github_app_ingress_socket.rs"]
mod socket;
const INGRESS_INVALID: &str = "github_app_ingress_input_invalid";
const INGRESS_CAPACITY: &str = "github_app_ingress_capacity_exceeded";
/// Explicit host capabilities, never selected by incoming HTTP or stored JSON.
pub struct PageService {
    pub allow_serving: bool,
    pub options: super::pages::Options,
    pub clock: Arc<super::worker::Clock>,
}
pub fn start_pages(
    engine: Engine,
    host: Host,
    options: Options,
    config: Config,
    pages: PageService,
    d: &Deadline,
) -> Result<ServerHandle> {
    pages.admit_limits(&config, &options, d)?;
    config.admit_binding()?;
    start_inner(engine, host, options, config, Some(pages), d)
}
impl PageService {
    /// Ordinary flags and bounds before selecting host files, TLS keys or credentials.
    pub fn admit_limits(&self, config: &Config, options: &Options, d: &Deadline) -> Result<()> {
        config.admit_mode(options, Some(self), d)
    }
}
pub struct Config {
    pub allow_webhooks: bool,
    pub listen: SocketAddr,
    pub allow_remote: bool,
    pub tls: Option<TlsMaterial>,
    pub server_names: Vec<String>,
    pub request_timeout_ms: u64,
    pub lifetime_ms: u64,
    pub max_connections: usize,
    pub max_requests: usize,
    pub max_response_bytes: usize,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            allow_webhooks: false,
            listen: "127.0.0.1:0".parse().expect("literal loopback"),
            allow_remote: false,
            tls: None,
            server_names: vec![],
            request_timeout_ms: 5000,
            lifetime_ms: 600_000,
            max_connections: 4,
            max_requests: 1000,
            max_response_bytes: 4096,
        }
    }
}
pub struct ServerHandle {
    address: SocketAddr,
    announcement: Value,
    stop: Arc<AtomicBool>,
    worker: Option<JoinHandle<Result<Value>>>,
}
impl ServerHandle {
    pub fn local_addr(&self) -> SocketAddr {
        self.address
    }
    pub fn announcement(&self) -> &Value {
        &self.announcement
    }
    pub fn shutdown(mut self) -> Result<Value> {
        self.stop.store(true, Ordering::SeqCst);
        self.finish()
    }
    pub fn wait(mut self) -> Result<Value> {
        self.finish()
    }
    fn finish(&mut self) -> Result<Value> {
        self.worker
            .take()
            .ok_or_else(|| anyhow::anyhow!(INGRESS_INVALID))?
            .join()
            .map_err(|_| anyhow::anyhow!("github_app_ingress_worker_failed"))?
    }
}
impl Drop for ServerHandle {
    fn drop(&mut self) {
        if self.worker.is_some() {
            self.stop.store(true, Ordering::SeqCst);
            if self.finish().is_err() {
                eprintln!("github_app_ingress_cleanup_unverified");
            }
        }
    }
}
impl Config {
    pub fn admit(&self, o: &Options, d: &Deadline) -> Result<()> {
        self.admit_limits(o, d)?;
        self.admit_binding()
    }
    fn admit_binding(&self) -> Result<()> {
        ensure!(
            self.listen.ip().is_loopback() || (self.allow_remote && self.tls.is_some()),
            "github_app_remote_tls_permission_required"
        );
        ensure!(
            if self.tls.is_some() {
                !self.server_names.is_empty()
            } else {
                self.server_names.is_empty()
            },
            INGRESS_INVALID
        );
        Ok(())
    }
    /// Preflight ordinary host flags before loading the separately selected TLS key.
    pub fn admit_limits(&self, o: &Options, d: &Deadline) -> Result<()> {
        self.admit_mode(o, None, d)
    }
    fn admit_mode(&self, o: &Options, pages: Option<&PageService>, d: &Deadline) -> Result<()> {
        if let Some(pages) = pages {
            ensure!(
                pages.allow_serving,
                "github_app_pages_serving_permission_required"
            );
            pages.options.admit_read(d)?;
            ensure!(
                !self.allow_webhooks || o.allow_writes,
                "github_app_permission_required"
            );
        } else {
            ensure!(
                self.allow_webhooks && o.allow_writes,
                "github_app_permission_required"
            );
        }
        d.check()?;
        super::options(o)?;
        ensure!(
            self.max_connections > 0
                && self.max_requests > 0
                && self.lifetime_ms > 0
                && self.max_response_bytes > 0
                && self.request_timeout_ms > 0
                && o.max_payload_bytes > 0
                && o.max_output_bytes > 0,
            INGRESS_CAPACITY
        );
        ensure!(
            self.max_connections <= 8
                && self.max_requests <= 10_000
                && self.lifetime_ms <= 86_400_000
                && self.max_response_bytes <= if pages.is_some() { 4_065_536 } else { 65_536 }
                && (10..=10_000).contains(&self.request_timeout_ms),
            INGRESS_INVALID
        );
        let mut names = BTreeSet::new();
        ensure!(
            self.server_names.len() <= 8
                && self.server_names.iter().all(|name| !name.is_empty()
                    && name.len() <= 253
                    && name.is_ascii()
                    && name.to_ascii_lowercase() == *name
                    && !name.contains('*')
                    && names.insert(name)
                    && rustls::pki_types::ServerName::try_from(name.clone()).is_ok()),
            INGRESS_INVALID
        );
        Ok(())
    }
}
#[derive(Default)]
struct Counters {
    durable: AtomicUsize,
    pings: AtomicUsize,
    refused: AtomicUsize,
    replies: AtomicUsize,
}
struct Shared {
    engine: Mutex<Engine>,
    workspace_id: String,
    pages: Option<PageService>,
    host: Host,
    options: Options,
    config: Config,
    address: SocketAddr,
    tls: Option<Arc<rustls::ServerConfig>>,
    deadline: Deadline,
    stop: Arc<AtomicBool>,
    counts: Counters,
}
pub fn start(
    engine: Engine,
    host: Host,
    o: Options,
    c: Config,
    d: &Deadline,
) -> Result<ServerHandle> {
    c.admit(&o, d)?;
    start_inner(engine, host, o, c, None, d)
}
fn start_inner(
    engine: Engine,
    host: Host,
    o: Options,
    mut c: Config,
    pages: Option<PageService>,
    d: &Deadline,
) -> Result<ServerHandle> {
    host.profile.validate()?;
    ensure!(
        engine.store.db.is_autocommit(),
        "github_app_transaction_required"
    );
    let tls = c.tls.take().map(socket::tls_server).transpose()?;
    let stop = Arc::new(AtomicBool::new(false));
    let deadline = Deadline::run_bounded(d, stop.clone(), || Ok(Deadline::new(c.lifetime_ms)))?;
    let listener = TcpListener::bind(c.listen)
        .map_err(|_| anyhow::anyhow!("github_app_listener_unavailable"))?;
    listener.set_nonblocking(true)?;
    let address = listener.local_addr()?;
    let scheme = if tls.is_some() { "https" } else { "http" };
    let endpoint = if c.allow_webhooks {
        format!("{scheme}://{address}/github/webhook")
    } else {
        format!("{scheme}://{address}")
    };
    let mut announcement = json!({"endpoint":endpoint,
        "tls":tls.is_some(),"durableQueue":c.allow_webhooks,"sourceWritten":false,"published":false,
        "installationTokenVerified":false,"currentHeadVerified":false,"complete":false});
    if pages.is_some() {
        announcement["pageServing"] = json!(true);
        announcement["webhookServing"] = json!(c.allow_webhooks);
        announcement["pageEndpointTemplate"] =
            json!(format!("{scheme}://{address}/p/{{pageId}}?t={{token}}"));
    }
    // The complete announcement is admitted before any worker can mutate state.
    crate::json::encode_bounded(&announcement, o.max_output_bytes.saturating_sub(1), d)?;
    d.check()?;
    let shared = Arc::new(Shared {
        workspace_id: engine.store.workspace_id.clone(),
        pages,
        engine: Mutex::new(engine),
        host,
        options: o,
        config: c,
        address,
        tls,
        deadline,
        stop: stop.clone(),
        counts: Counters::default(),
    });
    let worker = thread::Builder::new()
        .name("yoke-github-ingress".into())
        .spawn(move || run(listener, shared))
        .map_err(|_| anyhow::anyhow!("github_app_ingress_worker_failed"))?;
    Ok(ServerHandle {
        address,
        announcement,
        stop,
        worker: Some(worker),
    })
}
fn run(listener: TcpListener, shared: Arc<Shared>) -> Result<Value> {
    let mut workers: Vec<JoinHandle<()>> = Vec::new();
    let mut received = 0usize;
    let result = (|| -> Result<()> {
        while shared.deadline.check().is_ok() && received < shared.config.max_requests {
            let mut index = 0;
            while index < workers.len() {
                if workers[index].is_finished() {
                    workers
                        .swap_remove(index)
                        .join()
                        .map_err(|_| anyhow::anyhow!("github_app_ingress_worker_failed"))?;
                } else {
                    index += 1;
                }
            }
            match listener.accept() {
                Ok((stream, peer)) => {
                    received += 1;
                    if !shared.config.allow_remote && !peer.ip().is_loopback() {
                        continue;
                    }
                    if workers.len() >= shared.config.max_connections {
                        let _ =
                            Deadline::run_bounded(&shared.deadline, shared.stop.clone(), || {
                                http::handle(stream, &shared, &Deadline::new(200), true);
                                Ok(())
                            });
                    } else {
                        let state = shared.clone();
                        workers.push(
                            thread::Builder::new()
                                .name("yoke-github-delivery".into())
                                .spawn(move || {
                                    let _ = Deadline::run_bounded(
                                        &state.deadline,
                                        state.stop.clone(),
                                        || {
                                            http::handle(
                                                stream,
                                                &state,
                                                &Deadline::new(state.config.request_timeout_ms),
                                                false,
                                            );
                                            Ok(())
                                        },
                                    );
                                })
                                .map_err(|_| anyhow::anyhow!("github_app_ingress_worker_failed"))?,
                        );
                    }
                }
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(5))
                }
                Err(_) => anyhow::bail!("github_app_listener_unavailable"),
            }
        }
        Ok(())
    })();
    drop(listener);
    if result.is_err() {
        shared.stop.store(true, Ordering::SeqCst);
    }
    let mut worker_failed = false;
    for worker in workers {
        if worker.join().is_err() {
            worker_failed = true;
        }
    }
    result?;
    ensure!(!worker_failed, "github_app_ingress_worker_failed");
    Ok(
        json!({"connectionsReceived":received,"durableAdmissions":shared.counts.durable.load(Ordering::SeqCst),
        "authenticatedPings":shared.counts.pings.load(Ordering::SeqCst),"refusedRequests":shared.counts.refused.load(Ordering::SeqCst),
        "responsesWritten":shared.counts.replies.load(Ordering::SeqCst),"listenerClosed":true,"workersJoined":true,
        "sourceWritten":false,"published":false,"complete":false}),
    )
}
