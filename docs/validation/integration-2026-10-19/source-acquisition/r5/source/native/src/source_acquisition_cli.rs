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
        #[arg(long, requires = "end_byte", conflicts_with = "pdf_text")]
        start_byte: Option<usize>,
        #[arg(long, requires = "start_byte", conflicts_with = "pdf_text")]
        end_byte: Option<usize>,
        /// Invoke only this installed native executable's bounded PDF parser.
        #[arg(long)]
        pdf_text: bool,
    },
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
        } => {
            let raw = super::meaning_archive_cli::read_input(input, 16_000_000, &deadline)?;
            let source = ReceivedSource::load_archive(&raw, &deadline)?;
            if *pdf_text {
                source.pdf_text(&std::env::current_exe()?, cap, &deadline)?
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
        anyhow::bail!("source_acquisition_output_delivery_failed");
    }
    Ok(true)
}
