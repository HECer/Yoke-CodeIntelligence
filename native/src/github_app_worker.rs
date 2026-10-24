//! Durable host-selected workers; execution and publication authority stay explicit.
use super::Host;
use crate::{capture::Deadline, store::Store};
use anyhow::{Result, ensure};
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::{SystemTime, UNIX_EPOCH},
};
#[path = "github_app_worker_history.rs"]
pub mod history;
#[path = "github_app_worker_source.rs"]
mod source;
#[path = "github_app_worker_state.rs"]
mod state;
pub const RECORD_KIND: &str = "github_app_work_v1";
const INVALID: &str = "github_app_worker_input_invalid";
const BINDING: &str = "github_app_worker_binding_invalid";
const CAPACITY: &str = "github_app_worker_capacity_exceeded";
const MAX_RECORD_BYTES: usize = 4_100_000;
const MAC_DOMAIN: &[u8] = b"yoke-code-intelligence/github-app-worker/v1\n";
#[derive(Clone)]
pub struct Options {
    pub queue: super::Options,
    pub max_records: usize,
    pub max_store_bytes: usize,
    pub max_output_bytes: usize,
    pub max_lease_seconds: u64,
}
impl Default for Options {
    fn default() -> Self {
        Self {
            queue: super::Options::default(),
            max_records: 1000,
            max_store_bytes: 64_000_000,
            max_output_bytes: 64_000,
            max_lease_seconds: 900,
        }
    }
}
impl Options {
    pub fn admit(&self, write: bool, d: &Deadline) -> Result<()> {
        ensure!(
            !write || self.queue.allow_writes,
            "github_app_permission_required"
        );
        d.check()?;
        super::options(&self.queue)?;
        ensure!(
            self.max_records > 0
                && self.max_store_bytes > 0
                && self.max_output_bytes > 0
                && self.max_lease_seconds > 0,
            CAPACITY
        );
        ensure!(
            self.max_records <= 1000
                && self.max_store_bytes <= 64_000_000
                && self.max_output_bytes <= 4_000_000
                && self.max_lease_seconds <= 900,
            INVALID
        );
        Ok(())
    }
}
pub struct Clock {
    _fixture: Option<Arc<AtomicU64>>,
    observed: AtomicU64,
}
impl Clock {
    pub fn system() -> Self {
        Self {
            _fixture: None,
            observed: AtomicU64::new(0),
        }
    }
    /// Explicit in-process host fixture capability; never selected by stored or incoming JSON.
    pub fn for_fixture(clock: Arc<AtomicU64>) -> Self {
        Self {
            _fixture: Some(clock),
            observed: AtomicU64::new(0),
        }
    }
    pub(crate) fn now(&self) -> Result<u64> {
        let now = if let Some(clock) = &self._fixture {
            clock.load(Ordering::SeqCst)
        } else {
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_err(|_| anyhow::anyhow!(INVALID))?
                .as_secs()
        };
        ensure!((60..=253_402_296_000).contains(&now), INVALID);
        // Read observations stay monotonic for this host clock capability. Across
        // processes/restarts only persisted transition timestamps establish a floor.
        self.observed
            .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |prior| {
                (now >= prior).then_some(now)
            })
            .map_err(|_| anyhow::anyhow!("github_app_worker_clock_regressed"))?;
        Ok(now)
    }
}
#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ClaimRequest {
    pub schema_version: u32,
    pub job_id: String,
    pub expected_queue_revision: usize,
    pub worker: String,
    pub request_id: String,
    pub lease_seconds: u64,
}
impl ClaimRequest {
    pub fn parse(raw: &[u8]) -> Result<Self> {
        let request: Self = crate::json::decode(crate::json::parse(raw, 4096)?)?;
        request.validate()?;
        Ok(request)
    }
    fn validate(&self) -> Result<()> {
        ensure!(
            self.schema_version == 1
                && crate::model::valid_id(&self.job_id)
                && (1..=2000).contains(&self.expected_queue_revision)
                && worker_id(&self.worker)
                && uuid(&self.request_id)
                && (1..=900).contains(&self.lease_seconds),
            INVALID
        );
        Ok(())
    }
}
pub struct Claim {
    evidence: Value,
    lease: state::Lease,
}
impl Claim {
    pub fn evidence(&self) -> &Value {
        &self.evidence
    }
}
pub struct StatusRequest {
    pub offset: usize,
    pub limit: usize,
}
impl Default for StatusRequest {
    fn default() -> Self {
        Self {
            offset: 0,
            limit: 100,
        }
    }
}
pub fn claim(
    _store: &Store,
    _host: &Host,
    _request: &ClaimRequest,
    _o: &Options,
    _clock: &Clock,
    _d: &Deadline,
) -> Result<Claim> {
    _o.admit(true, _d)?;
    _request.validate()?;
    ensure!(_request.lease_seconds <= _o.max_lease_seconds, CAPACITY);
    state::claim(_store, _host, _request, _o, _clock, _d)
}
pub fn release(
    _store: &Store,
    _host: &Host,
    _claim: &Claim,
    _request_id: &str,
    _o: &Options,
    _clock: &Clock,
    _d: &Deadline,
) -> Result<Value> {
    _o.admit(true, _d)?;
    ensure!(uuid(_request_id), INVALID);
    state::release(_store, _host, _claim, _request_id, _o, _clock, _d)
}
pub fn status(
    _store: &Store,
    _host: &Host,
    _request: &StatusRequest,
    _o: &Options,
    _clock: &Clock,
    _d: &Deadline,
) -> Result<Value> {
    _o.admit(false, _d)?;
    ensure!(_request.offset <= 1000 && _request.limit <= 100, INVALID);
    state::status(_store, _host, _request, _o, _clock, _d)
}
/// Reconstruct an opaque identity handle from authenticated durable history.
/// The returned receipt is historical; execution must recheck current authority.
pub fn recover(
    _store: &Store,
    _host: &Host,
    _claim_id: &str,
    _o: &Options,
    _clock: &Clock,
    _d: &Deadline,
) -> Result<Claim> {
    _o.admit(false, _d)?;
    ensure!(crate::model::valid_id(_claim_id), INVALID);
    state::recover(_store, _host, _claim_id, _o, _clock, _d)
}
fn worker_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 80
        && value.as_bytes()[0].is_ascii_alphanumeric()
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b'.'))
}
fn uuid(value: &str) -> bool {
    value.len() == 36
        && value.bytes().enumerate().all(|(i, b)| {
            if [8, 13, 18, 23].contains(&i) {
                b == b'-'
            } else {
                b.is_ascii_digit() || (b'a'..=b'f').contains(&b)
            }
        })
}
fn boundary() -> Value {
    json!({"sourceWritten":false,"published":false,"complete":false,"modelCalls":0,"networkRequests":0,
        "workerExecutionVerified":false,"installationTokenVerified":false,"currentHeadVerified":false})
}
fn mac_message(value: &Value, d: &Deadline) -> Result<Vec<u8>> {
    let raw = crate::json::encode_bounded(value, MAX_RECORD_BYTES, d)?;
    let mut message = Vec::with_capacity(MAC_DOMAIN.len() + raw.len());
    message.extend_from_slice(MAC_DOMAIN);
    message.extend_from_slice(&raw);
    Ok(message)
}
fn sign(host: &Host, value: &Value, d: &Deadline) -> Result<String> {
    let message = mac_message(value, d)?;
    let key = host.keys.first().ok_or_else(|| anyhow::anyhow!(BINDING))?;
    let mut mac =
        Hmac::<sha2::Sha256>::new_from_slice(&key.0).map_err(|_| anyhow::anyhow!(BINDING))?;
    for chunk in message.chunks(65536) {
        d.check()?;
        mac.update(chunk);
    }
    Ok(format!(
        "sha256={}",
        hex::encode(mac.finalize().into_bytes())
    ))
}
fn verify(host: &Host, value: &Value, signature: &str, d: &Deadline) -> Result<()> {
    let message = mac_message(value, d)?;
    let mut verified = false;
    for key in &host.keys {
        d.check()?;
        verified |= key.verify(&message, signature).is_ok();
    }
    ensure!(verified, BINDING);
    Ok(())
}

/// Structural retention proof only; no key lookup, worker authority or generic CAS walk.
pub fn validated_inline_records(
    store: &Store,
    d: &Deadline,
) -> Result<std::collections::BTreeSet<String>> {
    Ok(state::retained_roots(store, d)?.record_ids)
}
pub(crate) struct RetainedRoots {
    pub record_ids: std::collections::BTreeSet<String>,
    pub snapshot_ids: std::collections::BTreeSet<String>,
    pub publications: std::collections::BTreeMap<String, PublicationBinding>,
}
/// Closed historical report facts for replay/retention; never a source or send capability.
pub(crate) struct PublicationBinding {
    pub job: super::Job,
    pub completed_at_unix: u64,
    pub review_hash: String,
    pub head_snapshot_id: String,
    pub base_snapshot_id: Option<String>,
    pub facts: Value,
    /// Fixed-bound display projection only. None must not invalidate other history.
    pub page_graph_hash: Option<String>,
}
pub(crate) fn publication_bindings(
    store: &Store,
    host: Option<&Host>,
    options: &Options,
    d: &Deadline,
) -> Result<std::collections::BTreeMap<String, PublicationBinding>> {
    state::publication_bindings(store, host, options, d)
}
pub(crate) fn retained_roots(store: &Store, d: &Deadline) -> Result<RetainedRoots> {
    state::retained_roots(store, d)
}

/// Minted only by authenticated history replay with fresh selected source checks.
/// It cannot be deserialized from a completion receipt supplied by a caller.
pub(crate) struct PublicationSource {
    job: super::Job,
    completion_id: String,
    queue_revision: usize,
    queue_checkpoint: String,
    completed_at_unix: u64,
    review: source::Review,
}
impl PublicationSource {
    pub(crate) fn head_snapshot_id(&self) -> &str {
        &self.review.head_snapshot_id
    }
    pub(crate) fn completed_at_unix(&self) -> u64 {
        self.completed_at_unix
    }
    pub(crate) fn job(&self) -> &super::Job {
        &self.job
    }
    pub(crate) fn completion_id(&self) -> &str {
        &self.completion_id
    }
    pub(crate) fn queue_revision(&self) -> usize {
        self.queue_revision
    }
    pub(crate) fn queue_checkpoint(&self) -> &str {
        &self.queue_checkpoint
    }
    pub(crate) fn review_hash(&self) -> Result<String> {
        Ok(crate::model::identity(&serde_json::to_value(&self.review)?))
    }
    pub(crate) fn facts(&self) -> Value {
        self.review.publication_facts()
    }
    /// Native replayed impacts for the closed page projector, never caller HTML.
    pub(crate) fn page_impacts(&self) -> (&Value, Option<&Value>) {
        self.review.page_impacts()
    }
    pub(crate) fn matches_provider(
        &self,
        provider: &crate::github_app_auth::read::PullRequest,
    ) -> bool {
        self.review.matches_provider(provider)
    }
}
pub(crate) fn publication_source(
    store: &Store,
    host: &Host,
    completion_id: &str,
    revision: usize,
    options: &Options,
    d: &Deadline,
) -> Result<PublicationSource> {
    state::publication_source(store, host, completion_id, Some(revision), None, options, d)
}
pub(crate) fn historical_publication_source(
    store: &Store,
    host: &Host,
    completion_id: &str,
    options: &Options,
    d: &Deadline,
) -> Result<PublicationSource> {
    state::publication_source(store, host, completion_id, None, None, options, d)
}
/// Historical source plus a current repository-read grant, never selected send authority.
pub(crate) fn recovery_publication_source(
    store: &Store,
    host: &Host,
    completion_id: &str,
    installation: u64,
    options: &Options,
    d: &Deadline,
) -> Result<PublicationSource> {
    state::publication_source(
        store,
        host,
        completion_id,
        None,
        Some(installation),
        options,
        d,
    )
}
pub(crate) fn publication_policy<T>(
    store: &Store,
    d: &Deadline,
    run: impl FnOnce(&Deadline) -> Result<T>,
) -> Result<T> {
    source::policy(store, d, run)
}

#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RunRequest {
    pub schema_version: u32,
    pub claim_id: String,
    pub head_snapshot_id: String,
    pub base_snapshot_id: Option<String>,
    pub request_id: String,
}
impl RunRequest {
    pub fn parse(raw: &[u8]) -> Result<Self> {
        let request: Self = crate::json::decode(crate::json::parse(raw, 4096)?)?;
        request.validate()?;
        Ok(request)
    }
    fn validate(&self) -> Result<()> {
        ensure!(
            self.schema_version == 1
                && crate::model::valid_id(&self.claim_id)
                && crate::model::valid_id(&self.head_snapshot_id)
                && self
                    .base_snapshot_id
                    .as_ref()
                    .is_none_or(|id| crate::model::valid_id(id))
                && uuid(&self.request_id),
            INVALID
        );
        Ok(())
    }
}
#[derive(Clone)]
pub struct RunOptions {
    pub allow_network: bool,
    pub max_requests: usize,
    pub max_response_bytes: usize,
    pub max_total_bytes: usize,
    pub max_report_bytes: usize,
    pub max_files: usize,
}
impl Default for RunOptions {
    fn default() -> Self {
        Self {
            allow_network: false,
            max_requests: 33,
            max_response_bytes: 1_000_000,
            max_total_bytes: 8_000_000,
            max_report_bytes: 3_000_000,
            max_files: 3000,
        }
    }
}
impl RunOptions {
    pub fn admit(&self, d: &Deadline) -> Result<()> {
        d.check()?;
        ensure!(
            self.max_requests > 0
                && self.max_response_bytes > 0
                && self.max_total_bytes > 0
                && self.max_report_bytes > 0
                && self.max_files > 0,
            CAPACITY
        );
        ensure!(
            self.max_requests <= 33
                && self.max_response_bytes <= 2_000_000
                && self.max_total_bytes <= 64_000_000
                && self.max_report_bytes <= 3_000_000
                && self.max_files <= 3000,
            INVALID
        );
        ensure!(self.max_requests >= 3, CAPACITY);
        Ok(())
    }
}
pub struct RunContext<'a> {
    pub store: &'a Store,
    pub host: &'a Host,
    pub options: &'a Options,
    pub clock: &'a Clock,
}
pub fn run(
    _context: &RunContext<'_>,
    _client: &mut crate::github_app_auth::Client,
    _request: &RunRequest,
    _remote: &RunOptions,
    _d: &Deadline,
) -> Result<Value> {
    ensure!(
        _context.options.queue.allow_writes,
        "github_app_permission_required"
    );
    ensure!(
        _remote.allow_network,
        "github_app_network_permission_required"
    );
    _context.options.admit(true, _d)?;
    _remote.admit(_d)?;
    _request.validate()?;
    ensure!(
        tokio::runtime::Handle::try_current().is_err(),
        "github_app_requires_synchronous_host"
    );
    ensure!(
        _context.store.db.is_autocommit(),
        "github_app_worker_caller_transaction_refused"
    );
    source::policy(_context.store, _d, |_d| {
        let admit_historical = |value: Value| -> Result<Value> {
            crate::json::encode_bounded(&value["review"], _remote.max_report_bytes, _d)
                .map_err(|_| anyhow::anyhow!(CAPACITY))?;
            Ok(value)
        };
        if let state::Start::Historical(value) = state::begin_run(_context, _request, _d)? {
            return admit_historical(value);
        }
        let prepared = source::prepare(_context.store, _request, _d)?;
        // Source validation can be substantial; refresh authority before any token mint.
        let (claim, job) = match state::begin_run(_context, _request, _d)? {
            state::Start::Historical(value) => return admit_historical(value),
            state::Start::Active { claim, job } => (claim, job),
        };
        crate::github_app_auth::read::admit_job(_client, &job, _d)?;
        let token = _client.authorize(job.installation_id, job.repository_id, _d)?;
        let mut reader = crate::github_app_auth::read::Reader::new(
            _client,
            &token,
            &job,
            crate::github_app_auth::read::Limits {
                max_requests: _remote.max_requests,
                max_response_bytes: _remote.max_response_bytes,
                max_total_bytes: _remote.max_total_bytes,
                max_files: _remote.max_files,
            },
            _d,
        )?;
        reader.repository()?;
        let initial = reader.pull_request()?;
        let files = reader.files(&initial)?;
        let analysis = source::available(
            prepared.analyze(_context.store, &files, &job, initial, _d),
            _d,
        )?;
        reader.pull_request()?;
        let review = analysis.finish(reader.evidence()?);
        crate::json::encode_bounded(&review, _remote.max_report_bytes, _d)
            .map_err(|_| anyhow::anyhow!(CAPACITY))?;
        state::complete(
            _context,
            _request,
            &claim,
            review,
            || reader.check_lease(),
            _remote.max_report_bytes,
            _d,
        )
    })
}
