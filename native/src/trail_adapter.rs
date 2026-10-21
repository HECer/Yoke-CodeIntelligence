//! Independent adapters for the six pinned Trail/Graft public query contracts.
use crate::{capture::Deadline, project_session::Session};
use anyhow::{Context, Result, bail, ensure};
use serde_json::{Value, json};
use std::collections::BTreeMap;

const LOGICAL_ENVELOPE_LIMIT: usize = 4_000_000;

#[derive(serde::Serialize)]
struct LogicalEnvelope<'a> {
    status: &'static str,
    data: &'a Value,
}

struct EnvelopeCounter<'a> {
    deadline: &'a Deadline,
    limit: usize,
    size: usize,
    failure: Option<crate::retrieval_scopes::RetrievalHardError>,
}

impl std::io::Write for EnvelopeCounter<'_> {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        use crate::retrieval_scopes::{RetrievalHardError, check_deadline};
        if self.failure.is_none() {
            self.failure = check_deadline(self.deadline).err();
        }
        if self.failure.is_none() && bytes.len() > self.limit.saturating_sub(self.size) {
            self.failure = Some(RetrievalHardError::OutputCapacity("compatibility_envelope"));
        }
        if self.failure.is_some() {
            return Err(std::io::Error::other("Logical envelope admission stopped"));
        }
        self.size += bytes.len();
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        if self.failure.is_none() {
            self.failure = crate::retrieval_scopes::check_deadline(self.deadline).err();
        }
        if self.failure.is_some() {
            Err(std::io::Error::other("Logical envelope admission stopped"))
        } else {
            Ok(())
        }
    }
}

/// Count the exact logical response without copying its definitions or buffering JSON.
fn logical_envelope_size(
    data: &Value,
    limit: usize,
    deadline: &Deadline,
) -> crate::retrieval_scopes::RetrievalResult<usize> {
    use crate::retrieval_scopes::{RetrievalHardError, check_deadline};
    check_deadline(deadline)?;
    let mut counter = EnvelopeCounter {
        deadline,
        limit,
        size: 0,
        failure: None,
    };
    let encoded = serde_json::to_writer(&mut counter, &LogicalEnvelope { status: "ok", data });
    if let Some(failure) = counter.failure {
        return Err(failure);
    }
    encoded.map_err(|_| RetrievalHardError::Internal("compatibility_envelope_encoding"))?;
    check_deadline(deadline)?;
    Ok(counter.size)
}

#[cfg(test)]
#[path = "trail_envelope_tests.rs"]
mod envelope_tests;

const WALK_RELATIONS: &[&str] = &[
    "calls",
    "references",
    "imports",
    "implements",
    "extends",
    "inherits",
];
fn definitions() -> Vec<(&'static str, &'static str, Value)> {
    vec![
        (
            "find_code",
            "Rank captured source bodies using automatic manifest-balanced package scopes when usable, with observable bounded global fallback; exact source anchors accompany heuristic scores.",
            json!({"query":{"type":"string"},"limit":{"type":"number","default":5},"full":{"type":"boolean","default":false},"in":{"type":"string"}}),
        ),
        (
            "find_all",
            "Search captured source with exact Unicode byte ranges and innermost symbol ownership; output coverage is bounded and explicit.",
            json!({"pattern":{"type":"string"},"in":{"type":"string"},"ignore_case":{"type":"boolean","default":false},"fixed":{"type":"boolean","default":false}}),
        ),
        (
            "trace_calls",
            "Trace named symbols or files through captured directed observations, including budgeted complete closure.",
            json!({"symbol":{"type":"string"},"direction":{"enum":["in","out"],"default":"in"},"depth":{"anyOf":[{"type":"integer"},{"enum":["all"]}],"default":1},"in":{"type":"string"}}),
        ),
        (
            "file_api",
            "Read body-free multiline declaration headers for an exact captured file or unique basename.",
            json!({"file":{"type":"string"}}),
        ),
        (
            "repo_map",
            "Orient the captured repository by directory and structural incoming coupling; ownership is not inferred as fact.",
            json!({"max_dirs":{"type":"number","default":16}}),
        ),
        (
            "check_freshness",
            "Compare captured disk drift against the last disk index without refreshing or publishing a graph first.",
            json!({}),
        ),
    ]
}
fn schema(name: &str, properties: Value) -> Value {
    let required = match name {
        "find_code" => vec!["query"],
        "find_all" => vec!["pattern"],
        "trace_calls" => vec!["symbol"],
        "file_api" => vec!["file"],
        _ => vec![],
    };
    json!({"type":"object","properties":properties,"required":required,"additionalProperties":false})
}
pub fn tools(prefix: &str) -> Value {
    let prefix = if prefix == "trail" { "trail" } else { "graft" };
    Value::Array(definitions().into_iter().map(|(name,description,properties)|json!({"name":format!("{prefix}_{name}"),"description":description,"inputSchema":schema(name,properties),"annotations":{"readOnlyHint":true,"destructiveHint":false,"openWorldHint":false}})).collect())
}
fn operation(name: &str) -> Result<&str> {
    let operation = name
        .strip_prefix("graft_")
        .or_else(|| name.strip_prefix("trail_"))
        .context("Unknown Trail tool prefix")?;
    Ok(match operation {
        "ask" => "find_code",
        "grep" => "find_all",
        "callers" => "trace_calls",
        "skeleton" => "file_api",
        "map" => "repo_map",
        "check" => "check_freshness",
        name => name,
    })
}
/// One owned query identity across both public prefixes and their legacy aliases.
pub(crate) fn steering_identity(name: &str) -> Option<&str> {
    operation(name).ok().filter(|op| {
        matches!(
            *op,
            "find_code" | "find_all" | "trace_calls" | "file_api" | "repo_map" | "check_freshness"
        )
    })
}
fn count(value: &Value, maximum: usize) -> Result<usize> {
    let n = value
        .as_u64()
        .filter(|n| *n >= 1 && *n <= maximum as u64)
        .context("Count must be a bounded positive integer")?;
    Ok(n as usize)
}
pub fn invoke(
    session: &mut Session,
    name: &str,
    args: &Value,
    deadline: &Deadline,
) -> Result<Value> {
    deadline.check()?;
    ensure!(
        session.tool_allowed(name),
        "Tool excluded by session steering"
    );
    let operation = operation(name)?;
    let (_, _, properties) = definitions()
        .into_iter()
        .find(|(name, _, _)| *name == operation)
        .context("Unknown Trail query")?;
    let args = crate::compatibility::arguments(args, &schema(operation, properties))?;
    let engine = session.active_engine_at(deadline)?;
    if operation == "check_freshness" {
        return crate::freshness::inspect(engine, deadline);
    }
    let (snapshot, graph, _, _) = if operation == "find_code" {
        engine.index_retrieval_at(deadline)?
    } else {
        engine.index(deadline)?
    };
    match operation {
        "file_api" => crate::file_api::inspect_all(
            &engine.store,
            &snapshot,
            &graph,
            args["file"].as_str().context("File required")?,
            deadline,
        ),
        "find_code" => {
            let limit = count(&args["limit"], 200)?;
            let options = json!({"query":args["query"],"limit":limit,"relativePath":args["in"].as_str().unwrap_or(""),"includeBody":true});
            let envelope_overhead =
                logical_envelope_size(&Value::Null, LOGICAL_ENVELOPE_LIMIT, deadline)? - 4;
            let output_limit = LOGICAL_ENVELOPE_LIMIT
                .checked_sub(envelope_overhead)
                .ok_or(crate::retrieval_scopes::RetrievalHardError::OutputCapacity(
                    "compatibility_envelope",
                ))?;
            let mut result = crate::retrieval::find_compatibility_auto(
                &engine.store,
                &snapshot,
                &graph,
                &options,
                deadline,
                output_limit,
            )?;
            let scope = crate::capture::source_scope_policy(&engine.store, &snapshot)?;
            let mut sources = BTreeMap::new();
            let mut source_bytes = 0usize;
            let mut output_bytes = 0usize;
            let query = args["query"]
                .as_str()
                .context("Query required")?
                .to_lowercase();
            let term_text = crate::query::identifier_terms(&query);
            let terms: Vec<_> = term_text
                .split_whitespace()
                .filter(|term| term.len() >= 2)
                .take(32)
                .collect();
            let full = args["full"] == true;
            for item in result["items"]
                .as_array_mut()
                .context("Retrieval items missing")?
            {
                deadline.check()?;
                let path = item["path"].as_str().context("Retrieval path missing")?;
                if !sources.contains_key(path) {
                    let bytes = scope.read_source(path)?;
                    source_bytes = source_bytes
                        .checked_add(bytes.len())
                        .context("Source budget overflow")?;
                    ensure!(
                        source_bytes <= 32 * 1024 * 1024,
                        "Selected definition sources exceed the 32 MiB budget"
                    );
                    sources.insert(path.to_owned(), String::from_utf8(bytes)?);
                }
                let text = &sources[path];
                let start = item["startByte"]
                    .as_u64()
                    .context("Definition start missing")? as usize;
                let end = item["endByte"].as_u64().context("Definition end missing")? as usize;
                let body = text
                    .get(start..end)
                    .context("Definition source range invalid")?;
                let (body_start, body_end) = if full {
                    (start, end)
                } else {
                    let mut offsets = vec![0usize];
                    offsets.extend(
                        body.match_indices('\n')
                            .map(|(at, _)| at + 1)
                            .filter(|at| *at < body.len()),
                    );
                    let mut hit = 0usize;
                    let mut best = 0usize;
                    for (line, at) in offsets.iter().enumerate() {
                        deadline.check()?;
                        let next = offsets.get(line + 1).copied().unwrap_or(body.len());
                        let text = body[*at..next].to_lowercase();
                        let score = if text.contains(&query) {
                            terms.len() + 1
                        } else {
                            terms.iter().filter(|term| text.contains(**term)).count()
                        };
                        if score > best {
                            best = score;
                            hit = line;
                        }
                    }
                    let first = hit.saturating_sub(3).min(offsets.len().saturating_sub(8));
                    (
                        start + offsets[first],
                        start + offsets.get(first + 8).copied().unwrap_or(body.len()),
                    )
                };
                output_bytes = output_bytes
                    .checked_add(body_end - body_start)
                    .context("Definition output overflow")?;
                ensure!(
                    output_bytes <= 32 * 1024 * 1024,
                    "Selected definition output exceeds the 32 MiB budget"
                );
                item["body"] = json!(&text[body_start..body_end]);
                item["bodyStartByte"] = json!(body_start);
                item["bodyEndByte"] = json!(body_end);
                item["bodyTruncated"] = json!(body_start > start || body_end < end);
                item["bodyMode"] = json!(if full {
                    "full_definition"
                } else {
                    "eight_line_crux"
                });
            }
            result["fullDefinitionRequested"] = args["full"].clone();
            result["definitionOutputBytes"] = json!(output_bytes);
            result["adapterLimitations"] = json!([]);
            // Definitions can grow after ranking's 16k excerpts. Admit the exact
            // final logical envelope before the existing protocol artifact branch.
            logical_envelope_size(&result, LOGICAL_ENVELOPE_LIMIT, deadline)?;
            Ok(result)
        }
        "find_all" => {
            let mut result = crate::query_features::text_find(
                &engine.store,
                &snapshot,
                &graph,
                &json!({"query":args["pattern"],"regex":args["fixed"]!=true,"ignoreCase":args["ignore_case"],"relativePath":args["in"].as_str().unwrap_or(""),"groupBySymbol":true,"limit":100000,"_minimalOccurrences":true,"_boundedTrailOccurrences":true}),
                deadline,
            )?;
            let mut incoming = BTreeMap::new();
            for edge in &graph.observations {
                deadline.check()?;
                if !WALK_RELATIONS.contains(&edge.relation.as_str()) {
                    continue;
                }
                if let Some(target) = &edge.target {
                    *incoming.entry(target.as_str()).or_insert(0usize) += 1;
                }
            }
            if let Some(items) = result["items"].as_array_mut() {
                for item in items.iter_mut() {
                    let coupling = item["symbol"]["id"]
                        .as_str()
                        .and_then(|id| incoming.get(id))
                        .copied()
                        .unwrap_or(0);
                    item["incomingCoupling"] = json!(coupling);
                }
                items.sort_by(|a, b| {
                    b["incomingCoupling"]
                        .as_u64()
                        .cmp(&a["incomingCoupling"].as_u64())
                        .then_with(|| a["path"].as_str().cmp(&b["path"].as_str()))
                        .then_with(|| a["startByte"].as_u64().cmp(&b["startByte"].as_u64()))
                });
            }
            result["adapterLimitations"] = json!([
                "At most 100000 occurrences and 32 MiB of occurrence rows are returned; total, totalLowerBound and truncation report actual coverage."
            ]);
            Ok(result)
        }
        "trace_calls" => {
            let symbol = args["symbol"].as_str().context("Symbol required")?;
            ensure!(
                !symbol.is_empty() && symbol.len() <= 16000,
                "Invalid symbol selector"
            );
            let filter = crate::query_features::PathFilter::new(
                &json!({"relativePath":args["in"].as_str().unwrap_or("")}),
            )?;
            let qualified = symbol.replace('.', "/");
            let suffix = format!("/{qualified}");
            let files: Vec<_> = snapshot
                .files
                .iter()
                .filter(|file| {
                    filter.matches(&file.path)
                        && (file.path == symbol
                            || (!symbol.contains('/')
                                && file.path.rsplit('/').next() == Some(symbol)))
                })
                .map(|file| file.path.as_str())
                .collect();
            let mut roots = Vec::new();
            let root_limit = if files.is_empty() { 200 } else { 100000 };
            for entity in &graph.entities {
                deadline.check()?;
                if filter.matches(&entity.path)
                    && (entity.name == symbol
                        || entity.qualified_name == symbol
                        || entity.qualified_name == qualified
                        || entity.qualified_name.ends_with(&suffix)
                        || files.contains(&entity.path.as_str()))
                {
                    ensure!(
                        roots.len() < root_limit,
                        "Symbol selector matches too many declarations; qualify it"
                    );
                    roots.push(entity.id.clone());
                }
            }
            ensure!(
                !roots.is_empty(),
                "No captured symbol or file matches selector"
            );
            ensure!(
                roots.len() <= root_limit,
                "Symbol selector matches too many declarations; qualify it"
            );
            let depth = if args["depth"] == "all" {
                graph.entities.len().max(1)
            } else {
                args["depth"]
                    .as_u64()
                    .filter(|n| *n <= 100000)
                    .context("Invalid trace depth")? as usize
            };
            let mut result = crate::query::trace(
                &graph,
                &json!({"entityIds":roots,"direction":args["direction"],"depth":depth,"maxVisited":100000,"relations":WALK_RELATIONS}),
                deadline,
            )?;
            result["snapshotId"] = json!(snapshot.id);
            result["selector"] = json!(symbol);
            result["matchedRoots"] = json!(roots);
            result["nativeRelationFallbacks"] = json!(["inherits"]);
            Ok(result)
        }
        "repo_map" => {
            let max = count(&args["max_dirs"], 1000)?;
            let ranked = |a: &Value, b: &Value| {
                b["incomingCoupling"]
                    .as_u64()
                    .cmp(&a["incomingCoupling"].as_u64())
                    .then_with(|| a["entityId"].as_str().cmp(&b["entityId"].as_str()))
            };
            let mut incoming = BTreeMap::new();
            for edge in &graph.observations {
                deadline.check()?;
                if !WALK_RELATIONS.contains(&edge.relation.as_str()) {
                    continue;
                }
                if let Some(target) = &edge.target {
                    *incoming.entry(target.as_str()).or_insert(0usize) += 1;
                }
            }
            let mut directories: BTreeMap<String, (usize, Vec<Value>)> = BTreeMap::new();
            for file in &snapshot.files {
                deadline.check()?;
                let directory = file.path.rsplit_once('/').map_or(".", |(parent, _)| parent);
                directories.entry(directory.into()).or_default().0 += 1;
            }
            let mut hotspots = Vec::new();
            for entity in &graph.entities {
                deadline.check()?;
                if entity.kind == "file" {
                    continue;
                }
                let coupling = incoming.get(entity.id.as_str()).copied().unwrap_or(0);
                let row = json!({"entityId":entity.id,"name":entity.qualified_name,"path":entity.path,"startByte":entity.start_byte,"endByte":entity.end_byte,"incomingCoupling":coupling});
                let directory = entity
                    .path
                    .rsplit_once('/')
                    .map_or(".", |(parent, _)| parent);
                let hubs = &mut directories.entry(directory.into()).or_default().1;
                hubs.push(row.clone());
                hubs.sort_by(ranked);
                hubs.truncate(5);
                hotspots.push(row);
                hotspots.sort_by(ranked);
                hotspots.truncate(20);
            }
            hotspots.sort_by(ranked);
            hotspots.truncate(20);
            let total = directories.len();
            let rows: Vec<_> = directories
                .into_iter()
                .take(max)
                .map(|(path, (files, mut hubs))| {
                    hubs.sort_by(ranked);
                    hubs.truncate(5);
                    json!({"path":path,"files":files,"hubs":hubs})
                })
                .collect();
            Ok(
                json!({"snapshotId":snapshot.id,"directories":rows,"directoryCount":total,"droppedDirectories":total.saturating_sub(max),"hotspots":hotspots,"captureOmissions":snapshot.omissions,"complete":false,"ranking":"structural_incoming_coupling"}),
            )
        }
        _ => bail!("Unsupported Trail query"),
    }
}
