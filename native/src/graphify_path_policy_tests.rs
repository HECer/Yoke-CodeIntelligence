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

#[test]
fn hop_rendering_keeps_full_evidence_order_and_shared_sites_without_collapsing_observations() {
    let mut view = equal_length_routes();
    // Pure rendering fixture, not source admission: deliberately put IDs in
    // reverse lexical order and share one site across observations and hops.
    let first = json!({
        "id":"f".repeat(64),"path":"synthetic.rs","startByte":0,"endByte":2,
        "anchor":"first original site","trust":"synthetic_unit_only","producer":"unit_fixture"
    });
    let shared = json!({
        "id":"0".repeat(64),"path":"synthetic.rs","startByte":3,"endByte":7,
        "anchor":"shared original site","trust":"synthetic_unit_only","producer":"unit_fixture"
    });
    view.evidence = vec![first.clone(), shared.clone()];
    view.observations[2]["evidenceIds"] = json!([shared["id"], first["id"]]);
    view.observations[3]["evidenceIds"] = json!([shared["id"]]);
    let mut parallel = view.observations[2].clone();
    parallel["id"] = json!(hash(b"parallel weaker-route claim"));
    parallel["resolution"] = json!("syntactic");
    parallel["evidenceIds"] = json!([shared["id"]]);
    view.observations.push(parallel.clone());
    let result = shortest(
        &view,
        &json!({
            "source":"start","target":"end","max_hops":4,"undirected":false,
            "path_policy":"weakest_source_categories_v1"
        }),
        &Deadline::new(1000),
    )
    .unwrap();
    assert_eq!(result["pathSourceCategory"], "inferred");
    let hops = result["pathHops"].as_array().unwrap();
    assert_eq!(hops.len(), 2);
    assert_eq!(
        hops[0]["observations"],
        json!([view.observations[2], parallel])
    );
    assert_eq!(hops[0]["evidence"], json!([first, shared]));
    assert_eq!(hops[1]["evidence"], json!([shared]));
}

// These are explicitly synthetic category-policy cases. They specify route
// enumeration and do not establish public admission of numeric source claims.
fn parallel_node_routes(route_count: usize) -> View {
    let mut view = equal_length_routes();
    view.nodes = ["start", "end"]
        .into_iter()
        .map(|name| json!({"id":hash(name.as_bytes()),"name":name}))
        .collect();
    view.observations.clear();
    for ordinal in 0..route_count {
        let name = format!("branch_{ordinal:03}");
        view.nodes
            .push(json!({"id":hash(name.as_bytes()),"name":name}));
        for copy in 0..2 {
            view.observations.push(json!({
                "id":hash(format!("entry {ordinal} copy {copy}").as_bytes()),
                "source":hash(b"start"),"target":hash(name.as_bytes()),
                "relation":"call","resolution":"syntactic","evidenceIds":[]
            }));
        }
        view.observations.push(json!({
            "id":hash(format!("exit {ordinal}").as_bytes()),
            "source":hash(name.as_bytes()),"target":hash(b"end"),
            "relation":"call","resolution":"syntactic","evidenceIds":[]
        }));
    }
    view
}

#[test]
fn a_weaker_257th_node_route_does_not_displace_the_ranked_256_route_prefix() {
    let mut view = parallel_node_routes(257);
    // Both original copies of the last route are outside the ranked prefix.
    for copy in 0..2 {
        view.observations[256 * 3 + copy]["resolution"] = json!("unmapped_claim");
    }
    let original = serde_json::to_vec(&view).unwrap();
    let args = json!({"source":"start","target":"end","max_hops":2,"undirected":false,"path_policy":"weakest_source_categories_v1"});
    let result = shortest(&view, &args, &Deadline::new(1000)).unwrap();
    assert_eq!(
        result["pathObservationIds"],
        json!([hash(b"entry 0 copy 0"), hash(b"exit 0")]),
        "Only the first 256 distinct shortest node routes are ranked; the 257th only detects the cap"
    );
    assert_eq!(result["pathSourceCategory"], "extracted");
    assert_eq!(result["shortestNodePathsConsidered"], 256);
    assert_eq!(result["shortestNodePathsCapped"], true);
    assert_eq!(result["shortestNodePathLimit"], 256);
    assert_eq!(
        result["pathSelectionScope"],
        "first_256_distinct_shortest_node_paths"
    );
    assert_eq!(serde_json::to_vec(&view).unwrap(), original);
}

#[test]
fn every_parallel_claim_on_an_in_prefix_hop_affects_its_route_without_adding_routes() {
    let mut view = parallel_node_routes(257);
    // The second original copy is weaker. It must affect route 255 even when
    // node-path enumeration coalesces the duplicate neighbor for counting.
    view.observations[255 * 3 + 1]["resolution"] = json!("heuristic");
    for copy in 0..2 {
        view.observations[256 * 3 + copy]["resolution"] = json!("unmapped_claim");
    }
    let args = json!({"source":"start","target":"end","max_hops":2,"undirected":false,"path_policy":"weakest_source_categories_v1"});
    let result = shortest(&view, &args, &Deadline::new(1000)).unwrap();
    assert_eq!(
        result["pathObservationIds"],
        json!([hash(b"entry 255 copy 1"), hash(b"exit 255")]),
        "Parallel evidence is retained and ranked within a hop, never counted as a separate node path"
    );
    assert_eq!(result["pathSourceCategory"], "inferred");
    assert_eq!(
        result["pathHops"][0]["observations"],
        json!([view.observations[255 * 3], view.observations[255 * 3 + 1]])
    );
    assert_eq!(result["shortestNodePathsConsidered"], 256);
    assert_eq!(result["shortestNodePathsCapped"], true);
}

#[test]
fn complete_node_routes_use_predecessor_order_across_multilevel_reconvergence() {
    let mut view = parallel_node_routes(0);
    for name in ["left", "right"] {
        view.nodes
            .push(json!({"id":hash(name.as_bytes()),"name":name}));
        view.observations.push(json!({
            "id":hash(format!("start {name}").as_bytes()),"source":hash(b"start"),
            "target":hash(name.as_bytes()),"relation":"call","resolution":"syntactic","evidenceIds":[]
        }));
    }
    // 128 shared suffixes each admit left/right, plus one final left-only
    // suffix: 257 complete paths but only two first-hop neighbors. Target-
    // predecessor order visits both prefixes of each earlier suffix before
    // the last suffix; forward DFS would incorrectly rank its weak route.
    for ordinal in 0..129 {
        let name = format!("middle_{ordinal:03}");
        view.nodes
            .push(json!({"id":hash(name.as_bytes()),"name":name}));
        for prefix in ["left", "right"] {
            if ordinal == 128 && prefix == "right" {
                continue;
            }
            view.observations.push(json!({
                "id":hash(format!("{prefix} {name}").as_bytes()),"source":hash(prefix.as_bytes()),
                "target":hash(name.as_bytes()),"relation":"call",
                "resolution":if ordinal == 128 {"unmapped_claim"} else {"syntactic"},"evidenceIds":[]
            }));
        }
        view.observations.push(json!({
            "id":hash(format!("{name} end").as_bytes()),"source":hash(name.as_bytes()),
            "target":hash(b"end"),"relation":"call","resolution":"syntactic","evidenceIds":[]
        }));
    }
    let before = serde_json::to_vec(&view).unwrap();
    let result = shortest(
        &view,
        &json!({
            "source":"start","target":"end","max_hops":3,"undirected":false,
            "path_policy":"weakest_source_categories_v1"
        }),
        &Deadline::new(1000),
    )
    .unwrap();
    assert_eq!(result["hops"], 3);
    assert_eq!(
        result["pathObservationIds"],
        json!([
            hash(b"start left"),
            hash(b"left middle_000"),
            hash(b"middle_000 end")
        ]),
        "Count complete node routes and rank the target-predecessor prefix, not first-hop neighbors or forward DFS"
    );
    assert_eq!(result["pathSourceCategory"], "extracted");
    assert_eq!(result["shortestNodePathsConsidered"], 256);
    assert_eq!(result["shortestNodePathsCapped"], true);
    assert_eq!(serde_json::to_vec(&view).unwrap(), before);
}

#[test]
fn node_route_counts_keep_self_unreachable_hop_bounds_cycles_and_reverse_traversal_distinct() {
    let mut view = parallel_node_routes(257);
    view.observations.push(json!({
        "id":hash(b"cycle back to start"),"source":hash(b"branch_000"),"target":hash(b"start"),
        "relation":"call","resolution":"syntactic","evidenceIds":[]
    }));
    view.observations.push(json!({
        "id":hash(b"self cycle"),"source":hash(b"start"),"target":hash(b"start"),
        "relation":"call","resolution":"syntactic","evidenceIds":[]
    }));
    let before = serde_json::to_vec(&view).unwrap();
    let mut args = json!({"source":"start","target":"start","max_hops":2,"undirected":false,"path_policy":"weakest_source_categories_v1"});
    let same = shortest(&view, &args, &Deadline::new(1000)).unwrap();
    assert_eq!(same["found"], true);
    assert_eq!(same["hops"], 0);
    assert_eq!(same["shortestNodePathsConsidered"], 1);
    assert_eq!(same["shortestNodePathsCapped"], false);
    assert_eq!(same["pathSourceCategory"], Value::Null);
    assert_eq!(same["pathHops"], json!([]));
    args["source"] = json!("end");
    let absent = shortest(&view, &args, &Deadline::new(1000)).unwrap();
    assert_eq!(absent["found"], false);
    assert_eq!(absent["shortestNodePathsConsidered"], 0);
    assert_eq!(absent["shortestNodePathsCapped"], false);
    assert_eq!(absent["pathHops"], json!([]));
    args["undirected"] = json!(true);
    args["max_hops"] = json!(1);
    let bounded = shortest(&view, &args, &Deadline::new(1000)).unwrap();
    assert_eq!(bounded["found"], false);
    assert_eq!(bounded["shortestNodePathsConsidered"], 0);
    assert_eq!(bounded["shortestNodePathsCapped"], false);
    args["max_hops"] = json!(2);
    let reverse = shortest(&view, &args, &Deadline::new(1000)).unwrap();
    assert_eq!(reverse["found"], true);
    assert_eq!(reverse["hops"], 2);
    assert_eq!(reverse["shortestNodePathsConsidered"], 256);
    assert_eq!(reverse["shortestNodePathsCapped"], true);
    assert_eq!(
        reverse["pathObservationIds"],
        json!([hash(b"exit 0"), hash(b"entry 0 copy 0")])
    );
    assert_eq!(
        reverse["pathHops"][1]["observations"],
        json!([
            view.observations[0],
            view.observations[1],
            view.observations[257 * 3]
        ])
    );
    assert_eq!(serde_json::to_vec(&view).unwrap(), before);
}

#[test]
fn path_prefix_enumeration_refuses_expired_or_live_cancelled_requests_without_partial_results() {
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };
    let view = parallel_node_routes(257);
    let before = serde_json::to_vec(&view).unwrap();
    let args = json!({"source":"start","target":"end","max_hops":2,"undirected":false,"path_policy":"weakest_source_categories_v1"});
    let expired = shortest(&view, &args, &Deadline::new(0)).unwrap_err();
    assert!(expired.to_string().contains("deadline"), "{expired:#}");
    let cancelled = Arc::new(AtomicBool::new(false));
    let result = Deadline::run_bounded(&Deadline::new(1000), cancelled.clone(), || {
        let inherited = Deadline::new(1000);
        cancelled.store(true, Ordering::SeqCst);
        let inner = shortest(&view, &args, &inherited).expect_err(
            "The inner operation must refuse before the outer owner checks cancellation",
        );
        assert!(inner.to_string().contains("cancelled"), "{inner:#}");
        Ok(())
    });
    let error = result.unwrap_err();
    assert!(error.to_string().contains("cancelled"), "{error:#}");
    assert_eq!(serde_json::to_vec(&view).unwrap(), before);
}
