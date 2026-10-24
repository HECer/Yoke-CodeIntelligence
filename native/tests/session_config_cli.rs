//! Owned host-policy/source fixtures; no personal configuration or process execution.
use serde_json::{Value, json};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};
fn command(cwd: &Path) -> Command {
    let mut c = Command::new(env!("CARGO_BIN_EXE_yoke"));
    c.current_dir(cwd);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        c.creation_flags(0x0800_0000);
    }
    c
}
fn invoke(cwd: &Path, args: &[&str]) -> std::process::Output {
    command(cwd).args(args).output().unwrap()
}
fn select(root: &Path, files: Vec<Value>) -> PathBuf {
    let p = root.join("selection.json");
    fs::write(
        &p,
        serde_json::to_vec(
            &json!({"hostProfile":{"context":"desktop-app","steering":{}},"files":files}),
        )
        .unwrap(),
    )
    .unwrap();
    p
}
fn selected(path: &Path, target: Value) -> Value {
    json!({"path":path,"sha256":yoke::model::hash(fs::read(path).unwrap()),"target":target})
}
fn config_args<'a>(verb: &'a str, selection: &'a Path) -> Vec<&'a str> {
    vec![
        "--max-bytes",
        "4000000",
        "session-config",
        verb,
        "--selection",
        selection.to_str().unwrap(),
        "--allow-configuration-read",
        "--allow-policy-projection",
    ]
}
fn mcp(cwd: &Path, source: &Path, host: &Path) -> Vec<u8> {
    let mut child = command(cwd)
        .args([
            "--root",
            source.to_str().unwrap(),
            "--max-bytes",
            "4000000",
            "serve-session",
            "--id",
            "owned",
            "--profile",
            "serena",
            "--host-profile",
            host.to_str().unwrap(),
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let messages = [
        json!({"jsonrpc":"2.0","id":1,"method":"initialize"}),
        json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}),
        json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"get_current_config","arguments":{}}}),
    ];
    {
        let mut input = child.stdin.take().unwrap();
        for message in messages {
            writeln!(input, "{message}").unwrap();
        }
    }
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    output.stdout
}

#[test]
fn actual_standard_native_defaults_export_same_profile_and_report_unavailable_extensions() {
    let root = tempfile::tempdir().unwrap();
    let full = root.path().join("full.yml");
    let raw = include_bytes!("fixtures/native-project-defaults.yml");
    fs::write(&full, raw).unwrap();
    let omitted = [
        "encoding:",
        "activation_command:",
        "activation_command_timeout:",
        "language_backend:",
        "ls_additional_workspace_folders:",
        "added_modes:",
    ];
    let reduced = root.path().join("reduced.yml");
    let text = std::str::from_utf8(raw)
        .unwrap()
        .split_inclusive('\n')
        .filter(|line| !omitted.iter().any(|prefix| line.starts_with(prefix)))
        .collect::<String>();
    fs::write(&reduced, text).unwrap();
    let selection_full = root.path().join("full-selection.json");
    let path = select(
        root.path(),
        vec![selected(
            &full,
            json!({"kind":"project","project":"default"}),
        )],
    );
    fs::copy(path, &selection_full).unwrap();
    let selection_reduced = select(
        root.path(),
        vec![selected(
            &reduced,
            json!({"kind":"project","project":"default"}),
        )],
    );
    let plan = invoke(root.path(), &config_args("plan", &selection_full));
    assert!(
        plan.status.success(),
        "{}",
        String::from_utf8_lossy(&plan.stderr)
    );
    let value: Value = serde_json::from_slice(&plan.stdout).unwrap();
    assert_eq!(value["complete"], false);
    assert_eq!(value["sourceExecuted"], false);
    assert_eq!(
        value["sources"][0]["nativeEquivalentDefaults"]["encoding"],
        "utf-8"
    );
    assert_eq!(
        value["sources"][0]["nativeDefaultCapabilities"]["activationCommands"],
        false
    );
    assert_eq!(
        value["sources"][0]["originalSha256"],
        yoke::model::hash(raw)
    );
    let a = root.path().join("a.json");
    let b = root.path().join("b.json");
    for (selection, output) in [(&selection_full, &a), (&selection_reduced, &b)] {
        let mut args = config_args("export", selection);
        args.extend(["--output", output.to_str().unwrap(), "--allow-output"]);
        let result = invoke(root.path(), &args);
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
    }
    assert_eq!(fs::read(&a).unwrap(), fs::read(&b).unwrap());
    assert_eq!(fs::read(&full).unwrap(), raw);
    assert!(!root.path().join(".yoke").exists());
    fs::write(&full, b"activation_command: owned_private_canary\n").unwrap();
    select(
        root.path(),
        vec![selected(
            &full,
            json!({"kind":"project","project":"default"}),
        )],
    );
    let result = invoke(root.path(), &config_args("plan", &selection_reduced));
    assert!(!result.status.success() && result.stdout.is_empty());
    let error = String::from_utf8_lossy(&result.stderr);
    assert!(
        error.contains("session_config_activation_command_unavailable")
            && !error.contains("owned_private_canary")
    );
    assert!(!root.path().join(".yoke").exists());
}

#[test]
fn actual_yaml_plan_and_create_only_profile_match_existing_json_session_and_mcp() {
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("source");
    fs::create_dir(&source).unwrap();
    let raw_source = "// owned \u{1f4a1}\r\nfn source() {}\r\n".as_bytes();
    fs::write(source.join("main.rs"), raw_source).unwrap();
    let global = root.path().join("global.yml");
    fs::write(
        &global,
        b"default_modes: [planning]\r\nexcluded_tools: [read_file]\r\n",
    )
    .unwrap();
    let project = root.path().join("project.yml");
    fs::write(&project, "initial_prompt: |\r\n  Evidence \u{1f4a1}.\r\n").unwrap();
    let selection = select(
        root.path(),
        vec![
            selected(&global, json!({"kind":"global"})),
            selected(&project, json!({"kind":"project","project":"default"})),
        ],
    );
    let expected = root.path().join("expected.json");
    fs::write(&expected,serde_json::to_vec(&json!({"context":"desktop-app","steering":{"global":{"defaultModes":["planning"],"excludedTools":["read_file"]},"projects":{"default":{"project":{"instructions":"Evidence \u{1f4a1}.\n"}}}}})).unwrap()).unwrap();
    let requests = root.path().join("requests.json");
    fs::write(&requests, br#"[{"operation":"configuration","args":{}}]"#).unwrap();
    let baseline = invoke(
        root.path(),
        &[
            "--root",
            source.to_str().unwrap(),
            "session",
            "--id",
            "owned",
            "--host-profile",
            expected.to_str().unwrap(),
            "--requests",
            requests.to_str().unwrap(),
        ],
    );
    assert!(baseline.status.success());
    let old_mcp = mcp(root.path(), &source, &expected);
    let plan = invoke(root.path(), &config_args("plan", &selection));
    assert!(
        plan.status.success(),
        "{}",
        String::from_utf8_lossy(&plan.stderr)
    );
    let value: Value = serde_json::from_slice(&plan.stdout).unwrap();
    assert_eq!(value["sourceExecuted"], false);
    assert_eq!(value["templatesEvaluated"], false);
    assert_eq!(
        value["sources"][0]["originalSha256"],
        yoke::model::hash(fs::read(&global).unwrap())
    );
    assert!(!root.path().join(".yoke").exists());
    let output = root.path().join("profile.json");
    let mut args = config_args("export", &selection);
    args.extend(["--output", output.to_str().unwrap(), "--allow-output"]);
    let exported = invoke(root.path(), &args);
    assert!(
        exported.status.success(),
        "{}",
        String::from_utf8_lossy(&exported.stderr)
    );
    let replay = invoke(
        root.path(),
        &[
            "--root",
            source.to_str().unwrap(),
            "session",
            "--id",
            "owned",
            "--host-profile",
            output.to_str().unwrap(),
            "--requests",
            requests.to_str().unwrap(),
        ],
    );
    assert!(
        replay.status.success(),
        "{}",
        String::from_utf8_lossy(&replay.stderr)
    );
    assert_eq!(baseline.stdout, replay.stdout);
    assert_eq!(mcp(root.path(), &source, &output), old_mcp);
    assert_eq!(fs::read(source.join("main.rs")).unwrap(), raw_source);
    let original = fs::read(&output).unwrap();
    assert!(!invoke(root.path(), &args).status.success());
    assert_eq!(fs::read(&output).unwrap(), original);
}

#[test]
fn grants_stale_sources_unknown_settings_and_foreign_outputs_refuse_without_effects() {
    let root = tempfile::tempdir().unwrap();
    let input = root.path().join("global.yml");
    fs::write(&input, b"excluded_tools: [read_file]\n").unwrap();
    let selection = select(
        root.path(),
        vec![selected(&input, json!({"kind":"global"}))],
    );
    let output = root.path().join("profile.json");
    let mut args = config_args("export", &selection);
    args.extend(["--output", output.to_str().unwrap(), "--allow-output"]);
    for omitted in [
        "--allow-configuration-read",
        "--allow-policy-projection",
        "--allow-output",
    ] {
        let reduced: Vec<_> = args.iter().copied().filter(|v| *v != omitted).collect();
        let r = invoke(root.path(), &reduced);
        assert!(!r.status.success() && r.stdout.is_empty());
        assert!(
            String::from_utf8_lossy(&r.stderr).contains("session_config_host_permission_required")
        );
    }
    fs::write(&output, b"foreign original").unwrap();
    assert!(!invoke(root.path(), &args).status.success());
    assert_eq!(fs::read(&output).unwrap(), b"foreign original");
    fs::remove_file(&output).unwrap();
    fs::write(&input, b"excluded_tools: []\n").unwrap();
    let r = invoke(root.path(), &args);
    assert!(!r.status.success() && r.stdout.is_empty() && !output.exists());
    fs::write(&input, b"activation_command: owned_private_canary\n").unwrap();
    select(
        root.path(),
        vec![selected(&input, json!({"kind":"global"}))],
    );
    let r = invoke(root.path(), &args);
    assert!(!r.status.success() && r.stdout.is_empty() && !output.exists());
    assert!(!String::from_utf8_lossy(&r.stderr).contains("owned_private_canary"));
    assert!(!root.path().join(".yoke").exists());
}

#[test]
fn selected_hardlinks_relative_paths_and_windows_junction_ancestors_refuse() {
    let root = tempfile::tempdir().unwrap();
    let input = root.path().join("global.yml");
    fs::write(&input, b"{}\n").unwrap();
    let alias = root.path().join("hard.yml");
    fs::hard_link(&input, &alias).unwrap();
    let selection = select(
        root.path(),
        vec![selected(&alias, json!({"kind":"global"}))],
    );
    let r = invoke(root.path(), &config_args("plan", &selection));
    assert!(!r.status.success() && r.stdout.is_empty());
    assert_eq!(fs::read(&input).unwrap(), b"{}\n");
    let relative = invoke(
        root.path(),
        &[
            "session-config",
            "plan",
            "--selection",
            "selection.json",
            "--allow-configuration-read",
            "--allow-policy-projection",
        ],
    );
    assert!(!relative.status.success());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let real = root.path().join("real");
        fs::create_dir(&real).unwrap();
        let file = real.join("global.yml");
        fs::write(&file, b"{}\n").unwrap();
        let junction = root.path().join("junction");
        let result = Command::new("cmd.exe")
            .args(["/d", "/c", "mklink", "/J"])
            .arg(&junction)
            .arg(&real)
            .creation_flags(0x0800_0000)
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        select(
            root.path(),
            vec![selected(
                &junction.join("global.yml"),
                json!({"kind":"global"}),
            )],
        );
        let r = invoke(root.path(), &config_args("plan", &selection));
        assert!(!r.status.success() && r.stdout.is_empty());
        assert_eq!(fs::read(file).unwrap(), b"{}\n");
    }
    assert!(!root.path().join(".yoke").exists());
}

#[test]
fn reply_budget_is_admitted_before_create_only_profile_publication() {
    let root = tempfile::tempdir().unwrap();
    let input = root.path().join("mode.yml");
    fs::write(&input, b"prompt: Owned bounded literal.\n").unwrap();
    let files = (0..16)
        .map(|i| selected(&input, json!({"kind":"mode","name":format!("owned-{i}")})))
        .collect();
    let selection = select(root.path(), files);
    let output = root.path().join("profile.json");
    let r = invoke(
        root.path(),
        &[
            "--max-bytes",
            "1024",
            "session-config",
            "export",
            "--selection",
            selection.to_str().unwrap(),
            "--output",
            output.to_str().unwrap(),
            "--allow-configuration-read",
            "--allow-policy-projection",
            "--allow-output",
        ],
    );
    assert!(!r.status.success() && r.stdout.is_empty() && !output.exists());
    assert_eq!(
        fs::read(input).unwrap(),
        b"prompt: Owned bounded literal.\n"
    );
    assert!(!root.path().join(".yoke").exists());
}
