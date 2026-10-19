use serde_json::{Value, json};
use std::{fs, path::Path};
use yoke::{
    Engine,
    analysis::AnalyzerEpoch,
    capture::{self, CaptureOptions, Deadline},
    lsp,
    project_session::{HostProfile, Session},
};

const SOURCE_PATH: &str = "src/mód.rs";
const SOURCE: &str = "fn helper() {}\n";
const MOVED: &str = "// provider prepared move\nfn helper() {}\n";

struct Fixture {
    root: tempfile::TempDir,
    log: tempfile::TempDir,
    session: Session,
}
fn fixture(epoch: AnalyzerEpoch, options: CaptureOptions) -> Fixture {
    let root = tempfile::tempdir().unwrap();
    let log = tempfile::tempdir().unwrap();
    fs::create_dir_all(root.path().join("src")).unwrap();
    fs::create_dir_all(root.path().join("target.rs")).unwrap();
    fs::create_dir_all(root.path().join("node_modules/sdk")).unwrap();
    fs::write(root.path().join(SOURCE_PATH), SOURCE).unwrap();
    fs::write(root.path().join("node_modules/sdk/main.rs"), SOURCE).unwrap();
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
                        .join("tests/fixtures/facade-file-move-lsp.mjs")
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
        Some("facade-file-move"),
        &Deadline::new(30000),
    )
    .unwrap();
    Fixture { root, log, session }
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
    yoke::agent_repl::invoke_facade(&mut f.session, "jb.move", args, None, &Deadline::new(30000))
}
fn assert_refused(f: &mut Fixture, args: Value) {
    let result = invoke(f, args);
    assert!(result.is_err(), "Invalid facade move accepted: {result:?}");
    assert_eq!(
        records(f),
        (0, 0),
        "Rejected move left persistent change objects"
    );
    assert_eq!(
        fs::read_to_string(f.root.path().join(SOURCE_PATH)).unwrap(),
        SOURCE
    );
    assert!(
        !transcript(f)
            .iter()
            .any(|request| request["method"] == "workspace/executeCommand")
    );
}

#[test]
fn file_move_omitted_null_and_empty_selectors_preserve_basename_in_the_target_parent_for_v8_v9() {
    for epoch in [AnalyzerEpoch::V8, AnalyzerEpoch::V9] {
        for selector in [None, Some(Value::Null), Some(json!(""))] {
            let mut f = fixture(epoch, CaptureOptions::default());
            let mut args = json!({"relative_path":SOURCE_PATH,"target_relative_path":"target.rs"});
            if let Some(selector) = selector {
                args["name_path"] = selector;
            }
            let value = invoke(&mut f, args).unwrap();
            let plan = &value["plan"];
            let result = f
                .session
                .engine
                .store
                .snapshot(plan["resultSnapshotId"].as_str().unwrap())
                .unwrap();
            assert!(
                result
                    .files
                    .iter()
                    .any(|file| file.path == "target.rs/mód.rs"),
                "target is a parent directory, not a new filename: {value}"
            );
            assert!(!result.files.iter().any(|file| file.path == SOURCE_PATH));
            assert_eq!(
                yoke::query::source(&f.session.engine.store, &result, "target.rs/mód.rs")
                    .unwrap()
                    .0,
                MOVED
            );
            assert_eq!(
                capture::analyzer_epoch_for_snapshot(&f.session.engine.store, &result).unwrap(),
                epoch
            );
            assert_eq!(records(&f), (1, 1));
            let requests = transcript(&f);
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
            assert!(old.ends_with(SOURCE_PATH));
            assert!(
                new.ends_with("target.rs/mód.rs"),
                "provider saw the wrong destination: {}",
                new.display()
            );
            assert_eq!(old.file_name(), new.file_name());
            assert!(
                !requests
                    .iter()
                    .any(|request| request["method"] == "workspace/executeCommand")
            );
            assert_eq!(
                fs::read_to_string(f.root.path().join(SOURCE_PATH)).unwrap(),
                SOURCE
            );
            assert!(f.root.path().join("target.rs").is_dir());
            assert!(!f.root.path().join("target.rs/mód.rs").exists());
        }
    }
}

#[test]
fn move_target_parent_must_be_explicit_and_must_not_be_an_existing_file() {
    for target in [
        None,
        Some(Value::Null),
        Some(json!("src/mód.rs")),
        Some(json!("not-present")),
    ] {
        let mut f = fixture(AnalyzerEpoch::V8, CaptureOptions::default());
        let mut args = json!({"relative_path":SOURCE_PATH,"name_path":null});
        if let Some(target) = target {
            args["target_relative_path"] = target;
        }
        assert_refused(&mut f, args);
        assert!(
            transcript(&f).is_empty(),
            "invalid target queried a provider"
        );
    }
}

#[test]
fn occupied_final_child_and_same_parent_moves_refuse_without_change_objects() {
    for parent in ["target.rs", "src"] {
        let mut f = fixture(AnalyzerEpoch::V8, CaptureOptions::default());
        fs::write(f.root.path().join("target.rs/mód.rs"), "fn occupied() {}\n").unwrap();
        assert_refused(
            &mut f,
            json!({"relative_path":SOURCE_PATH,"target_relative_path":parent}),
        );
        assert!(
            transcript(&f).is_empty(),
            "collision/no-op queried a provider"
        );
        assert_eq!(
            fs::read_to_string(f.root.path().join("target.rs/mód.rs")).unwrap(),
            "fn occupied() {}\n"
        );
    }
}

#[test]
fn symbol_parent_and_malformed_file_selectors_are_explicit_refusals() {
    for args in [
        json!({"relative_path":SOURCE_PATH,"target_relative_path":"target.rs","target_parent_name_path":"Parent"}),
        json!({"relative_path":SOURCE_PATH,"target_relative_path":"target.rs","name_path":false}),
        json!({"relative_path":SOURCE_PATH,"target_relative_path":"target.rs","name_path":0}),
        json!({"relative_path":SOURCE_PATH,"target_relative_path":"target.rs","name_path":" "}),
        json!({"relative_path":SOURCE_PATH,"target_relative_path":"target.rs","name_path":"helper"}),
    ] {
        let mut f = fixture(AnalyzerEpoch::V8, CaptureOptions::default());
        assert_refused(&mut f, args);
        assert!(transcript(&f).is_empty());
    }
}

#[test]
fn directory_move_with_a_file_only_provider_negotiates_capabilities_but_publishes_no_plan() {
    let mut f = fixture(AnalyzerEpoch::V8, CaptureOptions::default());
    assert_refused(
        &mut f,
        json!({"relative_path":"src","target_relative_path":"target.rs"}),
    );
    let requests = transcript(&f);
    assert!(
        requests
            .iter()
            .any(|request| request["method"] == "initialize")
    );
    assert!(
        !requests
            .iter()
            .any(|request| request["method"] == "workspace/willRenameFiles")
    );
}

#[test]
fn unsafe_target_parents_and_captured_readonly_or_excluded_scopes_never_prepare_plans() {
    for parent in [
        "../outside",
        ".git",
        "node_modules/sdk",
        "C:/outside",
        "target.rs/../other",
    ] {
        let mut f = fixture(AnalyzerEpoch::V8, CaptureOptions::default());
        assert_refused(
            &mut f,
            json!({"relative_path":SOURCE_PATH,"target_relative_path":parent}),
        );
        assert!(
            transcript(&f).is_empty(),
            "unsafe destination queried a provider"
        );
    }
    let mut f = fixture(
        AnalyzerEpoch::V9,
        CaptureOptions {
            only_dirs: vec!["src".into()],
            ..Default::default()
        },
    );
    assert_refused(
        &mut f,
        json!({"relative_path":SOURCE_PATH,"target_relative_path":"target.rs"}),
    );
    assert!(transcript(&f).is_empty());
    for (source, parent) in [
        (SOURCE_PATH, "node_modules/sdk"),
        ("node_modules/sdk/main.rs", "target.rs"),
    ] {
        let mut f = fixture(
            AnalyzerEpoch::V9,
            CaptureOptions {
                read_only_dependency_dirs: vec!["node_modules".into()],
                ..Default::default()
            },
        );
        let (snapshot, _, _, _) = f.session.engine.index(&Deadline::new(30000)).unwrap();
        assert!(
            capture::source_path_policy(
                &f.session.engine.store,
                &snapshot,
                "node_modules/sdk/main.rs"
            )
            .unwrap()
            .read_only
        );
        assert_refused(
            &mut f,
            json!({"relative_path":source,"target_relative_path":parent}),
        );
        assert!(transcript(&f).is_empty());
        assert_eq!(
            fs::read_to_string(f.root.path().join("node_modules/sdk/main.rs")).unwrap(),
            SOURCE
        );
    }
}

#[test]
fn move_permissions_and_external_project_refusal_precede_provider_start() {
    let mut f = fixture(AnalyzerEpoch::V8, CaptureOptions::default());
    f.session
        .invoke(
            "switchModes",
            &json!({"modes":["planning"]}),
            &Deadline::new(30000),
        )
        .unwrap();
    assert_refused(
        &mut f,
        json!({"relative_path":SOURCE_PATH,"target_relative_path":"target.rs","name_path":""}),
    );
    assert!(transcript(&f).is_empty());
    let mut f = fixture(AnalyzerEpoch::V8, CaptureOptions::default());
    let result = yoke::agent_repl::invoke_facade(
        &mut f.session,
        "jb.move",
        json!({"relative_path":SOURCE_PATH,"target_relative_path":"target.rs"}),
        Some("default"),
        &Deadline::new(30000),
    );
    assert!(result.is_err());
    assert_eq!(records(&f), (0, 0));
    assert!(transcript(&f).is_empty());
}

#[test]
fn omitted_binary_host_destination_is_still_occupied_before_provider_and_plan() {
    let mut f = fixture(AnalyzerEpoch::V8, CaptureOptions::default());
    let destination = "target.rs/mód.rs";
    let opaque = b"\0opaque binary existing destination\n";
    fs::write(f.root.path().join(destination), opaque).unwrap();
    let (snapshot, _, _, _) = f.session.engine.index(&Deadline::new(30000)).unwrap();
    assert!(
        !snapshot.files.iter().any(|file| file.path == destination),
        "regression must use an existing host object absent from the captured manifest"
    );
    assert!(
        snapshot
            .omissions
            .iter()
            .any(|omission| omission.path == destination),
        "binary destination must have an observed capture omission"
    );
    assert_refused(
        &mut f,
        json!({"relative_path":SOURCE_PATH,"target_relative_path":"target.rs","name_path":null}),
    );
    assert!(
        transcript(&f).is_empty(),
        "snapshot absence must not be interpreted as host-path absence"
    );
    assert_eq!(fs::read(f.root.path().join(destination)).unwrap(), opaque);
}
