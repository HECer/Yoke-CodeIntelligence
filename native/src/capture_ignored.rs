//! Strict immutable observed-ignore metadata for explicit host read-only captures.
use super::{CaptureOptions, Deadline};
use crate::{
    model::{Omission, Snapshot, SourceFile, identity},
    store::Store,
};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

pub(super) const RECORD_KIND: &str = "native_capture_scope_ignored_v1";
const POLICY: &str = "observed_native_ignore_filter_at_capture";
const MARKER: &str = "native-ignored-capture-v1";
const MAX_BYTES: usize = 32_000_000;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Member {
    pub path: String,
    pub content_hash: String,
    pub ignored_at_capture: bool,
}
fn members<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> std::result::Result<Vec<Member>, D::Error> {
    struct Members;
    impl<'de> serde::de::Visitor<'de> for Members {
        type Value = Vec<Member>;
        fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("at most 100000 source members")
        }
        fn visit_seq<A: serde::de::SeqAccess<'de>>(
            self,
            mut sequence: A,
        ) -> std::result::Result<Self::Value, A::Error> {
            let mut rows = vec![];
            while let Some(row) = sequence.next_element()? {
                if rows.len() == 100_000 {
                    return Err(serde::de::Error::custom("Ignored member capacity exceeded"));
                }
                rows.push(row);
            }
            Ok(rows)
        }
    }
    deserializer.deserialize_seq(Members)
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Record {
    pub schema_version: u8,
    pub snapshot_id: String,
    pub snapshot_hash: String,
    pub workspace_id: String,
    pub analyzer: String,
    pub options_hash: String,
    pub options: CaptureOptions,
    pub policy: String,
    pub proof_hash: String,
    #[serde(deserialize_with = "members")]
    pub members: Vec<Member>,
}
pub(super) fn read(store: &Store, id: &str) -> Result<Record> {
    store.check_deadline()?;
    let kind = rusqlite::limits::Limit::SQLITE_LIMIT_LENGTH;
    let previous = store.db.limit(kind)?;
    store
        .db
        .set_limit(kind, previous.min((MAX_BYTES + 4096) as i32))?;
    let result = (|| -> Result<Record> {
        let (length, storage): (usize, String) = store.db.query_row(
            "SELECT length(CAST(value AS BLOB)),typeof(value) FROM records WHERE kind=?1 AND id=?2",
            rusqlite::params![RECORD_KIND, id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        ensure!(
            length <= MAX_BYTES && storage == "text",
            "Ignored capture record capacity/type mismatch"
        );
        let raw: String = store.db.query_row(
            "SELECT value FROM records WHERE kind=?1 AND id=?2",
            rusqlite::params![RECORD_KIND, id],
            |row| row.get(0),
        )?;
        let record = serde_json::from_str(&raw)?;
        store.check_deadline()?;
        Ok(record)
    })();
    store.db.set_limit(kind, previous)?;
    result
}
struct Budget<'a, F> {
    bytes: usize,
    check: &'a F,
}
impl<F: Fn() -> Result<()>> std::io::Write for Budget<'_, F> {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        (self.check)().map_err(std::io::Error::other)?;
        self.bytes = self
            .bytes
            .checked_add(bytes.len())
            .ok_or_else(|| std::io::Error::other("Ignored capture size overflow"))?;
        if self.bytes > MAX_BYTES {
            return Err(std::io::Error::other(
                "Ignored capture record capacity exceeded",
            ));
        }
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
fn proof_hash(record: &Record) -> String {
    identity(&(&record.policy, &record.options_hash, &record.members))
}
fn snapshot_id(record: &Record, snapshot: &Snapshot) -> String {
    identity(&(
        &snapshot.workspace_id,
        "disk",
        &record.analyzer,
        &snapshot.files,
        &snapshot.omissions,
        &record.options_hash,
        MARKER,
        &record.proof_hash,
    ))
}
pub(super) fn validate(
    record: &Record,
    snapshot: &Snapshot,
    analyzer: &str,
    check: &impl Fn() -> Result<()>,
) -> Result<BTreeSet<String>> {
    check()?;
    serde_json::to_writer(&mut Budget { bytes: 0, check }, record)?;
    ensure!(
        record.schema_version == 1
            && record.policy == POLICY
            && crate::analysis::AnalyzerEpoch::parse(&record.analyzer).is_ok()
            && crate::analysis::AnalyzerEpoch::parse(analyzer).is_ok()
            && snapshot.kind == "disk"
            && snapshot.capture_consistency == "stable-double-scan"
            && record.snapshot_id == snapshot.id
            && record.workspace_id == snapshot.workspace_id
            && record.snapshot_hash == identity(snapshot)
            && record.options == record.options.validated()?
            && !record.options.include_ignored_dirs.is_empty()
            && record.options_hash == identity(&record.options)
            && record.proof_hash == proof_hash(record)
            && snapshot.id == snapshot_id(record, snapshot)
            && record.members.len() == snapshot.files.len()
            && record.members.len() <= 100_000,
        "Ignored capture identity/proof mismatch"
    );
    super::validate_manifest_options_with(
        snapshot,
        &record.options,
        record.analyzer != analyzer,
        check,
    )?;
    let mut ignored = BTreeSet::new();
    let mut previous = None;
    for (row, file) in record.members.iter().zip(&snapshot.files) {
        check()?;
        ensure!(
            row.path == file.path
                && row.content_hash == file.hash
                && crate::model::valid_id(&row.content_hash)
                && previous.is_none_or(|prior: &str| prior < row.path.as_str())
                && (!row.ignored_at_capture || record.options.ignored_prefix(&row.path)),
            "Ignored member binding/order mismatch"
        );
        crate::paths::normalized_syntax(&row.path)?;
        if row.ignored_at_capture {
            ignored.insert(row.path.clone());
        }
        previous = Some(row.path.as_str());
    }
    Ok(ignored)
}
pub(super) fn retention(store: &Store, id: &str, value: &serde_json::Value) -> Result<()> {
    let check = || store.check_deadline();
    check()?;
    ensure!(
        value["members"]
            .as_array()
            .is_some_and(|rows| rows.len() <= 100_000),
        "Ignored retention member capacity mismatch"
    );
    serde_json::to_writer(
        &mut Budget {
            bytes: 0,
            check: &check,
        },
        value,
    )?;
    let record: Record = serde_json::from_value(value.clone())?;
    ensure!(
        serde_json::to_value(&record)? == *value
            && record.schema_version == 1
            && record.snapshot_id == id
            && record.workspace_id == store.workspace_id
            && crate::analysis::AnalyzerEpoch::parse(&record.analyzer).is_ok()
            && record.policy == POLICY
            && record.options == record.options.validated()?
            && !record.options.include_ignored_dirs.is_empty()
            && record.options_hash == identity(&record.options)
            && record.proof_hash == proof_hash(&record)
            && crate::model::valid_id(&record.snapshot_hash),
        "Ignored retention proof mismatch"
    );
    let mut previous = None;
    for row in &record.members {
        check()?;
        super::read_path_kind(&row.path, &record.options)?;
        ensure!(
            crate::model::valid_id(&row.content_hash)
                && record.options.includes(&row.path)
                && previous.is_none_or(|prior: &str| prior < row.path.as_str())
                && (!row.ignored_at_capture || record.options.ignored_prefix(&row.path)),
            "Ignored retention member mismatch"
        );
        previous = Some(row.path.as_str());
    }
    let published: bool = store.db.query_row(
        "SELECT EXISTS(SELECT 1 FROM snapshots WHERE id=?1)",
        [id],
        |row| row.get(0),
    )?;
    if published {
        validate(
            &record,
            &store.snapshot(id)?,
            crate::model::ANALYZER,
            &check,
        )?;
    }
    Ok(())
}
pub(super) fn capture(
    epoch: crate::analysis::AnalyzerEpoch,
    store: &Store,
    options: &CaptureOptions,
    files: Vec<SourceFile>,
    omissions: Vec<Omission>,
    ignored: &BTreeSet<String>,
    deadline: &Deadline,
) -> Result<Snapshot> {
    let check = || deadline.check();
    let mut record = Record {
        schema_version: 1,
        snapshot_id: String::new(),
        snapshot_hash: String::new(),
        workspace_id: store.workspace_id.clone(),
        analyzer: epoch.producer().into(),
        options_hash: identity(options),
        options: options.clone(),
        policy: POLICY.into(),
        proof_hash: String::new(),
        members: vec![],
    };
    let mut size = 1024usize;
    for file in &files {
        deadline.check()?;
        size = size
            .checked_add(file.path.len().saturating_mul(6) + 256)
            .context("Ignored capture metadata overflow")?;
        ensure!(
            size <= MAX_BYTES,
            "Ignored capture metadata capacity exceeded"
        );
        record.members.push(Member {
            path: file.path.clone(),
            content_hash: file.hash.clone(),
            ignored_at_capture: ignored.contains(&file.path),
        });
    }
    record.proof_hash = proof_hash(&record);
    let mut snapshot = Snapshot {
        id: String::new(),
        workspace_id: store.workspace_id.clone(),
        kind: "disk".into(),
        files,
        omissions,
        capture_consistency: "stable-double-scan".into(),
    };
    snapshot.id = snapshot_id(&record, &snapshot);
    record.snapshot_id = snapshot.id.clone();
    record.snapshot_hash = identity(&snapshot);
    validate(&record, &snapshot, epoch.producer(), &check)?;
    let raw = serde_json::to_string(&record)?;
    ensure!(
        raw.len() <= MAX_BYTES,
        "Ignored capture record capacity exceeded"
    );
    store.db.execute(
        "INSERT INTO records(kind,id,value) VALUES(?1,?2,?3) ON CONFLICT(kind,id) DO NOTHING",
        rusqlite::params![RECORD_KIND, snapshot.id, raw],
    )?;
    ensure!(
        read(store, &snapshot.id)? == record,
        "Immutable ignored capture collision"
    );
    Ok(snapshot)
}
pub(super) fn transition(
    origin: &Snapshot,
    next: &Snapshot,
    options: &CaptureOptions,
    check: &impl Fn() -> Result<()>,
) -> Result<()> {
    if options.include_ignored_dirs.is_empty() {
        return Ok(());
    }
    let next_files: std::collections::BTreeMap<_, _> = next
        .files
        .iter()
        .map(|file| (file.path.as_str(), file))
        .collect();
    let old: std::collections::BTreeMap<_, _> = origin
        .files
        .iter()
        .map(|file| (file.path.as_str(), file))
        .collect();
    for file in &origin.files {
        check()?;
        if options.ignored_prefix(&file.path) {
            ensure!(
                next_files.get(file.path.as_str()).copied() == Some(file),
                "Read-only ignored source changed or disappeared"
            );
        }
    }
    for file in &next.files {
        check()?;
        if options.ignored_prefix(&file.path) {
            ensure!(
                old.get(file.path.as_str()).copied() == Some(file),
                "Read-only ignored source was created or changed"
            );
        }
    }
    Ok(())
}
