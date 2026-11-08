use std::io::{Cursor, Write};
use yoke::{Engine, capture::Deadline};
fn package(parts: &[(&str, &str)]) -> Vec<u8> {
    let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
    for (name, body) in parts {
        writer
            .start_file(
                *name,
                zip::write::SimpleFileOptions::default()
                    .compression_method(zip::CompressionMethod::Deflated),
            )
            .unwrap();
        writer.write_all(body.as_bytes()).unwrap();
    }
    writer.finish().unwrap().into_inner()
}
const RELS: &str = r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>"#;
const WORD_TYPES: &str = r#"<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/></Types>"#;
fn word_package(xml: &str) -> Vec<u8> {
    package(&[
        ("_rels/.rels", RELS),
        ("[Content_Types].xml", WORD_TYPES),
        ("word/document.xml", xml),
    ])
}
fn word_xml(text: &str) -> String {
    format!(
        "<w:document xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\"><w:body><w:p><w:r><w:t>{text}</w:t></w:r></w:p></w:body></w:document>"
    )
}

#[test]
fn unicode_zip_extra_fields_cannot_replace_admitted_part_identity() {
    let mut bytes = package(&[
        ("_rels/.rels", RELS),
        ("[Content_Types].xml", WORD_TYPES),
        ("word/document.xml", &word_xml("original")),
        ("harmless.xml", &word_xml("replacement")),
    ]);
    let raw_name = b"harmless.xml";
    let central = bytes
        .windows(4)
        .enumerate()
        .find_map(|(offset, signature)| {
            (signature == b"PK\x01\x02"
                && bytes.get(offset + 46..offset + 46 + raw_name.len()) == Some(raw_name))
            .then_some(offset)
        })
        .unwrap();
    let mut crc = !0u32;
    for byte in raw_name {
        crc ^= *byte as u32;
        for _ in 0..8 {
            crc = (crc >> 1) ^ (0xedb88320u32 & 0u32.wrapping_sub(crc & 1));
        }
    }
    let mut payload = vec![1];
    payload.extend_from_slice(&(!crc).to_le_bytes());
    payload.extend_from_slice(b"../outside.xml");
    let mut extra = 0x7075u16.to_le_bytes().to_vec();
    extra.extend_from_slice(&(payload.len() as u16).to_le_bytes());
    extra.extend_from_slice(&payload);
    let old_extra = u16::from_le_bytes(bytes[central + 30..central + 32].try_into().unwrap());
    bytes[central + 30..central + 32]
        .copy_from_slice(&(old_extra + extra.len() as u16).to_le_bytes());
    let insert = central + 46 + raw_name.len() + old_extra as usize;
    bytes.splice(insert..insert, extra.iter().copied());
    let end = bytes.len() - 22;
    let directory_size = u32::from_le_bytes(bytes[end + 12..end + 16].try_into().unwrap());
    bytes[end + 12..end + 16].copy_from_slice(&(directory_size + extra.len() as u32).to_le_bytes());
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("alias.docx"), bytes).unwrap();
    let engine = Engine::open(root.path()).unwrap();
    assert!(
        yoke::media::inspect(&engine, "alias.docx", &Deadline::new(20000)).is_err(),
        "A decoded Unicode extra field replaced an already admitted package part"
    );
}

#[test]
fn forbidden_xml_scalars_cannot_enter_persisted_office_text() {
    for value in ["&#xFFFF;", "&#xFFFE;", "&#x1;"] {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(
            root.path().join("invalid.docx"),
            word_package(&word_xml(value)),
        )
        .unwrap();
        let engine = Engine::open(root.path()).unwrap();
        assert!(
            yoke::media::inspect(&engine, "invalid.docx", &Deadline::new(20000)).is_err(),
            "Forbidden reference {value} was accepted"
        );
    }
}

#[test]
fn package_paths_bounds_entities_secrets_and_crc_fail_before_persistence() {
    let valid = word_xml("safe text");
    let mut oversized = word_package(&valid);
    let central = oversized
        .windows(4)
        .position(|bytes| bytes == b"PK\x01\x02")
        .unwrap();
    oversized[central + 24..central + 28].copy_from_slice(&16_000_001u32.to_le_bytes());
    let mut corrupt = word_package(&valid);
    let central = corrupt
        .windows(4)
        .position(|bytes| bytes == b"PK\x01\x02")
        .unwrap();
    corrupt[central + 16] ^= 0xff;
    let secret =
        word_xml(&format!("sk-{}<w:r/> {}", "A".repeat(16), "A".repeat(16))).replace("<w:r/> ", "");
    let split_secret = word_xml(&format!(
        "sk-{}</w:t><w:t>{}",
        "A".repeat(16),
        "A".repeat(16)
    ));
    let dtd = format!(
        "<!DOCTYPE w:document [<!ENTITY remote SYSTEM 'file:///never-read'>]>{}",
        word_xml("&remote;")
    );
    let namespace = word_xml("safe").replace(
        "<w:document ",
        &format!("<w:document xmlns:unused=\"{}\" ", "x".repeat(1000)),
    );
    let cases = vec![
        oversized,
        corrupt,
        word_package(&secret),
        word_package(&split_secret),
        word_package(&dtd),
        word_package(&namespace),
        package(&[
            ("_rels/.rels", RELS),
            ("[Content_Types].xml", WORD_TYPES),
            ("word/document.xml", &valid),
            ("../outside.txt", "outside"),
        ]),
        package(&[
            ("_rels/.rels", RELS),
            ("[Content_Types].xml", WORD_TYPES),
            ("word/document.xml", &valid),
            ("WORD/DOCUMENT.XML", &valid),
        ]),
    ];
    for (index, bytes) in cases.into_iter().enumerate() {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("invalid.docx"), bytes).unwrap();
        let engine = Engine::open(root.path()).unwrap();
        assert!(
            yoke::media::inspect(&engine, "invalid.docx", &Deadline::new(20000)).is_err(),
            "Unsafe package fixture {index} was accepted"
        );
        assert_eq!(
            engine
                .store
                .db
                .query_row(
                    "SELECT count(*) FROM records WHERE kind='native_media_asset_v1'",
                    [],
                    |row| row.get::<_, usize>(0)
                )
                .unwrap(),
            0,
            "Fixture {index} persisted an asset"
        );
        assert!(!root.path().join("outside.txt").exists());
    }
}

#[test]
fn public_cli_reads_office_without_starting_office_or_a_worker() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(
        root.path().join("fixture.docx"),
        word_package(&word_xml("Public CLI text")),
    )
    .unwrap();
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_yoke"))
        .arg("--root")
        .arg(root.path())
        .args(["media", "inspect", "fixture.docx"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["segments"][0]["text"], "Public CLI text");
    assert_eq!(value["modelCalls"], 0);
    assert_eq!(value["sourceWritten"], false);
}

#[test]
fn public_office_cli_retains_large_results_as_artifacts_within_its_output_budget() {
    let root = tempfile::tempdir().unwrap();
    let paragraphs = (0..60)
        .map(|index| format!("<w:p><w:r><w:t>Paragraph {index}</w:t></w:r></w:p>"))
        .collect::<String>();
    let xml = format!(
        "<w:document xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\"><w:body>{paragraphs}</w:body></w:document>"
    );
    std::fs::write(root.path().join("fixture.docx"), word_package(&xml)).unwrap();
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_yoke"))
        .arg("--root")
        .arg(root.path())
        .args(["--max-bytes", "1024", "media", "inspect", "fixture.docx"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        output.stdout.len() <= 1024,
        "CLI exceeded requested output capacity: {}",
        output.stdout.len()
    );
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["status"], "partial");
    let engine = Engine::open(root.path()).unwrap();
    let stored: serde_json::Value = serde_json::from_slice(
        &engine
            .store
            .read_artifact(value["artifactId"].as_str().unwrap())
            .unwrap(),
    )
    .unwrap();
    assert_eq!(stored["metadata"]["format"], "docx");
    assert_eq!(stored["segments"].as_array().unwrap().len(), 60);
}
#[test]
fn native_docx_inspection_decodes_source_bound_unicode_paragraphs_and_tables() {
    let root = tempfile::tempdir().unwrap();
    let xml = r#"<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body><w:p><w:r><w:t>Grüße &amp; 世界</w:t><w:tab/><w:t>one</w:t><w:br/><w:t>two</w:t></w:r></w:p><w:tbl><w:tr><w:tc><w:p><w:r><w:t>Cell</w:t></w:r></w:p></w:tc></w:tr></w:tbl></w:body></w:document>"#;
    let bytes = package(&[
        ("_rels/.rels", RELS),
        (
            "[Content_Types].xml",
            r#"<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/></Types>"#,
        ),
        ("word/document.xml", xml),
    ]);
    std::fs::write(root.path().join("fixture.docx"), &bytes).unwrap();
    let engine = Engine::open(root.path()).unwrap();
    let observed = yoke::media::inspect(&engine, "fixture.docx", &Deadline::new(20000)).unwrap();
    assert_eq!(observed["metadata"]["format"], "docx");
    assert_eq!(observed["metadata"]["nativeTextAvailable"], true);
    let paragraphs = observed["segments"].as_array().unwrap();
    assert_eq!(paragraphs.len(), 2);
    assert_eq!(paragraphs[0]["text"], "Grüße & 世界\tone\ntwo");
    assert_eq!(paragraphs[1]["text"], "Cell");
    for paragraph in paragraphs {
        assert_eq!(paragraph["part"], "word/document.xml");
        assert_eq!(paragraph["partHash"], yoke::model::hash(xml));
        let start = paragraph["xmlStartByte"].as_u64().unwrap() as usize;
        let end = paragraph["xmlEndByte"].as_u64().unwrap() as usize;
        assert!(xml.get(start..end).unwrap().starts_with("<w:p>"));
    }
    assert_eq!(observed["sourceHash"], yoke::model::hash(&bytes));
    assert!(!root.path().join("word").exists());
}

#[test]
fn native_xlsx_keeps_shared_inline_values_and_formula_cache_distinct() {
    let root = tempfile::tempdir().unwrap();
    let types = r#"<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/></Types>"#;
    let rels = RELS.replace("word/document.xml", "xl/workbook.xml");
    let workbook = r#"<workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><sheets><sheet name="Daten &amp; 世界" sheetId="1" r:id="sheet1"/></sheets></workbook>"#;
    let links = r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="sheet1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/sheet1.xml"/><Relationship Id="strings" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/sharedStrings" Target="sharedStrings.xml"/><Relationship Id="external" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/externalLink" TargetMode="External" Target="https://example.invalid/never-read"/></Relationships>"#;
    let strings = r#"<sst xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><si><r><t>Grü</t></r><r><t>ße &amp; 世界</t></r><rPh><t>phonetic is not display text</t></rPh></si></sst>"#;
    let worksheet = r#"<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><sheetData><row r="1"><c r="A1" t="s"><v>0</v></c><c r="B1" t="inlineStr"><is><t>inline</t></is></c><c r="C1"><f>SUM(1,2)</f><v>3</v></c><c r="D1"><f>SUM(9,9)</f></c><c r="E1" t="b"><v>1</v></c></row></sheetData></worksheet>"#;
    std::fs::write(
        root.path().join("fixture.xlsx"),
        package(&[
            ("[Content_Types].xml", types),
            ("_rels/.rels", &rels),
            ("xl/workbook.xml", workbook),
            ("xl/_rels/workbook.xml.rels", links),
            ("xl/sharedStrings.xml", strings),
            ("xl/worksheets/sheet1.xml", worksheet),
        ]),
    )
    .unwrap();
    let engine = Engine::open(root.path()).unwrap();
    let observed = yoke::media::inspect(&engine, "fixture.xlsx", &Deadline::new(20000)).unwrap();
    let cells = observed["segments"].as_array().unwrap();
    assert_eq!(cells.len(), 5);
    assert_eq!(cells[0]["text"], "Grüße & 世界");
    assert_eq!(cells[0]["sheet"], "Daten & 世界");
    assert_eq!(cells[1]["text"], "inline");
    assert_eq!(cells[2]["text"], "3");
    assert_eq!(cells[2]["formula"], "SUM(1,2)");
    assert_eq!(cells[2]["formulaEvaluated"], false);
    assert_eq!(cells[3]["cachedValueAvailable"], false);
    assert_eq!(cells[4]["text"], "true");
    assert_eq!(
        observed["metadata"]["externalRelationships"],
        "never_followed"
    );
}

#[test]
fn office_refuses_wrong_main_root_instead_of_extracting_fake_document_text() {
    let root = tempfile::tempdir().unwrap();
    let xml = r#"<w:notADocument xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:p><w:r><w:t>fabricated main document</w:t></w:r></w:p></w:notADocument>"#;
    let types = r#"<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/></Types>"#;
    std::fs::write(
        root.path().join("invalid.docx"),
        package(&[
            ("_rels/.rels", RELS),
            ("[Content_Types].xml", types),
            ("word/document.xml", xml),
        ]),
    )
    .unwrap();
    let engine = Engine::open(root.path()).unwrap();
    assert!(yoke::media::inspect(&engine, "invalid.docx", &Deadline::new(20000)).is_err());
    assert_eq!(
        engine
            .store
            .db
            .query_row(
                "SELECT count(*) FROM records WHERE kind='native_media_asset_v1'",
                [],
                |row| row.get::<_, usize>(0)
            )
            .unwrap(),
        0
    );
}
