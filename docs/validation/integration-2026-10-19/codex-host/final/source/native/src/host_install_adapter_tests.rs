use super::*;
fn fixture() -> (tempfile::TempDir, Selection, PathBuf, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("Codex config \u{1f4a1}");
    let source = dir.path().join("source \u{1f4a1}");
    let state = dir.path().join("state");
    for p in [&root, &source, &state] {
        fs::create_dir(p).unwrap();
    }
    let exe = dir.path().join("owned executable.exe");
    let policy = dir.path().join("owned policy.json");
    fs::write(&exe, b"owned fixture executable - never run").unwrap();
    fs::write(&policy, b"{}").unwrap();
    let selection = Selection::codex_mcp(
        "owned-fixture",
        std::slice::from_ref(&root),
        &exe,
        &policy,
        &state,
        &source,
        &Deadline::new(30000),
    )
    .unwrap();
    (dir, selection, root, source)
}
#[test]
fn generic_serialization_and_installation_identity_retain_the_released_shape() {
    let (_dir, codex, root, _source) = fixture();
    let d = Deadline::new(30000);
    let AdapterBinding::CodexMcpV1 {
        executable_path,
        policy_path,
        ..
    } = codex.bindings.adapter.as_ref().unwrap();
    let generic = Selection::new(
        "owned-fixture",
        &[root],
        Path::new(executable_path),
        Path::new(policy_path),
        Path::new(&codex.bindings.state_root.path).parent().unwrap(),
        &d,
    );
    // The internal Volume GUID spelling must never become a fresh user selector.
    assert!(generic.is_err());
    let mut bindings = codex.bindings.clone();
    bindings.adapter = None;
    let generic = Selection {
        installation: codex.installation.clone(),
        bindings,
    };
    assert_eq!(
        generic.installation_id(),
        identity(&(
            PROFILE_ID,
            &generic.installation,
            &generic.bindings.roots,
            &generic.bindings.state_root
        ))
    );
    let legacy = serde_json::to_value(&generic.bindings).unwrap();
    assert!(legacy.get("adapter").is_none());
    let restored: Bindings = serde_json::from_value(legacy.clone()).unwrap();
    assert_eq!(serde_json::to_value(restored).unwrap(), legacy);
    assert_ne!(generic.installation_id(), codex.installation_id());
    assert_eq!(generic.target_filename(), "AGENTS.md");
    assert_eq!(codex.profile_id(), adapter::CODEX_PROFILE);
}
#[test]
fn codex_plan_binds_exact_launch_data_without_source_state_or_default_authority() {
    let (_dir, selection, root, source) = fixture();
    let human = "# human \u{1f4a1}\r\n[mcp_servers.other]\r\ncommand = \"unrelated\"\r\n";
    fs::write(root.join("config.toml"), human).unwrap();
    let d = Deadline::new(30000);
    let plan = plan_install(&selection, 1000, &d).unwrap();
    assert_eq!(plan.profile, adapter::CODEX_PROFILE);
    assert_eq!(plan.files[0].path, "config.toml");
    let raw = host_config::apply_patch(human.as_bytes(), &plan.files[0].patch, &d).unwrap();
    assert!(raw.starts_with(human.as_bytes()));
    let value: toml::Value = toml::from_str(std::str::from_utf8(&raw).unwrap()).unwrap();
    let server = &value["mcp_servers"]["yoke_owned-fixture"];
    let AdapterBinding::CodexMcpV1 {
        source_path,
        executable_path,
        policy_path,
        ..
    } = selection.bindings.adapter.as_ref().unwrap();
    assert_eq!(server["command"].as_str(), Some(executable_path.as_str()));
    assert_eq!(server["cwd"].as_str(), Some(source_path.as_str()));
    let args = server["args"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(
        args,
        [
            "--root",
            source_path,
            "serve-session",
            "--id",
            "owned-fixture",
            "--host-profile",
            policy_path,
            "--profile",
            "native"
        ]
    );
    assert_eq!(server["env_vars"].as_array().unwrap().len(), 0);
    let tools = server["enabled_tools"].as_array().unwrap();
    assert!(
        !tools
            .iter()
            .any(|v| v.as_str().unwrap().contains("apply") || v.as_str() == Some("ci_session"))
    );
    assert!(!state(&selection).exists());
    assert!(!source.join(".yoke").exists());
    assert_eq!(
        fs::read(root.join("config.toml")).unwrap(),
        human.as_bytes()
    );
}
#[test]
fn codex_install_update_and_uninstall_preserve_human_tables_and_original_empty_files() {
    let (_dir, selection, root, _source) = fixture();
    let d = Deadline::new(60000);
    let original = b"# existing config\r\n";
    let path = root.join("config.toml");
    fs::write(&path, original).unwrap();
    let plan = plan_install(&selection, 1000, &d).unwrap();
    let authority = WriteAuthority::new(&selection);
    let installed = apply(&selection, &authority, &plan, 1001, &d).unwrap();
    assert_eq!(installed.profile, adapter::CODEX_PROFILE);
    assert_eq!(installed.files[0].path, "config.toml");
    let mut current = fs::read(&path).unwrap();
    current.extend_from_slice(b"\r\n[mcp_servers.human]\r\ncommand = \"human\"\r\n");
    fs::write(&path, &current).unwrap();
    let update = plan_install(&selection, 1002, &d).unwrap();
    apply(&selection, &authority, &update, 1003, &d).unwrap();
    let remove = plan_uninstall(&selection, 1004, &d).unwrap();
    apply(&selection, &authority, &remove, 1005, &d).unwrap();
    let remaining = fs::read(&path).unwrap();
    assert!(remaining.starts_with(original));
    assert!(remaining.ends_with(b"[mcp_servers.human]\r\ncommand = \"human\"\r\n"));
    assert!(
        !std::str::from_utf8(&remaining)
            .unwrap()
            .contains("yoke_owned-fixture")
    );
    let (_dir, selection, root, _source) = fixture();
    let path = root.join("config.toml");
    fs::write(&path, []).unwrap();
    let authority = WriteAuthority::new(&selection);
    let plan = plan_install(&selection, 1000, &d).unwrap();
    apply(&selection, &authority, &plan, 1001, &d).unwrap();
    let remove = plan_uninstall(&selection, 1002, &d).unwrap();
    apply(&selection, &authority, &remove, 1003, &d).unwrap();
    assert!(path.exists());
    assert_eq!(fs::read(path).unwrap(), b"");
}
#[test]
fn foreign_codex_table_modified_owned_entry_and_cross_profile_authority_refuse() {
    let (_dir, selection, root, _source) = fixture();
    let path = root.join("config.toml");
    let foreign = b"[mcp_servers.\"yoke_owned-fixture\"]\ncommand=\"foreign\"\n";
    fs::write(&path, foreign).unwrap();
    let d = Deadline::new(30000);
    assert!(plan_install(&selection, 1000, &d).is_err());
    assert_eq!(fs::read(&path).unwrap(), foreign);
    assert!(!state(&selection).exists());
    fs::write(&path, []).unwrap();
    let plan = plan_install(&selection, 1000, &d).unwrap();
    let mut generic_bindings = selection.bindings.clone();
    generic_bindings.adapter = None;
    let generic = Selection {
        installation: selection.installation.clone(),
        bindings: generic_bindings,
    };
    assert!(apply(&selection, &WriteAuthority::new(&generic), &plan, 1001, &d).is_err());
    assert!(!state(&selection).exists());
    apply(
        &selection,
        &WriteAuthority::new(&selection),
        &plan,
        1001,
        &d,
    )
    .unwrap();
    let original = fs::read(&path).unwrap();
    let altered = std::str::from_utf8(&original)
        .unwrap()
        .replace("enabled = true", "enabled = false");
    fs::write(&path, altered.as_bytes()).unwrap();
    assert!(plan_uninstall(&selection, 1002, &d).is_err());
    assert_eq!(fs::read(&path).unwrap(), altered.as_bytes());
}
#[test]
fn changed_source_identity_and_imported_profile_or_target_cannot_redirect_writes() {
    let (_dir, selection, root, source) = fixture();
    let d = Deadline::new(30000);
    let path = root.join("config.toml");
    let original = b"# retained\n";
    fs::write(&path, original).unwrap();
    let plan = plan_install(&selection, 1000, &d).unwrap();
    for (profile, target_path) in [
        (PROFILE_ID, "config.toml"),
        (adapter::CODEX_PROFILE, "AGENTS.md"),
    ] {
        let mut altered = plan.clone();
        altered.profile = profile.into();
        altered.files[0].path = target_path.into();
        altered.id = plan_id(&altered);
        assert!(
            apply(
                &selection,
                &WriteAuthority::new(&selection),
                &altered,
                1001,
                &d
            )
            .is_err()
        );
    }
    let moved = source.with_file_name("moved source");
    fs::rename(&source, &moved).unwrap();
    fs::create_dir(&source).unwrap();
    assert!(
        apply(
            &selection,
            &WriteAuthority::new(&selection),
            &plan,
            1001,
            &d
        )
        .is_err()
    );
    assert_eq!(fs::read(&path).unwrap(), original);
    assert!(!state(&selection).exists());
    assert!(!source.join(".yoke").exists());
}

#[test]
fn codex_initialization_repair_replays_the_canonical_toml_edit_without_touching_targets() {
    let (_dir, selection, root, source) = fixture();
    let d = Deadline::new(30000);
    let path = root.join("config.toml");
    let original = b"# existing human config\r\n";
    fs::write(&path, original).unwrap();
    let plan = plan_install(&selection, 1000, &d).unwrap();
    let scaffold = state(&selection);
    fs::create_dir(&scaffold).unwrap();
    fs::write(scaffold.join("writer.lock"), []).unwrap();
    for name in ["plans", "receipts", "journals"] {
        fs::create_dir(scaffold.join(name)).unwrap();
    }
    let authority = WriteAuthority::new(&selection);
    let token = initialization_token(&plan);
    let first = recover_initialization(&selection, &authority, &plan, &token, 1001, &d).unwrap();
    let second = recover_initialization(&selection, &authority, &plan, &token, 1002, &d).unwrap();
    assert_eq!(first, second);
    assert_eq!(fs::read(&path).unwrap(), original);
    assert!(!source.join(".yoke").exists());
    apply(&selection, &authority, &plan, 1003, &d).unwrap();
    assert!(fs::read(path).unwrap().starts_with(original));
}
