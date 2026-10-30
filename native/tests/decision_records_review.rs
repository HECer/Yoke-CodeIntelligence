use serde_json::{Value, json};
use std::{fs, process::Command};
use tempfile::TempDir;
use yoke::{
    Engine,
    capture::Deadline,
    decision_records::{DecisionStore, Scope},
    model::Snapshot,
    retention::{self, Policy},
    store::Store,
};

fn fixture() -> (TempDir, Engine, Snapshot) {
    let directory = TempDir::new().unwrap();
    fs::write(
        directory.path().join("config.rs"),
        "const MODE: &str = \"old\";\n",
    )
    .unwrap();
    let mut engine = Engine::open(directory.path()).unwrap();
    let snapshot = engine.index(&Deadline::new(10_000)).unwrap().0;
    (directory, engine, snapshot)
}

fn draft(body: &str) -> Value {
    json!({"key":"storage-policy","kind":"preference","title":"Retain source evidence","body":body,"author":"Ada","date":"2026-10-15","paths":["config.rs"],"rules":[{"kind":"changed_value","path":"config.rs","before":"\"old\"","after":"\"new\""}]})
}

fn personal<'a>(source: &'a Store, storage: &'a Store) -> DecisionStore<'a> {
    DecisionStore::new(
        source,
        storage,
        Scope::Personal {
            host_id: "review-windows-host".into(),
        },
        &Deadline::new(10_000),
    )
    .unwrap()
}

#[test]
fn personal_record_source_history_cannot_be_retired_from_the_source_store() {
    let (directory, mut engine, baseline) = fixture();
    let personal_directory = TempDir::new().unwrap();
    let storage = Store::open(personal_directory.path()).unwrap();
    let mut private_draft = draft("PERSONAL_ONLY_BODY_97d44e42");
    private_draft["title"] = json!("PERSONAL_ONLY_TITLE_0231ab8f");
    private_draft["author"] = json!("PERSONAL_ONLY_AUTHOR_e54e76cb");
    private_draft["cost"] =
        json!({"currency":"EUR","microunits":778899,"basis":"PERSONAL_ONLY_COST_BASIS_19a7f4a2"});
    let authored = personal(&engine.store, &storage)
        .invoke(
            &baseline,
            "write",
            &json!({"draft":private_draft}),
            &Deadline::new(10_000),
        )
        .unwrap();
    let source_rows = engine
        .store
        .db
        .prepare("SELECT value FROM records")
        .unwrap()
        .query_map([], |row| row.get::<_, String>(0))
        .unwrap()
        .collect::<rusqlite::Result<Vec<_>>>()
        .unwrap()
        .join("\n");
    for private_text in [
        "PERSONAL_ONLY_BODY_97d44e42",
        "PERSONAL_ONLY_TITLE_0231ab8f",
        "PERSONAL_ONLY_AUTHOR_e54e76cb",
        "PERSONAL_ONLY_COST_BASIS_19a7f4a2",
        "review-windows-host",
    ] {
        assert!(
            !source_rows.contains(private_text),
            "Personal text was copied into the source repository Store: {private_text}"
        );
    }
    fs::write(
        directory.path().join("config.rs"),
        "const MODE: &str = \"new\";\n",
    )
    .unwrap();
    let current = engine.index(&Deadline::new(10_000)).unwrap().0;
    assert_ne!(baseline.id, current.id);

    let plan = retention::plan(&engine.store, &Policy::default(), &Deadline::new(10_000)).unwrap();
    assert_eq!(
        plan["unknownDependencyKinds"],
        json!([]),
        "Source pin support must be typed rather than disabling all retirement"
    );
    assert!(
        !plan["snapshotIds"]
            .as_array()
            .unwrap()
            .contains(&json!(baseline.id)),
        "Source retention admitted a snapshot still referenced by a personal decision: {plan}"
    );
    let plan_id = plan["planId"].as_str().unwrap();
    retention::apply(
        &engine.store,
        plan_id,
        plan_id,
        true,
        &Deadline::new(10_000),
    )
    .unwrap();
    let record = personal(&engine.store, &storage)
        .get("storage-policy", &Deadline::new(10_000))
        .unwrap();
    assert_eq!(record.id, authored["revision"]["id"]);
    assert_eq!(record.draft.body, "PERSONAL_ONLY_BODY_97d44e42");
    assert_eq!(record.draft.author, "PERSONAL_ONLY_AUTHOR_e54e76cb");
    assert!(engine.store.snapshot(&baseline.id).is_ok());
}

#[test]
fn shared_personal_store_preserves_source_workspace_isolation_for_identical_keys() {
    let (_first_directory, first, first_snapshot) = fixture();
    let (_second_directory, second, second_snapshot) = fixture();
    let personal_directory = TempDir::new().unwrap();
    let storage = Store::open(personal_directory.path()).unwrap();
    let first_store = personal(&first.store, &storage);
    let second_store = personal(&second.store, &storage);
    let a = first_store
        .invoke(
            &first_snapshot,
            "write",
            &json!({"draft":draft("First source policy.")}),
            &Deadline::new(10_000),
        )
        .unwrap();
    let b = second_store
        .invoke(
            &second_snapshot,
            "write",
            &json!({"draft":draft("Second source policy.")}),
            &Deadline::new(10_000),
        )
        .unwrap();
    assert_ne!(a["revision"]["id"], b["revision"]["id"]);
    let a_records = first_store.list(&Deadline::new(10_000)).unwrap();
    let b_records = second_store.list(&Deadline::new(10_000)).unwrap();
    assert_eq!(a_records.len(), 1);
    assert_eq!(b_records.len(), 1);
    assert_eq!(a_records[0].draft.body, "First source policy.");
    assert_eq!(b_records[0].draft.body, "Second source policy.");
    assert!(
        first_store
            .invoke(
                &second_snapshot,
                "read",
                &json!({"key":"storage-policy"}),
                &Deadline::new(10_000)
            )
            .is_err()
    );
    assert!(yoke::decision_records::validated_roots(&storage, &Deadline::new(10_000)).is_err());
}

#[test]
fn changed_value_with_both_literals_in_candidate_does_not_claim_a_transition() {
    let (directory, mut engine, baseline) = fixture();
    {
        let records = DecisionStore::new(
            &engine.store,
            &engine.store,
            Scope::Repository,
            &Deadline::new(10_000),
        )
        .unwrap();
        records
            .invoke(
                &baseline,
                "write",
                &json!({"draft":draft("Replace the previous value.")}),
                &Deadline::new(10_000),
            )
            .unwrap();
    }
    fs::write(
        directory.path().join("config.rs"),
        "const MODE: &str = \"old\";\nconst OTHER: &str = \"new\";\n",
    )
    .unwrap();
    let candidate = engine.index(&Deadline::new(10_000)).unwrap().0;
    let records = DecisionStore::new(
        &engine.store,
        &engine.store,
        Scope::Repository,
        &Deadline::new(10_000),
    )
    .unwrap();
    let result = records
        .check("storage-policy", &candidate, &Deadline::new(10_000))
        .unwrap();
    assert_eq!(result["rules"][0]["outcome"], "inconclusive");
    assert_eq!(
        result["classification"],
        "literal_heuristic_not_semantic_proof"
    );
    assert_eq!(
        result["rules"][0]["beforeEvidence"][0]["snapshotId"],
        baseline.id
    );
    assert_eq!(
        result["rules"][0]["afterEvidence"][0]["snapshotId"],
        candidate.id
    );
}

#[test]
fn cli_budget_artifacts_keep_personal_record_content_in_the_personal_store() {
    let source_directory = TempDir::new().unwrap();
    fs::write(
        source_directory.path().join("config.rs"),
        "const MODE: &str = \"old\";\n",
    )
    .unwrap();
    let personal_directory = TempDir::new().unwrap();
    let input_directory = TempDir::new().unwrap();
    let input_path = input_directory.path().join("input.json");
    let body = "Private preference attribution stays in selected personal storage. ".repeat(200);
    fs::write(
        &input_path,
        serde_json::to_vec(&json!({"draft":draft(&body)})).unwrap(),
    )
    .unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_yoke"))
        .arg("--root")
        .arg(source_directory.path())
        .args([
            "decisions",
            "write",
            "--allow-record-writes",
            "--max-bytes",
            "1024",
            "--personal-root",
        ])
        .arg(personal_directory.path())
        .args(["--host-id", "review-windows-host", "--input"])
        .arg(&input_path)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stdout.len() <= 1024);
    let envelope: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(envelope["status"], "partial");
    let artifact = envelope["artifactId"].as_str().unwrap();
    let source = Engine::open(source_directory.path()).unwrap();
    assert!(
        source.store.read_blob(artifact).is_err(),
        "Personal result artifact was copied into the source repository Store"
    );
    let selected = Store::open(personal_directory.path()).unwrap();
    let complete: Value = serde_json::from_slice(&selected.read_blob(artifact).unwrap()).unwrap();
    assert_eq!(complete["revision"]["draft"]["body"], body);
    assert_eq!(
        envelope["readWith"]["root"],
        json!(personal_directory.path())
    );
}
