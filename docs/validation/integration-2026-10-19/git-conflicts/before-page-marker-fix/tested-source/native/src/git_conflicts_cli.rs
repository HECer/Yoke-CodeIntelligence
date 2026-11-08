//! Explicit current-worktree index reads; no Engine initialization or source writes.
use anyhow::{Result, ensure};
use clap::Args;
use std::{
    io::Write,
    path::{Path, PathBuf},
};
use yoke::{
    capture::Deadline,
    git_workflows::conflicts::{self, Limits, Options, ReadRequest},
    store::Store,
};
#[derive(Args)]
pub struct Arguments {
    #[arg(long)]
    allow_index_read: bool,
    #[arg(long)]
    allow_git_sources: bool,
    #[arg(long)]
    trust_git: bool,
    #[arg(long)]
    git: Option<PathBuf>,
    #[arg(long)]
    scope_snapshot: Vec<String>,
    #[arg(long)]
    limits: Option<PathBuf>,
    #[arg(long)]
    read_source: Option<PathBuf>,
}
impl Arguments {
    pub fn run(&self, root: &Path, timeout: u64, max_bytes: u64) -> Result<bool> {
        ensure!(
            self.allow_index_read && self.trust_git,
            "git_conflicts_permission_required"
        );
        ensure!(
            (1..=600_000).contains(&timeout) && max_bytes <= 4_000_000,
            "git_conflicts_input_invalid"
        );
        ensure!(max_bytes > 0, "git_conflicts_capacity_exceeded");
        let d = Deadline::new(timeout);
        let read = |path: &Path, max| -> Result<Vec<u8>> {
            ensure!(path.is_absolute(), "git_conflicts_input_invalid");
            crate::feedback_cli::host_bytes(path, max, &d)
                .map_err(|_| anyhow::anyhow!("git_conflicts_input_invalid"))
        };
        let limits: Limits = if let Some(path) = &self.limits {
            yoke::json::parse(&read(path, 8192)?, 8192)
                .and_then(yoke::json::decode)
                .map_err(|_| anyhow::anyhow!("git_conflicts_input_invalid"))?
        } else {
            Limits::default()
        };
        let request = self
            .read_source
            .as_ref()
            .map(|p| ReadRequest::parse(&read(p, 8192)?))
            .transpose()?;
        let options = Options {
            allow_index_read: self.allow_index_read,
            allow_git_sources: self.allow_git_sources,
            limits,
            read_request: request,
        };
        options.admit(&self.scope_snapshot, &d)?;
        ensure!(
            !root
                .components()
                .any(|c| matches!(c, std::path::Component::ParentDir)),
            "git_conflicts_input_invalid"
        );
        let root = if root.is_absolute() {
            root.to_owned()
        } else {
            std::env::current_dir()?.join(root)
        };
        let git = crate::selected_git(self.git.clone(), self.trust_git)?;
        let store = Store::open_existing_read_only(&root, &d)
            .map_err(|_| anyhow::anyhow!("git_conflicts_store_unavailable"))?;
        let report = conflicts::inspect(&store, &root, &self.scope_snapshot, &git, &options, &d)?;
        ensure!(
            report.canonical_bytes().len() < max_bytes as usize,
            "git_conflicts_capacity_exceeded"
        );
        d.check()?;
        let mut stdout = std::io::stdout().lock();
        stdout.write_all(report.canonical_bytes())?;
        stdout.write_all(b"\n")?;
        Ok(true)
    }
}
