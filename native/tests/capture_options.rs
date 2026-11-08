use std::{collections::BTreeMap, fs};
use yoke::{
    capture::{CaptureOptions, Deadline, capture, capture_with_options},
    store::Store,
};

#[test]
fn scoped_capture_uses_directory_segments_and_keeps_protections() {
    let root = tempfile::tempdir().unwrap();
    for path in ["src/a.rs", "src/.env", "src-extra/b.rs", "docs/a.md"] {
        let absolute = root.path().join(path);
        fs::create_dir_all(absolute.parent().unwrap()).unwrap();
        fs::write(absolute, "fn sample(){}\n").unwrap();
    }
    let store = Store::open(root.path()).unwrap();
    let options = CaptureOptions {
        only_dirs: vec!["src".into()],
        ..Default::default()
    };
    let snapshot =
        capture_with_options(root.path(), &store, &options, &Deadline::new(5000)).unwrap();
    assert_eq!(
        snapshot
            .files
            .iter()
            .map(|f| f.path.as_str())
            .collect::<Vec<_>>(),
        ["src/a.rs"]
    );
    assert!(
        snapshot
            .omissions
            .iter()
            .any(|o| o.path == "src/.env" && o.reason == "protected-path")
    );
}

#[test]
fn language_overrides_are_host_typed_and_change_immutable_identity() {
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("a.custom"), "fn sample(){}\n").unwrap();
    let store = Store::open(root.path()).unwrap();
    let baseline = capture(root.path(), &store, &Deadline::new(5000)).unwrap();
    let options = CaptureOptions {
        language_overrides: BTreeMap::from([("custom".into(), "rust".into())]),
        ..Default::default()
    };
    let snapshot =
        capture_with_options(root.path(), &store, &options, &Deadline::new(5000)).unwrap();
    assert_eq!(snapshot.files[0].language, "rust");
    assert_ne!(baseline.id, snapshot.id);
    assert_eq!(baseline.files[0].hash, snapshot.files[0].hash);
    assert_eq!(
        yoke::capture::validate_language(&store, &snapshot, "a.custom").unwrap(),
        "rust"
    );
    let mut tampered = snapshot.clone();
    tampered.files[0].language = "python".into();
    assert!(yoke::capture::validate_language(&store, &tampered, "a.custom").is_err());
    let mut tampered = snapshot.clone();
    tampered.files[0].hash = "f".repeat(64);
    assert!(yoke::capture::validate_language(&store, &tampered, "a.custom").is_err());
    assert!(
        serde_json::from_str::<CaptureOptions>(
            r#"{"languageOverrides":{"custom":"rust"},"allowSecrets":true}"#
        )
        .is_err()
    );
}

#[test]
fn unsafe_scope_and_unknown_language_fail_before_capture() {
    let root = tempfile::tempdir().unwrap();
    let store = Store::open(root.path()).unwrap();
    for path in ["../other", "C:/other", "src/../other", "/other"] {
        let options = CaptureOptions {
            only_dirs: vec![path.into()],
            ..Default::default()
        };
        assert!(
            capture_with_options(root.path(), &store, &options, &Deadline::new(5000)).is_err(),
            "{path}"
        );
    }
    let options = CaptureOptions {
        language_overrides: BTreeMap::from([("x".into(), "made-up".into())]),
        ..Default::default()
    };
    assert!(capture_with_options(root.path(), &store, &options, &Deadline::new(5000)).is_err());
}
#[test]
fn derived_snapshot_preserves_only_proven_capture_options() {
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("a.custom"), "fn original(){}\n").unwrap();
    let store = Store::open(root.path()).unwrap();
    let options = CaptureOptions {
        language_overrides: BTreeMap::from([("custom".into(), "rust".into())]),
        ..Default::default()
    };
    let origin = capture_with_options(root.path(), &store, &options, &Deadline::new(5000)).unwrap();
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
    derived.files[0].hash = store.blob(b"fn replaced(){}\n").unwrap();
    derived.files[0].size = 16;
    store
        .db
        .execute(
            "INSERT INTO snapshots VALUES(?1,?2)",
            rusqlite::params![derived.id, serde_json::to_string(&derived).unwrap()],
        )
        .unwrap();
    yoke::capture::inherit_options(&store, &origin, &derived).unwrap();
    assert_eq!(
        yoke::capture::options_for_snapshot(&store, &derived).unwrap(),
        options
    );
    assert_eq!(
        yoke::capture::language_for_snapshot(&store, &derived, "new.custom").unwrap(),
        "rust"
    );
    assert_eq!(
        yoke::capture::capture_for_snapshot(root.path(), &store, &derived, &Deadline::new(5000))
            .unwrap()
            .id,
        origin.id
    );
    let mut tampered = derived.clone();
    tampered.files[0].language = "python".into();
    assert!(yoke::capture::options_for_snapshot(&store, &tampered).is_err());
    assert!(yoke::capture::language_for_snapshot(&store, &derived, "../outside.custom").is_err());
}
#[test]
fn explicit_include_lifts_only_builtin_skip_and_never_widens_only_dirs() {
    let root = tempfile::tempdir().unwrap();
    for path in [
        "src/build/a.rs",
        "build/b.rs",
        "src/a.rs",
        ".yoke/hidden.rs",
    ] {
        let file = root.path().join(path);
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        fs::write(file, "fn sample(){}\n").unwrap();
    }
    let store = Store::open(root.path()).unwrap();
    let options = CaptureOptions {
        only_dirs: vec!["src".into()],
        include_dirs: vec!["build".into()],
        ..Default::default()
    };
    let snapshot =
        capture_with_options(root.path(), &store, &options, &Deadline::new(5000)).unwrap();
    assert_eq!(
        snapshot
            .files
            .iter()
            .map(|f| f.path.as_str())
            .collect::<Vec<_>>(),
        ["src/a.rs", "src/build/a.rs"]
    );
    for dir in [".yoke", ".git", "src/build", "target", "node_modules"] {
        let options = CaptureOptions {
            include_dirs: vec![dir.into()],
            ..Default::default()
        };
        assert!(options.validated().is_err());
    }
}
#[test]
fn known_lsp_only_languages_are_explicit_capture_metadata_without_ast_claims() {
    assert_eq!(yoke::capture::language("a.fs"), "fsharp");
    assert_eq!(yoke::capture::language("a.adb"), "ada");
    assert_eq!(yoke::capture::language("a.hs"), "haskell");
    let options = CaptureOptions {
        language_overrides: BTreeMap::from([("m".into(), "matlab".into())]),
        ..Default::default()
    };
    let options = options.validated().unwrap();
    assert_eq!(options.language_for("a.m"), "matlab");
    assert!(yoke::analysis::grammar("matlab").is_none());
    assert_eq!(yoke::capture::language("a.m"), "objc");
}
#[test]
fn capture_option_proof_obeys_the_shared_store_deadline() {
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("a.rs"), "fn run(){}").unwrap();
    let store = Store::open(root.path()).unwrap();
    let snapshot = capture(root.path(), &store, &Deadline::new(5000)).unwrap();
    assert!(store.set_deadline(&Deadline::new(0)).is_err());
    assert!(yoke::capture::options_for_snapshot(&store, &snapshot).is_err());
}
#[test]
fn exhausted_origin_chain_cannot_publish_an_invalid_capture_proof() {
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("a.custom"), "fn run(){}").unwrap();
    let store = Store::open(root.path()).unwrap();
    let options = CaptureOptions {
        language_overrides: BTreeMap::from([("custom".into(), "rust".into())]),
        ..Default::default()
    };
    let mut origin =
        capture_with_options(root.path(), &store, &options, &Deadline::new(5000)).unwrap();
    store
        .db
        .execute(
            "INSERT INTO snapshots VALUES(?1,?2)",
            rusqlite::params![origin.id, serde_json::to_string(&origin).unwrap()],
        )
        .unwrap();
    for i in 1..=16 {
        let mut derived = origin.clone();
        derived.id = yoke::model::identity(&("chain", i));
        derived.kind = "derived".into();
        store
            .db
            .execute(
                "INSERT INTO snapshots VALUES(?1,?2)",
                rusqlite::params![derived.id, serde_json::to_string(&derived).unwrap()],
            )
            .unwrap();
        let result = yoke::capture::inherit_options(&store, &origin, &derived);
        if i == 16 {
            assert!(result.is_err());
            let exists: bool = store
                .db
                .query_row(
                    "SELECT EXISTS(SELECT 1 FROM records WHERE kind=?1 AND id=?2)",
                    rusqlite::params![yoke::capture::DERIVED_OPTIONS_RECORD_KIND, derived.id],
                    |row| row.get(0),
                )
                .unwrap();
            assert!(
                !exists,
                "Failed inheritance published an unusable immutable proof"
            );
        } else {
            result.unwrap();
            origin = derived;
        }
    }
}
