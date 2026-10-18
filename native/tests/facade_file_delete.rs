use serde_json::{Value, json};
use std::{fs, path::Path};
use yoke::{
    Engine,
    analysis::AnalyzerEpoch,
    capture::{self, CaptureOptions, Deadline},
    changes, lsp,
    project_session::{HostProfile, Session},
};

const SOURCE: &str = "pub fn helper() {}\n";
const REFERENCES: &str = "mod old;\nfn caller() { old::helper(); }\n";

struct Fixture {
    root: tempfile::TempDir,
    log: tempfile::TempDir,
    session: Session,
}
fn fixture(epoch: AnalyzerEpoch, options: CaptureOptions) -> Fixture {
    let root = tempfile::tempdir().unwrap();
    let log = tempfile::tempdir().unwrap();
    for directory in ["src", "other", "ignored", "node_modules/sdk"] {
        fs::create_dir_all(root.path().join(directory)).unwrap();
    }
    fs::write(root.path().join("src/old.rs"), SOURCE).unwrap();
    fs::write(root.path().join("src/lib.rs"), REFERENCES).unwrap();
    fs::write(root.path().join("ignored/old.rs"), SOURCE).unwrap();
    fs::write(root.path().join("node_modules/sdk/old.rs"), SOURCE).unwrap();
    fs::write(root.path().join(".gitignore"), "ignored/\n!node_modules/\n").unwrap();
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
        Some("facade-file-delete"),
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
fn no_provider(f: &Fixture) {
    assert!(
        !f.log.path().join("protocol.log").exists(),
        "File deletion must not start a semantic provider"
    );
}
fn originals(f: &Fixture) {
    assert_eq!(
        fs::read_to_string(f.root.path().join("src/old.rs")).unwrap(),
        SOURCE
    );
    assert_eq!(
        fs::read_to_string(f.root.path().join("src/lib.rs")).unwrap(),
        REFERENCES
    );
}
fn invoke(f: &mut Fixture, args: Value) -> anyhow::Result<Value> {
    yoke::agent_repl::invoke_facade(
        &mut f.session,
        "jb.safe_delete",
        args,
        None,
        &Deadline::new(30000),
    )
}
fn assert_refused(f: &mut Fixture, args: Value) -> String {
    let value = invoke(f, args);
    assert!(
        value.is_err(),
        "Invalid forced file deletion accepted: {value:?}"
    );
    assert_eq!(
        records(f),
        (0, 0),
        "Refused deletion published change objects"
    );
    no_provider(f);
    originals(f);
    format!("{:#}", value.err().unwrap())
}
fn assert_prepared(f: &Fixture, value: &Value, base: &str, epoch: AnalyzerEpoch) {
    assert_eq!(value["snapshotId"], base);
    assert_eq!(value["forced"], true);
    assert_eq!(value["propagate"], false);
    assert_eq!(value["referencesRewritten"], false);
    assert_eq!(
        value["sourceBinding"],
        json!({"snapshotId":base,"path":"src/old.rs","contentHash":yoke::model::hash(SOURCE)})
    );
    assert_eq!(value["remainingUsagesMayBreak"], true);
    assert_eq!(value["usageCheck"]["performed"], false);
    assert_eq!(value["usageCheck"]["exhaustiveCompilerProof"], false);
    assert_eq!(value["commandsExecuted"], false);
    assert!(
        value["warnings"]
            .as_array()
            .is_some_and(|warnings| !warnings.is_empty()),
        "Explicit force must disclose that usages were not checked or rewritten"
    );
    let plan = &value["plan"];
    assert_eq!(plan["baseSnapshotId"], base);
    let files = plan["files"].as_array().unwrap();
    assert_eq!(files.len(), 1);
    assert_eq!(files[0]["path"], "src/old.rs");
    assert_eq!(files[0]["beforeHash"], yoke::model::hash(SOURCE));
    assert!(
        files[0]["afterHash"].is_null(),
        "Whole file must be deleted, not replaced with an empty file"
    );
    let stored: changes::Plan = f
        .session
        .engine
        .store
        .get_record("plan", plan["id"].as_str().unwrap())
        .unwrap();
    assert_eq!(
        serde_json::to_value(stored).unwrap(),
        *plan,
        "Returned plan must exactly match its immutable stored record"
    );
    let result = f
        .session
        .engine
        .store
        .snapshot(plan["resultSnapshotId"].as_str().unwrap())
        .unwrap();
    assert!(!result.files.iter().any(|file| file.path == "src/old.rs"));
    assert_eq!(
        yoke::query::source(&f.session.engine.store, &result, "src/lib.rs")
            .unwrap()
            .0,
        REFERENCES,
        "Force with propagation disabled must not rewrite existing usages"
    );
    assert_eq!(
        capture::analyzer_epoch_for_snapshot(&f.session.engine.store, &result).unwrap(),
        epoch
    );
    assert_eq!(records(f), (1, 1));
    originals(f);
    no_provider(f);
}
fn prepared(epoch: AnalyzerEpoch) -> (Fixture, Value) {
    let mut f = fixture(epoch, CaptureOptions::default());
    let (base, _, _, _) = f.session.engine.index(&Deadline::new(30000)).unwrap();
    let value = invoke(
        &mut f,
        json!({"relative_path":"src/old.rs","delete_even_if_used":true}),
    )
    .unwrap();
    assert_prepared(&f, &value, &base.id, epoch);
    (f, value)
}
fn approval_fixture(f: &Fixture, value: &Value) {
    let id = value["plan"]["id"].as_str().unwrap();
    f.session.engine.approve(id, false).unwrap();
    // Explicit host-gate fixture record reaches real apply preflight; no compiler was run.
    f.session.engine.store.record("verification", id,
        &json!({"status":"passed","level":"executed","snapshotId":value["plan"]["resultSnapshotId"]})).unwrap();
}

#[test]
fn forced_file_delete_omitted_and_null_selectors_create_one_exact_delete_for_v8_v9() {
    for epoch in [AnalyzerEpoch::V8, AnalyzerEpoch::V9] {
        for selector in [None, Some(Value::Null)] {
            let mut f = fixture(epoch, CaptureOptions::default());
            let (base, _, _, _) = f.session.engine.index(&Deadline::new(30000)).unwrap();
            let mut args = json!({"relative_path":"src/old.rs","delete_even_if_used":true});
            if let Some(selector) = selector {
                args["name_path"] = selector;
                args["propagate"] = json!(false);
            }
            let value = invoke(&mut f, args)
                .expect("Explicit file force must not require a symbol selector");
            assert_prepared(&f, &value, &base.id, epoch);
        }
    }
}

#[test]
fn public_forced_file_delete_accepts_generated_null_and_false_defaults() {
    let mut f = fixture(AnalyzerEpoch::V9, CaptureOptions::default());
    let (base, _, _, _) = f.session.engine.index(&Deadline::new(30000)).unwrap();
    let value = yoke::compatibility::invoke_serena(
        &mut f.session,
        "jet_brains_safe_delete",
        &json!({"relative_path":"src/old.rs","delete_even_if_used":true}),
        &Deadline::new(30000),
    )
    .unwrap();
    assert_prepared(&f, &value, &base.id, AnalyzerEpoch::V9);
}

#[test]
fn forced_file_delete_materializes_the_plan_without_removing_the_original_or_its_callers() {
    let (f, value) = prepared(AnalyzerEpoch::V8);
    let receipt = changes::apply(
        &f.session.engine.store,
        &f.session.engine.root,
        &json!({"planId":value["plan"]["id"],"idempotencyKey":"isolated-file-delete"}),
        false,
        &Deadline::new(30000),
    )
    .unwrap();
    assert_eq!(receipt.status, "applied");
    let tree = Path::new(receipt.sandbox_path.as_deref().unwrap());
    assert!(!tree.join("src/old.rs").exists());
    assert_eq!(
        fs::read_to_string(tree.join("src/lib.rs")).unwrap(),
        REFERENCES
    );
    originals(&f);
    no_provider(&f);
}

#[test]
fn forced_file_delete_workspace_apply_keeps_host_approval_and_verification_gates() {
    let (f, value) = prepared(AnalyzerEpoch::V9);
    let args = json!({"planId":value["plan"]["id"],"idempotencyKey":"workspace-file-delete","mode":"workspace"});
    assert!(
        changes::apply(
            &f.session.engine.store,
            &f.session.engine.root,
            &args,
            false,
            &Deadline::new(30000)
        )
        .is_err()
    );
    assert!(
        changes::apply(
            &f.session.engine.store,
            &f.session.engine.root,
            &args,
            true,
            &Deadline::new(30000)
        )
        .is_err()
    );
    f.session
        .engine
        .approve(value["plan"]["id"].as_str().unwrap(), false)
        .unwrap();
    assert!(
        changes::apply(
            &f.session.engine.store,
            &f.session.engine.root,
            &args,
            true,
            &Deadline::new(30000)
        )
        .is_err()
    );
    originals(&f);
    approval_fixture(&f, &value);
    let receipt = changes::apply(
        &f.session.engine.store,
        &f.session.engine.root,
        &args,
        true,
        &Deadline::new(30000),
    )
    .unwrap();
    assert_eq!(receipt.status, "applied");
    assert_eq!(receipt.changed_paths, vec!["src/old.rs"]);
    assert!(!f.root.path().join("src/old.rs").exists());
    assert_eq!(
        fs::read_to_string(f.root.path().join("src/lib.rs")).unwrap(),
        REFERENCES
    );
    no_provider(&f);
}

#[test]
fn forced_file_delete_stale_workspace_preflight_preserves_changed_sources_for_v8_v9() {
    for epoch in [AnalyzerEpoch::V8, AnalyzerEpoch::V9] {
        let (f, value) = prepared(epoch);
        approval_fixture(&f, &value);
        fs::write(
            f.root.path().join("src/old.rs"),
            "pub fn changed_after_plan() {}\n",
        )
        .unwrap();
        let result = changes::apply(
            &f.session.engine.store,
            &f.session.engine.root,
            &json!({"planId":value["plan"]["id"],"idempotencyKey":"stale-file-delete","mode":"workspace"}),
            true,
            &Deadline::new(30000),
        );
        assert!(format!("{:#}", result.err().unwrap()).contains("Workspace snapshot is stale"));
        assert_eq!(
            fs::read_to_string(f.root.path().join("src/old.rs")).unwrap(),
            "pub fn changed_after_plan() {}\n"
        );
        assert_eq!(
            fs::read_to_string(f.root.path().join("src/lib.rs")).unwrap(),
            REFERENCES
        );
        assert_eq!(records(&f), (2, 1));
        no_provider(&f);
    }
}

#[test]
fn default_safe_mode_and_propagation_are_distinct_unsupported_file_intents() {
    for (options, reason) in [
        (json!({}), "file usage proof"),
        (json!({"delete_even_if_used":false}), "file usage proof"),
        (
            json!({"delete_even_if_used":true,"propagate":true}),
            "propagation",
        ),
        (json!({"propagate":true}), "propagation"),
        (json!({"delete_even_if_used":null}), "boolean"),
        (json!({"delete_even_if_used":"true"}), "boolean"),
        (
            json!({"delete_even_if_used":true,"propagate":null}),
            "boolean",
        ),
        (json!({"delete_even_if_used":true,"propagate":0}), "boolean"),
    ] {
        let mut f = fixture(AnalyzerEpoch::V8, CaptureOptions::default());
        let mut args = json!({"relative_path":"src/old.rs","name_path":null});
        args.as_object_mut()
            .unwrap()
            .extend(options.as_object().unwrap().clone());
        let error = assert_refused(&mut f, args);
        assert!(
            error.contains(reason),
            "Expected refusal for {reason}: {error}"
        );
    }
}

#[test]
fn empty_malformed_directory_missing_and_unsafe_file_selectors_never_prepare_deletions() {
    for selector in [json!(""), json!(" "), json!(false), json!(0), json!([])] {
        let mut f = fixture(AnalyzerEpoch::V8, CaptureOptions::default());
        assert_refused(
            &mut f,
            json!({"relative_path":"src/old.rs","name_path":selector,"delete_even_if_used":true}),
        );
    }
    for source in [
        "src",
        "missing.rs",
        "../outside.rs",
        "src\\old.rs",
        "C:/outside.rs",
        "src/old.rs:stream",
        ".env",
        ".git/config",
        "src/OLD.rs",
    ] {
        let mut f = fixture(AnalyzerEpoch::V9, CaptureOptions::default());
        assert_refused(
            &mut f,
            json!({"relative_path":source,"delete_even_if_used":true}),
        );
    }
}

#[test]
fn readonly_ignored_excluded_and_uncaptured_binary_sources_cannot_be_force_deleted() {
    for (options, source) in [
        (
            CaptureOptions {
                read_only_dependency_dirs: vec!["node_modules".into()],
                ..Default::default()
            },
            "node_modules/sdk/old.rs",
        ),
        (
            CaptureOptions {
                include_ignored_dirs: vec!["ignored".into()],
                ..Default::default()
            },
            "ignored/old.rs",
        ),
        (
            CaptureOptions {
                only_dirs: vec!["other".into()],
                ..Default::default()
            },
            "src/old.rs",
        ),
    ] {
        let mut f = fixture(AnalyzerEpoch::V9, options);
        let (snapshot, _, _, _) = f.session.engine.index(&Deadline::new(30000)).unwrap();
        if source != "src/old.rs" {
            assert!(
                capture::source_path_policy(&f.session.engine.store, &snapshot, source)
                    .unwrap()
                    .read_only
            );
        }
        assert_refused(
            &mut f,
            json!({"relative_path":source,"delete_even_if_used":true}),
        );
        assert_eq!(
            fs::read_to_string(f.root.path().join(source)).unwrap(),
            SOURCE
        );
    }
    let mut f = fixture(AnalyzerEpoch::V8, CaptureOptions::default());
    fs::write(
        f.root.path().join("src/opaque.rs"),
        b"\0opaque captured omission\n",
    )
    .unwrap();
    let (snapshot, _, _, _) = f.session.engine.index(&Deadline::new(30000)).unwrap();
    assert!(
        !snapshot
            .files
            .iter()
            .any(|file| file.path == "src/opaque.rs")
    );
    assert!(
        snapshot
            .omissions
            .iter()
            .any(|omission| omission.path == "src/opaque.rs")
    );
    assert_refused(
        &mut f,
        json!({"relative_path":"src/opaque.rs","delete_even_if_used":true}),
    );
    assert_eq!(
        fs::read(f.root.path().join("src/opaque.rs")).unwrap(),
        b"\0opaque captured omission\n"
    );
}

#[test]
fn source_symlink_cannot_redirect_a_forced_file_deletion() {
    let mut f = fixture(AnalyzerEpoch::V9, CaptureOptions::default());
    let link = f.root.path().join("src/linked.rs");
    #[cfg(windows)]
    std::os::windows::fs::symlink_file(f.root.path().join("src/old.rs"), &link).unwrap();
    #[cfg(unix)]
    std::os::unix::fs::symlink(f.root.path().join("src/old.rs"), &link).unwrap();
    assert_refused(
        &mut f,
        json!({"relative_path":"src/linked.rs","delete_even_if_used":true}),
    );
    assert!(
        fs::symlink_metadata(&link)
            .unwrap()
            .file_type()
            .is_symlink()
    );
    fs::remove_file(link).unwrap();
}

#[test]
fn forced_file_delete_permission_and_external_context_denials_have_zero_change_effects() {
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
        json!({"relative_path":"src/old.rs","delete_even_if_used":true}),
    );
    let mut f = fixture(AnalyzerEpoch::V9, CaptureOptions::default());
    let result = yoke::agent_repl::invoke_facade(
        &mut f.session,
        "jb.safe_delete",
        json!({"relative_path":"src/old.rs","delete_even_if_used":true}),
        Some("default"),
        &Deadline::new(30000),
    );
    assert!(result.is_err());
    assert_eq!(records(&f), (0, 0));
    originals(&f);
    no_provider(&f);
}
