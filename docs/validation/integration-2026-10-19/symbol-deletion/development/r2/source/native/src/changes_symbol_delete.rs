//! Derive declaration deletion spans without changing stored entity coordinates.
use crate::{
    analysis::{self, AnalyzerEpoch},
    capture::Deadline,
    model::{Entity, SourceFile},
};
use anyhow::{Context, Result, ensure};
use tree_sitter::{Node, Parser};

const UNSUPPORTED: &str =
    "Symbol deletion requires an independently removable TypeScript/JavaScript declaration";
const MAX_NODES: usize = 200_000;

fn declaration_child(parent: Node<'_>, child: Node<'_>) -> bool {
    if parent.kind() == "export_statement" {
        return parent.child_by_field_name("declaration") == Some(child)
            || parent.child_by_field_name("value") == Some(child);
    }
    let mut cursor = parent.walk();
    let mut declarations = parent
        .named_children(&mut cursor)
        .filter(|node| node.kind() != "comment");
    declarations.next() == Some(child) && declarations.next().is_none()
}

pub(super) fn range(
    epoch: AnalyzerEpoch,
    file: &SourceFile,
    entity: &Entity,
    text: &str,
    deadline: &Deadline,
) -> Result<(usize, usize)> {
    deadline.check()?;
    ensure!(
        text.len() <= 4_000_000 && !text.contains('\0'),
        "Symbol deletion source capacity exceeded"
    );
    crate::paths::utf8_range(text, entity.start_byte, entity.end_byte)?;
    ensure!(
        entity.path == file.path && entity.language == file.language,
        "Symbol deletion source binding mismatch"
    );
    let mut parser = Parser::new();
    parser.set_language(&analysis::grammar(&file.language).context(UNSUPPORTED)?)?;
    let mut cancelled = |_: &tree_sitter::ParseState| deadline.check().is_err();
    let tree = parser
        .parse_with_options(
            &mut |offset, _| &text.as_bytes()[offset.min(text.len())..],
            None,
            Some(tree_sitter::ParseOptions::new().progress_callback(&mut cancelled)),
        )
        .context("Symbol deletion parser cancelled")?;
    deadline.check()?;
    ensure!(
        !tree.root_node().has_error(),
        "Symbol deletion requires error-free source syntax"
    );
    let mut found = None;
    let mut stack = vec![tree.root_node()];
    let mut count = 0usize;
    while let Some(node) = stack.pop() {
        deadline.check()?;
        count += 1;
        ensure!(count <= MAX_NODES, "Symbol deletion AST capacity exceeded");
        if node.start_byte() == entity.start_byte
            && node.end_byte() == entity.end_byte
            && let Some((kind, name)) = analysis::symbol(epoch, node, text, &file.language)
            && kind == entity.kind
            && Some(name.start_byte()) == entity.name_start_byte
            && Some(name.end_byte()) == entity.name_end_byte
            && name.utf8_text(text.as_bytes())? == entity.name
        {
            ensure!(found.is_none(), "Ambiguous symbol deletion AST binding");
            found = Some(node);
        }
        let mut cursor = node.walk();
        stack.extend(node.named_children(&mut cursor));
    }
    let mut selected = found.context("Symbol deletion exact AST binding missing")?;
    let member = matches!(
        selected.kind(),
        "method_definition" | "method_signature" | "abstract_method_signature"
    );
    if selected.kind() == "variable_declarator" {
        let parent = selected.parent().context(UNSUPPORTED)?;
        ensure!(
            matches!(
                parent.kind(),
                "lexical_declaration" | "variable_declaration"
            ) && declaration_child(parent, selected),
            UNSUPPORTED
        );
        selected = parent;
    } else if matches!(selected.kind(), "function_expression" | "class") {
        // A named default expression can be removed only with its owning export.
        let parent = selected.parent().context(UNSUPPORTED)?;
        ensure!(
            parent.kind() == "export_statement" && declaration_child(parent, selected),
            UNSUPPORTED
        );
    } else {
        ensure!(
            member
                || matches!(
                    selected.kind(),
                    "function_declaration"
                        | "generator_function_declaration"
                        | "class_declaration"
                        | "interface_declaration"
                        | "enum_declaration"
                        | "type_alias_declaration"
                ),
            UNSUPPORTED
        );
    }
    // These wrappers own one declaration. Never ascend through a block, class,
    // expression, sibling declaration or consumer to make a selected span fit.
    while let Some(parent) = selected.parent() {
        if !matches!(parent.kind(), "export_statement" | "ambient_declaration") {
            break;
        }
        ensure!(declaration_child(parent, selected), UNSUPPORTED);
        selected = parent;
    }
    let parent = selected.parent().context(UNSUPPORTED)?;
    if member {
        let mut previous = selected.prev_named_sibling();
        while previous.is_some_and(|node| node.kind() == "comment") {
            previous = previous.and_then(|node| node.prev_named_sibling());
        }
        ensure!(
            !previous.is_some_and(|node| node.kind() == "decorator"),
            "Decorated member deletion requires a checked decorator ownership range"
        );
    }
    ensure!(
        if member {
            matches!(
                parent.kind(),
                "class_body" | "interface_body" | "object_type"
            )
        } else {
            matches!(parent.kind(), "program" | "statement_block")
        },
        UNSUPPORTED
    );
    deadline.check()?;
    Ok((selected.start_byte(), selected.end_byte()))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture(text: &str) -> (SourceFile, Entity) {
        let file = SourceFile {
            path: "module.ts".into(),
            hash: crate::model::hash(text),
            size: text.len(),
            language: "typescript".into(),
            mode: 0o644,
        };
        let extraction = analysis::extract(&file, text, &Deadline::new(30_000)).unwrap();
        let entity = extraction
            .entities
            .into_iter()
            .find(|e| e.name == "deleteMe")
            .unwrap();
        (file, entity)
    }
    #[test]
    fn deletion_range_rejects_forged_coordinates_names_kinds_and_source_binding() {
        let text = "export function deleteMe() {}\n";
        let (file, entity) = fixture(text);
        for variant in 0..7 {
            let mut forged = entity.clone();
            match variant {
                0 => forged.start_byte -= 1,
                1 => forged.end_byte -= 1,
                2 => forged.name = "other".into(),
                3 => forged.name_start_byte = None,
                4 => forged.kind = "class".into(),
                5 => forged.path = "../module.ts".into(),
                _ => forged.language = "javascript".into(),
            }
            assert!(
                range(
                    AnalyzerEpoch::V8,
                    &file,
                    &forged,
                    text,
                    &Deadline::new(30_000)
                )
                .is_err()
            );
        }
    }
    #[test]
    fn deletion_range_checks_source_size_nul_syntax_and_deadline() {
        let text = "export function deleteMe() {}\n";
        let (file, entity) = fixture(text);
        for suffix in ["\0".to_string(), "/*".to_string(), " ".repeat(4_000_001)] {
            let expanded = format!("{text}{suffix}");
            assert!(
                range(
                    AnalyzerEpoch::V8,
                    &file,
                    &entity,
                    &expanded,
                    &Deadline::new(30_000)
                )
                .is_err()
            );
        }
        assert!(range(AnalyzerEpoch::V8, &file, &entity, text, &Deadline::new(0)).is_err());
    }
    #[test]
    fn deletion_range_accepts_exact_source_capacity_and_rejects_ast_overflow() {
        let text = "export function deleteMe() {}\n";
        let (file, entity) = fixture(text);
        let mut expanded = text.to_string();
        expanded.push_str(&" ".repeat(4_000_000 - text.len()));
        assert!(
            range(
                AnalyzerEpoch::V8,
                &file,
                &entity,
                &expanded,
                &Deadline::new(30_000)
            )
            .is_ok()
        );
        let expanded = format!("{text}{}", "0;".repeat(MAX_NODES));
        let error = range(
            AnalyzerEpoch::V8,
            &file,
            &entity,
            &expanded,
            &Deadline::new(30_000),
        )
        .unwrap_err();
        assert!(error.to_string().contains("AST capacity"), "{error:#}");
    }
}
