use std::{fs, path::Path, process::Command};
use tempfile::TempDir;
use yoke::{
    Engine,
    analysis::AnalyzerEpoch,
    capture::Deadline,
    git_workflows::{self, HostGit, WorkflowOptions},
};

fn git(host: &HostGit, root: &Path, args: &[&str]) -> String {
    let output = Command::new(&host.executable)
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
            "core.hooksPath=",
            "-c",
            "commit.gpgsign=false",
            "-c",
            "core.autocrlf=false",
            "-c",
            "user.name=Epoch Fixture",
            "-c",
            "user.email=epoch@example.invalid",
        ])
        .args(args)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap().trim().into()
}

fn fixture() -> (TempDir, HostGit, String) {
    let root = TempDir::new().unwrap();
    let host = HostGit::discover().unwrap();
    git(&host, root.path(), &["init", "-b", "main"]);
    fs::write(
        root.path().join("local.lisp"),
        "(defun run () (labels ((local () 1)) (local)))\n",
    )
    .unwrap();
    git(&host, root.path(), &["add", "local.lisp"]);
    git(&host, root.path(), &["commit", "-m", "base"]);
    let base = git(&host, root.path(), &["rev-parse", "HEAD"]);
    (root, host, base)
}

#[test]
fn selected_epoch_is_shared_by_working_and_committed_git_report_sides() {
    let (root, host, base) = fixture();
    fs::write(
        root.path().join("local.lisp"),
        "(defun run () (labels ((local () 2)) (local)))\n",
    )
    .unwrap();
    git(&host, root.path(), &["add", "local.lisp"]);
    git(&host, root.path(), &["commit", "-m", "change"]);
    fs::write(
        root.path().join("local.lisp"),
        "(defun run () (labels ((local () 3)) (local)))\n",
    )
    .unwrap();
    let mut engine = Engine::open(root.path()).unwrap();
    let options = WorkflowOptions {
        base_ref: base,
        ..Default::default()
    };
    let old = git_workflows::analyze(&mut engine, &host, &options, &Deadline::new(30000)).unwrap();
    assert_eq!(
        git_workflows::validate_source(
            &engine.store,
            old["baseSnapshotId"].as_str().unwrap(),
            &Deadline::new(10000)
        )
        .unwrap()["analyzer"],
        "yoke-tree-sitter-8"
    );
    assert!(
        !old["changedSymbols"]
            .as_array()
            .unwrap()
            .iter()
            .any(|node| node["name"] == "local")
    );

    engine.capture_epoch = AnalyzerEpoch::V9;
    for head_ref in [None, Some("HEAD".into())] {
        let options = WorkflowOptions {
            head_ref,
            ..options.clone()
        };
        let report =
            git_workflows::analyze(&mut engine, &host, &options, &Deadline::new(30000)).unwrap();
        for side in ["base", "head"] {
            let key = format!("{side}SnapshotId");
            let snapshot = engine
                .store
                .snapshot(report[&key].as_str().unwrap())
                .unwrap();
            let epoch = yoke::capture::analyzer_epoch_for_snapshot_with_deadline(
                &engine.store,
                &snapshot,
                &Deadline::new(10000),
            )
            .unwrap();
            assert_eq!(
                epoch,
                AnalyzerEpoch::V9,
                "selected epoch did not reach {side} in {}",
                report["headMode"]
            );
            assert!(
                report["changedSymbols"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|node| node["name"] == "local"
                        && node["side"] == side
                        && node["producer"] == "yoke-tree-sitter-9"),
                "missing selected-epoch local declaration on {side}"
            );
        }
        let head = engine
            .store
            .snapshot(report["headSnapshotId"].as_str().unwrap())
            .unwrap();
        let expected_value = if options.head_ref.is_some() { "2" } else { "3" };
        let (source, _) = yoke::query::source(&engine.store, &head, "local.lisp").unwrap();
        assert!(source.contains(&format!("local () {expected_value}")));
        engine.capture_epoch = AnalyzerEpoch::V8;
        assert_eq!(
            git_workflows::load(
                &engine.store,
                report["artifactId"].as_str().unwrap(),
                &Deadline::new(10000)
            )
            .unwrap(),
            report
        );
        engine
            .store
            .db
            .execute("DELETE FROM graphs WHERE id=?1", [&head.id])
            .unwrap();
        let (_, replay, _, _) = engine
            .analyze_snapshot(head, &Deadline::new(10000))
            .unwrap();
        assert!(
            replay
                .entities
                .iter()
                .any(|node| node.name == "local" && node.producer == "yoke-tree-sitter-9")
        );
        engine.capture_epoch = AnalyzerEpoch::V9;
    }
    assert_eq!(
        git_workflows::load(
            &engine.store,
            old["artifactId"].as_str().unwrap(),
            &Deadline::new(10000)
        )
        .unwrap(),
        old
    );
}

#[test]
fn worktree_cache_seed_uses_selected_epoch_without_copying_other_epoch_entries() {
    let (root, host, _) = fixture();
    let mut parent = Engine::open(root.path()).unwrap();
    parent.index(&Deadline::new(10000)).unwrap();
    parent.capture_epoch = AnalyzerEpoch::V9;
    parent.index(&Deadline::new(10000)).unwrap();
    let child_dir = TempDir::new().unwrap();
    let child_root = child_dir.path().join("child");
    git(
        &host,
        root.path(),
        &[
            "worktree",
            "add",
            "--detach",
            child_root.to_str().unwrap(),
            "HEAD",
        ],
    );
    let mut child = Engine::open(&child_root).unwrap();
    child.capture_epoch = AnalyzerEpoch::V9;
    let receipt = git_workflows::seed_worktree_cache(
        &mut child,
        &host,
        root.path(),
        &parent.store.workspace_id,
        &Deadline::new(30000),
    )
    .unwrap();
    assert_eq!(receipt["analyzer"], "yoke-tree-sitter-9");
    assert_eq!(receipt["copiedParserEntries"], 1);
    assert_eq!(receipt["parsedFiles"], 0);
    assert_eq!(receipt["reusedFiles"], 1);
    let snapshot = child
        .store
        .snapshot(receipt["targetSnapshotId"].as_str().unwrap())
        .unwrap();
    let graph = child.store.graph(&snapshot.id).unwrap();
    assert!(
        graph
            .entities
            .iter()
            .any(|node| node.name == "local" && node.producer == "yoke-tree-sitter-9")
    );
    let old_key = AnalyzerEpoch::V8.cache_key(&snapshot.files[0]);
    let old_count: usize = child
        .store
        .db
        .query_row(
            "SELECT count(*) FROM cache WHERE key=?1",
            [&old_key],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(
        old_count, 0,
        "parent analyzer-8 cache leaked into selected analyzer-9 seed"
    );
    let source_before = fs::read(child_root.join("local.lisp")).unwrap();
    child.capture_epoch = AnalyzerEpoch::V8;
    let (_, replay, _, _) = child
        .analyze_snapshot(snapshot, &Deadline::new(10000))
        .unwrap();
    assert!(
        replay
            .entities
            .iter()
            .any(|node| node.name == "local" && node.producer == "yoke-tree-sitter-9")
    );
    assert_eq!(
        fs::read(child_root.join("local.lisp")).unwrap(),
        source_before
    );
}
