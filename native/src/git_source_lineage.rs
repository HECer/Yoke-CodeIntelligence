//! Bound the complete metadata lineage before the general capture resolver runs.
use super::super::{SOURCE_KIND, SourceReceipt, record_value};
use crate::{
    capture::{self, Deadline, DerivedCaptureOptionsRecord},
    model::{Snapshot, valid_id},
    store::Store,
};
use anyhow::{Result, ensure};
use rusqlite::OptionalExtension;

#[derive(Debug)]
pub(crate) struct LineageCapacity;
impl std::fmt::Display for LineageCapacity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Git source lineage metadata capacity exceeded")
    }
}
impl std::error::Error for LineageCapacity {}

struct Budget<'a> {
    store: &'a Store,
    d: &'a Deadline,
    remaining: usize,
}
impl Budget<'_> {
    fn charge(&mut self, bytes: usize) -> Result<()> {
        self.d.check()?;
        ensure!(bytes <= self.remaining, LineageCapacity);
        self.remaining -= bytes;
        Ok(())
    }
    fn record(&mut self, kind: &str, id: &str) -> Result<Option<usize>> {
        self.d.check()?;
        let bytes: Option<usize> = self
            .store
            .db
            .query_row(
                "SELECT length(CAST(value AS BLOB)) FROM records WHERE kind=?1 AND id=?2",
                rusqlite::params![kind, id],
                |row| row.get(0),
            )
            .optional()?;
        if let Some(bytes) = bytes {
            self.charge(bytes)?;
        }
        Ok(bytes)
    }
    fn absent(&self, kind: &str, id: &str) -> Result<()> {
        self.d.check()?;
        let exists: bool = self.store.db.query_row(
            "SELECT EXISTS(SELECT 1 FROM records WHERE kind=?1 AND id=?2)",
            rusqlite::params![kind, id],
            |row| row.get(0),
        )?;
        ensure!(!exists, "Git preview requires a direct disk capture origin");
        Ok(())
    }
}

fn disk_records(budget: &mut Budget<'_>, snapshot: &Snapshot) -> Result<()> {
    ensure!(
        snapshot.kind == "disk",
        "Git preview requires a direct disk capture origin"
    );
    ensure!(snapshot.files.len() <= 10_000, LineageCapacity);
    // A disk kind alone is insufficient: inherited proof rows take precedence
    // over that kind in the general capture resolver.
    for kind in [
        capture::DERIVED_OPTIONS_RECORD_KIND,
        "overlay-snapshot",
        SOURCE_KIND,
    ] {
        budget.absent(kind, &snapshot.id)?;
    }
    for kind in [capture::OPTIONS_RECORD_KIND, capture::IGNORED_RECORD_KIND] {
        budget.record(kind, &snapshot.id)?;
    }
    budget.d.check()
}

pub(crate) fn disk_preflight(
    store: &Store,
    snapshot: &Snapshot,
    maximum: usize,
    d: &Deadline,
) -> Result<usize> {
    ensure!(maximum <= 16_000_000, LineageCapacity);
    let mut budget = Budget {
        store,
        d,
        remaining: maximum,
    };
    disk_records(&mut budget, snapshot)?;
    Ok(maximum - budget.remaining)
}

pub(super) fn preflight(
    store: &Store,
    snapshot: &Snapshot,
    maximum: usize,
    d: &Deadline,
) -> Result<usize> {
    ensure!(maximum <= 16_000_000, LineageCapacity);
    let mut budget = Budget {
        store,
        d,
        remaining: maximum,
    };
    // These rows could select a different capture resolver branch.
    for kind in [
        capture::OPTIONS_RECORD_KIND,
        capture::IGNORED_RECORD_KIND,
        "overlay-snapshot",
    ] {
        budget.absent(kind, &snapshot.id)?;
    }
    let size = budget
        .record(SOURCE_KIND, &snapshot.id)?
        .ok_or_else(|| anyhow::anyhow!("Git source receipt unavailable"))?;
    let receipt: SourceReceipt =
        crate::json::decode(record_value(store, SOURCE_KIND, &snapshot.id, size, d)?)?;
    let size = budget
        .record(capture::DERIVED_OPTIONS_RECORD_KIND, &snapshot.id)?
        .ok_or_else(|| anyhow::anyhow!("Git capture inheritance unavailable"))?;
    let inheritance: DerivedCaptureOptionsRecord = crate::json::decode(record_value(
        store,
        capture::DERIVED_OPTIONS_RECORD_KIND,
        &snapshot.id,
        size,
        d,
    )?)?;
    let origin_id = &receipt.origin_snapshot_id;
    ensure!(
        valid_id(origin_id) && inheritance.origin_snapshot_id == *origin_id,
        "Git source capture origin mismatch"
    );
    let bytes: usize = store.db.query_row(
        "SELECT length(CAST(value AS BLOB)) FROM snapshots WHERE id=?1",
        [origin_id],
        |row| row.get(0),
    )?;
    budget.charge(bytes)?;
    let raw: Vec<u8> = store.db.query_row(
        "SELECT substr(CAST(value AS BLOB),1,?2) FROM snapshots WHERE id=?1",
        rusqlite::params![origin_id, bytes + 1],
        |row| row.get(0),
    )?;
    ensure!(raw.len() == bytes, "Git capture origin changed during read");
    let origin: Snapshot = crate::json::decode(crate::json::parse(&raw, bytes)?)?;
    d.check()?;
    ensure!(
        origin.id == *origin_id
            && origin.workspace_id == store.workspace_id
            && origin.kind == "disk",
        "Git preview requires a direct disk capture origin"
    );
    disk_records(&mut budget, &origin)?;
    Ok(maximum - budget.remaining)
}
