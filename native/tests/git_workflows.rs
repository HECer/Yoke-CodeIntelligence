use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};
use tempfile::TempDir;
use yoke::{
    Engine,
    capture::Deadline,
    git_workflows::{self, HostGit, WorkflowOptions},
};

// These are semantic/admission regressions with real Git processes, not a load
// benchmark. Keep their 30 s request deadlines while bounding fixture contention
// on hosts where the default Rust runner starts all cases at once.
static ACTIVE_FIXTURES: std::sync::Mutex<usize> = std::sync::Mutex::new(0);
static FIXTURE_AVAILABLE: std::sync::Condvar = std::sync::Condvar::new();
struct FixturePermit;
impl FixturePermit {
    fn acquire() -> Self {
        let active = ACTIVE_FIXTURES
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        let mut active = FIXTURE_AVAILABLE
            .wait_while(active, |count| *count >= 4)
            .unwrap_or_else(|error| error.into_inner());
        *active += 1;
        Self
    }
}
impl Drop for FixturePermit {
    fn drop(&mut self) {
        let mut active = ACTIVE_FIXTURES
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        *active -= 1;
        FIXTURE_AVAILABLE.notify_one();
    }
}
struct Fixture {
    dir: TempDir,
    _permit: FixturePermit,
}
impl Fixture {
    fn path(&self) -> &Path {
        self.dir.path()
    }
}

fn executable() -> PathBuf {
    std::env::split_paths(&std::env::var_os("PATH").unwrap())
        .map(|path| path.join(if cfg!(windows) { "git.exe" } else { "git" }))
        .find(|path| path.is_absolute() && path.is_file())
        .unwrap()
        .canonicalize()
        .unwrap()
}
fn git(root: &Path, args: &[&str]) -> String {
    let result = Command::new(executable())
        .current_dir(root)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env(
            "GIT_CONFIG_GLOBAL",
            if cfg!(windows) { "NUL" } else { "/dev/null" },
        )
        .args([
            "-c",
            "init.templateDir=",
            "-c",
            "user.name=Test Author",
            "-c",
            "user.email=test@example.invalid",
            "-c",
            "core.autocrlf=false",
        ])
        .args(args)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "git failed: {}",
        String::from_utf8_lossy(&result.stderr)
    );
    String::from_utf8(result.stdout).unwrap().trim().into()
}
fn fixture() -> (Fixture, HostGit, String) {
    let permit = FixturePermit::acquire();
    let dir = Fixture {
        dir: TempDir::new().unwrap(),
        _permit: permit,
    };
    git(dir.path(), &["init", "-b", "main"]);
    fs::write(
        dir.path().join("main.rs"),
        "fn changed() {}\nfn caller(){changed();}\n",
    )
    .unwrap();
    fs::write(
        dir.path().join("CODEOWNERS"),
        "*.rs @rust-team\n/main.rs @specific-owner\n",
    )
    .unwrap();
    git(dir.path(), &["add", "--", "main.rs", "CODEOWNERS"]);
    git(dir.path(), &["commit", "-m", "base"]);
    let base = git(dir.path(), &["rev-parse", "HEAD"]);
    (
        dir,
        HostGit {
            executable: executable(),
        },
        base,
    )
}
#[test]
fn historical_git_refuses_disk_ignored_capture_options_before_publication() {
    let (dir, host, _) = fixture();
    fs::create_dir(dir.path().join("selected")).unwrap();
    fs::write(dir.path().join("selected/public.rs"), "fn public() {}\n").unwrap();
    git(dir.path(), &["add", "--", "selected/public.rs"]);
    git(dir.path(), &["commit", "-m", "unignored selected source"]);
    let mut engine = Engine::open(dir.path()).unwrap();
    engine.capture_options.include_ignored_dirs = vec!["selected".into()];
    let d = Deadline::new(30000);
    let (snapshot, _, _, _) = engine.index(&d).unwrap();
    assert!(
        !yoke::capture::source_scope_policy(&engine.store, &snapshot)
            .unwrap()
            .ignored_at_capture("selected/public.rs")
            .unwrap()
    );
    let before: (i64, i64) = engine
        .store
        .db
        .query_row(
            "SELECT (SELECT count(*) FROM snapshots), (SELECT count(*) FROM records)",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    for selected_host in [
        host,
        HostGit {
            executable: dir.path().join("must-not-be-executed.exe"),
        },
    ] {
        let error =
            git_workflows::analyze(&mut engine, &selected_host, &WorkflowOptions::default(), &d)
                .expect_err("Disk ignored-source proof must never authorize historical Git source");
        assert!(
            error
                .to_string()
                .contains("independent committed ignore proof"),
            "Wrong admission error: {error:#}"
        );
    }
    let after: (i64, i64) = engine
        .store
        .db
        .query_row(
            "SELECT (SELECT count(*) FROM snapshots), (SELECT count(*) FROM records)",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(
        before, after,
        "Rejected Git scope must not publish historical snapshots or records"
    );
}
#[test]
fn working_diff_binds_git_base_and_native_source_and_incoming_impact() {
    let (dir, host, base) = fixture();
    fs::write(
        dir.path().join("main.rs"),
        "fn changed(){let added=1;}\nfn caller(){changed();}\n",
    )
    .unwrap();
    let before = fs::read(dir.path().join("main.rs")).unwrap();
    let index_before = fs::read(dir.path().join(".git/index")).unwrap();
    let mut engine = Engine::open(dir.path()).unwrap();
    let report = git_workflows::analyze(
        &mut engine,
        &host,
        &WorkflowOptions::default(),
        &Deadline::new(30000),
    )
    .unwrap();
    assert_eq!(report["baseCommit"], base);
    assert_ne!(report["baseSnapshotId"], report["headSnapshotId"]);
    assert!(
        report["changedSymbols"]
            .as_array()
            .unwrap()
            .iter()
            .any(|node| node["name"] == "changed")
    );
    assert!(
        report["impact"]
            .as_array()
            .unwrap()
            .iter()
            .any(|node| node["name"] == "caller")
    );
    assert!(
        report["ownerSuggestions"]
            .as_array()
            .unwrap()
            .iter()
            .any(|owner| owner["owner"] == "@specific-owner")
    );
    assert_eq!(fs::read(dir.path().join("main.rs")).unwrap(), before);
    assert_eq!(
        fs::read(dir.path().join(".git/index")).unwrap(),
        index_before
    );
}
#[test]
fn deleted_and_renamed_commit_sources_keep_base_symbols_and_exact_original_paths() {
    let (dir, host, base) = fixture();
    git(dir.path(), &["mv", "main.rs", "renamed.rs"]);
    fs::write(dir.path().join("renamed.rs"), "fn caller(){}\n").unwrap();
    git(dir.path(), &["add", "--", "renamed.rs"]);
    git(dir.path(), &["commit", "-m", "rename and delete"]);
    let mut engine = Engine::open(dir.path()).unwrap();
    let options = WorkflowOptions {
        base_ref: base,
        head_ref: Some("HEAD".into()),
        ..Default::default()
    };
    let report =
        git_workflows::analyze(&mut engine, &host, &options, &Deadline::new(30000)).unwrap();
    assert!(
        report["changedSymbols"]
            .as_array()
            .unwrap()
            .iter()
            .any(|node| node["name"] == "changed"
                && node["side"] == "base"
                && node["path"] == "main.rs")
    );
    assert!(
        report["changes"]
            .as_array()
            .unwrap()
            .iter()
            .any(|change| change["basePath"] == "main.rs")
    );
}
#[test]
fn workflow_never_runs_repo_filters_fsmonitor_hooks_or_ref_options() {
    let (dir, host, _) = fixture();
    fs::write(
        dir.path().join(".gitattributes"),
        "*.rs filter=evil diff=evil\n",
    )
    .unwrap();
    git(dir.path(), &["add", "--", ".gitattributes"]);
    git(dir.path(), &["commit", "-m", "attributes"]);
    let config = dir.path().join(".git/config");
    let mut text = fs::read_to_string(&config).unwrap();
    text.push_str("\n[filter \"evil\"]\n\tclean = cmd /c echo evil>filter-ran\n\tsmudge = cmd /c echo evil>smudge-ran\n[diff \"evil\"]\n\ttextconv = cmd /c echo evil>textconv-ran\n[core]\n\tfsmonitor = cmd /c echo evil>fsmonitor-ran\n");
    fs::write(config, text).unwrap();
    fs::write(
        dir.path().join("main.rs"),
        "fn changed(){let x=2;}\nfn caller(){changed();}\n",
    )
    .unwrap();
    let mut engine = Engine::open(dir.path()).unwrap();
    git_workflows::analyze(
        &mut engine,
        &host,
        &WorkflowOptions::default(),
        &Deadline::new(30000),
    )
    .unwrap();
    for name in ["filter-ran", "smudge-ran", "textconv-ran", "fsmonitor-ran"] {
        assert!(!dir.path().join(name).exists(), "executed {name}");
    }
    let bad = WorkflowOptions {
        base_ref: "--output=ref-injection".into(),
        ..Default::default()
    };
    assert!(git_workflows::analyze(&mut engine, &host, &bad, &Deadline::new(30000)).is_err());
    assert!(!dir.path().join("ref-injection").exists());
}
#[test]
fn local_branch_overlap_and_history_owners_are_actual_metadata_candidates() {
    let (dir, host, _) = fixture();
    git(dir.path(), &["checkout", "-b", "feature"]);
    fs::write(
        dir.path().join("main.rs"),
        "fn changed(){let feature=1;}\nfn caller(){changed();}\n",
    )
    .unwrap();
    git(dir.path(), &["add", "--", "main.rs"]);
    git(dir.path(), &["commit", "-m", "feature"]);
    git(dir.path(), &["checkout", "main"]);
    fs::write(
        dir.path().join("main.rs"),
        "fn changed(){let local=1;}\nfn caller(){changed();}\n",
    )
    .unwrap();
    let mut engine = Engine::open(dir.path()).unwrap();
    let report = git_workflows::analyze(
        &mut engine,
        &host,
        &WorkflowOptions::default(),
        &Deadline::new(30000),
    )
    .unwrap();
    assert!(report["branches"].as_array().unwrap().iter().any(|branch| {
        branch["ref"] == "refs/heads/feature"
            && branch["overlappingPaths"]
                .as_array()
                .unwrap()
                .iter()
                .any(|path| path == "main.rs")
    }));
    assert!(
        report["history"]
            .as_array()
            .unwrap()
            .iter()
            .any(|commit| commit["author"] == "Test Author")
    );
    assert!(
        report["ownerSuggestions"]
            .as_array()
            .unwrap()
            .iter()
            .any(|owner| owner["source"] == "git_history")
    );
}
#[test]
fn historical_git_source_proof_rejects_missing_tree_membership() {
    let (dir, host, _) = fixture();
    let mut engine = Engine::open(dir.path()).unwrap();
    let report = git_workflows::analyze(
        &mut engine,
        &host,
        &WorkflowOptions::default(),
        &Deadline::new(30000),
    )
    .unwrap();
    let id = report["baseSnapshotId"].as_str().unwrap();
    let proof = git_workflows::validate_source(&engine.store, id, &Deadline::new(10000)).unwrap();
    assert_eq!(proof["commitId"], report["baseCommit"]);
    let mut forged = proof;
    forged["treeEntries"] = serde_json::json!([]);
    engine
        .store
        .db
        .execute(
            "UPDATE records SET value=?1 WHERE kind=?2 AND id=?3",
            rusqlite::params![forged.to_string(), git_workflows::SOURCE_KIND, id],
        )
        .unwrap();
    assert!(git_workflows::validate_source(&engine.store, id, &Deadline::new(10000)).is_err());
}
#[test]
fn impact_keeps_parallel_call_occurrences_and_absent_file_has_no_false_anchor() {
    let (dir, host, base) = fixture();
    fs::write(
        dir.path().join("main.rs"),
        "fn changed() {}\nfn caller(){changed();changed();}\n",
    )
    .unwrap();
    git(dir.path(), &["add", "--", "main.rs"]);
    git(dir.path(), &["commit", "-m", "parallel calls"]);
    fs::write(
        dir.path().join("main.rs"),
        "fn changed(){let x=2;}\nfn caller(){changed();changed();}\n",
    )
    .unwrap();
    let mut engine = Engine::open(dir.path()).unwrap();
    let report = git_workflows::analyze(
        &mut engine,
        &host,
        &WorkflowOptions::default(),
        &Deadline::new(30000),
    )
    .unwrap();
    assert!(
        report["impactObservations"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|edge| edge["side"] == "head" && edge["relation"] == "calls")
            .count()
            >= 2
    );
    fs::remove_file(dir.path().join("main.rs")).unwrap();
    let report = git_workflows::analyze(
        &mut engine,
        &host,
        &WorkflowOptions {
            base_ref: base,
            ..Default::default()
        },
        &Deadline::new(30000),
    )
    .unwrap();
    let deleted = report["changes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|change| change["status"] == "deleted")
        .unwrap();
    assert!(
        deleted["hunks"][0]["head"].is_null(),
        "absent file cannot carry invented captured source hash"
    );
}
#[test]
fn immutable_git_report_renderings_and_digests_remain_bound_after_live_changes() {
    let (dir, host, _) = fixture();
    fs::write(
        dir.path().join("main.rs"),
        "fn changed(){let x=2;}\nfn caller(){changed();}\n",
    )
    .unwrap();
    let mut engine = Engine::open(dir.path()).unwrap();
    let report = git_workflows::analyze(
        &mut engine,
        &host,
        &WorkflowOptions::default(),
        &Deadline::new(30000),
    )
    .unwrap();
    fs::write(dir.path().join("main.rs"), "fn unrelated(){}\n").unwrap();
    engine.index(&Deadline::new(10000)).unwrap();
    let loaded = git_workflows::load(
        &engine.store,
        report["artifactId"].as_str().unwrap(),
        &Deadline::new(10000),
    )
    .unwrap();
    assert_eq!(loaded, report);
    for format in ["json", "text", "markdown", "mermaid", "html"] {
        let text =
            git_workflows::render(&engine.store, &report, format, &Deadline::new(10000)).unwrap();
        assert!(text.contains("changed"), "{format}");
    }
    for (kind, path) in [
        ("file", Some("main.rs")),
        ("report", None),
        ("history", None),
    ] {
        let value =
            git_workflows::digest(&engine.store, &report, kind, path, &Deadline::new(10000))
                .unwrap();
        assert_eq!(value["reportArtifactId"], report["artifactId"]);
        assert_eq!(value["kind"], kind);
    }
    let mut forged = report;
    forged["changedSymbols"][0]["name"] = serde_json::json!("invented");
    assert!(git_workflows::render(&engine.store, &forged, "html", &Deadline::new(10000)).is_err());
}
#[test]
fn git_history_dependencies_require_captured_readonly_scope_proof() {
    let (dir, host, _) = fixture();
    fs::create_dir_all(dir.path().join("node_modules/sdk")).unwrap();
    fs::write(
        dir.path().join("node_modules/sdk/api.ts"),
        "export class NativeDependency {}\n",
    )
    .unwrap();
    git(dir.path(), &["add", "--", "node_modules/sdk/api.ts"]);
    git(dir.path(), &["commit", "-m", "dependency source"]);
    fs::write(
        dir.path().join("node_modules/sdk/api.ts"),
        "export class NativeDependency { value=1; }\n",
    )
    .unwrap();
    let mut engine = Engine::open(dir.path()).unwrap();
    engine.capture_options.read_only_dependency_dirs = vec!["node_modules".into()];
    let report = git_workflows::analyze(
        &mut engine,
        &host,
        &WorkflowOptions::default(),
        &Deadline::new(30000),
    )
    .unwrap();
    let base = engine
        .store
        .snapshot(report["baseSnapshotId"].as_str().unwrap())
        .unwrap();
    assert!(
        base.files
            .iter()
            .any(|file| file.path == "node_modules/sdk/api.ts")
    );
    assert!(
        yoke::capture::source_path_policy(&engine.store, &base, "node_modules/sdk/api.ts")
            .unwrap()
            .read_only
    );
    let digest = git_workflows::digest(
        &engine.store,
        &report,
        "file",
        Some("node_modules/sdk/api.ts"),
        &Deadline::new(10000),
    )
    .unwrap();
    assert_eq!(digest["data"]["path"], "node_modules/sdk/api.ts");
    assert_eq!(
        git_workflows::load(
            &engine.store,
            report["artifactId"].as_str().unwrap(),
            &Deadline::new(10000)
        )
        .unwrap(),
        report
    );
}
#[test]
fn missing_promisor_object_never_executes_remote_helper() {
    let (dir, host, _) = fixture();
    let oid = git(dir.path(), &["rev-parse", "HEAD:main.rs"]);
    let script = dir.path().join("hostile-fetch.sh");
    fs::write(
        &script,
        "#!/bin/sh\necho reached > hostile-fetch-marker\nexit 1\n",
    )
    .unwrap();
    let url = format!("ext::sh {}", script.to_string_lossy().replace('\\', "/"));
    git(dir.path(), &["config", "remote.origin.url", &url]);
    git(dir.path(), &["config", "remote.origin.promisor", "true"]);
    git(dir.path(), &["config", "extensions.partialClone", "origin"]);
    git(dir.path(), &["config", "protocol.ext.allow", "always"]);
    let object = dir
        .path()
        .join(".git/objects")
        .join(&oid[..2])
        .join(&oid[2..]);
    assert!(
        object
            .canonicalize()
            .unwrap()
            .starts_with(dir.path().canonicalize().unwrap())
    );
    fs::remove_file(object).unwrap();
    let mut engine = Engine::open(dir.path()).unwrap();
    let result = git_workflows::analyze(
        &mut engine,
        &host,
        &WorkflowOptions::default(),
        &Deadline::new(30000),
    );
    assert!(result.is_err(), "missing immutable object must fail closed");
    assert!(
        !dir.path().join("hostile-fetch-marker").exists(),
        "read-only Git attempted an implicit fetch"
    );
}
#[test]
fn unicode_refs_resolve_to_exact_commit_without_argument_interpretation() {
    let (dir, host, base) = fixture();
    git(dir.path(), &["branch", "funktion/änderung-日本"]);
    let mut engine = Engine::open(dir.path()).unwrap();
    let options = WorkflowOptions {
        base_ref: "funktion/änderung-日本".into(),
        ..WorkflowOptions::default()
    };
    let report =
        git_workflows::analyze(&mut engine, &host, &options, &Deadline::new(30000)).unwrap();
    assert_eq!(report["baseCommit"], base);
}
#[test]
fn unrelated_local_branch_is_reported_without_aborting_other_results() {
    let (dir, host, _) = fixture();
    let tree = git(dir.path(), &["rev-parse", "HEAD^{tree}"]);
    let unrelated = git(dir.path(), &["commit-tree", &tree, "-m", "orphan"]);
    git(
        dir.path(),
        &["update-ref", "refs/heads/unrelated", &unrelated],
    );
    let mut engine = Engine::open(dir.path()).unwrap();
    let report = git_workflows::analyze(
        &mut engine,
        &host,
        &WorkflowOptions::default(),
        &Deadline::new(30000),
    )
    .unwrap();
    let branch = report["branches"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["ref"] == "refs/heads/unrelated")
        .unwrap();
    assert_eq!(branch["relationship"], "unrelated_history");
    assert!(branch["mergeBaseCommit"].is_null());
}
#[test]
fn worktree_cache_seed_copies_verified_parser_entries_without_parent_state() {
    let (dir, host, _) = fixture();
    let mut parent = Engine::open(dir.path()).unwrap();
    let (parent_snapshot, _, _, _) = parent.index(&Deadline::new(10000)).unwrap();
    parent
        .store
        .record(
            "human_note",
            "private-parent-note",
            &serde_json::json!({"text":"parent only"}),
        )
        .unwrap();
    let child_dir = TempDir::new().unwrap();
    let child_root = child_dir.path().join("child");
    git(
        dir.path(),
        &[
            "worktree",
            "add",
            "-b",
            "seed-child",
            child_root.to_str().unwrap(),
        ],
    );
    let mut child = Engine::open(&child_root).unwrap();
    let receipt = git_workflows::seed_worktree_cache(
        &mut child,
        &host,
        dir.path(),
        &parent.store.workspace_id,
        &Deadline::new(30000),
    )
    .unwrap();
    assert!(receipt["copiedParserEntries"].as_u64().unwrap() > 0);
    assert_eq!(receipt["parentWorkspaceId"], parent.store.workspace_id);
    assert_ne!(receipt["targetWorkspaceId"], parent.store.workspace_id);
    assert!(child.store.snapshot(&parent_snapshot.id).is_err());
    assert!(
        child
            .store
            .get_record::<serde_json::Value>("human_note", "private-parent-note")
            .is_err()
    );
    assert!(
        git_workflows::seed_worktree_cache(
            &mut child,
            &host,
            dir.path(),
            "wrong-workspace",
            &Deadline::new(30000)
        )
        .is_err()
    );
}
#[test]
fn resealed_git_report_cannot_invent_impact_distance_or_omit_changes() {
    let (dir, host, _) = fixture();
    fs::write(
        dir.path().join("main.rs"),
        "fn changed(){let x=1;}\nfn caller(){changed();}\n",
    )
    .unwrap();
    let mut engine = Engine::open(dir.path()).unwrap();
    let report = git_workflows::analyze(
        &mut engine,
        &host,
        &WorkflowOptions::default(),
        &Deadline::new(30000),
    )
    .unwrap();
    for mode in ["distance", "omission"] {
        let mut forged = report.clone();
        forged.as_object_mut().unwrap().remove("artifactId");
        if mode == "distance" {
            forged["impact"][0]["distance"] = serde_json::json!(999);
        } else {
            forged["changes"] = serde_json::json!([]);
        }
        let id = engine
            .store
            .artifact(&serde_json::to_vec(&forged).unwrap())
            .unwrap();
        let mut record: serde_json::Value = engine
            .store
            .get_record(
                git_workflows::REPORT_KIND,
                report["artifactId"].as_str().unwrap(),
            )
            .unwrap();
        record["artifactId"] = serde_json::json!(id);
        engine
            .store
            .record(git_workflows::REPORT_KIND, &id, &record)
            .unwrap();
        assert!(
            git_workflows::load(&engine.store, &id, &Deadline::new(10000)).is_err(),
            "accepted invented {mode}"
        );
    }
}
#[test]
fn resealed_git_report_rejects_unknown_historical_metadata_fields() {
    let (dir, host, _) = fixture();
    let mut engine = Engine::open(dir.path()).unwrap();
    let mut report = git_workflows::analyze(
        &mut engine,
        &host,
        &WorkflowOptions::default(),
        &Deadline::new(30000),
    )
    .unwrap();
    let original = report["artifactId"].as_str().unwrap().to_owned();
    report.as_object_mut().unwrap().remove("artifactId");
    report["history"][0]["shellCommand"] = serde_json::json!("invented-metadata");
    let id = engine
        .store
        .artifact(&serde_json::to_vec(&report).unwrap())
        .unwrap();
    let mut record: serde_json::Value = engine
        .store
        .get_record(git_workflows::REPORT_KIND, &original)
        .unwrap();
    record["artifactId"] = serde_json::json!(id);
    engine
        .store
        .record(git_workflows::REPORT_KIND, &id, &record)
        .unwrap();
    assert!(git_workflows::load(&engine.store, &id, &Deadline::new(10000)).is_err());
}
#[test]
fn oversized_git_source_receipt_is_rejected_before_sqlite_cell_allocation() {
    let (dir, host, _) = fixture();
    let mut engine = Engine::open(dir.path()).unwrap();
    let report = git_workflows::analyze(
        &mut engine,
        &host,
        &WorkflowOptions::default(),
        &Deadline::new(30000),
    )
    .unwrap();
    let id = report["baseSnapshotId"].as_str().unwrap();
    engine
        .store
        .db
        .execute(
            "UPDATE records SET value=CAST(zeroblob(64000000) AS TEXT) WHERE kind=?1 AND id=?2",
            rusqlite::params![git_workflows::SOURCE_KIND, id],
        )
        .unwrap();
    let baseline = unsafe { rusqlite::ffi::sqlite3_memory_used() };
    unsafe {
        rusqlite::ffi::sqlite3_memory_highwater(1);
    }
    let result = git_workflows::validate_source(&engine.store, id, &Deadline::new(10000));
    let peak = unsafe { rusqlite::ffi::sqlite3_memory_highwater(0) };
    assert!(result.is_err());
    assert!(
        peak - baseline < 20_000_000,
        "allocated {} SQLite bytes before source receipt rejection",
        peak - baseline
    );
}
#[test]
fn typed_git_records_and_exact_commit_artifacts_are_known_retention_roots() {
    let (dir, host, _) = fixture();
    let mut engine = Engine::open(dir.path()).unwrap();
    git_workflows::analyze(
        &mut engine,
        &host,
        &WorkflowOptions::default(),
        &Deadline::new(30000),
    )
    .unwrap();
    let plan = yoke::retention::plan(
        &engine.store,
        &yoke::retention::Policy::default(),
        &Deadline::new(30000),
    )
    .unwrap();
    assert_eq!(plan["unknownDependencyKinds"], serde_json::json!([]));
}
