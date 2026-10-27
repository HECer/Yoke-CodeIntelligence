//! Self-contained typed Meaning-View archives with storeless proof replay.
use crate::{
    capture::Deadline,
    graph_view::{self, ArchiveProof, VerifiedArchivedMeaningView, View},
    meaning_build::{FrozenMemberReader, FrozenSourceIndex, FrozenSourceMember},
    model::{Graph, Snapshot, SourceFile, hash, valid_id},
    store::Store,
};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

const PROFILE: &str = "meaning_view_archive_v1";
const MAX_WIRE: usize = 32_000_000;
const MAX_SOURCES: usize = 64_000_000;

#[derive(Clone, Copy, Debug)]
pub struct Limits {
    pub max_wire_bytes: usize,
    pub max_source_bytes: usize,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            max_wire_bytes: MAX_WIRE,
            max_source_bytes: MAX_SOURCES,
        }
    }
}
impl Limits {
    pub(crate) fn check(self, deadline: &Deadline) -> Result<()> {
        deadline.check()?;
        ensure!(
            (1..=MAX_WIRE).contains(&self.max_wire_bytes)
                && (1..=MAX_SOURCES).contains(&self.max_source_bytes),
            "Invalid typed archive capacity"
        );
        Ok(())
    }
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Wire {
    profile: String,
    snapshot: Snapshot,
    archive: ArchiveProof,
    source_objects: BTreeMap<String, String>,
}
// Neither transport type has a public constructor or a Deserialize implementation.
pub struct PreparedMeaningArchive {
    bytes: Vec<u8>,
}
impl std::fmt::Debug for PreparedMeaningArchive {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PreparedMeaningArchive")
            .field("total_bytes", &self.bytes.len())
            .finish()
    }
}
impl PreparedMeaningArchive {
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
}
pub struct CheckedMeaningArchive {
    bytes: Vec<u8>,
    archive_id: String,
    verified: VerifiedArchivedMeaningView,
    manifest: FrozenSourceIndex,
    source_objects: BTreeMap<String, String>,
}
impl std::fmt::Debug for CheckedMeaningArchive {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CheckedMeaningArchive")
            .field("archive_id", &self.archive_id)
            .field("total_bytes", &self.bytes.len())
            .finish()
    }
}
impl CheckedMeaningArchive {
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
    pub fn archive_id(&self) -> &str {
        &self.archive_id
    }
    pub fn snapshot(&self) -> &Snapshot {
        self.verified.snapshot()
    }
    pub fn canonical_graph(&self) -> &Graph {
        self.verified.canonical_graph()
    }
    pub fn view(&self) -> &View {
        self.verified.view()
    }
    pub(crate) fn source_projection(&self) -> &Graph {
        self.verified.source_projection()
    }
    pub(crate) fn proof(&self) -> &ArchiveProof {
        self.verified.proof()
    }
    pub(crate) fn source_objects(&self) -> &BTreeMap<String, String> {
        &self.source_objects
    }
    pub(crate) fn source_files(&self) -> impl Iterator<Item = &SourceFile> {
        self.manifest.unique_files()
    }
    pub(crate) fn source_scope_count(&self) -> usize {
        self.manifest.scope_count()
    }
    pub(crate) fn source_file_entries(&self) -> usize {
        self.manifest.file_entries()
    }
    pub(crate) fn source_bytes(&self) -> usize {
        self.manifest.unique_bytes()
    }
    pub(crate) fn analyzer_epoch(&self) -> crate::analysis::AnalyzerEpoch {
        self.verified.analyzer_epoch()
    }
}

fn source_manifest(
    snapshot: &Snapshot,
    proof: &ArchiveProof,
    limits: Limits,
    deadline: &Deadline,
) -> Result<FrozenSourceIndex> {
    limits.check(deadline)?;
    ensure!(
        proof.view.meaning.is_some() && proof.source_scopes().next().is_some(),
        "Typed archive requires a complete explicit meaning source proof"
    );
    for scope in std::iter::once(snapshot).chain(proof.source_scopes()) {
        deadline.check()?;
        ensure!(
            valid_id(&scope.id)
                && valid_id(&scope.workspace_id)
                && scope.workspace_id == snapshot.workspace_id
                && matches!(scope.kind.as_str(), "disk" | "derived" | "overlay"),
            "Typed archive source has no supported native scope"
        );
    }
    // The common index verifies complete scope identity and exact file membership;
    // native capture admission above remains specific to this archive profile.
    FrozenSourceIndex::new(
        std::iter::once(snapshot).chain(proof.source_scopes()),
        limits.max_source_bytes,
        deadline,
    )
}

fn encode(wire: &Wire, limits: Limits, deadline: &Deadline) -> Result<Vec<u8>> {
    // Value objects use sorted keys. Embedded raw payload strings and array order stay exact.
    crate::json::encode_bounded(
        &serde_json::to_value(wire)?,
        limits.max_wire_bytes,
        deadline,
    )
}

/// Prepare an already-published selection through the guarded archive path.
pub fn prepare_published(
    store: &Store,
    view_id: &str,
    limits: Limits,
    deadline: &Deadline,
) -> Result<CheckedMeaningArchive> {
    limits.check(deadline)?;
    let verified = graph_view::archive_meaning_view_by_id(
        store,
        view_id,
        limits.max_wire_bytes,
        limits.max_source_bytes,
        deadline,
    )?;
    package_verified(store, verified, limits, deadline)
}

pub fn prepare(
    store: &Store,
    view: &View,
    limits: Limits,
    deadline: &Deadline,
) -> Result<PreparedMeaningArchive> {
    limits.check(deadline)?;
    // The bridge performs every source-policy preflight before its first source CAS read.
    let verified = graph_view::archive_meaning_view(
        store,
        view,
        limits.max_wire_bytes,
        limits.max_source_bytes,
        deadline,
    )?;
    let checked = package_verified(store, verified, limits, deadline)?;
    Ok(PreparedMeaningArchive {
        bytes: checked.bytes,
    })
}

fn package_verified(
    store: &Store,
    verified: VerifiedArchivedMeaningView,
    limits: Limits,
    deadline: &Deadline,
) -> Result<CheckedMeaningArchive> {
    let manifest = source_manifest(verified.snapshot(), verified.proof(), limits, deadline)?;
    // Recheck every scope before copying any source; a hash is never a source grant.
    for scope in manifest.scopes() {
        let policy = crate::capture::source_scope_policy(store, scope)?;
        policy.require_legacy_text("Meaning archive source export")?;
        for file in &scope.files {
            deadline.check()?;
            ensure!(
                policy.file(&file.path)? == file,
                "Typed archive source policy membership changed"
            );
        }
    }
    let mut source_objects = BTreeMap::new();
    for scope in manifest.scopes() {
        let policy = crate::capture::source_scope_policy(store, scope)?;
        let bound = manifest.bind(scope, deadline)?;
        for member in bound.members() {
            let file = member.file();
            deadline.check()?;
            if !source_objects.contains_key(&file.hash) {
                let bytes = policy.read_source(&file.path)?;
                ensure!(
                    bytes.len() == file.size && hash(&bytes) == file.hash,
                    "Typed archive source bytes changed"
                );
                source_objects.insert(file.hash.clone(), String::from_utf8(bytes)?);
            }
        }
    }
    let wire = Wire {
        profile: PROFILE.into(),
        snapshot: verified.snapshot().clone(),
        archive: verified.proof().clone(),
        source_objects,
    };
    let bytes = encode(&wire, limits, deadline)?;
    Ok(CheckedMeaningArchive {
        archive_id: hash(&bytes),
        bytes,
        verified,
        manifest,
        source_objects: wire.source_objects,
    })
}

pub fn parse(bytes: &[u8], limits: Limits, deadline: &Deadline) -> Result<CheckedMeaningArchive> {
    limits.check(deadline)?;
    let raw = crate::json::parse(bytes, limits.max_wire_bytes)?;
    let wire: Wire = crate::json::decode(raw.clone())?;
    // Snapshot/Graph contain historical non-strict serde types: refuse any lossy nested decode.
    ensure!(
        wire.profile == PROFILE
            && serde_json::to_value(&wire)? == raw
            && raw["archive"]["meaningArchive"].is_object(),
        "Unknown or lossy typed archive schema"
    );
    let manifest = source_manifest(&wire.snapshot, &wire.archive, limits, deadline)?;
    ensure!(
        wire.source_objects.len() == manifest.unique_files().count(),
        "Typed archive source object set mismatch"
    );
    for file in manifest.unique_files() {
        deadline.check()?;
        let text = wire
            .source_objects
            .get(&file.hash)
            .context("Typed archive source object missing")?;
        ensure!(
            text.len() == file.size && hash(text.as_bytes()) == file.hash,
            "Typed archive source object hash/size mismatch"
        );
    }
    let mut read = |member: &FrozenSourceMember, d: &Deadline| -> Result<Vec<u8>> {
        d.check()?;
        let file = member.file();
        Ok(wire
            .source_objects
            .get(&file.hash)
            .context("Typed archive source object absent")?
            .as_bytes()
            .to_vec())
    };
    let verified = graph_view::replay_meaning_view_archive(
        &wire.snapshot,
        &wire.archive,
        &mut read,
        limits.max_wire_bytes,
        limits.max_source_bytes,
        deadline,
    )?;
    let canonical = encode(&wire, limits, deadline)?;
    Ok(CheckedMeaningArchive {
        archive_id: hash(&canonical),
        bytes: canonical,
        verified,
        manifest,
        source_objects: wire.source_objects,
    })
}

/// Registry-only reassembly reads the complete native registration closure, never another Store.
pub(crate) fn reassemble(
    snapshot: &Snapshot,
    proof: &ArchiveProof,
    read_source: &mut FrozenMemberReader<'_>,
    limits: Limits,
    deadline: &Deadline,
) -> Result<CheckedMeaningArchive> {
    let manifest = source_manifest(snapshot, proof, limits, deadline)?;
    let mut source_objects = BTreeMap::new();
    for scope in manifest.scopes() {
        let bound = manifest.bind(scope, deadline)?;
        for member in bound.members() {
            let file = member.file();
            deadline.check()?;
            if !source_objects.contains_key(&file.hash) {
                let bytes = read_source(&member, deadline)?;
                ensure!(
                    bytes.len() == file.size && hash(&bytes) == file.hash,
                    "Typed collection source object mismatch"
                );
                source_objects.insert(file.hash.clone(), String::from_utf8(bytes)?);
            }
        }
    }
    let wire = Wire {
        profile: PROFILE.into(),
        snapshot: snapshot.clone(),
        archive: proof.clone(),
        source_objects,
    };
    parse(&encode(&wire, limits, deadline)?, limits, deadline)
}
