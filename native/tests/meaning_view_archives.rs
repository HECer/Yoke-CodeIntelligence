//! Typed portable Meaning-View replay and adversarial transport contracts.
#[path = "support/meaning_archive_fixture.rs"]
mod support;
use serde_json::{Value, json};
use std::{collections::BTreeSet, fs};
use support::{A, B, HISTORY_NEW, HISTORY_OLD, d, fixture, remove_source_workspace, store_state};
use yoke::{
    analysis::AnalyzerEpoch,
    capture::Deadline,
    graph_view::View,
    graph_view_archive::{self as archive, Limits},
    model::{hash, identity},
};

fn prepared(f: &support::Fixture) -> Vec<u8> {
    archive::prepare(&f.engine.store, &f.view, Limits::default(), &d())
        .expect("A genuine published typed View needs an explicit portable archive")
        .bytes()
        .to_vec()
}
fn reseal_view(wire: &mut Value) {
    // A malicious producer can recompute public hashes; replay must still reject false claims.
    let mut view: View = serde_json::from_value(wire["archive"]["view"].clone()).unwrap();
    view.id.clear();
    view.artifact_id.clear();
    view.id = identity(&view);
    view.artifact_id = hash(serde_json::to_vec(&view).unwrap());
    wire["archive"]["view"] = serde_json::to_value(view).unwrap();
}

#[test]
fn portable_archive_replays_both_epochs_without_a_store_live_tree_or_worker() {
    // Break caught: packaging only current/quoted files, dropping typed rows, or reading live state.
    for epoch in [AnalyzerEpoch::V8, AnalyzerEpoch::V9] {
        let f = fixture(epoch);
        let before = store_state(&f);
        let bytes = prepared(&f);
        assert_eq!(
            store_state(&f),
            before,
            "Preparation must publish no artifacts or records"
        );
        let wire: Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(wire["profile"], "meaning_view_archive_v1");
        assert_eq!(
            serde_json::to_vec(&wire).unwrap(),
            bytes,
            "Preparation must use the canonical Value transport encoder"
        );
        assert!(wire.get("sourceAuthority").is_none());
        let sources = wire["sourceObjects"].as_object().unwrap();
        assert_eq!(sources.get(&hash(HISTORY_OLD)).unwrap(), HISTORY_OLD);
        assert_eq!(sources.get(&hash(HISTORY_NEW)).unwrap(), HISTORY_NEW);
        assert_eq!(sources.get(&hash(A)).unwrap(), A);
        assert_eq!(sources.get(&hash(B)).unwrap(), B);
        let scopes = wire["archive"]["meaningArchive"]["proof"]["sourceManifests"]
            .as_array()
            .unwrap();
        assert!(scopes.iter().any(|s| s["id"] == f.original.id));
        assert!(scopes.iter().any(|s| s["id"] == f.snapshot.id));
        let saved_view = serde_json::to_value(&f.view).unwrap();
        let saved_snapshot = serde_json::to_value(&f.snapshot).unwrap();
        let graph_hash = identity(&f.graph);
        let selected_layer_id = identity(f.view.meaning.as_ref().unwrap());
        let calls = fs::read(&f.marker).unwrap();
        let marker = f.marker.clone();
        let _retained_fixture = remove_source_workspace(f);
        let checked = archive::parse(&bytes, Limits::default(), &d()).unwrap();
        assert_eq!(checked.archive_id(), hash(&bytes));
        assert_eq!(checked.bytes(), bytes);
        assert_eq!(
            serde_json::to_value(checked.snapshot()).unwrap(),
            saved_snapshot
        );
        assert_eq!(identity(checked.canonical_graph()), graph_hash);
        assert_eq!(serde_json::to_value(checked.view()).unwrap(), saved_view);
        assert_eq!(
            identity(checked.view().meaning.as_ref().unwrap()),
            selected_layer_id
        );
        // Equivalent formatting must reach replay, rather than fail an encoding-order gate.
        let object = wire.as_object().unwrap();
        let reversed = object
            .iter()
            .rev()
            .map(|(key, value)| {
                format!(
                    "{}:{}",
                    serde_json::to_string(key).unwrap(),
                    serde_json::to_string(value).unwrap()
                )
            })
            .collect::<Vec<_>>()
            .join(",");
        let reversed = format!(" {{ {reversed} }} \n").into_bytes();
        let pretty = serde_json::to_vec_pretty(&wire).unwrap();
        assert_ne!(reversed, bytes);
        assert_ne!(pretty, bytes);
        for equivalent in [serde_json::to_vec(&wire).unwrap(), pretty, reversed] {
            let normalized = archive::parse(&equivalent, Limits::default(), &d()).unwrap();
            assert_eq!(normalized.bytes(), bytes);
            assert_eq!(normalized.archive_id(), hash(&bytes));
            assert_eq!(serde_json::to_value(normalized.view()).unwrap(), saved_view);
        }
        assert_eq!(fs::read(marker).unwrap(), calls);
    }
}

#[test]
fn replay_preserves_typed_claim_ids_ordered_roles_exact_quotes_and_unknown_receipts() {
    // Break caught: entity/clique conversion, re-namespacing, receipt rewriting or unknown=>zero.
    let f = fixture(AnalyzerEpoch::V9);
    let original = f.view.meaning.as_ref().unwrap();
    let checked = archive::parse(&prepared(&f), Limits::default(), &d()).unwrap();
    let layer = checked.view().meaning.as_ref().unwrap();
    assert_eq!(layer, original);
    assert_eq!(layer.scope.analyzer_epoch, "yoke-tree-sitter-9");
    let packet = layer
        .packets
        .iter()
        .find(|p| !p.hyperedges.is_empty())
        .unwrap();
    assert_eq!(packet.relations.len(), 3);
    let forward = packet
        .relations
        .iter()
        .find(|r| r.description == "Forward")
        .unwrap();
    let parallel = packet
        .relations
        .iter()
        .find(|r| r.description == "Parallel")
        .unwrap();
    let reverse = packet
        .relations
        .iter()
        .find(|r| r.description == "Reverse")
        .unwrap();
    assert_eq!(forward.source, parallel.source);
    assert_eq!(forward.target, parallel.target);
    assert_ne!(forward.id, parallel.id);
    assert_eq!(forward.source, reverse.target);
    assert_eq!(forward.target, reverse.source);
    let role_labels: Vec<_> = packet.hyperedges[0]
        .participants
        .iter()
        .map(|p| p.role.as_str())
        .collect();
    assert_eq!(role_labels, ["output", "input", "input"]);
    let concept = |id: &str| {
        packet
            .concepts
            .iter()
            .find(|c| c.id == id || c.claim_id == id)
            .unwrap()
    };
    let participant_labels: Vec<_> = packet.hyperedges[0]
        .participants
        .iter()
        .map(|p| concept(&p.concept).label.as_str())
        .collect();
    assert_eq!(
        participant_labels,
        ["Write path", "Storage policy", "Read path"]
    );
    let mut quote_paths = BTreeSet::new();
    for evidence in &packet.evidence {
        let text = match evidence.path.as_str() {
            "a.rs" => A,
            "b.rs" => B,
            _ => panic!("Unexpected citation path"),
        };
        assert_eq!(evidence.text, text[evidence.start_byte..evidence.end_byte]);
        assert_eq!(evidence.content_hash, hash(text));
        quote_paths.insert(evidence.path.as_str());
    }
    assert_eq!(quote_paths, BTreeSet::from(["a.rs", "b.rs"]));
    for packet in &layer.packets {
        assert!(packet.provenance.rebound);
        assert_eq!(packet.provenance.original_snapshot_id, f.original.id);
        assert!(packet.provenance.provider_reported_cost_micros.is_none());
        assert!(packet.provenance.provider_reported_usage.is_none());
        assert!(!packet.provenance.original_job_id.is_empty());
        assert!(!packet.provenance.response_artifact_id.is_empty());
    }
    assert_eq!(checked.view().nodes, f.view.nodes);
    assert_eq!(checked.view().observations, f.view.observations);
    assert_eq!(checked.view().evidence, f.view.evidence);
    assert!(
        !checked
            .canonical_graph()
            .entities
            .iter()
            .any(|e| e.kind.starts_with("meaning_"))
    );
}

#[test]
fn missing_historical_uncited_file_and_wrong_same_path_bytes_are_not_repaired_from_live_state() {
    // Break caught: original sources resolved by current path or only quoted files retained.
    let f = fixture(AnalyzerEpoch::V8);
    let bytes = prepared(&f);
    let baseline = store_state(&f);
    for mode in 0..3 {
        let mut wire: Value = serde_json::from_slice(&bytes).unwrap();
        let objects = wire["sourceObjects"].as_object_mut().unwrap();
        match mode {
            0 => {
                objects.remove(&hash(HISTORY_OLD)).unwrap();
            }
            1 => {
                objects.insert(hash(HISTORY_OLD), json!(HISTORY_NEW));
            }
            _ => {
                objects.insert(hash("orphan bytes"), json!("orphan bytes"));
            }
        }
        assert!(
            archive::parse(&serde_json::to_vec(&wire).unwrap(), Limits::default(), &d()).is_err(),
            "Non-exact frozen object set accepted for mode {mode}"
        );
    }
    assert_eq!(store_state(&f), baseline);
}

#[test]
fn archive_replay_rejects_publicly_resealed_source_view_and_meaning_changes() {
    // Break caught: validating only Meaning/canonical Graph, then returning unverified source View rows.
    let f = fixture(AnalyzerEpoch::V9);
    let wire: Value = serde_json::from_slice(&prepared(&f)).unwrap();
    let baseline = store_state(&f);
    assert!(
        archive::parse(&serde_json::to_vec(&wire).unwrap(), Limits::default(), &d()).is_ok(),
        "Equivalent Value encoding must reach proof validation before malicious cases"
    );
    let original_meaning = wire["archive"]["view"]["meaning"].clone();
    let original_proof = wire["archive"]["meaningArchive"].clone();
    for mode in 0..15 {
        let mut changed = wire.clone();
        match mode {
            0 => {
                changed["archive"]["view"]["meaning"]["packets"][0]["evidence"][0]["text"] =
                    json!("fabricated quotation")
            }
            1 => changed["archive"]["view"]["meaning"]["packets"]
                .as_array_mut()
                .unwrap()
                .iter_mut()
                .find(|p| !p["hyperedges"].as_array().unwrap().is_empty())
                .unwrap()["hyperedges"][0]["participants"]
                .as_array_mut()
                .unwrap()
                .swap(0, 1),
            2 => {
                changed["archive"]["view"]["meaning"]["scope"]["analyzerEpoch"] =
                    json!("yoke-tree-sitter-8")
            }
            3 => {
                changed["archive"]["view"]["meaning"]["packets"][0]["provenance"]["responseArtifactId"] =
                    json!("0".repeat(64))
            }
            4 => {
                changed["archive"]["view"]["meaning"]["packets"][0]["provenance"]["providerReportedCostMicros"] =
                    json!(0)
            }
            5 => {
                changed["archive"]["view"]["nodes"]
                    .as_array_mut()
                    .unwrap()
                    .pop()
                    .unwrap();
            }
            6 => changed["archive"]["view"]["nodes"][0]["name"] = json!("fabricated source label"),
            7 => changed["archive"]["view"]["observations"] = json!([]),
            8 => {
                changed["archive"]["view"]["observations"][0]["relation"] =
                    json!("fabricated_relation")
            }
            9 => changed["archive"]["view"]["evidence"] = json!([]),
            10 => {
                changed["archive"]["view"]["evidence"][0]["producer"] =
                    json!("fabricated-evidence-provider")
            }
            11 => {
                changed["archive"]["view"]["limitations"] =
                    json!(["fabricated completeness limitation"])
            }
            12 => changed["archive"]["view"]["complete"] = json!(true),
            13 => {
                changed["archive"]["view"]["nodes"][0]["producer"] =
                    json!("fabricated-node-provider")
            }
            _ => changed["archive"]["view"]["overlays"] = json!([]),
        }
        if mode >= 5 {
            assert_eq!(changed["archive"]["view"]["meaning"], original_meaning);
            assert_eq!(changed["archive"]["meaningArchive"], original_proof);
        }
        assert_ne!(
            changed, wire,
            "Mutation {mode} must actually change the input"
        );
        reseal_view(&mut changed);
        assert!(
            archive::parse(
                &serde_json::to_vec(&changed).unwrap(),
                Limits::default(),
                &d()
            )
            .is_err(),
            "Publicly resealed typed mismatch accepted for mode {mode}"
        );
    }
    assert_eq!(store_state(&f), baseline);
}

#[test]
fn portable_bridge_requires_complete_raw_closure_and_rejects_foreign_or_unpaired_sidecars() {
    // Break caught: new parser skips opaque proof replay or treats portable proof as foreign source authority.
    let f = fixture(AnalyzerEpoch::V8);
    let wire: Value = serde_json::from_slice(&prepared(&f)).unwrap();
    for mode in 0..10 {
        let mut changed = wire.clone();
        match mode {
            0 => {
                changed["archive"]
                    .as_object_mut()
                    .unwrap()
                    .remove("meaningArchive");
            }
            1 => {
                changed["archive"]["view"]
                    .as_object_mut()
                    .unwrap()
                    .remove("meaning");
            }
            2..=4 => {
                let kind = match mode {
                    2 => "native_model_attempt_v1",
                    3 => "native_meaning_job_v1",
                    _ => "native_meaning_cache_v1",
                };
                let rows = changed["archive"]["meaningArchive"]["proof"]["rows"]
                    .as_array_mut()
                    .unwrap();
                let position = rows.iter().position(|r| r["key"]["kind"] == kind).unwrap();
                rows.remove(position);
            }
            5 => changed["snapshot"]["workspaceId"] = json!("0".repeat(64)),
            6 => changed["sourceAuthority"] = json!({"kind":"federatedSources","binding":{}}),
            7 => {
                let selected_job =
                    changed["archive"]["view"]["meaning"]["selections"][0]["jobId"].clone();
                let row = changed["archive"]["meaningArchive"]["proof"]["rows"]
                    .as_array_mut()
                    .unwrap()
                    .iter_mut()
                    .find(|r| {
                        r["key"]["kind"] == "native_meaning_job_v1"
                            && r["key"]["id"] == selected_job
                    })
                    .unwrap();
                let mut job: Value =
                    serde_json::from_str(row["payload"].as_str().unwrap()).unwrap();
                job["packets"][0]["originJobId"] = selected_job;
                row["payload"] = json!(serde_json::to_string(&job).unwrap());
            }
            8 => {
                changed["archive"]["meaningArchive"]["proof"]["sourceManifests"][0]["files"][0]["path"] =
                    json!("../outside.rs")
            }
            _ => {
                let rows = changed["archive"]["meaningArchive"]["proof"]["rows"]
                    .as_array_mut()
                    .unwrap();
                let row = rows[0].clone();
                *rows = vec![row; 4097];
            }
        }
        assert!(
            archive::parse(
                &serde_json::to_vec(&changed).unwrap(),
                Limits::default(),
                &d()
            )
            .is_err(),
            "Missing/foreign typed closure accepted for mode {mode}"
        );
    }
}

#[test]
fn portable_wire_rejects_unknown_nested_properties_duplicate_keys_and_unknown_profiles() {
    // Break caught: lossy Snapshot/Graph deserialization silently strips unknown input before proof validation.
    let f = fixture(AnalyzerEpoch::V9);
    let bytes = prepared(&f);
    let wire: Value = serde_json::from_slice(&bytes).unwrap();
    for mode in 0..8 {
        let mut changed = wire.clone();
        match mode {
            0 => changed["unknown"] = json!(true),
            1 => changed["snapshot"]["unknown"] = json!(true),
            2 => changed["snapshot"]["files"][0]["unknown"] = json!(true),
            3 => changed["snapshot"]["omissions"][0]["unknown"] = json!(true),
            4 => changed["archive"]["canonicalGraph"]["entities"][0]["unknown"] = json!(true),
            5 => changed["archive"]["meaningArchive"]["unknown"] = json!(true),
            6 => {
                changed["archive"]["view"]["meaning"]["packets"][0]["provenance"]["unknown"] =
                    json!(true)
            }
            _ => changed["profile"] = json!("meaning_view_archive_v2"),
        }
        assert!(
            archive::parse(
                &serde_json::to_vec(&changed).unwrap(),
                Limits::default(),
                &d()
            )
            .is_err(),
            "mode {mode}"
        );
    }
    let raw = std::str::from_utf8(&bytes).unwrap();
    assert!(raw.starts_with('{'));
    let duplicate = format!("{{\"profile\":\"meaning_view_archive_v1\",{}", &raw[1..]);
    assert!(archive::parse(duplicate.as_bytes(), Limits::default(), &d()).is_err());
}

#[test]
fn complete_wire_and_distinct_source_limits_are_admitted_without_store_side_effects() {
    // Break caught: charging excerpts/metadata only, double-counting repeated hashes, or admitting after writes.
    let f = fixture(AnalyzerEpoch::V8);
    let baseline = store_state(&f);
    let bytes = prepared(&f);
    let wire: Value = serde_json::from_slice(&bytes).unwrap();
    let unique_bytes: usize = wire["sourceObjects"]
        .as_object()
        .unwrap()
        .values()
        .map(|v| v.as_str().unwrap().len())
        .sum();
    let exact = Limits {
        max_wire_bytes: bytes.len(),
        max_source_bytes: unique_bytes,
    };
    assert!(archive::parse(&bytes, exact, &d()).is_ok());
    for limits in [
        Limits {
            max_wire_bytes: bytes.len() - 1,
            ..exact
        },
        Limits {
            max_source_bytes: unique_bytes - 1,
            ..exact
        },
        Limits {
            max_wire_bytes: 0,
            ..exact
        },
        Limits {
            max_source_bytes: 64_000_001,
            ..exact
        },
    ] {
        assert!(archive::parse(&bytes, limits, &d()).is_err());
        assert!(archive::prepare(&f.engine.store, &f.view, limits, &d()).is_err());
    }
    assert!(archive::parse(&bytes, exact, &Deadline::new(0)).is_err());
    assert!(archive::prepare(&f.engine.store, &f.view, exact, &Deadline::new(0)).is_err());
    assert_eq!(store_state(&f), baseline);
}

#[test]
fn portable_transport_keeps_task_totals_current_attempt_usage_and_original_unknown_response_distinct()
 {
    // Break caught: bridge substitutes a later task/attempt aggregate for original published response metadata.
    let f = fixture(AnalyzerEpoch::V9);
    let saved = f.view.meaning.as_ref().unwrap().clone();
    let deadline = d();
    let ledger = yoke::ledger::Ledger::new(&f.engine.store, &deadline);
    let unrelated = ledger
        .reserve(
            &f.provider.task_id,
            90,
            f.provider.task_limit_micros,
            &f.provider.currency,
            &f.provider.model_key().unwrap(),
        )
        .unwrap();
    ledger.mark_sent(&unrelated).unwrap();
    ledger.settle(&unrelated, Some(90)).unwrap();
    let original_attempt = &saved.packets[0].provenance.attempt_id;
    ledger.settle(original_attempt, Some(8)).unwrap();
    ledger
        .usage(
            original_attempt,
            "later-host-report",
            yoke::ledger::Usage {
                input_tokens: 12_345,
                output_tokens: 678,
                cached_input_tokens: 12,
            },
        )
        .unwrap();
    let baseline = store_state(&f);
    let bytes = prepared(&f);
    let wire: Value = serde_json::from_slice(&bytes).unwrap();
    let rows = wire["archive"]["meaningArchive"]["proof"]["rows"]
        .as_array()
        .unwrap();
    let task = rows
        .iter()
        .find(|r| r["key"]["kind"] == "native_model_task_v1")
        .unwrap();
    let task: Value = serde_json::from_str(task["payload"].as_str().unwrap()).unwrap();
    assert_eq!(task["totals"]["actualMicros"], 98);
    let attempt = rows
        .iter()
        .find(|r| {
            r["key"]["kind"] == "native_model_attempt_v1" && r["key"]["id"] == *original_attempt
        })
        .unwrap();
    let attempt: Value = serde_json::from_str(attempt["payload"].as_str().unwrap()).unwrap();
    assert_eq!(attempt["usage"]["inputTokens"], 12_345);
    let receipts = wire["archive"]["meaningArchive"]["proof"]["artifacts"]
        .as_array()
        .unwrap();
    let receipt = receipts
        .iter()
        .find(|a| a["artifactId"] == saved.packets[0].provenance.response_artifact_id)
        .unwrap();
    let receipt: Value = serde_json::from_str(receipt["payload"].as_str().unwrap()).unwrap();
    assert!(receipt["actualCostMicros"].is_null());
    assert!(receipt["usage"].is_null());
    let replay = archive::parse(&bytes, Limits::default(), &d()).unwrap();
    assert_eq!(replay.view().meaning.as_ref().unwrap(), &saved);
    assert!(
        replay
            .view()
            .meaning
            .as_ref()
            .unwrap()
            .packets
            .iter()
            .all(|p| p.provenance.provider_reported_cost_micros.is_none()
                && p.provenance.provider_reported_usage.is_none())
    );
    assert_eq!(store_state(&f), baseline);
}

#[test]
fn published_id_preparation_returns_exact_checked_transport_without_load_or_publication() {
    // Break caught: published export loads unguarded CAS, loses full View/IDs, or publishes while preparing.
    for epoch in [AnalyzerEpoch::V8, AnalyzerEpoch::V9] {
        let f = fixture(epoch);
        let baseline = store_state(&f);
        let expected = prepared(&f);
        let checked =
            archive::prepare_published(&f.engine.store, &f.view.id, Limits::default(), &d())
                .expect("A genuine published selection needs guarded preparation by ID");
        assert_eq!(checked.bytes(), expected);
        assert_eq!(checked.archive_id(), hash(&expected));
        assert_eq!(checked.view().id, f.view.id);
        assert_eq!(checked.view().artifact_id, f.view.artifact_id);
        assert_eq!(
            serde_json::to_value(checked.view()).unwrap(),
            serde_json::to_value(&f.view).unwrap()
        );
        assert_eq!(identity(checked.canonical_graph()), identity(&f.graph));
        assert_eq!(store_state(&f), baseline);
    }
}
