//! Delete one bound function-valued declarator and one AST-owned delimiter.
//! Delimiter trivia/comments are never guessed into declaration ownership.
use super::{Deletion, declaration_child};
use crate::capture::Deadline;
use anyhow::{Context, Result, ensure};
use tree_sitter::Node;

const INVALID: &str = "Sibling deletion requires a checked statement variable declarator";
pub(super) fn select(selected: Node<'_>, text: &str, d: &Deadline) -> Result<Deletion> {
    d.check()?;
    ensure!(
        selected.kind() == "variable_declarator"
            && selected
                .child_by_field_name("name")
                .is_some_and(|name| name.kind() == "identifier"),
        INVALID
    );
    let parent = selected.parent().context(INVALID)?;
    ensure!(
        matches!(
            parent.kind(),
            "lexical_declaration" | "variable_declaration"
        ) && parent
            .child(0)
            .is_some_and(|token| matches!(token.kind(), "const" | "let" | "var")),
        INVALID
    );
    let mut owner = parent;
    while let Some(wrapper) = owner.parent() {
        d.check()?;
        if !matches!(wrapper.kind(), "export_statement" | "ambient_declaration") {
            break;
        }
        ensure!(declaration_child(wrapper, owner), INVALID);
        owner = wrapper;
    }
    ensure!(
        owner
            .parent()
            .is_some_and(|container| matches!(container.kind(), "program" | "statement_block")),
        INVALID
    );
    let mut declarations = Vec::new();
    let mut comments = false;
    let mut cursor = parent.walk();
    for node in parent.named_children(&mut cursor) {
        d.check()?;
        if node.kind() == "comment" {
            comments = true;
        } else {
            ensure!(node.kind() == "variable_declarator", INVALID);
            declarations.push(node);
        }
    }
    let index = declarations
        .iter()
        .position(|node| *node == selected)
        .context(INVALID)?;
    if declarations.len() == 1 {
        return Ok(Deletion {
            start: owner.start_byte(),
            end: owner.end_byte(),
            replacement: "",
            separator: None,
        });
    }
    // A comment between declarators may carry an attachment to either side.
    // Refuse the finite operation instead of deleting or reattaching that data.
    ensure!(
        !comments,
        "Sibling declaration comment ownership is unsupported"
    );
    let mut commas = Vec::new();
    let mut cursor = parent.walk();
    for node in parent.children(&mut cursor) {
        d.check()?;
        if node.kind() == "," {
            ensure!(
                node.end_byte() == node.start_byte() + 1 && node.utf8_text(text.as_bytes())? == ",",
                INVALID
            );
            commas.push(node);
        }
    }
    ensure!(commas.len() + 1 == declarations.len(), INVALID);
    for (i, comma) in commas.iter().enumerate() {
        d.check()?;
        ensure!(
            declarations[i].end_byte() <= comma.start_byte()
                && comma.end_byte() <= declarations[i + 1].start_byte(),
            INVALID
        );
    }
    let comma = commas[if index + 1 < declarations.len() {
        index
    } else {
        index - 1
    }];
    Ok(Deletion {
        start: selected.start_byte(),
        end: selected.end_byte(),
        replacement: "",
        separator: Some((comma.start_byte(), comma.end_byte())),
    })
}
