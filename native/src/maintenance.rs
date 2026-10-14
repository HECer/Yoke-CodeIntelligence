//! Conservative host-authorized maintenance of immutable local state.
use crate::{capture::Deadline, model::valid_id, paths::is_link, store::Store};
use anyhow::{Context, Result, ensure};
use rusqlite::{OptionalExtension, Transaction, TransactionBehavior, params};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeSet,
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

#[derive(Clone, Debug)]
pub struct Policy {
    pub apply: bool,
    pub minimum_age_ms: u64,
    pub purge_quarantine: bool,
}
impl Default for Policy {
    fn default() -> Self {
        Self {
            apply: false,
            minimum_age_ms: 86_400_000,
            purge_quarantine: false,
        }
    }
}
pub fn collect(store: &Store, policy: &Policy, deadline: &Deadline) -> Result<Value> {
    ensure!(
        !policy.purge_quarantine,
        "Irreversible quarantine purge is unavailable; bytes are preserved"
    );
    ensure!(
        policy.minimum_age_ms <= 365 * 86_400_000,
        "Maintenance age exceeds the supported limit"
    );
    deadline.check()?;
    store.with_exclusive_client(deadline, || collect_exclusive(store, policy, deadline))
}
pub fn pin(
    store: &Store,
    snapshot: &str,
    id: &str,
    reason: &str,
    deadline: &Deadline,
) -> Result<()> {
    pin_id(id)?;
    ensure!(reason.len() <= 1000, "Pin reason is too long");
    deadline.check()?;
    store.set_deadline(deadline)?;
    store.snapshot(snapshot)?;
    store.record(
        "snapshot-pin",
        id,
        &json!({"snapshotId":snapshot,"reason":reason}),
    )
}
pub fn unpin(store: &Store, id: &str, deadline: &Deadline) -> Result<()> {
    pin_id(id)?;
    store.set_deadline(deadline)?;
    deadline.check()?;
    store.db.execute(
        "DELETE FROM records WHERE kind='snapshot-pin' AND id=?1",
        [id],
    )?;
    Ok(())
}
fn pin_id(id: &str) -> Result<()> {
    ensure!(
        !id.is_empty()
            && id.len() <= 200
            && id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"_.-".contains(&b)),
        "Invalid snapshot pin ID"
    );
    Ok(())
}
fn directory(path: &Path, create: bool) -> Result<bool> {
    match fs::symlink_metadata(path) {
        Ok(meta) => {
            ensure!(
                meta.is_dir() && !is_link(&meta),
                "Unsafe maintenance directory"
            );
            Ok(true)
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            if create {
                fs::create_dir(path)?;
                directory(path, false)?;
            }
            Ok(create)
        }
        Err(error) => Err(error.into()),
    }
}
fn leaf(path: &Path) -> Result<fs::Metadata> {
    let meta = fs::symlink_metadata(path)?;
    ensure!(meta.is_file() && !is_link(&meta), "Unsafe maintenance file");
    Ok(meta)
}
fn checked_hash(path: &Path, expected: &str, deadline: &Deadline) -> Result<()> {
    deadline.check()?;
    leaf(path)?;
    let mut file = fs::File::open(path)?;
    let mut digest = Sha256::new();
    let mut bytes = [0u8; 65536];
    loop {
        deadline.check()?;
        let count = file.read(&mut bytes)?;
        if count == 0 {
            break;
        }
        digest.update(&bytes[..count]);
    }
    ensure!(
        hex::encode(digest.finalize()) == expected,
        "Maintenance content hash mismatch"
    );
    Ok(())
}
fn references(value: &Value, marked: &mut BTreeSet<String>, deadline: &Deadline) -> Result<()> {
    let mut queue = vec![value];
    let mut nodes = 0usize;
    while let Some(value) = queue.pop() {
        nodes += 1;
        ensure!(
            nodes <= 2_000_000,
            "Maintenance reference capacity exceeded"
        );
        if nodes.is_multiple_of(512) {
            deadline.check()?;
        }
        match value {
            Value::String(value) if valid_id(value) => {
                marked.insert(value.clone());
            }
            Value::Array(values) => queue.extend(values),
            Value::Object(values) => {
                for (key, value) in values {
                    if valid_id(key) {
                        marked.insert(key.clone());
                    }
                    queue.push(value);
                }
            }
            _ => {}
        }
    }
    Ok(())
}
#[derive(Default)]
struct Roots {
    marked: BTreeSet<String>,
    snapshots: usize,
    graphs: usize,
    caches: usize,
    records: usize,
}
fn roots(store: &Store, deadline: &Deadline) -> Result<Roots> {
    let mut tables = store
        .db
        .prepare("SELECT name FROM sqlite_master WHERE type='table'")?;
    for table in tables.query_map([], |row| row.get::<_, String>(0))? {
        let table = table?;
        let known = [
            "meta",
            "snapshots",
            "graphs",
            "cache",
            "records",
            "symbols",
            "documents",
        ]
        .contains(&table.as_str())
            || table.starts_with("sqlite_")
            || ["symbols", "documents"].iter().any(|prefix| {
                ["data", "idx", "content", "docsize", "config"]
                    .iter()
                    .any(|suffix| table == format!("{prefix}_{suffix}"))
            });
        ensure!(known, "Unknown storage table; maintenance refused");
    }
    let mut out = Roots::default();
    for table in ["snapshots", "graphs", "cache", "records"] {
        let key_column = if table == "cache" { "key" } else { "id" };
        let mut statement = store
            .db
            .prepare(&format!("SELECT {key_column},value FROM {table}"))?;
        let mut count = 0usize;
        for row in statement.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })? {
            deadline.check()?;
            count += 1;
            ensure!(count <= 200_000, "Maintenance row capacity exceeded");
            let (key, text) = row.context("Corrupt persisted metadata; maintenance refused")?;
            if valid_id(&key) {
                out.marked.insert(key);
            }
            ensure!(
                text.len() <= 128_000_000,
                "Persisted metadata exceeds maintenance capacity"
            );
            let value: Value = serde_json::from_str(&text)
                .map_err(|_| anyhow::anyhow!("Corrupt persisted metadata; maintenance refused"))?;
            let valid_shape = match table {
                "snapshots" => {
                    serde_json::from_str::<crate::model::Snapshot>(&text).is_ok_and(|snapshot| {
                        valid_id(&snapshot.id)
                            && snapshot.files.iter().all(|file| valid_id(&file.hash))
                    })
                }
                "graphs" => serde_json::from_str::<crate::model::Graph>(&text)
                    .is_ok_and(|graph| valid_id(&graph.snapshot_id)),
                "cache" => serde_json::from_str::<crate::model::Extraction>(&text).is_ok(),
                _ => true,
            };
            ensure!(
                valid_shape,
                "Corrupt persisted metadata shape; maintenance refused"
            );
            references(&value, &mut out.marked, deadline)?;
        }
        match table {
            "snapshots" => out.snapshots = count,
            "graphs" => out.graphs = count,
            "cache" => out.caches = count,
            _ => out.records = count,
        }
    }
    if let Some(latest) = store.latest()? {
        out.marked.insert(latest);
    }
    Ok(out)
}
#[derive(Clone)]
struct Candidate {
    id: String,
    bytes: u64,
}
fn candidates(
    store: &Store,
    roots: &Roots,
    age: u64,
    deadline: &Deadline,
) -> Result<(Vec<Candidate>, usize, usize, usize)> {
    let mut candidates = Vec::new();
    let mut protected = 0;
    let mut unsupported = 0;
    let mut unreferenced = 0;
    let now = SystemTime::now();
    let blob_root = store.dir.join("blobs");
    directory(&blob_root, false)?;
    let mut count = 0usize;
    for item in fs::read_dir(&blob_root)? {
        deadline.check()?;
        count += 1;
        ensure!(count <= 200_000, "Maintenance blob capacity exceeded");
        let item = item?;
        let meta = fs::symlink_metadata(item.path())?;
        ensure!(
            !is_link(&meta),
            "Blob link/reparse entry refused during maintenance"
        );
        let Some(id) = item.file_name().to_str().map(str::to_owned) else {
            unsupported += 1;
            continue;
        };
        if !valid_id(&id) || !meta.is_file() {
            unsupported += 1;
            continue;
        }
        if roots.marked.contains(&id) {
            protected += 1;
            continue;
        }
        unreferenced += 1;
        let Ok(elapsed) = now.duration_since(meta.modified()?) else {
            continue;
        };
        if elapsed.as_millis() >= u128::from(age) {
            candidates.push(Candidate {
                id,
                bytes: meta.len(),
            });
        }
    }
    candidates.sort_by(|a, b| a.id.cmp(&b.id));
    Ok((candidates, protected, unsupported, unreferenced))
}
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Manifest {
    schema_version: u32,
    id: String,
    workspace_id: String,
    created_ms: u64,
    blob_ids: Vec<String>,
}
struct Run {
    manifest: Manifest,
    path: PathBuf,
    committed: bool,
    restored: bool,
}
fn runs(store: &Store, deadline: &Deadline) -> Result<Vec<Run>> {
    let root = store.dir.join("quarantine");
    if !directory(&root, false)? {
        return Ok(Vec::new());
    }
    let mut out = Vec::new();
    for item in fs::read_dir(&root)? {
        deadline.check()?;
        ensure!(out.len() < 10_000, "Quarantine run capacity exceeded");
        let item = item?;
        let name = item.file_name();
        let id = name
            .to_str()
            .ok_or_else(|| anyhow::anyhow!("Invalid quarantine run name"))?;
        ensure!(
            valid_id(id),
            "Unknown quarantine entry; maintenance refused"
        );
        directory(&item.path(), false)?;
        let path = item.path().join("manifest.json");
        let meta = leaf(&path)?;
        ensure!(
            meta.len() <= 16 * 1024 * 1024,
            "Quarantine manifest is too large"
        );
        let manifest: Manifest = serde_json::from_slice(&fs::read(&path)?)
            .map_err(|_| anyhow::anyhow!("Corrupt quarantine manifest; maintenance refused"))?;
        ensure!(
            manifest.schema_version == 1
                && manifest.id == id
                && manifest.workspace_id == store.workspace_id
                && manifest.blob_ids.len() <= 200_000
                && manifest.blob_ids.iter().all(|id| valid_id(id)),
            "Invalid quarantine manifest identity"
        );
        let unique: BTreeSet<_> = manifest.blob_ids.iter().collect();
        ensure!(
            unique.len() == manifest.blob_ids.len(),
            "Duplicate quarantine blob declaration"
        );
        for entry in fs::read_dir(item.path())? {
            let entry = entry?;
            leaf(&entry.path())?;
            let entry_name = entry.file_name();
            let text = entry_name
                .to_str()
                .ok_or_else(|| anyhow::anyhow!("Invalid quarantine file name"))?;
            ensure!(
                text == "manifest.json" || manifest.blob_ids.iter().any(|id| id == text),
                "Undeclared quarantine file; maintenance refused"
            );
        }
        let committed = store
            .db
            .query_row(
                "SELECT 1 FROM records WHERE kind='maintenance-committed' AND id=?1",
                [id],
                |_| Ok(()),
            )
            .optional()?
            .is_some();
        let restored = store
            .db
            .query_row(
                "SELECT 1 FROM records WHERE kind='maintenance-restored' AND id=?1",
                [id],
                |_| Ok(()),
            )
            .optional()?
            .is_some();
        out.push(Run {
            manifest,
            path: item.path(),
            committed,
            restored,
        });
    }
    out.sort_by(|a, b| a.manifest.id.cmp(&b.manifest.id));
    Ok(out)
}
fn restore(
    store: &Store,
    run: &Run,
    marked: &BTreeSet<String>,
    deadline: &Deadline,
) -> Result<usize> {
    let mut count = 0;
    for id in &run.manifest.blob_ids {
        deadline.check()?;
        let source = run.path.join(id);
        let destination = store.dir.join("blobs").join(id);
        if !source.try_exists()? {
            continue;
        }
        if run.committed && !marked.contains(id) {
            continue;
        }
        checked_hash(&source, id, deadline)?;
        if destination.try_exists()? {
            checked_hash(&destination, id, deadline)?;
            continue;
        }
        fs::rename(&source, &destination)?;
        count += 1;
    }
    if !run.committed && !run.restored {
        store.record(
            "maintenance-restored",
            &run.manifest.id,
            &json!({"schemaVersion":1,"workspaceId":store.workspace_id,"restoredBlobs":count}),
        )?;
    }
    Ok(count)
}
fn rollback(store: &Store, run: &Path, moved: &[String], deadline: &Deadline) -> Result<()> {
    for id in moved.iter().rev() {
        deadline.check()?;
        let source = run.join(id);
        let destination = store.dir.join("blobs").join(id);
        checked_hash(&source, id, deadline)?;
        if destination.try_exists()? {
            checked_hash(&destination, id, deadline)?;
            continue;
        }
        deadline.check()?;
        fs::rename(source, destination)?;
    }
    Ok(())
}
fn apply(store: &Store, items: &[Candidate], deadline: &Deadline) -> Result<Option<PathBuf>> {
    if items.is_empty() {
        return Ok(None);
    }
    let quarantine = store.dir.join("quarantine");
    directory(&quarantine, true)?;
    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes).map_err(|_| anyhow::anyhow!("System randomness unavailable"))?;
    let id = hex::encode(bytes);
    let run = quarantine.join(&id);
    fs::create_dir(&run)?;
    let manifest = Manifest {
        schema_version: 1,
        id: id.clone(),
        workspace_id: store.workspace_id.clone(),
        created_ms: SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis() as u64,
        blob_ids: items.iter().map(|item| item.id.clone()).collect(),
    };
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(run.join("manifest.json"))?;
    file.write_all(&serde_json::to_vec(&manifest)?)?;
    file.sync_all()?;
    drop(file);
    let mut moved = Vec::new();
    let mut committed = false;
    let result = (|| -> Result<()> {
        store.set_deadline(deadline)?;
        let transaction = Transaction::new_unchecked(&store.db, TransactionBehavior::Immediate)?;
        for item in items {
            deadline.check()?;
            let source = store.dir.join("blobs").join(&item.id);
            checked_hash(&source, &item.id, deadline)?;
            let destination = run.join(&item.id);
            ensure!(
                !destination.try_exists()?,
                "Quarantine target already exists"
            );
            deadline.check()?;
            fs::rename(source, destination)?;
            moved.push(item.id.clone());
        }
        deadline.check()?;
        transaction.execute("INSERT INTO records VALUES('maintenance-committed',?1,?2)",params![id,serde_json::to_string(&json!({"schemaVersion":1,"workspaceId":store.workspace_id,"quarantinedBlobs":items.len()}))?])?;
        deadline.check()?;
        store.set_deadline(deadline)?;
        transaction.commit()?;
        // COMMIT is the completion point. The pre-commit check and SQLite deadline
        // bound the transaction; a subsequent clock check must not undo a complete
        // quarantine merely because the clock advanced after that point.
        committed = true;
        Ok(())
    })();
    if let Err(error) = result {
        let cleanup = Deadline::new(1000);
        // Every error currently precedes COMMIT. Keep the guard explicit so future
        // post-commit work cannot introduce an unconditional, repeated busy wait.
        if committed {
            store.set_deadline(&cleanup)?;
            store.db.busy_timeout(Duration::ZERO)?;
            store
                .db
                .execute(
                    "DELETE FROM records WHERE kind='maintenance-committed' AND id=?1",
                    [&id],
                )
                .with_context(|| {
                    format!(
                        "Committed quarantine run {id} retained; metadata cleanup needs recovery"
                    )
                })?;
        }
        if rollback(store, &run, &moved, &cleanup).is_err() {
            return Err(error).with_context(|| format!(
                "Maintenance failed; rollback budget exhausted or restoration failed; prepared quarantine run {id} retained for recovery"
            ));
        }
        return Err(error).with_context(|| {
            format!("Maintenance failed; prepared quarantine run {id} retained for recovery")
        });
    }
    Ok(Some(run))
}

fn collect_exclusive(store: &Store, policy: &Policy, deadline: &Deadline) -> Result<Value> {
    deadline.check()?;
    directory(&store.dir, false)?;
    directory(&store.dir.join("blobs"), false)?;
    store.set_deadline(deadline)?;
    let roots = roots(store, deadline)?;
    let runs = runs(store, deadline)?;
    let pending = runs
        .iter()
        .filter(|run| !run.committed && !run.restored)
        .count();
    let mut restored = 0;
    if policy.apply {
        for run in &runs {
            restored += restore(store, run, &roots.marked, deadline)?;
        }
    }
    let (items, protected, unsupported, unreferenced) =
        candidates(store, &roots, policy.minimum_age_ms, deadline)?;
    let bytes = items
        .iter()
        .try_fold(0u64, |sum, item| sum.checked_add(item.bytes))
        .context("Maintenance byte count overflow")?;
    let path = if policy.apply {
        apply(store, &items, deadline)?
    } else {
        deadline.check()?;
        None
    };
    Ok(
        json!({"applied":policy.apply,"retainedSnapshots":roots.snapshots,"retiredSnapshots":0,"retainedGraphs":roots.graphs,"retainedCacheEntries":roots.caches,"retainedRecords":roots.records,"protectedBlobs":protected,"unreferencedBlobs":unreferenced,"eligibleUnreferencedBlobs":items.len(),"deferredYoungBlobs":unreferenced-items.len(),"eligibleBytes":bytes,"quarantinedBlobs":if policy.apply{items.len()}else{0},"quarantinedBytes":if policy.apply{bytes}else{0},"reclaimedBytes":0,"quarantinePath":path,"pendingRecoveryRuns":pending,"restoredBlobs":restored,"unsupportedBlobEntries":unsupported,"scope":"exclusive_local_store","snapshotRetirementSupported":false,"irreversibleRemovalSupported":false,"limitations":["All graph, snapshot, cache and generic record roots are retained regardless of apparent age or status.","Quarantine preserves bytes; it does not reclaim physical disk space. Irreversible purge and history retirement require a later explicit audited workflow."]}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expired_rollback_budget_preserves_quarantine_for_later_recovery() {
        let dir = tempfile::TempDir::new().unwrap();
        let engine = crate::Engine::open(dir.path()).unwrap();
        let id = engine.store.blob(b"bounded cleanup orphan").unwrap();
        let run_id = crate::model::hash(b"bounded cleanup run");
        let run = engine.store.dir.join("quarantine").join(&run_id);
        fs::create_dir_all(&run).unwrap();
        let manifest = Manifest {
            schema_version: 1,
            id: run_id,
            workspace_id: engine.store.workspace_id.clone(),
            created_ms: 0,
            blob_ids: vec![id.clone()],
        };
        fs::write(
            run.join("manifest.json"),
            serde_json::to_vec(&manifest).unwrap(),
        )
        .unwrap();
        fs::rename(engine.store.dir.join("blobs").join(&id), run.join(&id)).unwrap();
        assert!(
            rollback(
                &engine.store,
                &run,
                std::slice::from_ref(&id),
                &Deadline::new(0)
            )
            .is_err()
        );
        assert_eq!(fs::read(run.join(&id)).unwrap(), b"bounded cleanup orphan");
        assert!(engine.store.read_blob(&id).is_err());
        let report = collect(
            &engine.store,
            &Policy {
                apply: true,
                minimum_age_ms: 365 * 86_400_000,
                purge_quarantine: false,
            },
            &Deadline::new(10000),
        )
        .unwrap();
        assert_eq!(report["restoredBlobs"], 1);
        assert_eq!(
            engine.store.read_blob(&id).unwrap(),
            b"bounded cleanup orphan"
        );
    }
}
