//! Explicit owned policy directories; no personal configuration or default discovery.
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
};

fn command(root: &Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_yoke"));
    command.current_dir(root);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000);
    }
    command
}
fn invoke(root: &Path, verb: &str, selection: &Path, cap: usize, extra: &[&str]) -> Output {
    command(root)
        .args([
            "--max-bytes",
            &cap.to_string(),
            "session-config",
            verb,
            "--selection",
            selection.to_str().unwrap(),
            "--allow-configuration-read",
            "--allow-policy-projection",
        ])
        .args(extra)
        .output()
        .unwrap()
}
fn selection(root: &Path, files: Vec<Value>, directories: Vec<Value>) -> PathBuf {
    let path = root.join("discovery.json");
    fs::write(&path, serde_json::to_vec(&json!({
        "hostProfile":{"context":"desktop-app","steering":{}}, "files":files,"directories":directories
    })).unwrap()).unwrap();
    path
}
fn good(output: &Output) -> Value {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}
fn refused(output: &Output, marker: &str) {
    assert!(
        !output.status.success() && output.stdout.is_empty(),
        "{output:?}"
    );
    assert!(
        String::from_utf8_lossy(&output.stderr).contains(marker),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn discovered_unicode_crlf_bom_sources_replay_exactly_through_existing_plan_and_export() {
    let owned = tempfile::tempdir().unwrap();
    let root = owned.path().join("policy e\u{301} \u{1f9ed}");
    let modes = root.join("modes");
    let contexts = root.join("contexts");
    fs::create_dir_all(&modes).unwrap();
    fs::create_dir(&contexts).unwrap();
    let global = root.join("global.yml");
    let project = root.join("project.yml");
    fs::write(
        &global,
        b"default_modes: [planning]\r\nexcluded_tools: [read_file]\r\n",
    )
    .unwrap();
    fs::write(&project, "\u{feff}initial_prompt: 'Evidence \u{1f4a1}'\r\n").unwrap();
    fs::write(contexts.join("zeta.YAML"), b"prompt: 'Zeta context'\n").unwrap();
    fs::write(contexts.join("alpha.yml"), b"prompt: 'Alpha context'\n").unwrap();
    fs::write(modes.join("owned.yml"), b"prompt: 'Owned mode'\n").unwrap();
    fs::write(modes.join("notes.txt"), b"not YAML; never projected").unwrap();
    let files = vec![
        json!({"path":global,"target":{"kind":"global"}}),
        json!({"path":project,"target":{"kind":"project","project":"default"}}),
    ];
    let selected = selection(
        &root,
        files,
        vec![
            json!({"path":contexts,"kind":"contexts"}),
            json!({"path":modes,"kind":"modes"}),
        ],
    );
    let first = invoke(&root, "discover", &selected, 4_000_000, &[]);
    let value = good(&first);
    assert_eq!(
        first.stdout,
        invoke(&root, "discover", &selected, 4_000_000, &[]).stdout
    );
    assert_eq!(value["schema"], "native_session_config_discovery_v1");
    assert_eq!(value["discoveryComplete"], true);
    assert_eq!(value["complete"], false);
    for field in ["sourceExecuted", "templatesEvaluated", "storeOpened"] {
        assert_eq!(value[field], false, "{field}");
    }
    assert_eq!(value["networkRequests"], 0);
    assert_eq!(value["modelCalls"], 0);
    let pinned = value["selection"]["files"].as_array().unwrap();
    assert_eq!(pinned.len(), 5);
    assert_eq!(
        pinned[2]["target"],
        json!({"kind":"context","name":"alpha"})
    );
    assert_eq!(pinned[3]["target"], json!({"kind":"context","name":"zeta"}));
    assert_eq!(pinned[4]["target"], json!({"kind":"mode","name":"owned"}));
    for item in pinned {
        let p = Path::new(item["path"].as_str().unwrap());
        assert_eq!(item["sha256"], yoke::model::hash(fs::read(p).unwrap()));
    }
    let replay = root.join("pinned.json");
    fs::write(&replay, serde_json::to_vec(&value["selection"]).unwrap()).unwrap();
    let planned = good(&invoke(&root, "plan", &replay, 4_000_000, &[]));
    assert_eq!(planned, value["projection"]);
    let output = root.join("profile.json");
    good(&invoke(
        &root,
        "export",
        &replay,
        4_000_000,
        &["--output", output.to_str().unwrap(), "--allow-output"],
    ));
    let profile: Value = serde_json::from_slice(&fs::read(&output).unwrap()).unwrap();
    assert_eq!(profile, planned["hostProfile"]);
    let before = fs::read(&output).unwrap();
    let again = invoke(
        &root,
        "export",
        &replay,
        4_000_000,
        &["--output", output.to_str().unwrap(), "--allow-output"],
    );
    assert!(!again.status.success());
    assert_eq!(fs::read(&output).unwrap(), before);
    assert_eq!(
        fs::read(&global).unwrap(),
        b"default_modes: [planning]\r\nexcluded_tools: [read_file]\r\n"
    );
    assert!(!root.join(".yoke").exists());
    assert!(!modes.join(".yoke").exists());
}

#[test]
fn missing_grants_refuse_before_opening_the_selection() {
    let root = tempfile::tempdir().unwrap();
    let missing = root.path().join("not-present.json");
    for flags in [
        vec![],
        vec!["--allow-configuration-read"],
        vec!["--allow-policy-projection"],
    ] {
        let output = command(root.path())
            .args([
                "session-config",
                "discover",
                "--selection",
                missing.to_str().unwrap(),
            ])
            .args(flags)
            .output()
            .unwrap();
        refused(&output, "session_config_host_permission_required");
    }
}

#[test]
fn empty_selected_directory_is_an_observed_empty_result_but_no_selectors_refuse() {
    let root = tempfile::tempdir().unwrap();
    let modes = root.path().join("modes");
    fs::create_dir(&modes).unwrap();
    fs::write(modes.join("README.txt"), b"not YAML").unwrap();
    let p = selection(
        root.path(),
        vec![],
        vec![json!({"path":modes,"kind":"modes"})],
    );
    let value = good(&invoke(root.path(), "discover", &p, 4_000_000, &[]));
    assert_eq!(value["selection"]["files"], json!([]));
    assert_eq!(value["discoveryComplete"], true);
    assert_eq!(value["directories"][0]["yamlFiles"], json!([]));
    let p = selection(root.path(), vec![], vec![]);
    refused(
        &invoke(root.path(), "discover", &p, 4_000_000, &[]),
        "session_config_discovery_selection_required",
    );
}

#[test]
fn duplicate_extensions_or_repeated_directory_targets_refuse_without_choosing_a_winner() {
    let root = tempfile::tempdir().unwrap();
    let modes = root.path().join("modes");
    fs::create_dir(&modes).unwrap();
    fs::write(modes.join("owned.yml"), b"prompt: first\n").unwrap();
    fs::write(modes.join("owned.yaml"), b"prompt: second\n").unwrap();
    let p = selection(
        root.path(),
        vec![],
        vec![json!({"path":modes,"kind":"modes"})],
    );
    refused(
        &invoke(root.path(), "discover", &p, 4_000_000, &[]),
        "session_config_discovery_target_ambiguous",
    );
    fs::remove_file(modes.join("owned.yaml")).unwrap();
    let p = selection(
        root.path(),
        vec![],
        vec![
            json!({"path":modes,"kind":"modes"}),
            json!({"path":modes,"kind":"modes"}),
        ],
    );
    refused(
        &invoke(root.path(), "discover", &p, 4_000_000, &[]),
        "session_config_discovery_target_ambiguous",
    );
}

#[test]
fn existing_pin_is_enforced_and_discovered_selection_becomes_stale_after_source_change() {
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("global.yml");
    fs::write(&source, b"excluded_tools: [read_file]\n").unwrap();
    let p = selection(
        root.path(),
        vec![json!({"path":source,"sha256":"f".repeat(64),"target":{"kind":"global"}})],
        vec![],
    );
    refused(
        &invoke(root.path(), "discover", &p, 4_000_000, &[]),
        "session_config_source_changed",
    );
    let p = selection(
        root.path(),
        vec![json!({"path":source,"target":{"kind":"global"}})],
        vec![],
    );
    let value = good(&invoke(root.path(), "discover", &p, 4_000_000, &[]));
    let pinned = root.path().join("pinned.json");
    fs::write(&pinned, serde_json::to_vec(&value["selection"]).unwrap()).unwrap();
    fs::write(&source, b"excluded_tools: []\n").unwrap();
    refused(
        &invoke(root.path(), "plan", &pinned, 4_000_000, &[]),
        "session_config_source_changed",
    );
    assert!(!root.path().join(".yoke").exists());
}

#[test]
fn discovery_cannot_register_runtime_execute_templates_or_activate_commands() {
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("project.yml");
    let p = selection(
        root.path(),
        vec![json!({"path":source,"target":{"kind":"project","project":"default"}})],
        vec![],
    );
    for (body, marker) in [
        (
            "activation_command: private_owned_canary\n",
            "session_config_activation_command_unavailable",
        ),
        (
            "initial_prompt: '{{ private_owned_canary }}'\n",
            "session_config_template_evaluation_unavailable",
        ),
        (
            "command: private_owned_canary\n",
            "session_config_setting_unsupported",
        ),
    ] {
        fs::write(&source, body).unwrap();
        let output = invoke(root.path(), "discover", &p, 4_000_000, &[]);
        refused(&output, marker);
        assert!(!String::from_utf8_lossy(&output.stderr).contains("private_owned_canary"));
    }
    assert!(!root.path().join(".yoke").exists());
    assert!(!root.path().join("private_owned_canary").exists());
}

#[test]
fn source_byte_and_total_budgets_refuse_without_partial_results() {
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("global.yml");
    fs::write(&source, vec![b'x'; 65_537]).unwrap();
    let p = selection(
        root.path(),
        vec![json!({"path":source,"target":{"kind":"global"}})],
        vec![],
    );
    refused(
        &invoke(root.path(), "discover", &p, 4_000_000, &[]),
        "session_config_discovery_source_refused",
    );
    let modes = root.path().join("modes");
    fs::create_dir(&modes).unwrap();
    let content = format!("#{}\nprompt: owned\n", "x".repeat(65_000));
    for n in 0..31 {
        fs::write(modes.join(format!("owned{n:02}.yml")), &content).unwrap();
    }
    let p = selection(
        root.path(),
        vec![],
        vec![json!({"path":modes,"kind":"modes"})],
    );
    refused(
        &invoke(root.path(), "discover", &p, 4_000_000, &[]),
        "session_config_source_capacity_exceeded",
    );
}

#[test]
fn complete_candidate_count_is_admitted_before_any_payload_or_hardlink_read() {
    let root = tempfile::tempdir().unwrap();
    let modes = root.path().join("modes");
    fs::create_dir(&modes).unwrap();
    let canary = root.path().join("canary");
    fs::write(&canary, b"private_owned_canary").unwrap();
    fs::hard_link(&canary, modes.join("owned000.yml")).unwrap();
    for n in 1..269 {
        fs::write(modes.join(format!("owned{n:03}.yml")), b"prompt: owned\n").unwrap();
    }
    let p = selection(
        root.path(),
        vec![],
        vec![json!({"path":modes,"kind":"modes"})],
    );
    refused(
        &invoke(root.path(), "discover", &p, 4_000_000, &[]),
        "session_config_file_capacity_exceeded",
    );
    assert_eq!(fs::read(canary).unwrap(), b"private_owned_canary");
}

#[test]
fn directory_entry_budget_includes_non_yaml_entries() {
    let root = tempfile::tempdir().unwrap();
    let modes = root.path().join("modes");
    fs::create_dir(&modes).unwrap();
    for n in 0..1025 {
        fs::write(modes.join(format!("noise{n:04}.txt")), b"").unwrap();
    }
    let p = selection(
        root.path(),
        vec![],
        vec![json!({"path":modes,"kind":"modes"})],
    );
    refused(
        &invoke(root.path(), "discover", &p, 4_000_000, &[]),
        "session_config_discovery_entry_capacity",
    );
}

#[test]
fn output_budget_and_unknown_selector_fields_cannot_publish_partial_selection() {
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("project.yml");
    fs::write(&source, format!("initial_prompt: '{}'\n", "a".repeat(2000))).unwrap();
    let p = selection(
        root.path(),
        vec![json!({"path":source,"target":{"kind":"project","project":"default"}})],
        vec![],
    );
    let output = invoke(root.path(), "discover", &p, 1024, &[]);
    assert!(!output.status.success() && output.stdout.is_empty());
    let error = String::from_utf8_lossy(&output.stderr);
    assert!(
        error.contains("budget") || error.contains("capacity") || error.contains("exceed"),
        "{error}"
    );
    fs::write(
        &p,
        br#"{"hostProfile":{},"directories":[],"outsideRoot":"private_owned_canary"}"#,
    )
    .unwrap();
    refused(
        &invoke(root.path(), "discover", &p, 4_000_000, &[]),
        "session_config_discovery_selection_invalid",
    );
    assert!(!root.path().join(".yoke").exists());
}

#[test]
fn non_directory_and_hardlinked_yaml_sources_refuse() {
    let root = tempfile::tempdir().unwrap();
    let file = root.path().join("not-a-directory");
    fs::write(&file, b"owned").unwrap();
    let p = selection(
        root.path(),
        vec![],
        vec![json!({"path":file,"kind":"modes"})],
    );
    refused(
        &invoke(root.path(), "discover", &p, 4_000_000, &[]),
        "session_config_discovery_directory_refused",
    );
    let modes = root.path().join("modes");
    fs::create_dir(&modes).unwrap();
    let source = root.path().join("source.yml");
    fs::write(&source, b"prompt: owned\n").unwrap();
    fs::hard_link(&source, modes.join("owned.yml")).unwrap();
    let p = selection(
        root.path(),
        vec![],
        vec![json!({"path":modes,"kind":"modes"})],
    );
    refused(
        &invoke(root.path(), "discover", &p, 4_000_000, &[]),
        "session_config_discovery_source_refused",
    );
}

#[test]
fn linked_directory_cannot_discover_foreign_yaml() {
    let root = tempfile::tempdir().unwrap();
    let target = root.path().join("target");
    let link = root.path().join("alias");
    fs::create_dir(&target).unwrap();
    fs::write(target.join("owned.yml"), b"prompt: private_owned_canary\n").unwrap();
    #[cfg(windows)]
    {
        let mut node = Command::new("node");
        use std::os::windows::process::CommandExt;
        node.creation_flags(0x0800_0000);
        let result = node
            .args([
                "-e",
                "require('node:fs').symlinkSync(process.argv[1],process.argv[2],'junction')",
                target.to_str().unwrap(),
                link.to_str().unwrap(),
            ])
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
    }
    #[cfg(unix)]
    std::os::unix::fs::symlink(&target, &link).unwrap();
    let p = selection(
        root.path(),
        vec![],
        vec![json!({"path":link,"kind":"modes"})],
    );
    let result = invoke(root.path(), "discover", &p, 4_000_000, &[]);
    refused(&result, "session_config_discovery_directory_refused");
    assert!(!String::from_utf8_lossy(&result.stderr).contains("private_owned_canary"));
}
