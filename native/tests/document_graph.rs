use serde_json::json;
use std::io::{Cursor, Write};
use yoke::{Engine, capture::Deadline, document_graph};
fn package(parts: &[(&str, &str)]) -> Vec<u8> {
    let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
    for (name, body) in parts {
        zip.start_file(
            *name,
            zip::write::SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Deflated),
        )
        .unwrap();
        zip.write_all(body.as_bytes()).unwrap();
    }
    zip.finish().unwrap().into_inner()
}
fn docx() -> Vec<u8> {
    package(&[
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
            r#"<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body><w:p><w:r><w:t>Hello &amp; 世界</w:t></w:r></w:p><w:p><w:r><w:t>Second paragraph</w:t></w:r></w:p></w:body></w:document>"#,
        ),
    ])
}
#[test]
fn document_records_and_exact_binary_xml_text_artifacts_are_retention_roots() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("notes.docx"), docx()).unwrap();
    let engine = Engine::open(root.path()).unwrap();
    let d = Deadline::new(60000);
    document_graph::build(&engine, "notes.docx", &d).unwrap();
    let report = yoke::retention::plan(&engine.store, &Default::default(), &d).unwrap();
    assert_eq!(report["unknownDependencyKinds"], json!([]), "{report}");
}
#[test]
fn document_query_types_utf8_ranges_and_seed_capacity_are_strict() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("notes.docx"), docx()).unwrap();
    let engine = Engine::open(root.path()).unwrap();
    let d = Deadline::new(60000);
    let built = document_graph::build(&engine, "notes.docx", &d).unwrap();
    let checked = document_graph::load(
        &engine.store,
        built["documentGraphId"].as_str().unwrap(),
        &d,
    )
    .unwrap();
    let found = document_graph::find(&checked, &json!({"query":"世界"}), &d).unwrap();
    let segment = found["items"][0]["segmentId"].clone();
    for contradictory in [
        json!({"segmentId":segment,"part":"word/document.xml"}),
        json!({"segmentId":segment,"part":false}),
        json!({"sourceSpace":"partXml","part":"word/document.xml","segmentId":segment}),
        json!({"sourceSpace":"partXml","part":"word/document.xml","segmentId":false}),
    ] {
        assert!(
            document_graph::read(&checked, &contradictory, &d).is_err(),
            "Contradictory coordinate selector accepted: {contradictory}"
        );
    }
    assert!(
        document_graph::read(
            &checked,
            &json!({"segmentId":segment,"sourceSpace":false}),
            &d
        )
        .is_err()
    );
    assert!(
        document_graph::read(
            &checked,
            &json!({"segmentId":segment,"startByte":9,"endByte":10}),
            &d
        )
        .is_err(),
        "Byte range splits Unicode source"
    );
    assert!(
        document_graph::trace(
            &checked,
            &json!({"nodeIds":[segment],"direction":false}),
            &d
        )
        .is_err()
    );
    let payload = document_graph::explorer_payload(&checked, &d).unwrap();
    let ids: Vec<_> = payload["documentGraph"]["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .take(2)
        .map(|node| node["id"].clone())
        .collect();
    assert!(document_graph::trace(&checked, &json!({"nodeIds":ids,"maxVisited":1}), &d).is_err());
}
#[test]
fn resealed_registered_graph_cannot_invent_labels_from_the_original_container() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("notes.docx"), docx()).unwrap();
    let engine = Engine::open(root.path()).unwrap();
    let d = Deadline::new(60000);
    let built = document_graph::build(&engine, "notes.docx", &d).unwrap();
    let id = built["documentGraphId"].as_str().unwrap();
    let bytes = engine
        .store
        .read_artifact(built["artifactId"].as_str().unwrap())
        .unwrap();
    let original = String::from_utf8(bytes).unwrap();
    assert_eq!(
        yoke::model::hash(
            original
                .replacen(&format!("\"id\":\"{id}\""), "\"id\":\"\"", 1)
                .as_bytes()
        ),
        id,
        "Streaming identity must preserve the original blank-ID serialized-byte contract"
    );
    let changed = original
        .replace("Hello & 世界", "invented_without_source")
        .replacen(&format!("\"id\":\"{id}\""), "\"id\":\"\"", 1);
    assert_ne!(changed, original);
    let forged_id = yoke::model::hash(changed.as_bytes());
    let forged = changed.replacen("\"id\":\"\"", &format!("\"id\":\"{forged_id}\""), 1);
    let artifact = engine.store.artifact(forged.as_bytes()).unwrap();
    let mut record: serde_json::Value = engine
        .store
        .get_record(document_graph::RECORD_KIND, id)
        .unwrap();
    record["documentGraphId"] = json!(forged_id);
    record["artifactId"] = json!(artifact);
    engine
        .store
        .record(document_graph::RECORD_KIND, &forged_id, &record)
        .unwrap();
    assert!(document_graph::load(&engine.store, &forged_id, &d).is_err());
    assert!(yoke::retention::plan(&engine.store, &Default::default(), &d).is_err());
}
#[test]
fn archives_reject_future_schema_corrupt_objects_and_human_destination_overwrite() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("notes.docx"), docx()).unwrap();
    let engine = Engine::open(root.path()).unwrap();
    let d = Deadline::new(60000);
    let built = document_graph::build(&engine, "notes.docx", &d).unwrap();
    let checked = document_graph::load(
        &engine.store,
        built["documentGraphId"].as_str().unwrap(),
        &d,
    )
    .unwrap();
    let destination = tempfile::tempdir().unwrap();
    let manifest = destination.path().join("document-graph.json");
    std::fs::write(&manifest, "human notes").unwrap();
    assert!(document_graph::write_archive(&checked, destination.path(), &d).is_err());
    assert_eq!(std::fs::read_to_string(&manifest).unwrap(), "human notes");
    let archive = tempfile::tempdir().unwrap();
    document_graph::write_archive(&checked, archive.path(), &d).unwrap();
    let manifest = archive.path().join("document-graph.json");
    let original = std::fs::read(&manifest).unwrap();
    let mut value: serde_json::Value = serde_json::from_slice(&original).unwrap();
    value["futureAuthority"] = json!(true);
    std::fs::write(&manifest, serde_json::to_vec(&value).unwrap()).unwrap();
    assert!(document_graph::load_archive(archive.path(), &d).is_err());
    std::fs::write(&manifest, original).unwrap();
    let object = archive
        .path()
        .join("objects")
        .join(built["decodedTextHash"].as_str().unwrap());
    let mut text = std::fs::read(&object).unwrap();
    text[0] = b'X';
    std::fs::write(object, text).unwrap();
    assert!(document_graph::load_archive(archive.path(), &d).is_err());
}
#[test]
fn xlsx_shared_strings_bind_both_reference_and_value_xml_with_cached_formula_metadata() {
    let bytes = package(&[
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
            r#"<workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><other><sheet name="Data" r:id="decoy"/></other><sheets><sheet name="Data" sheetId="1" r:id="sheet"/></sheets></workbook>"#,
        ),
        (
            "xl/_rels/workbook.xml.rels",
            r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="sheet" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/sheet1.xml"/><Relationship Id="strings" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/sharedStrings" Target="sharedStrings.xml"/></Relationships>"#,
        ),
        (
            "xl/sharedStrings.xml",
            r#"<sst xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><si><t>Shared &amp; 世界</t></si></sst>"#,
        ),
        (
            "xl/worksheets/sheet1.xml",
            r#"<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><sheetData><row r="1"><c r="A1" t="s"><v>0</v></c><c r="B1"><f>1+2</f><v>3</v></c></row></sheetData></worksheet>"#,
        ),
    ]);
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("book.xlsx"), bytes).unwrap();
    let engine = Engine::open(root.path()).unwrap();
    let d = Deadline::new(60000);
    let built = document_graph::build(&engine, "book.xlsx", &d).unwrap();
    let checked = document_graph::load(
        &engine.store,
        built["documentGraphId"].as_str().unwrap(),
        &d,
    )
    .unwrap();
    let found = document_graph::find(&checked, &json!({"query":"Shared & 世界"}), &d).unwrap();
    let read = document_graph::read(
        &checked,
        &json!({"segmentId":found["items"][0]["segmentId"]}),
        &d,
    )
    .unwrap();
    let origins = read["xmlOrigins"].as_array().unwrap();
    assert!(
        origins
            .iter()
            .any(|origin| origin["role"] == "worksheetCell"
                && origin["part"] == "xl/worksheets/sheet1.xml")
    );
    assert!(
        origins
            .iter()
            .any(|origin| origin["role"] == "sharedStringValue"
                && origin["part"] == "xl/sharedStrings.xml")
    );
    let sheet_origin = origins
        .iter()
        .find(|origin| origin["role"] == "workbookSheet")
        .unwrap();
    let xml=document_graph::read(&checked,&json!({"sourceSpace":"partXml","part":sheet_origin["part"],"startByte":sheet_origin["startByte"],"endByte":sheet_origin["endByte"]}),&d).unwrap();
    assert!(
        xml["text"].as_str().unwrap().contains("r:id=\"sheet\""),
        "Sheet provenance selected an unvisited decoy element: {xml}"
    );
    let payload = document_graph::explorer_payload(&checked, &d).unwrap();
    assert!(
        payload["documentGraph"]["segments"]
            .as_array()
            .unwrap()
            .iter()
            .any(|segment| segment["cell"] == "B1" && segment["formula"] == "1+2")
    );
}
#[test]
fn docx_search_read_trace_and_archive_preserve_distinct_exact_coordinates() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("main.rs"), "fn unchanged(){}\n").unwrap();
    std::fs::write(root.path().join("notes.docx"), docx()).unwrap();
    let mut engine = Engine::open(root.path()).unwrap();
    let d = Deadline::new(60000);
    let (snapshot, canonical, _, _) = engine.index(&d).unwrap();
    let built = document_graph::build(&engine, "notes.docx", &d).unwrap();
    let id = built["documentGraphId"].as_str().unwrap();
    let checked = document_graph::load(&engine.store, id, &d).unwrap();
    let hits = document_graph::find(&checked, &json!({"query":"世界"}), &d).unwrap();
    let hit = &hits["items"][0];
    assert_eq!(hit["text"], "世界");
    assert_eq!(hit["sourceSpace"], "decodedText");
    let segment = hit["segmentId"].clone();
    let text = document_graph::read(&checked, &json!({"segmentId":segment}), &d).unwrap();
    assert_eq!(text["text"], "Hello & 世界");
    assert_eq!(text["sourceSpace"], "decodedText");
    let origin = &text["xmlOrigins"][0];
    let xml=document_graph::read(&checked,&json!({"sourceSpace":"partXml","part":origin["part"],"startByte":origin["startByte"],"endByte":origin["endByte"]}),&d).unwrap();
    assert!(xml["text"].as_str().unwrap().contains("&amp;"));
    assert_eq!(xml["sourceSpace"], "partXml");
    let traced = document_graph::trace(
        &checked,
        &json!({"nodeIds":[segment],"direction":"in","depth":2}),
        &d,
    )
    .unwrap();
    assert!(
        traced["nodes"]
            .as_array()
            .unwrap()
            .iter()
            .any(|node| node["kind"] == "documentAsset")
    );
    assert_eq!(engine.store.latest().unwrap(), Some(snapshot.id.clone()));
    assert_eq!(
        yoke::model::identity(&engine.store.graph(&snapshot.id).unwrap()),
        yoke::model::identity(&canonical)
    );
    let archive = tempfile::tempdir().unwrap();
    document_graph::write_archive(&checked, archive.path(), &d).unwrap();
    std::fs::remove_file(root.path().join("notes.docx")).unwrap();
    drop(engine);
    let archived = document_graph::load_archive(archive.path(), &d).unwrap();
    assert_eq!(
        document_graph::find(&archived, &json!({"query":"世界"}), &d).unwrap()["items"][0]["text"],
        "世界"
    );
    let html = document_graph::explorer(&archived, &d).unwrap();
    assert!(String::from_utf8(html).unwrap().contains("Document graph"));
}
