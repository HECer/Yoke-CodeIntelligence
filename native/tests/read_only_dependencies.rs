use serde_json::json;
use std::fs;
use yoke::{
    capture::{self, CaptureOptions, Deadline},
    store::Store,
};

#[test]
fn explicit_readonly_dependency_capture_keeps_secret_state_and_size_protections() {
    let root = tempfile::tempdir().unwrap();
    for (path, text) in [
        ("src/main.rs", "fn actual() {}\n"),
        ("node_modules/pkg/index.js", "export function dep() {}\n"),
        ("target/generated.rs", "fn generated() {}\n"),
        ("node_modules/pkg/.env", "private environment"),
        ("target/.yoke/state.json", "{}"),
        ("node_modules/pkg/credentials.json", "private credentials"),
    ] {
        let absolute = root.path().join(path);
        fs::create_dir_all(absolute.parent().unwrap()).unwrap();
        fs::write(absolute, text).unwrap();
    }
    let store = Store::open(root.path()).unwrap();
    let ordinary = capture::capture(root.path(), &store, &Deadline::new(5000)).unwrap();
    assert_eq!(ordinary.files.len(), 1);
    let options: CaptureOptions =
        serde_json::from_value(json!({"readOnlyDependencyDirs":["node_modules","target"]}))
            .expect("Typed host read-only dependency scope");
    let snapshot =
        capture::capture_with_options(root.path(), &store, &options, &Deadline::new(5000)).unwrap();
    assert_eq!(
        snapshot
            .files
            .iter()
            .map(|file| file.path.as_str())
            .collect::<Vec<_>>(),
        [
            "node_modules/pkg/index.js",
            "src/main.rs",
            "target/generated.rs"
        ]
    );
    assert_ne!(snapshot.id, ordinary.id);
    assert!(
        capture::source_path_policy(&store, &snapshot, "node_modules/pkg/index.js")
            .unwrap()
            .read_only
    );
    assert!(
        capture::source_path_policy(&store, &snapshot, "target/generated.rs")
            .unwrap()
            .read_only
    );
    assert!(
        !capture::source_path_policy(&store, &snapshot, "src/main.rs")
            .unwrap()
            .read_only
    );
    assert!(capture::source_path_policy(&store, &snapshot, "node_modules/pkg/.env").is_err());
    let mut tampered = snapshot.clone();
    tampered.files[0].hash = "f".repeat(64);
    assert!(capture::source_path_policy(&store, &tampered, "node_modules/pkg/index.js").is_err());
    let mut unknown = snapshot.clone();
    unknown.id = "0".repeat(64);
    assert!(capture::source_path_policy(&store, &unknown, "node_modules/pkg/index.js").is_err());
    assert!(
        snapshot
            .omissions
            .iter()
            .any(|omission| omission.path.ends_with(".env") && omission.reason == "protected-path")
    );
    assert!(yoke::paths::normalized("node_modules/pkg/index.js").is_err());
    assert!(yoke::paths::normalized("target/generated.rs").is_err());
}
#[test]
fn empty_dependency_option_preserves_version_one_identity_and_rejects_other_protected_scopes() {
    assert_eq!(
        serde_json::to_value(CaptureOptions::default()).unwrap(),
        json!({"onlyDirs":[],"includeDirs":[],"languageOverrides":{}})
    );
    for directory in [
        ".git",
        ".yoke",
        ".env",
        "../target",
        "target/nested",
        "vendor",
        ".ssh",
    ] {
        let options: CaptureOptions =
            serde_json::from_value(json!({"readOnlyDependencyDirs":[directory]})).unwrap();
        assert!(options.validated().is_err(), "accepted {directory}");
    }
}
#[test]
fn real_git_ignored_dependencies_stay_ignored_under_the_readonly_host_scope() {
    let root = tempfile::tempdir().unwrap();
    let output = std::process::Command::new("git")
        .args(["init", "--quiet"])
        .arg(root.path())
        .output()
        .unwrap();
    assert!(output.status.success());
    fs::write(root.path().join(".gitignore"), "node_modules/\n").unwrap();
    fs::create_dir_all(root.path().join("node_modules/pkg")).unwrap();
    fs::write(
        root.path().join("node_modules/pkg/index.js"),
        "export const ignored = true;\n",
    )
    .unwrap();
    let store = Store::open(root.path()).unwrap();
    let options: CaptureOptions =
        serde_json::from_value(json!({"readOnlyDependencyDirs":["node_modules"]})).unwrap();
    let snapshot =
        capture::capture_with_options(root.path(), &store, &options, &Deadline::new(5000)).unwrap();
    assert!(
        !snapshot
            .files
            .iter()
            .any(|file| file.path.starts_with("node_modules/"))
    );
    assert!(capture::source_path_policy(&store, &snapshot, "node_modules/pkg/index.js").is_err());
}

#[test]
fn derived_dependency_members_require_exact_origin_proof_and_remain_virtual() {
    let root = tempfile::tempdir().unwrap();
    fs::create_dir_all(root.path().join("node_modules/pkg")).unwrap();
    fs::write(
        root.path().join("node_modules/pkg/index.js"),
        "export const dep = 1;\n",
    )
    .unwrap();
    fs::write(root.path().join("main.rs"), "fn main() {}\n").unwrap();
    let store = Store::open(root.path()).unwrap();
    let options: CaptureOptions =
        serde_json::from_value(json!({"readOnlyDependencyDirs":["node_modules"]})).unwrap();
    let origin =
        capture::capture_with_options(root.path(), &store, &options, &Deadline::new(5000)).unwrap();
    store
        .db
        .execute(
            "INSERT INTO snapshots VALUES(?1,?2)",
            rusqlite::params![origin.id, serde_json::to_string(&origin).unwrap()],
        )
        .unwrap();
    let mut derived = origin.clone();
    derived.id = "a".repeat(64);
    derived.kind = "derived".into();
    store
        .db
        .execute(
            "INSERT INTO snapshots VALUES(?1,?2)",
            rusqlite::params![derived.id, serde_json::to_string(&derived).unwrap()],
        )
        .unwrap();
    assert!(capture::source_path_policy(&store, &derived, "node_modules/pkg/index.js").is_err());
    capture::inherit_options(&store, &origin, &derived).unwrap();
    assert!(
        capture::source_path_policy(&store, &derived, "node_modules/pkg/index.js")
            .unwrap()
            .read_only
    );
    assert!(
        capture::source_path_policy(&store, &derived, "main.rs")
            .unwrap()
            .read_only
    );
    let mut federated = derived.clone();
    federated.id = "b".repeat(64);
    federated.kind = "federated".into();
    store
        .db
        .execute(
            "INSERT INTO snapshots VALUES(?1,?2)",
            rusqlite::params![federated.id, serde_json::to_string(&federated).unwrap()],
        )
        .unwrap();
    assert!(capture::source_path_policy(&store, &federated, "node_modules/pkg/index.js").is_err());
    store
        .db
        .execute(
            "DELETE FROM records WHERE kind='native_capture_options_v1' AND id=?1",
            [&origin.id],
        )
        .unwrap();
    assert!(capture::source_path_policy(&store, &derived, "node_modules/pkg/index.js").is_err());
}

#[test]
fn prospective_read_admission_is_scoped_metadata_and_cannot_read_uncaptured_bytes() {
    let root = tempfile::tempdir().unwrap();
    fs::create_dir_all(root.path().join("src")).unwrap();
    fs::write(root.path().join("src/main.rs"), "fn main() {}\n").unwrap();
    let store = Store::open(root.path()).unwrap();
    let options: CaptureOptions = serde_json::from_value(json!({"readOnlyDependencyDirs":["node_modules"],"onlyDirs":["src","node_modules"],"languageOverrides":{"custom":"rust"}})).unwrap();
    let origin =
        capture::capture_with_options(root.path(), &store, &options, &Deadline::new(5000)).unwrap();
    let (language, policy) =
        capture::captured_read_path_policy(&store, &origin, "node_modules/new/added.custom")
            .unwrap();
    assert_eq!(language, "rust");
    assert!(policy.read_only);
    assert_eq!(
        capture::source_scope_policy(&store, &origin)
            .unwrap()
            .prospective_path_policy("node_modules/new/added.custom")
            .unwrap(),
        (language, policy)
    );
    assert!(capture::source_path_policy(&store, &origin, "node_modules/new/added.custom").is_err());
    for path in [
        "outside.rs",
        "target/new.rs",
        "node_modules/.aws/config",
        "node_modules/pkg/.env",
        "node_modules/../src/main.rs",
    ] {
        assert!(
            capture::captured_read_path_policy(&store, &origin, path).is_err(),
            "accepted {path}"
        );
    }
    let mut foreign = origin.clone();
    foreign.id = "c".repeat(64);
    foreign.kind = "federated".into();
    store
        .db
        .execute(
            "INSERT INTO snapshots VALUES(?1,?2)",
            rusqlite::params![foreign.id, serde_json::to_string(&foreign).unwrap()],
        )
        .unwrap();
    assert!(capture::captured_read_path_policy(&store, &foreign, "src/new.rs").is_err());
}

#[test]
fn dependency_sources_retain_file_size_binary_and_secret_content_omissions() {
    let root = tempfile::tempdir().unwrap();
    fs::create_dir_all(root.path().join("target")).unwrap();
    fs::File::create(root.path().join("target/large.rs"))
        .unwrap()
        .set_len(4_000_001)
        .unwrap();
    fs::write(root.path().join("target/binary.rs"), b"fn fake() {}\0").unwrap();
    fs::write(
        root.path().join("target/secret.rs"),
        "-----BEGIN PRIVATE KEY-----",
    )
    .unwrap();
    let store = Store::open(root.path()).unwrap();
    let options: CaptureOptions =
        serde_json::from_value(json!({"readOnlyDependencyDirs":["target"]})).unwrap();
    let snapshot =
        capture::capture_with_options(root.path(), &store, &options, &Deadline::new(5000)).unwrap();
    assert!(snapshot.files.is_empty());
    for (path, reason) in [
        ("target/large.rs", "file-size-limit"),
        ("target/binary.rs", "binary-or-invalid-utf8"),
        ("target/secret.rs", "obvious-secret"),
    ] {
        assert!(
            snapshot
                .omissions
                .iter()
                .any(|omission| omission.path == path && omission.reason == reason),
            "missing {path}: {reason}"
        );
        assert!(capture::source_path_policy(&store, &snapshot, path).is_err());
    }
}

#[test]
fn materialized_language_server_tree_reads_proven_dependency_members_only() {
    let root = tempfile::tempdir().unwrap();
    fs::create_dir_all(root.path().join("node_modules/pkg")).unwrap();
    let content = "export const dep = 1;\n";
    fs::write(root.path().join("node_modules/pkg/index.js"), content).unwrap();
    let store = Store::open(root.path()).unwrap();
    let options: CaptureOptions =
        serde_json::from_value(json!({"readOnlyDependencyDirs":["node_modules"]})).unwrap();
    let snapshot =
        capture::capture_with_options(root.path(), &store, &options, &Deadline::new(5000)).unwrap();
    let target = tempfile::tempdir().unwrap();
    yoke::changes::materialize(&store, &snapshot, target.path(), &Deadline::new(5000)).unwrap();
    assert_eq!(
        fs::read_to_string(target.path().join("node_modules/pkg/index.js")).unwrap(),
        content
    );
    let mut tampered = snapshot.clone();
    tampered.files[0].path = "node_modules/pkg/.env".into();
    let rejected = tempfile::tempdir().unwrap();
    assert!(
        yoke::changes::materialize(&store, &tampered, rejected.path(), &Deadline::new(5000))
            .is_err()
    );
    assert!(!rejected.path().join("node_modules").exists());
}

#[test]
fn an_unknown_capture_epoch_cannot_grant_dependency_source_authority() {
    let root = tempfile::tempdir().unwrap();
    fs::create_dir_all(root.path().join("node_modules/pkg")).unwrap();
    fs::write(
        root.path().join("node_modules/pkg/index.js"),
        "export const dep = 1;\n",
    )
    .unwrap();
    let store = Store::open(root.path()).unwrap();
    let options: CaptureOptions =
        serde_json::from_value(json!({"readOnlyDependencyDirs":["node_modules"]})).unwrap();
    let mut snapshot =
        capture::capture_with_options(root.path(), &store, &options, &Deadline::new(5000)).unwrap();
    let mut record: capture::CaptureOptionsRecord = store
        .get_record(capture::OPTIONS_RECORD_KIND, &snapshot.id)
        .unwrap();
    record.analyzer = "unknown-future-epoch".into();
    snapshot.id = yoke::model::identity(&(
        &store.workspace_id,
        "disk",
        &record.analyzer,
        &snapshot.files,
        &snapshot.omissions,
        &record.options_hash,
    ));
    record.snapshot_id = snapshot.id.clone();
    store
        .record(capture::OPTIONS_RECORD_KIND, &snapshot.id, &record)
        .unwrap();
    assert!(capture::source_path_policy(&store, &snapshot, "node_modules/pkg/index.js").is_err());
    assert!(
        capture::captured_read_path_policy(&store, &snapshot, "node_modules/pkg/new.js").is_err()
    );
}

#[test]
fn editor_overlay_inherits_native_capture_scope_without_dependency_write_access() {
    let root = tempfile::tempdir().unwrap();
    fs::create_dir_all(root.path().join("node_modules/pkg")).unwrap();
    fs::create_dir_all(root.path().join("src")).unwrap();
    fs::write(
        root.path().join("node_modules/pkg/index.js"),
        "export const dep = 1;\n",
    )
    .unwrap();
    fs::write(root.path().join("src/main.custom"), "fn saved() {}\n").unwrap();
    let mut engine = yoke::Engine::open(root.path()).unwrap();
    engine.capture_options = serde_json::from_value(json!({"readOnlyDependencyDirs":["node_modules"],"onlyDirs":["src","node_modules"],"languageOverrides":{"custom":"rust"}})).unwrap();
    let deadline = Deadline::new(20000);
    let session = yoke::editor::open(&mut engine, &deadline).unwrap();
    yoke::editor::update(
        &engine,
        &session.id,
        &json!({"path":"src/main.custom","version":1,"text":"fn unsaved() {}\n"}),
        &deadline,
    )
    .unwrap();
    assert!(yoke::editor::update(&engine, &session.id, &json!({"path":"node_modules/pkg/index.js","version":1,"text":"export const injected = 1;\n"}), &deadline).is_err());
    let snapshot = yoke::editor::capture(&mut engine, &session.id, &deadline).unwrap();
    assert_eq!(
        snapshot
            .files
            .iter()
            .find(|file| file.path == "src/main.custom")
            .unwrap()
            .language,
        "rust"
    );
    let scope = capture::source_scope_policy(&engine.store, &snapshot).unwrap();
    assert!(
        scope
            .path_policy("node_modules/pkg/index.js")
            .unwrap()
            .read_only
    );
    assert_eq!(
        String::from_utf8(scope.read_source("src/main.custom").unwrap()).unwrap(),
        "fn unsaved() {}\n"
    );
}

#[test]
fn authentic_legacy_overlay_receipt_proves_readonly_source_without_a_live_session() {
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("main.rs"), "fn saved() {}\n").unwrap();
    let store = Store::open(root.path()).unwrap();
    let mut origin = capture::capture(root.path(), &store, &Deadline::new(5000)).unwrap();
    origin.id = yoke::model::identity(&(
        &store.workspace_id,
        "disk",
        "yoke-tree-sitter-4",
        &origin.files,
        &origin.omissions,
    ));
    store
        .db
        .execute(
            "INSERT INTO snapshots VALUES(?1,?2)",
            rusqlite::params![origin.id, serde_json::to_string(&origin).unwrap()],
        )
        .unwrap();
    let mut overlay = origin.clone();
    overlay.kind = "overlay".into();
    overlay.capture_consistency = "overlay-from-stable-disk-and-immutable-buffers".into();
    let content = b"fn historical_unsaved() {}\n";
    overlay.files[0].hash = store.blob(content).unwrap();
    overlay.files[0].size = content.len();
    let session_id = "e".repeat(64);
    let versions = std::collections::BTreeMap::from([("main.rs".to_owned(), 1u32)]);
    overlay.id = yoke::model::identity(&(
        "yoke-tree-sitter-4",
        &origin.id,
        &session_id,
        1u64,
        &versions,
        &overlay.files,
    ));
    store
        .db
        .execute(
            "INSERT INTO snapshots VALUES(?1,?2)",
            rusqlite::params![overlay.id, serde_json::to_string(&overlay).unwrap()],
        )
        .unwrap();
    let receipt = json!({"snapshotId":overlay.id,"sessionId":session_id,"revision":1,"versions":versions,"baseSnapshotId":origin.id});
    store
        .record("overlay-snapshot", &overlay.id, &receipt)
        .unwrap();
    let scope = capture::source_scope_policy(&store, &overlay).unwrap();
    assert!(scope.path_policy("main.rs").unwrap().read_only);
    assert_eq!(scope.read_source("main.rs").unwrap(), content);
    let mut invalid = receipt;
    invalid["futureGrant"] = json!(true);
    store
        .record("overlay-snapshot", &overlay.id, &invalid)
        .unwrap();
    assert!(capture::source_scope_policy(&store, &overlay).is_err());
}

#[test]
fn passive_provider_materialization_can_omit_exact_source_selectors_without_changing_capture() {
    let root = tempfile::tempdir().unwrap();
    fs::create_dir_all(root.path().join(".cargo")).unwrap();
    let selector = "[build]\nrustc = 'source-owned-selector.exe'\n";
    fs::write(root.path().join(".cargo/config.toml"), selector).unwrap();
    fs::write(root.path().join("main.rs"), "fn main() {}\n").unwrap();
    let store = Store::open(root.path()).unwrap();
    let deadline = Deadline::new(10000);
    let snapshot = capture::capture(root.path(), &store, &deadline).unwrap();
    let before = serde_json::to_value(&snapshot).unwrap();
    let omitted = std::collections::BTreeSet::from([".cargo/config.toml".to_owned()]);
    let destination = tempfile::tempdir().unwrap();
    yoke::changes::materialize_selected_sources(
        &store,
        &snapshot,
        destination.path(),
        &omitted,
        &deadline,
    )
    .unwrap();
    assert!(destination.path().join("main.rs").is_file());
    assert!(!destination.path().join(".cargo/config.toml").exists());
    assert_eq!(serde_json::to_value(&snapshot).unwrap(), before);
    assert_eq!(
        String::from_utf8(
            capture::source_scope_policy(&store, &snapshot)
                .unwrap()
                .read_source(".cargo/config.toml")
                .unwrap()
        )
        .unwrap(),
        selector
    );
    let ordinary = tempfile::tempdir().unwrap();
    yoke::changes::materialize(&store, &snapshot, ordinary.path(), &deadline).unwrap();
    assert_eq!(
        fs::read_to_string(ordinary.path().join(".cargo/config.toml")).unwrap(),
        selector
    );
    let rejected = destination.path().join("rejected");
    assert!(
        yoke::changes::materialize_selected_sources(
            &store,
            &snapshot,
            &rejected,
            &std::collections::BTreeSet::from(["../escape.rs".into()]),
            &deadline
        )
        .is_err()
    );
    assert!(!rejected.exists());
}
