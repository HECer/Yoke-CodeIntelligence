//! Finite, source-bound propagation through one captured ES function module.
use super::provider;
use crate::{
    Engine, analysis,
    capture::Deadline,
    changes::{self, Edit},
    model::{Entity, Graph, Snapshot, SourceFile, identity},
};
use anyhow::{Context, Result, ensure};
use serde::Serialize;
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    ops::Range,
};
use tree_sitter::{Node, Parser, Tree};

#[path = "semantic_propagated_delete_ast.rs"]
mod result_ast;

const UNSUPPORTED: &str = "Propagation requires one captured TypeScript/JavaScript ES module of named function declarations and independently removable call statements";
const CAPACITY: &str = "propagated_delete_capacity";
const MAX_DECLARATIONS: usize = 16;
const MAX_REFERENCES: usize = 4000;
const MAX_USAGES: usize = 128;
const DIRECT: &str = "captured_es_function_propagation_v1";
const WRAPPED: &str = "captured_es_function_propagation_v2";
const BINDINGS: &str = "captured_es_function_propagation_v3";
const MAX_WRAPPERS: usize = 32;

pub(super) fn validate_policy(args: &Value) -> Result<()> {
    if let Some(policy) = args.get("propagationPolicy") {
        ensure!(
            matches!(policy.as_str(), Some(DIRECT | WRAPPED | BINDINGS))
                && args["propagate"] == true
                && args["namePath"]
                    .as_str()
                    .is_some_and(|name| !name.is_empty()),
            "Propagation policy requires a supported policy, propagate=true and a symbol selection"
        );
    }
    Ok(())
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct Location {
    path: String,
    start_byte: usize,
    end_byte: usize,
}
impl Location {
    fn within(&self, path: &str, span: &Range<usize>) -> bool {
        self.path == path && self.start_byte >= span.start && self.end_byte <= span.end
    }
}
struct Declaration {
    entity: Entity,
    whole: Range<usize>,
    exported: bool,
    reported: Vec<Location>,
    possible: Vec<Location>,
    external_literal: bool,
}
struct Module {
    tree: Tree,
    declarations: Vec<Declaration>,
}

fn declaration<'a>(node: Node<'a>, bindings: bool) -> Result<Node<'a>> {
    if node.kind() == "function_declaration" && node.child_by_field_name("body").is_some() {
        return Ok(node);
    }
    ensure!(
        bindings && matches!(node.kind(), "lexical_declaration" | "variable_declaration"),
        UNSUPPORTED
    );
    ensure!(
        node.named_child_count() == 1
            && node
                .child(0)
                .is_some_and(|token| matches!(token.kind(), "const" | "let" | "var")),
        "Propagation requires one statement function-valued binding"
    );
    let binding = node.named_child(0).context(UNSUPPORTED)?;
    let name = binding.child_by_field_name("name").context(UNSUPPORTED)?;
    let value = binding.child_by_field_name("value").context(UNSUPPORTED)?;
    ensure!(
        binding.kind() == "variable_declarator"
            && name.kind() == "identifier"
            && matches!(value.kind(), "arrow_function" | "function_expression")
            && value.child_by_field_name("body").is_some()
            && value.child_by_field_name("name").is_none(),
        "Propagation requires a plain binding to an arrow or anonymous function"
    );
    Ok(binding)
}

fn parse(
    file: &SourceFile,
    text: &str,
    graph: &Graph,
    selected: &Entity,
    bindings: bool,
    d: &Deadline,
) -> Result<Module> {
    ensure!(text.len() <= 256_000, CAPACITY);
    let mut parser = Parser::new();
    parser.set_language(&analysis::grammar(&file.language).context(UNSUPPORTED)?)?;
    let mut cancelled = |_: &tree_sitter::ParseState| d.check().is_err();
    let tree = parser
        .parse_with_options(
            &mut |at, _| &text.as_bytes()[at.min(text.len())..],
            None,
            Some(tree_sitter::ParseOptions::new().progress_callback(&mut cancelled)),
        )
        .context("Propagation parser cancelled")?;
    d.check()?;
    ensure!(!tree.root_node().has_error(), UNSUPPORTED);
    let mut declarations = Vec::new();
    let mut exported_any = false;
    let mut names = BTreeSet::new();
    let mut cursor = tree.root_node().walk();
    for root in tree.root_node().named_children(&mut cursor) {
        d.check()?;
        if matches!(
            root.kind(),
            "comment" | "expression_statement" | "empty_statement"
        ) {
            continue;
        }
        let exported = root.kind() == "export_statement";
        let node = if exported {
            root.child_by_field_name("declaration")
                .context(UNSUPPORTED)?
        } else {
            root
        };
        let node = declaration(node, bindings)?;
        let name = node.child_by_field_name("name").context(UNSUPPORTED)?;
        let name_text = name.utf8_text(text.as_bytes())?;
        ensure!(
            !name_text.contains('\\') && names.insert(name_text.to_string()),
            "Propagation requires unique unescaped module function names"
        );
        let matches: Vec<_> = graph
            .entities
            .iter()
            .filter(|entity| {
                entity.path == file.path
                    && entity.language == file.language
                    && entity.kind == "function"
                    && entity.start_byte == node.start_byte()
                    && entity.end_byte == node.end_byte()
                    && entity.name_start_byte == Some(name.start_byte())
                    && entity.name_end_byte == Some(name.end_byte())
                    && entity.name == name_text
            })
            .take(2)
            .collect();
        ensure!(
            matches.len() == 1,
            "Propagation exact declaration/source binding missing"
        );
        ensure!(declarations.len() < MAX_DECLARATIONS, CAPACITY);
        exported_any |= exported;
        declarations.push(Declaration {
            entity: (*matches[0]).clone(),
            whole: root.byte_range(),
            exported,
            reported: Vec::new(),
            possible: Vec::new(),
            external_literal: false,
        });
    }
    ensure!(
        exported_any
            && declarations
                .iter()
                .any(|item| item.entity.id == selected.id),
        UNSUPPORTED
    );
    drop(cursor);
    Ok(Module { tree, declarations })
}

fn context(value: &Value) -> Result<Value> {
    ensure!(
        value["serverProfileHash"].is_string() && value["providerSourceOmissions"].is_array(),
        "Propagation provider/source context missing"
    );
    let result = json!({"serverProfileHash":value["serverProfileHash"],"serverProfileBinding":value["serverProfileBinding"],"providerSourceOmissions":value["providerSourceOmissions"],"providerSourcePolicy":value["providerSourcePolicy"],"sourceExecutionAuthorized":value["sourceExecutionAuthorized"]});
    ensure!(serde_json::to_vec(&result)?.len() <= 64_000, CAPACITY);
    Ok(result)
}

fn inventory(
    engine: &Engine,
    snapshot: &Snapshot,
    selected: &Entity,
    d: &Deadline,
) -> Result<BTreeMap<String, String>> {
    ensure!(
        snapshot.kind == "disk" && snapshot.files.len() <= 2000,
        CAPACITY
    );
    let scope = crate::capture::source_scope_policy(&engine.store, snapshot)?;
    scope.check_host_write_scope(&selected.path)?;
    let mut result = BTreeMap::new();
    let mut bytes = 0usize;
    for file in &snapshot.files {
        d.check()?;
        bytes = bytes.checked_add(file.size).context(CAPACITY)?;
        ensure!(bytes <= 16_000_000, CAPACITY);
        result.insert(
            file.path.clone(),
            String::from_utf8(scope.read_source(&file.path)?)?,
        );
    }
    Ok(result)
}

fn originals(engine: &Engine, snapshot: &Snapshot, d: &Deadline) -> Result<()> {
    for file in &snapshot.files {
        d.check()?;
        let path = crate::paths::scoped(&engine.root, &file.path)?;
        ensure!(
            crate::verify::file_integrity(file, &path, d)?,
            "Propagation captured physical source became stale"
        );
    }
    Ok(())
}

fn add_possible(
    declaration: &mut Declaration,
    location: Location,
    total: &mut usize,
) -> Result<()> {
    *total += 1;
    ensure!(*total <= MAX_REFERENCES, CAPACITY);
    declaration.possible.push(location);
    Ok(())
}

fn graph_candidates(
    module: &mut Module,
    snapshot: &Snapshot,
    graph: &Graph,
    texts: &BTreeMap<String, String>,
    total: &mut usize,
    d: &Deadline,
) -> Result<()> {
    ensure!(
        graph.observations.len() <= 200_000 && graph.evidence.len() <= 200_000,
        CAPACITY
    );
    let mut selected = Vec::new();
    let mut needed = BTreeSet::new();
    for observation in &graph.observations {
        d.check()?;
        if observation.relation == "contains" {
            continue;
        }
        for (index, declaration) in module.declarations.iter().enumerate() {
            if observation.target.as_deref() == Some(declaration.entity.id.as_str())
                || observation.target_label == declaration.entity.name
                || observation.target_label.rsplit(['.', ':', '/']).next()
                    == Some(declaration.entity.name.as_str())
            {
                ensure!(
                    observation.snapshot_id == snapshot.id && !observation.evidence_ids.is_empty(),
                    "Propagation graph usage has no original evidence"
                );
                for id in &observation.evidence_ids {
                    ensure!(selected.len() < MAX_REFERENCES, CAPACITY);
                    selected.push((index, id.as_str()));
                    needed.insert(id.as_str());
                }
            }
        }
    }
    let mut evidence = BTreeMap::new();
    for item in &graph.evidence {
        d.check()?;
        if needed.contains(item.id.as_str()) {
            ensure!(
                evidence.insert(item.id.as_str(), item).is_none(),
                "Propagation graph evidence is ambiguous"
            );
        }
    }
    for (index, id) in selected {
        d.check()?;
        let item = evidence
            .get(id)
            .context("Propagation graph evidence missing")?;
        let file = snapshot
            .files
            .iter()
            .find(|file| file.path == item.path)
            .context("Propagation graph source absent")?;
        let text = texts
            .get(&item.path)
            .context("Propagation graph source outside captured scope")?;
        ensure!(
            item.snapshot_id == snapshot.id && item.content_hash == file.hash,
            "Propagation graph source binding differs"
        );
        crate::paths::utf8_range(text, item.start_byte, item.end_byte)?;
        add_possible(
            &mut module.declarations[index],
            Location {
                path: item.path.clone(),
                start_byte: item.start_byte,
                end_byte: item.end_byte,
            },
            total,
        )?;
    }
    Ok(())
}

fn lexical_candidates(
    module: &mut Module,
    path: &str,
    text: &str,
    texts: &BTreeMap<String, String>,
    total: &mut usize,
    d: &Deadline,
) -> Result<()> {
    let mut stack = vec![module.tree.root_node()];
    let mut count = 0usize;
    while let Some(node) = stack.pop() {
        d.check()?;
        count += 1;
        ensure!(count <= 200_000, CAPACITY);
        let identifier = matches!(
            node.kind(),
            "identifier"
                | "property_identifier"
                | "shorthand_property_identifier"
                | "shorthand_property_identifier_pattern"
        );
        let opaque = matches!(
            node.kind(),
            "comment" | "string" | "template_string" | "regex"
        );
        if identifier || opaque {
            let raw = node.utf8_text(text.as_bytes())?;
            for declaration in &mut module.declarations {
                let own_name = node.start_byte() == declaration.entity.name_start_byte.unwrap()
                    && node.end_byte() == declaration.entity.name_end_byte.unwrap();
                if !own_name
                    && (identifier && raw == declaration.entity.name
                        || opaque
                            && raw
                                .to_lowercase()
                                .contains(&declaration.entity.name.to_lowercase()))
                {
                    add_possible(
                        declaration,
                        Location {
                            path: path.into(),
                            start_byte: node.start_byte(),
                            end_byte: node.end_byte(),
                        },
                        total,
                    )?;
                }
            }
        }
        let mut cursor = node.walk();
        stack.extend(node.named_children(&mut cursor));
    }
    for (other, text) in texts {
        d.check()?;
        if other == path {
            continue;
        }
        let lowered = text.to_lowercase();
        for declaration in &mut module.declarations {
            d.check()?;
            if lowered.contains(&declaration.entity.name.to_lowercase()) {
                declaration.external_literal = true;
            }
        }
    }
    Ok(())
}

fn reference_token(tree: &Tree, start: usize, end: usize) -> bool {
    let Some(name) = tree.root_node().named_descendant_for_byte_range(start, end) else {
        return false;
    };
    if name.byte_range() != (start..end)
        || !matches!(name.kind(), "identifier" | "shorthand_property_identifier")
    {
        return false;
    }
    let Some(parent) = name.parent() else {
        return false;
    };
    // Admit known expression-use positions. Unknown declaration, pattern and
    // parameter contexts must not become proof that a dependency was used.
    match parent.kind() {
        "call_expression" => parent.child_by_field_name("function") == Some(name),
        "new_expression" => parent.child_by_field_name("constructor") == Some(name),
        "arrow_function" => parent.child_by_field_name("body") == Some(name),
        "variable_declarator" | "pair" => parent.child_by_field_name("value") == Some(name),
        "member_expression" => parent.child_by_field_name("object") == Some(name),
        "subscript_expression" => {
            parent.child_by_field_name("object") == Some(name)
                || parent.child_by_field_name("index") == Some(name)
        }
        "assignment_expression" | "augmented_assignment_expression" => {
            parent.child_by_field_name("left") == Some(name)
                || parent.child_by_field_name("right") == Some(name)
        }
        "object" => name.kind() == "shorthand_property_identifier",
        "arguments"
        | "return_statement"
        | "yield_expression"
        | "await_expression"
        | "expression_statement"
        | "parenthesized_expression"
        | "array"
        | "template_substitution"
        | "computed_property_name"
        | "unary_expression"
        | "binary_expression"
        | "ternary_expression"
        | "sequence_expression"
        | "spread_element"
        | "update_expression"
        | "type_query"
        | "as_expression"
        | "satisfies_expression"
        | "non_null_expression" => true,
        _ => false,
    }
}

fn removable_call(
    tree: &Tree,
    location: &Location,
    path: &str,
    text: &str,
    wrapped: bool,
) -> Option<Range<usize>> {
    if location.path != path {
        return None;
    }
    let name = tree
        .root_node()
        .named_descendant_for_byte_range(location.start_byte, location.end_byte)?;
    if name.kind() != "identifier" || name.byte_range() != (location.start_byte..location.end_byte)
    {
        return None;
    }
    let call = name.parent()?;
    if call.kind() != "call_expression" || call.child_by_field_name("function") != Some(name) {
        return None;
    }
    if !wrapped {
        let statement = call.parent()?;
        return (statement.kind() == "expression_statement").then(|| statement.byte_range());
    }
    // Only a plain identifier call: optional chains, type arguments and
    // unclassified call children do not acquire new authority.
    if call.child_count() != 2
        || call.named_child_count() != 2
        || call.child_by_field_name("arguments") != call.named_child(1)
    {
        return None;
    }
    let mut expression = call;
    let mut wrappers = 0;
    loop {
        let parent = expression.parent()?;
        if parent.kind() == "expression_statement" {
            return matches!(parent.parent()?.kind(), "program" | "statement_block")
                .then(|| parent.byte_range());
        }
        if wrappers == MAX_WRAPPERS || parent.named_child_count() != 1 {
            return None;
        }
        match parent.kind() {
            "parenthesized_expression" | "await_expression"
                if parent.named_child(0) == Some(expression) => {}
            "unary_expression"
                if parent.child_by_field_name("argument") == Some(expression)
                    && parent
                        .child_by_field_name("operator")?
                        .utf8_text(text.as_bytes())
                        .ok()?
                        == "void" => {}
            _ => return None,
        }
        expression = parent;
        wrappers += 1;
    }
}

fn checked_result(
    file: &SourceFile,
    text: &str,
    before: &Tree,
    edits: &[Edit],
    d: &Deadline,
) -> Result<()> {
    let mut spans: Vec<_> = edits
        .iter()
        .map(|edit| match edit {
            Edit::Replace {
                start_byte,
                end_byte,
                ..
            } => Ok(*start_byte..*end_byte),
            _ => anyhow::bail!("Unexpected propagated deletion effect"),
        })
        .collect::<Result<_>>()?;
    spans.sort_unstable_by_key(|span| std::cmp::Reverse(span.start));
    let mut after = text.to_string();
    let mut previous = text.len();
    for span in &spans {
        d.check()?;
        ensure!(
            span.start <= span.end
                && span.end <= previous
                && after.is_char_boundary(span.start)
                && after.is_char_boundary(span.end),
            "Invalid propagated deletion ranges"
        );
        previous = span.start;
        after.replace_range(span.clone(), "");
    }
    let mut parser = Parser::new();
    parser.set_language(&analysis::grammar(&file.language).context(UNSUPPORTED)?)?;
    let mut cancelled = |_: &tree_sitter::ParseState| d.check().is_err();
    let tree = parser
        .parse_with_options(
            &mut |at, _| &after.as_bytes()[at.min(after.len())..],
            None,
            Some(tree_sitter::ParseOptions::new().progress_callback(&mut cancelled)),
        )
        .context("Propagation result parser cancelled")?;
    d.check()?;
    ensure!(
        !tree.root_node().has_error(),
        "Propagated deletion would leave invalid syntax"
    );
    result_ast::unchanged(before, &tree, &spans, d)?;
    Ok(())
}

fn removed(location: &Location, path: &str, regions: &BTreeSet<(usize, usize)>) -> bool {
    regions
        .iter()
        .any(|(start, end)| location.within(path, &(*start..*end)))
}

pub(super) fn prepare(
    engine: &mut Engine,
    snapshot: &Snapshot,
    graph: &Graph,
    selected: &Entity,
    args: &Value,
    d: &Deadline,
) -> Result<Value> {
    d.check()?;
    validate_policy(args)?;
    let policy = args
        .get("propagationPolicy")
        .and_then(Value::as_str)
        .unwrap_or(DIRECT);
    let wrapped = policy != DIRECT;
    let file = snapshot
        .files
        .iter()
        .find(|file| file.path == selected.path)
        .context(UNSUPPORTED)?
        .clone();
    ensure!(
        graph.entities.len() <= 200_000
            && graph.observations.len() <= 200_000
            && graph.evidence.len() <= 200_000,
        CAPACITY
    );
    ensure!(
        matches!(file.language.as_str(), "typescript" | "javascript")
            && (file.path.ends_with(".ts") || file.path.ends_with(".js")),
        UNSUPPORTED
    );
    let texts = inventory(engine, snapshot, selected, d)?;
    let text = texts.get(&file.path).context(UNSUPPORTED)?;
    let mut module = parse(&file, text, graph, selected, policy == BINDINGS, d)?;
    originals(engine, snapshot, d)?;
    let mut provider_context = None;
    let mut total_references = 0usize;
    let mut provider_references = 0usize;
    for declaration in &mut module.declarations {
        d.check()?;
        let definition = provider(
            engine,
            snapshot,
            graph,
            &declaration.entity,
            "definition",
            &json!({"maxItems":32}),
            d,
        )?;
        let definitions = definition["locations"]
            .as_array()
            .context("Propagation definition proof unavailable")?;
        let name_start = declaration.entity.name_start_byte.context(UNSUPPORTED)?;
        let name_end = declaration.entity.name_end_byte.context(UNSUPPORTED)?;
        ensure!(
            definition["truncated"] != true
                && definitions.len() == 1
                && definitions[0]["path"] == file.path
                && definitions[0]["contentHash"] == file.hash
                && definitions[0]["startByte"] == name_start
                && definitions[0]["endByte"] == name_end
                && definitions[0]["text"] == declaration.entity.name,
            "Propagation requires one exact self-bound provider definition"
        );
        let references = provider(
            engine,
            snapshot,
            graph,
            &declaration.entity,
            "references",
            &json!({"includeDeclaration":true,"maxItems":2000}),
            d,
        )?;
        let locations = references["locations"]
            .as_array()
            .context("Propagation references unavailable")?;
        ensure!(
            references["truncated"] != true && !locations.is_empty() && locations.len() <= 2000,
            "Propagation requires bounded references including the declaration"
        );
        ensure!(
            locations.iter().any(|item| item["path"] == file.path
                && item["startByte"] == name_start
                && item["endByte"] == name_end),
            "Propagation references omitted the declaration"
        );
        for location in locations {
            d.check()?;
            total_references += 1;
            provider_references += 1;
            ensure!(total_references <= MAX_REFERENCES, CAPACITY);
            let path = location["path"]
                .as_str()
                .context("Propagation reference path missing")?;
            let member = snapshot
                .files
                .iter()
                .find(|file| file.path == path)
                .context("Propagation reference outside captured scope")?;
            let source = texts
                .get(path)
                .context("Propagation reference source missing")?;
            let start = usize::try_from(
                location["startByte"]
                    .as_u64()
                    .context("Propagation reference start missing")?,
            )?;
            let end = usize::try_from(
                location["endByte"]
                    .as_u64()
                    .context("Propagation reference end missing")?,
            )?;
            ensure!(
                location["contentHash"] == member.hash
                    && location["text"] == declaration.entity.name
                    && crate::paths::utf8_range(source, start, end)? == declaration.entity.name,
                "Propagation reference/source binding mismatch"
            );
            if path == file.path && start == name_start && end == name_end {
                continue;
            }
            ensure!(
                path != file.path || reference_token(&module.tree, start, end),
                "Propagation provider reference requires a classified identifier use, not a comment, string or binding position"
            );
            declaration.reported.push(Location {
                path: path.into(),
                start_byte: start,
                end_byte: end,
            });
        }
        let bound = context(&definition)?;
        ensure!(
            context(&references)? == bound
                && provider_context
                    .as_ref()
                    .is_none_or(|prior| prior == &bound),
            "Propagation provider/source policy changed"
        );
        provider_context = Some(bound);
    }
    graph_candidates(
        &mut module,
        snapshot,
        graph,
        &texts,
        &mut total_references,
        d,
    )?;
    lexical_candidates(
        &mut module,
        &file.path,
        text,
        &texts,
        &mut total_references,
        d,
    )?;
    let target = module
        .declarations
        .iter()
        .position(|item| item.entity.id == selected.id)
        .context(UNSUPPORTED)?;
    let mut usages = BTreeSet::new();
    for location in &module.declarations[target].reported {
        d.check()?;
        if let Some(span) = removable_call(&module.tree, location, &file.path, text, wrapped) {
            usages.insert((span.start, span.end));
            ensure!(usages.len() <= MAX_USAGES, CAPACITY);
        }
    }
    let mut candidates = BTreeSet::from([target]);
    loop {
        d.check()?;
        let before = candidates.len();
        for (index, declaration) in module.declarations.iter().enumerate() {
            if declaration.reported.iter().any(|location| {
                removed(location, &file.path, &usages)
                    || candidates.iter().any(|owner| {
                        location.within(&file.path, &module.declarations[*owner].whole)
                    })
            }) {
                candidates.insert(index);
            }
        }
        if candidates.len() == before {
            break;
        }
    }
    let mut live = BTreeSet::new();
    for index in &candidates {
        d.check()?;
        if *index == target {
            continue;
        }
        let declaration = &module.declarations[*index];
        if declaration.exported
            || declaration.external_literal
            || declaration
                .reported
                .iter()
                .chain(&declaration.possible)
                .any(|location| {
                    !removed(location, &file.path, &usages)
                        && !candidates.iter().any(|owner| {
                            location.within(&file.path, &module.declarations[*owner].whole)
                        })
                })
        {
            live.insert(*index);
        }
    }
    loop {
        d.check()?;
        let before = live.len();
        for index in &candidates {
            if *index == target {
                continue;
            }
            if module.declarations[*index]
                .reported
                .iter()
                .chain(&module.declarations[*index].possible)
                .any(|location| {
                    !removed(location, &file.path, &usages)
                        && live.iter().any(|owner| {
                            location.within(&file.path, &module.declarations[*owner].whole)
                        })
                })
            {
                live.insert(*index);
            }
        }
        if live.len() == before {
            break;
        }
    }
    let deleted: BTreeSet<_> = candidates.difference(&live).copied().collect();
    let target_declaration = &module.declarations[target];
    ensure!(
        !target_declaration.external_literal,
        "Captured external name occurrence prevents propagated deletion"
    );
    for location in target_declaration
        .reported
        .iter()
        .chain(&target_declaration.possible)
    {
        d.check()?;
        ensure!(
            removed(location, &file.path, &usages)
                || deleted
                    .iter()
                    .any(|owner| location.within(&file.path, &module.declarations[*owner].whole)),
            "Propagation has an unsupported or unbound surviving usage"
        );
    }
    let usages: Vec<_> = usages
        .into_iter()
        .filter(|(start, end)| {
            !deleted.iter().any(|owner| {
                *start >= module.declarations[*owner].whole.start
                    && *end <= module.declarations[*owner].whole.end
            })
        })
        .collect();
    let mut edits = Vec::new();
    let mut deleted_metadata = Vec::new();
    for index in &deleted {
        d.check()?;
        let declaration = &module.declarations[*index];
        edits.push(Edit::Replace {
            path: file.path.clone(),
            expected_hash: file.hash.clone(),
            start_byte: declaration.whole.start,
            end_byte: declaration.whole.end,
            text: String::new(),
        });
        deleted_metadata.push(json!({"entityId":declaration.entity.id,"name":declaration.entity.name,"path":file.path,"contentHash":file.hash,"startByte":declaration.whole.start,"endByte":declaration.whole.end,"entityStartByte":declaration.entity.start_byte,"entityEndByte":declaration.entity.end_byte}));
    }
    for (start, end) in &usages {
        edits.push(Edit::Replace {
            path: file.path.clone(),
            expected_hash: file.hash.clone(),
            start_byte: *start,
            end_byte: *end,
            text: String::new(),
        });
    }
    let retained: Vec<_> = live.iter().map(|index| {let declaration=&module.declarations[*index];json!({"entityId":declaration.entity.id,"name":declaration.entity.name,"exported":declaration.exported,"capturedExternalNameCandidate":declaration.external_literal,"reason":"surviving_reported_or_possible_reference_or_export"})}).collect();
    let proof = json!({"policy":policy,"performed":true,"deletedDeclarations":deleted_metadata,"removedUsageCount":usages.len(),"removedUsages":usages.iter().map(|(start,end)|json!({"path":file.path,"contentHash":file.hash,"startByte":start,"endByte":end,"kind":"expression_statement"})).collect::<Vec<_>>(),"retainedCandidates":retained,"providerRequestCount":module.declarations.len()*2,"providerReferenceCount":provider_references,"reportedAndPossibleEvidenceCount":total_references,"capturedFileCount":snapshot.files.len(),"capturedManifestHash":identity(&snapshot.files),"exhaustiveCompilerProof":false,"providerContext":provider_context});
    ensure!(serde_json::to_vec(&proof)?.len() <= 128_000, CAPACITY);
    if wrapped {
        checked_result(&file, text, &module.tree, &edits, d)?;
    }
    originals(engine, snapshot, d)?;
    let plan = changes::prepare(
        &mut engine.store,
        snapshot,
        graph,
        &json!({"edits":edits}),
        d,
    )?;
    Ok(
        json!({"snapshotId":snapshot.id,"plan":plan,"provider":"host_selected_lsp","equivalent":"independent_captured_function_propagation","forced":args["deleteEvenIfUsed"]==true,"propagation":proof,"complete":false,"remainingUsagesMayBreak":true,"commandsExecuted":false}),
    )
}
