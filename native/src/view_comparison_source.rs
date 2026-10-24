//! Pure structural correspondences and directed evidence-qualified multiset deltas.
use super::{
    ComparisonOptions, DIGEST_METHOD, bytes_digest, canonical_digest, invariant, pick, row_units,
    text, unsigned, values,
};
use crate::{
    capture::Deadline,
    graph_view::{
        View,
        comparison_input::{BoundComparisonScope, CheckedComparisonSide},
    },
    model::SourceFile,
};
use anyhow::{Result, anyhow, ensure};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

struct Scope<'a> {
    bound: BoundComparisonScope<'a>,
    files: BTreeMap<&'a str, &'a SourceFile>,
}
struct Texts<'a> {
    scopes: BTreeMap<&'a str, Scope<'a>>,
    ranges: BTreeMap<(String, String, usize, usize), String>,
}
impl<'a> Texts<'a> {
    fn new(side: &'a CheckedComparisonSide<'_>, deadline: &Deadline) -> Result<Self> {
        let mut scopes = BTreeMap::new();
        for snapshot in side.source_scopes() {
            deadline.check()?;
            let bound = side.bind_scope(snapshot, deadline)?;
            let mut files = BTreeMap::new();
            for file in &snapshot.files {
                deadline.check()?;
                ensure!(
                    files.insert(file.path.as_str(), file).is_none(),
                    "comparison_input_invariant"
                );
            }
            ensure!(
                scopes
                    .insert(snapshot.id.as_str(), Scope { bound, files })
                    .is_none(),
                "comparison_input_invariant"
            );
        }
        Ok(Self {
            scopes,
            ranges: BTreeMap::new(),
        })
    }
    fn range(
        &mut self,
        snapshot_id: &str,
        path: &str,
        start: usize,
        end: usize,
        deadline: &Deadline,
    ) -> Result<String> {
        deadline.check()?;
        let key = (snapshot_id.to_owned(), path.to_owned(), start, end);
        if let Some(hash) = self.ranges.get(&key) {
            return Ok(hash.clone());
        }
        let scope = self.scopes.get(snapshot_id).ok_or_else(invariant)?;
        let file = scope.files.get(path).ok_or_else(invariant)?;
        let admitted_snapshot = scope.bound.snapshot();
        let source = scope.bound.source(file, deadline)?;
        ensure!(
            source.member().file() == *file
                && source.member().snapshot().id == admitted_snapshot.id
                && admitted_snapshot.id == snapshot_id,
            "comparison_input_invariant"
        );
        let snippet = source.text().get(start..end).ok_or_else(invariant)?;
        let hash = bytes_digest(snippet.as_bytes(), deadline)?;
        self.ranges.insert(key, hash.clone());
        Ok(hash)
    }
    fn anchor(&mut self, value: &Value, deadline: &Deadline) -> Result<String> {
        self.range(
            text(value, "snapshotId")?,
            text(value, "path")?,
            unsigned(value, "startByte")?,
            unsigned(value, "endByte")?,
            deadline,
        )
    }
}

struct Rows {
    used: usize,
    limit: usize,
}
impl Rows {
    fn ensure_additional(&self, amount: usize) -> Result<()> {
        let next = self
            .used
            .checked_add(amount)
            .ok_or_else(|| anyhow!("comparison_capacity_exceeded"))?;
        ensure!(next <= self.limit, "comparison_capacity_exceeded");
        Ok(())
    }
    fn charge(&mut self, amount: usize) -> Result<()> {
        let next = self
            .used
            .checked_add(amount)
            .ok_or_else(|| anyhow!("comparison_capacity_exceeded"))?;
        ensure!(next <= self.limit, "comparison_capacity_exceeded");
        self.used = next;
        Ok(())
    }
    fn push(&mut self, output: &mut Vec<Value>, value: Value, deadline: &Deadline) -> Result<()> {
        self.charge(
            row_units(&value, deadline)?
                .checked_add(1)
                .ok_or_else(|| anyhow!("comparison_capacity_exceeded"))?,
        )?;
        output.push(value);
        Ok(())
    }
}

#[derive(Clone)]
struct NodeState {
    semantic_key: String,
    correspondence: Option<String>,
    ambiguous: bool,
}
struct NodeResult {
    changes: Vec<Value>,
    ambiguous: Vec<Value>,
    before: BTreeMap<String, NodeState>,
    after: BTreeMap<String, NodeState>,
}

pub(super) fn compare(
    before: &CheckedComparisonSide<'_>,
    after: &CheckedComparisonSide<'_>,
    options: &ComparisonOptions,
    deadline: &Deadline,
) -> Result<Value> {
    let mut left_texts = Texts::new(before, deadline)?;
    let mut right_texts = Texts::new(after, deadline)?;
    let mut rows = Rows {
        used: 0,
        limit: options.max_result_rows,
    };
    let nodes = compare_nodes(
        (before, after),
        (&mut left_texts, &mut right_texts),
        options,
        &mut rows,
        deadline,
    )?;
    let (observations, ambiguous) = compare_observations(
        before.view(),
        after.view(),
        &nodes,
        &mut left_texts,
        &mut right_texts,
        &mut rows,
        deadline,
    )?;
    let (before_calls, before_self) = totals(before.view(), deadline)?;
    let (after_calls, after_self) = totals(after.view(), deadline)?;
    let result = json!({"nodeChanges":nodes.changes,"ambiguousNodes":nodes.ambiguous,
        "observationChanges":observations,"ambiguousObservations":ambiguous,
        "totals":{"beforeCalls":before_calls,"afterCalls":after_calls,
            "beforeSelfLoops":before_self,"afterSelfLoops":after_self}});
    ensure!(
        row_units(&result, deadline)? == rows.used,
        "comparison_input_invariant"
    );
    Ok(result)
}

fn totals(view: &View, deadline: &Deadline) -> Result<(usize, usize)> {
    let mut calls = 0usize;
    let mut self_loops = 0usize;
    for row in &view.observations {
        deadline.check()?;
        if row["relation"] == "calls" {
            calls = calls
                .checked_add(1)
                .ok_or_else(|| anyhow!("comparison_capacity_exceeded"))?;
            if !row["target"].is_null() && row["source"] == row["target"] {
                self_loops = self_loops
                    .checked_add(1)
                    .ok_or_else(|| anyhow!("comparison_capacity_exceeded"))?;
            }
        }
    }
    Ok((calls, self_loops))
}

fn node_units(original: &Value) -> Result<usize> {
    Ok(values(&original["sourceBindings"])?.len())
}

fn node_pair_units(left: &Value, right: &Value) -> Result<usize> {
    node_units(left)?
        .checked_add(node_units(right)?)
        .and_then(|units| units.checked_add(1))
        .ok_or_else(|| anyhow!("comparison_capacity_exceeded"))
}

fn observation_units(original: &Value) -> Result<usize> {
    let bindings = original
        .get("sourceBindings")
        .map(values)
        .transpose()?
        .map_or(0, <[Value]>::len);
    values(&original["evidenceIds"])?
        .len()
        .checked_mul(2)
        .and_then(|units| units.checked_add(bindings))
        .and_then(|units| units.checked_add(1))
        .ok_or_else(|| anyhow!("comparison_capacity_exceeded"))
}

fn groups<'a>(view: &'a View, deadline: &Deadline) -> Result<BTreeMap<String, Vec<&'a Value>>> {
    let mut parent_keys = BTreeMap::new();
    for node in &view.nodes {
        deadline.check()?;
        if let Some(key) = node.get("key").and_then(Value::as_str) {
            ensure!(
                parent_keys
                    .insert((text(node, "origin")?, text(node, "producer")?, key), node)
                    .is_none(),
                "comparison_input_invariant"
            );
        }
    }
    let mut groups: BTreeMap<String, Vec<&Value>> = BTreeMap::new();
    for node in &view.nodes {
        deadline.check()?;
        let mut parents = Vec::new();
        let mut current = node;
        let mut visited = BTreeSet::new();
        while let Some(key) = current.get("parentKey").and_then(Value::as_str) {
            deadline.check()?;
            ensure!(
                visited.len() < 128 && visited.insert(key),
                "comparison_input_invariant"
            );
            let Some(parent) =
                parent_keys.get(&(text(current, "origin")?, text(current, "producer")?, key))
            else {
                parents.push(json!({"unresolvedParentLabelContext":true}));
                break;
            };
            parents.push(json!({"kind":parent["kind"],"label":node_label(parent)?}));
            current = parent;
        }
        parents.reverse();
        if parents.is_empty()
            && let Some(container) = node.get("containerName")
        {
            parents.push(json!({"containerLabel":container}));
        }
        let key = canonical_digest(
            &json!({"profile":"yoke_view_comparison_structural_key_1",
            "origin":node["origin"],"producer":node["producer"],"path":node["path"],"kind":node["kind"],
            "label":node_label(node)?,"parentLabelContext":parents}),
            deadline,
        )?;
        groups.entry(key).or_default().push(node);
    }
    Ok(groups)
}

fn node_label(node: &Value) -> Result<&str> {
    node.get("qualifiedName")
        .and_then(Value::as_str)
        .or_else(|| node.get("name").and_then(Value::as_str))
        .ok_or_else(invariant)
}

fn primary_files<'a>(view: &'a View, deadline: &Deadline) -> Result<BTreeMap<&'a str, &'a Value>> {
    let mut files = BTreeMap::new();
    for node in &view.nodes {
        deadline.check()?;
        if node["origin"] == "canonical" && node["kind"] == "file" {
            ensure!(
                files.insert(text(node, "path")?, node).is_none(),
                "comparison_input_invariant"
            );
        }
    }
    Ok(files)
}

fn compare_nodes(
    sides: (&CheckedComparisonSide<'_>, &CheckedComparisonSide<'_>),
    texts: (&mut Texts<'_>, &mut Texts<'_>),
    options: &ComparisonOptions,
    rows: &mut Rows,
    deadline: &Deadline,
) -> Result<NodeResult> {
    let (before, after) = sides;
    let (left_texts, right_texts) = texts;
    let left_groups = groups(before.view(), deadline)?;
    let right_groups = groups(after.view(), deadline)?;
    let mut result = NodeResult {
        changes: Vec::new(),
        ambiguous: Vec::new(),
        before: BTreeMap::new(),
        after: BTreeMap::new(),
    };
    let mut used_left = BTreeSet::new();
    let mut used_right = BTreeSet::new();
    let left_files = primary_files(before.view(), deadline)?;
    let right_files = primary_files(after.view(), deadline)?;
    for declaration in &options.declared_file_continuity {
        deadline.check()?;
        let left = *left_files
            .get(declaration.before_path.as_str())
            .ok_or_else(invariant)?;
        let right = *right_files
            .get(declaration.after_path.as_str())
            .ok_or_else(invariant)?;
        let key = canonical_digest(
            &json!({"profile":"yoke_view_comparison_declared_file_1",
            "beforeViewId":before.view().id,"afterViewId":after.view().id,
            "beforeSnapshotId":before.primary_snapshot().id,"afterSnapshotId":after.primary_snapshot().id,
            "beforePath":declaration.before_path,"beforeHash":declaration.before_hash,
            "afterPath":declaration.after_path,"afterHash":declaration.after_hash}),
            deadline,
        )?;
        used_left.insert(text(left, "id")?.to_owned());
        used_right.insert(text(right, "id")?.to_owned());
        let state = NodeState {
            semantic_key: key.clone(),
            correspondence: Some(key.clone()),
            ambiguous: false,
        };
        result
            .before
            .insert(text(left, "id")?.to_owned(), state.clone());
        result.after.insert(text(right, "id")?.to_owned(), state);
        rows.ensure_additional(node_pair_units(left, right)?)?;
        rows.push(
            &mut result.changes,
            paired_node(
                (before.view(), after.view()),
                (left, right),
                &key,
                "host_declared_file_continuity_not_proven_rename",
                (&mut *left_texts, &mut *right_texts),
                deadline,
            )?,
            deadline,
        )?;
    }
    let keys = left_groups
        .keys()
        .chain(right_groups.keys())
        .cloned()
        .collect::<BTreeSet<_>>();
    for key in keys {
        deadline.check()?;
        let left = left_groups
            .get(&key)
            .into_iter()
            .flatten()
            .copied()
            .filter(|n| !used_left.contains(n["id"].as_str().unwrap_or("")))
            .collect::<Vec<_>>();
        let right = right_groups
            .get(&key)
            .into_iter()
            .flatten()
            .copied()
            .filter(|n| !used_right.contains(n["id"].as_str().unwrap_or("")))
            .collect::<Vec<_>>();
        if left.is_empty() && right.is_empty() {
            continue;
        }
        let ambiguous = left.len() > 1 || right.len() > 1;
        let paired = !ambiguous && left.len() == 1 && right.len() == 1;
        let state = NodeState {
            semantic_key: key.clone(),
            correspondence: paired.then(|| key.clone()),
            ambiguous,
        };
        for node in &left {
            result
                .before
                .insert(text(node, "id")?.to_owned(), state.clone());
        }
        for node in &right {
            result
                .after
                .insert(text(node, "id")?.to_owned(), state.clone());
        }
        if ambiguous {
            let mut left_rows = Vec::new();
            let mut right_rows = Vec::new();
            let mut temporary_units = 1usize;
            for (view, side, input, output) in [
                (before.view(), "before", &left, &mut left_rows),
                (after.view(), "after", &right, &mut right_rows),
            ] {
                for node in input {
                    deadline.check()?;
                    let prospective = temporary_units
                        .checked_add(node_units(node)?)
                        .and_then(|units| units.checked_add(1))
                        .ok_or_else(|| anyhow!("comparison_capacity_exceeded"))?;
                    rows.ensure_additional(prospective)?;
                    let row = redact_node(view, node, side, deadline)?;
                    let units = row_units(&row, deadline)?
                        .checked_add(1)
                        .ok_or_else(|| anyhow!("comparison_capacity_exceeded"))?;
                    temporary_units = temporary_units
                        .checked_add(units)
                        .ok_or_else(|| anyhow!("comparison_capacity_exceeded"))?;
                    ensure!(
                        temporary_units <= rows.limit.saturating_sub(rows.used),
                        "comparison_capacity_exceeded"
                    );
                    output.push(row);
                }
            }
            rows.push(&mut result.ambiguous, json!({"semanticKey":key,"beforeCount":left.len(),"afterCount":right.len(),
                "beforeRows":left_rows,"afterRows":right_rows,"identityBasis":"ambiguous_structural_label_candidates","verifiedIdentity":false}), deadline)?;
        } else if paired {
            rows.ensure_additional(node_pair_units(left[0], right[0])?)?;
            rows.push(
                &mut result.changes,
                paired_node(
                    (before.view(), after.view()),
                    (left[0], right[0]),
                    &key,
                    "unique_structural_label_candidate",
                    (&mut *left_texts, &mut *right_texts),
                    deadline,
                )?,
                deadline,
            )?;
        } else {
            let (view, node, side, kind) = if let Some(node) = left.first() {
                (before.view(), *node, "before", "removed")
            } else {
                (after.view(), right[0], "after", "added")
            };
            rows.ensure_additional(
                node_units(node)?
                    .checked_add(1)
                    .ok_or_else(|| anyhow!("comparison_capacity_exceeded"))?,
            )?;
            let row = redact_node(view, node, side, deadline)?;
            rows.push(&mut result.changes, json!({"changeKind":kind,"semanticKey":key,"correspondenceId":null,
                "before":if side == "before" {row.clone()} else {Value::Null},
                "after":if side == "after" {row} else {Value::Null},
                "identityBasis":"selected_candidate_only_not_complete_deletion","verifiedIdentity":false}), deadline)?;
        }
    }
    Ok(result)
}

fn paired_node(
    views: (&View, &View),
    nodes: (&Value, &Value),
    key: &str,
    basis: &str,
    texts: (&mut Texts<'_>, &mut Texts<'_>),
    deadline: &Deadline,
) -> Result<Value> {
    let (before, after) = views;
    let (left, right) = nodes;
    let (left_texts, right_texts) = texts;
    let body = |view: &View, node: &Value, texts: &mut Texts<'_>| -> Result<Option<String>> {
        if node.get("startByte").is_none() || node.get("endByte").is_none() {
            return Ok(None);
        }
        Ok(Some(texts.range(
            &view.snapshot_id,
            text(node, "path")?,
            unsigned(node, "startByte")?,
            unsigned(node, "endByte")?,
            deadline,
        )?))
    };
    let left_body = body(before, left, left_texts)?;
    let right_body = body(after, right, right_texts)?;
    let content_changed = left_body.is_some() && right_body.is_some() && left_body != right_body;
    let range_fields = [
        "startByte",
        "endByte",
        "startLine",
        "endLine",
        "nameStartByte",
        "nameEndByte",
    ];
    let range_changed = range_fields
        .iter()
        .any(|field| left.get(*field) != right.get(*field));
    let kind = if content_changed && range_changed {
        "content_and_range_changed"
    } else if content_changed {
        "content_changed"
    } else if range_changed {
        "range_relocated"
    } else if metadata(left) != metadata(right) {
        "metadata_changed"
    } else {
        "unchanged"
    };
    Ok(
        json!({"changeKind":kind,"semanticKey":key,"correspondenceId":key,
        "before":redact_node(before,left,"before",deadline)?,"after":redact_node(after,right,"after",deadline)?,
        "identityBasis":basis,"verifiedIdentity":false}),
    )
}
fn metadata(row: &Value) -> Value {
    let mut value = row.clone();
    if let Some(fields) = value.as_object_mut() {
        for field in [
            "id",
            "key",
            "parentKey",
            "baseNodeId",
            "overlayArtifactId",
            "snapshotId",
            "evidenceIds",
            "startByte",
            "endByte",
            "startLine",
            "endLine",
            "nameStartByte",
            "nameEndByte",
        ] {
            fields.remove(field);
        }
        if let Some(bindings) = fields
            .get_mut("sourceBindings")
            .and_then(Value::as_array_mut)
        {
            for binding in bindings {
                if let Some(anchor) = binding.as_object_mut() {
                    anchor.remove("snapshotId");
                    anchor.remove("contentHash");
                }
            }
        }
    }
    value
}
fn redact_anchor(original: &Value) -> Result<Value> {
    let mut row = pick(
        original,
        &[
            "snapshotId",
            "path",
            "contentHash",
            "startByte",
            "endByte",
            "startLine",
            "endLine",
            "anchor",
        ],
    )?;
    row["textOmitted"] = json!(true);
    for field in ["trust", "producer"] {
        if let Some(value) = original.get(field) {
            row[field] = value.clone();
        }
    }
    Ok(row)
}
fn redact_node(view: &View, original: &Value, side: &str, deadline: &Deadline) -> Result<Value> {
    let digest = canonical_digest(original, deadline)?;
    let mut row = pick(original, &["path", "name", "kind", "origin", "producer"])?;
    let bindings = values(&original["sourceBindings"])?
        .iter()
        .map(redact_anchor)
        .collect::<Result<Vec<_>>>()?;
    row["side"] = json!(side);
    row["originalNodeId"] = original["id"].clone();
    row["originalRowDigest"] = json!(digest);
    row["digestMethod"] = json!(DIGEST_METHOD);
    row["workspaceId"] = json!(view.workspace_id);
    row["snapshotId"] = json!(view.snapshot_id);
    row["graphViewId"] = json!(view.id);
    row["representativeAnchor"] = if bindings.len() == 1 {
        bindings[0].clone()
    } else {
        Value::Null
    };
    row["sourceBindings"] = json!(bindings);
    row["textOmitted"] = json!(true);
    for field in ["qualifiedName", "trust", "providerAttestation"] {
        if let Some(value) = original.get(field) {
            row[field] = value.clone();
        }
    }
    if let Some(signature) = original.get("signature") {
        row["signatureDigest"] = json!(canonical_digest(signature, deadline)?);
    }
    Ok(row)
}
fn redact_evidence(original: &Value, deadline: &Deadline) -> Result<Value> {
    let mut row = redact_anchor(original)?;
    row["originalEvidenceId"] = original["id"].clone();
    row["originalRowDigest"] = json!(canonical_digest(original, deadline)?);
    row["digestMethod"] = json!(DIGEST_METHOD);
    Ok(row)
}
fn redact_observation(
    view: &View,
    original: &Value,
    side: &str,
    evidence: &BTreeMap<&str, &Value>,
    deadline: &Deadline,
) -> Result<Value> {
    let mut row = pick(
        original,
        &[
            "snapshotId",
            "targetLabel",
            "relation",
            "resolution",
            "origin",
            "producer",
            "evidenceIds",
        ],
    )?;
    row["side"] = json!(side);
    row["originalObservationId"] = original["id"].clone();
    row["originalRowDigest"] = json!(canonical_digest(original, deadline)?);
    row["digestMethod"] = json!(DIGEST_METHOD);
    row["sourceId"] = original["source"].clone();
    row["targetId"] = original["target"].clone();
    row["textOmitted"] = json!(true);
    let mut anchors = Vec::new();
    for id in values(&original["evidenceIds"])? {
        deadline.check()?;
        let source = evidence
            .get(id.as_str().ok_or_else(invariant)?)
            .ok_or_else(invariant)?;
        anchors.push(redact_evidence(source, deadline)?);
    }
    row["evidence"] = json!(anchors);
    if let Some(bindings) = original.get("sourceBindings") {
        row["sourceBindings"] = json!(
            values(bindings)?
                .iter()
                .map(redact_anchor)
                .collect::<Result<Vec<_>>>()?
        );
    }
    for field in ["trust", "providerAttestation"] {
        if let Some(value) = original.get(field) {
            row[field] = value.clone();
        }
    }
    ensure!(
        view.snapshot_id == text(&row, "snapshotId")?,
        "comparison_input_invariant"
    );
    Ok(row)
}

struct ObservationGroup<'a> {
    descriptor: Value,
    before: Vec<&'a Value>,
    after: Vec<&'a Value>,
    ambiguous: bool,
}
fn endpoint(
    states: &BTreeMap<String, NodeState>,
    id: &str,
    side: &str,
) -> Result<(Value, Option<String>, bool)> {
    let state = states.get(id).ok_or_else(invariant)?;
    let key = if let Some(correspondence) = &state.correspondence {
        json!({"correspondence":correspondence})
    } else if state.ambiguous {
        json!({"ambiguousStructuralKey":state.semantic_key})
    } else {
        json!({"unmatchedSide":side,"originalNodeId":id})
    };
    Ok((key, state.correspondence.clone(), state.ambiguous))
}
fn nonrange_flags(original: &Value, evidence: bool) -> Value {
    let mut flags = original.clone();
    if let Some(fields) = flags.as_object_mut() {
        for field in [
            "id",
            "baseEvidenceId",
            "baseObservationId",
            "snapshotId",
            "path",
            "contentHash",
            "startByte",
            "endByte",
            "startLine",
            "endLine",
            "overlayArtifactId",
            "source",
            "target",
            "evidenceIds",
            "sourceBindings",
        ] {
            fields.remove(field);
        }
        if !evidence {
            fields.remove("targetLabel");
        }
    }
    flags
}
fn collect_observation_side<'view>(
    view: &'view View,
    side: &str,
    states: &BTreeMap<String, NodeState>,
    texts: &mut Texts<'_>,
    evidence: &BTreeMap<&str, &Value>,
    groups: &mut BTreeMap<String, ObservationGroup<'view>>,
    deadline: &Deadline,
) -> Result<()> {
    for original in &view.observations {
        deadline.check()?;
        let (source_key, source_correspondence, source_ambiguous) =
            endpoint(states, text(original, "source")?, side)?;
        let (target_key, target_correspondence, target_ambiguous) =
            if let Some(id) = original["target"].as_str() {
                endpoint(states, id, side)?
            } else {
                (
                    json!({"unresolvedTargetLabel":original["targetLabel"]}),
                    None,
                    false,
                )
            };
        let mut evidence_keys = Vec::new();
        for id in values(&original["evidenceIds"])? {
            deadline.check()?;
            let anchor = evidence
                .get(id.as_str().ok_or_else(invariant)?)
                .ok_or_else(invariant)?;
            evidence_keys.push(
                json!({"snippetHash":texts.anchor(anchor,deadline)?,"precision":anchor["anchor"],
                "kindAndFlags":nonrange_flags(anchor,true)}),
            );
        }
        let key = canonical_digest(
            &json!({"profile":"yoke_view_comparison_observation_key_1",
            "source":source_key,"target":target_key,"targetLabel":original["targetLabel"],
            "relation":original["relation"],"resolution":original["resolution"],"origin":original["origin"],
            "producer":original["producer"],"nonrangeFlags":nonrange_flags(original,false),"evidence":evidence_keys}),
            deadline,
        )?;
        let group = groups.entry(key.clone()).or_insert_with(|| ObservationGroup {
            descriptor:json!({"semanticKey":key,"sourceCorrespondenceId":source_correspondence,"targetCorrespondenceId":target_correspondence,
                "relation":original["relation"],"resolution":original["resolution"],"origin":original["origin"],"producer":original["producer"]}),
            before:Vec::new(),after:Vec::new(),ambiguous:source_ambiguous || target_ambiguous,
        });
        if side == "before" {
            group.before.push(original);
        } else {
            group.after.push(original);
        }
    }
    Ok(())
}

fn compare_observations(
    before: &View,
    after: &View,
    nodes: &NodeResult,
    left_texts: &mut Texts<'_>,
    right_texts: &mut Texts<'_>,
    rows: &mut Rows,
    deadline: &Deadline,
) -> Result<(Vec<Value>, Vec<Value>)> {
    let left_evidence = before
        .evidence
        .iter()
        .map(|e| Ok((text(e, "id")?, e)))
        .collect::<Result<BTreeMap<_, _>>>()?;
    let right_evidence = after
        .evidence
        .iter()
        .map(|e| Ok((text(e, "id")?, e)))
        .collect::<Result<BTreeMap<_, _>>>()?;
    let mut groups: BTreeMap<String, ObservationGroup<'_>> = BTreeMap::new();
    collect_observation_side(
        before,
        "before",
        &nodes.before,
        left_texts,
        &left_evidence,
        &mut groups,
        deadline,
    )?;
    collect_observation_side(
        after,
        "after",
        &nodes.after,
        right_texts,
        &right_evidence,
        &mut groups,
        deadline,
    )?;
    let mut ordinary = Vec::new();
    let mut ambiguous = Vec::new();
    for (_, mut group) in groups {
        deadline.check()?;
        if !group.ambiguous && group.before.len() == group.after.len() {
            continue;
        }
        rows.charge(1)?;
        let mut before_rows = Vec::new();
        let mut after_rows = Vec::new();
        for (view, side, input, evidence, output) in [
            (
                before,
                "before",
                &group.before,
                &left_evidence,
                &mut before_rows,
            ),
            (
                after,
                "after",
                &group.after,
                &right_evidence,
                &mut after_rows,
            ),
        ] {
            for original in input {
                deadline.check()?;
                rows.ensure_additional(observation_units(original)?)?;
                let row = redact_observation(view, original, side, evidence, deadline)?;
                rows.push(output, row, deadline)?;
            }
        }
        let old = i64::try_from(group.before.len())
            .map_err(|_| anyhow!("comparison_capacity_exceeded"))?;
        let new = i64::try_from(group.after.len())
            .map_err(|_| anyhow!("comparison_capacity_exceeded"))?;
        group.descriptor["beforeCount"] = json!(old);
        group.descriptor["afterCount"] = json!(new);
        group.descriptor["delta"] = json!(new - old);
        group.descriptor["beforeRows"] = json!(before_rows);
        group.descriptor["afterRows"] = json!(after_rows);
        group.descriptor["occurrenceIdentity"] = json!("not_inferred");
        if group.ambiguous {
            group.descriptor["identityBasis"] = json!("ambiguous_structural_label_candidates");
            group.descriptor["verifiedIdentity"] = json!(false);
            ambiguous.push(group.descriptor);
        } else {
            ordinary.push(group.descriptor);
        }
    }
    Ok((ordinary, ambiguous))
}
