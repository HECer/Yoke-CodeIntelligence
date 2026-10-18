use serde_json::{Value, json};
use std::{
    io::{Cursor, Write},
    path::Path,
    process::Command,
};

fn docx() -> Vec<u8> {
    let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
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
            r#"<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body><w:p><w:r><w:t>Public &amp; immutable 世界</w:t></w:r></w:p></w:body></w:document>"#,
        ),
    ] {
        writer
            .start_file(name, zip::write::SimpleFileOptions::default())
            .unwrap();
        writer.write_all(text.as_bytes()).unwrap();
    }
    writer.finish().unwrap().into_inner()
}
fn cli(root: &Path, arguments: &[&str]) -> Value {
    let output = Command::new(env!("CARGO_BIN_EXE_yoke"))
        .arg("--root")
        .arg(root)
        .arg("document")
        .args(arguments)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}
#[test]
fn public_document_cli_builds_searches_reads_and_queries_archive_without_source_store() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("notes.docx"), docx()).unwrap();
    let built = cli(root.path(), &["build", "notes.docx"]);
    let id = built["documentGraphId"].as_str().unwrap();
    let result = cli(
        root.path(),
        &["query", id, "find", "--args", r#"{"query":"世界"}"#],
    );
    assert_eq!(result["items"][0]["text"], "世界");
    let segment = result["items"][0]["segmentId"].as_str().unwrap();
    let args = json!({"segmentId":segment}).to_string();
    let read = cli(root.path(), &["query", id, "read", "--args", &args]);
    assert_eq!(read["text"], "Public & immutable 世界");
    assert_eq!(read["sourceSpace"], "decodedText");
    let archive = tempfile::tempdir().unwrap();
    let html = root.path().join("document.html");
    cli(
        root.path(),
        &["explorer", id, "--output", html.to_str().unwrap()],
    );
    let html_bytes = std::fs::read(&html).unwrap();
    assert!(String::from_utf8_lossy(&html_bytes).contains("Document graph"));
    let refused = Command::new(env!("CARGO_BIN_EXE_yoke"))
        .arg("--root")
        .arg(root.path())
        .args(["document", "explorer", id, "--output"])
        .arg(&html)
        .output()
        .unwrap();
    assert!(
        !refused.status.success(),
        "Explorer export overwrote an existing file"
    );
    assert_eq!(std::fs::read(&html).unwrap(), html_bytes);
    cli(
        root.path(),
        &["archive", id, "--output", archive.path().to_str().unwrap()],
    );
    let unrelated = tempfile::tempdir().unwrap();
    std::fs::remove_file(root.path().join("notes.docx")).unwrap();
    let result = cli(
        unrelated.path(),
        &[
            "query-archive",
            archive.path().to_str().unwrap(),
            "find",
            "--args",
            r#"{"query":"世界"}"#,
        ],
    );
    assert_eq!(result["items"][0]["text"], "世界");
    let archived_html = unrelated.path().join("archived.html");
    cli(
        unrelated.path(),
        &[
            "explorer-archive",
            archive.path().to_str().unwrap(),
            "--output",
            archived_html.to_str().unwrap(),
        ],
    );
    assert_eq!(std::fs::read(archived_html).unwrap(), html_bytes);
    assert!(
        !unrelated.path().join(".yoke").exists(),
        "Standalone archive queries must not open a source store"
    );
}
