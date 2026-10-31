//! Pure path-policy counterexample; public View admission is tested separately.
use super::*;
use crate::model::hash;

fn equal_length_routes() -> View {
    let names = ["start", "strong", "weak", "end"];
    let observation = |name: &str, source: &str, target: &str, resolution: &str| {
        json!({
            "id": hash(name.as_bytes()),
            "source": hash(source.as_bytes()),
            "target": hash(target.as_bytes()),
            "relation": "call",
            "resolution": resolution,
            "evidenceIds": []
        })
    };
    View {
        id: hash(b"synthetic path policy view"),
        artifact_id: hash(b"synthetic path policy artifact"),
        snapshot_id: hash(b"synthetic path policy snapshot"),
        workspace_id: hash(b"synthetic path policy workspace"),
        base_graph_hash: hash(b"synthetic path policy graph"),
        overlays: vec![],
        nodes: names
            .into_iter()
            .map(|name| json!({"id": hash(name.as_bytes()), "name": name}))
            .collect(),
        // The stronger route is deliberately encountered first by ordinary BFS.
        observations: vec![
            observation("strong first", "start", "strong", "syntactic"),
            observation("strong last", "strong", "end", "syntactic"),
            observation("weak first", "start", "weak", "heuristic"),
            observation("weak last", "weak", "end", "syntactic"),
        ],
        evidence: vec![],
        limitations: vec!["Synthetic unit input; no source or runtime admission claim".into()],
        complete: false,
        meaning: None,
    }
}

#[test]
fn selected_source_category_policy_chooses_the_weakest_equal_length_route() {
    let view = equal_length_routes();
    let before = serde_json::to_vec(&view).unwrap();
    let args = json!({
        "source": "start",
        "target": "end",
        "max_hops": 4,
        "undirected": false,
        "path_policy": "weakest_source_categories_v1"
    });
    let result = shortest(&view, &args, &Deadline::new(1000)).unwrap();
    assert_eq!(result["found"], true);
    assert_eq!(result["hops"], 2);
    assert_eq!(
        result["pathObservationIds"],
        json!([hash(b"weak first"), hash(b"weak last")]),
        "The selected conservative policy must not return the first stronger BFS route"
    );
    assert_eq!(serde_json::to_vec(&view).unwrap(), before);
}

#[test]
fn omitted_policy_preserves_the_original_first_observation_route() {
    let view = equal_length_routes();
    let args = json!({"source":"start","target":"end","max_hops":4,"undirected":false});
    let result = shortest(&view, &args, &Deadline::new(1000)).unwrap();
    assert_eq!(
        result["pathObservationIds"],
        json!([hash(b"strong first"), hash(b"strong last")])
    );
    assert!(result.get("pathPolicy").is_none());
}

#[test]
fn unmapped_claim_is_weaker_than_a_recognized_inference() {
    let mut view = equal_length_routes();
    view.observations[0]["resolution"] = json!("future_unmapped_claim");
    let args = json!({"source":"start","target":"end","max_hops":4,"undirected":false,"path_policy":"weakest_source_categories_v1"});
    let result = shortest(&view, &args, &Deadline::new(1000)).unwrap();
    assert_eq!(
        result["pathObservationIds"],
        json!([hash(b"strong first"), hash(b"strong last")])
    );
    assert_eq!(result["pathSourceCategory"], "unmapped");
}

#[test]
fn selected_policy_keeps_direction_and_does_not_invent_a_self_path_claim() {
    let view = equal_length_routes();
    let mut args = json!({"source":"end","target":"start","max_hops":4,"undirected":false,"path_policy":"weakest_source_categories_v1"});
    let reverse = shortest(&view, &args, &Deadline::new(1000)).unwrap();
    assert_eq!(reverse["found"], false);
    assert_eq!(reverse["pathSourceCategory"], Value::Null);
    args["undirected"] = json!(true);
    let reverse = shortest(&view, &args, &Deadline::new(1000)).unwrap();
    assert_eq!(
        reverse["pathObservationIds"],
        json!([hash(b"weak last"), hash(b"weak first")])
    );
    args["target"] = json!("end");
    let same = shortest(&view, &args, &Deadline::new(1000)).unwrap();
    assert_eq!(same["found"], true);
    assert_eq!(same["hops"], 0);
    assert_eq!(same["pathSourceCategory"], Value::Null);
    assert_eq!(same["pathObservationIds"], json!([]));
}

#[test]
fn reconverging_prefixes_keep_the_first_tie_after_a_weaker_suffix() {
    let mut view = equal_length_routes();
    view.nodes.push(json!({"id":hash(b"tail"),"name":"tail"}));
    view.observations.push(json!({
        "id":hash(b"unmapped suffix"),"source":hash(b"end"),"target":hash(b"tail"),
        "relation":"call","resolution":"unmapped_claim","evidenceIds":[]
    }));
    let args = json!({"source":"start","target":"tail","max_hops":4,"undirected":false,"path_policy":"weakest_source_categories_v1"});
    let result = shortest(&view, &args, &Deadline::new(1000)).unwrap();
    // Keeping only the weakest prefix at `end` would wrongly discard this
    // earlier prefix before the final hop makes both routes equally weak.
    assert_eq!(
        result["pathObservationIds"],
        json!([
            hash(b"strong first"),
            hash(b"strong last"),
            hash(b"unmapped suffix")
        ])
    );
    assert_eq!(result["pathSourceCategory"], "unmapped");
}

#[test]
fn a_longer_weaker_route_cannot_displace_a_shorter_stronger_route() {
    let mut view = equal_length_routes();
    view.nodes
        .push(json!({"id":hash(b"detour"),"name":"detour"}));
    view.observations[3]["target"] = json!(hash(b"detour"));
    view.observations.push(json!({
        "id":hash(b"detour last"),"source":hash(b"detour"),"target":hash(b"end"),
        "relation":"call","resolution":"syntactic","evidenceIds":[]
    }));
    let args = json!({"source":"start","target":"end","max_hops":4,"undirected":false,"path_policy":"weakest_source_categories_v1"});
    let result = shortest(&view, &args, &Deadline::new(1000)).unwrap();
    assert_eq!(result["hops"], 2);
    assert_eq!(
        result["pathObservationIds"],
        json!([hash(b"strong first"), hash(b"strong last")])
    );
    assert_eq!(result["pathSourceCategory"], "extracted");
}
