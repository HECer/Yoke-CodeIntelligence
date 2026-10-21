//! Checked publication boundary for source-reconstructed body frames.
use super::*;
use crate::model::Graph;
use std::collections::{BTreeMap, BTreeSet};
#[cfg(test)]
#[path = "compiler_inline_body_admission_tests.rs"]
mod tests;
type Contents = BTreeMap<String, String>;
type Key = (String, usize, usize);
#[derive(Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
enum Target {
    Captured {
        span: Span,
    },
    Module {
        path: String,
    },
    Library {
        name: String,
        hash: String,
        start_byte: usize,
        end_byte: usize,
        text: String,
    },
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Binding {
    span: Span,
    resolution: String,
    targets: Vec<Target>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Type {
    span: Span,
    r#type: String,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Observations {
    bindings: Vec<Binding>,
    types: Vec<Type>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Effect {
    schema: u32,
    path: String,
    keep_definition: bool,
    config: String,
    context_id: String,
    action_kind: String,
    frames: Vec<Frame>,
    references: Vec<Span>,
    #[serde(default)]
    var_assignments: Vec<Binding>,
    #[serde(default)]
    var_types: Vec<Type>,
    before: Observations,
    after: Observations,
    before_diagnostics: Vec<Value>,
    after_diagnostics: Vec<Value>,
    edits: Vec<Span>,
    result_hash: String,
}
struct Origin {
    before_start: usize,
    before_end: usize,
    after_start: usize,
    after_end: usize,
    role: &'static str,
}
fn key(span: &Span) -> Key {
    (span.path.clone(), span.start_byte, span.end_byte)
}
fn exact(span: &Span, sources: &Contents) -> Result<()> {
    ensure!(
        crate::paths::utf8_range(
            sources
                .get(&span.path)
                .context("Body observation path outside capture")?,
            span.start_byte,
            span.end_byte
        )? == span.text,
        "Body observation source mismatch"
    );
    Ok(())
}
pub(crate) fn classic<'a>(
    tree: &'a Tree,
    symbol: &Entity,
    source: &str,
    d: &Deadline,
) -> Result<Option<Node<'a>>> {
    let mut stack = vec![tree.root_node()];
    let mut visits = 0usize;
    let mut found = None;
    while let Some(n) = stack.pop() {
        d.check()?;
        visits += 1;
        ensure!(
            visits <= 200_000,
            "Body declaration syntax capacity exceeded"
        );
        if n.kind() == "function_declaration"
            && let Some(name) = n.child_by_field_name("name")
            && Some(name.start_byte()) == symbol.name_start_byte
            && Some(name.end_byte()) == symbol.name_end_byte
        {
            ensure!(
                n.end_byte() == symbol.end_byte
                    && name.utf8_text(source.as_bytes())? == symbol.name
                    && (n.start_byte() == symbol.start_byte
                        || n.parent().is_some_and(|p| p.kind() == "export_statement"
                            && p.start_byte() == symbol.start_byte)),
                "Body captured declaration mismatch"
            );
            ensure!(found.is_none(), "Body declaration ambiguous");
            found = Some(n);
        }
        if n.start_byte() <= symbol.name_start_byte.unwrap_or(symbol.start_byte)
            && n.end_byte() >= symbol.name_end_byte.unwrap_or(symbol.end_byte)
        {
            stack.extend(children(n));
        }
    }
    Ok(found)
}
fn bindings<'a>(
    o: &'a Observations,
    sources: &Contents,
    d: &Deadline,
) -> Result<BTreeMap<Key, &'a Binding>> {
    ensure!(
        o.bindings.len() <= 100_000 && o.types.len() <= 10_000,
        "Body observation capacity exceeded"
    );
    let mut map = BTreeMap::new();
    for b in &o.bindings {
        d.check()?;
        exact(&b.span, sources)?;
        ensure!(
            b.targets.len() <= 128
                && matches!(
                    b.resolution.as_str(),
                    "resolved" | "unbound" | "no_declarations"
                )
                && (b.resolution == "resolved") != b.targets.is_empty()
                && map.insert(key(&b.span), b).is_none(),
            "Body binding shape or duplicate observation"
        );
        for t in &b.targets {
            if let Target::Captured { span } = t {
                exact(span, sources)?;
            }
            if let Target::Module { path } = t {
                ensure!(
                    sources.contains_key(path),
                    "Body module target outside capture"
                );
            }
        }
    }
    let mut type_keys = BTreeSet::new();
    let mut type_bytes = 0usize;
    for t in &o.types {
        d.check()?;
        exact(&t.span, sources)?;
        type_bytes = type_bytes.saturating_add(t.r#type.len());
        ensure!(
            !t.r#type.is_empty()
                && t.r#type.len() <= 16_000
                && type_bytes <= 2_000_000
                && type_keys.insert(key(&t.span))
                && map.contains_key(&key(&t.span)),
            "Body type shape, capacity or duplicate observation"
        );
    }
    Ok(map)
}
fn type_coverage(tree: &Tree, path: &str, rows: &[Type], d: &Deadline) -> Result<()> {
    let reported: BTreeSet<_> = rows.iter().map(|t| key(&t.span)).collect();
    let mut stack = vec![tree.root_node()];
    let mut visits = 0usize;
    while let Some(n) = stack.pop() {
        d.check()?;
        visits += 1;
        ensure!(
            visits <= 200_000,
            "Body type coverage syntax capacity exceeded"
        );
        let name = match n.kind() {
            "variable_declarator" | "function_declaration" | "function_signature" => {
                n.child_by_field_name("name")
            }
            "required_parameter" | "optional_parameter" => {
                n.child_by_field_name("pattern").map(|p| {
                    if p.kind() == "rest_pattern" {
                        children(p).first().copied().unwrap_or(p)
                    } else {
                        p
                    }
                })
            }
            "identifier"
                if n.parent().is_some_and(|p| {
                    p.kind() == "formal_parameters"
                        || p.kind() == "arrow_function"
                            && p.child_by_field_name("parameter")
                                .is_some_and(|v| v.id() == n.id())
                }) =>
            {
                Some(n)
            }
            "assignment_pattern" if n.parent().is_some_and(|p| p.kind() == "formal_parameters") => {
                n.child_by_field_name("left")
            }
            "for_in_statement" if n.child_by_field_name("kind").is_some() => {
                n.child_by_field_name("left")
            }
            "catch_clause" => n.child_by_field_name("parameter"),
            "rest_pattern" if n.parent().is_some_and(|p| p.kind() == "formal_parameters") => {
                children(n).first().copied()
            }
            _ => None,
        };
        if let Some(pattern) = name {
            let names = if matches!(
                pattern.kind(),
                "identifier"
                    | "array_pattern"
                    | "object_pattern"
                    | "assignment_pattern"
                    | "object_assignment_pattern"
                    | "rest_pattern"
                    | "shorthand_property_identifier_pattern"
            ) {
                variables::bound(pattern, d)?
            } else {
                vec![]
            };
            for name in names {
                ensure!(
                    reported.contains(&(path.to_owned(), name.start_byte(), name.end_byte())),
                    "Body declaration type coverage absent"
                );
            }
        }
        stack.extend(children(n));
    }
    Ok(())
}
struct Checker<'a> {
    before: &'a Contents,
    after: &'a Contents,
    path: &'a str,
    origins: &'a [Origin],
    aliases: &'a BTreeMap<variables::Range, variables::Range>,
    spec: &'a CompilerSpec,
    libraries: &'a Value,
    cache: BTreeMap<String, String>,
    bytes: usize,
}
impl Checker<'_> {
    fn targets(&mut self, targets: &[Target], after: bool, d: &Deadline) -> Result<Vec<String>> {
        let mut ids = vec![];
        let mut merged = BTreeSet::new();
        for target in targets {
            d.check()?;
            let id = match target {
                Target::Module { path } => {
                    ensure!(
                        self.before.contains_key(path),
                        "Body module target outside capture"
                    );
                    format!("module:{path}")
                }
                Target::Captured { span } => {
                    exact(span, if after { self.after } else { self.before })?;
                    let (start, end) = if after && span.path == self.path {
                        let origin = self
                            .origins
                            .iter()
                            .find(|o| {
                                span.start_byte >= o.after_start && span.end_byte <= o.after_end
                            })
                            .context("Body definition lacks an original-byte correspondence")?;
                        (
                            origin.before_start + span.start_byte - origin.after_start,
                            origin.before_start + span.end_byte - origin.after_start,
                        )
                    } else {
                        (span.start_byte, span.end_byte)
                    };
                    let (start, end) = if span.path == self.path {
                        self.aliases
                            .get(&(start, end))
                            .copied()
                            .unwrap_or((start, end))
                    } else {
                        (start, end)
                    };
                    let text = crate::paths::utf8_range(
                        self.before
                            .get(&span.path)
                            .context("Body target original source absent")?,
                        start,
                        end,
                    )?;
                    let id =
                        identity(&("body_original_definition_v1", &span.path, start, end, text));
                    if span.path == self.path
                        && self.aliases.contains_key(&(start, end))
                        && !merged.insert(id.clone())
                    {
                        continue;
                    }
                    id
                }
                Target::Library {
                    name,
                    hash,
                    start_byte,
                    end_byte,
                    text,
                } => {
                    ensure!(
                        name.len() <= 200
                            && name.starts_with("lib.")
                            && name.ends_with(".d.ts")
                            && name.bytes().all(|c| c.is_ascii_lowercase()
                                || c.is_ascii_digit()
                                || matches!(c, b'.' | b'-'))
                            && self.libraries[name] == *hash,
                        "Body trusted-library target mismatch"
                    );
                    if !self.cache.contains_key(name) {
                        let p = self
                            .spec
                            .typescript
                            .canonicalize()?
                            .parent()
                            .context("Body compiler library directory absent")?
                            .join(name);
                        let mut f = fs::File::open(p)?;
                        let len = usize::try_from(f.metadata()?.len())?;
                        self.bytes = self.bytes.saturating_add(len);
                        ensure!(
                            len <= 4_000_000 && self.bytes <= 16_000_000,
                            "Body library capacity exceeded"
                        );
                        let mut data = Vec::with_capacity(len);
                        f.by_ref().take(len as u64 + 1).read_to_end(&mut data)?;
                        ensure!(
                            data.len() == len && crate::model::hash(&data) == *hash,
                            "Body library source changed"
                        );
                        self.cache.insert(name.clone(), String::from_utf8(data)?);
                    }
                    ensure!(
                        crate::paths::utf8_range(&self.cache[name], *start_byte, *end_byte)?
                            == text,
                        "Body library anchor mismatch"
                    );
                    identity(&("body_library_v1", name, hash, start_byte, end_byte, text))
                }
            };
            ids.push(id);
        }
        Ok(ids)
    }
}
#[allow(clippy::too_many_arguments)]
fn check(
    engine: &Engine,
    spec: &CompilerSpec,
    snapshot: &Snapshot,
    symbol: &Entity,
    source: &str,
    tree: &Tree,
    declaration: Node<'_>,
    keep: bool,
    e: &Effect,
    report: &Value,
    d: &Deadline,
) -> Result<(Vec<changes::Edit>, String, Value)> {
    ensure!(
        e.schema == 1
            && e.path == symbol.path
            && e.keep_definition == keep
            && e.action_kind == "native.inline.function.body"
            && crate::model::valid_id(&e.context_id)
            && crate::model::valid_id(&e.result_hash)
            && !e.frames.is_empty()
            && e.frames.len() <= 128
            && e.references.len() == e.frames.len()
            && e.edits.len() == e.frames.len() + usize::from(!keep)
            && e.config.len() <= 4096
            && e.before_diagnostics.len() <= 20_000
            && e.after_diagnostics.len() <= 20_000,
        "Body effect contract mismatch"
    );
    let mut expected = vec![];
    let vars = variables::analyze(declaration, source, d)?;
    ensure!(
        e.var_types.len() == vars.groups.len(),
        "Body var type witness coverage mismatch"
    );
    let mut hints = vec![];
    let mut seed_groups = vec![];
    let mut type_bytes = 0usize;
    for (index, (g, t)) in vars.groups.iter().zip(&e.var_types).enumerate() {
        d.check()?;
        let id = g.ids[0];
        type_bytes = type_bytes.saturating_add(t.r#type.len());
        ensure!(
            t.span.path == symbol.path
                && t.span.start_byte == id.start_byte()
                && t.span.end_byte == id.end_byte()
                && t.span.text == id.utf8_text(source.as_bytes())?
                && !t.r#type.is_empty()
                && t.r#type.len() <= 16_000
                && type_bytes <= 2_000_000,
            "Body var original type witness mismatch"
        );
        if let Some(original) = e.before.types.iter().find(|v| key(&v.span) == key(&t.span)) {
            ensure!(
                original.r#type == t.r#type,
                "Body var type contradicts declaration observation"
            );
        }
        hints.push(
            if g.annotation.is_none() && g.function.is_none() && variables::primitive(&t.r#type) {
                Some(t.r#type.clone())
            } else {
                None
            },
        );
        if g.annotation.is_none()
            && g.function.is_none()
            && g.parameter.is_none()
            && !variables::primitive(&t.r#type)
        {
            seed_groups.push(index);
        }
    }
    if hints.iter().all(Option::is_none) {
        hints.clear();
    }
    ensure!(
        e.var_assignments.len() == vars.ids.len(),
        "Body var assignment witness coverage mismatch"
    );
    let mut synthetic = BTreeSet::new();
    let mut call_names = BTreeSet::new();
    let mut rendered_bytes = 0usize;
    let mut copy_count = 0usize;
    for frame in &e.frames {
        d.check()?;
        ensure!(
            frame.var_types == hints && frame.var_seed_groups == seed_groups,
            "Body var frame type hints differ from original witnesses"
        );
        for name in std::iter::once(&frame.names.result)
            .chain(std::iter::once(&frame.names.exit))
            .chain(&frame.names.arguments)
            .chain(&frame.names.var_parameters)
            .chain(&frame.names.var_seeds)
            .chain(&frame.names.return_type)
            .chain(&frame.names.argument_capture)
        {
            d.check()?;
            ensure!(
                synthetic.insert(name.clone()),
                "Body name reused across frames"
            );
        }
        let call = node_at(tree, frame.call_start, frame.call_end, d)?;
        let name = call
            .child_by_field_name("function")
            .context("Body call name absent")?;
        call_names.insert((symbol.path.clone(), name.start_byte(), name.end_byte()));
        let rendered = render(tree, source, &symbol.language, declaration, frame, d)?;
        rendered_bytes = rendered_bytes.saturating_add(rendered.text.len());
        copy_count = copy_count.saturating_add(rendered.copies.len());
        ensure!(
            rendered_bytes <= 4_000_000 && copy_count <= 20_000,
            "Body aggregate frame output/correspondence capacity exceeded"
        );
        expected.push((
            caller_start(
                source,
                node_at(tree, frame.statement_start, frame.statement_end, d)?,
                d,
            )?,
            frame.statement_end,
            rendered.text,
            rendered.copies,
        ));
    }
    let before_hash = crate::model::hash(source);
    if !keep {
        ensure!(
            declaration.parent().is_some_and(|p| matches!(
                p.kind(),
                "program" | "statement_block" | "switch_case" | "switch_default"
            )) && symbol.start_byte == declaration.start_byte(),
            "Body exported/unsupported declaration must be retained"
        );
        // Keep a statement boundary even without attached callee docs: removing
        // a preceding token must not turn its trailing JSDoc into leading docs.
        let deletion = ";".into();
        expected.push((symbol.start_byte, symbol.end_byte, deletion, vec![]));
    }
    expected.sort_by_key(|e| e.0);
    ensure!(
        expected.windows(2).all(|p| p[0].1 <= p[1].0),
        "Body statement effects overlap"
    );
    ensure!(
        e.edits
            .iter()
            .zip(&expected)
            .all(|(a, b)| a.path == symbol.path
                && a.start_byte == b.0
                && a.end_byte == b.1
                && a.text == b.2),
        "Body compiler replacement differs from native source reconstruction"
    );
    let mut result = String::new();
    let mut origins = vec![];
    let mut cursor = 0;
    let mut changes = vec![];
    for (start, end, text, copies) in expected {
        d.check()?;
        let gap = crate::paths::utf8_range(source, cursor, start)?;
        let at = result.len();
        result.push_str(gap);
        origins.push(Origin {
            before_start: cursor,
            before_end: start,
            after_start: at,
            after_end: result.len(),
            role: "caller",
        });
        let at = result.len();
        for c in copies {
            origins.push(Origin {
                before_start: c.original_start,
                before_end: c.original_end,
                after_start: at + c.output_start,
                after_end: at + c.output_end,
                role: c.role,
            });
        }
        ensure!(
            result.len().saturating_add(text.len()) <= 4_000_000,
            "Body result capacity exceeded"
        );
        result.push_str(&text);
        changes.push(changes::Edit::Replace {
            path: symbol.path.clone(),
            expected_hash: before_hash.clone(),
            start_byte: start,
            end_byte: end,
            text,
        });
        cursor = end;
    }
    let at = result.len();
    result.push_str(crate::paths::utf8_range(source, cursor, source.len())?);
    origins.push(Origin {
        before_start: cursor,
        before_end: source.len(),
        after_start: at,
        after_end: result.len(),
        role: "caller",
    });
    ensure!(
        result.len() <= 4_000_000 && crate::model::hash(&result) == e.result_hash,
        "Body result integrity mismatch"
    );
    let result_tree = parse(&symbol.language, &result, d)?;
    let mut before = Contents::new();
    let mut bytes = 0usize;
    for f in &snapshot.files {
        d.check()?;
        bytes = bytes.saturating_add(f.size);
        ensure!(
            bytes <= 10_000_000,
            "Body captured-source capacity exceeded"
        );
        before.insert(
            f.path.clone(),
            String::from_utf8(engine.store.read_blob_exact(&f.hash, f.size, 10_000_000)?)?,
        );
    }
    let mut after = before.clone();
    after.insert(symbol.path.clone(), result.clone());
    let bm = bindings(&e.before, &before, d)?;
    let mut am = bindings(&e.after, &after, d)?;
    type_coverage(tree, &symbol.path, &e.before.types, d)?;
    type_coverage(&result_tree, &symbol.path, &e.after.types, d)?;
    let declared = declaration
        .child_by_field_name("name")
        .context("Body original name absent")?;
    let definition_key = (
        symbol.path.clone(),
        declared.start_byte(),
        declared.end_byte(),
    );
    let actual: BTreeSet<_> = bm
        .values()
        .filter(|b| {
            b.resolution == "resolved"
                && b.targets.len() == 1
                && matches!(&b.targets[0],Target::Captured{span}if key(span)==definition_key)
                && key(&b.span) != definition_key
        })
        .map(|b| key(&b.span))
        .collect();
    let reported: BTreeSet<_> = e.references.iter().map(key).collect();
    ensure!(
        actual == call_names && reported == call_names && reported.len() == e.references.len(),
        "Body bound call coverage mismatch"
    );
    for r in &e.references {
        exact(r, &before)?;
    }
    // Independent syntax coverage prevents omission of a selected-file identifier
    // from both compiler observation sets from disguising a lost consumer.
    let mut stack = vec![tree.root_node()];
    let mut count = 0usize;
    while let Some(n) = stack.pop() {
        d.check()?;
        count += 1;
        ensure!(
            count <= 200_000,
            "Body identifier coverage capacity exceeded"
        );
        if matches!(
            n.kind(),
            "identifier"
                | "type_identifier"
                | "property_identifier"
                | "shorthand_property_identifier"
                | "shorthand_property_identifier_pattern"
                | "statement_identifier"
        ) {
            ensure!(
                bm.contains_key(&(symbol.path.clone(), n.start_byte(), n.end_byte())),
                "Body original identifier coverage absent"
            );
        }
        stack.extend(children(n));
    }
    let aliases = vars.aliases();
    let mut checker = Checker {
        before: &before,
        after: &after,
        path: &symbol.path,
        origins: &origins,
        aliases: &aliases,
        spec,
        libraries: &report["standardLibraryHashes"],
        cache: BTreeMap::new(),
        bytes: 0,
    };
    // Removed call/declaration observations still need source-bound targets.
    for b in bm.values() {
        checker.targets(&b.targets, false, d)?;
    }
    let mut assignments = BTreeMap::new();
    for b in &e.var_assignments {
        d.check()?;
        exact(&b.span, &before)?;
        let site = (b.span.start_byte, b.span.end_byte);
        let node = vars
            .ids
            .get(&site)
            .context("Body var witness has no original binding site")?;
        ensure!(
            b.span.path == symbol.path
                && b.resolution == "resolved"
                && !b.targets.is_empty()
                && b.targets.len() <= 128
                && assignments.insert(key(&b.span), b).is_none(),
            "Body var assignment witness shape mismatch"
        );
        let original = vars.assignment_origin(*node, source, d)?;
        let expected = bm
            .get(&(symbol.path.clone(), original.0, original.1))
            .context("Body var original value binding absent")?;
        ensure!(
            checker.targets(&b.targets, false, d)?
                == checker.targets(&expected.targets, false, d)?,
            "Body var initialization changes its original lexical target"
        );
    }
    let mut compared = 0usize;
    let mut assignment_properties = 0usize;
    for b in bm.values() {
        d.check()?;
        let locations: Vec<_> = if b.span.path != symbol.path {
            vec![(key(&b.span), "caller")]
        } else {
            origins
                .iter()
                .filter(|o| b.span.start_byte >= o.before_start && b.span.end_byte <= o.before_end)
                .map(|o| {
                    (
                        (
                            symbol.path.clone(),
                            o.after_start + b.span.start_byte - o.before_start,
                            o.after_start + b.span.end_byte - o.before_start,
                        ),
                        o.role,
                    )
                })
                .collect()
        };
        if locations.is_empty() {
            ensure!(
                key(&b.span) == definition_key
                    || call_names.contains(&key(&b.span))
                    || b.span.path == symbol.path
                        && vars.removable(b.span.start_byte, b.span.end_byte),
                "Body removes an unexplained original identifier"
            );
            continue;
        }
        for (at, role) in locations {
            let original = if role == "var_assignment" {
                assignments.get(&key(&b.span)).copied().unwrap_or(b)
            } else {
                b
            };
            let a = am
                .remove(&at)
                .context("Body result binding coverage absent")?;
            if role == "var_property" {
                // A binding-pattern property read becomes an assignment-pattern
                // PropertyAssignment name. TypeScript reports that own syntax
                // symbol, not the original input-type property. Only the exact
                // independently reconstructed key may make this transition.
                ensure!(
                    b.span.text == a.span.text
                        && a.resolution == "resolved"
                        && a.targets.len() == 1
                        && matches!(&a.targets[0], Target::Captured{span} if key(span) == key(&a.span)),
                    "Body var assignment property lacks its own exact syntax target"
                );
                assignment_properties += 1;
                continue;
            }
            ensure!(
                b.span.text == a.span.text
                    && original.resolution == a.resolution
                    && checker.targets(&original.targets, false, d)?
                        == checker.targets(&a.targets, true, d)?,
                "Body changed an original static binding"
            );
            compared += 1;
        }
    }
    let mut const_assertions = 0usize;
    for a in am.values() {
        d.check()?;
        if a.span.path == symbol.path
            && a.span.text == "const"
            && matches!(a.resolution.as_str(), "unbound" | "no_declarations")
            && a.targets.is_empty()
            && fixed_const_assertion(&result_tree, a.span.start_byte, a.span.end_byte)
        {
            const_assertions += 1;
            continue;
        }
        ensure!(
            a.span.path == symbol.path && synthetic.contains(&a.span.text),
            "Body unexplained new identifier observation"
        );
        for t in &a.targets {
            ensure!(
                matches!(t,Target::Captured{span}if span.path==symbol.path&&synthetic.contains(&span.text)),
                "Body synthetic binding has foreign targets"
            );
            if let Target::Captured { span } = t {
                exact(span, &after)?;
            }
        }
    }
    let after_types: BTreeMap<_, _> = e.after.types.iter().map(|t| (key(&t.span), t)).collect();
    ensure!(
        after_types.len() == e.after.types.len(),
        "Body duplicate type observations"
    );
    let mut type_count = 0usize;
    for t in &e.before.types {
        d.check()?;
        exact(&t.span, &before)?;
        ensure!(t.r#type.len() <= 16_000, "Body type text capacity exceeded");
        let locations: Vec<_> = if t.span.path != symbol.path {
            vec![key(&t.span)]
        } else {
            origins
                .iter()
                .filter(|o| {
                    matches!(o.role, "caller" | "argument")
                        && t.span.start_byte >= o.before_start
                        && t.span.end_byte <= o.before_end
                })
                .map(|o| {
                    (
                        symbol.path.clone(),
                        o.after_start + t.span.start_byte - o.before_start,
                        o.after_start + t.span.end_byte - o.before_start,
                    )
                })
                .collect()
        };
        for at in locations {
            let a = after_types
                .get(&at)
                .context("Body surviving declaration type observation missing")?;
            exact(&a.span, &after)?;
            ensure!(
                t.r#type == a.r#type,
                "Body surviving declaration type changed"
            );
            type_count += 1;
        }
    }
    Ok((
        changes,
        result,
        json!({"knownStaticBindingsCompared":compared,"fixedConstTypeAssertionsChecked":const_assertions,"survivingDeclarationTypesCompared":type_count,"sourceReconstruction":"original_function_body_frames_v1","varAssignmentPropertiesChecked":assignment_properties,"functionVarGroups":vars.groups.iter().map(|g|json!({"name":g.name,"declarations":g.ids.iter().map(|n|json!({"startByte":n.start_byte(),"endByte":n.end_byte()})).collect::<Vec<_>>(),"parameter":g.parameter.map(|n|json!({"startByte":n.start_byte(),"endByte":n.end_byte()})),"function":g.function.map(|n|json!({"startByte":n.start_byte(),"endByte":n.end_byte()}))})).collect::<Vec<_>>(),"complete":false}),
    ))
}
#[allow(clippy::too_many_arguments)]
pub(crate) fn prepare(
    engine: &mut Engine,
    spec: &CompilerSpec,
    snapshot: &Snapshot,
    graph: &Graph,
    symbol: &Entity,
    source: &str,
    tree: &Tree,
    declaration: Node<'_>,
    keep: bool,
    binding: &str,
    d: &Deadline,
) -> Result<Value> {
    let intent = json!({"path":symbol.path,"name":symbol.name,"startByte":symbol.start_byte,"endByte":symbol.end_byte,"nameStartByte":symbol.name_start_byte,"nameEndByte":symbol.name_end_byte,"keepDefinition":keep});
    let report = crate::projects::execute(
        &engine.store,
        snapshot,
        spec,
        "inlineBody",
        Some(intent.clone()),
        d,
    )?;
    ensure!(
        host_binding(spec, d)? == binding
            && report["compilerHash"] == host_file(&spec.typescript, 16_000_000, d)?.1,
        "Body compiler host binding changed"
    );
    let effect: Effect = crate::json::decode(report["bodyInline"].clone())?;
    let (edits, result, proof) = check(
        engine,
        spec,
        snapshot,
        symbol,
        source,
        tree,
        declaration,
        keep,
        &effect,
        &report,
        d,
    )?;
    ensure!(
        effect.config == "[inferred]"
            || snapshot
                .files
                .iter()
                .any(|f| f.path == effect.config && f.path.ends_with(".json")),
        "Body compiler configuration outside capture"
    );
    diagnostic_spans(
        engine,
        snapshot,
        &symbol.path,
        source,
        &effect.config,
        &effect.before_diagnostics,
        d,
    )?;
    diagnostic_spans(
        engine,
        snapshot,
        &symbol.path,
        &result,
        &effect.config,
        &effect.after_diagnostics,
        d,
    )?;
    let plan = changes::prepare(
        &mut engine.store,
        snapshot,
        graph,
        &json!({"edits":edits}),
        d,
    )?;
    Ok(
        json!({"snapshotId":snapshot.id,"actions":[{"kind":effect.action_kind,"usable":true,"plan":plan}],"provider":"native_function_body_with_host_typescript_bindings","compiler":"5.8.3","compilerHash":report["compilerHash"],"hostBinding":binding,"standardLibraryHashes":report["standardLibraryHashes"],"compilerContextId":effect.context_id,"compilerConfig":effect.config,"intent":intent,"sourceBindings":report["sourceBindings"],"sourceHash":crate::model::hash(source),"effectDigest":identity(&report["bodyInline"]),"bindingCheck":proof,"beforeDiagnostics":effect.before_diagnostics,"afterDiagnostics":effect.after_diagnostics,"limitations":report["limitations"],"equivalent":"native_classic_function_body_frames","complete":false,"commandsExecuted":false,"sourceExecuted":false,"sourceTreeWritten":false,"verificationRequired":true}),
    )
}
