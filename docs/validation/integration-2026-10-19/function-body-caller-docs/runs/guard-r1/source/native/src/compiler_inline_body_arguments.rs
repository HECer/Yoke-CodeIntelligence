//! Source-derived callable argument context; only a fixed rest collector executes.
use super::*;
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Doc {
    #[serde(default)]
    piece: Option<DocPiece>,
    #[serde(default)]
    optional: bool,
}
struct Parameter<'a> {
    pattern: Node<'a>,
    annotation: Option<Node<'a>>,
    rest: bool,
    bindings: Vec<Node<'a>>,
}
fn parameters<'a>(formal: Node<'a>, d: &Deadline) -> Result<Vec<Parameter<'a>>> {
    let mut result = vec![];
    for p in children(formal) {
        d.check()?;
        let mut pattern = match p.kind() {
            "required_parameter" | "optional_parameter" => p.child_by_field_name("pattern"),
            "assignment_pattern" => p.child_by_field_name("left"),
            _ => Some(p),
        }
        .context("Argument signature parameter absent")?;
        let rest = pattern.kind() == "rest_pattern";
        if rest {
            let values = children(pattern);
            ensure!(values.len() == 1, "Argument rest binding multiplicity");
            pattern = values[0];
        }
        let bindings = variables::bound(pattern, d)?;
        result.push(Parameter {
            pattern,
            annotation: p.child_by_field_name("type"),
            rest,
            bindings,
        });
        ensure!(
            result.len() <= 64,
            "Argument signature parameter capacity exceeded"
        );
    }
    ensure!(
        result.iter().map(|p| p.bindings.len()).sum::<usize>() <= 2000,
        "Argument signature binding capacity exceeded"
    );
    Ok(result)
}
fn checked_piece(source: &str, docs: &[Node<'_>], piece: &DocPiece, template: bool) -> Result<()> {
    let text = crate::paths::utf8_range(source, piece.start_byte, piece.end_byte)?;
    ensure!(
        !text.is_empty()
            && text.len() <= 16_000
            && !text.contains("*/")
            && piece.tag == template
            && docs
                .iter()
                .any(|doc| piece.start_byte >= doc.start_byte() + 3
                    && piece.end_byte <= doc.end_byte() - 2),
        "Argument documentation piece outside original comment"
    );
    if template {
        ensure!(
            text.strip_prefix("@template").is_some_and(|s| s
                .chars()
                .next()
                .is_some_and(|c| c.is_whitespace() || matches!(c, '{' | '['))),
            "Argument signature documentation is not a template tag"
        );
    }
    Ok(())
}
fn fallback(names: &[String], index: usize, rest: bool) -> String {
    let tuple = format!(
        "((typeof {}) extends (...{}: infer {}) => unknown ? {} : never)",
        names[0], names[5], names[6], names[6]
    );
    if rest {
        format!(
            "({tuple} extends [{}...infer {}] ? {} : never)",
            "unknown,".repeat(index),
            names[6],
            names[6]
        )
    } else {
        format!("{tuple}[{index}]")
    }
}
#[allow(clippy::too_many_arguments)]
fn parameter_type(
    out: &mut Rendered,
    source: &str,
    p: &Parameter<'_>,
    doc: Option<&Doc>,
    names: &[String],
    index: usize,
    js: bool,
) -> Result<()> {
    if js {
        if let Some(piece) = doc.and_then(|doc| doc.piece.as_ref()) {
            let text = crate::paths::utf8_range(source, piece.start_byte, piece.end_byte)?;
            if p.rest && text.starts_with("...") {
                out.generated("((")?;
                out.original(
                    source,
                    piece.start_byte + 3,
                    piece.end_byte,
                    "argument_signature",
                )?;
                out.generated(")[])")?;
            } else {
                out.original(
                    source,
                    piece.start_byte,
                    piece.end_byte,
                    "argument_signature",
                )?;
            }
        } else {
            out.generated(&fallback(names, index, p.rest))?;
        }
    } else if p.pattern.kind() == "identifier" {
        out.generated("typeof ")?;
        out.original(
            source,
            p.pattern.start_byte(),
            p.pattern.end_byte(),
            "argument_signature",
        )?;
    } else if let Some(ty) = p.annotation {
        ensure!(
            source.as_bytes().get(ty.start_byte()) == Some(&b':'),
            "Argument type annotation marker absent"
        );
        out.generated("(")?;
        out.original(
            source,
            ty.start_byte() + 1,
            ty.end_byte(),
            "argument_signature",
        )?;
        out.generated(")")?;
    } else {
        out.generated(&fallback(names, index, p.rest))?;
    }
    Ok(())
}
#[allow(clippy::too_many_arguments)]
pub(super) fn emit(
    out: &mut Rendered,
    source: &str,
    declaration: Node<'_>,
    list: Node<'_>,
    frame: &Frame,
    language: &str,
    line: &str,
    d: &Deadline,
) -> Result<()> {
    d.check()?;
    let args = children(list);
    let names = &frame.names.argument_capture;
    if frame.direct_arguments {
        ensure!(
            !args.is_empty()
                && names.is_empty()
                && frame.argument_docs.is_empty()
                && frame.argument_templates.is_empty(),
            "Direct argument recipe has capture metadata"
        );
        let mut cursor = list.start_byte() + 1;
        let mut c = list.walk();
        let commas: Vec<_> = list.children(&mut c).filter(|n| n.kind() == ",").collect();
        let trivia = |out: &mut Rendered, start: usize, end: usize| -> Result<()> {
            let mut at = start;
            for comma in &commas {
                if comma.start_byte() >= at && comma.end_byte() <= end {
                    out.original(source, at, comma.start_byte(), "argument_trivia")?;
                    at = comma.end_byte();
                }
            }
            out.original(source, at, end, "argument_trivia")
        };
        for (arg, name) in args.iter().zip(&frame.names.arguments) {
            d.check()?;
            out.generated(&format!(
                "const {name} = {}",
                if anonymous_return_value(*arg, d)? {
                    "(void 0,"
                } else {
                    "("
                }
            ))?;
            trivia(out, cursor, arg.start_byte())?;
            out.original(source, arg.start_byte(), arg.end_byte(), "argument")?;
            out.generated(&format!(");{line}"))?;
            cursor = arg.end_byte();
        }
        trivia(out, cursor, list.end_byte() - 1)?;
        return out.generated(line);
    }

    if args.is_empty() {
        ensure!(
            names.is_empty()
                && frame.argument_docs.is_empty()
                && frame.argument_templates.is_empty(),
            "Empty argument capture has metadata"
        );
        out.original(
            source,
            list.start_byte() + 1,
            list.end_byte() - 1,
            "argument_trivia",
        )?;
        return out.generated(line);
    }
    ensure!(
        names.len() == 7,
        "Argument capture requires seven distinct fresh names"
    );
    let formal = declaration
        .child_by_field_name("parameters")
        .context("Argument signature parameters absent")?;
    let params = parameters(formal, d)?;
    let js = matches!(language, "javascript" | "jsx");
    ensure!(
        if js {
            frame.argument_docs.len() == params.len()
        } else {
            frame.argument_docs.is_empty() && frame.argument_templates.is_empty()
        },
        "Argument signature documentation shape mismatch"
    );
    let docs = return_docs(source, declaration, d)?;
    ensure!(
        frame.argument_templates.len() <= 64,
        "Argument template capacity exceeded"
    );
    let mut previous = 0;
    for piece in &frame.argument_templates {
        d.check()?;
        checked_piece(source, &docs, piece, true)?;
        ensure!(
            piece.start_byte >= previous,
            "Argument templates overlap or are unordered"
        );
        previous = piece.end_byte;
    }
    for doc in &frame.argument_docs {
        d.check()?;
        if let Some(piece) = &doc.piece {
            checked_piece(source, &docs, piece, false)?;
        }
    }
    out.generated(&format!(
        "if (void 0 !== void 0) {{{line}var {} = ",
        names[0]
    ))?;
    emit_documentation(
        out,
        source,
        &documentation_pieces(source, declaration, frame, d)?,
        "argument_signature",
        line,
    )?;
    out.generated("function")?;
    out.original(
        source,
        formal.start_byte(),
        declaration.end_byte(),
        "argument_signature",
    )?;
    out.generated(&format!(";void {};{line}var {} = ", names[0], names[1]))?;
    if js {
        out.generated("/**")?;
        for template in &frame.argument_templates {
            out.generated(line)?;
            out.original(
                source,
                template.start_byte,
                template.end_byte,
                "argument_signature",
            )?;
        }
        for (index, p) in params.iter().enumerate() {
            let doc = &frame.argument_docs[index];
            let name = if p.pattern.kind() == "identifier" {
                parameters::identifier_name(p.pattern.utf8_text(source.as_bytes())?, d)?
            } else {
                format!("__ci_inline_parameter_{index}")
            };
            out.generated(&format!("{line} * @param {{"))?;
            if let Some(piece) = &doc.piece {
                out.original(
                    source,
                    piece.start_byte,
                    piece.end_byte,
                    "argument_signature",
                )?;
            } else {
                out.generated(&fallback(names, index, p.rest))?;
            }
            out.generated(&format!(
                "}} {}{name}{}",
                if doc.optional { "[" } else { "" },
                if doc.optional { "]" } else { "" }
            ))?;
        }
        out.generated(&format!("{line} * @returns {{["))?;
    }
    if !js {
        out.generated("function")?;
        out.original(
            source,
            formal.start_byte(),
            formal.end_byte(),
            "argument_signature",
        )?;
        out.generated(": [")?;
    }
    for (index, p) in params.iter().enumerate() {
        d.check()?;
        if index > 0 {
            out.generated(",")?;
        }
        if p.rest {
            out.generated("...(")?;
        }
        parameter_type(
            out,
            source,
            p,
            frame.argument_docs.get(index),
            names,
            index,
            js,
        )?;
        if p.rest {
            out.generated(")")?;
        }
    }
    if !params.last().is_some_and(|p| p.rest) {
        if !params.is_empty() {
            out.generated(",")?;
        }
        out.generated("...unknown[]")?;
    }
    out.generated("]")?;
    if js {
        out.generated(&format!("}}{line} */{line}function"))?;
        out.original(
            source,
            formal.start_byte(),
            formal.end_byte(),
            "argument_signature",
        )?;
    }
    out.generated("{")?;
    for p in &params {
        for name in &p.bindings {
            d.check()?;
            out.generated("void ")?;
            out.original(
                source,
                name.start_byte(),
                name.end_byte(),
                "argument_signature",
            )?;
            out.generated(";")?;
        }
    }
    out.generated(&format!(
        "throw void 0;}};void {};{line}}}{line}const {} = ",
        names[1], names[2]
    ))?;
    if js {
        out.generated(&format!("/** @type {{typeof {}}} */ (/** @type {{unknown}} */ (/** @param {{unknown[]}} {} */ (...{})=>{}));{line}",names[1],names[3],names[3],names[3]))?;
    } else {
        out.generated(&format!(
            "((...{}: unknown[])=>{}) as unknown as typeof {};{line}",
            names[3], names[3], names[1]
        ))?;
    }
    out.generated(&format!("const {} = {}", names[4], names[2]))?;
    let mut cursor = list.start_byte();
    for arg in &args {
        d.check()?;
        out.original(source, cursor, arg.start_byte(), "argument_trivia")?;
        out.original(source, arg.start_byte(), arg.end_byte(), "argument")?;
        cursor = arg.end_byte();
    }
    out.original(source, cursor, list.end_byte(), "argument_trivia")?;
    out.generated(&format!(";{line}"))?;
    for (index, arg) in frame.names.arguments.iter().enumerate() {
        out.generated(&format!(
            "const {arg} = {}[{index}];void {arg};{line}",
            names[4]
        ))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn documentation_types_require_original_attached_utf8_comments_and_closed_template_tags() {
        let source = "// \u{03c0}\n/** @template T\n * @param {T} n */\nfunction helper(n){}";
        let d = Deadline::new(10_000);
        let tree = parse("javascript", source, &d).unwrap();
        let declaration = children(tree.root_node())[0];
        let docs = return_docs(source, declaration, &d).unwrap();
        let start = source.find("{T}").unwrap() + 1;
        let piece = DocPiece {
            start_byte: start,
            end_byte: start + 1,
            tag: false,
        };
        checked_piece(source, &docs, &piece, false).unwrap();
        for bad in [
            DocPiece {
                start_byte: 0,
                ..piece.clone()
            },
            DocPiece {
                end_byte: source.len() + 1,
                ..piece.clone()
            },
            DocPiece {
                end_byte: start,
                ..piece.clone()
            },
            DocPiece {
                tag: true,
                ..piece.clone()
            },
            DocPiece {
                start_byte: source.find('\u{03c0}').unwrap() + 1,
                ..piece.clone()
            },
            DocPiece {
                start_byte: docs[0].start_byte() + 3,
                end_byte: docs[0].end_byte(),
                tag: false,
            },
        ] {
            assert!(checked_piece(source, &docs, &bad, false).is_err());
        }
        let template = DocPiece {
            start_byte: source.find("@template").unwrap(),
            end_byte: source.find("\n * @param").unwrap(),
            tag: true,
        };
        checked_piece(source, &docs, &template, true).unwrap();
        assert!(checked_piece(source, &docs, &DocPiece { tag: true, ..piece }, true).is_err());
        let invalid = "/** @template_fake */\nfunction helper(){}";
        let tree = parse("javascript", invalid, &d).unwrap();
        let declaration = children(tree.root_node())[0];
        let docs = return_docs(invalid, declaration, &d).unwrap();
        assert!(
            checked_piece(
                invalid,
                &docs,
                &DocPiece {
                    start_byte: 4,
                    end_byte: invalid.find(" */").unwrap(),
                    tag: true
                },
                true
            )
            .is_err()
        );
        let long = format!("/**{}*/\nfunction helper(){{}}", "x".repeat(16_001));
        let tree = parse("javascript", &long, &d).unwrap();
        let declaration = children(tree.root_node())[0];
        let docs = return_docs(&long, declaration, &d).unwrap();
        assert!(
            checked_piece(
                &long,
                &docs,
                &DocPiece {
                    start_byte: 3,
                    end_byte: 16_004,
                    tag: false
                },
                false
            )
            .is_err()
        );
    }
}
