//! Explicit host-authorized atomic history capture; no credential/model/source execution.
use anyhow::{Result, ensure};
use clap::Args;
use std::{
    io::Write,
    path::{Path, PathBuf},
};
use yoke::{
    Engine,
    capture::Deadline,
    github_app::{
        HostProfile,
        digest::{self, acquisition},
    },
};
#[derive(Args)]
pub struct Arguments {
    #[arg(long)]
    profile: PathBuf,
    #[arg(long)]
    request: PathBuf,
    #[arg(long)]
    allow_capture: bool,
    #[arg(long)]
    allow_history: bool,
    #[arg(long)]
    trust_git: bool,
    #[arg(long)]
    git: Option<PathBuf>,
    #[arg(long, default_value_t = 16)]
    max_commits: usize,
    #[arg(long, default_value_t = 4000)]
    max_visits: usize,
    #[arg(long, default_value_t = 4000)]
    max_frontier: usize,
    #[arg(long, default_value_t = 16000)]
    max_parent_ids: usize,
    #[arg(long, default_value_t = 1_000_000)]
    max_commit_bytes: usize,
    #[arg(long, default_value_t = 64_000_000)]
    max_history_bytes: usize,
    #[arg(long, default_value_t = 32)]
    max_snapshots: usize,
    #[arg(long, default_value_t = 10_000)]
    max_files: usize,
    #[arg(long, default_value_t = 32_000_000)]
    max_source_bytes: usize,
    #[arg(long, default_value_t = 16_000_000)]
    max_capture_commit_bytes: usize,
    #[arg(long, default_value_t = 16_000_000)]
    max_tree_bytes: usize,
    #[arg(long, default_value_t = 100_000)]
    max_tree_entries: usize,
}
impl Arguments {
    pub fn run(&self, root: &Path, writes: bool, timeout: u64, max_bytes: u64) -> Result<bool> {
        ensure!(
            self.allow_capture && self.allow_history && self.trust_git && writes,
            "github_app_digest_acquisition_permission_required"
        );
        ensure!(
            (10..=600_000).contains(&timeout) && max_bytes <= 4_000_000,
            "github_app_digest_input_invalid"
        );
        let d = Deadline::new(timeout);
        let options = acquisition::Options {
            allow_capture: self.allow_capture,
            allow_writes: writes,
            max_snapshots: self.max_snapshots,
            max_files: self.max_files,
            max_source_bytes: self.max_source_bytes,
            max_commit_bytes: self.max_capture_commit_bytes,
            max_tree_bytes: self.max_tree_bytes,
            max_tree_entries: self.max_tree_entries,
            max_output_bytes: max_bytes as usize,
        };
        let digest = digest::Options {
            max_files: self.max_files,
            max_source_bytes: self.max_source_bytes,
            max_output_bytes: (max_bytes as usize).saturating_sub(1),
            ..Default::default()
        };
        let history = digest::HistoryOptions {
            max_commits: self.max_commits,
            max_visits: self.max_visits,
            max_frontier: self.max_frontier,
            max_parent_ids: self.max_parent_ids,
            max_commit_bytes: self.max_commit_bytes,
            max_history_bytes: self.max_history_bytes,
        };
        options.admit(&digest, &history, &d)?;
        ensure!(
            self.profile.is_absolute() && self.request.is_absolute(),
            "github_app_digest_input_invalid"
        );
        let read = |path: &Path, cap: usize| {
            crate::feedback_cli::host_bytes(path, cap, &d)
                .map_err(|_| anyhow::anyhow!("github_app_digest_input_invalid"))
        };
        let profile = HostProfile::parse(&read(&self.profile, 32_000)?)
            .map_err(|_| anyhow::anyhow!("github_app_digest_input_invalid"))?;
        let request = digest::PreviewRequest::parse(&read(&self.request, 4096)?)
            .map_err(|_| anyhow::anyhow!("github_app_digest_input_invalid"))?;
        digest::authorize_profile(&profile, &request)?;
        let git = crate::selected_git(self.git.clone(), self.trust_git)?;
        crate::validate_existing_batch_store(root, &d)
            .map_err(|_| anyhow::anyhow!("github_app_digest_store_unavailable"))?;
        let mut engine = Engine::open_with_timeout(root, d.remaining())
            .map_err(|_| anyhow::anyhow!("github_app_digest_store_unavailable"))?;
        let value = acquisition::acquire(
            &mut engine,
            &profile,
            &digest,
            &request,
            &git,
            &history,
            &options,
            &d,
        )?;
        let mut raw = yoke::json::encode_bounded(&value, max_bytes as usize - 1, &d)?;
        raw.push(b'\n');
        d.check()?;
        std::io::stdout().lock().write_all(&raw)?;
        Ok(true)
    }
}
