use anyhow::{Result, ensure};
use clap::{Args, Subcommand};
use std::{io::Write, path::Path};
use yoke::{capture::Deadline, repository_discovery};

#[derive(Args)]
pub struct Arguments {
    #[command(subcommand)]
    operation: Operation,
}
#[derive(Subcommand)]
enum Operation {
    /// List bounded .git marker candidates without opening source or project state.
    Discover {
        #[arg(long, default_value_t = 4)]
        max_depth: usize,
        #[arg(long, default_value_t = 20000)]
        max_entries: usize,
        #[arg(long, default_value_t = 2000)]
        max_directories: usize,
        #[arg(long, default_value_t = 256)]
        max_repositories: usize,
        /// Exact relative candidate path, or . for the search root; grants no authority.
        #[arg(long)]
        select: Option<String>,
    },
}
impl Arguments {
    pub fn run(&self, root: &Path, timeout: u64, max_bytes: u64) -> Result<bool> {
        ensure!(
            (1..=600_000).contains(&timeout) && (1..=4_000_000).contains(&max_bytes),
            "repository_discovery_invalid_limits"
        );
        let Operation::Discover {
            max_depth,
            max_entries,
            max_directories,
            max_repositories,
            select,
        } = &self.operation;
        if let Some(path) = select
            && path != "."
        {
            yoke::paths::normalized(path)?;
        }
        let d = Deadline::new(timeout);
        let report = repository_discovery::discover(
            root,
            &repository_discovery::Options {
                max_depth: *max_depth,
                max_entries: *max_entries,
                max_directories: *max_directories,
                max_repositories: *max_repositories,
                max_report_bytes: max_bytes as usize,
            },
            &d,
        )?;
        // Selected output remains a candidate; it cannot become a HostProject.
        let value = if let Some(path) = select {
            serde_json::json!({
                "producer":report.producer,
                "searchRoot":report.search_root,
                "candidate":report.select(path)?,
                "complete":false,
                "registrationGranted":false,
                "sourceAccessGranted":false
            })
        } else {
            serde_json::to_value(report)?
        };
        let mut bytes = serde_json::to_vec(&value)?;
        bytes.push(b'\n');
        ensure!(
            bytes.len() <= max_bytes as usize,
            "repository_discovery_capacity_exceeded"
        );
        d.check()?;
        std::io::stdout().lock().write_all(&bytes)?;
        Ok(true)
    }
}
