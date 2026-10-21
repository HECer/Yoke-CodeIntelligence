#![cfg(windows)]
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::{Duration, Instant},
};
use yoke::{
    capture::Deadline,
    host_install::{self, Selection, WriteAuthority},
};
struct Fixture {
    dir: tempfile::TempDir,
    source: PathBuf,
    root: PathBuf,
    state: PathBuf,
    exe: PathBuf,
    policy: PathBuf,
    selection: Selection,
}
fn fixture(real: bool) -> Fixture {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("source space ' \u{03c0}");
    let root = dir.path().join("client \u{1f30d}");
    let state = dir.path().join("private state");
    for p in [&source, &root, &state] {
        fs::create_dir(p).unwrap();
    }
    fs::write(source.join("owned.rs"), b"fn owned(){}\r\n").unwrap();
    let exe = if real {
        PathBuf::from(env!("CARGO_BIN_EXE_yoke"))
    } else {
        let p = dir.path().join("never run.exe");
        fs::write(&p, b"owned fixture, never executed").unwrap();
        p
    };
    let policy = dir.path().join("policy.json");
    fs::write(&policy, b"{}").unwrap();
    let selection = Selection::claude_session_hooks(
        "owned-hooks",
        std::slice::from_ref(&root),
        &exe,
        &policy,
        &state,
        &source,
        &Deadline::new(30000),
    )
    .unwrap();
    Fixture {
        dir,
        source,
        root,
        state,
        exe,
        policy,
        selection,
    }
}
fn manifest(root: &Path) -> BTreeMap<PathBuf, String> {
    let mut values = BTreeMap::new();
    let mut todo = vec![root.to_path_buf()];
    while let Some(path) = todo.pop() {
        for entry in fs::read_dir(path).unwrap() {
            let p = entry.unwrap().path();
            if p.is_dir() {
                todo.push(p)
            } else {
                values.insert(
                    p.strip_prefix(root).unwrap().into(),
                    yoke::model::hash(fs::read(p).unwrap()),
                );
            }
        }
    }
    values
}
fn install(f: &Fixture) -> Vec<u8> {
    let d = Deadline::new(60000);
    let plan = host_install::plan_install(&f.selection, 1000, &d).unwrap();
    assert_eq!(plan.profile, "claude-session-hooks-v1");
    assert_eq!(plan.files[0].path, "yoke-claude-hooks.json");
    assert!(fs::read_dir(&f.state).unwrap().next().is_none());
    host_install::apply(
        &f.selection,
        &WriteAuthority::new(&f.selection),
        &plan,
        1001,
        &d,
    )
    .unwrap();
    fs::read(f.root.join("yoke-claude-hooks.json")).unwrap()
}
fn launch(settings: &[u8]) -> String {
    let v: Value = serde_json::from_slice(settings).unwrap();
    v["hooks"]["SessionStart"][0]["hooks"][0]["args"][6]
        .as_str()
        .unwrap()
        .into()
}
fn event(f: &Fixture, source: &str) -> Vec<u8> {
    serde_json::to_vec(&json!({"hook_event_name":"SessionStart","source":source,"cwd":f.source,"session_id":"untrusted session","transcript_path":"../../private/no-read.jsonl","future":{"command":"never execute","permissions":true}})).unwrap()
}
fn callback(launch: &str, input: &[u8], max: usize) -> anyhow::Result<Vec<u8>> {
    host_install::claude_session_start(launch.as_bytes(), input, max, &Deadline::new(30000))
}

#[test]
fn literal_exec_settings_roundtrip_and_advisory_context_ignore_untrusted_fields() {
    let f = fixture(false);
    fs::write(f.root.join("settings.json"), b"{\"human\":true}\r\n").unwrap();
    let settings = install(&f);
    let v: Value = serde_json::from_slice(&settings).unwrap();
    let h = &v["hooks"]["SessionStart"][0]["hooks"][0];
    assert_eq!(h["command"], f.exe.to_str().unwrap());
    assert!(h["args"].is_array());
    assert!(h.get("shell").is_none());
    assert!(v.get("permissions").is_none());
    let launch = launch(&settings);
    let before = manifest(f.dir.path());
    let mut prior = None;
    for source in ["startup", "resume", "clear", "compact"] {
        let output = callback(&launch, &event(&f, source), 16000).unwrap();
        let value: Value = serde_json::from_slice(&output).unwrap();
        assert_eq!(value.as_object().unwrap().len(), 1);
        assert!(value.get("initialUserMessage").is_none());
        assert!(value.get("decision").is_none());
        let c = value["hookSpecificOutput"]["additionalContext"]
            .as_str()
            .unwrap();
        let context: Value = serde_json::from_str(c).unwrap();
        assert_eq!(context["snapshotStatus"], "not_checked");
        assert_eq!(context["permissionsGranted"], false);
        assert_eq!(context["observedExecution"], false);
        assert!(
            !c.contains("untrusted session")
                && !c.contains("no-read")
                && !c.contains("never execute")
                && !c.contains("source space")
        );
        if let Some(prior) = prior {
            assert_eq!(output, prior)
        }
        prior = Some(output);
    }
    assert_eq!(before, manifest(f.dir.path()));
    assert!(!f.source.join(".yoke").exists());
    let d = Deadline::new(30000);
    let remove = host_install::plan_uninstall(&f.selection, 1002, &d).unwrap();
    host_install::apply(
        &f.selection,
        &WriteAuthority::new(&f.selection),
        &remove,
        1003,
        &d,
    )
    .unwrap();
    assert!(!f.root.join("yoke-claude-hooks.json").exists());
    assert_eq!(
        fs::read(f.root.join("settings.json")).unwrap(),
        b"{\"human\":true}\r\n"
    );
    assert!(callback(&launch, &event(&f, "startup"), 16000).is_err());
}
#[test]
fn strict_event_protocol_duplicate_keys_depth_and_exact_newline_budgets() {
    let f = fixture(false);
    let launch = launch(&install(&f));
    let good = event(&f, "startup");
    let output = callback(&launch, &good, 16000).unwrap();
    let before = manifest(f.dir.path());
    assert_eq!(callback(&launch, &good, output.len()).unwrap(), output);
    assert!(callback(&launch, &good, output.len() - 1).is_err());
    assert!(callback(&launch, &good, 0).is_err());
    let v: Value = serde_json::from_slice(&good).unwrap();
    let mut bad = vec![
        b"{}".to_vec(),
        b"null".to_vec(),
        b"{\"hook_event_name\":\"SessionStart\",\"hook_event_name\":\"SessionStart\"}".to_vec(),
        [vec![b' '; 65537], good.clone()].concat(),
        [vec![b'['; 66], vec![b']'; 66]].concat(),
    ];
    for (key, value) in [
        ("hook_event_name", json!("PreToolUse")),
        ("source", json!("malicious secret")),
        ("cwd", json!(f.dir.path())),
        ("cwd", Value::Null),
    ] {
        let mut m = v.clone();
        m[key] = value;
        bad.push(serde_json::to_vec(&m).unwrap());
    }
    bad.push(
        good[..good.len() - 1]
            .iter()
            .copied()
            .chain(b",\"future\":{}}".iter().copied())
            .collect(),
    );
    for input in bad {
        assert!(callback(&launch, &input, 16000).is_err());
    }
    let mut wrong: Value = serde_json::from_str(&launch).unwrap();
    wrong["bindings"]["adapter"]["kind"] = json!("codex_mcp_v1");
    assert!(callback(&wrong.to_string(), &good, 16000).is_err());
    assert!(callback(&(launch.clone() + " trailing"), &good, 16000).is_err());
    assert!(callback(&"x".repeat(16001), &good, 16000).is_err());
    assert_eq!(before, manifest(f.dir.path()));
}
#[test]
fn existing_empty_identical_and_foreign_settings_are_never_adopted() {
    for bytes in [b"".to_vec(), b"{}".to_vec(), b"{\"hooks\":{}}\r\n".to_vec()] {
        let f = fixture(false);
        fs::write(f.root.join("yoke-claude-hooks.json"), bytes).unwrap();
        let before = manifest(f.dir.path());
        assert!(host_install::plan_install(&f.selection, 1000, &Deadline::new(30000)).is_err());
        assert_eq!(before, manifest(f.dir.path()));
    }
    let f = fixture(false);
    let d = Deadline::new(30000);
    let p = host_install::plan_install(&f.selection, 1000, &d).unwrap();
    let bytes = yoke::host_config::apply_patch(&[], &p.files[0].patch, &d).unwrap();
    fs::write(f.root.join("yoke-claude-hooks.json"), bytes).unwrap();
    let before = manifest(f.dir.path());
    assert!(host_install::plan_install(&f.selection, 1000, &d).is_err());
    assert!(
        host_install::apply(
            &f.selection,
            &WriteAuthority::new(&f.selection),
            &p,
            1001,
            &d
        )
        .is_err()
    );
    assert_eq!(before, manifest(f.dir.path()));
}
#[test]
fn modified_owned_file_and_stale_policy_executable_source_refuse_without_writes() {
    let f = fixture(false);
    let launch = launch(&install(&f));
    let good = event(&f, "startup");
    let path = f.root.join("yoke-claude-hooks.json");
    let original = fs::read(&path).unwrap();
    fs::write(&path, [original.clone(), b" ".to_vec()].concat()).unwrap();
    let before = manifest(f.dir.path());
    assert!(callback(&launch, &good, 16000).is_err());
    assert!(host_install::plan_uninstall(&f.selection, 1002, &Deadline::new(30000)).is_err());
    assert_eq!(before, manifest(f.dir.path()));
    fs::write(&path, &original).unwrap();
    for path in [&f.policy, &f.exe] {
        let original = fs::read(path).unwrap();
        fs::write(path, b"changed host bytes").unwrap();
        let before = manifest(f.dir.path());
        assert!(callback(&launch, &good, 16000).is_err());
        assert_eq!(before, manifest(f.dir.path()));
        fs::write(path, original).unwrap();
    }
    let moved = f.dir.path().join("moved-source");
    fs::rename(&f.source, &moved).unwrap();
    fs::create_dir(&f.source).unwrap();
    let before = manifest(f.dir.path());
    assert!(callback(&launch, &good, 16000).is_err());
    assert_eq!(before, manifest(f.dir.path()));
}
#[test]
fn explicit_hook_paths_refuse_interpolation_nesting_traversal_and_aliases() {
    let f = fixture(false);
    let d = Deadline::new(30000);
    for root in [
        f.source.clone(),
        f.root.join(".."),
        f.root.join("${CLAUDE_PROJECT_DIR}"),
    ] {
        if !root.exists() {
            fs::create_dir(&root).unwrap();
        }
        assert!(
            Selection::claude_session_hooks(
                "bad",
                &[root],
                &f.exe,
                &f.policy,
                &f.state,
                &f.source,
                &d
            )
            .is_err()
        );
    }
    let state = f.dir.path().join("${CLAUDE_PLUGIN_ROOT}");
    fs::create_dir(&state).unwrap();
    assert!(
        Selection::claude_session_hooks(
            "bad",
            std::slice::from_ref(&f.root),
            &f.exe,
            &f.policy,
            &state,
            &f.source,
            &d
        )
        .is_err()
    );
    assert!(
        Selection::claude_session_hooks(
            "bad",
            std::slice::from_ref(&f.root),
            &f.exe,
            &f.exe,
            &f.state,
            &f.source,
            &d
        )
        .is_err()
    );
    assert!(!f.source.join(".yoke").exists());
    assert!(fs::read_dir(&f.state).unwrap().next().is_none());
}
#[test]
fn actual_cli_advisory_static_refusal_no_source_engine_and_stalled_pipe_deadline() {
    let f = fixture(true);
    let launch = launch(&install(&f));
    let before = manifest(f.dir.path());
    for input in [
        event(&f, "startup"),
        b"{\"secret_marker\":\"never echo me\"}".to_vec(),
    ] {
        let mut child = Command::new(&f.exe)
            .args(["claude-session-start", "--launch-json", &launch])
            .current_dir(&f.source)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        child.stdin.take().unwrap().write_all(&input).unwrap();
        let out = child.wait_with_output().unwrap();
        if input == event(&f, "startup") {
            assert!(
                out.status.success(),
                "{}",
                String::from_utf8_lossy(&out.stderr)
            );
            assert_eq!(out.stdout, callback(&launch, &input, 32000).unwrap());
        } else {
            assert!(!out.status.success());
            assert!(out.stdout.is_empty());
            assert!(String::from_utf8_lossy(&out.stderr).contains("claude_session_start_refused"));
            assert!(!String::from_utf8_lossy(&out.stderr).contains("never echo me"));
        }
    }
    let start = Instant::now();
    let mut child = Command::new(&f.exe)
        .args([
            "--timeout",
            "100",
            "claude-session-start",
            "--launch-json",
            &launch,
        ])
        .current_dir(&f.source)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let _open = child.stdin.take().unwrap();
    while child.try_wait().unwrap().is_none() {
        if start.elapsed() > Duration::from_secs(5) {
            child.kill().unwrap();
            panic!("Hook stdin deadline blocked")
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    let out = child.wait_with_output().unwrap();
    assert!(!out.status.success() && out.stdout.is_empty());
    assert!(start.elapsed() < Duration::from_secs(5));
    assert_eq!(before, manifest(f.dir.path()));
    assert!(!f.source.join(".yoke").exists());
}
