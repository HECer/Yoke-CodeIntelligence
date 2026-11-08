//! Explicit App-completion binding for bounded persistent local history jobs.
use super::*;
use crate::{
    Engine,
    git_workflows::HostGit,
    github_app::digest::{self, PreviewRequest, history_jobs as jobs},
    model::{identity, valid_id},
};
use anyhow::Context as _;
use rusqlite::{OptionalExtension, params};

pub const RECORD_KIND: &str = "github_app_history_binding_v1";
const ROW_BYTES: usize = 16_000;
const DOMAIN: &[u8] = b"yoke-code-intelligence/github-app-history/v1\n";
#[derive(Debug)]
pub(crate) struct Refusal(&'static str);
impl std::fmt::Display for Refusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.0)
    }
}
impl std::error::Error for Refusal {}
fn failure(code: &'static str) -> anyhow::Error {
    Refusal(code).into()
}
const INVALID: &str = "github_app_history_input_invalid";
const BINDING: &str = "github_app_history_binding_invalid";
const CAPACITY: &str = "github_app_history_capacity_exceeded";

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Selection {
    pub schema_version: u8,
    pub completion_id: String,
    #[serde(default)]
    pub expected_queue_revision: Option<usize>,
}
impl Selection {
    pub fn parse(raw: &[u8]) -> Result<Self> {
        let selection: Self = crate::json::parse(raw, 4096)
            .and_then(crate::json::decode)
            .map_err(|_| failure(INVALID))?;
        selection.validate()?;
        Ok(selection)
    }
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.schema_version == 1 && valid_id(&self.completion_id),
            failure(INVALID)
        );
        Ok(())
    }
}
/// Current host grants; none are deserialized from a job/binding or request.
#[derive(Default)]
pub struct Options {
    pub allow_app_history: bool,
    pub worker: super::Options,
    pub digest: digest::Options,
    pub jobs: jobs::Options,
}
pub struct Context<'a> {
    pub store: &'a Store,
    pub host: &'a Host,
    pub options: &'a Options,
}
pub struct WriteContext<'a> {
    pub engine: &'a Engine,
    pub host: &'a Host,
    pub options: &'a Options,
}
impl WriteContext<'_> {
    fn read(&self) -> Context<'_> {
        Context {
            store: &self.engine.store,
            host: self.host,
            options: self.options,
        }
    }
}
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Binding {
    schema_version: u8,
    id: String,
    workspace_id: String,
    app_id: u64,
    completion_id: String,
    worker_job_id: String,
    worker_review_hash: String,
    head_commit_id: String,
    request: PreviewRequest,
    plan: jobs::Plan,
    signature256: String,
}
fn binding_id(workspace: &str, app: u64, completion: &str) -> String {
    identity(&(RECORD_KIND, workspace, app, completion))
}
fn unsigned(binding: &Binding, d: &Deadline) -> Result<Vec<u8>> {
    let mut value = serde_json::to_value(binding)?;
    value
        .as_object_mut()
        .context(failure(BINDING))?
        .remove("signature256");
    let bytes = crate::json::encode_bounded(&value, ROW_BYTES, d)?;
    let mut message = DOMAIN.to_vec();
    message.extend(bytes);
    d.check()?;
    Ok(message)
}
fn sign(binding: &Binding, host: &Host, d: &Deadline) -> Result<String> {
    let message = unsigned(binding, d)?;
    let key = host.keys.first().context(failure(BINDING))?;
    let mut mac = Hmac::<sha2::Sha256>::new_from_slice(&key.0).map_err(|_| failure(BINDING))?;
    mac.update(&message);
    d.check()?;
    Ok(format!(
        "sha256={}",
        hex::encode(mac.finalize().into_bytes())
    ))
}
fn authenticate(binding: &Binding, host: &Host, d: &Deadline) -> Result<()> {
    let message = unsigned(binding, d)?;
    let mut verified = false;
    for key in &host.keys {
        d.check()?;
        verified |= key.verify(&message, &binding.signature256).is_ok();
    }
    ensure!(verified, failure(BINDING));
    Ok(())
}
fn load(store: &Store, id: &str, d: &Deadline) -> Result<Option<Binding>> {
    d.check()?;
    ensure!(valid_id(id), failure(INVALID));
    let size: Option<usize> = store
        .db
        .query_row(
            "SELECT length(CAST(value AS BLOB)) FROM records WHERE kind=?1 AND id=?2",
            params![RECORD_KIND, id],
            |r| r.get(0),
        )
        .optional()?;
    let Some(size) = size else { return Ok(None) };
    ensure!(size <= ROW_BYTES, failure(CAPACITY));
    let raw: String = store.db.query_row(
        "SELECT value FROM records WHERE kind=?1 AND id=?2 AND length(CAST(value AS BLOB))=?3",
        params![RECORD_KIND, id, size],
        |r| r.get(0),
    )?;
    let binding: Binding = crate::json::parse(raw.as_bytes(), ROW_BYTES)
        .and_then(crate::json::decode)
        .map_err(|_| failure(BINDING))?;
    ensure!(
        binding.schema_version == 1
            && binding.id == id
            && binding.workspace_id == store.workspace_id
            && super::super::numeric(binding.app_id)
            && valid_id(&binding.completion_id)
            && valid_id(&binding.worker_job_id)
            && valid_id(&binding.worker_review_hash)
            && valid_id(&binding.request.snapshot_id)
            && binding_id(&store.workspace_id, binding.app_id, &binding.completion_id) == id
            && matches!(binding.head_commit_id.len(), 40 | 64)
            && binding
                .head_commit_id
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
        failure(BINDING)
    );
    PreviewRequest::parse(&crate::json::encode_bounded(&binding.request, 4096, d)?)?;
    binding.plan.validate()?;
    super::super::signature_bytes(&binding.signature256)?;
    d.check()?;
    Ok(Some(binding))
}
fn preflight(
    c: &Context<'_>,
    selection: &Selection,
    write: bool,
    current: bool,
    d: &Deadline,
) -> Result<()> {
    ensure!(
        c.options.allow_app_history,
        failure("github_app_history_permission_required")
    );
    d.check()?;
    selection.validate()?;
    c.options.worker.admit(write, d)?;
    if current {
        ensure!(
            selection.expected_queue_revision.is_some(),
            failure(INVALID)
        );
    }
    Ok(())
}
fn checked_source(
    c: &Context<'_>,
    s: &Selection,
    current: bool,
    d: &Deadline,
) -> Result<PublicationSource> {
    let source = if current {
        super::publication_source(
            c.store,
            c.host,
            &s.completion_id,
            s.expected_queue_revision.context(failure(INVALID))?,
            &c.options.worker,
            d,
        )
    } else {
        super::historical_publication_source(
            c.store,
            c.host,
            &s.completion_id,
            &c.options.worker,
            d,
        )
    }
    .map_err(|e| {
        if let Err(deadline) = d.check() {
            deadline
        } else if crate::process::cleanup_unproven(&e) {
            e
        } else {
            failure("github_app_history_worker_unavailable")
        }
    })?;
    let receipt = crate::git_workflows::validate_source(c.store, source.head_snapshot_id(), d)
        .map_err(|_| {
            d.check()
                .err()
                .unwrap_or_else(|| failure("github_app_history_git_head_required"))
        })?;
    ensure!(
        receipt["commitId"].as_str() == Some(source.job().head_sha.as_str()),
        failure("github_app_history_git_head_required")
    );
    Ok(source)
}
fn request(source: &PublicationSource) -> PreviewRequest {
    PreviewRequest {
        schema_version: 1,
        snapshot_id: source.head_snapshot_id().into(),
        installation_id: source.job().installation_id,
        repository_id: source.job().repository_id,
        repository: source.job().repository.clone(),
    }
}
fn matches_source(
    binding: &Binding,
    source: &PublicationSource,
    c: &Context<'_>,
    d: &Deadline,
) -> Result<()> {
    authenticate(binding, c.host, d)?;
    ensure!(
        binding.app_id == c.host.profile.app_id
            && binding.completion_id == source.completion_id()
            && binding.worker_job_id == source.job().id
            && binding.worker_review_hash == source.review_hash()?
            && binding.head_commit_id == source.job().head_sha
            && identity(&binding.request) == identity(&request(source)),
        failure(BINDING)
    );
    jobs::validate_app_binding(
        c.store,
        binding.app_id,
        &binding.id,
        &binding.request,
        &binding.plan,
        d,
    )?;
    Ok(())
}
fn envelope(
    binding: &Binding,
    source: &PublicationSource,
    history: Value,
    current: bool,
    binding_written: bool,
) -> Value {
    json!({"kind":"github_app_history_v1","bindingId":binding.id,"historyJobId":binding.id,
        "workspaceId":binding.workspace_id,"appId":binding.app_id,"completionId":binding.completion_id,
        "workerJobId":binding.worker_job_id,"workerReviewHash":binding.worker_review_hash,
        "headSnapshotId":binding.request.snapshot_id,"headCommitId":binding.head_commit_id,
        "queueRevision":source.queue_revision(),"queueCheckpoint":source.queue_checkpoint(),
        "currentQueueSelectionChecked":current,"workerCompletionReplayed":true,
        "remoteFreshness":"original_worker_report_only","currentHeadVerified":false,
        "bindingStateWritten":binding_written,"history":history,
        "sourceWritten":false,"latestChanged":false,"networkRequests":0,"modelCalls":0,"complete":false})
}
fn context<'a>(c: &'a Context<'_>) -> jobs::Context<'a> {
    jobs::Context {
        profile: &c.host.profile,
        digest: &c.options.digest,
        options: &c.options.jobs,
    }
}
pub fn create(
    c: &WriteContext<'_>,
    s: &Selection,
    plan: &jobs::Plan,
    d: &Deadline,
) -> Result<Value> {
    let engine = c.engine;
    let read = c.read();
    let c = &read;
    preflight(c, s, true, true, d)?;
    plan.validate()?;
    let hc = context(c);
    jobs::transaction(&hc, c.store, true, d, || {
        let source = checked_source(c, s, true, d)?;
        let id = binding_id(
            &c.store.workspace_id,
            c.host.profile.app_id,
            &s.completion_id,
        );
        if let Some(binding) = load(c.store, &id, d)? {
            ensure!(identity(&binding.plan) == identity(plan), failure(BINDING));
            matches_source(&binding, &source, c, d)?;
            let history = jobs::status_in_transaction(c.store, &hc, &id, d)?;
            return Ok(envelope(&binding, &source, history, true, false));
        }
        let request = request(&source);
        let history = jobs::create_in_transaction(engine, &hc, &id, &request, plan, d)?;
        ensure!(
            history["stateWritten"] == true,
            failure("github_app_history_foreign_job_refused")
        );
        let mut binding = Binding {
            schema_version: 1,
            id: id.clone(),
            workspace_id: c.store.workspace_id.clone(),
            app_id: c.host.profile.app_id,
            completion_id: s.completion_id.clone(),
            worker_job_id: source.job().id.clone(),
            worker_review_hash: source.review_hash()?,
            head_commit_id: source.job().head_sha.clone(),
            request,
            plan: plan.clone(),
            signature256: String::new(),
        };
        binding.signature256 = sign(&binding, c.host, d)?;
        let bytes = crate::json::encode_bounded(&binding, ROW_BYTES, d)?;
        let (count,total):(usize,usize)=c.store.db.query_row("SELECT count(*),COALESCE(sum(length(CAST(value AS BLOB))),0) FROM records WHERE kind=?1",[RECORD_KIND],|r|Ok((r.get(0)?,r.get(1)?)))?;
        ensure!(
            count < 8 && total + bytes.len() <= 128_000,
            failure(CAPACITY)
        );
        c.store.db.execute(
            "INSERT INTO records(kind,id,value) VALUES(?1,?2,?3)",
            params![RECORD_KIND, id, std::str::from_utf8(&bytes)?],
        )?;
        Ok(envelope(&binding, &source, history, true, true))
    })
}
fn existing(
    c: &Context<'_>,
    s: &Selection,
    current: bool,
    d: &Deadline,
) -> Result<(Binding, PublicationSource)> {
    let source = checked_source(c, s, current, d)?;
    let id = binding_id(
        &c.store.workspace_id,
        c.host.profile.app_id,
        &s.completion_id,
    );
    let binding = load(c.store, &id, d)?.context(failure(BINDING))?;
    matches_source(&binding, &source, c, d)?;
    Ok((binding, source))
}
pub fn status(c: &Context<'_>, s: &Selection, d: &Deadline) -> Result<Value> {
    preflight(c, s, false, false, d)?;
    let hc = context(c);
    jobs::transaction(&hc, c.store, false, d, || {
        let (binding, source) = existing(c, s, false, d)?;
        let history = jobs::status_in_transaction(c.store, &hc, &binding.id, d)?;
        Ok(envelope(&binding, &source, history, false, false))
    })
}
pub fn step(
    c: &WriteContext<'_>,
    s: &Selection,
    revision: u64,
    git: &HostGit,
    d: &Deadline,
) -> Result<Value> {
    let engine = c.engine;
    let read = c.read();
    let c = &read;
    preflight(c, s, true, true, d)?;
    let hc = context(c);
    jobs::transaction(&hc, c.store, true, d, || {
        let (binding, source) = existing(c, s, true, d)?;
        let history = jobs::step_in_transaction(engine, &hc, &binding.id, revision, git, d)?;
        Ok(envelope(&binding, &source, history, true, false))
    })
}
pub fn transition(
    c: &Context<'_>,
    s: &Selection,
    revision: u64,
    state: jobs::State,
    d: &Deadline,
) -> Result<Value> {
    let current = state == jobs::State::Active;
    preflight(c, s, true, current, d)?;
    let hc = context(c);
    jobs::transaction(&hc, c.store, true, d, || {
        let (binding, source) = existing(c, s, current, d)?;
        let history =
            jobs::transition_in_transaction(c.store, &hc, &binding.id, revision, state, d)?;
        Ok(envelope(&binding, &source, history, current, false))
    })
}
pub fn run(
    c: &WriteContext<'_>,
    s: &Selection,
    mut revision: u64,
    batches: usize,
    git: &HostGit,
    d: &Deadline,
) -> Result<Value> {
    preflight(&c.read(), s, true, true, d)?;
    ensure!((1..=128).contains(&batches), failure(CAPACITY));
    let mut result = None;
    for _ in 0..batches {
        let next = step(c, s, revision, git, d)?;
        revision = revision.checked_add(1).context(failure(CAPACITY))?;
        let finished = next["history"]["state"] == "complete";
        result = Some(next);
        if finished {
            break;
        }
    }
    result.context(failure(INVALID))
}
/// Closed structural retention validation; key lookup and execution stay off.
pub(crate) fn retained_roots(
    store: &Store,
    d: &Deadline,
) -> Result<std::collections::BTreeSet<(String, String)>> {
    let (count, total_bytes): (usize, usize) = store.db.query_row(
        "SELECT count(*),COALESCE(sum(length(CAST(value AS BLOB))),0) FROM records WHERE kind=?1",
        [RECORD_KIND],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    ensure!(count <= 8 && total_bytes <= 128_000, failure(CAPACITY));
    if count == 0 {
        return Ok(Default::default());
    }
    let mut statement = store
        .db
        .prepare("SELECT CASE WHEN length(id)=64 THEN id ELSE NULL END,length(CAST(value AS BLOB)) FROM records WHERE kind=?1 ORDER BY id")?;
    let mut roots = std::collections::BTreeSet::new();
    let mut total = 0usize;
    let bindings = super::publication_bindings(store, None, &super::Options::default(), d)?;
    for row in statement.query_map([RECORD_KIND], |r| {
        Ok((r.get::<_, Option<String>>(0)?, r.get::<_, usize>(1)?))
    })? {
        d.check()?;
        let (id, size) = row?;
        let id = id.context(failure(BINDING))?;
        total = total.checked_add(size).context(failure(CAPACITY))?;
        ensure!(
            roots.len() / 3 < 8 && total <= 128_000 && size <= ROW_BYTES,
            failure(CAPACITY)
        );
        let binding = load(store, &id, d)?.context(failure(BINDING))?;
        let source = bindings
            .get(&binding.completion_id)
            .context(failure(BINDING))?;
        ensure!(
            source.job.app_id == binding.app_id
                && source.job.id == binding.worker_job_id
                && source.review_hash == binding.worker_review_hash
                && source.head_snapshot_id == binding.request.snapshot_id
                && source.job.head_sha == binding.head_commit_id
                && source.job.installation_id == binding.request.installation_id
                && source.job.repository_id == binding.request.repository_id
                && source.job.repository == binding.request.repository,
            failure(BINDING)
        );
        let key = jobs::validate_app_binding(
            store,
            binding.app_id,
            &binding.id,
            &binding.request,
            &binding.plan,
            d,
        )?;
        let receipt =
            crate::git_workflows::validate_source(store, &binding.request.snapshot_id, d)?;
        ensure!(
            receipt["commitId"].as_str() == Some(binding.head_commit_id.as_str()),
            failure(BINDING)
        );
        roots.insert((RECORD_KIND.into(), binding.id));
        roots.insert((super::RECORD_KIND.into(), binding.completion_id));
        roots.insert((jobs::RECORD_KIND.into(), key));
    }
    Ok(roots)
}
