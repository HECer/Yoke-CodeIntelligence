use serde_json::{Value, json};
use std::{
    io::{Read, Write},
    net::{SocketAddr, TcpStream},
    time::{Duration, Instant},
};
use yoke::{
    mcp_http::{self, Config, Principal, Target},
    project_session::HostProfile,
    protocol::Profile,
};
use zeroize::Zeroizing;

fn exchange(address: SocketAddr, sid: Option<&str>, value: Value) -> (u16, String) {
    let body = serde_json::to_vec(&value).unwrap();
    let session = sid
        .map(|s| format!("Mcp-Session-Id: {s}\r\nMcp-Protocol-Version: 2025-11-25\r\n"))
        .unwrap_or_default();
    let mut socket = TcpStream::connect(address).unwrap();
    socket
        .set_read_timeout(Some(Duration::from_secs(8)))
        .unwrap();
    write!(socket, "POST /mcp HTTP/1.1\r\nHost: {address}\r\nAuthorization: Bearer {}\r\nAccept: application/json, text/event-stream\r\nContent-Type: application/json\r\nContent-Length: {}\r\n{session}\r\n", hex::encode([47;32]), body.len()).unwrap();
    socket.write_all(&body).unwrap();
    let mut response = Vec::new();
    socket.read_to_end(&mut response).unwrap();
    let end = response.windows(4).position(|b| b == b"\r\n\r\n").unwrap();
    let header = std::str::from_utf8(&response[..end]).unwrap();
    let sid = header
        .lines()
        .find_map(|line| {
            line.split_once(':')
                .filter(|(k, _)| k.eq_ignore_ascii_case("mcp-session-id"))
                .map(|(_, v)| v.trim().to_owned())
        })
        .unwrap_or_default();
    (
        header.split_whitespace().nth(1).unwrap().parse().unwrap(),
        sid,
    )
}

#[test]
fn replacing_principal_policy_cancels_running_session_before_waiting_for_read_lease() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("main.rs"), "fn target(){}\n").unwrap();
    let marker = root.path().join("hover.log");
    let mut target = Target::new(root.path(), Profile::Session, HostProfile::default());
    target.servers.push(yoke::lsp::ConfiguredServer {
        server: yoke::lsp::Server {
            language: "rust".into(),
            command: "node".into(),
            args: vec![
                std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("tests/fixtures/mcp-cancel-lsp.mjs")
                    .display()
                    .to_string(),
                marker.display().to_string(),
            ],
            trust: true,
        },
        configuration: yoke::lsp::HostConfiguration {
            passive_profile: Some(yoke::lsp::PassiveProfile::ContractFixture),
            ..Default::default()
        },
    });
    let old = Principal::new("replace", Zeroizing::new(vec![47; 32]), target).unwrap();
    let replacement = Principal::new(
        "replace",
        Zeroizing::new(vec![48; 32]),
        Target::new(root.path(), Profile::Session, HostProfile::default()),
    )
    .unwrap();
    let server = mcp_http::serve(
        Config {
            workers: 1,
            request_timeout_ms: 4000,
            ..Default::default()
        },
        vec![old],
    )
    .unwrap();
    let address = server.local_addr();
    let (status, sid) = exchange(
        address,
        None,
        json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25"}}),
    );
    assert_eq!(status, 200);
    let running = std::thread::spawn(move || {
        exchange(
            address,
            Some(&sid),
            json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"ci_find","arguments":{"languageServer":{"path":"main.rs","offset":3,"operation":"hover"},"timeoutMs":600000}}}),
        )
    });
    let wait = Instant::now();
    while !marker.exists() && wait.elapsed() < Duration::from_secs(5) {
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(
        marker.exists(),
        "Owned provider must actually enter the running request"
    );
    let start = Instant::now();
    server.upsert(replacement).unwrap();
    let elapsed = start.elapsed();
    let response = running.join().unwrap();
    server.shutdown().unwrap();
    assert_eq!(response.0, 503);
    assert!(
        elapsed < Duration::from_secs(2),
        "Policy replacement waited for obsolete request timeout: {elapsed:?}"
    );
}

#[test]
fn collection_grants_are_borrowed_per_dispatch_and_sources_remain_noncanonical() {
    use std::collections::BTreeMap;
    use yoke::{
        Engine,
        capture::Deadline,
        federation::{self, ExportScope, ImportGrant, Scope},
        federation_archive::FederationArchiveAccess,
        graph_collections::Registry,
        project_session::{HostCollection, Session},
        protocol::{CollectionGrants, ConnectionState},
    };
    let d = Deadline::new(60000);
    let sender_root = tempfile::tempdir().unwrap();
    let receiver_root = tempfile::tempdir().unwrap();
    let registry_root = tempfile::tempdir().unwrap();
    std::fs::write(sender_root.path().join("main.rs"), "fn foreign(){}\n").unwrap();
    let mut sender = Engine::open(sender_root.path()).unwrap();
    let receiver = Engine::open(receiver_root.path()).unwrap();
    let (snapshot, _, _, _) = sender.index(&d).unwrap();
    let secret = [51; 32];
    let scope = Scope {
        tenant_id: "tenant".into(),
        project_id: "project".into(),
        sender_id: "sender".into(),
        key_id: "key".into(),
    };
    let grant = ImportGrant {
        scope: scope.clone(),
        public_key: hex::encode(
            ed25519_dalek::SigningKey::from_bytes(&secret)
                .verifying_key()
                .to_bytes(),
        ),
        sender_workspace_id: snapshot.workspace_id.clone(),
        exclude: vec![],
    };
    let wire = federation::export_snapshot(
        &sender.store,
        &snapshot,
        &ExportScope {
            scope,
            ttl_ms: 60000,
        },
        &secret,
        &d,
    )
    .unwrap();
    let imported = federation::import_snapshot(&receiver, wire, &grant, &d).unwrap();
    let grants = BTreeMap::from([("current".into(), grant)]);
    let registry = Registry::open(registry_root.path()).unwrap();
    registry
        .register_federated_sources_cas(
            "foreign",
            &FederationArchiveAccess::new(&receiver, &grants),
            "current",
            imported["remoteId"].as_str().unwrap(),
            None,
            &d,
        )
        .unwrap();
    drop(registry);
    let host = HostProfile {
        collections: vec![HostCollection {
            name: "library".into(),
            registry_root: registry_root.path().into(),
            allowed_members: vec!["foreign".into()],
            allow_registration: false,
            allow_registry_coordination: cfg!(unix),
            receiver_project: Some("default".into()),
        }],
        ..Default::default()
    };
    let mut session = Session::open(receiver, host, Some("grant-review"), &d).unwrap();
    let mut state = ConnectionState::default();
    state.message_session(&mut session,Profile::Session,&serde_json::to_vec(&json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25"}})).unwrap()).unwrap();
    let policy = CollectionGrants::from([("library".into(), grants)]);
    let dispatch = |state: &mut ConnectionState,
                    session: &mut Session,
                    policy: Option<&CollectionGrants>,
                    operation: &str,
                    args: Value| {
        let bytes = serde_json::to_vec(&json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"ci_session","arguments":{"operation":operation,"args":args}}})).unwrap();
        let response = match policy {
            Some(grants) => state.message_session_with_collection_grants(session, grants, &bytes),
            None => state.message_session(session, Profile::Session, &bytes),
        }
        .unwrap()
        .unwrap();
        serde_json::from_str::<Value>(response["result"]["content"][0]["text"].as_str().unwrap())
            .unwrap()
    };
    let read = json!({"collection":"library","member":"foreign","path":"main.rs","startByte":0,"endByte":15});
    assert_eq!(
        dispatch(
            &mut state,
            &mut session,
            Some(&policy),
            "collections.read",
            read.clone()
        )["data"]["text"],
        "fn foreign(){}\n"
    );
    assert_eq!(
        dispatch(
            &mut state,
            &mut session,
            None,
            "collections.read",
            read.clone()
        )["status"],
        "error",
        "Prior borrowed authority must not remain in Session/ConnectionState"
    );
    assert_eq!(
        dispatch(
            &mut state,
            &mut session,
            Some(&CollectionGrants::new()),
            "collections.read",
            read.clone()
        )["status"],
        "error"
    );
    let joined = dispatch(
        &mut state,
        &mut session,
        Some(&policy),
        "collections.join",
        json!({"collection":"library","members":["foreign"]}),
    );
    assert_eq!(joined["status"], "ok");
    assert_eq!(joined["data"]["nodes"].as_array().unwrap().len(), 0);
    assert_eq!(joined["data"]["observations"].as_array().unwrap().len(), 0);
    assert_eq!(
        joined["data"]["lineage"][0]["sourceAuthority"]["kind"],
        "federatedSources"
    );
    assert_eq!(
        dispatch(
            &mut state,
            &mut session,
            Some(&policy),
            "collections.read",
            read
        )["status"],
        "ok"
    );
}

#[test]
fn non_session_profile_rejects_host_collection_grants_before_opening_source_state() {
    use std::collections::BTreeMap;
    use yoke::{
        federation::{ImportGrant, Scope},
        project_session::HostCollection,
    };
    let root = tempfile::tempdir().unwrap();
    let registry = tempfile::tempdir().unwrap();
    let host = HostProfile {
        collections: vec![HostCollection {
            name: "library".into(),
            registry_root: registry.path().into(),
            allowed_members: vec!["foreign".into()],
            allow_registration: false,
            allow_registry_coordination: cfg!(unix),
            receiver_project: Some("default".into()),
        }],
        ..Default::default()
    };
    let mut target = Target::new(root.path(), Profile::Native, host);
    target.collection_grants.insert(
        "library".into(),
        BTreeMap::from([(
            "current".into(),
            ImportGrant {
                scope: Scope {
                    tenant_id: "tenant".into(),
                    project_id: "project".into(),
                    sender_id: "sender".into(),
                    key_id: "key".into(),
                },
                public_key: "00".repeat(32),
                sender_workspace_id: "fixture".into(),
                exclude: vec![],
            },
        )]),
    );
    assert!(Principal::new("profile-review", Zeroizing::new(vec![37; 32]), target).is_err());
    assert!(!root.path().join(".yoke").exists());
}
