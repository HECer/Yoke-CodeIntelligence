//! Explicit data preparation from an opaque checked mixed view. No provider authority.
use super::*;
use crate::model::hash;

const CONTRACT: &str = "mixed_meaning_inputs_v1";
const MAX_REQUEST_BYTES: usize = 64_000;
const MAX_UNIT_TEXT_BYTES: usize = 16_384;
const MAX_ITEM_BYTES: usize = 256_000;
const MAX_CONTEXT_TEXT_BYTES: usize = 1_000_000;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MeaningUnitSelection {
    pub label: String,
    pub source: SourceRef,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MeaningContextGroup {
    pub label: String,
    pub units: Vec<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MeaningInputRequest {
    pub schema_version: u8,
    pub units: Vec<MeaningUnitSelection>,
    #[serde(default)]
    pub context_groups: Vec<MeaningContextGroup>,
}
fn label(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}
fn validate(
    r: &MeaningInputRequest,
    source: &Selection,
    pdf: Option<&SelectionV2>,
    d: &Deadline,
) -> Result<()> {
    check(d)?;
    input(
        r.schema_version == 1 && (1..=16).contains(&r.units.len()) && r.context_groups.len() <= 16,
    )?;
    encode(r, MAX_REQUEST_BYTES, d)?;
    let mut labels = BTreeSet::new();
    for unit in &r.units {
        check(d)?;
        input(label(&unit.label) && labels.insert(unit.label.as_str()))?;
        let q = Query::Read {
            source: unit.source.clone(),
        };
        if let Some(pdf) = pdf {
            validate_pdf_query(&q, pdf, d)?;
        } else {
            validate_query(&q, source, d)?;
        }
        let (start, end) = match &unit.source {
            SourceRef::SourceUtf8 {
                start_byte,
                end_byte,
                ..
            }
            | SourceRef::DocumentDecodedText {
                start_byte,
                end_byte,
                ..
            }
            | SourceRef::DocumentPartXml {
                start_byte,
                end_byte,
                ..
            }
            | SourceRef::PdfPageText {
                start_byte,
                end_byte,
                ..
            } => (*start_byte, *end_byte),
        };
        input(start < end && end - start <= MAX_UNIT_TEXT_BYTES)?;
    }
    let mut groups = BTreeSet::new();
    for group in &r.context_groups {
        check(d)?;
        input(
            label(&group.label)
                && groups.insert(group.label.as_str())
                && (1..=16).contains(&group.units.len()),
        )?;
        input(
            group
                .units
                .iter()
                .all(|label| labels.contains(label.as_str())),
        )?;
    }
    check(d)
}
pub fn validate_meaning_inputs(r: &MeaningInputRequest, s: &Selection, d: &Deadline) -> Result<()> {
    validate(r, s, None, d)
}
pub fn validate_pdf_meaning_inputs(
    r: &MeaningInputRequest,
    s: &SelectionV2,
    d: &Deadline,
) -> Result<()> {
    validate(r, &s.native_selection(), Some(s), d)
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Unit<'a> {
    id: String,
    label: &'a str,
    text_bytes: usize,
    item: Value,
    #[serde(skip)]
    source: &'a SourceRef,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Context<'a> {
    label: &'a str,
    unit_ids: Vec<String>,
    text_bytes: usize,
}
/// Only constructed by re-reading typed references from an already checked view.
/// Exported JSON is data; it cannot deserialize into this capability.
pub struct CheckedMeaningInputs<'a> {
    view: &'a CheckedMixedView,
    input_hash: String,
    units: Vec<Unit<'a>>,
    contexts: Vec<Context<'a>>,
    unit_text_bytes: usize,
    context_text_bytes: usize,
}
impl CheckedMixedView {
    pub fn prepare_meaning_inputs<'a>(
        &'a self,
        request: &'a MeaningInputRequest,
        d: &Deadline,
    ) -> Result<CheckedMeaningInputs<'a>> {
        prepare(self, request, d).map_err(|e| sanitized(e, d))
    }
}
fn prepare<'a>(
    view: &'a CheckedMixedView,
    request: &'a MeaningInputRequest,
    d: &Deadline,
) -> Result<CheckedMeaningInputs<'a>> {
    validate(request, &view.selection, view.pdf_selection.as_ref(), d)?;
    let selection = if let Some(pdf) = &view.pdf_selection {
        encode(pdf, MAX_SELECTION_BYTES, d)?
    } else {
        encode(&view.selection, MAX_SELECTION_BYTES, d)?
    };
    let selection_hash = hash(&selection);
    let request_hash = hash(&encode(request, MAX_REQUEST_BYTES, d)?);
    let mut units = Vec::new();
    let mut unit_text_bytes = 0;
    let side = view.source.before();
    let scope = side.bind_scope(side.primary_snapshot(), d)?;
    for unit in &request.units {
        check(d)?;
        let item = query::read_item(view, &unit.source, &scope, d)?;
        let text_bytes = item.text.len();
        let item_bytes = encode(&item, MAX_ITEM_BYTES, d)?;
        let item_hash = hash(&item_bytes);
        let id = hash(&encode(
            &(CONTRACT, &selection_hash, &unit.label, item_hash),
            MAX_REQUEST_BYTES,
            d,
        )?);
        unit_text_bytes += text_bytes;
        units.push(Unit {
            id,
            label: &unit.label,
            text_bytes,
            item: crate::json::parse(&item_bytes, MAX_ITEM_BYTES)?,
            source: &unit.source,
        });
    }
    let mut contexts = Vec::new();
    let mut context_text_bytes = 0;
    for group in &request.context_groups {
        let mut unit_ids = Vec::new();
        let mut text_bytes = 0;
        for label in &group.units {
            check(d)?;
            let unit = units
                .iter()
                .find(|u| u.label == label)
                .ok_or_else(|| failure("mixed_view_input_invalid"))?;
            // Each occurrence counts, including repeated selections of the same unit.
            text_bytes += unit.text_bytes;
            unit_ids.push(unit.id.clone());
        }
        context_text_bytes += text_bytes;
        capacity(context_text_bytes <= MAX_CONTEXT_TEXT_BYTES)?;
        contexts.push(Context {
            label: &group.label,
            unit_ids,
            text_bytes,
        });
    }
    let input_hash = hash(&encode(
        &(
            CONTRACT,
            selection_hash,
            request_hash,
            units.iter().map(|u| &u.id).collect::<Vec<_>>(),
        ),
        MAX_REQUEST_BYTES,
        d,
    )?);
    check(d)?;
    Ok(CheckedMeaningInputs {
        view,
        input_hash,
        units,
        contexts,
        unit_text_bytes,
        context_text_bytes,
    })
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Export<'a, 'b> {
    kind: &'static str,
    input_hash: &'a str,
    mixed_view: query::Metadata<'b>,
    units: &'a [Unit<'b>],
    context_groups: &'a [Context<'b>],
    unit_text_bytes: usize,
    context_text_bytes: usize,
    model_calls: u8,
    source_written: bool,
    complete: bool,
    limitations: [&'static str; 4],
}
impl CheckedMeaningInputs<'_> {
    pub fn input_hash(&self) -> &str {
        &self.input_hash
    }
    pub(super) fn view(&self) -> &CheckedMixedView {
        self.view
    }
    pub(super) fn sources(&self) -> impl Iterator<Item = (&str, &SourceRef)> {
        self.units
            .iter()
            .map(|unit| (unit.id.as_str(), unit.source))
    }
    /// Provider transport is data only. The checked capability and original
    /// receipt/source metadata remain local; exact submitted bytes are explicit.
    pub(super) fn provider_payload(&self, contract_hash: &str, d: &Deadline) -> Result<Value> {
        check(d)?;
        input(valid_id(contract_hash))?;
        let mut units = Vec::new();
        for unit in &self.units {
            check(d)?;
            let text = unit
                .item
                .get("text")
                .and_then(Value::as_str)
                .ok_or_else(|| failure("mixed_view_parent_refused"))?;
            units.push(serde_json::json!({"sourceId":unit.id,"source":unit.source,
                "text":text,"omittedBytes":0}));
        }
        output(
            &serde_json::json!({"kind":"mixed_meaning_provider_input_v1",
            "inputHash":self.input_hash,"contractHash":contract_hash,
            "units":units,"contextGroups":self.contexts}),
            1_000_000,
            d,
        )
    }
    pub fn export(&self, max_output_bytes: usize, d: &Deadline) -> Result<Value> {
        input(max_output_bytes <= MAX_OUTPUT_BYTES)?;
        let limit = max_output_bytes.min(self.view.options.max_output_bytes);
        capacity(limit > 0)?;
        output(&Export {
            kind: CONTRACT,
            input_hash: &self.input_hash,
            mixed_view: query::metadata(self.view, d)?,
            units: &self.units,
            context_groups: &self.contexts,
            unit_text_bytes: self.unit_text_bytes,
            context_text_bytes: self.context_text_bytes,
            model_calls: 0,
            source_written: false,
            complete: false,
            limitations: [
                "Prepared data only; no semantic claims, graph relationships or provider execution.",
                "Host labels and context order are selections, not semantic identity or corroboration.",
                "Decoded text, PDF page text and part XML retain their original coordinate spaces; no native source authority is promoted.",
                "Text byte counts include repeated context occurrences, not provider wire bytes, tokens or cost.",
            ],
        }, limit, d).map_err(|e| sanitized(e, d))
    }
}
