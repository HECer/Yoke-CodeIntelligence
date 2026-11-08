use super::*;

#[test]
fn input_source_pair_event_and_output_limits_refuse_before_any_publication() {
    let f = fixture();
    let e = draft(&f, Kind::Concept, 1, 1000, Outcome::Success);
    let before = state(&f);
    for mode in 0..5 {
        let mut o = Options::default();
        match mode {
            0 => o.max_events = 0,
            1 => o.max_input_bytes = 0,
            2 => o.max_source_bytes = 0,
            3 => o.max_pairs = 0,
            _ => o.max_output_bytes = 0,
        }
        assert_eq!(
            meaning::record(&f.engine.store, &e, &o, &d())
                .unwrap_err()
                .to_string(),
            "feedback_capacity_exceeded",
            "{mode}"
        );
        assert_eq!(state(&f), before);
    }
    record(&f, &e).unwrap();
    let before = state(&f);
    for mode in 0..6 {
        let mut o = Options::default();
        match mode {
            0 => o.max_events = 0,
            1 => o.max_input_bytes = 0,
            2 => o.max_source_bytes = 0,
            3 => o.max_pairs = 0,
            4 => o.max_candidates = 0,
            _ => o.max_output_bytes = 0,
        }
        assert_eq!(
            meaning::rank(&f.engine.store, &query(&f, 1000), &o, &d())
                .unwrap_err()
                .to_string(),
            "feedback_capacity_exceeded",
            "{mode}"
        );
        assert_eq!(state(&f), before);
    }
    assert_eq!(calls(&f), 3);
}

#[test]
fn sql_publication_failure_read_transaction_and_expired_deadline_preserve_state_and_limits() {
    use rusqlite::limits::Limit;
    let f = fixture();
    let e = draft(&f, Kind::Concept, 1, 1000, Outcome::Success);
    let limit = f.engine.store.db.limit(Limit::SQLITE_LIMIT_LENGTH).unwrap();
    let before = state(&f);
    assert!(
        meaning::record(
            &f.engine.store,
            &e,
            &Options::default(),
            &yoke::capture::Deadline::new(0)
        )
        .is_err()
    );
    assert_eq!(state(&f), before);
    f.engine.store.db.execute_batch("BEGIN").unwrap();
    assert_eq!(
        record(&f, &e).unwrap_err().to_string(),
        "feedback_transaction_unavailable"
    );
    rank(&f, &query(&f, 1000)).unwrap();
    assert!(!f.engine.store.db.is_autocommit());
    f.engine.store.db.execute_batch("ROLLBACK").unwrap();
    assert_eq!(state(&f), before);
    f.engine.store.db.execute_batch("CREATE TEMP TRIGGER refuse_meaning_feedback BEFORE INSERT ON records WHEN NEW.kind='native_meaning_feedback_event_v1' BEGIN SELECT RAISE(ABORT,'owned fixture refusal'); END").unwrap();
    assert!(record(&f, &e).is_err());
    assert_eq!(state(&f), before);
    f.engine
        .store
        .db
        .execute_batch("DROP TRIGGER refuse_meaning_feedback")
        .unwrap();
    assert_eq!(
        f.engine.store.db.limit(Limit::SQLITE_LIMIT_LENGTH).unwrap(),
        limit
    );
    assert_eq!(record(&f, &e).unwrap()["inserted"], true);
}

#[test]
fn closed_shapes_subject_uniqueness_limit_counts_and_ordering_do_not_create_an_empty_complete_result()
 {
    let f = fixture();
    let value = serde_json::to_value(draft(&f, Kind::Concept, 1, 1000, Outcome::Success)).unwrap();
    for location in ["root", "target", "reference", "quote"] {
        let mut bad = value.clone();
        match location {
            "root" => bad["executionObserved"] = json!(true),
            "target" => bad["target"]["knowledgeId"] = json!("a".repeat(64)),
            "reference" => bad["target"]["reference"]["label"] = json!("Storage policy"),
            _ => bad["sourceEvidence"][0]["source"]["snapshotId"] = json!(f.original.id),
        }
        assert!(yoke::json::decode::<Draft>(bad).is_err());
    }
    let good = serde_json::to_string(&value).unwrap();
    let duplicate = format!("{{\"schemaVersion\":1,{}", good.strip_prefix('{').unwrap());
    assert!(yoke::json::parse(duplicate.as_bytes(), 64_000).is_err());
    let mut q = query(&f, 1000);
    q.targets.push(q.targets[0].clone());
    q.targets[1].evaluated.view_id = "a".repeat(64);
    assert!(meaning::validate_rank(&q, &d()).is_err());
    q.targets = [Kind::Concept, Kind::Relation, Kind::Hyperedge]
        .into_iter()
        .map(|kind| target(&f, kind))
        .collect();
    q.limit = 1;
    let before = state(&f);
    let v = rank(&f, &q).unwrap();
    assert_eq!(v["selectedTargets"], 3);
    assert_eq!(v["notReturnedByLimit"], 2);
    assert_eq!(v["items"].as_array().unwrap().len(), 1);
    assert_eq!(v["items"][0]["score"], 0);
    false_authorities(&v);
    assert_eq!(state(&f), before);
}

#[test]
fn missing_meaning_selection_missing_source_and_corrupt_event_refuse_retention_and_ranking() {
    let f = fixture();
    let mut e = draft(&f, Kind::Concept, 1, 1000, Outcome::Success);
    let plain = graph_view::build(
        &f.engine.store,
        &f.snapshot,
        &f.graph,
        &Default::default(),
        &d(),
    )
    .unwrap();
    e.target.origin = pin(&plain);
    let before = state(&f);
    assert!(record(&f, &e).is_err());
    assert_eq!(state(&f), before);
    let original = record(&f, &draft(&f, Kind::Concept, 1, 1000, Outcome::Success)).unwrap();
    yoke::retention::plan(&f.engine.store, &Default::default(), &d()).unwrap();
    let path = f.engine.store.dir.join("blobs").join(hash(source::A));
    let bytes = fs::read(&path).unwrap();
    fs::remove_file(&path).unwrap();
    assert!(rank(&f, &query(&f, 1000)).is_err());
    assert!(yoke::retention::plan(&f.engine.store, &Default::default(), &d()).is_err());
    fs::write(&path, bytes).unwrap();
    let mut bad = original["event"].clone();
    bad["extra"] = json!("feedback_capacity_exceeded");
    f.engine
        .store
        .record(meaning::EVENT_KIND, original["id"].as_str().unwrap(), &bad)
        .unwrap();
    let before = state(&f);
    assert!(rank(&f, &query(&f, 1000)).is_err());
    assert!(yoke::retention::plan(&f.engine.store, &Default::default(), &d()).is_err());
    assert_eq!(state(&f), before);
}

#[test]
fn actual_cli_record_rank_and_lost_stdout_retries_use_explicit_writes_and_no_provider() {
    let f = fixture();
    let e = draft(&f, Kind::Hyperedge, 1, 1000, Outcome::Success);
    let input = f.dir.path().join("event.json");
    let query_path = f.dir.path().join("rank.json");
    fs::write(&input, serde_json::to_vec(&e).unwrap()).unwrap();
    let mut q = query(&f, 1000);
    q.targets = vec![e.target.clone()];
    fs::write(&query_path, serde_json::to_vec(&q).unwrap()).unwrap();
    let command = || {
        let mut c = Command::new(env!("CARGO_BIN_EXE_yoke"));
        c.arg("--root")
            .arg(&f.engine.root)
            .args(["--max-bytes", "4000000"]);
        c
    };
    let denied = command()
        .args(["feedback", "meaning-record", "--input"])
        .arg(&input)
        .output()
        .unwrap();
    assert!(!denied.status.success());
    assert!(denied.stdout.is_empty());
    assert_eq!(count(&f), 0);
    let original_latest = f.engine.store.latest().unwrap();
    let status = command()
        .args(["--allow-writes", "feedback", "meaning-record", "--input"])
        .arg(&input)
        .stdout(std::process::Stdio::null())
        .status()
        .unwrap();
    assert!(status.success());
    let retry = command()
        .args(["--allow-writes", "feedback", "meaning-record", "--input"])
        .arg(&input)
        .output()
        .unwrap();
    assert!(
        retry.status.success(),
        "{}",
        String::from_utf8_lossy(&retry.stderr)
    );
    let result: Value = serde_json::from_slice(&retry.stdout).unwrap();
    assert_eq!(result["inserted"], false);
    false_authorities(&result);
    assert_eq!(count(&f), 1);
    let before = state(&f);
    let out = command()
        .args(["feedback", "meaning-rank", "--input"])
        .arg(&query_path)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let cli: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(cli, rank(&f, &q).unwrap());
    assert_eq!(state(&f), before);
    assert_eq!(f.engine.store.latest().unwrap(), original_latest);
    assert_eq!(calls(&f), 3);
}

#[test]
fn cli_permission_and_closed_input_refusals_precede_missing_workspace_initialization() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("missing-workspace");
    let absent = temp.path().join("missing.json");
    let command = || {
        let mut c = Command::new(env!("CARGO_BIN_EXE_yoke"));
        c.arg("--root").arg(&root);
        c
    };
    let denied = command()
        .args(["feedback", "meaning-record", "--input"])
        .arg(&absent)
        .output()
        .unwrap();
    assert!(!denied.status.success());
    assert!(denied.stdout.is_empty());
    assert!(String::from_utf8_lossy(&denied.stderr).contains("feedback_write_permission_required"));
    assert!(!root.exists());
    let input = temp.path().join("bad.json");
    fs::write(&input, b"{\"schemaVersion\":1,\"schemaVersion\":1}").unwrap();
    let bad = command()
        .args(["--allow-writes", "feedback", "meaning-record", "--input"])
        .arg(&input)
        .output()
        .unwrap();
    assert!(!bad.status.success());
    assert!(bad.stdout.is_empty());
    assert!(!root.exists());
    let wrong_flag = command()
        .args(["--allow-writes", "feedback", "meaning-rank", "--input"])
        .arg(&absent)
        .output()
        .unwrap();
    assert!(!wrong_flag.status.success());
    assert!(!root.exists());
}
