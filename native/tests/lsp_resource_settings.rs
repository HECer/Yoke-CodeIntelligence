use serde_json::{Value, json};
use std::{fs, path::Path, process::Command, sync::OnceLock};
use yoke::{
    Engine,
    capture::Deadline,
    lsp::{self, ConfiguredServer, SemanticRequest},
    project_session::{HostProfile, HostProject, Session},
};

const ALT: &str = "unicode/e\u{301}-\u{1f9ed}.rs";
const SOURCE: &str = "// owned \u{1f30d}\r\nfn visible() {}\r\n";
fn d() -> Deadline {
    Deadline::new(30_000)
}
fn source(root: &Path) {
    fs::create_dir_all(root.join("unicode")).unwrap();
    fs::write(root.join("main.rs"), SOURCE).unwrap();
    fs::write(root.join(ALT), "fn alternate() {}\n").unwrap();
    fs::write(root.join("entry.ts"), "export function entry() {}\n").unwrap();
    fs::write(root.join(".env"), "OWNED_PROTECTED_CANARY=unavailable\n").unwrap();
}
fn node() -> String {
    static NODE: OnceLock<String> = OnceLock::new();
    NODE.get_or_init(|| {
        let r = Command::new("node")
            .args(["-p", "process.execPath"])
            .output()
            .unwrap();
        assert!(r.status.success());
        String::from_utf8(r.stdout).unwrap().trim().into()
    })
    .clone()
}
fn provider(log: &Path, resources: Option<Value>, mode: &str) -> ConfiguredServer {
    let mut configuration = json!({
        "passiveProfile":"contract_fixture",
        "settings":{"base":"original","nested":{"base":true,"selected":"original"},"drop":"original"}
    });
    if let Some(resources) = resources {
        configuration["resourceSettings"] = resources;
    }
    ConfiguredServer {
        server: lsp::Server {
            command: node(),
            args: vec![
                Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("tests/fixtures/resource-settings-lsp.mjs")
                    .display()
                    .to_string(),
                log.display().to_string(),
                mode.into(),
            ],
            language: "rust".into(),
            trust: true,
        },
        configuration: serde_json::from_value(configuration).unwrap(),
    }
}
fn run(engine: &mut Engine, server: &ConfiguredServer) -> anyhow::Result<Value> {
    let (snapshot, graph, _, _) = engine.index(&d())?;
    lsp::run_at_configured(
        engine,
        server,
        &snapshot,
        &graph,
        SemanticRequest {
            path: "main.rs",
            offset: 0,
            operation: "hover",
            new_name: None,
        },
        &json!({}),
        &d(),
    )
}
fn body(report: &Value) -> Value {
    serde_json::from_str(report["response"]["contents"]["value"].as_str().unwrap()).unwrap()
}
fn resources(tag: &str) -> Value {
    json!({"main.rs":{"tag":"main"},(ALT):{"tag":tag,"nested":{"selected":tag},"drop":null}})
}

#[test]
fn actual_scoped_callbacks_preserve_order_sections_unicode_and_immutable_source() {
    let r = tempfile::tempdir().unwrap();
    let root = r.path().join("source");
    source(&root);
    let log = r.path().join("log");
    let server = provider(&log, Some(resources("alternate")), "normal");
    let mut engine = Engine::open(&root).unwrap();
    let (snapshot, graph, _, _) = engine.index(&d()).unwrap();
    fs::write(root.join(ALT), "fn changed_live_disk() {}\n").unwrap();
    let report = lsp::run_at_configured(
        &mut engine,
        &server,
        &snapshot,
        &graph,
        SemanticRequest {
            path: "main.rs",
            offset: 0,
            operation: "hover",
            new_name: None,
        },
        &json!({}),
        &d(),
    )
    .unwrap();
    let b = body(&report);
    let p = b["pulled"].as_array().unwrap();
    assert_eq!(p.len(), 10);
    assert_eq!(p[0], b["settings"]);
    assert_eq!(p[1]["tag"], "main");
    assert_eq!(p[2]["tag"], "alternate");
    assert_eq!(p[2]["nested"], json!({"selected":"alternate"}));
    assert!(p[2]["drop"].is_null());
    assert_eq!(p[3], "alternate");
    assert_eq!(p[4], "original");
    for index in [5, 6, 7, 9] {
        assert!(p[index].is_null(), "{b}");
    }
    assert_eq!(p[8], b["settings"]);
    for index in [1, 2] {
        assert_eq!(p[index]["cargo"]["buildScripts"]["enable"], false);
        assert_eq!(p[index]["rust-analyzer"]["procMacro"]["enable"], false);
    }
    assert_eq!(b["openedMain"], SOURCE);
    assert_eq!(b["openedAlt"], "fn alternate() {}\n");
    assert_eq!(report["snapshotId"], snapshot.id);
    assert_eq!(report["sourceExecutionAuthorized"], false);
    assert_eq!(fs::read(root.join("main.rs")).unwrap(), SOURCE.as_bytes());
    assert_eq!(
        fs::read_to_string(root.join(ALT)).unwrap(),
        "fn changed_live_disk() {}\n"
    );
}

#[test]
fn absent_and_null_resources_preserve_configuration_and_profile_identity() {
    let r = tempfile::tempdir().unwrap();
    let root = r.path().join("source");
    source(&root);
    let log = r.path().join("log");
    let absent = provider(&log, None, "normal");
    let null = provider(&log, Some(Value::Null), "normal");
    assert_eq!(
        serde_json::to_vec(&absent.configuration).unwrap(),
        serde_json::to_vec(&null.configuration).unwrap()
    );
    let mut engine = Engine::open(&root).unwrap();
    let a = run(&mut engine, &absent).unwrap();
    let b = run(&mut engine, &null).unwrap();
    assert_eq!(a["serverProfileHash"], b["serverProfileHash"]);
    assert_eq!(body(&a), body(&b));
}

#[test]
fn resource_membership_paths_and_budgets_refuse_before_provider_start() {
    for value in [
        json!({"../escape":{"tag":"bad"}}),
        json!({"missing.rs":{"tag":"bad"}}),
        json!({".env":{"tag":"bad"}}),
        json!({"main.rs":{"large":"x".repeat(64000)}}),
        Value::Object((0..101).map(|n| (format!("x{n}.rs"), json!({}))).collect()),
    ] {
        let r = tempfile::tempdir().unwrap();
        let root = r.path().join("source");
        source(&root);
        let log = r.path().join("log");
        let server = provider(&log, Some(value.clone()), "normal");
        let mut engine = Engine::open(&root).unwrap();
        assert!(run(&mut engine, &server).is_err(), "{value}");
        assert!(!log.exists(), "Provider started before resource admission");
        assert_eq!(fs::read(root.join("main.rs")).unwrap(), SOURCE.as_bytes());
    }
}

#[test]
fn scoped_settings_cannot_enable_passive_rust_execution_or_tool_redirects() {
    for settings in [
        json!({"cargo":{"buildScripts":{"enable":true}}}),
        json!({"rust-analyzer":{"procMacro":{"enable":true}}}),
        json!({"cargo":{"extraEnv":{"RUSTC_WRAPPER":"owned-unavailable"}}}),
        json!({"workspace":{"discoverConfig":{"command":["owned-unavailable"]}}}),
        json!({"rust-analyzer":null,"cargo":{"extraEnv":{"RUSTC_WRAPPER":"owned-unavailable"}}}),
        json!({"rust-analyzer":{},"cargo":{"extraEnv":{"rustc_wrapper":"owned-unavailable"}}}),
        json!({"rust-analyzer":{"cargo":{"extraEnv":{"RuStC_WrApPeR":"owned-unavailable"}}}}),
        json!({"rust-analyzer":{},"procMacro":{"enable":true}}),
    ] {
        let r = tempfile::tempdir().unwrap();
        let root = r.path().join("source");
        source(&root);
        let log = r.path().join("log");
        let server = provider(&log, Some(json!({"main.rs":settings})), "normal");
        let mut engine = Engine::open(&root).unwrap();
        let error = run(&mut engine, &server).unwrap_err();
        assert!(error.to_string().contains("require"), "{error}");
        assert!(!log.exists(), "Passive settings reached provider");
        assert_eq!(fs::read(root.join("main.rs")).unwrap(), SOURCE.as_bytes());
    }
}

#[test]
fn malformed_resource_callback_is_an_error_without_source_changes() {
    for mode in ["invalid-scope", "oversized-scope", "invalid-item"] {
        let r = tempfile::tempdir().unwrap();
        let root = r.path().join("source");
        source(&root);
        let log = r.path().join("log");
        let server = provider(&log, Some(resources("selected")), mode);
        let mut engine = Engine::open(&root).unwrap();
        assert!(run(&mut engine, &server).is_err());
        assert!(log.exists());
        assert_eq!(fs::read(root.join("main.rs")).unwrap(), SOURCE.as_bytes());
    }
}

#[test]
fn project_overlays_start_from_original_registration_and_stale_bindings_refuse() {
    let r = tempfile::tempdir().unwrap();
    let a = r.path().join("primary");
    let b = r.path().join("secondary");
    source(&a);
    source(&b);
    let log = r.path().join("log");
    let mut engine = Engine::open(&a).unwrap();
    engine
        .register_backend(
            "rust",
            provider(&log, Some(json!({(ALT):{"registered":true}})), "normal"),
        )
        .unwrap();
    let mut profile:HostProfile=serde_json::from_value(json!({
        "languageServers":{"global":["rust"]},"trustedServerSettingsProjects":["default","second"],
        "languageServerSettings":{"projects":{"default":{"project":{"rust":{"resourceSettings":{(ALT):{"tag":"primary"}}}}},"second":{"project":{"rust":{}}}}}
    })).unwrap();
    profile.projects.push(HostProject {
        name: "second".into(),
        root: b,
        read_only: true,
        query_only: false,
    });
    let mut session = Session::open(engine, profile.clone(), Some("resource-bound"), &d()).unwrap();
    let hover = |session: &mut Session| {
        let r = session
            .active_engine()
            .unwrap()
            .invoke(
                "ci_find",
                json!({"languageServer":{"path":"main.rs","offset":0,"operation":"hover"}}),
            )
            .unwrap();
        assert_eq!(r["status"], "ok", "{r}");
        body(&r["data"])["pulled"][2].clone()
    };
    let p = hover(&mut session);
    assert_eq!(p["tag"], "primary");
    assert_eq!(p["registered"], true);
    session
        .invoke("activate", &json!({"project":"second"}), &d())
        .unwrap();
    let q = hover(&mut session);
    assert!(q.get("tag").is_none());
    assert_eq!(q["registered"], true);
    profile.language_server_settings = serde_json::from_value(
        json!({"global":{"rust":{"resourceSettings":{(ALT):{"tag":"changed"}}}}}),
    )
    .unwrap();
    let mut another = Engine::open(&a).unwrap();
    another
        .register_backend(
            "rust",
            provider(&log, Some(json!({(ALT):{"registered":true}})), "normal"),
        )
        .unwrap();
    let error = Session::open(another, profile, Some("resource-bound"), &d())
        .err()
        .expect("changed resource policy must refuse the named session");
    assert!(
        error.to_string().contains("Session host binding changed"),
        "{error}"
    );
    assert_eq!(fs::read(a.join("main.rs")).unwrap(), SOURCE.as_bytes());
}

#[test]
fn host_registration_projects_resources_and_preserves_absent_null_defaults() {
    let r = tempfile::tempdir().unwrap();
    let root = r.path().join("source");
    source(&root);
    let log = r.path().join("log");
    let selected = provider(&log, None, "normal");
    let mut raw = json!({"backendId":"rust","command":selected.server.command,
        "args":selected.server.args,"trust":true,"passiveProfile":"contract_fixture",
        "settings":selected.configuration.settings});
    let absent: yoke::server_catalog::HostRegistration =
        serde_json::from_value(raw.clone()).unwrap();
    raw["resourceSettings"] = Value::Null;
    let null: yoke::server_catalog::HostRegistration = serde_json::from_value(raw.clone()).unwrap();
    assert_eq!(
        serde_json::to_vec(&absent.configured().unwrap().configuration).unwrap(),
        serde_json::to_vec(&null.configured().unwrap().configuration).unwrap()
    );
    raw["resourceSettings"] = resources("host-registration");
    let registration: yoke::server_catalog::HostRegistration = serde_json::from_value(raw).unwrap();
    let configured = registration.configured().unwrap();
    assert!(!log.exists());
    let mut engine = Engine::open(&root).unwrap();
    assert_eq!(
        body(&run(&mut engine, &configured).unwrap())["pulled"][2]["tag"],
        "host-registration"
    );
}

#[test]
fn pinned_yaml_resource_projection_requires_host_trust_and_retains_source_pin() {
    use yoke::session_config::{self, Source, Target};
    let bytes=b"ls_specific_settings:\n  rust:\n    resourceSettings:\n      main.rs:\n        tag: pinned-resource\n";
    let hash = yoke::model::hash(bytes);
    let target = Target::Global;
    let selection = [Source {
        target: &target,
        expected_sha256: &hash,
        bytes,
    }];
    assert!(session_config::project(&HostProfile::default(), &selection, &d()).is_err());
    let base: HostProfile = serde_json::from_value(json!({
        "languageServers":{"global":["rust"]},"trustedServerSettingsProjects":["default"]
    }))
    .unwrap();
    let projection = session_config::project(&base, &selection, &d()).unwrap();
    assert_eq!(projection.plan()["runtimeTrustFromYaml"], false);
    assert_eq!(projection.plan()["storeOpened"], false);
    let r = tempfile::tempdir().unwrap();
    let root = r.path().join("source");
    source(&root);
    let log = r.path().join("log");
    let mut engine = Engine::open(&root).unwrap();
    engine
        .register_backend("rust", provider(&log, None, "normal"))
        .unwrap();
    let mut session = Session::open(engine, projection.profile().clone(), None, &d()).unwrap();
    let report = session
        .active_engine()
        .unwrap()
        .invoke(
            "ci_find",
            json!({"languageServer":{"path":"main.rs","offset":0,"operation":"hover"}}),
        )
        .unwrap();
    assert_eq!(report["status"], "ok", "{report}");
    assert_eq!(body(&report["data"])["pulled"][1]["tag"], "pinned-resource");
    assert!(
        session_config::project(
            &base,
            &[Source {
                target: &target,
                expected_sha256: &"0".repeat(64),
                bytes
            }],
            &d()
        )
        .is_err()
    );
    assert_eq!(fs::read(root.join("main.rs")).unwrap(), SOURCE.as_bytes());
}

#[test]
fn project_resource_trust_and_backend_isolation_are_independent() {
    let r = tempfile::tempdir().unwrap();
    let root = r.path().join("source");
    source(&root);
    let log = r.path().join("log");
    let make_engine = || {
        let mut engine = Engine::open(&root).unwrap();
        engine
            .register_backend("rust", provider(&log, None, "normal"))
            .unwrap();
        let mut ts = provider(
            &log,
            Some(json!({"entry.ts":{"tag":"typescript"}})),
            "normal",
        );
        ts.server.language = "typescript".into();
        engine.register_backend("typescript", ts).unwrap();
        engine
    };
    let mut host = json!({"languageServers":{"global":["rust","typescript"]},
        "languageServerSettings":{"global":{"rust":{"resourceSettings":resources("rust-only")}}}});
    let untrusted: HostProfile = serde_json::from_value(host.clone()).unwrap();
    let error = Session::open(make_engine(), untrusted, None, &d())
        .err()
        .unwrap();
    assert!(error.to_string().contains("host_trust"), "{error}");
    assert!(!log.exists());
    host["trustedServerSettingsProjects"] = json!(["default"]);
    let mut session = Session::open(
        make_engine(),
        serde_json::from_value(host).unwrap(),
        None,
        &d(),
    )
    .unwrap();
    let invoke = |session: &mut Session, path: &str| {
        let report = session
            .active_engine()
            .unwrap()
            .invoke(
                "ci_find",
                json!({"languageServer":{"path":path,"offset":0,"operation":"hover"}}),
            )
            .unwrap();
        assert_eq!(report["status"], "ok", "{report}");
        body(&report["data"])
    };
    assert_eq!(
        invoke(&mut session, "main.rs")["pulled"][2]["tag"],
        "rust-only"
    );
    let ts = invoke(&mut session, "entry.ts");
    assert_eq!(ts["pulled"][1]["tag"], "typescript");
    assert!(ts["pulled"][2].get("tag").is_none(), "{ts}");
}

#[test]
fn authenticated_http_resources_preserve_principal_and_revocation_boundaries() {
    use std::{
        io::{Read, Write},
        net::{SocketAddr, TcpStream},
        time::Duration,
    };
    use yoke::{
        mcp_http::{self, Config, Principal, Target},
        protocol::Profile,
    };
    fn request(
        address: SocketAddr,
        sid: Option<&str>,
        authorized: bool,
        value: Value,
    ) -> (u16, String, Value) {
        let body = serde_json::to_vec(&value).unwrap();
        let auth = if authorized {
            format!("Authorization: Bearer {}\r\n", hex::encode([84; 32]))
        } else {
            String::new()
        };
        let session = sid
            .map(|s| format!("Mcp-Session-Id: {s}\r\nMcp-Protocol-Version: 2025-11-25\r\n"))
            .unwrap_or_default();
        let mut stream = TcpStream::connect(address).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(15)))
            .unwrap();
        stream.write_all(format!("POST /mcp HTTP/1.1\r\nHost: {address}\r\n{auth}{session}Accept: application/json, text/event-stream\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n",body.len()).as_bytes()).unwrap();
        stream.write_all(&body).unwrap();
        let mut wire = Vec::new();
        stream.read_to_end(&mut wire).unwrap();
        let split = wire.windows(4).position(|b| b == b"\r\n\r\n").unwrap();
        let headers = std::str::from_utf8(&wire[..split]).unwrap();
        let status = headers.split_whitespace().nth(1).unwrap().parse().unwrap();
        let sid = headers
            .lines()
            .find_map(|line| {
                line.split_once(':')
                    .filter(|(key, _)| key.eq_ignore_ascii_case("mcp-session-id"))
                    .map(|(_, value)| value.trim().to_owned())
            })
            .unwrap_or_default();
        (
            status,
            sid,
            serde_json::from_slice(&wire[split + 4..]).unwrap_or(Value::Null),
        )
    }
    let r = tempfile::tempdir().unwrap();
    let root = r.path().join("source");
    source(&root);
    let log = r.path().join("log");
    let profile:HostProfile=serde_json::from_value(json!({"languageServers":{"global":["rust"]},
        "trustedServerSettingsProjects":["default"],"languageServerSettings":{"global":{"rust":{"resourceSettings":resources("http-resource")}}}})).unwrap();
    let mut target = Target::new(&root, Profile::Session, profile);
    let s = provider(&log, None, "normal");
    target.host_servers.push(serde_json::from_value(json!({"backendId":"rust","command":s.server.command,"args":s.server.args,"configuration":s.configuration})).unwrap());
    let principal = Principal::new(
        "resource-settings",
        zeroize::Zeroizing::new(vec![84; 32]),
        target,
    )
    .unwrap();
    assert!(!log.exists());
    let server = mcp_http::serve(Config::default(), vec![principal]).unwrap();
    let address = server.local_addr();
    let init = json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"owned-resources","version":"1"}}});
    assert_eq!(request(address, None, false, init.clone()).0, 401);
    let (status, sid, _) = request(address, None, true, init);
    assert_eq!(status, 200);
    assert!(!sid.is_empty());
    assert!(!log.exists());
    let call = json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"ci_find","arguments":{"languageServer":{"path":"main.rs","offset":0,"operation":"hover"}}}});
    let (status, _, reply) = request(address, Some(&sid), true, call.clone());
    assert_eq!(status, 200);
    let report: Value =
        serde_json::from_str(reply["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(report["status"], "ok", "{report}");
    assert_eq!(body(&report["data"])["pulled"][2]["tag"], "http-resource");
    let before = fs::read(&log).unwrap();
    assert!(server.revoke("resource-settings").unwrap());
    assert_eq!(request(address, Some(&sid), true, call).0, 401);
    assert_eq!(fs::read(&log).unwrap(), before);
    server.shutdown().unwrap();
    assert!(TcpStream::connect(address).is_err());
    assert_eq!(fs::read(root.join("main.rs")).unwrap(), SOURCE.as_bytes());
}
