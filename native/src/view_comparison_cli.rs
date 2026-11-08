//! Explicit host delivery for fully checked historical View comparisons.
use anyhow::{Result, anyhow, bail, ensure};
use clap::Args;
use serde::Deserialize;
use std::{
    collections::BTreeSet,
    io::{self, Write},
    path::{Component, Path, PathBuf},
};
use yoke::{
    capture::Deadline,
    model::valid_id,
    store::Store,
    view_comparison::{self, ComparisonOptions, DeclaredFileContinuity, SelectionPin},
};

pub(super) const INPUT: &str = "view_comparison_input_invalid";
const CAPACITY: &str = "view_comparison_capacity_exceeded";
const EXPIRED: &str = "view_comparison_deadline_exceeded";

#[derive(Args)]
pub struct Arguments {
    /// Host JSON pin for the earlier already-published selection.
    #[arg(long)]
    before: PathBuf,
    /// Host JSON pin for the later already-published selection.
    #[arg(long)]
    after: PathBuf,
    /// Optional explicit primary-file continuity declarations, never rename proof.
    #[arg(long)]
    continuity: Option<PathBuf>,
    #[arg(long, default_value_t = 10_000)]
    max_rows: usize,
    #[arg(long, default_value_t = 64_000_000)]
    max_input_bytes: usize,
    #[arg(long, default_value_t = 64_000_000)]
    max_source_bytes: usize,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct HostPin {
    schema_version: u32,
    workspace_id: String,
    snapshot_id: String,
    view_id: String,
    view_artifact_id: String,
    base_graph_hash: String,
}
impl HostPin {
    fn checked(self) -> Result<SelectionPin> {
        ensure!(
            self.schema_version == 1
                && [
                    &self.workspace_id,
                    &self.snapshot_id,
                    &self.view_id,
                    &self.view_artifact_id,
                    &self.base_graph_hash,
                ]
                .into_iter()
                .all(|value| valid_id(value)),
            INPUT
        );
        Ok(SelectionPin {
            workspace_id: self.workspace_id,
            snapshot_id: self.snapshot_id,
            view_id: self.view_id,
            view_artifact_id: self.view_artifact_id,
            base_graph_hash: self.base_graph_hash,
        })
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct HostContinuity {
    schema_version: u32,
    declarations: Vec<HostFileContinuity>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct HostFileContinuity {
    before_path: String,
    before_hash: String,
    after_path: String,
    after_hash: String,
}

fn absolute_host(path: &Path) -> Result<PathBuf> {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()?.join(path)
    };
    let mut normalized = PathBuf::new();
    for component in absolute.components() {
        match component {
            Component::ParentDir => bail!(INPUT),
            Component::CurDir => {}
            _ => normalized.push(component.as_os_str()),
        }
    }
    // Preserve link components for the shared guarded reader to reject.
    Ok(normalized)
}

fn read_host<T: serde::de::DeserializeOwned>(
    path: &Path,
    capacity: usize,
    deadline: &Deadline,
) -> Result<T> {
    let path = absolute_host(path)?;
    let bytes = super::meaning_archive_cli::read_input(&path, capacity, deadline)?;
    let value = yoke::json::decode(yoke::json::parse(&bytes, capacity)?)?;
    deadline.check()?;
    Ok(value)
}

fn continuity(path: Option<&Path>, deadline: &Deadline) -> Result<Vec<DeclaredFileContinuity>> {
    let Some(path) = path else {
        return Ok(Vec::new());
    };
    let host: HostContinuity = read_host(path, 128_000, deadline)?;
    ensure!(
        host.schema_version == 1 && host.declarations.len() <= 100,
        INPUT
    );
    let mut before = BTreeSet::new();
    let mut after = BTreeSet::new();
    let mut declarations = Vec::new();
    for row in host.declarations {
        deadline.check()?;
        ensure!(
            row.before_path.len() <= 4096
                && row.after_path.len() <= 4096
                && yoke::paths::normalized_syntax(&row.before_path)? == row.before_path
                && yoke::paths::normalized_syntax(&row.after_path)? == row.after_path
                && valid_id(&row.before_hash)
                && valid_id(&row.after_hash)
                && before.insert(row.before_path.clone())
                && after.insert(row.after_path.clone()),
            INPUT
        );
        declarations.push(DeclaredFileContinuity {
            before_path: row.before_path,
            before_hash: row.before_hash,
            after_path: row.after_path,
            after_hash: row.after_hash,
        });
    }
    Ok(declarations)
}

fn static_error(error: anyhow::Error, deadline: &Deadline) -> anyhow::Error {
    let marker = if deadline.check().is_err() {
        EXPIRED
    } else if error.chain().any(|cause| {
        matches!(
            cause.to_string().as_str(),
            CAPACITY | "comparison_capacity_exceeded"
        )
    }) {
        CAPACITY
    } else {
        INPUT
    };
    // Never retain the cause chain, source paths, host values or OS diagnostics.
    anyhow!(marker)
}

pub(super) fn run(args: &Arguments, root: &Path, timeout_ms: u64, max_bytes: u64) -> Result<bool> {
    // Inherent nonempty result requirements refuse before any file or state open.
    ensure!(
        max_bytes > 1
            && args.max_rows != 0
            && args.max_input_bytes != 0
            && args.max_source_bytes != 0,
        CAPACITY
    );
    ensure!(
        max_bytes <= 4_000_000
            && args.max_rows <= 1_000_000
            && args.max_input_bytes <= 128_000_000
            && args.max_source_bytes <= 128_000_000,
        INPUT
    );
    ensure!(timeout_ms != 0, EXPIRED);
    ensure!(timeout_ms <= 600_000, INPUT);
    let deadline = Deadline::new(timeout_ms);
    execute(args, root, max_bytes as usize, &deadline)
        .map_err(|error| static_error(error, &deadline))
}

fn execute(args: &Arguments, root: &Path, max_bytes: usize, deadline: &Deadline) -> Result<bool> {
    deadline.check()?;
    let before: HostPin = read_host(&args.before, 4096, deadline)?;
    let before = before.checked()?;
    let after: HostPin = read_host(&args.after, 4096, deadline)?;
    let after = after.checked()?;
    let options = ComparisonOptions {
        max_result_rows: args.max_rows,
        // Reserve the delivered newline before deriving the canonical result.
        max_output_bytes: max_bytes - 1,
        max_input_bytes: args.max_input_bytes,
        max_source_bytes: args.max_source_bytes,
        declared_file_continuity: continuity(args.continuity.as_deref(), deadline)?,
    };
    let root = absolute_host(root)?;
    // Keep guarded existing directories/artifacts alive through Store teardown.
    // Missing/incomplete stores never gain constructor-created state or sidecars.
    let _held = super::meaning_archive_cli::existing_source_state(&root, deadline)?;
    let store = Store::open_with_timeout(&root, deadline.remaining())?;
    // This sealed adapter checks BOTH pins and replays original source authority.
    // Do not load a View through a live/default source-selection path first.
    let result = view_comparison::compare_published(&store, &before, &after, &options, deadline)?;
    let bytes = result.canonical_bytes();
    ensure!(
        bytes
            .len()
            .checked_add(1)
            .is_some_and(|size| size <= max_bytes),
        CAPACITY
    );
    deadline.check()?;
    let mut output = io::stdout().lock();
    output.write_all(bytes)?;
    output.write_all(b"\n")?;
    output.flush()?;
    Ok(true)
}
