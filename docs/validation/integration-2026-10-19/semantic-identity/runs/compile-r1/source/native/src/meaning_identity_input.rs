//! Existing native joint source/receipt replay constructs the only prepared input.
use super::*;
pub(super) fn concepts(v: &Value) -> Result<BTreeMap<ConceptRef, &Value>> {
    let mut result = BTreeMap::new();
    for node in rows(v, "nodes")? {
        let members = rows(node, "members")?;
        require(members.len() == 1, BINDING)?;
        let reference: ConceptRef =
            crate::json::decode(members[0]["reference"].clone()).map_err(|_| fail(BINDING))?;
        require(
            result.insert(reference, &members[0]["record"]).is_none(),
            BINDING,
        )?;
    }
    Ok(result)
}
pub(super) fn prepare(store: &Store, r: &Request, o: &Options, d: &Deadline) -> Result<Prepared> {
    validate_request(r, d)?;
    validate_options(o, d)?;
    require(store.workspace_id == r.selection.workspace_id, BINDING)?;
    let review = meaning_join::Review {
        schema_version: 1,
        workspace_id: r.selection.workspace_id.clone(),
        snapshot_id: r.selection.snapshot_id.clone(),
        base_graph_hash: r.selection.base_graph_hash.clone(),
        graph_ids: r.selection.graph_ids.clone(),
        groups: vec![],
    };
    let limits = meaning_join::Options {
        max_graphs: 4,
        max_concepts: o.max_concepts,
        max_input_bytes: o.max_input_bytes.min(32_000_000),
        max_output_bytes: MAX_SOURCE,
        ..Default::default()
    };
    let source = meaning_join::join(store, &review, &limits, d).map_err(|e| {
        if e.to_string().contains("capacity") {
            fail(CAPACITY)
        } else {
            fail(BINDING)
        }
    })?;
    let concepts = concepts(source.as_value())?;
    require(concepts.len() <= o.max_concepts, CAPACITY)?;
    let pair_count = concepts.len() * concepts.len().saturating_sub(1) / 2;
    require(pair_count <= o.max_pairs, CAPACITY)?;
    for p in &r.additional_pairs {
        require(
            concepts.contains_key(&p.left) && concepts.contains_key(&p.right),
            INPUT,
        )?;
    }
    let (candidates, similarity_work) = lexical::discover(&concepts, r, o, d)?;
    let source_hash = hash(encode(source.as_value(), MAX_SOURCE, d)?);
    let admitted_bytes = source.as_value()["statistics"]["admittedInputBytes"]
        .as_u64()
        .and_then(|v| usize::try_from(v).ok())
        .ok_or_else(|| fail(BINDING))?;
    let binding = SourceBinding {
        selection: r.selection.clone(),
        source_projection_id: text(source.as_value(), "projectionId")?.into(),
        source_hash,
        candidate_set_id: identity(&("meaning-identity-candidates-v1", r, &candidates)),
    };
    d.check()?;
    Ok(Prepared {
        request: r.clone(),
        binding,
        source,
        candidates,
        admitted_bytes,
        similarity_work,
    })
}
pub(super) fn payload(p: &Prepared, contract: &str, d: &Deadline) -> Result<Value> {
    let v = json!({"kind":"meaning_identity_input_v1","sourceBinding":p.binding,"request":p.request,"source":p.source.as_value(),"candidates":p.candidates,
        "contractHash":contract,"candidateAlgorithm":"native_nfkc_context_lower_minhash64_lsh16x4_jaro_v1","mergePolicy":"complete_same_entity_clique_or_refuse",
        "selectedSourceOmittedBytes":0,"semanticIdentityEstablished":false});
    encode(&v, MAX_PAYLOAD, d)?;
    Ok(v)
}
