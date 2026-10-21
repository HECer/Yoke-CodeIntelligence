//! Explicit trusted gws execution. No imported field can supply host permission.
use super::{Envelope, ExporterClaim, Limits, ReceivedExport, Shortcut};
use crate::{capture::Deadline, paths, process};
use anyhow::{Result, ensure};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, OpenOptions},
    io::Read,
    path::{Component, Path, PathBuf},
    process::Command,
};

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ClientSpec {
    pub executable: PathBuf,
    pub executable_sha256: String,
    pub version: String,
    pub configuration_directory: PathBuf,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CaptureSpec {
    pub shortcut_path: String,
    pub expected_shortcut_sha256: String,
    pub expected_file_id: String,
    pub client: ClientSpec,
}
/// Host-only request state; deliberately not deserializable from captured JSON.
#[derive(Default)]
pub struct Permissions {
    pub allow_network: bool,
    pub allow_exporter_execution: bool,
    pub allow_client_state: bool,
}
fn host_path(path: &Path) -> Result<()> {
    ensure!(
        path.is_absolute(),
        "google_workspace_absolute_host_path_required"
    );
    for part in path.components() {
        match part {
            Component::Normal(name) => {
                paths::normalized_syntax(name.to_str().ok_or_else(|| {
                    anyhow::anyhow!("google_workspace_host_path_encoding_invalid")
                })?)?;
            }
            Component::ParentDir | Component::CurDir => {
                anyhow::bail!("google_workspace_host_path_noncanonical")
            }
            #[cfg(windows)]
            Component::Prefix(prefix) => {
                use std::path::Prefix;
                ensure!(
                    matches!(prefix.kind(), Prefix::Disk(_) | Prefix::VerbatimDisk(_)),
                    "google_workspace_device_host_path_refused"
                );
            }
            _ => {}
        }
    }
    Ok(())
}
fn directory(path: &Path) -> Result<PathBuf> {
    host_path(path)?;
    for ancestor in path.ancestors() {
        let metadata = fs::symlink_metadata(ancestor)?;
        ensure!(
            metadata.is_dir() && !paths::is_link(&metadata),
            "google_workspace_host_directory_refused"
        );
    }
    let metadata = fs::symlink_metadata(path)?;
    ensure!(
        metadata.is_dir() && !paths::is_link(&metadata),
        "google_workspace_host_directory_refused"
    );
    let physical = fs::canonicalize(path)?;
    ensure!(
        fs::metadata(&physical)?.is_dir(),
        "google_workspace_host_directory_changed"
    );
    Ok(physical)
}
fn open_regular(path: &Path, max: usize) -> Result<File> {
    host_path(path)?;
    let metadata = fs::symlink_metadata(path)?;
    ensure!(
        metadata.is_file() && !paths::is_link(&metadata) && metadata.len() <= max as u64,
        "google_workspace_host_file_refused"
    );
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        options.share_mode(1).custom_flags(0x0020_0000);
    }
    let file = options.open(path)?;
    let opened = file.metadata()?;
    ensure!(
        opened.is_file() && !paths::is_link(&opened) && opened.len() == metadata.len(),
        "google_workspace_host_file_changed"
    );
    Ok(file)
}
fn hash_program(path: &Path, deadline: &Deadline) -> Result<(File, String)> {
    let mut file = open_regular(path, 200_000_000)?;
    let size = file.metadata()?.len();
    let mut digest = Sha256::new();
    let mut chunk = [0; 65_536];
    let mut count = 0usize;
    loop {
        deadline.check()?;
        let n = file.read(&mut chunk)?;
        if n == 0 {
            break;
        }
        count = count
            .checked_add(n)
            .ok_or_else(|| anyhow::anyhow!("google_workspace_exporter_capacity_exceeded"))?;
        ensure!(
            count <= 200_000_000,
            "google_workspace_exporter_capacity_exceeded"
        );
        digest.update(&chunk[..n]);
    }
    ensure!(
        count as u64 == size && file.metadata()?.len() == size,
        "google_workspace_exporter_changed"
    );
    Ok((file, hex::encode(digest.finalize())))
}
fn command(program: &Path, cwd: &Path, config: &Path) -> Command {
    let mut command = Command::new(program);
    command
        .current_dir(cwd)
        .env("GOOGLE_WORKSPACE_CLI_CONFIG_DIR", config)
        // gws otherwise falls back from the selected config to the user's
        // unrelated well-known gcloud ADC. An explicit missing owned path makes
        // that fallback fail closed without reading or copying credentials.
        .env(
            "GOOGLE_APPLICATION_CREDENTIALS",
            cwd.join("adc-not-selected.json"),
        );
    command
}
fn execute(command: Command, deadline: &Deadline) -> Result<process::CapturedOutput> {
    let output = process::run_capture(
        command,
        &[],
        deadline,
        process::CaptureLimits {
            stdin_bytes: 0,
            stdout_bytes: 32_768,
            stderr_bytes: 32_768,
        },
    )?;
    deadline.check()?;
    ensure!(
        output.output_complete && output.stdin_complete && !output.timed_out && !output.io_error,
        "google_workspace_exporter_output_incomplete"
    );
    ensure!(
        !output.output_exceeded,
        "google_workspace_exporter_output_capacity_exceeded"
    );
    ensure!(
        output.exit_code == Some(0),
        "google_workspace_exporter_request_failed"
    );
    Ok(output)
}
fn archive_admission(
    shortcut: &Shortcut,
    claim: &ExporterClaim,
    limits: &Limits,
    deadline: &Deadline,
) -> Result<()> {
    let frame = Envelope {
        schema_version: 1,
        kind: super::KIND.to_owned(),
        metadata: shortcut.metadata().clone(),
        requested_mime_type: shortcut.metadata().kind.export_mime().to_owned(),
        exporter_claim: claim.clone(),
        body_sha256: "0".repeat(64),
        body_length: limits.max_body_bytes,
        body_base64: String::new(),
        source_id: "0".repeat(64),
    };
    let header = crate::json::encode_bounded(&frame, limits.max_archive_bytes, deadline)?;
    ensure!(
        header
            .len()
            .checked_add(limits.max_body_bytes.div_ceil(3).saturating_mul(4))
            .is_some_and(|n| n <= limits.max_archive_bytes),
        "google_workspace_archive_capture_budget_exceeded"
    );
    Ok(())
}
/// Run only the fixed Drive export operation, under separately supplied host grants.
/// The trusted external client is not an OS sandbox or network attestation.
pub fn capture(
    source_root: &Path,
    shortcut_bytes: &[u8],
    spec: &CaptureSpec,
    permissions: &Permissions,
    limits: &Limits,
    deadline: &Deadline,
) -> Result<ReceivedExport> {
    deadline.check()?;
    ensure!(
        permissions.allow_network
            && permissions.allow_exporter_execution
            && permissions.allow_client_state,
        "google_workspace_host_permission_required"
    );
    limits.validate()?;
    let shortcut = Shortcut::parse(&spec.shortcut_path, shortcut_bytes, deadline)?;
    shortcut
        .metadata()
        .check_selection(&spec.expected_shortcut_sha256, &spec.expected_file_id)?;
    // gws 0.22.5 exposes no resource-key header on this operation. Never silently
    // discard a selected key or turn missing transport support into empty success.
    ensure!(
        shortcut.resource_key().is_none(),
        "google_workspace_resource_key_transport_unavailable"
    );
    let root = directory(source_root)?;
    host_path(&spec.client.executable)?;
    ensure!(
        spec.client.version == "0.22.5",
        "google_workspace_exporter_version_unsupported"
    );
    super::shortcut::digest(&spec.client.executable_sha256)?;
    let raw = fs::symlink_metadata(&spec.client.executable)?;
    ensure!(
        raw.is_file() && !paths::is_link(&raw),
        "google_workspace_exporter_path_refused"
    );
    let program = fs::canonicalize(&spec.client.executable)?;
    let _program_parent = directory(
        spec.client
            .executable
            .parent()
            .ok_or_else(|| anyhow::anyhow!("google_workspace_exporter_parent_required"))?,
    )?;
    let config = directory(&spec.client.configuration_directory)?;
    ensure!(
        !program.starts_with(&root) && !config.starts_with(&root),
        "google_workspace_exporter_inside_source_refused"
    );
    let (program_guard, program_hash) = hash_program(&program, deadline)?;
    ensure!(
        program_hash == spec.client.executable_sha256,
        "google_workspace_exporter_changed"
    );
    let claim = ExporterClaim {
        executable_sha256: program_hash.clone(),
        version: spec.client.version.clone(),
    };
    archive_admission(&shortcut, &claim, limits, deadline)?;
    let temp_base = directory(&std::env::temp_dir())?;
    ensure!(
        !temp_base.starts_with(&root),
        "google_workspace_temporary_directory_inside_source_refused"
    );
    let owned = tempfile::Builder::new()
        .prefix("yoke-google-export-")
        .tempdir_in(temp_base)?;
    let job = fs::canonicalize(owned.path())?;
    ensure!(
        !job.starts_with(&root),
        "google_workspace_temporary_directory_inside_source_refused"
    );
    fs::write(job.join(".env"), b"")?;
    let result = (|| {
        let mut version = command(&program, &job, &config);
        version.arg("--version");
        let version = execute(version, deadline)?;
        let text = std::str::from_utf8(&version.stdout)
            .map_err(|_| anyhow::anyhow!("google_workspace_exporter_version_invalid"))?;
        ensure!(
            text.lines().next() == Some("gws 0.22.5"),
            "google_workspace_exporter_version_invalid"
        );
        let params =
            serde_json::to_string(&serde_json::json!({"fileId":shortcut.metadata().file_id,
            "mimeType":shortcut.metadata().kind.export_mime()}))?;
        let mut request = command(&program, &job, &config);
        request.args([
            "drive",
            "files",
            "export",
            "--params",
            &params,
            "--output",
            "export.bin",
        ]);
        execute(request, deadline)?;
        ensure!(
            directory(&spec.client.configuration_directory)? == config,
            "google_workspace_client_configuration_changed"
        );
        let (_after_guard, after_hash) = hash_program(&program, deadline)?;
        ensure!(
            after_hash == program_hash && program_guard.metadata()?.is_file(),
            "google_workspace_exporter_changed"
        );
        let mut file = open_regular(&job.join("export.bin"), limits.max_body_bytes)?;
        let expected = file.metadata()?.len();
        let mut body = Vec::new();
        let mut chunk = [0; 65_536];
        loop {
            deadline.check()?;
            let n = file.read(&mut chunk)?;
            if n == 0 {
                break;
            }
            ensure!(
                body.len()
                    .checked_add(n)
                    .is_some_and(|n| n <= limits.max_body_bytes),
                "google_workspace_body_capacity_exceeded"
            );
            body.extend_from_slice(&chunk[..n]);
        }
        ensure!(
            body.len() as u64 == expected && file.metadata()?.len() == expected,
            "google_workspace_export_body_changed"
        );
        ReceivedExport::from_received_data(
            shortcut.metadata().clone(),
            claim,
            &body,
            limits,
            deadline,
        )
    })();
    if result.as_ref().is_err_and(process::cleanup_unproven) {
        let _retained = owned.keep();
        return result;
    }
    result
}
