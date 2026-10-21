use serde_json::{Value, json};
use std::fs;
use yoke::{
    Engine,
    capture::Deadline,
    graph_extensions::{self, AnalysisCategory as Category, AnalysisOptions, SourceCategoryPolicy},
    model::{Graph, Snapshot, identity},
};

fn fixture() -> (tempfile::TempDir, Engine, Snapshot, Graph) {
    let root = tempfile::tempdir().unwrap();
    fs::create_dir(root.path().join("src")).unwrap();
    fs::write(
        root.path().join("src/a.rs"),
        "pub fn alpha(){ beta();beta(); }\r\nfn local(){}\r\npub fn Path(){alpha();}\r\n",
    )
    .unwrap();
    fs::write(root.path().join("src/b.rs"), "pub fn beta(){ alpha(); }\n").unwrap();
    let mut engine = Engine::open(root.path()).unwrap();
    let (snapshot, graph, _, _) = engine.index(&Deadline::new(30000)).unwrap();
    (root, engine, snapshot, graph)
}
#[test]
fn explicit_policy_preserves_original_graph_and_legacy_report_identity() {
    let (_root, engine, snapshot, graph) = fixture();
    let before = identity(&graph);
    let legacy = graph_extensions::analyze(
        &engine.store,
        &snapshot,
        &graph,
        &AnalysisOptions::default(),
        &Deadline::new(30000),
    )
    .unwrap();
    assert!(legacy["options"].get("sourceCategoryPolicy").is_none());
    assert!(legacy.get("analysisPolicy").is_none());
    let mut policy = SourceCategoryPolicy::default();
    policy
        .resolution_categories
        .insert("heuristic".into(), Category::Ambiguous);
    policy.excluded_labels.insert("Path".into());
    let options = AnalysisOptions {
        source_category_policy: Some(policy),
        uncertain_weight: 0.0,
        hub_limit: 100,
        ..Default::default()
    };
    let report = graph_extensions::analyze(
        &engine.store,
        &snapshot,
        &graph,
        &options,
        &Deadline::new(30000),
    )
    .unwrap();
    assert_eq!(report["producer"], "yoke-source-category-report-1");
    assert_eq!(report["graphHash"], before);
    assert_eq!(
        report["audit"]["byResolution"],
        legacy["audit"]["byResolution"]
    );
    assert_eq!(report["sourceBindings"], legacy["sourceBindings"]);
    assert_eq!(report["evidenceAnchors"], legacy["evidenceAnchors"]);
    assert_ne!(report["artifactId"], legacy["artifactId"]);
    assert!(
        report["godNodes"]
            .as_array()
            .unwrap()
            .iter()
            .all(|n| n["label"] != "Path")
    );
    let path = graph.entities.iter().find(|e| e.name == "Path").unwrap();
    assert!(
        report["sourceBindings"]
            .as_array()
            .unwrap()
            .iter()
            .any(|n| n["entityId"] == path.id)
    );
    assert_eq!(
        report["clustering"]["uncertainWeightAppliesTo"],
        "policy_ambiguous_only"
    );
    assert!(
        report["surprises"]
            .as_array()
            .unwrap()
            .iter()
            .all(|n| n.get("rankingReasons").is_some() && n.get("policyCategory").is_some())
    );
    assert_eq!(identity(&engine.store.graph(&snapshot.id).unwrap()), before);
    let after = graph_extensions::analyze(
        &engine.store,
        &snapshot,
        &graph,
        &AnalysisOptions::default(),
        &Deadline::new(30000),
    )
    .unwrap();
    assert_eq!(after, legacy);
    let again = graph_extensions::analyze(
        &engine.store,
        &snapshot,
        &graph,
        &options,
        &Deadline::new(30000),
    )
    .unwrap();
    assert_eq!(again, report);
}
#[test]
fn declared_categories_runtime_families_and_scores_are_independent_of_source_authority() {
    let (_root, _engine, _snapshot, graph) = fixture();
    let mut from = graph
        .entities
        .iter()
        .find(|e| e.name == "alpha")
        .unwrap()
        .clone();
    let mut to = graph
        .entities
        .iter()
        .find(|e| e.name == "beta")
        .unwrap()
        .clone();
    let mut edge = graph
        .observations
        .iter()
        .find(|e| e.relation == "calls" && e.target.is_some())
        .unwrap()
        .clone();
    from.path = "src/client.ts".into();
    to.path = "src/implementation.js".into();
    edge.resolution = "heuristic".into();
    let mut policy = SourceCategoryPolicy::default();
    assert_eq!(policy.score(&edge, &from, &to, false, 3, 3).0, 2);
    to.path = "src/implementation.py".into();
    let (score, reasons) = policy.score(&edge, &from, &to, true, 3, 3);
    assert_eq!(score, 0);
    assert!(reasons.contains(&"inferred_structural_cross_runtime_or_document_suppressed"));
    to.path = "src/design.md".into();
    assert_eq!(policy.score(&edge, &from, &to, true, 3, 3).0, 0);
    edge.relation = "semantically_similar_to".into();
    to.path = "papers/design.pdf".into();
    let (score, reasons) = policy.score(&edge, &from, &to, true, 3, 3);
    assert_eq!(score, 10);
    assert!(reasons.contains(&"semantic_similarity_claim"));
    edge.relation = "calls".into();
    policy.relation_weights.insert("calls".into(), 2.0);
    assert_eq!(policy.weight(&edge, 0.25), 2.0);
    policy
        .resolution_categories
        .insert("heuristic".into(), Category::Ambiguous);
    assert_eq!(policy.weight(&edge, 0.25), 0.5);
    edge.resolution = "syntactic".into();
    assert_eq!(policy.weight(&edge, 0.25), 2.0);
    edge.resolution = "provider_unknown".into();
    assert_eq!(policy.category(&edge), Category::Unmapped);
    to.path = "src/implementation.js".into();
    assert_eq!(policy.score(&edge, &from, &to, false, 3, 3).0, 0);
    assert_eq!(edge.resolution, "provider_unknown");
    assert!(policy.eligible(&from, 1));
    policy.excluded_labels.insert(from.name.clone());
    assert!(!policy.eligible(&from, 1));
    policy.excluded_labels.clear();
    from.name = ".synthetic()".into();
    assert!(policy.eligible(&from, 1));
    policy.exclude_stub_labels = true;
    assert!(!policy.eligible(&from, 1));
    from.name = "actual_extensionless".into();
    from.path = "Makefile".into();
    assert!(policy.eligible(&from, 1));
    policy.extensionless_as_concepts = true;
    assert!(!policy.eligible(&from, 1));
}
#[test]
fn policy_protocol_bounds_deadlines_and_unknown_fields_refuse_without_publication() {
    let (_root, engine, snapshot, graph) = fixture();
    let count = || {
        engine
            .store
            .db
            .query_row("SELECT COUNT(*) FROM records", [], |r| r.get::<_, i64>(0))
            .unwrap()
    };
    let before = count();
    let mut invalid = Vec::new();
    let p = SourceCategoryPolicy {
        schema_version: 2,
        ..Default::default()
    };
    invalid.push(p);
    for value in [-1.0, f64::INFINITY, f64::NAN, 1000.1] {
        let mut p = SourceCategoryPolicy::default();
        p.relation_weights.insert("calls".into(), value);
        invalid.push(p);
    }
    let mut p = SourceCategoryPolicy::default();
    p.excluded_labels.insert("bad\0label".into());
    invalid.push(p);
    let mut p = SourceCategoryPolicy::default();
    p.excluded_labels.insert(format!("ghp_{}", "A".repeat(36)));
    invalid.push(p);
    let mut p = SourceCategoryPolicy::default();
    for i in 0..129 {
        p.excluded_labels.insert(format!("{i}"));
    }
    invalid.push(p);
    let mut p = SourceCategoryPolicy::default();
    for i in 0..120 {
        p.excluded_labels.insert(format!("{i}{}", "x".repeat(180)));
    }
    invalid.push(p);
    for policy in invalid {
        let options = AnalysisOptions {
            source_category_policy: Some(policy),
            ..Default::default()
        };
        assert!(
            graph_extensions::analyze(
                &engine.store,
                &snapshot,
                &graph,
                &options,
                &Deadline::new(30000)
            )
            .is_err()
        );
        assert_eq!(count(), before);
    }
    let mut value = serde_json::to_value(SourceCategoryPolicy::default()).unwrap();
    value["allowNetwork"] = json!(true);
    assert!(serde_json::from_value::<SourceCategoryPolicy>(value).is_err());
    assert!(
        yoke::json::parse(
            br#"{"resolutionCategories":{"heuristic":"inferred","heuristic":"extracted"}}"#,
            64000
        )
        .is_err()
    );
    let options = AnalysisOptions {
        source_category_policy: Some(SourceCategoryPolicy::default()),
        ..Default::default()
    };
    assert!(
        graph_extensions::analyze(
            &engine.store,
            &snapshot,
            &graph,
            &options,
            &Deadline::new(0)
        )
        .is_err()
    );
    assert_eq!(count(), before);
    let parsed: Value = serde_json::to_value(AnalysisOptions::default()).unwrap();
    assert!(parsed.get("sourceCategoryPolicy").is_none());
}

#[test]
fn selected_policy_report_replays_exact_configuration_and_rejects_resealed_metadata() {
    let (_root, engine, snapshot, graph) = fixture();
    let deadline = Deadline::new(30000);
    let view = yoke::graph_view::build(
        &engine.store,
        &snapshot,
        &graph,
        &yoke::graph_view::BuildOptions::default(),
        &deadline,
    )
    .unwrap();
    let options = AnalysisOptions {
        source_category_policy: Some(SourceCategoryPolicy::default()),
        ..Default::default()
    };
    let report = graph_extensions::analyze_view(&engine.store, &view, &options, &deadline).unwrap();
    assert_eq!(report["producer"], "yoke-source-category-view-report-1");
    let id = report["artifactId"].as_str().unwrap();
    let record: Value = engine
        .store
        .get_record(graph_extensions::VIEW_REPORT_KIND, id)
        .unwrap();
    let roots =
        graph_extensions::validate_view_report_record(&engine.store, id, &record, &deadline)
            .unwrap();
    assert_eq!(roots.graph_view_id, view.id);
    let original = engine.store.read_artifact(id).unwrap();
    let mut forged = record.clone();
    forged["options"]["sourceCategoryPolicy"]["relationWeights"]["calls"] = json!(0.0);
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
    assert!(
        graph_extensions::validate_view_report_record(&engine.store, id, &forged, &deadline)
            .is_err()
    );
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
    graph_extensions::validate_view_report_record(&engine.store, id, &record, &deadline).unwrap();
    assert_eq!(
        identity(&engine.store.graph(&snapshot.id).unwrap()),
        identity(&graph)
    );
}

#[test]
fn explicit_percentile_policy_suppresses_the_source_bound_hubs_at_its_declared_boundary() {
    let (_root, engine, snapshot, graph) = fixture();
    let expected: std::collections::BTreeSet<_> = graph
        .entities
        .iter()
        .filter(|e| {
            (e.kind == "file" && e.path == "src/a.rs")
                || matches!(e.name.as_str(), "alpha" | "beta")
        })
        .map(|e| e.id.clone())
        .collect();
    assert_eq!(expected.len(), 3);
    let options = AnalysisOptions {
        source_category_policy: Some(SourceCategoryPolicy::default()),
        exclude_hubs_percentile: Some(66.0),
        ..Default::default()
    };
    let report = graph_extensions::analyze(
        &engine.store,
        &snapshot,
        &graph,
        &options,
        &Deadline::new(30000),
    )
    .unwrap();
    let actual: std::collections::BTreeSet<_> = report["clustering"]["excludedHubs"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_owned())
        .collect();
    assert_eq!(actual, expected);
    assert_eq!(report["graphHash"], identity(&graph));
}
