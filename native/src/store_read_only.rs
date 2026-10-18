//! Existing-only application-state access; no schema, journal-mode or identity setup.
use super::{Store, lock_is_contended};
use crate::{capture::Deadline, model::valid_id, paths::is_link};
use anyhow::{Result, ensure};
use rusqlite::{Connection, OpenFlags, limits::Limit};
use std::{
    cell::{Cell, RefCell},
    fs,
    path::Path,
    time::Duration,
};

fn existing_file(path: &Path) -> Result<fs::File> {
    let metadata = fs::symlink_metadata(path)?;
    ensure!(
        metadata.is_file() && !is_link(&metadata),
        "Existing state file must be regular and unlinked"
    );
    Ok(fs::OpenOptions::new().read(true).open(path)?)
}
fn shared(file: &fs::File, d: &Deadline) -> Result<()> {
    loop {
        d.check()?;
        match fs2::FileExt::try_lock_shared(file) {
            Ok(()) => return d.check(),
            Err(error) if lock_is_contended(&error) => {
                std::thread::sleep(d.remaining().min(Duration::from_millis(2)))
            }
            Err(error) => return Err(error.into()),
        }
    }
}
fn sql<T>(
    db: &Connection,
    d: &Deadline,
    mut read: impl FnMut() -> rusqlite::Result<T>,
) -> Result<T> {
    loop {
        d.check()?;
        db.busy_timeout(Duration::ZERO)?;
        let result = read();
        d.check()?;
        match result {
            Ok(value) => return Ok(value),
            Err(rusqlite::Error::SqliteFailure(error, _))
                if matches!(
                    error.code,
                    rusqlite::ErrorCode::DatabaseBusy | rusqlite::ErrorCode::DatabaseLocked
                ) =>
            {
                std::thread::sleep(d.remaining().min(Duration::from_millis(2)))
            }
            Err(error) => return Err(error.into()),
        }
    }
}
impl Store {
    /// Open an initialized Store without creating application state or changing
    /// its schema/journal mode. SQLite may coordinate live WAL through sidecars.
    pub fn open_existing_read_only(root: &Path, d: &Deadline) -> Result<Self> {
        d.check()?;
        let dir = root.join(".yoke");
        for path in [&dir, &dir.join("blobs"), &dir.join("trees")] {
            let metadata = fs::symlink_metadata(path)?;
            ensure!(
                metadata.is_dir() && !is_link(&metadata),
                "Existing state directory must be unlinked"
            );
        }
        let admission_gate = existing_file(&dir.join("maintenance.lock"))?;
        let client_lock = existing_file(&dir.join("clients.lock"))?;
        shared(&admission_gate, d)?;
        shared(&client_lock, d)?;
        fs2::FileExt::unlock(&admission_gate)?;
        let _database = existing_file(&dir.join("index.sqlite"))?;
        for name in [
            "index.sqlite-wal",
            "index.sqlite-shm",
            "index.sqlite-journal",
        ] {
            match fs::symlink_metadata(dir.join(name)) {
                Ok(metadata) => ensure!(
                    metadata.is_file() && !is_link(&metadata),
                    "Existing SQLite sidecar must be regular and unlinked"
                ),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(error.into()),
            }
        }
        d.check()?;
        let db = Connection::open_with_flags(
            dir.join("index.sqlite"),
            OpenFlags::SQLITE_OPEN_READ_ONLY,
        )?;
        db.busy_timeout(Duration::ZERO)?;
        let bounded = d.clone();
        db.progress_handler(1000, Some(move || bounded.check().is_err()));
        let prior = db.set_limit(Limit::SQLITE_LIMIT_LENGTH, 65_536)?;
        let workspace = (|| -> Result<String> {
            let raw: Vec<u8> = sql(&db, d, || {
                db.query_row(
                    "SELECT substr(CAST(value AS BLOB),1,65) FROM meta WHERE key='workspace'",
                    [],
                    |row| row.get(0),
                )
            })?;
            ensure!(
                raw.len() == 64,
                "Existing Store workspace identity unavailable"
            );
            let workspace = String::from_utf8(raw)?;
            ensure!(
                valid_id(&workspace),
                "Existing Store workspace identity invalid"
            );
            for query in [
                "SELECT id,value FROM snapshots LIMIT 0",
                "SELECT id,value FROM graphs LIMIT 0",
                "SELECT key,value FROM cache LIMIT 0",
                "SELECT kind,id,value FROM records LIMIT 0",
                "SELECT snapshot,id,name,path,signature FROM symbols LIMIT 0",
                "SELECT snapshot,path,text FROM documents LIMIT 0",
            ] {
                sql(&db, d, || db.prepare(query).map(|_| ()))?;
            }
            Ok(workspace)
        })();
        db.set_limit(Limit::SQLITE_LIMIT_LENGTH, prior)?;
        let workspace_id = workspace?;
        d.check()?;
        Ok(Self {
            dir,
            db,
            workspace_id,
            deadline: RefCell::new(Some(d.clone())),
            client_lock,
            admission_gate,
            client_valid: Cell::new(true),
            maintenance_active: Cell::new(false),
        })
    }
}
