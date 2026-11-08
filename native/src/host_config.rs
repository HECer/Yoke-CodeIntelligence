//! Pure, lossless edits for explicitly selected host configuration bytes.
use crate::capture::Deadline;
use crate::model::hash;
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
#[path = "host_config_claude_hooks.rs"]
mod claude_hooks;
#[path = "host_config_mcp.rs"]
mod mcp;
pub use claude_hooks::patch as claude_hook_settings_patch;
pub use claude_hooks::patch_source as claude_source_hook_settings_patch;
pub use mcp::json_mcp_patch;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Format {
    AgentsMarkdown,
    JsoncNamedEntry,
    JsonMcpNamedEntryV1,
    ClaudeHookSettingsV1,
    ClaudeSourceHookSettingsV1,
    TomlTable,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ByteEdit {
    pub start_byte: usize,
    pub end_byte: usize,
    pub expected: Vec<u8>,
    pub replacement: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OwnedFragment {
    pub format: Format,
    pub selector: String,
    pub bytes: Vec<u8>,
    pub hash: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Patch {
    pub edits: Vec<ByteEdit>,
    pub before_hash: String,
    pub after_hash: String,
    pub owned_fragment: Option<OwnedFragment>,
}

const MAX_INPUT: usize = 1_048_576;
const MAX_BODY: usize = 65_536;
const MAX_FRAGMENT: usize = MAX_BODY * 2 + 256;
const START: &str = "<!-- yoke:managed:generic-agents-v1:start -->";
const END: &str = "<!-- yoke:managed:generic-agents-v1:end -->";
const SELECTOR: &str = "generic-agents-v1";
const RESERVED: &[u8] = b"<!-- yoke:";

fn text<'a>(bytes: &'a [u8], maximum: usize, deadline: &Deadline) -> Result<&'a str> {
    deadline.check()?;
    ensure!(
        bytes.len() <= maximum,
        "Host configuration byte capacity exceeded"
    );
    ensure!(!bytes.contains(&0), "Host configuration contains NUL bytes");
    std::str::from_utf8(bytes).map_err(|_| anyhow::anyhow!("Host configuration must be UTF-8"))
}

fn occurrences(bytes: &[u8], needle: &[u8]) -> Vec<usize> {
    bytes
        .windows(needle.len())
        .enumerate()
        .filter_map(|(offset, part)| (part == needle).then_some(offset))
        .collect()
}

fn line_ending(bytes: &[u8]) -> &'static str {
    match bytes.iter().position(|byte| *byte == b'\n') {
        Some(offset) if offset > 0 && bytes[offset - 1] == b'\r' => "\r\n",
        _ => "\n",
    }
}

fn checked_owned<'a>(
    prior: &'a OwnedFragment,
    deadline: &Deadline,
) -> Result<(&'a [u8], &'static str)> {
    ensure!(
        prior.format == Format::AgentsMarkdown && prior.selector == SELECTOR,
        "Host configuration receipt selector differs"
    );
    let fragment = text(&prior.bytes, MAX_FRAGMENT, deadline)?;
    ensure!(
        hash(&prior.bytes) == prior.hash,
        "Host configuration owned fragment hash differs"
    );
    let prefix_len = if fragment.starts_with("\r\n") {
        2
    } else {
        usize::from(fragment.starts_with('\n'))
    };
    let block = &fragment[prefix_len..];
    ensure!(
        block.starts_with(START),
        "Host configuration owned block is invalid"
    );
    let eol = line_ending(block.as_bytes());
    ensure!(
        block.starts_with(&format!("{START}{eol}")) && block.ends_with(&format!("{END}{eol}")),
        "Host configuration owned block delimiters differ"
    );
    ensure!(
        occurrences(block.as_bytes(), RESERVED).len() == 2,
        "Host configuration owned block has ambiguous markers"
    );
    if prefix_len > 0 {
        ensure!(
            &fragment[..prefix_len] == eol,
            "Host configuration owned separator differs"
        );
    }
    let body_start = START.len() + eol.len();
    let body_end = block.len() - END.len() - eol.len();
    let body = &block[body_start..body_end];
    ensure!(
        body.is_empty() || body.ends_with(eol),
        "Host configuration owned body has no terminal line ending"
    );
    Ok((&prior.bytes[..prefix_len], eol))
}

fn render_agents(body: &[u8], prefix: &[u8], eol: &str, deadline: &Deadline) -> Result<Vec<u8>> {
    let body = text(body, MAX_BODY, deadline)?;
    ensure!(
        occurrences(body.as_bytes(), RESERVED).is_empty(),
        "Host configuration body contains reserved markers"
    );
    let normalized = body.replace("\r\n", "\n");
    ensure!(
        !normalized.contains('\r'),
        "Host configuration body contains unsupported line endings"
    );
    let body = normalized.replace('\n', eol);
    let mut out =
        Vec::with_capacity(prefix.len() + START.len() + END.len() + body.len() + 3 * eol.len());
    out.extend_from_slice(prefix);
    out.extend_from_slice(START.as_bytes());
    out.extend_from_slice(eol.as_bytes());
    out.extend_from_slice(body.as_bytes());
    if !body.is_empty() && !body.ends_with(eol) {
        out.extend_from_slice(eol.as_bytes());
    }
    out.extend_from_slice(END.as_bytes());
    out.extend_from_slice(eol.as_bytes());
    deadline.check()?;
    Ok(out)
}

/// Plan one generic AGENTS block. `prior` must originate in a verified installer receipt.
pub fn agents_patch(
    input: &[u8],
    body: Option<&[u8]>,
    prior: Option<&OwnedFragment>,
    deadline: &Deadline,
) -> Result<Patch> {
    text(input, MAX_INPUT, deadline)?;
    ensure!(
        body.is_some() || prior.is_some(),
        "Removing a host configuration block requires a receipt"
    );
    let (start, end, prefix, eol) = if let Some(prior) = prior {
        let (prefix, eol) = checked_owned(prior, deadline)?;
        let positions = occurrences(input, &prior.bytes);
        ensure!(
            positions.len() == 1 && occurrences(input, RESERVED).len() == 2,
            "Host configuration owned fragment changed or is ambiguous"
        );
        let start = positions[0];
        let marker_start = start + prefix.len();
        ensure!(
            marker_start == 0
                || (marker_start == 3 && input.starts_with(b"\xef\xbb\xbf"))
                || input[marker_start - 1] == b'\n',
            "Host configuration marker is not on its own line"
        );
        (start, start + prior.bytes.len(), prefix.to_vec(), eol)
    } else {
        ensure!(
            occurrences(input, RESERVED).is_empty(),
            "Host configuration contains unowned reserved markers"
        );
        let eol = line_ending(input);
        let content = input.strip_prefix(b"\xef\xbb\xbf").unwrap_or(input);
        let prefix = if content.is_empty() || content.ends_with(b"\n") {
            Vec::new()
        } else {
            eol.as_bytes().to_vec()
        };
        (input.len(), input.len(), prefix, eol)
    };
    let replacement = match body {
        Some(body) => render_agents(body, &prefix, eol, deadline)?,
        None => Vec::new(),
    };
    let owned_fragment = body.map(|_| OwnedFragment {
        format: Format::AgentsMarkdown,
        selector: SELECTOR.into(),
        hash: hash(&replacement),
        bytes: replacement.clone(),
    });
    let edits = vec![ByteEdit {
        start_byte: start,
        end_byte: end,
        expected: input[start..end].to_vec(),
        replacement,
    }];
    let output = splice(input, &edits, deadline)?;
    Ok(Patch {
        edits,
        before_hash: hash(input),
        after_hash: hash(&output),
        owned_fragment,
    })
}

fn splice(input: &[u8], edits: &[ByteEdit], deadline: &Deadline) -> Result<Vec<u8>> {
    let input_text = text(input, MAX_INPUT, deadline)?;
    ensure!(
        !edits.is_empty() && edits.len() <= 16,
        "Host configuration edit count exceeded"
    );
    let mut position = 0;
    let mut previous_start = None;
    let mut output_len = input.len();
    for edit in edits {
        deadline.check()?;
        ensure!(
            edit.start_byte >= position
                && edit.start_byte <= edit.end_byte
                && edit.end_byte <= input.len()
                && previous_start != Some(edit.start_byte),
            "Host configuration edit ranges overlap or are invalid"
        );
        ensure!(
            input_text.is_char_boundary(edit.start_byte)
                && input_text.is_char_boundary(edit.end_byte),
            "Host configuration edit splits UTF-8 source"
        );
        ensure!(
            input[edit.start_byte..edit.end_byte] == edit.expected,
            "Host configuration edit bytes differ"
        );
        text(&edit.expected, MAX_FRAGMENT, deadline)?;
        text(&edit.replacement, MAX_FRAGMENT, deadline)?;
        output_len = output_len
            .checked_sub(edit.end_byte - edit.start_byte)
            .and_then(|n| n.checked_add(edit.replacement.len()))
            .ok_or_else(|| anyhow::anyhow!("Host configuration output length overflow"))?;
        ensure!(
            output_len <= MAX_INPUT,
            "Host configuration output byte capacity exceeded"
        );
        position = edit.end_byte;
        previous_start = Some(edit.start_byte);
    }
    let mut out = Vec::with_capacity(output_len);
    position = 0;
    for edit in edits {
        deadline.check()?;
        out.extend_from_slice(&input[position..edit.start_byte]);
        out.extend_from_slice(&edit.replacement);
        position = edit.end_byte;
    }
    out.extend_from_slice(&input[position..]);
    text(&out, MAX_INPUT, deadline)?;
    Ok(out)
}

/// Apply reviewed byte edits in memory. This function grants no filesystem authority.
pub fn apply_patch(input: &[u8], patch: &Patch, deadline: &Deadline) -> Result<Vec<u8>> {
    text(input, MAX_INPUT, deadline)?;
    ensure!(
        hash(input) == patch.before_hash,
        "Host configuration input hash differs"
    );
    let output = splice(input, &patch.edits, deadline)?;
    ensure!(
        hash(&output) == patch.after_hash,
        "Host configuration output hash differs"
    );
    if let Some(owned) = &patch.owned_fragment {
        ensure!(
            !owned.bytes.is_empty(),
            "Host configuration owned fragment is empty"
        );
        ensure!(
            owned.format == Format::JsonMcpNamedEntryV1
                || occurrences(&output, &owned.bytes).len() == 1,
            "Host configuration output owned fragment differs"
        );
        match owned.format {
            Format::AgentsMarkdown => {
                checked_owned(owned, deadline)?;
                ensure!(
                    occurrences(&output, RESERVED).len() == 2,
                    "Host configuration output has ambiguous markers"
                );
            }
            Format::JsoncNamedEntry => {
                checked_jsonc_owned(&output, owned, deadline)?;
            }
            Format::JsonMcpNamedEntryV1 => mcp::checked_owned(&output, owned, deadline)?,
            Format::ClaudeHookSettingsV1 => claude_hooks::checked(&output, owned, deadline)?,
            Format::ClaudeSourceHookSettingsV1 => {
                claude_hooks::checked_source(&output, owned, deadline)?
            }
            Format::TomlTable => {
                checked_toml_owned(&output, owned, deadline)?;
            }
        }
    }
    deadline.check()?;
    Ok(output)
}

#[derive(Debug)]
struct JsonNode {
    start: usize,
    end: usize,
    object: Option<JsonObject>,
}
#[derive(Debug)]
struct JsonObject {
    members: Vec<JsonMember>,
    close: usize,
}
#[derive(Debug)]
struct JsonMember {
    key: String,
    key_start: usize,
    value: JsonNode,
    comma: Option<usize>,
}

struct JsoncParser<'a> {
    bytes: &'a [u8],
    pos: usize,
    nodes: usize,
    deadline: &'a Deadline,
}

impl JsoncParser<'_> {
    fn trivia(&mut self) -> Result<()> {
        loop {
            self.deadline.check()?;
            while self
                .bytes
                .get(self.pos)
                .is_some_and(|byte| matches!(byte, b' ' | b'\t' | b'\r' | b'\n'))
            {
                self.pos += 1;
            }
            if self.bytes.get(self.pos..self.pos + 2) == Some(b"//") {
                while self.bytes.get(self.pos).is_some_and(|b| *b != b'\n') {
                    self.pos += 1;
                }
            } else if self.bytes.get(self.pos..self.pos + 2) == Some(b"/*") {
                self.pos += 2;
                while self.bytes.get(self.pos..self.pos + 2) != Some(b"*/") {
                    ensure!(
                        self.pos < self.bytes.len(),
                        "Unterminated host JSONC comment"
                    );
                    self.pos += 1;
                }
                self.pos += 2;
            } else {
                return Ok(());
            }
        }
    }

    fn string(&mut self) -> Result<String> {
        let start = self.pos;
        ensure!(
            self.bytes.get(self.pos) == Some(&b'"'),
            "Host JSONC object key must be a string"
        );
        self.pos += 1;
        loop {
            match self.bytes.get(self.pos) {
                Some(b'"') => {
                    self.pos += 1;
                    break;
                }
                Some(b'\\') => {
                    self.pos += 2;
                }
                Some(_) => {
                    self.pos += 1;
                }
                None => anyhow::bail!("Unterminated host JSONC string"),
            }
        }
        serde_json::from_slice(&self.bytes[start..self.pos])
            .map_err(|_| anyhow::anyhow!("Invalid host JSONC string"))
    }

    fn value(&mut self, depth: usize) -> Result<JsonNode> {
        self.trivia()?;
        self.nodes += 1;
        ensure!(
            depth <= 64 && self.nodes <= 16_384,
            "Host JSONC structural capacity exceeded"
        );
        let start = self.pos;
        let mut object = None;
        match self.bytes.get(self.pos) {
            Some(b'{') => {
                self.pos += 1;
                let mut members = Vec::new();
                let mut keys = std::collections::BTreeSet::new();
                loop {
                    self.trivia()?;
                    if self.bytes.get(self.pos) == Some(&b'}') {
                        break;
                    }
                    let key_start = self.pos;
                    let key = self.string()?;
                    ensure!(keys.insert(key.clone()), "Duplicate decoded host JSONC key");
                    self.trivia()?;
                    ensure!(
                        self.bytes.get(self.pos) == Some(&b':'),
                        "Host JSONC member is missing a colon"
                    );
                    self.pos += 1;
                    let value = self.value(depth + 1)?;
                    self.trivia()?;
                    let comma = if self.bytes.get(self.pos) == Some(&b',') {
                        let at = self.pos;
                        self.pos += 1;
                        Some(at)
                    } else {
                        None
                    };
                    members.push(JsonMember {
                        key,
                        key_start,
                        value,
                        comma,
                    });
                    if comma.is_none() {
                        self.trivia()?;
                        ensure!(
                            self.bytes.get(self.pos) == Some(&b'}'),
                            "Host JSONC members require commas"
                        );
                        break;
                    }
                }
                object = Some(JsonObject {
                    members,
                    close: self.pos,
                });
                self.pos += 1;
            }
            Some(b'[') => {
                self.pos += 1;
                loop {
                    self.trivia()?;
                    if self.bytes.get(self.pos) == Some(&b']') {
                        self.pos += 1;
                        break;
                    }
                    self.value(depth + 1)?;
                    self.trivia()?;
                    if self.bytes.get(self.pos) == Some(&b',') {
                        self.pos += 1;
                    } else {
                        ensure!(
                            self.bytes.get(self.pos) == Some(&b']'),
                            "Host JSONC array requires commas"
                        );
                        self.pos += 1;
                        break;
                    }
                }
            }
            Some(b'"') => {
                self.string()?;
            }
            Some(_) => {
                while self
                    .bytes
                    .get(self.pos)
                    .is_some_and(|b| !b.is_ascii_whitespace() && !b",]}/".contains(b))
                {
                    self.pos += 1;
                }
                ensure!(self.pos > start, "Invalid host JSONC token");
                crate::json::parse(&self.bytes[start..self.pos], MAX_INPUT)?;
            }
            None => anyhow::bail!("Incomplete host JSONC value"),
        }
        Ok(JsonNode {
            start,
            end: self.pos,
            object,
        })
    }
}

fn jsonc_document(bytes: &[u8], deadline: &Deadline) -> Result<JsonNode> {
    text(bytes, MAX_INPUT, deadline)?;
    let mut parser = JsoncParser {
        bytes,
        pos: usize::from(bytes.starts_with(b"\xef\xbb\xbf")) * 3,
        nodes: 0,
        deadline,
    };
    let root = parser.value(0)?;
    parser.trivia()?;
    ensure!(
        parser.pos == bytes.len() && root.object.is_some(),
        "Host JSONC document must be one object"
    );
    deadline.check()?;
    Ok(root)
}

fn checked_path(path: &[String], allow_empty: bool) -> Result<()> {
    ensure!(
        (allow_empty || !path.is_empty()) && path.len() <= 16,
        "Host configuration selector path capacity exceeded"
    );
    ensure!(
        path.iter().all(|part| !part.is_empty()
            && part.len() <= 128
            && !part.chars().any(char::is_control)),
        "Host configuration selector component is invalid"
    );
    Ok(())
}

fn jsonc_parent<'a>(root: &'a JsonNode, path: &[String]) -> Result<&'a JsonObject> {
    let mut node = root;
    for part in path {
        let object = node
            .object
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Host JSONC selector traverses a non-object"))?;
        node = &object
            .members
            .iter()
            .find(|member| member.key == *part)
            .ok_or_else(|| anyhow::anyhow!("Host JSONC parent object is missing"))?
            .value;
    }
    node.object
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("Host JSONC parent is not an object"))
}

fn structured_receipt(
    prior: &OwnedFragment,
    format: Format,
    selector: &str,
    deadline: &Deadline,
) -> Result<()> {
    text(&prior.bytes, MAX_FRAGMENT, deadline)?;
    ensure!(
        !prior.bytes.is_empty()
            && prior.format == format
            && prior.selector == selector
            && hash(&prior.bytes) == prior.hash,
        "Host configuration owned receipt differs"
    );
    Ok(())
}

fn checked_jsonc_owned(
    input: &[u8],
    prior: &OwnedFragment,
    deadline: &Deadline,
) -> Result<(usize, usize)> {
    ensure!(
        prior.selector.len() <= 8_192,
        "Host JSONC receipt selector capacity exceeded"
    );
    let (parent, key): (Vec<String>, String) = serde_json::from_str(&prior.selector)
        .map_err(|_| anyhow::anyhow!("Invalid host JSONC receipt selector"))?;
    checked_path(&parent, true)?;
    checked_path(std::slice::from_ref(&key), false)?;
    let selector = serde_json::to_string(&(&parent, &key))?;
    structured_receipt(prior, Format::JsoncNamedEntry, &selector, deadline)?;
    let root = jsonc_document(input, deadline)?;
    let object = jsonc_parent(&root, &parent)?;
    let member = object
        .members
        .iter()
        .find(|member| member.key == key)
        .ok_or_else(|| anyhow::anyhow!("Host JSONC owned member is missing"))?;
    let positions = occurrences(input, &prior.bytes);
    ensure!(
        positions.len() == 1,
        "Host JSONC owned fragment changed or is ambiguous"
    );
    let start = positions[0];
    let end = start + prior.bytes.len();
    let leading = usize::from(prior.bytes.starts_with(b","));
    ensure!(
        member.key_start == start + leading
            && member.value.start > member.key_start
            && member.value.end <= end
            && end <= object.close,
        "Host JSONC owned fragment is outside its member"
    );
    let mut fragment = Vec::with_capacity(prior.bytes.len() + 2);
    fragment.push(b'{');
    fragment.extend_from_slice(&prior.bytes[leading..]);
    fragment.push(b'}');
    let parsed = jsonc_document(&fragment, deadline)?;
    ensure!(
        parsed.object.as_ref().is_some_and(|o| o.members.len() == 1
            && o.members[0].key == key
            && o.members[0].value.object.is_some()),
        "Host JSONC receipt does not own exactly one named object"
    );
    Ok((start, end))
}

fn single_patch(
    input: &[u8],
    range: std::ops::Range<usize>,
    replacement: Vec<u8>,
    format: Format,
    selector: String,
    keep: bool,
    deadline: &Deadline,
) -> Result<Patch> {
    let owned_fragment = keep.then(|| OwnedFragment {
        format,
        selector,
        hash: hash(&replacement),
        bytes: replacement.clone(),
    });
    let edits = vec![ByteEdit {
        start_byte: range.start,
        end_byte: range.end,
        expected: input[range].to_vec(),
        replacement,
    }];
    let output = splice(input, &edits, deadline)?;
    Ok(Patch {
        edits,
        before_hash: hash(input),
        after_hash: hash(&output),
        owned_fragment,
    })
}

/// Edit one named object under an existing JSON/JSONC object without formatting its neighbors.
pub fn jsonc_patch(
    input: &[u8],
    parent: &[String],
    key: &str,
    value: Option<&[u8]>,
    prior: Option<&OwnedFragment>,
    deadline: &Deadline,
) -> Result<Patch> {
    checked_path(parent, true)?;
    checked_path(&[key.to_owned()], false)?;
    let selector = serde_json::to_string(&(parent, key))?;
    ensure!(
        value.is_some() || prior.is_some(),
        "Removing a host JSONC entry requires a receipt"
    );
    if let Some(value) = value {
        text(value, MAX_BODY, deadline)?;
        ensure!(
            crate::json::parse(value, MAX_BODY)?.is_object(),
            "Host JSONC named entry value must be a unique-key JSON object"
        );
    }
    let root = jsonc_document(input, deadline)?;
    let object = jsonc_parent(&root, parent)?;
    let (start, end, leading_comma, trailing_comma) = if let Some(prior) = prior {
        structured_receipt(prior, Format::JsoncNamedEntry, &selector, deadline)?;
        let (start, end) = checked_jsonc_owned(input, prior, deadline)?;
        (
            start,
            end,
            prior.bytes.starts_with(b","),
            prior.bytes.last() == Some(&b','),
        )
    } else {
        ensure!(
            !object.members.iter().any(|member| member.key == key),
            "Host JSONC named entry is already unowned"
        );
        let trailing = object
            .members
            .last()
            .is_some_and(|member| member.comma.is_some());
        (
            object.close,
            object.close,
            !object.members.is_empty() && !trailing,
            trailing,
        )
    };
    let mut replacement = Vec::new();
    if let Some(value) = value {
        if leading_comma {
            replacement.push(b',');
        }
        replacement.extend_from_slice(serde_json::to_string(key)?.as_bytes());
        replacement.push(b':');
        replacement.extend_from_slice(value);
        if trailing_comma {
            replacement.push(b',');
        }
    }
    let patch = single_patch(
        input,
        start..end,
        replacement,
        Format::JsoncNamedEntry,
        selector,
        value.is_some(),
        deadline,
    )?;
    // A foreign punctuation change may make removal unsafe; never delete extra human bytes.
    let output = splice(input, &patch.edits, deadline)?;
    jsonc_document(&output, deadline)?;
    if let Some(owned) = &patch.owned_fragment {
        checked_jsonc_owned(&output, owned, deadline)?;
    }
    Ok(patch)
}

fn toml_document(input: &[u8], deadline: &Deadline) -> Result<toml::Value> {
    let input = text(input, MAX_INPUT, deadline)?;
    ensure!(
        input.lines().count() <= 16_384,
        "Host TOML line capacity exceeded"
    );
    let parsed = toml::from_str::<toml::Value>(input.trim_start_matches('\u{feff}'))
        .map_err(|_| anyhow::anyhow!("Invalid or duplicate-key host TOML"))?;
    deadline.check()?;
    ensure!(parsed.is_table(), "Host TOML must be a table document");
    Ok(parsed)
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum TomlString {
    None,
    Basic,
    Literal,
    MultiBasic,
    MultiLiteral,
}

/// Locate real table headers; brackets inside comments, arrays and multiline strings are not headers.
fn toml_headers(input: &[u8], deadline: &Deadline) -> Result<Vec<usize>> {
    let mut out = Vec::new();
    let mut state = TomlString::None;
    let mut pos = usize::from(input.starts_with(b"\xef\xbb\xbf")) * 3;
    let mut at_line_start = true;
    let mut containers = 0usize;
    while pos < input.len() {
        if pos.is_multiple_of(1024) {
            deadline.check()?;
        }
        let byte = input[pos];
        if state == TomlString::None {
            if byte == b'#' {
                while pos < input.len() && input[pos] != b'\n' {
                    pos += 1;
                }
                continue;
            }
            if byte == b'\n' {
                at_line_start = true;
                pos += 1;
                continue;
            }
            if at_line_start && matches!(byte, b' ' | b'\t' | b'\r') {
                pos += 1;
                continue;
            }
            if at_line_start && containers == 0 && byte == b'[' {
                out.push(pos);
                // A syntactically valid header cannot contain a multiline string or a newline.
                while pos < input.len() && input[pos] != b'\n' {
                    pos += 1;
                }
                continue;
            }
            at_line_start = false;
            if input.get(pos..pos + 3) == Some(b"\"\"\"") {
                state = TomlString::MultiBasic;
                pos += 3;
                continue;
            }
            if input.get(pos..pos + 3) == Some(b"'''") {
                state = TomlString::MultiLiteral;
                pos += 3;
                continue;
            }
            match byte {
                b'"' => state = TomlString::Basic,
                b'\'' => state = TomlString::Literal,
                b'[' | b'{' => containers += 1,
                b']' | b'}' => {
                    containers = containers.saturating_sub(1);
                }
                _ => {}
            }
        } else {
            let basic = matches!(state, TomlString::Basic | TomlString::MultiBasic);
            if basic && byte == b'\\' {
                pos += 2;
                continue;
            }
            let triple = match state {
                TomlString::MultiBasic => Some(b"\"\"\"".as_slice()),
                TomlString::MultiLiteral => Some(b"'''".as_slice()),
                _ => None,
            };
            if let Some(triple) = triple {
                if input.get(pos..pos + 3) == Some(triple) {
                    // TOML permits up to two quote characters immediately before the closing triple.
                    let quote = triple[0];
                    let mut run = 3;
                    while input.get(pos + run) == Some(&quote) {
                        run += 1;
                    }
                    state = TomlString::None;
                    pos += run;
                    continue;
                }
            } else if (state == TomlString::Basic && byte == b'"')
                || (state == TomlString::Literal && byte == b'\'')
            {
                state = TomlString::None;
            }
        }
        pos += 1;
    }
    deadline.check()?;
    Ok(out)
}

fn toml_flat_body(body: &[u8], maximum: usize, deadline: &Deadline) -> Result<()> {
    text(body, maximum, deadline)?;
    let parsed = toml_document(body, deadline)?;
    ensure!(
        toml_headers(body, deadline)?.is_empty()
            && parsed
                .as_table()
                .is_some_and(|table| table.values().all(|value| !value.is_table()
                    && !value
                        .as_array()
                        .is_some_and(|array| array.iter().any(toml::Value::is_table)))),
        "Host TOML managed body must contain flat keys without table headers"
    );
    Ok(())
}

fn toml_selector(path: &[String]) -> Result<String> {
    checked_path(path, false)?;
    ensure!(
        path.iter().all(|part| part
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))),
        "Host TOML managed path requires bare key components"
    );
    Ok(serde_json::to_string(path)?)
}

fn existing_toml_path<'a>(
    root: &'a toml::Value,
    path: &[String],
) -> Result<Option<&'a toml::Value>> {
    let mut node = root;
    for part in path {
        let table = node
            .as_table()
            .ok_or_else(|| anyhow::anyhow!("Host TOML selector traverses a non-table"))?;
        if let Some(next) = table.get(part) {
            node = next;
        } else {
            return Ok(None);
        }
    }
    Ok(Some(node))
}

fn checked_toml_owned(
    input: &[u8],
    prior: &OwnedFragment,
    deadline: &Deadline,
) -> Result<(usize, usize)> {
    ensure!(
        prior.selector.len() <= 8_192,
        "Host TOML receipt selector capacity exceeded"
    );
    let path: Vec<String> = serde_json::from_str(&prior.selector)
        .map_err(|_| anyhow::anyhow!("Invalid host TOML receipt selector"))?;
    let selector = toml_selector(&path)?;
    structured_receipt(prior, Format::TomlTable, &selector, deadline)?;
    let document = toml_document(input, deadline)?;
    ensure!(
        existing_toml_path(&document, &path)?.is_some_and(toml::Value::is_table),
        "Host TOML owned table is missing"
    );
    let positions = occurrences(input, &prior.bytes);
    ensure!(
        positions.len() == 1,
        "Host TOML owned fragment changed or is ambiguous"
    );
    let start = positions[0];
    let end = start + prior.bytes.len();
    let fragment = text(&prior.bytes, MAX_FRAGMENT, deadline)?;
    let prefix_len = if fragment.starts_with("\r\n") {
        2
    } else {
        usize::from(fragment.starts_with('\n'))
    };
    let eol = line_ending(prior.bytes.as_slice());
    let header = format!("[{}]{eol}", path.join("."));
    ensure!(
        fragment[prefix_len..].starts_with(&header) && fragment.ends_with(eol),
        "Host TOML owned table header differs"
    );
    if prefix_len > 0 {
        ensure!(
            &fragment[..prefix_len] == eol,
            "Host TOML owned separator differs"
        );
    }
    let body = &prior.bytes[prefix_len + header.len()..];
    toml_flat_body(body, MAX_FRAGMENT, deadline)?;
    let marker = start + prefix_len;
    ensure!(
        marker == 0
            || (marker == 3 && input.starts_with(b"\xef\xbb\xbf"))
            || input[marker - 1] == b'\n',
        "Host TOML owned header is not on its own line"
    );
    let headers = toml_headers(input, deadline)?;
    ensure!(
        headers.contains(&marker),
        "Host TOML owned header is inside another value"
    );
    let next_header = headers
        .into_iter()
        .find(|offset| *offset >= end)
        .unwrap_or(input.len());
    let trailing = text(&input[end..next_header], MAX_INPUT, deadline)?;
    ensure!(
        trailing.lines().all(|line| {
            let line = line.trim();
            line.is_empty() || line.starts_with('#')
        }),
        "Host TOML human keys were appended under the owned table"
    );
    Ok((start, end))
}

/// Append, replace or remove one flat managed TOML table; never reparent appended human keys.
pub fn toml_patch(
    input: &[u8],
    path: &[String],
    body: Option<&[u8]>,
    prior: Option<&OwnedFragment>,
    deadline: &Deadline,
) -> Result<Patch> {
    let selector = toml_selector(path)?;
    let document = toml_document(input, deadline)?;
    ensure!(
        body.is_some() || prior.is_some(),
        "Removing a host TOML table requires a receipt"
    );
    if let Some(body) = body {
        toml_flat_body(body, MAX_BODY, deadline)?;
    }
    let (start, end, prefix, eol) = if let Some(prior) = prior {
        structured_receipt(prior, Format::TomlTable, &selector, deadline)?;
        let (start, end) = checked_toml_owned(input, prior, deadline)?;
        let prefix_len = if prior.bytes.starts_with(b"\r\n") {
            2
        } else {
            usize::from(prior.bytes.starts_with(b"\n"))
        };
        (
            start,
            end,
            prior.bytes[..prefix_len].to_vec(),
            line_ending(&prior.bytes),
        )
    } else {
        ensure!(
            existing_toml_path(&document, path)?.is_none(),
            "Host TOML table is already unowned"
        );
        let eol = line_ending(input);
        let content = input.strip_prefix(b"\xef\xbb\xbf").unwrap_or(input);
        let prefix = if content.is_empty() || content.ends_with(b"\n") {
            Vec::new()
        } else {
            eol.as_bytes().to_vec()
        };
        (input.len(), input.len(), prefix, eol)
    };
    if body.is_none()
        && !prefix.is_empty()
        && start > 0
        && input[start - 1] != b'\n'
        && end < input.len()
    {
        ensure!(
            input[end..].starts_with(b"\n") || input[end..].starts_with(b"\r\n"),
            "Host TOML removal would join a human line with its appendix"
        );
    }
    let mut replacement = Vec::new();
    if let Some(body) = body {
        let normalized = text(body, MAX_BODY, deadline)?.replace("\r\n", "\n");
        ensure!(
            !normalized.contains('\r'),
            "Host TOML managed body contains unsupported line endings"
        );
        let body = normalized.replace('\n', eol);
        replacement.extend_from_slice(&prefix);
        replacement.extend_from_slice(format!("[{}]{eol}", path.join(".")).as_bytes());
        replacement.extend_from_slice(body.as_bytes());
        if !body.is_empty() && !body.ends_with(eol) {
            replacement.extend_from_slice(eol.as_bytes());
        }
    }
    let patch = single_patch(
        input,
        start..end,
        replacement,
        Format::TomlTable,
        selector,
        body.is_some(),
        deadline,
    )?;
    let output = splice(input, &patch.edits, deadline)?;
    toml_document(&output, deadline)?;
    if let Some(owned) = &patch.owned_fragment {
        checked_toml_owned(&output, owned, deadline)?;
    }
    Ok(patch)
}
