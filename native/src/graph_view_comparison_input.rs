//! Sealed comparison input admission over fully replayed immutable selections.
//! Source bytes and authority remain inside sealed, fully replayed input capabilities.

use anyhow::Result;
use rusqlite::{OptionalExtension, Transaction, TransactionBehavior, params};
use serde::Serialize;
use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    fmt,
    io::Write,
};

use crate::{
    analysis::AnalyzerEpoch,
    capture::{CaptureOptions, Deadline},
    graph_view::{ArchiveProof, View},
    graph_view_archive::CheckedMeaningArchive,
    meaning_build::{
        BoundFrozenScope, FrozenScopePreflight, FrozenSourceIndex, FrozenSourceMember,
    },
    model::{Graph, Snapshot, SourceFile, hash, identity, valid_id},
    store::Store,
    view_comparison::{ComparisonOptions, SelectionPin},
};

const MAX_PROOF: usize = 32_000_000;
// This is the EXISTING per-side archive/index source cap, not a caller aggregate cap.
const MAX_SIDE_SOURCE_BYTES: usize = 64_000_000;
const PROFILE: &str = "yoke_view_comparison_input_1";

#[derive(Debug)]
struct InputFailure(&'static str);
impl fmt::Display for InputFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.0)
    }
}
impl std::error::Error for InputFailure {}
fn failure(code: &'static str) -> anyhow::Error {
    InputFailure(code).into()
}
fn require(condition: bool, code: &'static str) -> Result<()> {
    if condition {
        Ok(())
    } else {
        Err(failure(code))
    }
}
fn capacity(condition: bool) -> Result<()> {
    require(condition, "comparison_capacity_exceeded")
}

// Ordinary loader errors are not a disclosure channel for source/proof values.
// Preserve native deadline/cancel refusals and our own bounded static classifications.
fn sanitized(error: anyhow::Error, deadline: &Deadline) -> anyhow::Error {
    for cause in error.chain() {
        if let Some(cause) = cause.downcast_ref::<InputFailure>() {
            return failure(cause.0);
        }
        // Exact released static messages only. Never expose native Display or
        // retain a native cause: those chains may contain paths or quoted rows.
        if matches!(
            cause.to_string().as_str(),
            "Initial archive metadata capacity exceeded"
                | "Meaning record exceeds capacity"
                | "Meaning dependency exceeds remaining input capacity"
                | "Meaning dependency closure exceeds remaining input capacity"
                | "Meaning archive sources exceed byte capacity"
                | "Meaning archive source count exceeds capacity"
                | "Frozen source scope capacity exceeded"
                | "Frozen source byte capacity exceeded"
                | "View record capacity exceeded"
                | "View artifact byte capacity exceeded"
                | "View input capacity exceeded"
                | "Selected overlay aggregate byte capacity exceeded"
                | "Archived capture proof byte budget exceeded"
                | "JSON input exceeds byte capacity"
                | "JSON output exceeds byte capacity"
        ) {
            return failure("comparison_capacity_exceeded");
        }
    }
    if let Err(error) = deadline.check() {
        return error;
    }
    failure("comparison_input_invalid")
}

struct OwnedSide {
    snapshot: Snapshot,
    proof: ArchiveProof,
    projection: Graph,
    source_objects: BTreeMap<String, String>,
}
enum SideData<'input> {
    Published(Box<OwnedSide>),
    Archive(&'input CheckedMeaningArchive),
}
pub(crate) struct CheckedComparisonPair<'input> {
    before: CheckedComparisonSide<'input>,
    after: CheckedComparisonSide<'input>,
    accounting: ComparisonInputAccounting,
}
pub(crate) struct CheckedComparisonSide<'input> {
    data: SideData<'input>,
    index: FrozenSourceIndex,
    epoch: AnalyzerEpoch,
    options: CaptureOptions,
}
#[derive(Debug)]
pub(crate) struct ComparisonInputAccounting {
    #[cfg_attr(
        not(test),
        allow(
            dead_code,
            reason = "Retained internal audit accounting API; exercised by precise input tests."
        )
    )]
    before_proof_bytes: usize,
    #[cfg_attr(
        not(test),
        allow(
            dead_code,
            reason = "Retained internal audit accounting API; exercised by precise input tests."
        )
    )]
    after_proof_bytes: usize,
    total_proof_bytes: usize,
    distinct_source_bytes: usize,
    #[cfg_attr(
        not(test),
        allow(
            dead_code,
            reason = "Retained internal audit accounting API; exercised by precise input tests."
        )
    )]
    distinct_source_objects: usize,
}
pub(crate) struct BoundComparisonScope<'side> {
    bound: BoundFrozenScope<'side>,
    source_objects: &'side BTreeMap<String, String>,
}
pub(crate) struct ComparisonSource<'scope> {
    member: FrozenSourceMember,
    text: &'scope str,
}
impl fmt::Debug for CheckedComparisonPair<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CheckedComparisonPair")
            .field("accounting", &self.accounting)
            .finish_non_exhaustive()
    }
}
impl fmt::Debug for CheckedComparisonSide<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CheckedComparisonSide")
            .field("analyzer_epoch", &self.epoch)
            .finish_non_exhaustive()
    }
}

pub(crate) fn prepare_published_pair(
    store: &Store,
    before: &SelectionPin,
    after: &SelectionPin,
    options: &ComparisonOptions,
    deadline: &Deadline,
) -> Result<CheckedComparisonPair<'static>> {
    prepare_published(store, before, after, options, None, deadline)
}

/// An additional consumer may deny a scope after native authority admission.
/// This observer cannot grant scope or replace original source-policy checks.
pub(crate) fn prepare_published_pair_with_scope_preflight(
    store: &Store,
    before: &SelectionPin,
    after: &SelectionPin,
    options: &ComparisonOptions,
    extra_preflight: &mut FrozenScopePreflight<'_>,
    deadline: &Deadline,
) -> Result<CheckedComparisonPair<'static>> {
    prepare_published(
        store,
        before,
        after,
        options,
        Some(extra_preflight),
        deadline,
    )
}

pub(crate) fn prepare_archive_pair<'input>(
    before: &'input CheckedMeaningArchive,
    after: &'input CheckedMeaningArchive,
    options: &ComparisonOptions,
    deadline: &Deadline,
) -> Result<CheckedComparisonPair<'input>> {
    (|| {
        validate_options(options, deadline)?;
        require(
            before.snapshot().workspace_id == after.snapshot().workspace_id,
            "comparison_workspace_mismatch",
        )?;
        primary_continuity(before.snapshot(), after.snapshot(), options, deadline)?;
        let before = checked_side(SideData::Archive(before), deadline)?;
        let after = checked_side(SideData::Archive(after), deadline)?;
        finish_pair(before, after, options, deadline)
    })()
    .map_err(|error| sanitized(error, deadline))
}

#[cfg(all(test, windows))]
pub(crate) fn prepare_published_pair_guarded(
    store: &Store,
    before: &SelectionPin,
    after: &SelectionPin,
    options: &ComparisonOptions,
    extra_preflight: &mut FrozenScopePreflight<'_>,
    deadline: &Deadline,
) -> Result<CheckedComparisonPair<'static>> {
    prepare_published(
        store,
        before,
        after,
        options,
        Some(extra_preflight),
        deadline,
    )
}

fn prepare_published(
    store: &Store,
    before: &SelectionPin,
    after: &SelectionPin,
    options: &ComparisonOptions,
    mut extra: Option<&mut FrozenScopePreflight<'_>>,
    deadline: &Deadline,
) -> Result<CheckedComparisonPair<'static>> {
    // The private observer may add a denial only after real native admission.
    // Its original cause is retained only for the cfg(test) guarded entrypoint.
    let mut extra_error = None;
    let result = (|| {
        validate_options(options, deadline)?;
        validate_pin(before)?;
        validate_pin(after)?;
        require(
            before.workspace_id == store.workspace_id,
            "comparison_pin_mismatch workspace_id",
        )?;
        require(
            after.workspace_id == store.workspace_id,
            "comparison_pin_mismatch workspace_id",
        )?;
        store.set_deadline(deadline)?;
        let _read = if store.db.is_autocommit() {
            Some(Transaction::new_unchecked(
                &store.db,
                TransactionBehavior::Deferred,
            )?)
        } else {
            None
        };
        // BOTH complete, bounded registrations and all requested bindings precede CAS.
        let before_record = registration(store, before, deadline)?;
        let after_record = registration(store, after, deadline)?;
        let before_scope = initial_scope(store, &before_record, deadline)?;
        let after_scope = initial_scope(store, &after_record, deadline)?;
        primary_continuity(&before_scope.0, &after_scope.0, options, deadline)?;
        let before = published_side(
            store,
            before_record,
            before_scope,
            &mut extra,
            &mut extra_error,
            deadline,
        )?;
        // Caller proof bytes are the exact typed envelope, independent of
        // physical native rows, source bytes and ledger capacity reserves.
        count_side(&before, options.max_input_bytes, deadline)?;
        let after = published_side(
            store,
            after_record,
            after_scope,
            &mut extra,
            &mut extra_error,
            deadline,
        )?;
        finish_pair(before, after, options, deadline)
        // Dropping only our own read-only transaction ends its read view.
        // An existing caller transaction is never committed or rolled back here.
    })();
    match result {
        Err(error) => Err(extra_error.unwrap_or_else(|| sanitized(error, deadline))),
        Ok(pair) => Ok(pair),
    }
}

fn validate_pin(pin: &SelectionPin) -> Result<()> {
    require(
        [
            &pin.workspace_id,
            &pin.snapshot_id,
            &pin.view_id,
            &pin.view_artifact_id,
            &pin.base_graph_hash,
        ]
        .into_iter()
        .all(|id| valid_id(id)),
        "comparison_invalid_pin",
    )
}

fn validate_options(options: &ComparisonOptions, deadline: &Deadline) -> Result<()> {
    deadline.check()?;
    // Every usize is a finite caller budget, including zero. Defaults are not
    // upper bounds. Required proof/source charges enforce their actual budgets.
    capacity(options.declared_file_continuity.len() <= 100)?;
    let mut before = BTreeSet::new();
    let mut after = BTreeSet::new();
    for item in &options.declared_file_continuity {
        deadline.check()?;
        require(
            item.before_path.len() <= 4096
                && item.after_path.len() <= 4096
                && crate::paths::normalized_syntax(&item.before_path)? == item.before_path
                && crate::paths::normalized_syntax(&item.after_path)? == item.after_path
                && valid_id(&item.before_hash)
                && valid_id(&item.after_hash)
                && before.insert(item.before_path.as_str())
                && after.insert(item.after_path.as_str()),
            "comparison_primary_file_binding_mismatch",
        )?;
    }
    Ok(())
}

fn primary_continuity(
    before: &Snapshot,
    after: &Snapshot,
    options: &ComparisonOptions,
    deadline: &Deadline,
) -> Result<()> {
    let mut before_files = BTreeMap::new();
    let mut after_files = BTreeMap::new();
    for file in &before.files {
        deadline.check()?;
        require(
            before_files.insert(file.path.as_str(), file).is_none(),
            "comparison_input_invalid",
        )?;
    }
    for file in &after.files {
        deadline.check()?;
        require(
            after_files.insert(file.path.as_str(), file).is_none(),
            "comparison_input_invalid",
        )?;
    }
    for item in &options.declared_file_continuity {
        deadline.check()?;
        require(
            before_files
                .get(item.before_path.as_str())
                .is_some_and(|file| file.hash == item.before_hash)
                && after_files
                    .get(item.after_path.as_str())
                    .is_some_and(|file| file.hash == item.after_hash),
            "comparison_primary_file_binding_mismatch",
        )?;
    }
    Ok(())
}

fn registration(store: &Store, pin: &SelectionPin, deadline: &Deadline) -> Result<super::Record> {
    deadline.check()?;
    let exists: bool = store.db.query_row(
        "SELECT EXISTS(SELECT 1 FROM records WHERE kind=?1 AND id=?2)",
        params![super::RECORD_KIND, &pin.view_id],
        |row| row.get(0),
    )?;
    require(exists, "comparison_registration_missing")?;
    let raw = super::record_value(store, super::RECORD_KIND, &pin.view_id, deadline)?;
    let record: super::Record = crate::json::decode(raw.clone())?;
    require(
        serde_json::to_value(&record)? == raw,
        "comparison_registration_invalid",
    )?;
    require(
        record.view_id == pin.view_id,
        "comparison_pin_mismatch view_id",
    )?;
    require(
        record.workspace_id == pin.workspace_id,
        "comparison_pin_mismatch workspace_id",
    )?;
    require(
        record.snapshot_id == pin.snapshot_id,
        "comparison_pin_mismatch snapshotId",
    )?;
    require(
        record.artifact_id == pin.view_artifact_id,
        "comparison_pin_mismatch viewArtifactId",
    )?;
    require(
        record.base_graph_hash == pin.base_graph_hash,
        "comparison_pin_mismatch baseGraphHash",
    )?;
    require(
        record
            .overlay_ids
            .iter()
            .chain(&record.meaning_graph_ids)
            .all(|id| valid_id(id)),
        "comparison_registration_invalid",
    )?;
    capacity(
        record
            .overlay_ids
            .len()
            .checked_add(record.meaning_graph_ids.len())
            .is_some_and(|count| count <= 32)
            && record.meaning_graph_ids.len() <= 29,
    )?;
    Ok(record)
}

fn initial_scope(
    store: &Store,
    record: &super::Record,
    deadline: &Deadline,
) -> Result<(Snapshot, Graph)> {
    let (snapshot, graph) = crate::meaning_build::archive_initial_scope(
        store,
        &record.snapshot_id,
        MAX_PROOF,
        deadline,
    )?;
    require(
        identity(&graph) == record.base_graph_hash,
        "comparison_input_invalid",
    )?;
    Ok((snapshot, graph))
}

fn published_side(
    store: &Store,
    record: super::Record,
    initial: (Snapshot, Graph),
    extra: &mut Option<&mut FrozenScopePreflight<'_>>,
    extra_error: &mut Option<anyhow::Error>,
    deadline: &Deadline,
) -> Result<CheckedComparisonSide<'static>> {
    // Keep every unchanged native/raw path on its independent fixed profile.
    let owned = if record.meaning_graph_ids.is_empty() {
        source_only_side(store, &record, initial, extra, extra_error, deadline)?
    } else {
        let mut seen = BTreeSet::new();
        let mut preflight = |scope: &Snapshot, d: &Deadline| {
            super::preflight_native_archive_scope(store, scope, d)?;
            require(seen.insert(scope.id.clone()), "comparison_input_invalid")?;
            if let Some(observer) = extra.as_deref_mut()
                && let Err(error) = observer(scope, d)
            {
                *extra_error = Some(error);
                return Err(failure("comparison_extra_preflight_denied"));
            }
            Ok(())
        };
        let verified = super::archive_meaning_view_by_id_guarded(
            store,
            &record.view_id,
            MAX_PROOF,
            MAX_SIDE_SOURCE_BYTES,
            &mut preflight,
            deadline,
        )?;
        let super::VerifiedArchivedMeaningView {
            snapshot,
            proof,
            source_projection,
            ..
        } = verified;
        let index = source_index(&snapshot, &proof, deadline)?;
        require(
            seen == index.scopes().map(|scope| scope.id.clone()).collect(),
            "comparison_input_invalid",
        )?;
        let source_objects = read_objects(store, &index, deadline)?;
        OwnedSide {
            snapshot,
            proof,
            projection: source_projection,
            source_objects,
        }
    };
    checked_side(SideData::Published(Box::new(owned)), deadline)
}

fn checked_side<'input>(
    data: SideData<'input>,
    deadline: &Deadline,
) -> Result<CheckedComparisonSide<'input>> {
    let (snapshot, proof) = match &data {
        SideData::Published(side) => (&side.snapshot, &side.proof),
        SideData::Archive(side) => (side.snapshot(), side.proof()),
    };
    let capture = proof
        .capture_scope
        .as_ref()
        .ok_or_else(|| failure("comparison_input_invalid"))?;
    let epoch = crate::capture::archived_analyzer_epoch(snapshot, capture, deadline)?
        .ok_or_else(|| failure("comparison_epoch_unsupported"))?;
    let normalized =
        crate::capture::archived_capture_options(snapshot, capture, deadline)?.validated()?;
    let index = source_index(snapshot, proof, deadline)?;
    // Preserve the existing per-side proof cap, independently of the pair envelope.
    count_serialized(proof, MAX_PROOF, deadline)?;
    Ok(CheckedComparisonSide {
        data,
        index,
        epoch,
        options: normalized,
    })
}

fn source_index(
    snapshot: &Snapshot,
    proof: &ArchiveProof,
    deadline: &Deadline,
) -> Result<FrozenSourceIndex> {
    // Capture-lineage snapshots are metadata, never a source grant. Exact scope
    // validation and real Store policies are separate prerequisites for loading.
    let mut capture_scopes = Vec::new();
    if let Some(capture) = &proof.capture_scope
        && let Some(nodes) = capture.get("nodes").and_then(Value::as_array)
    {
        capacity(nodes.len() <= 16)?;
        for node in nodes {
            deadline.check()?;
            let raw = node
                .get("snapshot")
                .ok_or_else(|| failure("comparison_input_invalid"))?;
            let scope: Snapshot = crate::json::decode(raw.clone())?;
            require(
                serde_json::to_value(&scope)? == *raw,
                "comparison_input_invalid",
            )?;
            capture_scopes.push(scope);
        }
    }
    FrozenSourceIndex::new(
        std::iter::once(snapshot)
            .chain(proof.source_scopes())
            .chain(&capture_scopes),
        MAX_SIDE_SOURCE_BYTES,
        deadline,
    )
}

fn read_objects(
    store: &Store,
    index: &FrozenSourceIndex,
    deadline: &Deadline,
) -> Result<BTreeMap<String, String>> {
    // Admit every scope/membership before the first copy's source read. A content
    // hash deduplicates reads only after its complete current scope was admitted.
    let policies = index
        .scopes()
        .map(|scope| {
            deadline.check()?;
            let policy = crate::capture::source_scope_policy(store, scope)?;
            for file in &scope.files {
                require(policy.file(&file.path)? == file, "comparison_input_invalid")?;
            }
            Ok((scope.id.as_str(), policy))
        })
        .collect::<Result<BTreeMap<_, _>>>()?;
    let mut objects = BTreeMap::new();
    for scope in index.scopes() {
        let bound = index.bind(scope, deadline)?;
        let policy = policies
            .get(scope.id.as_str())
            .ok_or_else(|| failure("comparison_input_invalid"))?;
        for member in bound.members() {
            deadline.check()?;
            let file = member.file();
            if let std::collections::btree_map::Entry::Vacant(entry) =
                objects.entry(file.hash.clone())
            {
                let bytes = policy.read_source(&file.path)?;
                require(
                    bytes.len() == file.size && hash(&bytes) == file.hash,
                    "comparison_input_invalid",
                )?;
                entry.insert(String::from_utf8(bytes)?);
            }
        }
    }
    Ok(objects)
}

fn finish_pair<'input>(
    before: CheckedComparisonSide<'input>,
    after: CheckedComparisonSide<'input>,
    options: &ComparisonOptions,
    deadline: &Deadline,
) -> Result<CheckedComparisonPair<'input>> {
    require(
        before.primary_snapshot().workspace_id == after.primary_snapshot().workspace_id,
        "comparison_workspace_mismatch",
    )?;
    require(before.epoch == after.epoch, "comparison_epoch_mismatch")?;
    require(
        before.options == after.options,
        "comparison_policy_mismatch",
    )?;
    let before_proof_bytes = count_side(&before, options.max_input_bytes, deadline)?;
    let after_proof_bytes = count_side(
        &after,
        options.max_input_bytes.saturating_sub(before_proof_bytes),
        deadline,
    )?;
    let total_proof_bytes = before_proof_bytes
        .checked_add(after_proof_bytes)
        .ok_or_else(|| failure("comparison_capacity_exceeded"))?;
    let mut union = BTreeMap::<&str, &str>::new();
    let mut distinct_source_bytes = 0usize;
    for side in [&before, &after] {
        let objects = side.objects();
        require(
            objects.len() == side.index.unique_files().count(),
            "comparison_input_invalid",
        )?;
        for file in side.index.unique_files() {
            deadline.check()?;
            let text = objects
                .get(&file.hash)
                .ok_or_else(|| failure("comparison_input_invalid"))?;
            require(text.len() == file.size, "comparison_input_invalid")?;
            match union.entry(file.hash.as_str()) {
                std::collections::btree_map::Entry::Occupied(prior) => {
                    require(*prior.get() == text.as_str(), "comparison_input_invalid")?;
                }
                std::collections::btree_map::Entry::Vacant(entry) => {
                    require(
                        hash(text.as_bytes()) == file.hash,
                        "comparison_input_invalid",
                    )?;
                    distinct_source_bytes = distinct_source_bytes
                        .checked_add(text.len())
                        .ok_or_else(|| failure("comparison_capacity_exceeded"))?;
                    capacity(distinct_source_bytes <= options.max_source_bytes)?;
                    entry.insert(text);
                }
            }
        }
    }
    let accounting = ComparisonInputAccounting {
        before_proof_bytes,
        after_proof_bytes,
        total_proof_bytes,
        distinct_source_bytes,
        distinct_source_objects: union.len(),
    };
    drop(union);
    deadline.check()?;
    Ok(CheckedComparisonPair {
        before,
        after,
        accounting,
    })
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CountedSideProof<'a> {
    profile: &'static str,
    snapshot: &'a Snapshot,
    archive: &'a ArchiveProof,
}
fn count_side(
    side: &CheckedComparisonSide<'_>,
    maximum: usize,
    deadline: &Deadline,
) -> Result<usize> {
    count_serialized(
        &CountedSideProof {
            profile: PROFILE,
            snapshot: side.primary_snapshot(),
            archive: side.proof(),
        },
        maximum,
        deadline,
    )
}
fn count_serialized<T: Serialize + ?Sized>(
    value: &T,
    maximum: usize,
    deadline: &Deadline,
) -> Result<usize> {
    struct Counter<'a> {
        size: usize,
        maximum: usize,
        deadline: &'a Deadline,
        overflow: bool,
    }
    impl Write for Counter<'_> {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.deadline.check().map_err(std::io::Error::other)?;
            if bytes.len() > self.maximum.saturating_sub(self.size) {
                self.overflow = true;
                return Err(std::io::Error::other("comparison_capacity_exceeded"));
            }
            self.size += bytes.len();
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            self.deadline.check().map_err(std::io::Error::other)
        }
    }
    let mut counter = Counter {
        size: 0,
        maximum,
        deadline,
        overflow: false,
    };
    if let Err(error) = serde_json::to_writer(&mut counter, value) {
        deadline.check()?;
        if counter.overflow {
            return Err(failure("comparison_capacity_exceeded"));
        }
        return Err(error.into());
    }
    deadline.check()?;
    Ok(counter.size)
}

impl<'input> CheckedComparisonPair<'input> {
    pub(crate) fn before(&self) -> &CheckedComparisonSide<'input> {
        &self.before
    }
    pub(crate) fn after(&self) -> &CheckedComparisonSide<'input> {
        &self.after
    }
    pub(crate) fn accounting(&self) -> &ComparisonInputAccounting {
        &self.accounting
    }
}
impl CheckedComparisonSide<'_> {
    fn proof(&self) -> &ArchiveProof {
        match &self.data {
            SideData::Published(side) => &side.proof,
            SideData::Archive(side) => side.proof(),
        }
    }
    fn objects(&self) -> &BTreeMap<String, String> {
        match &self.data {
            SideData::Published(side) => &side.source_objects,
            SideData::Archive(side) => side.source_objects(),
        }
    }
    pub(crate) fn view(&self) -> &View {
        &self.proof().view
    }
    pub(crate) fn primary_snapshot(&self) -> &Snapshot {
        match &self.data {
            SideData::Published(side) => &side.snapshot,
            SideData::Archive(side) => side.snapshot(),
        }
    }
    pub(crate) fn canonical_graph(&self) -> &Graph {
        &self.proof().canonical_graph
    }
    pub(crate) fn source_projection(&self) -> &Graph {
        match &self.data {
            SideData::Published(side) => &side.projection,
            SideData::Archive(side) => side.source_projection(),
        }
    }
    pub(crate) fn analyzer_epoch(&self) -> AnalyzerEpoch {
        self.epoch
    }
    pub(crate) fn primary_capture_options(&self) -> &CaptureOptions {
        &self.options
    }
    pub(crate) fn source_scopes(&self) -> impl Iterator<Item = &Snapshot> {
        self.index.scopes()
    }
    pub(crate) fn bind_scope(
        &self,
        snapshot: &Snapshot,
        deadline: &Deadline,
    ) -> Result<BoundComparisonScope<'_>> {
        let bound = self
            .index
            .bind(snapshot, deadline)
            .map_err(|error| sanitized(error, deadline))?;
        Ok(BoundComparisonScope {
            bound,
            source_objects: self.objects(),
        })
    }
    pub(crate) fn bind_named_scope(
        &self,
        id: &str,
        deadline: &Deadline,
    ) -> Result<BoundComparisonScope<'_>> {
        let bound = self
            .index
            .scope_named(id, deadline)
            .map_err(|error| sanitized(error, deadline))?;
        Ok(BoundComparisonScope {
            bound,
            source_objects: self.objects(),
        })
    }
}
impl ComparisonInputAccounting {
    #[cfg_attr(
        not(test),
        allow(
            dead_code,
            reason = "Retained internal audit accounting API; exercised by precise input tests."
        )
    )]
    pub(crate) fn before_proof_bytes(&self) -> usize {
        self.before_proof_bytes
    }
    #[cfg_attr(
        not(test),
        allow(
            dead_code,
            reason = "Retained internal audit accounting API; exercised by precise input tests."
        )
    )]
    pub(crate) fn after_proof_bytes(&self) -> usize {
        self.after_proof_bytes
    }
    pub(crate) fn total_proof_bytes(&self) -> usize {
        self.total_proof_bytes
    }
    pub(crate) fn distinct_source_bytes(&self) -> usize {
        self.distinct_source_bytes
    }
    #[cfg_attr(
        not(test),
        allow(
            dead_code,
            reason = "Retained internal audit accounting API; exercised by precise input tests."
        )
    )]
    pub(crate) fn distinct_source_objects(&self) -> usize {
        self.distinct_source_objects
    }
}
impl BoundComparisonScope<'_> {
    pub(crate) fn snapshot(&self) -> &Snapshot {
        self.bound.snapshot()
    }
    pub(crate) fn source_named<'scope>(
        &'scope self,
        path: &str,
        deadline: &Deadline,
    ) -> Result<ComparisonSource<'scope>> {
        let member = self
            .bound
            .member_named(path, deadline)
            .map_err(|error| sanitized(error, deadline))?;
        let text = self
            .source_objects
            .get(&member.file().hash)
            .ok_or_else(|| failure("comparison_input_invalid"))?;
        Ok(ComparisonSource { member, text })
    }
    pub(crate) fn source<'scope>(
        &'scope self,
        file: &SourceFile,
        deadline: &Deadline,
    ) -> Result<ComparisonSource<'scope>> {
        let member = self
            .bound
            .member(file, deadline)
            .map_err(|error| sanitized(error, deadline))?;
        let text = self
            .source_objects
            .get(&member.file().hash)
            .ok_or_else(|| failure("comparison_input_invalid"))?;
        Ok(ComparisonSource { member, text })
    }
}
impl<'scope> ComparisonSource<'scope> {
    pub(crate) fn member(&self) -> &FrozenSourceMember {
        &self.member
    }
    pub(crate) fn text(&self) -> &'scope str {
        self.text
    }
}

fn source_only_side(
    store: &Store,
    record: &super::Record,
    initial: (Snapshot, Graph),
    extra: &mut Option<&mut FrozenScopePreflight<'_>>,
    extra_error: &mut Option<anyhow::Error>,
    deadline: &Deadline,
) -> Result<OwnedSide> {
    let (snapshot, canonical_graph) = initial;
    // Native authority routines below use legacy Store row access. Admit every
    // row they can reach first, in this SAME retained SQL read view. Discovery
    // is not authority: original capture/replay validators still run afterward.
    let scopes = admit_source_lineage(store, &snapshot, deadline)?;
    let index = FrozenSourceIndex::new(&scopes, MAX_SIDE_SOURCE_BYTES, deadline)?;
    for scope in index.scopes() {
        deadline.check()?;
        let policy = crate::capture::source_scope_policy(store, scope)?;
        for file in &scope.files {
            require(policy.file(&file.path)? == file, "comparison_input_invalid")?;
        }
        if let Some(observer) = extra.as_deref_mut()
            && let Err(error) = observer(scope, deadline)
        {
            *extra_error = Some(error);
            return Err(failure("comparison_extra_preflight_denied"));
        }
    }
    let mut selected_bytes = 0usize;
    capacity(registered_artifact_size(store, &record.artifact_id, deadline)? <= MAX_PROOF)?;
    for id in &record.overlay_ids {
        selected_bytes = selected_bytes
            .checked_add(registered_artifact_size(store, id, deadline)?)
            .ok_or_else(|| failure("comparison_capacity_exceeded"))?;
        capacity(selected_bytes <= MAX_PROOF)?;
    }
    // Complete scope policies above precede every source/View/selected CAS open.
    let bytes = super::artifact_limit(store, &record.artifact_id, MAX_PROOF, deadline)?;
    let raw = crate::json::parse(&bytes, MAX_PROOF)?;
    let mut view: View = crate::json::decode(raw.clone())?;
    require(
        serde_json::to_value(&view)? == raw
            && view.artifact_id.is_empty()
            && serde_json::to_vec(&view)? == bytes
            && view.meaning.is_none(),
        "comparison_input_invalid",
    )?;
    view.artifact_id = record.artifact_id.clone();
    require(
        view.id == record.view_id
            && view.snapshot_id == record.snapshot_id
            && view.workspace_id == record.workspace_id
            && view.base_graph_hash == record.base_graph_hash
            && view
                .overlays
                .iter()
                .map(|row| row["artifactId"].as_str().unwrap_or("").to_owned())
                .collect::<Vec<_>>()
                == record.overlay_ids,
        "comparison_registration_invalid",
    )?;
    let mut inputs = Vec::new();
    for id in &record.overlay_ids {
        let payload = super::artifact_limit(store, id, MAX_PROOF, deadline)?;
        inputs.push(super::ArchivedInput {
            artifact_id: id.clone(),
            payload: String::from_utf8(payload)?,
        });
    }
    let source_objects = read_objects(store, &index, deadline)?;
    let capture_scope = crate::capture::archived_capture_scope(store, &snapshot, deadline)?;
    let proof = ArchiveProof {
        view,
        canonical_graph,
        inputs,
        capture_scope: Some(capture_scope),
        meaning_archive: None,
    };
    count_serialized(&proof, MAX_PROOF, deadline)?;
    let replay_index = source_index(&snapshot, &proof, deadline)?;
    require(
        index.scope_count() == replay_index.scope_count(),
        "comparison_input_invalid",
    )?;
    for scope in index.scopes() {
        replay_index.bind(scope, deadline)?;
    }
    let mut texts = BTreeMap::new();
    for file in &snapshot.files {
        let text = source_objects
            .get(&file.hash)
            .ok_or_else(|| failure("comparison_input_invalid"))?;
        texts.insert(file.path.clone(), text.clone());
    }
    let (projection, epoch) = super::validate_archive(&snapshot, &proof, &texts, deadline)?;
    require(epoch.is_some(), "comparison_epoch_unsupported")?;
    // The legacy source archive consumer checks exact rows; comparison additionally
    // binds the ENTIRE original View (including complete and every descriptor).
    // Reuse its existing generators rather than inventing another row validator.
    let mut sources = BTreeMap::new();
    for file in &snapshot.files {
        let text = texts
            .get(&file.path)
            .ok_or_else(|| failure("comparison_input_invalid"))?
            .clone();
        sources.insert(
            file.path.clone(),
            super::Source {
                lines: crate::paths::line_starts(&text),
                text,
                hash: file.hash.clone(),
            },
        );
    }
    let mut expected =
        super::canonical_view(&snapshot, &proof.canonical_graph, &sources, deadline)?;
    super::replay_archived_inputs(&snapshot, &proof, &sources, &mut expected, deadline)?;
    super::validate_contents(&expected, &snapshot, &sources, deadline)?;
    expected.id = identity(&expected);
    expected.artifact_id = hash(crate::json::encode_bounded(&expected, MAX_PROOF, deadline)?);
    require(
        serde_json::to_value(&expected)? == serde_json::to_value(&proof.view)?,
        "comparison_input_invalid",
    )?;
    Ok(OwnedSide {
        snapshot,
        proof,
        projection,
        source_objects,
    })
}

fn registered_artifact_size(store: &Store, id: &str, deadline: &Deadline) -> Result<usize> {
    let raw = super::record_value(store, "artifact", id, deadline)?;
    super::fields(&raw, &["artifactId", "totalBytes"])?;
    require(raw["artifactId"] == id, "comparison_input_invalid")?;
    super::number(&raw, "totalBytes")
}

struct MetadataBudget {
    bytes: usize,
    keys: usize,
}
impl MetadataBudget {
    fn charge(&mut self, bytes: usize) -> Result<()> {
        self.bytes = self
            .bytes
            .checked_add(bytes)
            .ok_or_else(|| failure("comparison_capacity_exceeded"))?;
        self.keys = self
            .keys
            .checked_add(1)
            .ok_or_else(|| failure("comparison_capacity_exceeded"))?;
        capacity(self.bytes <= MAX_PROOF && self.keys <= 4096)
    }
    fn remaining(&self) -> usize {
        MAX_PROOF.saturating_sub(self.bytes)
    }
}

fn bounded_capture_row(
    store: &Store,
    table: &'static str,
    kind: &str,
    id: &str,
    maximum: usize,
    budget: &mut MetadataBudget,
    deadline: &Deadline,
) -> Result<Option<Value>> {
    deadline.check()?;
    require(valid_id(id), "comparison_input_invalid")?;
    let size: Option<usize> = match table {
        "snapshots" => store
            .db
            .query_row(
                "SELECT length(CAST(value AS BLOB)) FROM snapshots WHERE id=?1",
                [id],
                |row| row.get(0),
            )
            .optional()?,
        "records" => store
            .db
            .query_row(
                "SELECT length(CAST(value AS BLOB)) FROM records WHERE kind=?1 AND id=?2",
                params![kind, id],
                |row| row.get(0),
            )
            .optional()?,
        _ => return Err(failure("comparison_input_invalid")),
    };
    let Some(size) = size else { return Ok(None) };
    capacity(size <= maximum && size <= budget.remaining())?;
    budget.charge(size)?;
    let raw: String = match table {
        "snapshots" => store.db.query_row(
            "SELECT value FROM snapshots WHERE id=?1 AND length(CAST(value AS BLOB))<=?2",
            params![id, size],
            |row| row.get(0),
        )?,
        "records" => store.db.query_row(
            "SELECT value FROM records WHERE kind=?1 AND id=?2 AND length(CAST(value AS BLOB))<=?3",
            params![kind, id, size],
            |row| row.get(0),
        )?,
        _ => return Err(failure("comparison_input_invalid")),
    };
    require(raw.len() == size, "comparison_input_invalid")?;
    Ok(Some(crate::json::parse(raw.as_bytes(), maximum)?))
}

fn admit_source_lineage(
    store: &Store,
    primary: &Snapshot,
    deadline: &Deadline,
) -> Result<Vec<Snapshot>> {
    let mut budget = MetadataBudget { bytes: 0, keys: 0 };
    let graph_size: usize = store.db.query_row(
        "SELECT length(CAST(value AS BLOB)) FROM graphs WHERE id=?1",
        [&primary.id],
        |row| row.get(0),
    )?;
    budget.charge(graph_size)?; // The already-admitted primary Graph shares this metadata cap.
    let mut pending = VecDeque::from([primary.id.clone()]);
    let mut visited = BTreeSet::new();
    let mut scopes = Vec::new();
    while let Some(id) = pending.pop_front() {
        deadline.check()?;
        if !visited.insert(id.clone()) {
            continue;
        }
        capacity(visited.len() <= 16)?;
        let raw = bounded_capture_row(
            store,
            "snapshots",
            "",
            &id,
            MAX_PROOF,
            &mut budget,
            deadline,
        )?
        .ok_or_else(|| failure("comparison_input_invalid"))?;
        let scope: Snapshot = crate::json::decode(raw.clone())?;
        require(
            serde_json::to_value(&scope)? == raw
                && scope.id == id
                && scope.workspace_id == store.workspace_id
                && matches!(scope.kind.as_str(), "disk" | "derived" | "overlay"),
            "comparison_input_invalid",
        )?;
        let mut derived = false;
        let mut overlay = false;
        for kind in [
            crate::capture::OPTIONS_RECORD_KIND,
            crate::capture::IGNORED_RECORD_KIND,
            crate::capture::DERIVED_OPTIONS_RECORD_KIND,
            crate::git_workflows::SOURCE_KIND,
            "overlay-snapshot",
        ] {
            let Some(row) = bounded_capture_row(
                store,
                "records",
                kind,
                &id,
                16_000_000,
                &mut budget,
                deadline,
            )?
            else {
                continue;
            };
            let origin = if kind == crate::capture::DERIVED_OPTIONS_RECORD_KIND {
                derived = true;
                Some("originSnapshotId")
            } else if kind == "overlay-snapshot" {
                overlay = true;
                Some("baseSnapshotId")
            } else if kind == crate::git_workflows::SOURCE_KIND {
                let commit = row["commitArtifactId"]
                    .as_str()
                    .ok_or_else(|| failure("comparison_input_invalid"))?;
                // Git's original pure archive validator has its own 1MB commit cap.
                // Admit the exact registration before its existing bounded CAS reader.
                let registered = bounded_capture_row(
                    store,
                    "records",
                    "artifact",
                    commit,
                    64_000,
                    &mut budget,
                    deadline,
                )?
                .ok_or_else(|| failure("comparison_input_invalid"))?;
                super::fields(&registered, &["artifactId", "totalBytes"])?;
                require(
                    registered["artifactId"] == commit,
                    "comparison_input_invalid",
                )?;
                capacity(super::number(&registered, "totalBytes")? <= 1_000_000)?;
                Some("originSnapshotId")
            } else {
                None
            };
            if let Some(field) = origin {
                let origin = row[field]
                    .as_str()
                    .ok_or_else(|| failure("comparison_input_invalid"))?;
                require(valid_id(origin), "comparison_input_invalid")?;
                pending.push_back(origin.to_owned());
            }
        }
        require(
            (scope.kind != "derived" || derived) && (scope.kind != "overlay" || overlay),
            "comparison_input_invalid",
        )?;
        if scope.id == primary.id {
            require(
                serde_json::to_value(&scope)? == serde_json::to_value(primary)?,
                "comparison_input_invalid",
            )?;
        }
        scopes.push(scope);
    }
    Ok(scopes)
}
