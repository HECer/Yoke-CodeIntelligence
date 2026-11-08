//! Explicit App-completion driver; host permission precedes files and keys.
use anyhow::{Result, ensure};
use clap::{Args, Subcommand};
use std::{
    io::Write,
    path::{Path, PathBuf},
};
use yoke::{
    Engine,
    capture::Deadline,
    github_app::{
        self as app,
        digest::{self, history_jobs as jobs},
        worker::{self, history},
    },
    store::Store,
};
const INVALID: &str = "github_app_history_input_invalid";
#[derive(Args)]
pub struct Arguments {
    #[arg(long)]
    profile: PathBuf,
    #[arg(long)]
    selection: PathBuf,
    #[arg(long)]
    allow_app_history: bool,
    #[arg(long)]
    allow_jobs: bool,
    #[arg(long)]
    allow_capture: bool,
    #[arg(long)]
    trust_git: bool,
    #[arg(long)]
    git: Option<PathBuf>,
    #[arg(long, default_value_t = 8)]
    max_batch_visits: usize,
    #[arg(long, default_value_t = 32)]
    max_batch_objects: usize,
    #[arg(long, default_value_t = 2_000_000)]
    max_batch_proof_bytes: usize,
    #[command(subcommand)]
    command: Operation,
}
#[derive(Subcommand)]
enum Operation {
    Create {
        #[arg(long)]
        plan: Option<PathBuf>,
    },
    Status,
    Step {
        #[arg(long)]
        revision: u64,
    },
    Run {
        #[arg(long)]
        revision: u64,
        #[arg(long, default_value_t = 8)]
        max_batches: usize,
    },
    Pause {
        #[arg(long)]
        revision: u64,
    },
    Resume {
        #[arg(long)]
        revision: u64,
    },
    Cancel {
        #[arg(long)]
        revision: u64,
    },
}
impl Arguments {
    pub fn run(&self, root: &Path, writes: bool, timeout: u64, max_bytes: u64) -> Result<bool> {
        let read_only = matches!(self.command, Operation::Status);
        let capture = matches!(self.command, Operation::Step { .. } | Operation::Run { .. });
        let current = matches!(
            self.command,
            Operation::Create { .. }
                | Operation::Step { .. }
                | Operation::Run { .. }
                | Operation::Resume { .. }
        );
        ensure!(
            self.allow_app_history
                && self.allow_jobs
                && (read_only || (writes && self.allow_capture))
                && (!capture || self.trust_git),
            "github_app_history_permission_required"
        );
        ensure!(
            !read_only || (!writes && !self.allow_capture && !self.trust_git && self.git.is_none()),
            INVALID
        );
        ensure!(capture || (!self.trust_git && self.git.is_none()), INVALID);
        ensure!(
            (10..=600_000).contains(&timeout)
                && (1024..=4_000_000).contains(&max_bytes)
                && self.profile.is_absolute()
                && self.selection.is_absolute()
                && (1..=16).contains(&self.max_batch_visits)
                && (1..=64).contains(&self.max_batch_objects)
                && (1..=2_000_000).contains(&self.max_batch_proof_bytes),
            INVALID
        );
        if let Operation::Run { max_batches, .. } = self.command {
            ensure!((1..=128).contains(&max_batches), INVALID);
        }
        let d = Deadline::new(timeout);
        let read = |path: &Path, max| -> Result<Vec<u8>> {
            ensure!(path.is_absolute(), INVALID);
            crate::feedback_cli::host_bytes(path, max, &d).map_err(|_| anyhow::anyhow!(INVALID))
        };
        let profile = app::HostProfile::parse(&read(&self.profile, 32_000)?)
            .map_err(|_| anyhow::anyhow!(INVALID))?;
        let selection = history::Selection::parse(&read(&self.selection, 4096)?)?;
        ensure!(
            !current || selection.expected_queue_revision.is_some(),
            INVALID
        );
        let plan = if let Operation::Create { plan } = &self.command {
            let value: jobs::Plan = if let Some(path) = plan {
                yoke::json::parse(&read(path, 4096)?, 4096)
                    .and_then(yoke::json::decode)
                    .map_err(|_| anyhow::anyhow!(INVALID))?
            } else {
                jobs::Plan::default()
            };
            value.validate()?;
            Some(value)
        } else {
            None
        };
        let git = if capture {
            Some(crate::selected_git(self.git.clone(), self.trust_git)?)
        } else {
            None
        };
        let options = history::Options {
            allow_app_history: self.allow_app_history,
            worker: worker::Options {
                queue: app::Options {
                    allow_writes: writes,
                    max_output_bytes: max_bytes as usize,
                    ..Default::default()
                },
                max_output_bytes: max_bytes as usize,
                ..Default::default()
            },
            digest: digest::Options::default(),
            jobs: jobs::Options {
                allow_jobs: self.allow_jobs,
                acquisition: digest::acquisition::Options {
                    allow_writes: writes,
                    allow_capture: self.allow_capture,
                    max_output_bytes: max_bytes as usize,
                    ..Default::default()
                },
                max_batch_visits: self.max_batch_visits,
                max_batch_objects: self.max_batch_objects,
                max_batch_proof_bytes: self.max_batch_proof_bytes,
            },
        };
        let host = app::Host::from_os(profile)?;
        let value = if read_only {
            let store = Store::open_existing_read_only(root, &d)
                .map_err(|_| anyhow::anyhow!("github_app_digest_store_unavailable"))?;
            history::status(
                &history::Context {
                    store: &store,
                    host: &host,
                    options: &options,
                },
                &selection,
                &d,
            )?
        } else {
            crate::validate_existing_batch_store(root, &d)
                .map_err(|_| anyhow::anyhow!("github_app_digest_store_unavailable"))?;
            let engine = Engine::open_with_timeout(root, d.remaining())
                .map_err(|_| anyhow::anyhow!("github_app_digest_store_unavailable"))?;
            let write = history::WriteContext {
                engine: &engine,
                host: &host,
                options: &options,
            };
            let read = history::Context {
                store: &engine.store,
                host: &host,
                options: &options,
            };
            match &self.command {
                Operation::Create { .. } => {
                    history::create(&write, &selection, plan.as_ref().unwrap(), &d)?
                }
                Operation::Step { revision } => {
                    history::step(&write, &selection, *revision, git.as_ref().unwrap(), &d)?
                }
                Operation::Run {
                    revision,
                    max_batches,
                } => history::run(
                    &write,
                    &selection,
                    *revision,
                    *max_batches,
                    git.as_ref().unwrap(),
                    &d,
                )?,
                Operation::Pause { revision } => {
                    history::transition(&read, &selection, *revision, jobs::State::Paused, &d)?
                }
                Operation::Resume { revision } => {
                    history::transition(&read, &selection, *revision, jobs::State::Active, &d)?
                }
                Operation::Cancel { revision } => {
                    history::transition(&read, &selection, *revision, jobs::State::Cancelled, &d)?
                }
                Operation::Status => unreachable!(),
            }
        };
        let mut bytes = yoke::json::encode_bounded(&value, max_bytes as usize - 1, &d)?;
        bytes.push(b'\n');
        d.check()?;
        std::io::stdout().lock().write_all(&bytes)?;
        Ok(true)
    }
}
