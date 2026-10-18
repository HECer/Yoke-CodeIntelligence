use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    io::{Read, Write},
    net::{SocketAddr, TcpStream},
    time::Duration,
};
use yoke::{
    Engine,
    capture::Deadline,
    federation::{self, ExportScope, ImportGrant, Scope},
    federation_archive::FederationArchiveAccess,
    graph_collections::Registry,
    mcp_http::{self, Config, Principal, Target},
    project_session::{HostCollection, HostProfile},
    protocol::Profile,
};
use zeroize::Zeroizing;

fn exchange(
    address: SocketAddr,
    token: u8,
    sid: Option<&str>,
    body: Value,
) -> (u16, String, Value) {
    let body = serde_json::to_vec(&body).unwrap();
    let session = sid
        .map(|s| format!("Mcp-Session-Id: {s}\r\nMcp-Protocol-Version: 2025-11-25\r\n"))
        .unwrap_or_default();
    let header = format!(
        "POST /mcp HTTP/1.1\r\nHost: {address}\r\nAuthorization: Bearer {}\r\nAccept: application/json, text/event-stream\r\nContent-Type: application/json\r\nContent-Length: {}\r\n{session}\r\n",
        hex::encode([token; 32]),
        body.len()
    );
    let mut stream = TcpStream::connect(address).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(15)))
        .unwrap();
    stream.write_all(header.as_bytes()).unwrap();
    stream.write_all(&body).unwrap();
    let mut wire = Vec::new();
    stream.read_to_end(&mut wire).unwrap();
    let split = wire.windows(4).position(|b| b == b"\r\n\r\n").unwrap();
    let headers = std::str::from_utf8(&wire[..split]).unwrap();
    let status = headers.split_whitespace().nth(1).unwrap().parse().unwrap();
    let session = headers
        .lines()
        .find_map(|line| {
            line.split_once(':')
                .filter(|(k, _)| k.eq_ignore_ascii_case("mcp-session-id"))
                .map(|(_, v)| v.trim().to_owned())
        })
        .unwrap_or_default();
    let value = serde_json::from_slice(&wire[split + 4..]).unwrap_or(Value::Null);
    (status, session, value)
}
fn initialize(address: SocketAddr, token: u8) -> String {
    let (status, sid, value) = exchange(
        address,
        token,
        None,
        json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"collection-test","version":"1"}}}),
    );
    assert_eq!(status, 200, "{value}");
    assert!(!sid.is_empty());
    sid
}
fn call(address: SocketAddr, token: u8, sid: &str, operation: &str, args: Value) -> (u16, Value) {
    let (status, _, value) = exchange(
        address,
        token,
        Some(sid),
        json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"ci_session","arguments":{"operation":operation,"args":args}}}),
    );
    let result = value["result"]["structuredContent"].clone();
    let result = if result.is_null() {
        value["result"]["content"][0]["text"]
            .as_str()
            .and_then(|s| serde_json::from_str(s).ok())
            .unwrap_or(value)
    } else {
        result
    };
    (status, result)
}

#[test]
fn current_principal_grants_are_injected_per_http_call_and_removed_on_upsert() {
    let d = Deadline::new(60000);
    let source = tempfile::tempdir().unwrap();
    let receiver_root = tempfile::tempdir().unwrap();
    let registry_root = tempfile::tempdir().unwrap();
    std::fs::write(source.path().join("main.rs"), "fn foreign(){}\n").unwrap();
    let mut sender = Engine::open(source.path()).unwrap();
    let receiver = Engine::open(receiver_root.path()).unwrap();
    let (snapshot, _, _, _) = sender.index(&d).unwrap();
    let secret = [71; 32];
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
    let remote = imported["remoteId"].as_str().unwrap();
    let grants = BTreeMap::from([("live".into(), grant)]);
    let registry = Registry::open(registry_root.path()).unwrap();
    let access = FederationArchiveAccess::new(&receiver, &grants);
    registry
        .register_federated_sources_cas("foreign", &access, "live", remote, None, &d)
        .unwrap();
    drop(registry);
    let host = HostProfile {
        collections: vec![HostCollection {
            name: "library".into(),
            registry_root: registry_root.path().into(),
            allowed_members: vec!["foreign".into()],
            allow_registration: false,
            receiver_project: Some("default".into()),
        }],
        ..Default::default()
    };
    let make = |id: &str, token: u8, allow: bool| {
        let mut target = Target::new(receiver_root.path(), Profile::Session, host.clone());
        if allow {
            target
                .collection_grants
                .insert("library".into(), grants.clone());
        }
        Principal::new(id, Zeroizing::new(vec![token; 32]), target).unwrap()
    };
    let server = mcp_http::serve(
        Config::default(),
        vec![make("allowed", 81, true), make("denied", 82, false)],
    )
    .unwrap();
    let address = server.local_addr();
    let sid = initialize(address, 81);
    let denied = initialize(address, 82);
    let args = json!({"collection":"library","member":"foreign","path":"main.rs","startByte":0,"endByte":15});
    let (status, result) = call(address, 81, &sid, "collections.read", args.clone());
    assert_eq!(status, 200);
    assert_eq!(result["status"], "ok", "{result}");
    assert_eq!(result["data"]["text"], "fn foreign(){}\n");
    let (_, result) = call(address, 82, &denied, "collections.read", args.clone());
    assert_eq!(result["status"], "error", "{result}");
    let (_, result) = call(
        address,
        81,
        &sid,
        "collections.read",
        json!({"collection":"library","member":"foreign","path":"main.rs","startByte":0,"endByte":15,"grant":grants["live"]}),
    );
    assert_eq!(result["status"], "error");
    server.upsert(make("allowed", 81, false)).unwrap();
    assert_ne!(
        call(address, 81, &sid, "collections.read", args.clone()).0,
        200
    );
    let fresh = initialize(address, 81);
    let (_, result) = call(address, 81, &fresh, "collections.read", args);
    assert_eq!(result["status"], "error", "{result}");
}

#[test]
fn native_archive_does_not_open_a_configured_but_unused_federation_receiver() {
    use yoke::project_session::HostProject;
    let d = Deadline::new(30000);
    let origin = tempfile::tempdir().unwrap();
    let central = tempfile::tempdir().unwrap();
    let active = tempfile::tempdir().unwrap();
    let unused = tempfile::tempdir().unwrap();
    std::fs::write(origin.path().join("main.rs"), "fn archived(){}\n").unwrap();
    let mut engine = Engine::open(origin.path()).unwrap();
    let (s, g, _, _) = engine.index(&d).unwrap();
    let registry = Registry::open(central.path()).unwrap();
    registry
        .register("native", &engine.store, &s, &g, &d)
        .unwrap();
    drop(registry);
    drop(engine);
    origin.close().unwrap();
    let host = HostProfile {
        projects: vec![HostProject {
            name: "receiver".into(),
            root: unused.path().into(),
            read_only: true,
            query_only: true,
        }],
        collections: vec![HostCollection {
            name: "library".into(),
            registry_root: central.path().into(),
            allowed_members: vec!["native".into()],
            allow_registration: false,
            receiver_project: Some("receiver".into()),
        }],
        ..Default::default()
    };
    let mut target = Target::new(active.path(), Profile::Session, host);
    target.collection_grants.insert(
        "library".into(),
        BTreeMap::from([(
            "live".into(),
            ImportGrant {
                scope: Scope {
                    tenant_id: "t".into(),
                    project_id: "p".into(),
                    sender_id: "s".into(),
                    key_id: "k".into(),
                },
                public_key: "a".repeat(64),
                sender_workspace_id: "b".repeat(64),
                exclude: vec![],
            },
        )]),
    );
    let server = mcp_http::serve(
        Config::default(),
        vec![Principal::new("reader", Zeroizing::new(vec![83; 32]), target).unwrap()],
    )
    .unwrap();
    let address = server.local_addr();
    let sid = initialize(address, 83);
    let (_, result) = call(
        address,
        83,
        &sid,
        "collections.read",
        json!({"collection":"library","member":"native","path":"main.rs","startByte":0,"endByte":16}),
    );
    assert_eq!(result["status"], "ok", "{result}");
    assert_eq!(result["data"]["text"], "fn archived(){}\n");
    assert!(!unused.path().join(".yoke").exists());
}
