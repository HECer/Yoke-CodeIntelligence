//! Compare actual compiler observations through a native structural correspondence.
use super::{
    Binding, Contents, Declaration, Effect, Import, Key, Module, NamedBinding, Observations, Span,
    Target, ast,
};
use crate::{
    Engine,
    capture::Deadline,
    model::{Snapshot, identity},
    projects::CompilerSpec,
};
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::Read,
};

#[derive(Default)]
struct Correspondence {
    before: BTreeMap<Key, String>,
    after: BTreeMap<Key, String>,
    names: BTreeMap<String, (String, String)>,
    pairs: Vec<(String, ast::Event, ast::Event)>,
}
impl Correspondence {
    fn add(
        &mut self,
        scope: &str,
        path_before: &str,
        path_after: &str,
        before: &ast::Projection,
        after: &ast::Projection,
    ) -> Result<()> {
        ensure!(
            before.events.len() == after.events.len(),
            "Move changes unrelated syntax or declaration multiplicity"
        );
        for (i, (b, a)) in before.events.iter().zip(&after.events).enumerate() {
            ensure!(
                b.signature == a.signature,
                "Move changes an original operator/literal/statement/declaration shape"
            );
            let id = format!("{scope}:{i}");
            for (s, e) in &b.aliases {
                self.before.insert((path_before.into(), *s, *e), id.clone());
            }
            for (s, e) in &a.aliases {
                self.after.insert((path_after.into(), *s, *e), id.clone());
            }
            if b.identifier && a.identifier {
                self.names
                    .insert(id.clone(), (b.text.clone(), a.text.clone()));
                self.pairs.push((id, b.clone(), a.clone()));
            }
        }
        Ok(())
    }
}
struct Checker<'a> {
    before: &'a Contents,
    after: &'a Contents,
    changed: &'a BTreeSet<String>,
    mapping: &'a Correspondence,
    spec: &'a CompilerSpec,
    library_hashes: &'a Value,
    libraries: BTreeMap<String, String>,
    library_bytes: usize,
}
impl Checker<'_> {
    fn ids(
        &mut self,
        resolution: &str,
        targets: &[Target],
        after: bool,
        d: &Deadline,
    ) -> Result<Vec<String>> {
        ensure!(
            targets.len() <= 128
                && matches!(resolution, "resolved" | "unbound" | "no_declarations")
                && (resolution == "resolved") == !targets.is_empty(),
            "Move binding resolution/target shape mismatch"
        );
        let contents = if after { self.after } else { self.before };
        let map = if after {
            &self.mapping.after
        } else {
            &self.mapping.before
        };
        let mut result = Vec::new();
        for target in targets {
            d.check()?;
            let id = match target {
                Target::Captured { span } => {
                    span.check(contents)?;
                    if self.changed.contains(&span.path) {
                        let id = map
                            .get(&span.key())
                            .context("Move definition has no independent syntax correspondence")?;
                        if let Some((b, a)) = self.mapping.names.get(id) {
                            ensure!(
                                b == a,
                                "Move renames an original declaration/property/parameter"
                            );
                        }
                        id.clone()
                    } else {
                        identity(&(
                            "unchanged_compiler_move_anchor_v1",
                            &span.path,
                            span.start_byte,
                            span.end_byte,
                            &span.text,
                        ))
                    }
                }
                Target::Module { path } => {
                    ensure!(
                        contents.contains_key(path),
                        "Move module binding outside captured files"
                    );
                    format!("module:{path}")
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
                            && regex::Regex::new(r"\Alib(?:\.[a-z0-9.-]+)?\.d\.ts\z")?
                                .is_match(name)
                            && self.library_hashes[name] == *hash,
                        "Move trusted-library binding mismatch"
                    );
                    if !self.libraries.contains_key(name) {
                        let file = self
                            .spec
                            .typescript
                            .canonicalize()?
                            .parent()
                            .context("Move compiler library directory absent")?
                            .join(name);
                        let mut file = fs::File::open(file)?;
                        let size = usize::try_from(file.metadata()?.len())?;
                        self.library_bytes = self.library_bytes.saturating_add(size);
                        ensure!(
                            file.metadata()?.is_file()
                                && size <= 4_000_000
                                && self.library_bytes <= 16_000_000,
                            "Move trusted-library source capacity exceeded"
                        );
                        let mut bytes = Vec::with_capacity(size);
                        file.by_ref()
                            .take(size as u64 + 1)
                            .read_to_end(&mut bytes)?;
                        ensure!(
                            bytes.len() == size && crate::model::hash(&bytes) == *hash,
                            "Move trusted-library bytes changed"
                        );
                        self.libraries
                            .insert(name.clone(), String::from_utf8(bytes)?);
                    }
                    ensure!(
                        crate::paths::utf8_range(&self.libraries[name], *start_byte, *end_byte)?
                            == text,
                        "Move library definition span mismatch"
                    );
                    identity(&(
                        "compiler_move_library_v1",
                        name,
                        hash,
                        start_byte,
                        end_byte,
                        text,
                    ))
                }
            };
            result.push(id);
        }
        Ok(result)
    }
    fn pair(&mut self, b: &Binding, a: &Binding, d: &Deadline) -> Result<bool> {
        b.span.check(self.before)?;
        a.span.check(self.after)?;
        ensure!(
            b.resolution == a.resolution,
            "Move loses or invents compiler binding coverage"
        );
        let bi = self.ids(&b.resolution, &b.targets, false, d)?;
        let ai = self.ids(&a.resolution, &a.targets, true, d)?;
        ensure!(
            bi == ai,
            "Move changed a known compiler binding or declaration order"
        );
        if b.resolution != "resolved" {
            ensure!(
                b.span.text == a.span.text,
                "Move changes an unresolved/unmodeled identifier"
            );
            return Ok(false);
        }
        Ok(true)
    }
}
fn binding_map<'a>(
    observations: &'a Observations,
    contents: &Contents,
    d: &Deadline,
) -> Result<BTreeMap<Key, &'a Binding>> {
    ensure!(
        observations.bindings.len() <= 100_000 && observations.modules.len() <= 20_000,
        "Move observation capacity exceeded"
    );
    let mut result = BTreeMap::new();
    for binding in &observations.bindings {
        d.check()?;
        binding.span.check(contents)?;
        ensure!(
            result.insert(binding.span.key(), binding).is_none(),
            "Move repeated identifier observation"
        );
    }
    Ok(result)
}
fn module_map(observations: &Observations) -> Result<BTreeMap<&str, &Module>> {
    let mut result = BTreeMap::new();
    for module in &observations.modules {
        ensure!(
            module.exports.len() <= 20_000
                && module.imports.len() <= 2000
                && result.insert(module.path.as_str(), module).is_none(),
            "Move module observation shape/capacity mismatch"
        );
    }
    Ok(result)
}
fn exports(
    checker: &mut Checker<'_>,
    module: &Module,
    after: bool,
    d: &Deadline,
) -> Result<BTreeMap<String, Vec<String>>> {
    let mut result = BTreeMap::new();
    for NamedBinding {
        name,
        resolution,
        targets,
    } in &module.exports
    {
        d.check()?;
        ensure!(
            !name.is_empty() && name.len() <= 4096,
            "Move export name capacity exceeded"
        );
        let ids = checker.ids(resolution, targets, after, d)?;
        ensure!(
            !ids.is_empty() && result.insert(name.clone(), ids).is_none(),
            "Move unresolved/duplicate export observation"
        );
    }
    Ok(result)
}
fn import_keys(
    checker: &mut Checker<'_>,
    imports: &[Import],
    after: bool,
    d: &Deadline,
) -> Result<BTreeSet<String>> {
    let contents = if after { checker.after } else { checker.before };
    let mut result = BTreeSet::new();
    for import in imports {
        d.check()?;
        import.span.check(contents)?;
        ensure!(
            import.names.len() <= 2000 && matches!(import.kind.as_str(), "import" | "reexport"),
            "Move import name capacity exceeded"
        );
        let modules = checker.ids(&import.module.resolution, &import.module.targets, after, d)?;
        ensure!(
            !modules.is_empty() && modules.iter().all(|id| id.starts_with("module:")),
            "Move import module is not captured"
        );
        for name in &import.names {
            name.span.check(contents)?;
            ensure!(
                name.span.path == import.span.path
                    && name.span.start_byte >= import.span.start_byte
                    && name.span.end_byte <= import.span.end_byte,
                "Move import name source ownership mismatch"
            );
            let targets = checker.ids(&name.resolution, &name.targets, after, d)?;
            ensure!(!targets.is_empty(), "Move import binding is unavailable");
            result.insert(identity(&(
                &import.kind,
                &modules,
                &name.span.text,
                &targets,
            )));
        }
    }
    Ok(result)
}
fn inside(span: &Span, ranges: &[(usize, usize)]) -> bool {
    ranges
        .iter()
        .any(|(s, e)| span.start_byte >= *s && span.end_byte <= *e)
}

#[allow(clippy::too_many_arguments)]
pub(super) fn check(
    engine: &Engine,
    spec: &CompilerSpec,
    snapshot: &Snapshot,
    symbol: &Declaration,
    destination: &str,
    before: &Contents,
    after: &Contents,
    effect: &Effect,
    report: &Value,
    d: &Deadline,
) -> Result<Value> {
    let changed: BTreeSet<_> = effect.result_hashes.keys().cloned().collect();
    let bm = binding_map(&effect.before, before, d)?;
    let am = binding_map(&effect.after, after, d)?;
    let bmods = module_map(&effect.before)?;
    let amods = module_map(&effect.after)?;
    ensure!(
        bmods.keys().eq(amods.keys())
            && bmods.contains_key(symbol.path.as_str())
            && bmods.contains_key(destination)
            && bmods.keys().all(|path| before.contains_key(*path)),
        "Move compiler program source coverage changed"
    );
    ensure!(
        bmods[&*symbol.path].is_module && bmods[destination].is_module,
        "Move input modules are not ES module scopes"
    );
    let epoch =
        crate::capture::analyzer_epoch_for_snapshot_with_deadline(&engine.store, snapshot, d)?;
    let st = ast::parse(&symbol.language, &before[&symbol.path], d)?;
    let selected = ast::selection(&st, symbol, &before[&symbol.path], epoch, d)?;
    let target_language = &snapshot
        .files
        .iter()
        .find(|f| f.path == destination)
        .context("Move destination absent")?
        .language;
    let tt = ast::parse(target_language, &after[destination], d)?;
    let moved = ast::named_statement(&tt, &after[destination], &symbol.name, d)?
        .context("Move did not insert the selected declaration")?;
    ensure!(
        selected.node.kind() == moved.node.kind(),
        "Move changes the selected declaration kind"
    );
    let old_name = Span {
        path: symbol.path.clone(),
        start_byte: symbol
            .name_start_byte
            .context("Move original name absent")?,
        end_byte: symbol.name_end_byte.context("Move original name absent")?,
        text: symbol.name.clone(),
    };
    let new_name = ast::span(
        destination,
        moved
            .node
            .child_by_field_name("name")
            .context("Moved declaration name absent")?,
        &after[destination],
    )?;
    ensure!(
        old_name.text == new_name.text,
        "Move changes the selected name"
    );
    let mut mapping = Correspondence::default();
    let moved_before = ast::project(
        &st,
        &symbol.path,
        &before[&symbol.path],
        &bm,
        &old_name,
        &symbol.path,
        None,
        Some(&selected),
        d,
    )?;
    let moved_after = ast::project(
        &tt,
        destination,
        &after[destination],
        &am,
        &new_name,
        destination,
        None,
        Some(&moved),
        d,
    )?;
    mapping.add(
        "selected",
        &symbol.path,
        destination,
        &moved_before,
        &moved_after,
    )?;
    let mut new_linkage = BTreeMap::new();
    let mut empty = BTreeMap::new();
    for path in &changed {
        d.check()?;
        let file = snapshot
            .files
            .iter()
            .find(|file| file.path == *path)
            .context("Move changed source absent")?;
        let bt = ast::parse(&file.language, &before[path], d)?;
        let at = ast::parse(&file.language, &after[path], d)?;
        ast::preamble(&before[path], &after[path])?;
        ensure!(
            ast::directives(&bt, &before[path], d)? == ast::directives(&at, &after[path], d)?,
            "Move changes an original directive prologue"
        );
        let bsel = if path == &symbol.path {
            Some(ast::selection(&bt, symbol, &before[path], epoch, d)?)
        } else {
            None
        };
        let asel = if path == destination {
            Some(
                ast::named_statement(&at, &after[path], &symbol.name, d)?
                    .context("Moved declaration absent")?,
            )
        } else {
            None
        };
        if path == &symbol.path {
            ensure!(
                ast::named_statement(&at, &after[path], &symbol.name, d)?.is_none(),
                "Move leaves or recreates the original declaration"
            );
        }
        if path == destination {
            ensure!(
                ast::named_statement(&bt, &before[path], &symbol.name, d)?.is_none(),
                "Move target declaration already existed"
            );
        }
        let b = ast::project(
            &bt,
            path,
            &before[path],
            &bm,
            &old_name,
            &symbol.path,
            bsel.as_ref(),
            None,
            d,
        )?;
        let a = ast::project(
            &at,
            path,
            &after[path],
            &am,
            &new_name,
            destination,
            asel.as_ref(),
            None,
            d,
        )?;
        ensure!(
            b.side_effect_imports == a.side_effect_imports,
            "Move changes standalone side-effect imports"
        );
        for (projection, module) in [(&b, bmods[path.as_str()]), (&a, amods[path.as_str()])] {
            let reported: Vec<_> = module
                .imports
                .iter()
                .map(|import| {
                    (
                        import.span.start_byte,
                        import.span.end_byte,
                        import.kind.clone(),
                    )
                })
                .collect();
            ensure!(
                projection.module_links == reported
                    && module
                        .imports
                        .iter()
                        .all(|import| import.span.path == *path),
                "Move module linkage coverage differs from independent syntax"
            );
        }
        if bmods[path.as_str()].is_module != amods[path.as_str()].is_module {
            ensure!(
                path == &symbol.path && a.declaration_empty,
                "Move changes a surviving module/script boundary"
            );
        }
        empty.insert(path.clone(), a.declaration_empty);
        new_linkage.insert(path.clone(), a.linkage.clone());
        mapping.add(&format!("body:{path}"), path, path, &b, &a)?;
    }
    let mut checker = Checker {
        before,
        after,
        changed: &changed,
        mapping: &mapping,
        spec,
        library_hashes: &report["standardLibraryHashes"],
        libraries: BTreeMap::new(),
        library_bytes: 0,
    };
    let selected_id = checker.ids(
        "resolved",
        &[Target::Captured {
            span: old_name.clone(),
        }],
        false,
        d,
    )?;
    ensure!(
        selected_id
            == checker.ids(
                "resolved",
                &[Target::Captured {
                    span: new_name.clone()
                }],
                true,
                d
            )?,
        "Move selected binding correspondence mismatch"
    );
    let mut known = 0usize;
    let mut unknown = 0usize;
    let mut free = BTreeSet::new();
    for (id, b, a) in &mapping.pairs {
        d.check()?;
        let bkey = if id.starts_with("selected:") {
            (
                symbol.path.clone(),
                b.binding.unwrap_or(b.range).0,
                b.binding.unwrap_or(b.range).1,
            )
        } else {
            let prefix = id
                .rsplit_once(':')
                .context("Move correspondence scope invalid")?
                .0;
            let path = prefix
                .strip_prefix("body:")
                .context("Move correspondence file scope invalid")?;
            (
                path.into(),
                b.binding.unwrap_or(b.range).0,
                b.binding.unwrap_or(b.range).1,
            )
        };
        let akey = if id.starts_with("selected:") {
            (
                destination.into(),
                a.binding.unwrap_or(a.range).0,
                a.binding.unwrap_or(a.range).1,
            )
        } else {
            (
                bkey.0.clone(),
                a.binding.unwrap_or(a.range).0,
                a.binding.unwrap_or(a.range).1,
            )
        };
        match (bm.get(&bkey), am.get(&akey)) {
            (Some(b), Some(a)) => {
                if checker.pair(b, a, d)? {
                    known += 1;
                } else {
                    unknown += 1;
                }
                if id.starts_with("selected:") {
                    free.extend(
                        checker
                            .ids(&b.resolution, &b.targets, false, d)?
                            .into_iter()
                            .filter(|id| id.starts_with(&format!("body:{}:", symbol.path))),
                    );
                }
            }
            (None, None) => {
                ensure!(
                    b.text == a.text,
                    "Move changes an unmodeled identifier atom"
                );
                unknown += 1;
            }
            _ => anyhow::bail!("Move identifier observation coverage changed"),
        }
    }
    // Unchanged source still participates: a changed export/import must not silently
    // rebind a consumer whose bytes happened to need no compiler edit.
    for (key, b) in &bm {
        if !changed.contains(&key.0) {
            let a = am
                .get(key)
                .context("Move loses unchanged-file binding coverage")?;
            if checker.pair(b, a, d)? {
                known += 1;
            } else {
                unknown += 1;
            }
        }
    }
    ensure!(
        am.keys()
            .filter(|key| !changed.contains(&key.0))
            .all(|key| bm.contains_key(key)),
        "Move invents unchanged-file binding coverage"
    );
    let mut added_exports = Vec::new();
    let mut old_source_names = BTreeSet::from([symbol.name.clone()]);
    for (name, ids) in exports(&mut checker, bmods[&*symbol.path], false, d)? {
        if ids == selected_id {
            old_source_names.insert(name);
        }
    }
    for (path, module) in &bmods {
        let a = amods[path];
        let be = exports(&mut checker, module, false, d)?;
        let ae = exports(&mut checker, a, true, d)?;
        for (name, ids) in &be {
            if *path == symbol.path && *ids == selected_id {
                ensure!(
                    ae.get(name).is_none_or(|new| new == ids),
                    "Move rebinds an original selected export"
                );
            } else {
                ensure!(
                    ae.get(name) == Some(ids),
                    "Move drops/rebinds an unrelated original export"
                );
            }
        }
        for (name, ids) in &ae {
            if be.contains_key(name) {
                continue;
            }
            let allowed_selected =
                *path == destination && *ids == selected_id && old_source_names.contains(name);
            let allowed_dependency = *path == symbol.path
                && ids.len() == 1
                && free.contains(&ids[0])
                && mapping
                    .names
                    .get(&ids[0])
                    .is_some_and(|(b, a)| b == name && a == name);
            ensure!(
                allowed_selected || allowed_dependency,
                "Move adds an unrelated public export"
            );
            added_exports.push(json!({"path":path,"name":name,"reason":if allowed_selected{"moved_declaration"}else{"source_bound_free_dependency"}}));
        }
    }
    // Import rewrites may preserve an old binding or serve an actual post-move
    // consumer. New unused imports and uncaptured modules cannot hide side effects.
    for (path, module) in &amods {
        let original = import_keys(&mut checker, &bmods[path].imports, false, d)?;
        let _ = import_keys(&mut checker, &module.imports, true, d)?;
        let mut used = BTreeSet::new();
        for ids in exports(&mut checker, module, true, d)?.values() {
            used.extend(ids.iter().cloned());
        }
        for binding in effect
            .after
            .bindings
            .iter()
            .filter(|b| b.span.path == *path)
        {
            if new_linkage
                .get(*path)
                .is_some_and(|ranges| inside(&binding.span, ranges))
            {
                continue;
            }
            if binding.targets.iter().any(
                |target| matches!(target,Target::Captured{span}if span.key()==binding.span.key()),
            ) {
                continue;
            }
            used.extend(checker.ids(&binding.resolution, &binding.targets, true, d)?);
        }
        for import in &module.imports {
            let modules =
                checker.ids(&import.module.resolution, &import.module.targets, true, d)?;
            for name in &import.names {
                let targets = checker.ids(&name.resolution, &name.targets, true, d)?;
                let preserved = original.contains(&identity(&(
                    &import.kind,
                    &modules,
                    &name.span.text,
                    &targets,
                )));
                ensure!(
                    preserved || targets.iter().any(|id| used.contains(id)),
                    "Move adds an unused import binding"
                );
            }
        }
    }
    for (diagnostics, contents) in [
        (&effect.before_diagnostics, before),
        (&effect.after_diagnostics, after),
    ] {
        ensure!(
            diagnostics.len() <= 20_000,
            "Move diagnostic capacity exceeded"
        );
        for diag in diagnostics {
            d.check()?;
            ensure!(
                diag["severity"] == "warning" && diag["config"] == effect.config,
                "Move diagnostic severity/context mismatch"
            );
            if let Some(text) = diag["text"].as_str() {
                let s = Span {
                    path: diag["path"]
                        .as_str()
                        .context("Move diagnostic path absent")?
                        .into(),
                    start_byte: diag["startByte"]
                        .as_u64()
                        .context("Move diagnostic start absent")?
                        as usize,
                    end_byte: diag["endByte"]
                        .as_u64()
                        .context("Move diagnostic end absent")?
                        as usize,
                    text: text.into(),
                };
                s.check(contents)?;
            }
        }
    }
    Ok(
        json!({"knownStaticBindingsCompared":known,"unresolvedOrUnmodeledAtoms":unknown,"mergedDeclarationOrderPreserved":true,"addedExports":added_exports,"sourceBecameDeclarationEmpty":empty.get(&symbol.path),"semantics":"actual_compiler_observations_through_native_source_syntax_correspondence_not_runtime_or_exhaustive_reference_proof","complete":false}),
    )
}
