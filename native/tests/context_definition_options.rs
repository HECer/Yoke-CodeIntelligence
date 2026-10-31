use serde_json::{Value, json};
use std::{
    fs,
    io::{Read, Write},
    net::SocketAddr,
};
use yoke::{
    Engine,
    capture::Deadline,
    project_session::{HostProfile, Session},
    session_config::{self, Source, Target},
};

const SOURCE: &str = "// owned \u{1f4a1} e\u{301}\r\nfn visible() {}\r\n";

fn profile(output: Option<Value>, single: bool, startup: bool) -> HostProfile {
    let mut value = json!({"context":"owned", "steering":{"contexts":{"owned":{
        "description":"owned context e\u{301}", "instructions":"owned instructions",
        "singleProject":single, "excludedTools":["execute_shell_command"],
        "toolDescriptionOverrides":{"read_file":"Read owned \u{1f4a1} {{ literal }}", "execute_shell_command":"Still excluded"}
    }}}});
    if let Some(output) = output {
        value["steering"]["contexts"]["owned"]["structuredToolOutput"] = output;
    }
    if startup {
        value["startupProject"] = json!("default");
    }
    serde_json::from_value(value).unwrap()
}

fn setup(host: HostProfile, id: &str, d: &Deadline) -> (tempfile::TempDir, Session) {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("owned \u{3c0} e\u{301}");
    fs::create_dir(&root).unwrap();
    fs::write(root.join("main.rs"), SOURCE.as_bytes()).unwrap();
    let session = Session::open(Engine::open(&root).unwrap(), host, Some(id), d).unwrap();
    (temp, session)
}

fn wire(session: &mut Session, messages: Vec<Value>) -> Vec<Value> {
    let input: String = messages.iter().map(|v| v.to_string() + "\n").collect();
    let mut output = Vec::new();
    yoke::protocol::serve_serena(session, std::io::Cursor::new(input), &mut output).unwrap();
    String::from_utf8(output)
        .unwrap()
        .lines()
        .map(|s| serde_json::from_str(s).unwrap())
        .collect()
}

#[test]
fn complete_context_and_mode_yaml_keep_literal_metadata_and_null_prompts_without_rendering() {
    let d = Deadline::new(20000);
    let base: HostProfile = serde_json::from_value(
        json!({"context":"owned", "startupProject":"default", "steering":{}}),
    )
    .unwrap();
    let context = "description: 'owned {{ literal }}'\nprompt: null\nexcluded_tools: []\nincluded_optional_tools: []\nfixed_tools: []\nexcluded_apis: []\nincluded_apis: []\ntool_description_overrides: {read_file: 'Read owned source'}\nsingle_project: true\nstructured_tool_output: null\n";
    let mode = "description: 'mode metadata'\nprompt: null\nexcluded_tools: []\nincluded_optional_tools: []\nfixed_tools: []\nexcluded_apis: []\nincluded_apis: []\n";
    let targets = [
        Target::Context {
            name: "owned".into(),
        },
        Target::Mode {
            name: "owned-mode".into(),
        },
    ];
    let hashes = [yoke::model::hash(context), yoke::model::hash(mode)];
    let sources = [
        Source {
            target: &targets[0],
            expected_sha256: &hashes[0],
            bytes: context.as_bytes(),
        },
        Source {
            target: &targets[1],
            expected_sha256: &hashes[1],
            bytes: mode.as_bytes(),
        },
    ];
    let mut calls = 0;
    let projected = {
        let mut renderer = |_: &str, _: &Deadline| {
            calls += 1;
            anyhow::bail!("Null prompts must not invoke a renderer")
        };
        session_config::project_with_prompt_renderer(&base, &sources, &d, &mut renderer).unwrap()
    };
    assert_eq!(calls, 0);
    let value: Value = serde_json::from_slice(&projected.profile_bytes(&d).unwrap()).unwrap();
    let definition = &value["steering"]["contexts"]["owned"];
    assert_eq!(definition["description"], "owned {{ literal }}");
    assert_eq!(definition["instructions"], "");
    assert_eq!(definition["singleProject"], true);
    assert!(
        definition
            .as_object()
            .unwrap()
            .contains_key("structuredToolOutput")
    );
    assert!(definition["structuredToolOutput"].is_null());
    assert_eq!(
        value["steering"]["modes"]["owned-mode"]["description"],
        "mode metadata"
    );
    assert_eq!(projected.plan()["templatesEvaluated"], false);
    assert_eq!(projected.plan()["sourceExecuted"], false);
    assert_eq!(yoke::model::hash(context), hashes[0]);
    assert_eq!(yoke::model::hash(mode), hashes[1]);
}

#[test]
fn invalid_scope_types_tools_and_capacities_refuse_before_any_prompt_renderer() {
    let d = Deadline::new(20000);
    let base: HostProfile =
        serde_json::from_value(json!({"context":"owned", "steering":{}})).unwrap();
    let cases: [(Target, String); 8] = [
        (
            Target::Context {
                name: "owned".into(),
            },
            "prompt: '{{ x }}'\ntool_description_overrides: {foreign_tool: text}\n".into(),
        ),
        (
            Target::Context {
                name: "owned".into(),
            },
            "prompt: '{{ x }}'\nsingle_project: 1\n".into(),
        ),
        (
            Target::Context {
                name: "owned".into(),
            },
            "prompt: '{{ x }}'\nstructured_tool_output: []\n".into(),
        ),
        (
            Target::Context {
                name: "owned".into(),
            },
            format!(
                "prompt: '{{{{ x }}}}'\ndescription: '{}'\n",
                "x".repeat(8193)
            ),
        ),
        (
            Target::Mode {
                name: "owned-mode".into(),
            },
            "prompt: '{{ x }}'\nstructured_tool_output: false\n".into(),
        ),
        (
            Target::Mode {
                name: "owned-mode".into(),
            },
            "prompt: '{{ x }}'\nsingle_project: false\n".into(),
        ),
        (Target::Global, "single_project: true\n".into()),
        (
            Target::Project {
                project: "default".into(),
            },
            "tool_description_overrides: {}\n".into(),
        ),
    ];
    for (target, text) in cases {
        let hash = yoke::model::hash(&text);
        let mut calls = 0;
        {
            let mut renderer = |_: &str, _: &Deadline| {
                calls += 1;
                Ok(session_config::RenderedPrompt {
                    text: "rendered".into(),
                    evidence: json!({}),
                })
            };
            assert!(
                session_config::project_with_prompt_renderer(
                    &base,
                    &[Source {
                        target: &target,
                        expected_sha256: &hash,
                        bytes: text.as_bytes()
                    }],
                    &d,
                    &mut renderer
                )
                .is_err()
            );
        }
        assert_eq!(calls, 0, "{text}");
    }
}

#[test]
fn selected_descriptions_do_not_change_schemas_or_reintroduce_excluded_tools() {
    let d = Deadline::new(20000);
    let (_temp, mut session) = setup(profile(Some(json!(false)), false, false), "catalog", &d);
    let rows = wire(
        &mut session,
        vec![
            json!({"jsonrpc":"2.0","id":1,"method":"initialize"}),
            json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}),
        ],
    );
    let tools = rows[1]["result"]["tools"].as_array().unwrap();
    assert!(!tools.iter().any(|v| v["name"] == "execute_shell_command"));
    let baseline = yoke::compatibility::serena_tools();
    for tool in tools {
        let old = baseline
            .as_array()
            .unwrap()
            .iter()
            .find(|v| v["name"] == tool["name"])
            .unwrap();
        assert_eq!(tool["inputSchema"], old["inputSchema"]);
        assert!(tool.get("outputSchema").is_none());
        if tool["name"] == "read_file" {
            assert_eq!(tool["description"], "Read owned \u{1f4a1} {{ literal }}");
        } else {
            assert_eq!(tool["description"], old["description"]);
        }
    }
    assert_eq!(
        fs::read(session.engine.root.join("main.rs")).unwrap(),
        SOURCE.as_bytes()
    );
}

#[test]
fn explicit_always_never_and_auto_structured_serena_results_preserve_text_and_errors() {
    let d = Deadline::new(20000);
    for (output, structured) in [
        (None, false),
        (Some(json!(false)), false),
        (Some(json!(true)), true),
        (Some(Value::Null), true),
    ] {
        let (_temp, mut session) = setup(profile(output, false, false), "wire", &d);
        let rows = wire(
            &mut session,
            vec![
                json!({"jsonrpc":"2.0","id":1,"method":"initialize"}),
                json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}),
                json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"get_current_config","arguments":{}}}),
                json!({"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"execute_shell_command","arguments":{"command":"must not run"}}}),
            ],
        );
        assert_eq!(rows.len(), 4);
        for tool in rows[1]["result"]["tools"].as_array().unwrap() {
            assert_eq!(tool.get("outputSchema").is_some(), structured);
        }
        assert_eq!(rows[2]["result"]["isError"], false);
        assert_eq!(rows[3]["result"]["isError"], true);
        for row in &rows[2..] {
            let result = &row["result"];
            let text: Value =
                serde_json::from_str(result["content"][0]["text"].as_str().unwrap()).unwrap();
            assert_eq!(result.get("structuredContent").is_some(), structured);
            if structured {
                assert_eq!(result["structuredContent"], text);
            }
        }
        assert_eq!(
            fs::read(session.engine.root.join("main.rs")).unwrap(),
            SOURCE.as_bytes()
        );
    }
}

#[test]
fn custom_single_project_uses_only_the_host_startup_and_existing_resume_guards() {
    let d = Deadline::new(20000);
    let host = profile(Some(json!(false)), true, true);
    let (_temp, mut session) = setup(host.clone(), "fixed", &d);
    assert_eq!(
        session.invoke("configuration", &json!({}), &d).unwrap()["singleProject"],
        true
    );
    assert!(
        session
            .invoke("activate", &json!({"project":"default"}), &d)
            .is_err()
    );
    assert!(
        yoke::compatibility::invoke_serena(
            &mut session,
            "activate_project",
            &json!({"project":"default","session":"fixed"}),
            &d
        )
        .is_err()
    );
    let root = session.engine.root.clone();
    drop(session);
    let mut changed = serde_json::to_value(&host).unwrap();
    changed["steering"]["contexts"]["owned"]["singleProject"] = json!(false);
    let changed: HostProfile = serde_json::from_value(changed).unwrap();
    assert!(Session::open(Engine::open(&root).unwrap(), changed, Some("fixed"), &d).is_err());
    let (_other, mut unbound) = setup(profile(None, true, false), "unbound", &d);
    assert_eq!(
        unbound
            .invoke("activate", &json!({"project":"default"}), &d)
            .unwrap()["activeProject"],
        "default"
    );
    assert_eq!(fs::read(root.join("main.rs")).unwrap(), SOURCE.as_bytes());
}

fn post(address: SocketAddr, session: Option<&str>, body: Value) -> (u16, Option<String>, Value) {
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
fn real_http_serena_uses_the_same_selected_structured_context_and_source_scope() {
    use yoke::mcp_http::{self, Config, Principal, Target};
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("main.rs"), SOURCE.as_bytes()).unwrap();
    let target = Target::new(
        root.path(),
        yoke::protocol::Profile::Serena,
        profile(Some(Value::Null), false, false),
    );
    let principal = Principal::new(
        "owned-context",
        zeroize::Zeroizing::new(vec![51; 32]),
        target,
    )
    .unwrap();
    let server = mcp_http::serve(Config::default(), vec![principal]).unwrap();
    let address = server.local_addr();
    let (status, id, _) = post(
        address,
        None,
        json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"owned-context","version":"1"}}}),
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
    let (status, _, result) = post(
        address,
        Some(&id),
        json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"get_current_config","arguments":{}}}),
    );
    assert_eq!(status, 200);
    assert_eq!(result["result"]["isError"], false);
    let text: Value =
        serde_json::from_str(result["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(result["result"]["structuredContent"], text);
    server.shutdown().unwrap();
    assert_eq!(
        fs::read(root.path().join("main.rs")).unwrap(),
        SOURCE.as_bytes()
    );
}
