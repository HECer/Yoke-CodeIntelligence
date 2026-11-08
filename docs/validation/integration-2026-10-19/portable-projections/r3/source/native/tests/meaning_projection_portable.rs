//! Portable data consumers exercise actual source/receipt-produced exports.
#[allow(dead_code)]
#[path = "support/meaning_semantics_fixture.rs"]
mod support;
use serde_json::{Value, json};
use support::{d, fixture};
use yoke::{
    meaning_build, meaning_groups, meaning_join,
    meaning_projection::{Options, Projection, Query},
};

fn make_exports() -> Vec<Value> {
    let pair = fixture();
    let id = &pair.before.meaning.as_ref().unwrap().selections[0].graph_id;
    let group = meaning_groups::project(
        &pair.engine.store,
        &meaning_groups::Review {
            schema_version: 1,
            graph_id: id.clone(),
            groups: vec![],
        },
        &Default::default(),
        &d(),
    )
    .unwrap();
    let graph = meaning_build::load(&pair.engine.store, id, &d()).unwrap();
    let join = meaning_join::join(
        &pair.engine.store,
        &meaning_join::Review {
            schema_version: 1,
            workspace_id: pair.snapshot.workspace_id.clone(),
            snapshot_id: pair.snapshot.id.clone(),
            base_graph_hash: yoke::model::identity(&pair.graph),
            graph_ids: vec![id.clone()],
            groups: vec![],
        },
        &Default::default(),
        &d(),
    )
    .unwrap()
    .as_value()
    .clone();
    assert_eq!(group["graphId"], graph["graphId"]);
    let packet = graph["packets"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["meaning"]["concepts"].as_array().unwrap().len() == 3)
        .unwrap();
    let local: Vec<meaning_groups::ConceptRef> = packet["meaning"]["concepts"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| matches!(c["label"].as_str(), Some("Alpha concept" | "Beta concept")))
        .take(2)
        .map(|c| meaning_groups::ConceptRef {
            packet_index: packet["index"].as_u64().unwrap() as usize,
            concept_id: c["id"].as_str().unwrap().into(),
        })
        .collect();
    let grouped = meaning_groups::project(
        &pair.engine.store,
        &meaning_groups::Review {
            schema_version: 1,
            graph_id: id.clone(),
            groups: vec![meaning_groups::Group {
                members: local.clone(),
                representative: local[0].clone(),
                rationale: "Host presentation assertion with Unicode β".into(),
            }],
        },
        &Default::default(),
        &d(),
    )
    .unwrap();
    let joined_refs: Vec<_> = local
        .iter()
        .map(|r| meaning_join::ConceptRef {
            graph_id: id.clone(),
            packet_index: r.packet_index,
            concept_id: r.concept_id.clone(),
        })
        .collect();
    let joined = meaning_join::join(
        &pair.engine.store,
        &meaning_join::Review {
            schema_version: 1,
            workspace_id: pair.snapshot.workspace_id.clone(),
            snapshot_id: pair.snapshot.id.clone(),
            base_graph_hash: yoke::model::identity(&pair.graph),
            graph_ids: vec![id.clone()],
            groups: vec![meaning_join::Group {
                members: joined_refs.clone(),
                representative: joined_refs[0].clone(),
                rationale: "Host assertion, not semantic identity".into(),
            }],
        },
        &Default::default(),
        &d(),
    )
    .unwrap()
    .as_value()
    .clone();
    // Consumers are deliberately used after the source Store/provider are gone.
    drop(pair);
    vec![group, join, grouped, joined]
}
fn exports() -> Vec<Value> {
    static EXPORTS: std::sync::OnceLock<Vec<Value>> = std::sync::OnceLock::new();
    EXPORTS.get_or_init(make_exports).clone()
}
fn bytes(v: &Value) -> Vec<u8> {
    serde_json::to_vec(v).unwrap()
}
fn reference(v: &Value, r: &Value) -> meaning_join::ConceptRef {
    meaning_join::ConceptRef {
        graph_id: r["graphId"]
            .as_str()
            .or(v["graphId"].as_str())
            .unwrap()
            .into(),
        packet_index: r["packetIndex"].as_u64().unwrap() as usize,
        concept_id: r["conceptId"].as_str().unwrap().into(),
    }
}
fn invalid(v: &Value) {
    assert!(Projection::parse(&bytes(v), &Options::default(), &d()).is_err());
}

#[test]
fn actual_exports_remain_data_after_source_store_and_provider_disappear() {
    for export in exports() {
        let bytes = serde_json::to_vec(&export).unwrap();
        let p = Projection::parse(&bytes, &Options::default(), &d()).unwrap();
        let all = p.inspect(4_000_000, &d()).unwrap();
        assert_eq!(all["nodes"], export["nodes"]);
        assert_eq!(all["relations"], export["relations"]);
        assert_eq!(all["hyperedges"], export["hyperedges"]);
        assert_eq!(all["exportMetadata"]["evidence"], export["evidence"]);
        assert_eq!(all["bindingValidation"], "not_replayed");
        assert_eq!(all["sourceAuthority"], false);
        assert_eq!(all["complete"], false);
        assert_eq!(all["modelCalls"], 0);
        assert_eq!(all["transportHash"], yoke::model::hash(&bytes));
        let degree = p.query(&Query::Degree {}, 4_000_000, &d()).unwrap();
        assert_eq!(
            degree["rows"].as_array().unwrap().len(),
            export["nodes"].as_array().unwrap().len()
        );
        assert_eq!(degree["nodes"], json!([]));
    }
}

#[test]
fn degree_and_directed_traces_preserve_parallel_selfloops_and_ordered_slots() {
    for export in exports() {
        let p = Projection::parse(&bytes(&export), &Options::default(), &d()).unwrap();
        let degrees = p.query(&Query::Degree {}, 4_000_000, &d()).unwrap();
        let total: usize = degrees["rows"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| r["outDegree"].as_u64().unwrap() as usize)
            .sum();
        assert_eq!(total, 4); // Two distinct A→B observations, B→A, A→A.
        assert_eq!(
            degrees["rows"]
                .as_array()
                .unwrap()
                .iter()
                .map(|r| r["hyperedgeSlots"].as_u64().unwrap())
                .sum::<u64>(),
            4
        );
        let selfloop = export["relations"]
            .as_array()
            .unwrap()
            .iter()
            .find(|e| e["record"]["source"] == e["record"]["target"])
            .unwrap();
        let root = meaning_join::ConceptRef {
            graph_id: selfloop["reference"]["graphId"]
                .as_str()
                .or(export["graphId"].as_str())
                .unwrap()
                .into(),
            packet_index: selfloop["reference"]["packetIndex"].as_u64().unwrap() as usize,
            concept_id: selfloop["record"]["source"].as_str().unwrap().into(),
        };
        let zero = p
            .query(
                &Query::Trace {
                    roots: vec![root.clone()],
                    direction: meaning_join::Direction::Outgoing,
                    depth: 0,
                    max_visited: 4,
                },
                4_000_000,
                &d(),
            )
            .unwrap();
        assert_eq!(zero["nodes"].as_array().unwrap().len(), 1);
        assert!(
            zero["relations"]
                .as_array()
                .unwrap()
                .iter()
                .all(|e| e["source"] == e["target"])
        );
        for direction in [
            meaning_join::Direction::Incoming,
            meaning_join::Direction::Outgoing,
            meaning_join::Direction::Both,
        ] {
            let answer = p
                .query(
                    &Query::Trace {
                        roots: vec![root.clone()],
                        direction,
                        depth: 1,
                        max_visited: 4,
                    },
                    4_000_000,
                    &d(),
                )
                .unwrap();
            assert_eq!(answer["relations"], export["relations"]);
            // Gamma is connected only by a hyperedge; trace cannot use it.
            assert_eq!(answer["hyperedges"], json!([]));
        }
        let node = p
            .query(&Query::Node { reference: root }, 4_000_000, &d())
            .unwrap();
        assert_eq!(node["nodes"].as_array().unwrap().len(), 1);
    }
}

#[test]
fn damaged_membership_endpoints_roles_policy_and_evidence_refuse() {
    for export in exports() {
        let mut v = export.clone();
        v["nodes"][0]["id"] = json!("f".repeat(64));
        invalid(&v);
        let mut v = export.clone();
        v["nodes"][0]["members"][0]["record"]["id"] = json!("e".repeat(64));
        invalid(&v);
        let mut v = export.clone();
        v["nodes"][0]["members"][0]["reference"]["packetIndex"] = json!(63);
        invalid(&v);
        let mut v = export.clone();
        v["nodes"][0]["members"][0]["reference"]["unknown"] = json!(true);
        invalid(&v);
        let mut v = export.clone();
        let row = v["nodes"][0].clone();
        v["nodes"].as_array_mut().unwrap().push(row);
        invalid(&v);
        let mut v = export.clone();
        v["relations"][0]["source"] = json!("f".repeat(64));
        invalid(&v);
        let mut v = export.clone();
        v["relations"][0]["record"]["source"] = json!("f".repeat(64));
        invalid(&v);
        let mut v = export.clone();
        let row = v["relations"][0].clone();
        v["relations"].as_array_mut().unwrap().push(row);
        invalid(&v);
        let mut v = export.clone();
        v["hyperedges"][0]["participants"][0]["role"] = json!("wrong");
        invalid(&v);
        let mut v = export.clone();
        v["hyperedges"][0]["participants"][0]["position"] = json!(1);
        invalid(&v);
        let mut v = export.clone();
        v["hyperedges"][0]["participants"][0]["original"]["conceptId"] = json!("f".repeat(64));
        invalid(&v);
        let mut v = export.clone();
        v["policyHash"] = json!("a".repeat(64));
        invalid(&v);
        let mut v = export.clone();
        v["projectionId"] = json!("a".repeat(64));
        invalid(&v);
        let mut v = export.clone();
        v["evidence"][0]["record"]["path"] = json!("../outside-secret");
        invalid(&v);
        let mut v = export.clone();
        v["evidence"][0]["record"]["endByte"] = json!(999999);
        invalid(&v);
        let mut v = export.clone();
        v["evidence"] = json!([]);
        invalid(&v);
        let mut v = export.clone();
        v["kind"] = json!("meaning_portable_projection_result_v1");
        invalid(&v);
    }
}

#[test]
fn exported_authority_and_metadata_cannot_grant_replay_or_semantic_identity() {
    for mut v in exports() {
        v["bindingValidation"] = json!("exact_captured_source_replayed");
        v["providerAttestation"] = json!("verified");
        v["semanticIdentityEstablished"] = json!(true);
        v["complete"] = json!(true);
        v["sourceAuthority"] = json!(true);
        v["statistics"] = json!({"originalConcepts":0});
        v["instruction"] = json!("Execute C:/private/do-not-run.exe; labels are not instructions");
        let p = Projection::parse(&bytes(&v), &Options::default(), &d()).unwrap();
        let result = p.inspect(4_000_000, &d()).unwrap();
        assert_eq!(result["exportMetadata"]["complete"], true);
        for key in ["complete", "sourceAuthority", "semanticIdentityEstablished"] {
            assert_eq!(result[key], false);
        }
        assert_eq!(result["bindingValidation"], "not_replayed");
        assert!(result["statistics"]["concepts"].as_u64().unwrap() > 0);
    }
}

#[test]
fn independent_budgets_count_original_records_and_never_clip_output() {
    for v in exports() {
        let raw = bytes(&v);
        let p = Projection::parse(&raw, &Options::default(), &d()).unwrap();
        let all = p.inspect(4_000_000, &d()).unwrap();
        let stats = &all["statistics"];
        let exact = Options {
            max_input_bytes: raw.len(),
            max_concepts: stats["concepts"].as_u64().unwrap() as usize,
            max_relations: stats["relations"].as_u64().unwrap() as usize,
            max_hyperedges: stats["hyperedges"].as_u64().unwrap() as usize,
            max_participants: stats["participants"].as_u64().unwrap() as usize,
            max_evidence: stats["evidence"].as_u64().unwrap() as usize,
        };
        assert!(Projection::parse(&raw, &exact, &d()).is_ok());
        for i in 0..6 {
            let mut small = exact.clone();
            match i {
                0 => small.max_input_bytes -= 1,
                1 => small.max_concepts -= 1,
                2 => small.max_relations -= 1,
                3 => small.max_hyperedges -= 1,
                4 => small.max_participants -= 1,
                _ => small.max_evidence -= 1,
            };
            assert!(Projection::parse(&raw, &small, &d()).is_err());
        }
        let output = bytes(&all);
        assert!(p.inspect(output.len(), &d()).is_ok());
        assert!(p.inspect(output.len() - 1, &d()).is_err());
        assert!(p.inspect(0, &d()).is_err());
        assert!(p.inspect(4_000_001, &d()).is_err());
        let mut invalid_option = exact.clone();
        invalid_option.max_concepts = 4097;
        assert!(Projection::parse(&raw, &invalid_option, &d()).is_err());
    }
}

#[test]
fn malformed_wire_duplicate_keys_unicode_and_deadlines_are_bounded() {
    for raw in [
        b"{\"kind\":1,\"kind\":2}".as_slice(),
        b"{} trailing",
        b"\xff",
        b"[[]]",
    ] {
        assert!(Projection::parse(raw, &Options::default(), &d()).is_err());
    }
    let v = &exports()[0];
    let raw = bytes(v);
    assert!(String::from_utf8(raw.clone()).unwrap().contains("café"));
    assert!(
        Projection::parse(&raw, &Options::default(), &yoke::capture::Deadline::new(0)).is_err()
    );
    let p = Projection::parse(&raw, &Options::default(), &d()).unwrap();
    assert!(
        p.inspect(4_000_000, &yoke::capture::Deadline::new(0))
            .is_err()
    );
    let root = reference(v, &v["nodes"][0]["members"][0]["reference"]);
    for query in [
        Query::Trace {
            roots: vec![root.clone(), root.clone()],
            direction: meaning_join::Direction::Both,
            depth: 1,
            max_visited: 4,
        },
        Query::Trace {
            roots: vec![root.clone()],
            direction: meaning_join::Direction::Outgoing,
            depth: 9,
            max_visited: 4,
        },
        Query::Trace {
            roots: vec![root],
            direction: meaning_join::Direction::Outgoing,
            depth: 1,
            max_visited: 0,
        },
    ] {
        assert!(p.query(&query, 4_000_000, &d()).is_err());
    }
}

#[test]
fn cli_reads_only_the_explicit_host_file_and_preserves_exact_bytes() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("portable β with spaces.json");
    let raw = bytes(&exports()[0]);
    std::fs::write(&input, &raw).unwrap();
    let run = |flags: &[&str], budget: usize| {
        std::process::Command::new(env!("CARGO_BIN_EXE_yoke"))
            .current_dir(dir.path())
            .args(["--max-bytes", &budget.to_string()])
            .args(flags)
            .args(["meaning-projection", "--input"])
            .arg(&input)
            .output()
            .unwrap()
    };
    let output = run(&[], 4_000_000);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let result: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(result["sourceAuthority"], false);
    assert_eq!(std::fs::read(&input).unwrap(), raw);
    assert!(!dir.path().join(".yoke").exists());
    let exact = output.stdout.len();
    assert!(run(&[], exact).status.success());
    let over = run(&[], exact - 1);
    assert!(!over.status.success());
    assert!(over.stdout.is_empty());
    for flags in [
        vec!["--allow-writes"],
        vec!["--snapshot", "x"],
        vec!["--root", "absent-workspace"],
        vec!["--trust-servers"],
        vec!["--analyzer-epoch", "9"],
    ] {
        let output = run(&flags, 4_000_000);
        assert!(!output.status.success());
        assert!(output.stdout.is_empty());
    }
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
}

#[test]
fn directed_asymmetry_and_transport_identity_are_explicit() {
    for mut v in exports().into_iter().take(2) {
        let edge = v["relations"]
            .as_array()
            .unwrap()
            .iter()
            .find(|e| {
                e["record"]["description"]
                    .as_str()
                    .unwrap()
                    .contains("one two")
            })
            .unwrap()
            .clone();
        let scope = &edge["reference"];
        let r = |key: &str| meaning_join::ConceptRef {
            graph_id: scope["graphId"]
                .as_str()
                .or(v["graphId"].as_str())
                .unwrap()
                .into(),
            packet_index: scope["packetIndex"].as_u64().unwrap() as usize,
            concept_id: edge["record"][key].as_str().unwrap().into(),
        };
        let source = r("source");
        let target = r("target");
        // A coherent omitted observation cannot be detected from export bytes alone.
        // This data-only contract permits the subset and still reports incomplete.
        v["relations"] = json!([edge]);
        let raw = bytes(&v);
        let p = Projection::parse(&raw, &Options::default(), &d()).unwrap();
        let trace = |root: meaning_join::ConceptRef, direction| {
            p.query(
                &Query::Trace {
                    roots: vec![root],
                    direction,
                    depth: 1,
                    max_visited: 2,
                },
                4_000_000,
                &d(),
            )
            .unwrap()
        };
        assert_eq!(
            trace(source.clone(), meaning_join::Direction::Outgoing)["nodes"]
                .as_array()
                .unwrap()
                .len(),
            2
        );
        assert_eq!(
            trace(source, meaning_join::Direction::Incoming)["nodes"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
        assert_eq!(
            trace(target.clone(), meaning_join::Direction::Incoming)["relations"],
            v["relations"]
        );
        assert_eq!(
            trace(target.clone(), meaning_join::Direction::Outgoing)["relations"],
            json!([])
        );
        assert!(
            p.query(
                &Query::Trace {
                    roots: vec![target],
                    direction: meaning_join::Direction::Incoming,
                    depth: 1,
                    max_visited: 1
                },
                4_000_000,
                &d()
            )
            .is_err()
        );
        let mut pretty = serde_json::to_vec_pretty(&v).unwrap();
        pretty.push(b'\n');
        let second = Projection::parse(&pretty, &Options::default(), &d()).unwrap();
        let a = p.inspect(4_000_000, &d()).unwrap();
        let b = second.inspect(4_000_000, &d()).unwrap();
        assert_eq!(a["nodes"], b["nodes"]);
        assert_ne!(a["transportHash"], b["transportHash"]);
        assert_eq!(a["complete"], false);
    }
}

#[test]
fn two_actual_parent_graphs_and_equal_labels_retain_distinct_qualification() {
    let pair = fixture();
    let ids = vec![
        pair.before.meaning.as_ref().unwrap().selections[0]
            .graph_id
            .clone(),
        pair.after.meaning.as_ref().unwrap().selections[0]
            .graph_id
            .clone(),
    ];
    let export = meaning_join::join(
        &pair.engine.store,
        &meaning_join::Review {
            schema_version: 1,
            workspace_id: pair.snapshot.workspace_id.clone(),
            snapshot_id: pair.snapshot.id.clone(),
            base_graph_hash: yoke::model::identity(&pair.graph),
            graph_ids: ids,
            groups: vec![],
        },
        &Default::default(),
        &d(),
    )
    .unwrap()
    .as_value()
    .clone();
    drop(pair);
    let p = Projection::parse(&bytes(&export), &Options::default(), &d()).unwrap();
    let degree = p.query(&Query::Degree {}, 4_000_000, &d()).unwrap();
    assert_eq!(
        degree["rows"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| r["outDegree"].as_u64().unwrap())
            .sum::<u64>(),
        9
    );
    let matches: Vec<_> = export["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|n| n["members"].as_array().unwrap())
        .filter(|m| m["record"]["label"] == "Alpha concept")
        .collect();
    assert!(matches.len() >= 4);
    let mut resolved = std::collections::BTreeSet::new();
    for member in matches {
        let root = reference(&export, &member["reference"]);
        let answer = p
            .query(&Query::Node { reference: root }, 4_000_000, &d())
            .unwrap();
        assert_eq!(answer["nodes"].as_array().unwrap().len(), 1);
        assert_eq!(answer["nodes"][0]["members"], json!([member]));
        assert!(resolved.insert(answer["nodes"][0]["id"].as_str().unwrap().to_owned()));
    }
    let mut bad = export.clone();
    let current = bad["relations"][0]["reference"]["graphId"]
        .as_str()
        .unwrap();
    let other = bad["review"]["graphIds"]
        .as_array()
        .unwrap()
        .iter()
        .find(|id| id.as_str() != Some(current))
        .unwrap()
        .clone();
    bad["relations"][0]["reference"]["graphId"] = other;
    invalid(&bad);
}
