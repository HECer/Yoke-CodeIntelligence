//! Genuine local Git fixtures and independent source/state oracles for R5-A.
#![allow(dead_code)] // Shared by qualification, public-feature and private-scope targets.
use super::test_yoke::{
    Engine,
    capture::{CaptureOptions, Deadline},
    git_workflows::{self, HostGit, WorkflowOptions},
    model::hash,
};
use rusqlite::types::ValueRef;
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::{Condvar, Mutex},
};
use tempfile::TempDir;

pub const MOVED_BASE: &str = "// café <tag>&\r\nfn changed() {\r\n    let a = 1;\r\n    let b = 2;\r\n}\r\nfn caller() { changed(); changed(); }\r\n";
pub const MOVED_HEAD: &str = "// café <tag>&\r\nfn changed() {\r\n    let a = 3;\r\n    let b = 4;\r\n}\r\nfn caller() { changed(); changed(); }\r\n";

static ACTIVE: Mutex<usize> = Mutex::new(0);
static AVAILABLE: Condvar = Condvar::new();

struct Permit;
impl Permit {
    fn acquire() -> Self {
        let active = ACTIVE.lock().unwrap_or_else(|error| error.into_inner());
        let mut active = AVAILABLE
            .wait_while(active, |count| *count >= 4)
            .unwrap_or_else(|error| error.into_inner());
        *active += 1;
        Self
    }
}
impl Drop for Permit {
    fn drop(&mut self) {
        let mut active = ACTIVE.lock().unwrap_or_else(|error| error.into_inner());
        *active -= 1;
        AVAILABLE.notify_one();
    }
}

fn executable() -> PathBuf {
    std::env::split_paths(&std::env::var_os("PATH").expect("host PATH"))
        .map(|path| path.join(if cfg!(windows) { "git.exe" } else { "git" }))
        .find(|path| path.is_absolute() && path.is_file())
        .expect("real host Git executable")
        .canonicalize()
        .unwrap()
}

fn git(root: &Path, args: &[&str]) -> String {
    let output = Command::new(executable())
        .current_dir(root)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env(
            "GIT_CONFIG_GLOBAL",
            if cfg!(windows) { "NUL" } else { "/dev/null" },
        )
        .args([
            "-c",
            "init.templateDir=",
            "-c",
            "core.autocrlf=false",
            "-c",
            "core.hooksPath=",
            "-c",
            "user.name=Owned R5 fixture",
            "-c",
            "user.email=owned-r5@example.invalid",
        ])
        .args(args)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "fixture Git failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap().trim().into()
}

fn write_sources(root: &Path, sources: &[(&str, &str)]) {
    for (path, text) in sources {
        let absolute = root.join(path);
        fs::create_dir_all(absolute.parent().unwrap()).unwrap();
        fs::write(absolute, text).unwrap();
    }
}

pub struct Fixture {
    pub dir: TempDir,
    pub engine: Engine,
    pub report: Value,
    pub base: BTreeMap<String, String>,
    pub head: BTreeMap<String, String>,
    pub index_bytes: Vec<u8>,
    _permit: Permit,
}

impl Fixture {
    pub fn new(base: &[(&str, &str)], head: &[(&str, &str)]) -> Self {
        Self::new_with_options(base, head, CaptureOptions::default())
    }

    pub fn new_with_options(
        base: &[(&str, &str)],
        head: &[(&str, &str)],
        capture_options: CaptureOptions,
    ) -> Self {
        Self::new_with_profile(
            base,
            head,
            capture_options,
            super::test_yoke::analysis::AnalyzerEpoch::active().unwrap(),
        )
    }

    pub fn new_with_profile(
        base: &[(&str, &str)],
        head: &[(&str, &str)],
        capture_options: CaptureOptions,
        epoch: super::test_yoke::analysis::AnalyzerEpoch,
    ) -> Self {
        let permit = Permit::acquire();
        let dir = TempDir::new().unwrap();
        git(dir.path(), &["init", "-b", "main"]);
        write_sources(dir.path(), base);
        // A retained tracked policy file permits genuine all-addition/deletion fixtures.
        fs::write(dir.path().join(".gitignore"), ".yoke/\n").unwrap();
        git(dir.path(), &["add", "--all"]);
        git(dir.path(), &["commit", "-m", "owned base"]);
        let base_commit = git(dir.path(), &["rev-parse", "HEAD"]);
        for (path, _) in base {
            fs::remove_file(dir.path().join(path)).unwrap();
        }
        write_sources(dir.path(), head);
        let index_bytes = fs::read(dir.path().join(".git/index")).unwrap();
        let mut engine = Engine::open(dir.path()).unwrap();
        engine.capture_options = capture_options;
        engine.capture_epoch = epoch;
        let report = git_workflows::analyze(
            &mut engine,
            &HostGit {
                executable: executable(),
            },
            &WorkflowOptions {
                base_ref: base_commit,
                history_limit: 1,
                branch_limit: 0,
                ..Default::default()
            },
            &Deadline::new(30_000),
        )
        .expect("genuine released Git workflow report");
        assert_eq!(
            fs::read(dir.path().join(".git/index")).unwrap(),
            index_bytes
        );
        Self {
            dir,
            engine,
            report,
            base: base
                .iter()
                .map(|(path, text)| ((*path).into(), (*text).into()))
                .collect(),
            head: head
                .iter()
                .map(|(path, text)| ((*path).into(), (*text).into()))
                .collect(),
            index_bytes,
            _permit: permit,
        }
    }

    pub fn moved() -> Self {
        Self::new(
            &[("src/alt-ü.rs", MOVED_BASE)],
            &[("src/neu-λ.rs", MOVED_HEAD)],
        )
    }

    pub fn moved_scoped() -> Self {
        Self::new_with_options(
            &[("src/alt-ü.rs", MOVED_BASE)],
            &[("src/neu-λ.rs", MOVED_HEAD)],
            CaptureOptions {
                only_dirs: vec!["src".into()],
                ..Default::default()
            },
        )
    }

    pub fn id(&self) -> &str {
        self.report["artifactId"].as_str().unwrap()
    }

    pub fn text(&self, side: &str, path: &str) -> &str {
        match side {
            "base" => self.base.get(path),
            "head" => self.head.get(path),
            _ => panic!("invalid fixture source side"),
        }
        .expect("literal fixture source")
    }

    pub fn source_for_quote(&self, quote: &Value) -> &str {
        let side = if quote["snapshotId"] == self.report["baseSnapshotId"] {
            "base"
        } else {
            assert_eq!(quote["snapshotId"], self.report["headSnapshotId"]);
            "head"
        };
        self.text(side, quote["path"].as_str().unwrap())
    }
}

pub fn expected_range(text: &str, start: usize, end: usize) -> Value {
    assert!(start <= end && end <= text.len());
    assert!(text.is_char_boundary(start) && text.is_char_boundary(end));
    let line = |offset: usize| {
        1 + text.as_bytes()[..offset]
            .iter()
            .filter(|b| **b == b'\n')
            .count()
    };
    json!({"startByte":start,"endByte":end,"startLine":line(start),"endLine":if start == end { line(start) } else { line(end-1) }})
}

pub fn assert_complete_quote(fixture: &Fixture, quote: &Value, anchor: &Value) {
    let text = fixture.source_for_quote(quote);
    let start = anchor["startByte"].as_u64().unwrap() as usize;
    let end = anchor["endByte"].as_u64().unwrap() as usize;
    assert_eq!(quote["snapshotId"], anchor["snapshotId"]);
    assert_eq!(quote["path"], anchor["path"]);
    assert_eq!(quote["contentHash"], hash(text.as_bytes()));
    assert_eq!(quote["requested"], expected_range(text, start, end));
    assert_eq!(quote["range"], expected_range(text, start, end));
    assert_eq!(quote["text"], &text[start..end]);
    assert_eq!(quote["status"], "complete");
    assert!(quote["reason"].is_null());
}

pub fn all_quotes(details: &Value) -> Vec<&Value> {
    let mut quotes = Vec::new();
    for change in details["changes"].as_array().unwrap() {
        for hunk in change["hunks"].as_array().unwrap() {
            for side in ["base", "head"] {
                if !hunk[side].is_null() {
                    quotes.push(&hunk[side]);
                }
            }
        }
    }
    for collection in ["changedSymbols", "impact"] {
        for row in details[collection].as_array().unwrap() {
            quotes.push(&row["source"]);
        }
    }
    for row in details["impactObservations"].as_array().unwrap() {
        for evidence in row["evidence"].as_array().unwrap() {
            quotes.push(&evidence["source"]);
        }
    }
    quotes
}

pub fn assert_originals_and_complete_quotes(fixture: &Fixture, details: &Value) {
    for field in ["changes", "changedSymbols", "impact", "impactObservations"] {
        let originals = fixture.report[field].as_array().unwrap();
        let rows = details[field].as_array().unwrap();
        assert_eq!(
            rows.len(),
            originals.len(),
            "complete {field} occurrence list"
        );
        for (index, (row, original)) in rows.iter().zip(originals).enumerate() {
            assert_eq!(row["index"], index);
            assert_eq!(
                &row["original"], original,
                "no original occurrence/claim rewrite"
            );
            match field {
                "changes" => {
                    let expected = original["hunks"].as_array().unwrap();
                    let hunks = row["hunks"].as_array().unwrap();
                    assert_eq!(hunks.len(), expected.len());
                    for (i, (hunk, anchors)) in hunks.iter().zip(expected).enumerate() {
                        assert_eq!(hunk["index"], i);
                        for side in ["base", "head"] {
                            if anchors[side].is_null() {
                                assert!(hunk[side].is_null());
                            } else {
                                assert_complete_quote(fixture, &hunk[side], &anchors[side]);
                            }
                        }
                    }
                }
                "impactObservations" => {
                    let expected = original["evidence"].as_array().unwrap();
                    let evidence = row["evidence"].as_array().unwrap();
                    assert_eq!(evidence.len(), expected.len());
                    for (i, (quote, anchor)) in evidence.iter().zip(expected).enumerate() {
                        assert_eq!(quote["index"], i);
                        assert_eq!(quote["evidenceId"], anchor["id"]);
                        assert_complete_quote(fixture, &quote["source"], anchor);
                    }
                }
                _ => assert_complete_quote(fixture, &row["source"], original),
            }
        }
    }
}

pub fn rows(value: &Value) -> usize {
    match value {
        Value::Array(items) => items.len() + items.iter().map(rows).sum::<usize>(),
        Value::Object(fields) => fields.values().map(rows).sum(),
        _ => 0,
    }
}

pub fn sorted(value: &Value) -> Value {
    match value {
        Value::Object(fields) => {
            let ordered: BTreeMap<_, _> = fields
                .iter()
                .map(|(key, value)| (key.clone(), sorted(value)))
                .collect();
            serde_json::to_value(ordered).unwrap()
        }
        Value::Array(items) => Value::Array(items.iter().map(sorted).collect()),
        _ => value.clone(),
    }
}

pub fn state(engine: &Engine) -> Value {
    let mut tables = BTreeMap::new();
    let names: Vec<String> = engine.store.db.prepare(
        "SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%' ORDER BY name"
    ).unwrap().query_map([], |row| row.get(0)).unwrap().collect::<rusqlite::Result<_>>().unwrap();
    for name in names {
        let quoted = name.replace('"', "\"\"");
        let mut query = engine
            .store
            .db
            .prepare(&format!("SELECT * FROM \"{quoted}\""))
            .unwrap();
        let columns = query.column_count();
        let mut records: Vec<String> = query
            .query_map([], |row| {
                let values: Vec<Value> = (0..columns)
                    .map(|i| match row.get_ref(i).unwrap() {
                        ValueRef::Null => json!(["null"]),
                        ValueRef::Integer(value) => json!(["integer", value]),
                        ValueRef::Real(value) => json!(["real", value.to_bits().to_string()]),
                        ValueRef::Text(value) => json!(["text", hex::encode(value)]),
                        ValueRef::Blob(value) => json!(["blob", hex::encode(value)]),
                    })
                    .collect();
                Ok(serde_json::to_string(&values).unwrap())
            })
            .unwrap()
            .collect::<rusqlite::Result<_>>()
            .unwrap();
        records.sort();
        tables.insert(name, records);
    }
    let blobs: BTreeMap<String, String> = fs::read_dir(engine.store.dir.join("blobs"))
        .unwrap()
        .map(|entry| {
            let path = entry.unwrap().path();
            assert!(path.is_file());
            (
                path.file_name().unwrap().to_str().unwrap().to_owned(),
                hash(fs::read(path).unwrap()),
            )
        })
        .collect();
    let total_changes: i64 = engine
        .store
        .db
        .query_row("SELECT total_changes()", [], |row| row.get(0))
        .unwrap();
    json!({"tables":tables,"blobs":blobs,"totalChanges":total_changes,"autocommit":engine.store.db.is_autocommit()})
}

pub fn assert_static_error(error: anyhow::Error, marker: &str) {
    let chain: Vec<_> = error.chain().map(ToString::to_string).collect();
    assert_eq!(chain, [marker]);
    let diagnostic = format!("{error:#?}");
    for sentinel in [
        "R5_SCOPE_DENIAL_SENTINEL",
        "R5_TAMPER_SENTINEL",
        "café",
        "alt-ü",
        "neu-λ",
        "SELECT",
        "Win32",
        "os error",
    ] {
        assert!(
            !diagnostic.contains(sentinel),
            "error chain leaked {sentinel}"
        );
    }
}
