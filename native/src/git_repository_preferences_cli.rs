use anyhow::{Result, ensure};
use clap::{Args, Subcommand};
use std::{io::Write, path::Path};
use yoke::{
    capture::Deadline,
    git_workflows::repository_preferences::{self, Expected},
    store::Store,
};

#[derive(Args)]
pub struct Arguments {
    #[command(subcommand)]
    operation: Operation,
}
#[derive(Subcommand)]
enum Operation {
    Show,
    Set {
        #[arg(long)]
        allow_host_writes: bool,
        #[arg(long)]
        expect: String,
        #[arg(long, num_args = 1)]
        follow_submodules: Option<bool>,
        #[arg(long, num_args = 1)]
        follow_nested_repositories: Option<bool>,
    },
    Clear {
        #[arg(long)]
        allow_host_writes: bool,
        #[arg(long)]
        expect: String,
    },
}
impl Arguments {
    pub fn run(&self, root: &Path, timeout: u64, max_bytes: u64) -> Result<bool> {
        ensure!(
            (1..=600_000).contains(&timeout) && (2048..=65_536).contains(&max_bytes),
            "repository_selection_invalid_limits"
        );
        let d = Deadline::new(timeout);
        let selection =
            match &self.operation {
                Operation::Show => repository_preferences::show(root, &d)?,
                Operation::Set {
                    allow_host_writes,
                    expect,
                    follow_submodules,
                    follow_nested_repositories,
                } => {
                    ensure!(
                        *allow_host_writes,
                        "repository_selection_host_write_permission_required"
                    );
                    ensure!(
                        follow_submodules.is_some() || follow_nested_repositories.is_some(),
                        "repository_selection_empty_patch"
                    );
                    let expected = Expected::parse(expect)?;
                    let previous = repository_preferences::show(root, &d)?;
                    if previous.is_none() {
                        ensure!(
                            matches!(expected, Expected::Absent),
                            "repository_selection_stale"
                        );
                    }
                    let mut choice = previous.map(|s| s.choice).unwrap_or_default();
                    if let Some(value) = follow_submodules {
                        choice.follow_submodules = *value;
                    }
                    if let Some(value) = follow_nested_repositories {
                        choice.follow_nested_repositories = *value;
                    }
                    let canonical = root.canonicalize()?;
                    ensure!(
                        1024 + serde_json::to_vec(&canonical.to_str().ok_or_else(
                            || anyhow::anyhow!("repository_selection_root_invalid")
                        )?)?
                        .len()
                            <= max_bytes as usize,
                        "repository_selection_capacity_exceeded"
                    );
                    let store = Store::open_with_timeout(root, d.remaining())?;
                    Some(repository_preferences::set(
                        &store, &expected, choice, true, &d,
                    )?)
                }
                Operation::Clear {
                    allow_host_writes,
                    expect,
                } => {
                    ensure!(
                        *allow_host_writes,
                        "repository_selection_host_write_permission_required"
                    );
                    let expected = Expected::parse(expect)?;
                    let previous = repository_preferences::show(root, &d)?;
                    if previous.is_none() {
                        ensure!(
                            matches!(expected, Expected::Absent),
                            "repository_selection_stale"
                        );
                        None
                    } else {
                        let store = Store::open_with_timeout(root, d.remaining())?;
                        repository_preferences::clear(&store, &expected, true, &d)?;
                        None
                    }
                }
            };
        let mut bytes = serde_json::to_vec(
            &serde_json::json!({"producer":"yoke-host-git-repository-selection-1","selection":selection,"hostControl":true,"gitTrustGranted":false,"projectSourceWritten":false}),
        )?;
        bytes.push(b'\n');
        ensure!(
            bytes.len() <= max_bytes as usize,
            "repository_selection_capacity_exceeded"
        );
        d.check()?;
        std::io::stdout().lock().write_all(&bytes)?;
        Ok(true)
    }
}
