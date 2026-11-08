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
    let (job_key, job_raw): (String, String) = f
        .engine
        .store
        .db
        .query_row(
            "SELECT id,value FROM records WHERE kind=?1",
            [jobs::RECORD_KIND],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    let mut changed_job: Value = serde_json::from_str(&job_raw).unwrap();
    changed_job["plan"]["traversal"]["objects"] = json!(16000);
    // Consistent edits to both stored plans must still fail the original MAC.
    f.engine
        .store
        .record(jobs::RECORD_KIND, &job_key, &changed_job)
        .unwrap();
    let mut changed = original.clone();
    changed["plan"]["traversal"]["objects"] = json!(16000);
    f.engine
        .store
        .record(history::RECORD_KIND, &id, &changed)
        .unwrap();
    let tampered = f.facts();
    assert_eq!(
        history::status(&f.read(&o), &s, &d)
            .unwrap_err()
            .to_string(),
        "github_app_history_binding_invalid"
    );
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
fn signing_key_rotation_preserves_original_binding_and_current_grants_still_apply() {
    let f = Fixture::new(true);
    let (s, _) = f.complete();
    let o = f.options();
    let d = deadline();
    history::create(&f.write(&o), &s, &jobs::Plan::default(), &d).unwrap();
    let before = f.facts();
    let profile=app::HostProfile::parse(br#"{"schemaVersion":1,"appId":123,"secretCredential":"current-owned","priorSecretCredentials":["prior-owned"],"grants":[{"installationId":42,"repositoryId":9,"repository":"acme/demo"}]}"#).unwrap();
    let rotated = app::Host::with_signing_keys(
        profile,
        vec![
            app::SigningKey::from_host_bytes(b"another public owned current signing key").unwrap(),
            app::SigningKey::from_host_bytes(KEY).unwrap(),
        ],
    )
    .unwrap();
    let write = history::WriteContext {
        engine: &f.engine,
        host: &rotated,
        options: &o,
    };
    assert_eq!(
        history::create(&write, &s, &jobs::Plan::default(), &d).unwrap()["bindingStateWritten"],
        false
    );
    assert_eq!(f.facts(), before);
    let denied=app::Host::with_signing_key(app::HostProfile::parse(br#"{"schemaVersion":1,"appId":123,"secretCredential":"owned","grants":[{"installationId":42,"repositoryId":10,"repository":"acme/other"}]}"#).unwrap(),app::SigningKey::from_host_bytes(KEY).unwrap()).unwrap();
    assert!(
        history::status(
            &history::Context {
                store: &f.engine.store,
                host: &denied,
                options: &o
            },
            &s,
            &d
        )
        .is_err()
    );
    assert_eq!(f.facts(), before);
}

#[test]
fn deleting_the_signed_binding_never_adopts_a_preexisting_generic_job() {
    let f = Fixture::new(true);
    let (s, _) = f.complete();
    let o = f.options();
    let d = deadline();
    history::create(&f.write(&o), &s, &jobs::Plan::default(), &d).unwrap();
    f.engine
        .store
        .db
        .execute("DELETE FROM records WHERE kind=?1", [history::RECORD_KIND])
        .unwrap();
    let before = f.facts();
    assert_eq!(
        history::create(&f.write(&o), &s, &jobs::Plan::default(), &d)
            .unwrap_err()
            .to_string(),
        "github_app_history_foreign_job_refused"
    );
    assert_eq!(f.facts(), before);
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

#[test]
#[ignore = "Requires an outer controller owning one temporary Windows signing credential"]
fn actual_cli_lost_stdout_status_pause_resume_and_run_use_the_owned_os_key() {
    let name =
        std::env::var("YOKE_APP_HISTORY_CREDENTIAL").expect("explicit outer credential owner");
    let nonce = name.strip_prefix("test-app-history-").unwrap();
    assert!(
        nonce.len() == 32
            && nonce
                .bytes()
                .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
    );
    let loaded = yoke::host_credentials::read_github_webhook_secret(&name).unwrap();
    assert!(loaded.as_slice() == KEY);
    drop(loaded);
    let f = Fixture::new(true);
    let (s, _) = f.complete();
    let host = tempfile::tempdir().unwrap();
    let profile = json!({"schemaVersion":1,"appId":123,"secretCredential":name,
        "grants":[{"installationId":42,"repositoryId":9,"repository":"acme/demo"}]});
    for (n, v) in [
        ("profile.json", profile),
        ("selection.json", serde_json::to_value(&s).unwrap()),
    ] {
        std::fs::write(host.path().join(n), serde_json::to_vec(&v).unwrap()).unwrap();
    }
    let command = |writes: bool, capture: bool| {
        let mut c = Command::new(env!("CARGO_BIN_EXE_yoke"));
        c.arg("--root")
            .arg(f.root.path())
            .args(["--max-bytes", "4000000"]);
        if writes {
            c.arg("--allow-writes");
        }
        c.args(["app-history-job", "--profile"])
            .arg(host.path().join("profile.json"))
            .arg("--selection")
            .arg(host.path().join("selection.json"))
            .args([
                "--allow-app-history",
                "--allow-jobs",
                "--max-batch-visits",
                "1",
            ]);
        if writes {
            c.arg("--allow-capture");
        }
        if capture {
            c.arg("--trust-git");
        }
        c
    };
    let (reader, writer) = std::io::pipe().unwrap();
    drop(reader);
    let output = command(true, false)
        .arg("create")
        .stdout(writer)
        .stderr(std::process::Stdio::piped())
        .output()
        .unwrap();
    assert!(!output.status.success());
    let invoke = |writes, capture, args: &[&str]| {
        let output = command(writes, capture).args(args).output().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        serde_json::from_slice::<Value>(&output.stdout).unwrap()
    };
    assert_eq!(invoke(false, false, &["status"])["history"]["revision"], 0);
    assert_eq!(
        invoke(true, false, &["create"])["bindingStateWritten"],
        false
    );
    assert_eq!(
        invoke(true, false, &["pause", "--revision", "0"])["history"]["state"],
        "paused"
    );
    assert_eq!(
        invoke(true, false, &["resume", "--revision", "1"])["history"]["revision"],
        2
    );
    let done = invoke(
        true,
        true,
        &["run", "--revision", "2", "--max-batches", "8"],
    );
    assert_eq!(done["history"]["state"], "complete");
    assert_eq!(done["history"]["visitedCommits"], 2);
    std::fs::rename(
        f.root.path().join(".git"),
        f.root.path().join("offline-git"),
    )
    .unwrap();
    assert_eq!(
        invoke(false, false, &["status"])["history"]["state"],
        "complete"
    );
    assert_eq!(
        std::fs::read(f.root.path().join(PATH)).unwrap(),
        SOURCE.as_bytes()
    );
}

#[test]
fn worker_execution_drives_resumable_history_without_repeating_http_or_adopting_revision() {
    let f = Fixture::new(true);
    let o = f.options();
    let d = deadline();
    let provider = Provider::new(f.head.clone(), f.base.clone());
    let mut client = f.client(&provider, &d);
    let remote = worker::RunOptions {
        allow_network: true,
        ..Default::default()
    };
    let mut e = history::Execution {
        context: f.write(&o),
        client: &mut client,
        request: &f.request,
        remote: &remote,
        clock: &f.clock,
        git: &f.git,
    };
    let mut schedule = history::Schedule {
        schema_version: 1,
        expected_queue_revision: 2,
        history_revision: 0,
        max_batches: 1,
        plan: jobs::Plan::default(),
    };
    let first = history::execute(&mut e, &schedule, &d).unwrap();
    assert_eq!(first["worker"]["historical"], false);
    assert_eq!(first["worker"]["networkRequests"], 8);
    assert_eq!(first["history"]["history"]["revision"], 1);
    assert_eq!(first["history"]["history"]["visitedCommits"], 1);
    let before = f.facts();
    assert!(history::execute(&mut e, &schedule, &d).is_err());
    assert_eq!(f.facts(), before);
    assert_eq!(provider.seen.load(Ordering::SeqCst), 8);
    schedule.history_revision = 1;
    let done = history::execute(&mut e, &schedule, &d).unwrap();
    assert_eq!(done["worker"]["historical"], true);
    assert_eq!(done["worker"]["networkRequests"], 0);
    assert_eq!(done["history"]["history"]["state"], "complete");
    assert_eq!(provider.seen.load(Ordering::SeqCst), 8);
    assert_eq!(done["complete"], false);
    assert!(!done.to_string().contains(TOKEN));
}

#[test]
fn driver_denials_precede_http_and_cannot_create_worker_completions_or_history() {
    let f = Fixture::new(true);
    let d = deadline();
    let provider = Provider::new(f.head.clone(), f.base.clone());
    let mut client = f.client(&provider, &d);
    let remote = worker::RunOptions {
        allow_network: true,
        ..Default::default()
    };
    let schedule = history::Schedule {
        schema_version: 1,
        expected_queue_revision: 2,
        history_revision: 0,
        max_batches: 1,
        plan: jobs::Plan::default(),
    };
    let before = f.facts();
    for case in 0..4 {
        let mut o = f.options();
        match case {
            0 => o.allow_app_history = false,
            1 => o.jobs.allow_jobs = false,
            2 => o.jobs.acquisition.allow_capture = false,
            _ => o.jobs.acquisition.max_output_bytes = 1024,
        };
        let mut e = history::Execution {
            context: f.write(&o),
            client: &mut client,
            request: &f.request,
            remote: &remote,
            clock: &f.clock,
            git: &f.git,
        };
        assert!(history::execute(&mut e, &schedule, &d).is_err());
        assert_eq!(f.facts(), before);
        assert_eq!(provider.seen.load(Ordering::SeqCst), 0);
    }
}

#[test]
fn a_future_initial_history_revision_cannot_create_a_binding_or_native_job() {
    let f = Fixture::new(true);
    let o = f.options();
    let d = deadline();
    let provider = Provider::new(f.head.clone(), f.base.clone());
    let mut client = f.client(&provider, &d);
    let remote = worker::RunOptions {
        allow_network: true,
        ..Default::default()
    };
    let schedule = history::Schedule {
        schema_version: 1,
        expected_queue_revision: 2,
        history_revision: 7,
        max_batches: 1,
        plan: jobs::Plan::default(),
    };
    let mut e = history::Execution {
        context: f.write(&o),
        client: &mut client,
        request: &f.request,
        remote: &remote,
        clock: &f.clock,
        git: &f.git,
    };
    assert!(
        history::execute(&mut e, &schedule, &d)
            .unwrap_err()
            .to_string()
            .contains("stale_revision")
    );
    let jobs: usize = f
        .engine
        .store
        .db
        .query_row(
            "SELECT count(*) FROM records WHERE kind IN (?1,?2)",
            [history::RECORD_KIND, jobs::RECORD_KIND],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(
        jobs, 0,
        "A stale initial fence must not publish either history row"
    );
    assert_eq!(provider.seen.load(Ordering::SeqCst), 8);
    assert_eq!(
        f.facts()
            .iter()
            .filter(|(kind, _, _)| kind == worker::RECORD_KIND)
            .count(),
        2,
        "The prior worker completion remains durable"
    );
}
