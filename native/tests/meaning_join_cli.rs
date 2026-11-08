//! Cross-graph host projections over actual published source/receipt closures.
#[allow(dead_code)] // Shared comparison support also exposes archive-only helpers.
#[path = "support/meaning_semantics_fixture.rs"]
mod support;
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    fs,
    path::Path,
    process::{Command, Output},
};
use support::{MeaningPair, d, fixture};
use yoke::{
    meaning_build,
    model::{hash, identity},
};

struct Input {
    pair: MeaningPair,
    parents: Vec<Value>,
    review: Value,
    file: std::path::PathBuf,
}
fn reference(graph: &Value, label: &str) -> Value {
    let packet = graph["packets"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["meaning"]["concepts"].as_array().unwrap().len() == 3)
        .unwrap();
    let concept = packet["meaning"]["concepts"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["label"] == label)
        .unwrap();
    json!({"graphId":graph["graphId"],"packetIndex":packet["index"],"conceptId":concept["id"]})
}
fn input() -> Input {
    let pair = fixture();
    let ids: Vec<_> = [&pair.before, &pair.after]
        .iter()
        .map(|v| v.meaning.as_ref().unwrap().selections[0].graph_id.clone())
        .collect();
    let parents: Vec<_> = ids
        .iter()
        .map(|id| meaning_build::load(&pair.engine.store, id, &d()).unwrap())
        .collect();
    let a = reference(&parents[0], "Alpha concept");
    let b = reference(&parents[0], "Beta concept");
    let second_a = reference(&parents[1], "Alpha concept");
    let review = json!({"schemaVersion":1,"workspaceId":pair.snapshot.workspace_id,
        "snapshotId":pair.snapshot.id,"baseGraphHash":identity(&pair.graph),"graphIds":ids,
        "groups":[{"members":[a.clone(),b,second_a],"representative":a,
            "rationale":"Explicit host assertion across two inferred graph generations"}]});
    let file = pair.dir.path().join("join-review.json");
    fs::write(&file, serde_json::to_vec(&review).unwrap()).unwrap();
    Input {
        pair,
        parents,
        review,
        file,
    }
}
fn command(root: &Path, file: &Path, max_bytes: usize, globals: &[&str]) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_yoke"));
    command
        .arg("--root")
        .arg(root)
        .arg("--timeout")
        .arg("30000")
        .arg("--max-bytes")
        .arg(max_bytes.to_string())
        .args(globals)
        .args(["meaning", "join", "--review"])
        .arg(file);
    command
}
fn run(i: &Input, options: &[&str], query: Option<Value>) -> Output {
    let mut c = command(&i.pair.engine.root, &i.file, 4_000_000, &[]);
    c.args(options);
    if let Some(query) = query {
        c.arg("--query").arg(serde_json::to_string(&query).unwrap());
    }
    c.output().unwrap()
}
fn success(output: Output) -> Value {
    assert!(
        output.status.success(),
        "stderr:{}\nstdout:{}",
        String::from_utf8_lossy(&output.stderr),
        String::from_utf8_lossy(&output.stdout)
    );
    assert!(output.stdout.ends_with(b"\n"));
    serde_json::from_slice(&output.stdout).unwrap()
}
fn refused(output: Output, marker: &str) {
    assert!(
        !output.status.success(),
        "Refused operation unexpectedly succeeded"
    );
    assert!(output.stdout.is_empty(), "Refusal published partial stdout");
    assert!(
        String::from_utf8_lossy(&output.stderr).contains(marker),
        "Expected {marker}, stderr:{}",
        String::from_utf8_lossy(&output.stderr)
    );
}
fn state(i: &Input) -> Value {
    let db = &i.pair.engine.store.db;
    let tables: Vec<String> = db.prepare("SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%' ORDER BY name").unwrap()
        .query_map([],|r| r.get(0)).unwrap().collect::<rusqlite::Result<_>>().unwrap();
    let mut sql = BTreeMap::new();
    for table in tables {
        let query = format!("SELECT * FROM \"{}\"", table.replace('"', "\"\""));
        let mut statement = db.prepare(&query).unwrap();
        let count = statement.column_count();
        let mut rows = statement
            .query_map([], |r| {
                (0..count)
                    .map(|n| Ok(format!("{:?}", r.get_ref(n)?)))
                    .collect::<rusqlite::Result<Vec<_>>>()
            })
            .unwrap()
            .collect::<rusqlite::Result<Vec<_>>>()
            .unwrap();
        rows.sort();
        sql.insert(table, rows);
    }
    fn files(root: &Path, path: &Path, rows: &mut BTreeMap<String, String>) {
        for entry in fs::read_dir(path).unwrap() {
            let path = entry.unwrap().path();
            let metadata = fs::symlink_metadata(&path).unwrap();
            assert!(!metadata.file_type().is_symlink());
            if metadata.is_dir() {
                files(root, &path, rows);
            } else {
                rows.insert(
                    path.strip_prefix(root).unwrap().to_str().unwrap().into(),
                    hash(fs::read(path).unwrap()),
                );
            }
        }
    }
    let mut cas = BTreeMap::new();
    for name in ["blobs", "trees"] {
        files(
            &i.pair.engine.store.dir,
            &i.pair.engine.store.dir.join(name),
            &mut cas,
        );
    }
    json!({"sql":sql,"cas":cas,"providerCalls":fs::read(&i.pair.marker).unwrap()})
}
fn grouped_node(report: &Value) -> &Value {
    report["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|n| n["members"].as_array().unwrap().len() == 3)
        .unwrap()
}

#[test]
fn joined_graphs_retain_all_original_claims_directed_copies_slots_and_receipts_without_live_source()
{
    let i = input();
    fs::remove_file(i.pair.engine.root.join("a.rs")).unwrap();
    fs::remove_file(i.pair.engine.root.join("b.rs")).unwrap();
    let before = state(&i);
    let report = success(run(&i, &[], None));
    assert_eq!(report["kind"], "meaning_join_projection_v1");
    assert_eq!(report["nodes"].as_array().unwrap().len(), 8);
    assert_eq!(report["relations"].as_array().unwrap().len(), 9);
    assert_eq!(report["hyperedges"].as_array().unwrap().len(), 2);
    assert_eq!(report["parents"].as_array().unwrap().len(), 2);
    assert_eq!(
        report["relations"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|r| r["source"] == r["target"])
            .count(),
        6
    );
    assert_eq!(
        report["hyperedges"]
            .as_array()
            .unwrap()
            .iter()
            .map(|h| h["participants"].as_array().unwrap().len())
            .sum::<usize>(),
        8
    );
    assert_eq!(
        grouped_node(&report)["members"].as_array().unwrap().len(),
        3
    );
    for parent in &i.parents {
        let proof = report["parents"]
            .as_array()
            .unwrap()
            .iter()
            .find(|p| p["graphId"] == parent["graphId"])
            .unwrap();
        assert_eq!(proof["snapshotId"], parent["snapshotId"]);
        assert_eq!(proof["jobId"], parent["jobId"]);
        assert_eq!(proof["graphHash"], parent["graphHash"]);
        assert_eq!(proof["coverage"], parent["coverage"]);
        for packet in parent["packets"].as_array().unwrap() {
            let p = proof["packets"]
                .as_array()
                .unwrap()
                .iter()
                .find(|p| p["packetIndex"] == packet["index"])
                .unwrap();
            for (output, input) in [
                ("inputHash", &packet["inputHash"]),
                ("provenance", &packet["provenance"]),
                ("sourceBindings", &packet["meaning"]["sourceBindings"]),
                ("coverage", &packet["meaning"]["coverage"]),
            ] {
                assert_eq!(&p[output], input);
            }
            for concept in packet["meaning"]["concepts"].as_array().unwrap() {
                let members: Vec<_> = report["nodes"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .flat_map(|n| n["members"].as_array().unwrap())
                    .filter(|m| {
                        m["reference"]["graphId"] == parent["graphId"]
                            && m["reference"]["packetIndex"] == packet["index"]
                            && m["reference"]["conceptId"] == concept["id"]
                    })
                    .collect();
                assert_eq!(members.len(), 1);
                assert_eq!(members[0]["record"], *concept);
            }
            for (collection, id_field) in
                [("relations", "relationId"), ("hyperedges", "hyperedgeId")]
            {
                for original in packet["meaning"][collection].as_array().unwrap() {
                    let projected: Vec<_> = report[collection]
                        .as_array()
                        .unwrap()
                        .iter()
                        .filter(|r| {
                            r["reference"]["graphId"] == parent["graphId"]
                                && r["reference"]["packetIndex"] == packet["index"]
                                && r["reference"][id_field] == original["id"]
                        })
                        .collect();
                    assert_eq!(projected.len(), 1);
                    assert_eq!(projected[0]["record"], *original);
                    if collection == "hyperedges" {
                        let slots = projected[0]["participants"].as_array().unwrap();
                        let originals = original["participants"].as_array().unwrap();
                        assert_eq!(slots.len(), originals.len());
                        for (position, (slot, original)) in slots.iter().zip(originals).enumerate()
                        {
                            assert_eq!(slot["position"], position);
                            assert_eq!(slot["role"], original["role"]);
                            assert_eq!(slot["original"]["graphId"], parent["graphId"]);
                            assert_eq!(slot["original"]["conceptId"], original["concept"]);
                        }
                    }
                }
            }
            for original in packet["meaning"]["evidence"].as_array().unwrap() {
                let rows: Vec<_> = report["evidence"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .filter(|e| {
                        e["graphId"] == parent["graphId"]
                            && e["packetIndex"] == packet["index"]
                            && e["record"]["id"] == original["id"]
                    })
                    .collect();
                assert_eq!(rows.len(), 1);
                assert_eq!(rows[0]["record"], *original);
            }
        }
    }
    assert_eq!(report["modelCalls"], 0);
    assert_eq!(report["complete"], false);
    assert_eq!(report["semanticIdentityEstablished"], false);
    assert_eq!(state(&i), before);
    assert_eq!(
        fs::read_to_string(&i.pair.marker).unwrap().lines().count(),
        6
    );
}

#[test]
fn empty_join_groups_keep_equal_labels_and_cross_graph_references_distinct() {
    let mut i = input();
    i.review["groups"] = json!([]);
    fs::write(&i.file, serde_json::to_vec(&i.review).unwrap()).unwrap();
    let report = success(run(&i, &[], None));
    assert_eq!(report["nodes"].as_array().unwrap().len(), 10);
    assert!(
        report["nodes"]
            .as_array()
            .unwrap()
            .iter()
            .all(|n| n["members"].as_array().unwrap().len() == 1)
    );
    let ids: std::collections::BTreeSet<_> = report["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|n| n["id"].as_str().unwrap())
        .collect();
    assert_eq!(ids.len(), 10);
    assert!(
        report["nodes"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|n| n["members"][0]["record"]["label"] == "Alpha concept")
            .count()
            > 2
    );
}

#[test]
fn join_order_and_host_member_order_do_not_change_projection_bytes_or_identity() {
    let mut i = input();
    let first = run(&i, &[], None);
    success(first.clone());
    i.review["graphIds"].as_array_mut().unwrap().reverse();
    i.review["groups"][0]["members"]
        .as_array_mut()
        .unwrap()
        .reverse();
    fs::write(&i.file, serde_json::to_vec(&i.review).unwrap()).unwrap();
    let second = run(&i, &[], None);
    success(second.clone());
    assert_eq!(first.stdout, second.stdout);
}

#[test]
fn checked_lookup_degrees_and_directed_trace_keep_selfloops_and_raw_slot_counts() {
    let i = input();
    let reference = i.review["groups"][0]["representative"].clone();
    let report = success(run(&i, &[], None));
    let node = grouped_node(&report);
    let lookup = success(run(
        &i,
        &[],
        Some(json!({"operation":"node","reference":reference})),
    ));
    assert_eq!(lookup["nodes"], json!([node]));
    let degree = success(run(&i, &[], Some(json!({"operation":"degree"}))));
    let row = degree["rows"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["nodeId"] == node["id"])
        .unwrap();
    assert_eq!(row["inDegree"], 8);
    assert_eq!(row["outDegree"], 7);
    assert_eq!(row["hyperedgeSlots"], 5);
    for (depth, nodes, edges) in [(0, 1, 6), (1, 2, 9)] {
        let trace = success(run(
            &i,
            &[],
            Some(
                json!({"operation":"trace","roots":[reference],"direction":"outgoing","depth":depth,"maxVisited":16}),
            ),
        ));
        assert_eq!(trace["nodes"].as_array().unwrap().len(), nodes);
        assert_eq!(trace["relations"].as_array().unwrap().len(), edges);
        assert_eq!(trace["traversal"], "directed_relations_only");
        assert_eq!(trace["complete"], false);
    }
}

#[test]
fn join_capacities_and_exact_output_newline_refuse_the_whole_request_without_publication() {
    let i = input();
    let before = state(&i);
    let good = run(&i, &[], None);
    success(good.clone());
    for (flag, value) in [
        ("--max-graphs", "1"),
        ("--max-groups", "0"),
        ("--max-members", "2"),
        ("--max-concepts", "9"),
        ("--max-relations", "8"),
        ("--max-hyperedges", "1"),
        ("--max-participants", "7"),
        ("--max-evidence", "0"),
        ("--max-input-bytes", "0"),
    ] {
        refused(run(&i, &[flag, value], None), "meaning_join_");
        assert_eq!(state(&i), before);
    }
    let exact = command(&i.pair.engine.root, &i.file, good.stdout.len(), &[])
        .output()
        .unwrap();
    success(exact.clone());
    assert_eq!(exact.stdout, good.stdout);
    refused(
        command(&i.pair.engine.root, &i.file, good.stdout.len() - 1, &[])
            .output()
            .unwrap(),
        "meaning_join_",
    );
    let reference = i.review["groups"][0]["representative"].clone();
    refused(
        run(
            &i,
            &[],
            Some(
                json!({"operation":"trace","roots":[reference],"direction":"outgoing","depth":1,"maxVisited":1}),
            ),
        ),
        "meaning_join_",
    );
    assert_eq!(state(&i), before);
}

#[test]
fn join_refuses_forged_pins_unselected_members_and_tampered_parent_receipts() {
    let mut i = input();
    let good = i.review.clone();
    for key in ["workspaceId", "snapshotId", "baseGraphHash"] {
        i.review = good.clone();
        i.review[key] = json!(hash("forged-pin"));
        fs::write(&i.file, serde_json::to_vec(&i.review).unwrap()).unwrap();
        let before = state(&i);
        refused(run(&i, &[], None), "meaning_join_");
        assert_eq!(state(&i), before);
    }
    i.review = good.clone();
    i.review["groups"][0]["members"][0]["graphId"] = json!(hash("unselected-parent"));
    fs::write(&i.file, serde_json::to_vec(&i.review).unwrap()).unwrap();
    refused(run(&i, &[], None), "meaning_join_");
    fs::write(&i.file, serde_json::to_vec(&good).unwrap()).unwrap();
    let response = i.parents[0]["packets"][0]["provenance"]["responseArtifactId"]
        .as_str()
        .unwrap();
    let path = i.pair.engine.store.dir.join("blobs").join(response);
    let mut bytes = fs::read(&path).unwrap();
    bytes[0] ^= 1;
    fs::write(path, bytes).unwrap();
    let before = state(&i);
    refused(run(&i, &[], None), "meaning_join_");
    assert_eq!(state(&i), before);
}

#[test]
fn join_global_capture_and_execution_flags_refuse_with_valid_registered_parents() {
    let i = input();
    success(run(&i, &[], None));
    let before = state(&i);
    let id = hash("global-selected-snapshot");
    for flags in [
        vec!["--snapshot", id.as_str()],
        vec!["--capture-config", "missing.json"],
        vec!["--include-ignored-dir", "ignored"],
        vec!["--analyzer-epoch", "9"],
        vec!["--language-server-config", "missing.json"],
        vec!["--trust-servers"],
        vec!["--allow-writes"],
    ] {
        refused(
            command(&i.pair.engine.root, &i.file, 4_000_000, &flags)
                .output()
                .unwrap(),
            "Meaning graph",
        );
        assert_eq!(state(&i), before);
    }
}

#[test]
fn join_host_admission_precedes_missing_or_incomplete_store_initialization() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().join("workspace");
    fs::create_dir(&root).unwrap();
    let file = directory.path().join("review.json");
    let review = json!({"schemaVersion":1,"workspaceId":hash("workspace"),"snapshotId":hash("snapshot"),"baseGraphHash":hash("graph"),"graphIds":[hash("meaning")],"groups":[]});
    for bad in [
        json!({}),
        json!({"schemaVersion":2}),
        {
            let mut v = review.clone();
            v["extra"] = json!(true);
            v
        },
        {
            let mut v = review.clone();
            v["graphIds"] = json!([hash("meaning"), hash("meaning")]);
            v
        },
    ] {
        fs::write(&file, serde_json::to_vec(&bad).unwrap()).unwrap();
        refused(
            command(&root, &file, 4_000_000, &[]).output().unwrap(),
            "meaning_join_",
        );
        assert!(!root.join(".yoke").exists());
    }
    fs::write(&file, serde_json::to_vec(&review).unwrap()).unwrap();
    refused(
        command(&root, &file, 4_000_000, &[]).output().unwrap(),
        "existing initialized workspace store",
    );
    assert!(!root.join(".yoke").exists());
    fs::create_dir(root.join(".yoke")).unwrap();
    fs::write(root.join(".yoke/human-note"), "keep").unwrap();
    refused(
        command(&root, &file, 4_000_000, &[]).output().unwrap(),
        "existing initialized workspace store",
    );
    assert_eq!(fs::read_dir(root.join(".yoke")).unwrap().count(), 1);
    assert_eq!(
        fs::read_to_string(root.join(".yoke/human-note")).unwrap(),
        "keep"
    );
}

#[test]
fn malformed_join_queries_refuse_before_creating_workspace_state() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().join("workspace");
    fs::create_dir(&root).unwrap();
    let file = directory.path().join("review.json");
    let graph = hash("meaning");
    let reference = json!({"graphId":graph,"packetIndex":0,"conceptId":hash("concept")});
    fs::write(
        &file,
        serde_json::to_vec(&json!({"schemaVersion":1,"workspaceId":hash("workspace"),
        "snapshotId":hash("snapshot"),"baseGraphHash":hash("base"),"graphIds":[graph],"groups":[]}))
        .unwrap(),
    )
    .unwrap();
    for query in [
        json!({"operation":"degree","extra":true}),
        json!({"operation":"unrecognized"}),
        json!({"operation":"node","reference":{"graphId":graph,"packetIndex":64,"conceptId":hash("concept")}}),
        json!({"operation":"node","reference":{"graphId":hash("unselected"),"packetIndex":0,"conceptId":hash("concept")}}),
        json!({"operation":"trace","roots":[{"graphId":hash("unselected"),"packetIndex":0,"conceptId":hash("concept")}],"direction":"outgoing","depth":1,"maxVisited":10}),
        json!({"operation":"trace","roots":[],"direction":"outgoing","depth":1,"maxVisited":10}),
        json!({"operation":"trace","roots":[reference.clone(),reference.clone()],"direction":"outgoing","depth":1,"maxVisited":10}),
        json!({"operation":"trace","roots":[reference.clone()],"direction":"invalid","depth":1,"maxVisited":10}),
        json!({"operation":"trace","roots":[reference.clone()],"direction":"outgoing","depth":9,"maxVisited":10}),
        json!({"operation":"trace","roots":[reference.clone()],"direction":"outgoing","depth":1,"maxVisited":4097}),
    ] {
        refused(
            command(&root, &file, 4_000_000, &[])
                .arg("--query")
                .arg(serde_json::to_string(&query).unwrap())
                .output()
                .unwrap(),
            "meaning_join_input_invalid",
        );
        assert!(!root.join(".yoke").exists());
    }
    refused(
        command(&root, &file, 4_000_000, &[])
            .args(["--query", r#"{"operation":"degree","operation":"degree"}"#])
            .output()
            .unwrap(),
        "meaning_join_input_invalid",
    );
    assert!(!root.join(".yoke").exists());
}

#[test]
fn exact_joint_input_capacity_and_representative_changes_preserve_logical_membership() {
    let mut i = input();
    let before = state(&i);
    let first = success(run(&i, &[], None));
    let bytes = first["statistics"]["admittedInputBytes"].as_u64().unwrap();
    let exact = success(run(&i, &["--max-input-bytes", &bytes.to_string()], None));
    assert_eq!(first, exact);
    refused(
        run(&i, &["--max-input-bytes", &(bytes - 1).to_string()], None),
        "meaning_join_capacity_exceeded",
    );
    i.review["groups"][0]["representative"] = i.review["groups"][0]["members"][1].clone();
    fs::write(&i.file, serde_json::to_vec(&i.review).unwrap()).unwrap();
    let second = success(run(&i, &[], None));
    assert_ne!(first["policyHash"], second["policyHash"]);
    assert_ne!(first["projectionId"], second["projectionId"]);
    assert_eq!(grouped_node(&first)["id"], grouped_node(&second)["id"]);
    assert_eq!(first["relations"], second["relations"]);
    assert_eq!(first["hyperedges"], second["hyperedges"]);
    i.review["groups"][0]["rationale"] = json!("Another explicit host presentation decision");
    fs::write(&i.file, serde_json::to_vec(&i.review).unwrap()).unwrap();
    let third = success(run(&i, &[], None));
    assert_ne!(second["policyHash"], third["policyHash"]);
    assert_ne!(second["projectionId"], third["projectionId"]);
    assert_eq!(grouped_node(&second)["id"], grouped_node(&third)["id"]);
    assert_eq!(second["relations"], third["relations"]);
    assert_eq!(second["hyperedges"], third["hyperedges"]);
    assert_eq!(state(&i), before);
}

#[test]
fn library_replay_restores_sqlite_limits_and_respects_an_existing_read_transaction() {
    use rusqlite::limits::Limit;
    use yoke::meaning_join::{self, Options, Review};
    let i = input();
    let review: Review = serde_json::from_value(i.review.clone()).unwrap();
    let before = state(&i);
    let original = i
        .pair
        .engine
        .store
        .db
        .limit(Limit::SQLITE_LIMIT_LENGTH)
        .unwrap();
    let transaction = i.pair.engine.store.db.unchecked_transaction().unwrap();
    let graph =
        meaning_join::join(&i.pair.engine.store, &review, &Options::default(), &d()).unwrap();
    assert_eq!(graph.as_value()["nodes"].as_array().unwrap().len(), 8);
    assert!(!i.pair.engine.store.db.is_autocommit());
    assert_eq!(
        i.pair
            .engine
            .store
            .db
            .limit(Limit::SQLITE_LIMIT_LENGTH)
            .unwrap(),
        original
    );
    let mut forged = review.clone();
    forged.base_graph_hash = hash("forged");
    assert!(meaning_join::join(&i.pair.engine.store, &forged, &Options::default(), &d()).is_err());
    assert!(!i.pair.engine.store.db.is_autocommit());
    assert_eq!(
        i.pair
            .engine
            .store
            .db
            .limit(Limit::SQLITE_LIMIT_LENGTH)
            .unwrap(),
        original
    );
    transaction.rollback().unwrap();
    assert!(i.pair.engine.store.db.is_autocommit());
    let options = Options {
        max_input_bytes: 1,
        ..Options::default()
    };
    assert!(
        meaning_join::join(&i.pair.engine.store, &review, &options, &d())
            .err()
            .unwrap()
            .to_string()
            .contains("capacity_exceeded")
    );
    assert!(i.pair.engine.store.db.is_autocommit());
    assert_eq!(
        i.pair
            .engine
            .store
            .db
            .limit(Limit::SQLITE_LIMIT_LENGTH)
            .unwrap(),
        original
    );
    assert_eq!(state(&i), before);
}

#[test]
fn join_and_checked_queries_honor_expired_and_shared_cancelled_deadlines_without_writes() {
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };
    use yoke::{
        capture::Deadline,
        meaning_join::{self, Options, Query, Review},
    };
    let i = input();
    let review: Review = serde_json::from_value(i.review.clone()).unwrap();
    let before = state(&i);
    let graph =
        meaning_join::join(&i.pair.engine.store, &review, &Options::default(), &d()).unwrap();
    let expired = Deadline::new(0);
    assert_eq!(
        meaning_join::join(&i.pair.engine.store, &review, &Options::default(), &expired)
            .err()
            .unwrap()
            .to_string(),
        "meaning_join_deadline_exceeded"
    );
    assert_eq!(
        graph
            .query(&Query::Degree {}, 4_000_000, &expired)
            .unwrap_err()
            .to_string(),
        "meaning_join_deadline_exceeded"
    );
    let cancel = Arc::new(AtomicBool::new(false));
    let result = Deadline::run_bounded(&d(), Arc::clone(&cancel), || {
        let inherited = Deadline::new(60_000);
        cancel.store(true, Ordering::SeqCst);
        assert_eq!(
            meaning_join::join(
                &i.pair.engine.store,
                &review,
                &Options::default(),
                &inherited
            )
            .err()
            .unwrap()
            .to_string(),
            "meaning_join_deadline_exceeded"
        );
        assert_eq!(
            graph
                .query(&Query::Degree {}, 4_000_000, &inherited)
                .unwrap_err()
                .to_string(),
            "meaning_join_deadline_exceeded"
        );
        Ok(())
    });
    assert!(result.is_err());
    assert_eq!(state(&i), before);
}

#[test]
fn compact_utf8_hash_oracle_binds_membership_policy_and_projection_independently() {
    fn string(v: &Value) -> String {
        serde_json::to_string(v.as_str().unwrap()).unwrap()
    }
    fn ref_key(v: &Value) -> (String, u64, String) {
        (
            v["graphId"].as_str().unwrap().into(),
            v["packetIndex"].as_u64().unwrap(),
            v["conceptId"].as_str().unwrap().into(),
        )
    }
    fn ref_bytes(v: &Value) -> String {
        format!(
            "{{\"graphId\":{},\"packetIndex\":{},\"conceptId\":{}}}",
            string(&v["graphId"]),
            v["packetIndex"],
            string(&v["conceptId"])
        )
    }
    let mut i = input();
    i.review["groups"][0]["rationale"] = json!("Host says \"caf\u{e9}\"\nwith exact CRLF\r\n");
    fs::write(&i.file, serde_json::to_vec(&i.review).unwrap()).unwrap();
    let report = success(run(&i, &[], None));
    let mut ids: Vec<_> = i.review["graphIds"]
        .as_array()
        .unwrap()
        .iter()
        .map(string)
        .collect();
    ids.sort();
    let group = &i.review["groups"][0];
    let mut members = group["members"].as_array().unwrap().clone();
    members.sort_by_key(ref_key);
    let group_bytes = format!(
        "{{\"members\":[{}],\"representative\":{},\"rationale\":{}}}",
        members.iter().map(ref_bytes).collect::<Vec<_>>().join(","),
        ref_bytes(&group["representative"]),
        string(&group["rationale"])
    );
    let review_bytes = format!(
        "{{\"schemaVersion\":1,\"workspaceId\":{},\"snapshotId\":{},\"baseGraphHash\":{},\"graphIds\":[{}],\"groups\":[{}]}}",
        string(&i.review["workspaceId"]),
        string(&i.review["snapshotId"]),
        string(&i.review["baseGraphHash"]),
        ids.join(","),
        group_bytes
    );
    let policy = hash(format!("[\"meaning-join-policy-v1\",{review_bytes}]"));
    assert_eq!(report["policyHash"], policy);
    assert_eq!(
        report["projectionId"],
        hash(format!("[\"meaning-join-projection-v1\",\"{policy}\"]"))
    );
    for node in report["nodes"].as_array().unwrap() {
        let mut members: Vec<_> = node["members"]
            .as_array()
            .unwrap()
            .iter()
            .map(|m| m["reference"].clone())
            .collect();
        members.sort_by_key(ref_key);
        assert_eq!(
            node["id"],
            hash(format!(
                "[\"meaning-join-node-v1\",[{}]]",
                members.iter().map(ref_bytes).collect::<Vec<_>>().join(",")
            ))
        );
    }
}
