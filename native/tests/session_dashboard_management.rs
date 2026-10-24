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
