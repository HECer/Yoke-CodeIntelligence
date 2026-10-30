//! Finite captured-hierarchy candidates derived from an existing verified Git report.
use crate::{
    analysis::AnalyzerEpoch,
    capture::Deadline,
    model::{Graph, Snapshot, SourceFile, valid_id},
    store::Store,
};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize, Serializer};
use serde_json::Value;
use std::{
    collections::BTreeSet,
    fmt,
    io::{self, Write},
};

#[path = "git_affected_index.rs"]
mod index;
#[path = "git_affected_output.rs"]
mod output;
#[path = "git_affected_source.rs"]
mod source;
#[cfg(test)]
#[path = "git_affected_tests.rs"]
mod tests;
#[path = "git_affected_walk.rs"]
mod walk;

const POLICY: &str = "captured_hierarchy_v1";
const LIMITATIONS: [&str; 5] = [
    "structural_candidate_not_compiler_or_runtime_proof",
    "same_name_members_not_override_or_rename_identity",
    "captured_report_not_live_ref_freshness",
    "selected_depths_not_exhaustive_reachability",
    "new_source_bytes_exclude_inherited_loader_io",
];

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AffectedOptions {
    pub policy: String,
    pub max_graph_entities: usize,
    pub max_graph_observations: usize,
    pub max_graph_evidence: usize,
    pub max_original_seeds: usize,
    pub max_expanded_seeds: usize,
    pub max_hierarchy_depth: usize,
    pub max_incoming_depth: usize,
    pub max_states: usize,
    pub max_edge_visits: usize,
    pub max_rows: usize,
    pub max_source_bytes: usize,
    pub max_output_bytes: usize,
}

impl Default for AffectedOptions {
    fn default() -> Self {
        Self {
            policy: "captured_hierarchy_v1".into(),
            max_graph_entities: 5_000,
            max_graph_observations: 50_000,
            max_graph_evidence: 100_000,
            max_original_seeds: 2_000,
            max_expanded_seeds: 4_000,
            max_hierarchy_depth: 16,
            max_incoming_depth: 8,
            max_states: 20_000,
            max_edge_visits: 200_000,
            max_rows: 20_000,
            max_source_bytes: 64_000_000,
            max_output_bytes: 4_000_000,
        }
    }
}

#[derive(Clone, Debug)]
pub struct AffectedDetails {
    value: Value,
    canonical_bytes: Vec<u8>,
}

impl AffectedDetails {
    pub fn value(&self) -> &Value {
        &self.value
    }

    pub fn canonical_bytes(&self) -> &[u8] {
        &self.canonical_bytes
    }
}

impl Serialize for AffectedDetails {
    fn serialize<S: Serializer>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error> {
        self.value.serialize(serializer)
    }
}

pub fn derive(
    store: &Store,
    report_id: &str,
    options: &AffectedOptions,
    deadline: &Deadline,
) -> Result<AffectedDetails> {
    derive_inner(store, report_id, options, deadline).map_err(|error| public_error(error, deadline))
}

fn validate_options(options: &AffectedOptions) -> Result<()> {
    let maximum = AffectedOptions::default();
    ensure!(options.policy == POLICY, "Unsupported affected policy");
    for (actual, maximum) in [
        (options.max_graph_entities, maximum.max_graph_entities),
        (
            options.max_graph_observations,
            maximum.max_graph_observations,
        ),
        (options.max_graph_evidence, maximum.max_graph_evidence),
        (options.max_original_seeds, maximum.max_original_seeds),
        (options.max_expanded_seeds, maximum.max_expanded_seeds),
        (options.max_hierarchy_depth, maximum.max_hierarchy_depth),
        (options.max_incoming_depth, maximum.max_incoming_depth),
        (options.max_states, maximum.max_states),
        (options.max_edge_visits, maximum.max_edge_visits),
        (options.max_rows, maximum.max_rows),
        (options.max_source_bytes, maximum.max_source_bytes),
        (options.max_output_bytes, maximum.max_output_bytes),
    ] {
        ensure!(actual <= maximum, "Affected option exceeds finite profile");
    }
    Ok(())
}

fn derive_inner(
    store: &Store,
    report_id: &str,
    options: &AffectedOptions,
    deadline: &Deadline,
) -> Result<AffectedDetails> {
    deadline.check()?;
    validate_options(options)?;
    ensure!(valid_id(report_id), "Invalid affected report identity");
    let limits = super::impact_details::GraphInputLimits::new(
        options.max_graph_entities,
        options.max_graph_observations,
        options.max_graph_evidence,
    )?;
    super::impact_details::with_checked_report_inputs(
        store,
        report_id,
        limits,
        deadline,
        |inputs| {
            let access: &dyn SourceAccess = &inputs;
            let mut budget = Budget::new(options, deadline);
            validate_unique_entities(access, deadline)?;
            let indexes = [
                index::Index::build(Side::Base, access, &mut budget)?,
                index::Index::build(Side::Head, access, &mut budget)?,
            ];
            let mut sources = source::SourceCache::new(access);
            let walked = walk::derive(access, &indexes, &mut sources, &mut budget)?;
            let (value, canonical_bytes) =
                output::finish(access, &indexes, &mut sources, &walked, &mut budget)?;
            deadline.check()?;
            Ok(AffectedDetails {
                value,
                canonical_bytes,
            })
        },
    )
}

fn validate_unique_entities(access: &dyn SourceAccess, deadline: &Deadline) -> Result<()> {
    for side in [Side::Base, Side::Head] {
        let mut ids = BTreeSet::new();
        let mut keys = BTreeSet::new();
        for entity in &access.graph(side).entities {
            deadline.check()?;
            ensure!(
                ids.insert(entity.id.as_str()) && keys.insert(entity.key.as_str()),
                "Duplicate affected entity identity or key"
            );
        }
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Side {
    Base,
    Head,
}
impl Side {
    fn label(self) -> &'static str {
        match self {
            Self::Base => "base",
            Self::Head => "head",
        }
    }
    fn index(self) -> usize {
        match self {
            Self::Base => 0,
            Self::Head => 1,
        }
    }
    fn parse(value: &Value) -> Result<Self> {
        match value.as_str() {
            Some("base") => Ok(Self::Base),
            Some("head") => Ok(Self::Head),
            _ => anyhow::bail!("Invalid affected source side"),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Eligibility {
    Expandable { owner: usize },
    Unsupported(&'static str),
}

trait SourceAccess {
    fn report_artifact_id(&self) -> &str;
    fn workspace_id(&self) -> &str;
    fn snapshot(&self, side: Side) -> &Snapshot;
    fn graph(&self, side: Side) -> &Graph;
    fn graph_hash(&self, side: Side) -> &str;
    fn analyzer_epoch(&self, side: Side) -> AnalyzerEpoch;
    fn file(&self, side: Side, path: &str) -> Result<&SourceFile>;
    fn read_source(&self, side: Side, path: &str) -> Result<Vec<u8>>;
    fn changed_symbols(&self) -> &[Value];
    fn impact(&self) -> &[Value];
    fn impact_observations(&self) -> &[Value];
}

impl SourceAccess for super::impact_details::CheckedGitInputs<'_> {
    fn report_artifact_id(&self) -> &str {
        self.report_artifact_id()
    }
    fn workspace_id(&self) -> &str {
        self.workspace_id()
    }
    fn snapshot(&self, side: Side) -> &Snapshot {
        match side {
            Side::Base => self.base().snapshot(),
            Side::Head => self.head().snapshot(),
        }
    }
    fn graph(&self, side: Side) -> &Graph {
        match side {
            Side::Base => self.base().graph(),
            Side::Head => self.head().graph(),
        }
    }
    fn graph_hash(&self, side: Side) -> &str {
        match side {
            Side::Base => self.base().graph_hash(),
            Side::Head => self.head().graph_hash(),
        }
    }
    fn analyzer_epoch(&self, side: Side) -> AnalyzerEpoch {
        match side {
            Side::Base => self.base().analyzer_epoch(),
            Side::Head => self.head().analyzer_epoch(),
        }
    }
    fn file(&self, side: Side, path: &str) -> Result<&SourceFile> {
        match side {
            Side::Base => self.base().file(path),
            Side::Head => self.head().file(path),
        }
    }
    fn read_source(&self, side: Side, path: &str) -> Result<Vec<u8>> {
        match side {
            Side::Base => self.base().read_source(path),
            Side::Head => self.head().read_source(path),
        }
    }
    fn changed_symbols(&self) -> &[Value] {
        self.changed_symbols()
    }
    fn impact(&self) -> &[Value] {
        self.impact()
    }
    fn impact_observations(&self) -> &[Value] {
        self.impact_observations()
    }
}

#[derive(Debug)]
struct Capacity;
impl fmt::Display for Capacity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("git_affected_details_capacity_exceeded")
    }
}
impl std::error::Error for Capacity {}
fn capacity() -> anyhow::Error {
    Capacity.into()
}
fn public_error(error: anyhow::Error, deadline: &Deadline) -> anyhow::Error {
    let marker = if deadline.check().is_err() {
        "git_affected_details_deadline_exceeded"
    } else if error.is::<Capacity>() || super::impact_details::is_checked_input_capacity(&error) {
        "git_affected_details_capacity_exceeded"
    } else {
        "git_affected_details_input_invalid"
    };
    anyhow::anyhow!(marker)
}

#[derive(Default)]
struct Consumed {
    original_seed_rows: usize,
    unique_expanded_seeds: usize,
    hierarchy_states: usize,
    incoming_states: usize,
    index_observation_scans: usize,
    hierarchy_adjacency_visits: usize,
    incoming_adjacency_visits: usize,
    new_evidence_source_bytes: usize,
}

struct Budget<'a> {
    options: &'a AffectedOptions,
    deadline: &'a Deadline,
    counts: Consumed,
    rows: usize,
    reserved_wire: usize,
}
impl<'a> Budget<'a> {
    fn new(options: &'a AffectedOptions, deadline: &'a Deadline) -> Self {
        Self {
            options,
            deadline,
            counts: Consumed::default(),
            rows: 0,
            reserved_wire: 0,
        }
    }
    fn check(&self) -> Result<()> {
        self.deadline.check()
    }
    fn add(value: &mut usize, amount: usize, maximum: usize) -> Result<()> {
        let next = value.checked_add(amount).ok_or_else(capacity)?;
        if next > maximum {
            return Err(capacity());
        }
        *value = next;
        Ok(())
    }
    fn charge_original(&mut self, amount: usize) -> Result<()> {
        self.check()?;
        Self::add(
            &mut self.counts.original_seed_rows,
            amount,
            self.options.max_original_seeds,
        )
    }
    fn charge_expanded(&mut self) -> Result<()> {
        self.check()?;
        Self::add(
            &mut self.counts.unique_expanded_seeds,
            1,
            self.options.max_expanded_seeds,
        )
    }
    fn charge_state(&mut self, hierarchy: bool) -> Result<()> {
        self.check()?;
        let total = self
            .counts
            .hierarchy_states
            .checked_add(self.counts.incoming_states)
            .and_then(|n| n.checked_add(1))
            .ok_or_else(capacity)?;
        if total > self.options.max_states {
            return Err(capacity());
        }
        let field = if hierarchy {
            &mut self.counts.hierarchy_states
        } else {
            &mut self.counts.incoming_states
        };
        Self::add(field, 1, self.options.max_states)
    }
    fn charge_index_scan(&mut self) -> Result<()> {
        self.charge_edge(0)
    }
    fn charge_adjacency(&mut self, hierarchy: bool) -> Result<()> {
        self.charge_edge(if hierarchy { 1 } else { 2 })
    }
    fn charge_edge(&mut self, which: u8) -> Result<()> {
        self.check()?;
        let total = self
            .counts
            .index_observation_scans
            .checked_add(self.counts.hierarchy_adjacency_visits)
            .and_then(|n| n.checked_add(self.counts.incoming_adjacency_visits))
            .and_then(|n| n.checked_add(1))
            .ok_or_else(capacity)?;
        if total > self.options.max_edge_visits {
            return Err(capacity());
        }
        let field = match which {
            0 => &mut self.counts.index_observation_scans,
            1 => &mut self.counts.hierarchy_adjacency_visits,
            _ => &mut self.counts.incoming_adjacency_visits,
        };
        Self::add(field, 1, self.options.max_edge_visits)
    }
    fn charge_source(&mut self, amount: usize) -> Result<()> {
        self.check()?;
        Self::add(
            &mut self.counts.new_evidence_source_bytes,
            amount,
            self.options.max_source_bytes,
        )
    }
    fn charge_rows(&mut self, amount: usize) -> Result<()> {
        self.check()?;
        Self::add(&mut self.rows, amount, self.options.max_rows)
    }
    fn ensure_wire(&self, amount: usize) -> Result<()> {
        self.check()?;
        if amount > self.options.max_output_bytes {
            return Err(capacity());
        }
        Ok(())
    }
    fn reserve_serialized<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<()> {
        self.check()?;
        let remaining = self
            .options
            .max_output_bytes
            .checked_sub(self.reserved_wire)
            .ok_or_else(capacity)?;
        let mut count = WireCounter {
            count: 0,
            maximum: remaining,
            deadline: self.deadline,
        };
        if serde_json::to_writer(&mut count, value).is_err() {
            self.check()?;
            return Err(capacity());
        }
        Self::add(
            &mut self.reserved_wire,
            count.count,
            self.options.max_output_bytes,
        )
    }
}

struct WireCounter<'a> {
    count: usize,
    maximum: usize,
    deadline: &'a Deadline,
}
impl Write for WireCounter<'_> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.deadline
            .check()
            .map_err(|_| io::Error::other("deadline"))?;
        let next = self
            .count
            .checked_add(bytes.len())
            .ok_or_else(|| io::Error::other("capacity"))?;
        if next > self.maximum {
            return Err(io::Error::other("capacity"));
        }
        self.count = next;
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

// A test can deny work through the existing inherited cancellation flag. These
// observers never grant authority, replace a deadline or manufacture an error.
#[cfg(test)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum WorkPhase {
    IndexScan,
    HierarchyAdjacency,
    IncomingAdjacency,
    WitnessComparison,
    OutputSort,
    CanonicalEncoding,
}

#[cfg(test)]
struct WorkCancellation {
    phase: WorkPhase,
    after: usize,
    seen: usize,
    flag: std::sync::Arc<std::sync::atomic::AtomicBool>,
}

#[cfg(test)]
thread_local! {
    static WORK_CANCELLATION: std::cell::RefCell<Option<WorkCancellation>> = const {
        std::cell::RefCell::new(None)
    };
}

#[cfg(test)]
struct WorkCancellationGuard(std::marker::PhantomData<*mut ()>);

#[cfg(test)]
impl WorkCancellationGuard {
    fn seen(&self) -> usize {
        WORK_CANCELLATION.with(|state| state.borrow().as_ref().map_or(0, |state| state.seen))
    }
}

#[cfg(test)]
impl Drop for WorkCancellationGuard {
    fn drop(&mut self) {
        WORK_CANCELLATION.with(|state| *state.borrow_mut() = None);
    }
}

#[cfg(test)]
fn cancel_work_at(
    phase: WorkPhase,
    after: usize,
    flag: std::sync::Arc<std::sync::atomic::AtomicBool>,
) -> WorkCancellationGuard {
    assert!(after > 0);
    WORK_CANCELLATION.with(|state| {
        let mut state = state.borrow_mut();
        assert!(state.is_none(), "Nested affected cancellation observer");
        *state = Some(WorkCancellation {
            phase,
            after,
            seen: 0,
            flag,
        });
    });
    WorkCancellationGuard(std::marker::PhantomData)
}

#[cfg(test)]
fn observe_work(phase: WorkPhase) {
    WORK_CANCELLATION.with(|state| {
        let mut state = state.borrow_mut();
        if let Some(state) = state.as_mut()
            && state.phase == phase
        {
            state.seen += 1;
            if state.seen >= state.after {
                state.flag.store(true, std::sync::atomic::Ordering::SeqCst);
            }
        }
    });
}
