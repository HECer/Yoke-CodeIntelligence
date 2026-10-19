//! Bounded retained source verification; one CAS read per selected file.
use super::{
    SourceReceipt, artifact_bytes, cas_bytes, identity, record_value,
    validate_source_objects_retained,
};
use crate::{
    analysis::AnalyzerEpoch,
    capture::{Deadline, DerivedCaptureOptionsRecord},
    model::Snapshot,
    store::Store,
};
use anyhow::{Result, ensure};
use std::collections::{BTreeMap, BTreeSet};

#[path = "git_source_lineage.rs"]
mod lineage;
pub(crate) use lineage::{LineageCapacity, disk_preflight};

pub(super) struct Header {
    pub(super) receipt: SourceReceipt,
    pub(super) inheritance: DerivedCaptureOptionsRecord,
    pub(super) commit: Vec<u8>,
    proof_bytes: usize,
}
pub(super) fn header(store: &Store, snapshot: &Snapshot, d: &Deadline) -> Result<Header> {
    header_for_selection(store, snapshot, None, None, d)
}
fn header_for_selection(
    store: &Store,
    snapshot: &Snapshot,
    allowed_commits: Option<&BTreeSet<String>>,
    proof_limit: Option<usize>,
    d: &Deadline,
) -> Result<Header> {
    d.check()?;
    store.set_deadline(d)?;
    ensure!(
        identity(&store.snapshot(&snapshot.id)?) == identity(snapshot),
        "Git source requires exact immutable manifest"
    );
    let receipt: SourceReceipt = crate::json::decode(record_value(
        store,
        super::SOURCE_KIND,
        &snapshot.id,
        16_000_000,
        d,
    )?)?;
    ensure!(
        snapshot.kind == "derived"
            && snapshot.capture_consistency == "immutable_git_objects_verified"
            && [
                "yoke-tree-sitter-9",
                "yoke-tree-sitter-8",
                "yoke-tree-sitter-6",
                "yoke-tree-sitter-4"
            ]
            .contains(&receipt.analyzer.as_str())
            && receipt.snapshot_id == snapshot.id
            && receipt.workspace_id == store.workspace_id
            && snapshot.workspace_id == store.workspace_id
            && receipt.snapshot_hash == identity(snapshot)
            && receipt.producer == "yoke-native-git-1"
            && receipt.blobs.len() == snapshot.files.len(),
        "Git source receipt scope mismatch"
    );
    let inheritance: DerivedCaptureOptionsRecord =
        store.get_record(crate::capture::DERIVED_OPTIONS_RECORD_KIND, &snapshot.id)?;
    ensure!(
        inheritance.origin_snapshot_id == receipt.origin_snapshot_id,
        "Git source capture origin mismatch"
    );
    crate::capture::options_for_snapshot_at(store, snapshot, AnalyzerEpoch::V9)?;
    ensure!(
        allowed_commits.is_none_or(|ids| ids.contains(&receipt.commit_id)),
        "Git source commit outside selected history"
    );
    let proof_bytes = if let Some(limit) = proof_limit {
        let size: usize = store.db.query_row(
            "SELECT length(CAST(value AS BLOB)) FROM records WHERE kind='artifact' AND id=?1",
            [&receipt.commit_artifact_id],
            |r| r.get(0),
        )?;
        ensure!(size <= limit.min(64_000), LineageCapacity);
        let registration = record_value(store, "artifact", &receipt.commit_artifact_id, size, d)?;
        let total = usize::try_from(
            registration["totalBytes"]
                .as_u64()
                .ok_or_else(|| anyhow::anyhow!("Git artifact byte count unavailable"))?,
        )?;
        ensure!(total <= limit - size && total <= 1_000_000, LineageCapacity);
        size + total
    } else {
        0
    };
    let commit = artifact_bytes(store, &receipt.commit_artifact_id, 1_000_000, d)?;
    Ok(Header {
        receipt,
        inheritance,
        commit,
        proof_bytes,
    })
}
fn budget(snapshot: &Snapshot, maximum: usize, d: &Deadline) -> Result<()> {
    ensure!(
        maximum <= 64_000_000 && snapshot.files.len() <= 10000,
        "Git retained source capacity exceeded"
    );
    let mut bytes = 0usize;
    for file in &snapshot.files {
        d.check()?;
        ensure!(
            file.size <= 4_000_000 && file.size <= maximum.saturating_sub(bytes),
            "Git retained source capacity exceeded"
        );
        bytes += file.size;
    }
    Ok(())
}
fn collect(
    store: &Store,
    snapshot: &Snapshot,
    header: &Header,
    d: &Deadline,
) -> Result<BTreeMap<String, String>> {
    let mut sources = BTreeMap::new();
    validate_source_objects_retained(
        snapshot,
        &header.receipt,
        &header.commit,
        &mut |file| cas_bytes(store, &file.hash, file.size, 4_000_000, d),
        &mut |file, text| {
            ensure!(
                sources.insert(file.path.clone(), text).is_none(),
                "Duplicate Git retained source path"
            );
            Ok(())
        },
        d,
    )?;
    Ok(sources)
}
pub(super) fn retained(
    store: &Store,
    snapshot: &Snapshot,
    maximum: usize,
    d: &Deadline,
) -> Result<(Header, BTreeMap<String, String>)> {
    budget(snapshot, maximum, d)?;
    let header = header(store, snapshot, d)?;
    let sources = collect(store, snapshot, &header, d)?;
    Ok((header, sources))
}
/// Constructed only after complete commit/tree/blob and capture-scope verification.
pub(crate) struct VerifiedSource {
    parts: VerifiedParts,
}
pub(crate) struct TreeFact {
    pub(crate) mode: String,
    pub(crate) object_id: String,
    pub(crate) object_type: String,
}
pub(crate) struct VerifiedParts {
    pub(crate) epoch: AnalyzerEpoch,
    pub(crate) commit: String,
    pub(crate) tree: String,
    pub(crate) sources: BTreeMap<String, String>,
    pub(crate) entries: BTreeMap<String, TreeFact>,
    pub(crate) lineage_bytes: usize,
    pub(crate) proof_bytes: usize,
}
impl VerifiedSource {
    pub(crate) fn into_parts(self) -> VerifiedParts {
        self.parts
    }
}
pub(crate) fn verified(
    store: &Store,
    snapshot: &Snapshot,
    maximum: usize,
    lineage_maximum: usize,
    allowed_commits: Option<&BTreeSet<String>>,
    proof_limit: Option<usize>,
    d: &Deadline,
) -> Result<VerifiedSource> {
    budget(snapshot, maximum, d)?;
    let lineage_bytes = lineage::preflight(store, snapshot, lineage_maximum, d)?;
    let header = header_for_selection(store, snapshot, allowed_commits, proof_limit, d)?;
    let epoch = AnalyzerEpoch::parse(&header.receipt.analyzer)?;
    let sources = collect(store, snapshot, &header, d)?;
    Ok(VerifiedSource {
        parts: VerifiedParts {
            epoch,
            commit: header.receipt.commit_id,
            tree: header.receipt.tree_id,
            sources,
            entries: header
                .receipt
                .tree_entries
                .into_iter()
                .map(|entry| {
                    (
                        entry.path,
                        TreeFact {
                            mode: entry.mode,
                            object_id: entry.object_id,
                            object_type: entry.object_type,
                        },
                    )
                })
                .collect(),
            lineage_bytes,
            proof_bytes: header.proof_bytes,
        },
    })
}
