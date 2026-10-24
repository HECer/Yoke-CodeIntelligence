//! Google shortcut data, never a grant to read an account or execute a client.
//! Original pointer bytes are hash-bound; resource keys are never serialized.
use crate::{capture::Deadline, model, paths};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use zeroize::Zeroizing;

pub const MAX_SHORTCUT_BYTES: usize = 65_536;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    Document,
    Spreadsheet,
    Presentation,
}
impl Kind {
    pub fn for_path(path: &str) -> Result<Self> {
        paths::normalized(path)?;
        match path
            .rsplit('.')
            .next()
            .unwrap_or("")
            .to_ascii_lowercase()
            .as_str()
        {
            "gdoc" => Ok(Self::Document),
            "gsheet" => Ok(Self::Spreadsheet),
            "gslides" => Ok(Self::Presentation),
            _ => anyhow::bail!("google_workspace_shortcut_kind_unsupported"),
        }
    }
    pub fn export_mime(self) -> &'static str {
        match self {
            Self::Document => "text/markdown",
            Self::Spreadsheet => {
                "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet"
            }
            Self::Presentation => "text/plain",
        }
    }
    fn resource_kind(self, name: &str) -> bool {
        name == "file"
            || match self {
                Self::Document => name == "document",
                Self::Spreadsheet => name == "spreadsheet",
                Self::Presentation => name == "presentation",
            }
    }
    fn url_kind(self, name: &str) -> bool {
        name == "file"
            || match self {
                Self::Document => name == "document",
                Self::Spreadsheet => name == "spreadsheets",
                Self::Presentation => name == "presentation",
            }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Metadata {
    pub kind: Kind,
    pub shortcut_path: String,
    pub shortcut_sha256: String,
    pub file_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub declared_account_sha256: Option<String>,
}
impl Metadata {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            Kind::for_path(&self.shortcut_path)? == self.kind,
            "google_workspace_shortcut_kind_mismatch"
        );
        digest(&self.shortcut_sha256)?;
        file_id(&self.file_id)?;
        if let Some(account) = &self.declared_account_sha256 {
            digest(account)?;
        }
        Ok(())
    }
    pub fn check_selection(
        &self,
        expected_shortcut_sha256: &str,
        expected_file_id: &str,
    ) -> Result<()> {
        self.validate()?;
        digest(expected_shortcut_sha256)?;
        file_id(expected_file_id)?;
        ensure!(
            self.shortcut_sha256 == expected_shortcut_sha256 && self.file_id == expected_file_id,
            "google_workspace_shortcut_precondition_changed"
        );
        Ok(())
    }
}
pub(crate) fn digest(value: &str) -> Result<()> {
    ensure!(
        value.len() == 64
            && value
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
        "google_workspace_digest_invalid"
    );
    Ok(())
}
fn file_id(value: &str) -> Result<()> {
    ensure!(
        !value.is_empty()
            && value.len() <= 256
            && value
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-'))
            && !paths::obvious_secret(value),
        "google_workspace_file_id_invalid"
    );
    Ok(())
}
fn text_field<'a>(map: &'a Map<String, Value>, name: &str) -> Result<Option<&'a str>> {
    match map.get(name) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(value)) => Ok(Some(value)),
        _ => anyhow::bail!("google_workspace_shortcut_field_invalid"),
    }
}
fn add_id(ids: &mut Vec<String>, value: &str) -> Result<()> {
    if !value.is_empty() {
        file_id(value)?;
        ids.push(value.to_owned());
    }
    Ok(())
}
fn add_key(keys: &mut Vec<Zeroizing<String>>, value: &str) -> Result<()> {
    if !value.is_empty() {
        ensure!(
            value.len() <= 1024
                && value
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-')),
            "google_workspace_resource_key_invalid"
        );
        keys.push(Zeroizing::new(value.to_owned()));
    }
    Ok(())
}

/// Parsing records data only. Account labels are declarations, not authentication.
/// Callers must separately admit host-selected source identity and client grants.
pub struct Shortcut {
    metadata: Metadata,
    resource_key: Option<Zeroizing<String>>,
}
impl Shortcut {
    pub fn parse(path: &str, bytes: &[u8], deadline: &Deadline) -> Result<Self> {
        deadline.check()?;
        ensure!(
            bytes.len() <= MAX_SHORTCUT_BYTES,
            "google_workspace_shortcut_capacity_exceeded"
        );
        let kind = Kind::for_path(path)?;
        let source = std::str::from_utf8(bytes)
            .map_err(|_| anyhow::anyhow!("google_workspace_shortcut_encoding_invalid"))?;
        ensure!(
            !source.contains('\0') && !paths::obvious_secret(source),
            "google_workspace_shortcut_source_refused"
        );
        let value = crate::json::parse(bytes, MAX_SHORTCUT_BYTES)?;
        let map = value
            .as_object()
            .ok_or_else(|| anyhow::anyhow!("google_workspace_shortcut_object_required"))?;
        let mut ids = Vec::new();
        let mut keys = Vec::new();
        for name in ["doc_id", "file_id", "fileId", "id"] {
            if let Some(value) = text_field(map, name)? {
                add_id(&mut ids, value)?;
            }
        }
        if let Some(value) = text_field(map, "resource_id")?.filter(|v| !v.is_empty()) {
            let (prefix, id) = value
                .split_once(':')
                .ok_or_else(|| anyhow::anyhow!("google_workspace_resource_id_invalid"))?;
            ensure!(
                kind.resource_kind(prefix),
                "google_workspace_resource_id_kind_mismatch"
            );
            add_id(&mut ids, id)?;
        }
        for name in ["resource_key", "resourceKey"] {
            if let Some(value) = text_field(map, name)? {
                add_key(&mut keys, value)?;
            }
        }
        if let Some(value) = text_field(map, "url")?.filter(|v| !v.is_empty()) {
            ensure!(
                value.len() <= 8192
                    && value.is_ascii()
                    && !value
                        .bytes()
                        .any(|b| b.is_ascii_control() || b.is_ascii_whitespace())
                    && !value.contains('\\'),
                "google_workspace_shortcut_url_invalid"
            );
            let parsed = url::Url::parse(value)
                .map_err(|_| anyhow::anyhow!("google_workspace_shortcut_url_invalid"))?;
            ensure!(
                parsed.scheme() == "https"
                    && parsed.username().is_empty()
                    && parsed.password().is_none()
                    && parsed.port_or_known_default() == Some(443)
                    && matches!(
                        parsed.host_str(),
                        Some("docs.google.com" | "drive.google.com")
                    ),
                "google_workspace_shortcut_url_refused"
            );
            let before = ids.len();
            let segments = parsed
                .path_segments()
                .ok_or_else(|| anyhow::anyhow!("google_workspace_shortcut_url_invalid"))?
                .collect::<Vec<_>>();
            let selected = match segments.as_slice() {
                [prefix, "d", id, ..] => Some((prefix, id)),
                [prefix, "u", account, "d", id, ..]
                    if !account.is_empty()
                        && account.len() <= 4
                        && account.bytes().all(|b| b.is_ascii_digit()) =>
                {
                    Some((prefix, id))
                }
                _ => None,
            };
            if let Some((prefix, id)) = selected {
                ensure!(
                    kind.url_kind(prefix),
                    "google_workspace_shortcut_url_kind_mismatch"
                );
                add_id(&mut ids, id)?;
            }
            for (name, value) in parsed.query_pairs() {
                deadline.check()?;
                if name == "id" {
                    add_id(&mut ids, &value)?;
                }
                if name.eq_ignore_ascii_case("resourcekey") {
                    add_key(&mut keys, &value)?;
                }
            }
            ensure!(
                ids.len() > before,
                "google_workspace_shortcut_url_id_absent"
            );
        }
        let id = ids
            .first()
            .ok_or_else(|| anyhow::anyhow!("google_workspace_file_id_absent"))?;
        ensure!(
            ids.iter().all(|candidate| candidate == id),
            "google_workspace_shortcut_ids_ambiguous"
        );
        ensure!(
            keys.first()
                .is_none_or(|first| keys.iter().all(|key| key.as_str() == first.as_str())),
            "google_workspace_resource_keys_ambiguous"
        );
        let declared_account_sha256 =
            if let Some(value) = text_field(map, "email")?.filter(|v| !v.is_empty()) {
                ensure!(
                    value.len() <= 512 && !value.chars().any(char::is_control),
                    "google_workspace_account_label_invalid"
                );
                Some(model::hash(value))
            } else {
                None
            };
        let metadata = Metadata {
            kind,
            shortcut_path: path.to_owned(),
            shortcut_sha256: model::hash(bytes),
            file_id: id.clone(),
            declared_account_sha256,
        };
        metadata.validate()?;
        deadline.check()?;
        Ok(Self {
            metadata,
            resource_key: keys.into_iter().next(),
        })
    }
    pub fn metadata(&self) -> &Metadata {
        &self.metadata
    }
    pub fn has_resource_key(&self) -> bool {
        self.resource_key.is_some()
    }
    pub(crate) fn resource_key(&self) -> Option<&str> {
        self.resource_key.as_ref().map(|value| value.as_str())
    }
    pub fn describe(&self) -> Value {
        json!({"kind":"google_workspace_shortcut_metadata","metadata":self.metadata,
            "requestedMimeType":self.metadata.kind.export_mime(),"resourceKeyPresent":self.has_resource_key(),
            "shortcutBytesCaptured":false,"accountIdentityVerified":false,"acquisitionPerformed":false,
            "executionAuthority":"none","complete":false})
    }
}
