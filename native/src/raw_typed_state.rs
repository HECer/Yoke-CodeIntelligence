//! Bounded logical SQL/CAS state fingerprints for the raw/typed budget fixture.
//! Preserves SQLite cell types and exact content identities.
//! These fingerprints do not measure physical SQLite/WAL bytes or process memory.
use crate::Engine;
use rusqlite::types::ValueRef;
use sha2::{Digest, Sha256};
use std::{fs, io::Read, path::Path};

const MAX_TABLES: usize = 128;
const MAX_ROWS: usize = 4096;
const MAX_COLUMNS: usize = 64;
const MAX_CELL_BYTES: usize = 2_000_000;
const MAX_TOTAL_BYTES: usize = 32_000_000;
const MAX_BLOBS: usize = 128;

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Cell {
    Null,
    Integer(i64),
    Real(u64),
    Text(usize, String),
    Blob(usize, String),
}

fn charge(total: &mut usize, bytes: usize) {
    *total = total
        .checked_add(bytes)
        .expect("Fixture inventory overflow");
    assert!(*total <= MAX_TOTAL_BYTES, "Fixture inventory exceeds bound");
}

fn cell(value: ValueRef<'_>, total: &mut usize) -> Cell {
    match value {
        ValueRef::Null => Cell::Null,
        ValueRef::Integer(n) => Cell::Integer(n),
        ValueRef::Real(n) => Cell::Real(n.to_bits()),
        ValueRef::Text(bytes) | ValueRef::Blob(bytes) => {
            assert!(bytes.len() <= MAX_CELL_BYTES, "Fixture cell exceeds bound");
            charge(total, bytes.len());
            let hash = crate::model::hash(bytes);
            if matches!(value, ValueRef::Text(_)) {
                Cell::Text(bytes.len(), hash)
            } else {
                Cell::Blob(bytes.len(), hash)
            }
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub(super) struct State {
    schema: Vec<Vec<Cell>>,
    tables: Vec<(String, Vec<Vec<Cell>>)>,
    blobs: Vec<(String, u64, String)>,
    call_log: Option<(u64, String)>,
    local_total_changes: i64,
    caller_autocommit: bool,
}

fn file_state(path: &Path, maximum: usize) -> (u64, String) {
    let metadata = fs::symlink_metadata(path).expect("Read fixture file metadata");
    assert!(metadata.is_file() && !metadata.file_type().is_symlink());
    assert!(
        metadata.len() <= maximum as u64,
        "Fixture file exceeds bound"
    );
    let mut reader = fs::File::open(path)
        .expect("Open fixture inventory file")
        .take((maximum as u64).checked_add(1).unwrap());
    let mut hash = Sha256::new();
    let mut bytes = 0u64;
    let mut buffer = [0u8; 8192];
    loop {
        let count = reader
            .read(&mut buffer)
            .expect("Read fixture inventory file");
        if count == 0 {
            break;
        }
        bytes = bytes.checked_add(count as u64).unwrap();
        assert!(bytes <= maximum as u64, "Fixture file grew beyond bound");
        hash.update(&buffer[..count]);
    }
    assert_eq!(
        bytes,
        metadata.len(),
        "Fixture file changed during inventory"
    );
    (bytes, hex::encode(hash.finalize()))
}

pub(super) fn state(engine: &Engine, call_log: &Path) -> State {
    let database: String = engine
        .store
        .db
        .query_row(
            "SELECT file FROM pragma_database_list WHERE name='main'",
            [],
            |row| row.get(0),
        )
        .expect("Resolve fixture database");
    // A fresh connection observes committed foreign writes as well as writes
    // through the supplied Store. No comparison API participates in this oracle.
    let connection =
        rusqlite::Connection::open_with_flags(database, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
            .expect("Open read-only fixture inventory");
    let transaction = connection.unchecked_transaction().unwrap();
    let mut total = 0usize;
    let mut schema = Vec::new();
    let mut names = Vec::new();
    {
        let mut statement = transaction
            .prepare("SELECT type,name,tbl_name,sql FROM sqlite_master ORDER BY type,name,tbl_name")
            .unwrap();
        let mut rows = statement.query([]).unwrap();
        while let Some(row) = rows.next().unwrap() {
            assert!(schema.len() < MAX_TABLES, "Fixture schema exceeds bound");
            schema.push(
                (0..4)
                    .map(|column| cell(row.get_ref(column).unwrap(), &mut total))
                    .collect(),
            );
            let kind = row.get_ref(0).unwrap().as_str().unwrap();
            let name = row.get_ref(1).unwrap().as_str().unwrap();
            assert!(name.len() <= 256 && kind.len() <= 32);
            if kind == "table" {
                names.push(name.to_owned());
            }
        }
    }
    let mut tables = Vec::new();
    let mut row_count = 0usize;
    for name in names {
        let quoted = name.replace('"', "\"\"");
        let mut statement = transaction
            .prepare(&format!("SELECT * FROM \"{quoted}\""))
            .unwrap();
        let columns = statement.column_count();
        assert!(columns <= MAX_COLUMNS, "Fixture column count exceeds bound");
        let mut rows = statement.query([]).unwrap();
        let mut contents = Vec::new();
        while let Some(row) = rows.next().unwrap() {
            row_count = row_count.checked_add(1).unwrap();
            assert!(row_count <= MAX_ROWS, "Fixture row count exceeds bound");
            contents.push(
                (0..columns)
                    .map(|column| cell(row.get_ref(column).unwrap(), &mut total))
                    .collect::<Vec<_>>(),
            );
        }
        contents.sort();
        tables.push((name, contents));
    }
    let mut blobs = Vec::new();
    for entry in fs::read_dir(engine.store.dir.join("blobs")).unwrap() {
        assert!(blobs.len() < MAX_BLOBS, "Fixture CAS count exceeds bound");
        let entry = entry.unwrap();
        let name = entry
            .file_name()
            .into_string()
            .expect("Fixture CAS name is UTF-8");
        assert!(crate::model::valid_id(&name), "Unexpected fixture CAS name");
        let (size, hash) = file_state(&entry.path(), MAX_CELL_BYTES);
        charge(&mut total, usize::try_from(size).unwrap());
        blobs.push((name, size, hash));
    }
    blobs.sort();
    let call_log = match fs::symlink_metadata(call_log) {
        Ok(_) => Some(file_state(call_log, 4096)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(_) => panic!("Cannot inspect fixture call log"),
    };
    State {
        schema,
        tables,
        blobs,
        call_log,
        local_total_changes: engine
            .store
            .db
            .query_row("SELECT total_changes()", [], |row| row.get(0))
            .unwrap(),
        caller_autocommit: engine.store.db.is_autocommit(),
    }
}
