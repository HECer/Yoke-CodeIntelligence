//! Same-thread management over actual private HTTP; no Session crosses threads.
use serde_json::{Value, json};
use std::{
    fs,
    io::{Read, Write},
    net::{Shutdown, SocketAddr, TcpStream},
    thread,
    time::{Duration, Instant},
};
use yoke::{
    Engine,
    capture::Deadline,
    project_session::{HostProfile, Session},
};
fn d() -> Deadline {
    Deadline::new(20_000)
}
fn session(timeout: u64) -> (tempfile::TempDir, Session) {
    let root = tempfile::tempdir().unwrap();
    fs::write(
        root.path().join("main.rs"),
        "// owned 🦀\r\nfn example() {}\r\n",
    )
    .unwrap();
    let host:HostProfile=serde_json::from_value(json!({"dashboard":{"management":{"operationTimeoutMs":timeout}},"steering":{},"notesPolicy":{"writable":true}})).unwrap();
    let session = Session::open(
        Engine::open(root.path()).unwrap(),
        host,
        Some("managed"),
        &d(),
    )
    .unwrap();
    (root, session)
}
fn start(s: &mut Session) -> (SocketAddr, String) {
    s.enable_dashboard_management(&d()).unwrap();
    let result = s.invoke("openDashboard", &json!({}), &d()).unwrap();
    let url = reqwest::Url::parse(result["url"].as_str().unwrap()).unwrap();
    (
        format!("127.0.0.1:{}", url.port().unwrap())
            .parse()
            .unwrap(),
        url.fragment().unwrap().to_owned(),
    )
}
fn raw(address: SocketAddr, bytes: Vec<u8>) -> (u16, Value) {
    let mut stream = TcpStream::connect_timeout(&address, Duration::from_secs(2)).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(8)))
        .unwrap();
    stream.write_all(&bytes).unwrap();
    stream.shutdown(Shutdown::Write).unwrap();
    let mut result = Vec::new();
    if let Err(e) = stream.read_to_end(&mut result) {
        assert_eq!(e.kind(), std::io::ErrorKind::ConnectionReset);
    }
    let at = result.windows(4).position(|p| p == b"\r\n\r\n").unwrap();
    let header = String::from_utf8_lossy(&result[..at]);
    let status = header.split_whitespace().nth(1).unwrap().parse().unwrap();
    let value = serde_json::from_slice(&result[at + 4..]).unwrap_or(Value::Null);
    (status, value)
}
fn view(address: SocketAddr, token: &str) -> Value {
    let (status, value) = raw(
        address,
        format!(
            "GET /api/state HTTP/1.1\r\nHost: {address}\r\nAuthorization: Bearer {token}\r\n\r\n"
        )
        .into_bytes(),
    );
    assert_eq!(status, 200);
    value
}
fn wire(address: SocketAddr, token: &str, request: Value) -> Vec<u8> {
    let body = serde_json::to_vec(&request).unwrap();
    let mut bytes=format!("POST /api/manage HTTP/1.1\r\nHost: {address}\r\nOrigin: http://{address}\r\nAuthorization: Bearer {token}\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n",body.len()).into_bytes();
    bytes.extend(body);
    bytes
}
fn request(state: &Value, action: Value) -> Value {
    json!({"id":"a".repeat(64),"sessionStateSha256":state["view"]["management"]["sessionStateSha256"],"project":state["view"]["activeProject"],"action":action})
}
fn call(s: &mut Session, address: SocketAddr, token: &str, action: Value) -> (u16, Value) {
    let request = request(&view(address, token), action);
    let bytes = wire(address, token, request);
    let client = thread::spawn(move || raw(address, bytes));
    let until = Instant::now() + Duration::from_secs(10);
    while !client.is_finished() {
        s.poll_dashboard_management(&d()).unwrap();
        assert!(Instant::now() < until);
        thread::sleep(Duration::from_millis(1));
    }
    client.join().unwrap()
}
fn language_engine(root: &std::path::Path, log: &std::path::Path) -> Engine {
    let node = std::process::Command::new("node")
        .args(["-p", "process.execPath"])
        .output()
        .unwrap();
    assert!(node.status.success());
    let command = String::from_utf8(node.stdout).unwrap().trim().to_owned();
    let mut engine = Engine::open(root).unwrap();
    for id in ["rust", "typescript", "angular"] {
        let configured = yoke::lsp::ConfiguredServer {
            server: yoke::lsp::Server {
                language: yoke::server_catalog::get(id).unwrap().languages[0].into(),
                command: command.clone(),
                args: vec![
                    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                        .join("tests/fixtures/server-selection-lsp.mjs")
                        .display()
                        .to_string(),
                    log.display().to_string(),
                    id.into(),
                    "0".into(),
                ],
                trust: true,
            },
            configuration: yoke::lsp::HostConfiguration {
                passive_profile: Some(yoke::lsp::PassiveProfile::ContractFixture),
                ..Default::default()
            },
        };
        if id == "rust" {
            engine
                .register_configured_server(configured.clone())
                .unwrap();
        }
        engine.register_backend(id, configured).unwrap();
    }
    engine
}
fn language_host(grant: Value) -> HostProfile {
    serde_json::from_value(
        json!({"dashboard":{"management":{"languageServers":grant}},"steering":{}}),
    )
    .unwrap()
}
fn language_hover(s: &mut Session, path: &str) -> Value {
    s.active_engine()
        .unwrap()
        .invoke(
            "ci_find",
            json!({"languageServer":{"path":path,"offset":0,"operation":"hover"}}),
        )
        .unwrap()
}
#[test]
fn dashboard_cannot_activate_dormant_backend_without_separate_grant() {
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("main.rs"), "fn example() {}\n").unwrap();
    let log = root.path().join("owned-log");
    let mut session = Session::open(
        language_engine(root.path(), &log),
        language_host(Value::Null),
        Some("no-grant"),
        &d(),
    )
    .unwrap();
    let (address, token) = start(&mut session);
    let before = session.invoke("configuration", &json!({}), &d()).unwrap();
    assert_eq!(
        view(address, &token)["view"]["management"]["languageSelection"],
        false
    );
    for action in [
        json!({"kind":"selectLanguageServers","selection":{"kind":"backends","ids":["typescript"]}}),
        json!({"kind":"resetLanguageServers"}),
    ] {
        let (status, result) = call(&mut session, address, &token, action);
        assert_eq!(status, 403, "{result}");
        assert_eq!(result["notApplied"], true);
    }
    assert_eq!(
        before,
        session.invoke("configuration", &json!({}), &d()).unwrap()
    );
    assert!(!log.exists());
}
#[test]
fn dashboard_language_grant_and_request_are_closed_bounded_and_host_registered() {
    let root = tempfile::tempdir().unwrap();
    let log = root.path().join("owned-log");
    for grant in [
        json!({"backendIds":["rust","rust"]}),
        json!({"backendIds":["unknown"]}),
        json!({"backendIds":["typescript_vts"]}),
        json!({"legacyLanguages":["missing"]}),
        json!({"legacyLanguages":["rust","rust"]}),
        json!({"backendIds":vec!["rust";33]}),
    ] {
        assert!(
            Session::open(
                language_engine(root.path(), &log),
                language_host(grant),
                Some("invalid"),
                &d()
            )
            .is_err()
        );
    }
    assert!(
        serde_json::from_value::<HostProfile>(
            json!({"dashboard":{"management":{"languageServers":{"command":"untrusted"}}}})
        )
        .is_err()
    );
    let host = language_host(json!({"backendIds":["typescript"],"legacyLanguages":["rust"]}));
    let mut session = Session::open(
        language_engine(root.path(), &log),
        host,
        Some("closed"),
        &d(),
    )
    .unwrap();
    let (address, token) = start(&mut session);
    for selection in [
        json!({"kind":"backends","ids":[]}),
        json!({"kind":"backends","ids":["typescript","typescript"]}),
        json!({"kind":"backends","ids":["typescript"],"command":"injected"}),
        json!({"kind":"legacy","languages":vec!["rust";101]}),
    ] {
        let bytes = wire(
            address,
            &token,
            request(
                &view(address, &token),
                json!({"kind":"selectLanguageServers","selection":selection}),
            ),
        );
        assert_eq!(raw(address, bytes).0, 400);
    }
    for selection in [
        json!({"kind":"backends","ids":["angular"]}),
        json!({"kind":"legacy","languages":["typescript"]}),
    ] {
        assert_eq!(
            call(
                &mut session,
                address,
                &token,
                json!({"kind":"selectLanguageServers","selection":selection})
            )
            .0,
            403
        );
    }
    assert!(!log.exists());
}
#[test]
fn dashboard_ordered_selection_uses_real_provider_and_disable_never_returns_empty_success() {
    let root = tempfile::tempdir().unwrap();
    let original = b"export function visible() {}\r\n";
    fs::write(root.path().join("entry.ts"), original).unwrap();
    fs::write(root.path().join("main.rs"), "fn example() {}\n").unwrap();
    let log = root.path().join("owned-log");
    let host =
        language_host(json!({"backendIds":["angular","typescript"],"legacyLanguages":["rust"]}));
    let mut session = Session::open(
        language_engine(root.path(), &log),
        host,
        Some("ordered"),
        &d(),
    )
    .unwrap();
    let (address, token) = start(&mut session);
    for ids in [
        json!(["angular", "typescript"]),
        json!(["typescript", "angular"]),
    ] {
        let (status, result) = call(
            &mut session,
            address,
            &token,
            json!({"kind":"selectLanguageServers","selection":{"kind":"backends","ids":ids}}),
        );
        assert_eq!(status, 200, "{result}");
        assert_eq!(result["sourceWritten"], false);
        assert_eq!(view(address, &token)["view"]["selectedBackends"], ids);
        let hover = language_hover(&mut session, "entry.ts");
        assert_eq!(hover["status"], "ok", "{hover}");
        assert_eq!(hover["data"]["selectedBackendId"], ids[0]);
    }
    assert_eq!(
        call(
            &mut session,
            address,
            &token,
            json!({"kind":"selectLanguageServers","selection":{"kind":"legacy","languages":[]}})
        )
        .0,
        200
    );
    let hover = language_hover(&mut session, "main.rs");
    assert_eq!(hover["status"], "error", "{hover}");
    assert!(session.active_engine().unwrap().servers.is_empty());
    assert_eq!(
        call(
            &mut session,
            address,
            &token,
            json!({"kind":"resetLanguageServers"})
        )
        .0,
        200
    );
    assert!(
        session
            .active_engine()
            .unwrap()
            .servers
            .contains_key("rust")
    );
    assert_eq!(fs::read(root.path().join("entry.ts")).unwrap(), original);
}
#[test]
fn dashboard_language_overrides_are_project_local_persistent_and_stale_requests_refuse() {
    let root = tempfile::tempdir().unwrap();
    let second = tempfile::tempdir().unwrap();
    for path in [root.path(), second.path()] {
        fs::write(path.join("main.rs"), "// π\r\nfn example() {}\r\n").unwrap();
    }
    let log = root.path().join("owned-log");
    let mut host = language_host(json!({"backendIds":["typescript"],"legacyLanguages":["rust"]}));
    host.projects.push(yoke::project_session::HostProject {
        name: "second".into(),
        root: second.path().into(),
        read_only: true,
        query_only: false,
    });
    let mut session = Session::open(
        language_engine(root.path(), &log),
        host.clone(),
        Some("projects"),
        &d(),
    )
    .unwrap();
    let (address, token) = start(&mut session);
    let old = view(address, &token);
    assert_eq!(
        call(
            &mut session,
            address,
            &token,
            json!({"kind":"selectLanguageServers","selection":{"kind":"legacy","languages":[]}})
        )
        .0,
        200
    );
    let stale = wire(
        address,
        &token,
        request(&old, json!({"kind":"resetLanguageServers"})),
    );
    let client = thread::spawn(move || raw(address, stale));
    while !client.is_finished() {
        session.poll_dashboard_management(&d()).unwrap();
        thread::sleep(Duration::from_millis(1));
    }
    assert_eq!(client.join().unwrap().0, 409);
    assert_eq!(
        call(
            &mut session,
            address,
            &token,
            json!({"kind":"activateProject","project":"second"})
        )
        .0,
        200
    );
    assert!(
        session
            .active_engine()
            .unwrap()
            .servers
            .contains_key("rust")
    );
    assert_eq!(call(&mut session,address,&token,json!({"kind":"selectLanguageServers","selection":{"kind":"backends","ids":["typescript"]}})).0,200);
    assert_eq!(
        call(
            &mut session,
            address,
            &token,
            json!({"kind":"activateProject","project":"default"})
        )
        .0,
        200
    );
    assert!(session.active_engine().unwrap().servers.is_empty());
    drop(session);
    let mut resumed = Session::open(
        language_engine(root.path(), &log),
        host,
        Some("projects"),
        &d(),
    )
    .unwrap();
    assert!(resumed.active_engine().unwrap().servers.is_empty());
    resumed
        .invoke("activate", &json!({"project":"second"}), &d())
        .unwrap();
    assert_eq!(
        resumed.invoke("configuration", &json!({}), &d()).unwrap()["dashboardLanguageSelection"]["selectedBackends"],
        json!(["typescript"])
    );
    assert!(!log.exists());
    for path in [root.path(), second.path()] {
        assert_eq!(
            fs::read(path.join("main.rs")).unwrap(),
            "// π\r\nfn example() {}\r\n".as_bytes()
        );
    }
}
#[test]
fn dashboard_language_registry_mutation_is_not_accepted_as_an_old_host_grant() {
    let root = tempfile::tempdir().unwrap();
    let log = root.path().join("owned-log");
    let host = language_host(json!({"legacyLanguages":["rust"]}));
    let mut session = Session::open(
        language_engine(root.path(), &log),
        host.clone(),
        Some("pinned"),
        &d(),
    )
    .unwrap();
    let (address, token) = start(&mut session);
    session
        .engine
        .servers
        .get_mut("rust")
        .unwrap()
        .args
        .push("changed".into());
    assert_eq!(
        call(
            &mut session,
            address,
            &token,
            json!({"kind":"resetLanguageServers"})
        )
        .0,
        409
    );
    assert!(session.invoke("configuration", &json!({}), &d()).is_err());
    drop(session);
    let mut changed = language_engine(root.path(), &log);
    changed
        .servers
        .get_mut("rust")
        .unwrap()
        .args
        .push("changed".into());
    assert!(Session::open(changed, host, Some("pinned"), &d()).is_err());
    assert!(!log.exists());
}
#[test]
fn live_memory_create_read_write_rename_delete_and_head_cas_preserve_source_bytes() {
    let (root, mut session) = session(3000);
    let original = fs::read(root.path().join("main.rs")).unwrap();
    let (address, token) = start(&mut session);
    let (status, created) = call(
        &mut session,
        address,
        &token,
        json!({"kind":"createMemory","name":"owned/π","content":"first\r\n<script>inert</script>"}),
    );
    assert_eq!(status, 200, "{created}");
    let first = created["data"]["note"]["id"].as_str().unwrap().to_owned();
    assert_eq!(
        call(
            &mut session,
            address,
            &token,
            json!({"kind":"createMemory","name":"owned/π","content":"lost update"})
        )
        .0,
        409
    );
    let (status, read) = call(
        &mut session,
        address,
        &token,
        json!({"kind":"readMemory","name":"owned/π"}),
    );
    assert_eq!(status, 200);
    assert_eq!(
        read["data"]["note"]["content"],
        "first\r\n<script>inert</script>"
    );
    let (status, written) = call(
        &mut session,
        address,
        &token,
        json!({"kind":"writeMemory","name":"owned/π","content":"second","expectedRevision":first}),
    );
    assert_eq!(status, 200, "{written}");
    let second = written["data"]["note"]["id"].as_str().unwrap().to_owned();
    assert_eq!(call(&mut session,address,&token,json!({"kind":"writeMemory","name":"owned/π","content":"stale","expectedRevision":first})).0,409);
    let (status, renamed) = call(
        &mut session,
        address,
        &token,
        json!({"kind":"renameMemory","oldName":"owned/π","newName":"owned/new","expectedRevision":second}),
    );
    assert_eq!(status, 200, "{renamed}");
    let (_, read) = call(
        &mut session,
        address,
        &token,
        json!({"kind":"readMemory","name":"owned/new"}),
    );
    let revision = read["data"]["note"]["id"].as_str().unwrap();
    assert_eq!(
        call(
            &mut session,
            address,
            &token,
            json!({"kind":"deleteMemory","name":"owned/new","expectedRevision":revision})
        )
        .0,
        200
    );
    assert_eq!(
        call(
            &mut session,
            address,
            &token,
            json!({"kind":"readMemory","name":"owned/new"})
        )
        .0,
        404
    );
    assert_eq!(
        call(
            &mut session,
            address,
            &token,
            json!({"kind":"createMemory","name":"owned/new","content":"recreated"})
        )
        .0,
        200
    );
    assert_eq!(fs::read(root.path().join("main.rs")).unwrap(), original);
}
#[test]
fn queued_expiry_and_stale_session_requests_never_apply_after_their_caller_exits() {
    let (_root, mut session) = session(100);
    let (address, token) = start(&mut session);
    let state = view(address, &token);
    let bytes = wire(
        address,
        &token,
        request(
            &state,
            json!({"kind":"createMemory","name":"late","content":"must not publish"}),
        ),
    );
    let client = thread::spawn(move || raw(address, bytes));
    let (status, value) = client.join().unwrap();
    assert_eq!(status, 408);
    assert_eq!(value["notApplied"], true);
    session.poll_dashboard_management(&d()).unwrap();
    assert!(
        session.invoke("notes.list", &json!({}), &d()).unwrap()["items"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    let mut stale = request(&state, json!({"kind":"switchModes","modes":["editing"]}));
    stale["sessionStateSha256"] = json!("f".repeat(64));
    let bytes = wire(address, &token, stale);
    let client = thread::spawn(move || raw(address, bytes));
    while !client.is_finished() {
        session.poll_dashboard_management(&d()).unwrap();
        thread::sleep(Duration::from_millis(1));
    }
    assert_eq!(client.join().unwrap().0, 409);
}
#[test]
fn live_mode_changes_update_the_same_owner_and_cannot_bypass_no_memories_or_global_scope() {
    let (_root, mut session) = session(3000);
    let (address, token) = start(&mut session);
    let (status, value) = call(
        &mut session,
        address,
        &token,
        json!({"kind":"switchModes","modes":["editing","no-memories"]}),
    );
    assert_eq!(status, 200, "{value}");
    assert!(
        session.invoke("configuration", &json!({}), &d()).unwrap()["modes"]
            .as_array()
            .unwrap()
            .iter()
            .any(|m| m == "no-memories")
    );
    assert_ne!(
        call(
            &mut session,
            address,
            &token,
            json!({"kind":"createMemory","name":"blocked","content":"refuse"})
        )
        .0,
        200
    );
    assert_eq!(
        call(
            &mut session,
            address,
            &token,
            json!({"kind":"switchModes","modes":["editing"]})
        )
        .0,
        200
    );
    assert_eq!(
        call(
            &mut session,
            address,
            &token,
            json!({"kind":"createMemory","name":"owned","content":"current"})
        )
        .0,
        200
    );
    let mut bad = request(
        &view(address, &token),
        json!({"kind":"readMemory","name":"owned"}),
    );
    bad["action"]["scope"] = json!("global");
    assert_eq!(raw(address, wire(address, &token, bad)).0, 400);
}
#[test]
fn management_requires_a_host_driver_and_absent_setting_keeps_original_serialization() {
    let (_root, mut session) = session(3000);
    assert!(session.invoke("openDashboard", &json!({}), &d()).is_err());
    let config = yoke::session_dashboard::Config::default();
    let encoded = serde_json::to_value(config).unwrap();
    assert!(encoded.get("management").is_none());
}

#[test]
fn management_auth_origin_json_and_framing_refuse_before_any_queued_write() {
    let (root, mut session) = session(3000);
    let original = fs::read(root.path().join("main.rs")).unwrap();
    let (address, token) = start(&mut session);
    let selected = view(address, &token);
    let message = request(
        &selected,
        json!({"kind":"createMemory","name":"refused","content":"must not write"}),
    );
    let valid = String::from_utf8(wire(address, &token, message)).unwrap();
    for (bytes, expected) in [
        (
            valid.replace(&format!("Origin: http://{address}\r\n"), ""),
            403,
        ),
        (
            valid.replace(
                &format!("Origin: http://{address}"),
                "Origin: https://untrusted.invalid",
            ),
            403,
        ),
        (
            valid.replace(
                &format!("Bearer {token}"),
                &format!("Bearer {}", "0".repeat(64)),
            ),
            401,
        ),
        (
            valid.replace("Content-Type: application/json", "Content-Type: text/plain"),
            415,
        ),
        (valid.replace("Content-Length: ", "Content-Length: 0"), 413),
        (
            valid.replace(
                "Content-Type: application/json\r\n",
                "Content-Type: application/json\r\nTransfer-Encoding: chunked\r\n",
            ),
            400,
        ),
        (valid.clone() + "GET /api/state HTTP/1.1\r\n\r\n", 400),
    ] {
        assert_eq!(raw(address, bytes.into_bytes()).0, expected);
    }
    for body in [
        br#"{"id":"a","id":"b"}"#.as_slice(),
        br#"{"action":{"kind":"execute","command":"must never execute"}}"#.as_slice(),
    ] {
        let mut bytes = format!("POST /api/manage HTTP/1.1\r\nHost: {address}\r\nOrigin: http://{address}\r\nAuthorization: Bearer {token}\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n",body.len()).into_bytes();
        bytes.extend(body);
        assert_eq!(raw(address, bytes).0, 400);
    }
    session.poll_dashboard_management(&d()).unwrap();
    let (status, result) = call(
        &mut session,
        address,
        &token,
        json!({"kind":"listMemories"}),
    );
    assert_eq!(status, 200);
    assert!(result["data"]["items"].as_array().unwrap().is_empty());
    assert_eq!(fs::read(root.path().join("main.rs")).unwrap(), original);
}

#[test]
fn management_host_flags_and_read_only_memory_policy_are_independent() {
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("main.rs"), b"fn source() {}\r\n").unwrap();
    let host: HostProfile = serde_json::from_value(json!({"dashboard":{"management":{"sessionControls":false}},"steering":{},"notesPolicy":{"writable":false}})).unwrap();
    let mut session = Session::open(
        Engine::open(root.path()).unwrap(),
        host,
        Some("restricted"),
        &d(),
    )
    .unwrap();
    session.enable_dashboard_management(&d()).unwrap();
    let result = session.invoke("openDashboard", &json!({}), &d()).unwrap();
    assert_eq!(result["capabilities"]["memoryEditing"], false);
    assert_eq!(result["capabilities"]["sessionControls"], false);
    let url = reqwest::Url::parse(result["url"].as_str().unwrap()).unwrap();
    let address = format!("127.0.0.1:{}", url.port().unwrap())
        .parse()
        .unwrap();
    let token = url.fragment().unwrap();
    assert_eq!(view(address, token)["capabilities"]["memoryEditing"], false);
    for action in [
        json!({"kind":"createMemory","name":"denied","content":"no permission"}),
        json!({"kind":"switchModes","modes":["editing"]}),
    ] {
        let (status, result) = call(&mut session, address, token, action);
        assert_eq!(status, 403, "{result}");
        assert_eq!(result["notApplied"], true);
    }
}

fn delayed_exchange(address: SocketAddr, bytes: Vec<u8>) -> std::io::Result<(u16, Value)> {
    let mut stream = TcpStream::connect_timeout(&address, Duration::from_secs(2))?;
    stream.set_read_timeout(Some(Duration::from_secs(5)))?;
    // Let the nonblocking listener accept before the first request byte exists.
    // Headers and body are separately delivered, as with a normal HTTP client.
    thread::sleep(Duration::from_millis(60));
    let header_end = bytes.windows(4).position(|b| b == b"\r\n\r\n").unwrap() + 4;
    let split = header_end / 2;
    stream.write_all(&bytes[..split])?;
    thread::sleep(Duration::from_millis(20));
    stream.write_all(&bytes[split..header_end])?;
    if header_end < bytes.len() {
        thread::sleep(Duration::from_millis(20));
        let split = header_end + (bytes.len() - header_end) / 2;
        stream.write_all(&bytes[header_end..split])?;
        thread::sleep(Duration::from_millis(20));
        stream.write_all(&bytes[split..])?;
    }
    stream.shutdown(Shutdown::Write)?;
    let mut response = Vec::new();
    if let Err(error) = stream.read_to_end(&mut response)
        && error.kind() != std::io::ErrorKind::ConnectionReset
    {
        return Err(error);
    }
    let end = response
        .windows(4)
        .position(|b| b == b"\r\n\r\n")
        .expect("Complete delayed response headers")
        + 4;
    let status = String::from_utf8_lossy(&response[..end])
        .split_whitespace()
        .nth(1)
        .unwrap()
        .parse()
        .unwrap();
    let value = serde_json::from_slice(&response[end..]).expect("Complete delayed JSON response");
    Ok((status, value))
}

#[test]
fn accepted_connections_wait_for_split_headers_and_body_under_the_original_deadline() {
    let (root, mut session) = session(3000);
    let source = fs::read(root.path().join("main.rs")).unwrap();
    let (address, token) = start(&mut session);
    let get = format!(
        "GET /api/state HTTP/1.1\r\nHost: {address}\r\nAuthorization: Bearer {token}\r\n\r\n"
    )
    .into_bytes();
    let (status, selected) = delayed_exchange(address, get)
        .expect("Delayed GET must remain connected until its bounded request deadline");
    assert_eq!(status, 200);
    let body = request(
        &selected,
        json!({"kind":"createMemory","name":"split/π","content":"inert 🦀\r\n<script>text</script>"}),
    );
    let bytes = wire(address, &token, body);
    let client = thread::spawn(move || delayed_exchange(address, bytes));
    let until = Instant::now() + Duration::from_secs(8);
    while !client.is_finished() {
        session.poll_dashboard_management(&d()).unwrap();
        assert!(Instant::now() < until);
        thread::sleep(Duration::from_millis(1));
    }
    let (status, created) = client
        .join()
        .unwrap()
        .expect("Delayed management body must be read completely");
    assert_eq!(status, 200, "{created}");
    assert_eq!(
        created["data"]["note"]["content"],
        "inert 🦀\r\n<script>text</script>"
    );
    assert_eq!(fs::read(root.path().join("main.rs")).unwrap(), source);
}

#[test]
fn incomplete_request_cannot_extend_the_host_dashboard_lifetime() {
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("main.rs"), b"fn source() {}\r\n").unwrap();
    let host: HostProfile = serde_json::from_value(json!({"dashboard":{"management":{},"requestTimeoutMs":3000,"lifetimeMs":300},"steering":{}})).unwrap();
    let mut session = Session::open(
        Engine::open(root.path()).unwrap(),
        host,
        Some("lifetime"),
        &d(),
    )
    .unwrap();
    let (address, token) = start(&mut session);
    let mut stream = TcpStream::connect_timeout(&address, Duration::from_secs(2)).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    let began = Instant::now();
    // Keep the write half open with an incomplete header. Its per-request
    // timeout is longer than the explicitly shorter private capability lifetime.
    stream
        .write_all(
            format!(
                "GET /api/state HTTP/1.1\r\nHost: {address}\r\nAuthorization: Bearer {token}\r\n"
            )
            .as_bytes(),
        )
        .unwrap();
    let mut response = Vec::new();
    if let Err(error) = stream.read_to_end(&mut response) {
        assert_eq!(error.kind(), std::io::ErrorKind::ConnectionReset);
    }
    assert!(
        began.elapsed() < Duration::from_millis(1500),
        "The original host lifetime must cap an already accepted request"
    );
    let header = String::from_utf8_lossy(&response);
    assert_eq!(header.split_whitespace().nth(1), Some("408"));
    assert!(!header.contains("native_session_dashboard_v1"));
}

#[cfg(windows)]
fn document_session(writable: bool) -> (tempfile::TempDir, Session, std::path::PathBuf) {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("source");
    fs::create_dir(&source).unwrap();
    fs::write(source.join("main.rs"), b"fn untouched() {}\r\n").unwrap();
    let selected = temp.path().join("host.yaml");
    fs::write(&selected, "\u{feff}# original\r\nvalue: inert\r\n").unwrap();
    let journal = temp.path().join("journals");
    fs::create_dir(&journal).unwrap();
    let host:HostProfile=serde_json::from_value(json!({"dashboard":{"management":{"sessionControls":false,"memories":false,"documents":{"documents":[{"id":"host","label":"Owned host settings","path":selected,"scope":{"kind":"global"},"writable":writable}],"journalRoot":journal}}}})).unwrap();
    let session = Session::open(
        Engine::open(&source).unwrap(),
        host,
        Some("documents"),
        &d(),
    )
    .unwrap();
    (temp, session, selected)
}
#[cfg(windows)]
fn document_call(
    s: &mut Session,
    address: SocketAddr,
    token: &str,
    request: Value,
) -> (u16, Value) {
    call(
        s,
        address,
        token,
        json!({"kind":"document","request":request}),
    )
}
#[cfg(windows)]
#[test]
fn selected_global_document_requires_reviewed_plan_and_keeps_source_and_running_profile() {
    let (temp, mut session, path) = document_session(true);
    let (address, token) = start(&mut session);
    let state = view(address, &token);
    assert_eq!(state["capabilities"]["configurationReading"], true);
    assert_eq!(state["capabilities"]["configurationEditing"], true);
    assert_eq!(state["capabilities"]["projectConfigurationEditing"], false);
    let reopened = session.invoke("openDashboard", &json!({}), &d()).unwrap();
    assert_eq!(reopened["capabilities"]["configurationReading"], true);
    assert_eq!(reopened["capabilities"]["configurationEditing"], true);
    assert_eq!(state["view"]["management"]["sessionControls"], false);
    assert_eq!(
        state["view"]["management"]["configurationFiles"]["choices"][0]["writable"],
        true
    );
    let original = session.invoke("configuration", &json!({}), &d()).unwrap();
    let (status, loaded) = document_call(
        &mut session,
        address,
        &token,
        json!({"kind":"read","documentId":"host"}),
    );
    assert_eq!(status, 200, "{loaded}");
    let content = "\u{feff}# explicit proposal\r\ncommand: <script>inert</script>\r\n";
    let before = fs::read(&path).unwrap();
    let revision = loaded["data"]["document"]["revision"].clone();
    let (status, planned) = document_call(
        &mut session,
        address,
        &token,
        json!({"kind":"plan","documentId":"host","expectedRevision":revision,"content":content}),
    );
    assert_eq!(status, 200, "{planned}");
    assert_eq!(planned["fileChanged"], false);
    assert_eq!(fs::read(&path).unwrap(), before);
    let id = planned["data"]["plan"]["id"].clone();
    let (status, result) = document_call(
        &mut session,
        address,
        &token,
        json!({"kind":"apply","documentId":"host","planId":id}),
    );
    assert_eq!(status, 200, "{result}");
    assert_eq!(result["fileChanged"], true);
    assert_eq!(result["sourceWritten"], false);
    assert_eq!(fs::read(&path).unwrap(), content.as_bytes());
    assert_eq!(
        fs::read(temp.path().join("source/main.rs")).unwrap(),
        b"fn untouched() {}\r\n"
    );
    assert_eq!(
        session.invoke("configuration", &json!({}), &d()).unwrap(),
        original
    );
    let (status, replay) = document_call(
        &mut session,
        address,
        &token,
        json!({"kind":"apply","documentId":"host","planId":id}),
    );
    assert_eq!(status, 422, "{replay}");
    assert_eq!(fs::read(&path).unwrap(), content.as_bytes());
    let (status, pending) = document_call(
        &mut session,
        address,
        &token,
        json!({"kind":"pending","documentId":"host"}),
    );
    assert_eq!(status, 200, "{pending}");
    assert_eq!(pending["data"]["journals"], json!([]));
}
#[cfg(windows)]
#[test]
fn document_selection_does_not_grant_arbitrary_paths_or_readonly_writes() {
    let (_temp, mut session, path) = document_session(false);
    let (address, token) = start(&mut session);
    assert_eq!(
        view(address, &token)["capabilities"]["configurationReading"],
        true
    );
    assert_eq!(
        view(address, &token)["capabilities"]["configurationEditing"],
        false
    );
    let before = fs::read(&path).unwrap();
    let (status, loaded) = document_call(
        &mut session,
        address,
        &token,
        json!({"kind":"read","documentId":"host"}),
    );
    assert_eq!(status, 200);
    for request in [
        json!({"kind":"read","documentId":"outside"}),
        json!({"kind":"read","documentId":"../host"}),
        json!({"kind":"read","documentId":"host","path":path}),
        json!({"kind":"plan","documentId":"host","expectedRevision":loaded["data"]["document"]["revision"],"content":"unapproved"}),
        json!({"kind":"apply","documentId":"host","planId":"a".repeat(64)}),
        json!({"kind":"pending","documentId":"host"}),
    ] {
        let (status, result) = document_call(&mut session, address, &token, request);
        assert_ne!(status, 200, "{result}");
        assert_eq!(fs::read(&path).unwrap(), before);
    }
}
#[cfg(windows)]
#[test]
fn competing_file_change_refuses_reviewed_document_plan_without_overwriting_new_bytes() {
    let (_temp, mut session, path) = document_session(true);
    let (address, token) = start(&mut session);
    let (_, loaded) = document_call(
        &mut session,
        address,
        &token,
        json!({"kind":"read","documentId":"host"}),
    );
    let (status, planned) = document_call(
        &mut session,
        address,
        &token,
        json!({"kind":"plan","documentId":"host","expectedRevision":loaded["data"]["document"]["revision"],"content":"proposal\r\n"}),
    );
    assert_eq!(status, 200);
    fs::write(&path, b"competing saved bytes\r\n").unwrap();
    let (status, result) = document_call(
        &mut session,
        address,
        &token,
        json!({"kind":"apply","documentId":"host","planId":planned["data"]["plan"]["id"]}),
    );
    assert_eq!(status, 422, "{result}");
    assert_eq!(fs::read(&path).unwrap(), b"competing saved bytes\r\n");
}
#[cfg(windows)]
#[test]
fn project_document_payloads_follow_active_project_and_never_gain_workspace_write_authority() {
    let temp = tempfile::tempdir().unwrap();
    let primary = temp.path().join("primary");
    let secondary = temp.path().join("secondary");
    let journal = temp.path().join("journal");
    for root in [&primary, &secondary, &journal] {
        fs::create_dir(root).unwrap();
    }
    fs::write(primary.join("project.yaml"), b"primary: original\r\n").unwrap();
    fs::write(secondary.join("project.yaml"), b"secondary: original\r\n").unwrap();
    let host:HostProfile=serde_json::from_value(json!({"projects":[{"name":"other","root":secondary}],"dashboard":{"management":{"documents":{"documents":[{"id":"primary","label":"Primary","path":primary.join("project.yaml"),"scope":{"kind":"project","project":"default"},"writable":true},{"id":"secondary","label":"Secondary","path":secondary.join("project.yaml"),"scope":{"kind":"project","project":"other"},"writable":true}],"journalRoot":journal}}}})).unwrap();
    let mut session = Session::open(
        Engine::open(&primary).unwrap(),
        host,
        Some("project-documents"),
        &d(),
    )
    .unwrap();
    let (address, token) = start(&mut session);
    let (_, loaded) = document_call(
        &mut session,
        address,
        &token,
        json!({"kind":"read","documentId":"primary"}),
    );
    assert_eq!(
        loaded["data"]["document"]["content"],
        "primary: original\r\n"
    );
    let (status, _) = document_call(
        &mut session,
        address,
        &token,
        json!({"kind":"read","documentId":"secondary"}),
    );
    assert_eq!(status, 403);
    let (status, _) = document_call(
        &mut session,
        address,
        &token,
        json!({"kind":"plan","documentId":"primary","expectedRevision":loaded["data"]["document"]["revision"],"content":"unapproved"}),
    );
    assert_eq!(status, 422);
    assert_eq!(
        view(address, &token)["view"]["management"]["configurationFiles"]["choices"][0]["writable"],
        false
    );
    let old = view(address, &token);
    let (status, result) = call(
        &mut session,
        address,
        &token,
        json!({"kind":"activateProject","project":"other"}),
    );
    assert_eq!(status, 200, "{result}");
    let (_, loaded) = document_call(
        &mut session,
        address,
        &token,
        json!({"kind":"read","documentId":"secondary"}),
    );
    assert_eq!(
        loaded["data"]["document"]["content"],
        "secondary: original\r\n"
    );
    let stale = wire(
        address,
        &token,
        request(
            &old,
            json!({"kind":"document","request":{"kind":"read","documentId":"primary"}}),
        ),
    );
    let client = thread::spawn(move || raw(address, stale));
    while !client.is_finished() {
        session.poll_dashboard_management(&d()).unwrap();
        thread::sleep(Duration::from_millis(1));
    }
    assert_eq!(client.join().unwrap().0, 409);
    assert_eq!(
        fs::read(primary.join("project.yaml")).unwrap(),
        b"primary: original\r\n"
    );
    assert_eq!(
        fs::read(secondary.join("project.yaml")).unwrap(),
        b"secondary: original\r\n"
    );
}

#[cfg(windows)]
#[test]
fn refreshed_envelope_cannot_rebind_an_old_file_proposal_to_a_new_session_revision() {
    let (_temp, mut session, path) = document_session(true);
    let (address, token) = start(&mut session);
    let before = fs::read(&path).unwrap();
    let (_, loaded) = document_call(
        &mut session,
        address,
        &token,
        json!({"kind":"read","documentId":"host"}),
    );
    let (status, planned) = document_call(
        &mut session,
        address,
        &token,
        json!({"kind":"plan","documentId":"host","expectedRevision":loaded["data"]["document"]["revision"],"content":"old reviewed proposal\r\n"}),
    );
    assert_eq!(status, 200);
    session
        .invoke("switchModes", &json!({"modes":["editing"]}), &d())
        .unwrap();
    let (status, result) = document_call(
        &mut session,
        address,
        &token,
        json!({"kind":"apply","documentId":"host","planId":planned["data"]["plan"]["id"]}),
    );
    assert_eq!(status, 422, "{result}");
    assert_eq!(fs::read(&path).unwrap(), before);
    let (status, pending) = document_call(
        &mut session,
        address,
        &token,
        json!({"kind":"pending","documentId":"host"}),
    );
    assert_eq!(status, 200);
    assert_eq!(pending["data"]["journals"], json!([]));
}

#[cfg(windows)]
#[test]
fn a_new_host_journal_selection_reports_the_original_file_authority_instead_of_empty_completion() {
    let (temp, mut first, path) = document_session(true);
    let (address, token) = start(&mut first);
    let (_, loaded) = document_call(
        &mut first,
        address,
        &token,
        json!({"kind":"read","documentId":"host"}),
    );
    let (_, planned) = document_call(
        &mut first,
        address,
        &token,
        json!({"kind":"plan","documentId":"host","expectedRevision":loaded["data"]["document"]["revision"],"content":"original authority saved\r\n"}),
    );
    let (status, result) = document_call(
        &mut first,
        address,
        &token,
        json!({"kind":"apply","documentId":"host","planId":planned["data"]["plan"]["id"]}),
    );
    assert_eq!(status, 200, "{result}");
    let journal = temp.path().join("different-journal");
    fs::create_dir(&journal).unwrap();
    let host:HostProfile=serde_json::from_value(json!({"dashboard":{"management":{"documents":{"documents":[{"id":"host","label":"Changed host selection","path":path,"scope":{"kind":"global"},"writable":true}],"journalRoot":journal}}}})).unwrap();
    let mut second = Session::open(
        Engine::open(temp.path().join("source")).unwrap(),
        host,
        Some("different-document-authority"),
        &d(),
    )
    .unwrap();
    let (address, token) = start(&mut second);
    let (status, pending) = document_call(
        &mut second,
        address,
        &token,
        json!({"kind":"pending","documentId":"host"}),
    );
    assert_eq!(status, 200, "{pending}");
    assert_eq!(pending["data"]["blockedByDifferentAuthority"], true);
    assert!(yoke::model::valid_id(
        pending["data"]["originalAuthority"].as_str().unwrap()
    ));
    let (status, loaded) = document_call(
        &mut second,
        address,
        &token,
        json!({"kind":"read","documentId":"host"}),
    );
    assert_eq!(status, 200);
    let (_, planned) = document_call(
        &mut second,
        address,
        &token,
        json!({"kind":"plan","documentId":"host","expectedRevision":loaded["data"]["document"]["revision"],"content":"new authority must not silently adopt\r\n"}),
    );
    let (status, result) = document_call(
        &mut second,
        address,
        &token,
        json!({"kind":"apply","documentId":"host","planId":planned["data"]["plan"]["id"]}),
    );
    assert_eq!(status, 422, "{result}");
    assert_eq!(fs::read(&path).unwrap(), b"original authority saved\r\n");
    assert_eq!(
        fs::read(temp.path().join("source/main.rs")).unwrap(),
        b"fn untouched() {}\r\n"
    );
}

#[cfg(windows)]
fn log_session(selected: bool, denied: bool) -> (tempfile::TempDir, Session, std::path::PathBuf) {
    let fixture = tempfile::tempdir().unwrap();
    let source = fixture.path().join("source");
    fs::create_dir(&source).unwrap();
    fs::write(source.join("main.rs"), b"fn untouched() {}\r\n").unwrap();
    let path = fixture.path().join("private.log");
    let mut host = json!({"dashboard":{"management":{"sessionControls":false,"memories":false}},"steering":{}});
    if selected {
        host["dashboard"]["management"]["rawLogs"] = json!({"logs":[{"id":"private","label":"Owned private log","path":path}],"maxWindowBytes":64});
    }
    if denied {
        host["steering"]["global"] = json!({"excludedTools":["ci_session"]});
    }
    let session = Session::open(
        Engine::open(&source).unwrap(),
        serde_json::from_value(host).unwrap(),
        Some("private-logs"),
        &d(),
    )
    .unwrap();
    (fixture, session, path)
}
#[cfg(windows)]
#[test]
fn private_raw_logs_require_explicit_read_and_never_enter_activity_or_state() {
    use base64::Engine as _;
    let (fixture, mut session, path) = log_session(true, false);
    let (address, token) = start(&mut session);
    let before = view(address, &token);
    assert_eq!(before["capabilities"]["rawLogs"], true);
    assert_eq!(
        before["view"]["management"]["rawLogs"]["automaticCapture"],
        false
    );
    assert!(!path.exists());
    let (status, missing) = call(
        &mut session,
        address,
        &token,
        json!({"kind":"readLog","logId":"private"}),
    );
    assert_eq!(status, 422, "{missing}");
    assert!(!path.exists());
    let bytes = b"owned private marker\r\n<script>inert()</script>\xff";
    fs::write(&path, bytes).unwrap();
    let (status, read) = call(
        &mut session,
        address,
        &token,
        json!({"kind":"readLog","logId":"private"}),
    );
    assert_eq!(status, 200, "{read}");
    assert_eq!(read["fileChanged"], false);
    assert_eq!(read["metadataChanged"], false);
    assert_eq!(read["sourceWritten"], false);
    let o = &read["data"]["observation"];
    assert_eq!(o["textLossy"], true);
    assert_eq!(o["immutableSnapshot"], false);
    assert_eq!(
        base64::engine::general_purpose::STANDARD
            .decode(o["base64"].as_str().unwrap())
            .unwrap(),
        bytes
    );
    let after = view(address, &token);
    assert_eq!(
        after["view"]["management"]["sessionStateSha256"],
        before["view"]["management"]["sessionStateSha256"]
    );
    let serialized = serde_json::to_string(&after).unwrap();
    assert!(!serialized.contains("owned private marker"));
    assert!(!serialized.contains("<script>"));
    assert!(
        after["activity"]
            .as_array()
            .unwrap()
            .iter()
            .any(|r| r["operation"] == "dashboard.logs.read")
    );
    assert_eq!(fs::read(&path).unwrap(), bytes);
    assert_eq!(
        fs::read(fixture.path().join("source/main.rs")).unwrap(),
        b"fn untouched() {}\r\n"
    );
}
#[cfg(windows)]
#[test]
fn raw_log_selection_does_not_grant_paths_larger_windows_or_other_session_access() {
    let (_fixture, mut session, path) = log_session(true, false);
    fs::write(&path, b"private original").unwrap();
    let (address, token) = start(&mut session);
    for action in [
        json!({"kind":"readLog","logId":"private","path":path}),
        json!({"kind":"readLog","logId":"../outside"}),
        json!({"kind":"readLog","logId":"private","maxBytes":0}),
        json!({"kind":"readLog","logId":"private","maxBytes":65}),
        json!({"kind":"readLog","logId":"not-selected"}),
    ] {
        let (status, refused) = call(&mut session, address, &token, action);
        assert!(status >= 400, "{refused}");
        assert!(refused["data"].is_null());
    }
    for (selected, denied) in [(false, false), (true, true)] {
        let (_other_fixture, mut other, other_path) = log_session(selected, denied);
        fs::write(&other_path, b"different private original").unwrap();
        let (other_address, other_token) = start(&mut other);
        let (status, refused) = call(
            &mut other,
            other_address,
            &other_token,
            json!({"kind":"readLog","logId":"private"}),
        );
        assert_eq!(status, 403, "{refused}");
        let wrong = raw(
            other_address,
            wire(
                other_address,
                &token,
                request(
                    &view(other_address, &other_token),
                    json!({"kind":"readLog","logId":"private"}),
                ),
            ),
        );
        assert_eq!(wrong.0, 401);
        assert_eq!(fs::read(other_path).unwrap(), b"different private original");
    }
    assert_eq!(fs::read(path).unwrap(), b"private original");
}
#[test]
fn absent_or_null_raw_log_grant_preserves_serialized_management_defaults() {
    let absent: yoke::session_dashboard::ManagementConfig =
        serde_json::from_value(json!({})).unwrap();
    let null: yoke::session_dashboard::ManagementConfig =
        serde_json::from_value(json!({"rawLogs":null})).unwrap();
    assert_eq!(absent, null);
    let encoded = serde_json::to_value(absent).unwrap();
    assert!(encoded.get("rawLogs").is_none());
    assert!(
        serde_json::from_value::<yoke::session_dashboard::ManagementConfig>(
            json!({"rawLogs":{"logs":[],"command":"anything"}})
        )
        .is_err()
    );
}

#[cfg(windows)]
#[test]
fn pending_host_history_is_explicitly_incomplete_and_never_an_empty_complete_recovery_list() {
    let (temp, mut session, path) = document_session(true);
    let (address, token) = start(&mut session);
    let (_, loaded) = document_call(
        &mut session,
        address,
        &token,
        json!({"kind":"read","documentId":"host"}),
    );
    let (_, planned) = document_call(
        &mut session,
        address,
        &token,
        json!({"kind":"plan","documentId":"host","expectedRevision":loaded["data"]["document"]["revision"],"content":"owned saved configuration\r\n"}),
    );
    let (status, reply) = document_call(
        &mut session,
        address,
        &token,
        json!({"kind":"apply","documentId":"host","planId":planned["data"]["plan"]["id"]}),
    );
    assert_eq!(status, 200, "{reply}");
    let owner = fs::read_dir(temp.path())
        .unwrap()
        .map(|e| e.unwrap().path())
        .find(|p| {
            p.file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with(".yoke-doc-owner-")
        })
        .unwrap();
    let owner_name = owner.file_name().unwrap().to_str().unwrap();
    let target = owner_name
        .strip_prefix(".yoke-doc-owner-")
        .unwrap()
        .strip_suffix(".json")
        .unwrap();
    let marker = temp
        .path()
        .join(format!(".yoke-doc-history-{target}.pending.json"));
    fs::write(&marker, b"foreign pending marker must be preserved").unwrap();
    let original = fs::read(&path).unwrap();
    let (status, pending) = document_call(
        &mut session,
        address,
        &token,
        json!({"kind":"pending","documentId":"host"}),
    );
    assert_eq!(status, 200, "{pending}");
    assert_eq!(pending["data"]["blockedByHistoryArchive"], true);
    assert_eq!(pending["data"]["complete"], false);
    assert_eq!(pending["data"]["journals"], json!([]));
    let (status, _) = document_call(
        &mut session,
        address,
        &token,
        json!({"kind":"apply","documentId":"host","planId":planned["data"]["plan"]["id"]}),
    );
    assert_eq!(status, 422);
    assert_eq!(fs::read(path).unwrap(), original);
    assert_eq!(
        fs::read(marker).unwrap(),
        b"foreign pending marker must be preserved"
    );
}
