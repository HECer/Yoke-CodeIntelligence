//! Host-selected acquisition and storeless replay of immutable HTTP bodies.
use anyhow::{Result, ensure};
use clap::{Args, Subcommand};
use serde_json::json;
use std::{io::Write, path::PathBuf};
use yoke::{
    capture::Deadline,
    source_acquisition::{Limits, ReceivedSource},
    source_acquisition_http::{self, Permissions, Spec},
};

#[derive(Args)]
pub struct Arguments {
    #[command(subcommand)]
    operation: Operation,
}
#[derive(Subcommand)]
enum Operation {
    /// Preview one pinned native Drive request without reading a token or opening a socket.
    GoogleHttpPlan {
        #[arg(long)]
        config: PathBuf,
        #[arg(long)]
        source_root: PathBuf,
        #[arg(long)]
        allow_shortcut_read: bool,
        #[arg(long)]
        trust_native_exporter: bool,
        #[arg(long)]
        limits: Option<PathBuf>,
    },
    /// Export through fixed native HTTPS; the token is accepted only on private stdin.
    GoogleHttpCapture {
        #[arg(long)]
        config: PathBuf,
        #[arg(long)]
        source_root: PathBuf,
        #[arg(long)]
        output: PathBuf,
        #[arg(long)]
        allow_shortcut_read: bool,
        #[arg(long)]
        allow_network: bool,
        #[arg(long)]
        allow_private_credential: bool,
        #[arg(long)]
        trust_native_exporter: bool,
        #[arg(long)]
        access_token_stdin: bool,
        #[arg(long)]
        allow_output: bool,
        #[arg(long)]
        limits: Option<PathBuf>,
    },
    /// Explicitly export one hash-selected Google Workspace shortcut with a trusted host client.
    GoogleCapture {
        #[arg(long)]
        config: PathBuf,
        #[arg(long)]
        source_root: PathBuf,
        #[arg(long)]
        output: PathBuf,
        #[arg(long)]
        allow_shortcut_read: bool,
        #[arg(long)]
        allow_network: bool,
        #[arg(long)]
        allow_exporter_execution: bool,
        #[arg(long)]
        allow_client_state: bool,
        #[arg(long)]
        allow_output: bool,
        #[arg(long)]
        limits: Option<PathBuf>,
    },
    /// Replay received export bytes without a client, credentials, Store or network.
    GoogleRead {
        #[arg(long)]
        input: PathBuf,
        #[arg(long, requires = "end_byte", conflicts_with = "office_text")]
        start_byte: Option<usize>,
        #[arg(long, requires = "start_byte", conflicts_with = "office_text")]
        end_byte: Option<usize>,
        #[arg(long)]
        office_text: bool,
    },
    ArxivCapture {
        #[arg(long)]
        config: PathBuf,
        #[arg(long)]
        output: PathBuf,
        #[arg(long)]
        allow_network: bool,
        #[arg(long)]
        allow_output: bool,
        #[arg(long)]
        allow_rate_coordination: bool,
        #[arg(long)]
        limits: Option<PathBuf>,
    },
    ArxivRateStatus {
        #[arg(long)]
        coordination_file: PathBuf,
    },
    ArxivRateRecover {
        #[arg(long)]
        coordination_file: PathBuf,
        #[arg(long)]
        expected_hash: String,
        #[arg(long)]
        allow_rate_recovery: bool,
    },
    Capture {
        #[arg(long)]
        config: PathBuf,
        #[arg(long)]
        output: PathBuf,
        #[arg(long)]
        allow_network: bool,
        #[arg(long)]
        allow_output: bool,
        #[arg(long)]
        allow_loopback_http: bool,
        #[arg(long)]
        limits: Option<PathBuf>,
    },
    Read {
        #[arg(long)]
        input: PathBuf,
        #[arg(long, requires = "end_byte", conflicts_with_all = ["pdf_text","arxiv_metadata"])]
        start_byte: Option<usize>,
        #[arg(long, requires = "start_byte", conflicts_with_all = ["pdf_text","arxiv_metadata"])]
        end_byte: Option<usize>,
        /// Invoke only this installed native executable's bounded PDF parser.
        #[arg(long)]
        pdf_text: bool,
        #[arg(long, conflicts_with = "pdf_text")]
        arxiv_metadata: bool,
    },
}
struct HttpInput {
    root: PathBuf,
    path: PathBuf,
    shortcut: Vec<u8>,
    spec: yoke::google_workspace::HttpSpec,
    limits: yoke::google_workspace::Limits,
}
fn http_input(
    config: &std::path::Path,
    source_root: &std::path::Path,
    limits: Option<&std::path::PathBuf>,
    deadline: &Deadline,
) -> Result<HttpInput> {
    ensure!(
        source_root.is_absolute(),
        "google_workspace_absolute_source_root_required"
    );
    let raw = super::meaning_archive_cli::read_input(config, 131072, deadline)?;
    let spec: yoke::google_workspace::HttpSpec =
        yoke::json::parse(&raw, 131072).and_then(yoke::json::decode)?;
    let limits = if let Some(path) = limits {
        let raw = super::meaning_archive_cli::read_input(path, 16384, deadline)?;
        yoke::json::parse(&raw, 16384).and_then(yoke::json::decode)?
    } else {
        yoke::google_workspace::Limits::default()
    };
    let path = yoke::paths::scoped(source_root, &spec.shortcut_path)?;
    let shortcut = super::meaning_archive_cli::read_input(&path, 65536, deadline)?;
    Ok(HttpInput {
        root: std::fs::canonicalize(source_root)?,
        path,
        shortcut,
        spec,
        limits,
    })
}
pub fn run(args: &Arguments, timeout: u64, max_bytes: u64) -> Result<bool> {
    ensure!(
        (10..=600000).contains(&timeout) && (1024..=16_000_000).contains(&max_bytes),
        "source_acquisition_host_budget_invalid"
    );
    let deadline = Deadline::new(timeout);
    let cap = max_bytes as usize - 1;
    let mut committed_reply = None;
    let value = match &args.operation {
        Operation::GoogleHttpPlan {
            config,
            source_root,
            allow_shortcut_read,
            trust_native_exporter,
            limits,
        } => {
            ensure!(
                *allow_shortcut_read && *trust_native_exporter,
                "google_workspace_http_host_permission_required"
            );
            let input = http_input(config, source_root, limits.as_ref(), &deadline)?;
            yoke::google_workspace::plan_http(
                &input.root,
                &input.shortcut,
                &input.spec,
                &input.limits,
                &deadline,
            )?
        }
        Operation::GoogleHttpCapture {
            config,
            source_root,
            output,
            allow_shortcut_read,
            allow_network,
            allow_private_credential,
            trust_native_exporter,
            access_token_stdin,
            allow_output,
            limits,
        } => {
            ensure!(
                *allow_shortcut_read
                    && *allow_network
                    && *allow_private_credential
                    && *trust_native_exporter
                    && *access_token_stdin
                    && *allow_output,
                "google_workspace_http_host_permission_required"
            );
            let input = http_input(config, source_root, limits.as_ref(), &deadline)?;
            let _held = super::meaning_archive_cli::fresh_destination(output, &deadline)?;
            // Admit source, executable, archive and reply before accepting private input.
            yoke::google_workspace::plan_http(
                &input.root,
                &input.shortcut,
                &input.spec,
                &input.limits,
                &deadline,
            )?;
            let shape = |id: &str, bytes: usize| json!({"kind":"native_google_workspace_http_capture_v1","sourceId":id,"bodySha256":id,"archiveHash":id,"archiveBytes":bytes,"output":output,"outputCreated":true,"captureClaimsAreAuthority":false,"googleIdentityVerified":false,"accountIdentityVerified":false,"exporterHashMatchedAtCapture":true,"shortcutMatchedBeforeAndAfter":true,"transport":"native_reqwest_http1_fixed_google","requests":1,"redirectsFollowed":0,"proxyUsed":false,"credentialInput":"private_stdin","credentialsStored":false,"resourceKeyInMetadata":false,"libraryInternalAllocationsBounded":false,"modelCalls":0,"sourceExecuted":false,"complete":false});
            yoke::json::encode_bounded(&shape(&"f".repeat(64), usize::MAX), cap, &deadline)?;
            let token = yoke::google_workspace::read_private_token_stdin(&deadline)?;
            let source = yoke::google_workspace::capture_http(
                &input.root,
                &input.shortcut,
                &input.spec,
                &yoke::google_workspace::HttpPermissions {
                    allow_network: *allow_network,
                    allow_private_credential: *allow_private_credential,
                    trust_native_exporter: *trust_native_exporter,
                },
                &token,
                &input.limits,
                &deadline,
            )?;
            ensure!(
                super::meaning_archive_cli::read_input(&input.path, 65536, &deadline)?
                    == input.shortcut,
                "google_workspace_shortcut_changed_after_export"
            );
            let bytes = source.archive(&deadline)?;
            let summary = source.summary();
            let mut value = shape(summary["sourceId"].as_str().unwrap(), bytes.len());
            value["bodySha256"] = summary["bodySha256"].clone();
            value["archiveHash"] = json!(yoke::model::hash(&bytes));
            let reply = yoke::json::encode_bounded(&value, cap, &deadline)?;
            super::meaning_archive_cli::write_new(output, &bytes, &deadline)?;
            committed_reply = Some(reply);
            value
        }
        Operation::GoogleCapture {
            config,
            source_root,
            output,
            allow_shortcut_read,
            allow_network,
            allow_exporter_execution,
            allow_client_state,
            allow_output,
            limits,
        } => {
            ensure!(
                *allow_shortcut_read
                    && *allow_network
                    && *allow_exporter_execution
                    && *allow_client_state
                    && *allow_output,
                "google_workspace_host_permission_required"
            );
            ensure!(
                source_root.is_absolute(),
                "google_workspace_absolute_source_root_required"
            );
            let raw = super::meaning_archive_cli::read_input(config, 131072, &deadline)?;
            let spec: yoke::google_workspace::CaptureSpec =
                yoke::json::parse(&raw, 131072).and_then(yoke::json::decode)?;
            let limits = if let Some(path) = limits {
                let raw = super::meaning_archive_cli::read_input(path, 16384, &deadline)?;
                yoke::json::parse(&raw, 16384)
                    .and_then(yoke::json::decode::<yoke::google_workspace::Limits>)?
            } else {
                yoke::google_workspace::Limits::default()
            };
            limits.validate()?;
            // Keep host-selected ancestry visible to the no-link input reader.
            let path = yoke::paths::scoped(source_root, &spec.shortcut_path)?;
            let shortcut = super::meaning_archive_cli::read_input(&path, 65536, &deadline)?;
            let root = std::fs::canonicalize(source_root)?;
            let selected =
                yoke::google_workspace::Shortcut::parse(&spec.shortcut_path, &shortcut, &deadline)?;
            selected
                .metadata()
                .check_selection(&spec.expected_shortcut_sha256, &spec.expected_file_id)?;
            let _held = super::meaning_archive_cli::fresh_destination(output, &deadline)?;
            let shape = |id: &str, bytes: usize| {
                json!({"kind":"native_google_workspace_capture_v1",
                "sourceId":id,"bodySha256":id,"archiveHash":id,"archiveBytes":bytes,
                "output":output,"outputCreated":true,"captureClaimsAreAuthority":false,
                "googleIdentityVerified":false,"accountIdentityVerified":false,
                "exporterInvocations":2,"exporterHashMatchedAtCapture":true,
                "shortcutMatchedBeforeAndAfter":true,
                "modelCalls":0,"sourceExecuted":false,"complete":false})
            };
            yoke::json::encode_bounded(&shape(&"f".repeat(64), usize::MAX), cap, &deadline)?;
            let source = yoke::google_workspace::capture(
                &root,
                &shortcut,
                &spec,
                &yoke::google_workspace::Permissions {
                    allow_network: *allow_network,
                    allow_exporter_execution: *allow_exporter_execution,
                    allow_client_state: *allow_client_state,
                },
                &limits,
                &deadline,
            )?;
            // A completed export cannot silently publish against a changed selection.
            let after = super::meaning_archive_cli::read_input(&path, 65536, &deadline)?;
            ensure!(
                after == shortcut,
                "google_workspace_shortcut_changed_after_export"
            );
            let bytes = source.archive(&deadline)?;
            let inspected = source.summary();
            let mut value = shape(inspected["sourceId"].as_str().unwrap(), bytes.len());
            value["bodySha256"] = inspected["bodySha256"].clone();
            value["archiveHash"] = json!(yoke::model::hash(&bytes));
            let reply = yoke::json::encode_bounded(&value, cap, &deadline)?;
            super::meaning_archive_cli::write_new(output, &bytes, &deadline)?;
            committed_reply = Some(reply);
            value
        }
        Operation::GoogleRead {
            input,
            start_byte,
            end_byte,
            office_text,
        } => {
            let raw = super::meaning_archive_cli::read_input(input, 16_000_000, &deadline)?;
            let source = yoke::google_workspace::ReceivedExport::from_archive(
                &raw,
                &yoke::google_workspace::Limits::default(),
                &deadline,
            )?;
            if *office_text {
                source.inspect_office(&deadline)?
            } else if let (Some(start), Some(end)) = (start_byte, end_byte) {
                source.read(*start, *end, &deadline)?
            } else {
                source.summary()
            }
        }
        Operation::ArxivCapture {
            config,
            output,
            allow_network,
            allow_output,
            allow_rate_coordination,
            limits,
        } => {
            ensure!(
                *allow_network && *allow_output && *allow_rate_coordination,
                "arxiv_source_host_permission_required"
            );
            let raw = super::meaning_archive_cli::read_input(config, 131072, &deadline)?;
            let spec: yoke::arxiv_source::Spec =
                yoke::json::parse(&raw, 131072).and_then(yoke::json::decode)?;
            spec.validate()?;
            let limits = if let Some(path) = limits {
                let raw = super::meaning_archive_cli::read_input(path, 16384, &deadline)?;
                yoke::json::parse(&raw, 16384).and_then(yoke::json::decode::<Limits>)?
            } else {
                Limits::default()
            };
            limits.validate()?;
            let _held = super::meaning_archive_cli::fresh_destination(output, &deadline)?;
            let shape = |id: &str, bytes: usize| json!({"kind":"native_arxiv_source_capture","sourceId":id,"sourceHash":id,"archiveHash":id,"archiveBytes":bytes,"output":output,"outputCreated":true,"captureClaimsAreAuthority":false,"complete":false});
            yoke::json::encode_bounded(&shape(&"f".repeat(64), usize::MAX), cap, &deadline)?;
            let source = yoke::arxiv_source::capture(
                &spec,
                &yoke::arxiv_source::Permissions {
                    allow_network: *allow_network,
                    allow_rate_coordination: *allow_rate_coordination,
                },
                &limits,
                &deadline,
            )?;
            let bytes = source.encode_archive(&deadline)?;
            let inspected = source.as_value();
            let mut value = shape(inspected["sourceId"].as_str().unwrap(), bytes.len());
            value["sourceHash"] = inspected["sourceHash"].clone();
            value["archiveHash"] = json!(yoke::model::hash(&bytes));
            let reply = yoke::json::encode_bounded(&value, cap, &deadline)?;
            super::meaning_archive_cli::write_new(output, &bytes, &deadline)?;
            committed_reply = Some(reply);
            value
        }
        Operation::ArxivRateStatus { coordination_file } => {
            yoke::arxiv_source::rate_status(coordination_file, &deadline)?
        }
        Operation::ArxivRateRecover {
            coordination_file,
            expected_hash,
            allow_rate_recovery,
        } => {
            ensure!(
                *allow_rate_recovery,
                "arxiv_rate_recovery_permission_required"
            );
            yoke::json::encode_bounded(
                &json!({"kind":"native_arxiv_rate_recovery_v1","journalHash":"f".repeat(64),"sequence":u64::MAX,"completedAtMs":u64::MAX,"cooldownMs":3000,"coordinationUpdated":true,"networkConnections":0}),
                cap,
                &deadline,
            )?;
            let value = yoke::arxiv_source::recover_rate(
                coordination_file,
                true,
                expected_hash,
                &deadline,
            )?;
            let reply = serde_json::to_vec(&value).map_err(|_| {
                anyhow::anyhow!(
                    "arxiv_rate_recovery_completed_delivery_failed journalHash={}",
                    value["journalHash"].as_str().unwrap()
                )
            })?;
            ensure!(
                reply.len() <= cap,
                "arxiv_rate_recovery_completed_delivery_failed journalHash={}",
                value["journalHash"].as_str().unwrap()
            );
            committed_reply = Some(reply);
            value
        }
        Operation::Capture {
            config,
            output,
            allow_network,
            allow_output,
            allow_loopback_http,
            limits,
        } => {
            ensure!(
                *allow_network && *allow_output,
                "source_acquisition_host_permission_required"
            );
            let raw = super::meaning_archive_cli::read_input(config, 131072, &deadline)?;
            let spec: Spec = yoke::json::parse(&raw, 131072).and_then(yoke::json::decode)?;
            spec.validate()?;
            let limits = if let Some(path) = limits {
                let raw = super::meaning_archive_cli::read_input(path, 16384, &deadline)?;
                yoke::json::parse(&raw, 16384).and_then(yoke::json::decode::<Limits>)?
            } else {
                Limits::default()
            };
            limits.validate()?;
            let _held = super::meaning_archive_cli::fresh_destination(output, &deadline)?;
            let shape = |id: &str, bytes: usize| json!({"kind":"native_http_source_capture","sourceId":id,"sourceHash":id,"archiveHash":id,"archiveBytes":bytes,"output":output,"outputCreated":true,"captureClaimsAreAuthority":false,"complete":false});
            yoke::json::encode_bounded(&shape(&"f".repeat(64), usize::MAX), cap, &deadline)?;
            let source = source_acquisition_http::capture(
                &spec,
                &Permissions {
                    allow_network: *allow_network,
                    allow_loopback_http: *allow_loopback_http,
                },
                &limits,
                &deadline,
            )?;
            let bytes = source.encode_archive(&deadline)?;
            let inspected = source.as_value();
            let mut value = shape(inspected["sourceId"].as_str().unwrap(), bytes.len());
            value["sourceHash"] = inspected["sourceHash"].clone();
            value["archiveHash"] = json!(yoke::model::hash(&bytes));
            let reply = yoke::json::encode_bounded(&value, cap, &deadline)?;
            super::meaning_archive_cli::write_new(output, &bytes, &deadline)?;
            committed_reply = Some(reply);
            value
        }
        Operation::Read {
            input,
            start_byte,
            end_byte,
            pdf_text,
            arxiv_metadata,
        } => {
            let raw = super::meaning_archive_cli::read_input(input, 16_000_000, &deadline)?;
            let source = ReceivedSource::load_archive(&raw, &deadline)?;
            if *pdf_text {
                source.pdf_text(&std::env::current_exe()?, cap, &deadline)?
            } else if *arxiv_metadata {
                yoke::arxiv_source::inspect(&source, cap, &deadline)?
            } else if let (Some(start), Some(end)) = (start_byte, end_byte) {
                source.read_range(*start, *end, cap, &deadline)?
            } else {
                source.as_value()
            }
        }
    };
    let mut bytes = if let Some(reply) = committed_reply {
        reply
    } else {
        yoke::json::encode_bounded(&value, cap, &deadline)?
    };
    bytes.push(b'\n');
    if std::io::stdout().lock().write_all(&bytes).is_err() {
        if value["outputCreated"] == true {
            anyhow::bail!(
                "source_acquisition_delivery_failed_after_publication sourceId={} archiveHash={}",
                value["sourceId"].as_str().unwrap(),
                value["archiveHash"].as_str().unwrap()
            );
        }
        if value["coordinationUpdated"] == true {
            anyhow::bail!(
                "arxiv_rate_recovery_completed_delivery_failed journalHash={}",
                value["journalHash"].as_str().unwrap()
            );
        }
        anyhow::bail!("source_acquisition_output_delivery_failed");
    }
    Ok(true)
}
