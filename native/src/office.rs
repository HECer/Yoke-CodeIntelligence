//! Bounded Open XML package text extraction. No package member is executed or
//! extracted to disk; formula values are cached data, never recalculated.
use crate::{capture::Deadline, model::hash, paths::obvious_secret};
use anyhow::{Context, Result, bail, ensure};
use quick_xml::{NsReader, Reader, events::Event, name::ResolveResult};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    io::{Cursor, Read, Write},
};

const MAX_PART: usize = 16_000_000;
const MAX_EXPANDED: usize = 32_000_000;
const MAX_TEXT: usize = 2_000_000;
const MAX_REPORT: usize = 7_900_000;
const PACKAGE: &str = "http://schemas.openxmlformats.org/package/2006/relationships";
const CONTENT_TYPES: &str = "http://schemas.openxmlformats.org/package/2006/content-types";

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct XmlOrigin {
    pub(crate) role: String,
    pub(crate) part: String,
    pub(crate) part_hash: String,
    pub(crate) start_byte: usize,
    pub(crate) end_byte: usize,
}
pub(crate) struct DocumentSegment {
    pub(crate) text: String,
    pub(crate) part: String,
    pub(crate) origins: Vec<XmlOrigin>,
    pub(crate) sheet: Option<String>,
    pub(crate) cell: Option<String>,
    pub(crate) cell_type: Option<String>,
    pub(crate) formula: Option<String>,
}
pub(crate) struct DocumentExtraction {
    pub(crate) segments: Vec<DocumentSegment>,
    pub(crate) parts: BTreeMap<String, Vec<u8>>,
}

/// Additional provenance for document graphs; the historical inspection output
/// remains unchanged. XML spans are enclosing elements, not a character map.
pub(crate) fn extract_document(
    bytes: &[u8],
    format: &str,
    deadline: &Deadline,
) -> Result<DocumentExtraction> {
    let extracted = extract(bytes, format, deadline)?;
    let package = unpack(bytes, deadline)?;
    let mut part_hashes = BTreeMap::new();
    for (name, bytes) in &package {
        deadline.check()?;
        part_hashes.insert(name.as_str(), hash(bytes));
    }
    let mut parsed = BTreeMap::new();
    let mut selected = BTreeMap::new();
    let mut shared: Option<(String, Vec<usize>)> = None;
    let mut workbook: Option<String> = None;
    if format == "xlsx" {
        let root = links(&package, "", deadline)?;
        let main = root
            .iter()
            .find(|link| role(&link.kind, "officeDocument"))
            .and_then(|link| link.target.as_ref())
            .context("Workbook main part missing")?;
        workbook = Some(main.clone());
        parsed.insert(main.clone(), xml(part(&package, main)?, deadline)?);
        for link in links(&package, main, deadline)? {
            if role(&link.kind, "sharedStrings") {
                let path = link.target.context("External shared strings refused")?;
                let nodes = xml(part(&package, &path)?, deadline)?;
                let indices = nodes[0].children.clone();
                parsed.insert(path.clone(), nodes);
                shared = Some((path, indices));
            }
        }
    }
    let mut segments = Vec::new();
    for raw in extracted["segments"]
        .as_array()
        .context("Office segments missing")?
    {
        deadline.check()?;
        let path = raw["part"]
            .as_str()
            .context("Office segment part missing")?
            .to_owned();
        let start = raw["xmlStartByte"]
            .as_u64()
            .context("Office XML start missing")? as usize;
        let end = raw["xmlEndByte"]
            .as_u64()
            .context("Office XML end missing")? as usize;
        let bytes = part(&package, &path)?;
        crate::paths::utf8_range(std::str::from_utf8(bytes)?, start, end)?;
        let mut origins = vec![XmlOrigin {
            role: if format == "docx" {
                "paragraph"
            } else {
                "worksheetCell"
            }
            .into(),
            part: path.clone(),
            part_hash: part_hashes[path.as_str()].clone(),
            start_byte: start,
            end_byte: end,
        }];
        if format == "xlsx" {
            if !parsed.contains_key(&path) {
                parsed.retain(|name, _| {
                    Some(name) == workbook.as_ref()
                        || shared.as_ref().is_some_and(|(shared, _)| shared == name)
                });
                parsed.insert(path.clone(), xml(bytes, deadline)?);
            }
            let nodes = &parsed[&path];
            let cell = &nodes[nodes
                .binary_search_by_key(&start, |node| node.start)
                .map_err(|_| anyhow::anyhow!("Cell origin element absent"))?];
            ensure!(
                cell.end == end && sheet(&cell.ns) && cell.name == "c",
                "Cell origin element differs"
            );
            if raw["cellType"] == "s" {
                let value = cell
                    .children
                    .iter()
                    .map(|&id| &nodes[id])
                    .find(|node| sheet(&node.ns) && node.name == "v")
                    .context("Shared string reference absent")?;
                let index = value.text.parse::<usize>()?;
                let (shared_path, indices) =
                    shared.as_ref().context("Shared string origin absent")?;
                let node = &parsed[shared_path][*indices
                    .get(index)
                    .context("Shared string source index outside table")?];
                origins.push(XmlOrigin {
                    role: "sharedStringValue".into(),
                    part: shared_path.clone(),
                    part_hash: part_hashes[shared_path.as_str()].clone(),
                    start_byte: node.start,
                    end_byte: node.end,
                });
            }
            let workbook_path = workbook.as_ref().context("Workbook source absent")?;
            let sheet_name = raw["sheet"].as_str().context("Worksheet label absent")?;
            let sheet_node = parsed[workbook_path]
                .iter()
                .find(|node| {
                    sheet(&node.ns)
                        && node.name == "sheet"
                        && node.attr("name") == Some(sheet_name)
                        && node.parent.is_some_and(|parent| {
                            sheet(&parsed[workbook_path][parent].ns)
                                && parsed[workbook_path][parent].name == "sheets"
                                && parsed[workbook_path][parent].parent == Some(0)
                        })
                })
                .context("Worksheet label source absent")?;
            origins.push(XmlOrigin {
                role: "workbookSheet".into(),
                part: workbook_path.clone(),
                part_hash: part_hashes[workbook_path.as_str()].clone(),
                start_byte: sheet_node.start,
                end_byte: sheet_node.end,
            });
        }
        for origin in &origins {
            if !selected.contains_key(&origin.part) {
                selected.insert(origin.part.clone(), part(&package, &origin.part)?.to_vec());
            }
        }
        segments.push(DocumentSegment {
            text: raw["text"]
                .as_str()
                .context("Office decoded text absent")?
                .into(),
            part: path,
            origins,
            sheet: raw["sheet"].as_str().map(str::to_owned),
            cell: raw["cell"].as_str().map(str::to_owned),
            cell_type: raw["cellType"].as_str().map(str::to_owned),
            formula: raw["formula"].as_str().map(str::to_owned),
        });
    }
    Ok(DocumentExtraction {
        segments,
        parts: selected,
    })
}
fn word(ns: &str) -> bool {
    matches!(
        ns,
        "http://schemas.openxmlformats.org/wordprocessingml/2006/main"
            | "http://purl.oclc.org/ooxml/wordprocessingml/main"
    )
}
fn sheet(ns: &str) -> bool {
    matches!(
        ns,
        "http://schemas.openxmlformats.org/spreadsheetml/2006/main"
            | "http://purl.oclc.org/ooxml/spreadsheetml/main"
    )
}
fn relationship(ns: &str) -> bool {
    matches!(
        ns,
        "http://schemas.openxmlformats.org/officeDocument/2006/relationships"
            | "http://purl.oclc.org/ooxml/officeDocument/relationships"
    )
}
fn u16_at(bytes: &[u8], position: usize) -> Result<usize> {
    Ok(u16::from_le_bytes(
        bytes
            .get(position..position + 2)
            .context("Truncated ZIP field")?
            .try_into()?,
    ) as usize)
}
fn u32_at(bytes: &[u8], position: usize) -> Result<usize> {
    Ok(u32::from_le_bytes(
        bytes
            .get(position..position + 4)
            .context("Truncated ZIP field")?
            .try_into()?,
    ) as usize)
}
fn part_name(name: &str) -> Result<()> {
    ensure!(
        !name.is_empty()
            && name.len() <= 1024
            && !name.starts_with('/')
            && !name.contains(['\\', ':', '\0'])
            && !name.chars().any(char::is_control),
        "Invalid package part path"
    );
    ensure!(
        name.trim_end_matches('/')
            .split('/')
            .all(|part| !part.is_empty() && part != "." && part != ".."),
        "Traversing package part path"
    );
    Ok(())
}
// Admit directory sizes/counts before the ZIP library allocates directory state.
fn admit_zip(bytes: &[u8], deadline: &Deadline) -> Result<Vec<String>> {
    ensure!(
        bytes.len() >= 22 && bytes.len() <= 50_000_000 && bytes.starts_with(b"PK\x03\x04"),
        "Unsupported Open XML ZIP container"
    );
    let end = (bytes.len().saturating_sub(65557)..=bytes.len() - 22)
        .rev()
        .find(|&position| {
            bytes.get(position..position + 4) == Some(b"PK\x05\x06")
                && u16_at(bytes, position + 20)
                    .is_ok_and(|length| position + 22 + length == bytes.len())
        })
        .context("Missing ZIP end directory")?;
    let count = u16_at(bytes, end + 10)?;
    ensure!(
        u16_at(bytes, end + 4)? == 0
            && u16_at(bytes, end + 6)? == 0
            && u16_at(bytes, end + 8)? == count
            && (1..=2048).contains(&count),
        "Multidisk, ZIP64 or excessive ZIP entry count"
    );
    let directory_size = u32_at(bytes, end + 12)?;
    let directory = u32_at(bytes, end + 16)?;
    ensure!(
        directory_size <= 2_000_000 && directory.checked_add(directory_size) == Some(end),
        "Unsupported ZIP directory bounds"
    );
    let mut position = directory;
    let mut names = BTreeSet::new();
    let mut admitted_names = Vec::new();
    let mut spans = vec![];
    let mut expanded = 0usize;
    for _ in 0..count {
        deadline.check()?;
        ensure!(
            bytes.get(position..position + 4) == Some(b"PK\x01\x02"),
            "Invalid ZIP directory member"
        );
        let flags = u16_at(bytes, position + 8)?;
        let method = u16_at(bytes, position + 10)?;
        let compressed = u32_at(bytes, position + 20)?;
        let size = u32_at(bytes, position + 24)?;
        let name_length = u16_at(bytes, position + 28)?;
        let extra = u16_at(bytes, position + 30)?;
        let comment = u16_at(bytes, position + 32)?;
        ensure!(
            flags & 0x2041 == 0
                && matches!(method, 0 | 8)
                && size <= MAX_PART
                && compressed <= 50_000_000
                && name_length <= 1024
                && extra <= 8192
                && comment <= 1024
                && u16_at(bytes, position + 34)? == 0,
            "Encrypted or unsupported ZIP member"
        );
        expanded = expanded
            .checked_add(size)
            .context("ZIP expansion overflow")?;
        ensure!(
            expanded <= MAX_EXPANDED,
            "ZIP expanded byte capacity exceeded"
        );
        let name = std::str::from_utf8(
            bytes
                .get(position + 46..position + 46 + name_length)
                .context("Truncated ZIP name")?,
        )?;
        part_name(name)?;
        ensure!(
            names.insert(name.to_lowercase()),
            "Duplicate or case-aliased package part"
        );
        admitted_names.push(name.to_owned());
        let attributes = u32_at(bytes, position + 38)?;
        let kind = (attributes >> 16) & 0o170000;
        ensure!(
            matches!(kind, 0 | 0o100000 | 0o040000) && attributes & 0x400 == 0,
            "Linked or special ZIP member refused"
        );
        let local = u32_at(bytes, position + 42)?;
        ensure!(
            local < directory
                && bytes.get(local..local + 4) == Some(b"PK\x03\x04")
                && u16_at(bytes, local + 6)? == flags
                && u16_at(bytes, local + 8)? == method,
            "ZIP local header mismatch"
        );
        let local_name = u16_at(bytes, local + 26)?;
        let local_extra = u16_at(bytes, local + 28)?;
        ensure!(
            local_name == name_length
                && local_extra <= 8192
                && bytes.get(local + 30..local + 30 + local_name) == Some(name.as_bytes()),
            "ZIP local part name mismatch"
        );
        let data = local
            .checked_add(30 + local_name + local_extra)
            .context("ZIP member offset overflow")?;
        let finish = data
            .checked_add(compressed)
            .context("ZIP member offset overflow")?;
        ensure!(finish <= directory, "ZIP member overlaps directory");
        spans.push((local, finish));
        position = position
            .checked_add(46 + name_length + extra + comment)
            .context("ZIP directory overflow")?;
        ensure!(position <= end, "Truncated ZIP directory");
    }
    spans.sort_unstable();
    ensure!(
        position == end && spans.windows(2).all(|pair| pair[0].1 <= pair[1].0),
        "Overlapping ZIP members or directory data"
    );
    Ok(admitted_names)
}
fn unpack(bytes: &[u8], deadline: &Deadline) -> Result<BTreeMap<String, Vec<u8>>> {
    let admitted_names = admit_zip(bytes, deadline)?;
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes))?;
    ensure!(
        archive.len() == admitted_names.len(),
        "ZIP member count changed"
    );
    let mut parts = BTreeMap::new();
    let mut expanded = 0usize;
    for (i, admitted_name) in admitted_names.iter().enumerate() {
        deadline.check()?;
        let mut file = archive.by_index(i)?;
        ensure!(
            !file.encrypted() && !file.is_symlink() && file.size() <= MAX_PART as u64,
            "Unsupported package member"
        );
        let name = file.name()?.into_owned();
        ensure!(name == *admitted_name, "ZIP decoded part identity changed");
        if file.is_dir() {
            continue;
        }
        let expected = file.size() as usize;
        ensure!(
            expanded.saturating_add(expected) <= MAX_EXPANDED,
            "Package expansion capacity exceeded"
        );
        let mut output = Vec::new();
        let mut buffer = [0u8; 32768];
        loop {
            deadline.check()?;
            let length = file.read(&mut buffer)?;
            if length == 0 {
                break;
            }
            ensure!(
                output.len().saturating_add(length) <= expected,
                "Package member exceeds declared size"
            );
            output.extend_from_slice(&buffer[..length]);
        }
        ensure!(output.len() == expected, "Truncated package member");
        ensure!(
            !obvious_secret(&String::from_utf8_lossy(&output)),
            "Package contains an obvious secret"
        );
        expanded += output.len();
        parts.insert(name, output);
    }
    Ok(parts)
}

struct Node {
    ns: String,
    name: String,
    attrs: BTreeMap<(String, String), String>,
    parent: Option<usize>,
    children: Vec<usize>,
    text: String,
    start: usize,
    end: usize,
}
impl Node {
    fn attr(&self, name: &str) -> Option<&str> {
        self.attrs
            .get(&(String::new(), name.into()))
            .map(String::as_str)
    }
}
fn namespace(value: ResolveResult<'_>) -> Result<String> {
    match value {
        ResolveResult::Bound(value) => {
            ensure!(
                value.as_ref().len() <= 256,
                "XML namespace capacity exceeded"
            );
            Ok(value.as_ref().to_owned())
        }
        ResolveResult::Unbound => Ok(String::new()),
        ResolveResult::Unknown(_) => bail!("Unbound XML namespace"),
    }
}
fn xml_scalar(c: char) -> bool {
    matches!(c, '\t' | '\n' | '\r') || (c >= ' ' && c != '\u{fffe}' && c != '\u{ffff}')
}
// A namespace-free pass admits declaration storage before NsReader registers
// any untrusted prefix/URI bindings. Text is bounded before DOM copies as well.
fn admit_xml(text: &str, deadline: &Deadline) -> Result<()> {
    let mut reader = Reader::from_str(text);
    let (mut nodes, mut attributes, mut depth, mut decoded) = (0usize, 0usize, 0usize, 0usize);
    loop {
        deadline.check()?;
        let event = reader.read_event()?;
        let empty = matches!(&event, Event::Empty(_));
        match event {
            Event::Start(event) | Event::Empty(event) => {
                nodes += 1;
                ensure!(
                    nodes <= 100_000 && event.len() <= 64000 && event.name().as_ref().len() <= 512,
                    "XML node/tag capacity exceeded"
                );
                if !empty {
                    depth += 1;
                    ensure!(depth <= 128, "XML depth capacity exceeded");
                }
                let mut count = 0;
                for attr in event.attributes() {
                    let attr = attr?;
                    count += 1;
                    attributes += 1;
                    let key = attr.key.as_ref();
                    let namespace = key == "xmlns" || key.starts_with("xmlns:");
                    ensure!(
                        count <= 128
                            && attributes <= 100_000
                            && key.len() <= 256
                            && attr.value.len() <= if namespace { 256 } else { 16000 },
                        "XML attribute/namespace capacity exceeded"
                    );
                }
            }
            Event::End(_) => {
                depth = depth.checked_sub(1).context("Unbalanced XML depth")?;
            }
            Event::Text(value) => {
                let length = value.xml10_content().len();
                ensure!(
                    length <= MAX_TEXT.saturating_sub(decoded),
                    "XML decoded text capacity exceeded"
                );
                decoded += length;
            }
            Event::CData(value) => {
                let length = value.xml10_content().len();
                ensure!(
                    length <= MAX_TEXT.saturating_sub(decoded),
                    "XML decoded text capacity exceeded"
                );
                decoded += length;
            }
            Event::GeneralRef(_) => {
                ensure!(decoded < MAX_TEXT, "XML reference capacity exceeded");
                decoded += 1;
            }
            Event::Decl(value) => {
                ensure!(
                    value.version()?.as_ref() == "1.0",
                    "Unsupported XML version"
                );
                if let Some(encoding) = value.encoding() {
                    let encoding = encoding?;
                    ensure!(
                        encoding.eq_ignore_ascii_case("utf-8")
                            || encoding.eq_ignore_ascii_case("utf8"),
                        "Unsupported XML encoding declaration"
                    );
                }
            }
            Event::DocType(_) => bail!("XML document types and external entities are unsupported"),
            Event::Eof => break,
            _ => {}
        }
    }
    ensure!(depth == 0, "Unbalanced XML depth");
    Ok(())
}
fn xml(bytes: &[u8], deadline: &Deadline) -> Result<Vec<Node>> {
    let text =
        std::str::from_utf8(bytes).context("Open XML part must use supported UTF-8 encoding")?;
    ensure!(
        text.chars().all(xml_scalar),
        "Invalid XML control character"
    );
    admit_xml(text, deadline)?;
    let mut reader = NsReader::from_str(text);
    let mut nodes: Vec<Node> = vec![];
    let mut stack: Vec<usize> = vec![];
    let mut roots = 0;
    let mut decoded = 0usize;
    let mut attribute_count = 0usize;
    loop {
        deadline.check()?;
        let start = reader.buffer_position() as usize;
        let (resolved, event) = reader.read_resolved_event()?;
        let ns = namespace(resolved)?;
        let end = reader.buffer_position() as usize;
        match event {
            Event::Start(event) | Event::Empty(event) => {
                let empty = bytes.get(end.saturating_sub(2)..end) == Some(b"/>");
                ensure!(
                    nodes.len() < 100_000 && stack.len() < 128,
                    "Open XML node/depth capacity exceeded"
                );
                ensure!(
                    event.local_name().as_ref().len() <= 256,
                    "XML name capacity exceeded"
                );
                let name = event.local_name().as_ref().to_owned();
                let mut attrs = BTreeMap::new();
                for attr in event.attributes() {
                    let attr = attr?;
                    let key = attr.key.as_ref();
                    attribute_count += 1;
                    ensure!(
                        attribute_count <= 100_000 && key.len() <= 256,
                        "XML attribute count/name capacity exceeded"
                    );
                    if key == "xmlns" || key.starts_with("xmlns:") {
                        ensure!(attr.value.len() <= 256, "XML namespace capacity exceeded");
                        continue;
                    }
                    ensure!(
                        attrs.len() < 128 && attr.value.len() <= 16000,
                        "XML attribute capacity exceeded"
                    );
                    let (attr_ns, local) = reader.resolver().resolve_attribute(attr.key);
                    let key = (namespace(attr_ns)?, local.as_ref().to_owned());
                    let value = attr
                        .normalized_value(quick_xml::XmlVersion::Implicit1_0)?
                        .into_owned();
                    ensure!(
                        value.chars().all(xml_scalar)
                            && !obvious_secret(&value)
                            && attrs.insert(key, value).is_none(),
                        "Duplicate, invalid or secret XML attribute"
                    );
                }
                let id = nodes.len();
                if let Some(&parent) = stack.last() {
                    nodes[parent].children.push(id);
                } else {
                    roots += 1;
                }
                nodes.push(Node {
                    ns,
                    name,
                    attrs,
                    parent: stack.last().copied(),
                    children: vec![],
                    text: String::new(),
                    start,
                    end,
                });
                if !empty {
                    stack.push(id);
                }
            }
            Event::End(_) => {
                let id = stack.pop().context("Unbalanced XML end")?;
                nodes[id].end = end;
            }
            Event::Text(event) => {
                let value = event.xml10_content();
                if let Some(&id) = stack.last() {
                    ensure!(
                        value.len() <= MAX_TEXT.saturating_sub(decoded),
                        "XML decoded text capacity exceeded"
                    );
                    decoded += value.len();
                    nodes[id].text.push_str(&value);
                } else {
                    ensure!(value.trim().is_empty(), "Text outside XML root");
                }
            }
            Event::CData(event) => {
                let id = *stack.last().context("CDATA outside XML root")?;
                let value = event.xml10_content();
                ensure!(
                    value.len() <= MAX_TEXT.saturating_sub(decoded),
                    "XML text capacity exceeded"
                );
                decoded += value.len();
                nodes[id].text.push_str(&value);
            }
            Event::GeneralRef(event) => {
                let id = *stack.last().context("Reference outside XML root")?;
                let value = if let Some(c) = event.resolve_char_ref()? {
                    c.to_string()
                } else {
                    match event.xml10_content().as_ref() {
                        "amp" => "&",
                        "lt" => "<",
                        "gt" => ">",
                        "quot" => "\"",
                        "apos" => "'",
                        _ => bail!("Unknown XML entity"),
                    }
                    .to_owned()
                };
                ensure!(
                    value.chars().all(xml_scalar),
                    "Invalid XML character reference"
                );
                ensure!(
                    value.len() <= MAX_TEXT.saturating_sub(decoded),
                    "XML text capacity exceeded"
                );
                decoded += value.len();
                nodes[id].text.push_str(&value);
            }
            Event::DocType(_) => bail!("XML document types and external entities are unsupported"),
            Event::Eof => break,
            _ => {}
        }
    }
    ensure!(
        roots == 1 && stack.is_empty(),
        "Missing or unbalanced XML root"
    );
    Ok(nodes)
}
fn part<'a>(parts: &'a BTreeMap<String, Vec<u8>>, name: &str) -> Result<&'a [u8]> {
    parts
        .get(name)
        .map(Vec::as_slice)
        .context("Required Open XML part missing")
}
fn target(base: &str, value: &str) -> Result<String> {
    ensure!(
        !value.is_empty() && value.len() <= 1024 && !value.contains(['\\', ':', '?', '#', '\0']),
        "External or unsupported package target"
    );
    let mut decoded = Vec::new();
    let mut offset = 0;
    let bytes = value.as_bytes();
    while offset < bytes.len() {
        if bytes[offset] == b'%' {
            ensure!(offset + 2 < bytes.len(), "Invalid encoded package path");
            let digit = |b: u8| char::from(b).to_digit(16).context("Invalid path escape");
            decoded.push((digit(bytes[offset + 1])? * 16 + digit(bytes[offset + 2])?) as u8);
            offset += 3;
        } else {
            decoded.push(bytes[offset]);
            offset += 1;
        }
    }
    let value = std::str::from_utf8(&decoded)?;
    ensure!(
        !value.contains(['\\', ':', '?', '#', '\0']) && !value.starts_with("//"),
        "Invalid decoded package target"
    );
    let mut components: Vec<&str> = if value.starts_with('/') {
        vec![]
    } else {
        base.rsplit_once('/')
            .map_or(vec![], |(parent, _)| parent.split('/').collect())
    };
    for item in value.trim_start_matches('/').split('/') {
        match item {
            "" => bail!("Empty package target component"),
            "." => {}
            ".." => {
                components.pop().context("Package target escapes archive")?;
            }
            _ => components.push(item),
        }
    }
    let result = components.join("/");
    part_name(&result)?;
    Ok(result)
}
struct Link {
    id: String,
    kind: String,
    target: Option<String>,
}
fn links(parts: &BTreeMap<String, Vec<u8>>, base: &str, deadline: &Deadline) -> Result<Vec<Link>> {
    let name = if base.is_empty() {
        "_rels/.rels".into()
    } else {
        let (parent, file) = base.rsplit_once('/').unwrap_or(("", base));
        format!(
            "{}{}/{}.rels",
            if parent.is_empty() {
                String::new()
            } else {
                format!("{parent}/")
            },
            "_rels",
            file
        )
    };
    let Some(bytes) = parts.get(&name) else {
        return Ok(vec![]);
    };
    let nodes = xml(bytes, deadline)?;
    ensure!(
        nodes[0].ns == PACKAGE && nodes[0].name == "Relationships",
        "Invalid package relationship root"
    );
    let mut result = vec![];
    let mut ids = BTreeSet::new();
    for &index in &nodes[0].children {
        let node = &nodes[index];
        ensure!(
            node.ns == PACKAGE && node.name == "Relationship",
            "Invalid relationship element"
        );
        let id = node
            .attr("Id")
            .context("Relationship ID missing")?
            .to_owned();
        ensure!(ids.insert(id.clone()), "Duplicate relationship ID");
        let kind = node
            .attr("Type")
            .context("Relationship type missing")?
            .to_owned();
        let raw = node.attr("Target").context("Relationship target missing")?;
        let resolved = match node.attr("TargetMode") {
            Some("External") => None,
            None | Some("Internal") => Some(target(base, raw)?),
            _ => bail!("Invalid relationship target mode"),
        };
        result.push(Link {
            id,
            kind,
            target: resolved,
        });
    }
    Ok(result)
}
fn role(kind: &str, name: &str) -> bool {
    kind.strip_prefix("http://schemas.openxmlformats.org/officeDocument/2006/relationships/")
        .or_else(|| kind.strip_prefix("http://purl.oclc.org/ooxml/officeDocument/relationships/"))
        .is_some_and(|role| role == name)
}
fn rich_text(
    nodes: &[Node],
    root: usize,
    word_mode: bool,
    limit: usize,
    deadline: &Deadline,
) -> Result<String> {
    let mut result = String::new();
    let mut stack = vec![root];
    while let Some(index) = stack.pop() {
        deadline.check()?;
        let node = &nodes[index];
        let matching = if word_mode {
            word(&node.ns)
        } else {
            sheet(&node.ns)
        };
        if matching && node.name == "t" {
            ensure!(
                node.text.len() <= limit.saturating_sub(result.len()),
                "Office text capacity exceeded"
            );
            result.push_str(&node.text);
        } else if word_mode && matching && node.name == "tab" {
            ensure!(result.len() < limit, "Office text capacity exceeded");
            result.push('\t');
        } else if word_mode && matching && matches!(node.name.as_str(), "br" | "cr") {
            ensure!(result.len() < limit, "Office text capacity exceeded");
            result.push('\n');
        } else if index != root
            && matching
            && ((word_mode && node.name == "p") || (!word_mode && node.name == "rPh"))
        {
            continue;
        } else {
            stack.extend(node.children.iter().rev().copied());
        }
        ensure!(result.len() <= limit, "Office text capacity exceeded");
    }
    ensure!(
        !obvious_secret(&result),
        "Extracted Office text contains an obvious secret"
    );
    Ok(result)
}
fn segment(part: &str, part_hash: &str, node: &Node, text: String) -> Value {
    json!({"part":part,"partHash":part_hash,"xmlStartByte":node.start,"xmlEndByte":node.end,"text":text,"origin":"decoded_openxml_content_not_rendered_layout"})
}

pub(crate) fn extract(bytes: &[u8], format: &str, deadline: &Deadline) -> Result<Value> {
    ensure!(
        matches!(format, "docx" | "xlsx"),
        "Unsupported Office format"
    );
    let parts = unpack(bytes, deadline)?;
    let root = links(&parts, "", deadline)?;
    let main: Vec<_> = root
        .iter()
        .filter(|link| role(&link.kind, "officeDocument"))
        .collect();
    ensure!(main.len() == 1, "One main Office relationship required");
    let main = main[0]
        .target
        .as_deref()
        .context("External main Office document refused")?;
    let types = xml(part(&parts, "[Content_Types].xml")?, deadline)?;
    ensure!(
        types[0].ns == CONTENT_TYPES && types[0].name == "Types",
        "Invalid Office content types"
    );
    let expected = if format == "docx" {
        "application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"
    } else {
        "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"
    };
    let overrides: Vec<_> = types
        .iter()
        .filter(|node| {
            node.ns == CONTENT_TYPES
                && node.name == "Override"
                && node.attr("PartName") == Some(format!("/{main}").as_str())
        })
        .collect();
    ensure!(
        overrides.len() == 1 && overrides[0].attr("ContentType") == Some(expected),
        "Office content type mismatch or macro-enabled main part"
    );
    let relationships = links(&parts, main, deadline)?;
    let mut segments = vec![];
    let mut report_bytes = 0usize;
    let mut total = 0usize;
    if format == "docx" {
        let mut selected = vec![(main.to_owned(), "document")];
        for link in &relationships {
            if let Some((_, root)) = [
                ("header", "hdr"),
                ("footer", "ftr"),
                ("footnotes", "footnotes"),
                ("endnotes", "endnotes"),
                ("comments", "comments"),
            ]
            .into_iter()
            .find(|(name, _)| role(&link.kind, name))
            {
                selected.push((
                    link.target
                        .clone()
                        .context("External Word text part refused")?,
                    root,
                ));
            }
        }
        let mut seen = BTreeMap::new();
        for (name, root) in selected {
            if let Some(previous) = seen.insert(name.clone(), root) {
                ensure!(previous == root, "Conflicting Word part roles");
                continue;
            }
            let bytes = part(&parts, &name)?;
            let part_hash = hash(bytes);
            let nodes = xml(bytes, deadline)?;
            ensure!(
                word(&nodes[0].ns) && nodes[0].name == root,
                "Invalid Word part root"
            );
            if root == "document" {
                ensure!(
                    nodes[0]
                        .children
                        .iter()
                        .filter(|&&id| word(&nodes[id].ns) && nodes[id].name == "body")
                        .count()
                        == 1,
                    "One Word body required"
                );
            }
            for (id, node) in nodes.iter().enumerate() {
                if word(&node.ns) && node.name == "p" {
                    let text =
                        rich_text(&nodes, id, true, MAX_TEXT.saturating_sub(total), deadline)?;
                    total += text.len();
                    ensure!(
                        total <= MAX_TEXT && segments.len() < 20_000,
                        "Office text/segment capacity exceeded"
                    );
                    push_segment(
                        &mut segments,
                        &mut report_bytes,
                        segment(&name, &part_hash, node, text),
                        deadline,
                    )?;
                }
            }
        }
    } else {
        spreadsheet(
            &parts,
            main,
            &relationships,
            deadline,
            &mut segments,
            &mut total,
            &mut report_bytes,
        )?;
    }
    deadline.check()?;
    Ok(
        json!({"metadata":{"kind":"document","format":format,"nativeTextAvailable":true,"provider":"yoke-native-openxml-1","version":"zip-9.0.3/quick-xml-0.42.0","exhaustive":false,"textBytes":total,"segmentCount":segments.len(),"packageParts":parts.len(),"formulaEvaluation":"never","externalRelationships":"never_followed"},"segments":segments,"limitations":["Decoded package text and cached cell values are not rendered Office layout, recalculated formulas, OCR or macro execution.","Only UTF-8 XML, bounded non-ZIP64 stored/deflate packages and supported main document types are admitted."]}),
    )
}

fn spreadsheet(
    parts: &BTreeMap<String, Vec<u8>>,
    main: &str,
    relationships: &[Link],
    deadline: &Deadline,
    segments: &mut Vec<Value>,
    total: &mut usize,
    report_bytes: &mut usize,
) -> Result<()> {
    let workbook = xml(part(parts, main)?, deadline)?;
    ensure!(
        sheet(&workbook[0].ns) && workbook[0].name == "workbook",
        "Invalid workbook root"
    );
    let shared_links: Vec<_> = relationships
        .iter()
        .filter(|link| role(&link.kind, "sharedStrings"))
        .collect();
    ensure!(shared_links.len() <= 1, "Ambiguous shared string table");
    let mut strings = vec![];
    if let Some(link) = shared_links.first() {
        let nodes = xml(
            part(
                parts,
                link.target
                    .as_deref()
                    .context("External shared strings refused")?,
            )?,
            deadline,
        )?;
        ensure!(
            sheet(&nodes[0].ns) && nodes[0].name == "sst",
            "Invalid shared strings root"
        );
        let mut size = 0usize;
        for &id in &nodes[0].children {
            ensure!(
                sheet(&nodes[id].ns) && nodes[id].name == "si",
                "Invalid shared string member"
            );
            let text = rich_text(&nodes, id, false, MAX_TEXT.saturating_sub(size), deadline)?;
            size += text.len();
            ensure!(
                size <= MAX_TEXT && strings.len() < 50_000,
                "Shared string capacity exceeded"
            );
            strings.push(text);
        }
    }
    let mut sheet_names = BTreeSet::new();
    let mut sheet_parts = BTreeSet::new();
    for sheet_node in workbook.iter().filter(|node| {
        sheet(&node.ns)
            && node.name == "sheet"
            && node.parent.is_some_and(|id| {
                sheet(&workbook[id].ns)
                    && workbook[id].name == "sheets"
                    && workbook[id].parent == Some(0)
            })
    }) {
        let name = sheet_node.attr("name").context("Sheet name missing")?;
        ensure!(
            sheet_names.insert(name.to_lowercase()),
            "Duplicate sheet name"
        );
        let ids: Vec<_> = sheet_node
            .attrs
            .iter()
            .filter(|((ns, key), _)| relationship(ns) && key == "id")
            .map(|(_, value)| value.as_str())
            .collect();
        ensure!(ids.len() == 1, "Sheet relationship missing or ambiguous");
        let selected: Vec<_> = relationships
            .iter()
            .filter(|link| link.id == ids[0] && role(&link.kind, "worksheet"))
            .collect();
        ensure!(selected.len() == 1, "Worksheet relationship missing");
        let path = selected[0]
            .target
            .as_deref()
            .context("External worksheet refused")?;
        ensure!(
            sheet_parts.insert(path.to_owned()),
            "Duplicate worksheet part"
        );
        let bytes = part(parts, path)?;
        let part_hash = hash(bytes);
        let nodes = xml(bytes, deadline)?;
        ensure!(
            sheet(&nodes[0].ns) && nodes[0].name == "worksheet",
            "Invalid worksheet root"
        );
        let mut addresses = BTreeSet::new();
        for (id, node) in nodes.iter().enumerate().filter(|(_, node)| {
            sheet(&node.ns)
                && node.name == "c"
                && node.parent.is_some_and(|row| {
                    sheet(&nodes[row].ns)
                        && nodes[row].name == "row"
                        && nodes[row].parent.is_some_and(|data| {
                            sheet(&nodes[data].ns)
                                && nodes[data].name == "sheetData"
                                && nodes[data].parent == Some(0)
                        })
                })
        }) {
            let address = node
                .attr("r")
                .context("Explicit worksheet cell address required")?;
            cell_address(address)?;
            ensure!(
                addresses.insert(address.to_owned()),
                "Duplicate worksheet cell"
            );
            let values: Vec<_> = node
                .children
                .iter()
                .map(|&id| &nodes[id])
                .filter(|node| sheet(&node.ns) && node.name == "v")
                .collect();
            let formulas: Vec<_> = node
                .children
                .iter()
                .map(|&id| &nodes[id])
                .filter(|node| sheet(&node.ns) && node.name == "f")
                .collect();
            ensure!(
                values.len() <= 1 && formulas.len() <= 1,
                "Ambiguous cell value or formula"
            );
            let raw = values.first().map(|node| node.text.as_str());
            let kind = node.attr("t").unwrap_or("n");
            let text = match (kind, raw) {
                ("s", Some(value)) => copy_cell_text(
                    strings
                        .get(value.parse::<usize>()?)
                        .context("Shared string index out of bounds")?,
                    *total,
                )?,
                ("inlineStr", _) => {
                    rich_text(&nodes, id, false, MAX_TEXT.saturating_sub(*total), deadline)?
                }
                ("b", Some("0")) => "false".into(),
                ("b", Some("1")) => "true".into(),
                ("b", Some(_)) => bail!("Invalid boolean cell value"),
                ("n" | "str" | "d" | "e", Some(value)) => copy_cell_text(value, *total)?,
                ("n" | "str" | "d" | "e" | "s" | "b", None) => String::new(),
                _ => bail!("Unsupported cell type"),
            };
            *total += text.len();
            ensure!(
                *total <= MAX_TEXT && segments.len() < 20_000 && !obvious_secret(&text),
                "Office cell text/segment capacity or secret refusal"
            );
            let mut value = segment(path, &part_hash, node, text);
            value["sheet"] = json!(name);
            value["cell"] = json!(address);
            value["cellType"] = json!(kind);
            value["cachedValueAvailable"] = json!(raw.is_some() || kind == "inlineStr");
            if let Some(formula) = formulas.first() {
                *total += formula.text.len();
                ensure!(
                    *total <= MAX_TEXT && !obvious_secret(&formula.text),
                    "Formula text capacity or secret refusal"
                );
                value["formula"] = json!(formula.text);
                value["formulaEvaluated"] = json!(false);
            }
            push_segment(segments, report_bytes, value, deadline)?;
        }
    }
    Ok(())
}
fn push_segment(
    segments: &mut Vec<Value>,
    used: &mut usize,
    value: Value,
    deadline: &Deadline,
) -> Result<()> {
    struct Budget<'a> {
        used: &'a mut usize,
        deadline: &'a Deadline,
    }
    impl Write for Budget<'_> {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.deadline.check().map_err(std::io::Error::other)?;
            if bytes.len() > MAX_REPORT.saturating_sub(*self.used) {
                return Err(std::io::Error::other(
                    "Office report byte capacity exceeded",
                ));
            }
            *self.used += bytes.len();
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut budget = Budget { used, deadline };
    serde_json::to_writer(&mut budget, &value)?;
    budget.write_all(b",")?;
    segments.push(value);
    Ok(())
}
fn copy_cell_text(value: &str, total: usize) -> Result<String> {
    ensure!(
        value.len() <= MAX_TEXT.saturating_sub(total),
        "Office cell text capacity exceeded"
    );
    Ok(value.to_owned())
}
fn cell_address(value: &str) -> Result<()> {
    ensure!(
        !value.is_empty() && value.len() <= 10,
        "Invalid cell address"
    );
    let split = value
        .bytes()
        .position(|b| b.is_ascii_digit())
        .context("Cell row missing")?;
    let letters = &value[..split];
    let digits = &value[split..];
    ensure!(
        !letters.is_empty()
            && letters.bytes().all(|b| b.is_ascii_uppercase())
            && digits.bytes().all(|b| b.is_ascii_digit())
            && !digits.starts_with('0'),
        "Invalid cell address"
    );
    let column = letters
        .bytes()
        .try_fold(0usize, |total, b| {
            total.checked_mul(26)?.checked_add((b - b'A' + 1) as usize)
        })
        .context("Cell column overflow")?;
    let row = digits.parse::<usize>()?;
    ensure!(
        column <= 16384 && (1..=1_048_576).contains(&row),
        "Cell address exceeds worksheet bounds"
    );
    Ok(())
}
