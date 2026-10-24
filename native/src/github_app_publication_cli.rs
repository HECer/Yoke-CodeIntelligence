//! Explicit publication roles; admission precedes host files, keys and Store opening.
use anyhow::{Result, ensure};
use clap::{Args, Subcommand};
use std::{
    io::Write,
    path::{Path, PathBuf},
};
use yoke::{
    Engine,
    capture::Deadline,
    github_app::{self as app, publication, worker},
    github_app_auth as auth,
};

const INVALID: &str = "github_app_publication_input_invalid";
#[derive(Args)]
pub struct Arguments {
    #[command(subcommand)]
    operation: Operation,
    #[arg(long, global = true, default_value_t = 48_000)]
    max_body_bytes: usize,
    #[arg(long, global = true, default_value_t = 1000)]
    max_records: usize,
    #[arg(long, global = true, default_value_t = 64_000_000)]
    max_worker_store_bytes: usize,
    #[arg(long, global = true)]
    allow_publication: bool,
    #[arg(long, global = true)]
    allow_page_links: bool,
    #[arg(long, global = true, default_value_t = 1000)]
    max_journal_records: usize,
    #[arg(long, global = true, default_value_t = 64_000_000)]
    max_journal_bytes: usize,
}
#[derive(Subcommand)]
enum Operation {
    Preview {
        #[arg(long)]
        request: Option<PathBuf>,
    },
    Status {
        #[arg(long)]
        request: Option<PathBuf>,
    },
    Publish(Remote),
    Recover(Remote),
}
#[derive(Args)]
struct Remote {
    #[arg(long)]
    request: Option<PathBuf>,
    #[arg(long)]
    auth_profile: Option<PathBuf>,
    #[arg(long, default_value_t = 35)]
    max_requests: usize,
    #[arg(long, default_value_t = 1_000_000)]
    max_response_bytes: usize,
    #[arg(long, default_value_t = 8_000_000)]
    max_total_bytes: usize,
}
enum Request {
    Preview(publication::PreviewRequest),
    Status(publication::StatusRequest),
    Publish(publication::PublishRequest),
    Recover(publication::RecoverRequest),
}
pub struct Prepared {
    host: app::Host,
    request: Request,
    client: Option<Box<auth::Client>>,
    remote: Option<publication::PublishOptions>,
    options: publication::Options,
}
fn bytes(path: Option<&Path>, limit: usize, d: &Deadline) -> Result<Vec<u8>> {
    crate::feedback_cli::host_bytes(path.ok_or_else(|| anyhow::anyhow!(INVALID))?, limit, d)
        .map_err(|_| anyhow::anyhow!(INVALID))
}
impl Arguments {
    pub fn check_permission(&self, webhooks: bool, network: bool, writes: bool) -> Result<()> {
        ensure!(webhooks, "github_app_permission_required");
        ensure!(
            !self.allow_page_links
                || !matches!(
                    self.operation,
                    Operation::Status { .. } | Operation::Recover(_)
                ),
            INVALID
        );
        match self.operation {
            Operation::Preview { .. } | Operation::Status { .. } => {
                ensure!(!network && !writes && !self.allow_publication, INVALID);
            }
            Operation::Publish(_) => {
                ensure!(
                    self.allow_publication,
                    "github_app_publication_permission_required"
                );
                ensure!(writes, "github_app_permission_required");
                ensure!(network, "github_app_network_permission_required");
            }
            Operation::Recover(_) => {
                ensure!(!self.allow_publication, INVALID);
                ensure!(writes, "github_app_permission_required");
                ensure!(network, "github_app_network_permission_required");
            }
        }
        Ok(())
    }
    pub fn prepare(
        &self,
        profile: Option<&Path>,
        queue: app::Options,
        network: bool,
        d: &Deadline,
    ) -> Result<Prepared> {
        self.check_permission(true, network, queue.allow_writes)?;
        ensure!(
            queue.max_payload_bytes > 0
                && queue.max_deliveries > 0
                && queue.max_jobs > 0
                && queue.max_transitions > 0
                && queue.max_store_bytes > 0
                && queue.max_output_bytes > 0
                && self.max_records > 0
                && self.max_worker_store_bytes > 0
                && self.max_journal_records > 0
                && self.max_journal_bytes > 0
                && self.max_body_bytes > 0,
            "github_app_publication_capacity_exceeded"
        );
        let options = publication::Options {
            allow_publication: self.allow_publication,
            allow_page_links: self.allow_page_links,
            max_body_bytes: self.max_body_bytes,
            max_output_bytes: queue.max_output_bytes,
            max_journal_records: self.max_journal_records,
            max_journal_bytes: self.max_journal_bytes,
            worker: worker::Options {
                max_records: self.max_records,
                max_store_bytes: self.max_worker_store_bytes,
                max_output_bytes: queue.max_output_bytes,
                queue,
                ..Default::default()
            },
        };
        options.admit(d)?;
        let remote = match &self.operation {
            Operation::Publish(args) | Operation::Recover(args) => {
                let remote = publication::PublishOptions {
                    allow_network: network,
                    max_requests: args.max_requests,
                    max_response_bytes: args.max_response_bytes,
                    max_total_bytes: args.max_total_bytes,
                };
                remote.admit(d)?;
                Some(remote)
            }
            _ => None,
        };
        // Validate both bounded explicit host documents before looking up any credential.
        let profile = app::HostProfile::parse(&bytes(profile, 32_000, d)?)
            .map_err(|_| anyhow::anyhow!(INVALID))?;
        let request = match &self.operation {
            Operation::Preview { request } => Request::Preview(
                publication::PreviewRequest::parse(&bytes(request.as_deref(), 4096, d)?)
                    .map_err(|_| anyhow::anyhow!(INVALID))?,
            ),
            Operation::Status { request } => Request::Status(
                publication::StatusRequest::parse(&bytes(request.as_deref(), 4096, d)?)
                    .map_err(|_| anyhow::anyhow!(INVALID))?,
            ),
            Operation::Publish(args) => Request::Publish(
                publication::PublishRequest::parse(&bytes(args.request.as_deref(), 4096, d)?)
                    .map_err(|_| anyhow::anyhow!(INVALID))?,
            ),
            Operation::Recover(args) => Request::Recover(
                publication::RecoverRequest::parse(&bytes(args.request.as_deref(), 4096, d)?)
                    .map_err(|_| anyhow::anyhow!(INVALID))?,
            ),
        };
        if let Request::Preview(request) = &request {
            ensure!(
                request.page.is_none() || self.allow_page_links,
                "github_app_page_links_permission_required"
            );
        }
        let auth_profile = match &self.operation {
            Operation::Publish(args) | Operation::Recover(args) => {
                let auth =
                    auth::AuthProfile::parse(&bytes(args.auth_profile.as_deref(), 32_000, d)?)
                        .map_err(|_| anyhow::anyhow!(INVALID))?;
                publication::admit_profiles(
                    &profile,
                    &auth,
                    matches!(self.operation, Operation::Publish(_)),
                    d,
                )?;
                Some(auth)
            }
            _ => None,
        };
        d.check()?;
        let host = app::Host::from_os(profile)?;
        let client = auth_profile
            .map(|profile| {
                auth::Client::production(
                    profile,
                    auth::Options {
                        allow_network: network,
                        ..Default::default()
                    },
                    d,
                )
                .map(Box::new)
            })
            .transpose()?;
        d.check()?;
        Ok(Prepared {
            host,
            request,
            client,
            remote,
            options,
        })
    }
}
impl Prepared {
    pub fn run(mut self, engine: &Engine, d: &Deadline) -> Result<bool> {
        let clock = worker::Clock::system();
        let context = publication::Context {
            store: &engine.store,
            host: &self.host,
            options: &self.options,
            clock: &clock,
        };
        let value = match &self.request {
            Request::Preview(request) => publication::preview_with_context(&context, request, d)?,
            Request::Status(request) => publication::status(&context, request, d)?,
            Request::Publish(request) => publication::publish(
                &context,
                self.client
                    .as_deref_mut()
                    .ok_or_else(|| anyhow::anyhow!(INVALID))?,
                request,
                self.remote
                    .as_ref()
                    .ok_or_else(|| anyhow::anyhow!(INVALID))?,
                d,
            )?,
            Request::Recover(request) => publication::recover(
                &context,
                self.client
                    .as_deref_mut()
                    .ok_or_else(|| anyhow::anyhow!(INVALID))?,
                request,
                self.remote
                    .as_ref()
                    .ok_or_else(|| anyhow::anyhow!(INVALID))?,
                d,
            )?,
        };
        let raw = yoke::json::encode_bounded(&value, self.options.max_output_bytes - 1, d)?;
        d.check()?;
        let mut stdout = std::io::stdout().lock();
        stdout.write_all(&raw)?;
        stdout.write_all(b"\n")?;
        stdout.flush()?;
        Ok(true)
    }
}
