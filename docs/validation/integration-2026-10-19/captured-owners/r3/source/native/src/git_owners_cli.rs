//! Explicit stored-source owner candidates; no Git/provider/worktree fallback.
use anyhow::{Result, ensure};
use clap::Args;
use std::{
    io::Write,
    path::{Path, PathBuf},
};
use yoke::{
    capture::Deadline,
    git_workflows::owners::{self, Options},
    store::Store,
};
#[derive(Args)]
pub struct Arguments {
    id: String,
    #[arg(long)]
    options: Option<PathBuf>,
}
impl Arguments {
    pub fn run(&self, root: &Path, timeout: u64, max_bytes: u64) -> Result<bool> {
        ensure!(
            (1..=600_000).contains(&timeout) && max_bytes <= 4_000_000,
            "git_owners_input_invalid"
        );
        ensure!(max_bytes > 0, "git_owners_capacity_exceeded");
        let d = Deadline::new(timeout);
        let options = if let Some(path) = &self.options {
            ensure!(path.is_absolute(), "git_owners_input_invalid");
            Options::parse(
                &crate::feedback_cli::host_bytes(path, 8192, &d)
                    .map_err(|_| anyhow::anyhow!("git_owners_input_invalid"))?,
            )?
        } else {
            Options::default()
        };
        options.admit(&self.id, &d)?;
        ensure!(
            !root
                .components()
                .any(|c| matches!(c, std::path::Component::ParentDir)),
            "git_owners_input_invalid"
        );
        let root = if root.is_absolute() {
            root.to_owned()
        } else {
            std::env::current_dir()?.join(root)
        };
        let store = Store::open_existing_read_only(&root, &d)
            .map_err(|_| anyhow::anyhow!("git_owners_store_unavailable"))?;
        let value = owners::derive_with_options(&store, &self.id, &options, &d)?;
        let mut bytes = yoke::json::encode_bounded(&value, max_bytes as usize - 1, &d)
            .map_err(|_| anyhow::anyhow!("git_owners_capacity_exceeded"))?;
        bytes.push(b'\n');
        d.check()?;
        std::io::stdout().lock().write_all(&bytes)?;
        Ok(true)
    }
}
