use serde_json::json;
use std::fs;
use tempfile::TempDir;
use yoke::{Engine, capture::Deadline};
#[test]
fn graph_ranking_retrieves_a_source_connected_neighbor_without_lexical_match() {
    let root = TempDir::new().unwrap();
    fs::write(
        root.path().join("main.rs"),
        "fn request(){ let payment=1; helper(); }\nfn helper(){ let unrelated=2; }\n",
    )
    .unwrap();
    let mut engine = Engine::open(root.path()).unwrap();
    let deadline = Deadline::new(20000);
    let (snapshot, graph, _, _) = engine.index(&deadline).unwrap();
    let lexical = yoke::retrieval::find(
        &engine.store,
        &snapshot,
        &graph,
        &json!({"query":"payment","graphWeight":0.0,"perFileLimit":10}),
        &deadline,
    )
    .unwrap();
    assert_eq!(lexical["items"].as_array().unwrap().len(), 1, "{lexical}");
    let graph_ranked = yoke::retrieval::find(
        &engine.store,
        &snapshot,
        &graph,
        &json!({"query":"payment","graphWeight":0.5,"perFileLimit":10}),
        &deadline,
    )
    .unwrap();
    let neighbor = graph_ranked["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["name"] == "helper")
        .unwrap();
    assert_eq!(neighbor["bm25Score"], 0.0);
    assert!(neighbor["graphScore"].as_f64().unwrap() > 0.0);
    assert!(graph_ranked["graphIterations"].as_u64().unwrap() > 0);
}
#[test]
fn task_retrieval_ranks_source_bodies_and_balances_files_with_exact_anchors() {
    let root = TempDir::new().unwrap();
    fs::write(
        root.path().join("dense.rs"),
        "fn first(){let payment=1;}\nfn second(){let payment=2;}\nfn third(){let payment=3;}\n",
    )
    .unwrap();
    fs::write(
        root.path().join("other.py"),
        "def process():\n    payment = 4\n    return payment\n",
    )
    .unwrap();
    let mut e = Engine::open(root.path()).unwrap();
    let d = Deadline::new(20000);
    let (s, g, _, _) = e.index(&d).unwrap();
    let result = yoke::retrieval::find(
        &e.store,
        &s,
        &g,
        &json!({"query":"payment","limit":2,"perFileLimit":1,"includeBody":true}),
        &d,
    )
    .unwrap();
    let items = result["items"].as_array().unwrap();
    assert_eq!(items.len(), 2, "{result}");
    assert_ne!(items[0]["path"], items[1]["path"]);
    for item in items {
        let path = item["path"].as_str().unwrap();
        let (text, hash) = yoke::query::source(&e.store, &s, path).unwrap();
        assert_eq!(item["contentHash"], hash);
        assert_eq!(
            item["body"],
            text.get(
                item["startByte"].as_u64().unwrap() as usize
                    ..item["bodyEndByte"].as_u64().unwrap() as usize
            )
            .unwrap()
        );
        assert!(item["bm25Score"].as_f64().unwrap() > 0.0);
    }
    assert_eq!(result["ranking"], "source_body_bm25_personalized_graph");
}
#[test]
fn retrieval_rejects_foreign_graphs_and_unbounded_or_unknown_options() {
    let root = TempDir::new().unwrap();
    fs::write(root.path().join("main.rs"), "fn payment() {}\n").unwrap();
    let mut e = Engine::open(root.path()).unwrap();
    let d = Deadline::new(20000);
    let (s, g, _, _) = e.index(&d).unwrap();
    let mut wrong = g.clone();
    wrong.entities[0].name = "forged".into();
    assert!(yoke::retrieval::find(&e.store, &s, &wrong, &json!({"query":"payment"}), &d).is_err());
    for args in [
        json!({"query":"payment","perFileLimit":0}),
        json!({"query":"payment","graphWeight":2}),
        json!({"query":"payment","execute":true}),
    ] {
        assert!(yoke::retrieval::find(&e.store, &s, &g, &args, &d).is_err());
    }
}
