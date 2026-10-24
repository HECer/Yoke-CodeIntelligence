use serde_json::{Value, json};
use std::{fs, path::Path};
use tempfile::TempDir;
use yoke::{Engine, capture::Deadline, lsp::Server, semantic_refactor};
fn fixture(source: &str, mode: &str) -> (TempDir, Engine) {
    let root = TempDir::new().unwrap();
    fs::write(root.path().join("main.rs"), source).unwrap();
    let mut engine = Engine::open(root.path()).unwrap();
    engine
        .register_configured_server(yoke::lsp::ConfiguredServer {
            server: Server {
                command: "node".into(),
                args: vec![
                    Path::new(env!("CARGO_MANIFEST_DIR"))
                        .join("tests/fixtures/refactor-lsp.mjs")
                        .display()
                        .to_string(),
                    mode.into(),
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
    (root, engine)
}
fn args() -> Value {
    json!({"namePath":"helper","relativePath":"main.rs"})
}
#[test]
fn explicit_force_delete_keeps_verified_usage_context_and_still_refuses_foreign_or_stale_sources() {
    let source = "fn helper() {}\nfn caller(){helper();}\n";
    let (root, mut engine) = fixture(source, "provider-use");
    let mut request = args();
    request["deleteEvenIfUsed"] = json!(true);
    let value =
        semantic_refactor::invoke(&mut engine, "safeDelete", &request, &Deadline::new(20000))
            .unwrap();
    assert!(value["plan"]["id"].is_string(), "{value}");
    assert_eq!(value["forced"], true);
    assert_eq!(value["usageCheck"]["capturedReferenceCount"], 1);
    assert_eq!(value["usageCheck"]["providerReferenceCount"], 1);
    assert_eq!(
        value["usageCheck"]["sourceBindings"]["main.rs"],
        yoke::model::hash(source)
    );
    assert_eq!(
        fs::read_to_string(root.path().join("main.rs")).unwrap(),
        source
    );
    let (foreign, mut engine) = fixture(source, "foreign-reference");
    assert!(
        semantic_refactor::invoke(&mut engine, "safeDelete", &request, &Deadline::new(20000))
            .is_err()
    );
    assert_eq!(
        engine
            .store
            .db
            .query_row(
                "SELECT count(*) FROM records WHERE kind='plan'",
                [],
                |row| row.get::<_, i64>(0)
            )
            .unwrap(),
        0
    );
    let snapshot = engine.index(&Deadline::new(20000)).unwrap().0;
    fs::write(foreign.path().join("main.rs"), "fn helper(){changed();}\n").unwrap();
    request["snapshotId"] = json!(snapshot.id);
    assert!(
        semantic_refactor::invoke(&mut engine, "safeDelete", &request, &Deadline::new(20000))
            .is_err()
    );
    assert_eq!(
        engine
            .store
            .db
            .query_row(
                "SELECT count(*) FROM records WHERE kind='plan'",
                [],
                |row| row.get::<_, i64>(0)
            )
            .unwrap(),
        0
    );
}
#[test]
fn unused_semantic_delete_requires_registered_source_proof_and_only_prepares_a_guarded_plan() {
    let source = "fn helper() {}\n";
    let (root, mut engine) = fixture(source, "");
    let value =
        semantic_refactor::invoke(&mut engine, "safeDelete", &args(), &Deadline::new(20000))
            .unwrap();
    assert!(value["plan"]["id"].is_string(), "{value}");
    assert_eq!(value["usageCheck"]["providerReferenceCount"], 0);
    assert_eq!(
        fs::read_to_string(root.path().join("main.rs")).unwrap(),
        source
    );
}

#[test]
fn indexed_symbol_refactoring_selects_the_same_source_order_overload_as_the_facade() {
    let source = "fn helper(value: i32) {}\nfn helper(value: String) {}\n";
    let (root, mut engine) = fixture(source, "");
    let request = json!({"namePath":"helper[1]","relativePath":"main.rs"});
    let value =
        semantic_refactor::invoke(&mut engine, "safeDelete", &request, &Deadline::new(20000))
            .unwrap();
    let diff = value["plan"]["diff"].as_str().unwrap();
    assert!(diff.contains("-fn helper(value: String)"), "{value}");
    assert!(!diff.contains("-fn helper(value: i32)"), "{value}");
    assert!(value["plan"]["id"].is_string(), "{value}");
    assert_eq!(
        fs::read_to_string(root.path().join("main.rs")).unwrap(),
        source
    );
}
#[test]
fn semantic_delete_refuses_actual_candidate_unresolved_foreign_and_stale_uses_without_publishing_plans()
 {
    for (source, mode) in [
        ("fn helper() {}\nfn caller(){helper();}\n", ""),
        ("fn helper() {}\nfn caller(){unknown.helper();}\n", ""),
        ("fn helper() {}\n", "provider-use"),
        ("fn helper() {}\n", "foreign-reference"),
    ] {
        let (_root, mut engine) = fixture(source, mode);
        let result =
            semantic_refactor::invoke(&mut engine, "safeDelete", &args(), &Deadline::new(20000));
        assert!(
            result.is_err(),
            "unsafe deletion accepted {source:?} {mode}: {result:?}"
        );
        let count: i64 = engine
            .store
            .db
            .query_row(
                "SELECT count(*) FROM records WHERE kind='plan'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 0);
    }
    let (root, mut engine) = fixture("fn helper() {}\n", "");
    let snapshot = engine.index(&Deadline::new(20000)).unwrap().0;
    fs::write(root.path().join("main.rs"), "fn helper(){ changed(); }\n").unwrap();
    let mut request = args();
    request["snapshotId"] = json!(snapshot.id);
    assert!(
        semantic_refactor::invoke(&mut engine, "safeDelete", &request, &Deadline::new(20000))
            .is_err()
    );
}
#[test]
fn inline_refactoring_accepts_only_the_requested_kind_and_explicit_workspace_edits() {
    let (root, mut engine) = fixture("fn helper() {}\n", "");
    let value =
        semantic_refactor::invoke(&mut engine, "inline", &args(), &Deadline::new(20000)).unwrap();
    assert_eq!(value["actions"][0]["kind"], "refactor.inline");
    assert_eq!(value["actions"][0]["usable"], true);
    assert_eq!(
        fs::read_to_string(root.path().join("main.rs")).unwrap(),
        "fn helper() {}\n"
    );
    for mode in ["wrong-action-kind", "command-only"] {
        let (_root, mut engine) = fixture("fn helper() {}\n", mode);
        let result =
            semantic_refactor::invoke(&mut engine, "inline", &args(), &Deadline::new(20000));
        assert!(
            result.is_err(),
            "unusable inline accepted {mode}: {result:?}"
        );
        let count: i64 = engine
            .store
            .db
            .query_row(
                "SELECT count(*) FROM records WHERE kind='plan'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(
            count, 0,
            "unrequested/unusable provider action published a hidden plan"
        );
    }
}

#[test]
fn file_move_negotiates_will_rename_and_combines_pre_rename_edits_into_one_guarded_plan() {
    let (root, mut engine) = fixture("fn helper() {}\n", "");
    let value = semantic_refactor::invoke(
        &mut engine,
        "move",
        &json!({"relativePath":"main.rs","targetRelativePath":"moved.rs"}),
        &Deadline::new(20000),
    )
    .unwrap();
    let files = value["plan"]["files"].as_array().unwrap();
    assert_eq!(files.len(), 2, "{value}");
    assert!(
        files
            .iter()
            .any(|file| file["path"] == "main.rs" && file["afterHash"].is_null())
    );
    let created = files
        .iter()
        .find(|file| file["path"] == "moved.rs")
        .unwrap();
    let bytes = engine
        .store
        .read_blob(created["afterHash"].as_str().unwrap())
        .unwrap();
    assert_eq!(
        String::from_utf8(bytes).unwrap(),
        "// rename prepared\nfn helper() {}\n"
    );
    assert_eq!(
        fs::read_to_string(root.path().join("main.rs")).unwrap(),
        "fn helper() {}\n"
    );
    assert!(!root.path().join("moved.rs").exists());
}

#[test]
fn inspection_equivalent_uses_actual_registered_diagnostics_and_declared_kind_registry() {
    let (_root, mut engine) = fixture("fn helper() {}\n", "");
    let deadline = Deadline::new(20000);
    let value = semantic_refactor::invoke(
        &mut engine,
        "inspections",
        &json!({"relativePath":"main.rs","minSeverity":2,"inspectionNames":["unused"]}),
        &deadline,
    )
    .unwrap();
    assert_eq!(value["items"].as_array().unwrap().len(), 1, "{value}");
    assert_eq!(value["items"][0]["code"], "unused");
    assert_eq!(value["provider"], "host_selected_lsp");
    let registry = semantic_refactor::invoke(
        &mut engine,
        "listInspections",
        &json!({"language":"rust","groupContains":"refactor"}),
        &deadline,
    )
    .unwrap();
    let kinds = registry["items"].as_array().unwrap();
    assert!(
        kinds.iter().any(|kind| kind["kind"] == "refactor.inline"),
        "{registry}"
    );
    assert!(
        kinds
            .iter()
            .all(|kind| kind["kind"].as_str().unwrap().contains("refactor"))
    );
    assert_eq!(
        registry["registrySemantics"],
        "standard_lsp_declared_code_action_kinds_not_proprietary_inspection_catalog"
    );
}
