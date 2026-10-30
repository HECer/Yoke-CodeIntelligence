//! Lossless quotient under a complete positive-clique policy. No input graph changes.
use super::*;
pub(super) fn project(p: &Prepared, f: &Fragment, d: &Deadline) -> Result<Value> {
    let concepts = source::concepts(p.source.as_value())?;
    require(f.decisions.len() == p.candidates.len(), BINDING)?;
    let candidates: BTreeMap<_, _> = p.candidates.iter().map(|c| (c.id.as_str(), c)).collect();
    let mut decisions = BTreeMap::new();
    let mut positive = BTreeMap::<ConceptRef, BTreeSet<ConceptRef>>::new();
    for (index, decision) in f.decisions.iter().enumerate() {
        d.check()?;
        label(&decision.rationale, 1024)?;
        let c = *candidates
            .get(decision.candidate_id.as_str())
            .ok_or_else(|| fail(BINDING))?;
        require(
            decisions
                .insert((&c.left, &c.right), (index, decision.outcome))
                .is_none(),
            BINDING,
        )?;
        require((2..=16).contains(&decision.support.len()), BINDING)?;
        let mut supported = BTreeSet::new();
        let mut unique = BTreeSet::new();
        for support in &decision.support {
            d.check()?;
            require(
                support.reference == c.left || support.reference == c.right,
                BINDING,
            )?;
            let record = *concepts
                .get(&support.reference)
                .ok_or_else(|| fail(BINDING))?;
            require(
                valid_id(&support.evidence_id)
                    && rows(record, "evidenceIds")?
                        .iter()
                        .any(|id| id.as_str() == Some(&support.evidence_id))
                    && unique.insert((&support.reference, &support.evidence_id)),
                BINDING,
            )?;
            require(
                rows(p.source.as_value(), "evidence")?.iter().any(|e| {
                    e["graphId"] == support.reference.graph_id
                        && e["packetIndex"] == support.reference.packet_index
                        && e["record"]["id"] == support.evidence_id
                }),
                BINDING,
            )?;
            supported.insert(&support.reference);
        }
        require(
            supported.contains(&c.left) && supported.contains(&c.right),
            BINDING,
        )?;
        if decision.outcome == Outcome::SameEntity {
            positive
                .entry(c.left.clone())
                .or_default()
                .insert(c.right.clone());
            positive
                .entry(c.right.clone())
                .or_default()
                .insert(c.left.clone());
        }
    }
    let mut remaining = concepts.keys().cloned().collect::<BTreeSet<_>>();
    let mut groups = Vec::new();
    while let Some(root) = remaining.pop_first() {
        let mut members = BTreeSet::from([root.clone()]);
        let mut pending = vec![root];
        while let Some(node) = pending.pop() {
            d.check()?;
            if let Some(next) = positive.get(&node) {
                for n in next {
                    if members.insert(n.clone()) {
                        remaining.remove(n);
                        pending.push(n.clone());
                    }
                }
            }
        }
        let members = members.into_iter().collect::<Vec<_>>();
        for i in 0..members.len() {
            for j in i + 1..members.len() {
                require(
                    decisions
                        .get(&(&members[i], &members[j]))
                        .is_some_and(|(_, o)| *o == Outcome::SameEntity),
                    "meaning_identity_non_clique_refused",
                )?;
            }
        }
        groups.push(members);
    }
    let mut target = BTreeMap::new();
    let mut nodes = Vec::new();
    for members in groups {
        d.check()?;
        let id = identity(&("meaning-identity-node-v1", &members));
        for m in &members {
            target.insert(m.clone(), id.clone());
        }
        nodes.push(json!({"id":id,"representative":members[0],"members":members.iter().map(|m|json!({"reference":m,"record":concepts[m]})).collect::<Vec<_>>(),
            "groupingAuthority":if members.len()>1 {"unverified_model_same_entity_clique"}else{"original_singleton"},"semanticIdentityEstablished":false}));
    }
    nodes.sort_by(|a, b| a["id"].as_str().cmp(&b["id"].as_str()));
    let mut remap = BTreeMap::new();
    for node in rows(p.source.as_value(), "nodes")? {
        let reference: ConceptRef =
            crate::json::decode(rows(node, "members")?[0]["reference"].clone())
                .map_err(|_| fail(BINDING))?;
        remap.insert(text(node, "id")?.to_owned(), target[&reference].clone());
    }
    let mut relations = Vec::new();
    for row in rows(p.source.as_value(), "relations")? {
        d.check()?;
        let mut row = row.clone();
        for (key, original) in [
            ("source", "originalProjectedSource"),
            ("target", "originalProjectedTarget"),
        ] {
            let old = text(&row, key)?.to_owned();
            row[original] = json!(old);
            row[key] = json!(remap.get(&old).ok_or_else(|| fail(BINDING))?);
        }
        relations.push(row);
    }
    let mut hyperedges = Vec::new();
    for row in rows(p.source.as_value(), "hyperedges")? {
        d.check()?;
        let mut row = row.clone();
        for slot in row["participants"]
            .as_array_mut()
            .ok_or_else(|| fail(BINDING))?
        {
            let old = text(slot, "nodeId")?.to_owned();
            slot["originalNodeId"] = json!(old);
            slot["nodeId"] = json!(remap.get(&old).ok_or_else(|| fail(BINDING))?);
        }
        hyperedges.push(row);
    }
    d.check()?;
    Ok(
        json!({"kind":"meaning_identity_quotient_v1","sourceBinding":p.binding,"originalSelection":p.source.as_value(),"nodes":nodes,"relations":relations,"hyperedges":hyperedges,
        "originalFragment":f,"candidates":p.candidates,"originalConceptCount":concepts.len(),"projectedNodeCount":target.values().collect::<BTreeSet<_>>().len(),
        "mergePolicy":"complete_same_entity_clique_or_refuse","groupingAuthority":"explicit_host_policy_over_unverified_model_decisions",
        "semanticIdentityEstablished":false,"canonicalSourceAuthority":false,"executionObserved":false,"originalClaimsModified":false,"selectionComplete":true,"complete":false}),
    )
}
