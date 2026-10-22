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
const APP: &str = "// app \u{1f4a1} e\u{301}\r\nfn local() { shared(); }\r\n";
const EXTRA: &str = "// extra \u{1f4a1} e\u{301}\r\npub fn shared() -> u8 { 7 }\r\n";
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
    let root = parent.join(name);
    fs::create_dir(&root).unwrap();
    for name in ["app", "shared", "other"] {
        fs::create_dir(root.join(name)).unwrap();
    }
    fs::write(root.join("app/main.rs"), APP).unwrap();
    fs::write(root.join("shared/\u{e9}.rs"), EXTRA).unwrap();
    fs::write(root.join("other/other.rs"), "fn other() {}\n").unwrap();
    root
}
fn configured(log: &Path) -> lsp::ConfiguredServer {
    lsp::ConfiguredServer {
        server: lsp::Server {
            command: node(),
            args: vec![
                Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("tests/fixtures/semantic-workspace-lsp.mjs")
                    .display()
                    .to_string(),
                log.display().to_string(),
            ],
            language: "rust".into(),
            trust: true,
        },
        configuration: lsp::HostConfiguration {
            passive_profile: Some(lsp::PassiveProfile::ContractFixture),
            ..Default::default()
        },
    }
}
fn engine(root: &Path, log: &Path) -> Engine {
    let mut e = Engine::open(root).unwrap();
    e.register_backend("rust", configured(log)).unwrap();
    e
}
fn profile(extra: Value) -> HostProfile {
    serde_json::from_value(json!({"languageServers":{"global":["rust"]},"symbolWorkspaceFolders":{"global":["app"]},"additionalWorkspaceFolders":extra})).unwrap()
}
fn open(root: &Path, log: &Path, host: HostProfile, id: &str) -> Session {
    Session::open(engine(root, log), host, Some(id), &d()).unwrap()
}
fn response(engine: &mut Engine, operation: &str) -> Value {
    engine.invoke("ci_find",json!({"languageServer":{"path":"app/main.rs","offset":APP.find("shared").unwrap(),"operation":operation}})).unwrap()
}
fn success(engine: &mut Engine, operation: &str) -> Value {
    let value = response(engine, operation);
    assert_eq!(value["status"], "ok", "{value}");
    value["data"].clone()
}
fn info(engine: &mut Engine) -> Value {
    let value = success(engine, "hover");
    serde_json::from_str(value["response"]["contents"]["value"].as_str().unwrap()).unwrap()
}
fn rows(log: &Path) -> Vec<Value> {
    fs::read_to_string(log)
        .unwrap_or_default()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}
fn symbols(session: &mut Session) -> Value {
    agent_repl::invoke_facade(
        session,
        "lsp.find_symbol",
        json!({"name_path_pattern":"*"}),
        None,
        &d(),
    )
    .unwrap()
}
fn frozen(engine: &mut Engine) -> Vec<u8> {
    let (snapshot, graph, _, _) = engine.index(&d()).unwrap();
    serde_json::to_vec(&(snapshot, graph)).unwrap()
}

fn unicode_roots(root: &Path) -> [(&'static str, &'static str); 2] {
    let values = [
        (
            "\u{130}",
            "// \u{1f4a1} e\u{301}\r\nfn unicode_first() {}\r\n",
        ),
        (
            "i\u{307}",
            "// \u{1f9ed} e\u{301}\r\nfn unicode_second() {}\r\n",
        ),
    ];
    assert_ne!(values[0].0, values[1].0);
    assert_eq!(values[0].0.to_lowercase(), values[1].0.to_lowercase());
    for (name, text) in values {
        // Exclusive directory creation also rejects a real filesystem alias.
        fs::create_dir(root.join(name)).unwrap();
        fs::write(root.join(name).join("main.rs"), text).unwrap();
    }
    for (name, text) in values {
        assert_eq!(
            fs::read(root.join(name).join("main.rs")).unwrap(),
            text.as_bytes()
        );
    }
    values
}

#[test]
fn distinct_unicode_roots_keep_both_source_bound_symbol_projections() {
    let r = tempfile::tempdir().unwrap();
    let root = root(r.path(), "source");
    let log = r.path().join("log");
    let values = unicode_roots(&root);
    let roots = values.iter().map(|v| v.0.to_owned()).collect::<Vec<_>>();
    assert_eq!(yoke::symbol_workspace::folders(&roots).unwrap(), roots);
    if cfg!(windows) {
        assert!(yoke::symbol_workspace::folders(&["app".into(), "APP".into()]).is_err());
    }
    let before = frozen(&mut engine(&root, &log));
    let p = serde_json::from_value(json!({"symbolWorkspaceFolders":{"global":roots}})).unwrap();
    let mut s = open(&root, &log, p, "unicode-symbols");
    let found = symbols(&mut s);
    assert_eq!(found["items"].as_array().unwrap().len(), 2, "{found}");
    assert_eq!(found["symbolIndexScope"]["folders"], json!(roots));
    for (folder, text) in values {
        let path = format!("{folder}/main.rs");
        let item = found["items"]
            .as_array()
            .unwrap()
            .iter()
            .find(|v| v["path"] == path)
            .unwrap();
        assert_eq!(item["contentHash"], yoke::model::hash(text));
        let start = item["nameStartByte"].as_u64().unwrap() as usize;
        let end = item["nameEndByte"].as_u64().unwrap() as usize;
        assert_eq!(&text[start..end], item["name"].as_str().unwrap());
        assert!(text[..start].contains('\u{301}'));
    }
    assert_eq!(frozen(&mut s.engine), before);
    assert!(!log.exists());
}

#[test]
fn distinct_unicode_primary_and_additional_roots_reach_provider_and_callback() {
    let r = tempfile::tempdir().unwrap();
    let root = root(r.path(), "source");
    let log = r.path().join("log");
    let values = unicode_roots(&root);
    let before = frozen(&mut engine(&root, &log));
    let p = serde_json::from_value(json!({"languageServers":{"global":["rust"]},"symbolWorkspaceFolders":{"global":[values[0].0]},"additionalWorkspaceFolders":{"global":[values[1].0]}})).unwrap();
    let mut s = open(&root, &log, p, "unicode-context");
    let value = s.engine.invoke("ci_find", json!({"languageServer":{"path":format!("{}/main.rs",values[0].0),"offset":values[0].1.find("unicode_first").unwrap(),"operation":"hover"}})).unwrap();
    assert_eq!(value["status"], "ok", "{value}");
    let metadata: Value = serde_json::from_str(
        value["data"]["response"]["contents"]["value"]
            .as_str()
            .unwrap(),
    )
    .unwrap();
    assert_eq!(metadata["folders"], json!([values[0].0, values[1].0]));
    assert_eq!(metadata["callbackVerified"], true);
    assert_eq!(value["data"]["contentHash"], yoke::model::hash(values[0].1));
    let found = symbols(&mut s);
    assert_eq!(found["items"].as_array().unwrap().len(), 1);
    assert_eq!(
        found["items"][0]["path"],
        format!("{}/main.rs", values[0].0)
    );
    assert_eq!(frozen(&mut s.engine), before);
    for (folder, text) in values {
        assert_eq!(
            fs::read(root.join(folder).join("main.rs")).unwrap(),
            text.as_bytes()
        );
    }
}

#[test]
fn absent_null_and_empty_additional_policies_keep_raw_bindings_source_and_graph() {
    let r = tempfile::tempdir().unwrap();
    let root = root(r.path(), "source");
    let log = r.path().join("log");
    let base = profile(Value::Null);
    let bytes = serde_json::to_vec(&base).unwrap();
    let mut old = open(&root, &log, base, "legacy");
    let config = old.invoke("configuration", &json!({}), &d()).unwrap();
    let hover = success(&mut old.engine, "hover");
    let graph = frozen(&mut old.engine);
    for extra in [
        Value::Null,
        json!({}),
        json!({"global":[],"projects":{"default":{"project":[],"local":null}}}),
    ] {
        let p = profile(extra);
        assert_eq!(serde_json::to_vec(&p).unwrap(), bytes);
        let mut s = open(&root, &log, p, "legacy");
        assert_eq!(s.invoke("configuration", &json!({}), &d()).unwrap(), config);
        assert_eq!(success(&mut s.engine, "hover"), hover);
        assert_eq!(frozen(&mut s.engine), graph);
        assert_eq!(success(&mut s.engine, "references")["locations"], json!([]));
    }
    assert_eq!(fs::read(root.join("app/main.rs")).unwrap(), APP.as_bytes());
    assert_eq!(
        fs::read(root.join("shared/\u{e9}.rs")).unwrap(),
        EXTRA.as_bytes()
    );
}

#[test]
fn actual_cross_package_locations_bind_extra_source_without_expanding_symbol_index() {
    let r = tempfile::tempdir().unwrap();
    let root = root(r.path(), "source");
    let log = r.path().join("log");
    let mut old = open(&root, &log, profile(Value::Null), "old");
    let before = frozen(&mut old.engine);
    assert_eq!(
        success(&mut old.engine, "references")["locations"],
        json!([])
    );
    let mut s = open(
        &root,
        &log,
        profile(json!({"global":["shared"]})),
        "selected",
    );
    let actual = info(&mut s.engine);
    assert_eq!(actual["folders"], json!(["app", "shared"]));
    assert_eq!(actual["callbackVerified"], true);
    assert_eq!(actual["fullCapturedExtraPresent"], true);
    let refs = success(&mut s.engine, "references");
    assert_eq!(refs["complete"], false);
    assert_eq!(refs["locations"].as_array().unwrap().len(), 1);
    let reference = &refs["locations"][0];
    assert_eq!(reference["path"], "shared/\u{e9}.rs");
    assert_eq!(
        reference["contentHash"],
        yoke::model::hash(EXTRA.as_bytes())
    );
    assert_eq!(reference["startByte"], EXTRA.find("shared").unwrap());
    assert_eq!(reference["endByte"], EXTRA.find("shared").unwrap() + 6);
    assert_eq!(reference["text"], "shared");
    assert_eq!(
        refs["additionalWorkspaceScope"]["sourceCaptureWidened"],
        false
    );
    assert_eq!(
        refs["additionalWorkspaceScope"]["symbolIndexPolicyUnchanged"],
        true
    );
    assert_eq!(refs["sourceExecutionAuthorized"], false);
    let found = symbols(&mut s);
    assert_eq!(
        found["items"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v["name"].as_str().unwrap())
            .collect::<Vec<_>>(),
        vec!["local"]
    );
    assert_eq!(frozen(&mut s.engine), before);
    assert_eq!(fs::read(root.join("app/main.rs")).unwrap(), APP.as_bytes());
    assert_eq!(
        fs::read(root.join("shared/\u{e9}.rs")).unwrap(),
        EXTRA.as_bytes()
    );
}

#[test]
fn project_local_empty_clear_null_inheritance_and_stale_state_remain_independent() {
    let r = tempfile::tempdir().unwrap();
    let primary = root(r.path(), "primary");
    let second = root(r.path(), "second");
    let log = r.path().join("log");
    let mut p = profile(
        json!({"global":["shared"],"projects":{"default":{"project":["other"],"local":[]}}}),
    );
    p.projects.push(HostProject {
        name: "second".into(),
        root: second,
        read_only: true,
        query_only: false,
    });
    let mut s = open(&primary, &log, p.clone(), "projects");
    let mut stale = open(&primary, &log, p.clone(), "projects");
    assert_eq!(info(&mut s.engine)["folders"], json!(["app"]));
    s.invoke("activate", &json!({"project":"second"}), &d())
        .unwrap();
    assert_eq!(
        info(s.active_engine().unwrap())["folders"],
        json!(["app", "shared"])
    );
    assert_eq!(
        success(s.active_engine().unwrap(), "references")["locations"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert!(
        agent_repl::invoke_facade(
            &mut stale,
            "lsp.find_symbol",
            json!({"name_path_pattern":"*"}),
            None,
            &d()
        )
        .is_err()
    );
    let mut changed = p.clone();
    changed
        .additional_workspace_folders
        .as_mut()
        .unwrap()
        .projects
        .get_mut("default")
        .unwrap()
        .local = None;
    assert!(
        Session::open(
            engine(&primary, &log),
            changed.clone(),
            Some("projects"),
            &d()
        )
        .is_err()
    );
    let mut inherited = open(&primary, &log, changed, "inherited");
    assert_eq!(
        info(&mut inherited.engine)["folders"],
        json!(["app", "other"])
    );
    assert_eq!(
        info(inherited.active_engine().unwrap())["folders"],
        json!(["app", "other"])
    );
    s.invoke("activate", &json!({"project":"default"}), &d())
        .unwrap();
    assert_eq!(info(&mut s.engine)["folders"], json!(["app"]));
}

#[test]
fn direct_staged_and_refresh_paths_share_extra_roots_and_root_relocation_refuses() {
    let r = tempfile::tempdir().unwrap();
    let root = root(r.path(), "source");
    let log = r.path().join("log");
    let mut s = open(&root, &log, profile(json!({"global":["shared"]})), "routes");
    let (snapshot, graph, _, _) = s.engine.index(&d()).unwrap();
    let provider = configured(&log);
    let direct = lsp::run_at_configured(
        &mut s.engine,
        &provider,
        &snapshot,
        &graph,
        lsp::SemanticRequest {
            path: "app/main.rs",
            offset: 0,
            operation: "hover",
            new_name: None,
        },
        &json!({}),
        &d(),
    )
    .unwrap();
    let value: Value =
        serde_json::from_str(direct["response"]["contents"]["value"].as_str().unwrap()).unwrap();
    assert_eq!(value["folders"], json!(["app", "shared"]));
    let staged = success(&mut s.engine, "codeActions");
    assert_eq!(staged["items"][0]["usable"], true);
    assert_eq!(
        staged["items"][0]["plan"]["files"][0]["path"],
        "app/main.rs"
    );
    assert_eq!(staged["serverProfileHash"], direct["serverProfileHash"]);
    let refreshed = lsp::restart_registered(&mut s.engine, &d()).unwrap();
    assert_eq!(refreshed["providers"].as_array().unwrap().len(), 1);
    let plans_before: i64 = s
        .engine
        .store
        .db
        .query_row(
            "SELECT count(*) FROM records WHERE kind='plan'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    let before = rows(&log).len();
    let result = yoke::semantic_refactor::invoke(
        &mut s.engine,
        "move",
        &json!({"relativePath":"shared","targetRelativePath":"moved"}),
        &d(),
    );
    let error = result.unwrap_err().to_string();
    assert!(
        error.contains("relocate a host-registered workspace root"),
        "{error}"
    );
    assert_eq!(rows(&log).len(), before);
    let plans_after: i64 = s
        .engine
        .store
        .db
        .query_row(
            "SELECT count(*) FROM records WHERE kind='plan'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(plans_after, plans_before);
    assert!(!root.join("moved").exists());
    assert_eq!(fs::read(root.join("app/main.rs")).unwrap(), APP.as_bytes());
}

#[test]
fn owned_move_intent_receives_extra_roots_and_retains_original_source() {
    let r = tempfile::tempdir().unwrap();
    let root = root(r.path(), "source");
    let log = r.path().join("owned.log");
    let before = "pub fn helper() -> u8 { 7 }\n";
    fs::write(root.join("app/source.rs"), before).unwrap();
    fs::write(
        root.join("app/destination.rs"),
        "pub mod bucket {\npub fn sentinel() {}\n}\n",
    )
    .unwrap();
    let mut e = Engine::open(&root).unwrap();
    e.allow_writes = true;
    let mut provider = configured(&log);
    provider.server.args = vec![
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/own-refactor-intent-lsp.mjs")
            .display()
            .to_string(),
        log.display().to_string(),
        "valid".into(),
    ];
    provider.configuration.refactor_intent_profile = Some(lsp::RefactorIntentProfile::OwnV1);
    e.register_backend("rust", provider).unwrap();
    let mut s = Session::open(
        e,
        profile(json!({"global":["shared"]})),
        Some("owned"),
        &d(),
    )
    .unwrap();
    let moved=agent_repl::invoke_facade(&mut s,"jb.move",json!({"relative_path":"app/source.rs","name_path":"helper","target_relative_path":"app/destination.rs"}),None,&d()).unwrap();
    assert!(moved["plan"].is_object());
    let events = rows(&log);
    let init = events
        .iter()
        .find(|row| row["method"] == "initialize")
        .unwrap();
    let folders = init["params"]["workspaceFolders"].as_array().unwrap();
    assert_eq!(folders.len(), 2);
    assert!(folders[0]["uri"].as_str().unwrap().ends_with("/app"));
    assert!(folders[1]["uri"].as_str().unwrap().ends_with("/shared"));
    assert!(
        events
            .iter()
            .any(|row| row["method"] == "yoke/refactorIntent")
    );
    assert_eq!(
        fs::read_to_string(root.join("app/source.rs")).unwrap(),
        before
    );
}

#[test]
fn schema_external_alias_capacity_and_uncaptured_roots_refuse_before_provider() {
    let r = tempfile::tempdir().unwrap();
    let root = root(r.path(), "source");
    let log = r.path().join("log");
    for extra in [
        json!({"global":["../outside"]}),
        json!({"global":["/outside"]}),
        json!({"global":["C:/outside"]}),
        json!({"global":["\\\\server\\share"]}),
        json!({"global":["shared","./shared"]}),
        json!({"global":vec!["shared";33]}),
        json!({"global":["x".repeat(65537)]}),
        json!({"projects":{"unregistered":{"local":["shared"]}}}),
    ] {
        assert!(Session::open(engine(&root, &log), profile(extra), Some("bad"), &d()).is_err());
        assert!(!log.exists());
    }
    for extra in [
        json!({"trust":true}),
        json!({"global":true}),
        json!({"global":[false]}),
    ] {
        assert!(
            serde_json::from_value::<HostProfile>(json!({"additionalWorkspaceFolders":extra}))
                .is_err()
        );
    }
    fs::create_dir(root.join("empty")).unwrap();
    for folder in ["missing", "empty", ".git", ".yoke"] {
        let p = profile(json!({"global":[folder]}));
        if let Ok(mut s) = Session::open(
            engine(&root, &log),
            p,
            Some(folder.trim_start_matches('.')),
            &d(),
        ) {
            let reply = response(&mut s.engine, "hover");
            assert_eq!(reply["status"], "error", "{reply}");
        }
        assert!(!log.exists());
    }
    fs::write(root.join(".gitignore"), "shared/\n").unwrap();
    let mut s = open(
        &root,
        &log,
        profile(json!({"global":["shared"]})),
        "ignored",
    );
    assert_eq!(response(&mut s.engine, "hover")["status"], "error");
    assert!(!log.exists());
    assert_eq!(
        fs::read(root.join("shared/\u{e9}.rs")).unwrap(),
        EXTRA.as_bytes()
    );
    let capacity_root = root.parent().unwrap().join("capacity");
    fs::create_dir(&capacity_root).unwrap();
    for index in 0..33 {
        fs::create_dir(capacity_root.join(format!("p{index}"))).unwrap();
        fs::write(
            capacity_root.join(format!("p{index}/main.rs")),
            format!("fn f{index}() {{}}\n"),
        )
        .unwrap();
    }
    fs::create_dir(capacity_root.join("app")).unwrap();
    fs::write(capacity_root.join("app/main.rs"), APP).unwrap();
    let mut p = profile(json!({"global":(16..33).map(|i|format!("p{i}")).collect::<Vec<_>>()}));
    p.symbol_workspace_folders.as_mut().unwrap().global =
        Some((0..16).map(|i| format!("p{i}")).collect());
    let mut s = open(&capacity_root, &log, p, "capacity");
    assert_eq!(response(&mut s.engine, "hover")["status"], "error");
    assert!(!log.exists());
}

#[test]
fn registered_roots_are_preserved_deduplicated_and_invalid_callbacks_remain_errors() {
    let r = tempfile::tempdir().unwrap();
    let root = root(r.path(), "source");
    let log = r.path().join("log");
    for (label, extra, expected) in [
        (
            "append",
            json!({"global":["other","shared"]}),
            json!(["other", "shared"]),
        ),
        ("empty", json!({"global":[]}), json!(["other"])),
    ] {
        let mut e = Engine::open(&root).unwrap();
        let mut provider = configured(&log);
        provider.configuration.workspace_folders = vec!["other".into()];
        e.register_backend("rust", provider).unwrap();
        let mut p = profile(extra);
        p.symbol_workspace_folders = None;
        let mut s = Session::open(e, p, Some(label), &d()).unwrap();
        assert_eq!(info(&mut s.engine)["folders"], expected);
    }
    let denied_log = r.path().join("denied.log");
    let mut e = Engine::open(&root).unwrap();
    let mut provider = configured(&denied_log);
    provider.server.args.push("bad-callback".into());
    e.register_backend("rust", provider).unwrap();
    let mut s = Session::open(
        e,
        profile(json!({"global":["shared"]})),
        Some("badcallback"),
        &d(),
    )
    .unwrap();
    let result = response(&mut s.engine, "references");
    assert_eq!(result["status"], "error");
    assert!(result.get("locations").is_none());
    assert!(
        rows(&denied_log)
            .iter()
            .any(|row| row["event"] == "initialize" && row["folders"] == json!(["app", "shared"]))
    );
    assert_eq!(fs::read(root.join("app/main.rs")).unwrap(), APP.as_bytes());
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
fn real_http_uses_host_extra_roots_and_rejects_request_forgery_and_revocation() {
    use yoke::{
        mcp_http::{self, Config, Principal, Target},
        protocol::Profile,
    };
    let r = tempfile::tempdir().unwrap();
    let root = root(r.path(), "source");
    let log = r.path().join("http.log");
    let mut target = Target::new(
        &root,
        Profile::Session,
        profile(json!({"global":["shared"]})),
    );
    let provider = configured(&log);
    target.host_servers.push(serde_json::from_value(json!({"backendId":"rust","command":provider.server.command,"args":provider.server.args,"configuration":provider.configuration})).unwrap());
    let principal = Principal::new(
        "owned-semantic-context",
        zeroize::Zeroizing::new(vec![73; 32]),
        target,
    )
    .unwrap();
    assert!(!log.exists());
    let server = mcp_http::serve(Config::default(), vec![principal]).unwrap();
    let address = server.local_addr();
    let init = json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"owned-semantic-context","version":"1"}}});
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
    let request = json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"ci_find","arguments":{"languageServer":{"path":"app/main.rs","offset":APP.find("shared").unwrap(),"operation":"references"}}}});
    let (status, _, reply) = http(address, Some(sid), true, request.clone());
    assert_eq!(status, 200);
    assert_eq!(reply["result"]["isError"], false, "{reply}");
    let value: Value =
        serde_json::from_str(reply["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(value["status"], "ok", "{value}");
    assert_eq!(
        value["data"]["locations"][0]["contentHash"],
        yoke::model::hash(EXTRA.as_bytes())
    );
    let before = rows(&log).len();
    let mut forged = request.clone();
    forged["id"] = json!(3);
    forged["params"]["arguments"]["additionalWorkspaceFolders"] = json!(["../outside"]);
    let denied = http(address, Some(sid), true, forged);
    assert!(
        denied.2.get("error").is_some() || denied.2["result"]["isError"] == true,
        "{}",
        denied.2
    );
    assert_eq!(rows(&log).len(), before);
    assert!(server.revoke("owned-semantic-context").unwrap());
    assert_eq!(http(address, Some(sid), true, request).0, 401);
    assert_eq!(rows(&log).len(), before);
    server.shutdown().unwrap();
    assert!(std::net::TcpStream::connect(address).is_err());
    assert_eq!(fs::read(root.join("app/main.rs")).unwrap(), APP.as_bytes());
}
