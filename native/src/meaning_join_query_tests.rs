use super::*;
use serde_json::json;

// A deliberately asymmetric checked-capability fixture isolates traversal from
// replay, which the CLI integration tests exercise with actual stored receipts.
fn fixture() -> (JoinedGraph, Vec<ConceptRef>) {
    let graph_id = crate::model::hash("query-only-unit-fixture");
    let references: Vec<_> = (0..4)
        .map(|n| ConceptRef {
            graph_id: graph_id.clone(),
            packet_index: 0,
            concept_id: crate::model::hash(n.to_string()),
        })
        .collect();
    let nodes: Vec<_> = references
        .iter()
        .enumerate()
        .map(|(n, r)| json!({"id":n.to_string(),"members":[{"reference":r}]}))
        .collect();
    let relations = vec![
        json!({"source":"0","target":"1","id":"parallel-a"}),
        json!({"source":"0","target":"1","id":"parallel-b"}),
        json!({"source":"1","target":"2","id":"chain"}),
        json!({"source":"0","target":"0","id":"selfloop"}),
    ];
    let hyperedges = vec![
        json!({"participants":[{"nodeId":"0","role":"input"},{"nodeId":"3","role":"output"}]}),
    ];
    (
        JoinedGraph {
            value: json!({"complete":false,"nodes":nodes,"relations":relations,"hyperedges":hyperedges}),
            graph_ids: vec![graph_id],
        },
        references,
    )
}
fn trace(g: &JoinedGraph, r: &ConceptRef, direction: Direction, depth: usize) -> Value {
    g.query(
        &Query::Trace {
            roots: vec![r.clone()],
            direction,
            depth,
            max_visited: 4,
        },
        MAX_OUTPUT,
        &Deadline::new(10000),
    )
    .unwrap()
}
#[test]
fn asymmetric_directions_and_depth_keep_exact_induced_parallel_observations() {
    let (g, r) = fixture();
    let outgoing = trace(&g, &r[1], Direction::Outgoing, 1);
    assert_eq!(
        outgoing["nodes"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v["id"].as_str().unwrap())
            .collect::<Vec<_>>(),
        vec!["1", "2"]
    );
    assert_eq!(
        outgoing["relations"],
        json!([{"source":"1","target":"2","id":"chain"}])
    );
    let incoming = trace(&g, &r[1], Direction::Incoming, 1);
    assert_eq!(
        incoming["nodes"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v["id"].as_str().unwrap())
            .collect::<Vec<_>>(),
        vec!["0", "1"]
    );
    assert_eq!(
        incoming["relations"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v["id"].as_str().unwrap())
            .collect::<Vec<_>>(),
        vec!["parallel-a", "parallel-b", "selfloop"]
    );
    let both = trace(&g, &r[1], Direction::Both, 1);
    assert_eq!(both["nodes"].as_array().unwrap().len(), 3);
    assert_eq!(both["relations"], g.as_value()["relations"]);
    let zero = trace(&g, &r[0], Direction::Both, 0);
    assert_eq!(zero["nodes"].as_array().unwrap().len(), 1);
    assert_eq!(
        zero["relations"],
        json!([{"source":"0","target":"0","id":"selfloop"}])
    );
}
#[test]
fn hyperedges_never_create_directed_steps_or_return_partial_role_slots() {
    let (g, r) = fixture();
    for direction in [Direction::Outgoing, Direction::Incoming, Direction::Both] {
        let result = trace(&g, &r[3], direction, 8);
        assert_eq!(result["nodes"].as_array().unwrap().len(), 1);
        assert_eq!(result["relations"], json!([]));
        assert_eq!(result["hyperedges"], json!([]));
    }
    let both = g
        .query(
            &Query::Trace {
                roots: vec![r[0].clone(), r[3].clone()],
                direction: Direction::Both,
                depth: 0,
                max_visited: 2,
            },
            MAX_OUTPUT,
            &Deadline::new(10000),
        )
        .unwrap();
    assert_eq!(both["hyperedges"], g.as_value()["hyperedges"]);
    assert_eq!(both["relations"].as_array().unwrap().len(), 1);
}
#[test]
fn trace_capacity_refuses_before_clipping_and_preserves_the_checked_value() {
    let (g, r) = fixture();
    let before = g.as_value().clone();
    let query = Query::Trace {
        roots: vec![r[0].clone()],
        direction: Direction::Outgoing,
        depth: 2,
        max_visited: 2,
    };
    assert_eq!(
        g.query(&query, MAX_OUTPUT, &Deadline::new(10000))
            .unwrap_err()
            .to_string(),
        "meaning_join_capacity_exceeded"
    );
    assert_eq!(g.as_value(), &before);
    assert_eq!(
        trace(&g, &r[0], Direction::Outgoing, 2)["nodes"]
            .as_array()
            .unwrap()
            .len(),
        3
    );
}
