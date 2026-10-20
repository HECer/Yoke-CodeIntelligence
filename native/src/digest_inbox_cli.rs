//! Received metadata reads use explicit host inputs, never an Engine or sender key.
use anyhow::{Result, ensure};
use clap::{Args, Subcommand};
use std::{
    io::Write,
    path::{Path, PathBuf},
};
use yoke::{
    capture::Deadline,
    github_app::{
        HostProfile,
        digest::service::{
            self,
            receiver::{self, interpretation, query, verification},
        },
    },
    store::Store,
};
#[derive(Args)]
pub struct Arguments {
    #[command(subcommand)]
    command: Operation,
}
#[derive(Subcommand)]
enum Operation {
    Local(Local),
    Remote(Remote),
    Verify(LocalVerification),
    Interpret(LocalInterpretation),
}
#[derive(Args)]
struct Local {
    #[arg(long)]
    profile: PathBuf,
    #[arg(long)]
    principal: String,
    #[arg(long)]
    request: PathBuf,
    #[arg(long)]
    allow_query: bool,
    #[arg(long, default_value_t = 32_000)]
    max_visited: usize,
    #[arg(long, default_value_t = 32_000_000)]
    max_search_bytes: usize,
}
#[derive(Args)]
struct Remote {
    #[arg(long)]
    profile: PathBuf,
    #[arg(long)]
    app_profile: PathBuf,
    #[arg(long)]
    request: PathBuf,
    #[arg(long)]
    ca: Option<PathBuf>,
    #[arg(long)]
    allow_query: bool,
    #[arg(long)]
    allow_network: bool,
}
#[derive(Args)]
struct LocalVerification {
    #[arg(long)]
    profile: PathBuf,
    #[arg(long)]
    principal: String,
    #[arg(long)]
    request: PathBuf,
    #[arg(long)]
    source_root: PathBuf,
    #[arg(long)]
    source_profile: PathBuf,
    #[arg(long)]
    allow_query: bool,
    #[arg(long)]
    allow_source_verification: bool,
    #[arg(long)]
    require_git_identity: bool,
    #[arg(long, default_value_t = 4000)]
    max_symbols: usize,
    #[arg(long, default_value_t = 32_000_000)]
    max_source_bytes: usize,
}
#[derive(Args)]
struct LocalInterpretation {
    #[command(flatten)]
    source: LocalVerification,
    #[arg(long)]
    allow_source_interpretation: bool,
    #[arg(long, default_value_t = 32_000_000)]
    max_meaning_input_bytes: usize,
}
fn verification_flags(a: &LocalVerification) -> Result<()> {
    ensure!(
        a.allow_query && a.allow_source_verification,
        "github_app_digest_verification_permission_required"
    );
    ensure!(
        a.source_root.is_absolute()
            && (1..=4000).contains(&a.max_symbols)
            && a.max_source_bytes <= 32_000_000,
        "github_app_digest_verification_input_invalid"
    );
    Ok(())
}
fn bytes(path: &Path, cap: usize, d: &Deadline) -> Result<Vec<u8>> {
    ensure!(path.is_absolute(), "github_app_digest_query_input_invalid");
    crate::feedback_cli::host_bytes(path, cap, d)
        .map_err(|_| anyhow::anyhow!("github_app_digest_query_input_invalid"))
}
impl Arguments {
    pub fn run(&self, root: &Path, timeout: u64, max_bytes: u64) -> Result<bool> {
        // Permission precedes file/Store/credential acquisition.
        match &self.command {
            Operation::Interpret(a) => {
                ensure!(
                    a.allow_source_interpretation
                        && a.source.allow_query
                        && a.source.allow_source_verification,
                    "github_app_digest_interpretation_permission_required"
                );
                verification_flags(&a.source)?;
                ensure!(
                    (1..=32_000_000).contains(&a.max_meaning_input_bytes),
                    "github_app_digest_interpretation_capacity_exceeded"
                );
            }
            Operation::Verify(a) => verification_flags(a)?,
            Operation::Local(a) => {
                ensure!(a.allow_query, "github_app_digest_query_permission_required")
            }
            Operation::Remote(a) => ensure!(
                a.allow_query && a.allow_network,
                "github_app_digest_query_permission_required"
            ),
        }
        let max_allowed = match self.command {
            Operation::Interpret(_) => 64_000_000,
            Operation::Verify(_) => 16_000_000,
            _ => 65_536,
        };
        ensure!(
            (10..=600_000).contains(&timeout) && (1024..=max_allowed).contains(&max_bytes),
            "github_app_digest_query_capacity_exceeded"
        );
        let d = Deadline::new(timeout);
        let value = match &self.command {
            Operation::Interpret(a) => {
                let source = &a.source;
                let profile = receiver::Profile::parse(&bytes(&source.profile, 32_000, &d)?)?;
                let source_profile =
                    HostProfile::parse(&bytes(&source.source_profile, 32_000, &d)?)?;
                let request = interpretation::Request::parse(&bytes(&source.request, 16_384, &d)?)?;
                let verification_options = verification::Options {
                    allow_query: source.allow_query,
                    allow_verification: source.allow_source_verification,
                    require_git_identity: source.require_git_identity,
                    max_symbols: source.max_symbols,
                    max_output_bytes: max_bytes.min(16_000_000) as usize,
                    ..Default::default()
                };
                let options = interpretation::Options {
                    allow_source_interpretation: a.allow_source_interpretation,
                    max_input_bytes: a.max_meaning_input_bytes,
                    max_output_bytes: max_bytes as usize,
                    ..Default::default()
                };
                interpretation::admit(
                    &profile,
                    &source.principal,
                    &source_profile,
                    &request,
                    &verification_options,
                    &options,
                    &d,
                )?;
                let source_options = yoke::github_app::digest::Options {
                    max_source_bytes: source.max_source_bytes,
                    ..Default::default()
                };
                source_options.validate()?;
                let receiver = Store::open_existing_read_only(root, &d)?;
                let local = Store::open_existing_read_only(&source.source_root, &d)?;
                interpretation::interpret(
                    &verification::Context {
                        receiver_store: &receiver,
                        receiver_profile: &profile,
                        principal: &source.principal,
                        source_store: &local,
                        source_profile: &source_profile,
                        source_options: &source_options,
                        options: &verification_options,
                    },
                    &request,
                    &options,
                    &d,
                )?
                .export(max_bytes as usize, &d)?
            }
            Operation::Verify(a) => {
                let profile = receiver::Profile::parse(&bytes(&a.profile, 32_000, &d)?)?;
                let source_profile = HostProfile::parse(&bytes(&a.source_profile, 32_000, &d)?)?;
                let request = verification::Request::parse(&bytes(&a.request, 8192, &d)?)?;
                let options = verification::Options {
                    allow_query: a.allow_query,
                    allow_verification: a.allow_source_verification,
                    require_git_identity: a.require_git_identity,
                    max_symbols: a.max_symbols,
                    max_output_bytes: max_bytes as usize,
                    ..Default::default()
                };
                verification::admit(
                    &profile,
                    &a.principal,
                    &source_profile,
                    &request,
                    &options,
                    &d,
                )?;
                let source_options = yoke::github_app::digest::Options {
                    max_source_bytes: a.max_source_bytes,
                    ..Default::default()
                };
                source_options.validate()?;
                let receiver = Store::open_existing_read_only(root, &d)?;
                let source = Store::open_existing_read_only(&a.source_root, &d)?;
                verification::verify(
                    &verification::Context {
                        receiver_store: &receiver,
                        receiver_profile: &profile,
                        principal: &a.principal,
                        source_store: &source,
                        source_profile: &source_profile,
                        source_options: &source_options,
                        options: &options,
                    },
                    &request,
                    &d,
                )?
                .export(max_bytes as usize, &d)?
            }
            Operation::Local(a) => {
                let profile = receiver::Profile::parse(&bytes(&a.profile, 32_000, &d)?)?;
                let request = query::Request::parse(&bytes(&a.request, 8192, &d)?)?;
                let options = query::Options {
                    allow_query: a.allow_query,
                    max_visited: a.max_visited,
                    max_search_bytes: a.max_search_bytes,
                    max_output_bytes: max_bytes as usize,
                };
                // Validate all host inputs and scope before selecting existing state.
                query::admit_local(&profile, &a.principal, &request, &options, &d)?;
                let store = Store::open_existing_read_only(root, &d)?;
                query::local(&store, &profile, &a.principal, &request, &options, &d)?
            }
            Operation::Remote(a) => {
                ensure!(
                    root == Path::new("."),
                    "github_app_digest_query_remote_does_not_select_source"
                );
                let profile = service::Profile::parse(&bytes(&a.profile, 32_000, &d)?)?;
                let app = HostProfile::parse(&bytes(&a.app_profile, 32_000, &d)?)?;
                let request = query::Request::parse(&bytes(&a.request, 8192, &d)?)?;
                let ca = a.ca.as_ref().map(|p| bytes(p, 32_000, &d)).transpose()?;
                query::client::remote(
                    &profile,
                    &app,
                    &request,
                    &query::client::Options {
                        allow_query: a.allow_query,
                        allow_network: a.allow_network,
                        max_output_bytes: max_bytes as usize,
                        ..Default::default()
                    },
                    ca.as_deref(),
                    &d,
                )?
            }
        };
        let mut raw = yoke::json::encode_bounded(&value, max_bytes as usize - 1, &d)?;
        raw.push(b'\n');
        d.check()?;
        std::io::stdout().lock().write_all(&raw)?;
        Ok(true)
    }
}
