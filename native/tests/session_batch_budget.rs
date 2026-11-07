//! Actual Session CLI output admission; owned inert sources and no providers.
use serde_json::{Value, json};
use std::{fs, path::PathBuf, process::Command};

struct Fixture {
    _base: tempfile::TempDir,
    root: PathBuf,
    profile: PathBuf,
    requests: PathBuf,
}
impl Fixture {
    fn new() -> Self {
        let base = tempfile::tempdir().unwrap();
        let root = base.path().join("workspace");
        fs::create_dir(&root).unwrap();
        fs::write(root.join("main.rs"), "// owned 🦀\r\nfn inert() {}\r\n").unwrap();
        fs::write(root.join(".env"), b"PRIVATE_CANARY=not-output\n").unwrap();
        let profile = base.path().join("host.json");
        fs::write(&profile, br#"{"notesPolicy":{"writable":true}}"#).unwrap();
        let requests = base.path().join("requests.json");
        Self {
            _base: base,
            root,
            profile,
            requests,
        }
    }
    fn run(&self, rows: Value, limit: u64) -> std::process::Output {
        fs::write(&self.requests, serde_json::to_vec(&rows).unwrap()).unwrap();
        Command::new(env!("CARGO_BIN_EXE_yoke"))
            .arg("--root")
            .arg(&self.root)
            .args(["--timeout", "20000", "--max-bytes"])
            .arg(limit.to_string())
            .args(["session", "--id", "budget", "--host-profile"])
            .arg(&self.profile)
            .arg("--requests")
            .arg(&self.requests)
            .output()
            .unwrap()
    }
    fn records(&self, kind: &str) -> u64 {
        let db = rusqlite::Connection::open(self.root.join(".yoke/index.sqlite")).unwrap();
        db.query_row("SELECT count(*) FROM records WHERE kind=?1", [kind], |r| {
            r.get(0)
        })
        .unwrap()
    }
    fn unchanged(&self) {
        assert_eq!(
            fs::read(self.root.join("main.rs")).unwrap(),
            "// owned 🦀\r\nfn inert() {}\r\n".as_bytes()
        );
        assert_eq!(
            fs::read(self.root.join(".env")).unwrap(),
            b"PRIVATE_CANARY=not-output\n"
        );
        assert_eq!(
            self.records("artifact"),
            0,
            "Private results must not become durable transport artifacts"
        );
    }
}

#[test]
fn successful_small_batch_keeps_its_original_shape_and_complete_bounded_stdout() {
    let fixture = Fixture::new();
    let output = fixture.run(json!([{"operation":"checkOnboarding","args":{}}]), 1024);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stdout.len() <= 1024 && output.stdout.ends_with(b"\n"));
    let value: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["atomicBatch"], false);
    assert_eq!(value["sourceWritten"], false);
    assert_eq!(value["results"].as_array().unwrap().len(), 1);
    assert_eq!(value["results"][0]["onboarded"], false);
    fixture.unchanged();
}

#[test]
fn explicit_global_cap_refuses_the_batch_without_stdout_or_artifact_fallback() {
    let fixture = Fixture::new();
    let rows = (0..100)
        .map(|_| json!({"operation":"configuration","args":{}}))
        .collect::<Vec<_>>();
    let output = fixture.run(json!(rows), 1024);
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    let error = String::from_utf8_lossy(&output.stderr);
    assert!(
        error.contains("Session reply byte budget exceeded"),
        "{error}"
    );
    assert!(error.contains("batch is not atomic"));
    assert!(!error.contains("PRIVATE_CANARY"));
    fixture.unchanged();
}

#[test]
fn exactly_full_successful_prefix_does_not_execute_a_following_mutation() {
    let fixture = Fixture::new();
    let mut rows = (0..25)
        .map(|_| json!({"operation":"checkOnboarding","args":{}}))
        .collect::<Vec<_>>();
    let prefix = fixture.run(json!(rows), 4_000_000);
    assert!(
        prefix.status.success(),
        "{}",
        String::from_utf8_lossy(&prefix.stderr)
    );
    let exact = prefix.stdout.len() as u64;
    assert!((1024..=4_000_000).contains(&exact));
    rows.push(
        json!({"operation":"notes.write","args":{"name":"later","content":"must not execute"}}),
    );
    let output = fixture.run(json!(rows), exact);
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    let error = String::from_utf8_lossy(&output.stderr);
    assert!(error.contains("before operation 26"), "{error}");
    assert!(error.contains("Session reply byte budget exceeded"));
    assert_eq!(fixture.records(yoke::notes::HEAD_KIND), 0);
    fixture.unchanged();
}

#[test]
fn earlier_mutation_is_acknowledged_and_later_requests_do_not_run_after_refusal() {
    let fixture = Fixture::new();
    let output = fixture.run(
        json!([
            {"operation":"notes.write","args":{"name":"first","content":"π🦀\r\n".repeat(1500)}},
            {"operation":"notes.read","args":{"name":"first"}},
            {"operation":"notes.write","args":{"name":"later","content":"must not execute"}}
        ]),
        1024,
    );
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    let error = String::from_utf8_lossy(&output.stderr);
    assert!(
        error.contains("Session reply byte budget exceeded"),
        "{error}"
    );
    assert!(error.contains("prior effects may have occurred"));
    assert!(!error.contains("π🦀"));
    assert_eq!(fixture.records(yoke::notes::HEAD_KIND), 1);
    fixture.unchanged();
}
