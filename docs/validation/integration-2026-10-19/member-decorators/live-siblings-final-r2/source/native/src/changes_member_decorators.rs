//! Checked contiguous sibling decorator ownership for concrete class methods.
use crate::capture::Deadline;
use anyhow::{Context, Result, ensure};
use tree_sitter::Node;

pub(super) fn start(selected: Node<'_>, text: &str, d: &Deadline) -> Result<usize> {
    d.check()?;
    let mut previous = selected.prev_named_sibling();
    let mut next_start = selected.start_byte();
    let mut start = selected.start_byte();
    let mut decorators = 0usize;
    let mut gaps_valid = true;
    while let Some(node) = previous {
        d.check()?;
        if !matches!(node.kind(), "comment" | "decorator") {
            break;
        }
        let gap = text
            .get(node.end_byte()..next_start)
            .context("Member attachment source range invalid")?;
        gaps_valid &= gap.chars().all(char::is_whitespace);
        if node.kind() == "decorator" {
            ensure!(
                gaps_valid,
                "Member attachment gap contains unsupported tokens"
            );
            gaps_valid = true;
            decorators += 1;
            ensure!(decorators <= 64, "Member decorator capacity exceeded");
            ensure!(
                text.get(node.start_byte()..node.end_byte())
                    .is_some_and(|raw| raw.starts_with('@')),
                "Member decorator source binding invalid"
            );
            start = node.start_byte();
        }
        next_start = node.start_byte();
        previous = node.prev_named_sibling();
    }
    if decorators > 0 {
        ensure!(
            selected.kind() == "method_definition"
                && selected
                    .parent()
                    .is_some_and(|parent| parent.kind() == "class_body")
                && selected
                    .child_by_field_name("body")
                    .is_some_and(|body| body.kind() == "statement_block"),
            "Member decorators require a concrete class method"
        );
        ensure!(
            selected
                .child_by_field_name("name")
                .is_some_and(
                    |name| matches!(name.kind(), "property_identifier" | "identifier")
                        && name
                            .utf8_text(text.as_bytes())
                            .is_ok_and(|name| name != "constructor")
                ),
            "Decorated constructor or non-simple member name is unsupported"
        );
    }
    Ok(start)
}
