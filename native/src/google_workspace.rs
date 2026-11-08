//! Immutable received Google Workspace exports; imported claims grant no authority.
//! The exact received representation is separate from its original shortcut hash.
use crate::{capture::Deadline, model};
use anyhow::{Result, ensure};
use base64::{Engine as _, prelude::BASE64_STANDARD};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
#[path = "google_workspace_shortcut.rs"]
mod shortcut;
pub use shortcut::{Kind, Metadata, Shortcut};
#[path = "google_workspace_capture.rs"]
mod acquisition;
pub use acquisition::{CaptureSpec, ClientSpec, Permissions, capture};
#[path = "google_workspace_http.rs"]
mod native_http;
pub use native_http::{
    HostAccessToken, HttpPermissions, HttpSpec, capture_http, plan_http, read_private_token_stdin,
};

const KIND: &str = "native_google_workspace_export_v1";
const MAX_BODY_BYTES: usize = 10_000_000;
const MAX_ARCHIVE_BYTES: usize = 16_000_000;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Limits {
    pub max_body_bytes: usize,
    pub max_archive_bytes: usize,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            max_body_bytes: MAX_BODY_BYTES,
            max_archive_bytes: MAX_ARCHIVE_BYTES,
        }
    }
}
impl Limits {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            (1..=MAX_BODY_BYTES).contains(&self.max_body_bytes)
                && (1..=MAX_ARCHIVE_BYTES).contains(&self.max_archive_bytes),
            "google_workspace_limits_invalid"
        );
        Ok(())
    }
}

/// These fields record a client claim, not attestation of remote account access.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ExporterClaim {
    pub executable_sha256: String,
    pub version: String,
}
impl ExporterClaim {
    fn validate(&self) -> Result<()> {
        shortcut::digest(&self.executable_sha256)?;
        ensure!(
            !self.version.is_empty()
                && self.version.len() <= 64
                && self
                    .version
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b'_')),
            "google_workspace_exporter_claim_invalid"
        );
        Ok(())
    }
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Envelope {
    schema_version: u8,
    kind: String,
    metadata: Metadata,
    requested_mime_type: String,
    exporter_claim: ExporterClaim,
    body_sha256: String,
    body_length: usize,
    body_base64: String,
    source_id: String,
}

/// A checked received-body archive. This is never a source/file/network grant.
/// Loading a coherent archive establishes bytes, not a Google file or owner.
pub struct ReceivedExport {
    metadata: Metadata,
    exporter_claim: ExporterClaim,
    body: Vec<u8>,
    body_sha256: String,
    source_id: String,
    limits: Limits,
}
fn source_id(metadata: &Metadata, exporter: &ExporterClaim, body_hash: &str) -> String {
    model::identity(&(KIND, metadata, exporter, body_hash))
}
fn validate_body(kind: Kind, body: &[u8]) -> Result<()> {
    match kind {
        Kind::Document | Kind::Presentation => {
            let text = std::str::from_utf8(body)
                .map_err(|_| anyhow::anyhow!("google_workspace_export_encoding_invalid"))?;
            ensure!(
                !text.contains('\0') && !crate::paths::obvious_secret(text),
                "google_workspace_export_text_refused"
            );
        }
        Kind::Spreadsheet => {
            ensure!(
                body.starts_with(b"PK\x03\x04") || body.starts_with(b"PK\x05\x06"),
                "google_workspace_export_container_signature_invalid"
            );
        }
    }
    Ok(())
}
impl ReceivedExport {
    /// Data admission only. Live acquisition needs separate host permission gates.
    pub fn from_received_data(
        metadata: Metadata,
        exporter_claim: ExporterClaim,
        body: &[u8],
        limits: &Limits,
        deadline: &Deadline,
    ) -> Result<Self> {
        deadline.check()?;
        limits.validate()?;
        metadata.validate()?;
        exporter_claim.validate()?;
        ensure!(
            body.len() <= limits.max_body_bytes,
            "google_workspace_body_capacity_exceeded"
        );
        validate_body(metadata.kind, body)?;
        let body_sha256 = model::hash(body);
        let source_id = source_id(&metadata, &exporter_claim, &body_sha256);
        let received = Self {
            metadata,
            exporter_claim,
            body: body.to_owned(),
            body_sha256,
            source_id,
            limits: limits.clone(),
        };
        received.archive(deadline)?;
        deadline.check()?;
        Ok(received)
    }
    pub fn from_archive(bytes: &[u8], limits: &Limits, deadline: &Deadline) -> Result<Self> {
        deadline.check()?;
        limits.validate()?;
        ensure!(
            bytes.len() <= limits.max_archive_bytes,
            "google_workspace_archive_capacity_exceeded"
        );
        let value = crate::json::parse(bytes, limits.max_archive_bytes)?;
        let envelope: Envelope = crate::json::decode(value)?;
        ensure!(
            envelope.schema_version == 1 && envelope.kind == KIND,
            "google_workspace_archive_schema_unsupported"
        );
        envelope.metadata.validate()?;
        envelope.exporter_claim.validate()?;
        ensure!(
            envelope.requested_mime_type == envelope.metadata.kind.export_mime(),
            "google_workspace_export_mime_mismatch"
        );
        ensure!(
            envelope.body_length <= limits.max_body_bytes
                && envelope.body_base64.len() == envelope.body_length.div_ceil(3).saturating_mul(4),
            "google_workspace_body_capacity_exceeded"
        );
        let body = BASE64_STANDARD
            .decode(&envelope.body_base64)
            .map_err(|_| anyhow::anyhow!("google_workspace_archive_base64_invalid"))?;
        ensure!(
            body.len() == envelope.body_length
                && body.len() <= limits.max_body_bytes
                && BASE64_STANDARD.encode(&body) == envelope.body_base64,
            "google_workspace_archive_body_length_mismatch"
        );
        shortcut::digest(&envelope.body_sha256)?;
        shortcut::digest(&envelope.source_id)?;
        ensure!(
            model::hash(&body) == envelope.body_sha256,
            "google_workspace_archive_body_hash_mismatch"
        );
        ensure!(
            source_id(
                &envelope.metadata,
                &envelope.exporter_claim,
                &envelope.body_sha256
            ) == envelope.source_id,
            "google_workspace_archive_source_id_mismatch"
        );
        validate_body(envelope.metadata.kind, &body)?;
        deadline.check()?;
        Ok(Self {
            metadata: envelope.metadata,
            exporter_claim: envelope.exporter_claim,
            body,
            body_sha256: envelope.body_sha256,
            source_id: envelope.source_id,
            limits: limits.clone(),
        })
    }
    pub fn archive(&self, deadline: &Deadline) -> Result<Vec<u8>> {
        deadline.check()?;
        let encoded_len = self.body.len().div_ceil(3).saturating_mul(4);
        ensure!(
            encoded_len <= self.limits.max_archive_bytes,
            "google_workspace_archive_capacity_exceeded"
        );
        let value = Envelope {
            schema_version: 1,
            kind: KIND.to_owned(),
            metadata: self.metadata.clone(),
            requested_mime_type: self.metadata.kind.export_mime().to_owned(),
            exporter_claim: self.exporter_claim.clone(),
            body_sha256: self.body_sha256.clone(),
            body_length: self.body.len(),
            body_base64: BASE64_STANDARD.encode(&self.body),
            source_id: self.source_id.clone(),
        };
        crate::json::encode_bounded(&value, self.limits.max_archive_bytes, deadline)
    }
    pub fn summary(&self) -> Value {
        json!({"kind":KIND,"sourceId":self.source_id,"bodySha256":self.body_sha256,"bodyBytes":self.body.len(),
            "metadata":self.metadata,"requestedMimeType":self.metadata.kind.export_mime(),"exporterClaim":self.exporter_claim,
            "sourceSpace":"received_export_body_bytes","shortcutBytesCaptured":false,"shortcutHashStatus":"declared_on_import",
            "googleIdentityVerified":false,"exporterClaimVerified":false,"accountIdentityVerified":false,
            "contentInspection":"not_requested","modelCalls":0,"sourceExecuted":false,
            "executionAuthority":"none","complete":false})
    }
    pub fn read(&self, start: usize, end: usize, deadline: &Deadline) -> Result<Value> {
        deadline.check()?;
        ensure!(
            start <= end && end <= self.body.len() && end - start <= 65_536,
            "google_workspace_read_range_invalid"
        );
        let bytes = &self.body[start..end];
        let text = std::str::from_utf8(bytes).ok();
        deadline.check()?;
        Ok(
            json!({"sourceId":self.source_id,"bodySha256":self.body_sha256,"sourceSpace":"received_export_body_bytes",
            "startByte":start,"endByte":end,"dataBase64":BASE64_STANDARD.encode(bytes),"utf8":text,
            "utf8Complete":text.is_some(),"executionAuthority":"none","complete":false}),
        )
    }
    pub fn inspect_office(&self, deadline: &Deadline) -> Result<Value> {
        deadline.check()?;
        ensure!(
            self.metadata.kind == Kind::Spreadsheet,
            "google_workspace_office_format_unsupported"
        );
        let inspection = crate::office::extract(&self.body, "xlsx", deadline)?;
        deadline.check()?;
        Ok(
            json!({"sourceId":self.source_id,"bodySha256":self.body_sha256,"sourceSpace":"received_export_container",
            "inspection":inspection,"sourceExecuted":false,"modelCalls":0,"complete":false}),
        )
    }
}

#[cfg(test)]
#[path = "google_workspace_tests.rs"]
mod tests;
