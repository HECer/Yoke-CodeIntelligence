//! Permission admission precedes host files, credentials and the source Store.
use anyhow::{Result, ensure};
use clap::{Args, Subcommand};
use std::{
    io::Write,
    path::{Path, PathBuf},
};
use yoke::{
    Engine,
    capture::Deadline,
    github_app::{self as app, Headers, Host, HostProfile, Options},
};

#[derive(Args)]
pub struct Arguments {
    #[command(subcommand)]
    operation: Operation,
    #[arg(long, global = true)]
    allow_webhooks: bool,
    #[arg(long, global = true)]
    allow_github: bool,
    #[arg(long, global = true)]
    profile: Option<PathBuf>,
    #[arg(long, global = true, default_value_t = 512_000)]
    max_payload_bytes: usize,
    #[arg(long, global = true, default_value_t = 1000)]
    max_deliveries: usize,
    #[arg(long, global = true, default_value_t = 1000)]
    max_jobs: usize,
    #[arg(long, global = true, default_value_t = 1000)]
    max_transitions: usize,
    #[arg(long, global = true, default_value_t = 64_000_000)]
    max_store_bytes: usize,
}
#[derive(Subcommand)]
enum Operation {
    Digest(crate::github_app_digest_cli::Arguments),
    Pages(crate::github_app_pages_cli::Arguments),
    Publication(crate::github_app_publication_cli::Arguments),
    Worker(crate::github_app_worker_cli::Arguments),
    Serve(crate::github_app_ingress_cli::Arguments),
    Authorize {
        #[arg(long)]
        installation: Option<u64>,
        #[arg(long)]
        repository: Option<u64>,
        #[arg(long, default_value_t = 4)]
        max_requests: usize,
        #[arg(long, default_value_t = 1_000_000)]
        max_response_bytes: usize,
        #[arg(long, default_value_t = 4_000_000)]
        max_total_bytes: usize,
    },
    Ingest {
        #[arg(long)]
        delivery: PathBuf,
        #[arg(long)]
        payload: PathBuf,
    },
    Queue {
        #[arg(long, default_value_t = 0)]
        offset: usize,
        #[arg(long, default_value_t = 100)]
        limit: usize,
    },
    Select {
        #[arg(long)]
        job: String,
        #[arg(long)]
        expected_revision: usize,
    },
}
enum Input {
    Ingest(Headers, Vec<u8>),
    Queue(usize, usize),
    Select(String, usize),
}
pub struct Prepared {
    host: Host,
    input: Input,
    options: Options,
}
fn bytes(path: &Path, limit: usize, d: &Deadline) -> Result<Vec<u8>> {
    // Same strict host-path admission as explicit feedback input, no repository discovery.
    crate::feedback_cli::host_bytes(path, limit, d)
        .map_err(|_| anyhow::anyhow!("github_app_input_invalid"))
}
impl Arguments {
    pub fn check_permission(&self, allow_writes: bool) -> Result<()> {
        if let Operation::Digest(args) = &self.operation {
            args.check_permission(self.allow_webhooks, self.allow_github, allow_writes)?;
            // Queue-only settings do not configure a local captured-source read.
            ensure!(
                self.max_payload_bytes == 512_000
                    && self.max_deliveries == 1000
                    && self.max_jobs == 1000
                    && self.max_transitions == 1000
                    && self.max_store_bytes == app::MAX_STORE_BYTES,
                "github_app_digest_input_invalid"
            );
            return Ok(());
        }
        if let Operation::Pages(args) = &self.operation {
            return args.check_permission(self.allow_webhooks, self.allow_github, allow_writes);
        }
        if let Operation::Publication(args) = &self.operation {
            return args.check_permission(self.allow_webhooks, self.allow_github, allow_writes);
        }
        if let Operation::Worker(args) = &self.operation {
            return args.check_permission(self.allow_webhooks, self.allow_github, allow_writes);
        }
        if matches!(self.operation, Operation::Authorize { .. }) {
            ensure!(self.allow_github, "github_app_network_permission_required");
            ensure!(
                !self.allow_webhooks && !allow_writes,
                "github_app_auth_input_invalid"
            );
            return Ok(());
        }
        ensure!(!self.allow_github, "github_app_input_invalid");
        ensure!(self.allow_webhooks, "github_app_permission_required");
        let write = matches!(
            self.operation,
            Operation::Ingest { .. } | Operation::Select { .. } | Operation::Serve(_)
        );
        ensure!(!write || allow_writes, "github_app_permission_required");
        ensure!(write || !allow_writes, "github_app_input_invalid");
        Ok(())
    }
    pub fn prepare(&self, max_bytes: u64, allow_writes: bool, d: &Deadline) -> Result<Prepared> {
        self.check_permission(allow_writes)?;
        ensure!(
            max_bytes <= 4_000_000
                && self.max_payload_bytes <= app::MAX_PAYLOAD_BYTES
                && self.max_deliveries <= 1000
                && self.max_jobs <= 1000
                && self.max_transitions <= 1000
                && self.max_store_bytes <= app::MAX_STORE_BYTES,
            "github_app_input_invalid"
        );
        let options = Options {
            allow_writes,
            max_payload_bytes: self.max_payload_bytes,
            max_deliveries: self.max_deliveries,
            max_jobs: self.max_jobs,
            max_transitions: self.max_transitions,
            max_store_bytes: self.max_store_bytes,
            max_output_bytes: max_bytes as usize,
        };
        let profile_path = self
            .profile
            .as_deref()
            .ok_or_else(|| anyhow::anyhow!("github_app_input_invalid"))?;
        let profile = HostProfile::parse(&bytes(profile_path, 32_000, d)?)
            .map_err(|_| anyhow::anyhow!("github_app_input_invalid"))?;
        let input = match &self.operation {
            Operation::Digest(_) => anyhow::bail!("github_app_digest_input_invalid"),
            Operation::Pages(_) => anyhow::bail!("github_app_pages_input_invalid"),
            Operation::Publication(_) => anyhow::bail!("github_app_publication_input_invalid"),
            Operation::Worker(_) => anyhow::bail!("github_app_worker_input_invalid"),
            Operation::Serve(_) => anyhow::bail!("github_app_ingress_input_invalid"),
            Operation::Authorize { .. } => anyhow::bail!("github_app_auth_input_invalid"),
            Operation::Ingest { delivery, payload } => Input::Ingest(
                Headers::parse(&bytes(delivery, 2000, d)?)?,
                bytes(payload, self.max_payload_bytes, d)?,
            ),
            Operation::Queue { offset, limit } => {
                ensure!(*offset <= 1000 && *limit <= 100, "github_app_input_invalid");
                Input::Queue(*offset, *limit)
            }
            Operation::Select {
                job,
                expected_revision,
            } => {
                ensure!(
                    yoke::model::valid_id(job) && *expected_revision <= 2000,
                    "github_app_input_invalid"
                );
                Input::Select(job.clone(), *expected_revision)
            }
        };
        d.check()?;
        let host = Host::from_os(profile)?;
        if let Input::Ingest(headers, raw) = &input {
            host.preflight_delivery(headers, raw, &options, d)?;
        }
        d.check()?;
        Ok(Prepared {
            host,
            input,
            options,
        })
    }
    pub fn is_authorize(&self) -> bool {
        matches!(self.operation, Operation::Authorize { .. })
    }
    pub fn is_digest(&self) -> bool {
        matches!(self.operation, Operation::Digest(_))
    }
    pub fn prepare_digest(
        &self,
        max_bytes: u64,
        allow_writes: bool,
        d: &Deadline,
    ) -> Result<crate::github_app_digest_cli::Prepared> {
        self.check_permission(allow_writes)?;
        let Operation::Digest(args) = &self.operation else {
            anyhow::bail!("github_app_digest_input_invalid")
        };
        args.prepare(self.profile.as_deref(), max_bytes, d)
    }
    pub fn is_worker(&self) -> bool {
        matches!(self.operation, Operation::Worker(_))
    }
    pub fn is_publication(&self) -> bool {
        matches!(self.operation, Operation::Publication(_))
    }
    pub fn prepare_publication(
        &self,
        max_bytes: u64,
        allow_writes: bool,
        d: &Deadline,
    ) -> Result<crate::github_app_publication_cli::Prepared> {
        self.check_permission(allow_writes)?;
        let Operation::Publication(args) = &self.operation else {
            anyhow::bail!("github_app_publication_input_invalid")
        };
        let queue = Options {
            allow_writes,
            max_payload_bytes: self.max_payload_bytes,
            max_deliveries: self.max_deliveries,
            max_jobs: self.max_jobs,
            max_transitions: self.max_transitions,
            max_store_bytes: self.max_store_bytes,
            max_output_bytes: usize::try_from(max_bytes)
                .map_err(|_| anyhow::anyhow!("github_app_publication_input_invalid"))?,
        };
        args.prepare(self.profile.as_deref(), queue, self.allow_github, d)
    }
    pub fn prepare_worker(
        &self,
        max_bytes: u64,
        allow_writes: bool,
        d: &Deadline,
    ) -> Result<crate::github_app_worker_cli::Prepared> {
        self.check_permission(allow_writes)?;
        let Operation::Worker(args) = &self.operation else {
            anyhow::bail!("github_app_worker_input_invalid")
        };
        let queue = Options {
            allow_writes,
            max_payload_bytes: self.max_payload_bytes,
            max_deliveries: self.max_deliveries,
            max_jobs: self.max_jobs,
            max_transitions: self.max_transitions,
            max_store_bytes: self.max_store_bytes,
            max_output_bytes: usize::try_from(max_bytes)
                .map_err(|_| anyhow::anyhow!("github_app_worker_input_invalid"))?,
        };
        args.prepare(self.profile.as_deref(), queue, self.allow_github, d)
    }
    pub fn is_serve(&self) -> bool {
        matches!(self.operation, Operation::Serve(_))
            || matches!(&self.operation, Operation::Pages(args) if args.is_serve())
    }
    pub fn is_pages(&self) -> bool {
        matches!(self.operation, Operation::Pages(_))
    }
    pub fn prepare_pages(
        &self,
        max_bytes: u64,
        allow_writes: bool,
        timeout: u64,
        d: &Deadline,
    ) -> Result<crate::github_app_pages_cli::Prepared> {
        self.check_permission(allow_writes)?;
        let Operation::Pages(args) = &self.operation else {
            anyhow::bail!("github_app_pages_input_invalid")
        };
        let queue = Options {
            allow_writes,
            max_payload_bytes: self.max_payload_bytes,
            max_deliveries: self.max_deliveries,
            max_jobs: self.max_jobs,
            max_transitions: self.max_transitions,
            max_store_bytes: self.max_store_bytes,
            max_output_bytes: usize::try_from(max_bytes)
                .map_err(|_| anyhow::anyhow!("github_app_pages_input_invalid"))?,
        };
        args.prepare(
            self.profile.as_deref(),
            queue,
            self.allow_webhooks,
            self.allow_github,
            timeout,
            d,
        )
    }
    pub fn prepare_service(
        &self,
        max_bytes: u64,
        allow_writes: bool,
        timeout: u64,
        d: &Deadline,
    ) -> Result<crate::github_app_ingress_cli::Prepared> {
        self.check_permission(allow_writes)?;
        let Operation::Serve(args) = &self.operation else {
            anyhow::bail!("github_app_ingress_input_invalid");
        };
        let options = Options {
            allow_writes,
            max_payload_bytes: self.max_payload_bytes,
            max_deliveries: self.max_deliveries,
            max_jobs: self.max_jobs,
            max_transitions: self.max_transitions,
            max_store_bytes: self.max_store_bytes,
            max_output_bytes: usize::try_from(max_bytes)
                .map_err(|_| anyhow::anyhow!("github_app_ingress_input_invalid"))?,
        };
        args.prepare(
            self.profile.as_deref(),
            self.allow_webhooks,
            options,
            timeout,
            d,
        )
    }
    pub fn run_authorize(&self, max_bytes: u64, d: &Deadline) -> Result<bool> {
        use yoke::github_app_auth::{AuthProfile, Client, Options as AuthOptions};
        let Operation::Authorize {
            installation,
            repository,
            max_requests,
            max_response_bytes,
            max_total_bytes,
        } = &self.operation
        else {
            anyhow::bail!("github_app_auth_input_invalid");
        };
        ensure!(
            self.max_payload_bytes == 512_000
                && self.max_deliveries == 1000
                && self.max_jobs == 1000
                && self.max_transitions == 1000
                && self.max_store_bytes == 64_000_000
                && max_bytes <= 4_000_000,
            "github_app_auth_input_invalid"
        );
        let options = AuthOptions {
            allow_network: self.allow_github,
            max_requests: *max_requests,
            max_response_bytes: *max_response_bytes,
            max_total_bytes: *max_total_bytes,
            max_output_bytes: max_bytes as usize,
        };
        options.admit(d)?;
        let installation =
            installation.ok_or_else(|| anyhow::anyhow!("github_app_auth_input_invalid"))?;
        let repository =
            repository.ok_or_else(|| anyhow::anyhow!("github_app_auth_input_invalid"))?;
        let path = self
            .profile
            .as_deref()
            .ok_or_else(|| anyhow::anyhow!("github_app_auth_input_invalid"))?;
        let profile = AuthProfile::parse(&bytes(path, 32_000, d)?)
            .map_err(|_| anyhow::anyhow!("github_app_auth_input_invalid"))?;
        let mut client = Client::production(profile, options, d)?;
        let lease = client.authorize(installation, repository, d)?;
        let raw =
            yoke::json::encode_bounded(lease.evidence(), max_bytes.saturating_sub(1) as usize, d)?;
        d.check()?;
        let mut stdout = std::io::stdout().lock();
        stdout.write_all(&raw)?;
        stdout.write_all(b"\n")?;
        stdout.flush()?;
        Ok(true)
    }
}
impl Prepared {
    pub fn run(&self, engine: &Engine, d: &Deadline) -> Result<bool> {
        let value = match &self.input {
            Input::Ingest(headers, raw) => {
                app::ingest(&engine.store, &self.host, headers, raw, &self.options, d)?
            }
            Input::Queue(offset, limit) => {
                app::queue(&engine.store, &self.host, *offset, *limit, &self.options, d)?
            }
            Input::Select(job, revision) => {
                app::select(&engine.store, &self.host, job, *revision, &self.options, d)?
            }
        };
        let bytes =
            yoke::json::encode_bounded(&value, self.options.max_output_bytes.saturating_sub(1), d)?;
        d.check()?;
        let mut stdout = std::io::stdout().lock();
        stdout.write_all(&bytes)?;
        stdout.write_all(b"\n")?;
        stdout.flush()?;
        Ok(true)
    }
}
