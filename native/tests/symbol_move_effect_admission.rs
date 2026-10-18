//! Isolated effect failures must reach the owned response and the effect boundary.
use serde_json::{Value, json};
use std::{fs, path::Path};
use yoke::{
    Engine,
    analysis::AnalyzerEpoch,
    capture::Deadline,
    lsp,
    project_session::{HostProfile, Session},
};

const SOURCE: &str = "// source comment\npub fn helper() -> u8 { 7 }\n";
const DESTINATION: &str = "pub mod bucket {\npub fn sentinel() {}\n}\n";
const NESTED: &str = "pub mod bucket {\npub mod nested {}\npub fn sentinel() {}\n}\n";

struct Fixture {
    session: Session,
    root: tempfile::TempDir,
    log: tempfile::TempDir,
}
fn fixture(epoch: AnalyzerEpoch, scenario: &str, source: &str) -> Fixture {
    let root = tempfile::tempdir().unwrap();
    let log = tempfile::tempdir().unwrap();
    fs::create_dir(root.path().join("src")).unwrap();
    fs::write(root.path().join("src/source.rs"), source).unwrap();
    fs::write(
        root.path().join("src/destination.rs"),
        if scenario == "nested_inner" {
            NESTED
        } else {
            DESTINATION
        },
    )
    .unwrap();
    let mut engine = Engine::open(root.path()).unwrap();
    engine.capture_epoch = epoch;
    engine
        .register_configured_server(lsp::ConfiguredServer {
            server: lsp::Server {
                command: "node".into(),
                args: vec![
                    Path::new(env!("CARGO_MANIFEST_DIR"))
                        .join("tests/fixtures/own-refactor-intent-lsp.mjs")
                        .display()
                        .to_string(),
                    log.path().join("protocol.log").display().to_string(),
                    scenario.into(),
                ],
                language: "rust".into(),
                trust: true,
            },
            configuration: serde_json::from_value(
                json!({"passiveProfile":"contract_fixture","refactorIntentProfile":"own_v1"}),
            )
            .unwrap(),
        })
        .unwrap();
    let session = Session::open(
        engine,
        HostProfile::default(),
        Some("symbol-move-effect-admission"),
        &Deadline::new(30000),
    )
    .unwrap();
    Fixture { session, root, log }
}
fn invoke(f: &mut Fixture) -> anyhow::Result<Value> {
    yoke::agent_repl::invoke_facade(
        &mut f.session,
        "jb.move",
        json!({"relative_path":"src/source.rs","name_path":"helper","target_relative_path":"src/destination.rs","target_parent_name_path":"bucket"}),
        None,
        &Deadline::new(30000),
    )
}
fn requests(f: &Fixture) -> Vec<Value> {
    let path = f.log.path().join("protocol.log");
    if !path.exists() {
        return vec![];
    }
    fs::read_to_string(path)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}
fn change_objects(f: &Fixture) -> (Vec<(String, String, String)>, usize) {
    let db = &f.session.engine.store.db;
    let records = db.prepare("SELECT kind,id,value FROM records WHERE kind IN ('plan','approval','receipt') ORDER BY kind,id").unwrap().query_map([], |row| Ok((row.get(0)?,row.get(1)?,row.get(2)?))).unwrap().collect::<rusqlite::Result<Vec<_>>>().unwrap();
    let snapshots = db
        .query_row(
            "SELECT COUNT(*) FROM snapshots WHERE json_extract(value,'$.kind')='derived'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    (records, snapshots)
}
fn reject_effect(scenario: &str, reason: &str) {
    let mut f = fixture(AnalyzerEpoch::V9, scenario, SOURCE);
    let before = change_objects(&f);
    let error = invoke(&mut f).unwrap_err().to_string();
    let transcript = requests(&f);
    assert_eq!(
        transcript
            .iter()
            .filter(|v| v["method"] == "yoke/refactorIntent")
            .count(),
        1,
        "Effect case must reach exactly one actual owned request: {error}"
    );
    assert!(
        !transcript
            .iter()
            .any(|v| v["method"] == "workspace/executeCommand")
    );
    assert!(
        error.contains(&format!("Move effect rejected: {reason}")),
        "Scenario {scenario} stopped before its causal effect rule: {error}"
    );
    assert_eq!(
        change_objects(&f),
        before,
        "Effect refusal published change objects"
    );
    assert_eq!(
        fs::read_to_string(f.root.path().join("src/source.rs")).unwrap(),
        SOURCE
    );
    assert_eq!(
        fs::read_to_string(f.root.path().join("src/destination.rs")).unwrap(),
        if scenario == "nested_inner" {
            NESTED
        } else {
            DESTINATION
        }
    );
}
macro_rules! rejected_effects {
    ($($name:ident => ($scenario:literal, $reason:literal)),* $(,)?) => { $(
        #[test] fn $name() { reject_effect($scenario, $reason); }
    )* };
}
rejected_effects! {
    unrelated_comment_is_not_a_source_cut => ("noop", "exact source cut required"),
    copy_only_is_not_a_move => ("copy_only", "exact source cut required"),
    deletion_alone_has_no_preserved_insertion => ("delete_only", "exact declaration insertion required"),
    changed_body_is_not_a_preserved_declaration => ("body_change", "exact declaration insertion required"),
    outside_parent_insertion_is_not_requested_placement => ("wrong_parent", "insertion destination differs"),
    broad_source_rewrite_is_not_one_isolated_cut => ("whole_source", "exact source cut required"),
    broad_destination_rewrite_is_not_one_pure_insertion => ("whole_target", "exact declaration insertion required"),
    split_deletion_is_not_one_original_cut => ("split_cut", "exact source cut required"),
    repeated_null_version_document_is_not_one_original_input => ("repeated_original", "only one original document per path"),
    applied_resource_operation_is_refused => ("resource", "resource operations are unsupported"),
    unapplied_resource_noop_is_also_refused => ("resource_noop", "resource operations are unsupported"),
    duplicate_identical_insertions_are_refused => ("duplicate_copy", "exact declaration insertion required"),
    same_named_parent_with_changed_header_is_refused => ("replace_parent", "destination parent boundary changed"),
    nested_child_is_not_the_requested_direct_parent => ("nested_inner", "insertion destination differs"),
}

#[test]
fn exact_original_cut_and_inline_insertion_prepare_one_real_plan_for_each_epoch() {
    for epoch in [AnalyzerEpoch::V8, AnalyzerEpoch::V9] {
        let mut f = fixture(epoch, "valid", SOURCE);
        let result = invoke(&mut f).unwrap();
        assert_eq!(
            requests(&f)
                .iter()
                .filter(|v| v["method"] == "yoke/refactorIntent")
                .count(),
            1
        );
        assert_eq!(change_objects(&f).0.len(), 1);
        assert_eq!(change_objects(&f).1, 1);
        let snapshot = f
            .session
            .engine
            .store
            .snapshot(result["plan"]["resultSnapshotId"].as_str().unwrap())
            .unwrap();
        assert_eq!(
            yoke::query::source(&f.session.engine.store, &snapshot, "src/source.rs")
                .unwrap()
                .0,
            "// source comment\n\n"
        );
        assert_eq!(
            yoke::query::source(&f.session.engine.store, &snapshot, "src/destination.rs")
                .unwrap()
                .0,
            "pub mod bucket {\npub fn sentinel() {}\n\npub fn helper() -> u8 { 7 }\n}\n"
        );
        assert_eq!(
            yoke::capture::analyzer_epoch_for_snapshot(&f.session.engine.store, &snapshot).unwrap(),
            epoch
        );
        assert_eq!(result["complete"], false);
        assert_eq!(result["commandsExecuted"], false);
        assert_eq!(
            fs::read_to_string(f.root.path().join("src/source.rs")).unwrap(),
            SOURCE
        );
        assert_eq!(
            fs::read_to_string(f.root.path().join("src/destination.rs")).unwrap(),
            DESTINATION
        );
    }
}

#[test]
fn block_docs_multiline_attributes_and_intervening_trivia_require_attachment_capability() {
    for source in [
        "/** selected documentation */\npub fn helper() -> u8 { 7 }\n",
        "#[allow(\n dead_code\n)]\npub fn helper() -> u8 { 7 }\n",
        "/// selected documentation\n\n// unrelated trivia\n\npub fn helper() -> u8 { 7 }\n",
        "/** selected docs */\n/* unrelated comment */\n\npub fn helper() -> u8 { 7 }\n",
    ] {
        let mut f = fixture(AnalyzerEpoch::V8, "missing_attachment_capability", source);
        let before = change_objects(&f);
        let error = invoke(&mut f).unwrap_err().to_string();
        assert!(
            error.contains("source-attachment"),
            "Source attachment capability refusal must be causal: {error}"
        );
        assert!(
            !requests(&f)
                .iter()
                .any(|r| r["method"] == "yoke/refactorIntent"),
            "Unsupported source attachment must refuse before intent dispatch"
        );
        assert_eq!(change_objects(&f), before);
        assert_eq!(
            fs::read_to_string(f.root.path().join("src/source.rs")).unwrap(),
            source
        );
    }
}

#[test]
fn direct_native_file_move_refuses_every_explicit_symbol_parent_form_before_provider() {
    for parent in [
        Value::Null,
        json!("bucket"),
        json!(""),
        json!(7),
        json!([]),
        json!({}),
    ] {
        let mut f = fixture(AnalyzerEpoch::V8, "valid", SOURCE);
        let before = change_objects(&f);
        let error = yoke::semantic_refactor::invoke(
            &mut f.session.engine, "move",
            &json!({"relativePath":"src/source.rs","targetRelativePath":"src/relocated.rs","targetParentNamePath":parent}),
            &Deadline::new(30000),
        ).unwrap_err().to_string();
        assert!(
            error.contains("File move does not accept a destination symbol parent"),
            "Direct native symbol-only input reached another layer: {error}"
        );
        assert!(
            requests(&f).is_empty(),
            "Direct file admission must precede provider launch"
        );
        assert_eq!(change_objects(&f), before);
        assert_eq!(
            fs::read_to_string(f.root.path().join("src/source.rs")).unwrap(),
            SOURCE
        );
        assert!(!f.root.path().join("src/relocated.rs").exists());
    }
}

#[test]
fn source_methods_locals_trait_and_foreign_functions_refuse_before_provider() {
    for source in [
        "pub struct S; impl S { pub fn helper() -> u8 { 7 } }\n",
        "pub fn outer() { fn helper() -> u8 { 7 } }\n",
        "pub trait T { fn helper() -> u8 { 7 } }\n",
        "pub trait T { fn helper() -> u8; }\n",
        "extern \"C\" { fn helper() -> u8; }\n",
    ] {
        let mut f = fixture(AnalyzerEpoch::V9, "valid", source);
        let before = change_objects(&f);
        let error = invoke(&mut f).unwrap_err().to_string();
        assert!(
            error.contains("root or inline-module functions")
                || error.contains("required actual Rust AST declaration"),
            "Source-parent refusal stopped at another rule: {error}"
        );
        assert!(requests(&f).is_empty());
        assert_eq!(change_objects(&f), before);
        assert_eq!(
            fs::read_to_string(f.root.path().join("src/source.rs")).unwrap(),
            source
        );
    }
}

#[test]
fn already_parented_same_file_function_refuses_before_provider() {
    for (source, parent, reason) in [
        (SOURCE, None, "already belongs to the destination root"),
        (
            "pub mod bucket { pub fn helper() -> u8 { 7 } }\n",
            Some("bucket"),
            "already belongs to the destination parent",
        ),
    ] {
        let mut f = fixture(AnalyzerEpoch::V8, "valid", source);
        let before = change_objects(&f);
        let mut args = json!({"relative_path":"src/source.rs","name_path":"helper","target_relative_path":"src/source.rs"});
        if let Some(parent) = parent {
            args["target_parent_name_path"] = json!(parent);
        }
        let error = yoke::agent_repl::invoke_facade(
            &mut f.session,
            "jb.move",
            args,
            None,
            &Deadline::new(30000),
        )
        .unwrap_err()
        .to_string();
        assert!(
            error.contains(reason),
            "Already-parented refusal stopped at another rule: {error}"
        );
        assert!(requests(&f).is_empty());
        assert_eq!(change_objects(&f), before);
        assert_eq!(
            fs::read_to_string(f.root.path().join("src/source.rs")).unwrap(),
            source
        );
    }
}

#[test]
fn original_preceding_edit_maps_the_selected_module_without_replacing_it() {
    let mut f = fixture(AnalyzerEpoch::V9, "shift_before_parent", SOURCE);
    let result = invoke(&mut f).unwrap();
    let transcript = requests(&f);
    assert_eq!(
        transcript
            .iter()
            .filter(|v| v["event"] == "fixtureOwnV1Result"
                && v["scenario"] == "shift_before_parent"
                && v["kind"] == "edit")
            .count(),
        1
    );
    assert_eq!(
        transcript
            .iter()
            .filter(|v| v["method"] == "yoke/refactorIntent")
            .count(),
        1
    );
    let snapshot = f
        .session
        .engine
        .store
        .snapshot(result["plan"]["resultSnapshotId"].as_str().unwrap())
        .unwrap();
    assert_eq!(
        yoke::query::source(&f.session.engine.store, &snapshot, "src/destination.rs")
            .unwrap()
            .0,
        "// provider preceding edit\npub mod bucket {\npub fn sentinel() {}\n\npub fn helper() -> u8 { 7 }\n}\n"
    );
    assert_eq!(
        yoke::query::source(&f.session.engine.store, &snapshot, "src/source.rs")
            .unwrap()
            .0,
        "// source comment\n\n"
    );
    assert_eq!(change_objects(&f).0.len(), 1);
    assert_eq!(change_objects(&f).1, 1);
    assert_eq!(result["complete"], false);
    assert_eq!(
        result["referenceCompleteness"],
        "exact_declaration_and_ast_placement_only_not_exhaustive_compiler_reference_binding"
    );
    assert_eq!(
        fs::read_to_string(f.root.path().join("src/source.rs")).unwrap(),
        SOURCE
    );
    assert_eq!(
        fs::read_to_string(f.root.path().join("src/destination.rs")).unwrap(),
        DESTINATION
    );
}

#[test]
fn invalid_third_rust_file_is_rejected_before_change_publication() {
    let mut f = fixture(AnalyzerEpoch::V9, "invalid_third_rust", SOURCE);
    let original = "pub fn reference() {}\n";
    fs::write(f.root.path().join("src/reference.rs"), original).unwrap();
    let before = change_objects(&f);
    let error = invoke(&mut f).unwrap_err().to_string();
    assert_eq!(
        requests(&f)
            .iter()
            .filter(|v| v["event"] == "fixtureOwnV1Result"
                && v["scenario"] == "invalid_third_rust"
                && v["kind"] == "edit")
            .count(),
        1,
        "Third-file refusal requires a real edit response"
    );
    assert_eq!(
        requests(&f)
            .iter()
            .filter(|v| v["method"] == "yoke/refactorIntent")
            .count(),
        1
    );
    assert!(
        error.contains("Move effect rejected: final Rust syntax invalid"),
        "Third-file syntax refusal stopped at another layer: {error}"
    );
    assert_eq!(change_objects(&f), before);
    assert_eq!(
        fs::read_to_string(f.root.path().join("src/source.rs")).unwrap(),
        SOURCE
    );
    assert_eq!(
        fs::read_to_string(f.root.path().join("src/destination.rs")).unwrap(),
        DESTINATION
    );
    assert_eq!(
        fs::read_to_string(f.root.path().join("src/reference.rs")).unwrap(),
        original
    );
}

const ROOT_PREAMBLE: &str = "#![allow(dead_code)]\npub mod bucket {\npub fn sentinel() {}\n}\n";
const MODULE_PREAMBLE: &str = "pub mod bucket {\n#![allow(dead_code)]\npub fn sentinel() {}\n}\n";
const OUTER_ATTACHMENT: &str =
    "pub mod bucket {\n/// Sentinel docs\n#[inline]\npub fn sentinel() {}\n}\n";
const PARENT_ATTACHMENT: &str =
    "/// Bucket docs\n#[allow(dead_code)]\npub mod bucket {\npub fn sentinel() {}\n}\n";

fn assert_edit_response(f: &Fixture, scenario: &str) {
    let transcript = requests(f);
    assert_eq!(
        transcript
            .iter()
            .filter(|v| v["method"] == "yoke/refactorIntent")
            .count(),
        1
    );
    assert_eq!(
        transcript
            .iter()
            .filter(|v| v["event"] == "fixtureOwnV1Result"
                && v["scenario"] == scenario
                && v["kind"] == "edit")
            .count(),
        1,
        "Requires one real emitted edit, not a fixture RPC error"
    );
    assert!(
        !transcript
            .iter()
            .any(|v| v["method"] == "workspace/executeCommand")
    );
}
fn reject_preservation(scenario: &str, destination: &str, reason: &str) {
    let mut f = fixture(AnalyzerEpoch::V9, scenario, SOURCE);
    fs::write(f.root.path().join("src/destination.rs"), destination).unwrap();
    let before = change_objects(&f);
    let result = invoke(&mut f);
    assert_edit_response(&f, scenario);
    let error = result
        .expect_err("Provider changed a protected source fact while preparing a move")
        .to_string();
    assert!(
        error.contains(&format!("Move effect rejected: {reason}")),
        "Preservation refusal stopped at another layer: {error}"
    );
    assert_eq!(change_objects(&f), before);
    assert_eq!(
        fs::read_to_string(f.root.path().join("src/source.rs")).unwrap(),
        SOURCE
    );
    assert_eq!(
        fs::read_to_string(f.root.path().join("src/destination.rs")).unwrap(),
        destination
    );
}
#[test]
fn provider_cannot_delete_the_original_root_inner_preamble() {
    reject_preservation(
        "delete_root_preamble",
        ROOT_PREAMBLE,
        "protected Rust preamble changed",
    );
}
#[test]
fn provider_cannot_delete_the_original_module_inner_preamble() {
    reject_preservation(
        "delete_module_preamble",
        MODULE_PREAMBLE,
        "protected Rust preamble changed",
    );
}
#[test]
fn provider_cannot_delete_existing_outer_documentation_or_attributes() {
    reject_preservation(
        "delete_outer_attachment",
        OUTER_ATTACHMENT,
        "existing Rust attachment changed",
    );
}
#[test]
fn provider_cannot_transfer_existing_outer_attachments_to_a_new_item() {
    reject_preservation(
        "transfer_outer_attachment",
        OUTER_ATTACHMENT,
        "existing Rust attachment changed",
    );
}
#[test]
fn provider_cannot_transfer_the_selected_parent_modules_own_attachments() {
    reject_preservation(
        "transfer_parent_attachment",
        PARENT_ATTACHMENT,
        "existing Rust attachment changed",
    );
}
#[test]
fn provider_cannot_recreate_the_original_function_in_a_comment_wrapped_insertion() {
    reject_preservation("wrapped_source_copy", DESTINATION, "extra function copy");
}
#[test]
fn provider_cannot_add_a_comment_wrapped_function_copy_before_the_target_module() {
    reject_preservation("wrapped_target_copy", DESTINATION, "extra function copy");
}

fn assert_result_sources(f: &Fixture, result: &Value, source: &str, destination: &str) {
    let snapshot = f
        .session
        .engine
        .store
        .snapshot(result["plan"]["resultSnapshotId"].as_str().unwrap())
        .unwrap();
    assert_eq!(
        yoke::query::source(&f.session.engine.store, &snapshot, "src/source.rs")
            .unwrap()
            .0,
        source
    );
    assert_eq!(
        yoke::query::source(&f.session.engine.store, &snapshot, "src/destination.rs")
            .unwrap()
            .0,
        destination
    );
    assert_eq!(change_objects(f).0.len(), 1);
    assert_eq!(change_objects(f).1, 1);
    assert_eq!(result["complete"], false);
    assert_eq!(result["commandsExecuted"], false);
}
fn with_inserted_helper(destination: &str) -> String {
    // Each literal corpus ends with the selected bucket's original closing brace.
    let at = destination.rfind('}').unwrap();
    format!(
        "{}\npub fn helper() -> u8 {{ 7 }}\n{}",
        &destination[..at],
        &destination[at..]
    )
}
#[test]
fn unmodified_preambles_and_existing_attachments_remain_valid_counterparts() {
    for destination in [
        ROOT_PREAMBLE,
        MODULE_PREAMBLE,
        OUTER_ATTACHMENT,
        PARENT_ATTACHMENT,
    ] {
        let mut f = fixture(AnalyzerEpoch::V9, "valid", SOURCE);
        fs::write(f.root.path().join("src/destination.rs"), destination).unwrap();
        let result = invoke(&mut f).unwrap();
        assert_edit_response(&f, "valid");
        assert_result_sources(
            &f,
            &result,
            "// source comment\n\n",
            &with_inserted_helper(destination),
        );
        assert_eq!(
            fs::read_to_string(f.root.path().join("src/source.rs")).unwrap(),
            SOURCE
        );
        assert_eq!(
            fs::read_to_string(f.root.path().join("src/destination.rs")).unwrap(),
            destination
        );
    }
}
#[test]
fn added_comment_and_raw_string_declaration_text_is_not_a_function_copy() {
    let mut f = fixture(AnalyzerEpoch::V9, "literal_decl_text", SOURCE);
    let result = invoke(&mut f).unwrap();
    assert_edit_response(&f, "literal_decl_text");
    assert_result_sources(
        &f,
        &result,
        "// source comment\n\n/* pub fn helper() -> u8 { 7 } */\npub const MOVE_TEXT: &str = r###\"pub fn helper() -> u8 { 7 }\"###;\n\n",
        &with_inserted_helper(DESTINATION),
    );
    assert_eq!(
        fs::read_to_string(f.root.path().join("src/source.rs")).unwrap(),
        SOURCE
    );
    assert_eq!(
        fs::read_to_string(f.root.path().join("src/destination.rs")).unwrap(),
        DESTINATION
    );
}
#[test]
fn original_identical_declarations_keep_their_distinct_canonical_identity() {
    let source = "pub fn helper() -> u8 { 7 }\npub fn helper() -> u8 { 7 }\n";
    for destination in [
        DESTINATION.to_owned(),
        format!("pub fn helper() -> u8 {{ 7 }}\n{DESTINATION}"),
    ] {
        let mut f = fixture(AnalyzerEpoch::V9, "valid", source);
        fs::write(f.root.path().join("src/destination.rs"), &destination).unwrap();
        let result = yoke::agent_repl::invoke_facade(&mut f.session, "jb.move", json!({"relative_path":"src/source.rs","name_path":"helper[1]","target_relative_path":"src/destination.rs","target_parent_name_path":"bucket"}), None, &Deadline::new(30000)).unwrap();
        assert_edit_response(&f, "valid");
        assert_result_sources(
            &f,
            &result,
            "pub fn helper() -> u8 { 7 }\n\n",
            &with_inserted_helper(&destination),
        );
        let transcript = requests(&f);
        let request = transcript
            .iter()
            .find(|v| v["method"] == "yoke/refactorIntent")
            .unwrap();
        assert_eq!(
            request["params"]["intent"]["source"]["canonicalNamePath"],
            "helper[1]"
        );
        assert_eq!(
            fs::read_to_string(f.root.path().join("src/source.rs")).unwrap(),
            source
        );
        assert_eq!(
            fs::read_to_string(f.root.path().join("src/destination.rs")).unwrap(),
            destination
        );
    }
}

const SOURCE_ROOT_PREAMBLE: &str = "#![allow(dead_code)]\npub fn helper() -> u8 { 7 }\n";
const SOURCE_MODULE_PREAMBLE: &str =
    "pub mod source_bucket {\n#![allow(dead_code)]\npub fn helper() -> u8 { 7 }\n}\n";
const SOURCE_REMAINING_ATTACHMENT: &str =
    "pub fn helper() -> u8 { 7 }\n/// Remaining docs\n#[inline]\npub fn remaining() {}\n";
fn reject_source_preservation(scenario: &str, source: &str, reason: &str) {
    let mut f = fixture(AnalyzerEpoch::V9, scenario, source);
    let before = change_objects(&f);
    let result = invoke(&mut f);
    assert_edit_response(&f, scenario);
    let error = result
        .expect_err("Provider changed protected source syntax outside the moved declaration")
        .to_string();
    assert!(
        error.contains(&format!("Move effect rejected: {reason}")),
        "Source preservation refusal stopped at another layer: {error}"
    );
    assert_eq!(change_objects(&f), before);
    assert_eq!(
        fs::read_to_string(f.root.path().join("src/source.rs")).unwrap(),
        source
    );
    assert_eq!(
        fs::read_to_string(f.root.path().join("src/destination.rs")).unwrap(),
        DESTINATION
    );
}
#[test]
fn provider_cannot_delete_source_root_or_module_inner_preambles() {
    for source in [SOURCE_ROOT_PREAMBLE, SOURCE_MODULE_PREAMBLE] {
        reject_source_preservation(
            "delete_source_preamble",
            source,
            "protected Rust preamble changed",
        );
    }
}
#[test]
fn provider_cannot_transfer_a_remaining_source_items_outer_attachments() {
    reject_source_preservation(
        "transfer_source_attachment",
        SOURCE_REMAINING_ATTACHMENT,
        "existing Rust attachment changed",
    );
}
#[test]
fn unchanged_source_preambles_and_remaining_attachments_are_valid_counterparts() {
    for (source, remaining) in [
        (SOURCE_ROOT_PREAMBLE, "#![allow(dead_code)]\n\n"),
        (
            SOURCE_MODULE_PREAMBLE,
            "pub mod source_bucket {\n#![allow(dead_code)]\n\n}\n",
        ),
        (
            SOURCE_REMAINING_ATTACHMENT,
            "\n/// Remaining docs\n#[inline]\npub fn remaining() {}\n",
        ),
    ] {
        let mut f = fixture(AnalyzerEpoch::V9, "valid", source);
        let result = invoke(&mut f).unwrap();
        assert_edit_response(&f, "valid");
        assert_result_sources(&f, &result, remaining, &with_inserted_helper(DESTINATION));
        assert_eq!(
            fs::read_to_string(f.root.path().join("src/source.rs")).unwrap(),
            source
        );
        assert_eq!(
            fs::read_to_string(f.root.path().join("src/destination.rs")).unwrap(),
            DESTINATION
        );
    }
}

// These positive cases distinguish ownership preservation from freezing all
// contents of an existing attributed declaration.
fn assert_attributed_reference_update(scenario: &str, declaration: &str, owner_kind: &str) {
    let destination = format!("{declaration}{DESTINATION}");
    let mut f = fixture(AnalyzerEpoch::V9, scenario, SOURCE);
    fs::write(f.root.path().join("src/destination.rs"), &destination).unwrap();
    let result = invoke(&mut f);
    assert_edit_response(&f, scenario);
    let result = result.expect("A checked reference edit may change an existing item's path while preserving its original attachment owner");
    let updated = destination.replace(
        "crate::source::helper",
        "crate::destination::bucket::helper",
    );
    let expected = with_inserted_helper(&updated);
    assert_result_sources(&f, &result, "// source comment\n\n", &expected);
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&tree_sitter_rust::LANGUAGE.into())
        .unwrap();
    let tree = parser.parse(expected.as_bytes(), None).unwrap();
    assert!(!tree.root_node().has_error());
    let mut cursor = tree.root_node().walk();
    let owners: Vec<_> = tree
        .root_node()
        .named_children(&mut cursor)
        .filter(|node| node.kind() == "attribute_item")
        .map(|node| node.next_named_sibling().unwrap().kind())
        .collect();
    assert_eq!(
        owners,
        vec![owner_kind],
        "Original outer attribute must still belong to the same declaration kind"
    );
    assert_eq!(
        result["referenceCompleteness"],
        "exact_declaration_and_ast_placement_only_not_exhaustive_compiler_reference_binding"
    );
    assert_eq!(
        fs::read_to_string(f.root.path().join("src/source.rs")).unwrap(),
        SOURCE
    );
    assert_eq!(
        fs::read_to_string(f.root.path().join("src/destination.rs")).unwrap(),
        destination
    );
}
#[test]
fn attributed_use_reference_path_can_change_without_replacing_its_owner() {
    assert_attributed_reference_update(
        "update_attributed_use",
        "/// Reference docs\n#[allow(unused_imports)]\nuse crate::source::helper;\n",
        "use_declaration",
    );
}
#[test]
fn attributed_const_and_static_initializers_can_change_reference_paths() {
    for (declaration, kind) in [
        (
            "/// Reference docs\n#[allow(dead_code)]\npub const REF: fn() -> u8 = crate::source::helper;\n",
            "const_item",
        ),
        (
            "/// Reference docs\n#[allow(dead_code)]\npub static REF: fn() -> u8 = crate::source::helper;\n",
            "static_item",
        ),
    ] {
        assert_attributed_reference_update("update_attributed_const", declaration, kind);
    }
}
