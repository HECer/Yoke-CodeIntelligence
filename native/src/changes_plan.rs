//! Replay the original native plan identity and complete captured byte delta.
//! A saved review or approval ID must not authorize a different stored effect.
use super::{Plan, PlannedFile};
use crate::{
    capture::{self, Deadline},
    model::{Snapshot, SourceFile, identity, valid_id},
    store::Store,
};
use anyhow::{Context, Result, ensure};
use rusqlite::{Transaction, TransactionBehavior, limits::Limit};
use serde_json::Value;
use similar::TextDiff;
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

const INVALID: &str = "changes_plan_invalid";
const CAPACITY: &str = "changes_plan_capacity_exceeded";
const MAX_PLAN_BYTES: usize = 64_000_000;
const MAX_SNAPSHOT_BYTES: usize = 32_000_000;
const MAX_SOURCE_BYTES: usize = 128_000_000;

pub(super) fn plan_identity(plan: &Plan) -> String {
    let original = if let Some(directory) = &plan.directory_move {
        identity(&(
            "directory-move-v1",
            &plan.base_snapshot_id,
            &plan.result_snapshot_id,
            &plan.files,
            plan.created_at,
            directory,
        ))
    } else {
        identity(&(
            &plan.base_snapshot_id,
            &plan.result_snapshot_id,
            &plan.files,
            plan.created_at,
        ))
    };
    let original = if let Some(ending) = plan.source_line_ending {
        identity(&(original, "source-line-ending-v1", ending))
    } else {
        original
    };
    if let Some(encoding) = plan.source_encoding {
        identity(&(original, crate::source_encoding::VERSION, encoding))
    } else {
        original
    }
}

fn raw(store: &Store, snapshot: bool, id: &str, maximum: usize, d: &Deadline) -> Result<Value> {
    d.check()?;
    ensure!(valid_id(id), INVALID);
    let bytes: Vec<u8> = if snapshot {
        store.db.query_row(
            "SELECT substr(CAST(value AS BLOB),1,?2) FROM snapshots WHERE id=?1",
            rusqlite::params![id, maximum + 1],
            |r| r.get(0),
        )?
    } else {
        store.db.query_row(
            "SELECT substr(CAST(value AS BLOB),1,?2) FROM records WHERE kind='plan' AND id=?1",
            rusqlite::params![id, maximum + 1],
            |r| r.get(0),
        )?
    };
    ensure!(bytes.len() <= maximum, CAPACITY);
    crate::json::parse(&bytes, maximum)
}
fn files<'a>(snapshot: &'a Snapshot, d: &Deadline) -> Result<BTreeMap<&'a str, &'a SourceFile>> {
    ensure!(snapshot.files.len() <= 100_000, CAPACITY);
    let mut result = BTreeMap::new();
    let mut prior = None;
    for file in &snapshot.files {
        d.check()?;
        ensure!(
            crate::paths::normalized_syntax(&file.path)? == file.path
                && valid_id(&file.hash)
                && file.size <= 4_000_000
                && prior.is_none_or(|p: &str| p < file.path.as_str()),
            INVALID
        );
        prior = Some(file.path.as_str());
        result.insert(file.path.as_str(), file);
    }
    Ok(result)
}
fn serialized_limit<T: serde::Serialize>(value: &T, maximum: usize, d: &Deadline) -> Result<()> {
    struct Counter<'a> {
        remaining: usize,
        deadline: &'a Deadline,
    }
    impl std::io::Write for Counter<'_> {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.deadline.check().map_err(std::io::Error::other)?;
            self.remaining = self
                .remaining
                .checked_sub(bytes.len())
                .ok_or_else(|| std::io::Error::other(CAPACITY))?;
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let result = serde_json::to_writer(
        Counter {
            remaining: maximum,
            deadline: d,
        },
        value,
    );
    d.check()?;
    ensure!(result.is_ok(), CAPACITY);
    Ok(())
}
/// Refuse plans outside the reader's finite contract before publishing a result.
/// CAS writes already made while constructing edits retain their existing semantics.
pub(super) fn admit_prepared(
    plan: &Plan,
    base: &Snapshot,
    result: &Snapshot,
    d: &Deadline,
) -> Result<()> {
    ensure!(
        !plan.files.is_empty() && plan.files.len() <= 2000 && plan.diff.len() <= 16_000_000,
        CAPACITY
    );
    let before = files(base, d)?;
    let after = files(result, d)?;
    let mut objects = BTreeMap::new();
    let mut admitted = 0usize;
    for file in &plan.files {
        d.check()?;
        for (members, id) in [(&before, &file.before_hash), (&after, &file.after_hash)] {
            if let Some(id) = id {
                let member = members.get(file.path.as_str()).context(INVALID)?;
                ensure!(member.hash == *id, INVALID);
                if let Some(size) = objects.insert(id, member.size) {
                    ensure!(size == member.size, INVALID);
                } else {
                    admitted = admitted.checked_add(member.size).context(CAPACITY)?;
                    ensure!(admitted <= MAX_SOURCE_BYTES, CAPACITY);
                }
            }
        }
    }
    serialized_limit(base, MAX_SNAPSHOT_BYTES, d)?;
    serialized_limit(result, MAX_SNAPSHOT_BYTES, d)?;
    serialized_limit(plan, MAX_PLAN_BYTES, d)
}
fn binding(
    plan: &Plan,
    base: &Snapshot,
    result: &Snapshot,
    store: &Store,
    d: &Deadline,
) -> Result<()> {
    ensure!(
        plan.workspace_id == store.workspace_id
            && base.workspace_id == store.workspace_id
            && result.workspace_id == store.workspace_id
            && plan.base_snapshot_id == base.id
            && plan.result_snapshot_id == result.id
            && result.kind == "derived"
            && result.capture_consistency == "derived-from-immutable"
            && serde_json::to_value(&result.omissions)? == serde_json::to_value(&base.omissions)?,
        INVALID
    );
    let epoch = capture::analyzer_epoch_for_snapshot_with_deadline(store, base, d)?;
    ensure!(
        plan.source_encoding == crate::source_encoding::selected(store, base)?
            && plan.source_encoding == crate::source_encoding::selected(store, result)?
            && plan.source_encoding
                == plan
                    .source_encoding
                    .and_then(crate::source_encoding::Policy::canonical),
        INVALID
    );
    if let Some(directory) = &plan.directory_move {
        directory.validate_result(base, result, d)?;
    }
    ensure!(
        epoch.derived_snapshot_id(base, &result.files) == result.id,
        INVALID
    );
    let before = files(base, d)?;
    let after = files(result, d)?;
    let paths: BTreeSet<_> = before.keys().chain(after.keys()).copied().collect();
    let mut expected = vec![];
    for path in paths {
        d.check()?;
        let b = before.get(path).copied();
        let a = after.get(path).copied();
        if b.map(|f| &f.hash) == a.map(|f| &f.hash) {
            if let (Some(b), Some(a)) = (b, a) {
                ensure!(b == a, INVALID);
            }
            continue;
        }
        let moved = plan
            .directory_move
            .as_ref()
            .and_then(|directory| directory.source_for_destination(path));
        let expected_mode = b.or(moved).map_or(0o644, |f| f.mode);
        expected.push(PlannedFile {
            path: path.into(),
            before_hash: b.map(|f| f.hash.clone()),
            after_hash: a.map(|f| f.hash.clone()),
            mode: expected_mode,
        });
        if let Some(a) = a {
            ensure!(a.mode == expected_mode, INVALID);
        }
    }
    ensure!(
        !expected.is_empty()
            && expected.len() <= 2000
            && serde_json::to_value(&plan.files)? == serde_json::to_value(&expected)?,
        INVALID
    );
    Ok(())
}
fn review(
    plan: &Plan,
    base: &Snapshot,
    result: &Snapshot,
    store: &Store,
    d: &Deadline,
) -> Result<()> {
    let before = capture::source_scope_policy(store, base)?;
    let after = capture::source_scope_policy(store, result)?;
    let mut objects = BTreeMap::<String, usize>::new();
    let mut admitted = 0usize;
    // Admit all selected byte counts and both path policies before body reads.
    for file in &plan.files {
        d.check()?;
        before.check_host_write_scope(&file.path)?;
        for (scope, id) in [(&before, &file.before_hash), (&after, &file.after_hash)] {
            if let Some(id) = id {
                let member = scope.file(&file.path)?;
                ensure!(member.hash == *id, INVALID);
                if let Some(size) = objects.get(id) {
                    ensure!(*size == member.size, INVALID);
                } else {
                    admitted = admitted.checked_add(member.size).context(CAPACITY)?;
                    ensure!(admitted <= MAX_SOURCE_BYTES, CAPACITY);
                    objects.insert(id.clone(), member.size);
                }
            }
        }
    }
    let mut texts = BTreeMap::<String, Arc<str>>::new();
    let mut diff = String::new();
    let mut warnings = vec![];
    for file in &plan.files {
        d.check()?;
        let mut pair = [Arc::<str>::from(""), Arc::<str>::from("")];
        for (i, scope, id) in [
            (0, &before, &file.before_hash),
            (1, &after, &file.after_hash),
        ] {
            if let Some(id) = id {
                let text = if let Some(text) = texts.get(id) {
                    Arc::clone(text)
                } else {
                    let bytes = scope.read_source(&file.path)?;
                    ensure!(objects[id] == bytes.len(), INVALID);
                    let view =
                        crate::source_encoding::View::decode(bytes, plan.source_encoding, &|| {
                            d.check()
                        })?;
                    ensure!(
                        !view.text().contains('\0') && !crate::paths::obvious_secret(view.text()),
                        INVALID
                    );
                    let text: Arc<str> = Arc::from(view.into_text());
                    texts.insert(id.clone(), Arc::clone(&text));
                    text
                };
                pair[i] = text;
            }
        }
        if let Some(ending) = plan.source_line_ending {
            ensure!(ending.canonical(pair[1].as_bytes(), d)?, INVALID);
        }
        if plan.source_encoding.is_some() {
            crate::source_encoding::append_diff(
                &mut diff,
                pair[0].as_ref(),
                pair[1].as_ref(),
                if file.before_hash.is_some() {
                    &file.path
                } else {
                    "/dev/null"
                },
                if file.after_hash.is_some() {
                    &file.path
                } else {
                    "/dev/null"
                },
                d,
            )?;
        } else {
            let addition = TextDiff::from_lines(pair[0].as_ref(), pair[1].as_ref())
                .unified_diff()
                .context_radius(3)
                .header(
                    if file.before_hash.is_some() {
                        &file.path
                    } else {
                        "/dev/null"
                    },
                    if file.after_hash.is_some() {
                        &file.path
                    } else {
                        "/dev/null"
                    },
                )
                .to_string();
            ensure!(
                addition.len() <= 16_000_000usize.saturating_sub(diff.len()),
                CAPACITY
            );
            diff.push_str(&addition);
        }
        let folded = file.path.to_ascii_lowercase();
        if folded.contains("test") || folded.contains("spec") {
            warnings.push(format!(
                "Test-like file modified: {}; host approval must include test changes",
                file.path
            ));
        }
    }
    ensure!(plan.diff == diff && plan.warnings == warnings, INVALID);
    d.check()
}
pub(super) fn load(store: &Store, id: &str, d: &Deadline) -> Result<Plan> {
    let d = store.inherited_deadline(d)?;
    let run = || -> Result<Plan> {
        crate::github_app::worker::publication_policy(store, &d, |_| {
            let prior = store.db.limit(Limit::SQLITE_LIMIT_LENGTH)?;
            store
                .db
                .set_limit(Limit::SQLITE_LIMIT_LENGTH, prior.min(64_004_096))?;
            let tx = if store.db.is_autocommit() {
                Some(Transaction::new_unchecked(
                    &store.db,
                    TransactionBehavior::Deferred,
                )?)
            } else {
                None
            };
            let plan: Plan = crate::json::decode(raw(store, false, id, MAX_PLAN_BYTES, &d)?)?;
            ensure!(
                plan.id == id
                    && valid_id(id)
                    && plan.workspace_id == store.workspace_id
                    && plan.created_at.checked_add(86400) == Some(plan.expires_at)
                    && plan.id == plan_identity(&plan),
                INVALID
            );
            let base: Snapshot = crate::json::decode(raw(
                store,
                true,
                &plan.base_snapshot_id,
                MAX_SNAPSHOT_BYTES,
                &d,
            )?)?;
            let result: Snapshot = crate::json::decode(raw(
                store,
                true,
                &plan.result_snapshot_id,
                MAX_SNAPSHOT_BYTES,
                &d,
            )?)?;
            binding(&plan, &base, &result, store, &d)?;
            review(&plan, &base, &result, store, &d)?;
            d.check()?;
            if let Some(tx) = tx {
                tx.commit()?;
            }
            Ok(plan)
        })
    };
    run().map_err(|error| {
        if d.check().is_err() {
            anyhow::anyhow!("changes_plan_deadline_or_cancelled")
        } else if error.to_string() == CAPACITY
            || error.chain().any(|e| matches!(e.downcast_ref::<rusqlite::Error>(),
                Some(rusqlite::Error::SqliteFailure(code,_)) if code.extended_code == rusqlite::ffi::SQLITE_TOOBIG)) {
            anyhow::anyhow!(CAPACITY)
        } else {
            anyhow::anyhow!(INVALID)
        }
    })
}

#[cfg(test)]
mod capacity_tests {
    use super::*;
    #[test]
    fn manifest_count_preserves_the_native_capture_ceiling() {
        let mut snapshot = Snapshot {
            id: "1".repeat(64),
            workspace_id: "owned-metadata-fixture".into(),
            kind: "disk".into(),
            omissions: vec![],
            capture_consistency: "fixture".into(),
            files: (0..100_000)
                .map(|i| SourceFile {
                    path: format!("src/f{i:06}.rs"),
                    hash: "2".repeat(64),
                    size: 1,
                    language: "rust".into(),
                    mode: 0o644,
                })
                .collect(),
        };
        let d = Deadline::new(30_000);
        assert_eq!(files(&snapshot, &d).unwrap().len(), 100_000);
        snapshot.files.push(SourceFile {
            path: "src/overflow.rs".into(),
            ..snapshot.files[0].clone()
        });
        assert_eq!(files(&snapshot, &d).err().unwrap().to_string(), CAPACITY);
    }
    #[test]
    fn serialized_admission_counts_escaped_json_bytes_and_the_exact_boundary() {
        let value = "\u{1}\r\ncafé🦀";
        let exact = serde_json::to_vec(&value).unwrap().len();
        let d = Deadline::new(30_000);
        serialized_limit(&value, exact, &d).unwrap();
        assert_eq!(
            serialized_limit(&value, exact - 1, &d)
                .unwrap_err()
                .to_string(),
            CAPACITY
        );
        assert!(serialized_limit(&value, exact, &Deadline::new(0)).is_err());
    }
}
