use serde_json::{Value, json};
use std::{fs, path::PathBuf};
use yoke::{
    Engine,
    capture::Deadline,
    project_session::{HostProfile, Session},
};

fn session(root: &std::path::Path, log: &std::path::Path, mode: &str) -> Session {
    let mut engine = Engine::open(root).unwrap();
    engine
        .register_configured_server(yoke::lsp::ConfiguredServer {
            server: yoke::lsp::Server {
                command: "node".into(),
                args: vec![
                    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                        .join("tests/fixtures/lifecycle-lsp.mjs")
                        .display()
                        .to_string(),
                    log.display().to_string(),
                    mode.into(),
                ],
                language: "rust".into(),
                trust: true,
            },
            configuration: yoke::lsp::HostConfiguration {
                passive_profile: Some(yoke::lsp::PassiveProfile::ContractFixture),
                ..Default::default()
            },
        })
        .unwrap();
    Session::open(
        engine,
        HostProfile::default(),
        Some("lifecycle"),
        &Deadline::new(20000),
    )
    .unwrap()
}
fn restart(session: &mut Session) -> anyhow::Result<Value> {
    yoke::agent_repl::invoke_facade(
        session,
        "lsp.restart_language_server",
        json!({}),
        None,
        &Deadline::new(20000),
    )
}
#[test]
fn restart_performs_new_real_provider_handshakes_and_advances_only_owned_status() {
    let root = tempfile::tempdir().unwrap();
    let log = tempfile::NamedTempFile::new().unwrap();
    let source = "fn actual() {}\n";
    fs::write(root.path().join("main.rs"), source).unwrap();
    let mut session = session(root.path(), log.path(), "healthy");
    let first = restart(&mut session).unwrap();
    let second = restart(&mut session).unwrap();
    assert_eq!(first["generation"], 1);
    assert_eq!(second["generation"], 2);
    assert_eq!(first["snapshotId"], second["snapshotId"]);
    assert_eq!(
        first["providers"][0]["contentHash"],
        yoke::model::hash(source)
    );
    assert_eq!(
        second["providers"][0]["serverProfileHash"],
        first["providers"][0]["serverProfileHash"]
    );
    assert_eq!(second["lifecycle"], "per_request_reinitialization");
    assert_eq!(second["persistentProcessesRestarted"], 0);
    let events: Vec<Value> = fs::read_to_string(log.path())
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(
        events
            .iter()
            .filter(|event| event["event"] == "initialize")
            .count(),
        2
    );
    assert_eq!(
        events
            .iter()
            .filter(|event| event["event"] == "shutdown")
            .count(),
        2
    );
    assert_ne!(events[0]["pid"], events[2]["pid"]);
    assert_eq!(
        fs::read_to_string(root.path().join("main.rs")).unwrap(),
        source
    );
    let plans: i64 = session
        .engine
        .store
        .db
        .query_row(
            "SELECT count(*) FROM records WHERE kind='plan'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(plans, 0);
}
#[test]
fn failed_health_does_not_publish_status_generation_or_claim_restart() {
    let root = tempfile::tempdir().unwrap();
    let log = tempfile::NamedTempFile::new().unwrap();
    fs::write(root.path().join("main.rs"), "fn actual() {}\n").unwrap();
    let mut session = session(root.path(), log.path(), "bad-capabilities");
    assert!(restart(&mut session).is_err());
    let exists: bool = session
        .engine
        .store
        .db
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE name='native_lsp_lifecycle')",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert!(!exists, "failed negotiation published lifecycle status");
    let events = fs::read_to_string(log.path()).unwrap();
    assert!(events.contains("initialize"));
}
