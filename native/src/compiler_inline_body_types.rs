//! Source-owned inference witnesses. Their initializer statements never execute.
use super::*;
use std::collections::BTreeMap;

struct Site<'a> {
    declaration: Node<'a>,
    pattern: Node<'a>,
    value: Option<Node<'a>>,
    loop_kind: Option<&'static str>,
    placement: Node<'a>,
    ids: Vec<(Node<'a>, usize)>,
    names: BTreeMap<String, usize>,
}
fn range(n: Node<'_>) -> (usize, usize) {
    (n.start_byte(), n.end_byte())
}
fn placement<'a>(mut n: Node<'a>, d: &Deadline) -> Result<Node<'a>> {
    for _ in 0..2000 {
        d.check()?;
        let Some(p) = n.parent() else { return Ok(n) };
        if p.kind() != "labeled_statement"
            || p.child_by_field_name("body")
                .is_none_or(|v| v.id() != n.id())
        {
            return Ok(n);
        }
        n = p;
    }
    anyhow::bail!("Var type statement label depth exceeded")
}
fn site<'a>(id: Node<'a>, d: &Deadline) -> Result<Site<'a>> {
    let mut at = Some(id);
    for _ in 0..2000 {
        d.check()?;
        let n = at.context("Var type source declaration absent")?;
        if n.kind() == "variable_declarator" {
            let statement = n.parent().context("Var type statement absent")?;
            ensure!(
                statement.kind() == "variable_declaration",
                "Var type source is not var"
            );
            let place = statement
                .parent()
                .filter(|p| {
                    p.kind() == "for_statement"
                        && p.child_by_field_name("initializer")
                            .is_some_and(|v| v.id() == statement.id())
                })
                .unwrap_or(statement);
            return Ok(Site {
                declaration: n,
                pattern: n
                    .child_by_field_name("name")
                    .context("Var type pattern absent")?,
                value: n.child_by_field_name("value"),
                loop_kind: None,
                placement: placement(place, d)?,
                ids: vec![],
                names: BTreeMap::new(),
            });
        }
        if n.kind() == "for_in_statement"
            && let Some(pattern) = n.child_by_field_name("left")
            && pattern.start_byte() <= id.start_byte()
            && pattern.end_byte() >= id.end_byte()
        {
            ensure!(
                n.child_by_field_name("kind")
                    .is_some_and(|v| v.kind() == "var"),
                "Var type loop keyword mismatch"
            );
            let mut cursor = n.walk();
            let operators: Vec<_> = n
                .children(&mut cursor)
                .filter(|v| matches!(v.kind(), "of" | "in"))
                .collect();
            ensure!(operators.len() == 1, "Var type loop operator unavailable");
            let kind = if operators[0].kind() == "of" {
                "of"
            } else {
                "in"
            };
            return Ok(Site {
                declaration: n,
                pattern,
                value: Some(
                    n.child_by_field_name("right")
                        .context("Var type loop source absent")?,
                ),
                loop_kind: Some(kind),
                placement: placement(n, d)?,
                ids: vec![],
                names: BTreeMap::new(),
            });
        }
        at = n.parent();
    }
    anyhow::bail!("Var type declaration depth exceeded")
}
pub(super) struct Plan<'a> {
    sites: Vec<Site<'a>>,
    groups: BTreeMap<usize, usize>,
    pub name_count: usize,
}
pub(super) fn analyze<'a>(
    vars: &variables::Plan<'a>,
    indices: &[usize],
    source: &str,
    d: &Deadline,
) -> Result<Plan<'a>> {
    ensure!(
        indices.len() <= 512 && indices.windows(2).all(|v| v[0] < v[1]),
        "Var type group order/capacity mismatch"
    );
    let mut sites = BTreeMap::new();
    let mut group_sites = BTreeMap::new();
    for &index in indices {
        d.check()?;
        let g = vars
            .groups
            .get(index)
            .context("Var type group outside source")?;
        ensure!(
            g.annotation.is_none() && g.parameter.is_none() && g.function.is_none(),
            "Var type group unsupported"
        );
        let mut found = None;
        for &id in &g.ids {
            let s = site(id, d)?;
            if s.value.is_some() || s.declaration.child_by_field_name("type").is_some() {
                found = Some(s);
                break;
            }
        }
        let s = found.context("Var source type witness unavailable")?;
        let key = range(s.declaration);
        group_sites.insert(index, key);
        sites.entry(key).or_insert(s);
    }
    let mut name_count = 0;
    for s in sites.values_mut() {
        for id in variables::bound(s.pattern, d)? {
            d.check()?;
            let name = parameters::identifier_name(id.utf8_text(source.as_bytes())?, d)?;
            let index = if let Some(&i) = s.names.get(&name) {
                i
            } else {
                let i = name_count;
                name_count += 1;
                s.names.insert(name, i);
                i
            };
            s.ids.push((id, index));
        }
    }
    ensure!(
        name_count <= 2000 && sites.len() <= 512,
        "Var type witness capacity exceeded"
    );
    let mut groups = BTreeMap::new();
    for (index, key) in group_sites {
        groups.insert(
            index,
            *sites[&key]
                .names
                .get(&vars.groups[index].name)
                .context("Var type witness binding absent")?,
        );
    }
    Ok(Plan {
        sites: sites.into_values().collect(),
        groups,
        name_count,
    })
}
impl Plan<'_> {
    pub fn name_for<'a>(&self, index: usize, names: &'a Names) -> Option<&'a str> {
        self.groups
            .get(&index)
            .map(|&i| names.var_seeds[i].as_str())
    }
    fn witness(&self, s: &Site<'_>, source: &str, names: &Names, d: &Deadline) -> Result<Rendered> {
        let mut out = Rendered::default();
        out.generated(if s.loop_kind.is_some() {
            "for (var "
        } else {
            "var "
        })?;
        let mut cursor = s.pattern.start_byte();
        for (id, index) in &s.ids {
            d.check()?;
            out.original(source, cursor, id.start_byte(), "type_witness")?;
            if id.kind() == "shorthand_property_identifier_pattern"
                || id.parent().is_some_and(|p| {
                    p.kind() == "object_assignment_pattern"
                        && p.child_by_field_name("left")
                            .is_some_and(|left| left.id() == id.id())
                })
            {
                let name = parameters::identifier_name(id.utf8_text(source.as_bytes())?, d)?;
                out.generated(&format!("{}: ", serde_json::to_string(&name)?))?;
            }
            out.generated(&names.var_seeds[*index])?;
            cursor = id.end_byte();
        }
        out.original(source, cursor, s.pattern.end_byte(), "type_witness")?;
        if let Some(kind) = s.loop_kind {
            out.generated(&format!(" {kind} ("))?;
            let value = s.value.context("Var type loop value absent")?;
            out.original(source, value.start_byte(), value.end_byte(), "type_witness")?;
            out.generated(")) {")?;
        } else {
            out.original(
                source,
                s.pattern.end_byte(),
                s.value.map_or(s.declaration.end_byte(), |v| v.end_byte()),
                "type_witness",
            )?;
            if s.value.is_none() {
                out.generated(" = void 0 as any")?;
            }
            out.generated(";")?;
        }
        let mut reads: Vec<_> = s.names.values().copied().collect();
        reads.sort_unstable();
        for index in reads {
            out.generated(&format!("void {};", names.var_seeds[index]))?;
        }
        if s.loop_kind.is_some() {
            out.generated("}")?;
        }
        Ok(out)
    }
    pub fn changes(
        &self,
        source: &str,
        names: &Names,
        line: &str,
        d: &Deadline,
    ) -> Result<Vec<(usize, usize, Rendered)>> {
        ensure!(
            names.var_seeds.len() == self.name_count,
            "Var type witness name count mismatch"
        );
        let mut placements: BTreeMap<(usize, usize), Vec<&Site<'_>>> = BTreeMap::new();
        for s in &self.sites {
            placements.entry(range(s.placement)).or_default().push(s);
        }
        let mut result = vec![];
        for ((start, end), sites) in placements {
            d.check()?;
            let mut open = Rendered::default();
            open.generated(&format!("{{{line}if (void 0 !== void 0) {{{line}"))?;
            for (i, s) in sites.iter().enumerate() {
                if i > 0 {
                    open.generated(line)?;
                }
                open.append(self.witness(s, source, names, d)?)?;
            }
            open.generated(&format!("{line}}}{line}"))?;
            let mut close = Rendered::default();
            close.generated(&format!("{line}}}"))?;
            result.push((start, start, open));
            result.push((end, end, close));
        }
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn names(seeds: Vec<String>) -> Names {
        Names {
            return_type: vec![],
            argument_capture: vec![],
            result: "__ci_inline_1".into(),
            exit: "__ci_inline_2".into(),
            arguments: vec![],
            do_first: None,
            var_parameters: vec![],
            var_seeds: seeds,
        }
    }
    #[test]
    fn source_type_witnesses_preserve_keys_labels_offsets_and_original_reads() {
        let source = "// 🌍\r\nfunction helper(){const source=[{row:{n:1}}];outer:for(var {\\u0072ow} of source){continue outer;}var read=()=>row.n;}";
        let d = Deadline::new(10_000);
        let tree = parse("typescript", source, &d).unwrap();
        let mut cursor = tree.root_node().walk();
        let declaration = tree
            .root_node()
            .named_children(&mut cursor)
            .find(|n| n.kind() == "function_declaration")
            .unwrap();
        let vars = variables::analyze(declaration, source, &d).unwrap();
        let plan = analyze(&vars, &[0, 1], source, &d).unwrap();
        assert_eq!(plan.name_count, 2);
        let names = names(vec!["__ci_inline_3".into(), "__ci_inline_4".into()]);
        let changes = plan.changes(source, &names, "\r\n", &d).unwrap();
        assert_eq!(changes.len(), 4);
        assert_eq!(changes[0].0, source.find("outer:").unwrap());
        assert!(changes[0].2.text.contains("if (void 0 !== void 0)"));
        assert!(changes[0].2.text.contains("{\"row\": __ci_inline_3}"));
        assert_eq!(plan.name_for(0, &names), Some("__ci_inline_3"));
        for (start, end, rendered) in changes {
            assert_eq!(start, end);
            for c in rendered.copies {
                assert_eq!(
                    &source[c.original_start..c.original_end],
                    &rendered.text[c.output_start..c.output_end]
                );
                assert_eq!(c.role, "type_witness");
            }
        }
    }
    #[test]
    fn malformed_type_groups_missing_names_and_deadlines_cannot_build_witnesses() {
        let source = "function helper(n:any){var n={x:1};var row={x:2};}";
        let d = Deadline::new(10_000);
        let tree = parse("typescript", source, &d).unwrap();
        let declaration = tree.root_node().named_child(0).unwrap();
        let vars = variables::analyze(declaration, source, &d).unwrap();
        for indices in [&[0][..], &[1, 1][..], &[9999][..]] {
            assert!(analyze(&vars, indices, source, &d).is_err());
        }
        let plan = analyze(&vars, &[1], source, &d).unwrap();
        assert!(plan.changes(source, &names(vec![]), "\n", &d).is_err());
        assert!(analyze(&vars, &[1], source, &Deadline::new(0)).is_err());
    }
}
