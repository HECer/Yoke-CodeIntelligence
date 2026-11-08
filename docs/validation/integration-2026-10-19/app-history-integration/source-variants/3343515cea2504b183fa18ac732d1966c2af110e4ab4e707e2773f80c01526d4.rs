//! Actual owned Git/worker completions, durable rollback and authority replay.
#![cfg(windows)]
include!("support/github_app_worker_history_fixture.rs");
fn deadline() -> Deadline {
    Deadline::new(30_000)
}
fn binding(f: &Fixture) -> (String, Value) {
    f.engine
        .store
        .db
        .query_row(
            "SELECT id,value FROM records WHERE kind=?1",
            [history::RECORD_KIND],
            |r| {
                Ok((
                    r.get(0)?,
                    serde_json::from_str(&r.get::<_, String>(1)?).unwrap(),
                ))
            },
        )
        .unwrap()
}
#[test]
fn checked_completion_creates_one_signed_job_and_offline_status_uses_read_only_store() {
    let f = Fixture::new(true);
    let (s, review) = f.complete();
    let o = f.options();
    let d = deadline();
    let before_head = git(&f.git, f.root.path(), &["rev-parse", "HEAD"]);
    let first = history::create(&f.write(&o), &s, &jobs::Plan::default(), &d).unwrap();
    assert_eq!(first["bindingStateWritten"], true);
    assert_eq!(first["headCommitId"], f.head);
    assert_eq!(first["headSnapshotId"], review["review"]["headSnapshotId"]);
    assert_eq!(first["currentQueueSelectionChecked"], true);
    assert_eq!(first["currentHeadVerified"], false);
    assert_eq!(first["history"]["revision"], 0);
    assert_eq!(first["modelCalls"], 0);
    assert_eq!(first["networkRequests"], 0);
    assert!(!first.to_string().contains(TOKEN));
    let before = f.facts();
    let second = history::create(&f.write(&o), &s, &jobs::Plan::default(), &d).unwrap();
    assert_eq!(second["bindingId"], first["bindingId"]);
    assert_eq!(second["bindingStateWritten"], false);
    assert_eq!(f.facts(), before);
    std::fs::rename(
        f.root.path().join(".git"),
        f.root.path().join("owned-git-offline"),
    )
    .unwrap();
    let store = yoke::store::Store::open_existing_read_only(f.root.path(), &d).unwrap();
    let mut historical = s.clone();
    historical.expected_queue_revision = None;
    let result = history::status(
        &history::Context {
            store: &store,
            host: &f.host,
            options: &o,
        },
        &historical,
        &d,
    )
    .unwrap();
    assert_eq!(result["history"]["revision"], 0);
    assert_eq!(result["currentQueueSelectionChecked"], false);
    assert_eq!(f.facts(), before);
    assert_eq!(before_head, f.head);
    assert_eq!(
        std::fs::read(f.root.path().join(PATH)).unwrap(),
        SOURCE.as_bytes()
    );
}
#[test]
fn resumable_batches_use_exact_revisions_and_never_update_checkout_or_latest() {
    let f = Fixture::new(true);
    let (s, _) = f.complete();
    let o = f.options();
    let d = deadline();
    history::create(&f.write(&o), &s, &jobs::Plan::default(), &d).unwrap();
    let latest = f.engine.store.latest().unwrap();
    let step = history::step(&f.write(&o), &s, 0, &f.git, &d).unwrap();
    assert_eq!(step["history"]["revision"], 1);
    assert_eq!(step["history"]["visitedCommits"], 1);
    let before = f.facts();
    assert!(
        history::step(&f.write(&o), &s, 0, &f.git, &d)
            .unwrap_err()
            .to_string()
            .contains("stale_revision")
    );
    assert_eq!(f.facts(), before);
    let complete = history::run(&f.write(&o), &s, 1, 8, &f.git, &d).unwrap();
    assert_eq!(complete["history"]["state"], "complete");
    assert_eq!(complete["history"]["visitedCommits"], 2);
    assert_eq!(complete["complete"], false);
    assert_eq!(complete["history"]["traversalExhausted"], true);
    assert_eq!(f.engine.store.latest().unwrap(), latest);
    assert_eq!(git(&f.git, f.root.path(), &["rev-parse", "HEAD"]), f.head);
    assert_eq!(
        std::fs::read(f.root.path().join(PATH)).unwrap(),
        SOURCE.as_bytes()
    );
    let restarted = Engine::open(f.root.path()).unwrap();
    let read = history::Context {
        store: &restarted.store,
        host: &f.host,
        options: &o,
    };
    assert_eq!(
        history::status(&read, &s, &d).unwrap()["history"]["state"],
        "complete"
    );
}
#[test]
fn changed_queue_blocks_acquisition_but_allows_historical_pause_and_status() {
    let f = Fixture::new(true);
    let (mut s, review) = f.complete();
    let o = f.options();
    let d = deadline();
    history::create(&f.write(&o), &s, &jobs::Plan::default(), &d).unwrap();
    app::select(
        &f.engine.store,
        &f.host,
        review["jobId"].as_str().unwrap(),
        2,
        &f.worker.queue,
        &d,
    )
    .unwrap();
    let before = f.facts();
    assert!(history::step(&f.write(&o), &s, 0, &f.git, &d).is_err());
    assert_eq!(f.facts(), before);
    assert_eq!(
        history::status(&f.read(&o), &s, &d).unwrap()["history"]["revision"],
        0
    );
    let paused = history::transition(&f.read(&o), &s, 0, jobs::State::Paused, &d).unwrap();
    assert_eq!(paused["history"]["revision"], 1);
    assert!(history::transition(&f.read(&o), &s, 1, jobs::State::Active, &d).is_err());
    s.expected_queue_revision = Some(3);
    assert_eq!(
        history::transition(&f.read(&o), &s, 1, jobs::State::Active, &d).unwrap()["history"]["revision"],
        2
    );
    assert_eq!(
        history::transition(&f.read(&o), &s, 2, jobs::State::Cancelled, &d).unwrap()["history"]["state"],
        "cancelled"
    );
    assert!(history::transition(&f.read(&o), &s, 3, jobs::State::Active, &d).is_err());
}
#[test]
fn permissions_output_and_sql_failure_cannot_leave_half_an_app_job() {
    let f = Fixture::new(true);
    let (s, _) = f.complete();
    let mut o = f.options();
    let d = deadline();
    let before = f.facts();
    o.allow_app_history = false;
    assert!(history::create(&f.write(&o), &s, &jobs::Plan::default(), &d).is_err());
    o.allow_app_history = true;
    o.jobs.acquisition.max_output_bytes = 1024;
    assert!(history::create(&f.write(&o), &s, &jobs::Plan::default(), &d).is_err());
    assert_eq!(f.facts(), before);
    o.jobs.acquisition.max_output_bytes = 4_000_000;
    f.engine.store.db.execute_batch("CREATE TEMP TRIGGER fail_app_history BEFORE INSERT ON records WHEN NEW.kind='github_app_history_binding_v1' BEGIN SELECT RAISE(ABORT,'owned SQL fixture failure'); END;").unwrap();
    let error = history::create(&f.write(&o), &s, &jobs::Plan::default(), &d)
        .unwrap_err()
        .to_string();
    assert!(!error.contains("owned SQL fixture failure"));
    assert_eq!(f.facts(), before);
    f.engine
        .store
        .db
        .execute_batch("DROP TRIGGER fail_app_history;")
        .unwrap();
    history::create(&f.write(&o), &s, &jobs::Plan::default(), &d).unwrap();
    let before = f.facts();
    o.jobs.acquisition.allow_capture = false;
    assert!(history::step(&f.write(&o), &s, 0, &f.git, &d).is_err());
    assert_eq!(f.facts(), before);
}
#[test]
fn a_disk_snapshot_or_an_unrelated_worker_record_cannot_become_history_authority() {
    let f = Fixture::new(false);
    let (mut s, _) = f.complete();
    let o = f.options();
    let before = f.facts();
    assert!(
        history::create(&f.write(&o), &s, &jobs::Plan::default(), &deadline())
            .unwrap_err()
            .to_string()
            .contains("git_head_required")
    );
    assert_eq!(f.facts(), before);
    s.completion_id = f.request.claim_id.clone();
    assert!(history::create(&f.write(&o), &s, &jobs::Plan::default(), &deadline()).is_err());
    assert_eq!(f.facts(), before);
}
#[test]
fn authenticated_original_plan_and_native_job_must_survive_every_retry() {
    let f = Fixture::new(true);
    let (s, _) = f.complete();
    let o = f.options();
    let d = deadline();
    let mut plan = jobs::Plan::default();
    plan.traversal.objects = 1;
    let created = history::create(&f.write(&o), &s, &plan, &d).unwrap();
    let before = f.facts();
    assert!(history::create(&f.write(&o), &s, &jobs::Plan::default(), &d).is_err());
    assert_eq!(f.facts(), before);
    let wrong = host(b"another public owned history signing key");
    assert!(
        history::status(
            &history::Context {
                store: &f.engine.store,
                host: &wrong,
                options: &o
            },
            &s,
            &d
        )
        .is_err()
    );
    let (id, original) = binding(&f);
    let mut changed = original.clone();
    changed["plan"]["traversal"]["objects"] = json!(16000);
    f.engine
        .store
        .record(history::RECORD_KIND, &id, &changed)
        .unwrap();
    let tampered = f.facts();
    assert!(history::status(&f.read(&o), &s, &d).is_err());
    assert_eq!(f.facts(), tampered);
    f.engine
        .store
        .record(history::RECORD_KIND, &id, &original)
        .unwrap();
    f.engine
        .store
        .db
        .execute("DELETE FROM records WHERE kind=?1", [jobs::RECORD_KIND])
        .unwrap();
    let missing = f.facts();
    assert!(history::create(&f.write(&o), &s, &plan, &d).is_err());
    assert_eq!(f.facts(), missing);
    assert_eq!(created["historyJobId"], id);
}
#[test]
fn retention_validates_typed_bindings_and_rejects_foreign_source_substitution() {
    let f = Fixture::new(true);
    let (s, _) = f.complete();
    let o = f.options();
    let d = deadline();
    history::create(&f.write(&o), &s, &jobs::Plan::default(), &d).unwrap();
    history::step(&f.write(&o), &s, 0, &f.git, &d).unwrap();
    let plan = yoke::retention::plan(&f.engine.store, &Default::default(), &d).unwrap();
    assert!(
        !plan["snapshotIds"]
            .as_array()
            .unwrap()
            .contains(&json!(f.request.head_snapshot_id))
    );
    let (id, mut value) = binding(&f);
    value["request"]["snapshotId"] = json!("f".repeat(64));
    f.engine
        .store
        .record(history::RECORD_KIND, &id, &value)
        .unwrap();
    let before = f.facts();
    assert!(yoke::retention::plan(&f.engine.store, &Default::default(), &d).is_err());
    assert_eq!(f.facts(), before);
}
#[test]
fn closed_selection_rejects_duplicate_fields_unknown_flags_and_unbounded_ids() {
    for raw in [
        format!(
            "{{\"schemaVersion\":1,\"completionId\":\"{}\",\"completionId\":\"{}\"}}",
            "a".repeat(64),
            "b".repeat(64)
        ),
        format!(
            "{{\"schemaVersion\":1,\"completionId\":\"{}\",\"allowWrites\":true}}",
            "a".repeat(64)
        ),
        format!(
            "{{\"schemaVersion\":1,\"completionId\":\"{}\"}}",
            "a".repeat(4096)
        ),
        "{\"schemaVersion\":2,\"completionId\":\"../private\"}".into(),
    ] {
        let error = history::Selection::parse(raw.as_bytes())
            .err()
            .expect("invalid selection");
        assert_eq!(error.to_string(), "github_app_history_input_invalid");
    }
}
