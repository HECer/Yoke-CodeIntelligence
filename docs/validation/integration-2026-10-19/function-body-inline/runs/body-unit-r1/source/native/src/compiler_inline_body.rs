//! Original-byte reconstruction of function-body call frames.
//! The controller and compiler witnesses admit scope/binding authority separately.
use super::*;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Names {
    result: String,
    exit: String,
    arguments: Vec<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Frame {
    statement_start: usize,
    statement_end: usize,
    call_start: usize,
    call_end: usize,
    names: Names,
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
    ensure!(
        matches!(parent.kind(), "expression_statement" | "return_statement")
            && children(parent)
                .first()
                .is_some_and(|n| n.id() == value.id()),
        "Function-body expression context needs sequencing"
    );
    Ok(parent)
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
                "variable_declaration"
                    | "with_statement"
                    | "await_expression"
                    | "yield_expression"
                    | "this"
                    | "super"
                    | "meta_property"
            ),
            "Function-body scope form needs additional admission"
        );
        if n.kind() == "identifier" {
            ensure!(
                n.utf8_text(source.as_bytes())? != "arguments",
                "Function-body arguments requires its original call frame"
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
fn returns<'a>(body: Node<'a>, source: &str, d: &Deadline) -> Result<Vec<Node<'a>>> {
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
/// This renderer never trusts a supplied replacement string. Every original
/// expression, comment and body fragment is sliced from the checked source.
pub(super) fn render(
    tree: &Tree,
    source: &str,
    declaration: Node<'_>,
    frame: &Frame,
    d: &Deadline,
) -> Result<Rendered> {
    d.check()?;
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
    scope_forms(body, source, d)?;
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
    let args = children(
        call.child_by_field_name("arguments")
            .context("Function-body arguments absent")?,
    );
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
    let mut bindings = vec![];
    for p in parameter_nodes {
        d.check()?;
        let id = if p.kind() == "identifier" {
            p
        } else {
            ensure!(
                p.kind() == "required_parameter" && p.child_by_field_name("value").is_none(),
                "Function-body parameter form unavailable"
            );
            p.child_by_field_name("pattern")
                .context("Function-body parameter name absent")?
        };
        ensure!(
            id.kind() == "identifier",
            "Function-body parameter pattern unavailable"
        );
        bindings.push((p, id));
    }
    let names: Vec<_> = std::iter::once(&frame.names.result)
        .chain(std::iter::once(&frame.names.exit))
        .chain(&frame.names.arguments)
        .collect();
    ensure!(
        names.iter().all(|n| fresh_name(n))
            && names.iter().collect::<BTreeSet<_>>().len() == names.len(),
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
            let text = n.utf8_text(source.as_bytes())?;
            ensure!(
                !names.iter().any(|name| name.as_str() == text),
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
    out.generated(&format!("let {};{line}{{{line}", frame.names.result))?;
    for (arg, generated) in args.iter().zip(&frame.names.arguments) {
        out.generated(&format!("const {generated} = ("))?;
        out.original(source, arg.start_byte(), arg.end_byte(), "argument")?;
        out.generated(&format!(");{line}"))?;
    }
    out.generated(&format!("{{{line}"))?;
    for (i, (parameter, _)) in bindings.iter().enumerate() {
        out.generated("let ")?;
        out.original(
            source,
            parameter.start_byte(),
            parameter.end_byte(),
            "parameter",
        )?;
        out.generated(&format!(
            " = {};{line}",
            frame
                .names
                .arguments
                .get(i)
                .map(String::as_str)
                .unwrap_or("void 0")
        ))?;
    }
    out.generated(&format!("{}: ", frame.names.exit))?;
    let mut cursor = body.start_byte();
    for ret in returns(body, source, d)? {
        out.original(source, cursor, ret.start_byte(), "body")?;
        let values = children(ret);
        ensure!(
            values.len() <= 1,
            "Function-body return expression multiplicity"
        );
        out.generated(&format!("{{ {} = ", frame.names.result))?;
        let tail_start = if let Some(value) = values.first() {
            out.generated("(")?;
            out.original(source, ret.start_byte() + 6, value.end_byte(), "body")?;
            out.generated(")")?;
            value.end_byte()
        } else {
            out.generated("void 0")?;
            ret.start_byte() + 6
        };
        out.original(source, tail_start, ret.end_byte(), "body")?;
        out.generated(&format!("; break {}; }}", frame.names.exit))?;
        cursor = ret.end_byte();
    }
    out.original(source, cursor, body.end_byte(), "body")?;
    out.generated(&format!("{line}}}{line}}}{line}"))?;
    out.original(source, statement.start_byte(), call.start_byte(), "caller")?;
    out.generated(&frame.names.result)?;
    out.original(source, call.end_byte(), statement.end_byte(), "caller")?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn frame_reconstruction_keeps_argument_order_parameter_scope_and_finally_returns() {
        let source = "function helper(n:number,m:number){try{return n+m;}finally{n++;}}\nconst answer=helper(n++,2);";
        let d = Deadline::new(30_000);
        let tree = parse("typescript", source, &d).unwrap();
        let declaration = tree.root_node().named_child(0).unwrap();
        let call_start = source.rfind("helper(").unwrap();
        let statement_start = source.find("const answer").unwrap();
        let frame = Frame {
            statement_start,
            statement_end: source.len(),
            call_start,
            call_end: source.len() - 1,
            names: Names {
                result: "__ci_inline_1".into(),
                exit: "__ci_inline_2".into(),
                arguments: vec!["__ci_inline_3".into(), "__ci_inline_4".into()],
            },
        };
        let out = render(&tree, source, declaration, &frame, &d).unwrap();
        assert!(
            out.text.find("const __ci_inline_3 = (n++)").unwrap()
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
                "argument" | "parameter" | "body" | "caller"
            ));
        }
        let mut bad = frame.clone();
        bad.names.arguments[0] = bad.names.result.clone();
        assert!(render(&tree, source, declaration, &bad, &d).is_err());
        bad = frame.clone();
        bad.call_start += 1;
        assert!(render(&tree, source, declaration, &bad, &d).is_err());
        assert!(render(&tree, source, declaration, &frame, &Deadline::new(0)).is_err());
    }
}
