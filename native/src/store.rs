use crate::{
    model::{Extraction, Graph, Snapshot, hash, valid_id},
    paths::is_link,
};
use anyhow::{Context, Result, ensure};
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Serialize, de::DeserializeOwned};
use std::{
    cell::Cell,
    fs,
    io::Write,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

pub struct Store {
    pub dir: PathBuf,
    pub db: Connection,
    pub workspace_id: String,
    deadline: Cell<Option<Instant>>,
    client_lock: fs::File,
    admission_gate: fs::File,
    client_valid: Cell<bool>,
    maintenance_active: Cell<bool>,
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
fn lock_until(file: &fs::File, exclusive: bool, end: Instant) -> Result<()> {
    loop {
        ensure!(Instant::now() < end, "State client lock deadline exceeded");
        let result = if exclusive {
            fs2::FileExt::try_lock_exclusive(file)
        } else {
            fs2::FileExt::try_lock_shared(file)
        };
        match result {
            Ok(()) => return Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
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
        )?;
        self.maintenance_active.set(true);
        let mut upgrade = ClientUpgrade {
            store: self,
            gate_locked: true,
            restore_client: false,
            exclusive: false,
        };
        fs2::FileExt::unlock(&self.client_lock)?;
        self.client_valid.set(false);
        upgrade.restore_client = true;
        fs2::FileExt::try_lock_exclusive(&self.client_lock)
            .context("Maintenance requires all other state clients to close")?;
        upgrade.exclusive = true;
        self.client_valid.set(true);
        let result = operation();
        upgrade.restore()?;
        result
    }
    pub fn open(root: &Path) -> Result<Self> {
        Self::open_with_timeout(root, Duration::from_secs(10))
    }
    pub fn open_with_timeout(root: &Path, timeout: Duration) -> Result<Self> {
        let started = Instant::now();
        let dir = root.join(".yoke");
        if let Ok(meta) = fs::symlink_metadata(&dir) {
            ensure!(!is_link(&meta), "State directory must not be a link");
        }
        fs::create_dir_all(&dir)?;
        for name in ["blobs", "trees"] {
            let path = dir.join(name);
            if let Ok(meta) = fs::symlink_metadata(&path) {
                ensure!(!is_link(&meta), "State subdirectory must not be a link");
            }
            fs::create_dir_all(path)?;
        }
        let admission_gate = state_lock(&dir, "maintenance.lock")?;
        let client_lock = state_lock(&dir, "clients.lock")?;
        let end = started + timeout;
        lock_until(&admission_gate, false, end)?;
        lock_until(&client_lock, false, end)?;
        fs2::FileExt::unlock(&admission_gate)?;
        let db_path = dir.join("index.sqlite");
        for name in [
            "index.sqlite",
            "index.sqlite-wal",
            "index.sqlite-shm",
            "index.sqlite-journal",
        ] {
            if let Ok(meta) = fs::symlink_metadata(dir.join(name)) {
                ensure!(
                    !is_link(&meta),
                    "Database or SQLite sidecar must not be a link"
                );
            }
        }
        let db = Connection::open(db_path)?;
        db.busy_timeout(
            timeout
                .saturating_sub(started.elapsed())
                .min(Duration::from_secs(10)),
        )?;
        db.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON; PRAGMA synchronous=FULL;
            CREATE TABLE IF NOT EXISTS meta(key TEXT PRIMARY KEY,value TEXT NOT NULL);
            CREATE TABLE IF NOT EXISTS snapshots(id TEXT PRIMARY KEY,value TEXT NOT NULL);
            CREATE TABLE IF NOT EXISTS graphs(id TEXT PRIMARY KEY,value TEXT NOT NULL);
            CREATE TABLE IF NOT EXISTS cache(key TEXT PRIMARY KEY,value TEXT NOT NULL);
            CREATE TABLE IF NOT EXISTS records(kind TEXT NOT NULL,id TEXT NOT NULL,value TEXT NOT NULL,PRIMARY KEY(kind,id));
            CREATE VIRTUAL TABLE IF NOT EXISTS symbols USING fts5(snapshot UNINDEXED,id UNINDEXED,name,path UNINDEXED,signature);
            CREATE VIRTUAL TABLE IF NOT EXISTS documents USING fts5(snapshot UNINDEXED,path UNINDEXED,text);")?;
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
                db.execute("INSERT OR IGNORE INTO meta VALUES('workspace',?1)", [&id])?;
                db.query_row("SELECT value FROM meta WHERE key='workspace'", [], |r| {
                    r.get(0)
                })?
            }
        };
        Ok(Self {
            dir,
            db,
            workspace_id,
            deadline: Cell::new(None),
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
                self.read_blob(&id)? == bytes,
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
            ensure!(self.read_blob(&id)? == bytes, "Concurrent blob corruption");
        }
        Ok(id)
    }
    pub fn read_blob(&self, id: &str) -> Result<Vec<u8>> {
        ensure!(valid_id(id), "Invalid content ID");
        let path = self.dir.join("blobs").join(id);
        ensure!(!is_link(&fs::symlink_metadata(&path)?), "Blob link refused");
        let bytes = fs::read(path)?;
        ensure!(hash(&bytes) == id, "Content hash mismatch");
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
    pub fn read_artifact(&self, id: &str) -> Result<Vec<u8>> {
        ensure!(valid_id(id), "Invalid artifact ID");
        let registered: Option<String> = self
            .db
            .query_row(
                "SELECT value FROM records WHERE kind='artifact' AND id=?1",
                [id],
                |row| row.get(0),
            )
            .optional()?;
        let expected = if let Some(raw) = registered {
            #[derive(serde::Deserialize)]
            #[serde(rename_all = "camelCase", deny_unknown_fields)]
            struct Artifact {
                artifact_id: String,
                total_bytes: usize,
            }
            let record: Artifact = serde_json::from_str(&raw)?;
            ensure!(
                record.artifact_id == id,
                "Artifact registration identity mismatch"
            );
            Some(record.total_bytes)
        } else {
            let local: bool=self.db.query_row(
                "SELECT EXISTS(SELECT 1 FROM snapshots AS s JOIN json_each(s.value,'$.files') AS f WHERE json_extract(s.value,'$.kind')!='federated' AND json_extract(f.value,'$.hash')=?1)",
                [id],|row|row.get(0)
            )?;
            ensure!(
                local,
                "Artifact is not published or belongs to explicit foreign-source scope"
            );
            None
        };
        let bytes = self.read_blob(id)?;
        ensure!(
            expected.is_none_or(|size| size == bytes.len()),
            "Artifact registration size mismatch"
        );
        Ok(bytes)
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
        self.db.execute("INSERT INTO records VALUES(?1,?2,?3) ON CONFLICT(kind,id) DO UPDATE SET value=excluded.value",params![kind,id,serde_json::to_string(value)?])?;
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
        Ok(self.db.execute(
            "UPDATE records SET value=?1 WHERE kind=?2 AND id=?3 AND value=?4",
            params![
                serde_json::to_string(next)?,
                kind,
                id,
                serde_json::to_string(expected)?
            ],
        )? == 1)
    }
    pub fn delete_record<T: Serialize>(&self, kind: &str, id: &str, expected: &T) -> Result<bool> {
        self.check_deadline()?;
        Ok(self.db.execute(
            "DELETE FROM records WHERE kind=?1 AND id=?2 AND value=?3",
            params![kind, id, serde_json::to_string(expected)?],
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
        self.check_deadline()?;
        let documents: Vec<_> = snapshot
            .files
            .iter()
            .map(|file| {
                self.check_deadline()?;
                Ok((
                    file.path.clone(),
                    String::from_utf8(self.read_blob(&file.hash)?)?,
                ))
            })
            .collect::<Result<_>>()?;
        self.check_deadline()?;
        let end = self.deadline.get();
        let check = || -> Result<()> {
            ensure!(
                end.is_none_or(|end| Instant::now() < end),
                "Request deadline exceeded during publication"
            );
            Ok(())
        };
        let tx = self.db.transaction()?;
        let snapshot_inserted = tx.execute(
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
            tx.execute(
                "INSERT INTO records(kind,id,value) VALUES('snapshot-published',?1,?2)",
                params![snapshot.id, serde_json::to_string(&receipt)?],
            )?;
        }
        let inserted = tx.execute(
            "INSERT OR IGNORE INTO graphs VALUES(?1,?2)",
            params![snapshot.id, serde_json::to_string(graph)?],
        )?;
        if inserted > 0 {
            for e in &graph.entities {
                check()?;
                tx.execute(
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
                tx.execute(
                    "INSERT INTO documents VALUES(?1,?2,?3)",
                    params![snapshot.id, path, text],
                )?;
            }
        }
        check()?;
        tx.execute("INSERT INTO meta VALUES('latest',?1) ON CONFLICT(key) DO UPDATE SET value=excluded.value",[&snapshot.id])?;
        check()?;
        tx.commit()?;
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
        let end = Instant::now() + deadline.remaining();
        self.deadline.set(Some(end));
        self.db
            .progress_handler(1000, Some(move || Instant::now() >= end));
        self.check_deadline()
    }
    pub fn clear_deadline(&self) {
        self.deadline.set(None);
        self.db.progress_handler(0, None::<fn() -> bool>);
    }
    fn check_deadline(&self) -> Result<()> {
        ensure!(
            self.client_valid.get(),
            "State client lease could not be restored; reopen state"
        );
        let duration = self.deadline.get().map_or(Duration::from_secs(10), |end| {
            end.saturating_duration_since(Instant::now())
        });
        ensure!(
            !duration.is_zero(),
            "Request deadline exceeded before database write"
        );
        self.db
            .busy_timeout(duration.min(Duration::from_secs(10)))?;
        Ok(())
    }
}
