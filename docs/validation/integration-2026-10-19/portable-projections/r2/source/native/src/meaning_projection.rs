//! Data-only consumers. No Store or checked source capability is constructed.
pub use crate::meaning_join::{ConceptRef, Direction, Query};
use crate::{capture::Deadline, model::valid_id};
use anyhow::{Result, bail};
use serde::Serialize;
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

pub const MAX_BYTES: usize = 4_000_000;
#[derive(Clone, Debug)]
pub struct Options {
    pub max_input_bytes: usize,
    pub max_concepts: usize,
    pub max_relations: usize,
    pub max_hyperedges: usize,
    pub max_participants: usize,
    pub max_evidence: usize,
}
impl Default for Options {
    fn default() -> Self {
        Self {
            max_input_bytes: MAX_BYTES,
            max_concepts: 4096,
            max_relations: 32768,
            max_hyperedges: 8192,
            max_participants: 65536,
            max_evidence: 65536,
        }
    }
}
fn input(ok: bool) -> Result<()> {
    if !ok {
        bail!("meaning_projection_input_invalid");
    }
    Ok(())
}
fn capacity(ok: bool) -> Result<()> {
    if !ok {
        bail!("meaning_projection_capacity_exceeded");
    }
    Ok(())
}
fn check(d: &Deadline) -> Result<()> {
    d.check()
        .map_err(|_| anyhow::anyhow!("meaning_projection_deadline_exceeded"))
}
pub fn validate_options(o: &Options) -> Result<()> {
    input(
        o.max_input_bytes <= MAX_BYTES
            && o.max_concepts <= 4096
            && o.max_relations <= 32768
            && o.max_hyperedges <= 8192
            && o.max_participants <= 65536
            && o.max_evidence <= 65536,
    )
}
fn rows<'a>(v: &'a Value, k: &str) -> Result<&'a [Value]> {
    v[k].as_array()
        .map(Vec::as_slice)
        .ok_or_else(|| anyhow::anyhow!("meaning_projection_input_invalid"))
}
fn text<'a>(v: &'a Value, k: &str) -> Result<&'a str> {
    v[k].as_str()
        .ok_or_else(|| anyhow::anyhow!("meaning_projection_input_invalid"))
}
fn number(v: &Value, k: &str) -> Result<usize> {
    v[k].as_u64()
        .and_then(|n| usize::try_from(n).ok())
        .ok_or_else(|| anyhow::anyhow!("meaning_projection_input_invalid"))
}
fn fields(v: &Value, keys: &[&str]) -> Result<()> {
    input(
        v.as_object()
            .is_some_and(|m| m.len() == keys.len() && keys.iter().all(|k| m.contains_key(*k))),
    )
}
fn decode<T: serde::de::DeserializeOwned>(v: &Value) -> Result<T> {
    crate::json::decode(v.clone()).map_err(|_| anyhow::anyhow!("meaning_projection_input_invalid"))
}
fn encode<T: Serialize>(v: &T, limit: usize, d: &Deadline) -> Result<Vec<u8>> {
    check(d)?;
    let bytes = crate::json::encode_bounded(v, limit, d);
    check(d)?;
    bytes.map_err(|_| anyhow::anyhow!("meaning_projection_capacity_exceeded"))
}
fn digest<T: Serialize>(v: &T, d: &Deadline) -> Result<String> {
    let h = crate::model::hash(encode(v, 128_128, d)?);
    check(d)?;
    Ok(h)
}
#[derive(Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
struct Counts {
    graphs: usize,
    packets: usize,
    concepts: usize,
    nodes: usize,
    relations: usize,
    hyperedges: usize,
    participants: usize,
    evidence: usize,
}

/// This type certifies projection structure only, never a captured source replay.
pub struct Projection {
    value: Value,
    graph_ids: Vec<String>,
    single_graph: Option<String>,
    transport_hash: String,
    counts: Counts,
    members: BTreeMap<ConceptRef, usize>,
}
impl Projection {
    fn reference(&self, v: &Value) -> Result<ConceptRef> {
        let r = if let Some(id) = &self.single_graph {
            let r: crate::meaning_groups::ConceptRef = decode(v)?;
            ConceptRef {
                graph_id: id.clone(),
                packet_index: r.packet_index,
                concept_id: r.concept_id,
            }
        } else {
            decode(v)?
        };
        input(
            valid_id(&r.graph_id)
                && valid_id(&r.concept_id)
                && r.packet_index < 64
                && self.graph_ids.contains(&r.graph_id),
        )?;
        Ok(r)
    }
    fn qualification(&self, v: &Value) -> Result<(String, usize)> {
        let graph = if let Some(id) = &self.single_graph {
            id.as_str()
        } else {
            text(v, "graphId")?
        };
        let packet = number(v, "packetIndex")?;
        input(packet < 64 && self.graph_ids.iter().any(|g| g == graph))?;
        Ok((graph.to_owned(), packet))
    }
    fn node_id(&self, r: &ConceptRef) -> Result<&str> {
        let i = self
            .members
            .get(r)
            .ok_or_else(|| anyhow::anyhow!("meaning_projection_input_invalid"))?;
        text(&rows(&self.value, "nodes")?[*i], "id")
    }
    pub fn parse(bytes: &[u8], o: &Options, d: &Deadline) -> Result<Self> {
        validate_options(o)?;
        check(d)?;
        capacity(bytes.len() <= o.max_input_bytes)?;
        let value = crate::json::parse(bytes, o.max_input_bytes)
            .map_err(|_| anyhow::anyhow!("meaning_projection_input_invalid"))?;
        check(d)?;
        input(value["schemaVersion"] == 1)?;
        let single = match text(&value, "kind")? {
            "meaning_group_projection_v1" => Some(text(&value, "graphId")?.to_owned()),
            "meaning_join_projection_v1" => None,
            _ => {
                input(false)?;
                unreachable!()
            }
        };
        let mut graph_ids = if let Some(id) = &single {
            vec![id.clone()]
        } else {
            rows(&value["review"], "graphIds")?
                .iter()
                .map(|v| {
                    v.as_str()
                        .map(str::to_owned)
                        .ok_or_else(|| anyhow::anyhow!("meaning_projection_input_invalid"))
                })
                .collect::<Result<Vec<_>>>()?
        };
        input(
            !graph_ids.is_empty() && graph_ids.len() <= 8 && graph_ids.iter().all(|g| valid_id(g)),
        )?;
        graph_ids.sort();
        input(graph_ids.windows(2).all(|w| w[0] != w[1]))?;
        let mut p = Self {
            value,
            graph_ids,
            single_graph: single,
            transport_hash: crate::model::hash(bytes),
            counts: Counts::default(),
            members: BTreeMap::new(),
        };
        p.validate(o, d)?;
        check(d)?;
        Ok(p)
    }
    fn validate(&mut self, o: &Options, d: &Deadline) -> Result<()> {
        // Packet metadata is retained as data. Only qualification is consumed.
        let mut packets = BTreeSet::new();
        if let Some(id) = &self.single_graph {
            for packet in rows(&self.value, "packets")? {
                check(d)?;
                let index = number(packet, "packetIndex")?;
                input(index < 64 && packets.insert((id.clone(), index)))?;
            }
        } else {
            let mut parents = BTreeSet::new();
            for parent in rows(&self.value, "parents")? {
                check(d)?;
                let id = text(parent, "graphId")?;
                input(self.graph_ids.iter().any(|g| g == id) && parents.insert(id.to_owned()))?;
                for packet in rows(parent, "packets")? {
                    check(d)?;
                    let index = number(packet, "packetIndex")?;
                    input(index < 64 && packets.insert((id.to_owned(), index)))?;
                }
            }
            input(parents.len() == self.graph_ids.len())?;
        }
        capacity(packets.len() <= 512)?;
        self.counts.graphs = self.graph_ids.len();
        self.counts.packets = packets.len();
        let nodes = rows(&self.value, "nodes")?;
        capacity(nodes.len() <= o.max_concepts)?;
        self.counts.nodes = nodes.len();
        let mut ids = BTreeSet::new();
        for (i, node) in nodes.iter().enumerate() {
            check(d)?;
            fields(node, &["id", "representative", "members"])?;
            let id = text(node, "id")?;
            input(valid_id(id) && ids.insert(id))?;
            let members = rows(node, "members")?;
            input(!members.is_empty() && members.len() <= 128)?;
            self.counts.concepts += members.len();
            capacity(self.counts.concepts <= o.max_concepts)?;
            let mut refs = Vec::new();
            for member in members {
                check(d)?;
                fields(member, &["reference", "record"])?;
                let r = self.reference(&member["reference"])?;
                input(
                    packets.contains(&(r.graph_id.clone(), r.packet_index))
                        && text(&member["record"], "id")? == r.concept_id,
                )?;
                input(self.members.insert(r.clone(), i).is_none())?;
                refs.push(r);
            }
            refs.sort();
            let rep = self.reference(&node["representative"])?;
            input(refs.contains(&rep))?;
            let expected = if let Some(graph) = &self.single_graph {
                let local: Vec<_> = refs
                    .iter()
                    .map(|r| crate::meaning_groups::ConceptRef {
                        packet_index: r.packet_index,
                        concept_id: r.concept_id.clone(),
                    })
                    .collect();
                digest(&("meaning-group-node-v1", graph, local), d)?
            } else {
                digest(&("meaning-join-node-v1", &refs), d)?
            };
            input(id == expected)?;
        }
        self.validate_policy(d)?;
        let mut evidence = BTreeSet::new();
        self.counts.evidence = rows(&self.value, "evidence")?.len();
        capacity(self.counts.evidence <= o.max_evidence)?;
        for e in rows(&self.value, "evidence")? {
            check(d)?;
            let (g, p) = self.qualification(e)?;
            input(packets.contains(&(g.clone(), p)))?;
            let record = &e["record"];
            let id = text(record, "id")?;
            input(valid_id(id) && evidence.insert((g, p, id.to_owned())))?;
            input(valid_id(text(record, "sourceId")?) && valid_id(text(record, "contentHash")?))?;
            let start = number(record, "startByte")?;
            let end = number(record, "endByte")?;
            input(
                start < end && end <= 4 * 1024 * 1024 && end - start == text(record, "text")?.len(),
            )?;
            let path = text(record, "path")?;
            input(
                path.len() <= 4096
                    && crate::paths::normalized_syntax(path).is_ok_and(|p| p == path),
            )?;
        }
        let support = |record: &Value, g: &str, p: usize| -> Result<()> {
            let refs = rows(record, "evidenceIds")?;
            input(!refs.is_empty() && refs.len() <= 8)?;
            let mut unique = BTreeSet::new();
            for r in refs {
                check(d)?;
                let id = r
                    .as_str()
                    .ok_or_else(|| anyhow::anyhow!("meaning_projection_input_invalid"))?;
                input(unique.insert(id) && evidence.contains(&(g.to_owned(), p, id.to_owned())))?;
            }
            Ok(())
        };
        for node in nodes {
            for member in rows(node, "members")? {
                check(d)?;
                let r = self.reference(&member["reference"])?;
                support(&member["record"], &r.graph_id, r.packet_index)?;
            }
        }
        let mut relation_ids = BTreeSet::new();
        self.counts.relations = rows(&self.value, "relations")?.len();
        capacity(self.counts.relations <= o.max_relations)?;
        for edge in rows(&self.value, "relations")? {
            check(d)?;
            fields(edge, &["reference", "record", "source", "target"])?;
            let (g, p) = self.qualification(&edge["reference"])?;
            let r = &edge["record"];
            let id = text(r, "id")?;
            input(
                valid_id(id)
                    && text(&edge["reference"], "relationId")? == id
                    && relation_ids.insert((g.clone(), p, id)),
            )?;
            for k in ["source", "target"] {
                let endpoint = ConceptRef {
                    graph_id: g.clone(),
                    packet_index: p,
                    concept_id: text(r, k)?.to_owned(),
                };
                input(self.node_id(&endpoint)? == text(edge, k)?)?;
            }
            support(r, &g, p)?;
        }
        let mut hyper_ids = BTreeSet::new();
        self.counts.hyperedges = rows(&self.value, "hyperedges")?.len();
        capacity(self.counts.hyperedges <= o.max_hyperedges)?;
        for edge in rows(&self.value, "hyperedges")? {
            check(d)?;
            fields(edge, &["reference", "record", "participants"])?;
            let (g, p) = self.qualification(&edge["reference"])?;
            let r = &edge["record"];
            let id = text(r, "id")?;
            input(
                valid_id(id)
                    && text(&edge["reference"], "hyperedgeId")? == id
                    && hyper_ids.insert((g.clone(), p, id)),
            )?;
            let slots = rows(edge, "participants")?;
            let original = rows(r, "participants")?;
            input((2..=16).contains(&slots.len()) && slots.len() == original.len())?;
            self.counts.participants += slots.len();
            capacity(self.counts.participants <= o.max_participants)?;
            for (i, (slot, old)) in slots.iter().zip(original).enumerate() {
                check(d)?;
                fields(slot, &["position", "role", "original", "nodeId"])?;
                let reference = self.reference(&slot["original"])?;
                input(
                    number(slot, "position")? == i
                        && text(slot, "role")? == text(old, "role")?
                        && reference.graph_id == g
                        && reference.packet_index == p
                        && reference.concept_id == text(old, "concept")?
                        && self.node_id(&reference)? == text(slot, "nodeId")?,
                )?;
            }
            support(r, &g, p)?;
        }
        check(d)
    }
    fn validate_policy(&self, d: &Deadline) -> Result<()> {
        let mut groups: Vec<(Vec<ConceptRef>, ConceptRef)> = Vec::new();
        let policy;
        let projection;
        if let Some(id) = &self.single_graph {
            let mut review: crate::meaning_groups::Review = decode(&self.value["review"])?;
            let o = crate::meaning_groups::Options {
                max_groups: 512,
                max_members: 4096,
                max_concepts: 4096,
                max_relations: 32768,
                max_hyperedges: 8192,
                max_participants: 65536,
                max_evidence: 65536,
                ..Default::default()
            };
            crate::meaning_groups::validate_review(&review, &o, d)
                .map_err(|_| anyhow::anyhow!("meaning_projection_input_invalid"))?;
            input(review.graph_id == *id)?;
            for g in &mut review.groups {
                check(d)?;
                g.members.sort();
            }
            review.groups.sort_by(|a, b| a.members.cmp(&b.members));
            for g in &review.groups {
                let refs = g
                    .members
                    .iter()
                    .map(|r| ConceptRef {
                        graph_id: id.clone(),
                        packet_index: r.packet_index,
                        concept_id: r.concept_id.clone(),
                    })
                    .collect();
                let rep = ConceptRef {
                    graph_id: id.clone(),
                    packet_index: g.representative.packet_index,
                    concept_id: g.representative.concept_id.clone(),
                };
                groups.push((refs, rep));
            }
            policy = digest(&("meaning-group-policy-v1", &review), d)?;
            projection = digest(&("meaning-group-projection-v1", id, &policy), d)?;
        } else {
            let mut review: crate::meaning_join::Review = decode(&self.value["review"])?;
            let o = crate::meaning_join::Options {
                max_graphs: 8,
                max_groups: 512,
                max_concepts: 4096,
                max_relations: 32768,
                max_hyperedges: 8192,
                max_participants: 65536,
                max_evidence: 65536,
                ..Default::default()
            };
            crate::meaning_join::validate_review(&review, &o, d)
                .map_err(|_| anyhow::anyhow!("meaning_projection_input_invalid"))?;
            review.graph_ids.sort();
            input(review.graph_ids == self.graph_ids)?;
            for g in &mut review.groups {
                check(d)?;
                g.members.sort();
            }
            review.groups.sort_by(|a, b| a.members.cmp(&b.members));
            for g in &review.groups {
                groups.push((g.members.clone(), g.representative.clone()));
            }
            policy = digest(&("meaning-join-policy-v1", &review), d)?;
            projection = digest(&("meaning-join-projection-v1", &policy), d)?;
        }
        check(d)?;
        input(
            text(&self.value, "policyHash")? == policy
                && text(&self.value, "projectionId")? == projection,
        )?;
        let mut grouped_nodes = BTreeSet::new();
        for (refs, rep) in groups {
            check(d)?;
            let i = *self
                .members
                .get(&rep)
                .ok_or_else(|| anyhow::anyhow!("meaning_projection_input_invalid"))?;
            let node = &rows(&self.value, "nodes")?[i];
            input(
                grouped_nodes.insert(i)
                    && self.reference(&node["representative"])? == rep
                    && rows(node, "members")?.len() == refs.len(),
            )?;
            for r in refs {
                check(d)?;
                input(self.members.get(&r) == Some(&i))?;
            }
        }
        for (i, node) in rows(&self.value, "nodes")?.iter().enumerate() {
            check(d)?;
            input(grouped_nodes.contains(&i) || rows(node, "members")?.len() == 1)?;
        }
        check(d)
    }
    pub fn inspect(&self, max_bytes: usize, d: &Deadline) -> Result<Value> {
        let selected = rows(&self.value, "nodes")?
            .iter()
            .map(|n| text(n, "id"))
            .collect::<Result<BTreeSet<_>>>()?;
        self.answer(None, &selected, Vec::new(), max_bytes, d)
    }
    pub fn query(&self, q: &Query, max_bytes: usize, d: &Deadline) -> Result<Value> {
        check(d)?;
        crate::meaning_join::validate_query(q, &self.graph_ids, d)
            .map_err(|_| anyhow::anyhow!("meaning_projection_input_invalid"))?;
        check(d)?;
        let mut selected = BTreeSet::new();
        let mut degrees = Vec::new();
        match q {
            Query::Node { reference } => {
                selected.insert(self.node_id(reference)?);
            }
            Query::Degree {} => {
                let mut counts = BTreeMap::new();
                for node in rows(&self.value, "nodes")? {
                    check(d)?;
                    counts.insert(text(node, "id")?, (0usize, 0usize, 0usize));
                }
                for edge in rows(&self.value, "relations")? {
                    check(d)?;
                    counts
                        .get_mut(text(edge, "source")?)
                        .ok_or_else(|| anyhow::anyhow!("meaning_projection_input_invalid"))?
                        .1 += 1;
                    counts
                        .get_mut(text(edge, "target")?)
                        .ok_or_else(|| anyhow::anyhow!("meaning_projection_input_invalid"))?
                        .0 += 1;
                }
                for edge in rows(&self.value, "hyperedges")? {
                    for slot in rows(edge, "participants")? {
                        check(d)?;
                        counts
                            .get_mut(text(slot, "nodeId")?)
                            .ok_or_else(|| anyhow::anyhow!("meaning_projection_input_invalid"))?
                            .2 += 1;
                    }
                }
                for (id, (incoming, outgoing, slots)) in counts {
                    check(d)?;
                    degrees.push(json!({"nodeId":id,"inDegree":incoming,"outDegree":outgoing,"hyperedgeSlots":slots}));
                }
            }
            Query::Trace {
                roots,
                direction,
                depth,
                max_visited,
            } => {
                for root in roots {
                    check(d)?;
                    selected.insert(self.node_id(root)?);
                    capacity(selected.len() <= *max_visited)?;
                }
                let mut frontier = selected.clone();
                for _ in 0..*depth {
                    check(d)?;
                    if frontier.is_empty() {
                        break;
                    }
                    let mut next = BTreeSet::new();
                    for edge in rows(&self.value, "relations")? {
                        check(d)?;
                        let source = text(edge, "source")?;
                        let target = text(edge, "target")?;
                        for (eligible, candidate) in [
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
                            if eligible && !selected.contains(candidate) && next.insert(candidate) {
                                capacity(selected.len() + next.len() <= *max_visited)?;
                            }
                        }
                    }
                    selected.extend(next.iter().copied());
                    frontier = next;
                }
            }
        }
        self.answer(Some(q), &selected, degrees, max_bytes, d)
    }
    fn answer(
        &self,
        q: Option<&Query>,
        selected: &BTreeSet<&str>,
        degrees: Vec<Value>,
        max_bytes: usize,
        d: &Deadline,
    ) -> Result<Value> {
        input(max_bytes <= MAX_BYTES)?;
        capacity(max_bytes > 0)?;
        check(d)?;
        let metadata: BTreeMap<_, _> = self
            .value
            .as_object()
            .ok_or_else(|| anyhow::anyhow!("meaning_projection_input_invalid"))?
            .iter()
            .filter(|(k, _)| !matches!(k.as_str(), "nodes" | "relations" | "hyperedges"))
            .collect();
        let mut nodes = Vec::new();
        let mut relations = Vec::new();
        let mut hyperedges = Vec::new();
        for n in rows(&self.value, "nodes")? {
            check(d)?;
            if selected.contains(text(n, "id")?) {
                nodes.push(n);
            }
        }
        for e in rows(&self.value, "relations")? {
            check(d)?;
            if selected.contains(text(e, "source")?) && selected.contains(text(e, "target")?) {
                relations.push(e);
            }
        }
        for e in rows(&self.value, "hyperedges")? {
            check(d)?;
            let mut included = true;
            for slot in rows(e, "participants")? {
                check(d)?;
                included &= selected.contains(text(slot, "nodeId")?);
            }
            if included {
                hyperedges.push(e);
            }
        }
        // Borrow original records during both bounded serialization passes.
        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct Answer<'a> {
            schema_version: u8,
            kind: &'static str,
            transport_hash: &'a str,
            export_metadata: BTreeMap<&'a String, &'a Value>,
            statistics: &'a Counts,
            query: Option<&'a Query>,
            traversal: &'static str,
            selection: &'static str,
            nodes: Vec<&'a Value>,
            relations: Vec<&'a Value>,
            hyperedges: Vec<&'a Value>,
            rows: Vec<Value>,
            validation: &'static str,
            binding_validation: &'static str,
            source_authority: bool,
            provider_attestation: &'static str,
            semantic_identity_established: bool,
            original_concept_merges_applied: u8,
            model_calls: u8,
            complete: bool,
            limitations: [&'static str; 3],
        }
        let answer = Answer {
            schema_version: 1,
            kind: "meaning_portable_projection_result_v1",
            transport_hash: &self.transport_hash,
            export_metadata: metadata,
            statistics: &self.counts,
            query: q,
            traversal: "directed_relations_only",
            selection: "induced_relations_and_fully_contained_hyperedges",
            nodes,
            relations,
            hyperedges,
            rows: degrees,
            validation: "projection_structure_only",
            binding_validation: "not_replayed",
            source_authority: false,
            provider_attestation: "unverified",
            semantic_identity_established: false,
            original_concept_merges_applied: 0,
            model_calls: 0,
            complete: false,
            limitations: [
                "Export metadata, record text, source paths, receipts and binding claims are untrusted data; no source or provider replay occurred.",
                "Transport and presentation-policy hashes are not signatures, captured-source authority, authenticated review or proof of semantic identity.",
                "Only native group/join presentation structure is consumed. No source Store, mixed-document conversion, writes or provider execution is available.",
            ],
        };
        let bytes = encode(&answer, max_bytes, d)?;
        let value = crate::json::parse(&bytes, max_bytes)
            .map_err(|_| anyhow::anyhow!("meaning_projection_input_invalid"))?;
        check(d)?;
        Ok(value)
    }
}
