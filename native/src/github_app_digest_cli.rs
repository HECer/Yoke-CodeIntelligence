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
    #[arg(long, global = true)]
    discussion_capture: Option<PathBuf>,
    #[arg(long, global = true)]
    allow_discussions: bool,
    #[arg(long, global = true)]
    allow_fixture_discussions: bool,
    #[arg(long, global = true)]
    handoff_preview: bool,
    #[arg(long, global = true)]
    service_profile: Option<PathBuf>,
    #[arg(long, global = true)]
    service_approval: Option<PathBuf>,
    #[arg(long, global = true)]
    service_ca: Option<PathBuf>,
    #[arg(long, global = true)]
    allow_service: bool,
    #[arg(long, global = true)]
    allow_service_network: bool,
    #[arg(long, global = true)]
    allow_fixture_service: bool,
}
#[derive(Subcommand)]
enum Operation {
    Preview {
        #[arg(long)]
        request: Option<PathBuf>,
    },
    History(HistoryArguments),
    ServiceStatus {
        #[arg(long)]
        request_id: String,
    },
    ServiceRecover {
        #[arg(long)]
        request_id: String,
    },
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
    /// Map already captured source snapshots; never capture historical sources here.
    #[arg(long)]
    map_captured_history: bool,
    /// Additional immutable captured Git snapshot ID (the selected head is implicit).
    #[arg(long = "captured-snapshot", requires = "map_captured_history")]
    captured_snapshots: Vec<String>,
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
    request: Option<digest::PreviewRequest>,
    options: digest::Options,
    output_bytes: usize,
    discussions: Option<yoke::github::discussions::Capture>,
    discussion_permission: digest::discussions::Permission,
    service: Option<digest::service::Profile>,
    approval: Option<digest::service::Approval>,
    service_options: digest::service::Options,
    service_query: Option<(String, bool)>,
    service_ca: Option<Vec<u8>>,
    history: Option<(
        yoke::git_workflows::HostGit,
        digest::HistoryOptions,
        Option<digest::MappingSelection>,
    )>,
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
        ensure!(!webhooks && !network, INVALID);
        let service_query = matches!(
            self.operation,
            Operation::ServiceStatus { .. } | Operation::ServiceRecover { .. }
        );
        let remote = self.service_approval.is_some()
            || matches!(self.operation, Operation::ServiceRecover { .. });
        ensure!(
            self.service_profile.is_some() == self.allow_service
                && writes == remote
                && self.allow_service_network == remote
                && (!self.allow_fixture_service || self.allow_service)
                && (!remote || self.allow_service)
                && (self.service_ca.is_none() || remote)
                && (!service_query
                    || (self.allow_service
                        && self.service_approval.is_none()
                        && self.discussion_capture.is_none()
                        && !self.handoff_preview))
                && (self.service_profile.is_none()
                    || service_query
                    || self.discussion_capture.is_some()),
            INVALID
        );
        ensure!(
            self.discussion_capture.is_some() == self.allow_discussions
                && (self.discussion_capture.is_some()
                    || (!self.allow_fixture_discussions && !self.handoff_preview)),
            INVALID
        );
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
            Operation::ServiceStatus { .. } | Operation::ServiceRecover { .. } => (&None, None),
        };
        let profile_path = profile.ok_or_else(|| anyhow::anyhow!(INVALID))?;
        let request_path = request.as_deref();
        let service_query = match &self.operation {
            Operation::ServiceStatus { request_id } => Some((request_id.clone(), false)),
            Operation::ServiceRecover { request_id } => Some((request_id.clone(), true)),
            _ => None,
        };
        ensure!(request_path.is_some() || service_query.is_some(), INVALID);
        if let Some((request, _)) = &service_query {
            digest::service::admit_status_request(request)?;
        }
        // Admit both paths before reading either host-selected document.
        ensure!(
            profile_path.is_absolute()
                && request_path.is_none_or(Path::is_absolute)
                && self
                    .service_profile
                    .as_deref()
                    .is_none_or(Path::is_absolute)
                && self
                    .service_approval
                    .as_deref()
                    .is_none_or(Path::is_absolute)
                && self.service_ca.as_deref().is_none_or(Path::is_absolute)
                && self
                    .discussion_capture
                    .as_deref()
                    .is_none_or(Path::is_absolute),
            INVALID
        );
        let profile = HostProfile::parse(&bytes(profile_path, 32_000, d)?)
            .map_err(|_| public_error(d, INVALID))?;
        let request = request_path
            .map(|p| -> Result<_> {
                digest::PreviewRequest::parse(&bytes(p, 4096, d)?)
                    .map_err(|_| public_error(d, INVALID))
            })
            .transpose()?;
        if let Some(request) = &request {
            digest::authorize_profile(&profile, request)?;
        }
        let service = self
            .service_profile
            .as_deref()
            .map(|p| -> Result<_> { digest::service::Profile::parse(&bytes(p, 32_000, d)?) })
            .transpose()?;
        let approval = self
            .service_approval
            .as_deref()
            .map(|p| -> Result<_> { digest::service::Approval::parse(&bytes(p, 4096, d)?) })
            .transpose()?;
        ensure!(
            self.service_ca.is_none()
                || service
                    .as_ref()
                    .is_some_and(|s| s.endpoint.starts_with("https:")),
            INVALID
        );
        let service_ca = self
            .service_ca
            .as_deref()
            .map(|p| bytes(p, 32_000, d))
            .transpose()?;
        let discussion_permission = digest::discussions::Permission {
            allow_discussions: self.allow_discussions,
            allow_fixture: self.allow_fixture_discussions,
            handoff_preview: self.handoff_preview,
        };
        let discussions = self
            .discussion_capture
            .as_deref()
            .map(|path| -> Result<_> {
                let capture = yoke::github::discussions::Capture::parse(
                    &bytes(
                        path,
                        yoke::github::discussions::Profile::default().max_output_bytes,
                        d,
                    )?,
                    d,
                )?;
                digest::discussions::authorize(
                    &profile,
                    request.as_ref().ok_or_else(|| anyhow::anyhow!(INVALID))?,
                    &capture,
                    &discussion_permission,
                    d,
                )?;
                Ok(capture)
            })
            .transpose()?;
        let history = history
            .map(|history| -> Result<_> {
                let selection = if history.map_captured_history {
                    let selection = digest::MappingSelection {
                        snapshot_ids: history.captured_snapshots.clone(),
                        limits: digest::MappingOptions::default(),
                    };
                    selection
                        .validate(request.as_ref().ok_or_else(|| anyhow::anyhow!(INVALID))?)?;
                    Some(selection)
                } else {
                    None
                };
                let git = crate::selected_git(history.git.clone(), history.trust_git)
                    .map_err(|_| public_error(d, INVALID))?;
                Ok((git, history.options(), selection))
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
                max_output_bytes: if service.is_some() {
                    hard.max_output_bytes
                } else {
                    max_bytes as usize - 1
                },
                ..hard
            },
            output_bytes: max_bytes as usize,
            discussions,
            discussion_permission,
            service,
            approval,
            service_options: digest::service::Options {
                allow_service: self.allow_service,
                allow_network: self.allow_service_network,
                allow_writes: self.service_approval.is_some()
                    || service_query.as_ref().is_some_and(|(_, recover)| *recover),
                allow_fixture: self.allow_fixture_service,
                max_output_bytes: max_bytes as usize,
            },
            service_query,
            service_ca,
            history,
        })
    }
}
impl Prepared {
    pub fn run(self, root: &Path, d: &Deadline) -> Result<bool> {
        let store = if self.service_options.allow_writes {
            crate::validate_existing_batch_store(root, d)
                .map_err(|_| public_error(d, "github_app_digest_store_unavailable"))?;
            Store::open_with_timeout(root, d.remaining())
                .map_err(|_| public_error(d, "github_app_digest_store_unavailable"))?
        } else {
            Store::open_existing_read_only(root, d)
                .map_err(|_| public_error(d, "github_app_digest_store_unavailable"))?
        };
        let value = if let Some((request, recover)) = &self.service_query {
            let host = yoke::github_app::Host::from_os(self.profile.clone())?;
            let context = digest::service::Context {
                store: &store,
                host: &host,
                profile: self
                    .service
                    .as_ref()
                    .ok_or_else(|| anyhow::anyhow!(INVALID))?,
                options: &self.service_options,
            };
            if *recover {
                let client = self.service_client(&context, d)?;
                digest::service::recover(&context, request, &client, d)?
            } else {
                digest::service::status(&context, request, d)?
            }
        } else if let Some(capture) = &self.discussions {
            let source = if let Some((git, options, selection)) = &self.history {
                digest::discussions::Source::History {
                    git,
                    options,
                    mapping: selection.as_ref(),
                }
            } else {
                digest::discussions::Source::Current
            };
            let context = digest::discussions::Context {
                store: &store,
                profile: &self.profile,
                options: &self.options,
                permission: &self.discussion_permission,
                source,
            };
            let request = self
                .request
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!(INVALID))?;
            if let Some(service) = &self.service {
                if let Some(approval) = &self.approval {
                    // Reject stale payload approval before any signing or service
                    // credential is read. Send replays again under its journal lock.
                    digest::service::propose(&context, request, capture, service, d)?
                        .check_approval(approval)?;
                    let host = yoke::github_app::Host::from_os(self.profile.clone())?;
                    let service_context = digest::service::Context {
                        store: &store,
                        host: &host,
                        profile: service,
                        options: &self.service_options,
                    };
                    let client = self.service_client(&service_context, d)?;
                    digest::service::send(
                        &service_context,
                        &context,
                        request,
                        capture,
                        approval,
                        &client,
                        d,
                    )?
                } else {
                    digest::service::propose(&context, request, capture, service, d)?.value(d)?
                }
            } else {
                digest::discussions::preview(&context, request, capture, d)?
            }
        } else if let Some((git, history_options, selection)) = &self.history {
            if let Some(selection) = selection {
                digest::preview_mapped_history_with_profile(
                    &store,
                    &self.profile,
                    &self.options,
                    self.request
                        .as_ref()
                        .ok_or_else(|| anyhow::anyhow!(INVALID))?,
                    git,
                    history_options,
                    selection,
                    d,
                )?
            } else {
                digest::preview_history_with_profile(
                    &store,
                    &self.profile,
                    &self.options,
                    self.request
                        .as_ref()
                        .ok_or_else(|| anyhow::anyhow!(INVALID))?,
                    git,
                    history_options,
                    d,
                )?
            }
        } else {
            digest::preview_with_profile(
                &store,
                &self.profile,
                &self.options,
                self.request
                    .as_ref()
                    .ok_or_else(|| anyhow::anyhow!(INVALID))?,
                d,
            )?
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
    fn service_client(
        &self,
        c: &digest::service::Context<'_>,
        d: &Deadline,
    ) -> Result<digest::service::Client> {
        if c.profile.endpoint.starts_with("http:") {
            let url =
                reqwest::Url::parse(&c.profile.endpoint).map_err(|_| anyhow::anyhow!(INVALID))?;
            let ip = url
                .host_str()
                .ok_or_else(|| anyhow::anyhow!(INVALID))?
                .trim_matches(['[', ']'])
                .parse::<std::net::IpAddr>()
                .map_err(|_| anyhow::anyhow!(INVALID))?;
            let port = url.port().ok_or_else(|| anyhow::anyhow!(INVALID))?;
            digest::service::Client::for_fixture(c, std::net::SocketAddr::new(ip, port), d)
        } else {
            if let Some(ca) = &self.service_ca {
                digest::service::Client::from_os_with_ca(c, ca, d)
            } else {
                digest::service::Client::from_os(c, d)
            }
        }
    }
}
