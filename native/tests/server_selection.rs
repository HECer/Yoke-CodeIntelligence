use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};
use yoke::{
    Engine, agent_repl,
    capture::Deadline,
    lsp,
    project_session::{HostProfile, HostProject, Session},
};
const RUST: &str = "// owned \u{1f4a1} e\u{301}\r\nfn visible() {}\r\n";
const CURRENT_BACKENDS: [&str; 3] = ["vbnet", "typescript_tsgo", "graphql"];
fn d() -> Deadline {
    Deadline::new(30000)
}
fn node() -> String {
    let result = Command::new("node")
        .args(["-p", "process.execPath"])
        .output()
        .unwrap();
    assert!(result.status.success());
    String::from_utf8(result.stdout).unwrap().trim().into()
}
fn root(parent: &Path, name: &str) -> PathBuf {
    let root = parent.join(name);
    fs::create_dir(&root).unwrap();
    for (name, body) in [
        ("main.rs", RUST),
        ("entry.ts", "export function entry() {}\n"),
        ("extra.js", "export function extra() {}\n"),
        ("view.html", "<h1>owned</h1>\n"),
    ] {
        fs::write(root.join(name), body).unwrap();
    }
    root
}
fn configured(id: &str, log: &Path, actor: &str, delay: u64) -> lsp::ConfiguredServer {
    lsp::ConfiguredServer {
        server: lsp::Server {
            command: node(),
            args: vec![
                Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("tests/fixtures/server-selection-lsp.mjs")
                    .display()
                    .to_string(),
                log.display().to_string(),
                actor.into(),
                delay.to_string(),
            ],
            language: yoke::server_catalog::get(id).unwrap().languages[0].into(),
            trust: true,
        },
        configuration: lsp::HostConfiguration {
            passive_profile: Some(lsp::PassiveProfile::ContractFixture),
            ..Default::default()
        },
    }
}
fn engine(root: &Path, log: &Path, registered: bool) -> Engine {
    let mut e = Engine::open(root).unwrap();
    e.allow_writes = true;
    e.register_configured_server(configured("rust", log, "legacy", 0))
        .unwrap();
    if registered {
        for id in ["rust", "typescript", "angular"] {
            e.register_backend(id, configured(id, log, id, 0)).unwrap();
        }
    }
    e
}
fn profile(v: Value) -> HostProfile {
    serde_json::from_value(v).unwrap()
}
fn open(root: &Path, log: &Path, p: HostProfile, id: &str) -> Session {
    Session::open(engine(root, log, true), p, Some(id), &d()).unwrap()
}
fn hover(s: &mut Session, path: &str) -> Value {
    let value = s
        .engine
        .invoke(
            "ci_find",
            json!({"languageServer":{"path":path,"offset":0,"operation":"hover"}}),
        )
        .unwrap();
    assert_eq!(value["status"], "ok", "{value}");
    value["data"].clone()
}
fn actor(v: &Value) -> Value {
    serde_json::from_str::<Value>(v["response"]["contents"]["value"].as_str().unwrap()).unwrap()["actor"].clone()
}
fn rows(log: &Path) -> Vec<Value> {
    fs::read_to_string(log)
        .unwrap_or_default()
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect()
}

#[test]
fn current_backend_names_need_host_registration_before_any_process_starts() {
    let r = tempfile::tempdir().unwrap();
    let root = root(r.path(), "source");
    let log = r.path().join("never-started.log");
    for backend in CURRENT_BACKENDS {
        let result = Session::open(
            engine(&root, &log, false),
            profile(json!({"languageServers":{"global":[backend]}})),
            Some("unregistered-current"),
            &d(),
        );
        assert!(
            result
                .err()
                .unwrap()
                .to_string()
                .contains("language_server_not_host_registered")
        );
        assert!(!log.exists());
    }
    assert_eq!(fs::read(root.join("main.rs")).unwrap(), RUST.as_bytes());
}

#[test]
fn current_backends_route_owned_unicode_source_with_explicit_graphql_capture_and_language_ids() {
    let r = tempfile::tempdir().unwrap();
    let root = root(r.path(), "source");
    let log = r.path().join("current-backends.log");
    let vb = "' owned \u{1f4a1} e\u{301}\r\nModule Owned\r\nEnd Module\r\n";
    let graphql = "# owned \u{1f4a1} e\u{301}\r\ntype Query { visible: String }\r\n";
    fs::write(root.join("Main.vb"), vb).unwrap();
    fs::write(root.join("schema.graphql"), graphql).unwrap();
    fs::write(root.join("fragment.gql"), graphql).unwrap();
    let mut e = engine(&root, &log, false);
    for suffix in ["graphql", "gql"] {
        e.capture_options
            .language_overrides
            .insert(suffix.into(), "graphql".into());
    }
    for backend in CURRENT_BACKENDS {
        let mut server = configured(backend, &log, backend, 0);
        server
            .configuration
            .language_ids
            .insert("vbnet".into(), "vb".into());
        e.register_backend(backend, server).unwrap();
    }
    let mut session = Session::open(
        e,
        profile(json!({"languageServers":{"global":CURRENT_BACKENDS}})),
        Some("current-backends"),
        &d(),
    )
    .unwrap();
    for (path, backend, language) in [
        ("Main.vb", "vbnet", "vb"),
        ("entry.ts", "typescript_tsgo", "typescript"),
        ("schema.graphql", "graphql", "graphql"),
        ("fragment.gql", "graphql", "graphql"),
    ] {
        let result = hover(&mut session, path);
        assert_eq!(actor(&result), backend);
        assert_eq!(result["selectedBackendId"], backend);
        assert_eq!(result["backendSelection"]["fallback"], false);
        assert_eq!(result["backendSelection"]["runtimeCertification"], false);
        assert_eq!(result["sourceExecutionAuthorized"], false);
        let observed: Value =
            serde_json::from_str(result["response"]["contents"]["value"].as_str().unwrap())
                .unwrap();
        assert_eq!(observed["path"], path);
        assert_eq!(observed["languageId"], language);
    }
    assert!(rows(&log).iter().all(|row| row["actor"] != "legacy"));
    assert_eq!(fs::read(root.join("Main.vb")).unwrap(), vb.as_bytes());
    assert_eq!(
        fs::read(root.join("schema.graphql")).unwrap(),
        graphql.as_bytes()
    );
    assert_eq!(
        fs::read(root.join("fragment.gql")).unwrap(),
        graphql.as_bytes()
    );
    assert_eq!(fs::read(root.join("main.rs")).unwrap(), RUST.as_bytes());
}
#[test]
fn dormant_registrations_and_absent_null_empty_policy_keep_raw_legacy_bindings_and_source_ids() {
    let r = tempfile::tempdir().unwrap();
    let root = root(r.path(), "source");
    let log = r.path().join("log");
    let p = HostProfile::default();
    let bytes = serde_json::to_vec(&p).unwrap();
    let mut old = Session::open(engine(&root, &log, false), p, Some("legacy"), &d()).unwrap();
    let configuration = old.invoke("configuration", &json!({}), &d()).unwrap();
    let old_reply = hover(&mut old, "main.rs");
    assert_eq!(actor(&old_reply), "legacy");
    assert!(old_reply.get("selectedBackendId").is_none());
    for policy in [
        Value::Null,
        json!({}),
        json!({"global":null,"projects":{"default":{"local":null}}}),
    ] {
        let p = profile(json!({"languageServers":policy}));
        assert_eq!(serde_json::to_vec(&p).unwrap(), bytes);
        let mut s = open(&root, &log, p, "legacy");
        assert_eq!(
            s.invoke("configuration", &json!({}), &d()).unwrap(),
            configuration
        );
        assert_eq!(
            serde_json::to_vec(&hover(&mut s, "main.rs")).unwrap(),
            serde_json::to_vec(&old_reply).unwrap()
        );
        assert!(
            s.engine
                .register_backend(
                    "typescript_vts",
                    configured("typescript_vts", &log, "later", 0)
                )
                .is_err()
        );
    }
    assert!(
        rows(&log)
            .iter()
            .filter(|r| r["method"] == "initialize")
            .all(|r| r["actor"] == "legacy")
    );
    assert_eq!(fs::read(root.join("main.rs")).unwrap(), RUST.as_bytes());
}
#[test]
fn actual_overlapping_extension_order_and_first_fallback_select_only_current_host_backends() {
    for ids in [
        json!(["angular", "typescript"]),
        json!(["typescript", "angular"]),
    ] {
        let r = tempfile::tempdir().unwrap();
        let root = root(r.path(), "source");
        let log = r.path().join("log");
        let mut s = open(
            &root,
            &log,
            profile(json!({"languageServers":{"global":ids}})),
            "order",
        );
        for (path, expected, matched) in [
            ("entry.ts", ids[0].as_str().unwrap(), true),
            ("extra.js", "typescript", true),
            ("view.html", "angular", true),
            ("main.rs", ids[0].as_str().unwrap(), false),
        ] {
            let v = hover(&mut s, path);
            assert_eq!(actor(&v), expected);
            assert_eq!(v["selectedBackendId"], expected);
            assert_eq!(v["backendSelection"]["fallback"], !matched);
            assert_eq!(v["backendSelection"]["runtimeCertification"], false);
            assert_eq!(v["sourceExecutionAuthorized"], false);
        }
        assert!(rows(&log).iter().all(|v| v["actor"] != "legacy"));
        assert_eq!(fs::read(root.join("main.rs")).unwrap(), RUST.as_bytes());
    }
}
#[test]
fn project_local_selection_activation_stale_state_and_registry_changes_bind_independently() {
    let r = tempfile::tempdir().unwrap();
    let primary = root(r.path(), "primary");
    let secondary = root(r.path(), "secondary");
    let log = r.path().join("log");
    let mut p = profile(
        json!({"languageServers":{"global":["angular","typescript"],"projects":{"second":{"project":["angular"],"local":["typescript"]}}}}),
    );
    p.projects.push(HostProject {
        name: "second".into(),
        root: secondary,
        read_only: true,
        query_only: false,
    });
    let mut s = open(&primary, &log, p.clone(), "projects");
    let mut stale = open(&primary, &log, p.clone(), "projects");
    assert_eq!(actor(&hover(&mut s, "entry.ts")), "angular");
    s.invoke("activate", &json!({"project":"second"}), &d())
        .unwrap();
    let v = agent_repl::invoke_facade(
        &mut s,
        "lsp.find_symbol",
        json!({"name_path_pattern":"entry","relative_path":"entry.ts","include_info":true}),
        None,
        &d(),
    )
    .unwrap();
    assert_eq!(actor(&v["items"][0]["info"]), "typescript");
    assert!(stale.invoke("symbols.find", &json!({}), &d()).is_err());
    let mut e = Engine::open(&primary).unwrap();
    e.allow_writes = true;
    e.register_configured_server(configured("rust", &log, "legacy", 0))
        .unwrap();
    for id in ["rust", "typescript", "angular"] {
        e.register_backend(
            id,
            configured(id, &log, if id == "angular" { "changed" } else { id }, 0),
        )
        .unwrap();
    }
    assert!(Session::open(e, p, Some("projects"), &d()).is_err());
}
#[test]
fn schema_catalog_registration_and_explicit_runtime_grants_refuse_before_any_provider() {
    let r = tempfile::tempdir().unwrap();
    let root = root(r.path(), "source");
    let log = r.path().join("log");
    for ids in [
        json!([]),
        json!(["rust", "rust"]),
        json!(["unknown"]),
        json!(["../rust"]),
        json!([false]),
        json!("rust"),
        json!(vec!["rust"; 33]),
    ] {
        if let Ok(p) =
            serde_json::from_value::<HostProfile>(json!({"languageServers":{"global":ids}}))
        {
            assert!(Session::open(engine(&root, &log, true), p, Some("bad"), &d()).is_err());
        }
    }
    let p = profile(json!({"languageServers":{"global":["rust"]}}));
    assert!(Session::open(engine(&root, &log, false), p, Some("not-registered"), &d()).is_err());
    let mut e = Engine::open(&root).unwrap();
    let mut denied = configured("rust", &log, "denied", 0);
    denied.server.trust = false;
    assert!(e.register_backend("rust", denied).is_err());
    let mut relative = configured("rust", &log, "relative", 0);
    relative.server.command = "node".into();
    assert!(e.register_backend("rust", relative).is_err());
    e.register_backend("rust", configured("rust", &log, "accepted", 0))
        .unwrap();
    assert!(
        e.register_backend("rust", configured("rust", &log, "replacement", 0))
            .is_err()
    );
    for v in [
        json!({"language":"rust","backendId":"rust","command":node()}),
        json!({"backendId":"rust","command":node(),"trust":true}),
    ] {
        assert!(serde_json::from_value::<yoke::server_selection::HostServerSpec>(v).is_err());
    }
    let spec: yoke::server_selection::HostServerSpec =
        serde_json::from_value(json!({"backendId":"typescript","command":node()})).unwrap();
    assert!(spec.register(&mut e, false).is_err());
    assert!(!log.exists());
    let mut oversized = configured("typescript", &log, "oversized", 0);
    oversized.server.args.push("x".repeat(128001));
    assert!(e.register_backend("typescript", oversized).is_err());
    assert!(
        Session::open(
            e,
            profile(json!({"languageServers":{"global":["typescript"]}})),
            Some("no-partial-grant"),
            &d()
        )
        .is_err()
    );
    let mut e = Engine::open(&root).unwrap();
    let mut missing_profile = configured("rust", &log, "no-passive-grant", 0);
    missing_profile.configuration.passive_profile = None;
    e.register_backend("rust", missing_profile).unwrap();
    let mut s = Session::open(
        e,
        profile(json!({"languageServers":{"global":["rust"]}})),
        Some("no-source-execution"),
        &d(),
    )
    .unwrap();
    let result = s
        .engine
        .invoke(
            "ci_find",
            json!({"languageServer":{"path":"main.rs","offset":0,"operation":"hover"}}),
        )
        .unwrap();
    assert_eq!(result["status"], "error");
    assert!(result.to_string().contains("Passive provider unavailable"));
    assert!(!log.exists());
}
#[test]
fn direct_transport_and_refactor_registry_follow_selection_and_declared_ids_bind_profile_hashes() {
    let r = tempfile::tempdir().unwrap();
    let root = root(r.path(), "source");
    let log = r.path().join("log");
    let mut s = open(
        &root,
        &log,
        profile(json!({"languageServers":{"global":["angular","typescript"]}})),
        "direct",
    );
    let legacy = lsp::ConfiguredServer {
        server: s.engine.servers["rust"].clone(),
        configuration: s.engine.server_configurations["rust"].clone(),
    };
    let (snapshot, graph, _, _) = s.engine.index(&d()).unwrap();
    let v = lsp::run_at_configured(
        &mut s.engine,
        &legacy,
        &snapshot,
        &graph,
        lsp::SemanticRequest {
            path: "entry.ts",
            offset: 0,
            operation: "hover",
            new_name: None,
        },
        &json!({}),
        &d(),
    )
    .unwrap();
    assert_eq!(actor(&v), "angular");
    let inspected =
        yoke::semantic_refactor::invoke(&mut s.engine, "listInspections", &json!({}), &d())
            .unwrap();
    assert_eq!(inspected["items"].as_array().unwrap().len(), 2);
    let mut hashes = vec![];
    for id in ["typescript", "typescript_vts"] {
        let mut e = Engine::open(&root).unwrap();
        e.register_backend(id, configured(id, &log, "identical-program", 0))
            .unwrap();
        let p = profile(json!({"languageServers":{"global":[id]}}));
        let mut s = Session::open(e, p, Some(id), &d()).unwrap();
        let v = hover(&mut s, "entry.ts");
        hashes.push(v["serverProfileHash"].clone());
        assert_eq!(v["selectedBackendId"], id);
    }
    assert_ne!(hashes[0], hashes[1]);
}
#[test]
fn owned_move_uses_registered_backend_and_mixed_directory_backends_refuse_before_startup() {
    let r = tempfile::tempdir().unwrap();
    let root = root(r.path(), "source");
    let log = r.path().join("own.log");
    fs::create_dir(root.join("src")).unwrap();
    let before = "pub fn helper() -> u8 { 7 }\n";
    fs::write(root.join("src/source.rs"), before).unwrap();
    fs::write(
        root.join("src/destination.rs"),
        "pub mod bucket {\npub fn sentinel() {}\n}\n",
    )
    .unwrap();
    let mut own = configured("rust", &log, "unused", 0);
    own.server.args = vec![
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/own-refactor-intent-lsp.mjs")
            .display()
            .to_string(),
        log.display().to_string(),
        "valid".into(),
    ];
    own.configuration.refactor_intent_profile = Some(lsp::RefactorIntentProfile::OwnV1);
    let mut e = Engine::open(&root).unwrap();
    e.allow_writes = true;
    e.register_backend("rust", own).unwrap();
    let mut s = Session::open(
        e,
        profile(json!({"languageServers":{"global":["rust"]}})),
        Some("own"),
        &d(),
    )
    .unwrap();
    let v=agent_repl::invoke_facade(&mut s,"jb.move",json!({"relative_path":"src/source.rs","name_path":"helper","target_relative_path":"src/destination.rs"}),None,&d()).unwrap();
    assert!(v["plan"].is_object());
    assert!(
        rows(&log)
            .iter()
            .any(|r| r["method"] == "yoke/refactorIntent")
    );
    assert_eq!(
        fs::read_to_string(root.join("src/source.rs")).unwrap(),
        before
    );
    let mixed_log = r.path().join("mixed.log");
    fs::create_dir(root.join("mixed")).unwrap();
    fs::write(root.join("mixed/a.ts"), "export function a() {}\n").unwrap();
    fs::write(root.join("mixed/b.js"), "export function b() {}\n").unwrap();
    let mut s = open(
        &root,
        &mixed_log,
        profile(json!({"languageServers":{"global":["angular","typescript"]}})),
        "mixed",
    );
    let error = yoke::semantic_refactor::invoke(
        &mut s.engine,
        "move",
        &json!({"relativePath":"mixed","targetRelativePath":"moved"}),
        &d(),
    )
    .unwrap_err();
    assert!(
        error.to_string().contains("multiple selected backends"),
        "{error}"
    );
    assert!(!mixed_log.exists());
    assert!(!root.join("moved").exists());
}
#[test]
fn lifecycle_probes_only_selected_assigned_backends_and_host_cancellation_stays_an_error() {
    let r = tempfile::tempdir().unwrap();
    let root = root(r.path(), "source");
    let log = r.path().join("log");
    let mut s = open(
        &root,
        &log,
        profile(json!({"languageServers":{"global":["typescript","angular"]}})),
        "lifecycle",
    );
    let status = lsp::restart_registered(&mut s.engine, &d()).unwrap();
    assert_eq!(status["providers"].as_array().unwrap().len(), 2);
    assert_eq!(status["providers"][0]["backendId"], "typescript");
    assert!(rows(&log).iter().all(|r| r["actor"] != "legacy"));
    let cancel_log = r.path().join("cancel.log");
    let mut e = Engine::open(&root).unwrap();
    e.register_backend("rust", configured("rust", &cancel_log, "slow", 10000))
        .unwrap();
    let mut s = Session::open(
        e,
        profile(json!({"languageServers":{"global":["rust"]}})),
        Some("cancel"),
        &d(),
    )
    .unwrap();
    let cancelled = Arc::new(AtomicBool::new(false));
    let flag = Arc::clone(&cancelled);
    let watched = cancel_log.clone();
    let observer = std::thread::spawn(move || {
        let until = Instant::now() + Duration::from_secs(15);
        while Instant::now() < until {
            if rows(&watched)
                .iter()
                .any(|r| r["method"] == "textDocument/hover")
            {
                flag.store(true, Ordering::SeqCst);
                return true;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        flag.store(true, Ordering::SeqCst);
        false
    });
    let result = Deadline::run_bounded(&d(), cancelled, || {
        agent_repl::invoke_facade(
            &mut s,
            "lsp.find_symbol",
            json!({"name_path_pattern":"visible","relative_path":"main.rs","include_info":true}),
            None,
            &d(),
        )
    });
    assert!(observer.join().unwrap());
    assert!(
        format!("{:#}", result.unwrap_err())
            .to_lowercase()
            .contains("cancel")
    );
}

fn http(
    address: std::net::SocketAddr,
    sid: Option<&str>,
    authorized: bool,
    body: Value,
) -> (u16, std::collections::BTreeMap<String, String>, Value) {
    use std::io::{Read, Write};
    let encoded = serde_json::to_vec(&body).unwrap();
    let auth = if authorized {
        format!("Authorization: Bearer {}\r\n", hex::encode([73; 32]))
    } else {
        String::new()
    };
    let session = sid
        .map(|s| format!("Mcp-Session-Id: {s}\r\nMcp-Protocol-Version: 2025-11-25\r\n"))
        .unwrap_or_default();
    let mut stream = std::net::TcpStream::connect(address).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(15)))
        .unwrap();
    stream.write_all(format!("POST /mcp HTTP/1.1\r\nHost: {address}\r\n{auth}{session}Accept: application/json, text/event-stream\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n",encoded.len()).as_bytes()).unwrap();
    stream.write_all(&encoded).unwrap();
    let mut bytes = vec![];
    loop {
        let mut chunk = [0; 8192];
        let n = stream.read(&mut chunk).unwrap();
        assert_ne!(n, 0);
        bytes.extend_from_slice(&chunk[..n]);
        if let Some(end) = bytes.windows(4).position(|p| p == b"\r\n\r\n") {
            let header = std::str::from_utf8(&bytes[..end]).unwrap();
            let length: usize = header
                .split("\r\n")
                .filter_map(|s| s.split_once(':'))
                .find(|(key, _)| key.eq_ignore_ascii_case("content-length"))
                .unwrap()
                .1
                .trim()
                .parse()
                .unwrap();
            if bytes.len() >= end + 4 + length {
                let status = header.split_whitespace().nth(1).unwrap().parse().unwrap();
                let headers = header
                    .split("\r\n")
                    .skip(1)
                    .filter_map(|s| s.split_once(':'))
                    .map(|(key, value)| (key.to_ascii_lowercase(), value.trim().to_owned()))
                    .collect();
                let value = if length == 0 {
                    Value::Null
                } else {
                    serde_json::from_slice(&bytes[end + 4..end + 4 + length]).unwrap()
                };
                return (status, headers, value);
            }
        }
    }
}
#[test]
fn real_http_principal_host_registrations_dispatch_selection_and_revocation_precedes_provider() {
    use yoke::{
        mcp_http::{self, Config, Principal, Target},
        protocol::Profile,
    };
    let r = tempfile::tempdir().unwrap();
    let root = root(r.path(), "source");
    let log = r.path().join("log");
    let host = profile(json!({"languageServers":{"global":["angular","typescript"]}}));
    let mut target = Target::new(&root, Profile::Session, host);
    for id in ["angular", "typescript"] {
        target.host_servers.push(serde_json::from_value(json!({"backendId":id,"command":node(),"args":[Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/server-selection-lsp.mjs"),log,id],"configuration":{"passiveProfile":"contract_fixture"}})).unwrap());
    }
    let principal = Principal::new(
        "owned-selector",
        zeroize::Zeroizing::new(vec![73; 32]),
        target,
    )
    .unwrap();
    assert!(!log.exists());
    let server = mcp_http::serve(Config::default(), vec![principal]).unwrap();
    let address = server.local_addr();
    let init = json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"owned-server-selection","version":"1"}}});
    assert_eq!(http(address, None, false, init.clone()).0, 401);
    let (status, headers, _) = http(address, None, true, init);
    assert_eq!(status, 200);
    let sid = &headers["mcp-session-id"];
    assert_eq!(
        http(
            address,
            Some(sid),
            true,
            json!({"jsonrpc":"2.0","method":"notifications/initialized"})
        )
        .0,
        202
    );
    assert!(!log.exists());
    let request = json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"ci_find","arguments":{"languageServer":{"path":"entry.ts","offset":0,"operation":"hover"}}}});
    let (status, _, reply) = http(address, Some(sid), true, request.clone());
    assert_eq!(status, 200);
    assert_eq!(reply["result"]["isError"], false, "{reply}");
    let result: Value =
        serde_json::from_str(reply["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(result["status"], "ok", "{result}");
    assert_eq!(actor(&result["data"]), "angular");
    assert_eq!(result["data"]["selectedBackendId"], "angular");
    let before = rows(&log).len();
    assert!(server.revoke("owned-selector").unwrap());
    assert_eq!(http(address, Some(sid), true, request).0, 401);
    assert_eq!(rows(&log).len(), before);
    server.shutdown().unwrap();
    assert!(std::net::TcpStream::connect(address).is_err());
}
