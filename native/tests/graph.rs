use std::fs;
use tempfile::TempDir;
use yoke::{Engine, capture::Deadline};
#[test]
fn community_partition_separates_dense_call_groups_despite_a_bridge() {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("main.rs"),"fn a(){b();c();} fn b(){a();c();} fn c(){a();b();d();d();d();} fn d(){e();f();} fn e(){d();f();} fn f(){d();e();}").unwrap();
    let mut engine = Engine::open(dir.path()).unwrap();
    let (_, mut graph, _, _) = engine.index(&Deadline::new(10000)).unwrap();
    graph.entities.retain(|e| e.kind == "function");
    graph.observations.retain(|e| e.relation == "calls");
    let original = serde_json::to_string(&graph.observations).unwrap();
    let output = yoke::query::analytics(&graph, &Deadline::new(10000)).unwrap();
    let mut communities: Vec<Vec<String>> = output["communities"]
        .as_array()
        .unwrap()
        .iter()
        .map(|community| {
            let mut names: Vec<_> = community["members"]
                .as_array()
                .unwrap()
                .iter()
                .map(|id| {
                    graph
                        .entities
                        .iter()
                        .find(|e| e.id == *id)
                        .unwrap()
                        .name
                        .clone()
                })
                .collect();
            names.sort();
            names
        })
        .collect();
    communities.sort();
    assert_eq!(communities, vec![vec!["a", "b", "c"], vec!["d", "e", "f"]]);
    assert_eq!(
        serde_json::to_string(&graph.observations).unwrap(),
        original
    );
    assert_eq!(output["communityAlgorithm"], "leiden");
}
