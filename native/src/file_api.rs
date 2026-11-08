//! Immutable, exact-file API projection with source-bound declaration headers.
use crate::{
    capture::Deadline,
    model::{Graph, Snapshot},
    store::Store,
};
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use tree_sitter::{ParseOptions, Parser};
pub fn inspect(
    store: &Store,
    snapshot: &Snapshot,
    graph: &Graph,
    args: &Value,
    deadline: &Deadline,
) -> Result<Value> {
    inspect_selected(store, snapshot, graph, args, deadline, 200)
}
pub(crate) fn inspect_all(
    store: &Store,
    snapshot: &Snapshot,
    graph: &Graph,
    file: &str,
    deadline: &Deadline,
) -> Result<Value> {
    inspect_selected(
        store,
        snapshot,
        graph,
        &json!({"file":file,"limit":100000}),
        deadline,
        100000,
    )
}
fn inspect_selected(
    store: &Store,
    snapshot: &Snapshot,
    graph: &Graph,
    args: &Value,
    deadline: &Deadline,
    maximum: usize,
) -> Result<Value> {
    crate::query_features::source_scope(store, snapshot, graph, deadline)?;
    let object = args
        .as_object()
        .context("File API arguments must be object")?;
    ensure!(
        object
            .keys()
            .all(|key| ["file", "limit", "offset"].contains(&key.as_str())),
        "Unknown file API argument"
    );
    let requested = args["file"].as_str().context("File required")?;
    crate::paths::normalized_syntax(requested)?;
    let limit = match args.get("limit") {
        Some(value) => value
            .as_u64()
            .filter(|n| (1..=maximum as u64).contains(n))
            .context("Invalid file API limit")? as usize,
        None => 200,
    };
    let offset = match args.get("offset") {
        Some(value) => value
            .as_u64()
            .filter(|n| *n <= 100000)
            .context("Invalid file API offset")? as usize,
        None => 0,
    };
    let file = if let Some(file) = snapshot.files.iter().find(|file| file.path == requested) {
        file
    } else {
        ensure!(!requested.contains('/'), "Exact captured file unavailable");
        let matches: Vec<_> = snapshot
            .files
            .iter()
            .filter(|file| file.path.rsplit('/').next() == Some(requested))
            .collect();
        ensure!(
            matches.len() == 1,
            "Basename must resolve to one captured file"
        );
        matches[0]
    };
    let scope = crate::capture::source_scope_policy(store, snapshot)?;
    let view = scope.source_view(&file.path)?;
    let source = view.text();
    let mut declarations: Vec<_> = graph
        .entities
        .iter()
        .filter(|entity| entity.path == file.path && entity.kind != "file")
        .collect();
    declarations.sort_by_key(|entity| (entity.start_byte, entity.end_byte, &entity.id));
    let selected: Vec<_> = declarations
        .iter()
        .skip(offset)
        .take(limit)
        .copied()
        .collect();
    let targets: BTreeSet<_> = selected
        .iter()
        .map(|entity| (entity.start_byte, entity.end_byte))
        .collect();
    let mut headers = BTreeMap::new();
    let segments = crate::languages::embedded_units(&file.language, source)?
        .map(|units| {
            units
                .into_iter()
                .filter(|unit| unit.unsupported.is_none())
                .map(|unit| (unit.start, unit.end, unit.language.to_owned()))
                .collect::<Vec<_>>()
        })
        .unwrap_or_else(|| vec![(0, source.len(), file.language.clone())]);
    for (shift, finish, language) in segments {
        let Some(language) = crate::analysis::grammar(&language) else {
            continue;
        };
        let parse_text = crate::paths::utf8_range(source, shift, finish)?;
        let mut parser = Parser::new();
        parser.set_language(&language)?;
        let mut cancelled = |_: &tree_sitter::ParseState| deadline.check().is_err();
        let tree = parser
            .parse_with_options(
                &mut |position, _| &parse_text.as_bytes()[position.min(parse_text.len())..],
                None,
                Some(ParseOptions::new().progress_callback(&mut cancelled)),
            )
            .context("API parser cancelled or unavailable")?;
        let mut stack = vec![tree.root_node()];
        let mut visited = 0usize;
        while let Some(node) = stack.pop() {
            deadline.check()?;
            visited += 1;
            ensure!(visited <= 1_000_000, "API AST budget exceeded");
            let span = (
                view.to_raw(shift + node.start_byte())?,
                view.to_raw(shift + node.end_byte())?,
            );
            if targets.contains(&span) {
                let body = node
                    .child_by_field_name("body")
                    .or_else(|| {
                        node.child_by_field_name("value")
                            .and_then(|value| value.child_by_field_name("body"))
                    })
                    .or_else(|| {
                        node.child_by_field_name("rhs")
                            .and_then(|value| value.child_by_field_name("body"))
                    });
                let end = body.map(|body| body.start_byte()).or_else(|| {
                    matches!(
                        node.kind(),
                        "method_signature"
                            | "abstract_method_signature"
                            | "function_signature_item"
                            | "type_alias_declaration"
                            | "type_item"
                            | "const_item"
                            | "constant_declaration"
                    )
                    .then_some(node.end_byte())
                });
                if let Some(end) = end {
                    headers.insert(span, view.to_raw(shift + end)?);
                }
            }
            let mut cursor = node.walk();
            stack.extend(node.named_children(&mut cursor));
        }
    }
    let mut items = Vec::new();
    let mut omitted = 0usize;
    let mut output_bytes = 0usize;
    let mut byte_capped = false;
    for entity in selected {
        deadline.check()?;
        let mut row = json!({"entityId":entity.id,"name":entity.name,"qualifiedName":entity.qualified_name,"kind":entity.kind,"path":file.path,"contentHash":file.hash,"startByte":entity.start_byte,"endByte":entity.end_byte,"startLine":entity.start_line,"endLine":entity.end_line,"signatureAvailable":false,"trust":"source-data"});
        if let Some(end) = headers.get(&(entity.start_byte, entity.end_byte)) {
            let header = view.quote(entity.start_byte, *end)?.trim_end();
            let mut length = header.len().min(16000);
            while !header.is_char_boundary(length) {
                length -= 1;
            }
            row["signature"] = json!(&header[..length]);
            row["signatureStartByte"] = json!(entity.start_byte);
            row["signatureEndByte"] =
                json!(view.to_raw(view.to_decoded(entity.start_byte)? + length)?);
            row["signatureAvailable"] = json!(true);
            row["signatureTruncated"] = json!(length < header.len());
        } else {
            omitted += 1;
            row["signatureOmission"] =
                json!("No verified body-free AST header for this captured declaration");
        }
        view.annotate(&mut row, &file.hash);
        let size = serde_json::to_vec(&row)?.len() + 1;
        if output_bytes.saturating_add(size) > 32 * 1024 * 1024 {
            byte_capped = true;
            break;
        }
        output_bytes += size;
        items.push(row);
    }
    let next = offset + items.len();
    Ok(
        json!({"snapshotId":snapshot.id,"path":file.path,"contentHash":file.hash,"items":items,"total":declarations.len(),"offset":offset,"nextOffset":if next<declarations.len(){Some(next)}else{None},"truncated":next<declarations.len(),"outputBytes":output_bytes,"byteBudgetExceeded":byte_capped,"signatureOmissions":omitted,"selectedHeaderCoverageComplete":omitted==0&&!byte_capped,"complete":false,"limitations":["Declaration coverage follows the captured structural parser; omitted headers are not reconstructed from body text."]}),
    )
}
