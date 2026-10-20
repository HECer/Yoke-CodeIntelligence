//! Source-checked host-submitted interpretations; no provider receipt is invented.
use super::*;
use crate::{
    meaning_graph::{self, Fragment, Support},
    model::hash,
};

const KIND: &str = "mixed_meaning_claims_v1";
pub const MAX_CLAIM_BYTES: usize = 1_000_000;
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MixedMeaningClaimRequest {
    pub schema_version: u8,
    pub input_hash: String,
    pub fragment: Fragment,
}
fn invalid() -> anyhow::Error {
    failure("mixed_meaning_claim_invalid")
}
pub fn validate_meaning_claim_request(
    request: &MixedMeaningClaimRequest,
    d: &Deadline,
) -> Result<()> {
    check(d)?;
    if request.schema_version != 1
        || !valid_id(&request.input_hash)
        || request.fragment.concepts.len() > 128
        || request.fragment.relations.len() > 256
        || request.fragment.hyperedges.len() > 64
        || !(1..=16).contains(&request.fragment.coverage.len())
    {
        return Err(invalid());
    }
    encode(request, MAX_CLAIM_BYTES, d)?;
    check(d)
}
pub(super) fn range(source: &SourceRef) -> (usize, usize) {
    match source {
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
    }
}
pub(super) fn quote_source(source: &SourceRef, span: &Support) -> SourceRef {
    let mut source = source.clone();
    let (start, end) = match &mut source {
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
        } => (start_byte, end_byte),
    };
    *start = span.start_byte;
    *end = span.end_byte;
    source
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Quote {
    support: Support,
    item: Value,
}
/// The original checked inputs remain borrowed. A host JSON graph alone cannot
/// construct this capability or establish provider/canonical interpretation truth.
pub struct CheckedMixedMeaningClaims<'inputs, 'source> {
    inputs: &'inputs CheckedMeaningInputs<'source>,
    request: MixedMeaningClaimRequest,
    id: String,
    quotes: Vec<Quote>,
}
impl<'source> CheckedMeaningInputs<'source> {
    pub fn check_meaning_claims<'inputs>(
        &'inputs self,
        request: &MixedMeaningClaimRequest,
        d: &Deadline,
    ) -> Result<CheckedMixedMeaningClaims<'inputs, 'source>> {
        let checked = (|| {
            validate_meaning_claim_request(request, d)?;
            if request.input_hash != self.input_hash() {
                return Err(invalid());
            }
            let sources: BTreeMap<_, _> = self.sources().collect();
            let ranges = sources
                .iter()
                .map(|(id, source)| (*id, range(source)))
                .collect();
            meaning_graph::validate_fragment_ranges(&request.fragment, &ranges, d).map_err(
                |_| {
                    if check(d).is_err() {
                        failure("mixed_view_deadline_exceeded")
                    } else {
                        invalid()
                    }
                },
            )?;
            let spans: BTreeSet<_> = request
                .fragment
                .concepts
                .iter()
                .flat_map(|c| &c.support)
                .chain(request.fragment.relations.iter().flat_map(|c| &c.support))
                .chain(request.fragment.hyperedges.iter().flat_map(|c| &c.support))
                .collect();
            let view = self.view();
            let side = view.source.before();
            let scope = side.bind_scope(side.primary_snapshot(), d)?;
            let mut quotes = Vec::new();
            let mut quote_bytes = 0usize;
            let mut encoded_bytes = 0usize;
            for span in spans {
                check(d)?;
                let source = sources.get(span.source_id.as_str()).ok_or_else(invalid)?;
                let source = quote_source(source, span);
                let item = query::read_item(view, &source, &scope, d)?;
                quote_bytes += item.text.len();
                capacity(quote_bytes <= 128_000)?;
                let bytes = encode(&item, 256_000, d)?;
                encoded_bytes += bytes.len();
                capacity(encoded_bytes <= MAX_OUTPUT_BYTES)?;
                quotes.push(Quote {
                    support: span.clone(),
                    item: crate::json::parse(&bytes, 256_000)?,
                });
            }
            let id = hash(&encode(&(KIND, request), MAX_CLAIM_BYTES + 128, d)?);
            check(d)?;
            Ok(CheckedMixedMeaningClaims {
                inputs: self,
                request: request.clone(),
                id,
                quotes,
            })
        })();
        checked.map_err(|e| sanitized(e, d))
    }
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Export<'a> {
    kind: &'static str,
    id: &'a str,
    input_hash: &'a str,
    inputs: Value,
    original_fragment: &'a Fragment,
    support_quotes: &'a [Quote],
    interpretation_origin: &'static str,
    provider_attestation: &'static str,
    canonical_source_authority: bool,
    complete: bool,
    model_calls: u8,
    source_written: bool,
    limitations: [&'static str; 3],
}
impl CheckedMixedMeaningClaims<'_, '_> {
    pub fn export(&self, max_output_bytes: usize, d: &Deadline) -> Result<Value> {
        input(max_output_bytes <= MAX_OUTPUT_BYTES)?;
        let limit = max_output_bytes.min(self.inputs.view().options.max_output_bytes);
        capacity(limit > 0)?;
        self.export_data(limit, limit, false, d)
    }
    pub(super) fn provider_validation_export(&self, limit: usize, d: &Deadline) -> Result<Value> {
        input((1..=64_000_000).contains(&limit))?;
        self.export_data(limit, MAX_OUTPUT_BYTES, true, d)
    }
    fn export_data(
        &self,
        limit: usize,
        input_limit: usize,
        provider: bool,
        d: &Deadline,
    ) -> Result<Value> {
        let inputs = self.inputs.export(input_limit, d)?;
        let provider_id = crate::model::hash(&encode(
            &("mixed_meaning_provider_claim_validation_v1", &self.id),
            256,
            d,
        )?);
        output(&Export {
            kind: if provider { "mixed_meaning_provider_claim_validation_v1" } else { KIND },
            id: if provider { &provider_id } else { &self.id }, input_hash: self.inputs.input_hash(), inputs,
            original_fragment: &self.request.fragment, support_quotes: &self.quotes,
            interpretation_origin: if provider { "provider_reported_unverified_interpretation" } else { "host_submitted_unverified_interpretation" },
            provider_attestation: if provider { "unverified" } else { "not_established" }, canonical_source_authority: false,
            complete: false, model_calls: 0, source_written: false,
            limitations: [
                "Exact checked source support does not establish interpretation correctness, concept identity or relationship truth.",
                "Original concept keys, claim order, parallel directed relations and ordered hyperedge participants remain separate observations.",
                if provider { "This source-validation component performs no provider call of its own; the enclosing operation carries separate accounting and received-response provenance." }
                else { "This read-only check performs no semantic provider execution, creates no provider receipt and persists no graph." },
            ],
        }, limit, d).map_err(|e| sanitized(e, d))
    }
}
