use serde_json::{Value, json};
use std::{fs, path::Path};
use yoke::{
    Engine,
    analysis::AnalyzerEpoch,
    capture::{self, CaptureOptions, Deadline},
    lsp,
    project_session::{HostProfile, Session},
};

const SOURCE: &str = "pub fn helper() {}\n";
const REFERENCES: &str = "mod old;\nfn caller() { old::helper(); }\n// old.rs stays literal prose\nconst NOTE: &str = \"old.rs\";\n";
const RENAMED_REFERENCES: &str = "mod renamed;\nfn caller() { renamed::helper(); }\n// old.rs stays literal prose\nconst NOTE: &str = \"old.rs\";\n";

struct Fixture {
    root: tempfile::TempDir,
    log: tempfile::TempDir,
    session: Session,
    source: String,
    references: String,
}
fn fixture(epoch: AnalyzerEpoch, options: CaptureOptions, root_file: bool) -> Fixture {
    let root = tempfile::tempdir().unwrap();
    let log = tempfile::tempdir().unwrap();
    fs::create_dir_all(root.path().join("src")).unwrap();
    fs::create_dir_all(root.path().join("other")).unwrap();
    fs::create_dir_all(root.path().join("node_modules/sdk")).unwrap();
    let (source, references) = if root_file {
        ("old.rs", "lib.rs")
    } else {
        ("src/old.rs", "src/lib.rs")
    };
    fs::write(root.path().join(source), SOURCE).unwrap();
    fs::write(root.path().join(references), REFERENCES).unwrap();
    fs::write(root.path().join("node_modules/sdk/old.rs"), SOURCE).unwrap();
    fs::write(root.path().join(".gitignore"), "!node_modules/\n").unwrap();
    let mut engine = Engine::open(root.path()).unwrap();
    engine.capture_epoch = epoch;
    engine.capture_options = options;
    engine
        .register_configured_server(lsp::ConfiguredServer {
            server: lsp::Server {
                command: "node".into(),
                args: vec![
                    Path::new(env!("CARGO_MANIFEST_DIR"))
                        .join("tests/fixtures/facade-file-rename-lsp.mjs")
                        .display()
                        .to_string(),
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
        Some("facade-file-rename"),
        &Deadline::new(30000),
    )
    .unwrap();
    Fixture {
        root,
        log,
        session,
        source: source.into(),
        references: references.into(),
    }
}
fn records(f: &Fixture) -> (usize, usize) {
    let objects = f
        .session
        .engine
        .store
        .db
        .query_row(
            "SELECT COUNT(*) FROM records WHERE kind IN ('plan','approval','receipt')",
            [],
            |row| row.get(0),
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
            |row| row.get(0),
        )
        .unwrap();
    (objects, derived)
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
fn invoke(f: &mut Fixture, args: Value) -> anyhow::Result<Value> {
    yoke::agent_repl::invoke_facade(
        &mut f.session,
        "jb.rename",
        args,
        None,
        &Deadline::new(30000),
    )
}
fn assert_refused(f: &mut Fixture, args: Value) {
    let result = invoke(f, args);
    assert!(
        result.is_err(),
        "Invalid facade file rename accepted: {result:?}"
    );
    assert_eq!(
        records(f),
        (0, 0),
        "Refused file rename left persistent change objects"
    );
    assert!(
        transcript(f).is_empty(),
        "Admission failure started a provider"
    );
    assert_eq!(
        fs::read_to_string(f.root.path().join(&f.source)).unwrap(),
        SOURCE
    );
    assert_eq!(
        fs::read_to_string(f.root.path().join(&f.references)).unwrap(),
        REFERENCES
    );
}
fn assert_prepared(
    f: &Fixture,
    value: &Value,
    destination: &str,
    references: &str,
    epoch: AnalyzerEpoch,
) {
    let plan = &value["plan"];
    let result = f
        .session
        .engine
        .store
        .snapshot(plan["resultSnapshotId"].as_str().unwrap())
        .unwrap();
    assert!(!result.files.iter().any(|file| file.path == f.source));
    assert_eq!(
        yoke::query::source(&f.session.engine.store, &result, destination)
            .unwrap()
            .0,
        SOURCE
    );
    assert_eq!(
        yoke::query::source(&f.session.engine.store, &result, &f.references)
            .unwrap()
            .0,
        references
    );
    assert_eq!(
        capture::analyzer_epoch_for_snapshot(&f.session.engine.store, &result).unwrap(),
        epoch
    );
    let files = plan["files"].as_array().unwrap();
    assert_eq!(
        files.len(),
        3,
        "Source deletion, destination creation and real reference edit are required"
    );
    assert!(
        files
            .iter()
            .any(|file| file["path"] == f.source && file["afterHash"].is_null())
    );
    assert!(
        files
            .iter()
            .any(|file| file["path"] == destination && file["beforeHash"].is_null())
    );
    assert_eq!(records(f), (1, 1));
    let requests = transcript(f);
    let renames: Vec<_> = requests
        .iter()
        .filter(|request| request["method"] == "workspace/willRenameFiles")
        .collect();
    assert_eq!(renames.len(), 1);
    let old = url::Url::parse(renames[0]["params"]["files"][0]["oldUri"].as_str().unwrap())
        .unwrap()
        .to_file_path()
        .unwrap();
    let new = url::Url::parse(renames[0]["params"]["files"][0]["newUri"].as_str().unwrap())
        .unwrap()
        .to_file_path()
        .unwrap();
    assert!(old.ends_with(&f.source));
    assert!(new.ends_with(destination));
    assert_eq!(
        old.parent(),
        new.parent(),
        "File rename must retain the original parent"
    );
    assert!(
        !requests
            .iter()
            .any(|request| request["method"] == "workspace/executeCommand")
    );
    assert_eq!(
        fs::read_to_string(f.root.path().join(&f.source)).unwrap(),
        SOURCE
    );
    assert_eq!(
        fs::read_to_string(f.root.path().join(&f.references)).unwrap(),
        REFERENCES
    );
    assert!(!f.root.path().join(destination).exists());
}

#[test]
fn nullable_file_rename_omitted_and_null_selectors_preserve_parent_for_v8_v9() {
    for epoch in [AnalyzerEpoch::V8, AnalyzerEpoch::V9] {
        for selector in [None, Some(Value::Null)] {
            let mut f = fixture(epoch, CaptureOptions::default(), false);
            let mut args = json!({"relative_path":"src/old.rs","new_name":"renamed.rs"});
            if let Some(selector) = selector {
                args["name_path"] = selector;
            }
            let value =
                invoke(&mut f, args).expect("Omitted/null name_path must select the captured file");
            assert_prepared(&f, &value, "src/renamed.rs", RENAMED_REFERENCES, epoch);
        }
    }
}

#[test]
fn public_file_rename_accepts_generated_false_defaults_and_explicit_false_options() {
    for epoch in [AnalyzerEpoch::V8, AnalyzerEpoch::V9] {
        for explicit in [false, true] {
            let mut f = fixture(epoch, CaptureOptions::default(), false);
            let mut args = json!({"relative_path":"src/old.rs","new_name":"renamed.rs"});
            if explicit {
                args["name_path"] = Value::Null;
                args["rename_in_comments"] = json!(false);
                args["rename_in_text_occurrences"] = json!(false);
            }
            let value = yoke::compatibility::invoke_serena(
                &mut f.session,
                "jet_brains_rename",
                &args,
                &Deadline::new(30000),
            )
            .expect("Public null-file defaults must reach the supported file path");
            assert_prepared(&f, &value, "src/renamed.rs", RENAMED_REFERENCES, epoch);
        }
    }
}

#[test]
fn root_file_rename_uses_the_supplied_unicode_basename_without_creating_a_parent() {
    let mut f = fixture(AnalyzerEpoch::V9, CaptureOptions::default(), true);
    let value = invoke(
        &mut f,
        json!({"relative_path":"old.rs","new_name":"renam\u{00e9}d.rs","name_path":null,
        "rename_in_comments":false,"rename_in_text_occurrences":false}),
    )
    .unwrap();
    assert_prepared(
        &f,
        &value,
        "renam\u{00e9}d.rs",
        "mod renam\u{00e9}d;\nfn caller() { renam\u{00e9}d::helper(); }\n// old.rs stays literal prose\nconst NOTE: &str = \"old.rs\";\n",
        AnalyzerEpoch::V9,
    );
}

#[test]
fn empty_selectors_and_true_or_malformed_file_occurrence_options_refuse_before_provider() {
    for selector in [json!(""), json!(" "), json!(false), json!(0), json!([])] {
        let mut f = fixture(AnalyzerEpoch::V8, CaptureOptions::default(), false);
        assert_refused(
            &mut f,
            json!({"relative_path":"src/old.rs","new_name":"renamed.rs","name_path":selector}),
        );
    }
    for flags in [
        json!({"rename_in_comments":true}),
        json!({"rename_in_text_occurrences":true}),
        json!({"rename_in_comments":true,"rename_in_text_occurrences":true}),
        json!({"rename_in_comments":null}),
        json!({"rename_in_text_occurrences":null}),
        json!({"rename_in_comments":"false"}),
    ] {
        let mut f = fixture(AnalyzerEpoch::V8, CaptureOptions::default(), false);
        let mut args =
            json!({"relative_path":"src/old.rs","new_name":"renamed.rs","name_path":null});
        args.as_object_mut()
            .unwrap()
            .extend(flags.as_object().unwrap().clone());
        assert_refused(&mut f, args);
    }
}

#[test]
fn file_rename_new_name_is_a_portable_single_basename_and_never_a_destination_path() {
    for name in [
        "",
        ".",
        "..",
        "../renamed.rs",
        "nested/renamed.rs",
        "nested\\renamed.rs",
        "/renamed.rs",
        "C:/renamed.rs",
        "renamed.rs:stream",
        "NUL.rs",
        "COM1",
        "renamed.rs.",
        "renamed.rs ",
        ".env",
        "secret.key",
        "bad?name.rs",
        "bad\nname.rs",
    ] {
        let mut f = fixture(AnalyzerEpoch::V8, CaptureOptions::default(), false);
        assert_refused(
            &mut f,
            json!({"relative_path":"src/old.rs","new_name":name}),
        );
    }
}

#[test]
fn occupied_file_directory_uncaptured_binary_and_source_alias_refuse_before_provider() {
    for kind in ["captured", "binary", "directory", "source", "source_case"] {
        let mut f = fixture(AnalyzerEpoch::V9, CaptureOptions::default(), false);
        let destination = "src/renamed.rs";
        match kind {
            "captured" => fs::write(f.root.path().join(destination), "fn occupied() {}\n").unwrap(),
            "binary" => fs::write(
                f.root.path().join(destination),
                b"\0opaque existing destination\n",
            )
            .unwrap(),
            "directory" => fs::create_dir(f.root.path().join(destination)).unwrap(),
            _ => {}
        }
        if kind == "binary" {
            let (snapshot, _, _, _) = f.session.engine.index(&Deadline::new(30000)).unwrap();
            assert!(!snapshot.files.iter().any(|file| file.path == destination));
            assert!(
                snapshot
                    .omissions
                    .iter()
                    .any(|omission| omission.path == destination)
            );
        }
        let name = match kind {
            "source" => "old.rs",
            "source_case" => "OLD.rs",
            _ => "renamed.rs",
        };
        assert_refused(
            &mut f,
            json!({"relative_path":"src/old.rs","new_name":name,"name_path":null}),
        );
        if kind == "binary" {
            assert_eq!(
                fs::read(f.root.path().join(destination)).unwrap(),
                b"\0opaque existing destination\n"
            );
        }
        if kind == "captured" {
            assert_eq!(
                fs::read_to_string(f.root.path().join(destination)).unwrap(),
                "fn occupied() {}\n"
            );
        }
        if kind == "directory" {
            assert!(f.root.path().join(destination).is_dir());
        }
    }
}

#[test]
fn directory_missing_excluded_and_readonly_sources_have_no_file_rename_authority() {
    for source in ["src", "missing.rs", "../outside.rs", ".git/config"] {
        let mut f = fixture(AnalyzerEpoch::V8, CaptureOptions::default(), false);
        assert_refused(
            &mut f,
            json!({"relative_path":source,"new_name":"renamed.rs","name_path":null}),
        );
    }
    let mut f = fixture(
        AnalyzerEpoch::V9,
        CaptureOptions {
            only_dirs: vec!["other".into()],
            ..Default::default()
        },
        false,
    );
    assert_refused(
        &mut f,
        json!({"relative_path":"src/old.rs","new_name":"renamed.rs"}),
    );
    let mut f = fixture(
        AnalyzerEpoch::V9,
        CaptureOptions {
            read_only_dependency_dirs: vec!["node_modules".into()],
            ..Default::default()
        },
        false,
    );
    let (snapshot, _, _, _) = f.session.engine.index(&Deadline::new(30000)).unwrap();
    assert!(
        capture::source_path_policy(
            &f.session.engine.store,
            &snapshot,
            "node_modules/sdk/old.rs"
        )
        .unwrap()
        .read_only
    );
    assert_refused(
        &mut f,
        json!({"relative_path":"node_modules/sdk/old.rs","new_name":"renamed.rs"}),
    );
    assert_eq!(
        fs::read_to_string(f.root.path().join("node_modules/sdk/old.rs")).unwrap(),
        SOURCE
    );
}

#[test]
fn file_rename_permissions_and_external_context_refusal_precede_provider_start() {
    let mut f = fixture(AnalyzerEpoch::V8, CaptureOptions::default(), false);
    f.session
        .invoke(
            "switchModes",
            &json!({"modes":["planning"]}),
            &Deadline::new(30000),
        )
        .unwrap();
    assert_refused(
        &mut f,
        json!({"relative_path":"src/old.rs","new_name":"renamed.rs"}),
    );
    let mut f = fixture(AnalyzerEpoch::V9, CaptureOptions::default(), false);
    let result = yoke::agent_repl::invoke_facade(
        &mut f.session,
        "jb.rename",
        json!({"relative_path":"src/old.rs","new_name":"renamed.rs","name_path":null}),
        Some("default"),
        &Deadline::new(30000),
    );
    assert!(result.is_err());
    assert!(transcript(&f).is_empty());
    assert_eq!(records(&f), (0, 0));
}

#[test]
fn guarded_file_rename_plan_refuses_stale_source_before_any_workspace_write() {
    for epoch in [AnalyzerEpoch::V8, AnalyzerEpoch::V9] {
        let mut f = fixture(epoch, CaptureOptions::default(), false);
        let value = invoke(
            &mut f,
            json!({"relative_path":"src/old.rs","new_name":"renamed.rs"}),
        )
        .unwrap();
        assert_prepared(&f, &value, "src/renamed.rs", RENAMED_REFERENCES, epoch);
        let id = value["plan"]["id"].as_str().unwrap();
        // Host-gate fixture records reach the real stale-source preflight; no compiler was run.
        f.session.engine.store.record("verification", id, &json!({"status":"passed","level":"executed","snapshotId":value["plan"]["resultSnapshotId"]})).unwrap();
        f.session.engine.approve(id, false).unwrap();
        fs::write(
            f.root.path().join("src/old.rs"),
            "pub fn changed_after_plan() {}\n",
        )
        .unwrap();
        let before = transcript(&f).len();
        let applied = yoke::changes::apply(
            &f.session.engine.store,
            &f.session.engine.root,
            &json!({"planId":id,"idempotencyKey":"stale-file-rename","mode":"workspace"}),
            true,
            &Deadline::new(30000),
        );
        assert!(format!("{:#}", applied.err().unwrap()).contains("Workspace snapshot is stale"));
        assert_eq!(
            fs::read_to_string(f.root.path().join("src/old.rs")).unwrap(),
            "pub fn changed_after_plan() {}\n"
        );
        assert_eq!(
            fs::read_to_string(f.root.path().join("src/lib.rs")).unwrap(),
            REFERENCES
        );
        assert!(!f.root.path().join("src/renamed.rs").exists());
        assert_eq!(transcript(&f).len(), before);
        assert_eq!(records(&f), (2, 1));
    }
}
