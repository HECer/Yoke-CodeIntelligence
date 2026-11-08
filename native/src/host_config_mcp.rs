//! Strict JSON MCP ownership, including created document/container boundaries.
//! Only entry bytes and necessary comma tokens are edited; foreign values remain.
use super::*;
use anyhow::Context;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Ownership {
    schema_version: u8,
    key: String,
    parent_created: bool,
    document_created: bool,
}
fn key(value: &str) -> Result<()> {
    ensure!(
        !value.is_empty()
            && value.len() <= 128
            && value
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-')),
        "Invalid managed MCP server name"
    );
    Ok(())
}
fn document(input: &[u8], d: &Deadline) -> Result<JsonNode> {
    text(input, MAX_INPUT, d)?;
    let without_bom = input.strip_prefix(b"\xef\xbb\xbf").unwrap_or(input);
    ensure!(
        crate::json::parse(without_bom, MAX_INPUT)?.is_object(),
        "Managed MCP configuration must be strict unique-key JSON object"
    );
    jsonc_document(input, d)
}
fn member_bytes(name: &str, value: &[u8], d: &Deadline) -> Result<Vec<u8>> {
    text(value, MAX_BODY, d)?;
    ensure!(
        value.first() == Some(&b'{')
            && value.last() == Some(&b'}')
            && crate::json::parse(value, MAX_BODY)?.is_object(),
        "Managed MCP entry must be a tight unique-key JSON object"
    );
    let mut bytes = serde_json::to_vec(name)?;
    bytes.push(b':');
    bytes.extend_from_slice(value);
    Ok(bytes)
}
fn ownership(prior: &OwnedFragment, d: &Deadline) -> Result<Ownership> {
    text(&prior.bytes, MAX_FRAGMENT, d)?;
    ensure!(
        prior.format == Format::JsonMcpNamedEntryV1 && hash(&prior.bytes) == prior.hash,
        "Managed MCP ownership hash or format differs"
    );
    let value = crate::json::parse(prior.selector.as_bytes(), 1024)?;
    let meta: Ownership = crate::json::decode(value)?;
    ensure!(
        meta.schema_version == 1 && (!meta.document_created || meta.parent_created),
        "Managed MCP ownership schema invalid"
    );
    key(&meta.key)?;
    ensure!(
        serde_json::to_string(&meta)? == prior.selector,
        "Managed MCP ownership selector is not canonical"
    );
    Ok(meta)
}
fn object(node: &JsonNode) -> Result<&JsonObject> {
    node.object
        .as_ref()
        .context("Managed MCP parent must be an object")
}
fn mcp_member(root: &JsonNode) -> Result<Option<&JsonMember>> {
    Ok(object(root)?.members.iter().find(|m| m.key == "mcpServers"))
}
pub(super) fn checked_owned(input: &[u8], prior: &OwnedFragment, d: &Deadline) -> Result<()> {
    let meta = ownership(prior, d)?;
    let root = document(input, d)?;
    let parent = mcp_member(&root)?.context("Managed MCP parent disappeared")?;
    let member = object(&parent.value)?
        .members
        .iter()
        .find(|m| m.key == meta.key)
        .context("Managed MCP entry disappeared")?;
    ensure!(
        input.get(member.key_start..member.value.end) == Some(prior.bytes.as_slice()),
        "Managed MCP entry differs from receipt-owned bytes"
    );
    Ok(())
}
fn edit(input: &[u8], range: std::ops::Range<usize>, replacement: Vec<u8>) -> ByteEdit {
    ByteEdit {
        start_byte: range.start,
        end_byte: range.end,
        expected: input[range].to_vec(),
        replacement,
    }
}
fn remove(input: &[u8], object: &JsonObject, index: usize) -> Result<Vec<ByteEdit>> {
    let member = &object.members[index];
    let mut edits = vec![edit(input, member.key_start..member.value.end, vec![])];
    let comma = member
        .comma
        .or_else(|| index.checked_sub(1).and_then(|i| object.members[i].comma));
    if let Some(at) = comma {
        edits.push(edit(input, at..at + 1, vec![]));
    }
    edits.sort_by_key(|e| e.start_byte);
    Ok(edits)
}
fn finish(
    input: &[u8],
    edits: Vec<ByteEdit>,
    owned: Option<(Ownership, Vec<u8>)>,
    d: &Deadline,
) -> Result<Patch> {
    let after = splice(input, &edits, d)?;
    if !after.is_empty() {
        document(&after, d)?;
    }
    let owned_fragment = owned
        .map(|(meta, bytes)| -> Result<OwnedFragment> {
            Ok(OwnedFragment {
                format: Format::JsonMcpNamedEntryV1,
                selector: serde_json::to_string(&meta)?,
                hash: hash(&bytes),
                bytes,
            })
        })
        .transpose()?;
    if let Some(owned) = &owned_fragment {
        checked_owned(&after, owned, d)?;
    }
    d.check()?;
    Ok(Patch {
        edits,
        before_hash: hash(input),
        after_hash: hash(&after),
        owned_fragment,
    })
}
/// Pure strict-JSON MCP patch. A serialized receipt or patch is never write authority.
pub fn json_mcp_patch(
    input: &[u8],
    name: &str,
    value: Option<&[u8]>,
    prior: Option<&OwnedFragment>,
    d: &Deadline,
) -> Result<Patch> {
    d.check()?;
    key(name)?;
    ensure!(
        value.is_some() || prior.is_some(),
        "Managed MCP removal requires ownership"
    );
    let authored = value.map(|v| member_bytes(name, v, d)).transpose()?;
    if input.is_empty() {
        ensure!(prior.is_none(), "Managed MCP document disappeared");
        let bytes = authored.context("Managed MCP installation body required")?;
        let mut output = b"{\"mcpServers\":{".to_vec();
        output.extend_from_slice(&bytes);
        output.extend_from_slice(b"}}\n");
        return finish(
            input,
            vec![edit(input, 0..0, output)],
            Some((
                Ownership {
                    schema_version: 1,
                    key: name.into(),
                    parent_created: true,
                    document_created: true,
                },
                bytes,
            )),
            d,
        );
    }
    let root = document(input, d)?;
    let root_object = object(&root)?;
    let parent = mcp_member(&root)?;
    if let Some(prior) = prior {
        let meta = ownership(prior, d)?;
        ensure!(meta.key == name, "Managed MCP ownership name differs");
        checked_owned(input, prior, d)?;
        let parent = parent.context("Managed MCP parent disappeared")?;
        let members = object(&parent.value)?;
        let index = members
            .members
            .iter()
            .position(|m| m.key == name)
            .context("Managed MCP entry disappeared")?;
        let member = &members.members[index];
        if let Some(bytes) = authored {
            return finish(
                input,
                vec![edit(
                    input,
                    member.key_start..member.value.end,
                    bytes.clone(),
                )],
                Some((meta, bytes)),
                d,
            );
        }
        // A created container is cleaned only while its exact owned shell remains.
        // Human formatting outside the entry is preserved when that shell changes.
        let owned_parent_shell = meta.parent_created
            && members.members.len() == 1
            && input.get(parent.key_start..member.key_start) == Some(b"\"mcpServers\":{")
            && input.get(member.value.end..parent.value.end) == Some(b"}");
        if owned_parent_shell {
            let parent_index = root_object
                .members
                .iter()
                .position(|m| m.key == "mcpServers")
                .unwrap();
            if meta.document_created
                && root_object.members.len() == 1
                && input.get(..parent.key_start) == Some(b"{")
                && input.get(parent.value.end..) == Some(b"}\n")
            {
                return finish(input, vec![edit(input, 0..input.len(), vec![])], None, d);
            }
            return finish(input, remove(input, root_object, parent_index)?, None, d);
        }
        return finish(input, remove(input, members, index)?, None, d);
    }
    let bytes = authored.context("Managed MCP installation body required")?;
    let mut replacement = Vec::new();
    let (at, parent_created) = if let Some(parent) = parent {
        let members = object(&parent.value)?;
        ensure!(
            !members.members.iter().any(|m| m.key == name),
            "Managed MCP entry is already unowned"
        );
        if !members.members.is_empty() {
            replacement.push(b',');
        }
        replacement.extend_from_slice(&bytes);
        (members.close, false)
    } else {
        if !root_object.members.is_empty() {
            replacement.push(b',');
        }
        replacement.extend_from_slice(b"\"mcpServers\":{");
        replacement.extend_from_slice(&bytes);
        replacement.push(b'}');
        (root_object.close, true)
    };
    finish(
        input,
        vec![edit(input, at..at, replacement)],
        Some((
            Ownership {
                schema_version: 1,
                key: name.into(),
                parent_created,
                document_created: false,
            },
            bytes,
        )),
        d,
    )
}
