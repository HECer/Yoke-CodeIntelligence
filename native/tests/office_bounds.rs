use std::{
    alloc::{GlobalAlloc, Layout, System},
    io::{Cursor, Write},
    sync::atomic::{AtomicBool, AtomicUsize, Ordering},
};
struct Observed;
static ACTIVE: AtomicBool = AtomicBool::new(false);
static TOTAL: AtomicUsize = AtomicUsize::new(0);
static LARGEST: AtomicUsize = AtomicUsize::new(0);
unsafe impl GlobalAlloc for Observed {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if ACTIVE.load(Ordering::Relaxed) {
            TOTAL.fetch_add(layout.size(), Ordering::Relaxed);
            LARGEST.fetch_max(layout.size(), Ordering::Relaxed);
        }
        unsafe { System.alloc(layout) }
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        if ACTIVE.load(Ordering::Relaxed) {
            TOTAL.fetch_add(layout.size(), Ordering::Relaxed);
            LARGEST.fetch_max(layout.size(), Ordering::Relaxed);
        }
        unsafe { System.alloc_zeroed(layout) }
    }
    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        if ACTIVE.load(Ordering::Relaxed) {
            TOTAL.fetch_add(size, Ordering::Relaxed);
            LARGEST.fetch_max(size, Ordering::Relaxed);
        }
        unsafe { System.realloc(pointer, layout, size) }
    }
    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        unsafe { System.dealloc(pointer, layout) }
    }
}
#[global_allocator]
static ALLOCATOR: Observed = Observed;

#[test]
fn rejected_office_text_is_admitted_before_dom_and_extraction_copies() {
    let xml = format!(
        "<w:document xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\"><w:body><w:p><w:r><w:t>{}</w:t></w:r></w:p></w:body></w:document>",
        "a".repeat(13_000_000)
    );
    let rels = r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="main" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>"#;
    let types = r#"<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/></Types>"#;
    let mut writer = zip::ZipWriter::new(Cursor::new(vec![]));
    for (name, body) in [
        ("_rels/.rels", rels),
        ("[Content_Types].xml", types),
        ("word/document.xml", xml.as_str()),
    ] {
        writer
            .start_file(
                name,
                zip::write::SimpleFileOptions::default()
                    .compression_method(zip::CompressionMethod::Deflated),
            )
            .unwrap();
        writer.write_all(body.as_bytes()).unwrap();
    }
    let root = tempfile::tempdir().unwrap();
    std::fs::write(
        root.path().join("large.docx"),
        writer.finish().unwrap().into_inner(),
    )
    .unwrap();
    let engine = yoke::Engine::open(root.path()).unwrap();
    yoke::paths::obvious_secret("warm static pattern");
    TOTAL.store(0, Ordering::Relaxed);
    ACTIVE.store(true, Ordering::Relaxed);
    let result = yoke::media::inspect(&engine, "large.docx", &yoke::capture::Deadline::new(20000));
    ACTIVE.store(false, Ordering::Relaxed);
    assert!(result.is_err());
    let allocated = TOTAL.load(Ordering::Relaxed);
    assert!(
        allocated < 48_000_000,
        "Rejected text incurred {allocated} allocation bytes after bounded decompression"
    );
    // Short cell contents can still produce a huge report when a valid long
    // package part name appears in every source anchor. Admit output as rows are
    // produced, before allocating the complete rejected serialization.
    let path = format!("xl/worksheets/{}.xml", "x".repeat(900));
    let target = path.strip_prefix("xl/").unwrap();
    let workbook = r#"<workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><sheets><sheet name="Data" sheetId="1" r:id="sheet"/></sheets></workbook>"#;
    let relations = format!(
        "<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\"><Relationship Id=\"sheet\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet\" Target=\"{target}\"/></Relationships>"
    );
    let main_rels = rels.replace("word/document.xml", "xl/workbook.xml");
    let main_types = types
        .replace("/word/document.xml", "/xl/workbook.xml")
        .replace(
            "wordprocessingml.document.main+xml",
            "spreadsheetml.sheet.main+xml",
        );
    let rows = (1..=20000)
        .map(|row| format!("<row r=\"{row}\"><c r=\"A{row}\"><v>1</v></c></row>"))
        .collect::<String>();
    let worksheet = format!(
        "<worksheet xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\"><sheetData>{rows}</sheetData></worksheet>"
    );
    let mut writer = zip::ZipWriter::new(Cursor::new(vec![]));
    for (name, body) in [
        ("_rels/.rels", main_rels.as_str()),
        ("[Content_Types].xml", main_types.as_str()),
        ("xl/workbook.xml", workbook),
        ("xl/_rels/workbook.xml.rels", relations.as_str()),
        (path.as_str(), worksheet.as_str()),
    ] {
        writer
            .start_file(
                name,
                zip::write::SimpleFileOptions::default()
                    .compression_method(zip::CompressionMethod::Deflated),
            )
            .unwrap();
        writer.write_all(body.as_bytes()).unwrap();
    }
    std::fs::write(
        root.path().join("large.xlsx"),
        writer.finish().unwrap().into_inner(),
    )
    .unwrap();
    LARGEST.store(0, Ordering::Relaxed);
    ACTIVE.store(true, Ordering::Relaxed);
    let result = yoke::media::inspect(&engine, "large.xlsx", &yoke::capture::Deadline::new(20000));
    ACTIVE.store(false, Ordering::Relaxed);
    assert!(result.is_err());
    let largest = LARGEST.load(Ordering::Relaxed);
    assert!(
        largest < 24_000_000,
        "Rejected report allocated {largest} bytes in one block before output admission"
    );
}
