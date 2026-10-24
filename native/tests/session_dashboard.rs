use serde_json::{Value, json};
use std::{
    fs,
    io::{Read, Write},
    net::{Shutdown, SocketAddr, TcpStream},
    path::Path,
    time::{Duration, Instant},
};
use yoke::{
    Engine,
    capture::Deadline,
    project_session::{HostProfile, Session},
    session_dashboard::Config,
};
fn d() -> Deadline {
    Deadline::new(20000)
}
fn root() -> tempfile::TempDir {
    let r = tempfile::tempdir().unwrap();
    fs::write(
        r.path().join("main.rs"),
        "// owned \u{1f4a1} e\u{301}\r\nfn source() {}\r\n",
    )
    .unwrap();
    r
}
fn host(config: Config) -> HostProfile {
    serde_json::from_value(json!({"dashboard":config,"steering":{}})).unwrap()
}
fn open(root: &Path, profile: HostProfile, label: &str) -> Session {
    Session::open(Engine::open(root).unwrap(), profile, Some(label), &d()).unwrap()
}
fn invoke(s: &mut Session, operation: &str, args: Value) -> Value {
    s.invoke(operation, &args, &d()).unwrap()
}
fn start(s: &mut Session) -> (SocketAddr, String) {
    let info = invoke(s, "openDashboard", json!({}));
    assert_eq!(info["browserOpened"], false);
    assert_eq!(info["complete"], false);
    let url = reqwest::Url::parse(info["url"].as_str().unwrap()).unwrap();
    assert_eq!(url.host_str(), Some("127.0.0.1"));
    let token = url.fragment().unwrap().to_owned();
    assert_eq!(token.len(), 64);
    (
        format!("127.0.0.1:{}", url.port().unwrap())
            .parse()
            .unwrap(),
        token,
    )
}
fn raw(address: SocketAddr, request: &[u8]) -> (u16, String, Vec<u8>) {
    let mut stream = TcpStream::connect_timeout(&address, Duration::from_secs(3)).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(7)))
        .unwrap();
    stream.write_all(request).unwrap();
    stream.shutdown(Shutdown::Write).unwrap();
    let mut bytes = Vec::new();
    if let Err(error) = stream.read_to_end(&mut bytes) {
        assert_eq!(error.kind(), std::io::ErrorKind::ConnectionReset);
    }
    let end = bytes.windows(4).position(|s| s == b"\r\n\r\n").unwrap();
    let header = String::from_utf8(bytes[..end].to_vec()).unwrap();
    let status = header.split_whitespace().nth(1).unwrap().parse().unwrap();
    let length: usize = header
        .lines()
        .find_map(|line| line.strip_prefix("Content-Length: "))
        .unwrap()
        .parse()
        .unwrap();
    assert_eq!(
        bytes.len() - end - 4,
        length,
        "A reset must not hide a truncated HTTP response"
    );
    (status, header, bytes[end + 4..].to_vec())
}
fn get(address: SocketAddr, token: Option<&str>, path: &str) -> (u16, String, Vec<u8>) {
    let auth = token
        .map(|v| format!("Authorization: Bearer {v}\r\n"))
        .unwrap_or_default();
    raw(
        address,
        format!("GET {path} HTTP/1.1\r\nHost: {address}\r\n{auth}\r\n").as_bytes(),
    )
}
fn state(address: SocketAddr, token: &str) -> Value {
    let response = get(address, Some(token), "/api/state");
    assert_eq!(response.0, 200);
    serde_json::from_slice(&response.2).unwrap()
}
fn records(s: &Session) -> Vec<(String, String, String)> {
    let mut query = s
        .engine
        .store
        .db
        .prepare("SELECT kind,id,value FROM records ORDER BY kind,id")
        .unwrap();
    query
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap()
}
fn closed(address: SocketAddr) {
    let until = Instant::now() + Duration::from_secs(2);
    loop {
        if TcpStream::connect(address).is_err() {
            return;
        }
        assert!(Instant::now() < until, "dashboard listener did not close");
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn absent_null_and_ungranted_dashboard_keep_default_profile_configuration_and_records() {
    let r = root();
    let profile = HostProfile::default();
    let original = serde_json::to_vec(&profile).unwrap();
    let mut s = open(r.path(), profile, "legacy");
    let before = invoke(&mut s, "configuration", json!({}));
    let old = records(&s);
    assert!(s.invoke("openDashboard", &json!({}), &d()).is_err());
    assert_eq!(records(&s), old);
    let null: HostProfile = serde_json::from_value(json!({"dashboard":null})).unwrap();
    assert_eq!(serde_json::to_vec(&null).unwrap(), original);
    let mut resumed = open(r.path(), null, "legacy");
    assert_eq!(invoke(&mut resumed, "configuration", json!({})), before);
    assert_eq!(records(&resumed), old);
    assert!(
        yoke::agent_repl::invoke_facade(&mut resumed, "cfg.open_dashboard", json!({}), None, &d())
            .is_err()
    );
}
#[test]
fn private_bootstrap_and_exact_bound_state_never_expose_capability_or_source() {
    let r = root();
    let mut s = open(r.path(), host(Config::default()), "private-session");
    let before = records(&s);
    let (address, token) = start(&mut s);
    assert_eq!(
        records(&s),
        before,
        "Opening the dashboard must not persist activity or capability records"
    );
    let public = get(address, None, "/");
    assert_eq!(public.0, 200);
    let html = String::from_utf8(public.2).unwrap();
    assert!(
        !html.contains(&token)
            && !html.contains("private-session")
            && !html.contains("fn source()")
    );
    for required in [
        "Cache-Control: no-store",
        "Referrer-Policy: no-referrer",
        "X-Frame-Options: DENY",
        "frame-ancestors 'none'",
        "connect-src 'self'",
    ] {
        assert!(public.1.contains(required));
    }
    let current = state(address, &token);
    assert_eq!(current["view"]["session"], "private-session");
    assert_eq!(current["view"]["activeProject"], "default");
    assert_eq!(current["activity"], json!([]));
    assert_eq!(current["complete"], false);
    let bytes = serde_json::to_string(&current).unwrap();
    assert!(
        !bytes.contains(&token)
            && !bytes.contains("fn source()")
            && !bytes.contains(&r.path().display().to_string())
    );
    assert_eq!(records(&s), before);
    let (same, again) = start(&mut s);
    assert_eq!(same, address);
    assert!(again == token);
    drop(s);
    closed(address);
}
#[test]
fn authorization_origin_host_framing_routes_and_mutations_refuse() {
    let r = root();
    let mut s = open(r.path(), host(Config::default()), "protocol");
    let (address, token) = start(&mut s);
    let before = records(&s);
    assert_eq!(get(address, None, "/api/state").0, 401);
    assert_eq!(get(address, Some("wrong"), "/api/state").0, 401);
    for (tail, code) in [
        ("Origin: https://example.invalid\r\n", 403),
        ("hOsT: localhost\r\n", 400),
        ("Content-Length: 0\r\ncontent-length: 0\r\n", 400),
        ("Content-Length: 1\r\n", 400),
        ("Transfer-Encoding: chunked\r\n", 400),
        ("Expect: 100-continue\r\n", 400),
        ("Content-Length: +0\r\n", 400),
    ] {
        let reply=raw(address,format!("GET /api/state HTTP/1.1\r\nHost: {address}\r\nAuthorization: Bearer {token}\r\n{tail}\r\n").as_bytes());
        assert_eq!(reply.0, code);
    }
    assert_eq!(raw(address,format!("GET /api/state HTTP/1.1\r\nHost: localhost:{}\r\nAuthorization: Bearer {token}\r\n\r\n",address.port()).as_bytes()).0,403);
    for path in [
        "/api/state?root=..",
        "/%61pi/state",
        "/api/../state",
        "/api/write",
        "/api/shutdown",
    ] {
        assert_eq!(get(address, Some(&token), path).0, 404);
    }
    assert_eq!(raw(address,format!("POST /api/state HTTP/1.1\r\nHost: {address}\r\nAuthorization: Bearer {token}\r\nContent-Length: 0\r\n\r\n").as_bytes()).0,405);
    assert_eq!(raw(address,format!("GET /api/state HTTP/1.1\r\nHost: {address}\r\nAuthorization: Bearer {token}\r\n\r\nGET / HTTP/1.1\r\nHost: {address}\r\n\r\n").as_bytes()).0,400);
    assert_eq!(records(&s), before);
    assert_eq!(state(address, &token)["statistics"]["completed"], 0);
}
#[test]
fn structured_activity_has_actual_status_and_timing_without_arguments_results_or_unknown_text() {
    let r = root();
    let mut s = open(r.path(), host(Config::default()), "activity");
    let (address, token) = start(&mut s);
    invoke(
        &mut s,
        "notes.write",
        json!({"name":"owned","content":"PRIVATE_ARGUMENT_AND_RESULT <script>alert(1)</script> \u{1f4a1} e\u{301}"}),
    );
    invoke(&mut s, "notes.read", json!({"name":"owned"}));
    assert!(
        s.invoke(
            "PRIVATE_UNKNOWN_OPERATION",
            &json!({"secret":"PRIVATE_VALUE"}),
            &d()
        )
        .is_err()
    );
    let v = state(address, &token);
    assert_eq!(v["statistics"]["completed"], 3);
    assert_eq!(v["statistics"]["failed"], 1);
    assert_eq!(v["statistics"]["running"], 0);
    let rows = v["activity"].as_array().unwrap();
    assert_eq!(rows.len(), 3);
    assert_eq!(rows[0]["operation"], "notes.write");
    assert_eq!(rows[2]["operation"], "other_operation");
    assert_eq!(rows[2]["status"], "failed");
    assert!(
        rows.iter()
            .all(|row| row["durationMs"].is_u64() && row["startedAtMs"].is_u64())
    );
    let serialized = serde_json::to_string(&v).unwrap();
    assert!(!serialized.contains("PRIVATE_") && !serialized.contains("<script>"));
    assert_eq!(v["argumentsRecorded"], false);
    assert_eq!(v["resultsRecorded"], false);
}
#[test]
fn bounded_activity_retains_totals_and_exposes_dropped_rows() {
    let r = root();
    let mut s = open(
        r.path(),
        host(Config {
            activity_capacity: 2,
            ..Default::default()
        }),
        "capacity",
    );
    let (address, token) = start(&mut s);
    for _ in 0..5 {
        invoke(&mut s, "configuration", json!({}));
    }
    let v = state(address, &token);
    assert_eq!(v["statistics"]["completed"], 5);
    assert_eq!(v["statistics"]["droppedRows"], 3);
    assert_eq!(v["activity"].as_array().unwrap().len(), 2);
    assert_eq!(v["activity"][0]["id"], 4);
    assert_eq!(v["activity"][1]["id"], 5);
}
#[test]
fn current_modes_projects_and_public_tool_catalog_update_together() {
    let r = root();
    let second = root();
    let mut h = host(Config::default());
    h.projects.push(yoke::project_session::HostProject {
        name: "second".into(),
        root: second.path().to_path_buf(),
        read_only: true,
        query_only: false,
    });
    let mut s = open(r.path(), h, "current");
    let (address, token) = start(&mut s);
    invoke(&mut s, "activate", json!({"project":"second"}));
    invoke(&mut s, "switchModes", json!({"modes":["planning"]}));
    let v = state(address, &token);
    assert_eq!(v["view"]["activeProject"], "second");
    assert_eq!(v["view"]["modes"], json!(["planning"]));
    assert!(
        v["view"]["publicTools"]
            .as_array()
            .unwrap()
            .iter()
            .all(|name| name != "replace_content")
    );
    let mut protocol = yoke::protocol::Connection::serena(&mut s);
    protocol
        .message(br#"{"jsonrpc":"2.0","id":1,"method":"initialize"}"#)
        .unwrap();
    let result = protocol
        .message(br#"{"jsonrpc":"2.0","id":2,"method":"tools/list"}"#)
        .unwrap()
        .unwrap();
    let mut expected = result["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| row["name"].as_str().unwrap().to_owned())
        .collect::<Vec<_>>();
    expected.sort();
    assert_eq!(v["view"]["publicTools"], json!(expected));
}
#[test]
fn stale_session_rows_fail_closed_until_opened_from_current_host_session() {
    let r = root();
    let h = host(Config::default());
    let mut first = open(r.path(), h.clone(), "stale");
    let (address, token) = start(&mut first);
    let mut other = open(r.path(), h.clone(), "stale");
    invoke(&mut other, "switchModes", json!({"modes":["planning"]}));
    assert_eq!(get(address, Some(&token), "/api/state").0, 409);
    assert!(first.invoke("configuration", &json!({}), &d()).is_err());
    let mut current = open(r.path(), h, "stale");
    let (fresh, fresh_token) = start(&mut current);
    assert_eq!(
        state(fresh, &fresh_token)["view"]["modes"],
        json!(["planning"])
    );
    assert_eq!(get(address, Some(&token), "/api/state").0, 409);
    drop(current);
    closed(fresh);
    drop(first);
    closed(address);
}
#[test]
fn finite_lifetime_request_budget_and_session_drop_close_only_owned_listener() {
    let r = root();
    let mut s = open(
        r.path(),
        host(Config {
            max_requests: 2,
            ..Default::default()
        }),
        "request-limit",
    );
    let (address, token) = start(&mut s);
    assert_eq!(get(address, None, "/").0, 200);
    assert_eq!(get(address, Some(&token), "/api/state").0, 200);
    closed(address);
    let (fresh, fresh_token) = start(&mut s);
    assert!(fresh != address || fresh_token != token);
    drop(s);
    closed(fresh);
    let mut expired = open(
        r.path(),
        host(Config {
            lifetime_ms: 100,
            ..Default::default()
        }),
        "lifetime",
    );
    let (address, _) = start(&mut expired);
    closed(address);
    drop(expired);
}
#[test]
fn strict_host_schema_budget_and_yaml_authority_refusals_publish_no_sessions() {
    let r = root();
    for config in [
        json!({"root":"../outside"}),
        json!({"browserCommand":"source.exe"}),
        json!({"port":65536}),
        json!({"activityCapacity":0}),
        json!({"activityCapacity":1001}),
        json!({"maxRequests":0}),
        json!({"maxRequests":100001}),
        json!({"requestTimeoutMs":9}),
        json!({"requestTimeoutMs":5001}),
        json!({"lifetimeMs":99}),
        json!({"lifetimeMs":86400001}),
    ] {
        if let Ok(profile) = serde_json::from_value::<HostProfile>(json!({"dashboard":config})) {
            assert!(
                Session::open(
                    Engine::open(r.path()).unwrap(),
                    profile,
                    Some("invalid"),
                    &d()
                )
                .is_err()
            );
        }
    }
    let store = Engine::open(r.path()).unwrap();
    let count: i64 = store
        .store
        .db
        .query_row(
            "SELECT count(*) FROM records WHERE kind=?1",
            [yoke::project_session::RECORD_KIND],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(count, 0);
    let bytes = b"dashboard: {}\n";
    let hash = yoke::model::hash(bytes);
    assert!(
        yoke::session_config::project(
            &HostProfile::default(),
            &[yoke::session_config::Source {
                target: &yoke::session_config::Target::Global,
                expected_sha256: &hash,
                bytes
            }],
            &d()
        )
        .is_err()
    );
}
#[test]
fn external_facade_contexts_and_tool_exclusions_cannot_open_dashboard() {
    let r = root();
    let mut s = open(r.path(), host(Config::default()), "facade");
    assert!(
        yoke::agent_repl::invoke_facade(
            &mut s,
            "cfg.open_dashboard",
            json!({}),
            Some("second"),
            &d()
        )
        .is_err()
    );
    assert!(
        s.invoke("openDashboard", &json!({"root":"../outside"}), &d())
            .is_err()
    );
    let mut h: HostProfile = serde_json::from_value(
        json!({"dashboard":{},"steering":{"global":{"excludedTools":["open_dashboard"]}}}),
    )
    .unwrap();
    h.context = "desktop-app".into();
    let mut excluded = open(r.path(), h, "excluded");
    assert!(
        yoke::compatibility::invoke_serena(&mut excluded, "open_dashboard", &json!({}), &d())
            .is_err()
    );
    let info = yoke::agent_repl::invoke_facade(&mut s, "cfg.open_dashboard", json!({}), None, &d())
        .unwrap();
    assert_eq!(info["browserOpened"], false);
    assert_eq!(info["capabilities"]["configurationEditing"], false);
}

#[test]
fn actual_pending_provider_operation_is_visible_then_completes_without_result_capture() {
    let r = root();
    let mut engine = Engine::open(r.path()).unwrap();
    let node = std::process::Command::new("node")
        .args(["-p", "process.execPath"])
        .output()
        .unwrap();
    assert!(node.status.success());
    let fixture = r#"let b=Buffer.alloc(0);function reply(m,result){let s=Buffer.from(JSON.stringify({jsonrpc:'2.0',id:m.id,result}));process.stdout.write('Content-Length: '+s.length+'\r\n\r\n');process.stdout.write(s)}process.stdin.on('data',c=>{b=Buffer.concat([b,c]);for(;;){let i=b.indexOf('\r\n\r\n');if(i<0)return;let n=Number(/Content-Length: (\d+)/i.exec(b.subarray(0,i).toString())[1]);if(b.length<i+4+n)return;let m=JSON.parse(b.subarray(i+4,i+4+n));b=b.subarray(i+4+n);if(m.method==='initialize')reply(m,{capabilities:{hoverProvider:true,positionEncoding:'utf-16'}});else if(m.method==='textDocument/hover')setTimeout(()=>reply(m,{contents:'PRIVATE_PROVIDER_RESULT'}),350);else if(m.method==='shutdown')reply(m,null);else if(m.method==='exit')process.exit(0)}})"#;
    engine
        .register_configured_server(yoke::lsp::ConfiguredServer {
            server: yoke::lsp::Server {
                command: String::from_utf8(node.stdout).unwrap().trim().into(),
                args: vec!["-e".into(), fixture.into()],
                language: "rust".into(),
                trust: true,
            },
            configuration: yoke::lsp::HostConfiguration {
                passive_profile: Some(yoke::lsp::PassiveProfile::ContractFixture),
                ..Default::default()
            },
        })
        .unwrap();
    let mut s = Session::open(engine, host(Config::default()), Some("running"), &d()).unwrap();
    let (address, token) = start(&mut s);
    let worker = std::thread::spawn(move || {
        let result = {
            let mut protocol = yoke::protocol::Connection::session(&mut s);
            protocol
                .message(br#"{"jsonrpc":"2.0","id":1,"method":"initialize"}"#)
                .unwrap();
            let reply=protocol.message(br#"{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"ci_find","arguments":{"languageServer":{"path":"main.rs","offset":0,"operation":"hover"}}}}"#).unwrap().unwrap();
            let result: Value =
                serde_json::from_str(reply["result"]["content"][0]["text"].as_str().unwrap())
                    .unwrap();
            result
        };
        (s, result)
    });
    let until = Instant::now() + Duration::from_secs(5);
    loop {
        let v = state(address, &token);
        if v["statistics"]["running"] == 1 {
            assert_eq!(v["activity"][0]["status"], "running");
            break;
        }
        assert!(
            Instant::now() < until,
            "running provider operation was not observed"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
    let (s, result) = worker.join().unwrap();
    assert_eq!(result["status"], "ok");
    let v = state(address, &token);
    assert_eq!(v["statistics"]["running"], 0);
    assert_eq!(v["statistics"]["completed"], 1);
    assert_eq!(v["activity"][0]["status"], "completed");
    assert!(
        !serde_json::to_string(&v)
            .unwrap()
            .contains("PRIVATE_PROVIDER_RESULT")
    );
    drop(s);
    closed(address);
}

#[test]
fn actual_mcp_http_revocation_also_closes_its_dashboard_capability() {
    use yoke::{mcp_http, protocol::Profile};
    let r = root();
    let target = mcp_http::Target::new(r.path(), Profile::Session, host(Config::default()));
    let principal = mcp_http::Principal::new(
        "dashboard-owner",
        zeroize::Zeroizing::new(vec![73; 32]),
        target,
    )
    .unwrap();
    let server = mcp_http::serve(mcp_http::Config::default(), vec![principal]).unwrap();
    let endpoint = format!("http://{}/mcp", server.local_addr());
    let _ = rustls::crypto::ring::default_provider().install_default();
    let client = reqwest::blocking::Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(10))
        .build()
        .unwrap();
    let request = |sid: Option<&str>, body: Value| {
        let mut call = client
            .post(&endpoint)
            .header("Authorization", format!("Bearer {}", hex::encode([73; 32])))
            .header("Accept", "application/json, text/event-stream")
            .header("Content-Type", "application/json")
            .body(serde_json::to_vec(&body).unwrap());
        if let Some(sid) = sid {
            call = call
                .header("Mcp-Session-Id", sid)
                .header("Mcp-Protocol-Version", "2025-11-25");
        }
        let response = call.send().unwrap();
        let status = response.status().as_u16();
        let sid = response
            .headers()
            .get("mcp-session-id")
            .map(|v| v.to_str().unwrap().to_owned());
        let bytes = response.bytes().unwrap();
        (
            status,
            sid,
            if bytes.is_empty() {
                Value::Null
            } else {
                serde_json::from_slice(&bytes).unwrap()
            },
        )
    };
    let init = request(
        None,
        json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"owned-dashboard-test","version":"0.5.0"}}}),
    );
    assert_eq!(init.0, 200);
    let sid = init.1.unwrap();
    assert_eq!(
        request(
            Some(&sid),
            json!({"jsonrpc":"2.0","method":"notifications/initialized"})
        )
        .0,
        202
    );
    let reply = request(
        Some(&sid),
        json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"ci_session","arguments":{"operation":"openDashboard","args":{}}}}),
    );
    assert_eq!(reply.0, 200);
    let value: Value =
        serde_json::from_str(reply.2["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(value["status"], "ok");
    let url = reqwest::Url::parse(value["data"]["url"].as_str().unwrap()).unwrap();
    let address: SocketAddr = format!("127.0.0.1:{}", url.port().unwrap())
        .parse()
        .unwrap();
    let token = url.fragment().unwrap();
    assert_eq!(
        state(address, token)["schema"],
        "native_session_dashboard_v1"
    );
    assert!(server.revoke("dashboard-owner").unwrap());
    closed(address);
    assert_eq!(
        request(
            Some(&sid),
            json!({"jsonrpc":"2.0","id":3,"method":"tools/list"})
        )
        .0,
        401
    );
    server.shutdown().unwrap();
}

#[test]
fn oversized_and_incomplete_headers_have_bounded_refusal_and_leave_listener_usable() {
    let r = root();
    let mut s = open(
        r.path(),
        host(Config {
            request_timeout_ms: 100,
            ..Default::default()
        }),
        "framing-bounds",
    );
    let (address, _) = start(&mut s);
    let request = format!(
        "GET / HTTP/1.1\r\nHost: {address}\r\nX-Oversize: {}\r\n\r\n",
        "a".repeat(16400)
    );
    assert_eq!(raw(address, request.as_bytes()).0, 431);
    let mut stream = TcpStream::connect(address).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    let start = Instant::now();
    stream.write_all(b"GET /").unwrap();
    let mut bytes = Vec::new();
    stream.read_to_end(&mut bytes).unwrap();
    assert!(start.elapsed() < Duration::from_secs(2));
    assert!(bytes.starts_with(b"HTTP/1.1 408 "));
    assert_eq!(get(address, None, "/").0, 200);
    drop(s);
    closed(address);
}
