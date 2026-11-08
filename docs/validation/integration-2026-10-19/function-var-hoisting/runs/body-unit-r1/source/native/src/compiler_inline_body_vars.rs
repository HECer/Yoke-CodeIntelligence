//! Function-owned var declarations, distinct declaration/assignment roles and hoists.
use super::*;
use std::collections::BTreeMap;
pub(super) type Range = (usize, usize);
pub(super) struct Group<'a> {
    pub name: String,
    pub ids: Vec<Node<'a>>,
    pub parameter: Option<Node<'a>>,
    pub function: Option<Node<'a>>,
    annotation: Option<Node<'a>>,
}
pub(super) enum Change<'a> {
    Declaration(Node<'a>),
    Keyword(Node<'a>),
    Pattern(Node<'a>),
}
impl Change<'_> {
    pub fn range(&self) -> Range {
        let n = match self {
            Self::Declaration(n) | Self::Keyword(n) | Self::Pattern(n) => n,
        };
        (n.start_byte(), n.end_byte())
    }
}
pub(super) struct Plan<'a> {
    pub groups: Vec<Group<'a>>,
    pub ids: BTreeMap<Range, Node<'a>>,
    pub changes: Vec<Change<'a>>,
}
pub(super) fn bound<'a>(pattern: Node<'a>, d: &Deadline) -> Result<Vec<Node<'a>>> {
    let mut found = vec![];
    let mut stack = vec![pattern];
    let mut count = 0;
    while let Some(n) = stack.pop() {
        d.check()?;
        count += 1;
        ensure!(count <= 20_000, "Var binding pattern capacity exceeded");
        match n.kind() {
            "identifier" | "shorthand_property_identifier_pattern" => found.push(n),
            "required_parameter" | "optional_parameter" => stack.push(
                n.child_by_field_name("pattern")
                    .context("Var parameter pattern absent")?,
            ),
            "assignment_pattern" | "object_assignment_pattern" => stack.push(
                n.child_by_field_name("left")
                    .context("Var default pattern absent")?,
            ),
            "pair_pattern" => stack.push(
                n.child_by_field_name("value")
                    .context("Var property pattern absent")?,
            ),
            "array_pattern" | "object_pattern" | "rest_pattern" => stack.extend(children(n)),
            _ => anyhow::bail!("Var binding pattern unavailable"),
        }
    }
    found.sort_by_key(Node::start_byte);
    Ok(found)
}
fn key(n: Node<'_>) -> Range {
    (n.start_byte(), n.end_byte())
}
fn name(n: Node<'_>, source: &str, d: &Deadline) -> Result<String> {
    parameters::identifier_name(n.utf8_text(source.as_bytes())?, d)
}
pub(super) fn analyze<'a>(declaration: Node<'a>, source: &str, d: &Deadline) -> Result<Plan<'a>> {
    let body = declaration
        .child_by_field_name("body")
        .context("Var body absent")?;
    let mut nodes = vec![];
    let mut changes = vec![];
    let mut stack = vec![body];
    let mut visits = 0;
    while let Some(n) = stack.pop() {
        d.check()?;
        visits += 1;
        ensure!(visits <= 200_000, "Var scope syntax capacity exceeded");
        if n.id() != body.id()
            && (function_boundary(n)
                || matches!(
                    n.kind(),
                    "class" | "class_declaration" | "abstract_class_declaration"
                ))
        {
            continue;
        }
        if n.kind() == "variable_declaration" {
            ensure!(
                source.get(n.start_byte()..n.start_byte() + 3) == Some("var"),
                "Var declaration keyword mismatch"
            );
            for v in children(n) {
                ensure!(
                    v.kind() == "variable_declarator",
                    "Var declaration child unavailable"
                );
                let pattern = v
                    .child_by_field_name("name")
                    .context("Var declaration name absent")?;
                let annotation = v.child_by_field_name("type");
                for id in bound(pattern, d)? {
                    nodes.push((
                        id,
                        if pattern.kind() == "identifier" {
                            annotation
                        } else {
                            None
                        },
                    ));
                }
            }
            changes.push(Change::Declaration(n));
        } else if n.kind() == "for_in_statement"
            && n.child_by_field_name("kind")
                .is_some_and(|k| k.kind() == "var")
        {
            ensure!(
                n.child_by_field_name("value").is_none(),
                "Var for-in initializer unavailable in strict scope"
            );
            let keyword = n.child_by_field_name("kind").unwrap();
            let pattern = n
                .child_by_field_name("left")
                .context("Var loop pattern absent")?;
            for id in bound(pattern, d)? {
                nodes.push((id, None));
            }
            changes.push(Change::Keyword(keyword));
            changes.push(Change::Pattern(pattern));
        }
        stack.extend(children(n));
    }
    nodes.sort_by_key(|(n, _)| n.start_byte());
    ensure!(
        nodes.len() <= 2000 && changes.len() <= 2000,
        "Var declaration capacity exceeded"
    );
    let mut parameters = BTreeMap::new();
    for p in children(
        declaration
            .child_by_field_name("parameters")
            .context("Var parameters absent")?,
    ) {
        for id in bound(p, d)? {
            parameters.insert(name(id, source, d)?, id);
        }
    }
    let mut functions = BTreeMap::new();
    for f in children(body) {
        if function_boundary(f)
            && let Some(id) = f.child_by_field_name("name")
        {
            functions.insert(name(id, source, d)?, id);
        }
    }
    let mut groups: Vec<Group<'a>> = vec![];
    let mut indices = BTreeMap::new();
    let mut ids = BTreeMap::new();
    for (id, annotation) in nodes {
        let text = name(id, source, d)?;
        let index = if let Some(&index) = indices.get(&text) {
            index
        } else {
            let index = groups.len();
            indices.insert(text.clone(), index);
            groups.push(Group {
                name: text.clone(),
                ids: vec![],
                parameter: parameters.get(&text).copied(),
                function: functions.get(&text).copied(),
                annotation,
            });
            index
        };
        groups[index].ids.push(id);
        ensure!(
            ids.insert(key(id), id).is_none(),
            "Var duplicate syntax site"
        );
    }
    ensure!(groups.len() <= 512, "Var name capacity exceeded");
    changes.sort_by_key(Change::range);
    ensure!(
        changes.windows(2).all(|p| p[0].range().1 <= p[1].range().0),
        "Var effect overlap"
    );
    Ok(Plan {
        groups,
        ids,
        changes,
    })
}
impl Plan<'_> {
    pub fn copies(&self) -> usize {
        self.groups
            .iter()
            .filter(|g| g.parameter.is_some() && g.function.is_none())
            .count()
    }
    pub fn aliases(&self) -> BTreeMap<Range, Range> {
        let mut result = BTreeMap::new();
        for g in &self.groups {
            let first = key(g.ids[0]);
            for id in g
                .ids
                .iter()
                .chain(g.parameter.iter())
                .chain(g.function.iter())
            {
                result.insert(key(*id), first);
            }
        }
        result
    }
    pub fn removable(&self, start: usize, end: usize) -> bool {
        self.ids.contains_key(&(start, end))
    }
    pub fn assignment_origin(&self, id: Node<'_>, source: &str, d: &Deadline) -> Result<Range> {
        let text = name(id, source, d)?;
        let mut at = id.parent();
        while let Some(n) = at {
            if n.kind() == "catch_clause"
                && let Some(p) = n.child_by_field_name("parameter")
                && p.kind() == "identifier"
                && name(p, source, d)? == text
            {
                return Ok(key(p));
            }
            if function_boundary(n) {
                break;
            }
            at = n.parent();
        }
        Ok(self
            .groups
            .iter()
            .find(|g| g.name == text)
            .context("Var assignment group absent")?
            .ids
            .first()
            .map(|n| key(*n))
            .unwrap())
    }
    pub fn captures(
        &self,
        out: &mut Rendered,
        source: &str,
        names: &Names,
        line: &str,
    ) -> Result<()> {
        ensure!(
            names.var_parameters.len() == self.copies(),
            "Var parameter copy capacity mismatch"
        );
        let mut index = 0;
        for g in &self.groups {
            if g.function.is_none()
                && let Some(p) = g.parameter
            {
                out.generated(&format!("const {} = (", names.var_parameters[index]))?;
                out.original(source, p.start_byte(), p.end_byte(), "parameter")?;
                out.generated(&format!(");{line}"))?;
                index += 1;
            }
        }
        Ok(())
    }
    pub fn hoists(
        &self,
        out: &mut Rendered,
        source: &str,
        names: &Names,
        line: &str,
    ) -> Result<()> {
        let mut index = 0;
        for g in &self.groups {
            if g.function.is_some() {
                continue;
            }
            let id = g.ids[0];
            out.generated("let ")?;
            out.original(source, id.start_byte(), id.end_byte(), "var_hoist")?;
            if let Some(t) = g.annotation {
                out.original(source, t.start_byte(), t.end_byte(), "var_type")?;
            }
            if g.parameter.is_some() {
                out.generated(&format!(" = {}", names.var_parameters[index]))?;
                index += 1;
            }
            out.generated(&format!(";{line}"))?;
        }
        Ok(())
    }
}
pub(super) fn replacement(change: &Change<'_>, source: &str, d: &Deadline) -> Result<Rendered> {
    let mut out = Rendered::default();
    match change {
        Change::Keyword(_) => {}
        Change::Pattern(p) => {
            out.original(source, p.start_byte(), p.end_byte(), "var_assignment")?
        }
        Change::Declaration(n) => {
            out.generated("(")?;
            let mut cursor = n.start_byte() + 3;
            for v in children(*n) {
                d.check()?;
                let pattern = v
                    .child_by_field_name("name")
                    .context("Var pattern absent")?;
                out.original(source, cursor, pattern.start_byte(), "body")?;
                if let Some(value) = v.child_by_field_name("value") {
                    out.original(
                        source,
                        pattern.start_byte(),
                        pattern.end_byte(),
                        "var_assignment",
                    )?;
                    let annotation = v.child_by_field_name("type");
                    let mut c = v.walk();
                    let equal = v
                        .children(&mut c)
                        .find(|n| n.kind() == "=" && n.end_byte() <= value.start_byte())
                        .context("Var assignment token absent")?;
                    let prefix_end = annotation.map_or(equal.start_byte(), |t| t.start_byte());
                    out.original(source, pattern.end_byte(), prefix_end, "body")?;
                    if let Some(t) = annotation {
                        out.original(source, t.end_byte(), equal.start_byte(), "body")?;
                    }
                    out.generated("= (")?;
                    if annotation.is_some() {
                        out.generated("(")?;
                    }
                    out.original(source, equal.end_byte(), value.end_byte(), "body")?;
                    if let Some(t) = annotation {
                        out.generated(") as (")?;
                        out.original(source, t.start_byte() + 1, t.end_byte(), "var_type")?;
                        out.generated(")")?;
                    }
                    out.generated(")")?;
                } else {
                    out.generated("void ")?;
                    if let Some(t) = v.child_by_field_name("type") {
                        out.original(source, pattern.end_byte(), t.start_byte(), "body")?;
                        out.generated("(0 as unknown as (")?;
                        out.original(source, t.start_byte() + 1, t.end_byte(), "var_type")?;
                        out.generated("))")?;
                        out.original(source, t.end_byte(), v.end_byte(), "body")?;
                    } else {
                        out.generated("0")?;
                        out.original(source, pattern.end_byte(), v.end_byte(), "body")?;
                    }
                }
                cursor = v.end_byte();
            }
            out.generated(")")?;
            out.original(source, cursor, n.end_byte(), "body")?;
        }
    }
    Ok(out)
}
