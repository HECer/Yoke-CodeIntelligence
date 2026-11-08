//! Pure redacted Meaning comparison for already admitted published Views.
//! Source/proof admission and final response quotas belong to the parent.
use crate::{
    capture::Deadline,
    graph_view::View,
    meaning_graph::Binding,
    meaning_view::{
        MeaningConcept, MeaningEvidence, MeaningHyperedge, MeaningPacket, MeaningRelation,
        MeaningSelectionRef, ReceiptProvenance,
    },
};
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

const DIGEST_METHOD: &str = "sha256_canonical_original_json_v1";

enum Claim<'a> {
    Concept(&'a MeaningConcept),
    Relation(&'a MeaningRelation),
    Hyperedge(&'a MeaningHyperedge),
}
impl Claim<'_> {
    fn id(&self) -> &str {
        match self {
            Self::Concept(row) => &row.id,
            Self::Relation(row) => &row.id,
            Self::Hyperedge(row) => &row.id,
        }
    }
    fn materialized_id(&self) -> &str {
        match self {
            Self::Concept(row) => &row.claim_id,
            Self::Relation(row) => &row.claim_id,
            Self::Hyperedge(row) => &row.claim_id,
        }
    }
    fn evidence_ids(&self) -> &[String] {
        match self {
            Self::Concept(row) => &row.evidence_ids,
            Self::Relation(row) => &row.evidence_ids,
            Self::Hyperedge(row) => &row.evidence_ids,
        }
    }
    fn kind(&self) -> &'static str {
        match self {
            Self::Concept(_) => "concept",
            Self::Relation(_) => "relation",
            Self::Hyperedge(_) => "hyperedge",
        }
    }
    fn digest(&self, deadline: &Deadline) -> Result<String> {
        // Hash the complete original selected row, including omitted prose.
        match self {
            Self::Concept(row) => super::canonical_digest(row, deadline),
            Self::Relation(row) => super::canonical_digest(row, deadline),
            Self::Hyperedge(row) => super::canonical_digest(row, deadline),
        }
    }
    fn nested_rows(&self) -> Result<usize> {
        let evidence = self
            .evidence_ids()
            .len()
            .checked_mul(2)
            .context("Comparison Meaning evidence-row count overflow")?;
        let participants = match self {
            Self::Hyperedge(row) => row.participants.len(),
            _ => 0,
        };
        evidence
            .checked_add(participants)
            .context("Comparison Meaning claim-row count overflow")
    }
}

struct Rows {
    used: usize,
    limit: usize,
}
impl Rows {
    fn admit(&mut self, additional: usize, deadline: &Deadline) -> Result<()> {
        deadline.check()?;
        let used = self
            .used
            .checked_add(additional)
            .context("Comparison Meaning row count overflow")?;
        ensure!(used <= self.limit, "comparison_capacity_exceeded");
        self.used = used;
        Ok(())
    }
}

struct ProjectedClaim {
    kind: &'static str,
    digest: String,
    value: Value,
}

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
struct ReceiptKey {
    original_job_id: String,
    response_artifact_id: String,
    attempt_id: String,
    request_id: String,
}
impl From<&ReceiptProvenance> for ReceiptKey {
    fn from(original: &ReceiptProvenance) -> Self {
        Self {
            original_job_id: original.original_job_id.clone(),
            response_artifact_id: original.response_artifact_id.clone(),
            attempt_id: original.attempt_id.clone(),
            request_id: original.request_id.clone(),
        }
    }
}

#[derive(Default)]
struct SideProjection {
    present: bool,
    claims: BTreeMap<String, ProjectedClaim>,
    receipts: Vec<Value>,
    receipt_indices: BTreeMap<ReceiptKey, Vec<usize>>,
    unknown_cost_packets: usize,
    unknown_usage_packets: usize,
}

fn evidence(original: &MeaningEvidence, deadline: &Deadline) -> Result<Value> {
    deadline.check()?;
    Ok(json!({
        "originalEvidenceId": original.id,
        "baseEvidenceId": original.base_evidence_id,
        "originalRowDigest": super::canonical_digest(original, deadline)?,
        "digestMethod": DIGEST_METHOD,
        "sourceId": original.source_id,
        "snapshotId": original.snapshot_id,
        "path": original.path,
        "contentHash": original.content_hash,
        "startByte": original.start_byte,
        "endByte": original.end_byte,
        "startLine": original.start_line,
        "endLine": original.end_line,
        "precision": "captured_utf8_quote_range",
        "bindingDerivation": "checked_meaning_evidence_quote_range_not_declaration",
        "textOmitted": true,
    }))
}

fn unit_binding(original: &Binding, deadline: &Deadline) -> Result<Value> {
    deadline.check()?;
    Ok(json!({
        "originalRowDigest": super::canonical_digest(original, deadline)?,
        "digestMethod": DIGEST_METHOD,
        "sourceId": original.source_id,
        "path": original.path,
        "contentHash": original.content_hash,
        "startByte": original.start_byte,
        "endByte": original.end_byte,
        "textOmitted": true,
    }))
}

fn receipt(
    packet: &MeaningPacket,
    selection: &MeaningSelectionRef,
    side: &str,
    deadline: &Deadline,
) -> Result<Value> {
    deadline.check()?;
    let original = &packet.provenance;
    // This digest excludes descriptor fields and the separately bound units.
    let digest = super::canonical_digest(original, deadline)?;
    let mut bindings = Vec::with_capacity(packet.source_bindings.len());
    for binding in &packet.source_bindings {
        deadline.check()?;
        bindings.push(unit_binding(binding, deadline)?);
    }
    let mut coverage = Vec::with_capacity(packet.coverage.len());
    for unit in &packet.coverage {
        deadline.check()?;
        coverage.push(json!({"sourceId":unit.source_id,"outcome":unit.outcome}));
    }
    Ok(json!({
        "side": side,
        "originalRowDigest": digest,
        "digestMethod": DIGEST_METHOD,
        "selectionIndex": packet.selection_index,
        "packetIndex": packet.index,
        "graphId": selection.graph_id,
        "jobId": selection.job_id,
        "snapshotId": original.snapshot_id,
        "originalSnapshotId": original.original_snapshot_id,
        "originalJobId": original.original_job_id,
        "responseArtifactId": original.response_artifact_id,
        "attemptId": original.attempt_id,
        "requestId": original.request_id,
        "model": original.model,
        "modelVersion": original.model_version,
        "currency": original.currency,
        "providerReportedCostMicros": original.provider_reported_cost_micros,
        "providerReportedUsage": original.provider_reported_usage,
        "rebound": original.rebound,
        "bindingValidation": original.binding_validation,
        "sourceBindings": bindings,
        "coverage": coverage,
        "textOmitted": true,
    }))
}

struct PacketProjection<'a> {
    view: &'a View,
    packet: &'a MeaningPacket,
    selection: &'a MeaningSelectionRef,
    side: &'a str,
    evidence: BTreeMap<&'a str, &'a MeaningEvidence>,
}
impl PacketProjection<'_> {
    fn claim(&self, original: Claim<'_>, deadline: &Deadline) -> Result<ProjectedClaim> {
        deadline.check()?;
        let digest = original.digest(deadline)?;
        let mut evidence = Vec::with_capacity(original.evidence_ids().len());
        for id in original.evidence_ids() {
            deadline.check()?;
            let original = self
                .evidence
                .get(id.as_str())
                .context("Comparison Meaning claim lacks admitted evidence")?;
            evidence.push(self::evidence(original, deadline)?);
        }
        let mut value = json!({
            "side": self.side,
            "kind": original.kind(),
            "originalClaimId": original.id(),
            "materializedClaimId": original.materialized_id(),
            "originalRowDigest": digest,
            "digestMethod": DIGEST_METHOD,
            "workspaceId": self.view.workspace_id,
            "snapshotId": self.view.snapshot_id,
            "graphViewId": self.view.id,
            "graphId": self.selection.graph_id,
            "jobId": self.selection.job_id,
            "selectionIndex": self.packet.selection_index,
            "packetIndex": self.packet.index,
            "evidenceIds": original.evidence_ids(),
            "evidence": evidence,
            "resolution": "inferred_concept",
            "providerAttestation": "unverified",
            "textOmitted": true,
        });
        match &original {
            Claim::Concept(_) => {}
            Claim::Relation(row) => {
                value["sourceId"] = json!(row.source);
                value["targetId"] = json!(row.target);
                value["relation"] = json!(row.relation);
            }
            Claim::Hyperedge(row) => {
                value["relation"] = json!(row.relation);
                let mut participants = Vec::with_capacity(row.participants.len());
                for participant in &row.participants {
                    deadline.check()?;
                    participants
                        .push(json!({"role":participant.role,"concept":participant.concept}));
                }
                value["participants"] = Value::Array(participants);
            }
        }
        deadline.check()?;
        Ok(ProjectedClaim {
            kind: original.kind(),
            digest,
            value,
        })
    }
}

fn project(
    view: &View,
    side: &str,
    rows: &mut Rows,
    deadline: &Deadline,
) -> Result<SideProjection> {
    deadline.check()?;
    let Some(layer) = &view.meaning else {
        return Ok(SideProjection::default());
    };
    let mut result = SideProjection {
        present: true,
        ..SideProjection::default()
    };
    let mut seen_packets = BTreeSet::new();
    for packet in &layer.packets {
        deadline.check()?;
        ensure!(
            seen_packets.insert((packet.selection_index, packet.index)),
            "Comparison Meaning packet repeated"
        );
        let selection = layer
            .selections
            .get(packet.selection_index)
            .context("Comparison Meaning selection absent")?;
        let mut evidence_index = BTreeMap::new();
        for original in &packet.evidence {
            deadline.check()?;
            ensure!(
                evidence_index
                    .insert(original.id.as_str(), original)
                    .is_none(),
                "Comparison Meaning evidence repeated"
            );
        }
        let projection = PacketProjection {
            view,
            packet,
            selection,
            side,
            evidence: evidence_index,
        };
        let originals = packet
            .concepts
            .iter()
            .map(Claim::Concept)
            .chain(packet.relations.iter().map(Claim::Relation))
            .chain(packet.hyperedges.iter().map(Claim::Hyperedge));
        for original in originals {
            deadline.check()?;
            rows.admit(original.nested_rows()?, deadline)?;
            let id = original.id().to_owned();
            let claim = projection.claim(original, deadline)?;
            ensure!(
                result.claims.insert(id, claim).is_none(),
                "Comparison selected Meaning claim repeated"
            );
        }
        if packet.provenance.provider_reported_cost_micros.is_none() {
            result.unknown_cost_packets = result
                .unknown_cost_packets
                .checked_add(1)
                .context("Comparison unknown-cost count overflow")?;
        }
        if packet.provenance.provider_reported_usage.is_none() {
            result.unknown_usage_packets = result
                .unknown_usage_packets
                .checked_add(1)
                .context("Comparison unknown-usage count overflow")?;
        }
        // One provenance row plus every independently retained unit and coverage
        // entry. Charge before constructing those descriptor arrays.
        let receipt_rows = packet
            .source_bindings
            .len()
            .checked_add(packet.coverage.len())
            .and_then(|nested| nested.checked_add(1))
            .context("Comparison Meaning receipt-row count overflow")?;
        rows.admit(receipt_rows, deadline)?;
        let key = ReceiptKey::from(&packet.provenance);
        result
            .receipt_indices
            .entry(key)
            .or_default()
            .push(result.receipts.len());
        result
            .receipts
            .push(receipt(packet, selection, side, deadline)?);
    }
    deadline.check()?;
    Ok(result)
}

/// The caller must supply Views from the comparison's sealed admitted pair.
/// This pure helper does not establish source grants or interpret provider truth.
pub(super) fn compare(
    before: &View,
    after: &View,
    max_result_rows: usize,
    deadline: &Deadline,
) -> Result<Value> {
    deadline.check()?;
    let mut rows = Rows {
        used: 0,
        limit: max_result_rows,
    };
    let before = project(before, "before", &mut rows, deadline)?;
    let after = project(after, "after", &mut rows, deadline)?;
    let ids: BTreeSet<_> = before
        .claims
        .keys()
        .chain(after.claims.keys())
        .map(String::as_str)
        .collect();
    let mut shared = Vec::new();
    let mut added = Vec::new();
    let mut removed = Vec::new();
    let mut changes = Vec::with_capacity(ids.len());
    for id in ids {
        deadline.check()?;
        // One selected ID and one claimChanges element; nested descriptors were
        // already admitted individually while projecting both sides.
        rows.admit(2, deadline)?;
        let row = match (before.claims.get(id), after.claims.get(id)) {
            (Some(left), Some(right)) => {
                ensure!(
                    left.kind == right.kind && left.digest == right.digest,
                    "Comparison selected Meaning identity conflicts"
                );
                shared.push(id);
                json!({
                    "changeKind":"shared","before":left.value,"after":right.value,
                    "identityBasis":"exact_selected_claim_id","verifiedIdentity":false,
                })
            }
            (Some(left), None) => {
                removed.push(id);
                json!({
                    "changeKind":"removed","before":left.value,"after":null,
                    "identityBasis":"selected_claim_identity_absent","verifiedIdentity":false,
                })
            }
            (None, Some(right)) => {
                added.push(id);
                json!({
                    "changeKind":"added","before":null,"after":right.value,
                    "identityBasis":"selected_claim_identity_absent","verifiedIdentity":false,
                })
            }
            (None, None) => anyhow::bail!("Comparison selected Meaning identity absent"),
        };
        changes.push(row);
    }
    let mut reuse = Vec::new();
    for (key, before_indices) in &before.receipt_indices {
        deadline.check()?;
        let Some(after_indices) = after.receipt_indices.get(key) else {
            continue;
        };
        rows.admit(1, deadline)?;
        let mut before_rows = Vec::with_capacity(before_indices.len());
        let mut after_rows = Vec::with_capacity(after_indices.len());
        for index in before_indices {
            deadline.check()?;
            let copy_rows = super::row_units(&before.receipts[*index], deadline)?
                .checked_add(1)
                .context("Comparison Meaning receipt-copy row count overflow")?;
            rows.admit(copy_rows, deadline)?;
            before_rows.push(before.receipts[*index].clone());
        }
        for index in after_indices {
            deadline.check()?;
            let copy_rows = super::row_units(&after.receipts[*index], deadline)?
                .checked_add(1)
                .context("Comparison Meaning receipt-copy row count overflow")?;
            rows.admit(copy_rows, deadline)?;
            after_rows.push(after.receipts[*index].clone());
        }
        reuse.push(json!({
            "originalJobId":key.original_job_id,
            "responseArtifactId":key.response_artifact_id,
            "attemptId":key.attempt_id,
            "requestId":key.request_id,
            "beforeRows":before_rows,
            "afterRows":after_rows,
            "provenance":"original_receipt_reuse_not_claim_identity",
        }));
    }
    deadline.check()?;
    let result = json!({
        "beforePresent":before.present,
        "afterPresent":after.present,
        "sharedClaimIds":shared,
        "addedClaimIds":added,
        "removedClaimIds":removed,
        "claimChanges":changes,
        "receiptReuse":reuse,
        "provenance":{
            "beforeUnknownCostPackets":before.unknown_cost_packets,
            "afterUnknownCostPackets":after.unknown_cost_packets,
            "beforeUnknownUsagePackets":before.unknown_usage_packets,
            "afterUnknownUsagePackets":after.unknown_usage_packets,
            "beforeRows":before.receipts,
            "afterRows":after.receipts,
        },
        "resolution":"inferred_concept",
        "providerAttestation":"unverified",
    });
    super::check_component_rows(&result, max_result_rows, deadline)?;
    ensure!(
        rows.used == super::row_units(&result, deadline)?,
        "Comparison Meaning row accounting mismatch"
    );
    deadline.check()?;
    Ok(result)
}
