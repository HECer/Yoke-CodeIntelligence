use crate::{
    model::{Extraction, Graph, Snapshot, hash, valid_id},
    paths::is_link,
};
use anyhow::{Context, Result, ensure};
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Serialize, de::DeserializeOwned};
use sha2::{Digest, Sha256};
use std::{
    cell::{Cell, RefCell},
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

#[path = "store_busy.rs"]
mod busy;
#[path = "store_read_only.rs"]
mod read_only;

pub struct Store {
    pub dir: PathBuf,
    pub db: Connection,
    pub workspace_id: String,
    deadline: RefCell<Option<crate::capture::Deadline>>,
    /// Invocation-owned policy; never reconstructed from stored source or plans.
    pub(crate) source_line_ending: Option<crate::source_line_endings::ResolvedLineEnding>,
    client_lock: fs::File,
    admission_gate: fs::File,
    client_valid: Cell<bool>,
    maintenance_active: Cell<bool>,
}
pub struct ArtifactPage {
    pub bytes: Vec<u8>,
    pub total_bytes: usize,
}
fn state_lock(dir: &Path, name: &str) -> Result<fs::File> {
    let path = dir.join(name);
    if let Ok(meta) = fs::symlink_metadata(&path) {
        ensure!(
            !is_link(&meta) && meta.is_file(),
            "State lock must be a regular local file"
        );
    }
    Ok(fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(path)?)
}
fn lock_is_contended(error: &std::io::Error) -> bool {
    error.kind() == std::io::ErrorKind::WouldBlock
        || error
            .raw_os_error()
            .is_some_and(|code| Some(code) == fs2::lock_contended_error().raw_os_error())
}
fn lock_until(
    file: &fs::File,
    exclusive: bool,
    end: Instant,
    deadline: Option<&crate::capture::Deadline>,
) -> Result<()> {
    loop {
        if let Some(deadline) = deadline {
            deadline.check()?;
        }
        ensure!(Instant::now() < end, "State client lock deadline exceeded");
        let result = if exclusive {
            fs2::FileExt::try_lock_exclusive(file)
        } else {
            fs2::FileExt::try_lock_shared(file)
        };
        match result {
            Ok(()) => return Ok(()),
            Err(error) if lock_is_contended(&error) => {
                std::thread::sleep(
                    end.saturating_duration_since(Instant::now())
                        .min(Duration::from_millis(5)),
                );
            }
            Err(error) => return Err(error.into()),
        }
    }
}
struct ClientUpgrade<'a> {
    store: &'a Store,
    gate_locked: bool,
    restore_client: bool,
    exclusive: bool,
}
impl ClientUpgrade<'_> {
    fn restore(&mut self) -> Result<()> {
        if self.restore_client {
            if self.exclusive {
                fs2::FileExt::unlock(&self.store.client_lock)?;
                self.exclusive = false;
            }
            // Admission is still exclusively held: existing clients can only
            // hold shared leases and new clients/upgrades cannot race this.
            fs2::FileExt::lock_shared(&self.store.client_lock)?;
            self.store.client_valid.set(true);
            self.restore_client = false;
        }
        if self.gate_locked {
            fs2::FileExt::unlock(&self.store.admission_gate)?;
            self.gate_locked = false;
        }
        self.store.maintenance_active.set(false);
        Ok(())
    }
}
impl Drop for ClientUpgrade<'_> {
    fn drop(&mut self) {
        if self.restore().is_err() {
            self.store.client_valid.set(false);
        }
    }
}
impl Store {
    pub fn with_exclusive_client<T>(
        &self,
        deadline: &crate::capture::Deadline,
        operation: impl FnOnce() -> Result<T>,
    ) -> Result<T> {
        ensure!(
            self.client_valid.get() && !self.maintenance_active.get(),
            "State client lease is unavailable or already exclusive"
        );
        self.set_deadline(deadline)?;
        lock_until(
            &self.admission_gate,
            true,
            Instant::now() + deadline.remaining(),
            Some(deadline),
        )?;
        self.maintenance_active.set(true);
        let mut upgrade = ClientUpgrade {
            store: self,
            gate_locked: true,
            restore_client: false,
            exclusive: false,
        };
        deadline.check()?;
        fs2::FileExt::unlock(&self.client_lock)?;
        self.client_valid.set(false);
        upgrade.restore_client = true;
        fs2::FileExt::try_lock_exclusive(&self.client_lock)
            .context("Maintenance requires all other state clients to close")?;
        upgrade.exclusive = true;
        self.client_valid.set(true);
        deadline.check()?;
        let result = operation();
        upgrade.restore()?;
        result
    }
    pub fn open(root: &Path) -> Result<Self> {
        Self::open_with_timeout(root, Duration::from_secs(10))
    }
    pub fn open_with_timeout(root: &Path, timeout: Duration) -> Result<Self> {
        let deadline = crate::capture::Deadline::from_duration(timeout);
        deadline.check()?;
        let dir = root.join(".yoke");
        if let Ok(meta) = fs::symlink_metadata(&dir) {
            ensure!(!is_link(&meta), "State directory must not be a link");
        }
        fs::create_dir_all(&dir)?;
        for name in ["blobs", "trees"] {
            deadline.check()?;
            let path = dir.join(name);
            if let Ok(meta) = fs::symlink_metadata(&path) {
                ensure!(!is_link(&meta), "State subdirectory must not be a link");
            }
            fs::create_dir_all(path)?;
        }
        deadline.check()?;
        let admission_gate = state_lock(&dir, "maintenance.lock")?;
        let client_lock = state_lock(&dir, "clients.lock")?;
        let end = Instant::now() + deadline.remaining();
        lock_until(&admission_gate, false, end, Some(&deadline))?;
        lock_until(&client_lock, false, end, Some(&deadline))?;
        fs2::FileExt::unlock(&admission_gate)?;
        let db_path = dir.join("index.sqlite");
        for name in [
            "index.sqlite",
            "index.sqlite-wal",
            "index.sqlite-shm",
            "index.sqlite-journal",
        ] {
            deadline.check()?;
            if let Ok(meta) = fs::symlink_metadata(dir.join(name)) {
                ensure!(
                    !is_link(&meta),
                    "Database or SQLite sidecar must not be a link"
                );
            }
        }
        deadline.check()?;
        let db = Connection::open(db_path)?;
        busy::install(&db, Some(&deadline))?;
        let bounded = deadline.clone();
        db.progress_handler(1000, Some(move || bounded.check().is_err()));
        db.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON; PRAGMA synchronous=FULL;
            CREATE TABLE IF NOT EXISTS meta(key TEXT PRIMARY KEY,value TEXT NOT NULL);
            CREATE TABLE IF NOT EXISTS snapshots(id TEXT PRIMARY KEY,value TEXT NOT NULL);
            CREATE TABLE IF NOT EXISTS graphs(id TEXT PRIMARY KEY,value TEXT NOT NULL);
            CREATE TABLE IF NOT EXISTS cache(key TEXT PRIMARY KEY,value TEXT NOT NULL);
            CREATE TABLE IF NOT EXISTS records(kind TEXT NOT NULL,id TEXT NOT NULL,value TEXT NOT NULL,PRIMARY KEY(kind,id));
            CREATE VIRTUAL TABLE IF NOT EXISTS symbols USING fts5(snapshot UNINDEXED,id UNINDEXED,name,path UNINDEXED,signature);
            CREATE VIRTUAL TABLE IF NOT EXISTS documents USING fts5(snapshot UNINDEXED,path UNINDEXED,text);")?;
        deadline.check()?;
        let workspace_id: Option<String> = db
            .query_row("SELECT value FROM meta WHERE key='workspace'", [], |r| {
                r.get(0)
            })
            .optional()?;
        let workspace_id = match workspace_id {
            Some(id) => id,
            None => {
                let random = tempfile::Builder::new()
                    .prefix("yoke-identity-")
                    .tempfile()?;
                let id = hash(format!(
                    "{}:{:?}:{:?}",
                    root.display(),
                    std::time::SystemTime::now(),
                    random.path()
                ));
                deadline.check()?;
                db.execute("INSERT OR IGNORE INTO meta VALUES('workspace',?1)", [&id])?;
                db.query_row("SELECT value FROM meta WHERE key='workspace'", [], |r| {
                    r.get(0)
                })?
            }
        };
        deadline.check()?;
        // The setup budget bounds this opening, not future unrelated operations
        // on the returned Store. Those select their own checked request policy.
        db.progress_handler(0, None::<fn() -> bool>);
        busy::install(&db, None)?;
        Ok(Self {
            dir,
            db,
            workspace_id,
            deadline: RefCell::new(None),
            source_line_ending: None,
            client_lock,
            admission_gate,
            client_valid: Cell::new(true),
            maintenance_active: Cell::new(false),
        })
    }
    pub fn blob(&self, bytes: &[u8]) -> Result<String> {
        let id = hash(bytes);
        let path = self.dir.join("blobs").join(&id);
        if path.exists() {
            ensure!(
                self.read_blob_exact(&id, bytes.len(), 512_000_000)? == bytes,
                "Existing content blob is corrupt"
            );
            return Ok(id);
        }
        let mut temporary = tempfile::NamedTempFile::new_in(self.dir.join("blobs"))?;
        temporary.write_all(bytes)?;
        temporary.as_file().sync_all()?;
        if let Err(error) = temporary.persist_noclobber(&path) {
            if !path.exists() {
                return Err(error.error.into());
            }
            ensure!(
                self.read_blob_exact(&id, bytes.len(), 512_000_000)? == bytes,
                "Concurrent blob corruption"
            );
        }
        Ok(id)
    }
    pub fn read_blob(&self, id: &str) -> Result<Vec<u8>> {
        ensure!(valid_id(id), "Invalid content ID");
        let path = self.dir.join("blobs").join(id);
        let metadata = fs::symlink_metadata(&path)?;
        let size = usize::try_from(metadata.len()).context("Blob size overflow")?;
        self.read_blob_exact(id, size, 512_000_000)
    }
    /// Admit a verified expected size before allocating bytes. This is not a source-scope grant.
    pub fn read_blob_exact(&self, id: &str, expected: usize, maximum: usize) -> Result<Vec<u8>> {
        self.check_deadline()?;
        ensure!(
            valid_id(id) && expected <= maximum,
            "Invalid or oversized content read"
        );
        let path = self.dir.join("blobs").join(id);
        let metadata = fs::symlink_metadata(&path)?;
        ensure!(
            metadata.is_file() && !is_link(&metadata) && metadata.len() == expected as u64,
            "Content file type or expected size mismatch"
        );
        let mut file = fs::File::open(path)?;
        let opened = file.metadata()?;
        ensure!(
            opened.is_file() && !is_link(&opened) && opened.len() == expected as u64,
            "Opened content size mismatch"
        );
        let mut bytes = Vec::with_capacity(expected);
        let mut buffer = [0u8; 64000];
        loop {
            self.check_deadline()?;
            let remaining = expected.saturating_sub(bytes.len()).saturating_add(1);
            let maximum = remaining.min(buffer.len());
            let count = file.read(&mut buffer[..maximum])?;
            if count == 0 {
                break;
            }
            ensure!(
                count <= expected.saturating_sub(bytes.len()),
                "Content grew beyond admitted size"
            );
            bytes.extend_from_slice(&buffer[..count]);
        }
        ensure!(bytes.len() == expected, "Content size changed during read");
        ensure!(hash(&bytes) == id, "Content hash mismatch");
        self.check_deadline()?;
        Ok(bytes)
    }
    pub fn artifact(&self, bytes: &[u8]) -> Result<String> {
        let id = self.blob(bytes)?;
        self.record(
            "artifact",
            &id,
            &serde_json::json!({"artifactId":id,"totalBytes":bytes.len()}),
        )?;
        Ok(id)
    }
    pub fn artifact_size(&self, id: &str) -> Result<usize> {
        self.check_deadline()?;
        ensure!(valid_id(id), "Invalid artifact ID");
        let prior = self.db.set_limit(
            rusqlite::limits::Limit::SQLITE_LIMIT_LENGTH,
            self.db
                .limit(rusqlite::limits::Limit::SQLITE_LIMIT_LENGTH)?
                .min(64000),
        )?;
        let registration: rusqlite::Result<Option<String>> = self
            .db
            .query_row(
                "SELECT value FROM records WHERE kind='artifact' AND id=?1",
                [id],
                |row| row.get(0),
            )
            .optional();
        self.db
            .set_limit(rusqlite::limits::Limit::SQLITE_LIMIT_LENGTH, prior)?;
        let registered = registration?;
        let expected = if let Some(raw) = registered {
            #[derive(serde::Deserialize)]
            #[serde(rename_all = "camelCase", deny_unknown_fields)]
            struct Artifact {
                artifact_id: String,
                total_bytes: usize,
            }
            let record: Artifact = crate::json::decode(crate::json::parse(raw.as_bytes(), 64000)?)?;
            ensure!(
                record.artifact_id == id,
                "Artifact registration identity mismatch"
            );
            record.total_bytes
        } else {
            let prior = self.db.set_limit(
                rusqlite::limits::Limit::SQLITE_LIMIT_LENGTH,
                self.db
                    .limit(rusqlite::limits::Limit::SQLITE_LIMIT_LENGTH)?
                    .min(32_000_000),
            )?;
            let membership = (|| -> Result<Option<usize>> {
                let mut statement=self.db.prepare("SELECT s.id,json_extract(f.value,'$.path'),json_extract(f.value,'$.size') FROM snapshots AS s JOIN json_each(s.value,'$.files') AS f WHERE json_extract(s.value,'$.kind')!='federated' AND json_extract(f.value,'$.hash')=?1 AND json_type(f.value,'$.size')='integer' ORDER BY s.id LIMIT 1000")?;
                let mut rows = statement.query([id])?;
                while let Some(row) = rows.next()? {
                    self.check_deadline()?;
                    let snapshot_id: String = row.get(0)?;
                    let path: String = row.get(1)?;
                    let size: i64 = row.get(2)?;
                    if !(0..=4_000_000).contains(&size) {
                        continue;
                    }
                    let proof = (|| -> Result<()> {
                        let snapshot = self.snapshot(&snapshot_id)?;
                        crate::capture::source_path_policy(self, &snapshot, &path)?;
                        ensure!(
                            snapshot.files.iter().any(|file| file.path == path
                                && file.hash == id
                                && file.size == size as usize),
                            "Source content membership mismatch"
                        );
                        Ok(())
                    })();
                    if proof.is_ok() {
                        return Ok(Some(size as usize));
                    }
                }
                Ok(None)
            })();
            self.db
                .set_limit(rusqlite::limits::Limit::SQLITE_LIMIT_LENGTH, prior)?;
            membership?.context(
                "Artifact lacks published registration or a verified local captured-source owner",
            )?
        };
        ensure!(
            expected <= 512_000_000,
            "Artifact size exceeds content admission limit"
        );
        Ok(expected)
    }
    pub fn read_artifact(&self, id: &str) -> Result<Vec<u8>> {
        self.read_artifact_limited(id, 512_000_000)
    }
    pub fn read_artifact_limited(&self, id: &str, maximum: usize) -> Result<Vec<u8>> {
        let expected = self.artifact_size(id)?;
        self.read_blob_exact(id, expected, maximum)
    }
    /// Verify the whole immutable object while retaining only a bounded page.
    pub fn read_artifact_page(
        &self,
        id: &str,
        start: usize,
        maximum: usize,
        utf8: bool,
    ) -> Result<ArtifactPage> {
        let expected = self.artifact_size(id)?;
        ensure!(
            start <= expected && (1..=4_000_000).contains(&maximum),
            "Invalid artifact page bounds"
        );
        let path = self.dir.join("blobs").join(id);
        let metadata = fs::symlink_metadata(&path)?;
        ensure!(
            metadata.is_file() && !is_link(&metadata) && metadata.len() == expected as u64,
            "Artifact file type or size mismatch"
        );
        let mut file = fs::File::open(path)?;
        let opened = file.metadata()?;
        ensure!(
            opened.is_file() && !is_link(&opened) && opened.len() == expected as u64,
            "Opened artifact size mismatch"
        );
        let end = start.saturating_add(maximum).min(expected);
        let mut bytes = Vec::with_capacity(end - start);
        let mut validation = Vec::with_capacity(if utf8 { 64004 } else { 0 });
        let mut hasher = Sha256::new();
        let mut offset = 0usize;
        let mut buffer = [0u8; 64000];
        loop {
            self.check_deadline()?;
            let maximum = expected
                .saturating_sub(offset)
                .saturating_add(1)
                .min(buffer.len());
            let count = file.read(&mut buffer[..maximum])?;
            if count == 0 {
                break;
            }
            ensure!(
                count <= expected.saturating_sub(offset),
                "Artifact grew beyond admitted size"
            );
            hasher.update(&buffer[..count]);
            if utf8 {
                if start >= offset && start < offset + count {
                    ensure!(
                        buffer[start - offset] & 0xc0 != 0x80,
                        "Artifact cursor splits a UTF-8 code point"
                    );
                }
                validation.extend_from_slice(&buffer[..count]);
                match std::str::from_utf8(&validation) {
                    Ok(_) => validation.clear(),
                    Err(error) => {
                        ensure!(
                            error.error_len().is_none(),
                            "Artifact contains non-UTF8 data; request base64"
                        );
                        let valid = error.valid_up_to();
                        let tail = validation.len() - valid;
                        ensure!(tail <= 3, "Invalid UTF-8 stream boundary");
                        validation.copy_within(valid.., 0);
                        validation.truncate(tail);
                    }
                }
            }
            let first = start.max(offset);
            let last = end.min(offset + count);
            if first < last {
                bytes.extend_from_slice(&buffer[first - offset..last - offset]);
            }
            offset += count;
        }
        ensure!(
            offset == expected && (!utf8 || validation.is_empty()),
            "Artifact ended before admitted bytes or UTF-8 sequence completed"
        );
        ensure!(
            hex::encode(hasher.finalize()) == id,
            "Artifact content hash mismatch"
        );
        if utf8 && let Err(error) = std::str::from_utf8(&bytes) {
            ensure!(error.error_len().is_none(), "Invalid artifact page UTF-8");
            bytes.truncate(error.valid_up_to());
        }
        ensure!(
            !utf8 || !bytes.is_empty() || start == expected,
            "Artifact page budget cannot fit the next UTF-8 character"
        );
        self.check_deadline()?;
        Ok(ArtifactPage {
            bytes,
            total_bytes: expected,
        })
    }
    pub fn snapshot(&self, id: &str) -> Result<Snapshot> {
        let snapshot: Snapshot = self.load("snapshots", id)?;
        ensure!(
            snapshot.id == id && snapshot.workspace_id == self.workspace_id,
            "Stored snapshot lookup identity or workspace mismatch"
        );
        Ok(snapshot)
    }
    pub fn graph(&self, id: &str) -> Result<Graph> {
        let graph: Graph = self.load("graphs", id)?;
        ensure!(
            graph.snapshot_id == id,
            "Stored graph lookup identity mismatch"
        );
        ensure!(
            graph.entities.iter().all(|entity| entity.snapshot_id == id)
                && graph
                    .observations
                    .iter()
                    .all(|observation| observation.snapshot_id == id)
                && graph
                    .evidence
                    .iter()
                    .all(|evidence| evidence.snapshot_id == id),
            "Stored graph member snapshot identity mismatch"
        );
        Ok(graph)
    }
    fn load<T: DeserializeOwned>(&self, table: &str, id: &str) -> Result<T> {
        ensure!(valid_id(id), "Invalid snapshot ID");
        let raw: String = self
            .db
            .query_row(
                &format!("SELECT value FROM {table} WHERE id=?1"),
                [id],
                |r| r.get(0),
            )
            .context("Snapshot or graph not found")?;
        Ok(serde_json::from_str(&raw)?)
    }
    pub fn cache(&self, key: &str) -> Result<Option<Extraction>> {
        let raw: Option<String> = self
            .db
            .query_row("SELECT value FROM cache WHERE key=?1", [key], |r| r.get(0))
            .optional()?;
        raw.map(|v| Ok(serde_json::from_str(&v)?)).transpose()
    }
    pub fn put_cache(&self, key: &str, value: &Extraction) -> Result<()> {
        self.check_deadline()?;
        self.db.execute(
            "INSERT OR IGNORE INTO cache VALUES(?1,?2)",
            params![key, serde_json::to_string(value)?],
        )?;
        Ok(())
    }
    pub fn put_caches(
        &mut self,
        values: &[(String, Extraction)],
        deadline: &crate::capture::Deadline,
    ) -> Result<()> {
        self.check_deadline()?;
        let tx = self.db.transaction()?;
        {
            let mut insert = tx.prepare_cached("INSERT OR IGNORE INTO cache VALUES(?1,?2)")?;
            for (key, value) in values {
                deadline.check()?;
                insert.execute(params![key, serde_json::to_string(value)?])?;
            }
        }
        deadline.check()?;
        tx.commit()?;
        Ok(())
    }
    pub fn record<T: Serialize>(&self, kind: &str, id: &str, value: &T) -> Result<()> {
        self.check_deadline()?;
        let value = serde_json::to_string(value)?;
        self.check_deadline()?;
        self.db.execute("INSERT INTO records VALUES(?1,?2,?3) ON CONFLICT(kind,id) DO UPDATE SET value=excluded.value",params![kind,id,value])?;
        Ok(())
    }
    pub fn replace_record<T: Serialize>(
        &self,
        kind: &str,
        id: &str,
        expected: &T,
        next: &T,
    ) -> Result<bool> {
        self.check_deadline()?;
        let next = serde_json::to_string(next)?;
        let expected = serde_json::to_string(expected)?;
        self.check_deadline()?;
        Ok(self.db.execute(
            "UPDATE records SET value=?1 WHERE kind=?2 AND id=?3 AND value=?4",
            params![next, kind, id, expected],
        )? == 1)
    }
    pub fn delete_record<T: Serialize>(&self, kind: &str, id: &str, expected: &T) -> Result<bool> {
        self.check_deadline()?;
        let expected = serde_json::to_string(expected)?;
        self.check_deadline()?;
        Ok(self.db.execute(
            "DELETE FROM records WHERE kind=?1 AND id=?2 AND value=?3",
            params![kind, id, expected],
        )? == 1)
    }
    pub fn get_record<T: DeserializeOwned>(&self, kind: &str, id: &str) -> Result<T> {
        let value: String = self
            .db
            .query_row(
                "SELECT value FROM records WHERE kind=?1 AND id=?2",
                params![kind, id],
                |r| r.get(0),
            )
            .context("Record not found")?;
        Ok(serde_json::from_str(&value)?)
    }
    pub fn records<T: DeserializeOwned>(&self, kind: &str) -> Result<Vec<T>> {
        let mut statement = self
            .db
            .prepare("SELECT value FROM records WHERE kind=?1 ORDER BY id")?;
        let rows = statement.query_map([kind], |r| r.get::<_, String>(0))?;
        rows.map(|v| Ok(serde_json::from_str(&v?)?)).collect()
    }
    pub fn publish(&mut self, snapshot: &Snapshot, graph: &Graph) -> Result<()> {
        self.publish_with_latest(snapshot, graph, true)
    }
    pub fn publish_historical(&mut self, snapshot: &Snapshot, graph: &Graph) -> Result<()> {
        self.publish_with_latest(snapshot, graph, false)
    }
    fn publish_with_latest(
        &mut self,
        snapshot: &Snapshot,
        graph: &Graph,
        update_latest: bool,
    ) -> Result<()> {
        self.publish_rows(snapshot, graph, update_latest, false)
    }
    /// Only owned atomic acquisition uses this path; public publication still
    /// owns its transaction and refuses nesting. No latest pointer is changed.
    pub(crate) fn publish_historical_pending(
        &self,
        snapshot: &Snapshot,
        graph: &Graph,
    ) -> Result<()> {
        ensure!(
            !self.db.is_autocommit(),
            "Historical publication requires owned transaction"
        );
        self.publish_rows(snapshot, graph, false, true)
    }
    fn publish_rows(
        &self,
        snapshot: &Snapshot,
        graph: &Graph,
        update_latest: bool,
        pending: bool,
    ) -> Result<()> {
        self.check_deadline()?;
        let documents: Vec<_> = snapshot
            .files
            .iter()
            .map(|file| {
                self.check_deadline()?;
                Ok((
                    file.path.clone(),
                    String::from_utf8(self.read_blob_exact(&file.hash, file.size, 4_000_000)?)?,
                ))
            })
            .collect::<Result<_>>()?;
        self.check_deadline()?;
        let publication_deadline = self.deadline.borrow().clone();
        let check = || -> Result<()> {
            if let Some(deadline) = &publication_deadline {
                deadline.check()?;
            }
            Ok(())
        };
        let tx = if pending {
            None
        } else {
            Some(rusqlite::Transaction::new_unchecked(
                &self.db,
                rusqlite::TransactionBehavior::Deferred,
            )?)
        };
        let db: &Connection = tx.as_deref().unwrap_or(&self.db);
        let snapshot_inserted = db.execute(
            "INSERT OR IGNORE INTO snapshots VALUES(?1,?2)",
            params![snapshot.id, serde_json::to_string(snapshot)?],
        )?;
        if snapshot_inserted > 0 {
            let published_at = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)?
                .as_millis();
            let receipt = serde_json::json!({
                "snapshotId": snapshot.id, "workspaceId": snapshot.workspace_id,
                "publishedAtMs": u64::try_from(published_at)?
            });
            db.execute(
                "INSERT INTO records(kind,id,value) VALUES('snapshot-published',?1,?2)",
                params![snapshot.id, serde_json::to_string(&receipt)?],
            )?;
        }
        let inserted = db.execute(
            "INSERT OR IGNORE INTO graphs VALUES(?1,?2)",
            params![snapshot.id, serde_json::to_string(graph)?],
        )?;
        if inserted > 0 {
            for e in &graph.entities {
                check()?;
                db.execute(
                    "INSERT INTO symbols VALUES(?1,?2,?3,?4,?5)",
                    params![
                        snapshot.id,
                        e.id,
                        crate::query::identifier_terms(&format!("{} {}", e.name, e.qualified_name)),
                        e.path,
                        e.signature
                    ],
                )?;
            }
            for (path, text) in documents {
                check()?;
                db.execute(
                    "INSERT INTO documents VALUES(?1,?2,?3)",
                    params![snapshot.id, path, text],
                )?;
            }
        }
        if pending {
            ensure!(
                serde_json::to_value(self.snapshot(&snapshot.id)?)?
                    == serde_json::to_value(snapshot)?
                    && serde_json::to_value(self.graph(&snapshot.id)?)?
                        == serde_json::to_value(graph)?,
                "Immutable historical publication collision"
            );
        }
        check()?;
        if update_latest {
            db.execute("INSERT INTO meta VALUES('latest',?1) ON CONFLICT(key) DO UPDATE SET value=excluded.value",[&snapshot.id])?;
        }
        check()?;
        if let Some(tx) = tx {
            tx.commit()?;
        }
        Ok(())
    }
    pub fn latest(&self) -> Result<Option<String>> {
        Ok(self
            .db
            .query_row("SELECT value FROM meta WHERE key='latest'", [], |r| {
                r.get(0)
            })
            .optional()?)
    }
    pub fn set_latest(&self, id: &str) -> Result<()> {
        self.check_deadline()?;
        ensure!(valid_id(id), "Invalid latest snapshot ID");
        self.db.execute("INSERT INTO meta VALUES('latest',?1) ON CONFLICT(key) DO UPDATE SET value=excluded.value", [id])?;
        Ok(())
    }
    pub fn set_deadline(&self, deadline: &crate::capture::Deadline) -> Result<()> {
        self.replace_deadline(Some(deadline.clone()));
        self.check_deadline()
    }
    pub fn clear_deadline(&self) {
        self.replace_deadline(None);
    }
    pub(crate) fn inherited_deadline(
        &self,
        requested: &crate::capture::Deadline,
    ) -> Result<crate::capture::Deadline> {
        if let Some(prior) = self.deadline.borrow().as_ref() {
            requested.intersection(prior)
        } else {
            requested.check()?;
            Ok(requested.clone())
        }
    }
    /// Swap only the Store-owned progress policy; scoped callers restore their saved policy.
    pub(crate) fn replace_deadline(
        &self,
        deadline: Option<crate::capture::Deadline>,
    ) -> Option<crate::capture::Deadline> {
        let prior = self.deadline.replace(deadline.clone());
        busy::replace_if_present(&self.db, deadline.as_ref());
        if let Some(bounded) = deadline {
            self.db
                .progress_handler(1000, Some(move || bounded.check().is_err()));
        } else {
            self.db.progress_handler(0, None::<fn() -> bool>);
        }
        prior
    }
    pub(crate) fn check_deadline(&self) -> Result<()> {
        ensure!(
            self.client_valid.get(),
            "State client lease could not be restored; reopen state"
        );
        let deadline = self.deadline.borrow();
        if let Some(bounded) = deadline.as_ref() {
            bounded.check()?;
        }
        let duration = deadline
            .as_ref()
            .map_or(Duration::from_secs(10), crate::capture::Deadline::remaining);
        ensure!(
            !duration.is_zero(),
            "Request deadline exceeded before database write"
        );
        busy::install(&self.db, deadline.as_ref())?;
        Ok(())
    }
}
