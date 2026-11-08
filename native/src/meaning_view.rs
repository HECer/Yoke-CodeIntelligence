//! Explicit published meaning selections, separate from anchored code entities.
//! Reads and replay use frozen source and receipt proofs, without a provider.
use crate::{
    capture::Deadline,
    ledger::Usage,
    meaning_build::{self, CheckedPublishedSet, MeaningArchiveProof, MeaningDependencies},
    meaning_graph::{Binding, Participant, UnitCoverage},
    model::{Graph, Snapshot},
    store::Store,
};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

const MAX_INPUT_BYTES: usize = 32_000_000;
const MAX_SOURCE_BYTES: usize = 64_000_000;
const MAX_SELECTIONS: usize = 29;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum MeaningResolution {
    #[serde(rename = "inferred_concept")]
    InferredConcept,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProviderAttestation {
    #[serde(rename = "unverified")]
    Unverified,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClaimKind {
    Concept,
    Relation,
    Hyperedge,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MeaningScope {
    pub workspace_id: String,
    pub snapshot_id: String,
    pub base_graph_hash: String,
    pub analyzer_epoch: String,
    pub capture_policy_hash: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MeaningSelectionRef {
    pub graph_id: String,
    pub job_id: String,
    pub artifact_id: String,
    pub total_bytes: usize,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReceiptProvenance {
    pub snapshot_id: String,
    pub original_snapshot_id: String,
    pub original_job_id: String,
    pub response_artifact_id: String,
    pub attempt_id: String,
    pub request_id: String,
    pub model: String,
    pub model_version: String,
    pub currency: String,
    pub provider_reported_cost_micros: Option<u64>,
    pub provider_reported_usage: Option<Usage>,
    pub rebound: bool,
    pub binding_validation: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MeaningConcept {
    pub id: String,
    /// Original materialized claim identity, never replaced by the view ID.
    pub claim_id: String,
    pub label: String,
    pub description: String,
    pub aliases: Vec<String>,
    pub evidence_ids: Vec<String>,
    pub resolution: MeaningResolution,
    pub provider_attestation: ProviderAttestation,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MeaningRelation {
    pub id: String,
    pub claim_id: String,
    pub source: String,
    pub target: String,
    pub relation: String,
    pub description: String,
    pub evidence_ids: Vec<String>,
    pub resolution: MeaningResolution,
    pub provider_attestation: ProviderAttestation,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MeaningHyperedge {
    pub id: String,
    pub claim_id: String,
    pub relation: String,
    /// Order and repeated role labels are source claims, not direction.
    pub participants: Vec<Participant>,
    pub description: String,
    pub evidence_ids: Vec<String>,
    pub resolution: MeaningResolution,
    pub provider_attestation: ProviderAttestation,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MeaningEvidence {
    pub id: String,
    pub base_evidence_id: String,
    pub source_id: String,
    pub snapshot_id: String,
    pub path: String,
    pub content_hash: String,
    pub start_byte: usize,
    pub end_byte: usize,
    pub start_line: usize,
    pub end_line: usize,
    pub text: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CitationLink {
    pub id: String,
    /// Namespaced concept/relation/hyperedge ID; relation citations are metadata.
    pub claim: String,
    pub claim_kind: ClaimKind,
    pub evidence_id: String,
    /// The actual canonical file entity at the cited path, never a guessed symbol.
    pub file_entity_id: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MeaningPacket {
    pub selection_index: usize,
    pub index: usize,
    pub input_hash: String,
    pub unit_set_hash: String,
    pub contract_hash: String,
    pub source_bindings: Vec<Binding>,
    pub provenance: ReceiptProvenance,
    pub concepts: Vec<MeaningConcept>,
    pub relations: Vec<MeaningRelation>,
    pub hyperedges: Vec<MeaningHyperedge>,
    pub evidence: Vec<MeaningEvidence>,
    pub citation_links: Vec<CitationLink>,
    pub coverage: Vec<UnitCoverage>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MeaningLayer {
    pub schema_version: u8,
    pub scope: MeaningScope,
    pub selections: Vec<MeaningSelectionRef>,
    pub packets: Vec<MeaningPacket>,
    /// Distinct immutable bytes plus the bounded capacity of mutable ledger rows.
    pub input_bytes: usize,
}

/// Portable proof metadata. Consumers must also retain all `source_scopes()`
/// files in their immutable archive source store. No installed provider is used.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MeaningArchive {
    schema_version: u8,
    proof: MeaningArchiveProof,
}
impl MeaningArchive {
    pub fn source_scopes(&self) -> impl Iterator<Item = &Snapshot> {
        self.proof.source_scopes()
    }
}

pub fn load_selected(
    store: &Store,
    snapshot: &Snapshot,
    base: &Graph,
    ids: &[String],
    remaining_input_bytes: usize,
    deadline: &Deadline,
) -> Result<Option<MeaningLayer>> {
    if ids.is_empty() {
        return Ok(None);
    }
    let checked = meaning_build::checked_published_set(
        store,
        snapshot,
        base,
        &selection_ids(ids)?,
        remaining_input_bytes.min(MAX_INPUT_BYTES),
        deadline,
    )?;
    Ok(Some(from_checked(snapshot, base, &checked, deadline)?))
}
pub fn charged_input_bytes(layer: &MeaningLayer) -> Result<usize> {
    ensure!(
        layer.schema_version == 1 && layer.input_bytes > 0 && layer.input_bytes <= MAX_INPUT_BYTES,
        "Invalid meaning layer input accounting"
    );
    ensure!(
        !layer.selections.is_empty() && layer.selections.len() <= MAX_SELECTIONS,
        "Invalid meaning layer selection count"
    );
    Ok(layer.input_bytes)
}
pub fn validate_selected(
    store: &Store,
    snapshot: &Snapshot,
    base: &Graph,
    layer: &MeaningLayer,
    remaining_input_bytes: usize,
    deadline: &Deadline,
) -> Result<MeaningDependencies> {
    let checked = check_layer(
        store,
        snapshot,
        base,
        layer,
        remaining_input_bytes,
        deadline,
    )?;
    Ok(checked.dependencies().clone())
}
pub fn archive_selected(
    store: &Store,
    snapshot: &Snapshot,
    base: &Graph,
    layer: &MeaningLayer,
    max_archive_bytes: usize,
    max_source_bytes: usize,
    deadline: &Deadline,
) -> Result<MeaningArchive> {
    archive_limits(max_archive_bytes, max_source_bytes)?;
    let checked = check_layer(store, snapshot, base, layer, MAX_INPUT_BYTES, deadline)?;
    source_capacity(
        checked.archive().source_scopes(),
        max_source_bytes,
        deadline,
    )?;
    let archive = MeaningArchive {
        schema_version: 1,
        proof: checked.archive().clone(),
    };
    crate::json::encode_bounded(&archive, max_archive_bytes, deadline)?;
    Ok(archive)
}
pub(crate) fn archive_selected_ids_with_scope_preflight(
    store: &Store,
    snapshot: &Snapshot,
    base: &Graph,
    ids: &[String],
    limits: meaning_build::ArchivePreparationLimits,
    preflight: &mut meaning_build::FrozenScopePreflight<'_>,
    deadline: &Deadline,
) -> Result<(MeaningLayer, MeaningArchive)> {
    archive_limits(limits.max_archive_bytes, limits.max_source_bytes)?;
    let checked = meaning_build::checked_published_set_for_archive(
        store,
        snapshot,
        base,
        &selection_ids(ids)?,
        limits,
        preflight,
        deadline,
    )?;
    let layer = from_checked(snapshot, base, &checked, deadline)?;
    source_capacity(
        checked.archive().source_scopes(),
        limits.max_source_bytes,
        deadline,
    )?;
    let archive = MeaningArchive {
        schema_version: 1,
        proof: checked.archive().clone(),
    };
    crate::json::encode_bounded(&archive, limits.max_archive_bytes, deadline)?;
    Ok((layer, archive))
}
pub fn replay_archive(
    snapshot: &Snapshot,
    base: &Graph,
    archive: &MeaningArchive,
    read_source: &mut meaning_build::FrozenSourceReader<'_>,
    max_archive_bytes: usize,
    max_source_bytes: usize,
    deadline: &Deadline,
) -> Result<MeaningLayer> {
    replay_archive_members(
        snapshot,
        base,
        archive,
        &mut |member, deadline| read_source(member.snapshot(), member.file(), deadline),
        max_archive_bytes,
        max_source_bytes,
        deadline,
    )
}
pub(crate) fn replay_archive_members(
    snapshot: &Snapshot,
    base: &Graph,
    archive: &MeaningArchive,
    read_source: &mut meaning_build::FrozenMemberReader<'_>,
    max_archive_bytes: usize,
    max_source_bytes: usize,
    deadline: &Deadline,
) -> Result<MeaningLayer> {
    archive_limits(max_archive_bytes, max_source_bytes)?;
    ensure!(
        archive.schema_version == 1,
        "Unsupported meaning archive schema"
    );
    crate::json::encode_bounded(archive, max_archive_bytes, deadline)?;
    source_capacity(archive.source_scopes(), max_source_bytes, deadline)?;
    let checked = meaning_build::replay_published_set_members(
        snapshot,
        base,
        &archive.proof,
        read_source,
        MAX_INPUT_BYTES,
        max_source_bytes,
        deadline,
    )?;
    from_checked(snapshot, base, &checked, deadline)
}

fn selection_ids(ids: &[String]) -> Result<Vec<String>> {
    ensure!(
        !ids.is_empty() && ids.len() <= MAX_SELECTIONS,
        "Meaning selection count exceeds capacity"
    );
    let mut sorted = BTreeSet::new();
    for id in ids {
        ensure!(
            crate::model::valid_id(id) && sorted.insert(id.clone()),
            "Invalid or duplicate meaning graph identity"
        );
    }
    Ok(sorted.into_iter().collect())
}

fn check_layer(
    store: &Store,
    snapshot: &Snapshot,
    base: &Graph,
    layer: &MeaningLayer,
    remaining: usize,
    d: &Deadline,
) -> Result<CheckedPublishedSet> {
    charged_input_bytes(layer)?;
    let ids = layer
        .selections
        .iter()
        .map(|s| s.graph_id.clone())
        .collect::<Vec<_>>();
    let checked = meaning_build::checked_published_set(
        store,
        snapshot,
        base,
        &selection_ids(&ids)?,
        remaining.min(MAX_INPUT_BYTES),
        d,
    )?;
    ensure!(
        *layer == from_checked(snapshot, base, &checked, d)?,
        "Meaning layer differs from its checked published proof"
    );
    Ok(checked)
}

fn archive_limits(archive: usize, source: usize) -> Result<()> {
    ensure!(
        (1..=MAX_INPUT_BYTES).contains(&archive) && (1..=MAX_SOURCE_BYTES).contains(&source),
        "Invalid meaning archive capacity"
    );
    Ok(())
}

fn source_capacity<'a>(
    scopes: impl Iterator<Item = &'a Snapshot>,
    maximum: usize,
    d: &Deadline,
) -> Result<()> {
    let mut hashes = BTreeMap::new();
    let mut total = 0usize;
    let mut files = 0usize;
    for snapshot in scopes {
        d.check()?;
        for file in &snapshot.files {
            files = files
                .checked_add(1)
                .context("Meaning archive source count overflow")?;
            ensure!(
                files <= 100_000,
                "Meaning archive source count exceeds capacity"
            );
            if let Some(size) = hashes.insert(&file.hash, file.size) {
                ensure!(size == file.size, "Meaning archive source size conflict");
            } else {
                total = total
                    .checked_add(file.size)
                    .context("Meaning archive source size overflow")?;
                ensure!(
                    total <= maximum,
                    "Meaning archive sources exceed byte capacity"
                );
            }
        }
    }
    Ok(())
}

fn namespaced(domain: &str, graph: &str, packet: usize, original: &str) -> String {
    crate::model::identity(&(domain, graph, packet, original))
}

fn array<'a>(v: &'a Value, key: &str) -> Result<&'a Vec<Value>> {
    v.get(key)
        .and_then(Value::as_array)
        .context("Malformed checked meaning array")
}
fn string<'a>(v: &'a Value, key: &str) -> Result<&'a str> {
    v.get(key)
        .and_then(Value::as_str)
        .context("Malformed checked meaning text")
}
fn number(v: &Value, key: &str) -> Result<usize> {
    usize::try_from(
        v.get(key)
            .and_then(Value::as_u64)
            .context("Malformed checked meaning integer")?,
    )
    .context("Meaning integer exceeds host capacity")
}
fn remap_evidence(raw: &Value, ids: &BTreeMap<String, String>) -> Result<Vec<String>> {
    array(raw, "evidenceIds")?
        .iter()
        .map(|id| {
            ids.get(id.as_str().context("Malformed meaning evidence identity")?)
                .cloned()
                .context("Meaning claim cites missing evidence")
        })
        .collect()
}

fn from_checked(
    snapshot: &Snapshot,
    base: &Graph,
    set: &CheckedPublishedSet,
    d: &Deadline,
) -> Result<MeaningLayer> {
    d.check()?;
    let first = set
        .graphs()
        .first()
        .context("Empty checked meaning selection")?;
    ensure!(
        set.graphs().len() <= MAX_SELECTIONS,
        "Meaning selection count exceeds capacity"
    );
    let scope = MeaningScope {
        workspace_id: first.workspace_id.clone(),
        snapshot_id: first.snapshot_id.clone(),
        base_graph_hash: first.base_graph_hash.clone(),
        analyzer_epoch: first.analyzer_epoch.clone(),
        capture_policy_hash: first.capture_policy_hash.clone(),
    };
    ensure!(
        scope.workspace_id == snapshot.workspace_id
            && scope.snapshot_id == snapshot.id
            && scope.base_graph_hash == crate::model::identity(base),
        "Meaning selection scope mismatch"
    );
    let mut files = BTreeMap::new();
    for entity in &base.entities {
        if entity.kind == "file" {
            ensure!(
                files
                    .insert(entity.path.as_str(), entity.id.as_str())
                    .is_none(),
                "Ambiguous canonical file entity"
            );
        }
    }
    let mut layer = MeaningLayer {
        schema_version: 1,
        scope,
        selections: Vec::new(),
        packets: Vec::new(),
        input_bytes: set.admitted_bytes(),
    };
    let mut vertices = 0usize;
    let mut edges = 0usize;
    let mut evidence_count = 0usize;
    let mut source_lines = BTreeMap::new();
    for (selection_index, graph) in set.graphs().iter().enumerate() {
        d.check()?;
        ensure!(
            graph.workspace_id == layer.scope.workspace_id
                && graph.snapshot_id == layer.scope.snapshot_id
                && graph.base_graph_hash == layer.scope.base_graph_hash
                && graph.analyzer_epoch == layer.scope.analyzer_epoch
                && graph.capture_policy_hash == layer.scope.capture_policy_hash,
            "Meaning selections use different source policies"
        );
        layer.selections.push(MeaningSelectionRef {
            graph_id: graph.graph_id.clone(),
            job_id: graph.job_id.clone(),
            artifact_id: graph.artifact_id.clone(),
            total_bytes: graph.total_bytes,
        });
        let raw_packets = array(&graph.value, "packets")?;
        ensure!(
            raw_packets.len() <= 64,
            "Meaning packet count exceeds capacity"
        );
        for raw in raw_packets {
            d.check()?;
            let index = number(raw, "index")?;
            let meaning = &raw["meaning"];
            let source_bindings: Vec<Binding> =
                crate::json::decode(meaning["sourceBindings"].clone())?;
            let mut provenance = raw["provenance"].clone();
            provenance["model"] = Value::String(graph.model.clone());
            provenance["modelVersion"] = Value::String(graph.model_version.clone());
            let provenance: ReceiptProvenance = crate::json::decode(provenance)?;
            ensure!(
                provenance.snapshot_id == snapshot.id
                    && provenance.binding_validation == "exact_captured_source_replayed",
                "Meaning packet lacks exact captured source lineage"
            );
            let mut packet = MeaningPacket {
                selection_index,
                index,
                input_hash: string(raw, "inputHash")?.into(),
                unit_set_hash: string(meaning, "unitSetHash")?.into(),
                contract_hash: string(meaning, "contractHash")?.into(),
                source_bindings,
                provenance,
                concepts: Vec::new(),
                relations: Vec::new(),
                hyperedges: Vec::new(),
                evidence: Vec::new(),
                citation_links: Vec::new(),
                coverage: crate::json::decode(meaning["coverage"].clone())?,
            };
            let mut evidence_ids = BTreeMap::new();
            for raw in array(meaning, "evidence")? {
                d.check()?;
                let original = string(raw, "id")?;
                let source_id = string(raw, "sourceId")?;
                let path = string(raw, "path")?;
                let content_hash = string(raw, "contentHash")?;
                let start_byte = number(raw, "startByte")?;
                let end_byte = number(raw, "endByte")?;
                let binding = packet
                    .source_bindings
                    .iter()
                    .find(|b| b.source_id == source_id)
                    .context("Meaning evidence cites missing source")?;
                ensure!(
                    binding.path == path
                        && binding.content_hash == content_hash
                        && start_byte >= binding.start_byte
                        && start_byte < end_byte
                        && end_byte <= binding.end_byte,
                    "Meaning evidence escaped its source binding"
                );
                let source = set.source(&snapshot.id, path)?;
                let text = source
                    .get(start_byte..end_byte)
                    .context("Meaning evidence is not an exact UTF-8 range")?;
                ensure!(
                    text == string(raw, "text")?,
                    "Meaning quotation differs from captured bytes"
                );
                let id = namespaced("meaning-view-evidence-v1", &graph.graph_id, index, original);
                ensure!(
                    evidence_ids.insert(original.into(), id.clone()).is_none(),
                    "Duplicate meaning evidence"
                );
                let lines = source_lines
                    .entry(path.to_owned())
                    .or_insert_with(|| crate::paths::line_starts(source));
                packet.evidence.push(MeaningEvidence {
                    id,
                    base_evidence_id: original.into(),
                    source_id: source_id.into(),
                    snapshot_id: snapshot.id.clone(),
                    path: path.into(),
                    content_hash: content_hash.into(),
                    start_byte,
                    end_byte,
                    start_line: lines.partition_point(|position| *position <= start_byte),
                    end_line: lines.partition_point(|position| *position < end_byte),
                    text: text.into(),
                });
            }
            let mut concept_ids: BTreeMap<String, String> = BTreeMap::new();
            for raw in array(meaning, "concepts")? {
                d.check()?;
                let original = string(raw, "id")?;
                let id = namespaced("meaning-view-concept-v1", &graph.graph_id, index, original);
                ensure!(
                    concept_ids.insert(original.into(), id.clone()).is_none(),
                    "Duplicate meaning concept"
                );
                let mut value = raw.clone();
                value["id"] = Value::String(id);
                value["claimId"] = Value::String(original.into());
                value["evidenceIds"] = serde_json::to_value(remap_evidence(raw, &evidence_ids)?)?;
                packet.concepts.push(crate::json::decode(value)?);
            }
            for raw in array(meaning, "relations")? {
                d.check()?;
                let original = string(raw, "id")?;
                let mut value = raw.clone();
                value["id"] = Value::String(namespaced(
                    "meaning-view-relation-v1",
                    &graph.graph_id,
                    index,
                    original,
                ));
                value["claimId"] = Value::String(original.into());
                value["source"] = Value::String(
                    concept_ids
                        .get(string(raw, "source")?)
                        .context("Missing meaning relation endpoint")?
                        .clone(),
                );
                value["target"] = Value::String(
                    concept_ids
                        .get(string(raw, "target")?)
                        .context("Missing meaning relation endpoint")?
                        .clone(),
                );
                value["evidenceIds"] = serde_json::to_value(remap_evidence(raw, &evidence_ids)?)?;
                packet.relations.push(crate::json::decode(value)?);
            }
            for raw in array(meaning, "hyperedges")? {
                d.check()?;
                let original = string(raw, "id")?;
                let mut value = raw.clone();
                value["id"] = Value::String(namespaced(
                    "meaning-view-hyperedge-v1",
                    &graph.graph_id,
                    index,
                    original,
                ));
                value["claimId"] = Value::String(original.into());
                let mut participants: Vec<Participant> =
                    crate::json::decode(raw["participants"].clone())?;
                for participant in &mut participants {
                    participant.concept = concept_ids
                        .get(&participant.concept)
                        .context("Missing meaning hyperedge participant")?
                        .clone();
                }
                value["participants"] = serde_json::to_value(participants)?;
                value["evidenceIds"] = serde_json::to_value(remap_evidence(raw, &evidence_ids)?)?;
                packet.hyperedges.push(crate::json::decode(value)?);
            }
            for (claim, kind, evidence) in packet
                .concepts
                .iter()
                .map(|c| (&c.id, ClaimKind::Concept, &c.evidence_ids))
                .chain(
                    packet
                        .relations
                        .iter()
                        .map(|c| (&c.id, ClaimKind::Relation, &c.evidence_ids)),
                )
                .chain(
                    packet
                        .hyperedges
                        .iter()
                        .map(|c| (&c.id, ClaimKind::Hyperedge, &c.evidence_ids)),
                )
            {
                for evidence_id in evidence {
                    d.check()?;
                    let evidence = packet
                        .evidence
                        .iter()
                        .find(|e| &e.id == evidence_id)
                        .context("Missing meaning citation evidence")?;
                    let file = *files
                        .get(evidence.path.as_str())
                        .context("Meaning citation has no canonical file entity")?;
                    packet.citation_links.push(CitationLink {
                        id: crate::model::identity(&(
                            "meaning-view-citation-v1",
                            claim,
                            kind,
                            evidence_id,
                            file,
                        )),
                        claim: claim.clone(),
                        claim_kind: kind,
                        evidence_id: evidence_id.clone(),
                        file_entity_id: file.into(),
                    });
                }
            }
            vertices += packet.concepts.len() + packet.hyperedges.len();
            edges += packet.relations.len()
                + packet.citation_links.len()
                + packet
                    .hyperedges
                    .iter()
                    .map(|h| h.participants.len())
                    .sum::<usize>();
            evidence_count += packet.evidence.len();
            ensure!(
                vertices <= 20_000 && edges <= 100_000 && evidence_count <= 100_000,
                "Meaning layer graph capacity exceeded"
            );
            layer.packets.push(packet);
        }
    }
    charged_input_bytes(&layer)?;
    crate::json::encode_bounded(&layer, MAX_INPUT_BYTES, d)?;
    Ok(layer)
}
