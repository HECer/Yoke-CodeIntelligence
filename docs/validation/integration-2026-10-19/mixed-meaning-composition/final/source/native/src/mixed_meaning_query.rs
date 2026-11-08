//! Literal queries over one source-replayed mixed interpretation graph.
//! Original array positions identify parallel observations; roles imply no flow.
use super::*;

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ConceptRef {
    pub graph_id: String,
    pub key: String,
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Direction {
    Outgoing,
    Incoming,
    Both,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case", deny_unknown_fields)]
pub enum Query {
    Node {
        reference: ConceptRef,
    },
    /// Case-sensitive literal substring of labels/aliases; no semantic ranking.
    Find {
        query: String,
    },
    Degree {},
    Trace {
        roots: Vec<ConceptRef>,
        direction: Direction,
        depth: usize,
        #[serde(rename = "maxVisited")]
        max_visited: usize,
    },
}
fn invalid() -> anyhow::Error {
    failure("mixed_meaning_query_invalid")
}
pub(super) fn reference_valid(r: &ConceptRef) -> bool {
    valid_id(&r.graph_id)
        && !r.key.is_empty()
        && r.key.len() <= 80
        && r.key
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'-' | b'_'))
}
/// Shape admission performs no Store access, source replay or worker execution.
pub fn validate(q: &Query, d: &Deadline) -> Result<()> {
    check(d)?;
    let valid = match q {
        Query::Node { reference } => reference_valid(reference),
        Query::Find { query } => {
            !query.is_empty() && query.len() <= 1024 && !query.chars().any(char::is_control)
        }
        Query::Degree {} => true,
        Query::Trace {
            roots,
            depth,
            max_visited,
            ..
        } => {
            !roots.is_empty()
                && roots.len() <= 32
                && *depth <= 8
                && (1..=128).contains(max_visited)
                && roots.iter().all(reference_valid)
                && roots.iter().collect::<BTreeSet<_>>().len() == roots.len()
        }
    };
    if !valid {
        return Err(invalid());
    }
    encode(q, 64_000, d)?;
    check(d)
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Node<'a> {
    index: usize,
    reference: ConceptRef,
    claim: &'a crate::meaning_graph::ConceptClaim,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Relation<'a> {
    index: usize,
    source: ConceptRef,
    target: ConceptRef,
    claim: &'a crate::meaning_graph::RelationClaim,
}
#[derive(Serialize)]
struct Hyperedge<'a> {
    index: usize,
    claim: &'a crate::meaning_graph::HyperedgeClaim,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Degree {
    reference: ConceptRef,
    in_degree: usize,
    out_degree: usize,
    hyperedge_slots: usize,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Answer<'a> {
    kind: &'static str,
    graph_id: &'a str,
    query: &'a Query,
    original_graph: &'a Value,
    nodes: Vec<Node<'a>>,
    relations: Vec<Relation<'a>>,
    hyperedges: Vec<Hyperedge<'a>>,
    degrees: Vec<Degree>,
    traversal: &'static str,
    relation_selection: &'static str,
    hyperedge_selection: &'static str,
    selection_complete: bool,
    complete: bool,
    canonical_source_authority: bool,
    provider_calls_this_read: u8,
    source_written: bool,
}
fn resolve<'a>(g: &'a CheckedGraph, r: &ConceptRef) -> Result<&'a str> {
    if r.graph_id != g.id() {
        return Err(invalid());
    }
    g.fragment
        .concepts
        .iter()
        .find(|c| c.key == r.key)
        .map(|c| c.key.as_str())
        .ok_or_else(invalid)
}
pub(super) fn run(g: &CheckedGraph, q: &Query, limit: usize, d: &Deadline) -> Result<Value> {
    validate(q, d)?;
    input(limit <= MAX_GRAPH)?;
    capacity(limit > 0)?;
    let reference = |key: &str| ConceptRef {
        graph_id: g.id().into(),
        key: key.into(),
    };
    let mut selected = BTreeSet::new();
    let mut traversed = BTreeSet::new();
    match q {
        Query::Node { reference } => {
            selected.insert(resolve(g, reference)?);
        }
        Query::Find { query } => {
            for c in &g.fragment.concepts {
                check(d)?;
                if c.label.contains(query) || c.aliases.iter().any(|a| a.contains(query)) {
                    selected.insert(c.key.as_str());
                }
            }
        }
        Query::Degree {} => selected.extend(g.fragment.concepts.iter().map(|c| c.key.as_str())),
        Query::Trace {
            roots,
            direction,
            depth,
            max_visited,
        } => {
            for root in roots {
                selected.insert(resolve(g, root)?);
            }
            capacity(selected.len() <= *max_visited)?;
            let mut frontier = selected.clone();
            for _ in 0..*depth {
                check(d)?;
                let mut next = BTreeSet::new();
                for (i, r) in g.fragment.relations.iter().enumerate() {
                    check(d)?;
                    let target = if matches!(direction, Direction::Outgoing | Direction::Both)
                        && frontier.contains(r.source.as_str())
                    {
                        Some(r.target.as_str())
                    } else if matches!(direction, Direction::Incoming | Direction::Both)
                        && frontier.contains(r.target.as_str())
                    {
                        Some(r.source.as_str())
                    } else {
                        None
                    };
                    if let Some(target) = target {
                        traversed.insert(i);
                        if selected.insert(target) {
                            next.insert(target);
                        }
                        capacity(selected.len() <= *max_visited)?;
                    }
                }
                frontier = next;
                if frontier.is_empty() {
                    break;
                }
            }
        }
    }
    let mut nodes = Vec::new();
    let mut degrees = Vec::new();
    for (index, c) in g.fragment.concepts.iter().enumerate() {
        check(d)?;
        if !selected.contains(c.key.as_str()) {
            continue;
        }
        nodes.push(Node {
            index,
            reference: reference(&c.key),
            claim: c,
        });
        if matches!(q, Query::Degree {}) {
            degrees.push(Degree {
                reference: reference(&c.key),
                in_degree: g
                    .fragment
                    .relations
                    .iter()
                    .filter(|r| r.target == c.key)
                    .count(),
                out_degree: g
                    .fragment
                    .relations
                    .iter()
                    .filter(|r| r.source == c.key)
                    .count(),
                hyperedge_slots: g
                    .fragment
                    .hyperedges
                    .iter()
                    .flat_map(|h| &h.participants)
                    .filter(|p| p.concept == c.key)
                    .count(),
            });
        }
    }
    let mut relations = Vec::new();
    for (index, r) in g.fragment.relations.iter().enumerate() {
        check(d)?;
        let include = if matches!(q, Query::Trace { .. }) {
            traversed.contains(&index)
        } else {
            selected.contains(r.source.as_str()) || selected.contains(r.target.as_str())
        };
        if include {
            relations.push(Relation {
                index,
                source: reference(&r.source),
                target: reference(&r.target),
                claim: r,
            });
        }
    }
    let mut hyperedges = Vec::new();
    for (index, h) in g.fragment.hyperedges.iter().enumerate() {
        check(d)?;
        if h.participants
            .iter()
            .any(|p| selected.contains(p.concept.as_str()))
        {
            hyperedges.push(Hyperedge { index, claim: h });
        }
    }
    output(
        &Answer {
            kind: "mixed_meaning_query_v1",
            graph_id: g.id(),
            query: q,
            original_graph: &g.value,
            nodes,
            relations,
            hyperedges,
            degrees,
            traversal: "directed_relation_observations_only_no_hyperedge_flow",
            relation_selection: if matches!(q, Query::Trace { .. }) {
                "traversed_original_indices"
            } else {
                "incident_to_selected_concepts"
            },
            hyperedge_selection: "incident_original_ordered_roles_not_traversed",
            selection_complete: true,
            complete: false,
            canonical_source_authority: false,
            provider_calls_this_read: 0,
            source_written: false,
        },
        limit,
        d,
    )
}
