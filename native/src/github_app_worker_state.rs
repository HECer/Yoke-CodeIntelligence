//! Authenticated, bounded worker history. SQL transactions never span remote work.
use super::super::{
    Job, WorkClass,
    state::{WorkerViews, worker_views},
};
use super::*;
use crate::model::{identity, valid_id};
use rusqlite::{Transaction, TransactionBehavior, limits::Limit, params};
use std::{collections::BTreeMap, time::Duration};

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
#[derive(Clone, Serialize, Deserialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
enum Event {
    Claim {
        request: ClaimRequest,
        queue_checkpoint: String,
        fence: u64,
        at_unix: u64,
        expires_at_unix: u64,
    },
    Release {
        claim_id: String,
        job_id: String,
        worker: String,
        request_id: String,
        fence: u64,
        at_unix: u64,
    },
    Complete {
        request: RunRequest,
        job_id: String,
        worker: String,
        fence: u64,
        queue_revision: usize,
        queue_checkpoint: String,
        at_unix: u64,
        review: Box<source::Review>,
    },
}
#[derive(Clone)]
pub(super) struct Lease {
    claim_id: String,
    app_id: u64,
    request: ClaimRequest,
    queue_checkpoint: String,
    fence: u64,
    at: u64,
    expires: u64,
}
struct JobState {
    lease: Lease,
    released: bool,
    completion: Option<String>,
}
#[derive(Default)]
struct History {
    records: Vec<(String, Record)>,
    requests: BTreeMap<(u64, String), usize>,
    jobs: BTreeMap<String, JobState>,
    last_at: BTreeMap<u64, u64>,
    bytes: usize,
}
struct Context<'a> {
    store: &'a Store,
    host: Option<&'a Host>,
    o: &'a Options,
    d: &'a Deadline,
}
type Pending = (String, String);
struct Outcome<T> {
    result: T,
    output: Value,
    pending: Option<Pending>,
}
fn unsigned(record: &Record) -> Result<Value> {
    let mut value = serde_json::to_value(record)?;
    value
        .as_object_mut()
        .ok_or_else(|| anyhow::anyhow!(BINDING))?
        .remove("signature256");
    Ok(value)
}
impl History {
    fn apply(
        &mut self,
        id: String,
        record: Record,
        len: usize,
        views: &WorkerViews,
        c: &Context<'_>,
    ) -> Result<()> {
        c.d.check()?;
        ensure!(
            record.schema_version == 1
                && record.workspace_id == c.store.workspace_id
                && super::super::numeric(record.app_id)
                && record.sequence == self.records.len() + 1
                && valid_id(&id)
                && id == identity(&serde_json::to_value(&record)?),
            BINDING
        );
        super::super::signature_bytes(&record.signature256)
            .map_err(|_| anyhow::anyhow!(BINDING))?;
        if let Some(host) = c.host
            && record.app_id == host.profile.app_id
        {
            verify(host, &unsigned(&record)?, &record.signature256, c.d)?;
        }
        let (request_id, at) = match &record.event {
            Event::Claim {
                request,
                queue_checkpoint,
                fence,
                at_unix,
                expires_at_unix,
            } => {
                request.validate()?;
                let proof = views
                    .proofs
                    .get(&(
                        queue_checkpoint.clone(),
                        request.job_id.clone(),
                        request.expected_queue_revision,
                    ))
                    .ok_or_else(|| anyhow::anyhow!(BINDING))?;
                ensure!(
                    valid_id(queue_checkpoint)
                        && proof.selected
                        && proof.job.app_id == record.app_id
                        && proof.job.work_class == WorkClass::Review
                        && *expires_at_unix
                            == at_unix
                                .checked_add(request.lease_seconds)
                                .ok_or_else(|| anyhow::anyhow!(BINDING))?,
                    BINDING
                );
                let prior = self.jobs.get(&request.job_id);
                let next = prior.map_or(Ok(1), |j| {
                    j.lease
                        .fence
                        .checked_add(1)
                        .ok_or_else(|| anyhow::anyhow!(BINDING))
                })?;
                ensure!(
                    *fence == next
                        && prior.is_none_or(|j| j.completion.is_none()
                            && (j.released || *at_unix >= j.lease.expires)),
                    BINDING
                );
                self.jobs.insert(
                    request.job_id.clone(),
                    JobState {
                        lease: Lease {
                            claim_id: id.clone(),
                            app_id: record.app_id,
                            request: request.clone(),
                            queue_checkpoint: queue_checkpoint.clone(),
                            fence: *fence,
                            at: *at_unix,
                            expires: *expires_at_unix,
                        },
                        released: false,
                        completion: None,
                    },
                );
                (&request.request_id, *at_unix)
            }
            Event::Release {
                claim_id,
                job_id,
                worker,
                request_id,
                fence,
                at_unix,
            } => {
                ensure!(
                    valid_id(claim_id) && valid_id(job_id) && worker_id(worker) && uuid(request_id),
                    BINDING
                );
                let row = self
                    .jobs
                    .get_mut(job_id)
                    .ok_or_else(|| anyhow::anyhow!(BINDING))?;
                ensure!(
                    row.lease.app_id == record.app_id
                        && row.lease.claim_id == *claim_id
                        && row.lease.request.worker == *worker
                        && row.lease.fence == *fence
                        && !row.released
                        && *at_unix < row.lease.expires,
                    BINDING
                );
                row.released = true;
                (request_id, *at_unix)
            }
            Event::Complete {
                request,
                job_id,
                worker,
                fence,
                queue_revision,
                queue_checkpoint,
                at_unix,
                review,
            } => {
                request.validate()?;
                let proof = views
                    .proofs
                    .get(&(queue_checkpoint.clone(), job_id.clone(), *queue_revision))
                    .ok_or_else(|| anyhow::anyhow!(BINDING))?;
                ensure!(
                    valid_id(job_id)
                        && worker_id(worker)
                        && valid_id(queue_checkpoint)
                        && proof.selected
                        && proof.job.app_id == record.app_id
                        && proof.job.work_class == WorkClass::Review,
                    BINDING
                );
                let row = self
                    .jobs
                    .get_mut(job_id)
                    .ok_or_else(|| anyhow::anyhow!(BINDING))?;
                ensure!(
                    row.lease.app_id == record.app_id
                        && row.lease.claim_id == request.claim_id
                        && row.lease.request.worker == *worker
                        && row.lease.fence == *fence
                        && row.lease.request.expected_queue_revision == *queue_revision
                        && !row.released
                        && *at_unix < row.lease.expires,
                    BINDING
                );
                review.validate(request, &proof.job, c.d)?;
                row.released = true;
                row.completion = Some(id.clone());
                (&request.request_id, *at_unix)
            }
        };
        ensure!(
            (60..=253_402_296_000).contains(&at)
                && at >= self.last_at.get(&record.app_id).copied().unwrap_or(0),
            BINDING
        );
        ensure!(
            self.requests
                .insert((record.app_id, request_id.clone()), self.records.len())
                .is_none(),
            BINDING
        );
        self.last_at.insert(record.app_id, at);
        self.bytes = self
            .bytes
            .checked_add(len)
            .ok_or_else(|| anyhow::anyhow!(CAPACITY))?;
        ensure!(
            self.bytes <= c.o.max_store_bytes && self.records.len() < c.o.max_records,
            CAPACITY
        );
        self.records.push((id, record));
        Ok(())
    }
    fn append(&mut self, event: Event, views: &WorkerViews, c: &Context<'_>) -> Result<Pending> {
        let host = c.host.ok_or_else(|| anyhow::anyhow!(BINDING))?;
        let mut record = Record {
            schema_version: 1,
            workspace_id: c.store.workspace_id.clone(),
            app_id: host.profile.app_id,
            sequence: self.records.len() + 1,
            event,
            signature256: String::new(),
        };
        record.signature256 = sign(host, &unsigned(&record)?, c.d)?;
        // The closed typed schema is serialized through Value for one canonical field order.
        let value = serde_json::to_value(&record)?;
        let raw = crate::json::encode_bounded(&value, MAX_RECORD_BYTES, c.d)
            .map_err(|_| anyhow::anyhow!(CAPACITY))?;
        let id = identity(&value);
        self.apply(id.clone(), record, raw.len(), views, c)?;
        Ok((id, String::from_utf8(raw)?))
    }
    fn prior_request(&self, app_id: u64, request_id: &str) -> Option<&(String, Record)> {
        self.requests
            .get(&(app_id, request_id.to_owned()))
            .map(|i| &self.records[*i])
    }
}
fn load(c: &Context<'_>) -> Result<(History, WorkerViews)> {
    c.d.check()?;
    let (count, bytes, bad): (usize, usize, usize) = c.store.db.query_row(
        "SELECT count(*),coalesce(sum(length(CAST(value AS BLOB))),0),coalesce(sum(CASE WHEN length(CAST(id AS BLOB))!=64 OR length(CAST(value AS BLOB))>?2 THEN 1 ELSE 0 END),0) FROM records WHERE kind=?1",
        params![RECORD_KIND,MAX_RECORD_BYTES], |r|Ok((r.get(0)?,r.get(1)?,r.get(2)?)))?;
    ensure!(
        count <= c.o.max_records && bytes <= c.o.max_store_bytes && bad == 0,
        CAPACITY
    );
    let mut rows = Vec::new();
    let mut requested: BTreeMap<String, Vec<(String, usize)>> = BTreeMap::new();
    {
        let mut statement = c
            .store
            .db
            .prepare("SELECT id,value FROM records WHERE kind=?1 ORDER BY id")?;
        for row in statement.query_map([RECORD_KIND], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
        })? {
            c.d.check()?;
            let (id, raw) = row?;
            let record: Record =
                crate::json::decode(crate::json::parse(raw.as_bytes(), MAX_RECORD_BYTES)?)
                    .map_err(|_| anyhow::anyhow!(BINDING))?;
            if let Event::Claim {
                request,
                queue_checkpoint,
                ..
            } = &record.event
            {
                requested
                    .entry(queue_checkpoint.clone())
                    .or_default()
                    .push((request.job_id.clone(), request.expected_queue_revision));
            }
            if let Event::Complete {
                job_id,
                queue_revision,
                queue_checkpoint,
                ..
            } = &record.event
            {
                requested
                    .entry(queue_checkpoint.clone())
                    .or_default()
                    .push((job_id.clone(), *queue_revision));
            }
            rows.push((id, record, raw.len()));
        }
    }
    ensure!(rows.len() == count, BINDING);
    let views = worker_views(c.store, c.host, &requested, &c.o.queue, c.d)?;
    rows.sort_by_key(|(_, r, _)| r.sequence);
    let mut history = History::default();
    for (id, record, len) in rows {
        history.apply(id, record, len, &views, c)?;
    }
    ensure!(history.bytes == bytes, BINDING);
    Ok((history, views))
}
fn operation<T>(
    c: &Context<'_>,
    clock: Option<&Clock>,
    write: bool,
    run: impl FnOnce(&mut History, &WorkerViews, u64) -> Result<Outcome<T>>,
) -> Result<T> {
    operation_with_limit(c, clock, write, MAX_RECORD_BYTES + 65_536, || Ok(()), run)
}
fn operation_with_limit<T>(
    c: &Context<'_>,
    clock: Option<&Clock>,
    write: bool,
    row_limit: usize,
    mut final_check: impl FnMut() -> Result<()>,
    run: impl FnOnce(&mut History, &WorkerViews, u64) -> Result<Outcome<T>>,
) -> Result<T> {
    if let Some(host) = c.host {
        host.profile.validate()?;
    }
    c.d.check()?;
    ensure!(
        !write || c.store.db.is_autocommit(),
        "github_app_worker_caller_transaction_refused"
    );
    let prior_busy: u64 = c
        .store
        .db
        .query_row("PRAGMA busy_timeout", [], |r| r.get(0))?;
    let prior_length = c.store.db.limit(Limit::SQLITE_LIMIT_LENGTH)?;
    let prior_deadline = c.store.replace_deadline(Some(c.d.clone()));
    let result = (|| -> Result<T> {
        c.store.check_deadline()?;
        c.store
            .db
            .busy_timeout(c.d.remaining().min(Duration::from_millis(50)))?;
        c.store.db.set_limit(
            Limit::SQLITE_LIMIT_LENGTH,
            prior_length.min(row_limit as i32),
        )?;
        let tx = if c.store.db.is_autocommit() {
            Some(Transaction::new_unchecked(
                &c.store.db,
                if write {
                    TransactionBehavior::Immediate
                } else {
                    TransactionBehavior::Deferred
                },
            )?)
        } else {
            None
        };
        let (mut history, views) = load(c)?;
        let now = clock.map(Clock::now).transpose()?.unwrap_or(0);
        if let Some(host) = c.host
            && clock.is_some()
        {
            ensure!(
                now >= history
                    .last_at
                    .get(&host.profile.app_id)
                    .copied()
                    .unwrap_or(0),
                "github_app_worker_clock_regressed"
            );
        }
        let outcome = run(&mut history, &views, now)?;
        crate::json::encode_bounded(&outcome.output, c.o.max_output_bytes - 1, c.d)
            .map_err(|_| anyhow::anyhow!(CAPACITY))?;
        let writing = outcome.pending.is_some();
        if let Some((id, raw)) = outcome.pending {
            ensure!(write && tx.is_some(), BINDING);
            final_check()?;
            c.store.db.execute(
                "INSERT INTO records(kind,id,value) VALUES(?1,?2,?3)",
                params![RECORD_KIND, id, raw],
            )?;
        }
        c.d.check()?;
        if writing {
            final_check()?;
        }
        if let Some(tx) = tx {
            tx.commit()?;
        }
        Ok(outcome.result)
    })();
    // All statements and owned transactions have left scope. Always attempt every restore.
    let length_restore = c
        .store
        .db
        .set_limit(Limit::SQLITE_LIMIT_LENGTH, prior_length);
    let _ = c.store.replace_deadline(prior_deadline);
    let busy_restore = c.store.db.busy_timeout(Duration::from_millis(prior_busy));
    ensure!(
        length_restore.is_ok() && busy_restore.is_ok(),
        "github_app_worker_sql_policy_restore_failed"
    );
    if result.is_err() {
        c.d.check()?;
    }
    result
}
fn granted<'a>(views: &'a WorkerViews, host: &Host, job_id: &str) -> Result<&'a Job> {
    let row = views
        .latest
        .get(job_id)
        .ok_or_else(|| anyhow::anyhow!("github_app_worker_job_unavailable"))?;
    ensure!(
        host.profile.permits(&row.job),
        "github_app_worker_grant_denied"
    );
    Ok(&row.job)
}
fn current(views: &WorkerViews, lease: &Lease, host: &Host) -> Result<()> {
    let job = granted(views, host, &lease.request.job_id)?;
    let row = views
        .latest
        .get(&job.id)
        .ok_or_else(|| anyhow::anyhow!(BINDING))?;
    ensure!(
        row.selected
            && row.revision == lease.request.expected_queue_revision
            && job.work_class == WorkClass::Review,
        "github_app_worker_queue_stale"
    );
    Ok(())
}
fn lease_from(id: &str, record: &Record) -> Result<Lease> {
    let Event::Claim {
        request,
        queue_checkpoint,
        fence,
        at_unix,
        expires_at_unix,
    } = &record.event
    else {
        anyhow::bail!("github_app_worker_request_conflict");
    };
    Ok(Lease {
        claim_id: id.into(),
        app_id: record.app_id,
        request: request.clone(),
        queue_checkpoint: queue_checkpoint.clone(),
        fence: *fence,
        at: *at_unix,
        expires: *expires_at_unix,
    })
}
fn claim_value(lease: &Lease, reused: bool) -> Value {
    let mut value = boundary();
    value.as_object_mut().expect("boundary object").extend([
        ("jobId".into(), json!(lease.request.job_id)),
        ("claimId".into(), json!(lease.claim_id)),
        ("worker".into(), json!(lease.request.worker)),
        ("fence".into(), json!(lease.fence)),
        ("claimedAtUnix".into(), json!(lease.at)),
        ("expiresAtUnix".into(), json!(lease.expires)),
        (
            "queueRevision".into(),
            json!(lease.request.expected_queue_revision),
        ),
        ("queueCheckpoint".into(), json!(lease.queue_checkpoint)),
        ("reused".into(), json!(reused)),
        ("historical".into(), json!(reused)),
        ("executionAuthority".into(), json!(false)),
    ]);
    value
}
pub(super) fn claim(
    store: &Store,
    host: &Host,
    request: &ClaimRequest,
    o: &Options,
    clock: &Clock,
    d: &Deadline,
) -> Result<Claim> {
    let c = Context {
        store,
        host: Some(host),
        o,
        d,
    };
    operation(&c, Some(clock), true, |history, views, now| {
        granted(views, host, &request.job_id)?;
        if let Some((id, record)) = history.prior_request(host.profile.app_id, &request.request_id)
        {
            let lease = lease_from(id, record)?;
            ensure!(
                lease.request == *request,
                "github_app_worker_request_conflict"
            );
            let evidence = claim_value(&lease, true);
            return Ok(Outcome {
                output: evidence.clone(),
                result: Claim { evidence, lease },
                pending: None,
            });
        }
        let prior = history.jobs.get(&request.job_id);
        ensure!(
            prior.is_none_or(|j| j.completion.is_none()),
            "github_app_worker_job_completed"
        );
        ensure!(
            prior.is_none_or(|j| j.released || now >= j.lease.expires),
            "github_app_worker_already_claimed"
        );
        let fence = prior.map_or(Ok(1), |j| {
            j.lease
                .fence
                .checked_add(1)
                .ok_or_else(|| anyhow::anyhow!(CAPACITY))
        })?;
        let lease = Lease {
            claim_id: String::new(),
            app_id: host.profile.app_id,
            request: request.clone(),
            queue_checkpoint: views.checkpoint.clone(),
            fence,
            at: now,
            expires: now
                .checked_add(request.lease_seconds)
                .ok_or_else(|| anyhow::anyhow!(INVALID))?,
        };
        current(views, &lease, host)?;
        let pending = history.append(
            Event::Claim {
                request: request.clone(),
                queue_checkpoint: lease.queue_checkpoint.clone(),
                fence,
                at_unix: now,
                expires_at_unix: lease.expires,
            },
            views,
            &c,
        )?;
        let lease = Lease {
            claim_id: pending.0.clone(),
            ..lease
        };
        let evidence = claim_value(&lease, false);
        Ok(Outcome {
            output: evidence.clone(),
            result: Claim { evidence, lease },
            pending: Some(pending),
        })
    })
}
fn release_value(id: &str, lease: &Lease, reused: bool) -> Value {
    let mut value = boundary();
    value.as_object_mut().expect("boundary object").extend([
        ("status".into(), json!("released")),
        ("releaseId".into(), json!(id)),
        ("jobId".into(), json!(lease.request.job_id)),
        ("claimId".into(), json!(lease.claim_id)),
        ("fence".into(), json!(lease.fence)),
        ("reused".into(), json!(reused)),
        ("historical".into(), json!(reused)),
    ]);
    value
}
pub(super) fn recover(
    store: &Store,
    host: &Host,
    claim_id: &str,
    o: &Options,
    clock: &Clock,
    d: &Deadline,
) -> Result<Claim> {
    let c = Context {
        store,
        host: Some(host),
        o,
        d,
    };
    operation(&c, Some(clock), false, |history, views, _| {
        let (id, record) = history
            .records
            .iter()
            .find(|(id, _)| id == claim_id)
            .ok_or_else(|| anyhow::anyhow!("github_app_worker_claim_unavailable"))?;
        let lease = lease_from(id, record)?;
        granted(views, host, &lease.request.job_id)?;
        let evidence = claim_value(&lease, true);
        Ok(Outcome {
            output: evidence.clone(),
            result: Claim { evidence, lease },
            pending: None,
        })
    })
}
pub(super) fn release(
    store: &Store,
    host: &Host,
    claim: &Claim,
    request_id: &str,
    o: &Options,
    clock: &Clock,
    d: &Deadline,
) -> Result<Value> {
    let c = Context {
        store,
        host: Some(host),
        o,
        d,
    };
    operation(&c, Some(clock), true, |history, views, now| {
        let lease = &claim.lease;
        granted(views, host, &lease.request.job_id)?;
        ensure!(
            lease.app_id == host.profile.app_id,
            "github_app_worker_grant_denied"
        );
        if let Some((id, record)) = history.prior_request(host.profile.app_id, request_id) {
            ensure!(
                matches!(&record.event, Event::Release {claim_id, job_id, worker, fence, ..}
                if *claim_id == lease.claim_id && *job_id == lease.request.job_id && *worker == lease.request.worker && *fence == lease.fence),
                "github_app_worker_request_conflict"
            );
            let value = release_value(id, lease, true);
            return Ok(Outcome {
                output: value.clone(),
                result: value,
                pending: None,
            });
        }
        let row = history
            .jobs
            .get(&lease.request.job_id)
            .ok_or_else(|| anyhow::anyhow!(BINDING))?;
        ensure!(
            row.lease.claim_id == lease.claim_id
                && row.lease.fence == lease.fence
                && !row.released
                && now < row.lease.expires,
            "github_app_worker_lease_stale"
        );
        let pending = history.append(
            Event::Release {
                claim_id: lease.claim_id.clone(),
                job_id: lease.request.job_id.clone(),
                worker: lease.request.worker.clone(),
                request_id: request_id.into(),
                fence: lease.fence,
                at_unix: now,
            },
            views,
            &c,
        )?;
        let value = release_value(&pending.0, lease, false);
        Ok(Outcome {
            output: value.clone(),
            result: value,
            pending: Some(pending),
        })
    })
}
pub(super) fn status(
    store: &Store,
    host: &Host,
    request: &StatusRequest,
    o: &Options,
    clock: &Clock,
    d: &Deadline,
) -> Result<Value> {
    let c = Context {
        store,
        host: Some(host),
        o,
        d,
    };
    operation(&c, Some(clock), false, |history, views, now| {
        let mut jobs = Vec::new();
        for (job_id, row) in &history.jobs {
            d.check()?;
            let job = views
                .latest
                .get(job_id)
                .ok_or_else(|| anyhow::anyhow!(BINDING))?;
            if !host.profile.permits(&job.job) {
                continue;
            }
            let lease = &row.lease;
            let mut value = claim_value(lease, false);
            value["status"] = json!(if row.completion.is_some() {
                "completed"
            } else if row.released {
                "released"
            } else if now >= lease.expires {
                "expired"
            } else {
                "claimed"
            });
            value["queueStale"] =
                json!(!job.selected || job.revision != lease.request.expected_queue_revision);
            value["completionId"] = json!(row.completion);
            jobs.push(value);
        }
        let total = jobs.len();
        let rows: Vec<_> = jobs
            .into_iter()
            .skip(request.offset)
            .take(request.limit)
            .collect();
        let mut value = boundary();
        value.as_object_mut().expect("boundary object").extend([
            ("jobs".into(), json!(rows)),
            ("total".into(), json!(total)),
            ("offset".into(), json!(request.offset)),
            ("limit".into(), json!(request.limit)),
            (
                "hasMore".into(),
                json!(request.offset.saturating_add(request.limit) < total),
            ),
        ]);
        Ok(Outcome {
            output: value.clone(),
            result: value,
            pending: None,
        })
    })
}

pub(super) enum Start {
    Historical(Value),
    Active { claim: Box<Claim>, job: Job },
}
fn active(
    history: &History,
    views: &WorkerViews,
    host: &Host,
    lease: &Lease,
    now: u64,
) -> Result<()> {
    current(views, lease, host)?;
    let row = history
        .jobs
        .get(&lease.request.job_id)
        .ok_or_else(|| anyhow::anyhow!(BINDING))?;
    ensure!(
        row.lease.app_id == host.profile.app_id
            && row.lease.claim_id == lease.claim_id
            && row.lease.fence == lease.fence
            && !row.released
            && now < row.lease.expires,
        "github_app_worker_lease_stale"
    );
    Ok(())
}
fn completion_value(id: &str, record: &Record, historical: bool) -> Result<Value> {
    let Event::Complete {
        request,
        job_id,
        worker,
        fence,
        queue_revision,
        queue_checkpoint,
        at_unix,
        review,
    } = &record.event
    else {
        anyhow::bail!("github_app_worker_request_conflict")
    };
    let mut value = boundary();
    value.as_object_mut().expect("boundary object").extend([
        ("status".into(), json!("completed")),
        ("completionId".into(), json!(id)),
        ("claimId".into(), json!(request.claim_id)),
        ("jobId".into(), json!(job_id)),
        ("worker".into(), json!(worker)),
        ("fence".into(), json!(fence)),
        ("queueRevision".into(), json!(queue_revision)),
        ("queueCheckpoint".into(), json!(queue_checkpoint)),
        ("completedAtUnix".into(), json!(at_unix)),
        ("historical".into(), json!(historical)),
        ("executionAuthority".into(), json!(false)),
        ("workerExecutionVerified".into(), json!(true)),
        ("installationTokenVerified".into(), json!(!historical)),
        ("currentHeadVerified".into(), json!(!historical)),
        (
            "networkRequests".into(),
            json!(if historical {
                0
            } else {
                4 + review.provider_reads.requests
            }),
        ),
        ("review".into(), serde_json::to_value(review)?),
    ]);
    Ok(value)
}
fn historical_run(
    history: &History,
    views: &WorkerViews,
    c: &Context<'_>,
    request: &RunRequest,
) -> Result<Option<Value>> {
    let host = c.host.ok_or_else(|| anyhow::anyhow!(BINDING))?;
    let Some((id, record)) = history.prior_request(host.profile.app_id, &request.request_id) else {
        return Ok(None);
    };
    let Event::Complete {
        request: prior,
        job_id,
        review,
        ..
    } = &record.event
    else {
        anyhow::bail!("github_app_worker_request_conflict")
    };
    ensure!(prior == request, "github_app_worker_request_conflict");
    granted(views, host, job_id)?;
    review.verify_source(c.store, true, c.d)?;
    Ok(Some(completion_value(id, record, true)?))
}
pub(super) fn begin_run(
    context: &RunContext<'_>,
    request: &RunRequest,
    d: &Deadline,
) -> Result<Start> {
    let c = Context {
        store: context.store,
        host: Some(context.host),
        o: context.options,
        d,
    };
    operation_with_limit(
        &c,
        Some(context.clock),
        false,
        source::MAX_ROW + 65_536,
        || Ok(()),
        |history, views, now| {
            if let Some(value) = historical_run(history, views, &c, request)? {
                return Ok(Outcome {
                    output: value.clone(),
                    result: Start::Historical(value),
                    pending: None,
                });
            }
            let (id, record) = history
                .records
                .iter()
                .find(|(id, _)| *id == request.claim_id)
                .ok_or_else(|| anyhow::anyhow!("github_app_worker_claim_unavailable"))?;
            let lease = lease_from(id, record)?;
            active(history, views, context.host, &lease, now)?;
            let job = granted(views, context.host, &lease.request.job_id)?.clone();
            let evidence = claim_value(&lease, false);
            Ok(Outcome {
                output: json!({}),
                result: Start::Active {
                    claim: Box::new(Claim { evidence, lease }),
                    job,
                },
                pending: None,
            })
        },
    )
}
pub(super) fn complete(
    context: &RunContext<'_>,
    request: &RunRequest,
    claim: &Claim,
    review: source::Review,
    token_check: impl Fn() -> Result<()>,
    report_limit: usize,
    d: &Deadline,
) -> Result<Value> {
    let c = Context {
        store: context.store,
        host: Some(context.host),
        o: context.options,
        d,
    };
    operation_with_limit(
        &c,
        Some(context.clock),
        true,
        source::MAX_ROW + 65_536,
        || {
            token_check()?;
            ensure!(
                context.clock.now()? < claim.lease.expires,
                "github_app_worker_lease_stale"
            );
            Ok(())
        },
        |history, views, now| {
            if let Some(mut value) = historical_run(history, views, &c, request)? {
                crate::json::encode_bounded(&value["review"], report_limit, d)
                    .map_err(|_| anyhow::anyhow!(CAPACITY))?;
                // This attempt already performed authorization and provider reads.
                // The selected durable report remains historical and idempotent.
                value["networkRequests"] = json!(4 + review.provider_reads.requests);
                return Ok(Outcome {
                    output: value.clone(),
                    result: value,
                    pending: None,
                });
            }
            let lease = &claim.lease;
            ensure!(lease.claim_id == request.claim_id, BINDING);
            active(history, views, context.host, lease, now)?;
            let job = granted(views, context.host, &lease.request.job_id)?;
            review.validate(request, job, d)?;
            review.verify_source(c.store, true, d)?;
            let at = context.clock.now()?;
            ensure!(
                at >= now && at < lease.expires,
                "github_app_worker_lease_stale"
            );
            token_check()?;
            let pending = history.append(
                Event::Complete {
                    request: request.clone(),
                    job_id: lease.request.job_id.clone(),
                    worker: lease.request.worker.clone(),
                    fence: lease.fence,
                    queue_revision: lease.request.expected_queue_revision,
                    queue_checkpoint: views.checkpoint.clone(),
                    at_unix: at,
                    review: Box::new(review),
                },
                views,
                &c,
            )?;
            let (_, record) = history
                .records
                .last()
                .ok_or_else(|| anyhow::anyhow!(BINDING))?;
            let value = completion_value(&pending.0, record, false)?;
            Ok(Outcome {
                output: value.clone(),
                result: value,
                pending: Some(pending),
            })
        },
    )
}

pub(super) fn publication_source(
    store: &Store,
    host: &Host,
    completion_id: &str,
    revision: Option<usize>,
    recovery_installation: Option<u64>,
    options: &Options,
    d: &Deadline,
) -> Result<PublicationSource> {
    let c = Context {
        store,
        host: Some(host),
        o: options,
        d,
    };
    operation_with_limit(
        &c,
        None,
        false,
        source::MAX_ROW + 65_536,
        || Ok(()),
        |history, views, _| {
            let (_, record) = history
                .records
                .iter()
                .find(|(id, record)| id == completion_id && record.app_id == host.profile.app_id)
                .ok_or_else(|| anyhow::anyhow!("github_app_publication_completion_unavailable"))?;
            let Event::Complete {
                job_id,
                review,
                queue_revision,
                queue_checkpoint,
                at_unix,
                ..
            } = &record.event
            else {
                anyhow::bail!("github_app_publication_completion_unavailable")
            };
            let current = views
                .latest
                .get(job_id)
                .ok_or_else(|| anyhow::anyhow!(BINDING))?;
            let job = &current.job;
            if let Some(installation) = recovery_installation {
                ensure!(
                    revision.is_none()
                        && host.profile.app_id == job.app_id
                        && host
                            .profile
                            .grants
                            .iter()
                            .any(|g| g.installation_id == installation
                                && g.repository_id == job.repository_id
                                && g.repository == job.repository),
                    "github_app_worker_grant_denied"
                );
            } else {
                granted(views, host, job_id)?;
            }
            if let Some(revision) = revision {
                ensure!(
                    current.selected && current.revision == revision,
                    "github_app_publication_queue_stale"
                );
            }
            review.verify_source(store, true, d)?;
            Ok(Outcome {
                output: json!({}),
                pending: None,
                result: PublicationSource {
                    job: job.clone(),
                    completion_id: completion_id.into(),
                    completed_at_unix: *at_unix,
                    queue_revision: revision.unwrap_or(*queue_revision),
                    queue_checkpoint: if revision.is_some() {
                        views.checkpoint.clone()
                    } else {
                        queue_checkpoint.clone()
                    },
                    review: *review.clone(),
                },
            })
        },
    )
}

fn bindings(
    history: &History,
    views: &WorkerViews,
    d: &Deadline,
) -> Result<BTreeMap<String, PublicationBinding>> {
    let mut result = BTreeMap::new();
    for (id, record) in &history.records {
        d.check()?;
        if let Event::Complete {
            job_id,
            review,
            at_unix,
            ..
        } = &record.event
        {
            let job = views
                .latest
                .get(job_id)
                .ok_or_else(|| anyhow::anyhow!(BINDING))?
                .job
                .clone();
            result.insert(
                id.clone(),
                PublicationBinding {
                    job,
                    completed_at_unix: *at_unix,
                    review_hash: identity(&serde_json::to_value(review)?),
                    head_snapshot_id: review.head_snapshot_id.clone(),
                    base_snapshot_id: review.base_snapshot_id.clone(),
                    facts: review.publication_facts(),
                    page_graph_hash: {
                        let (head, base) = review.page_impacts();
                        super::super::pages::retained_graph_hash(head, base, d)?
                    },
                },
            );
        }
    }
    Ok(result)
}
pub(super) fn publication_bindings(
    store: &Store,
    host: Option<&Host>,
    options: &Options,
    d: &Deadline,
) -> Result<BTreeMap<String, PublicationBinding>> {
    let c = Context {
        store,
        host,
        o: options,
        d,
    };
    operation_with_limit(
        &c,
        None,
        false,
        source::MAX_ROW + 65_536,
        || Ok(()),
        |history, views, _| {
            Ok(Outcome {
                result: bindings(history, views, d)?,
                output: json!({}),
                pending: None,
            })
        },
    )
}
pub(super) fn retained_roots(store: &Store, d: &Deadline) -> Result<RetainedRoots> {
    let o = Options {
        queue: super::super::Options {
            max_payload_bytes: super::super::MAX_PAYLOAD_BYTES,
            ..Default::default()
        },
        ..Default::default()
    };
    o.admit(false, d)?;
    let c = Context {
        store,
        host: None,
        o: &o,
        d,
    };
    operation_with_limit(
        &c,
        None,
        false,
        source::MAX_ROW + 65_536,
        || Ok(()),
        |history, views, _| {
            let ids = history.records.iter().map(|(id, _)| id.clone()).collect();
            let mut snapshots = std::collections::BTreeSet::new();
            let mut retained = source::Retained::default();
            for (_, record) in &history.records {
                if let Event::Complete { review, .. } = &record.event {
                    retained.check(store, review, d)?;
                    snapshots.extend(review.roots().cloned());
                }
            }
            Ok(Outcome {
                result: RetainedRoots {
                    record_ids: ids,
                    snapshot_ids: snapshots,
                    publications: bindings(history, views, d)?,
                },
                output: json!({}),
                pending: None,
            })
        },
    )
}
