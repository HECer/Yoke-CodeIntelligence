//! Real Git, restart and SQL fault tests; no network, credential or model fixture.
use serde_json::{Value, json};
use std::{collections::BTreeSet, path::Path, process::Command};
use yoke::{
    Engine,
    capture::Deadline,
    git_workflows::{self, HostGit},
    github_app::{
        HostProfile,
        digest::{
            self,
            history_jobs::{self, State},
        },
    },
    model::Snapshot,
};
fn git(g: &HostGit, root: &Path, args: &[&str]) -> String {
    let null = if cfg!(windows) { "NUL" } else { "/dev/null" };
    let out = Command::new(&g.executable)
        .current_dir(root)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", null)
        .env("GIT_CONFIG_SYSTEM", null)
        .args([
            "-c",
            "init.templateDir=",
            "-c",
            "core.autocrlf=false",
            "-c",
            "commit.gpgsign=false",
            "-c",
            "user.name=History Job Fixture",
            "-c",
            "user.email=fixture@example.invalid",
        ])
        .args(args)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).unwrap().trim().into()
}
fn profile() -> HostProfile {
    HostProfile::parse(br#"{"schemaVersion":1,"appId":123,"secretCredential":"history-jobs-no-key","grants":[{"installationId":42,"repositoryId":9,"repository":"org/repo"}]}"#).unwrap()
}
struct Fixture {
    root: tempfile::TempDir,
    engine: Engine,
    git: HostGit,
    head: Snapshot,
    request: digest::PreviewRequest,
    commits: Vec<String>,
}
impl Fixture {
    fn new(count: usize) -> Self {
        Self::format(count, "sha1")
    }
    fn format(count: usize, format: &str) -> Self {
        let root = tempfile::tempdir().unwrap();
        let g = HostGit::discover().unwrap();
        git(
            &g,
            root.path(),
            &["init", "-q", &format!("--object-format={format}")],
        );
        std::fs::write(root.path().join(".gitignore"), ".yoke/\n").unwrap();
        let mut commits = vec![];
        for i in 0..count {
            std::fs::write(
                root.path().join("café.rs"),
                format!("// 🦀 café\r\npub fn value()->u32{{{i}}}\r\n"),
            )
            .unwrap();
            git(&g, root.path(), &["add", "--all"]);
            git(
                &g,
                root.path(),
                &["commit", "-q", "-m", &format!("commit {i}")],
            );
            commits.push(git(&g, root.path(), &["rev-parse", "HEAD"]));
        }
        let d = Deadline::new(60_000);
        let mut engine = Engine::open(root.path()).unwrap();
        let epoch = if format == "sha256" {
            yoke::analysis::AnalyzerEpoch::V9
        } else {
            yoke::analysis::AnalyzerEpoch::V8
        };
        engine.capture_epoch = epoch;
        let origin = engine.index(&d).unwrap().0;
        let head = git_workflows::capture_commit_at(
            epoch,
            &mut engine,
            &g,
            &origin,
            commits.last().unwrap(),
            &d,
        )
        .unwrap()
        .0;
        let request = digest::PreviewRequest {
            schema_version: 1,
            snapshot_id: head.id.clone(),
            installation_id: 42,
            repository_id: 9,
            repository: "org/repo".into(),
        };
        Self {
            root,
            engine,
            git: g,
            head,
            request,
            commits,
        }
    }
    fn create(&self, o: &history_jobs::Options, p: &history_jobs::Plan) -> Value {
        history_jobs::create(
            &self.engine,
            &context(o),
            &id(),
            &self.request,
            p,
            &Deadline::new(30_000),
        )
        .unwrap()
    }
    fn step(&self, o: &history_jobs::Options, revision: u64) -> anyhow::Result<Value> {
        history_jobs::step(
            &self.engine,
            &context(o),
            &id(),
            revision,
            &self.git,
            &Deadline::new(30_000),
        )
    }
    fn status(&self, o: &history_jobs::Options) -> anyhow::Result<Value> {
        history_jobs::status(
            &self.engine.store,
            &context(o),
            &id(),
            &Deadline::new(30_000),
        )
    }
    fn row(&self) -> (String, Value) {
        self.engine
            .store
            .db
            .query_row(
                "SELECT id,value FROM records WHERE kind=?1",
                [history_jobs::RECORD_KIND],
                |r| {
                    Ok((
                        r.get(0)?,
                        serde_json::from_str(&r.get::<_, String>(1)?).unwrap(),
                    ))
                },
            )
            .unwrap()
    }
    fn facts(&self) -> Value {
        let mut rows = vec![];
        for t in ["snapshots", "graphs", "records", "symbols", "documents"] {
            let n: usize = self
                .engine
                .store
                .db
                .query_row(&format!("SELECT count(*) FROM {t}"), [], |r| r.get(0))
                .unwrap();
            rows.push((t, n));
        }
        json!({"counts":rows,"latest":self.engine.store.latest().unwrap(),"job":self.row().1})
    }
}
fn id() -> String {
    "7".repeat(64)
}
fn options() -> history_jobs::Options {
    history_jobs::Options {
        allow_jobs: true,
        acquisition: digest::acquisition::Options {
            allow_capture: true,
            allow_writes: true,
            ..Default::default()
        },
        max_batch_visits: 3,
        ..Default::default()
    }
}
// Static test-owned host configuration, with no real key lookup.
fn context(o: &history_jobs::Options) -> history_jobs::Context<'_> {
    static PROFILE: std::sync::OnceLock<HostProfile> = std::sync::OnceLock::new();
    static DIGEST: std::sync::OnceLock<digest::Options> = std::sync::OnceLock::new();
    history_jobs::Context {
        profile: PROFILE.get_or_init(profile),
        digest: DIGEST.get_or_init(digest::Options::default),
        options: o,
    }
}
#[test]
fn history_over_mapping_limit_resumes_after_real_store_restart_and_never_changes_checkout_or_latest()
 {
    let mut f = Fixture::new(38);
    let o = options();
    let p = history_jobs::Plan::default();
    assert_eq!(f.create(&o, &p)["revision"], 0);
    let source = std::fs::read(f.root.path().join("café.rs")).unwrap();
    let index = std::fs::read(f.root.path().join(".git/index")).unwrap();
    let latest = f.engine.store.latest().unwrap();
    let mut revision = 0;
    let mut rows = BTreeSet::new();
    let mut batches = 0;
    loop {
        let result = f.step(&o, revision).unwrap();
        batches += 1;
        assert_eq!(result["revision"], revision + 1);
        assert_eq!(result["complete"], false);
        assert_eq!(result["sourceBodiesReplayed"], false);
        assert_eq!(result["batch"]["mappedSourceBodiesReplayed"], true);
        assert_eq!(result["batch"]["historyMapping"]["unavailableCommits"], 0);
        for row in result["batch"]["commits"].as_array().unwrap() {
            assert!(rows.insert(row["sha"].as_str().unwrap().to_owned()));
        }
        revision += 1;
        let done = result["state"] == "complete";
        let root = f.root.path().to_owned();
        drop(f.engine);
        f.engine = Engine::open(&root).unwrap();
        let status = f.status(&o).unwrap();
        assert_eq!(status["revision"], revision);
        if done {
            assert_eq!(status["traversalExhausted"], true);
            break;
        }
        assert!(batches < 30);
    }
    assert!(batches > 10);
    assert_eq!(rows, f.commits.iter().cloned().collect());
    assert_eq!(f.status(&o).unwrap()["recordedCaptureReferences"], 37);
    assert_eq!(
        std::fs::read(f.root.path().join("café.rs")).unwrap(),
        source
    );
    assert_eq!(
        std::fs::read(f.root.path().join(".git/index")).unwrap(),
        index
    );
    assert_eq!(f.engine.store.latest().unwrap(), latest);
    assert!(f.step(&o, revision).is_err());
}
#[test]
fn lost_ack_stale_revision_pause_resume_cancel_and_current_grants() {
    let f = Fixture::format(6, "sha256");
    let o = options();
    let p = history_jobs::Plan::default();
    f.create(&o, &p);
    let r = f.step(&o, 0).unwrap();
    let before = f.facts();
    assert_eq!(f.create(&o, &p)["revision"], 1);
    assert!(
        f.step(&o, 0)
            .unwrap_err()
            .to_string()
            .contains("stale_revision")
    );
    assert_eq!(f.facts(), before);
    assert_eq!(r["revision"], 1);
    let pause = history_jobs::transition(
        &f.engine.store,
        &context(&o),
        &id(),
        1,
        State::Paused,
        &Deadline::new(10_000),
    )
    .unwrap();
    assert_eq!(pause["revision"], 2);
    assert!(f.step(&o, 2).is_err());
    history_jobs::transition(
        &f.engine.store,
        &context(&o),
        &id(),
        2,
        State::Active,
        &Deadline::new(10_000),
    )
    .unwrap();
    let revoked = HostProfile::parse(br#"{"schemaVersion":1,"appId":123,"secretCredential":"history-jobs-no-key","grants":[{"installationId":42,"repositoryId":10,"repository":"org/revoked"}]}"#).unwrap();
    let c = history_jobs::Context {
        profile: &revoked,
        ..context(&o)
    };
    assert!(history_jobs::step(&f.engine, &c, &id(), 3, &f.git, &Deadline::new(10_000)).is_err());
    history_jobs::transition(
        &f.engine.store,
        &context(&o),
        &id(),
        3,
        State::Cancelled,
        &Deadline::new(10_000),
    )
    .unwrap();
    assert!(
        history_jobs::transition(
            &f.engine.store,
            &context(&o),
            &id(),
            4,
            State::Active,
            &Deadline::new(10_000)
        )
        .is_err()
    );
    assert_eq!(f.status(&o).unwrap()["state"], "cancelled");
}
#[test]
fn output_and_late_sql_failure_roll_back_job_and_all_new_captures() {
    let f = Fixture::new(6);
    let o = options();
    f.create(&o, &history_jobs::Plan::default());
    let before = f.facts();
    f.engine.store.db.execute_batch("CREATE TRIGGER deny_job_update BEFORE UPDATE ON records WHEN NEW.kind='native_digest_history_job_v1' BEGIN SELECT RAISE(ABORT,'fault'); END;").unwrap();
    assert!(f.step(&o, 0).is_err());
    assert_eq!(f.facts(), before);
    f.engine
        .store
        .db
        .execute_batch("DROP TRIGGER deny_job_update;")
        .unwrap();
    let mut small = options();
    small.acquisition.max_output_bytes = 1024;
    assert!(f.step(&small, 0).is_err());
    assert_eq!(f.facts(), before);
    assert_eq!(f.step(&o, 0).unwrap()["revision"], 1);
}
#[test]
fn cumulative_capture_and_source_limits_do_not_reset_between_batches() {
    for fault in ["captures", "files", "source", "commit", "tree", "entries"] {
        let f = Fixture::new(7);
        let mut o = options();
        o.max_batch_visits = 1;
        let mut p = history_jobs::Plan::default();
        match fault {
            "captures" => p.max_captures = 1,
            "files" => p.max_candidate_files = 2,
            "source" => p.max_source_bytes = 70,
            "commit" => p.max_commit_bytes = 400,
            "tree" => p.max_tree_bytes = 250,
            "entries" => p.max_tree_entries = 2,
            _ => unreachable!(),
        };
        f.create(&o, &p);
        let first = f.step(&o, 0).unwrap();
        assert_eq!(first["revision"], 1, "{fault}");
        let before = f.facts();
        assert!(f.step(&o, 1).is_err(), "{fault}");
        assert_eq!(f.facts(), before, "{fault}");
    }
}
#[test]
fn corrupt_or_unrelated_proof_and_reordered_or_missing_visits_never_resume() {
    for fault in [
        "raw",
        "visit",
        "unrelated",
        "missing",
        "complete",
        "capture",
        "permission",
        "unknown",
    ] {
        let f = Fixture::new(7);
        let o = options();
        f.create(&o, &history_jobs::Plan::default());
        f.step(&o, 0).unwrap();
        let (key, mut job) = f.row();
        match fault {
            "raw" => {
                let key = job["proof"]["objects"]
                    .as_object()
                    .unwrap()
                    .keys()
                    .next()
                    .unwrap()
                    .clone();
                job["proof"]["objects"][key] = json!("tampered");
            }
            "visit" => job["proof"]["visited"].as_array_mut().unwrap().swap(0, 1),
            "unrelated" => {
                job["proof"]["objects"]["1".repeat(40)] = json!("foreign");
            }
            "missing" => {
                let key = job["proof"]["visited"][0].as_str().unwrap().to_owned();
                job["proof"]["objects"]
                    .as_object_mut()
                    .unwrap()
                    .remove(&key);
            }
            "complete" => job["state"] = json!("complete"),
            "capture" => {
                job["captures"]["1".repeat(40)] = json!("9".repeat(64));
            }
            "permission" => job["allowWrites"] = json!(true),
            "unknown" => job["proof"]["ready"] = json!([]),
            _ => unreachable!(),
        }
        f.engine
            .store
            .record(history_jobs::RECORD_KIND, &key, &job)
            .unwrap();
        let before = f.facts();
        assert!(f.status(&o).is_err(), "{fault}");
        assert!(f.step(&o, 1).is_err(), "{fault}");
        assert_eq!(f.facts(), before);
    }
}
#[test]
fn default_permission_caller_transaction_and_durability_refuse_before_progress() {
    let f = Fixture::new(4);
    let o = options();
    f.create(&o, &history_jobs::Plan::default());
    let before = f.facts();
    assert!(f.step(&history_jobs::Options::default(), 0).is_err());
    f.engine.store.db.execute_batch("BEGIN IMMEDIATE;").unwrap();
    assert!(f.step(&o, 0).is_err());
    f.engine.store.db.execute_batch("ROLLBACK;").unwrap();
    f.engine
        .store
        .db
        .execute_batch("PRAGMA synchronous=NORMAL;")
        .unwrap();
    assert!(f.step(&o, 0).is_err());
    f.engine
        .store
        .db
        .execute_batch("PRAGMA synchronous=FULL;")
        .unwrap();
    assert_eq!(f.facts(), before);
    assert!(
        history_jobs::step(&f.engine, &context(&o), &id(), 0, &f.git, &Deadline::new(0)).is_err()
    );
    assert_eq!(f.facts(), before);
}
#[test]
fn typed_retention_keeps_head_origin_and_every_recorded_capture_but_refuses_forgery() {
    let f = Fixture::new(5);
    let o = options();
    f.create(&o, &history_jobs::Plan::default());
    f.step(&o, 0).unwrap();
    let job = f.row().1;
    let report =
        yoke::retention::plan(&f.engine.store, &Default::default(), &Deadline::new(30_000))
            .unwrap();
    assert_eq!(report["unknownDependencyKinds"], json!([]));
    let retired = report["snapshotIds"].as_array().unwrap();
    assert!(!retired.contains(&json!(f.head.id)));
    for id in job["captures"].as_object().unwrap().values() {
        assert!(!retired.contains(id));
    }
    let (key, mut forged) = f.row();
    let first = forged["captures"]
        .as_object()
        .unwrap()
        .keys()
        .next()
        .unwrap()
        .clone();
    forged["captures"][first] = json!(f.head.id);
    f.engine
        .store
        .record(history_jobs::RECORD_KIND, &key, &forged)
        .unwrap();
    assert!(
        yoke::retention::plan(&f.engine.store, &Default::default(), &Deadline::new(30_000))
            .is_err()
    );
}
#[test]
fn branched_clock_skew_and_equal_dates_checkpoint_partial_frontiers_before_selecting() {
    for side_date in [300, 200] {
        let mut f = Fixture::new(1);
        let tree = git(&f.git, f.root.path(), &["rev-parse", "HEAD^{tree}"]);
        let make = |parents: &[&str], date: u64, message: &str| -> String {
            let mut command = Command::new(&f.git.executable);
            command
                .current_dir(f.root.path())
                .env("GIT_CONFIG_NOSYSTEM", "1")
                .env(
                    "GIT_CONFIG_GLOBAL",
                    if cfg!(windows) { "NUL" } else { "/dev/null" },
                )
                .env("GIT_AUTHOR_NAME", "Cursor Fixture")
                .env("GIT_AUTHOR_EMAIL", "fixture@example.invalid")
                .env("GIT_COMMITTER_NAME", "Cursor Fixture")
                .env("GIT_COMMITTER_EMAIL", "fixture@example.invalid")
                .env("GIT_AUTHOR_DATE", format!("@{date} +0000"))
                .env("GIT_COMMITTER_DATE", format!("@{date} +0000"))
                .args([
                    "-c",
                    "commit.gpgsign=false",
                    "commit-tree",
                    &tree,
                    "-m",
                    message,
                ]);
            for parent in parents {
                command.args(["-p", parent]);
            }
            let out = command.output().unwrap();
            assert!(
                out.status.success(),
                "{}",
                String::from_utf8_lossy(&out.stderr)
            );
            String::from_utf8(out.stdout).unwrap().trim().into()
        };
        let a = make(&[], 100, "root");
        let b = make(&[&a], 500, "late parent");
        let c = make(&[&a], side_date, "side");
        let d = make(&[&b], 200, "early child");
        let h = make(&[&d, &c], 600, "merge");
        git(&f.git, f.root.path(), &["update-ref", "HEAD", &h]);
        let origin_id: String = f
            .engine
            .store
            .get_record::<Value>(git_workflows::SOURCE_KIND, &f.head.id)
            .unwrap()["originSnapshotId"]
            .as_str()
            .unwrap()
            .into();
        let origin = f.engine.store.snapshot(&origin_id).unwrap();
        f.head = git_workflows::capture_commit_at(
            yoke::analysis::AnalyzerEpoch::V8,
            &mut f.engine,
            &f.git,
            &origin,
            &h,
            &Deadline::new(30_000),
        )
        .unwrap()
        .0;
        f.request.snapshot_id = f.head.id.clone();
        let mut o = options();
        if side_date == 300 {
            // The head fits, while the following parents require a later byte
            // batch. Already admitted lookahead must survive this boundary.
            o.max_batch_proof_bytes = git(&f.git, f.root.path(), &["cat-file", "-s", &h])
                .parse()
                .unwrap();
        } else {
            o.max_batch_objects = 1;
        }
        f.create(&o, &history_jobs::Plan::default());
        let mut revision = 0;
        let mut lookahead_only = false;
        loop {
            let result = f.step(&o, revision).unwrap();
            revision += 1;
            if result["batch"]["history"]["visitedCommits"] == 0 {
                lookahead_only = true;
                assert!(result["unreadFrontierObjects"].as_u64().unwrap() > 0);
            }
            if result["state"] == "complete" {
                break;
            }
            assert!(revision < 20);
        }
        assert!(lookahead_only);
        let expected = if side_date == 300 {
            json!([h, c, d, b, a])
        } else {
            json!([h, d, b, c, a])
        };
        assert_eq!(f.row().1["proof"]["visited"], expected);
        assert_eq!(f.status(&o).unwrap()["recordedCaptureReferences"], 4);
        // Equal dates select direct parents by insertion order, rather than OID
        // lexical order. The independent real Git walker supplies the same oracle.
        let oracle = git(&f.git, f.root.path(), &["log", "--format=%H"]);
        assert_eq!(
            f.row().1["proof"]["visited"],
            json!(oracle.lines().collect::<Vec<_>>())
        );
    }
}
#[test]
fn real_cli_creates_runs_resumes_and_reads_existing_state_without_a_signing_key() {
    let f = Fixture::new(9);
    let host = tempfile::tempdir().unwrap();
    let profile = host.path().join("profile.json");
    let request = host.path().join("request.json");
    std::fs::write(&profile,serde_json::to_vec(&json!({"schemaVersion":1,"appId":123,"secretCredential":"history-jobs-no-key","grants":[{"installationId":42,"repositoryId":9,"repository":"org/repo"}]})).unwrap()).unwrap();
    std::fs::write(&request, serde_json::to_vec(&f.request).unwrap()).unwrap();
    let root = f.root.path().to_owned();
    drop(f.engine);
    let call = |operation: &[&str], write: bool, trust: bool| {
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_yoke"));
        cmd.arg("--root")
            .arg(&root)
            .args(["--max-bytes", "4000000"]);
        if write {
            cmd.arg("--allow-writes");
        }
        cmd.args([
            "digest-history-job",
            "--allow-jobs",
            "--id",
            &id(),
            "--profile",
        ])
        .arg(&profile)
        .args(["--max-batch-visits", "2"]);
        if write {
            cmd.arg("--allow-capture");
        }
        if trust {
            cmd.arg("--trust-git");
        }
        cmd.args(operation);
        cmd.output().unwrap()
    };
    let output = call(
        &["create", "--request", request.to_str().unwrap()],
        true,
        false,
    );
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let output = call(
        &["run", "--revision", "0", "--max-batches", "2"],
        true,
        true,
    );
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let value: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["revision"], 2);
    assert_eq!(value["state"], "active");
    let output = call(&["status"], false, false);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        serde_json::from_slice::<Value>(&output.stdout).unwrap()["revision"],
        2
    );
    let output = call(
        &["run", "--revision", "2", "--max-batches", "8"],
        true,
        true,
    );
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let value: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["state"], "complete");
    assert_eq!(value["visitedCommits"], 9);
    let output = call(&["step", "--revision", "0"], true, true);
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
}
#[test]
fn cli_denials_precede_host_files_and_never_initialize_missing_store() {
    let root = tempfile::tempdir().unwrap();
    for flags in [
        vec![],
        vec!["--allow-jobs"],
        vec!["--allow-jobs", "--allow-capture"],
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_yoke"))
            .arg("--root")
            .arg(root.path())
            .args([
                "digest-history-job",
                "--profile",
                "missing.json",
                "--id",
                &id(),
            ])
            .args(flags)
            .args(["step", "--revision", "0"])
            .output()
            .unwrap();
        assert!(!output.status.success());
        assert!(output.stdout.is_empty());
        assert!(String::from_utf8_lossy(&output.stderr).contains("permission_required"));
        assert!(!root.path().join(".yoke").exists());
    }
}
#[test]
fn object_proof_and_parent_limits_are_cumulative_across_restartable_steps() {
    for fault in ["objects", "bytes", "parents"] {
        let f = Fixture::new(5);
        let mut o = options();
        o.max_batch_visits = 1;
        let mut p = history_jobs::Plan::default();
        match fault {
            "objects" => p.traversal.objects = 1,
            "bytes" => {
                p.traversal.raw_bytes = git(
                    &f.git,
                    f.root.path(),
                    &["cat-file", "-s", f.commits.last().unwrap()],
                )
                .parse::<usize>()
                .unwrap()
            }
            "parents" => p.traversal.parent_ids = 1,
            _ => unreachable!(),
        }
        f.create(&o, &p);
        f.step(&o, 0).unwrap();
        let before = f.facts();
        assert!(f.step(&o, 1).is_err(), "{fault}");
        assert_eq!(f.facts(), before);
        assert_eq!(f.status(&o).unwrap()["traversalExhausted"], false);
    }
}
#[test]
fn swapped_valid_local_capture_references_cannot_supply_another_commits_mapping() {
    let f = Fixture::new(7);
    let o = options();
    f.create(&o, &history_jobs::Plan::default());
    f.step(&o, 0).unwrap();
    let (key, mut job) = f.row();
    let map = job["captures"].as_object_mut().unwrap();
    // The last visited commit's direct parent will be reused in the next batch.
    let a = f.commits[3].clone();
    let b = f.commits[4].clone();
    let va = map[&a].clone();
    let vb = map[&b].clone();
    map.insert(a, vb);
    map.insert(b, va);
    f.engine
        .store
        .record(history_jobs::RECORD_KIND, &key, &job)
        .unwrap();
    let before = f.facts();
    assert!(f.step(&o, 1).is_err());
    assert_eq!(f.facts(), before);
}
#[test]
fn missing_git_object_and_sql_lock_deadline_leave_the_checkpoint_unchanged() {
    let f = Fixture::new(5);
    let o = options();
    f.create(&o, &history_jobs::Plan::default());
    let before = f.facts();
    let oid = &f.commits[3];
    let object = f
        .root
        .path()
        .join(".git/objects")
        .join(&oid[..2])
        .join(&oid[2..]);
    let hidden = object.with_extension("unavailable");
    std::fs::rename(&object, &hidden).unwrap();
    assert!(f.step(&o, 0).is_err());
    assert_eq!(f.facts(), before);
    std::fs::rename(hidden, object).unwrap();
    let writer = rusqlite::Connection::open(f.root.path().join(".yoke/index.sqlite")).unwrap();
    writer.execute_batch("BEGIN IMMEDIATE;").unwrap();
    let start = std::time::Instant::now();
    assert!(
        history_jobs::step(
            &f.engine,
            &context(&o),
            &id(),
            0,
            &f.git,
            &Deadline::new(40)
        )
        .is_err()
    );
    assert!(start.elapsed() < std::time::Duration::from_millis(600));
    writer.execute_batch("ROLLBACK;").unwrap();
    assert_eq!(f.facts(), before);
    assert_eq!(f.step(&o, 0).unwrap()["revision"], 1);
}
