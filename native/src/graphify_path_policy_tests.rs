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
