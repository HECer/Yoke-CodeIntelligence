//! Owned standard-LSP folder fixture, exact native effects and guarded real filesystem writes.
use serde_json::{Value, json};
use std::{fs, path::Path};
use yoke::{
    Engine,
    analysis::AnalyzerEpoch,
    capture::{CaptureOptions, Deadline},
    changes, lsp,
    project_session::{HostProfile, Session},
};
const BEFORE: &str = "pub fn original(){ /* café 🦀 */ }\r\n";
const AFTER: &str = "// provider prepared folder move\r\npub fn original(){ /* café 🦀 */ }\r\n";
fn d() -> Deadline {
    Deadline::new(30_000)
}
struct Fixture {
    session: Session,
    log: tempfile::TempDir,
    root: tempfile::TempDir,
}
fn fixture(epoch: AnalyzerEpoch, mode: &str) -> Fixture {
    let root = tempfile::tempdir().unwrap();
    let log = tempfile::tempdir().unwrap();
    fs::create_dir_all(root.path().join("src/tree/sub")).unwrap();
    fs::create_dir_all(root.path().join("src/tree/empty/nested")).unwrap();
    fs::create_dir(root.path().join("pkg.rs")).unwrap();
    fs::write(root.path().join("src/tree/a.rs"), BEFORE).unwrap();
    fs::write(
        root.path().join("src/tree/sub/café.rs"),
        "pub fn retained(){}\r\n",
    )
    .unwrap();
    fs::write(
        root.path().join("src/use.rs"),
        "// original folder reference\r\n",
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
                        .join("tests/fixtures/directory-move-lsp.mjs")
                        .display()
                        .to_string(),
                    log.path().join("protocol.log").display().to_string(),
                    mode.into(),
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
        Some("owned-directory-move"),
        &d(),
    )
    .unwrap();
    Fixture { session, log, root }
}
fn invoke(f: &mut Fixture, args: Value) -> anyhow::Result<Value> {
    yoke::agent_repl::invoke_facade(&mut f.session, "jb.move", args, None, &d())
}
fn records(f: &Fixture) -> (usize, usize) {
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
fn transcript(f: &Fixture) -> Vec<Value> {
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
fn unchanged(f: &Fixture) {
    assert_eq!(
        fs::read(f.root.path().join("src/tree/a.rs")).unwrap(),
        BEFORE.as_bytes()
    );
    assert!(f.root.path().join("src/tree/empty/nested").is_dir());
    assert!(!f.root.path().join("pkg.rs/tree").exists());
}
#[test]
fn omitted_null_empty_directory_selectors_bind_one_parent_basename_plan_for_both_epochs() {
    for epoch in [AnalyzerEpoch::V8, AnalyzerEpoch::V9] {
        for selector in [None, Some(Value::Null), Some(json!(""))] {
            let mut f = fixture(epoch, "edits");
            let mut args = json!({"relative_path":"src/tree","target_relative_path":"pkg.rs"});
            if let Some(selector) = selector {
                args["name_path"] = selector;
            }
            let value = invoke(&mut f, args).unwrap();
            let plan: &Value = &value["plan"];
            assert_eq!(value["equivalent"], "independent_captured_directory_move");
            assert_eq!(value["complete"], false);
            assert_eq!(value["commandsExecuted"], false);
            assert_eq!(plan["directoryMove"]["source"], "src/tree");
            assert_eq!(plan["directoryMove"]["destination"], "pkg.rs/tree");
            assert_eq!(records(&f), (1, 1));
            unchanged(&f);
            let result = f
                .session
                .engine
                .store
                .snapshot(plan["resultSnapshotId"].as_str().unwrap())
                .unwrap();
            assert_eq!(
                yoke::query::source(&f.session.engine.store, &result, "pkg.rs/tree/a.rs")
                    .unwrap()
                    .0,
                AFTER
            );
            assert_eq!(
                yoke::capture::analyzer_epoch_for_snapshot(&f.session.engine.store, &result)
                    .unwrap(),
                epoch
            );
            let requests = transcript(&f);
            let rename = requests
                .iter()
                .find(|r| r["method"] == "workspace/willRenameFiles")
                .unwrap();
            assert!(
                rename["params"]["files"][0]["oldUri"]
                    .as_str()
                    .unwrap()
                    .ends_with("/src/tree")
            );
            assert!(
                rename["params"]["files"][0]["newUri"]
                    .as_str()
                    .unwrap()
                    .ends_with("/pkg.rs/tree")
            );
            assert!(
                !requests
                    .iter()
                    .any(|r| r["method"] == "workspace/executeCommand")
            );
        }
    }
}
#[test]
fn native_directory_move_keeps_the_explicit_final_name_contract() {
    let mut f = fixture(AnalyzerEpoch::V9, "null");
    let value = yoke::semantic_refactor::invoke(
        &mut f.session.engine,
        "move",
        &json!({"relativePath":"src/tree","targetRelativePath":"pkg.rs/renamed"}),
        &d(),
    )
    .unwrap();
    assert_eq!(
        value["plan"]["directoryMove"]["destination"],
        "pkg.rs/renamed"
    );
    assert_eq!(records(&f), (1, 1));
    unchanged(&f);
}
#[test]
fn occupied_missing_protected_and_uncaptured_tree_entries_refuse_before_provider_start() {
    for case in [
        "occupied",
        "missing-parent",
        "protected-file",
        "uncaptured-binary",
        "missing-source",
        "descendant",
    ] {
        let mut f = fixture(AnalyzerEpoch::V8, "edits");
        let mut args = json!({"relative_path":"src/tree","target_relative_path":"pkg.rs"});
        match case {
            "occupied" => fs::create_dir(f.root.path().join("pkg.rs/tree")).unwrap(),
            "missing-parent" => args["target_relative_path"] = json!("absent"),
            "protected-file" => fs::write(
                f.root.path().join("src/tree/.env"),
                "OWNED_FIXTURE=not-a-secret\n",
            )
            .unwrap(),
            "uncaptured-binary" => fs::write(
                f.root.path().join("src/tree/opaque.bin"),
                b"\0owned opaque bytes",
            )
            .unwrap(),
            "missing-source" => args["relative_path"] = json!("absent"),
            _ => args["target_relative_path"] = json!("src/tree/sub"),
        }
        assert!(invoke(&mut f, args).is_err(), "{case}");
        assert_eq!(records(&f), (0, 0));
        assert!(transcript(&f).is_empty(), "{case}");
        assert_eq!(
            fs::read(f.root.path().join("src/tree/a.rs")).unwrap(),
            BEFORE.as_bytes()
        );
    }
}
#[test]
fn directory_edit_permission_and_excluded_scope_refusals_precede_provider_start() {
    let mut f = fixture(AnalyzerEpoch::V9, "edits");
    f.session
        .invoke("switchModes", &json!({"modes":["planning"]}), &d())
        .unwrap();
    assert!(
        invoke(
            &mut f,
            json!({"relative_path":"src/tree","target_relative_path":"pkg.rs"})
        )
        .is_err()
    );
    assert!(transcript(&f).is_empty());
    assert_eq!(records(&f), (0, 0));
    let mut f = fixture(AnalyzerEpoch::V9, "edits");
    f.session.engine.capture_options = CaptureOptions {
        only_dirs: vec!["src".into()],
        ..Default::default()
    };
    assert!(
        invoke(
            &mut f,
            json!({"relative_path":"src/tree","target_relative_path":"pkg.rs"})
        )
        .is_err()
    );
    assert!(transcript(&f).is_empty());
    assert_eq!(records(&f), (0, 0));
    unchanged(&f);
}
#[test]
fn folder_capability_and_original_coordinate_text_only_effects_are_required_before_publication() {
    for mode in [
        "file-only",
        "bad-resource",
        "outside",
        "bad-version",
        "unknown-root",
    ] {
        let mut f = fixture(AnalyzerEpoch::V8, mode);
        let error = invoke(
            &mut f,
            json!({"relative_path":"src/tree","target_relative_path":"pkg.rs"}),
        )
        .err()
        .expect("Unsupported folder edit must refuse");
        assert!(!format!("{error:#}").contains("fixture_private_value"));
        assert_eq!(records(&f), (0, 0), "{mode}");
        unchanged(&f);
        let calls = transcript(&f);
        assert!(
            !calls
                .iter()
                .any(|r| r["method"] == "workspace/executeCommand")
        );
        if mode == "file-only" {
            assert!(
                !calls
                    .iter()
                    .any(|r| r["method"] == "workspace/willRenameFiles")
            );
        }
    }
}
#[test]
fn actual_owned_verification_applies_and_recovers_the_full_directory_shape() {
    let mut f = fixture(AnalyzerEpoch::V9, "edits");
    let value = invoke(
        &mut f,
        json!({"relative_path":"src/tree","target_relative_path":"pkg.rs"}),
    )
    .unwrap();
    let id = value["plan"]["id"].as_str().unwrap();
    let verification=yoke::verify::execute(&f.session.engine,id,"node",&["-e".into(),"const fs=require('fs');process.exit(fs.existsSync('pkg.rs/tree/empty/nested')&&!fs.existsSync('src/tree')&&fs.readFileSync('pkg.rs/tree/a.rs','utf8').startsWith('// provider prepared folder move\\r\\n')?0:1)".into()],true,&d()).unwrap();
    assert_eq!(verification["status"], "passed", "{verification}");
    assert_eq!(verification["level"], "executed");
    changes::approve(&f.session.engine.store, id, false).unwrap();
    let receipt = changes::apply(
        &f.session.engine.store,
        f.root.path(),
        &json!({"planId":id,"idempotencyKey":"owned-folder-full-roundtrip","mode":"workspace"}),
        true,
        &d(),
    )
    .unwrap();
    assert!(!f.root.path().join("src/tree").exists());
    assert!(f.root.path().join("pkg.rs/tree/empty/nested").is_dir());
    assert_eq!(
        fs::read(f.root.path().join("pkg.rs/tree/a.rs")).unwrap(),
        AFTER.as_bytes()
    );
    changes::recover(&f.session.engine.store, f.root.path(), &receipt.id, true).unwrap();
    unchanged(&f);
    assert_eq!(
        fs::read(f.root.path().join("src/use.rs")).unwrap(),
        b"// original folder reference\r\n"
    );
}

#[cfg(windows)]
#[test]
fn directory_destination_parent_requires_its_exact_existing_host_spelling() {
    let mut f = fixture(AnalyzerEpoch::V8, "edits");
    let result = invoke(
        &mut f,
        json!({"relative_path":"src/tree","target_relative_path":"PKG.RS"}),
    );
    assert!(
        result.is_err(),
        "A directory plan used an alias spelling of its existing parent"
    );
    assert_eq!(records(&f), (0, 0));
    assert!(transcript(&f).is_empty());
    unchanged(&f);
}
