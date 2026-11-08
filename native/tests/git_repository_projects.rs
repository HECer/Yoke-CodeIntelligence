use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    fs,
    io::{Read, Write},
    net::{SocketAddr, TcpStream},
    path::Path,
    process::Command,
    time::Duration,
};
use yoke::{
    Engine,
    capture::Deadline,
    git_workflows::{
        HostGit,
        repository_host::Host,
        repository_preferences::{self as preferences, Choice, Expected, Resolved},
        repository_projects::Hosts,
        repository_scope::{self, Options},
    },
    mcp_http::{self, Config, Principal, Target},
    project_session::{HostProfile, HostProject, RECORD_KIND, Session},
    protocol::Profile,
};
use zeroize::Zeroizing;
static HTTP: std::sync::Mutex<()> = std::sync::Mutex::new(());
fn d() -> Deadline {
    Deadline::new(60000)
}
fn repository(root: &Path, git: &HostGit, files: &[&str]) {
    fs::create_dir_all(root).unwrap();
    let run = |args: &[&str]| {
        let o = Command::new(&git.executable)
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
        assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    };
    run(&["init", "--quiet", "--template="]);
    for file in files {
        let path = root.join(file);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, b"pub fn owned(){}\r\n").unwrap();
        run(&["add", "--", file]);
    }
    run(&["commit", "--quiet", "-m", "Owned source"]);
}
struct Response {
    status: u16,
    headers: BTreeMap<String, String>,
    body: Value,
}
fn request(address: SocketAddr, token: u8, sid: Option<&str>, message: Value) -> Response {
    let bytes = serde_json::to_vec(&message).unwrap();
    let mut socket = TcpStream::connect(address).unwrap();
    socket
        .set_read_timeout(Some(Duration::from_secs(90)))
        .unwrap();
    let session = sid
        .map(|sid| format!("Mcp-Session-Id: {sid}\r\nMcp-Protocol-Version: 2025-11-25\r\n"))
        .unwrap_or_default();
    let header = format!(
        "POST /mcp HTTP/1.1\r\nHost: {address}\r\nAuthorization: Bearer {}\r\nAccept: application/json, text/event-stream\r\nContent-Type: application/json\r\nContent-Length: {}\r\n{session}\r\n",
        hex::encode([token; 32]),
        bytes.len()
    );
    socket.write_all(header.as_bytes()).unwrap();
    socket.write_all(&bytes).unwrap();
    let mut response = Vec::new();
    loop {
        let mut chunk = [0; 8192];
        let n = socket.read(&mut chunk).unwrap();
        assert_ne!(n, 0);
        response.extend_from_slice(&chunk[..n]);
        assert!(response.len() <= 16_001_000);
        if let Some(end) = response.windows(4).position(|v| v == b"\r\n\r\n") {
            let header = std::str::from_utf8(&response[..end]).unwrap();
            let length: usize = header
                .split("\r\n")
                .filter_map(|v| v.split_once(':'))
                .find(|(k, _)| k.eq_ignore_ascii_case("content-length"))
                .unwrap()
                .1
                .trim()
                .parse()
                .unwrap();
            assert!(length <= 16_000_000);
            if response.len() == end + 4 + length {
                break;
            }
        }
    }
    let end = response.windows(4).position(|v| v == b"\r\n\r\n").unwrap();
    let text = std::str::from_utf8(&response[..end]).unwrap();
    let status = text.split_whitespace().nth(1).unwrap().parse().unwrap();
    let headers = text
        .split("\r\n")
        .filter_map(|v| v.split_once(':'))
        .map(|(k, v)| (k.to_ascii_lowercase(), v.trim().to_owned()))
        .collect();
    let body = if response.len() == end + 4 {
        Value::Null
    } else {
        serde_json::from_slice(&response[end + 4..]).unwrap_or_else(|_| {
            Value::String(String::from_utf8_lossy(&response[end + 4..]).into_owned())
        })
    };
    Response {
        status,
        headers,
        body,
    }
}
fn init() -> Value {
    json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"owned-git-http","version":"1"}}})
}
fn initialize(address: SocketAddr, token: u8) -> String {
    let response = request(address, token, None, init());
    assert_eq!(response.status, 200, "{}", response.body);
    assert!(response.body.get("error").is_none(), "{}", response.body);
    let sid = response.headers["mcp-session-id"].clone();
    assert!(yoke::model::valid_id(&sid));
    let notification = request(
        address,
        token,
        Some(&sid),
        json!({"jsonrpc":"2.0","method":"notifications/initialized"}),
    );
    assert_eq!(notification.status, 202);
    sid
}
fn call(address: SocketAddr, token: u8, sid: &str, name: &str, args: Value) -> Value {
    let response = request(
        address,
        token,
        Some(sid),
        json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":name,"arguments":args}}),
    );
    assert_eq!(response.status, 200);
    assert!(response.body.get("error").is_none(), "{}", response.body);
    serde_json::from_str(
        response.body["result"]["content"][0]["text"]
            .as_str()
            .unwrap(),
    )
    .unwrap()
}
fn config() -> Config {
    Config {
        request_timeout_ms: 30000,
        ..Default::default()
    }
}
fn snapshots(root: &Path) -> Vec<Value> {
    let db = rusqlite::Connection::open_with_flags(
        root.join(".yoke/index.sqlite"),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .unwrap();
    let mut stmt = db
        .prepare("SELECT value FROM snapshots ORDER BY id")
        .unwrap();
    stmt.query_map([], |r| r.get::<_, String>(0))
        .unwrap()
        .map(|s| serde_json::from_str(&s.unwrap()).unwrap())
        .collect()
}

fn profile(secondary: &Path) -> HostProfile {
    HostProfile {
        projects: vec![HostProject {
            name: "secondary".into(),
            root: secondary.into(),
            read_only: false,
            query_only: false,
        }],
        ..Default::default()
    }
}
fn drivers(
    root: &Path,
    host: &HostProfile,
    git: &HostGit,
    secondary_limit: Option<usize>,
) -> BTreeMap<String, Host> {
    std::iter::once(("default", root))
        .chain(
            host.projects
                .iter()
                .map(|p| (p.name.as_str(), p.root.as_path())),
        )
        .map(|(name, path)| {
            let mut options = Options {
                allow_index_read: true,
                ..Default::default()
            };
            if name != "default"
                && let Some(limit) = secondary_limit
            {
                options.limits.max_index_bytes = limit;
            }
            (
                name.into(),
                Host::new(
                    path,
                    git.clone(),
                    options,
                    Resolved::current(path, None, Some(name != "default"), &d()).unwrap(),
                    &d(),
                )
                .unwrap(),
            )
        })
        .collect()
}
fn selected_target(
    root: &Path,
    host: HostProfile,
    git: &HostGit,
    selected_profile: Profile,
) -> Target {
    let selected = Hosts::new(root, &host, drivers(root, &host, git, None), &d()).unwrap();
    let mut target = Target::new(root, selected_profile, host);
    target.git_repository_projects = Some(selected);
    target
}
fn selected_session(
    root: &Path,
    host: HostProfile,
    git: &HostGit,
    label: &str,
    secondary_limit: Option<usize>,
    writes: bool,
) -> anyhow::Result<Session> {
    let selected = drivers(root, &host, git, secondary_limit);
    let primary = selected["default"].clone();
    let selected = Hosts::new(root, &host, selected, &d())?;
    let mut engine = primary.open(&d())?;
    engine.allow_writes = writes;
    Session::open_with_git_projects(engine, host, Some(label), selected, &d())
}
fn rows(root: &Path) -> Vec<(String, String, String)> {
    let db = rusqlite::Connection::open_with_flags(
        root.join(".yoke/index.sqlite"),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .unwrap();
    let mut statement = db
        .prepare("SELECT kind,id,value FROM records ORDER BY kind,id")
        .unwrap();
    statement
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap()
}

#[test]
fn cli_project_selections_need_independent_permissions_and_exact_registered_names_before_credentials()
 {
    for (flags, selection, expected) in [
        (
            vec![],
            json!({"default":{}}),
            "requires --allow-git-index-read and --trust-git",
        ),
        (
            vec!["--trust-git"],
            json!({"default":{}}),
            "requires --allow-git-index-read and --trust-git",
        ),
        (
            vec!["--allow-git-index-read"],
            json!({"default":{}}),
            "requires --allow-git-index-read and --trust-git",
        ),
        (
            vec!["--trust-git", "--allow-git-index-read"],
            json!({}),
            "exactly default and every registered project",
        ),
        (
            vec!["--trust-git", "--allow-git-index-read"],
            json!({"default":{},"foreign":{}}),
            "exactly default and every registered project",
        ),
        (
            vec!["--trust-git", "--allow-git-index-read"],
            json!({"default":{"program":"relative"}}),
            "repository_host_program_invalid",
        ),
        (
            vec![],
            json!({"default":{"trustGit":true}}),
            "Invalid host input schema",
        ),
    ] {
        let fixture = tempfile::tempdir().unwrap();
        let config = fixture.path().join("host.json");
        fs::write(&config,serde_json::to_vec(&json!({"principals":[{"id":"owned","tokenCredentialName":"must-not-be-resolved","gitRepositoryProjects":selection}]})).unwrap()).unwrap();
        let output = Command::new(env!("CARGO_BIN_EXE_yoke"))
            .arg("--root")
            .arg(fixture.path())
            .args(["serve-http", "--trust-service", "--config"])
            .arg(config)
            .args(flags)
            .output()
            .unwrap();
        assert!(!output.status.success());
        assert!(output.stdout.is_empty());
        assert!(
            String::from_utf8_lossy(&output.stderr).contains(expected),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(!fixture.path().join(".yoke").exists());
    }
}

#[test]
fn typed_selection_rejects_unknown_missing_mismatched_and_overlapping_roots_without_source_state() {
    let fixture = tempfile::tempdir().unwrap();
    let git = HostGit::discover().unwrap();
    let a = fixture.path().join("a");
    let b = fixture.path().join("b");
    repository(&a, &git, &["main.rs"]);
    repository(&b, &git, &["main.rs"]);
    let mut host = profile(&b);
    host.projects[0].root = fixture.path().join("never-resolved");
    let primary = drivers(&a, &HostProfile::default(), &git, None);
    assert!(Hosts::new(&a, &host, primary.clone(), &d()).is_err());
    assert!(!host.projects[0].root.exists());
    let mut unknown = primary.clone();
    unknown.insert("foreign".into(), primary["default"].clone());
    assert!(Hosts::new(&a, &HostProfile::default(), unknown, &d()).is_err());
    let mut wrong = drivers(&a, &profile(&b), &git, None);
    wrong.insert("secondary".into(), primary["default"].clone());
    assert!(Hosts::new(&a, &profile(&b), wrong, &d()).is_err());
    let nested = a.join("nested");
    repository(&nested, &git, &["child.rs"]);
    let host = profile(&nested);
    assert!(Hosts::new(&a, &host, drivers(&a, &host, &git, None), &d()).is_err());
    assert!(
        !a.join(".yoke").exists() && !b.join(".yoke").exists() && !nested.join(".yoke").exists()
    );
}

#[test]
fn http_secondary_first_capture_uses_its_own_scope_and_complete_source_policy() {
    let _gate = HTTP.lock().unwrap_or_else(|p| p.into_inner());
    let fixture = tempfile::tempdir().unwrap();
    let git = HostGit::discover().unwrap();
    let a = fixture.path().join("primary \u{3c0}");
    let b = fixture.path().join("secondary \u{df}");
    repository(&a, &git, &["main.rs", "excluded.rs", "primary-secret.rs"]);
    repository(&b, &git, &["main.rs", "excluded.rs", "secondary-secret.rs"]);
    repository(&b.join("nested"), &git, &["child.rs"]);
    let mut host = profile(&b);
    host.source_exclusions=Some(serde_json::from_value(json!({"global":[["excluded.rs"]],"projects":{"default":{"project":[["primary-secret.rs"]]},"secondary":{"local":[["secondary-secret.rs"]]}}})).unwrap());
    let server = mcp_http::serve(
        config(),
        vec![
            Principal::new(
                "owned",
                Zeroizing::new(vec![51; 32]),
                selected_target(&a, host, &git, Profile::Session),
            )
            .unwrap(),
        ],
    )
    .unwrap();
    assert!(snapshots(&a).is_empty());
    assert!(!b.join(".yoke").exists());
    let address = server.local_addr();
    let sid = initialize(address, 51);
    let before = snapshots(&a);
    assert_eq!(before.len(), 1);
    assert_eq!(
        before[0]["files"]
            .as_array()
            .unwrap()
            .iter()
            .map(|f| f["path"].as_str().unwrap())
            .collect::<Vec<_>>(),
        ["main.rs"]
    );
    assert!(!b.join(".yoke").exists());
    let read = call(
        address,
        51,
        &sid,
        "ci_session",
        json!({"operation":"queryProject","args":{"project":"secondary","tool":"ci_read","arguments":{"path":"nested/child.rs"}}}),
    );
    assert_eq!(read["status"], "ok");
    let selected = snapshots(&b);
    assert_eq!(selected.len(), 1);
    assert_eq!(
        selected[0]["files"]
            .as_array()
            .unwrap()
            .iter()
            .map(|f| f["path"].as_str().unwrap())
            .collect::<Vec<_>>(),
        ["main.rs", "nested/child.rs"]
    );
    assert_eq!(snapshots(&a), before);
    let id = selected[0]["id"].as_str().unwrap();
    let db = rusqlite::Connection::open_with_flags(
        b.join(".yoke/index.sqlite"),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .unwrap();
    let proof: String = db
        .query_row(
            "SELECT value FROM records WHERE kind='native_capture_options_v1' AND id=?1",
            [id],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(
        serde_json::from_str::<Value>(&proof).unwrap()["options"]["repositoryScope"]["readOnlyRoots"],
        json!(["nested"])
    );
    let denied = call(
        address,
        51,
        &sid,
        "ci_session",
        json!({"operation":"queryProject","args":{"project":fixture.path().join("never-selected"),"tool":"ci_read","arguments":{"path":"main.rs"}}}),
    );
    assert_eq!(denied["status"], "error");
    assert!(!fixture.path().join("never-selected").exists());
}

#[test]
fn registered_git_projects_preserve_primary_independent_secondary_and_query_only_permissions() {
    let fixture = tempfile::tempdir().unwrap();
    let git = HostGit::discover().unwrap();
    let a = fixture.path().join("a");
    let b = fixture.path().join("b");
    let c = fixture.path().join("c");
    for root in [&a, &b, &c] {
        repository(root, &git, &["main.rs"]);
    }
    repository(&b.join("nested"), &git, &["child.rs"]);
    let mut host = profile(&b);
    host.primary_read_only = true;
    host.projects.push(HostProject {
        name: "reference".into(),
        root: c.clone(),
        read_only: true,
        query_only: true,
    });
    let mut session = selected_session(&a, host, &git, "permissions", None, true).unwrap();
    assert!(!session.active_engine().unwrap().allow_writes);
    session
        .invoke("activate", &json!({"project":"secondary"}), &d())
        .unwrap();
    let engine = session.active_engine().unwrap();
    assert!(engine.allow_writes);
    let snapshot = engine.index(&d()).unwrap().0;
    let child = snapshot
        .files
        .iter()
        .find(|f| f.path == "nested/child.rs")
        .unwrap();
    let denied=engine.invoke("ci_change_prepare",json!({"snapshotId":snapshot.id,"edits":[{"kind":"replace","path":child.path,"expectedHash":child.hash,"startByte":0,"endByte":child.size,"text":"pub fn refused(){}\n"}]})).unwrap();
    assert_eq!(denied["status"], "error");
    assert!(
        session
            .invoke("activate", &json!({"project":"reference"}), &d())
            .is_err()
    );
    let read = session
        .invoke(
            "queryProject",
            &json!({"project":"reference","tool":"ci_read","arguments":{"path":"main.rs"}}),
            &d(),
        )
        .unwrap();
    assert_eq!(read["status"], "ok");
    assert!(
        session
            .invoke(
                "queryProject",
                &json!({"project":"reference","tool":"ci_change_prepare","arguments":{"edits":[]}}),
                &d()
            )
            .is_err()
    );
    session
        .invoke("activate", &json!({"project":"default"}), &d())
        .unwrap();
    assert!(!session.active_engine().unwrap().allow_writes);
}

#[test]
fn session_resume_keeps_dynamic_secondary_membership_but_binds_secondary_limits() {
    let fixture = tempfile::tempdir().unwrap();
    let git = HostGit::discover().unwrap();
    let a = fixture.path().join("a");
    let b = fixture.path().join("b");
    repository(&a, &git, &["main.rs"]);
    repository(&b, &git, &["main.rs"]);
    let host = profile(&b);
    let mut session =
        selected_session(&a, host.clone(), &git, "stable-projects", None, false).unwrap();
    session
        .invoke("activate", &json!({"project":"secondary"}), &d())
        .unwrap();
    let id = yoke::model::identity(&(
        RECORD_KIND,
        &session.engine.store.workspace_id,
        "stable-projects",
    ));
    let before = session
        .engine
        .store
        .get_record::<Value>(RECORD_KIND, &id)
        .unwrap();
    fs::write(b.join("added.rs"), b"pub fn later_member(){}\n").unwrap();
    assert!(
        session
            .active_engine()
            .unwrap()
            .index(&d())
            .unwrap()
            .0
            .files
            .iter()
            .any(|f| f.path == "added.rs")
    );
    drop(session);
    let resumed = selected_session(&a, host.clone(), &git, "stable-projects", None, false).unwrap();
    assert_eq!(
        resumed
            .engine
            .store
            .get_record::<Value>(RECORD_KIND, &id)
            .unwrap(),
        before
    );
    drop(resumed);
    assert!(selected_session(&a, host, &git, "stable-projects", Some(7_999_999), false).is_err());
    let engine = Engine::open(&a).unwrap();
    assert_eq!(
        engine.store.get_record::<Value>(RECORD_KIND, &id).unwrap(),
        before
    );
}

#[test]
fn inactive_secondary_preference_changes_refuse_notes_and_existing_native_http_dispatch() {
    let _gate = HTTP.lock().unwrap_or_else(|p| p.into_inner());
    let fixture = tempfile::tempdir().unwrap();
    let git = HostGit::discover().unwrap();
    let a = fixture.path().join("a");
    let b = fixture.path().join("b");
    repository(&a, &git, &["main.rs"]);
    repository(&b, &git, &["main.rs"]);
    let host = profile(&b);
    let mut session =
        selected_session(&a, host.clone(), &git, "stale-projects", None, false).unwrap();
    let server = mcp_http::serve(
        config(),
        vec![
            Principal::new(
                "owned",
                Zeroizing::new(vec![52; 32]),
                selected_target(&a, host, &git, Profile::Native),
            )
            .unwrap(),
        ],
    )
    .unwrap();
    let address = server.local_addr();
    let sid = initialize(address, 52);
    let before = rows(&a);
    let store = yoke::store::Store::open(&b).unwrap();
    preferences::set(
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
    assert!(
        session
            .invoke(
                "notes.write",
                &json!({"name":"must-not-publish","content":"blocked"}),
                &d()
            )
            .is_err()
    );
    assert_eq!(rows(&a), before);
    let denied = request(
        address,
        52,
        Some(&sid),
        json!({"jsonrpc":"2.0","id":4,"method":"tools/list","params":{}}),
    );
    assert!(denied.status != 200 || denied.body.get("error").is_some());
    assert_eq!(rows(&a), before);
    assert!(snapshots(&b).is_empty());
}

#[test]
fn concurrent_session_state_refuses_note_mutation_with_registered_git_hosts() {
    let fixture = tempfile::tempdir().unwrap();
    let git = HostGit::discover().unwrap();
    let a = fixture.path().join("a");
    let b = fixture.path().join("b");
    repository(&a, &git, &["main.rs"]);
    repository(&b, &git, &["main.rs"]);
    let host = profile(&b);
    let mut first =
        selected_session(&a, host.clone(), &git, "concurrent-projects", None, false).unwrap();
    let mut second = selected_session(&a, host, &git, "concurrent-projects", None, false).unwrap();
    second
        .invoke("activate", &json!({"project":"secondary"}), &d())
        .unwrap();
    let before = rows(&a);
    assert!(
        first
            .invoke(
                "notes.write",
                &json!({"name":"must-not-publish","content":"blocked"}),
                &d()
            )
            .is_err()
    );
    assert_eq!(rows(&a), before);
}

#[test]
fn invalid_secondary_metadata_refuses_before_its_cold_store_or_activation_state() {
    let fixture = tempfile::tempdir().unwrap();
    let git = HostGit::discover().unwrap();
    let a = fixture.path().join("a");
    let b = fixture.path().join("not-a-repository");
    repository(&a, &git, &["main.rs"]);
    fs::create_dir(&b).unwrap();
    let mut session =
        selected_session(&a, profile(&b), &git, "bad-secondary", None, false).unwrap();
    let before = rows(&a);
    assert!(
        session
            .invoke("activate", &json!({"project":"secondary"}), &d())
            .is_err()
    );
    assert_eq!(rows(&a), before);
    assert!(!b.join(".yoke").exists());
}

#[test]
fn secondary_capture_keeps_one_index_byte_budget_and_refuses_before_state_update() {
    let fixture = tempfile::tempdir().unwrap();
    let git = HostGit::discover().unwrap();
    let a = fixture.path().join("a");
    let b = fixture.path().join("b");
    repository(&a, &git, &["main.rs"]);
    repository(&b, &git, &["main.rs"]);
    let acquired = repository_scope::inspect(
        &b,
        &git,
        &Options {
            allow_index_read: true,
            ..Default::default()
        },
        &d(),
    )
    .unwrap()
    .value()["observation"]["acquiredIndexBytes"]
        .as_u64()
        .unwrap() as usize;
    assert!(acquired > 0);
    let mut session = selected_session(
        &a,
        profile(&b),
        &git,
        "short-secondary",
        Some(acquired),
        false,
    )
    .unwrap();
    let before = rows(&a);
    assert!(
        session
            .invoke("activate", &json!({"project":"secondary"}), &d())
            .is_err()
    );
    assert_eq!(rows(&a), before);
    assert!(snapshots(&b).is_empty());
}

#[test]
fn startup_secondary_capture_applies_its_policy_without_publishing_primary_source() {
    let _gate = HTTP.lock().unwrap_or_else(|p| p.into_inner());
    let fixture = tempfile::tempdir().unwrap();
    let git = HostGit::discover().unwrap();
    let a = fixture.path().join("a");
    let b = fixture.path().join("b");
    repository(&a, &git, &["main.rs"]);
    repository(&b, &git, &["main.rs", "excluded.rs"]);
    let mut host = profile(&b);
    host.startup_project = Some("secondary".into());
    host.source_exclusions = Some(
        serde_json::from_value(json!({"projects":{"secondary":{"local":[["excluded.rs"]]}}}))
            .unwrap(),
    );
    let server = mcp_http::serve(
        config(),
        vec![
            Principal::new(
                "owned",
                Zeroizing::new(vec![53; 32]),
                selected_target(&a, host, &git, Profile::Session),
            )
            .unwrap(),
        ],
    )
    .unwrap();
    assert!(snapshots(&a).is_empty());
    assert!(!b.join(".yoke").exists());
    let sid = initialize(server.local_addr(), 53);
    assert!(snapshots(&a).is_empty());
    let selected = snapshots(&b);
    assert_eq!(selected.len(), 1);
    assert_eq!(
        selected[0]["files"]
            .as_array()
            .unwrap()
            .iter()
            .map(|f| f["path"].as_str().unwrap())
            .collect::<Vec<_>>(),
        ["main.rs"]
    );
    let config = call(
        server.local_addr(),
        53,
        &sid,
        "ci_session",
        json!({"operation":"configuration","args":{}}),
    );
    assert_eq!(config["status"], "ok");
    assert_eq!(config["data"]["activeProject"], "secondary");
}

#[test]
fn every_registered_cli_git_selection_is_admitted_before_any_foreign_resolution_or_credential() {
    for (selected, mixed, expected) in [
        (
            json!({"default":{}}),
            false,
            "exactly default and every registered project",
        ),
        (
            json!({"default":{},"secondary":{"program":"relative-git"}}),
            false,
            "repository_host_program_invalid",
        ),
        (
            json!({"default":{},"secondary":{"limits":{"maxIndexBytes":8000001}}}),
            false,
            "repository_scope_invalid_limits",
        ),
        (
            json!({"default":{},"secondary":{}}),
            true,
            "Select either a fixed Git host or registered Git project hosts",
        ),
    ] {
        let fixture = tempfile::tempdir().unwrap();
        let missing = fixture.path().join("must-not-be-resolved");
        let file = fixture.path().join("host.json");
        let mut principal = json!({"id":"owned","tokenCredentialName":"must-not-be-resolved","hostProfile":{"projects":[{"name":"secondary","root":missing,"readOnly":true,"queryOnly":true}]},"gitRepositoryProjects":selected});
        if mixed {
            principal["gitRepositoryCapture"] = json!({});
        }
        fs::write(
            &file,
            serde_json::to_vec(&json!({"principals":[principal]})).unwrap(),
        )
        .unwrap();
        let output = Command::new(env!("CARGO_BIN_EXE_yoke"))
            .arg("--root")
            .arg(fixture.path())
            .args([
                "serve-http",
                "--trust-service",
                "--allow-git-index-read",
                "--trust-git",
                "--config",
            ])
            .arg(file)
            .output()
            .unwrap();
        assert!(!output.status.success());
        assert!(output.stdout.is_empty());
        assert!(
            String::from_utf8_lossy(&output.stderr).contains(expected),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(!missing.exists() && !fixture.path().join(".yoke").exists());
    }
}
