use serde_json::Value;
use yoke::{
    Engine,
    capture::Deadline,
    export::{self, SourceExplorerOptions},
};

fn embedded(html: &[u8], id: &str) -> Value {
    let html = std::str::from_utf8(html).unwrap();
    let marker = format!("<script type=\"application/json\" id=\"{id}\">");
    serde_json::from_str(
        html.split_once(&marker)
            .unwrap()
            .1
            .split_once("</script>")
            .unwrap()
            .0,
    )
    .unwrap()
}
fn fixture() -> (
    tempfile::TempDir,
    Engine,
    yoke::model::Snapshot,
    yoke::model::Graph,
) {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("π.ts"), "\u{feff}// </script><script>globalThis.INJECTED=1</script> __YOKE_GRAPH_DATA__\r\nexport function café(x: number) {\r\n  const emoji = '😀';\r\n  return x + emoji.length;\r\n}\r\ncafé(1); café(2);\r\n").unwrap();
    let mut engine = Engine::open(root.path()).unwrap();
    let (snapshot, graph, _, _) = engine.index(&Deadline::new(10000)).unwrap();
    (root, engine, snapshot, graph)
}
#[test]
fn source_export_preserves_original_utf8_bom_crlf_and_parallel_graph() {
    let (root, engine, snapshot, graph) = fixture();
    let deadline = Deadline::new(10000);
    let old = export::serialize_explorer(&engine.store, &snapshot, &graph, &deadline).unwrap();
    let html = export::serialize_source_explorer(
        &engine.store,
        &snapshot,
        &graph,
        &Default::default(),
        &deadline,
    )
    .unwrap();
    let payload = embedded(&html, "sources");
    assert_eq!(payload["snapshotId"], snapshot.id);
    assert_eq!(payload["graphHash"], yoke::model::identity(&graph));
    assert_eq!(payload["files"][0]["binding"]["path"], "π.ts");
    assert_eq!(
        payload["files"][0]["text"].as_str().unwrap().as_bytes(),
        std::fs::read(root.path().join("π.ts")).unwrap()
    );
    assert_eq!(
        embedded(&html, "data"),
        serde_json::to_value(&graph).unwrap()
    );
    assert_eq!(
        export::serialize_explorer(&engine.store, &snapshot, &graph, &deadline).unwrap(),
        old
    );
    assert!(
        !std::str::from_utf8(&html)
            .unwrap()
            .contains("<script>globalThis.INJECTED")
    );
    assert!(
        !std::str::from_utf8(&old)
            .unwrap()
            .contains("id=\"sources\"")
    );
    let calls: Vec<_> = graph
        .observations
        .iter()
        .filter(|o| o.relation == "calls")
        .collect();
    assert_eq!(calls.len(), 2);
    assert_ne!(calls[0].id, calls[1].id);
    // Export is an immutable capture, never a later disk read.
    std::fs::write(root.path().join("π.ts"), "foreign later disk bytes").unwrap();
    assert_eq!(
        export::serialize_source_explorer(
            &engine.store,
            &snapshot,
            &graph,
            &Default::default(),
            &deadline
        )
        .unwrap(),
        html
    );
}
#[test]
fn source_budget_refuses_before_blob_open_and_output_budget_is_exact() {
    let (_root, engine, snapshot, graph) = fixture();
    let deadline = Deadline::new(10000);
    let html = export::serialize_source_explorer(
        &engine.store,
        &snapshot,
        &graph,
        &Default::default(),
        &deadline,
    )
    .unwrap();
    let mut options = SourceExplorerOptions {
        max_source_bytes: snapshot.files[0].size,
        max_output_bytes: html.len(),
    };
    assert_eq!(
        export::serialize_source_explorer(&engine.store, &snapshot, &graph, &options, &deadline)
            .unwrap(),
        html
    );
    options.max_output_bytes -= 1;
    assert!(
        export::serialize_source_explorer(&engine.store, &snapshot, &graph, &options, &deadline)
            .unwrap_err()
            .to_string()
            .contains("output byte budget")
    );
    std::fs::remove_file(engine.store.dir.join("blobs").join(&snapshot.files[0].hash)).unwrap();
    options.max_source_bytes -= 1;
    assert!(
        export::serialize_source_explorer(&engine.store, &snapshot, &graph, &options, &deadline)
            .unwrap_err()
            .to_string()
            .contains("source byte budget")
    );
}
#[test]
fn tampered_graph_path_range_snapshot_and_blob_refuse_without_output() {
    let (root, engine, snapshot, graph) = fixture();
    let deadline = Deadline::new(10000);
    let output = root.path().join("must-not-exist.html");
    for mode in 0..3 {
        let mut changed = graph.clone();
        match mode {
            0 => changed.entities[0].path = "../outside.ts".into(),
            1 => changed.entities[0].start_byte = usize::MAX,
            _ => changed.entities[0].signature = "foreign".into(),
        }
        assert!(
            export::write_source_explorer(
                &engine.store,
                &snapshot,
                &changed,
                &output,
                &Default::default(),
                &deadline
            )
            .is_err()
        );
        assert!(!output.exists());
    }
    let mut changed = snapshot.clone();
    changed.files[0].path = "C:/foreign.ts".into();
    assert!(
        export::write_source_explorer(
            &engine.store,
            &changed,
            &graph,
            &output,
            &Default::default(),
            &deadline
        )
        .is_err()
    );
    let blob = engine.store.dir.join("blobs").join(&snapshot.files[0].hash);
    let mut bytes = std::fs::read(&blob).unwrap();
    bytes[3] ^= 1;
    std::fs::write(blob, bytes).unwrap();
    assert!(
        export::write_source_explorer(
            &engine.store,
            &snapshot,
            &graph,
            &output,
            &Default::default(),
            &deadline
        )
        .is_err()
    );
    assert!(!output.exists());
}
#[test]
fn source_export_is_create_only_and_options_are_closed() {
    let (root, engine, snapshot, graph) = fixture();
    let deadline = Deadline::new(10000);
    let output = root.path().join("source.html");
    export::write_source_explorer(
        &engine.store,
        &snapshot,
        &graph,
        &output,
        &Default::default(),
        &deadline,
    )
    .unwrap();
    let before = std::fs::read(&output).unwrap();
    export::write_source_explorer(
        &engine.store,
        &snapshot,
        &graph,
        &output,
        &Default::default(),
        &deadline,
    )
    .unwrap();
    assert_eq!(std::fs::read(&output).unwrap(), before);
    std::fs::write(&output, b"foreign output").unwrap();
    assert!(
        export::write_source_explorer(
            &engine.store,
            &snapshot,
            &graph,
            &output,
            &Default::default(),
            &deadline
        )
        .is_err()
    );
    assert_eq!(std::fs::read(&output).unwrap(), b"foreign output");
    assert!(
        serde_json::from_str::<SourceExplorerOptions>(
            r#"{"maxSourceBytes":1,"maxOutputBytes":1024,"allowNetwork":true}"#
        )
        .is_err()
    );
    assert!(
        serde_json::from_str::<SourceExplorerOptions>(
            r#"{"maxSourceBytes":1,"maxSourceBytes":2,"maxOutputBytes":1024}"#
        )
        .is_err()
    );
}
#[test]
fn actual_cli_source_mode_is_explicit_and_keeps_canonical_json() {
    let (root, _engine, snapshot, graph) = fixture();
    let output = root.path().join("source.html");
    let result = std::process::Command::new(env!("CARGO_BIN_EXE_yoke"))
        .args([
            "--root",
            root.path().to_str().unwrap(),
            "--snapshot",
            &snapshot.id,
            "export",
            "--format",
            "html-source",
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
    let html = std::fs::read(output).unwrap();
    assert_eq!(
        embedded(&html, "data"),
        serde_json::to_value(graph).unwrap()
    );
    assert_eq!(embedded(&html, "sources")["snapshotId"], snapshot.id);
}
