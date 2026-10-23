use serde_json::{Value, json};
use std::{
    fs,
    io::Write,
    path::Path,
    process::{Command, Stdio},
};
use yoke::{
    Engine,
    capture::Deadline,
    git_workflows::{
        HostGit,
        repository_host::Host,
        repository_preferences::{self as preferences, Choice, Expected, Resolved},
        repository_scope::Options,
    },
    project_session::{HostProfile, HostProject, RECORD_KIND, Session},
    store::Store,
};
fn d() -> Deadline {
    Deadline::new(60000)
}
fn repository(root: &Path, git: &HostGit, file: &str) {
    fs::create_dir_all(root).unwrap();
    for args in [
        vec!["init", "--quiet", "--template="],
        vec!["add", "--", file],
        vec!["commit", "--quiet", "-m", "Owned source"],
    ] {
        if args[0] == "add" {
            fs::write(root.join(file), b"pub fn owned(){}\r\n").unwrap();
        }
        let output = Command::new(&git.executable)
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
fn engine(root: &Path, git: &HostGit, nested: bool, max_repositories: usize) -> Engine {
    let mut options = Options {
        allow_index_read: true,
        ..Default::default()
    };
    options.limits.max_repositories = max_repositories;
    Host::new(
        root,
        git.clone(),
        options,
        Resolved::current(root, None, Some(nested), &d()).unwrap(),
        &d(),
    )
    .unwrap()
    .open(&d())
    .unwrap()
}
fn record(store: &Store, id: &str) -> Value {
    store.get_record(RECORD_KIND, id).unwrap()
}

#[test]
fn session_resume_binds_stable_host_policy_instead_of_changing_membership() {
    let fixture = tempfile::tempdir().unwrap();
    let git = HostGit::discover().unwrap();
    let root = fixture.path().join("source π '");
    repository(&root, &git, "main.rs");
    let initial = engine(&root, &git, false, 64);
    let id = yoke::model::identity(&(RECORD_KIND, &initial.store.workspace_id, "owned-binding"));
    let mut session =
        Session::open(initial, HostProfile::default(), Some("owned-binding"), &d()).unwrap();
    let store = Store::open(&root).unwrap();
    let before = record(&store, &id);
    fs::write(root.join("new.rs"), b"pub fn new_member(){}\r\n").unwrap();
    let selected = session.active_engine().unwrap().index(&d()).unwrap().0;
    assert!(selected.files.iter().any(|f| f.path == "new.rs"));
    drop(session);
    let resumed = Session::open(
        engine(&root, &git, false, 64),
        HostProfile::default(),
        Some("owned-binding"),
        &d(),
    )
    .unwrap();
    drop(resumed);
    assert_eq!(record(&store, &id), before);
    let changed = Session::open(
        engine(&root, &git, false, 32),
        HostProfile::default(),
        Some("owned-binding"),
        &d(),
    )
    .err()
    .unwrap();
    assert!(
        changed.to_string().contains("Session host binding changed"),
        "{changed}"
    );
    assert_eq!(record(&store, &id), before);
    let mut alias = git.clone();
    let selected = git.executable.to_str().unwrap();
    let spelling = if let Some(rest) = selected.strip_prefix("\\\\?\\UNC\\") {
        format!("\\\\{rest}")
    } else if let Some(rest) = selected.strip_prefix("\\\\?\\") {
        rest.to_owned()
    } else {
        format!(
            "{}{}.{sep}{}",
            git.executable.parent().unwrap().display(),
            std::path::MAIN_SEPARATOR,
            git.executable.file_name().unwrap().to_str().unwrap(),
            sep = std::path::MAIN_SEPARATOR
        )
    };
    alias.executable = std::path::PathBuf::from(spelling);
    assert_ne!(alias.executable.as_os_str(), git.executable.as_os_str());
    let changed = Session::open(
        engine(&root, &alias, false, 64),
        HostProfile::default(),
        Some("owned-binding"),
        &d(),
    )
    .err()
    .unwrap();
    assert!(
        changed.to_string().contains("Session host binding changed"),
        "{changed}"
    );
    assert_eq!(record(&store, &id), before);
}

#[test]
fn persisted_state_and_changed_preferences_refuse_before_note_mutation() {
    let fixture = tempfile::tempdir().unwrap();
    let git = HostGit::discover().unwrap();
    let root = fixture.path().join("source");
    repository(&root, &git, "main.rs");
    let initial = engine(&root, &git, false, 64);
    let id = yoke::model::identity(&(RECORD_KIND, &initial.store.workspace_id, "owned-state"));
    let mut session =
        Session::open(initial, HostProfile::default(), Some("owned-state"), &d()).unwrap();
    session
        .invoke(
            "notes.write",
            &json!({"name":"owned","content":"original","paths":["main.rs"]}),
            &d(),
        )
        .unwrap();
    let store = Store::open(&root).unwrap();
    let original = record(&store, &id);
    let mut forged = original.clone();
    forged["revision"] = (forged["revision"].as_u64().unwrap() + 1).into();
    store
        .db
        .execute(
            "UPDATE records SET value=?1 WHERE kind=?2 AND id=?3",
            rusqlite::params![forged.to_string(), RECORD_KIND, id],
        )
        .unwrap();
    let error = session
        .invoke(
            "notes.write",
            &json!({"name":"owned","content":"must refuse"}),
            &d(),
        )
        .unwrap_err();
    assert!(error.to_string().contains("concurrently"), "{error}");
    assert_eq!(record(&store, &id), forged);
    store
        .db
        .execute(
            "UPDATE records SET value=?1 WHERE kind=?2 AND id=?3",
            rusqlite::params![original.to_string(), RECORD_KIND, id],
        )
        .unwrap();
    preferences::set(&store, &Expected::Absent, Choice::default(), true, &d()).unwrap();
    let error = session
        .invoke(
            "notes.write",
            &json!({"name":"owned","content":"must refuse again"}),
            &d(),
        )
        .unwrap_err();
    assert!(
        error.to_string().contains("repository_selection_stale"),
        "{error}"
    );
    assert_eq!(record(&store, &id), original);
    let mut restarted = Session::open(
        engine(&root, &git, false, 64),
        HostProfile::default(),
        Some("owned-read-note"),
        &d(),
    )
    .unwrap();
    let note = restarted
        .invoke("notes.read", &json!({"name":"owned"}), &d())
        .unwrap();
    assert!(note.to_string().contains("original"));
    assert!(!note.to_string().contains("must refuse"));
}

#[test]
fn fixed_root_session_rejects_foreign_registration_before_resolving_it() {
    let fixture = tempfile::tempdir().unwrap();
    let git = HostGit::discover().unwrap();
    let root = fixture.path().join("source");
    repository(&root, &git, "main.rs");
    let foreign = fixture.path().join("not-created-foreign");
    let profile = HostProfile {
        projects: vec![HostProject {
            name: "foreign".into(),
            root: foreign.clone(),
            read_only: true,
            query_only: false,
        }],
        ..Default::default()
    };
    let error = Session::open(
        engine(&root, &git, false, 64),
        profile.clone(),
        Some("refused-foreign"),
        &d(),
    )
    .err()
    .unwrap();
    assert!(error.to_string().contains("fixed-root session"), "{error}");
    assert!(!foreign.exists());
    fs::create_dir(&foreign).unwrap();
    fs::write(foreign.join("foreign.rs"), b"pub fn foreign_owned(){}\r\n").unwrap();
    let mut ordinary = Session::open(
        Engine::open(&root).unwrap(),
        profile,
        Some("ordinary-registration"),
        &d(),
    )
    .unwrap();
    ordinary
        .invoke("activate", &json!({"project":"foreign"}), &d())
        .unwrap();
    assert!(
        ordinary
            .active_engine()
            .unwrap()
            .index(&d())
            .unwrap()
            .0
            .files
            .iter()
            .any(|f| f.path == "foreign.rs")
    );
}

#[test]
fn selected_adapters_reuse_host_capture_and_keep_default_runtime_unavailable() {
    let fixture = tempfile::tempdir().unwrap();
    let git = HostGit::discover().unwrap();
    let root = fixture.path().join("source");
    repository(&root, &git, "main.rs");
    repository(&root.join("nested"), &git, "child.rs");
    let mut session = Session::open(
        engine(&root, &git, true, 64),
        HostProfile::default(),
        Some("adapters"),
        &d(),
    )
    .unwrap();
    let found = yoke::trail_adapter::invoke(
        &mut session,
        "trail_find_code",
        &json!({"query":"owned","in":"nested"}),
        &d(),
    )
    .unwrap();
    assert!(!found["items"].as_array().unwrap().is_empty(), "{found}");
    let graft = yoke::trail_adapter::invoke(
        &mut session,
        "graft_file_api",
        &json!({"file":"nested/child.rs"}),
        &d(),
    )
    .unwrap();
    assert!(!graft["items"].as_array().unwrap().is_empty());
    let read = yoke::compatibility::invoke_serena(
        &mut session,
        "read_file",
        &json!({"relative_path":"nested/child.rs"}),
        &d(),
    )
    .unwrap();
    assert!(read.to_string().contains("owned"), "{read}");
    let (mime, body) =
        yoke::graphify_adapter::resource(&mut session, "graphify://stats", &d()).unwrap();
    assert!(!mime.is_empty());
    assert!(!body.is_empty());
    let current = session.active_engine().unwrap();
    assert!(
        current
            .capture_options
            .repository_scope
            .as_ref()
            .unwrap()
            .read_only_roots
            .contains(&"nested".to_owned())
    );
    assert!(!current.allow_writes);
    assert!(
        yoke::compatibility::invoke_serena(
            &mut session,
            "execute_shell_command",
            &json!({"command":"owned-must-not-start"}),
            &d()
        )
        .is_err()
    );
    assert!(
        session
            .invoke("activate", &json!({"project":"foreign"}), &d())
            .is_err()
    );
}

#[test]
fn cli_selects_only_explicit_fixed_root_profiles_without_protocol_summary_noise() {
    let fixture = tempfile::tempdir().unwrap();
    let git = HostGit::discover().unwrap();
    let root = fixture.path().join("source");
    repository(&root, &git, "main.rs");
    for (profile, count) in [
        (None, 8),
        (Some("native"), 8),
        (Some("session"), 9),
        (Some("serena"), 53),
        (Some("trail"), 6),
        (Some("graft"), 6),
        (Some("graphify"), 10),
    ] {
        let mut command = Command::new(env!("CARGO_BIN_EXE_yoke"));
        command
            .arg("--root")
            .arg(&root)
            .args([
                "git",
                "repository-scope",
                "--capture",
                "--serve",
                "--allow-index-read",
                "--trust-git",
            ])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        if let Some(profile) = profile {
            command.args(["--profile", profile]);
        }
        let mut process = command.spawn().unwrap();
        let mut input = process.stdin.take().unwrap();
        for message in [
            json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25"}}),
            json!({"jsonrpc":"2.0","method":"notifications/initialized"}),
            json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}),
        ] {
            writeln!(input, "{message}").unwrap();
        }
        drop(input);
        let output = process.wait_with_output().unwrap();
        assert!(
            output.status.success(),
            "{profile:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let rows = String::from_utf8(output.stdout)
            .unwrap()
            .lines()
            .map(|s| serde_json::from_str::<Value>(s).unwrap())
            .collect::<Vec<_>>();
        assert_eq!(rows.len(), 2, "{profile:?}: {rows:?}");
        assert_eq!(rows[0]["id"], 1);
        assert_eq!(
            rows[1]["result"]["tools"].as_array().unwrap().len(),
            count,
            "{profile:?}"
        );
    }
    let cold = fixture.path().join("cold");
    fs::create_dir(&cold).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_yoke"))
        .arg("--root")
        .arg(&cold)
        .args(["git", "repository-scope", "--capture", "--profile", "trail"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    assert!(!cold.join(".yoke").exists());
}
