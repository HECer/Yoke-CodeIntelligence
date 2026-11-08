//! Historical comparison source-closure and archive-integrity adversarial tests.
//! Genuine positive controls precede CAS, proof and source-binding mutations.
//! Owned local fixture workers run only during explicit setup.
#[path = "support/historical_fixture_state.rs"]
mod oracle;
#[path = "support/meaning_comparison_fixture.rs"]
mod support;

#[path = "support/comparison_error_redaction.rs"]
mod error_redaction;
use serde_json::{Value, json};
use std::fs;
use yoke::{
    analysis::AnalyzerEpoch,
    graph_view::View,
    graph_view_archive::{self as archive, Limits},
    model::{hash, identity},
    view_comparison::{self, ComparisonOptions, SelectionPin},
};

fn valid_comparison(f: &support::Fixture) {
    let pin = SelectionPin::from_view(&f.view);
    let result = error_redaction::checked(view_comparison::compare_published(
        &f.engine.store,
        &pin,
        &pin,
        &ComparisonOptions::default(),
        &support::d(),
    ))
    .expect("A genuine published historical closure must compare before any mutation");
    assert_eq!(result.value()["before"]["graphViewId"], f.view.id);
    assert_eq!(result.value()["after"]["graphViewId"], f.view.id);
    assert_eq!(result.value()["complete"], false);
}

fn reseal_view(wire: &mut Value) {
    let mut view: View = serde_json::from_value(wire["archive"]["view"].clone()).unwrap();
    view.id.clear();
    view.artifact_id.clear();
    view.id = identity(&view);
    view.artifact_id = hash(serde_json::to_vec(&view).unwrap());
    wire["archive"]["view"] = serde_json::to_value(view).unwrap();
}

#[test]
fn h08_published_comparison_requires_uncited_history_and_uncorrupted_view_overlay_cas() {
    let f = support::fixture(AnalyzerEpoch::V9);
    valid_comparison(&f);
    let checked = archive::prepare_published(
        &f.engine.store,
        &f.view.id,
        Limits::default(),
        &support::d(),
    )
    .unwrap();
    let wire: Value = serde_json::from_slice(checked.bytes()).unwrap();
    let old_hash = hash(support::HISTORY_OLD);
    assert_eq!(wire["sourceObjects"][&old_hash], support::HISTORY_OLD);
    assert!(
        f.view
            .meaning
            .as_ref()
            .unwrap()
            .packets
            .iter()
            .flat_map(|packet| &packet.evidence)
            .all(|evidence| evidence.path != "history.rs")
    );
    let calls = fs::read(&f.marker).unwrap();
    let initial = oracle::state(&f.engine);
    fs::write(f.engine.root.join("history.rs"), support::HISTORY_OLD).unwrap();
    let overlay = f.view.overlays[0]["artifactId"]
        .as_str()
        .expect("A genuine Markdown overlay must have its published artifact");
    for (mode, blob_id) in [
        ("missing_uncited_history", old_hash.as_str()),
        ("wrong_same_path_history_bytes", old_hash.as_str()),
        ("corrupt_view", f.view.artifact_id.as_str()),
        ("corrupt_overlay", overlay),
    ] {
        let path = f.engine.store.dir.join("blobs").join(blob_id);
        let original = fs::read(&path).unwrap();
        assert_eq!(hash(&original), blob_id);
        if mode == "missing_uncited_history" {
            fs::remove_file(&path).unwrap();
        } else if mode == "wrong_same_path_history_bytes" {
            fs::write(&path, support::HISTORY_NEW).unwrap();
        } else {
            let mut corrupt = original.clone();
            assert!(!corrupt.is_empty());
            corrupt[0] ^= 1;
            fs::write(&path, corrupt).unwrap();
        }
        let tampered_baseline = oracle::state(&f.engine);
        assert!(
            archive::prepare_published(
                &f.engine.store,
                &f.view.id,
                Limits::default(),
                &support::d()
            )
            .is_err(),
            "Existing checked archive must detect the {mode} mutation"
        );
        let pin = SelectionPin::from_view(&f.view);
        let error = match error_redaction::checked(view_comparison::compare_published(
            &f.engine.store,
            &pin,
            &pin,
            &ComparisonOptions::default(),
            &support::d(),
        )) {
            Ok(value) => {
                drop(value);
                panic!("A comparison must not repair an invalid historical closure")
            }
            Err(error) => error,
        };
        assert!(
            !format!("{error:#}").contains("implementation_unavailable"),
            "A refusing scaffold is not this closure oracle"
        );
        assert_eq!(oracle::state(&f.engine), tampered_baseline);
        assert_eq!(fs::read(&f.marker).unwrap(), calls);
        assert_eq!(
            fs::read_to_string(f.engine.root.join("history.rs")).unwrap(),
            support::HISTORY_OLD
        );
        fs::write(&path, original).unwrap();
        valid_comparison(&f);
        assert_eq!(oracle::state(&f.engine), initial);
    }
}

#[test]
fn h08_portable_full_source_and_proof_closure_cannot_be_repaired_from_live_paths() {
    let f = support::fixture(AnalyzerEpoch::V8);
    let prepared =
        archive::prepare(&f.engine.store, &f.view, Limits::default(), &support::d()).unwrap();
    let original: Value = serde_json::from_slice(prepared.bytes()).unwrap();
    let good = archive::parse(prepared.bytes(), Limits::default(), &support::d()).unwrap();
    error_redaction::checked(view_comparison::compare_meaning_archives(
        &good,
        &good,
        &ComparisonOptions::default(),
        &support::d(),
    ))
    .expect("Valid checked-byte comparison before malformed archive candidates");
    let baseline = oracle::state(&f.engine);
    let calls = fs::read(&f.marker).unwrap();
    fs::write(f.engine.root.join("history.rs"), support::HISTORY_OLD).unwrap();
    for mode in [
        "missing_uncited",
        "wrong_same_path",
        "corrupt_embedded_overlay",
        "missing_original_receipt",
    ] {
        let mut changed = original.clone();
        match mode {
            "missing_uncited" => {
                changed["sourceObjects"]
                    .as_object_mut()
                    .unwrap()
                    .remove(&hash(support::HISTORY_OLD))
                    .unwrap();
            }
            "wrong_same_path" => {
                changed["sourceObjects"][hash(support::HISTORY_OLD)] = json!(support::HISTORY_NEW);
            }
            "corrupt_embedded_overlay" => {
                changed["archive"]["inputs"][0]["payload"] = json!("{\"forged\":true}");
            }
            _ => {
                let receipt = f.view.meaning.as_ref().unwrap().packets[0]
                    .provenance
                    .response_artifact_id
                    .clone();
                let rows = changed["archive"]["meaningArchive"]["proof"]["artifacts"]
                    .as_array_mut()
                    .unwrap();
                let position = rows
                    .iter()
                    .position(|row| row["artifactId"] == receipt)
                    .unwrap();
                rows.remove(position);
            }
        }
        assert_ne!(changed, original);
        assert!(
            archive::parse(
                &serde_json::to_vec(&changed).unwrap(),
                Limits::default(),
                &support::d()
            )
            .is_err(),
            "Full portable closure accepted {mode}"
        );
        assert_eq!(oracle::state(&f.engine), baseline);
        assert_eq!(fs::read(&f.marker).unwrap(), calls);
    }
}

#[test]
fn h09_public_view_resealing_does_not_authorize_forged_precision_ranges_edges_or_completeness() {
    let f = support::fixture(AnalyzerEpoch::V9);
    let prepared =
        archive::prepare(&f.engine.store, &f.view, Limits::default(), &support::d()).unwrap();
    let original: Value = serde_json::from_slice(prepared.bytes()).unwrap();
    let good = archive::parse(prepared.bytes(), Limits::default(), &support::d()).unwrap();
    error_redaction::checked(view_comparison::compare_meaning_archives(
        &good,
        &good,
        &ComparisonOptions::default(),
        &support::d(),
    ))
    .expect("Genuine checked container must reach the comparison before resealed tampering");
    let baseline = oracle::state(&f.engine);
    let calls = fs::read(&f.marker).unwrap();
    for mode in 0..7 {
        let mut changed = original.clone();
        let view = &mut changed["archive"]["view"];
        match mode {
            0 | 1 => {
                let node = view["nodes"]
                    .as_array_mut()
                    .unwrap()
                    .iter_mut()
                    .find(|node| node["name"] == "alpha" && node["kind"] == "function")
                    .unwrap();
                let binding = &mut node["sourceBindings"][0];
                if mode == 0 {
                    let replacement = if binding["anchor"] == "file" {
                        "source_range"
                    } else {
                        "file"
                    };
                    binding["anchor"] = json!(replacement);
                } else {
                    binding["endByte"] = json!(binding["endByte"].as_u64().unwrap() - 1);
                }
            }
            2 => {
                view["evidence"][0]["startLine"] =
                    json!(view["evidence"][0]["startLine"].as_u64().unwrap() + 1);
            }
            3 => {
                let row = view["observations"]
                    .as_array_mut()
                    .unwrap()
                    .iter_mut()
                    .find(|row| row["source"] != row["target"] && row["target"].is_string())
                    .unwrap();
                row["target"] = row["source"].clone();
            }
            4 => {
                let row = view["observations"]
                    .as_array_mut()
                    .unwrap()
                    .iter_mut()
                    .find(|row| {
                        row["evidenceIds"]
                            .as_array()
                            .is_some_and(|ids| !ids.is_empty())
                    })
                    .unwrap();
                row["evidenceIds"] = json!([]);
            }
            5 => {
                view["limitations"]
                    .as_array_mut()
                    .unwrap()
                    .push(json!("forged completeness assertion"));
            }
            _ => {
                view["complete"] = json!(true);
            }
        }
        assert_ne!(
            changed, original,
            "Each named mutation must actually alter the wire"
        );
        assert_eq!(
            changed["archive"]["meaningArchive"],
            original["archive"]["meaningArchive"]
        );
        assert_eq!(
            changed["archive"]["view"]["meaning"],
            original["archive"]["view"]["meaning"]
        );
        reseal_view(&mut changed);
        let resealed: View = serde_json::from_value(changed["archive"]["view"].clone()).unwrap();
        let mut identity_input = resealed.clone();
        identity_input.id.clear();
        identity_input.artifact_id.clear();
        assert_eq!(resealed.id, identity(&identity_input));
        identity_input.id = resealed.id.clone();
        assert_eq!(
            resealed.artifact_id,
            hash(serde_json::to_vec(&identity_input).unwrap())
        );
        assert!(
            archive::parse(
                &serde_json::to_vec(&changed).unwrap(),
                Limits::default(),
                &support::d()
            )
            .is_err(),
            "Resealed forged View accepted mode {mode}"
        );
        assert_eq!(oracle::state(&f.engine), baseline);
        assert_eq!(fs::read(&f.marker).unwrap(), calls);
    }
}
