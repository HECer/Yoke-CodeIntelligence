//! Explicit captured-hierarchy Git candidates from a checked stored report.
use anyhow::{Result, anyhow, bail, ensure};
use std::{
    io::{self, Write},
    path::{Component, Path, PathBuf},
};
use yoke::{
    capture::Deadline,
    git_workflows::affected_details::{self, AffectedOptions},
    store::Store,
};

pub(super) const INPUT: &str = "git_affected_details_input_invalid";
const CAPACITY: &str = "git_affected_details_capacity_exceeded";
const EXPIRED: &str = "git_affected_details_deadline_exceeded";
const MAX_OPTIONS_BYTES: usize = 64_000;

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
    // Keep links visible until the shared guarded host-path admission.
    Ok(normalized)
}

fn static_error(error: anyhow::Error, deadline: &Deadline) -> anyhow::Error {
    let marker = if deadline.check().is_err() || error.to_string() == EXPIRED {
        EXPIRED
    } else if error.to_string() == CAPACITY {
        CAPACITY
    } else {
        INPUT
    };
    anyhow!(marker)
}

fn validate_options(options: &AffectedOptions) -> Result<()> {
    ensure!(options.policy == "captured_hierarchy_v1", INPUT);
    let maximum = AffectedOptions::default();
    for (actual, maximum) in [
        (options.max_graph_entities, maximum.max_graph_entities),
        (
            options.max_graph_observations,
            maximum.max_graph_observations,
        ),
        (options.max_graph_evidence, maximum.max_graph_evidence),
        (options.max_original_seeds, maximum.max_original_seeds),
        (options.max_expanded_seeds, maximum.max_expanded_seeds),
        (options.max_hierarchy_depth, maximum.max_hierarchy_depth),
        (options.max_incoming_depth, maximum.max_incoming_depth),
        (options.max_states, maximum.max_states),
        (options.max_edge_visits, maximum.max_edge_visits),
        (options.max_rows, maximum.max_rows),
        (options.max_source_bytes, maximum.max_source_bytes),
        (options.max_output_bytes, maximum.max_output_bytes),
    ] {
        ensure!(actual <= maximum, INPUT);
    }
    Ok(())
}

pub(super) fn run(
    root: &Path,
    report_id: &str,
    options_path: Option<&Path>,
    timeout_ms: u64,
    max_bytes: u64,
) -> Result<bool> {
    // Static refusals precede host-path resolution and options-file access.
    ensure!(max_bytes != 0, CAPACITY);
    ensure!(max_bytes <= 4_000_000, INPUT);
    ensure!(timeout_ms != 0, EXPIRED);
    ensure!(timeout_ms <= 600_000, INPUT);
    let deadline = Deadline::new(timeout_ms);
    execute(root, report_id, options_path, max_bytes as usize, &deadline)
        .map_err(|error| static_error(error, &deadline))
}

fn execute(
    root: &Path,
    report_id: &str,
    options_path: Option<&Path>,
    max_bytes: usize,
    deadline: &Deadline,
) -> Result<bool> {
    deadline.check()?;
    ensure!(yoke::model::valid_id(report_id), INPUT);
    let options: AffectedOptions = match options_path {
        None => AffectedOptions::default(),
        Some(path) => {
            let path = absolute_host(path)?;
            let bytes = super::meaning_archive_cli::read_input(&path, MAX_OPTIONS_BYTES, deadline)?;
            yoke::json::decode(yoke::json::parse(&bytes, MAX_OPTIONS_BYTES)?)?
        }
    };
    validate_options(&options)?;
    let root = absolute_host(root)?;
    // Complete existing state is admitted before the normal Store constructor.
    // Retain all shared guards through the client, derivation and delivery.
    let _held = super::meaning_archive_cli::existing_source_state(&root, deadline)?;
    deadline.check()?;
    let store = Store::open_with_timeout(&root, deadline.remaining())?;
    let result = affected_details::derive(&store, report_id, &options, deadline)?;
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
