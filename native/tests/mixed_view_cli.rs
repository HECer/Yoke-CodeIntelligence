//! Mixed queries keep native UTF-8 and reconstructed Office coordinates distinct.
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    fs,
    io::{Cursor, Write},
    path::Path,
    process::{Command, Output},
};
use yoke::{
    Engine,
    capture::Deadline,
    document_graph, graph_view,
    model::{hash, identity},
};

#[path = "support/meaning_comparison_fixture.rs"]
mod historical_support;
#[allow(dead_code)]
#[path = "support/meaning_semantics_fixture.rs"]
mod meaning_support;
const SOURCE: &str =
    "// caf\u{e9} \u{4e16}\u{754c}\r\npub fn storage() -> &'static str { \"storage policy\" }\r\n";
const DOC_XML: &str = r#"<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body><w:p><w:r><w:t>storage &amp; 世界</w:t></w:r></w:p><w:p><w:r><w:t>Second storage paragraph</w:t></w:r></w:p></w:body></w:document>"#;
fn package(parts: &[(&str, &str)]) -> Vec<u8> {
    let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
    for (name, text) in parts {
        zip.start_file(
            *name,
            zip::write::SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Deflated),
        )
        .unwrap();
        zip.write_all(text.as_bytes()).unwrap();
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
        ("word/document.xml", DOC_XML),
    ])
}
fn xlsx() -> Vec<u8> {
    package(&[
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
        (
            "xl/sharedStrings.xml",
            r#"<sst xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><si><t>storage &amp; 世界</t></si></sst>"#,
        ),
        (
            "xl/worksheets/sheet1.xml",
            r#"<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><sheetData><row r="1"><c r="A1" t="s"><v>0</v></c><c r="B1"><f>1+2</f><v>3</v></c></row></sheetData></worksheet>"#,
        ),
    ])
}
struct Input {
    _dir: tempfile::TempDir,
    engine: Engine,
    selection: Value,
    file: std::path::PathBuf,
    documents: Vec<Value>,
}
fn input() -> Input {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("lib.rs"), SOURCE).unwrap();
    fs::write(dir.path().join("notes.docx"), docx()).unwrap();
    fs::write(dir.path().join("book.xlsx"), xlsx()).unwrap();
    let mut engine = Engine::open(dir.path()).unwrap();
    let d = Deadline::new(60000);
    let (snapshot, graph) = engine.select(&json!({}), &d).unwrap();
    let view =
        graph_view::build(&engine.store, &snapshot, &graph, &Default::default(), &d).unwrap();
    let documents: Vec<_> = ["notes.docx", "book.xlsx"]
        .iter()
        .map(|path| document_graph::build(&engine, path, &d).unwrap())
        .collect();
    let pins:Vec<_>=documents.iter().map(|doc|json!({"documentGraphId":doc["documentGraphId"],"artifactId":doc["artifactId"],"containerHash":doc["sourceHash"],"decodedTextHash":doc["decodedTextHash"]})).collect();
    let selection = json!({"schemaVersion":1,"source":{"workspaceId":snapshot.workspace_id,"snapshotId":snapshot.id,"viewId":view.id,
        "viewArtifactId":view.artifact_id,"baseGraphHash":identity(&graph)},"documents":pins});
    let file = dir.path().join("host-selection.json");
    fs::write(&file, serde_json::to_vec(&selection).unwrap()).unwrap();
    Input {
        _dir: dir,
        engine,
        selection,
        file,
        documents,
    }
}
fn command(root: &Path, file: &Path, max_bytes: usize, globals: &[&str]) -> Command {
    let mut c = Command::new(env!("CARGO_BIN_EXE_yoke"));
    c.arg("--root")
        .arg(root)
        .args(["--timeout", "30000", "--max-bytes"])
        .arg(max_bytes.to_string())
        .args(globals)
        .args(["mixed-view", "--selection"])
        .arg(file);
    c
}
fn run(i: &Input, query: Value, options: &[&str]) -> Output {
    command(&i.engine.root, &i.file, 4_000_000, &[])
        .args(options)
        .arg("--query")
        .arg(serde_json::to_string(&query).unwrap())
        .output()
        .unwrap()
}
fn success(output: Output) -> Value {
    assert!(
        output.status.success(),
        "stderr:{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stdout.ends_with(b"\n"));
    serde_json::from_slice(&output.stdout).unwrap()
}
fn refused(output: Output, marker: &str) {
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    assert!(
        String::from_utf8_lossy(&output.stderr).contains(marker),
        "Expected {marker}, stderr:{}",
        String::from_utf8_lossy(&output.stderr)
    );
}
fn state(i: &Input) -> Value {
    let db = &i.engine.store.db;
    let tables:Vec<String>=db.prepare("SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%' ORDER BY name").unwrap().query_map([],|r|r.get(0)).unwrap().collect::<rusqlite::Result<_>>().unwrap();
    let mut sql = BTreeMap::new();
    for table in tables {
        let mut s = db
            .prepare(&format!("SELECT * FROM \"{}\"", table.replace('"', "\"\"")))
            .unwrap();
        let count = s.column_count();
        let mut rows = s
            .query_map([], |r| {
                (0..count)
                    .map(|n| Ok(format!("{:?}", r.get_ref(n)?)))
                    .collect::<rusqlite::Result<Vec<_>>>()
            })
            .unwrap()
            .collect::<rusqlite::Result<Vec<_>>>()
            .unwrap();
        rows.sort();
        sql.insert(table, rows);
    }
    fn files(root: &Path, path: &Path, all: &mut BTreeMap<String, String>) {
        for entry in fs::read_dir(path).unwrap() {
            let path = entry.unwrap().path();
            let meta = fs::symlink_metadata(&path).unwrap();
            assert!(!meta.file_type().is_symlink());
            if meta.is_dir() {
                files(root, &path, all)
            } else {
                all.insert(
                    path.strip_prefix(root).unwrap().to_str().unwrap().into(),
                    hash(fs::read(path).unwrap()),
                );
            }
        }
    }
    let mut cas = BTreeMap::new();
    for name in ["blobs", "trees"] {
        files(
            &i.engine.store.dir,
            &i.engine.store.dir.join(name),
            &mut cas,
        );
    }
    json!({"sql":sql,"cas":cas})
}
fn save(i: &Input) {
    fs::write(&i.file, serde_json::to_vec(&i.selection).unwrap()).unwrap();
}

#[test]
fn mixed_find_replays_code_docx_and_xlsx_without_live_files_or_writes() {
    let i = input();
    for name in ["lib.rs", "notes.docx", "book.xlsx"] {
        fs::remove_file(i.engine.root.join(name)).unwrap();
    }
    let before = state(&i);
    let found = success(run(&i, json!({"operation":"find","query":"世界"}), &[]));
    assert_eq!(found["total"], 3);
    assert_eq!(found["items"].as_array().unwrap().len(), 3);
    assert_eq!(found["truncated"], false);
    assert_eq!(found["complete"], false);
    assert_eq!(found["modelCalls"], 0);
    let items = found["items"].as_array().unwrap();
    assert_eq!(items[0]["source"]["kind"], "sourceUtf8");
    assert_eq!(
        items[0]["source"]["snapshotId"],
        i.selection["source"]["snapshotId"]
    );
    assert_eq!(items[0]["source"]["contentHash"], hash(SOURCE));
    assert_eq!(
        items[0]["source"]["startByte"],
        SOURCE.find("世界").unwrap()
    );
    assert!(
        items[1..]
            .iter()
            .all(|r| r["source"]["kind"] == "documentDecodedText"
                && !r["xmlOrigins"].as_array().unwrap().is_empty())
    );
    assert!(items.iter().all(|r| r["text"] == "世界"));
    assert_eq!(state(&i), before);
}
#[test]
fn mixed_typed_reads_preserve_utf8_and_xml_origins_and_refuse_container_offsets() {
    let i = input();
    let found = success(run(&i, json!({"operation":"find","query":"世界"}), &[]));
    let before = state(&i);
    for item in found["items"].as_array().unwrap() {
        let result = success(run(
            &i,
            json!({"operation":"read","source":item["source"]}),
            &[],
        ));
        assert_eq!(result["item"]["text"], "世界");
        assert_eq!(result["item"]["source"], item["source"]);
        let mut invalid = item["source"].clone();
        invalid["startByte"] = json!(invalid["startByte"].as_u64().unwrap() + 1);
        refused(
            run(&i, json!({"operation":"read","source":invalid}), &[]),
            "mixed_view_",
        );
    }
    let start = DOC_XML.find("世界").unwrap();
    let reference = json!({"kind":"documentPartXml","documentGraphId":i.documents[0]["documentGraphId"],"part":"word/document.xml","contentHash":hash(DOC_XML),"startByte":start,"endByte":start+"世界".len()});
    let read = success(run(&i, json!({"operation":"read","source":reference}), &[]));
    assert_eq!(read["item"]["text"], "世界");
    assert_eq!(read["item"]["containerHash"], i.documents[0]["sourceHash"]);
    refused(
        run(
            &i,
            json!({"operation":"read","source":{"kind":"opaqueContainer","documentGraphId":i.documents[0]["documentGraphId"],"startByte":0,"endByte":8}}),
            &[],
        ),
        "mixed_view_input_invalid",
    );
    assert_eq!(state(&i), before);
}
#[test]
fn global_mixed_pagination_and_case_match_oracles_are_independent_of_source_spaces() {
    let i = input();
    let all = success(run(
        &i,
        json!({"operation":"find","query":"storage","limit":20}),
        &[],
    ));
    assert_eq!(all["total"], 5);
    let page = success(run(
        &i,
        json!({"operation":"find","query":"storage","offset":2,"limit":2}),
        &[],
    ));
    assert_eq!(page["total"], 5);
    assert_eq!(page["truncated"], true);
    assert_eq!(
        page["items"],
        json!(&all["items"].as_array().unwrap()[2..4])
    );
    assert_eq!(
        success(run(
            &i,
            json!({"operation":"find","query":"STORAGE","ignoreCase":false}),
            &[]
        ))["total"],
        0
    );
    assert_eq!(
        success(run(
            &i,
            json!({"operation":"find","query":"STORAGE","ignoreCase":true}),
            &[]
        ))["total"],
        5
    );
    refused(
        run(
            &i,
            json!({"operation":"find","query":"storage","limit":1}),
            &["--max-matches", "4"],
        ),
        "mixed_view_capacity_exceeded",
    );
}
#[test]
fn every_explicit_source_and_document_pin_is_checked_before_results_or_writes() {
    let mut i = input();
    let good = i.selection.clone();
    success(run(&i, json!({"operation":"describe"}), &[]));
    let before = state(&i);
    for key in [
        "workspaceId",
        "snapshotId",
        "viewId",
        "viewArtifactId",
        "baseGraphHash",
    ] {
        i.selection = good.clone();
        i.selection["source"][key] = json!(hash("forged"));
        save(&i);
        refused(run(&i, json!({"operation":"describe"}), &[]), "mixed_view_");
        assert_eq!(state(&i), before);
    }
    for key in [
        "documentGraphId",
        "artifactId",
        "containerHash",
        "decodedTextHash",
    ] {
        i.selection = good.clone();
        i.selection["documents"][1][key] = json!(hash("forged"));
        save(&i);
        refused(run(&i, json!({"operation":"describe"}), &[]), "mixed_view_");
        assert_eq!(state(&i), before);
    }
    i.selection = good;
    save(&i);
    let path = i
        .engine
        .store
        .dir
        .join("blobs")
        .join(i.documents[1]["sourceHash"].as_str().unwrap());
    let mut bytes = fs::read(&path).unwrap();
    bytes[0] ^= 1;
    fs::write(path, bytes).unwrap();
    let corrupt = state(&i);
    refused(
        run(&i, json!({"operation":"describe"}), &[]),
        "mixed_view_parent_refused",
    );
    assert_eq!(state(&i), corrupt);
}
#[test]
fn later_document_pin_denial_precedes_any_bulk_native_source_read() {
    let mut i = input();
    i.selection["documents"]
        .as_array_mut()
        .unwrap()
        .sort_by(|a, b| {
            a["documentGraphId"]
                .as_str()
                .cmp(&b["documentGraphId"].as_str())
        });
    i.selection["documents"][1]["containerHash"] = json!(hash("forged-later-pin"));
    save(&i);
    fs::write(
        i.engine.store.dir.join("blobs").join(hash(SOURCE)),
        "poisoned earlier native source",
    )
    .unwrap();
    let before = state(&i);
    refused(
        run(&i, json!({"operation":"describe"}), &[]),
        "mixed_view_pin_mismatch",
    );
    assert_eq!(state(&i), before);
}
#[test]
fn exact_mixed_output_and_capacity_refusals_never_publish_partial_results() {
    let i = input();
    let before = state(&i);
    let query = json!({"operation":"find","query":"世界"});
    let good = run(&i, query.clone(), &[]);
    success(good.clone());
    assert!(good.stdout.len() > 1024);
    let text = serde_json::to_string(&query).unwrap();
    let exact = command(&i.engine.root, &i.file, good.stdout.len(), &[])
        .args(["--query", &text])
        .output()
        .unwrap();
    success(exact.clone());
    assert_eq!(exact.stdout, good.stdout);
    refused(
        command(&i.engine.root, &i.file, good.stdout.len() - 1, &[])
            .args(["--query", &text])
            .output()
            .unwrap(),
        "mixed_view_capacity_exceeded",
    );
    for (flag, value) in [
        ("--max-documents", "1"),
        ("--max-metadata-bytes", "0"),
        ("--max-source-bytes", "0"),
    ] {
        refused(
            run(&i, query.clone(), &[flag, value]),
            "mixed_view_capacity_exceeded",
        );
        assert_eq!(state(&i), before);
    }
    assert_eq!(state(&i), before);
}
#[test]
fn mixed_read_global_capture_and_execution_flags_refuse_with_actual_valid_parents() {
    let i = input();
    success(run(&i, json!({"operation":"describe"}), &[]));
    let before = state(&i);
    let id = hash("snapshot");
    for flags in [
        vec!["--snapshot", id.as_str()],
        vec!["--capture-config", "missing.json"],
        vec!["--include-ignored-dir", "ignored"],
        vec!["--analyzer-epoch", "9"],
        vec!["--language-server-config", "missing.json"],
        vec!["--trust-servers"],
        vec!["--allow-writes"],
    ] {
        refused(
            command(&i.engine.root, &i.file, 4_000_000, &flags)
                .output()
                .unwrap(),
            "Mixed views",
        );
        assert_eq!(state(&i), before);
    }
}
#[test]
fn malformed_mixed_selection_and_query_are_rejected_before_missing_store_initialization() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("workspace");
    fs::create_dir(&root).unwrap();
    let file = dir.path().join("selection.json");
    let valid = json!({"schemaVersion":1,"source":{"workspaceId":hash("workspace"),"snapshotId":hash("snapshot"),"viewId":hash("view"),"viewArtifactId":hash("view-artifact"),"baseGraphHash":hash("base")},"documents":[{"documentGraphId":hash("doc"),"artifactId":hash("doc-artifact"),"containerHash":hash("container"),"decodedTextHash":hash("text")}]});
    for invalid in [
        json!({}),
        {
            let mut v = valid.clone();
            v["extra"] = json!(true);
            v
        },
        {
            let mut v = valid.clone();
            v["documents"] = json!([]);
            v
        },
        {
            let mut v = valid.clone();
            v["documents"] = json!([v["documents"][0].clone(), v["documents"][0].clone()]);
            v
        },
    ] {
        fs::write(&file, serde_json::to_vec(&invalid).unwrap()).unwrap();
        refused(
            command(&root, &file, 4_000_000, &[]).output().unwrap(),
            "mixed_view_input_invalid",
        );
        assert!(!root.join(".yoke").exists());
    }
    fs::write(&file, serde_json::to_vec(&valid).unwrap()).unwrap();
    for query in [
        json!({"operation":"describe","extra":true}),
        json!({"operation":"find","query":""}),
        json!({"operation":"read","source":{"kind":"documentDecodedText","documentGraphId":hash("doc"),"segmentId":hash("segment"),"contentHash":hash("wrong-text"),"startByte":0,"endByte":1}}),
        json!({"operation":"read","source":{"kind":"documentPartXml","documentGraphId":hash("unselected"),"part":"word/document.xml","contentHash":hash("part"),"startByte":0,"endByte":1}}),
    ] {
        refused(
            command(&root, &file, 4_000_000, &[])
                .arg("--query")
                .arg(serde_json::to_string(&query).unwrap())
                .output()
                .unwrap(),
            "mixed_view_input_invalid",
        );
        assert!(!root.join(".yoke").exists());
    }
    refused(
        command(&root, &file, 4_000_000, &[]).output().unwrap(),
        "existing initialized workspace store",
    );
    assert!(!root.join(".yoke").exists());
}

#[test]
fn artifact_registration_bytes_participate_in_the_exact_aggregate_metadata_budget() {
    let i = input();
    let first = success(run(&i, json!({"operation":"describe"}), &[]));
    let object = hash(DOC_XML);
    let original: String = i
        .engine
        .store
        .db
        .query_row(
            "SELECT value FROM records WHERE kind='artifact' AND id=?1",
            [&object],
            |row| row.get(0),
        )
        .unwrap();
    let padded = format!("{}{original}", " ".repeat(4096));
    i.engine
        .store
        .db
        .execute(
            "UPDATE records SET value=?2 WHERE kind='artifact' AND id=?1",
            rusqlite::params![object, padded],
        )
        .unwrap();
    let before = state(&i);
    let second = success(run(&i, json!({"operation":"describe"}), &[]));
    assert_eq!(
        second["admittedMetadataBytes"].as_u64().unwrap(),
        first["admittedMetadataBytes"].as_u64().unwrap() + 4096
    );
    assert_eq!(second["admittedSourceBytes"], first["admittedSourceBytes"]);
    let exact = second["admittedMetadataBytes"].as_u64().unwrap();
    success(run(
        &i,
        json!({"operation":"describe"}),
        &["--max-metadata-bytes", &exact.to_string()],
    ));
    refused(
        run(
            &i,
            json!({"operation":"describe"}),
            &["--max-metadata-bytes", &(exact - 1).to_string()],
        ),
        "mixed_view_capacity_exceeded",
    );
    assert_eq!(state(&i), before);
}

#[test]
fn exact_aggregate_source_reservation_refuses_before_corrupt_source_access_and_sorts_documents() {
    let mut i = input();
    let first = run(&i, json!({"operation":"describe"}), &[]);
    let report = success(first.clone());
    i.selection["documents"].as_array_mut().unwrap().reverse();
    save(&i);
    let second = run(&i, json!({"operation":"describe"}), &[]);
    success(second.clone());
    assert_eq!(first.stdout, second.stdout);
    let source_bytes = report["admittedSourceBytes"].as_u64().unwrap();
    success(run(
        &i,
        json!({"operation":"describe"}),
        &["--max-source-bytes", &source_bytes.to_string()],
    ));
    fs::write(
        i.engine.store.dir.join("blobs").join(hash(SOURCE)),
        "poisoned earlier native source",
    )
    .unwrap();
    let before = state(&i);
    refused(
        run(
            &i,
            json!({"operation":"describe"}),
            &["--max-source-bytes", &(source_bytes - 1).to_string()],
        ),
        "mixed_view_capacity_exceeded",
    );
    assert_eq!(state(&i), before);
}

#[test]
fn mixed_library_capability_retains_frozen_bytes_after_transaction_rollback_and_cas_loss() {
    use rusqlite::limits::Limit;
    use yoke::mixed_view::{self, Options, Query, Selection};
    let i = input();
    let selection: Selection = serde_json::from_value(i.selection.clone()).unwrap();
    let d = Deadline::new(60000);
    let original = i.engine.store.db.limit(Limit::SQLITE_LIMIT_LENGTH).unwrap();
    let before = state(&i);
    let tx = i.engine.store.db.unchecked_transaction().unwrap();
    let view = mixed_view::load(&i.engine.store, &selection, &Options::default(), &d).unwrap();
    assert!(!i.engine.store.db.is_autocommit());
    assert_eq!(
        i.engine.store.db.limit(Limit::SQLITE_LIMIT_LENGTH).unwrap(),
        original
    );
    tx.rollback().unwrap();
    assert!(i.engine.store.db.is_autocommit());
    assert_eq!(state(&i), before);
    fs::remove_file(i.engine.store.dir.join("blobs").join(hash(SOURCE))).unwrap();
    fs::remove_file(
        i.engine
            .store
            .dir
            .join("blobs")
            .join(i.documents[0]["sourceHash"].as_str().unwrap()),
    )
    .unwrap();
    let query: Query = serde_json::from_value(json!({"operation":"find","query":"世界"})).unwrap();
    assert_eq!(view.query(&query, 4_000_000, &d).unwrap()["total"], 3);
    assert!(mixed_view::load(&i.engine.store, &selection, &Options::default(), &d).is_err());
    assert!(i.engine.store.db.is_autocommit());
    assert_eq!(
        i.engine.store.db.limit(Limit::SQLITE_LIMIT_LENGTH).unwrap(),
        original
    );
}

#[test]
fn mixed_load_and_query_honor_expired_and_shared_cancelled_deadlines_without_writes() {
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };
    use yoke::mixed_view::{self, Options, Query, Selection};
    let i = input();
    let selection: Selection = serde_json::from_value(i.selection.clone()).unwrap();
    let d = Deadline::new(60000);
    let view = mixed_view::load(&i.engine.store, &selection, &Options::default(), &d).unwrap();
    let before = state(&i);
    let expired = Deadline::new(0);
    assert_eq!(
        mixed_view::load(&i.engine.store, &selection, &Options::default(), &expired)
            .err()
            .unwrap()
            .to_string(),
        "mixed_view_deadline_exceeded"
    );
    assert_eq!(
        view.query(&Query::Describe {}, 4_000_000, &expired)
            .unwrap_err()
            .to_string(),
        "mixed_view_deadline_exceeded"
    );
    let cancel = Arc::new(AtomicBool::new(false));
    let result = Deadline::run_bounded(&d, Arc::clone(&cancel), || {
        let inherited = Deadline::new(120000);
        cancel.store(true, Ordering::SeqCst);
        assert_eq!(
            mixed_view::load(&i.engine.store, &selection, &Options::default(), &inherited)
                .err()
                .unwrap()
                .to_string(),
            "mixed_view_deadline_exceeded"
        );
        assert_eq!(
            view.query(&Query::Describe {}, 4_000_000, &inherited)
                .unwrap_err()
                .to_string(),
            "mixed_view_deadline_exceeded"
        );
        Ok(())
    });
    assert!(result.is_err());
    assert_eq!(state(&i), before);
}

#[test]
fn mixed_source_provenance_retains_exact_meaning_packets_coverage_and_search_scope_roles() {
    let pair = meaning_support::fixture();
    let d = Deadline::new(60000);
    fs::write(pair.engine.root.join("notes.docx"), docx()).unwrap();
    let doc = document_graph::build(&pair.engine, "notes.docx", &d).unwrap();
    let selection = json!({"schemaVersion":1,"source":{"workspaceId":pair.before.workspace_id,"snapshotId":pair.before.snapshot_id,
        "viewId":pair.before.id,"viewArtifactId":pair.before.artifact_id,"baseGraphHash":pair.before.base_graph_hash},
        "documents":[{"documentGraphId":doc["documentGraphId"],"artifactId":doc["artifactId"],"containerHash":doc["sourceHash"],"decodedTextHash":doc["decodedTextHash"]}]});
    let file = pair.dir.path().join("mixed-host-selection.json");
    fs::write(&file, serde_json::to_vec(&selection).unwrap()).unwrap();
    fs::remove_file(pair.engine.root.join("a.rs")).unwrap();
    fs::remove_file(pair.engine.root.join("b.rs")).unwrap();
    fs::remove_file(pair.engine.root.join("notes.docx")).unwrap();
    let calls = fs::read(&pair.marker).unwrap();
    let result = success(
        command(&pair.engine.root, &file, 4_000_000, &[])
            .output()
            .unwrap(),
    );
    let provenance = &result["sourceProvenance"];
    assert_eq!(provenance["viewComplete"], pair.before.complete);
    assert_eq!(
        provenance["viewLimitations"],
        json!(pair.before.limitations)
    );
    assert_eq!(provenance["overlays"], json!(pair.before.overlays));
    let layer = pair.before.meaning.as_ref().unwrap();
    assert_eq!(provenance["meaningScope"], json!(layer.scope));
    assert_eq!(provenance["meaningSelections"], json!(layer.selections));
    assert_eq!(
        provenance["meaningPackets"].as_array().unwrap().len(),
        layer.packets.len()
    );
    for (proof, packet) in provenance["meaningPackets"]
        .as_array()
        .unwrap()
        .iter()
        .zip(&layer.packets)
    {
        let expected = serde_json::to_value(packet).unwrap();
        for key in [
            "selectionIndex",
            "index",
            "inputHash",
            "unitSetHash",
            "contractHash",
            "sourceBindings",
            "provenance",
            "coverage",
        ] {
            assert_eq!(proof[key], expected[key]);
        }
    }
    let scopes = provenance["sourceScopes"].as_array().unwrap();
    assert_eq!(
        scopes
            .iter()
            .filter(|s| s["role"] == "primary_search")
            .count(),
        1
    );
    assert!(scopes.iter().all(|s| s["role"]
        == if s["snapshotId"] == pair.snapshot.id {
            "primary_search"
        } else {
            "receipt_replay_only"
        }));
    assert_eq!(fs::read(&pair.marker).unwrap(), calls);
}

#[test]
fn mixed_library_search_admission_bounds_bytes_before_whitespace_normalization() {
    use yoke::mixed_view::{self, Query, Selection};
    let i = input();
    let selection: Selection = serde_json::from_value(i.selection.clone()).unwrap();
    let d = Deadline::new(60000);
    let query = |text: String| Query::Find {
        query: text,
        ignore_case: false,
        offset: 0,
        limit: 1,
    };
    mixed_view::validate_query(&query(format!("x{}", " ".repeat(4095))), &selection, &d).unwrap();
    for text in [format!("x{}", " ".repeat(4096)), " ".repeat(8_000_000)] {
        assert_eq!(
            mixed_view::validate_query(&query(text), &selection, &d)
                .unwrap_err()
                .to_string(),
            "mixed_view_input_invalid"
        );
    }
}

#[test]
fn historical_receipt_scope_is_retained_but_never_searched_as_current_source() {
    let f = historical_support::fixture(yoke::analysis::AnalyzerEpoch::V9);
    let d = Deadline::new(60000);
    assert_ne!(f.original.id, f.snapshot.id);
    assert!(
        f.view
            .meaning
            .as_ref()
            .unwrap()
            .packets
            .iter()
            .all(|p| p.provenance.rebound)
    );
    fs::write(f.engine.root.join("notes.docx"), docx()).unwrap();
    let doc = document_graph::build(&f.engine, "notes.docx", &d).unwrap();
    let selection = json!({"schemaVersion":1,"source":{"workspaceId":f.view.workspace_id,"snapshotId":f.view.snapshot_id,"viewId":f.view.id,
        "viewArtifactId":f.view.artifact_id,"baseGraphHash":f.view.base_graph_hash},"documents":[{"documentGraphId":doc["documentGraphId"],
        "artifactId":doc["artifactId"],"containerHash":doc["sourceHash"],"decodedTextHash":doc["decodedTextHash"]}]});
    let file = f.dir.path().join("mixed-historical-selection.json");
    fs::write(&file, serde_json::to_vec(&selection).unwrap()).unwrap();
    for name in ["a.rs", "b.rs", "history.rs", "GUIDE.md", "notes.docx"] {
        fs::remove_file(f.engine.root.join(name)).unwrap();
    }
    let calls = fs::read(&f.marker).unwrap();
    let result = success(
        command(&f.engine.root, &file, 4_000_000, &[])
            .output()
            .unwrap(),
    );
    let scopes = result["sourceProvenance"]["sourceScopes"]
        .as_array()
        .unwrap();
    assert!(
        scopes
            .iter()
            .any(|s| s["snapshotId"] == f.original.id && s["role"] == "receipt_replay_only")
    );
    assert!(
        scopes
            .iter()
            .any(|s| s["snapshotId"] == f.snapshot.id && s["role"] == "primary_search")
    );
    for (term, total) in [
        ("__HISTORICAL_OLD_SOURCE_ONLY_394ace__", 0),
        ("__HISTORICAL_NEW_SOURCE_ONLY_6a52bc__", 1),
    ] {
        let query = serde_json::to_string(&json!({"operation":"find","query":term})).unwrap();
        let found = success(
            command(&f.engine.root, &file, 4_000_000, &[])
                .args(["--query", &query])
                .output()
                .unwrap(),
        );
        assert_eq!(found["total"], total);
    }
    assert_eq!(fs::read(&f.marker).unwrap(), calls);
}
