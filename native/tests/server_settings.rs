use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::OnceLock,
};
use yoke::{
    Engine, agent_repl,
    capture::Deadline,
    lsp,
    project_session::{HostProfile, HostProject, Session},
};
const SOURCE: &str = "// owned \u{1f4a1} e\u{301}\r\nfn visible() {}\r\n";
fn d() -> Deadline {
    Deadline::new(30000)
}
fn node() -> String {
    static NODE: OnceLock<String> = OnceLock::new();
    NODE.get_or_init(|| {
        let out = Command::new("node")
            .args(["-p", "process.execPath"])
            .output()
            .unwrap();
        assert!(out.status.success());
        String::from_utf8(out.stdout).unwrap().trim().into()
    })
    .clone()
}
fn root(parent: &Path, name: &str) -> PathBuf {
    let p = parent.join(name);
    fs::create_dir(&p).unwrap();
    fs::write(p.join("main.rs"), SOURCE).unwrap();
    fs::write(p.join("entry.ts"), "export function entry() {}\n").unwrap();
    p
}
fn configured(id: &str, log: &Path, execution: bool) -> lsp::ConfiguredServer {
    lsp::ConfiguredServer {
        server: lsp::Server {
            command: node(),
            args: vec![
                Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("tests/fixtures/server-settings-lsp.mjs")
                    .display()
                    .to_string(),
                log.display().to_string(),
                id.into(),
                "0".into(),
            ],
            language: yoke::server_catalog::get(id).unwrap().languages[0].into(),
            trust: true,
        },
        configuration: lsp::HostConfiguration {
            initialization_options: json!({"keep":"base","nested":{"base":1,"old":2},"replace":"original"}),
            passive_profile: Some(lsp::PassiveProfile::ContractFixture),
            allow_source_execution: execution,
            ..Default::default()
        },
    }
}
fn engine(root: &Path, log: &Path, execution: bool) -> Engine {
    let mut e = Engine::open(root).unwrap();
    e.register_configured_server(configured("rust", log, execution))
        .unwrap();
    for id in ["rust", "typescript"] {
        e.register_backend(id, configured(id, log, execution))
            .unwrap();
    }
    e
}
fn profile(settings: Value) -> HostProfile {
    serde_json::from_value(json!({"languageServers":{"global":["rust","typescript"]},"trustedServerSettingsProjects":["default"],"languageServerSettings":settings})).unwrap()
}
fn open(root: &Path, log: &Path, p: HostProfile, id: &str, execution: bool) -> Session {
    Session::open(engine(root, log, execution), p, Some(id), &d()).unwrap()
}
fn hover(e: &mut Engine, path: &str) -> Value {
    let reply = e
        .invoke(
            "ci_find",
            json!({"languageServer":{"path":path,"offset":0,"operation":"hover"}}),
        )
        .unwrap();
    assert_eq!(reply["status"], "ok", "{reply}");
    reply["data"].clone()
}
fn payload(reply: &Value) -> Value {
    serde_json::from_str(reply["response"]["contents"]["value"].as_str().unwrap()).unwrap()
}
fn rows(log: &Path) -> Vec<Value> {
    fs::read_to_string(log)
        .unwrap_or_default()
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect()
}

#[test]
fn absent_null_empty_and_dormant_settings_grants_keep_original_wire_and_binding() {
    let r = tempfile::tempdir().unwrap();
    let root = root(r.path(), "source");
    let log = r.path().join("log");
    let old = HostProfile::default();
    let bytes = serde_json::to_vec(&old).unwrap();
    let mut original = open(&root, &log, old, "legacy", false);
    let config = original.invoke("configuration", &json!({}), &d()).unwrap();
    let response = hover(&mut original.engine, "main.rs");
    for settings in [
        Value::Null,
        json!({}),
        json!({"global":{},"projects":{"default":{"project":null,"local":{}}}}),
    ] {
        let mut p: HostProfile =
            serde_json::from_value(json!({"languageServerSettings":settings})).unwrap();
        assert_eq!(serde_json::to_vec(&p).unwrap(), bytes);
        p.trusted_server_settings_projects = vec!["default".into()];
        let mut s = open(&root, &log, p, "legacy", false);
        assert_eq!(s.invoke("configuration", &json!({}), &d()).unwrap(), config);
        assert_eq!(
            serde_json::to_vec(&hover(&mut s.engine, "main.rs")).unwrap(),
            serde_json::to_vec(&response).unwrap()
        );
    }
    assert_eq!(fs::read(root.join("main.rs")).unwrap(), SOURCE.as_bytes());
}

#[test]
fn actual_initialize_shallow_overlay_whole_backend_layers_and_projects_are_independent() {
    let r = tempfile::tempdir().unwrap();
    let primary = root(r.path(), "primary");
    let secondary = root(r.path(), "second");
    let log = r.path().join("log");
    let mut p = profile(
        json!({"global":{"rust":{"initializationOptions":{"nested":{"global":3},"replace":null}},"typescript":{"initializationOptions":{"tag":"global-ts"}}},"projects":{"default":{"project":{"rust":{"initializationOptions":{"nested":{"project":4},"onlyProject":true}}},"local":{"rust":null}},"second":{"local":{"rust":{}}}}}),
    );
    p.projects.push(HostProject {
        name: "second".into(),
        root: secondary,
        read_only: true,
        query_only: false,
    });
    p.trusted_server_settings_projects.push("second".into());
    let mut s = open(&primary, &log, p.clone(), "layers", false);
    let first = hover(&mut s.engine, "main.rs");
    assert_eq!(
        payload(&first)["initializationOptions"],
        json!({"keep":"base","nested":{"project":4},"replace":"original","onlyProject":true,"cargo":{"buildScripts":{"enable":false}},"procMacro":{"enable":false},"checkOnSave":false})
    );
    assert_eq!(
        payload(&hover(&mut s.engine, "entry.ts"))["initializationOptions"]["tag"],
        "global-ts"
    );
    let refreshed = lsp::restart_registered(&mut s.engine, &d()).unwrap();
    assert_eq!(refreshed["providers"].as_array().unwrap().len(), 2);
    s.invoke("activate", &json!({"project":"second"}), &d())
        .unwrap();
    let second = hover(s.active_engine().unwrap(), "main.rs");
    assert_eq!(
        payload(&second)["initializationOptions"],
        json!({"keep":"base","nested":{"base":1,"old":2},"replace":"original","cargo":{"buildScripts":{"enable":false}},"procMacro":{"enable":false},"checkOnSave":false})
    );
    assert_ne!(first["serverProfileHash"], second["serverProfileHash"]);
    s.invoke("activate", &json!({"project":"default"}), &d())
        .unwrap();
    assert_eq!(payload(&hover(&mut s.engine, "main.rs")), payload(&first));
    assert!(
        rows(&log)
            .iter()
            .filter(|v| v["event"] == "initialization-options")
            .count()
            >= 6
    );
    assert_eq!(
        fs::read(primary.join("main.rs")).unwrap(),
        SOURCE.as_bytes()
    );
}

#[test]
fn launch_replacement_and_appending_reach_real_process_only_with_both_host_grants() {
    let r = tempfile::tempdir().unwrap();
    let root = root(r.path(), "source");
    let log = r.path().join("log");
    let original = configured("rust", &log, true);
    let mut p = profile(
        json!({"global":{"rust":{"ls_base_cmd":[node(),original.server.args[0].clone()],"ls_args":[log.display().to_string(),"replaced","0"],"ls_extra_args":["owned-extra-\u{e9}"]}}}),
    );
    assert!(
        Session::open(
            engine(&root, &log, false),
            p.clone(),
            Some("no-source-grant"),
            &d()
        )
        .is_err()
    );
    assert!(!log.exists());
    p.trusted_server_settings_projects.clear();
    assert!(
        Session::open(
            engine(&root, &log, true),
            p.clone(),
            Some("no-settings-grant"),
            &d()
        )
        .is_err()
    );
    assert!(!log.exists());
    p.trusted_server_settings_projects.push("default".into());
    let mut s = open(&root, &log, p, "launch", true);
    let v = hover(&mut s.engine, "main.rs");
    let body = payload(&v);
    assert_eq!(body["actor"], "replaced");
    assert_eq!(body["argv"][3], "owned-extra-\u{e9}");
    assert_eq!(v["sourceExecutionAuthorized"], true);
    let p = profile(json!({"global":{"rust":{"ls_path":node(),"ls_extra_args":["path-extra"]}}}));
    let mut s = open(&root, &log, p, "path", true);
    assert_eq!(
        payload(&hover(&mut s.engine, "main.rs"))["argv"][3],
        "path-extra"
    );
    assert_eq!(fs::read(root.join("main.rs")).unwrap(), SOURCE.as_bytes());
}

#[test]
fn effective_passive_policy_refuses_execution_options_before_starting_and_accepts_safe_overlay() {
    let r = tempfile::tempdir().unwrap();
    let root = root(r.path(), "source");
    let log = r.path().join("log");
    for (label, enable) in [("denied", true), ("safe", false)] {
        let mut e = Engine::open(&root).unwrap();
        let mut provider = configured("rust", &log, false);
        provider.configuration.passive_profile = Some(lsp::PassiveProfile::RustAnalyzer);
        e.register_backend("rust", provider).unwrap();
        let p = profile(
            json!({"global":{"rust":{"initializationOptions":{"cargo":{"buildScripts":{"enable":enable}},"nested":{"unicode":"\u{e9} e\u{301}"}}}}}),
        );
        let mut p = p;
        p.language_servers.as_mut().unwrap().global = Some(vec!["rust".into()]);
        let mut s = Session::open(e, p, Some(label), &d()).unwrap();
        let response = s
            .engine
            .invoke(
                "ci_find",
                json!({"languageServer":{"path":"main.rs","offset":0,"operation":"hover"}}),
            )
            .unwrap();
        if enable {
            assert_eq!(response["status"], "error");
            assert!(!log.exists());
        } else {
            assert_eq!(response["status"], "ok", "{response}");
            assert_eq!(
                payload(&response["data"])["initializationOptions"]["cargo"]["buildScripts"]["enable"],
                false
            );
        }
    }
    assert_eq!(fs::read(root.join("main.rs")).unwrap(), SOURCE.as_bytes());
}

#[test]
fn changed_policy_registry_or_host_grants_refuse_original_session_and_stale_clients() {
    let r = tempfile::tempdir().unwrap();
    let primary = root(r.path(), "source");
    let secondary = root(r.path(), "second");
    let log = r.path().join("log");
    let mut p = profile(json!({"global":{"rust":{"initializationOptions":{"tag":"a"}}}}));
    p.projects.push(HostProject {
        name: "second".into(),
        root: secondary,
        read_only: false,
        query_only: false,
    });
    p.trusted_server_settings_projects.push("second".into());
    let mut s = open(&primary, &log, p.clone(), "binding", false);
    let mut stale = open(&primary, &log, p.clone(), "binding", false);
    s.invoke("activate", &json!({"project":"second"}), &d())
        .unwrap();
    assert!(
        agent_repl::invoke_facade(
            &mut stale,
            "lsp.find_symbol",
            json!({"name_path_pattern":"visible"}),
            None,
            &d()
        )
        .is_err()
    );
    let mut changed = p.clone();
    changed
        .language_server_settings
        .as_mut()
        .unwrap()
        .global
        .as_mut()
        .unwrap()
        .get_mut("rust")
        .unwrap()
        .as_mut()
        .unwrap()
        .initialization_options
        .as_mut()
        .unwrap()
        .insert("tag".into(), json!("b"));
    assert!(
        Session::open(
            engine(&primary, &log, false),
            changed,
            Some("binding"),
            &d()
        )
        .is_err()
    );
    let mut changed = p.clone();
    changed
        .trusted_server_settings_projects
        .retain(|v| v != "default");
    assert!(
        Session::open(
            engine(&primary, &log, false),
            changed,
            Some("binding"),
            &d()
        )
        .is_err()
    );
    let mut e = Engine::open(&primary).unwrap();
    let mut provider = configured("rust", &log, false);
    provider.configuration.initialization_options["keep"] = json!("changed");
    e.register_backend("rust", provider).unwrap();
    e.register_backend("typescript", configured("typescript", &log, false))
        .unwrap();
    assert!(Session::open(e, p, Some("binding"), &d()).is_err());
}

#[test]
fn closed_schema_path_unicode_capacity_and_missing_authority_fail_before_process_or_session() {
    let r = tempfile::tempdir().unwrap();
    let root = root(r.path(), "source");
    let log = r.path().join("log");
    for settings in [
        json!({"trust":true}),
        json!({"allowSourceExecution":true}),
        json!({"initializationOptions":[]}),
        json!({"ls_path":"relative.exe"}),
        json!({"ls_base_cmd":[]}),
        json!({"ls_base_cmd":[node()],"ls_path":node()}),
        json!({"ls_extra_args":["nul\0argument"]}),
        json!({"ls_args":vec!["a";129]}),
        json!({"initializationOptions":{"huge":"x".repeat(64001)}}),
        json!({"initializationOptions":{"many":vec![0;4097]}}),
    ] {
        if let Ok(p) = serde_json::from_value::<HostProfile>(
            json!({"languageServers":{"global":["rust"]},"trustedServerSettingsProjects":["default"],"languageServerSettings":{"global":{"rust":settings}}}),
        ) {
            assert!(Session::open(engine(&root, &log, false), p, Some("bad"), &d()).is_err());
        }
    }
    for settings in [
        json!({"global":{"../rust":{}}}),
        json!({"global":{"angular":{}}}),
        json!({"projects":{"unregistered":{"project":{"rust":{}}}}}),
    ] {
        assert!(
            Session::open(
                engine(&root, &log, false),
                profile(settings),
                Some("bad"),
                &d()
            )
            .is_err()
        );
    }
    let mut p = profile(json!({"global":{"rust":{}}}));
    p.language_servers = None;
    assert!(Session::open(engine(&root, &log, false), p, Some("bad"), &d()).is_err());
    assert!(!log.exists());
    let db = rusqlite::Connection::open(root.join(".yoke/index.sqlite")).unwrap();
    let count: i64 = db
        .query_row(
            "SELECT count(*) FROM records WHERE kind='native_agent_session_v1'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(count, 0);
    let mut e = Engine::open(&root).unwrap();
    let mut tcp = configured("rust", &log, true);
    tcp.server.command.clear();
    tcp.server.args.clear();
    tcp.configuration.tcp_endpoint = Some("127.0.0.1:9".parse().unwrap());
    e.register_backend("rust", tcp).unwrap();
    let mut p = profile(json!({"global":{"rust":{"ls_args":[]}}}));
    p.language_servers.as_mut().unwrap().global = Some(vec!["rust".into()]);
    assert!(Session::open(e, p, Some("tcp"), &d()).is_err());
    assert_eq!(fs::read(root.join("main.rs")).unwrap(), SOURCE.as_bytes());
}

#[test]
fn actual_staged_and_owned_refactor_intents_use_settings_without_rewriting_source() {
    let r = tempfile::tempdir().unwrap();
    let root = root(r.path(), "source");
    let log = r.path().join("log");
    let p = profile(
        json!({"global":{"rust":{"initializationOptions":{"actionTitle":"selected-action"}}}}),
    );
    let mut s = open(&root, &log, p, "staged", false);
    let response = s
        .engine
        .invoke(
            "ci_find",
            json!({"languageServer":{"path":"main.rs","offset":0,"operation":"codeActions"}}),
        )
        .unwrap();
    assert_eq!(response["status"], "ok", "{response}");
    let action = &response["data"]["items"][0];
    assert_eq!(action["title"], "selected-action");
    assert_eq!(action["usable"], true);
    assert_eq!(action["plan"]["files"][0]["path"], "main.rs");
    fs::create_dir(root.join("src")).unwrap();
    let before = "pub fn helper() -> u8 { 7 }\n";
    fs::write(root.join("src/source.rs"), before).unwrap();
    fs::write(
        root.join("src/destination.rs"),
        "pub mod bucket {\npub fn sentinel() {}\n}\n",
    )
    .unwrap();
    let owned_log = r.path().join("owned.log");
    let mut e = Engine::open(&root).unwrap();
    e.allow_writes = true;
    let mut provider = configured("rust", &owned_log, false);
    provider.server.args = vec![
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/own-refactor-intent-lsp.mjs")
            .display()
            .to_string(),
        owned_log.display().to_string(),
        "valid".into(),
    ];
    provider.configuration.refactor_intent_profile = Some(lsp::RefactorIntentProfile::OwnV1);
    e.register_backend("rust", provider).unwrap();
    let mut p = profile(
        json!({"global":{"rust":{"initializationOptions":{"ownedMarker":"selected-owned"}}}}),
    );
    p.language_servers.as_mut().unwrap().global = Some(vec!["rust".into()]);
    let mut s = Session::open(e, p, Some("owned"), &d()).unwrap();
    let moved=agent_repl::invoke_facade(&mut s,"jb.move",json!({"relative_path":"src/source.rs","name_path":"helper","target_relative_path":"src/destination.rs"}),None,&d()).unwrap();
    assert!(moved["plan"].is_object());
    let transcript = rows(&owned_log);
    assert!(transcript.iter().any(|v| v["method"] == "initialize"
        && v["params"]["initializationOptions"]["ownedMarker"] == "selected-owned"));
    assert!(
        transcript
            .iter()
            .any(|v| v["method"] == "yoke/refactorIntent")
    );
    assert_eq!(fs::read(root.join("main.rs")).unwrap(), SOURCE.as_bytes());
    assert_eq!(
        fs::read_to_string(root.join("src/source.rs")).unwrap(),
        before
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
        .set_read_timeout(Some(std::time::Duration::from_secs(15)))
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
fn actual_http_settings_require_current_principal_and_revocation_precedes_new_process() {
    use yoke::{
        mcp_http::{self, Config, Principal, Target},
        protocol::Profile,
    };
    let r = tempfile::tempdir().unwrap();
    let root = root(r.path(), "source");
    let log = r.path().join("log");
    let host = profile(
        json!({"global":{"rust":{"initializationOptions":{"httpMarker":"selected-http"}}}}),
    );
    let mut target = Target::new(&root, Profile::Session, host);
    for id in ["rust", "typescript"] {
        let provider = configured(id, &log, false);
        target.host_servers.push(serde_json::from_value(json!({"backendId":id,"command":provider.server.command,"args":provider.server.args,"configuration":provider.configuration})).unwrap());
    }
    let principal = Principal::new(
        "owned-settings",
        zeroize::Zeroizing::new(vec![73; 32]),
        target,
    )
    .unwrap();
    assert!(!log.exists());
    let server = mcp_http::serve(Config::default(), vec![principal]).unwrap();
    let address = server.local_addr();
    let init = json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"owned-settings","version":"1"}}});
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
    let request = json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"ci_find","arguments":{"languageServer":{"path":"main.rs","offset":0,"operation":"hover"}}}});
    let (status, _, reply) = http(address, Some(sid), true, request.clone());
    assert_eq!(status, 200);
    assert_eq!(reply["result"]["isError"], false, "{reply}");
    let value: Value =
        serde_json::from_str(reply["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(value["status"], "ok", "{value}");
    assert_eq!(
        payload(&value["data"])["initializationOptions"]["httpMarker"],
        "selected-http"
    );
    let before = rows(&log).len();
    assert!(server.revoke("owned-settings").unwrap());
    assert_eq!(http(address, Some(sid), true, request).0, 401);
    assert_eq!(rows(&log).len(), before);
    server.shutdown().unwrap();
    assert!(std::net::TcpStream::connect(address).is_err());
    assert_eq!(fs::read(root.join("main.rs")).unwrap(), SOURCE.as_bytes());
}
