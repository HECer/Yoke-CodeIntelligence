use serde_json::Value;
use std::{fs, path::Path, process::Command};
use yoke::{
    capture::Deadline,
    git_workflows::{
        HostGit,
        repository_preferences::{self as preferences, Choice, Expected, Resolved},
        repository_scope,
    },
    store::Store,
};

fn d() -> Deadline {
    Deadline::new(30000)
}
fn command(root: &Path, args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_yoke"))
        .arg("--root")
        .arg(root)
        .args(args)
        .output()
        .unwrap()
}
fn git(host: &HostGit, root: &Path, args: &[&str]) {
    let output = Command::new(&host.executable)
        .arg("-C")
        .arg(root)
        .args([
            "-c",
            "core.hooksPath=",
            "-c",
            "user.name=Owned fixture",
            "-c",
            "user.email=owned@example.invalid",
            "-c",
            "commit.gpgSign=false",
        ])
        .args(args)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{:?}: {}",
        args,
        String::from_utf8_lossy(&output.stderr)
    );
}
fn repository(root: &Path, host: &HostGit, name: &str) {
    fs::create_dir_all(root).unwrap();
    git(host, root, &["init", "--quiet", "--template="]);
    fs::write(root.join(name), "pub fn owned() { let π = 1; }\r\n").unwrap();
    git(host, root, &["add", "--", name]);
    git(host, root, &["commit", "--quiet", "-m", "Owned"]);
}

#[test]
fn cold_show_and_refused_cli_controls_create_no_state_or_source() {
    let root = tempfile::tempdir().unwrap();
    assert!(preferences::show(root.path(), &d()).unwrap().is_none());
    assert!(!root.path().join(".yoke").exists());
    let shown = command(root.path(), &["git", "repository-preferences", "show"]);
    assert!(shown.status.success());
    assert_eq!(
        serde_json::from_slice::<Value>(&shown.stdout).unwrap()["selection"],
        Value::Null
    );
    for args in [
        vec![
            "git",
            "repository-preferences",
            "set",
            "--expect",
            "absent",
            "--follow-submodules",
            "true",
        ],
        vec![
            "git",
            "repository-preferences",
            "set",
            "--allow-host-writes",
            "--expect",
            "bad",
            "--follow-submodules",
            "true",
        ],
        vec![
            "git",
            "repository-preferences",
            "set",
            "--allow-host-writes",
            "--expect",
            "absent",
        ],
        vec![
            "git",
            "repository-preferences",
            "clear",
            "--allow-host-writes",
            "--expect",
            "absent",
        ],
    ] {
        let output = command(root.path(), &args);
        if args[2] != "clear" {
            assert!(!output.status.success());
            assert!(output.stdout.is_empty());
        } else {
            assert!(output.status.success());
        }
        assert!(!root.path().join(".yoke").exists());
    }
}

#[test]
fn cas_lifecycle_preserves_other_state_and_rejects_stale_ids() {
    let root = tempfile::tempdir().unwrap();
    let store = Store::open(root.path()).unwrap();
    store
        .db
        .execute(
            "INSERT INTO meta(key,value) VALUES('owned-unrelated','preserve')",
            [],
        )
        .unwrap();
    let choice = Choice {
        follow_submodules: true,
        follow_nested_repositories: false,
    };
    let first = preferences::set(&store, &Expected::Absent, choice.clone(), true, &d()).unwrap();
    assert_eq!(first.revision, 1);
    let before = store.db.total_changes();
    assert!(preferences::set(&store, &Expected::Absent, Choice::default(), true, &d()).is_err());
    assert_eq!(store.db.total_changes(), before);
    assert!(
        preferences::set(
            &store,
            &Expected::Id(first.id.clone()),
            Choice::default(),
            false,
            &d()
        )
        .is_err()
    );
    assert_eq!(store.db.total_changes(), before);
    let second = preferences::set(
        &store,
        &Expected::Id(first.id.clone()),
        Choice::default(),
        true,
        &d(),
    )
    .unwrap();
    assert_eq!(second.revision, 2);
    assert_eq!(second.incarnation, first.incarnation);
    assert_ne!(second.id, first.id);
    let before = store.db.total_changes();
    assert!(preferences::clear(&store, &Expected::Id(first.id), true, &d()).is_err());
    assert_eq!(store.db.total_changes(), before);
    preferences::clear(&store, &Expected::Id(second.id), true, &d()).unwrap();
    assert!(preferences::get(&store, &d()).unwrap().is_none());
    assert_eq!(
        store
            .db
            .query_row(
                "SELECT value FROM meta WHERE key='owned-unrelated'",
                [],
                |r| r.get::<_, String>(0)
            )
            .unwrap(),
        "preserve"
    );
    assert_eq!(
        store
            .db
            .query_row("SELECT count(*) FROM snapshots", [], |r| r
                .get::<_, usize>(0))
            .unwrap(),
        0
    );
}

#[test]
fn clear_and_recreate_cannot_revalidate_an_old_expected_identity() {
    let root = tempfile::tempdir().unwrap();
    let store = Store::open(root.path()).unwrap();
    let first = preferences::set(&store, &Expected::Absent, Choice::default(), true, &d()).unwrap();
    preferences::clear(&store, &Expected::Id(first.id.clone()), true, &d()).unwrap();
    let second =
        preferences::set(&store, &Expected::Absent, Choice::default(), true, &d()).unwrap();
    assert_ne!(second.incarnation, first.incarnation);
    assert_ne!(second.id, first.id);
    let before = store.db.total_changes();
    assert!(
        preferences::set(
            &store,
            &Expected::Id(first.id),
            Choice::default(),
            true,
            &d()
        )
        .is_err()
    );
    assert_eq!(store.db.total_changes(), before);
}

#[test]
fn competing_connections_with_one_expected_id_have_exactly_one_winner() {
    let root = tempfile::tempdir().unwrap();
    let store = Store::open(root.path()).unwrap();
    let first = preferences::set(&store, &Expected::Absent, Choice::default(), true, &d()).unwrap();
    let connections = [
        Store::open(root.path()).unwrap(),
        Store::open(root.path()).unwrap(),
    ];
    let barrier = std::sync::Barrier::new(2);
    let outcomes = std::thread::scope(|scope| {
        let handles: Vec<_> = connections
            .into_iter()
            .enumerate()
            .map(|(index, connection)| {
                let barrier = &barrier;
                let expected = Expected::Id(first.id.clone());
                scope.spawn(move || {
                    barrier.wait();
                    preferences::set(
                        &connection,
                        &expected,
                        Choice {
                            follow_submodules: index == 0,
                            follow_nested_repositories: index == 1,
                        },
                        true,
                        &d(),
                    )
                })
            })
            .collect();
        handles
            .into_iter()
            .map(|handle| handle.join().unwrap())
            .collect::<Vec<_>>()
    });
    assert_eq!(outcomes.iter().filter(|result| result.is_ok()).count(), 1);
    let winner = outcomes
        .iter()
        .find_map(|result| result.as_ref().ok())
        .unwrap();
    let loser = outcomes
        .iter()
        .find_map(|result| result.as_ref().err())
        .unwrap();
    assert!(
        loser.to_string().contains("repository_selection_stale"),
        "{loser}"
    );
    assert_eq!(winner.revision, 2);
    assert_eq!(
        preferences::get(&store, &d()).unwrap().as_ref(),
        Some(winner)
    );
}

#[test]
fn selection_appearing_after_absent_resolution_refuses_capture_publication() {
    let fixture = tempfile::tempdir().unwrap();
    let host = HostGit::discover().unwrap();
    let root = fixture.path().join("parent");
    repository(&root, &host, "main.rs");
    let resolved = Resolved::current(&root, None, None, &d()).unwrap();
    assert!(!root.join(".yoke").exists());
    let store = Store::open(&root).unwrap();
    preferences::set(&store, &Expected::Absent, Choice::default(), true, &d()).unwrap();
    let options = repository_scope::Options {
        allow_index_read: true,
        ..Default::default()
    };
    let error = repository_scope::capture_root_resolved(&root, &host, &options, &resolved, &d())
        .unwrap_err();
    assert!(
        error.to_string().contains("repository_selection_stale"),
        "{error}"
    );
    assert_eq!(
        store
            .db
            .query_row("SELECT count(*) FROM snapshots", [], |row| row
                .get::<_, usize>(0))
            .unwrap(),
        0
    );
    assert_eq!(
        store
            .db
            .query_row("SELECT count(*) FROM graphs", [], |row| row
                .get::<_, usize>(0))
            .unwrap(),
        0
    );
}

#[test]
fn malformed_foreign_oversized_and_expired_control_data_fail_closed() {
    let root = tempfile::tempdir().unwrap();
    let store = Store::open(root.path()).unwrap();
    let selected =
        preferences::set(&store, &Expected::Absent, Choice::default(), true, &d()).unwrap();
    let base = serde_json::to_value(&selected).unwrap();
    for case in 0..5 {
        let mut value = base.clone();
        match case {
            0 => value["root"] = "C:/unavailable-foreign-root".into(),
            1 => value["workspaceId"] = "a".repeat(64).into(),
            2 => value["schemaVersion"] = 99.into(),
            3 => value["trustGit"] = true.into(),
            4 => value["id"] = "b".repeat(64).into(),
            _ => unreachable!(),
        };
        store
            .db
            .execute(
                "UPDATE meta SET value=?1 WHERE key='host_git_repository_selection_v1'",
                [value.to_string()],
            )
            .unwrap();
        let before = store.db.total_changes();
        assert!(preferences::get(&store, &d()).is_err());
        assert_eq!(store.db.total_changes(), before);
    }
    for raw in ["x".repeat(20_000), "{\"id\":1,\"id\":2}".into()] {
        store
            .db
            .execute(
                "UPDATE meta SET value=?1 WHERE key='host_git_repository_selection_v1'",
                [raw],
            )
            .unwrap();
        assert!(preferences::get(&store, &d()).is_err());
    }
    store
        .db
        .execute(
            "UPDATE meta SET value=?1 WHERE key='host_git_repository_selection_v1'",
            [base.to_string()],
        )
        .unwrap();
    assert_eq!(preferences::get(&store, &d()).unwrap(), Some(selected));
    let before = store.db.total_changes();
    assert!(
        preferences::set(
            &store,
            &Expected::parse(base["id"].as_str().unwrap()).unwrap(),
            Choice::default(),
            true,
            &Deadline::new(0)
        )
        .is_err()
    );
    assert_eq!(store.db.total_changes(), before);
}

#[test]
fn changed_selection_is_refused_before_capture_proof_publication() {
    let fixture = tempfile::tempdir().unwrap();
    let host = HostGit::discover().unwrap();
    let root = fixture.path().join("parent π '");
    repository(&root, &host, "main.rs");
    let store = Store::open(&root).unwrap();
    let first = preferences::set(&store, &Expected::Absent, Choice::default(), true, &d()).unwrap();
    let resolved = Resolved::current(&root, None, None, &d()).unwrap();
    preferences::set(
        &store,
        &Expected::Id(first.id),
        Choice {
            follow_submodules: true,
            follow_nested_repositories: true,
        },
        true,
        &d(),
    )
    .unwrap();
    let count = store
        .db
        .query_row("SELECT count(*) FROM snapshots", [], |r| {
            r.get::<_, usize>(0)
        })
        .unwrap();
    let options = repository_scope::Options {
        allow_index_read: true,
        ..Default::default()
    };
    let error = repository_scope::capture_root_resolved(&root, &host, &options, &resolved, &d())
        .unwrap_err();
    assert!(
        error.to_string().contains("repository_selection_stale"),
        "{error}"
    );
    assert_eq!(
        store
            .db
            .query_row("SELECT count(*) FROM snapshots", [], |r| r
                .get::<_, usize>(0))
            .unwrap(),
        count
    );
}

#[test]
fn cli_reuses_opted_in_choices_only_for_explicit_trusted_capture_and_supports_disable_override() {
    let fixture = tempfile::tempdir().unwrap();
    let host = HostGit::discover().unwrap();
    let root = fixture.path().join("parent");
    repository(&root, &host, "main.rs");
    repository(&root.join("nested"), &host, "child.rs");
    let set = command(
        &root,
        &[
            "git",
            "repository-preferences",
            "set",
            "--allow-host-writes",
            "--expect",
            "absent",
            "--follow-nested-repositories",
            "true",
        ],
    );
    assert!(
        set.status.success(),
        "{}",
        String::from_utf8_lossy(&set.stderr)
    );
    let id = serde_json::from_slice::<Value>(&set.stdout).unwrap()["selection"]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let denied = command(&root, &["git", "repository-scope", "--capture"]);
    assert!(!denied.status.success());
    let capture = |extra: &[&str]| {
        let mut args = vec![
            "git",
            "repository-scope",
            "--capture",
            "--allow-index-read",
            "--trust-git",
        ];
        args.extend_from_slice(extra);
        let output = command(&root, &args);
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        serde_json::from_slice::<Value>(&output.stdout).unwrap()
    };
    let selected = capture(&[]);
    assert_eq!(selected["hostSelection"]["id"], id);
    let excluded = capture(&["--no-follow-nested-repositories"]);
    assert_ne!(selected["snapshotId"], excluded["snapshotId"]);
    let engine = yoke::Engine::open(&root).unwrap();
    let yes = engine
        .store
        .snapshot(selected["snapshotId"].as_str().unwrap())
        .unwrap();
    let no = engine
        .store
        .snapshot(excluded["snapshotId"].as_str().unwrap())
        .unwrap();
    assert!(yes.files.iter().any(|f| f.path == "nested/child.rs"));
    assert!(!no.files.iter().any(|f| f.path == "nested/child.rs"));
    assert_eq!(
        preferences::get(&engine.store, &d()).unwrap().unwrap().id,
        id
    );
    drop(engine);
    assert_eq!(capture(&[])["snapshotId"], selected["snapshotId"]);
    let metadata = command(
        &root,
        &[
            "git",
            "repository-scope",
            "--allow-index-read",
            "--trust-git",
        ],
    );
    assert!(metadata.status.success());
    assert_eq!(
        serde_json::from_slice::<Value>(&metadata.stdout).unwrap()["observation"]["followNestedRepositories"],
        false
    );
}
