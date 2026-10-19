use serde_json::json;
use std::{collections::BTreeMap, fs};
use yoke::{Engine, capture::Deadline, model::identity, neo4j_publish};
#[test]
fn verified_payload_preserves_parallel_directions_exact_evidence_and_original_source_bytes() {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir(dir.path().join("src")).unwrap();
    let a = "// Owned \u{1f4a1} evidence\r\nfn alpha(){beta();beta();gamma();missing();}\r\nfn beta(){alpha();}\r\n";
    fs::write(dir.path().join("src/a.rs"), a).unwrap();
    fs::write(
        dir.path().join("src/b.rs"),
        "fn gamma(){delta();}\nfn delta(){gamma();}\n",
    )
    .unwrap();
    let mut engine = Engine::open(dir.path()).unwrap();
    let d = Deadline::new(30000);
    let (snapshot, graph, _, _) = engine.index(&d).unwrap();
    let original = identity(&graph);
    let mut parallel = BTreeMap::new();
    for obs in &graph.observations {
        *parallel
            .entry((&obs.source, &obs.target, &obs.relation))
            .or_insert(0) += 1;
    }
    assert!(parallel.values().any(|n| *n >= 2));
    let payload = neo4j_publish::build_payload(
        &engine.store,
        &snapshot,
        &graph,
        &neo4j_publish::Limits::default(),
        &d,
    )
    .unwrap();
    assert_eq!(payload["entities"], json!(graph.entities));
    assert_eq!(payload["observations"], json!(graph.observations));
    assert_eq!(payload["evidence"], json!(graph.evidence));
    assert_eq!(payload["snapshotId"], snapshot.id);
    assert_eq!(payload["graphHash"], original);
    assert_eq!(payload["sourceAuthority"], false);
    assert_eq!(payload["complete"], false);
    assert_eq!(payload["networkConnections"], 0);
    assert_eq!(payload["modelCalls"], 0);
    assert_eq!(identity(&graph), original);
    assert_eq!(fs::read(dir.path().join("src/a.rs")).unwrap(), a.as_bytes());
    for evidence in &graph.evidence {
        let bytes = fs::read(dir.path().join(&evidence.path)).unwrap();
        assert_eq!(evidence.content_hash, yoke::model::hash(&bytes));
        assert!(evidence.start_byte <= evidence.end_byte && evidence.end_byte <= bytes.len());
    }
}
