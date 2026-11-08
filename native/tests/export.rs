use serde_json::Value;
use std::{fs, process::Command};
use tempfile::TempDir;
#[test]
fn canonical_export_preserves_directed_parallel_evidence() {
    let dir = TempDir::new().unwrap();
    fs::write(
        dir.path().join("main.rs"),
        "fn helper(){} fn main(){helper();helper();}\n",
    )
    .unwrap();
    let output = dir.path().join("graph.json");
    let result = Command::new(env!("CARGO_BIN_EXE_yoke"))
        .args([
            "--root",
            dir.path().to_str().unwrap(),
            "export",
            "--output",
            output.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let graph: Value = serde_json::from_slice(&fs::read(output).unwrap()).unwrap();
    let edges: Vec<_> = graph["observations"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|o| o["relation"] == "calls")
        .collect();
    assert_eq!(edges.len(), 2);
    assert_eq!(edges[0]["source"], edges[1]["source"]);
    assert_eq!(edges[0]["target"], edges[1]["target"]);
    assert_ne!(edges[0]["evidenceIds"], edges[1]["evidenceIds"]);
}
