//! Original-byte reconstruction of function-body call frames.
//! The controller and compiler witnesses admit scope/binding authority separately.
use super::*;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
#[path = "compiler_inline_body_admission.rs"]
mod admission;
#[path = "compiler_inline_body_arguments.rs"]
mod arguments;
#[path = "compiler_inline_body_types.rs"]
mod inferred;
#[path = "compiler_inline_body_parameters.rs"]
mod parameters;
#[path = "compiler_inline_body_predicates.rs"]
mod predicates;
#[path = "compiler_inline_body_vars.rs"]
mod variables;
pub(super) use admission::{classic, prepare};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Names {
    result: String,
    exit: String,
    arguments: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    do_first: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    var_parameters: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    var_seeds: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    return_type: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    argument_capture: Vec<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Frame {
    statement_start: usize,
    statement_end: usize,
    call_start: usize,
    call_end: usize,
    names: Names,
    result_type: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    var_types: Vec<Option<String>>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    var_seed_groups: Vec<usize>,
    #[serde(default)]
    literal_box: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    doc_pieces: Option<Vec<DocPiece>>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    argument_docs: Vec<arguments::Doc>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    argument_templates: Vec<DocPiece>,
    #[serde(default)]
    direct_arguments: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    predicate_names: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    inferred_predicate_parameter: Option<usize>,
    #[serde(default, skip_serializing_if = "false_flag")]
    dense_arguments: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct DocPiece {
    start_byte: usize,
    end_byte: usize,
    tag: bool,
}
fn false_flag(value: &bool) -> bool {
    !*value
}
fn fixed_const_assertion(tree: &Tree, start: usize, end: usize) -> bool {
    tree.root_node()
        .descendant_for_byte_range(start, end)
        .is_some_and(|n| {
            n.start_byte() == start
                && n.end_byte() == end
                && n.kind() == "const"
                && n.parent()
                    .is_some_and(|p| p.kind() == "as_expression" && p.end_byte() == end)
        })
}
#[derive(Debug)]
pub(super) struct Copy {
    pub original_start: usize,
    pub original_end: usize,
    pub output_start: usize,
    pub output_end: usize,
    pub role: &'static str,
}
#[derive(Debug, Default)]
pub(super) struct Rendered {
    pub text: String,
    pub copies: Vec<Copy>,
}
impl Rendered {
    fn append(&mut self, other: Rendered) -> Result<()> {
        let offset = self.text.len();
        self.generated(&other.text)?;
        ensure!(
            self.copies.len().saturating_add(other.copies.len()) <= 20_000,
            "Function-body copy capacity exceeded"
        );
        self.copies.extend(other.copies.into_iter().map(|mut c| {
            c.output_start += offset;
            c.output_end += offset;
            c
        }));
        Ok(())
    }
    fn generated(&mut self, text: &str) -> Result<()> {
        ensure!(
            self.text.len().saturating_add(text.len()) <= 4_000_000,
            "Function-body frame output capacity exceeded"
        );
        self.text.push_str(text);
        Ok(())
    }
    fn original(
        &mut self,
        source: &str,
        start: usize,
        end: usize,
        role: &'static str,
    ) -> Result<()> {
        let text = crate::paths::utf8_range(source, start, end)?;
        let output_start = self.text.len();
        self.generated(text)?;
        self.copies.push(Copy {
            original_start: start,
            original_end: end,
            output_start,
            output_end: self.text.len(),
            role,
        });
        ensure!(
            self.copies.len() <= 20_000,
            "Function-body copy capacity exceeded"
        );
        Ok(())
    }
}
fn anonymous_return_value(mut value: Node<'_>, d: &Deadline) -> Result<bool> {
    for _ in 0..2000 {
        d.check()?;
        match value.kind() {
            "arrow_function" | "function_expression" | "generator_function" | "class" => {
                return Ok(value.child_by_field_name("name").is_none());
            }
            "parenthesized_expression"
            | "as_expression"
            | "satisfies_expression"
            | "non_null_expression"
            | "instantiation_expression" => {
                value = *children(value)
                    .first()
                    .context("Return value wrapper is empty")?;
            }
            "type_assertion" => {
                value = *children(value)
                    .last()
                    .context("Return type assertion is empty")?;
            }
            _ => return Ok(false),
        }
    }
    anyhow::bail!("Return value wrapper capacity exceeded")
}
fn fresh_name(name: &str) -> bool {
    name.strip_prefix("__ci_inline_").is_some_and(|suffix| {
        suffix.len() <= 6
            && suffix
                .parse::<u32>()
                .is_ok_and(|n| (1..=100_000).contains(&n) && suffix == n.to_string())
    })
}
fn children<'a>(node: Node<'a>) -> Vec<Node<'a>> {
    let mut c = node.walk();
    node.named_children(&mut c)
        .filter(|n| n.kind() != "comment")
        .collect()
}
fn node_at<'a>(tree: &'a Tree, start: usize, end: usize, d: &Deadline) -> Result<Node<'a>> {
    let mut stack = vec![tree.root_node()];
    let mut visits = 0usize;
    while let Some(n) = stack.pop() {
        d.check()?;
        visits += 1;
        ensure!(visits <= 200_000, "Function-body anchor capacity exceeded");
        if n.start_byte() == start && n.end_byte() == end {
            return Ok(n);
        }
        if n.start_byte() <= start && n.end_byte() >= end {
            stack.extend(children(n));
        }
    }
    anyhow::bail!("Function-body exact syntax anchor absent")
}
fn statement_for(call: Node<'_>) -> Result<Node<'_>> {
    let mut value = call;
    while value
        .parent()
        .is_some_and(|p| p.kind() == "parenthesized_expression")
    {
        value = value.parent().unwrap();
    }
    let parent = value.parent().context("Function-body call parent absent")?;
    if parent.kind() == "variable_declarator" {
        ensure!(
            parent
                .child_by_field_name("value")
                .is_some_and(|n| n.id() == value.id()),
            "Function-body call is not the complete initializer"
        );
        let declaration = parent
            .parent()
            .context("Function-body variable statement absent")?;
        ensure!(
            !declaration
                .parent()
                .is_some_and(|n| n.kind() == "for_statement"),
            "Function-body for initializer needs separate sequencing"
        );
        ensure!(
            declaration.kind() == "lexical_declaration"
                && children(declaration)
                    .iter()
                    .filter(|n| n.kind() == "variable_declarator")
                    .count()
                    == 1,
            "Function-body sibling declaration needs separate sequencing"
        );
        return Ok(
            if declaration
                .parent()
                .is_some_and(|p| p.kind() == "export_statement")
            {
                declaration.parent().unwrap()
            } else {
                declaration
            },
        );
    }
    if parent.kind() == "assignment_expression" {
        let mut cursor = parent.walk();
        ensure!(
            parent
                .child_by_field_name("left")
                .is_some_and(|n| n.kind() == "identifier")
                && parent
                    .child_by_field_name("right")
                    .is_some_and(|n| n.id() == value.id())
                && parent.children(&mut cursor).any(|n| n.kind() == "="),
            "Function-body assignment target or right side needs sequencing"
        );
        let mut assignment = parent;
        let mut depth = 0usize;
        while assignment
            .parent()
            .is_some_and(|p| p.kind() == "parenthesized_expression")
        {
            depth += 1;
            ensure!(
                depth <= 2000,
                "Function-body assignment wrapper capacity exceeded"
            );
            assignment = assignment.parent().unwrap();
        }
        let statement = assignment
            .parent()
            .context("Function-body assignment statement absent")?;
        ensure!(
            statement.kind() == "expression_statement"
                && children(statement)
                    .first()
                    .is_some_and(|n| n.id() == assignment.id()),
            "Function-body assignment context needs sequencing"
        );
        return Ok(statement);
    }
    let complete_value = match parent.kind() {
        "if_statement" | "while_statement" | "for_statement" | "do_statement" => {
            parent.child_by_field_name("condition")
        }
        "switch_statement" => parent.child_by_field_name("value"),
        "expression_statement" | "return_statement" | "throw_statement" => {
            children(parent).first().copied()
        }
        _ => None,
    };
    ensure!(
        complete_value.is_some_and(|n| n.id() == value.id()),
        "Function-body expression context needs sequencing"
    );
    Ok(parent)
}
fn while_parts<'a>(
    statement: Node<'a>,
    call: Node<'a>,
    source: &str,
) -> Result<Option<(Node<'a>, Node<'a>)>> {
    if statement.kind() != "while_statement" {
        return Ok(None);
    }
    let condition = statement
        .child_by_field_name("condition")
        .context("Function-body while condition absent")?;
    let body = statement
        .child_by_field_name("body")
        .context("Function-body while body absent")?;
    ensure!(
        condition.kind() == "parenthesized_expression"
            && condition.end_byte() >= condition.start_byte() + 2
            && source.as_bytes().get(condition.start_byte()) == Some(&b'(')
            && source.as_bytes().get(condition.end_byte() - 1) == Some(&b')')
            && condition.start_byte() < call.start_byte()
            && call.end_byte() < condition.end_byte()
            && condition.end_byte() <= body.start_byte()
            && body.end_byte() == statement.end_byte()
            && statement_for(call)?.id() == statement.id(),
        "Function-body while delimiter or call ownership mismatch"
    );
    Ok(Some((condition, body)))
}
fn for_parts<'a>(
    statement: Node<'a>,
    call: Node<'a>,
    source: &str,
) -> Result<Option<(Node<'a>, Node<'a>)>> {
    if statement.kind() != "for_statement" {
        return Ok(None);
    }
    // The pinned grammar gives the expression AND its semicolon this field.
    // Read exactly those two nodes; the separator stays in the original header.
    let mut cursor = statement.walk();
    let mut fields = statement.children_by_field_name("condition", &mut cursor);
    let condition = fields
        .next()
        .context("Function-body for condition absent")?;
    let separator = fields
        .next()
        .context("Function-body for separator absent")?;
    let initializer = statement
        .child_by_field_name("initializer")
        .context("Function-body for initializer absent")?;
    let body = statement
        .child_by_field_name("body")
        .context("Function-body for body absent")?;
    ensure!(
        fields.next().is_none()
            && condition.is_named()
            && condition.kind() != "empty_statement"
            && separator.kind() == ";"
            && separator.end_byte() == separator.start_byte() + 1
            && source.as_bytes().get(separator.start_byte()) == Some(&b';')
            && initializer.end_byte() <= condition.start_byte()
            && condition.start_byte() <= call.start_byte()
            && call.end_byte() <= condition.end_byte()
            && condition.end_byte() <= separator.start_byte()
            && separator.end_byte() < body.start_byte()
            && body.end_byte() == statement.end_byte()
            && statement_for(call)?.id() == statement.id(),
        "Function-body for separator or call ownership mismatch"
    );
    Ok(Some((condition, body)))
}
#[derive(Clone, Copy)]
struct DoParts<'a> {
    condition: Node<'a>,
    body: Node<'a>,
    do_keyword: Node<'a>,
    while_keyword: Node<'a>,
}
fn do_parts<'a>(statement: Node<'a>, call: Node<'a>, source: &str) -> Result<Option<DoParts<'a>>> {
    if statement.kind() != "do_statement" {
        return Ok(None);
    }
    let condition = statement
        .child_by_field_name("condition")
        .context("Function-body do condition absent")?;
    let body = statement
        .child_by_field_name("body")
        .context("Function-body do body absent")?;
    let mut cursor = statement.walk();
    let keywords: Vec<_> = statement
        .children(&mut cursor)
        .filter(|n| matches!(n.kind(), "do" | "while"))
        .collect();
    ensure!(keywords.len() == 2, "Function-body do keywords absent");
    let do_keyword = keywords[0];
    let while_keyword = keywords[1];
    ensure!(
        do_keyword.kind() == "do"
            && while_keyword.kind() == "while"
            && do_keyword.start_byte() == statement.start_byte()
            && source.as_bytes().get(do_keyword.byte_range()) == Some(&b"do"[..])
            && source.as_bytes().get(while_keyword.byte_range()) == Some(&b"while"[..])
            && do_keyword.end_byte() <= body.start_byte()
            && body.end_byte() <= while_keyword.start_byte()
            && while_keyword.end_byte() <= condition.start_byte()
            && condition.kind() == "parenthesized_expression"
            && condition.end_byte() >= condition.start_byte() + 2
            && source.as_bytes().get(condition.start_byte()) == Some(&b'(')
            && source.as_bytes().get(condition.end_byte() - 1) == Some(&b')')
            && condition.start_byte() < call.start_byte()
            && call.end_byte() < condition.end_byte()
            && condition.end_byte() <= statement.end_byte()
            && statement_for(call)?.id() == statement.id(),
        "Function-body do delimiter, keyword or call ownership mismatch"
    );
    Ok(Some(DoParts {
        condition,
        body,
        do_keyword,
        while_keyword,
    }))
}
fn scope_forms(body: Node<'_>, source: &str, d: &Deadline) -> Result<()> {
    let mut stack = vec![body];
    let mut visits = 0usize;
    while let Some(n) = stack.pop() {
        d.check()?;
        visits += 1;
        ensure!(
            visits <= 200_000,
            "Function-body scope syntax capacity exceeded"
        );
        ensure!(
            !matches!(
                n.kind(),
                "with_statement"
                    | "await_expression"
                    | "yield_expression"
                    | "this"
                    | "super"
                    | "meta_property"
                    | "private_property_identifier"
            ),
            "Function-body scope form needs additional admission"
        );
        if n.kind() == "accessibility_modifier" {
            ensure!(
                !matches!(n.utf8_text(source.as_bytes())?, "private" | "protected"),
                "Function-body nominal private class needs preservation"
            );
        }
        if n.kind() == "identifier" {
            ensure!(
                !matches!(
                    parameters::identifier_name(n.utf8_text(source.as_bytes())?, d)?.as_str(),
                    "arguments" | "eval"
                ),
                "Function-body dynamic invocation identifier requires its original scope"
            );
        }
        if n.kind() == "call_expression"
            && let Some(target) = n.child_by_field_name("function")
        {
            ensure!(
                !(target.kind() == "identifier" && target.utf8_text(source.as_bytes())? == "eval"),
                "Function-body direct eval has dynamic binding authority"
            );
        }
        stack.extend(children(n));
    }
    Ok(())
}
fn function_boundary(node: Node<'_>) -> bool {
    matches!(
        node.kind(),
        "function_declaration"
            | "function_expression"
            | "generator_function"
            | "generator_function_declaration"
            | "arrow_function"
            | "method_definition"
    )
}
fn returns<'a>(body: Node<'a>, d: &Deadline) -> Result<Vec<Node<'a>>> {
    let mut stack = vec![body];
    let mut found = vec![];
    let mut visits = 0usize;
    while let Some(n) = stack.pop() {
        d.check()?;
        visits += 1;
        ensure!(visits <= 200_000, "Function-body syntax capacity exceeded");
        if n.id() != body.id() && function_boundary(n) {
            continue;
        }
        if n.kind() == "return_statement" {
            found.push(n);
            continue;
        }
        let mut c = n.walk();
        stack.extend(n.named_children(&mut c));
    }
    found.sort_by_key(Node::start_byte);
    ensure!(
        found.len() <= 1999,
        "Function-body return capacity exceeded"
    );
    Ok(found)
}
fn return_docs<'a>(source: &str, declaration: Node<'a>, d: &Deadline) -> Result<Vec<Node<'a>>> {
    d.check()?;
    let owner = declaration
        .parent()
        .filter(|n| n.kind() == "export_statement")
        .unwrap_or(declaration);
    let mut before = owner.prev_named_sibling();
    let mut end = owner.start_byte();
    let mut comments = vec![];
    let mut count = 0;
    let mut docs = vec![];
    while let Some(n) = before {
        d.check()?;
        if n.kind() != "comment"
            || !crate::paths::utf8_range(source, n.end_byte(), end)?
                .chars()
                .all(|c| c.is_whitespace() || c == '\u{feff}')
        {
            break;
        }
        count += 1;
        ensure!(
            count <= 200_000,
            "Body return type comment scan capacity exceeded"
        );
        comments.push(n);
        end = n.start_byte();
        before = n.prev_named_sibling();
    }
    let mut leading = end == 0;
    if !comments.is_empty() {
        for (visited, (index, ch)) in source[..end].char_indices().rev().enumerate() {
            d.check()?;
            ensure!(
                visited < 200_000,
                "Body return type whitespace capacity exceeded"
            );
            if !ch.is_whitespace() && ch != '\u{feff}' {
                break;
            }
            leading |= matches!(ch, '\r' | '\n') || index == 0;
        }
    }
    let mut cursor = end;
    for comment in comments.into_iter().rev() {
        d.check()?;
        leading |= crate::paths::utf8_range(source, cursor, comment.start_byte())?
            .bytes()
            .any(|b| matches!(b, b'\r' | b'\n'));
        let text = comment.utf8_text(source.as_bytes())?;
        if leading && text.starts_with("/**") && !text.starts_with("/**/") {
            docs.push(comment);
            ensure!(
                docs.len() <= 64,
                "Body return type comment capacity exceeded"
            );
        }
        cursor = comment.end_byte();
    }
    Ok(docs)
}
// Move attached caller documentation with its original lexical declaration.
// The source parser determines the widened edit; the provider supplies no range.
fn caller_start(source: &str, statement: Node<'_>, d: &Deadline) -> Result<usize> {
    if matches!(statement.kind(), "lexical_declaration" | "export_statement") {
        return Ok(return_docs(source, statement, d)?
            .first()
            .map_or(statement.start_byte(), Node::start_byte));
    }
    Ok(statement.start_byte())
}
fn documentation_pieces(
    source: &str,
    declaration: Node<'_>,
    frame: &Frame,
    d: &Deadline,
) -> Result<Vec<DocPiece>> {
    let docs = return_docs(source, declaration, d)?;
    let Some(pieces) = &frame.doc_pieces else {
        return Ok(docs
            .iter()
            .map(|n| DocPiece {
                start_byte: n.start_byte(),
                end_byte: n.end_byte(),
                tag: false,
            })
            .collect());
    };
    ensure!(
        pieces.len() <= 512,
        "Body documentation piece capacity exceeded"
    );
    ensure!(
        docs.iter()
            .any(|n| n.utf8_text(source.as_bytes()).is_ok_and(|s| [
                "@typedef",
                "@callback",
                "@import"
            ]
            .iter()
            .any(|tag| s.contains(tag)))),
        "Body split documentation lacks original declarations"
    );
    let mut end = 0;
    for piece in pieces {
        d.check()?;
        ensure!(
            piece.start_byte >= end && piece.start_byte < piece.end_byte,
            "Body documentation piece order mismatch"
        );
        let text = crate::paths::utf8_range(source, piece.start_byte, piece.end_byte)?;
        ensure!(
            piece.tag
                && docs.iter().any(|n| piece.start_byte >= n.start_byte() + 3
                    && piece.end_byte <= n.end_byte() - 2)
                && !text.contains("*/"),
            "Body documentation piece outside original comment"
        );
        let name = text
            .strip_prefix('@')
            .context("Body documentation tag marker absent")?
            .split(|c: char| !c.is_ascii_alphabetic())
            .next()
            .unwrap_or("");
        ensure!(
            [
                "param",
                "returns",
                "return",
                "template",
                "this",
                "type",
                "satisfies",
                "overload",
                "constructor",
                "class",
                "extends",
                "augments",
                "implements"
            ]
            .contains(&name),
            "Body documentation tag outside function contract"
        );
        end = piece.end_byte;
    }
    Ok(pieces.clone())
}
fn emit_documentation(
    out: &mut Rendered,
    source: &str,
    docs: &[DocPiece],
    role: &'static str,
    line: &str,
) -> Result<()> {
    let tags = docs.first().is_some_and(|doc| doc.tag);
    ensure!(
        docs.iter().all(|doc| doc.tag == tags),
        "Body documentation recipe is mixed"
    );
    if tags {
        out.generated("/** ")?;
    }
    for doc in docs {
        out.original(source, doc.start_byte, doc.end_byte, role)?;
        out.generated(line)?;
    }
    if tags {
        out.generated(&format!(" */{line}"))?;
    }
    Ok(())
}
fn return_witness(
    out: &mut Rendered,
    source: &str,
    declaration: Node<'_>,
    frame: &Frame,
    language: &str,
    line: &str,
    d: &Deadline,
) -> Result<()> {
    d.check()?;
    let names = &frame.names;
    if names.return_type.is_empty() {
        return Ok(());
    }
    ensure!(
        names.return_type.len() == 2,
        "Body return type name count mismatch"
    );
    let docs = documentation_pieces(source, declaration, frame, d)?;
    out.generated(&format!(
        "if (void 0 !== void 0) {{{line}var {} = ",
        names.return_type[0]
    ))?;
    emit_documentation(out, source, &docs, "return_witness", line)?;
    let parameters = declaration
        .child_by_field_name("parameters")
        .context("Body return type parameters absent")?;
    out.generated("function")?;
    out.original(
        source,
        parameters.start_byte(),
        declaration.end_byte(),
        "return_witness",
    )?;
    out.generated(&format!(";var {} = ", names.return_type[1]))?;
    let js = matches!(language, "javascript" | "jsx");
    if frame.literal_box {
        out.generated(if js {
            "/** @type {const} */ ({[\"value\"]: "
        } else {
            "({[\"value\"]: "
        })?;
    }
    out.generated(&format!(
        "{}({})",
        names.return_type[0],
        names.arguments.join(","),
    ))?;
    if frame.literal_box {
        out.generated(if js { "})" } else { "} as const)" })?;
    }
    out.generated(&format!(";void {};{line}}}{line}", names.return_type[1]))?;
    Ok(())
}
/// This renderer never trusts a supplied replacement string. Every original
/// expression, comment and body fragment is sliced from the checked source.
pub(super) fn render(
    tree: &Tree,
    source: &str,
    language: &str,
    declaration: Node<'_>,
    frame: &Frame,
    d: &Deadline,
) -> Result<Rendered> {
    d.check()?;
    ensure!(
        matches!(language, "typescript" | "tsx" | "javascript" | "jsx"),
        "Function-body language unavailable"
    );
    ensure!(
        declaration.kind() == "function_declaration",
        "Function-body declaration kind unavailable"
    );
    let name = declaration
        .child_by_field_name("name")
        .context("Function-body name absent")?;
    let parameters = declaration
        .child_by_field_name("parameters")
        .context("Function-body parameters absent")?;
    let body = declaration
        .child_by_field_name("body")
        .context("Function-body body absent")?;
    let mut c = declaration.walk();
    ensure!(
        !declaration
            .children(&mut c)
            .any(|n| matches!(n.kind(), "async" | "*" | "type_parameters")),
        "Function-body async/generic/generator frame unavailable"
    );
    let statement = node_at(tree, frame.statement_start, frame.statement_end, d)?;
    let call = node_at(tree, frame.call_start, frame.call_end, d)?;
    ensure!(
        call.kind() == "call_expression" && statement_for(call)?.id() == statement.id(),
        "Function-body statement ownership mismatch"
    );
    let repeated = while_parts(statement, call, source)?;
    let for_repeated = for_parts(statement, call, source)?;
    let do_repeated = do_parts(statement, call, source)?;
    ensure!(
        frame.names.do_first.is_some() == do_repeated.is_some(),
        "Function-body do first-iteration name ownership mismatch"
    );
    scope_forms(declaration, source, d)?;
    let return_nodes = returns(body, d)?;
    documentation_pieces(source, declaration, frame, d)?;
    ensure!(
        !frame.literal_box || frame.names.return_type.len() == 2,
        "Body literal carrier lacks inferred result"
    );
    ensure!(
        frame.names.return_type.len()
            == if declaration.child_by_field_name("return_type").is_none()
                && frame.result_type.is_none()
            {
                2
            } else {
                0
            },
        "Body return type witness shape mismatch"
    );
    let vars = variables::analyze(declaration, source, d)?;
    let typed = inferred::analyze(&vars, &frame.var_seed_groups, source, d)?;
    ensure!(
        vars.copies() == frame.names.var_parameters.len()
            && typed.name_count == frame.names.var_seeds.len(),
        "Var parameter copy shape mismatch"
    );
    let called = call
        .child_by_field_name("function")
        .context("Function-body call target absent")?;
    ensure!(
        called.kind() == "identifier"
            && called.utf8_text(source.as_bytes())? == name.utf8_text(source.as_bytes())?
            && call.start_byte() >= statement.start_byte()
            && call.end_byte() <= statement.end_byte()
            && (statement.end_byte() <= declaration.start_byte()
                || statement.start_byte() >= declaration.end_byte()),
        "Function-body call ownership mismatch"
    );
    let argument_list = call
        .child_by_field_name("arguments")
        .context("Function-body arguments absent")?;
    let args = children(argument_list);
    ensure!(
        args.len() <= 64
            && !args.iter().any(|n| n.kind() == "spread_element")
            && args.len() == frame.names.arguments.len(),
        "Function-body argument shape/capacity mismatch"
    );
    let parameter_nodes = children(parameters);
    ensure!(
        parameter_nodes.len() <= 64,
        "Function-body parameter capacity exceeded"
    );
    let names: Vec<_> = std::iter::once(&frame.names.result)
        .chain(std::iter::once(&frame.names.exit))
        .chain(&frame.names.arguments)
        .chain(&frame.names.var_parameters)
        .chain(&frame.names.var_seeds)
        .chain(&frame.names.return_type)
        .chain(&frame.names.argument_capture)
        .chain(&frame.names.do_first)
        .chain(&frame.predicate_names)
        .collect();
    let name_set: BTreeSet<_> = names.iter().map(|n| n.as_str()).collect();
    ensure!(
        names.iter().all(|n| fresh_name(n)) && name_set.len() == names.len(),
        "Function-body synthetic names are not distinct canonical names"
    );
    let mut stack = vec![tree.root_node()];
    let mut visits = 0usize;
    while let Some(n) = stack.pop() {
        d.check()?;
        visits += 1;
        ensure!(
            visits <= 200_000,
            "Function-body freshness syntax capacity exceeded"
        );
        if n.kind().contains("identifier") {
            let text = parameters::identifier_name(n.utf8_text(source.as_bytes())?, d)?;
            ensure!(
                !name_set.contains(text.as_str()),
                "Function-body synthetic name is already in source"
            );
        }
        let mut c = n.walk();
        stack.extend(n.named_children(&mut c));
    }
    let line = if source.contains("\r\n") {
        "\r\n"
    } else {
        "\n"
    };
    let mut out = Rendered::default();
    let wrap = matches!(
        statement.kind(),
        "return_statement"
            | "expression_statement"
            | "throw_statement"
            | "if_statement"
            | "switch_statement"
    );
    if let Some(parts) = do_repeated {
        // Continue performs the for update before entering the original test.
        // The first iteration skips that test and enters the untouched body.
        let first = frame.names.do_first.as_ref().unwrap();
        out.generated(&format!(
            "for (let {first} = true; ; {first} = false) {{{line}if (!{first}) {{{line}"
        ))?;
        out.original(
            source,
            parts.body.end_byte(),
            parts.while_keyword.start_byte(),
            "caller",
        )?;
        out.generated(line)?;
        out.original(
            source,
            parts.while_keyword.end_byte(),
            parts.condition.start_byte(),
            "caller",
        )?;
        out.generated(line)?;
    } else if let Some((condition, loop_body)) = for_repeated {
        // Preserve the actual for initializer/update and its per-iteration
        // environment. Continue still performs that update before the next test.
        out.original(
            source,
            statement.start_byte(),
            condition.start_byte(),
            "caller",
        )?;
        out.generated("true")?;
        out.original(
            source,
            condition.end_byte(),
            loop_body.start_byte(),
            "caller",
        )?;
        out.generated(&format!("{{{line}"))?;
    } else if let Some((condition, loop_body)) = repeated {
        // A caller label must still name this loop. Captures belong to each
        // condition test, including tests reached by an original continue.
        out.original(
            source,
            statement.start_byte(),
            condition.start_byte() + 1,
            "caller",
        )?;
        out.generated("true")?;
        out.original(
            source,
            condition.end_byte() - 1,
            loop_body.start_byte(),
            "caller",
        )?;
        out.generated(&format!("{{{line}"))?;
    } else if wrap {
        out.generated(&format!("{{{line}"))?;
    }
    let predicate = predicates::begin(&mut out, source, declaration, frame, language, line, d)?;
    out.generated(&format!("let {}", frame.names.result))?;
    if let Some(predicate) = predicate {
        out.generated(if predicate.assertion {
            ": (void) | undefined"
        } else {
            ": (boolean) | undefined"
        })?;
    } else if let Some(annotation) = declaration.child_by_field_name("return_type") {
        out.generated(": (")?;
        out.original(
            source,
            annotation.start_byte() + 1,
            annotation.end_byte(),
            "return_type",
        )?;
        out.generated(") | undefined")?;
    }
    out.generated(&format!(";{line}{{{line}"))?;
    arguments::emit(
        &mut out,
        source,
        declaration,
        argument_list,
        frame,
        language,
        line,
        d,
    )?;
    return_witness(&mut out, source, declaration, frame, language, line, d)?;
    out.generated(&format!("{{{line}"))?;
    parameters::emit(
        &mut out,
        source,
        parameters,
        &frame.names.arguments,
        line,
        d,
    )?;
    vars.captures(&mut out, source, &frame.names, line)?;
    if !return_nodes.is_empty() {
        out.generated(&format!("{}: ", frame.names.exit))?;
    }
    out.generated(&format!("{{{line}if (void 0 === void 0) "))?;
    out.original(source, body.start_byte(), body.start_byte() + 1, "body")?;
    if !vars.groups.is_empty() {
        out.generated(line)?;
        vars.hoists(&mut out, source, frame, &typed, language, line)?;
    }
    let mut replacements = typed.changes(source, &frame.names, line, d)?;
    for change in &vars.changes {
        let (start, end) = change.range();
        replacements.push((start, end, variables::replacement(change, source, d)?));
    }
    for ret in return_nodes {
        let mut value_out = Rendered::default();
        let values = children(ret);
        ensure!(
            values.len() <= 1,
            "Function-body return expression multiplicity"
        );
        value_out.generated(&format!("{{ {} = ", frame.names.result))?;
        let tail_start = if let Some(value) = values.first() {
            value_out.generated(if anonymous_return_value(*value, d)? {
                "(void 0,"
            } else {
                "("
            })?;
            value_out.original(source, ret.start_byte() + 6, value.end_byte(), "body")?;
            value_out.generated(")")?;
            value.end_byte()
        } else {
            value_out.generated("void 0")?;
            ret.start_byte() + 6
        };
        value_out.original(source, tail_start, ret.end_byte(), "body")?;
        value_out.generated(&format!("; break {}; }}", frame.names.exit))?;
        replacements.push((ret.start_byte(), ret.end_byte(), value_out));
    }
    replacements.sort_by_key(|r| (r.0, r.1));
    ensure!(
        replacements.len() <= 2000 && replacements.windows(2).all(|p| p[0].1 <= p[1].0),
        "Body var/return reconstruction overlap or capacity"
    );
    let mut cursor = body.start_byte() + 1;
    for (start, end, value) in replacements {
        out.original(source, cursor, start, "body")?;
        out.append(value)?;
        cursor = end;
    }
    out.original(source, cursor, body.end_byte() - 1, "body")?;
    out.original(source, body.end_byte() - 1, body.end_byte(), "body")?;
    // A finally break/continue can cancel a pending return.
    out.generated(&format!("{line}{} = void 0;{line}", frame.names.result))?;
    out.generated(&format!("}}{line}}}{line}"))?;
    predicates::finish(&mut out, frame, line)?;
    out.generated(&format!("}}{line}"))?;
    if let Some(parts) = do_repeated {
        out.generated("if (!")?;
        out.original(
            source,
            parts.condition.start_byte(),
            call.start_byte(),
            "caller",
        )?;
    } else if let Some((condition, _)) = for_repeated {
        out.generated("if (!(")?;
        out.original(source, condition.start_byte(), call.start_byte(), "caller")?;
    } else if let Some((condition, _)) = repeated {
        out.generated("if (!")?;
        out.original(source, condition.start_byte(), call.start_byte(), "caller")?;
    } else {
        out.original(
            source,
            caller_start(source, statement, d)?,
            call.start_byte(),
            "caller",
        )?;
    }
    let annotated = declaration.child_by_field_name("return_type");
    ensure!(
        annotated.is_none() || frame.result_type.is_none(),
        "Function-body return annotation must retain its original source"
    );
    if let Some(predicate) = predicate {
        predicates::result(&mut out, source, &args, frame, predicate, d)?;
    } else if let Some(annotation) = annotated {
        ensure!(
            matches!(language, "typescript" | "tsx")
                && source.as_bytes().get(annotation.start_byte()) == Some(&b':'),
            "Function-body return annotation shape mismatch"
        );
        out.generated(&format!("({} as unknown as (", frame.names.result))?;
        out.original(
            source,
            annotation.start_byte() + 1,
            annotation.end_byte(),
            "return_type",
        )?;
        out.generated("))")?;
    } else if let Some(hint) = &frame.result_type {
        ensure!(
            [
                "any",
                "unknown",
                "number",
                "string",
                "boolean",
                "bigint",
                "symbol",
                "undefined",
                "void",
                "never"
            ]
            .contains(&hint.as_str()),
            "Function-body generated return type unavailable"
        );
        out.generated(&if matches!(language, "javascript" | "jsx") {
            format!(
                "/** @type {{{hint}}} */ (/** @type {{unknown}} */ ({}))",
                frame.names.result
            )
        } else {
            format!("({} as unknown as ({hint}))", frame.names.result)
        })?;
    } else {
        let names = &frame.names.return_type;
        let hint = if frame.literal_box {
            format!("(typeof {})[\"value\"]", names[1])
        } else {
            format!("typeof {}", names[1])
        };
        out.generated(&if matches!(language, "javascript" | "jsx") {
            format!(
                "/** @type {{{hint}}} */ (/** @type {{unknown}} */ ({}))",
                frame.names.result
            )
        } else {
            format!("({} as unknown as ({hint}))", frame.names.result)
        })?;
    }
    if let Some(parts) = do_repeated {
        out.original(
            source,
            call.end_byte(),
            parts.condition.end_byte(),
            "caller",
        )?;
        out.generated(&format!(") break;{line}"))?;
        out.original(
            source,
            parts.condition.end_byte(),
            statement.end_byte(),
            "caller",
        )?;
        out.generated(&format!("{line}}}{line}"))?;
        out.original(
            source,
            parts.do_keyword.end_byte(),
            parts.body.start_byte(),
            "caller",
        )?;
        out.original(
            source,
            parts.body.start_byte(),
            parts.body.end_byte(),
            "caller",
        )?;
        out.generated(&format!("{line}}}"))?;
    } else if let Some((condition, loop_body)) = for_repeated {
        out.original(source, call.end_byte(), condition.end_byte(), "caller")?;
        out.generated(&format!(")) break;{line}"))?;
        out.original(
            source,
            loop_body.start_byte(),
            loop_body.end_byte(),
            "caller",
        )?;
        out.generated(&format!("{line}}}"))?;
    } else if let Some((condition, loop_body)) = repeated {
        out.original(source, call.end_byte(), condition.end_byte(), "caller")?;
        out.generated(&format!(") break;{line}"))?;
        out.original(
            source,
            loop_body.start_byte(),
            loop_body.end_byte(),
            "caller",
        )?;
        out.generated(&format!("{line}}}"))?;
    } else {
        out.original(source, call.end_byte(), statement.end_byte(), "caller")?;
    }
    if wrap {
        out.generated(&format!("{line}}}"))?;
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn statement_heads_require_the_exact_original_ast_field() {
        let d = Deadline::new(10_000);
        for (statement, expected) in [
            ("if(((helper()))){}else{}", Some("if_statement")),
            (
                "switch(((helper()))){default:break;}",
                Some("switch_statement"),
            ),
            ("throw ((helper()));", Some("throw_statement")),
            ("target = ((helper()));", Some("expression_statement")),
            ("(((target = helper())));", Some("expression_statement")),
            (
                "if(flag)target=helper();else target=0;",
                Some("expression_statement"),
            ),
            ("box.value=helper();", None),
            ("box[key()]=helper();", None),
            ("base().value=helper();", None),
            ("[target]=helper();", None),
            ("({target}=helper());", None),
            ("target+=helper();", None),
            ("target||=helper();", None),
            ("other=target=helper();", None),
            ("return target=helper();", None),
            ("use(target=helper());", None),
            ("for(target=helper();;){break;}", None),
            ("((target=helper()),target++);", None),
            ("(target as number)=helper();", None),
            ("if(flag&&helper()){}", None),
            ("switch(1+helper()){}", None),
            ("throw other(helper());", None),
            ("while(helper()){}", Some("while_statement")),
            ("while(((helper()))){}", Some("while_statement")),
            ("while(helper()&&flag){}", None),
            ("do{}while(helper());", Some("do_statement")),
            ("do{}while(((helper())));", Some("do_statement")),
            ("do{}while(helper()&&flag);", None),
            ("for(;helper();){}", Some("for_statement")),
            ("for(;((helper()));tick()){}", Some("for_statement")),
            ("for(;helper()&&flag;){}", None),
            ("for(let value=helper();;){}", None),
            ("for(;true;helper()){}", None),
            ("for(const x of helper()){}", None),
            ("switch(flag){case helper():break;}", None),
        ] {
            let source = format!("function run(){{{statement}}}");
            let tree = parse("typescript", &source, &d).unwrap();
            let start = source.find("helper()").unwrap();
            let call = node_at(&tree, start, start + "helper()".len(), &d).unwrap();
            let actual = statement_for(call);
            assert_eq!(actual.as_ref().ok().map(|n| n.kind()), expected, "{source}");
        }
        for depth in [1, 2000, 2001] {
            let source = format!("{}target=helper(){};", "(".repeat(depth), ")".repeat(depth));
            let tree = parse("typescript", &source, &d).unwrap();
            let start = source.find("helper()").unwrap();
            let call = node_at(&tree, start, start + "helper()".len(), &d).unwrap();
            assert_eq!(statement_for(call).is_ok(), depth <= 2000, "{depth}");
        }
    }
    #[test]
    fn while_reconstruction_requires_owned_condition_delimiters_and_preserves_body_boundaries() {
        let d = Deadline::new(10_000);
        for language in ["typescript", "tsx", "javascript", "jsx"] {
            for newline in ["\n", "\r\n"] {
                let source = "export {};\nouter:while /* café 🦀 header */ (/* test */((helper()))/* tail */) /* between */ {continue outer;}\n".replace('\n', newline);
                let tree = parse(language, &source, &d).unwrap();
                let start = source.find("helper()").unwrap();
                let call = node_at(&tree, start, start + "helper()".len(), &d).unwrap();
                let statement = statement_for(call).unwrap();
                let (condition, body) = while_parts(statement, call, &source).unwrap().unwrap();
                assert_eq!(
                    &source[condition.start_byte()..condition.end_byte()],
                    "(/* test */((helper()))/* tail */)"
                );
                assert_eq!(
                    &source[body.start_byte()..body.end_byte()],
                    "{continue outer;}"
                );
                assert_eq!(statement.end_byte(), body.end_byte());
                assert_eq!(
                    &source[statement.start_byte()..condition.start_byte() + 1],
                    "while /* café 🦀 header */ ("
                );
                let mut altered = source.clone();
                altered.replace_range(condition.start_byte()..condition.start_byte() + 1, "[");
                assert!(while_parts(statement, call, &altered).is_err());
                let mut altered = source.clone();
                altered.replace_range(condition.end_byte() - 1..condition.end_byte(), "]");
                assert!(while_parts(statement, call, &altered).is_err());
            }
        }
        for source in [
            "while(flag){helper();}",
            "while(helper()&&flag){}",
            "while(flag){while(helper())break;}",
        ] {
            let tree = parse("typescript", source, &d).unwrap();
            let statement = tree.root_node().named_child(0).unwrap();
            let start = source.find("helper()").unwrap();
            let call = node_at(&tree, start, start + "helper()".len(), &d).unwrap();
            assert!(while_parts(statement, call, source).is_err(), "{source}");
        }
    }
    #[test]
    fn for_reconstruction_owns_the_condition_and_separator_without_accepting_other_header_fields() {
        let d = Deadline::new(10_000);
        for language in ["typescript", "tsx", "javascript", "jsx"] {
            for initializer in ["let n=0", "var n=0", "const n=0", ""] {
                for newline in ["\n", "\r\n"] {
                    let source = format!("export {{}};\nouter:for /* café 🦀 header */ ({initializer};/* before */((helper()))/* after */;tick())/* between */{{continue outer;}}\n").replace('\n', newline);
                    let tree = parse(language, &source, &d).unwrap();
                    let start = source.find("helper()").unwrap();
                    let call = node_at(&tree, start, start + "helper()".len(), &d).unwrap();
                    let statement = statement_for(call).unwrap();
                    let (condition, body) = for_parts(statement, call, &source).unwrap().unwrap();
                    assert_eq!(
                        &source[condition.start_byte()..condition.end_byte()],
                        "((helper()))"
                    );
                    assert_eq!(
                        &source[condition.end_byte()..body.start_byte()],
                        "/* after */;tick())/* between */"
                    );
                    assert_eq!(
                        &source[body.start_byte()..body.end_byte()],
                        "{continue outer;}"
                    );
                    let separator = source.find("/* after */;").unwrap() + "/* after */".len();
                    let mut altered = source.clone();
                    altered.replace_range(separator..separator + 1, ":");
                    assert!(for_parts(statement, call, &altered).is_err());
                }
            }
        }
        for source in [
            "for(let value=helper();true;tick()){}",
            "for(;true;helper()){}",
            "for(;flag;){helper();}",
            "for(;helper()&&flag;){}",
            "for(;flag;){for(;helper();)break;}",
        ] {
            let tree = parse("typescript", source, &d).unwrap();
            let statement = tree.root_node().named_child(0).unwrap();
            let start = source.find("helper()").unwrap();
            let call = node_at(&tree, start, start + "helper()".len(), &d).unwrap();
            assert!(for_parts(statement, call, source).is_err(), "{source}");
        }
    }
    #[test]
    fn anonymous_return_barriers_follow_expression_wrappers_and_preserve_never_call_flow() {
        let d = Deadline::new(10_000);
        for (value, expected) in [
            ("()=>3", true),
            ("function(){}", true),
            ("function*(){}", true),
            ("class{}", true),
            ("(((function(){})))", true),
            ("((()=>3) as (()=>number))", true),
            ("<(()=>number)>(()=>3)", true),
            ("((()=>3) satisfies (()=>number))", true),
            ("(function(){})!", true),
            ("(<T>(n:T)=>n)<number>", true),
            ("opaque()", false),
            ("(opaque())", false),
            ("opaque() as number", false),
            ("(void 0,()=>3)", false),
            ("{run:()=>3}", false),
            ("flag?()=>3:()=>4", false),
            ("'()=>3'", false),
            ("function named(){}", false),
            ("class Named{}", false),
        ] {
            let source = format!("function helper(){{return {value};}}");
            let tree = parse("typescript", &source, &d).unwrap();
            let declaration = tree.root_node().named_child(0).unwrap();
            let ret = returns(declaration.child_by_field_name("body").unwrap(), &d).unwrap()[0];
            let expression = children(ret)[0];
            assert_eq!(
                anonymous_return_value(expression, &d).unwrap(),
                expected,
                "{value}: {}",
                expression.to_sexp()
            );
            assert!(anonymous_return_value(expression, &Deadline::new(0)).is_err());
        }
    }
    #[test]
    fn fixed_const_type_tokens_cannot_be_other_keywords_properties_or_comment_text() {
        let d = Deadline::new(10_000);
        for (source, expected) in [
            ("let x=({[\"value\"]:3} as const);", true),
            ("const x=3;", false),
            ("let x=\"const\";", false),
            ("// as const\nlet x=3;", false),
            ("type X={const:number};", false),
            ("let x={} as Namespace.const;", false),
        ] {
            let tree = parse("typescript", source, &d).unwrap();
            let at = source.rfind("const").unwrap();
            assert_eq!(
                fixed_const_assertion(&tree, at, at + 5),
                expected,
                "{source}"
            );
            assert!(!fixed_const_assertion(
                &tree,
                source.len() + 1,
                source.len() + 2
            ));
        }
    }
    #[test]
    fn do_reconstruction_owns_keywords_delimiters_body_and_the_complete_condition() {
        let d = Deadline::new(10_000);
        for language in ["typescript", "tsx", "javascript", "jsx"] {
            for newline in ["\n", "\r\n"] {
                for body in [
                    "{continue outer;}",
                    "if(flag)continue outer;else break outer;",
                    ";",
                ] {
                    for tail in [";", ""] {
                        let source = format!("export {{}};\nouter:do /* café 🦀 before */ {body} /* after body */ while /* before test */ (/* test */((helper()))/* tail */){tail}\n").replace('\n', newline);
                        let tree = parse(language, &source, &d).unwrap();
                        let start = source.find("helper()").unwrap();
                        let call = node_at(&tree, start, start + "helper()".len(), &d).unwrap();
                        let statement = statement_for(call).unwrap();
                        let parts = do_parts(statement, call, &source).unwrap().unwrap();
                        assert_eq!(&source[parts.body.byte_range()], body);
                        assert_eq!(
                            &source[parts.condition.byte_range()],
                            "(/* test */((helper()))/* tail */)"
                        );
                        assert_eq!(&source[parts.do_keyword.byte_range()], "do");
                        assert_eq!(&source[parts.while_keyword.byte_range()], "while");
                        for (start, end, replacement) in [
                            (
                                parts.do_keyword.start_byte(),
                                parts.do_keyword.end_byte(),
                                "if",
                            ),
                            (
                                parts.while_keyword.start_byte(),
                                parts.while_keyword.end_byte(),
                                "whilX",
                            ),
                            (
                                parts.condition.start_byte(),
                                parts.condition.start_byte() + 1,
                                "[",
                            ),
                            (
                                parts.condition.end_byte() - 1,
                                parts.condition.end_byte(),
                                "]",
                            ),
                        ] {
                            let mut altered = source.clone();
                            altered.replace_range(start..end, replacement);
                            assert!(do_parts(statement, call, &altered).is_err());
                        }
                    }
                }
            }
        }
        for source in [
            "do{helper();}while(flag);",
            "do{}while(helper()&&flag);",
            "do{do{}while(helper());}while(flag);",
        ] {
            let tree = parse("typescript", source, &d).unwrap();
            let statement = tree.root_node().named_child(0).unwrap();
            let start = source.find("helper()").unwrap();
            let call = node_at(&tree, start, start + "helper()".len(), &d).unwrap();
            assert!(do_parts(statement, call, source).is_err(), "{source}");
        }
    }
    #[test]
    fn do_first_iteration_names_are_required_fresh_and_absent_from_legacy_witnesses() {
        let source = "function helper():boolean{return false;}do{}while(helper());";
        let d = Deadline::new(10_000);
        let tree = parse("typescript", source, &d).unwrap();
        let declaration = tree.root_node().named_child(0).unwrap();
        let start = source.rfind("helper()").unwrap();
        let call = node_at(&tree, start, start + "helper()".len(), &d).unwrap();
        let statement = statement_for(call).unwrap();
        let mut frame: Frame = serde_json::from_value(serde_json::json!({
            "statementStart": statement.start_byte(), "statementEnd": statement.end_byte(),
            "callStart": call.start_byte(), "callEnd": call.end_byte(),
            "names": {"result":"__ci_inline_1","exit":"__ci_inline_2","arguments":[]}
        }))
        .unwrap();
        assert!(
            serde_json::to_value(&frame).unwrap()["names"]
                .get("doFirst")
                .is_none()
        );
        assert!(render(&tree, source, "typescript", declaration, &frame, &d).is_err());
        frame.names.do_first = Some("__ci_inline_3".into());
        let out = render(&tree, source, "typescript", declaration, &frame, &d).unwrap();
        assert!(
            out.text
                .starts_with("for (let __ci_inline_3 = true; ; __ci_inline_3 = false) {")
        );
        parse("typescript", &out.text, &d).unwrap();
        for copy in &out.copies {
            assert_eq!(
                &source[copy.original_start..copy.original_end],
                &out.text[copy.output_start..copy.output_end]
            );
        }
        frame.names.do_first = Some(frame.names.result.clone());
        assert!(render(&tree, source, "typescript", declaration, &frame, &d).is_err());
        frame.names.do_first = Some("first".into());
        assert!(render(&tree, source, "typescript", declaration, &frame, &d).is_err());
        frame.names.do_first = Some("__ci_inline_3".into());
        assert!(
            render(
                &tree,
                source,
                "typescript",
                declaration,
                &frame,
                &Deadline::new(0)
            )
            .is_err()
        );
    }
    #[test]
    fn return_type_witnesses_keep_comment_ownership_original_bytes_and_bounds() {
        let names = Names {
            do_first: None,
            result: "__ci_inline_1".into(),
            exit: "__ci_inline_2".into(),
            arguments: vec![],
            var_parameters: vec![],
            var_seeds: vec![],
            return_type: vec!["__ci_inline_3".into(), "__ci_inline_4".into()],
            argument_capture: vec![],
        };
        let frame = Frame {
            statement_start: 0,
            statement_end: 0,
            call_start: 0,
            call_end: 0,
            names,
            result_type: None,
            var_types: vec![],
            var_seed_groups: vec![],
            literal_box: false,
            doc_pieces: None,
            argument_docs: vec![],
            argument_templates: vec![],
            direct_arguments: false,
            predicate_names: vec![],
            inferred_predicate_parameter: None,
            dense_arguments: false,
        };
        let doc = "/** @returns {{n:number}} */";
        for (prefix, suffix, attached) in [
            (format!("{doc}\n"), "", true),
            (format!("const marker=1;{doc} "), "", false),
            (format!("const marker=1;\n{doc} "), "", true),
            (
                format!("const marker=1;/* multiline\n */ {doc} "),
                "",
                false,
            ),
            (format!("const marker=1;\u{2028}{doc} "), "", false),
            (format!("\u{feff}{doc} "), "", true),
            (format!("{doc}\u{feff} "), "", true),
            (format!("{{{doc} "), "}", false),
            (format!("{{\r\n{doc} "), "}", true),
        ] {
            let source = format!("{prefix}function helper(n=3){{return {{n}};}}{suffix}");
            let d = Deadline::new(10_000);
            let tree = parse("javascript", &source, &d).unwrap();
            let mut stack = vec![tree.root_node()];
            let mut found = None;
            while let Some(n) = stack.pop() {
                if n.kind() == "function_declaration" {
                    found = Some(n);
                    break;
                }
                stack.extend(children(n));
            }
            let declaration = found.unwrap();
            let mut out = Rendered::default();
            return_witness(
                &mut out,
                &source,
                declaration,
                &frame,
                "javascript",
                "\n",
                &d,
            )
            .unwrap();
            assert_eq!(out.text.contains(doc), attached, "{source}");
            for c in &out.copies {
                assert_eq!(
                    &source[c.original_start..c.original_end],
                    &out.text[c.output_start..c.output_end]
                );
                assert_eq!(c.role, "return_witness");
            }
            assert!(
                return_witness(
                    &mut Rendered::default(),
                    &source,
                    declaration,
                    &frame,
                    "javascript",
                    "\n",
                    &Deadline::new(0)
                )
                .is_err()
            );
        }
        for prefix in [
            "/** ordinary */\n".repeat(65),
            format!("{}{}\n", " ".repeat(200_001), doc),
        ] {
            let source = format!("{prefix}function helper(){{return {{n:3}};}}");
            let d = Deadline::new(10_000);
            let tree = parse("javascript", &source, &d).unwrap();
            let mut c = tree.root_node().walk();
            let declaration = tree
                .root_node()
                .named_children(&mut c)
                .find(|n| n.kind() == "function_declaration")
                .unwrap();
            assert!(
                return_witness(
                    &mut Rendered::default(),
                    &source,
                    declaration,
                    &frame,
                    "javascript",
                    "\n",
                    &d
                )
                .is_err()
            );
        }
    }
    #[test]
    fn caller_comment_ownership_is_derived_from_bounded_original_syntax() {
        for (prefix, suffix, attached) in [
            ("/** @type {number} */\n", "", true),
            ("const marker=1;/** @type {number} */ ", "", false),
            ("const marker=1;\n/** @type {number} */ ", "", true),
            ("\u{feff}/** @type {number} */ ", "", true),
            ("/** @type {number} */\u{feff} ", "", true),
            ("{/** @type {number} */ ", "}", false),
            ("{\r\n/** @type {number} */ ", "}", true),
            ("const marker=1;\u{2028}/** @type {number} */ ", "", false),
            (
                "// \u{03c0}\u{1f30d}\r\n/** ordinary */\r\n/* retained */\r\n/** @type {number} */ ",
                "",
                true,
            ),
            ("/* ordinary */ ", "", false),
        ] {
            let source = format!("{prefix}const result=helper();{suffix}");
            let d = Deadline::new(10_000);
            let tree = parse("javascript", &source, &d).unwrap();
            let start = source.find("const result").unwrap();
            let statement =
                node_at(&tree, start, start + "const result=helper();".len(), &d).unwrap();
            let at = caller_start(&source, statement, &d).unwrap();
            assert_eq!(
                at,
                if attached {
                    source.find("/**").unwrap()
                } else {
                    start
                },
                "{source}"
            );
            assert!(source.is_char_boundary(at));
            assert!(caller_start(&source, statement, &Deadline::new(0)).is_err());
        }
        for prefix in [
            "/** ordinary */\n".repeat(65),
            format!("{}\n/** @type {{number}} */\n", " ".repeat(200_001)),
        ] {
            let source = format!("{prefix}const result=helper();");
            let d = Deadline::new(10_000);
            let tree = parse("javascript", &source, &d).unwrap();
            let start = source.find("const result").unwrap();
            let statement = node_at(&tree, start, source.len(), &d).unwrap();
            assert!(caller_start(&source, statement, &d).is_err());
        }
    }
    #[test]
    fn invocation_identifiers_refuse_wrappers_and_unicode_escapes_without_execution() {
        for expression in [
            "(eval)('typeof helper')",
            "eval!('typeof helper')",
            "(eval as (s:string)=>any)('typeof helper')",
            r"(\u0065val)('typeof helper')",
            r"\u0061rguments[0]",
        ] {
            let source = format!(
                "function helper(n:number){{return {expression};}}export const answer=helper(2);"
            );
            let d = Deadline::new(10_000);
            let tree = parse("typescript", &source, &d).unwrap();
            assert!(
                scope_forms(tree.root_node().named_child(0).unwrap(), &source, &d).is_err(),
                "{expression}"
            );
        }
    }
    #[test]
    fn owned_frame_execution_keeps_finally_scope_nested_returns_and_argument_comments() {
        let cases = [
            "const events=[];let n=9;function arg(x){events.push(x);return x;}function helper(n,m){events.push(n,m);n++;return [n,m];}const result=helper(arg(n),arg(2));globalThis.observed={result,events,n};",
            "let n=1;const events=[];function helper(n){events.push(n);return ()=>n;}const result=helper((events.push('first'),n),(events.push('extra'),n=3));globalThis.observed={value:result(),events,n};",
            "const events=[];function helper(n){try{events.push('try');return n+1;}finally{events.push('finally');return n+2;}}const result=helper(3);globalThis.observed={result,events};",
            "let n=1;function helper(x){try{return n;}finally{n=7;}}const result=helper(3);globalThis.observed={result,n};",
            "function helper(){exit:while(true){try{return 1;}finally{break exit;}}}const result=helper();globalThis.observed={result,isUndefined:result===undefined};",
            "function helper(){for(let n=0;n<2;n++){try{return n;}finally{continue;}}}const result=helper();globalThis.observed={result,isUndefined:result===undefined};",
            "function helper(n){const inner=()=>{return n+1;};return inner();}const result=helper(3);globalThis.observed=result;",
            "const events=[];function helper(n){if(n){events.push(n);return n;}events.push(0);}const a=helper(2);const b=helper(0);globalThis.observed={a,b,events};",
            "const events=[];function helper(n){return /* begin */ events.push(n),n+2 /* end */;}const result=helper(/* cafÃƒÆ’Ã†â€™Ãƒâ€šÃ‚Â© */3,/* extra */4,/* trailing */);globalThis.observed={result,events};",
            "const events=[];function helper(){events.push('called');return 7;}function outer(flag){if(flag)return helper(/* empty */);return 2;}globalThis.observed=[outer(false),outer(true),events];",
            "function helper(n){var x=n+1;return x;}const result=helper(2);globalThis.observed=result;",
            "function helper(){var before=typeof x;var x=1;return [before,x];}const result=helper();globalThis.observed=result;",
            "function helper(n){var n=n+1;return n;}const result=helper(2);globalThis.observed=result;",
            "function helper(n=1,f=()=>n){var n=2;return [n,f()];}const result=helper();globalThis.observed=result;",
            "function helper(){const fs=[];for(var i=0;i<3;i++)fs.push(()=>i);return fs.map(f=>f());}const result=helper();globalThis.observed=result;",
            "function helper(){const fs=[];for(var [n] of [[1],[2]])fs.push(()=>n);return fs.map(f=>f());}const result=helper();globalThis.observed=result;",
            "function helper(){const fs=[];for(var n in {a:1,b:2})fs.push(()=>n);return fs.map(f=>f());}const result=helper();globalThis.observed=result;",
            "function helper(){var {x:n=3,...rest}={y:4};return [n,rest];}const result=helper();globalThis.observed=result;",
            "function helper(){try{throw 1;}catch(e){var e=2;}return e;}const result=helper();globalThis.observed={result,empty:result===undefined};",
            "function helper(){var f;function f(){return 1;}return f();}const result=helper();globalThis.observed=result;",
            "function helper(){var f=()=>1;return f.name;}const result=helper();globalThis.observed=result;",
        ];
        for source in cases {
            let d = Deadline::new(30_000);
            let tree = parse("javascript", source, &d).unwrap();
            let mut cursor = tree.root_node().walk();
            let declaration = tree
                .root_node()
                .named_children(&mut cursor)
                .find(|n| {
                    n.kind() == "function_declaration"
                        && n.child_by_field_name("name")
                            .is_some_and(|v| v.utf8_text(source.as_bytes()) == Ok("helper"))
                })
                .unwrap();
            let mut stack = vec![tree.root_node()];
            let mut calls = vec![];
            while let Some(n) = stack.pop() {
                if n.id() == declaration.id() {
                    continue;
                }
                if n.kind() == "call_expression"
                    && n.child_by_field_name("function")
                        .is_some_and(|n| n.utf8_text(source.as_bytes()) == Ok("helper"))
                {
                    calls.push(n);
                }
                stack.extend(children(n));
            }
            calls.sort_by_key(Node::start_byte);
            let mut serial = 1;
            let mut edits = vec![(
                declaration.start_byte(),
                declaration.end_byte(),
                String::new(),
            )];
            for call in calls {
                let statement = statement_for(call).unwrap();
                let mut fresh = || {
                    let v = format!("__ci_inline_{serial}");
                    serial += 1;
                    v
                };
                let mut names = Names {
                    do_first: None,
                    return_type: vec![],
                    argument_capture: vec![],
                    var_parameters: vec![],
                    var_seeds: vec![],
                    result: fresh(),
                    exit: fresh(),
                    arguments: children(call.child_by_field_name("arguments").unwrap())
                        .iter()
                        .map(|_| fresh())
                        .collect(),
                };
                names.var_parameters = (0..variables::analyze(declaration, source, &d)
                    .unwrap()
                    .copies())
                    .map(|_| fresh())
                    .collect();
                names.return_type = (0..2).map(|_| fresh()).collect();
                if !names.arguments.is_empty() {
                    names.argument_capture = (0..7).map(|_| fresh()).collect();
                }
                let argument_docs = if names.arguments.is_empty() {
                    vec![]
                } else {
                    children(declaration.child_by_field_name("parameters").unwrap())
                        .iter()
                        .map(|_| arguments::Doc::default())
                        .collect()
                };
                let frame = Frame {
                    literal_box: false,
                    doc_pieces: None,
                    argument_docs,
                    argument_templates: vec![],
                    direct_arguments: false,
                    predicate_names: vec![],
                    inferred_predicate_parameter: None,
                    dense_arguments: false,
                    statement_start: statement.start_byte(),
                    statement_end: statement.end_byte(),
                    call_start: call.start_byte(),
                    call_end: call.end_byte(),
                    names,
                    result_type: None,
                    var_types: vec![],
                    var_seed_groups: vec![],
                };
                let out = render(&tree, source, "javascript", declaration, &frame, &d).unwrap();
                for copy in &out.copies {
                    assert_eq!(
                        &source[copy.original_start..copy.original_end],
                        &out.text[copy.output_start..copy.output_end]
                    );
                }
                edits.push((statement.start_byte(), statement.end_byte(), out.text));
            }
            edits.sort_by_key(|e| e.0);
            assert!(edits.windows(2).all(|p| p[0].1 <= p[1].0));
            let mut after = source.to_owned();
            for (start, end, text) in edits.into_iter().rev() {
                after.replace_range(start..end, &text);
            }
            parse("javascript", &after, &d).unwrap();
            let node = std::env::split_paths(&std::env::var_os("PATH").unwrap())
                .map(|p| p.join(if cfg!(windows) { "node.exe" } else { "node" }))
                .find(|p| p.is_file())
                .unwrap();
            let script = format!(
                "const vm=require('node:vm');function observe(s){{const c={{}};vm.runInNewContext(s,c,{{timeout:1000}});return JSON.stringify(c.observed);}}if(observe({})!==observe({}))throw Error('owned frame changed observations');",
                serde_json::to_string(source).unwrap(),
                serde_json::to_string(&after).unwrap()
            );
            let output = std::process::Command::new(node)
                .args(["-e", &script])
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{}\n{after}",
                String::from_utf8_lossy(&output.stderr)
            );
            for comment in [
                "/* cafÃƒÆ’Ã†â€™Ãƒâ€šÃ‚Â© */",
                "/* extra */",
                "/* trailing */",
                "/* empty */",
            ] {
                if source.contains(comment) {
                    assert!(after.contains(comment));
                }
            }
        }
    }
    #[test]
    fn frame_reconstruction_keeps_argument_order_parameter_scope_and_finally_returns() {
        let source = "function helper(n:number,m:number){try{return n+m;}finally{n++;}}\nconst answer=helper(n++,2);";
        let d = Deadline::new(30_000);
        let tree = parse("typescript", source, &d).unwrap();
        let declaration = tree.root_node().named_child(0).unwrap();
        let call_start = source.rfind("helper(").unwrap();
        let statement_start = source.find("const answer").unwrap();
        let frame = Frame {
            literal_box: false,
            doc_pieces: None,
            argument_docs: vec![],
            argument_templates: vec![],
            direct_arguments: false,
            predicate_names: vec![],
            inferred_predicate_parameter: None,
            dense_arguments: false,
            statement_start,
            statement_end: source.len(),
            call_start,
            call_end: source.len() - 1,
            names: Names {
                do_first: None,
                return_type: vec!["__ci_inline_5".into(), "__ci_inline_6".into()],
                argument_capture: (7..=13).map(|n| format!("__ci_inline_{n}")).collect(),
                var_parameters: vec![],
                var_seeds: vec![],
                result: "__ci_inline_1".into(),
                exit: "__ci_inline_2".into(),
                arguments: vec!["__ci_inline_3".into(), "__ci_inline_4".into()],
            },
            result_type: None,
            var_types: vec![],
            var_seed_groups: vec![],
        };
        let out = render(&tree, source, "typescript", declaration, &frame, &d).unwrap();
        assert!(
            out.text.find("__ci_inline_9(n++,2)").unwrap()
                < out.text.find("let n:number = __ci_inline_3").unwrap()
        );
        assert!(out.text.contains("break __ci_inline_2"));
        assert!(out.text.contains("finally{n++;}"));
        parse("typescript", &out.text, &d).unwrap();
        for copy in out.copies {
            assert_eq!(
                &source[copy.original_start..copy.original_end],
                &out.text[copy.output_start..copy.output_end]
            );
            assert!(matches!(
                copy.role,
                "argument"
                    | "argument_trivia"
                    | "parameter"
                    | "body"
                    | "caller"
                    | "return_type"
                    | "return_witness"
                    | "argument_signature"
            ));
        }
        let mut bad = frame.clone();
        bad.names.arguments[0] = bad.names.result.clone();
        assert!(render(&tree, source, "typescript", declaration, &bad, &d).is_err());
        bad = frame.clone();
        bad.call_start += 1;
        assert!(render(&tree, source, "typescript", declaration, &bad, &d).is_err());
        assert!(
            render(
                &tree,
                source,
                "typescript",
                declaration,
                &frame,
                &Deadline::new(0)
            )
            .is_err()
        );
    }
}
