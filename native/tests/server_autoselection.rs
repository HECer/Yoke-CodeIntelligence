use serde_json::{Value, json};
use std::{
    fs,
    io::{Read, Write},
    path::Path,
    process::Command,
};
use yoke::{
    Engine,
    capture::Deadline,
    lsp,
    project_session::{HostProfile, HostProject, Session},
    session_config::{self, Source, Target},
};
const RUST: &str = "// owned \u{1f4a1} e\u{301}\r\nfn visible() {}\r\n";
const PYTHON: &str = "# owned \u{1f4a1} e\u{301}\r\ndef visible():\r\n    pass\r\n";
fn d() -> Deadline {
    Deadline::new(30000)
}
fn node() -> String {
    let out = Command::new("node")
        .args(["-p", "process.execPath"])
        .output()
        .unwrap();
    assert!(out.status.success());
    String::from_utf8(out.stdout).unwrap().trim().into()
}
fn configured(id: &str, log: &Path, actor: &str) -> lsp::ConfiguredServer {
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
                "0".into(),
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
fn engine(root: &Path, log: &Path, ids: &[&str]) -> Engine {
    let mut engine = Engine::open(root).unwrap();
    engine
        .register_configured_server(configured("rust", log, "legacy"))
        .unwrap();
    engine
        .register_configured_server(configured("python", log, "legacy-python"))
        .unwrap();
    for id in ids {
        engine
            .register_backend(id, configured(id, log, id))
            .unwrap();
    }
    engine
}
fn profile(value: Value) -> HostProfile {
    serde_json::from_value(value).unwrap()
}
fn automatic(priorities: Value) -> HostProfile {
    profile(json!({"languageServerAutoSelection":{"priorities":priorities}}))
}
fn source(root: &Path) {
    fs::write(root.join("main.rs"), RUST).unwrap();
    fs::write(root.join("owned.py"), PYTHON).unwrap();
}
fn config(session: &mut Session) -> Value {
    session.invoke("configuration", &json!({}), &d()).unwrap()
}
fn actor(value: &Value) -> Value {
    serde_json::from_str::<Value>(value["response"]["contents"]["value"].as_str().unwrap()).unwrap()
        ["actor"]
        .clone()
}
fn query(session: &mut Session, path: &str) -> Value {
    session
        .engine
        .invoke(
            "ci_find",
            json!({"languageServer":{"path":path,"offset":0,"operation":"hover"}}),
        )
        .unwrap()
}

#[test]
fn global_projection_distinguishes_default_opt_in_null_clear_and_invalid_scope_types() {
    let target = Target::Global;
    let text = b"ls_priorities: {}\n";
    let hash = yoke::model::hash(text);
    let base = HostProfile::default();
    let selected = session_config::project(
        &base,
        &[Source {
            target: &target,
            expected_sha256: &hash,
            bytes: text,
        }],
        &d(),
    )
    .unwrap();
    assert_eq!(
        serde_json::to_value(selected.profile()).unwrap()["languageServerAutoSelection"],
        json!({"priorities":{}})
    );
    let clear = b"ls_priorities: null\n";
    let hash = yoke::model::hash(clear);
    let cleared = session_config::project(
        selected.profile(),
        &[Source {
            target: &target,
            expected_sha256: &hash,
            bytes: clear,
        }],
        &d(),
    )
    .unwrap();
    assert_eq!(
        serde_json::to_value(cleared.profile()).unwrap(),
        serde_json::to_value(base).unwrap()
    );
    for bad in [
        "ls_priorities: true\n",
        "ls_priorities: {rust: true}\n",
        "ls_priorities: {rust: 1.5}\n",
        "ls_priorities: {unknown: 2}\n",
        "ls_priorities: {rust: 2147483648}\n",
    ] {
        let hash = yoke::model::hash(bad);
        assert!(
            session_config::project(
                &HostProfile::default(),
                &[Source {
                    target: &target,
                    expected_sha256: &hash,
                    bytes: bad.as_bytes()
                }],
                &d()
            )
            .is_err()
        );
    }
    for target in [
        Target::Project {
            project: "default".into(),
        },
        Target::ProjectLocal {
            project: "default".into(),
        },
        Target::Context {
            name: "owned".into(),
        },
        Target::Mode {
            name: "owned".into(),
        },
    ] {
        let text = b"ls_priorities: {rust: 3}\nprompt: '{{ value }}'\n";
        let hash = yoke::model::hash(text);
        let mut calls = 0;
        let mut renderer = |_: &str, _: &Deadline| {
            calls += 1;
            Ok(session_config::RenderedPrompt {
                text: "rendered".into(),
                evidence: json!({}),
            })
        };
        assert!(
            session_config::project_with_prompt_renderer(
                &HostProfile::default(),
                &[Source {
                    target: &target,
                    expected_sha256: &hash,
                    bytes: text
                }],
                &d(),
                &mut renderer
            )
            .is_err()
        );
        assert_eq!(calls, 0);
    }
}
#[test]
fn actual_owned_provider_routing_stays_frozen_after_source_edits_and_reopen() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("owned \u{3c0} e\u{301}");
    fs::create_dir(&root).unwrap();
    source(&root);
    let log = temp.path().join("providers.log");
    let host = automatic(json!({"python":0}));
    let mut session = Session::open(
        engine(&root, &log, &["rust", "python"]),
        host.clone(),
        Some("frozen"),
        &d(),
    )
    .unwrap();
    let initial = config(&mut session);
    assert_eq!(
        initial["automaticLanguageServers"]["default"]["backendId"],
        "rust"
    );
    assert!(!log.exists());
    let result = query(&mut session, "main.rs");
    assert_eq!(result["status"], "ok", "{result}");
    assert_eq!(result["data"]["selectedBackendId"], "rust");
    assert_eq!(actor(&result["data"]), "rust");
    let before = fs::read(&log).unwrap();
    let rejected = query(&mut session, "owned.py");
    assert_eq!(rejected["status"], "error", "{rejected}");
    let (snapshot, graph, _, _) = session.engine.index(&d()).unwrap();
    assert!(
        lsp::run_at_configured(
            &mut session.engine,
            &configured("python", &log, "direct-legacy"),
            &snapshot,
            &graph,
            lsp::SemanticRequest {
                path: "owned.py",
                offset: 0,
                operation: "hover",
                new_name: None
            },
            &json!({}),
            &d(),
        )
        .is_err()
    );
    assert_eq!(fs::read(&log).unwrap(), before);
    for i in 0..4 {
        fs::write(root.join(format!("extra{i}.py")), PYTHON).unwrap();
    }
    drop(session);
    let mut resumed = Session::open(
        engine(&root, &log, &["rust", "python"]),
        host,
        Some("frozen"),
        &d(),
    )
    .unwrap();
    assert_eq!(
        config(&mut resumed)["automaticLanguageServers"],
        initial["automaticLanguageServers"]
    );
    assert_eq!(actor(&query(&mut resumed, "main.rs")["data"]), "rust");
    assert!(
        Session::open(
            engine(&root, &log, &["rust", "python"]),
            automatic(json!({"python":3})),
            Some("frozen"),
            &d()
        )
        .is_err()
    );
    assert_eq!(fs::read(root.join("main.rs")).unwrap(), RUST.as_bytes());
    assert_eq!(fs::read(root.join("owned.py")).unwrap(), PYTHON.as_bytes());
}
#[test]
fn missing_winner_registration_refuses_and_empty_detection_never_uses_legacy_fallback() {
    let temp = tempfile::tempdir().unwrap();
    source(temp.path());
    let log = temp.path().join("never-started.log");
    let error = Session::open(
        engine(temp.path(), &log, &["rust"]),
        automatic(json!({"python":3})),
        Some("unregistered"),
        &d(),
    )
    .err()
    .unwrap();
    assert!(
        error
            .to_string()
            .contains("language_server_not_host_registered")
    );
    assert!(!log.exists());
    let mut empty = Session::open(
        engine(temp.path(), &log, &["rust", "python"]),
        automatic(json!({"rust":0,"python":0})),
        Some("empty"),
        &d(),
    )
    .unwrap();
    let status = config(&mut empty);
    assert!(status["automaticLanguageServers"]["default"]["backendId"].is_null());
    assert_eq!(
        status["automaticLanguageServers"]["default"]["matchedFiles"],
        0
    );
    assert_eq!(query(&mut empty, "main.rs")["status"], "error");
    assert!(lsp::restart_registered(&mut empty.engine, &d()).is_err());
    assert!(!log.exists());
}
#[test]
fn explicit_ordered_selections_override_auto_and_absent_null_host_policy_preserves_legacy() {
    let temp = tempfile::tempdir().unwrap();
    source(temp.path());
    let log = temp.path().join("providers.log");
    let mut old = Session::open(
        engine(temp.path(), &log, &[]),
        HostProfile::default(),
        Some("legacy"),
        &d(),
    )
    .unwrap();
    let original = config(&mut old);
    drop(old);
    let host = profile(json!({"languageServerAutoSelection":null}));
    assert_eq!(
        serde_json::to_value(&host).unwrap(),
        serde_json::to_value(HostProfile::default()).unwrap()
    );
    let mut same = Session::open(
        engine(temp.path(), &log, &["rust", "python"]),
        host,
        Some("legacy"),
        &d(),
    )
    .unwrap();
    assert_eq!(config(&mut same), original);
    assert_eq!(actor(&query(&mut same, "main.rs")["data"]), "legacy");
    let host = profile(
        json!({"languageServerAutoSelection":{"priorities":{"rust":0,"python":3}},"languageServers":{"global":["rust"]}}),
    );
    let mut explicit = Session::open(
        engine(temp.path(), &log, &["rust", "python"]),
        host,
        Some("explicit"),
        &d(),
    )
    .unwrap();
    assert_eq!(
        config(&mut explicit)["automaticLanguageSelectionActive"],
        false
    );
    assert_eq!(actor(&query(&mut explicit, "owned.py")["data"]), "rust");
}
#[test]
fn secondary_creation_survives_activation_state_publication_and_revision_advances_twice() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("primary");
    let other = temp.path().join("secondary");
    fs::create_dir(&root).unwrap();
    fs::create_dir(&other).unwrap();
    fs::write(root.join("main.rs"), RUST).unwrap();
    fs::write(other.join("owned.py"), PYTHON).unwrap();
    let log = temp.path().join("providers.log");
    let mut host = automatic(json!({}));
    host.projects.push(HostProject {
        name: "secondary".into(),
        root: other.canonicalize().unwrap(),
        read_only: true,
        query_only: false,
    });
    let mut session = Session::open(
        engine(&root, &log, &["rust", "python"]),
        host.clone(),
        Some("secondary"),
        &d(),
    )
    .unwrap();
    let before = config(&mut session)["revision"].as_u64().unwrap();
    let activated = session
        .invoke("activate", &json!({"project":"secondary"}), &d())
        .unwrap();
    assert_eq!(activated["activeProject"], "secondary");
    assert_eq!(
        activated["automaticLanguageServers"]["secondary"]["backendId"],
        "python"
    );
    assert_eq!(activated["revision"].as_u64().unwrap(), before + 2);
    let result=session.invoke("queryProject",&json!({"project":"secondary","tool":"ci_find","arguments":{"languageServer":{"path":"owned.py","offset":0,"operation":"hover"}}}),&d()).unwrap();
    assert_eq!(actor(&result["data"]), "python");
    drop(session);
    let mut resumed = Session::open(
        engine(&root, &log, &["rust", "python"]),
        host,
        Some("secondary"),
        &d(),
    )
    .unwrap();
    assert_eq!(
        config(&mut resumed)["automaticLanguageServers"],
        activated["automaticLanguageServers"]
    );
    assert_eq!(fs::read(other.join("owned.py")).unwrap(), PYTHON.as_bytes());
}
#[test]
fn real_stdio_reports_the_persisted_creation_choice_without_starting_a_provider() {
    let temp = tempfile::tempdir().unwrap();
    source(temp.path());
    let log = temp.path().join("providers.log");
    let mut session = Session::open(
        engine(temp.path(), &log, &["rust", "python"]),
        automatic(json!({"python":0})),
        Some("wire"),
        &d(),
    )
    .unwrap();
    let input=[json!({"jsonrpc":"2.0","id":1,"method":"initialize"}),json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"get_current_config","arguments":{}}})].iter().map(|v|v.to_string()+"\n").collect::<String>();
    let mut output = Vec::new();
    yoke::protocol::serve_serena(&mut session, std::io::Cursor::new(input), &mut output).unwrap();
    let rows: Vec<Value> = String::from_utf8(output)
        .unwrap()
        .lines()
        .map(|s| serde_json::from_str(s).unwrap())
        .collect();
    assert_eq!(rows[1]["result"]["isError"], false);
    let v: Value =
        serde_json::from_str(rows[1]["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
    let text = v.to_string();
    assert!(text.contains("automaticLanguageServers") && text.contains("rust"));
    assert!(!log.exists());
}

fn post(
    address: std::net::SocketAddr,
    session: Option<&str>,
    body: Value,
) -> (u16, Option<String>, Value) {
    let bytes = serde_json::to_vec(&body).unwrap();
    let session = session
        .map(|id| format!("Mcp-Session-Id: {id}\r\nMcp-Protocol-Version: 2025-11-25\r\n"))
        .unwrap_or_default();
    let mut stream = std::net::TcpStream::connect(address).unwrap();
    stream
        .set_read_timeout(Some(std::time::Duration::from_secs(10)))
        .unwrap();
    write!(stream,"POST /mcp HTTP/1.1\r\nHost: {address}\r\nAuthorization: Bearer {}\r\n{session}Accept: application/json, text/event-stream\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n", hex::encode([51;32]), bytes.len()).unwrap();
    stream.write_all(&bytes).unwrap();
    let mut output = Vec::new();
    loop {
        let mut block = [0; 8192];
        let n = stream.read(&mut block).unwrap();
        assert_ne!(n, 0);
        output.extend_from_slice(&block[..n]);
        assert!(output.len() <= 1_000_000);
        if let Some(end) = output.windows(4).position(|b| b == b"\r\n\r\n") {
            let header = std::str::from_utf8(&output[..end]).unwrap();
            let length: usize = header
                .lines()
                .filter_map(|s| s.split_once(':'))
                .find(|(k, _)| k.eq_ignore_ascii_case("content-length"))
                .unwrap()
                .1
                .trim()
                .parse()
                .unwrap();
            if output.len() >= end + 4 + length {
                let id = header
                    .lines()
                    .filter_map(|s| s.split_once(':'))
                    .find(|(k, _)| k.eq_ignore_ascii_case("mcp-session-id"))
                    .map(|(_, v)| v.trim().to_owned());
                return (
                    header.split_whitespace().nth(1).unwrap().parse().unwrap(),
                    id,
                    if length == 0 {
                        Value::Null
                    } else {
                        serde_json::from_slice(&output[end + 4..end + 4 + length]).unwrap()
                    },
                );
            }
        }
    }
}

#[test]
fn actual_authenticated_http_uses_host_registered_creation_selection_and_revocation() {
    use yoke::mcp_http::{self, Config, Principal, Target};
    let temp = tempfile::tempdir().unwrap();
    source(temp.path());
    let log = temp.path().join("providers.log");
    let mut target = Target::new(
        temp.path(),
        yoke::protocol::Profile::Session,
        automatic(json!({"python":3})),
    );
    for id in ["rust", "python"] {
        let configured = configured(id, &log, id);
        target.host_servers.push(serde_json::from_value(json!({"backendId":id,"command":configured.server.command,"args":configured.server.args,"configuration":{"passiveProfile":"contract_fixture"}})).unwrap());
    }
    let principal =
        Principal::new("owned-auto", zeroize::Zeroizing::new(vec![51; 32]), target).unwrap();
    let server = mcp_http::serve(Config::default(), vec![principal]).unwrap();
    let address = server.local_addr();
    let (status, id, _) = post(
        address,
        None,
        json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"owned-auto","version":"1"}}}),
    );
    assert_eq!(status, 200);
    let id = id.unwrap();
    assert_eq!(
        post(
            address,
            Some(&id),
            json!({"jsonrpc":"2.0","method":"notifications/initialized"})
        )
        .0,
        202
    );
    assert!(!log.exists());
    let request = json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"ci_find","arguments":{"languageServer":{"path":"owned.py","offset":0,"operation":"hover"}}}});
    let (status, _, reply) = post(address, Some(&id), request.clone());
    assert_eq!(status, 200);
    assert_eq!(reply["result"]["isError"], false, "{reply}");
    let result: Value =
        serde_json::from_str(reply["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(result["status"], "ok");
    assert_eq!(result["data"]["selectedBackendId"], "python");
    assert_eq!(actor(&result["data"]), "python");
    let before = fs::read(&log).unwrap();
    assert!(server.revoke("owned-auto").unwrap());
    assert_eq!(post(address, Some(&id), request).0, 401);
    assert_eq!(fs::read(&log).unwrap(), before);
    server.shutdown().unwrap();
    assert_eq!(
        fs::read(temp.path().join("owned.py")).unwrap(),
        PYTHON.as_bytes()
    );
}
