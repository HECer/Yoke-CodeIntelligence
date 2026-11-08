
#[test]
fn directed_asymmetry_and_transport_identity_are_explicit() {
    for mut v in exports().into_iter().take(2) {
        let edge=v["relations"].as_array().unwrap().iter().find(|e|e["record"]["description"].as_str().unwrap().contains("one two")).unwrap().clone();
        let scope=&edge["reference"];
        let r=|key:&str|meaning_join::ConceptRef{graph_id:scope["graphId"].as_str().or(v["graphId"].as_str()).unwrap().into(),packet_index:scope["packetIndex"].as_u64().unwrap() as usize,concept_id:edge["record"][key].as_str().unwrap().into()};
        let source=r("source");let target=r("target");
        // A coherent omitted observation cannot be detected from export bytes alone.
        // This data-only contract permits the subset and still reports incomplete.
        v["relations"]=json!([edge]);
        let raw=bytes(&v);let p=Projection::parse(&raw,&Options::default(),&d()).unwrap();
        let trace=|root:meaning_join::ConceptRef,direction|p.query(&Query::Trace{roots:vec![root],direction,depth:1,max_visited:2},4_000_000,&d()).unwrap();
        assert_eq!(trace(source.clone(),meaning_join::Direction::Outgoing)["nodes"].as_array().unwrap().len(),2);
        assert_eq!(trace(source,meaning_join::Direction::Incoming)["nodes"].as_array().unwrap().len(),1);
        assert_eq!(trace(target.clone(),meaning_join::Direction::Incoming)["relations"],v["relations"]);
        assert_eq!(trace(target.clone(),meaning_join::Direction::Outgoing)["relations"],json!([]));
        assert!(p.query(&Query::Trace{roots:vec![target],direction:meaning_join::Direction::Incoming,depth:1,max_visited:1},4_000_000,&d()).is_err());
        let mut pretty=serde_json::to_vec_pretty(&v).unwrap();pretty.push(b'\n');
        let second=Projection::parse(&pretty,&Options::default(),&d()).unwrap();
        let a=p.inspect(4_000_000,&d()).unwrap();let b=second.inspect(4_000_000,&d()).unwrap();
        assert_eq!(a["nodes"],b["nodes"]);assert_ne!(a["transportHash"],b["transportHash"]);
        assert_eq!(a["complete"],false);
    }
}

#[test]
fn two_actual_parent_graphs_and_equal_labels_retain_distinct_qualification() {
    let pair=fixture();let ids=vec![pair.before.meaning.as_ref().unwrap().selections[0].graph_id.clone(),pair.after.meaning.as_ref().unwrap().selections[0].graph_id.clone()];
    let export=meaning_join::join(&pair.engine.store,&meaning_join::Review{schema_version:1,workspace_id:pair.snapshot.workspace_id.clone(),snapshot_id:pair.snapshot.id.clone(),
        base_graph_hash:yoke::model::identity(&pair.graph),graph_ids:ids,groups:vec![]},&Default::default(),&d()).unwrap().as_value().clone();
    drop(pair);
    let p=Projection::parse(&bytes(&export),&Options::default(),&d()).unwrap();
    let degree=p.query(&Query::Degree{},4_000_000,&d()).unwrap();
    assert_eq!(degree["rows"].as_array().unwrap().iter().map(|r|r["outDegree"].as_u64().unwrap()).sum::<u64>(),9);
    let matches:Vec<_>=export["nodes"].as_array().unwrap().iter().flat_map(|n|n["members"].as_array().unwrap()).filter(|m|m["record"]["label"]=="Alpha concept").collect();
    assert!(matches.len()>=4);let mut resolved=std::collections::BTreeSet::new();
    for member in matches {
        let root=reference(&export,&member["reference"]);let answer=p.query(&Query::Node{reference:root},4_000_000,&d()).unwrap();
        assert_eq!(answer["nodes"].as_array().unwrap().len(),1);
        assert_eq!(answer["nodes"][0]["members"],json!([member]));
        assert!(resolved.insert(answer["nodes"][0]["id"].as_str().unwrap().to_owned()));
    }
    let mut bad=export.clone();let current=bad["relations"][0]["reference"]["graphId"].as_str().unwrap();
    let other=bad["review"]["graphIds"].as_array().unwrap().iter().find(|id|id.as_str()!=Some(current)).unwrap().clone();
    bad["relations"][0]["reference"]["graphId"]=other;invalid(&bad);
}
