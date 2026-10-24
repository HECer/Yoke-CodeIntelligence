//! Immutable raw intake plus receipts, transactionally committed before HTTP.
use super::*;
use rusqlite::{Transaction, TransactionBehavior, limits::Limit, params};
use std::time::{SystemTime, UNIX_EPOCH};
const MAX_RECORD: usize = 8_016_384;
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Record {
    schema_version: u32,
    receiver_workspace_id: String,
    endpoint: String,
    principal_id: String,
    proposal_id: String,
    payload_hash: String,
    received_at_unix: u64,
    body: String,
}
impl Record {
    fn validate(&self, store: &Store, d: &Deadline) -> Result<Scope> {
        ensure!(
            self.schema_version == 1
                && self.receiver_workspace_id == store.workspace_id
                && valid_id(&self.proposal_id)
                && valid_id(&self.payload_hash)
                && crate::github_app::credential_name(&self.principal_id)
                && (60..=253_402_296_000).contains(&self.received_at_unix)
                && self.body.len() <= 4_000_000
                && hash(self.body.as_bytes()) == self.payload_hash,
            BINDING
        );
        // Validate old endpoint syntax independently of current host grants.
        let p = Profile {
            schema_version: 1,
            endpoint: self.endpoint.clone(),
            grants: vec![],
            max_payload_bytes: 4_000_000,
            max_records: 1000,
            max_stored_bytes: 32_000_000,
        };
        p.origin().map_err(|_| anyhow::anyhow!(BINDING))?;
        let s = Submission::parse(self.body.as_bytes(), 4_000_000, d)
            .map_err(|_| anyhow::anyhow!(BINDING))?;
        ensure!(
            s.proposal(&self.endpoint, self.body.as_bytes()) == self.proposal_id,
            BINDING
        );
        Ok(s.scope())
    }
    fn receipt(&self) -> Value {
        json!({"schemaVersion":1,"proposalId":self.proposal_id,"payloadHash":self.payload_hash,"receiptId":identity(self),"accepted":true})
    }
}
struct History {
    records: BTreeMap<String, (Record, Scope)>,
    bytes: usize,
}
fn load(store: &Store, p: &Profile, d: &Deadline) -> Result<History> {
    let mut query=store.db.prepare("SELECT id,typeof(value),length(CAST(value AS BLOB)) FROM records WHERE kind=?1 ORDER BY id")?;
    let mut rows = query.query([RECORD_KIND])?;
    let mut history = History {
        records: BTreeMap::new(),
        bytes: 0,
    };
    while let Some(row) = rows.next()? {
        d.check()?;
        let id: String = row.get(0)?;
        let kind: String = row.get(1)?;
        let len: usize = row.get(2)?;
        ensure!(valid_id(&id) && kind == "text", BINDING);
        ensure!(
            len <= MAX_RECORD
                && history.records.len() < p.max_records
                && history.bytes.saturating_add(len) <= p.max_stored_bytes,
            CAPACITY
        );
        let raw: String = store.db.query_row(
            "SELECT value FROM records WHERE kind=?1 AND id=?2",
            params![RECORD_KIND, id],
            |r| r.get(0),
        )?;
        ensure!(raw.len() == len, BINDING);
        let r: Record = crate::json::decode(
            crate::json::parse(raw.as_bytes(), MAX_RECORD).map_err(|_| anyhow::anyhow!(BINDING))?,
        )
        .map_err(|_| anyhow::anyhow!(BINDING))?;
        ensure!(id == r.proposal_id, BINDING);
        let s = r.validate(store, d)?;
        history.bytes += len;
        history.records.insert(id, (r, s));
    }
    Ok(history)
}
fn durable(store: &Store) -> Result<()> {
    let sync: u32 = store.db.query_row("PRAGMA synchronous", [], |r| r.get(0))?;
    let mode: String = store
        .db
        .query_row("PRAGMA journal_mode", [], |r| r.get(0))?;
    ensure!(
        (2..=3).contains(&sync) && mode == "wal",
        "github_app_digest_receiver_sql_durability_required"
    );
    Ok(())
}
fn sql<T>(
    store: &Store,
    p: &Profile,
    write: bool,
    d: &Deadline,
    run: impl FnOnce(&History) -> Result<T>,
) -> Result<T> {
    ensure!(
        store.db.is_autocommit(),
        "github_app_digest_receiver_caller_transaction_refused"
    );
    crate::github_app::worker::publication_policy(store, d, || {
        let prior = store.db.limit(Limit::SQLITE_LIMIT_LENGTH)?;
        store.db.set_limit(
            Limit::SQLITE_LIMIT_LENGTH,
            prior.min((MAX_RECORD + 65536) as i32),
        )?;
        if write {
            durable(store)?;
        }
        let tx = Transaction::new_unchecked(
            &store.db,
            if write {
                TransactionBehavior::Immediate
            } else {
                TransactionBehavior::Deferred
            },
        )?;
        let h = load(store, p, d)?;
        let value = run(&h)?;
        d.check()?;
        tx.commit()?;
        d.check()?;
        Ok(value)
    })
}
pub(super) fn validate(store: &Store, p: &Profile, d: &Deadline) -> Result<()> {
    sql(store, p, true, d, |_| Ok(()))
}
pub(super) fn receive(
    store: &Store,
    shared: &Shared,
    access: &Access,
    id: &str,
    raw: &[u8],
    d: &Deadline,
) -> Result<(u16, Value)> {
    ensure!(raw.len() <= shared.profile.max_payload_bytes, CAPACITY);
    let submitted = Submission::parse(raw, shared.profile.max_payload_bytes, d)?;
    ensure!(
        access.permits(&submitted),
        "github_app_digest_receiver_scope_denied"
    );
    ensure!(
        submitted.proposal(&shared.profile.endpoint, raw) == id,
        BINDING
    );
    let at = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
    sql(store, &shared.profile, true, d, |h| {
        if let Some((r, s)) = h.records.get(id) {
            ensure!(
                r.endpoint == shared.profile.endpoint
                    && r.principal_id == access.id
                    && access.permits_scope(s)
                    && r.body.as_bytes() == raw,
                BINDING
            );
            return Ok((200, r.receipt()));
        }
        let record = Record {
            schema_version: 1,
            receiver_workspace_id: store.workspace_id.clone(),
            endpoint: shared.profile.endpoint.clone(),
            principal_id: access.id.clone(),
            proposal_id: id.into(),
            payload_hash: hash(raw),
            received_at_unix: at,
            body: std::str::from_utf8(raw)
                .map_err(|_| anyhow::anyhow!(INVALID))?
                .into(),
        };
        record.validate(store, d)?;
        let encoded = crate::json::encode_bounded(&serde_json::to_value(&record)?, MAX_RECORD, d)?;
        ensure!(
            h.records.len() < shared.profile.max_records
                && h.bytes.saturating_add(encoded.len()) <= shared.profile.max_stored_bytes,
            CAPACITY
        );
        let receipt = record.receipt();
        http::packet(201, &receipt, shared.config.max_response_bytes, d)?;
        d.check()?;
        store.db.execute(
            "INSERT INTO records(kind,id,value) VALUES(?1,?2,?3)",
            params![RECORD_KIND, id, std::str::from_utf8(&encoded)?],
        )?;
        Ok((201, receipt))
    })
}
pub(super) fn get(
    store: &Store,
    shared: &Shared,
    accesses: &[&Access],
    id: &str,
    d: &Deadline,
) -> Result<Option<Value>> {
    sql(store, &shared.profile, false, d, |h| {
        Ok(h.records
            .get(id)
            .filter(|(r, s)| {
                r.endpoint == shared.profile.endpoint
                    && accesses
                        .iter()
                        .any(|a| a.id == r.principal_id && a.permits_scope(s))
            })
            .map(|(r, _)| r.receipt()))
    })
}
pub(super) fn retained(store: &Store, d: &Deadline) -> Result<BTreeSet<String>> {
    // No body field or remote snapshot identifier is ever a local CAS root.
    let p = Profile {
        schema_version: 1,
        endpoint: "https://unused.invalid/".into(),
        grants: vec![],
        max_payload_bytes: 4_000_000,
        max_records: 1000,
        max_stored_bytes: 32_000_000,
    };
    let h = load(store, &p, d)?;
    Ok(h.records.into_keys().collect())
}
