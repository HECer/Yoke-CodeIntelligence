//! Signed publication intents. Only a fresh committed intent can mint a send permit.
use super::*;
use hmac::{Hmac, Mac};
use rusqlite::{Transaction, TransactionBehavior, limits::Limit, params};
use std::{collections::BTreeMap, fs, time::Duration};

const BINDING: &str = "github_app_publication_binding_invalid";
const MAX_RECORD: usize = 192_000;
const MAX_RECORDS: usize = 1000;
const MAX_BYTES: usize = 64_000_000;
const MAX_TERMINAL: usize = 16_000;
const DOMAIN: &[u8] = b"yoke-code-intelligence/github-app-publication/v1\n";
struct Replay<'a> {
    store: &'a Store,
    host: Option<&'a Host>,
    options: &'a Options,
}
fn replay<'a>(c: &'a Context<'_>) -> Replay<'a> {
    Replay {
        store: c.store,
        host: Some(c.host),
        options: c.options,
    }
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Data {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub page_link: Option<super::super::pages::LinkBinding>,
    pub job: super::super::Job,
    pub completion_id: String,
    pub completed_at_unix: u64,
    pub queue_revision: usize,
    pub queue_checkpoint: String,
    pub head_snapshot_id: String,
    pub base_snapshot_id: Option<String>,
    pub review_hash: String,
    pub body: String,
    pub body_hash: String,
    pub proposal_id: String,
}
impl Data {
    fn selections(&self) -> impl Iterator<Item = (&String, usize)> {
        std::iter::once((&self.queue_checkpoint, self.queue_revision)).chain(
            self.page_link
                .iter()
                .map(|l| (&l.source_queue_checkpoint, l.source_queue_revision)),
        )
    }
    pub fn contract(&self) -> Value {
        let job = &self.job;
        let mut contract = json!({"schemaVersion":1,"rendererVersion":"yoke-pr-comment-v1",
            "workspaceId":job.workspace_id,"appId":job.app_id,"installationId":job.installation_id,
            "repositoryId":job.repository_id,"repository":job.repository,"number":job.number,
            "jobId":job.id,"completionId":self.completion_id,"queueRevision":self.queue_revision,
            "headSha":job.head_sha,"baseSha":job.base_sha,"headSnapshotId":self.head_snapshot_id,
            "baseSnapshotId":self.base_snapshot_id,"reviewHash":self.review_hash,
            "bodyHash":self.body_hash,"body":self.body});
        linked_pages::attach(&mut contract, self.page_link.as_ref());
        contract
    }
    fn validate(&self, workspace: &str) -> Result<()> {
        use crate::model::{hash, identity, valid_id};
        let j = &self.job;
        ensure!(
            j.workspace_id == workspace
                && valid_id(&j.id)
                && super::super::numeric(j.app_id)
                && super::super::numeric(j.installation_id)
                && super::super::numeric(j.repository_id)
                && super::super::numeric(j.number)
                && super::super::repository(&j.repository)
                && j.work_class == super::super::WorkClass::Review
                && [&j.head_sha, &j.base_sha].iter().all(|s| s.len() == 40
                    && s.bytes()
                        .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)))
                && [
                    &self.completion_id,
                    &self.queue_checkpoint,
                    &self.head_snapshot_id,
                    &self.review_hash,
                    &self.body_hash,
                    &self.proposal_id
                ]
                .iter()
                .all(|s| valid_id(s))
                && self.base_snapshot_id.as_ref().is_none_or(|s| valid_id(s))
                && (1..=2000).contains(&self.queue_revision)
                && (60..=253_402_296_000).contains(&self.completed_at_unix)
                && !self.body.is_empty()
                && self.body.len() <= 60_000
                && hash(self.body.as_bytes()) == self.body_hash
                && identity(&self.contract()) == self.proposal_id,
            BINDING
        );
        linked_pages::validate_data(self)
    }
    fn group(&self) -> (u64, u64, u64) {
        (self.job.app_id, self.job.repository_id, self.job.number)
    }
}
#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub(super) enum Target {
    Create {},
    Observe {
        comment_id: u64,
        body_hash: String,
    },
    Update {
        comment_id: u64,
        previous_body_hash: String,
    },
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Comment {
    pub id: u64,
    pub body_hash: String,
    pub html_url: String,
    pub fixture: bool,
}
impl Comment {
    pub fn validate(&self, data: &Data) -> Result<()> {
        ensure!(
            super::super::numeric(self.id)
                && self.body_hash == data.body_hash
                && self.html_url
                    == format!(
                        "https://github.com/{}/pull/{}#issuecomment-{}",
                        data.job.repository, data.job.number, self.id
                    ),
            BINDING
        );
        Ok(())
    }
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
enum Event {
    Intent {
        request: PublishRequest,
        data: Box<Data>,
        target: Target,
        at_unix: u64,
    },
    Published {
        intent_id: String,
        comment: Comment,
        network_requests: usize,
        write_performed: bool,
        at_unix: u64,
    },
    Reconciled {
        intent_id: String,
        comment: Comment,
        checked_installation_id: u64,
        network_requests: usize,
        at_unix: u64,
    },
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Record {
    schema_version: u32,
    workspace_id: String,
    app_id: u64,
    sequence: usize,
    event: Event,
    signature256: String,
}
struct Entry {
    id: String,
    request: PublishRequest,
    data: Data,
    target: Target,
    published: Option<(String, Comment, usize, u64)>,
    reconciled: bool,
    unchanged: bool,
}
#[derive(Default)]
struct History {
    records: Vec<(String, Record)>,
    entries: Vec<Entry>,
    requests: BTreeMap<(u64, String), usize>,
    active: BTreeMap<(u64, u64, u64), usize>,
    latest: BTreeMap<(u64, u64, u64), usize>,
    last_at: BTreeMap<u64, u64>,
    bytes: usize,
}
fn unsigned(record: &Record) -> Result<Value> {
    let mut value = serde_json::to_value(record)?;
    value
        .as_object_mut()
        .ok_or_else(|| anyhow::anyhow!(BINDING))?
        .remove("signature256");
    Ok(value)
}
fn message(record: &Record, d: &Deadline) -> Result<Vec<u8>> {
    let mut message = DOMAIN.to_vec();
    message.extend_from_slice(&crate::json::encode_bounded(
        &unsigned(record)?,
        MAX_RECORD,
        d,
    )?);
    Ok(message)
}
fn sign(record: &Record, host: &Host, d: &Deadline) -> Result<String> {
    let key = host.keys.first().ok_or_else(|| anyhow::anyhow!(BINDING))?;
    let mut mac =
        Hmac::<sha2::Sha256>::new_from_slice(&key.0).map_err(|_| anyhow::anyhow!(BINDING))?;
    for chunk in message(record, d)?.chunks(65536) {
        d.check()?;
        mac.update(chunk);
    }
    Ok(format!(
        "sha256={}",
        hex::encode(mac.finalize().into_bytes())
    ))
}
impl History {
    fn apply(
        &mut self,
        id: String,
        record: Record,
        len: usize,
        c: &Replay<'_>,
        d: &Deadline,
    ) -> Result<()> {
        d.check()?;
        ensure!(
            record.schema_version == 1
                && record.workspace_id == c.store.workspace_id
                && record.sequence == self.records.len() + 1
                && super::super::numeric(record.app_id)
                && id == crate::model::identity(&serde_json::to_value(&record)?),
            BINDING
        );
        super::super::signature_bytes(&record.signature256)
            .map_err(|_| anyhow::anyhow!(BINDING))?;
        if let Some(host) = c.host.filter(|h| record.app_id == h.profile.app_id) {
            let message = message(&record, d)?;
            ensure!(
                host.keys
                    .iter()
                    .any(|k| k.verify(&message, &record.signature256).is_ok()),
                BINDING
            );
        }
        let at = match &record.event {
            Event::Intent {
                request,
                data,
                target,
                at_unix,
            } => {
                request.validate()?;
                data.validate(&c.store.workspace_id)?;
                linked_pages::validate_intent_time(data.page_link.as_ref(), *at_unix)?;
                ensure!(
                    data.job.app_id == record.app_id
                        && *at_unix >= data.completed_at_unix
                        && request.completion_id == data.completion_id
                        && request.expected_queue_revision == data.queue_revision
                        && request.proposal_id == data.proposal_id
                        && request.body_hash == data.body_hash
                        && request
                            .page
                            .as_ref()
                            .map(|r| (&r.page_id, r.expected_page_version))
                            == data
                                .page_link
                                .as_ref()
                                .map(|l| (&l.page_id, l.page_version))
                        && !self.active.contains_key(&data.group()),
                    BINDING
                );
                let expected = self.target(data)?;
                ensure!(
                    *target == expected
                        && !self
                            .entries
                            .iter()
                            .any(|e| e.data.job.app_id == record.app_id
                                && e.data.proposal_id == data.proposal_id),
                    BINDING
                );
                let index = self.entries.len();
                ensure!(
                    self.requests
                        .insert((record.app_id, request.request_id.clone()), index)
                        .is_none(),
                    BINDING
                );
                self.active.insert(data.group(), index);
                self.entries.push(Entry {
                    id: id.clone(),
                    request: request.clone(),
                    data: *data.clone(),
                    target: target.clone(),
                    published: None,
                    reconciled: false,
                    unchanged: false,
                });
                *at_unix
            }
            Event::Published {
                intent_id,
                comment,
                network_requests,
                write_performed,
                at_unix,
            } => {
                let index = self
                    .entries
                    .iter()
                    .position(|e| e.id == *intent_id)
                    .ok_or_else(|| anyhow::anyhow!(BINDING))?;
                let entry = &mut self.entries[index];
                comment.validate(&entry.data)?;
                ensure!(
                    entry.data.job.app_id == record.app_id
                        && entry.published.is_none()
                        && self.active.get(&entry.data.group()) == Some(&index)
                        && (8..=39).contains(network_requests)
                        && *write_performed != matches!(entry.target, Target::Observe { .. }),
                    BINDING
                );
                if let Target::Update { comment_id, .. } | Target::Observe { comment_id, .. } =
                    entry.target
                {
                    ensure!(comment_id == comment.id, BINDING);
                }
                entry.published = Some((id.clone(), comment.clone(), *network_requests, *at_unix));
                entry.unchanged = !*write_performed;
                self.active.remove(&entry.data.group());
                self.latest.insert(entry.data.group(), index);
                *at_unix
            }
            Event::Reconciled {
                intent_id,
                comment,
                checked_installation_id,
                network_requests,
                at_unix,
            } => {
                let index = self
                    .entries
                    .iter()
                    .position(|e| e.id == *intent_id)
                    .ok_or_else(|| anyhow::anyhow!(BINDING))?;
                let entry = &mut self.entries[index];
                comment.validate(&entry.data)?;
                ensure!(
                    entry.data.job.app_id == record.app_id
                        && entry.published.is_none()
                        && self.active.get(&entry.data.group()) == Some(&index)
                        && *network_requests == 6
                        && super::super::numeric(*checked_installation_id)
                        && !matches!(entry.target, Target::Observe { .. }),
                    BINDING
                );
                if let Target::Update {
                    comment_id,
                    ref previous_body_hash,
                } = entry.target
                {
                    ensure!(
                        comment_id == comment.id && *previous_body_hash != entry.data.body_hash,
                        BINDING
                    );
                }
                entry.published = Some((id.clone(), comment.clone(), *network_requests, *at_unix));
                entry.reconciled = true;
                self.active.remove(&entry.data.group());
                self.latest.insert(entry.data.group(), index);
                *at_unix
            }
        };
        ensure!(
            (60..=253_402_296_000).contains(&at)
                && at >= self.last_at.get(&record.app_id).copied().unwrap_or(0),
            BINDING
        );
        self.last_at.insert(record.app_id, at);
        self.bytes = self
            .bytes
            .checked_add(len)
            .ok_or_else(|| anyhow::anyhow!(CAPACITY))?;
        ensure!(
            self.records.len() < c.options.max_journal_records.min(MAX_RECORDS)
                && self.bytes <= c.options.max_journal_bytes.min(MAX_BYTES),
            CAPACITY
        );
        self.records.push((id, record));
        Ok(())
    }
    fn target(&self, data: &Data) -> Result<Target> {
        if let Some(index) = self.latest.get(&data.group()) {
            let (_, comment, _, _) = self.entries[*index]
                .published
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!(BINDING))?;
            if comment.body_hash == data.body_hash {
                return Ok(Target::Observe {
                    comment_id: comment.id,
                    body_hash: comment.body_hash.clone(),
                });
            }
            Ok(Target::Update {
                comment_id: comment.id,
                previous_body_hash: comment.body_hash.clone(),
            })
        } else {
            Ok(Target::Create {})
        }
    }
    fn append(&mut self, event: Event, c: &Context<'_>, d: &Deadline) -> Result<(String, String)> {
        let mut record = Record {
            schema_version: 1,
            workspace_id: c.store.workspace_id.clone(),
            app_id: c.host.profile.app_id,
            sequence: self.records.len() + 1,
            event,
            signature256: String::new(),
        };
        record.signature256 = sign(&record, c.host, d)?;
        let value = serde_json::to_value(&record)?;
        let raw = crate::json::encode_bounded(&value, MAX_RECORD, d)?;
        ensure!(
            matches!(&record.event, Event::Intent { .. }) || raw.len() <= MAX_TERMINAL,
            CAPACITY
        );
        let id = crate::model::identity(&value);
        self.apply(id.clone(), record, raw.len(), &replay(c), d)?;
        Ok((id, String::from_utf8(raw)?))
    }
}
fn load(
    c: &Replay<'_>,
    cached: Option<&BTreeMap<String, worker::PublicationBinding>>,
    d: &Deadline,
) -> Result<History> {
    let (count,bytes,bad):(usize,usize,usize)=c.store.db.query_row(
        "SELECT count(*),coalesce(sum(length(CAST(value AS BLOB))),0),coalesce(sum(CASE WHEN typeof(value)!='text' OR length(CAST(value AS BLOB))>?2 OR length(CAST(id AS BLOB))!=64 THEN 1 ELSE 0 END),0) FROM records WHERE kind=?1",
        params![RECORD_KIND,MAX_RECORD],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?)))?;
    ensure!(
        count <= c.options.max_journal_records.min(MAX_RECORDS)
            && bytes <= c.options.max_journal_bytes.min(MAX_BYTES)
            && bad == 0,
        CAPACITY
    );
    let mut rows = Vec::new();
    let mut proofs: BTreeMap<String, Vec<(String, usize)>> = BTreeMap::new();
    {
        let mut statement = c
            .store
            .db
            .prepare("SELECT id,value FROM records WHERE kind=?1 ORDER BY id")?;
        for row in statement.query_map([RECORD_KIND], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
        })? {
            d.check()?;
            let (id, raw) = row?;
            let record: Record =
                crate::json::decode(crate::json::parse(raw.as_bytes(), MAX_RECORD)?)
                    .map_err(|_| anyhow::anyhow!(BINDING))?;
            if let Event::Intent { data, .. } = &record.event {
                for (checkpoint, revision) in data.selections() {
                    proofs
                        .entry(checkpoint.clone())
                        .or_default()
                        .push((data.job.id.clone(), revision));
                }
            }
            rows.push((id, record, raw.len()));
        }
    }
    let views =
        super::super::state::worker_views(c.store, c.host, &proofs, &c.options.worker.queue, d)?;
    rows.sort_by_key(|(_, r, _)| r.sequence);
    let owned = if count > 0 && cached.is_none() {
        Some(worker::publication_bindings(
            c.store,
            c.host,
            &c.options.worker,
            d,
        )?)
    } else {
        None
    };
    let bindings = cached.or(owned.as_ref());
    let mut bodies = BTreeMap::new();
    let mut history = History::default();
    for (id, record, len) in rows {
        if let Event::Intent { data, .. } = &record.event {
            let binding = bindings
                .and_then(|b| b.get(&data.completion_id))
                .ok_or_else(|| anyhow::anyhow!(BINDING))?;
            ensure!(
                binding.job == data.job
                    && binding.review_hash == data.review_hash
                    && binding.head_snapshot_id == data.head_snapshot_id
                    && binding.base_snapshot_id == data.base_snapshot_id
                    && binding.completed_at_unix == data.completed_at_unix,
                BINDING
            );
            linked_pages::validate_data(data)?;
            linked_pages::validate_worker(data, binding)?;
            let body_key = crate::model::identity(
                &json!({"completionId":data.completion_id,"pageLink":data.page_link}),
            );
            if !bodies.contains_key(&body_key) {
                let body = render_body(
                    &binding.job,
                    &data.completion_id,
                    &binding.review_hash,
                    &binding.facts,
                    data.page_link.as_ref(),
                    60_000,
                    d,
                )?;
                bodies.insert(body_key.clone(), body);
            }
            ensure!(bodies.get(&body_key) == Some(&data.body), BINDING);
            for (checkpoint, revision) in data.selections() {
                let proof = views
                    .proofs
                    .get(&(checkpoint.clone(), data.job.id.clone(), revision))
                    .ok_or_else(|| anyhow::anyhow!(BINDING))?;
                ensure!(proof.job == data.job, BINDING);
            }
        }
        history.apply(id, record, len, c, d)?;
    }
    ensure!(
        history.records.len() == count && history.bytes == bytes,
        BINDING
    );
    ensure!(
        history.records.len() + history.active.len() <= c.options.max_journal_records
            && history.bytes + history.active.len() * MAX_TERMINAL <= c.options.max_journal_bytes,
        CAPACITY
    );
    Ok(history)
}
pub(super) fn validated_inline_records(
    store: &Store,
    worker_roots: &worker::RetainedRoots,
    d: &Deadline,
) -> Result<std::collections::BTreeSet<String>> {
    let mut options = Options::default();
    options.worker.queue.max_payload_bytes = super::super::MAX_PAYLOAD_BYTES;
    options.admit(d)?;
    worker::publication_policy(store, d, |d| {
        store
            .db
            .busy_timeout(d.remaining().min(Duration::from_millis(50)))?;
        let prior = store.db.limit(Limit::SQLITE_LIMIT_LENGTH)?;
        store
            .db
            .set_limit(Limit::SQLITE_LIMIT_LENGTH, prior.min(64_065_536))?;
        let tx = if store.db.is_autocommit() {
            Some(Transaction::new_unchecked(
                &store.db,
                TransactionBehavior::Deferred,
            )?)
        } else {
            None
        };
        let context = Replay {
            store,
            host: None,
            options: &options,
        };
        let history = load(&context, Some(&worker_roots.publications), d)?;
        let result = history.records.iter().map(|(id, _)| id.clone()).collect();
        d.check()?;
        if let Some(tx) = tx {
            tx.commit()?;
        }
        d.check()?;
        Ok(result)
    })
}
fn sql<T>(
    c: &Context<'_>,
    write: bool,
    d: &Deadline,
    run: impl FnOnce(&mut History) -> Result<T>,
) -> Result<T> {
    ensure!(
        c.store.db.is_autocommit(),
        "github_app_publication_caller_transaction_refused"
    );
    if write {
        admit_durability(c, d)?;
    }
    worker::publication_policy(c.store, d, |d| {
        c.store
            .db
            .busy_timeout(d.remaining().min(Duration::from_millis(50)))?;
        let prior = c.store.db.limit(Limit::SQLITE_LIMIT_LENGTH)?;
        c.store
            .db
            .set_limit(Limit::SQLITE_LIMIT_LENGTH, prior.min(64_065_536))?;
        let tx = Transaction::new_unchecked(
            &c.store.db,
            if write {
                TransactionBehavior::Immediate
            } else {
                TransactionBehavior::Deferred
            },
        )?;
        let mut history = load(&replay(c), None, d)?;
        let result = run(&mut history)?;
        d.check()?;
        tx.commit()?;
        d.check()?;
        Ok(result)
    })
}
pub(super) fn admit_durability(c: &Context<'_>, d: &Deadline) -> Result<()> {
    d.check()?;
    let synchronous: u32 = c
        .store
        .db
        .query_row("PRAGMA synchronous", [], |r| r.get(0))?;
    let journal: String = c
        .store
        .db
        .query_row("PRAGMA journal_mode", [], |r| r.get(0))?;
    ensure!(
        (2..=3).contains(&synchronous) && journal == "wal",
        "github_app_publication_sql_durability_required"
    );
    Ok(())
}
fn value(entry: &Entry, historical: bool) -> Value {
    let published = entry.published.as_ref();
    json!({"status":if published.is_some(){"published"}else{"unknown"},"historical":historical,
        "intentId":entry.id,"publicationId":published.map(|p|&p.0),"jobId":entry.data.job.id,
        "completionId":entry.data.completion_id,"proposalId":entry.data.proposal_id,"bodyHash":entry.data.body_hash,
        "commentId":published.map(|p|p.1.id),"htmlUrl":published.map(|p|&p.1.html_url),
        "fixture":published.map(|p|p.1.fixture),"networkRequests":if historical {0}else{published.map_or(0,|p|p.2)},
        "published":published.is_some(),"publicationAuthority":false,"sourceWritten":false,"complete":false,"modelCalls":0,
        "currentHeadVerified":!historical && published.is_some() && !entry.reconciled,"installationTokenVerified":!historical && published.is_some(),
        "reconciled":entry.reconciled,"unchanged":entry.unchanged,
        "writeConfirmed":published.is_some() && !entry.reconciled && !entry.unchanged,
        "providerScope":"two_matching_preflight_observations_not_atomic_remote_or_whole_tree_proof"})
}
fn output(value: &Value, c: &Context<'_>, d: &Deadline) -> Result<()> {
    crate::json::encode_bounded(value, c.options.max_output_bytes - 1, d)
        .map_err(|_| d.check().err().unwrap_or_else(|| anyhow::anyhow!(CAPACITY)))?;
    Ok(())
}
fn checked_source(
    entry: &Entry,
    c: &Context<'_>,
    installation: Option<u64>,
    d: &Deadline,
) -> Result<()> {
    let source = if let Some(installation) = installation {
        worker::recovery_publication_source(
            c.store,
            c.host,
            &entry.data.completion_id,
            installation,
            &c.options.worker,
            d,
        )?
    } else {
        worker::historical_publication_source(
            c.store,
            c.host,
            &entry.data.completion_id,
            &c.options.worker,
            d,
        )?
    };
    ensure!(
        source.job() == &entry.data.job && source.review_hash()? == entry.data.review_hash,
        BINDING
    );
    let rendered = proposal(
        c.store,
        &source,
        entry.data.page_link.as_ref(),
        c.options,
        d,
    )?;
    ensure!(
        rendered["bodyHash"] == entry.data.body_hash && rendered["body"] == entry.data.body,
        BINDING
    );
    Ok(())
}
// Historical reads use a current repository grant without changing the original
// job's installation or creating authority for another send.
fn checked_current_source(entry: &Entry, c: &Context<'_>, d: &Deadline) -> Result<()> {
    let grant = c
        .host
        .profile
        .grants
        .iter()
        .find(|g| {
            g.repository_id == entry.data.job.repository_id
                && g.repository == entry.data.job.repository
        })
        .ok_or_else(|| anyhow::anyhow!("github_app_worker_grant_denied"))?;
    checked_source(entry, c, Some(grant.installation_id), d)
}
pub(super) fn lookup(
    c: &Context<'_>,
    request: &PublishRequest,
    d: &Deadline,
) -> Result<Option<Value>> {
    sql(c, false, d, |h| {
        let Some(index) = h
            .requests
            .get(&(c.host.profile.app_id, request.request_id.clone()))
        else {
            return Ok(None);
        };
        let entry = &h.entries[*index];
        ensure!(
            &entry.request == request,
            "github_app_publication_request_conflict"
        );
        checked_current_source(entry, c, d)?;
        let value = value(entry, true);
        output(&value, c, d)?;
        Ok(Some(value))
    })
}
pub(super) struct JournalLock {
    _file: fs::File,
}
pub(super) fn status(c: &Context<'_>, request: &StatusRequest, d: &Deadline) -> Result<Value> {
    sql(c, false, d, |h| {
        let index = h
            .requests
            .get(&(c.host.profile.app_id, request.request_id.clone()))
            .ok_or_else(|| anyhow::anyhow!("github_app_publication_intent_unavailable"))?;
        let entry = &h.entries[*index];
        checked_current_source(entry, c, d)?;
        let result = value(entry, true);
        output(&result, c, d)?;
        Ok(result)
    })
}
pub(super) enum Recovery {
    Historical(Value),
    Pending(Box<Data>),
}
fn recovery_index(h: &History, c: &Context<'_>, request: &RecoverRequest) -> Result<usize> {
    h.entries
        .iter()
        .position(|e| e.id == request.intent_id && e.data.job.app_id == c.host.profile.app_id)
        .ok_or_else(|| anyhow::anyhow!("github_app_publication_intent_unavailable"))
}
fn recovery_source(
    entry: &Entry,
    c: &Context<'_>,
    request: &RecoverRequest,
    d: &Deadline,
) -> Result<()> {
    if let Some((_, comment, _, _)) = &entry.published {
        ensure!(
            comment.id == request.comment_id,
            "github_app_publication_request_conflict"
        );
    }
    if let Target::Update {
        comment_id,
        previous_body_hash,
    } = &entry.target
    {
        ensure!(
            *comment_id == request.comment_id,
            "github_app_publication_request_conflict"
        );
        ensure!(
            *previous_body_hash != entry.data.body_hash,
            "github_app_publication_ambiguous_update"
        );
    }
    checked_source(entry, c, Some(request.installation_id), d)
}
pub(super) fn recovery(
    c: &Context<'_>,
    request: &RecoverRequest,
    d: &Deadline,
) -> Result<Recovery> {
    sql(c, false, d, |h| {
        let index = recovery_index(h, c, request)?;
        let entry = &h.entries[index];
        recovery_source(entry, c, request, d)?;
        if entry.published.is_some() {
            let result = value(entry, true);
            output(&result, c, d)?;
            return Ok(Recovery::Historical(result));
        }
        ensure!(
            h.active.get(&entry.data.group()) == Some(&index)
                && !matches!(entry.target, Target::Observe { .. }),
            BINDING
        );
        Ok(Recovery::Pending(Box::new(entry.data.clone())))
    })
}
pub(super) fn reconcile(
    c: &Context<'_>,
    request: &RecoverRequest,
    comment: Comment,
    requests: usize,
    token_check: impl Fn() -> Result<()>,
    d: &Deadline,
) -> Result<Value> {
    sql(c, true, d, |h| {
        let index = recovery_index(h, c, request)?;
        let entry = &h.entries[index];
        recovery_source(entry, c, request, d)?;
        comment.validate(&entry.data)?;
        ensure!(comment.id == request.comment_id && requests == 6, BINDING);
        if entry.published.is_some() {
            let result = value(entry, true);
            output(&result, c, d)?;
            return Ok(result);
        }
        ensure!(
            h.active.get(&entry.data.group()) == Some(&index)
                && !matches!(entry.target, Target::Observe { .. }),
            BINDING
        );
        let at = c.clock.now()?;
        ensure!(
            at >= h.last_at.get(&c.host.profile.app_id).copied().unwrap_or(0),
            "github_app_publication_clock_regressed"
        );
        token_check()?;
        let pending = h.append(
            Event::Reconciled {
                intent_id: request.intent_id.clone(),
                comment,
                checked_installation_id: request.installation_id,
                network_requests: requests,
                at_unix: at,
            },
            c,
            d,
        )?;
        let result = value(&h.entries[index], false);
        output(&result, c, d)?;
        token_check()?;
        c.store.db.execute(
            "INSERT INTO records(kind,id,value) VALUES(?1,?2,?3)",
            params![RECORD_KIND, pending.0, pending.1],
        )?;
        token_check()?;
        d.check()?;
        Ok(result)
    })
}
pub(super) fn observe(
    c: &Context<'_>,
    request: &PublishRequest,
    comment: Comment,
    requests: usize,
    token_check: impl Fn() -> Result<()>,
    d: &Deadline,
) -> Result<Value> {
    sql(c, true, d, |h| {
        let data = prepared_data(c, request, d)?;
        comment.validate(&data)?;
        ensure!(
            !h.active.contains_key(&data.group()),
            "github_app_publication_outcome_unknown"
        );
        ensure!(
            !h.requests
                .contains_key(&(data.job.app_id, request.request_id.clone()))
                && !h
                    .entries
                    .iter()
                    .any(|e| e.data.proposal_id == data.proposal_id),
            "github_app_publication_request_conflict"
        );
        let target = h.target(&data)?;
        ensure!(
            matches!(&target,Target::Observe {comment_id,body_hash} if *comment_id == comment.id && *body_hash == data.body_hash),
            BINDING
        );
        reserve(h, c)?;
        let at = c.clock.now()?;
        ensure!(
            at >= data
                .completed_at_unix
                .max(h.last_at.get(&data.job.app_id).copied().unwrap_or(0)),
            "github_app_publication_clock_regressed"
        );
        token_check()?;
        let intent = h.append(
            Event::Intent {
                request: request.clone(),
                data: Box::new(data),
                target,
                at_unix: at,
            },
            c,
            d,
        )?;
        let index = h.entries.len() - 1;
        let terminal = h.append(
            Event::Published {
                intent_id: intent.0.clone(),
                comment,
                network_requests: requests,
                write_performed: false,
                at_unix: at,
            },
            c,
            d,
        )?;
        let result = value(&h.entries[index], false);
        output(&result, c, d)?;
        token_check()?;
        for pending in [intent, terminal] {
            c.store.db.execute(
                "INSERT INTO records(kind,id,value) VALUES(?1,?2,?3)",
                params![RECORD_KIND, pending.0, pending.1],
            )?;
            token_check()?;
            d.check()?;
        }
        Ok(result)
    })
}
pub(super) fn plan(c: &Context<'_>, data: &Data, d: &Deadline) -> Result<Target> {
    sql(c, false, d, |h| {
        ensure!(
            !h.active.contains_key(&data.group()),
            "github_app_publication_outcome_unknown"
        );
        ensure!(
            !h.entries
                .iter()
                .any(|e| e.data.proposal_id == data.proposal_id),
            "github_app_publication_request_conflict"
        );
        ensure!(
            c.clock.now()?
                >= data
                    .completed_at_unix
                    .max(h.last_at.get(&data.job.app_id).copied().unwrap_or(0)),
            "github_app_publication_clock_regressed"
        );
        reserve(h, c)?;
        h.target(data)
    })
}
fn reserve(h: &History, c: &Context<'_>) -> Result<()> {
    ensure!(
        h.records.len() + h.active.len() + 2 <= c.options.max_journal_records
            && h.bytes + h.active.len() * MAX_TERMINAL + MAX_RECORD + MAX_TERMINAL
                <= c.options.max_journal_bytes,
        CAPACITY
    );
    Ok(())
}
pub(super) fn lock(c: &Context<'_>, d: &Deadline) -> Result<JournalLock> {
    let path = c.store.dir.join("github-publication.lock");
    if let Ok(meta) = fs::symlink_metadata(&path) {
        ensure!(meta.is_file() && !crate::paths::is_link(&meta), BINDING);
    }
    let mut open = fs::OpenOptions::new();
    open.read(true).write(true).create(true).truncate(false);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        // Read/write sharing permits waiters; deny delete sharing while any owner is alive.
        open.share_mode(0x00000001 | 0x00000002);
    }
    let file = open.open(path)?;
    loop {
        d.check()?;
        match fs2::FileExt::try_lock_exclusive(&file) {
            Ok(()) => return Ok(JournalLock { _file: file }),
            Err(e)
                if e.kind() == std::io::ErrorKind::WouldBlock
                    || e.raw_os_error() == fs2::lock_contended_error().raw_os_error() =>
            {
                std::thread::sleep(d.remaining().min(Duration::from_millis(5)))
            }
            Err(_) => anyhow::bail!(BINDING),
        }
    }
}
pub(super) struct Permit {
    pub id: String,
    pub data: Data,
    pub target: Target,
}
pub(super) fn intend(
    c: &Context<'_>,
    request: &PublishRequest,
    token_check: impl Fn() -> Result<()>,
    d: &Deadline,
) -> Result<Permit> {
    sql(c, true, d, |h| {
        let data = prepared_data(c, request, d)?;
        ensure!(
            !h.active.contains_key(&data.group()),
            "github_app_publication_outcome_unknown"
        );
        ensure!(
            !h.requests
                .contains_key(&(data.job.app_id, request.request_id.clone()))
                && !h
                    .entries
                    .iter()
                    .any(|e| e.data.proposal_id == data.proposal_id),
            "github_app_publication_request_conflict"
        );
        let target = h.target(&data)?;
        ensure!(!matches!(target, Target::Observe { .. }), BINDING);
        reserve(h, c)?;
        let at = c.clock.now()?;
        ensure!(
            at >= data
                .completed_at_unix
                .max(h.last_at.get(&data.job.app_id).copied().unwrap_or(0)),
            "github_app_publication_clock_regressed"
        );
        token_check()?;
        let pending = h.append(
            Event::Intent {
                request: request.clone(),
                data: Box::new(data.clone()),
                target: target.clone(),
                at_unix: at,
            },
            c,
            d,
        )?;
        let entry = h.entries.last().ok_or_else(|| anyhow::anyhow!(BINDING))?;
        output(&value(entry, true), c, d)?;
        // Admit a worst-sized checked result before the irreversible request.
        let mut complete = value(entry, false);
        complete["publicationId"] = json!("f".repeat(64));
        complete["commentId"] = json!(9_007_199_254_740_991u64);
        complete["networkRequests"] = json!(39);
        complete["htmlUrl"] = json!(format!(
            "https://github.com/{}/pull/{}#issuecomment-9007199254740991",
            data.job.repository, data.job.number
        ));
        complete["status"] = json!("published");
        complete["published"] = json!(true);
        output(&complete, c, d)?;
        token_check()?;
        c.store.db.execute(
            "INSERT INTO records(kind,id,value) VALUES(?1,?2,?3)",
            params![RECORD_KIND, pending.0, pending.1],
        )?;
        token_check()?;
        d.check()?;
        Ok(Permit {
            id: pending.0,
            data,
            target,
        })
    })
}
pub(super) fn finish(
    c: &Context<'_>,
    permit: &Permit,
    comment: Comment,
    requests: usize,
    token_check: impl Fn() -> Result<()>,
    d: &Deadline,
) -> Result<Value> {
    sql(c, true, d, |h| {
        comment.validate(&permit.data)?;
        let index = h
            .entries
            .iter()
            .position(|e| e.id == permit.id)
            .ok_or_else(|| anyhow::anyhow!(BINDING))?;
        ensure!(
            h.entries[index].published.is_none()
                && h.active.get(&permit.data.group()) == Some(&index),
            BINDING
        );
        // Recheck captured bytes and current selection; historical success cannot authorize a write.
        let current =
            prepared_captured_data(c, &h.entries[index].request, &h.entries[index].data, d)?;
        ensure!(current.proposal_id == permit.data.proposal_id, BINDING);
        token_check()?;
        let at = c.clock.now()?;
        ensure!(
            at >= h.last_at.get(&c.host.profile.app_id).copied().unwrap_or(0),
            "github_app_publication_clock_regressed"
        );
        let pending = h.append(
            Event::Published {
                intent_id: permit.id.clone(),
                comment,
                network_requests: requests,
                write_performed: true,
                at_unix: at,
            },
            c,
            d,
        )?;
        let value = value(&h.entries[index], false);
        output(&value, c, d)?;
        token_check()?;
        c.store.db.execute(
            "INSERT INTO records(kind,id,value) VALUES(?1,?2,?3)",
            params![RECORD_KIND, pending.0, pending.1],
        )?;
        token_check()?;
        d.check()?;
        Ok(value)
    })
}
