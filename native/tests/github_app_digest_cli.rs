//! Actual local CLI admission and existing-store preservation; no OS credential setup.
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    process::{Command, Output},
};
use yoke::{Engine, capture::Deadline};

#[path = "support/github_app_digest_history_cli.rs"]
mod captured_history;

fn command(root: &Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_yoke"));
    command.arg("--root").arg(root);
    command
}
fn marker(output: Output, expected: &str) {
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    let error = String::from_utf8(output.stderr).unwrap();
    assert!(error.contains(expected), "{error}");
    assert!(!error.contains("INPUT_BODY_CANARY"));
}
fn profile() -> Value {
    json!({"schemaVersion":1,"appId":123,"secretCredential":"digest-cli-key-must-not-be-read",
        "grants":[{"installationId":42,"repositoryId":9,"repository":"acme/demo"}]})
}
fn request(id: &str) -> Value {
    json!({"schemaVersion":1,"snapshotId":id,"installationId":42,"repositoryId":9,"repository":"acme/demo"})
}
fn write(path: &Path, value: &Value) {
    std::fs::write(path, serde_json::to_vec(value).unwrap()).unwrap();
}
fn preview(root: &Path, profile: &Path, request: &Path) -> Command {
    let mut command = command(root);
    command
        .args([
            "github-app",
            "digest",
            "--allow-digest",
            "preview",
            "--profile",
        ])
        .arg(profile)
        .arg("--request")
        .arg(request);
    command
}
fn files(root: &Path) -> BTreeMap<PathBuf, String> {
    fn visit(root: &Path, directory: &Path, result: &mut BTreeMap<PathBuf, String>) {
        for entry in std::fs::read_dir(directory).unwrap() {
            let entry = entry.unwrap();
            if entry.file_type().unwrap().is_dir() {
                visit(root, &entry.path(), result);
            } else {
                assert!(entry.file_type().unwrap().is_file());
                result.insert(
                    entry.path().strip_prefix(root).unwrap().to_owned(),
                    yoke::model::hash(std::fs::read(entry.path()).unwrap()),
                );
            }
        }
    }
    let mut result = BTreeMap::new();
    visit(root, root, &mut result);
    result
}

#[test]
fn digest_read_consent_precedes_inputs_credentials_and_workspace_creation() {
    let folder = tempfile::tempdir().unwrap();
    let root = folder.path().join("absent-source");
    marker(
        command(&root)
            .args(["github-app", "digest", "preview"])
            .output()
            .unwrap(),
        "github_app_digest_permission_required",
    );
    assert!(!root.exists());
    assert_eq!(std::fs::read_dir(folder.path()).unwrap().count(), 0);
}

#[test]
fn digest_refuses_network_webhook_and_write_authority_before_host_inputs() {
    let folder = tempfile::tempdir().unwrap();
    let root = folder.path().join("absent-source");
    for flag in ["--allow-writes", "--allow-github", "--allow-webhooks"] {
        let mut command = command(&root);
        if flag == "--allow-writes" {
            command.arg(flag);
        }
        command.arg("github-app");
        if flag != "--allow-writes" {
            command.arg(flag);
        }
        marker(
            command
                .args(["digest", "--allow-digest", "preview"])
                .output()
                .unwrap(),
            "github_app_digest_input_invalid",
        );
        assert!(!root.exists());
    }
    assert_eq!(std::fs::read_dir(folder.path()).unwrap().count(), 0);
}

#[test]
fn digest_profile_grant_is_checked_before_opening_an_absent_workspace() {
    let folder = tempfile::tempdir().unwrap();
    let root = folder.path().join("absent-source");
    let profile_path = folder.path().join("profile.json");
    let request_path = folder.path().join("request.json");
    write(&profile_path, &profile());
    let mut wrong = request(&"a".repeat(64));
    wrong["repositoryId"] = json!(10);
    write(&request_path, &wrong);
    marker(
        preview(&root, &profile_path, &request_path)
            .output()
            .unwrap(),
        "github_app_digest_grant_denied",
    );
    assert!(!root.exists());
    assert_eq!(std::fs::read_dir(folder.path()).unwrap().count(), 2);
}

#[test]
fn digest_cli_refuses_source_bearing_and_duplicate_request_fields_without_echo() {
    let folder = tempfile::tempdir().unwrap();
    let root = folder.path().join("absent-source");
    let profile_path = folder.path().join("profile.json");
    let request_path = folder.path().join("request.json");
    write(&profile_path, &profile());
    let mut bad = request(&"a".repeat(64));
    bad["body"] = json!("INPUT_BODY_CANARY");
    write(&request_path, &bad);
    marker(
        preview(&root, &profile_path, &request_path)
            .output()
            .unwrap(),
        "github_app_digest_input_invalid",
    );
    let raw = serde_json::to_string(&request(&"a".repeat(64))).unwrap();
    std::fs::write(&request_path, raw.replacen('{', "{\"schemaVersion\":1,", 1)).unwrap();
    marker(
        preview(&root, &profile_path, &request_path)
            .output()
            .unwrap(),
        "github_app_digest_input_invalid",
    );
    assert!(!root.exists());
}

#[test]
fn local_digest_cli_needs_no_signing_key_and_preserves_an_existing_delete_mode_store() {
    let folder = tempfile::tempdir().unwrap();
    let root = folder.path().join("source");
    std::fs::create_dir(&root).unwrap();
    std::fs::write(
        root.join("café.rs"),
        "pub fn café() { let _private = \"CAPTURED_BODY_CANARY\"; }\n",
    )
    .unwrap();
    let mut engine = Engine::open(&root).unwrap();
    let snapshot = engine.index(&Deadline::new(30_000)).unwrap().0;
    drop(engine);
    let db_path = root.join(".yoke/index.sqlite");
    let db = rusqlite::Connection::open(&db_path).unwrap();
    let mode: String = db
        .query_row("PRAGMA journal_mode=DELETE", [], |row| row.get(0))
        .unwrap();
    assert_eq!(mode, "delete");
    drop(db);
    std::fs::write(root.join("café.rs"), "pub fn changed_after_capture() {}\n").unwrap();
    let profile_path = folder.path().join("profile.json");
    let request_path = folder.path().join("request.json");
    write(&profile_path, &profile());
    write(&request_path, &request(&snapshot.id));
    let before = files(&root);
    let output = preview(&root, &profile_path, &request_path)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stderr.is_empty());
    assert_eq!(output.stdout.last(), Some(&b'\n'));
    let value: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["snapshotId"], snapshot.id);
    assert_eq!(value["sourceCommitVerified"], false);
    assert_eq!(value["networkRequests"], 0);
    assert_eq!(value["modelCalls"], 0);
    assert_eq!(value["complete"], false);
    assert!(
        value["symbols"]
            .as_array()
            .unwrap()
            .iter()
            .any(|symbol| symbol["name"] == "café")
    );
    let raw = String::from_utf8(output.stdout.clone()).unwrap();
    assert!(!raw.contains("CAPTURED_BODY_CANARY"));
    assert!(!raw.contains("changed_after_capture"));
    assert_eq!(files(&root), before);
    let exact = preview(&root, &profile_path, &request_path)
        .arg("--max-bytes")
        .arg(output.stdout.len().to_string())
        .output()
        .unwrap();
    assert!(
        exact.status.success(),
        "{}",
        String::from_utf8_lossy(&exact.stderr)
    );
    assert_eq!(exact.stdout, output.stdout);
    marker(
        preview(&root, &profile_path, &request_path)
            .arg("--max-bytes")
            .arg((output.stdout.len() - 1).to_string())
            .output()
            .unwrap(),
        "github_app_digest_capacity_exceeded",
    );
    assert_eq!(files(&root), before);
    let db =
        rusqlite::Connection::open_with_flags(&db_path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
            .unwrap();
    assert_eq!(
        db.query_row("PRAGMA journal_mode", [], |row| row.get::<_, String>(0))
            .unwrap(),
        "delete"
    );
}

#[test]
fn locked_existing_store_obeys_the_cli_deadline_before_native_projection() {
    let folder = tempfile::tempdir().unwrap();
    let root = folder.path().join("source");
    std::fs::create_dir(&root).unwrap();
    std::fs::write(root.join("api.rs"), "pub fn api() {}\n").unwrap();
    let mut engine = Engine::open(&root).unwrap();
    let id = engine.index(&Deadline::new(30_000)).unwrap().0.id;
    drop(engine);
    let db = rusqlite::Connection::open(root.join(".yoke/index.sqlite")).unwrap();
    db.execute_batch("PRAGMA journal_mode=DELETE").unwrap();
    let profile_path = folder.path().join("profile.json");
    let request_path = folder.path().join("request.json");
    write(&profile_path, &profile());
    write(&request_path, &request(&id));
    let start = std::time::Instant::now();
    let baseline = preview(&root, &profile_path, &request_path)
        .output()
        .unwrap();
    let baseline_time = start.elapsed();
    assert!(
        baseline.status.success(),
        "{}",
        String::from_utf8_lossy(&baseline.stderr)
    );
    db.execute_batch("BEGIN EXCLUSIVE").unwrap();
    let start = std::time::Instant::now();
    let output = preview(&root, &profile_path, &request_path)
        .args(["--timeout", "25"])
        .output()
        .unwrap();
    let elapsed = start.elapsed();
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .to_ascii_lowercase()
            .contains("deadline"),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        elapsed < baseline_time + std::time::Duration::from_millis(150),
        "Locked existing store took {elapsed:?}, baseline {baseline_time:?}"
    );
    db.execute_batch("ROLLBACK").unwrap();
}

#[test]
fn local_digest_requires_absolute_host_input_paths_before_workspace_discovery() {
    let folder = tempfile::tempdir().unwrap();
    let root = folder.path().join("absent-source");
    let profile_path = folder.path().join("profile.json");
    let request_path = folder.path().join("request.json");
    write(&profile_path, &profile());
    write(&request_path, &request(&"a".repeat(64)));
    for (profile_arg, request_arg) in [
        (Path::new("profile.json"), request_path.as_path()),
        (profile_path.as_path(), Path::new("request.json")),
    ] {
        marker(
            preview(&root, profile_arg, request_arg)
                .current_dir(folder.path())
                .output()
                .unwrap(),
            "github_app_digest_input_invalid",
        );
        assert!(!root.exists());
    }
}

#[test]
fn digest_existing_store_access_refuses_missing_lease_files_without_recreating_them() {
    let folder = tempfile::tempdir().unwrap();
    let root = folder.path().join("source");
    std::fs::create_dir(&root).unwrap();
    std::fs::write(root.join("api.rs"), "pub fn api() {}\n").unwrap();
    let mut engine = Engine::open(&root).unwrap();
    let id = engine.index(&Deadline::new(30_000)).unwrap().0.id;
    drop(engine);
    for name in ["clients.lock", "maintenance.lock"] {
        std::fs::remove_file(root.join(".yoke").join(name)).unwrap();
    }
    let profile_path = folder.path().join("profile.json");
    let request_path = folder.path().join("request.json");
    write(&profile_path, &profile());
    write(&request_path, &request(&id));
    let before = files(&root);
    marker(
        preview(&root, &profile_path, &request_path)
            .output()
            .unwrap(),
        "github_app_digest_store_unavailable",
    );
    assert_eq!(files(&root), before);
}

#[test]
fn local_digest_reads_the_committed_live_wal_without_checkpointing_or_ignoring_it() {
    let folder = tempfile::tempdir().unwrap();
    let root = folder.path().join("source");
    std::fs::create_dir(&root).unwrap();
    std::fs::write(root.join("api.rs"), "pub fn newest_wal_api() {}\n").unwrap();
    let mut engine = Engine::open(&root).unwrap();
    let id = engine.index(&Deadline::new(30_000)).unwrap().0.id;
    assert!(
        std::fs::metadata(root.join(".yoke/index.sqlite-wal"))
            .unwrap()
            .len()
            > 0
    );
    let profile_path = folder.path().join("profile.json");
    let request_path = folder.path().join("request.json");
    write(&profile_path, &profile());
    write(&request_path, &request(&id));
    // SHM read marks are SQLite coordination, not application publication.
    let state = || {
        files(&root)
            .into_iter()
            .filter(|(path, _)| path.file_name() != Some(std::ffi::OsStr::new("index.sqlite-shm")))
            .collect::<BTreeMap<_, _>>()
    };
    let before = state();
    let output = preview(&root, &profile_path, &request_path)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let value: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["snapshotId"], id);
    assert!(
        value["symbols"]
            .as_array()
            .unwrap()
            .iter()
            .any(|symbol| symbol["name"] == "newest_wal_api")
    );
    assert_eq!(state(), before);
    assert_eq!(engine.store.snapshot(&id).unwrap().id, id);
}

fn git(host: &yoke::git_workflows::HostGit, root: &Path, args: &[&str]) -> String {
    let output = Command::new(&host.executable)
        .current_dir(root)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env(
            "GIT_CONFIG_GLOBAL",
            if cfg!(windows) { "NUL" } else { "/dev/null" },
        )
        .env(
            "GIT_CONFIG_SYSTEM",
            if cfg!(windows) { "NUL" } else { "/dev/null" },
        )
        .args([
            "-c",
            "init.templateDir=",
            "-c",
            "core.autocrlf=false",
            "-c",
            "commit.gpgsign=false",
            "-c",
            "user.name=Digest CLI Fixture",
            "-c",
            "user.email=fixture@example.invalid",
        ])
        .args(args)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap().trim().to_owned()
}

#[test]
fn git_digest_cli_uses_verified_captured_objects_after_git_metadata_and_worktree_change() {
    let folder = tempfile::tempdir().unwrap();
    let root = folder.path().join("source");
    std::fs::create_dir(&root).unwrap();
    let host = yoke::git_workflows::HostGit::discover().unwrap();
    git(&host, &root, &["init", "-q"]);
    std::fs::write(
        root.join("café.rs"),
        "pub fn café_git_api() { let _body = \"GIT_BODY_CANARY\"; }\n",
    )
    .unwrap();
    git(&host, &root, &["add", "--", "café.rs"]);
    git(&host, &root, &["commit", "-q", "-m", "owned CLI snapshot"]);
    let commit = git(&host, &root, &["rev-parse", "HEAD"]);
    let tree = git(&host, &root, &["rev-parse", "HEAD^{tree}"]);
    let d = Deadline::new(30_000);
    let mut engine = Engine::open(&root).unwrap();
    let origin = engine.index(&d).unwrap().0;
    let id = yoke::git_workflows::capture_commit_at(
        yoke::analysis::AnalyzerEpoch::V8,
        &mut engine,
        &host,
        &origin,
        &commit,
        &d,
    )
    .unwrap()
    .0
    .id;
    drop(engine);
    let db = rusqlite::Connection::open(root.join(".yoke/index.sqlite")).unwrap();
    db.execute_batch("PRAGMA journal_mode=DELETE").unwrap();
    drop(db);
    std::fs::rename(root.join(".git"), root.join("unavailable_git_metadata")).unwrap();
    std::fs::write(
        root.join("café.rs"),
        "pub fn current_checkout_canary() {}\n",
    )
    .unwrap();
    let profile_path = folder.path().join("profile.json");
    let request_path = folder.path().join("request.json");
    write(&profile_path, &profile());
    write(&request_path, &request(&id));
    let before = files(&root);
    let output = preview(&root, &profile_path, &request_path)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let value: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["sourceCommitVerified"], true);
    assert_eq!(value["sourceCommitId"], commit);
    assert_eq!(value["sourceTreeId"], tree);
    assert_eq!(value["providerVerified"], false);
    assert_eq!(value["networkRequests"], 0);
    assert_eq!(value["complete"], false);
    assert!(
        value["symbols"]
            .as_array()
            .unwrap()
            .iter()
            .any(|symbol| symbol["name"] == "café_git_api")
    );
    let raw = String::from_utf8(output.stdout).unwrap();
    assert!(!raw.contains("GIT_BODY_CANARY"));
    assert!(!raw.contains("current_checkout_canary"));
    assert_eq!(files(&root), before);
}

#[test]
fn digest_cli_hard_budgets_are_checked_before_missing_inputs_or_store() {
    let folder = tempfile::tempdir().unwrap();
    let root = folder.path().join("absent-source");
    for (flag, value) in [
        ("--max-symbols", "4001"),
        ("--max-source-bytes", "32000001"),
        ("--max-metadata-bytes", "64000001"),
        ("--max-lineage-metadata-bytes", "16000001"),
        ("--max-bytes", "4000001"),
        ("--max-bytes", "0"),
    ] {
        marker(
            command(&root)
                .args([
                    "github-app",
                    "digest",
                    "--allow-digest",
                    flag,
                    value,
                    "preview",
                ])
                .output()
                .unwrap(),
            "github_app_digest_capacity_exceeded",
        );
        assert!(!root.exists());
    }
    assert_eq!(std::fs::read_dir(folder.path()).unwrap().count(), 0);
}
