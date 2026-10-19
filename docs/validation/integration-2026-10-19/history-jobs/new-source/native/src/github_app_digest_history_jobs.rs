//! Durable local history acquisition. Each step owns one publication transaction;
//! raw Merkle proofs reconstruct the frontier and exact revisions fence retries.
use super::*;
pub use crate::git_workflows::history_read::cursor::Limits as TraversalLimits;
use crate::{
    Engine,
    analysis::AnalyzerEpoch,
    git_workflows::{
        self, CaptureBudget, HostGit,
        history_read::{
            History,
            cursor::{Cursor, Proof},
        },
    },
    model::{Snapshot, identity},
};
use rusqlite::{OptionalExtension, Transaction, TransactionBehavior};
use std::collections::{BTreeMap, BTreeSet};
pub const RECORD_KIND: &str = "native_digest_history_job_v1";
const ROW_BYTES: usize = 32_000_000;
const STORE_BYTES: usize = 64_000_000;
#[derive(Debug)]
enum JobFailure {
    Invalid,
    Stale,
    State,
    Capacity,
}
impl std::fmt::Display for JobFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Invalid => "github_app_digest_history_job_invalid",
            Self::Stale => "github_app_digest_history_job_stale_revision",
            Self::State => "github_app_digest_history_job_state_refused",
            Self::Capacity => "github_app_digest_capacity_exceeded",
        })
    }
}
impl std::error::Error for JobFailure {}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Plan {
    pub traversal: TraversalLimits,
    pub max_captures: usize,
    pub max_candidate_files: usize,
    pub max_source_bytes: usize,
    pub max_commit_bytes: usize,
    pub max_tree_bytes: usize,
    pub max_tree_entries: usize,
}
impl Default for Plan {
    fn default() -> Self {
        Self {
            traversal: TraversalLimits::default(),
            max_captures: 16_000,
            max_candidate_files: 1_000_000,
            max_source_bytes: 1_000_000_000,
            max_commit_bytes: 256_000_000,
            max_tree_bytes: 256_000_000,
            max_tree_entries: 1_000_000,
        }
    }
}
impl Plan {
    pub fn validate(&self) -> Result<()> {
        self.traversal.validate()?;
        let hard = Self::default();
        ensure!(
            self.max_captures <= hard.max_captures
                && self.max_candidate_files <= hard.max_candidate_files
                && self.max_source_bytes <= hard.max_source_bytes
                && self.max_commit_bytes <= hard.max_commit_bytes
                && self.max_tree_bytes <= hard.max_tree_bytes
                && self.max_tree_entries <= hard.max_tree_entries,
            JobFailure::Capacity
        );
        Ok(())
    }
}
/// Current host authority only. These flags are never read from a saved plan.
pub struct Options {
    pub allow_jobs: bool,
    pub acquisition: acquisition::Options,
    pub max_batch_visits: usize,
    pub max_batch_objects: usize,
    pub max_batch_proof_bytes: usize,
}
impl Default for Options {
    fn default() -> Self {
        Self {
            allow_jobs: false,
            acquisition: acquisition::Options::default(),
            max_batch_visits: 8,
            max_batch_objects: 32,
            max_batch_proof_bytes: 2_000_000,
        }
    }
}
pub struct Context<'a> {
    pub profile: &'a super::super::HostProfile,
    pub digest: &'a super::Options,
    pub options: &'a Options,
}
#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Totals {
    files: usize,
    source_bytes: usize,
    commit_bytes: usize,
    tree_bytes: usize,
    tree_entries: usize,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Job {
    schema_version: u32,
    workspace_id: String,
    app_id: u64,
    job_id: String,
    request: PreviewRequest,
    head_commit_id: String,
    origin_snapshot_id: String,
    analyzer: String,
    plan: Plan,
    revision: u64,
    state: State,
    proof: Proof,
    captures: BTreeMap<String, String>,
    totals: Totals,
}
#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum State {
    Active,
    Paused,
    Cancelled,
    Complete,
}
fn key(workspace: &str, app: u64, id: &str) -> Result<String> {
    ensure!(crate::model::valid_id(id), JobFailure::Invalid);
    Ok(identity(&(RECORD_KIND, workspace, app, id)))
}
fn read(store: &Store, lookup_key: &str, d: &Deadline) -> Result<Option<Job>> {
    d.check()?;
    let raw: Option<Vec<u8>> = store
        .db
        .query_row(
            "SELECT substr(CAST(value AS BLOB),1,?2) FROM records WHERE kind=?1 AND id=?3",
            rusqlite::params![RECORD_KIND, ROW_BYTES + 1, lookup_key],
            |r| r.get(0),
        )
        .optional()?;
    raw.map(|raw| {
        let job: Job = crate::json::decode(crate::json::parse(&raw, ROW_BYTES)?)?;
        ensure!(
            key(&job.workspace_id, job.app_id, &job.job_id)? == lookup_key,
            JobFailure::Invalid
        );
        Ok(job)
    })
    .transpose()
}
fn replay(store: &Store, job: &Job, d: &Deadline) -> Result<Cursor> {
    d.check()?;
    job.plan.validate()?;
    job.request.validate()?;
    ensure!(
        job.schema_version == 1
            && job.workspace_id == store.workspace_id
            && super::super::numeric(job.app_id)
            && crate::model::valid_id(&job.job_id)
            && crate::model::valid_id(&job.origin_snapshot_id)
            && matches!(
                AnalyzerEpoch::parse(&job.analyzer)?,
                AnalyzerEpoch::V8 | AnalyzerEpoch::V9
            )
            && job.revision <= 100_000
            && job.captures.len() <= job.plan.max_captures
            && job.totals.files <= job.plan.max_candidate_files
            && job.totals.source_bytes <= job.plan.max_source_bytes
            && job.totals.commit_bytes <= job.plan.max_commit_bytes
            && job.totals.tree_bytes <= job.plan.max_tree_bytes
            && job.totals.tree_entries <= job.plan.max_tree_entries,
        JobFailure::Invalid
    );
    let cursor = Cursor::replay(&job.head_commit_id, &job.proof, &job.plan.traversal, d)?;
    let expected = Cursor::required_captures(&job.head_commit_id, &job.proof, d)?;
    ensure!(
        job.captures.keys().cloned().collect::<BTreeSet<_>>() == expected
            && job
                .captures
                .values()
                .all(|id| crate::model::valid_id(id) && id != &job.request.snapshot_id)
            && job.captures.values().collect::<BTreeSet<_>>().len() == job.captures.len()
            && (job.state != State::Complete || cursor.exhausted())
            && (!cursor.exhausted() || matches!(job.state, State::Complete | State::Cancelled)),
        JobFailure::Invalid
    );
    Ok(cursor)
}
fn current(c: &Context<'_>, store: &Store, job: &Job, d: &Deadline) -> Result<Cursor> {
    ensure!(job.app_id == c.profile.app_id, JobFailure::Invalid);
    authorize_profile(c.profile, &job.request)?;
    replay(store, job, d)
}
fn summary(job: &Job, cursor: &Cursor, written: bool) -> Value {
    json!({"schemaVersion":1,"kind":"native_digest_history_job_status","jobId":job.job_id,
        "workspaceId":job.workspace_id,"appId":job.app_id,"request":job.request,
        "headCommitId":job.head_commit_id,"originSnapshotId":job.origin_snapshot_id,"analyzer":job.analyzer,
        "revision":job.revision,"state":job.state,"visitedCommits":job.proof.visited.len(),
        "objectsRead":job.proof.objects.len(),"rawCommitBytes":cursor.raw_bytes,"parentIdsObserved":cursor.parent_ids,
        "frontierCommits":cursor.frontier_len(),"unreadFrontierObjects":cursor.missing_len(),
        "recordedCaptureReferences":job.captures.len(),"acquisition":job.totals,"plan":job.plan,
        "traversalExhausted":cursor.exhausted(),"sourceBodiesReplayed":false,
        "stateWritten":written,"sourceWritten":false,"latestChanged":false,"networkRequests":0,"modelCalls":0,
        "complete":false,"limitations":["Exhaustion is reconstructed from verified raw commit objects and their ordered parent closure. Git authorship and remote repository identity are not attested.",
        "Capture references report recorded progress. Status does not reread all historical source bodies or attest semantic/graph completeness. Each selected batch replays its mapped sources before atomic publication.",
        "A bounded local job never starts itself, a model, source code, a network request or a credential lookup. Host permissions and App grants must be supplied again on every operation."]})
}
fn persist(store: &Store, id: &str, old: Option<&Job>, job: &Job, d: &Deadline) -> Result<()> {
    crate::json::encode_bounded(job, ROW_BYTES, d)?;
    let (count,bytes):(usize,usize) = store.db.query_row(
        "SELECT count(*),COALESCE(sum(length(CAST(value AS BLOB))),0) FROM records WHERE kind=?1 AND id<>?2",
        rusqlite::params![RECORD_KIND,id], |r|Ok((r.get(0)?,r.get(1)?)))?;
    let next = crate::json::encode_bounded(job, STORE_BYTES.saturating_sub(bytes), d)?;
    ensure!(
        count < 8 && bytes + next.len() <= STORE_BYTES,
        JobFailure::Capacity
    );
    if let Some(old) = old {
        ensure!(
            store.replace_record(RECORD_KIND, id, old, job)?,
            JobFailure::Stale
        );
    } else {
        store.db.execute(
            "INSERT INTO records(kind,id,value) VALUES(?1,?2,?3)",
            rusqlite::params![RECORD_KIND, id, std::str::from_utf8(&next)?],
        )?;
    }
    Ok(())
}
fn sql(
    c: &Context<'_>,
    store: &Store,
    writes: bool,
    d: &Deadline,
    f: impl FnOnce() -> Result<Value>,
) -> Result<Value> {
    let result = (|| {
        ensure!(c.options.allow_jobs, JobFailure::State);
        ensure!(
            (1024..=4_000_000).contains(&c.options.acquisition.max_output_bytes),
            JobFailure::Capacity
        );
        c.digest.validate()?;
        c.profile.validate()?;
        ensure!(
            (1..=16).contains(&c.options.max_batch_visits)
                && (1..=64).contains(&c.options.max_batch_objects)
                && (1..=2_000_000).contains(&c.options.max_batch_proof_bytes),
            JobFailure::Capacity
        );
        if writes {
            c.options
                .acquisition
                .admit(c.digest, &HistoryOptions::default(), d)?;
        }
        ensure!(store.db.is_autocommit(), JobFailure::State);
        crate::github_app::worker::publication_policy(store, d, || {
            use rusqlite::limits::Limit;
            let prior = store.db.limit(Limit::SQLITE_LIMIT_LENGTH)?;
            store
                .db
                .set_limit(Limit::SQLITE_LIMIT_LENGTH, prior.min(64_004_096))?;
            if writes {
                let mode: String = store
                    .db
                    .query_row("PRAGMA journal_mode", [], |r| r.get(0))?;
                let sync: u32 = store.db.query_row("PRAGMA synchronous", [], |r| r.get(0))?;
                ensure!(mode == "wal" && (2..=3).contains(&sync), JobFailure::State);
            }
            let tx = Transaction::new_unchecked(
                &store.db,
                if writes {
                    TransactionBehavior::Immediate
                } else {
                    TransactionBehavior::Deferred
                },
            )?;
            let value = f()?;
            crate::json::encode_bounded(
                &value,
                c.options
                    .acquisition
                    .max_output_bytes
                    .checked_sub(1)
                    .context(CAPACITY)?,
                d,
            )
            .context(CAPACITY)?;
            d.check()?;
            tx.commit()?;
            Ok(value)
        })
    })();
    result.map_err(|e| {
        if crate::process::cleanup_unproven(&e) {
            return e.context("github_app_digest_history_job_cleanup_unproven");
        }
        if let Err(e) = d.check() {
            return e;
        }
        if e.is::<JobFailure>()
            || e.is::<Failure>()
            || e.is::<git_workflows::history_read::Capacity>()
        {
            e
        } else {
            anyhow::anyhow!("github_app_digest_history_job_unavailable")
        }
    })
}
fn head(
    engine: &Engine,
    c: &Context<'_>,
    request: &PreviewRequest,
    d: &Deadline,
) -> Result<(source::Projection, Snapshot)> {
    let head = source::project(
        &source::Context {
            store: &engine.store,
            options: c.digest,
            proof_limit: Some(MappingOptions::default().max_proof_bytes),
        },
        request,
        d,
    )?;
    ensure!(head.omissions == 0 && head.commit_id.is_some(), SOURCE);
    let receipt: Value = engine
        .store
        .get_record(git_workflows::SOURCE_KIND, &request.snapshot_id)?;
    let origin = engine
        .store
        .snapshot(receipt["originSnapshotId"].as_str().context(SOURCE)?)?;
    ensure!(
        origin.kind == "disk" && origin.workspace_id == engine.store.workspace_id,
        SOURCE
    );
    Ok((head, origin))
}
pub fn create(
    engine: &Engine,
    c: &Context<'_>,
    id: &str,
    request: &PreviewRequest,
    plan: &Plan,
    d: &Deadline,
) -> Result<Value> {
    sql(c, &engine.store, true, d, || {
        plan.validate()?;
        authorize_profile(c.profile, request)?;
        let key = key(&engine.store.workspace_id, c.profile.app_id, id)?;
        if let Some(job) = read(&engine.store, &key, d)? {
            ensure!(
                identity(&job.request) == identity(request)
                    && identity(&job.plan) == identity(plan),
                JobFailure::Invalid
            );
            let cursor = current(c, &engine.store, &job, d)?;
            return Ok(summary(&job, &cursor, false));
        }
        let (head, origin) = head(engine, c, request, d)?;
        let job = Job {
            schema_version: 1,
            workspace_id: engine.store.workspace_id.clone(),
            app_id: c.profile.app_id,
            job_id: id.into(),
            request: request.clone(),
            head_commit_id: head.commit_id.context(SOURCE)?,
            origin_snapshot_id: origin.id,
            analyzer: head.analyzer,
            plan: plan.clone(),
            revision: 0,
            state: State::Active,
            proof: Proof::default(),
            captures: BTreeMap::new(),
            totals: Totals::default(),
        };
        let cursor = replay(&engine.store, &job, d)?;
        persist(&engine.store, &key, None, &job, d)?;
        Ok(summary(&job, &cursor, true))
    })
}
pub fn status(store: &Store, c: &Context<'_>, id: &str, d: &Deadline) -> Result<Value> {
    sql(c, store, false, d, || {
        let key = key(&store.workspace_id, c.profile.app_id, id)?;
        let job = read(store, &key, d)?.context(JobFailure::Invalid)?;
        let cursor = current(c, store, &job, d)?;
        Ok(summary(&job, &cursor, false))
    })
}
pub fn transition(
    store: &Store,
    c: &Context<'_>,
    id: &str,
    revision: u64,
    state: State,
    d: &Deadline,
) -> Result<Value> {
    sql(c, store, true, d, || {
        let key = key(&store.workspace_id, c.profile.app_id, id)?;
        let old = read(store, &key, d)?.context(JobFailure::Invalid)?;
        let cursor = current(c, store, &old, d)?;
        ensure!(old.revision == revision, JobFailure::Stale);
        ensure!(
            matches!(
                (old.state, state),
                (State::Active, State::Paused | State::Cancelled)
                    | (State::Paused, State::Active | State::Cancelled)
            ),
            JobFailure::State
        );
        let mut job = old.clone();
        job.state = state;
        job.revision += 1;
        replay(store, &job, d)?;
        persist(store, &key, Some(&old), &job, d)?;
        Ok(summary(&job, &cursor, true))
    })
}
pub fn step(
    engine: &Engine,
    c: &Context<'_>,
    id: &str,
    revision: u64,
    git: &HostGit,
    d: &Deadline,
) -> Result<Value> {
    sql(c, &engine.store, true, d, || {
        let key = key(&engine.store.workspace_id, c.profile.app_id, id)?;
        let old = read(&engine.store, &key, d)?.context(JobFailure::Invalid)?;
        let mut cursor = current(c, &engine.store, &old, d)?;
        ensure!(old.revision == revision, JobFailure::Stale);
        ensure!(old.state == State::Active, JobFailure::State);
        let (head, origin) = head(engine, c, &old.request, d)?;
        ensure!(
            head.commit_id.as_deref() == Some(old.head_commit_id.as_str())
                && head.analyzer == old.analyzer
                && origin.id == old.origin_snapshot_id,
            SOURCE
        );
        let mut job = old.clone();
        let (mut objects, mut proof_bytes, mut visits) = (0usize, 0usize, 0usize);
        let mut rows = vec![];
        while visits < c.options.max_batch_visits && !cursor.exhausted() {
            while cursor.next_pending().is_some() && objects < c.options.max_batch_objects {
                let Some(bytes) = cursor.read_pending(
                    git,
                    &engine.root,
                    &mut job.proof,
                    c.options.max_batch_proof_bytes.saturating_sub(proof_bytes),
                    d,
                )?
                else {
                    // Preserve admitted lookahead when only this batch's byte
                    // remainder is too small. A fresh batch must fit an object.
                    ensure!(objects > 0, JobFailure::Capacity);
                    break;
                };
                proof_bytes += bytes;
                objects += 1;
            }
            if cursor.next_pending().is_some() {
                break;
            }
            let Some((oid, row)) = cursor.visit(d)? else {
                break;
            };
            job.proof.visited.push(oid);
            visits += 1;
            if let Some(row) = row {
                rows.push(row);
            }
        }
        let targets = Cursor::required_captures(&job.head_commit_id, &job.proof, d)?;
        ensure!(targets.len() <= job.plan.max_captures, JobFailure::Capacity);
        let a = &c.options.acquisition;
        let mut remaining = CaptureBudget {
            files: a
                .max_files
                .min(job.plan.max_candidate_files - job.totals.files),
            source_bytes: a
                .max_source_bytes
                .min(job.plan.max_source_bytes - job.totals.source_bytes),
            commit_bytes: a
                .max_commit_bytes
                .min(job.plan.max_commit_bytes - job.totals.commit_bytes),
            tree_bytes: a
                .max_tree_bytes
                .min(job.plan.max_tree_bytes - job.totals.tree_bytes),
            tree_entries: a
                .max_tree_entries
                .min(job.plan.max_tree_entries - job.totals.tree_entries),
        };
        let initial = Totals {
            files: remaining.files,
            source_bytes: remaining.source_bytes,
            commit_bytes: remaining.commit_bytes,
            tree_bytes: remaining.tree_bytes,
            tree_entries: remaining.tree_entries,
        };
        let mut added = vec![];
        for oid in targets.difference(&job.captures.keys().cloned().collect()) {
            let (snapshot, _) = git_workflows::capture_commit_pending(
                AnalyzerEpoch::parse(&job.analyzer)?,
                engine,
                git,
                &origin,
                oid,
                &mut remaining,
                d,
            )?;
            job.captures.insert(oid.clone(), snapshot.id.clone());
            added.push(snapshot.id);
        }
        job.totals.files += initial.files - remaining.files;
        job.totals.source_bytes += initial.source_bytes - remaining.source_bytes;
        job.totals.commit_bytes += initial.commit_bytes - remaining.commit_bytes;
        job.totals.tree_bytes += initial.tree_bytes - remaining.tree_bytes;
        job.totals.tree_entries += initial.tree_entries - remaining.tree_entries;
        let mut selected = BTreeSet::new();
        for row in &rows {
            for oid in
                std::iter::once(&row["sha"]).chain(row["parentIds"].as_array().context(SOURCE)?)
            {
                let oid = oid.as_str().context(SOURCE)?;
                if oid != job.head_commit_id {
                    selected.insert(job.captures.get(oid).context(SOURCE)?.clone());
                }
            }
        }
        ensure!(selected.len() < a.max_snapshots, JobFailure::Capacity);
        for (oid, id) in job.captures.iter().filter(|(_, id)| selected.contains(*id)) {
            let receipt: Value = engine.store.get_record(git_workflows::SOURCE_KIND, id)?;
            ensure!(
                receipt["originSnapshotId"] == origin.id
                    && receipt["commitId"] == *oid
                    && receipt["analyzer"] == job.analyzer,
                SOURCE
            );
        }
        rows.reverse();
        let mut history = History {
            commits: rows,
            summary: json!({"selectionOrder":"commit_date_priority_parent_insertion_order",
            "outputOrder":"reverse_selection_order_within_batch","visitedCommits":visits,"objectsRead":objects,
            "rawCommitBytes":proof_bytes,"frontierCommits":cursor.frontier_len(),"exhausted":cursor.exhausted(),"complete":false}),
        };
        let mapping = mapping::attach(
            &engine.store,
            c.digest,
            &job.request,
            &head,
            &mut history,
            &MappingSelection {
                snapshot_ids: selected.into_iter().collect(),
                limits: MappingOptions {
                    max_snapshots: a.max_snapshots,
                    ..Default::default()
                },
            },
            d,
        )?;
        ensure!(mapping["unavailableCommits"] == 0, SOURCE);
        job.revision += 1;
        if cursor.exhausted() {
            job.state = State::Complete;
        }
        let checked = replay(&engine.store, &job, d)?;
        persist(&engine.store, &key, Some(&old), &job, d)?;
        let mut value = summary(&job, &checked, true);
        value["kind"] = json!("native_digest_history_job_batch");
        value["batch"] = json!({"history":history.summary,"commits":history.commits,"historyMapping":mapping,
            "newSnapshotIds":added,"mappedSourceBodiesReplayed":true});
        Ok(value)
    })
}
pub(crate) fn retained_roots(
    store: &Store,
    d: &Deadline,
) -> Result<(BTreeSet<String>, BTreeSet<String>)> {
    let mut statement = store
        .db
        .prepare("SELECT id FROM records WHERE kind=?1 ORDER BY id")?;
    let mut records = BTreeSet::new();
    let mut roots = BTreeSet::new();
    let mut bytes = 0usize;
    for id in statement.query_map([RECORD_KIND], |r| r.get::<_, String>(0))? {
        d.check()?;
        ensure!(records.len() < 8, JobFailure::Capacity);
        let id = id?;
        let job = read(store, &id, d)?.context(JobFailure::Invalid)?;
        bytes += crate::json::encode_bounded(&job, STORE_BYTES.saturating_sub(bytes), d)?.len();
        replay(store, &job, d)?;
        ensure!(
            id == key(&store.workspace_id, job.app_id, &job.job_id)?,
            JobFailure::Invalid
        );
        // Closed local source selectors alone carry retention authority. Raw
        // commit messages, Git IDs and arbitrary JSON strings are never roots.
        for (oid, snapshot) in std::iter::once((&job.head_commit_id, &job.request.snapshot_id))
            .chain(job.captures.iter())
        {
            let receipt = git_workflows::validate_source(store, snapshot, d)?;
            ensure!(
                receipt["commitId"] == *oid
                    && receipt["originSnapshotId"] == job.origin_snapshot_id
                    && receipt["analyzer"] == job.analyzer,
                SOURCE
            );
            roots.insert(snapshot.clone());
        }
        roots.insert(job.origin_snapshot_id);
        records.insert(id);
    }
    Ok((records, roots))
}
