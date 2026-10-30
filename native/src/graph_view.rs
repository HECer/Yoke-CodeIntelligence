//! Immutable query views combine explicitly selected source-bound overlays.
use crate::{
    capture::Deadline,
    model::{Graph, Snapshot, hash, identity, valid_id},
    paths::{is_link, utf8_range},
    store::Store,
};
use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    fs,
    io::Read,
};
const MAX_BYTES: usize = 32_000_000;
pub const RECORD_KIND: &str = "native_graph_view_v1";
#[path = "graph_view_comparison_input.rs"]
pub(crate) mod comparison_input;

#[cfg(all(test, windows))]
#[path = "graph_view_comparison_input_tests.rs"]
mod comparison_input_tests;

#[cfg(all(test, windows))]
#[path = "graph_view_comparison_pin_tests.rs"]
mod comparison_pin_tests;

#[cfg(test)]
#[path = "graph_view_comparison_boundary_tests.rs"]
mod comparison_boundary_tests;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum StructuralKind {
    Manifest,
    Config,
    Markdown,
}
fn default_kinds() -> Vec<StructuralKind> {
    vec![
        StructuralKind::Manifest,
        StructuralKind::Config,
        StructuralKind::Markdown,
    ]
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BuildOptions {
    #[serde(default = "default_kinds")]
    pub structural_kinds: Vec<StructuralKind>,
    #[serde(default)]
    pub structural_overlay_ids: Vec<String>,
    #[serde(default)]
    pub semantic_overlay_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub meaning_graph_ids: Vec<String>,
}
impl Default for BuildOptions {
    fn default() -> Self {
        Self {
            structural_kinds: default_kinds(),
            structural_overlay_ids: vec![],
            semantic_overlay_ids: vec![],
            meaning_graph_ids: vec![],
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct View {
    pub id: String,
    pub artifact_id: String,
    pub snapshot_id: String,
    pub workspace_id: String,
    pub base_graph_hash: String,
    pub overlays: Vec<Value>,
    pub nodes: Vec<Value>,
    pub observations: Vec<Value>,
    pub evidence: Vec<Value>,
    pub limitations: Vec<String>,
    pub complete: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub meaning: Option<crate::meaning_view::MeaningLayer>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct ArchivedInput {
    artifact_id: String,
    payload: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct ArchiveProof {
    pub view: View,
    canonical_graph: Graph,
    inputs: Vec<ArchivedInput>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    capture_scope: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    meaning_archive: Option<crate::meaning_view::MeaningArchive>,
}
impl ArchiveProof {
    /// Candidate source manifests only; this iterator grants no source access.
    pub(crate) fn source_scopes(&self) -> impl Iterator<Item = &Snapshot> {
        self.meaning_archive
            .iter()
            .flat_map(crate::meaning_view::MeaningArchive::source_scopes)
    }
}
#[derive(Debug)]
pub(crate) struct VerifiedArchivedMeaningView {
    snapshot: Snapshot,
    canonical_graph: Graph,
    source_projection: Graph,
    view: View,
    proof: ArchiveProof,
    analyzer_epoch: crate::analysis::AnalyzerEpoch,
}
impl VerifiedArchivedMeaningView {
    pub(crate) fn snapshot(&self) -> &Snapshot {
        &self.snapshot
    }
    pub(crate) fn canonical_graph(&self) -> &Graph {
        &self.canonical_graph
    }
    pub(crate) fn source_projection(&self) -> &Graph {
        &self.source_projection
    }
    pub(crate) fn view(&self) -> &View {
        &self.view
    }
    pub(crate) fn proof(&self) -> &ArchiveProof {
        &self.proof
    }
    pub(crate) fn analyzer_epoch(&self) -> crate::analysis::AnalyzerEpoch {
        self.analyzer_epoch
    }
}
pub(crate) fn archive_meaning_view_by_id(
    store: &Store,
    view_id: &str,
    max_proof_bytes: usize,
    max_source_bytes: usize,
    deadline: &Deadline,
) -> Result<VerifiedArchivedMeaningView> {
    archive_meaning_view_by_id_guarded(
        store,
        view_id,
        max_proof_bytes,
        max_source_bytes,
        &mut |scope, d| preflight_native_archive_scope(store, scope, d),
        deadline,
    )
}
pub(crate) fn archive_meaning_view_by_id_guarded(
    store: &Store,
    view_id: &str,
    max_proof_bytes: usize,
    max_source_bytes: usize,
    preflight: &mut crate::meaning_build::FrozenScopePreflight<'_>,
    deadline: &Deadline,
) -> Result<VerifiedArchivedMeaningView> {
    archive_registered_meaning_view(
        store,
        view_id,
        None,
        max_proof_bytes,
        max_source_bytes,
        preflight,
        deadline,
    )
}
#[cfg(all(test, windows))]
#[path = "meaning_archive_by_id_order_tests.rs"]
mod archive_by_id_order_tests;

pub(crate) fn archive_meaning_view(
    store: &Store,
    view: &View,
    max_proof_bytes: usize,
    max_source_bytes: usize,
    deadline: &Deadline,
) -> Result<VerifiedArchivedMeaningView> {
    archive_registered_meaning_view(
        store,
        &view.id,
        Some(view),
        max_proof_bytes,
        max_source_bytes,
        &mut |scope, d| preflight_native_archive_scope(store, scope, d),
        deadline,
    )
}
fn archive_record_matches_view(record: &Record, view: &View) -> Result<()> {
    let layer = view
        .meaning
        .as_ref()
        .context("Meaning archive requires a typed layer")?;
    ensure!(
        record.view_id == view.id
            && record.artifact_id == view.artifact_id
            && record.snapshot_id == view.snapshot_id
            && record.workspace_id == view.workspace_id
            && record.base_graph_hash == view.base_graph_hash
            && record.meaning_graph_ids
                == layer
                    .selections
                    .iter()
                    .map(|s| s.graph_id.clone())
                    .collect::<Vec<_>>()
            && record.overlay_ids
                == view
                    .overlays
                    .iter()
                    .map(|s| s["artifactId"].as_str().unwrap_or("").to_owned())
                    .collect::<Vec<_>>(),
        "Meaning archive differs from published View registration"
    );
    Ok(())
}
fn archive_registered_meaning_view(
    store: &Store,
    view_id: &str,
    expected_view: Option<&View>,
    max_proof_bytes: usize,
    max_source_bytes: usize,
    preflight: &mut crate::meaning_build::FrozenScopePreflight<'_>,
    deadline: &Deadline,
) -> Result<VerifiedArchivedMeaningView> {
    meaning_archive_limits(max_proof_bytes, max_source_bytes, deadline)?;
    store.set_deadline(deadline)?;
    let _read = if store.db.is_autocommit() {
        Some(rusqlite::Transaction::new_unchecked(
            &store.db,
            rusqlite::TransactionBehavior::Deferred,
        )?)
    } else {
        None
    };
    let raw_record = record_value(store, RECORD_KIND, view_id, deadline)?;
    let record: Record = crate::json::decode(raw_record.clone())?;
    ensure!(
        serde_json::to_value(&record)? == raw_record
            && record.view_id == view_id
            && record.workspace_id == store.workspace_id
            && valid_id(&record.snapshot_id)
            && valid_id(&record.base_graph_hash)
            && valid_id(&record.artifact_id)
            && !record.meaning_graph_ids.is_empty(),
        "Invalid typed Meaning View registration"
    );
    if let Some(view) = expected_view {
        archive_record_matches_view(&record, view)?;
    }
    let (snapshot, canonical_graph) = crate::meaning_build::archive_initial_scope(
        store,
        &record.snapshot_id,
        max_proof_bytes,
        deadline,
    )?;
    ensure!(
        identity(&canonical_graph) == record.base_graph_hash,
        "Meaning archive canonical graph binding mismatch"
    );
    let mut input_bytes = 0;
    for id in &record.overlay_ids {
        charge_input(store, id, &mut input_bytes, deadline)?;
    }
    // The bounded registration supplies explicit published selections. No ordinary
    // View load is allowed before this complete source-policy preflight.
    let (layer, meaning_archive) = crate::meaning_view::archive_selected_ids_with_scope_preflight(
        store,
        &snapshot,
        &canonical_graph,
        &record.meaning_graph_ids,
        crate::meaning_build::ArchivePreparationLimits {
            remaining_input_bytes: MAX_BYTES
                .checked_sub(input_bytes)
                .context("View input capacity exceeded")?,
            max_archive_bytes: max_proof_bytes,
            max_source_bytes,
        },
        preflight,
        deadline,
    )?;
    let published = artifact(store, &record.artifact_id, deadline)?;
    let mut view: View = crate::json::decode(crate::json::parse(&published, MAX_BYTES)?)?;
    ensure!(
        view.artifact_id.is_empty() && serde_json::to_vec(&view)? == published,
        "Meaning archive View differs from original published bytes"
    );
    view.artifact_id = record.artifact_id.clone();
    archive_record_matches_view(&record, &view)?;
    ensure!(
        view.meaning.as_ref() == Some(&layer),
        "Meaning View layer differs from checked published proof"
    );
    if let Some(expected) = expected_view {
        ensure!(
            serde_json::to_value(expected)? == serde_json::to_value(&view)?,
            "Meaning archive differs from supplied complete View"
        );
    }
    let mut inputs = Vec::with_capacity(record.overlay_ids.len());
    for id in &record.overlay_ids {
        inputs.push(ArchivedInput {
            artifact_id: id.clone(),
            payload: String::from_utf8(artifact(store, id, deadline)?)?,
        });
    }
    let proof = ArchiveProof {
        view,
        canonical_graph,
        inputs,
        capture_scope: Some(crate::capture::archived_capture_scope(
            store, &snapshot, deadline,
        )?),
        meaning_archive: Some(meaning_archive),
    };
    // Reuse one native read policy per complete admitted scope, never a per-file rebuild.
    let index = crate::meaning_build::FrozenSourceIndex::new(
        std::iter::once(&snapshot).chain(proof.source_scopes()),
        max_source_bytes,
        deadline,
    )?;
    let policies = index
        .scopes()
        .map(|scope| {
            Ok((
                scope.id.as_str(),
                crate::capture::source_scope_policy(store, scope)?,
            ))
        })
        .collect::<Result<BTreeMap<_, _>>>()?;
    replay_meaning_view_archive(
        &snapshot,
        &proof,
        &mut |member, d| {
            d.check()?;
            let policy = policies
                .get(member.snapshot().id.as_str())
                .context("Meaning archive source policy absent")?;
            let file = member.file();
            policy.require_legacy_text("Meaning archive source replay")?;
            ensure!(
                policy.file(&file.path)? == file,
                "Meaning archive source membership mismatch"
            );
            policy.read_source(&file.path)
        },
        max_proof_bytes,
        max_source_bytes,
        deadline,
    )
}

pub(crate) fn replay_meaning_view_archive(
    snapshot: &Snapshot,
    proof: &ArchiveProof,
    read_source: &mut crate::meaning_build::FrozenMemberReader<'_>,
    max_proof_bytes: usize,
    max_source_bytes: usize,
    deadline: &Deadline,
) -> Result<VerifiedArchivedMeaningView> {
    meaning_archive_limits(max_proof_bytes, max_source_bytes, deadline)?;
    crate::json::encode_bounded(proof, max_proof_bytes, deadline)?;
    let view = &proof.view;
    let layer = view
        .meaning
        .as_ref()
        .context("Meaning archive View layer absent")?;
    let meaning_archive = proof
        .meaning_archive
        .as_ref()
        .context("Meaning archive proof absent")?;
    ensure!(
        snapshot.id == view.snapshot_id
            && snapshot.workspace_id == view.workspace_id
            && proof.canonical_graph.snapshot_id == snapshot.id
            && identity(&proof.canonical_graph) == view.base_graph_hash
            && valid_id(&view.id)
            && valid_id(&view.artifact_id),
        "Meaning archive source scope mismatch"
    );
    let capture_scope = proof
        .capture_scope
        .as_ref()
        .context("Meaning archive capture proof absent")?;
    let epoch = crate::capture::archived_analyzer_epoch(snapshot, capture_scope, deadline)?
        .context("Meaning archive native extraction epoch unavailable")?;
    ensure!(
        matches!(
            epoch,
            crate::analysis::AnalyzerEpoch::V8 | crate::analysis::AnalyzerEpoch::V9
        ),
        "Meaning archive requires native epoch8 or epoch9"
    );
    let scopes = crate::meaning_build::FrozenSourceIndex::new(
        std::iter::once(snapshot).chain(proof.source_scopes()),
        max_source_bytes,
        deadline,
    )?;
    let mut objects = BTreeMap::new();
    for scope in scopes.scopes() {
        let bound = scopes.bind(scope, deadline)?;
        for member in bound.members() {
            deadline.check()?;
            let file = member.file();
            if let std::collections::btree_map::Entry::Vacant(entry) =
                objects.entry(file.hash.clone())
            {
                let bytes = read_source(&member, deadline)?;
                ensure!(
                    bytes.len() == file.size && hash(&bytes) == file.hash,
                    "Meaning archive frozen source hash/size mismatch"
                );
                std::str::from_utf8(&bytes)?;
                entry.insert(bytes);
            }
        }
    }
    let mut texts = BTreeMap::new();
    for file in &snapshot.files {
        texts.insert(
            file.path.clone(),
            String::from_utf8(objects[&file.hash].clone())?,
        );
    }
    ensure!(
        validate_archived_canonical_graph(
            snapshot,
            &proof.canonical_graph,
            capture_scope,
            &texts,
            deadline
        )? == Some(epoch),
        "Meaning archive canonical extraction epoch mismatch"
    );
    let mut sources = BTreeMap::new();
    for file in &snapshot.files {
        let text = texts[&file.path].clone();
        sources.insert(
            file.path.clone(),
            Source {
                lines: crate::paths::line_starts(&text),
                text,
                hash: file.hash.clone(),
            },
        );
    }
    safe_file_presentation(view)?;
    validate_contents(view, snapshot, &sources, deadline)?;
    let mut expected = canonical_view(snapshot, &proof.canonical_graph, &sources, deadline)?;
    let selected_bytes =
        replay_archived_inputs(snapshot, proof, &sources, &mut expected, deadline)?;
    let checked_layer = crate::meaning_view::replay_archive_members(
        snapshot,
        &proof.canonical_graph,
        meaning_archive,
        &mut |member, _| {
            let file = member.file();
            objects
                .get(&file.hash)
                .cloned()
                .context("Meaning callback source object absent")
        },
        max_proof_bytes,
        max_source_bytes,
        deadline,
    )?;
    ensure!(
        &checked_layer == layer,
        "Archived Meaning layer differs from complete checked proof"
    );
    let mut total = selected_bytes;
    charge_input_bytes(
        crate::meaning_view::charged_input_bytes(&checked_layer)?,
        &mut total,
    )?;
    expected.meaning = Some(checked_layer);
    validate_contents(&expected, snapshot, &sources, deadline)?;
    expected.id = identity(&expected);
    let sealed = crate::json::encode_bounded(&expected, MAX_BYTES, deadline)?;
    expected.artifact_id = hash(&sealed);
    ensure!(
        serde_json::to_value(&expected)? == serde_json::to_value(view)?,
        "Archived complete View differs from canonical and selected proof replay"
    );
    let source_projection = project_source_rows(&expected)?;
    Ok(VerifiedArchivedMeaningView {
        snapshot: snapshot.clone(),
        canonical_graph: proof.canonical_graph.clone(),
        source_projection,
        view: expected,
        proof: proof.clone(),
        analyzer_epoch: epoch,
    })
}
fn meaning_archive_limits(proof: usize, source: usize, deadline: &Deadline) -> Result<()> {
    deadline.check()?;
    ensure!(
        (1..=MAX_BYTES).contains(&proof) && (1..=64_000_000).contains(&source),
        "Invalid typed View archive limits"
    );
    Ok(())
}
fn preflight_native_archive_scope(
    store: &Store,
    snapshot: &Snapshot,
    deadline: &Deadline,
) -> Result<()> {
    deadline.check()?;
    ensure!(
        snapshot.workspace_id == store.workspace_id
            && serde_json::to_value(store.snapshot(&snapshot.id)?)?
                == serde_json::to_value(snapshot)?,
        "Meaning archive requires an exact local published source manifest"
    );
    let policy = crate::capture::source_scope_policy(store, snapshot)?;
    let epoch = crate::capture::analyzer_epoch_for_snapshot(store, snapshot)?;
    ensure!(
        matches!(
            epoch,
            crate::analysis::AnalyzerEpoch::V8 | crate::analysis::AnalyzerEpoch::V9
        ),
        "Meaning archive source has no native epoch8 or epoch9 authority"
    );
    for file in &snapshot.files {
        deadline.check()?;
        ensure!(
            policy.file(&file.path)? == file,
            "Meaning archive source policy membership mismatch"
        );
    }
    Ok(())
}
fn replay_archived_inputs(
    snapshot: &Snapshot,
    proof: &ArchiveProof,
    sources: &Sources,
    expected: &mut View,
    deadline: &Deadline,
) -> Result<usize> {
    let view = &proof.view;
    ensure!(
        proof.inputs.len() == view.overlays.len(),
        "Archive selected input count mismatch"
    );
    let mut bytes = 0usize;
    let mut ids = BTreeSet::new();
    for (input, selected) in proof.inputs.iter().zip(&view.overlays) {
        deadline.check()?;
        bytes = bytes
            .checked_add(input.payload.len())
            .context("Archive selected bytes overflow")?;
        ensure!(
            bytes <= MAX_BYTES,
            "Archive selected bytes capacity exceeded"
        );
        ensure!(
            ids.insert(&input.artifact_id)
                && selected["artifactId"] == input.artifact_id
                && crate::model::hash(input.payload.as_bytes()) == input.artifact_id,
            "Archive selected artifact identity mismatch"
        );
        let raw = crate::json::parse(input.payload.as_bytes(), MAX_BYTES)?;
        let semantic = match selected["origin"].as_str() {
            Some("structural_overlay") => false,
            Some("semantic_overlay") => true,
            _ => bail!("Unsupported archive selected origin"),
        };
        if !semantic {
            let _: crate::manifest_graph::DerivedGraph = crate::json::decode(raw.clone())?;
            ensure!(
                raw["artifactId"] == ""
                    && raw["snapshotId"] == snapshot.id
                    && raw["workspaceId"] == snapshot.workspace_id
                    && raw["graphHash"] == view.base_graph_hash
                    && matches!(
                        raw["producer"].as_str(),
                        Some(
                            "yoke-manifest-graph-1"
                                | "yoke-config-graph-1"
                                | "yoke-markdown-graph-2"
                        )
                    ),
                "Archive structural origin mismatch"
            );
        } else {
            ensure!(
                raw["snapshotId"] == snapshot.id
                    && raw["baseGraphHash"] == view.base_graph_hash
                    && raw["overlayId"] == selected["overlayId"],
                "Archive semantic origin mismatch"
            );
        }
        overlay(
            expected,
            snapshot,
            sources,
            &raw,
            &input.artifact_id,
            semantic,
            deadline,
        )?;
    }
    Ok(bytes)
}
pub(crate) fn archive(
    store: &Store,
    view: &View,
    deadline: &Deadline,
) -> Result<(Snapshot, Graph, ArchiveProof)> {
    checked(store, view, deadline)?;
    ensure!(
        view.meaning.is_none(),
        "Typed meaning collection archives are not available through the legacy source projection"
    );
    let snapshot = store.snapshot(&view.snapshot_id)?;
    let canonical_graph = store.graph(&view.snapshot_id)?;
    let mut inputs = vec![];
    let mut total = 0usize;
    for selected in &view.overlays {
        let id = selected["artifactId"]
            .as_str()
            .context("Archive overlay artifact missing")?;
        let bytes = artifact(store, id, deadline)?;
        total = total
            .checked_add(bytes.len())
            .context("Archive overlay byte overflow")?;
        ensure!(
            total <= MAX_BYTES,
            "Archive overlay aggregate capacity exceeded"
        );
        inputs.push(ArchivedInput {
            artifact_id: id.into(),
            payload: String::from_utf8(bytes)?,
        });
    }
    let proof = ArchiveProof {
        view: view.clone(),
        canonical_graph,
        inputs,
        capture_scope: Some(crate::capture::archived_capture_scope(
            store, &snapshot, deadline,
        )?),
        meaning_archive: None,
    };
    Ok((snapshot, projection(view)?, proof))
}
pub(crate) fn validate_archive(
    snapshot: &Snapshot,
    proof: &ArchiveProof,
    texts: &BTreeMap<String, String>,
    deadline: &Deadline,
) -> Result<(Graph, Option<crate::analysis::AnalyzerEpoch>)> {
    ensure!(
        proof.view.meaning.is_none() && proof.meaning_archive.is_none(),
        "Typed meaning archives require a multi-snapshot archive consumer"
    );
    let epoch = validate_archived_canonical_graph(
        snapshot,
        &proof.canonical_graph,
        proof.capture_scope.as_ref().unwrap_or(&Value::Null),
        texts,
        deadline,
    )?;
    let view = &proof.view;
    safe_file_presentation(view)?;
    ensure!(
        snapshot.workspace_id == view.workspace_id
            && snapshot.id == view.snapshot_id
            && proof.canonical_graph.snapshot_id == snapshot.id
            && identity(&proof.canonical_graph) == view.base_graph_hash
            && valid_id(&view.id)
            && valid_id(&view.artifact_id)
            && proof.inputs.len() == view.overlays.len(),
        "Archive view source/origin scope mismatch"
    );
    let mut sealed = view.clone();
    sealed.artifact_id.clear();
    ensure!(
        crate::model::hash(serde_json::to_vec(&sealed)?) == view.artifact_id,
        "Archive original view artifact mismatch"
    );
    sealed.id.clear();
    ensure!(
        identity(&sealed) == view.id,
        "Archive original view identity mismatch"
    );
    let mut sources = BTreeMap::new();
    let mut total = 0usize;
    for file in &snapshot.files {
        deadline.check()?;
        let text = texts
            .get(&file.path)
            .context("Archive source bytes absent")?;
        ensure!(
            text.len() == file.size
                && file.size <= 4_000_000
                && crate::model::hash(text.as_bytes()) == file.hash,
            "Archive source hash/size mismatch"
        );
        total = total
            .checked_add(file.size)
            .context("Archive source byte overflow")?;
        ensure!(total <= 64_000_000, "Archive source capacity exceeded");
        sources.insert(
            file.path.clone(),
            Source {
                text: text.clone(),
                hash: file.hash.clone(),
                lines: crate::paths::line_starts(text),
            },
        );
    }
    ensure!(
        texts.len() == snapshot.files.len(),
        "Archive source manifest mismatch"
    );
    validate_contents(view, snapshot, &sources, deadline)?;
    let mut expected = canonical_view(snapshot, &proof.canonical_graph, &sources, deadline)?;
    replay_archived_inputs(snapshot, proof, &sources, &mut expected, deadline)?;
    ensure!(
        expected.nodes == view.nodes
            && expected.observations == view.observations
            && expected.evidence == view.evidence
            && expected.overlays == view.overlays
            && expected.limitations == view.limitations,
        "Archive view rows differ from original canonical/selected artifacts"
    );
    Ok((projection(view)?, epoch))
}
/// Hashes seal claims; replay binds canonical claims to exact captured source bytes.
pub(crate) fn validate_archived_canonical_graph(
    snapshot: &Snapshot,
    graph: &Graph,
    capture_scope: &Value,
    texts: &BTreeMap<String, String>,
    deadline: &Deadline,
) -> Result<Option<crate::analysis::AnalyzerEpoch>> {
    let epoch = crate::capture::archived_analyzer_epoch(snapshot, capture_scope, deadline)?;
    let Some(epoch) = epoch else {
        return Ok(None);
    };
    let mut extractions = Vec::with_capacity(snapshot.files.len());
    for file in &snapshot.files {
        deadline.check()?;
        let text = texts
            .get(&file.path)
            .context("Archive replay source absent")?;
        ensure!(
            text.len() == file.size && hash(text.as_bytes()) == file.hash,
            "Archive replay source hash/size mismatch"
        );
        extractions.push(crate::analysis::extract_at(epoch, file, text, deadline)?);
    }
    ensure!(
        texts.len() == snapshot.files.len(),
        "Archive replay source manifest mismatch"
    );
    let expected = crate::analysis::assemble_at(epoch, snapshot, extractions, deadline)?;
    ensure!(
        identity(&expected) == identity(graph),
        "Archive canonical graph differs from exact source replay"
    );
    Ok(Some(epoch))
}
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Record {
    view_id: String,
    artifact_id: String,
    snapshot_id: String,
    workspace_id: String,
    base_graph_hash: String,
    overlay_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    meaning_graph_ids: Vec<String>,
}
struct Source {
    text: String,
    hash: String,
    lines: Vec<usize>,
}
type Sources = BTreeMap<String, Source>;
fn sources(store: &Store, snapshot: &Snapshot, deadline: &Deadline) -> Result<Sources> {
    let mut out = BTreeMap::new();
    let mut bytes = 0usize;
    for file in &snapshot.files {
        deadline.check()?;
        bytes = bytes
            .checked_add(file.size)
            .context("View source capacity overflow")?;
        ensure!(bytes <= 64_000_000, "View source capacity exceeded");
        let (text, hash) = crate::query::source(store, snapshot, &file.path)?;
        out.insert(
            file.path.clone(),
            Source {
                lines: crate::paths::line_starts(&text),
                text,
                hash,
            },
        );
    }
    Ok(out)
}
fn number(value: &Value, key: &str) -> Result<usize> {
    usize::try_from(
        value[key]
            .as_u64()
            .with_context(|| format!("Missing unsigned view field: {key}"))?,
    )
    .context("View byte offset overflow")
}
fn binding(
    snapshot: &Snapshot,
    sources: &Sources,
    path: &str,
    start: usize,
    end: usize,
    precision: &str,
) -> Result<Value> {
    let source = sources
        .get(path)
        .context("View anchor path outside captured source")?;
    utf8_range(&source.text, start, end)?;
    Ok(
        json!({"snapshotId":snapshot.id,"path":path,"contentHash":source.hash,"startByte":start,"endByte":end,"startLine":source.lines.partition_point(|at|*at<=start),"endLine":source.lines.partition_point(|at|*at<=end),"anchor":precision}),
    )
}
fn normalize_anchor(snapshot: &Snapshot, sources: &Sources, value: &Value) -> Result<Value> {
    ensure!(
        value["snapshotId"] == snapshot.id,
        "View evidence snapshot mismatch"
    );
    let path = value["path"]
        .as_str()
        .context("View evidence path missing")?;
    let source = sources
        .get(path)
        .context("View evidence path outside source")?;
    ensure!(
        value["contentHash"] == source.hash,
        "View evidence source hash mismatch"
    );
    let precision = value["anchor"]
        .as_str()
        .context("View evidence precision missing")?;
    ensure!(
        [
            "exact",
            "file",
            "unique_literal_occurrence",
            "exact_authored_link",
            "exact_authored_citation",
            "descriptor_line",
            "call_range",
            "symbol_selection",
            "source_range"
        ]
        .contains(&precision),
        "Unsupported view evidence precision"
    );
    let normalized = binding(
        snapshot,
        sources,
        path,
        number(value, "startByte")?,
        number(value, "endByte")?,
        precision,
    )?;
    if let Some(line) = value.get("startLine") {
        ensure!(
            line == &normalized["startLine"],
            "View evidence start line mismatch"
        );
    }
    if let Some(line) = value.get("endLine") {
        let end = number(value, "endByte")?;
        let inclusive = source
            .lines
            .partition_point(|at| *at <= end.saturating_sub(1));
        ensure!(
            line == &normalized["endLine"] || line.as_u64() == Some(inclusive as u64),
            "View evidence end line mismatch"
        );
    }
    Ok(normalized)
}
fn record_value(store: &Store, kind: &str, id: &str, deadline: &Deadline) -> Result<Value> {
    deadline.check()?;
    store.set_deadline(deadline)?;
    ensure!(valid_id(id), "Invalid selected graph artifact ID");
    let length: i64 = store.db.query_row(
        "SELECT length(CAST(value AS BLOB)) FROM records WHERE kind=?1 AND id=?2",
        rusqlite::params![kind, id],
        |r| r.get(0),
    )?;
    ensure!(
        (0..=64000).contains(&length),
        "View record capacity exceeded"
    );
    let raw: String = store.db.query_row(
        "SELECT value FROM records WHERE kind=?1 AND id=?2 AND length(CAST(value AS BLOB))<=64000",
        rusqlite::params![kind, id],
        |r| r.get(0),
    )?;
    crate::json::parse(raw.as_bytes(), 64000)
}
fn artifact(store: &Store, id: &str, deadline: &Deadline) -> Result<Vec<u8>> {
    artifact_limit(store, id, MAX_BYTES, deadline)
}
fn preflight_view_registration(
    store: &Store,
    tx: &rusqlite::Transaction<'_>,
    id: &str,
    expected: &Value,
    deadline: &Deadline,
) -> Result<()> {
    deadline.check()?;
    ensure!(
        std::ptr::eq(&**tx, &store.db),
        "View publication belongs to a different Store connection"
    );
    let present: bool = tx.query_row(
        "SELECT EXISTS(SELECT 1 FROM records WHERE kind=?1 AND id=?2)",
        rusqlite::params![RECORD_KIND, id],
        |row| row.get(0),
    )?;
    if present {
        ensure!(
            record_value(store, RECORD_KIND, id, deadline)? == *expected,
            "Stored graph view record differs"
        );
    }
    Ok(())
}
fn charge_input(store: &Store, id: &str, used: &mut usize, deadline: &Deadline) -> Result<()> {
    let registered = record_value(store, "artifact", id, deadline)?;
    fields(&registered, &["artifactId", "totalBytes"])?;
    ensure!(
        registered["artifactId"] == id,
        "Selected input artifact identity mismatch"
    );
    charge_input_bytes(number(&registered, "totalBytes")?, used)
}
fn charge_input_bytes(bytes: usize, used: &mut usize) -> Result<()> {
    *used = used
        .checked_add(bytes)
        .context("Selected overlay byte capacity overflow")?;
    ensure!(
        *used <= MAX_BYTES,
        "Selected overlay aggregate byte capacity exceeded"
    );
    Ok(())
}
fn artifact_limit(store: &Store, id: &str, maximum: usize, deadline: &Deadline) -> Result<Vec<u8>> {
    ensure!(valid_id(id), "Invalid view artifact ID");
    let record = record_value(store, "artifact", id, deadline)?;
    fields(&record, &["artifactId", "totalBytes"])?;
    ensure!(
        record["artifactId"] == id,
        "View artifact registration mismatch"
    );
    let expected = number(&record, "totalBytes")?;
    ensure!(expected <= maximum, "View artifact byte capacity exceeded");
    let path = store.dir.join("blobs").join(id);
    let meta = fs::symlink_metadata(&path)?;
    ensure!(
        meta.is_file() && !is_link(&meta) && meta.len() == expected as u64,
        "View artifact type or size mismatch"
    );
    let mut input = fs::File::open(path)?;
    let mut out = Vec::with_capacity(expected);
    let mut chunk = [0u8; 65536];
    loop {
        deadline.check()?;
        let count = input.read(&mut chunk)?;
        if count == 0 {
            break;
        }
        ensure!(
            out.len().saturating_add(count) <= expected,
            "View artifact grew during read"
        );
        out.extend_from_slice(&chunk[..count]);
    }
    ensure!(
        out.len() == expected && hash(&out) == id,
        "View artifact content mismatch"
    );
    deadline.check()?;
    Ok(out)
}
fn source_overlay(
    store: &Store,
    snapshot: &Snapshot,
    base: &Graph,
    id: &str,
    deadline: &Deadline,
) -> Result<Value> {
    let record = record_value(store, crate::manifest_graph::RECORD_KIND, id, deadline)?;
    fields(
        &record,
        &[
            "artifactId",
            "snapshotId",
            "workspaceId",
            "graphHash",
            "producer",
        ],
    )?;
    ensure!(
        record["artifactId"] == id
            && record["snapshotId"] == snapshot.id
            && record["workspaceId"] == snapshot.workspace_id
            && record["graphHash"] == identity(base),
        "Structural overlay source scope mismatch"
    );
    let value = crate::json::parse(&artifact(store, id, deadline)?, MAX_BYTES)?;
    let _: crate::manifest_graph::DerivedGraph = crate::json::decode(value.clone())?;
    ensure!(
        value["artifactId"] == ""
            && value["snapshotId"] == snapshot.id
            && value["workspaceId"] == snapshot.workspace_id
            && value["graphHash"] == identity(base)
            && value["producer"] == record["producer"],
        "Structural overlay artifact scope mismatch"
    );
    ensure!(
        matches!(
            value["producer"].as_str(),
            Some("yoke-manifest-graph-1" | "yoke-config-graph-1" | "yoke-markdown-graph-2")
        ),
        "Unsupported structural graph overlay producer"
    );
    Ok(value)
}
fn limits(view: &View) -> Result<()> {
    let mut node_count = view.nodes.len();
    let mut edge_count = view.observations.len();
    let mut evidence_count = view.evidence.len();
    let mut selections = view.overlays.len();
    if let Some(layer) = &view.meaning {
        let mut ids: BTreeSet<&str> = view.nodes.iter().filter_map(|n| n["id"].as_str()).collect();
        selections = selections.saturating_add(layer.selections.len());
        for packet in &layer.packets {
            node_count = node_count
                .saturating_add(packet.concepts.len())
                .saturating_add(packet.relations.len())
                .saturating_add(packet.hyperedges.len());
            edge_count = edge_count
                .saturating_add(packet.relations.len())
                .saturating_add(packet.citation_links.len());
            evidence_count = evidence_count.saturating_add(packet.evidence.len());
            for h in &packet.hyperedges {
                edge_count = edge_count.saturating_add(h.participants.len());
            }
            for id in packet
                .concepts
                .iter()
                .map(|n| n.id.as_str())
                .chain(packet.relations.iter().map(|n| n.id.as_str()))
                .chain(packet.hyperedges.iter().map(|n| n.id.as_str()))
            {
                ensure!(
                    ids.insert(id),
                    "Meaning identity conflicts with selected source or another claim"
                );
            }
        }
    }
    ensure!(
        node_count <= 20000 && edge_count <= 100000 && evidence_count <= 100000 && selections <= 32,
        "Graph view capacity exceeded"
    );
    Ok(())
}
fn node_binding(snapshot: &Snapshot, sources: &Sources, node: &Value) -> Result<Value> {
    let path = node["path"].as_str().context("View node path missing")?;
    let source = sources.get(path).context("View node source absent")?;
    ensure!(
        node.get("contentHash").is_none_or(|h| h == &source.hash),
        "View node source hash mismatch"
    );
    binding(
        snapshot,
        sources,
        path,
        number(node, "startByte")?,
        number(node, "endByte")?,
        "source_range",
    )
}
fn overlay(
    view: &mut View,
    snapshot: &Snapshot,
    sources: &Sources,
    raw: &Value,
    artifact_id: &str,
    semantic: bool,
    deadline: &Deadline,
) -> Result<()> {
    let origin = if semantic {
        "semantic_overlay"
    } else {
        "structural_overlay"
    };
    let producer = raw["producer"]
        .as_str()
        .context("Overlay producer missing")?;
    if semantic {
        let version = raw["schemaVersion"]
            .as_u64()
            .context("Semantic overlay version missing")?;
        let mut allowed = vec![
            "schemaVersion",
            "snapshotId",
            "baseGraphHash",
            "sourceBindings",
            "operation",
            "producer",
            "serverProfileHash",
            "nodes",
            "evidence",
            "observations",
            "complete",
            "providerAttestation",
            "limitations",
            "overlayId",
        ];
        if version == 2 {
            allowed.push("providerContext");
            fields(
                &raw["providerContext"],
                &["sourceExecutionAuthorized", "sourcePolicy", "omissions"],
            )?;
            crate::semantic_enrichment::validate_archived_provider_context(
                snapshot,
                &raw["providerContext"],
                deadline,
            )?;
        }
        fields(raw, &allowed)?;
        ensure!(
            matches!(version, 1 | 2)
                && raw["complete"] == false
                && raw["providerAttestation"] == "unverified",
            "Invalid semantic overlay schema"
        );
    }
    let nodes = raw["nodes"].as_array().context("Overlay nodes missing")?;
    let evidence = raw["evidence"]
        .as_array()
        .context("Overlay evidence missing")?;
    let observations = raw["observations"]
        .as_array()
        .context("Overlay observations missing")?;
    let mut node_ids = BTreeMap::new();
    let mut anchor_ids = BTreeMap::new();
    let mut bindings = BTreeMap::new();
    for anchor in evidence {
        deadline.check()?;
        fields(
            anchor,
            &[
                "id",
                "snapshotId",
                "path",
                "contentHash",
                "startByte",
                "endByte",
                "startLine",
                "endLine",
                "anchor",
                "producer",
                "serverProfileHash",
                "trust",
            ],
        )?;
        let original = anchor["id"]
            .as_str()
            .filter(|id| valid_id(id))
            .context("Invalid overlay evidence ID")?;
        let id = identity(&("view_evidence_v1", artifact_id, original));
        ensure!(
            anchor_ids.insert(original.to_owned(), id.clone()).is_none(),
            "Duplicate overlay evidence ID"
        );
        let bound = normalize_anchor(snapshot, sources, anchor)?;
        bindings.insert(original.to_owned(), bound.clone());
        let mut value = anchor.clone();
        for (k, v) in bound.as_object().unwrap() {
            value[k] = v.clone();
        }
        value["id"] = json!(id);
        value["baseEvidenceId"] = json!(original);
        value["overlayArtifactId"] = json!(artifact_id);
        value["origin"] = json!(origin);
        value["producer"] = json!(producer);
        value["trust"] = json!(if semantic {
            "source_bound_provider_claim"
        } else {
            "source_bound_structural_claim"
        });
        view.evidence.push(value);
        limits(view)?;
    }
    for node in nodes {
        deadline.check()?;
        if semantic {
            fields(
                node,
                &[
                    "id",
                    "snapshotId",
                    "path",
                    "contentHash",
                    "startByte",
                    "endByte",
                    "selectionStartByte",
                    "selectionEndByte",
                    "name",
                    "kind",
                    "baseEntityIds",
                    "producer",
                    "serverProfileHash",
                    "origin",
                ],
            )?;
        } else {
            fields(
                node,
                &[
                    "id",
                    "snapshotId",
                    "path",
                    "kind",
                    "name",
                    "ecosystem",
                    "evidenceIds",
                    "jsonPointer",
                ],
            )?;
        }
        let original = node["id"]
            .as_str()
            .filter(|id| valid_id(id))
            .context("Invalid overlay node ID")?;
        let id = identity(&("view_node_v1", artifact_id, original));
        ensure!(
            node_ids.insert(original.to_owned(), id.clone()).is_none(),
            "Duplicate overlay node ID"
        );
        let mut value = node.clone();
        let path = node["path"].as_str().context("Overlay node path missing")?;
        ensure!(
            node["snapshotId"] == snapshot.id
                && sources.contains_key(path)
                && node["name"]
                    .as_str()
                    .is_some_and(|s| !s.is_empty() && s.len() <= 4096),
            "Invalid overlay node label or path"
        );
        let anchors = if semantic {
            vec![node_binding(snapshot, sources, node)?]
        } else {
            let ids = node["evidenceIds"]
                .as_array()
                .context("Structural node evidence missing")?;
            ensure!(
                !ids.is_empty() && ids.len() <= 100,
                "Structural node evidence count invalid"
            );
            ids.iter()
                .map(|id| {
                    let key = id.as_str().context("Invalid structural node evidence ID")?;
                    let b = bindings
                        .get(key)
                        .context("Structural node evidence absent")?;
                    ensure!(
                        b["path"] == path,
                        "Structural node evidence escaped source file"
                    );
                    Ok(b.clone())
                })
                .collect::<Result<Vec<_>>>()?
        };
        value["sourceBindings"] = json!(anchors);
        value["id"] = json!(id);
        value["baseNodeId"] = json!(original);
        value["overlayArtifactId"] = json!(artifact_id);
        value["snapshotId"] = json!(snapshot.id);
        value["origin"] = json!(origin);
        value["producer"] = json!(producer);
        value["trust"] = json!(if semantic {
            "source_bound_provider_claim"
        } else {
            "source_bound_structural_claim"
        });
        if semantic {
            value["providerKind"] = node["kind"].clone();
            value["kind"] = json!(match node["kind"].as_u64() {
                Some(5) => "class",
                Some(6) => "method",
                Some(11) => "interface",
                Some(12) => "function",
                Some(23) => "struct",
                Some(10) => "enum",
                _ => "semantic_symbol",
            });
        }
        if let Some(ids) = node["evidenceIds"].as_array() {
            value["evidenceIds"] = json!(
                ids.iter()
                    .map(|id| anchor_ids.get(id.as_str().unwrap_or("")))
                    .collect::<Vec<_>>()
            );
        }
        view.nodes.push(value);
        limits(view)?;
    }
    let canonical: BTreeSet<_> = view
        .nodes
        .iter()
        .filter(|node| node["origin"] == "canonical")
        .filter_map(|n| n["id"].as_str())
        .collect();
    let mut arc_ids = BTreeSet::new();
    for arc in observations {
        deadline.check()?;
        if semantic {
            fields(
                arc,
                &[
                    "id",
                    "snapshotId",
                    "source",
                    "target",
                    "relation",
                    "resolution",
                    "evidenceIds",
                    "producer",
                    "serverProfileHash",
                    "ordinal",
                ],
            )?;
        } else {
            fields(
                arc,
                &[
                    "id",
                    "source",
                    "target",
                    "targetLabel",
                    "relation",
                    "resolution",
                    "evidenceIds",
                    "dependencyAlias",
                    "scope",
                    "ecosystem",
                    "inherited",
                    "componentType",
                    "event",
                    "fragment",
                    "candidateEntityIds",
                    "targetNamespace",
                ],
            )?;
        }
        let original = arc["id"]
            .as_str()
            .filter(|id| valid_id(id))
            .context("Invalid overlay observation ID")?;
        ensure!(arc_ids.insert(original), "Duplicate overlay observation ID");
        let source = node_ids
            .get(arc["source"].as_str().context("Overlay source missing")?)
            .context("Overlay source node absent")?;
        let target = if arc["target"].is_null() {
            None
        } else {
            let target = arc["target"].as_str().context("Invalid overlay target")?;
            if arc["targetNamespace"] == "canonical_graph" {
                ensure!(
                    canonical.contains(target),
                    "Overlay canonical target absent"
                );
                Some(target.to_owned())
            } else {
                Some(
                    node_ids
                        .get(target)
                        .context("Overlay target node absent")?
                        .clone(),
                )
            }
        };
        let evidence_ids = arc["evidenceIds"]
            .as_array()
            .context("Overlay observation evidence missing")?;
        ensure!(
            !evidence_ids.is_empty() && evidence_ids.len() <= 10000,
            "Overlay observation evidence count invalid"
        );
        let ids = evidence_ids
            .iter()
            .map(|id| {
                anchor_ids
                    .get(id.as_str().unwrap_or(""))
                    .cloned()
                    .context("Overlay observation evidence absent")
            })
            .collect::<Result<Vec<_>>>()?;
        let mut value = arc.clone();
        value["id"] = json!(identity(&("view_observation_v1", artifact_id, original)));
        value["baseObservationId"] = json!(original);
        value["source"] = json!(source);
        value["target"] = json!(target);
        value["evidenceIds"] = json!(ids);
        value["origin"] = json!(origin);
        value["producer"] = json!(producer);
        value["overlayArtifactId"] = json!(artifact_id);
        value["snapshotId"] = json!(snapshot.id);
        value["trust"] = json!(if semantic {
            "source_bound_provider_claim"
        } else {
            "source_bound_structural_claim"
        });
        if semantic && value["targetLabel"].is_null() {
            value["targetLabel"] = json!("provider_semantic_target");
        }
        view.observations.push(value);
        limits(view)?;
    }
    view.overlays.push(json!({"artifactId":artifact_id,"overlayId":raw.get("overlayId"),"producer":producer,"origin":origin,"providerAttestation":if semantic{"unverified"}else{"structural_source_claim"},"serverProfileHash":raw.get("serverProfileHash")}));
    if semantic && raw["schemaVersion"] == 2 {
        let descriptor = view.overlays.last_mut().unwrap();
        descriptor["schemaVersion"] = json!(2);
        descriptor["providerContext"] = raw["providerContext"].clone();
        descriptor["limitations"] = raw["limitations"].clone();
    }

    limits(view)?;
    Ok(())
}
fn canonical_view(
    snapshot: &Snapshot,
    graph: &Graph,
    sources: &Sources,
    deadline: &Deadline,
) -> Result<View> {
    let mut view=View{id:String::new(),artifact_id:String::new(),snapshot_id:snapshot.id.clone(),workspace_id:snapshot.workspace_id.clone(),base_graph_hash:identity(graph),overlays:vec![],nodes:vec![],observations:vec![],evidence:vec![],limitations:vec!["Views combine selected immutable source claims; they do not replace canonical graphs or prove runtime/compiler correctness.".into(),"File and unique-literal anchors retain their declared precision. Semantic provider claims remain independently unverified.".into()],complete:false,meaning:None};
    for node in &graph.entities {
        deadline.check()?;
        let mut value = serde_json::to_value(node)?;
        if node.kind == "file" {
            value["signature"] = json!(format!("{} [{}]", node.path, node.language));
        }
        value["sourceBindings"] = json!([binding(
            snapshot,
            sources,
            &node.path,
            node.start_byte,
            node.end_byte,
            "source_range"
        )?]);
        value["origin"] = json!("canonical");
        value["trust"] = json!("canonical_source_structure");
        view.nodes.push(value);
    }
    for anchor in &graph.evidence {
        let mut value = serde_json::to_value(anchor)?;
        value["origin"] = json!("canonical");
        view.evidence.push(value);
    }
    for arc in &graph.observations {
        let mut value = serde_json::to_value(arc)?;
        value["origin"] = json!("canonical");
        view.observations.push(value);
    }
    limits(&view)?;
    Ok(view)
}
pub fn build(
    store: &Store,
    snapshot: &Snapshot,
    graph: &Graph,
    options: &BuildOptions,
    deadline: &Deadline,
) -> Result<View> {
    crate::graph_extensions::validate_scope(store, snapshot, graph, deadline)?;
    ensure!(
        options.structural_kinds.len() <= 3
            && options.structural_overlay_ids.len()
                + options.semantic_overlay_ids.len()
                + options.meaning_graph_ids.len()
                <= 29,
        "View selection capacity exceeded"
    );
    let unique: BTreeSet<_> = options
        .structural_kinds
        .iter()
        .map(|kind| format!("{kind:?}"))
        .collect();
    ensure!(
        unique.len() == options.structural_kinds.len(),
        "Duplicate structural kind selected"
    );
    // Selection scope and its complete immutable dependency budget precede
    // structural preparation. All publication waits for complete View admission.
    let meaning = crate::meaning_view::load_selected(
        store,
        snapshot,
        graph,
        &options.meaning_graph_ids,
        MAX_BYTES,
        deadline,
    )?;
    let mut input_bytes = meaning
        .as_ref()
        .map(crate::meaning_view::charged_input_bytes)
        .transpose()?
        .unwrap_or(0);
    ensure!(
        input_bytes <= MAX_BYTES,
        "Meaning view input capacity exceeded"
    );
    let sources = sources(store, snapshot, deadline)?;
    let mut view = canonical_view(snapshot, graph, &sources, deadline)?;
    view.meaning = meaning;
    limits(&view)?;
    let mut selected = options.structural_overlay_ids.clone();
    let mut prepared = BTreeMap::new();
    for kind in &options.structural_kinds {
        let value = match kind {
            StructuralKind::Manifest => {
                crate::manifest_graph::prepare(store, snapshot, graph, deadline)?
            }
            StructuralKind::Config => {
                crate::config_graph::prepare(store, snapshot, graph, deadline)?
            }
            StructuralKind::Markdown => {
                crate::markdown_graph::prepare(store, snapshot, graph, deadline)?
            }
        };
        let id = value.artifact_id().to_owned();
        selected.push(id.clone());
        ensure!(
            prepared.insert(id, value).is_none(),
            "Duplicate prepared structural artifact"
        );
    }
    let mut seen = BTreeSet::new();
    selected.sort();
    for id in selected {
        ensure!(
            seen.insert(id.clone()),
            "Duplicate selected overlay artifact"
        );
        let raw = if let Some(output) = prepared.get(&id) {
            charge_input_bytes(output.byte_len(), &mut input_bytes)?;
            let raw = crate::json::parse(output.bytes(), MAX_BYTES)?;
            let _: crate::manifest_graph::DerivedGraph = crate::json::decode(raw.clone())?;
            ensure!(
                raw["artifactId"] == ""
                    && raw["snapshotId"] == snapshot.id
                    && raw["workspaceId"] == snapshot.workspace_id
                    && raw["graphHash"] == view.base_graph_hash
                    && matches!(
                        raw["producer"].as_str(),
                        Some(
                            "yoke-manifest-graph-1"
                                | "yoke-config-graph-1"
                                | "yoke-markdown-graph-2"
                        )
                    ),
                "Prepared structural overlay source scope mismatch"
            );
            raw
        } else {
            charge_input(store, &id, &mut input_bytes, deadline)?;
            source_overlay(store, snapshot, graph, &id, deadline)?
        };
        overlay(&mut view, snapshot, &sources, &raw, &id, false, deadline)?;
    }
    let mut semantic = options.semantic_overlay_ids.clone();
    semantic.sort();
    for id in semantic {
        ensure!(
            valid_id(&id) && seen.insert(id.clone()),
            "Invalid or duplicate semantic overlay ID"
        );
        let record = crate::semantic_enrichment::record_for_overlay(store, &id, deadline)?;
        let artifact_id = record["artifactId"]
            .as_str()
            .context("Semantic artifact ID absent")?;
        charge_input(store, artifact_id, &mut input_bytes, deadline)?;
        let raw = crate::semantic_enrichment::load(store, &id, deadline)?;
        ensure!(
            raw["snapshotId"] == snapshot.id && raw["baseGraphHash"] == identity(graph),
            "Semantic overlay source scope mismatch"
        );
        overlay(
            &mut view,
            snapshot,
            &sources,
            &raw,
            artifact_id,
            true,
            deadline,
        )?;
    }
    validate_contents(&view, snapshot, &sources, deadline)?;
    view.id = identity(&view);
    let bytes = serde_json::to_vec(&view)?;
    ensure!(
        bytes.len() <= MAX_BYTES,
        "Graph view serialized capacity exceeded"
    );
    view.artifact_id = hash(&bytes);
    let record = Record {
        view_id: view.id.clone(),
        artifact_id: view.artifact_id.clone(),
        snapshot_id: view.snapshot_id.clone(),
        workspace_id: view.workspace_id.clone(),
        base_graph_hash: view.base_graph_hash.clone(),
        overlay_ids: view
            .overlays
            .iter()
            .map(|value| value["artifactId"].as_str().unwrap().to_owned())
            .collect(),
        meaning_graph_ids: view
            .meaning
            .as_ref()
            .map(|layer| {
                layer
                    .selections
                    .iter()
                    .map(|selection| selection.graph_id.clone())
                    .collect()
            })
            .unwrap_or_default(),
    };
    let value = serde_json::to_value(&record)?;
    let encoded = serde_json::to_string(&value)?;
    deadline.check()?;
    let tx =
        rusqlite::Transaction::new_unchecked(&store.db, rusqlite::TransactionBehavior::Immediate)?;
    // Every predictable registration conflict is admission, including a later
    // token. Sweep the entire batch before the first physical CAS write.
    for output in prepared.values() {
        output.preflight(&tx, deadline)?;
    }
    preflight_view_registration(store, &tx, &view.id, &value, deadline)?;
    for output in prepared.into_values() {
        output.publish_into(&tx, deadline)?;
    }
    ensure!(
        store.artifact(&bytes)? == view.artifact_id,
        "Graph view artifact identity mismatch"
    );
    store.db.execute(
        "INSERT INTO records(kind,id,value) VALUES(?1,?2,?3) ON CONFLICT(kind,id) DO NOTHING",
        rusqlite::params![RECORD_KIND, view.id, encoded],
    )?;
    ensure!(
        record_value(store, RECORD_KIND, &view.id, deadline)? == value,
        "Stored graph view record differs"
    );
    deadline.check()?;
    tx.commit()?;
    Ok(view)
}
fn validate_contents(
    view: &View,
    snapshot: &Snapshot,
    sources: &Sources,
    deadline: &Deadline,
) -> Result<()> {
    limits(view)?;
    ensure!(
        !view.complete
            && view.snapshot_id == snapshot.id
            && view.workspace_id == snapshot.workspace_id,
        "View source scope mismatch"
    );
    let mut nodes = BTreeSet::new();
    for node in &view.nodes {
        deadline.check()?;
        ensure!(
            node["snapshotId"] == snapshot.id
                && node["name"]
                    .as_str()
                    .is_some_and(|s| !s.is_empty() && s.len() <= 4096)
                && node["kind"]
                    .as_str()
                    .is_some_and(|s| !s.is_empty() && s.len() <= 100)
                && node["id"].as_str().is_some_and(valid_id)
                && nodes.insert(node["id"].as_str().unwrap()),
            "Invalid or duplicate view node identity"
        );
        let bindings = node["sourceBindings"]
            .as_array()
            .context("View node source bindings missing")?;
        ensure!(
            !bindings.is_empty() && bindings.len() <= 100,
            "View node anchor count invalid"
        );
        for value in bindings {
            ensure!(
                value["path"] == node["path"],
                "View node binding path mismatch"
            );
            normalize_anchor(snapshot, sources, value)?;
        }
    }
    let mut evidence = BTreeSet::new();
    for anchor in &view.evidence {
        deadline.check()?;
        ensure!(
            anchor["id"].as_str().is_some_and(valid_id)
                && evidence.insert(anchor["id"].as_str().unwrap()),
            "Invalid or duplicate view evidence identity"
        );
        normalize_anchor(snapshot, sources, anchor)?;
    }
    let mut arcs = BTreeSet::new();
    for arc in &view.observations {
        deadline.check()?;
        ensure!(
            arc["id"].as_str().is_some_and(valid_id) && arcs.insert(arc["id"].as_str().unwrap()),
            "Invalid or duplicate view observation identity"
        );
        ensure!(
            arc["source"].as_str().is_some_and(|id| nodes.contains(id))
                && (arc["target"].is_null()
                    || arc["target"].as_str().is_some_and(|id| nodes.contains(id)))
                && arc["relation"]
                    .as_str()
                    .is_some_and(|s| !s.is_empty() && s.len() <= 100)
                && arc["resolution"]
                    .as_str()
                    .is_some_and(|s| !s.is_empty() && s.len() <= 100),
            "View observation endpoint absent"
        );
        let anchors = arc["evidenceIds"]
            .as_array()
            .context("View observation evidence missing")?;
        ensure!(
            !anchors.is_empty()
                && anchors
                    .iter()
                    .all(|id| id.as_str().is_some_and(|id| evidence.contains(id))),
            "View observation evidence absent"
        );
    }
    Ok(())
}
pub fn load(store: &Store, id: &str, deadline: &Deadline) -> Result<View> {
    let record: Record = crate::json::decode(record_value(store, RECORD_KIND, id, deadline)?)?;
    ensure!(
        record.view_id == id
            && record.workspace_id == store.workspace_id
            && valid_id(&record.snapshot_id)
            && valid_id(&record.base_graph_hash),
        "Invalid graph view record scope"
    );
    let snapshot = store.snapshot(&record.snapshot_id)?;
    let graph = store.graph(&record.snapshot_id)?;
    crate::graph_extensions::validate_scope(store, &snapshot, &graph, deadline)?;
    ensure!(
        identity(&graph) == record.base_graph_hash,
        "Graph view base graph mismatch"
    );
    let mut view: View = crate::json::decode(crate::json::parse(
        &artifact(store, &record.artifact_id, deadline)?,
        MAX_BYTES,
    )?)?;
    ensure!(
        view.id == id
            && view.artifact_id.is_empty()
            && view.base_graph_hash == record.base_graph_hash
            && view
                .overlays
                .iter()
                .map(|value| value["artifactId"].as_str().unwrap_or("").to_owned())
                .collect::<Vec<_>>()
                == record.overlay_ids,
        "Graph view artifact record mismatch"
    );
    let claimed = view.id.clone();
    view.id.clear();
    ensure!(
        identity(&view) == claimed,
        "Graph view content identity mismatch"
    );
    view.id = claimed;
    safe_file_presentation(&view)?;
    let sources = sources(store, &snapshot, deadline)?;
    validate_contents(&view, &snapshot, &sources, deadline)?;
    let mut expected = canonical_view(&snapshot, &graph, &sources, deadline)?;
    let mut selected = BTreeSet::new();
    let mut input_bytes = 0;
    for selection in &view.overlays {
        deadline.check()?;
        let artifact_id = selection["artifactId"]
            .as_str()
            .context("View selected artifact missing")?;
        ensure!(
            selected.insert(artifact_id),
            "Duplicate view selected artifact"
        );
        charge_input(store, artifact_id, &mut input_bytes, deadline)?;
        let (raw, semantic) = match selection["origin"].as_str() {
            Some("structural_overlay") => (
                source_overlay(store, &snapshot, &graph, artifact_id, deadline)?,
                false,
            ),
            Some("semantic_overlay") => {
                let overlay_id = selection["overlayId"]
                    .as_str()
                    .context("View semantic overlay identity missing")?;
                let record =
                    crate::semantic_enrichment::record_for_overlay(store, overlay_id, deadline)?;
                ensure!(
                    record["artifactId"] == artifact_id,
                    "View semantic artifact identity mismatch"
                );
                let raw = crate::semantic_enrichment::load(store, overlay_id, deadline)?;
                ensure!(
                    raw["snapshotId"] == snapshot.id && raw["baseGraphHash"] == identity(&graph),
                    "View semantic source scope mismatch"
                );
                (raw, true)
            }
            _ => anyhow::bail!("Unsupported view overlay origin"),
        };
        overlay(
            &mut expected,
            &snapshot,
            &sources,
            &raw,
            artifact_id,
            semantic,
            deadline,
        )?;
    }
    expected.meaning = crate::meaning_view::load_selected(
        store,
        &snapshot,
        &graph,
        &record.meaning_graph_ids,
        MAX_BYTES
            .checked_sub(input_bytes)
            .context("View selected input capacity exceeded")?,
        deadline,
    )?;
    ensure!(
        expected.meaning == view.meaning
            && expected.nodes == view.nodes
            && expected.observations == view.observations
            && expected.evidence == view.evidence
            && expected.overlays == view.overlays
            && expected.limitations == view.limitations,
        "View claims differ from canonical graph or selected published overlay origins"
    );
    view.artifact_id = record.artifact_id;
    Ok(view)
}
fn safe_file_presentation(view: &View) -> Result<()> {
    for node in &view.nodes {
        if node["origin"] == "canonical" && node["kind"] == "file" {
            let path = node["path"]
                .as_str()
                .context("Canonical file presentation path missing")?;
            let language = node["language"]
                .as_str()
                .context("Canonical file presentation language missing")?;
            let signature = node["signature"]
                .as_str()
                .context("Canonical file presentation signature missing")?;
            ensure!(
                signature == format!("{path} [{language}]"),
                "Legacy presentation repair required: build a new derived view from the original snapshot; original artifact bytes remain available"
            );
        }
    }
    Ok(())
}
/// Retention accepts only records whose registered artifacts and source claims
/// pass the same immutable validation as query views. It does not publish views.
pub(crate) fn validate_retention_record(
    store: &Store,
    kind: &str,
    id: &str,
    value: &Value,
    deadline: &Deadline,
) -> Result<Option<Vec<String>>> {
    if ![
        RECORD_KIND,
        crate::manifest_graph::RECORD_KIND,
        crate::semantic_enrichment::RECORD_KIND,
        crate::semantic_enrichment::RECORD_KIND_V2,
        crate::graph_exports::VIEW_EXPORT_KIND,
    ]
    .contains(&kind)
    {
        return Ok(None);
    }
    ensure!(
        record_value(store, kind, id, deadline)? == *value,
        "Graph retention record changed"
    );
    if kind == crate::graph_exports::VIEW_EXPORT_KIND {
        fields(
            value,
            &[
                "snapshotId",
                "workspaceId",
                "graphHash",
                "artifactId",
                "format",
                "options",
                "graphViewId",
                "baseGraphHash",
            ],
        )?;
        let view_id = value["graphViewId"]
            .as_str()
            .context("View export source identity missing")?;
        let view = load(store, view_id, deadline)?;
        let options: crate::graph_exports::ExportOptions =
            crate::json::decode(value["options"].clone())?;
        ensure!(
            value["snapshotId"] == view.snapshot_id
                && value["workspaceId"] == view.workspace_id
                && value["baseGraphHash"] == view.base_graph_hash
                && value["graphHash"] == identity(&projection(&view)?)
                && value["artifactId"] == id
                && value["format"] == serde_json::to_value(options.format)?
                && (1..=5000).contains(&options.max_nodes)
                && (1..=50000).contains(&options.max_edges),
            "View export scope/options mismatch"
        );
        let bytes = artifact_limit(store, id, 64_000_000, deadline)?;
        let mut bundle: crate::graph_exports::ExportBundle =
            crate::json::decode(crate::json::parse(&bytes, 64_000_000)?)?;
        ensure!(
            bundle.artifact_id.is_empty()
                && bundle.snapshot_id == view.snapshot_id
                && bundle.workspace_id == view.workspace_id
                && bundle.graph_hash == value["graphHash"]
                && serde_json::to_value(bundle.format)? == value["format"],
            "View export artifact scope mismatch"
        );
        bundle.artifact_id = id.into();
        crate::graph_exports::validate_bundle(&bundle, deadline)?;
        return Ok(Some(vec![id.into()]));
    }
    if kind == RECORD_KIND {
        let view = load(store, id, deadline)?;
        return Ok(Some(vec![view.artifact_id]));
    }
    let snapshot_id = value["snapshotId"]
        .as_str()
        .context("Graph retention snapshot missing")?;
    let snapshot = store.snapshot(snapshot_id)?;
    let graph = store.graph(snapshot_id)?;
    crate::graph_extensions::validate_scope(store, &snapshot, &graph, deadline)?;
    let semantic = crate::semantic_enrichment::RECORD_KINDS.contains(&kind);
    let raw = if semantic {
        crate::semantic_enrichment::load(store, id, deadline)?
    } else {
        source_overlay(store, &snapshot, &graph, id, deadline)?
    };
    let artifact_id = value["artifactId"]
        .as_str()
        .context("Graph retention artifact missing")?;
    artifact(store, artifact_id, deadline)?;
    let sources = sources(store, &snapshot, deadline)?;
    let mut view = View {
        id: String::new(),
        artifact_id: String::new(),
        snapshot_id: snapshot.id.clone(),
        workspace_id: snapshot.workspace_id.clone(),
        base_graph_hash: identity(&graph),
        overlays: vec![],
        nodes: vec![],
        observations: vec![],
        evidence: vec![],
        limitations: vec![],
        complete: false,
        meaning: None,
    };
    for node in &graph.entities {
        let mut value = serde_json::to_value(node)?;
        value["origin"] = json!("canonical");
        value["sourceBindings"] = json!([binding(
            &snapshot,
            &sources,
            &node.path,
            node.start_byte,
            node.end_byte,
            "source_range"
        )?]);
        view.nodes.push(value);
    }
    overlay(
        &mut view,
        &snapshot,
        &sources,
        &raw,
        artifact_id,
        semantic,
        deadline,
    )?;
    validate_contents(&view, &snapshot, &sources, deadline)?;
    Ok(Some(vec![artifact_id.to_owned()]))
}
fn checked(store: &Store, view: &View, deadline: &Deadline) -> Result<()> {
    let loaded = load(store, &view.id, deadline)?;
    ensure!(
        identity(&loaded) == identity(view),
        "Graph view differs from immutable published artifact"
    );
    Ok(())
}
/// Typed retention roots are returned only after the complete published View is replayed.
pub(crate) fn meaning_dependencies(
    store: &Store,
    view: &View,
    deadline: &Deadline,
) -> Result<Option<crate::meaning_build::MeaningDependencies>> {
    checked(store, view, deadline)?;
    let Some(layer) = &view.meaning else {
        return Ok(None);
    };
    let snapshot = store.snapshot(&view.snapshot_id)?;
    let graph = store.graph(&view.snapshot_id)?;
    let mut bytes = 0;
    for selected in &view.overlays {
        charge_input(
            store,
            selected["artifactId"]
                .as_str()
                .context("View overlay artifact missing")?,
            &mut bytes,
            deadline,
        )?;
    }
    Ok(Some(crate::meaning_view::validate_selected(
        store,
        &snapshot,
        &graph,
        layer,
        MAX_BYTES
            .checked_sub(bytes)
            .context("View input capacity exceeded")?,
        deadline,
    )?))
}
fn fields(args: &Value, allowed: &[&str]) -> Result<()> {
    let args = args.as_object().context("View query must be an object")?;
    ensure!(
        args.keys().all(|key| allowed.contains(&key.as_str())),
        "Unknown graph view query option"
    );
    Ok(())
}
fn maximum(args: &Value, key: &str, default: usize, min: usize, max: usize) -> Result<usize> {
    let value = args
        .get(key)
        .map(|v| v.as_u64().context("Invalid view query capacity"))
        .transpose()?
        .unwrap_or(default as u64);
    let value = usize::try_from(value).context("View query capacity overflow")?;
    ensure!((min..=max).contains(&value), "View query capacity exceeded");
    Ok(value)
}
fn meaning_items(view: &View, deadline: &Deadline) -> Result<Vec<Value>> {
    let Some(layer) = &view.meaning else {
        return Ok(vec![]);
    };
    let mut rows = Vec::new();
    for packet in &layer.packets {
        deadline.check()?;
        let selection = layer
            .selections
            .get(packet.selection_index)
            .context("Meaning packet selection missing")?;
        let mut present = |mut item: Value, kind: &str, name: &str| -> Result<()> {
            deadline.check()?;
            item["kind"] = json!(kind);
            item["name"] = json!(name);
            item["origin"] = json!("meaning_graph");
            item["meaningGraphId"] = json!(selection.graph_id);
            item["packetIndex"] = json!(packet.index);
            item["provenance"] = serde_json::to_value(&packet.provenance)?;
            rows.push(item);
            Ok(())
        };
        for item in &packet.concepts {
            present(serde_json::to_value(item)?, "meaning_concept", &item.label)?;
        }
        for item in &packet.relations {
            present(
                serde_json::to_value(item)?,
                "meaning_relation",
                &item.relation,
            )?;
        }
        for item in &packet.hyperedges {
            present(
                serde_json::to_value(item)?,
                "meaning_hyperedge",
                &item.relation,
            )?;
        }
    }
    Ok(rows)
}
fn meaning_citation_index(
    view: &View,
) -> Result<BTreeMap<&str, Vec<&crate::meaning_view::MeaningEvidence>>> {
    let mut result = BTreeMap::new();
    let Some(layer) = &view.meaning else {
        return Ok(result);
    };
    for packet in &layer.packets {
        let evidence: BTreeMap<_, _> = packet.evidence.iter().map(|e| (e.id.as_str(), e)).collect();
        let claims = packet
            .concepts
            .iter()
            .map(|c| (c.id.as_str(), c.evidence_ids.as_slice()))
            .chain(
                packet
                    .relations
                    .iter()
                    .map(|c| (c.id.as_str(), c.evidence_ids.as_slice())),
            )
            .chain(
                packet
                    .hyperedges
                    .iter()
                    .map(|c| (c.id.as_str(), c.evidence_ids.as_slice())),
            );
        for (id, ids) in claims {
            let quotes = ids
                .iter()
                .map(|id| {
                    evidence
                        .get(id.as_str())
                        .copied()
                        .context("Meaning citation missing")
                })
                .collect::<Result<Vec<_>>>()?;
            ensure!(
                result.insert(id, quotes).is_none(),
                "Duplicate meaningful query identity"
            );
        }
        for item in &packet.evidence {
            ensure!(
                result.insert(item.id.as_str(), vec![item]).is_none(),
                "Duplicate meaning evidence identity"
            );
        }
    }
    Ok(result)
}
/// Meaning reads return inferred claim data and its own exact citations. A
/// supporting source range is never presented as a declaration of the claim.
pub fn read_meaning(
    store: &Store,
    view: &View,
    args: &Value,
    deadline: &Deadline,
) -> Result<Option<Value>> {
    let Some(id) = args["entityId"].as_str() else {
        return Ok(None);
    };
    if view.meaning.is_none() || view.nodes.iter().any(|node| node["id"] == id) {
        return Ok(None);
    }
    checked(store, view, deadline)?;
    let citations = meaning_citation_index(view)?;
    let Some(quotes) = citations.get(id) else {
        return Ok(None);
    };
    fields(args, &["entityId"])?;
    let item = meaning_items(view, deadline)?
        .into_iter()
        .find(|item| item["id"] == id)
        .map(Ok)
        .unwrap_or_else(|| {
            let mut item = serde_json::to_value(quotes[0])?;
            item["kind"] = json!("meaning_evidence");
            item["origin"] = json!("meaning_graph");
            Ok::<_, anyhow::Error>(item)
        })?;
    Ok(Some(
        json!({"graphViewId":view.id,"snapshotId":view.snapshot_id,"entityId":id,"item":item,"citations":quotes,"complete":false,"representation":"inferred_claim_with_exact_source_citations"}),
    ))
}
fn find_inner(view: &View, args: &Value, deadline: &Deadline) -> Result<Value> {
    fields(args, &["query", "kind", "path", "limit"])?;
    let query = args["query"].as_str().context("View query text required")?;
    ensure!(
        !query.is_empty() && query.len() <= 1000,
        "Invalid view query text"
    );
    for key in ["kind", "path"] {
        ensure!(
            args.get(key)
                .is_none_or(|value| value.as_str().is_some_and(|s| s.len() <= 4096)),
            "Invalid view query filter"
        );
    }
    let limit = maximum(args, "limit", 30, 1, 2000)?;
    let needle = query.to_lowercase();
    let mut found = Vec::new();
    for node in &view.nodes {
        deadline.check()?;
        if args.get("kind").is_some_and(|kind| kind != &node["kind"])
            || args["path"]
                .as_str()
                .is_some_and(|path| node["path"] != path)
        {
            continue;
        }
        if node["name"]
            .as_str()
            .unwrap_or("")
            .to_lowercase()
            .contains(&needle)
            || node["qualifiedName"]
                .as_str()
                .unwrap_or("")
                .to_lowercase()
                .contains(&needle)
        {
            found.push(node.clone());
        }
    }
    let citations = meaning_citation_index(view)?;
    for mut item in meaning_items(view, deadline)? {
        deadline.check()?;
        if args.get("kind").is_some_and(|kind| kind != &item["kind"])
            || args["path"].as_str().is_some_and(|path| {
                !citations
                    .get(item["id"].as_str().unwrap())
                    .is_some_and(|quotes| quotes.iter().any(|q| q.path == path))
            })
        {
            continue;
        }
        let alias = item["aliases"].as_array().and_then(|aliases| {
            aliases.iter().find(|alias| {
                alias
                    .as_str()
                    .is_some_and(|a| a.to_lowercase().contains(&needle))
            })
        });
        if ["name", "description"].iter().any(|field| {
            item[*field]
                .as_str()
                .is_some_and(|s| s.to_lowercase().contains(&needle))
        }) || alias.is_some()
        {
            if let Some(alias) = alias.cloned() {
                item["aliasMatch"] =
                    json!({"label":alias,"interpretation":"search_candidate_not_identity"});
            }
            found.push(item);
        }
    }
    found.sort_by_key(|node| {
        (
            node["name"].as_str() != Some(query),
            node["path"].as_str().unwrap_or("").to_owned(),
            node["origin"].as_str().unwrap_or("").to_owned(),
            node["id"].as_str().unwrap().to_owned(),
        )
    });
    let count = found.len();
    found.truncate(limit);
    let retrieval = if view.meaning.is_some() {
        "source_names_or_meaning_labels_descriptions_alias_candidates"
    } else {
        "case_insensitive_name_substring_with_exact_name_preference"
    };
    Ok(
        json!({"graphViewId":view.id,"snapshotId":view.snapshot_id,"items":found,"itemsOmitted":count.saturating_sub(limit),"complete":false,"retrieval":retrieval}),
    )
}
pub fn find(store: &Store, view: &View, args: &Value, deadline: &Deadline) -> Result<Value> {
    checked(store, view, deadline)?;
    find_inner(view, args, deadline)
}
fn projection(view: &View) -> Result<Graph> {
    ensure!(
        view.meaning.is_none(),
        "Meaning claims require typed query traversal; source-only Graph projection is unavailable"
    );
    project_source_rows(view)
}
fn project_source_rows(view: &View) -> Result<Graph> {
    let mut entities = Vec::new();
    for node in &view.nodes {
        let b = &node["sourceBindings"][0];
        entities.push(crate::model::Entity {
            id: node["id"].as_str().unwrap().into(),
            key: node["key"]
                .as_str()
                .unwrap_or(node["id"].as_str().unwrap())
                .into(),
            snapshot_id: view.snapshot_id.clone(),
            path: node["path"].as_str().unwrap().into(),
            name: node["name"].as_str().unwrap().into(),
            qualified_name: node["qualifiedName"]
                .as_str()
                .unwrap_or(node["name"].as_str().unwrap())
                .into(),
            kind: node["kind"].as_str().unwrap_or("unknown").into(),
            language: node["language"].as_str().unwrap_or("derived").into(),
            signature: node["signature"].as_str().unwrap_or("").into(),
            start_byte: number(b, "startByte")?,
            end_byte: number(b, "endByte")?,
            start_line: number(b, "startLine")?,
            end_line: number(b, "endLine")?,
            name_start_byte: None,
            name_end_byte: None,
            parent_key: node["parentKey"].as_str().map(str::to_owned),
            producer: node["producer"].as_str().unwrap_or("canonical").into(),
        });
    }
    Ok(Graph {
        snapshot_id: view.snapshot_id.clone(),
        entities,
        observations: serde_json::from_value(json!(view.observations))?,
        evidence: serde_json::from_value(json!(view.evidence))?,
        ..Default::default()
    })
}
#[derive(Clone, Copy)]
enum MixedArcKind<'a> {
    Source(usize),
    Relation(&'a str),
    Hyperedge(&'a str),
    Citation(&'a str),
}
struct MixedArc<'a> {
    source: &'a str,
    target: Option<&'a str>,
    relation: &'a str,
    resolution: &'a str,
    neutral: bool,
    kind: MixedArcKind<'a>,
}
fn trace_meaning_inner(view: &View, args: &Value, deadline: &Deadline) -> Result<Value> {
    let layer = view.meaning.as_ref().context("Meaning layer missing")?;
    let mut rows = view.nodes.clone();
    rows.extend(meaning_items(view, deadline)?);
    let known: BTreeSet<_> = rows.iter().filter_map(|row| row["id"].as_str()).collect();
    let mut visited: BTreeSet<String> = args["entityIds"]
        .as_array()
        .unwrap()
        .iter()
        .map(|id| id.as_str().unwrap().into())
        .collect();
    ensure!(
        visited.iter().all(|id| known.contains(id.as_str())),
        "Unknown typed view trace entity"
    );
    let depth = maximum(args, "depth", 2, 0, 8)?;
    let maximum = maximum(args, "maxVisited", 300, 1, 20000)?;
    let direction = args["direction"].as_str().unwrap_or("out");
    let mut arcs = Vec::new();
    for (index, arc) in view.observations.iter().enumerate() {
        deadline.check()?;
        arcs.push(MixedArc {
            source: arc["source"].as_str().context("View arc source missing")?,
            target: arc["target"].as_str(),
            relation: arc["relation"].as_str().unwrap(),
            resolution: arc["resolution"].as_str().unwrap(),
            neutral: false,
            kind: MixedArcKind::Source(index),
        });
    }
    for packet in &layer.packets {
        for arc in &packet.relations {
            deadline.check()?;
            arcs.push(MixedArc {
                source: &arc.source,
                target: Some(&arc.target),
                relation: &arc.relation,
                resolution: "inferred_concept",
                neutral: false,
                kind: MixedArcKind::Relation(&arc.id),
            });
        }
        for hyperedge in &packet.hyperedges {
            for participant in &hyperedge.participants {
                deadline.check()?;
                // An incidence vertex costs a hop. Roles/order are data, never an inferred arrow.
                arcs.push(MixedArc {
                    source: &participant.concept,
                    target: Some(&hyperedge.id),
                    relation: &hyperedge.relation,
                    resolution: "inferred_concept",
                    neutral: true,
                    kind: MixedArcKind::Hyperedge(&hyperedge.id),
                });
            }
        }
        for citation in &packet.citation_links {
            deadline.check()?;
            if citation.claim_kind == crate::meaning_view::ClaimKind::Relation {
                continue;
            }
            arcs.push(MixedArc {
                source: &citation.claim,
                target: Some(&citation.file_entity_id),
                relation: "cites_source",
                resolution: "inferred_concept",
                neutral: true,
                kind: MixedArcKind::Citation(&citation.id),
            });
        }
    }
    let mut adjacency: BTreeMap<&str, Vec<usize>> = BTreeMap::new();
    for (index, arc) in arcs.iter().enumerate() {
        deadline.check()?;
        if (arc.neutral && direction != "both")
            || args["relations"]
                .as_array()
                .is_some_and(|a| !a.iter().any(|v| v == arc.relation))
            || args["resolutions"]
                .as_array()
                .is_some_and(|a| !a.iter().any(|v| v == arc.resolution))
        {
            continue;
        }
        if direction != "in" {
            adjacency.entry(arc.source).or_default().push(index);
        }
        if direction != "out"
            && let Some(target) = arc.target
        {
            adjacency.entry(target).or_default().push(index);
        }
    }
    let mut queue: VecDeque<_> = visited.iter().cloned().map(|id| (id, 0usize)).collect();
    let mut source_arcs = BTreeSet::new();
    let mut relations = BTreeSet::new();
    let mut hyperedges = BTreeSet::new();
    let mut citations = BTreeSet::new();
    let mut truncated = false;
    while let Some((id, distance)) = queue.pop_front() {
        deadline.check()?;
        if distance >= depth {
            continue;
        }
        for index in adjacency.get(id.as_str()).into_iter().flatten() {
            deadline.check()?;
            let arc = &arcs[*index];
            let other = if arc.source == id {
                arc.target
            } else {
                Some(arc.source)
            };
            if let Some(other) = other
                && !visited.contains(other)
            {
                if visited.len() >= maximum {
                    truncated = true;
                    continue;
                }
                visited.insert(other.into());
                queue.push_back((other.into(), distance + 1));
            }
            match arc.kind {
                MixedArcKind::Source(index) => {
                    source_arcs.insert(index);
                }
                MixedArcKind::Relation(id) => {
                    relations.insert(id);
                }
                MixedArcKind::Hyperedge(id) => {
                    hyperedges.insert(id);
                }
                MixedArcKind::Citation(id) => {
                    citations.insert(id);
                }
            }
        }
    }
    let entities: Vec<_> = rows
        .iter()
        .filter(|row| visited.contains(row["id"].as_str().unwrap()))
        .collect();
    let observations: Vec<_> = view
        .observations
        .iter()
        .enumerate()
        .filter(|(i, _)| source_arcs.contains(i))
        .map(|(_, arc)| arc)
        .collect();
    let evidence_ids: BTreeSet<_> = observations
        .iter()
        .flat_map(|o| o["evidenceIds"].as_array().unwrap())
        .filter_map(Value::as_str)
        .collect();
    let evidence: Vec<_> = view
        .evidence
        .iter()
        .filter(|e| evidence_ids.contains(e["id"].as_str().unwrap()))
        .collect();
    let mut selected_concepts = Vec::new();
    let mut selected_relations = Vec::new();
    let mut selected_hyperedges = Vec::new();
    let mut selected_citations = Vec::new();
    let mut selected_evidence_ids = BTreeSet::new();
    for packet in &layer.packets {
        for item in &packet.concepts {
            if visited.contains(&item.id) {
                selected_concepts.push(item);
                selected_evidence_ids.extend(item.evidence_ids.iter().map(String::as_str));
            }
        }
        for item in &packet.relations {
            if relations.contains(item.id.as_str()) || visited.contains(&item.id) {
                selected_relations.push(item);
                selected_evidence_ids.extend(item.evidence_ids.iter().map(String::as_str));
            }
        }
        for item in &packet.hyperedges {
            if hyperedges.contains(item.id.as_str()) || visited.contains(&item.id) {
                selected_hyperedges.push(item);
                selected_evidence_ids.extend(item.evidence_ids.iter().map(String::as_str));
            }
        }
        for item in &packet.citation_links {
            if citations.contains(item.id.as_str()) {
                selected_citations.push(item);
                selected_evidence_ids.insert(item.evidence_id.as_str());
            }
        }
    }
    let selected_evidence: Vec<_> = layer
        .packets
        .iter()
        .flat_map(|p| &p.evidence)
        .filter(|e| selected_evidence_ids.contains(e.id.as_str()))
        .collect();
    Ok(
        json!({"graphViewId":view.id,"snapshotId":view.snapshot_id,"entities":entities,"observations":observations,"evidence":evidence,"truncated":truncated,"complete":false,
        "meaning":{"scope":layer.scope,"selections":layer.selections,"concepts":selected_concepts,"relations":selected_relations,"hyperedges":selected_hyperedges,"citationLinks":selected_citations,"evidence":selected_evidence,"incidence":"neutral_only_for_both;concept_to_hyperedge_to_concept_costs_two_hops","providerAttestation":"unverified"}}),
    )
}
fn trace_inner(view: &View, args: &Value, deadline: &Deadline) -> Result<Value> {
    fields(
        args,
        &[
            "entityIds",
            "direction",
            "depth",
            "maxVisited",
            "relations",
            "resolutions",
        ],
    )?;
    let ids = args["entityIds"]
        .as_array()
        .context("View trace entities required")?;
    ensure!(
        !ids.is_empty()
            && ids.len() <= 100
            && ids.iter().all(|id| id.as_str().is_some_and(valid_id)),
        "Invalid view trace entity IDs"
    );
    maximum(args, "depth", 2, 0, 8)?;
    let max_visited = maximum(args, "maxVisited", 300, 1, 20000)?;
    let seed_count = ids
        .iter()
        .map(|id| id.as_str().unwrap())
        .collect::<BTreeSet<_>>()
        .len();
    ensure!(
        seed_count <= max_visited,
        "View trace initial entities exceed maxVisited"
    );
    ensure!(
        args.get("direction").is_none_or(|v| v
            .as_str()
            .is_some_and(|s| ["in", "out", "both"].contains(&s))),
        "Invalid view trace direction"
    );
    for key in ["relations", "resolutions"] {
        ensure!(
            args.get(key)
                .is_none_or(|v| v.as_array().is_some_and(|a| a.len() <= 100
                    && a.iter().all(|v| v.as_str().is_some_and(|s| s.len() <= 100)))),
            "Invalid view trace filter"
        );
    }
    if view.meaning.is_some() {
        return trace_meaning_inner(view, args, deadline);
    }
    let mut result = crate::query::trace(&projection(view)?, args, deadline)?;
    for (key, items) in [
        ("entities", &view.nodes),
        ("observations", &view.observations),
        ("evidence", &view.evidence),
    ] {
        let ids: BTreeSet<_> = result[key]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v["id"].as_str().unwrap().to_owned())
            .collect();
        result[key] = json!(
            items
                .iter()
                .filter(|v| ids.contains(v["id"].as_str().unwrap()))
                .collect::<Vec<_>>()
        );
    }
    result["graphViewId"] = json!(view.id);
    result["snapshotId"] = json!(view.snapshot_id);
    Ok(result)
}
pub fn trace(store: &Store, view: &View, args: &Value, deadline: &Deadline) -> Result<Value> {
    checked(store, view, deadline)?;
    trace_inner(view, args, deadline)
}
pub fn context(store: &Store, view: &View, args: &Value, deadline: &Deadline) -> Result<Value> {
    checked(store, view, deadline)?;
    fields(
        args,
        &["query", "kind", "path", "limit", "depth", "maxSourceBytes"],
    )?;
    let limit = maximum(args, "limit", 5, 1, 20)?;
    let depth = maximum(args, "depth", 1, 0, 8)?;
    let maximum = maximum(args, "maxSourceBytes", 16000, 100, 256000)?;
    let mut find_args = args.clone();
    find_args
        .as_object_mut()
        .unwrap()
        .retain(|key, _| ["query", "kind", "path"].contains(&key.as_str()));
    find_args["limit"] = json!(limit);
    let found = find_inner(view, &find_args, deadline)?;
    let ids: Vec<_> = found["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|node| node["id"].clone())
        .collect();
    let mut neighborhood = if ids.is_empty() {
        json!({"entities":[],"observations":[],"evidence":[],"complete":false})
    } else {
        trace_inner(
            view,
            &json!({"entityIds":ids,"depth":depth,"maxVisited":300}),
            deadline,
        )?
    };
    if let Some(evidence) = neighborhood
        .get_mut("meaning")
        .and_then(|meaning| meaning.get_mut("evidence"))
        .and_then(Value::as_array_mut)
    {
        for citation in evidence {
            citation
                .as_object_mut()
                .context("Meaning evidence must be an object")?
                .remove("text");
            citation["textOmitted"] = json!(true);
        }
    }
    let snapshot = store.snapshot(&view.snapshot_id)?;
    let mut excerpts = vec![];
    let mut omissions = vec![];
    let environment_files: BTreeSet<_> = view
        .nodes
        .iter()
        .filter(|node| node["kind"] == "environment_name")
        .filter_map(|node| node["path"].as_str())
        .collect();
    let mut used = 0;
    let mut source_truncated = false;
    let meaning_citations = meaning_citation_index(view)?;
    for node in found["items"].as_array().unwrap() {
        deadline.check()?;
        if node["origin"] == "meaning_graph" {
            let quotes = meaning_citations
                .get(
                    node["id"]
                        .as_str()
                        .context("Meaning entity identity missing")?,
                )
                .context("Meaning entity citations missing")?;
            for citation in quotes {
                deadline.check()?;
                let mut anchor = serde_json::to_value(citation)?;
                anchor.as_object_mut().unwrap().remove("text");
                if environment_files.contains(citation.path.as_str()) {
                    omissions.push(json!({"entityId":node["id"],"path":citation.path,"reason":"environment_values_omitted","citationAnchor":anchor}));
                    continue;
                }
                ensure!(
                    citation.snapshot_id == snapshot.id,
                    "Meaning citation snapshot mismatch"
                );
                let (text, content_hash) = crate::query::source(store, &snapshot, &citation.path)?;
                ensure!(
                    content_hash == citation.content_hash
                        && utf8_range(&text, citation.start_byte, citation.end_byte)?
                            == citation.text,
                    "Meaning context citation differs from captured source"
                );
                let start = citation.start_byte;
                let end = citation.end_byte;
                let mut stop = end.min(start.saturating_add(maximum.saturating_sub(used)));
                while stop > start && !text.is_char_boundary(stop) {
                    stop -= 1;
                }
                if stop == start {
                    source_truncated = true;
                    omissions.push(json!({"entityId":node["id"],"snapshotId":snapshot.id,"path":citation.path,"contentHash":content_hash,"reason":"source_budget_exhausted","citationAnchor":anchor}));
                    continue;
                }
                used += stop - start;
                source_truncated |= stop < end;
                excerpts.push(json!({"entityId":node["id"],"snapshotId":snapshot.id,"path":citation.path,"contentHash":content_hash,"startByte":start,"endByte":stop,"text":text[start..stop],"anchor":"source_range","citationAnchor":anchor,"bodyTruncated":stop<end}));
            }
            continue;
        }
        let b = &node["sourceBindings"][0];
        let (path, mut start, mut end) = (
            b["path"].as_str().unwrap(),
            number(b, "startByte")?,
            number(b, "endByte")?,
        );
        if environment_files.contains(path) {
            omissions.push(json!({"entityId":node["id"],"path":path,"reason":"environment_values_omitted","declarationAnchor":b}));
            continue;
        }
        let (text, hash) = crate::query::source(store, &snapshot, path)?;
        // Container declarations describe the captured document; their literal
        // declaration anchor remains separate from the expanded excerpt.
        if matches!(node["kind"].as_str(), Some("package" | "document")) {
            start = 0;
            end = text.len();
        }
        let mut stop = end.min(start.saturating_add(maximum.saturating_sub(used)));
        while !text.is_char_boundary(stop) && stop > start {
            stop -= 1;
        }
        if stop == start && start < end {
            source_truncated = true;
            omissions.push(json!({"entityId":node["id"],"snapshotId":snapshot.id,"path":path,"contentHash":hash,"reason":"source_budget_exhausted","declarationAnchor":b}));
            continue;
        }
        source_truncated |= stop < end;
        used += stop - start;
        excerpts.push(json!({"entityId":node["id"],"snapshotId":snapshot.id,"path":path,"contentHash":hash,"startByte":start,"endByte":stop,"text":text[start..stop],"anchor":"source_range","declarationAnchor":b,"bodyTruncated":stop<end}));
    }
    Ok(
        json!({"graphViewId":view.id,"snapshotId":view.snapshot_id,"selected":found["items"],"neighborhood":neighborhood,"sources":excerpts,"sourceOmissions":omissions,"sourceBytes":used,"sourceTruncated":source_truncated,"truncated":source_truncated||found["itemsOmitted"].as_u64().is_some_and(|count|count>0)||neighborhood["truncated"]==true,"complete":false,"limitations":view.limitations}),
    )
}
pub fn export(store: &Store, view: &View, format: &str, deadline: &Deadline) -> Result<Value> {
    checked(store, view, deadline)?;
    ensure!(
        view.meaning.is_none(),
        "Typed meaning export is unavailable through source-only formats"
    );
    if format != "json" {
        let format: crate::graph_exports::ExportFormat = crate::json::decode(json!(format))?;
        let bundle = crate::graph_exports::generate_verified(
            store,
            view,
            &crate::graph_exports::ExportOptions::new(format),
            deadline,
        )?;
        return Ok(
            json!({"graphViewId":view.id,"baseGraphHash":view.base_graph_hash,"overlays":view.overlays,"complete":false,"limitations":view.limitations,"bundle":bundle}),
        );
    }
    Ok(
        json!({"graphViewId":view.id,"artifactId":view.artifact_id,"snapshotId":view.snapshot_id,"workspaceId":view.workspace_id,"baseGraphHash":view.base_graph_hash,"overlays":view.overlays,"nodes":view.nodes,"observations":view.observations,"evidence":view.evidence,"complete":false,"limitations":view.limitations}),
    )
}
pub(crate) fn validated_projection(
    store: &Store,
    view: &View,
    deadline: &Deadline,
) -> Result<Graph> {
    checked(store, view, deadline)?;
    projection(view)
}
#[cfg(test)]
#[path = "graph_view_comparison_raw_typed_tests.rs"]
mod comparison_raw_typed_tests;
