use serde_json::{Value, json};
use std::fs;
use tempfile::TempDir;
use yoke::{
    Engine,
    capture::Deadline,
    project_session::{HostProfile, Session},
};
fn fixture() -> (TempDir, Session, Deadline) {
    let root = TempDir::new().unwrap();
    fs::write(
        root.path().join("main.rs"),
        "fn α(){ beta(); beta(); }\nfn beta(){ gamma(); }\nfn gamma(){}\n",
    )
    .unwrap();
    let d = Deadline::new(30000);
    let session = Session::open(
        Engine::open(root.path()).unwrap(),
        HostProfile::default(),
        Some("graphify"),
        &d,
    )
    .unwrap();
    (root, session, d)
}
fn run(s: &mut Session, n: &str, a: Value, d: &Deadline) -> Value {
    yoke::graphify_adapter::invoke(s, n, &a, d).unwrap()
}
#[test]
fn ten_pinned_schemas_six_resources_and_pr_unavailability_are_explicit() {
    let (_root, mut s, d) = fixture();
    let tools = yoke::graphify_adapter::tools();
    assert_eq!(tools.as_array().unwrap().len(), 10);
    for row in tools.as_array().unwrap() {
        assert_eq!(
            row["inputSchema"]["properties"]["project_path"]["type"],
            "string"
        );
    }
    assert_eq!(
        yoke::graphify_adapter::resources()
            .as_array()
            .unwrap()
            .len(),
        6
    );
    assert!(
        yoke::graphify_adapter::invoke(&mut s, "list_prs", &json!({}), &d)
            .unwrap_err()
            .to_string()
            .contains("unavailable")
    );
    for uri in [
        "report",
        "stats",
        "god-nodes",
        "surprises",
        "audit",
        "questions",
    ] {
        let (mime, body) =
            yoke::graphify_adapter::resource(&mut s, &format!("graphify://{uri}"), &d).unwrap();
        assert!(!mime.is_empty());
        assert!(!body.is_empty());
    }
}
#[test]
fn parallel_neighbors_exact_evidence_and_directed_shortest_path_are_preserved() {
    let (_root, mut s, d) = fixture();
    let result = run(
        &mut s,
        "get_neighbors",
        json!({"label":"α","relation_filter":"call"}),
        &d,
    );
    let edges = result["observations"].as_array().unwrap();
    assert_eq!(edges.len(), 2);
    assert_ne!(edges[0]["id"], edges[1]["id"]);
    assert!(!result["evidence"].as_array().unwrap().is_empty());
    let forward = run(
        &mut s,
        "shortest_path",
        json!({"source":"α","target":"gamma"}),
        &d,
    );
    assert_eq!(forward["found"], true);
    assert_eq!(forward["hops"], 2);
    let reverse = run(
        &mut s,
        "shortest_path",
        json!({"source":"gamma","target":"α"}),
        &d,
    );
    assert_eq!(reverse["found"], false);
    let reverse = run(
        &mut s,
        "shortest_path",
        json!({"source":"gamma","target":"α","undirected":true}),
        &d,
    );
    assert_eq!(reverse["found"], true);
}
#[test]
fn filters_depth_and_conservative_token_byte_budget_are_real() {
    let (_root, mut s, d) = fixture();
    for mode in ["bfs", "dfs"] {
        let r = run(
            &mut s,
            "query_graph",
            json!({"question":"α","mode":mode,"depth":1,"context_filter":["call"]}),
            &d,
        );
        assert_eq!(r["nodes"].as_array().unwrap().len(), 2);
        assert!(
            r["observations"]
                .as_array()
                .unwrap()
                .iter()
                .all(|e| e["relation"] == "calls")
        );
    }
    let r = run(
        &mut s,
        "query_graph",
        json!({"question":"α","token_budget":512}),
        &d,
    );
    assert!(serde_json::to_vec(&r).unwrap().len() <= 512 * 3);
    assert!(r["truncated"].as_bool().unwrap());
    assert!(
        yoke::graphify_adapter::invoke(
            &mut s,
            "query_graph",
            &json!({"question":"α","depth":7}),
            &d
        )
        .is_err()
    );
    assert!(
        yoke::graphify_adapter::invoke(
            &mut s,
            "query_graph",
            &json!({"question":"α","context_filter":["imaginary"]}),
            &d
        )
        .is_err()
    );
}
#[test]
fn foreign_roots_unknown_arguments_and_ambiguous_names_fail_closed() {
    let (root, mut s, d) = fixture();
    let foreign = TempDir::new().unwrap();
    assert!(
        yoke::graphify_adapter::invoke(
            &mut s,
            "graph_stats",
            &json!({"project_path":foreign.path().to_str().unwrap()}),
            &d
        )
        .is_err()
    );
    assert!(
        yoke::graphify_adapter::invoke(
            &mut s,
            "get_node",
            &json!({"label":"α","execute":true}),
            &d
        )
        .is_err()
    );
    fs::write(root.path().join("other.rs"), "fn beta(){}\n").unwrap();
    s.active_engine_at(&d).unwrap().index(&d).unwrap();
    let r = run(&mut s, "get_node", json!({"label":"beta"}), &d);
    assert_eq!(r["ambiguous"], true);
    assert!(r["candidates"].as_array().unwrap().len() >= 2);
}
#[test]
fn community_and_ambiguous_neighbor_responses_honor_the_requested_budget() {
    let (root, mut s, d) = fixture();
    fs::write(root.path().join("other.rs"), "fn beta(){}\n").unwrap();
    s.active_engine_at(&d).unwrap().index(&d).unwrap();
    let neighbors = run(
        &mut s,
        "get_neighbors",
        json!({"label":"beta","token_budget":512}),
        &d,
    );
    assert!(
        serde_json::to_vec(&neighbors).unwrap().len() <= 1536,
        "Ambiguity response ignored token_budget"
    );
    fs::write(
        root.path().join("many.rs"),
        (0..100)
            .map(|i| format!("fn member_{i}(){{}}\n"))
            .collect::<String>(),
    )
    .unwrap();
    s.active_engine_at(&d).unwrap().index(&d).unwrap();
    let community = run(
        &mut s,
        "get_community",
        json!({"community_id":0,"token_budget":512}),
        &d,
    );
    assert_eq!(community["found"], true);
    assert!(serde_json::to_vec(&community).unwrap().len() <= 1536);
}
#[test]
fn question_context_inference_is_labeled_and_explicit_filter_takes_precedence() {
    let (_root, mut s, d) = fixture();
    let inferred = run(
        &mut s,
        "query_graph",
        json!({"question":"what calls beta?","token_budget":65536}),
        &d,
    );
    assert_eq!(inferred["contextFilter"], json!(["call"]));
    assert_eq!(inferred["contextFilterSource"], "heuristic");
    assert!(
        inferred["observations"]
            .as_array()
            .unwrap()
            .iter()
            .all(|e| e["relation"] == "calls")
    );
    let explicit = run(
        &mut s,
        "query_graph",
        json!({"question":"what calls beta?","context_filter":["contains"],"token_budget":65536}),
        &d,
    );
    assert_eq!(explicit["contextFilter"], json!(["contains"]));
    assert_eq!(explicit["contextFilterSource"], "explicit");
}
#[test]
fn shortest_path_bounds_are_consistent_for_missing_and_ambiguous_endpoints() {
    let (root, mut s, d) = fixture();
    fs::write(root.path().join("other.rs"), "fn beta(){}\n").unwrap();
    s.active_engine_at(&d).unwrap().index(&d).unwrap();
    for source in ["missing", "beta", "α"] {
        for max in [0, 65] {
            assert!(
                yoke::graphify_adapter::invoke(
                    &mut s,
                    "shortest_path",
                    &json!({"source":source,"target":"gamma","max_hops":max}),
                    &d
                )
                .is_err()
            );
        }
    }
}
