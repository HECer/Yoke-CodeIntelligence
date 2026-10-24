//! Pure historical comparison of two fully admitted immutable selections.
//! Input authority belongs to the sealed comparison_input adapter.
use anyhow::{Result, anyhow, ensure};
use serde::{Serialize, Serializer};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{collections::BTreeSet, io::Write};

use crate::{
    capture::Deadline,
    graph_view::{
        View,
        comparison_input::{self, CheckedComparisonPair, CheckedComparisonSide},
    },
    graph_view_archive::CheckedMeaningArchive,
    model::valid_id,
    store::Store,
};

#[cfg(test)]
#[path = "view_comparison_budget_tests.rs"]
mod budget_tests;
#[path = "view_comparison_meaning.rs"]
mod meaning;
#[path = "view_comparison_source.rs"]
mod source;

const MAX_INPUT_BYTES: usize = 64_000_000;
const DIGEST_METHOD: &str = "sha256_canonical_original_json_v1";

#[derive(Clone, Debug)]
pub struct SelectionPin {
    pub workspace_id: String,
    pub snapshot_id: String,
    pub view_id: String,
    pub view_artifact_id: String,
    pub base_graph_hash: String,
}
impl SelectionPin {
    pub fn from_view(view: &View) -> Self {
        Self {
            workspace_id: view.workspace_id.clone(),
            snapshot_id: view.snapshot_id.clone(),
            view_id: view.id.clone(),
            view_artifact_id: view.artifact_id.clone(),
            base_graph_hash: view.base_graph_hash.clone(),
        }
    }
}
#[derive(Clone, Debug)]
pub struct DeclaredFileContinuity {
    pub before_path: String,
    pub before_hash: String,
    pub after_path: String,
    pub after_hash: String,
}
#[derive(Clone, Debug)]
pub struct ComparisonOptions {
    pub max_result_rows: usize,
    pub max_output_bytes: usize,
    pub max_input_bytes: usize,
    pub max_source_bytes: usize,
    pub declared_file_continuity: Vec<DeclaredFileContinuity>,
}
impl Default for ComparisonOptions {
    fn default() -> Self {
        Self {
            max_result_rows: 10_000,
            max_output_bytes: 4_000_000,
            max_input_bytes: MAX_INPUT_BYTES,
            max_source_bytes: MAX_INPUT_BYTES,
            declared_file_continuity: Vec::new(),
        }
    }
}
#[derive(Debug)]
pub struct Comparison {
    value: Value,
    canonical_bytes: Vec<u8>,
}
impl Comparison {
    pub fn value(&self) -> &Value {
        &self.value
    }
    pub fn canonical_bytes(&self) -> &[u8] {
        &self.canonical_bytes
    }
}
impl Serialize for Comparison {
    fn serialize<S: Serializer>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error> {
        self.value.serialize(serializer)
    }
}

pub fn compare_published(
    store: &Store,
    before: &SelectionPin,
    after: &SelectionPin,
    options: &ComparisonOptions,
    deadline: &Deadline,
) -> Result<Comparison> {
    deadline.check()?;
    validate_options(options)?;
    let pair = comparison_input::prepare_published_pair(store, before, after, options, deadline)?;
    compare_pair(&pair, options, deadline)
}
pub fn compare_meaning_archives(
    before: &CheckedMeaningArchive,
    after: &CheckedMeaningArchive,
    options: &ComparisonOptions,
    deadline: &Deadline,
) -> Result<Comparison> {
    deadline.check()?;
    validate_options(options)?;
    let pair = comparison_input::prepare_archive_pair(before, after, options, deadline)?;
    compare_pair(&pair, options, deadline)
}

fn validate_options(options: &ComparisonOptions) -> Result<()> {
    ensure!(
        options.declared_file_continuity.len() <= 100,
        "comparison_capacity_exceeded"
    );
    let mut before = BTreeSet::new();
    let mut after = BTreeSet::new();
    for item in &options.declared_file_continuity {
        let canonical_before = crate::paths::normalized_syntax(&item.before_path)
            .map_err(|_| anyhow!("comparison_invalid_continuity"))?;
        let canonical_after = crate::paths::normalized_syntax(&item.after_path)
            .map_err(|_| anyhow!("comparison_invalid_continuity"))?;
        ensure!(
            canonical_before == item.before_path
                && canonical_after == item.after_path
                && valid_id(&item.before_hash)
                && valid_id(&item.after_hash)
                && before.insert(item.before_path.as_str())
                && after.insert(item.after_path.as_str()),
            "comparison_invalid_continuity"
        );
    }
    Ok(())
}

fn compare_pair(
    pair: &CheckedComparisonPair<'_>,
    options: &ComparisonOptions,
    deadline: &Deadline,
) -> Result<Comparison> {
    deadline.check()?;
    let before = pair.before();
    let after = pair.after();
    ensure!(
        before.view().workspace_id == after.view().workspace_id,
        "comparison_workspace_mismatch"
    );
    ensure!(
        before.analyzer_epoch() == after.analyzer_epoch(),
        "comparison_analyzer_epoch_mismatch"
    );
    ensure!(
        before.primary_capture_options() == after.primary_capture_options(),
        "comparison_policy_mismatch"
    );
    ensure!(
        pair.accounting().total_proof_bytes() <= options.max_input_bytes
            && pair.accounting().distinct_source_bytes() <= options.max_source_bytes,
        "comparison_capacity_exceeded"
    );
    let header_units = header_row_units(before, after, options, deadline)?;
    ensure!(
        header_units <= options.max_result_rows,
        "comparison_capacity_exceeded"
    );
    let declarations = continuity(before, after, options, deadline)?;
    let left = side_descriptor(before, deadline)?;
    let right = side_descriptor(after, deadline)?;
    let header_remaining = options
        .max_result_rows
        .checked_sub(header_units)
        .ok_or_else(|| anyhow!("comparison_capacity_exceeded"))?;
    let selections = selection_changes(&left, &right, header_remaining, deadline)?;
    let selection_units = array_row_units(&selections, deadline)?;
    let coverage = coverage_changes(
        &left,
        &right,
        before.view().meaning.is_some(),
        after.view().meaning.is_some(),
        header_remaining
            .checked_sub(selection_units)
            .ok_or_else(|| anyhow!("comparison_capacity_exceeded"))?,
        deadline,
    )?;
    let coverage_units = array_row_units(&coverage, deadline)?;
    let reserved = header_units
        .checked_add(selection_units)
        .and_then(|units| units.checked_add(coverage_units))
        .ok_or_else(|| anyhow!("comparison_capacity_exceeded"))?;
    let remaining = options
        .max_result_rows
        .checked_sub(reserved)
        .ok_or_else(|| anyhow!("comparison_capacity_exceeded"))?;
    let source_options = ComparisonOptions {
        max_result_rows: remaining,
        ..options.clone()
    };
    let source = source::compare(before, after, &source_options, deadline)?;
    let remaining = remaining
        .checked_sub(row_units(&source, deadline)?)
        .ok_or_else(|| anyhow!("comparison_capacity_exceeded"))?;
    let meaning = meaning::compare(before.view(), after.view(), remaining, deadline)?;
    let mut limitations = vec![
        "selected_inputs_not_complete_history",
        "structural_candidates_not_verified_identity",
        "source_precision_preserved",
    ];
    if before.view().meaning.is_some() || after.view().meaning.is_some() {
        limitations.push("inferred_meaning_unverified");
    }
    limitations.sort_unstable();
    let mut value = json!({
        "producer":"yoke-view-comparison-1", "options":options_value(options),
        "before":left,"after":right,"selectionChanges":selections,
        "declaredFileContinuity":declarations,"renameProofAvailable":false,
        "source":source,"meaning":meaning,"coverageChanges":coverage,
        "rowBudget":{"used":0,"limit":options.max_result_rows},
        "complete":false,"limitations":limitations,
    });
    let used = row_units(&value, deadline)?;
    ensure!(
        used <= options.max_result_rows,
        "comparison_capacity_exceeded"
    );
    value["rowBudget"]["used"] = json!(used);
    value.sort_all_objects();
    let comparison_id = digest_value(&value, options.max_output_bytes, deadline)?;
    value["comparisonId"] = json!(comparison_id);
    value.sort_all_objects();
    let canonical_bytes = encode_output(&value, options.max_output_bytes, deadline)?;
    deadline.check()?;
    Ok(Comparison {
        value,
        canonical_bytes,
    })
}

fn header_row_units(
    before: &CheckedComparisonSide<'_>,
    after: &CheckedComparisonSide<'_>,
    options: &ComparisonOptions,
    deadline: &Deadline,
) -> Result<usize> {
    let mut units = if before.view().meaning.is_some() || after.view().meaning.is_some() {
        4usize
    } else {
        3usize
    };
    units = units
        .checked_add(
            options
                .declared_file_continuity
                .len()
                .checked_mul(2)
                .ok_or_else(|| anyhow!("comparison_capacity_exceeded"))?,
        )
        .ok_or_else(|| anyhow!("comparison_capacity_exceeded"))?;
    for side in [before, after] {
        deadline.check()?;
        let view = side.view();
        for count in [
            view.overlays.len(),
            view.meaning
                .as_ref()
                .map_or(0, |layer| layer.selections.len()),
            side.canonical_graph().coverage.languages.len(),
            side.primary_snapshot().omissions.len(),
        ] {
            units = units
                .checked_add(count)
                .ok_or_else(|| anyhow!("comparison_capacity_exceeded"))?;
        }
    }
    Ok(units)
}

fn array_row_units(values: &[Value], deadline: &Deadline) -> Result<usize> {
    let mut units = values.len();
    for value in values {
        deadline.check()?;
        units = units
            .checked_add(row_units(value, deadline)?)
            .ok_or_else(|| anyhow!("comparison_capacity_exceeded"))?;
    }
    Ok(units)
}

fn options_value(options: &ComparisonOptions) -> Value {
    json!({"maxResultRows":options.max_result_rows,"maxOutputBytes":options.max_output_bytes,
        "maxInputBytes":options.max_input_bytes,"maxSourceBytes":options.max_source_bytes,
        "declaredFileContinuity":options.declared_file_continuity.iter().map(|d| json!({
            "beforePath":d.before_path,"beforeHash":d.before_hash,"afterPath":d.after_path,"afterHash":d.after_hash,
        })).collect::<Vec<_>>()})
}

fn continuity(
    before: &CheckedComparisonSide<'_>,
    after: &CheckedComparisonSide<'_>,
    options: &ComparisonOptions,
    deadline: &Deadline,
) -> Result<Vec<Value>> {
    let mut result = Vec::new();
    let left_files = before
        .primary_snapshot()
        .files
        .iter()
        .map(|file| (file.path.as_str(), file))
        .collect::<std::collections::BTreeMap<_, _>>();
    let right_files = after
        .primary_snapshot()
        .files
        .iter()
        .map(|file| (file.path.as_str(), file))
        .collect::<std::collections::BTreeMap<_, _>>();
    for item in &options.declared_file_continuity {
        deadline.check()?;
        ensure!(
            left_files
                .get(item.before_path.as_str())
                .is_some_and(|file| file.hash == item.before_hash)
                && right_files
                    .get(item.after_path.as_str())
                    .is_some_and(|file| file.hash == item.after_hash),
            "comparison_primary_file_binding_mismatch"
        );
        result.push(json!({"beforePath":item.before_path,"beforeHash":item.before_hash,
            "afterPath":item.after_path,"afterHash":item.after_hash,
            "workspaceId":before.view().workspace_id,"beforeSnapshotId":before.primary_snapshot().id,
            "afterSnapshotId":after.primary_snapshot().id,"beforeGraphViewId":before.view().id,
            "afterGraphViewId":after.view().id,"provenance":"host_declared_file_continuity_not_proven_rename",
            "verifiedIdentity":false}));
    }
    Ok(result)
}

fn side_descriptor(side: &CheckedComparisonSide<'_>, deadline: &Deadline) -> Result<Value> {
    let view = side.view();
    let snapshot = side.primary_snapshot();
    let capture = side.primary_capture_options();
    let policy = json!({"profile":"yoke_view_comparison_policy_1","analyzerEpoch":side.analyzer_epoch().producer(),
        "captureOptions":{"onlyDirs":capture.only_dirs,"includeDirs":capture.include_dirs,
            "languageOverrides":capture.language_overrides,"readOnlyDependencyDirs":capture.read_only_dependency_dirs,
            "includeIgnoredDirs":capture.include_ignored_dirs}});
    let mut overlays = Vec::new();
    for original in &view.overlays {
        deadline.check()?;
        let mut overlay = pick(
            original,
            &[
                "artifactId",
                "overlayId",
                "producer",
                "origin",
                "providerAttestation",
                "serverProfileHash",
            ],
        )?;
        overlay["originalRowDigest"] = json!(canonical_digest(original, deadline)?);
        overlay["digestMethod"] = json!(DIGEST_METHOD);
        if let Some(version) = original.get("schemaVersion") {
            overlay["schemaVersion"] = version.clone();
        }
        overlays.push(overlay);
    }
    let mut selections = Vec::new();
    if let Some(meaning) = &view.meaning {
        for selected in &meaning.selections {
            deadline.check()?;
            selections.push(pick(
                &serde_json::to_value(selected).map_err(|_| invariant())?,
                &["graphId", "jobId", "artifactId"],
            )?);
        }
    }
    let coverage = &side.canonical_graph().coverage;
    let compact_coverage = json!({"originalRowDigest":canonical_digest(coverage, deadline)?,"digestMethod":DIGEST_METHOD,
        "indexedFiles":coverage.indexed_files,"sourceFiles":coverage.source_files,"unresolved":coverage.unresolved,
        "omissions":coverage.omissions,"exhaustive":coverage.exhaustive,"semantic":coverage.semantic,
        "languages":coverage.languages.iter().map(|(name,count)| json!({"name":name,"count":count})).collect::<Vec<_>>()});
    deadline.check()?;
    // This is the existing typed graph identity as a descriptor, never a source grant.
    let projection_hash = serialized_digest(side.source_projection(), MAX_INPUT_BYTES, deadline)?;
    deadline.check()?;
    let mut result = json!({"workspaceId":view.workspace_id,"snapshotId":snapshot.id,"graphViewId":view.id,
        "viewArtifactId":view.artifact_id,"baseGraphHash":view.base_graph_hash,"sourceProjectionHash":projection_hash,
        "comparisonPolicyHash":canonical_digest(&policy, deadline)?,"analyzerEpoch":side.analyzer_epoch().producer(),
        "snapshotKind":snapshot.kind,"captureConsistency":snapshot.capture_consistency,
        "overlays":overlays,"meaningSelections":selections,"coverage":compact_coverage,"omissions":snapshot.omissions});
    if let Some(meaning) = &view.meaning {
        result["meaningCapturePolicyHash"] = json!(meaning.scope.capture_policy_hash);
    }
    Ok(result)
}

fn charge_rows(used: &mut usize, amount: usize, limit: usize) -> Result<()> {
    let next = used
        .checked_add(amount)
        .ok_or_else(|| anyhow!("comparison_capacity_exceeded"))?;
    ensure!(next <= limit, "comparison_capacity_exceeded");
    *used = next;
    Ok(())
}

fn selection_changes(
    before: &Value,
    after: &Value,
    limit: usize,
    deadline: &Deadline,
) -> Result<Vec<Value>> {
    let mut result = Vec::new();
    let mut used = 0usize;
    for (field, stratum) in [
        ("overlays", "source_overlay"),
        ("meaningSelections", "meaning_selection"),
    ] {
        let left = values(&before[field])?;
        let right = values(&after[field])?;
        let left_keys = left
            .iter()
            .map(|item| canonical_digest(item, deadline))
            .collect::<Result<BTreeSet<_>>>()?;
        let right_keys = right
            .iter()
            .map(|item| canonical_digest(item, deadline))
            .collect::<Result<BTreeSet<_>>>()?;
        for (old, new, kind) in [(left, &right_keys, "removed"), (right, &left_keys, "added")] {
            for item in old {
                deadline.check()?;
                if !new.contains(&canonical_digest(item, deadline)?) {
                    charge_rows(
                        &mut used,
                        row_units(item, deadline)?
                            .checked_add(1)
                            .ok_or_else(|| anyhow!("comparison_capacity_exceeded"))?,
                        limit,
                    )?;
                    result.push(json!({"changeKind":kind,"stratum":stratum,
                        "before":if kind == "removed" { item.clone() } else { Value::Null },
                        "after":if kind == "added" { item.clone() } else { Value::Null }}));
                }
            }
        }
    }
    ensure!(
        array_row_units(&result, deadline)? == used,
        "comparison_input_invariant"
    );
    Ok(result)
}

fn coverage_changes(
    before: &Value,
    after: &Value,
    before_meaning: bool,
    after_meaning: bool,
    limit: usize,
    deadline: &Deadline,
) -> Result<Vec<Value>> {
    let mut result = Vec::new();
    let mut used = 0usize;
    if before["coverage"] != after["coverage"] {
        charge_rows(
            &mut used,
            row_units(&before["coverage"], deadline)?
                .checked_add(row_units(&after["coverage"], deadline)?)
                .and_then(|units| units.checked_add(1))
                .ok_or_else(|| anyhow!("comparison_capacity_exceeded"))?,
            limit,
        )?;
        result.push(json!({"changeKind":"coverage","beforeCoverage":before["coverage"],"afterCoverage":after["coverage"]}));
    }
    if before["omissions"] != after["omissions"] {
        charge_rows(
            &mut used,
            row_units(&before["omissions"], deadline)?
                .checked_add(row_units(&after["omissions"], deadline)?)
                .and_then(|units| units.checked_add(1))
                .ok_or_else(|| anyhow!("comparison_capacity_exceeded"))?,
            limit,
        )?;
        result.push(json!({"changeKind":"omissions","beforeOmissions":before["omissions"],"afterOmissions":after["omissions"]}));
    }
    if before_meaning != after_meaning {
        deadline.check()?;
        charge_rows(&mut used, 1, limit)?;
        result.push(json!({"changeKind":"meaning_presence","beforePresent":before_meaning,"afterPresent":after_meaning}));
    }
    ensure!(
        array_row_units(&result, deadline)? == used,
        "comparison_input_invariant"
    );
    Ok(result)
}

fn invariant() -> anyhow::Error {
    anyhow!("comparison_input_invariant")
}
fn text<'a>(value: &'a Value, field: &str) -> Result<&'a str> {
    value
        .get(field)
        .and_then(Value::as_str)
        .ok_or_else(invariant)
}
fn values(value: &Value) -> Result<&[Value]> {
    value.as_array().map(Vec::as_slice).ok_or_else(invariant)
}
fn unsigned(value: &Value, field: &str) -> Result<usize> {
    value
        .get(field)
        .and_then(Value::as_u64)
        .and_then(|n| usize::try_from(n).ok())
        .ok_or_else(invariant)
}
fn pick(value: &Value, fields: &[&str]) -> Result<Value> {
    let mut result = serde_json::Map::new();
    for field in fields {
        result.insert(
            (*field).to_owned(),
            value.get(*field).ok_or_else(invariant)?.clone(),
        );
    }
    Ok(Value::Object(result))
}

pub(super) fn row_units(value: &Value, deadline: &Deadline) -> Result<usize> {
    fn count(value: &Value, deadline: &Deadline, depth: usize) -> Result<usize> {
        deadline.check()?;
        ensure!(depth <= 128, "comparison_capacity_exceeded");
        let mut total = 0usize;
        match value {
            Value::Array(values) => {
                total = values.len();
                for child in values {
                    total = total
                        .checked_add(count(child, deadline, depth + 1)?)
                        .ok_or_else(|| anyhow!("comparison_capacity_exceeded"))?;
                    #[cfg(test)]
                    budget_tests::row_counter_progress();
                }
            }
            Value::Object(values) => {
                for child in values.values() {
                    total = total
                        .checked_add(count(child, deadline, depth + 1)?)
                        .ok_or_else(|| anyhow!("comparison_capacity_exceeded"))?;
                    #[cfg(test)]
                    budget_tests::row_counter_progress();
                }
            }
            _ => {}
        }
        Ok(total)
    }
    let total = count(value, deadline, 0)?;
    deadline.check()?;
    Ok(total)
}
pub(super) fn check_component_rows(value: &Value, limit: usize, deadline: &Deadline) -> Result<()> {
    ensure!(
        row_units(value, deadline)? <= limit,
        "comparison_capacity_exceeded"
    );
    Ok(())
}

struct ByteSink<'a> {
    deadline: &'a Deadline,
    maximum: usize,
    count: usize,
    bytes: Option<Vec<u8>>,
    hash: Option<Sha256>,
    failure: Option<anyhow::Error>,
}
impl Write for ByteSink<'_> {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        let operation = (|| -> Result<()> {
            self.deadline.check()?;
            let next = self
                .count
                .checked_add(bytes.len())
                .ok_or_else(|| anyhow!("comparison_capacity_exceeded"))?;
            ensure!(next <= self.maximum, "comparison_capacity_exceeded");
            for chunk in bytes.chunks(16 * 1024) {
                self.deadline.check()?;
                if let Some(hash) = &mut self.hash {
                    hash.update(chunk);
                }
                if let Some(buffer) = &mut self.bytes {
                    buffer.extend_from_slice(chunk);
                }
            }
            self.count = next;
            Ok(())
        })();
        if let Err(error) = operation {
            self.failure = Some(error);
            return Err(std::io::Error::other("comparison_bounded_writer"));
        }
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
fn serialize_into<T: Serialize + ?Sized>(value: &T, sink: &mut ByteSink<'_>) -> Result<()> {
    if serde_json::to_writer(&mut *sink, value).is_err() {
        return Err(sink.failure.take().unwrap_or_else(invariant));
    }
    sink.deadline.check()?;
    Ok(())
}
fn encode_output(value: &Value, maximum: usize, deadline: &Deadline) -> Result<Vec<u8>> {
    encode_bounded(value, maximum, deadline)
}
fn encode_bounded<T: Serialize + ?Sized>(
    value: &T,
    maximum: usize,
    deadline: &Deadline,
) -> Result<Vec<u8>> {
    let mut counter = ByteSink {
        deadline,
        maximum,
        count: 0,
        bytes: None,
        hash: None,
        failure: None,
    };
    serialize_into(value, &mut counter)?;
    let mut writer = ByteSink {
        deadline,
        maximum,
        count: 0,
        bytes: Some(Vec::with_capacity(counter.count)),
        hash: None,
        failure: None,
    };
    serialize_into(value, &mut writer)?;
    ensure!(writer.count == counter.count, "comparison_input_invariant");
    writer.bytes.ok_or_else(invariant)
}
pub(super) fn canonical_digest<T: Serialize + ?Sized>(
    original: &T,
    deadline: &Deadline,
) -> Result<String> {
    canonical_digest_limited(original, MAX_INPUT_BYTES, deadline)
}
fn canonical_digest_limited<T: Serialize + ?Sized>(
    original: &T,
    maximum: usize,
    deadline: &Deadline,
) -> Result<String> {
    deadline.check()?;
    // Admit the full serialized original before constructing an owned Value.
    let mut preflight = ByteSink {
        deadline,
        maximum,
        count: 0,
        bytes: None,
        hash: None,
        failure: None,
    };
    serialize_into(original, &mut preflight)?;
    let mut value = serde_json::to_value(original).map_err(|_| invariant())?;
    value.sort_all_objects();
    deadline.check()?;
    digest_value(&value, maximum, deadline)
}
fn digest_value(value: &Value, maximum: usize, deadline: &Deadline) -> Result<String> {
    serialized_digest(value, maximum, deadline)
}
fn serialized_digest<T: Serialize + ?Sized>(
    value: &T,
    maximum: usize,
    deadline: &Deadline,
) -> Result<String> {
    let mut sink = ByteSink {
        deadline,
        maximum,
        count: 0,
        bytes: None,
        hash: Some(Sha256::new()),
        failure: None,
    };
    serialize_into(value, &mut sink)?;
    Ok(hex::encode(sink.hash.ok_or_else(invariant)?.finalize()))
}
fn bytes_digest(bytes: &[u8], deadline: &Deadline) -> Result<String> {
    let mut digest = Sha256::new();
    for chunk in bytes.chunks(16 * 1024) {
        deadline.check()?;
        digest.update(chunk);
    }
    deadline.check()?;
    Ok(hex::encode(digest.finalize()))
}
