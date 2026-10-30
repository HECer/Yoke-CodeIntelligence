use serde_json::json;
use std::{collections::BTreeMap, fs};
use yoke::{Engine, capture::Deadline, model::identity, neo4j_publish};
fn fixture() -> (
    tempfile::TempDir,
    Engine,
    yoke::model::Snapshot,
    yoke::model::Graph,
) {
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join("owned.rs"),
        "fn alpha(){beta();beta();}\nfn beta(){alpha();}\n",
    )
    .unwrap();
    let mut engine = Engine::open(dir.path()).unwrap();
    let (snapshot, graph, _, _) = engine.index(&Deadline::new(30000)).unwrap();
    (dir, engine, snapshot, graph)
}
#[test]
fn altered_graph_labels_or_an_unrelated_source_store_cannot_be_published_as_verified() {
    let (_dir, engine, snapshot, graph) = fixture();
    let mut changed = graph.clone();
    changed.observations[0].resolution = "observed_execution".into();
    let d = Deadline::new(30000);
    assert!(
        neo4j_publish::build_payload(&engine.store, &snapshot, &changed, &Default::default(), &d)
            .is_err()
    );
    let other = tempfile::tempdir().unwrap();
    let unrelated = Engine::open(other.path()).unwrap();
    assert!(
        neo4j_publish::build_payload(&unrelated.store, &snapshot, &graph, &Default::default(), &d)
            .is_err()
    );
}
#[test]
fn exact_serialized_capacity_is_admitted_and_lower_budgets_publish_nothing() {
    let (_dir, engine, snapshot, graph) = fixture();
    let d = Deadline::new(30000);
    let before = engine.store.db.total_changes();
    let original =
        neo4j_publish::build_payload(&engine.store, &snapshot, &graph, &Default::default(), &d)
            .unwrap();
    let size = serde_json::to_vec(&original).unwrap().len();
    let mut limits = neo4j_publish::Limits {
        max_bytes: size,
        ..Default::default()
    };
    assert_eq!(
        neo4j_publish::build_payload(&engine.store, &snapshot, &graph, &limits, &d).unwrap(),
        original
    );
    limits.max_bytes = size - 1;
    assert!(neo4j_publish::build_payload(&engine.store, &snapshot, &graph, &limits, &d).is_err());
    limits = Default::default();
    limits.max_nodes = graph.entities.len() - 1;
    assert!(neo4j_publish::build_payload(&engine.store, &snapshot, &graph, &limits, &d).is_err());
    limits = Default::default();
    limits.max_observations = graph.observations.len() - 1;
    assert!(neo4j_publish::build_payload(&engine.store, &snapshot, &graph, &limits, &d).is_err());
    limits = Default::default();
    limits.max_evidence = graph.evidence.len() - 1;
    assert!(neo4j_publish::build_payload(&engine.store, &snapshot, &graph, &limits, &d).is_err());
    assert_eq!(engine.store.db.total_changes(), before);
}
#[test]
fn closed_limits_and_expired_requests_cannot_grant_network_or_source_authority() {
    let (_dir, engine, snapshot, graph) = fixture();
    assert!(serde_json::from_value::<neo4j_publish::Limits>(json!({"allowNetwork":true})).is_err());
    assert!(
        neo4j_publish::build_payload(
            &engine.store,
            &snapshot,
            &graph,
            &Default::default(),
            &Deadline::new(0)
        )
        .is_err()
    );
    let limits = neo4j_publish::Limits {
        max_bytes: 16_000_001,
        ..Default::default()
    };
    assert!(
        neo4j_publish::build_payload(
            &engine.store,
            &snapshot,
            &graph,
            &limits,
            &Deadline::new(30000)
        )
        .is_err()
    );
}
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
