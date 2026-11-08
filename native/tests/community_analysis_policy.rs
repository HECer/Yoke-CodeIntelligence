use serde_json::{Value, json};
use std::{collections::BTreeSet, fs};
use yoke::{
    Engine,
    capture::Deadline,
    graph_extensions::{self, AnalysisOptions, CommunityPolicy, SourceCategoryPolicy},
    model::{Graph, Snapshot, identity},
};

fn fixture() -> (tempfile::TempDir, Engine, Snapshot, Graph) {
    let root = tempfile::tempdir().unwrap();
    fs::create_dir(root.path().join("src")).unwrap();
    fs::write(
        root.path().join("src/a.rs"),
        "// owned \u{1f30d}\r\nfn alpha(){beta();beta();}\r\nfn local(){}\r\n",
    )
    .unwrap();
    fs::write(root.path().join("src/b.rs"), "fn beta(){alpha();}\n").unwrap();
    let mut engine = Engine::open(root.path()).unwrap();
    let (snapshot, graph, _, _) = engine.index(&Deadline::new(30000)).unwrap();
    (root, engine, snapshot, graph)
}
fn options() -> AnalysisOptions {
    AnalysisOptions {
        community_policy: Some(CommunityPolicy::NeighborMajorityV1 {}),
        ..Default::default()
    }
}

fn stable_memberships(report: &Value) -> BTreeSet<Vec<String>> {
    report["communities"]
        .as_array()
        .unwrap()
        .iter()
        .map(|community| {
            let mut keys: Vec<_> = community["stableMemberKeys"]
                .as_array()
                .unwrap()
                .iter()
                .map(|key| key.as_str().unwrap().to_owned())
                .collect();
            keys.sort();
            keys
        })
        .collect()
}

#[test]
fn location_churn_changes_source_bindings_without_randomizing_the_community_partition() {
    let (root, mut engine, first, first_graph) = fixture();
    let d = Deadline::new(30000);
    let selected = AnalysisOptions {
        exclude_hubs_percentile: Some(50.0),
        ..options()
    };
    let before =
        graph_extensions::analyze(&engine.store, &first, &first_graph, &selected, &d).unwrap();
    let path = root.path().join("src/a.rs");
    let mut shifted = "// location shift \u{1f30d}\r\n".as_bytes().to_vec();
    shifted.extend_from_slice(&fs::read(&path).unwrap());
    fs::write(&path, shifted).unwrap();
    let (snapshot, graph, _, _) = engine.index(&d).unwrap();
    let selected = AnalysisOptions {
        previous_snapshot_id: Some(first.id.clone()),
        ..selected
    };
    let after = graph_extensions::analyze(&engine.store, &snapshot, &graph, &selected, &d).unwrap();
    assert_eq!(stable_memberships(&before), stable_memberships(&after));
    assert_ne!(before["graphHash"], after["graphHash"]);
    assert_ne!(before["sourceBindings"], after["sourceBindings"]);
    assert_eq!(
        after["clustering"]["communityPolicy"]["nodeOrdering"],
        "native_source_path_qualified_name_kind_start_byte"
    );
}

#[test]
fn explicit_community_mode_preserves_original_graph_and_both_legacy_report_families() {
    for source_categories in [false, true] {
        let (root, engine, snapshot, graph) = fixture();
        let d = Deadline::new(30000);
        let source = fs::read(root.path().join("src/a.rs")).unwrap();
        let original_graph = identity(&graph);
        let old_options = AnalysisOptions {
            source_category_policy: source_categories.then(SourceCategoryPolicy::default),
            ..Default::default()
        };
        let before =
            graph_extensions::analyze(&engine.store, &snapshot, &graph, &old_options, &d).unwrap();
        assert_eq!(
            before["producer"],
            if source_categories {
                "yoke-source-category-report-1"
            } else {
                "yoke-graph-report-v1"
            }
        );
        assert!(before["options"].get("communityPolicy").is_none());
        assert!(before["clustering"].get("communityPolicy").is_none());
        let selected = AnalysisOptions {
            community_policy: Some(CommunityPolicy::NeighborMajorityV1 {}),
            ..old_options.clone()
        };
        let report =
            graph_extensions::analyze(&engine.store, &snapshot, &graph, &selected, &d).unwrap();
        assert_eq!(report["producer"], "yoke-community-policy-report-1");
        assert_eq!(
            report["options"]["communityPolicy"],
            json!({"kind":"neighbor_majority_v1"})
        );
        assert_eq!(
            report["clustering"]["communityPolicy"]["kind"],
            "neighbor_majority_v1"
        );
        assert_eq!(
            report["clustering"]["communityPolicy"]["maximumSizeGuaranteed"],
            false
        );
        assert_eq!(
            report["clustering"]["hubPercentilePolicy"],
            "floor_count_times_percentile_minus_one_clamped_unique_positive_degree"
        );
        for key in [
            "graphHash",
            "sourceBindings",
            "evidenceAnchors",
            "snapshotId",
        ] {
            assert_eq!(report[key], before[key]);
        }
        assert_eq!(
            report["audit"]["byResolution"],
            before["audit"]["byResolution"]
        );
        assert_eq!(report["modelCalls"], 0);
        assert_eq!(report["complete"], false);
        assert_ne!(report["artifactId"], before["artifactId"]);
        let mut members = Vec::new();
        for community in report["communities"].as_array().unwrap() {
            members.extend(
                community["members"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|id| id.as_str().unwrap().to_owned()),
            );
        }
        assert_eq!(members.len(), graph.entities.len());
        assert_eq!(
            members.into_iter().collect::<BTreeSet<_>>(),
            graph.entities.iter().map(|e| e.id.clone()).collect()
        );
        let repeated =
            graph_extensions::analyze(&engine.store, &snapshot, &graph, &selected, &d).unwrap();
        assert_eq!(
            serde_json::to_vec(&report).unwrap(),
            serde_json::to_vec(&repeated).unwrap()
        );
        let after =
            graph_extensions::analyze(&engine.store, &snapshot, &graph, &old_options, &d).unwrap();
        assert_eq!(
            serde_json::to_vec(&before).unwrap(),
            serde_json::to_vec(&after).unwrap()
        );
        assert_eq!(identity(&graph), original_graph);
        assert_eq!(fs::read(root.path().join("src/a.rs")).unwrap(), source);
    }
}

#[test]
fn selected_community_report_replays_exact_mode_and_refuses_resealed_mode_substitution() {
    let (_root, engine, snapshot, graph) = fixture();
    let d = Deadline::new(30000);
    let view =
        yoke::graph_view::build(&engine.store, &snapshot, &graph, &Default::default(), &d).unwrap();
    let selected = AnalysisOptions {
        source_category_policy: Some(SourceCategoryPolicy::default()),
        ..options()
    };
    let report = graph_extensions::analyze_view(&engine.store, &view, &selected, &d).unwrap();
    assert_eq!(report["producer"], "yoke-community-policy-view-report-1");
    let id = report["artifactId"].as_str().unwrap();
    let record: Value = engine
        .store
        .get_record(graph_extensions::VIEW_REPORT_KIND, id)
        .unwrap();
    let original = engine.store.read_artifact(id).unwrap();
    let roots =
        graph_extensions::validate_view_report_record(&engine.store, id, &record, &d).unwrap();
    assert_eq!(roots.graph_view_id, view.id);
    let purge = yoke::retention::purge_plan(&engine.store, 0, &d).unwrap();
    assert_eq!(purge["unknownDependencyKinds"], json!([]));
    for item in purge["items"].as_array().unwrap() {
        assert_ne!(item["blobId"], report["artifactId"]);
        assert_ne!(item["blobId"], report["markdownArtifactId"]);
        assert_ne!(item["blobId"], view.artifact_id);
    }
    let mut forged = record.clone();
    forged["options"]
        .as_object_mut()
        .unwrap()
        .remove("communityPolicy");
    forged["producer"] = json!("yoke-source-category-view-report-1");
    engine
        .store
        .db
        .execute(
            "UPDATE records SET value=?1 WHERE kind=?2 AND id=?3",
            rusqlite::params![
                serde_json::to_string(&forged).unwrap(),
                graph_extensions::VIEW_REPORT_KIND,
                id
            ],
        )
        .unwrap();
    assert!(graph_extensions::validate_view_report_record(&engine.store, id, &forged, &d).is_err());
    assert!(yoke::retention::purge_plan(&engine.store, 0, &d).is_err());
    assert_eq!(engine.store.read_artifact(id).unwrap(), original);
    engine
        .store
        .db
        .execute(
            "UPDATE records SET value=?1 WHERE kind=?2 AND id=?3",
            rusqlite::params![
                serde_json::to_string(&record).unwrap(),
                graph_extensions::VIEW_REPORT_KIND,
                id
            ],
        )
        .unwrap();
    graph_extensions::validate_view_report_record(&engine.store, id, &record, &d).unwrap();
}

#[test]
fn canonical_community_retention_pins_history_and_rejects_rehashed_projection_claims() {
    let (root, mut engine, first, _) = fixture();
    let d = Deadline::new(30000);
    fs::write(
        root.path().join("src/b.rs"),
        "fn beta(){alpha();alpha();}\n",
    )
    .unwrap();
    let (snapshot, graph, _, _) = engine.index(&d).unwrap();
    let selected = AnalysisOptions {
        previous_snapshot_id: Some(first.id.clone()),
        ..options()
    };
    let report =
        graph_extensions::analyze(&engine.store, &snapshot, &graph, &selected, &d).unwrap();
    fs::write(
        root.path().join("src/b.rs"),
        "fn beta(){alpha();}\nfn newest(){}\n",
    )
    .unwrap();
    engine.index(&d).unwrap();
    let plan = yoke::retention::plan(&engine.store, &Default::default(), &d).unwrap();
    assert_eq!(plan["unknownDependencyKinds"], json!([]));
    assert_eq!(plan["snapshotIds"], json!([]));
    let purge = yoke::retention::purge_plan(&engine.store, 0, &d).unwrap();
    assert_eq!(purge["unknownDependencyKinds"], json!([]));
    for item in purge["items"].as_array().unwrap() {
        for file in first.files.iter().chain(&snapshot.files) {
            assert_ne!(item["blobId"], file.hash);
        }
    }
    let old = report["artifactId"].as_str().unwrap();
    let original = engine.store.read_artifact(old).unwrap();
    let mut forged: Value = serde_json::from_slice(&original).unwrap();
    forged["clustering"]["communityPolicy"]["splitStages"] = json!(99);
    let bytes = serde_json::to_vec(&forged).unwrap();
    let id = engine.store.artifact(&bytes).unwrap();
    let mut record: Value = engine
        .store
        .get_record(graph_extensions::REPORT_KIND, old)
        .unwrap();
    record["artifactId"] = json!(id);
    engine
        .store
        .record(graph_extensions::REPORT_KIND, &id, &record)
        .unwrap();
    assert!(yoke::retention::purge_plan(&engine.store, 0, &d).is_err());
    assert_eq!(engine.store.read_artifact(&id).unwrap(), bytes);
    assert_eq!(engine.store.read_artifact(old).unwrap(), original);
}

#[test]
fn unknown_policy_versions_extra_grants_and_expired_admission_never_publish() {
    let (_root, engine, snapshot, graph) = fixture();
    for value in [
        json!({"communityPolicy":{"kind":"neighbor_majority_v2"}}),
        json!({"communityPolicy":{"kind":"neighbor_majority_v1","allowRuntime":true}}),
        json!({"communityPolicy":{"kind":"neighbor_majority_v1"},"allowExecution":true}),
    ] {
        assert!(yoke::json::decode::<AnalysisOptions>(value).is_err());
    }
    assert!(
        yoke::json::parse(
            br#"{"communityPolicy":{"kind":"neighbor_majority_v1"},"communityPolicy":null}"#,
            1024
        )
        .is_err()
    );
    let count = || {
        engine
            .store
            .db
            .query_row(
                "SELECT count(*) FROM records WHERE kind IN (?1,?2)",
                rusqlite::params![
                    graph_extensions::REPORT_KIND,
                    graph_extensions::VIEW_REPORT_KIND
                ],
                |row| row.get::<_, i64>(0),
            )
            .unwrap()
    };
    let before = count();
    assert!(
        graph_extensions::analyze(
            &engine.store,
            &snapshot,
            &graph,
            &options(),
            &Deadline::new(0)
        )
        .is_err()
    );
    engine.store.set_deadline(&Deadline::new(30000)).unwrap();
    assert_eq!(count(), before);
}

#[test]
fn explicit_zero_weights_keep_original_observations_and_report_true_projected_isolates() {
    let (_root, engine, snapshot, graph) = fixture();
    let policy = SourceCategoryPolicy {
        relation_weights: graph
            .observations
            .iter()
            .map(|edge| (edge.relation.clone(), 0.0))
            .collect(),
        ..Default::default()
    };
    let selected = AnalysisOptions {
        source_category_policy: Some(policy),
        ..options()
    };
    let report = graph_extensions::analyze(
        &engine.store,
        &snapshot,
        &graph,
        &selected,
        &Deadline::new(30000),
    )
    .unwrap();
    assert_eq!(report["graphHash"], identity(&graph));
    assert_eq!(
        report["audit"]["totalObservations"],
        graph.observations.len()
    );
    assert_eq!(
        report["communities"].as_array().unwrap().len(),
        graph.entities.len()
    );
    for community in report["communities"].as_array().unwrap() {
        assert_eq!(community["members"].as_array().unwrap().len(), 1);
        assert_eq!(community["cohesion"], 1.0);
    }
    assert_eq!(report["clustering"]["quality"], Value::Null);
    assert_eq!(
        report["clustering"]["communityPolicy"]["isolatedNodes"]
            .as_array()
            .unwrap()
            .len(),
        graph.entities.len()
    );
    assert_eq!(report["clustering"]["communityPolicy"]["leidenCalls"], 0);
    assert_eq!(report["complete"], false);
}
