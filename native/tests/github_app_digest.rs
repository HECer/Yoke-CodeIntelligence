//! Native captured-source digest contracts; no models, project execution or HTTP.
use serde_json::{Value, json};
use yoke::{
    Engine,
    capture::Deadline,
    github_app::{self as app, digest},
};

#[path = "support/github_app_digest_git.rs"]
mod captured_git;

const SOURCE: &str = "// café\r\npub fn café_api() -> &'static str { \"PRIVATE_IMPLEMENTATION_CANARY\" }\r\npub fn second_api() -> u32 { 2 }\r\npub(crate) fn crate_only() -> u32 { 7 }\r\nfn hidden() -> u32 { 9 }\r\n";
fn host() -> app::Host {
    app::Host::with_signing_key(
        app::HostProfile::parse(
            &serde_json::to_vec(&json!({
                "schemaVersion":1,"appId":123,"secretCredential":"not-read",
                "grants":[{"installationId":42,"repositoryId":9,"repository":"acme/demo"}]
            }))
            .unwrap(),
        )
        .unwrap(),
        app::SigningKey::from_host_bytes(b"public deterministic owned native digest fixture key")
            .unwrap(),
    )
    .unwrap()
}
fn request(snapshot: &str) -> digest::PreviewRequest {
    digest::PreviewRequest {
        schema_version: 1,
        snapshot_id: snapshot.into(),
        installation_id: 42,
        repository_id: 9,
        repository: "acme/demo".into(),
    }
}
fn marker(result: anyhow::Result<Value>, expected: &str) {
    let error = result.expect_err("native digest must refuse");
    assert!(format!("{error:#}").contains(expected), "{error:#}");
}
fn fixture() -> (tempfile::TempDir, Engine, String, app::Host, Deadline) {
    fixture_with(SOURCE)
}
fn fixture_with(source: &str) -> (tempfile::TempDir, Engine, String, app::Host, Deadline) {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("café.rs"), source.as_bytes()).unwrap();
    let d = Deadline::new(30_000);
    let mut engine = Engine::open(root.path()).unwrap();
    let id = engine.index(&d).unwrap().0.id;
    (root, engine, id, host(), d)
}
fn cas(engine: &Engine) -> std::collections::BTreeMap<String, String> {
    std::fs::read_dir(engine.store.dir.join("blobs"))
        .unwrap()
        .map(|entry| {
            let entry = entry.unwrap();
            (
                entry.file_name().into_string().unwrap(),
                yoke::model::hash(std::fs::read(entry.path()).unwrap()),
            )
        })
        .collect()
}
fn run(
    engine: &Engine,
    host: &app::Host,
    request: &digest::PreviewRequest,
    o: &digest::Options,
    d: &Deadline,
) -> anyhow::Result<Value> {
    digest::preview(
        &digest::Context {
            store: &engine.store,
            host,
            options: o,
        },
        request,
        d,
    )
}

#[test]
fn valid_closed_digest_request_is_admitted_without_provider_or_body_fields() {
    let valid = serde_json::to_vec(&request(&"a".repeat(64))).unwrap();
    let parsed = digest::PreviewRequest::parse(&valid).unwrap();
    assert_eq!(parsed.repository, "acme/demo");
    assert_eq!(parsed.snapshot_id, "a".repeat(64));
}

#[test]
fn digest_request_rejects_duplicate_keys_urls_bodies_and_invalid_native_identities() {
    let valid = serde_json::to_value(request(&"a".repeat(64))).unwrap();
    assert!(digest::PreviewRequest::parse(&serde_json::to_vec(&valid).unwrap()).is_ok());
    for (key, value) in [
        ("url", json!("https://attacker.invalid")),
        ("body", json!("untrusted digest")),
        ("token", json!("caller-token")),
        ("snapshotId", json!("../source")),
        ("repository", json!("acme/demo/escape")),
        ("repositoryId", json!(0)),
        ("installationId", json!(9_007_199_254_740_992u64)),
    ] {
        let mut bad = valid.clone();
        bad[key] = value;
        assert!(digest::PreviewRequest::parse(&serde_json::to_vec(&bad).unwrap()).is_err());
    }
    let duplicate = format!(
        "{{\"schemaVersion\":1,\"schemaVersion\":1,\"snapshotId\":\"{}\",\"installationId\":42,\"repositoryId\":9,\"repository\":\"acme/demo\"}}",
        "a".repeat(64)
    );
    assert!(digest::PreviewRequest::parse(duplicate.as_bytes()).is_err());
}

#[test]
fn native_digest_projects_only_public_headers_and_exact_captured_symbol_fingerprints() {
    let (root, engine, id, host, d) = fixture();
    let changes = engine.store.db.total_changes();
    let blobs = cas(&engine);
    let value = run(
        &engine,
        &host,
        &request(&id),
        &digest::Options::default(),
        &d,
    )
    .unwrap();
    let symbols = value["symbols"].as_array().unwrap();
    let names: std::collections::BTreeSet<_> = symbols
        .iter()
        .map(|s| s["name"].as_str().unwrap())
        .collect();
    assert_eq!(
        names,
        std::collections::BTreeSet::from(["café_api", "second_api"])
    );
    let public = symbols.iter().find(|s| s["name"] == "café_api").unwrap();
    assert_eq!(public["path"], "café.rs");
    assert_eq!(public["signature"], "pub fn café_api() -> &'static str");
    let start = SOURCE.find("pub fn café_api").unwrap();
    let end = SOURCE.find("}\r\n").unwrap() + 1;
    assert_eq!(public["startByte"], start);
    assert_eq!(public["endByte"], end);
    assert_eq!(
        public["fingerprint"],
        yoke::model::hash(&SOURCE.as_bytes()[start..end])
    );
    assert_eq!(public["contentHash"], yoke::model::hash(SOURCE));
    assert!(
        !serde_json::to_string(&value)
            .unwrap()
            .contains("PRIVATE_IMPLEMENTATION_CANARY")
    );
    assert_eq!(value["snapshotId"], id);
    assert_eq!(value["providerVerified"], false);
    assert_eq!(value["sourceCommitVerified"], false);
    assert_eq!(value["complete"], false);
    assert_eq!(value["networkRequests"], 0);
    assert_eq!(value["modelCalls"], 0);
    assert_eq!(value["sourceWritten"], false);
    assert_eq!(engine.store.db.total_changes(), changes);
    assert_eq!(cas(&engine), blobs);
    assert_eq!(
        std::fs::read(root.path().join("café.rs")).unwrap(),
        SOURCE.as_bytes()
    );
}

#[test]
fn digest_uses_immutable_captured_bytes_after_current_source_changes() {
    let (root, engine, id, host, d) = fixture();
    let o = digest::Options::default();
    let before = run(&engine, &host, &request(&id), &o, &d).unwrap();
    std::fs::write(
        root.path().join("café.rs"),
        b"pub fn changed_after_capture() {}\n",
    )
    .unwrap();
    assert_eq!(run(&engine, &host, &request(&id), &o, &d).unwrap(), before);
    assert_eq!(
        std::fs::read(root.path().join("café.rs")).unwrap(),
        b"pub fn changed_after_capture() {}\n"
    );
}

#[test]
fn digest_symbol_limit_reports_omissions_and_complete_stdout_budget_precedes_mutations() {
    let (_root, engine, id, host, d) = fixture();
    let changes = engine.store.db.total_changes();
    let mut o = digest::Options {
        max_symbols: 1,
        ..Default::default()
    };
    let value = run(&engine, &host, &request(&id), &o, &d).unwrap();
    assert_eq!(value["symbols"].as_array().unwrap().len(), 1);
    assert_eq!(value["symbolCandidates"], 2);
    assert_eq!(value["symbolOmissions"], 1);
    assert_eq!(value["complete"], false);
    o.max_output_bytes = 1;
    marker(
        run(&engine, &host, &request(&id), &o, &d),
        "github_app_digest_capacity_exceeded",
    );
    assert_eq!(engine.store.db.total_changes(), changes);
}

#[test]
fn digest_refuses_grant_mismatch_and_caller_owned_transaction() {
    let (_root, engine, id, host, d) = fixture();
    let o = digest::Options::default();
    let mut bad = request(&id);
    bad.repository_id = 10;
    marker(
        run(&engine, &host, &bad, &o, &d),
        "github_app_digest_grant_denied",
    );
    engine.store.db.execute_batch("CREATE TEMP TABLE caller_value(n); INSERT INTO caller_value VALUES(7); BEGIN IMMEDIATE; UPDATE caller_value SET n=8").unwrap();
    marker(
        run(&engine, &host, &request(&id), &o, &d),
        "github_app_digest_caller_transaction_refused",
    );
    assert!(!engine.store.db.is_autocommit());
    assert_eq!(
        engine
            .store
            .db
            .query_row("SELECT n FROM caller_value", [], |row| row.get::<_, u64>(0))
            .unwrap(),
        8
    );
    engine.store.db.execute_batch("ROLLBACK").unwrap();
    assert_eq!(
        engine
            .store
            .db
            .query_row("SELECT n FROM caller_value", [], |row| row.get::<_, u64>(0))
            .unwrap(),
        7
    );
}

#[test]
fn public_constant_initializers_and_private_module_children_are_not_exported_as_source() {
    let source = "pub const PUBLIC_FLAG: &str = \"CONST_INITIALIZER_CANARY\";\r\npub static PUBLIC_STATIC: &str = \"STATIC_INITIALIZER_CANARY\";\r\nmod private { pub fn disguised_public() -> u32 { 1 } }\r\npub mod visible { pub fn visible_child() -> u32 { 2 } }\r\n";
    let (_root, engine, id, host, d) = fixture_with(source);
    let blobs = cas(&engine);
    let value = run(
        &engine,
        &host,
        &request(&id),
        &digest::Options::default(),
        &d,
    )
    .unwrap();
    let raw = serde_json::to_string(&value).unwrap();
    for canary in [
        "CONST_INITIALIZER_CANARY",
        "STATIC_INITIALIZER_CANARY",
        "disguised_public",
    ] {
        assert!(
            !raw.contains(canary),
            "private initializer or inaccessible API escaped"
        );
    }
    let symbols = value["symbols"].as_array().unwrap();
    assert!(symbols.iter().any(|s| s["name"] == "visible_child"));
    let constant = symbols.iter().find(|s| s["name"] == "PUBLIC_FLAG").unwrap();
    assert_eq!(constant["signature"], "pub const PUBLIC_FLAG: &str");
    let static_item = symbols
        .iter()
        .find(|s| s["name"] == "PUBLIC_STATIC")
        .unwrap();
    assert_eq!(static_item["signature"], "pub static PUBLIC_STATIC: &str");
    assert_eq!(cas(&engine), blobs);
}

#[test]
fn digest_replays_native_source_and_refuses_changed_graph_names_spans_or_producers() {
    for field in ["name", "endByte", "producer"] {
        let (_root, engine, id, host, d) = fixture();
        let o = digest::Options::default();
        assert!(run(&engine, &host, &request(&id), &o, &d).is_ok());
        let raw: String = engine
            .store
            .db
            .query_row("SELECT value FROM graphs WHERE id=?1", [&id], |row| {
                row.get(0)
            })
            .unwrap();
        let mut graph: Value = serde_json::from_str(&raw).unwrap();
        let node = graph["entities"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|s| s["name"] == "café_api")
            .unwrap();
        node[field] = match field {
            "name" => json!("forged_public_name"),
            "endByte" => json!(SOURCE.len()),
            _ => json!("yoke-tree-sitter-999"),
        };
        engine
            .store
            .db
            .execute(
                "UPDATE graphs SET value=?1 WHERE id=?2",
                rusqlite::params![serde_json::to_string(&graph).unwrap(), id],
            )
            .unwrap();
        let changes = engine.store.db.total_changes();
        let blobs = cas(&engine);
        marker(
            run(&engine, &host, &request(&id), &o, &d),
            "github_app_digest_source_unavailable",
        );
        assert_eq!(engine.store.db.total_changes(), changes);
        assert_eq!(cas(&engine), blobs);
    }
}

#[test]
fn digest_work_caps_apply_even_when_no_symbols_are_requested() {
    let (_root, engine, id, host, d) = fixture();
    let zero = digest::Options {
        max_symbols: 0,
        ..Default::default()
    };
    let value = run(&engine, &host, &request(&id), &zero, &d).unwrap();
    assert_eq!(value["symbolCandidates"], 2);
    assert_eq!(value["symbolOmissions"], 2);
    assert!(value["symbols"].as_array().unwrap().is_empty());
    for option in [
        digest::Options {
            max_files: 0,
            ..zero.clone()
        },
        digest::Options {
            max_entities: 1,
            ..zero.clone()
        },
        digest::Options {
            max_metadata_bytes: 1,
            ..zero.clone()
        },
        digest::Options {
            max_ast_nodes: 1,
            ..zero.clone()
        },
        digest::Options {
            max_source_bytes: 1,
            ..zero.clone()
        },
    ] {
        let changes = engine.store.db.total_changes();
        let blobs = cas(&engine);
        marker(
            run(&engine, &host, &request(&id), &option, &d),
            "github_app_digest_capacity_exceeded",
        );
        assert_eq!(engine.store.db.total_changes(), changes);
        assert_eq!(cas(&engine), blobs);
    }
}

#[test]
fn cancelled_digest_preserves_source_and_restores_the_store_deadline() {
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };
    let (_root, engine, id, host, d) = fixture();
    let o = digest::Options::default();
    let before = run(&engine, &host, &request(&id), &o, &d).unwrap();
    let changes = engine.store.db.total_changes();
    let blobs = cas(&engine);
    let cancel = Arc::new(AtomicBool::new(false));
    let result = Deadline::run_bounded(&d, Arc::clone(&cancel), || {
        let child = Deadline::new(30_000);
        cancel.store(true, Ordering::SeqCst);
        let result = run(&engine, &host, &request(&id), &o, &child);
        cancel.store(false, Ordering::SeqCst);
        result
    });
    marker(result, "cancelled");
    assert_eq!(engine.store.db.total_changes(), changes);
    assert_eq!(cas(&engine), blobs);
    assert_eq!(run(&engine, &host, &request(&id), &o, &d).unwrap(), before);
}

#[test]
fn public_reexports_remain_explicitly_unresolved_without_exposing_private_targets() {
    let source = "mod private { pub fn reexport_target() {} }\npub use private::reexport_target;\npub fn direct() {}\n";
    let (_root, engine, id, host, d) = fixture_with(source);
    let value = run(
        &engine,
        &host,
        &request(&id),
        &digest::Options::default(),
        &d,
    )
    .unwrap();
    assert!(
        value["symbols"]
            .as_array()
            .unwrap()
            .iter()
            .any(|s| s["name"] == "direct")
    );
    assert!(
        !value["symbols"]
            .as_array()
            .unwrap()
            .iter()
            .any(|s| s["name"] == "reexport_target")
    );
    assert_eq!(value["visibilityUnresolved"], 1);
    assert_eq!(value["compilerVerified"], false);
    assert_eq!(value["complete"], false);
}

#[test]
fn unsupported_language_exports_are_reported_as_unknown_instead_of_empty_complete() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("api.rs"), b"pub fn native_api() {}\n").unwrap();
    std::fs::write(
        root.path().join("api.ts"),
        b"export function web_api() { return 'TS_BODY_CANARY'; }\n",
    )
    .unwrap();
    let d = Deadline::new(30_000);
    let mut engine = Engine::open(root.path()).unwrap();
    let id = engine.index(&d).unwrap().0.id;
    let value = run(
        &engine,
        &host(),
        &request(&id),
        &digest::Options::default(),
        &d,
    )
    .unwrap();
    assert!(
        value["symbols"]
            .as_array()
            .unwrap()
            .iter()
            .any(|s| s["name"] == "native_api")
    );
    assert_eq!(value["unsupportedFiles"], 1);
    assert_eq!(value["complete"], false);
    assert!(
        !serde_json::to_string(&value)
            .unwrap()
            .contains("TS_BODY_CANARY")
    );
}

#[test]
fn digest_refuses_corrupt_captured_bytes_without_repairing_or_writing_them() {
    let (_root, engine, id, host, d) = fixture();
    let o = digest::Options::default();
    assert!(run(&engine, &host, &request(&id), &o, &d).is_ok());
    let blob = engine
        .store
        .dir
        .join("blobs")
        .join(yoke::model::hash(SOURCE));
    std::fs::write(&blob, vec![b'x'; SOURCE.len()]).unwrap();
    let blobs = cas(&engine);
    let changes = engine.store.db.total_changes();
    marker(
        run(&engine, &host, &request(&id), &o, &d),
        "github_app_digest_source_unavailable",
    );
    assert_eq!(engine.store.db.total_changes(), changes);
    assert_eq!(cas(&engine), blobs);
}

#[test]
fn executable_array_type_expressions_are_omitted_from_body_free_headers() {
    let source = "pub static ARRAY_API: [u8; \"HEADER_LITERAL_CANARY\".len()] = [0; 21];\npub fn array_api(_: [u8; \"PARAMETER_LITERAL_CANARY\".len()]) {}\npub fn safe_api() {}\n";
    let (_root, engine, id, host, d) = fixture_with(source);
    let value = run(
        &engine,
        &host,
        &request(&id),
        &digest::Options::default(),
        &d,
    )
    .unwrap();
    assert!(
        value["symbols"]
            .as_array()
            .unwrap()
            .iter()
            .any(|s| s["name"] == "safe_api")
    );
    let raw = serde_json::to_string(&value).unwrap();
    assert!(!raw.contains("HEADER_LITERAL_CANARY"));
    assert!(!raw.contains("PARAMETER_LITERAL_CANARY"));
    assert_eq!(value["visibilityUnresolved"], 2);
}

#[test]
fn private_ast_nesting_is_bounded_before_native_replay_even_with_zero_output_rows() {
    let source = format!(
        "{}fn hidden() {{}}{}",
        "mod nested {".repeat(20),
        "}".repeat(20)
    );
    let (_root, engine, id, host, d) = fixture_with(&source);
    let options = digest::Options {
        max_symbols: 0,
        max_ast_depth: 8,
        ..Default::default()
    };
    let changes = engine.store.db.total_changes();
    let blobs = cas(&engine);
    marker(
        run(&engine, &host, &request(&id), &options, &d),
        "github_app_digest_capacity_exceeded",
    );
    assert_eq!(engine.store.db.total_changes(), changes);
    assert_eq!(cas(&engine), blobs);
}

#[test]
fn native_replay_names_have_an_aggregate_budget_independent_of_emitted_symbols() {
    let (_root, engine, id, host, d) = fixture();
    let options = digest::Options {
        max_symbols: 0,
        max_replay_name_bytes: 1,
        ..Default::default()
    };
    marker(
        run(&engine, &host, &request(&id), &options, &d),
        "github_app_digest_capacity_exceeded",
    );
}

#[test]
fn malformed_saved_graph_cannot_echo_source_bearing_values_through_errors() {
    let (_root, engine, id, host, d) = fixture();
    let options = digest::Options::default();
    assert!(run(&engine, &host, &request(&id), &options, &d).is_ok());
    let raw: String = engine
        .store
        .db
        .query_row("SELECT value FROM graphs WHERE id=?1", [&id], |row| {
            row.get(0)
        })
        .unwrap();
    let mut graph: Value = serde_json::from_str(&raw).unwrap();
    graph["entities"][0]["startByte"] = json!("PRIVATE_ERROR_CANARY");
    engine
        .store
        .db
        .execute(
            "UPDATE graphs SET value=?1 WHERE id=?2",
            rusqlite::params![serde_json::to_string(&graph).unwrap(), id],
        )
        .unwrap();
    let error = run(&engine, &host, &request(&id), &options, &d).unwrap_err();
    let text = format!("{error:#}");
    assert!(text.contains("github_app_digest_source_unavailable"));
    assert!(
        !text.contains("PRIVATE_ERROR_CANARY"),
        "source-bearing decode detail escaped: {text}"
    );
}

#[test]
fn blocked_first_sql_read_obeys_deadline_and_restores_the_original_busy_timeout() {
    use std::time::{Duration, Instant};
    let (_root, engine, id, host, d) = fixture();
    let o = digest::Options::default();
    assert!(run(&engine, &host, &request(&id), &o, &d).is_ok());
    engine
        .store
        .db
        .execute_batch("PRAGMA wal_checkpoint(TRUNCATE); PRAGMA journal_mode=DELETE;")
        .unwrap();
    engine
        .store
        .db
        .busy_timeout(Duration::from_millis(731))
        .unwrap();
    let path = engine.store.dir.join("index.sqlite");
    let (ready_send, ready_receive) = std::sync::mpsc::channel();
    let blocker = std::thread::spawn(move || {
        let db = rusqlite::Connection::open(path).unwrap();
        db.execute_batch("BEGIN EXCLUSIVE").unwrap();
        ready_send.send(()).unwrap();
        std::thread::sleep(Duration::from_millis(500));
        db.execute_batch("ROLLBACK").unwrap();
    });
    ready_receive.recv_timeout(Duration::from_secs(5)).unwrap();
    let started = Instant::now();
    let result = run(&engine, &host, &request(&id), &o, &Deadline::new(25));
    let elapsed = started.elapsed();
    blocker.join().unwrap();
    marker(result, "deadline");
    assert!(
        elapsed < Duration::from_millis(250),
        "first read ignored deadline: {elapsed:?}"
    );
    let restored: u64 = engine
        .store
        .db
        .query_row("PRAGMA busy_timeout", [], |row| row.get(0))
        .unwrap();
    assert_eq!(restored, 731);
}

#[test]
fn header_literals_and_generic_defaults_are_unresolved_but_known_abi_headers_survive() {
    let source = "pub static ARRAY: [u8; \"RAW_ARRAY_CANARY\"] = [];\npub fn defaulted<const N: usize = \"DEFAULT_CANARY\">() {}\npub fn typed(_: Wrapper<\"TYPE_ARGUMENT_CANARY\">) {}\npub extern \"UNKNOWN_ABI_CANARY\" fn unknown_abi() {}\npub extern \"C\" fn known_abi() {}\n";
    let (_root, engine, id, host, d) = fixture_with(source);
    let value = run(
        &engine,
        &host,
        &request(&id),
        &digest::Options::default(),
        &d,
    )
    .unwrap();
    let raw = serde_json::to_string(&value).unwrap();
    for canary in [
        "RAW_ARRAY_CANARY",
        "DEFAULT_CANARY",
        "TYPE_ARGUMENT_CANARY",
        "UNKNOWN_ABI_CANARY",
    ] {
        assert!(!raw.contains(canary), "header expression escaped: {canary}");
    }
    let symbols = value["symbols"].as_array().unwrap();
    assert_eq!(symbols.len(), 1);
    assert_eq!(symbols[0]["signature"], "pub extern \"C\" fn known_abi()");
    assert_eq!(value["visibilityUnresolved"], 4);
}

#[test]
fn language_override_cannot_route_document_formats_through_rust_replay() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("safe.rs"), b"pub fn safe_api() {}\n").unwrap();
    std::fs::write(
        root.path().join("schema.sql"),
        b"pub fn overridden_api() {}\n",
    )
    .unwrap();
    let d = Deadline::new(30_000);
    let mut engine = Engine::open(root.path()).unwrap();
    engine
        .capture_options
        .language_overrides
        .insert("sql".into(), "rust".into());
    let id = engine.index(&d).unwrap().0.id;
    let value = run(
        &engine,
        &host(),
        &request(&id),
        &digest::Options::default(),
        &d,
    )
    .unwrap();
    let symbols = value["symbols"].as_array().unwrap();
    assert_eq!(symbols.len(), 1);
    assert_eq!(symbols[0]["name"], "safe_api");
    assert_eq!(value["unsupportedFiles"], 1);
    let sql = value["sources"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["path"] == "schema.sql")
        .unwrap();
    assert_eq!(sql["capturedBytesVerified"], true);
    assert_eq!(sql["exportPolicyAvailable"], false);
}

#[test]
fn repeated_call_receiver_text_is_bounded_before_native_replay() {
    let source = format!(
        "pub fn api() {{ {}{}; }}\n",
        "r".repeat(4096),
        ".step()".repeat(12)
    );
    let (_root, engine, id, host, d) = fixture_with(&source);
    assert!(
        run(
            &engine,
            &host,
            &request(&id),
            &digest::Options::default(),
            &d
        )
        .is_ok()
    );
    let options = digest::Options {
        max_symbols: 0,
        max_replay_name_bytes: 32_000,
        ..Default::default()
    };
    marker(
        run(&engine, &host, &request(&id), &options, &d),
        "github_app_digest_capacity_exceeded",
    );
}

#[test]
fn declaration_projection_does_not_require_unused_rationale_graph_rows() {
    let source = "// NOTE: PRIVATE_RATIONALE_CANARY\npub fn declaration() {}\n";
    let (_root, engine, id, host, d) = fixture_with(source);
    let o = digest::Options::default();
    let before = run(&engine, &host, &request(&id), &o, &d).unwrap();
    let raw: String = engine
        .store
        .db
        .query_row("SELECT value FROM graphs WHERE id=?1", [&id], |row| {
            row.get(0)
        })
        .unwrap();
    let mut graph: Value = serde_json::from_str(&raw).unwrap();
    let entities = graph["entities"].as_array_mut().unwrap();
    assert!(entities.iter().any(|entity| entity["kind"] == "rationale"));
    entities.retain(|entity| entity["kind"] != "rationale");
    engine
        .store
        .db
        .execute(
            "UPDATE graphs SET value=?1 WHERE id=?2",
            rusqlite::params![serde_json::to_string(&graph).unwrap(), id],
        )
        .unwrap();
    let changes = engine.store.db.total_changes();
    let blobs = cas(&engine);
    let after = run(&engine, &host, &request(&id), &o, &d).unwrap();
    assert_eq!(after, before);
    assert!(
        !serde_json::to_string(&after)
            .unwrap()
            .contains("PRIVATE_RATIONALE_CANARY")
    );
    assert_eq!(engine.store.db.total_changes(), changes);
    assert_eq!(cas(&engine), blobs);
}

#[test]
fn replay_preserves_native_method_identity_and_long_unicode_signature_finalization() {
    let arguments = (0..300)
        .map(|n| format!("argument_{n}: &'static str"))
        .collect::<Vec<_>>()
        .join(",\r\n");
    let boundary = format!("{}é", "a".repeat(4095 - "pub fn boundary(".len()));
    let source = format!(
        "pub struct Café;\r\npub trait Trait {{ fn method(&self); }}\r\nimpl Café {{ pub fn new() -> Self {{ Self }} }}\r\npub fn outer() {{ fn inner() {{}} }}\r\npub fn long_args({arguments}) {{}}\r\npub fn boundary({boundary}: u8) {{}}\r\npub const VALUE: u32 = 1;\r\n"
    );
    let (_root, engine, id, host, d) = fixture_with(&source);
    let value = run(
        &engine,
        &host,
        &request(&id),
        &digest::Options::default(),
        &d,
    )
    .unwrap();
    let symbols = value["symbols"].as_array().unwrap();
    let graph = engine.store.graph(&id).unwrap();
    let native_boundary = graph
        .entities
        .iter()
        .find(|entity| entity.name == "boundary")
        .unwrap();
    assert_eq!(native_boundary.signature.len(), 4095);
    assert!(symbols.iter().any(|symbol| symbol["name"] == "boundary"
        && symbol["signature"].as_str().unwrap().contains('é')));
    assert!(symbols.iter().any(|symbol| symbol["name"] == "outer"));
    assert!(symbols.iter().any(|symbol| symbol["name"] == "VALUE"));
    let long = symbols
        .iter()
        .find(|symbol| symbol["name"] == "long_args")
        .unwrap();
    assert!(long["signature"].as_str().unwrap().len() > 4096);
    assert!(!symbols.iter().any(|symbol| {
        ["inner", "method", "new"]
            .iter()
            .any(|name| symbol["name"] == *name)
    }));
}

#[test]
fn disk_preview_refuses_shadow_inheritance_and_overlay_selector_records() {
    let (root, mut engine, id, host, d) = fixture();
    let o = digest::Options::default();
    assert!(run(&engine, &host, &request(&id), &o, &d).is_ok());
    let selected = engine.store.snapshot(&id).unwrap();
    std::fs::write(
        root.path().join("origin_only.rs"),
        "pub fn origin_only() {}\n",
    )
    .unwrap();
    let origin = engine.index(&d).unwrap().0;
    assert_ne!(origin.id, id);
    yoke::capture::inherit_options(&engine.store, &origin, &selected).unwrap();
    assert_eq!(
        yoke::capture::analyzer_epoch_for_snapshot_with_deadline(&engine.store, &selected, &d)
            .unwrap()
            .producer(),
        "yoke-tree-sitter-8"
    );
    for kind in [
        yoke::capture::DERIVED_OPTIONS_RECORD_KIND,
        "overlay-snapshot",
    ] {
        if kind == "overlay-snapshot" {
            engine
                .store
                .db
                .execute(
                    "DELETE FROM records WHERE kind=?1 AND id=?2",
                    rusqlite::params![yoke::capture::DERIVED_OPTIONS_RECORD_KIND, id],
                )
                .unwrap();
            engine
                .store
                .db
                .execute(
                    "INSERT INTO records(kind,id,value) VALUES(?1,?2,'{}')",
                    rusqlite::params![kind, id],
                )
                .unwrap();
        }
        let changes = engine.store.db.total_changes();
        let blobs = cas(&engine);
        marker(
            run(&engine, &host, &request(&id), &o, &d),
            "github_app_digest_source_unavailable",
        );
        assert_eq!(engine.store.db.total_changes(), changes);
        assert_eq!(cas(&engine), blobs);
    }
}

#[test]
fn configured_disk_proof_bytes_have_an_independent_budget() {
    let (_root, mut engine, _, host, d) = fixture();
    engine
        .capture_options
        .language_overrides
        .insert("rs".into(), "rust".into());
    let id = engine.index(&d).unwrap().0.id;
    let raw: String = engine
        .store
        .db
        .query_row(
            "SELECT value FROM records WHERE kind=?1 AND id=?2",
            rusqlite::params![yoke::capture::OPTIONS_RECORD_KIND, id],
            |row| row.get(0),
        )
        .unwrap();
    let o = digest::Options {
        max_lineage_metadata_bytes: raw.len(),
        ..Default::default()
    };
    assert!(run(&engine, &host, &request(&id), &o, &d).is_ok());
    let changes = engine.store.db.total_changes();
    let blobs = cas(&engine);
    let o = digest::Options {
        max_lineage_metadata_bytes: raw.len() - 1,
        ..o
    };
    marker(
        run(&engine, &host, &request(&id), &o, &d),
        "github_app_digest_capacity_exceeded",
    );
    assert_eq!(engine.store.db.total_changes(), changes);
    assert_eq!(cas(&engine), blobs);
}
