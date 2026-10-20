//! Queries only accept the opaque projection produced by joint parent replay.
use super::*;

#[cfg(test)]
#[path = "meaning_join_query_tests.rs"]
mod tests;

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
    Degree {},
    Trace {
        roots: Vec<ConceptRef>,
        direction: Direction,
        depth: usize,
        #[serde(rename = "maxVisited")]
        max_visited: usize,
    },
}
pub fn validate_query(q: &Query, graph_ids: &[String], d: &Deadline) -> Result<()> {
    check(d)?;
    match q {
        Query::Node { reference } => {
            input(reference_valid(reference) && graph_ids.contains(&reference.graph_id))?
        }
        Query::Degree {} => (),
        Query::Trace {
            roots,
            depth,
            max_visited,
            ..
        } => {
            input(!roots.is_empty() && roots.len() <= 32 && *depth <= 8 && *max_visited <= 4096)?;
            for root in roots {
                check(d)?;
                input(reference_valid(root) && graph_ids.contains(&root.graph_id))?;
            }
            let unique: BTreeSet<_> = roots.iter().collect();
            input(unique.len() == roots.len())?;
        }
    }
    check(d)
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Degree<'a> {
    node_id: &'a str,
    in_degree: usize,
    out_degree: usize,
    hyperedge_slots: usize,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Answer<'a> {
    #[serde(flatten)]
    metadata: BTreeMap<&'a str, &'a Value>,
    kind: &'static str,
    query: &'a Query,
    traversal: &'static str,
    selection: &'static str,
    nodes: Vec<&'a Value>,
    relations: Vec<&'a Value>,
    hyperedges: Vec<&'a Value>,
    rows: Vec<Degree<'a>>,
}
fn resolve<'a>(nodes: &'a [Value], reference: &ConceptRef, d: &Deadline) -> Result<&'a str> {
    for node in nodes {
        check(d)?;
        for member in rows(node, "members")? {
            check(d)?;
            let r = &member["reference"];
            if r["graphId"].as_str() == Some(&reference.graph_id)
                && r["packetIndex"].as_u64() == Some(reference.packet_index as u64)
                && r["conceptId"].as_str() == Some(&reference.concept_id)
            {
                return text(node, "id");
            }
        }
    }
    Err(failure("meaning_join_input_invalid"))
}
pub(super) fn run(graph: &JoinedGraph, q: &Query, limit: usize, d: &Deadline) -> Result<Value> {
    run_projection(
        graph.as_value(),
        &graph.graph_ids,
        q,
        limit,
        MAX_OUTPUT,
        "meaning_join_query_v1",
        d,
    )
}
pub(super) fn run_projection(
    value: &Value,
    graph_ids: &[String],
    q: &Query,
    limit: usize,
    maximum: usize,
    kind: &'static str,
    d: &Deadline,
) -> Result<Value> {
    validate_query(q, graph_ids, d)?;
    input(limit <= maximum)?;
    capacity(limit > 0)?;
    let nodes = rows(value, "nodes")?;
    let relations = rows(value, "relations")?;
    let hyperedges = rows(value, "hyperedges")?;
    let mut selected = BTreeSet::new();
    let mut degrees = Vec::new();
    match q {
        Query::Node { reference } => {
            selected.insert(resolve(nodes, reference, d)?);
        }
        Query::Trace {
            roots,
            direction,
            depth,
            max_visited,
        } => {
            for root in roots {
                check(d)?;
                selected.insert(resolve(nodes, root, d)?);
                capacity(selected.len() <= *max_visited)?;
            }
            let mut frontier = selected.clone();
            for _ in 0..*depth {
                check(d)?;
                if frontier.is_empty() {
                    break;
                }
                let mut next = BTreeSet::new();
                for relation in relations {
                    check(d)?;
                    let source = text(relation, "source")?;
                    let target = text(relation, "target")?;
                    for candidate in [
                        (
                            matches!(direction, Direction::Outgoing | Direction::Both)
                                && frontier.contains(source),
                            target,
                        ),
                        (
                            matches!(direction, Direction::Incoming | Direction::Both)
                                && frontier.contains(target),
                            source,
                        ),
                    ] {
                        if candidate.0
                            && !selected.contains(candidate.1)
                            && next.insert(candidate.1)
                        {
                            capacity(selected.len() + next.len() <= *max_visited)?;
                        }
                    }
                }
                selected.extend(next.iter().copied());
                frontier = next;
            }
        }
        Query::Degree {} => {
            let mut counts = BTreeMap::new();
            for node in nodes {
                check(d)?;
                counts.insert(text(node, "id")?, (0, 0, 0));
            }
            for relation in relations {
                check(d)?;
                counts
                    .get_mut(text(relation, "source")?)
                    .ok_or_else(|| failure("meaning_join_parent_refused"))?
                    .1 += 1;
                counts
                    .get_mut(text(relation, "target")?)
                    .ok_or_else(|| failure("meaning_join_parent_refused"))?
                    .0 += 1;
            }
            for edge in hyperedges {
                for slot in rows(edge, "participants")? {
                    check(d)?;
                    counts
                        .get_mut(text(slot, "nodeId")?)
                        .ok_or_else(|| failure("meaning_join_parent_refused"))?
                        .2 += 1;
                }
            }
            degrees = counts
                .into_iter()
                .map(
                    |(node_id, (in_degree, out_degree, hyperedge_slots))| Degree {
                        node_id,
                        in_degree,
                        out_degree,
                        hyperedge_slots,
                    },
                )
                .collect();
        }
    }
    let mut answer = Answer {
        metadata: value
            .as_object()
            .ok_or_else(|| failure("meaning_join_parent_refused"))?
            .iter()
            .filter(|(k, _)| !matches!(k.as_str(), "kind" | "nodes" | "relations" | "hyperedges"))
            .map(|(k, v)| (k.as_str(), v))
            .collect(),
        kind,
        query: q,
        traversal: "directed_relations_only",
        selection: "induced_relations_and_fully_contained_hyperedges",
        nodes: Vec::new(),
        relations: Vec::new(),
        hyperedges: Vec::new(),
        rows: degrees,
    };
    for node in nodes {
        check(d)?;
        if selected.contains(text(node, "id")?) {
            answer.nodes.push(node);
        }
    }
    for relation in relations {
        check(d)?;
        if selected.contains(text(relation, "source")?)
            && selected.contains(text(relation, "target")?)
        {
            answer.relations.push(relation);
        }
    }
    for edge in hyperedges {
        check(d)?;
        let mut included = true;
        for slot in rows(edge, "participants")? {
            check(d)?;
            included &= selected.contains(text(slot, "nodeId")?);
        }
        if included {
            answer.hyperedges.push(edge);
        }
    }
    let bytes = encode(&answer, limit, d)?;
    let result = crate::json::parse(&bytes, limit)?;
    check(d)?;
    Ok(result)
}
