//! Explicit immutable source preparation; no capture, artifact publication or HTTP.
use super::*;
use crate::{
    github_app::Job,
    model::{Graph, Snapshot, hash, valid_id},
};
use rusqlite::{OptionalExtension, Transaction, TransactionBehavior, limits::Limit, params};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::Read,
    time::Duration,
};

pub(super) const UNAVAILABLE: &str = "github_app_worker_source_unavailable";
pub(super) fn available<T>(result: Result<T>, d: &Deadline) -> Result<T> {
    result.map_err(|_| {
        d.check()
            .err()
            .unwrap_or_else(|| anyhow::anyhow!(UNAVAILABLE))
    })
}
pub(super) const MAX_ROW: usize = 64_000_000;
const MAX_METADATA: usize = 128_000_000;
const KINDS: [&str; 4] = [
    crate::capture::OPTIONS_RECORD_KIND,
    crate::capture::IGNORED_RECORD_KIND,
    crate::capture::DERIVED_OPTIONS_RECORD_KIND,
    "overlay-snapshot",
];
#[derive(Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "camelCase")]
enum Slot {
    Snapshot,
    Graph,
    Options,
    Ignored,
    Derived,
    Overlay,
}
impl Slot {
    fn kind(&self) -> Option<&'static str> {
        match self {
            Self::Snapshot | Self::Graph => None,
            Self::Options => Some(KINDS[0]),
            Self::Ignored => Some(KINDS[1]),
            Self::Derived => Some(KINDS[2]),
            Self::Overlay => Some(KINDS[3]),
        }
    }
    fn slots() -> [Self; 4] {
        [Self::Options, Self::Ignored, Self::Derived, Self::Overlay]
    }
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RowSeal {
    slot: Slot,
    id: String,
    bytes: Option<usize>,
    hash: Option<String>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Proof {
    pub snapshot_id: String,
    lineage: Vec<String>,
    seals: Vec<RowSeal>,
}
pub(super) struct PreparedSnapshot {
    snapshot: Snapshot,
    graph: Graph,
    proof: Proof,
}
pub(super) struct Prepared {
    head: PreparedSnapshot,
    base: Option<PreparedSnapshot>,
}
pub(super) struct Analysis {
    head: Proof,
    base: Option<Proof>,
    bindings: Vec<Binding>,
    provider: crate::github_app_auth::read::PullRequest,
    impact: Value,
    base_impact: Option<Value>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Binding {
    snapshot_id: String,
    path: String,
    content_hash: String,
    git_blob_sha: String,
    start_byte: usize,
    end_byte: usize,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Review {
    schema_version: u32,
    pub head_snapshot_id: String,
    pub base_snapshot_id: Option<String>,
    head_proof: Proof,
    base_proof: Option<Proof>,
    bindings: Vec<Binding>,
    provider: crate::github_app_auth::read::PullRequest,
    pub provider_reads: crate::github_app_auth::read::Receipt,
    impact: Value,
    base_impact: Option<Value>,
    impact_hash: String,
    base_impact_hash: Option<String>,
}

/// Restore the caller's SQL policy even when pure analysis uses legacy Store helpers.
pub(super) fn policy<T>(
    store: &Store,
    d: &Deadline,
    run: impl FnOnce(&Deadline) -> Result<T>,
) -> Result<T> {
    let effective = store.inherited_deadline(d)?;
    let busy: u64 = store
        .db
        .query_row("PRAGMA busy_timeout", [], |r| r.get(0))?;
    let length = store.db.limit(Limit::SQLITE_LIMIT_LENGTH)?;
    let prior = store.replace_deadline(Some(effective.clone()));
    // A public caller's busy_timeout replaces SQLite's callback. Updating the
    // registered deadline data alone does not reinstall that callback before
    // direct SQL (including BEGIN IMMEDIATE) starts waiting on a writer.
    let result = store.check_deadline().and_then(|()| run(&effective));
    let restore_length = store.db.set_limit(Limit::SQLITE_LIMIT_LENGTH, length);
    store.replace_deadline(prior);
    let restore_busy = store.db.busy_timeout(Duration::from_millis(busy));
    ensure!(
        restore_length.is_ok() && restore_busy.is_ok(),
        "github_app_worker_sql_policy_restore_failed"
    );
    if result
        .as_ref()
        .err()
        .is_some_and(crate::process::cleanup_unproven)
    {
        return result;
    }
    effective.check()?;
    result
}

#[cfg(test)]
mod policy_tests {
    use super::*;

    #[test]
    fn publication_policy_reinstalls_deadline_waits_and_restores_external_sql_settings() {
        let root = tempfile::tempdir().unwrap();
        let store = Store::open(root.path()).unwrap();
        for fail in [false, true] {
            store.db.busy_timeout(Duration::from_millis(777)).unwrap();
            let original_limit = store.db.limit(Limit::SQLITE_LIMIT_LENGTH).unwrap();
            let result = policy(&store, &Deadline::new(2000), |effective| {
                effective.check()?;
                let busy: u64 = store
                    .db
                    .query_row("PRAGMA busy_timeout", [], |row| row.get(0))?;
                // Installing a custom callback clears the built-in timeout.
                assert_eq!(busy, 0);
                store.db.set_limit(Limit::SQLITE_LIMIT_LENGTH, 65536)?;
                if fail {
                    anyhow::bail!("owned_policy_failure");
                }
                Ok(())
            });
            assert_eq!(result.is_err(), fail);
            assert_eq!(
                store.db.limit(Limit::SQLITE_LIMIT_LENGTH).unwrap(),
                original_limit
            );
            assert_eq!(
                store
                    .db
                    .query_row("PRAGMA busy_timeout", [], |row| row.get::<_, u64>(0))
                    .unwrap(),
                777
            );
        }
    }

    #[test]
    fn expired_store_deadline_refuses_longer_publication_before_running_sql() {
        let root = tempfile::tempdir().unwrap();
        let store = Store::open(root.path()).unwrap();
        store.set_deadline(&Deadline::new(0)).unwrap_err();
        store.db.busy_timeout(Duration::from_millis(777)).unwrap();
        let called = std::cell::Cell::new(false);
        let error = policy(&store, &Deadline::new(2000), |_| {
            called.set(true);
            Ok(())
        })
        .unwrap_err();
        assert!(error.to_string().contains("deadline"));
        assert!(!called.get());
        assert_eq!(
            store
                .db
                .query_row("PRAGMA busy_timeout", [], |row| row.get::<_, u64>(0))
                .unwrap(),
            777
        );
        assert!(store.inherited_deadline(&Deadline::new(2000)).is_err());
    }

    #[test]
    fn publication_receives_the_intersected_store_deadline_instead_of_longer_request() {
        let root = tempfile::tempdir().unwrap();
        let store = Store::open(root.path()).unwrap();
        store.set_deadline(&Deadline::new(20_000)).unwrap();
        store.db.busy_timeout(Duration::from_millis(777)).unwrap();
        let requested = Deadline::new(600_000);
        policy(&store, &requested, |effective| {
            assert!(effective.remaining() <= Duration::from_secs(20));
            assert!(effective.remaining() < requested.remaining());
            Ok(())
        })
        .unwrap();
        assert!(
            store.inherited_deadline(&requested).unwrap().remaining() <= Duration::from_secs(20)
        );
        assert_eq!(
            store
                .db
                .query_row("PRAGMA busy_timeout", [], |row| row.get::<_, u64>(0))
                .unwrap(),
            777
        );
    }
}

// Fixed slots are the only table/kind selectors. Absent proof rows are sealed too.
fn read_row(store: &Store, slot: &Slot, id: &str, d: &Deadline) -> Result<Option<Vec<u8>>> {
    d.check()?;
    let (meta_sql, body_sql) = match slot {
        Slot::Snapshot => (
            "SELECT typeof(value),length(CAST(value AS BLOB)) FROM snapshots WHERE id=?1",
            "SELECT substr(CAST(value AS BLOB),?2,65536) FROM snapshots WHERE id=?1",
        ),
        Slot::Graph => (
            "SELECT typeof(value),length(CAST(value AS BLOB)) FROM graphs WHERE id=?1",
            "SELECT substr(CAST(value AS BLOB),?2,65536) FROM graphs WHERE id=?1",
        ),
        _ => (
            "SELECT typeof(value),length(CAST(value AS BLOB)) FROM records WHERE id=?1 AND kind=?2",
            "SELECT substr(CAST(value AS BLOB),?3,65536) FROM records WHERE id=?1 AND kind=?2",
        ),
    };
    let metadata = if let Some(kind) = slot.kind() {
        store
            .db
            .query_row(meta_sql, params![id, kind], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, usize>(1)?))
            })
            .optional()?
    } else {
        store
            .db
            .query_row(meta_sql, [id], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, usize>(1)?))
            })
            .optional()?
    };
    let Some((ty, len)) = metadata else {
        return Ok(None);
    };
    ensure!(ty == "text" && len <= MAX_ROW, UNAVAILABLE);
    let mut raw = Vec::with_capacity(len);
    let mut stmt = store.db.prepare(body_sql)?;
    while raw.len() < len {
        d.check()?;
        let chunk: Vec<u8> = if let Some(kind) = slot.kind() {
            stmt.query_row(params![id, kind, raw.len() + 1], |r| r.get(0))?
        } else {
            stmt.query_row(params![id, raw.len() + 1], |r| r.get(0))?
        };
        ensure!(
            !chunk.is_empty() && raw.len().saturating_add(chunk.len()) <= len,
            UNAVAILABLE
        );
        raw.extend(chunk);
    }
    d.check()?;
    Ok(Some(raw))
}
fn digest(raw: &[u8], d: &Deadline) -> Result<String> {
    let mut digest = Sha256::new();
    for chunk in raw.chunks(65536) {
        d.check()?;
        digest.update(chunk);
    }
    Ok(hex::encode(digest.finalize()))
}
fn seal(slot: Slot, id: &str, raw: &Option<Vec<u8>>, d: &Deadline) -> Result<RowSeal> {
    Ok(RowSeal {
        slot,
        id: id.into(),
        bytes: raw.as_ref().map(Vec::len),
        hash: raw.as_ref().map(|v| digest(v, d)).transpose()?,
    })
}
fn decode<T: serde::de::DeserializeOwned>(raw: &[u8]) -> Result<T> {
    crate::json::decode(crate::json::parse(raw, MAX_ROW)?)
}
fn prepare_one(
    store: &Store,
    id: &str,
    total: &mut usize,
    d: &Deadline,
) -> Result<PreparedSnapshot> {
    let graph_raw = read_row(store, &Slot::Graph, id, d)?;
    let graph: Graph = decode(
        graph_raw
            .as_deref()
            .ok_or_else(|| anyhow::anyhow!(UNAVAILABLE))?,
    )?;
    *total = total
        .checked_add(graph_raw.as_ref().map_or(0, Vec::len))
        .ok_or_else(|| anyhow::anyhow!(CAPACITY))?;
    let mut seals = vec![seal(Slot::Graph, id, &graph_raw, d)?];
    let mut lineage = Vec::new();
    let mut next = id.to_owned();
    let mut root = None;
    loop {
        d.check()?;
        ensure!(
            lineage.len() < 16 && valid_id(&next) && !lineage.contains(&next),
            UNAVAILABLE
        );
        let raw = read_row(store, &Slot::Snapshot, &next, d)?;
        let snapshot: Snapshot =
            decode(raw.as_deref().ok_or_else(|| anyhow::anyhow!(UNAVAILABLE))?)?;
        ensure!(
            snapshot.id == next && snapshot.workspace_id == store.workspace_id,
            UNAVAILABLE
        );
        *total = total
            .checked_add(raw.as_ref().map_or(0, Vec::len))
            .ok_or_else(|| anyhow::anyhow!(CAPACITY))?;
        seals.push(seal(Slot::Snapshot, &next, &raw, d)?);
        lineage.push(next.clone());
        if root.is_none() {
            root = Some(snapshot.clone());
        }
        let mut derived = None;
        let mut overlay = None;
        for slot in Slot::slots() {
            let raw = read_row(store, &slot, &next, d)?;
            *total = total
                .checked_add(raw.as_ref().map_or(0, Vec::len))
                .ok_or_else(|| anyhow::anyhow!(CAPACITY))?;
            ensure!(*total <= MAX_METADATA, CAPACITY);
            if slot == Slot::Derived {
                derived = raw
                    .as_ref()
                    .map(|r| crate::json::parse(r, MAX_ROW))
                    .transpose()?;
            }
            if slot == Slot::Overlay {
                overlay = raw
                    .as_ref()
                    .map(|r| crate::json::parse(r, MAX_ROW))
                    .transpose()?;
            }
            seals.push(seal(slot, &next, &raw, d)?);
        }
        if let Some(derived) = derived {
            next = derived["originSnapshotId"]
                .as_str()
                .ok_or_else(|| anyhow::anyhow!(UNAVAILABLE))?
                .into();
        } else if snapshot.kind == "overlay" {
            next = overlay
                .as_ref()
                .and_then(|v| v["baseSnapshotId"].as_str())
                .ok_or_else(|| anyhow::anyhow!(UNAVAILABLE))?
                .into();
        } else {
            break;
        }
    }
    let snapshot = root.ok_or_else(|| anyhow::anyhow!(UNAVAILABLE))?;
    crate::graph_extensions::validate_scope(store, &snapshot, &graph, d)?;
    let proof = Proof {
        snapshot_id: id.into(),
        lineage,
        seals,
    };
    proof.validate()?;
    Ok(PreparedSnapshot {
        snapshot,
        graph,
        proof,
    })
}
pub(super) fn prepare(store: &Store, request: &RunRequest, d: &Deadline) -> Result<Prepared> {
    let result = (|| {
        ensure!(
            store.db.is_autocommit(),
            "github_app_worker_caller_transaction_refused"
        );
        store
            .db
            .busy_timeout(d.remaining().min(Duration::from_millis(50)))?;
        let prior_length = store.db.limit(Limit::SQLITE_LIMIT_LENGTH)?;
        store.db.set_limit(
            Limit::SQLITE_LIMIT_LENGTH,
            prior_length.min((MAX_ROW + 65536) as i32),
        )?;
        let tx = Transaction::new_unchecked(&store.db, TransactionBehavior::Deferred)?;
        let mut total = 0;
        let head = prepare_one(store, &request.head_snapshot_id, &mut total, d)?;
        let base = request
            .base_snapshot_id
            .as_ref()
            .map(|id| prepare_one(store, id, &mut total, d))
            .transpose()?;
        d.check()?;
        tx.commit()?;
        Ok(Prepared { head, base })
    })();
    available(result, d)
}
impl Proof {
    fn validate(&self) -> Result<()> {
        ensure!(
            valid_id(&self.snapshot_id)
                && (1..=16).contains(&self.lineage.len())
                && self.lineage.first() == Some(&self.snapshot_id)
                && self.lineage.iter().all(|id| valid_id(id))
                && self.lineage.iter().collect::<BTreeSet<_>>().len() == self.lineage.len(),
            BINDING
        );
        let mut expected = BTreeSet::from([(Slot::Graph, self.snapshot_id.clone())]);
        for id in &self.lineage {
            expected.insert((Slot::Snapshot, id.clone()));
            for slot in Slot::slots() {
                expected.insert((slot, id.clone()));
            }
        }
        ensure!(self.seals.len() == expected.len(), BINDING);
        let mut total = 0usize;
        for seal in &self.seals {
            ensure!(
                expected.remove(&(seal.slot.clone(), seal.id.clone())),
                BINDING
            );
            match (&seal.bytes, &seal.hash) {
                (Some(bytes), Some(hash)) => {
                    ensure!(*bytes <= MAX_ROW && valid_id(hash), BINDING);
                    total = total
                        .checked_add(*bytes)
                        .ok_or_else(|| anyhow::anyhow!(BINDING))?;
                }
                (None, None) => ensure!(seal.slot.kind().is_some(), BINDING),
                _ => anyhow::bail!(BINDING),
            }
        }
        ensure!(expected.is_empty() && total <= MAX_METADATA, BINDING);
        Ok(())
    }
    fn verify_rows(&self, store: &Store, d: &Deadline) -> Result<Snapshot> {
        self.validate()?;
        let mut root = None;
        for prior in &self.seals {
            let raw = read_row(store, &prior.slot, &prior.id, d)?;
            let actual = seal(prior.slot.clone(), &prior.id, &raw, d)?;
            ensure!(
                actual.bytes == prior.bytes && actual.hash == prior.hash,
                UNAVAILABLE
            );
            if prior.slot == Slot::Snapshot && prior.id == self.snapshot_id {
                root = Some(decode(
                    raw.as_deref().ok_or_else(|| anyhow::anyhow!(UNAVAILABLE))?,
                )?);
            }
        }
        root.ok_or_else(|| anyhow::anyhow!(UNAVAILABLE))
    }
}
fn verify_blobs(store: &Store, snapshot: &Snapshot, d: &Deadline) -> Result<()> {
    let mut total = 0usize;
    for file in &snapshot.files {
        d.check()?;
        ensure!(valid_id(&file.hash) && file.size <= 4_000_000, UNAVAILABLE);
        total = total
            .checked_add(file.size)
            .ok_or_else(|| anyhow::anyhow!(CAPACITY))?;
        ensure!(total <= 64_000_000, CAPACITY);
        let path = store.dir.join("blobs").join(&file.hash);
        let meta = fs::symlink_metadata(&path)?;
        ensure!(
            meta.is_file() && !crate::paths::is_link(&meta) && meta.len() == file.size as u64,
            UNAVAILABLE
        );
        let mut input = fs::File::open(&path)?;
        let mut digest = Sha256::new();
        let mut bytes = 0usize;
        let mut buffer = [0u8; 65536];
        loop {
            d.check()?;
            let n = input.read(&mut buffer)?;
            if n == 0 {
                break;
            }
            bytes = bytes
                .checked_add(n)
                .ok_or_else(|| anyhow::anyhow!(UNAVAILABLE))?;
            ensure!(bytes <= file.size, UNAVAILABLE);
            digest.update(&buffer[..n]);
        }
        ensure!(
            bytes == file.size
                && hex::encode(digest.finalize()) == file.hash
                && input.metadata()?.len() == meta.len(),
            UNAVAILABLE
        );
    }
    Ok(())
}
impl Prepared {
    pub(super) fn analyze(
        self,
        store: &Store,
        files: &[Value],
        job: &Job,
        provider: crate::github_app_auth::read::PullRequest,
        d: &Deadline,
    ) -> Result<Analysis> {
        let head_files: Vec<_> = files
            .iter()
            .filter(|v| v["status"] != "removed")
            .cloned()
            .collect();
        let base_files: Vec<_> = files
            .iter()
            .filter(|v| v["status"] == "removed")
            .cloned()
            .collect();
        let (mut impact, matches) = crate::github::worker_source_impact(
            store,
            &self.head.snapshot,
            &self.head.graph,
            &head_files,
            &job.head_sha,
            d,
        )?
        .into_parts();
        impact["workerCoverage"] = json!({"listedFiles":files.len(),"headFiles":head_files.len(),
            "removedFiles":base_files.len(),"removedSource":if self.base.is_some() {"explicit_base_snapshot"} else {"base_snapshot_not_supplied_removed_files_not_analyzed"},
            "providerScope":"two_matching_observations_not_atomic_remote_or_whole_tree_proof"});
        let mut bindings: Vec<Binding> = matches
            .into_iter()
            .map(|v| crate::json::decode(v.evidence()))
            .collect::<Result<_>>()?;
        let base_impact = if let Some(base) = &self.base {
            let (mut value, matches) = crate::github::worker_source_impact(
                store,
                &base.snapshot,
                &base.graph,
                &base_files,
                &job.base_sha,
                d,
            )?
            .into_parts();
            if let Some(commit) = value
                .as_object_mut()
                .and_then(|v| v.remove("reportedHeadSha"))
            {
                value["reportedBaseSha"] = commit;
            }
            if let Some(rows) = value["files"].as_array_mut() {
                for row in rows {
                    if let Some(v) = row
                        .as_object_mut()
                        .and_then(|r| r.remove("reportedHeadSha"))
                    {
                        row["reportedBaseSha"] = v;
                    }
                }
            }
            bindings.extend(
                matches
                    .into_iter()
                    .map(|v| crate::json::decode(v.evidence()))
                    .collect::<Result<Vec<Binding>>>()?,
            );
            Some(value)
        } else {
            None
        };
        ensure!(!bindings.is_empty(), UNAVAILABLE);
        Ok(Analysis {
            head: self.head.proof,
            base: self.base.map(|v| v.proof),
            bindings,
            provider,
            impact,
            base_impact,
        })
    }
}
impl Analysis {
    pub(super) fn finish(self, provider_reads: crate::github_app_auth::read::Receipt) -> Review {
        Review {
            schema_version: 1,
            head_snapshot_id: self.head.snapshot_id.clone(),
            base_snapshot_id: self.base.as_ref().map(|v| v.snapshot_id.clone()),
            head_proof: self.head,
            base_proof: self.base,
            bindings: self.bindings,
            provider: self.provider,
            provider_reads,
            impact_hash: crate::model::identity(&self.impact),
            base_impact_hash: self.base_impact.as_ref().map(crate::model::identity),
            impact: self.impact,
            base_impact: self.base_impact,
        }
    }
}
impl Review {
    pub(super) fn matches_provider(
        &self,
        provider: &crate::github_app_auth::read::PullRequest,
    ) -> bool {
        &self.provider == provider
    }
    pub(super) fn page_impacts(&self) -> (&Value, Option<&Value>) {
        (&self.impact, self.base_impact.as_ref())
    }
    /// Whitelisted display facts only. The returned JSON is never an authority handle.
    pub(super) fn publication_facts(&self) -> Value {
        let section = |impact: &Value| {
            json!({
                "changedSymbols": impact["changedSymbols"].as_array().map_or(0, Vec::len),
                "incomingCandidates": impact["incomingCandidates"].as_array().map_or(0, Vec::len),
                "observations": impact["observations"].as_array().map_or(0, Vec::len),
                "files": impact["files"].as_array().into_iter().flatten().map(|file|
                    json!({"path":file["path"],"sourceMatch":file["sourceMatch"],
                        "hunksVerified":file["hunksVerified"],"reason":file["reason"]})).collect::<Vec<_>>(),
                "omissions": impact["omissions"].as_array().into_iter().flatten().map(|row|
                    json!({"path":row["path"],"reason":row["reason"]})).collect::<Vec<_>>(),
            })
        };
        json!({"headSnapshotId":self.head_snapshot_id,"baseSnapshotId":self.base_snapshot_id,
            "bindings":self.bindings,"head":section(&self.impact),
            "base":self.base_impact.as_ref().map(section),
            "coverage":self.impact["workerCoverage"]})
    }
    pub(super) fn validate(&self, request: &RunRequest, job: &Job, d: &Deadline) -> Result<()> {
        d.check()?;
        self.head_proof.validate()?;
        if let Some(base) = &self.base_proof {
            base.validate()?;
        }
        let p = &self.provider_reads;
        ensure!(
            self.schema_version == 1
                && self.head_snapshot_id == request.head_snapshot_id
                && self.base_snapshot_id == request.base_snapshot_id
                && self.head_proof.snapshot_id == self.head_snapshot_id
                && self.base_proof.as_ref().map(|v| &v.snapshot_id)
                    == self.base_snapshot_id.as_ref()
                && self.provider.number == job.number
                && self.provider.head_sha == job.head_sha
                && self.provider.base_sha == job.base_sha
                && self.provider.changed_files <= 3000
                && p.app_id == job.app_id
                && p.installation_id == job.installation_id
                && p.repository_id == job.repository_id
                && p.repository == job.repository
                && p.number == job.number
                && p.head_sha == job.head_sha
                && p.base_sha == job.base_sha
                && p.api_version == "2026-03-10"
                && p.requests == 3 + self.provider.changed_files.div_ceil(100)
                && p.response_bytes <= 64_000_000
                && (60..=253_402_296_000).contains(&p.initial_observed_at_unix)
                && (p.initial_observed_at_unix..=253_402_296_000)
                    .contains(&p.final_observed_at_unix)
                && (1..=3000).contains(&self.bindings.len()),
            BINDING
        );
        let mut unique = BTreeSet::new();
        for b in &self.bindings {
            d.check()?;
            ensure!(
                (b.snapshot_id == self.head_snapshot_id
                    || self.base_snapshot_id.as_ref() == Some(&b.snapshot_id))
                    && crate::paths::normalized_syntax(&b.path)? == b.path
                    && valid_id(&b.content_hash)
                    && b.git_blob_sha.len() == 40
                    && b.git_blob_sha
                        .bytes()
                        .all(|v| v.is_ascii_digit() || (b'a'..=b'f').contains(&v))
                    && b.start_byte == 0
                    && b.end_byte <= 4_000_000
                    && unique.insert((&b.snapshot_id, &b.path)),
                BINDING
            );
        }
        let raw = crate::json::encode_bounded(&self.impact, 3_000_000, d)?;
        ensure!(
            hash(raw) == self.impact_hash
                && self.base_impact_hash == self.base_impact.as_ref().map(crate::model::identity),
            BINDING
        );
        if let Some(base) = &self.base_impact {
            crate::json::encode_bounded(base, 3_000_000, d)?;
        }
        Ok(())
    }
    /// Freshness of this selected report only; status/history replay does not rehash old datasets.
    pub(super) fn verify_source(
        &self,
        store: &Store,
        full_bytes: bool,
        d: &Deadline,
    ) -> Result<()> {
        let head = self.head_proof.verify_rows(store, d)?;
        let base = self
            .base_proof
            .as_ref()
            .map(|p| p.verify_rows(store, d))
            .transpose()?;
        for b in &self.bindings {
            let snapshot = if b.snapshot_id == head.id {
                &head
            } else {
                base.as_ref().ok_or_else(|| anyhow::anyhow!(BINDING))?
            };
            ensure!(
                snapshot.workspace_id == store.workspace_id
                    && snapshot.files.iter().any(|f| f.path == b.path
                        && f.hash == b.content_hash
                        && f.size == b.end_byte),
                BINDING
            );
        }
        if full_bytes {
            available(verify_blobs(store, &head, d), d)?;
            if let Some(base) = &base {
                available(verify_blobs(store, base, d), d)?;
            }
        }
        Ok(())
    }
    pub(super) fn roots(&self) -> impl Iterator<Item = &String> {
        std::iter::once(&self.head_snapshot_id).chain(self.base_snapshot_id.iter())
    }
}
#[derive(Default)]
pub(super) struct Retained {
    snapshots: BTreeMap<String, Snapshot>,
    bytes: usize,
}
impl Retained {
    /// Metadata relationships only. Full source/graph attestation belongs to active use.
    pub(super) fn check(&mut self, store: &Store, review: &Review, d: &Deadline) -> Result<()> {
        for id in review.roots() {
            if !self.snapshots.contains_key(id) {
                let raw = read_row(store, &Slot::Snapshot, id, d)?
                    .ok_or_else(|| anyhow::anyhow!(BINDING))?;
                self.bytes = self
                    .bytes
                    .checked_add(raw.len())
                    .ok_or_else(|| anyhow::anyhow!(CAPACITY))?;
                ensure!(self.bytes <= MAX_METADATA, CAPACITY);
                let snapshot: Snapshot = decode(&raw)?;
                ensure!(
                    snapshot.id == *id && snapshot.workspace_id == store.workspace_id,
                    BINDING
                );
                self.snapshots.insert(id.clone(), snapshot);
            }
        }
        for b in &review.bindings {
            d.check()?;
            ensure!(
                self.snapshots
                    .get(&b.snapshot_id)
                    .is_some_and(|s| s.files.iter().any(|f| f.path == b.path
                        && f.hash == b.content_hash
                        && f.size == b.end_byte)),
                BINDING
            );
        }
        Ok(())
    }
}
