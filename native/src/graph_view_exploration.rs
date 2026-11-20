use super::{MixedArc, MixedArcKind, View, load, meaning_items, view_arcs};
use crate::{capture::Deadline, model::valid_id, store::Store};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    cmp::Reverse,
    collections::{BTreeMap, BTreeSet, VecDeque},
};
use unicode_normalization::{UnicodeNormalization, char::is_combining_mark};

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
enum Mode {
    #[default]
    Bfs,
    Dfs,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
enum Direction {
    In,
    Out,
    #[default]
    Both,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Options {
    query: Option<String>,
    #[serde(default)]
    seeds: Vec<String>,
    #[serde(default)]
    mode: Mode,
    #[serde(default)]
    direction: Direction,
    depth: Option<usize>,
    max_seeds: Option<usize>,
    max_visited: Option<usize>,
    contexts: Option<BTreeSet<String>>,
    #[serde(default)]
    relations: BTreeSet<String>,
    #[serde(default)]
    resolutions: BTreeSet<String>,
}

fn words(label: &str) -> Vec<String> {
    let mut normalized = String::new();
    let mut follows_lower = false;
    for c in label.nfkd().filter(|c| !is_combining_mark(*c)) {
        if c.is_alphanumeric() {
            if follows_lower && c.is_uppercase() {
                normalized.push(' ');
            }
            normalized.extend(c.to_lowercase());
        } else {
            normalized.push(' ');
        }
        follows_lower = c.is_lowercase() || c.is_ascii_digit();
    }
    normalized.split_whitespace().map(str::to_owned).collect()
}

fn edge_context(relation: &str) -> &str {
    match relation {
        "calls" => "call",
        "imports" => "import",
        "references" => "reference",
        "contains" => "containment",
        "inherits" => "inheritance",
        "exports" => "export",
        other => other,
    }
}

fn query_intent(term: &str) -> Option<&'static str> {
    match term {
        "call" | "calls" | "called" | "invokes" => Some("call"),
        "import" | "imports" | "imported" => Some("import"),
        "reference" | "references" | "referenced" => Some("reference"),
        "contain" | "contains" => Some("containment"),
        "inherits" | "extends" => Some("inheritance"),
        "export" | "exports" | "exported" => Some("export"),
        _ => None,
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Seed<'a> {
    id: &'a str,
    score: Option<usize>,
    matched_terms: Vec<&'a str>,
    selector: &'static str,
}

fn ranked<'a>(
    nodes: &BTreeMap<&'a str, &'a Value>,
    terms: &'a [String],
    deadline: &Deadline,
) -> Result<Vec<Seed<'a>>> {
    let mut seeds = Vec::new();
    for (&id, node) in nodes {
        deadline.check()?;
        let name = words(node["name"].as_str().unwrap_or(""));
        let qualified = words(node["qualifiedName"].as_str().unwrap_or(""));
        let path = words(node["path"].as_str().unwrap_or(""));
        let aliases: Vec<_> = node["aliases"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .flat_map(words)
            .collect();
        let bonus = if name == terms {
            24
        } else if qualified == terms {
            18
        } else {
            0
        };
        let mut score = bonus;
        let mut matched_terms = Vec::new();
        for term in terms {
            let weight = if name.contains(term) {
                6
            } else if name.iter().any(|part| part.contains(term)) {
                3
            } else if qualified.contains(term) || aliases.contains(term) {
                2
            } else {
                usize::from(path.contains(term))
            };
            if weight > 0 {
                score += weight;
                matched_terms.push(term.as_str());
            }
        }
        if score > 0 {
            seeds.push(Seed {
                id,
                score: Some(score),
                matched_terms,
                selector: "ranked_text",
            });
        }
    }
    seeds.sort_unstable_by_key(|seed| (Reverse(seed.score), seed.id));
    Ok(seeds)
}

fn explicit<'a>(
    nodes: &BTreeMap<&'a str, &'a Value>,
    selectors: &[String],
    deadline: &Deadline,
) -> Result<(Vec<Seed<'a>>, Vec<Value>)> {
    let mut selected = BTreeSet::new();
    let mut seeds = Vec::new();
    let mut unresolved = Vec::new();
    for selector in selectors {
        deadline.check()?;
        let mut matches = Vec::new();
        if let Some((&id, _)) = nodes.get_key_value(selector.as_str()) {
            matches.push(id);
        } else {
            for (&id, node) in nodes {
                deadline.check()?;
                if node["name"] == *selector
                    || node["qualifiedName"] == *selector
                    || (node["kind"] == "file" && node["path"] == *selector)
                {
                    matches.push(id);
                }
            }
        }
        match matches.as_slice() {
            [id] if selected.insert(*id) => seeds.push(Seed {
                id,
                score: None,
                matched_terms: vec![],
                selector: "exact",
            }),
            [_] => {}
            _ => unresolved.push(json!({"seed":selector,"candidateCount":matches.len()})),
        }
    }
    Ok((seeds, unresolved))
}

#[derive(Clone, Copy)]
struct Step<'a> {
    id: &'a str,
    depth: usize,
    via: Option<(&'a str, usize)>,
}

struct Traversal<'a> {
    order: Vec<&'a str>,
    best: BTreeMap<&'a str, Step<'a>>,
    truncated: bool,
}

fn traverse<'a>(
    initial: VecDeque<Step<'a>>,
    arcs: &[MixedArc<'a>],
    allowed: &[bool],
    options: &Options,
    depth: usize,
    maximum: usize,
    deadline: &Deadline,
) -> Result<Traversal<'a>> {
    let mut adjacent: BTreeMap<&str, Vec<usize>> = BTreeMap::new();
    for (index, arc) in arcs.iter().enumerate() {
        deadline.check()?;
        if allowed[index] {
            if options.direction != Direction::In {
                adjacent.entry(arc.source).or_default().push(index);
            }
            if options.direction != Direction::Out
                && let Some(target) = arc.target
                && (target != arc.source || options.direction == Direction::In)
            {
                adjacent.entry(target).or_default().push(index);
            }
        }
    }
    let mut order: Vec<_> = initial.iter().map(|step| step.id).collect();
    let mut best: BTreeMap<_, _> = initial.iter().map(|step| (step.id, *step)).collect();
    let mut truncated = false;
    let mut pending = initial;
    while let Some(step) = pending.pop_front() {
        deadline.check()?;
        if let Some(previous) = best.get(&step.id) {
            if step.via.is_some() && previous.depth <= step.depth {
                continue;
            }
        } else if best.len() == maximum {
            truncated = true;
            continue;
        } else {
            order.push(step.id);
        }
        best.insert(step.id, step);
        if step.depth == depth {
            continue;
        }
        let neighbors = adjacent.get(step.id).into_iter().flatten();
        let next = neighbors.filter_map(|&index| {
            let arc = &arcs[index];
            let target = if arc.source == step.id {
                arc.target?
            } else {
                arc.source
            };
            Some(Step {
                id: target,
                depth: step.depth + 1,
                via: Some((step.id, index)),
            })
        });
        if options.mode == Mode::Dfs {
            for child in next.rev() {
                deadline.check()?;
                pending.push_front(child);
            }
        } else {
            for child in next {
                deadline.check()?;
                pending.push_back(child);
            }
        }
    }
    Ok(Traversal {
        order,
        best,
        truncated,
    })
}

fn arc_id<'a>(view: &'a View, arc: &MixedArc<'a>) -> Result<&'a str> {
    match arc.kind {
        MixedArcKind::Source(index) => view.observations[index]["id"]
            .as_str()
            .context("Selected observation identity missing"),
        MixedArcKind::Relation(id) | MixedArcKind::Hyperedge(id) | MixedArcKind::Citation(id) => {
            Ok(id)
        }
    }
}

fn dependency<'a>(view: &'a View, arc: &MixedArc<'a>) -> Result<Value> {
    let kind = match arc.kind {
        MixedArcKind::Source(_) => "observation",
        MixedArcKind::Relation(_) => "meaning_relation",
        MixedArcKind::Hyperedge(_) => "meaning_incidence",
        MixedArcKind::Citation(_) => "meaning_citation",
    };
    Ok(json!({
        "id":arc_id(view, arc)?,
        "kind":kind,
        "source":arc.source,
        "target":arc.target,
        "relation":arc.relation,
        "resolution":arc.resolution,
        "context":edge_context(arc.relation),
        "neutral":arc.neutral
    }))
}

/// Read a published graph view and explain selection and traversal against its exact observations.
pub fn explore(
    store: &Store,
    view_id: &str,
    expected_snapshot: Option<&str>,
    options: Options,
    explain: bool,
    deadline: &Deadline,
) -> Result<Value> {
    let depth = options.depth.unwrap_or(if explain { 1 } else { 3 });
    let max_seeds = options.max_seeds.unwrap_or(if explain { 1 } else { 3 });
    let max_visited = options.max_visited.unwrap_or(300);
    ensure!(
        valid_id(view_id)
            && depth <= 8
            && (1..=20).contains(&max_seeds)
            && (1..=2000).contains(&max_visited)
            && (options.query.is_some() == options.seeds.is_empty())
            && options.seeds.len() <= 20
            && options
                .contexts
                .iter()
                .chain([&options.relations, &options.resolutions])
                .all(|set| set.len() <= 100 && set.iter().all(|s| !s.is_empty() && s.len() <= 100))
            && options
                .seeds
                .iter()
                .all(|s| !s.is_empty() && s.len() <= 1000),
        "Invalid graph exploration options"
    );
    let mut terms = Vec::new();
    if let Some(query) = &options.query {
        ensure!(query.len() <= 1000, "Graph query exceeds its byte budget");
        let mut seen = BTreeSet::new();
        terms = words(query)
            .into_iter()
            .filter(|term| seen.insert(term.clone()))
            .collect();
        ensure!(
            (1..=32).contains(&terms.len()),
            "Graph query needs 1 to 32 distinct terms"
        );
    }
    let (contexts, context_source): (BTreeSet<&str>, _) = match &options.contexts {
        Some(explicit) => (
            explicit
                .iter()
                .map(|context| edge_context(context))
                .collect(),
            "explicit",
        ),
        None => {
            let inferred: BTreeSet<_> =
                terms.iter().filter_map(|term| query_intent(term)).collect();
            let source = if inferred.is_empty() {
                "none"
            } else {
                "query_intent"
            };
            (inferred, source)
        }
    };
    let content_terms = terms
        .iter()
        .filter(|term| {
            query_intent(term).is_none()
                && !matches!(
                    term.as_str(),
                    "a" | "an"
                        | "the"
                        | "how"
                        | "why"
                        | "what"
                        | "which"
                        | "who"
                        | "does"
                        | "do"
                        | "is"
                        | "are"
                        | "to"
                        | "of"
                        | "and"
                        | "in"
                )
        })
        .cloned()
        .collect::<Vec<_>>();
    let terms = if content_terms.is_empty() {
        terms
    } else {
        content_terms
    };
    store.set_deadline(deadline)?;
    let view = load(store, view_id, deadline)?;
    ensure!(
        expected_snapshot.is_none_or(|id| id == view.snapshot_id),
        "Graph exploration snapshot does not match selected view"
    );
    let meaning = meaning_items(&view, deadline)?;
    let mut nodes = BTreeMap::new();
    for row in view.nodes.iter().chain(&meaning) {
        deadline.check()?;
        nodes.insert(
            row["id"].as_str().context("View entity identity missing")?,
            row,
        );
    }
    let (mut seeds, unresolved) = if options.query.is_some() {
        (ranked(&nodes, &terms, deadline)?, vec![])
    } else {
        explicit(&nodes, &options.seeds, deadline)?
    };
    let ranked_candidates = options.query.as_ref().map(|_| seeds.len());
    if ranked_candidates.is_some() {
        seeds.truncate(max_seeds.min(max_visited));
    }
    ensure!(
        seeds.len() <= max_visited,
        "Explicit seeds exceed maxVisited"
    );
    let initial = seeds
        .iter()
        .map(|seed| Step {
            id: seed.id,
            depth: 0,
            via: None,
        })
        .collect();
    let arcs = view_arcs(&view, deadline)?;
    let mut allowed = Vec::with_capacity(arcs.len());
    for arc in &arcs {
        deadline.check()?;
        allowed.push(
            (!arc.neutral || options.direction == Direction::Both)
                && (options.relations.is_empty() || options.relations.contains(arc.relation))
                && (options.resolutions.is_empty() || options.resolutions.contains(arc.resolution))
                && (contexts.is_empty() || contexts.contains(edge_context(arc.relation))),
        );
    }
    let Traversal {
        order,
        best,
        truncated,
    } = traverse(
        initial,
        &arcs,
        &allowed,
        &options,
        depth,
        max_visited,
        deadline,
    )?;
    let mut observations = Vec::new();
    let mut evidence_ids = BTreeSet::new();
    let mut typed = Vec::new();
    let mut claims = BTreeSet::new();
    for (arc, permitted) in arcs.iter().zip(allowed) {
        deadline.check()?;
        if !permitted
            || !best.contains_key(arc.source)
            || arc.target.is_some_and(|id| !best.contains_key(id))
        {
            continue;
        }
        match arc.kind {
            MixedArcKind::Source(index) => {
                let original = &view.observations[index];
                evidence_ids.extend(
                    original["evidenceIds"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .filter_map(Value::as_str),
                );
                observations.push(original);
            }
            MixedArcKind::Relation(id) | MixedArcKind::Hyperedge(id) => {
                typed.push(dependency(&view, arc)?);
                claims.insert(id);
            }
            MixedArcKind::Citation(_) => typed.push(dependency(&view, arc)?),
        }
    }
    let mut entities = Vec::new();
    let mut visits = Vec::new();
    for id in order {
        deadline.check()?;
        entities.push(nodes[id]);
        let step = best[&id];
        let from = step
            .via
            .map(|(from, index)| -> Result<Value> {
                Ok(json!({"from":from,"observation":dependency(&view,&arcs[index])?}))
            })
            .transpose()?;
        visits.push(json!({"id":id,"depth":step.depth,"via":from}));
    }
    let evidence: Vec<_> = view
        .evidence
        .iter()
        .filter(|row| {
            row["id"]
                .as_str()
                .is_some_and(|id| evidence_ids.contains(id))
        })
        .collect();
    let claims: Vec<_> = meaning
        .iter()
        .filter(|row| row["id"].as_str().is_some_and(|id| claims.contains(id)))
        .collect();
    deadline.check()?;
    Ok(json!({
        "graphViewId":view.id,
        "snapshotId":view.snapshot_id,
        "mode":options.mode,
        "direction":options.direction,
        "depth":depth,
        "seedCandidates":ranked_candidates,
        "seeds":seeds,
        "unresolvedSeeds":unresolved,
        "filters":{"contexts":contexts,"contextSource":context_source,"relations":options.relations,"resolutions":options.resolutions},
        "visits":visits,
        "entities":entities,
        "observations":observations,
        "evidence":evidence,
        "meaningObservations":typed,
        "meaningClaims":claims,
        "truncated":truncated,
        "complete":false
    }))
}
