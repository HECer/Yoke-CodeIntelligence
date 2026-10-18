//! Unpersisted Git impact explanations derived only from verified publications.
use crate::{
    capture::{CapturedSourceScope, Deadline},
    model::{Graph, Snapshot, valid_id},
    store::Store,
};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize, Serializer, ser::SerializeMap};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
    io::{self, Write},
    sync::Arc,
};

#[path = "git_impact_candidates.rs"]
mod candidates;
#[cfg(test)]
#[path = "git_affected_admission_tests.rs"]
mod checked_admission_tests;
#[path = "git_impact_quotes.rs"]
mod quotes;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DetailOptions {
    pub max_candidate_pairs: usize,
    pub max_source_bytes: usize,
    pub max_similarity_lines: usize,
    pub max_similarity_work: usize,
    pub max_rows: usize,
    pub max_output_bytes: usize,
    pub max_quote_bytes: usize,
    pub max_total_quote_bytes: usize,
}
impl Default for DetailOptions {
    fn default() -> Self {
        Self {
            max_candidate_pairs: 100_000,
            max_source_bytes: 128_000_000,
            max_similarity_lines: 200_000,
            max_similarity_work: 10_000_000,
            max_rows: 200_000,
            max_output_bytes: 4_000_000,
            max_quote_bytes: 16_384,
            max_total_quote_bytes: 1_000_000,
        }
    }
}

#[derive(Clone, Debug)]
pub struct ImpactDetails {
    value: Value,
    canonical_bytes: Vec<u8>,
}
impl ImpactDetails {
    pub fn value(&self) -> &Value {
        &self.value
    }
    pub fn canonical_bytes(&self) -> &[u8] {
        &self.canonical_bytes
    }
}
impl Serialize for ImpactDetails {
    fn serialize<S: Serializer>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error> {
        self.value.serialize(serializer)
    }
}

#[derive(Debug)]
struct Capacity;
impl fmt::Display for Capacity {
    fn fmt(&self, output: &mut fmt::Formatter<'_>) -> fmt::Result {
        output.write_str("git_impact_details_capacity_exceeded")
    }
}
impl std::error::Error for Capacity {}
fn capacity() -> anyhow::Error {
    Capacity.into()
}

fn public_error(error: anyhow::Error, deadline: &Deadline) -> anyhow::Error {
    let message = if deadline.check().is_err() {
        "git_impact_details_deadline_exceeded"
    } else if is_checked_input_capacity(&error) {
        "git_impact_details_capacity_exceeded"
    } else {
        "git_impact_details_input_invalid"
    };
    anyhow::anyhow!(message)
}

pub(super) fn is_checked_input_capacity(error: &anyhow::Error) -> bool {
    error.is::<Capacity>() || error.chain().any(|cause| {
        cause.downcast_ref::<rusqlite::Error>().is_some_and(|error| matches!(error,
            rusqlite::Error::SqliteFailure(code,_) if code.extended_code == rusqlite::ffi::SQLITE_TOOBIG))
    }) || error.chain().any(|cause| matches!(cause.to_string().as_str(),
        "Git report collection capacity exceeded"|"Git hunk capacity exceeded"|
        "Git record byte capacity exceeded"|"Git CAS identity/capacity exceeded"|
        "Derived graph capacity exceeded"|"Source validation capacity exceeded"|
        "Native diff line capacity exceeded"|"Git impact node capacity exceeded"))
}

struct Counter<'a> {
    count: usize,
    maximum: usize,
    deadline: &'a Deadline,
    refused: bool,
}
impl Write for Counter<'_> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if self.deadline.check().is_err() {
            return Err(io::Error::other("deadline"));
        }
        let next = self.count.checked_add(bytes.len());
        if next.is_none_or(|next| next > self.maximum) {
            self.refused = true;
            return Err(io::Error::other("capacity"));
        }
        self.count = next.unwrap();
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
fn encoded_len<T: Serialize + ?Sized>(
    value: &T,
    maximum: usize,
    deadline: &Deadline,
) -> Result<usize> {
    deadline.check()?;
    let mut counter = Counter {
        count: 0,
        maximum,
        deadline,
        refused: false,
    };
    let result = serde_json::to_writer(&mut counter, value);
    if counter.refused {
        return Err(capacity());
    }
    result?;
    deadline.check()?;
    Ok(counter.count)
}

/// A cumulative reservation of disjoint fragments, never a substitute for the
/// final exact canonical wire count. No caller-sized buffer is allocated.
struct Budget {
    rows: usize,
    maximum_rows: usize,
    reserved: usize,
    maximum_wire: usize,
    deadline: Deadline,
}
impl Budget {
    fn new(options: &DetailOptions, deadline: &Deadline) -> Self {
        Self {
            rows: 0,
            maximum_rows: options.max_rows,
            reserved: 0,
            maximum_wire: options.max_output_bytes,
            deadline: deadline.clone(),
        }
    }
    fn charge(&mut self, count: usize) -> Result<()> {
        self.deadline.check()?;
        let next = self.rows.checked_add(count).ok_or_else(capacity)?;
        if next > self.maximum_rows {
            return Err(capacity());
        }
        self.rows = next;
        Ok(())
    }
    fn reserve_serialized<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<()> {
        let remaining = self
            .maximum_wire
            .checked_sub(self.reserved)
            .ok_or_else(capacity)?;
        let count = encoded_len(value, remaining, &self.deadline)?;
        self.reserved = self.reserved.checked_add(count).ok_or_else(capacity)?;
        Ok(())
    }
    fn charge_value(&mut self, value: &Value) -> Result<()> {
        self.deadline.check()?;
        match value {
            Value::Array(items) => {
                self.charge(items.len())?;
                for item in items {
                    self.charge_value(item)?;
                }
            }
            Value::Object(fields) => {
                for item in fields.values() {
                    self.charge_value(item)?;
                }
            }
            _ => {}
        }
        Ok(())
    }
    fn clone_value(&mut self, value: &Value) -> Result<Value> {
        self.charge_value(value)?;
        self.reserve_serialized(value)?;
        self.deadline.check()?;
        let cloned = value.clone();
        self.deadline.check()?;
        Ok(cloned)
    }
}

struct HashWriter<'a> {
    hasher: Sha256,
    deadline: &'a Deadline,
}
impl Write for HashWriter<'_> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if self.deadline.check().is_err() {
            return Err(io::Error::other("deadline"));
        }
        self.hasher.update(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
fn digest_serialized<T: Serialize + ?Sized>(value: &T, deadline: &Deadline) -> Result<String> {
    deadline.check()?;
    let mut writer = HashWriter {
        hasher: Sha256::new(),
        deadline,
    };
    {
        let mut buffered = io::BufWriter::with_capacity(16_384, &mut writer);
        serde_json::to_writer(&mut buffered, value)?;
        buffered.flush()?;
    }
    deadline.check()?;
    Ok(hex::encode(writer.hasher.finalize()))
}

/// Fixed raw profiles belong to metadata admission, independently of the
/// caller's selected-source, logical-line, row, quote and canonical-wire limits.
struct MetadataEnvelope<'a> {
    database: &'a rusqlite::Connection,
    prior: i32,
}
impl<'a> MetadataEnvelope<'a> {
    fn enter(store: &'a Store) -> Result<Self> {
        let limit = rusqlite::limits::Limit::SQLITE_LIMIT_LENGTH;
        let cap = store.db.limit(limit)?.min(64_004_096);
        let prior = store.db.set_limit(limit, cap)?;
        Ok(Self {
            database: &store.db,
            prior,
        })
    }
}
impl Drop for MetadataEnvelope<'_> {
    fn drop(&mut self) {
        // Restoring a previously returned nonnegative connection limit requires
        // no SQL, allocation, permission or transaction operation.
        let _ = self
            .database
            .set_limit(rusqlite::limits::Limit::SQLITE_LIMIT_LENGTH, self.prior);
    }
}
fn metadata_cell<T>(
    store: &Store,
    table: &str,
    id: &str,
    maximum: usize,
    read: impl FnOnce() -> Result<T>,
) -> Result<T> {
    ensure!(
        ["snapshots", "graphs"].contains(&table) && valid_id(id),
        "Invalid metadata lookup"
    );
    let limit = rusqlite::limits::Limit::SQLITE_LIMIT_LENGTH;
    let cap = store.db.limit(limit)?.min(i32::try_from(
        maximum.checked_add(4096).ok_or_else(capacity)?,
    )?);
    let prior = store.db.set_limit(limit, cap)?;
    let result = (|| -> Result<T> {
        let size: usize = store.db.query_row(
            &format!("SELECT length(CAST(value AS BLOB)) FROM {table} WHERE id=?1"),
            [id],
            |row| row.get(0),
        )?;
        if size > maximum {
            return Err(capacity());
        }
        read()
    })();
    store.db.set_limit(limit, prior)?;
    result
}
fn bound_metadata(
    store: &Store,
    id: &str,
    deadline: &Deadline,
) -> Result<(super::ReportData, Snapshot, Snapshot)> {
    let (report, base, head, _, _) = bound_metadata_graphs(store, id, deadline)?;
    Ok((report, base, head))
}

fn bound_metadata_graphs(
    store: &Store,
    id: &str,
    deadline: &Deadline,
) -> Result<(super::ReportData, Snapshot, Snapshot, Graph, Graph)> {
    deadline.check()?;
    ensure!(valid_id(id), "Invalid report ID");
    let record: super::WorkflowRecord = crate::json::decode(super::record_value(
        store,
        super::REPORT_KIND,
        id,
        64_000,
        deadline,
    )?)?;
    ensure!(
        record.artifact_id == id
            && record.workspace_id == store.workspace_id
            && record.producer == "yoke-native-git-workflow-1",
        "Report publication mismatch"
    );
    let raw = super::artifact_bytes(store, id, 16_000_000, deadline)?;
    let report: super::ReportData = crate::json::decode(crate::json::parse(&raw, 16_000_000)?)?;
    ensure!(
        report.schema_version == 1
            && !report.complete
            && report.workspace_id == record.workspace_id
            && report.base_snapshot_id == record.base_snapshot_id
            && report.head_snapshot_id == record.head_snapshot_id
            && report.origin_snapshot_id == record.origin_snapshot_id
            && report.base_graph_hash == record.base_graph_hash
            && report.head_graph_hash == record.head_graph_hash
            && super::oid_valid(&report.base_commit)
            && super::oid_valid(&report.head_commit),
        "Report metadata binding mismatch"
    );
    for id in [
        &report.base_snapshot_id,
        &report.head_snapshot_id,
        &report.origin_snapshot_id,
        &report.base_graph_hash,
        &report.head_graph_hash,
    ] {
        ensure!(valid_id(id), "Invalid metadata identity");
    }
    let base = metadata_cell(
        store,
        "snapshots",
        &report.base_snapshot_id,
        32_000_000,
        || store.snapshot(&report.base_snapshot_id),
    )?;
    let head = metadata_cell(
        store,
        "snapshots",
        &report.head_snapshot_id,
        32_000_000,
        || store.snapshot(&report.head_snapshot_id),
    )?;
    let base_graph: Graph = metadata_cell(store, "graphs", &base.id, 64_000_000, || {
        store.graph(&base.id)
    })?;
    let head_graph: Graph = metadata_cell(store, "graphs", &head.id, 64_000_000, || {
        store.graph(&head.id)
    })?;
    ensure!(base.kind == "derived", "Git base metadata kind mismatch");
    match report.head_mode.as_str() {
        "commit" => ensure!(head.kind == "derived", "Git head metadata kind mismatch"),
        "captured_working_tree" => ensure!(
            head.kind == "disk" && head.id == report.origin_snapshot_id,
            "Git working metadata kind mismatch"
        ),
        _ => anyhow::bail!("Unsupported Git head metadata mode"),
    }
    ensure!(
        digest_serialized(&base_graph, deadline)? == report.base_graph_hash
            && digest_serialized(&head_graph, deadline)? == report.head_graph_hash,
        "Graph metadata binding mismatch"
    );
    // This is only a necessary ordering gate. The real released load below must
    // validate every receipt, source byte, anchor and reconstructed report row.
    Ok((report, base, head, base_graph, head_graph))
}
fn admit_scope(
    scope: &CapturedSourceScope<'_>,
    snapshot: &Snapshot,
    deadline: &Deadline,
) -> Result<()> {
    if snapshot.files.len() > 20_000 {
        return Err(capacity());
    }
    let mut seen = BTreeSet::new();
    for file in &snapshot.files {
        deadline.check()?;
        ensure!(
            seen.insert(file.path.as_str()) && valid_id(&file.hash) && file.size <= 4_000_000,
            "Invalid source member"
        );
        let member = scope.file(&file.path)?;
        ensure!(member == file, "Captured source member mismatch");
    }
    Ok(())
}

/// Lower caller limits supplement the unchanged raw metadata and loader bounds.
pub(super) struct GraphInputLimits {
    entities: usize,
    observations: usize,
    evidence: usize,
}
impl GraphInputLimits {
    pub(super) fn new(entities: usize, observations: usize, evidence: usize) -> Result<Self> {
        ensure!(
            entities <= 5_000 && observations <= 50_000 && evidence <= 100_000,
            "Invalid checked graph limits"
        );
        Ok(Self {
            entities,
            observations,
            evidence,
        })
    }
    fn admit(&self, graph: &Graph) -> Result<()> {
        if graph.entities.len() > self.entities
            || graph.observations.len() > self.observations
            || graph.evidence.len() > self.evidence
        {
            return Err(capacity());
        }
        Ok(())
    }
}

/// Only this module can construct these views from a retained, verified report.
pub(super) struct CheckedGitInputs<'view> {
    report_artifact_id: &'view str,
    report: &'view super::ReportData,
    base: CheckedGitSide<'view>,
    head: CheckedGitSide<'view>,
}
impl<'view> CheckedGitInputs<'view> {
    pub(super) fn report_artifact_id(&self) -> &str {
        self.report_artifact_id
    }
    pub(super) fn workspace_id(&self) -> &str {
        &self.report.workspace_id
    }
    pub(super) fn changed_symbols(&self) -> &[Value] {
        &self.report.changed_symbols
    }
    pub(super) fn impact(&self) -> &[Value] {
        &self.report.impact
    }
    pub(super) fn impact_observations(&self) -> &[Value] {
        &self.report.impact_observations
    }
    pub(super) fn base(&self) -> &CheckedGitSide<'view> {
        &self.base
    }
    pub(super) fn head(&self) -> &CheckedGitSide<'view> {
        &self.head
    }
}

pub(super) struct CheckedGitSide<'view> {
    snapshot: &'view Snapshot,
    graph: &'view Graph,
    graph_hash: &'view str,
    analyzer_epoch: crate::analysis::AnalyzerEpoch,
    scope: &'view CapturedSourceScope<'view>,
}
impl CheckedGitSide<'_> {
    pub(super) fn snapshot(&self) -> &Snapshot {
        self.snapshot
    }
    pub(super) fn graph(&self) -> &Graph {
        self.graph
    }
    pub(super) fn graph_hash(&self) -> &str {
        self.graph_hash
    }
    pub(super) fn analyzer_epoch(&self) -> crate::analysis::AnalyzerEpoch {
        self.analyzer_epoch
    }
    pub(super) fn file(&self, path: &str) -> Result<&crate::model::SourceFile> {
        self.scope.file(path)
    }
    pub(super) fn read_source(&self, path: &str) -> Result<Vec<u8>> {
        self.scope.read_source(path)
    }
}

/// Keep the source scopes, SQLite envelope and one read view through final encoding.
pub(super) fn with_checked_report_inputs<R>(
    store: &Store,
    report_id: &str,
    limits: GraphInputLimits,
    deadline: &Deadline,
    consume: impl for<'view> FnOnce(CheckedGitInputs<'view>) -> Result<R>,
) -> Result<R> {
    deadline.check()?;
    ensure!(valid_id(report_id), "Invalid report ID");
    store.set_deadline(deadline)?;
    let _metadata_envelope = MetadataEnvelope::enter(store)?;
    let _read = if store.db.is_autocommit() {
        Some(rusqlite::Transaction::new_unchecked(
            &store.db,
            rusqlite::TransactionBehavior::Deferred,
        )?)
    } else {
        None
    };
    let (preflight_report, base, head, base_graph, head_graph) =
        bound_metadata_graphs(store, report_id, deadline)?;
    let base_scope = crate::capture::source_scope_policy(store, &base)?;
    let head_scope = crate::capture::source_scope_policy(store, &head)?;
    admit_scope(&base_scope, &base, deadline)?;
    admit_scope(&head_scope, &head, deadline)?;
    #[cfg(test)]
    observe_checked_members(&base, &head, deadline)?;
    limits.admit(&base_graph)?;
    limits.admit(&head_graph)?;
    let mut verified = super::load(store, report_id, deadline)?;
    ensure!(
        verified["artifactId"] == report_id
            && verified["workspaceId"] == preflight_report.workspace_id
            && verified["baseSnapshotId"] == base.id
            && verified["headSnapshotId"] == head.id
            && verified["baseGraphHash"] == preflight_report.base_graph_hash
            && verified["headGraphHash"] == preflight_report.head_graph_hash,
        "Verified report changed after metadata admission"
    );
    drop(preflight_report);
    verified
        .as_object_mut()
        .context("Verified report object missing")?
        .remove("artifactId");
    let report: super::ReportData = crate::json::decode(verified)?;
    let base_epoch =
        crate::capture::analyzer_epoch_for_snapshot_with_deadline(store, &base, deadline)?;
    let head_epoch =
        crate::capture::analyzer_epoch_for_snapshot_with_deadline(store, &head, deadline)?;
    deadline.check()?;
    let result = consume(CheckedGitInputs {
        report_artifact_id: report_id,
        report: &report,
        base: CheckedGitSide {
            snapshot: &base,
            graph: &base_graph,
            graph_hash: &report.base_graph_hash,
            analyzer_epoch: base_epoch,
            scope: &base_scope,
        },
        head: CheckedGitSide {
            snapshot: &head,
            graph: &head_graph,
            graph_hash: &report.head_graph_hash,
            analyzer_epoch: head_epoch,
            scope: &head_scope,
        },
    })?;
    deadline.check()?;
    Ok(result)
}

#[cfg(test)]
type CheckedMemberPolicy = Box<dyn FnMut(&str, &str) -> Result<()>>;
#[cfg(test)]
thread_local! {
    static CHECKED_MEMBER_POLICY: std::cell::RefCell<Option<CheckedMemberPolicy>> = const { std::cell::RefCell::new(None) };
}
#[cfg(test)]
fn with_checked_member_policy<T>(
    policy: impl FnMut(&str, &str) -> Result<()> + 'static,
    body: impl FnOnce() -> T,
) -> T {
    struct Restore(Option<CheckedMemberPolicy>);
    impl Drop for Restore {
        fn drop(&mut self) {
            CHECKED_MEMBER_POLICY.with(|slot| {
                slot.replace(self.0.take());
            });
        }
    }
    let _restore = Restore(CHECKED_MEMBER_POLICY.with(|slot| slot.replace(Some(Box::new(policy)))));
    body()
}
#[cfg(test)]
fn observe_checked_members(base: &Snapshot, head: &Snapshot, deadline: &Deadline) -> Result<()> {
    for (side, snapshot) in [("base", base), ("head", head)] {
        for member in &snapshot.files {
            deadline.check()?;
            CHECKED_MEMBER_POLICY.with(|slot| {
                if let Some(policy) = slot.borrow_mut().as_mut() {
                    policy(side, &member.path)?;
                }
                Ok::<(), anyhow::Error>(())
            })?;
        }
    }
    Ok(())
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Side {
    Base,
    Head,
}
impl Side {
    fn parse(value: &Value) -> Result<Self> {
        match value.as_str() {
            Some("base") => Ok(Self::Base),
            Some("head") => Ok(Self::Head),
            _ => anyhow::bail!("Invalid source side"),
        }
    }
}
struct CheckedSource {
    snapshot_id: String,
    path: String,
    hash: String,
    text: Arc<str>,
}
impl CheckedSource {
    fn binding(&self) -> Value {
        json!({"snapshotId":self.snapshot_id,"path":self.path,"contentHash":self.hash})
    }
}
type Sources = BTreeMap<(Side, String), CheckedSource>;
fn path(value: &Value) -> Result<&str> {
    value.as_str().context("Missing captured path")
}
fn array(value: &Value) -> Result<&Vec<Value>> {
    value.as_array().context("Missing report array")
}
fn object<const N: usize>(fields: [(&str, Value); N]) -> Value {
    Value::Object(
        fields
            .into_iter()
            .map(|(key, value)| (key.to_owned(), value))
            .collect(),
    )
}
fn select_sources(
    report: &super::ReportData,
    deadline: &Deadline,
) -> Result<BTreeSet<(Side, String)>> {
    let mut required = BTreeSet::new();
    for change in &report.changes {
        deadline.check()?;
        match change["status"].as_str() {
            Some("deleted") => {
                required.insert((Side::Base, path(&change["basePath"])?.to_owned()));
            }
            Some("added") => {
                required.insert((Side::Head, path(&change["headPath"])?.to_owned()));
            }
            Some("modified") => {}
            _ => anyhow::bail!("Invalid change status"),
        }
        for hunk in array(&change["hunks"])? {
            for (side, key) in [(Side::Base, "base"), (Side::Head, "head")] {
                deadline.check()?;
                if !hunk[key].is_null() {
                    required.insert((side, path(&hunk[key]["path"])?.to_owned()));
                }
            }
        }
    }
    for row in report.changed_symbols.iter().chain(&report.impact) {
        deadline.check()?;
        required.insert((Side::parse(&row["side"])?, path(&row["path"])?.to_owned()));
    }
    for row in &report.impact_observations {
        let side = Side::parse(&row["side"])?;
        for evidence in array(&row["evidence"])? {
            deadline.check()?;
            required.insert((side, path(&evidence["path"])?.to_owned()));
        }
    }
    Ok(required)
}
fn read_selected(
    report: &super::ReportData,
    base: &Snapshot,
    head: &Snapshot,
    base_scope: &CapturedSourceScope<'_>,
    head_scope: &CapturedSourceScope<'_>,
    options: &DetailOptions,
    deadline: &Deadline,
) -> Result<(Sources, usize)> {
    let required = select_sources(report, deadline)?;
    let mut unique_sizes = BTreeMap::<String, usize>::new();
    let mut admitted = 0usize;
    for (side, path) in &required {
        deadline.check()?;
        let scope = match side {
            Side::Base => base_scope,
            Side::Head => head_scope,
        };
        let member = scope.file(path)?;
        if let Some(size) = unique_sizes.get(&member.hash) {
            ensure!(*size == member.size, "Repeated source size mismatch");
        } else {
            admitted = admitted.checked_add(member.size).ok_or_else(capacity)?;
            if admitted > options.max_source_bytes {
                return Err(capacity());
            }
            unique_sizes.insert(member.hash.clone(), member.size);
        }
    }
    let mut by_hash = BTreeMap::<String, Arc<str>>::new();
    let mut sources = BTreeMap::new();
    let mut actual = 0usize;
    for (side, path) in required {
        deadline.check()?;
        let (scope, snapshot) = match side {
            Side::Base => (base_scope, base),
            Side::Head => (head_scope, head),
        };
        let member = scope.file(&path)?;
        let text = if let Some(text) = by_hash.get(&member.hash) {
            Arc::clone(text)
        } else {
            let text = String::from_utf8(scope.read_source(&path)?)?;
            ensure!(text.len() == member.size, "Selected source length mismatch");
            actual = actual.checked_add(text.len()).ok_or_else(capacity)?;
            if actual > options.max_source_bytes {
                return Err(capacity());
            }
            let shared: Arc<str> = Arc::from(text);
            by_hash.insert(member.hash.clone(), Arc::clone(&shared));
            shared
        };
        ensure!(text.len() == member.size, "Cached source member mismatch");
        sources.insert(
            (side, path.clone()),
            CheckedSource {
                snapshot_id: snapshot.id.clone(),
                path,
                hash: member.hash.clone(),
                text,
            },
        );
    }
    ensure!(admitted == actual, "Selected source counter mismatch");
    Ok((sources, actual))
}
fn source<'a>(sources: &'a Sources, side: Side, anchor: &Value) -> Result<&'a CheckedSource> {
    sources
        .get(&(side, path(&anchor["path"])?.to_owned()))
        .context("Missing checked selected source")
}
fn build_changes(
    report: &super::ReportData,
    sources: &Sources,
    quoter: &mut quotes::Quoter,
    budget: &mut Budget,
    deadline: &Deadline,
) -> Result<Vec<Value>> {
    let mut rows = Vec::new();
    for (index, change) in report.changes.iter().enumerate() {
        deadline.check()?;
        budget.charge(1)?;
        let original = budget.clone_value(change)?;
        let mut hunks = Vec::new();
        for (index, hunk) in array(&change["hunks"])?.iter().enumerate() {
            budget.charge(1)?;
            let mut pair = [Value::Null, Value::Null];
            for (position, side, key) in [(0, Side::Base, "base"), (1, Side::Head, "head")] {
                deadline.check()?;
                if !hunk[key].is_null() {
                    pair[position] = quoter.quote(
                        source(sources, side, &hunk[key])?,
                        &hunk[key],
                        budget,
                        deadline,
                    )?;
                }
            }
            let [base, head] = pair;
            hunks.push(object([
                ("index", json!(index)),
                ("base", base),
                ("head", head),
            ]));
        }
        rows.push(object([
            ("index", json!(index)),
            ("original", original),
            ("hunks", Value::Array(hunks)),
        ]));
    }
    Ok(rows)
}
fn build_nodes(
    originals: &[Value],
    sources: &Sources,
    quoter: &mut quotes::Quoter,
    budget: &mut Budget,
    deadline: &Deadline,
) -> Result<Vec<Value>> {
    let mut rows = Vec::new();
    for (index, original) in originals.iter().enumerate() {
        deadline.check()?;
        budget.charge(1)?;
        let original_copy = budget.clone_value(original)?;
        let quote = quoter.quote(
            source(sources, Side::parse(&original["side"])?, original)?,
            original,
            budget,
            deadline,
        )?;
        rows.push(object([
            ("index", json!(index)),
            ("original", original_copy),
            ("source", quote),
        ]));
    }
    Ok(rows)
}
fn build_observations(
    report: &super::ReportData,
    sources: &Sources,
    quoter: &mut quotes::Quoter,
    budget: &mut Budget,
    deadline: &Deadline,
) -> Result<Vec<Value>> {
    let mut rows = Vec::new();
    for (index, original) in report.impact_observations.iter().enumerate() {
        deadline.check()?;
        budget.charge(1)?;
        let original_copy = budget.clone_value(original)?;
        let side = Side::parse(&original["side"])?;
        let mut evidence = Vec::new();
        for (index, anchor) in array(&original["evidence"])?.iter().enumerate() {
            budget.charge(1)?;
            let quote = quoter.quote(source(sources, side, anchor)?, anchor, budget, deadline)?;
            evidence.push(object([
                ("index", json!(index)),
                ("evidenceId", anchor["id"].clone()),
                ("source", quote),
            ]));
        }
        rows.push(object([
            ("index", json!(index)),
            ("original", original_copy),
            ("evidence", Value::Array(evidence)),
        ]));
    }
    Ok(rows)
}

fn sort_objects(value: &mut Value, deadline: &Deadline) -> Result<()> {
    deadline.check()?;
    match value {
        Value::Object(fields) => {
            for field in fields.values_mut() {
                sort_objects(field, deadline)?;
            }
            let ordered: BTreeMap<_, _> = std::mem::take(fields).into_iter().collect();
            for (key, value) in ordered {
                deadline.check()?;
                fields.insert(key, value);
            }
        }
        Value::Array(items) => {
            for item in items {
                sort_objects(item, deadline)?;
            }
        }
        _ => {}
    }
    Ok(())
}
fn count_rows(value: &Value, deadline: &Deadline) -> Result<usize> {
    deadline.check()?;
    match value {
        Value::Array(items) => items.iter().try_fold(items.len(), |count, item| {
            count
                .checked_add(count_rows(item, deadline)?)
                .ok_or_else(capacity)
        }),
        Value::Object(fields) => fields.values().try_fold(0usize, |count, item| {
            count
                .checked_add(count_rows(item, deadline)?)
                .ok_or_else(capacity)
        }),
        _ => Ok(0),
    }
}
struct WithoutIdentity<'a>(&'a Value);
impl Serialize for WithoutIdentity<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error> {
        let fields = self.0.as_object().expect("owned result object");
        let mut output = serializer.serialize_map(Some(fields.len() - 1))?;
        for (key, value) in fields {
            if key != "resultId" {
                output.serialize_entry(key, value)?;
            }
        }
        output.end()
    }
}
struct WireWriter<'a> {
    bytes: Vec<u8>,
    maximum: usize,
    deadline: &'a Deadline,
    refused: bool,
}
impl Write for WireWriter<'_> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if self.deadline.check().is_err() {
            return Err(io::Error::other("deadline"));
        }
        if self
            .bytes
            .len()
            .checked_add(bytes.len())
            .is_none_or(|size| size > self.maximum)
        {
            self.refused = true;
            return Err(io::Error::other("capacity"));
        }
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
fn finish(
    mut value: Value,
    budget: &Budget,
    options: &DetailOptions,
    deadline: &Deadline,
) -> Result<ImpactDetails> {
    ensure!(
        count_rows(&value, deadline)? == budget.rows,
        "Result row accounting mismatch"
    );
    sort_objects(&mut value, deadline)?;
    let mut settled = false;
    for _ in 0..32 {
        deadline.check()?;
        let length = encoded_len(&value, options.max_output_bytes, deadline)?;
        if value["wireBytes"].as_u64() == Some(u64::try_from(length)?) {
            settled = true;
            break;
        }
        value["wireBytes"] = json!(length);
    }
    ensure!(settled, "Wire fixed point did not settle");
    let id = digest_serialized(&WithoutIdentity(&value), deadline)?;
    value["resultId"] = json!(id);
    let mut writer = WireWriter {
        bytes: Vec::new(),
        maximum: options.max_output_bytes,
        deadline,
        refused: false,
    };
    let result = serde_json::to_writer(&mut writer, &value);
    if writer.refused {
        return Err(capacity());
    }
    result?;
    deadline.check()?;
    ensure!(
        value["wireBytes"].as_u64() == Some(u64::try_from(writer.bytes.len())?),
        "Canonical wire length mismatch"
    );
    Ok(ImpactDetails {
        value,
        canonical_bytes: writer.bytes,
    })
}

pub fn derive(
    store: &Store,
    report_id: &str,
    options: &DetailOptions,
    deadline: &Deadline,
) -> Result<ImpactDetails> {
    derive_inner(store, report_id, options, deadline).map_err(|error| public_error(error, deadline))
}
fn derive_inner(
    store: &Store,
    report_id: &str,
    options: &DetailOptions,
    deadline: &Deadline,
) -> Result<ImpactDetails> {
    deadline.check()?;
    store.set_deadline(deadline)?;
    let _metadata_envelope = MetadataEnvelope::enter(store)?;
    let _read = if store.db.is_autocommit() {
        Some(rusqlite::Transaction::new_unchecked(
            &store.db,
            rusqlite::TransactionBehavior::Deferred,
        )?)
    } else {
        None
    };
    let (preflight_report, base, head) = bound_metadata(store, report_id, deadline)?;
    let base_scope = crate::capture::source_scope_policy(store, &base)?;
    let head_scope = crate::capture::source_scope_policy(store, &head)?;
    admit_scope(&base_scope, &base, deadline)?;
    admit_scope(&head_scope, &head, deadline)?;
    // BOTH real scopes must pass before any test-only additional restriction.
    observe_scope_policy("base")?;
    // A repeated ID is the same persisted row under this retained transaction.
    if base.id != head.id {
        observe_scope_policy("head")?;
    }
    let mut verified = super::load(store, report_id, deadline)?;
    ensure!(
        verified["artifactId"] == report_id
            && verified["workspaceId"] == preflight_report.workspace_id
            && verified["baseSnapshotId"] == base.id
            && verified["headSnapshotId"] == head.id
            && verified["baseGraphHash"] == preflight_report.base_graph_hash
            && verified["headGraphHash"] == preflight_report.head_graph_hash,
        "Verified report changed after metadata admission"
    );
    // Use the real loader's normalized report, not an authority inferred from
    // preflight metadata. Release that earlier body before consuming this one.
    drop(preflight_report);
    verified
        .as_object_mut()
        .context("Verified report object missing")?
        .remove("artifactId");
    let report: super::ReportData = crate::json::decode(verified)?;
    let mut budget = Budget::new(options, deadline);
    budget.charge(4)?;
    budget.reserve_serialized(options)?;
    let (sources, source_bytes) = read_selected(
        &report,
        &base,
        &head,
        &base_scope,
        &head_scope,
        options,
        deadline,
    )?;
    let mut deleted = Vec::new();
    let mut added = Vec::new();
    for change in &report.changes {
        deadline.check()?;
        if change["status"] == "deleted" {
            deleted.push(
                sources
                    .get(&(Side::Base, path(&change["basePath"])?.to_owned()))
                    .context("Deleted checked source missing")?,
            );
        } else if change["status"] == "added" {
            added.push(
                sources
                    .get(&(Side::Head, path(&change["headPath"])?.to_owned()))
                    .context("Added checked source missing")?,
            );
        }
    }
    let (rename_candidates, candidate_summary) =
        candidates::compare(&deleted, &added, options, &mut budget, deadline)?;
    let mut quoter = quotes::Quoter::new(options);
    let changes = build_changes(&report, &sources, &mut quoter, &mut budget, deadline)?;
    let changed = build_nodes(
        &report.changed_symbols,
        &sources,
        &mut quoter,
        &mut budget,
        deadline,
    )?;
    let impacts = build_nodes(&report.impact, &sources, &mut quoter, &mut budget, deadline)?;
    let observations = build_observations(&report, &sources, &mut quoter, &mut budget, deadline)?;
    let value = object([
        ("schemaVersion", json!(1)),
        ("kind", json!("git_impact_details_v1")),
        ("algorithmVersion", json!("yoke_git_impact_details_1")),
        ("resultId", json!("0".repeat(64))),
        ("reportArtifactId", json!(report_id)),
        ("workspaceId", Value::String(report.workspace_id)),
        ("baseSnapshotId", Value::String(base.id.clone())),
        ("headSnapshotId", Value::String(head.id.clone())),
        ("baseGraphHash", Value::String(report.base_graph_hash)),
        ("headGraphHash", Value::String(report.head_graph_hash)),
        ("options", serde_json::to_value(options)?),
        ("sourceBytes", json!(source_bytes)),
        ("candidateSummary", candidate_summary),
        ("renameCandidates", Value::Array(rename_candidates)),
        ("changes", Value::Array(changes)),
        ("changedSymbols", Value::Array(changed)),
        ("impact", Value::Array(impacts)),
        ("impactObservations", Value::Array(observations)),
        ("coverage", quoter.coverage()),
        ("rowUnits", json!(budget.rows)),
        ("wireBytes", json!(0)),
        ("complete", json!(false)),
        (
            "limitations",
            json!([
                "structural_candidate_not_runtime_proof",
                "rename_candidates_not_identity",
                "captured_report_not_live_ref_freshness",
                "obvious_secret_filter_not_complete_dlp"
            ]),
        ),
    ]);
    finish(value, &budget, options, deadline)
}

/// HTML has a separate fixed expansion bound derived from the actual admitted
/// canonical bytes, never from a caller-sized preallocation or a new grant.
pub fn render_html(details: &ImpactDetails, deadline: &Deadline) -> Result<String> {
    (||->Result<String>{
        const PREFIX:&str="<!doctype html><html><head><meta charset=\"utf-8\"><title>Git impact details</title></head><body><pre>";
        const SUFFIX:&str="</pre></body></html>";
        deadline.check()?;
        let maximum=details.canonical_bytes.len().checked_mul(6).and_then(|size|size.checked_add(PREFIX.len()+SUFFIX.len())).ok_or_else(capacity)?;
        let mut writer=WireWriter {bytes:Vec::new(),maximum,deadline,refused:false};
        writer.write_all(PREFIX.as_bytes())?;
        for character in std::str::from_utf8(&details.canonical_bytes)?.chars() {
            deadline.check()?;
            let mut encoded=[0;4];
            let escaped=match character {'&'=>"&amp;",'<'=>"&lt;",'>'=>"&gt;",'"'=>"&quot;",'\''=>"&#39;",_=>character.encode_utf8(&mut encoded)};
            writer.write_all(escaped.as_bytes())?;
        }
        writer.write_all(SUFFIX.as_bytes())?;deadline.check()?;
        Ok(String::from_utf8(writer.bytes)?)
    })().map_err(|error|public_error(error,deadline))
}

#[cfg(test)]
type ScopePolicy = Box<dyn FnMut(&str) -> Result<()>>;
#[cfg(test)]
thread_local! {
    static EXTRA_SCOPE_POLICY:std::cell::RefCell<Option<ScopePolicy>>=std::cell::RefCell::new(None);
    static CANDIDATE_OBSERVER:std::cell::RefCell<Option<Box<dyn FnMut()>>>=std::cell::RefCell::new(None);
}
#[cfg(test)]
fn observe_scope_policy(side: &str) -> Result<()> {
    EXTRA_SCOPE_POLICY.with(|slot| match slot.borrow_mut().as_mut() {
        Some(callback) => callback(side),
        None => Ok(()),
    })
}
#[cfg(not(test))]
fn observe_scope_policy(_side: &str) -> Result<()> {
    Ok(())
}
fn observe_candidate_pair(deadline: &Deadline) -> Result<()> {
    deadline.check()?;
    #[cfg(test)]
    CANDIDATE_OBSERVER.with(|slot| {
        if let Some(callback) = slot.borrow_mut().as_mut() {
            callback();
        }
    });
    deadline.check()
}
#[cfg(test)]
fn with_additional_scope_policy<T>(
    policy: impl FnMut(&str) -> Result<()> + 'static,
    body: impl FnOnce() -> T,
) -> T {
    struct Restore(Option<ScopePolicy>);
    impl Drop for Restore {
        fn drop(&mut self) {
            EXTRA_SCOPE_POLICY.with(|slot| {
                slot.replace(self.0.take());
            });
        }
    }
    let _restore = Restore(EXTRA_SCOPE_POLICY.with(|slot| slot.replace(Some(Box::new(policy)))));
    body()
}
#[cfg(test)]
fn with_candidate_pair_observer<T>(observe: impl FnMut() + 'static, body: impl FnOnce() -> T) -> T {
    struct Restore(Option<Box<dyn FnMut()>>);
    impl Drop for Restore {
        fn drop(&mut self) {
            CANDIDATE_OBSERVER.with(|slot| {
                slot.replace(self.0.take());
            });
        }
    }
    let _restore = Restore(CANDIDATE_OBSERVER.with(|slot| slot.replace(Some(Box::new(observe)))));
    body()
}
#[cfg(test)]
#[path = "git_impact_details_scope_tests.rs"]
mod scope_tests;
