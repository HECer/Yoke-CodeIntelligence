use serde_json::{Value, json};
use std::fs;
use std::time::{Duration, Instant};
use tempfile::TempDir;
use yoke::{Engine, capture::Deadline, media, model::hash};

fn fixture() -> (TempDir, Engine) {
    let root = TempDir::new().unwrap();
    fs::write(root.path().join("main.rs"), "fn main() {}\n").unwrap();
    let engine = Engine::open(root.path()).unwrap();
    (root, engine)
}
fn png() -> Vec<u8> {
    hex::decode("89504e470d0a1a0a0000000d49484452000000010000000108060000001f15c4890000000b49444154789c6360000200000500017a5eab3f0000000049454e44ae426082").unwrap()
}
fn wav() -> Vec<u8> {
    let mut out = Vec::new();
    out.extend(b"RIFF");
    out.extend(40u32.to_le_bytes());
    out.extend(b"WAVEfmt ");
    out.extend(16u32.to_le_bytes());
    out.extend(1u16.to_le_bytes());
    out.extend(1u16.to_le_bytes());
    out.extend(8000u32.to_le_bytes());
    out.extend(16000u32.to_le_bytes());
    out.extend(2u16.to_le_bytes());
    out.extend(16u16.to_le_bytes());
    out.extend(b"data");
    out.extend(4u32.to_le_bytes());
    out.extend([0u8; 4]);
    out
}
fn jpeg() -> Vec<u8> {
    // Deliberately structural JPEG fixture, not a decoder/visual-quality test.
    vec![
        0xff, 0xd8, 0xff, 0xc0, 0, 11, 8, 0, 2, 0, 3, 1, 1, 0x11, 0, 0xff, 0xda, 0, 8, 1, 1, 0, 0,
        63, 0, 0x10, 0x20, 0xff, 0xd9,
    ]
}
fn mp4() -> Vec<u8> {
    let mut file = Vec::new();
    file.extend(20u32.to_be_bytes());
    file.extend(b"ftypisom");
    file.extend(0u32.to_be_bytes());
    file.extend(b"isom");
    let mut header = vec![0u8; 100];
    header[12..16].copy_from_slice(&1000u32.to_be_bytes());
    header[16..20].copy_from_slice(&2500u32.to_be_bytes());
    file.extend(116u32.to_be_bytes());
    file.extend(b"moov");
    file.extend(108u32.to_be_bytes());
    file.extend(b"mvhd");
    file.extend(header);
    file
}
fn published_bytes(engine: &mut Engine, id: &str, encoding: &str) -> Vec<u8> {
    use base64::{Engine as _, engine::general_purpose::STANDARD};
    let mut bytes = Vec::new();
    let mut cursor = "0".to_string();
    loop {
        let page = engine
            .invoke(
                "ci_read",
                json!({"artifactId":id,"cursor":cursor,"encoding":encoding,"maxBytes":1024}),
            )
            .unwrap();
        assert_eq!(page["status"], "ok", "{page}");
        assert_eq!(page["data"]["artifactId"], id);
        assert_eq!(page["data"]["cursor"], cursor);
        let chunk = page["data"]["chunk"].as_str().unwrap();
        if encoding == "base64" {
            assert_eq!(page["data"]["encoding"], "base64");
            bytes.extend(STANDARD.decode(chunk).unwrap());
        } else {
            bytes.extend_from_slice(chunk.as_bytes());
        }
        if let Some(next) = page["data"]["nextCursor"].as_str() {
            assert_eq!(next.parse::<usize>().unwrap(), bytes.len());
            assert!(next.parse::<usize>().unwrap() > cursor.parse::<usize>().unwrap());
            cursor = next.to_string();
        } else {
            assert_eq!(page["data"]["totalBytes"], bytes.len());
            break;
        }
    }
    assert_eq!(hash(&bytes), id);
    bytes
}
fn artifact(engine: &mut Engine, result: &Value) -> Value {
    serde_json::from_slice(&published_bytes(
        engine,
        result["artifactId"].as_str().unwrap(),
        "utf8",
    ))
    .unwrap()
}

#[test]
fn native_media_metadata_is_technical_and_binary_capture_is_explicit() {
    let (root, mut engine) = fixture();
    fs::write(root.path().join("Bild 🌍.png"), png()).unwrap();
    fs::write(root.path().join("sample.wav"), wav()).unwrap();
    fs::write(root.path().join("sample.jpg"), jpeg()).unwrap();
    fs::write(root.path().join("sample.mp4"), mp4()).unwrap();
    let (source, _, _, _) = engine.index(&Deadline::new(10_000)).unwrap();
    assert!(
        !source.files.iter().any(|f| f.path.ends_with(".png")
            || f.path.ends_with(".wav")
            || f.path.ends_with(".mp4"))
    );
    let image = media::inspect(&engine, "Bild 🌍.png", &Deadline::new(10_000)).unwrap();
    assert_eq!(image["sourceHash"], hash(png()));
    assert_eq!(image["metadata"]["width"], 1);
    assert_eq!(image["metadata"]["height"], 1);
    assert_eq!(image["contentInterpretation"], "not_performed");
    assert_eq!(
        engine
            .store
            .read_blob(image["blobId"].as_str().unwrap())
            .unwrap(),
        png()
    );
    assert_eq!(
        artifact(&mut engine, &image)["workspaceId"],
        engine.store.workspace_id
    );
    let audio = media::inspect(&engine, "sample.wav", &Deadline::new(10_000)).unwrap();
    assert_eq!(audio["metadata"]["channels"], 1);
    assert_eq!(audio["metadata"]["sampleRate"], 8000);
    assert_eq!(audio["metadata"]["frames"], 2);
    assert!((audio["metadata"]["durationSeconds"].as_f64().unwrap() - 0.00025).abs() < 1e-12);
    let image = media::inspect(&engine, "sample.jpg", &Deadline::new(10_000)).unwrap();
    assert_eq!(image["metadata"]["width"], 3);
    assert_eq!(image["metadata"]["height"], 2);
    let video = media::inspect(&engine, "sample.mp4", &Deadline::new(10_000)).unwrap();
    assert_eq!(video["metadata"]["durationSeconds"], 2.5);
    assert_eq!(video["metadata"]["majorBrand"], "isom");
}

#[test]
fn malformed_and_truncated_containers_fail_without_fabricated_metadata() {
    let (root, engine) = fixture();
    for (path, bytes) in [
        ("bad.png", png()[..40].to_vec()),
        ("bad.jpg", jpeg()[..14].to_vec()),
        ("bad.wav", wav()[..46].to_vec()),
        ("bad.mp4", mp4()[..40].to_vec()),
    ] {
        fs::write(root.path().join(path), bytes).unwrap();
        assert!(
            media::inspect(&engine, path, &Deadline::new(10_000)).is_err(),
            "accepted {path}"
        );
    }
    let mut corrupt = png();
    corrupt[29] ^= 1;
    fs::write(root.path().join("corrupt.png"), corrupt).unwrap();
    assert!(media::inspect(&engine, "corrupt.png", &Deadline::new(10_000)).is_err());
}

#[test]
fn supplied_unicode_transcript_binds_asset_digest_and_discloses_unverified_authorship() {
    let (root, mut engine) = fixture();
    fs::write(root.path().join("sample.wav"), wav()).unwrap();
    let input = json!({"expectedHash":hash(wav()),"producer":"human-editor","text":"Grüße 🌍. Rechnung geprüft.","language":"de"});
    let result =
        media::ingest_transcript(&engine, "sample.wav", input.clone(), &Deadline::new(10_000))
            .unwrap();
    let data = artifact(&mut engine, &result);
    assert_eq!(data["text"], "Grüße 🌍. Rechnung geprüft.");
    assert_eq!(data["sourceHash"], hash(wav()));
    assert_eq!(
        data["origin"],
        "host_supplied_transcript_not_verified_against_asset"
    );
    assert_eq!(data["producer"], "human-editor");
    assert_eq!(result["modelCalls"], 0);
    let mut stale = input.clone();
    stale["expectedHash"] = json!("0".repeat(64));
    assert!(
        media::ingest_transcript(&engine, "sample.wav", stale, &Deadline::new(10_000)).is_err()
    );
    let mut secret = input.clone();
    secret["text"] = json!(format!("ghp_{}", "A".repeat(36)));
    assert!(
        media::ingest_transcript(&engine, "sample.wav", secret, &Deadline::new(10_000)).is_err()
    );
    let mut authority = input;
    authority["allowWrites"] = json!(true);
    assert!(
        media::ingest_transcript(&engine, "sample.wav", authority, &Deadline::new(10_000)).is_err()
    );
}

#[test]
fn explicitly_captured_binary_assets_are_published_with_exact_ids_and_paged_bytes() {
    let (root, mut engine) = fixture();
    let mut bytes = wav();
    bytes.resize(2048, 0);
    bytes[4..8].copy_from_slice(&2040u32.to_le_bytes());
    bytes[40..44].copy_from_slice(&2004u32.to_le_bytes());
    fs::write(root.path().join("long.wav"), &bytes).unwrap();
    let result = media::inspect(&engine, "long.wav", &Deadline::new(10_000)).unwrap();
    let id = result["blobId"].as_str().unwrap();
    assert_eq!(id, hash(&bytes));
    assert_eq!(result["sourceHash"], id);
    assert_eq!(published_bytes(&mut engine, id, "base64"), bytes);
    assert_eq!(artifact(&mut engine, &result)["metadata"]["frames"], 1002);
}

#[test]
fn media_persistence_refreshes_the_remaining_deadline_after_asset_capture() {
    let (root, engine) = fixture();
    let mut bytes = wav();
    bytes.resize(4_000_044, 0);
    bytes[4..8].copy_from_slice(&4_000_036u32.to_le_bytes());
    bytes[40..44].copy_from_slice(&4_000_000u32.to_le_bytes());
    fs::write(root.path().join("large.wav"), &bytes).unwrap();
    let started = Instant::now();
    let original = media::inspect(&engine, "large.wav", &Deadline::new(10_000)).unwrap();
    let baseline = started.elapsed();
    let budget_ms = (baseline.as_millis() as u64 * 3).max(500);
    let foreign = rusqlite::Connection::open(engine.store.dir.join("index.sqlite")).unwrap();
    foreign.execute_batch("BEGIN IMMEDIATE").unwrap();
    let started = Instant::now();
    let error = media::inspect(&engine, "large.wav", &Deadline::new(budget_ms)).unwrap_err();
    let elapsed = started.elapsed();
    foreign.execute_batch("ROLLBACK").unwrap();
    engine.store.clear_deadline();
    assert!(
        elapsed < Duration::from_millis(budget_ms + 250),
        "Media persistence reused the initial timeout after asset capture: budget={budget_ms}ms elapsed={elapsed:?} baseline={baseline:?} error={error}"
    );
    let count: usize = engine
        .store
        .db
        .query_row(
            "SELECT count(*) FROM records WHERE kind='native_media_asset_v1'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(count, 1);
    assert_eq!(fs::read(root.path().join("large.wav")).unwrap(), bytes);
    assert_eq!(
        engine
            .store
            .read_artifact(original["blobId"].as_str().unwrap())
            .unwrap(),
        bytes
    );
}

#[test]
fn media_cannot_bypass_source_scope_links_ignore_or_protected_paths() {
    let (root, engine) = fixture();
    fs::write(root.path().join(".gitignore"), "ignored.png\nhidden/\n").unwrap();
    fs::write(root.path().join("ignored.png"), png()).unwrap();
    fs::write(root.path().join(".env.png"), png()).unwrap();
    fs::create_dir(root.path().join("hidden")).unwrap();
    fs::write(root.path().join("hidden/in.png"), png()).unwrap();
    for path in [
        "../outside.png",
        "ignored.png",
        ".env.png",
        "hidden/in.png",
        ".yoke/index.sqlite",
        "C:/outside.png",
    ] {
        assert!(
            media::inspect(&engine, path, &Deadline::new(10_000)).is_err(),
            "accepted {path}"
        );
    }
    fs::write(root.path().join("valid.png"), png()).unwrap();
    assert!(media::inspect(&engine, "valid.png", &Deadline::new(0)).is_err());
    let external = TempDir::new().unwrap();
    fs::write(external.path().join("outside.png"), png()).unwrap();
    #[cfg(windows)]
    std::os::windows::fs::symlink_file(
        external.path().join("outside.png"),
        root.path().join("link.png"),
    )
    .unwrap();
    #[cfg(unix)]
    std::os::unix::fs::symlink(
        external.path().join("outside.png"),
        root.path().join("link.png"),
    )
    .unwrap();
    assert!(media::inspect(&engine, "link.png", &Deadline::new(10_000)).is_err());
}

fn pdf() -> Vec<u8> {
    let content = b"BT /F1 12 Tf 10 100 Td (Native invoice Gr\xfc\xdfe) Tj ET";
    pdf_with(
        b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>",
        content,
        Vec::new(),
        "",
    )
}

fn pdf_stream(content: &[u8], dictionary: &str) -> Vec<u8> {
    [
        format!("<< {dictionary} /Length {} >>\nstream\n", content.len()).as_bytes(),
        content,
        b"\nendstream",
    ]
    .concat()
}

fn pdf_with(font: &[u8], content: &[u8], extra: Vec<Vec<u8>>, resources: &str) -> Vec<u8> {
    let mut objects = vec![
        b"<< /Type /Catalog /Pages 2 0 R >>".to_vec(),
        b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_vec(),
        format!("<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] /Resources << /Font << /F1 4 0 R >> {resources} >> /Contents 5 0 R >>").into_bytes(),
        font.to_vec(),
        pdf_stream(content, ""),
    ];
    objects.extend(extra);
    pdf_objects(&objects)
}

fn pdf_objects(objects: &[Vec<u8>]) -> Vec<u8> {
    let mut out = b"%PDF-1.4\n".to_vec();
    let mut offsets = vec![0usize];
    for (i, object) in objects.iter().enumerate() {
        offsets.push(out.len());
        out.extend(format!("{} 0 obj\n", i + 1).as_bytes());
        out.extend(object);
        out.extend(b"\nendobj\n");
    }
    let xref = out.len();
    out.extend(format!("xref\n0 {}\n0000000000 65535 f \n", offsets.len()).as_bytes());
    for offset in offsets.iter().skip(1) {
        out.extend(format!("{offset:010} 00000 n \n").as_bytes());
    }
    out.extend(
        format!(
            "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n",
            offsets.len()
        )
        .as_bytes(),
    );
    out
}

fn unicode_pdf(content: &[u8], spaces: &[(&str, &str)], mappings: &[(&str, &str)]) -> Vec<u8> {
    let mut cmap = format!(
        "/CIDInit /ProcSet findresource begin\n12 dict begin\nbegincmap\n/CIDSystemInfo << /Registry (Adobe) /Ordering (UCS) /Supplement 0 >> def\n/CMapName /Fixture def\n/CMapType 2 def\n{} begincodespacerange\n",
        spaces.len()
    );
    for (start, end) in spaces {
        cmap.push_str(&format!("<{start}> <{end}>\n"));
    }
    cmap.push_str(&format!(
        "endcodespacerange\n{} beginbfchar\n",
        mappings.len()
    ));
    for (source, target) in mappings {
        cmap.push_str(&format!("<{source}> <{target}>\n"));
    }
    cmap.push_str("endbfchar\nendcmap\nCMapName currentdict /CMap defineresource pop\nend\nend\n");
    pdf_with(
        b"<< /Type /Font /Subtype /Type0 /BaseFont /Fixture /Encoding /Identity-H /ToUnicode 6 0 R /DescendantFonts [7 0 R] >>",
        content,
        vec![pdf_stream(cmap.as_bytes(), ""), b"<< /Type /Font /Subtype /CIDFontType2 /BaseFont /Fixture /CIDSystemInfo << /Registry (Adobe) /Ordering (Identity) /Supplement 0 >> >>".to_vec()],
        "",
    )
}

#[test]
fn native_pdf_worker_extracts_actual_text_without_opening_a_source_index() {
    let (root, mut engine) = fixture();
    fs::write(root.path().join("invoice.pdf"), pdf()).unwrap();
    let executable = std::path::Path::new(env!("CARGO_BIN_EXE_yoke"));
    let result =
        media::inspect_with_worker(&engine, "invoice.pdf", executable, &Deadline::new(20_000))
            .unwrap();
    let data = artifact(&mut engine, &result);
    assert_eq!(data["metadata"]["pages"], 1);
    assert_eq!(data["metadata"]["nativeTextAvailable"], true);
    assert!(
        data["segments"][0]["text"]
            .as_str()
            .unwrap()
            .contains("Native invoice Grüße")
    );
    assert_eq!(data["segments"][0]["origin"], "native_pdf_text_not_ocr");
    assert_eq!(data["resourceLimits"]["memoryBytes"], 268435456);
    assert_eq!(data["sourceHash"], hash(pdf()));
    let inspect = media::inspect(&engine, "invoice.pdf", &Deadline::new(10_000)).unwrap();
    assert_eq!(inspect["metadata"]["nativeTextAvailable"], false);
    let worker_dir = TempDir::new().unwrap();
    fs::write(worker_dir.path().join("in.pdf"), pdf()).unwrap();
    let output = std::process::Command::new(executable)
        .arg("media-pdf-worker")
        .arg("--input")
        .arg(worker_dir.path().join("in.pdf"))
        .current_dir(worker_dir.path())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!worker_dir.path().join(".yoke").exists());
}

#[test]
fn pdf_worker_deadline_and_malformed_input_fail_closed() {
    let (root, engine) = fixture();
    fs::write(root.path().join("invoice.pdf"), pdf()).unwrap();
    let executable = std::path::Path::new(env!("CARGO_BIN_EXE_yoke"));
    assert!(
        media::inspect_with_worker(&engine, "invoice.pdf", executable, &Deadline::new(0)).is_err()
    );
    fs::write(
        root.path().join("bad.pdf"),
        b"%PDF-1.4\n1 0 obj<< /Type /Catalog >>endobj\n%%EOF",
    )
    .unwrap();
    assert!(
        media::inspect_with_worker(&engine, "bad.pdf", executable, &Deadline::new(20_000)).is_err()
    );
}

#[test]
fn strict_pdf_visitor_decodes_nonbmp_tounicode_and_text_array_line_operators() {
    let (root, mut engine) = fixture();
    let cmap = b"/CIDInit /ProcSet findresource begin\n12 dict begin\nbegincmap\n/CIDSystemInfo << /Registry (Adobe) /Ordering (UCS) /Supplement 0 >> def\n/CMapName /Fixture def\n/CMapType 2 def\n1 begincodespacerange\n<0000> <ffff>\nendcodespacerange\n3 beginbfchar\n<0001> <0041>\n<0002> <d83cdf0d>\n<0003> <00fc>\nendbfchar\nendcmap\nCMapName currentdict /CMap defineresource pop\nend\nend\n";
    let unicode = pdf_with(
        b"<< /Type /Font /Subtype /Type0 /BaseFont /Fixture /Encoding /Identity-H /ToUnicode 6 0 R /DescendantFonts [7 0 R] >>",
        b"BT /F1 12 Tf <000100020003> Tj ET",
        vec![pdf_stream(cmap, ""), b"<< /Type /Font /Subtype /CIDFontType2 /BaseFont /Fixture /CIDSystemInfo << /Registry (Adobe) /Ordering (Identity) /Supplement 0 >> >>".to_vec()],
        "",
    );
    let lines = pdf_with(
        b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>",
        b"BT /F1 12 Tf 14 TL [(Invoice) -250 (paid)] TJ T* (Next) Tj (Quoted) ' 0 0 (Double) \" ET",
        Vec::new(),
        "",
    );
    let executable = std::path::Path::new(env!("CARGO_BIN_EXE_yoke"));
    for (name, bytes, text) in [
        ("unicode.pdf", unicode, "A🌍ü\n"),
        ("lines.pdf", lines, "Invoice paid\nNext\nQuoted\nDouble\n"),
    ] {
        fs::write(root.path().join(name), &bytes).unwrap();
        let result =
            media::inspect_with_worker(&engine, name, executable, &Deadline::new(20_000)).unwrap();
        let data = artifact(&mut engine, &result);
        assert_eq!(data["segments"][0]["text"], text);
        assert_eq!(data["sourceHash"], hash(&bytes));
        assert_eq!(data["metadata"]["provider"], "yoke-native-pdf-text-1");
        assert_eq!(data["metadata"]["version"], "lopdf-0.42.0");
        assert_eq!(data["metadata"]["exhaustive"], false);
        assert_eq!(
            data["metadata"]["extractionScope"],
            "direct_page_content_streams"
        );
    }
}

#[test]
fn strict_pdf_visitor_refuses_unknown_fonts_malformed_encodings_and_content() {
    let (root, engine) = fixture();
    let normal =
        b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>";
    let invalid_encoding =
        b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /UnknownEncoding >>";
    let bad_unicode = b"<< /Type /Font /Subtype /Type0 /BaseFont /Fixture /Encoding /Identity-H /ToUnicode 6 0 R >>";
    let executable = std::path::Path::new(env!("CARGO_BIN_EXE_yoke"));
    let cases = [
        (
            normal.as_slice(),
            b"BT /Missing 12 Tf (hidden) Tj ET".as_slice(),
            Vec::new(),
        ),
        (
            invalid_encoding.as_slice(),
            b"BT /F1 12 Tf (hidden) Tj ET".as_slice(),
            Vec::new(),
        ),
        (
            normal.as_slice(),
            b"BT /F1 12 Tf (unclosed".as_slice(),
            Vec::new(),
        ),
        (
            normal.as_slice(),
            b"BT (missing selected font) Tj ET".as_slice(),
            Vec::new(),
        ),
        (
            bad_unicode.as_slice(),
            b"BT /F1 12 Tf <0001> Tj ET".as_slice(),
            vec![pdf_stream(b"malformed unicode map", "")],
        ),
    ];
    for (i, (font, content, extra)) in cases.into_iter().enumerate() {
        let name = format!("invalid-{i}.pdf");
        fs::write(root.path().join(&name), pdf_with(font, content, extra, "")).unwrap();
        assert!(
            media::inspect_with_worker(&engine, &name, executable, &Deadline::new(20_000)).is_err(),
            "Unsupported or malformed text was silently accepted: {name}"
        );
    }
}

#[test]
fn strict_pdf_visitor_discloses_form_text_exclusion_and_enforces_page_text_cap() {
    let (root, mut engine) = fixture();
    let font = b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>";
    let form = pdf_stream(
        b"BT /F1 12 Tf (FORM_ONLY) Tj ET",
        "/Type /XObject /Subtype /Form /BBox [0 0 200 200] /Resources << /Font << /F1 4 0 R >> >>",
    );
    fs::write(
        root.path().join("form.pdf"),
        pdf_with(
            font,
            b"BT /F1 12 Tf (Direct text) Tj ET /Form1 Do",
            vec![form],
            "/XObject << /Form1 6 0 R >>",
        ),
    )
    .unwrap();
    let executable = std::path::Path::new(env!("CARGO_BIN_EXE_yoke"));
    let result =
        media::inspect_with_worker(&engine, "form.pdf", executable, &Deadline::new(20_000))
            .unwrap();
    let data = artifact(&mut engine, &result);
    assert_eq!(data["segments"][0]["text"], "Direct text\n");
    assert!(
        data["warnings"]
            .as_array()
            .unwrap()
            .iter()
            .any(|warning| warning.as_str().unwrap().contains("Form XObject"))
    );
    assert_eq!(data["metadata"]["exhaustive"], false);
    let content = format!("BT /F1 12 Tf ({}) Tj ET", "a".repeat(200_001));
    fs::write(
        root.path().join("too-long.pdf"),
        pdf_with(font, content.as_bytes(), Vec::new(), ""),
    )
    .unwrap();
    assert!(
        media::inspect_with_worker(&engine, "too-long.pdf", executable, &Deadline::new(20_000))
            .is_err()
    );
}

#[test]
fn strict_pdf_visitor_respects_code_spaces_and_rejects_ambiguous_or_invalid_unicode() {
    let (root, mut engine) = fixture();
    let executable = std::path::Path::new(env!("CARGO_BIN_EXE_yoke"));
    let valid = unicode_pdf(
        b"BT /F1 12 Tf <4181017a> Tj ET",
        &[("00", "7f"), ("8100", "81ff")],
        &[("41", "0041"), ("8101", "d83cdf0d"), ("7a", "007a")],
    );
    fs::write(root.path().join("mixed-length.pdf"), valid).unwrap();
    let result = media::inspect_with_worker(
        &engine,
        "mixed-length.pdf",
        executable,
        &Deadline::new(20_000),
    )
    .unwrap();
    assert_eq!(
        artifact(&mut engine, &result)["segments"][0]["text"],
        "A🌍z\n"
    );
    let invalid = [
        unicode_pdf(
            b"BT /F1 12 Tf <8101> Tj ET",
            &[("00", "ff"), ("8100", "81ff")],
            &[("81", "0058"), ("01", "0059"), ("8101", "d83cdf0d")],
        ),
        unicode_pdf(
            b"BT /F1 12 Tf <8101> Tj ET",
            &[("00", "7f"), ("8100", "81ff")],
            &[("81", "0058"), ("01", "0059"), ("8101", "d83cdf0d")],
        ),
        unicode_pdf(
            b"BT /F1 12 Tf <0001> Tj ET",
            &[("0000", "ffff")],
            &[("0001", "d800")],
        ),
        unicode_pdf(
            b"BT /F1 12 Tf <0002> Tj ET",
            &[("0000", "ffff")],
            &[("0001", "0041")],
        ),
    ];
    for (i, bytes) in invalid.into_iter().enumerate() {
        let name = format!("invalid-code-space-{i}.pdf");
        fs::write(root.path().join(&name), bytes).unwrap();
        assert!(
            media::inspect_with_worker(&engine, &name, executable, &Deadline::new(20_000)).is_err(),
            "Malformed or ambiguous ToUnicode codes were decoded as complete text: {name}"
        );
    }
}

#[test]
fn strict_pdf_visitor_validates_every_page_tree_child_count_and_parent() {
    let (root, mut engine) = fixture();
    let executable = std::path::Path::new(env!("CARGO_BIN_EXE_yoke"));
    let mut objects = vec![
        b"<< /Type /Catalog /Pages 2 0 R >>".to_vec(),
        b"<< /Type /Pages /Kids [6 0 R] /Count 1 >>".to_vec(),
        b"<< /Type /Page /Parent 6 0 R /MediaBox [0 0 200 200] /Resources << /Font << /F1 4 0 R >> >> /Contents 5 0 R >>".to_vec(),
        b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>".to_vec(),
        pdf_stream(b"BT /F1 12 Tf (Present page) Tj ET", ""),
        b"<< /Type /Pages /Parent 2 0 R /Kids [3 0 R] /Count 1 >>".to_vec(),
    ];
    fs::write(root.path().join("nested-pages.pdf"), pdf_objects(&objects)).unwrap();
    let result = media::inspect_with_worker(
        &engine,
        "nested-pages.pdf",
        executable,
        &Deadline::new(20_000),
    )
    .unwrap();
    assert_eq!(
        artifact(&mut engine, &result)["segments"][0]["text"],
        "Present page\n"
    );
    for (i, branch) in [
        "<< /Type /Pages /Parent 2 0 R /Kids [3 0 R 99 0 R] /Count 2 >>",
        "<< /Type /Pages /Parent 2 0 R /Kids [3 0 R] /Count 2 >>",
        "<< /Type /Pages /Parent 2 0 R /Kids [3 0 R 3 0 R] /Count 2 >>",
        "<< /Type /Pages /Parent 2 0 R /Kids [6 0 R] /Count 1 >>",
        "<< /Type /Pages /Parent 2 0 R /Kids [3 0 R 4 0 R] /Count 2 >>",
    ]
    .into_iter()
    .enumerate()
    {
        objects[5] = branch.as_bytes().to_vec();
        let name = format!("invalid-pages-{i}.pdf");
        fs::write(root.path().join(&name), pdf_objects(&objects)).unwrap();
        assert!(
            media::inspect_with_worker(&engine, &name, executable, &Deadline::new(20_000)).is_err(),
            "Malformed page tree was silently reduced to partial native text: {name}"
        );
    }
    objects[5] = b"<< /Type /Pages /Parent 99 0 R /Kids [3 0 R] /Count 1 >>".to_vec();
    fs::write(
        root.path().join("invalid-parent.pdf"),
        pdf_objects(&objects),
    )
    .unwrap();
    assert!(
        media::inspect_with_worker(
            &engine,
            "invalid-parent.pdf",
            executable,
            &Deadline::new(20_000)
        )
        .is_err()
    );
}
