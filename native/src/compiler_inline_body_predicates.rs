//! Original predicate signatures stay inert; a fixed value relay preserves flow.
use super::*;
#[derive(Clone, Copy, Debug)]
pub(super) struct Predicate {
    pub assertion: bool,
    parameter: usize,
}
pub(super) fn classify(
    declaration: Node<'_>,
    source: &str,
    d: &Deadline,
) -> Result<Option<Predicate>> {
    d.check()?;
    let Some(annotation) = declaration.child_by_field_name("return_type") else {
        return Ok(None);
    };
    let assertion = match annotation.kind() {
        "type_predicate_annotation" => false,
        "asserts_annotation" => true,
        _ => return Ok(None),
    };
    let mut node = annotation;
    let mut depth = 0;
    while matches!(
        node.kind(),
        "type_predicate_annotation" | "asserts_annotation" | "asserts"
    ) {
        d.check()?;
        depth += 1;
        ensure!(depth <= 4, "Predicate annotation depth exceeded");
        let named = children(node);
        ensure!(named.len() == 1, "Predicate annotation shape differs");
        node = named[0];
    }
    let name = if node.kind() == "type_predicate" {
        node.child_by_field_name("name")
            .context("Predicate target absent")?
    } else {
        node
    };
    ensure!(
        name.kind() == "identifier",
        "Predicate this/nonidentifier target unavailable"
    );
    let name = parameters::identifier_name(name.utf8_text(source.as_bytes())?, d)?;
    let formal = declaration
        .child_by_field_name("parameters")
        .context("Predicate parameters absent")?;
    let mut matched = None;
    for (index, p) in children(formal).into_iter().enumerate() {
        d.check()?;
        let pattern = p.child_by_field_name("pattern").unwrap_or(p);
        if pattern.kind() == "identifier"
            && parameters::identifier_name(pattern.utf8_text(source.as_bytes())?, d)? == name
        {
            ensure!(
                matched.replace(index).is_none(),
                "Predicate parameter is ambiguous"
            );
        }
    }
    Ok(Some(Predicate {
        assertion,
        parameter: matched.context("Predicate parameter binding absent")?,
    }))
}
#[allow(clippy::too_many_arguments)]
pub(super) fn begin(
    out: &mut Rendered,
    source: &str,
    declaration: Node<'_>,
    frame: &Frame,
    language: &str,
    line: &str,
    d: &Deadline,
) -> Result<Option<Predicate>> {
    let explicit = classify(declaration, source, d)?;
    let predicate = match (explicit, frame.inferred_predicate_parameter) {
        (Some(predicate), None) => Some(predicate),
        (None, Some(parameter)) => {
            ensure!(
                declaration.child_by_field_name("return_type").is_none()
                    && frame.result_type.as_deref() == Some("boolean"),
                "Inferred predicate lacks its source boolean witness"
            );
            let formal = declaration
                .child_by_field_name("parameters")
                .context("Inferred predicate parameters absent")?;
            let parameters = children(formal);
            let parameter_node = parameters
                .get(parameter)
                .context("Inferred predicate parameter outside source")?;
            ensure!(
                parameter_node
                    .child_by_field_name("pattern")
                    .unwrap_or(*parameter_node)
                    .kind()
                    == "identifier",
                "Inferred predicate parameter is not an identifier"
            );
            Some(Predicate {
                assertion: false,
                parameter,
            })
        }
        (None, None) => None,
        (Some(_), Some(_)) => anyhow::bail!("Explicit predicate cannot adopt an inferred target"),
    };
    ensure!(
        frame.predicate_names.len() == if predicate.is_some() { 3 } else { 0 },
        "Predicate relay name shape differs"
    );
    if predicate.is_none() {
        return Ok(None);
    }
    ensure!(
        matches!(language, "typescript" | "tsx")
            && (frame.result_type.is_none() || frame.inferred_predicate_parameter.is_some())
            && frame.names.return_type.is_empty(),
        "Predicate result witness differs"
    );
    let names = &frame.predicate_names;
    out.generated(&format!(
        "if (void 0 !== void 0) {{{line}var {} = ",
        names[0]
    ))?;
    emit_documentation(
        out,
        source,
        &documentation_pieces(source, declaration, frame, d)?,
        "predicate_signature",
        line,
    )?;
    out.generated("function")?;
    let formal = declaration
        .child_by_field_name("parameters")
        .context("Predicate signature absent")?;
    out.original(
        source,
        formal.start_byte(),
        declaration.end_byte(),
        "predicate_signature",
    )?;
    out.generated(&format!(
        ";void {};{line}}}{line}let {}: typeof {};{line}",
        names[0], names[1], names[0]
    ))?;
    Ok(predicate)
}
pub(super) fn finish(out: &mut Rendered, frame: &Frame, line: &str) -> Result<()> {
    if frame.predicate_names.is_empty() {
        return Ok(());
    }
    let n = &frame.predicate_names;
    out.generated(&format!(
        "{} = ((...{}: unknown[])=>{}) as unknown as typeof {};{line}",
        n[1], n[2], frame.names.result, n[0]
    ))
}
pub(super) fn result(
    out: &mut Rendered,
    source: &str,
    args: &[Node<'_>],
    frame: &Frame,
    predicate: Predicate,
    d: &Deadline,
) -> Result<()> {
    out.generated(&format!("{}(", frame.predicate_names[1]))?;
    for (index, arg) in args.iter().copied().enumerate() {
        d.check()?;
        if index > 0 {
            out.generated(",")?;
        }
        let mut node = arg;
        let mut depth = 0;
        while node.kind() == "parenthesized_expression" {
            depth += 1;
            ensure!(depth <= 2000, "Predicate argument wrapper capacity");
            let named = children(node);
            ensure!(named.len() == 1, "Predicate argument wrapper differs");
            node = named[0];
        }
        if index == predicate.parameter && node.kind() == "identifier" {
            // Admission separately proves a captured nonambient lexical binding.
            out.original(
                source,
                arg.start_byte(),
                arg.end_byte(),
                "predicate_argument",
            )?;
        } else {
            // Every real argument was already evaluated before parameter setup.
            // The relay ignores its arguments; these inert placeholders do not
            // reread a getter, call, iterator or a removed parameter environment.
            out.generated("void 0 as never")?;
        }
    }
    out.generated(")")
}

/// A repeated type-flow reference must be an ordinary captured lexical read.
/// Property/global/ambient/export/namespace access cannot use this privilege.
pub(super) fn lexical_target(
    tree: &Tree,
    start: usize,
    end: usize,
    source: &str,
    d: &Deadline,
) -> Result<()> {
    let name = node_at(tree, start, end, d)?;
    ensure!(
        matches!(
            name.kind(),
            "identifier" | "shorthand_property_identifier_pattern"
        ),
        "Predicate relay target is not a lexical name"
    );
    let mut at = name.parent();
    let mut found = None;
    let mut depth = 0;
    while let Some(node) = at {
        d.check()?;
        depth += 1;
        ensure!(depth <= 2000, "Predicate lexical scope capacity");
        if matches!(
            node.kind(),
            "variable_declarator" | "required_parameter" | "optional_parameter"
        ) {
            let pattern = node
                .child_by_field_name("name")
                .or_else(|| node.child_by_field_name("pattern"))
                .context("Predicate lexical pattern absent")?;
            ensure!(
                variables::bound(pattern, d)?
                    .iter()
                    .any(|id| id.start_byte() == start && id.end_byte() == end),
                "Predicate relay target is outside declaration bindings"
            );
            found = Some(node);
            break;
        }
        ensure!(
            !matches!(node.kind(), "program" | "statement_block" | "class_body"),
            "Predicate relay target is not a variable/parameter binding"
        );
        at = node.parent();
    }
    let binding = found.context("Predicate lexical declaration absent")?;
    let _ = source;
    let mut at = binding.parent();
    let mut depth = 0;
    while let Some(node) = at {
        d.check()?;
        depth += 1;
        ensure!(depth <= 2000, "Predicate owning scope capacity");
        ensure!(
            !matches!(
                node.kind(),
                "export_statement" | "ambient_declaration" | "internal_module" | "module"
            ),
            "Predicate relay target has ambient/property exposure"
        );
        if matches!(
            node.kind(),
            "function_declaration"
                | "function_expression"
                | "arrow_function"
                | "method_definition"
                | "generator_function"
                | "generator_function_declaration"
        ) {
            ensure!(
                node.child_by_field_name("body").is_some(),
                "Predicate parameter belongs to a type-only signature"
            );
            return Ok(());
        }
        if node.kind() == "program" {
            return Ok(());
        }
        at = node.parent();
    }
    anyhow::bail!("Predicate relay target has no lexical owner")
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn predicate_source_classification_and_lexical_references_keep_closed_bounds() {
        let d = Deadline::new(10000);
        for (source, expected) in [
            (
                "function f(v:unknown):v is string{return true;}",
                Some((false, 0)),
            ),
            (
                "function f(x:number,v:unknown):asserts v is string{}",
                Some((true, 1)),
            ),
            ("function f(v:unknown):asserts v{}", Some((true, 0))),
            ("function f(v:unknown):boolean{return true;}", None),
            ("function f(v:unknown){return true;}", None),
            (
                "function f(val\\u0075e:unknown):value is string{return true;}",
                Some((false, 0)),
            ),
        ] {
            let tree = parse("typescript", source, &d).unwrap();
            let function = tree.root_node().named_child(0).unwrap();
            assert_eq!(
                classify(function, source, &d)
                    .unwrap()
                    .map(|p| (p.assertion, p.parameter)),
                expected
            );
            assert!(classify(function, source, &Deadline::new(0)).is_err());
        }
        for (source, allowed) in [
            ("const owned=1; export const answer=owned;", true),
            ("export const owned=1; export const answer=owned;", false),
            (
                "declare const owned:unknown; export const answer=owned;",
                false,
            ),
            ("namespace N { export let owned=1; }", false),
            ("function run(owned:unknown){return owned;}", true),
            ("export function run(owned:unknown){return owned;}", true),
            ("function run(){let owned:unknown;return owned;}", true),
            ("const box={get owned(){return 1;}};", false),
            (
                "import {owned} from './source'; export const answer=owned;",
                false,
            ),
            ("let {owned}={owned:1}; export const answer=owned;", true),
        ] {
            let tree = parse("typescript", source, &d).unwrap();
            let start = source.find("owned").unwrap();
            assert_eq!(
                lexical_target(&tree, start, start + 5, source, &d).is_ok(),
                allowed,
                "{source}"
            );
            assert!(lexical_target(&tree, start, start + 5, source, &Deadline::new(0)).is_err());
        }
    }
    #[test]
    fn predicate_relay_reconstruction_rejects_forged_shapes_bindings_and_inference_targets() {
        let d = Deadline::new(10000);
        let source = "function helper(value:unknown):value is string{return typeof value==='string';}\nconst input:unknown='owned';\nconst answer=helper(input);";
        let tree = parse("typescript", source, &d).unwrap();
        let declaration = tree.root_node().named_child(0).unwrap();
        let statement = tree.root_node().named_child(2).unwrap();
        let call_start = source.rfind("helper(").unwrap();
        let frame = Frame {
            statement_start: statement.start_byte(),
            statement_end: statement.end_byte(),
            call_start,
            call_end: source.rfind(')').unwrap() + 1,
            names: Names {
                result: "__ci_inline_1".into(),
                exit: "__ci_inline_2".into(),
                arguments: vec!["__ci_inline_3".into()],
                do_first: None,
                var_parameters: vec![],
                var_seeds: vec![],
                return_type: vec![],
                argument_capture: vec![],
            },
            result_type: None,
            var_types: vec![],
            var_seed_groups: vec![],
            literal_box: false,
            doc_pieces: None,
            argument_docs: vec![],
            argument_templates: vec![],
            direct_arguments: true,
            predicate_names: vec![
                "__ci_inline_4".into(),
                "__ci_inline_5".into(),
                "__ci_inline_6".into(),
            ],
            inferred_predicate_parameter: None,
            dense_arguments: false,
        };
        let output = render(&tree, source, "typescript", declaration, &frame, &d).unwrap();
        assert!(
            output
                .text
                .contains("let __ci_inline_1: (boolean) | undefined")
        );
        assert!(output.text.contains("__ci_inline_5(input)"));
        assert_eq!(
            output
                .copies
                .iter()
                .filter(|c| c.role == "predicate_argument")
                .count(),
            1
        );
        for names in [
            vec![],
            vec!["__ci_inline_4".into()],
            vec![
                "__ci_inline_4".into(),
                "__ci_inline_3".into(),
                "__ci_inline_6".into(),
            ],
            vec![
                "__ci_inline_4".into(),
                "console".into(),
                "__ci_inline_6".into(),
            ],
        ] {
            let mut bad = frame.clone();
            bad.predicate_names = names;
            assert!(render(&tree, source, "typescript", declaration, &bad, &d).is_err());
        }
        let mut bad = frame.clone();
        bad.inferred_predicate_parameter = Some(0);
        assert!(render(&tree, source, "typescript", declaration, &bad, &d).is_err());
        let mut bad = frame.clone();
        bad.result_type = Some("boolean".into());
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
        let inferred = source.replace(":value is string", "");
        let tree = parse("typescript", &inferred, &d).unwrap();
        let declaration = tree.root_node().named_child(0).unwrap();
        let statement = tree.root_node().named_child(2).unwrap();
        let mut frame = frame;
        frame.statement_start = statement.start_byte();
        frame.statement_end = statement.end_byte();
        frame.call_start = inferred.rfind("helper(").unwrap();
        frame.call_end = inferred.rfind(')').unwrap() + 1;
        frame.result_type = Some("boolean".into());
        frame.inferred_predicate_parameter = Some(0);
        assert!(render(&tree, &inferred, "typescript", declaration, &frame, &d).is_ok());
        frame.inferred_predicate_parameter = Some(64);
        assert!(render(&tree, &inferred, "typescript", declaration, &frame, &d).is_err());
    }
}
