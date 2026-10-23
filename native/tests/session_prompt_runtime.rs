//! Actual opt-in live catalog rendering and pure trusted-host authority boundaries.
use serde_json::{Value, json};
use std::{
    fs,
    io::Write,
    path::PathBuf,
    process::{Command, Output, Stdio},
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};
use yoke::{
    capture::Deadline,
    project_session::{HostProfile, Session},
    session_prompt::{Config, Runtime},
};
struct Fixture {
    root: tempfile::TempDir,
    profile: PathBuf,
    source: PathBuf,
    original: Vec<u8>,
}
const SYSTEM: &str = "{{ {'tools': available_tools, 'markers': available_markers, 'names': tool_names, 'subject': subject}|tojson }}";
impl Fixture {
    fn new() -> Self {
        let root = tempfile::tempdir().unwrap();
        let profile = root.path().join("host.json");
        let source = root.path().join("main.rs");
        let original = "// owned e\u{301} \u{1f980}\r\nfn visible() {}\r\n"
            .as_bytes()
            .to_vec();
        fs::write(&source, &original).unwrap();
        fs::write(
            root.path().join(".env"),
            b"OWNED_PROTECTED_CANARY=not-readable\n",
        )
        .unwrap();
        let f = Self {
            root,
            profile,
            source,
            original,
        };
        f.set(json!({"context":"desktop-app","modes":["open"],"promptTemplates":{"context":{"variables":{"subject":"owned e\u{301} \u{1f980}"},"templates":{"system_prompt":SYSTEM}},"systemPrompt":"system_prompt"},"steering":{"modes":{"open":{"instructions":"OPEN {{ subject }}"},"closed":{"instructions":"CLOSED {{ subject }}","excludedTools":["ci_find"]}}}}));
        f
    }
    fn value(&self) -> Value {
        serde_json::from_slice(&fs::read(&self.profile).unwrap()).unwrap()
    }
    fn set(&self, value: Value) {
        fs::write(&self.profile, serde_json::to_vec(&value).unwrap()).unwrap();
    }
    fn command(&self, allowed: bool) -> Command {
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_yoke"));
        cmd.args(["--root"]).arg(self.root.path()).args([
            "--timeout",
            "30000",
            "--max-bytes",
            "4000000",
        ]);
        if allowed {
            cmd.arg("--allow-session-templates");
        }
        cmd
    }
    fn mcp(&self, profile: &str, requests: &[Value], allowed: bool) -> Output {
        let mut child = self
            .command(allowed)
            .args([
                "serve-session",
                "--profile",
                profile,
                "--id",
                profile,
                "--host-profile",
            ])
            .arg(&self.profile)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let input = requests
            .iter()
            .map(|v| format!("{v}\n"))
            .collect::<String>();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(input.as_bytes())
            .unwrap();
        child.wait_with_output().unwrap()
    }
    fn batch(&self, requests: Value, allowed: bool) -> Output {
        let path = self.root.path().join("requests.json");
        fs::write(&path, serde_json::to_vec(&requests).unwrap()).unwrap();
        self.command(allowed)
            .args(["session", "--id", "batch", "--host-profile"])
            .arg(&self.profile)
            .arg("--requests")
            .arg(path)
            .output()
            .unwrap()
    }
    fn unchanged(&self) {
        assert_eq!(fs::read(&self.source).unwrap(), self.original);
        assert_eq!(
            fs::read(self.root.path().join(".env")).unwrap(),
            b"OWNED_PROTECTED_CANARY=not-readable\n"
        );
        assert!(!self.root.path().join("PWNED").exists());
    }
}
fn message(id: u64, method: &str, params: Value) -> Value {
    json!({"jsonrpc":"2.0","id":id,"method":method,"params":params})
}
fn frames(out: Output) -> Vec<Value> {
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout)
        .unwrap()
        .lines()
        .map(|v| serde_json::from_str(v).unwrap())
        .collect()
}
fn frame(values: &[Value], id: u64) -> &Value {
    values.iter().find(|v| v["id"] == id).unwrap()
}
fn data(frame: &Value) -> Value {
    assert_eq!(frame["result"]["isError"], false, "{frame}");
    let text = frame["result"]["content"][0]["text"].as_str().unwrap();
    let envelope: Value = serde_json::from_str(text).unwrap();
    assert_eq!(envelope["status"], "ok");
    envelope["data"].clone()
}
fn root_data(instructions: &str) -> Value {
    let start = instructions.find('{').unwrap();
    let end = instructions[start..]
        .find("\n\n[host steering:")
        .map_or(instructions.len(), |n| start + n);
    serde_json::from_str(&instructions[start..end]).unwrap()
}
fn tool_names(frame: &Value) -> Vec<String> {
    let mut names = frame["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v["name"].as_str().unwrap().to_owned())
        .collect::<Vec<_>>();
    names.sort();
    names
}
fn refuses(out: Output, reason: &str) {
    assert!(!out.status.success());
    assert!(out.stdout.is_empty());
    assert!(
        String::from_utf8_lossy(&out.stderr).contains(reason),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
}
fn worker(input: Value) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_yoke"))
        .args(["session-config", "render-worker"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(&serde_json::to_vec(&input).unwrap())
        .unwrap();
    child.wait_with_output().unwrap()
}
#[test]
fn live_native_mode_changes_share_the_exact_published_catalog_and_revision_binding() {
    let f = Fixture::new();
    let requests = [
        message(1, "initialize", json!({})),
        message(2, "tools/list", json!({})),
        message(
            3,
            "tools/call",
            json!({"name":"ci_session","arguments":{"operation":"initialInstructions","args":{}}}),
        ),
        message(
            4,
            "tools/call",
            json!({"name":"ci_session","arguments":{"operation":"switchModes","args":{"modes":["closed"]}}}),
        ),
        message(5, "tools/list", json!({})),
        message(
            6,
            "tools/call",
            json!({"name":"ci_session","arguments":{"operation":"initialInstructions","args":{}}}),
        ),
    ];
    let values = frames(f.mcp("native", &requests, true));
    let before = data(frame(&values, 3));
    let after = data(frame(&values, 6));
    let a = root_data(before["instructions"].as_str().unwrap());
    let b = root_data(after["instructions"].as_str().unwrap());
    assert_eq!(a["tools"], json!(tool_names(frame(&values, 2))));
    assert_eq!(b["tools"], json!(tool_names(frame(&values, 5))));
    assert!(a["tools"].as_array().unwrap().contains(&json!("ci_find")));
    assert!(!b["tools"].as_array().unwrap().contains(&json!("ci_find")));
    assert_eq!(a["subject"], "owned e\u{301} \u{1f980}");
    assert_ne!(
        before["promptRendering"]["binding"]["revision"],
        after["promptRendering"]["binding"]["revision"]
    );
    assert_ne!(
        before["promptRendering"]["binding"]["catalog"]["selectedCatalogSha256"],
        after["promptRendering"]["binding"]["catalog"]["selectedCatalogSha256"]
    );
    assert!(
        values
            .iter()
            .any(|v| v["method"] == "notifications/tools/list_changed")
    );
    for item in before["promptRendering"]["fragments"].as_array().unwrap() {
        assert_eq!(
            item["rendering"]["schema"],
            "native_session_prompt_jinja_v3"
        );
        assert_eq!(item["rendering"]["memoryCeilingBytes"], 256 * 1024 * 1024);
        assert!(
            item["rendering"]["runtimeBindingSha256"]
                .as_str()
                .unwrap()
                .len()
                == 64
        );
    }
    f.unchanged();
}
#[test]
fn live_serena_fixed_optional_selection_derives_real_names_and_inherited_marker_api() {
    let f = Fixture::new();
    let mut host = f.value();
    host["optionalDefaults"] = json!("serena_optional_v1");
    host["steering"]["global"] = json!({"fixedTools":["find_symbol","initial_instructions"]});
    f.set(host);
    let values = frames(f.mcp(
        "serena",
        &[
            message(1, "initialize", json!({})),
            message(2, "tools/list", json!({})),
            message(
                3,
                "tools/call",
                json!({"name":"initial_instructions","arguments":{}}),
            ),
        ],
        true,
    ));
    let v = data(frame(&values, 3));
    let root = root_data(v["instructions"].as_str().unwrap());
    assert_eq!(
        root["tools"],
        json!(["find_symbol", "initial_instructions"])
    );
    assert_eq!(root["tools"], json!(tool_names(frame(&values, 2))));
    assert_eq!(
        root["markers"],
        json!([
            "ToolMarkerDoesNotRequireActiveProject",
            "ToolMarkerSymbolicRead"
        ])
    );
    assert_eq!(root["names"]["find_symbol"], "find_symbol");
    assert_eq!(
        root_data(
            frame(&values, 1)["result"]["instructions"]
                .as_str()
                .unwrap()
        ),
        root
    );
    f.unchanged();
}
#[test]
fn live_trail_graft_and_graphify_initialization_use_their_actual_transport_names() {
    for profile in ["trail", "graft", "graphify"] {
        let f = Fixture::new();
        let values = frames(f.mcp(
            profile,
            &[
                message(1, "initialize", json!({})),
                message(2, "tools/list", json!({})),
            ],
            true,
        ));
        let root = root_data(
            frame(&values, 1)["result"]["instructions"]
                .as_str()
                .unwrap(),
        );
        assert_eq!(root["tools"], json!(tool_names(frame(&values, 2))));
        if profile != "graphify" {
            assert_eq!(root["names"]["find_code"], format!("{profile}_find_code"));
        }
        assert_eq!(
            frame(&values, 1)["result"]["promptRendering"]["binding"]["catalog"]["profile"],
            profile
        );
        f.unchanged();
    }
}
#[test]
fn actual_project_activation_recomputes_project_modes_and_rendered_guidance() {
    let f = Fixture::new();
    let other = tempfile::tempdir().unwrap();
    fs::write(other.path().join("other.rs"), b"fn other() {}\n").unwrap();
    let mut host = f.value();
    host["projects"] =
        json!([{"name":"other","root":other.path(),"readOnly":true,"queryOnly":false}]);
    host["steering"]["projects"] = json!({"default":{"project":{"defaultModes":["open"],"instructions":"PROJECT-A {{ subject }}"}},"other":{"project":{"defaultModes":["closed"],"instructions":"PROJECT-B {{ subject }}"}}});
    f.set(host);
    let out=f.batch(json!([{"operation":"initialInstructions","args":{}},{"operation":"activate","args":{"project":"other"}},{"operation":"initialInstructions","args":{}}]),true);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    let a = &v["results"][0];
    let b = &v["results"][2];
    assert!(
        a["instructions"]
            .as_str()
            .unwrap()
            .contains("PROJECT-A owned")
    );
    assert!(!b["instructions"].as_str().unwrap().contains("PROJECT-A"));
    assert!(
        b["instructions"]
            .as_str()
            .unwrap()
            .contains("PROJECT-B owned")
    );
    assert_eq!(b["promptRendering"]["binding"]["activeProject"], "other");
    assert!(
        root_data(a["instructions"].as_str().unwrap())["tools"]
            .as_array()
            .unwrap()
            .contains(&json!("ci_find"))
    );
    assert!(
        !root_data(b["instructions"].as_str().unwrap())["tools"]
            .as_array()
            .unwrap()
            .contains(&json!("ci_find"))
    );
    f.unchanged();
    assert_eq!(
        fs::read(other.path().join("other.rs")).unwrap(),
        b"fn other() {}\n"
    );
}
#[test]
fn host_permission_precedes_missing_catalog_reads_and_json_cannot_supply_a_renderer() {
    let f = Fixture::new();
    let mut host = f.value();
    host["promptTemplates"]["context"]["catalogs"] =
        json!([{"path":f.root.path().join("absent.yml"),"sha256":"0".repeat(64)}]);
    f.set(host.clone());
    refuses(
        f.mcp("native", &[], false),
        "session_prompt_host_permission_required",
    );
    host["promptRuntime"] = json!({"trusted":true});
    assert!(serde_json::from_value::<HostProfile>(host).is_err());
    f.unchanged();
}
#[test]
fn reserved_runtime_values_stale_pins_and_duplicate_catalog_handles_refuse_before_render() {
    let f = Fixture::new();
    for name in yoke::session_prompt::RUNTIME_VARIABLES {
        let mut host = f.value();
        host["promptTemplates"]["context"]["variables"][*name] = json!(["forged"]);
        f.set(host);
        refuses(
            f.mcp("native", &[], true),
            "session_prompt_reserved_runtime_variable",
        );
        let mut reset = f.value();
        reset["promptTemplates"]["context"]["variables"]
            .as_object_mut()
            .unwrap()
            .remove(*name);
        f.set(reset);
    }
    let source = f.root.path().join("cat.yml");
    let raw = b"prompts: {entry: owned}\n";
    fs::write(&source, raw).unwrap();
    let mut host = f.value();
    host["promptTemplates"]["context"]["catalogs"] =
        json!([{"path":source,"sha256":"0".repeat(64)}]);
    f.set(host.clone());
    refuses(
        f.mcp("native", &[], true),
        "session_config_catalog_source_changed",
    );
    let item = json!({"path":source,"sha256":yoke::model::hash(raw)});
    host["promptTemplates"]["context"]["catalogs"] = json!([item.clone(), item]);
    host["promptTemplates"]["context"]["collisionPolicy"] = json!("last_wins");
    f.set(host);
    refuses(
        f.mcp("native", &[], true),
        "session_config_catalog_source_alias",
    );
    f.unchanged();
}
#[test]
fn failed_runtime_render_does_not_initialize_or_publish_partial_guidance() {
    for text in [
        "{{ missing }}",
        "{{ 'x' * 9000 }}",
        "{{ env.PATH }}",
        "{% include 'C:/Windows/win.ini' %}",
    ] {
        let f = Fixture::new();
        let mut host = f.value();
        host["promptTemplates"]["context"]["templates"]["system_prompt"] = json!(text);
        f.set(host);
        let values = frames(f.mcp(
            "native",
            &[
                message(1, "initialize", json!({})),
                message(2, "tools/list", json!({})),
            ],
            true,
        ));
        assert_eq!(frame(&values, 1)["error"]["code"], -32001);
        assert_eq!(frame(&values, 2)["error"]["code"], -32002);
        assert!(frame(&values, 1).get("result").is_none());
        f.unchanged();
    }
}
#[test]
fn worker_three_binds_inert_runtime_data_and_refuses_version_mix_and_named_text_forgery() {
    let binding = json!({"schema":"native_session_prompt_runtime_v1","source":"owned"});
    let req = json!({"schema":3,"context":{"variables":{"x":"owned"}},"prompt":"{{ x }}","runtimeBinding":binding});
    let out = worker(req);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["schema"], "native_session_prompt_jinja_v3");
    assert_eq!(v["rendered"], "owned");
    assert_eq!(
        v["runtimeBindingSha256"],
        yoke::model::hash(serde_json::to_vec(&binding).unwrap())
    );
    for req in [
        json!({"schema":1,"context":{},"prompt":"x","runtimeBinding":null}),
        json!({"schema":2,"context":{},"prompt":"x","runtimeBinding":{}}),
        json!({"schema":3,"context":{},"prompt":"x"}),
        json!({"schema":3,"context":{},"prompt":"x","runtimeBinding":null}),
        json!({"schema":3,"context":{"templates":{"entry":"owned"}},"prompt":"forged","templateName":"entry","runtimeBinding":{}}),
        json!({"schema":3,"context":{"catalogs":[]},"prompt":"x","runtimeBinding":{}}),
    ] {
        let out = worker(req);
        assert!(!out.status.success());
        assert!(out.stdout.is_empty());
    }
}
#[test]
fn raw_library_profiles_need_typed_host_authority_and_stale_sessions_do_not_call_it() {
    let f = Fixture::new();
    let config = Config::default();
    let mut host: HostProfile = serde_json::from_value(
        json!({"context":"desktop-app","steering":{"global":{"instructions":"owned"}}}),
    )
    .unwrap();
    host.prompt_templates = Some(config.clone());
    let d = Deadline::new(20000);
    let denied = Session::open(
        yoke::Engine::open(f.root.path()).unwrap(),
        host.clone(),
        Some("authority"),
        &d,
    );
    assert!(
        denied
            .err()
            .unwrap()
            .to_string()
            .contains("session_prompt_host_permission_required")
    );
    let calls = Arc::new(AtomicUsize::new(0));
    let seen = calls.clone();
    host.prompt_runtime = Some(
        Runtime::trusted(
            &config,
            Arc::new(move |_, _| {
                seen.fetch_add(1, Ordering::SeqCst);
                Ok(yoke::session_config::RenderedPrompt {
                    text: "owned".into(),
                    evidence: Value::Null,
                })
            }),
            &d,
        )
        .unwrap(),
    );
    let mut first = Session::open(
        yoke::Engine::open(f.root.path()).unwrap(),
        host.clone(),
        Some("authority"),
        &d,
    )
    .unwrap();
    let stale = Session::open(
        yoke::Engine::open(f.root.path()).unwrap(),
        host.clone(),
        Some("authority"),
        &d,
    )
    .unwrap();
    first
        .invoke("switchModes", &json!({"modes":["planning"]}), &d)
        .unwrap();
    assert!(stale.prompt_instructions(&d).is_err());
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    first.prompt_instructions(&d).unwrap();
    assert!(calls.load(Ordering::SeqCst) > 0);
    let mut config = config;
    config
        .context
        .variables
        .insert("changed".into(), json!(true));
    host.prompt_templates = Some(config.clone());
    host.prompt_runtime = Some(
        Runtime::trusted(
            &config,
            Arc::new(|_, _| {
                Ok(yoke::session_config::RenderedPrompt {
                    text: "owned".into(),
                    evidence: Value::Null,
                })
            }),
            &d,
        )
        .unwrap(),
    );
    assert!(
        Session::open(
            yoke::Engine::open(f.root.path()).unwrap(),
            host,
            Some("authority"),
            &d
        )
        .is_err()
    );
    f.unchanged();
}
#[test]
fn http_native_target_and_ungranted_session_target_fail_before_source_state_opens() {
    let f = Fixture::new();
    let config = Config::default();
    let d = Deadline::new(20000);
    let mut host = HostProfile {
        prompt_templates: Some(config.clone()),
        ..Default::default()
    };
    let target = yoke::mcp_http::Target::new(
        f.root.path(),
        yoke::protocol::Profile::Session,
        host.clone(),
    );
    assert!(
        yoke::mcp_http::Principal::new("owned", zeroize::Zeroizing::new(vec![19; 32]), target)
            .is_err()
    );
    host.prompt_runtime = Some(
        Runtime::trusted(
            &config,
            Arc::new(|_, _| {
                Ok(yoke::session_config::RenderedPrompt {
                    text: "owned".into(),
                    evidence: Value::Null,
                })
            }),
            &d,
        )
        .unwrap(),
    );
    let target = yoke::mcp_http::Target::new(f.root.path(), yoke::protocol::Profile::Native, host);
    assert!(
        yoke::mcp_http::Principal::new("owned", zeroize::Zeroizing::new(vec![19; 32]), target)
            .is_err()
    );
    assert!(!f.root.path().join(".yoke").exists());
    f.unchanged();
}
#[test]
fn deferred_yaml_export_and_discovery_preserve_raw_prompts_and_require_runtime_authority() {
    let f = Fixture::new();
    let mut host = f.value();
    host.as_object_mut().unwrap().remove("promptTemplates");
    let mode = f.root.path().join("open.yml");
    let prompt = "{% if 'ci_find' in available_tools %}HAS-READ{% else %}NO-READ{% endif %}";
    let raw = format!("prompt: {}\r\n", serde_json::to_string(prompt).unwrap());
    fs::write(&mode, &raw).unwrap();
    let selection = f.root.path().join("selection.json");
    let ctx = f.value()["promptTemplates"]["context"].clone();
    fs::write(&selection,serde_json::to_vec(&json!({"hostProfile":host,"files":[{"path":mode,"sha256":yoke::model::hash(&raw),"target":{"kind":"mode","name":"open"}}],"templateContext":ctx,"systemPrompt":"system_prompt"})).unwrap()).unwrap();
    let run = |op: &str, extra: &[&str]| {
        Command::new(env!("CARGO_BIN_EXE_yoke"))
            .args([
                "--max-bytes",
                "1000000",
                "session-config",
                op,
                "--selection",
            ])
            .arg(&selection)
            .args([
                "--allow-configuration-read",
                "--allow-policy-projection",
                "--allow-template-evaluation",
                "--defer-template-evaluation",
            ])
            .args(extra)
            .output()
            .unwrap()
    };
    let out = run("plan", &[]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let plan: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(plan["templatesEvaluated"], false);
    assert_eq!(plan["templatesDeferred"], true);
    assert_eq!(
        plan["hostProfile"]["steering"]["modes"]["open"]["instructions"],
        prompt
    );
    assert_eq!(
        plan["sources"][0]["promptDeferred"]["originalPromptSha256"],
        yoke::model::hash(prompt)
    );
    let output = f.root.path().join("export.json");
    let out = run(
        "export",
        &["--output", output.to_str().unwrap(), "--allow-output"],
    );
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let exported: Value = serde_json::from_slice(&fs::read(&output).unwrap()).unwrap();
    assert_eq!(exported, plan["hostProfile"]);
    assert!(!f.root.path().join(".yoke").exists());
    let out = run("discover", &[]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let discovered: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(discovered["projection"], plan);
    assert_eq!(discovered["selection"]["systemPrompt"], "system_prompt");
    f.set(exported);
    refuses(
        f.batch(
            json!([{"operation":"initialInstructions","args":{}}]),
            false,
        ),
        "session_prompt_host_permission_required",
    );
    let out = f.batch(json!([{"operation":"initialInstructions","args":{}}]), true);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert!(
        v["results"][0]["instructions"]
            .as_str()
            .unwrap()
            .contains("HAS-READ")
    );
    assert_eq!(fs::read(&mode).unwrap(), raw.as_bytes());
    f.unchanged();
}
