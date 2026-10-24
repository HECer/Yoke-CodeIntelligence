//! Explicit local driver; every atomic batch rechecks grants and exact revision.
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
        HostProfile,
        digest::{
            self,
            history_jobs::{self, State},
        },
    },
    store::Store,
};
#[derive(Args)]
pub struct Arguments {
    #[arg(long)]
    profile: PathBuf,
    #[arg(long)]
    id: String,
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
        request: PathBuf,
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
        ensure!(
            self.allow_jobs
                && (read_only || (writes && self.allow_capture))
                && (!capture || self.trust_git),
            "github_app_digest_history_job_permission_required"
        );
        ensure!(
            (10..=600_000).contains(&timeout)
                && (1024..=4_000_000).contains(&max_bytes)
                && self.profile.is_absolute()
                && yoke::model::valid_id(&self.id)
                && (1..=16).contains(&self.max_batch_visits)
                && (1..=64).contains(&self.max_batch_objects)
                && (1..=2_000_000).contains(&self.max_batch_proof_bytes),
            "github_app_digest_history_job_input_invalid"
        );
        if let Operation::Run { max_batches, .. } = self.command {
            ensure!(
                (1..=128).contains(&max_batches),
                "github_app_digest_history_job_input_invalid"
            );
        }
        let d = Deadline::new(timeout);
        let read = |path: &Path, max| -> Result<Vec<u8>> {
            ensure!(
                path.is_absolute(),
                "github_app_digest_history_job_input_invalid"
            );
            crate::feedback_cli::host_bytes(path, max, &d)
                .map_err(|_| anyhow::anyhow!("github_app_digest_history_job_input_invalid"))
        };
        let profile = HostProfile::parse(&read(&self.profile, 32_000)?)
            .map_err(|_| anyhow::anyhow!("github_app_digest_history_job_input_invalid"))?;
        let options = history_jobs::Options {
            allow_jobs: self.allow_jobs,
            acquisition: digest::acquisition::Options {
                allow_capture: self.allow_capture,
                allow_writes: writes,
                max_output_bytes: max_bytes as usize,
                ..Default::default()
            },
            max_batch_visits: self.max_batch_visits,
            max_batch_objects: self.max_batch_objects,
            max_batch_proof_bytes: self.max_batch_proof_bytes,
        };
        let digest = digest::Options::default();
        let context = history_jobs::Context {
            profile: &profile,
            digest: &digest,
            options: &options,
        };
        // All host files and creation scope are admitted before Store setup.
        let creation = if let Operation::Create { request, plan } = &self.command {
            let request = digest::PreviewRequest::parse(&read(request, 4096)?)
                .map_err(|_| anyhow::anyhow!("github_app_digest_history_job_input_invalid"))?;
            digest::authorize_profile(&profile, &request)?;
            let plan: history_jobs::Plan = if let Some(path) = plan {
                yoke::json::parse(&read(path, 4096)?, 4096)
                    .and_then(yoke::json::decode)
                    .map_err(|_| anyhow::anyhow!("github_app_digest_history_job_input_invalid"))?
            } else {
                history_jobs::Plan::default()
            };
            plan.validate()?;
            Some((request, plan))
        } else {
            None
        };
        let git = if capture {
            Some(crate::selected_git(self.git.clone(), self.trust_git)?)
        } else {
            None
        };
        let value = if read_only {
            ensure!(
                !writes && !self.allow_capture && !self.trust_git && self.git.is_none(),
                "github_app_digest_history_job_input_invalid"
            );
            let store = Store::open_existing_read_only(root, &d)
                .map_err(|_| anyhow::anyhow!("github_app_digest_store_unavailable"))?;
            history_jobs::status(&store, &context, &self.id, &d)?
        } else {
            crate::validate_existing_batch_store(root, &d)
                .map_err(|_| anyhow::anyhow!("github_app_digest_store_unavailable"))?;
            let engine = Engine::open_with_timeout(root, d.remaining())
                .map_err(|_| anyhow::anyhow!("github_app_digest_store_unavailable"))?;
            match &self.command {
                Operation::Create { .. } => {
                    let (request, plan) = creation.as_ref().unwrap();
                    history_jobs::create(&engine, &context, &self.id, request, plan, &d)?
                }
                Operation::Step { revision } => history_jobs::step(
                    &engine,
                    &context,
                    &self.id,
                    *revision,
                    git.as_ref().unwrap(),
                    &d,
                )?,
                Operation::Run {
                    revision,
                    max_batches,
                } => {
                    let mut revision = *revision;
                    let mut last = None;
                    for _ in 0..*max_batches {
                        d.check()?;
                        let value = history_jobs::step(
                            &engine,
                            &context,
                            &self.id,
                            revision,
                            git.as_ref().unwrap(),
                            &d,
                        )?;
                        revision = value["revision"].as_u64().unwrap();
                        let done = value["state"] == "complete";
                        last = Some(value);
                        if done {
                            break;
                        }
                    }
                    last.unwrap()
                }
                Operation::Pause { revision } => history_jobs::transition(
                    &engine.store,
                    &context,
                    &self.id,
                    *revision,
                    State::Paused,
                    &d,
                )?,
                Operation::Resume { revision } => history_jobs::transition(
                    &engine.store,
                    &context,
                    &self.id,
                    *revision,
                    State::Active,
                    &d,
                )?,
                Operation::Cancel { revision } => history_jobs::transition(
                    &engine.store,
                    &context,
                    &self.id,
                    *revision,
                    State::Cancelled,
                    &d,
                )?,
                Operation::Status => unreachable!(),
            }
        };
        let mut raw = yoke::json::encode_bounded(&value, max_bytes as usize - 1, &d)?;
        raw.push(b'\n');
        d.check()?;
        std::io::stdout().lock().write_all(&raw)?;
        Ok(true)
    }
}
