//! Selected source codecs. Raw CAS bytes remain authoritative; text is a view.
use crate::{capture::Deadline, model, project_session::HostProfile};
use anyhow::{Result, bail, ensure};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const MAX_RAW: usize = 4_000_000;
pub const MAX_TEXT: usize = 4_000_000;
pub const MAX_MAPPING: usize = 32_000_008;
pub const VERSION: &str = "source_encoding_v1";

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Codec {
    Utf8,
    Utf8Sig,
    Utf16,
    Utf16Le,
    Utf16Be,
    Latin1,
}
impl Codec {
    /// Deliberately finite, documented Python 3.11 standard aliases. No registry,
    /// platform defaults, heuristic detection, Windows-1252 or replacement mode.
    pub fn parse(value: &str) -> Result<Self> {
        ensure!(
            value.len() <= 32 && value.is_ascii(),
            "source_encoding_unsupported"
        );
        let name = value.to_ascii_lowercase().replace('-', "_");
        Ok(match name.as_str() {
            "utf_8" | "utf8" | "u8" | "utf" | "cp65001" => Self::Utf8,
            "utf_8_sig" => Self::Utf8Sig,
            "utf_16" | "utf16" | "u16" => Self::Utf16,
            "utf_16_le" | "utf_16le" => Self::Utf16Le,
            "utf_16_be" | "utf_16be" => Self::Utf16Be,
            "latin_1" | "latin1" | "iso_8859_1" | "iso8859_1" | "8859" | "cp819" | "latin"
            | "l1" => Self::Latin1,
            _ => bail!("source_encoding_unsupported"),
        })
    }
    pub fn policy(self) -> Option<Policy> {
        (self != Self::Utf8).then_some(Policy::StrictV1 { codec: self })
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Policy {
    StrictV1 { codec: Codec },
}
impl Policy {
    pub fn codec(self) -> Codec {
        match self {
            Self::StrictV1 { codec } => codec,
        }
    }
    pub(crate) fn canonical(self) -> Option<Self> {
        self.codec().policy()
    }
}
#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields, default)]
pub struct ProjectEncodings {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project: Option<Codec>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub local: Option<Codec>,
}
#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields, default)]
pub struct HostEncodings {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub global: Option<Codec>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub projects: BTreeMap<String, ProjectEncodings>,
}
impl HostEncodings {
    pub(crate) fn non_utf8(&self) -> bool {
        self.global
            .into_iter()
            .chain(
                self.projects
                    .values()
                    .flat_map(|p| p.project.into_iter().chain(p.local)),
            )
            .any(|c| c != Codec::Utf8)
    }
    pub fn select(&self, project: &str) -> Option<Codec> {
        self.projects
            .get(project)
            .and_then(|p| p.local.or(p.project))
            .or(self.global)
    }
    pub(crate) fn empty(&self) -> bool {
        self.global.is_none()
            && self
                .projects
                .values()
                .all(|p| p.project.is_none() && p.local.is_none())
    }
    pub(crate) fn validate(&self, host: &HostProfile) -> Result<()> {
        ensure!(self.projects.len() <= 101, "source_encoding_project_budget");
        for name in self.projects.keys() {
            ensure!(
                name == "default" || host.projects.iter().any(|p| p.name == *name),
                "source_encoding_project_not_host_registered"
            );
        }
        Ok(())
    }
}

/// Strict, bounded scalar boundary mapping. Neither half a surrogate pair nor
/// an interior UTF-8 byte can be used as an original-source boundary.
pub struct View {
    raw: Vec<u8>,
    text: String,
    text_hash: String,
    policy: Option<Policy>,
    codec: Codec,
    prefix: usize,
    boundaries: Vec<(u32, u32)>,
}
impl View {
    pub fn decode(
        raw: Vec<u8>,
        policy: Option<Policy>,
        check: &impl Fn() -> Result<()>,
    ) -> Result<Self> {
        check()?;
        ensure!(raw.len() <= MAX_RAW, "source_encoding_raw_budget");
        let policy = policy.and_then(Policy::canonical);
        let mut codec = policy.map_or(Codec::Utf8, Policy::codec);
        let prefix = if codec == Codec::Utf16 {
            codec = match raw.get(..2) {
                Some([0xff, 0xfe]) => Codec::Utf16Le,
                Some([0xfe, 0xff]) => Codec::Utf16Be,
                _ => bail!("source_encoding_utf16_bom_required"),
            };
            2
        } else if codec == Codec::Utf8Sig && raw.starts_with(&[0xef, 0xbb, 0xbf]) {
            3
        } else {
            0
        };
        if matches!(codec, Codec::Utf8 | Codec::Utf8Sig) {
            let text = std::str::from_utf8(&raw[prefix..])?;
            ensure!(text.len() <= MAX_TEXT, "source_encoding_decoded_budget");
            let text = text.to_owned();
            let text_hash = checked_hash(text.as_bytes(), check)?;
            check()?;
            return Ok(Self {
                raw,
                text,
                text_hash,
                policy,
                codec,
                prefix,
                boundaries: Vec::new(),
            });
        }
        let scan = |emit: &mut dyn FnMut(char, usize) -> Result<()>| -> Result<()> {
            let mut i = prefix;
            while i < raw.len() {
                if i % 8192 <= 3 {
                    check()?;
                }
                let ch = if codec == Codec::Latin1 {
                    let c = char::from(raw[i]);
                    i += 1;
                    c
                } else {
                    let unit = |at: usize| -> Result<u16> {
                        let pair = raw
                            .get(at..at + 2)
                            .ok_or_else(|| anyhow::anyhow!("source_encoding_utf16_odd_bytes"))?;
                        Ok(if codec == Codec::Utf16Le {
                            u16::from_le_bytes([pair[0], pair[1]])
                        } else {
                            u16::from_be_bytes([pair[0], pair[1]])
                        })
                    };
                    let first = unit(i)?;
                    i += 2;
                    let scalar = if (0xd800..=0xdbff).contains(&first) {
                        let second = unit(i)?;
                        i += 2;
                        ensure!(
                            (0xdc00..=0xdfff).contains(&second),
                            "source_encoding_utf16_unpaired_surrogate"
                        );
                        0x10000 + ((u32::from(first) - 0xd800) << 10) + u32::from(second) - 0xdc00
                    } else {
                        ensure!(
                            !(0xdc00..=0xdfff).contains(&first),
                            "source_encoding_utf16_unpaired_surrogate"
                        );
                        u32::from(first)
                    };
                    char::from_u32(scalar)
                        .ok_or_else(|| anyhow::anyhow!("source_encoding_invalid_scalar"))?
                };
                emit(ch, i)?;
            }
            check()
        };
        let (mut size, mut count) = (0usize, 1usize);
        scan(&mut |ch, _| {
            size = size
                .checked_add(ch.len_utf8())
                .ok_or_else(|| anyhow::anyhow!("source_encoding_decoded_budget"))?;
            count += 1;
            ensure!(size <= MAX_TEXT, "source_encoding_decoded_budget");
            ensure!(count <= MAX_MAPPING / 8, "source_encoding_mapping_budget");
            Ok(())
        })?;
        // Both passes use the same held raw vector; admission precedes allocations.
        let mut text = String::with_capacity(size);
        let mut boundaries = Vec::with_capacity(count);
        boundaries.push((0, prefix as u32));
        scan(&mut |ch, end| {
            text.push(ch);
            boundaries.push((text.len() as u32, end as u32));
            Ok(())
        })?;
        check()?;
        let text_hash = checked_hash(text.as_bytes(), check)?;
        Ok(Self {
            raw,
            text,
            text_hash,
            policy,
            codec,
            prefix,
            boundaries,
        })
    }
    pub fn text(&self) -> &str {
        &self.text
    }
    pub fn raw(&self) -> &[u8] {
        &self.raw
    }
    pub fn policy(&self) -> Option<Policy> {
        self.policy
    }
    pub fn into_text(self) -> String {
        self.text
    }
    pub(crate) fn retained_bytes(&self) -> usize {
        self.raw.len() + self.text.len() + self.boundaries.len() * 8
    }
    pub fn to_raw(&self, decoded: usize) -> Result<usize> {
        if matches!(self.codec, Codec::Utf8 | Codec::Utf8Sig) {
            crate::paths::utf8_range(&self.text, 0, decoded)?;
            return Ok(decoded + self.prefix);
        }
        let value = u32::try_from(decoded)?;
        let i = self
            .boundaries
            .binary_search_by_key(&value, |b| b.0)
            .map_err(|_| anyhow::anyhow!("source_encoding_invalid_decoded_boundary"))?;
        Ok(self.boundaries[i].1 as usize)
    }
    pub fn to_decoded(&self, original: usize) -> Result<usize> {
        if matches!(self.codec, Codec::Utf8 | Codec::Utf8Sig) {
            let decoded = original
                .checked_sub(self.prefix)
                .ok_or_else(|| anyhow::anyhow!("source_encoding_invalid_raw_boundary"))?;
            crate::paths::utf8_range(&self.text, 0, decoded)?;
            return Ok(decoded);
        }
        // The consumed BOM is immutable prefix data. A whole-file read may
        // include it, but edits must begin at the first actual text boundary.
        let value = u32::try_from(original)?;
        let i = self
            .boundaries
            .binary_search_by_key(&value, |b| b.1)
            .map_err(|_| anyhow::anyhow!("source_encoding_invalid_raw_boundary"))?;
        Ok(self.boundaries[i].0 as usize)
    }
    pub fn decoded_range(&self, start: usize, end: usize) -> Result<(usize, usize)> {
        if self.codec == Codec::Utf8 {
            crate::paths::utf8_range(&self.text, start, end)?;
            return Ok((start, end));
        }
        ensure!(
            start <= end && end <= self.raw.len(),
            "source_encoding_invalid_range"
        );
        Ok((self.to_decoded(start)?, self.to_decoded(end)?))
    }
    pub fn quote(&self, start: usize, end: usize) -> Result<&str> {
        let start = if start == 0 && self.prefix != 0 && end >= self.prefix {
            self.prefix
        } else {
            start
        };
        let (a, b) = self.decoded_range(start, end)?;
        Ok(&self.text[a..b])
    }
    pub fn encode_like(&self, text: &str, check: &impl Fn() -> Result<()>) -> Result<Vec<u8>> {
        ensure!(
            self.codec != Codec::Utf8Sig || self.prefix != 0 || !text.starts_with('\u{feff}'),
            "source_encoding_utf8_sig_ambiguous_leading_bom"
        );
        let mut bytes = encode(text, self.codec, check)?;
        if self.prefix != 0 {
            ensure!(
                bytes.len() <= MAX_RAW - self.prefix,
                "source_encoding_raw_budget"
            );
            let mut prefixed = Vec::with_capacity(bytes.len() + self.prefix);
            prefixed.extend_from_slice(&self.raw[..self.prefix]);
            prefixed.append(&mut bytes);
            bytes = prefixed;
        }
        check()?;
        Ok(bytes)
    }
    pub fn evidence(&self, raw_hash: &str) -> serde_json::Value {
        serde_json::json!({"schema":VERSION,"policy":self.policy,"rawContentHash":raw_hash,"analysisContentHash":self.text_hash,"offsetSpace":"original_source_bytes","textSpace":"decoded_utf8","bomPrefixBytes":self.prefix,"rawBytes":self.raw.len(),"decodedBytes":self.text.len()})
    }
    pub fn annotate(&self, value: &mut serde_json::Value, hash: &str) {
        if self.policy.is_some() {
            value["sourceView"] = self.evidence(hash);
        }
    }
}

fn checked_hash(bytes: &[u8], check: &impl Fn() -> Result<()>) -> Result<String> {
    use sha2::{Digest, Sha256};
    let mut hash = Sha256::new();
    for chunk in bytes.chunks(16384) {
        check()?;
        hash.update(chunk);
    }
    check()?;
    Ok(hex::encode(hash.finalize()))
}

fn encode(text: &str, codec: Codec, check: &impl Fn() -> Result<()>) -> Result<Vec<u8>> {
    check()?;
    ensure!(text.len() <= MAX_TEXT, "source_encoding_decoded_budget");
    let mut size = 0usize;
    for (i, ch) in text.char_indices() {
        if i % 8192 <= 3 {
            check()?;
        }
        let add = match codec {
            Codec::Latin1 => {
                ensure!(
                    u32::from(ch) <= 255,
                    "source_encoding_unrepresentable_latin1"
                );
                1
            }
            Codec::Utf16Le | Codec::Utf16Be => ch.len_utf16() * 2,
            Codec::Utf8 | Codec::Utf8Sig => ch.len_utf8(),
            Codec::Utf16 => bail!("source_encoding_utf16_bom_required"),
        };
        size = size
            .checked_add(add)
            .ok_or_else(|| anyhow::anyhow!("source_encoding_raw_budget"))?;
        ensure!(size <= MAX_RAW, "source_encoding_raw_budget");
    }
    let mut bytes = Vec::with_capacity(size);
    for (i, ch) in text.char_indices() {
        if i % 8192 <= 3 {
            check()?;
        }
        match codec {
            Codec::Latin1 => bytes.push(ch as u8),
            Codec::Utf8 | Codec::Utf8Sig => {
                bytes.extend_from_slice(ch.encode_utf8(&mut [0; 4]).as_bytes())
            }
            _ => {
                for unit in ch.encode_utf16(&mut [0; 2]) {
                    bytes.extend_from_slice(&if codec == Codec::Utf16Le {
                        unit.to_le_bytes()
                    } else {
                        unit.to_be_bytes()
                    });
                }
            }
        }
    }
    check()?;
    Ok(bytes)
}
pub(crate) fn encode_new(text: &str, policy: Option<Policy>, d: &Deadline) -> Result<Vec<u8>> {
    // A new BOM-selected UTF-16 file uses a deterministic LE BOM, independent
    // of the host CPU. Existing files always retain their captured byte order.
    if policy.is_some_and(|p| p.codec() == Codec::Utf16) {
        let body = encode(text, Codec::Utf16Le, &|| d.check())?;
        ensure!(body.len() <= MAX_RAW - 2, "source_encoding_raw_budget");
        let mut raw = Vec::with_capacity(body.len() + 2);
        raw.extend_from_slice(&[0xff, 0xfe]);
        raw.extend_from_slice(&body);
        return Ok(raw);
    }
    if policy.is_some_and(|p| p.codec() == Codec::Utf8Sig) {
        let body = encode(text, Codec::Utf8, &|| d.check())?;
        ensure!(body.len() <= MAX_RAW - 3, "source_encoding_raw_budget");
        let mut raw = Vec::with_capacity(body.len() + 3);
        raw.extend_from_slice(&[0xef, 0xbb, 0xbf]);
        raw.extend_from_slice(&body);
        return Ok(raw);
    }
    encode(text, policy.map_or(Codec::Utf8, Policy::codec), &|| {
        d.check()
    })
}
pub(crate) fn selected(
    store: &crate::store::Store,
    snapshot: &model::Snapshot,
) -> Result<Option<Policy>> {
    Ok(crate::capture::options_for_snapshot(store, snapshot)?.source_encoding)
}
/// Publication historically accepts legacy manifests owned by other producers.
/// Only a native scope receipt can select a different codec; absence preserves
/// the preceding UTF-8 publication behavior, not a new authority fallback.
pub(crate) fn publication_policy(
    store: &crate::store::Store,
    snapshot: &model::Snapshot,
) -> Result<Option<Policy>> {
    store.check_deadline()?;
    let receipt: bool = store.db.query_row("SELECT EXISTS(SELECT 1 FROM records WHERE id=?1 AND kind IN ('native_capture_options_v1','native_capture_scope_ignored_v1','native_capture_scope_derived_v1'))",[&snapshot.id],|r|r.get(0))?;
    if receipt {
        Ok(crate::capture::options_for_publication(store, snapshot)?.source_encoding)
    } else {
        Ok(None)
    }
}
pub(crate) fn require_legacy(
    store: &crate::store::Store,
    snapshot: &model::Snapshot,
    consumer: &str,
) -> Result<()> {
    ensure!(
        publication_policy(store, snapshot)?.is_none(),
        "source_encoding_capability_unavailable: {consumer} requires legacy UTF-8 source proof"
    );
    Ok(())
}

/// Parser coordinates and lexical-resolution coordinates move together. Keys
/// retain every parallel edge, but cannot collide with the released UTF-8 cache.
pub(crate) fn extract(
    epoch: crate::analysis::AnalyzerEpoch,
    file: &model::SourceFile,
    view: &View,
    d: &Deadline,
) -> Result<model::Extraction> {
    let mut extraction = crate::analysis::extract_at(epoch, file, view.text(), d)?;
    if view.policy().is_none() {
        return Ok(extraction);
    }
    let mut keys = BTreeMap::new();
    for entity in &mut extraction.entities {
        d.check()?;
        entity.start_byte = view.to_raw(entity.start_byte)?;
        entity.end_byte = view.to_raw(entity.end_byte)?;
        entity.name_start_byte = entity.name_start_byte.map(|n| view.to_raw(n)).transpose()?;
        entity.name_end_byte = entity.name_end_byte.map(|n| view.to_raw(n)).transpose()?;
        let key = model::identity(&(
            VERSION,
            view.policy(),
            &entity.key,
            entity.start_byte,
            entity.end_byte,
        ));
        keys.insert(entity.key.clone(), key.clone());
        entity.key = key;
    }
    for entity in &mut extraction.entities {
        if let Some(parent) = &mut entity.parent_key {
            *parent = keys
                .get(parent)
                .ok_or_else(|| anyhow::anyhow!("source_encoding_parent_key_missing"))?
                .clone();
        }
    }
    for edge in &mut extraction.edges {
        d.check()?;
        edge.start_byte = view.to_raw(edge.start_byte)?;
        edge.end_byte = view.to_raw(edge.end_byte)?;
        edge.source_key = keys
            .get(&edge.source_key)
            .ok_or_else(|| anyhow::anyhow!("source_encoding_edge_key_missing"))?
            .clone();
        if let Some(key) = keys.get(&edge.target_label) {
            edge.target_label = key.clone();
        }
    }
    for diagnostic in &mut extraction.diagnostics {
        d.check()?;
        diagnostic.start_byte = view.to_raw(diagnostic.start_byte)?;
    }
    for scope in &mut extraction.resolution.scopes {
        d.check()?;
        scope.start = view.to_raw(scope.start)?;
        scope.end = view.to_raw(scope.end)?;
    }
    for binding in &mut extraction.resolution.bindings {
        d.check()?;
        binding.defined_at = view.to_raw(binding.defined_at)?;
        if let crate::resolution::BindingTarget::Entity(key) = &mut binding.target {
            *key = keys
                .get(key)
                .ok_or_else(|| anyhow::anyhow!("source_encoding_binding_key_missing"))?
                .clone();
        }
    }
    for usage in &mut extraction.resolution.uses {
        d.check()?;
        usage.start = view.to_raw(usage.start)?;
        usage.end = view.to_raw(usage.end)?;
        usage.source_key = keys
            .get(&usage.source_key)
            .ok_or_else(|| anyhow::anyhow!("source_encoding_usage_key_missing"))?
            .clone();
    }
    for arity in &mut extraction.resolution.callable_arities {
        arity.entity_key = keys
            .get(&arity.entity_key)
            .ok_or_else(|| anyhow::anyhow!("source_encoding_arity_key_missing"))?
            .clone();
    }
    Ok(extraction)
}
pub(crate) fn qualify_graph(graph: &mut model::Graph, policy: Option<Policy>) {
    if policy.is_some() {
        for evidence in &mut graph.evidence {
            evidence.anchor = "exact_decoded_view_original_byte_boundaries_v1".into();
        }
        graph.coverage.limitations.push("Selected source_encoding_v1: signatures and names are decoded UTF-8 views; content hashes and all entity/evidence byte ranges identify original encoded bytes. No compiler/runtime execution is implied.".into());
    }
}

/// Selected-codec review uses a deterministic linear full-file unified diff.
/// This avoids an uninterruptible alignment algorithm and admits every output
/// byte before allocation. The released UTF-8 diff recipe is unchanged.
pub(crate) fn append_diff(
    output: &mut String,
    before: &str,
    after: &str,
    old_path: &str,
    new_path: &str,
    d: &Deadline,
) -> Result<()> {
    let lines = |text: &str| -> Result<usize> {
        let mut count = 0;
        for chunk in text.as_bytes().chunks(16384) {
            d.check()?;
            count += chunk.iter().filter(|&&b| b == b'\n').count();
        }
        Ok(count + usize::from(!text.is_empty() && !text.ends_with('\n')))
    };
    let a = lines(before)?;
    let b = lines(after)?;
    let header = format!(
        "--- {old_path}\n+++ {new_path}\n@@ -{},{} +{},{} @@\n",
        usize::from(a != 0),
        a,
        usize::from(b != 0),
        b
    );
    let size = header
        .len()
        .checked_add(before.len())
        .and_then(|n| n.checked_add(after.len()))
        .and_then(|n| n.checked_add(a))
        .and_then(|n| n.checked_add(b))
        .and_then(|n| n.checked_add(64))
        .ok_or_else(|| anyhow::anyhow!("source_encoding_diff_budget"))?;
    ensure!(
        size <= 16_000_000usize.saturating_sub(output.len()),
        "source_encoding_diff_budget"
    );
    d.check()?;
    output.try_reserve_exact(size)?;
    output.push_str(&header);
    for (prefix, text) in [('-', before), ('+', after)] {
        for line in text.split_inclusive('\n') {
            d.check()?;
            output.push(prefix);
            // A single long line is bounded too, with cancellation between chunks.
            let mut start = 0;
            while start < line.len() {
                d.check()?;
                let mut end = (start + 16384).min(line.len());
                while !line.is_char_boundary(end) {
                    end -= 1;
                }
                output.push_str(&line[start..end]);
                start = end;
            }
        }
        if !text.is_empty() && !text.ends_with('\n') {
            output.push_str("\n\\ No newline at end of file\n");
        }
    }
    d.check()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn encoded_review_diff_is_deterministic_bounded_and_cancellable() {
        let mut diff = String::new();
        append_diff(
            &mut diff,
            "é\r\nlast",
            "café\r\nlast",
            "a.rs",
            "a.rs",
            &Deadline::new(30000),
        )
        .unwrap();
        assert_eq!(
            diff,
            "--- a.rs\n+++ a.rs\n@@ -1,2 +1,2 @@\n-é\r\n-last\n\\ No newline at end of file\n+café\r\n+last\n\\ No newline at end of file\n"
        );
        let mut same = String::new();
        append_diff(
            &mut same,
            "é\r\nlast",
            "café\r\nlast",
            "a.rs",
            "a.rs",
            &Deadline::new(30000),
        )
        .unwrap();
        assert_eq!(same, diff);
        let mut full = "x".repeat(16_000_000);
        assert!(append_diff(&mut full, "a", "b", "a", "b", &Deadline::new(30000)).is_err());
        assert_eq!(full.len(), 16_000_000);
        let mut untouched = String::new();
        assert!(append_diff(&mut untouched, "a", "b", "a", "b", &Deadline::new(0)).is_err());
        assert!(untouched.is_empty());
    }
}
