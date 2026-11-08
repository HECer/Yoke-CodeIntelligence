//! Host-selected native workers. Permission and budget checks precede all host files.
use anyhow::{Result, ensure};
use clap::{Args, Subcommand};
use std::{
    io::Write,
    path::{Path, PathBuf},
};
use yoke::{
    Engine,
    capture::Deadline,
    github_app::{self as app, worker},
    github_app_auth as auth,
};

const INVALID: &str = "github_app_worker_input_invalid";
#[derive(Args)]
pub struct Arguments {
    #[command(subcommand)]
    operation: Operation,
    #[arg(long, global = true, default_value_t = 1000)]
    max_records: usize,
    #[arg(long, global = true, default_value_t = 64_000_000)]
    max_worker_store_bytes: usize,
    #[arg(long, global = true, default_value_t = 900)]
    max_lease_seconds: u64,
}
#[derive(Subcommand)]
enum Operation {
    Claim {
        #[arg(long)]
        request: Option<PathBuf>,
    },
    Release {
        #[arg(long)]
        claim: Option<String>,
        #[arg(long)]
        request_id: Option<String>,
    },
    Recover {
        #[arg(long)]
        claim: Option<String>,
    },
    Status {
        #[arg(long, default_value_t = 0)]
        offset: usize,
        #[arg(long, default_value_t = 100)]
        limit: usize,
    },
    Run {
        #[arg(long)]
        request: Option<PathBuf>,
        #[arg(long)]
        auth_profile: Option<PathBuf>,
        #[arg(long, default_value_t = 33)]
        max_requests: usize,
        #[arg(long, default_value_t = 1_000_000)]
        max_response_bytes: usize,
        #[arg(long, default_value_t = 8_000_000)]
        max_total_bytes: usize,
        #[arg(long, default_value_t = 3_000_000)]
        max_report_bytes: usize,
        #[arg(long, default_value_t = 3000)]
        max_files: usize,
        #[arg(long)]
        history_schedule: Option<PathBuf>,
        #[arg(long)]
        allow_app_history: bool,
        #[arg(long)]
        allow_history_jobs: bool,
        #[arg(long)]
        allow_history_capture: bool,
        #[arg(long)]
        trust_history_git: bool,
        #[arg(long)]
        history_git: Option<PathBuf>,
    },
}
struct HistoryPrepared {
    schedule: worker::history::Schedule,
    options: worker::history::Options,
    git: yoke::git_workflows::HostGit,
}
enum Input {
    Claim(worker::ClaimRequest),
    Release(String, String),
    Recover(String),
    Status(worker::StatusRequest),
    Run(
        worker::RunRequest,
        Box<auth::Client>,
        worker::RunOptions,
        Option<Box<HistoryPrepared>>,
    ),
}
enum Admitted {
    Local(Input),
    Run(
        worker::RunRequest,
        auth::AuthProfile,
        Option<Box<HistoryPrepared>>,
    ),
}
pub struct Prepared {
    host: app::Host,
    options: worker::Options,
    input: Input,
}
fn host_bytes(path: Option<&Path>, limit: usize, d: &Deadline) -> Result<Vec<u8>> {
    crate::feedback_cli::host_bytes(path.ok_or_else(|| anyhow::anyhow!(INVALID))?, limit, d)
        .map_err(|_| anyhow::anyhow!(INVALID))
}
fn uuid(id: &str) -> bool {
    id.len() == 36
        && id.bytes().enumerate().all(|(i, b)| {
            if [8, 13, 18, 23].contains(&i) {
                b == b'-'
            } else {
                b.is_ascii_digit() || (b'a'..=b'f').contains(&b)
            }
        })
}
impl Arguments {
    pub fn check_permission(&self, webhooks: bool, network: bool, writes: bool) -> Result<()> {
        ensure!(webhooks, "github_app_permission_required");
        let write = matches!(
            self.operation,
            Operation::Claim { .. } | Operation::Release { .. } | Operation::Run { .. }
        );
        ensure!(!write || writes, "github_app_permission_required");
        ensure!(write || !writes, INVALID);
        if matches!(self.operation, Operation::Run { .. }) {
            ensure!(network, "github_app_network_permission_required");
        } else {
            ensure!(!network, INVALID);
        }
        if let Operation::Run {
            history_schedule,
            allow_app_history,
            allow_history_jobs,
            allow_history_capture,
            trust_history_git,
            history_git,
            ..
        } = &self.operation
        {
            if history_schedule.is_some() {
                ensure!(
                    *allow_app_history
                        && *allow_history_jobs
                        && *allow_history_capture
                        && *trust_history_git,
                    "github_app_history_permission_required"
                );
            } else {
                ensure!(
                    !allow_app_history
                        && !allow_history_jobs
                        && !allow_history_capture
                        && !trust_history_git
                        && history_git.is_none(),
                    INVALID
                );
            }
        }
        Ok(())
    }
    pub fn prepare(
        &self,
        profile_path: Option<&Path>,
        queue: app::Options,
        network: bool,
        d: &Deadline,
    ) -> Result<Prepared> {
        self.check_permission(true, network, queue.allow_writes)?;
        // Worker CLI zero limits refuse before host files; ordinary queue semantics stay unchanged.
        ensure!(
            queue.max_payload_bytes > 0
                && queue.max_deliveries > 0
                && queue.max_jobs > 0
                && queue.max_transitions > 0
                && queue.max_store_bytes > 0,
            "github_app_worker_capacity_exceeded"
        );
        let write = matches!(
            self.operation,
            Operation::Claim { .. } | Operation::Release { .. } | Operation::Run { .. }
        );
        let options = worker::Options {
            max_records: self.max_records,
            max_store_bytes: self.max_worker_store_bytes,
            max_output_bytes: queue.max_output_bytes,
            max_lease_seconds: self.max_lease_seconds,
            queue,
        };
        options.admit(write, d)?;
        if let Operation::Run {
            history_schedule: Some(_),
            ..
        } = &self.operation
        {
            ensure!(
                options.max_output_bytes >= 4096,
                "github_app_history_capacity_exceeded"
            );
        }
        let remote = if let Operation::Run {
            max_requests,
            max_response_bytes,
            max_total_bytes,
            max_report_bytes,
            max_files,
            ..
        } = &self.operation
        {
            let limits = worker::RunOptions {
                allow_network: network,
                max_requests: *max_requests,
                max_response_bytes: *max_response_bytes,
                max_total_bytes: *max_total_bytes,
                max_report_bytes: *max_report_bytes,
                max_files: *max_files,
            };
            limits.admit(d)?;
            Some(limits)
        } else {
            None
        };
        // Validate all explicit input files before loading either credential.
        let profile = app::HostProfile::parse(&host_bytes(profile_path, 32_000, d)?)?;
        let input = match &self.operation {
            Operation::Claim { request } => {
                let request =
                    worker::ClaimRequest::parse(&host_bytes(request.as_deref(), 4096, d)?)?;
                ensure!(
                    request.lease_seconds <= options.max_lease_seconds,
                    "github_app_worker_capacity_exceeded"
                );
                Admitted::Local(Input::Claim(request))
            }
            Operation::Release { claim, request_id } => {
                let claim = claim.as_ref().ok_or_else(|| anyhow::anyhow!(INVALID))?;
                let request_id = request_id
                    .as_ref()
                    .ok_or_else(|| anyhow::anyhow!(INVALID))?;
                ensure!(yoke::model::valid_id(claim) && uuid(request_id), INVALID);
                Admitted::Local(Input::Release(claim.clone(), request_id.clone()))
            }
            Operation::Recover { claim } => {
                let claim = claim.as_ref().ok_or_else(|| anyhow::anyhow!(INVALID))?;
                ensure!(yoke::model::valid_id(claim), INVALID);
                Admitted::Local(Input::Recover(claim.clone()))
            }
            Operation::Status { offset, limit } => {
                ensure!(*offset <= 1000 && *limit <= 100, INVALID);
                Admitted::Local(Input::Status(worker::StatusRequest {
                    offset: *offset,
                    limit: *limit,
                }))
            }
            Operation::Run {
                request,
                auth_profile: path,
                history_schedule,
                history_git,
                ..
            } => {
                let request = worker::RunRequest::parse(&host_bytes(request.as_deref(), 4096, d)?)?;
                let profile = auth::AuthProfile::parse(&host_bytes(path.as_deref(), 32_000, d)?)?;
                let history = if let Some(path) = history_schedule {
                    let schedule =
                        worker::history::Schedule::parse(&host_bytes(Some(path), 4096, d)?)?;
                    let git = crate::selected_git(history_git.clone(), true)?;
                    Some(Box::new(HistoryPrepared {
                        schedule,
                        git,
                        options: worker::history::Options {
                            allow_app_history: true,
                            worker: options.clone(),
                            digest: app::digest::Options::default(),
                            jobs: app::digest::history_jobs::Options {
                                allow_jobs: true,
                                acquisition: app::digest::acquisition::Options {
                                    allow_capture: true,
                                    allow_writes: options.queue.allow_writes,
                                    max_output_bytes: options.max_output_bytes,
                                    ..Default::default()
                                },
                                ..Default::default()
                            },
                        },
                    }))
                } else {
                    None
                };
                Admitted::Run(request, profile, history)
            }
        };
        let host = app::Host::from_os(profile)?;
        let input = match input {
            Admitted::Run(request, profile, history) => {
                let client = auth::Client::production(
                    profile,
                    auth::Options {
                        allow_network: network,
                        ..Default::default()
                    },
                    d,
                )?;
                Input::Run(
                    request,
                    Box::new(client),
                    remote.ok_or_else(|| anyhow::anyhow!(INVALID))?,
                    history,
                )
            }
            Admitted::Local(input) => input,
        };
        d.check()?;
        Ok(Prepared {
            host,
            options,
            input,
        })
    }
}
impl Prepared {
    pub fn run(mut self, engine: &Engine, d: &Deadline) -> Result<bool> {
        let clock = worker::Clock::system();
        let value = match &mut self.input {
            Input::Claim(request) => {
                worker::claim(&engine.store, &self.host, request, &self.options, &clock, d)?
                    .evidence()
                    .clone()
            }
            Input::Release(id, request_id) => {
                let claim =
                    worker::recover(&engine.store, &self.host, id, &self.options, &clock, d)?;
                worker::release(
                    &engine.store,
                    &self.host,
                    &claim,
                    request_id,
                    &self.options,
                    &clock,
                    d,
                )?
            }
            Input::Recover(id) => {
                worker::recover(&engine.store, &self.host, id, &self.options, &clock, d)?
                    .evidence()
                    .clone()
            }
            Input::Status(request) => {
                worker::status(&engine.store, &self.host, request, &self.options, &clock, d)?
            }
            Input::Run(request, client, remote, Some(history)) => worker::history::execute(
                &mut worker::history::Execution {
                    context: worker::history::WriteContext {
                        engine,
                        host: &self.host,
                        options: &history.options,
                    },
                    client,
                    request,
                    remote,
                    clock: &clock,
                    git: &history.git,
                },
                &history.schedule,
                d,
            )?,
            Input::Run(request, client, remote, None) => worker::run(
                &worker::RunContext {
                    store: &engine.store,
                    host: &self.host,
                    options: &self.options,
                    clock: &clock,
                },
                client,
                request,
                remote,
                d,
            )?,
        };
        let bytes =
            yoke::json::encode_bounded(&value, self.options.max_output_bytes.saturating_sub(1), d)?;
        d.check()?;
        let mut out = std::io::stdout().lock();
        out.write_all(&bytes)?;
        out.write_all(b"\n")?;
        out.flush()?;
        Ok(true)
    }
}
