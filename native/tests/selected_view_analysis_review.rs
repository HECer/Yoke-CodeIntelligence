use serde_json::{Value, json};
use std::{collections::BTreeSet, fs};
use tempfile::TempDir;
use yoke::{
    Engine,
    capture::Deadline,
    graph_extensions::{self, VIEW_REPORT_KIND},
    graph_view::{self, BuildOptions, View},
    maintenance, retention,
};

struct Fixture {
    root: TempDir,
    engine: Engine,
    view: View,
    report: Value,
    unrooted_snapshot: String,
    current_snapshot: String,
}

fn fixture() -> Fixture {
    let root = TempDir::new().unwrap();
    fs::write(
        root.path().join("main.rs"),
        "fn helper(){helper();}\nfn caller(){helper();helper();}\n",
    )
    .unwrap();
    fs::write(
        root.path().join("guide.md"),
        "# Guide\n`main.rs::helper`\n`main.rs::helper`\n",
    )
    .unwrap();
    fs::write(
        root.path().join("package.json"),
        r#"{"name":"app","description":"app","dependencies":{"external":"^1"}}"#,
    )
    .unwrap();
    let mut engine = Engine::open(root.path()).unwrap();
    let deadline = Deadline::new(30000);
    let (snapshot, graph, _, _) = engine.index(&deadline).unwrap();
    let file = snapshot.files.iter().find(|f| f.path == "main.rs").unwrap();
    let helper = graph.entities.iter().find(|n| n.name == "helper").unwrap();
    let caller = graph.entities.iter().find(|n| n.name == "caller").unwrap();
    let item = |node: &yoke::model::Entity| {
        json!({
            "snapshotId":snapshot.id,"contentHash":file.hash,"path":file.path,
            "startByte":node.start_byte,"endByte":node.end_byte,
            "selectionStartByte":node.name_start_byte.unwrap(),
            "selectionEndByte":node.name_end_byte.unwrap(),"name":node.name,"kind":12
        })
    };
    let at = caller.start_byte + "fn caller(){".len();
    let semantic = yoke::semantic_enrichment::persist(
        &engine.store,
        &snapshot,
        &graph,
        &json!({
            "snapshotId":snapshot.id,"operation":"incomingCalls",
            "producer":"independent_retention_fixture","serverProfileHash":"a".repeat(64),
            "items":[{"root":item(helper),"from":item(caller),"fromRanges":[{
                "snapshotId":snapshot.id,"contentHash":file.hash,"path":file.path,
                "startByte":at,"endByte":at+6
            }]}]
        }),
        &deadline,
    )
    .unwrap();
    let view = graph_view::build(
        &engine.store,
        &snapshot,
        &graph,
        &BuildOptions {
            semantic_overlay_ids: vec![semantic["overlayId"].as_str().unwrap().into()],
            ..BuildOptions::default()
        },
        &deadline,
    )
    .unwrap();
    let report =
        graph_extensions::analyze_view(&engine.store, &view, &Default::default(), &deadline)
            .unwrap();
    fs::write(root.path().join("main.rs"), "fn middle_unrooted() {}\n").unwrap();
    let unrooted_snapshot = engine.index(&deadline).unwrap().0.id;
    fs::write(root.path().join("main.rs"), "fn current() {}\n").unwrap();
    let current_snapshot = engine.index(&deadline).unwrap().0.id;
    assert_ne!(view.snapshot_id, unrooted_snapshot);
    assert_ne!(unrooted_snapshot, current_snapshot);
    Fixture {
        root,
        engine,
        view,
        report,
        unrooted_snapshot,
        current_snapshot,
    }
}

#[test]
fn retention_plan_recognizes_selected_report_and_preserves_exact_history_and_cas_roots() {
    let fixture = fixture();
    let store = &fixture.engine.store;
    let deadline = Deadline::new(30000);
    let id = fixture.report["artifactId"].as_str().unwrap();
    let record: Value = store.get_record(VIEW_REPORT_KIND, id).unwrap();

    // This is the retention entry point, not merely the report validator helper.
    let plan = retention::plan(store, &Default::default(), &deadline).unwrap();
    assert_eq!(plan["unknownDependencyKinds"], json!([]));
    assert_eq!(plan["snapshotIds"], json!([fixture.unrooted_snapshot]));

    let mut expected = BTreeSet::from([
        id.to_owned(),
        fixture.report["markdownArtifactId"]
            .as_str()
            .unwrap()
            .to_owned(),
        fixture.view.artifact_id.clone(),
    ]);
    for overlay in &fixture.view.overlays {
        expected.insert(overlay["artifactId"].as_str().unwrap().to_owned());
    }
    assert!(
        fixture.view.overlays.len() >= 3,
        "Need manifest, Markdown and semantic overlays"
    );
    let roots =
        graph_extensions::validate_view_report_record(store, id, &record, &deadline).unwrap();
    assert_eq!(roots.snapshot_id, fixture.view.snapshot_id);
    assert_eq!(roots.graph_view_id, fixture.view.id);
    assert_eq!(
        roots.artifact_ids.into_iter().collect::<BTreeSet<_>>(),
        expected
    );
    let exact_bytes: Vec<_> = expected
        .iter()
        .map(|id| {
            (
                id.clone(),
                fs::read(store.dir.join("blobs").join(id)).unwrap(),
            )
        })
        .collect();

    let plan_id = plan["planId"].as_str().unwrap();
    let applied = retention::apply(store, plan_id, plan_id, true, &deadline).unwrap();
    assert_eq!(applied["retiredSnapshots"], 1);
    assert!(store.snapshot(&fixture.unrooted_snapshot).is_err());
    assert!(store.snapshot(&fixture.view.snapshot_id).is_ok());
    assert!(store.snapshot(&fixture.current_snapshot).is_ok());
    assert_eq!(
        yoke::model::identity(&store.graph(&fixture.view.snapshot_id).unwrap()),
        fixture.view.base_graph_hash
    );
    assert_eq!(
        serde_json::to_value(graph_view::load(store, &fixture.view.id, &deadline).unwrap())
            .unwrap(),
        serde_json::to_value(&fixture.view).unwrap()
    );

    // Real collection and a real unrelated purge candidate ensure protection is not vacuous.
    let orphan = store
        .blob(b"independent selected report retention orphan")
        .unwrap();
    let collected = maintenance::collect(
        store,
        &maintenance::Policy {
            apply: true,
            minimum_age_ms: 0,
            purge_quarantine: false,
        },
        &deadline,
    )
    .unwrap();
    assert!(collected["quarantinedBlobs"].as_u64().unwrap() >= 1);
    let purge = retention::purge_plan(store, 0, &deadline).unwrap();
    assert_eq!(purge["unknownDependencyKinds"], json!([]));
    let candidates: BTreeSet<_> = purge["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| row["blobId"].as_str().unwrap().to_owned())
        .collect();
    assert!(candidates.contains(&orphan));
    assert!(expected.is_disjoint(&candidates));
    for (id, bytes) in exact_bytes {
        assert_eq!(fs::read(store.dir.join("blobs").join(id)).unwrap(), bytes);
    }
    assert_eq!(
        fs::read_to_string(fixture.root.path().join("main.rs")).unwrap(),
        "fn current() {}\n"
    );
}

#[test]
fn retention_plan_refuses_forged_selected_report_scope_before_retiring_any_history() {
    let fixture = fixture();
    let store = &fixture.engine.store;
    let deadline = Deadline::new(30000);
    let id = fixture.report["artifactId"].as_str().unwrap();
    let record: Value = store.get_record(VIEW_REPORT_KIND, id).unwrap();
    let initial = retention::plan(store, &Default::default(), &deadline).unwrap();
    assert_eq!(initial["unknownDependencyKinds"], json!([]));
    assert_eq!(initial["snapshotIds"], json!([fixture.unrooted_snapshot]));
    let mut variants = Vec::new();
    for field in [
        "baseGraphHash",
        "projectionGraphHash",
        "graphViewId",
        "viewArtifactId",
    ] {
        let mut forged = record.clone();
        forged[field] = json!("f".repeat(64));
        variants.push((field, forged));
    }
    let mut wrong_snapshot = record.clone();
    wrong_snapshot["snapshotId"] = json!(fixture.unrooted_snapshot);
    variants.push(("snapshotId", wrong_snapshot));
    let mut future = record.clone();
    future["futureDependencies"] = json!([fixture.current_snapshot]);
    variants.push(("unknown dependency field", future));
    for (field, forged) in variants {
        store
            .db
            .execute(
                "UPDATE records SET value=?1 WHERE kind=?2 AND id=?3",
                rusqlite::params![forged.to_string(), VIEW_REPORT_KIND, id],
            )
            .unwrap();
        match retention::plan(store, &Default::default(), &deadline) {
            Err(_) => {}
            Ok(refused) => {
                assert!(
                    !refused["unknownDependencyKinds"]
                        .as_array()
                        .unwrap()
                        .is_empty(),
                    "accepted forged {field}"
                );
                assert_eq!(
                    refused["snapshotIds"],
                    json!([]),
                    "offered retirement for forged {field}"
                );
            }
        }
        assert!(store.snapshot(&fixture.view.snapshot_id).is_ok(), "{field}");
        assert!(
            store.snapshot(&fixture.unrooted_snapshot).is_ok(),
            "{field}"
        );
        assert!(store.snapshot(&fixture.current_snapshot).is_ok(), "{field}");
        store
            .db
            .execute(
                "UPDATE records SET value=?1 WHERE kind=?2 AND id=?3",
                rusqlite::params![record.to_string(), VIEW_REPORT_KIND, id],
            )
            .unwrap();
    }
    let restored = retention::plan(store, &Default::default(), &deadline).unwrap();
    assert_eq!(restored["unknownDependencyKinds"], json!([]));
    assert_eq!(restored["snapshotIds"], json!([fixture.unrooted_snapshot]));
}
