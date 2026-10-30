//! Explicit host CLI contracts for typed portable Meaning archives.
#[path = "support/meaning_archive_fixture.rs"]
mod support;

use serde_json::Value;
use std::{
    fs,
    path::Path,
    process::{Command, Output, Stdio},
};
use support::{d, fixture, registry_state, remove_source_workspace, store_state};
use tempfile::TempDir;
use yoke::{
    analysis::AnalyzerEpoch,
    graph_collections::Registry,
    graph_view_archive::{self as archive, Limits},
    model::{hash, valid_id},
};

fn command(cwd: &Path) -> Command {
    let mut result = Command::new(env!("CARGO_BIN_EXE_yoke"));
    result
        .current_dir(cwd)
        .args(["--timeout", "30000", "meaning-archive"]);
    result
}
fn success(output: Output) -> Value {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).expect("Archive CLI stdout must be metadata JSON")
}
fn export(f: &support::Fixture, destination: &Path) -> Output {
    command(f.dir.path())
        .args(["export", &f.view.id, "--root"])
        .arg(&f.engine.root)
        .arg("--output")
        .arg(destination)
        .output()
        .unwrap()
}
fn input(f: &support::Fixture) -> std::path::PathBuf {
    let path = f.dir.path().join("supplied-meaning-archive.json");
    let prepared = archive::prepare(&f.engine.store, &f.view, Limits::default(), &d()).unwrap();
    fs::write(&path, prepared.bytes()).unwrap();
    path
}
fn register(
    cwd: &Path,
    registry: &Path,
    input: &Path,
    name: &str,
    expected: Option<&str>,
) -> Command {
    let mut result = command(cwd);
    result
        .args(["register", name, "--registry"])
        .arg(registry)
        .arg("--input")
        .arg(input)
        .arg("--allow-registration");
    if let Some(id) = expected {
        result.args(["--expected", id]);
    } else {
        result.arg("--expect-absent");
    }
    result
}

#[test]
fn published_view_export_preserves_full_typed_archive_without_source_publication() {
    let f = fixture(AnalyzerEpoch::V9);
    let before = store_state(&f);
    let destination = f.dir.path().join("new-archive.json");
    let metadata = success(export(&f, &destination));
    let bytes = fs::read(&destination).unwrap();
    let checked = archive::parse(&bytes, Limits::default(), &d()).unwrap();
    assert_eq!(checked.snapshot().id, f.snapshot.id);
    assert_ne!(f.original.id, f.snapshot.id);
    assert_eq!(
        serde_json::to_value(checked.canonical_graph()).unwrap(),
        serde_json::to_value(&f.graph).unwrap()
    );
    assert!(
        checked
            .view()
            .meaning
            .as_ref()
            .unwrap()
            .packets
            .iter()
            .any(
                |packet| packet.provenance.original_snapshot_id == f.original.id
                    && packet.provenance.snapshot_id == f.snapshot.id
            )
    );
    assert!(
        !Path::new(&f.provider.args[0]).exists(),
        "The owned worker is removed before archive commands"
    );
    assert_eq!(
        serde_json::to_value(checked.view()).unwrap(),
        serde_json::to_value(&f.view).unwrap()
    );
    assert_eq!(metadata["kind"], "typed_meaning_view");
    assert_eq!(metadata["archiveId"], checked.archive_id());
    assert_eq!(metadata["contentHash"], hash(&bytes));
    assert_eq!(metadata["wireBytes"], bytes.len());
    assert_eq!(metadata["viewId"], f.view.id);
    assert_eq!(metadata["outputCreated"], true);
    assert_eq!(metadata["meaning"]["providerAttestation"], "unverified");
    assert_eq!(metadata["executionAuthority"], "none");
    assert_eq!(store_state(&f), before);
    assert_eq!(fs::read_to_string(&f.marker).unwrap().lines().count(), 3);
}

#[test]
fn inspect_supplied_bytes_is_storeless_and_emits_unknown_metadata_without_quotes() {
    let f = fixture(AnalyzerEpoch::V8);
    let input = input(&f);
    let bytes = fs::read(&input).unwrap();
    let view_id = f.view.id.clone();
    let marker = f.marker.clone();
    let calls = fs::read(&marker).unwrap();
    let _retained_fixture = remove_source_workspace(f);
    let cwd = TempDir::new().unwrap();
    let output = command(cwd.path())
        .args(["inspect", "--input"])
        .arg(&input)
        .args(["--max-bytes", "1024"])
        .output()
        .unwrap();
    assert!(output.stdout.len() <= 1024);
    let text = String::from_utf8_lossy(&output.stdout).to_string();
    let metadata = success(output);
    assert_eq!(metadata["archiveId"], hash(&bytes));
    assert_eq!(metadata["viewId"], view_id);
    assert_eq!(metadata["meaning"]["packetCount"], 3);
    assert_eq!(metadata["meaning"]["unknownCostPacketCount"], 3);
    assert_eq!(metadata["meaning"]["unknownUsagePacketCount"], 3);
    for forbidden in [
        "Storage policy",
        "exact café evidence",
        "native_model_attempt_v1",
        "sourceObjects",
    ] {
        assert!(
            !text.contains(forbidden),
            "Inspect leaked complete proof/source content"
        );
    }
    assert!(!cwd.path().join(".yoke").exists());
    assert_eq!(fs::read(marker).unwrap(), calls);
}

#[test]
fn typed_registration_cas_and_load_keep_exact_bytes_after_all_original_state_is_removed() {
    let f = fixture(AnalyzerEpoch::V9);
    let input = input(&f);
    let expected_bytes = fs::read(&input).unwrap();
    let registry_dir = TempDir::new().unwrap();
    let cwd = TempDir::new().unwrap();
    let metadata = success(
        register(cwd.path(), registry_dir.path(), &input, "typed", None)
            .output()
            .unwrap(),
    );
    let id = metadata["registrationId"].as_str().unwrap().to_owned();
    assert!(valid_id(&id));
    let before_stale = registry_state(registry_dir.path());
    assert!(
        !register(cwd.path(), registry_dir.path(), &input, "typed", None)
            .output()
            .unwrap()
            .status
            .success()
    );
    assert!(
        !register(
            cwd.path(),
            registry_dir.path(),
            &input,
            "typed",
            Some(&"0".repeat(64))
        )
        .output()
        .unwrap()
        .status
        .success()
    );
    assert_eq!(registry_state(registry_dir.path()), before_stale);
    let same = success(
        register(cwd.path(), registry_dir.path(), &input, "typed", Some(&id))
            .output()
            .unwrap(),
    );
    assert_eq!(same["registrationId"], id);
    let _retained_fixture = remove_source_workspace(f);
    fs::remove_file(&input).unwrap();
    let destination = cwd.path().join("loaded.json");
    let db_before = fs::read(registry_dir.path().join("yoke-collection.sqlite")).unwrap();
    let loaded = success(
        command(cwd.path())
            .args(load_prefix())
            .arg(registry_dir.path())
            .arg("--output")
            .arg(&destination)
            .output()
            .unwrap(),
    );
    assert_eq!(loaded["archiveId"], hash(&expected_bytes));
    assert_eq!(fs::read(&destination).unwrap(), expected_bytes);
    let metadata_only = success(
        command(cwd.path())
            .args(load_prefix())
            .arg(registry_dir.path())
            .output()
            .unwrap(),
    );
    assert_eq!(metadata_only["archiveId"], loaded["archiveId"]);
    assert_eq!(registry_state(registry_dir.path()), before_stale);
    assert_eq!(
        fs::read(registry_dir.path().join("yoke-collection.sqlite")).unwrap(),
        db_before
    );
    assert!(!cwd.path().join(".yoke").exists());
}

#[test]
fn permission_and_explicit_cas_precede_any_input_or_registry_creation() {
    let cwd = TempDir::new().unwrap();
    let registry = TempDir::new().unwrap();
    let missing = cwd.path().join("missing-input.json");
    let denied = command(cwd.path())
        .args(["register", "typed", "--registry"])
        .arg(registry.path())
        .arg("--input")
        .arg(&missing)
        .arg("--expect-absent")
        .output()
        .unwrap();
    assert!(!denied.status.success());
    assert!(String::from_utf8_lossy(&denied.stderr).contains("--allow-registration"));
    for extra in [vec![], vec!["--expect-absent", "--expected", "bad"]] {
        let output = command(cwd.path())
            .args(["register", "typed", "--registry"])
            .arg(registry.path())
            .arg("--input")
            .arg(&missing)
            .arg("--allow-registration")
            .args(extra)
            .output()
            .unwrap();
        assert!(!output.status.success());
    }
    assert_eq!(fs::read_dir(registry.path()).unwrap().count(), 0);
    assert!(!cwd.path().join(".yoke").exists());
}

#[test]
fn invalid_budgets_and_missing_existing_store_fail_before_filesystem_initialization() {
    let cwd = TempDir::new().unwrap();
    let source = cwd.path().join("missing-source");
    let destination = cwd.path().join("never-created.json");
    for args in [
        vec!["--max-bytes", "1"],
        vec!["--max-wire-bytes", "0"],
        vec!["--max-source-bytes", "64000001"],
        vec!["--timeout", "0"],
        vec![],
    ] {
        let output = command(cwd.path())
            .args(["export", &"a".repeat(64), "--root"])
            .arg(&source)
            .arg("--output")
            .arg(&destination)
            .args(args)
            .output()
            .unwrap();
        assert!(!output.status.success());
        assert!(!source.exists());
        assert!(!destination.exists());
    }
    let registry = TempDir::new().unwrap();
    let output = register(
        cwd.path(),
        registry.path(),
        &cwd.path().join("missing.json"),
        "typed",
        None,
    )
    .args(["--max-bytes", "1"])
    .output()
    .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("output budget"));
    assert_eq!(fs::read_dir(registry.path()).unwrap().count(), 0);
}

#[test]
fn duplicate_malformed_and_over_budget_bytes_cannot_initialize_a_registry() {
    let f = fixture(AnalyzerEpoch::V8);
    let input = input(&f);
    let bytes = fs::read(&input).unwrap();
    success(
        command(f.dir.path())
            .args(["inspect", "--input"])
            .arg(&input)
            .output()
            .unwrap(),
    );
    let duplicate = [
        b"{\"profile\":\"meaning_view_archive_v1\",".as_slice(),
        &bytes[1..],
    ]
    .concat();
    for (payload, budget) in [
        (b"{}".to_vec(), 32_000_000usize),
        (duplicate, 32_000_000),
        (bytes.clone(), bytes.len() - 1),
    ] {
        let bad = f.dir.path().join("bad.json");
        fs::write(&bad, payload).unwrap();
        let registry = TempDir::new().unwrap();
        let output = register(f.dir.path(), registry.path(), &bad, "typed", None)
            .args(["--max-wire-bytes", &budget.to_string()])
            .output()
            .unwrap();
        assert!(!output.status.success());
        assert_eq!(fs::read_dir(registry.path()).unwrap().count(), 0);
    }
}

#[test]
fn source_provider_flags_are_rejected_without_reading_selected_files() {
    let cwd = TempDir::new().unwrap();
    let missing = cwd.path().join("missing.json");
    for extra in [
        vec!["--trust-servers"],
        vec!["--allow-writes"],
        vec!["--analyzer-epoch", "9"],
        vec!["--include-ignored-dir", "generated"],
    ] {
        let output = command(cwd.path())
            .args(["inspect", "--input"])
            .arg(&missing)
            .args(extra)
            .output()
            .unwrap();
        assert!(!output.status.success());
        let error = String::from_utf8_lossy(&output.stderr);
        assert!(
            error.contains("capture") || error.contains("provider"),
            "{error}"
        );
    }
    assert!(!cwd.path().join(".yoke").exists());
}

#[test]
fn existing_output_file_directory_and_hardlink_are_never_overwritten() {
    let f = fixture(AnalyzerEpoch::V9);
    let marker = f.dir.path().join("human-note.txt");
    fs::write(&marker, b"preserve human bytes").unwrap();
    let hardlink = f.dir.path().join("hardlink.json");
    fs::hard_link(&marker, &hardlink).unwrap();
    let directory = f.dir.path().join("existing-directory");
    fs::create_dir(&directory).unwrap();
    let before = store_state(&f);
    for path in [&marker, &hardlink, &directory] {
        assert!(!export(&f, path).status.success());
    }
    assert_eq!(fs::read(marker).unwrap(), b"preserve human bytes");
    assert_eq!(store_state(&f), before);
    #[cfg(windows)]
    {
        let absent = f.dir.path().join("must-stay-absent.json");
        let link = f.dir.path().join("dangling.json");
        std::os::windows::fs::symlink_file(&absent, &link).unwrap();
        assert!(!export(&f, &link).status.success());
        assert!(!absent.exists());
        let directory_link = f.dir.path().join("linked-directory");
        std::os::windows::fs::symlink_dir(&directory, &directory_link).unwrap();
        assert!(
            !export(&f, &directory_link.join("must-not-be-created.json"))
                .status
                .success()
        );
        assert!(!directory.join("must-not-be-created.json").exists());
    }
}

#[test]
fn stdout_loss_reports_already_durable_registration_and_output_archive_identities() {
    let f = fixture(AnalyzerEpoch::V9);
    let input = input(&f);
    let registry_dir = TempDir::new().unwrap();
    let mut request = register(f.dir.path(), registry_dir.path(), &input, "durable", None);
    request.stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = request.spawn().unwrap();
    drop(child.stdout.take());
    let result = child.wait_with_output().unwrap();
    assert!(!result.status.success());
    let registry = Registry::open_existing_for_host(
        registry_dir.path(),
        cfg!(unix),
        &yoke::capture::Deadline::new(30_000),
    )
    .unwrap();
    let archive = registry
        .load_meaning_archive("durable", Limits::default(), &d())
        .unwrap();
    let metadata = registry.describe("durable", &d()).unwrap();
    let registration = metadata["registrationId"].as_str().unwrap();
    let error = String::from_utf8_lossy(&result.stderr);
    assert!(
        error.contains("durable")
            && error.contains(registration)
            && error.contains(archive.archive_id()),
        "{error}"
    );
    drop(registry);
    let destination = f.dir.path().join("durable-output.json");
    let mut request = command(f.dir.path());
    request
        .args(["export", &f.view.id, "--root"])
        .arg(&f.engine.root)
        .arg("--output")
        .arg(&destination)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = request.spawn().unwrap();
    drop(child.stdout.take());
    let result = child.wait_with_output().unwrap();
    assert!(!result.status.success());
    let bytes = fs::read(&destination).expect("Complete published output survives stdout loss");
    let checked = archive::parse(&bytes, Limits::default(), &d()).unwrap();
    let error = String::from_utf8_lossy(&result.stderr);
    assert!(
        error.contains("durable")
            && error.contains(checked.archive_id())
            && error.contains(destination.to_str().unwrap()),
        "{error}"
    );
    assert_eq!(fs::read_to_string(f.marker).unwrap().lines().count(), 3);
}

#[test]
fn export_refuses_incomplete_store_without_initializing_any_state() {
    // These databases satisfy the existing basic schema/workspace reader. The
    // failure oracle catches constructor initialization before missing-View refusal.
    fn inventory(root: &Path) -> std::collections::BTreeMap<String, Option<String>> {
        fn visit(
            base: &Path,
            path: &Path,
            result: &mut std::collections::BTreeMap<String, Option<String>>,
        ) {
            for entry in fs::read_dir(path).unwrap() {
                let path = entry.unwrap().path();
                let key = path
                    .strip_prefix(base)
                    .unwrap()
                    .to_str()
                    .unwrap()
                    .to_owned();
                let metadata = fs::symlink_metadata(&path).unwrap();
                assert!(!yoke::paths::is_link(&metadata));
                if metadata.is_dir() {
                    result.insert(key, None);
                    visit(base, &path, result);
                } else {
                    result.insert(key, Some(hash(fs::read(path).unwrap())));
                }
            }
        }
        let mut result = std::collections::BTreeMap::new();
        visit(root, root, &mut result);
        result
    }
    let mut mutations = Vec::new();
    for missing in [
        "all",
        "blobs",
        "trees",
        "maintenance.lock",
        "clients.lock",
        "journal_mode",
    ] {
        let cwd = TempDir::new().unwrap();
        let source = cwd.path().join("existing-basic-workspace");
        let state = source.join(".yoke");
        fs::create_dir_all(&state).unwrap();
        let db = rusqlite::Connection::open(state.join("index.sqlite")).unwrap();
        db.execute_batch("CREATE TABLE meta(key TEXT PRIMARY KEY,value TEXT NOT NULL);
            CREATE TABLE snapshots(id TEXT PRIMARY KEY,value TEXT NOT NULL);
            CREATE TABLE graphs(id TEXT PRIMARY KEY,value TEXT NOT NULL);
            CREATE TABLE cache(key TEXT PRIMARY KEY,value TEXT NOT NULL);
            CREATE TABLE records(kind TEXT NOT NULL,id TEXT NOT NULL,value TEXT NOT NULL,PRIMARY KEY(kind,id));
            CREATE VIRTUAL TABLE symbols USING fts5(snapshot UNINDEXED,id UNINDEXED,name,path UNINDEXED,signature);
            CREATE VIRTUAL TABLE documents USING fts5(snapshot UNINDEXED,path UNINDEXED,text);")
            .unwrap();
        db.execute("INSERT INTO meta VALUES('workspace',?1)", ["a".repeat(64)])
            .unwrap();
        if missing != "all" && missing != "journal_mode" {
            db.execute_batch("PRAGMA journal_mode=WAL").unwrap();
        }
        drop(db);
        if missing != "all" {
            for name in ["blobs", "trees"] {
                if missing != name {
                    fs::create_dir(state.join(name)).unwrap();
                }
            }
            for name in ["maintenance.lock", "clients.lock"] {
                if missing != name {
                    fs::write(state.join(name), []).unwrap();
                }
            }
        }
        let before = inventory(&source);
        let destination = cwd.path().join("never-published.json");
        let result = command(cwd.path())
            .args(["export", &"b".repeat(64), "--root"])
            .arg(&source)
            .arg("--output")
            .arg(&destination)
            .output()
            .unwrap();
        assert!(!result.status.success(), "Missing state variant {missing}");
        let after = inventory(&source);
        if after != before {
            mutations.push(format!("{missing}: before {before:?}; after {after:?}"));
        }
        assert!(!destination.exists());
    }
    assert!(
        mutations.is_empty(),
        "Failed export initialized incomplete state: {}",
        mutations.join("\n")
    );
}

fn load_prefix() -> &'static [&'static str] {
    if cfg!(unix) {
        &[
            "load",
            "typed",
            "--allow-registry-coordination",
            "--registry",
        ]
    } else {
        &["load", "typed", "--registry"]
    }
}
