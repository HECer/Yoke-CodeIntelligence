use serde_json::json;
use std::fs;
use tempfile::TempDir;
#[test]
fn clipped_single_line_context_keeps_the_match_and_exact_unicode_bytes() {
    let root = TempDir::new().unwrap();
    let text = format!("{}needle{}", "🌍".repeat(300000), " tail".repeat(1000));
    fs::write(root.path().join("long.txt"), &text).unwrap();
    let mut engine = Engine::open(root.path()).unwrap();
    let result = engine
        .invoke(
            "ci_find",
            json!({"kind":"text","query":"needle","timeoutMs":20000}),
        )
        .unwrap();
    assert_eq!(result["status"], "ok", "{result}");
    let item = &result["data"]["items"][0];
    let context = item["context"].as_str().unwrap();
    assert!(context.contains("needle"), "Clipped context lost the match");
    assert!(context.len() <= 16000);
    let begin = item["contextStartByte"].as_u64().unwrap() as usize;
    let end = item["contextEndByte"].as_u64().unwrap() as usize;
    assert_eq!(context, &text[begin..end]);
    assert!(begin <= 1200000 && end >= 1200006);
    assert_eq!(item["contextTruncated"], true);
}
#[test]
fn text_search_streams_the_captured_corpus_beyond_the_symbol_body_cache_budget() {
    let root = TempDir::new().unwrap();
    let body = "a".repeat(3_999_000);
    for index in 0..9 {
        let text = if index == 8 {
            format!("{body}\nunique_needle\n")
        } else {
            body.clone()
        };
        fs::write(root.path().join(format!("file{index:02}.txt")), text).unwrap();
    }
    let mut engine = Engine::open(root.path()).unwrap();
    let result = engine
        .invoke(
            "ci_find",
            json!({"kind":"text","query":"unique_needle","timeoutMs":120000}),
        )
        .unwrap();
    assert_eq!(result["status"], "ok", "{result}");
    assert_eq!(result["data"]["items"][0]["path"], "file08.txt", "{result}");
    assert_eq!(result["data"]["omittedPaths"], json!([]));
}
#[test]
fn text_search_pages_past_the_first_ten_thousand_occurrences() {
    let root = TempDir::new().unwrap();
    fs::write(root.path().join("many.txt"), "hit\n".repeat(11010)).unwrap();
    let mut engine = Engine::open(root.path()).unwrap();
    let result = engine
        .invoke(
            "ci_find",
            json!({"kind":"text","query":"hit","offset":10000,"limit":5,"timeoutMs":20000}),
        )
        .unwrap();
    assert_eq!(result["status"], "ok", "{result}");
    let items = result["data"]["items"].as_array().unwrap();
    assert_eq!(items.len(), 5, "{result}");
    assert_eq!(items[0]["startByte"], 40000);
    assert_eq!(items[4]["startByte"], 40016);
    assert_eq!(result["data"]["truncated"], true);
}
#[test]
fn body_retrieval_in_file_mode_never_returns_symbol_rows() {
    let (_dir, mut engine) = project();
    let result = engine
        .invoke(
            "ci_find",
            json!({"kind":"file","query":"payment","searchBody":true}),
        )
        .unwrap();
    assert_eq!(result["status"], "ok", "{result}");
    let items = result["data"]["items"].as_array().unwrap();
    assert!(!items.is_empty());
    assert!(items.iter().all(|item| item["kind"] == "file"), "{result}");
}
use yoke::Engine;

fn project() -> (TempDir, Engine) {
    let dir = TempDir::new().unwrap();
    fs::create_dir_all(dir.path().join("src/foo")).unwrap();
    fs::create_dir_all(dir.path().join("src/foobar")).unwrap();
    fs::write(dir.path().join("src/foo/service.ts"), "class Service {\n  run(value: string) {\n    const payment = value;\n    return payment;\n  }\n  helper() { return this.run('🌍'); }\n}\n").unwrap();
    fs::write(
        dir.path().join("src/foobar/other.ts"),
        "function unrelated() {}\n",
    )
    .unwrap();
    let engine = Engine::open(dir.path()).unwrap();
    (dir, engine)
}

#[test]
fn exact_name_path_symbol_query_has_source_bound_body_and_depth() {
    let (_dir, mut engine) = project();
    let result = engine
        .invoke(
            "ci_find",
            json!({"kind":"symbol", "namePath":"Service/run", "includeBody":true, "depth":1}),
        )
        .unwrap();
    let items = result["data"]["items"].as_array().unwrap();
    assert_eq!(items.len(), 1, "{result}");
    assert_eq!(items[0]["name"], "run");
    assert!(items[0]["body"].as_str().unwrap().contains("const payment"));
    assert!(items[0]["contentHash"].as_str().unwrap().len() == 64);
    assert_eq!(items[0]["bodyTruncated"], false);
}

#[test]
fn relative_directory_scope_is_segment_aware_and_globs_are_explicit() {
    let (_dir, mut engine) = project();
    let result = engine.invoke("ci_find", json!({"kind":"symbol", "relativePath":"src/foo", "namePath":"*", "substringMatching":true})).unwrap();
    assert!(
        result["data"]["items"]
            .as_array()
            .unwrap()
            .iter()
            .all(|item| item["path"] == "src/foo/service.ts")
    );
    let result = engine
        .invoke("ci_find", json!({"kind":"file", "pathGlob":"src/foo/*.ts"}))
        .unwrap();
    assert_eq!(result["data"]["items"].as_array().unwrap().len(), 1);
}

#[test]
fn task_words_inside_multiline_bodies_are_searchable_without_becoming_fact() {
    let (_dir, mut engine) = project();
    let result = engine
        .invoke("ci_find", json!({"kind":"symbol", "query":"payment"}))
        .unwrap();
    assert!(
        result["data"]["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["name"] == "run"),
        "{result}"
    );
}

#[test]
fn multiline_case_insensitive_search_exposes_exact_occurrences_and_symbol_owners() {
    let (_dir, mut engine) = project();
    let result = engine.invoke("ci_find", json!({"kind":"text", "query":"CONST payment = value;\\n\\s+return payment", "regex":true, "ignoreCase":true, "multiline":true, "groupBySymbol":true, "beforeContextLines":1, "afterContextLines":1})).unwrap();
    let item = &result["data"]["items"][0];
    assert_eq!(item["path"], "src/foo/service.ts");
    assert_eq!(item["symbol"]["name"], "run");
    assert!(item["occurrenceId"].as_str().unwrap().len() == 64);
    let snapshot = engine
        .store
        .snapshot(result["snapshotId"].as_str().unwrap())
        .unwrap();
    let (text, _) =
        yoke::query::source(&engine.store, &snapshot, item["path"].as_str().unwrap()).unwrap();
    let start = item["startByte"].as_u64().unwrap() as usize;
    let end = item["endByte"].as_u64().unwrap() as usize;
    assert_eq!(
        &text[start..end],
        "const payment = value;\n    return payment"
    );
    assert!(item["context"].as_str().unwrap().contains("run(value"));
}

#[test]
fn selected_occurrences_and_count_guards_prepare_an_atomic_reviewable_bulk_plan() {
    let (_dir, mut engine) = project();
    let result = engine.invoke("ci_change_prepare", json!({"bulkReplace":{"pattern":"payment", "replacement":"invoice", "literal":true, "expectedCount":2, "relativePath":"src/foo", "dryRun":true}})).unwrap();
    assert_eq!(result["data"]["matchCount"], 2);
    assert_eq!(result["data"]["dryRun"], true);
    assert!(
        result["data"]["occurrences"]
            .as_array()
            .unwrap()
            .iter()
            .all(|item| item["occurrenceId"].as_str().unwrap().len() == 64)
    );
    assert!(
        fs::read_to_string(engine.root.join("src/foo/service.ts"))
            .unwrap()
            .contains("payment")
    );
    assert_eq!(engine.invoke("ci_change_prepare", json!({"bulkReplace":{"pattern":"payment", "replacement":"invoice", "literal":true, "expectedCount":1, "relativePath":"src/foo"}})).unwrap()["status"], "error");
}

#[test]
fn bulk_edit_selection_is_atomic_and_conflicting_modes_are_rejected() {
    let (dir, mut engine) = project();
    let original = fs::read(dir.path().join("src/foo/service.ts")).unwrap();
    let dry = engine.invoke("ci_change_prepare", json!({"bulkReplace":{"pattern":"payment","replacement":"invoice","literal":true,"dryRun":true}})).unwrap();
    assert_eq!(dry["status"], "ok", "{dry}");
    let selected = dry["data"]["occurrences"][0]["occurrenceId"].clone();
    let plan = engine.invoke("ci_change_prepare", json!({"snapshotId":dry["snapshotId"],"bulkReplace":{"pattern":"payment","replacement":"invoice","literal":true,"expectedCount":2,"occurrenceIds":[selected]}})).unwrap();
    assert_eq!(plan["status"], "ok", "{plan}");
    assert_eq!(plan["data"]["selectedCount"], 1);
    assert_eq!(plan["data"]["sourceWritten"], false);
    assert_eq!(
        fs::read(dir.path().join("src/foo/service.ts")).unwrap(),
        original
    );
    let conflict = engine.invoke("ci_change_prepare",json!({"edits":[{"kind":"create","path":"unintended.ts","text":""}],"bulkReplace":{"pattern":"payment","replacement":"invoice","literal":true,"dryRun":true}})).unwrap();
    assert_eq!(conflict["status"], "error", "{conflict}");
    fs::write(dir.path().join("src/foo/service.ts"), "// changed\n").unwrap();
    let stale = engine.invoke("ci_change_prepare", json!({"snapshotId":dry["snapshotId"],"bulkReplace":{"pattern":"payment","replacement":"invoice","literal":true,"expectedCount":2}})).unwrap();
    assert_eq!(stale["status"], "error", "{stale}");
    assert!(!dir.path().join("unintended.ts").exists());
}

#[test]
fn advertised_native_protocol_reaches_advanced_semantic_operations() {
    use std::path::Path;
    let dir = TempDir::new().unwrap();
    let source = "// 🌍\nfn helper() {}\nfn main() { helper(); }\n";
    fs::write(dir.path().join("main.rs"), source).unwrap();
    let mut engine = Engine::open(dir.path()).unwrap();
    engine
        .register_configured_server(yoke::lsp::ConfiguredServer {
            server: yoke::lsp::Server {
                command: "node".into(),
                args: vec![
                    Path::new(env!("CARGO_MANIFEST_DIR"))
                        .join("tests/fixtures/advanced-lsp.mjs")
                        .display()
                        .to_string(),
                    String::new(),
                ],
                language: "rust".into(),
                trust: true,
            },
            configuration: yoke::lsp::HostConfiguration {
                passive_profile: Some(yoke::lsp::PassiveProfile::ContractFixture),
                ..Default::default()
            },
        })
        .unwrap();
    let result=engine.invoke("ci_find",json!({"languageServer":{"operation":"incomingCalls","path":"main.rs","offset":44,"options":{"depth":1,"maxItems":10}},"timeoutMs":20000})).unwrap();
    assert_eq!(result["status"], "ok", "{result}");
    assert_eq!(
        result["data"]["items"][0]["fromRanges"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    assert_eq!(result["metrics"]["languageServerAttempts"], 1);
    assert_eq!(
        fs::read_to_string(dir.path().join("main.rs")).unwrap(),
        source
    );
    for args in [
        json!({"languageServer":{"operation":"incomingCalls","path":"main.rs","offset":44},"namePath":"helper"}),
        json!({"languageServer":{"operation":"incomingCalls","path":"main.rs","offset":44,"options":{"trust":true}}}),
        json!({"kind":"semantic","model":"x","vector":[1.0],"includeBody":true}),
        json!({"kind":"text","namePath":"helper"}),
    ] {
        assert!(yoke::validate("ci_find", &args).is_err(), "{args}");
    }
    let tools = yoke::protocol::tools();
    let find = tools
        .as_array()
        .unwrap()
        .iter()
        .find(|tool| tool["name"] == "ci_find")
        .unwrap();
    assert!(find["inputSchema"]["properties"].get("namePath").is_some());
    assert!(
        find["inputSchema"]["properties"]["languageServer"]["properties"]["options"].is_object()
    );
}

#[test]
fn public_engine_semantics_preserve_host_configuration_callbacks() {
    let dir = TempDir::new().unwrap();
    fs::write(
        dir.path().join("main.rs"),
        "// 🌍\nfn helper() {}\nfn main() { helper(); }\n",
    )
    .unwrap();
    let mut engine = Engine::open(dir.path()).unwrap();
    engine
        .register_configured_server(yoke::lsp::ConfiguredServer {
            server: yoke::lsp::Server {
                command: "node".into(),
                args: vec![
                    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                        .join("tests/fixtures/advanced-lsp.mjs")
                        .display()
                        .to_string(),
                    "host-settings".into(),
                ],
                language: "rust".into(),
                trust: true,
            },
            configuration: yoke::lsp::HostConfiguration {
                passive_profile: Some(yoke::lsp::PassiveProfile::ContractFixture),
                settings: json!({"feature":{"enabled":true}}),
                ..Default::default()
            },
        })
        .unwrap();
    let result=engine.invoke("ci_find",json!({"languageServer":{"operation":"typeDefinition","path":"main.rs","offset":44},"timeoutMs":1000})).unwrap();
    assert_eq!(result["status"], "ok", "{result}");
    assert_eq!(result["data"]["locations"][0]["text"], "helper");
}
