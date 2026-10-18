//! Deterministic historical comparison, source-evidence and continuity contracts.
//! Logical SQL/CAS and call-log oracles cover read-only comparison behavior.
//! Owned local Node workers belong to explicit Meaning fixture setup only.
#[path = "support/comparison_error_redaction.rs"]
mod error_redaction;
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
};
use tempfile::TempDir;
use yoke::{
    Engine,
    analysis::AnalyzerEpoch,
    capture::Deadline,
    graph_view::{self, BuildOptions, View},
    view_comparison::{self, ComparisonOptions, DeclaredFileContinuity, SelectionPin},
};

fn deadline() -> Deadline {
    Deadline::new(30_000)
}
const BEFORE: &str = "// __SOURCE_BEFORE_EVIDENCE_ONLY_81c49a__\nfn helper(){helper();}\nfn caller(){helper();helper();}\n";
const AFTER: &str = "// __SOURCE_AFTER_EVIDENCE_ONLY_f69d03__\nfn helper(){helper();caller();}\nfn caller(){helper();}\n";
const GUIDE: &str = "# Guide\n<!-- __SOURCE_DOC_EVIDENCE_ONLY_c5a918__ -->\n`main.rs::helper`\n";
const SOURCE_SENTINELS: &[&str] = &[
    "__SOURCE_BEFORE_EVIDENCE_ONLY_81c49a__",
    "__SOURCE_AFTER_EVIDENCE_ONLY_f69d03__",
    "__SOURCE_DOC_EVIDENCE_ONLY_c5a918__",
];

struct PairFixture {
    _root: TempDir,
    engine: Engine,
    before: View,
    after: View,
}
fn pair() -> PairFixture {
    let root = TempDir::new().unwrap();
    fs::write(root.path().join("main.rs"), BEFORE).unwrap();
    fs::write(root.path().join("guide.md"), GUIDE).unwrap();
    let mut engine = Engine::open(root.path()).unwrap();
    engine.capture_epoch = AnalyzerEpoch::V9;
    let (snapshot, graph, _, _) = engine.index(&deadline()).unwrap();
    let before = graph_view::build(
        &engine.store,
        &snapshot,
        &graph,
        &BuildOptions::default(),
        &deadline(),
    )
    .unwrap();
    fs::write(root.path().join("main.rs"), AFTER).unwrap();
    // Same semantic capture options; only the observed omission coverage changes.
    fs::write(root.path().join("opaque.bin"), [0, 255, 0]).unwrap();
    let (snapshot, graph, _, _) = engine.index(&deadline()).unwrap();
    let after = graph_view::build(
        &engine.store,
        &snapshot,
        &graph,
        &BuildOptions::default(),
        &deadline(),
    )
    .unwrap();
    assert_ne!(before.snapshot_id, after.snapshot_id);
    PairFixture {
        _root: root,
        engine,
        before,
        after,
    }
}

fn database_path(engine: &Engine) -> String {
    engine
        .store
        .db
        .query_row(
            "SELECT file FROM pragma_database_list WHERE name='main'",
            [],
            |r| r.get(0),
        )
        .unwrap()
}
fn cell(value: rusqlite::types::ValueRef<'_>) -> Value {
    use rusqlite::types::ValueRef;
    match value {
        ValueRef::Null => json!(["null"]),
        ValueRef::Integer(n) => json!(["integer", n]),
        ValueRef::Real(n) => json!(["realBits", n.to_bits().to_string()]),
        ValueRef::Text(bytes) => json!(["textBytes", bytes]),
        ValueRef::Blob(bytes) => json!(["blobBytes", bytes]),
    }
}
// Fresh reader detects committed same-count replacements through another connection.
// Logical schema/rows only; no claim about SQLite/WAL file-byte immutability.
fn sql_state(engine: &Engine) -> Value {
    let connection = rusqlite::Connection::open_with_flags(
        database_path(engine),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .unwrap();
    let transaction = connection.unchecked_transaction().unwrap();
    let schema = {
        let mut statement = transaction
            .prepare("SELECT type,name,tbl_name,sql FROM sqlite_master ORDER BY type,name,tbl_name")
            .unwrap();
        statement
            .query_map([], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, Option<String>>(3)?,
                ))
            })
            .unwrap()
            .collect::<rusqlite::Result<Vec<_>>>()
            .unwrap()
    };
    let mut tables = BTreeMap::new();
    for (kind, name, _, definition) in &schema {
        if kind != "table" {
            continue;
        }
        let quoted = name.replace('"', r#""""#);
        let mut statement = transaction
            .prepare(&format!(r#"SELECT * FROM "{quoted}""#))
            .unwrap();
        let count = statement.column_count();
        let rows = statement
            .query_map([], |row| {
                (0..count)
                    .map(|column| row.get_ref(column).map(cell))
                    .collect::<rusqlite::Result<Vec<_>>>()
            })
            .unwrap();
        let mut rows = rows.collect::<rusqlite::Result<Vec<_>>>().unwrap();
        rows.sort_by_key(|row| serde_json::to_vec(row).unwrap());
        tables.insert(name.clone(), json!({"definition":definition,"rows":rows}));
    }
    json!({"schema":schema,"tables":tables})
}
// Fixture-only content inventory also catches unregistered physical CAS writes.
fn state(engine: &Engine) -> Value {
    let mut blobs = BTreeMap::new();
    for entry in fs::read_dir(engine.store.dir.join("blobs")).unwrap() {
        let entry = entry.unwrap();
        blobs.insert(
            entry.file_name().to_string_lossy().into_owned(),
            fs::read(entry.path()).unwrap(),
        );
    }
    json!({"sql":sql_state(engine),"blobs":blobs})
}
fn calibrate_foreign_connection_mutation_oracle(engine: &Engine) {
    let before = sql_state(engine);
    let local_changes: i64 = engine
        .store
        .db
        .query_row("SELECT total_changes()", [], |r| r.get(0))
        .unwrap();
    let writer = rusqlite::Connection::open(database_path(engine)).unwrap();
    let (kind, id, raw): (String, String, String) = writer
        .query_row(
            "SELECT kind,id,value FROM records ORDER BY kind,id LIMIT 1",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .unwrap();
    let count: i64 = writer
        .query_row("SELECT count(*) FROM records", [], |r| r.get(0))
        .unwrap();
    assert_eq!(
        writer
            .execute(
                "UPDATE records SET value=?1 WHERE kind=?2 AND id=?3",
                rusqlite::params![format!("{raw} "), kind, id]
            )
            .unwrap(),
        1
    );
    assert_ne!(sql_state(engine), before);
    assert_eq!(
        writer
            .query_row("SELECT count(*) FROM records", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        count
    );
    assert_eq!(
        engine
            .store
            .db
            .query_row("SELECT total_changes()", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        local_changes
    );
    writer
        .execute(
            "UPDATE records SET value=?1 WHERE kind=?2 AND id=?3",
            rusqlite::params![raw, kind, id],
        )
        .unwrap();
    assert_eq!(sql_state(engine), before);
}

// Count contract is EVERY response array element, recursively, not group headers alone.
fn row_units(value: &Value) -> usize {
    match value {
        Value::Array(values) => values.len() + values.iter().map(row_units).sum::<usize>(),
        Value::Object(values) => values.values().map(row_units).sum(),
        _ => 0,
    }
}
// These are NEW comparison-only digests, never replacements for released
// View/Graph/capture/Meaning/receipt identities. Preserve all array ordering.
fn canonical_new_comparison_digest(original: &Value) -> String {
    let mut canonical = original.clone();
    canonical.sort_all_objects();
    yoke::model::identity(&canonical)
}

fn assert_closed_no_quote_tree(value: &Value, sentinels: &[&str]) {
    // Global UNION-key/sentinel guard, NOT complete position-aware DTO validation.
    // Specific occurrence/evidence fields bind real rows below. Future implementation
    // must still use typed per-position allowlists and cover every descriptor variant.
    const ALLOWED: &[&str] = &[
        "producer",
        "comparisonId",
        "options",
        "maxResultRows",
        "maxOutputBytes",
        "maxInputBytes",
        "maxSourceBytes",
        "before",
        "after",
        "selectionChanges",
        "declaredFileContinuity",
        "renameProofAvailable",
        "source",
        "meaning",
        "coverageChanges",
        "rowBudget",
        "complete",
        "limitations",
        "workspaceId",
        "snapshotId",
        "graphViewId",
        "viewArtifactId",
        "baseGraphHash",
        "sourceProjectionHash",
        "comparisonPolicyHash",
        "meaningCapturePolicyHash",
        "snapshotKind",
        "captureConsistency",
        "overlays",
        "meaningSelections",
        "coverage",
        "omissions",
        "path",
        "reason",
        "artifactId",
        "overlayId",
        "graphId",
        "jobId",
        "schemaVersion",
        "selectionIndex",
        "packetIndex",
        "model",
        "modelVersion",
        "currency",
        "beforePath",
        "beforeHash",
        "afterPath",
        "afterHash",
        "beforeSnapshotId",
        "afterSnapshotId",
        "provenance",
        "nodeChanges",
        "ambiguousNodes",
        "observationChanges",
        "ambiguousObservations",
        "totals",
        "changeKind",
        "identityBasis",
        "verifiedIdentity",
        "semanticKey",
        "beforeCount",
        "afterCount",
        "delta",
        "beforeRows",
        "afterRows",
        "occurrenceIdentity",
        "relation",
        "resolution",
        "beforeCalls",
        "afterCalls",
        "beforeSelfLoops",
        "afterSelfLoops",
        "side",
        "originalNodeId",
        "originalObservationId",
        "originalEvidenceId",
        "originalClaimId",
        "originalRowDigest",
        "digestMethod",
        "sourceId",
        "targetId",
        "targetLabel",
        "origin",
        "trust",
        "providerAttestation",
        "evidenceIds",
        "evidence",
        "sourceBindings",
        "representativeAnchor",
        "name",
        "qualifiedName",
        "kind",
        "signatureDigest",
        "textOmitted",
        "contentHash",
        "startByte",
        "endByte",
        "startLine",
        "endLine",
        "anchor",
        "beforePresent",
        "afterPresent",
        "sharedClaimIds",
        "addedClaimIds",
        "removedClaimIds",
        "receiptReuse",
        "claimChanges",
        "participants",
        "role",
        "concept",
        "originalSnapshotId",
        "originalJobId",
        "responseArtifactId",
        "attemptId",
        "requestId",
        "rebound",
        "bindingValidation",
        "providerReportedCostMicros",
        "providerReportedUsage",
        "beforeUnknownCostPackets",
        "afterUnknownCostPackets",
        "beforeUnknownUsagePackets",
        "afterUnknownUsagePackets",
        "used",
        "limit",
        "beforeOmissions",
        "afterOmissions",
        "beforeCoverage",
        "afterCoverage",
        "afterGraphViewId",
        "analyzerEpoch",
        "baseEvidenceId",
        "beforeGraphViewId",
        "bindingDerivation",
        "cachedInputTokens",
        "correspondenceId",
        "count",
        "exhaustive",
        "indexedFiles",
        "inputTokens",
        "languages",
        "materializedClaimId",
        "outcome",
        "outputTokens",
        "precision",
        "semantic",
        "serverProfileHash",
        "sourceCorrespondenceId",
        "sourceFiles",
        "stratum",
        "targetCorrespondenceId",
        "unresolved",
    ];
    if value.get("comparisonId").is_some() {
        // Complete final output minus only comparisonId, then recursively sorted.
        // This self-consistency check does not replace H10's positional schema.
        let mut identity_value = value.clone();
        let claimed = identity_value
            .as_object_mut()
            .unwrap()
            .remove("comparisonId")
            .unwrap();
        assert_eq!(claimed, canonical_new_comparison_digest(&identity_value));
    }
    match value {
        Value::Object(fields) => {
            for (key, child) in fields {
                assert!(
                    ALLOWED.contains(&key.as_str()),
                    "Unexpected descriptor key {key}"
                );
                assert_closed_no_quote_tree(child, sentinels);
            }
            if fields.contains_key("originalNodeId")
                || fields.contains_key("originalObservationId")
                || fields.contains_key("originalEvidenceId")
                || fields.contains_key("originalClaimId")
                || fields.contains_key("anchor")
            {
                assert_eq!(fields.get("textOmitted"), Some(&json!(true)));
            }
        }
        Value::Array(values) => values
            .iter()
            .for_each(|child| assert_closed_no_quote_tree(child, sentinels)),
        Value::String(text) => {
            for sentinel in sentinels {
                assert!(
                    !text.contains(sentinel),
                    "Evidence-only sentinel leaked under a nested output field: {sentinel}"
                );
            }
        }
        _ => {}
    }
}
fn node_id<'a>(view: &'a View, name: &str) -> &'a str {
    let matches = view
        .nodes
        .iter()
        .filter(|n| n["name"] == name && n["kind"] == "function")
        .collect::<Vec<_>>();
    assert_eq!(matches.len(), 1);
    matches[0]["id"].as_str().unwrap()
}
fn assert_occurrences(rows: &Value, view: &View, source: &str, target: &str) {
    let expected = view
        .observations
        .iter()
        .filter(|row| {
            row["relation"] == "calls" && row["source"] == source && row["target"] == target
        })
        .collect::<Vec<_>>();
    let actual = rows.as_array().unwrap();
    let expected_ids = expected
        .iter()
        .map(|r| r["id"].as_str().unwrap())
        .collect::<BTreeSet<_>>();
    let actual_ids = actual
        .iter()
        .map(|r| r["originalObservationId"].as_str().unwrap())
        .collect::<BTreeSet<_>>();
    assert_eq!(actual.len(), expected.len());
    assert_eq!(actual_ids, expected_ids);
    for row in actual {
        let original = expected
            .iter()
            .find(|r| r["id"] == row["originalObservationId"])
            .unwrap();
        assert_eq!(
            row["originalRowDigest"],
            canonical_new_comparison_digest(original)
        );
        assert_eq!(row["digestMethod"], "sha256_canonical_original_json_v1");
        assert_eq!(row["snapshotId"], original["snapshotId"]);
        assert_eq!(row["sourceId"], source);
        assert_eq!(row["targetId"], target);
        assert_eq!(row["evidenceIds"], original["evidenceIds"]);
        assert_eq!(
            row.get("sourceBindings").is_some(),
            original.get("sourceBindings").is_some(),
            "Do not synthesize original observation bindings from referenced evidence/nodes"
        );
        assert_eq!(row["textOmitted"], true);
        let evidence = row["evidence"].as_array().unwrap();
        assert_eq!(
            evidence.len(),
            original["evidenceIds"].as_array().unwrap().len()
        );
        for (redacted, evidence_id) in evidence
            .iter()
            .zip(original["evidenceIds"].as_array().unwrap())
        {
            let original_evidence = view
                .evidence
                .iter()
                .find(|e| e["id"] == *evidence_id)
                .unwrap();
            assert_eq!(redacted["originalEvidenceId"], *evidence_id);
            assert_eq!(
                redacted["originalRowDigest"],
                canonical_new_comparison_digest(original_evidence)
            );
            assert_eq!(
                redacted["digestMethod"],
                "sha256_canonical_original_json_v1"
            );
            assert_eq!(redacted["textOmitted"], true);
            for key in [
                "snapshotId",
                "path",
                "contentHash",
                "startByte",
                "endByte",
                "startLine",
                "endLine",
                "anchor",
            ] {
                assert_eq!(
                    redacted[key], original_evidence[key],
                    "Original evidence metadata changed: {key}"
                );
            }
        }
    }
}
#[cfg(windows)]
fn lock_all_cas(engine: &Engine) -> Vec<fs::File> {
    use std::os::windows::fs::OpenOptionsExt;
    let locks = fs::read_dir(engine.store.dir.join("blobs"))
        .unwrap()
        .map(|e| {
            fs::OpenOptions::new()
                .read(true)
                .share_mode(0)
                .open(e.unwrap().path())
                .unwrap()
        })
        .collect::<Vec<_>>();
    assert!(!locks.is_empty());
    locks
}
fn compare(
    f: &PairFixture,
    options: &ComparisonOptions,
) -> anyhow::Result<view_comparison::Comparison> {
    error_redaction::checked(view_comparison::compare_published(
        &f.engine.store,
        &SelectionPin::from_view(&f.before),
        &SelectionPin::from_view(&f.after),
        options,
        &deadline(),
    ))
}

#[test]
fn explicit_pair_replays_historical_bytes_and_is_deterministic_without_publication() {
    let mut f = pair();
    calibrate_foreign_connection_mutation_oracle(&f.engine);
    let before = state(&f.engine);
    fs::write(
        f._root.path().join("main.rs"),
        "fn unrelated_live_bytes() {}\n",
    )
    .unwrap();
    let first = compare(&f, &ComparisonOptions::default()).unwrap();
    let second = compare(&f, &ComparisonOptions::default()).unwrap();
    assert_eq!(first.canonical_bytes(), second.canonical_bytes());
    assert_eq!(first.value()["before"]["graphViewId"], f.before.id);
    assert_eq!(first.value()["after"]["graphViewId"], f.after.id);
    assert_eq!(first.value()["complete"], false);
    assert_eq!(
        first.value()["before"]["comparisonPolicyHash"],
        first.value()["after"]["comparisonPolicyHash"]
    );
    assert!(
        first.value()["before"]["omissions"]
            .as_array()
            .unwrap()
            .iter()
            .all(|o| o["path"] != "opaque.bin")
    );
    assert!(
        first.value()["after"]["omissions"]
            .as_array()
            .unwrap()
            .iter()
            .any(|o| o["path"] == "opaque.bin")
    );
    assert!(
        !first.value()["coverageChanges"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert_closed_no_quote_tree(first.value(), SOURCE_SENTINELS);
    assert_eq!(state(&f.engine), before);
    assert_eq!(
        fs::read_to_string(f._root.path().join("main.rs")).unwrap(),
        "fn unrelated_live_bytes() {}\n"
    );
    // A genuine semantic policy change is refused even if no build/ file exists.
    f.engine.capture_options.include_dirs.push("build".into());
    let (snapshot, graph, _, _) = f.engine.index(&deadline()).unwrap();
    let changed_policy = graph_view::build(
        &f.engine.store,
        &snapshot,
        &graph,
        &BuildOptions::default(),
        &deadline(),
    )
    .unwrap();
    let policy_baseline = state(&f.engine);
    let error = match error_redaction::checked(view_comparison::compare_published(
        &f.engine.store,
        &SelectionPin::from_view(&f.before),
        &SelectionPin::from_view(&changed_policy),
        &ComparisonOptions::default(),
        &deadline(),
    )) {
        Ok(value) => {
            drop(value);
            panic!("Changed capture options must refuse")
        }
        Err(error) => error,
    };
    assert!(format!("{error:#}").contains("comparison_policy_mismatch"));
    assert_eq!(state(&f.engine), policy_baseline);
}

#[test]
fn directed_parallel_delta_does_not_infer_which_identical_occurrence_survived() {
    let f = pair();
    let output = compare(&f, &ComparisonOptions::default()).unwrap();
    let groups = output.value()["source"]["observationChanges"]
        .as_array()
        .unwrap();
    let calls = groups
        .iter()
        .filter(|g| g["relation"] == "calls")
        .collect::<Vec<_>>();
    assert_eq!(calls.len(), 2);
    let removed = calls.iter().find(|g| g["delta"] == -1).unwrap();
    let added = calls.iter().find(|g| g["delta"] == 1).unwrap();
    assert_eq!(removed["beforeCount"], 2);
    assert_eq!(removed["afterCount"], 1);
    assert_eq!(removed["beforeRows"].as_array().unwrap().len(), 2);
    assert_eq!(removed["afterRows"].as_array().unwrap().len(), 1);
    assert_eq!(removed["occurrenceIdentity"], "not_inferred");
    assert_eq!(added["beforeCount"], 0);
    assert_eq!(added["afterCount"], 1);
    assert_occurrences(
        &removed["beforeRows"],
        &f.before,
        node_id(&f.before, "caller"),
        node_id(&f.before, "helper"),
    );
    assert_occurrences(
        &removed["afterRows"],
        &f.after,
        node_id(&f.after, "caller"),
        node_id(&f.after, "helper"),
    );
    assert_occurrences(
        &added["beforeRows"],
        &f.before,
        node_id(&f.before, "helper"),
        node_id(&f.before, "caller"),
    );
    assert_occurrences(
        &added["afterRows"],
        &f.after,
        node_id(&f.after, "helper"),
        node_id(&f.after, "caller"),
    );
    assert_closed_no_quote_tree(output.value(), SOURCE_SENTINELS);
    assert_eq!(output.value()["source"]["totals"]["beforeCalls"], 3);
    assert_eq!(output.value()["source"]["totals"]["afterCalls"], 3);
    assert_eq!(output.value()["source"]["totals"]["beforeSelfLoops"], 1);
    assert_eq!(output.value()["source"]["totals"]["afterSelfLoops"], 1);
    let reverse = error_redaction::checked(view_comparison::compare_published(
        &f.engine.store,
        &SelectionPin::from_view(&f.after),
        &SelectionPin::from_view(&f.before),
        &ComparisonOptions::default(),
        &deadline(),
    ))
    .unwrap();
    assert_ne!(
        output.value()["comparisonId"],
        reverse.value()["comparisonId"]
    );
}

#[test]
fn stale_snapshot_artifact_and_graph_pins_refuse_without_mutation() {
    let f = pair();
    compare(&f, &ComparisonOptions::default()).expect("Valid pair control before stale pins");
    let baseline = state(&f.engine);
    for mode in 0..3 {
        let mut stale = SelectionPin::from_view(&f.after);
        match mode {
            0 => stale.snapshot_id = f.before.snapshot_id.clone(),
            1 => stale.view_artifact_id = f.before.artifact_id.clone(),
            _ => stale.base_graph_hash = f.before.base_graph_hash.clone(),
        }
        #[cfg(windows)]
        let locks = lock_all_cas(&f.engine);
        let error = match error_redaction::checked(view_comparison::compare_published(
            &f.engine.store,
            &SelectionPin::from_view(&f.before),
            &stale,
            &ComparisonOptions::default(),
            &deadline(),
        )) {
            Ok(value) => {
                drop(value);
                panic!("Stale pin must refuse")
            }
            Err(error) => error,
        };
        #[cfg(windows)]
        drop(locks);
        let message = format!("{error:#}");
        assert!(
            message.contains("comparison_pin_mismatch"),
            "Pin admission must precede CAS access"
        );
        assert!(message.contains(["snapshotId", "viewArtifactId", "baseGraphHash"][mode]));
        // On Windows a premature CAS open would instead cause a real OS32 error.
        // On other platforms this is an error-class oracle only, not a read-order proof.
        assert_eq!(state(&f.engine), baseline);
    }
}

#[test]
fn wrong_store_and_cross_workspace_pins_do_not_fall_back_to_equal_paths() {
    let f = pair();
    let other = pair();
    compare(&f, &ComparisonOptions::default()).expect("Valid first Store control");
    compare(&other, &ComparisonOptions::default()).expect("Valid second Store control");
    let baseline = state(&other.engine);
    let original = state(&f.engine);
    assert!(
        error_redaction::checked(view_comparison::compare_published(
            &other.engine.store,
            &SelectionPin::from_view(&f.before),
            &SelectionPin::from_view(&f.after),
            &ComparisonOptions::default(),
            &deadline()
        ))
        .is_err()
    );
    assert!(
        error_redaction::checked(view_comparison::compare_published(
            &f.engine.store,
            &SelectionPin::from_view(&f.before),
            &SelectionPin::from_view(&other.after),
            &ComparisonOptions::default(),
            &deadline()
        ))
        .is_err()
    );
    assert_eq!(state(&other.engine), baseline);
    assert_eq!(state(&f.engine), original);
}

#[test]
fn all_output_or_deadline_failures_publish_nothing() {
    let f = pair();
    compare(&f, &ComparisonOptions::default()).expect("Valid pair control before tight caps");
    let baseline = state(&f.engine);
    let generous = compare(&f, &ComparisonOptions::default()).unwrap();
    let exact_units = row_units(generous.value());
    assert_eq!(generous.value()["rowBudget"]["used"], exact_units);
    let groups = generous.value()["source"]["observationChanges"]
        .as_array()
        .unwrap();
    let members = groups
        .iter()
        .map(|g| {
            g["beforeRows"].as_array().unwrap().len() + g["afterRows"].as_array().unwrap().len()
        })
        .sum::<usize>();
    assert!(members >= 4 && exact_units >= groups.len() + members);
    assert!(
        compare(
            &f,
            &ComparisonOptions {
                max_result_rows: exact_units,
                ..Default::default()
            }
        )
        .is_ok()
    );
    let error = match compare(
        &f,
        &ComparisonOptions {
            max_result_rows: exact_units - 1,
            ..Default::default()
        },
    ) {
        Ok(value) => {
            drop(value);
            panic!("One less than complete nested row units must refuse")
        }
        Err(error) => error,
    };
    assert!(format!("{error:#}").contains("comparison_capacity_exceeded"));
    for options in [
        ComparisonOptions {
            max_output_bytes: 1,
            ..Default::default()
        },
        ComparisonOptions {
            max_result_rows: 1,
            ..Default::default()
        },
        ComparisonOptions {
            max_input_bytes: 1,
            ..Default::default()
        },
        ComparisonOptions {
            max_source_bytes: 1,
            ..Default::default()
        },
    ] {
        assert!(compare(&f, &options).is_err());
        assert_eq!(state(&f.engine), baseline);
    }
    assert!(
        error_redaction::checked(view_comparison::compare_published(
            &f.engine.store,
            &SelectionPin::from_view(&f.before),
            &SelectionPin::from_view(&f.after),
            &ComparisonOptions::default(),
            &Deadline::new(0)
        ))
        .is_err()
    );
    assert_eq!(state(&f.engine), baseline);
}

#[test]
fn identical_bytes_across_paths_do_not_manufacture_rename_lineage() {
    let root = TempDir::new().unwrap();
    fs::write(root.path().join("old.rs"), BEFORE).unwrap();
    let mut engine = Engine::open(root.path()).unwrap();
    let (snapshot, graph, _, _) = engine.index(&deadline()).unwrap();
    let before = graph_view::build(
        &engine.store,
        &snapshot,
        &graph,
        &BuildOptions::default(),
        &deadline(),
    )
    .unwrap();
    fs::rename(root.path().join("old.rs"), root.path().join("new.rs")).unwrap();
    let (snapshot, graph, _, _) = engine.index(&deadline()).unwrap();
    let after = graph_view::build(
        &engine.store,
        &snapshot,
        &graph,
        &BuildOptions::default(),
        &deadline(),
    )
    .unwrap();
    let a = SelectionPin::from_view(&before);
    let b = SelectionPin::from_view(&after);
    let plain = error_redaction::checked(view_comparison::compare_published(
        &engine.store,
        &a,
        &b,
        &ComparisonOptions::default(),
        &deadline(),
    ))
    .unwrap();
    assert_eq!(plain.value()["declaredFileContinuity"], json!([]));
    assert_eq!(plain.value()["renameProofAvailable"], false);
    let old_ids = before
        .nodes
        .iter()
        .filter(|n| n["path"] == "old.rs")
        .map(|n| n["id"].as_str().unwrap())
        .collect::<BTreeSet<_>>();
    let new_ids = after
        .nodes
        .iter()
        .filter(|n| n["path"] == "new.rs")
        .map(|n| n["id"].as_str().unwrap())
        .collect::<BTreeSet<_>>();
    let changes = plain.value()["source"]["nodeChanges"].as_array().unwrap();
    let removed = changes
        .iter()
        .filter(|r| r["changeKind"] == "removed" && r["before"]["path"] == "old.rs")
        .map(|r| r["before"]["originalNodeId"].as_str().unwrap())
        .collect::<BTreeSet<_>>();
    let added = changes
        .iter()
        .filter(|r| r["changeKind"] == "added" && r["after"]["path"] == "new.rs")
        .map(|r| r["after"]["originalNodeId"].as_str().unwrap())
        .collect::<BTreeSet<_>>();
    assert!(!old_ids.is_empty() && !new_ids.is_empty());
    assert_eq!(removed, old_ids);
    assert_eq!(added, new_ids);
    assert!(
        !changes
            .iter()
            .any(|r| r["before"]["path"] == "old.rs" && r["after"]["path"] == "new.rs")
    );
    let association = DeclaredFileContinuity {
        before_path: "old.rs".into(),
        before_hash: yoke::model::hash(BEFORE),
        after_path: "new.rs".into(),
        after_hash: yoke::model::hash(BEFORE),
    };
    let options = ComparisonOptions {
        declared_file_continuity: vec![association],
        ..Default::default()
    };
    let explicit = error_redaction::checked(view_comparison::compare_published(
        &engine.store,
        &a,
        &b,
        &options,
        &deadline(),
    ))
    .unwrap();
    assert_eq!(
        explicit.value()["declaredFileContinuity"][0]["provenance"],
        "host_declared_file_continuity_not_proven_rename"
    );
    assert_eq!(explicit.value()["renameProofAvailable"], false);
    let old_file = before
        .nodes
        .iter()
        .find(|n| n["path"] == "old.rs" && n["kind"] == "file")
        .unwrap();
    let new_file = after
        .nodes
        .iter()
        .find(|n| n["path"] == "new.rs" && n["kind"] == "file")
        .unwrap();
    let file_pair = explicit.value()["source"]["nodeChanges"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| {
            r["before"]["originalNodeId"] == old_file["id"]
                && r["after"]["originalNodeId"] == new_file["id"]
        })
        .expect("Explicit PRIMARY file association must change only candidate pairing");
    assert_eq!(file_pair["verifiedIdentity"], false);
    assert_eq!(
        file_pair["identityBasis"],
        "host_declared_file_continuity_not_proven_rename"
    );
    assert_eq!(
        file_pair["before"]["originalRowDigest"],
        canonical_new_comparison_digest(old_file)
    );
    assert_eq!(
        file_pair["after"]["originalRowDigest"],
        canonical_new_comparison_digest(new_file)
    );
    let bad = ComparisonOptions {
        declared_file_continuity: vec![DeclaredFileContinuity {
            before_path: "old.rs".into(),
            before_hash: yoke::model::hash("not the source"),
            after_path: "new.rs".into(),
            after_hash: yoke::model::hash(BEFORE),
        }],
        ..Default::default()
    };
    assert!(
        error_redaction::checked(view_comparison::compare_published(
            &engine.store,
            &a,
            &b,
            &bad,
            &deadline()
        ))
        .is_err()
    );
}

// Copy this ignored owned fixture with the test only after root release.
#[path = "support/meaning_comparison_fixture.rs"]
mod owned_meaning_fixture;

#[test]
fn checked_archives_compare_after_live_store_and_worker_are_gone_without_quote_leaks() {
    use yoke::graph_view_archive::{self as archive, Limits};
    let f = owned_meaning_fixture::fixture(AnalyzerEpoch::V9);
    let ids = f
        .view
        .meaning
        .as_ref()
        .unwrap()
        .selections
        .iter()
        .map(|s| s.graph_id.clone())
        .collect();
    let minimal = graph_view::build(
        &f.engine.store,
        &f.snapshot,
        &f.graph,
        &BuildOptions {
            structural_kinds: vec![],
            meaning_graph_ids: ids,
            ..Default::default()
        },
        &deadline(),
    )
    .unwrap();
    let before = archive::parse(
        archive::prepare(&f.engine.store, &minimal, Limits::default(), &deadline())
            .unwrap()
            .bytes(),
        Limits::default(),
        &deadline(),
    )
    .unwrap();
    let after = archive::parse(
        archive::prepare(&f.engine.store, &f.view, Limits::default(), &deadline())
            .unwrap()
            .bytes(),
        Limits::default(),
        &deadline(),
    )
    .unwrap();
    let original_policy_hash = f
        .view
        .meaning
        .as_ref()
        .unwrap()
        .scope
        .capture_policy_hash
        .clone();
    let original_view_ids = (minimal.id.clone(), f.view.id.clone());
    let calls = fs::read(&f.marker).unwrap();
    let marker = f.marker.clone();
    let _retained = owned_meaning_fixture::remove_source_workspace(f);
    let comparison = error_redaction::checked(view_comparison::compare_meaning_archives(
        &before,
        &after,
        &ComparisonOptions::default(),
        &deadline(),
    ))
    .unwrap();
    assert_eq!(comparison.value()["meaning"]["beforePresent"], true);
    assert_eq!(comparison.value()["meaning"]["afterPresent"], true);
    assert!(
        !comparison.value()["selectionChanges"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        comparison.value()["meaning"]["providerAttestation"],
        "unverified"
    );
    assert_eq!(
        comparison.value()["meaning"]["resolution"],
        "inferred_concept"
    );
    assert_eq!(
        comparison.value()["meaning"]["provenance"]["beforeUnknownCostPackets"],
        3
    );
    assert_eq!(
        comparison.value()["meaning"]["provenance"]["afterUnknownUsagePackets"],
        3
    );
    assert_eq!(
        comparison.value()["before"]["meaningCapturePolicyHash"],
        original_policy_hash
    );
    assert_eq!(
        comparison.value()["after"]["meaningCapturePolicyHash"],
        original_policy_hash
    );
    assert_eq!(
        comparison.value()["before"]["graphViewId"],
        original_view_ids.0
    );
    assert_eq!(
        comparison.value()["after"]["graphViewId"],
        original_view_ids.1
    );
    assert!(
        comparison.value()["meaning"]["addedClaimIds"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert!(
        comparison.value()["meaning"]["removedClaimIds"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    let meaning_sentinels = [
        "__MEANING_A_EVIDENCE_ONLY_6d249d__",
        "__MEANING_B_EVIDENCE_ONLY_b87d91__",
        "__MEANING_DOC_EVIDENCE_ONLY_e210b9__",
        "__HISTORICAL_OLD_SOURCE_ONLY_394ace__",
        "__HISTORICAL_NEW_SOURCE_ONLY_6a52bc__",
    ];
    assert_closed_no_quote_tree(comparison.value(), &meaning_sentinels);
    let current_hash = yoke::model::hash(owned_meaning_fixture::HISTORY_NEW);
    let association = ComparisonOptions {
        declared_file_continuity: vec![DeclaredFileContinuity {
            before_path: "history.rs".into(),
            before_hash: current_hash.clone(),
            after_path: "history.rs".into(),
            after_hash: current_hash,
        }],
        ..Default::default()
    };
    let valid = error_redaction::checked(view_comparison::compare_meaning_archives(
        &before,
        &after,
        &association,
        &deadline(),
    ))
    .expect("Current PRIMARY file association control");
    assert_eq!(
        valid.value()["declaredFileContinuity"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    let old_hash = yoke::model::hash(owned_meaning_fixture::HISTORY_OLD);
    let invalid = ComparisonOptions {
        declared_file_continuity: vec![DeclaredFileContinuity {
            before_path: "history.rs".into(),
            before_hash: old_hash,
            after_path: "history.rs".into(),
            after_hash: yoke::model::hash(owned_meaning_fixture::HISTORY_NEW),
        }],
        ..Default::default()
    };
    let error = match error_redaction::checked(view_comparison::compare_meaning_archives(
        &before,
        &after,
        &invalid,
        &deadline(),
    )) {
        Ok(value) => {
            drop(value);
            panic!("An available old union member is not the PRIMARY source file")
        }
        Err(error) => error,
    };
    assert!(format!("{error:#}").contains("comparison_primary_file_binding_mismatch"));
    assert_eq!(fs::read(marker).unwrap(), calls);
}
