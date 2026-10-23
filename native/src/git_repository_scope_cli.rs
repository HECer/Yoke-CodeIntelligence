use anyhow::{Result, ensure};
use clap::Args;
use std::{
    io::Write,
    path::{Path, PathBuf},
};
use yoke::{
    capture::Deadline,
    git_workflows::repository_scope::{self, Limits, Options},
};

#[derive(Args)]
pub struct Arguments {
    /// Explicitly capture the selected working files; crossed repositories stay read-only.
    #[arg(long)]
    capture: bool,
    #[arg(long)]
    max_scope_bytes: Option<usize>,
    #[arg(long)]
    allow_index_read: bool,
    #[arg(long)]
    trust_git: bool,
    #[arg(long)]
    git: Option<PathBuf>,
    #[arg(long, conflicts_with = "no_follow_submodules")]
    follow_submodules: bool,
    #[arg(long)]
    no_follow_submodules: bool,
    #[arg(long, conflicts_with = "no_follow_nested_repositories")]
    follow_nested_repositories: bool,
    #[arg(long)]
    no_follow_nested_repositories: bool,
    #[arg(long, default_value_t = 64)]
    max_repositories: usize,
    #[arg(long, default_value_t = 20000)]
    max_entries: usize,
    #[arg(long, default_value_t = 8000000)]
    max_index_bytes: usize,
    #[arg(long, default_value_t = 16)]
    max_depth: usize,
}
impl Arguments {
    pub fn run(&self, root: &Path, timeout: u64, max_bytes: u64) -> Result<bool> {
        ensure!(
            self.allow_index_read && self.trust_git,
            "repository_scope_index_permission_required"
        );
        ensure!(
            (1..=600000).contains(&timeout) && max_bytes <= 4_000_000,
            "repository_scope_invalid_limits"
        );
        ensure!(
            self.capture || self.max_scope_bytes.is_none(),
            "repository_scope_invalid_limits"
        );
        if self.capture {
            ensure!(max_bytes >= 2048, "repository_scope_capacity_exceeded");
        }
        let limits = Limits {
            max_repositories: self.max_repositories,
            max_entries: self.max_entries,
            max_index_bytes: self.max_index_bytes,
            max_depth: self.max_depth,
            max_output_bytes: if self.capture {
                self.max_scope_bytes.unwrap_or(1_000_000)
            } else {
                max_bytes as usize
            },
        };
        limits.validate()?;
        let git = crate::selected_git(self.git.clone(), self.trust_git)?;
        let d = Deadline::new(timeout);
        let options = Options {
            allow_index_read: true,
            follow_submodules: self.follow_submodules,
            follow_nested_repositories: self.follow_nested_repositories,
            limits,
        };
        let mut bytes = if self.capture {
            let preference = yoke::git_workflows::repository_preferences::Resolved::current(
                root,
                if self.follow_submodules {
                    Some(true)
                } else if self.no_follow_submodules {
                    Some(false)
                } else {
                    None
                },
                if self.follow_nested_repositories {
                    Some(true)
                } else if self.no_follow_nested_repositories {
                    Some(false)
                } else {
                    None
                },
                &d,
            )?;
            let (snapshot, graph, parsed, reused) =
                repository_scope::capture_root_resolved(root, &git, &options, &preference, &d)?;
            let mut result = serde_json::json!({"producer":"yoke-git-repository-capture-1","snapshotId":snapshot.id,"files":snapshot.files.len(),"entities":graph.entities.len(),"observations":graph.observations.len(),"parsedFiles":parsed,"reusedFiles":reused,"crossedRepositoryPolicy":"read-only","projectSourceWritten":false,"defaultCaptureChanged":false});
            if let Some(selected) = preference.selection() {
                result["hostSelection"] = serde_json::json!({"id":selected.id,"revision":selected.revision,"effectiveChoice":preference.choice});
            }
            serde_json::to_vec(&result)?
        } else {
            repository_scope::inspect(root, &git, &options, &d)?
                .canonical_bytes()
                .to_vec()
        };
        bytes.push(b'\n');
        ensure!(
            bytes.len() <= max_bytes as usize,
            "repository_scope_capacity_exceeded"
        );
        d.check()?;
        std::io::stdout().lock().write_all(&bytes)?;
        Ok(true)
    }
}
