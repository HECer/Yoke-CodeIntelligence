use crate::{
    analysis::grammar,
    capture::{Deadline, source_scope_policy},
    model::{Graph, Snapshot},
    resolution::captured_module_paths,
    store::Store,
};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use tree_sitter::{Node, ParseOptions, Parser};

const MAX_SITES: usize = 50_000;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct ImportCycleOptions {
    pub max_length: usize,
    pub limit: usize,
    pub max_search_steps: usize,
}
impl Default for ImportCycleOptions {
    fn default() -> Self {
        Self {
            max_length: 5,
            limit: 20,
            max_search_steps: 250_000,
        }
    }
}
impl ImportCycleOptions {
    pub(super) fn validate(&self) -> Result<()> {
        ensure!(
            (1..=12).contains(&self.max_length)
                && (1..=100).contains(&self.limit)
                && (1..=2_000_000).contains(&self.max_search_steps),
            "Invalid import cycle bounds"
        );
        Ok(())
    }
}

fn esm(language: &str) -> bool {
    matches!(language, "typescript" | "tsx" | "javascript" | "jsx")
}

fn has_type_keyword(node: Node<'_>) -> bool {
    let mut cursor = node.walk();
    node.children(&mut cursor)
        .any(|child| child.kind() == "type" && !child.is_named())
}

fn type_only(node: Node<'_>) -> bool {
    if has_type_keyword(node) {
        return true;
    }
    let mut cursor = node.walk();
    let Some(bindings) = node
        .named_children(&mut cursor)
        .find(|child| matches!(child.kind(), "import_clause" | "export_clause"))
    else {
        return false;
    };
    let group = if bindings.kind() == "import_clause" {
        if bindings.named_child_count() != 1 {
            return false;
        }
        let Some(group) = bindings
            .named_child(0)
            .filter(|child| child.kind() == "named_imports")
        else {
            return false;
        };
        group
    } else {
        bindings
    };
    let mut cursor = group.walk();
    let mut names = group
        .named_children(&mut cursor)
        .filter(|child| matches!(child.kind(), "import_specifier" | "export_specifier"));
    names.next().is_some_and(has_type_keyword) && names.all(has_type_keyword)
}

fn modules<'a>(node: Node<'_>, text: &'a [u8], language: &str) -> Result<Vec<&'a str>> {
    if esm(language) {
        let Some(source) = node.child_by_field_name("source") else {
            return Ok(vec![]);
        };
        let literal = source.utf8_text(text)?;
        let quote = literal.chars().next().context("Missing module quote")?;
        return Ok(literal
            .strip_prefix(quote)
            .and_then(|value| value.strip_suffix(quote))
            .filter(|value| {
                matches!(quote, '\'' | '"')
                    && (value.starts_with("./") || value.starts_with("../"))
                    && !value.contains('\\')
            })
            .into_iter()
            .collect());
    }
    if node.kind() == "import_from_statement" {
        return Ok(node
            .child_by_field_name("module_name")
            .map(|name| name.utf8_text(text))
            .transpose()?
            .into_iter()
            .filter(|value| !value.trim_start_matches('.').is_empty())
            .collect());
    }
    let mut cursor = node.walk();
    node.children_by_field_name("name", &mut cursor)
        .map(|name| {
            Ok(name
                .child_by_field_name("name")
                .unwrap_or(name)
                .utf8_text(text)?)
        })
        .collect()
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Site {
    kind: &'static str,
    source: String,
    content_hash: String,
    start_byte: usize,
    end_byte: usize,
    start_line: usize,
    end_line: usize,
    observation_ids: Vec<String>,
    evidence_ids: Vec<String>,
}

type ModuleEdges<'a> = BTreeMap<(&'a str, &'a str), Vec<Site>>;
type ObservationKey<'a> = (&'a str, &'a str, usize, usize);
type ObservationLinks<'a> = BTreeMap<ObservationKey<'a>, Vec<(&'a str, &'a str)>>;

#[derive(Default, Serialize)]
#[serde(rename_all = "camelCase")]
struct Coverage {
    excluded_type_only_statements: usize,
    unresolved_or_ambiguous_modules: usize,
    skipped_parse_error_files: usize,
    original_resolved_observation_sites: usize,
    parsed_statement_sites: usize,
}

fn project<'a>(
    store: &Store,
    snapshot: &'a Snapshot,
    graph: &'a Graph,
    deadline: &Deadline,
) -> Result<(ModuleEdges<'a>, Coverage)> {
    let files: BTreeMap<_, _> = snapshot
        .files
        .iter()
        .map(|file| (file.path.as_str(), file))
        .collect();
    let selected: BTreeMap<_, _> = graph
        .entities
        .iter()
        .filter(|node| node.kind == "file")
        .filter_map(|node| {
            files
                .get_key_value(node.path.as_str())
                .map(|(&path, file)| (path, *file))
        })
        .collect();
    let endpoints: BTreeMap<_, _> = graph
        .entities
        .iter()
        .map(|entity| (entity.id.as_str(), entity.path.as_str()))
        .collect();
    let evidence: BTreeMap<_, _> = graph
        .evidence
        .iter()
        .map(|value| (value.id.as_str(), value))
        .collect();
    let mut bindings = ObservationLinks::new();
    let mut edges = ModuleEdges::new();
    let mut coverage = Coverage::default();
    for observation in &graph.observations {
        deadline.check()?;
        if !matches!(
            observation.relation.as_str(),
            "imports" | "imports_from" | "re_exports"
        ) {
            continue;
        }
        let Some(source) = endpoints.get(observation.source.as_str()).copied() else {
            continue;
        };
        let Some(file) = selected.get(source) else {
            continue;
        };
        let parsed = esm(&file.language) || file.language == "python";
        let Some(target) = observation
            .target
            .as_deref()
            .and_then(|id| endpoints.get(id))
            .copied()
            .filter(|path| selected.contains_key(path))
        else {
            if !parsed {
                coverage.unresolved_or_ambiguous_modules += 1;
            }
            continue;
        };
        if !matches!(
            observation.resolution.as_str(),
            "syntactic" | "exact_binding"
        ) {
            continue;
        }
        for id in &observation.evidence_ids {
            deadline.check()?;
            let Some(anchor) = evidence
                .get(id.as_str())
                .filter(|a| a.path == source && a.content_hash == file.hash)
            else {
                continue;
            };
            if parsed {
                bindings
                    .entry((source, target, anchor.start_byte, anchor.end_byte))
                    .or_default()
                    .push((&observation.id, id));
            } else {
                edges.entry((source, target)).or_default().push(Site {
                    kind: "resolved_import_observation",
                    source: source.to_owned(),
                    content_hash: anchor.content_hash.clone(),
                    start_byte: anchor.start_byte,
                    end_byte: anchor.end_byte,
                    start_line: anchor.start_line,
                    end_line: anchor.end_line,
                    observation_ids: vec![observation.id.clone()],
                    evidence_ids: vec![id.clone()],
                });
                coverage.original_resolved_observation_sites += 1;
                ensure!(
                    coverage.original_resolved_observation_sites <= MAX_SITES,
                    "Captured import site capacity exceeded"
                );
            }
        }
    }
    let scope = source_scope_policy(store, snapshot)?;
    let mut parser = Parser::new();
    for (&source, file) in &selected {
        deadline.check()?;
        let language = file.language.as_str();
        if !esm(language) && language != "python" {
            continue;
        }
        let contents = scope.source_view(source)?;
        let text = contents.text().as_bytes();
        parser.set_language(&grammar(language).context("Module grammar absent")?)?;
        let mut cancelled = |_: &tree_sitter::ParseState| deadline.check().is_err();
        let tree = parser
            .parse_with_options(
                &mut |offset, _| text.get(offset..).unwrap_or_default(),
                None,
                Some(ParseOptions::new().progress_callback(&mut cancelled)),
            )
            .context("Captured import parse stopped")?;
        let root = tree.root_node();
        if root.has_error() {
            coverage.skipped_parse_error_files += 1;
            continue;
        }
        let mut cursor = root.walk();
        for statement in root.named_children(&mut cursor) {
            deadline.check()?;
            let kind = match statement.kind() {
                "import_statement" | "import_from_statement" => "static_import",
                "export_statement" if esm(language) => "static_reexport",
                _ => continue,
            };
            if esm(language) && type_only(statement) {
                coverage.excluded_type_only_statements += 1;
                continue;
            }
            let imports = modules(statement, text, language)?;
            if imports.is_empty() {
                if kind == "static_import" || statement.child_by_field_name("source").is_some() {
                    coverage.unresolved_or_ambiguous_modules += 1;
                }
                continue;
            }
            for module in imports {
                let present = |path: &str| {
                    selected.get(path).is_some_and(|candidate| {
                        if esm(language) {
                            esm(&candidate.language)
                        } else {
                            candidate.language == language
                        }
                    })
                };
                let targets = captured_module_paths(source, module, language, present, present);
                let [target] = targets.as_slice() else {
                    coverage.unresolved_or_ambiguous_modules += 1;
                    continue;
                };
                let target = *selected.get_key_value(target.as_str()).unwrap().0;
                let start_byte = contents.to_raw(statement.start_byte())?;
                let end_byte = contents.to_raw(statement.end_byte())?;
                let originals = bindings
                    .get(&(source, target, start_byte, end_byte))
                    .into_iter()
                    .flatten();
                let mut observation_ids: Vec<_> =
                    originals.clone().map(|(id, _)| (*id).to_owned()).collect();
                let mut evidence_ids: Vec<_> = originals.map(|(_, id)| (*id).to_owned()).collect();
                observation_ids.sort();
                observation_ids.dedup();
                evidence_ids.sort();
                evidence_ids.dedup();
                edges.entry((source, target)).or_default().push(Site {
                    kind,
                    source: source.to_owned(),
                    content_hash: file.hash.clone(),
                    start_byte,
                    end_byte,
                    start_line: statement.start_position().row + 1,
                    end_line: statement.end_position().row + 1,
                    observation_ids,
                    evidence_ids,
                });
                coverage.parsed_statement_sites += 1;
                ensure!(
                    coverage.parsed_statement_sites + coverage.original_resolved_observation_sites
                        <= MAX_SITES,
                    "Captured import site capacity exceeded"
                );
            }
        }
    }
    Ok((edges, coverage))
}

type Adjacency<'a> = BTreeMap<&'a str, Vec<&'a str>>;

struct Search<'a> {
    limit: usize,
    remaining: usize,
    paths: Vec<Vec<&'a str>>,
    result_limit: bool,
    deadline: &'a Deadline,
}
impl<'a> Search<'a> {
    fn step(&mut self) -> Result<bool> {
        self.deadline.check()?;
        let Some(remaining) = self.remaining.checked_sub(1) else {
            return Ok(false);
        };
        self.remaining = remaining;
        Ok(true)
    }

    fn reverse_distances(
        &mut self,
        start: &'a str,
        length: usize,
        reverse: &Adjacency<'a>,
    ) -> Result<Option<BTreeMap<&'a str, usize>>> {
        let mut distance = BTreeMap::from([(start, 0)]);
        let mut pending = VecDeque::from([(start, 0)]);
        while let Some((target, hops)) = pending.pop_front() {
            if hops + 1 >= length {
                continue;
            }
            for &source in reverse.get(target).into_iter().flatten() {
                if !self.step()? {
                    return Ok(None);
                }
                if source >= start
                    && let std::collections::btree_map::Entry::Vacant(entry) =
                        distance.entry(source)
                {
                    entry.insert(hops + 1);
                    pending.push_back((source, hops + 1));
                }
            }
        }
        Ok(Some(distance))
    }

    fn visit(
        &mut self,
        remaining: usize,
        path: &mut Vec<&'a str>,
        next: &Adjacency<'a>,
        distance: &BTreeMap<&'a str, usize>,
    ) -> Result<bool> {
        let first = path[0];
        for &to in next.get(path.last().unwrap()).into_iter().flatten() {
            if !self.step()? {
                return Ok(false);
            }
            if to == first {
                if remaining == 1 {
                    if self.paths.len() == self.limit {
                        self.result_limit = true;
                        return Ok(false);
                    }
                    self.paths.push(path.clone());
                }
                continue;
            }
            if remaining > 1
                && distance.get(to).is_some_and(|&hops| hops < remaining)
                && !path.contains(&to)
            {
                path.push(to);
                let complete = self.visit(remaining - 1, path, next, distance)?;
                path.pop();
                if !complete {
                    return Ok(false);
                }
            }
        }
        Ok(true)
    }
}

fn enumerate<'a>(
    edges: &ModuleEdges<'a>,
    options: &ImportCycleOptions,
    deadline: &'a Deadline,
) -> Result<(Search<'a>, bool)> {
    let mut next = Adjacency::new();
    let mut reverse = Adjacency::new();
    for &(source, target) in edges.keys() {
        deadline.check()?;
        next.entry(source).or_default().push(target);
        reverse.entry(target).or_default().push(source);
    }
    let mut search = Search {
        limit: options.limit,
        remaining: options.max_search_steps,
        paths: Vec::new(),
        result_limit: false,
        deadline,
    };
    for length in 1..=options.max_length {
        for &start in next.keys() {
            if !search.step()? {
                return Ok((search, true));
            }
            let Some(distance) = search.reverse_distances(start, length, &reverse)? else {
                return Ok((search, true));
            };
            if !search.visit(length, &mut vec![start], &next, &distance)? {
                let truncated = !search.result_limit;
                return Ok((search, truncated));
            }
        }
    }
    Ok((search, false))
}

pub(super) fn report(
    store: &Store,
    snapshot: &Snapshot,
    graph: &Graph,
    options: &ImportCycleOptions,
    deadline: &Deadline,
) -> Result<Value> {
    let (edges, coverage) = project(store, snapshot, graph, deadline)?;
    let (search, budget_reached) = enumerate(&edges, options, deadline)?;
    let mut cycles = Vec::new();
    let mut used = BTreeSet::new();
    for path in &search.paths {
        let mut steps = Vec::new();
        for index in 0..path.len() {
            deadline.check()?;
            let from = path[index];
            let to = path[(index + 1) % path.len()];
            used.insert((from, to));
            steps.push(json!({"from":from,"to":to}));
        }
        cycles.push(json!({"paths":path,"length":path.len(),"steps":steps}));
    }
    let mut links = Vec::new();
    for (from, to) in used {
        deadline.check()?;
        links.push(json!({"from":from,"to":to,"sites":edges[&(from,to)]}));
    }
    deadline.check()?;
    Ok(json!({
        "scope":"captured_file_import_candidates",
        "options":options,
        "cycles":cycles,
        "links":links,
        "coverage":coverage,
        "fileLinks":edges.len(),
        "searchSteps":options.max_search_steps-search.remaining,
        "searchBudgetReached":budget_reached,
        "resultLimitReached":search.result_limit,
        "truncated":budget_reached||search.result_limit,
        "complete":false,
    }))
}
