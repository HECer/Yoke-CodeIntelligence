#![cfg(windows)]
//! Real captured Views and receipt-owned callbacks; no client/model equivalence claim.
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};
use yoke::{
    Engine,
    capture::Deadline,
    graph_view::{self, BuildOptions, View},
    host_install::{
        self, Selection, SourceHookLimits, SourceHookMode, SourceHookSource, WriteAuthority,
    },
    model::hash,
};

struct Fixture {
    _dir: tempfile::TempDir,
    source: PathBuf,
    root: PathBuf,
    state: PathBuf,
    exe: PathBuf,
    policy: PathBuf,
    view: View,
    selection: Selection,
}
fn fixture(limits: SourceHookLimits) -> Fixture {
    fixture_at(limits, "source ' İ π")
}
fn fixture_at(limits: SourceHookLimits, source_name: &str) -> Fixture {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join(source_name);
    let root = dir.path().join("client 🌍");
    let state = dir.path().join("state");
    for p in [&source, &root, &state] {
        fs::create_dir(p).unwrap();
    }
    fs::write(source.join("alpha.rs"), "pub fn helper(){ let π = 1; }\r\n").unwrap();
    fs::write(
        source.join("caller.rs"),
        "fn caller(){helper();helper();}\r\n",
    )
    .unwrap();
    let mut engine = Engine::open(&source).unwrap();
    let d = Deadline::new(30000);
    let (snapshot, graph, _, _) = engine.index(&d).unwrap();
    let view = graph_view::build(
        &engine.store,
        &snapshot,
        &graph,
        &BuildOptions::default(),
        &d,
    )
    .unwrap();
    drop(engine);
    let exe = PathBuf::from(env!("CARGO_BIN_EXE_yoke"));
    let policy = dir.path().join("host policy.json");
    fs::write(&policy, b"{}").unwrap();
    let selection = Selection::claude_source_hooks(
        "owned-source-hooks",
        std::slice::from_ref(&root),
        &exe,
        &policy,
        &state,
        SourceHookSource {
            root: &source,
            view_id: &view.id,
            limits,
        },
        &d,
    )
    .unwrap();
    Fixture {
        _dir: dir,
        source,
        root,
        state,
        exe,
        policy,
        view,
        selection,
    }
}
fn install_unprepared_reader(f: &Fixture) -> (Vec<u8>, String) {
    let d = Deadline::new(60000);
    let plan = host_install::plan_install(&f.selection, 1000, &d).unwrap();
    assert_eq!(plan.profile, "claude-source-hooks-v1");
    assert_eq!(plan.files[0].path, "yoke-claude-source-hooks.json");
    assert!(fs::read_dir(&f.state).unwrap().next().is_none());
    host_install::apply(
        &f.selection,
        &WriteAuthority::new(&f.selection),
        &plan,
        1001,
        &d,
    )
    .unwrap();
    let bytes = fs::read(f.root.join("yoke-claude-source-hooks.json")).unwrap();
    let v: Value = serde_json::from_slice(&bytes).unwrap();
    let launch = v["hooks"]["UserPromptSubmit"][0]["hooks"][0]["args"][7]
        .as_str()
        .unwrap()
        .to_owned();
    (bytes, launch)
}
fn install(f: &Fixture) -> (Vec<u8>, String) {
    let result = install_unprepared_reader(f);
    // SQLite may initialize shared-reader coordination even for a read-only
    // connection. The separate first-reader test measures that exact delta.
    let d = Deadline::new(30000);
    let store = yoke::store::Store::open_existing_read_only(&f.source, &d).unwrap();
    graph_view::load(&store, &f.view.id, &d).unwrap();
    drop(store);
    result
}
fn event(f: &Fixture, mode: SourceHookMode) -> Value {
    let name = match mode {
        SourceHookMode::SessionStart => "SessionStart",
        SourceHookMode::PromptSubmit => "UserPromptSubmit",
        SourceHookMode::PostEdit => "PostToolUse",
        SourceHookMode::Status => "status",
    };
    json!({"cwd":f.source,"hook_event_name":name,"source":"startup","prompt":"helper","tool_name":"Edit","tool_input":{"file_path":f.source.join("alpha.rs"),"new_string":"ignored and never applied"},"tool_response":{"permissionsGranted":true},"transcript_path":"../../never-read.jsonl","session_id":"untrusted"})
}
fn callback(
    mode: SourceHookMode,
    launch: &str,
    input: &Value,
    max: usize,
) -> anyhow::Result<Vec<u8>> {
    host_install::claude_source_hook(
        mode,
        launch.as_bytes(),
        &serde_json::to_vec(input).unwrap(),
        max,
        &Deadline::new(30000),
    )
}
fn context(bytes: &[u8]) -> Value {
    let v: Value = serde_json::from_slice(bytes).unwrap();
    serde_json::from_str(
        v["hookSpecificOutput"]["additionalContext"]
            .as_str()
            .unwrap(),
    )
    .unwrap()
}
fn manifest(root: &Path) -> BTreeMap<PathBuf, String> {
    let mut out = BTreeMap::new();
    let mut todo = vec![root.to_owned()];
    while let Some(path) = todo.pop() {
        for entry in fs::read_dir(path).unwrap() {
            let p = entry.unwrap().path();
            if p.is_dir() {
                todo.push(p)
            } else {
                out.insert(
                    p.strip_prefix(root).unwrap().to_owned(),
                    hash(fs::read(p).unwrap()),
                );
            }
        }
    }
    out
}

#[test]
fn first_reader_creates_only_sqlite_coordination_and_preserves_all_original_bytes() {
    let f = fixture(Default::default());
    let (_, launch) = install_unprepared_reader(&f);
    let before = manifest(f._dir.path());
    callback(
        SourceHookMode::PromptSubmit,
        &launch,
        &event(&f, SourceHookMode::PromptSubmit),
        65536,
    )
    .unwrap();
    let mut after = manifest(f._dir.path());
    let added = after
        .keys()
        .filter(|path| !before.contains_key(*path))
        .cloned()
        .collect::<Vec<_>>();
    let prefix = f.source.strip_prefix(f._dir.path()).unwrap().join(".yoke");
    assert_eq!(
        added,
        vec![
            prefix.join("index.sqlite-shm"),
            prefix.join("index.sqlite-wal")
        ]
    );
    assert_eq!(
        fs::metadata(f.source.join(".yoke/index.sqlite-wal"))
            .unwrap()
            .len(),
        0
    );
    assert_eq!(
        fs::metadata(f.source.join(".yoke/index.sqlite-shm"))
            .unwrap()
            .len(),
        32768
    );
    for path in added {
        after.remove(&path);
    }
    assert_eq!(before, after);
}

#[test]
fn source_context_and_incoming_parallel_observations_are_real_immutable_view_queries() {
    let f = fixture(Default::default());
    let (settings, launch) = install(&f);
    let before = manifest(f._dir.path());
    let config: Value = serde_json::from_slice(&settings).unwrap();
    assert!(config.get("statusLine").is_none());
    let prompt = context(
        &callback(
            SourceHookMode::PromptSubmit,
            &launch,
            &event(&f, SourceHookMode::PromptSubmit),
            65536,
        )
        .unwrap(),
    );
    assert_eq!(prompt["graphViewId"], f.view.id);
    assert_eq!(prompt["sourceFreshness"], "not_checked");
    assert_eq!(prompt["modelCalls"], 0);
    assert!(prompt["data"]["sourceBytes"].as_u64().unwrap() > 0);
    let sources = prompt["data"]["contexts"][0]["sources"].as_array().unwrap();
    assert!(!sources.is_empty());
    for source in sources {
        let bytes = fs::read(f.source.join(source["path"].as_str().unwrap())).unwrap();
        let start = source["startByte"].as_u64().unwrap() as usize;
        let end = source["endByte"].as_u64().unwrap() as usize;
        assert_eq!(source["contentHash"], hash(&bytes));
        assert_eq!(
            &bytes[start..end],
            source["text"].as_str().unwrap().as_bytes()
        );
    }
    let edit = context(
        &callback(
            SourceHookMode::PostEdit,
            &launch,
            &event(&f, SourceHookMode::PostEdit),
            65536,
        )
        .unwrap(),
    );
    assert_eq!(edit["data"]["editEventIndependentlyVerified"], false);
    assert_eq!(edit["data"]["liveSourceRead"], false);
    let observations = edit["data"]["impact"]["observations"].as_array().unwrap();
    assert!(
        observations.len() >= 2,
        "expected original parallel helper calls: {edit}"
    );
    for row in observations {
        assert!(f.view.observations.iter().any(|old| old["id"] == row["id"]));
    }
    let status = callback(
        SourceHookMode::Status,
        &launch,
        &event(&f, SourceHookMode::Status),
        256,
    )
    .unwrap();
    assert!(
        String::from_utf8(status)
            .unwrap()
            .contains("freshness unchecked")
    );
    assert_eq!(before, manifest(f._dir.path()));
}

#[test]
fn post_edit_after_disk_drift_remains_explicitly_historical_and_never_reindexes_or_applies_event_text()
 {
    let f = fixture(Default::default());
    let (_, launch) = install(&f);
    fs::write(f.source.join("alpha.rs"), b"pub fn changed(){}\r\n").unwrap();
    let before = manifest(f._dir.path());
    let edit = context(
        &callback(
            SourceHookMode::PostEdit,
            &launch,
            &event(&f, SourceHookMode::PostEdit),
            65536,
        )
        .unwrap(),
    );
    assert_eq!(edit["graphViewId"], f.view.id);
    assert_eq!(edit["indexPublished"], false);
    assert_eq!(edit["sourceApplicationPerformed"], false);
    assert_eq!(
        edit["data"]["capturedSourceHash"],
        hash("pub fn helper(){ let π = 1; }\r\n")
    );
    assert_eq!(before, manifest(f._dir.path()));
}

#[test]
fn zero_prompt_term_and_source_budgets_remain_zero_without_empty_complete_results() {
    for source in [false, true] {
        let limits = if source {
            SourceHookLimits {
                max_source_bytes: 0,
                ..Default::default()
            }
        } else {
            SourceHookLimits {
                max_terms: 0,
                ..Default::default()
            }
        };
        let f = fixture(limits);
        let (_, launch) = install(&f);
        let before = manifest(f._dir.path());
        let out = context(
            &callback(
                SourceHookMode::PromptSubmit,
                &launch,
                &event(&f, SourceHookMode::PromptSubmit),
                65536,
            )
            .unwrap(),
        );
        assert_eq!(out["data"]["contexts"], json!([]));
        assert_eq!(out["data"]["sourceBytes"], 0);
        assert_eq!(out["data"]["complete"], false);
        assert_eq!(out["data"]["termsOmitted"], 1);
        assert_eq!(before, manifest(f._dir.path()));
    }
}

#[test]
fn paths_event_authority_duplicate_keys_and_tiny_output_refuse_without_side_effects() {
    let f = fixture(Default::default());
    let (_, launch) = install(&f);
    let before = manifest(f._dir.path());
    for path in [
        f.source.join("../host policy.json"),
        f.source.join(".env"),
        f.source.join("new-uncaptured.rs"),
        f.root.join("outside.rs"),
    ] {
        let mut input = event(&f, SourceHookMode::PostEdit);
        input["tool_input"]["file_path"] = json!(path);
        assert!(callback(SourceHookMode::PostEdit, &launch, &input, 65536).is_err());
    }
    for key in ["cwd", "hook_event_name"] {
        let mut input = event(&f, SourceHookMode::PromptSubmit);
        input[key] = json!("private_marker_do_not_echo");
        assert!(callback(SourceHookMode::PromptSubmit, &launch, &input, 65536).is_err());
    }
    assert!(
        callback(
            SourceHookMode::PromptSubmit,
            &launch,
            &event(&f, SourceHookMode::PromptSubmit),
            1
        )
        .is_err()
    );
    let duplicate = br#"{"cwd":"private_marker","cwd":"again"}"#;
    assert!(
        host_install::claude_source_hook(
            SourceHookMode::PromptSubmit,
            launch.as_bytes(),
            duplicate,
            65536,
            &Deadline::new(30000)
        )
        .is_err()
    );
    let expired = Deadline::new(1);
    std::thread::sleep(std::time::Duration::from_millis(5));
    assert!(
        host_install::claude_source_hook(
            SourceHookMode::PromptSubmit,
            launch.as_bytes(),
            b"{}",
            65536,
            &expired
        )
        .is_err()
    );
    assert_eq!(before, manifest(f._dir.path()));
}

#[test]
fn changed_policy_settings_view_binding_and_captured_blob_refuse() {
    let f = fixture(Default::default());
    let (settings, launch) = install(&f);
    let input = event(&f, SourceHookMode::PromptSubmit);
    let original_policy = fs::read(&f.policy).unwrap();
    fs::write(&f.policy, b"{\"changed\":true}").unwrap();
    assert!(callback(SourceHookMode::PromptSubmit, &launch, &input, 65536).is_err());
    fs::write(&f.policy, &original_policy).unwrap();
    fs::write(f.root.join("yoke-claude-source-hooks.json"), b"{}").unwrap();
    assert!(callback(SourceHookMode::PromptSubmit, &launch, &input, 65536).is_err());
    fs::write(f.root.join("yoke-claude-source-hooks.json"), &settings).unwrap();
    let mut wrong: Value = serde_json::from_str(&launch).unwrap();
    wrong["bindings"]["adapter"]["viewId"] = json!("a".repeat(64));
    assert!(
        callback(
            SourceHookMode::PromptSubmit,
            &wrong.to_string(),
            &input,
            65536
        )
        .is_err()
    );
    let blob = f
        .source
        .join(".yoke/blobs")
        .join(hash("pub fn helper(){ let π = 1; }\r\n"));
    assert!(blob.is_file());
    fs::write(blob, b"changed captured bytes").unwrap();
    assert!(callback(SourceHookMode::PromptSubmit, &launch, &input, 65536).is_err());
}

#[test]
fn legacy_session_profile_bytes_remain_distinct_and_external_files_cannot_be_adopted() {
    let f = fixture(Default::default());
    let d = Deadline::new(30000);
    let old = Selection::claude_session_hooks(
        "owned-old-hooks",
        std::slice::from_ref(&f.root),
        &f.exe,
        &f.policy,
        &f.state,
        &f.source,
        &d,
    )
    .unwrap();
    let old_plan = host_install::plan_install(&old, 1000, &d).unwrap();
    assert_eq!(old_plan.profile, "claude-session-hooks-v1");
    assert_eq!(old_plan.files[0].path, "yoke-claude-hooks.json");
    fs::write(f.root.join("yoke-claude-source-hooks.json"), b"").unwrap();
    assert!(host_install::plan_install(&f.selection, 1000, &d).is_err());
    assert!(fs::read_dir(&f.state).unwrap().next().is_none());
}

#[test]
fn selected_status_bootstrap_preserves_unicode_quoted_bindings_and_uses_no_profile() {
    let mut f = fixture(Default::default());
    let shell = PathBuf::from(std::env::var_os("SystemRoot").unwrap())
        .join("System32/WindowsPowerShell/v1.0/powershell.exe");
    f.selection = f
        .selection
        .with_source_status_shell(&shell, &Deadline::new(30000))
        .unwrap();
    let (settings, launch) = install(&f);
    let settings: Value = serde_json::from_slice(&settings).unwrap();
    let command = settings["statusLine"]["command"].as_str().unwrap();
    let (prefix, encoded) = command
        .split_once(" -NoLogo -NoProfile -NonInteractive -EncodedCommand ")
        .unwrap();
    assert_eq!(prefix, shell.to_str().unwrap().replace('\\', "/"));
    assert!(!command.contains(f.source.to_str().unwrap()));
    let mut request = std::process::Command::new(&shell);
    request
        .args([
            "-NoLogo",
            "-NoProfile",
            "-NonInteractive",
            "-EncodedCommand",
            encoded,
        ])
        .current_dir(&f.source);
    for (key, name) in [
        ("HOME", "private-home"),
        ("USERPROFILE", "private-home"),
        ("LOCALAPPDATA", "private-local"),
        ("APPDATA", "private-roaming"),
        ("TEMP", "private-tmp"),
        ("TMP", "private-tmp"),
    ] {
        let path = f._dir.path().join(name);
        fs::create_dir_all(&path).unwrap();
        request.env(key, path);
    }
    let before = manifest(f._dir.path());
    let input = event(&f, SourceHookMode::Status);
    let command_started = std::time::Instant::now();
    let captured = yoke::process::run_capture(
        request,
        &serde_json::to_vec(&input).unwrap(),
        &Deadline::new(30000),
        yoke::process::CaptureLimits {
            stdin_bytes: 65536,
            stdout_bytes: 4096,
            stderr_bytes: 4096,
        },
    )
    .unwrap();
    assert_eq!(
        captured.exit_code,
        Some(0),
        "{}; stdin={}, output={}, timeout={}, io_error={}, elapsed={:?}; independent library callback: {:?}",
        String::from_utf8_lossy(&captured.stderr),
        captured.stdin_complete,
        captured.output_complete,
        captured.timed_out,
        captured.io_error,
        command_started.elapsed(),
        callback(SourceHookMode::Status, &launch, &input, 4096)
            .map(|_| "ok")
            .map_err(|error| format!("{error:#}"))
    );
    assert!(
        captured.stdin_complete
            && captured.output_complete
            && !captured.timed_out
            && !captured.output_exceeded
            && !captured.io_error
    );
    let native = callback(SourceHookMode::Status, &launch, &input, 4096).unwrap();
    assert_eq!(
        String::from_utf8(captured.stdout)
            .unwrap()
            .replace("\r\n", "\n"),
        String::from_utf8(native).unwrap(),
        "bootstrap stderr: {}",
        String::from_utf8_lossy(&captured.stderr)
    );
    assert_eq!(before, manifest(f._dir.path()));
}

#[test]
fn selected_shell_hash_and_source_boundary_are_checked_before_writes_or_callbacks() {
    let f = fixture_at(Default::default(), "source-ascii");
    let inside = f.source.join("powershell.exe");
    fs::write(&inside, b"owned inert source file; never execute").unwrap();
    let error = f
        .selection
        .with_source_status_shell(&inside, &Deadline::new(30000))
        .err()
        .unwrap();
    assert!(
        error.to_string().contains("outside analyzed source"),
        "unexpected source-shell refusal: {error:#}"
    );
    assert!(fs::read_dir(&f.state).unwrap().next().is_none());

    let mut f = fixture_at(Default::default(), "source-ascii");
    let outside = f._dir.path().join("powershell.exe");
    let original = b"owned inert selected shell; never execute";
    fs::write(&outside, original).unwrap();
    f.selection = f
        .selection
        .with_source_status_shell(&outside, &Deadline::new(30000))
        .unwrap();
    let d = Deadline::new(30000);
    let plan = host_install::plan_install(&f.selection, 1000, &d).unwrap();
    fs::write(&outside, b"changed inert shell").unwrap();
    assert!(
        host_install::apply(
            &f.selection,
            &WriteAuthority::new(&f.selection),
            &plan,
            1001,
            &d
        )
        .is_err()
    );
    assert!(fs::read_dir(&f.state).unwrap().next().is_none());
    assert!(!f.root.join("yoke-claude-source-hooks.json").exists());
    fs::write(&outside, original).unwrap();
    let (_, launch) = install(&f);
    fs::write(&outside, b"changed inert shell again").unwrap();
    let before = manifest(f._dir.path());
    assert!(
        callback(
            SourceHookMode::Status,
            &launch,
            &event(&f, SourceHookMode::Status),
            4096
        )
        .is_err()
    );
    assert_eq!(before, manifest(f._dir.path()));
}
