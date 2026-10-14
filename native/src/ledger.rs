//! Durable accounting for explicitly host-selected model attempts.
//! Provider-reported money and usage are not externally verified billing.
use crate::{
    capture::Deadline,
    model::{identity, valid_id},
    paths::obvious_secret,
    store::Store,
};
use anyhow::{Context, Result, ensure};
use rusqlite::{Connection, OptionalExtension, Transaction, TransactionBehavior, params};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use std::collections::BTreeMap;

const ATTEMPT_KIND: &str = "native_model_attempt_v1";
const TASK_KIND: &str = "native_model_task_v1";
const MAX_ATTEMPTS: usize = 20_000;
const MAX_COST: u64 = i64::MAX as u64 / MAX_ATTEMPTS as u64;
const MAX_EVENTS: usize = 256;
const MAX_RECORD_BYTES: usize = 64_000;

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Usage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cached_input_tokens: u64,
}
impl Usage {
    pub(crate) fn validate(&self) -> Result<()> {
        ensure!(
            self.input_tokens <= i64::MAX as u64
                && self.output_tokens <= i64::MAX as u64
                && self.cached_input_tokens <= self.input_tokens,
            "Invalid provider-reported token usage"
        );
        Ok(())
    }
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Totals {
    pub actual_micros: u64,
    pub reserved_micros: u64,
    pub unknown: usize,
    pub encumbered_micros: u64,
    pub attempts: usize,
    pub reservation_exceeded: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub currency: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task_limit_micros: Option<u64>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Task {
    schema: u8,
    task: String,
    currency: String,
    limit_micros: u64,
    totals: Totals,
}
#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum State {
    Reserved,
    Sent,
    Unknown,
    Settled,
    NotSent,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Attempt {
    schema: u8,
    id: String,
    task: String,
    currency: String,
    provider: String,
    state: State,
    reserved_micros: u64,
    actual_micros: Option<u64>,
    usage: Option<Usage>,
    events: BTreeMap<String, Usage>,
    accounting: String,
}
fn label(value: &str, max: usize) -> Result<()> {
    ensure!(
        !value.trim().is_empty()
            && value.len() <= max
            && !value.chars().any(char::is_control)
            && !obvious_secret(value),
        "Invalid accounting label"
    );
    Ok(())
}
pub(crate) fn validate_cost(value: u64) -> Result<()> {
    // Per-attempt and global caps keep all accumulated integer totals
    // representable even when every provider-reported cost overruns.
    ensure!(
        value <= MAX_COST,
        "Accounting amount exceeds integer capacity"
    );
    Ok(())
}
fn read_record<T: DeserializeOwned>(db: &Connection, kind: &str, id: &str) -> Result<Option<T>> {
    let raw: Option<String> = db
        .query_row(
            "SELECT value FROM records WHERE kind=?1 AND id=?2",
            params![kind, id],
            |row| row.get(0),
        )
        .optional()?;
    raw.map(|raw| {
        ensure!(
            raw.len() <= MAX_RECORD_BYTES,
            "Accounting record exceeds capacity"
        );
        Ok(serde_json::from_str(&raw)?)
    })
    .transpose()
}
fn write_record<T: Serialize>(tx: &Transaction<'_>, kind: &str, id: &str, value: &T) -> Result<()> {
    let raw = serde_json::to_string(value)?;
    ensure!(
        raw.len() <= MAX_RECORD_BYTES,
        "Accounting record exceeds capacity"
    );
    tx.execute("INSERT INTO records VALUES(?1,?2,?3) ON CONFLICT(kind,id) DO UPDATE SET value=excluded.value",
        params![kind, id, raw])?;
    Ok(())
}
fn validate_task(task: &Task, expected: &str) -> Result<()> {
    ensure!(
        task.schema == 1 && task.task == expected,
        "Accounting task identity mismatch"
    );
    label(&task.task, 200)?;
    ensure!(
        task.currency.len() == 3 && task.currency.bytes().all(|c| c.is_ascii_uppercase()),
        "Invalid accounting currency"
    );
    validate_cost(task.limit_micros)?;
    ensure!(
        task.totals.attempts <= MAX_ATTEMPTS
            && task.totals.unknown <= task.totals.attempts
            && task
                .totals
                .actual_micros
                .checked_add(task.totals.reserved_micros)
                == Some(task.totals.encumbered_micros),
        "Accounting totals are inconsistent"
    );
    Ok(())
}

/// Request-scoped accounting over the workspace's durable SQLite records.
pub struct Ledger<'a> {
    store: &'a Store,
    deadline: &'a Deadline,
}
impl<'a> Ledger<'a> {
    pub fn new(store: &'a Store, deadline: &'a Deadline) -> Self {
        Self { store, deadline }
    }
    fn transaction(&self) -> Result<Transaction<'_>> {
        self.deadline.check()?;
        self.store.set_deadline(self.deadline)?;
        self.store.db.busy_timeout(self.deadline.remaining())?;
        Ok(Transaction::new_unchecked(
            &self.store.db,
            TransactionBehavior::Immediate,
        )?)
    }
    fn commit(&self, tx: Transaction<'_>) -> Result<()> {
        self.deadline.check()?;
        tx.commit()?;
        Ok(())
    }
    pub fn reserve(
        &self,
        task: &str,
        reservation: u64,
        limit: u64,
        currency: &str,
        provider: &str,
    ) -> Result<String> {
        label(task, 200)?;
        label(provider, 200)?;
        validate_cost(reservation)?;
        validate_cost(limit)?;
        ensure!(
            currency.len() == 3 && currency.bytes().all(|c| c.is_ascii_uppercase()),
            "Currency must be three uppercase ASCII letters"
        );
        let tx = self.transaction()?;
        let task_id = identity(&task);
        let mut policy = if let Some(policy) = read_record::<Task>(&tx, TASK_KIND, &task_id)? {
            validate_task(&policy, task)?;
            ensure!(
                policy.currency == currency && policy.limit_micros == limit,
                "Task currency and cost limit are immutable"
            );
            policy
        } else {
            let tasks: usize = tx.query_row(
                "SELECT count(*) FROM records WHERE kind=?1",
                [TASK_KIND],
                |row| row.get(0),
            )?;
            ensure!(tasks < 1_024, "Accounting task capacity reached");
            Task {
                schema: 1,
                task: task.into(),
                currency: currency.into(),
                limit_micros: limit,
                totals: Totals::default(),
            }
        };
        ensure!(
            !policy.totals.reservation_exceeded,
            "A prior provider cost exceeded its reservation; this task is blocked"
        );
        let encumbered = policy
            .totals
            .encumbered_micros
            .checked_add(reservation)
            .context("Accounting integer overflow")?;
        ensure!(encumbered <= limit, "Task cost budget exhausted");
        let (attempts, bytes): (usize, usize) = tx.query_row(
            "SELECT count(*),coalesce(sum(length(CAST(value AS BLOB))),0) FROM records WHERE kind=?1", [ATTEMPT_KIND],
            |row| Ok((row.get(0)?, row.get(1)?)))?;
        ensure!(
            attempts < MAX_ATTEMPTS && bytes < 256 * 1024 * 1024,
            "Accounting attempt capacity reached"
        );
        let mut nonce = [0u8; 32];
        getrandom::fill(&mut nonce)?;
        let id = hex::encode(nonce);
        let attempt = Attempt {
            schema: 1,
            id: id.clone(),
            task: task.into(),
            currency: currency.into(),
            provider: provider.into(),
            state: State::Reserved,
            reserved_micros: reservation,
            actual_micros: None,
            usage: None,
            events: BTreeMap::new(),
            accounting: "provider_reported_unverified_or_unknown".into(),
        };
        policy.totals.reserved_micros = policy
            .totals
            .reserved_micros
            .checked_add(reservation)
            .context("Accounting integer overflow")?;
        policy.totals.encumbered_micros = encumbered;
        policy.totals.attempts += 1;
        write_record(&tx, ATTEMPT_KIND, &id, &attempt)?;
        write_record(&tx, TASK_KIND, &task_id, &policy)?;
        self.commit(tx)?;
        Ok(id)
    }
    fn change(
        &self,
        id: &str,
        operation: impl FnOnce(&mut Attempt, &mut Task) -> Result<()>,
    ) -> Result<()> {
        ensure!(valid_id(id), "Invalid provider attempt identity");
        let tx = self.transaction()?;
        let mut attempt: Attempt =
            read_record(&tx, ATTEMPT_KIND, id)?.context("Unknown provider attempt")?;
        ensure!(
            attempt.schema == 1 && attempt.id == id && attempt.events.len() <= MAX_EVENTS,
            "Provider attempt record identity mismatch"
        );
        validate_cost(attempt.reserved_micros)?;
        if let Some(actual) = attempt.actual_micros {
            validate_cost(actual)?;
        }
        let task_id = identity(&attempt.task);
        let mut task: Task =
            read_record(&tx, TASK_KIND, &task_id)?.context("Provider task policy is missing")?;
        validate_task(&task, &attempt.task)?;
        ensure!(
            task.currency == attempt.currency,
            "Provider attempt currency mismatch"
        );
        operation(&mut attempt, &mut task)?;
        validate_task(&task, &attempt.task)?;
        write_record(&tx, ATTEMPT_KIND, id, &attempt)?;
        write_record(&tx, TASK_KIND, &task_id, &task)?;
        self.commit(tx)
    }
    pub fn mark_sent(&self, id: &str) -> Result<()> {
        self.change(id, |attempt, task| {
            ensure!(
                matches!(attempt.state, State::Reserved | State::Sent),
                "Cannot send a completed or unknown attempt"
            );
            if attempt.state == State::Reserved {
                task.totals.unknown += 1;
                attempt.state = State::Sent;
            }
            Ok(())
        })
    }
    pub fn usage(&self, id: &str, event: &str, usage: Usage) -> Result<()> {
        label(event, 128)?;
        usage.validate()?;
        self.change(id, |attempt, _| {
            ensure!(
                matches!(attempt.state, State::Sent | State::Unknown | State::Settled),
                "Unsent attempt cannot receive token usage"
            );
            if let Some(prior) = attempt.events.get(event) {
                ensure!(prior == &usage, "Usage event idempotency conflict");
                return Ok(());
            }
            ensure!(
                attempt.events.len() < MAX_EVENTS,
                "Provider usage event capacity reached"
            );
            let old = attempt.usage.clone().unwrap_or_default();
            attempt.usage = Some(Usage {
                input_tokens: old.input_tokens.max(usage.input_tokens),
                output_tokens: old.output_tokens.max(usage.output_tokens),
                cached_input_tokens: old.cached_input_tokens.max(usage.cached_input_tokens),
            });
            attempt.events.insert(event.into(), usage);
            Ok(())
        })
    }
    pub fn settle(&self, id: &str, actual: Option<u64>) -> Result<()> {
        if let Some(actual) = actual {
            validate_cost(actual)?;
        }
        self.change(id, |attempt, task| {
            if attempt.state == State::Settled {
                ensure!(
                    attempt.actual_micros == actual,
                    "Settlement idempotency conflict"
                );
                return Ok(());
            }
            ensure!(
                matches!(attempt.state, State::Sent | State::Unknown),
                "Only a sent attempt can be settled"
            );
            if let Some(actual) = actual {
                task.totals.reserved_micros = task
                    .totals
                    .reserved_micros
                    .checked_sub(attempt.reserved_micros)
                    .context("Accounting reservation underflow")?;
                task.totals.actual_micros = task
                    .totals
                    .actual_micros
                    .checked_add(actual)
                    .context("Accounting integer overflow")?;
                task.totals.encumbered_micros = task
                    .totals
                    .actual_micros
                    .checked_add(task.totals.reserved_micros)
                    .context("Accounting integer overflow")?;
                task.totals.unknown = task
                    .totals
                    .unknown
                    .checked_sub(1)
                    .context("Accounting unknown count underflow")?;
                task.totals.reservation_exceeded |= actual > attempt.reserved_micros;
                attempt.actual_micros = Some(actual);
                attempt.state = State::Settled;
            } else {
                attempt.state = State::Unknown;
            }
            Ok(())
        })
    }
    pub fn cancel_before_send(&self, id: &str) -> Result<()> {
        self.change(id, |attempt, task| {
            ensure!(
                attempt.state == State::Reserved
                    && attempt.usage.is_none()
                    && attempt.events.is_empty(),
                "Only a never-sent attempt can be cancelled"
            );
            task.totals.reserved_micros = task
                .totals
                .reserved_micros
                .checked_sub(attempt.reserved_micros)
                .context("Accounting reservation underflow")?;
            task.totals.encumbered_micros = task
                .totals
                .actual_micros
                .checked_add(task.totals.reserved_micros)
                .context("Accounting integer overflow")?;
            attempt.state = State::NotSent;
            attempt.actual_micros = Some(0);
            Ok(())
        })
    }
    pub fn total(&self, task: &str) -> Result<Totals> {
        label(task, 200)?;
        self.store.set_deadline(self.deadline)?;
        if let Some(policy) = read_record::<Task>(&self.store.db, TASK_KIND, &identity(&task))? {
            validate_task(&policy, task)?;
            self.deadline.check()?;
            let mut totals = policy.totals;
            totals.currency = Some(policy.currency);
            totals.task_limit_micros = Some(policy.limit_micros);
            Ok(totals)
        } else {
            Ok(Totals::default())
        }
    }
}
