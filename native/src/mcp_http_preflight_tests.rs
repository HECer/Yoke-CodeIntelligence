use super::*;

fn automatic_target(temp: &std::path::Path) -> (Target, PathBuf) {
    let root = temp.join("owned");
    std::fs::create_dir(&root).unwrap();
    std::fs::write(root.join("main.rs"), "fn owned() {}\n").unwrap();
    let marker = temp.join("provider-started");
    let worker = temp.join("provider.cjs");
    std::fs::write(
        &worker,
        "require('node:fs').writeFileSync(process.argv[2], 'started'); process.exit(99);",
    )
    .unwrap();
    let node = std::process::Command::new("node")
        .args(["-p", "process.execPath"])
        .output()
        .unwrap();
    assert!(node.status.success());
    let command = String::from_utf8(node.stdout).unwrap().trim().to_owned();
    let host = serde_json::from_value(json!({
        "languageServerAutoSelection":{"priorities":{"rust":3}}
    }))
    .unwrap();
    let mut target = Target::new(&root, Profile::Session, host);
    target.host_servers.push(
        serde_json::from_value(json!({
            "backendId":"rust", "command":command, "args":[worker, marker],
            "configuration":{"passiveProfile":"contract_fixture"}
        }))
        .unwrap(),
    );
    (target, marker)
}

fn counts(root: &std::path::Path) -> (u64, u64) {
    let engine = Engine::open(root).unwrap();
    let sessions = engine
        .store
        .db
        .query_row(
            "SELECT COUNT(*) FROM records WHERE kind=?1",
            [crate::project_session::RECORD_KIND],
            |row| row.get(0),
        )
        .unwrap();
    let snapshots = engine
        .store
        .db
        .query_row("SELECT COUNT(*) FROM snapshots", [], |row| row.get(0))
        .unwrap();
    (sessions, snapshots)
}

#[test]
fn principal_preflight_never_captures_publishes_or_starts_the_automatic_provider() {
    let temp = tempfile::tempdir().unwrap();
    let (target, marker) = automatic_target(temp.path());
    let root = target.root.clone();
    assert_eq!(counts(&root), (0, 0));
    Principal::new("owned-auto", Zeroizing::new(vec![51; 32]), target.clone()).unwrap();
    assert_eq!(counts(&root), (0, 0));
    assert!(!marker.exists());

    // The ordinary inspector still refuses an uninitialized automatic choice.
    let deadline = Deadline::new(30_000);
    let error = Session::inspect(
        target.engine(&deadline).unwrap(),
        target.host,
        Some("inspect"),
        &deadline,
    )
    .err()
    .unwrap();
    assert!(
        error
            .to_string()
            .contains("language_auto_inspection_requires_initialized_selection")
    );
    assert_eq!(counts(&root), (0, 0));
    assert!(!marker.exists());
}

#[test]
fn host_preflight_keeps_deadline_policy_and_write_authority_refusals() {
    let temp = tempfile::tempdir().unwrap();
    let (target, marker) = automatic_target(temp.path());
    assert!(target.preflight(&Deadline::new(0)).is_err());
    let mut invalid = target.clone();
    invalid.host = serde_json::from_value(json!({
        "languageServerAutoSelection":{"priorities":{"foreign-backend":2}}
    }))
    .unwrap();
    assert!(Principal::new("invalid-auto", Zeroizing::new(vec![51; 32]), invalid).is_err());
    let mut writes = target.clone();
    // Host defaults include editing; select a non-editing profile for this refusal.
    writes.host.modes = vec!["interactive".into()];
    writes.allow_writes = true;
    let error = Principal::new("invalid-writes", Zeroizing::new(vec![51; 32]), writes)
        .err()
        .unwrap();
    assert!(
        error
            .to_string()
            .contains("Host write authority requires editing mode")
    );
    let mut allowed = target.clone();
    allowed.host.modes = vec!["interactive".into(), "editing".into()];
    allowed.allow_writes = true;
    Principal::new("editing-writes", Zeroizing::new(vec![51; 32]), allowed).unwrap();
    assert_eq!(counts(&target.root), (0, 0));
    assert!(!marker.exists());
}

#[test]
fn preflight_cannot_rebind_retained_state_and_active_creation_still_checks_registration() {
    let temp = tempfile::tempdir().unwrap();
    let (target, marker) = automatic_target(temp.path());
    let deadline = Deadline::new(30_000);
    let mut session = Session::open(
        target.engine(&deadline).unwrap(),
        target.host.clone(),
        Some("retained"),
        &deadline,
    )
    .unwrap();
    let configuration = session
        .invoke("configuration", &json!({}), &deadline)
        .unwrap();
    assert_eq!(
        configuration["automaticLanguageServers"]["default"]["backendId"],
        "rust"
    );
    drop(session);
    let before = counts(&target.root);
    Session::preflight(
        target.engine(&deadline).unwrap(),
        target.host.clone(),
        Some("retained"),
        None,
        &deadline,
    )
    .unwrap();
    let mut foreign = target.host.clone();
    foreign.language_server_auto_selection =
        Some(serde_json::from_value(json!({"priorities":{"rust":1}})).unwrap());
    assert!(
        Session::preflight(
            target.engine(&deadline).unwrap(),
            foreign,
            Some("retained"),
            None,
            &deadline
        )
        .is_err()
    );
    assert_eq!(counts(&target.root), before);
    let mut resumed = Session::inspect(
        target.engine(&deadline).unwrap(),
        target.host.clone(),
        Some("retained"),
        &deadline,
    )
    .unwrap();
    assert_eq!(
        resumed
            .invoke("configuration", &json!({}), &deadline)
            .unwrap(),
        configuration
    );
    drop(resumed);

    let mut missing = target.clone();
    missing.host_servers.clear();
    let error = missing.open(&deadline).err().unwrap();
    assert!(
        error
            .to_string()
            .contains("language_server_not_host_registered")
    );
    assert!(!marker.exists());
    assert_eq!(
        std::fs::read(target.root.join("main.rs")).unwrap(),
        b"fn owned() {}\n"
    );
}
