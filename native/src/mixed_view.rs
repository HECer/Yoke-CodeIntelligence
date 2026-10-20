//! Checked mixed queries retain distinct native, decoded-text and XML spaces.
use crate::{capture::Deadline, model::valid_id, store::Store};
use anyhow::Result;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
};
#[path = "mixed_view_query.rs"]
mod query;
pub use query::{Query, SourceRef, validate_pdf_query, validate_query};
#[path = "mixed_meaning_input.rs"]
mod meaning_input;
pub use meaning_input::{
    CheckedMeaningInputs, MeaningContextGroup, MeaningInputRequest, MeaningUnitSelection,
    validate_meaning_inputs, validate_pdf_meaning_inputs,
};
#[path = "mixed_pdf_input.rs"]
mod pdf;
pub use pdf::{
    NativePdfWorker, PDF_EXTRACTION_POLICY, PdfPin, SelectionV2, validate_pdf_selection,
};

pub const MAX_SELECTION_BYTES: usize = 64_000;
pub const MAX_OUTPUT_BYTES: usize = 4_000_000;
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SourcePin {
    pub workspace_id: String,
    pub snapshot_id: String,
    pub view_id: String,
    pub view_artifact_id: String,
    pub base_graph_hash: String,
}
impl SourcePin {
    fn native(&self) -> crate::view_comparison::SelectionPin {
        crate::view_comparison::SelectionPin {
            workspace_id: self.workspace_id.clone(),
            snapshot_id: self.snapshot_id.clone(),
            view_id: self.view_id.clone(),
            view_artifact_id: self.view_artifact_id.clone(),
            base_graph_hash: self.base_graph_hash.clone(),
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DocumentPin {
    pub document_graph_id: String,
    pub artifact_id: String,
    pub container_hash: String,
    pub decoded_text_hash: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Selection {
    pub schema_version: u8,
    pub source: SourcePin,
    pub documents: Vec<DocumentPin>,
}
#[derive(Clone, Debug)]
pub struct Options {
    pub max_documents: usize,
    pub max_metadata_bytes: usize,
    pub max_source_bytes: usize,
    pub max_matches: usize,
    pub max_output_bytes: usize,
}
impl Default for Options {
    fn default() -> Self {
        Self {
            max_documents: 4,
            max_metadata_bytes: 64_000_000,
            max_source_bytes: 64_000_000,
            max_matches: 100_000,
            max_output_bytes: MAX_OUTPUT_BYTES,
        }
    }
}
#[derive(Debug)]
pub(crate) struct Refusal(&'static str);
impl fmt::Display for Refusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.0)
    }
}
impl std::error::Error for Refusal {}
pub(crate) fn failure(code: &'static str) -> anyhow::Error {
    Refusal(code).into()
}
pub(crate) fn input(ok: bool) -> Result<()> {
    if ok {
        Ok(())
    } else {
        Err(failure("mixed_view_input_invalid"))
    }
}
pub(crate) fn capacity(ok: bool) -> Result<()> {
    if ok {
        Ok(())
    } else {
        Err(failure("mixed_view_capacity_exceeded"))
    }
}
pub(crate) fn parent(ok: bool) -> Result<()> {
    if ok {
        Ok(())
    } else {
        Err(failure("mixed_view_parent_refused"))
    }
}
pub(crate) fn pin(ok: bool) -> Result<()> {
    if ok {
        Ok(())
    } else {
        Err(failure("mixed_view_pin_mismatch"))
    }
}
pub(crate) fn check(d: &Deadline) -> Result<()> {
    d.check()
        .map_err(|_| failure("mixed_view_deadline_exceeded"))
}
fn sanitized(error: anyhow::Error, d: &Deadline) -> anyhow::Error {
    for cause in error.chain() {
        if let Some(code) = cause.downcast_ref::<Refusal>() {
            return failure(code.0);
        }
        if let Some(rusqlite::Error::SqliteFailure(code, _)) =
            cause.downcast_ref::<rusqlite::Error>()
            && code.code == rusqlite::ErrorCode::TooBig
        {
            return failure("mixed_view_capacity_exceeded");
        }
        if matches!(
            cause.to_string().as_str(),
            "comparison_capacity_exceeded"
                | "Meaning dependency exceeds remaining input capacity"
                | "JSON output exceeds byte capacity"
        ) {
            return failure("mixed_view_capacity_exceeded");
        }
    }
    if check(d).is_err() {
        failure("mixed_view_deadline_exceeded")
    } else {
        failure("mixed_view_parent_refused")
    }
}
pub fn validate_options(o: &Options) -> Result<()> {
    input(
        o.max_documents <= 4
            && o.max_metadata_bytes <= 64_000_000
            && o.max_source_bytes <= 64_000_000
            && o.max_matches <= 100_000
            && o.max_output_bytes <= MAX_OUTPUT_BYTES,
    )
}
pub fn validate_selection(s: &Selection, o: &Options, d: &Deadline) -> Result<()> {
    validate_options(o)?;
    check(d)?;
    input(s.schema_version == 1 && !s.documents.is_empty())?;
    capacity(s.documents.len() <= o.max_documents)?;
    validate_source_pin(&s.source)?;
    validate_document_pins(&s.documents, d)?;
    encode(s, MAX_SELECTION_BYTES, d)?;
    check(d)
}
fn validate_source_pin(source: &SourcePin) -> Result<()> {
    input(
        [
            &source.workspace_id,
            &source.snapshot_id,
            &source.view_id,
            &source.view_artifact_id,
            &source.base_graph_hash,
        ]
        .into_iter()
        .all(|v| valid_id(v)),
    )
}
fn validate_document_pins(documents: &[DocumentPin], d: &Deadline) -> Result<()> {
    let mut seen = BTreeSet::new();
    for p in documents {
        check(d)?;
        input(
            [
                &p.document_graph_id,
                &p.artifact_id,
                &p.container_hash,
                &p.decoded_text_hash,
            ]
            .into_iter()
            .all(|v| valid_id(v))
                && seen.insert(&p.document_graph_id),
        )?;
    }
    check(d)
}
pub(crate) fn encode<T: Serialize>(v: &T, limit: usize, d: &Deadline) -> Result<Vec<u8>> {
    crate::json::encode_bounded(v, limit, d).map_err(|_| {
        if check(d).is_err() {
            failure("mixed_view_deadline_exceeded")
        } else {
            failure("mixed_view_capacity_exceeded")
        }
    })
}
pub(crate) fn output<T: Serialize>(v: &T, limit: usize, d: &Deadline) -> Result<Value> {
    let bytes = encode(v, limit, d)?;
    let value = crate::json::parse(&bytes, limit)?;
    check(d)?;
    Ok(value)
}
/// Only joint checked replay can construct this immutable capability.
pub struct CheckedMixedView {
    selection: Selection,
    source: crate::graph_view::comparison_input::CheckedComparisonPair<'static>,
    documents: Vec<crate::document_graph::CheckedDocumentGraph>,
    pdf_selection: Option<SelectionV2>,
    pdfs: Vec<pdf::CheckedPdf>,
    options: Options,
    metadata_bytes: usize,
    source_bytes: usize,
}
impl CheckedMixedView {
    pub fn query(&self, q: &Query, max_output_bytes: usize, d: &Deadline) -> Result<Value> {
        query::run(self, q, max_output_bytes, d).map_err(|e| sanitized(e, d))
    }
}
pub fn load(store: &Store, s: &Selection, o: &Options, d: &Deadline) -> Result<CheckedMixedView> {
    validate_selection(s, o, d).map_err(|e| sanitized(e, d))?;
    load_inner(store, s, None, None, o, d)
}
pub fn load_with_pdf(
    store: &Store,
    s: &SelectionV2,
    worker: &NativePdfWorker,
    o: &Options,
    d: &Deadline,
) -> Result<CheckedMixedView> {
    validate_pdf_selection(s, o, d).map_err(|e| sanitized(e, d))?;
    let mut selected = s.clone();
    selected
        .documents
        .sort_by(|a, b| a.document_graph_id.cmp(&b.document_graph_id));
    selected.pdfs.sort_by(|a, b| a.asset_id.cmp(&b.asset_id));
    load_inner(
        store,
        &selected.native_selection(),
        Some(&selected),
        Some(worker),
        o,
        d,
    )
}
fn load_inner(
    store: &Store,
    s: &Selection,
    pdf_selection: Option<&SelectionV2>,
    worker: Option<&NativePdfWorker>,
    o: &Options,
    d: &Deadline,
) -> Result<CheckedMixedView> {
    let run = || -> Result<CheckedMixedView> {
        capacity(o.max_metadata_bytes > 0 && o.max_source_bytes > 0)?;
        pin(s.source.workspace_id == store.workspace_id)?;
        let mut selection = s.clone();
        selection
            .documents
            .sort_by(|a, b| a.document_graph_id.cmp(&b.document_graph_id));
        store.set_deadline(d)?;
        let transaction = if store.db.is_autocommit() {
            Some(store.db.unchecked_transaction()?)
        } else {
            None
        };
        let admitted =
            crate::document_graph::mixed_input::prepare(store, &selection.documents, o, d)?;
        let pdf = pdf::prepare(
            store,
            pdf_selection.map_or(&[][..], |s| s.pdfs.as_slice()),
            o.max_metadata_bytes
                .checked_sub(admitted.metadata_bytes())
                .ok_or_else(|| failure("mixed_view_capacity_exceeded"))?,
            o.max_source_bytes
                .checked_sub(admitted.source_bytes())
                .ok_or_else(|| failure("mixed_view_capacity_exceeded"))?,
            d,
        )?;
        let document_source_bytes = admitted
            .source_bytes()
            .checked_add(pdf.source_bytes)
            .ok_or_else(|| failure("mixed_view_capacity_exceeded"))?;
        let document_metadata_bytes = admitted
            .metadata_bytes()
            .checked_add(pdf.metadata_bytes)
            .ok_or_else(|| failure("mixed_view_capacity_exceeded"))?;
        let mut native_sources = BTreeMap::new();
        let mut native_source_bytes = 0usize;
        let remaining = o
            .max_source_bytes
            .checked_sub(document_source_bytes)
            .ok_or_else(|| failure("mixed_view_capacity_exceeded"))?;
        let mut preflight = |scope: &crate::model::Snapshot, d: &Deadline| -> Result<()> {
            check(d)?;
            for file in &scope.files {
                check(d)?;
                if let Some(size) = native_sources.get(&file.hash) {
                    parent(*size == file.size)?;
                } else {
                    native_source_bytes = native_source_bytes
                        .checked_add(file.size)
                        .ok_or_else(|| failure("mixed_view_capacity_exceeded"))?;
                    capacity(native_source_bytes <= remaining)?;
                    native_sources.insert(file.hash.clone(), file.size);
                }
            }
            Ok(())
        };
        let options = crate::view_comparison::ComparisonOptions {
            max_input_bytes: o
                .max_metadata_bytes
                .checked_sub(document_metadata_bytes)
                .ok_or_else(|| failure("mixed_view_capacity_exceeded"))?,
            max_source_bytes: remaining,
            ..Default::default()
        };
        let pin = selection.source.native();
        let source =
            crate::graph_view::comparison_input::prepare_published_pair_with_scope_preflight(
                store,
                &pin,
                &pin,
                &options,
                &mut preflight,
                d,
            )?;
        let metadata_bytes = source
            .accounting()
            .total_proof_bytes()
            .checked_add(document_metadata_bytes)
            .ok_or_else(|| failure("mixed_view_capacity_exceeded"))?;
        let source_bytes = source
            .accounting()
            .distinct_source_bytes()
            .checked_add(document_source_bytes)
            .ok_or_else(|| failure("mixed_view_capacity_exceeded"))?;
        capacity(metadata_bytes <= o.max_metadata_bytes && source_bytes <= o.max_source_bytes)?;
        let documents = admitted.load(store, d)?;
        let pdfs = match worker {
            Some(worker) => pdf.load(store, worker, d)?,
            None => {
                parent(pdf_selection.is_none())?;
                Vec::new()
            }
        };
        if let Some(transaction) = transaction {
            transaction.commit()?;
        }
        check(d)?;
        Ok(CheckedMixedView {
            selection,
            source,
            documents,
            pdf_selection: pdf_selection.cloned(),
            pdfs,
            options: o.clone(),
            metadata_bytes,
            source_bytes,
        })
    };
    run().map_err(|e| sanitized(e, d))
}
