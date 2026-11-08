//! Authored source-bound decisions and preferences; never execution authority.
use crate::{
    capture::Deadline,
    model::{Snapshot, identity, valid_id},
    paths,
    store::Store,
};
use anyhow::{Context, Result, bail, ensure};
use rusqlite::{OptionalExtension, Transaction, TransactionBehavior, params};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::{Read, Write},
    path::Path,
};

pub const REVISION_KIND: &str = "native_decision_revision_v1";
pub const HEAD_KIND: &str = "native_decision_head_v1";
pub const HOST_KIND: &str = "native_decision_host_v1";
pub const PIN_KIND: &str = "native_decision_source_pin_v1";
const MAX_REVISION: usize = 256_000;
const MAX_TOTAL: usize = 32_000_000;
const MAX_HEADS: usize = 2_000;
const MAX_REVISIONS: usize = 10_000;
const MAX_FILE: usize = 4_000_000;
const MAX_BUNDLE: usize = 64_000_000;
const MAX_MARKDOWN: usize = 1_000_000;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Scope {
    Repository,
    Personal { host_id: String },
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Draft {
    pub key: String,
    pub kind: RecordKind,
    pub title: String,
    pub body: String,
    pub author: String,
    pub date: String,
    pub paths: Vec<String>,
    pub cost: Option<Cost>,
    #[serde(default)]
    pub rules: Vec<LiteralRule>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecordKind {
    Decision,
    Preference,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Cost {
    pub currency: String,
    pub microunits: u64,
    pub basis: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum LiteralRule {
    Follows {
        path: String,
        literal: String,
    },
    RuledOut {
        path: String,
        literal: String,
    },
    ChangedValue {
        path: String,
        before: String,
        after: String,
    },
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Anchor {
    pub path: String,
    pub content_hash: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Revision {
    pub schema_version: u32,
    pub id: String,
    pub number: u64,
    pub parent_revision: Option<String>,
    pub source_workspace_id: String,
    pub snapshot_id: String,
    pub anchors: Vec<Anchor>,
    pub draft: Draft,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Head {
    schema_version: u32,
    source_workspace_id: String,
    storage_workspace_id: String,
    scope: Scope,
    key: String,
    revision_id: String,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct HostPin {
    schema_version: u32,
    storage_workspace_id: String,
    host_id: String,
}
/// Repository-side dependency metadata deliberately excludes personal authored text.
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SourcePin {
    schema_version: u32,
    source_workspace_id: String,
    storage_workspace_id: String,
    host_binding: String,
    revision_id: String,
    number: u64,
    parent_revision_id: Option<String>,
    parent_pin_id: Option<String>,
    snapshot_id: String,
    anchors: Vec<Anchor>,
}
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Bundle {
    schema_version: u32,
    source_workspace_id: String,
    heads: BTreeMap<String, String>,
    revisions: Vec<Revision>,
}
pub struct DecisionStore<'a> {
    source: &'a Store,
    storage: &'a Store,
    scope: Scope,
}

fn text(value: &str, maximum: usize, allow_empty: bool) -> Result<()> {
    ensure!(
        value.len() <= maximum && (allow_empty || !value.trim().is_empty()),
        "Invalid decision text size"
    );
    ensure!(
        !value
            .chars()
            .any(|c| c.is_control() && !matches!(c, '\n' | '\r' | '\t')),
        "Unsupported decision text control character"
    );
    ensure!(
        !paths::obvious_secret(value),
        "Obvious secret content refused"
    );
    Ok(())
}
fn label(value: &str) -> Result<()> {
    text(value, 200, false)?;
    ensure!(
        !value.chars().any(char::is_control),
        "Decision label must be a single line"
    );
    Ok(())
}
fn key(value: &str) -> Result<()> {
    ensure!(
        !value.is_empty()
            && value.len() <= 80
            && value
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
            && !value.starts_with('-')
            && !value.ends_with('-'),
        "Invalid decision key"
    );
    paths::normalized(value)?;
    Ok(())
}
fn date(value: &str) -> Result<()> {
    let b = value.as_bytes();
    ensure!(
        b.len() == 10
            && b[4] == b'-'
            && b[7] == b'-'
            && b.iter()
                .enumerate()
                .all(|(i, c)| i == 4 || i == 7 || c.is_ascii_digit()),
        "Invalid authored date"
    );
    let year: u32 = value[..4].parse()?;
    let month: usize = value[5..7].parse()?;
    let day: u32 = value[8..].parse()?;
    let days = [
        31,
        if year.is_multiple_of(4) && (!year.is_multiple_of(100) || year.is_multiple_of(400)) {
            29
        } else {
            28
        },
        31,
        30,
        31,
        30,
        31,
        31,
        30,
        31,
        30,
        31,
    ];
    ensure!(
        year > 0 && (1..=12).contains(&month) && day > 0 && day <= days[month - 1],
        "Invalid authored calendar date"
    );
    Ok(())
}
fn rule_path(rule: &LiteralRule) -> &str {
    match rule {
        LiteralRule::Follows { path, .. }
        | LiteralRule::RuledOut { path, .. }
        | LiteralRule::ChangedValue { path, .. } => path,
    }
}
fn validate_draft(draft: &Draft, d: &Deadline) -> Result<()> {
    crate::json::encode_bounded(draft, 128_000, d)?;
    key(&draft.key)?;
    text(&draft.title, 500, false)?;
    text(&draft.body, 64_000, false)?;
    label(&draft.author)?;
    date(&draft.date)?;
    ensure!(
        !draft.title.chars().any(char::is_control),
        "Decision title must be a single line"
    );
    ensure!(
        !draft.paths.is_empty() && draft.paths.len() <= 100 && draft.rules.len() <= 32,
        "Decision source or rule capacity exceeded"
    );
    let mut seen = BTreeSet::new();
    for path in &draft.paths {
        d.check()?;
        paths::normalized_syntax(path)?;
        ensure!(seen.insert(path), "Duplicate decision path");
    }
    if let Some(cost) = &draft.cost {
        ensure!(
            cost.currency.len() == 3 && cost.currency.bytes().all(|b| b.is_ascii_uppercase()),
            "Invalid authored cost currency"
        );
        text(&cost.basis, 500, false)?;
    }
    for rule in &draft.rules {
        ensure!(
            seen.contains(&rule_path(rule).to_owned()),
            "Decision rule path requires an explicit anchor"
        );
        match rule {
            LiteralRule::Follows { literal, .. } | LiteralRule::RuledOut { literal, .. } => {
                text(literal, 4096, false)?
            }
            LiteralRule::ChangedValue { before, after, .. } => {
                text(before, 4096, false)?;
                text(after, 4096, false)?;
                ensure!(before != after, "Changed-value literals must differ");
            }
        }
    }
    Ok(())
}
fn snapshot_scope(source: &Store, snapshot: &Snapshot, d: &Deadline) -> Result<()> {
    d.check()?;
    source.set_deadline(d)?;
    ensure!(
        valid_id(&snapshot.id)
            && source.workspace_id == snapshot.workspace_id
            && identity(&source.snapshot(&snapshot.id)?) == identity(snapshot),
        "Decision source snapshot mismatch"
    );
    Ok(())
}
fn source_text(source: &Store, snapshot: &Snapshot, path: &str, d: &Deadline) -> Result<String> {
    d.check()?;
    crate::capture::source_path_policy(source, snapshot, path)?;
    let file = snapshot
        .files
        .iter()
        .find(|f| f.path == path)
        .context("Decision source path absent from snapshot")?;
    let bytes = source.read_blob_exact(&file.hash, file.size, MAX_FILE)?;
    String::from_utf8(bytes).context("Decision anchors require UTF-8 source")
}
fn revision_id(revision: &Revision) -> String {
    let mut copy = revision.clone();
    copy.id.clear();
    identity(&copy)
}
fn head_id(head: &Head) -> String {
    identity(&(
        &head.source_workspace_id,
        &head.storage_workspace_id,
        &head.scope,
        &head.key,
    ))
}
fn raw(store: &Store, kind: &str, id: &str) -> Result<Option<String>> {
    ensure!(valid_id(id), "Invalid decision record ID");
    let size: Option<usize> = store
        .db
        .query_row(
            "SELECT length(CAST(value AS BLOB)) FROM records WHERE kind=?1 AND id=?2",
            params![kind, id],
            |r| r.get(0),
        )
        .optional()?;
    ensure!(
        size.is_none_or(|n| n <= MAX_REVISION),
        "Decision persisted record exceeds capacity"
    );
    let value: Option<String> = store
        .db
        .query_row(
            "SELECT value FROM records WHERE kind=?1 AND id=?2 AND length(CAST(value AS BLOB))<=?3",
            params![kind, id, MAX_REVISION],
            |r| r.get(0),
        )
        .optional()?;
    ensure!(
        size.is_some() == value.is_some(),
        "Decision persisted record changed"
    );
    Ok(value)
}
fn decode<T: serde::de::DeserializeOwned>(raw: &str) -> Result<T> {
    crate::json::decode(crate::json::parse(raw.as_bytes(), MAX_REVISION)?)
}
fn revision(storage: &Store, id: &str, d: &Deadline) -> Result<Revision> {
    d.check()?;
    let r: Revision =
        decode(&raw(storage, REVISION_KIND, id)?.context("Decision revision unavailable")?)?;
    ensure!(
        r.schema_version == 1
            && r.id == id
            && revision_id(&r) == id
            && r.number > 0
            && r.number <= MAX_REVISIONS as u64
            && valid_id(&r.source_workspace_id)
            && valid_id(&r.snapshot_id),
        "Decision revision identity mismatch"
    );
    ensure!(
        (r.number == 1 && r.parent_revision.is_none())
            || (r.number > 1 && r.parent_revision.as_deref().is_some_and(valid_id)),
        "Invalid decision revision parent"
    );
    validate_draft(&r.draft, d)?;
    ensure!(
        r.anchors.len() == r.draft.paths.len()
            && r.anchors
                .iter()
                .zip(&r.draft.paths)
                .all(|(a, p)| a.path == *p && valid_id(&a.content_hash)),
        "Decision revision anchors mismatch"
    );
    Ok(r)
}
fn validate_source(source: &Store, r: &Revision, d: &Deadline) -> Result<()> {
    validate_source_anchors(
        source,
        &r.source_workspace_id,
        &r.snapshot_id,
        &r.anchors,
        d,
    )
}
fn validate_source_anchors(
    source: &Store,
    workspace_id: &str,
    snapshot_id: &str,
    anchors: &[Anchor],
    d: &Deadline,
) -> Result<()> {
    ensure!(
        workspace_id == source.workspace_id && valid_id(snapshot_id),
        "Foreign decision source workspace"
    );
    ensure!(
        !anchors.is_empty() && anchors.len() <= 100,
        "Invalid decision source anchor count"
    );
    let snapshot = source.snapshot(snapshot_id)?;
    snapshot_scope(source, &snapshot, d)?;
    let mut total = 0usize;
    let mut seen = BTreeSet::new();
    for anchor in anchors {
        paths::normalized_syntax(&anchor.path)?;
        ensure!(
            seen.insert(&anchor.path) && valid_id(&anchor.content_hash),
            "Duplicate or invalid decision source anchor"
        );
        let file = snapshot
            .files
            .iter()
            .find(|f| f.path == anchor.path)
            .context("Decision source anchor absent")?;
        ensure!(
            file.hash == anchor.content_hash,
            "Decision source anchor hash mismatch"
        );
        total = total
            .checked_add(file.size)
            .context("Decision source size overflow")?;
        ensure!(
            total <= MAX_TOTAL,
            "Decision anchored source exceeds capacity"
        );
        source_text(source, &snapshot, &anchor.path, d)?;
    }
    Ok(())
}
fn validate_chain(
    source: &Store,
    storage: &Store,
    first: &Revision,
    d: &Deadline,
) -> Result<Vec<Revision>> {
    let mut result = vec![];
    let mut r = first.clone();
    loop {
        d.check()?;
        validate_source(source, &r, d)?;
        let parent = r.parent_revision.clone();
        result.push(r.clone());
        let Some(id) = parent else { break };
        let prior = revision(storage, &id, d)?;
        ensure!(
            prior.draft.key == r.draft.key
                && prior.source_workspace_id == r.source_workspace_id
                && prior.number.checked_add(1) == Some(r.number),
            "Decision parent chain mismatch"
        );
        r = prior;
        ensure!(
            result.len() < MAX_REVISIONS,
            "Decision history capacity exceeded"
        );
    }
    Ok(result)
}
fn save_revision(tx: &Transaction<'_>, r: &Revision, d: &Deadline) -> Result<()> {
    let bytes = crate::json::encode_bounded(r, MAX_REVISION, d)?;
    let raw = String::from_utf8(bytes)?;
    let old: Option<String> = tx
        .query_row(
            "SELECT value FROM records WHERE kind=?1 AND id=?2",
            params![REVISION_KIND, r.id],
            |row| row.get(0),
        )
        .optional()?;
    ensure!(
        old.as_ref().is_none_or(|v| v == &raw),
        "Immutable decision revision collision"
    );
    tx.execute(
        "INSERT OR IGNORE INTO records(kind,id,value) VALUES(?1,?2,?3)",
        params![REVISION_KIND, r.id, raw],
    )?;
    Ok(())
}
fn capacity(storage: &Store) -> Result<()> {
    let (count, bytes): (usize, usize) = storage.db.query_row(
        "SELECT count(*),coalesce(sum(length(CAST(value AS BLOB))),0) FROM records WHERE kind=?1",
        [REVISION_KIND],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    ensure!(
        count <= MAX_REVISIONS && bytes <= MAX_TOTAL,
        "Decision store capacity exceeded"
    );
    let count: usize = storage.db.query_row(
        "SELECT count(*) FROM records WHERE kind=?1",
        [HEAD_KIND],
        |r| r.get(0),
    )?;
    ensure!(count <= MAX_HEADS, "Decision head capacity exceeded");
    Ok(())
}
impl<'a> DecisionStore<'a> {
    pub fn new(source: &'a Store, storage: &'a Store, scope: Scope, d: &Deadline) -> Result<Self> {
        d.check()?;
        source.set_deadline(d)?;
        storage.set_deadline(d)?;
        match &scope {
            Scope::Repository => ensure!(
                source.workspace_id == storage.workspace_id
                    && source.dir.canonicalize()? == storage.dir.canonicalize()?,
                "Repository decisions require own source store"
            ),
            Scope::Personal { host_id } => {
                label(host_id)?;
                ensure!(
                    source.workspace_id != storage.workspace_id
                        && source.dir.canonicalize()? != storage.dir.canonicalize()?,
                    "Personal decisions require a separate host-selected store"
                );
                let pin = HostPin {
                    schema_version: 1,
                    storage_workspace_id: storage.workspace_id.clone(),
                    host_id: host_id.clone(),
                };
                let id = identity(&("decision-host-v1", &storage.workspace_id));
                let tx = Transaction::new_unchecked(&storage.db, TransactionBehavior::Immediate)?;
                let existing = raw(storage, HOST_KIND, &id)?;
                if let Some(existing) = existing {
                    let p: HostPin = decode(&existing)?;
                    ensure!(
                        p.schema_version == 1
                            && p.storage_workspace_id == storage.workspace_id
                            && p.host_id == *host_id,
                        "Personal decision host pin mismatch"
                    );
                } else {
                    tx.execute(
                        "INSERT INTO records(kind,id,value) VALUES(?1,?2,?3)",
                        params![HOST_KIND, id, serde_json::to_string(&pin)?],
                    )?;
                }
                tx.commit()?;
            }
        }
        Ok(Self {
            source,
            storage,
            scope,
        })
    }
    fn admit(&self, d: &Deadline) -> Result<()> {
        d.check()?;
        self.source.set_deadline(d)?;
        self.storage.set_deadline(d)?;
        capacity(self.storage)
    }
    fn head(&self, key: &str, revision_id: String) -> Head {
        Head {
            schema_version: 1,
            source_workspace_id: self.source.workspace_id.clone(),
            storage_workspace_id: self.storage.workspace_id.clone(),
            scope: self.scope.clone(),
            key: key.into(),
            revision_id,
        }
    }
    fn current(&self, name: &str, d: &Deadline) -> Result<Option<Revision>> {
        key(name)?;
        let expected = self.head(name, String::new());
        let id = head_id(&expected);
        let Some(raw) = raw(self.storage, HEAD_KIND, &id)? else {
            return Ok(None);
        };
        let h: Head = decode(&raw)?;
        ensure!(
            h.schema_version == 1
                && h.source_workspace_id == expected.source_workspace_id
                && h.storage_workspace_id == expected.storage_workspace_id
                && h.scope == expected.scope
                && h.key == name
                && head_id(&h) == id,
            "Decision head binding mismatch"
        );
        let r = revision(self.storage, &h.revision_id, d)?;
        ensure!(
            r.draft.key == h.key && r.source_workspace_id == h.source_workspace_id,
            "Decision head revision mismatch"
        );
        validate_chain(self.source, self.storage, &r, d)?;
        Ok(Some(r))
    }
    fn save_head(&self, tx: &Transaction<'_>, r: &Revision) -> Result<()> {
        let h = self.head(&r.draft.key, r.id.clone());
        tx.execute("INSERT INTO records(kind,id,value) VALUES(?1,?2,?3) ON CONFLICT(kind,id) DO UPDATE SET value=excluded.value",params![HEAD_KIND,head_id(&h),serde_json::to_string(&h)?])?;
        Ok(())
    }
    fn pin_sources(&self, revisions: &[Revision], d: &Deadline) -> Result<()> {
        let Scope::Personal { host_id } = &self.scope else {
            return Ok(());
        };
        let mut ordered: Vec<_> = revisions.iter().collect();
        ordered.sort_by_key(|r| r.number);
        let host_binding = crate::model::hash(host_id);
        let mut pin_ids = BTreeMap::new();
        let tx = Transaction::new_unchecked(&self.source.db, TransactionBehavior::Immediate)?;
        for r in ordered {
            d.check()?;
            validate_source(self.source, r, d)?;
            let parent_pin_id = r
                .parent_revision
                .as_ref()
                .map(|id| {
                    pin_ids
                        .get(id)
                        .cloned()
                        .context("Personal decision parent pin missing")
                })
                .transpose()?;
            let pin = SourcePin {
                schema_version: 1,
                source_workspace_id: self.source.workspace_id.clone(),
                storage_workspace_id: self.storage.workspace_id.clone(),
                host_binding: host_binding.clone(),
                revision_id: r.id.clone(),
                number: r.number,
                parent_revision_id: r.parent_revision.clone(),
                parent_pin_id,
                snapshot_id: r.snapshot_id.clone(),
                anchors: r.anchors.clone(),
            };
            let id = identity(&pin);
            let encoded = String::from_utf8(crate::json::encode_bounded(&pin, MAX_REVISION, d)?)?;
            if let Some(prior) = raw(self.source, PIN_KIND, &id)? {
                ensure!(prior == encoded, "Immutable personal source pin collision");
            } else {
                tx.execute(
                    "INSERT INTO records(kind,id,value) VALUES(?1,?2,?3)",
                    params![PIN_KIND, id, encoded],
                )?;
            }
            pin_ids.insert(r.id.clone(), id);
        }
        let (count,bytes): (usize,usize) = self.source.db.query_row("SELECT count(*),coalesce(sum(length(CAST(value AS BLOB))),0) FROM records WHERE kind=?1",[PIN_KIND],|r|Ok((r.get(0)?,r.get(1)?)))?;
        ensure!(
            count <= MAX_REVISIONS && bytes <= MAX_TOTAL,
            "Personal source pin capacity exceeded"
        );
        d.check()?;
        // Commit dependency roots first. A later personal failure may leave conservative orphan pins.
        tx.commit()?;
        Ok(())
    }
    pub fn write(
        &self,
        snapshot: &Snapshot,
        draft: Draft,
        expected_revision: Option<&str>,
        d: &Deadline,
    ) -> Result<Revision> {
        self.admit(d)?;
        snapshot_scope(self.source, snapshot, d)?;
        validate_draft(&draft, d)?;
        ensure!(
            expected_revision.is_none_or(valid_id),
            "Invalid expected decision revision"
        );
        let anchors = draft
            .paths
            .iter()
            .map(|path| {
                source_text(self.source, snapshot, path, d)?;
                let file = snapshot
                    .files
                    .iter()
                    .find(|f| f.path == *path)
                    .context("Decision source path absent")?;
                Ok(Anchor {
                    path: path.clone(),
                    content_hash: file.hash.clone(),
                })
            })
            .collect::<Result<Vec<_>>>()?;
        let tx = Transaction::new_unchecked(&self.storage.db, TransactionBehavior::Immediate)?;
        let prior = self.current(&draft.key, d)?;
        ensure!(
            prior.as_ref().map(|r| r.id.as_str()) == expected_revision,
            "Stale decision revision or create-only collision"
        );
        let mut r = Revision {
            schema_version: 1,
            id: String::new(),
            number: prior.as_ref().map_or(1, |r| r.number + 1),
            parent_revision: prior.map(|r| r.id),
            source_workspace_id: snapshot.workspace_id.clone(),
            snapshot_id: snapshot.id.clone(),
            anchors,
            draft,
        };
        r.id = revision_id(&r);
        validate_source(self.source, &r, d)?;
        if matches!(&self.scope, Scope::Personal { .. }) {
            self.pin_sources(&validate_chain(self.source, self.storage, &r, d)?, d)?;
        }
        save_revision(&tx, &r, d)?;
        self.save_head(&tx, &r)?;
        capacity(self.storage)?;
        d.check()?;
        tx.commit()?;
        Ok(r)
    }
    pub fn get(&self, name: &str, d: &Deadline) -> Result<Revision> {
        self.admit(d)?;
        self.current(name, d)?
            .context("Decision record unavailable")
    }
    pub fn list(&self, d: &Deadline) -> Result<Vec<Revision>> {
        self.admit(d)?;
        let mut stmt = self.storage.db.prepare(
            "SELECT id,length(CAST(value AS BLOB)) FROM records WHERE kind=?1 ORDER BY id LIMIT ?2",
        )?;
        let ids = stmt
            .query_map(params![HEAD_KIND, MAX_HEADS + 1], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, usize>(1)?))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        ensure!(ids.len() <= MAX_HEADS, "Decision head capacity exceeded");
        let mut records = BTreeMap::new();
        for (id, size) in ids {
            d.check()?;
            ensure!(size <= MAX_REVISION, "Decision head exceeds capacity");
            let h: Head =
                decode(&raw(self.storage, HEAD_KIND, &id)?.context("Decision head disappeared")?)?;
            ensure!(
                head_id(&h) == id
                    && h.schema_version == 1
                    && h.storage_workspace_id == self.storage.workspace_id,
                "Decision head identity mismatch"
            );
            if h.source_workspace_id == self.source.workspace_id && h.scope == self.scope {
                let r = self
                    .current(&h.key, d)?
                    .context("Decision head disappeared")?;
                ensure!(records.insert(h.key, r).is_none(), "Duplicate decision key");
            }
        }
        Ok(records.into_values().collect())
    }
    pub fn check(&self, name: &str, snapshot: &Snapshot, d: &Deadline) -> Result<Value> {
        self.admit(d)?;
        snapshot_scope(self.source, snapshot, d)?;
        let r = self.get(name, d)?;
        let baseline = self.source.snapshot(&r.snapshot_id)?;
        let mut outcomes = Vec::new();
        for (index, rule) in r.draft.rules.iter().enumerate() {
            d.check()?;
            let path = rule_path(rule);
            let before = source_text(self.source, &baseline, path, d)?;
            let Some(file) = snapshot.files.iter().find(|f| f.path == path) else {
                outcomes.push(json!({"ruleIndex":index,"rule":rule,"outcome":"inconclusive_missing_file","recordRevision":r.id}));
                continue;
            };
            let after = source_text(self.source, snapshot, path, d)?;
            let (outcome, before_matches, after_matches) = match rule {
                LiteralRule::Follows { literal, .. } => {
                    let b = matches(&baseline, path, &before, literal, d)?;
                    let a = matches(snapshot, path, &after, literal, d)?;
                    (if a.is_empty() { "violated" } else { "followed" }, b, a)
                }
                LiteralRule::RuledOut { literal, .. } => {
                    let b = matches(&baseline, path, &before, literal, d)?;
                    let a = matches(snapshot, path, &after, literal, d)?;
                    (if a.is_empty() { "followed" } else { "violated" }, b, a)
                }
                LiteralRule::ChangedValue {
                    before: old,
                    after: new,
                    ..
                } => {
                    let b = matches(&baseline, path, &before, old, d)?;
                    let a = matches(snapshot, path, &after, new, d)?;
                    let outcome = if !b.is_empty()
                        && !a.is_empty()
                        && !before.contains(new)
                        && !after.contains(old)
                    {
                        "literal_transition"
                    } else if before == after {
                        "unchanged"
                    } else {
                        "inconclusive"
                    };
                    (outcome, b, a)
                }
            };
            outcomes.push(json!({"ruleIndex":index,"rule":rule,"recordRevision":r.id,"outcome":outcome,"baselineSnapshotId":baseline.id,"candidateSnapshotId":snapshot.id,"candidateContentHash":file.hash,"baselineContentHash":baseline.files.iter().find(|f|f.path==path).context("Missing baseline anchor")?.hash,"beforeEvidence":before_matches,"afterEvidence":after_matches,"absenceScope":"complete_captured_file"}));
        }
        let value = json!({"classification":"literal_heuristic_not_semantic_proof","recordRevision":r.id,"authoredNote":{"title":r.draft.title,"body":r.draft.body,"author":r.draft.author,"date":r.draft.date},"sourceWritten":false,"rules":outcomes});
        crate::json::encode_bounded(&value, 4_000_000, d)?;
        Ok(value)
    }
    pub fn export_bundle(&self, destination: &Path, d: &Deadline) -> Result<Value> {
        self.admit(d)?;
        ensure!(
            destination.is_absolute(),
            "Explicit absolute decision export destination required"
        );
        let parent = destination
            .parent()
            .context("Decision export parent required")?;
        directory(parent)?;
        let filename = destination
            .file_name()
            .and_then(|n| n.to_str())
            .context("Invalid decision export name")?;
        paths::normalized_syntax(filename)?;
        ensure!(
            fs::symlink_metadata(destination)
                .is_err_and(|e| e.kind() == std::io::ErrorKind::NotFound),
            "Fresh decision export destination required"
        );
        let tx = Transaction::new_unchecked(&self.storage.db, TransactionBehavior::Deferred)?;
        let heads = self.list(d)?;
        let mut revisions = BTreeMap::new();
        for r in &heads {
            for prior in validate_chain(self.source, self.storage, r, d)? {
                revisions.insert(prior.id.clone(), prior);
            }
        }
        let bundle = Bundle {
            schema_version: 1,
            source_workspace_id: self.source.workspace_id.clone(),
            heads: heads
                .iter()
                .map(|r| (r.draft.key.clone(), r.id.clone()))
                .collect(),
            revisions: revisions.into_values().collect(),
        };
        let manifest = crate::json::encode_bounded(&bundle, MAX_TOTAL, d)?;
        let mut files = BTreeMap::from([("manifest.json".to_owned(), manifest)]);
        let mut total = files.values().map(Vec::len).sum::<usize>();
        for r in heads {
            let bytes = markdown(&r).into_bytes();
            total = total
                .checked_add(bytes.len())
                .context("Decision export size overflow")?;
            ensure!(total <= MAX_BUNDLE, "Decision export capacity exceeded");
            files.insert(format!("{}.md", r.draft.key), bytes);
        }
        d.check()?;
        fs::create_dir(destination)?;
        // A failed export remains a partial fresh bundle. Existing files are never overwritten or cleaned up.
        for (name, bytes) in files {
            d.check()?;
            directory(destination)?;
            let mut f = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(destination.join(name))?;
            f.write_all(&bytes)?;
            f.sync_all()?;
        }
        d.check()?;
        tx.commit()?;
        Ok(
            json!({"destination":destination,"heads":bundle.heads.len(),"revisions":bundle.revisions.len(),"bytes":total,"sourceWritten":false,"committed":false}),
        )
    }
    pub fn import_bundle(
        &self,
        directory_path: &Path,
        expected: &BTreeMap<String, Option<String>>,
        d: &Deadline,
    ) -> Result<Value> {
        self.admit(d)?;
        directory(directory_path)?;
        let bytes = read_file(&directory_path.join("manifest.json"), MAX_TOTAL, d)?;
        let bundle: Bundle = crate::json::decode(crate::json::parse(&bytes, MAX_TOTAL)?)?;
        ensure!(
            bundle.schema_version == 1
                && bundle.source_workspace_id == self.source.workspace_id
                && bundle.heads.len() <= MAX_HEADS
                && bundle.revisions.len() <= MAX_REVISIONS
                && bundle.heads.keys().eq(expected.keys()),
            "Decision bundle schema, source or expected-head mismatch"
        );
        for (key, id) in expected {
            crate::decision_records::key(key)?;
            ensure!(
                id.as_deref().is_none_or(valid_id),
                "Invalid expected decision revision"
            );
        }
        let mut revisions = BTreeMap::new();
        for r in bundle.revisions {
            d.check()?;
            ensure!(
                r.schema_version == 1
                    && valid_id(&r.id)
                    && revision_id(&r) == r.id
                    && r.number > 0
                    && r.number <= MAX_REVISIONS as u64,
                "Invalid imported decision revision"
            );
            validate_draft(&r.draft, d)?;
            ensure!(
                r.anchors.len() == r.draft.paths.len()
                    && r.anchors
                        .iter()
                        .zip(&r.draft.paths)
                        .all(|(a, p)| a.path == *p && valid_id(&a.content_hash)),
                "Imported decision anchor mismatch"
            );
            validate_source(self.source, &r, d)?;
            ensure!(
                revisions.insert(r.id.clone(), r).is_none(),
                "Duplicate imported decision revision"
            );
        }
        let mut visited = BTreeSet::new();
        let mut files = BTreeSet::from(["manifest.json".to_owned()]);
        let mut total = bytes.len();
        for (name, id) in &bundle.heads {
            key(name)?;
            ensure!(valid_id(id), "Invalid imported decision head");
            let r = revisions
                .get(id)
                .context("Missing imported decision head revision")?;
            ensure!(r.draft.key == *name, "Imported decision head key mismatch");
            let projection = markdown(r).into_bytes();
            let filename = format!("{name}.md");
            ensure!(
                projection.len() <= MAX_MARKDOWN,
                "Decision Markdown projection capacity exceeded"
            );
            let actual = read_file(&directory_path.join(&filename), MAX_MARKDOWN, d)?;
            ensure!(
                projection == actual,
                "Imported Markdown differs from authored projection"
            );
            total = total
                .checked_add(actual.len())
                .context("Decision import size overflow")?;
            ensure!(total <= MAX_BUNDLE, "Decision import capacity exceeded");
            files.insert(filename);
            let mut cursor = r;
            loop {
                d.check()?;
                visited.insert(cursor.id.clone());
                match &cursor.parent_revision {
                    None => {
                        ensure!(cursor.number == 1, "Imported decision root number mismatch");
                        break;
                    }
                    Some(parent) => {
                        let prior = revisions
                            .get(parent)
                            .context("Imported decision parent absent")?;
                        ensure!(
                            prior.draft.key == *name
                                && prior.number.checked_add(1) == Some(cursor.number),
                            "Imported decision parent chain mismatch"
                        );
                        cursor = prior;
                    }
                }
            }
        }
        ensure!(
            visited.len() == revisions.len(),
            "Unreachable imported decision revisions"
        );
        let mut actual_files = BTreeSet::new();
        for entry in fs::read_dir(directory_path)? {
            d.check()?;
            let entry = entry?;
            let metadata = fs::symlink_metadata(entry.path())?;
            ensure!(
                metadata.is_file() && !paths::is_link(&metadata),
                "Decision bundle must contain ordinary files only"
            );
            let name = entry
                .file_name()
                .into_string()
                .map_err(|_| anyhow::anyhow!("Non-UTF8 decision bundle filename"))?;
            ensure!(
                actual_files.insert(name) && actual_files.len() <= MAX_HEADS + 1,
                "Decision bundle file capacity exceeded"
            );
        }
        ensure!(actual_files == files, "Unexpected decision bundle files");
        let tx = Transaction::new_unchecked(&self.storage.db, TransactionBehavior::Immediate)?;
        for (name, expected_id) in expected {
            ensure!(
                self.current(name, d)?.as_ref().map(|r| &r.id) == expected_id.as_ref(),
                "Stale imported decision head"
            );
            if let Some(current_id) = expected_id {
                let mut cursor = revisions
                    .get(
                        bundle
                            .heads
                            .get(name)
                            .context("Imported decision head absent")?,
                    )
                    .context("Imported decision head revision absent")?;
                loop {
                    d.check()?;
                    if cursor.id == *current_id {
                        break;
                    }
                    let parent = cursor
                        .parent_revision
                        .as_ref()
                        .context("Imported decision would roll back or replace current lineage")?;
                    cursor = revisions
                        .get(parent)
                        .context("Imported decision lineage absent")?;
                }
            }
        }
        if matches!(&self.scope, Scope::Personal { .. }) {
            self.pin_sources(&revisions.values().cloned().collect::<Vec<_>>(), d)?;
        }
        for r in revisions.values() {
            save_revision(&tx, r, d)?;
        }
        for id in bundle.heads.values() {
            self.save_head(&tx, revisions.get(id).context("Missing imported head")?)?;
        }
        capacity(self.storage)?;
        d.check()?;
        tx.commit()?;
        Ok(
            json!({"importedHeads":bundle.heads.len(),"importedRevisions":revisions.len(),"sourceWritten":false}),
        )
    }
    pub fn invoke(
        &self,
        snapshot: &Snapshot,
        operation: &str,
        args: &Value,
        d: &Deadline,
    ) -> Result<Value> {
        self.admit(d)?;
        snapshot_scope(self.source, snapshot, d)?;
        crate::json::encode_bounded(args, MAX_REVISION, d)?;
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase", deny_unknown_fields)]
        struct WriteInput {
            draft: Draft,
            expected_revision: Option<String>,
        }
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct KeyInput {
            key: String,
        }
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Empty {}
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct ExportInput {
            destination: String,
        }
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase", deny_unknown_fields)]
        struct ImportInput {
            directory: String,
            expected_heads: BTreeMap<String, Option<String>>,
        }
        match operation {
            "write" => {
                let input: WriteInput = crate::json::decode(args.clone())?;
                Ok(
                    json!({"revision":self.write(snapshot,input.draft,input.expected_revision.as_deref(),d)?,"sourceWritten":false,"trust":"authored_data_not_execution_authority"}),
                )
            }
            "read" => {
                let input: KeyInput = crate::json::decode(args.clone())?;
                let r = self.get(&input.key, d)?;
                let currentness:Vec<_>=r.anchors.iter().map(|a|json!({"path":a.path,"state":match snapshot.files.iter().find(|f|f.path==a.path){Some(f) if f.hash==a.content_hash=>"current",Some(_)=>"changed",None=>"missing"}})).collect();
                Ok(
                    json!({"revision":r,"anchorCurrentness":currentness,"trust":"authored_data_not_execution_authority"}),
                )
            }
            "list" => {
                let _: Empty = crate::json::decode(args.clone())?;
                Ok(json!({"records":self.list(d)?,"scope":self.scope}))
            }
            "check" => {
                let input: KeyInput = crate::json::decode(args.clone())?;
                self.check(&input.key, snapshot, d)
            }
            "export" => {
                let input: ExportInput = crate::json::decode(args.clone())?;
                self.export_bundle(Path::new(&input.destination), d)
            }
            "import" => {
                let input: ImportInput = crate::json::decode(args.clone())?;
                self.import_bundle(Path::new(&input.directory), &input.expected_heads, d)
            }
            _ => bail!("Unknown decision record operation"),
        }
    }
}

fn matches(
    snapshot: &Snapshot,
    path: &str,
    source: &str,
    literal: &str,
    d: &Deadline,
) -> Result<Vec<Value>> {
    let file = snapshot
        .files
        .iter()
        .find(|f| f.path == path)
        .context("Missing decision evidence file")?;
    let mut result = Vec::new();
    for (start, matched) in source.match_indices(literal) {
        d.check()?;
        ensure!(
            result.len() < 64,
            "Decision literal evidence capacity exceeded"
        );
        result.push(json!({"snapshotId":snapshot.id,"workspaceId":snapshot.workspace_id,"path":path,"contentHash":file.hash,"startByte":start,"endByte":start+matched.len(),"text":matched}));
    }
    Ok(result)
}
fn escaped(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}
fn markdown(r: &Revision) -> String {
    let draft = &r.draft;
    let mut out = format!(
        "# Authored {:?}: {}\n\nRevision: `{}` ({}).\n\nDate: {}\n\nSource snapshot: `{}`\n\nAuthor and authored details are preserved in the fenced data below.\n\n",
        draft.kind, draft.key, r.id, r.number, draft.date, r.snapshot_id
    );
    let json = serde_json::to_string_pretty(draft).expect("serializable decision draft");
    let fence = "`".repeat(
        json.split(|c| c != '`')
            .map(str::len)
            .max()
            .unwrap_or(0)
            .max(2)
            + 1,
    );
    out.push_str(&format!("{fence}json\n{}\n{fence}\n\nAuthored data; no execution authority. Costs are supplied attribution, not measured billing.\n",escaped(&json)));
    out
}
fn directory(path: &Path) -> Result<()> {
    ensure!(
        path.is_absolute(),
        "Explicit absolute decision bundle directory required"
    );
    for part in path.ancestors() {
        let meta = fs::symlink_metadata(part)?;
        ensure!(
            meta.is_dir() && !paths::is_link(&meta),
            "Decision bundle directory link refused"
        );
    }
    Ok(())
}
fn read_file(path: &Path, maximum: usize, d: &Deadline) -> Result<Vec<u8>> {
    d.check()?;
    directory(path.parent().context("Decision bundle parent required")?)?;
    let meta = fs::symlink_metadata(path)?;
    ensure!(
        meta.is_file() && !paths::is_link(&meta) && meta.len() <= maximum as u64,
        "Invalid or oversized decision bundle file"
    );
    let mut file = fs::File::open(path)?;
    let opened = file.metadata()?;
    ensure!(
        opened.is_file() && opened.len() == meta.len(),
        "Decision bundle file changed"
    );
    let mut bytes = Vec::with_capacity(meta.len() as usize);
    let mut buffer = [0u8; 64000];
    loop {
        d.check()?;
        let n = file.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        ensure!(
            n <= maximum.saturating_sub(bytes.len()),
            "Decision bundle file grew beyond capacity"
        );
        bytes.extend_from_slice(&buffer[..n]);
    }
    ensure!(
        bytes.len() as u64 == meta.len(),
        "Decision bundle file size changed"
    );
    std::str::from_utf8(&bytes).context("Decision bundle requires UTF-8")?;
    Ok(bytes)
}

/// Fail closed when a personal store references source snapshots unavailable here.
/// Callers retain all returned source snapshots and CAS bytes; records remain durable roots.
pub struct RetentionRoots {
    pub snapshot_ids: BTreeSet<String>,
    pub blob_ids: BTreeSet<String>,
}
pub fn validated_roots(store: &Store, d: &Deadline) -> Result<RetentionRoots> {
    store.set_deadline(d)?;
    capacity(store)?;
    let mut roots = RetentionRoots {
        snapshot_ids: BTreeSet::new(),
        blob_ids: BTreeSet::new(),
    };
    let mut statement=store.db.prepare("SELECT kind,id,length(CAST(value AS BLOB)) FROM records WHERE kind IN (?1,?2,?3,?4) ORDER BY kind,id LIMIT ?5")?;
    let rows = statement
        .query_map(
            params![
                REVISION_KIND,
                HEAD_KIND,
                HOST_KIND,
                PIN_KIND,
                2 * MAX_REVISIONS + MAX_HEADS + 2
            ],
            |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, usize>(2)?,
                ))
            },
        )?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    ensure!(
        rows.len() <= 2 * MAX_REVISIONS + MAX_HEADS + 1,
        "Decision retention capacity exceeded"
    );
    let mut revisions = BTreeMap::new();
    let mut pins = BTreeMap::new();
    let mut heads = Vec::new();
    let mut pin_bytes = 0usize;
    for (kind, id, size) in rows {
        d.check()?;
        ensure!(
            size <= MAX_REVISION,
            "Decision retention record exceeds capacity"
        );
        match kind.as_str() {
            REVISION_KIND => {
                let r = revision(store, &id, d)?;
                validate_source(store, &r, d)?;
                roots.snapshot_ids.insert(r.snapshot_id.clone());
                roots
                    .blob_ids
                    .extend(r.anchors.iter().map(|a| a.content_hash.clone()));
                revisions.insert(id, r);
            }
            HEAD_KIND => {
                let h: Head = decode(
                    &raw(store, HEAD_KIND, &id)?.context("Missing decision retention head")?,
                )?;
                ensure!(
                    h.schema_version == 1
                        && head_id(&h) == id
                        && h.source_workspace_id == store.workspace_id
                        && h.storage_workspace_id == store.workspace_id
                        && h.scope == Scope::Repository,
                    "Decision retention head scope mismatch"
                );
                heads.push(h);
            }
            HOST_KIND => {
                let h: HostPin = decode(
                    &raw(store, HOST_KIND, &id)?.context("Missing decision retention host pin")?,
                )?;
                label(&h.host_id)?;
                ensure!(
                    h.schema_version == 1
                        && h.storage_workspace_id == store.workspace_id
                        && identity(&("decision-host-v1", &store.workspace_id)) == id,
                    "Decision retention host pin mismatch"
                );
            }
            PIN_KIND => {
                pin_bytes = pin_bytes
                    .checked_add(size)
                    .context("Decision pin byte overflow")?;
                ensure!(
                    pin_bytes <= MAX_TOTAL && pins.len() < MAX_REVISIONS,
                    "Decision source pin capacity exceeded"
                );
                let pin: SourcePin =
                    decode(&raw(store, PIN_KIND, &id)?.context("Missing decision source pin")?)?;
                ensure!(
                    pin.schema_version == 1
                        && identity(&pin) == id
                        && pin.source_workspace_id == store.workspace_id
                        && valid_id(&pin.storage_workspace_id)
                        && pin.storage_workspace_id != store.workspace_id
                        && valid_id(&pin.host_binding)
                        && valid_id(&pin.revision_id)
                        && pin.number > 0
                        && pin.number <= MAX_REVISIONS as u64,
                    "Decision source pin identity mismatch"
                );
                ensure!(
                    (pin.number == 1
                        && pin.parent_revision_id.is_none()
                        && pin.parent_pin_id.is_none())
                        || (pin.number > 1
                            && pin.parent_revision_id.as_deref().is_some_and(valid_id)
                            && pin.parent_pin_id.as_deref().is_some_and(valid_id)),
                    "Decision source pin parent mismatch"
                );
                validate_source_anchors(
                    store,
                    &pin.source_workspace_id,
                    &pin.snapshot_id,
                    &pin.anchors,
                    d,
                )?;
                roots.snapshot_ids.insert(pin.snapshot_id.clone());
                roots
                    .blob_ids
                    .extend(pin.anchors.iter().map(|a| a.content_hash.clone()));
                pins.insert(id, pin);
            }
            _ => bail!("Unknown decision retention schema"),
        }
    }
    // Every immediate parent decreases a bounded positive revision number, so cycles
    // cannot be admitted. Validate each record once instead of rescanning every chain.
    for r in revisions.values() {
        d.check()?;
        if let Some(parent) = &r.parent_revision {
            let prior = revisions
                .get(parent)
                .context("Decision retention parent absent")?;
            ensure!(
                prior.source_workspace_id == r.source_workspace_id
                    && prior.draft.key == r.draft.key
                    && prior.number.checked_add(1) == Some(r.number),
                "Decision retention parent alias mismatch"
            );
        }
    }
    for h in heads {
        let r = revisions
            .get(&h.revision_id)
            .context("Decision retention head revision absent")?;
        ensure!(
            r.draft.key == h.key && r.source_workspace_id == h.source_workspace_id,
            "Decision retention head alias mismatch"
        );
    }
    for pin in pins.values() {
        d.check()?;
        if let Some(parent) = &pin.parent_pin_id {
            let prior = pins
                .get(parent)
                .context("Decision source pin parent absent")?;
            ensure!(
                prior.source_workspace_id == pin.source_workspace_id
                    && prior.storage_workspace_id == pin.storage_workspace_id
                    && prior.host_binding == pin.host_binding
                    && Some(&prior.revision_id) == pin.parent_revision_id.as_ref()
                    && prior.number.checked_add(1) == Some(pin.number),
                "Decision source pin parent alias mismatch"
            );
        }
    }
    Ok(roots)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn authored_decision_creates_a_source_bound_immutable_revision() {
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::write(dir.path().join("config.rs"), "const X: bool = true;\n").unwrap();
        let mut engine = crate::Engine::open(dir.path()).unwrap();
        let d = Deadline::new(10_000);
        let (snapshot, _, _, _) = engine.index(&d).unwrap();
        let access =
            DecisionStore::new(&engine.store, &engine.store, Scope::Repository, &d).unwrap();
        let draft = Draft {
            key: "storage-policy".into(),
            kind: RecordKind::Decision,
            title: "Keep immutable source".into(),
            body: "Use snapshots".into(),
            author: "Ada".into(),
            date: "2026-10-15".into(),
            paths: vec!["config.rs".into()],
            cost: None,
            rules: Vec::new(),
        };
        let revision = access.write(&snapshot, draft, None, &d).unwrap();
        assert_eq!(revision.number, 1);
        assert_eq!(revision.anchors[0].content_hash, snapshot.files[0].hash);
        assert_eq!(access.get("storage-policy", &d).unwrap().id, revision.id);
    }
}
