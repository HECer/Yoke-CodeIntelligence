//! Complete bounded inline event replay and transactional output-first publication.
use super::*;
use rusqlite::{Transaction, TransactionBehavior, limits::Limit, params};
use std::collections::BTreeMap;

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Record {
    schema_version: u32,
    workspace_id: String,
    sequence: usize,
    event: Event,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
enum Event {
    Delivery {
        headers: Headers,
        payload: String,
        payload_hash: String,
        job: Box<Job>,
    },
    Select {
        app_id: u64,
        job_id: String,
        expected_revision: usize,
        affected: Vec<String>,
    },
}
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct JobState {
    job: Job,
    title: String,
    draft: bool,
    first_delivery_id: String,
    status: &'static str,
    group_revision: usize,
}
#[derive(Default)]
struct History {
    records: Vec<(String, Record)>,
    jobs: BTreeMap<String, JobState>,
    deliveries: BTreeMap<(u64, String), (String, String)>,
    payloads: BTreeMap<(u64, String), String>,
    revisions: BTreeMap<Group, usize>,
    transitions: usize,
    store_bytes: usize,
}
struct Admission<'a> {
    workspace: &'a str,
    host: Option<&'a Host>,
    options: &'a Options,
    deadline: &'a Deadline,
}
type Pending = (String, String);
type OperationResult = (Value, Option<Pending>);
impl History {
    fn apply(
        &mut self,
        id: String,
        record: Record,
        raw_len: usize,
        context: &Admission<'_>,
    ) -> Result<()> {
        let Admission {
            workspace,
            host,
            options: o,
            deadline: d,
        } = *context;
        d.check()?;
        ensure!(
            record.schema_version == 1
                && record.workspace_id == workspace
                && record.sequence == self.records.len() + 1
                && id == identity(&record),
            BINDING
        );
        match &record.event {
            Event::Delivery {
                headers,
                payload,
                payload_hash,
                job,
            } => {
                headers.validate()?;
                ensure!(
                    payload.len() <= o.max_payload_bytes
                        && hash(payload.as_bytes()) == *payload_hash,
                    BINDING
                );
                let checked = checked_payload(payload.as_bytes(), job.app_id, workspace)?;
                ensure!(job.as_ref() == &checked.job, BINDING);
                if let Some(host) = host
                    && job.app_id == host.profile.app_id
                {
                    host.verify(payload.as_bytes(), &headers.signature256)?;
                }
                let delivery_key = (job.app_id, headers.delivery_id.clone());
                let payload_key = (job.app_id, payload_hash.clone());
                ensure!(
                    !self.deliveries.contains_key(&delivery_key)
                        && !self.payloads.contains_key(&payload_key),
                    BINDING
                );
                self.deliveries
                    .insert(delivery_key, (payload_hash.clone(), job.id.clone()));
                self.payloads.insert(payload_key, job.id.clone());
                if !self.jobs.contains_key(&job.id) {
                    let group = job.group();
                    let revision = self.revisions.get(&group).copied().unwrap_or(0) + 1;
                    for row in self
                        .jobs
                        .values_mut()
                        .filter(|row| row.job.group() == group)
                    {
                        d.check()?;
                        row.group_revision = revision;
                    }
                    self.jobs.insert(
                        job.id.clone(),
                        JobState {
                            job: job.as_ref().clone(),
                            title: checked.title,
                            draft: checked.draft,
                            first_delivery_id: headers.delivery_id.clone(),
                            status: "queued",
                            group_revision: revision,
                        },
                    );
                    self.revisions.insert(group, revision);
                }
            }
            Event::Select {
                app_id,
                job_id,
                expected_revision,
                affected,
            } => {
                let job = self
                    .jobs
                    .get(job_id)
                    .ok_or_else(|| anyhow::anyhow!(BINDING))?;
                let group = job.job.group();
                ensure!(
                    job.job.app_id == *app_id
                        && *expected_revision == *self.revisions.get(&group).unwrap_or(&0),
                    BINDING
                );
                let existing: Vec<_> = self
                    .jobs
                    .iter()
                    .filter(|(_, j)| j.job.group() == group)
                    .map(|(id, _)| id.clone())
                    .collect();
                ensure!(*affected == existing, BINDING);
                for id in affected {
                    d.check()?;
                    let row = self
                        .jobs
                        .get_mut(id)
                        .ok_or_else(|| anyhow::anyhow!(BINDING))?;
                    row.status = if id == job_id {
                        "selected"
                    } else {
                        "suppressed"
                    };
                    row.group_revision = expected_revision + 1;
                }
                self.revisions.insert(group, expected_revision + 1);
                self.transitions += 1;
            }
        }
        self.store_bytes = self
            .store_bytes
            .checked_add(raw_len)
            .ok_or_else(|| anyhow::anyhow!(CAPACITY))?;
        ensure!(
            self.store_bytes <= o.max_store_bytes
                && self.deliveries.len() <= o.max_deliveries
                && self.jobs.len() <= o.max_jobs
                && self.transitions <= o.max_transitions,
            CAPACITY
        );
        self.records.push((id, record));
        Ok(())
    }
    fn append(
        &mut self,
        event: Event,
        store: &Store,
        host: &Host,
        o: &Options,
        d: &Deadline,
    ) -> Result<(String, String)> {
        let record = Record {
            schema_version: 1,
            workspace_id: store.workspace_id.clone(),
            sequence: self.records.len() + 1,
            event,
        };
        let raw = crate::json::encode_bounded(&record, MAX_RECORD_BYTES, d)
            .map_err(|_| anyhow::anyhow!(CAPACITY))?;
        let id = identity(&record);
        self.apply(
            id.clone(),
            record,
            raw.len(),
            &Admission {
                workspace: &store.workspace_id,
                host: Some(host),
                options: o,
                deadline: d,
            },
        )?;
        Ok((id, String::from_utf8(raw)?))
    }
}
fn load(store: &Store, host: Option<&Host>, o: &Options, d: &Deadline) -> Result<History> {
    load_observed(store, host, o, d, |_| Ok(()))
}
fn load_observed(
    store: &Store,
    host: Option<&Host>,
    o: &Options,
    d: &Deadline,
    mut observe: impl FnMut(&History) -> Result<()>,
) -> Result<History> {
    d.check()?;
    let (count,bytes,bad):(usize,usize,usize)=store.db.query_row(
        "SELECT count(*),coalesce(sum(length(CAST(value AS BLOB))),0),coalesce(sum(CASE WHEN length(CAST(id AS BLOB))!=64 OR length(CAST(value AS BLOB))>?2 THEN 1 ELSE 0 END),0) FROM records WHERE kind=?1",
        params![RECORD_KIND,MAX_RECORD_BYTES],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?)))?;
    ensure!(
        count <= o.max_deliveries.saturating_add(o.max_transitions)
            && bytes <= o.max_store_bytes
            && bad == 0,
        CAPACITY
    );
    let mut statement = store
        .db
        .prepare("SELECT id,value FROM records WHERE kind=?1 ORDER BY id")?;
    let mut records = Vec::new();
    for row in statement.query_map([RECORD_KIND], |r| {
        Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
    })? {
        d.check()?;
        let (id, raw) = row?;
        ensure!(
            crate::model::valid_id(&id) && raw.len() <= MAX_RECORD_BYTES,
            BINDING
        );
        let record: Record =
            crate::json::decode(crate::json::parse(raw.as_bytes(), MAX_RECORD_BYTES)?)
                .map_err(|_| anyhow::anyhow!(BINDING))?;
        records.push((id, record, raw.len()));
    }
    ensure!(records.len() == count, BINDING);
    records.sort_by_key(|(_, r, _)| r.sequence);
    let mut history = History::default();
    for (id, record, len) in records {
        history.apply(
            id,
            record,
            len,
            &Admission {
                workspace: &store.workspace_id,
                host,
                options: o,
                deadline: d,
            },
        )?;
        observe(&history)?;
    }
    ensure!(history.store_bytes == bytes, BINDING);
    Ok(history)
}
pub(super) type WorkerBindingKey = (String, String, usize);
#[derive(Clone)]
pub(super) struct WorkerJob {
    pub job: Job,
    pub revision: usize,
    pub selected: bool,
}
pub(super) struct WorkerViews {
    pub proofs: BTreeMap<WorkerBindingKey, WorkerJob>,
    pub latest: BTreeMap<String, WorkerJob>,
    pub checkpoint: String,
}
/// Collect only requested historical selection proofs during one queue replay.
pub(super) fn worker_views(
    store: &Store,
    host: Option<&Host>,
    requested: &BTreeMap<String, Vec<(String, usize)>>,
    o: &Options,
    d: &Deadline,
) -> Result<WorkerViews> {
    let mut proofs = BTreeMap::new();
    let history = load_observed(store, host, o, d, |history| {
        let checkpoint = &history
            .records
            .last()
            .ok_or_else(|| anyhow::anyhow!(BINDING))?
            .0;
        if let Some(rows) = requested.get(checkpoint) {
            for (job_id, revision) in rows {
                d.check()?;
                let row = history
                    .jobs
                    .get(job_id)
                    .ok_or_else(|| anyhow::anyhow!(BINDING))?;
                ensure!(
                    row.status == "selected" && row.group_revision == *revision,
                    BINDING
                );
                proofs.insert(
                    (checkpoint.clone(), job_id.clone(), *revision),
                    WorkerJob {
                        job: row.job.clone(),
                        revision: *revision,
                        selected: true,
                    },
                );
            }
        }
        Ok(())
    })?;
    let checkpoint = history
        .records
        .last()
        .map(|r| r.0.clone())
        .unwrap_or_default();
    let mut latest = BTreeMap::new();
    for (id, row) in history.jobs {
        d.check()?;
        let job = WorkerJob {
            job: row.job,
            revision: row.group_revision,
            selected: row.status == "selected",
        };
        if job.selected {
            proofs.insert((checkpoint.clone(), id.clone(), job.revision), job.clone());
        }
        latest.insert(id, job);
    }
    for (checkpoint, rows) in requested {
        for (job, revision) in rows {
            ensure!(
                proofs.contains_key(&(checkpoint.clone(), job.clone(), *revision)),
                BINDING
            );
        }
    }
    Ok(WorkerViews {
        proofs,
        latest,
        checkpoint,
    })
}
fn operation(
    store: &Store,
    host: Option<&Host>,
    o: &Options,
    d: &Deadline,
    write: bool,
    quick_lock: bool,
    run: impl FnOnce(&mut History) -> Result<OperationResult>,
) -> Result<Value> {
    options(o)?;
    d.check()?;
    ensure!(!write || o.allow_writes, "github_app_permission_required");
    ensure!(
        !write || store.db.is_autocommit(),
        "github_app_caller_transaction_refused"
    );
    store.set_deadline(d)?;
    if quick_lock {
        d.check()?;
        store
            .db
            .busy_timeout(d.remaining().min(std::time::Duration::from_millis(50)))?;
    }
    let prior = store.db.limit(Limit::SQLITE_LIMIT_LENGTH)?;
    store.db.set_limit(
        Limit::SQLITE_LIMIT_LENGTH,
        prior.min((MAX_RECORD_BYTES + 65_536) as i32),
    )?;
    let result = (|| -> Result<Value> {
        let tx = if store.db.is_autocommit() {
            Some(Transaction::new_unchecked(
                &store.db,
                if write {
                    TransactionBehavior::Immediate
                } else {
                    TransactionBehavior::Deferred
                },
            )?)
        } else {
            None
        };
        let mut history = load(store, host, o, d)?;
        let (result, pending) = run(&mut history)?;
        admit_output(&result, o, d)?;
        if let Some((id, raw)) = pending {
            ensure!(write && tx.is_some(), BINDING);
            store.db.execute(
                "INSERT INTO records(kind,id,value) VALUES(?1,?2,?3)",
                params![RECORD_KIND, id, raw],
            )?;
        }
        d.check()?;
        if let Some(tx) = tx {
            tx.commit()?;
        }
        Ok(result)
    })();
    store.db.set_limit(Limit::SQLITE_LIMIT_LENGTH, prior)?;
    if result.is_err() {
        d.check()?;
    }
    result
}
fn receipt(headers: &Headers, payload_hash: &str, job_id: &str, reused: bool) -> Value {
    let mut result = boundary();
    result.as_object_mut().expect("boundary is object").extend([
        ("deliveryId".into(), json!(headers.delivery_id)),
        ("payloadHash".into(), json!(payload_hash)),
        ("jobId".into(), json!(job_id)),
        ("reused".into(), json!(reused)),
    ]);
    result
}
pub fn ingest(
    store: &Store,
    host: &Host,
    headers: &Headers,
    raw: &[u8],
    o: &Options,
    d: &Deadline,
) -> Result<Value> {
    ingest_with_lock_policy(store, host, headers, raw, o, d, false)
}
/// The dedicated ingress connection refuses contention promptly; it never retries a delivery.
pub(super) fn ingest_http(
    store: &Store,
    host: &Host,
    headers: &Headers,
    raw: &[u8],
    o: &Options,
    d: &Deadline,
) -> Result<Value> {
    let result = ingest_with_lock_policy(store, host, headers, raw, o, d, true);
    // The transaction has committed or rolled back/dropped before restoring connection policy.
    // Cleanup also runs when deadline admission or another setup step fails.
    store.clear_deadline();
    store.db.busy_timeout(std::time::Duration::from_secs(10))?;
    result
}
fn ingest_with_lock_policy(
    store: &Store,
    host: &Host,
    headers: &Headers,
    raw: &[u8],
    o: &Options,
    d: &Deadline,
    quick_lock: bool,
) -> Result<Value> {
    ensure!(o.allow_writes, "github_app_permission_required");
    options(o)?;
    d.check()?;
    ensure!(raw.len() <= o.max_payload_bytes, CAPACITY);
    headers.validate()?;
    host.verify(raw, &headers.signature256)?;
    d.check()?;
    let checked = checked_payload(raw, host.profile.app_id, &store.workspace_id)?;
    ensure!(
        host.profile.permits(&checked.job),
        "github_app_grant_denied"
    );
    let payload_hash = hash(raw);
    let job = checked.job;
    operation(store, Some(host), o, d, true, quick_lock, |history| {
        if let Some((prior, job_id)) = history
            .deliveries
            .get(&(job.app_id, headers.delivery_id.clone()))
        {
            ensure!(*prior == payload_hash, "github_app_delivery_conflict");
            return Ok((receipt(headers, &payload_hash, job_id, true), None));
        }
        if let Some(job_id) = history.payloads.get(&(job.app_id, payload_hash.clone())) {
            return Ok((receipt(headers, &payload_hash, job_id, true), None));
        }
        let pending = history.append(
            Event::Delivery {
                headers: headers.clone(),
                payload: String::from_utf8(raw.to_vec()).map_err(|_| anyhow::anyhow!(INVALID))?,
                payload_hash: payload_hash.clone(),
                job: Box::new(job.clone()),
            },
            store,
            host,
            o,
            d,
        )?;
        Ok((
            receipt(headers, &payload_hash, &job.id, false),
            Some(pending),
        ))
    })
}
pub fn queue(
    store: &Store,
    host: &Host,
    offset: usize,
    limit: usize,
    o: &Options,
    d: &Deadline,
) -> Result<Value> {
    ensure!(limit <= 100 && offset <= 1000, INVALID);
    operation(store, Some(host), o, d, false, false, |history| {
        let rows: Vec<_> = history
            .jobs
            .values()
            .filter(|j| host.profile.permits(&j.job))
            .collect();
        let jobs: Vec<_> = rows.iter().skip(offset).take(limit).copied().collect();
        let mut value = boundary();
        let object = value.as_object_mut().expect("boundary is object");
        object.insert("jobs".into(), serde_json::to_value(jobs)?);
        object.insert("total".into(), json!(rows.len()));
        object.insert("offset".into(), json!(offset));
        object.insert("limit".into(), json!(limit));
        object.insert(
            "hasMore".into(),
            json!(offset.saturating_add(limit) < rows.len()),
        );
        Ok((value, None))
    })
}
pub fn select(
    store: &Store,
    host: &Host,
    job_id: &str,
    expected_revision: usize,
    o: &Options,
    d: &Deadline,
) -> Result<Value> {
    ensure!(o.allow_writes, "github_app_permission_required");
    ensure!(
        crate::model::valid_id(job_id) && expected_revision <= 2000,
        INVALID
    );
    operation(store, Some(host), o, d, true, false, |history| {
        let selected = history
            .jobs
            .get(job_id)
            .ok_or_else(|| anyhow::anyhow!("github_app_job_unavailable"))?;
        ensure!(
            host.profile.permits(&selected.job),
            "github_app_grant_denied"
        );
        let group = selected.job.group();
        let current = *history.revisions.get(&group).unwrap_or(&0);
        let previous = history
            .records
            .iter()
            .any(|(_, record)| match &record.event {
                Event::Select {
                    job_id: prior,
                    expected_revision: revision,
                    ..
                } => prior == job_id && *revision == expected_revision,
                _ => false,
            });
        if previous {
            let mut value = boundary();
            let object = value.as_object_mut().expect("boundary is object");
            object.insert("jobId".into(), json!(job_id));
            object.insert("groupRevision".into(), json!(expected_revision + 1));
            return Ok((value, None));
        }
        ensure!(
            current == expected_revision,
            "github_app_queue_revision_stale"
        );
        let affected = history
            .jobs
            .iter()
            .filter(|(_, j)| j.job.group() == group)
            .map(|(id, _)| id.clone())
            .collect();
        let pending = history.append(
            Event::Select {
                app_id: host.profile.app_id,
                job_id: job_id.into(),
                expected_revision,
                affected,
            },
            store,
            host,
            o,
            d,
        )?;
        let mut value = boundary();
        let object = value.as_object_mut().expect("boundary is object");
        object.insert("jobId".into(), json!(job_id));
        object.insert("groupRevision".into(), json!(current + 1));
        Ok((value, Some(pending)))
    })
}
/// Read-only retention proof: strict inline schema, identities and complete event
/// graph. No OS secret lookup and no raw payload field has CAS reference authority.
/// Cryptographic authenticity is rechecked separately by an explicitly keyed Host.
pub fn validated_inline_records(store: &Store, d: &Deadline) -> Result<BTreeSet<String>> {
    let o = Options {
        max_payload_bytes: MAX_PAYLOAD_BYTES,
        ..Default::default()
    };
    let mut ids = BTreeSet::new();
    operation(store, None, &o, d, false, false, |history| {
        ids.extend(history.records.iter().map(|(id, _)| id.clone()));
        Ok((json!({}), None))
    })?;
    Ok(ids)
}
