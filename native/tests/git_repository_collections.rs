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
    git_workflows::{
        HostGit,
        repository_host::Host,
        repository_preferences::{self, Choice, Expected, Resolved},
        repository_projects::Hosts,
        repository_scope::Options,
    },
    graph_collections::Registry,
    mcp_http::{self, Config, Principal, Target},
    project_session::{
        CollectionInvocationAccess, HostCollection, HostProfile, HostProject, Session,
    },
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

fn d() -> Deadline {
    Deadline::new(60_000)
}
fn repository(root: &std::path::Path, git: &HostGit, files: &[&str]) {
    std::fs::create_dir_all(root).unwrap();
    let run = |args: &[&str]| {
        let output = std::process::Command::new(&git.executable)
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
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    };
    run(&["init", "--quiet", "--template="]);
    for file in files {
        let path = root.join(file);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, b"pub fn owned(){}\r\n").unwrap();
        run(&["add", "--", file]);
    }
    run(&["commit", "--quiet", "-m", "Owned source"]);
}
fn profile(
    secondary: &std::path::Path,
    registry: &std::path::Path,
    write: bool,
    receiver: bool,
) -> HostProfile {
    HostProfile {
        projects: vec![HostProject {
            name: "secondary".into(),
            root: secondary.into(),
            read_only: true,
            query_only: true,
        }],
        collections: vec![HostCollection {
            name: "library".into(),
            registry_root: registry.into(),
            allowed_members: vec!["native".into(), "foreign".into()],
            allow_registration: write,
            allow_registry_coordination: cfg!(unix),
            receiver_project: receiver.then(|| "secondary".into()),
        }],
        ..Default::default()
    }
}
fn hosts(root: &std::path::Path, host: &HostProfile, git: &HostGit) -> Hosts {
    let drivers = std::iter::once(("default", root))
        .chain(
            host.projects
                .iter()
                .map(|p| (p.name.as_str(), p.root.as_path())),
        )
        .map(|(name, path)| {
            (
                name.into(),
                Host::new(
                    path,
                    git.clone(),
                    Options {
                        allow_index_read: true,
                        ..Default::default()
                    },
                    Resolved::current(path, None, Some(true), &d()).unwrap(),
                    &d(),
                )
                .unwrap(),
            )
        })
        .collect();
    Hosts::new(root, host, drivers, &d()).unwrap()
}
fn session(root: &std::path::Path, host: HostProfile, git: &HostGit) -> Session {
    let selected = hosts(root, &host, git);
    let engine = selected.open_primary(&d()).unwrap();
    let mut session =
        Session::open_with_git_projects(engine, host, Some("owned-collections"), selected, &d())
            .unwrap();
    session.index_initial_git_source(&d()).unwrap();
    session
}
fn target(root: &std::path::Path, host: HostProfile, git: &HostGit) -> Target {
    let selected = hosts(root, &host, git);
    let mut target = Target::new(root, Profile::Session, host);
    target.git_repository_projects = Some(selected);
    target
}
fn snapshots(root: &std::path::Path) -> Vec<Value> {
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
        .map(|r| serde_json::from_str(&r.unwrap()).unwrap())
        .collect()
}
fn move_owned(source: &std::path::Path, destination: &std::path::Path, root: &std::path::Path) {
    let root = root.canonicalize().unwrap();
    assert_eq!(
        source.canonicalize().unwrap().parent(),
        Some(root.as_path())
    );
    assert_eq!(destination.parent().unwrap().canonicalize().unwrap(), root);
    std::fs::rename(source, destination).unwrap();
}

#[test]
fn scoped_native_registration_and_archive_replay_preserve_filtered_source_without_live_git() {
    let fixture = tempfile::tempdir().unwrap();
    let git = HostGit::discover().unwrap();
    let a = fixture.path().join("a");
    let b = fixture.path().join("secondary \u{df}");
    let registry = fixture.path().join("registry");
    std::fs::create_dir(&registry).unwrap();
    repository(&a, &git, &["main.rs"]);
    repository(&b, &git, &["main.rs", "excluded.rs"]);
    repository(&b.join("nested"), &git, &["child.rs"]);
    let child = std::fs::read(b.join("nested/child.rs")).unwrap();
    let mut host = profile(&b, &registry, true, false);
    host.source_exclusions = Some(
        serde_json::from_value(json!({"projects":{"secondary":{"local":[["excluded.rs"]]}}}))
            .unwrap(),
    );
    let mut session = session(&a, host, &git);
    assert!(!b.join(".yoke").exists());
    let registered=session.invoke("collections.register",&json!({"collection":"library","member":"native","kind":"native","project":"secondary","expectedRegistration":null}),&d()).unwrap();
    assert!(registered["registrationId"].as_str().is_some());
    let selected = snapshots(&b);
    assert_eq!(selected.len(), 1);
    assert!(
        !selected[0]["files"]
            .as_array()
            .unwrap()
            .iter()
            .any(|f| f["path"] == "excluded.rs")
    );
    move_owned(&b.join(".git"), &b.join("git-off"), &b);
    move_owned(
        &b.join("nested/child.rs"),
        &b.join("nested/child-off.rs"),
        &b.join("nested"),
    );
    let read=session.invoke("collections.read",&json!({"collection":"library","member":"native","path":"nested/child.rs","startByte":0,"endByte":child.len()}),&d()).unwrap();
    assert_eq!(read["text"].as_str().unwrap().as_bytes(), child);
    assert!(session.invoke("collections.read",&json!({"collection":"library","member":"native","path":"excluded.rs","startByte":0,"endByte":1}),&d()).is_err());
    assert_eq!(snapshots(&b), selected);
    let stale=session.invoke("collections.register",&json!({"collection":"library","member":"native","kind":"native","project":"secondary","expectedRegistration":null}),&d()).unwrap_err();
    assert!(stale.to_string().contains("stale"));
    move_owned(&b.join("git-off"), &b.join(".git"), &b);
    move_owned(
        &b.join("nested/child-off.rs"),
        &b.join("nested/child.rs"),
        &b.join("nested"),
    );
    assert_eq!(std::fs::read(b.join("nested/child.rs")).unwrap(), child);
}

#[test]
fn registration_permission_and_current_preferences_precede_registry_or_unused_source_effects() {
    let fixture = tempfile::tempdir().unwrap();
    let git = HostGit::discover().unwrap();
    let a = fixture.path().join("a");
    let b = fixture.path().join("b");
    let registry = fixture.path().join("registry");
    std::fs::create_dir(&registry).unwrap();
    repository(&a, &git, &["main.rs"]);
    std::fs::create_dir(&b).unwrap();
    let mut session = session(&a, profile(&b, &registry, false, true), &git);
    let args = json!({"collection":"library","member":"native","kind":"native","project":"secondary","expectedRegistration":null});
    assert!(
        session
            .invoke("collections.register", &args, &d())
            .unwrap_err()
            .to_string()
            .contains("disabled")
    );
    assert!(!b.join(".yoke").exists() && !registry.join("yoke-collection.sqlite").exists());
    let receiver = Engine::open(&b).unwrap();
    repository_preferences::set(
        &receiver.store,
        &Expected::Absent,
        Choice::default(),
        true,
        &d(),
    )
    .unwrap();
    // The explicit capability's lifetime is limited to this invocation.
    let grants = BTreeMap::new();
    let cap = CollectionInvocationAccess::new("library", "secondary", &receiver, &grants).unwrap();
    assert!(
        session
            .invoke_collection_with_access(
                "collections.list",
                &json!({"collection":"library"}),
                &cap,
                &d()
            )
            .unwrap_err()
            .to_string()
            .contains("repository_selection_stale")
    );
    assert!(!registry.join("yoke-collection.sqlite").exists());
}

#[test]
fn current_http_grants_are_independent_of_local_git_membership_and_revocation_closes_sessions() {
    let fixture = tempfile::tempdir().unwrap();
    let git = HostGit::discover().unwrap();
    let a = fixture.path().join("a");
    let b = fixture.path().join("receiver \u{df}");
    let source = fixture.path().join("sender");
    let registry_root = fixture.path().join("registry");
    std::fs::create_dir(&registry_root).unwrap();
    std::fs::create_dir(&source).unwrap();
    repository(&a, &git, &["main.rs"]);
    repository(&b, &git, &["local.rs"]);
    let text = "fn foreign_source(){}\r\n";
    std::fs::write(source.join("foreign.rs"), text).unwrap();
    let mut sender = Engine::open(&source).unwrap();
    let receiver = Engine::open(&b).unwrap();
    let (snapshot, _, _, _) = sender.index(&d()).unwrap();
    let secret = [71; 32];
    let scope = Scope {
        tenant_id: "owned".into(),
        project_id: "library".into(),
        sender_id: "sender".into(),
        key_id: "test-key".into(),
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
            ttl_ms: 60_000,
        },
        &secret,
        &d(),
    )
    .unwrap();
    let imported = federation::import_snapshot(&receiver, wire, &grant, &d()).unwrap();
    let remote = imported["remoteId"].as_str().unwrap();
    let grants = BTreeMap::from([("live".into(), grant.clone())]);
    let registry = Registry::open(&registry_root).unwrap();
    let access = FederationArchiveAccess::new(&receiver, &grants);
    registry
        .register_federated_sources_cas("foreign", &access, "live", remote, None, &d())
        .unwrap();
    drop(registry);
    let before = snapshots(&b);
    // Imports retain signed receipts rather than creating local disk snapshots.
    assert!(before.is_empty());
    let host = profile(&b, &registry_root, false, true);
    let make = |id: &str, token: u8, selected: Option<ImportGrant>| {
        let mut target = target(&a, host.clone(), &git);
        if let Some(grant) = selected {
            target
                .collection_grants
                .insert("library".into(), BTreeMap::from([("live".into(), grant)]));
        }
        Principal::new(id, Zeroizing::new(vec![token; 32]), target).unwrap()
    };
    let server = mcp_http::serve(
        Config {
            request_timeout_ms: 30_000,
            ..Default::default()
        },
        vec![
            make("allowed", 91, Some(grant.clone())),
            make("denied", 92, None),
        ],
    )
    .unwrap();
    let address = server.local_addr();
    let sid = initialize(address, 91);
    let denied = initialize(address, 92);
    let args = json!({"collection":"library","member":"foreign","path":"foreign.rs","startByte":0,"endByte":text.len()});
    let (status, value) = call(address, 91, &sid, "collections.read", args.clone());
    assert_eq!(status, 200);
    assert_eq!(value["status"], "ok", "{value}");
    assert_eq!(value["data"]["text"], text);
    assert_eq!(snapshots(&b), before);
    let (_, value) = call(address, 92, &denied, "collections.read", args.clone());
    assert_eq!(value["status"], "error");
    let mut injected = args.clone();
    injected["grant"] = serde_json::to_value(&grant).unwrap();
    assert_eq!(
        call(address, 91, &sid, "collections.read", injected).1["status"],
        "error"
    );
    let mut narrowed = grant;
    narrowed.exclude.push("foreign.rs".into());
    server.upsert(make("allowed", 91, Some(narrowed))).unwrap();
    assert_ne!(
        call(address, 91, &sid, "collections.read", args.clone()).0,
        200
    );
    let current = initialize(address, 91);
    assert_eq!(
        call(address, 91, &current, "collections.read", args).1["status"],
        "error"
    );
    assert_eq!(snapshots(&b), before);
    let mut session = session(&a, host, &git);
    let wrong = Engine::open(&a).unwrap();
    let cap = CollectionInvocationAccess::new("library", "secondary", &wrong, &grants).unwrap();
    assert!(
        session
            .invoke_collection_with_access(
                "collections.describe",
                &json!({"collection":"library","member":"foreign"}),
                &cap,
                &d()
            )
            .is_err()
    );
}

#[test]
fn native_archive_with_unused_receiver_does_not_open_or_capture_that_project() {
    let fixture = tempfile::tempdir().unwrap();
    let git = HostGit::discover().unwrap();
    let a = fixture.path().join("a");
    let unused = fixture.path().join("unused");
    let source = fixture.path().join("original");
    let central = fixture.path().join("central");
    repository(&a, &git, &["main.rs"]);
    std::fs::create_dir(&unused).unwrap();
    std::fs::create_dir(&source).unwrap();
    std::fs::create_dir(&central).unwrap();
    let text = "fn archived_only(){}\n";
    std::fs::write(source.join("main.rs"), text).unwrap();
    let mut original = Engine::open(&source).unwrap();
    let (snapshot, graph, _, _) = original.index(&d()).unwrap();
    let registry = Registry::open(&central).unwrap();
    registry
        .register("native", &original.store, &snapshot, &graph, &d())
        .unwrap();
    drop(registry);
    drop(original);
    let mut target = target(&a, profile(&unused, &central, false, true), &git);
    target.collection_grants.insert(
        "library".into(),
        BTreeMap::from([(
            "unused".into(),
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
        Config {
            request_timeout_ms: 30_000,
            ..Default::default()
        },
        vec![Principal::new("reader", Zeroizing::new(vec![93; 32]), target).unwrap()],
    )
    .unwrap();
    let address = server.local_addr();
    let sid = initialize(address, 93);
    let (_, value) = call(
        address,
        93,
        &sid,
        "collections.read",
        json!({"collection":"library","member":"native","path":"main.rs","startByte":0,"endByte":text.len()}),
    );
    assert_eq!(value["status"], "ok", "{value}");
    assert_eq!(value["data"]["text"], text);
    assert!(!unused.join(".yoke").exists());
}

#[test]
fn fixed_single_git_host_keeps_its_collection_refusal_and_project_set_checks_receiver_names_first()
{
    let fixture = tempfile::tempdir().unwrap();
    let git = HostGit::discover().unwrap();
    let a = fixture.path().join("a");
    let b = fixture.path().join("b");
    let registry = fixture.path().join("registry");
    repository(&a, &git, &["main.rs"]);
    repository(&b, &git, &["main.rs"]);
    std::fs::create_dir(&registry).unwrap();
    let mut host = profile(&b, &registry, false, true);
    let primary = Host::new(
        &a,
        git.clone(),
        Options {
            allow_index_read: true,
            ..Default::default()
        },
        Resolved::current(&a, None, None, &d()).unwrap(),
        &d(),
    )
    .unwrap();
    let mut target = Target::new(&a, Profile::Session, host.clone());
    target.git_repository_capture = Some(primary);
    assert!(Principal::new("single", Zeroizing::new(vec![94; 32]), target).is_err());
    assert!(!a.join(".yoke").exists());
    host.collections[0].receiver_project = Some("unregistered".into());
    assert!(
        Hosts::validate_selectors(
            &host,
            &["default".to_owned(), "secondary".to_owned()].into()
        )
        .is_err()
    );
    assert!(!b.join(".yoke").exists());
}

#[test]
fn http_collection_grant_profile_and_receiver_admission_precede_credentials_or_cold_state() {
    for (profile, receiver, grant_name, expected) in [
        (
            "serena",
            "default",
            "library",
            "Collection grants require the Session profile",
        ),
        (
            "session",
            "default",
            "unregistered",
            "Collection grant policy is not host-registered",
        ),
        (
            "session",
            "unregistered",
            "library",
            "Collection receiver project is not registered",
        ),
    ] {
        let fixture = tempfile::tempdir().unwrap();
        let source = fixture.path().join("source");
        std::fs::create_dir(&source).unwrap();
        let registry = fixture.path().join("not-created");
        let file = fixture.path().join("host.json");
        let grant = json!({"scope":{"tenantId":"t","projectId":"p","senderId":"s","keyId":"k"},"publicKey":"a".repeat(64),"senderWorkspaceId":"b".repeat(64),"exclude":[]});
        let mut grants = serde_json::Map::new();
        grants.insert(grant_name.into(), json!({"live":grant}));
        let host = json!({"principals":[{"id":"owned","profile":profile,"tokenCredentialName":"must-not-be-read","hostProfile":{"collections":[{"name":"library","registryRoot":registry,"allowedMembers":["foreign"],"receiverProject":receiver}]},"gitRepositoryProjects":{"default":{}},"collectionGrants":grants}]});
        std::fs::write(&file, serde_json::to_vec(&host).unwrap()).unwrap();
        let output = std::process::Command::new(env!("CARGO_BIN_EXE_yoke"))
            .arg("--root")
            .arg(&source)
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
        assert!(!source.join(".yoke").exists() && !registry.exists());
    }
}
