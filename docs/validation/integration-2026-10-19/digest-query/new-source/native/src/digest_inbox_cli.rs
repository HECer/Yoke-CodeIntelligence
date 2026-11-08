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
            receiver::{self, query},
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
fn bytes(path: &Path, cap: usize, d: &Deadline) -> Result<Vec<u8>> {
    ensure!(path.is_absolute(), "github_app_digest_query_input_invalid");
    crate::feedback_cli::host_bytes(path, cap, d)
        .map_err(|_| anyhow::anyhow!("github_app_digest_query_input_invalid"))
}
impl Arguments {
    pub fn run(&self, root: &Path, timeout: u64, max_bytes: u64) -> Result<bool> {
        // Permission precedes file/Store/credential acquisition.
        match &self.command {
            Operation::Local(a) => {
                ensure!(a.allow_query, "github_app_digest_query_permission_required")
            }
            Operation::Remote(a) => ensure!(
                a.allow_query && a.allow_network,
                "github_app_digest_query_permission_required"
            ),
        }
        ensure!(
            (10..=600_000).contains(&timeout) && (1024..=65_536).contains(&max_bytes),
            "github_app_digest_query_capacity_exceeded"
        );
        let d = Deadline::new(timeout);
        let value = match &self.command {
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
