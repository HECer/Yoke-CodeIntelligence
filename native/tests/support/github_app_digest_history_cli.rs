//! Actual bounded commit-history admission over owned local Git repositories.
use super::{command, files, git, marker as shared_marker, profile, request, write};
use serde_json::{Value, json};
use std::{
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Output, Stdio},
};
use yoke::{Engine, capture::Deadline, git_workflows::HostGit};

#[path = "github_app_digest_history_dates_cli.rs"]
mod committer_date_order;

struct Fixture {
    _folder: tempfile::TempDir,
    root: PathBuf,
    host: HostGit,
    profile: PathBuf,
    request: PathBuf,
    head: String,
    ancestor: String,
}
fn object_path(root: &Path, id: &str) -> PathBuf {
    root.join(".git/objects").join(&id[..2]).join(&id[2..])
}
fn marker(output: Output, expected: &str) {
    let wire = String::from_utf8_lossy(&output.stderr);
    let static_marker = format!("yoke: {expected}");
    let static_json_capacity = format!("{static_marker}: JSON output exceeds byte capacity");
    assert!(
        wire.trim() == static_marker
            || (expected == "github_app_digest_capacity_exceeded"
                && wire.trim() == static_json_capacity),
        "{wire}"
    );
    shared_marker(output, expected);
}

#[test]
fn asymmetric_parent_walk_matches_git_date_priority_and_reverse_output() {
    let mut truncated_expected = Vec::new();
    let f = fixture(|host, root, tree, ancestor| {
        let left_old = commit(host, root, tree, &[ancestor], "left old");
        let left_new = commit(host, root, tree, &[&left_old], "left new");
        let right = commit(host, root, tree, &[ancestor], "right");
        truncated_expected = vec![right.clone(), left_new.clone()];
        commit(host, root, tree, &[&left_new, &right], "selected merge")
    });
    // The root uses the real fixture creation date, while synthetic descendants
    // use epoch 2. The independent Git walk must decide the resulting skew.
    let oracle = git(
        &f.host,
        &f.root,
        &["log", "--no-merges", "--reverse", "--format=%H", &f.head],
    );
    let expected: Vec<_> = oracle.lines().collect();
    let (full, _) = value(&mut history(&f));
    assert_eq!(
        full["commits"]
            .as_array()
            .unwrap()
            .iter()
            .map(|row| row["sha"].as_str().unwrap())
            .collect::<Vec<_>>(),
        expected
    );
    let (limited, _) = value(history(&f).arg("--max-commits").arg("2"));
    assert_eq!(
        limited["commits"]
            .as_array()
            .unwrap()
            .iter()
            .map(|row| row["sha"].as_str().unwrap())
            .collect::<Vec<_>>(),
        truncated_expected
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>()
    );
    assert_eq!(limited["history"]["frontierCommits"], 2);
    assert_eq!(limited["history"]["visitedCommits"], 3);
}
fn commit_bytes(host: &HostGit, root: &Path, bytes: &[u8]) -> String {
    let mut child = Command::new(&host.executable)
        .current_dir(root)
        .args([
            "hash-object",
            "--literally",
            "-t",
            "commit",
            "-w",
            "--stdin",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(bytes).unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success(), "Owned commit-object setup failed");
    String::from_utf8(output.stdout).unwrap().trim().to_owned()
}
fn commit(host: &HostGit, root: &Path, tree: &str, parents: &[&str], message: &str) -> String {
    let parents = parents
        .iter()
        .map(|id| format!("parent {id}\n"))
        .collect::<String>();
    let bytes = format!(
        "tree {tree}\n{parents}author Fixture <fixture@example.invalid> 2 +0000\ncommitter Fixture <fixture@example.invalid> 2 +0000\n\n{message}\n"
    );
    commit_bytes(host, root, bytes.as_bytes())
}
fn fixture(build: impl FnOnce(&HostGit, &Path, &str, &str) -> String) -> Fixture {
    let folder = tempfile::tempdir().unwrap();
    let root = folder.path().join("source");
    std::fs::create_dir(&root).unwrap();
    let host = HostGit::discover().unwrap();
    git(&host, &root, &["init", "-q"]);
    std::fs::write(
        root.join("café.rs"),
        "pub fn café_history_api() { let _body = \"SOURCE_BODY_CANARY\"; }\n",
    )
    .unwrap();
    git(&host, &root, &["add", "--", "café.rs"]);
    git(
        &host,
        &root,
        &[
            "commit",
            "-q",
            "-m",
            "root decision",
            "-m",
            "root body \u{1} field \u{2}\nsecond line",
        ],
    );
    let ancestor = git(&host, &root, &["rev-parse", "HEAD"]);
    let tree = git(&host, &root, &["rev-parse", "HEAD^{tree}"]);
    let head = build(&host, &root, &tree, &ancestor);
    let d = Deadline::new(30_000);
    let mut engine = Engine::open(&root).unwrap();
    let origin = engine.index(&d).unwrap().0;
    let id = yoke::git_workflows::capture_commit_at(
        yoke::analysis::AnalyzerEpoch::V8,
        &mut engine,
        &host,
        &origin,
        &head,
        &d,
    )
    .unwrap()
    .0
    .id;
    drop(engine);
    let db = rusqlite::Connection::open(root.join(".yoke/index.sqlite")).unwrap();
    db.execute_batch("PRAGMA journal_mode=DELETE").unwrap();
    drop(db);
    let profile_path = folder.path().join("profile.json");
    let request_path = folder.path().join("request.json");
    write(&profile_path, &profile());
    write(&request_path, &request(&id));
    Fixture {
        _folder: folder,
        root,
        host,
        profile: profile_path,
        request: request_path,
        head,
        ancestor,
    }
}
fn chain() -> Fixture {
    fixture(|host, root, tree, ancestor| {
        commit(
            host,
            root,
            tree,
            &[ancestor],
            "captured decision\n\nHISTORY_TEXT_CANARY\nUnicode café \u{1} and \u{2}",
        )
    })
}
fn history(f: &Fixture) -> Command {
    let mut result = command(&f.root);
    result
        .args([
            "github-app",
            "digest",
            "--allow-digest",
            "history",
            "--allow-history",
            "--trust-git",
            "--profile",
        ])
        .arg(&f.profile)
        .arg("--request")
        .arg(&f.request)
        .arg("--git")
        .arg(&f.host.executable);
    result
}
fn value(command: &mut Command) -> (Value, Vec<u8>) {
    let output = command.output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stdout.ends_with(b"\n"));
    (
        serde_json::from_slice(&output.stdout).unwrap(),
        output.stdout,
    )
}

#[test]
fn history_consent_and_git_trust_precede_host_inputs_and_store_creation() {
    let folder = tempfile::tempdir().unwrap();
    let root = folder.path().join("absent-source");
    for (flags, expected) in [
        (
            vec!["--trust-git"],
            "github_app_digest_history_permission_required",
        ),
        (
            vec!["--allow-history"],
            "github_app_digest_history_trust_required",
        ),
    ] {
        marker(
            command(&root)
                .args(["github-app", "digest", "--allow-digest", "history"])
                .args(flags)
                .output()
                .unwrap(),
            expected,
        );
        assert!(!root.exists());
        assert_eq!(std::fs::read_dir(folder.path()).unwrap().count(), 0);
    }
}

#[test]
fn history_refuses_network_webhook_and_write_authority_before_host_inputs() {
    let folder = tempfile::tempdir().unwrap();
    let root = folder.path().join("absent-source");
    for flag in ["--allow-writes", "--allow-github", "--allow-webhooks"] {
        let mut call = command(&root);
        if flag == "--allow-writes" {
            call.arg(flag);
        }
        call.arg("github-app");
        if flag != "--allow-writes" {
            call.arg(flag);
        }
        marker(
            call.args([
                "digest",
                "--allow-digest",
                "history",
                "--allow-history",
                "--trust-git",
            ])
            .output()
            .unwrap(),
            "github_app_digest_input_invalid",
        );
        assert!(!root.exists());
    }
}

#[test]
fn history_hard_caps_precede_missing_host_inputs() {
    let folder = tempfile::tempdir().unwrap();
    let root = folder.path().join("absent-source");
    for (flag, cap) in [
        ("--max-commits", "1001"),
        ("--max-visits", "4001"),
        ("--max-frontier", "4001"),
        ("--max-parent-ids", "16001"),
        ("--max-commit-bytes", "1000001"),
        ("--max-history-bytes", "64000001"),
    ] {
        marker(
            command(&root)
                .args([
                    "github-app",
                    "digest",
                    "--allow-digest",
                    "history",
                    "--allow-history",
                    "--trust-git",
                    flag,
                    cap,
                ])
                .output()
                .unwrap(),
            "github_app_digest_capacity_exceeded",
        );
        assert!(!root.exists());
    }
}

#[test]
fn history_grant_denial_precedes_absent_store_and_git_execution() {
    let f = chain();
    let absent = f.root.join("absent-workspace");
    let mut wrong: Value = serde_json::from_slice(&std::fs::read(&f.request).unwrap()).unwrap();
    wrong["repositoryId"] = json!(10);
    write(&f.request, &wrong);
    marker(
        command(&absent)
            .args([
                "github-app",
                "digest",
                "--allow-digest",
                "history",
                "--allow-history",
                "--trust-git",
                "--profile",
            ])
            .arg(&f.profile)
            .arg("--request")
            .arg(&f.request)
            .arg("--git")
            .arg(f.root.join("absent-git.exe"))
            .output()
            .unwrap(),
        "github_app_digest_grant_denied",
    );
    assert!(!absent.exists());
}

#[test]
fn disk_source_cannot_masquerade_as_an_empty_complete_git_history() {
    let f = chain();
    let mut engine = Engine::open(&f.root).unwrap();
    let disk = engine.index(&Deadline::new(30_000)).unwrap().0.id;
    drop(engine);
    let db = rusqlite::Connection::open(f.root.join(".yoke/index.sqlite")).unwrap();
    db.execute_batch("PRAGMA journal_mode=DELETE").unwrap();
    drop(db);
    write(&f.request, &request(&disk));
    let before = files(&f.root);
    marker(
        history(&f).output().unwrap(),
        "github_app_digest_history_unavailable",
    );
    assert_eq!(files(&f.root), before);
}

#[test]
fn history_uses_the_verified_captured_head_and_preserves_message_delimiters_and_output_boundary() {
    let f = chain();
    std::fs::write(
        f.root.join("café.rs"),
        "pub fn CURRENT_CHECKOUT_CANARY() {}\n",
    )
    .unwrap();
    git(&f.host, &f.root, &["add", "--", "café.rs"]);
    git(
        &f.host,
        &f.root,
        &["commit", "-q", "-m", "LIVE_CURRENT_HEAD_CANARY"],
    );
    let before = files(&f.root);
    let (result, raw) = value(&mut history(&f));
    assert_eq!(result["sourceCommitId"], f.head);
    assert_eq!(result["providerVerified"], false);
    assert_eq!(result["compilerVerified"], false);
    assert_eq!(result["complete"], false);
    assert_eq!(result["networkRequests"], 0);
    assert_eq!(result["modelCalls"], 0);
    assert_eq!(
        result["history"]["selectionOrder"],
        "commit_date_priority_parent_insertion_order"
    );
    assert_eq!(result["history"]["outputOrder"], "reverse_selection_order");
    assert_eq!(result["history"]["exhausted"], true);
    assert_eq!(result["history"]["visitedCommits"], 2);
    let commits = result["commits"].as_array().unwrap();
    assert_eq!(commits.len(), 2);
    assert_eq!(commits[0]["sha"], f.ancestor);
    assert_eq!(commits[0]["subject"], "root decision");
    assert_eq!(
        commits[0]["body"],
        "root body \u{1} field \u{2}\nsecond line"
    );
    assert_eq!(commits[1]["sha"], f.head);
    assert_eq!(
        commits[1]["body"],
        "HISTORY_TEXT_CANARY\nUnicode café \u{1} and \u{2}"
    );
    for commit in commits {
        assert_eq!(commit["filesStatus"], "unavailable");
        assert_eq!(commit["files"], json!([]));
        assert_eq!(commit["symbolsStatus"], "unavailable");
        assert_eq!(commit["symbols"], json!([]));
    }
    let text = String::from_utf8(raw.clone()).unwrap();
    for forbidden in [
        "SOURCE_BODY_CANARY",
        "CURRENT_CHECKOUT_CANARY",
        "LIVE_CURRENT_HEAD_CANARY",
    ] {
        assert!(!text.contains(forbidden));
    }
    let (_, exact) = value(history(&f).arg("--max-bytes").arg(raw.len().to_string()));
    assert_eq!(exact, raw);
    marker(
        history(&f)
            .arg("--max-bytes")
            .arg((raw.len() - 1).to_string())
            .output()
            .unwrap(),
        "github_app_digest_capacity_exceeded",
    );
    assert_eq!(files(&f.root), before);
}

#[test]
fn history_selection_and_visit_limits_report_the_unvisited_frontier() {
    let f = chain();
    for (flag, reason) in [
        ("--max-commits", "commit_limit"),
        ("--max-visits", "visit_limit"),
    ] {
        let (result, _) = value(history(&f).arg(flag).arg("1"));
        assert_eq!(result["commits"].as_array().unwrap().len(), 1);
        assert_eq!(result["commits"][0]["sha"], f.head);
        assert_eq!(result["history"]["visitedCommits"], 1);
        assert_eq!(result["history"]["frontierCommits"], 1);
        assert_eq!(result["history"]["exhausted"], false);
        assert_eq!(result["history"]["truncationReason"], reason);
        assert_eq!(result["complete"], false);
    }
}

#[test]
fn skipped_merge_objects_still_consume_visit_and_frontier_budgets() {
    let f = fixture(|host, root, tree, ancestor| {
        let left = commit(host, root, tree, &[ancestor], "left decision");
        let right = commit(host, root, tree, &[ancestor], "right decision");
        commit(host, root, tree, &[&left, &right], "merge decision")
    });
    let oracle = git(
        &f.host,
        &f.root,
        &["log", "--no-merges", "--reverse", "--format=%H", &f.head],
    );
    let expected: Vec<_> = oracle.lines().collect();
    let (result, _) = value(&mut history(&f));
    assert_eq!(result["history"]["visitedCommits"], 4);
    assert_eq!(result["history"]["mergeCommitsSkipped"], 1);
    assert_eq!(result["history"]["exhausted"], true);
    let commits = result["commits"].as_array().unwrap();
    assert_eq!(commits.len(), 3);
    assert!(commits.iter().all(|row| row["sha"] != f.head));
    assert!(commits.iter().any(|commit| commit["sha"] == f.ancestor));
    assert_eq!(
        commits
            .iter()
            .map(|row| row["sha"].as_str().unwrap())
            .collect::<Vec<_>>(),
        expected
    );
    let (truncated, _) = value(history(&f).arg("--max-commits").arg("2"));
    assert_eq!(
        truncated["commits"]
            .as_array()
            .unwrap()
            .iter()
            .map(|row| row["sha"].as_str().unwrap())
            .collect::<Vec<_>>(),
        expected[1..]
    );
    let (bounded, _) = value(history(&f).arg("--max-visits").arg("1"));
    assert_eq!(bounded["commits"], json!([]));
    assert_eq!(bounded["history"]["visitedCommits"], 1);
    assert_eq!(bounded["history"]["mergeCommitsSkipped"], 1);
    assert_eq!(bounded["history"]["frontierCommits"], 2);
    assert_eq!(bounded["history"]["truncationReason"], "visit_limit");
    marker(
        history(&f).arg("--max-frontier").arg("1").output().unwrap(),
        "github_app_digest_capacity_exceeded",
    );
    for maximum in [1, 3] {
        // Four parent references occur across the merge and both branches;
        // the cumulative limit also counts a shared ancestor more than once.
        marker(
            history(&f)
                .arg("--max-parent-ids")
                .arg(maximum.to_string())
                .output()
                .unwrap(),
            "github_app_digest_capacity_exceeded",
        );
    }
}

#[test]
fn frontier_budget_counts_the_queue_across_multiple_visited_commits() {
    let f = fixture(|host, root, tree, ancestor| {
        let c = commit(host, root, tree, &[ancestor], "c");
        let d = commit(host, root, tree, &[ancestor], "d");
        let e = commit(host, root, tree, &[ancestor], "e");
        let a = commit(host, root, tree, &[&c, &d], "a merge");
        let b = commit(host, root, tree, &[&e], "b");
        commit(host, root, tree, &[&a, &b], "selected merge")
    });
    // Every individual parent list fits two, but the queue grows to three.
    marker(
        history(&f).arg("--max-frontier").arg("2").output().unwrap(),
        "github_app_digest_capacity_exceeded",
    );
}

#[test]
fn per_commit_size_has_an_exact_lowered_runtime_boundary() {
    let f = chain();
    let maximum = [&f.head, &f.ancestor]
        .into_iter()
        .map(|id| {
            git(&f.host, &f.root, &["cat-file", "-s", id])
                .parse::<usize>()
                .unwrap()
        })
        .max()
        .unwrap();
    let (exact, _) = value(
        history(&f)
            .arg("--max-commit-bytes")
            .arg(maximum.to_string()),
    );
    assert_eq!(exact["history"]["visitedCommits"], 2);
    marker(
        history(&f)
            .arg("--max-commit-bytes")
            .arg((maximum - 1).to_string())
            .output()
            .unwrap(),
        "github_app_digest_capacity_exceeded",
    );
}

#[test]
fn skipped_merge_payloads_consume_the_aggregate_commit_byte_budget() {
    let mut ids = Vec::new();
    let f = fixture(|host, root, tree, ancestor| {
        let a = commit(host, root, tree, &[ancestor], "a");
        let b = commit(host, root, tree, &[ancestor], "b");
        let merge = commit(
            host,
            root,
            tree,
            &[&a, &b],
            "merge body charged even though omitted",
        );
        ids = vec![ancestor.to_owned(), a, b, merge.clone()];
        merge
    });
    let bytes: usize = ids
        .iter()
        .map(|id| {
            git(&f.host, &f.root, &["cat-file", "-s", id])
                .parse::<usize>()
                .unwrap()
        })
        .sum();
    let (exact, _) = value(
        history(&f)
            .arg("--max-history-bytes")
            .arg(bytes.to_string()),
    );
    assert_eq!(exact["history"]["visitedCommits"], 4);
    assert_eq!(exact["history"]["mergeCommitsSkipped"], 1);
    marker(
        history(&f)
            .arg("--max-history-bytes")
            .arg((bytes - 1).to_string())
            .output()
            .unwrap(),
        "github_app_digest_capacity_exceeded",
    );
}

#[test]
fn in_budget_substituted_ancestor_bytes_cannot_bypass_object_identity() {
    let f = chain();
    let ancestor = object_path(&f.root, &f.ancestor);
    std::fs::remove_file(&ancestor).unwrap();
    std::fs::copy(object_path(&f.root, &f.head), ancestor).unwrap();
    let before = files(&f.root);
    marker(
        history(&f).output().unwrap(),
        "github_app_digest_history_unavailable",
    );
    assert_eq!(files(&f.root), before);
}

#[test]
fn invalid_utf8_ancestor_messages_are_not_lossily_decoded_or_echoed() {
    let f = fixture(|host, root, tree, _| {
        let mut raw = format!("tree {tree}\nauthor Fixture <fixture@example.invalid> 1 +0000\ncommitter Fixture <fixture@example.invalid> 1 +0000\n\nINPUT_BODY_CANARY ").into_bytes();
        raw.push(0xff);
        let bad = commit_bytes(host, root, &raw);
        commit(host, root, tree, &[&bad], "valid selected head")
    });
    marker(
        history(&f).output().unwrap(),
        "github_app_digest_history_unavailable",
    );
}

#[test]
fn a_parent_with_a_non_commit_object_type_is_unavailable() {
    let f = fixture(|host, root, tree, _| {
        let path = root.join("blob-input.txt");
        std::fs::write(&path, b"INPUT_BODY_CANARY").unwrap();
        let blob = git(host, root, &["hash-object", "-w", "--", "blob-input.txt"]);
        std::fs::remove_file(path).unwrap();
        commit(host, root, tree, &[&blob], "valid selected head")
    });
    marker(
        history(&f).output().unwrap(),
        "github_app_digest_history_unavailable",
    );
}

#[test]
fn parent_like_message_lines_never_become_header_parent_links() {
    let f = fixture(|host, root, tree, _| {
        let ancestor = commit(
            host,
            root,
            tree,
            &[],
            &format!(
                "message subject\n\nparent {}\nHISTORY_PARENT_TEXT_CANARY",
                "0".repeat(40)
            ),
        );
        commit(host, root, tree, &[&ancestor], "selected head")
    });
    let (result, _) = value(&mut history(&f));
    assert_eq!(result["history"]["visitedCommits"], 2);
    assert_eq!(result["history"]["exhausted"], true);
    assert!(
        result["commits"][0]["body"]
            .as_str()
            .unwrap()
            .contains("HISTORY_PARENT_TEXT_CANARY")
    );
}

#[test]
fn history_aggregate_raw_commit_bytes_have_an_exact_runtime_boundary() {
    let f = chain();
    let bytes: usize = [&f.head, &f.ancestor]
        .into_iter()
        .map(|id| {
            git(&f.host, &f.root, &["cat-file", "-s", id])
                .parse::<usize>()
                .unwrap()
        })
        .sum();
    let before = files(&f.root);
    let (exact, _) = value(
        history(&f)
            .arg("--max-history-bytes")
            .arg(bytes.to_string()),
    );
    assert_eq!(exact["history"]["visitedCommits"], 2);
    marker(
        history(&f)
            .arg("--max-history-bytes")
            .arg((bytes - 1).to_string())
            .output()
            .unwrap(),
        "github_app_digest_capacity_exceeded",
    );
    assert_eq!(files(&f.root), before);
}

#[test]
fn history_reverifies_the_captured_receipt_and_commit_artifact_before_git_reads() {
    for field in ["commitId", "commitArtifact"] {
        let f = chain();
        let input: Value = serde_json::from_slice(&std::fs::read(&f.request).unwrap()).unwrap();
        let id = input["snapshotId"].as_str().unwrap();
        let db = rusqlite::Connection::open(f.root.join(".yoke/index.sqlite")).unwrap();
        let raw: String = db
            .query_row(
                "SELECT value FROM records WHERE kind=?1 AND id=?2",
                rusqlite::params![yoke::git_workflows::SOURCE_KIND, id],
                |row| row.get(0),
            )
            .unwrap();
        let mut proof: Value = serde_json::from_str(&raw).unwrap();
        if field == "commitId" {
            proof[field] = json!(f.ancestor);
            db.execute(
                "UPDATE records SET value=?1 WHERE kind=?2 AND id=?3",
                rusqlite::params![
                    serde_json::to_string(&proof).unwrap(),
                    yoke::git_workflows::SOURCE_KIND,
                    id
                ],
            )
            .unwrap();
        } else {
            std::fs::write(
                f.root
                    .join(".yoke/blobs")
                    .join(proof["commitArtifactId"].as_str().unwrap()),
                b"INPUT_BODY_CANARY",
            )
            .unwrap();
        }
        drop(db);
        let before = files(&f.root);
        marker(
            history(&f).output().unwrap(),
            "github_app_digest_source_unavailable",
        );
        assert_eq!(files(&f.root), before);
    }
}

#[test]
fn history_first_sql_wait_uses_the_cli_deadline_without_store_repair() {
    let f = chain();
    // BEGIN EXCLUSIVE in WAL mode still admits readers. Select rollback-journal
    // mode on this owned fixture so the first SQL read really is blocked.
    let db = rusqlite::Connection::open(f.root.join(".yoke/index.sqlite")).unwrap();
    let mode: String = db
        .query_row("PRAGMA journal_mode=DELETE", [], |row| row.get(0))
        .unwrap();
    assert_eq!(mode, "delete");
    // Calibrate only CLI startup/source admission, excluding healthy Git reads.
    let start = std::time::Instant::now();
    let baseline = super::preview(&f.root, &f.profile, &f.request)
        .output()
        .unwrap();
    let baseline_time = start.elapsed();
    assert!(
        baseline.status.success(),
        "{}",
        String::from_utf8_lossy(&baseline.stderr)
    );
    let before = files(&f.root);
    db.execute_batch("BEGIN EXCLUSIVE").unwrap();
    let start = std::time::Instant::now();
    let output = history(&f).args(["--timeout", "25"]).output().unwrap();
    let elapsed = start.elapsed();
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.to_ascii_lowercase().contains("deadline"),
        "Blocked first SQL read must report the CLI deadline: {stderr}"
    );
    assert!(
        elapsed < baseline_time + std::time::Duration::from_millis(150),
        "History SQL wait took {elapsed:?}, baseline {baseline_time:?}"
    );
    db.execute_batch("ROLLBACK").unwrap();
    assert_eq!(files(&f.root), before);
    let mode: String = db
        .query_row("PRAGMA journal_mode", [], |row| row.get(0))
        .unwrap();
    assert_eq!(
        mode, "delete",
        "Existing-only reads must not repair journal mode"
    );
}

#[test]
fn oversized_ancestor_is_refused_before_its_corrupt_object_hash_is_read() {
    let f = chain();
    let tree = git(&f.host, &f.root, &["rev-parse", "HEAD^{tree}"]);
    let large = commit(
        &f.host,
        &f.root,
        &tree,
        &[],
        &format!("UNREAD_ANCESTOR_BODY_CANARY\n{}", "x".repeat(1_000_000)),
    );
    // Git loose objects are read-only on Windows. Replace the owned fixture
    // path instead of attempting to overwrite its read-only file.
    let ancestor_path = object_path(&f.root, &f.ancestor);
    std::fs::remove_file(&ancestor_path).unwrap();
    std::fs::copy(object_path(&f.root, &large), ancestor_path).unwrap();
    let before = files(&f.root);
    marker(
        history(&f).output().unwrap(),
        "github_app_digest_capacity_exceeded",
    );
    assert_eq!(files(&f.root), before);
}

#[test]
fn missing_ancestor_refuses_without_a_successful_empty_history_or_store_repair() {
    let f = chain();
    std::fs::remove_file(object_path(&f.root, &f.ancestor)).unwrap();
    let before = files(&f.root);
    marker(
        history(&f).output().unwrap(),
        "github_app_digest_history_unavailable",
    );
    assert_eq!(files(&f.root), before);
}

#[test]
fn unsupported_encoding_and_malformed_parent_headers_have_static_errors() {
    for header in ["encoding ISO-8859-1\n", "parent ../INPUT_BODY_CANARY\n"] {
        let f = fixture(|host, root, tree, _| {
            let raw = format!(
                "tree {tree}\n{header}author Fixture <fixture@example.invalid> 1 +0000\ncommitter Fixture <fixture@example.invalid> 1 +0000\n\nINPUT_BODY_CANARY\n"
            );
            let ancestor = commit_bytes(host, root, raw.as_bytes());
            commit(host, root, tree, &[&ancestor], "selected head")
        });
        let before = files(&f.root);
        marker(
            history(&f).output().unwrap(),
            "github_app_digest_history_unavailable",
        );
        assert_eq!(files(&f.root), before);
    }
}

#[test]
fn misplaced_canonical_parent_header_refuses_without_partial_output_or_store_repair() {
    let f = fixture(|host, root, tree, unrelated| {
        let raw = format!(
            "tree {tree}\nauthor Fixture <fixture@example.invalid> 1 +0000\ncommitter Fixture <fixture@example.invalid> 1 +0000\nparent {unrelated}\n\nINPUT_BODY_CANARY\n"
        );
        let ancestor = commit_bytes(host, root, raw.as_bytes());
        commit(host, root, tree, &[&ancestor], "selected head")
    });
    let before = files(&f.root);
    marker(
        history(&f).output().unwrap(),
        "github_app_digest_history_unavailable",
    );
    assert_eq!(files(&f.root), before);
}
