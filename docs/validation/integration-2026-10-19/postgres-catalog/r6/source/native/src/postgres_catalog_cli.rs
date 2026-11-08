//! Explicit host capture, create-only publication, and frozen data queries.
use anyhow::{Result, ensure};
use clap::{Args, Subcommand};
use serde_json::json;
use std::{
    io::{IsTerminal, Read, Write},
    path::PathBuf,
};
use yoke::{
    capture::Deadline,
    postgres_catalog::{Catalog, Limits, Query},
    postgres_catalog_client::{self, Permissions, Spec},
};

#[derive(Args)]
pub struct Arguments {
    #[command(subcommand)]
    operation: Operation,
}
#[derive(Subcommand)]
enum Operation {
    /// Connect once with a host-trusted psql18 and fixed read-only catalog SQL.
    Capture {
        #[arg(long)]
        config: PathBuf,
        #[arg(long)]
        output: PathBuf,
        #[arg(long)]
        trust_client: bool,
        #[arg(long)]
        allow_network: bool,
        #[arg(long)]
        allow_loopback_trust: bool,
        #[arg(long)]
        allow_output: bool,
        #[arg(long)]
        limits: Option<PathBuf>,
    },
    /// Query only a supplied immutable catalog artifact. Never runs psql.
    Read {
        #[arg(long)]
        input: PathBuf,
        #[arg(long)]
        query: Option<String>,
    },
    /// Import exact private stdin bytes into the endpoint-scoped OS store.
    ImportPassword {
        #[arg(long)]
        config: PathBuf,
        #[arg(long)]
        allow_credential_create: bool,
    },
}
fn spec(path: &std::path::Path, d: &Deadline) -> Result<Spec> {
    let raw = super::meaning_archive_cli::read_input(path, 131072, d)?;
    let spec: Spec = yoke::json::decode(yoke::json::parse(&raw, 131072)?)?;
    spec.validate()?;
    Ok(spec)
}
pub fn run(args: &Arguments, timeout: u64, max_bytes: u64) -> Result<bool> {
    ensure!(
        (10..=600000).contains(&timeout) && (1024..=32_000_000).contains(&max_bytes),
        "postgres_catalog_host_budget_invalid"
    );
    let d = Deadline::new(timeout);
    let cap = max_bytes as usize - 1;
    let value = match &args.operation {
        Operation::Capture {
            config,
            output,
            trust_client,
            allow_network,
            allow_loopback_trust,
            allow_output,
            limits,
        } => {
            ensure!(
                *trust_client && *allow_network && *allow_output,
                "postgres_catalog_host_permission_required"
            );
            let limits = if let Some(path) = limits {
                let raw = super::meaning_archive_cli::read_input(path, 16384, &d)?;
                yoke::json::decode::<Limits>(yoke::json::parse(&raw, 16384)?)?
            } else {
                Limits::default()
            };
            limits.validate()?;
            let spec = spec(config, &d)?;
            let _held = super::meaning_archive_cli::fresh_destination(output, &d)?;
            // Admit the complete delivery metadata before execution or publication.
            let shape = |id: &str, bytes: usize| json!({"kind":"native_postgres_catalog_capture","catalogId":id,"archiveHash":id,"archiveBytes":bytes,"output":output,"outputCreated":true,"complete":false,"serverAttestation":"unverified","captureClaimsAreAuthority":false});
            yoke::json::encode_bounded(&shape(&"f".repeat(64), usize::MAX), cap, &d)?;
            let catalog = postgres_catalog_client::receive(
                &spec,
                &Permissions {
                    trust_client: *trust_client,
                    allow_network: *allow_network,
                    allow_loopback_trust: *allow_loopback_trust,
                },
                &limits,
                &d,
            )?;
            let bytes = catalog.encode_archive(&d)?;
            let value = shape(
                catalog.as_value()["catalogId"].as_str().unwrap(),
                bytes.len(),
            );
            let mut value = value;
            value["archiveHash"] = json!(yoke::model::hash(&bytes));
            yoke::json::encode_bounded(&value, cap, &d)?;
            super::meaning_archive_cli::write_new(output, &bytes, &d)?;
            value
        }
        Operation::Read { input, query } => {
            let query = query
                .as_ref()
                .map(|q| {
                    yoke::json::parse(q.as_bytes(), 16384).and_then(yoke::json::decode::<Query>)
                })
                .transpose()?
                .unwrap_or(Query::Inspect);
            let bytes = super::meaning_archive_cli::read_input(input, 32_000_000, &d)?;
            let catalog = Catalog::load_archive(&bytes, &Limits::default(), &d)?;
            catalog.query(&query, cap, &d)?
        }
        Operation::ImportPassword {
            config,
            allow_credential_create,
        } => {
            ensure!(
                *allow_credential_create,
                "postgres_catalog_credential_permission_required"
            );
            ensure!(
                !std::io::stdin().is_terminal(),
                "postgres_catalog_private_stdin_required"
            );
            let binding = spec(config, &d)?.credential_binding()?;
            yoke::json::encode_bounded(
                &json!({"credentialBinding":binding,"credentialType":"postgres","privateMaterial":"os_credential_store_only"}),
                cap,
                &d,
            )?;
            let mut bytes = zeroize::Zeroizing::new(Vec::new());
            std::io::stdin().take(2561).read_to_end(&mut bytes)?;
            d.check()?;
            yoke::host_credentials::import_postgres_password(&binding, &bytes)?
        }
    };
    let mut bytes = yoke::json::encode_bounded(&value, cap, &d)?;
    bytes.push(b'\n');
    d.check()?;
    std::io::stdout().lock().write_all(&bytes)?;
    Ok(true)
}
