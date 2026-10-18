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
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum State {
    Reserved,
    Sent,
    Unknown,
    Settled,
    NotSent,
}
pub type AttemptState = State;

/// A single-use host reservation, bound to its durable workspace and policy.
/// JSON and callers cannot manufacture or clone a send capability.
pub struct ReservedAttempt {
    id: String,
    workspace_id: String,
    task: String,
    provider: String,
    currency: String,
    amount: u64,
}
impl ReservedAttempt {
    pub fn id(&self) -> &str {
        &self.id
    }
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AttemptStatus {
    pub attempt_id: String,
    pub task_id: String,
    pub provider: String,
    pub currency: String,
    pub state: AttemptState,
    pub reserved_micros: u64,
    pub actual_cost_micros: Option<u64>,
    pub reported_usage: Option<Usage>,
    /// Immutable response event, distinct from later aggregate reconciliation.
    pub response_usage: Option<Usage>,
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
/// Validate a host budget before opening persistent accounting state.
pub fn validate_cost(value: u64) -> Result<()> {
    // Per-attempt and global caps keep all accumulated integer totals
    // representable even when every provider-reported cost overruns.
    ensure!(
        value <= MAX_COST,
        "Accounting amount exceeds integer capacity"
    );
    Ok(())
}
fn read_record<T: DeserializeOwned>(db: &Connection, kind: &str, id: &str) -> Result<Option<T>> {
    let limit = rusqlite::limits::Limit::SQLITE_LIMIT_LENGTH;
    let prior = db.limit(limit)?;
    db.set_limit(limit, prior.min((MAX_RECORD_BYTES + 4096) as i32))?;
    let result = (|| -> Result<Option<T>> {
        let size: Option<usize> = db
            .query_row(
                "SELECT length(CAST(value AS BLOB)) FROM records WHERE kind=?1 AND id=?2",
                params![kind, id],
                |row| row.get(0),
            )
            .optional()?;
        let Some(size) = size else { return Ok(None) };
        ensure!(
            size <= MAX_RECORD_BYTES,
            "Accounting record exceeds capacity"
        );
        let raw: String = db.query_row(
            "SELECT value FROM records WHERE kind=?1 AND id=?2 AND length(CAST(value AS BLOB))<=?3",
            params![kind, id, MAX_RECORD_BYTES],
            |row| row.get(0),
        )?;
        ensure!(
            raw.len() == size && raw.len() <= MAX_RECORD_BYTES,
            "Accounting record changed during read"
        );
        Ok(Some(serde_json::from_str(&raw)?))
    })();
    db.set_limit(limit, prior)?;
    result
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
fn validate_attempt(attempt: &Attempt, id: &str) -> Result<()> {
    ensure!(
        attempt.schema == 1
            && attempt.id == id
            && valid_id(id)
            && attempt.events.len() <= MAX_EVENTS
            && attempt.accounting == "provider_reported_unverified_or_unknown",
        "Provider attempt record identity mismatch"
    );
    label(&attempt.task, 200)?;
    label(&attempt.provider, 200)?;
    ensure!(
        attempt.currency.len() == 3 && attempt.currency.bytes().all(|b| b.is_ascii_uppercase()),
        "Invalid attempt currency"
    );
    validate_cost(attempt.reserved_micros)?;
    if let Some(cost) = attempt.actual_micros {
        validate_cost(cost)?;
    }
    let mut usage = Usage::default();
    for (event, reported) in &attempt.events {
        label(event, 128)?;
        reported.validate()?;
        usage.input_tokens = usage.input_tokens.max(reported.input_tokens);
        usage.output_tokens = usage.output_tokens.max(reported.output_tokens);
        usage.cached_input_tokens = usage.cached_input_tokens.max(reported.cached_input_tokens);
    }
    ensure!(
        attempt.usage == (!attempt.events.is_empty()).then_some(usage),
        "Provider usage record differs from its events"
    );
    match attempt.state {
        State::Reserved => ensure!(
            attempt.actual_micros.is_none() && attempt.usage.is_none(),
            "Reserved attempt has billing evidence"
        ),
        State::NotSent => ensure!(
            attempt.actual_micros == Some(0) && attempt.usage.is_none(),
            "Unsent attempt has billing evidence"
        ),
        State::Sent | State::Unknown => ensure!(
            attempt.actual_micros.is_none(),
            "Unsettled attempt has known cost"
        ),
        State::Settled => ensure!(
            attempt.actual_micros.is_some(),
            "Settled attempt has unknown cost"
        ),
    }
    Ok(())
}

/// Validate frozen accounting rows without opening a ledger or granting a send capability.
/// This checks the supplied task row, not current global totals or external billing.
pub fn checked_archived_attempt(
    attempt_id: &str,
    attempt: &serde_json::Value,
    task_record_id: &str,
    task: &serde_json::Value,
    deadline: &crate::capture::Deadline,
) -> Result<AttemptStatus> {
    deadline.check()?;
    ensure!(
        valid_id(attempt_id) && valid_id(task_record_id),
        "Invalid archived accounting record identity"
    );
    // Bound untrusted Values before cloning or decoding their typed records.
    crate::json::encode_bounded(attempt, MAX_RECORD_BYTES, deadline)?;
    crate::json::encode_bounded(task, MAX_RECORD_BYTES, deadline)?;
    let attempt: Attempt = crate::json::decode(attempt.clone())?;
    let task: Task = crate::json::decode(task.clone())?;
    validate_attempt(&attempt, attempt_id)?;
    validate_task(&task, &attempt.task)?;
    ensure!(
        task_record_id == identity(&task.task) && task.currency == attempt.currency,
        "Archived provider task key or currency mismatch"
    );
    deadline.check()?;
    Ok(AttemptStatus {
        attempt_id: attempt.id,
        task_id: attempt.task,
        provider: attempt.provider,
        currency: attempt.currency,
        state: attempt.state,
        reserved_micros: attempt.reserved_micros,
        actual_cost_micros: attempt.actual_micros,
        reported_usage: attempt.usage,
        response_usage: attempt.events.get("response").cloned(),
    })
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
    pub fn reserve_many(
        &self,
        task: &str,
        amounts: &[u64],
        limit: u64,
        currency: &str,
        provider: &str,
    ) -> Result<Vec<ReservedAttempt>> {
        self.reserve_many_registered(task, amounts, limit, currency, provider, |_, _| Ok(()))
    }
    pub(crate) fn reserve_many_registered(
        &self,
        task: &str,
        amounts: &[u64],
        limit: u64,
        currency: &str,
        provider: &str,
        register: impl FnOnce(&Transaction<'_>, &[ReservedAttempt]) -> Result<()>,
    ) -> Result<Vec<ReservedAttempt>> {
        self.deadline.check()?;
        label(task, 200)?;
        label(provider, 200)?;
        validate_cost(limit)?;
        ensure!(
            !amounts.is_empty() && amounts.len() <= 256,
            "Batch requires 1..256 reserved attempts"
        );
        ensure!(
            currency.len() == 3 && currency.bytes().all(|b| b.is_ascii_uppercase()),
            "Currency must be three uppercase ASCII letters"
        );
        let mut sum = 0u64;
        for amount in amounts {
            validate_cost(*amount)?;
            sum = sum
                .checked_add(*amount)
                .context("Accounting batch amount overflow")?;
        }
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
            .checked_add(sum)
            .context("Accounting integer overflow")?;
        ensure!(encumbered <= limit, "Task cost budget exhausted");
        let (count,bytes):(usize,usize)=tx.query_row("SELECT count(*),coalesce(sum(length(CAST(value AS BLOB))),0) FROM records WHERE kind=?1",[ATTEMPT_KIND],|r|Ok((r.get(0)?,r.get(1)?)))?;
        ensure!(
            count
                .checked_add(amounts.len())
                .is_some_and(|n| n <= MAX_ATTEMPTS)
                && policy
                    .totals
                    .attempts
                    .checked_add(amounts.len())
                    .is_some_and(|n| n <= MAX_ATTEMPTS),
            "Accounting attempt capacity reached"
        );
        let mut tokens = Vec::with_capacity(amounts.len());
        let mut added = 0usize;
        for amount in amounts {
            self.deadline.check()?;
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
                reserved_micros: *amount,
                actual_micros: None,
                usage: None,
                events: BTreeMap::new(),
                accounting: "provider_reported_unverified_or_unknown".into(),
            };
            let encoded = crate::json::encode_bounded(&attempt, MAX_RECORD_BYTES, self.deadline)?;
            added = added
                .checked_add(encoded.len())
                .context("Accounting record size overflow")?;
            ensure!(
                bytes
                    .checked_add(added)
                    .is_some_and(|n| n <= 256 * 1024 * 1024),
                "Accounting attempt capacity reached"
            );
            // An improbable random collision must roll back, never overwrite an attempt.
            tx.execute(
                "INSERT INTO records(kind,id,value) VALUES(?1,?2,?3)",
                params![ATTEMPT_KIND, id, std::str::from_utf8(&encoded)?],
            )?;
            tokens.push(ReservedAttempt {
                id,
                workspace_id: self.store.workspace_id.clone(),
                task: task.into(),
                provider: provider.into(),
                currency: currency.into(),
                amount: *amount,
            });
        }
        policy.totals.reserved_micros = policy
            .totals
            .reserved_micros
            .checked_add(sum)
            .context("Accounting integer overflow")?;
        policy.totals.encumbered_micros = encumbered;
        policy.totals.attempts += amounts.len();
        validate_task(&policy, task)?;
        write_record(&tx, TASK_KIND, &task_id, &policy)?;
        // The typed batch owner binds its initial record inside this same commit.
        // A rejected binder or expired deadline rolls back every reservation.
        register(&tx, &tokens)?;
        self.commit(tx)?;
        Ok(tokens)
    }
    pub fn send_reserved(&self, token: ReservedAttempt) -> Result<String> {
        ensure!(
            token.workspace_id == self.store.workspace_id,
            "Reservation belongs to another workspace"
        );
        self.change(&token.id, |attempt, task| {
            ensure!(
                attempt.state == State::Reserved
                    && attempt.task == token.task
                    && attempt.provider == token.provider
                    && attempt.currency == token.currency
                    && attempt.reserved_micros == token.amount
                    && attempt.actual_micros.is_none()
                    && attempt.usage.is_none()
                    && attempt.events.is_empty(),
                "Reservation token no longer matches a never-sent attempt"
            );
            ensure!(
                !task.totals.reservation_exceeded
                    && task.totals.encumbered_micros <= task.limit_micros,
                "Current task accounting blocks this reserved send"
            );
            task.totals.unknown = task
                .totals
                .unknown
                .checked_add(1)
                .context("Accounting unknown count overflow")?;
            attempt.state = State::Sent;
            Ok(())
        })?;
        Ok(token.id)
    }
    pub fn attempt_status(&self, id: &str) -> Result<AttemptStatus> {
        self.deadline.check()?;
        self.store.set_deadline(self.deadline)?;
        if !self.store.db.is_autocommit() {
            // History inspection already holds its transaction and client
            // exclusion. Borrow that snapshot instead of nesting BEGIN.
            return self.read_attempt_status(id);
        }
        let tx = self.transaction()?;
        let result = self.read_attempt_status(id)?;
        self.commit(tx)?;
        Ok(result)
    }
    fn read_attempt_status(&self, id: &str) -> Result<AttemptStatus> {
        ensure!(valid_id(id), "Invalid provider attempt identity");
        let attempt: Attempt =
            read_record(&self.store.db, ATTEMPT_KIND, id)?.context("Unknown provider attempt")?;
        validate_attempt(&attempt, id)?;
        let task: Task = read_record(&self.store.db, TASK_KIND, &identity(&attempt.task))?
            .context("Provider task policy is missing")?;
        validate_task(&task, &attempt.task)?;
        ensure!(
            task.currency == attempt.currency,
            "Provider attempt currency mismatch"
        );
        self.deadline.check()?;
        let result = AttemptStatus {
            attempt_id: attempt.id,
            task_id: attempt.task,
            provider: attempt.provider,
            currency: attempt.currency,
            state: attempt.state,
            reserved_micros: attempt.reserved_micros,
            actual_cost_micros: attempt.actual_micros,
            reported_usage: attempt.usage,
            response_usage: attempt.events.get("response").cloned(),
        };
        Ok(result)
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
