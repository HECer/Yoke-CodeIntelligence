//! Native declaration replay without call resolution, contracts or rationale.
use super::{AnalyzerEpoch, entity, symbol};
use crate::{
    capture::Deadline,
    model::{Entity, SourceFile},
};
use anyhow::{Result, ensure};
use tree_sitter::{Node, Tree};

/// The released native signature policy is shared with full graph enrichment.
/// This is an internal fact, not the digest's stricter body-free header policy.
pub(crate) fn signature(node: Node<'_>, source: &str, start: usize) -> (String, bool) {
    let body = node.child_by_field_name("body").or_else(|| {
        node.child_by_field_name("value")
            .and_then(|value| value.child_by_field_name("body"))
    });
    let end = body.map_or(node.end_byte(), |body| body.start_byte());
    let header = source[start..end].trim_end();
    let mut cap = header.len().min(4096);
    while !header.is_char_boundary(cap) {
        cap -= 1;
    }
    (header[..cap].into(), header.len() > cap)
}

pub(crate) fn rust_at(
    epoch: AnalyzerEpoch,
    file: &SourceFile,
    source: &str,
    tree: &Tree,
    d: &Deadline,
) -> Result<Vec<Entity>> {
    ensure!(
        file.language == "rust",
        "Native Rust declaration replay requires Rust source"
    );
    let mut lines = vec![0];
    lines.extend(
        source
            .bytes()
            .enumerate()
            .filter_map(|(offset, byte)| (byte == b'\n').then_some(offset + 1)),
    );
    let mut output = vec![entity(
        epoch,
        file,
        &file.path,
        "file",
        0,
        source.len(),
        source,
        &lines,
        None,
        None,
    )];
    let mut cursor = tree.walk();
    let mut owners = vec![0usize];
    let mut nodes = 0usize;
    loop {
        d.check()?;
        nodes += 1;
        ensure!(
            nodes <= 1_000_000 && owners.len() <= 128,
            "Native declaration traversal capacity exceeded"
        );
        let node = cursor.node();
        let mut owner = *owners.last().expect("root owner remains present");
        if let Some((kind, anchor)) = symbol(epoch, node, source, "rust") {
            let name = anchor.utf8_text(source.as_bytes())?;
            if !name.is_empty() && name.len() <= 1000 {
                ensure!(
                    output.len() < 200_000,
                    "Native declaration capacity exceeded"
                );
                // impl declarations use the type field rather than name. The
                // symbol selector is authoritative for both kinds of anchors.
                let mut item = entity(
                    epoch,
                    file,
                    name,
                    kind,
                    node.start_byte(),
                    node.end_byte(),
                    source,
                    &lines,
                    Some(&output[owner]),
                    Some((anchor.start_byte(), anchor.end_byte())),
                );
                item.signature = signature(node, source, item.start_byte).0;
                if item.kind == "function"
                    && matches!(
                        output[owner].kind.as_str(),
                        "class" | "struct" | "interface" | "implementation"
                    )
                {
                    // Keep the constructor's original key when finalizing kind,
                    // exactly as the released full resolution pass does.
                    item.kind = "method".into();
                }
                owner = output.len();
                output.push(item);
                *owners.last_mut().expect("current owner remains present") = owner;
            }
        }
        if cursor.goto_first_child() {
            owners.push(owner);
            continue;
        }
        loop {
            if cursor.goto_next_sibling() {
                let parent = owners
                    .get(owners.len().saturating_sub(2))
                    .copied()
                    .unwrap_or(0);
                *owners.last_mut().expect("current owner remains present") = parent;
                break;
            }
            if !cursor.goto_parent() {
                return Ok(output);
            }
            owners.pop();
        }
    }
}
