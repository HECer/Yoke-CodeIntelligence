//! Composition only consumes checked capabilities. Host links remain assertions.
use super::*;
use query::{ConceptRef, Direction, Query};

const MAX_REVIEW: usize = 128_000;
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Link {
    pub source: ConceptRef,
    pub target: ConceptRef,
    pub relation: String,
    pub rationale: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Review {
    pub schema_version: u8,
    pub graph_ids: Vec<String>,
    pub links: Vec<Link>,
}
fn invalid() -> anyhow::Error {
    failure("mixed_meaning_composition_invalid")
}
pub fn validate_review(r: &Review, d: &Deadline) -> Result<()> {
    check(d)?;
    if r.schema_version != 1
        || !(2..=4).contains(&r.graph_ids.len())
        || r.links.len() > 256
        || !r.graph_ids.iter().all(|id| valid_id(id))
        || r.graph_ids.iter().collect::<BTreeSet<_>>().len() != r.graph_ids.len()
    {
        return Err(invalid());
    }
    for link in &r.links {
        check(d)?;
        if !query::reference_valid(&link.source)
            || !query::reference_valid(&link.target)
            || !r.graph_ids.contains(&link.source.graph_id)
            || !r.graph_ids.contains(&link.target.graph_id)
            || link.relation.is_empty()
            || link.relation.len() > 80
            || !link
                .relation
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'-' | b'_'))
            || link.rationale.trim().is_empty()
            || link.rationale.len() > 1024
            || link.rationale.chars().any(char::is_control)
            || crate::paths::obvious_secret(&link.rationale)
        {
            return Err(invalid());
        }
    }
    encode(r, MAX_REVIEW, d)?;
    check(d)
}
/// Immutable explicit parents may come from different workspaces/history.
/// Each parent has already established its own independently checked scope.
pub struct Composition {
    graphs: Vec<CheckedGraph>,
    review: Review,
    id: String,
}
pub fn compose(
    graphs: Vec<CheckedGraph>,
    r: &Review,
    max_input_bytes: usize,
    d: &Deadline,
) -> Result<Composition> {
    let result = (|| {
        validate_review(r, d)?;
        input((1..=MAX_GRAPH).contains(&max_input_bytes))?;
        if graphs.len() != r.graph_ids.len()
            || !graphs.iter().zip(&r.graph_ids).all(|(g, id)| g.id() == id)
        {
            return Err(invalid());
        }
        let original: Vec<_> = graphs.iter().map(|g| &g.value).collect();
        encode(&original, max_input_bytes, d)?;
        let result = Composition {
            graphs,
            review: r.clone(),
            id: hash(encode(
                &("mixed_meaning_composition_v1", r),
                MAX_REVIEW + 128,
                d,
            )?),
        };
        for link in &r.links {
            check(d)?;
            result.resolve(&link.source)?;
            result.resolve(&link.target)?;
        }
        check(d)?;
        Ok(result)
    })();
    result.map_err(|e| sanitized(e, d))
}
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "camelCase")]
struct ObservationRef {
    graph_id: String,
    index: usize,
}
#[derive(Serialize)]
struct Node<'a> {
    index: usize,
    reference: ConceptRef,
    claim: &'a crate::meaning_graph::ConceptClaim,
}
#[derive(Serialize)]
struct Relation<'a> {
    reference: ObservationRef,
    source: ConceptRef,
    target: ConceptRef,
    claim: &'a crate::meaning_graph::RelationClaim,
}
#[derive(Serialize)]
struct Hyperedge<'a> {
    reference: ObservationRef,
    claim: &'a crate::meaning_graph::HyperedgeClaim,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Degree {
    reference: ConceptRef,
    original_relation_in_degree: usize,
    original_relation_out_degree: usize,
    host_link_in_degree: usize,
    host_link_out_degree: usize,
    hyperedge_slots: usize,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct HostLink<'a> {
    index: usize,
    claim: &'a Link,
    origin: &'static str,
    attestation: &'static str,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Answer<'a> {
    kind: &'static str,
    id: &'a str,
    query: Option<&'a Query>,
    original_graphs: Vec<&'a Value>,
    review: &'a Review,
    nodes: Vec<Node<'a>>,
    relations: Vec<Relation<'a>>,
    hyperedges: Vec<Hyperedge<'a>>,
    host_links: Vec<HostLink<'a>>,
    degrees: Vec<Degree>,
    include_host_links: bool,
    relation_selection: &'static str,
    traversal: &'static str,
    host_link_origin: &'static str,
    semantic_identity_established: bool,
    selection_complete: bool,
    complete: bool,
    canonical_source_authority: bool,
    provider_calls_this_read: u8,
    source_written: bool,
}
fn reference(g: &CheckedGraph, key: &str) -> ConceptRef {
    ConceptRef {
        graph_id: g.id().into(),
        key: key.into(),
    }
}
fn next<'a>(
    source: &'a ConceptRef,
    target: &'a ConceptRef,
    frontier: &BTreeSet<ConceptRef>,
    direction: Direction,
) -> Option<&'a ConceptRef> {
    if matches!(direction, Direction::Outgoing | Direction::Both) && frontier.contains(source) {
        Some(target)
    } else if matches!(direction, Direction::Incoming | Direction::Both)
        && frontier.contains(target)
    {
        Some(source)
    } else {
        None
    }
}
impl Composition {
    pub fn id(&self) -> &str {
        &self.id
    }
    pub fn export(&self, limit: usize, d: &Deadline) -> Result<Value> {
        self.run(None, false, limit, d).map_err(|e| sanitized(e, d))
    }
    pub fn query(
        &self,
        q: &Query,
        include_host_links: bool,
        limit: usize,
        d: &Deadline,
    ) -> Result<Value> {
        self.run(Some(q), include_host_links, limit, d)
            .map_err(|e| sanitized(e, d))
    }
    fn resolve(&self, r: &ConceptRef) -> Result<()> {
        if self
            .graphs
            .iter()
            .any(|g| g.id() == r.graph_id && g.fragment.concepts.iter().any(|c| c.key == r.key))
        {
            Ok(())
        } else {
            Err(invalid())
        }
    }
    fn run(&self, q: Option<&Query>, include: bool, limit: usize, d: &Deadline) -> Result<Value> {
        check(d)?;
        if let Some(q) = q {
            query::validate(q, d)?;
        }
        input(limit <= MAX_GRAPH)?;
        capacity(limit > 0)?;
        let mut all_nodes = Vec::new();
        let mut all_relations = Vec::new();
        let mut all_hyperedges = Vec::new();
        for g in &self.graphs {
            for (index, c) in g.fragment.concepts.iter().enumerate() {
                check(d)?;
                all_nodes.push(Node {
                    index,
                    reference: reference(g, &c.key),
                    claim: c,
                });
            }
            for (index, r) in g.fragment.relations.iter().enumerate() {
                check(d)?;
                all_relations.push(Relation {
                    reference: ObservationRef {
                        graph_id: g.id().into(),
                        index,
                    },
                    source: reference(g, &r.source),
                    target: reference(g, &r.target),
                    claim: r,
                });
            }
            for (index, h) in g.fragment.hyperedges.iter().enumerate() {
                check(d)?;
                all_hyperedges.push(Hyperedge {
                    reference: ObservationRef {
                        graph_id: g.id().into(),
                        index,
                    },
                    claim: h,
                });
            }
        }
        let mut selected = BTreeSet::new();
        let mut traversed = BTreeSet::new();
        let mut traversed_links = BTreeSet::new();
        match q {
            None | Some(Query::Degree {}) => {
                selected.extend(all_nodes.iter().map(|n| n.reference.clone()))
            }
            Some(Query::Node { reference }) => {
                self.resolve(reference)?;
                selected.insert(reference.clone());
            }
            Some(Query::Find { query }) => {
                for n in &all_nodes {
                    check(d)?;
                    if n.claim.label.contains(query)
                        || n.claim.aliases.iter().any(|a| a.contains(query))
                    {
                        selected.insert(n.reference.clone());
                    }
                }
            }
            Some(Query::Trace {
                roots,
                direction,
                depth,
                max_visited,
            }) => {
                for r in roots {
                    check(d)?;
                    self.resolve(r)?;
                    selected.insert(r.clone());
                }
                capacity(selected.len() <= *max_visited)?;
                let mut frontier = selected.clone();
                for _ in 0..*depth {
                    check(d)?;
                    let mut following = BTreeSet::new();
                    for edge in &all_relations {
                        check(d)?;
                        if let Some(target) =
                            next(&edge.source, &edge.target, &frontier, *direction)
                        {
                            traversed.insert(edge.reference.clone());
                            if selected.insert(target.clone()) {
                                following.insert(target.clone());
                            }
                            capacity(selected.len() <= *max_visited)?;
                        }
                    }
                    if include {
                        for (i, link) in self.review.links.iter().enumerate() {
                            check(d)?;
                            if let Some(target) =
                                next(&link.source, &link.target, &frontier, *direction)
                            {
                                traversed_links.insert(i);
                                if selected.insert(target.clone()) {
                                    following.insert(target.clone());
                                }
                                capacity(selected.len() <= *max_visited)?;
                            }
                        }
                    }
                    frontier = following;
                    if frontier.is_empty() {
                        break;
                    }
                }
            }
        }
        let trace = matches!(q, Some(Query::Trace { .. }));
        let mut degrees = Vec::new();
        if matches!(q, Some(Query::Degree {})) {
            for n in &all_nodes {
                check(d)?;
                let r = &n.reference;
                degrees.push(Degree {
                    reference: r.clone(),
                    original_relation_in_degree: all_relations
                        .iter()
                        .filter(|e| e.target == *r)
                        .count(),
                    original_relation_out_degree: all_relations
                        .iter()
                        .filter(|e| e.source == *r)
                        .count(),
                    host_link_in_degree: if include {
                        self.review.links.iter().filter(|e| e.target == *r).count()
                    } else {
                        0
                    },
                    host_link_out_degree: if include {
                        self.review.links.iter().filter(|e| e.source == *r).count()
                    } else {
                        0
                    },
                    hyperedge_slots: all_hyperedges
                        .iter()
                        .filter(|h| h.reference.graph_id == r.graph_id)
                        .flat_map(|h| &h.claim.participants)
                        .filter(|p| p.concept == r.key)
                        .count(),
                });
            }
        }
        let nodes = all_nodes
            .into_iter()
            .filter(|n| selected.contains(&n.reference))
            .collect();
        let relations = all_relations
            .into_iter()
            .filter(|e| {
                if trace {
                    traversed.contains(&e.reference)
                } else {
                    selected.contains(&e.source) || selected.contains(&e.target)
                }
            })
            .collect();
        let hyperedges = all_hyperedges
            .into_iter()
            .filter(|h| {
                h.claim.participants.iter().any(|p| {
                    selected.contains(&ConceptRef {
                        graph_id: h.reference.graph_id.clone(),
                        key: p.concept.clone(),
                    })
                })
            })
            .collect();
        let host_links = self
            .review
            .links
            .iter()
            .enumerate()
            .filter(|(i, e)| {
                if q.is_none() {
                    true
                } else if !include {
                    false
                } else if trace {
                    traversed_links.contains(i)
                } else {
                    selected.contains(&e.source) || selected.contains(&e.target)
                }
            })
            .map(|(index, claim)| HostLink {
                index,
                claim,
                origin: "host_asserted_unverified_relation",
                attestation: "unverified",
            })
            .collect();
        output(
            &Answer {
                kind: "mixed_meaning_composition_v1",
                id: &self.id,
                query: q,
                original_graphs: self.graphs.iter().map(|g| &g.value).collect(),
                review: &self.review,
                nodes,
                relations,
                hyperedges,
                host_links,
                degrees,
                include_host_links: include,
                relation_selection: if trace {
                    "traversed_original_indices"
                } else {
                    "incident_to_selected_concepts"
                },
                traversal: if include {
                    "original_provider_relation_observations_and_explicit_host_assertions_no_hyperedge_flow"
                } else {
                    "original_provider_relation_observations_only_no_hyperedge_flow"
                },
                host_link_origin: "host_asserted_unverified_relation",
                semantic_identity_established: false,
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
}
