//! Public CLI review gates; no live source directory or federation grant is needed for native archives.
use std::{fs, process::Command};
use yoke::{Engine, capture::Deadline, graph_collections::Registry};

#[test]
fn native_archived_source_reads_use_native_proof_without_foreign_authority() {
    let source = tempfile::tempdir().unwrap();
    let registry_root = tempfile::tempdir().unwrap();
    let absent = tempfile::tempdir().unwrap();
    let text = "pub fn café() -> u32 { 17 }\n";
    fs::write(source.path().join("main.rs"), text).unwrap();
    {
        let mut engine = Engine::open(source.path()).unwrap();
        let deadline = Deadline::new(5000);
        let (snapshot, graph, _, _) = engine.index(&deadline).unwrap();
        Registry::open(registry_root.path())
            .unwrap()
            .register_cas("native", &engine.store, &snapshot, &graph, None, &deadline)
            .unwrap();
    }
    source.close().unwrap();
    let never_open = absent.path().join("never-open-source");
    let mut command = Command::new(env!("CARGO_BIN_EXE_yoke"));
    command.arg("--root").arg(&never_open).arg("collections");
    // This positive fixture explicitly grants SQLite sidecar coordination;
    // the default refusal is covered by the separate registry contract tests.
    #[cfg(unix)]
    command.arg("--allow-registry-coordination");
    let output = command
        .arg("--registry")
        .arg(registry_root.path())
        .args([
            "read-source",
            "native",
            "main.rs",
            "--start-byte",
            "0",
            "--end-byte",
        ])
        .arg(text.len().to_string())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let read: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(read["text"], text);
    assert!(!never_open.exists());
}
