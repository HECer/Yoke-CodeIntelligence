use serde_json::Value;
use std::{fs, path::Path, process::Command};
use yoke::{
    capture::Deadline,
    git_workflows::{
        HostGit,
        repository_scope::{self, Limits, Options},
    },
};

fn git(host: &HostGit, root: &Path, args: &[&str]) -> Vec<u8> {
    let null = if cfg!(windows) { "NUL" } else { "/dev/null" };
    let output = Command::new(&host.executable)
        .arg("-C")
        .arg(root)
        .args(["-c", "core.hooksPath=", "-c", "commit.gpgSign=false"])
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", null)
        .env("GIT_AUTHOR_NAME", "Owned fixture")
        .env("GIT_AUTHOR_EMAIL", "owned@example.invalid")
        .env("GIT_COMMITTER_NAME", "Owned fixture")
        .env("GIT_COMMITTER_EMAIL", "owned@example.invalid")
        .args(args)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{:?}: {}",
        args,
        String::from_utf8_lossy(&output.stderr)
    );
    output.stdout
}
fn init(host: &HostGit, root: &Path) {
    fs::create_dir_all(root).unwrap();
    git(host, root, &["init", "--quiet", "--template="]);
}
fn commit(host: &HostGit, root: &Path, name: &str) -> String {
    fs::write(root.join(name), "fn owned() { let π = 1; }\r\n").unwrap();
    git(host, root, &["add", "--", name]);
    git(host, root, &["commit", "--quiet", "-m", "Owned fixture"]);
    String::from_utf8(git(host, root, &["rev-parse", "HEAD"]))
        .unwrap()
        .trim()
        .into()
}
fn inspect(root: &Path, host: &HostGit, submodules: bool, nested: bool) -> Value {
    repository_scope::inspect(
        root,
        host,
        &Options {
            allow_index_read: true,
            follow_submodules: submodules,
            follow_nested_repositories: nested,
            ..Default::default()
        },
        &Deadline::new(30000),
    )
    .unwrap()
    .value()
    .clone()
}

#[test]
fn independent_follow_choices_distinguish_initialized_gitlinks_nested_and_missing_children() {
    let fixture = tempfile::tempdir().unwrap();
    let root = fixture.path().join("parent π '");
    let host = HostGit::discover().unwrap();
    init(&host, &root);
    commit(&host, &root, "main.rs");
    let sub = root.join("submodule ß");
    init(&host, &sub);
    let oid = commit(&host, &sub, "sub.rs");
    let nested = root.join("nested");
    init(&host, &nested);
    commit(&host, &nested, "nested.rs");
    fs::create_dir(root.join("missing")).unwrap();
    for path in ["submodule ß", "missing"] {
        git(
            &host,
            &root,
            &[
                "update-index",
                "--add",
                "--cacheinfo",
                &format!("160000,{oid},{path}"),
            ],
        );
    }
    let hidden = root.join("hidden");
    init(&host, &hidden);
    commit(&host, &hidden, "private.rs");
    fs::write(root.join(".gitignore"), "hidden/\nmain.rs\n").unwrap();
    fs::write(sub.join(".gitignore"), "ignored.rs\n").unwrap();
    fs::write(sub.join("ignored.rs"), "must remain ignored").unwrap();
    let before = fs::read(root.join(".git/index")).unwrap();
    for (submodules, nested) in [(false, false), (true, false), (false, true), (true, true)] {
        let report = inspect(&root, &host, submodules, nested);
        let observation = &report["observation"];
        let files = observation["files"]
            .as_array()
            .unwrap()
            .iter()
            .map(|f| f["path"].as_str().unwrap())
            .collect::<Vec<_>>();
        assert!(files.contains(&"main.rs"));
        assert_eq!(files.contains(&"submodule ß/sub.rs"), submodules);
        assert_eq!(files.contains(&"nested/nested.rs"), nested);
        assert!(!files.iter().any(|p| p.contains("ignored.rs")
            || p.contains("private.rs")
            || p.starts_with("missing/")));
        let boundary = |path: &str| {
            observation["boundaries"]
                .as_array()
                .unwrap()
                .iter()
                .find(|b| b["path"] == path)
                .unwrap()
        };
        assert_eq!(boundary("submodule ß")["kind"], "gitlink");
        assert_eq!(boundary("nested")["kind"], "nested-clone-candidate");
        assert_eq!(
            boundary("missing")["state"],
            if submodules {
                "uninitialized-or-marker-missing"
            } else {
                "not-followed"
            }
        );
        assert_eq!(observation["sourceAccessGranted"], false);
        assert_eq!(observation["capturePolicyPublished"], false);
        assert_eq!(observation["complete"], false);
        assert_eq!(fs::read(root.join(".git/index")).unwrap(), before);
        assert!(!root.join(".yoke").exists());
        assert!(!sub.join(".yoke").exists());
    }
    assert_eq!(
        inspect(&root, &host, true, true),
        inspect(&root, &host, true, true)
    );
}

#[test]
fn worktree_git_files_follow_only_when_independently_selected() {
    let fixture = tempfile::tempdir().unwrap();
    let host = HostGit::discover().unwrap();
    let root = fixture.path().join("parent");
    let other = fixture.path().join("external-admin");
    init(&host, &root);
    commit(&host, &root, "main.rs");
    init(&host, &other);
    commit(&host, &other, "work.rs");
    let child = root.join("linked π");
    git(
        &host,
        &other,
        &[
            "worktree",
            "add",
            "--detach",
            child.to_str().unwrap(),
            "HEAD",
        ],
    );
    assert!(child.join(".git").is_file());
    assert!(
        !inspect(&root, &host, false, false)["observation"]["files"]
            .as_array()
            .unwrap()
            .iter()
            .any(|r| r["path"] == "linked π/work.rs")
    );
    assert!(
        inspect(&root, &host, false, true)["observation"]["files"]
            .as_array()
            .unwrap()
            .iter()
            .any(|r| r["path"] == "linked π/work.rs")
    );
}

#[test]
fn missing_child_marker_does_not_resolve_upward_and_broken_selected_git_files_fail() {
    let fixture = tempfile::tempdir().unwrap();
    let host = HostGit::discover().unwrap();
    let root = fixture.path().join("parent");
    init(&host, &root);
    let oid = commit(&host, &root, "main.rs");
    let child = root.join("broken");
    fs::create_dir(&child).unwrap();
    fs::write(child.join(".git"), "gitdir: nonexisting-admin\n").unwrap();
    git(
        &host,
        &root,
        &[
            "update-index",
            "--add",
            "--cacheinfo",
            &format!("160000,{oid},broken"),
        ],
    );
    assert!(
        repository_scope::inspect(
            &root,
            &host,
            &Options {
                allow_index_read: true,
                follow_submodules: true,
                ..Default::default()
            },
            &Deadline::new(30000)
        )
        .is_err()
    );
    fs::remove_file(child.join(".git")).unwrap();
    let report = inspect(&root, &host, true, false);
    assert_eq!(
        report["observation"]["repositories"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        report["observation"]["boundaries"][0]["state"],
        "uninitialized-or-marker-missing"
    );
}

#[test]
fn permissions_and_zero_budgets_refuse_without_creating_state() {
    let fixture = tempfile::tempdir().unwrap();
    let host = HostGit::discover().unwrap();
    let root = fixture.path().join("parent");
    init(&host, &root);
    commit(&host, &root, "main.rs");
    assert!(
        repository_scope::inspect(
            Path::new("/nonexisting-owned-path"),
            &host,
            &Options::default(),
            &Deadline::new(30000)
        )
        .unwrap_err()
        .to_string()
        .contains("permission_required")
    );
    for limits in [
        Limits {
            max_repositories: 0,
            ..Default::default()
        },
        Limits {
            max_entries: 0,
            ..Default::default()
        },
        Limits {
            max_index_bytes: 0,
            ..Default::default()
        },
        Limits {
            max_output_bytes: 0,
            ..Default::default()
        },
        Limits {
            max_repositories: 65,
            ..Default::default()
        },
    ] {
        assert!(
            repository_scope::inspect(
                &root,
                &host,
                &Options {
                    allow_index_read: true,
                    limits,
                    ..Default::default()
                },
                &Deadline::new(30000)
            )
            .is_err()
        );
        assert!(!root.join(".yoke").exists());
    }
    assert!(
        repository_scope::inspect(
            &root,
            &host,
            &Options {
                allow_index_read: true,
                ..Default::default()
            },
            &Deadline::new(0)
        )
        .is_err()
    );
}

#[test]
fn cli_requires_explicit_index_trust_and_refuses_source_flags_without_state() {
    let fixture = tempfile::tempdir().unwrap();
    let host = HostGit::discover().unwrap();
    let root = fixture.path().join("parent");
    init(&host, &root);
    commit(&host, &root, "main.rs");
    for flags in [
        vec![],
        vec!["--allow-index-read"],
        vec!["--trust-git"],
        vec!["--allow-index-read", "--trust-git", "--allow-writes"],
        vec![
            "--allow-index-read",
            "--trust-git",
            "--max-repositories",
            "0",
        ],
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_yoke"))
            .arg("--root")
            .arg(&root)
            .args(["git", "repository-scope"])
            .args(&flags)
            .output()
            .unwrap();
        assert!(!output.status.success(), "{flags:?}");
        assert!(output.stdout.is_empty());
        assert!(!root.join(".yoke").exists());
    }
    let output = Command::new(env!("CARGO_BIN_EXE_yoke"))
        .arg("--root")
        .arg(&root)
        .args([
            "git",
            "repository-scope",
            "--allow-index-read",
            "--trust-git",
        ])
        .arg("--git")
        .arg(&host.executable)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let value: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["observation"]["files"][0]["path"], "main.rs");
    assert!(!root.join(".yoke").exists());
}

#[test]
fn actual_local_submodule_git_file_retains_its_pinned_parent_index_boundary() {
    let fixture = tempfile::tempdir().unwrap();
    let host = HostGit::discover().unwrap();
    let root = fixture.path().join("parent π '");
    let origin = fixture.path().join("owned-module-origin");
    init(&host, &root);
    commit(&host, &root, "main.rs");
    init(&host, &origin);
    let pinned = commit(&host, &origin, "module.rs");
    // Explicit owned fixture setup permits this one local file-protocol clone.
    // The inspected production path still forces protocol.allow=never.
    git(
        &host,
        &root,
        &[
            "-c",
            "protocol.file.allow=always",
            "submodule",
            "add",
            "--",
            origin.to_str().unwrap(),
            "modules/real",
        ],
    );
    let child = root.join("modules/real");
    assert!(child.join(".git").is_file());
    let index = fs::read(root.join(".git/index")).unwrap();
    let marker = fs::read(child.join(".git")).unwrap();
    let report = inspect(&root, &host, true, false);
    let observation = &report["observation"];
    assert!(
        observation["files"]
            .as_array()
            .unwrap()
            .iter()
            .any(|f| f["path"] == "modules/real/module.rs")
    );
    let boundary = observation["boundaries"]
        .as_array()
        .unwrap()
        .iter()
        .find(|b| b["path"] == "modules/real")
        .unwrap();
    assert_eq!(boundary["kind"], "gitlink");
    assert_eq!(boundary["state"], "followed");
    let parent = observation["repositories"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["path"] == ".")
        .unwrap();
    assert!(
        parent["rows"]
            .as_array()
            .unwrap()
            .iter()
            .any(|r| r["path"] == "modules/real"
                && r["mode"] == "160000"
                && r["objectId"] == pinned
                && r["stage"] == 0)
    );
    assert_eq!(fs::read(root.join(".git/index")).unwrap(), index);
    assert_eq!(fs::read(child.join(".git")).unwrap(), marker);
    assert!(!root.join(".yoke").exists());
}

#[test]
fn active_fsmonitor_canary_is_disabled_and_source_filters_remain_inert() {
    let fixture = tempfile::tempdir().unwrap();
    let host = HostGit::discover().unwrap();
    let root = fixture.path().join("parent");
    init(&host, &root);
    commit(&host, &root, "main.rs");
    let hook = fixture.path().join("owned-monitor.sh");
    let canary = fixture.path().join("executed-canary");
    // The hook path is outside the analyzed root and is an explicitly authored,
    // harmless host test program. A positive control proves it actually runs.
    let target = canary.to_str().unwrap().replace('\\', "/");
    fs::write(
        &hook,
        format!("#!/bin/sh\nprintf executed > '{target}'\nprintf '\\0'\n"),
    )
    .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&hook, fs::Permissions::from_mode(0o755)).unwrap();
    }
    let program = hook.to_str().unwrap().replace('\\', "/");
    git(&host, &root, &["config", "core.fsmonitor", &program]);
    git(&host, &root, &["ls-files", "--cached", "-z"]);
    assert_eq!(fs::read(&canary).unwrap(), b"executed");
    fs::remove_file(&canary).unwrap();
    fs::write(root.join(".gitattributes"), "*.rs filter=owned-canary\n").unwrap();
    git(
        &host,
        &root,
        &[
            "-c",
            "core.fsmonitor=false",
            "config",
            "filter.owned-canary.clean",
            &program,
        ],
    );
    git(
        &host,
        &root,
        &[
            "-c",
            "core.fsmonitor=false",
            "config",
            "filter.owned-canary.smudge",
            &program,
        ],
    );
    let index = fs::read(root.join(".git/index")).unwrap();
    let source = fs::read(root.join("main.rs")).unwrap();
    let config = fs::read(root.join(".git/config")).unwrap();
    let report = inspect(&root, &host, false, false);
    assert!(
        report["observation"]["files"]
            .as_array()
            .unwrap()
            .iter()
            .any(|f| f["path"] == "main.rs")
    );
    assert!(!canary.exists());
    assert_eq!(fs::read(root.join("main.rs")).unwrap(), source);
    assert_eq!(fs::read(root.join(".git/index")).unwrap(), index);
    assert_eq!(fs::read(root.join(".git/config")).unwrap(), config);
}

#[test]
fn own_state_cache_names_do_not_change_the_index_observation() {
    let fixture = tempfile::tempdir().unwrap();
    let host = HostGit::discover().unwrap();
    let root = fixture.path().join("parent");
    init(&host, &root);
    commit(&host, &root, "main.rs");
    let before = inspect(&root, &host, false, false);
    fs::create_dir_all(root.join(".yoke/blobs")).unwrap();
    fs::create_dir_all(root.join("nested/.YoKe/blobs")).unwrap();
    fs::write(root.join(".yoke/blobs/new-cache"), b"owned cache").unwrap();
    fs::write(root.join("nested/.YoKe/blobs/other-cache"), b"owned cache").unwrap();
    assert_eq!(inspect(&root, &host, false, false), before);
}

#[test]
fn explicit_capture_persists_exact_selection_and_child_readonly_without_live_replay() {
    let fixture = tempfile::tempdir().unwrap();
    let host = HostGit::discover().unwrap();
    let root = fixture.path().join("parent π '");
    init(&host, &root);
    commit(&host, &root, "main.rs");
    let child = root.join("nested");
    init(&host, &child);
    commit(&host, &child, "child.rs");
    let original = fs::read(child.join("child.rs")).unwrap();
    let mut engine = yoke::Engine::open(&root).unwrap();
    let options = Options {
        allow_index_read: true,
        follow_nested_repositories: true,
        ..Default::default()
    };
    let snapshot = repository_scope::capture(&mut engine, &host, &options, &Deadline::new(30000))
        .unwrap()
        .0;
    let repeated = repository_scope::capture(&mut engine, &host, &options, &Deadline::new(30000))
        .unwrap()
        .0;
    assert_eq!(
        serde_json::to_value(&snapshot).unwrap(),
        serde_json::to_value(&repeated).unwrap()
    );
    let scope = yoke::capture::source_scope_policy(&engine.store, &snapshot).unwrap();
    assert!(scope.path_policy("nested/child.rs").unwrap().read_only);
    assert!(!scope.path_policy("main.rs").unwrap().read_only);
    assert!(scope.check_host_write_scope("nested/child.rs").is_err());
    assert!(scope.check_host_write_scope("new.rs").is_err());
    assert!(scope.check_host_write_scope("main.rs").is_ok());
    let saved: yoke::capture::CaptureOptionsRecord = engine
        .store
        .get_record(yoke::capture::OPTIONS_RECORD_KIND, &snapshot.id)
        .unwrap();
    assert!(saved.options.repository_scope.is_some());
    assert!(
        yoke::capture::capture_with_options(
            &root,
            &engine.store,
            &saved.options,
            &Deadline::new(30000)
        )
        .is_err()
    );
    let intended = fixture.path().canonicalize().unwrap();
    let from = child.canonicalize().unwrap();
    let to = fixture.path().join("removed-owned-child");
    assert!(
        from.starts_with(&intended) && to.parent().unwrap().canonicalize().unwrap() == intended
    );
    fs::rename(&from, &to).unwrap();
    let scope = yoke::capture::source_scope_policy(&engine.store, &snapshot).unwrap();
    assert_eq!(scope.read_source("nested/child.rs").unwrap(), original);
    assert!(scope.path_policy("nested/child.rs").unwrap().read_only);
    assert_eq!(
        serde_json::to_value(engine.store.snapshot(&snapshot.id).unwrap()).unwrap(),
        serde_json::to_value(&snapshot).unwrap()
    );
}

#[test]
fn no_follow_capture_excludes_children_and_new_cli_capture_is_explicit() {
    let fixture = tempfile::tempdir().unwrap();
    let host = HostGit::discover().unwrap();
    let root = fixture.path().join("parent");
    init(&host, &root);
    commit(&host, &root, "main.rs");
    let child = root.join("nested");
    init(&host, &child);
    commit(&host, &child, "child.rs");
    let before = inspect(&root, &host, false, false);
    assert!(!root.join(".yoke").exists());
    let result = Command::new(env!("CARGO_BIN_EXE_yoke"))
        .arg("--root")
        .arg(&root)
        .args([
            "git",
            "repository-scope",
            "--capture",
            "--allow-index-read",
            "--trust-git",
        ])
        .arg("--git")
        .arg(&host.executable)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let report: Value = serde_json::from_slice(&result.stdout).unwrap();
    let engine = yoke::Engine::open(&root).unwrap();
    let snapshot = engine
        .store
        .snapshot(report["snapshotId"].as_str().unwrap())
        .unwrap();
    assert!(snapshot.files.iter().any(|f| f.path == "main.rs"));
    assert!(!snapshot.files.iter().any(|f| f.path.starts_with("nested/")));
    assert!(snapshot.omissions.iter().any(
        |o| o.path == "nested" && o.reason == "repository-nested-clone-candidate-not-followed"
    ));
    assert_eq!(inspect(&root, &host, false, false), before);
    assert_eq!(report["crossedRepositoryPolicy"], "read-only");
    assert_eq!(report["projectSourceWritten"], false);
}

#[test]
fn stored_capture_refuses_forged_bindings_and_keeps_dirty_bytes_distinct_from_index_objects() {
    let fixture = tempfile::tempdir().unwrap();
    let host = HostGit::discover().unwrap();
    let root = fixture.path().join("parent");
    init(&host, &root);
    commit(&host, &root, "main.rs");
    let child = root.join("nested");
    init(&host, &child);
    commit(&host, &child, "child.rs");
    let dirty = b"fn dirty_working_source() { }\r\n";
    fs::write(child.join("child.rs"), dirty).unwrap();
    let mut engine = yoke::Engine::open(&root).unwrap();
    let snapshot = repository_scope::capture(
        &mut engine,
        &host,
        &Options {
            allow_index_read: true,
            follow_nested_repositories: true,
            ..Default::default()
        },
        &Deadline::new(30000),
    )
    .unwrap()
    .0;
    assert_eq!(
        yoke::capture::source_scope_policy(&engine.store, &snapshot)
            .unwrap()
            .read_source("nested/child.rs")
            .unwrap(),
        dirty
    );
    let saved: yoke::capture::CaptureOptionsRecord = engine
        .store
        .get_record(yoke::capture::OPTIONS_RECORD_KIND, &snapshot.id)
        .unwrap();
    let base = saved.options.repository_scope.unwrap();
    let mut bad = base.clone();
    bad.files.push("../outside".into());
    assert!(bad.validated().is_err());
    let mut bad = base.clone();
    bad.policy = "future-policy-v9".into();
    assert!(bad.validated().is_err());
    let mut bad = base.clone();
    bad.file_bindings
        .iter_mut()
        .find(|b| b.observed_path == "nested/child.rs")
        .unwrap()
        .captured_path = "main.rs".into();
    bad.files = vec!["main.rs".into()];
    assert!(bad.validated().is_err());
    let mut bad = base.clone();
    bad.observation["observation"]["sourceAccessGranted"] = true.into();
    assert!(bad.validated().is_err());
    let mut bad = base.clone();
    bad.observation["observation"]["repositories"][0]["indexListing"] = "? ../outside\0".into();
    assert!(bad.validated().is_err());
    let mut bad = base.clone();
    bad.read_only_roots.clear();
    assert!(bad.validated().is_err());
    assert_eq!(fs::read(child.join("child.rs")).unwrap(), dirty);
}

#[test]
fn current_repository_selection_restricts_old_writable_snapshots_before_plan_publication() {
    let fixture = tempfile::tempdir().unwrap();
    let host = HostGit::discover().unwrap();
    let root = fixture.path().join("parent");
    init(&host, &root);
    commit(&host, &root, "main.rs");
    let child = root.join("nested");
    init(&host, &child);
    commit(&host, &child, "child.rs");
    let mut engine = yoke::Engine::open(&root).unwrap();
    engine.allow_writes = true;
    let old = engine.index(&Deadline::new(30000)).unwrap().0;
    let member = old
        .files
        .iter()
        .find(|f| f.path == "nested/child.rs")
        .unwrap()
        .clone();
    let selected = repository_scope::capture(
        &mut engine,
        &host,
        &Options {
            allow_index_read: true,
            follow_nested_repositories: true,
            ..Default::default()
        },
        &Deadline::new(30000),
    )
    .unwrap()
    .0;
    let saved: yoke::capture::CaptureOptionsRecord = engine
        .store
        .get_record(yoke::capture::OPTIONS_RECORD_KIND, &selected.id)
        .unwrap();
    engine.capture_options = saved.options;
    let changes = engine.store.db.total_changes();
    let result=engine.invoke("ci_change_prepare",serde_json::json!({"snapshotId":old.id,"edits":[{"kind":"replace","path":member.path,"expectedHash":member.hash,"startByte":0,"endByte":member.size,"text":"fn refused(){}\r\n"}]})).unwrap();
    assert_eq!(result["status"], "error");
    assert!(
        result
            .to_string()
            .contains("repository_capture_current_host_scope_excludes_write"),
        "{result}"
    );
    assert_eq!(engine.store.db.total_changes(), changes);
    let parent = old.files.iter().find(|f| f.path == "main.rs").unwrap();
    let result=engine.invoke("ci_change_prepare",serde_json::json!({"snapshotId":old.id,"edits":[{"kind":"replace","path":parent.path,"expectedHash":parent.hash,"startByte":0,"endByte":parent.size,"text":"fn permitted_plan(){}\r\n"}]})).unwrap();
    assert_eq!(result["status"], "ok", "{result}");
    assert_eq!(
        fs::read(child.join("child.rs")).unwrap(),
        "fn owned() { let π = 1; }\r\n".as_bytes()
    );
}

#[test]
fn capture_and_recheck_share_one_index_byte_budget_before_snapshot_publication() {
    let fixture = tempfile::tempdir().unwrap();
    let host = HostGit::discover().unwrap();
    let root = fixture.path().join("parent");
    init(&host, &root);
    commit(&host, &root, "main.rs");
    let mut engine = yoke::Engine::open(&root).unwrap();
    let old = engine.index(&Deadline::new(30000)).unwrap().0;
    let report = inspect(&root, &host, false, false);
    let one = report["observation"]["acquiredIndexBytes"]
        .as_u64()
        .unwrap() as usize;
    assert!(one > 0);
    let options = Options {
        allow_index_read: true,
        limits: Limits {
            max_index_bytes: 2 * one - 1,
            ..Default::default()
        },
        ..Default::default()
    };
    let before = engine
        .store
        .db
        .query_row("SELECT count(*) FROM snapshots", [], |row| {
            row.get::<_, usize>(0)
        })
        .unwrap();
    assert!(
        repository_scope::capture(&mut engine, &host, &options, &Deadline::new(30000)).is_err()
    );
    assert_eq!(
        engine.store.latest().unwrap().as_deref(),
        Some(old.id.as_str())
    );
    assert_eq!(
        engine
            .store
            .db
            .query_row("SELECT count(*) FROM snapshots", [], |row| row
                .get::<_, usize>(0))
            .unwrap(),
        before
    );
    let options = Options {
        allow_index_read: true,
        limits: Limits {
            max_index_bytes: 2 * one,
            ..Default::default()
        },
        ..Default::default()
    };
    assert!(repository_scope::capture(&mut engine, &host, &options, &Deadline::new(30000)).is_ok());
}

#[test]
fn captured_repository_scope_is_retention_checked_and_copied_collection_replays_without_source_root()
 {
    let fixture = tempfile::tempdir().unwrap();
    let intended = fixture.path().canonicalize().unwrap();
    let host = HostGit::discover().unwrap();
    let root = fixture.path().join("parent");
    init(&host, &root);
    commit(&host, &root, "main.rs");
    let child = root.join("nested");
    init(&host, &child);
    commit(&host, &child, "child.rs");
    let archive = tempfile::tempdir().unwrap();
    let registry = yoke::graph_collections::Registry::open(archive.path()).unwrap();
    let mut engine = yoke::Engine::open(&root).unwrap();
    let (snapshot, graph, _, _) = repository_scope::capture(
        &mut engine,
        &host,
        &Options {
            allow_index_read: true,
            follow_nested_repositories: true,
            ..Default::default()
        },
        &Deadline::new(30000),
    )
    .unwrap();
    let original: Value = engine
        .store
        .get_record(yoke::capture::OPTIONS_RECORD_KIND, &snapshot.id)
        .unwrap();
    let report = yoke::retention::plan(
        &engine.store,
        &yoke::retention::Policy::default(),
        &Deadline::new(30000),
    )
    .unwrap();
    assert_eq!(report["unknownDependencyKinds"], serde_json::json!([]));
    assert_eq!(
        engine
            .store
            .get_record::<Value>(yoke::capture::OPTIONS_RECORD_KIND, &snapshot.id)
            .unwrap(),
        original
    );
    registry
        .register(
            "owned-repositories",
            &engine.store,
            &snapshot,
            &graph,
            &Deadline::new(30000),
        )
        .unwrap();
    drop(engine);
    assert_eq!(fixture.path().canonicalize().unwrap(), intended);
    assert!(root.canonicalize().unwrap().starts_with(&intended));
    fixture.close().unwrap();
    let result = registry
        .query(
            &["owned-repositories".into()],
            &serde_json::json!({"operation":"find","query":"owned","limit":20}),
            &Deadline::new(30000),
        )
        .unwrap();
    assert!(
        result["items"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|row| row["name"] == "owned")
            .count()
            >= 2,
        "{result}"
    );
}

#[test]
fn saved_repository_scope_can_recheck_source_after_git_administration_is_unavailable() {
    let fixture = tempfile::tempdir().unwrap();
    let intended = fixture.path().canonicalize().unwrap();
    let host = HostGit::discover().unwrap();
    let root = fixture.path().join("parent");
    init(&host, &root);
    commit(&host, &root, "main.rs");
    let mut engine = yoke::Engine::open(&root).unwrap();
    let snapshot = repository_scope::capture(
        &mut engine,
        &host,
        &Options {
            allow_index_read: true,
            ..Default::default()
        },
        &Deadline::new(30000),
    )
    .unwrap()
    .0;
    let from = root.join(".git").canonicalize().unwrap();
    let to = fixture.path().join("unavailable-owned-git-admin");
    assert!(
        from.starts_with(&intended) && to.parent().unwrap().canonicalize().unwrap() == intended
    );
    fs::rename(&from, &to).unwrap();
    let recaptured = yoke::capture::capture_for_snapshot(
        &root,
        &engine.store,
        &snapshot,
        &Deadline::new(30000),
    )
    .expect("A checked saved scope must support source-only staleness checks without starting Git");
    assert_eq!(
        serde_json::to_value(recaptured).unwrap(),
        serde_json::to_value(snapshot).unwrap()
    );
}

#[test]
fn saved_scope_observes_deleted_files_without_publishing_a_dangling_source_proof() {
    let fixture = tempfile::tempdir().unwrap();
    let host = HostGit::discover().unwrap();
    let root = fixture.path().join("parent");
    init(&host, &root);
    commit(&host, &root, "main.rs");
    let mut engine = yoke::Engine::open(&root).unwrap();
    let snapshot = repository_scope::capture(
        &mut engine,
        &host,
        &Options {
            allow_index_read: true,
            ..Default::default()
        },
        &Deadline::new(30000),
    )
    .unwrap()
    .0;
    let before = engine.store.db.total_changes();
    fs::remove_file(root.join("main.rs")).unwrap();
    let observed =
        yoke::capture::capture_for_snapshot(&root, &engine.store, &snapshot, &Deadline::new(30000))
            .unwrap();
    assert!(observed.files.is_empty());
    assert!(
        observed
            .omissions
            .iter()
            .any(|o| o.path == "main.rs" && o.reason == "repository-captured-file-missing")
    );
    assert_ne!(observed.id, snapshot.id);
    assert!(engine.store.snapshot(&observed.id).is_err());
    assert_eq!(engine.store.db.total_changes(), before);
    let retirement = yoke::retention::plan(
        &engine.store,
        &yoke::retention::Policy::default(),
        &Deadline::new(30000),
    )
    .unwrap();
    assert_eq!(retirement["unknownDependencyKinds"], serde_json::json!([]));
}
