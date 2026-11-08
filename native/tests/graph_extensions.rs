use std::fs;
use tempfile::TempDir;
use yoke::{
    Engine,
    capture::Deadline,
    graph_extensions::{self, AnalysisOptions},
    model::{Graph, Snapshot, identity},
};

fn fixture() -> (TempDir, Engine, Snapshot, Graph) {
    let dir = TempDir::new().unwrap();
    fs::create_dir(dir.path().join("src")).unwrap();
    fs::write(
        dir.path().join("src/a.rs"),
        "fn alpha(){beta();beta();gamma();missing();}\nfn beta(){alpha();}\n",
    )
    .unwrap();
    fs::write(
        dir.path().join("src/b.rs"),
        "fn gamma(){delta();}\nfn delta(){gamma();}\n",
    )
    .unwrap();
    let mut engine = Engine::open(dir.path()).unwrap();
    let (snapshot, graph, _, _) = engine.index(&Deadline::new(10000)).unwrap();
    (dir, engine, snapshot, graph)
}

#[test]
fn report_keeps_parallel_arcs_exact_anchors_and_uncertainty() {
    let (_dir, engine, snapshot, graph) = fixture();
    let before = identity(&graph);
    let report = graph_extensions::analyze(
        &engine.store,
        &snapshot,
        &graph,
        &AnalysisOptions::default(),
        &Deadline::new(10000),
    )
    .unwrap();
    assert_eq!(report["snapshotId"], snapshot.id);
    assert_eq!(report["graphHash"], before);
    assert_eq!(
        report["audit"]["totalObservations"],
        graph.observations.len()
    );
    assert!(
        report["audit"]["byResolution"]["heuristic"]
            .as_u64()
            .unwrap()
            > 0
    );
    assert!(
        report["audit"]["byResolution"]["unresolved"]
            .as_u64()
            .unwrap()
            > 0
    );
    let alpha = graph.entities.iter().find(|e| e.name == "alpha").unwrap();
    let hub = report["godNodes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["entityId"] == alpha.id)
        .unwrap();
    let expected = graph
        .observations
        .iter()
        .filter(|o| {
            o.target.is_some()
                && (o.source == alpha.id || o.target.as_deref() == Some(alpha.id.as_str()))
        })
        .map(|o| {
            usize::from(o.source == alpha.id)
                + usize::from(o.target.as_deref() == Some(alpha.id.as_str()))
        })
        .sum::<usize>();
    assert_eq!(hub["degree"], expected);
    assert_eq!(hub["anchor"]["path"], alpha.path);
    assert_eq!(hub["anchor"]["startByte"], alpha.start_byte);
    assert_eq!(report["complete"], false);
    assert_eq!(report["modelCalls"], 0);
    assert_eq!(identity(&engine.store.graph(&snapshot.id).unwrap()), before);
    assert!(
        report["surprises"]
            .as_array()
            .unwrap()
            .iter()
            .any(|s| s["reason"] == "cross_file_candidate")
    );
    assert!(
        report["questions"]
            .as_array()
            .unwrap()
            .iter()
            .any(|s| s["kind"] == "unresolved")
    );
    let artifact = engine
        .store
        .read_artifact(report["artifactId"].as_str().unwrap())
        .unwrap();
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&artifact).unwrap()["graphHash"],
        before
    );
    let markdown = String::from_utf8(
        engine
            .store
            .read_artifact(report["markdownArtifactId"].as_str().unwrap())
            .unwrap(),
    )
    .unwrap();
    assert!(markdown.contains("src/a.rs"));
    assert!(markdown.contains("not runtime or compiler proof"));
}

#[test]
fn report_is_reproducible_with_actual_leiden_and_source_bound_labels() {
    let (_dir, engine, snapshot, graph) = fixture();
    let options = AnalysisOptions {
        resolution: 2.5,
        max_community_size: 3,
        ..Default::default()
    };
    let a = graph_extensions::analyze(
        &engine.store,
        &snapshot,
        &graph,
        &options,
        &Deadline::new(10000),
    )
    .unwrap();
    let b = graph_extensions::analyze(
        &engine.store,
        &snapshot,
        &graph,
        &options,
        &Deadline::new(10000),
    )
    .unwrap();
    assert_eq!(a, b);
    assert_eq!(a["clustering"]["algorithm"], "leiden");
    assert_eq!(a["clustering"]["resolution"], 2.5);
    let communities = a["communities"].as_array().unwrap();
    assert!(!communities.is_empty());
    let mut seen = std::collections::BTreeSet::new();
    for c in communities {
        assert!(!c["label"].as_str().unwrap().is_empty());
        assert!(c["cohesion"].as_f64().unwrap() >= 0.0 && c["cohesion"].as_f64().unwrap() <= 1.0);
        for member in c["members"].as_array().unwrap() {
            assert!(seen.insert(member.as_str().unwrap().to_owned()));
        }
    }
    assert_eq!(seen.len(), graph.entities.len());
}

#[test]
fn snapshot_diff_ignores_offset_and_id_churn_but_detects_body_change() {
    let (dir, mut engine, before, before_graph) = fixture();
    fs::write(
        dir.path().join("src/a.rs"),
        "// inserted line\nfn alpha(){beta();beta();gamma();missing();}\nfn beta(){alpha();}\n",
    )
    .unwrap();
    let (after, after_graph, _, _) = engine.index(&Deadline::new(10000)).unwrap();
    let diff = graph_extensions::compare(
        &engine.store,
        &before,
        &before_graph,
        &after,
        &after_graph,
        &Deadline::new(10000),
    )
    .unwrap();
    assert!(diff["addedEntities"].as_array().unwrap().is_empty());
    assert!(diff["removedEntities"].as_array().unwrap().is_empty());
    assert!(
        diff["modifiedEntities"]
            .as_array()
            .unwrap()
            .iter()
            .all(|v| v["kind"] == "file")
    );
    assert!(
        diff["relocatedEntities"]
            .as_array()
            .unwrap()
            .iter()
            .any(|v| v["name"] == "alpha")
    );
    assert!(diff["addedObservations"].as_array().unwrap().is_empty());
    assert!(diff["removedObservations"].as_array().unwrap().is_empty());
    fs::write(
        dir.path().join("src/a.rs"),
        "// inserted line\nfn alpha(){beta();}\nfn beta(){alpha();}\n",
    )
    .unwrap();
    let (changed, changed_graph, _, _) = engine.index(&Deadline::new(10000)).unwrap();
    let options = AnalysisOptions {
        previous_snapshot_id: Some(after.id.clone()),
        ..Default::default()
    };
    let report = graph_extensions::analyze(
        &engine.store,
        &changed,
        &changed_graph,
        &options,
        &Deadline::new(10000),
    )
    .unwrap();
    assert!(
        report["diff"]["modifiedEntities"]
            .as_array()
            .unwrap()
            .iter()
            .any(|v| v["name"] == "alpha")
    );
    assert!(
        !report["diff"]["removedObservations"]
            .as_array()
            .unwrap()
            .is_empty()
    );
}

#[test]
fn expired_and_rebound_or_corrupt_source_reports_are_rejected() {
    let (_dir, engine, snapshot, graph) = fixture();
    let count = engine
        .store
        .records::<serde_json::Value>(graph_extensions::REPORT_KIND)
        .unwrap()
        .len();
    assert!(
        graph_extensions::analyze(
            &engine.store,
            &snapshot,
            &graph,
            &AnalysisOptions::default(),
            &Deadline::new(0)
        )
        .is_err()
    );
    let mut forged = graph.clone();
    forged.entities[0].name = "forged".into();
    assert!(
        graph_extensions::analyze(
            &engine.store,
            &snapshot,
            &forged,
            &AnalysisOptions::default(),
            &Deadline::new(10000)
        )
        .is_err()
    );
    let path = engine.store.dir.join("blobs").join(&snapshot.files[0].hash);
    fs::write(path, b"corrupt").unwrap();
    assert!(
        graph_extensions::analyze(
            &engine.store,
            &snapshot,
            &graph,
            &AnalysisOptions::default(),
            &Deadline::new(10000)
        )
        .is_err()
    );
    assert_eq!(
        engine
            .store
            .records::<serde_json::Value>(graph_extensions::REPORT_KIND)
            .unwrap()
            .len(),
        count
    );
}

#[test]
fn empty_graph_and_hub_exclusion_remain_explicit() {
    let dir = TempDir::new().unwrap();
    let mut engine = Engine::open(dir.path()).unwrap();
    let (s, g, _, _) = engine.index(&Deadline::new(10000)).unwrap();
    let empty = graph_extensions::analyze(
        &engine.store,
        &s,
        &g,
        &AnalysisOptions::default(),
        &Deadline::new(10000),
    )
    .unwrap();
    assert!(empty["communities"].as_array().unwrap().is_empty());
    assert_eq!(empty["complete"], false);
    let (_dir, engine, s, g) = fixture();
    let options = AnalysisOptions {
        exclude_hubs_percentile: Some(0.0),
        uncertain_weight: 0.0,
        ..Default::default()
    };
    let report =
        graph_extensions::analyze(&engine.store, &s, &g, &options, &Deadline::new(10000)).unwrap();
    assert!(
        !report["clustering"]["excludedHubs"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert_eq!(report["clustering"]["uncertainWeight"], 0.0);
}

#[test]
fn invalid_analysis_options_fail_without_persisting() {
    let (_dir, engine, s, g) = fixture();
    for options in [
        AnalysisOptions {
            resolution: f64::NAN,
            ..Default::default()
        },
        AnalysisOptions {
            resolution: -1.0,
            ..Default::default()
        },
        AnalysisOptions {
            uncertain_weight: 1.1,
            ..Default::default()
        },
        AnalysisOptions {
            exclude_hubs_percentile: Some(101.0),
            ..Default::default()
        },
        AnalysisOptions {
            max_iterations: 0,
            ..Default::default()
        },
        AnalysisOptions {
            hub_limit: 101,
            ..Default::default()
        },
    ] {
        assert!(
            graph_extensions::analyze(&engine.store, &s, &g, &options, &Deadline::new(10000))
                .is_err()
        );
    }
    assert!(
        engine
            .store
            .records::<serde_json::Value>(graph_extensions::REPORT_KIND)
            .unwrap()
            .is_empty()
    );
}

#[test]
fn persisted_wrong_line_anchors_are_not_certified_as_exact() {
    let (_dir, engine, s, mut g) = fixture();
    g.entities[0].start_line = 99;
    g.entities[0].end_line = 100;
    engine
        .store
        .db
        .execute(
            "UPDATE graphs SET value=?1 WHERE id=?2",
            rusqlite::params![serde_json::to_string(&g).unwrap(), s.id],
        )
        .unwrap();
    assert!(
        graph_extensions::analyze(
            &engine.store,
            &s,
            &g,
            &AnalysisOptions::default(),
            &Deadline::new(10000)
        )
        .is_err(),
        "Stored graph equality is insufficient when exact lines disagree with captured bytes"
    );
    assert!(
        engine
            .store
            .records::<serde_json::Value>(graph_extensions::REPORT_KIND)
            .unwrap()
            .is_empty()
    );
}

#[test]
fn duplicate_structural_identities_are_reported_ambiguous() {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("main.rs"), "fn same(){}\nfn same(){}\n").unwrap();
    let mut engine = Engine::open(dir.path()).unwrap();
    let (before, gb, _, _) = engine.index(&Deadline::new(10000)).unwrap();
    fs::write(
        dir.path().join("main.rs"),
        "// shift\nfn same(){}\nfn same(){}\n",
    )
    .unwrap();
    let (after, ga, _, _) = engine.index(&Deadline::new(10000)).unwrap();
    let diff = graph_extensions::compare(
        &engine.store,
        &before,
        &gb,
        &after,
        &ga,
        &Deadline::new(10000),
    )
    .unwrap();
    assert_eq!(diff["ambiguousEntityGroups"].as_array().unwrap().len(), 1);
    assert_eq!(
        diff["ambiguousEntityGroups"][0]["before"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    assert!(diff["addedEntities"].as_array().unwrap().is_empty());
    assert!(diff["removedEntities"].as_array().unwrap().is_empty());
}

#[test]
fn ambiguous_members_cannot_collide_community_identities() {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("main.rs"), "fn same(){}\nfn same(){}\n").unwrap();
    let mut engine = Engine::open(dir.path()).unwrap();
    let (s, g, _, _) = engine.index(&Deadline::new(10000)).unwrap();
    let options = AnalysisOptions {
        resolution: 100.0,
        max_community_size: 1,
        ..Default::default()
    };
    let report =
        graph_extensions::analyze(&engine.store, &s, &g, &options, &Deadline::new(10000)).unwrap();
    let communities = report["communities"].as_array().unwrap();
    let ids: std::collections::BTreeSet<_> = communities
        .iter()
        .map(|c| c["id"].as_str().unwrap())
        .collect();
    assert_eq!(
        ids.len(),
        communities.len(),
        "Ambiguous structural labels cannot make two distinct source groups share an ID"
    );
    assert!(
        communities
            .iter()
            .any(|c| c["identityMethod"] == "ambiguous_members_snapshot_scoped")
    );
}
