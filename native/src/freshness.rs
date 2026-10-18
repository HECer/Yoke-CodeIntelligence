//! Disk drift relative to the last completed disk index, without publication.
use crate::{
    Engine,
    capture::{self, Deadline},
    model::identity,
};
use anyhow::{Result, ensure};
use rusqlite::OptionalExtension;
use serde_json::{Value, json};
use std::collections::BTreeMap;

pub const BASELINE_KEY: &str = "last_disk_index_snapshot_id";
pub fn inspect(engine: &Engine, deadline: &Deadline) -> Result<Value> {
    engine.store.set_deadline(deadline)?;
    let baseline: Option<String> = engine
        .store
        .db
        .query_row("SELECT value FROM meta WHERE key=?1", [BASELINE_KEY], |r| {
            r.get(0)
        })
        .optional()?;
    let Some(id) = baseline else {
        return Ok(
            json!({"indexed":false,"fresh":null,"baselineSnapshotId":null,"reason":"No completed disk index checkpoint; run index explicitly."}),
        );
    };
    let before = engine.store.snapshot(&id)?;
    let graph = engine.store.graph(&id)?;
    crate::query_features::source_scope(&engine.store, &before, &graph, deadline)?;
    let options = capture::options_for_snapshot(&engine.store, &before)?;
    ensure!(
        options == engine.capture_options.validated()?,
        "Freshness capture scope differs from disk index; index the new host scope explicitly"
    );
    let epoch =
        capture::analyzer_epoch_for_snapshot_with_deadline(&engine.store, &before, deadline)?;
    ensure!(
        epoch == engine.capture_epoch,
        "Freshness analyzer differs from disk index; index the new host policy explicitly"
    );
    let observed = capture::capture_for_snapshot(&engine.root, &engine.store, &before, deadline)?;
    let old: BTreeMap<_, _> = before.files.iter().map(|f| (f.path.as_str(), f)).collect();
    let new: BTreeMap<_, _> = observed
        .files
        .iter()
        .map(|f| (f.path.as_str(), f))
        .collect();
    let mut added = Vec::new();
    let mut changed = Vec::new();
    let mut deleted = Vec::new();
    for (path, file) in &new {
        deadline.check()?;
        if let Some(prior) = old.get(path) {
            if prior.hash != file.hash || prior.mode != file.mode || prior.language != file.language
            {
                changed.push(json!({"path":path,"beforeHash":prior.hash,"afterHash":file.hash,"beforeMode":prior.mode,"afterMode":file.mode}));
            }
        } else {
            added.push(json!({"path":path,"contentHash":file.hash}));
        }
    }
    for (path, file) in &old {
        deadline.check()?;
        if !new.contains_key(path) {
            deleted.push(json!({"path":path,"contentHash":file.hash}));
        }
    }
    let omissions_changed = identity(&before.omissions) != identity(&observed.omissions);
    Ok(
        json!({"indexed":true,"fresh":added.is_empty() && changed.is_empty() && deleted.is_empty() && !omissions_changed,"baselineSnapshotId":id,"observedCaptureId":observed.id,"added":added,"changed":changed,"deleted":deleted,"omissionsChanged":omissions_changed,"omissions":observed.omissions,"comparison":"content_hash_language_and_mode","captureConsistency":observed.capture_consistency,"indexPublished":false,"scope":"captured_source","complete":false}),
    )
}
