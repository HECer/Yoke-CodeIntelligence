//! Explicit host source-write formatting. Captured and approved bytes stay immutable.
use crate::{capture::Deadline, project_session::HostProfile};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LineEnding {
    Lf,
    Crlf,
    Native,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ResolvedLineEnding {
    Lf,
    Crlf,
}
impl LineEnding {
    pub fn resolve(self) -> ResolvedLineEnding {
        match self {
            Self::Lf => ResolvedLineEnding::Lf,
            Self::Crlf => ResolvedLineEnding::Crlf,
            Self::Native if cfg!(windows) => ResolvedLineEnding::Crlf,
            Self::Native => ResolvedLineEnding::Lf,
        }
    }
}
#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields, default)]
pub struct ProjectLineEndings {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project: Option<LineEnding>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub local: Option<LineEnding>,
}
#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields, default)]
pub struct HostLineEndings {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub global: Option<LineEnding>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub projects: BTreeMap<String, ProjectLineEndings>,
}
impl HostLineEndings {
    pub fn select(&self, project: &str) -> Option<ResolvedLineEnding> {
        self.projects
            .get(project)
            .and_then(|p| p.local.or(p.project))
            .or(self.global)
            .map(LineEnding::resolve)
    }
    pub(crate) fn empty(&self) -> bool {
        self.global.is_none()
            && self
                .projects
                .values()
                .all(|p| p.project.is_none() && p.local.is_none())
    }
    pub(crate) fn validate(&self, host: &HostProfile) -> Result<()> {
        ensure!(self.projects.len() <= 101, "source_line_endings_capacity");
        for name in self.projects.keys() {
            ensure!(
                name == "default" || host.projects.iter().any(|p| p.name == *name),
                "source_line_endings_project_not_host_registered"
            );
        }
        Ok(())
    }
}
impl ResolvedLineEnding {
    /// Universal newline conversion applies after original UTF-8 edit offsets.
    /// Admit expanded bytes before allocation or CAS writes, with a finite deadline.
    pub(crate) fn normalize(self, text: String, d: &Deadline) -> Result<String> {
        let bytes = text.as_bytes();
        let mut size = 0usize;
        let mut i = 0;
        while i < bytes.len() {
            if i % 16384 <= 1 {
                d.check()?;
            }
            size += match bytes[i] {
                b'\r' | b'\n' => {
                    if bytes[i] == b'\r' && bytes.get(i + 1) == Some(&b'\n') {
                        i += 1;
                    }
                    if self == Self::Crlf { 2 } else { 1 }
                }
                _ => 1,
            };
            ensure!(size <= 4_000_000, "source_line_endings_file_budget");
            i += 1;
        }
        d.check()?;
        if self.canonical(bytes, d)? {
            return Ok(text);
        }
        let mut output = Vec::with_capacity(size);
        i = 0;
        while i < bytes.len() {
            if i % 16384 <= 1 {
                d.check()?;
            }
            match bytes[i] {
                b'\r' | b'\n' => {
                    if bytes[i] == b'\r' && bytes.get(i + 1) == Some(&b'\n') {
                        i += 1;
                    }
                    if self == Self::Crlf {
                        output.push(b'\r');
                    }
                    output.push(b'\n');
                }
                byte => output.push(byte),
            }
            i += 1;
        }
        d.check()?;
        Ok(String::from_utf8(output)?)
    }
    pub(crate) fn canonical(self, bytes: &[u8], d: &Deadline) -> Result<bool> {
        for (i, byte) in bytes.iter().enumerate() {
            if i % 16384 == 0 {
                d.check()?;
            }
            if match self {
                Self::Lf => *byte == b'\r',
                Self::Crlf => {
                    (*byte == b'\r' && bytes.get(i + 1) != Some(&b'\n'))
                        || (*byte == b'\n' && (i == 0 || bytes[i - 1] != b'\r'))
                }
            } {
                return Ok(false);
            }
        }
        d.check()?;
        Ok(true)
    }
}

/// Compiler evidence precedes this independently selected text transform.
pub(crate) fn qualify_provider_result(
    plan: &crate::changes::Plan,
    mut value: serde_json::Value,
) -> serde_json::Value {
    if let Some(ending) = plan.source_line_ending {
        value["providerEffectEquivalent"] = value["equivalent"].clone();
        value["equivalent"] = serde_json::json!("provider_effect_then_host_source_formatting");
        value["sourceFormatting"] = serde_json::json!({"lineEnding":ending,"appliedAfterProviderEvidence":true,"diagnosticsScope":"provider_effect_before_host_source_formatting","formattedResultRequiresVerification":true});
    }
    value
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exact_growth_budget_and_expired_deadline_are_admitted_before_conversion() {
        let d = Deadline::new(30000);
        let text = "\n".repeat(2_000_000);
        let result = ResolvedLineEnding::Crlf.normalize(text, &d).unwrap();
        assert_eq!(result.len(), 4_000_000);
        assert!(
            ResolvedLineEnding::Crlf
                .canonical(result.as_bytes(), &d)
                .unwrap()
        );
        assert_eq!(
            ResolvedLineEnding::Crlf
                .normalize("\n".repeat(2_000_001), &d)
                .unwrap_err()
                .to_string(),
            "source_line_endings_file_budget"
        );
        assert!(
            ResolvedLineEnding::Lf
                .normalize("\r\n".into(), &Deadline::new(0))
                .is_err()
        );
    }
    #[test]
    fn all_newline_forms_keep_unicode_bom_no_final_newline_and_idempotence() {
        let text = "\u{feff}e\u{301}\r\n\u{1f4a1}\rnext\nend";
        for (ending, expected) in [
            (
                ResolvedLineEnding::Lf,
                "\u{feff}e\u{301}\n\u{1f4a1}\nnext\nend",
            ),
            (
                ResolvedLineEnding::Crlf,
                "\u{feff}e\u{301}\r\n\u{1f4a1}\r\nnext\r\nend",
            ),
        ] {
            let actual = ending
                .normalize(text.into(), &Deadline::new(30000))
                .unwrap();
            assert_eq!(actual, expected);
            assert_eq!(
                ending
                    .normalize(actual.clone(), &Deadline::new(30000))
                    .unwrap(),
                actual
            );
            assert!(
                ending
                    .canonical(actual.as_bytes(), &Deadline::new(30000))
                    .unwrap()
            );
        }
    }
}
