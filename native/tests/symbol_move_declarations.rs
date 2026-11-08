//! Owned-protocol fixtures exercise real capture, checked edits and change plans.
//! These cases do not assert compiler reference completeness or live IDE parity.
use serde_json::{Value, json};
use std::{fs, path::Path};
use yoke::{
    Engine,
    analysis::AnalyzerEpoch,
    capture::Deadline,
    lsp,
    project_session::{HostProfile, Session},
};

const DECLARATIONS: &[(&str, &str)] = &[
    ("struct", "pub struct Helper<T> { pub value: T }"),
    ("enum", "pub enum Helper<T> { Value(T), Missing }"),
    ("interface", "pub trait Helper { fn read(&self) -> u8; }"),
    ("type", "pub type Helper<T> = Option<T>;"),
    ("constant", "pub const Helper: u8 = 7;"),
];
const DESTINATION: &str = "pub mod bucket {\npub fn sentinel() {}\n}\n";
const PREFIX: &str = "// Original caf\u{00e9} \u{1f642}\n";

struct Fixture {
    session: Session,
    root: tempfile::TempDir,
    log: tempfile::TempDir,
}
fn fixture(epoch: AnalyzerEpoch, scenario: &str, source: &str) -> Fixture {
    let root = tempfile::tempdir().unwrap();
    let log = tempfile::tempdir().unwrap();
    fs::write(root.path().join("source.rs"), source).unwrap();
    fs::write(root.path().join("destination.rs"), DESTINATION).unwrap();
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
        Some("declaration-moves"),
        &Deadline::new(30000),
    )
    .unwrap();
    Fixture { session, root, log }
}
fn invoke(f: &mut Fixture, selector: &str, target: &str, parent: bool) -> anyhow::Result<Value> {
    let mut args =
        json!({"relative_path":"source.rs","name_path":selector,"target_relative_path":target});
    if parent {
        args["target_parent_name_path"] = json!("bucket");
    }
    yoke::agent_repl::invoke_facade(&mut f.session, "jb.move", args, None, &Deadline::new(30000))
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
fn objects(f: &Fixture) -> (usize, usize) {
    let db = &f.session.engine.store.db;
    (
        db.query_row(
            "SELECT COUNT(*) FROM records WHERE kind IN ('plan','approval','receipt')",
            [],
            |r| r.get(0),
        )
        .unwrap(),
        db.query_row(
            "SELECT COUNT(*) FROM snapshots WHERE json_extract(value,'$.kind')='derived'",
            [],
            |r| r.get(0),
        )
        .unwrap(),
    )
}
fn source_after(f: &Fixture, result: &Value, path: &str) -> String {
    let snapshot = f
        .session
        .engine
        .store
        .snapshot(result["plan"]["resultSnapshotId"].as_str().unwrap())
        .unwrap();
    yoke::query::source(&f.session.engine.store, &snapshot, path)
        .unwrap()
        .0
}
fn assert_response(f: &Fixture, scenario: &str) {
    let log = requests(f);
    assert_eq!(
        log.iter()
            .filter(|r| r["method"] == "yoke/refactorIntent")
            .count(),
        1
    );
    assert_eq!(
        log.iter()
            .filter(|r| r["event"] == "fixtureOwnV1Result"
                && r["kind"] == "edit"
                && r["scenario"] == scenario)
            .count(),
        1
    );
    assert!(
        !log.iter()
            .any(|r| r["method"] == "workspace/executeCommand")
    );
}
fn accept(kind: &str, declaration: &str) {
    for epoch in [AnalyzerEpoch::V8, AnalyzerEpoch::V9] {
        for parent in [false, true] {
            let original = format!("{PREFIX}{declaration}\n");
            let mut f = fixture(epoch, "valid", &original);
            let result = invoke(&mut f, "Helper", "destination.rs", parent)
                .unwrap_or_else(|e| panic!("{kind}, {epoch:?}, parent={parent}: {e}"));
            assert_response(&f, "valid");
            assert_eq!(
                source_after(&f, &result, "source.rs"),
                format!("{PREFIX}\n")
            );
            let expected = if parent {
                format!("pub mod bucket {{\npub fn sentinel() {{}}\n\n{declaration}\n}}\n")
            } else {
                format!("{DESTINATION}\n{declaration}\n")
            };
            assert_eq!(source_after(&f, &result, "destination.rs"), expected);
            assert_eq!(objects(&f), (1, 1));
            assert_eq!(result["complete"], false);
            assert_eq!(result["commandsExecuted"], false);
            let log = requests(&f);
            let request = log
                .iter()
                .find(|r| r["method"] == "yoke/refactorIntent")
                .unwrap();
            let source = &request["params"]["intent"]["source"];
            assert_eq!(source["startByte"], PREFIX.len());
            assert_eq!(source["endByte"], PREFIX.len() + declaration.len());
            assert_eq!(source["nameStartByte"], original.find("Helper").unwrap());
            assert_eq!(source["nameEndByte"], original.find("Helper").unwrap() + 6);
            let graph = f
                .session
                .engine
                .store
                .graph(source["snapshotId"].as_str().unwrap())
                .unwrap();
            let entity = graph
                .entities
                .iter()
                .find(|e| e.id == source["entityId"])
                .unwrap();
            assert_eq!(entity.kind, kind);
            let snapshot = f
                .session
                .engine
                .store
                .snapshot(result["plan"]["resultSnapshotId"].as_str().unwrap())
                .unwrap();
            assert_eq!(
                yoke::capture::analyzer_epoch_for_snapshot(&f.session.engine.store, &snapshot)
                    .unwrap(),
                epoch
            );
            assert_eq!(
                fs::read_to_string(f.root.path().join("source.rs")).unwrap(),
                original
            );
            assert_eq!(
                fs::read_to_string(f.root.path().join("destination.rs")).unwrap(),
                DESTINATION
            );
        }
    }
}
macro_rules! accepts {
    ($($name:ident => $index:literal),* $(,)?) => { $(
        #[test] fn $name() { let (kind, declaration) = DECLARATIONS[$index]; accept(kind, declaration); }
    )* };
}
accepts! {
    struct_moves_preserve_generic_fields => 0,
    enum_moves_preserve_variants => 1,
    trait_moves_preserve_associated_signatures => 2,
    type_alias_moves_preserve_generic_target => 3,
    const_moves_preserve_initializer => 4,
}

#[test]
fn nonfunction_moves_cannot_leave_or_add_a_comment_wrapped_ast_copy() {
    for (_, declaration) in DECLARATIONS {
        for scenario in ["wrapped_source_copy", "wrapped_target_copy"] {
            let original = format!("{PREFIX}{declaration}\n");
            let mut f = fixture(AnalyzerEpoch::V9, scenario, &original);
            let error = invoke(&mut f, "Helper", "destination.rs", true)
                .unwrap_err()
                .to_string();
            assert_response(&f, scenario);
            assert!(
                error.contains("Move effect rejected: extra declaration copy"),
                "{scenario}: {error}"
            );
            assert_eq!(objects(&f), (0, 0));
            assert_eq!(
                fs::read_to_string(f.root.path().join("source.rs")).unwrap(),
                original
            );
            assert_eq!(
                fs::read_to_string(f.root.path().join("destination.rs")).unwrap(),
                DESTINATION
            );
        }
    }
}

#[test]
fn declaration_moves_still_require_an_exact_cut_and_unchanged_insert_at_the_selected_parent() {
    for (_, declaration) in DECLARATIONS {
        for (scenario, reason) in [
            ("copy_only", "exact source cut required"),
            ("delete_only", "exact declaration insertion required"),
            ("wrong_parent", "insertion destination differs"),
            ("whole_source", "exact source cut required"),
        ] {
            let original = format!("{PREFIX}{declaration}\n");
            let mut f = fixture(AnalyzerEpoch::V9, scenario, &original);
            let error = invoke(&mut f, "Helper", "destination.rs", true)
                .unwrap_err()
                .to_string();
            assert_response(&f, scenario);
            assert!(
                error.contains(&format!("Move effect rejected: {reason}")),
                "{scenario}: {error}"
            );
            assert_eq!(objects(&f), (0, 0));
            assert_eq!(
                fs::read_to_string(f.root.path().join("source.rs")).unwrap(),
                original
            );
            assert_eq!(
                fs::read_to_string(f.root.path().join("destination.rs")).unwrap(),
                DESTINATION
            );
        }
    }
}

#[test]
fn same_file_and_inline_source_moves_use_original_coordinates() {
    for (_, declaration) in DECLARATIONS {
        let original = format!("pub mod origin {{\n{declaration}\n}}\npub mod bucket {{}}\n");
        let mut f = fixture(AnalyzerEpoch::V9, "valid", &original);
        let result = invoke(&mut f, "origin/Helper", "source.rs", true).unwrap();
        assert_response(&f, "valid");
        assert_eq!(
            source_after(&f, &result, "source.rs"),
            format!("pub mod origin {{\n\n}}\npub mod bucket {{\n{declaration}\n}}\n")
        );
        assert_eq!(result["plan"]["files"].as_array().unwrap().len(), 1);
        assert_eq!(objects(&f), (1, 1));
        assert_eq!(
            fs::read_to_string(f.root.path().join("source.rs")).unwrap(),
            original
        );
    }
}

#[test]
fn unrelated_identical_declarations_keep_their_original_provenance() {
    for (_, declaration) in DECLARATIONS {
        let original = format!("{declaration}\npub mod other {{\n{declaration}\n}}\n");
        let mut f = fixture(AnalyzerEpoch::V9, "valid", &original);
        let result = invoke(&mut f, "/Helper", "destination.rs", true).unwrap();
        assert_response(&f, "valid");
        assert_eq!(
            source_after(&f, &result, "source.rs"),
            format!("\npub mod other {{\n{declaration}\n}}\n")
        );
        assert_eq!(
            source_after(&f, &result, "destination.rs"),
            format!("pub mod bucket {{\npub fn sentinel() {{}}\n\n{declaration}\n}}\n")
        );
    }
}

#[test]
fn nonfree_and_module_declarations_refuse_before_provider_work() {
    let sources: [String; 3] = [
        "pub struct Owner;\nimpl Owner { pub const Helper: u8 = 7; }\n".into(),
        "pub trait Owner { type Helper; }\n".into(),
        "pub mod Helper {}\n".into(),
    ];
    for original in sources {
        let mut f = fixture(AnalyzerEpoch::V9, "valid", &original);
        assert!(invoke(&mut f, "Helper", "destination.rs", true).is_err());
        assert!(requests(&f).is_empty());
        assert_eq!(objects(&f), (0, 0));
        assert_eq!(
            fs::read_to_string(f.root.path().join("source.rs")).unwrap(),
            original
        );
    }
}

const ATTACHMENTS: &str =
    "/// Selected caf\u{00e9} \u{1f642}\n#[allow(\n dead_code\n)]\n// Intervening trivia\n\n";
const PREAMBLE: &str = "//! Original module docs\n#![allow(dead_code)]\n";

fn all_declarations() -> Vec<&'static str> {
    std::iter::once("pub fn Helper() -> u8 { 7 }")
        .chain(DECLARATIONS.iter().map(|(_, declaration)| *declaration))
        .collect()
}

#[test]
fn negotiated_attachments_move_as_one_bundle_with_distinct_entity_and_text_ranges() {
    for declaration in all_declarations() {
        for epoch in [AnalyzerEpoch::V8, AnalyzerEpoch::V9] {
            for parent in [false, true] {
                let original =
                    format!("{PREAMBLE}{ATTACHMENTS}{declaration}\npub fn tail() {{}}\n");
                let mut f = fixture(epoch, "valid", &original);
                let result = invoke(&mut f, "Helper", "destination.rs", parent).unwrap();
                assert_response(&f, "valid");
                assert_eq!(
                    source_after(&f, &result, "source.rs"),
                    format!("{PREAMBLE}\npub fn tail() {{}}\n")
                );
                let bundle = format!("{ATTACHMENTS}{declaration}");
                let expected = if parent {
                    format!("pub mod bucket {{\npub fn sentinel() {{}}\n\n{bundle}\n}}\n")
                } else {
                    format!("{DESTINATION}\n{bundle}\n")
                };
                assert_eq!(source_after(&f, &result, "destination.rs"), expected);
                let log = requests(&f);
                let request = log
                    .iter()
                    .find(|r| r["method"] == "yoke/refactorIntent")
                    .unwrap();
                let intent = &request["params"]["intent"];
                let core_start = PREAMBLE.len() + ATTACHMENTS.len();
                assert_eq!(intent["source"]["startByte"], core_start);
                assert_eq!(intent["source"]["endByte"], core_start + declaration.len());
                assert_eq!(intent["sourceTextRange"]["startByte"], PREAMBLE.len());
                assert_eq!(
                    intent["sourceTextRange"]["endByte"],
                    core_start + declaration.len()
                );
                let graph = f
                    .session
                    .engine
                    .store
                    .graph(intent["source"]["snapshotId"].as_str().unwrap())
                    .unwrap();
                let entity = graph
                    .entities
                    .iter()
                    .find(|e| e.id == intent["source"]["entityId"])
                    .unwrap();
                assert_eq!(entity.start_byte, core_start);
                assert_eq!(objects(&f), (1, 1));
                assert_eq!(result["complete"], false);
                assert_eq!(result["commandsExecuted"], false);
                assert_eq!(
                    fs::read_to_string(f.root.path().join("source.rs")).unwrap(),
                    original
                );
                assert_eq!(
                    fs::read_to_string(f.root.path().join("destination.rs")).unwrap(),
                    DESTINATION
                );
            }
        }
    }
}

#[test]
fn attachments_require_an_explicit_recognized_provider_capability() {
    for scenario in ["missing_attachment_capability", "bad_attachment_capability"] {
        let original = format!("{ATTACHMENTS}pub struct Helper;\n");
        let mut f = fixture(AnalyzerEpoch::V9, scenario, &original);
        let error = invoke(&mut f, "Helper", "destination.rs", true)
            .unwrap_err()
            .to_string();
        let log = requests(&f);
        assert!(
            log.iter().any(|r| r["method"] == "initialize"),
            "Must reach capability negotiation: {error}"
        );
        assert!(!log.iter().any(|r| r["method"] == "yoke/refactorIntent"));
        assert!(error.contains("source-attachment"), "{error}");
        assert_eq!(objects(&f), (0, 0));
        assert_eq!(
            fs::read_to_string(f.root.path().join("source.rs")).unwrap(),
            original
        );
    }
}

#[test]
fn provider_cannot_leave_source_attachments_or_strip_them_from_the_target() {
    for declaration in all_declarations() {
        for (scenario, reason) in [
            ("node_only_transfer", "exact source cut required"),
            (
                "strip_transferred_attachments",
                "exact declaration insertion required",
            ),
        ] {
            let original = format!("{ATTACHMENTS}{declaration}\npub fn tail() {{}}\n");
            let mut f = fixture(AnalyzerEpoch::V9, scenario, &original);
            let error = invoke(&mut f, "Helper", "destination.rs", true)
                .unwrap_err()
                .to_string();
            assert_response(&f, scenario);
            assert!(
                error.contains(&format!("Move effect rejected: {reason}")),
                "{error}"
            );
            assert_eq!(objects(&f), (0, 0));
            assert_eq!(
                fs::read_to_string(f.root.path().join("source.rs")).unwrap(),
                original
            );
        }
    }
}

#[test]
fn same_file_inline_bundle_preserves_inner_docs_and_other_items_attachments() {
    let original = format!(
        "pub mod origin {{\n//! Origin docs\n{ATTACHMENTS}pub struct Helper;\n/// Tail docs\npub fn tail() {{}}\n}}\npub mod bucket {{}}\n"
    );
    let mut f = fixture(AnalyzerEpoch::V9, "valid", &original);
    let result = invoke(&mut f, "origin/Helper", "source.rs", true).unwrap();
    assert_response(&f, "valid");
    assert_eq!(
        source_after(&f, &result, "source.rs"),
        format!(
            "pub mod origin {{\n//! Origin docs\n\n/// Tail docs\npub fn tail() {{}}\n}}\npub mod bucket {{\n{ATTACHMENTS}pub struct Helper;\n}}\n"
        )
    );
    assert_eq!(result["plan"]["files"].as_array().unwrap().len(), 1);
    assert_eq!(
        fs::read_to_string(f.root.path().join("source.rs")).unwrap(),
        original
    );
}

#[test]
fn block_docs_crlf_and_bom_keep_exact_transfer_bytes_and_ordinary_leading_comments() {
    let prefix = "\u{feff}//! Root docs\r\n// Ordinary leading comment\r\n";
    let bundle = "/** Selected docs */\r\n#[allow(dead_code)]\r\npub enum Helper { A, B }";
    let original = format!("{prefix}{bundle}\r\n");
    let mut f = fixture(AnalyzerEpoch::V9, "valid", &original);
    let result = invoke(&mut f, "Helper", "destination.rs", false).unwrap();
    assert_response(&f, "valid");
    assert_eq!(
        source_after(&f, &result, "source.rs"),
        format!("{prefix}\r\n")
    );
    assert_eq!(
        source_after(&f, &result, "destination.rs"),
        format!("{DESTINATION}\n{bundle}\n")
    );
    assert_eq!(
        fs::read_to_string(f.root.path().join("source.rs")).unwrap(),
        original
    );
}

#[test]
fn transferred_bundles_accept_crlf_insertion_padding_without_changing_their_bytes() {
    let bundle = "/// Owned docs\r\n#[allow(dead_code)]\r\npub struct Helper;";
    let original = format!("{bundle}\r\n");
    let mut f = fixture(AnalyzerEpoch::V9, "crlf_padding", &original);
    let result = invoke(&mut f, "Helper", "destination.rs", true).unwrap();
    assert_response(&f, "crlf_padding");
    assert_eq!(source_after(&f, &result, "source.rs"), "\r\n");
    assert_eq!(
        source_after(&f, &result, "destination.rs"),
        format!("pub mod bucket {{\npub fn sentinel() {{}}\n\r\n{bundle}\r\n}}\n")
    );
}

#[test]
fn unattached_declarations_retain_the_three_field_provider_capability_contract() {
    let mut f = fixture(
        AnalyzerEpoch::V9,
        "missing_attachment_capability",
        "pub struct Helper;\n",
    );
    let result = invoke(&mut f, "Helper", "destination.rs", false).unwrap();
    assert_response(&f, "missing_attachment_capability");
    let log = requests(&f);
    let request = log
        .iter()
        .find(|r| r["method"] == "yoke/refactorIntent")
        .unwrap();
    assert!(request["params"]["intent"].get("sourceTextRange").is_none());
    assert_eq!(source_after(&f, &result, "source.rs"), "\n");
    assert_eq!(
        source_after(&f, &result, "destination.rs"),
        format!("{DESTINATION}\npub struct Helper;\n")
    );
}

#[test]
fn exact_moved_declaration_retains_nested_module_preambles_and_attributes() {
    let declaration = "pub fn Helper() { mod nested {\n//! Nested docs\n#![allow(dead_code)]\n/// Nested helper docs\n#[inline]\nfn inner() {}\n} }";
    let original = format!("{declaration}\n");
    let mut f = fixture(AnalyzerEpoch::V9, "valid", &original);
    let result = invoke(&mut f, "Helper", "destination.rs", true).unwrap();
    assert_response(&f, "valid");
    assert_eq!(source_after(&f, &result, "source.rs"), "\n");
    assert_eq!(
        source_after(&f, &result, "destination.rs"),
        format!("pub mod bucket {{\npub fn sentinel() {{}}\n\n{declaration}\n}}\n")
    );
}
