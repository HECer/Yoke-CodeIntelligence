use serde_json::{Value, json};
use std::{
    fs,
    io::Write,
    path::Path,
    process::{Command, Output, Stdio},
};
use yoke::{
    capture::Deadline,
    git_workflows::{
        HostGit,
        repository_host::Host,
        repository_preferences::{self, Choice, Expected, Resolved},
        repository_scope::Options,
    },
    project_session::RECORD_KIND,
};
fn d() -> Deadline {
    Deadline::new(60_000)
}
fn repository(root: &Path, git: &HostGit, files: &[&str]) {
    fs::create_dir_all(root).unwrap();
    let run = |args: &[&str]| {
        let result = Command::new(&git.executable)
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
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
    };
    run(&["init", "--quiet", "--template="]);
    for file in files {
        let path = root.join(file);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, b"pub fn owned_source(){}\r\n").unwrap();
        run(&["add", "--", file]);
    }
    run(&["commit", "--quiet", "-m", "Owned source"]);
}
fn write_json(path: &Path, value: Value) {
    fs::write(path, serde_json::to_vec(&value).unwrap()).unwrap();
}
fn launch(root: &Path, globals: &[String], arguments: &[String], calls: Vec<Value>) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_yoke"))
        .arg("--root")
        .arg(root)
        .args(["--timeout", "60000"])
        .args(globals)
        .arg("serve-session")
        .args(arguments)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut input = child.stdin.take().unwrap();
    if !calls.is_empty() {
        let mut messages = vec![
            json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25"}}),
            json!({"jsonrpc":"2.0","method":"notifications/initialized"}),
            json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}),
        ];
        messages.extend(calls.into_iter().enumerate().map(|(index,call)|json!({"jsonrpc":"2.0","id":index+3,"method":"tools/call","params":call})));
        for message in messages {
            writeln!(input, "{message}").unwrap();
        }
    }
    drop(input);
    child.wait_with_output().unwrap()
}
fn replies(output: &Output, calls: usize, count: usize) -> Vec<Value> {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let replies: Vec<Value> = String::from_utf8(output.stdout.clone())
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(replies.len(), calls + 2, "Unsolicited protocol output");
    assert_eq!(replies[0]["result"]["protocolVersion"], "2025-11-25");
    assert_eq!(
        replies[1]["result"]["tools"].as_array().unwrap().len(),
        count
    );
    for (index, reply) in replies.iter().enumerate() {
        assert_eq!(reply["id"], index + 1);
        assert!(reply.get("error").is_none(), "{reply}");
    }
    replies
}
fn payload(reply: &Value) -> Value {
    assert_ne!(reply["result"]["isError"], true);
    serde_json::from_str(reply["result"]["content"][0]["text"].as_str().unwrap()).unwrap()
}
fn arguments(profile: &str, selection: &Path, host: Option<&Path>, id: &str) -> Vec<String> {
    let mut result = vec![
        "--profile".into(),
        profile.into(),
        "--id".into(),
        id.into(),
        "--git-projects".into(),
        selection.display().to_string(),
        "--allow-git-index-read".into(),
        "--trust-git".into(),
    ];
    if let Some(host) = host {
        result.extend(["--host-profile".into(), host.display().to_string()]);
    }
    result
}
fn snapshots(root: &Path) -> Vec<Value> {
    let db = rusqlite::Connection::open_with_flags(
        root.join(".yoke/index.sqlite"),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .unwrap();
    let mut statement = db
        .prepare("SELECT value FROM snapshots ORDER BY id")
        .unwrap();
    statement
        .query_map([], |row| row.get::<_, String>(0))
        .unwrap()
        .map(|row| serde_json::from_str(&row.unwrap()).unwrap())
        .collect()
}
fn record(root: &Path, label: &str) -> Value {
    let store = yoke::store::Store::open(root).unwrap();
    let id = yoke::model::identity(&(RECORD_KIND, &store.workspace_id, label));
    store.get_record(RECORD_KIND, &id).unwrap()
}

#[test]
fn selection_permissions_schema_snapshot_and_runtime_admission_precede_cold_state() {
    for (selection, flags, globals, expected) in [
        (
            Some(json!({"default":{}})),
            vec![],
            vec![],
            "requires --allow-git-index-read and --trust-git",
        ),
        (
            Some(json!({"default":{}})),
            vec!["--trust-git"],
            vec![],
            "requires --allow-git-index-read and --trust-git",
        ),
        (
            Some(json!({"default":{}})),
            vec!["--allow-git-index-read"],
            vec![],
            "requires --allow-git-index-read and --trust-git",
        ),
        (
            None,
            vec!["--trust-git"],
            vec![],
            "Git permissions require --git-projects",
        ),
        (
            Some(json!({"default":{"trustGit":true}})),
            vec![],
            vec![],
            "Invalid host input schema",
        ),
        (
            Some(json!({"default":{},"foreign":{}})),
            vec!["--trust-git", "--allow-git-index-read"],
            vec![],
            "exactly default and every registered project",
        ),
        (
            Some(json!({"default":{}})),
            vec!["--trust-git", "--allow-git-index-read"],
            vec![
                "--snapshot",
                "0000000000000000000000000000000000000000000000000000000000000000",
            ],
            "select their own source snapshots",
        ),
        (
            Some(json!({"default":{}})),
            vec!["--trust-git", "--allow-git-index-read", "--trust-python"],
            vec![],
            "Python runtime options require an explicit Python executable",
        ),
    ] {
        let fixture = tempfile::tempdir().unwrap();
        let source = fixture.path().join("source");
        fs::create_dir(&source).unwrap();
        let file = fixture.path().join("selected.json");
        let mut args: Vec<String> = flags.into_iter().map(String::from).collect();
        if let Some(selection) = selection {
            write_json(&file, selection);
            args.extend(["--git-projects".into(), file.display().to_string()]);
        }
        let output = launch(
            &source,
            &globals.into_iter().map(String::from).collect::<Vec<_>>(),
            &args,
            vec![],
        );
        assert!(!output.status.success());
        assert!(output.stdout.is_empty());
        assert!(
            String::from_utf8_lossy(&output.stderr).contains(expected),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(!source.join(".yoke").exists());
    }
}

#[test]
fn unknown_registration_and_primary_metadata_failure_leave_no_source_store() {
    let fixture = tempfile::tempdir().unwrap();
    let source = fixture.path().join("source");
    fs::create_dir(&source).unwrap();
    let missing = fixture.path().join("never-resolved");
    let host = fixture.path().join("host.json");
    let selected = fixture.path().join("selection.json");
    write_json(
        &host,
        json!({"projects":[{"name":"secondary","root":missing,"readOnly":true,"queryOnly":true}]}),
    );
    write_json(&selected, json!({"default":{}}));
    let output = launch(
        &source,
        &[],
        &arguments("native", &selected, Some(&host), "invalid-selection"),
        vec![],
    );
    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("exactly default and every registered project")
    );
    assert!(!source.join(".yoke").exists() && !missing.exists());
    let output = launch(
        &source,
        &[],
        &arguments("native", &selected, None, "invalid-git"),
        vec![],
    );
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    assert!(!source.join(".yoke").exists());
}

#[test]
fn all_stdio_profiles_capture_selected_secondary_with_source_policy_and_exact_protocol_output() {
    let fixture = tempfile::tempdir().unwrap();
    let git = HostGit::discover().unwrap();
    let a = fixture.path().join("primary \u{3c0}");
    let b = fixture.path().join("secondary \u{df}");
    repository(&a, &git, &["main.rs"]);
    repository(&b, &git, &["main.rs", "excluded.rs"]);
    repository(&b.join("nested \u{df}"), &git, &["child.rs"]);
    fs::write(
        b.join("nested \u{df}/child.rs"),
        b"pub fn child_owned(){}\r\n",
    )
    .unwrap();
    let host = fixture.path().join("host.json");
    let selected = fixture.path().join("selected.json");
    write_json(
        &host,
        json!({"startupProject":"secondary","projects":[{"name":"secondary","root":b,"readOnly":true,"queryOnly":false}],"sourceExclusions":{"global":[["excluded.rs"]]}}),
    );
    write_json(
        &selected,
        json!({"default":{},"secondary":{"followNestedRepositories":true}}),
    );
    for (profile, count, call) in [
        (
            "native",
            9,
            json!({"name":"ci_read","arguments":{"path":"nested \u{df}/child.rs"}}),
        ),
        (
            "serena",
            53,
            json!({"name":"read_file","arguments":{"relative_path":"nested \u{df}/child.rs"}}),
        ),
        (
            "trail",
            6,
            json!({"name":"trail_find_code","arguments":{"query":"child_owned","in":"nested \u{df}"}}),
        ),
        (
            "graft",
            6,
            json!({"name":"graft_file_api","arguments":{"file":"nested \u{df}/child.rs"}}),
        ),
        (
            "graphify",
            10,
            json!({"name":"get_node","arguments":{"label":"child_owned","project_path":b}}),
        ),
    ] {
        let output = launch(
            &a,
            &[],
            &arguments(profile, &selected, Some(&host), &format!("owned-{profile}")),
            vec![call],
        );
        let replies = replies(&output, 1, count);
        let value = payload(&replies[2]);
        assert_eq!(value["status"], "ok");
        assert!(value.to_string().contains("child_owned"));
        assert!(snapshots(&a).is_empty());
        let captures = snapshots(&b);
        assert_eq!(captures.len(), 1);
        assert_eq!(
            captures[0]["files"]
                .as_array()
                .unwrap()
                .iter()
                .map(|f| f["path"].as_str().unwrap())
                .collect::<Vec<_>>(),
            ["main.rs", "nested \u{df}/child.rs"]
        );
    }
}

#[test]
fn primary_capture_filters_and_secondary_host_exclusions_apply_before_first_publication() {
    let fixture = tempfile::tempdir().unwrap();
    let git = HostGit::discover().unwrap();
    let source = fixture.path().join("source");
    repository(
        &source,
        &git,
        &["allowed/main.rs", "outside.rs", "allowed/excluded.rs"],
    );
    let selected = fixture.path().join("selection.json");
    let capture = fixture.path().join("capture.json");
    let host = fixture.path().join("host.json");
    write_json(&selected, json!({"default":{}}));
    write_json(&capture, json!({"onlyDirs":["allowed"]}));
    write_json(
        &host,
        json!({"sourceExclusions":{"global":[["allowed/excluded.rs"]]}}),
    );
    let output = launch(
        &source,
        &["--capture-config".into(), capture.display().to_string()],
        &arguments("native", &selected, Some(&host), "filtered"),
        vec![json!({"name":"ci_read","arguments":{"path":"allowed/main.rs"}})],
    );
    let replies = replies(&output, 1, 9);
    assert_eq!(payload(&replies[2])["status"], "ok");
    let captures = snapshots(&source);
    assert_eq!(captures.len(), 1);
    assert_eq!(
        captures[0]["files"]
            .as_array()
            .unwrap()
            .iter()
            .map(|f| f["path"].as_str().unwrap())
            .collect::<Vec<_>>(),
        ["allowed/main.rs"]
    );
}

#[test]
fn imported_scope_and_untrusted_server_config_refuse_before_new_target_state() {
    let fixture = tempfile::tempdir().unwrap();
    let git = HostGit::discover().unwrap();
    let a = fixture.path().join("proof-source");
    let b = fixture.path().join("fresh-target");
    repository(&a, &git, &["main.rs"]);
    repository(&b, &git, &["main.rs"]);
    let selected = fixture.path().join("selection.json");
    write_json(&selected, json!({"default":{}}));
    let engine = Host::new(
        &a,
        git,
        Options {
            allow_index_read: true,
            ..Default::default()
        },
        Resolved::current(&a, None, None, &d()).unwrap(),
        &d(),
    )
    .unwrap()
    .open(&d())
    .unwrap();
    let captured = fixture.path().join("captured.json");
    write_json(
        &captured,
        serde_json::to_value(&engine.capture_options).unwrap(),
    );
    let output = launch(
        &b,
        &["--capture-config".into(), captured.display().to_string()],
        &arguments("native", &selected, None, "scope-refused"),
        vec![],
    );
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("cannot import repository scopes"));
    assert!(!b.join(".yoke").exists());
    let servers = fixture.path().join("must-not-be-read.json");
    let output = launch(
        &b,
        &[
            "--language-server-config".into(),
            servers.display().to_string(),
        ],
        &arguments("native", &selected, None, "untrusted-server"),
        vec![],
    );
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("requires --trust-servers"));
    assert!(!servers.exists() && !b.join(".yoke").exists());
}

#[test]
fn named_stdio_resume_keeps_membership_but_refuses_changed_limits_and_preference_lifetime() {
    let fixture = tempfile::tempdir().unwrap();
    let git = HostGit::discover().unwrap();
    let a = fixture.path().join("a");
    let b = fixture.path().join("b");
    repository(&a, &git, &["main.rs"]);
    repository(&b, &git, &["main.rs"]);
    let selected = fixture.path().join("selection.json");
    let host = fixture.path().join("host.json");
    write_json(
        &host,
        json!({"startupProject":"secondary","projects":[{"name":"secondary","root":b,"readOnly":true,"queryOnly":false}]}),
    );
    write_json(&selected, json!({"default":{},"secondary":{}}));
    let args = arguments("native", &selected, Some(&host), "binding");
    let call = json!({"name":"ci_read","arguments":{"path":"main.rs"}});
    replies(&launch(&a, &[], &args, vec![call.clone()]), 1, 9);
    let before = record(&a, "binding");
    fs::write(b.join("new.rs"), b"pub fn new_member(){}\n").unwrap();
    replies(&launch(&a, &[], &args, vec![call.clone()]), 1, 9);
    assert_eq!(record(&a, "binding"), before);
    assert!(snapshots(&b).iter().any(|s| {
        s["files"]
            .as_array()
            .unwrap()
            .iter()
            .any(|f| f["path"] == "new.rs")
    }));
    write_json(
        &selected,
        json!({"default":{},"secondary":{"limits":{"maxIndexBytes":7999999}}}),
    );
    let output = launch(&a, &[], &args, vec![]);
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    assert_eq!(record(&a, "binding"), before);
    write_json(&selected, json!({"default":{},"secondary":{}}));
    let store = yoke::store::Store::open(&b).unwrap();
    repository_preferences::set(&store, &Expected::Absent, Choice::default(), true, &d()).unwrap();
    let output = launch(&a, &[], &args, vec![]);
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    assert_eq!(record(&a, "binding"), before);
}

#[test]
fn exhausted_primary_git_budget_refuses_before_store_and_protocol_output() {
    let fixture = tempfile::tempdir().unwrap();
    let git = HostGit::discover().unwrap();
    let source = fixture.path().join("source");
    repository(&source, &git, &["main.rs"]);
    let selected = fixture.path().join("selection.json");
    write_json(&selected, json!({"default":{"limits":{"maxIndexBytes":0}}}));
    let output = launch(
        &source,
        &[],
        &arguments("native", &selected, None, "budget"),
        vec![],
    );
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    assert!(!source.join(".yoke").exists());
}
