//! Explicit local source-read admission without App keys or provider traffic.
use anyhow::{Result, ensure};
use clap::{Args, Subcommand};
use std::{
    io::Write,
    path::{Path, PathBuf},
};
use yoke::{
    capture::Deadline,
    github_app::{HostProfile, digest},
    store::Store,
};

const INVALID: &str = "github_app_digest_input_invalid";
const CAPACITY: &str = "github_app_digest_capacity_exceeded";
#[derive(Args)]
pub struct Arguments {
    #[command(subcommand)]
    operation: Operation,
    #[arg(long, global = true)]
    allow_digest: bool,
    #[arg(long, global = true, default_value_t = 4000)]
    max_symbols: usize,
    #[arg(long, global = true, default_value_t = 32_000_000)]
    max_source_bytes: usize,
    #[arg(long, global = true, default_value_t = 64_000_000)]
    max_metadata_bytes: usize,
    #[arg(long, global = true, default_value_t = 16_000_000)]
    max_lineage_metadata_bytes: usize,
}
#[derive(Subcommand)]
enum Operation {
    Preview {
        #[arg(long)]
        request: Option<PathBuf>,
    },
    History(HistoryArguments),
}
#[derive(Args)]
struct HistoryArguments {
    #[arg(long)]
    request: Option<PathBuf>,
    #[arg(long)]
    allow_history: bool,
    #[arg(long)]
    trust_git: bool,
    #[arg(long)]
    git: Option<PathBuf>,
    #[arg(long, default_value_t = 1000)]
    max_commits: usize,
    #[arg(long, default_value_t = 4000)]
    max_visits: usize,
    #[arg(long, default_value_t = 4000)]
    max_frontier: usize,
    #[arg(long, default_value_t = 16_000)]
    max_parent_ids: usize,
    #[arg(long, default_value_t = 1_000_000)]
    max_commit_bytes: usize,
    #[arg(long, default_value_t = 64_000_000)]
    max_history_bytes: usize,
}
impl HistoryArguments {
    fn options(&self) -> digest::HistoryOptions {
        digest::HistoryOptions {
            max_commits: self.max_commits,
            max_visits: self.max_visits,
            max_frontier: self.max_frontier,
            max_parent_ids: self.max_parent_ids,
            max_commit_bytes: self.max_commit_bytes,
            max_history_bytes: self.max_history_bytes,
        }
    }
}
pub struct Prepared {
    profile: HostProfile,
    request: digest::PreviewRequest,
    options: digest::Options,
    output_bytes: usize,
    history: Option<(yoke::git_workflows::HostGit, digest::HistoryOptions)>,
}
fn public_error(d: &Deadline, marker: &str) -> anyhow::Error {
    d.check()
        .err()
        .unwrap_or_else(|| anyhow::anyhow!(marker.to_owned()))
}
fn bytes(path: &Path, maximum: usize, d: &Deadline) -> Result<Vec<u8>> {
    ensure!(path.is_absolute(), INVALID);
    crate::feedback_cli::host_bytes(path, maximum, d).map_err(|_| public_error(d, INVALID))
}
impl Arguments {
    pub fn check_permission(&self, webhooks: bool, network: bool, writes: bool) -> Result<()> {
        ensure!(self.allow_digest, "github_app_digest_permission_required");
        ensure!(!webhooks && !network && !writes, INVALID);
        if let Operation::History(history) = &self.operation {
            ensure!(
                history.allow_history,
                "github_app_digest_history_permission_required"
            );
            ensure!(
                history.trust_git,
                "github_app_digest_history_trust_required"
            );
        }
        Ok(())
    }
    pub fn prepare(
        &self,
        profile: Option<&Path>,
        max_bytes: u64,
        d: &Deadline,
    ) -> Result<Prepared> {
        let hard = digest::Options::default();
        ensure!(
            max_bytes > 0
                && max_bytes <= hard.max_output_bytes as u64
                && self.max_symbols <= hard.max_symbols
                && self.max_source_bytes <= hard.max_source_bytes
                && self.max_metadata_bytes <= hard.max_metadata_bytes
                && self.max_lineage_metadata_bytes <= hard.max_lineage_metadata_bytes,
            CAPACITY
        );
        let (request, history) = match &self.operation {
            Operation::Preview { request } => (request, None),
            Operation::History(history) => {
                history.options().validate()?;
                ensure!(
                    history.git.as_deref().is_none_or(Path::is_absolute),
                    INVALID
                );
                (&history.request, Some(history))
            }
        };
        let profile_path = profile.ok_or_else(|| anyhow::anyhow!(INVALID))?;
        let request_path = request.as_deref().ok_or_else(|| anyhow::anyhow!(INVALID))?;
        // Admit both paths before reading either host-selected document.
        ensure!(
            profile_path.is_absolute() && request_path.is_absolute(),
            INVALID
        );
        let profile = HostProfile::parse(&bytes(profile_path, 32_000, d)?)
            .map_err(|_| public_error(d, INVALID))?;
        let request = digest::PreviewRequest::parse(&bytes(request_path, 4096, d)?)
            .map_err(|_| public_error(d, INVALID))?;
        digest::authorize_profile(&profile, &request)?;
        let history = history
            .map(|history| -> Result<_> {
                let git = crate::selected_git(history.git.clone(), history.trust_git)
                    .map_err(|_| public_error(d, INVALID))?;
                Ok((git, history.options()))
            })
            .transpose()?;
        d.check()?;
        Ok(Prepared {
            profile,
            request,
            options: digest::Options {
                max_symbols: self.max_symbols,
                max_source_bytes: self.max_source_bytes,
                max_metadata_bytes: self.max_metadata_bytes,
                max_lineage_metadata_bytes: self.max_lineage_metadata_bytes,
                max_output_bytes: max_bytes as usize - 1,
                ..hard
            },
            output_bytes: max_bytes as usize,
            history,
        })
    }
}
impl Prepared {
    pub fn run(self, root: &Path, d: &Deadline) -> Result<bool> {
        let store = Store::open_existing_read_only(root, d)
            .map_err(|_| public_error(d, "github_app_digest_store_unavailable"))?;
        let value = if let Some((git, history_options)) = &self.history {
            digest::preview_history_with_profile(
                &store,
                &self.profile,
                &self.options,
                &self.request,
                git,
                history_options,
                d,
            )?
        } else {
            digest::preview_with_profile(&store, &self.profile, &self.options, &self.request, d)?
        };
        let mut raw = yoke::json::encode_bounded(&value, self.output_bytes - 1, d)
            .map_err(|_| public_error(d, CAPACITY))?;
        raw.push(b'\n');
        d.check()?;
        let mut stdout = std::io::stdout().lock();
        stdout.write_all(&raw)?;
        stdout.flush()?;
        Ok(true)
    }
}
