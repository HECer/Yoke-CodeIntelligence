//! Actual owned receiver/client sockets, SQLite, source and host credentials.
use serde_json::{Value, json};
use std::{
    io::{Read, Write},
    net::{SocketAddr, TcpStream},
    time::Duration,
};
use yoke::{
    Engine,
    capture::Deadline,
    github::discussions as provider,
    github_app::{
        Host, HostProfile, SigningKey,
        digest::{
            self, discussions,
            service::{self, receiver},
        },
    },
    model::{hash, identity},
    store::Store,
};
const TOKEN: [u8; 32] = [42; 32];
use receiver::query;
fn query_request(s: &Source, proposal_ids: Vec<String>) -> query::Request {
    query::Request {
        schema_version: 1,
        app_id: 123,
        sender_workspace_id: s.engine.store.workspace_id.clone(),
        installation_id: 42,
        repository_id: 9,
        repository: "org/repo".into(),
        proposal_ids,
        kinds: vec![
            query::Kind::Symbol,
            query::Kind::Commit,
            query::Kind::Pull,
            query::Kind::Observation,
            query::Kind::Relation,
        ],
        text: String::new(),
        offset: 0,
        limit: 100,
    }
}
fn query_start(
    s: &Source,
    host_allowed: bool,
    grant_allowed: bool,
) -> (tempfile::TempDir, receiver::ServerHandle) {
    let root = tempfile::tempdir().unwrap();
    let mut p = s.receiver_profile();
    p.grants[0].allow_query = grant_allowed;
    let h = receiver::start(
        Store::open(root.path()).unwrap(),
        p,
        receiver::Config {
            allow_query: host_allowed,
            max_response_bytes: 64_000,
            ..config()
        },
        &Deadline::new(20000),
    )
    .unwrap();
    (root, h)
}
fn query_http(h: &receiver::ServerHandle, q: &Value, token: &[u8; 32]) -> (u16, Value) {
    let c = reqwest::blocking::Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(5))
        .build()
        .unwrap();
    let response = c
        .post(format!("{}v1/digest-query", h.endpoint()))
        .bearer_auth(hex::encode(token))
        .header("Content-Type", "application/json")
        .body(serde_json::to_vec(q).unwrap())
        .send()
        .unwrap();
    let status = response.status().as_u16();
    (
        status,
        serde_json::from_slice(&response.bytes().unwrap()).unwrap(),
    )
}
fn receive_rich_claims(s: &Source, h: &receiver::ServerHandle) -> (String, Value) {
    // Explicit authenticated-sender fixture. Rehashed metadata is NOT a real
    // provider acquisition, Git walk, compiler binding or source replay.
    let mut wire = s.proposal(&s.client_profile(h.endpoint()))["body"].clone();
    let p = &mut wire["digest"]["payload"];
    let symbol = p["symbols"][0].clone();
    p["symbols"] = json!([symbol, symbol]);
    p["symbols"][0]["name"] = json!("café");
    p["symbols"][1]["name"] = json!("café");
    p["commits"] = json!([{"sha":"a".repeat(40),"treeId":"b".repeat(40),"parentIds":["c".repeat(40),"c".repeat(40)],"filesStatus":"reported","symbolsStatus":"reported"}]);
    p["threads"] = json!([{"number":7,"providerId":70,"headSha":"a".repeat(40),"baseSha":"c".repeat(40),"merged":true,"coverage":{"complete":false},"sourceAssociation":{"status":"unverified_sender_fixture"},"observations":[{"kind":"review","id":11,"bodyHash":hash(b"excluded body")},{"kind":"review_comment","id":12,"inReplyToId":99,"reviewId":11,"bodyHash":hash(b"excluded reply")}]}]);
    wire["digest"]["id"] = json!(hash(
        yoke::json::encode_bounded(&wire["digest"]["payload"], 4_000_000, &Deadline::new(20000))
            .unwrap()
    ));
    let raw = serde_json::to_vec(&wire).unwrap();
    let id = identity(
        &json!({"schemaVersion":1,"workspaceId":s.engine.store.workspace_id,"appId":123,"endpoint":h.endpoint(),"payloadHash":hash(&raw)}),
    );
    let (status, receipt) = request(h, "PUT", &id, &TOKEN, &raw);
    assert_eq!(status, 201);
    (id, receipt)
}
#[test]
fn queries_preserve_duplicate_ordinals_parallel_directions_and_unverified_claims() {
    let s = Source::new();
    let (root, h) = query_start(&s, true, true);
    let (id, receipt) = receive_rich_claims(&s, &h);
    let q = query_request(&s, vec![id]);
    let (status, result) = query_http(&h, &serde_json::to_value(&q).unwrap(), &TOKEN);
    assert_eq!(status, 200);
    assert_eq!(result["rows"].as_array().unwrap().len(), 11);
    assert_eq!(result["matchingRows"], 11);
    assert_eq!(result["complete"], false);
    assert_eq!(result["completeForSelectedMetadata"], true);
    assert_eq!(result["manifests"][0]["receiptId"], receipt["receiptId"]);
    let rows = result["rows"].as_array().unwrap();
    assert_ne!(rows[0]["id"], rows[1]["id"]);
    assert_eq!(rows[0]["metadata"], rows[1]["metadata"]);
    let parents = rows
        .iter()
        .filter(|r| r["metadata"]["data"]["relation"] == "reported_commit_parent")
        .collect::<Vec<_>>();
    assert_eq!(parents.len(), 2);
    assert_ne!(parents[0]["id"], parents[1]["id"]);
    assert_eq!(
        parents[0]["metadata"]["data"]["to"]["presentInSelectedMetadata"],
        false
    );
    let reply = rows
        .iter()
        .find(|r| r["metadata"]["data"]["relation"] == "reported_in_reply_to")
        .unwrap();
    assert_eq!(reply["metadata"]["data"]["from"]["providerId"], 12);
    assert_eq!(reply["metadata"]["data"]["to"]["providerId"], 99);
    assert_eq!(
        reply["metadata"]["data"]["to"]["presentInCapturedThread"],
        false
    );
    let review = rows
        .iter()
        .find(|r| r["metadata"]["data"]["relation"] == "reported_review_membership")
        .unwrap();
    assert_eq!(
        review["metadata"]["data"]["to"]["presentInCapturedThread"],
        true
    );
    assert!(
        rows.iter()
            .all(|r| r["sourceAuthority"] == false && r["semanticIdentityEstablished"] == false)
    );
    assert!(!result.to_string().contains("excluded reply"));
    assert_eq!(count(root.path()), 1);
    h.shutdown().unwrap();
}
#[test]
fn query_native_client_filters_unicode_pages_and_replays_local_read_only_without_new_records() {
    let s = Source::new();
    let (root, h) = query_start(&s, true, true);
    let (id, _) = receive_rich_claims(&s, &h);
    let profile = s.client_profile(h.endpoint());
    let mut q = query_request(&s, vec![id]);
    q.kinds = vec![query::Kind::Symbol];
    q.text = "café".into();
    q.limit = 1;
    let options = query::client::Options {
        allow_query: true,
        allow_network: true,
        allow_fixture: true,
        max_output_bytes: 64000,
    };
    let remote = query::client::for_fixture(
        &profile,
        &s.app,
        &q,
        &options,
        h.local_addr(),
        TOKEN,
        &Deadline::new(20000),
    )
    .unwrap();
    let first = &remote["result"];
    assert_eq!(first["matchingRows"], 2);
    assert_eq!(first["hasMore"], true);
    assert_eq!(first["completeForSelectedMetadata"], false);
    q.offset = 1;
    let second = query::client::for_fixture(
        &profile,
        &s.app,
        &q,
        &options,
        h.local_addr(),
        TOKEN,
        &Deadline::new(20000),
    )
    .unwrap();
    assert_eq!(second["result"]["hasMore"], false);
    assert_ne!(first["rows"][0]["id"], second["result"]["rows"][0]["id"]);
    let mut p = s.receiver_profile();
    p.endpoint = h.endpoint().into();
    p.grants[0].allow_query = true;
    h.shutdown().unwrap();
    let store = Store::open_existing_read_only(root.path(), &Deadline::new(20000)).unwrap();
    let local = query::local(
        &store,
        &p,
        "sender",
        &q,
        &query::Options {
            allow_query: true,
            max_output_bytes: 64000,
            ..Default::default()
        },
        &Deadline::new(20000),
    )
    .unwrap();
    assert_eq!(local, second["result"]);
    assert_eq!(count(root.path()), 1);
    q.text = "CAFÉ".into();
    assert_eq!(
        query::local(
            &store,
            &p,
            "sender",
            &q,
            &query::Options {
                allow_query: true,
                ..Default::default()
            },
            &Deadline::new(20000)
        )
        .unwrap()["matchingRows"],
        0
    );
}
#[test]
fn query_host_and_grant_permissions_default_deny_authentication_and_revocation_are_current() {
    let s = Source::new();
    let mut old = serde_json::to_value(s.receiver_profile()).unwrap();
    old["grants"][0]
        .as_object_mut()
        .unwrap()
        .remove("allowQuery");
    assert!(
        !receiver::Profile::parse(&serde_json::to_vec(&old).unwrap())
            .unwrap()
            .grants[0]
            .allow_query
    );
    for (host, grant) in [(false, true), (true, false), (false, false), (true, true)] {
        let (root, h) = query_start(&s, host, grant);
        let p = s.proposal(&s.client_profile(h.endpoint()));
        let id = p["proposalId"].as_str().unwrap();
        assert_eq!(request(&h, "PUT", id, &TOKEN, &body(&p)).0, 201);
        let q = serde_json::to_value(query_request(&s, vec![id.into()])).unwrap();
        assert_eq!(query_http(&h, &q, &[41; 32]).0, 401);
        assert_eq!(
            query_http(&h, &q, &TOKEN).0,
            if host && grant { 200 } else { 403 }
        );
        h.revoke("sender", &Deadline::new(20000)).unwrap();
        assert_eq!(query_http(&h, &q, &TOKEN).0, 401);
        assert_eq!(count(root.path()), 1);
        h.shutdown().unwrap();
    }
}
#[test]
fn query_all_selections_are_scoped_before_any_row_and_unknown_protocol_is_refused() {
    let s = Source::new();
    let (root, h) = query_start(&s, true, true);
    let (id, _) = receive_rich_claims(&s, &h);
    let q = query_request(&s, vec![id]);
    let mut missing = q.clone();
    missing.proposal_ids.push("f".repeat(64));
    let (status, response) = query_http(&h, &serde_json::to_value(missing).unwrap(), &TOKEN);
    assert_eq!(status, 404);
    assert!(response.get("rows").is_none());
    let mut foreign = q.clone();
    foreign.repository_id = 10;
    assert_eq!(
        query_http(&h, &serde_json::to_value(foreign).unwrap(), &TOKEN).0,
        403
    );
    let mut foreign = q.clone();
    foreign.sender_workspace_id = "f".repeat(64);
    assert_eq!(
        query_http(&h, &serde_json::to_value(foreign).unwrap(), &TOKEN).0,
        403
    );
    let mut unknown = serde_json::to_value(&q).unwrap();
    unknown["allowQuery"] = json!(true);
    assert_eq!(query_http(&h, &unknown, &TOKEN).0, 400);
    let mut duplicate = q.clone();
    duplicate.proposal_ids.push(q.proposal_ids[0].clone());
    assert_eq!(
        query_http(&h, &serde_json::to_value(duplicate).unwrap(), &TOKEN).0,
        400
    );
    assert_eq!(count(root.path()), 1);
    h.shutdown().unwrap();
}
#[test]
fn query_exact_work_search_and_output_budgets_refuse_whole_results_and_preserve_sql_policy() {
    let s = Source::new();
    let (root, h) = query_start(&s, true, true);
    let (id, _) = receive_rich_claims(&s, &h);
    let mut p = s.receiver_profile();
    p.endpoint = h.endpoint().into();
    p.grants[0].allow_query = true;
    h.shutdown().unwrap();
    let store = Store::open_existing_read_only(root.path(), &Deadline::new(20000)).unwrap();
    let mut q = query_request(&s, vec![id]);
    q.kinds = vec![query::Kind::Symbol];
    let o = query::Options {
        allow_query: true,
        max_visited: 2,
        max_output_bytes: 64000,
        ..Default::default()
    };
    let call = |r: &query::Request, options: &query::Options| {
        query::local(&store, &p, "sender", r, options, &Deadline::new(20000))
    };
    let exact = call(&q, &o).unwrap();
    assert_eq!(exact["visitedRows"], 2);
    let below = query::Options {
        max_visited: 1,
        ..o
    };
    assert!(
        call(&q, &below)
            .unwrap_err()
            .to_string()
            .contains("capacity")
    );
    let mut o = query::Options {
        allow_query: true,
        max_output_bytes: 64000,
        ..Default::default()
    };
    q.text = "café".into();
    let result = call(&q, &o).unwrap();
    let search = result["searchBytes"].as_u64().unwrap() as usize;
    assert!(search > 0);
    o.max_search_bytes = search;
    assert!(call(&q, &o).is_ok());
    o.max_search_bytes = search - 1;
    assert!(call(&q, &o).is_err());
    o.max_search_bytes = 0;
    assert!(call(&q, &o).is_err());
    o.max_search_bytes = 32_000_000;
    let result = call(&q, &o).unwrap();
    let bytes = yoke::json::encode_bounded(&result, 64000, &Deadline::new(20000))
        .unwrap()
        .len()
        + 1;
    o.max_output_bytes = bytes;
    assert_eq!(call(&q, &o).unwrap(), result);
    o.max_output_bytes = bytes - 1;
    assert!(call(&q, &o).is_err());
    use rusqlite::limits::Limit;
    let prior = store.db.limit(Limit::SQLITE_LIMIT_LENGTH).unwrap();
    assert!(call(&q, &o).is_err());
    assert_eq!(store.db.limit(Limit::SQLITE_LIMIT_LENGTH).unwrap(), prior);
    store.db.execute_batch("BEGIN").unwrap();
    assert!(
        call(&q, &o)
            .unwrap_err()
            .to_string()
            .contains("caller_transaction")
    );
    store.db.execute_batch("ROLLBACK").unwrap();
    assert_eq!(count(root.path()), 1);
}
#[test]
fn query_cli_default_denial_and_invalid_host_paths_never_initialize_state() {
    use std::process::Command;
    let root = tempfile::tempdir().unwrap();
    let host = tempfile::tempdir().unwrap();
    for args in [
        vec![
            "local",
            "--profile",
            "missing.json",
            "--principal",
            "sender",
            "--request",
            "missing.json",
        ],
        vec![
            "remote",
            "--profile",
            "missing.json",
            "--app-profile",
            "missing.json",
            "--request",
            "missing.json",
        ],
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_yoke"))
            .current_dir(root.path())
            .arg("digest-inbox")
            .args(args)
            .output()
            .unwrap();
        assert!(!output.status.success());
        assert!(output.stdout.is_empty());
        assert!(String::from_utf8_lossy(&output.stderr).contains("permission_required"));
        assert!(!root.path().join(".yoke").exists());
    }
    let output = Command::new(env!("CARGO_BIN_EXE_yoke"))
        .current_dir(root.path())
        .args([
            "digest-inbox",
            "local",
            "--allow-query",
            "--profile",
            "../missing.json",
            "--principal",
            "sender",
            "--request",
        ])
        .arg(host.path().join("query.json"))
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    assert!(!root.path().join(".yoke").exists());
}
#[test]
fn query_local_cli_matches_library_and_never_adds_source_or_processing_records() {
    let s = Source::new();
    let (root, h) = query_start(&s, true, true);
    let (id, _) = receive_rich_claims(&s, &h);
    let mut p = s.receiver_profile();
    p.endpoint = h.endpoint().into();
    p.grants[0].allow_query = true;
    h.shutdown().unwrap();
    let q = query_request(&s, vec![id]);
    let host = tempfile::tempdir().unwrap();
    std::fs::write(
        host.path().join("profile.json"),
        serde_json::to_vec(&p).unwrap(),
    )
    .unwrap();
    std::fs::write(
        host.path().join("query.json"),
        serde_json::to_vec(&q).unwrap(),
    )
    .unwrap();
    let db = rusqlite::Connection::open(root.path().join(".yoke/index.sqlite")).unwrap();
    let facts = || -> (usize, usize, usize) {
        (
            db.query_row("SELECT count(*) FROM records", [], |r| r.get(0))
                .unwrap(),
            db.query_row("SELECT count(*) FROM snapshots", [], |r| r.get(0))
                .unwrap(),
            db.query_row("SELECT count(*) FROM graphs", [], |r| r.get(0))
                .unwrap(),
        )
    };
    let before = facts();
    let mut command = std::process::Command::new(env!("CARGO_BIN_EXE_yoke"));
    command
        .arg("--root")
        .arg(root.path())
        .args([
            "--max-bytes",
            "64000",
            "digest-inbox",
            "local",
            "--allow-query",
            "--principal",
            "sender",
            "--profile",
        ])
        .arg(host.path().join("profile.json"))
        .arg("--request")
        .arg(host.path().join("query.json"));
    let output = command.output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let result: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(result["matchingRows"], 11);
    assert_eq!(result["sourceWritten"], false);
    assert_eq!(result["recordsWritten"], false);
    assert_eq!(facts(), before);
    let store = Store::open_existing_read_only(root.path(), &Deadline::new(20000)).unwrap();
    assert_eq!(
        query::local(
            &store,
            &p,
            "sender",
            &q,
            &query::Options {
                allow_query: true,
                max_output_bytes: 64000,
                ..Default::default()
            },
            &Deadline::new(20000)
        )
        .unwrap(),
        result
    );
}
#[test]
fn query_revocation_during_body_and_disabled_query_authentication_refuse_without_waiting_for_complete_body()
 {
    let s = Source::new();
    let (root, h) = query_start(&s, true, true);
    let (id, _) = receive_rich_claims(&s, &h);
    let q = serde_json::to_vec(&query_request(&s, vec![id])).unwrap();
    let authority = h
        .endpoint()
        .strip_prefix("http://")
        .unwrap()
        .trim_end_matches('/');
    let mut socket = TcpStream::connect(h.local_addr()).unwrap();
    socket
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    write!(socket,"POST /v1/digest-query HTTP/1.1\r\nHost: {authority}\r\nAuthorization: Bearer {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n",hex::encode(TOKEN),q.len()).unwrap();
    socket.write_all(&q[..1]).unwrap();
    std::thread::sleep(Duration::from_millis(30));
    h.revoke("sender", &Deadline::new(20000)).unwrap();
    socket.write_all(&q[1..]).unwrap();
    let mut response = vec![];
    socket.read_to_end(&mut response).unwrap();
    assert!(response.starts_with(b"HTTP/1.1 403"));
    assert!(!String::from_utf8_lossy(&response).contains("\"rows\""));
    assert_eq!(count(root.path()), 1);
    h.shutdown().unwrap();
    let (root, h) = query_start(&s, false, true);
    let mut socket = TcpStream::connect(h.local_addr()).unwrap();
    socket
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    let authority = h
        .endpoint()
        .strip_prefix("http://")
        .unwrap()
        .trim_end_matches('/');
    write!(socket,"POST /v1/digest-query HTTP/1.1\r\nHost: {authority}\r\nAuthorization: Bearer {}\r\nContent-Type: application/json\r\nContent-Length: 8000\r\n\r\n",hex::encode(TOKEN)).unwrap();
    let mut response = vec![];
    socket.read_to_end(&mut response).unwrap();
    assert!(response.starts_with(b"HTTP/1.1 403"));
    assert_eq!(count(root.path()), 0);
    h.shutdown().unwrap();
}
#[test]
fn query_corrupt_storage_expiry_and_unknown_remote_permissions_never_become_empty_complete_results()
{
    let s = Source::new();
    let (root, h) = query_start(&s, true, true);
    let (id, _) = receive_rich_claims(&s, &h);
    let mut p = s.receiver_profile();
    p.endpoint = h.endpoint().into();
    p.grants[0].allow_query = true;
    let q = query_request(&s, vec![id.clone()]);
    let service = s.client_profile(h.endpoint());
    let disabled = query::client::Options::default();
    assert!(
        query::client::for_fixture(
            &service,
            &s.app,
            &q,
            &disabled,
            h.local_addr(),
            TOKEN,
            &Deadline::new(20000)
        )
        .unwrap_err()
        .to_string()
        .contains("permission_required")
    );
    h.shutdown().unwrap();
    let store = Store::open_existing_read_only(root.path(), &Deadline::new(20000)).unwrap();
    let o = query::Options {
        allow_query: true,
        max_output_bytes: 64000,
        ..Default::default()
    };
    let expired = Deadline::new(10);
    std::thread::sleep(Duration::from_millis(15));
    assert!(query::local(&store, &p, "sender", &q, &o, &expired).is_err());
    let db = rusqlite::Connection::open(root.path().join(".yoke/index.sqlite")).unwrap();
    db.execute(
        "UPDATE records SET value='{}' WHERE kind=?1 AND id=?2",
        rusqlite::params![receiver::RECORD_KIND, id],
    )
    .unwrap();
    assert!(query::local(&store, &p, "sender", &q, &o, &Deadline::new(20000)).is_err());
    assert_eq!(count(root.path()), 1);
}
fn capture() -> provider::Capture {
    // Explicit unsigned fixture import; this is not live GitHub acquisition.
    let payload=provider::Payload {repository:"org/repo".into(),repository_id:9,requested_pulls:vec![],unacquired_pulls:vec![],pulls:vec![],
        discovery_coverage:provider::Coverage{complete:true,reason:"provider_reported_pages_complete".into(),pages:1},
        receipts:vec![provider::Receipt{url:"http://127.0.0.1:1/repos/org/repo".into(),response_hash:hash(b"owned metadata"),response_bytes:14,api_version:"2026-03-10".into(),body_retained:false},
            provider::Receipt{url:"http://127.0.0.1:1/repos/org/repo/pulls?per_page=100&page=1&state=closed&sort=updated&direction=desc".into(),response_hash:hash(b"[]"),response_bytes:2,api_version:"2026-03-10".into(),body_retained:false}],
        acquired_at_unix:1_700_000_000,atomic:false,transport:"explicit_host_loopback_fixture".into(),requests:2,response_bytes:16,text_bytes:0,observations:0,coverage_complete:true};
    let d = Deadline::new(20000);
    let c = provider::Capture {
        schema_version: 1,
        kind: "native_github_closed_pr_discussions".into(),
        id: hash(yoke::json::encode_bounded(&payload, 8_000_000, &d).unwrap()),
        payload,
    };
    c.validate(&d).unwrap();
    c
}
fn app_value() -> Value {
    json!({"schemaVersion":1,"appId":123,"secretCredential":"receiver-tests-no-os-read","grants":[{"installationId":42,"repositoryId":9,"repository":"org/repo"}]})
}
struct Source {
    root: tempfile::TempDir,
    engine: Engine,
    app: HostProfile,
    host: Host,
    request: digest::PreviewRequest,
    capture: provider::Capture,
    options: digest::Options,
    permission: discussions::Permission,
    service_options: service::Options,
}
impl Source {
    fn new() -> Self {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(
            root.path().join("main.rs"),
            "pub fn café(value:usize)->usize { value+123456789 }\r\n",
        )
        .unwrap();
        let mut engine = Engine::open(root.path()).unwrap();
        let snapshot = engine.index(&Deadline::new(20000)).unwrap().0;
        let app = HostProfile::parse(&serde_json::to_vec(&app_value()).unwrap()).unwrap();
        let host = Host::with_signing_key(
            app.clone(),
            SigningKey::from_host_bytes(b"owned-receiver-signing-key").unwrap(),
        )
        .unwrap();
        Self {
            root,
            engine,
            app,
            host,
            request: digest::PreviewRequest {
                schema_version: 1,
                snapshot_id: snapshot.id,
                installation_id: 42,
                repository_id: 9,
                repository: "org/repo".into(),
            },
            capture: capture(),
            options: Default::default(),
            permission: discussions::Permission {
                allow_discussions: true,
                allow_fixture: true,
                handoff_preview: true,
            },
            service_options: service::Options {
                allow_service: true,
                allow_network: true,
                allow_writes: true,
                allow_fixture: true,
                ..Default::default()
            },
        }
    }
    fn source(&self) -> discussions::Context<'_> {
        discussions::Context {
            store: &self.engine.store,
            profile: &self.app,
            options: &self.options,
            permission: &self.permission,
            source: discussions::Source::Current,
        }
    }
    fn client_profile(&self, endpoint: &str) -> service::Profile {
        service::Profile {
            schema_version: 1,
            endpoint: endpoint.into(),
            credential_name: if endpoint.starts_with("https:") {
                Some("receiver-test-token".into())
            } else {
                None
            },
            grants: vec![yoke::github_app::Grant {
                installation_id: 42,
                repository_id: 9,
                repository: "org/repo".into(),
            }],
            max_payload_bytes: 4_000_000,
            max_response_bytes: 64_000,
            max_journal_records: 1000,
            max_journal_bytes: 8_000_000,
        }
    }
    fn context<'a>(&'a self, p: &'a service::Profile) -> service::Context<'a> {
        service::Context {
            store: &self.engine.store,
            host: &self.host,
            profile: p,
            options: &self.service_options,
        }
    }
    fn proposal(&self, p: &service::Profile) -> Value {
        service::propose(
            &self.source(),
            &self.request,
            &self.capture,
            p,
            &Deadline::new(20000),
        )
        .unwrap()
        .value(&Deadline::new(20000))
        .unwrap()
    }
    fn approval(&self, p: &service::Profile) -> service::Approval {
        let v = self.proposal(p);
        service::Approval {
            schema_version: 1,
            proposal_id: v["proposalId"].as_str().unwrap().into(),
            payload_hash: v["payloadHash"].as_str().unwrap().into(),
            request_id: "01234567-89ab-cdef-0123-456789abcdef".into(),
            approval: "publish_exact_digest".into(),
        }
    }
    fn receiver_profile(&self) -> receiver::Profile {
        receiver::Profile {
            schema_version: 1,
            endpoint: "http://127.0.0.1:1/".into(),
            grants: vec![receiver::Access {
                allow_query: false,
                id: "sender".into(),
                token_hash: hash(TOKEN),
                app_id: 123,
                sender_workspace_id: self.engine.store.workspace_id.clone(),
                installation_id: 42,
                repository_id: 9,
                repository: "org/repo".into(),
            }],
            max_payload_bytes: 4_000_000,
            max_records: 1000,
            max_stored_bytes: 32_000_000,
        }
    }
}
fn config() -> receiver::Config {
    receiver::Config {
        allow_receiver: true,
        allow_network: true,
        allow_writes: true,
        allow_fixture: true,
        lifetime_ms: 20000,
        ..Default::default()
    }
}
fn start(s: &Source) -> (tempfile::TempDir, receiver::ServerHandle) {
    let root = tempfile::tempdir().unwrap();
    let h = receiver::start(
        Store::open(root.path()).unwrap(),
        s.receiver_profile(),
        config(),
        &Deadline::new(20000),
    )
    .unwrap();
    (root, h)
}
fn count(root: &std::path::Path) -> usize {
    let db = rusqlite::Connection::open(root.join(".yoke/index.sqlite")).unwrap();
    db.query_row(
        "SELECT count(*) FROM records WHERE kind=?1",
        [receiver::RECORD_KIND],
        |r| r.get(0),
    )
    .unwrap()
}
fn body(proposal: &Value) -> Vec<u8> {
    yoke::json::encode_bounded(&proposal["body"], 4_000_000, &Deadline::new(20000)).unwrap()
}
fn request(
    h: &receiver::ServerHandle,
    method: &str,
    id: &str,
    token: &[u8; 32],
    body: &[u8],
) -> (u16, Value) {
    if rustls::crypto::CryptoProvider::get_default().is_none() {
        let _ = rustls::crypto::ring::default_provider().install_default();
    }
    let c = reqwest::blocking::Client::builder()
        .no_proxy()
        .build()
        .unwrap();
    let url = format!("{}v1/digests/{id}", h.endpoint());
    let mut call = if method == "PUT" {
        c.put(url)
            .header("Content-Type", "application/json")
            .header("Idempotency-Key", id)
            .body(body.to_vec())
    } else {
        c.get(url)
    };
    call = call
        .bearer_auth(hex::encode(token))
        .timeout(Duration::from_secs(5));
    let r = call.send().unwrap();
    let status = r.status().as_u16();
    let bytes = r.bytes().unwrap();
    (status, serde_json::from_slice(&bytes).unwrap())
}
#[test]
fn real_native_client_and_receiver_commit_one_intake_and_reconcile_local_failure() {
    let s = Source::new();
    let (root, h) = start(&s);
    let p = s.client_profile(h.endpoint());
    let a = s.approval(&p);
    let c = s.context(&p);
    let client =
        service::Client::for_fixture_token(&c, h.local_addr(), TOKEN, &Deadline::new(20000))
            .unwrap();
    s.engine.store.db.execute_batch("CREATE TRIGGER refuse_local_ack BEFORE INSERT ON records WHEN NEW.kind='github_app_digest_service_v1' AND instr(NEW.value,'\"type\":\"accepted\"')>0 BEGIN SELECT RAISE(ABORT,'owned local ack failure'); END;").unwrap();
    assert!(
        service::send(
            &c,
            &s.source(),
            &s.request,
            &s.capture,
            &a,
            &client,
            &Deadline::new(20000)
        )
        .is_err()
    );
    assert_eq!(count(root.path()), 1);
    assert_eq!(
        service::status(&c, &a.request_id, &Deadline::new(20000)).unwrap()["status"],
        "unknown"
    );
    s.engine
        .store
        .db
        .execute_batch("DROP TRIGGER refuse_local_ack")
        .unwrap();
    let ack = service::recover(&c, &a.request_id, &client, &Deadline::new(20000)).unwrap();
    assert_eq!(ack["status"], "accepted");
    assert_eq!(ack["recovered"], true);
    assert_eq!(
        service::send(
            &c,
            &s.source(),
            &s.request,
            &s.capture,
            &a,
            &client,
            &Deadline::new(20000)
        )
        .unwrap()["networkRequests"],
        0
    );
    assert_eq!(count(root.path()), 1);
    assert_eq!(h.shutdown().unwrap()["requests"], 2);
}
#[test]
fn receipt_is_immutable_for_exact_repeats_and_survives_receiver_restart() {
    let s = Source::new();
    let (root, h) = start(&s);
    let endpoint = h.endpoint().to_owned();
    let address = h.local_addr();
    let proposal = s.proposal(&s.client_profile(&endpoint));
    let id = proposal["proposalId"].as_str().unwrap();
    let raw = body(&proposal);
    let (status, first) = request(&h, "PUT", id, &TOKEN, &raw);
    assert_eq!(status, 201);
    assert_eq!(first["payloadHash"], hash(&raw));
    let (status, second) = request(&h, "PUT", id, &TOKEN, &raw);
    assert_eq!(status, 200);
    assert_eq!(first, second);
    assert_eq!(count(root.path()), 1);
    h.shutdown().unwrap();
    let mut profile = s.receiver_profile();
    profile.endpoint = endpoint;
    let mut cfg = config();
    cfg.listen = address;
    let h = receiver::start(
        Store::open(root.path()).unwrap(),
        profile,
        cfg,
        &Deadline::new(20000),
    )
    .unwrap();
    assert_eq!(request(&h, "GET", id, &TOKEN, &[]), (200, first));
    h.shutdown().unwrap();
}
#[test]
fn authentication_scope_revocation_and_absent_receipts_do_not_publish_or_disclose() {
    let s = Source::new();
    let (root, h) = start(&s);
    let p = s.proposal(&s.client_profile(h.endpoint()));
    let id = p["proposalId"].as_str().unwrap();
    let raw = body(&p);
    assert_eq!(request(&h, "PUT", id, &[43; 32], &raw).0, 401);
    assert_eq!(count(root.path()), 0);
    let mut foreign = p["body"].clone();
    foreign["workspaceId"] = json!("f".repeat(64));
    let raw_foreign = serde_json::to_vec(&foreign).unwrap();
    assert_eq!(request(&h, "PUT", id, &TOKEN, &raw_foreign).0, 403);
    assert_eq!(count(root.path()), 0);
    assert_eq!(request(&h, "GET", id, &TOKEN, &[]).0, 404);
    assert_eq!(request(&h, "PUT", id, &TOKEN, &raw).0, 201);
    h.revoke("sender", &Deadline::new(20000)).unwrap();
    assert_eq!(request(&h, "GET", id, &TOKEN, &[]).0, 401);
    assert_eq!(request(&h, "PUT", id, &TOKEN, &raw).0, 401);
    assert_eq!(count(root.path()), 1);
    h.shutdown().unwrap();
}
#[test]
fn changed_wire_binding_and_rehashed_text_fields_cannot_enter_the_closed_handoff() {
    let s = Source::new();
    let (root, h) = start(&s);
    let p = s.proposal(&s.client_profile(h.endpoint()));
    let id = p["proposalId"].as_str().unwrap();
    for field in ["body", "diffHunk", "author"] {
        let mut value = p["body"].clone();
        value["digest"]["payload"]["symbols"][0][field] = json!("owned raw-text marker");
        value["digest"]["id"] = json!(hash(
            yoke::json::encode_bounded(
                &value["digest"]["payload"],
                4_000_000,
                &Deadline::new(20000)
            )
            .unwrap()
        ));
        let raw = serde_json::to_vec(&value).unwrap();
        let new_id = identity(
            &json!({"schemaVersion":1,"workspaceId":s.engine.store.workspace_id,"appId":123,"endpoint":h.endpoint(),"payloadHash":hash(&raw)}),
        );
        let (status, response) = request(&h, "PUT", &new_id, &TOKEN, &raw);
        assert_eq!(status, 400);
        assert!(!response.to_string().contains("raw-text"));
    }
    let raw = body(&p);
    assert_eq!(request(&h, "PUT", &"a".repeat(64), &TOKEN, &raw).0, 400);
    assert_eq!(count(root.path()), 0);
    assert_eq!(request(&h, "PUT", id, &TOKEN, &raw).0, 201);
    h.shutdown().unwrap();
}
#[test]
fn receiver_capacity_and_sql_failure_never_return_a_false_acceptance() {
    for fault in ["capacity", "sql"] {
        let s = Source::new();
        let root = tempfile::tempdir().unwrap();
        let store = Store::open(root.path()).unwrap();
        if fault == "sql" {
            store.db.execute_batch("CREATE TRIGGER refuse_receiver BEFORE INSERT ON records WHEN NEW.kind='github_app_digest_receiver_v1' BEGIN SELECT RAISE(ABORT,'private database marker'); END;").unwrap();
        }
        let mut profile = s.receiver_profile();
        if fault == "capacity" {
            profile.max_stored_bytes = 1024;
        }
        let h = receiver::start(store, profile, config(), &Deadline::new(20000)).unwrap();
        let p = s.proposal(&s.client_profile(h.endpoint()));
        let (status, response) = request(
            &h,
            "PUT",
            p["proposalId"].as_str().unwrap(),
            &TOKEN,
            &body(&p),
        );
        assert!(status >= 400);
        assert!(!response.to_string().contains("private database"));
        assert_eq!(count(root.path()), 0);
        h.shutdown().unwrap();
    }
}
#[test]
fn received_snapshot_ids_never_pin_matching_local_capture_history() {
    let mut s = Source::new();
    let profile = s.receiver_profile();
    let mut cfg = config();
    cfg.max_requests = 1;
    let h = receiver::start(
        Store::open(s.root.path()).unwrap(),
        profile,
        cfg,
        &Deadline::new(20000),
    )
    .unwrap();
    let p = s.proposal(&s.client_profile(h.endpoint()));
    let old = s.request.snapshot_id.clone();
    std::fs::write(s.root.path().join("main.rs"), "pub fn next_version(){}\n").unwrap();
    s.engine.index(&Deadline::new(20000)).unwrap();
    assert_eq!(
        request(
            &h,
            "PUT",
            p["proposalId"].as_str().unwrap(),
            &TOKEN,
            &body(&p)
        )
        .0,
        201
    );
    h.wait().unwrap();
    let plan =
        yoke::retention::plan(&s.engine.store, &Default::default(), &Deadline::new(20000)).unwrap();
    assert!(
        plan["snapshotIds"]
            .as_array()
            .unwrap()
            .contains(&json!(old))
    );
}
fn raw(addr: SocketAddr, bytes: &[u8]) -> Vec<u8> {
    let mut stream = TcpStream::connect(addr).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(3)))
        .unwrap();
    stream.write_all(bytes).unwrap();
    let mut response = vec![];
    let _ = stream.read_to_end(&mut response);
    response
}
#[test]
fn ambiguous_framing_foreign_hosts_and_bearerless_puts_refuse_before_body_or_state() {
    let s = Source::new();
    let (root, h) = start(&s);
    let host = h.local_addr();
    let id = "a".repeat(64);
    let token = hex::encode(TOKEN);
    let heads = [
        format!(
            "Host: {host}\r\nAuthorization: Bearer {token}\r\nContent-Length: 1\r\nContent-Length: 1\r\n"
        ),
        format!("Host: {host}\r\nAuthorization: Bearer {token}\r\nTransfer-Encoding: chunked\r\n"),
        format!("Host: foreign.invalid\r\nAuthorization: Bearer {token}\r\nContent-Length: 1\r\n"),
        format!("Host: {host}\r\nContent-Length: 4000000\r\n"),
    ];
    for head in heads {
        let response = raw(
            host,
            format!("PUT /v1/digests/{id} HTTP/1.1\r\n{head}\r\n").as_bytes(),
        );
        assert!(response.starts_with(b"HTTP/1.1 400") || response.starts_with(b"HTTP/1.1 401"));
    }
    assert_eq!(count(root.path()), 0);
    h.shutdown().unwrap();
}
#[test]
fn slow_partial_request_shutdown_and_initial_permissions_remain_bounded() {
    let s = Source::new();
    let root = tempfile::tempdir().unwrap();
    for flag in 0..4 {
        let mut c = config();
        match flag {
            0 => c.allow_receiver = false,
            1 => c.allow_network = false,
            2 => c.allow_writes = false,
            _ => c.allow_fixture = false,
        };
        assert!(
            receiver::start(
                Store::open(root.path()).unwrap(),
                s.receiver_profile(),
                c,
                &Deadline::new(20000)
            )
            .is_err()
        );
    }
    let (root, h) = start(&s);
    let mut stream = TcpStream::connect(h.local_addr()).unwrap();
    stream.write_all(b"PUT /v1/").unwrap();
    std::thread::sleep(Duration::from_millis(30));
    let at = std::time::Instant::now();
    h.shutdown().unwrap();
    assert!(at.elapsed() < Duration::from_secs(1));
    assert_eq!(count(root.path()), 0);
}
#[test]
fn actual_cli_default_denial_creates_no_state_or_listener() {
    let root = tempfile::tempdir().unwrap();
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_yoke"))
        .arg("--root")
        .arg(root.path())
        .args(["digest-receiver", "--profile", "C:/missing-receiver.json"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    assert!(!root.path().join(".yoke").exists());
    assert!(String::from_utf8_lossy(&output.stderr).contains("permission_required"));
}

#[test]
fn detached_host_cancellation_remains_binding_after_start_returns() {
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };
    let s = Source::new();
    let root = tempfile::tempdir().unwrap();
    let cancel = Arc::new(AtomicBool::new(false));
    let d = Deadline::run_bounded(&Deadline::new(20000), Arc::clone(&cancel), || {
        Ok(Deadline::new(20000))
    })
    .unwrap();
    let h = receiver::start(
        Store::open(root.path()).unwrap(),
        s.receiver_profile(),
        config(),
        &d,
    )
    .unwrap();
    let mut stream = TcpStream::connect(h.local_addr()).unwrap();
    stream.write_all(b"PUT /v1/").unwrap();
    std::thread::sleep(Duration::from_millis(30));
    let at = std::time::Instant::now();
    cancel.store(true, Ordering::SeqCst);
    h.wait().unwrap();
    assert!(at.elapsed() < Duration::from_secs(1));
    assert_eq!(count(root.path()), 0);
}

#[test]
fn revocation_during_body_acquisition_precedes_any_record_commit() {
    let s = Source::new();
    let (root, h) = start(&s);
    let p = s.proposal(&s.client_profile(h.endpoint()));
    let id = p["proposalId"].as_str().unwrap();
    let payload = body(&p);
    let address = h.local_addr();
    let mut stream = TcpStream::connect(address).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    write!(stream,"PUT /v1/digests/{id} HTTP/1.1\r\nHost: {address}\r\nAuthorization: Bearer {}\r\nContent-Type: application/json\r\nIdempotency-Key: {id}\r\nContent-Length: {}\r\n\r\n",hex::encode(TOKEN),payload.len()).unwrap();
    stream.write_all(&payload[..1]).unwrap();
    std::thread::sleep(Duration::from_millis(50));
    h.revoke("sender", &Deadline::new(20000)).unwrap();
    stream.write_all(&payload[1..]).unwrap();
    let mut response = vec![];
    let _ = stream.read_to_end(&mut response);
    assert!(response.starts_with(b"HTTP/1.1 401") || response.starts_with(b"HTTP/1.1 403"));
    assert_eq!(count(root.path()), 0);
    h.shutdown().unwrap();
}

#[test]
fn caller_sql_durability_and_corrupted_accepted_bytes_refuse_before_binding() {
    let s = Source::new();
    let root = tempfile::tempdir().unwrap();
    let store = Store::open(root.path()).unwrap();
    store.db.execute_batch("PRAGMA synchronous=NORMAL").unwrap();
    assert!(receiver::start(store, s.receiver_profile(), config(), &Deadline::new(20000)).is_err());
    let h = receiver::start(
        Store::open(root.path()).unwrap(),
        s.receiver_profile(),
        config(),
        &Deadline::new(20000),
    )
    .unwrap();
    let endpoint = h.endpoint().to_owned();
    let address = h.local_addr();
    let proposal = s.proposal(&s.client_profile(&endpoint));
    assert_eq!(
        request(
            &h,
            "PUT",
            proposal["proposalId"].as_str().unwrap(),
            &TOKEN,
            &body(&proposal)
        )
        .0,
        201
    );
    h.shutdown().unwrap();
    let db = rusqlite::Connection::open(root.path().join(".yoke/index.sqlite")).unwrap();
    let raw: String = db
        .query_row(
            "SELECT value FROM records WHERE kind=?1",
            [receiver::RECORD_KIND],
            |r| r.get(0),
        )
        .unwrap();
    let mut value: Value = serde_json::from_str(&raw).unwrap();
    value["body"] = json!("changed stored bytes");
    db.execute(
        "UPDATE records SET value=?1 WHERE kind=?2",
        rusqlite::params![
            serde_json::to_string(&value).unwrap(),
            receiver::RECORD_KIND
        ],
    )
    .unwrap();
    drop(db);
    let mut profile = s.receiver_profile();
    profile.endpoint = endpoint;
    let mut cfg = config();
    cfg.listen = address;
    assert!(
        receiver::start(
            Store::open(root.path()).unwrap(),
            profile,
            cfg,
            &Deadline::new(20000)
        )
        .is_err()
    );
    assert!(std::net::TcpListener::bind(address).is_ok());
}

#[test]
fn payload_capacity_and_expired_partial_body_do_not_commit_late_bytes() {
    let s = Source::new();
    let root = tempfile::tempdir().unwrap();
    let mut profile = s.receiver_profile();
    profile.max_payload_bytes = 1;
    let h = receiver::start(
        Store::open(root.path()).unwrap(),
        profile,
        config(),
        &Deadline::new(20000),
    )
    .unwrap();
    let address = h.local_addr();
    let response=raw(address,format!("PUT /v1/digests/{} HTTP/1.1\r\nHost: {address}\r\nAuthorization: Bearer {}\r\nContent-Type: application/json\r\nIdempotency-Key: {}\r\nContent-Length: 2\r\n\r\n","a".repeat(64),hex::encode(TOKEN),"a".repeat(64)).as_bytes());
    assert!(response.starts_with(b"HTTP/1.1 507"));
    assert_eq!(count(root.path()), 0);
    h.shutdown().unwrap();
    let root = tempfile::tempdir().unwrap();
    let mut cfg = config();
    cfg.request_timeout_ms = 100;
    let h = receiver::start(
        Store::open(root.path()).unwrap(),
        s.receiver_profile(),
        cfg,
        &Deadline::new(20000),
    )
    .unwrap();
    let address = h.local_addr();
    let p = s.proposal(&s.client_profile(h.endpoint()));
    let id = p["proposalId"].as_str().unwrap();
    let payload = body(&p);
    let mut stream = TcpStream::connect(address).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(1)))
        .unwrap();
    write!(stream,"PUT /v1/digests/{id} HTTP/1.1\r\nHost: {address}\r\nAuthorization: Bearer {}\r\nContent-Type: application/json\r\nIdempotency-Key: {id}\r\nContent-Length: {}\r\n\r\n",hex::encode(TOKEN),payload.len()).unwrap();
    stream.write_all(&payload[..1]).unwrap();
    std::thread::sleep(Duration::from_millis(180));
    let _ = stream.write_all(&payload[1..]);
    let mut response = vec![];
    let _ = stream.read_to_end(&mut response);
    assert!(!response.starts_with(b"HTTP/1.1 2"));
    assert_eq!(count(root.path()), 0);
    h.shutdown().unwrap();
}

#[test]
fn real_tls_accepts_selected_ca_and_refuses_wrong_trust_or_name_before_intake() {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let s = Source::new();
    let root = tempfile::tempdir().unwrap();
    let key = rcgen::KeyPair::generate().unwrap();
    let params = rcgen::CertificateParams::new(vec!["127.0.0.1".into()]).unwrap();
    let cert = params.self_signed(&key).unwrap();
    let mut profile = s.receiver_profile();
    profile.endpoint = "https://127.0.0.1:0/".into();
    let mut cfg = config();
    cfg.tls = Some(yoke::federation_http::TlsMaterial {
        certificates: vec![cert.der().to_vec()],
        private_key: zeroize::Zeroizing::new(key.serialize_der()),
    });
    let h = receiver::start(
        Store::open(root.path()).unwrap(),
        profile,
        cfg,
        &Deadline::new(20000),
    )
    .unwrap();
    let p = s.proposal(&s.client_profile(h.endpoint()));
    let id = p["proposalId"].as_str().unwrap();
    let raw = body(&p);
    let untrusted = reqwest::blocking::Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(2))
        .build()
        .unwrap();
    assert!(
        untrusted
            .get(format!("{}v1/digests/{id}", h.endpoint()))
            .bearer_auth(hex::encode(TOKEN))
            .send()
            .is_err()
    );
    assert_eq!(count(root.path()), 0);
    let trusted = reqwest::blocking::Client::builder()
        .no_proxy()
        .add_root_certificate(reqwest::Certificate::from_der(cert.der()).unwrap())
        .timeout(Duration::from_secs(3))
        .build()
        .unwrap();
    assert!(
        trusted
            .get(format!(
                "https://localhost:{}/v1/digests/{id}",
                h.local_addr().port()
            ))
            .bearer_auth(hex::encode(TOKEN))
            .send()
            .is_err()
    );
    assert_eq!(count(root.path()), 0);
    let response = trusted
        .put(format!("{}v1/digests/{id}", h.endpoint()))
        .bearer_auth(hex::encode(TOKEN))
        .header("Content-Type", "application/json")
        .header("Idempotency-Key", id)
        .body(raw.clone())
        .send()
        .unwrap();
    assert_eq!(response.status().as_u16(), 201);
    let ack: Value = serde_json::from_slice(&response.bytes().unwrap()).unwrap();
    assert_eq!(ack["payloadHash"], hash(&raw));
    assert_eq!(count(root.path()), 1);
    h.shutdown().unwrap();
}

#[cfg(windows)]
#[test]
#[ignore = "Requires actual Windows Credential Manager; creates/removes three unique TLS/token/signing credentials and starts only owned loopback CLI processes"]
fn actual_tls_receiver_and_sender_cli_recover_using_os_credentials_and_selected_ca() {
    use std::{
        io::{BufRead, BufReader},
        process::{Command, Stdio},
    };
    struct Credentials(Vec<String>);
    impl Drop for Credentials {
        fn drop(&mut self) {
            #[link(name = "Advapi32")]
            unsafe extern "system" {
                fn CredDeleteW(target: *const u16, kind: u32, flags: u32) -> i32;
            }
            for target in &self.0 {
                let wide: Vec<_> = target.encode_utf16().chain(Some(0)).collect();
                let result = unsafe { CredDeleteW(wide.as_ptr(), 1, 0) };
                if !std::thread::panicking() {
                    assert_ne!(result, 0, "owned credential cleanup failed");
                }
            }
        }
    }
    struct Child(std::process::Child);
    impl Drop for Child {
        fn drop(&mut self) {
            if self.0.try_wait().ok().flatten().is_none() {
                let _ = self.0.kill();
                let _ = self.0.wait();
            }
        }
    }
    let s = Source::new();
    let receiver_root = tempfile::tempdir().unwrap();
    let host = tempfile::tempdir().unwrap();
    let mut nonce = [0_u8; 16];
    getrandom::fill(&mut nonce).unwrap();
    let name = format!("test-receiver-{}", hex::encode(nonce));
    let mut created = Credentials(vec![]);
    let token = yoke::host_credentials::generate_token(&name).unwrap();
    created
        .0
        .push(format!("yoke-code-intelligence/service/token/{name}"));
    let tls = yoke::host_credentials::generate_tls(&name, &["127.0.0.1".into()]).unwrap();
    created
        .0
        .push(format!("yoke-code-intelligence/service/tls/{name}"));
    yoke::host_credentials::import_github_webhook_secret(&name, b"owned-receiver-signing-key")
        .unwrap();
    created.0.push(format!(
        "yoke-code-intelligence/service/github-webhook/{name}"
    ));
    use base64::Engine as _;
    let ca = base64::prelude::BASE64_STANDARD
        .decode(tls["certificateDerBase64"].as_str().unwrap())
        .unwrap();
    std::fs::write(host.path().join("ca.der"), ca).unwrap();
    let mut profile = s.receiver_profile();
    profile.endpoint = "https://127.0.0.1:0/".into();
    profile.grants[0].token_hash = token["tokenHash"].as_str().unwrap().into();
    profile.grants[0].allow_query = true;
    std::fs::write(
        host.path().join("receiver.json"),
        serde_json::to_vec(&profile).unwrap(),
    )
    .unwrap();
    let mut command = Command::new(env!("CARGO_BIN_EXE_yoke"));
    command
        .arg("--root")
        .arg(receiver_root.path())
        .args([
            "--allow-writes",
            "--timeout",
            "20000",
            "digest-receiver",
            "--allow-receiver",
            "--allow-network",
            "--allow-fixture",
            "--allow-query",
            "--max-response-bytes",
            "64000",
            "--max-requests",
            "3",
            "--profile",
        ])
        .arg(host.path().join("receiver.json"))
        .arg("--tls-credential")
        .arg(&name)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    use std::os::windows::process::CommandExt;
    command.creation_flags(0x08000000);
    let mut child = Child(command.spawn().unwrap());
    let mut line = String::new();
    BufReader::new(child.0.stdout.take().unwrap())
        .read_line(&mut line)
        .unwrap();
    assert!(
        !line.is_empty(),
        "owned receiver CLI produced no announcement"
    );
    let announced: Value = serde_json::from_str(&line).unwrap();
    let mut service = s.client_profile(announced["endpoint"].as_str().unwrap());
    service.credential_name = Some(name.clone());
    let approval = s.approval(&service);
    let mut app = app_value();
    app["secretCredential"] = json!(name);
    for (file, value) in [
        ("app.json", app),
        ("service.json", serde_json::to_value(&service).unwrap()),
        ("approval.json", serde_json::to_value(&approval).unwrap()),
        ("source.json", serde_json::to_value(&s.request).unwrap()),
        ("capture.json", serde_json::to_value(&s.capture).unwrap()),
    ] {
        std::fs::write(host.path().join(file), serde_json::to_vec(&value).unwrap()).unwrap();
    }
    s.engine.store.db.execute_batch("CREATE TRIGGER refuse_os_ack BEFORE INSERT ON records WHEN NEW.kind='github_app_digest_service_v1' AND instr(NEW.value,'\"type\":\"accepted\"')>0 BEGIN SELECT RAISE(ABORT,'owned local failure'); END;").unwrap();
    let cli = || {
        let mut c = Command::new(env!("CARGO_BIN_EXE_yoke"));
        c.creation_flags(0x08000000);
        c.arg("--root")
            .arg(s.root.path())
            .args([
                "--allow-writes",
                "--max-bytes",
                "4000000",
                "github-app",
                "--profile",
            ])
            .arg(host.path().join("app.json"))
            .args([
                "digest",
                "--allow-digest",
                "--allow-service",
                "--allow-service-network",
                "--service-profile",
            ])
            .arg(host.path().join("service.json"))
            .arg("--service-ca")
            .arg(host.path().join("ca.der"));
        c
    };
    let mut send = cli();
    send.args([
        "--allow-discussions",
        "--allow-fixture-discussions",
        "--discussion-capture",
    ])
    .arg(host.path().join("capture.json"))
    .arg("--service-approval")
    .arg(host.path().join("approval.json"))
    .args(["preview", "--request"])
    .arg(host.path().join("source.json"));
    let sent = send.output().unwrap();
    assert!(!sent.status.success());
    assert!(sent.stdout.is_empty());
    assert_eq!(count(receiver_root.path()), 1);
    s.engine
        .store
        .db
        .execute_batch("DROP TRIGGER refuse_os_ack")
        .unwrap();
    let mut recover = cli();
    recover.args(["service-recover", "--request-id", &approval.request_id]);
    let recovered = recover.output().unwrap();
    assert!(
        recovered.status.success(),
        "{}",
        String::from_utf8_lossy(&recovered.stderr)
    );
    assert_eq!(
        serde_json::from_slice::<Value>(&recovered.stdout).unwrap()["recovered"],
        true
    );
    // The remote query CLI uses only the service token and public CA. An
    // unavailable App signing credential and an empty cwd prove no sender
    // Store/key is opened by this read-only operation.
    let q = query_request(&s, vec![approval.proposal_id.clone()]);
    let mut query_app = app_value();
    query_app["secretCredential"] = json!("query-no-such-signing-key");
    std::fs::write(
        host.path().join("query-app.json"),
        serde_json::to_vec(&query_app).unwrap(),
    )
    .unwrap();
    std::fs::write(
        host.path().join("query.json"),
        serde_json::to_vec(&q).unwrap(),
    )
    .unwrap();
    let mut query_command = Command::new(env!("CARGO_BIN_EXE_yoke"));
    query_command.creation_flags(0x08000000);
    query_command
        .current_dir(host.path())
        .args([
            "--max-bytes",
            "64000",
            "digest-inbox",
            "remote",
            "--allow-query",
            "--allow-network",
            "--profile",
        ])
        .arg(host.path().join("service.json"))
        .arg("--app-profile")
        .arg(host.path().join("query-app.json"))
        .arg("--request")
        .arg(host.path().join("query.json"))
        .arg("--ca")
        .arg(host.path().join("ca.der"));
    let queried = query_command.output().unwrap();
    assert!(
        queried.status.success(),
        "{}",
        String::from_utf8_lossy(&queried.stderr)
    );
    let result: Value = serde_json::from_slice(&queried.stdout).unwrap();
    assert_eq!(result["networkRequests"], 1);
    assert_eq!(
        result["result"]["manifests"][0]["proposalId"],
        approval.proposal_id
    );
    assert_eq!(result["result"]["sourceAuthority"], false);
    assert!(!host.path().join(".yoke").exists());
    let end = std::time::Instant::now() + Duration::from_secs(3);
    let status = loop {
        if let Some(status) = child.0.try_wait().unwrap() {
            break status;
        }
        assert!(
            std::time::Instant::now() < end,
            "owned receiver did not finish after exactly three requests"
        );
        std::thread::sleep(Duration::from_millis(10));
    };
    assert!(status.success());
    assert_eq!(count(receiver_root.path()), 1);
}
