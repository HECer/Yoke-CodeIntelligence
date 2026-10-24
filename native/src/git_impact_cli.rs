//! Explicit computed Git details; no capture, tool selection or publication.
use anyhow::{Result, anyhow, bail, ensure};
use std::{
    borrow::Cow,
    io::{self, Write},
    path::{Component, Path, PathBuf},
};
use yoke::{
    capture::Deadline,
    git_workflows::impact_details::{self, DetailOptions},
    store::Store,
};

pub(super) const INPUT: &str = "git_impact_details_input_invalid";
const CAPACITY: &str = "git_impact_details_capacity_exceeded";
const EXPIRED: &str = "git_impact_details_deadline_exceeded";
const MAX_OPTIONS_BYTES: usize = 128_000;

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
    // Do not canonicalize first: that would follow links before guarded admission.
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
    // Never attach the source error or its potentially sensitive cause chain.
    anyhow!(marker)
}

pub(super) fn run(
    root: &Path,
    report_id: &str,
    options_path: Option<&Path>,
    format: Option<&str>,
    timeout_ms: u64,
    max_bytes: u64,
) -> Result<bool> {
    // These checks precede even host-path resolution or options-file access.
    ensure!(max_bytes != 0, CAPACITY);
    ensure!(max_bytes <= 4_000_000, INPUT);
    ensure!(timeout_ms != 0, EXPIRED);
    ensure!(timeout_ms <= 600_000, INPUT);
    let deadline = Deadline::new(timeout_ms);
    execute(
        root,
        report_id,
        options_path,
        format,
        max_bytes as usize,
        &deadline,
    )
    .map_err(|error| static_error(error, &deadline))
}

fn execute(
    root: &Path,
    report_id: &str,
    options_path: Option<&Path>,
    format: Option<&str>,
    max_bytes: usize,
    deadline: &Deadline,
) -> Result<bool> {
    deadline.check()?;
    ensure!(yoke::model::valid_id(report_id), INPUT);
    let format = format.unwrap_or("json");
    ensure!(matches!(format, "json" | "html"), INPUT);
    let options: DetailOptions = match options_path {
        None => DetailOptions::default(),
        Some(path) => {
            let path = absolute_host(path)?;
            let bytes = super::meaning_archive_cli::read_input(&path, MAX_OPTIONS_BYTES, deadline)?;
            yoke::json::decode(yoke::json::parse(&bytes, MAX_OPTIONS_BYTES)?)?
        }
    };
    let root = absolute_host(root)?;
    // The existing shared guard first admits every constructor-created artifact,
    // WAL mode and complete schema using readonly checks. Keep all handles alive
    // through the Store client and derivation; no missing state is initialized.
    let _held = super::meaning_archive_cli::existing_source_state(&root, deadline)?;
    deadline.check()?;
    let store = Store::open_with_timeout(&root, deadline.remaining())?;
    // Do not ordinary-load the report before derive's BOTH-scope source preflight.
    let result = impact_details::derive(&store, report_id, &options, deadline)?;
    let bytes: Cow<'_, [u8]> = match format {
        "json" => Cow::Borrowed(result.canonical_bytes()),
        "html" => Cow::Owned(impact_details::render_html(&result, deadline)?.into_bytes()),
        _ => unreachable!("validated detail output format"),
    };
    ensure!(
        bytes
            .len()
            .checked_add(1)
            .is_some_and(|size| size <= max_bytes),
        CAPACITY
    );
    deadline.check()?;
    // Admission is complete before the first write. An OS delivery failure cannot
    // retract accepted bytes, and never gains a raw error-chain diagnostic.
    let mut output = io::stdout().lock();
    output.write_all(&bytes)?;
    output.write_all(b"\n")?;
    Ok(true)
}
