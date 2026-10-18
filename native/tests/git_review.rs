use std::{fs, process::Command};
use yoke::{
    Engine,
    capture::{CaptureOptions, Deadline},
    git_workflows::{self, HostGit, WorkflowOptions},
};
// SQLite's memory observer is process-global; keep every fixture in this binary
// separate from the measured admission, including the large valid-source case.
static REVIEW_SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[test]
fn committed_ignore_policy_is_applied_even_when_policy_files_are_outside_only_dirs() {
    let _serial = REVIEW_SERIAL
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let root = tempfile::tempdir().unwrap();
    let host = HostGit::discover().unwrap();
    let git = |arguments: &[&str]| {
        let output = Command::new(&host.executable)
            .current_dir(root.path())
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
            .args(arguments)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    };
    git(&["init", "--quiet"]);
    fs::create_dir_all(root.path().join("src")).unwrap();
    fs::write(root.path().join(".gitignore"), "src/private.rs\n").unwrap();
    fs::write(root.path().join("src/main.rs"), "fn public() {}\n").unwrap();
    fs::write(
        root.path().join("src/private.rs"),
        "const PRIVATE_RUNTIME_VALUE: &str = \"local\";\n",
    )
    .unwrap();
    git(&["add", ".gitignore", "src/main.rs"]);
    git(&["add", "--force", "src/private.rs"]);
    git(&["commit", "--quiet", "-m", "source"]);
    let mut engine = Engine::open(root.path()).unwrap();
    engine.capture_options = CaptureOptions {
        only_dirs: vec!["src".into()],
        ..Default::default()
    };
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
        !base.files.iter().any(|file| file.path == "src/private.rs"),
        "Committed ignored source admitted because policy file was outside onlyDirs"
    );
    assert!(
        base.omissions
            .iter()
            .any(|omission| omission.path == "src/private.rs"
                && omission.reason == "committed-ignore-rule")
    );
}

#[test]
fn parent_cache_length_admission_does_not_materialize_oversized_sqlite_values() {
    let _serial = REVIEW_SERIAL
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let root = tempfile::tempdir().unwrap();
    let child = tempfile::tempdir().unwrap();
    let host = HostGit::discover().unwrap();
    let git = |arguments: &[&str]| {
        let output = Command::new(&host.executable)
            .current_dir(root.path())
            .args([
                "-c",
                "init.templateDir=",
                "-c",
                "user.name=Test Author",
                "-c",
                "user.email=test@example.invalid",
            ])
            .args(arguments)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    };
    git(&["init", "--quiet"]);
    fs::write(root.path().join("main.rs"), "fn public() {}\n").unwrap();
    git(&["add", "main.rs"]);
    git(&["commit", "--quiet", "-m", "source"]);
    git(&[
        "worktree",
        "add",
        "--detach",
        child.path().to_str().unwrap(),
        "HEAD",
    ]);
    let deadline = Deadline::new(30000);
    let mut parent = Engine::open(root.path()).unwrap();
    let snapshot = parent.index(&deadline).unwrap().0;
    let file = &snapshot.files[0];
    let key = yoke::model::identity(&(
        yoke::model::ANALYZER,
        &file.language,
        &file.hash,
        &file.path,
    ));
    assert_eq!(
        parent
            .store
            .db
            .execute(
                "UPDATE cache SET value=CAST(zeroblob(64000000) AS TEXT) WHERE key=?1",
                [key]
            )
            .unwrap(),
        1
    );
    let workspace = parent.store.workspace_id.clone();
    drop(parent);
    fs::write(child.path().join("main.rs"), "fn public() {}\n").unwrap();
    let mut target = Engine::open(child.path()).unwrap();
    let baseline = unsafe { rusqlite::ffi::sqlite3_memory_used() };
    unsafe {
        rusqlite::ffi::sqlite3_memory_highwater(1);
    }
    let result =
        git_workflows::seed_worktree_cache(&mut target, &host, root.path(), &workspace, &deadline);
    let peak = unsafe { rusqlite::ffi::sqlite3_memory_highwater(0) };
    assert!(result.is_err());
    assert!(
        peak - baseline < 16_000_000,
        "Parent cache length admission allocated {} SQLite bytes before rejecting a64MB value",
        peak - baseline
    );
}

#[test]
fn ignored_parent_directories_cannot_be_reopened_by_their_unvisited_nested_policy() {
    let _serial = REVIEW_SERIAL
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let root = tempfile::tempdir().unwrap();
    let host = HostGit::discover().unwrap();
    let git = |arguments: &[&str]| {
        let output = Command::new(&host.executable)
            .current_dir(root.path())
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
            .args(arguments)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    };
    git(&["init", "--quiet"]);
    fs::create_dir_all(root.path().join("ignored")).unwrap();
    fs::write(root.path().join(".gitignore"), "ignored/\n").unwrap();
    fs::write(root.path().join("main.rs"), "fn public() {}\n").unwrap();
    fs::write(root.path().join("ignored/.gitignore"), "!keep.rs\n").unwrap();
    fs::write(
        root.path().join("ignored/keep.rs"),
        "fn ignored_private() {}\n",
    )
    .unwrap();
    git(&["add", ".gitignore", "main.rs"]);
    git(&["add", "--force", "ignored/.gitignore", "ignored/keep.rs"]);
    git(&["commit", "--quiet", "-m", "source"]);
    let mut engine = Engine::open(root.path()).unwrap();
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
        !base
            .files
            .iter()
            .any(|file| file.path.starts_with("ignored/")),
        "Nested ignore policy reopened a directory pruned by its parent"
    );
}

#[test]
fn historical_author_metadata_cannot_publish_obvious_secret_values() {
    let _serial = REVIEW_SERIAL
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let root = tempfile::tempdir().unwrap();
    let host = HostGit::discover().unwrap();
    let git = |arguments: &[&str]| {
        let output = Command::new(&host.executable)
            .current_dir(root.path())
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
            .args(arguments)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    };
    git(&["init", "--quiet"]);
    fs::write(root.path().join("main.rs"), "fn first() {}\n").unwrap();
    git(&["add", "main.rs"]);
    let token = format!("ghp_{}", "B".repeat(36));
    let author = format!("user.name={token}");
    git(&["-c", &author, "commit", "--quiet", "-m", "old metadata"]);
    fs::write(root.path().join("main.rs"), "fn latest() {}\n").unwrap();
    git(&["add", "main.rs"]);
    git(&["commit", "--quiet", "-m", "latest clean metadata"]);
    let mut engine = Engine::open(root.path()).unwrap();
    let result = git_workflows::analyze(
        &mut engine,
        &host,
        &WorkflowOptions::default(),
        &Deadline::new(30000),
    );
    if let Ok(report) = result {
        assert!(
            !report.to_string().contains(&token),
            "Historical author secret leaked into the published workflow report"
        );
    }
}

#[test]
fn branch_overlap_includes_dependencies_authorized_by_the_captured_readonly_scope() {
    let _serial = REVIEW_SERIAL
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let root = tempfile::tempdir().unwrap();
    let host = HostGit::discover().unwrap();
    let git = |arguments: &[&str]| {
        let output = Command::new(&host.executable)
            .current_dir(root.path())
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
            .args(arguments)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    };
    git(&["init", "--quiet", "-b", "main"]);
    fs::create_dir_all(root.path().join("node_modules/pkg")).unwrap();
    let path = "node_modules/pkg/index.js";
    fs::write(root.path().join(path), "export const dep = 1;\n").unwrap();
    git(&["add", path]);
    git(&["commit", "--quiet", "-m", "source"]);
    git(&["checkout", "--quiet", "-b", "other"]);
    fs::write(root.path().join(path), "export const dep = 2;\n").unwrap();
    git(&["commit", "--quiet", "-am", "branch"]);
    git(&["checkout", "--quiet", "main"]);
    fs::write(root.path().join(path), "export const dep = 3;\n").unwrap();
    let mut engine = Engine::open(root.path()).unwrap();
    engine.capture_options =
        serde_json::from_value(serde_json::json!({"readOnlyDependencyDirs":["node_modules"]}))
            .unwrap();
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
        .find(|branch| branch["ref"] == "refs/heads/other")
        .unwrap();
    assert!(
        branch["overlappingPaths"]
            .as_array()
            .unwrap()
            .iter()
            .any(|value| value == path),
        "Scoped dependency branch overlap was lost: {branch}"
    );
}

#[test]
fn valid_committed_source_batches_are_partitioned_by_bytes_within_the_total_budget() {
    let _serial = REVIEW_SERIAL
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let root = tempfile::tempdir().unwrap();
    let host = HostGit::discover().unwrap();
    let git = |arguments: &[&str]| {
        let output = Command::new(&host.executable)
            .current_dir(root.path())
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
            .args(arguments)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    };
    git(&["init", "--quiet"]);
    let source = " ".repeat(2_100_000);
    for index in 0..16 {
        fs::write(root.path().join(format!("source{index:02}.rs")), &source).unwrap();
    }
    git(&["add", "."]);
    git(&["commit", "--quiet", "-m", "bounded source"]);
    let mut engine = Engine::open(root.path()).unwrap();
    let result = git_workflows::analyze(
        &mut engine,
        &host,
        &WorkflowOptions::default(),
        &Deadline::new(120000),
    );
    assert!(
        result.is_ok(),
        "Valid33.6MB committed source was rejected rather than partitioned into bounded batches: {result:?}"
    );
}
