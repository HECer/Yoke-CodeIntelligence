//! Explicit selected host configuration input and create-only profile output.
use anyhow::{Result, ensure};
use clap::{Args, Subcommand};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
};
use yoke::{
    capture::Deadline,
    session_config::{self, Source, Target},
};
#[path = "session_config_discovery_cli.rs"]
mod discovery;
#[path = "session_config_template_cli.rs"]
mod templates;
#[derive(Args)]
pub struct Arguments {
    #[command(subcommand)]
    operation: Operation,
}
#[derive(Subcommand)]
enum Operation {
    #[command(hide = true)]
    RenderWorker,
    Discover {
        #[arg(long)]
        selection: PathBuf,
        #[arg(long)]
        allow_configuration_read: bool,
        #[arg(long)]
        allow_policy_projection: bool,
        #[arg(long)]
        allow_template_evaluation: bool,
    },
    Plan {
        #[arg(long)]
        selection: PathBuf,
        #[arg(long)]
        allow_configuration_read: bool,
        #[arg(long)]
        allow_policy_projection: bool,
        #[arg(long)]
        allow_template_evaluation: bool,
    },
    Export {
        #[arg(long)]
        selection: PathBuf,
        #[arg(long)]
        output: PathBuf,
        #[arg(long)]
        allow_configuration_read: bool,
        #[arg(long)]
        allow_policy_projection: bool,
        #[arg(long)]
        allow_template_evaluation: bool,
        #[arg(long)]
        allow_output: bool,
    },
}
#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Selection {
    host_profile: yoke::project_session::HostProfile,
    files: Vec<FileSelection>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    template_context: Option<templates::TemplateContext>,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct FileSelection {
    path: PathBuf,
    sha256: String,
    target: Target,
}
struct Loaded {
    path: PathBuf,
    bytes: Vec<u8>,
    _held: Vec<File>,
}
#[cfg(windows)]
fn single_link(file: &File) -> Result<bool> {
    use std::{ffi::c_void, os::windows::io::AsRawHandle};
    #[repr(C)]
    struct Info {
        attributes: u32,
        times: [u32; 6],
        volume: u32,
        size: [u32; 2],
        links: u32,
        index: [u32; 2],
    }
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetFileInformationByHandle(file: *mut c_void, info: *mut Info) -> i32;
    }
    let mut info = Info {
        attributes: 0,
        times: [0; 6],
        volume: 0,
        size: [0; 2],
        links: 0,
        index: [0; 2],
    };
    ensure!(
        unsafe { GetFileInformationByHandle(file.as_raw_handle(), &mut info) } != 0,
        "session_config_file_identity_unavailable"
    );
    Ok(info.links == 1)
}
#[cfg(unix)]
fn single_link(file: &File) -> Result<bool> {
    use std::os::unix::fs::MetadataExt;
    Ok(file.metadata()?.nlink() == 1)
}
#[cfg(not(any(windows, unix)))]
fn single_link(_file: &File) -> Result<bool> {
    anyhow::bail!("session_config_file_identity_unavailable")
}
fn read(path: &Path, cap: usize, deadline: &Deadline) -> Result<Loaded> {
    deadline.check()?;
    super::meaning_archive_cli::host_path(path)?;
    let mut held = super::meaning_archive_cli::directory_guards(
        path.parent()
            .ok_or_else(|| anyhow::anyhow!("session_config_parent_required"))?,
        deadline,
    )?;
    let before = fs::symlink_metadata(path)?;
    ensure!(
        before.is_file() && !yoke::paths::is_link(&before) && before.len() <= cap as u64,
        "session_config_file_type_or_capacity_refused"
    );
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        options.share_mode(1).custom_flags(0x0020_0000);
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW);
    }
    let mut file = options.open(path)?;
    let opened = file.metadata()?;
    ensure!(
        opened.is_file()
            && !yoke::paths::is_link(&opened)
            && opened.len() == before.len()
            && single_link(&file)?,
        "session_config_file_changed_or_aliased"
    );
    let mut bytes = Vec::new();
    let mut chunk = [0; 4096];
    loop {
        deadline.check()?;
        let count = file.read(&mut chunk)?;
        if count == 0 {
            break;
        }
        ensure!(
            bytes.len().checked_add(count).is_some_and(|n| n <= cap),
            "session_config_file_capacity_exceeded"
        );
        bytes.extend_from_slice(&chunk[..count]);
    }
    ensure!(
        bytes.len() as u64 == opened.len()
            && file.metadata()?.len() == opened.len()
            && single_link(&file)?,
        "session_config_file_changed_or_aliased"
    );
    held.push(file);
    deadline.check()?;
    Ok(Loaded {
        path: path.into(),
        bytes,
        _held: held,
    })
}
fn prepare(
    path: &Path,
    deadline: &Deadline,
    allow_templates: bool,
) -> Result<(session_config::Projection, Vec<Loaded>)> {
    let selected = read(path, 128000, deadline)?;
    let selection: Selection =
        yoke::json::parse(&selected.bytes, 128000).and_then(yoke::json::decode)?;
    ensure!(
        selection.files.len() <= session_config::MAX_FILES,
        "session_config_file_capacity_exceeded"
    );
    templates::admit(
        selection.template_context.as_ref(),
        allow_templates,
        deadline,
    )?;
    let mut loaded = Vec::new();
    let mut total = 0usize;
    for selected in &selection.files {
        let value = read(&selected.path, session_config::MAX_FILE_BYTES, deadline)?;
        total = total
            .checked_add(value.bytes.len())
            .ok_or_else(|| anyhow::anyhow!("session_config_source_capacity_exceeded"))?;
        ensure!(
            total <= session_config::MAX_TOTAL_BYTES,
            "session_config_source_capacity_exceeded"
        );
        loaded.push(value);
    }
    let sources: Vec<_> = selection
        .files
        .iter()
        .zip(&loaded)
        .map(|(selected, loaded)| Source {
            target: &selected.target,
            expected_sha256: &selected.sha256,
            bytes: &loaded.bytes,
        })
        .collect();
    let projection = templates::project(
        &selection.host_profile,
        &sources,
        selection.template_context.as_ref(),
        deadline,
    )?;
    loaded.push(selected);
    Ok((projection, loaded))
}
fn recheck(loaded: &[Loaded], deadline: &Deadline) -> Result<()> {
    for original in loaded {
        ensure!(
            read(&original.path, original.bytes.len(), deadline)?.bytes == original.bytes,
            "session_config_source_changed_before_publication"
        );
    }
    Ok(())
}
pub fn run(args: &Arguments, timeout: u64, max_bytes: u64) -> Result<bool> {
    ensure!(
        (10..=600000).contains(&timeout) && (1024..=16_000_000).contains(&max_bytes),
        "session_config_host_budget_invalid"
    );
    let deadline = Deadline::new(timeout);
    let cap = max_bytes as usize - 1;
    match &args.operation {
        Operation::RenderWorker => templates::worker(&deadline, cap),
        Operation::Discover {
            selection,
            allow_configuration_read,
            allow_policy_projection,
            allow_template_evaluation,
        } => {
            ensure!(
                *allow_configuration_read && *allow_policy_projection,
                "session_config_host_permission_required"
            );
            let prepared = discovery::prepare_with_template_permission(
                selection,
                &deadline,
                *allow_template_evaluation,
            )?;
            let reply = yoke::json::encode_bounded(&prepared.value, cap, &deadline)?;
            prepared.recheck(&deadline)?;
            // Keep source and directory guards through delivery, as export does.
            let mut stdout = std::io::stdout().lock();
            stdout.write_all(&reply)?;
            stdout.write_all(b"\n")?;
            stdout.flush()?;
            Ok(true)
        }
        Operation::Plan {
            selection,
            allow_configuration_read,
            allow_policy_projection,
            allow_template_evaluation,
        } => {
            ensure!(
                *allow_configuration_read && *allow_policy_projection,
                "session_config_host_permission_required"
            );
            let (projection, held) = prepare(selection, &deadline, *allow_template_evaluation)?;
            let value = projection.plan();
            let reply = yoke::json::encode_bounded(&value, cap, &deadline)?;
            recheck(&held, &deadline)?;
            let mut stdout = std::io::stdout().lock();
            stdout.write_all(&reply)?;
            stdout.write_all(b"\n")?;
            stdout.flush()?;
            Ok(true)
        }
        Operation::Export {
            selection,
            output,
            allow_configuration_read,
            allow_policy_projection,
            allow_template_evaluation,
            allow_output,
        } => {
            ensure!(
                *allow_configuration_read && *allow_policy_projection && *allow_output,
                "session_config_host_permission_required"
            );
            let output_guard = super::meaning_archive_cli::fresh_destination(output, &deadline)?;
            let (projection, held) = prepare(selection, &deadline, *allow_template_evaluation)?;
            let profile = projection.profile_bytes(&deadline)?;
            let shape = |hash: &str, bytes: usize| json!({"schema":"native_session_config_export_v1","profileSha256":hash,"profileBytes":bytes,"output":output,"sources":projection.plan()["sources"],"outputCreated":true,"sourceBytesRewritten":false,"templatesEvaluated":projection.plan()["templatesEvaluated"],"sourceExecuted":false,"storeOpened":false,"networkRequests":0,"modelCalls":0,"complete":false});
            yoke::json::encode_bounded(&shape(&"f".repeat(64), usize::MAX), cap, &deadline)?;
            let hash = yoke::model::hash(&profile);
            let value = shape(&hash, profile.len());
            let reply = yoke::json::encode_bounded(&value, cap, &deadline)?;
            recheck(&held, &deadline)?;
            super::meaning_archive_cli::write_new(output, &profile, &deadline)?;
            drop(output_guard);
            let mut stdout = std::io::stdout().lock();
            if stdout
                .write_all(&reply)
                .and_then(|_| stdout.write_all(b"\n"))
                .and_then(|_| stdout.flush())
                .is_err()
            {
                anyhow::bail!("session_config_profile_committed_delivery_failed:{hash}");
            }
            Ok(true)
        }
    }
}
