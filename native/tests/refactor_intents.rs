use serde_json::{Value, json};
use std::{fs, path::Path};
use yoke::{
    Engine,
    capture::Deadline,
    lsp,
    project_session::{HostProfile, Session},
};

const RENAME_SOURCE: &str = "// 🌍 helper helper_more\nfn helper() {}\nconst LABEL: &str = \"helper helper_more\";\nfn caller() { helper(); }\n";
const INLINE_SOURCE: &str = "fn helper() -> i32 { 7 }\nfn caller() { let answer = helper(); }\n";

fn fixture(source: &str, mode: &str) -> (tempfile::TempDir, tempfile::TempDir, Session) {
    fixture_at(source, mode, yoke::analysis::AnalyzerEpoch::V8, false)
}

fn fixture_at(
    source: &str,
    mode: &str,
    epoch: yoke::analysis::AnalyzerEpoch,
    readonly: bool,
) -> (tempfile::TempDir, tempfile::TempDir, Session) {
    let root = tempfile::tempdir().unwrap();
    let log = tempfile::tempdir().unwrap();
    fs::write(root.path().join("main.rs"), source).unwrap();
    if readonly {
        fs::write(root.path().join(".gitignore"), "ignored/\n!node_modules/\n").unwrap();
        for folder in ["node_modules/sdk", "ignored"] {
            fs::create_dir_all(root.path().join(folder)).unwrap();
            fs::write(
                root.path().join(folder).join("dep.rs"),
                "// helper\nconst LABEL: &str = \"helper\";\n",
            )
            .unwrap();
        }
    }
    let mut engine = Engine::open(root.path()).unwrap();
    engine.capture_epoch = epoch;
    if readonly {
        engine.capture_options.read_only_dependency_dirs = vec!["node_modules".into()];
        engine.capture_options.include_ignored_dirs = vec!["ignored".into()];
    }
    engine
        .register_configured_server(lsp::ConfiguredServer {
            server: lsp::Server {
                command: "node".into(),
                args: vec![
                    Path::new(env!("CARGO_MANIFEST_DIR"))
                        .join("tests/fixtures/refactor-intents-lsp.mjs")
                        .display()
                        .to_string(),
                    mode.into(),
                    log.path().join("protocol.log").display().to_string(),
                ],
                language: "rust".into(),
                trust: true,
            },
            configuration: lsp::HostConfiguration {
                passive_profile: Some(lsp::PassiveProfile::ContractFixture),
                ..Default::default()
            },
        })
        .unwrap();
    let session = Session::open(
        engine,
        HostProfile::default(),
        Some("refactor-intents"),
        &Deadline::new(30000),
    )
    .unwrap();
    (root, log, session)
}

fn plan_count(session: &Session) -> usize {
    session
        .engine
        .store
        .db
        .query_row(
            "SELECT COUNT(*) FROM records WHERE kind IN ('plan','approval','receipt')",
            [],
            |r| r.get(0),
        )
        .unwrap()
}

fn derived_count(session: &Session) -> usize {
    session
        .engine
        .store
        .db
        .query_row(
            "SELECT COUNT(*) FROM snapshots WHERE json_extract(value,'$.kind')='derived'",
            [],
            |r| r.get(0),
        )
        .unwrap()
}

fn result_text(session: &Session, plan: &Value) -> String {
    let result = session
        .engine
        .store
        .snapshot(plan["resultSnapshotId"].as_str().unwrap())
        .unwrap();
    yoke::query::source(&session.engine.store, &result, "main.rs")
        .unwrap()
        .0
}

#[test]
fn invalid_later_inline_candidate_cannot_leave_an_earlier_hidden_plan_or_derivative() {
    let (root, log, mut session) = fixture(INLINE_SOURCE, "inline-batch-bad");
    let result = yoke::agent_repl::invoke_facade(
        &mut session,
        "jb.inline_symbol",
        json!({"relative_path":"main.rs","name_path":"helper"}),
        None,
        &Deadline::new(30000),
    );
    assert!(
        result.is_err(),
        "invalid WorkspaceEdit batch accepted: {result:?}"
    );
    assert_eq!(
        plan_count(&session),
        0,
        "a rejected action batch published a hidden plan"
    );
    assert_eq!(
        derived_count(&session),
        0,
        "a rejected action batch published a derived source graph"
    );
    assert_eq!(
        fs::read_to_string(root.path().join("main.rs")).unwrap(),
        INLINE_SOURCE
    );
    assert!(
        !fs::read_to_string(log.path().join("protocol.log"))
            .unwrap()
            .contains("executeCommand")
    );
}

#[test]
fn deterministic_later_noop_nul_or_secret_candidate_is_rejected_before_any_publication() {
    for mode in [
        "inline-batch-noop",
        "inline-batch-nul",
        "inline-batch-secret",
    ] {
        let (_root, _log, mut session) = fixture(INLINE_SOURCE, mode);
        let result = yoke::agent_repl::invoke_facade(
            &mut session,
            "jb.inline_symbol",
            json!({"relative_path":"main.rs","name_path":"helper"}),
            None,
            &Deadline::new(30000),
        );
        assert!(result.is_err(), "{mode}: {result:?}");
        assert_eq!(plan_count(&session), 0, "{mode} left a hidden plan");
        assert_eq!(derived_count(&session), 0, "{mode} left a derived graph");
    }
}

#[test]
fn amplified_candidate_contents_are_bounded_before_any_publication() {
    let source = format!("{INLINE_SOURCE}//{}\n", "x".repeat(1_100_000));
    let (_root, _log, mut session) = fixture(&source, "inline-amplified");
    let result = yoke::agent_repl::invoke_facade(
        &mut session,
        "jb.inline_symbol",
        json!({"relative_path":"main.rs","name_path":"helper"}),
        None,
        &Deadline::new(30000),
    );
    let error = result.expect_err("expanded candidate contents were not bounded");
    assert!(error.to_string().contains("capacity"), "{error:#}");
    assert_eq!(plan_count(&session), 0);
    assert_eq!(derived_count(&session), 0);
}

#[test]
fn requested_comment_and_text_rename_options_change_only_their_own_occurrence_classes() {
    for (comments, text) in [(false, false), (true, false), (false, true), (true, true)] {
        let (root, log, mut session) = fixture(RENAME_SOURCE, "rename");
        let value = yoke::agent_repl::invoke_facade(
            &mut session,
            "jb.rename",
            json!({
                "relative_path":"main.rs", "name_path":"helper", "new_name":"renamed",
                "rename_in_comments":comments, "rename_in_text_occurrences":text,
            }),
            None,
            &Deadline::new(30000),
        )
        .unwrap();
        let plan = &value["plan"];
        assert!(plan["id"].is_string(), "{value}");
        let expected = format!(
            "// 🌍 {} helper_more\nfn renamed() {{}}\nconst LABEL: &str = \"{} helper_more\";\nfn caller() {{ renamed(); }}\n",
            if comments { "renamed" } else { "helper" },
            if text { "renamed" } else { "helper" }
        );
        assert_eq!(
            result_text(&session, plan),
            expected,
            "comments={comments} text={text}"
        );
        assert_eq!(plan_count(&session), 1);
        assert_eq!(
            fs::read_to_string(root.path().join("main.rs")).unwrap(),
            RENAME_SOURCE
        );
        assert!(
            !fs::read_to_string(log.path().join("protocol.log"))
                .unwrap()
                .contains("executeCommand")
        );
    }
}

#[test]
fn inline_keep_definition_preserves_exact_declaration_bytes_while_replacing_a_call() {
    let (root, log, mut session) = fixture(INLINE_SOURCE, "inline-keep");
    let value = yoke::agent_repl::invoke_facade(
        &mut session,
        "jb.inline_symbol",
        json!({
            "relative_path":"main.rs", "name_path":"helper", "keep_definition":true,
        }),
        None,
        &Deadline::new(30000),
    )
    .unwrap();
    assert_eq!(value["actions"].as_array().unwrap().len(), 1, "{value}");
    let plan = &value["actions"][0]["plan"];
    assert_eq!(
        result_text(&session, plan),
        "fn helper() -> i32 { 7 }\nfn caller() { let answer = (7); }\n"
    );
    assert_eq!(plan_count(&session), 1);
    assert_eq!(
        fs::read_to_string(root.path().join("main.rs")).unwrap(),
        INLINE_SOURCE
    );
    assert!(
        !fs::read_to_string(log.path().join("protocol.log"))
            .unwrap()
            .contains("executeCommand")
    );
}

#[test]
fn flagged_refactoring_obeys_session_plan_permissions_before_querying_a_provider() {
    let (_root, log, mut session) = fixture(RENAME_SOURCE, "rename");
    let d = Deadline::new(30000);
    session
        .invoke("switchModes", &json!({"modes":["planning"]}), &d)
        .unwrap();
    assert!(yoke::agent_repl::invoke_facade(&mut session, "jb.rename", json!({
        "relative_path":"main.rs", "name_path":"helper", "new_name":"renamed", "rename_in_comments":true,
    }), None, &d).is_err());
    assert_eq!(plan_count(&session), 0);
    assert!(!log.path().join("protocol.log").exists());
}

#[test]
fn flagged_rename_rejects_unrelated_and_wholefile_provider_changes_before_publication() {
    for mode in ["rename-unrelated", "rename-unrelated-wholefile"] {
        let (_root, _log, mut session) = fixture(RENAME_SOURCE, mode);
        let result = yoke::agent_repl::invoke_facade(
            &mut session,
            "jb.rename",
            json!({"relative_path":"main.rs","name_path":"helper","new_name":"renamed","rename_in_comments":true}),
            None,
            &Deadline::new(30000),
        );
        assert!(
            result.is_err(),
            "{mode} did not rename the selected source name: {result:?}"
        );
        assert_eq!(plan_count(&session), 0, "{mode}");
        assert_eq!(derived_count(&session), 0, "{mode}");
    }
}

#[test]
fn keep_definition_rejects_broad_ambiguous_and_command_candidates_without_plans() {
    for mode in ["inline-wholefile", "inline-ambiguous", "inline-command"] {
        let (_root, _log, mut session) = fixture(INLINE_SOURCE, mode);
        let result = yoke::agent_repl::invoke_facade(
            &mut session,
            "jb.inline_symbol",
            json!({"relative_path":"main.rs","name_path":"helper","keep_definition":true}),
            None,
            &Deadline::new(30000),
        );
        assert!(result.is_err(), "{mode}: {result:?}");
        assert_eq!(plan_count(&session), 0, "{mode}");
        assert_eq!(derived_count(&session), 0, "{mode}");
    }
}

#[test]
fn native_occurrences_distinguish_string_comment_markers_and_unicode_word_boundaries() {
    let source = "// helper helper_ helperé helper\u{0301}\nfn helper() {}\nconst LABEL: &str = \"// helper helper_ helperé helper\u{0301}\";\nfn caller() { helper(); }\n";
    for (comments, text) in [(true, false), (false, true)] {
        let (_root, _log, mut session) = fixture(source, "rename");
        let value=yoke::agent_repl::invoke_facade(&mut session,"jb.rename",json!({"relative_path":"main.rs","name_path":"helper","new_name":"renamed","rename_in_comments":comments,"rename_in_text_occurrences":text}),None,&Deadline::new(30000)).unwrap();
        let expected = format!(
            "// {} helper_ helperé helper\u{0301}\nfn renamed() {{}}\nconst LABEL: &str = \"// {} helper_ helperé helper\u{0301}\";\nfn caller() {{ renamed(); }}\n",
            if comments { "renamed" } else { "helper" },
            if text { "renamed" } else { "helper" }
        );
        assert_eq!(result_text(&session, &value["plan"]), expected);
    }
}

#[test]
fn native_text_occurrences_refuse_potentially_interpolated_strings_before_publication() {
    let source = "fn helper() {}\nconst LABEL: &str = \"{helper}\";\nfn caller() { helper(); }\n";
    let (_root, _log, mut session) = fixture(source, "rename");
    let result = yoke::agent_repl::invoke_facade(
        &mut session,
        "jb.rename",
        json!({"relative_path":"main.rs","name_path":"helper","new_name":"renamed","rename_in_text_occurrences":true}),
        None,
        &Deadline::new(30000),
    );
    assert!(result.is_err(), "{result:?}");
    assert_eq!(plan_count(&session), 0);
    assert_eq!(derived_count(&session), 0);
}

#[test]
fn projected_refactors_keep_v8_v9_source_proofs_and_never_edit_readonly_or_ignored_sources() {
    use yoke::{analysis::AnalyzerEpoch, capture};
    for epoch in [AnalyzerEpoch::V8, AnalyzerEpoch::V9] {
        let (_root, _log, mut session) = fixture_at(RENAME_SOURCE, "rename", epoch, true);
        let value=yoke::agent_repl::invoke_facade(&mut session,"jb.rename",json!({"relative_path":"main.rs","name_path":"helper","new_name":"renamed","rename_in_comments":true,"rename_in_text_occurrences":true}),None,&Deadline::new(30000)).unwrap();
        let result = session
            .engine
            .store
            .snapshot(value["plan"]["resultSnapshotId"].as_str().unwrap())
            .unwrap();
        assert_eq!(
            capture::analyzer_epoch_for_snapshot(&session.engine.store, &result).unwrap(),
            epoch
        );
        for path in ["node_modules/sdk/dep.rs", "ignored/dep.rs"] {
            assert_eq!(
                yoke::query::source(&session.engine.store, &result, path)
                    .unwrap()
                    .0,
                "// helper\nconst LABEL: &str = \"helper\";\n"
            );
            assert!(
                value["intentProjection"]["excludedSources"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|source| source["path"] == path)
            );
        }
        assert_eq!(plan_count(&session), 1);
        let (_root, _log, mut session) = fixture_at(INLINE_SOURCE, "inline-keep", epoch, false);
        let value = yoke::agent_repl::invoke_facade(
            &mut session,
            "jb.inline_symbol",
            json!({"relative_path":"main.rs","name_path":"helper","keep_definition":true}),
            None,
            &Deadline::new(30000),
        )
        .unwrap();
        let result = session
            .engine
            .store
            .snapshot(
                value["actions"][0]["plan"]["resultSnapshotId"]
                    .as_str()
                    .unwrap(),
            )
            .unwrap();
        assert_eq!(
            capture::analyzer_epoch_for_snapshot(&session.engine.store, &result).unwrap(),
            epoch
        );
        assert_eq!(
            result_text(&session, &value["actions"][0]["plan"]),
            "fn helper() -> i32 { 7 }\nfn caller() { let answer = (7); }\n"
        );
    }
}

#[test]
fn keeping_an_inline_definition_requires_a_real_source_bound_call_replacement() {
    let (_root, _log, mut session) = fixture(INLINE_SOURCE, "inline-unrelated");
    let result = yoke::agent_repl::invoke_facade(
        &mut session,
        "jb.inline_symbol",
        json!({"relative_path":"main.rs","name_path":"helper","keep_definition":true}),
        None,
        &Deadline::new(30000),
    );
    assert!(
        result.is_err(),
        "an unrelated change is not an inline replacement: {result:?}"
    );
    assert_eq!(plan_count(&session), 0);
    assert_eq!(derived_count(&session), 0);
}

#[test]
fn native_text_rename_never_rewrites_rust_raw_string_delimiters() {
    for (name, literal, expected_literal, mode) in [
        ("r", "r#\"r\"#", "r#\"renamed\"#", "rename-raw"),
        ("b", "b\"b\"", "b\"renamed\"", "rename-literal-b"),
        ("c", "c\"c\"", "c\"renamed\"", "rename-literal-c"),
    ] {
        let source = format!(
            "fn {name}() {{}}\nconst LABEL: &str = {literal};\nfn caller() {{ {name}(); }}\n"
        );
        let (_root, _log, mut session) = fixture(&source, mode);
        let value=yoke::agent_repl::invoke_facade(&mut session,"jb.rename",json!({"relative_path":"main.rs","name_path":name,"new_name":"renamed","rename_in_text_occurrences":true}),None,&Deadline::new(30000)).unwrap();
        assert_eq!(
            result_text(&session, &value["plan"]),
            format!(
                "fn renamed() {{}}\nconst LABEL: &str = {expected_literal};\nfn caller() {{ renamed(); }}\n"
            ),
            "{literal}"
        );
    }
}

#[test]
fn requested_false_occurrence_flags_also_constrain_provider_supplied_edits() {
    for (mode, comments, text) in [
        ("rename-extra-string", true, false),
        ("rename-extra-comment", false, true),
    ] {
        let (_root, _log, mut session) = fixture(RENAME_SOURCE, mode);
        let result = yoke::agent_repl::invoke_facade(
            &mut session,
            "jb.rename",
            json!({"relative_path":"main.rs","name_path":"helper","new_name":"renamed","rename_in_comments":comments,"rename_in_text_occurrences":text}),
            None,
            &Deadline::new(30000),
        );
        assert!(
            result.is_err(),
            "{mode} ignored a disabled occurrence class: {result:?}"
        );
        assert_eq!(plan_count(&session), 0, "{mode}");
        assert_eq!(derived_count(&session), 0, "{mode}");
    }
}

#[test]
fn native_unicode_rename_refuses_byte_string_occurrences_before_publication() {
    for literal in ["b\"helper\"", "br#\"helper\"#"] {
        let source = format!(
            "fn helper() {{}}\nconst LABEL: &[u8] = {literal};\nfn caller() {{ helper(); }}\n"
        );
        let (_root, _log, mut session) = fixture(&source, "rename");
        let result = yoke::agent_repl::invoke_facade(
            &mut session,
            "jb.rename",
            json!({"relative_path":"main.rs","name_path":"helper","new_name":"café","rename_in_text_occurrences":true}),
            None,
            &Deadline::new(30000),
        );
        assert!(
            result.is_err(),
            "native Unicode must not create invalid byte-string contents: {literal}: {result:?}"
        );
        assert_eq!(plan_count(&session), 0, "{literal}");
        assert_eq!(derived_count(&session), 0, "{literal}");
    }
}
