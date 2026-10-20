//! Native session/CLI semantic planning; owned protocol fixtures are not compilers.
use serde_json::{Value, json};
use std::{fs, path::Path};
use yoke::{
    Engine,
    analysis::AnalyzerEpoch,
    capture::Deadline,
    lsp,
    project_session::{HostProfile, Session},
};

const SOURCE: &str = "\u{feff}// owned café 🦀\r\nfunction dependency() { return 1; }\r\nexport function removeMe() { return dependency(); }\r\nexport function keep() { void (removeMe()); return 2; }\r\n";
const AFTER: &str = "\u{feff}// owned café 🦀\r\n\r\n\r\nexport function keep() {  return 2; }\r\n";

fn d() -> Deadline {
    Deadline::new(30_000)
}
struct Fixture {
    session: Session,
    root: tempfile::TempDir,
    inputs: tempfile::TempDir,
}
fn fixture(epoch: AnalyzerEpoch, host: HostProfile) -> Fixture {
    let root = tempfile::tempdir().unwrap();
    let inputs = tempfile::tempdir().unwrap();
    fs::write(root.path().join("module.ts"), SOURCE).unwrap();
    let mut engine = Engine::open(root.path()).unwrap();
    engine.capture_epoch = epoch;
    engine
        .register_configured_server(lsp::ConfiguredServer {
            server: lsp::Server {
                command: "node".into(),
                args: vec![
                    Path::new(env!("CARGO_MANIFEST_DIR"))
                        .join("tests/fixtures/propagated-delete-lsp.mjs")
                        .display()
                        .to_string(),
                    inputs.path().join("protocol.log").display().to_string(),
                    "ok".into(),
                    root.path().join("module.ts").display().to_string(),
                ],
                language: "typescript".into(),
                trust: true,
            },
            configuration: lsp::HostConfiguration {
                passive_profile: Some(lsp::PassiveProfile::ContractFixture),
                ..Default::default()
            },
        })
        .unwrap();
    Fixture {
        session: Session::open(engine, host, Some("native-refactor"), &d()).unwrap(),
        root,
        inputs,
    }
}
fn request() -> Value {
    json!({"operation":"safeDelete","arguments":{"relativePath":"module.ts","namePath":"removeMe","propagate":true,"propagationPolicy":"captured_es_function_propagation_v2"}})
}
fn transcript(f: &Fixture) -> String {
    fs::read_to_string(f.inputs.path().join("protocol.log")).unwrap_or_default()
}
fn unpublished(f: &Fixture) {
    assert_eq!(f.session.engine.store.db.query_row("SELECT COUNT(*) FROM records WHERE kind IN ('plan','approval','receipt','journal')",[],|row|row.get::<_,usize>(0)).unwrap(),0);
    assert_eq!(
        fs::read(f.root.path().join("module.ts")).unwrap(),
        SOURCE.as_bytes()
    );
}

#[test]
fn session_refactor_prepares_the_native_plan_without_steering_or_source_writes() {
    for epoch in [AnalyzerEpoch::V8, AnalyzerEpoch::V9] {
        let mut f = fixture(epoch, HostProfile::default());
        let before = f.session.invoke("configuration", &json!({}), &d()).unwrap();
        let value = f
            .session
            .invoke("refactor.prepare", &request(), &d())
            .unwrap();
        assert_eq!(
            value["propagation"]["policy"],
            "captured_es_function_propagation_v2"
        );
        assert_eq!(value["complete"], false);
        assert_eq!(value["commandsExecuted"], false);
        assert_eq!(
            f.session.invoke("configuration", &json!({}), &d()).unwrap(),
            before
        );
        let result = f
            .session
            .engine
            .store
            .snapshot(value["plan"]["resultSnapshotId"].as_str().unwrap())
            .unwrap();
        assert_eq!(
            yoke::query::source(&f.session.engine.store, &result, "module.ts")
                .unwrap()
                .0,
            AFTER
        );
        assert_eq!(
            fs::read(f.root.path().join("module.ts")).unwrap(),
            SOURCE.as_bytes()
        );
        assert!(transcript(&f).contains("textDocument/references"));
        assert_eq!(
            f.session
                .engine
                .store
                .db
                .query_row(
                    "SELECT COUNT(*) FROM records WHERE kind IN ('approval','receipt','journal')",
                    [],
                    |row| row.get::<_, usize>(0)
                )
                .unwrap(),
            0
        );
    }
}
