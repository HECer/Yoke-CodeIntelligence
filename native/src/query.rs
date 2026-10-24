use crate::{
    capture::Deadline,
    model::{Entity, Graph, Snapshot},
    paths::utf8_range,
    store::Store,
};
use anyhow::{Result, bail, ensure};
use rusqlite::params;
use serde_json::{Value, json};
use std::collections::{HashMap, HashSet, VecDeque};
use unicode_normalization::{UnicodeNormalization, char::is_combining_mark};

pub fn source(store: &Store, snapshot: &Snapshot, path: &str) -> Result<(String, String)> {
    let scope = crate::capture::source_scope_policy(store, snapshot)?;
    source_in_scope(store, &scope, path)
}
pub(crate) fn source_in_scope(
    _store: &Store,
    scope: &crate::capture::CapturedSourceScope<'_>,
    path: &str,
) -> Result<(String, String)> {
    let file = scope.file(path)?;
    Ok((
        String::from_utf8(scope.read_source(path)?)?,
        file.hash.clone(),
    ))
}
pub fn read(store: &Store, snapshot: &Snapshot, graph: &Graph, args: &Value) -> Result<Value> {
    let entity = args
        .get("entityId")
        .and_then(Value::as_str)
        .map(|id| {
            graph
                .entities
                .iter()
                .find(|e| e.id == id)
                .ok_or_else(|| anyhow::anyhow!("Unknown entity"))
        })
        .transpose()?;
    let path = entity
        .map(|e| e.path.as_str())
        .or_else(|| args["path"].as_str())
        .ok_or_else(|| anyhow::anyhow!("Source path or entity required"))?;
    let (text, hash) = source(store, snapshot, path)?;
    let mut start = args["startByte"]
        .as_u64()
        .map(|n| n as usize)
        .unwrap_or_else(|| entity.map_or(0, |e| e.start_byte));
    let mut end = args["endByte"]
        .as_u64()
        .map(|n| n as usize)
        .unwrap_or_else(|| entity.map_or(text.len(), |e| e.end_byte));
    if args.get("startLine").is_some() || args.get("endLine").is_some() {
        ensure!(
            args.get("startByte").is_none() && args.get("endByte").is_none(),
            "Do not mix byte and line ranges"
        );
        let starts = crate::paths::line_starts(&text);
        let first = args["startLine"].as_u64().unwrap_or(1) as usize;
        let last = args["endLine"].as_u64().unwrap_or(starts.len() as u64) as usize;
        ensure!(
            first >= 1 && last >= first && last <= starts.len(),
            "Invalid line range"
        );
        start = starts[first - 1];
        end = starts.get(last).copied().unwrap_or(text.len());
    }
    let value = utf8_range(&text, start, end)?;
    Ok(
        json!({"path":path,"contentHash":hash,"startByte":start,"endByte":end,"text":value,"trust":"source-data"}),
    )
}
fn terms(query: &str) -> String {
    query
        .split(|c: char| !c.is_alphanumeric() && c != '_')
        .filter(|s| !s.is_empty())
        .take(32)
        .map(|s| format!("\"{}\"*", s.replace('"', "\"\"")))
        .collect::<Vec<_>>()
        .join(" OR ")
}
pub fn identifier_terms(value: &str) -> String {
    let chars: Vec<_> = value.chars().collect();
    let mut split = String::new();
    for (i, &c) in chars.iter().enumerate() {
        if !c.is_alphanumeric() {
            split.push(' ');
            continue;
        }
        if i > 0
            && c.is_uppercase()
            && (chars[i - 1].is_lowercase()
                || chars[i - 1].is_numeric()
                || (chars[i - 1].is_uppercase()
                    && chars.get(i + 1).is_some_and(|c| c.is_lowercase())))
        {
            split.push(' ');
        }
        split.push(c);
    }
    format!("{value} {split}")
}

fn documents(
    store: &Store,
    snapshot: &Snapshot,
    query: &str,
    path: &str,
    limit: usize,
    offset: usize,
    deadline: &Deadline,
) -> Result<Value> {
    let match_query = terms(query);
    ensure!(!match_query.is_empty(), "Document search needs words");
    // Rank with FTS, then locate a normalized token candidate in original bytes.
    // FTS auxiliary highlight/snippet can do quadratic work for repeated hits.
    // The candidate matcher is bounded and never claims an exact FTS5 position.
    let mut stmt = store.db.prepare("SELECT path FROM documents WHERE documents MATCH ?1 AND snapshot=?2 AND instr(path,?3)>0 ORDER BY bm25(documents),path LIMIT ?4 OFFSET ?5")?;
    let paths = stmt
        .query_map(
            params![
                match_query,
                snapshot.id,
                path,
                (limit + 1) as i64,
                offset as i64
            ],
            |row| row.get::<_, String>(0),
        )?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    let mut items = Vec::new();
    for path in paths.iter().take(limit) {
        deadline.check()?;
        let (text, hash) = source(store, snapshot, path)?;
        let hit = document_candidate(&text, query, deadline)?;
        let (hit_start, hit_end) = hit.unwrap_or((0, 0));
        let line_start = text[..hit_start].rfind(['\n', '\r']).map_or(0, |p| p + 1);
        let mut start = line_start.max(hit_start.saturating_sub(1200));
        while !text.is_char_boundary(start) {
            start += 1;
        }
        let line_end = text[hit_end..]
            .find('\n')
            .map_or(text.len(), |p| hit_end + p);
        let mut end = line_end.min(start + 4000);
        while !text.is_char_boundary(end) {
            end -= 1;
        }
        items.push(json!({"path":path,"contentHash":hash,"startByte":start,"endByte":end,"matchStartByte":hit.map(|value|value.0),"matchEndByte":hit.map(|value|value.1),"matchMethod":if hit.is_some(){"unicode-nfd-lowercase-token-prefix-candidate"}else{"fts5-document-level-match-span-unavailable"},"text":&text[start..end],"kind":"document","trust":"source-data","snippetTruncated":end<line_end||start>line_start}));
    }
    Ok(
        json!({"items":items,"offset":offset,"truncated":paths.len()>limit,"retrieval":"fts5-bm25-source-documents"}),
    )
}
fn document_candidate(
    text: &str,
    query: &str,
    deadline: &Deadline,
) -> Result<Option<(usize, usize)>> {
    let normalize = |word: &str| {
        word.nfd()
            .filter(|c| !is_combining_mark(*c))
            .flat_map(char::to_lowercase)
            .collect::<String>()
    };
    let terms: Vec<_> = query
        .split(|c: char| !c.is_alphanumeric() && !is_combining_mark(c))
        .filter(|word| !word.is_empty())
        .take(32)
        .map(normalize)
        .collect();
    let max = terms
        .iter()
        .map(|term| term.chars().count())
        .max()
        .unwrap_or(0);
    let matches = |word: &str| {
        if word.is_ascii() {
            terms.iter().any(|term| {
                word.get(..term.len())
                    .is_some_and(|prefix| prefix.eq_ignore_ascii_case(term))
            })
        } else {
            let normalized = word
                .nfd()
                .filter(|c| !is_combining_mark(*c))
                .flat_map(char::to_lowercase)
                .take(max)
                .collect::<String>();
            terms.iter().any(|term| normalized.starts_with(term))
        }
    };
    let mut start = None;
    for (index, (byte, c)) in text
        .char_indices()
        .chain(std::iter::once((text.len(), ' ')))
        .enumerate()
    {
        if index.is_multiple_of(2048) {
            deadline.check()?;
        }
        if c.is_alphanumeric() || is_combining_mark(c) {
            start.get_or_insert(byte);
        } else if let Some(first) = start.take()
            && matches(&text[first..byte])
        {
            return Ok(Some((first, byte)));
        }
    }
    Ok(None)
}
pub fn find(
    store: &Store,
    snapshot: &Snapshot,
    graph: &Graph,
    args: &Value,
    deadline: &Deadline,
) -> Result<Value> {
    let kind = args["kind"].as_str().unwrap_or("symbol");
    if kind == "text" {
        return crate::query_features::text_find(store, snapshot, graph, args, deadline);
    }
    if matches!(kind, "symbol" | "all" | "file") {
        return crate::query_features::symbols_find(store, snapshot, graph, args, deadline);
    }
    find_base(store, snapshot, graph, args, deadline)
}
pub(crate) fn find_base(
    store: &Store,
    snapshot: &Snapshot,
    graph: &Graph,
    args: &Value,
    deadline: &Deadline,
) -> Result<Value> {
    store.set_deadline(deadline)?;
    let query = args["query"].as_str().unwrap_or("");
    let kind = args["kind"].as_str().unwrap_or("symbol");
    let limit = args["limit"].as_u64().unwrap_or(30) as usize;
    let offset = args["offset"].as_u64().unwrap_or(0) as usize;
    let path = args["path"].as_str().unwrap_or("");
    let mut items = Vec::new();
    if kind == "document" {
        return documents(store, snapshot, query, path, limit, offset, deadline);
    }
    if kind == "text" {
        ensure!(!query.is_empty(), "Literal search needs a nonempty query");
        let regex = if args["regex"].as_bool().unwrap_or(false) {
            Some(
                regex::RegexBuilder::new(query)
                    .size_limit(1_000_000)
                    .dfa_size_limit(1_000_000)
                    .build()?,
            )
        } else {
            None
        };
        let mut skipped = 0;
        let mut truncated = false;
        for file in &snapshot.files {
            deadline.check()?;
            if !file.path.contains(path) {
                continue;
            }
            let text = String::from_utf8(store.read_blob(&file.hash)?)?;
            let matches: Box<dyn Iterator<Item = (usize, usize)> + '_> = if let Some(regex) = &regex
            {
                Box::new(regex.find_iter(&text).map(|m| (m.start(), m.end())))
            } else {
                Box::new(
                    text.match_indices(query)
                        .map(|(start, m)| (start, start + m.len())),
                )
            };
            for (start, end) in matches {
                deadline.check()?;
                if skipped < offset {
                    skipped += 1;
                    continue;
                }
                if items.len() == limit {
                    truncated = true;
                    break;
                }
                let line = text.as_bytes()[..start]
                    .iter()
                    .filter(|b| **b == b'\n')
                    .count()
                    + 1;
                let context_start = text[..start].rfind('\n').map_or(0, |p| p + 1);
                let context_end = text[end..].find('\n').map_or(text.len(), |p| end + p);
                items.push(json!({"path":file.path,"contentHash":file.hash,"startByte":start,"endByte":end,"startLine":line,"text":text[context_start..context_end].chars().take(500).collect::<String>(),"trust":"source-data"}));
            }
            if truncated {
                break;
            }
        }
        return Ok(json!({"items":items,"truncated":truncated,"offset":offset}));
    }
    let mut ids = Vec::new();
    let mut candidate_cap_reached = false;
    let match_query = terms(query);
    if !match_query.is_empty() {
        let mut stmt=store.db.prepare("SELECT id FROM symbols WHERE symbols MATCH ?1 AND snapshot=?2 AND instr(path,?3)>0 ORDER BY bm25(symbols),id LIMIT 100001")?;
        ids = stmt
            .query_map(params![match_query, snapshot.id, path], |r| {
                r.get::<_, String>(0)
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        candidate_cap_reached = ids.len() > 100000;
        ids.truncate(100000);
    }
    let by_id: HashMap<_, _> = graph.entities.iter().map(|e| (&e.id, e)).collect();
    let candidates: Vec<&Entity> = if query.is_empty() {
        graph.entities.iter().collect()
    } else {
        ids.iter().filter_map(|id| by_id.get(id).copied()).collect()
    };
    let filtered: Vec<_> = candidates
        .into_iter()
        .filter(|e| {
            e.path.contains(path)
                && match kind {
                    "file" => e.kind == "file",
                    "all" => true,
                    "symbol" => !matches!(e.kind.as_str(), "file" | "concept" | "rationale"),
                    "concept" => e.kind == "concept",
                    "rationale" => e.kind == "rationale",
                    "api_contract" => matches!(
                        e.kind.as_str(),
                        "api_contract"
                            | "table"
                            | "migration_table"
                            | "orm_model"
                            | "schema_type"
                            | "schema_field"
                    ),
                    _ => false,
                }
        })
        .collect();
    for e in filtered.iter().skip(offset).take(limit) {
        items.push(serde_json::to_value(e)?);
    }
    Ok(
        json!({"items":items,"total":if candidate_cap_reached {None}else{Some(filtered.len())},"totalLowerBound":filtered.len(),"candidateCapReached":candidate_cap_reached,"offset":offset,"truncated":candidate_cap_reached||filtered.len()>offset+limit}),
    )
}
pub fn trace(graph: &Graph, args: &Value, deadline: &Deadline) -> Result<Value> {
    let ids = args["entityIds"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("entityIds required"))?;
    let mut visited: HashSet<String> = ids
        .iter()
        .map(|id| id.as_str().unwrap_or("").to_string())
        .collect();
    ensure!(
        visited
            .iter()
            .all(|id| graph.entities.iter().any(|e| &e.id == id)),
        "Unknown trace entity"
    );
    let mut frontier: Vec<_> = visited.iter().cloned().collect();
    frontier.sort();
    let direction = args["direction"].as_str().unwrap_or("out");
    let depth = args["depth"].as_u64().unwrap_or(2);
    let max = args["maxVisited"].as_u64().unwrap_or(300) as usize;
    let relations = args["relations"].as_array();
    let resolutions = args["resolutions"].as_array();
    let mut selected = HashSet::new();
    let mut truncated = false;
    let mut adjacency: HashMap<&str, Vec<usize>> = HashMap::new();
    for (index, edge) in graph.observations.iter().enumerate() {
        if relations.is_some_and(|r| !r.iter().any(|v| v == &edge.relation))
            || resolutions.is_some_and(|r| !r.iter().any(|v| v == &edge.resolution))
        {
            continue;
        }
        if direction != "in" {
            adjacency.entry(&edge.source).or_default().push(index);
        }
        if direction != "out"
            && let Some(target) = &edge.target
        {
            adjacency.entry(target).or_default().push(index);
        }
    }
    for _ in 0..depth {
        let mut next = Vec::new();
        for id in frontier {
            deadline.check()?;
            for index in adjacency.get(id.as_str()).into_iter().flatten() {
                let edge = &graph.observations[*index];
                let other = if edge.source == id {
                    edge.target.as_ref()
                } else {
                    Some(&edge.source)
                };
                if let Some(other) = other
                    && !visited.contains(other)
                {
                    if visited.len() >= max {
                        truncated = true;
                        continue;
                    }
                    visited.insert(other.clone());
                    next.push(other.clone());
                }
                selected.insert(*index);
            }
        }
        if next.is_empty() {
            break;
        }
        frontier = next;
    }
    let observations: Vec<_> = graph
        .observations
        .iter()
        .enumerate()
        .filter(|(i, _)| selected.contains(i))
        .map(|(_, e)| e)
        .collect();
    let evidence_ids: HashSet<_> = observations.iter().flat_map(|e| &e.evidence_ids).collect();
    let evidence: Vec<_> = graph
        .evidence
        .iter()
        .filter(|e| evidence_ids.contains(&e.id))
        .collect();
    let entities: Vec<_> = graph
        .entities
        .iter()
        .filter(|e| visited.contains(&e.id))
        .collect();
    Ok(
        json!({"entities":entities,"observations":observations,"evidence":evidence,"truncated":truncated,"complete":false}),
    )
}
pub fn context(
    store: &Store,
    snapshot: &Snapshot,
    graph: &Graph,
    args: &Value,
    deadline: &Deadline,
) -> Result<Value> {
    if let Some(plan) = args.get("plan") {
        return context_plan(store, snapshot, graph, plan, deadline);
    }
    let query = args["query"].as_str().unwrap_or("");
    ensure!(!query.is_empty(), "Context needs a query");
    let found = find(
        store,
        snapshot,
        graph,
        &json!({"query":query,"kind":"all","limit":args["limit"].as_u64().unwrap_or(5)}),
        deadline,
    )?;
    let limit = args["limit"].as_u64().unwrap_or(5) as usize;
    let mut items = Vec::new();
    let mut delivered = HashSet::new();
    for entity in found["items"].as_array().unwrap() {
        deadline.check()?;
        let mut read = read(store, snapshot, graph, &json!({"entityId":entity["id"]}))?;
        if !delivered.insert((
            read["path"].clone().to_string(),
            read["startByte"].as_u64().unwrap(),
            read["endByte"].as_u64().unwrap(),
        )) {
            continue;
        }
        read["entity"] = entity.clone();
        if read["text"].as_str().unwrap().len() > 6000 {
            read.as_object_mut().unwrap().remove("text");
            read["bodyOmitted"] = json!(true);
            read["readWith"] =
                json!({"tool":"ci_read","snapshotId":snapshot.id,"entityId":entity["id"]});
        }
        items.push(read);
    }
    if items.len() < limit {
        let docs = documents(store, snapshot, query, "", limit, 0, deadline)?;
        for item in docs["items"].as_array().unwrap() {
            if items.len() == limit {
                break;
            }
            if delivered.insert((
                item["path"].to_string(),
                item["startByte"].as_u64().unwrap(),
                item["endByte"].as_u64().unwrap(),
            )) {
                items.push(item.clone());
            }
        }
    }
    let paths: HashSet<_> = items
        .iter()
        .filter_map(|item| item["path"].as_str())
        .collect();
    let tests: Vec<_> = snapshot
        .files
        .iter()
        .filter(|file| {
            let lower = file.path.to_ascii_lowercase();
            (lower.contains("test") || lower.contains("spec"))
                && paths.iter().any(|path| {
                    std::path::Path::new(path)
                        .file_stem()
                        .and_then(|s| s.to_str())
                        .is_some_and(|stem| file.path.contains(stem))
                })
        })
        .take(20)
        .map(|file| &file.path)
        .collect();
    Ok(
        json!({"items":items,"suggestedTests":tests,"testSelection":if tests.is_empty(){"unknown_use_broader_tests"}else{"filename_convention_candidates"},"coverage":graph.coverage,"complete":false,"limitations":["Source-first lexical context; no model-generated answer. Test hints are filename candidates, not observed execution coverage."]}),
    )
}

pub fn validate_context_plan(plan: &Value) -> Result<()> {
    let steps = plan
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("Context plan must be an array"))?;
    ensure!(
        !steps.is_empty() && steps.len() <= 16,
        "Context plan needs 1..16 steps"
    );
    for step in steps {
        let object = step
            .as_object()
            .ok_or_else(|| anyhow::anyhow!("Context step must be an object"))?;
        let op = step["op"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Context operation required"))?;
        let allowed: &[&str] = match op {
            "find" => &["op", "query", "kind", "limit"],
            "filter" => &["op", "field", "includes"],
            "references" | "read" => &["op"],
            "select" => &["op", "fields"],
            "group" => &["op", "field"],
            "limit" => &["op", "count"],
            _ => bail!("Unsupported read-only context operation"),
        };
        ensure!(
            object.keys().all(|key| allowed.contains(&key.as_str())),
            "Unknown context operation field"
        );
        for field in ["query", "kind", "field", "includes"] {
            if let Some(value) = step.get(field) {
                ensure!(
                    value.as_str().is_some_and(|s| s.len() <= 16000),
                    "Invalid context string"
                );
            }
        }
        if let Some(kind) = step["kind"].as_str() {
            require_kind(kind)?;
        }
        if let Some(limit) = step.get("limit") {
            ensure!(
                limit.as_u64().is_some_and(|n| (1..=100).contains(&n)),
                "Context limit must be 1..100"
            );
        }
        if op == "limit" {
            ensure!(
                step["count"].as_u64().is_some_and(|n| n <= 100),
                "Context count must be 0..100"
            );
        }
        if matches!(op, "filter" | "group") {
            ensure!(
                ["path", "kind", "language"].contains(&step["field"].as_str().unwrap_or(""))
                    || (op == "filter" && step["field"] == "name"),
                "Unsupported context field"
            );
        }
        if op == "select" {
            let fields = step["fields"]
                .as_array()
                .ok_or_else(|| anyhow::anyhow!("Select fields required"))?;
            ensure!(
                fields.len() <= 20
                    && fields
                        .iter()
                        .all(|f| f.as_str().is_some_and(|s| s.len() <= 100
                            && !matches!(s, "__proto__" | "constructor" | "prototype"))),
                "Unsafe or oversized select fields"
            );
        }
    }
    Ok(())
}

fn context_plan(
    store: &Store,
    snapshot: &Snapshot,
    graph: &Graph,
    plan: &Value,
    deadline: &Deadline,
) -> Result<Value> {
    validate_context_plan(plan)?;
    let mut current = Vec::<Value>::new();
    let mut observations = Vec::new();
    let mut evidence = Vec::new();
    let mut truncated = false;
    for step in plan.as_array().unwrap() {
        deadline.check()?;
        match step["op"].as_str().unwrap() {
            "find" => {
                let result = find(
                    store,
                    snapshot,
                    graph,
                    &json!({"query":step["query"].as_str().unwrap_or(""),"kind":step["kind"].as_str().unwrap_or("symbol"),"limit":step["limit"].as_u64().unwrap_or(20)}),
                    deadline,
                )?;
                truncated |= result["truncated"] == true;
                current = result["items"].as_array().unwrap().clone();
            }
            "limit" => {
                let count = step["count"].as_u64().unwrap() as usize;
                truncated |= current.len() > count;
                current.truncate(count);
            }
            "filter" => current.retain(|item| {
                item[step["field"].as_str().unwrap()]
                    .as_str()
                    .unwrap_or("")
                    .contains(step["includes"].as_str().unwrap_or(""))
            }),
            "read" => {
                truncated |= current.len() > 20;
                current = current
                    .iter()
                    .take(20)
                    .map(|item| {
                        deadline.check()?;
                        let args = if item["id"].is_string() {
                            json!({"entityId":item["id"]})
                        } else {
                            json!({"path":item["path"]})
                        };
                        read(store, snapshot, graph, &args)
                    })
                    .collect::<Result<_>>()?;
            }
            "references" => {
                truncated |= current.len() > 20;
                let ids: Vec<_> = current
                    .iter()
                    .take(20)
                    .map(|item| {
                        item["id"]
                            .as_str()
                            .ok_or_else(|| anyhow::anyhow!("References require entities from find"))
                    })
                    .collect::<Result<_>>()?;
                if ids.is_empty() {
                    current.clear();
                    continue;
                }
                let traced = trace(
                    graph,
                    &json!({"entityIds":ids,"direction":"in","relations":["references","calls","constructs"],"depth":1,"maxVisited":200}),
                    deadline,
                )?;
                truncated |= traced["truncated"] == true;
                current = traced["entities"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .filter(|item| !ids.contains(&item["id"].as_str().unwrap()))
                    .cloned()
                    .collect();
                observations.extend(traced["observations"].as_array().unwrap().iter().cloned());
                evidence.extend(traced["evidence"].as_array().unwrap().iter().cloned());
            }
            "select" => {
                current = current
                    .into_iter()
                    .map(|item| {
                        Value::Object(
                            step["fields"]
                                .as_array()
                                .unwrap()
                                .iter()
                                .filter_map(|field| {
                                    let name = field.as_str().unwrap();
                                    item.get(name).map(|v| (name.into(), v.clone()))
                                })
                                .collect(),
                        )
                    })
                    .collect()
            }
            "group" => {
                let mut groups = std::collections::BTreeMap::<String, Vec<Value>>::new();
                for item in current {
                    let key = item[step["field"].as_str().unwrap()]
                        .as_str()
                        .unwrap_or("unknown")
                        .to_string();
                    groups.entry(key).or_default().push(item);
                }
                current = groups
                    .into_iter()
                    .map(|(key, items)| json!({"key":key,"items":items}))
                    .collect();
            }
            _ => unreachable!("validated operation"),
        }
        ensure!(
            current.len() <= 1000 && observations.len() <= 32000 && evidence.len() <= 32000,
            "Context plan capacity exceeded"
        );
    }
    deadline.check()?;
    Ok(
        json!({"items":current,"observations":observations,"evidence":evidence,"steps":plan.as_array().unwrap().len(),"truncated":truncated,"intermediatesReturned":false,"complete":false,"trust":"source-data"}),
    )
}
pub fn shortest_path(
    graph: &Graph,
    from: &str,
    to: &str,
    direction: &str,
    max: usize,
    deadline: &Deadline,
) -> Result<Value> {
    ensure!(
        graph.entities.iter().any(|e| e.id == from) && graph.entities.iter().any(|e| e.id == to),
        "Unknown path endpoint"
    );
    let mut adjacent: HashMap<&str, Vec<(&str, usize)>> = HashMap::new();
    for (i, edge) in graph.observations.iter().enumerate() {
        if let Some(target) = &edge.target {
            adjacent.entry(&edge.source).or_default().push((target, i));
            if direction == "both" {
                adjacent.entry(target).or_default().push((&edge.source, i));
            }
        }
    }
    let mut queue = VecDeque::from([from]);
    let mut previous = HashMap::new();
    previous.insert(from, None);
    while let Some(id) = queue.pop_front() {
        deadline.check()?;
        if id == to {
            break;
        }
        for (next, edge) in adjacent.get(id).into_iter().flatten() {
            if !previous.contains_key(next) {
                ensure!(previous.len() < max, "Path traversal budget exceeded");
                previous.insert(*next, Some((id, *edge)));
                queue.push_back(*next);
            }
        }
    }
    if !previous.contains_key(to) {
        return Ok(json!({"found":false,"complete":false,"nodes":[],"observations":[]}));
    }
    let mut nodes = vec![to];
    let mut edges = Vec::new();
    let mut current = to;
    while let Some(Some((prev, index))) = previous.get(current) {
        edges.push(&graph.observations[*index]);
        nodes.push(*prev);
        current = prev;
    }
    nodes.reverse();
    edges.reverse();
    Ok(json!({"found":true,"nodes":nodes,"observations":edges,"direction":direction}))
}

pub fn analytics(graph: &Graph, deadline: &Deadline) -> Result<Value> {
    let n = graph.entities.len();
    if n == 0 {
        return Ok(json!({"pageRank":[],"communities":[],"components":[],"cycles":[]}));
    }
    ensure!(
        n <= 100_000 && graph.observations.len() <= 1_000_000,
        "Analytics graph budget exceeded"
    );
    let ids: HashMap<_, _> = graph
        .entities
        .iter()
        .enumerate()
        .map(|(i, e)| (e.id.as_str(), i))
        .collect();
    let mut out = vec![Vec::new(); n];
    let mut incoming = vec![Vec::new(); n];
    let mut weak = vec![Vec::new(); n];
    for edge in &graph.observations {
        if let Some(target) = &edge.target {
            let a = ids[edge.source.as_str()];
            let b = ids[target.as_str()];
            out[a].push(b);
            incoming[b].push(a);
            weak[a].push(b);
            weak[b].push(a);
        }
    }
    let mut rank = vec![1.0 / n as f64; n];
    for _ in 0..60 {
        deadline.check()?;
        let dangling: f64 = out
            .iter()
            .zip(&rank)
            .filter(|(edges, _)| edges.is_empty())
            .map(|(_, r)| r)
            .sum();
        let mut next = vec![(0.15 + 0.85 * dangling) / n as f64; n];
        for a in 0..n {
            if !out[a].is_empty() {
                for &b in &out[a] {
                    next[b] += 0.85 * rank[a] / out[a].len() as f64;
                }
            }
        }
        let delta: f64 = next.iter().zip(&rank).map(|(a, b)| (a - b).abs()).sum();
        rank = next;
        if delta < 1e-10 {
            break;
        }
    }
    let mut order: Vec<_> = (0..n).collect();
    order.sort_by(|a, b| rank[*b].total_cmp(&rank[*a]).then(a.cmp(b)));
    let page_rank:Vec<_>=order.iter().take(100).map(|i|json!({"entityId":graph.entities[*i].id,"score":rank[*i],"degree":out[*i].len()+incoming[*i].len()})).collect();
    let mut seen = vec![false; n];
    let mut components = Vec::new();
    for a in 0..n {
        if seen[a] {
            continue;
        }
        let mut queue = VecDeque::from([a]);
        seen[a] = true;
        let mut component = Vec::new();
        while let Some(i) = queue.pop_front() {
            deadline.check()?;
            component.push(graph.entities[i].id.clone());
            for &b in &weak[i] {
                if !seen[b] {
                    seen[b] = true;
                    queue.push_back(b);
                }
            }
        }
        components.push(component);
    }
    // Community detection is a separate projection; original directions and parallel evidence survive.
    ensure!(
        n <= 10000 && graph.observations.len() <= 100000,
        "Native community analysis budget exceeded"
    );
    let mut builder = leiden_rs::GraphDataBuilder::new(n);
    for edge in &graph.observations {
        if let Some(target) = &edge.target {
            builder.add_edge(ids[edge.source.as_str()], ids[target.as_str()], 1.0)?;
        }
    }
    let network = builder.build()?;
    let result = leiden_rs::Leiden::new(leiden_rs::LeidenConfig {
        max_iterations: 10,
        seed: Some(42),
        ..Default::default()
    })
    .run(&network)?;
    deadline.check()?;
    let communities: Vec<_> = result.partition.communities().iter().map(|(_, nodes)| json!({"members": nodes.iter().map(|i| &graph.entities[*i].id).collect::<Vec<_>>()})).collect();
    // Iterative Kosaraju keeps deep repositories off the Rust call stack.
    let mut finished = Vec::new();
    let mut seen = vec![false; n];
    for a in 0..n {
        if seen[a] {
            continue;
        }
        let mut stack = vec![(a, false)];
        while let Some((i, exit)) = stack.pop() {
            deadline.check()?;
            if exit {
                finished.push(i);
                continue;
            }
            if seen[i] {
                continue;
            }
            seen[i] = true;
            stack.push((i, true));
            for &b in out[i].iter().rev() {
                if !seen[b] {
                    stack.push((b, false));
                }
            }
        }
    }
    seen.fill(false);
    let mut cycles = Vec::new();
    for &a in finished.iter().rev() {
        if seen[a] {
            continue;
        }
        let mut stack = vec![a];
        seen[a] = true;
        let mut members = Vec::new();
        while let Some(i) = stack.pop() {
            members.push(i);
            for &b in &incoming[i] {
                if !seen[b] {
                    seen[b] = true;
                    stack.push(b);
                }
            }
        }
        if members.len() > 1 || out[a].contains(&a) {
            cycles.push(
                members
                    .into_iter()
                    .map(|i| &graph.entities[i].id)
                    .collect::<Vec<_>>(),
            );
        }
    }
    Ok(
        json!({"pageRank":page_rank,"components":components,"cycles":cycles,"communities":communities,"communityAlgorithm":"leiden","communityProducer":"leiden-rs-0.8.1","communitySeed":42,"communityQuality":result.quality,"communityProjection":"undirected-weighted-multiedges","complete":false}),
    )
}
pub fn require_kind(kind: &str) -> Result<()> {
    if !matches!(
        kind,
        "symbol" | "file" | "all" | "text" | "document" | "concept" | "rationale" | "api_contract"
    ) {
        bail!("Unsupported search kind: {kind}");
    }
    Ok(())
}
