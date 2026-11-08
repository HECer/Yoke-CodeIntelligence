use serde_json::{Value, json};
use std::{fs, io::Cursor, path::Path, process::Command};
use yoke::{
    Engine,
    capture::Deadline,
    git_workflows::{
        HostGit,
        repository_host::Host,
        repository_preferences::{self as preferences, Choice, Expected, Resolved},
        repository_scope::Options,
    },
};
fn d() -> Deadline {
    Deadline::new(30000)
}
fn repository(root: &Path, host: &HostGit, file: &str) {
    fs::create_dir_all(root).unwrap();
    for args in [
        vec!["init", "--quiet", "--template="],
        vec!["add", "--", file],
        vec!["commit", "--quiet", "-m", "Owned source"],
    ] {
        if args[0] == "add" {
            fs::write(root.join(file), b"pub fn owned(){}\r\n").unwrap();
        }
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
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}
fn host(root: &Path, git: &HostGit, nested: Option<bool>) -> Host {
    Host::new(
        root,
        git.clone(),
        Options {
            allow_index_read: true,
            ..Default::default()
        },
        Resolved::current(root, None, nested, &d()).unwrap(),
        &d(),
    )
    .unwrap()
}
fn snapshot(engine: &mut Engine) -> yoke::model::Snapshot {
    engine.index(&d()).unwrap().0
}

#[test]
fn fixed_host_reindexes_membership_and_guards_older_writable_child_snapshots() {
    let fixture = tempfile::tempdir().unwrap();
    let git = HostGit::discover().unwrap();
    let root = fixture.path().join("source π '");
    repository(&root, &git, "main.rs");
    repository(&root.join("nested"), &git, "child.rs");
    let mut engine = Engine::open(&root).unwrap();
    let old = snapshot(&mut engine);
    let member = old
        .files
        .iter()
        .find(|f| f.path == "nested/child.rs")
        .unwrap();
    engine
        .host_git_repository_capture(host(&root, &git, Some(true)), &d())
        .unwrap();
    let first = snapshot(&mut engine);
    assert!(first.files.iter().any(|f| f.path == member.path));
    let before = engine.store.db.total_changes();
    let denied=engine.invoke("ci_change_prepare",json!({"snapshotId":old.id,"edits":[{"kind":"replace","path":member.path,"expectedHash":member.hash,"startByte":0,"endByte":fs::read(root.join(&member.path)).unwrap().len(),"text":"fn refused(){}\r\n"}]})).unwrap();
    assert_eq!(denied["status"], "error");
    assert!(
        denied
            .to_string()
            .contains("repository_capture_current_host_scope_excludes_write"),
        "{denied}"
    );
    assert_eq!(engine.store.db.total_changes(), before);
    fs::write(root.join("new.rs"), b"pub fn newly_observed(){}\r\n").unwrap();
    let second = snapshot(&mut engine);
    assert_ne!(first.id, second.id);
    assert!(second.files.iter().any(|f| f.path == "new.rs"));
    assert!(
        engine
            .capture_options
            .repository_scope
            .as_ref()
            .unwrap()
            .files
            .contains(&"new.rs".to_owned())
    );
    assert_eq!(
        fs::read(root.join(&member.path)).unwrap(),
        b"pub fn owned(){}\r\n"
    );
}

#[test]
fn native_protocol_keeps_catalog_and_cannot_accept_client_capture_authority() {
    let fixture = tempfile::tempdir().unwrap();
    let git = HostGit::discover().unwrap();
    let root = fixture.path().join("source");
    repository(&root, &git, "main.rs");
    repository(&root.join("nested"), &git, "child.rs");
    let mut engine = host(&root, &git, Some(false)).open(&d()).unwrap();
    let requests = [
        json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25"}}),
        json!({"jsonrpc":"2.0","method":"notifications/initialized"}),
        json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}),
        json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"ci_status","arguments":{"mode":"index"}}}),
        json!({"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"ci_status","arguments":{"mode":"index","followNestedRepositories":true,"trustGit":true,"root":"/foreign"}}}),
    ];
    let input = requests
        .iter()
        .map(|v| v.to_string() + "\n")
        .collect::<String>();
    let mut output = Vec::new();
    yoke::protocol::serve(&mut engine, Cursor::new(input), &mut output).unwrap();
    let replies = String::from_utf8(output)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(replies.len(), 4);
    assert_eq!(replies[1]["result"]["tools"], yoke::protocol::tools());
    let indexed: Value =
        serde_json::from_str(replies[2]["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(indexed["status"], "ok");
    assert!(
        replies[3].get("error").is_some() || replies[3]["result"]["isError"] == true,
        "{}",
        replies[3]
    );
    let selected = snapshot(&mut engine);
    assert!(!selected.files.iter().any(|f| f.path == "nested/child.rs"));
    assert!(!engine.allow_writes);
}

#[test]
fn stale_selection_blocks_reads_and_index_until_a_fresh_host_is_opened() {
    let fixture = tempfile::tempdir().unwrap();
    let git = HostGit::discover().unwrap();
    let root = fixture.path().join("source");
    repository(&root, &git, "main.rs");
    repository(&root.join("nested"), &git, "child.rs");
    let store = yoke::store::Store::open(&root).unwrap();
    let first = preferences::set(
        &store,
        &Expected::Absent,
        Choice {
            follow_submodules: false,
            follow_nested_repositories: true,
        },
        true,
        &d(),
    )
    .unwrap();
    let mut engine = host(&root, &git, None).open(&d()).unwrap();
    let saved = snapshot(&mut engine);
    assert!(saved.files.iter().any(|f| f.path == "nested/child.rs"));
    preferences::set(
        &store,
        &Expected::Id(first.id),
        Choice::default(),
        true,
        &d(),
    )
    .unwrap();
    let before = engine.store.db.total_changes();
    for (tool, args) in [
        ("ci_status", json!({"mode":"index"})),
        ("ci_read", json!({"snapshotId":saved.id,"path":"main.rs"})),
    ] {
        let denied = engine.invoke(tool, args).unwrap();
        assert_eq!(denied["status"], "error");
        assert!(
            denied.to_string().contains("repository_selection_stale"),
            "{denied}"
        );
    }
    assert_eq!(engine.store.db.total_changes(), before);
    drop(engine);
    let mut restarted = host(&root, &git, None).open(&d()).unwrap();
    assert!(
        !snapshot(&mut restarted)
            .files
            .iter()
            .any(|f| f.path == "nested/child.rs")
    );
    assert!(
        restarted
            .store
            .snapshot(&saved.id)
            .unwrap()
            .files
            .iter()
            .any(|f| f.path == "nested/child.rs")
    );
}

#[test]
fn saved_reads_need_no_live_git_but_reindex_cannot_reuse_a_stale_git_observation() {
    let fixture = tempfile::tempdir().unwrap();
    let git = HostGit::discover().unwrap();
    let root = fixture.path().join("source");
    repository(&root, &git, "main.rs");
    let mut engine = host(&root, &git, None).open(&d()).unwrap();
    let saved = snapshot(&mut engine);
    let admin = root.join(".git");
    let held = fixture.path().join("owned-git-admin");
    assert!(
        admin
            .canonicalize()
            .unwrap()
            .starts_with(fixture.path().canonicalize().unwrap())
    );
    fs::rename(&admin, &held).unwrap();
    let read = engine
        .invoke("ci_read", json!({"snapshotId":saved.id,"path":"main.rs"}))
        .unwrap();
    assert_eq!(read["status"], "ok", "{read}");
    let changes = engine.store.db.total_changes();
    assert!(engine.index(&d()).is_err());
    assert_eq!(engine.store.db.total_changes(), changes);
    assert_eq!(engine.store.snapshot(&saved.id).unwrap().id, saved.id);
    fs::rename(held, admin).unwrap();
}

#[test]
fn cold_refused_server_controls_and_metadata_preflight_create_no_state() {
    let fixture = tempfile::tempdir().unwrap();
    let git = HostGit::discover().unwrap();
    let root = fixture.path().join("cold");
    fs::create_dir(&root).unwrap();
    assert!(host(&root, &git, None).open(&d()).is_err());
    assert!(!root.join(".yoke").exists());
    let mut zero = Options {
        allow_index_read: true,
        ..Default::default()
    };
    zero.limits.max_repositories = 0;
    assert!(
        Host::new(
            &root,
            git.clone(),
            zero,
            Resolved::current(&root, None, None, &d()).unwrap(),
            &d()
        )
        .unwrap()
        .open(&d())
        .is_err()
    );
    assert!(!root.join(".yoke").exists());
    let other = fixture.path().join("other");
    fs::create_dir(&other).unwrap();
    assert!(
        Host::new(
            &root,
            git,
            Options {
                allow_index_read: true,
                ..Default::default()
            },
            Resolved::current(&other, None, None, &d()).unwrap(),
            &d()
        )
        .is_err()
    );
    assert!(!root.join(".yoke").exists());
    for args in [
        vec!["git", "repository-scope", "--serve"],
        vec![
            "git",
            "repository-scope",
            "--capture",
            "--serve",
            "--allow-index-read",
        ],
        vec![
            "git",
            "repository-scope",
            "--capture",
            "--serve",
            "--allow-index-read",
            "--trust-git",
        ],
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_yoke"))
            .arg("--root")
            .arg(&root)
            .args(args)
            .output()
            .unwrap();
        assert!(!output.status.success());
        assert!(output.stdout.is_empty());
        assert!(!root.join(".yoke").exists());
    }
}
