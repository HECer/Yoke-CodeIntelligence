//! Explicit automatic capture of selected captured-head history. All SQL
//! publication, scope proofs, receipts, mappings and output admission are atomic.
use super::*;
use crate::{
    Engine,
    analysis::AnalyzerEpoch,
    git_workflows::{self, CaptureBudget, HostGit},
    model::{Snapshot, identity},
};
use rusqlite::{Transaction, TransactionBehavior, limits::Limit};
use std::collections::BTreeSet;
pub struct Options {
    pub allow_capture: bool,
    pub allow_writes: bool,
    pub max_snapshots: usize,
    pub max_files: usize,
    pub max_source_bytes: usize,
    pub max_commit_bytes: usize,
    pub max_tree_bytes: usize,
    pub max_tree_entries: usize,
    /// Includes the CLI LF; no snapshots commit if the result cannot fit.
    pub max_output_bytes: usize,
}
impl Default for Options {
    fn default() -> Self {
        Self {
            allow_capture: false,
            allow_writes: false,
            max_snapshots: 32,
            max_files: 10_000,
            max_source_bytes: 32_000_000,
            max_commit_bytes: 16_000_000,
            max_tree_bytes: 16_000_000,
            max_tree_entries: 100_000,
            max_output_bytes: 4_000_000,
        }
    }
}
impl Options {
    pub fn admit(
        &self,
        digest: &super::Options,
        history: &HistoryOptions,
        d: &Deadline,
    ) -> Result<()> {
        ensure!(
            self.allow_capture && self.allow_writes,
            "github_app_digest_acquisition_permission_required"
        );
        d.check()?;
        let hard = Self::default();
        ensure!(
            (1..=hard.max_snapshots).contains(&self.max_snapshots)
                && self.max_files <= hard.max_files
                && self.max_source_bytes <= hard.max_source_bytes
                && self.max_commit_bytes <= hard.max_commit_bytes
                && self.max_tree_bytes <= hard.max_tree_bytes
                && self.max_tree_entries <= hard.max_tree_entries
                && (1024..=hard.max_output_bytes).contains(&self.max_output_bytes),
            CAPACITY
        );
        digest.validate()?;
        history.validate()?;
        Ok(())
    }
}
#[allow(clippy::too_many_arguments)] // Independent current host grants and acquisition/replay limits.
pub fn acquire(
    engine: &mut Engine,
    profile: &super::super::HostProfile,
    digest: &super::Options,
    request: &PreviewRequest,
    git: &HostGit,
    history_options: &HistoryOptions,
    options: &Options,
    d: &Deadline,
) -> Result<Value> {
    options.admit(digest, history_options, d)?;
    authorize_profile(profile, request)?;
    ensure!(
        engine.store.db.is_autocommit(),
        "github_app_digest_acquisition_caller_transaction_refused"
    );
    let result = crate::github_app::worker::publication_policy(&engine.store, d, || {
        // The source proof/capture metadata and generated graphs share the
        // established row ceiling; the scoped policy restores the caller limit.
        let prior = engine.store.db.limit(Limit::SQLITE_LIMIT_LENGTH)?;
        engine
            .store
            .db
            .set_limit(Limit::SQLITE_LIMIT_LENGTH, prior.min(64_004_096))?;
        let mode: String = engine
            .store
            .db
            .query_row("PRAGMA journal_mode", [], |r| r.get(0))?;
        let sync: u32 = engine
            .store
            .db
            .query_row("PRAGMA synchronous", [], |r| r.get(0))?;
        ensure!(
            mode == "wal" && (2..=3).contains(&sync),
            "github_app_digest_acquisition_durability_required"
        );
        let tx = Transaction::new_unchecked(&engine.store.db, TransactionBehavior::Immediate)?;
        let head = source::project(
            &source::Context {
                store: &engine.store,
                options: digest,
                proof_limit: Some(MappingOptions::default().max_proof_bytes),
            },
            request,
            d,
        )?;
        ensure!(head.omissions == 0, CAPACITY);
        let commit = head
            .commit_id
            .as_deref()
            .ok_or_else(|| anyhow::anyhow!("github_app_digest_acquisition_git_source_required"))?;
        let epoch = AnalyzerEpoch::parse(&head.analyzer)?;
        let receipt: Value = engine
            .store
            .get_record(git_workflows::SOURCE_KIND, &request.snapshot_id)?;
        let origin: Snapshot = engine.store.snapshot(
            receipt["originSnapshotId"]
                .as_str()
                .ok_or_else(|| anyhow::anyhow!(SOURCE))?,
        )?;
        ensure!(
            origin.kind == "disk" && origin.workspace_id == engine.store.workspace_id,
            SOURCE
        );
        let mut history =
            git_workflows::history_read::read(git, &engine.root, commit, history_options, d)?;
        let mut targets = vec![];
        let mut seen = BTreeSet::from([commit.to_owned()]);
        for row in &history.commits {
            d.check()?;
            for sha in std::iter::once(&row["sha"]).chain(
                row["parentIds"]
                    .as_array()
                    .ok_or_else(|| anyhow::anyhow!(SOURCE))?,
            ) {
                let sha = sha.as_str().ok_or_else(|| anyhow::anyhow!(SOURCE))?;
                if seen.insert(sha.to_owned()) {
                    ensure!(seen.len() <= options.max_snapshots, CAPACITY);
                    targets.push(sha.to_owned());
                }
            }
        }
        let mut remaining = CaptureBudget {
            files: options.max_files,
            source_bytes: options.max_source_bytes,
            commit_bytes: options.max_commit_bytes,
            tree_bytes: options.max_tree_bytes,
            tree_entries: options.max_tree_entries,
        };
        let mut captures = vec![];
        let mut snapshot_ids = vec![];
        for sha in targets {
            d.check()?;
            let (snapshot, _) = git_workflows::capture_commit_pending(
                epoch,
                engine,
                git,
                &origin,
                &sha,
                &mut remaining,
                d,
            )?;
            let source: Value = engine
                .store
                .get_record(git_workflows::SOURCE_KIND, &snapshot.id)?;
            ensure!(
                source["commitId"] == sha && source["originSnapshotId"] == origin.id,
                SOURCE
            );
            captures.push(json!({"commitId":sha,"snapshotId":snapshot.id,"files":snapshot.files.len(),"sourceBytes":snapshot.files.iter().map(|f|f.size).sum::<usize>(),"snapshotHash":identity(&snapshot)}));
            snapshot_ids.push(snapshot.id);
        }
        let selection = MappingSelection {
            snapshot_ids: snapshot_ids.clone(),
            limits: MappingOptions {
                max_snapshots: options.max_snapshots,
                ..Default::default()
            },
        };
        selection.validate(request)?;
        let mapping = mapping::attach(
            &engine.store,
            digest,
            request,
            &head,
            &mut history,
            &selection,
            d,
        )?;
        ensure!(
            mapping["unavailableCommits"] == 0,
            "github_app_digest_acquisition_mapping_unavailable"
        );
        let state_written = !captures.is_empty();
        let value = json!({"schemaVersion":1,"kind":"native_captured_repository_history_acquisition","workspaceId":engine.store.workspace_id,"appId":profile.app_id,"request":request,
            "headCommitId":commit,"originSnapshotId":origin.id,"analyzer":epoch.producer(),"acquiredCaptures":captures,"capturedSnapshotIds":snapshot_ids,
            "history":history.summary,"commits":history.commits,"historyMapping":mapping,
            "acquisition":{"candidateFilesAdmitted":options.max_files-remaining.files,"sourceBytesAdmitted":options.max_source_bytes-remaining.source_bytes,"commitBytes":options.max_commit_bytes-remaining.commit_bytes,"treeListingBytes":options.max_tree_bytes-remaining.tree_bytes,"treeEntries":options.max_tree_entries-remaining.tree_entries},
            "sqlPublication":"atomic_after_mapping_and_output_admission","sourceWritten":false,"stateWritten":state_written,"latestChanged":false,"networkRequests":0,"modelCalls":0,"complete":false,
            "limitations":["Only bounded non-merge/non-empty-message history rows and their direct parents are selected by the existing captured-head reader. Traversal frontier/coverage remains explicit.","Capture inherits the exact verified disk-origin scope and uses immutable Git objects, not the current checkout or remote provider claims. Captured subsets and Rust export syntax do not establish semantic identity or whole-repository completeness.","Only application SQL publication is atomic. Durable CAS bytes written before failure can remain unreferenced; they grant no snapshot or source authority. No source files, Git refs/index/config, provider state, credentials or latest pointer are changed."]});
        crate::json::encode_bounded(&value, options.max_output_bytes - 1, d).context(CAPACITY)?;
        d.check()?;
        tx.commit()?;
        Ok(value)
    });
    result.map_err(|error| {
        if crate::process::cleanup_unproven(&error) {
            return error.context("github_app_digest_acquisition_cleanup_unproven");
        }
        if let Err(cause) = d.check() {
            return cause;
        }
        if error.is::<Failure>() {
            error
        } else {
            anyhow::anyhow!("github_app_digest_acquisition_unavailable")
        }
    })
}
