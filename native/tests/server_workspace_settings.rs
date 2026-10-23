use serde_json::{Value, json};
use std::{fs, path::Path, process::Command, sync::OnceLock};
use yoke::{
    Engine,
    capture::Deadline,
    lsp,
    project_session::{HostProfile, HostProject, Session},
};
const SOURCE: &str = "// owned \u{1f4a1} e\u{301}\r\nfn visible() {}\r\n";
fn d() -> Deadline {
    Deadline::new(30_000)
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
fn source(root: &Path) {
    fs::create_dir_all(root).unwrap();
    fs::write(root.join("main.rs"), SOURCE).unwrap();
    fs::write(root.join("entry.ts"), "export function entry() {}\n").unwrap();
}
fn provider(log: &Path, id: &str) -> lsp::ConfiguredServer {
    lsp::ConfiguredServer {
        server: lsp::Server {
            command: node(),
            args: vec![
                Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("tests/fixtures/server-workspace-settings-lsp.mjs")
                    .display()
                    .to_string(),
                log.display().to_string(),
            ],
            language: yoke::server_catalog::get(id).unwrap().languages[0].into(),
            trust: true,
        },
        configuration: lsp::HostConfiguration {
            passive_profile: Some(lsp::PassiveProfile::ContractFixture),
            initialization_options: json!({"originalInit":true}),
            settings: json!({"keep":"base", "nested":{"base":1}, "replace":"original"}),
            ..Default::default()
        },
    }
}
fn engine(root: &Path, log: &Path) -> Engine {
    let mut e = Engine::open(root).unwrap();
    for id in ["rust", "typescript"] {
        e.register_backend(id, provider(log, id)).unwrap();
    }
    e
}
fn host(settings: Value) -> HostProfile {
    serde_json::from_value(json!({"languageServers":{"global":["rust","typescript"]}, "trustedServerSettingsProjects":["default"], "languageServerSettings":settings})).unwrap()
}
fn hover(s: &mut Session, path: &str) -> (Value, Value) {
    let r = s
        .active_engine()
        .unwrap()
        .invoke(
            "ci_find",
            json!({"languageServer":{"path":path,"offset":0,"operation":"hover"}}),
        )
        .unwrap();
    assert_eq!(r["status"], "ok", "{r}");
    let value =
        serde_json::from_str(r["data"]["response"]["contents"]["value"].as_str().unwrap()).unwrap();
    (r["data"].clone(), value)
}
fn records(root: &Path) -> Vec<(String, String, String)> {
    let db = rusqlite::Connection::open_with_flags(
        root.join(".yoke/index.sqlite"),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .unwrap();
    db.prepare("SELECT kind,id,value FROM records ORDER BY kind,id")
        .unwrap()
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
        .unwrap()
        .map(Result::unwrap)
        .collect()
}

#[test]
fn actual_workspace_push_pull_and_staged_actions_use_shallow_original_settings() {
    let r = tempfile::tempdir().unwrap();
    let root = r.path().join("source");
    source(&root);
    let log = r.path().join("log");
    let p = host(
        json!({"global":{"rust":{"settings":{"nested":{"selected":"\u{e9} e\u{301}"},"replace":null,"actionTitle":"selected-action"}}}}),
    );
    let mut s = Session::open(engine(&root, &log), p, Some("workspace"), &d()).unwrap();
    let (reply, body) = hover(&mut s, "main.rs");
    let expected = json!({"keep":"base","nested":{"selected":"\u{e9} e\u{301}"},"replace":null,"actionTitle":"selected-action","cargo":{"buildScripts":{"enable":false}},"procMacro":{"enable":false},"checkOnSave":false,"rust-analyzer":{"cargo":{"buildScripts":{"enable":false}},"procMacro":{"enable":false},"checkOnSave":false}});
    assert_eq!(body["settings"], expected);
    assert_eq!(
        body["pulled"],
        json!([expected,{"selected":"\u{e9} e\u{301}"},null,null,"\u{e9} e\u{301}"])
    );
    assert_eq!(body["initializationOptions"]["originalInit"], true);
    assert_eq!(reply["sourceExecutionAuthorized"], false);
    let action = s
        .engine
        .invoke(
            "ci_find",
            json!({"languageServer":{"path":"main.rs","offset":0,"operation":"codeActions"}}),
        )
        .unwrap();
    assert_eq!(action["status"], "ok", "{action}");
    assert_eq!(action["data"]["items"][0]["title"], "selected-action");
    assert_eq!(action["data"]["items"][0]["usable"], true);
    assert_eq!(fs::read(root.join("main.rs")).unwrap(), SOURCE.as_bytes());
}

#[test]
fn whole_backend_layers_and_projects_never_inherit_another_projects_overlay() {
    let r = tempfile::tempdir().unwrap();
    let a = r.path().join("primary");
    let b = r.path().join("secondary");
    source(&a);
    source(&b);
    let log = r.path().join("log");
    let mut p = host(
        json!({"global":{"rust":{"settings":{"nested":{"global":1}}},"typescript":{"settings":{"tag":"typescript"}}},"projects":{"default":{"project":{"rust":{"settings":{"nested":{"selected":"primary"}}}},"local":{"rust":null}},"second":{"local":{"rust":{}}}}}),
    );
    p.projects.push(HostProject {
        name: "second".into(),
        root: b,
        read_only: true,
        query_only: false,
    });
    p.trusted_server_settings_projects.push("second".into());
    let mut s = Session::open(engine(&a, &log), p, Some("layers"), &d()).unwrap();
    let (first, body) = hover(&mut s, "main.rs");
    assert_eq!(body["settings"]["nested"], json!({"selected":"primary"}));
    assert_eq!(hover(&mut s, "entry.ts").1["settings"]["tag"], "typescript");
    s.invoke("activate", &json!({"project":"second"}), &d())
        .unwrap();
    let (second, body) = hover(&mut s, "main.rs");
    assert_eq!(body["settings"]["nested"], json!({"base":1}));
    assert_ne!(first["serverProfileHash"], second["serverProfileHash"]);
    s.invoke("activate", &json!({"project":"default"}), &d())
        .unwrap();
    assert_eq!(
        hover(&mut s, "main.rs").0["serverProfileHash"],
        first["serverProfileHash"]
    );
    assert_eq!(fs::read(a.join("main.rs")).unwrap(), SOURCE.as_bytes());
}

#[test]
fn absent_null_settings_preserve_named_binding_configuration_and_wire() {
    let r = tempfile::tempdir().unwrap();
    let root = r.path().join("source");
    source(&root);
    let log = r.path().join("log");
    let p = host(json!({"global":{"rust":{}}}));
    let original = serde_json::to_vec(&p).unwrap();
    let mut old = Session::open(engine(&root, &log), p, Some("same"), &d()).unwrap();
    let cfg = old.invoke("configuration", &json!({}), &d()).unwrap();
    let reply = hover(&mut old, "main.rs");
    let before = records(&root);
    let p = host(json!({"global":{"rust":{"settings":null}}}));
    assert_eq!(serde_json::to_vec(&p).unwrap(), original);
    let mut new = Session::open(engine(&root, &log), p, Some("same"), &d()).unwrap();
    assert_eq!(new.invoke("configuration", &json!({}), &d()).unwrap(), cfg);
    assert_eq!(hover(&mut new, "main.rs"), reply);
    assert_eq!(records(&root), before);
}

#[test]
fn project_and_passive_authority_are_checked_before_provider_start() {
    let r = tempfile::tempdir().unwrap();
    let root = r.path().join("source");
    source(&root);
    let log = r.path().join("log");
    let mut p = host(json!({"global":{"rust":{"settings":{"tag":"selected"}}}}));
    p.trusted_server_settings_projects.clear();
    assert!(Session::open(engine(&root, &log), p, Some("untrusted"), &d()).is_err());
    assert!(!log.exists());
    let mut e = Engine::open(&root).unwrap();
    let mut unsafe_provider = provider(&log, "rust");
    unsafe_provider.server.trust = false;
    assert!(e.register_backend("rust", unsafe_provider).is_err());
    assert!(!log.exists());
    for enabled in [true, false] {
        let mut e = Engine::open(&root).unwrap();
        let mut server = provider(&log, "rust");
        server.configuration.passive_profile = Some(lsp::PassiveProfile::RustAnalyzer);
        e.register_backend("rust", server).unwrap();
        let mut p = host(
            json!({"global":{"rust":{"settings":{"cargo":{"buildScripts":{"enable":enabled}}}}}}),
        );
        p.language_servers.as_mut().unwrap().global = Some(vec!["rust".into()]);
        let mut s = Session::open(
            e,
            p,
            Some(if enabled {
                "passive-denied"
            } else {
                "passive-safe"
            }),
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
        if enabled {
            assert_eq!(result["status"], "error");
            assert!(!log.exists());
        } else {
            assert_eq!(result["status"], "ok", "{result}");
        }
    }
    assert_eq!(fs::read(root.join("main.rs")).unwrap(), SOURCE.as_bytes());
}

#[test]
fn invalid_shapes_structure_merged_budgets_and_changed_bindings_refuse() {
    let r = tempfile::tempdir().unwrap();
    let root = r.path().join("source");
    source(&root);
    let log = r.path().join("log");
    for bad in [json!([]), json!(false), json!("text")] {
        assert!(
            serde_json::from_value::<yoke::server_settings::Settings>(json!({"settings":bad}))
                .is_err()
        );
    }
    let mut deep = json!(0);
    for _ in 0..33 {
        deep = json!({"next":deep});
    }
    for options in [
        json!({"huge":"x".repeat(64001)}),
        json!({"many":vec![0;4097]}),
        json!({"deep":deep}),
    ] {
        let p = host(json!({"global":{"rust":{"settings":options}}}));
        assert!(Session::open(engine(&root, &log), p, Some("bad"), &d()).is_err());
    }
    let mut e = Engine::open(&root).unwrap();
    let mut server = provider(&log, "rust");
    server.configuration.settings = json!({"original":"x".repeat(63000)});
    e.register_backend("rust", server).unwrap();
    let mut p = host(json!({"global":{"rust":{"settings":{"new":"y".repeat(2000)}}}}));
    p.language_servers.as_mut().unwrap().global = Some(vec!["rust".into()]);
    assert!(Session::open(e, p, Some("merged"), &d()).is_err());
    assert!(!log.exists());
    let p = host(json!({"global":{"rust":{"settings":{"tag":"one"}}}}));
    let _s = Session::open(engine(&root, &log), p, Some("bound"), &d()).unwrap();
    let before = records(&root);
    let changed = host(json!({"global":{"rust":{"settings":{"tag":"two"}}}}));
    assert!(Session::open(engine(&root, &log), changed, Some("bound"), &d()).is_err());
    assert_eq!(records(&root), before);
    assert!(!log.exists());
}

#[test]
fn pinned_yaml_uses_existing_host_grants_without_creating_runtime_authority() {
    use yoke::session_config::{self, Source, Target};
    let bytes=b"ls_specific_settings:\n  rust:\n    settings:\n      nested:\n        selected: owned\n      replace: null\n";
    let hash = yoke::model::hash(bytes);
    let target = Target::Global;
    let selection = [Source {
        target: &target,
        expected_sha256: &hash,
        bytes,
    }];
    assert!(session_config::project(&HostProfile::default(), &selection, &d()).is_err());
    let base = host(json!({}));
    let plan = session_config::project(
        &base,
        &[Source {
            target: &target,
            expected_sha256: &hash,
            bytes,
        }],
        &d(),
    )
    .unwrap()
    .plan();
    assert_eq!(
        plan["hostProfile"]["languageServerSettings"]["global"]["rust"]["settings"],
        json!({"nested":{"selected":"owned"},"replace":null})
    );
    assert_eq!(plan["runtimeTrustFromYaml"], false);
    assert_eq!(plan["storeOpened"], false);
    assert_eq!(plan["networkRequests"], 0);
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
    assert!(
        session_config::project(
            &base,
            &[Source {
                target: &target,
                expected_sha256: &hash,
                bytes
            }],
            &Deadline::new(0)
        )
        .is_err()
    );
}

#[test]
fn authenticated_http_uses_current_workspace_settings_and_revocation_precedes_process() {
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
            format!("Authorization: Bearer {}\r\n", hex::encode([74; 32]))
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
            .find_map(|l| {
                l.split_once(':')
                    .filter(|(k, _)| k.eq_ignore_ascii_case("mcp-session-id"))
                    .map(|(_, v)| v.trim().to_owned())
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
    let p = host(json!({"global":{"rust":{"settings":{"nested":{"selected":"http"}}}}}));
    let mut target = Target::new(&root, Profile::Session, p);
    for id in ["rust", "typescript"] {
        let s = provider(&log, id);
        target.host_servers.push(serde_json::from_value(json!({"backendId":id,"command":s.server.command,"args":s.server.args,"configuration":s.configuration})).unwrap());
    }
    let principal = Principal::new(
        "workspace-settings",
        zeroize::Zeroizing::new(vec![74; 32]),
        target,
    )
    .unwrap();
    assert!(!log.exists());
    let server = mcp_http::serve(Config::default(), vec![principal]).unwrap();
    let address = server.local_addr();
    let init = json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"owned-workspace-settings","version":"1"}}});
    assert_eq!(request(address, None, false, init.clone()).0, 401);
    let (status, sid, _) = request(address, None, true, init);
    assert_eq!(status, 200);
    assert!(!sid.is_empty());
    assert!(!log.exists());
    let call = json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"ci_find","arguments":{"languageServer":{"path":"main.rs","offset":0,"operation":"hover"}}}});
    let (status, _, reply) = request(address, Some(&sid), true, call.clone());
    assert_eq!(status, 200);
    let value: Value =
        serde_json::from_str(reply["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(value["status"], "ok", "{value}");
    let body: Value = serde_json::from_str(
        value["data"]["response"]["contents"]["value"]
            .as_str()
            .unwrap(),
    )
    .unwrap();
    assert_eq!(body["settings"]["nested"]["selected"], "http");
    assert_eq!(body["pulled"][4], "http");
    let before = fs::read(&log).unwrap();
    assert!(server.revoke("workspace-settings").unwrap());
    assert_eq!(request(address, Some(&sid), true, call).0, 401);
    assert_eq!(fs::read(&log).unwrap(), before);
    server.shutdown().unwrap();
    assert!(TcpStream::connect(address).is_err());
    assert_eq!(fs::read(root.join("main.rs")).unwrap(), SOURCE.as_bytes());
}
