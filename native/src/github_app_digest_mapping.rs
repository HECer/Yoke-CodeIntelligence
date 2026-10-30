//! Associations over explicitly selected immutable captures, never source acquisition.
use super::{CAPACITY, Options, PreviewRequest, SOURCE, source};
use crate::{
    capture::Deadline, git_workflows::history_read::History, model::identity, store::Store,
};
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone)]
pub struct MappingOptions {
    pub max_snapshots: usize,
    pub max_file_relations: usize,
    pub max_symbol_relations: usize,
    pub max_comparisons: usize,
    pub max_proof_bytes: usize,
}
impl Default for MappingOptions {
    fn default() -> Self {
        Self {
            max_snapshots: 32,
            max_file_relations: 20_000,
            max_symbol_relations: 8_000,
            max_comparisons: 200_000,
            max_proof_bytes: 32_000_000,
        }
    }
}
pub struct MappingSelection {
    /// Additional captured snapshot IDs; the request's head capture is implicit.
    pub snapshot_ids: Vec<String>,
    pub limits: MappingOptions,
}
impl MappingSelection {
    pub fn validate(&self, request: &PreviewRequest) -> Result<()> {
        let hard = MappingOptions::default();
        ensure!(
            self.limits.max_snapshots <= hard.max_snapshots
                && self.limits.max_file_relations <= hard.max_file_relations
                && self.limits.max_symbol_relations <= hard.max_symbol_relations
                && self.limits.max_comparisons <= hard.max_comparisons
                && self.limits.max_proof_bytes <= hard.max_proof_bytes
                && self.snapshot_ids.len() < self.limits.max_snapshots,
            CAPACITY
        );
        let mut seen = BTreeSet::from([request.snapshot_id.as_str()]);
        for id in &self.snapshot_ids {
            ensure!(
                crate::model::valid_id(id) && seen.insert(id.as_str()),
                "github_app_digest_input_invalid"
            );
        }
        Ok(())
    }
}

struct Budget {
    remaining: Options,
    files: usize,
    symbols: usize,
    comparisons: usize,
    proof_remaining: usize,
    row_bytes_remaining: usize,
}
impl Budget {
    fn consume(&mut self, projection: &source::Projection) -> Result<()> {
        // A truncated projection cannot establish that an unmatched export is absent.
        ensure!(projection.omissions == 0, CAPACITY);
        macro_rules! debit {
            ($field:ident, $amount:expr) => {
                self.remaining.$field = self
                    .remaining
                    .$field
                    .checked_sub($amount)
                    .context(CAPACITY)?;
            };
        }
        debit!(max_files, projection.usage.files);
        debit!(max_source_bytes, projection.usage.source_bytes);
        debit!(max_metadata_bytes, projection.usage.metadata_bytes);
        debit!(max_lineage_metadata_bytes, projection.usage.lineage_bytes);
        debit!(max_entities, projection.usage.entities);
        debit!(max_ast_nodes, projection.usage.ast_nodes);
        debit!(max_replay_name_bytes, projection.usage.replay_name_bytes);
        debit!(max_symbols, projection.symbols.len());
        self.proof_remaining = self
            .proof_remaining
            .checked_sub(projection.usage.proof_bytes)
            .context(CAPACITY)?;
        Ok(())
    }
    fn compare(&mut self, limits: &MappingOptions, d: &Deadline) -> Result<()> {
        d.check()?;
        ensure!(self.comparisons < limits.max_comparisons, CAPACITY);
        self.comparisons += 1;
        Ok(())
    }
    fn admit_row(&mut self, row: &Value, d: &Deadline) -> Result<()> {
        let bytes =
            crate::json::encode_bounded(row, self.row_bytes_remaining, d).context(CAPACITY)?;
        self.row_bytes_remaining -= bytes.len();
        Ok(())
    }
}
#[derive(Clone, Copy)]
struct Capture<'a> {
    id: &'a str,
    projection: &'a source::Projection,
}
impl Capture<'_> {
    fn commit(&self) -> Result<&str> {
        self.projection.commit_id.as_deref().context(SOURCE)
    }
    fn summary(&self) -> Result<Value> {
        Ok(json!({"snapshotId":self.id,"sourceCommitId":self.commit()?,
            "sourceTreeId":self.projection.tree_id,"analyzer":self.projection.analyzer,
            "capturedFiles":self.projection.usage.files,"exportedSymbols":self.projection.symbols.len(),
            "unsupportedFiles":self.projection.unsupported_files,
            "visibilityUnresolved":self.projection.visibility_unresolved,
            "sourceCommitVerified":true,"compilerVerified":false,"complete":false}))
    }
}
fn text<'a>(value: &'a Value, field: &str) -> Result<&'a str> {
    value[field].as_str().context(SOURCE)
}

#[allow(clippy::too_many_arguments)] // Pinned reader, selected head and independent host budgets.
pub(super) fn attach(
    store: &Store,
    options: &Options,
    request: &PreviewRequest,
    head: &source::Projection,
    history: &mut History,
    selection: &MappingSelection,
    d: &Deadline,
) -> Result<Value> {
    let mut allowed = BTreeSet::from([head.commit_id.clone().context(SOURCE)?]);
    for commit in &history.commits {
        d.check()?;
        allowed.insert(text(commit, "sha")?.to_owned());
        let parents = commit["parentIds"].as_array().context(SOURCE)?;
        ensure!(parents.len() <= 1, SOURCE);
        for parent in parents {
            allowed.insert(parent.as_str().context(SOURCE)?.to_owned());
        }
    }
    let mut budget = Budget {
        remaining: options.clone(),
        files: 0,
        symbols: 0,
        comparisons: 0,
        proof_remaining: selection.limits.max_proof_bytes,
        row_bytes_remaining: options.max_output_bytes,
    };
    budget.consume(head)?;
    let mut additional = Vec::new();
    // Canonical selection order makes accounting and output independent of CLI order.
    for id in selection.snapshot_ids.iter().collect::<BTreeSet<_>>() {
        d.check()?;
        let selected = PreviewRequest {
            snapshot_id: id.clone(),
            ..request.clone()
        };
        let projection = source::project_for_commits(
            &source::Context {
                store,
                options: &budget.remaining,
                proof_limit: Some(budget.proof_remaining),
            },
            &selected,
            Some(&allowed),
            d,
        )?;
        budget.consume(&projection)?;
        additional.push((id.as_str(), projection));
    }
    let mut by_commit = BTreeMap::new();
    for capture in std::iter::once(Capture {
        id: &request.snapshot_id,
        projection: head,
    })
    .chain(
        additional
            .iter()
            .map(|(id, projection)| Capture { id, projection }),
    ) {
        ensure!(
            by_commit
                .insert(capture.commit()?.to_owned(), capture)
                .is_none(),
            "github_app_digest_mapping_ambiguous_capture"
        );
    }
    let mut mapped = 0usize;
    let mut unavailable = 0usize;
    for commit in &mut history.commits {
        d.check()?;
        let sha = text(commit, "sha")?.to_owned();
        let parent = commit["parentIds"]
            .as_array()
            .context(SOURCE)?
            .first()
            .map(|v| v.as_str().context(SOURCE))
            .transpose()?;
        let Some(after) = by_commit.get(&sha).copied() else {
            unavailable += 1;
            commit["filesStatus"] = json!("unavailable_commit_snapshot");
            commit["symbolsStatus"] = json!("unavailable_commit_snapshot");
            continue;
        };
        let before = parent.and_then(|id| by_commit.get(id).copied());
        if parent.is_some() && before.is_none() {
            unavailable += 1;
            commit["filesStatus"] = json!("unavailable_parent_snapshot");
            commit["symbolsStatus"] = json!("unavailable_parent_snapshot");
            continue;
        }
        commit["files"] = json!(file_relations(
            before,
            after,
            &mut budget,
            &selection.limits,
            d
        )?);
        commit["symbols"] = json!(symbol_relations(
            before,
            after,
            &mut budget,
            &selection.limits,
            d
        )?);
        commit["filesStatus"] = json!("captured_path_union_verified");
        commit["symbolsStatus"] = json!("captured_rust_export_syntax");
        commit["mappedSnapshotId"] = json!(after.id);
        commit["parentSnapshotId"] = json!(before.map(|c| c.id));
        mapped += 1;
    }
    let status = if history.commits.is_empty() {
        "no_selected_commits"
    } else if unavailable == 0 {
        "selected_captures_mapped"
    } else {
        "partially_available_captures"
    };
    history.summary["filesStatus"] = json!(status);
    history.summary["symbolsStatus"] = json!(status);
    Ok(
        json!({"schemaVersion":1,"scope":"explicit_captured_path_union_and_rust_export_syntax",
        "snapshots":by_commit.values().map(Capture::summary).collect::<Result<Vec<_>>>()?,
        "mappedCommits":mapped,"unavailableCommits":unavailable,
        "filesStatus":status,"symbolsStatus":status,
        "fileRelations":budget.files,"symbolRelations":budget.symbols,"comparisons":budget.comparisons,
        "relationRowBytes":options.max_output_bytes-budget.row_bytes_remaining,
        "sourceBytesVerified":options.max_source_bytes-budget.remaining.max_source_bytes,
        "capturedFilesVerified":options.max_files-budget.remaining.max_files,
        "selectedMetadataBytesAdmitted":options.max_metadata_bytes-budget.remaining.max_metadata_bytes,
        "lineageMetadataBytesAdmitted":options.max_lineage_metadata_bytes-budget.remaining.max_lineage_metadata_bytes,
        "commitProofBytesAdmitted":selection.limits.max_proof_bytes-budget.proof_remaining,
        "astCapacityCharged":options.max_ast_nodes-budget.remaining.max_ast_nodes,
        "entityCapacityCharged":options.max_entities-budget.remaining.max_entities,
        "replayNameBytes":options.max_replay_name_bytes-budget.remaining.max_replay_name_bytes,
        "allTreeChangesEnumerated":false,"semanticIdentityEstablished":false,
        "compilerVerified":false,"providerVerified":false,"complete":false}),
    )
}

fn source_index(capture: Capture<'_>) -> Result<BTreeMap<&str, &Value>> {
    capture
        .projection
        .sources
        .iter()
        .map(|v| Ok((text(v, "path")?, v)))
        .collect()
}
fn file_anchor(
    capture: Option<Capture<'_>>,
    path: &str,
    sources: &BTreeMap<&str, &Value>,
) -> Result<Value> {
    let Some(capture) = capture else {
        return Ok(json!({"gitState":"absent_root_parent"}));
    };
    let entry = capture
        .projection
        .git_entries
        .as_ref()
        .context(SOURCE)?
        .get(path);
    let source = sources.get(path);
    Ok(json!({"snapshotId":capture.id,"commitId":capture.commit()?,
        "treeId":capture.projection.tree_id,"gitState":if entry.is_some(){"present"}else{"absent"},
        "gitObjectId":entry.map(|e| &e.object_id),"gitMode":entry.map(|e| &e.mode),
        "gitObjectType":entry.map(|e| &e.object_type),
        "captured":source.is_some(),"contentHash":source.map(|v| &v["contentHash"]),
        "sourceBytes":source.map(|v| &v["bytes"])}))
}
fn file_relations(
    before: Option<Capture<'_>>,
    after: Capture<'_>,
    budget: &mut Budget,
    limits: &MappingOptions,
    d: &Deadline,
) -> Result<Vec<Value>> {
    let old_sources = before.map(source_index).transpose()?.unwrap_or_default();
    let new_sources = source_index(after)?;
    let old_tree = before
        .map(|c| c.projection.git_entries.as_ref().context(SOURCE))
        .transpose()?;
    let new_tree = after.projection.git_entries.as_ref().context(SOURCE)?;
    let mut rows = Vec::new();
    for path in old_sources
        .keys()
        .chain(new_sources.keys())
        .copied()
        .collect::<BTreeSet<_>>()
    {
        budget.compare(limits, d)?;
        let old = old_tree.and_then(|tree| tree.get(path));
        let new = new_tree.get(path);
        let change = match (old, new) {
            (None, Some(_)) => "added",
            (Some(_), None) => "deleted",
            (Some(a), Some(b)) if a.object_id != b.object_id || a.object_type != b.object_type => {
                "modified"
            }
            (Some(a), Some(b)) if a.mode != b.mode => "mode_changed",
            (Some(_), Some(_)) => continue,
            (None, None) => anyhow::bail!(SOURCE),
        };
        ensure!(budget.files < limits.max_file_relations, CAPACITY);
        budget.files += 1;
        let old = file_anchor(before, path, &old_sources)?;
        let new = file_anchor(Some(after), path, &new_sources)?;
        let row = json!({"id":identity(&("native-digest-history-file-1",path,&old,&new)),
            "path":path,"change":change,"before":old,"after":new,
            "basis":"verified_git_tree_membership_and_objects","providerVerified":false});
        budget.admit_row(&row, d)?;
        rows.push(row);
    }
    Ok(rows)
}

#[derive(Clone, Copy)]
struct Symbol<'a> {
    capture: Capture<'a>,
    value: &'a Value,
    scope: &'a str,
}
impl Symbol<'_> {
    fn anchor(&self) -> Result<Value> {
        let mut value = self.value.clone();
        value["snapshotId"] = json!(self.capture.id);
        value["commitId"] = json!(self.capture.commit()?);
        value["qualifiedName"] = json!(self.scope);
        Ok(value)
    }
}
type Pair<'a> = (Vec<Symbol<'a>>, Vec<Symbol<'a>>);
struct Counterpart<'a> {
    capture: Option<Capture<'a>>,
    sources: BTreeMap<&'a str, &'a Value>,
}
impl<'a> Counterpart<'a> {
    fn new(capture: Option<Capture<'a>>) -> Result<Self> {
        Ok(Self {
            capture,
            sources: capture.map(source_index).transpose()?.unwrap_or_default(),
        })
    }
    fn status(&self, path: &str) -> Result<&'static str> {
        let Some(capture) = self.capture else {
            return Ok("absent_root_parent");
        };
        if !capture
            .projection
            .git_entries
            .as_ref()
            .context(SOURCE)?
            .contains_key(path)
        {
            return Ok("absent_git_path");
        }
        Ok(match self.sources.get(path) {
            None => "uncaptured_source",
            Some(source) if source["exportPolicyAvailable"] != true => "unsupported_export_policy",
            Some(_) => "captured_rust_export_syntax",
        })
    }
    fn anchor(&self, symbol: &Symbol<'_>) -> Result<Value> {
        let mut anchor = symbol.anchor()?;
        anchor["counterpartSourceStatus"] = json!(self.status(text(symbol.value, "path")?)?);
        Ok(anchor)
    }
}
struct Comparison<'a> {
    before: Counterpart<'a>,
    after: Counterpart<'a>,
}
fn symbols(capture: Capture<'_>) -> Result<Vec<Symbol<'_>>> {
    capture
        .projection
        .symbols
        .iter()
        .map(|value| {
            Ok(Symbol {
                capture,
                value,
                scope: capture
                    .projection
                    .symbol_scopes
                    .get(text(value, "id")?)
                    .context(SOURCE)?,
            })
        })
        .collect()
}
fn symbol_row(
    before: &[Symbol<'_>],
    after: &[Symbol<'_>],
    basis: &str,
    comparison: &Comparison<'_>,
    budget: &mut Budget,
    limits: &MappingOptions,
    d: &Deadline,
) -> Result<Value> {
    budget.compare(limits, d)?;
    ensure!(budget.symbols < limits.max_symbol_relations, CAPACITY);
    budget.symbols += 1;
    let old = before
        .iter()
        .map(|s| comparison.after.anchor(s))
        .collect::<Result<Vec<_>>>()?;
    let new = after
        .iter()
        .map(|s| comparison.before.anchor(s))
        .collect::<Result<Vec<_>>>()?;
    let relation = if before.is_empty() {
        if new.iter().any(|s| {
            matches!(
                s["counterpartSourceStatus"].as_str(),
                Some("uncaptured_source" | "unsupported_export_policy")
            )
        }) {
            "unavailable_before_export_projection"
        } else {
            "only_in_after_projection"
        }
    } else if after.is_empty() {
        if old.iter().any(|s| {
            matches!(
                s["counterpartSourceStatus"].as_str(),
                Some("uncaptured_source" | "unsupported_export_policy")
            )
        }) {
            "unavailable_after_export_projection"
        } else {
            "only_in_before_projection"
        }
    } else if before.len() != 1 || after.len() != 1 {
        "ambiguous_syntax_candidates"
    } else {
        "syntax_correspondence_candidate"
    };
    // The final ID has the same 64-byte representation as this placeholder.
    // Admit the row before hashing its potentially large duplicate groups.
    let mut row = json!({"id":"0".repeat(64),"relation":relation,"basis":basis,
        "declarationBytesEqual":if before.len()==1&&after.len()==1 {
            Some(text(before[0].value,"fingerprint")? == text(after[0].value,"fingerprint")?)
        } else { None },"semanticIdentityEstablished":false,"compilerVerified":false});
    row["before"] = Value::Array(old);
    row["after"] = Value::Array(new);
    budget.admit_row(&row, d)?;
    row["id"] = json!(identity(&(
        "native-digest-history-symbol-1",
        &row["before"],
        &row["after"],
        basis
    )));
    Ok(row)
}
fn symbol_relations(
    before: Option<Capture<'_>>,
    after: Capture<'_>,
    budget: &mut Budget,
    limits: &MappingOptions,
    d: &Deadline,
) -> Result<Vec<Value>> {
    let comparison = Comparison {
        before: Counterpart::new(before)?,
        after: Counterpart::new(Some(after))?,
    };
    let mut named = BTreeMap::<(String, String, String), Pair<'_>>::new();
    for (index, rows) in [
        before.map(symbols).transpose()?.unwrap_or_default(),
        symbols(after)?,
    ]
    .into_iter()
    .enumerate()
    {
        for symbol in rows {
            d.check()?;
            let key = (
                text(symbol.value, "path")?.into(),
                symbol.scope.into(),
                text(symbol.value, "kind")?.into(),
            );
            let entry = named.entry(key).or_default();
            if index == 0 {
                entry.0.push(symbol)
            } else {
                entry.1.push(symbol)
            }
        }
    }
    let mut rows = Vec::new();
    let mut remaining = BTreeMap::<(String, String), Pair<'_>>::new();
    for (_, (old, new)) in named {
        budget.compare(limits, d)?;
        if !old.is_empty() && !new.is_empty() {
            rows.push(symbol_row(
                &old,
                &new,
                "same_file_qualified_name_and_kind",
                &comparison,
                budget,
                limits,
                d,
            )?);
        } else {
            for (index, group) in [old, new].into_iter().enumerate() {
                for symbol in group {
                    let key = (
                        text(symbol.value, "kind")?.into(),
                        text(symbol.value, "fingerprint")?.into(),
                    );
                    let entry = remaining.entry(key).or_default();
                    if index == 0 {
                        entry.0.push(symbol)
                    } else {
                        entry.1.push(symbol)
                    }
                }
            }
        }
    }
    for (_, (old, new)) in remaining {
        rows.push(symbol_row(
            &old,
            &new,
            "captured_declaration_fingerprint",
            &comparison,
            budget,
            limits,
            d,
        )?);
    }
    Ok(rows)
}

#[cfg(test)]
mod output_budget_tests {
    use super::*;
    fn budget(bytes: usize) -> Budget {
        Budget {
            remaining: Options::default(),
            files: 0,
            symbols: 0,
            comparisons: 0,
            proof_remaining: 0,
            row_bytes_remaining: bytes,
        }
    }
    #[test]
    fn repeated_rows_charge_exact_escaped_json_and_refuse_without_a_partial_debit() {
        let row = json!({"text":"\"\\\n".repeat(100)});
        let size = serde_json::to_vec(&row).unwrap().len();
        let mut budget = budget(2 * size - 1);
        let d = Deadline::new(5000);
        budget.admit_row(&row, &d).unwrap();
        assert_eq!(budget.row_bytes_remaining, size - 1);
        assert!(budget.admit_row(&row, &d).is_err());
        assert_eq!(budget.row_bytes_remaining, size - 1);
    }
    #[test]
    fn zero_capacity_and_expired_deadlines_never_admit_a_row() {
        let row = json!(null);
        let mut empty = budget(0);
        assert!(empty.admit_row(&row, &Deadline::new(5000)).is_err());
        assert_eq!(empty.row_bytes_remaining, 0);
        let mut expired = budget(100);
        assert!(expired.admit_row(&row, &Deadline::new(0)).is_err());
        assert_eq!(expired.row_bytes_remaining, 100);
    }
}
