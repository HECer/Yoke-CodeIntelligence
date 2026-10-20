//! Derive declaration deletion spans without changing stored entity coordinates.
use crate::{
    analysis::{self, AnalyzerEpoch},
    capture::Deadline,
    model::{Entity, SourceFile},
};
use anyhow::{Context, Result, ensure};
use tree_sitter::{Node, Parser};

const UNSUPPORTED: &str = "Symbol deletion requires an independently removable declaration";
const MAX_NODES: usize = 200_000;
#[path = "changes_sibling_delete.rs"]
mod siblings;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Policy {
    Strict,
    PythonPass,
    PreserveDeclarators,
}

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
    let selected = selection(epoch, file, entity, text, Policy::Strict, deadline)?;
    Ok((selected.start, selected.end))
}

pub(super) struct Deletion {
    pub start: usize,
    pub end: usize,
    pub replacement: &'static str,
    pub separator: Option<(usize, usize)>,
}
pub(super) fn python_with_empty_body(
    epoch: AnalyzerEpoch,
    file: &SourceFile,
    entity: &Entity,
    text: &str,
    deadline: &Deadline,
) -> Result<Deletion> {
    ensure!(
        file.language == "python",
        "Empty Python body policy requires Python source"
    );
    selection(epoch, file, entity, text, Policy::PythonPass, deadline)
}
pub(super) fn preserving_declarators(
    epoch: AnalyzerEpoch,
    file: &SourceFile,
    entity: &Entity,
    text: &str,
    deadline: &Deadline,
) -> Result<Deletion> {
    deadline.check()?;
    ensure!(
        matches!(
            file.language.as_str(),
            "typescript" | "tsx" | "javascript" | "jsx"
        ),
        "Sibling declarators policy requires TypeScript or JavaScript source"
    );
    selection(
        epoch,
        file,
        entity,
        text,
        Policy::PreserveDeclarators,
        deadline,
    )
}
fn selection(
    epoch: AnalyzerEpoch,
    file: &SourceFile,
    entity: &Entity,
    text: &str,
    policy: Policy,
    deadline: &Deadline,
) -> Result<Deletion> {
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
    if file.language == "rust" {
        let selected =
            crate::refactor_contract::symbol_deletion_range(epoch, entity, text, deadline)?;
        return Ok(Deletion {
            start: selected.start,
            end: selected.end,
            replacement: "",
            separator: None,
        });
    }
    // Python's recognized UTF-8 BOM is outside every declaration. Keep the
    // original source and rebase parser coordinates instead of rewriting it.
    let offset = usize::from(file.language == "python" && text.starts_with('\u{feff}')) * 3;
    let input = &text[offset..];
    let mut parser = Parser::new();
    parser.set_language(&analysis::grammar(&file.language).context(UNSUPPORTED)?)?;
    let mut cancelled = |_: &tree_sitter::ParseState| deadline.check().is_err();
    let tree = parser
        .parse_with_options(
            &mut |at, _| &input.as_bytes()[at.min(input.len())..],
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
        if node.start_byte() + offset == entity.start_byte
            && node.end_byte() + offset == entity.end_byte
            && let Some((kind, name)) =
                analysis::declaration_symbol(epoch, node, input, &file.language, deadline)?
            && kind == entity.kind
            && Some(name.start_byte() + offset) == entity.name_start_byte
            && Some(name.end_byte() + offset) == entity.name_end_byte
            && name.utf8_text(input.as_bytes())? == entity.name
        {
            ensure!(found.is_none(), "Ambiguous symbol deletion AST binding");
            found = Some(node);
        }
        let mut cursor = node.walk();
        stack.extend(node.named_children(&mut cursor));
    }
    let mut selected = found.context("Symbol deletion exact AST binding missing")?;
    if policy == Policy::PreserveDeclarators {
        let edit = siblings::select(selected, input, deadline)?;
        let mut ranges = vec![(edit.start, edit.end)];
        ranges.extend(edit.separator);
        ranges.sort_unstable_by_key(|range| std::cmp::Reverse(range.0));
        let mut after = input.to_owned();
        for (start, end) in ranges {
            deadline.check()?;
            crate::paths::utf8_range(input, start, end)?;
            after.replace_range(start..end, "");
        }
        let checked = parser
            .parse_with_options(
                &mut |at, _| &after.as_bytes()[at.min(after.len())..],
                None,
                Some(tree_sitter::ParseOptions::new().progress_callback(&mut cancelled)),
            )
            .context("Symbol deletion result parser cancelled")?;
        deadline.check()?;
        ensure!(
            !checked.root_node().has_error(),
            "Sibling declaration deletion result must have error-free syntax"
        );
        return Ok(edit);
    }
    let fill_empty_python_body = policy == Policy::PythonPass;
    if file.language == "python" {
        ensure!(
            matches!(selected.kind(), "function_definition" | "class_definition"),
            UNSUPPORTED
        );
        if let Some(parent) = selected.parent()
            && parent.kind() == "decorated_definition"
        {
            ensure!(
                parent.child_by_field_name("definition") == Some(selected),
                UNSUPPORTED
            );
            selected = parent;
        }
        let parent = selected.parent().context(UNSUPPORTED)?;
        ensure!(matches!(parent.kind(), "module" | "block"), UNSUPPORTED);
        let mut replacement = "";
        if parent.kind() == "block" {
            let mut cursor = parent.walk();
            let mut remaining = false;
            for child in parent.named_children(&mut cursor) {
                deadline.check()?;
                if child != selected && child.kind() != "comment" {
                    remaining = true;
                }
            }
            ensure!(
                remaining || fill_empty_python_body,
                "Python deletion would leave a required body empty"
            );
            if !remaining {
                replacement = "pass";
            }
        }
        if fill_empty_python_body {
            // Check the exact resulting source using the same bounded parser.
            // No analyzed module is imported or executed. BOM stays outside
            // parser coordinates and is untouched in the actual byte edit.
            let after = format!(
                "{}{}{}",
                &input[..selected.start_byte()],
                replacement,
                &input[selected.end_byte()..]
            );
            let checked = parser
                .parse_with_options(
                    &mut |at, _| &after.as_bytes()[at.min(after.len())..],
                    None,
                    Some(tree_sitter::ParseOptions::new().progress_callback(&mut cancelled)),
                )
                .context("Symbol deletion result parser cancelled")?;
            deadline.check()?;
            ensure!(
                !checked.root_node().has_error(),
                "Python deletion result must have error-free syntax"
            );
        }
        deadline.check()?;
        return Ok(Deletion {
            start: selected.start_byte() + offset,
            end: selected.end_byte() + offset,
            replacement,
            separator: None,
        });
    }
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
            previous.is_none_or(|node| node.kind() != "decorator"),
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
    Ok(Deletion {
        start: selected.start_byte(),
        end: selected.end_byte(),
        replacement: "",
        separator: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn sibling_deletion_obeys_source_ast_syntax_and_deadline_bounds() {
        let text = "const deleteMe = () => 1, kept = () => 2;\n";
        let file = SourceFile {
            path: "module.ts".into(),
            hash: crate::model::hash(text),
            size: text.len(),
            language: "typescript".into(),
            mode: 0o644,
        };
        let entity = analysis::extract(&file, text, &Deadline::new(30_000))
            .unwrap()
            .entities
            .into_iter()
            .find(|e| e.name == "deleteMe")
            .unwrap();
        assert!(
            preserving_declarators(AnalyzerEpoch::V9, &file, &entity, text, &Deadline::new(0))
                .is_err()
        );
        for input in [
            format!("{text}{}", " ".repeat(4_000_001)),
            format!("{text}\0"),
            format!("{text}function invalid( {{"),
            format!("{text}{}", "0;".repeat(MAX_NODES)),
        ] {
            assert!(
                preserving_declarators(
                    AnalyzerEpoch::V9,
                    &file,
                    &entity,
                    &input,
                    &Deadline::new(30_000)
                )
                .is_err()
            );
        }
        let exact = format!("{text}{}", " ".repeat(4_000_000 - text.len()));
        let selected = preserving_declarators(
            AnalyzerEpoch::V9,
            &file,
            &entity,
            &exact,
            &Deadline::new(30_000),
        )
        .unwrap();
        let (start, end) = selected.separator.unwrap();
        assert_eq!(&text[start..end], ",");
        assert!(selected.end <= start || end <= selected.start);
    }
    #[test]
    fn explicit_python_pass_checks_source_capacity_syntax_deadline_and_exact_anchor() {
        let text = "class Parent:\n    def delete_me(self): pass\n";
        let file = SourceFile {
            path: "module.py".into(),
            hash: crate::model::hash(text),
            size: text.len(),
            language: "python".into(),
            mode: 0o644,
        };
        let entity = analysis::extract(&file, text, &Deadline::new(30_000))
            .unwrap()
            .entities
            .into_iter()
            .find(|e| e.name == "delete_me")
            .unwrap();
        assert!(
            python_with_empty_body(AnalyzerEpoch::V9, &file, &entity, text, &Deadline::new(0))
                .is_err()
        );
        for input in [
            format!("{text}{}", " ".repeat(4_000_001)),
            format!("{text}\0"),
            format!("{text}def invalid(:\n"),
        ] {
            assert!(
                python_with_empty_body(
                    AnalyzerEpoch::V9,
                    &file,
                    &entity,
                    &input,
                    &Deadline::new(30_000)
                )
                .is_err()
            );
        }
        let mut forged = entity.clone();
        forged.start_byte += 1;
        assert!(
            python_with_empty_body(
                AnalyzerEpoch::V9,
                &file,
                &forged,
                text,
                &Deadline::new(30_000)
            )
            .is_err()
        );
        let exact = format!("{text}{}", " ".repeat(4_000_000 - text.len()));
        let selected = python_with_empty_body(
            AnalyzerEpoch::V9,
            &file,
            &entity,
            &exact,
            &Deadline::new(30_000),
        )
        .unwrap();
        assert_eq!(selected.replacement, "pass");
    }
    #[test]
    fn rust_and_python_deletion_reject_forged_anchors_and_expired_or_oversized_inputs() {
        for (language, text) in [
            ("rust", "// 🦀\n#[inline]\nfn delete_me() {}\n"),
            (
                "python",
                "# 🦀\n@decorator\ndef delete_me():\n    return 1\n",
            ),
        ] {
            let file = SourceFile {
                path: "owned".into(),
                hash: crate::model::hash(text),
                size: text.len(),
                language: language.into(),
                mode: 0o644,
            };
            let entity = analysis::extract(&file, text, &Deadline::new(30_000))
                .unwrap()
                .entities
                .into_iter()
                .find(|e| e.name == "delete_me")
                .unwrap();
            for variant in 0..6 {
                let mut forged = entity.clone();
                match variant {
                    0 => forged.start_byte = text.find('🦀').unwrap() + 1,
                    1 => forged.end_byte -= 1,
                    2 => forged.name = "retained".into(),
                    3 => forged.name_start_byte = None,
                    4 => forged.path = "../owned".into(),
                    _ => forged.kind = "method".into(),
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
            assert!(range(AnalyzerEpoch::V8, &file, &entity, text, &Deadline::new(0)).is_err());
            let oversized = format!("{text}{}", " ".repeat(4_000_001));
            assert!(
                range(
                    AnalyzerEpoch::V8,
                    &file,
                    &entity,
                    &oversized,
                    &Deadline::new(30_000)
                )
                .is_err()
            );
        }
    }

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
