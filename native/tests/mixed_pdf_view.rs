//! Frozen native, Office and PDF text retain exact separate source spaces.
use serde_json::{Value, json};
use std::{
    fs,
    process::Command,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};
use yoke::{
    capture::Deadline,
    mixed_view::{self, Options, Query},
    model::hash,
};
#[path = "support/mixed_pdf_fixture.rs"]
mod support;
use support::*;
fn query(value: Value) -> Query {
    yoke::json::decode(value).unwrap()
}

#[test]
fn native_office_and_pdf_search_pages_and_typed_reads_survive_live_deletions() {
    let f = fixture();
    for name in ["billing.rs", "office.docx", "notes.pdf"] {
        fs::remove_file(f.dir.path().join(name)).unwrap();
    }
    let before = state(&f.engine);
    let d = Deadline::new(60000);
    let view = mixed_view::load_with_pdf(
        &f.engine.store,
        &f.selection,
        &worker(),
        &Options::default(),
        &d,
    )
    .unwrap();
    let description = view.query(&Query::Describe {}, 4_000_000, &d).unwrap();
    assert_eq!(description["selection"]["schemaVersion"], 2);
    assert_eq!(description["pdfs"][0]["containerHash"], hash(pdf()));
    for (index, text) in PAGES.iter().enumerate() {
        assert_eq!(
            description["pdfs"][0]["pages"][index]["contentHash"],
            hash(text.as_bytes())
        );
        assert_eq!(
            description["pdfs"][0]["pages"][index]["decodedTextBytes"],
            text.len()
        );
    }
    assert_eq!(description["modelCalls"], 0);
    assert_eq!(description["sourceWritten"], false);
    assert_eq!(description["complete"], false);
    assert!(
        description["limitations"]
            .as_array()
            .unwrap()
            .iter()
            .any(|line| line
                .as_str()
                .unwrap()
                .contains("Cached spreadsheet values may be stale"))
    );
    let all = view
        .query(
            &query(json!({"operation":"find","query":"STORAGE","ignoreCase":true,"limit":30})),
            4_000_000,
            &d,
        )
        .unwrap();
    assert_eq!(all["total"], 4);
    let items = all["items"].as_array().unwrap();
    assert_eq!(items[0]["source"]["kind"], "sourceUtf8");
    assert_eq!(items[1]["source"]["kind"], "documentDecodedText");
    assert_eq!(items[2]["source"]["kind"], "pdfPageText");
    assert_eq!(items[3]["source"]["page"], 2);
    let page=view.query(&query(json!({"operation":"find","query":"storage","ignoreCase":true,"offset":1,"limit":2})),4_000_000,&d).unwrap();
    assert_eq!(page["items"], json!(&items[1..3]));
    assert_eq!(page["total"], 4);
    for item in &items[2..] {
        let reference = &item["source"];
        let page = reference["page"].as_u64().unwrap() as usize;
        let start = reference["startByte"].as_u64().unwrap() as usize;
        let end = reference["endByte"].as_u64().unwrap() as usize;
        assert_eq!(reference["contentHash"], hash(PAGES[page - 1].as_bytes()));
        assert_eq!(item["text"], &PAGES[page - 1][start..end]);
        let read = view
            .query(
                &query(json!({"operation":"read","source":reference})),
                4_000_000,
                &d,
            )
            .unwrap();
        assert_eq!(read["item"]["text"], item["text"]);
        assert_eq!(read["item"]["source"], *reference);
    }
    assert_eq!(state(&f.engine), before);
}

#[test]
fn saved_inspection_text_is_ignored_and_corrupt_original_pdf_is_refused() {
    let f = fixture();
    fs::write(
        f.engine
            .store
            .dir
            .join("blobs")
            .join(&f.inspection_artifact),
        b"forged inspection claims",
    )
    .unwrap();
    let before = state(&f.engine);
    let d = Deadline::new(60000);
    let view = mixed_view::load_with_pdf(
        &f.engine.store,
        &f.selection,
        &worker(),
        &Options::default(),
        &d,
    )
    .unwrap();
    let found = view
        .query(
            &query(json!({"operation":"find","query":"Grüße"})),
            4_000_000,
            &d,
        )
        .unwrap();
    assert_eq!(found["total"], 2);
    assert_eq!(state(&f.engine), before);
    fs::write(
        f.engine.store.dir.join("blobs").join(hash(pdf())),
        b"invalid container",
    )
    .unwrap();
    let before = state(&f.engine);
    refused(
        mixed_view::load_with_pdf(
            &f.engine.store,
            &f.selection,
            &worker(),
            &Options::default(),
            &d,
        ),
        "mixed_view_parent_refused",
    );
    assert_eq!(state(&f.engine), before);
}

#[test]
fn pdf_pin_and_row_capacity_denials_precede_corrupt_native_source_reads() {
    let f = fixture();
    fs::write(
        f.engine.store.dir.join("blobs").join(hash(SOURCE)),
        b"corrupt native source",
    )
    .unwrap();
    let mut selected = f.selection.clone();
    selected.pdfs[0].container_hash = hash("wrong PDF");
    let before = state(&f.engine);
    refused(
        mixed_view::load_with_pdf(
            &f.engine.store,
            &selected,
            &worker(),
            &Options::default(),
            &Deadline::new(60000),
        ),
        "mixed_view_pin_mismatch",
    );
    assert_eq!(state(&f.engine), before);
    let id = &f.selection.pdfs[0].asset_id;
    let raw: String = f
        .engine
        .store
        .db
        .query_row(
            "SELECT value FROM records WHERE kind='native_media_asset_v1' AND id=?1",
            [id],
            |r| r.get(0),
        )
        .unwrap();
    f.engine
        .store
        .db
        .execute(
            "UPDATE records SET value=?2 WHERE kind='native_media_asset_v1' AND id=?1",
            rusqlite::params![id, raw + &" ".repeat(65000)],
        )
        .unwrap();
    let before = state(&f.engine);
    refused(
        mixed_view::load_with_pdf(
            &f.engine.store,
            &f.selection,
            &worker(),
            &Options::default(),
            &Deadline::new(60000),
        ),
        "mixed_view_capacity_exceeded",
    );
    assert_eq!(state(&f.engine), before);
}

#[test]
fn exact_combined_reservations_and_output_limits_refuse_whole_results() {
    let f = fixture();
    let d = Deadline::new(60000);
    let view = mixed_view::load_with_pdf(
        &f.engine.store,
        &f.selection,
        &worker(),
        &Options::default(),
        &d,
    )
    .unwrap();
    let describe = view.query(&Query::Describe {}, 4_000_000, &d).unwrap();
    let metadata = describe["admittedMetadataBytes"].as_u64().unwrap() as usize;
    let source = describe["admittedSourceBytes"].as_u64().unwrap() as usize;
    let options = Options {
        max_metadata_bytes: metadata,
        max_source_bytes: source,
        ..Default::default()
    };
    mixed_view::load_with_pdf(&f.engine.store, &f.selection, &worker(), &options, &d).unwrap();
    let before = state(&f.engine);
    for options in [
        Options {
            max_metadata_bytes: metadata - 1,
            ..options.clone()
        },
        Options {
            max_source_bytes: source - 1,
            ..options.clone()
        },
    ] {
        refused(
            mixed_view::load_with_pdf(&f.engine.store, &f.selection, &worker(), &options, &d),
            "mixed_view_capacity_exceeded",
        );
    }
    let size = serde_json::to_vec(&describe).unwrap().len();
    view.query(&Query::Describe {}, size, &d).unwrap();
    refused(
        view.query(&Query::Describe {}, size - 1, &d),
        "mixed_view_capacity_exceeded",
    );
    let zero = Options {
        max_matches: 0,
        ..Default::default()
    };
    let zero =
        mixed_view::load_with_pdf(&f.engine.store, &f.selection, &worker(), &zero, &d).unwrap();
    refused(
        zero.query(
            &query(json!({"operation":"find","query":"Page one"})),
            4_000_000,
            &d,
        ),
        "mixed_view_capacity_exceeded",
    );
    assert_eq!(state(&f.engine), before);
}

#[test]
fn pdf_read_refs_reject_foreign_pages_hashes_and_split_utf8_boundaries() {
    let f = fixture();
    let d = Deadline::new(60000);
    let view = mixed_view::load_with_pdf(
        &f.engine.store,
        &f.selection,
        &worker(),
        &Options::default(),
        &d,
    )
    .unwrap();
    let found = view
        .query(
            &query(json!({"operation":"find","query":"Grüße"})),
            4_000_000,
            &d,
        )
        .unwrap();
    let good = found["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|i| i["source"]["kind"] == "pdfPageText")
        .unwrap()["source"]
        .clone();
    let before = state(&f.engine);
    for (name, value) in [
        ("assetId", json!(hash("foreign"))),
        ("containerHash", json!(hash("other container"))),
        ("page", json!(1999)),
        ("contentHash", json!(hash("other decoded text"))),
        ("startByte", json!(good["startByte"].as_u64().unwrap() + 3)),
    ] {
        let mut bad = good.clone();
        bad[name] = value;
        refused(
            view.query(
                &query(json!({"operation":"read","source":bad})),
                4_000_000,
                &d,
            ),
            "mixed_view_",
        );
    }
    assert_eq!(state(&f.engine), before);
}

#[test]
fn caller_read_transaction_sqlite_limits_and_inherited_cancel_are_preserved() {
    let f = fixture();
    let d = Deadline::new(60000);
    let limit = rusqlite::limits::Limit::SQLITE_LIMIT_LENGTH;
    let original = f.engine.store.db.set_limit(limit, 900000).unwrap();
    f.engine.store.db.execute_batch("BEGIN").unwrap();
    let before = state(&f.engine);
    mixed_view::load_with_pdf(
        &f.engine.store,
        &f.selection,
        &worker(),
        &Options::default(),
        &d,
    )
    .unwrap();
    assert!(!f.engine.store.db.is_autocommit());
    assert_eq!(f.engine.store.db.limit(limit).unwrap(), 900000);
    refused(
        mixed_view::load_with_pdf(
            &f.engine.store,
            &f.selection,
            &worker(),
            &Options::default(),
            &Deadline::new(0),
        ),
        "mixed_view_deadline_exceeded",
    );
    let cancel = Arc::new(AtomicBool::new(false));
    let selected_worker = worker();
    let actual = Deadline::run_bounded(&d, Arc::clone(&cancel), || {
        let child = Deadline::new(60000);
        cancel.store(true, Ordering::SeqCst);
        refused(
            mixed_view::load_with_pdf(
                &f.engine.store,
                &f.selection,
                &selected_worker,
                &Options::default(),
                &child,
            ),
            "mixed_view_deadline_exceeded",
        );
        Ok(())
    });
    assert!(actual.is_err());
    assert!(!f.engine.store.db.is_autocommit());
    assert_eq!(f.engine.store.db.limit(limit).unwrap(), 900000);
    assert_eq!(state(&f.engine), before);
    f.engine.store.db.execute_batch("ROLLBACK").unwrap();
    f.engine.store.db.set_limit(limit, original).unwrap();
}

#[test]
fn schema2_cli_permission_and_unknown_worker_authority_refuse_before_missing_store() {
    let host = tempfile::tempdir().unwrap();
    let root = host.path().join("missing");
    let selected = host.path().join("selected.json");
    let pin = hash("pin");
    let mut value = json!({"schemaVersion":2,"source":{"workspaceId":pin,"snapshotId":pin,"viewId":pin,"viewArtifactId":pin,"baseGraphHash":pin},
        "documents":[],"pdfs":[{"assetId":pin,"containerHash":pin,"extractionPolicy":mixed_view::PDF_EXTRACTION_POLICY}]});
    fs::write(&selected, serde_json::to_vec(&value).unwrap()).unwrap();
    let result = Command::new(env!("CARGO_BIN_EXE_yoke"))
        .arg("--root")
        .arg(&root)
        .args(["mixed-view", "--selection"])
        .arg(&selected)
        .output()
        .unwrap();
    assert!(!result.status.success());
    assert!(result.stdout.is_empty());
    assert!(String::from_utf8_lossy(&result.stderr).contains("mixed_view_pdf_permission_required"));
    value["workerExecutable"] = json!("host executable authority is not selection data");
    fs::write(&selected, serde_json::to_vec(&value).unwrap()).unwrap();
    let result = Command::new(env!("CARGO_BIN_EXE_yoke"))
        .arg("--root")
        .arg(&root)
        .args(["mixed-view", "--native-pdf-text", "--selection"])
        .arg(&selected)
        .output()
        .unwrap();
    assert!(!result.status.success());
    assert!(result.stdout.is_empty());
    assert!(String::from_utf8_lossy(&result.stderr).contains("mixed_view_input_invalid"));
    assert!(!root.exists());
}

#[test]
fn non_bmp_pdf_page_references_use_original_decoded_utf8_not_glyph_or_container_offsets() {
    let mut f = fixture();
    let d = Deadline::new(60000);
    let original = unicode_pdf();
    fs::write(f.dir.path().join("unicode.pdf"), &original).unwrap();
    let asset = yoke::media::inspect_with_worker(
        &f.engine,
        "unicode.pdf",
        std::path::Path::new(env!("CARGO_BIN_EXE_yoke")),
        &d,
    )
    .unwrap();
    let id = asset["assetId"].as_str().unwrap();
    f.selection.pdfs.push(mixed_view::PdfPin {
        asset_id: id.into(),
        container_hash: hash(&original),
        extraction_policy: mixed_view::PDF_EXTRACTION_POLICY.into(),
    });
    fs::remove_file(f.dir.path().join("unicode.pdf")).unwrap();
    let before = state(&f.engine);
    let view = mixed_view::load_with_pdf(
        &f.engine.store,
        &f.selection,
        &worker(),
        &Options::default(),
        &d,
    )
    .unwrap();
    let found = view
        .query(
            &query(json!({"operation":"find","query":"🌍"})),
            4_000_000,
            &d,
        )
        .unwrap();
    let reference = found["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["source"]["assetId"] == id)
        .unwrap()["source"]
        .clone();
    assert_eq!(reference["contentHash"], hash("A🌍ü\n"));
    assert_eq!(reference["startByte"], 1);
    assert_eq!(reference["endByte"], 5);
    let read = view
        .query(
            &query(json!({"operation":"read","source":reference})),
            4_000_000,
            &d,
        )
        .unwrap();
    assert_eq!(read["item"]["text"], "🌍");
    let mut split = reference;
    split["endByte"] = json!(4);
    refused(
        view.query(
            &query(json!({"operation":"read","source":split})),
            4_000_000,
            &d,
        ),
        "mixed_view_input_invalid",
    );
    assert_eq!(state(&f.engine), before);
}

#[cfg(windows)]
#[test]
fn host_cancellation_after_pdf_spawn_refuses_and_original_process_handle_observes_exit() {
    use std::{
        ffi::c_void,
        os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle},
        sync::Mutex,
    };
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn OpenProcess(access: u32, inherit: i32, id: u32) -> *mut c_void;
        fn WaitForSingleObject(handle: *mut c_void, milliseconds: u32) -> u32;
    }
    let f = fixture();
    let d = Deadline::new(60000);
    let cancel = Arc::new(AtomicBool::new(false));
    let observed: Arc<Mutex<Option<OwnedHandle>>> = Arc::new(Mutex::new(None));
    let callback_handle = Arc::clone(&observed);
    let callback_cancel = Arc::clone(&cancel);
    let worker = worker().with_spawn_observer(move |pid, shared| {
        shared.check()?;
        let handle = unsafe { OpenProcess(0x0010_0000, 0, pid) };
        anyhow::ensure!(!handle.is_null(), "Owned PDF process handle unavailable");
        *callback_handle.lock().unwrap() = Some(unsafe { OwnedHandle::from_raw_handle(handle) });
        callback_cancel.store(true, Ordering::SeqCst);
        Ok(())
    });
    let before = state(&f.engine);
    let result = Deadline::run_bounded(&d, Arc::clone(&cancel), || {
        let child = Deadline::new(60000);
        refused(
            mixed_view::load_with_pdf(
                &f.engine.store,
                &f.selection,
                &worker,
                &Options::default(),
                &child,
            ),
            "mixed_view_deadline_exceeded",
        );
        Ok(())
    });
    assert!(result.is_err());
    let handle = observed
        .lock()
        .unwrap()
        .take()
        .expect("Actual owned worker must have been spawned");
    // The observer requests no termination. This original process handle avoids
    // PID-reuse ambiguity and independently observes producer-owned cleanup.
    assert_eq!(
        unsafe { WaitForSingleObject(handle.as_raw_handle(), 2000) },
        0
    );
    assert_eq!(state(&f.engine), before);
}
