use std::io::{Cursor, Write};
use yoke::{Engine, capture::Deadline, document_graph, model};

fn package() -> Vec<u8> {
    let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
    for (name, text) in [
        (
            "_rels/.rels",
            r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="main" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>"#,
        ),
        (
            "[Content_Types].xml",
            r#"<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/></Types>"#,
        ),
        (
            "word/document.xml",
            "<w:document xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\"><w:body><w:p><w:r><w:t>__YOKE_GRAPH_DATA__ &lt;/script&gt; &amp; 世界\u{2028}\u{2029}</w:t></w:r></w:p></w:body></w:document>",
        ),
    ] {
        zip.start_file(
            name,
            zip::write::SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Deflated),
        )
        .unwrap();
        zip.write_all(text.as_bytes()).unwrap();
    }
    zip.finish().unwrap().into_inner()
}
#[test]
fn checked_document_explorer_preserves_spaces_inert_text_and_original_canonical_graph() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("main.rs"), "fn untouched(){}\n").unwrap();
    std::fs::write(root.path().join("notes.docx"), package()).unwrap();
    let mut engine = Engine::open(root.path()).unwrap();
    let deadline = Deadline::new(60_000);
    let (snapshot, graph, _, _) = engine.index(&deadline).unwrap();
    let graph_hash = model::identity(&graph);
    let built = document_graph::build(&engine, "notes.docx", &deadline).unwrap();
    let checked = document_graph::load(
        &engine.store,
        built["documentGraphId"].as_str().unwrap(),
        &deadline,
    )
    .unwrap();
    let before = document_graph::explorer_payload(&checked, &deadline).unwrap();
    let html = String::from_utf8(document_graph::explorer(&checked, &deadline).unwrap()).unwrap();
    let embedded = html
        .split_once("<script type=\"application/json\" id=\"data\">")
        .unwrap()
        .1
        .split_once("</script>")
        .unwrap()
        .0;
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(embedded).unwrap(),
        before
    );
    let headings = html.split("<script").next().unwrap();
    assert!(headings.contains("Document graph"));
    assert!(headings.contains("Document JSON"));
    assert!(!headings.contains("Canonical JSON"));
    assert!(!headings.contains("Snapshot "));
    assert!(html.contains("connect-src 'none'"));
    assert!(!embedded.contains("</script>"));
    assert!(html.contains("partXml"));
    assert!(html.contains("decodedText"));
    assert!(html.contains("opaqueContainer"));
    assert_eq!(
        document_graph::explorer_payload(&checked, &deadline).unwrap(),
        before
    );
    assert_eq!(engine.store.latest().unwrap(), Some(snapshot.id.clone()));
    assert_eq!(
        model::identity(&engine.store.graph(&snapshot.id).unwrap()),
        graph_hash
    );
    let javascript = html
        .rsplit_once("<script>")
        .unwrap()
        .1
        .split_once("</script>")
        .unwrap()
        .0;
    let script_path = root.path().join("explorer-check.js");
    std::fs::write(&script_path, javascript).unwrap();
    let syntax = std::process::Command::new("node")
        .arg("--check")
        .arg(&script_path)
        .output()
        .unwrap();
    assert!(
        syntax.status.success(),
        "{}",
        String::from_utf8_lossy(&syntax.stderr)
    );
    let projection = javascript
        .split_once("function documentProjection(")
        .unwrap()
        .1
        .split_once("\nconst data=")
        .unwrap()
        .0;
    let text = javascript
        .split_once("function documentSegmentText(")
        .unwrap()
        .1
        .split_once("\nconst communityData=")
        .unwrap()
        .0;
    let script = format!(
        "const assert=require('node:assert/strict'),crypto=require('node:crypto');const payload={};function documentProjection({projection}\nconst decodedBytes=new TextEncoder().encode(payload.decodedText);function documentSegmentText({text}\nconst projected=documentProjection(payload);assert.deepEqual(projected.entities.map(node=>node.id),payload.documentGraph.nodes.map(node=>node.id));assert.strictEqual(projected.observations,payload.documentGraph.observations);assert.equal(projected.snapshotId,undefined);for(const segment of payload.documentGraph.segments){{const text=documentSegmentText(segment);assert.equal(new TextEncoder().encode(text).length,segment.endByte-segment.startByte);assert.equal(crypto.createHash('sha256').update(text).digest('hex'),segment.textHash);}}",
        serde_json::to_string(&before).unwrap()
    );
    let runtime = std::process::Command::new("node")
        .args(["-e", &script])
        .output()
        .unwrap();
    assert!(
        runtime.status.success(),
        "{}",
        String::from_utf8_lossy(&runtime.stderr)
    );
}
