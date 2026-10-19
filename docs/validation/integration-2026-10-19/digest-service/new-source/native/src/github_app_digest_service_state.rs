//! Bounded, signed, append-only intents and receipts. No transaction spans HTTP.
use super::*;
use hmac::{Hmac, Mac};
use rusqlite::{Transaction, TransactionBehavior, params};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

const MAX_RECORD: usize = 8192;
const DOMAIN: &[u8] = b"yoke-code-intelligence/digest-service/v1\n";
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Data {
    pub app_id: u64,
    pub request: PreviewRequest,
    pub snapshot_ids: Vec<String>,
    pub endpoint: String,
    pub fixture: bool,
    pub proposal_id: String,
    pub payload_hash: String,
    pub digest_id: String,
}
impl Data {
    fn group(&self) -> (u64, u64) {
        // Reinstallation must not bypass a previous unknown write for the
        // same App/repository. Installation remains part of exact approval.
        (self.app_id, self.request.repository_id)
    }
    fn validate(&self, workspace: &str) -> Result<()> {
        self.request
            .validate()
            .map_err(|_| anyhow::anyhow!(BINDING))?;
        let profile = Profile {
            schema_version: 1,
            endpoint: self.endpoint.clone(),
            credential_name: if self.fixture {
                None
            } else {
                Some("validation".into())
            },
            grants: vec![Grant {
                installation_id: self.request.installation_id,
                repository_id: self.request.repository_id,
                repository: self.request.repository.clone(),
            }],
            max_payload_bytes: 4_000_000,
            max_response_bytes: 64_000,
            max_journal_records: 1000,
            max_journal_bytes: 8_000_000,
        };
        profile.validate().map_err(|_| anyhow::anyhow!(BINDING))?;
        ensure!(
            self.fixture == profile.fixture()?
                && super::super::super::numeric(self.app_id)
                && (1..=32).contains(&self.snapshot_ids.len())
                && self.snapshot_ids.iter().all(|s| valid_id(s))
                && self.snapshot_ids.windows(2).all(|s| s[0] < s[1])
                && self.snapshot_ids.contains(&self.request.snapshot_id)
                && [&self.proposal_id, &self.payload_hash, &self.digest_id]
                    .iter()
                    .all(|s| valid_id(s))
                && self.proposal_id
                    == identity(
                        &json!({"schemaVersion":1,"workspaceId":workspace,"appId":self.app_id,"endpoint":self.endpoint,"payloadHash":self.payload_hash})
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
        request_id: String,
        data: Data,
    },
    Accepted {
        request_id: String,
        receipt: Receipt,
        recovered: bool,
    },
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Record {
    schema_version: u32,
    workspace_id: String,
    app_id: u64,
    sequence: usize,
    previous: Option<String>,
    at_unix: u64,
    event: Event,
    signature256: String,
}
struct Entry {
    intent_id: String,
    request_id: String,
    data: Data,
    receipt: Option<Receipt>,
    recovered: bool,
}
#[derive(Default)]
struct History {
    records: Vec<(String, Record)>,
    entries: BTreeMap<(u64, String), Entry>,
    pending: BTreeSet<(u64, u64)>,
    proposals: BTreeSet<(u64, String)>,
    bytes: usize,
}
fn message(record: &Record, d: &Deadline) -> Result<Vec<u8>> {
    let mut value = serde_json::to_value(record)?;
    value
        .as_object_mut()
        .ok_or_else(|| anyhow::anyhow!(BINDING))?
        .remove("signature256");
    let mut bytes = DOMAIN.to_vec();
    bytes.extend(crate::json::encode_bounded(&value, MAX_RECORD, d)?);
    Ok(bytes)
}
fn sign(record: &Record, host: &Host, d: &Deadline) -> Result<String> {
    let key = host.keys.first().ok_or_else(|| anyhow::anyhow!(BINDING))?;
    let mut mac =
        Hmac::<sha2::Sha256>::new_from_slice(&key.0).map_err(|_| anyhow::anyhow!(BINDING))?;
    mac.update(&message(record, d)?);
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
        bytes: usize,
        workspace: &str,
        host: Option<&Host>,
        d: &Deadline,
    ) -> Result<()> {
        d.check()?;
        ensure!(
            record.schema_version == 1
                && record.workspace_id == workspace
                && super::super::super::numeric(record.app_id)
                && record.sequence == self.records.len() + 1
                && record.previous.as_ref() == self.records.last().map(|(id, _)| id)
                && record.at_unix >= 60
                && record.at_unix <= 253_402_296_000
                && self
                    .records
                    .last()
                    .is_none_or(|(_, r)| r.at_unix <= record.at_unix)
                && record.signature256.len() == 71
                && record.signature256.starts_with("sha256=")
                && record.signature256.as_bytes()[7..]
                    .iter()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(b))
                && id == identity(&record),
            BINDING
        );
        if let Some(host) = host
            && record.app_id == host.profile.app_id
        {
            host.verify(&message(&record, d)?, &record.signature256)
                .map_err(|_| anyhow::anyhow!(BINDING))?;
        }
        match &record.event {
            Event::Intent {
                request_id: id,
                data,
            } => {
                data.validate(workspace)?;
                ensure!(
                    request_id(id)
                        && data.app_id == record.app_id
                        && !self.entries.contains_key(&(data.app_id, id.clone()))
                        && self.pending.insert(data.group())
                        && self
                            .proposals
                            .insert((data.app_id, data.proposal_id.clone())),
                    BINDING
                );
                self.entries.insert(
                    (data.app_id, id.clone()),
                    Entry {
                        intent_id: identity(&record),
                        request_id: id.clone(),
                        data: data.clone(),
                        receipt: None,
                        recovered: false,
                    },
                );
            }
            Event::Accepted {
                request_id: id,
                receipt,
                recovered,
            } => {
                let entry = self
                    .entries
                    .get_mut(&(record.app_id, id.clone()))
                    .ok_or_else(|| anyhow::anyhow!(BINDING))?;
                receipt
                    .validate(&entry.data)
                    .map_err(|_| anyhow::anyhow!(BINDING))?;
                ensure!(
                    entry.receipt.is_none() && self.pending.remove(&entry.data.group()),
                    BINDING
                );
                entry.receipt = Some(receipt.clone());
                entry.recovered = *recovered;
            }
        }
        self.bytes += bytes;
        self.records.push((id, record));
        Ok(())
    }
}
fn load(store: &Store, host: Option<&Host>, profile: &Profile, d: &Deadline) -> Result<History> {
    let mut stmt = store.db.prepare(
        "SELECT id,length(CAST(value AS BLOB)),typeof(value) FROM records WHERE kind=?1 ORDER BY id",
    )?;
    let mut rows = stmt.query([RECORD_KIND])?;
    let mut records = vec![];
    let mut total = 0usize;
    while let Some(row) = rows.next()? {
        d.check()?;
        let len: usize = row.get(1)?;
        ensure!(
            len <= MAX_RECORD
                && records.len() < profile.max_journal_records
                && total.saturating_add(len) <= profile.max_journal_bytes,
            CAPACITY
        );
        let id: String = row.get(0)?;
        ensure!(valid_id(&id) && row.get::<_, String>(2)? == "text", BINDING);
        // Do not materialize an unbounded caller-modified JSON value before
        // checking its stored byte length in the pinned SQL transaction.
        let raw: String = store.db.query_row(
            "SELECT value FROM records WHERE kind=?1 AND id=?2",
            params![RECORD_KIND, id],
            |r| r.get(0),
        )?;
        ensure!(raw.len() == len, BINDING);
        let record: Record = crate::json::decode(
            crate::json::parse(raw.as_bytes(), MAX_RECORD).map_err(|_| anyhow::anyhow!(BINDING))?,
        )
        .map_err(|_| anyhow::anyhow!(BINDING))?;
        records.push((id, record, len));
        total += len;
    }
    records.sort_by_key(|(_, r, _)| r.sequence);
    let mut history = History::default();
    for (id, record, len) in records {
        history.apply(id, record, len, &store.workspace_id, host, d)?;
    }
    reserve(&history, profile, 0)?;
    Ok(history)
}
fn reserve(h: &History, p: &Profile, new: usize) -> Result<()> {
    ensure!(
        h.records.len() + h.pending.len() + new * 2 <= p.max_journal_records
            && h.bytes
                .saturating_add((h.pending.len() + new * 2) * MAX_RECORD)
                <= p.max_journal_bytes,
        CAPACITY
    );
    Ok(())
}
fn durable(c: &Context<'_>) -> Result<()> {
    let sync: u32 = c
        .store
        .db
        .query_row("PRAGMA synchronous", [], |r| r.get(0))?;
    let mode: String = c
        .store
        .db
        .query_row("PRAGMA journal_mode", [], |r| r.get(0))?;
    ensure!(
        (2..=3).contains(&sync) && mode == "wal",
        "github_app_digest_service_sql_durability_required"
    );
    Ok(())
}
fn sql<T>(
    c: &Context<'_>,
    write: bool,
    d: &Deadline,
    run: impl FnOnce(&mut History) -> Result<T>,
) -> Result<T> {
    c.admit(false, write, d)?;
    crate::github_app::worker::publication_policy(c.store, d, || {
        use rusqlite::limits::Limit;
        let prior = c.store.db.limit(Limit::SQLITE_LIMIT_LENGTH)?;
        c.store.db.set_limit(
            Limit::SQLITE_LIMIT_LENGTH,
            prior.min((MAX_RECORD + 65536) as i32),
        )?;
        if write {
            durable(c)?;
        }
        let tx = Transaction::new_unchecked(
            &c.store.db,
            if write {
                TransactionBehavior::Immediate
            } else {
                TransactionBehavior::Deferred
            },
        )?;
        let mut h = load(c.store, Some(c.host), c.profile, d)?;
        let value = run(&mut h)?;
        d.check()?;
        tx.commit()?;
        d.check()?;
        Ok(value)
    })
}
fn current(c: &Context<'_>, entry: &Entry) -> Result<()> {
    c.scope(&entry.data.request)?;
    ensure!(
        entry.data.app_id == c.host.profile.app_id
            && entry.data.endpoint == c.profile.endpoint
            && entry.data.fixture == c.profile.fixture()?,
        BINDING
    );
    Ok(())
}
fn value(entry: &Entry, historical: bool, requests: usize) -> Value {
    json!({"schemaVersion":1,"kind":"native_digest_service_status","status":if entry.receipt.is_some(){"accepted"}else{"unknown"},
        "requestId":entry.request_id,"intentId":entry.intent_id,"proposalId":entry.data.proposal_id,"payloadHash":entry.data.payload_hash,
        "receiptId":entry.receipt.as_ref().map(|r|&r.receipt_id),"endpoint":entry.data.endpoint,"fixture":entry.data.fixture,
        "historical":historical,"recovered":entry.recovered,"networkRequests":requests,"serviceAccepted":entry.receipt.is_some(),
        "processingComplete":false,"retryAllowed":false,"sourceWritten":false,"modelCalls":0,
        "receiptScope":"selected_receiver_claim_about_exact_payload_not_processing_or_provider_authentication"})
}
fn output(c: &Context<'_>, value: &Value, d: &Deadline) -> Result<()> {
    crate::json::encode_bounded(value, c.options.max_output_bytes - 1, d)
        .map_err(|_| d.check().err().unwrap_or_else(|| anyhow::anyhow!(CAPACITY)))?;
    Ok(())
}
fn append(c: &Context<'_>, h: &mut History, event: Event, d: &Deadline) -> Result<String> {
    let at = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| anyhow::anyhow!(BINDING))?
        .as_secs();
    let mut record = Record {
        schema_version: 1,
        workspace_id: c.store.workspace_id.clone(),
        app_id: c.host.profile.app_id,
        sequence: h.records.len() + 1,
        previous: h.records.last().map(|(id, _)| id.clone()),
        at_unix: at,
        event,
        signature256: String::new(),
    };
    record.signature256 = sign(&record, c.host, d)?;
    let raw = crate::json::encode_bounded(&serde_json::to_value(&record)?, MAX_RECORD, d)
        .map_err(|_| anyhow::anyhow!(CAPACITY))?;
    let id = identity(&record);
    h.apply(
        id.clone(),
        record,
        raw.len(),
        &c.store.workspace_id,
        Some(c.host),
        d,
    )?;
    reserve(h, c.profile, 0)?;
    d.check()?;
    c.store.db.execute(
        "INSERT INTO records(kind,id,value) VALUES(?1,?2,?3)",
        params![RECORD_KIND, id, std::str::from_utf8(&raw)?],
    )?;
    Ok(id)
}
pub(super) fn existing(
    c: &Context<'_>,
    approval: &Approval,
    d: &Deadline,
) -> Result<Option<Value>> {
    sql(c, false, d, |h| {
        if let Some(entry) = h
            .entries
            .get(&(c.host.profile.app_id, approval.request_id.clone()))
        {
            current(c, entry)?;
            ensure!(
                entry.data.proposal_id == approval.proposal_id
                    && entry.data.payload_hash == approval.payload_hash,
                "github_app_digest_service_request_conflict"
            );
            ensure!(entry.receipt.is_some(), UNKNOWN);
            let v = value(entry, true, 0);
            output(c, &v, d)?;
            return Ok(Some(v));
        }
        ensure!(
            !h.proposals
                .contains(&(c.host.profile.app_id, approval.proposal_id.clone())),
            "github_app_digest_service_duplicate_proposal"
        );
        Ok(None)
    })
}
pub(super) fn intend(
    c: &Context<'_>,
    approval: &Approval,
    data: &Data,
    d: &Deadline,
) -> Result<()> {
    sql(c, true, d, |h| {
        c.scope(&data.request)?;
        data.validate(&c.store.workspace_id)?;
        ensure!(
            !h.entries
                .contains_key(&(data.app_id, approval.request_id.clone()))
                && !h
                    .proposals
                    .contains(&(data.app_id, data.proposal_id.clone())),
            "github_app_digest_service_request_conflict"
        );
        ensure!(!h.pending.contains(&data.group()), UNKNOWN);
        reserve(h, c.profile, 1)?;
        // Admit all terminal output variants before making the intent durable.
        let mut candidate = Entry {
            intent_id: "0".repeat(64),
            request_id: approval.request_id.clone(),
            data: data.clone(),
            receipt: None,
            recovered: false,
        };
        output(c, &value(&candidate, true, 0), d)?;
        candidate.receipt = Some(Receipt {
            schema_version: 1,
            proposal_id: data.proposal_id.clone(),
            payload_hash: data.payload_hash.clone(),
            receipt_id: "0".repeat(64),
            accepted: true,
        });
        for historical in [true, false] {
            for recovered in [true, false] {
                candidate.recovered = recovered;
                output(c, &value(&candidate, historical, 1), d)?;
            }
        }
        append(
            c,
            h,
            Event::Intent {
                request_id: approval.request_id.clone(),
                data: data.clone(),
            },
            d,
        )?;
        Ok(())
    })
}
pub(super) fn accept(
    c: &Context<'_>,
    request: &str,
    receipt: &Receipt,
    recovered: bool,
    d: &Deadline,
) -> Result<Value> {
    sql(c, true, d, |h| {
        let key = (c.host.profile.app_id, request.to_owned());
        let entry = h
            .entries
            .get(&key)
            .ok_or_else(|| anyhow::anyhow!(BINDING))?;
        current(c, entry)?;
        ensure!(entry.receipt.is_none(), BINDING);
        receipt.validate(&entry.data)?;
        append(
            c,
            h,
            Event::Accepted {
                request_id: request.into(),
                receipt: receipt.clone(),
                recovered,
            },
            d,
        )?;
        let v = value(
            h.entries
                .get(&key)
                .ok_or_else(|| anyhow::anyhow!(BINDING))?,
            false,
            1,
        );
        output(c, &v, d)?;
        Ok(v)
    })
}
pub(super) fn pending(
    c: &Context<'_>,
    request: &str,
    d: &Deadline,
) -> Result<(Data, Option<Value>)> {
    sql(c, false, d, |h| {
        let entry = h
            .entries
            .get(&(c.host.profile.app_id, request.into()))
            .ok_or_else(|| anyhow::anyhow!("github_app_digest_service_request_not_found"))?;
        current(c, entry)?;
        let mut worst = value(entry, true, 1);
        worst["historical"] = json!(false);
        worst["status"] = json!("accepted");
        worst["receiptId"] = json!("0".repeat(64));
        output(c, &worst, d)?;
        let terminal = entry.receipt.as_ref().map(|_| value(entry, true, 0));
        Ok((entry.data.clone(), terminal))
    })
}
pub(super) fn status(c: &Context<'_>, request: &str, d: &Deadline) -> Result<Value> {
    sql(c, false, d, |h| {
        let entry = h
            .entries
            .get(&(c.host.profile.app_id, request.into()))
            .ok_or_else(|| anyhow::anyhow!("github_app_digest_service_request_not_found"))?;
        current(c, entry)?;
        let v = value(entry, true, 0);
        output(c, &v, d)?;
        Ok(v)
    })
}
pub(super) struct Lock {
    _file: fs::File,
}
pub(super) fn lock(c: &Context<'_>, d: &Deadline) -> Result<Lock> {
    let path = c.store.dir.join("digest-service.lock");
    if let Ok(meta) = fs::symlink_metadata(&path) {
        ensure!(meta.is_file() && !crate::paths::is_link(&meta), BINDING);
    }
    let mut open = fs::OpenOptions::new();
    open.read(true).write(true).create(true).truncate(false);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        open.share_mode(0x00000001 | 0x00000002);
    }
    let file = open.open(path).map_err(|_| anyhow::anyhow!(BINDING))?;
    loop {
        d.check()?;
        match fs2::FileExt::try_lock_exclusive(&file) {
            Ok(()) => return Ok(Lock { _file: file }),
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
pub(super) fn retained_roots(
    store: &Store,
    d: &Deadline,
) -> Result<(BTreeSet<String>, BTreeSet<String>)> {
    // Retention recognizes the closed structural contract, never mints a send
    // permit and has no signing credential authority. Runtime replay verifies MACs.
    let profile = Profile {
        schema_version: 1,
        endpoint: "https://unused.invalid/".into(),
        credential_name: Some("unused".into()),
        grants: vec![],
        max_payload_bytes: 4_000_000,
        max_response_bytes: 64_000,
        max_journal_records: 1000,
        max_journal_bytes: 8_000_000,
    };
    let h = load(store, None, &profile, d)?;
    let roots = h
        .entries
        .values()
        .flat_map(|e| e.data.snapshot_ids.iter().cloned())
        .collect();
    Ok((h.records.into_iter().map(|(id, _)| id).collect(), roots))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rehashed_structurally_valid_intent_still_requires_its_original_mac() {
        let profile=crate::github_app::HostProfile::parse(br#"{"schemaVersion":1,"appId":123,"secretCredential":"fixture","grants":[{"installationId":42,"repositoryId":9,"repository":"org/repo"}]}"#).unwrap();
        let host = Host::with_signing_key(
            profile,
            crate::github_app::SigningKey::from_host_bytes(b"owned-journal-test-key").unwrap(),
        )
        .unwrap();
        let workspace = "1".repeat(64);
        let payload_hash = "2".repeat(64);
        let endpoint = "https://owned.invalid/".to_owned();
        let data = Data {
            app_id: 123,
            request: PreviewRequest {
                schema_version: 1,
                snapshot_id: "3".repeat(64),
                installation_id: 42,
                repository_id: 9,
                repository: "org/repo".into(),
            },
            snapshot_ids: vec!["3".repeat(64)],
            proposal_id: identity(
                &json!({"schemaVersion":1,"workspaceId":workspace,"appId":123,"endpoint":endpoint,"payloadHash":payload_hash}),
            ),
            endpoint,
            fixture: false,
            payload_hash,
            digest_id: "4".repeat(64),
        };
        let d = Deadline::new(20000);
        let mut record = Record {
            schema_version: 1,
            workspace_id: workspace.clone(),
            app_id: 123,
            sequence: 1,
            previous: None,
            at_unix: 100,
            event: Event::Intent {
                request_id: "01234567-89ab-cdef-0123-456789abcdef".into(),
                data,
            },
            signature256: String::new(),
        };
        record.signature256 = sign(&record, &host, &d).unwrap();
        History::default()
            .apply(
                identity(&record),
                record.clone(),
                1000,
                &workspace,
                Some(&host),
                &d,
            )
            .unwrap();
        if let Event::Intent { data, .. } = &mut record.event {
            data.digest_id = "5".repeat(64);
        }
        // Same actual typed serializer recomputes the row ID; closed structural
        // retention replay accepts the unchanged schema and valid field bindings.
        let forged_id = identity(&record);
        History::default()
            .apply(
                forged_id.clone(),
                record.clone(),
                1000,
                &workspace,
                None,
                &d,
            )
            .unwrap();
        assert!(
            History::default()
                .apply(forged_id, record, 1000, &workspace, Some(&host), &d)
                .is_err()
        );
    }
}
