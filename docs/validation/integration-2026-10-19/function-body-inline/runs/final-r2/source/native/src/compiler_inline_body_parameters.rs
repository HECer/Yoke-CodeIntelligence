//! Parameter initialization follows complete argument evaluation in a separate scope.
use super::*;
pub(super) fn identifier_name(raw: &str, d: &Deadline) -> Result<String> {
    let mut chars = raw.chars();
    let mut out = String::new();
    while let Some(c) = chars.next() {
        d.check()?;
        if c != '\\' {
            out.push(c);
            continue;
        }
        ensure!(
            chars.next() == Some('u'),
            "Body parameter identifier escape unsupported"
        );
        let first = chars.next().context("Body parameter escape incomplete")?;
        let mut hex = String::new();
        if first == '{' {
            loop {
                let n = chars.next().context("Body parameter escape incomplete")?;
                if n == '}' {
                    break;
                }
                hex.push(n);
                ensure!(hex.len() <= 6, "Body parameter escape capacity");
            }
        } else {
            hex.push(first);
            for _ in 0..3 {
                hex.push(chars.next().context("Body parameter escape incomplete")?);
            }
        }
        let value = u32::from_str_radix(&hex, 16)?;
        out.push(char::from_u32(value).context("Body parameter escape is not a scalar")?);
    }
    Ok(out)
}
pub(super) fn emit(
    out: &mut Rendered,
    source: &str,
    formal: Node<'_>,
    arguments: &[String],
    line: &str,
    d: &Deadline,
) -> Result<()> {
    let parameters = children(formal);
    ensure!(parameters.len() <= 64, "Body parameter capacity exceeded");
    let mut cursor = formal.start_byte() + 1;
    let mut walk = formal.walk();
    let commas: Vec<_> = formal
        .children(&mut walk)
        .filter(|n| n.kind() == ",")
        .collect();
    let trivia = |out: &mut Rendered, start: usize, end: usize| -> Result<()> {
        let mut at = start;
        for comma in &commas {
            if comma.start_byte() >= at && comma.end_byte() <= end {
                out.original(source, at, comma.start_byte(), "parameter")?;
                at = comma.end_byte();
            }
        }
        out.original(source, at, end, "parameter")?;
        if end > start {
            out.generated(line)?;
        }
        Ok(())
    };
    for (index, p) in parameters.iter().copied().enumerate() {
        d.check()?;
        trivia(out, cursor, p.start_byte())?;
        cursor = p.end_byte();
        let (mut pattern, annotation, value, optional) = match p.kind() {
            "required_parameter" | "optional_parameter" => (
                p.child_by_field_name("pattern")
                    .context("Body parameter pattern absent")?,
                p.child_by_field_name("type"),
                p.child_by_field_name("value"),
                p.kind() == "optional_parameter",
            ),
            "assignment_pattern" => (
                p.child_by_field_name("left")
                    .context("Body default pattern absent")?,
                None,
                p.child_by_field_name("right"),
                false,
            ),
            "identifier" | "object_pattern" | "array_pattern" | "rest_pattern" => {
                (p, None, None, false)
            }
            _ => anyhow::bail!("Body parameter form unavailable"),
        };
        let rest = pattern.kind() == "rest_pattern";
        if rest {
            ensure!(
                index + 1 == parameters.len() && value.is_none(),
                "Body rest parameter position/default invalid"
            );
            let values = children(pattern);
            ensure!(values.len() == 1, "Body rest pattern multiplicity");
            pattern = values[0];
        }
        ensure!(
            matches!(
                pattern.kind(),
                "identifier" | "object_pattern" | "array_pattern"
            ),
            "Body parameter pattern unavailable"
        );
        ensure!(
            pattern.utf8_text(source.as_bytes())? != "this",
            "Body erased this parameter unavailable"
        );
        out.generated("let ")?;
        out.original(
            source,
            pattern.start_byte(),
            pattern.end_byte(),
            "parameter",
        )?;
        if let Some(ty) = annotation {
            ensure!(
                source.as_bytes().get(ty.start_byte()) == Some(&b':'),
                "Body parameter type annotation mismatch"
            );
            out.original(
                source,
                pattern.end_byte(),
                if optional {
                    let mut c = p.walk();
                    p.children(&mut c)
                        .find(|n| n.kind() == "?")
                        .context("Body optional marker absent")?
                        .start_byte()
                } else {
                    ty.start_byte()
                },
                "parameter",
            )?;
            if optional {
                let mut c = p.walk();
                let q = p
                    .children(&mut c)
                    .find(|n| n.kind() == "?")
                    .context("Body optional marker absent")?;
                out.original(source, q.end_byte(), ty.start_byte(), "parameter")?;
                out.generated(": (")?;
                out.original(source, ty.start_byte() + 1, ty.end_byte(), "parameter")?;
                out.generated(") | undefined")?;
            } else {
                out.original(source, ty.start_byte(), ty.end_byte(), "parameter")?;
            }
        }
        let equal = if let Some(default) = value {
            let mut c = p.walk();
            Some(
                p.children(&mut c)
                    .find(|n| n.kind() == "=" && n.end_byte() <= default.start_byte())
                    .context("Body default assignment token absent")?,
            )
        } else {
            None
        };
        let prefix_end = equal.map_or(p.end_byte(), |n| n.start_byte());
        let prefix_start = annotation.map_or(pattern.end_byte(), |n| n.end_byte());
        if annotation.is_none() && optional {
            let mut c = p.walk();
            let q = p
                .children(&mut c)
                .find(|n| n.kind() == "?")
                .context("Body optional marker absent")?;
            out.original(source, prefix_start, q.start_byte(), "parameter")?;
            out.original(source, q.end_byte(), prefix_end, "parameter")?;
        } else {
            out.original(source, prefix_start, prefix_end, "parameter")?;
        }
        out.generated(" = ")?;
        let argument = arguments.get(index).map(String::as_str).unwrap_or("void 0");
        if rest {
            out.generated("[")?;
            out.generated(
                &arguments
                    .iter()
                    .skip(index)
                    .cloned()
                    .collect::<Vec<_>>()
                    .join(","),
            )?;
            out.generated("]")?;
        } else if let Some(default) = value {
            let equal = equal.context("Body default assignment token absent")?;
            if index < arguments.len() {
                out.generated(&format!("({argument}) === void 0 ? ("))?;
            } else {
                out.generated("(")?;
            }
            if pattern.kind() == "identifier" {
                let name = serde_json::to_string(&identifier_name(
                    pattern.utf8_text(source.as_bytes())?,
                    d,
                )?)?;
                out.generated(&format!("{{[{name}]: ("))?;
                if annotation.is_some() {
                    out.generated("(")?;
                }
                out.original(source, equal.end_byte(), default.end_byte(), "parameter")?;
                if let Some(ty) = annotation {
                    out.generated(") as (")?;
                    out.original(source, ty.start_byte() + 1, ty.end_byte(), "parameter")?;
                    out.generated(")")?;
                }
                out.generated(&format!(")}})[{name}]"))?;
            } else {
                out.generated("(")?;
                out.original(source, equal.end_byte(), default.end_byte(), "parameter")?;
                out.generated("))")?;
            }
            if index < arguments.len() {
                out.generated(&format!(" : ({argument})"))?;
            }
        } else {
            out.generated(argument)?;
        }
        out.generated(&format!(";{line}"))?;
    }
    trivia(out, cursor, formal.end_byte() - 1)?;
    Ok(())
}
