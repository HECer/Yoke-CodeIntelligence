//! Owned note/Session/transport fixtures; no human identity or semantic truth claim.
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    fs,
    io::{Read, Write},
    net::TcpStream,
    process::{Command, Stdio},
    sync::Arc,
    time::{Duration, Instant},
};
use yoke::{
    Engine,
    capture::Deadline,
    project_session::{HostProfile, Session},
    required_prompt_memories::{Pin, Policy, Runtime as Confirmation},
    session_prompt::{Config, Runtime},
};

const NAME: &str = "guide e\u{301}";
const BODY: &str = "owned 🦀\r\n{{ env('NOT_EVALUATED') }}";
const SOURCE: &str = "// original α\r\nfn original(){}\r\n";
fn d() -> Deadline {
    Deadline::new(30000)
}
fn renderer(config: &Config) -> Runtime {
    Runtime::trusted(
        config,
        Arc::new(|_, _| {
            Ok(yoke::session_config::RenderedPrompt {
                text: "BASE".into(),
                evidence: json!({}),
            })
        }),
        &d(),
    )
    .unwrap()
}
fn pin(name: &str, revision: &Value) -> Pin {
    let field = |key: &str| revision[key].as_str().unwrap().to_owned();
    Pin {
        name: name.into(),
        revision_id: field("revisionId"),
        content_sha256: field("contentSha256"),
        storage_workspace_id: field("storageWorkspaceId"),
        source_workspace_id: field("sourceWorkspaceId"),
        snapshot_id: field("snapshotId"),
    }
}
struct Fixture {
    root: tempfile::TempDir,
    host_files: tempfile::TempDir,
    host: HostProfile,
}
impl Fixture {
    fn new() -> Self {
        let root = tempfile::tempdir().unwrap();
        let host_files = tempfile::tempdir().unwrap();
        fs::write(root.path().join("main.rs"), SOURCE).unwrap();
        let mut seed = Session::open(
            Engine::open(root.path()).unwrap(),
            HostProfile::default(),
            Some("seed"),
            &d(),
        )
        .unwrap();
        yoke::agent_repl::invoke_facade(
            &mut seed,
            "mem.write_memory",
            json!({"memory_name":NAME,"content":BODY}),
            None,
            &d(),
        )
        .unwrap();
        drop(seed);
        let config: Config = serde_json::from_value(json!({
            "context":{"templates":{"root":"BASE"}},"systemPrompt":"root","memoryEmbeddings":[NAME]
        }))
        .unwrap();
        let mut host = HostProfile {
            prompt_runtime: Some(renderer(&config)),
            prompt_templates: Some(config),
            ..Default::default()
        };
        let selected = Session::open(
            Engine::open(root.path()).unwrap(),
            host.clone(),
            Some("pins"),
            &d(),
        )
        .unwrap();
        let instructions = selected.prompt_instructions(&d()).unwrap();
        let revision = &instructions.evidence.as_ref().unwrap()["binding"]["memorySnapshot"]["entries"]
            [NAME]["revision"];
        let field = |key: &str| revision[key].as_str().unwrap().to_owned();
        let pin = Pin {
            name: NAME.into(),
            revision_id: field("revisionId"),
            content_sha256: field("contentSha256"),
            storage_workspace_id: field("storageWorkspaceId"),
            source_workspace_id: field("sourceWorkspaceId"),
            snapshot_id: field("snapshotId"),
        };
        drop(selected);
        host.required_prompt_memories = Some(Policy {
            projects: BTreeMap::from([("default".into(), vec![pin])]),
            ..Default::default()
        });
        host.required_prompt_memory_runtime = Some(Confirmation::confirm(&host, &d()).unwrap());
        Self {
            root,
            host_files,
            host,
        }
    }
    fn open(&self, host: HostProfile, label: &str) -> anyhow::Result<Session> {
        Session::open(Engine::open(self.root.path())?, host, Some(label), &d())
    }
    fn change(&self, content: &str) {
        let mut writer = self.open(HostProfile::default(), "writer").unwrap();
        yoke::agent_repl::invoke_facade(
            &mut writer,
            "mem.write_memory",
            json!({"memory_name":NAME,"content":content}),
            None,
            &d(),
        )
        .unwrap();
    }
    fn profile(&self) -> std::path::PathBuf {
        let path = self.host_files.path().join("profile.json");
        fs::write(&path, serde_json::to_vec(&self.host).unwrap()).unwrap();
        path
    }
    fn command(&self) -> Command {
        let mut c = Command::new(env!("CARGO_BIN_EXE_yoke"));
        c.arg("--root").arg(self.root.path()).args([
            "--timeout",
            "30000",
            "--max-bytes",
            "1000000",
        ]);
        c
    }
    fn unchanged(&self) {
        assert_eq!(
            fs::read(self.root.path().join("main.rs")).unwrap(),
            SOURCE.as_bytes()
        );
    }
}

#[test]
fn every_selected_lifecycle_and_prompt_delivery_include_exact_required_note_once() {
    let f = Fixture::new();
    for _ in 0..2 {
        let session = f.open(f.host.clone(), "required").unwrap();
        for _ in 0..2 {
            let reply = session.prompt_instructions(&d()).unwrap();
            assert!(reply.text.starts_with("BASE"));
            assert_eq!(reply.text.matches(BODY).count(), 1);
            let proof = &reply.evidence.unwrap()["requiredMemories"];
            assert_eq!(proof["notes"][0]["contentSha256"], yoke::model::hash(BODY));
            assert_eq!(proof["humanIdentityAttested"], false);
            assert_eq!(proof["sourceAuthority"], false);
        }
    }
    f.unchanged();
}

#[test]
fn serialized_selection_never_reconstructs_confirmation_or_grants_a_renderer() {
    let f = Fixture::new();
    let mut decoded: HostProfile =
        serde_json::from_value(serde_json::to_value(&f.host).unwrap()).unwrap();
    assert!(f.open(decoded.clone(), "ungranted").is_err());
    decoded.prompt_runtime = Some(renderer(decoded.prompt_templates.as_ref().unwrap()));
    let error = f.open(decoded, "unconfirmed").err().unwrap().to_string();
    assert!(error.contains("host_confirmation_required"), "{error}");
    f.unchanged();
}

#[test]
fn changed_or_deleted_required_note_blocks_resume_and_does_not_silently_reconfirm() {
    for deleted in [false, true] {
        let f = Fixture::new();
        let session = f.open(f.host.clone(), "required").unwrap();
        if deleted {
            let db = rusqlite::Connection::open(f.root.path().join(".yoke/index.sqlite")).unwrap();
            db.execute(
                "DELETE FROM records WHERE kind='native_named_note_head_v1'",
                [],
            )
            .unwrap();
        } else {
            f.change("CHANGED");
        }
        assert!(session.prompt_instructions(&d()).is_err());
        assert!(f.open(f.host.clone(), "required").is_err());
        f.unchanged();
    }
}

#[test]
fn changed_host_confirmation_invalidates_the_persisted_session_instead_of_rebinding() {
    let f = Fixture::new();
    drop(f.open(f.host.clone(), "required").unwrap());
    let mut host = f.host.clone();
    host.required_prompt_memories
        .as_mut()
        .unwrap()
        .projects
        .get_mut("default")
        .unwrap()[0]
        .revision_id = "e".repeat(64);
    host.required_prompt_memory_runtime = Some(Confirmation::confirm(&host, &d()).unwrap());
    assert!(
        f.open(host, "required")
            .err()
            .unwrap()
            .to_string()
            .contains("host binding changed")
    );
    f.unchanged();
}

#[test]
fn note_change_during_a_trusted_render_refuses_the_entire_instruction_result() {
    let f = Fixture::new();
    let mut host = f.host.clone();
    let path = f.root.path().join(".yoke/index.sqlite");
    host.prompt_runtime = Some(
        Runtime::trusted(
            host.prompt_templates.as_ref().unwrap(),
            Arc::new(move |_, _| {
                let db = rusqlite::Connection::open(&path)?;
                db.execute(
                    "DELETE FROM records WHERE kind='native_named_note_head_v1'",
                    [],
                )?;
                Ok(yoke::session_config::RenderedPrompt {
                    text: "MUST_NOT_DELIVER".into(),
                    evidence: json!({}),
                })
            }),
            &d(),
        )
        .unwrap(),
    );
    let session = f.open(host, "changed-during-render").unwrap();
    assert!(
        session
            .prompt_instructions(&d())
            .err()
            .unwrap()
            .to_string()
            .contains("changed_during_render")
    );
    f.unchanged();
}

#[test]
fn actual_cli_requires_independent_confirmation_and_includes_notes_without_embed_calls() {
    let f = Fixture::new();
    let profile = f.profile();
    let requests = f.host_files.path().join("requests.json");
    fs::write(
        &requests,
        serde_json::to_vec(&json!([{"operation":"initialInstructions","args":{}}])).unwrap(),
    )
    .unwrap();
    let run = |allow: bool| {
        let mut command = f.command();
        command.arg("--allow-session-templates");
        if allow {
            command.arg("--allow-required-prompt-memories");
        }
        command
            .args(["session", "--id", "cli", "--host-profile"])
            .arg(&profile)
            .arg("--requests")
            .arg(&requests)
            .output()
            .unwrap()
    };
    let denied = run(false);
    assert!(!denied.status.success());
    assert!(String::from_utf8_lossy(&denied.stderr).contains("host_confirmation_required"));
    let accepted = run(true);
    assert!(
        accepted.status.success(),
        "{}",
        String::from_utf8_lossy(&accepted.stderr)
    );
    let reply: Value = serde_json::from_slice(&accepted.stdout).unwrap();
    assert_eq!(
        reply["results"][0]["instructions"]
            .as_str()
            .unwrap()
            .matches(BODY)
            .count(),
        1
    );
    f.unchanged();
}

#[test]
fn actual_native_and_serena_stdio_initialize_with_required_content_and_no_request_grant() {
    let f = Fixture::new();
    let profile = f.profile();
    for selected in ["native", "serena"] {
        let mut child = f
            .command()
            .args([
                "--allow-session-templates",
                "--allow-required-prompt-memories",
                "serve-session",
                "--profile",
                selected,
                "--id",
                selected,
                "--host-profile",
            ])
            .arg(&profile)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(b"{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"initialize\",\"params\":{}}\n")
            .unwrap();
        let out = child.wait_with_output().unwrap();
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        let reply: Value = serde_json::from_slice(
            out.stdout
                .split(|b| *b == b'\n')
                .find(|line| !line.is_empty())
                .unwrap(),
        )
        .unwrap();
        assert!(reply.get("error").is_none(), "{reply}");
        assert_eq!(
            reply["result"]["instructions"]
                .as_str()
                .unwrap()
                .matches(BODY)
                .count(),
            1
        );
    }
    f.unchanged();
}

#[test]
fn actual_http_initialization_delivers_confirmed_notes_and_changed_notes_refuse() {
    let f = Fixture::new();
    let token = hex::encode([23u8; 32]);
    let target = yoke::mcp_http::Target::new(
        f.root.path(),
        yoke::protocol::Profile::Session,
        f.host.clone(),
    );
    let principal =
        yoke::mcp_http::Principal::new("owned", zeroize::Zeroizing::new(vec![23; 32]), target)
            .unwrap();
    let server = yoke::mcp_http::serve(yoke::mcp_http::Config::default(), vec![principal]).unwrap();
    let initialize = || {
        let body = r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"owned-required-notes","version":"1"}}}"#;
        let mut stream = TcpStream::connect(server.local_addr()).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(10)))
            .unwrap();
        stream
            .set_write_timeout(Some(Duration::from_secs(10)))
            .unwrap();
        write!(stream,"POST /mcp HTTP/1.1\r\nHost: {}\r\nAuthorization: Bearer {token}\r\nAccept: application/json, text/event-stream\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",server.local_addr(),body.len()).unwrap();
        let mut bytes = vec![];
        let mut chunk = [0; 8192];
        let until = Instant::now() + Duration::from_secs(10);
        loop {
            let remaining = until.saturating_duration_since(Instant::now());
            assert!(!remaining.is_zero(), "Owned HTTP response deadline");
            stream.set_read_timeout(Some(remaining)).unwrap();
            let n = stream.read(&mut chunk).unwrap();
            if n == 0 {
                break;
            }
            assert!(bytes.len() + n <= 1_000_000);
            bytes.extend_from_slice(&chunk[..n]);
            if let Some(end) = bytes.windows(4).position(|x| x == b"\r\n\r\n") {
                let header = std::str::from_utf8(&bytes[..end]).unwrap();
                if let Some(length) = header.lines().find_map(|line| {
                    line.split_once(':')
                        .filter(|(k, _)| k.eq_ignore_ascii_case("content-length"))
                        .map(|(_, v)| v.trim().parse::<usize>().unwrap())
                }) {
                    assert!(length <= 1_000_000);
                    if bytes.len() >= end + 4 + length {
                        break;
                    }
                }
            }
        }
        let end = bytes.windows(4).position(|x| x == b"\r\n\r\n").unwrap() + 4;
        serde_json::from_slice::<Value>(&bytes[end..]).unwrap()
    };
    let accepted = initialize();
    assert_eq!(
        accepted["result"]["instructions"]
            .as_str()
            .unwrap()
            .matches(BODY)
            .count(),
        1
    );
    f.change("CHANGED");
    let denied = initialize();
    assert!(
        denied.get("error").is_some() && denied.get("result").is_none(),
        "{denied}"
    );
    server.shutdown().unwrap();
    f.unchanged();
}

#[test]
fn http_cli_refuses_missing_confirmation_before_resolving_any_credential() {
    let f = Fixture::new();
    let path = f.host_files.path().join("http.json");
    fs::write(
        &path,
        serde_json::to_vec(&json!({"principals":[{
            "id":"owned", "tokenCredentialName":"yoke-owned-required-notes-not-created",
            "profile":"session", "root":f.root.path(), "hostProfile":f.host
        }]}))
        .unwrap(),
    )
    .unwrap();
    let out = f
        .command()
        .args(["--allow-session-templates", "serve-http", "--config"])
        .arg(path)
        .arg("--trust-service")
        .output()
        .unwrap();
    assert!(!out.status.success());
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("required_memory_host_confirmation_required")
    );
    assert!(out.stdout.is_empty());
    f.unchanged();
}

#[test]
fn host_revocation_during_render_prevents_delivery_and_future_resume() {
    let f = Fixture::new();
    let mut host = f.host.clone();
    let confirmation = host
        .required_prompt_memory_runtime
        .as_ref()
        .unwrap()
        .clone();
    host.prompt_runtime = Some(
        Runtime::trusted(
            host.prompt_templates.as_ref().unwrap(),
            Arc::new(move |_, _| {
                confirmation.revoke();
                Ok(yoke::session_config::RenderedPrompt {
                    text: "MUST_NOT_DELIVER".into(),
                    evidence: json!({}),
                })
            }),
            &d(),
        )
        .unwrap(),
    );
    let session = f.open(host.clone(), "revoked").unwrap();
    assert!(
        session
            .prompt_instructions(&d())
            .err()
            .unwrap()
            .to_string()
            .contains("confirmation_revoked")
    );
    assert!(f.open(host, "revoked").is_err());
    f.unchanged();
}

#[test]
fn registered_project_transition_requires_its_own_pins_and_returns_fresh_guidance() {
    let primary = Fixture::new();
    let secondary = Fixture::new();
    for wrong_scope in [true, false] {
        let mut host = primary.host.clone();
        host.projects.push(yoke::project_session::HostProject {
            name: "other".into(),
            root: secondary.root.path().canonicalize().unwrap(),
            read_only: true,
            query_only: false,
        });
        let pin = if wrong_scope {
            primary.host.required_prompt_memories.as_ref().unwrap()
        } else {
            secondary.host.required_prompt_memories.as_ref().unwrap()
        }
        .projects["default"][0]
            .clone();
        host.required_prompt_memories
            .as_mut()
            .unwrap()
            .projects
            .insert("other".into(), vec![pin]);
        host.required_prompt_memory_runtime = Some(Confirmation::confirm(&host, &d()).unwrap());
        let mut session = primary
            .open(
                host,
                if wrong_scope {
                    "wrong-project"
                } else {
                    "right-project"
                },
            )
            .unwrap();
        let result = session.invoke("activate", &json!({"project":"other"}), &d());
        if wrong_scope {
            assert!(result.is_err());
            assert_eq!(
                session.invoke("configuration", &json!({}), &d()).unwrap()["activeProject"],
                "default"
            );
        } else {
            let value = result.unwrap();
            assert_eq!(value["activeProject"], "other");
            assert_eq!(
                value["instructions"]
                    .as_str()
                    .unwrap()
                    .matches(BODY)
                    .count(),
                1
            );
            assert_eq!(
                value["promptRendering"]["requiredMemories"]["project"],
                "other"
            );
        }
    }
    primary.unchanged();
    secondary.unchanged();
}

#[test]
fn disabling_required_memory_reads_refuses_before_mode_state_publication() {
    let f = Fixture::new();
    let mut session = f.open(f.host.clone(), "modes").unwrap();
    let before = session.invoke("configuration", &json!({}), &d()).unwrap();
    assert!(
        session
            .invoke("switchModes", &json!({"modes":["no-memories"]}), &d())
            .is_err()
    );
    assert_eq!(
        session.invoke("configuration", &json!({}), &d()).unwrap(),
        before
    );
    f.unchanged();
}

#[test]
fn confirmed_required_notes_do_not_bypass_mode_tool_api_or_ignored_read_policy() {
    for case in 0..4 {
        let f = Fixture::new();
        let unopened_global = tempfile::tempdir().unwrap();
        let mut host = f.host.clone();
        host.global_notes_root = Some(unopened_global.path().canonicalize().unwrap());
        host.prompt_templates.as_mut().unwrap().memory_embeddings =
            Some(vec![NAME.into(), "global/unavailable".into()]);
        host.prompt_runtime = Some(renderer(host.prompt_templates.as_ref().unwrap()));
        let expected = match case {
            0 => {
                host.modes = vec!["no-memories".into()];
                "Notes disabled"
            }
            1 => {
                host.steering = Some(
                    serde_json::from_value(json!({"global":{"excludedTools":["read_memory"]}}))
                        .unwrap(),
                );
                "Tool excluded"
            }
            2 => {
                host.steering = Some(
                    serde_json::from_value(json!({"global":{"excludedApis":["mem.read_memory"]}}))
                        .unwrap(),
                );
                "API excluded"
            }
            _ => {
                host.steering = Some(
                    serde_json::from_value(json!({"global":{"ignoredMemoryPatterns":["^guide"]}}))
                        .unwrap(),
                );
                "Note unavailable"
            }
        };
        host.required_prompt_memory_runtime = Some(Confirmation::confirm(&host, &d()).unwrap());
        let failure = f.open(host, "read-policy").err().unwrap();
        assert!(format!("{failure:#}").contains(expected), "{failure:#}");
        assert!(!unopened_global.path().join(".yoke").exists());
        f.unchanged();
    }
}

#[test]
fn global_and_project_required_notes_keep_distinct_stores_and_original_revisions() {
    let f = Fixture::new();
    let global = tempfile::tempdir().unwrap();
    drop(Engine::open(global.path()).unwrap());
    let global_root = global.path().canonicalize().unwrap();
    let mut writer = f
        .open(
            HostProfile {
                global_notes_root: Some(global_root.clone()),
                ..Default::default()
            },
            "global-writer",
        )
        .unwrap();
    yoke::agent_repl::invoke_facade(
        &mut writer,
        "mem.write_memory",
        json!({"memory_name":"global/team","content":"GLOBAL Ω\r\n"}),
        None,
        &d(),
    )
    .unwrap();
    drop(writer);
    let mut host = f.host.clone();
    host.global_notes_root = Some(global_root);
    host.required_prompt_memories = None;
    host.required_prompt_memory_runtime = None;
    host.prompt_templates.as_mut().unwrap().memory_embeddings =
        Some(vec![NAME.into(), "global/team".into()]);
    host.prompt_runtime = Some(renderer(host.prompt_templates.as_ref().unwrap()));
    let selected = f
        .open(host.clone(), "global-pin")
        .unwrap()
        .prompt_instructions(&d())
        .unwrap();
    let revision = &selected.evidence.unwrap()["binding"]["memorySnapshot"]["entries"]["global/team"]
        ["revision"];
    let global_pin = pin("global/team", revision);
    assert_ne!(
        global_pin.storage_workspace_id,
        global_pin.source_workspace_id
    );
    let mut policy = f.host.required_prompt_memories.clone().unwrap();
    policy.global.push(global_pin);
    host.required_prompt_memories = Some(policy);
    host.required_prompt_memory_runtime = Some(Confirmation::confirm(&host, &d()).unwrap());
    let selected = f
        .open(host, "global-required")
        .unwrap()
        .prompt_instructions(&d())
        .unwrap();
    assert_eq!(selected.text.matches(BODY).count(), 1);
    assert_eq!(selected.text.matches("GLOBAL Ω\r\n").count(), 1);
    assert_eq!(
        selected.evidence.unwrap()["requiredMemories"]["notes"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    f.unchanged();
}
