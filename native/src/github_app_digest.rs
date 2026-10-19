//! Closed native repository digest contracts; no incoming provider authority.
use super::Host;
pub use crate::git_workflows::HistoryOptions;
use crate::{capture::Deadline, store::Store};
use anyhow::{Context as _, Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[path = "github_app_digest_acquisition.rs"]
pub mod acquisition;
#[path = "github_app_digest_discussions.rs"]
pub mod discussions;
#[cfg(test)]
#[path = "github_app_digest_history_tests.rs"]
mod history_boundary_tests;
#[path = "github_app_digest_mapping.rs"]
mod mapping;
#[path = "github_app_digest_rust.rs"]
mod rust;
#[path = "github_app_digest_service.rs"]
pub mod service;
#[path = "github_app_digest_source.rs"]
mod source;
pub use mapping::{MappingOptions, MappingSelection};

#[derive(Clone, Copy, Debug)]
enum Failure {
    Capacity,
    Source,
}
impl std::fmt::Display for Failure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Capacity => "github_app_digest_capacity_exceeded",
            Self::Source => "github_app_digest_source_unavailable",
        })
    }
}
impl std::error::Error for Failure {}
const CAPACITY: Failure = Failure::Capacity;
const SOURCE: Failure = Failure::Source;

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PreviewRequest {
    pub schema_version: u32,
    pub snapshot_id: String,
    pub installation_id: u64,
    pub repository_id: u64,
    pub repository: String,
}
impl PreviewRequest {
    pub fn parse(raw: &[u8]) -> Result<Self> {
        let request: Self = crate::json::decode(crate::json::parse(raw, 4096)?)?;
        request.validate()?;
        Ok(request)
    }
    fn validate(&self) -> Result<()> {
        ensure!(
            self.schema_version == 1
                && crate::model::valid_id(&self.snapshot_id)
                && super::numeric(self.installation_id)
                && super::numeric(self.repository_id)
                && super::repository(&self.repository),
            "github_app_digest_input_invalid"
        );
        Ok(())
    }
}
#[derive(Clone)]
pub struct Options {
    pub max_symbols: usize,
    pub max_source_bytes: usize,
    pub max_output_bytes: usize,
    pub max_files: usize,
    pub max_entities: usize,
    pub max_metadata_bytes: usize,
    pub max_ast_nodes: usize,
    pub max_ast_depth: usize,
    pub max_replay_name_bytes: usize,
    pub max_lineage_metadata_bytes: usize,
}
impl Default for Options {
    fn default() -> Self {
        Self {
            max_symbols: 4000,
            max_source_bytes: 32_000_000,
            max_output_bytes: 4_000_000,
            max_files: 10_000,
            max_entities: 200_000,
            max_metadata_bytes: 64_000_000,
            max_ast_nodes: 1_000_000,
            max_ast_depth: 128,
            max_replay_name_bytes: 16_000_000,
            max_lineage_metadata_bytes: 16_000_000,
        }
    }
}
impl Options {
    pub fn validate(&self) -> Result<()> {
        let hard = Self::default();
        ensure!(
            self.max_symbols <= hard.max_symbols
                && self.max_source_bytes <= hard.max_source_bytes
                && self.max_output_bytes <= hard.max_output_bytes
                && self.max_files <= hard.max_files
                && self.max_entities <= hard.max_entities
                && self.max_metadata_bytes <= hard.max_metadata_bytes
                && self.max_ast_nodes <= hard.max_ast_nodes
                && self.max_ast_depth <= hard.max_ast_depth
                && self.max_replay_name_bytes <= hard.max_replay_name_bytes
                && self.max_lineage_metadata_bytes <= hard.max_lineage_metadata_bytes,
            CAPACITY
        );
        Ok(())
    }
}
pub struct Context<'a> {
    pub store: &'a Store,
    pub host: &'a Host,
    pub options: &'a Options,
}
pub fn preview(c: &Context<'_>, request: &PreviewRequest, d: &Deadline) -> Result<Value> {
    preview_with_profile(c.store, &c.host.profile, c.options, request, d)
}
/// Pure host-grant admission, shared by CLI preflight and the read boundary.
pub fn authorize_profile(profile: &super::HostProfile, request: &PreviewRequest) -> Result<()> {
    profile
        .validate()
        .map_err(|_| anyhow::anyhow!("github_app_digest_input_invalid"))?;
    request.validate()?;
    ensure!(
        profile
            .grants
            .iter()
            .any(|grant| grant.installation_id == request.installation_id
                && grant.repository_id == request.repository_id
                && grant.repository == request.repository),
        "github_app_digest_grant_denied"
    );
    Ok(())
}
/// Local captured-source reads require the current host profile, not an App key.
pub fn preview_with_profile(
    store: &Store,
    profile: &super::HostProfile,
    options: &Options,
    request: &PreviewRequest,
    d: &Deadline,
) -> Result<Value> {
    preview_with_history_reader(store, profile, options, request, d, None)
}
/// Associate explicitly selected, already captured Git snapshots with the
/// verified history. This never captures sources or grants provider authority.
#[allow(clippy::too_many_arguments)] // Independent host selectors and acquisition budgets.
pub fn preview_mapped_history_with_profile(
    store: &Store,
    profile: &super::HostProfile,
    options: &Options,
    request: &PreviewRequest,
    git: &crate::git_workflows::HostGit,
    history_options: &HistoryOptions,
    selection: &MappingSelection,
    d: &Deadline,
) -> Result<Value> {
    authorize_profile(profile, request)?;
    selection.validate(request)?;
    preview_history_mapped(
        store,
        profile,
        options,
        request,
        git,
        history_options,
        Some(selection),
        d,
    )
}
/// Explicit host-selected Git reads begin only after captured-source verification.
pub fn preview_history_with_profile(
    store: &Store,
    profile: &super::HostProfile,
    options: &Options,
    request: &PreviewRequest,
    git: &crate::git_workflows::HostGit,
    history_options: &HistoryOptions,
    d: &Deadline,
) -> Result<Value> {
    preview_history_mapped(
        store,
        profile,
        options,
        request,
        git,
        history_options,
        None,
        d,
    )
}
#[allow(clippy::too_many_arguments)] // Preserves the existing public history interface.
fn preview_history_mapped(
    store: &Store,
    profile: &super::HostProfile,
    options: &Options,
    request: &PreviewRequest,
    git: &crate::git_workflows::HostGit,
    history_options: &HistoryOptions,
    selection: Option<&MappingSelection>,
    d: &Deadline,
) -> Result<Value> {
    authorize_profile(profile, request)?;
    history_options.validate()?;
    d.check()?;
    let root = store
        .dir
        .parent()
        .context("github_app_digest_history_unavailable")?;
    let mut reader = |head: &str, d: &Deadline| {
        crate::git_workflows::history_read::read(git, root, head, history_options, d).map_err(
            |error| {
                if crate::process::cleanup_unproven(&error) {
                    return error.context("github_app_digest_history_cleanup_unproven");
                }
                if let Err(cause) = d.check() {
                    return cause;
                }
                if error.is::<crate::git_workflows::history_read::Capacity>() {
                    anyhow::anyhow!(CAPACITY)
                } else {
                    anyhow::anyhow!("github_app_digest_history_unavailable")
                }
            },
        )
    };
    preview_with_readers(
        store,
        profile,
        options,
        request,
        d,
        Some(&mut reader),
        selection,
    )
}
type HistoryReader<'a> =
    dyn FnMut(&str, &Deadline) -> Result<crate::git_workflows::history_read::History> + 'a;
fn preview_with_history_reader(
    store: &Store,
    profile: &super::HostProfile,
    options: &Options,
    request: &PreviewRequest,
    d: &Deadline,
    history_reader: Option<&mut HistoryReader<'_>>,
) -> Result<Value> {
    preview_with_readers(store, profile, options, request, d, history_reader, None)
}
fn preview_with_readers(
    store: &Store,
    profile: &super::HostProfile,
    options: &Options,
    request: &PreviewRequest,
    d: &Deadline,
    mut history_reader: Option<&mut HistoryReader<'_>>,
    selection: Option<&MappingSelection>,
) -> Result<Value> {
    authorize_profile(profile, request)?;
    d.check()?;
    let c = source::Context {
        store,
        options,
        proof_limit: selection.map(|selection| selection.limits.max_proof_bytes),
    };
    ensure!(
        c.store.db.is_autocommit(),
        "github_app_digest_caller_transaction_refused"
    );
    c.options.validate()?;
    // Each reader charges its own aggregate before decoding. The connection
    // ceiling must also admit lineage and separately bounded artifact rows.
    let row_ceiling = c
        .options
        .max_metadata_bytes
        .max(c.options.max_lineage_metadata_bytes)
        .max(64_000);
    let mut policy = source::ReadPolicy::enter(c.store, d, row_ceiling)?;
    let result = (|| {
        let read = c.store.db.unchecked_transaction().context(SOURCE)?;
        let projected = source::project(&c, request, d).map_err(|error| {
            if let Err(cause) = d.check() {
                return cause;
            }
            // Stored values and native decoder details never become public errors.
            // Typed capacity evidence cannot be forged by a string inside a row.
            match error.downcast_ref::<Failure>() {
                Some(Failure::Capacity) => anyhow::anyhow!(CAPACITY),
                _ => anyhow::anyhow!(SOURCE),
            }
        })?;
        if selection.is_some() {
            ensure!(projected.omissions == 0, CAPACITY);
        }
        let mut history = if let Some(reader) = history_reader.as_mut() {
            let head = projected
                .commit_id
                .as_deref()
                .context("github_app_digest_history_unavailable")?;
            Some(reader(head, d)?)
        } else {
            None
        };
        let mapped = if let Some(selection) = selection {
            Some(
                mapping::attach(
                    store,
                    options,
                    request,
                    &projected,
                    history.as_mut().context(SOURCE)?,
                    selection,
                    d,
                )
                .map_err(|error| {
                    if let Err(cause) = d.check() {
                        return cause;
                    }
                    if matches!(error.downcast_ref::<Failure>(), Some(Failure::Capacity))
                        || error.is::<crate::git_workflows::source_read::LineageCapacity>()
                    {
                        anyhow::anyhow!(CAPACITY)
                    } else {
                        anyhow::anyhow!(SOURCE)
                    }
                })?,
            )
        } else {
            None
        };
        let mut value = json!({
            "schemaVersion":1,"kind":"native_captured_repository_digest_preview",
            "snapshotId":request.snapshot_id,"installationId":request.installation_id,
            "repositoryId":request.repository_id,"repository":request.repository,
            "symbols":projected.symbols,"sources":projected.sources,
            "symbolCandidates":projected.candidates,"symbolOmissions":projected.omissions,
            "unsupportedFiles":projected.unsupported_files,
            "visibilityUnresolved":projected.visibility_unresolved,
            "visibilityScope":"file_syntax_only","externalVisibility":"unknown",
            "analyzer":projected.analyzer,"nativeEntitiesReplayed":projected.entities,
            "nativeEntityReplayScope":"captured_rust_declarations",
            "astNodesVisited":projected.ast_nodes,
            "providerVerified":false,"sourceCommitVerified":projected.commit_id.is_some(),"compilerVerified":false,
            "sourceCommitId":projected.commit_id,"sourceTreeId":projected.tree_id,
            "complete":false,"networkRequests":0,"modelCalls":0,"sourceWritten":false,
            "commits":[],"threads":[],
            "limitations":[
                "Native disk snapshots and locally proven immutable Git source snapshots are admitted. Local Git commit proofs do not verify a remote repository or current provider head.",
                "Rust declarations use explicit pub syntax and visible inline module ancestry within each captured file; crate roots, out-of-line modules, reexports, conditional compilation, macros, traits and implementations are not resolved.",
                "Other captured source bytes are verified, but their entities and export policies are not replayed. Commit history, provider identity and discussions are unavailable in this local projection.",
                "Native replay covers Rust declarations only. Call resolution, rationale, contracts, observations and graph completeness are not attested.",
                "Fingerprints bind complete captured declaration bytes; signatures exclude implementation bodies and const/static initializers."
            ]
        });
        if let Some(history) = history {
            value["kind"] = json!("native_captured_repository_digest_history_preview");
            value["commits"] = json!(history.commits);
            value["history"] = history.summary;
            let limitations = value["limitations"].as_array_mut().context(SOURCE)?;
            limitations[2] = json!(
                "Other captured source bytes are verified, but their entities and export policies are not replayed. Provider identity and discussions remain unavailable."
            );
            limitations.push(json!("History uses a bounded committer-date-priority traversal of hash-verified captured-head commit objects, preserves discovery order for equal dates and reverses selected rows. This is not a global timestamp sort; date lookahead shares all acquisition budgets. File differences and per-commit symbol associations are unavailable."));
            limitations.push(json!("Commit messages are untrusted repository metadata and may contain source code or secrets. Object identity does not verify authorship, signatures, provider ownership or a remote head."));
        }
        if let Some(mapped) = mapped {
            value["kind"] = json!("native_captured_repository_digest_mapped_history_preview");
            value["historyMapping"] = mapped;
            let limitations = value["limitations"].as_array_mut().context(SOURCE)?;
            limitations[5] = json!(
                "History uses the existing bounded verified commit traversal. File relations cover only the union of explicitly captured paths, not every Git tree change. Missing captures are unavailable, never an empty complete result."
            );
            limitations.push(json!("Symbol associations compare source-replayed file-local Rust export syntax. Matching qualified names or exact declaration fingerprints do not establish semantic identity, compiler accessibility, a rename or runtime dispatch."));
        }
        crate::json::encode_bounded(&value, c.options.max_output_bytes, d).context(CAPACITY)?;
        d.check()?;
        // A read transaction pins all selected metadata; this commits no SQL writes.
        read.commit().context(SOURCE)?;
        Ok(value)
    })();
    policy.restore()?;
    result
}
