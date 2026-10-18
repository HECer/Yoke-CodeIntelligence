use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    fs,
    io::{Cursor, Write},
    path::Path,
};
use yoke::{
    Engine,
    capture::Deadline,
    document_graph, graph_view, media,
    mixed_view::{
        DocumentPin, NativePdfWorker, PDF_EXTRACTION_POLICY, PdfPin, SelectionV2, SourcePin,
    },
    model::{hash, identity},
};

pub const SOURCE: &str = "// storage Grüße 🌍\r\npub fn billing() {}\r\n";
pub const PAGES: [&str; 2] = ["Page one storage Grüße\n", "Page two STORAGE café\n"];
pub fn pdf() -> Vec<u8> {
    let first = b"BT /F1 12 Tf 10 100 Td (Page one storage Gr\xfc\xdfe) Tj ET";
    let second = b"BT /F1 12 Tf 10 100 Td (Page two STORAGE caf\xe9) Tj ET";
    let stream = |text: &[u8]| {
        [
            format!("<< /Length {} >>\nstream\n", text.len()).as_bytes(),
            text,
            b"\nendstream",
        ]
        .concat()
    };
    let objects = [
        b"<< /Type /Catalog /Pages 2 0 R >>".to_vec(),
        b"<< /Type /Pages /Kids [3 0 R 4 0 R] /Count 2 >>".to_vec(),
        b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] /Resources << /Font << /F1 5 0 R >> >> /Contents 6 0 R >>".to_vec(),
        b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] /Resources << /Font << /F1 5 0 R >> >> /Contents 7 0 R >>".to_vec(),
        b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>".to_vec(),
        stream(first),stream(second),
    ];
    let mut raw = b"%PDF-1.4\n".to_vec();
    let mut offsets = vec![0usize];
    for (i, object) in objects.iter().enumerate() {
        offsets.push(raw.len());
        raw.extend(format!("{} 0 obj\n", i + 1).as_bytes());
        raw.extend(object);
        raw.extend(b"\nendobj\n");
    }
    let xref = raw.len();
    raw.extend(format!("xref\n0 {}\n0000000000 65535 f \n", offsets.len()).as_bytes());
    for offset in offsets.iter().skip(1) {
        raw.extend(format!("{offset:010} 00000 n \n").as_bytes());
    }
    raw.extend(
        format!(
            "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n",
            offsets.len()
        )
        .as_bytes(),
    );
    raw
}
pub fn unicode_pdf() -> Vec<u8> {
    let content = b"BT /F1 12 Tf 10 100 Td <000100020003> Tj ET";
    let cmap=b"/CIDInit /ProcSet findresource begin\n12 dict begin\nbegincmap\n/CIDSystemInfo << /Registry (Adobe) /Ordering (UCS) /Supplement 0 >> def\n/CMapName /Fixture def\n/CMapType 2 def\n1 begincodespacerange\n<0000> <ffff>\nendcodespacerange\n3 beginbfchar\n<0001> <0041>\n<0002> <d83cdf0d>\n<0003> <00fc>\nendbfchar\nendcmap\nCMapName currentdict /CMap defineresource pop\nend\nend\n";
    let stream = |text: &[u8]| {
        [
            format!("<< /Length {} >>\nstream\n", text.len()).as_bytes(),
            text,
            b"\nendstream",
        ]
        .concat()
    };
    let objects=[
        b"<< /Type /Catalog /Pages 2 0 R >>".to_vec(),
        b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_vec(),
        b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] /Resources << /Font << /F1 4 0 R >> >> /Contents 5 0 R >>".to_vec(),
        b"<< /Type /Font /Subtype /Type0 /BaseFont /Fixture /Encoding /Identity-H /ToUnicode 6 0 R /DescendantFonts [7 0 R] >>".to_vec(),
        stream(content),stream(cmap),
        b"<< /Type /Font /Subtype /CIDFontType2 /BaseFont /Fixture /CIDSystemInfo << /Registry (Adobe) /Ordering (Identity) /Supplement 0 >> >>".to_vec(),
    ];
    let mut raw = b"%PDF-1.4\n".to_vec();
    let mut offsets = vec![0usize];
    for (index, object) in objects.iter().enumerate() {
        offsets.push(raw.len());
        raw.extend(format!("{} 0 obj\n", index + 1).as_bytes());
        raw.extend(object);
        raw.extend(b"\nendobj\n");
    }
    let xref = raw.len();
    raw.extend(format!("xref\n0 {}\n0000000000 65535 f \n", offsets.len()).as_bytes());
    for offset in offsets.iter().skip(1) {
        raw.extend(format!("{offset:010} 00000 n \n").as_bytes());
    }
    raw.extend(
        format!(
            "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n",
            offsets.len()
        )
        .as_bytes(),
    );
    raw
}
fn docx() -> Vec<u8> {
    let parts = [
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
            r#"<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body><w:p><w:r><w:t>Office storage</w:t></w:r></w:p></w:body></w:document>"#,
        ),
    ];
    let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
    for (name, text) in parts {
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
pub struct Fixture {
    pub dir: tempfile::TempDir,
    pub engine: Engine,
    pub selection: SelectionV2,
    pub inspection_artifact: String,
}
pub fn worker() -> NativePdfWorker {
    NativePdfWorker::new(Path::new(env!("CARGO_BIN_EXE_yoke")), &Deadline::new(60000)).unwrap()
}
pub fn fixture() -> Fixture {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("billing.rs"), SOURCE).unwrap();
    fs::write(dir.path().join("notes.pdf"), pdf()).unwrap();
    fs::write(dir.path().join("office.docx"), docx()).unwrap();
    let mut engine = Engine::open(dir.path()).unwrap();
    let d = Deadline::new(60000);
    let (snapshot, graph) = engine.select(&json!({}), &d).unwrap();
    let view =
        graph_view::build(&engine.store, &snapshot, &graph, &Default::default(), &d).unwrap();
    let office = document_graph::build(&engine, "office.docx", &d).unwrap();
    let asset = media::inspect_with_worker(
        &engine,
        "notes.pdf",
        Path::new(env!("CARGO_BIN_EXE_yoke")),
        &d,
    )
    .unwrap();
    let selection = SelectionV2 {
        schema_version: 2,
        source: SourcePin {
            workspace_id: snapshot.workspace_id,
            snapshot_id: snapshot.id,
            view_id: view.id,
            view_artifact_id: view.artifact_id,
            base_graph_hash: identity(&graph),
        },
        documents: vec![DocumentPin {
            document_graph_id: office["documentGraphId"].as_str().unwrap().into(),
            artifact_id: office["artifactId"].as_str().unwrap().into(),
            container_hash: office["sourceHash"].as_str().unwrap().into(),
            decoded_text_hash: office["decodedTextHash"].as_str().unwrap().into(),
        }],
        pdfs: vec![PdfPin {
            asset_id: asset["assetId"].as_str().unwrap().into(),
            container_hash: hash(pdf()),
            extraction_policy: PDF_EXTRACTION_POLICY.into(),
        }],
    };
    Fixture {
        dir,
        engine,
        selection,
        inspection_artifact: asset["artifactId"].as_str().unwrap().into(),
    }
}
pub fn state(engine: &Engine) -> Value {
    engine.store.set_deadline(&Deadline::new(60000)).unwrap();
    let db = &engine.store.db;
    let tables:Vec<String>=db.prepare("SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%' ORDER BY name").unwrap()
        .query_map([],|r|r.get(0)).unwrap().collect::<rusqlite::Result<_>>().unwrap();
    let mut sql = BTreeMap::new();
    for table in tables {
        let mut statement = db
            .prepare(&format!("SELECT * FROM \"{}\"", table.replace('"', "\"\"")))
            .unwrap();
        let columns = statement.column_count();
        let mut rows: Vec<Vec<String>> = statement
            .query_map([], |r| {
                (0..columns)
                    .map(|n| Ok(format!("{:?}", r.get_ref(n)?)))
                    .collect::<rusqlite::Result<_>>()
            })
            .unwrap()
            .collect::<rusqlite::Result<_>>()
            .unwrap();
        rows.sort();
        sql.insert(table, rows);
    }
    fn visit(root: &Path, folder: &Path, files: &mut BTreeMap<String, String>) {
        for entry in fs::read_dir(folder).unwrap() {
            let path = entry.unwrap().path();
            let metadata = fs::symlink_metadata(&path).unwrap();
            assert!(!metadata.file_type().is_symlink());
            if metadata.is_dir() {
                visit(root, &path, files);
            } else {
                files.insert(
                    path.strip_prefix(root).unwrap().to_str().unwrap().into(),
                    hash(fs::read(path).unwrap()),
                );
            }
        }
    }
    let mut cas = BTreeMap::new();
    for folder in ["blobs", "trees"] {
        visit(&engine.store.dir, &engine.store.dir.join(folder), &mut cas);
    }
    json!({"sql":sql,"cas":cas})
}
pub fn refused<T>(result: anyhow::Result<T>, marker: &str) {
    let error = match result {
        Ok(_) => panic!("unexpected checked result"),
        Err(error) => error,
    };
    assert!(
        error.to_string().contains(marker),
        "Expected {marker}: {error:#}"
    );
}
