//! Host-explicit compiler analysis of two already checked captured Git sides.
use anyhow::{Result, anyhow, ensure};
use std::{
    io::{self, Write},
    path::{Component, Path, PathBuf},
};
use yoke::{
    capture::Deadline,
    git_workflows::compiler_affected::{self, Options},
    projects::CompilerSpec,
    store::Store,
};
pub(super) const INPUT: &str = "git_compiler_affected_input_invalid";
#[derive(clap::Args)]
pub(super) struct Arguments {
    id: String,
    #[arg(long)]
    node: PathBuf,
    #[arg(long)]
    typescript: PathBuf,
    #[arg(long)]
    trust_compiler: bool,
    #[arg(long, default_value_t = 8)]
    max_depth: usize,
}
impl Arguments {
    pub(super) fn run(&self, root: &Path, timeout: u64, max_bytes: u64) -> Result<bool> {
        ensure!(
            self.trust_compiler
                && self.node.is_absolute()
                && self.typescript.is_absolute()
                && yoke::model::valid_id(&self.id)
                && self.max_depth <= 32,
            INPUT
        );
        ensure!(
            (10..=600000).contains(&timeout) && (1024..=4000000).contains(&max_bytes),
            INPUT
        );
        let d = Deadline::new(timeout);
        let result = (|| -> Result<bool> {
            let root = if root.is_absolute() {
                root.to_path_buf()
            } else {
                std::env::current_dir()?.join(root)
            };
            ensure!(
                !root.components().any(|c| matches!(c, Component::ParentDir)),
                INPUT
            );
            let _held = super::meaning_archive_cli::existing_source_state(&root, &d)?;
            let store = Store::open_with_timeout(&root, d.remaining())?;
            let options = Options {
                max_depth: self.max_depth,
                max_output_bytes: max_bytes as usize,
                ..Default::default()
            };
            let data = compiler_affected::derive(
                &store,
                &self.id,
                &CompilerSpec {
                    node: self.node.clone(),
                    typescript: self.typescript.clone(),
                    trusted: true,
                },
                &options,
                &d,
            )?;
            let bytes = serde_json::to_vec(&data)?;
            ensure!(
                bytes.len() < max_bytes as usize,
                "Compiler-affected output capacity exceeded"
            );
            d.check()?;
            let mut out = io::stdout().lock();
            out.write_all(&bytes)?;
            out.write_all(b"\n")?;
            out.flush()?;
            Ok(true)
        })();
        result.map_err(|e| {
            anyhow!(if d.check().is_err() {
                "git_compiler_affected_deadline_exceeded"
            } else if e.to_string().contains("capacity") {
                "git_compiler_affected_capacity_exceeded"
            } else {
                INPUT
            })
        })
    }
}
