use super::*;
fn d() -> Deadline {
    Deadline::new(10_000)
}
fn shortcut(path: &str, value: Value) -> Shortcut {
    Shortcut::parse(path, &serde_json::to_vec(&value).unwrap(), &d()).unwrap()
}
fn received(body: &[u8]) -> ReceivedExport {
    ReceivedExport::from_received_data(
        shortcut("notes.gdoc", json!({"doc_id":"owned_123"}))
            .metadata()
            .clone(),
        ExporterClaim {
            executable_sha256: "a".repeat(64),
            version: "0.22.5".into(),
        },
        body,
        &Limits::default(),
        &d(),
    )
    .unwrap()
}
#[test]
fn pointer_aliases_and_supported_url_shapes_bind_the_original_hash() {
    for (path, value, kind) in [
        ("notes.gdoc", json!({"doc_id":"owned_123"}), Kind::Document),
        (
            "notes.GDOC",
            json!({"file_id":"owned_123","fileId":"owned_123","id":"owned_123"}),
            Kind::Document,
        ),
        (
            "sheet.gsheet",
            json!({"resource_id":"spreadsheet:owned_123"}),
            Kind::Spreadsheet,
        ),
        (
            "slides.gslides",
            json!({"url":"https://docs.google.com/presentation/d/owned_123/edit"}),
            Kind::Presentation,
        ),
        (
            "notes.gdoc",
            json!({"url":"https://drive.google.com/open?id=owned_123"}),
            Kind::Document,
        ),
        (
            "sheet.gsheet",
            json!({"url":"https://docs.google.com/spreadsheets/d/owned_123/edit?id=owned_123","fileId":"owned_123"}),
            Kind::Spreadsheet,
        ),
    ] {
        let bytes = serde_json::to_vec(&value).unwrap();
        let parsed = Shortcut::parse(path, &bytes, &d()).unwrap();
        assert_eq!(parsed.metadata().kind, kind);
        assert_eq!(parsed.metadata().file_id, "owned_123");
        assert_eq!(parsed.metadata().shortcut_sha256, model::hash(&bytes));
        parsed
            .metadata()
            .check_selection(&model::hash(&bytes), "owned_123")
            .unwrap();
        assert!(
            parsed
                .metadata()
                .check_selection(&"f".repeat(64), "owned_123")
                .is_err()
        );
        assert!(
            parsed
                .metadata()
                .check_selection(&model::hash(&bytes), "other")
                .is_err()
        );
        assert_eq!(parsed.describe()["acquisitionPerformed"], false);
        assert_eq!(parsed.describe()["executionAuthority"], "none");
    }
}
#[test]
fn pointer_data_cannot_steer_hosts_paths_aliases_or_permissions() {
    for value in [
        json!({"doc_id":"first","fileId":"second"}),
        json!({"doc_id":"first","url":"https://docs.google.com/document/d/second/edit"}),
        json!({"url":"https://evil.invalid/document/d/owned_123/edit"}),
        json!({"url":"https://docs.google.com.evil.invalid/document/d/owned_123/edit"}),
        json!({"url":"https://docs.google.com@evil.invalid/document/d/owned_123/edit"}),
        json!({"url":"http://docs.google.com/document/d/owned_123/edit"}),
        json!({"url":"https://docs.google.com:444/document/d/owned_123/edit"}),
        json!({"url":"https://docs.google.com/document/d/first/edit?id=second"}),
        json!({"doc_id":"../outside"}),
        json!({"doc_id":["owned_123"]}),
        json!({"resource_id":"presentation:owned_123"}),
        json!({"url":"https://docs.google.com/spreadsheets/d/owned_123/edit"}),
        json!({"doc_id":"owned_123","resourceKey":"first","resource_key":"second"}),
    ] {
        assert!(Shortcut::parse("notes.gdoc", &serde_json::to_vec(&value).unwrap(), &d()).is_err());
    }
    for path in [
        "../notes.gdoc",
        "/notes.gdoc",
        ".git/notes.gdoc",
        "a\\notes.gdoc",
        "AUX.gdoc",
        "notes.gdoc.",
        "notes.txt",
    ] {
        assert!(Shortcut::parse(path, br#"{"doc_id":"owned_123"}"#, &d()).is_err());
    }
    let parsed = shortcut(
        "notes.gdoc",
        json!({"doc_id":"owned_123","allowNetwork":true,"command":"hidden"}),
    );
    assert_eq!(parsed.describe()["acquisitionPerformed"], false);
    assert_eq!(parsed.describe()["executionAuthority"], "none");
}
#[test]
fn pointer_resource_keys_and_account_labels_do_not_enter_public_metadata() {
    let key = "owned_resource_key_value";
    let account = "owned-person@example.invalid";
    let parsed = shortcut(
        "notes.gdoc",
        json!({"doc_id":"owned_123","resourceKey":key,"email":account,
        "url":format!("https://docs.google.com/document/d/owned_123/edit?resourcekey={key}")}),
    );
    assert!(parsed.has_resource_key());
    assert_eq!(parsed.resource_key(), Some(key));
    let public = serde_json::to_string(&parsed.describe()).unwrap();
    assert!(!public.contains(key));
    assert!(!public.contains(account));
    assert_eq!(
        parsed.metadata().declared_account_sha256,
        Some(model::hash(account))
    );
    assert_eq!(parsed.describe()["accountIdentityVerified"], false);
}
#[test]
fn shortcut_duplicate_fields_encoding_deadline_and_capacity_fail_explicitly() {
    for bytes in [
        br#"{"doc_id":"a","doc_id":"b"}"#.as_slice(),
        b"[]",
        b"{",
        b"\xff",
    ] {
        assert!(Shortcut::parse("notes.gdoc", bytes, &d()).is_err());
    }
    let source = br#"{"doc_id":"owned_123"}"#;
    assert!(Shortcut::parse("notes.gdoc", source, &Deadline::new(0)).is_err());
    let mut too_large = source.to_vec();
    too_large.resize(shortcut::MAX_SHORTCUT_BYTES + 1, b' ');
    assert!(Shortcut::parse("notes.gdoc", &too_large, &d()).is_err());
    let mut exact = source.to_vec();
    exact.resize(shortcut::MAX_SHORTCUT_BYTES, b' ');
    Shortcut::parse("notes.gdoc", &exact, &d()).unwrap();
    let path = "caf\u{e9}/\u{03c0}.gdoc";
    assert_eq!(
        Shortcut::parse(path, source, &d())
            .unwrap()
            .metadata()
            .shortcut_path,
        path
    );
}
#[test]
fn archive_replay_and_split_unicode_reads_preserve_received_bytes() {
    let body = "# own\r\n\u{03c0}\u{1f30d}\n".as_bytes();
    let source = received(body);
    let archive = source.archive(&d()).unwrap();
    let replay = ReceivedExport::from_archive(&archive, &Limits::default(), &d()).unwrap();
    assert_eq!(archive, replay.archive(&d()).unwrap());
    let whole = replay.read(0, body.len(), &d()).unwrap();
    assert_eq!(
        BASE64_STANDARD
            .decode(whole["dataBase64"].as_str().unwrap())
            .unwrap(),
        body
    );
    assert_eq!(whole["utf8"], std::str::from_utf8(body).unwrap());
    let start = "# own\r\n".len();
    let split = replay.read(start, start + 1, &d()).unwrap();
    assert!(split["utf8"].is_null());
    assert_eq!(split["utf8Complete"], false);
    assert_eq!(
        BASE64_STANDARD
            .decode(split["dataBase64"].as_str().unwrap())
            .unwrap(),
        [0xcf]
    );
    assert!(replay.read(1, 0, &d()).is_err());
    assert!(replay.read(0, usize::MAX, &d()).is_err());
    assert!(replay.read(0, 1, &Deadline::new(0)).is_err());
    assert_eq!(replay.summary()["googleIdentityVerified"], false);
    assert_eq!(replay.summary()["shortcutBytesCaptured"], false);
    assert_eq!(replay.summary()["executionAuthority"], "none");
}
#[test]
fn archive_schema_mime_byte_hash_and_identity_forgeries_refuse() {
    let original: Value =
        serde_json::from_slice(&received(b"own bytes\n").archive(&d()).unwrap()).unwrap();
    for (pointer, value) in [
        ("/schemaVersion", json!(2)),
        ("/kind", json!("other")),
        ("/requestedMimeType", json!("text/html")),
        ("/bodyLength", json!(usize::MAX)),
        ("/bodySha256", json!("b".repeat(64))),
        ("/sourceId", json!("c".repeat(64))),
        ("/bodyBase64", json!("not base64")),
        ("/metadata/shortcutPath", json!("../notes.gdoc")),
        ("/metadata/fileId", json!("../outside")),
        ("/exporterClaim/executableSha256", json!("bad")),
    ] {
        let mut bad = original.clone();
        *bad.pointer_mut(pointer).unwrap() = value;
        assert!(
            ReceivedExport::from_archive(
                &serde_json::to_vec(&bad).unwrap(),
                &Limits::default(),
                &d()
            )
            .is_err(),
            "{pointer}"
        );
    }
    let mut bad = original;
    bad["allowNetwork"] = json!(true);
    assert!(
        ReceivedExport::from_archive(&serde_json::to_vec(&bad).unwrap(), &Limits::default(), &d())
            .is_err()
    );
}
#[test]
fn coherent_received_claims_never_become_source_or_account_authority() {
    let original = received(b"own bytes\n");
    let mut frame: Envelope = serde_json::from_slice(&original.archive(&d()).unwrap()).unwrap();
    frame.metadata.file_id = "another_declared_file".into();
    frame.source_id = source_id(&frame.metadata, &frame.exporter_claim, &frame.body_sha256);
    let replay = ReceivedExport::from_archive(
        &serde_json::to_vec(&frame).unwrap(),
        &Limits::default(),
        &d(),
    )
    .unwrap();
    assert_ne!(original.summary()["sourceId"], replay.summary()["sourceId"]);
    assert_eq!(replay.summary()["googleIdentityVerified"], false);
    assert_eq!(replay.summary()["exporterClaimVerified"], false);
    assert_eq!(replay.summary()["executionAuthority"], "none");
    assert_eq!(replay.summary()["modelCalls"], 0);
}
#[test]
fn archive_budgets_and_binary_inspection_remain_distinct_from_capture() {
    let source = received(b"own\n");
    let archive = source.archive(&d()).unwrap();
    let exact = Limits {
        max_body_bytes: 4,
        max_archive_bytes: archive.len(),
    };
    ReceivedExport::from_archive(&archive, &exact, &d()).unwrap();
    let too_small = Limits {
        max_body_bytes: 4,
        max_archive_bytes: archive.len() - 1,
    };
    assert!(ReceivedExport::from_archive(&archive, &too_small, &d()).is_err());
    for max in [0, usize::MAX] {
        assert!(
            Limits {
                max_body_bytes: max,
                max_archive_bytes: 16_000_000
            }
            .validate()
            .is_err()
        );
    }
    assert!(source.inspect_office(&d()).is_err());
    let metadata = shortcut("sheet.gsheet", json!({"fileId":"owned_123"}))
        .metadata()
        .clone();
    let binary = ReceivedExport::from_received_data(
        metadata,
        ExporterClaim {
            executable_sha256: "a".repeat(64),
            version: "0.22.5".into(),
        },
        b"PK\x03\x04not-a-workbook",
        &Limits::default(),
        &d(),
    )
    .unwrap();
    assert!(binary.inspect_office(&d()).is_err());
    assert_eq!(binary.summary()["contentInspection"], "not_requested");
    assert_eq!(binary.summary()["complete"], false);
    assert!(source.archive(&Deadline::new(0)).is_err());
}

#[test]
fn received_sheet_inspection_keeps_cached_cells_and_original_container_separate() {
    use std::io::{Cursor, Write};
    let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
    for (name, xml) in [
        (
            "[Content_Types].xml",
            r#"<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/></Types>"#,
        ),
        (
            "_rels/.rels",
            r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="xl/workbook.xml"/></Relationships>"#,
        ),
        (
            "xl/workbook.xml",
            r#"<workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><sheets><sheet name="Owned" sheetId="1" r:id="sheet1"/></sheets></workbook>"#,
        ),
        (
            "xl/_rels/workbook.xml.rels",
            r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="sheet1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/sheet1.xml"/></Relationships>"#,
        ),
        (
            "xl/worksheets/sheet1.xml",
            r#"<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><sheetData><row r="1"><c r="A1" t="inlineStr"><is><t>Owned &amp; π</t></is></c><c r="B1"><f>SUM(1,2)</f><v>3</v></c><c r="C1"><f>SUM(9,9)</f></c></row></sheetData></worksheet>"#,
        ),
    ] {
        zip.start_file(name, zip::write::SimpleFileOptions::default())
            .unwrap();
        zip.write_all(xml.as_bytes()).unwrap();
    }
    let bytes = zip.finish().unwrap().into_inner();
    let source = ReceivedExport::from_received_data(
        shortcut("sheet.gsheet", json!({"doc_id":"owned_123"}))
            .metadata()
            .clone(),
        ExporterClaim {
            executable_sha256: "a".repeat(64),
            version: "0.22.5".into(),
        },
        &bytes,
        &Limits::default(),
        &d(),
    )
    .unwrap();
    let replay =
        ReceivedExport::from_archive(&source.archive(&d()).unwrap(), &Limits::default(), &d())
            .unwrap();
    let inspected = replay.inspect_office(&d()).unwrap();
    assert_eq!(inspected["bodySha256"], model::hash(&bytes));
    assert_eq!(inspected["inspection"]["segments"][0]["text"], "Owned & π");
    assert_eq!(inspected["inspection"]["segments"][1]["text"], "3");
    assert_eq!(
        inspected["inspection"]["segments"][2]["cachedValueAvailable"],
        false
    );
    assert_eq!(
        inspected["inspection"]["metadata"]["formulaEvaluation"],
        "never"
    );
    assert_eq!(
        inspected["inspection"]["metadata"]["externalRelationships"],
        "never_followed"
    );
    assert_eq!(source.archive(&d()).unwrap(), replay.archive(&d()).unwrap());
}

#[test]
fn blank_documents_and_numbered_account_urls_remain_data_without_identity_claims() {
    for (path, prefix) in [
        ("doc.gdoc", "document"),
        ("sheet.gsheet", "spreadsheets"),
        ("slides.gslides", "presentation"),
    ] {
        let parsed = shortcut(
            path,
            json!({"url":format!("https://docs.google.com/{prefix}/u/0/d/owned_123/edit")}),
        );
        assert_eq!(parsed.metadata().file_id, "owned_123");
        assert_eq!(parsed.describe()["accountIdentityVerified"], false);
    }
    for path in ["doc.gdoc", "slides.gslides"] {
        for body in [b"".as_slice(), b" \r\n\t"] {
            let source = ReceivedExport::from_received_data(
                shortcut(path, json!({"doc_id":"owned_123"}))
                    .metadata()
                    .clone(),
                ExporterClaim {
                    executable_sha256: "a".repeat(64),
                    version: "0.22.5".into(),
                },
                body,
                &Limits::default(),
                &d(),
            )
            .unwrap();
            let replay = ReceivedExport::from_archive(
                &source.archive(&d()).unwrap(),
                &Limits::default(),
                &d(),
            )
            .unwrap();
            assert_eq!(
                replay.read(0, body.len(), &d()).unwrap()["utf8"],
                std::str::from_utf8(body).unwrap()
            );
            assert_eq!(replay.summary()["googleIdentityVerified"], false);
            assert_eq!(replay.summary()["complete"], false);
        }
    }
}
