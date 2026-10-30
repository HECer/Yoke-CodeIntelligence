use serde_json::{Value, json};
use std::fs;
use yoke::{
    Engine,
    capture::{self, CaptureOptions, Deadline},
    model::{ANALYZER, identity},
};

const KIND: &str = "native_capture_scope_ignored_v1";
fn root() -> tempfile::TempDir {
    let root = tempfile::tempdir().unwrap();
    fs::create_dir(root.path().join("ignored")).unwrap();
    fs::write(root.path().join(".gitignore"), "ignored/hidden.rs\n").unwrap();
    fs::write(root.path().join("main.rs"), "fn public() {}\n").unwrap();
    fs::write(root.path().join("ignored/hidden.rs"), "fn hidden() {}\n").unwrap();
    fs::write(root.path().join("ignored/visible.rs"), "fn visible() {}\n").unwrap();
    root
}
fn options() -> CaptureOptions {
    serde_json::from_value(json!({"includeIgnoredDirs":["ignored"]})).unwrap()
}
#[cfg(windows)]
fn short_directory_name(path: &std::path::Path) -> String {
    use std::os::windows::ffi::{OsStrExt, OsStringExt};
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetShortPathNameW(long: *const u16, short: *mut u16, size: u32) -> u32;
    }
    let wide: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
    let mut short = [0u16; 32768];
    let count = unsafe { GetShortPathNameW(wide.as_ptr(), short.as_mut_ptr(), short.len() as u32) }
        as usize;
    assert!(
        count > 0 && count < short.len(),
        "Windows short-path query failed"
    );
    let short = std::path::PathBuf::from(std::ffi::OsString::from_wide(&short[..count]));
    let alias = short.file_name().unwrap().to_str().unwrap();
    assert_ne!(
        Some(std::ffi::OsStr::new(alias)),
        path.file_name(),
        "This actual Windows gate requires enabled 8.3 directory aliases"
    );
    alias.to_owned()
}
fn proof(engine: &Engine, snapshot_id: &str) -> Value {
    engine.store.get_record(KIND, snapshot_id).unwrap()
}
#[test]
fn empty_ignored_option_preserves_released_serialization_and_default_capture_identity() {
    let root = root();
    let options: CaptureOptions = serde_json::from_value(json!({"includeIgnoredDirs":[]})).unwrap();
    assert_eq!(options, CaptureOptions::default());
    assert_eq!(
        serde_json::to_value(&options).unwrap(),
        json!({"onlyDirs":[],"includeDirs":[],"languageOverrides":{}})
    );
    let mut engine = Engine::open(root.path()).unwrap();
    let (snapshot, _, _, _) = engine.index(&Deadline::new(20000)).unwrap();
    assert!(
        !snapshot
            .files
            .iter()
            .any(|file| file.path == "ignored/hidden.rs")
    );
    assert_eq!(
        snapshot.id,
        identity(&(
            &engine.store.workspace_id,
            "disk",
            ANALYZER,
            &snapshot.files,
            &snapshot.omissions
        ))
    );
    let count: i64 = engine
        .store
        .db
        .query_row(
            "SELECT COUNT(*) FROM records WHERE kind=?1",
            [KIND],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(count, 0);
}

#[test]
fn empty_ignored_option_preserves_released_nondefault_configured_capture_identity() {
    #[derive(serde::Serialize)]
    #[serde(rename_all = "camelCase")]
    struct ReleasedOptions {
        only_dirs: Vec<String>,
        include_dirs: Vec<String>,
        language_overrides: std::collections::BTreeMap<String, String>,
    }
    let root = root();
    let mut engine = Engine::open(root.path()).unwrap();
    engine.capture_options = serde_json::from_value(
        json!({"onlyDirs":["ignored"],"languageOverrides":{"rs":"rust"},"includeIgnoredDirs":[]}),
    )
    .unwrap();
    let released = ReleasedOptions {
        only_dirs: vec!["ignored".into()],
        include_dirs: vec![],
        language_overrides: std::collections::BTreeMap::from([("rs".into(), "rust".into())]),
    };
    assert_eq!(
        serde_json::to_vec(&engine.capture_options).unwrap(),
        serde_json::to_vec(&released).unwrap()
    );
    let snapshot = engine.index(&Deadline::new(20000)).unwrap().0;
    assert_eq!(
        snapshot.id,
        identity(&(
            &engine.store.workspace_id,
            "disk",
            ANALYZER,
            &snapshot.files,
            &snapshot.omissions,
            identity(&released)
        ))
    );
    assert!(
        !snapshot
            .files
            .iter()
            .any(|file| file.path == "ignored/hidden.rs")
    );
}
#[test]
fn host_optin_captures_ignored_sources_read_only_and_binds_complete_observed_outcomes() {
    let root = root();
    let mut engine = Engine::open(root.path()).unwrap();
    engine.capture_options = options();
    let (snapshot, _, _, _) = engine.index(&Deadline::new(20000)).unwrap();
    let scope = capture::source_scope_policy(&engine.store, &snapshot).unwrap();
    assert!(scope.path_policy("ignored/hidden.rs").unwrap().read_only);
    assert!(scope.path_policy("ignored/visible.rs").unwrap().read_only);
    assert!(!scope.path_policy("main.rs").unwrap().read_only);
    assert_eq!(
        scope.read_source("ignored/hidden.rs").unwrap(),
        b"fn hidden() {}\n"
    );
    let proof = proof(&engine, &snapshot.id);
    assert_eq!(proof["policy"], "observed_native_ignore_filter_at_capture");
    let members = proof["members"].as_array().unwrap();
    assert_eq!(members.len(), snapshot.files.len());
    for file in &snapshot.files {
        let member = members
            .iter()
            .find(|member| member["path"] == file.path)
            .unwrap();
        assert_eq!(member["contentHash"], file.hash);
        assert_eq!(member["ignoredAtCapture"], file.path == "ignored/hidden.rs");
    }
    assert_eq!(
        capture::options_for_snapshot(&engine.store, &snapshot).unwrap(),
        options()
    );
}
#[test]
fn observed_ignore_outcomes_are_immutable_when_current_ignore_files_change() {
    let root = root();
    let mut engine = Engine::open(root.path()).unwrap();
    engine.capture_options = options();
    let deadline = Deadline::new(20000);
    let old = engine.index(&deadline).unwrap().0;
    let original = proof(&engine, &old.id);
    fs::write(root.path().join(".gitignore"), "ignored/visible.rs\n").unwrap();
    let new = engine.index(&deadline).unwrap().0;
    assert_ne!(old.id, new.id);
    assert_eq!(proof(&engine, &old.id), original);
    let changed = proof(&engine, &new.id);
    assert_eq!(
        changed["members"]
            .as_array()
            .unwrap()
            .iter()
            .find(|member| member["path"] == "ignored/hidden.rs")
            .unwrap()["ignoredAtCapture"],
        false
    );
    assert_eq!(
        changed["members"]
            .as_array()
            .unwrap()
            .iter()
            .find(|member| member["path"] == "ignored/visible.rs")
            .unwrap()["ignoredAtCapture"],
        true
    );
    assert_eq!(
        capture::source_scope_policy(&engine.store, &old)
            .unwrap()
            .read_source("ignored/hidden.rs")
            .unwrap(),
        b"fn hidden() {}\n"
    );
}
#[test]
fn host_ignored_option_never_relaxes_secret_internal_or_normalized_path_protection() {
    for path in [
        ".git",
        ".yoke",
        ".code-intelligence",
        ".env",
        "../outside",
        "./ignored",
        "ignored/../main.rs",
    ] {
        let parsed = serde_json::from_value::<CaptureOptions>(json!({"includeIgnoredDirs":[path]}));
        assert!(
            parsed
                .and_then(|options| options.validated().map_err(serde::de::Error::custom))
                .is_err(),
            "Unsafe host selector accepted: {path}"
        );
    }
    let root = root();
    fs::write(root.path().join("ignored/.env"), "needle secret").unwrap();
    fs::write(
        root.path().join("ignored/credential.rs"),
        "-----BEGIN PRIVATE KEY-----\nsynthetic test marker\n",
    )
    .unwrap();
    let mut engine = Engine::open(root.path()).unwrap();
    engine.capture_options = options();
    let snapshot = engine.index(&Deadline::new(20000)).unwrap().0;
    assert!(
        !snapshot
            .files
            .iter()
            .any(|file| file.path.contains(".env") || file.path.contains("credential"))
    );
}

#[test]
fn readonly_ignored_edits_and_new_files_fail_before_any_durable_plan_or_snapshot() {
    for (path, create) in [
        ("ignored/hidden.rs", false),
        ("ignored/visible.rs", false),
        ("ignored/new.rs", true),
    ] {
        let root = root();
        let mut engine = Engine::open(root.path()).unwrap();
        engine.capture_options = options();
        let deadline = Deadline::new(20000);
        let (snapshot, graph, _, _) = engine.index(&deadline).unwrap();
        let count: i64 = engine
            .store
            .db
            .query_row("SELECT COUNT(*) FROM snapshots", [], |row| row.get(0))
            .unwrap();
        let edit = if create {
            json!({"kind":"create","path":path,"text":"fn changed() {}\n"})
        } else {
            let source = snapshot
                .files
                .iter()
                .find(|file| file.path == path)
                .unwrap();
            json!({"kind":"replace","path":path,"expectedHash":source.hash,"startByte":0,"endByte":source.size,"text":"fn changed() {}\n"})
        };
        assert!(
            yoke::changes::prepare(
                &mut engine.store,
                &snapshot,
                &graph,
                &json!({"edits":[edit]}),
                &deadline
            )
            .is_err(),
            "Read-only plan accepted: {path}"
        );
        let after: i64 = engine
            .store
            .db
            .query_row("SELECT COUNT(*) FROM snapshots", [], |row| row.get(0))
            .unwrap();
        assert_eq!(
            after, count,
            "Denied edit published a hidden derived snapshot: {path}"
        );
        let plans: i64 = engine
            .store
            .db
            .query_row(
                "SELECT COUNT(*) FROM records WHERE kind='plan'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(plans, 0);
    }
}

#[test]
fn editor_ignored_prefix_updates_are_denied_before_buffer_revisions_change() {
    let root = root();
    let mut engine = Engine::open(root.path()).unwrap();
    engine.capture_options = options();
    let deadline = Deadline::new(20000);
    let editor = yoke::editor::open(&mut engine, &deadline).unwrap();
    for path in ["ignored/hidden.rs", "ignored/visible.rs", "ignored/new.rs"] {
        assert!(
            yoke::editor::update(
                &engine,
                &editor.id,
                &json!({"path":path,"version":1,"text":"fn changed() {}\n"}),
                &deadline
            )
            .is_err(),
            "Readonly buffer update accepted: {path}"
        );
        assert_eq!(
            yoke::editor::session(&engine.store, &editor.id)
                .unwrap()
                .revision,
            editor.revision
        );
    }
}

#[test]
fn derived_edits_outside_ignored_scope_keep_exact_old_classification_and_readonly_sources() {
    let root = root();
    let mut engine = Engine::open(root.path()).unwrap();
    engine.capture_options = options();
    let deadline = Deadline::new(20000);
    let (snapshot, graph, _, _) = engine.index(&deadline).unwrap();
    let source = snapshot
        .files
        .iter()
        .find(|file| file.path == "main.rs")
        .unwrap();
    let plan=yoke::changes::prepare(&mut engine.store,&snapshot,&graph,&json!({"edits":[{"kind":"replace","path":"main.rs","expectedHash":source.hash,"startByte":0,"endByte":source.size,"text":"fn changed() {}\n"}]}),&deadline).unwrap();
    let derived = engine.store.snapshot(&plan.result_snapshot_id).unwrap();
    let scope = capture::source_scope_policy(&engine.store, &derived).unwrap();
    assert!(scope.ignored_at_capture("ignored/hidden.rs").unwrap());
    assert!(!scope.ignored_at_capture("ignored/visible.rs").unwrap());
    assert_eq!(
        scope.read_source("ignored/hidden.rs").unwrap(),
        b"fn hidden() {}\n"
    );
    scope.check_host_write_scope("main.rs").unwrap();
    assert!(scope.check_host_write_scope("ignored/future.rs").is_err());
}

#[test]
fn ignored_source_archives_retain_exact_capture_proof_after_original_store_is_gone() {
    let root = root();
    let central = tempfile::tempdir().unwrap();
    let mut engine = Engine::open(root.path()).unwrap();
    engine.capture_options = options();
    let deadline = Deadline::new(20000);
    let (snapshot, graph, _, _) = engine.index(&deadline).unwrap();
    let registry = yoke::graph_collections::Registry::open(central.path()).unwrap();
    registry
        .register("ignored", &engine.store, &snapshot, &graph, &deadline)
        .unwrap();
    drop(engine);
    fs::write(root.path().join(".gitignore"), "*\n").unwrap();
    root.close().unwrap();
    let archived = registry
        .query(
            &["ignored".into()],
            &json!({"operation":"find","query":"hidden","limit":20}),
            &deadline,
        )
        .unwrap();
    assert!(
        archived["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["name"] == "hidden"),
        "{archived}"
    );
}

#[test]
fn ordinary_editor_overlays_preserve_ignored_source_classification_without_live_receipt_authority()
{
    let root = root();
    let mut engine = Engine::open(root.path()).unwrap();
    engine.capture_options = options();
    let deadline = Deadline::new(20000);
    let editor = yoke::editor::open(&mut engine, &deadline).unwrap();
    yoke::editor::update(
        &engine,
        &editor.id,
        &json!({"path":"main.rs","version":1,"text":"fn unsaved() {}\n"}),
        &deadline,
    )
    .unwrap();
    let overlay = yoke::editor::capture(&mut engine, &editor.id, &deadline).unwrap();
    let scope = capture::source_scope_policy(&engine.store, &overlay).unwrap();
    assert!(scope.ignored_at_capture("ignored/hidden.rs").unwrap());
    assert!(!scope.ignored_at_capture("ignored/visible.rs").unwrap());
    assert_eq!(scope.read_source("main.rs").unwrap(), b"fn unsaved() {}\n");
    assert!(scope.check_host_write_scope("ignored/new.rs").is_err());
    let graph = engine.store.graph(&overlay.id).unwrap();
    let registry_root = tempfile::tempdir().unwrap();
    let registry = yoke::graph_collections::Registry::open(registry_root.path()).unwrap();
    registry
        .register("overlay", &engine.store, &overlay, &graph, &deadline)
        .unwrap();
    drop(scope);
    drop(engine);
    root.close().unwrap();
    let found = registry
        .query(
            &["overlay".into()],
            &json!({"operation":"find","query":"hidden","limit":20}),
            &deadline,
        )
        .unwrap();
    assert!(
        found["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["name"] == "hidden")
    );
}

#[test]
fn retention_recognizes_only_the_exact_ignored_proof_kind_and_rejects_mutated_rows() {
    let root = root();
    let mut engine = Engine::open(root.path()).unwrap();
    engine.capture_options = options();
    let deadline = Deadline::new(20000);
    let snapshot = engine.index(&deadline).unwrap().0;
    let result = yoke::retention::plan(
        &engine.store,
        &yoke::retention::Policy::default(),
        &deadline,
    )
    .unwrap();
    assert_eq!(result["unknownDependencyKinds"], json!([]), "{result}");
    let mut altered = proof(&engine, &snapshot.id);
    altered["members"].as_array_mut().unwrap().pop();
    engine.store.record(KIND, &snapshot.id, &altered).unwrap();
    assert!(capture::source_scope_policy(&engine.store, &snapshot).is_err());
    assert!(
        yoke::retention::plan(
            &engine.store,
            &yoke::retention::Policy::default(),
            &deadline
        )
        .is_err()
    );
}

#[test]
#[cfg(windows)]
fn windows_case_aliases_cannot_create_files_inside_host_readonly_ignored_prefixes() {
    let root = root();
    let mut engine = Engine::open(root.path()).unwrap();
    engine.capture_options = options();
    let deadline = Deadline::new(20000);
    let (snapshot, graph, _, _) = engine.index(&deadline).unwrap();
    let result = yoke::changes::prepare(
        &mut engine.store,
        &snapshot,
        &graph,
        &json!({"edits":[{"kind":"create","path":"IGNORED/new.rs","text":"fn alias() {}\n"}]}),
        &deadline,
    );
    assert!(
        result.is_err(),
        "Case alias granted a readonly-prefix write plan"
    );
    let count: i64 = engine
        .store
        .db
        .query_row("SELECT COUNT(*) FROM snapshots", [], |row| row.get(0))
        .unwrap();
    assert_eq!(count, 1);
}

#[test]
#[cfg(windows)]
fn windows_short_name_aliases_cannot_create_files_inside_host_readonly_prefixes() {
    let root = root();
    let directory = "LongIgnoredSourceDirectory";
    fs::create_dir(root.path().join(directory)).unwrap();
    fs::write(
        root.path().join(directory).join("existing.rs"),
        "fn existing() {}\n",
    )
    .unwrap();
    fs::write(root.path().join(".gitignore"), format!("{directory}/\n")).unwrap();
    let alias = short_directory_name(&root.path().join(directory));
    let mut engine = Engine::open(root.path()).unwrap();
    engine.capture_options =
        serde_json::from_value(json!({"includeIgnoredDirs":[directory]})).unwrap();
    let deadline = Deadline::new(20000);
    let (snapshot, graph, _, _) = engine.index(&deadline).unwrap();
    let result = yoke::changes::prepare(
        &mut engine.store,
        &snapshot,
        &graph,
        &json!({"edits":[{"kind":"create","path":format!("{alias}/future.rs"),"text":"fn alias() {}\n"}]}),
        &deadline,
    );
    assert!(
        result.is_err(),
        "Real DOS short-name alias granted a readonly-prefix write plan"
    );
    let count: i64 = engine
        .store
        .db
        .query_row("SELECT COUNT(*) FROM snapshots", [], |row| row.get(0))
        .unwrap();
    assert_eq!(
        count, 1,
        "Readonly alias denial must precede derived publication"
    );
}

#[test]
#[cfg(windows)]
fn windows_short_name_aliases_do_not_grant_protected_writes_without_ignored_optin() {
    for dependency_optin in [false, true] {
        let root = root();
        fs::create_dir(root.path().join("node_modules")).unwrap();
        fs::write(
            root.path().join("node_modules/existing.rs"),
            "fn existing() {}\n",
        )
        .unwrap();
        let alias = short_directory_name(&root.path().join("node_modules"));
        let mut engine = Engine::open(root.path()).unwrap();
        if dependency_optin {
            engine.capture_options =
                serde_json::from_value(json!({"readOnlyDependencyDirs":["node_modules"]})).unwrap();
        }
        let deadline = Deadline::new(20000);
        let (snapshot, graph, _, _) = engine.index(&deadline).unwrap();
        assert!(yoke::changes::prepare(&mut engine.store,&snapshot,&graph,&json!({"edits":[{"kind":"create","path":format!("{alias}/future.rs"),"text":"fn alias() {}\n"}]}),&deadline).is_err(), "Short-name protected write granted with dependencyOptin={dependency_optin}");
        let count: i64 = engine
            .store
            .db
            .query_row("SELECT COUNT(*) FROM snapshots", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 1, "Protected alias denial must precede publication");
    }
}

#[test]
fn missing_future_reordered_and_incomplete_observed_proofs_never_fall_back_to_default_capture() {
    let root = root();
    let mut engine = Engine::open(root.path()).unwrap();
    engine.capture_options = options();
    let deadline = Deadline::new(20000);
    let snapshot = engine.index(&deadline).unwrap().0;
    let original = proof(&engine, &snapshot.id);
    for variant in 0..4 {
        let mut altered = original.clone();
        match variant {
            0 => altered["futureIgnoredAuthority"] = json!(true),
            1 => {
                altered["members"].as_array_mut().unwrap().pop();
            }
            2 => altered["members"].as_array_mut().unwrap().swap(0, 1),
            _ => altered["schemaVersion"] = json!(2),
        }
        engine.store.record(KIND, &snapshot.id, &altered).unwrap();
        assert!(
            capture::source_scope_policy(&engine.store, &snapshot).is_err(),
            "Malformed proof accepted {variant}"
        );
    }
    engine
        .store
        .db
        .execute(
            "DELETE FROM records WHERE kind=?1 AND id=?2",
            rusqlite::params![KIND, snapshot.id],
        )
        .unwrap();
    assert!(capture::source_scope_policy(&engine.store, &snapshot).is_err());
    assert!(capture::options_for_snapshot(&engine.store, &snapshot).is_err());
    engine.store.record(KIND, &snapshot.id, &original).unwrap();
    assert!(
        capture::source_scope_policy(&engine.store, &snapshot)
            .unwrap()
            .ignored_at_capture("ignored/hidden.rs")
            .unwrap()
    );
}
