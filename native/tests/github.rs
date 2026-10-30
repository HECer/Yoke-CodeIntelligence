use serde_json::json;
use std::{
    collections::BTreeMap,
    io::{Read, Write},
    net::{SocketAddr, TcpListener},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::Duration,
};
use yoke::{Engine, capture::Deadline, github::HostReadProfile};
type Routes = BTreeMap<String, (u16, Vec<(String, String)>, String)>;
struct HttpFixture {
    endpoint: SocketAddr,
    requests: Arc<Mutex<Vec<String>>>,
    stop: Arc<AtomicBool>,
    worker: Option<thread::JoinHandle<()>>,
}
impl HttpFixture {
    fn new(routes: Routes) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let endpoint = listener.local_addr().unwrap();
        listener.set_nonblocking(true).unwrap();
        let requests = Arc::new(Mutex::new(vec![]));
        let seen = requests.clone();
        let stop = Arc::new(AtomicBool::new(false));
        let done = stop.clone();
        let worker = thread::spawn(move || {
            while !done.load(Ordering::Relaxed) {
                match listener.accept() {
                    Ok((mut stream, _)) => {
                        // Winsock accepts can inherit the listener's nonblocking mode.
                        stream.set_nonblocking(false).unwrap();
                        stream
                            .set_read_timeout(Some(Duration::from_secs(2)))
                            .unwrap();
                        stream
                            .set_write_timeout(Some(Duration::from_secs(2)))
                            .unwrap();
                        let mut raw = vec![];
                        let mut byte = [0];
                        while raw.len() < 16384 {
                            match stream.read(&mut byte) {
                                Ok(0) => break,
                                Ok(_) => {}
                                Err(error) => {
                                    eprintln!(
                                        "Fixture read stopped before headers: {:?}, os={:?}, received={}",
                                        error.kind(),
                                        error.raw_os_error(),
                                        raw.len()
                                    );
                                    break;
                                }
                            }
                            raw.push(byte[0]);
                            if raw.ends_with(b"\r\n\r\n") {
                                break;
                            }
                        }
                        if !raw.ends_with(b"\r\n\r\n") {
                            continue;
                        }
                        let head = String::from_utf8(raw).unwrap();
                        let request = head.lines().next().unwrap_or("").to_owned();
                        seen.lock().unwrap().push(request.clone());
                        let path = request
                            .split_whitespace()
                            .nth(1)
                            .unwrap_or("")
                            .split('?')
                            .next()
                            .unwrap_or("");
                        let default = (404, vec![], "{}".into());
                        if !routes.contains_key(path) {
                            eprintln!(
                                "Fixture unknown request line: {request:?}; header bytes={}",
                                head.len()
                            );
                        }
                        let (status, headers, body) = routes.get(path).unwrap_or(&default);
                        let mut response = format!(
                            "HTTP/1.1 {status} Fixture\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n",
                            body.len()
                        );
                        for (key, value) in headers {
                            response.push_str(&format!("{key}: {value}\r\n"));
                        }
                        response.push_str("\r\n");
                        response.push_str(body);
                        let _ = stream.write_all(response.as_bytes());
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(2))
                    }
                    Err(_) => break,
                }
            }
        });
        Self {
            endpoint,
            requests,
            stop,
            worker: Some(worker),
        }
    }
}
impl Drop for HttpFixture {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(worker) = self.worker.take() {
            worker.join().unwrap();
        }
    }
}
fn response(value: serde_json::Value) -> (u16, Vec<(String, String)>, String) {
    (200, vec![], value.to_string())
}
#[test]
fn fixture_waits_for_delayed_complete_request_after_accept() {
    let fixture = HttpFixture::new(routes());
    let mut socket = std::net::TcpStream::connect(fixture.endpoint).unwrap();
    socket
        .set_read_timeout(Some(Duration::from_secs(3)))
        .unwrap();
    thread::sleep(Duration::from_millis(80));
    socket
        .write_all(b"GET /repos/org/repo HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
        .unwrap();
    let mut response = String::new();
    socket.read_to_string(&mut response).unwrap();
    assert!(
        response.starts_with("HTTP/1.1 200"),
        "response={response:?}"
    );
}

#[test]
fn deletion_point_patch_has_a_valid_empty_source_line_range() {
    let mut data = routes();
    data.insert("/repos/org/repo/pulls/1/files".into(),response(json!([{"sha":blob_sha(""),"filename":"main.rs","status":"modified","additions":0,"deletions":1,"changes":1,"patch":"@@ -1 +0,0 @@\n-fn old(){}"}])));
    let fixture = HttpFixture::new(data);
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("main.rs"), "").unwrap();
    let mut engine = Engine::open(root.path()).unwrap();
    let d = Deadline::new(20000);
    let mut client = yoke::github::ReadClient::for_fixture(
        &profile(),
        &yoke::github::HostTestProfile::loopback(fixture.endpoint).unwrap(),
        &d,
    )
    .unwrap();
    let result = client
        .query(&mut engine, "get_pr_impact", &json!({"pr_number":1}), &d)
        .unwrap();
    assert_eq!(result["impact"]["files"][0]["hunksVerified"], true);
    let anchor = &result["impact"]["files"][0]["hunks"][0]["anchor"];
    assert_eq!(anchor["startByte"], 0);
    assert_eq!(anchor["endByte"], 0);
    assert_eq!(anchor["startLine"], 1);
    assert_eq!(anchor["endLine"], 1);
}
fn profile() -> HostReadProfile {
    serde_json::from_value(
        json!({"allowNetwork":true,"allowedRepos":["org/repo"],"defaultRepo":"org/repo"}),
    )
    .unwrap()
}
fn routes() -> Routes {
    let head = "b".repeat(40);
    let pull = json!({"id":11,"number":1,"title":"Add feature","state":"open","draft":false,"updated_at":"2026-10-14T23:00:00Z","base":{"ref":"main","sha":"a".repeat(40),"repo":{"full_name":"org/repo"}},"head":{"ref":"feature","sha":head,"repo":{"full_name":"org/repo"}},"user":{"login":"contributor"}});
    BTreeMap::from([
        (
            "/repos/org/repo".into(),
            response(json!({"id":7,"full_name":"org/repo","default_branch":"main"})),
        ),
        (
            "/repos/org/repo/pulls".into(),
            response(json!([pull.clone()])),
        ),
        ("/repos/org/repo/pulls/1".into(), response(pull)),
        (
            format!("/repos/org/repo/commits/{head}/status"),
            response(
                json!({"sha":head,"state":"success","total_count":1,"statuses":[{"id":4,"context":"build","state":"success"}]}),
            ),
        ),
        (
            format!("/repos/org/repo/commits/{head}/check-runs"),
            response(
                json!({"total_count":1,"check_runs":[{"id":2,"name":"ci","head_sha":head,"status":"completed","conclusion":"success"}]}),
            ),
        ),
        (
            "/repos/org/repo/pulls/1/reviews".into(),
            response(
                json!([{"id":1,"user":{"login":"reviewer"},"state":"APPROVED","commit_id":head,"submitted_at":"2026-10-14T23:10:00Z"}]),
            ),
        ),
        ("/repos/org/repo/pulls/1/files".into(), response(json!([]))),
    ])
}
#[test]
fn explicit_loopback_http_reads_observed_ci_review_and_scope_without_commands() {
    let fixture = HttpFixture::new(routes());
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("main.rs"), "fn local(){}\n").unwrap();
    let mut engine = Engine::open(root.path()).unwrap();
    let d = Deadline::new(10000);
    let test = yoke::github::HostTestProfile::loopback(fixture.endpoint).unwrap();
    let mut client = yoke::github::ReadClient::for_fixture(&profile(), &test, &d).unwrap();
    let result = client
        .query(&mut engine, "list_prs", &json!({}), &d)
        .unwrap();
    assert_eq!(result["items"].as_array().unwrap().len(), 1);
    assert_eq!(result["items"][0]["ci"]["state"], "all_observed_success");
    assert_eq!(result["items"][0]["reviews"]["state"], "observed_approvals");
    assert!(result["budget"]["requests"].as_u64().unwrap() <= 40);
    assert!(
        fixture
            .requests
            .lock()
            .unwrap()
            .iter()
            .all(|request| request.starts_with("GET /repos/org/repo"))
    );
    assert!(
        client
            .query(&mut engine, "list_prs", &json!({"repo":"foreign/repo"}), &d)
            .is_err()
    );
}
#[test]
fn actual_http_redirect_foreign_pagination_and_response_caps_fail_closed() {
    for attack in ["redirect", "link", "size"] {
        let mut data = routes();
        if attack == "redirect" {
            data.insert(
                "/repos/org/repo".into(),
                (
                    301,
                    vec![("Location".into(), "https://outside.invalid/".into())],
                    "{}".into(),
                ),
            );
        }
        if attack == "link" {
            data.get_mut("/repos/org/repo/pulls").unwrap().1.push(("Link".into(),"<https://api.github.com/repos/foreign/repo/pulls?per_page=100&page=2>; rel=\"next\"".into()));
        }
        if attack == "size" {
            data.get_mut("/repos/org/repo").unwrap().2 =
                format!("{{\"padding\":\"{}\"}}", "x".repeat(2000));
        }
        let fixture = HttpFixture::new(data);
        let root = tempfile::tempdir().unwrap();
        let mut engine = Engine::open(root.path()).unwrap();
        let d = Deadline::new(10000);
        let mut scope = profile();
        if attack == "size" {
            scope.max_response_bytes = 256;
        }
        let mut client = yoke::github::ReadClient::for_fixture(
            &scope,
            &yoke::github::HostTestProfile::loopback(fixture.endpoint).unwrap(),
            &d,
        )
        .unwrap();
        assert!(
            client
                .query(&mut engine, "list_prs", &json!({}), &d)
                .is_err(),
            "{attack}"
        );
        let requests = fixture.requests.lock().unwrap();
        assert!(
            !requests.is_empty(),
            "{attack} never reached real HTTP transport"
        );
        assert!(requests.iter().all(|request| !request.contains("foreign")));
    }
}
#[test]
fn missing_ci_and_global_request_exhaustion_never_report_green_or_invent_empty_coverage() {
    let mut data = routes();
    data.remove(&format!(
        "/repos/org/repo/commits/{}/check-runs",
        "b".repeat(40)
    ));
    let fixture = HttpFixture::new(data);
    let root = tempfile::tempdir().unwrap();
    let mut engine = Engine::open(root.path()).unwrap();
    let d = Deadline::new(10000);
    let mut client = yoke::github::ReadClient::for_fixture(
        &profile(),
        &yoke::github::HostTestProfile::loopback(fixture.endpoint).unwrap(),
        &d,
    )
    .unwrap();
    let result = client
        .query(&mut engine, "list_prs", &json!({}), &d)
        .unwrap_or_else(|error| {
            panic!(
                "Fixture error: {error}; requests={:?}",
                fixture.requests.lock().unwrap()
            )
        });
    assert_eq!(result["items"][0]["ci"]["state"], "unknown");
    assert_eq!(result["items"][0]["ci"]["coverage"]["complete"], false);
    let mut scope = profile();
    scope.max_requests = 2;
    let mut client = yoke::github::ReadClient::for_fixture(
        &scope,
        &yoke::github::HostTestProfile::loopback(fixture.endpoint).unwrap(),
        &d,
    )
    .unwrap();
    let result = client
        .query(&mut engine, "list_prs", &json!({}), &d)
        .unwrap();
    assert_eq!(result["budget"]["requests"], 2);
    assert_eq!(result["items"][0]["ci"]["state"], "unknown");
    assert_eq!(result["items"][0]["reviews"]["coverage"]["complete"], false);
}
fn blob_sha(text: &str) -> String {
    use sha1::{Digest, Sha1};
    let mut h = Sha1::new();
    h.update(format!("blob {}\0", text.len()));
    h.update(text.as_bytes());
    hex::encode(h.finalize())
}
#[test]
fn triage_excludes_stale_wrong_base_and_future_metadata_with_explicit_reasons() {
    let mut data = routes();
    let mut pull: serde_json::Value =
        serde_json::from_str(&data["/repos/org/repo/pulls/1"].2).unwrap();
    pull["updated_at"] = json!(time::OffsetDateTime::now_utc().date().to_string() + "T00:00:00Z");
    let fresh = pull.clone();
    let mut stale = pull.clone();
    stale["number"] = json!(2);
    stale["id"] = json!(12);
    stale["updated_at"] = json!("1900-01-01T00:00:00Z");
    let mut wrong = pull.clone();
    wrong["number"] = json!(3);
    wrong["id"] = json!(13);
    wrong["base"]["ref"] = json!("other");
    let mut future = pull;
    future["number"] = json!(4);
    future["id"] = json!(14);
    future["updated_at"] = json!("3000-01-01T00:00:00Z");
    data.insert(
        "/repos/org/repo/pulls".into(),
        response(json!([fresh.clone(), stale, wrong, future])),
    );
    data.insert("/repos/org/repo/pulls/1".into(), response(fresh));
    let fixture = HttpFixture::new(data);
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("main.rs"), "fn local(){}\n").unwrap();
    let mut engine = Engine::open(root.path()).unwrap();
    let d = Deadline::new(20000);
    let mut client = yoke::github::ReadClient::for_fixture(
        &profile(),
        &yoke::github::HostTestProfile::loopback(fixture.endpoint).unwrap(),
        &d,
    )
    .unwrap();
    let result = client
        .query(&mut engine, "triage_prs", &json!({}), &d)
        .unwrap();
    assert_eq!(result["items"].as_array().unwrap().len(), 1);
    let reasons: Vec<_> = result["excluded"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| row["reason"].as_str().unwrap())
        .collect();
    assert_eq!(
        reasons,
        [
            "stale_by_host_update_age_policy",
            "wrong_base",
            "timestamp_invalid_or_future"
        ]
    );
    assert!(result["items"][0]["impact"]["snapshotId"].is_string());
}
#[test]
fn invalid_operation_arguments_are_rejected_before_any_hosted_request() {
    let fixture = HttpFixture::new(routes());
    let root = tempfile::tempdir().unwrap();
    let mut engine = Engine::open(root.path()).unwrap();
    let d = Deadline::new(20000);
    let mut client = yoke::github::ReadClient::for_fixture(
        &profile(),
        &yoke::github::HostTestProfile::loopback(fixture.endpoint).unwrap(),
        &d,
    )
    .unwrap();
    for (name, args) in [
        ("get_pr_impact", json!({"pr_number":0})),
        ("get_pr_impact", json!({"pr_number":1,"base":"main"})),
        ("list_prs", json!({"base":"bad ref"})),
        ("list_prs", json!({"repo":123})),
        ("triage_prs", json!({"allowNetwork":true})),
    ] {
        assert!(client.query(&mut engine, name, &args, &d).is_err());
        assert!(
            fixture.requests.lock().unwrap().is_empty(),
            "invalid {name} arguments reached HTTP"
        );
    }
}
#[test]
fn unchanged_patch_context_is_not_reported_as_changed_symbols() {
    let source = "fn before(){}\nfn changed(){}\nfn after(){}\n";
    let mut data = routes();
    data.insert("/repos/org/repo/pulls/1/files".into(), response(json!([{"sha":blob_sha(source),"filename":"main.rs","status":"modified","additions":1,"deletions":1,"changes":2,"patch":"@@ -1,3 +1,3 @@\n fn before(){}\n-fn old(){}\n+fn changed(){}\n fn after(){}"}])));
    let fixture = HttpFixture::new(data);
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("main.rs"), source).unwrap();
    let mut engine = Engine::open(root.path()).unwrap();
    let d = Deadline::new(20000);
    let mut client = yoke::github::ReadClient::for_fixture(
        &profile(),
        &yoke::github::HostTestProfile::loopback(fixture.endpoint).unwrap(),
        &d,
    )
    .unwrap();
    let result = client
        .query(&mut engine, "get_pr_impact", &json!({"pr_number":1}), &d)
        .unwrap();
    let names: Vec<_> = result["impact"]["changedSymbols"]
        .as_array()
        .unwrap()
        .iter()
        .map(|node| node["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, ["changed"]);
}
#[test]
fn pr_hunks_bind_exact_local_unicode_blob_and_incoming_structural_candidates() {
    let source = "fn Δchanged(){ other(); }\nfn other(){}\nfn caller(){ Δchanged(); }\n";
    let mut data = routes();
    data.insert("/repos/org/repo/pulls/1/files".into(),response(json!([{"sha":blob_sha(source),"filename":"main.rs","status":"modified","additions":1,"deletions":1,"changes":2,"patch":"@@ -1 +1 @@\n-fn original(){}\n+fn Δchanged(){ other(); }"}])));
    let fixture = HttpFixture::new(data);
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("main.rs"), source).unwrap();
    let mut engine = Engine::open(root.path()).unwrap();
    let d = Deadline::new(20000);
    let mut client = yoke::github::ReadClient::for_fixture(
        &profile(),
        &yoke::github::HostTestProfile::loopback(fixture.endpoint).unwrap(),
        &d,
    )
    .unwrap();
    let result = client
        .query(&mut engine, "get_pr_impact", &json!({"pr_number":1}), &d)
        .unwrap();
    let impact = &result["impact"];
    assert_eq!(
        impact["files"][0]["sourceMatch"],
        "local_blob_matches_reported_head"
    );
    assert_eq!(
        impact["files"][0]["sourceBinding"]["contentHash"],
        yoke::model::hash(source)
    );
    assert_eq!(impact["files"][0]["hunks"][0]["anchor"]["startLine"], 1);
    assert!(
        impact["changedSymbols"]
            .as_array()
            .unwrap()
            .iter()
            .any(|s| s["name"] == "Δchanged")
    );
    assert!(
        impact["incomingCandidates"]
            .as_array()
            .unwrap()
            .iter()
            .any(|s| s["name"] == "caller")
    );
    assert_eq!(
        impact["certainty"],
        "source_bound_structural_candidate_not_compiler_or_merge_proof"
    );
}
#[test]
fn nonmatching_working_source_never_claims_exact_pr_hunk_or_empty_impact() {
    let mut data = routes();
    data.insert("/repos/org/repo/pulls/1/files".into(),response(json!([{"sha":"d".repeat(40),"filename":"main.rs","status":"modified","additions":1,"deletions":1,"changes":2,"patch":"@@ -1 +1 @@\n-fn old(){}\n+fn remote(){}"}])));
    let fixture = HttpFixture::new(data);
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("main.rs"), "fn actual_local(){}\n").unwrap();
    let mut engine = Engine::open(root.path()).unwrap();
    let d = Deadline::new(20000);
    let mut client = yoke::github::ReadClient::for_fixture(
        &profile(),
        &yoke::github::HostTestProfile::loopback(fixture.endpoint).unwrap(),
        &d,
    )
    .unwrap();
    let result = client
        .query(&mut engine, "get_pr_impact", &json!({"pr_number":1}), &d)
        .unwrap();
    assert_eq!(
        result["impact"]["files"][0]["sourceMatch"],
        "path_only_candidate"
    );
    assert_eq!(result["impact"]["files"][0]["hunksVerified"], false);
    assert!(
        !result["impact"]["pathCandidates"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert!(
        result["impact"]["changedSymbols"]
            .as_array()
            .unwrap()
            .is_empty()
    );
}
#[test]
fn typed_host_scope_defaults_deny_network_and_reject_url_or_token_fields() {
    let profile = HostReadProfile::default();
    profile.validate().unwrap();
    assert!(!profile.allow_network);
    for value in [
        json!({"apiUrl":"http://127.0.0.1:1"}),
        json!({"token":"private-test-value"}),
        json!({"testOrigin":"http://127.0.0.1:1"}),
    ] {
        assert!(serde_json::from_value::<HostReadProfile>(value).is_err());
    }
    let root = tempfile::tempdir().unwrap();
    let mut engine = Engine::open(root.path()).unwrap();
    assert!(
        yoke::github::query(
            &profile,
            &mut engine,
            "list_prs",
            &json!({}),
            &Deadline::new(1000)
        )
        .unwrap_err()
        .to_string()
        .contains("unavailable")
    );
}
#[test]
fn repositories_credentials_and_global_request_response_budgets_are_validated() {
    let profile:HostReadProfile=serde_json::from_value(json!({"allowNetwork":true,"allowedRepos":["Org/repo"],"defaultRepo":"org/repo","maxRequests":40,"aggregateResponseBytes":33554432})).unwrap();
    profile.validate().unwrap();
    for update in [
        json!({"allowedRepos":["https://example.com/repo"]}),
        json!({"allowedRepos":["org/../repo"]}),
        json!({"allowedRepos":["Org/repo","org/REPO"]}),
        json!({"defaultRepo":"foreign/repo"}),
        json!({"credentialRef":"codex-agent-mail/devtesting@totalcreations.de"}),
        json!({"maxRequests":201}),
        json!({"maxPages":21}),
        json!({"maxResponseBytes":8388609}),
        json!({"aggregateResponseBytes":67108865}),
    ] {
        let mut value = serde_json::to_value(&profile).unwrap();
        for (k, v) in update.as_object().unwrap() {
            value[k] = v.clone();
        }
        assert!(
            serde_json::from_value::<HostReadProfile>(value)
                .unwrap()
                .validate()
                .is_err()
        );
    }
}
