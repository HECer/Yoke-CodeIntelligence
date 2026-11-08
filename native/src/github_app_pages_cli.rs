//! Explicit page roles; admit permissions and bounds before host inputs or Store opening.
use anyhow::{Result, ensure};
use clap::{Args, Subcommand};
use std::{
    io::Write,
    net::SocketAddr,
    path::{Path, PathBuf},
    sync::Arc,
};
use yoke::{
    Engine,
    capture::Deadline,
    github_app::{self as app, ingress, pages, worker},
};

const INVALID: &str = "github_app_pages_input_invalid";
#[derive(Args)]
pub struct Arguments {
    #[command(subcommand)]
    operation: Operation,
    #[arg(long, global = true)]
    allow_pages: bool,
    #[arg(long, global = true)]
    allow_page_serving: bool,
    #[arg(long, global = true, default_value_t = 1_209_600)]
    ttl_seconds: u64,
    #[arg(long, global = true, default_value_t = 500)]
    max_pages: usize,
    #[arg(long, global = true, default_value_t = 64_000_000)]
    max_page_store_bytes: usize,
    #[arg(long, global = true, default_value_t = 2_000_000)]
    max_html_bytes: usize,
    #[arg(long, global = true, default_value_t = 5000)]
    max_nodes: usize,
    #[arg(long, global = true, default_value_t = 20_000)]
    max_observations: usize,
    #[arg(long, global = true, default_value_t = 1000)]
    max_records: usize,
    #[arg(long, global = true, default_value_t = 64_000_000)]
    max_worker_store_bytes: usize,
}
#[derive(Subcommand)]
enum Operation {
    Preview {
        #[arg(long)]
        request: Option<PathBuf>,
    },
    Store {
        #[arg(long)]
        request: Option<PathBuf>,
    },
    Remove {
        #[arg(long)]
        request: Option<PathBuf>,
    },
    Serve(Serving),
}
#[derive(Args)]
struct Serving {
    #[arg(long, default_value = "127.0.0.1:0")]
    listen: SocketAddr,
    #[arg(long)]
    allow_remote_pages: bool,
    #[arg(long)]
    tls_credential: Option<String>,
    #[arg(long = "tls-server-name")]
    server_names: Vec<String>,
    #[arg(long, default_value_t = 5000)]
    request_timeout_ms: u64,
    #[arg(long)]
    lifetime_ms: Option<u64>,
    #[arg(long, default_value_t = 4)]
    max_connections: usize,
    #[arg(long, default_value_t = 1000)]
    max_requests: usize,
    #[arg(long, default_value_t = 4_065_536)]
    max_response_bytes: usize,
}
enum Request {
    Preview(pages::PreviewRequest),
    Store(pages::StoreRequest),
    Remove(pages::RemoveRequest),
    Serve(Box<ingress::Config>),
}
pub struct Prepared {
    host: app::Host,
    request: Request,
    options: pages::Options,
    clock: Arc<worker::Clock>,
}
fn bytes(path: Option<&Path>, cap: usize, d: &Deadline) -> Result<Vec<u8>> {
    crate::feedback_cli::host_bytes(path.ok_or_else(|| anyhow::anyhow!(INVALID))?, cap, d)
        .map_err(|_| anyhow::anyhow!(INVALID))
}
impl Arguments {
    pub fn is_serve(&self) -> bool {
        matches!(self.operation, Operation::Serve(_))
    }
    pub fn check_permission(&self, webhooks: bool, network: bool, writes: bool) -> Result<()> {
        if self.is_serve() {
            ensure!(
                self.allow_page_serving,
                "github_app_pages_serving_permission_required"
            );
            ensure!(self.allow_pages, "github_app_pages_permission_required");
            ensure!(!network, INVALID);
            ensure!(!webhooks || writes, "github_app_permission_required");
            ensure!(webhooks || !writes, INVALID);
        } else {
            ensure!(webhooks, "github_app_permission_required");
            if matches!(self.operation, Operation::Preview { .. }) {
                ensure!(
                    !network && !writes && !self.allow_pages && !self.allow_page_serving,
                    INVALID
                );
            } else {
                ensure!(self.allow_pages, "github_app_pages_permission_required");
                ensure!(writes, "github_app_permission_required");
                ensure!(!network && !self.allow_page_serving, INVALID);
            }
        }
        Ok(())
    }
    pub fn prepare(
        &self,
        profile: Option<&Path>,
        queue: app::Options,
        webhooks: bool,
        network: bool,
        timeout: u64,
        d: &Deadline,
    ) -> Result<Prepared> {
        self.check_permission(webhooks, network, queue.allow_writes)?;
        ensure!(
            queue.max_payload_bytes > 0
                && queue.max_deliveries > 0
                && queue.max_jobs > 0
                && queue.max_transitions > 0
                && queue.max_store_bytes > 0
                && queue.max_output_bytes > 0
                && self.ttl_seconds > 0
                && self.max_pages > 0
                && self.max_page_store_bytes > 0
                && self.max_html_bytes > 0
                && self.max_nodes > 0
                && self.max_observations > 0
                && self.max_records > 0
                && self.max_worker_store_bytes > 0,
            "github_app_pages_capacity_exceeded"
        );
        let options = pages::Options {
            allow_pages: self.allow_pages,
            ttl_seconds: self.ttl_seconds,
            max_pages: self.max_pages,
            max_store_bytes: self.max_page_store_bytes,
            max_html_bytes: self.max_html_bytes,
            max_nodes: self.max_nodes,
            max_observations: self.max_observations,
            max_output_bytes: queue.max_output_bytes,
            worker: worker::Options {
                max_records: self.max_records,
                max_store_bytes: self.max_worker_store_bytes,
                max_output_bytes: queue.max_output_bytes,
                queue,
                ..Default::default()
            },
        };
        options.admit(d)?;
        let clock = Arc::new(worker::Clock::system());
        let service = if let Operation::Serve(args) = &self.operation {
            let config = ingress::Config {
                allow_webhooks: webhooks,
                listen: args.listen,
                allow_remote: args.allow_remote_pages,
                tls: None,
                server_names: args.server_names.clone(),
                request_timeout_ms: args.request_timeout_ms,
                lifetime_ms: args.lifetime_ms.unwrap_or(timeout),
                max_connections: args.max_connections,
                max_requests: args.max_requests,
                max_response_bytes: args.max_response_bytes,
            };
            ingress::PageService {
                allow_serving: self.allow_page_serving,
                options: options.clone(),
                clock: clock.clone(),
            }
            .admit_limits(&config, &options.worker.queue, d)?;
            ensure!(
                args.listen.ip().is_loopback()
                    || (args.allow_remote_pages && args.tls_credential.is_some()),
                "github_app_remote_tls_permission_required"
            );
            ensure!(
                args.tls_credential.is_some() != args.server_names.is_empty(),
                INVALID
            );
            Some(config)
        } else {
            None
        };
        // Parse both explicit bounded documents before reading any host credential.
        let profile = app::HostProfile::parse(&bytes(profile, 32_000, d)?)
            .map_err(|_| anyhow::anyhow!(INVALID))?;
        let request =
            match &self.operation {
                Operation::Preview { request } => Request::Preview(pages::PreviewRequest::parse(
                    &bytes(request.as_deref(), 4096, d)?,
                )?),
                Operation::Store { request } => Request::Store(pages::StoreRequest::parse(
                    &bytes(request.as_deref(), 4096, d)?,
                )?),
                Operation::Remove { request } => Request::Remove(pages::RemoveRequest::parse(
                    &bytes(request.as_deref(), 4096, d)?,
                )?),
                Operation::Serve(args) => {
                    let mut config = service.ok_or_else(|| anyhow::anyhow!(INVALID))?;
                    if let Some(name) = &args.tls_credential {
                        let identity = yoke::host_credentials::read_tls(name)?;
                        config.tls = Some(yoke::federation_http::TlsMaterial {
                            certificates: vec![identity.certificate],
                            private_key: identity.private_key,
                        });
                    }
                    Request::Serve(Box::new(config))
                }
            };
        d.check()?;
        let host = app::Host::from_os(profile)?;
        d.check()?;
        Ok(Prepared {
            host,
            request,
            options,
            clock,
        })
    }
}
fn output(value: &serde_json::Value, cap: usize, d: &Deadline) -> Result<()> {
    let raw = yoke::json::encode_bounded(value, cap.saturating_sub(1), d)?;
    d.check()?;
    let mut stdout = std::io::stdout().lock();
    stdout.write_all(&raw)?;
    stdout.write_all(b"\n")?;
    stdout.flush()?;
    Ok(())
}
impl Prepared {
    pub fn run(self, engine: Engine, d: &Deadline) -> Result<bool> {
        if let Request::Serve(config) = self.request {
            let cap = self.options.max_output_bytes;
            let queue = self.options.worker.queue.clone();
            let handle = ingress::start_pages(
                engine,
                self.host,
                queue,
                *config,
                ingress::PageService {
                    allow_serving: true,
                    options: self.options,
                    clock: self.clock,
                },
                d,
            )?;
            output(handle.announcement(), cap, d)?;
            handle.wait()?;
            return Ok(true);
        }
        let context = pages::Context {
            store: &engine.store,
            host: &self.host,
            options: &self.options,
            clock: &self.clock,
        };
        let value = match &self.request {
            Request::Preview(request) => {
                pages::preview(&engine.store, &self.host, request, &self.options, d)?
            }
            Request::Store(request) => pages::store_page(&context, request, d)?,
            Request::Remove(request) => pages::remove_page(&context, request, d)?,
            Request::Serve(_) => unreachable!(),
        };
        output(&value, self.options.max_output_bytes, d)?;
        Ok(true)
    }
}
