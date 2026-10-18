use std::{
    alloc::{GlobalAlloc, Layout, System},
    io::{Cursor, Write},
    sync::atomic::{AtomicBool, AtomicUsize, Ordering},
};
use yoke::{Engine, capture::Deadline, document_graph};
struct Observe;
static ACTIVE: AtomicBool = AtomicBool::new(false);
static LARGEST: AtomicUsize = AtomicUsize::new(0);
static REVIEW: std::sync::Mutex<()> = std::sync::Mutex::new(());
#[global_allocator]
static ALLOCATOR: Observe = Observe;
unsafe impl GlobalAlloc for Observe {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if ACTIVE.load(Ordering::Relaxed) {
            LARGEST.fetch_max(layout.size(), Ordering::Relaxed);
        }
        unsafe { System.alloc(layout) }
    }
    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        unsafe { System.dealloc(pointer, layout) }
    }
    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        if ACTIVE.load(Ordering::Relaxed) {
            LARGEST.fetch_max(size, Ordering::Relaxed);
        }
        unsafe { System.realloc(pointer, layout, size) }
    }
}
fn fixture(paragraphs: usize) -> (tempfile::TempDir, Engine, Deadline) {
    let root = tempfile::tempdir().unwrap();
    let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
    let mut document = String::from(
        "<w:document xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\"><w:body>",
    );
    for _ in 0..paragraphs {
        document.push_str("<w:p><w:r><w:t>");
        document.push_str(&"a".repeat(110));
        document.push_str("</w:t></w:r></w:p>");
    }
    document.push_str("</w:body></w:document>");
    for (name, body) in [
        (
            "_rels/.rels",
            r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="main" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>"#,
        ),
        (
            "[Content_Types].xml",
            r#"<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/></Types>"#,
        ),
        ("word/document.xml", document.as_str()),
    ] {
        zip.start_file(
            name,
            zip::write::SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Deflated),
        )
        .unwrap();
        zip.write_all(body.as_bytes()).unwrap();
    }
    std::fs::write(
        root.path().join("many.docx"),
        zip.finish().unwrap().into_inner(),
    )
    .unwrap();
    let engine = Engine::open(root.path()).unwrap();
    let deadline = Deadline::new(120_000);
    (root, engine, deadline)
}
#[test]
fn document_trace_admits_serialized_output_before_large_allocation() {
    let _guard = REVIEW
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let (_root, engine, deadline) = fixture(6000);
    let built = document_graph::build(&engine, "many.docx", &deadline).unwrap();
    let checked = document_graph::load(
        &engine.store,
        built["documentGraphId"].as_str().unwrap(),
        &deadline,
    )
    .unwrap();
    let payload = document_graph::explorer_payload(&checked, &deadline).unwrap();
    let asset = payload["documentGraph"]["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|node| node["kind"] == "documentAsset")
        .unwrap()["id"]
        .clone();
    drop(payload);
    LARGEST.store(0, Ordering::SeqCst);
    ACTIVE.store(true, Ordering::SeqCst);
    let result = document_graph::trace(
        &checked,
        &serde_json::json!({"nodeIds":[asset],"direction":"out","depth":8,"maxVisited":20000}),
        &deadline,
    );
    ACTIVE.store(false, Ordering::SeqCst);
    let largest = LARGEST.load(Ordering::SeqCst);
    assert!(result.is_err(), "Oversized document result must be refused");
    assert!(
        largest <= 4_000_000,
        "Rejected result allocated {largest} bytes before output admission"
    );
}

#[test]
fn rejected_document_metadata_is_admitted_before_identity_serialization() {
    let _guard = REVIEW
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let root = tempfile::tempdir().unwrap();
    let mut worksheet = String::from(
        "<worksheet xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\"><sheetData>",
    );
    for row in 1..=12000 {
        worksheet.push_str(&format!(
            "<row r=\"{row}\"><c r=\"A{row}\" t=\"s\"><v>0</v></c></row>"
        ));
    }
    worksheet.push_str("</sheetData></worksheet>");
    let shared = format!(
        "<sst xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\"><si><t>{}</t></si></sst>",
        "a".repeat(50)
    );
    let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
    for (name, body) in [
        (
            "_rels/.rels",
            r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="main" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="xl/workbook.xml"/></Relationships>"#,
        ),
        (
            "[Content_Types].xml",
            r#"<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/></Types>"#,
        ),
        (
            "xl/workbook.xml",
            r#"<workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><sheets><sheet name="Data" sheetId="1" r:id="sheet"/></sheets></workbook>"#,
        ),
        (
            "xl/_rels/workbook.xml.rels",
            r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="sheet" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/sheet1.xml"/><Relationship Id="strings" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/sharedStrings" Target="sharedStrings.xml"/></Relationships>"#,
        ),
        ("xl/sharedStrings.xml", shared.as_str()),
        ("xl/worksheets/sheet1.xml", worksheet.as_str()),
    ] {
        zip.start_file(
            name,
            zip::write::SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Deflated),
        )
        .unwrap();
        zip.write_all(body.as_bytes()).unwrap();
    }
    std::fs::write(
        root.path().join("many.xlsx"),
        zip.finish().unwrap().into_inner(),
    )
    .unwrap();
    let engine = Engine::open(root.path()).unwrap();
    let deadline = Deadline::new(120_000);
    LARGEST.store(0, Ordering::SeqCst);
    ACTIVE.store(true, Ordering::SeqCst);
    let result = document_graph::build(&engine, "many.xlsx", &deadline);
    ACTIVE.store(false, Ordering::SeqCst);
    let largest = LARGEST.load(Ordering::SeqCst);
    assert!(
        result.is_err(),
        "Fixture must exceed the DocumentGraph metadata32MB cap"
    );
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("Document graph metadata capacity exceeded"),
        "Fixture must reach metadata admission rather than an unrelated package gate"
    );
    // The admitted XML DOM may reserve32MiB. A64MiB JSON Vec for rejected
    // metadata is a separate avoidable allocation beyond that parser budget.
    assert!(
        largest <= 33_554_432,
        "Rejected metadata allocated {largest} bytes before metadata admission"
    );
}
