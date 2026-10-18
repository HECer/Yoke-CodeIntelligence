//! Source-bound agent search and reviewable occurrence edits.
use crate::{
    Engine,
    capture::Deadline,
    changes,
    model::{Entity, Graph, Snapshot, identity},
    query,
    store::Store,
};
use anyhow::{Context, Result, ensure};
use regex::{Regex, RegexBuilder};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

const MAX_MATCHES: usize = 10_000;
const MAX_READ_BYTES: usize = 32_000_000;
const MAX_DIRECTORY_BYTES: usize = 32_000_000;

struct FilesListOptions<'a> {
    relative: &'a str,
    recursive: bool,
    skip_ignored: bool,
}
impl<'a> FilesListOptions<'a> {
    fn parse(args: &'a Value) -> Result<Self> {
        let object = args
            .as_object()
            .context("Directory arguments must be an object")?;
        ensure!(
            object
                .keys()
                .all(|key| ["relativePath", "recursive", "skipIgnored"].contains(&key.as_str())),
            "Unknown directory argument"
        );
        for flag in ["recursive", "skipIgnored"] {
            ensure!(
                args.get(flag).is_none_or(Value::is_boolean),
                "Directory flag must be boolean: {flag}"
            );
        }
        let relative = match args.get("relativePath") {
            Some(value) => value
                .as_str()
                .context("Directory relativePath must be a string")?,
            None => "",
        };
        if !relative.is_empty() && relative != "." {
            crate::paths::normalized_syntax(relative)?;
        }
        Ok(Self {
            relative,
            recursive: args["recursive"] == true,
            skip_ignored: args["skipIgnored"] == true,
        })
    }
}
/// Validate before opening or capturing an active or registered external project.
pub fn validate_files_list_args(args: &Value) -> Result<()> {
    FilesListOptions::parse(args).map(|_| ())
}
/// Project only immutable captured members; ignore metadata never grants reads.
pub fn files_list(
    store: &Store,
    snapshot: &Snapshot,
    args: &Value,
    deadline: &Deadline,
) -> Result<Value> {
    let options = FilesListOptions::parse(args)?;
    store.set_deadline(deadline)?;
    ensure!(
        snapshot.workspace_id == store.workspace_id
            && identity(&store.snapshot(&snapshot.id)?) == identity(snapshot),
        "Directory source manifest mismatch"
    );
    let source_scope = crate::capture::source_scope_policy(store, snapshot)?;
    let prefix = if options.relative.is_empty() || options.relative == "." {
        String::new()
    } else {
        format!("{}/", options.relative)
    };
    let mut directories = BTreeSet::new();
    let mut directory_bytes = 0usize;
    let mut known_directory = prefix.is_empty();
    let mut files = Vec::new();
    for file in &snapshot.files {
        deadline.check()?;
        ensure!(
            prefix.is_empty() || file.path != options.relative,
            "Captured directory path identifies a source file"
        );
        if let Some(relative) = file.path.strip_prefix(&prefix) {
            known_directory = true;
            source_scope.path_policy(&file.path)?;
            if options.skip_ignored && source_scope.ignored_at_capture(&file.path)? {
                continue;
            }
            if options.recursive || !relative.contains('/') {
                files.push(json!({"path":file.path,"contentHash":file.hash,"size":file.size}));
            }
            for (boundary, _) in relative.match_indices('/') {
                deadline.check()?;
                let directory = &file.path[..prefix.len() + boundary];
                if !directories.contains(directory) {
                    ensure!(
                        directories.len() < 100_000
                            && directory.len()
                                <= MAX_DIRECTORY_BYTES.saturating_sub(directory_bytes),
                        "Directory projection capacity exceeded; refine the requested path"
                    );
                    directory_bytes += directory.len();
                    directories.insert(directory.to_owned());
                }
                if !options.recursive {
                    break;
                }
            }
        }
    }
    ensure!(
        known_directory,
        "Captured directory is unknown; empty or uncaptured directories are not represented"
    );
    let result = json!({"snapshotId":snapshot.id,"files":files,"directories":directories,"scope":"captured_source","omissions":snapshot.omissions,"complete":false});
    deadline.check()?;
    Ok(result)
}

pub fn source_scope(
    store: &Store,
    snapshot: &Snapshot,
    graph: &Graph,
    deadline: &Deadline,
) -> Result<()> {
    store.set_deadline(deadline)?;
    ensure!(
        snapshot.workspace_id == store.workspace_id
            && identity(&store.snapshot(&snapshot.id)?) == identity(snapshot),
        "Search source manifest mismatch"
    );
    ensure!(
        graph.snapshot_id == snapshot.id
            && identity(&store.graph(&snapshot.id)?) == identity(graph),
        "Search graph manifest mismatch"
    );
    deadline.check()
}

fn glob_expression(pattern: &str, insensitive: bool) -> Result<Regex> {
    ensure!(
        pattern.len() <= 4096 && !pattern.contains('\0'),
        "Invalid glob"
    );
    let mut expression = String::from("^");
    let mut chars = pattern.chars().peekable();
    while let Some(ch) = chars.next() {
        match ch {
            '*' if chars.peek() == Some(&'*') => {
                chars.next();
                if chars.peek() == Some(&'/') {
                    chars.next();
                    expression.push_str("(?:.*/)?");
                } else {
                    expression.push_str(".*");
                }
            }
            '*' => expression.push_str("[^/]*"),
            '?' => expression.push_str("[^/]"),
            ch => expression.push_str(&regex::escape(&ch.to_string())),
        }
    }
    expression.push('$');
    Ok(RegexBuilder::new(&expression)
        .case_insensitive(insensitive)
        .size_limit(1_000_000)
        .dfa_size_limit(1_000_000)
        .build()?)
}

pub(crate) fn glob(pattern: &str, insensitive: bool) -> Result<Regex> {
    ensure!(
        !pattern.contains(['\\', ':'])
            && !pattern.starts_with('/')
            && !pattern.split('/').any(|part| part == ".."),
        "Invalid relative glob"
    );
    glob_expression(pattern, insensitive)
}

fn name_path(entity: &Entity, by_key: &BTreeMap<&str, &Entity>) -> String {
    let mut names = vec![entity.name.as_str()];
    let mut parent = entity.parent_key.as_deref();
    let mut visited = BTreeSet::new();
    while let Some(key) = parent {
        if !visited.insert(key) {
            break;
        }
        let Some(owner) = by_key.get(key) else {
            break;
        };
        if owner.kind != "file" {
            names.push(owner.name.as_str());
        }
        parent = owner.parent_key.as_deref();
    }
    names.reverse();
    names.join("/")
}

pub struct PathFilter {
    relative: Option<String>,
    include: Option<Regex>,
    exclude: Option<Regex>,
    legacy: String,
    skip_ignored: bool,
}
impl PathFilter {
    pub fn new(args: &Value) -> Result<Self> {
        ensure!(
            args.get("skipIgnored").is_none_or(Value::is_boolean),
            "skipIgnored must be boolean"
        );
        let relative = args["relativePath"]
            .as_str()
            .filter(|path| !path.is_empty() && *path != ".")
            .map(crate::paths::normalized_syntax)
            .transpose()?;
        Ok(Self {
            relative,
            include: args["pathGlob"]
                .as_str()
                .map(|path| glob(path, false))
                .transpose()?,
            exclude: args["excludeGlob"]
                .as_str()
                .map(|path| glob(path, false))
                .transpose()?,
            legacy: args["path"].as_str().unwrap_or("").into(),
            skip_ignored: args["skipIgnored"] == true,
        })
    }
    pub fn matches(&self, path: &str) -> bool {
        self.relative.as_ref().is_none_or(|relative| {
            path == relative
                || path
                    .strip_prefix(relative)
                    .is_some_and(|suffix| suffix.starts_with('/'))
        }) && self
            .include
            .as_ref()
            .is_none_or(|include| include.is_match(path))
            && self
                .exclude
                .as_ref()
                .is_none_or(|exclude| !exclude.is_match(path))
            && path.contains(&self.legacy)
    }
    fn matches_captured(
        &self,
        scope: &crate::capture::CapturedSourceScope<'_>,
        path: &str,
    ) -> Result<bool> {
        Ok(self.matches(path) && (!self.skip_ignored || !scope.ignored_at_capture(path)?))
    }
}

fn clip(text: &str, max: usize) -> (&str, bool) {
    let mut end = text.len().min(max);
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    (&text[..end], end != text.len())
}

type SourceCache = BTreeMap<String, (String, String)>;
fn source_cache(
    store: &Store,
    snapshot: &Snapshot,
    scope: &crate::capture::CapturedSourceScope<'_>,
    filter: &PathFilter,
    deadline: &Deadline,
) -> Result<(SourceCache, Vec<String>)> {
    let mut cache = BTreeMap::new();
    let mut omitted = Vec::new();
    let mut bytes = 0usize;
    for file in &snapshot.files {
        deadline.check()?;
        if !filter.matches_captured(scope, &file.path)? {
            continue;
        }
        if bytes.saturating_add(file.size) > MAX_READ_BYTES {
            omitted.push(file.path.clone());
            continue;
        }
        let (text, hash) = query::source_in_scope(store, scope, &file.path)?;
        bytes = bytes
            .checked_add(text.len())
            .context("Source cache size overflow")?;
        ensure!(bytes <= MAX_READ_BYTES, "Source cache byte budget exceeded");
        cache.insert(file.path.clone(), (text, hash));
    }
    Ok((cache, omitted))
}

fn occurrence_id(
    snapshot: &Snapshot,
    path: &str,
    hash: &str,
    start: usize,
    end: usize,
    matched: &str,
    query_identity: &str,
) -> String {
    identity(&(
        "native-source-occurrence-v1",
        &snapshot.id,
        path,
        hash,
        start,
        end,
        matched,
        query_identity,
    ))
}

fn owner<'a>(entities: &[&'a Entity], start: usize, end: usize) -> Option<&'a Entity> {
    entities
        .iter()
        .copied()
        .filter(|entity| entity.start_byte <= start && entity.end_byte >= end)
        .min_by_key(|entity| {
            (
                entity.end_byte.saturating_sub(entity.start_byte),
                &entity.id,
            )
        })
}

fn pattern(args: &Value) -> Result<Regex> {
    let query = args["query"].as_str().context("Search query required")?;
    ensure!(
        !query.is_empty() && query.len() <= 16_000,
        "Invalid search pattern"
    );
    let expression = if args["regex"] == true {
        query.to_owned()
    } else {
        regex::escape(query)
    };
    Ok(RegexBuilder::new(&expression)
        .case_insensitive(args["ignoreCase"] == true)
        .multi_line(args["multiline"] == true)
        .dot_matches_new_line(args["multiline"] == true)
        .size_limit(1_000_000)
        .dfa_size_limit(1_000_000)
        .build()?)
}

fn context(
    text: &str,
    lines: &[usize],
    start: usize,
    end: usize,
    before: usize,
    after: usize,
) -> (String, usize, usize, bool) {
    let first = lines
        .partition_point(|position| *position <= start)
        .saturating_sub(1);
    let last = lines
        .partition_point(|position| *position <= end)
        .saturating_sub(1);
    let natural_begin = lines[first.saturating_sub(before)];
    let finish = lines
        .get(last.saturating_add(after).saturating_add(1))
        .map_or(text.len(), |next| next.saturating_sub(1));
    let mut begin = natural_begin;
    if finish.saturating_sub(begin) > 16000 {
        begin = if end.saturating_sub(start) > 16000 {
            start
        } else {
            start
                .saturating_sub(8000)
                .max(natural_begin)
                .max(end.saturating_sub(16000))
        };
        while !text.is_char_boundary(begin) {
            begin += 1;
        }
    }
    let (snippet, truncated) = clip(&text[begin..finish], 16_000);
    (
        snippet.into(),
        begin,
        begin + snippet.len(),
        truncated || begin != natural_begin,
    )
}

pub fn text_find(
    store: &Store,
    snapshot: &Snapshot,
    graph: &Graph,
    args: &Value,
    deadline: &Deadline,
) -> Result<Value> {
    source_scope(store, snapshot, graph, deadline)?;
    text_find_verified(store, snapshot, graph, args, deadline)
}
pub fn text_find_selected(
    store: &Store,
    snapshot: &Snapshot,
    graph: &Graph,
    args: &Value,
    selected: &BTreeSet<String>,
    deadline: &Deadline,
) -> Result<Value> {
    source_scope(store, snapshot, graph, deadline)?;
    let captured: BTreeSet<_> = snapshot
        .files
        .iter()
        .map(|file| file.path.as_str())
        .collect();
    for path in selected {
        deadline.check()?;
        crate::paths::normalized_syntax(path)?;
        ensure!(
            captured.contains(path.as_str()),
            "Selected search path is not captured"
        );
    }
    text_find_filtered(store, snapshot, graph, args, Some(selected), deadline)
}
pub(crate) fn text_find_verified(
    store: &Store,
    snapshot: &Snapshot,
    graph: &Graph,
    args: &Value,
    deadline: &Deadline,
) -> Result<Value> {
    text_find_filtered(store, snapshot, graph, args, None, deadline)
}
fn text_find_filtered(
    store: &Store,
    snapshot: &Snapshot,
    graph: &Graph,
    args: &Value,
    selected: Option<&BTreeSet<String>>,
    deadline: &Deadline,
) -> Result<Value> {
    let scope = crate::capture::source_scope_policy(store, snapshot)?;
    text_find_filtered_in_scope(store, snapshot, graph, args, selected, &scope, deadline)
}
pub(crate) fn text_find_in_scope(
    store: &Store,
    snapshot: &Snapshot,
    graph: &Graph,
    args: &Value,
    scope: &crate::capture::CapturedSourceScope<'_>,
    deadline: &Deadline,
) -> Result<Value> {
    text_find_filtered_in_scope(store, snapshot, graph, args, None, scope, deadline)
}
fn text_find_filtered_in_scope(
    store: &Store,
    snapshot: &Snapshot,
    graph: &Graph,
    args: &Value,
    selected: Option<&BTreeSet<String>>,
    scope: &crate::capture::CapturedSourceScope<'_>,
    deadline: &Deadline,
) -> Result<Value> {
    let filter = PathFilter::new(args)?;
    let expression = pattern(args)?;
    let query_identity = identity(&(
        args["query"].as_str(),
        args["regex"] == true,
        args["ignoreCase"] == true,
        args["multiline"] == true,
    ));
    let omitted: Vec<String> = Vec::new();
    let limit = args["limit"].as_u64().unwrap_or(30) as usize;
    let offset = args["offset"].as_u64().unwrap_or(0) as usize;
    let scan_limit = MAX_MATCHES.max(offset.saturating_add(limit).saturating_add(1));
    let before = args["beforeContextLines"].as_u64().unwrap_or(0) as usize;
    let after = args["afterContextLines"].as_u64().unwrap_or(0) as usize;
    let mut items = Vec::new();
    let mut total = 0usize;
    let mut capped = false;
    let mut output_bytes = 0usize;
    let mut owners: BTreeMap<&str, Vec<&Entity>> = BTreeMap::new();
    if args["groupBySymbol"] == true {
        for entity in &graph.entities {
            deadline.check()?;
            if entity.kind != "file" {
                owners.entry(entity.path.as_str()).or_default().push(entity);
            }
        }
    }
    let mut files: Vec<_> = snapshot
        .files
        .iter()
        .filter(|file| {
            filter.matches(&file.path) && selected.is_none_or(|paths| paths.contains(&file.path))
        })
        .collect();
    files.sort_by(|a, b| a.path.cmp(&b.path));
    for file in files {
        deadline.check()?;
        let path = &file.path;
        if filter.skip_ignored && scope.ignored_at_capture(path)? {
            continue;
        }
        let (text, hash) = query::source_in_scope(store, scope, path)?;
        let line_starts = crate::paths::line_starts(&text);
        for matched in expression.find_iter(&text) {
            deadline.check()?;
            if total == scan_limit {
                capped = true;
                break;
            }
            let start = matched.start();
            let end = matched.end();
            if total >= offset && items.len() < limit {
                let (snippet, context_start, context_end, truncated) =
                    if args["_minimalOccurrences"] == true {
                        let (snippet, truncated) = clip(matched.as_str(), 500);
                        (snippet.to_owned(), start, start + snippet.len(), truncated)
                    } else {
                        context(&text, &line_starts, start, end, before, after)
                    };
                let line = line_starts.partition_point(|position| *position <= start);
                let mut item = json!({"path":path,"contentHash":hash,"startByte":start,"endByte":end,"startLine":line,"text":clip(matched.as_str(), 500).0,"matchedTextTruncated":matched.len()>500,"context":snippet,"contextStartByte":context_start,"contextEndByte":context_end,"contextTruncated":truncated,"occurrenceId":occurrence_id(snapshot,path,&hash,start,end,matched.as_str(),&query_identity),"trust":"source-data"});
                if args["groupBySymbol"] == true {
                    item["symbol"] = owner(
                        owners.get(path.as_str()).map_or(&[], Vec::as_slice),
                        start,
                        end,
                    )
                    .map(serde_json::to_value)
                    .transpose()?
                    .unwrap_or(Value::Null);
                }
                if args["_boundedTrailOccurrences"] == true {
                    let size = serde_json::to_vec(&item)?.len() + 1;
                    if output_bytes.saturating_add(size) > 32 * 1024 * 1024 {
                        total += 1;
                        capped = true;
                        break;
                    }
                    output_bytes += size;
                }
                items.push(item);
            }
            total += 1;
        }
        if capped {
            break;
        }
    }
    Ok(
        json!({"items":items,"offset":offset,"total":if capped || !omitted.is_empty(){None}else{Some(total)},"totalLowerBound":total,"truncated":capped || !omitted.is_empty() || total>offset+items.len(),"omittedPaths":omitted,"scope":"captured_source","complete":false}),
    )
}

pub fn symbols_find(
    store: &Store,
    snapshot: &Snapshot,
    graph: &Graph,
    args: &Value,
    deadline: &Deadline,
) -> Result<Value> {
    source_scope(store, snapshot, graph, deadline)?;
    symbols_find_verified(store, snapshot, graph, args, deadline)
}
pub(crate) fn symbols_find_verified(
    store: &Store,
    snapshot: &Snapshot,
    graph: &Graph,
    args: &Value,
    deadline: &Deadline,
) -> Result<Value> {
    let scope = crate::capture::source_scope_policy(store, snapshot)?;
    symbols_find_in_scope(store, snapshot, graph, args, &scope, deadline)
}
pub(crate) fn symbols_find_in_scope(
    store: &Store,
    snapshot: &Snapshot,
    graph: &Graph,
    args: &Value,
    scope: &crate::capture::CapturedSourceScope<'_>,
    deadline: &Deadline,
) -> Result<Value> {
    let filter = PathFilter::new(args)?;
    let kind = args["kind"].as_str().unwrap_or("symbol");
    let name = args["namePath"].as_str().filter(|name| !name.is_empty());
    let matcher = name
        .map(|name| glob_expression(name.trim_start_matches('/'), args["ignoreCase"] == true))
        .transpose()?;
    let include: BTreeSet<_> = args["includeKinds"]
        .as_array()
        .map(|values| values.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default();
    let exclude: BTreeSet<_> = args["excludeKinds"]
        .as_array()
        .map(|values| values.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default();
    let mut baseline_args = args.clone();
    baseline_args["query"] = json!(args["query"].as_str().unwrap_or(""));
    baseline_args["offset"] = json!(0);
    baseline_args["limit"] = json!(100_000);
    let base = query::find_base(store, snapshot, graph, &baseline_args, deadline)?;
    let ranks: BTreeMap<_, _> = base["items"]
        .as_array()
        .context("Symbol candidates missing")?
        .iter()
        .enumerate()
        .filter_map(|(rank, value)| value["id"].as_str().map(|id| (id.to_owned(), rank)))
        .collect();
    let mut selected: BTreeSet<String> = base["items"]
        .as_array()
        .context("Symbol candidates missing")?
        .iter()
        .filter_map(|value| value["id"].as_str().map(str::to_owned))
        .collect();
    let search_body = args["searchBody"] == true
        || (selected.is_empty()
            && args["query"]
                .as_str()
                .is_some_and(|query| !query.is_empty()));
    let mut cache = BTreeMap::new();
    let mut omitted = Vec::new();
    if search_body || args["includeBody"] == true {
        (cache, omitted) = source_cache(store, snapshot, scope, &filter, deadline)?;
    }
    if search_body {
        let terms: Vec<_> = args["query"]
            .as_str()
            .unwrap_or("")
            .split(|ch: char| !ch.is_alphanumeric() && ch != '_')
            .filter(|term| !term.is_empty())
            .map(str::to_lowercase)
            .collect();
        for entity in &graph.entities {
            deadline.check()?;
            if !filter.matches_captured(scope, &entity.path)?
                || entity.kind == "file" && kind == "symbol"
                || entity.kind != "file" && kind == "file"
            {
                continue;
            }
            if let Some((text, _)) = cache.get(&entity.path) {
                let body = text
                    .get(entity.start_byte..entity.end_byte)
                    .context("Symbol body range is invalid")?
                    .to_lowercase();
                if !terms.is_empty() && terms.iter().all(|term| body.contains(term)) {
                    selected.insert(entity.id.clone());
                }
            }
        }
    }
    let by_key: BTreeMap<_, _> = graph
        .entities
        .iter()
        .map(|entity| (entity.key.as_str(), entity))
        .collect();
    let mut seeds = BTreeSet::new();
    for entity in &graph.entities {
        deadline.check()?;
        if !selected.contains(&entity.id) || !filter.matches_captured(scope, &entity.path)? {
            continue;
        }
        let symbol_path = name_path(entity, &by_key);
        if matcher.as_ref().is_none_or(|matcher| {
            matcher.is_match(&symbol_path)
                || (!name.unwrap_or("").starts_with('/')
                    && symbol_path.split('/').enumerate().any(|(index, _)| {
                        matcher.is_match(
                            &symbol_path
                                .split('/')
                                .skip(index)
                                .collect::<Vec<_>>()
                                .join("/"),
                        )
                    }))
                || (args["substringMatching"] == true && symbol_path.contains(name.unwrap_or("")))
        }) {
            seeds.insert(entity.key.clone());
        }
    }
    let depth = args["depth"].as_u64().unwrap_or(0) as usize;
    let mut matches = Vec::new();
    for entity in &graph.entities {
        deadline.check()?;
        if !filter.matches_captured(scope, &entity.path)?
            || entity.kind == "file" && kind == "symbol"
            || entity.kind != "file" && kind == "file"
            || !include.is_empty() && !include.contains(entity.kind.as_str())
            || exclude.contains(entity.kind.as_str())
        {
            continue;
        }
        let mut selected = seeds.contains(&entity.key);
        let mut parent = entity.parent_key.as_deref();
        for _ in 0..depth {
            let Some(key) = parent else { break };
            if seeds.contains(key) {
                selected = true;
                break;
            }
            parent = by_key
                .get(key)
                .and_then(|parent| parent.parent_key.as_deref());
        }
        if selected {
            matches.push(entity);
        }
    }
    matches.sort_by_key(|entity| {
        (
            ranks.get(&entity.id).copied().unwrap_or(usize::MAX),
            &entity.path,
            entity.start_byte,
            &entity.kind,
            &entity.id,
        )
    });
    let offset = args["offset"].as_u64().unwrap_or(0) as usize;
    let limit = args["limit"].as_u64().unwrap_or(30) as usize;
    let mut items = Vec::new();
    for entity in matches.iter().skip(offset).take(limit) {
        let mut value = serde_json::to_value(entity)?;
        let file = snapshot
            .files
            .iter()
            .find(|file| file.path == entity.path)
            .context("Symbol source missing")?;
        value["contentHash"] = json!(file.hash);
        value["namePath"] = json!(name_path(entity, &by_key));
        if args["includeBody"] == true {
            if let Some((text, _)) = cache.get(&entity.path) {
                let source = text
                    .get(entity.start_byte..entity.end_byte)
                    .context("Symbol source range invalid")?;
                let (body, truncated) = clip(source, 16_000);
                value["body"] = json!(body);
                value["bodyEndByte"] = json!(entity.start_byte + body.len());
                value["bodyTruncated"] = json!(truncated);
            } else {
                value["body"] = Value::Null;
                value["bodyTruncated"] = json!(true);
            }
        }
        items.push(value);
    }
    Ok(
        json!({"items":items,"total":matches.len(),"offset":offset,"truncated":matches.len()>offset+limit || !omitted.is_empty(),"omittedPaths":omitted,"semantic":"structural_candidates","ranking":"lexical_not_evidence","complete":false}),
    )
}

pub fn bulk_replace(
    engine: &mut Engine,
    snapshot: &Snapshot,
    graph: &Graph,
    spec: &Value,
    deadline: &Deadline,
) -> Result<Value> {
    let object = spec
        .as_object()
        .context("Bulk replacement must be an object")?;
    ensure!(
        object.keys().all(|key| [
            "pattern",
            "replacement",
            "literal",
            "ignoreCase",
            "multiline",
            "relativePath",
            "pathGlob",
            "excludeGlob",
            "expectedCount",
            "occurrenceIds",
            "dryRun"
        ]
        .contains(&key.as_str())),
        "Unknown bulk replacement field"
    );
    for key in ["literal", "ignoreCase", "multiline", "dryRun"] {
        ensure!(
            spec.get(key).is_none_or(Value::is_boolean),
            "Invalid bulk flag"
        );
    }
    let replacement = spec["replacement"]
        .as_str()
        .context("Replacement text required")?;
    ensure!(
        replacement.len() <= 4_000_000 && !replacement.contains('\0'),
        "Invalid replacement text"
    );
    let query = spec["pattern"].as_str().context("Bulk pattern required")?;
    let args = json!({"query":query,"regex":spec["literal"]!=true,"ignoreCase":spec["ignoreCase"]==true,"multiline":spec["multiline"]==true,"relativePath":spec["relativePath"],"pathGlob":spec["pathGlob"],"excludeGlob":spec["excludeGlob"],"limit":MAX_MATCHES});
    let mut args = args;
    args["_minimalOccurrences"] = json!(true);
    let found = text_find(&engine.store, snapshot, graph, &args, deadline)?;
    ensure!(
        found["truncated"] == false,
        "Bulk replacement source/match scope exceeds budget"
    );
    let matches = found["items"].as_array().context("Bulk matches missing")?;
    if let Some(expected) = spec.get("expectedCount") {
        ensure!(
            expected
                .as_u64()
                .is_some_and(|count| count == matches.len() as u64),
            "Bulk expected-count mismatch"
        );
    }
    let mut selected = BTreeSet::new();
    let found_ids: BTreeSet<_> = matches
        .iter()
        .filter_map(|matched| matched["occurrenceId"].as_str())
        .collect();
    if let Some(ids) = spec.get("occurrenceIds") {
        let ids = ids.as_array().context("Occurrence IDs must be an array")?;
        ensure!(ids.len() <= MAX_MATCHES, "Too many occurrence IDs");
        for id in ids {
            deadline.check()?;
            let id = id.as_str().context("Invalid occurrence ID")?;
            ensure!(
                crate::model::valid_id(id) && selected.insert(id.to_owned()),
                "Invalid or duplicate occurrence ID"
            );
        }
        ensure!(!selected.is_empty(), "Empty occurrence selection");
        ensure!(
            selected.iter().all(|id| found_ids.contains(id.as_str())),
            "Stale or unknown occurrence ID"
        );
    }
    let chosen: Vec<_> = matches
        .iter()
        .filter(|matched| {
            selected.is_empty() || selected.contains(matched["occurrenceId"].as_str().unwrap_or(""))
        })
        .cloned()
        .collect();
    if spec["dryRun"] == true {
        return Ok(
            json!({"snapshotId":snapshot.id,"dryRun":true,"matchCount":matches.len(),"selectedCount":chosen.len(),"occurrences":chosen,"sourceWritten":false}),
        );
    }
    ensure!(!chosen.is_empty(), "No occurrences selected");
    ensure!(chosen.len() <= 2000, "Bulk edit count exceeds plan budget");
    let chosen_paths: BTreeSet<_> = chosen
        .iter()
        .filter_map(|matched| matched["path"].as_str())
        .collect();
    let mut selected_source_bytes = 0usize;
    for file in &snapshot.files {
        deadline.check()?;
        if chosen_paths.contains(file.path.as_str()) {
            selected_source_bytes = selected_source_bytes
                .checked_add(file.size)
                .context("Bulk source size overflow")?;
            ensure!(
                selected_source_bytes <= MAX_READ_BYTES,
                "Bulk selected source budget exceeded"
            );
        }
    }
    let (fresh, _, _, _) = engine.index(deadline)?;
    ensure!(fresh.id == snapshot.id, "Bulk source snapshot is stale");
    let expression = pattern(&args)?;
    let mut edits = Vec::new();
    let mut total = 0usize;
    let mut sources = SourceCache::new();
    for matched in &chosen {
        deadline.check()?;
        let path = matched["path"]
            .as_str()
            .context("Occurrence path missing")?;
        if !sources.contains_key(path) {
            sources.insert(
                path.to_owned(),
                query::source(&engine.store, snapshot, path)?,
            );
        }
        let (text, hash) = &sources[path];
        let start = matched["startByte"]
            .as_u64()
            .context("Occurrence start missing")? as usize;
        let end = matched["endByte"]
            .as_u64()
            .context("Occurrence end missing")? as usize;
        let captures = expression
            .captures_at(text, start)
            .context("Occurrence no longer matches")?;
        let whole = captures.get(0).context("Occurrence capture missing")?;
        ensure!(
            whole.start() == start && whole.end() == end,
            "Occurrence range mismatch"
        );
        let mut expanded = String::new();
        if spec["literal"] == true {
            ensure!(
                replacement.len() <= 4_000_000 - total,
                "Bulk replacement byte budget exceeded"
            );
            expanded.push_str(replacement);
        } else {
            expanded =
                crate::replacement::expand(&captures, replacement, 4_000_000 - total, deadline)?;
        }
        total = total
            .checked_add(expanded.len())
            .context("Replacement size overflow")?;
        ensure!(total <= 4_000_000, "Bulk replacement byte budget exceeded");
        edits.push(json!({"kind":"replace","path":path,"expectedHash":hash,"startByte":start,"endByte":end,"text":expanded}));
    }
    let plan = changes::prepare(
        &mut engine.store,
        snapshot,
        graph,
        &json!({"edits":edits}),
        deadline,
    )?;
    Ok(
        json!({"snapshotId":snapshot.id,"dryRun":false,"matchCount":matches.len(),"selectedCount":chosen.len(),"occurrences":chosen,"plan":plan,"sourceWritten":false,"requiresApproval":true}),
    )
}
