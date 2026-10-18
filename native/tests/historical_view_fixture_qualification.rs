//! Existing-API source View and original/rebound Meaning archive qualification.
//! The owned local Node worker runs only during explicit Meaning fixture setup.
//! Checked archives replay independently of the removed source workspace.
#[path = "support/historical_fixture_state.rs"]
mod oracle;
#[path = "support/meaning_comparison_fixture.rs"]
mod support;

use serde_json::Value;
use std::fs;
use yoke::{
    Engine,
    analysis::AnalyzerEpoch,
    capture,
    graph_view::{self, BuildOptions, StructuralKind},
    graph_view_archive::{self as archive, Limits},
    meaning_build,
    model::{hash, identity},
};

#[test]
fn genuine_source_only_views_qualify_epoch8_and_epoch9_and_the_mutation_oracle() {
    for epoch in [AnalyzerEpoch::V8, AnalyzerEpoch::V9] {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("a.rs"), support::A).unwrap();
        let mut engine = Engine::open(dir.path()).unwrap();
        engine.capture_epoch = epoch;
        let (snapshot, graph, _, _) = engine.index(&support::d()).unwrap();
        assert_eq!(
            capture::analyzer_epoch_for_snapshot(&engine.store, &snapshot).unwrap(),
            epoch
        );
        let view = graph_view::build(
            &engine.store,
            &snapshot,
            &graph,
            &BuildOptions::default(),
            &support::d(),
        )
        .unwrap();
        assert!(view.meaning.is_none());
        assert_eq!(view.snapshot_id, snapshot.id);
        assert_eq!(view.base_graph_hash, identity(&graph));
        assert!(view.nodes.iter().any(|node| node["name"] == "alpha"));
        oracle::calibrate_foreign_connection_mutation_oracle(&engine);
        let baseline = oracle::state(&engine);
        let loaded = graph_view::load(&engine.store, &view.id, &support::d()).unwrap();
        assert_eq!(
            serde_json::to_value(&loaded).unwrap(),
            serde_json::to_value(&view).unwrap()
        );
        assert_eq!(oracle::state(&engine), baseline);
        assert_eq!(
            fs::read_to_string(dir.path().join("a.rs")).unwrap(),
            support::A
        );
    }
}

#[test]
fn genuine_original_and_rebound_typed_archives_qualify_both_epochs_without_a_live_store() {
    for epoch in [AnalyzerEpoch::V8, AnalyzerEpoch::V9] {
        let f = support::fixture(epoch);
        let calls = fs::read(&f.marker).unwrap();
        assert_eq!(String::from_utf8(calls.clone()).unwrap().lines().count(), 3);
        assert!(!f.provider.command.as_os_str().is_empty());
        assert!(
            !std::path::Path::new(&f.provider.args[0]).exists(),
            "The exact owned worker is already removed"
        );
        assert_ne!(f.original.id, f.snapshot.id);
        assert!(!f.view.overlays.is_empty());
        let alpha = f
            .view
            .nodes
            .iter()
            .find(|node| node["name"] == "alpha" && node["kind"] == "function")
            .unwrap();
        let binding = &alpha["sourceBindings"][0];
        assert!(
            binding["startByte"].is_u64()
                && binding["endByte"].is_u64()
                && binding["anchor"].is_string()
        );
        assert!(f.view.evidence[0]["startLine"].is_u64());
        assert!(
            f.view
                .observations
                .iter()
                .any(|row| row["source"] != row["target"] && row["target"].is_string())
        );
        assert!(f.view.observations.iter().any(|row| {
            row["evidenceIds"]
                .as_array()
                .is_some_and(|ids| !ids.is_empty())
        }));
        for snapshot in [&f.original, &f.snapshot] {
            assert_eq!(
                capture::analyzer_epoch_for_snapshot(&f.engine.store, snapshot).unwrap(),
                epoch
            );
        }
        let rebound = f.view.meaning.as_ref().unwrap();
        assert!(
            rebound
                .packets
                .iter()
                .all(|packet| packet.provenance.rebound)
        );
        let original_job = rebound.packets[0].provenance.original_job_id.clone();
        assert!(
            rebound
                .packets
                .iter()
                .all(|packet| packet.provenance.original_job_id == original_job)
        );
        // Existing read-only public status supplies the genuine original published graph ID.
        // This is not another build, provider call, constructed ID or new comparison API.
        let original_status =
            meaning_build::status(&f.engine.store, &original_job, None, &support::d()).unwrap();
        assert_eq!(original_status["status"], "ready");
        let original_graph_id = original_status["graphId"].as_str().unwrap().to_owned();
        let original_graph = f.engine.store.graph(&f.original.id).unwrap();
        let original_view = graph_view::build(
            &f.engine.store,
            &f.original,
            &original_graph,
            &BuildOptions {
                structural_kinds: vec![StructuralKind::Markdown],
                meaning_graph_ids: vec![original_graph_id],
                ..BuildOptions::default()
            },
            &support::d(),
        )
        .unwrap();
        let original_layer = original_view.meaning.as_ref().unwrap();
        assert!(
            original_layer
                .packets
                .iter()
                .all(|packet| !packet.provenance.rebound)
        );
        assert_eq!(original_layer.scope.snapshot_id, f.original.id);
        assert_eq!(original_layer.scope.analyzer_epoch, epoch.producer());
        assert_eq!(rebound.scope.snapshot_id, f.snapshot.id);
        assert_eq!(rebound.scope.analyzer_epoch, epoch.producer());

        let baseline = oracle::state(&f.engine);
        let mut transports = Vec::new();
        for view in [&original_view, &f.view] {
            let by_value =
                archive::prepare(&f.engine.store, view, Limits::default(), &support::d()).unwrap();
            let by_id = archive::prepare_published(
                &f.engine.store,
                &view.id,
                Limits::default(),
                &support::d(),
            )
            .unwrap();
            assert_eq!(by_id.bytes(), by_value.bytes());
            assert_eq!(by_id.archive_id(), hash(by_value.bytes()));
            assert_eq!(
                serde_json::to_value(by_id.view()).unwrap(),
                serde_json::to_value(view).unwrap()
            );
            transports.push((
                by_value.bytes().to_vec(),
                serde_json::to_value(view).unwrap(),
            ));
        }
        let original_wire: Value = serde_json::from_slice(&transports[0].0).unwrap();
        let rebound_wire: Value = serde_json::from_slice(&transports[1].0).unwrap();
        assert_eq!(
            original_wire["sourceObjects"][hash(support::HISTORY_OLD)],
            support::HISTORY_OLD
        );
        assert!(
            original_wire["sourceObjects"]
                .as_object()
                .unwrap()
                .get(&hash(support::HISTORY_NEW))
                .is_none()
        );
        assert_eq!(
            rebound_wire["sourceObjects"][hash(support::HISTORY_OLD)],
            support::HISTORY_OLD
        );
        assert_eq!(
            rebound_wire["sourceObjects"][hash(support::HISTORY_NEW)],
            support::HISTORY_NEW
        );
        assert!(
            rebound
                .packets
                .iter()
                .flat_map(|packet| &packet.evidence)
                .all(|evidence| evidence.path != "history.rs")
        );
        assert_eq!(oracle::state(&f.engine), baseline);
        assert_eq!(fs::read(&f.marker).unwrap(), calls);

        let marker = f.marker.clone();
        let removed_root = f.engine.root.clone();
        let _retained = support::remove_source_workspace(f);
        assert!(!removed_root.exists());
        for (bytes, expected) in transports {
            let checked = archive::parse(&bytes, Limits::default(), &support::d()).unwrap();
            assert_eq!(checked.archive_id(), hash(&bytes));
            assert_eq!(checked.bytes(), bytes);
            assert_eq!(serde_json::to_value(checked.view()).unwrap(), expected);
            assert_eq!(
                checked.snapshot().id,
                expected["snapshotId"].as_str().unwrap()
            );
            assert_eq!(
                checked
                    .view()
                    .meaning
                    .as_ref()
                    .unwrap()
                    .scope
                    .analyzer_epoch,
                epoch.producer()
            );
            assert_eq!(
                serde_json::to_value(checked.canonical_graph()).unwrap()["snapshotId"],
                expected["snapshotId"]
            );
        }
        assert_eq!(fs::read(marker).unwrap(), calls);
    }
}
