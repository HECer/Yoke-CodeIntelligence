//! Behavioral acceptance tests for the host-selected own_v1 symbol-move contract.
//! This uses existing generic JSON APIs so missing own_v1 support is behavioral RED.
use serde_json::{Value, json};
use std::{fs, path::Path};
use yoke::{
    Engine,
    analysis::AnalyzerEpoch,
    capture::{self, CaptureOptions, Deadline},
    lsp,
    project_session::{HostProfile, Session},
};

const SOURCE: &str = "// UTF-8 caf\u{00e9} \u{1f642}\npub fn helper() -> u8 { 7 }\n";
const REMAINING: &str = "// UTF-8 caf\u{00e9} \u{1f642}\n\n";
const DESTINATION: &str = "pub mod bucket {\npub fn sentinel() {}\n}\n";
const FILE_TOP_RESULT: &str =
    "pub mod bucket {\npub fn sentinel() {}\n}\n\npub fn helper() -> u8 { 7 }\n";
const PARENT_RESULT: &str =
    "pub mod bucket {\npub fn sentinel() {}\n\npub fn helper() -> u8 { 7 }\n}\n";

struct Fixture {
    root: tempfile::TempDir,
    log: tempfile::TempDir,
    session: Session,
}
fn configuration(own: bool) -> anyhow::Result<lsp::HostConfiguration> {
    let mut value = json!({"passiveProfile":"contract_fixture"});
    if own {
        value["refactorIntentProfile"] = json!("own_v1");
    }
    Ok(serde_json::from_value(value)?)
}
fn fixture(epoch: AnalyzerEpoch, scenario: &str, own: bool) -> Fixture {
    fixture_with_options(epoch, scenario, own, CaptureOptions::default())
}
fn fixture_with_options(
    epoch: AnalyzerEpoch,
    scenario: &str,
    own: bool,
    options: CaptureOptions,
) -> Fixture {
    let root = tempfile::tempdir().unwrap();
    let log = tempfile::tempdir().unwrap();
    fs::create_dir(root.path().join("src")).unwrap();
    for directory in ["outside", "ignored", "node_modules/sdk"] {
        fs::create_dir_all(root.path().join(directory)).unwrap();
        fs::write(root.path().join(directory).join("source.rs"), SOURCE).unwrap();
        fs::write(
            root.path().join(directory).join("destination.rs"),
            DESTINATION,
        )
        .unwrap();
    }
    fs::write(root.path().join(".gitignore"), "ignored/\n!node_modules/\n").unwrap();
    fs::write(root.path().join("src/source.rs"), SOURCE).unwrap();
    fs::write(root.path().join("src/destination.rs"), DESTINATION).unwrap();
    let mut engine = Engine::open(root.path()).unwrap();
    engine.capture_epoch = epoch;
    engine.capture_options = options;
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
            configuration: configuration(own).expect(
                "Own-v1 JSON host opt-in must be accepted after its independent schema RED",
            ),
        })
        .unwrap();
    let session = Session::open(
        engine,
        HostProfile::default(),
        Some("symbol-move-own-v1"),
        &Deadline::new(30000),
    )
    .unwrap();
    Fixture { root, log, session }
}
fn objects(f: &Fixture) -> (usize, usize) {
    let changes = f
        .session
        .engine
        .store
        .db
        .query_row(
            "SELECT COUNT(*) FROM records WHERE kind IN ('plan','approval','receipt')",
            [],
            |r| r.get(0),
        )
        .unwrap();
    let derived = f
        .session
        .engine
        .store
        .db
        .query_row(
            "SELECT COUNT(*) FROM snapshots WHERE json_extract(value,'$.kind')='derived'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    (changes, derived)
}
fn requests(f: &Fixture) -> Vec<Value> {
    let path = f.log.path().join("protocol.log");
    if !path.exists() {
        return vec![];
    }
    fs::read_to_string(path)
        .unwrap()
        .lines()
        .map(|s| serde_json::from_str(s).unwrap())
        .collect()
}
fn assert_fixture_response(f: &Fixture, scenario: &str, kind: &str) {
    let transcript = requests(f);
    let responses: Vec<_> = transcript
        .iter()
        .filter(|value| value["event"] == "fixtureOwnV1Result")
        .collect();
    assert_eq!(
        responses.len(),
        1,
        "Fixture must actually return its result"
    );
    assert_eq!(responses[0]["scenario"], scenario);
    assert_eq!(responses[0]["kind"], kind);
}
fn assert_graph_symbol_anchor(binding: &Value, entity: &yoke::model::Entity) {
    assert_eq!(binding["entityId"], entity.id);
    assert_eq!(binding["snapshotId"], entity.snapshot_id);
    assert_eq!(binding["path"], entity.path);
    assert_eq!(binding["startByte"], json!(entity.start_byte));
    assert_eq!(binding["endByte"], json!(entity.end_byte));
    assert_eq!(binding["nameStartByte"], json!(entity.name_start_byte));
    assert_eq!(binding["nameEndByte"], json!(entity.name_end_byte));
}
fn invoke(f: &mut Fixture, parent: Option<&str>) -> anyhow::Result<Value> {
    let mut args = json!({"relative_path":"src/source.rs","name_path":"helper","target_relative_path":"src/destination.rs"});
    if let Some(parent) = parent {
        args["target_parent_name_path"] = json!(parent);
    }
    yoke::agent_repl::invoke_facade(&mut f.session, "jb.move", args, None, &Deadline::new(30000))
}
fn assert_originals(f: &Fixture, source: &str, destination: &str) {
    assert_eq!(
        fs::read_to_string(f.root.path().join("src/source.rs")).unwrap(),
        source
    );
    assert_eq!(
        fs::read_to_string(f.root.path().join("src/destination.rs")).unwrap(),
        destination
    );
}
fn assert_refused(f: &mut Fixture, parent: Option<&str>) {
    let value = invoke(f, parent);
    assert!(
        value.is_err(),
        "Unsupported/mismatched intent prepared a plan: {value:?}"
    );
    assert_eq!(
        objects(f),
        (0, 0),
        "Deterministic refusal published hidden change objects"
    );
    assert_originals(f, SOURCE, DESTINATION);
    assert!(
        !requests(f)
            .iter()
            .any(|r| r["method"] == "workspace/executeCommand")
    );
}
fn assert_prepared(
    f: &Fixture,
    value: &Value,
    expected_source: &str,
    expected_destination: &str,
    epoch: AnalyzerEpoch,
) {
    let plan = &value["plan"];
    let snapshot = f
        .session
        .engine
        .store
        .snapshot(plan["resultSnapshotId"].as_str().unwrap())
        .unwrap();
    assert_eq!(
        yoke::query::source(&f.session.engine.store, &snapshot, "src/source.rs")
            .unwrap()
            .0,
        expected_source
    );
    assert_eq!(
        yoke::query::source(&f.session.engine.store, &snapshot, "src/destination.rs")
            .unwrap()
            .0,
        expected_destination
    );
    assert_eq!(
        capture::analyzer_epoch_for_snapshot(&f.session.engine.store, &snapshot).unwrap(),
        epoch
    );
    assert_eq!(plan["files"].as_array().unwrap().len(), 2);
    assert_eq!(objects(f), (1, 1));
    assert_eq!(value["commandsExecuted"], false);
    assert_eq!(
        value["complete"], false,
        "Fixture success is not exhaustive compiler-reference proof"
    );
}

#[test]
fn own_v1_host_opt_in_is_optional_and_default_profile_bytes_remain_identical() {
    assert_eq!(
        serde_json::to_string(&lsp::HostConfiguration::default()).unwrap(),
        r#"{"tcpEndpoint":null,"initializationOptions":null,"settings":null,"workspaceFolders":[],"languageIds":{},"openLanguages":[]}"#
    );
    let own =
        configuration(true).expect("Explicit own_v1 must be a host-selected supported profile");
    assert_eq!(
        serde_json::to_value(own).unwrap()["refactorIntentProfile"],
        "own_v1"
    );
    for invalid in [
        json!(true),
        json!("own_v2"),
        json!({}),
        json!({"own_v1": null}),
    ] {
        assert!(
            serde_json::from_value::<lsp::HostConfiguration>(
                json!({"refactorIntentProfile":invalid})
            )
            .is_err()
        );
    }
}

#[test]
fn changing_only_the_owned_profile_refuses_session_resume_without_mutation_or_provider_start() {
    fn session_bytes(engine: &Engine) -> Vec<(String, Vec<u8>)> {
        engine
            .store
            .db
            .prepare("SELECT id,CAST(value AS BLOB) FROM records WHERE kind=?1 ORDER BY id")
            .unwrap()
            .query_map([yoke::project_session::RECORD_KIND], |row| {
                Ok((row.get(0)?, row.get(1)?))
            })
            .unwrap()
            .map(Result::unwrap)
            .collect()
    }

    for epoch in [AnalyzerEpoch::V8, AnalyzerEpoch::V9] {
        for own in [false, true] {
            let Fixture { root, log, session } = fixture(epoch, "valid", own);
            let server = session.engine.servers["rust"].clone();
            let original_configuration = session.engine.server_configurations["rust"].clone();
            let before = session_bytes(&session.engine);
            assert_eq!(before.len(), 1);
            drop(session);

            let registered_engine = |configuration| {
                let mut engine = Engine::open(root.path()).unwrap();
                engine.capture_epoch = epoch;
                engine
                    .register_configured_server(lsp::ConfiguredServer {
                        server: server.clone(),
                        configuration,
                    })
                    .unwrap();
                engine
            };
            let same = Session::open(
                registered_engine(original_configuration.clone()),
                HostProfile::default(),
                Some("symbol-move-own-v1"),
                &Deadline::new(30000),
            )
            .expect("Identical root, server arguments and host profile must resume");
            assert_eq!(session_bytes(&same.engine), before);
            drop(same);

            let mut changed_configuration = original_configuration;
            changed_configuration.refactor_intent_profile = if own {
                None
            } else {
                Some(lsp::RefactorIntentProfile::OwnV1)
            };
            let error = Session::open(
                registered_engine(changed_configuration),
                HostProfile::default(),
                Some("symbol-move-own-v1"),
                &Deadline::new(30000),
            )
            .err()
            .expect("Changing only the own_v1 opt-in must invalidate the session binding");
            assert!(error.to_string().contains("Session host binding changed"));
            assert_eq!(session_bytes(&Engine::open(root.path()).unwrap()), before);
            assert!(
                !log.path().join("protocol.log").exists(),
                "Opening or refusing a session must not launch the identical configured provider"
            );
            assert_eq!(
                fs::read_to_string(root.path().join("src/source.rs")).unwrap(),
                SOURCE
            );
            assert_eq!(
                fs::read_to_string(root.path().join("src/destination.rs")).unwrap(),
                DESTINATION
            );
        }
    }
}

#[test]
fn host_and_capability_must_both_enable_the_owned_move_protocol() {
    let mut f = fixture(AnalyzerEpoch::V8, "valid", false);
    assert_refused(&mut f, Some("bucket"));
    assert!(
        requests(&f).is_empty(),
        "Absent host opt-in must refuse before provider launch"
    );
    assert!(
        !requests(&f)
            .iter()
            .any(|r| r["method"] == "yoke/refactorIntent")
    );
    for scenario in ["missing_capability", "bad_capability_version"] {
        let mut f = fixture(AnalyzerEpoch::V8, scenario, true);
        assert_refused(&mut f, Some("bucket"));
        assert!(
            !requests(&f)
                .iter()
                .any(|r| r["method"] == "yoke/refactorIntent")
        );
    }
}

#[test]
fn owned_symbol_move_preserves_exact_declaration_and_actual_destination_parent_for_v8_v9() {
    for epoch in [AnalyzerEpoch::V8, AnalyzerEpoch::V9] {
        for (parent, expected, kind) in [
            (None, FILE_TOP_RESULT, "fileTopLevel"),
            (Some("bucket"), PARENT_RESULT, "parentSymbol"),
        ] {
            let mut f = fixture(epoch, "valid", true);
            let value = invoke(&mut f, parent)
                .expect("Exact supported source/destination move must prepare");
            assert_prepared(&f, &value, REMAINING, expected, epoch);
            assert_originals(&f, SOURCE, DESTINATION);
            let transcript = requests(&f);
            let request: Vec<_> = transcript
                .iter()
                .filter(|r| r["method"] == "yoke/refactorIntent")
                .collect();
            assert_eq!(request.len(), 1);
            let params = &request[0]["params"];
            assert_eq!(params["schemaVersion"], 1);
            assert_eq!(params["base"]["snapshotId"], value["snapshotId"]);
            let base = f
                .session
                .engine
                .store
                .snapshot(value["snapshotId"].as_str().unwrap())
                .unwrap();
            let graph = f.session.engine.store.graph(&base.id).unwrap();
            assert_eq!(params["base"]["workspaceId"], base.workspace_id);
            assert_eq!(params["base"]["manifestHash"], yoke::model::identity(&base));
            assert_eq!(
                params["base"]["serverProfileHash"],
                value["serverProfileHash"]
            );
            assert_eq!(params["base"]["analyzerProducer"], epoch.producer());
            assert_eq!(params["intent"]["operation"], "moveSymbol");
            assert_eq!(params["intent"]["destination"]["kind"], kind);
            let source = &params["intent"]["source"];
            let selected: Vec<_> = graph
                .entities
                .iter()
                .filter(|entity| entity.path == "src/source.rs" && entity.name == "helper")
                .collect();
            assert_eq!(selected.len(), 1);
            assert_graph_symbol_anchor(source, selected[0]);
            assert_eq!(source["snapshotId"], base.id);
            assert_eq!(source["path"], "src/source.rs");
            assert_eq!(source["canonicalNamePath"], "helper");
            assert_eq!(source["contentHash"], yoke::model::hash(SOURCE));
            let start = source["startByte"].as_u64().unwrap() as usize;
            let end = source["endByte"].as_u64().unwrap() as usize;
            assert_eq!(&SOURCE[start..end], "pub fn helper() -> u8 { 7 }");
            let ns = source["nameStartByte"].as_u64().unwrap() as usize;
            let ne = source["nameEndByte"].as_u64().unwrap() as usize;
            assert_eq!(&SOURCE[ns..ne], "helper");
            let destination = &params["intent"]["destination"];
            let file = if parent.is_some() {
                &destination["parent"]
            } else {
                &destination["file"]
            };
            assert_eq!(file["snapshotId"], base.id);
            assert_eq!(file["path"], "src/destination.rs");
            assert_eq!(file["contentHash"], yoke::model::hash(DESTINATION));
            if parent.is_some() {
                let selected_parent: Vec<_> = graph
                    .entities
                    .iter()
                    .filter(|entity| entity.path == "src/destination.rs" && entity.name == "bucket")
                    .collect();
                assert_eq!(selected_parent.len(), 1);
                assert_graph_symbol_anchor(file, selected_parent[0]);
                assert_eq!(file["canonicalNamePath"], "bucket");
                let start = destination["bodyStartByte"].as_u64().unwrap() as usize;
                let end = destination["bodyEndByte"].as_u64().unwrap() as usize;
                assert_eq!(&DESTINATION[start..end], "\npub fn sentinel() {}\n");
            }
            assert_fixture_response(&f, "valid", "edit");
            assert!(
                !transcript
                    .iter()
                    .any(|r| r["method"] == "workspace/executeCommand")
            );
        }
    }
}

#[test]
fn unsupported_ack_command_wrong_echo_and_unknown_response_fields_publish_nothing() {
    for scenario in [
        "unsupported",
        "ack_only",
        "command_only",
        "wrong_request_id",
        "wrong_schema",
        "extra_response_field",
    ] {
        let mut f = fixture(AnalyzerEpoch::V8, scenario, true);
        assert_refused(&mut f, Some("bucket"));
        assert_fixture_response(
            &f,
            scenario,
            match scenario {
                "unsupported" => "unsupported",
                "ack_only" | "command_only" => "other",
                _ => "edit",
            },
        );
        assert_eq!(
            requests(&f)
                .iter()
                .filter(|r| r["method"] == "yoke/refactorIntent")
                .count(),
            1,
            "This rejection must reach the causal provider response, not fail earlier admission"
        );
    }
}

#[test]
fn same_line_unicode_and_crlf_keep_original_byte_and_utf16_coordinates_distinct() {
    let before = "/* caf\u{00e9} \u{1f642} */ pub fn helper() -> u8 { 7 }\r\n";
    let remaining = "/* caf\u{00e9} \u{1f642} */ \r\n";
    let mut f = fixture(AnalyzerEpoch::V9, "valid", true);
    fs::write(f.root.path().join("src/source.rs"), before).unwrap();
    let value = invoke(&mut f, None).unwrap();
    assert_prepared(&f, &value, remaining, FILE_TOP_RESULT, AnalyzerEpoch::V9);
    assert_originals(&f, before, DESTINATION);
    let transcript = requests(&f);
    let request = transcript
        .iter()
        .find(|r| r["method"] == "yoke/refactorIntent")
        .unwrap();
    let source = &request["params"]["intent"]["source"];
    let start = source["startByte"].as_u64().unwrap() as usize;
    let end = source["endByte"].as_u64().unwrap() as usize;
    assert_eq!(&before[start..end], "pub fn helper() -> u8 { 7 }");
}

#[test]
fn top_level_scope_preserves_valid_destination_preambles_and_existing_outer_attachments() {
    for destination in [
        "\u{feff}pub mod bucket {\npub fn sentinel() {}\n}\n",
        "#!/usr/bin/env rust-script\npub mod bucket {\npub fn sentinel() {}\n}\n",
        "//! Destination docs\n#![allow(dead_code)]\npub mod bucket {\npub fn sentinel() {}\n}\n",
        "\u{feff}#!/usr/bin/env rust-script\n//! Destination docs\n#![allow(dead_code)]\n/// Sentinel docs\n#[inline]\npub fn sentinel() {}\n",
        "/// Sentinel docs\n#[inline]\npub fn sentinel() {}\n",
        "pub const NOTE: &str = \"#![allow(dead_code)] //! inner /// outer\";\npub mod bucket {}\n",
    ] {
        let mut f = fixture(AnalyzerEpoch::V9, "valid", true);
        fs::write(f.root.path().join("src/destination.rs"), destination).unwrap();
        let value = invoke(&mut f, None)
            .expect("Valid preambles remain at the front of the top-level scope");
        let expected = format!("{destination}\npub fn helper() -> u8 {{ 7 }}\n");
        assert_prepared(&f, &value, REMAINING, &expected, AnalyzerEpoch::V9);
        assert_originals(&f, SOURCE, destination);
        let transcript = requests(&f);
        let request = transcript
            .iter()
            .find(|r| r["method"] == "yoke/refactorIntent")
            .unwrap();
        assert_eq!(
            request["params"]["intent"]["destination"]["kind"],
            "fileTopLevel"
        );
    }
}

#[test]
fn actual_inline_module_body_preserves_inner_preamble_and_existing_item_attachments() {
    let destination = "//! Root docs\n#![allow(dead_code)]\n/// Bucket docs\n#[allow(dead_code)]\npub mod bucket {\n//! Module docs\n#![allow(dead_code)]\n/// Sentinel docs\n#[inline]\npub fn sentinel() {}\n}\n";
    let expected = "//! Root docs\n#![allow(dead_code)]\n/// Bucket docs\n#[allow(dead_code)]\npub mod bucket {\n//! Module docs\n#![allow(dead_code)]\n/// Sentinel docs\n#[inline]\npub fn sentinel() {}\n\npub fn helper() -> u8 { 7 }\n}\n";
    let mut f = fixture(AnalyzerEpoch::V8, "valid", true);
    fs::write(f.root.path().join("src/destination.rs"), destination).unwrap();
    let value =
        invoke(&mut f, Some("bucket")).expect("Destination must be the real inline module body");
    assert_prepared(&f, &value, REMAINING, expected, AnalyzerEpoch::V8);
    assert_originals(&f, SOURCE, destination);
}

#[test]
fn dangling_outer_attachments_and_malformed_destination_preambles_refuse_before_provider() {
    for (destination, parent) in [
        ("pub fn sentinel() {}\n/// Pending docs\n", None),
        ("pub fn sentinel() {}\n#[allow(dead_code)]\n", None),
        ("pub mod bucket {\n/// Pending docs\n}\n", Some("bucket")),
        ("pub mod bucket {\n#[allow(dead_code)]\n}\n", Some("bucket")),
        ("#![allow(dead_code)\npub fn sentinel() {}\n", None),
        ("pub fn sentinel() {}\n#!/usr/bin/env rust-script\n", None),
        (
            "pub fn sentinel() {}\n#![allow(dead_code)]\npub mod bucket {}\n",
            None,
        ),
        (
            "pub fn sentinel() {}\n//! Late inner docs\npub mod bucket {}\n",
            None,
        ),
        (
            "pub mod bucket {\npub fn sentinel() {}\n#![allow(dead_code)]\npub fn later() {}\n}\n",
            Some("bucket"),
        ),
        (
            "pub mod bucket {\npub fn sentinel() {}\n//! Late inner docs\npub fn later() {}\n}\n",
            Some("bucket"),
        ),
    ] {
        let mut f = fixture(AnalyzerEpoch::V9, "valid", true);
        fs::write(f.root.path().join("src/destination.rs"), destination).unwrap();
        let value = invoke(&mut f, parent);
        assert!(value.is_err());
        assert_eq!(objects(&f), (0, 0));
        assert!(
            requests(&f).is_empty(),
            "Unsafe syntax attachment must refuse before launch"
        );
        assert_originals(&f, SOURCE, destination);
    }
}

#[test]
fn functions_impls_and_traits_are_not_guessed_inline_module_destinations() {
    for (destination, parent) in [
        ("pub fn bucket() {}\n", "bucket"),
        ("pub trait Bucket {}\n", "Bucket"),
        ("pub struct Bucket;\nimpl Bucket {}\n", "Bucket"),
    ] {
        let mut f = fixture(AnalyzerEpoch::V8, "valid", true);
        fs::write(f.root.path().join("src/destination.rs"), destination).unwrap();
        let value = invoke(&mut f, Some(parent));
        assert!(value.is_err());
        assert_eq!(objects(&f), (0, 0));
        assert!(requests(&f).is_empty());
        assert_originals(&f, SOURCE, destination);
    }
}

#[test]
fn final_bytes_or_unrelated_changes_cannot_replace_isolated_original_operation_evidence() {
    for (scenario, reason) in [
        ("noop", "exact source cut required"),
        ("copy_only", "exact source cut required"),
        ("delete_only", "exact declaration insertion required"),
        ("body_change", "exact declaration insertion required"),
        ("wrong_parent", "insertion destination differs"),
        ("whole_file", "exact source cut required"),
        ("resource", "resource operations are unsupported"),
    ] {
        let mut f = fixture(AnalyzerEpoch::V9, scenario, true);
        let error = invoke(&mut f, Some("bucket")).unwrap_err().to_string();
        assert!(
            error.contains(&format!("Move effect rejected: {reason}")),
            "Scenario {scenario} did not reach its causal effect rule: {error}"
        );
        assert_eq!(objects(&f), (0, 0));
        assert_originals(&f, SOURCE, DESTINATION);
        assert_fixture_response(&f, scenario, "edit");
        assert_eq!(
            requests(&f)
                .iter()
                .filter(|r| r["method"] == "yoke/refactorIntent")
                .count(),
            1,
            "The malformed effect must be checked after a real owned-protocol response"
        );
        assert!(
            !requests(&f)
                .iter()
                .any(|row| row["method"] == "workspace/executeCommand")
        );
    }
    // Version 2 is rejected by LSP staging, before native effect admission.
    let mut f = fixture(AnalyzerEpoch::V9, "virtual", true);
    assert_refused(&mut f, Some("bucket"));
    assert_fixture_response(&f, "virtual", "edit");
    assert_eq!(
        requests(&f)
            .iter()
            .filter(|row| row["method"] == "yoke/refactorIntent")
            .count(),
        1
    );
}

#[test]
fn moving_one_overload_cannot_remove_a_different_same_name_declaration() {
    // Syntax fixtures preserve source-order overload selection; no Rust compiler is claimed.
    let overloads = "pub fn helper() -> u8 { 7 }\npub fn helper(value: u8) -> u8 { value }\n";
    for scenario in ["valid", "wrong_overload"] {
        let mut f = fixture(AnalyzerEpoch::V9, scenario, true);
        fs::write(f.root.path().join("src/source.rs"), overloads).unwrap();
        let result = yoke::agent_repl::invoke_facade(
            &mut f.session,
            "jb.move",
            json!({"relative_path":"src/source.rs","name_path":"helper[1]","target_relative_path":"src/destination.rs","target_parent_name_path":"bucket"}),
            None,
            &Deadline::new(30000),
        );
        if scenario == "wrong_overload" {
            let error = result.unwrap_err().to_string();
            assert!(
                error.contains("Move effect rejected: exact source cut required"),
                "Wrong overload must reach the source-cut effect rule: {error}"
            );
            assert_eq!(objects(&f), (0, 0));
        } else {
            let value = result.unwrap();
            assert_prepared(
                &f,
                &value,
                "pub fn helper() -> u8 { 7 }\n\n",
                "pub mod bucket {\npub fn sentinel() {}\n\npub fn helper(value: u8) -> u8 { value }\n}\n",
                AnalyzerEpoch::V9,
            );
        }
        assert_originals(&f, overloads, DESTINATION);
        let transcript = requests(&f);
        let request = transcript
            .iter()
            .find(|r| r["method"] == "yoke/refactorIntent")
            .unwrap();
        let graph = f
            .session
            .engine
            .store
            .graph(request["params"]["base"]["snapshotId"].as_str().unwrap())
            .unwrap();
        let selected = graph
            .entities
            .iter()
            .find(|entity| {
                entity.path == "src/source.rs"
                    && entity.name == "helper"
                    && entity.start_byte == overloads.find("pub fn helper(value").unwrap()
            })
            .unwrap();
        assert_graph_symbol_anchor(&request["params"]["intent"]["source"], selected);
        assert_fixture_response(&f, scenario, "edit");
        assert_eq!(
            request["params"]["intent"]["source"]["canonicalNamePath"],
            "helper[1]"
        );
    }
}

#[test]
fn same_file_symbol_move_uses_one_original_document_and_the_real_inline_parent() {
    let before = "pub fn helper() -> u8 { 7 }\npub mod bucket {\npub fn sentinel() {}\n}\n";
    let after = "\npub mod bucket {\npub fn sentinel() {}\n\npub fn helper() -> u8 { 7 }\n}\n";
    let mut f = fixture(AnalyzerEpoch::V9, "valid", true);
    fs::write(f.root.path().join("src/source.rs"), before).unwrap();
    let value = yoke::agent_repl::invoke_facade(&mut f.session, "jb.move",
        json!({"relative_path":"src/source.rs","name_path":"helper","target_relative_path":"src/source.rs","target_parent_name_path":"bucket"}),
        None, &Deadline::new(30000)).unwrap();
    let plan = &value["plan"];
    let snapshot = f
        .session
        .engine
        .store
        .snapshot(plan["resultSnapshotId"].as_str().unwrap())
        .unwrap();
    assert_eq!(
        yoke::query::source(&f.session.engine.store, &snapshot, "src/source.rs")
            .unwrap()
            .0,
        after
    );
    assert_eq!(plan["files"].as_array().unwrap().len(), 1);
    assert_eq!(objects(&f), (1, 1));
    assert_originals(&f, before, DESTINATION);
    let transcript = requests(&f);
    let request = transcript
        .iter()
        .find(|r| r["method"] == "yoke/refactorIntent")
        .unwrap();
    assert_eq!(
        request["params"]["documents"]["sourceUri"],
        request["params"]["documents"]["destinationUri"]
    );
}

#[test]
fn an_inline_destination_inside_the_moved_declaration_refuses_before_provider() {
    let mut f = fixture(AnalyzerEpoch::V9, "valid", true);
    let source = "pub fn container() {\nmod nested {}\n}\n";
    fs::write(f.root.path().join("src/source.rs"), source).unwrap();
    let result = yoke::agent_repl::invoke_facade(
        &mut f.session,
        "jb.move",
        json!({"relative_path":"src/source.rs","name_path":"container","target_relative_path":"src/source.rs","target_parent_name_path":"container/nested"}),
        None,
        &Deadline::new(30000),
    );
    assert!(result.is_err());
    assert_eq!(objects(&f), (0, 0));
    assert!(requests(&f).is_empty());
    assert_originals(&f, source, DESTINATION);
}

#[test]
fn unsupported_missing_directory_and_empty_destination_contracts_refuse_before_provider() {
    for destination in [
        json!({}),
        json!({"target_relative_path":null}),
        json!({"target_relative_path":"src"}),
        json!({"target_relative_path":"src/not-captured.rs"}),
        json!({"target_relative_path":"src/destination.rs","target_parent_name_path":""}),
        json!({"target_relative_path":"../outside.rs"}),
    ] {
        let mut f = fixture(AnalyzerEpoch::V8, "valid", true);
        let mut args = json!({"relative_path":"src/source.rs","name_path":"helper"});
        args.as_object_mut()
            .unwrap()
            .extend(destination.as_object().unwrap().clone());
        let value = yoke::agent_repl::invoke_facade(
            &mut f.session,
            "jb.move",
            args,
            None,
            &Deadline::new(30000),
        );
        assert!(value.is_err());
        assert_eq!(objects(&f), (0, 0));
        assert!(requests(&f).is_empty());
        assert_originals(&f, SOURCE, DESTINATION);
    }
}

#[test]
fn attached_documentation_or_attributes_are_not_silently_left_on_the_next_item() {
    for before in [
        "/// Selected documentation\npub fn helper() -> u8 { 7 }\n",
        "#[inline]\npub fn helper() -> u8 { 7 }\n",
        "/** Selected documentation */\npub fn helper() -> u8 { 7 }\n",
        "#[allow(\n dead_code\n)]\npub fn helper() -> u8 { 7 }\n",
        "/// Selected documentation\n\n// Ordinary trivia\n\npub fn helper() -> u8 { 7 }\n",
        "/** Selected documentation */\n/* Ordinary trivia */\n\npub fn helper() -> u8 { 7 }\n",
        "#[inline]\n// Ordinary trivia\n\npub fn helper() -> u8 { 7 }\n",
    ] {
        let mut f = fixture(AnalyzerEpoch::V8, "missing_attachment_capability", true);
        fs::write(f.root.path().join("src/source.rs"), before).unwrap();
        let value = invoke(&mut f, Some("bucket"));
        assert!(value.is_err());
        assert_eq!(objects(&f), (0, 0));
        let log = requests(&f);
        assert!(log.iter().any(|r| r["method"] == "initialize"));
        assert!(!log.iter().any(|r| r["method"] == "yoke/refactorIntent"));
        assert_originals(&f, before, DESTINATION);
    }
}

#[test]
fn symbol_move_mode_and_external_context_denials_precede_provider_work() {
    let mut f = fixture(AnalyzerEpoch::V8, "valid", true);
    f.session
        .invoke(
            "switchModes",
            &json!({"modes":["planning"]}),
            &Deadline::new(30000),
        )
        .unwrap();
    assert_refused(&mut f, Some("bucket"));
    assert!(requests(&f).is_empty());
    let mut f = fixture(AnalyzerEpoch::V8, "valid", true);
    let value = yoke::agent_repl::invoke_facade(
        &mut f.session,
        "jb.move",
        json!({"relative_path":"src/source.rs","name_path":"helper","target_relative_path":"src/destination.rs","target_parent_name_path":"bucket"}),
        Some("default"),
        &Deadline::new(30000),
    );
    assert!(value.is_err());
    assert_eq!(objects(&f), (0, 0));
    assert!(requests(&f).is_empty());
    assert_originals(&f, SOURCE, DESTINATION);
}

#[test]
fn captured_source_scope_cannot_be_widened_by_an_owned_provider_profile() {
    for (options, source, destination) in [
        (
            CaptureOptions {
                only_dirs: vec!["src".into()],
                ..Default::default()
            },
            "src/source.rs",
            "outside/destination.rs",
        ),
        (
            CaptureOptions {
                read_only_dependency_dirs: vec!["node_modules".into()],
                ..Default::default()
            },
            "src/source.rs",
            "node_modules/sdk/destination.rs",
        ),
        (
            CaptureOptions {
                read_only_dependency_dirs: vec!["node_modules".into()],
                ..Default::default()
            },
            "node_modules/sdk/source.rs",
            "src/destination.rs",
        ),
        (
            CaptureOptions {
                include_ignored_dirs: vec!["ignored".into()],
                ..Default::default()
            },
            "src/source.rs",
            "ignored/destination.rs",
        ),
        (
            CaptureOptions {
                include_ignored_dirs: vec!["ignored".into()],
                ..Default::default()
            },
            "ignored/source.rs",
            "src/destination.rs",
        ),
    ] {
        // A real configured own-v1 fixture exists; absence of a server cannot explain refusal.
        let mut f = fixture_with_options(AnalyzerEpoch::V9, "valid", true, options);
        let value = yoke::agent_repl::invoke_facade(
            &mut f.session,
            "jb.move",
            json!({"relative_path":source,"name_path":"helper","target_relative_path":destination,"target_parent_name_path":"bucket"}),
            None,
            &Deadline::new(30000),
        );
        assert!(value.is_err());
        assert_eq!(objects(&f), (0, 0));
        assert!(requests(&f).is_empty());
        assert_originals(&f, SOURCE, DESTINATION);
        assert_eq!(
            fs::read_to_string(f.root.path().join(source)).unwrap(),
            SOURCE
        );
        assert_eq!(
            fs::read_to_string(f.root.path().join(destination)).unwrap(),
            DESTINATION
        );
    }
}
