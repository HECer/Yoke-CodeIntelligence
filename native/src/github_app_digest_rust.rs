//! Conservative file-local Rust declaration syntax; no crate visibility claim.
use super::{Options, SOURCE, source::Projection};
use crate::{
    capture::Deadline,
    model::{Entity, SourceFile, hash, identity},
};
use anyhow::{Context as _, Result, ensure};
use serde_json::json;
use std::collections::BTreeMap;
use tree_sitter::{Node, Tree};

fn public(node: Node<'_>, text: &str) -> bool {
    let mut cursor = node.walk();
    node.named_children(&mut cursor).any(|child| {
        child.kind() == "visibility_modifier"
            && child.utf8_text(text.as_bytes()).ok() == Some("pub")
    })
}
fn header(node: Node<'_>, text: &str, d: &Deadline) -> Result<Option<String>> {
    let end = match node.kind() {
        "const_item" | "static_item" => node.child_by_field_name("type").map(|ty| ty.end_byte()),
        "function_item" | "mod_item" => node
            .child_by_field_name("body")
            .map(|body| body.start_byte()),
        _ => None,
    };
    let Some(end) = end else { return Ok(None) };
    if node.has_error() || end.saturating_sub(node.start_byte()) > 16_000 {
        return Ok(None);
    }
    let mut comments = Vec::new();
    let mut cursor = node.walk();
    loop {
        d.check()?;
        let item = cursor.node();
        if item.start_byte() < end {
            if item.kind() == "const_parameter" && item.child_by_field_name("value").is_some() {
                return Ok(None);
            }
            if matches!(
                item.kind(),
                "string_literal" | "raw_string_literal" | "char_literal"
            ) {
                let mut parent = cursor.clone();
                let allowed_abi = parent.goto_parent()
                    && parent.node().kind() == "extern_modifier"
                    && matches!(
                        item.utf8_text(text.as_bytes()).ok(),
                        Some("\"C\"" | "\"system\"" | "\"Rust\"")
                    );
                if !allowed_abi {
                    return Ok(None);
                }
            }
            if matches!(
                item.kind(),
                "block"
                    | "closure_expression"
                    | "async_block"
                    | "const_block"
                    | "struct_expression"
                    | "macro_invocation"
                    | "call_expression"
                    | "binary_expression"
                    | "unary_expression"
                    | "index_expression"
                    | "field_expression"
            ) {
                return Ok(None);
            }
            if matches!(item.kind(), "line_comment" | "block_comment") {
                comments.push((item.start_byte(), item.end_byte().min(end)));
            } else if cursor.goto_first_child() {
                continue;
            }
        }
        loop {
            if cursor.goto_next_sibling() {
                break;
            }
            if !cursor.goto_parent() {
                let mut signature = String::new();
                let mut at = node.start_byte();
                for (start, finish) in comments {
                    signature.push_str(text.get(at..start).context(SOURCE)?);
                    signature.push(' ');
                    at = finish;
                }
                signature.push_str(text.get(at..end).context(SOURCE)?);
                return Ok(Some(signature.trim().to_owned()));
            }
        }
    }
}
#[allow(clippy::too_many_arguments)]
pub(super) fn project(
    tree: &Tree,
    text: &str,
    file: &SourceFile,
    snapshot: &str,
    spans: &BTreeMap<(usize, usize), Entity>,
    options: &Options,
    output: &mut Projection,
    d: &Deadline,
) -> Result<()> {
    let mut modules = vec![tree.root_node()];
    while let Some(module) = modules.pop() {
        let mut cursor = module.walk();
        for node in module.named_children(&mut cursor) {
            d.check()?;
            if matches!(
                node.kind(),
                "impl_item" | "macro_invocation" | "attribute_item" | "inner_attribute_item"
            ) {
                output.visibility_unresolved += 1;
                continue;
            }
            if !public(node, text) {
                continue;
            }
            let kind = match node.kind() {
                "function_item" => "function",
                "const_item" => "constant",
                "static_item" => "static",
                "mod_item" => "module",
                _ => {
                    output.visibility_unresolved += 1;
                    continue;
                }
            };
            if node.kind() == "mod_item" {
                if let Some(body) = node.child_by_field_name("body") {
                    modules.push(body);
                } else {
                    output.visibility_unresolved += 1;
                }
            }
            let Some(signature) = header(node, text, d)? else {
                output.visibility_unresolved += 1;
                continue;
            };
            let name = node
                .child_by_field_name("name")
                .context(SOURCE)?
                .utf8_text(text.as_bytes())
                .context(SOURCE)?;
            let span = (node.start_byte(), node.end_byte());
            let native = spans.get(&span);
            ensure!(
                kind == "static" || native.is_some_and(|e| e.name == name && e.kind == kind),
                SOURCE
            );
            output.candidates += 1;
            if output.symbols.len() >= options.max_symbols {
                continue;
            }
            let own_id = identity(&("yoke-rust-digest-1", snapshot, &file.path, span.0, span.1));
            output.symbols.push(json!({
                "id":native.map_or(own_id.as_str(), |e| e.id.as_str()),
                "graphEntityId":native.map(|e| e.id.as_str()),
                "producer":native.map_or("yoke-rust-digest-1", |e| e.producer.as_str()),
                "path":file.path,"name":name,"kind":kind,"signature":signature,
                "fingerprint":hash(text.get(span.0..span.1).context(SOURCE)?),
                "contentHash":file.hash,"startByte":span.0,"endByte":span.1,
                "visibility":"explicit_pub_in_file","externalVisibility":"unknown",
                "compilerVerified":false
            }));
        }
    }
    Ok(())
}
