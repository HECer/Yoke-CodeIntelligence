//! Explicit authored outcomes with original source proofs; never execution authority.
use super::{Anchor, MEMORY_KIND, Memory, bounded_text};
use crate::{
    capture::Deadline,
    graph_view::comparison_input::{self, CheckedComparisonPair},
    model::{hash, identity, valid_id},
    store::Store,
    view_comparison::{ComparisonOptions, SelectionPin},
};
use anyhow::{Context, Result, ensure};
use rusqlite::{OptionalExtension, Transaction, TransactionBehavior, params};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

#[path = "knowledge_feedback_provenance.rs"]
mod provenance;
#[path = "knowledge_feedback_rank.rs"]
mod ranking;
#[path = "knowledge_feedback_store.rs"]
mod storage;
#[path = "knowledge_feedback_transcript.rs"]
mod transcript;

pub const EVENT_KIND: &str = "native_feedback_event_v1";
pub const TRANSCRIPT_KIND: &str = "native_feedback_transcript_v1";
pub const MAX_DRAFT_BYTES: usize = 64_000;
pub const MAX_TRANSCRIPT_BYTES: usize = 1_000_000;
const MAX_RECORD_BYTES: usize = 128_000;
const MAX_STORE_BYTES: usize = 32_000_000;
const DAY_MS: u64 = 86_400_000;

#[derive(Clone, Debug)]
pub struct Options {
    pub max_events: usize,
    pub max_input_bytes: usize,
    pub max_source_bytes: usize,
    pub max_pairs: usize,
    pub max_candidates: usize,
    pub max_output_bytes: usize,
}
impl Default for Options {
    fn default() -> Self {
        Self {
            max_events: 10_000,
            max_input_bytes: 64_000_000,
            max_source_bytes: 64_000_000,
            max_pairs: 32,
            max_candidates: 10_000,
            max_output_bytes: 4_000_000,
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ViewPin {
    pub workspace_id: String,
    pub snapshot_id: String,
    pub view_id: String,
    pub view_artifact_id: String,
    pub base_graph_hash: String,
}
impl ViewPin {
    fn native(&self) -> SelectionPin {
        SelectionPin {
            workspace_id: self.workspace_id.clone(),
            snapshot_id: self.snapshot_id.clone(),
            view_id: self.view_id.clone(),
            view_artifact_id: self.view_artifact_id.clone(),
            base_graph_hash: self.base_graph_hash.clone(),
        }
    }
    fn validate(&self) -> Result<()> {
        ensure!(
            [
                &self.workspace_id,
                &self.snapshot_id,
                &self.view_id,
                &self.view_artifact_id,
                &self.base_graph_hash
            ]
            .into_iter()
            .all(|id| valid_id(id)),
            "feedback_input_invalid"
        );
        Ok(())
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    Success,
    Failure,
    Inconclusive,
    Corroboration,
    Contradiction,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SourceQuote {
    pub path: String,
    pub content_hash: String,
    pub start_byte: usize,
    pub end_byte: usize,
    pub quote: String,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TranscriptQuote {
    pub transcript_id: String,
    pub content_hash: String,
    pub start_byte: usize,
    pub end_byte: usize,
    pub quote: String,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Draft {
    pub schema_version: u32,
    pub origin: ViewPin,
    pub evaluated: ViewPin,
    pub knowledge_id: String,
    pub actor: String,
    pub session: String,
    pub sequence: u32,
    pub occurred_at_ms: u64,
    pub outcome: Outcome,
    pub rationale: String,
    pub source_evidence: Vec<SourceQuote>,
    pub transcript_evidence: Vec<TranscriptQuote>,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Event {
    schema_version: u32,
    id: String,
    workspace_id: String,
    draft: Draft,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RankInput {
    pub schema_version: u32,
    pub origin: ViewPin,
    pub current: ViewPin,
    pub knowledge_ids: Vec<String>,
    pub as_of_ms: u64,
    pub decay_window_days: u32,
    pub limit: usize,
}
fn label(text: &str) -> Result<()> {
    bounded_text(text, 200, false)?;
    ensure!(
        !text.chars().any(char::is_control),
        "feedback_input_invalid"
    );
    Ok(())
}
pub fn validate_options(options: &Options) -> Result<()> {
    ensure!(
        options.max_events <= 10_000
            && options.max_input_bytes <= 64_000_000
            && options.max_source_bytes <= 64_000_000
            && options.max_pairs <= 32
            && options.max_candidates <= 10_000
            && options.max_output_bytes <= 4_000_000,
        "feedback_input_invalid"
    );
    Ok(())
}
pub fn validate_draft(input: &Draft, d: &Deadline) -> Result<()> {
    d.check()?;
    crate::json::encode_bounded(input, MAX_DRAFT_BYTES, d)?;
    input.origin.validate()?;
    input.evaluated.validate()?;
    ensure!(
        input.schema_version == 1
            && input.origin.workspace_id == input.evaluated.workspace_id
            && valid_id(&input.knowledge_id)
            && (1..=1_000_000).contains(&input.sequence)
            && input.source_evidence.len() <= 32
            && input.transcript_evidence.len() <= 8
            && input.source_evidence.len() + input.transcript_evidence.len() > 0,
        "feedback_input_invalid"
    );
    label(&input.actor)?;
    label(&input.session)?;
    bounded_text(&input.rationale, 2048, false)?;
    let mut total = 0usize;
    let mut native = BTreeSet::new();
    for quote in &input.source_evidence {
        d.check()?;
        ensure!(
            crate::paths::normalized(&quote.path)? == quote.path
                && valid_id(&quote.content_hash)
                && quote.end_byte >= quote.start_byte
                && quote.end_byte - quote.start_byte == quote.quote.len()
                && quote.quote.len() <= 16_000
                && native.insert(quote),
            "feedback_input_invalid"
        );
        bounded_text(&quote.quote, 16_000, false)?;
        total += quote.quote.len();
    }
    let mut transcripts = BTreeSet::new();
    for quote in &input.transcript_evidence {
        d.check()?;
        ensure!(
            valid_id(&quote.transcript_id)
                && valid_id(&quote.content_hash)
                && quote.end_byte >= quote.start_byte
                && quote.end_byte - quote.start_byte == quote.quote.len()
                && quote.quote.len() <= 16_000
                && transcripts.insert(quote),
            "feedback_input_invalid"
        );
        bounded_text(&quote.quote, 16_000, false)?;
        total += quote.quote.len();
    }
    ensure!(total <= 32_000, "feedback_capacity_exceeded");
    d.check()
}
pub fn validate_rank(input: &RankInput, d: &Deadline) -> Result<()> {
    d.check()?;
    crate::json::encode_bounded(input, MAX_DRAFT_BYTES, d)?;
    input.origin.validate()?;
    input.current.validate()?;
    ensure!(
        input.schema_version == 1
            && input.origin.workspace_id == input.current.workspace_id
            && (1..=128).contains(&input.knowledge_ids.len())
            && (1..=128).contains(&input.limit)
            && (1..=3650).contains(&input.decay_window_days)
            && input.knowledge_ids.iter().all(|id| valid_id(id))
            && input.knowledge_ids.iter().collect::<BTreeSet<_>>().len()
                == input.knowledge_ids.len(),
        "feedback_input_invalid"
    );
    d.check()
}
fn event_id(workspace: &str, draft: &Draft) -> String {
    identity(&(EVENT_KIND, workspace, draft))
}
fn canonical(mut draft: Draft) -> Draft {
    draft.source_evidence.sort();
    draft.transcript_evidence.sort();
    draft
}
fn output<T: Serialize>(value: &T, options: &Options, d: &Deadline) -> Result<Value> {
    let bytes = crate::json::encode_bounded(value, options.max_output_bytes, d)
        .map_err(|_| anyhow::anyhow!("feedback_capacity_exceeded"))?;
    let value = crate::json::parse(&bytes, options.max_output_bytes)?;
    d.check()?;
    Ok(value)
}
fn sanitize(error: anyhow::Error, d: &Deadline) -> anyhow::Error {
    if let Err(deadline) = d.check() {
        return deadline;
    }
    let code = error
        .chain()
        .find_map(|cause| match cause.to_string().as_str() {
            "feedback_input_invalid" => Some("feedback_input_invalid"),
            "feedback_capacity_exceeded" | "comparison_capacity_exceeded" => {
                Some("feedback_capacity_exceeded")
            }
            "feedback_sequence_conflict" => Some("feedback_sequence_conflict"),
            "feedback_transaction_unavailable" => Some("feedback_transaction_unavailable"),
            _ => None,
        })
        .unwrap_or("feedback_binding_invalid");
    anyhow::anyhow!(code)
}
pub fn record(store: &Store, draft: &Draft, options: &Options, d: &Deadline) -> Result<Value> {
    storage::record(store, draft, options, d).map_err(|error| sanitize(error, d))
}
pub fn rank(store: &Store, input: &RankInput, options: &Options, d: &Deadline) -> Result<Value> {
    ranking::rank(store, input, options, d).map_err(|error| sanitize(error, d))
}
pub fn import_transcript(
    store: &Store,
    text: &[u8],
    name: &str,
    author: &str,
    options: &Options,
    d: &Deadline,
) -> Result<Value> {
    transcript::import(store, text, name, author, options, d).map_err(|error| sanitize(error, d))
}
pub fn validate_transcript(
    text: &[u8],
    name: &str,
    author: &str,
    options: &Options,
    d: &Deadline,
) -> Result<()> {
    transcript::validate(text, name, author, options, d).map_err(|error| sanitize(error, d))
}
pub fn mine(store: &Store, id: &str, options: &Options, d: &Deadline) -> Result<Value> {
    transcript::mine(store, id, options, d).map_err(|error| sanitize(error, d))
}
#[derive(Default)]
pub(crate) struct RetainedRoots {
    pub snapshot_ids: BTreeSet<String>,
    pub blob_ids: BTreeSet<String>,
}
pub(crate) fn retained_roots(store: &Store, d: &Deadline) -> Result<RetainedRoots> {
    storage::retained_roots(store, d).map_err(|error| sanitize(error, d))
}
