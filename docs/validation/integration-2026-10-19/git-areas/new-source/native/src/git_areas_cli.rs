//! Existing-state captured impact classification; no Git/provider selection.
use anyhow::{Result, ensure};
use clap::Args;
use std::{
    io::Write,
    path::{Path, PathBuf},
};
use yoke::{
    capture::Deadline,
    git_workflows::areas::{self, Options},
    store::Store,
};

#[derive(Args)]
pub struct Arguments {
    id: String,
    #[arg(long)]
    meaning: Vec<String>,
    #[arg(long)]
    options: Option<PathBuf>,
}
impl Arguments {
    pub fn run(&self, root: &Path, timeout: u64, max_bytes: u64) -> Result<bool> {
        ensure!(
            (1..=600_000).contains(&timeout) && max_bytes <= 4_000_000,
            "git_areas_input_invalid"
        );
        ensure!(max_bytes > 0, "git_areas_capacity_exceeded");
        let d = Deadline::new(timeout);
        let options = if let Some(path) = &self.options {
            ensure!(path.is_absolute(), "git_areas_input_invalid");
            let raw = crate::feedback_cli::host_bytes(path, 8192, &d)
                .map_err(|_| anyhow::anyhow!("git_areas_input_invalid"))?;
            Options::parse(&raw)?
        } else {
            Options::default()
        };
        options.admit(&self.id, &self.meaning, &d)?;
        ensure!(
            !root
                .components()
                .any(|c| matches!(c, std::path::Component::ParentDir)),
            "git_areas_input_invalid"
        );
        let root = if root.is_absolute() {
            root.to_owned()
        } else {
            std::env::current_dir()?.join(root)
        };
        let store = Store::open_existing_read_only(&root, &d)
            .map_err(|_| anyhow::anyhow!("git_areas_store_unavailable"))?;
        let report = areas::derive(&store, &self.id, &self.meaning, &options, &d)?;
        ensure!(
            report.canonical_bytes().len() < max_bytes as usize,
            "git_areas_capacity_exceeded"
        );
        d.check()?;
        let mut stdout = std::io::stdout().lock();
        stdout.write_all(report.canonical_bytes())?;
        stdout.write_all(b"\n")?;
        Ok(true)
    }
}
