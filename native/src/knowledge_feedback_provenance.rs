//! Borrow original receipt metadata during output admission, without cloning claims.
use super::*;
use crate::graph_view::comparison_input::CheckedComparisonSide;
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Packet<'a> {
    selection_index: usize,
    index: usize,
    input_hash: &'a str,
    unit_set_hash: &'a str,
    contract_hash: &'a str,
    source_bindings: &'a [crate::meaning_graph::Binding],
    provenance: &'a crate::meaning_view::ReceiptProvenance,
    coverage: &'a [crate::meaning_graph::UnitCoverage],
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Scope<'a> {
    workspace_id: &'a str,
    snapshot_id: &'a str,
    kind: &'a str,
    capture_consistency: &'a str,
    files: usize,
    source_bytes: usize,
    role: &'static str,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct ViewProof<'a> {
    view_id: &'a str,
    view_artifact_id: &'a str,
    base_graph_hash: &'a str,
    view_complete: bool,
    view_limitations: &'a [String],
    overlays: &'a [Value],
    meaning_scope: Option<&'a crate::meaning_view::MeaningScope>,
    meaning_selections: &'a [crate::meaning_view::MeaningSelectionRef],
    meaning_packets: Vec<Packet<'a>>,
    provider_attestation: &'static str,
    source_scopes: Vec<Scope<'a>>,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct PairProof<'a> {
    pair_id: &'a str,
    origin: ViewProof<'a>,
    evaluated: ViewProof<'a>,
}
fn view<'a>(side: &'a CheckedComparisonSide<'_>, d: &Deadline) -> Result<ViewProof<'a>> {
    let original = side.view();
    let meaning = original.meaning.as_ref();
    let mut packets = Vec::new();
    if let Some(layer) = meaning {
        for packet in &layer.packets {
            d.check()?;
            packets.push(Packet {
                selection_index: packet.selection_index,
                index: packet.index,
                input_hash: &packet.input_hash,
                unit_set_hash: &packet.unit_set_hash,
                contract_hash: &packet.contract_hash,
                source_bindings: &packet.source_bindings,
                provenance: &packet.provenance,
                coverage: &packet.coverage,
            });
        }
    }
    let mut scopes = Vec::new();
    for snapshot in side.source_scopes() {
        d.check()?;
        let mut bytes = 0usize;
        for file in &snapshot.files {
            d.check()?;
            bytes = bytes
                .checked_add(file.size)
                .context("feedback_capacity_exceeded")?;
        }
        scopes.push(Scope {
            workspace_id: &snapshot.workspace_id,
            snapshot_id: &snapshot.id,
            kind: &snapshot.kind,
            capture_consistency: &snapshot.capture_consistency,
            files: snapshot.files.len(),
            source_bytes: bytes,
            role: if snapshot.id == side.primary_snapshot().id {
                "primary_evidence"
            } else {
                "receipt_replay_only"
            },
        });
    }
    Ok(ViewProof {
        view_id: &original.id,
        view_artifact_id: &original.artifact_id,
        base_graph_hash: &original.base_graph_hash,
        view_complete: original.complete,
        view_limitations: &original.limitations,
        overlays: &original.overlays,
        meaning_scope: meaning.map(|m| &m.scope),
        meaning_selections: meaning.map_or(&[][..], |m| m.selections.as_slice()),
        meaning_packets: packets,
        provider_attestation: "unverified",
        source_scopes: scopes,
    })
}
pub(super) fn pairs<'a>(
    context: &'a storage::Context<'_>,
    d: &Deadline,
) -> Result<Vec<PairProof<'a>>> {
    context
        .pairs
        .iter()
        .map(|(key, pair)| {
            d.check()?;
            Ok(PairProof {
                pair_id: key,
                origin: view(pair.before(), d)?,
                evaluated: view(pair.after(), d)?,
            })
        })
        .collect()
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Envelope<'a, T: Serialize> {
    #[serde(flatten)]
    value: &'a T,
    source_proofs: Vec<PairProof<'a>>,
}
pub(super) fn output_with_proofs<T: Serialize>(
    value: &T,
    context: &storage::Context<'_>,
    d: &Deadline,
) -> Result<Value> {
    output(
        &Envelope {
            value,
            source_proofs: pairs(context, d)?,
        },
        context.options,
        d,
    )
}
