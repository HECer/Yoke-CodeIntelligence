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
use std::collections::BTreeMap;

#[path = "git_source_lineage.rs"]
mod lineage;
pub(crate) use lineage::{LineageCapacity, disk_preflight};

pub(super) struct Header {
    pub(super) receipt: SourceReceipt,
    pub(super) inheritance: DerivedCaptureOptionsRecord,
    pub(super) commit: Vec<u8>,
}
pub(super) fn header(store: &Store, snapshot: &Snapshot, d: &Deadline) -> Result<Header> {
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
    let commit = artifact_bytes(store, &receipt.commit_artifact_id, 1_000_000, d)?;
    Ok(Header {
        receipt,
        inheritance,
        commit,
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
    epoch: AnalyzerEpoch,
    commit: String,
    tree: String,
    sources: BTreeMap<String, String>,
}
impl VerifiedSource {
    pub(crate) fn into_parts(self) -> (AnalyzerEpoch, String, String, BTreeMap<String, String>) {
        (self.epoch, self.commit, self.tree, self.sources)
    }
}
pub(crate) fn verified(
    store: &Store,
    snapshot: &Snapshot,
    maximum: usize,
    lineage_maximum: usize,
    d: &Deadline,
) -> Result<VerifiedSource> {
    budget(snapshot, maximum, d)?;
    lineage::preflight(store, snapshot, lineage_maximum, d)?;
    let header = header(store, snapshot, d)?;
    let epoch = AnalyzerEpoch::parse(&header.receipt.analyzer)?;
    let sources = collect(store, snapshot, &header, d)?;
    Ok(VerifiedSource {
        epoch,
        commit: header.receipt.commit_id,
        tree: header.receipt.tree_id,
        sources,
    })
}
