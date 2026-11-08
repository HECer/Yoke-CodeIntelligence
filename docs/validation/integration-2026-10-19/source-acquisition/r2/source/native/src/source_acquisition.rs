//! Frozen received HTTP representation bodies; no imported acquisition authority.
use crate::capture::Deadline;
use anyhow::{Result, ensure};
use base64::Engine as _;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Format {
    Text,
    Html,
    Pdf,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Limits {
    pub max_body_bytes: usize,
    pub max_archive_bytes: usize,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            max_body_bytes: 10_000_000,
            max_archive_bytes: 16_000_000,
        }
    }
}
impl Limits {
    pub fn validate(&self) -> Result<()> {
        input(
            self.max_body_bytes > 0
                && self.max_body_bytes <= 10_000_000
                && self.max_archive_bytes > 0
                && self.max_archive_bytes <= 16_000_000,
        )
    }
}
pub(crate) fn input(ok: bool) -> Result<()> {
    ensure!(ok, "source_acquisition_input_invalid");
    Ok(())
}
pub(crate) fn capacity(ok: bool) -> Result<()> {
    ensure!(ok, "source_acquisition_capacity_exceeded");
    Ok(())
}
pub(crate) fn check(d: &Deadline) -> Result<()> {
    d.check()
        .map_err(|_| anyhow::anyhow!("source_acquisition_deadline_exceeded"))
}
pub(crate) fn url(value: &str) -> Result<url::Url> {
    input(
        !value.is_empty()
            && value.len() <= 4096
            && value.is_ascii()
            && !value
                .bytes()
                .any(|b| b.is_ascii_control() || b.is_ascii_whitespace())
            && !value.contains('\\')
            && !crate::paths::obvious_secret(value),
    )?;
    let parsed =
        url::Url::parse(value).map_err(|_| anyhow::anyhow!("source_acquisition_input_invalid"))?;
    input(
        matches!(parsed.scheme(), "https" | "http")
            && parsed.host_str().is_some()
            && parsed.username().is_empty()
            && parsed.password().is_none()
            && parsed.fragment().is_none()
            && parsed.as_str() == value,
    )?;
    Ok(parsed)
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Metadata {
    pub schema_version: u8,
    pub format: Format,
    pub requested_url: String,
    pub http_status: u16,
    pub content_type: String,
    pub etag: Option<String>,
    pub last_modified: Option<String>,
}
fn header(value: &str, max: usize) -> Result<()> {
    input(
        value.len() <= max
            && value.is_ascii()
            && !value.bytes().any(|b| b.is_ascii_control())
            && !crate::paths::obvious_secret(value),
    )
}
impl Metadata {
    fn validate(&self, body: &[u8]) -> Result<()> {
        input(self.schema_version == 1 && self.http_status == 200)?;
        url(&self.requested_url)?;
        header(&self.content_type, 512)?;
        if let Some(e) = &self.etag {
            header(e, 512)?;
        }
        if let Some(e) = &self.last_modified {
            header(e, 200)?;
        }
        let mime = self
            .content_type
            .split(';')
            .next()
            .unwrap_or("")
            .trim()
            .to_ascii_lowercase();
        match self.format {
            Format::Text => {
                input(mime == "text/plain")?;
                let text = std::str::from_utf8(body)
                    .map_err(|_| anyhow::anyhow!("source_acquisition_text_encoding_unsupported"))?;
                input(!text.contains('\0') && !crate::paths::obvious_secret(text))?;
                for parameter in self.content_type.split(';').skip(1) {
                    let parameter = parameter.trim();
                    if let Some((key, value)) = parameter.split_once('=') {
                        if key.eq_ignore_ascii_case("charset") {
                            input(matches!(
                                value.trim_matches('"').to_ascii_lowercase().as_str(),
                                "utf-8" | "utf8" | "us-ascii"
                            ))?;
                        }
                    }
                }
            }
            Format::Html => input(mime == "text/html")?,
            Format::Pdf => input(mime == "application/pdf" && body.starts_with(b"%PDF-"))?,
        }
        Ok(())
    }
}
pub struct ReceivedSource {
    metadata: Metadata,
    body: Vec<u8>,
    value: Value,
    limits: Limits,
    claims: Option<Value>,
}
impl ReceivedSource {
    pub fn from_response(metadata: &Value, body: &[u8], d: &Deadline) -> Result<Self> {
        Self::from_response_with_limits(metadata, body, &Limits::default(), d)
    }
    pub fn from_response_with_limits(
        metadata: &Value,
        body: &[u8],
        limits: &Limits,
        d: &Deadline,
    ) -> Result<Self> {
        limits.validate()?;
        check(d)?;
        capacity(body.len() <= limits.max_body_bytes)?;
        crate::json::encode_bounded(metadata, 16384, d)
            .map_err(|_| anyhow::anyhow!("source_acquisition_capacity_exceeded"))?;
        let meta: Metadata = crate::json::decode(metadata.clone())
            .map_err(|_| anyhow::anyhow!("source_acquisition_input_invalid"))?;
        meta.validate(body)?;
        check(d)?;
        let hash = crate::model::hash(body);
        let id = crate::model::identity(&("received-http-representation-v1", &meta, &hash));
        let value = json!({"schemaVersion":1,"kind":"native_received_http_source_v1","sourceId":id,"sourceHash":hash,"bodyBytes":body.len(),"metadata":meta,
            "sourcePrecision":"received_http_representation_body_bytes","headerPrecision":"selected_client_normalized_fields_not_original_wire_headers",
            "textExtraction":match meta.format {Format::Text=>"literal_utf8",Format::Html=>"not_performed_no_article_or_browser_dom_claim",Format::Pdf=>"requires_explicit_native_pdf_worker"},
            "sourceAuthority":false,"serverAttestation":"unverified","canonicalCodeSnapshot":false,"networkConnections":0,"networkConnectionCountScope":"received_bytes_replay_only","modelCalls":0,"projectCodeExecutionAuthorized":false,"complete":false,
            "limitations":["This is a received representation and structural artifact, not attested server truth, semantic content, original HTTP wire bytes or a code SourceSnapshot.",
                "URLs, HTTP status, content type, entity tags, timestamps and capture claims are unverified data. Import grants no endpoint, credential, network, filesystem or execution authority.",
                "HTML is preserved as bytes; scripts, embedded resources, links and instructions are never executed or followed. No article, tweet, video, ASR, OCR or semantic graph is inferred.",
                "Frozen byte queries perform no acquisition. Raw body ranges refer to the received body only; native PDF extraction is a separate explicitly selected bounded worker operation."]});
        check(d)?;
        Ok(Self {
            metadata: meta,
            body: body.to_vec(),
            value,
            limits: limits.clone(),
            claims: None,
        })
    }
    pub fn as_value(&self) -> Value {
        self.value.clone()
    }
    pub fn body(&self) -> &[u8] {
        &self.body
    }
    pub fn format(&self) -> Format {
        self.metadata.format
    }
    pub fn capture_claims(&self) -> Option<&Value> {
        self.claims.as_ref()
    }
    pub(crate) fn set_claims(&mut self, value: Value) {
        self.claims = Some(value);
    }
    pub fn read_range(
        &self,
        start: usize,
        end: usize,
        max_bytes: usize,
        d: &Deadline,
    ) -> Result<Value> {
        check(d)?;
        input(start <= end && end <= self.body.len())?;
        capacity(end - start <= 65536 && max_bytes > 0 && max_bytes <= 16_000_000)?;
        let bytes = &self.body[start..end];
        let value = json!({"kind":"native_received_http_body_range_v1","sourceId":self.value["sourceId"],"sourceHash":self.value["sourceHash"],"startByte":start,"endByte":end,"totalBytes":self.body.len(),"base64":base64::prelude::BASE64_STANDARD.encode(bytes),"utf8":std::str::from_utf8(bytes).ok(),"sourcePrecision":"received_http_representation_body_bytes","sourceAuthority":false,"networkConnections":0,"modelCalls":0,"complete":false});
        crate::json::encode_bounded(&value, max_bytes, d)
            .map_err(|_| anyhow::anyhow!("source_acquisition_capacity_exceeded"))?;
        check(d)?;
        Ok(value)
    }
    pub fn encode_archive(&self, d: &Deadline) -> Result<Vec<u8>> {
        check(d)?;
        let value = Archive {
            schema_version: 1,
            kind: "native_received_http_archive_v1".into(),
            source_hash: crate::model::hash(&self.body),
            metadata: self.metadata.clone(),
            body_base64: base64::prelude::BASE64_STANDARD.encode(&self.body),
            capture_claims: self.claims.clone(),
        };
        let bytes = crate::json::encode_bounded(&value, self.limits.max_archive_bytes, d)
            .map_err(|_| anyhow::anyhow!("source_acquisition_capacity_exceeded"))?;
        check(d)?;
        Ok(bytes)
    }
    pub fn load_archive(bytes: &[u8], d: &Deadline) -> Result<Self> {
        Self::load_archive_with_limits(bytes, &Limits::default(), d)
    }
    pub fn load_archive_with_limits(bytes: &[u8], limits: &Limits, d: &Deadline) -> Result<Self> {
        limits.validate()?;
        check(d)?;
        capacity(bytes.len() <= limits.max_archive_bytes)?;
        let a: Archive = crate::json::parse(bytes, limits.max_archive_bytes)
            .and_then(crate::json::decode)
            .map_err(|_| anyhow::anyhow!("source_acquisition_input_invalid"))?;
        input(
            a.schema_version == 1
                && a.kind == "native_received_http_archive_v1"
                && crate::model::valid_id(&a.source_hash),
        )?;
        capacity(a.body_base64.len() <= limits.max_body_bytes.div_ceil(3) * 4)?;
        if let Some(c) = &a.capture_claims {
            crate::json::encode_bounded(c, 16384, d)
                .map_err(|_| anyhow::anyhow!("source_acquisition_capacity_exceeded"))?;
        }
        let body = base64::prelude::BASE64_STANDARD
            .decode(a.body_base64)
            .map_err(|_| anyhow::anyhow!("source_acquisition_input_invalid"))?;
        capacity(body.len() <= limits.max_body_bytes)?;
        input(crate::model::hash(&body) == a.source_hash)?;
        let mut source =
            Self::from_response_with_limits(&serde_json::to_value(a.metadata)?, &body, limits, d)?;
        source.claims = a.capture_claims;
        check(d)?;
        Ok(source)
    }
}
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Archive {
    schema_version: u8,
    kind: String,
    source_hash: String,
    metadata: Metadata,
    body_base64: String,
    capture_claims: Option<Value>,
}
