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
    capture::Deadline,
    git_workflows::{
        HostGit,
        repository_host::Host,
        repository_preferences::{self as preferences, Choice, Expected, Resolved},
        repository_scope::{self, Options},
    },
    mcp_http::{self, Config, Principal, Target},
    project_session::{HostProfile, HostProject},
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
fn target(
    root: &Path,
    git: &HostGit,
    profile: Profile,
    nested: bool,
    limit: Option<usize>,
) -> Target {
    let mut options = Options {
        allow_index_read: true,
        ..Default::default()
    };
    if let Some(limit) = limit {
        options.limits.max_index_bytes = limit;
    }
    let driver = Host::new(
        root,
        git.clone(),
        options,
        Resolved::current(root, None, Some(nested), &d()).unwrap(),
        &d(),
    )
    .unwrap();
    let mut target = Target::new(root, profile, HostProfile::default());
    target.git_repository_capture = Some(driver);
    target
}
fn principal(id: &str, token: u8, target: Target) -> Principal {
    Principal::new(id, Zeroizing::new(vec![token; 32]), target).unwrap()
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

#[test]
fn cli_git_data_cannot_grant_authority_or_reach_credentials() {
    for (flags, selection, host_profile, expected) in [
        (
            vec![],
            json!({}),
            json!({}),
            "requires --allow-git-index-read and --trust-git",
        ),
        (
            vec!["--trust-git"],
            json!({}),
            json!({}),
            "requires --allow-git-index-read and --trust-git",
        ),
        (
            vec!["--allow-git-index-read"],
            json!({}),
            json!({}),
            "requires --allow-git-index-read and --trust-git",
        ),
        (
            vec![],
            json!({"trustGit":true,"allowIndexRead":true}),
            json!({}),
            "Invalid host input schema",
        ),
        (
            vec!["--trust-git", "--allow-git-index-read"],
            json!({"program":"relative-git"}),
            json!({}),
            "repository_host_program_invalid",
        ),
        (
            vec!["--trust-git", "--allow-git-index-read"],
            json!({"limits":{"maxRepositories":65}}),
            json!({}),
            "repository_scope_invalid_limits",
        ),
        (
            vec!["--trust-git", "--allow-git-index-read"],
            json!({}),
            json!({"projects":[{"name":"foreign","root":"C:/not-resolved-owned-fixture"}]}),
            "fixed root",
        ),
    ] {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("host.json");
        fs::write(&path,json!({"tlsCredentialName":"owned-not-accessed-tls","principals":[{"id":"owned","tokenCredentialName":"owned-not-accessed-token","gitRepositoryCapture":selection,"hostProfile":host_profile}]}).to_string()).unwrap();
        let output = Command::new(env!("CARGO_BIN_EXE_yoke"))
            .arg("--root")
            .arg(root.path())
            .args(["serve-http", "--trust-service", "--config"])
            .arg(path)
            .args(flags)
            .output()
            .unwrap();
        assert!(!output.status.success());
        assert!(output.stdout.is_empty());
        assert!(
            String::from_utf8_lossy(&output.stderr).contains(expected),
            "{expected}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(!root.path().join(".yoke").exists());
    }
}

#[test]
fn authentication_precedes_source_capture_and_bad_metadata_precedes_cold_store() {
    let _gate = HTTP.lock().unwrap_or_else(|p| p.into_inner());
    let root = tempfile::tempdir().unwrap();
    let git = HostGit::discover().unwrap();
    let invalid = root.path().join("invalid");
    fs::create_dir(&invalid).unwrap();
    assert!(
        Principal::new(
            "invalid",
            Zeroizing::new(vec![31; 32]),
            target(&invalid, &git, Profile::Native, false, None)
        )
        .is_err()
    );
    assert!(!invalid.join(".yoke").exists());
    repository(root.path(), &git, &["main.rs"]);
    let server = mcp_http::serve(
        config(),
        vec![principal(
            "owned",
            31,
            target(root.path(), &git, Profile::Native, false, None),
        )],
    )
    .unwrap();
    let address = server.local_addr();
    assert!(snapshots(root.path()).is_empty());
    let denied = request(address, 32, None, init());
    assert_eq!(denied.status, 401);
    assert!(snapshots(root.path()).is_empty());
    let response = request(address, 31, None, init());
    assert_eq!(response.status, 200);
    assert!(response.body.get("error").is_none());
    assert_eq!(snapshots(root.path()).len(), 1);
}

#[test]
fn full_host_source_policy_applies_before_first_snapshot_publication() {
    let _gate = HTTP.lock().unwrap_or_else(|p| p.into_inner());
    let fixture = tempfile::tempdir().unwrap();
    let root = fixture.path().join("source π '");
    let git = HostGit::discover().unwrap();
    repository(
        &root,
        &git,
        &["allowed/main.rs", "allowed/excluded.rs", "outside.rs"],
    );
    let mut selected = target(&root, &git, Profile::Session, false, None);
    selected.capture_options.only_dirs = vec!["allowed".into()];
    selected.host.source_exclusions =
        Some(serde_json::from_value(json!({"global":[["allowed/excluded.rs"]]})).unwrap());
    let server = mcp_http::serve(config(), vec![principal("owned", 33, selected)]).unwrap();
    assert!(snapshots(&root).is_empty());
    let address = server.local_addr();
    let sid = initialize(address, 33);
    let initial = snapshots(&root);
    assert_eq!(initial.len(), 1);
    assert_eq!(
        initial[0]["files"]
            .as_array()
            .unwrap()
            .iter()
            .map(|f| f["path"].as_str().unwrap())
            .collect::<Vec<_>>(),
        ["allowed/main.rs"]
    );
    let read = call(
        address,
        33,
        &sid,
        "ci_read",
        json!({"path":"allowed/main.rs"}),
    );
    assert_eq!(read["status"], "ok");
    let denied = call(
        address,
        33,
        &sid,
        "ci_read",
        json!({"path":"allowed/excluded.rs"}),
    );
    assert_eq!(denied["status"], "error");
    assert!(snapshots(&root).iter().all(|s| {
        s["files"]
            .as_array()
            .unwrap()
            .iter()
            .all(|f| f["path"] == "allowed/main.rs")
    }));
}

#[test]
fn principals_have_distinct_fixed_roots_and_cannot_initialize_each_others_sources() {
    let _gate = HTTP.lock().unwrap_or_else(|p| p.into_inner());
    let fixture = tempfile::tempdir().unwrap();
    let a = fixture.path().join("a");
    let b = fixture.path().join("b");
    let git = HostGit::discover().unwrap();
    repository(&a, &git, &["a.rs"]);
    repository(&b, &git, &["b.rs"]);
    repository(&a.join("nested"), &git, &["child.rs"]);
    let server = mcp_http::serve(
        config(),
        vec![
            principal("a", 34, target(&a, &git, Profile::Native, true, None)),
            principal("b", 35, target(&b, &git, Profile::Native, false, None)),
        ],
    )
    .unwrap();
    let address = server.local_addr();
    let sid_a = initialize(address, 34);
    assert!(snapshots(&b).is_empty());
    let denied = call(
        address,
        34,
        &sid_a,
        "ci_status",
        json!({"mode":"index","root":b,"trustGit":true}),
    );
    assert_eq!(denied["status"], "error");
    assert!(snapshots(&b).is_empty());
    let wrong = request(
        address,
        35,
        Some(&sid_a),
        json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}),
    );
    assert_ne!(wrong.status, 200);
    assert!(snapshots(&b).is_empty());
    let sid_b = initialize(address, 35);
    let a_index = call(address, 34, &sid_a, "ci_status", json!({"mode":"index"}));
    let b_index = call(address, 35, &sid_b, "ci_status", json!({"mode":"index"}));
    assert_eq!(a_index["status"], "ok");
    assert_eq!(b_index["status"], "ok");
    assert!(
        a_index["data"]["snapshot"]["files"]
            .as_array()
            .unwrap()
            .iter()
            .any(|f| f["path"] == "nested/child.rs")
    );
    assert_eq!(
        b_index["data"]["snapshot"]["files"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(b_index["data"]["snapshot"]["files"][0]["path"], "b.rs");
}

#[test]
fn current_preferences_refuse_existing_and_new_http_sessions_without_new_source_proof() {
    let _gate = HTTP.lock().unwrap_or_else(|p| p.into_inner());
    let fixture = tempfile::tempdir().unwrap();
    let root = fixture.path().join("source");
    let git = HostGit::discover().unwrap();
    repository(&root, &git, &["main.rs"]);
    let server = mcp_http::serve(
        config(),
        vec![principal(
            "owned",
            36,
            target(&root, &git, Profile::Native, false, None),
        )],
    )
    .unwrap();
    let address = server.local_addr();
    let sid = initialize(address, 36);
    let before = snapshots(&root);
    let store = yoke::store::Store::open(&root).unwrap();
    preferences::set(&store, &Expected::Absent, Choice::default(), true, &d()).unwrap();
    let denied = call(address, 36, &sid, "ci_status", json!({"mode":"index"}));
    assert_eq!(denied["status"], "error");
    assert!(denied.to_string().contains("repository_selection_stale"));
    let next = request(address, 36, None, init());
    assert!(next.status != 200 || next.body.get("error").is_some());
    assert_eq!(snapshots(&root), before);
}

#[test]
fn metadata_preflight_and_first_capture_share_the_original_index_byte_budget() {
    let _gate = HTTP.lock().unwrap_or_else(|p| p.into_inner());
    let fixture = tempfile::tempdir().unwrap();
    let root = fixture.path().join("source");
    let git = HostGit::discover().unwrap();
    repository(&root, &git, &["main.rs"]);
    let observation = repository_scope::inspect(
        &root,
        &git,
        &Options {
            allow_index_read: true,
            ..Default::default()
        },
        &d(),
    )
    .unwrap();
    let acquired = observation.value()["observation"]["acquiredIndexBytes"]
        .as_u64()
        .unwrap() as usize;
    assert!(acquired > 0);
    {
        let server = mcp_http::serve(
            config(),
            vec![principal(
                "short",
                37,
                target(&root, &git, Profile::Native, false, Some(acquired)),
            )],
        )
        .unwrap();
        let response = request(server.local_addr(), 37, None, init());
        assert!(response.status != 200 || response.body.get("error").is_some());
        assert!(snapshots(&root).is_empty());
    }
    {
        let server = mcp_http::serve(
            config(),
            vec![principal(
                "exact",
                38,
                target(&root, &git, Profile::Native, false, Some(acquired * 2)),
            )],
        )
        .unwrap();
        let address = server.local_addr();
        let sid = initialize(address, 38);
        assert_eq!(snapshots(&root).len(), 1);
        let repeated = call(address, 38, &sid, "ci_status", json!({"mode":"index"}));
        assert_eq!(repeated["status"], "ok");
        assert_eq!(snapshots(&root).len(), 1);
    }
}

#[test]
fn typed_http_target_refuses_foreign_driver_and_registered_project_authority() {
    let fixture = tempfile::tempdir().unwrap();
    let git = HostGit::discover().unwrap();
    let a = fixture.path().join("a");
    let b = fixture.path().join("b");
    repository(&a, &git, &["a.rs"]);
    repository(&b, &git, &["b.rs"]);
    let mut foreign = target(&a, &git, Profile::Native, false, None);
    foreign.root = b.clone();
    assert!(Principal::new("foreign", Zeroizing::new(vec![39; 32]), foreign).is_err());
    assert!(!a.join(".yoke").exists() && !b.join(".yoke").exists());
    let missing = fixture.path().join("never-resolved");
    let mut multi = target(&a, &git, Profile::Session, false, None);
    multi.host.projects.push(HostProject {
        name: "foreign".into(),
        root: missing.clone(),
        read_only: true,
        query_only: true,
    });
    assert!(Principal::new("multi", Zeroizing::new(vec![40; 32]), multi).is_err());
    assert!(!missing.exists() && !a.join(".yoke").exists());
}
