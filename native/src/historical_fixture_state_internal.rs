//! Fixture-only logical DB/CAS mutation oracle; no comparison API dependency.
#![allow(dead_code)] // Shared test targets intentionally use different fixture members.
use crate::Engine;
use serde_json::{Value, json};
use std::{collections::BTreeMap, fs};

pub fn database_path(engine: &Engine) -> String {
    engine
        .store
        .db
        .query_row(
            "SELECT file FROM pragma_database_list WHERE name='main'",
            [],
            |r| r.get(0),
        )
        .unwrap()
}
fn cell(value: rusqlite::types::ValueRef<'_>) -> Value {
    use rusqlite::types::ValueRef;
    match value {
        ValueRef::Null => json!(["null"]),
        ValueRef::Integer(n) => json!(["integer", n]),
        ValueRef::Real(n) => json!(["realBits", n.to_bits().to_string()]),
        ValueRef::Text(bytes) => json!(["textBytes", bytes]),
        ValueRef::Blob(bytes) => json!(["blobBytes", bytes]),
    }
}
// Fresh reader detects committed same-count replacements through another connection.
// Logical schema/rows only; no claim about SQLite/WAL file-byte immutability.
pub fn sql_state(engine: &Engine) -> Value {
    let connection = rusqlite::Connection::open_with_flags(
        database_path(engine),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .unwrap();
    let transaction = connection.unchecked_transaction().unwrap();
    let schema = {
        let mut statement = transaction
            .prepare("SELECT type,name,tbl_name,sql FROM sqlite_master ORDER BY type,name,tbl_name")
            .unwrap();
        statement
            .query_map([], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, Option<String>>(3)?,
                ))
            })
            .unwrap()
            .collect::<rusqlite::Result<Vec<_>>>()
            .unwrap()
    };
    let mut tables = BTreeMap::new();
    for (kind, name, _, definition) in &schema {
        if kind != "table" {
            continue;
        }
        let quoted = name.replace('"', r#""""#);
        let mut statement = transaction
            .prepare(&format!(r#"SELECT * FROM "{quoted}""#))
            .unwrap();
        let count = statement.column_count();
        let rows = statement
            .query_map([], |row| {
                (0..count)
                    .map(|column| row.get_ref(column).map(cell))
                    .collect::<rusqlite::Result<Vec<_>>>()
            })
            .unwrap();
        let mut rows = rows.collect::<rusqlite::Result<Vec<_>>>().unwrap();
        rows.sort_by_key(|row| serde_json::to_vec(row).unwrap());
        tables.insert(name.clone(), json!({"definition":definition,"rows":rows}));
    }
    json!({"schema":schema,"tables":tables})
}
// Fixture-only content inventory also catches unregistered physical CAS writes.
pub fn state(engine: &Engine) -> Value {
    let mut blobs = BTreeMap::new();
    for entry in fs::read_dir(engine.store.dir.join("blobs")).unwrap() {
        let entry = entry.unwrap();
        blobs.insert(
            entry.file_name().to_string_lossy().into_owned(),
            fs::read(entry.path()).unwrap(),
        );
    }
    json!({"sql":sql_state(engine),"blobs":blobs})
}
pub fn calibrate_foreign_connection_mutation_oracle(engine: &Engine) {
    let before = sql_state(engine);
    let local_changes: i64 = engine
        .store
        .db
        .query_row("SELECT total_changes()", [], |r| r.get(0))
        .unwrap();
    let writer = rusqlite::Connection::open(database_path(engine)).unwrap();
    let (kind, id, raw): (String, String, String) = writer
        .query_row(
            "SELECT kind,id,value FROM records ORDER BY kind,id LIMIT 1",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .unwrap();
    let count: i64 = writer
        .query_row("SELECT count(*) FROM records", [], |r| r.get(0))
        .unwrap();
    assert_eq!(
        writer
            .execute(
                "UPDATE records SET value=?1 WHERE kind=?2 AND id=?3",
                rusqlite::params![format!("{raw} "), kind, id]
            )
            .unwrap(),
        1
    );
    assert_ne!(sql_state(engine), before);
    assert_eq!(
        writer
            .query_row("SELECT count(*) FROM records", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        count
    );
    assert_eq!(
        engine
            .store
            .db
            .query_row("SELECT total_changes()", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        local_changes
    );
    writer
        .execute(
            "UPDATE records SET value=?1 WHERE kind=?2 AND id=?3",
            rusqlite::params![raw, kind, id],
        )
        .unwrap();
    assert_eq!(sql_state(engine), before);
}
