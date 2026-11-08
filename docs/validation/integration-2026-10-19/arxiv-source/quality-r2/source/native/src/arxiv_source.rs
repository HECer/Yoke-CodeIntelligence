//! Received arXiv metadata, not publication, version or author authority.
pub use crate::arxiv_rate::{recover as recover_rate, status as rate_status};
use crate::{
    capture::Deadline,
    source_acquisition::{self as source, Format, ReceivedSource},
};
use anyhow::{Result, ensure};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::OnceLock,
};
use std::{net::SocketAddr, path::PathBuf};
#[derive(Clone, Copy, Debug, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Representation {
    #[default]
    Metadata,
    Pdf,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Spec {
    pub id: String,
    pub address: SocketAddr,
    pub coordination_file: PathBuf,
    #[serde(default)]
    pub representation: Representation,
    #[serde(default = "system_roots")]
    pub use_system_roots: bool,
    #[serde(default)]
    pub root_certificates_der_base64: Vec<String>,
}
fn system_roots() -> bool {
    true
}
#[derive(Default)]
pub struct Permissions {
    pub allow_network: bool,
    pub allow_rate_coordination: bool,
}
impl Spec {
    pub fn validate(&self) -> Result<()> {
        self.http_spec()?.validate_arxiv()?;
        Ok(())
    }
    fn http_spec(&self) -> Result<crate::source_acquisition_http::Spec> {
        validate_id(&self.id)?;
        let path = match self.representation {
            Representation::Metadata => "abs",
            Representation::Pdf => "pdf",
        };
        let port = if self.address.port() == 443 {
            String::new()
        } else {
            format!(":{}", self.address.port())
        };
        let spec = crate::source_acquisition_http::Spec {
            url: format!("https://arxiv.org{port}/{path}/{}", self.id),
            address: self.address,
            format: match self.representation {
                Representation::Metadata => Format::Html,
                Representation::Pdf => Format::Pdf,
            },
            use_system_roots: self.use_system_roots,
            root_certificates_der_base64: self.root_certificates_der_base64.clone(),
        };
        spec.validate_arxiv()?;
        Ok(spec)
    }
}
pub fn capture(
    spec: &Spec,
    permissions: &Permissions,
    limits: &source::Limits,
    deadline: &Deadline,
) -> Result<ReceivedSource> {
    ensure!(
        permissions.allow_network,
        "source_acquisition_network_permission_required"
    );
    ensure!(
        permissions.allow_rate_coordination,
        "arxiv_rate_coordination_permission_required"
    );
    limits.validate()?;
    let http = spec.http_spec()?;
    source::check(deadline)?;
    let permit = crate::arxiv_rate::admit(&spec.coordination_file, true, deadline)?;
    let result = crate::source_acquisition_http::capture_arxiv(
        &http,
        &crate::source_acquisition_http::Permissions {
            allow_network: true,
            allow_loopback_http: false,
        },
        limits,
        deadline,
        permit,
    )?;
    if matches!(spec.representation, Representation::Metadata) {
        let view = inspect(&result, 2_000_000, deadline)?;
        ensure!(
            view["idBinding"] == "received_base_id_candidates_match_url_label",
            "arxiv_source_selector_mismatch"
        );
    }
    Ok(result)
}

pub fn validate_id(id: &str) -> Result<()> {
    static PATTERN: OnceLock<regex::Regex> = OnceLock::new();
    source::input(id.len() <= 64 && id.is_ascii())?;
    let pattern=PATTERN.get_or_init(||regex::Regex::new(r"^(?:[0-9]{4}\.[0-9]{4,5}|[a-z][a-z0-9-]{0,31}(?:\.[A-Z]{2})?/[0-9]{7})(?:v[1-9][0-9]{0,3})?$").expect("constant arxiv ID pattern"));
    source::input(pattern.is_match(id))?;
    let numeric = id.split('/').next_back().unwrap();
    source::input(
        numeric
            .get(2..4)
            .and_then(|s| s.parse::<u8>().ok())
            .is_some_and(|n| (1..=12).contains(&n)),
    )
}
fn base_id(id: &str) -> &str {
    id.rsplit_once('v')
        .filter(|(_, v)| !v.is_empty() && v.bytes().all(|b| b.is_ascii_digit()))
        .map_or(id, |(base, _)| base)
}
fn selector(source: &ReceivedSource) -> Result<String> {
    source::input(source.format() == Format::Html)?;
    let value = source.as_value();
    let url = source::url(
        value["metadata"]["requestedUrl"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("arxiv_source_input_invalid"))?,
    )?;
    source::input(
        url.scheme() == "https" && url.host_str() == Some("arxiv.org") && url.query().is_none(),
    )?;
    let id = url
        .path()
        .strip_prefix("/abs/")
        .ok_or_else(|| anyhow::anyhow!("arxiv_source_metadata_unavailable"))?;
    validate_id(id)?;
    Ok(id.to_owned())
}
fn syntax(ok: bool) -> Result<()> {
    ensure!(ok, "arxiv_source_html_profile_unsupported");
    Ok(())
}
fn lower(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|b| char::from(b.to_ascii_lowercase()))
        .collect()
}
struct Tag {
    name: String,
    closing: bool,
    empty: bool,
    attributes: Vec<(String, String)>,
    start: usize,
    end: usize,
}
fn tag(bytes: &[u8], start: usize) -> Result<Tag> {
    syntax(bytes.get(start) == Some(&b'<'))?;
    let mut end = start + 1;
    let mut quote = None;
    loop {
        source::capacity(
            end.saturating_sub(start) <= 32768 && end < bytes.len() && end < 1_000_000,
        )?;
        let b = bytes[end];
        if let Some(q) = quote {
            if b == q {
                quote = None;
            }
        } else if matches!(b, b'\'' | b'"') {
            quote = Some(b);
        } else if b == b'>' {
            end += 1;
            break;
        }
        end += 1;
    }
    let mut i = start + 1;
    let closing = bytes.get(i) == Some(&b'/');
    if closing {
        i += 1;
    }
    let name_start = i;
    while bytes
        .get(i)
        .is_some_and(|b| b.is_ascii_alphanumeric() || *b == b'-')
    {
        i += 1;
    }
    syntax(i > name_start && i - name_start <= 64)?;
    let name = lower(&bytes[name_start..i]);
    let mut attributes = Vec::new();
    let mut empty = false;
    while i < end - 1 {
        while i < end - 1 && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        if i == end - 1 {
            break;
        }
        if bytes[i] == b'/' {
            i += 1;
            while i < end - 1 && bytes[i].is_ascii_whitespace() {
                i += 1;
            }
            syntax(i == end - 1)?;
            empty = true;
            break;
        }
        syntax(!closing)?;
        let a = i;
        while i < end - 1 && (bytes[i].is_ascii_alphanumeric() || b"_:-".contains(&bytes[i])) {
            i += 1;
        }
        syntax(i > a && i - a <= 64)?;
        let key = lower(&bytes[a..i]);
        while i < end - 1 && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        let raw = if bytes.get(i) == Some(&b'=') {
            i += 1;
            while i < end - 1 && bytes[i].is_ascii_whitespace() {
                i += 1;
            }
            syntax(i < end - 1)?;
            if matches!(bytes[i], b'\'' | b'"') {
                let q = bytes[i];
                i += 1;
                let begin = i;
                while i < end - 1 && bytes[i] != q {
                    i += 1;
                }
                syntax(i < end - 1)?;
                let value = &bytes[begin..i];
                i += 1;
                value
            } else {
                let begin = i;
                while i < end - 1 && !bytes[i].is_ascii_whitespace() {
                    syntax(!b"'\"<=`".contains(&bytes[i]))?;
                    i += 1;
                }
                &bytes[begin..i]
            }
        } else {
            b"".as_slice()
        };
        let raw = std::str::from_utf8(raw)
            .map_err(|_| anyhow::anyhow!("arxiv_source_text_encoding_unsupported"))?;
        source::capacity(raw.len() <= 65536 && attributes.len() < 64)?;
        attributes.push((key, raw.to_owned()));
    }
    Ok(Tag {
        name,
        closing,
        empty,
        attributes,
        start,
        end,
    })
}
fn decoded(value: &str) -> Result<String> {
    let value = quick_xml::escape::unescape(value)
        .map_err(|_| anyhow::anyhow!("arxiv_source_html_entity_unsupported"))?
        .into_owned();
    source::capacity(value.len() <= 65536)?;
    syntax(!value.contains('\0') && !crate::paths::obvious_secret(&value))?;
    Ok(value)
}
fn skip_raw(bytes: &[u8], mut pos: usize, name: &str, deadline: &Deadline) -> Result<usize> {
    let closing = format!("</{name}");
    let limit = bytes.len().min(1_000_000);
    loop {
        source::check(deadline)?;
        source::capacity(pos < limit)?;
        let Some(found) = bytes[pos..limit]
            .windows(closing.len())
            .position(|v| v.eq_ignore_ascii_case(closing.as_bytes()))
        else {
            anyhow::bail!("arxiv_source_html_profile_unsupported")
        };
        pos += found;
        if bytes
            .get(pos + closing.len())
            .is_some_and(|b| b.is_ascii_whitespace() || *b == b'>')
        {
            return Ok(tag(bytes, pos)?.end);
        }
        pos += closing.len();
    }
}
fn selected(name: &str) -> bool {
    matches!(
        name,
        "citation_title"
            | "citation_author"
            | "citation_date"
            | "citation_online_date"
            | "citation_pdf_url"
            | "citation_arxiv_id"
            | "citation_abstract"
    )
}
pub fn inspect(source: &ReceivedSource, max_bytes: usize, deadline: &Deadline) -> Result<Value> {
    source::check(deadline)?;
    source::capacity(max_bytes > 0 && max_bytes <= 16_000_000)?;
    let id = selector(source)?;
    let bytes = source.body();
    std::str::from_utf8(bytes)
        .map_err(|_| anyhow::anyhow!("arxiv_source_text_encoding_unsupported"))?;
    let hash = crate::model::hash(bytes);
    let mut pos = 0;
    let mut head = false;
    let mut ended = false;
    let mut stack = Vec::<String>::new();
    let mut observations = Vec::new();
    let mut decoded_bytes = 0usize;
    let mut events = 0usize;
    let scan_limit = bytes.len().min(1_000_000);
    while pos < bytes.len() {
        source::check(deadline)?;
        source::capacity(pos < scan_limit && events < 10000)?;
        let Some(offset) = bytes[pos..scan_limit].iter().position(|b| *b == b'<') else {
            break;
        };
        pos += offset;
        if bytes[pos..].starts_with(b"<!--") {
            source::capacity(pos.checked_add(4).is_some_and(|n| n <= scan_limit))?;
            let end = bytes[pos + 4..scan_limit]
                .windows(3)
                .position(|b| b == b"-->")
                .ok_or_else(|| anyhow::anyhow!("arxiv_source_html_profile_unsupported"))?;
            pos += 4 + end + 3;
            continue;
        }
        if bytes[pos..]
            .get(..9)
            .is_some_and(|b| b.eq_ignore_ascii_case(b"<!doctype"))
        {
            let end = bytes[pos..scan_limit]
                .iter()
                .position(|b| *b == b'>')
                .ok_or_else(|| anyhow::anyhow!("arxiv_source_html_profile_unsupported"))?;
            syntax(
                lower(&bytes[pos..pos + end + 1])
                    .split_whitespace()
                    .collect::<Vec<_>>()
                    .join(" ")
                    == "<!doctype html>",
            )?;
            pos += end + 1;
            continue;
        }
        let current = tag(bytes, pos)?;
        pos = current.end;
        events += 1;
        if !head {
            syntax(
                !current.closing
                    && !current.empty
                    && matches!(current.name.as_str(), "html" | "head"),
            )?;
            if current.name == "head" {
                head = true;
            }
            continue;
        }
        if current.name == "head" && current.closing {
            syntax(stack.is_empty())?;
            ended = true;
            break;
        }
        if current.name == "body" {
            anyhow::bail!("arxiv_source_html_profile_unsupported")
        }
        if current.closing {
            syntax(stack.last() == Some(&current.name))?;
            stack.pop();
            continue;
        }
        if matches!(current.name.as_str(), "script" | "style" | "title") && !current.empty {
            pos = skip_raw(bytes, pos, &current.name, deadline)?;
            continue;
        }
        if current.name == "meta" && stack.is_empty() {
            let names = current
                .attributes
                .iter()
                .filter(|(n, _)| n == "name")
                .map(|(_, v)| decoded(v).map(|v| v.to_ascii_lowercase()))
                .collect::<Result<Vec<_>>>()?;
            let values = current
                .attributes
                .iter()
                .filter(|(n, _)| n == "content")
                .map(|(_, v)| v)
                .collect::<Vec<_>>();
            if names.iter().any(|n| selected(n)) {
                syntax(names.len() == 1 && values.len() == 1)?;
                let name = &names[0];
                let value = decoded(values[0])?;
                decoded_bytes = decoded_bytes
                    .checked_add(value.len())
                    .ok_or_else(|| anyhow::anyhow!("source_acquisition_capacity_exceeded"))?;
                source::capacity(decoded_bytes <= 2_000_000 && observations.len() < 1024)?;
                observations.push(json!({"occurrenceId":crate::model::identity(&("arxiv-head-meta-v1",&hash,current.start,current.end,name)),"field":name,"value":value,"evidence":{"sourceHash":hash,"startByte":current.start,"endByte":current.end,"precision":"whole_received_meta_tag_not_character_map"},"executionObserved":false}));
            }
        } else if !current.empty
            && !matches!(
                current.name.as_str(),
                "meta"
                    | "link"
                    | "base"
                    | "br"
                    | "hr"
                    | "img"
                    | "input"
                    | "source"
                    | "track"
                    | "wbr"
                    | "area"
                    | "col"
                    | "embed"
                    | "param"
            )
        {
            syntax(matches!(current.name.as_str(), "template" | "noscript"))?;
            source::capacity(stack.len() < 64)?;
            stack.push(current.name);
        }
    }
    ensure!(
        head && ended && !observations.is_empty(),
        "arxiv_source_metadata_unavailable"
    );
    let mut counts = BTreeMap::<String, usize>::new();
    let mut ids = BTreeSet::new();
    for row in &observations {
        source::check(deadline)?;
        let field = row["field"].as_str().unwrap();
        *counts.entry(field.into()).or_default() += 1;
        if field == "citation_arxiv_id" {
            let value = row["value"].as_str().unwrap();
            validate_id(value)?;
            ids.insert(value.to_owned());
        }
    }
    let binding = if ids.is_empty() {
        "missing"
    } else if ids.iter().all(|value| base_id(value) == base_id(&id)) {
        "received_base_id_candidates_match_url_label"
    } else {
        "mismatch"
    };
    let value = json!({"schemaVersion":1,"kind":"native_received_arxiv_metadata_v1","selectorId":id,"sourceId":source.as_value()["sourceId"],"sourceHash":hash,"observations":observations,"fieldCounts":counts,"idBinding":binding,"versionBinding":"not_established_from_base_id_or_url_label_alone","parserPolicy":"finite_lexical_head_meta_xml_entity_subset_v1","authorIdentity":"unverified_labels_each_occurrence_separate","evidencePrecision":"received_body_whole_meta_tag_bytes","sourceAuthority":false,"serverAttestation":"unverified","canonicalCodeSnapshot":false,"networkConnections":0,"modelCalls":0,"projectCodeExecutionAuthorized":false,"complete":false,
        "limitations":["Received head metadata and URL labels are not publication/version/server truth or verified author identity. Parallel repeated labels remain distinct occurrences.","This finite lexical head/meta policy is not a browser DOM or article/paper extraction. External links/scripts/DTD resources are never fetched or executed; the entity decoder supports the XML/numeric subset only.","Whole-tag byte ranges quote original received body bytes. Decoded attributes are not character-mapped PDF/HTML source positions. Missing fields remain absent counts, never synthesized metadata."]});
    crate::json::encode_bounded(&value, max_bytes, deadline)
        .map_err(|_| anyhow::anyhow!("source_acquisition_capacity_exceeded"))?;
    source::check(deadline)?;
    Ok(value)
}
