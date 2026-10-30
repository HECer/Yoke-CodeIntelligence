//! Existing local model inferences qualified by independently checked received
//! declaration correspondence. No JSON constructor, provider dispatch or writes.
use super::*;
use crate::{
    github_app::{HostProfile, digest as native},
    meaning_view::{self, MeaningLayer},
    model::{Graph, Snapshot},
};
use anyhow::Context as _;
use serde::de::DeserializeOwned;

const INVALID: &str = "github_app_digest_interpretation_input_invalid";
const PERMISSION: &str = "github_app_digest_interpretation_permission_required";
const UNAVAILABLE: &str = "github_app_digest_interpretation_parent_unavailable";
const MISMATCH: &str = "github_app_digest_interpretation_source_mismatch";
const CAPACITY: &str = "github_app_digest_interpretation_capacity_exceeded";

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Request {
    pub schema_version: u8,
    pub verification: verification::Request,
    pub meaning_graph_ids: Vec<String>,
}
impl Request {
    pub fn parse(raw: &[u8]) -> Result<Self> {
        let r: Self = crate::json::parse(raw, 16_384)
            .and_then(crate::json::decode)
            .map_err(|_| anyhow::anyhow!(INVALID))?;
        r.validate()?;
        Ok(r)
    }
    pub fn validate(&self) -> Result<()> {
        ensure!(self.schema_version == 1, INVALID);
        self.verification.validate()?;
        ensure!(
            !self.meaning_graph_ids.is_empty()
                && self.meaning_graph_ids.len() <= 4
                && self.meaning_graph_ids.iter().all(|id| valid_id(id))
                && self.meaning_graph_ids.iter().collect::<BTreeSet<_>>().len()
                    == self.meaning_graph_ids.len(),
            INVALID
        );
        Ok(())
    }
}
/// Host authority only; selecting a graph or sending JSON cannot grant quoting.
pub struct Options {
    pub allow_source_interpretation: bool,
    pub max_input_bytes: usize,
    pub max_unit_bindings: usize,
    pub max_mapping_checks: usize,
    pub max_declaration_matches: usize,
    pub max_output_bytes: usize,
}
impl Default for Options {
    fn default() -> Self {
        Self {
            allow_source_interpretation: false,
            max_input_bytes: 32_000_000,
            max_unit_bindings: 4096,
            max_mapping_checks: 1_000_000,
            max_declaration_matches: 65_536,
            max_output_bytes: 16_000_000,
        }
    }
}
fn options(o: &Options, d: &Deadline) -> Result<()> {
    ensure!(o.allow_source_interpretation, PERMISSION);
    d.check()?;
    ensure!(
        (1..=32_000_000).contains(&o.max_input_bytes)
            && (1..=4096).contains(&o.max_unit_bindings)
            && (1..=4_000_000).contains(&o.max_mapping_checks)
            && (1..=65_536).contains(&o.max_declaration_matches)
            && (1024..=64_000_000).contains(&o.max_output_bytes),
        CAPACITY
    );
    Ok(())
}
pub fn admit(
    profile: &Profile,
    principal: &str,
    source: &HostProfile,
    r: &Request,
    verification_options: &verification::Options,
    o: &Options,
    d: &Deadline,
) -> Result<()> {
    options(o, d)?;
    r.validate()?;
    verification::admit(
        profile,
        principal,
        source,
        &r.verification,
        verification_options,
        d,
    )
}
fn parent_failure(d: &Deadline) -> anyhow::Error {
    d.check()
        .err()
        .unwrap_or_else(|| anyhow::anyhow!(UNAVAILABLE))
}
fn metadata<T: DeserializeOwned>(
    store: &Store,
    table: &str,
    id: &str,
    remaining: &mut usize,
    d: &Deadline,
) -> Result<T> {
    d.check()?;
    ensure!(matches!(table, "snapshots" | "graphs"), INVALID);
    ensure!(*remaining > 0, CAPACITY);
    let raw: Vec<u8> = store
        .db
        .query_row(
            &format!("SELECT substr(CAST(value AS BLOB),1,?2) FROM {table} WHERE id=?1"),
            rusqlite::params![id, *remaining + 1],
            |row| row.get(0),
        )
        .map_err(|_| parent_failure(d))?;
    ensure!(raw.len() <= *remaining, CAPACITY);
    *remaining -= raw.len();
    crate::json::parse(&raw, raw.len())
        .and_then(crate::json::decode)
        .map_err(|_| parent_failure(d))
}
fn replay(
    c: &verification::Context<'_>,
    r: &Request,
    o: &Options,
    d: &Deadline,
) -> Result<MeaningLayer> {
    let mut policy = native::source::ReadPolicy::enter(c.source_store, d, o.max_input_bytes)
        .map_err(|_| parent_failure(d))?;
    let result = (|| {
        let _read = if c.source_store.db.is_autocommit() {
            Some(
                rusqlite::Transaction::new_unchecked(
                    &c.source_store.db,
                    rusqlite::TransactionBehavior::Deferred,
                )
                .map_err(|_| parent_failure(d))?,
            )
        } else {
            None
        };
        let mut remaining = o.max_input_bytes;
        let snapshot: Snapshot = metadata(
            c.source_store,
            "snapshots",
            &r.verification.local_snapshot_id,
            &mut remaining,
            d,
        )?;
        let graph: Graph = metadata(
            c.source_store,
            "graphs",
            &r.verification.local_snapshot_id,
            &mut remaining,
            d,
        )?;
        ensure!(
            snapshot.id == r.verification.local_snapshot_id
                && snapshot.workspace_id == c.source_store.workspace_id
                && graph.snapshot_id == snapshot.id,
            MISMATCH
        );
        ensure!(remaining > 0, CAPACITY);
        meaning_view::load_selected(
            c.source_store,
            &snapshot,
            &graph,
            &r.meaning_graph_ids,
            remaining,
            d,
        )
        .map_err(|_| parent_failure(d))?
        .context(UNAVAILABLE)
    })();
    policy.restore().map_err(|_| parent_failure(d))?;
    result
}
fn integer(value: &Value, key: &str) -> Result<usize> {
    value[key]
        .as_u64()
        .and_then(|n| usize::try_from(n).ok())
        .context(UNAVAILABLE)
}
/// Retains the entire checked semantic layer and every declaration correspondence.
/// Its source quotations are present only behind explicit host interpretation permission.
pub struct CheckedInterpretation {
    value: Value,
}
pub fn interpret(
    c: &verification::Context<'_>,
    r: &Request,
    o: &Options,
    d: &Deadline,
) -> Result<CheckedInterpretation> {
    admit(
        c.receiver_profile,
        c.principal,
        c.source_profile,
        r,
        c.options,
        o,
        d,
    )?;
    let verified =
        verification::verify(c, &r.verification, d)?.export(c.options.max_output_bytes, d)?;
    // The receipt/source verification ends before semantic replay. Exact local
    // workspace/snapshot/file hashes and unit bounds join immutable observations;
    // this makes no atomic cross-Store/current-remote-head claim.
    let layer = replay(c, r, o, d)?;
    ensure!(
        layer.scope.workspace_id == c.source_store.workspace_id
            && layer.scope.snapshot_id == r.verification.local_snapshot_id,
        MISMATCH
    );
    let rows = verified["rows"].as_array().context(UNAVAILABLE)?;
    let mut by_file = BTreeMap::<(&str, &str), Vec<&Value>>::new();
    for row in rows {
        d.check()?;
        let source = &row["localSource"];
        by_file
            .entry((
                source["path"].as_str().context(UNAVAILABLE)?,
                source["contentHash"].as_str().context(UNAVAILABLE)?,
            ))
            .or_default()
            .push(row);
    }
    let mut bindings = Vec::new();
    let mut checks = 0usize;
    let mut matches = 0usize;
    for packet in &layer.packets {
        for unit in &packet.source_bindings {
            d.check()?;
            ensure!(bindings.len() < o.max_unit_bindings, CAPACITY);
            let candidates = by_file
                .get(&(unit.path.as_str(), unit.content_hash.as_str()))
                .context(MISMATCH)?;
            let mut correspondence = Vec::new();
            for row in candidates {
                d.check()?;
                ensure!(checks < o.max_mapping_checks, CAPACITY);
                checks += 1;
                let source = &row["localSource"];
                if integer(source, "startByte")? <= unit.start_byte
                    && unit.end_byte <= integer(source, "endByte")?
                {
                    ensure!(matches < o.max_declaration_matches, CAPACITY);
                    matches += 1;
                    correspondence.push(json!({"verificationRowId":row["id"],"receivedOrdinal":row["ordinal"],"receivedPointer":row["pointer"],
                        "localEntityId":source["entityId"],"localGraphEntityId":source["graphEntityId"],"matchGranularity":"submitted_unit_contained_in_verified_full_declaration",
                        "semanticIdentityEstablished":false}));
                }
            }
            ensure!(!correspondence.is_empty(), MISMATCH);
            bindings.push(json!({"selectionIndex":packet.selection_index,"packetIndex":packet.index,"inputHash":packet.input_hash,
                "unit":unit,"declarationCorrespondence":correspondence}));
        }
    }
    ensure!(!bindings.is_empty(), UNAVAILABLE);
    let mut value = json!({"schemaVersion":1,"kind":"native_received_source_interpretation_v1",
        "verification":verified,"requestedMeaningGraphIds":r.meaning_graph_ids,"meaningLayer":layer,"unitBindings":bindings,
        "mappingChecks":checks,"declarationMatches":matches,"sourceQuotesPresent":true,
        "providerCallsThisRead":0,"networkRequests":0,"recordsWritten":false,"sourceWritten":false,
        "interpretationResolution":"inferred_concept","providerAttestation":"unverified","semanticIdentityEstablished":false,
        "observedExecution":false,"sourceAuthority":false,"complete":false,
        "limitations":["Local Meaning source, canonical graph, capture lineage, immutable responses and accounting are independently replayed. All submitted units, including no-claims coverage, must fit independently matched received declaration fingerprints and exact local whole-file hashes.",
        "Model interpretations remain unverified inferences. Citation correspondence is not concept/declaration identity, compiler binding or runtime observation. All containing declarations and original directed parallel relations/hyperedge roles remain separate.",
        "The original explicit host Meaning build owns model attempts and accounting; this read resolves no provider and writes nothing. Sender snapshot/CAS identity, source-digest hash, discussion/history claims, whole-source equivalence and remote ownership remain unverified."]});
    let bytes = encoded(&value, o.max_output_bytes, d)?;
    value["interpretationId"] = json!(crate::model::hash(bytes));
    encoded(&value, o.max_output_bytes, d)?;
    Ok(CheckedInterpretation { value })
}
fn encoded(value: &Value, max: usize, d: &Deadline) -> Result<Vec<u8>> {
    ensure!((1024..=64_000_000).contains(&max), CAPACITY);
    d.check()?;
    let raw = crate::json::encode_bounded(value, max - 1, d)
        .map_err(|_| d.check().err().unwrap_or_else(|| anyhow::anyhow!(CAPACITY)))?;
    d.check()?;
    Ok(raw)
}
impl CheckedInterpretation {
    pub fn export(&self, max_bytes: usize, d: &Deadline) -> Result<Value> {
        encoded(&self.value, max_bytes, d)?;
        Ok(self.value.clone())
    }
}
