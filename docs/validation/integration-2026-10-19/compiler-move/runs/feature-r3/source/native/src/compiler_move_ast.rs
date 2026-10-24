//! Structural correspondence retains code and documentation, apart from imports,
//! export wrappers and explicitly compiler-bound identifier/namespace repairs.
use super::{Binding, Declaration, Key, Span, Target};
use crate::{analysis, capture::Deadline};
use anyhow::{Context, Result, ensure};
use std::collections::BTreeMap;
use tree_sitter::{Node, Tree};

pub(super) fn parse(language: &str, text: &str, d: &Deadline) -> Result<Tree> {
    ensure!(
        text.len() <= 4_000_000 && !text.contains('\0'),
        "Move syntax input capacity/NUL refusal"
    );
    let mut parser = tree_sitter::Parser::new();
    parser.set_language(&analysis::grammar(language).context("Move grammar unavailable")?)?;
    let mut cancelled = |_: &tree_sitter::ParseState| d.check().is_err();
    let tree = parser
        .parse_with_options(
            &mut |at, _| &text.as_bytes()[at.min(text.len())..],
            None,
            Some(tree_sitter::ParseOptions::new().progress_callback(&mut cancelled)),
        )
        .context("Move syntax parsing cancelled")?;
    d.check()?;
    ensure!(
        !tree.root_node().has_error(),
        "Move requires error-free source/result syntax"
    );
    Ok(tree)
}
#[derive(Clone)]
pub(super) struct Selection<'a> {
    pub node: Node<'a>,
    pub statement: Node<'a>,
    pub docs: Vec<Node<'a>>,
}
fn documentation<'a>(statement: Node<'a>, text: &str, d: &Deadline) -> Result<Vec<Node<'a>>> {
    let mut docs = Vec::new();
    let mut current = statement;
    while let Some(previous) = current.prev_named_sibling() {
        d.check()?;
        if previous.kind() != "comment" || !previous.utf8_text(text.as_bytes())?.starts_with("/**")
        {
            break;
        }
        ensure!(
            crate::paths::utf8_range(text, previous.end_byte(), current.start_byte())?
                .chars()
                .all(char::is_whitespace),
            "Move documentation gap is not whitespace"
        );
        docs.push(previous);
        ensure!(docs.len() <= 64, "Move documentation capacity exceeded");
        current = previous;
    }
    docs.reverse();
    Ok(docs)
}
fn unwrap(statement: Node<'_>) -> Node<'_> {
    if statement.kind() == "export_statement" {
        statement
            .child_by_field_name("declaration")
            .or_else(|| statement.child_by_field_name("value"))
            .unwrap_or(statement)
    } else {
        statement
    }
}
fn statement_for<'a>(node: Node<'a>, text: &str, d: &Deadline) -> Result<Selection<'a>> {
    let mut statement = node;
    if node.kind() == "variable_declarator" {
        statement = node.parent().context("Move binding statement absent")?;
        ensure!(
            matches!(
                statement.kind(),
                "lexical_declaration" | "variable_declaration"
            ),
            "Move binding must be a statement declaration"
        );
        let mut cursor = statement.walk();
        ensure!(
            statement
                .named_children(&mut cursor)
                .filter(|n| n.kind() == "variable_declarator")
                .count()
                == 1,
            "Move refuses unrelated sibling declarators"
        );
    }
    if let Some(parent) = statement
        .parent()
        .filter(|parent| parent.kind() == "export_statement")
    {
        ensure!(
            unwrap(parent) == statement,
            "Move export ownership mismatch"
        );
        statement = parent;
    }
    ensure!(
        statement
            .parent()
            .is_some_and(|parent| parent.kind() == "program"),
        "Move requires a top-level declaration"
    );
    Ok(Selection {
        node,
        statement,
        docs: documentation(statement, text, d)?,
    })
}
pub(super) fn selection<'a>(
    tree: &'a Tree,
    entity: &Declaration,
    text: &str,
    epoch: analysis::AnalyzerEpoch,
    d: &Deadline,
) -> Result<Selection<'a>> {
    let mut stack = vec![tree.root_node()];
    let mut count = 0;
    while let Some(node) = stack.pop() {
        d.check()?;
        count += 1;
        ensure!(count <= 200_000, "Move AST capacity exceeded");
        if node.start_byte() == entity.start_byte && node.end_byte() == entity.end_byte {
            if let Some((kind, name)) =
                analysis::declaration_symbol(epoch, node, text, &entity.language, d)?
            {
                ensure!(
                    entity
                        .graph_entity
                        .as_ref()
                        .is_some_and(|entity| kind == entity.kind)
                        && Some(name.start_byte()) == entity.name_start_byte
                        && Some(name.end_byte()) == entity.name_end_byte
                        && name.utf8_text(text.as_bytes())? == entity.name,
                    "Move declaration/name/kind differs from captured entity"
                );
                return statement_for(node, text, d);
            } else if entity.graph_entity.is_none() && node.kind() == "variable_declarator" {
                let name = node
                    .child_by_field_name("name")
                    .context("Move variable name absent")?;
                ensure!(
                    name.kind() == "identifier"
                        && Some(name.start_byte()) == entity.name_start_byte
                        && Some(name.end_byte()) == entity.name_end_byte
                        && name.utf8_text(text.as_bytes())? == entity.name,
                    "Move independent variable anchor mismatch"
                );
                return statement_for(node, text, d);
            }
        }
        if node.start_byte() <= entity.start_byte && node.end_byte() >= entity.end_byte {
            ensure!(
                stack.len().saturating_add(node.named_child_count()) <= 200_000,
                "Move AST frontier capacity exceeded"
            );
            let mut cursor = node.walk();
            stack.extend(node.named_children(&mut cursor));
        }
    }
    anyhow::bail!("Move captured declaration AST anchor absent")
}
pub(super) fn named_statement<'a>(
    tree: &'a Tree,
    text: &str,
    name_text: &str,
    d: &Deadline,
) -> Result<Option<Selection<'a>>> {
    let mut result = None;
    let mut cursor = tree.root_node().walk();
    ensure!(
        tree.root_node().named_child_count() <= 200_000,
        "Move root declaration capacity exceeded"
    );
    for statement in tree.root_node().named_children(&mut cursor) {
        d.check()?;
        let underlying = unwrap(statement);
        let nodes = if matches!(
            underlying.kind(),
            "lexical_declaration" | "variable_declaration"
        ) {
            let mut cursor = underlying.walk();
            underlying
                .named_children(&mut cursor)
                .filter(|n| n.kind() == "variable_declarator")
                .collect::<Vec<_>>()
        } else {
            vec![underlying]
        };
        for node in nodes {
            if let Some(name) = node.child_by_field_name("name")
                && name.utf8_text(text.as_bytes())? == name_text
            {
                ensure!(
                    result.is_none(),
                    "Move destination declaration is ambiguous"
                );
                result = Some(statement_for(node, text, d)?);
            }
        }
    }
    Ok(result)
}
pub(super) fn directives(tree: &Tree, text: &str, d: &Deadline) -> Result<Vec<String>> {
    let mut result = Vec::new();
    let mut cursor = tree.root_node().walk();
    for node in tree.root_node().named_children(&mut cursor) {
        d.check()?;
        if matches!(node.kind(), "comment" | "hash_bang_line") {
            continue;
        }
        if node.kind() == "expression_statement"
            && let Some(value) = node.named_child(0).filter(|n| n.kind() == "string")
        {
            result.push(value.utf8_text(text.as_bytes())?.to_owned());
            continue;
        }
        break;
    }
    Ok(result)
}
pub(super) fn is_identifier(kind: &str) -> bool {
    matches!(
        kind,
        "identifier"
            | "type_identifier"
            | "property_identifier"
            | "shorthand_property_identifier"
            | "shorthand_property_identifier_pattern"
            | "namespace_identifier"
            | "statement_identifier"
    )
}
#[derive(Clone, Debug)]
pub(super) struct Event {
    pub signature: String,
    pub range: (usize, usize),
    pub aliases: Vec<(usize, usize)>,
    pub identifier: bool,
    pub binding: Option<(usize, usize)>,
    pub text: String,
}
#[derive(Default)]
pub(super) struct Projection {
    pub events: Vec<Event>,
    pub linkage: Vec<(usize, usize)>,
    pub side_effect_imports: Vec<String>,
    pub module_links: Vec<(usize, usize, String)>,
    pub declaration_empty: bool,
}
fn selected_binding(binding: &Binding, selected: &Span) -> bool {
    binding.resolution == "resolved"
        && binding.targets
            == [Target::Captured {
                span: selected.clone(),
            }]
}
fn binding_at<'a>(
    bindings: &'a BTreeMap<Key, &Binding>,
    path: &str,
    node: Node<'_>,
) -> Option<&'a Binding> {
    bindings
        .get(&(path.into(), node.start_byte(), node.end_byte()))
        .copied()
}
#[allow(clippy::too_many_arguments)]
fn emit(
    node: Node<'_>,
    path: &str,
    text: &str,
    bindings: &BTreeMap<Key, &Binding>,
    selected: &Span,
    namespace_module: &str,
    aliases: Vec<(usize, usize)>,
    out: &mut Projection,
    d: &Deadline,
) -> Result<()> {
    let mut stack = vec![(node, false, 0usize, aliases)];
    while let Some((node, closing, depth, mut aliases)) = stack.pop() {
        d.check()?;
        ensure!(
            out.events.len() < 500_000 && depth <= 512,
            "Move canonical syntax capacity exceeded"
        );
        if closing {
            out.events.push(Event {
                signature: ")".into(),
                range: (node.start_byte(), node.end_byte()),
                aliases: vec![],
                identifier: false,
                binding: None,
                text: String::new(),
            });
            continue;
        }
        if node.kind() == ";"
            && node
                .parent()
                .is_none_or(|parent| parent.kind() != "for_statement")
        {
            continue;
        }
        let range = (node.start_byte(), node.end_byte());
        let raw = node.utf8_text(text.as_bytes())?;
        let property = match node.kind() {
            "member_expression" => node.child_by_field_name("property"),
            "nested_type_identifier" | "nested_identifier" => node.child_by_field_name("name"),
            _ => None,
        };
        let object = match node.kind() {
            "member_expression" => node.child_by_field_name("object"),
            "nested_type_identifier" | "nested_identifier" => node
                .child_by_field_name("module")
                .or_else(|| node.child_by_field_name("object")),
            _ => None,
        };
        if let (Some(property), Some(object)) = (property, object)
            && object.kind() == "identifier"
            && node.child_count() == 3
            && binding_at(bindings, path, property).is_some_and(|b| selected_binding(b, selected))
            && binding_at(bindings, path, object).is_some_and(|b| {
                b.resolution == "resolved"
                    && b.targets
                        == [Target::Module {
                            path: namespace_module.into(),
                        }]
            })
        {
            aliases.push(range);
            out.events.push(Event {
                signature: "@selected-reference".into(),
                range,
                aliases,
                identifier: true,
                binding: Some((property.start_byte(), property.end_byte())),
                text: property.utf8_text(text.as_bytes())?.into(),
            });
            continue;
        }
        if is_identifier(node.kind())
            && binding_at(bindings, path, node).is_some_and(|b| selected_binding(b, selected))
        {
            aliases.push(range);
            out.events.push(Event {
                signature: "@selected-reference".into(),
                range,
                aliases,
                identifier: true,
                binding: Some(range),
                text: raw.into(),
            });
            continue;
        }
        if node.kind() == "parenthesized_expression" {
            let mut cursor = node.walk();
            let children: Vec<_> = node.named_children(&mut cursor).collect();
            ensure!(
                children.len() == 1,
                "Move grouping/comment ownership is unsupported"
            );
            aliases.push(range);
            stack.push((children[0], false, depth + 1, aliases));
            continue;
        }
        let flatten = matches!(node.kind(), "formal_parameters" | "required_parameter");
        if !flatten {
            aliases.push(range);
            let identifier = is_identifier(node.kind());
            let signature = if node.child_count() == 0 && !identifier {
                format!("({}:{}:{}", node.kind(), raw.len(), raw)
            } else {
                format!("({}", node.kind())
            };
            out.events.push(Event {
                signature,
                range,
                aliases,
                identifier,
                binding: identifier.then_some(range),
                text: identifier.then(|| raw.to_owned()).unwrap_or_default(),
            });
            stack.push((node, true, depth, vec![]));
        }
        ensure!(
            stack.len().saturating_add(node.child_count()) <= 500_000,
            "Move syntax frontier capacity exceeded"
        );
        for i in (0..node.child_count()).rev() {
            if let Some(child) = node.child(i) {
                if node.kind() == "formal_parameters" && matches!(child.kind(), "(" | ")") {
                    continue;
                }
                stack.push((child, false, depth + 1, vec![]));
            }
        }
    }
    Ok(())
}
#[allow(clippy::too_many_arguments)]
pub(super) fn project(
    tree: &Tree,
    path: &str,
    text: &str,
    bindings: &BTreeMap<Key, &Binding>,
    selected: &Span,
    namespace_module: &str,
    remove: Option<&Selection<'_>>,
    only: Option<&Selection<'_>>,
    d: &Deadline,
) -> Result<Projection> {
    let mut result = Projection {
        declaration_empty: true,
        ..Default::default()
    };
    if let Some(only) = only {
        for node in only
            .docs
            .iter()
            .copied()
            .chain(std::iter::once(only.statement))
        {
            let underlying = unwrap(node);
            let aliases = if underlying != node {
                vec![(node.start_byte(), node.end_byte())]
            } else {
                vec![]
            };
            emit(
                underlying,
                path,
                text,
                bindings,
                selected,
                namespace_module,
                aliases,
                &mut result,
                d,
            )?;
        }
        return Ok(result);
    }
    let mut cursor = tree.root_node().walk();
    for node in tree.root_node().named_children(&mut cursor) {
        d.check()?;
        if remove.is_some_and(|remove| node == remove.statement || remove.docs.contains(&node)) {
            continue;
        }
        if node.kind() == "empty_statement" {
            continue;
        }
        let underlying = unwrap(node);
        if node.kind() == "import_statement"
            || node.kind() == "export_statement" && underlying == node
        {
            result.linkage.push((node.start_byte(), node.end_byte()));
            if node.child_by_field_name("source").is_some() {
                result.module_links.push((
                    node.start_byte(),
                    node.end_byte(),
                    if node.kind() == "import_statement" {
                        "import"
                    } else {
                        "reexport"
                    }
                    .into(),
                ));
            }
            if node.kind() == "import_statement" {
                let mut cursor = node.walk();
                if !node
                    .named_children(&mut cursor)
                    .any(|n| n.kind() == "import_clause")
                {
                    result
                        .side_effect_imports
                        .push(node.utf8_text(text.as_bytes())?.into());
                }
            }
            if node.kind() == "export_statement" && node.child_by_field_name("source").is_some() {
                let mut cursor = node.walk();
                if !node
                    .named_children(&mut cursor)
                    .any(|n| n.kind() == "export_clause")
                {
                    result
                        .side_effect_imports
                        .push(node.utf8_text(text.as_bytes())?.into());
                }
            }
            continue;
        }
        if !matches!(node.kind(), "comment" | "hash_bang_line")
            && !(node.kind() == "expression_statement"
                && node.named_child(0).is_some_and(|n| n.kind() == "string"))
        {
            result.declaration_empty = false;
        }
        let aliases = if underlying != node {
            vec![(node.start_byte(), node.end_byte())]
        } else {
            vec![]
        };
        emit(
            underlying,
            path,
            text,
            bindings,
            selected,
            namespace_module,
            aliases,
            &mut result,
            d,
        )?;
    }
    Ok(result)
}
pub(super) fn span(path: &str, node: Node<'_>, text: &str) -> Result<Span> {
    Ok(Span {
        path: path.into(),
        start_byte: node.start_byte(),
        end_byte: node.end_byte(),
        text: node.utf8_text(text.as_bytes())?.into(),
    })
}
pub(super) fn preamble(before: &str, after: &str) -> Result<()> {
    ensure!(
        before.starts_with('\u{feff}') == after.starts_with('\u{feff}'),
        "Move changes an original BOM"
    );
    let shebang = |s: &str| {
        s.trim_start_matches('\u{feff}')
            .lines()
            .next()
            .filter(|s| s.starts_with("#!"))
            .map(str::to_owned)
    };
    ensure!(
        shebang(before) == shebang(after),
        "Move changes an original shebang"
    );
    Ok(())
}
