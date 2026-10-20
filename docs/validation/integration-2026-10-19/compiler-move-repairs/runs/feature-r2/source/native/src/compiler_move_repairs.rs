//! Independently reconstruct bounded repairs and keep edits in original bytes.
use super::{Contents, Declaration, Effect, Span, Target, TextEdit, ast};
use crate::{capture::Deadline, model::Snapshot};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use tree_sitter::{Node, Tree};
#[cfg(test)]
#[path = "compiler_move_repair_tests.rs"]
mod tests;

#[derive(Debug, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub(super) enum Repair {
    Reexport {
        original: Span,
        current: Span,
        selected: Vec<usize>,
        module_text: String,
    },
    ImportAlias {
        import_name: Span,
        original_references: Vec<Span>,
        current_references: Vec<Span>,
        new_name: String,
    },
}
#[derive(Debug, Clone)]
enum Piece {
    Original(usize, usize),
    Inserted(String),
}
impl Piece {
    fn len(&self) -> usize {
        match self {
            Self::Original(s, e) => e - s,
            Self::Inserted(text) => text.len(),
        }
    }
}
fn split(pieces: &mut Vec<Piece>, at: usize, d: &Deadline) -> Result<usize> {
    let mut cursor = 0usize;
    for i in 0..pieces.len() {
        d.check()?;
        let size = pieces[i].len();
        if at == cursor {
            return Ok(i);
        }
        if at < cursor + size {
            let offset = at - cursor;
            let (left, right) = match &pieces[i] {
                Piece::Original(s, e) => (
                    Piece::Original(*s, *s + offset),
                    Piece::Original(*s + offset, *e),
                ),
                Piece::Inserted(text) => {
                    ensure!(
                        text.is_char_boundary(offset),
                        "Move repair splits inserted UTF-8"
                    );
                    (
                        Piece::Inserted(text[..offset].into()),
                        Piece::Inserted(text[offset..].into()),
                    )
                }
            };
            pieces.splice(i..=i, [left, right]);
            return Ok(i + 1);
        }
        cursor += size;
    }
    ensure!(at == cursor, "Move repair piece offset outside source");
    Ok(pieces.len())
}
fn apply_pieces(
    tables: &mut BTreeMap<String, Vec<Piece>>,
    changes: &[TextEdit],
    before: &Contents,
    d: &Deadline,
) -> Result<()> {
    for edit in changes.iter().rev() {
        d.check()?;
        let old = before
            .get(&edit.path)
            .context("Move repair path outside capture")?;
        let pieces = tables
            .entry(edit.path.clone())
            .or_insert_with(|| vec![Piece::Original(0, old.len())]);
        let start = split(pieces, edit.start_byte, d)?;
        let end = split(pieces, edit.end_byte, d)?;
        pieces.splice(
            start..end,
            (!edit.text.is_empty()).then(|| Piece::Inserted(edit.text.clone())),
        );
        ensure!(pieces.len() <= 8001, "Move repair piece capacity exceeded");
    }
    Ok(())
}
fn materialize(
    tables: &BTreeMap<String, Vec<Piece>>,
    before: &Contents,
    d: &Deadline,
) -> Result<Contents> {
    let mut result = before.clone();
    let mut bytes = 0usize;
    for (path, pieces) in tables {
        let mut text = String::new();
        for piece in pieces {
            d.check()?;
            match piece {
                Piece::Original(s, e) => {
                    text.push_str(crate::paths::utf8_range(&before[path], *s, *e)?)
                }
                Piece::Inserted(value) => text.push_str(value),
            }
            ensure!(
                text.len() <= 4_000_000,
                "Move repair result file capacity exceeded"
            );
        }
        bytes = bytes.saturating_add(text.len());
        ensure!(
            bytes <= 16_000_000,
            "Move repaired output capacity exceeded"
        );
        result.insert(path.clone(), text);
    }
    Ok(result)
}
fn compose(
    tables: &BTreeMap<String, Vec<Piece>>,
    before: &Contents,
    d: &Deadline,
) -> Result<Vec<TextEdit>> {
    let mut result = vec![];
    for (path, pieces) in tables {
        let mut cursor = 0usize;
        let mut pending = String::new();
        let mut emit = |start, end, text: String| -> Result<()> {
            if crate::paths::utf8_range(&before[path], start, end)? != text {
                result.push(TextEdit {
                    path: path.clone(),
                    start_byte: start,
                    end_byte: end,
                    text,
                });
                ensure!(
                    result.len() <= 2000,
                    "Move composed operation capacity exceeded"
                );
            }
            Ok(())
        };
        for piece in pieces {
            d.check()?;
            match piece {
                Piece::Inserted(text) => pending.push_str(text),
                Piece::Original(s, e) => {
                    ensure!(*s >= cursor, "Move original pieces reordered");
                    emit(cursor, *s, std::mem::take(&mut pending))?;
                    cursor = *e;
                }
            }
        }
        emit(cursor, before[path].len(), pending)?;
    }
    Ok(result)
}
fn exact<'a>(tree: &'a Tree, span: &Span, d: &Deadline) -> Result<Node<'a>> {
    let mut stack = vec![tree.root_node()];
    let mut visits = 0usize;
    let mut found = None;
    while let Some(node) = stack.pop() {
        d.check()?;
        visits += 1;
        ensure!(visits <= 200_000, "Move repair AST capacity exceeded");
        if node.start_byte() == span.start_byte && node.end_byte() == span.end_byte {
            found = Some(node);
        }
        if node.start_byte() <= span.start_byte && node.end_byte() >= span.end_byte {
            let mut c = node.walk();
            stack.extend(node.named_children(&mut c));
        }
    }
    found.context("Move repair exact syntax anchor absent")
}
fn export_clause(node: Node<'_>) -> Result<Node<'_>> {
    let mut c = node.walk();
    node.named_children(&mut c)
        .find(|n| n.kind() == "export_clause")
        .context("Move repair named export clause absent")
}
fn specs(clause: Node<'_>) -> Vec<Node<'_>> {
    let mut c = clause.walk();
    clause
        .named_children(&mut c)
        .filter(|n| n.kind() == "export_specifier")
        .collect()
}
fn reexport_text(
    text: &str,
    node: Node<'_>,
    selected: &[usize],
    module_text: &str,
    d: &Deadline,
) -> Result<String> {
    let clause = export_clause(node)?;
    let items = specs(clause);
    ensure!(
        !items.is_empty() && items.len() <= 2000,
        "Move repair export item capacity exceeded"
    );
    let source = node
        .child_by_field_name("source")
        .context("Move repair export module absent")?;
    let scratch = format!("export {{}} from {module_text};");
    let parsed = ast::parse("typescript", &scratch, d)?;
    let mut c = parsed.root_node().walk();
    let roots: Vec<_> = parsed.root_node().named_children(&mut c).collect();
    ensure!(
        roots.len() == 1
            && roots[0].kind() == "export_statement"
            && roots[0]
                .child_by_field_name("source")
                .is_some_and(|n| n.utf8_text(scratch.as_bytes()).ok() == Some(module_text)),
        "Move repair module must be exactly one string literal"
    );
    let original = node.utf8_text(text.as_bytes())?;
    if selected.len() == items.len() {
        let at = source.start_byte() - node.start_byte();
        return Ok(format!(
            "{}{}{}",
            &original[..at],
            module_text,
            &original[at + source.end_byte() - source.start_byte()..]
        ));
    }
    let mut c = clause.walk();
    let commas: Vec<_> = clause
        .children(&mut c)
        .filter(|n| n.kind() == ",")
        .collect();
    ensure!(
        commas.len() == items.len() - 1 || commas.len() == items.len(),
        "Move export separator shape mismatch"
    );
    let mut previous = clause.start_byte() + 1;
    let mut parts = Vec::new();
    for (i, item) in items.iter().enumerate() {
        d.check()?;
        let end = commas
            .get(i)
            .map_or(clause.end_byte() - 1, |n| n.start_byte());
        ensure!(
            previous <= item.start_byte() && item.end_byte() <= end,
            "Move export item ownership mismatch"
        );
        parts.push(crate::paths::utf8_range(text, previous, end)?);
        previous = commas.get(i).map_or(end, |n| n.end_byte());
    }
    let trailing = commas.len() == items.len();
    let tail = if trailing {
        crate::paths::utf8_range(text, previous, clause.end_byte() - 1)?
    } else {
        ""
    };
    let prefix = crate::paths::utf8_range(text, node.start_byte(), clause.start_byte() + 1)?;
    let suffix = crate::paths::utf8_range(text, clause.end_byte() - 1, node.end_byte())?;
    let at = source.start_byte() - (clause.end_byte() - 1);
    let new_suffix = format!(
        "{}{}{}",
        &suffix[..at],
        module_text,
        &suffix[at + source.end_byte() - source.start_byte()..]
    );
    let chosen: BTreeSet<_> = selected.iter().copied().collect();
    let kept: Vec<_> = parts
        .iter()
        .enumerate()
        .filter(|(i, _)| !chosen.contains(i))
        .map(|(_, p)| *p)
        .collect();
    let moved: Vec<_> = parts
        .iter()
        .enumerate()
        .filter(|(i, _)| chosen.contains(i))
        .map(|(_, p)| *p)
        .collect();
    Ok(format!(
        "{prefix}{}{tail_comma}{tail}{suffix}{line}{prefix}{}{moved_comma}{new_suffix}",
        kept.join(","),
        moved.join(","),
        tail_comma = if trailing { "," } else { "" },
        moved_comma = if trailing { "," } else { "" },
        line = if text.contains("\r\n") { "\r\n" } else { "\n" }
    ))
}
struct ContextForRepair<'a> {
    before: &'a Contents,
    current: &'a Contents,
    snapshot: &'a Snapshot,
    symbol: &'a Declaration,
    destination: &'a str,
    effect: &'a Effect,
}
impl ContextForRepair<'_> {
    fn tree(&self, span: &Span, original: bool, d: &Deadline) -> Result<Tree> {
        let contents = if original { self.before } else { self.current };
        span.check(contents)?;
        let file = self
            .snapshot
            .files
            .iter()
            .find(|f| f.path == span.path)
            .context("Move repair file absent")?;
        ast::parse(&file.language, &contents[&span.path], d)
    }
    fn changes(&self, repair: &Repair, d: &Deadline) -> Result<Vec<TextEdit>> {
        match repair {
            Repair::Reexport {
                original,
                current,
                selected,
                module_text,
            } => {
                ensure!(
                    original.path == current.path
                        && original.text == current.text
                        && !selected.is_empty()
                        && module_text.len() <= 4096
                        && !module_text.contains('\0'),
                    "Move reexport repair origin/shape mismatch"
                );
                let ot = self.tree(original, true, d)?;
                let ct = self.tree(current, false, d)?;
                let on = exact(&ot, original, d)?;
                let cn = exact(&ct, current, d)?;
                ensure!(
                    on.kind() == "export_statement"
                        && cn.kind() == "export_statement"
                        && on.parent().is_some_and(|p| p.kind() == "program")
                        && cn.parent().is_some_and(|p| p.kind() == "program"),
                    "Move reexport repair must own top-level exports"
                );
                let import = self
                    .effect
                    .before
                    .modules
                    .iter()
                    .find(|m| m.path == original.path)
                    .and_then(|m| {
                        m.imports
                            .iter()
                            .find(|i| i.kind == "reexport" && i.span.key() == original.key())
                    })
                    .context("Move reexport original module evidence absent")?;
                ensure!(
                    import.module.resolution == "resolved"
                        && import.module.targets
                            == [Target::Module {
                                path: self.symbol.path.clone()
                            }],
                    "Move reexport repair requires original selected source module"
                );
                let items = specs(export_clause(on)?);
                ensure!(
                    items.len() == import.names.len(),
                    "Move reexport original name coverage mismatch"
                );
                let mut expected = vec![];
                let selected_name = Span {
                    path: self.symbol.path.clone(),
                    start_byte: self
                        .symbol
                        .name_start_byte
                        .context("Move original name absent")?,
                    end_byte: self
                        .symbol
                        .name_end_byte
                        .context("Move original name absent")?,
                    text: self.symbol.name.clone(),
                };
                for (i, (item, binding)) in items.iter().zip(&import.names).enumerate() {
                    d.check()?;
                    let name = item
                        .child_by_field_name("alias")
                        .or_else(|| item.child_by_field_name("name"))
                        .context("Move reexport name absent")?;
                    ensure!(
                        binding.span.key()
                            == (original.path.clone(), name.start_byte(), name.end_byte()),
                        "Move reexport name anchor mismatch"
                    );
                    if binding.resolution == "resolved"
                        && binding.targets
                            == [Target::Captured {
                                span: selected_name.clone(),
                            }]
                    {
                        expected.push(i);
                    }
                }
                ensure!(
                    &expected == selected,
                    "Move reexport selected-name evidence mismatch"
                );
                let text =
                    reexport_text(&self.current[&current.path], cn, selected, module_text, d)?;
                Ok(vec![TextEdit {
                    path: current.path.clone(),
                    start_byte: current.start_byte,
                    end_byte: current.end_byte,
                    text,
                }])
            }
            Repair::ImportAlias {
                import_name,
                original_references,
                current_references,
                new_name,
            } => {
                ensure!(
                    import_name.path == self.destination
                        && !original_references.is_empty()
                        && original_references.len() == current_references.len()
                        && original_references.len() <= 2000,
                    "Move import alias repair scope/capacity mismatch"
                );
                let suffix = new_name
                    .strip_prefix("__ci_move_")
                    .context("Move repair fresh alias namespace mismatch")?;
                ensure!(
                    suffix.len() <= 6
                        && suffix.bytes().all(|b| b.is_ascii_digit())
                        && suffix
                            .parse::<usize>()
                            .is_ok_and(|n| (1..=100_000).contains(&n) && suffix == n.to_string())
                        && !self
                            .effect
                            .before
                            .bindings
                            .iter()
                            .any(|b| b.span.text == *new_name),
                    "Move repair alias is not fresh within its source observations"
                );
                let tree = self.tree(import_name, false, d)?;
                let node = exact(&tree, import_name, d)?;
                ensure!(
                    node.kind() == "identifier"
                        && node.parent().is_some_and(|p| matches!(
                            p.kind(),
                            "import_clause" | "import_specifier" | "namespace_import"
                        )),
                    "Move repair alias does not own an import binding"
                );
                let moved = ast::named_statement(
                    &tree,
                    &self.current[self.destination],
                    &self.symbol.name,
                    d,
                )?
                .context("Move repair moved declaration absent")?;
                let import_text = if node.parent().is_some_and(|p| {
                    p.kind() == "import_specifier" && p.child_by_field_name("alias").is_none()
                }) {
                    format!("{} as {new_name}", import_name.text)
                } else {
                    new_name.clone()
                };
                let mut changes = vec![TextEdit {
                    path: import_name.path.clone(),
                    start_byte: import_name.start_byte,
                    end_byte: import_name.end_byte,
                    text: import_text,
                }];
                let mut original_keys = BTreeSet::new();
                let mut current_keys = BTreeSet::new();
                let mut targets = None;
                for (original, current) in original_references.iter().zip(current_references) {
                    d.check()?;
                    original.check(self.before)?;
                    current.check(self.current)?;
                    ensure!(
                        original.path == self.symbol.path
                            && original.start_byte >= self.symbol.start_byte
                            && original.end_byte <= self.symbol.end_byte
                            && current.path == self.destination
                            && current.start_byte >= moved.node.start_byte()
                            && current.end_byte <= moved.node.end_byte()
                            && original_keys.insert(original.key())
                            && current_keys.insert(current.key()),
                        "Move alias references escape selected declaration or repeat"
                    );
                    let binding = self
                        .effect
                        .before
                        .bindings
                        .iter()
                        .find(|b| b.span.key() == original.key())
                        .context("Move repair original reference evidence absent")?;
                    ensure!(binding.resolution=="resolved"&&!binding.targets.is_empty()&&binding.targets.iter().all(|t|!matches!(t,Target::Library{..})&&!matches!(t,Target::Captured{span}if span.path==self.symbol.path&&span.start_byte>=self.symbol.start_byte&&span.end_byte<=self.symbol.end_byte)),"Move alias repair requires captured free binding");
                    if let Some(expected) = targets {
                        ensure!(
                            expected == &binding.targets,
                            "Move alias original reference targets differ"
                        );
                    } else {
                        targets = Some(&binding.targets);
                    }
                    let n = exact(&tree, current, d)?;
                    ensure!(
                        matches!(
                            n.kind(),
                            "identifier" | "type_identifier" | "shorthand_property_identifier"
                        ),
                        "Move repair reference must be an identifier"
                    );
                    let text = if n.kind() == "shorthand_property_identifier" {
                        format!("{}: {new_name}", current.text)
                    } else {
                        new_name.clone()
                    };
                    changes.push(TextEdit {
                        path: current.path.clone(),
                        start_byte: current.start_byte,
                        end_byte: current.end_byte,
                        text,
                    });
                }
                if let Some(module) = self
                    .effect
                    .before
                    .modules
                    .iter()
                    .find(|m| m.path == self.destination)
                {
                    ensure!(!module.imports.iter().flat_map(|i|&i.names).any(|b|b.span.text==import_name.text&&Some(&b.targets)==targets),"Move repair cannot rename a preserved target import");
                }
                changes.sort_by(|a, b| (&a.path, a.start_byte).cmp(&(&b.path, b.start_byte)));
                ensure!(
                    changes
                        .windows(2)
                        .all(|p| p[0].path != p[1].path || p[0].end_byte <= p[1].start_byte),
                    "Move alias repair overlaps"
                );
                Ok(changes)
            }
        }
    }
}
pub(super) fn admit(
    before: &Contents,
    original_edits: &[TextEdit],
    effect: &Effect,
    snapshot: &Snapshot,
    symbol: &Declaration,
    destination: &str,
    d: &Deadline,
) -> Result<(Vec<TextEdit>, Contents)> {
    if effect.repairs.is_empty() {
        ensure!(
            effect.composition.is_none(),
            "Move composition without repairs"
        );
        return Ok((
            original_edits.to_vec(),
            super::apply(original_edits, before, d)?,
        ));
    }
    ensure!(
        effect.repairs.len() <= 256
            && effect.composition.as_deref() == Some("original_byte_pieces_v1"),
        "Move repair composition contract mismatch"
    );
    let mut tables = BTreeMap::new();
    apply_pieces(&mut tables, original_edits, before, d)?;
    let mut current = materialize(&tables, before, d)?;
    let mut operations = effect.raw_edits.len();
    for repair in &effect.repairs {
        d.check()?;
        let changes = ContextForRepair {
            before,
            current: &current,
            snapshot,
            symbol,
            destination,
            effect,
        }
        .changes(repair, d)?;
        operations = operations.saturating_add(changes.len());
        ensure!(
            operations <= 2000,
            "Move repair operation capacity exceeded"
        );
        for edit in &changes {
            crate::paths::utf8_range(
                current.get(&edit.path).context("Move repair path absent")?,
                edit.start_byte,
                edit.end_byte,
            )?;
        }
        apply_pieces(&mut tables, &changes, before, d)?;
        current = materialize(&tables, before, d)?;
    }
    let edits = compose(&tables, before, d)?;
    for repair in &effect.repairs {
        if let Repair::Reexport {
            original, selected, ..
        } = repair
        {
            d.check()?;
            let old = effect
                .before
                .modules
                .iter()
                .find(|m| m.path == original.path)
                .and_then(|m| m.imports.iter().find(|i| i.span.key() == original.key()))
                .context("Move original reexport absent")?;
            let current = effect
                .after
                .modules
                .iter()
                .find(|m| m.path == original.path)
                .context("Move repaired reexport module absent")?;
            let mut forwarded = BTreeSet::new();
            for import in &current.imports {
                d.check()?;
                if import.kind == "reexport"
                    && import.module.resolution == "resolved"
                    && import.module.targets
                        == [Target::Module {
                            path: destination.into(),
                        }]
                {
                    for name in &import.names {
                        d.check()?;
                        forwarded.insert(name.span.text.as_str());
                    }
                }
            }
            for index in selected {
                d.check()?;
                ensure!(
                    old.names
                        .get(*index)
                        .is_some_and(|name| forwarded.contains(name.span.text.as_str())),
                    "Move repaired exports do not target selected destination module"
                );
            }
        }
    }
    ensure!(
        super::apply(&edits, before, d)? == current,
        "Move composed byte replay mismatch"
    );
    Ok((edits, current))
}
