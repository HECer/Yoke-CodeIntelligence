//! Actual memory embedding, immutable authored provenance and host-policy refusal.
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
    Engine,
    capture::Deadline,
    project_session::{HostProfile, Session},
    session_prompt::{
        Config, Runtime,
        memory::{Entry, Snapshot},
    },
};

struct Fixture {
    root: tempfile::TempDir,
    profile: PathBuf,
    original: Vec<u8>,
}
impl Fixture {
    fn new() -> Self {
        let root = tempfile::tempdir().unwrap();
        let original = "// own e\u{301} \u{1f980}\r\nfn visible() {}\r\n"
            .as_bytes()
            .to_vec();
        fs::write(root.path().join("main.rs"), &original).unwrap();
        fs::write(
            root.path().join(".env"),
            b"OWNED_PROTECTED_CANARY=not-readable\n",
        )
        .unwrap();
        let profile = root.path().join("profile.json");
        Self {
            root,
            profile,
            original,
        }
    }
    fn seed(&self, values: &[(&str, &str)], global: Option<&tempfile::TempDir>) {
        let mut host = json!({"notesPolicy":{"writable":true}});
        if let Some(g) = global {
            host["globalNotesRoot"] = json!(g.path());
        }
        let d = Deadline::new(30000);
        let mut session = Session::open(
            Engine::open(self.root.path()).unwrap(),
            serde_json::from_value(host).unwrap(),
            Some("seed"),
            &d,
        )
        .unwrap();
        for (name, text) in values {
            yoke::agent_repl::invoke_facade(
                &mut session,
                "mem.write_memory",
                json!({"memory_name":name,"content":text}),
                None,
                &d,
            )
            .unwrap();
        }
    }
    fn host(&self, prompt: &str, names: Value) -> Value {
        json!({"context":"desktop-app","modes":["editing"],"notesPolicy":{"writable":true},"promptTemplates":{"context":{"templates":{"root":prompt}},"systemPrompt":"root","memoryEmbeddings":names},"steering":{}})
    }
    fn set(&self, value: Value) {
        fs::write(&self.profile, serde_json::to_vec(&value).unwrap()).unwrap();
    }
    fn command(&self) -> Command {
        let mut c = Command::new(env!("CARGO_BIN_EXE_yoke"));
        c.arg("--root").arg(self.root.path()).args([
            "--timeout",
            "30000",
            "--max-bytes",
            "4000000",
        ]);
        c
    }
    fn mcp(&self, profile: &str, messages: &[Value]) -> Vec<Value> {
        let mut child = self
            .command()
            .args([
                "--allow-session-templates",
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
        let input = messages
            .iter()
            .map(|v| format!("{v}\n"))
            .collect::<String>();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(input.as_bytes())
            .unwrap();
        let out = child.wait_with_output().unwrap();
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        out.stdout
            .split(|b| *b == b'\n')
            .filter(|s| !s.is_empty())
            .map(|s| serde_json::from_slice(s).unwrap())
            .collect()
    }
    fn unchanged(&self) {
        assert_eq!(
            fs::read(self.root.path().join("main.rs")).unwrap(),
            self.original
        );
        assert_eq!(
            fs::read(self.root.path().join(".env")).unwrap(),
            b"OWNED_PROTECTED_CANARY=not-readable\n"
        );
        assert!(!self.root.path().join("PWNED").exists());
    }
    fn notes(&self) -> Vec<(String, String, String)> {
        let db = rusqlite::Connection::open_with_flags(
            self.root.path().join(".yoke/index.sqlite"),
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
        )
        .unwrap();
        db.prepare("SELECT kind,id,value FROM records WHERE kind IN ('native_named_note_head_v1','native_named_note_revision_v1') ORDER BY kind,id").unwrap().query_map([],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).unwrap().collect::<Result<_,_>>().unwrap()
    }
}
fn message(id: u64, method: &str, params: Value) -> Value {
    json!({"jsonrpc":"2.0","id":id,"method":method,"params":params})
}
fn initialize() -> Value {
    message(1, "initialize", json!({}))
}
fn frame(values: &[Value], id: u64) -> &Value {
    values.iter().find(|v| v["id"] == id).unwrap()
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
fn all_actual_transport_initializations_embed_inert_content_with_exact_authored_revision_receipts()
{
    let f = Fixture::new();
    let text = "owned e\u{301} \u{1f980}\r\n{{ env('TOKEN') }} {% include '.env' %}";
    f.seed(&[("guide & e\u{301}", text)], None);
    let before = f.notes();
    // A real Unicode name in the actual template, not a backslash escape approximation.
    let mut host = f.host("", json!(["guide & e\u{301}"]));
    host["promptTemplates"]["context"]["templates"]["root"] =
        json!(format!("{{{{ embed_memory('{}') }}}}", "guide & e\u{301}"));
    f.set(host);
    for profile in ["native", "serena", "graphify", "trail", "graft"] {
        let values = f.mcp(profile, &[initialize()]);
        let result = &frame(&values, 1)["result"];
        let instructions = result["instructions"]
            .as_str()
            .unwrap_or_else(|| panic!("{values:?}"));
        assert!(instructions.contains(text));
        assert!(instructions.contains("<memory name=\"guide &amp; e\u{301}\">"));
        let rendering = &result["promptRendering"];
        assert_eq!(rendering["memoryEmbeddingsPartial"], false);
        let evidence = &rendering["fragments"][0]["rendering"];
        assert_eq!(evidence["schema"], "native_session_prompt_jinja_v4");
        let used = &evidence["memoryEmbeddings"]["used"]["guide & e\u{301}"];
        assert_eq!(used["revision"]["contentSha256"], yoke::model::hash(text));
        assert_eq!(used["revision"]["scope"], "project");
        assert_eq!(used["calls"], 1);
        assert!(!rendering["binding"].to_string().contains(text));
        assert_eq!(f.notes(), before);
    }
    f.unchanged();
}

#[test]
fn selected_missing_memory_is_partial_but_an_unselected_name_fails_initialization() {
    let f = Fixture::new();
    f.seed(&[], None);
    f.set(f.host("A{{ embed_memory('missing') }}B", json!(["missing"])));
    let values = f.mcp("native", &[initialize()]);
    let result = &frame(&values, 1)["result"];
    assert!(result["instructions"].as_str().unwrap().contains("\n\nAB"));
    let r = &result["promptRendering"];
    assert_eq!(
        r["fragments"][0]["rendering"]["renderedSha256"],
        yoke::model::hash("AB")
    );
    assert_eq!(r["memoryEmbeddingsPartial"], true);
    assert_eq!(
        r["fragments"][0]["rendering"]["memoryEmbeddings"]["warnings"][0]["code"],
        "named_memory_unavailable"
    );
    f.set(f.host("{{ embed_memory('unselected') }}", json!(["missing"])));
    let values = f.mcp(
        "serena",
        &[initialize(), message(2, "tools/list", json!({}))],
    );
    assert_eq!(frame(&values, 1)["error"]["code"], -32001);
    assert_eq!(frame(&values, 2)["error"]["code"], -32002);
    f.unchanged();
}

#[test]
fn no_memory_tool_api_and_ignored_policies_refuse_before_a_global_store_is_opened() {
    for case in 0..4 {
        let f = Fixture::new();
        let global = tempfile::tempdir().unwrap();
        let mut host = f.host(
            "{{ embed_memory('global/secret') }}",
            json!(["global/secret"]),
        );
        host["globalNotesRoot"] = json!(global.path());
        let expected = match case {
            0 => {
                host["modes"] = json!(["no-memories"]);
                "Notes disabled"
            }
            1 => {
                host["steering"] = json!({"global":{"excludedTools":["read_memory"]}});
                "Tool excluded"
            }
            2 => {
                host["steering"] = json!({"global":{"excludedApis":["mem.read_memory"]}});
                "API excluded"
            }
            _ => {
                host["steering"] = json!({"global":{"ignoredMemoryPatterns":["^global/secret$"]}});
                "Note unavailable"
            }
        };
        f.set(host);
        let values = f.mcp("native", &[initialize()]);
        assert!(
            frame(&values, 1)["error"]["message"]
                .as_str()
                .unwrap()
                .contains(expected),
            "{values:?}"
        );
        assert!(!global.path().join(".yoke").exists());
        f.unchanged();
    }
}

#[test]
fn global_and_project_notes_keep_distinct_origins_and_activation_reads_the_new_project() {
    let f = Fixture::new();
    let other = Fixture::new();
    let global = tempfile::tempdir().unwrap();
    f.seed(
        &[("guide", "PRIMARY"), ("global/guide", "GLOBAL")],
        Some(&global),
    );
    other.seed(&[("guide", "SECONDARY")], None);
    let before = f.notes();
    let other_before = other.notes();
    let mut host = f.host(
        "{{ embed_memory('guide') }}{{ embed_memory('global/guide') }}",
        json!(["guide", "global/guide"]),
    );
    host["globalNotesRoot"] = json!(global.path());
    host["projects"] =
        json!([{"name":"other","root":other.root.path(),"readOnly":true,"queryOnly":false}]);
    f.set(host);
    let requests = f.root.path().join("requests.json");
    fs::write(&requests,serde_json::to_vec(&json!([{"operation":"initialInstructions","args":{}},{"operation":"activate","args":{"project":"other"}},{"operation":"initialInstructions","args":{}}])).unwrap()).unwrap();
    let out = f
        .command()
        .args([
            "--allow-session-templates",
            "session",
            "--id",
            "activate",
            "--host-profile",
        ])
        .arg(&f.profile)
        .arg("--requests")
        .arg(requests)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    let a = &v["results"][0];
    let b = &v["results"][2];
    assert!(a["instructions"].as_str().unwrap().contains("PRIMARY"));
    assert!(b["instructions"].as_str().unwrap().contains("SECONDARY"));
    assert!(b["instructions"].as_str().unwrap().contains("GLOBAL"));
    assert_ne!(
        a["promptRendering"]["binding"]["memorySnapshotSha256"],
        b["promptRendering"]["binding"]["memorySnapshotSha256"]
    );
    assert_eq!(f.notes(), before);
    assert_eq!(other.notes(), other_before);
    f.unchanged();
    other.unchanged();
}

#[test]
fn worker_four_requires_exact_inert_snapshot_binding_and_old_protocols_refuse_the_new_field() {
    let snapshot = Snapshot {
        entries: std::collections::BTreeMap::from([("missing".into(), Entry::Unavailable)]),
    };
    let input = json!({"schema":4,"context":{},"prompt":"{{ embed_memory('missing') }}","runtimeBinding":{"memorySnapshotSha256":snapshot.hash().unwrap()},"memoryContext":snapshot});
    let out = worker(input.clone());
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["schema"], "native_session_prompt_jinja_v4");
    assert_eq!(v["memoryEmbeddings"]["partial"], true);
    for schema in 1..=3 {
        let mut v = json!({"schema":schema,"context":{},"prompt":"literal","memoryContext":null});
        if schema == 3 {
            v["runtimeBinding"] = json!({});
        }
        let out = worker(v);
        assert!(!out.status.success());
        assert!(out.stdout.is_empty());
    }
    for null in [false, true] {
        let mut v = input.clone();
        if null {
            v["memoryContext"] = Value::Null;
        } else {
            v["runtimeBinding"]["memorySnapshotSha256"] = json!("a".repeat(64));
        }
        let out = worker(v);
        assert!(!out.status.success());
        assert!(out.stdout.is_empty());
    }
}

#[test]
fn changing_an_owned_note_head_during_a_trusted_render_refuses_delivery_and_preserves_the_change() {
    let f = Fixture::new();
    f.seed(&[("guide", "BEFORE")], None);
    let d = Deadline::new(30000);
    let config: Config = serde_json::from_value(json!({"memoryEmbeddings":["guide"]})).unwrap();
    let root = f.root.path().to_path_buf();
    let calls = Arc::new(AtomicUsize::new(0));
    let seen = Arc::clone(&calls);
    let runtime = Runtime::trusted(
        &config,
        Arc::new(move |input, _| {
            assert!(input.memories.is_some());
            seen.fetch_add(1, Ordering::SeqCst);
            let db = rusqlite::Connection::open(root.join(".yoke/index.sqlite"))?;
            db.execute(
                "DELETE FROM records WHERE kind='native_named_note_head_v1'",
                [],
            )?;
            Ok(yoke::session_config::RenderedPrompt {
                text: "MUST_NOT_DELIVER".into(),
                evidence: json!({}),
            })
        }),
        &d,
    )
    .unwrap();
    let host = HostProfile {
        prompt_templates: Some(config),
        prompt_runtime: Some(runtime),
        steering: Some(serde_json::from_value(json!({"global":{"instructions":"owned"}})).unwrap()),
        ..Default::default()
    };
    let session = Session::open(
        Engine::open(f.root.path()).unwrap(),
        host,
        Some("stale"),
        &d,
    )
    .unwrap();
    let error = session.prompt_instructions(&d).err().unwrap().to_string();
    assert!(error.contains("changed_during_render"), "{error}");
    assert!(calls.load(Ordering::SeqCst) > 0);
    assert!(
        f.notes()
            .iter()
            .all(|(kind, _, _)| kind != yoke::notes::HEAD_KIND)
    );
    f.unchanged();
}

#[test]
fn deferred_policy_and_discovery_preserve_explicit_memory_selection_without_loading_notes() {
    let f = Fixture::new();
    let yaml = f.root.path().join("global.yml");
    fs::write(&yaml, b"{}\r\n").unwrap();
    let selection = f.root.path().join("selection.json");
    let input = json!({"hostProfile":{"context":"desktop-app","steering":{}},"files":[{"path":yaml,"sha256":yoke::model::hash(fs::read(&yaml).unwrap()),"target":{"kind":"global"}}],"templateContext":{"templates":{"root":"{{ embed_memory('guide') }}"}},"systemPrompt":"root","memoryEmbeddings":["guide"]});
    fs::write(&selection, serde_json::to_vec(&input).unwrap()).unwrap();
    for operation in ["plan", "discover"] {
        let out = Command::new(env!("CARGO_BIN_EXE_yoke"))
            .args(["session-config", operation, "--selection"])
            .arg(&selection)
            .args([
                "--allow-configuration-read",
                "--allow-policy-projection",
                "--allow-template-evaluation",
                "--defer-template-evaluation",
            ])
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        let v: Value = serde_json::from_slice(&out.stdout).unwrap();
        let projection = if operation == "discover" {
            &v["projection"]
        } else {
            &v
        };
        assert_eq!(
            projection["hostProfile"]["promptTemplates"]["memoryEmbeddings"],
            json!(["guide"])
        );
        assert_eq!(projection["templatesEvaluated"], false);
        assert!(!f.root.path().join(".yoke").exists());
    }
    let mut eager = input.clone();
    eager.as_object_mut().unwrap().remove("systemPrompt");
    fs::write(&selection, serde_json::to_vec(&eager).unwrap()).unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_yoke"))
        .args(["session-config", "plan", "--selection"])
        .arg(&selection)
        .args([
            "--allow-configuration-read",
            "--allow-policy-projection",
            "--allow-template-evaluation",
        ])
        .output()
        .unwrap();
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("memory_requires_deferred"));
    f.unchanged();
}

#[test]
fn oversized_stored_head_and_tampered_revision_refuse_without_partial_guidance() {
    for oversized in [true, false] {
        let f = Fixture::new();
        f.seed(&[("guide", "ORIGINAL")], None);
        let db = rusqlite::Connection::open(f.root.path().join(".yoke/index.sqlite")).unwrap();
        if oversized {
            db.execute(
                "UPDATE records SET value=?1 WHERE kind='native_named_note_head_v1'",
                ["x".repeat(500000)],
            )
            .unwrap();
        } else {
            let (id, raw): (String, String) = db
                .query_row(
                    "SELECT id,value FROM records WHERE kind='native_named_note_revision_v1'",
                    [],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )
                .unwrap();
            let mut value: Value = serde_json::from_str(&raw).unwrap();
            value["content"] = json!("FORGED");
            db.execute(
                "UPDATE records SET value=?1 WHERE kind='native_named_note_revision_v1' AND id=?2",
                rusqlite::params![value.to_string(), id],
            )
            .unwrap();
        }
        drop(db);
        f.set(f.host("{{ embed_memory('guide') }}", json!(["guide"])));
        let values = f.mcp(
            "native",
            &[initialize(), message(2, "tools/list", json!({}))],
        );
        assert_eq!(frame(&values, 1)["error"]["code"], -32001);
        assert_eq!(frame(&values, 2)["error"]["code"], -32002);
        assert!(frame(&values, 1).get("result").is_none());
        f.unchanged();
    }
}

#[test]
fn an_available_note_cannot_expand_the_worker_output_budget() {
    let f = Fixture::new();
    let large = "x".repeat(9000);
    f.seed(&[("guide", &large)], None);
    let before = f.notes();
    f.set(f.host("{{ embed_memory('guide') }}", json!(["guide"])));
    let values = f.mcp("native", &[initialize()]);
    assert_eq!(frame(&values, 1)["error"]["code"], -32001);
    assert!(frame(&values, 1).get("result").is_none());
    assert_eq!(f.notes(), before);
    f.unchanged();
}
