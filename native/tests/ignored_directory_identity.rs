#![cfg(windows)]
use serde_json::json;
use std::fs;
use yoke::{
    Engine,
    capture::{self, CaptureOptions, Deadline},
};

fn deadline() -> Deadline {
    Deadline::new(30000)
}
fn fixture() -> tempfile::TempDir {
    let root = tempfile::tempdir().unwrap();
    for name in ["ß", "SS"] {
        fs::create_dir(root.path().join(name)).unwrap();
    }
    fs::write(root.path().join(".gitignore"), "ß/\n").unwrap();
    fs::write(root.path().join("ß/hidden.rs"), "fn hidden(){}\r\n").unwrap();
    fs::write(
        root.path().join("SS/plain.rs"),
        "fn ordinary(){ let π = 1; }\r\n",
    )
    .unwrap();
    root
}
fn options(dirs: &[&str], physical: bool) -> CaptureOptions {
    let mut value = json!({"includeIgnoredDirs":dirs});
    if physical {
        value["ignoredDirectoryIdentity"] = json!("windows-physical-v1");
    }
    serde_json::from_value::<CaptureOptions>(value)
        .unwrap()
        .validated()
        .unwrap()
}
fn replacement(snapshot: &yoke::model::Snapshot, path: &str) -> serde_json::Value {
    let file = snapshot
        .files
        .iter()
        .find(|file| file.path == path)
        .unwrap();
    json!({"edits":[{"kind":"replace","path":path,"expectedHash":file.hash,"startByte":0,"endByte":file.size,"text":"fn reviewed_change(){}\r\n"}]})
}

#[test]
fn selected_physical_identity_keeps_sharp_s_and_ascii_sibling_read_scopes_distinct() {
    let root = tempfile::tempdir().unwrap();
    for name in ["ß", "SS"] {
        fs::create_dir(root.path().join(name)).unwrap();
    }
    let original = "pub fn public(){ let π = 1; }\r\n";
    fs::write(root.path().join(".gitignore"), "ß/\n").unwrap();
    fs::write(root.path().join("ß/hidden.rs"), "fn hidden(){}\r\n").unwrap();
    fs::write(root.path().join("SS/plain.rs"), original).unwrap();
    let options: CaptureOptions = serde_json::from_value(json!({
        "includeIgnoredDirs":["ß"],
        "ignoredDirectoryIdentity":"windows-physical-v1"
    }))
    .unwrap();
    let mut engine = Engine::open(root.path()).unwrap();
    engine.capture_options = options.validated().unwrap();
    let snapshot = engine.index(&Deadline::new(30000)).unwrap().0;
    let scope = capture::source_scope_policy(&engine.store, &snapshot).unwrap();
    assert!(scope.path_policy("ß/hidden.rs").unwrap().read_only);
    assert!(!scope.path_policy("SS/plain.rs").unwrap().read_only);
    assert!(scope.check_host_write_scope("SS/plain.rs").is_ok());
    assert!(scope.check_host_write_scope("ß/hidden.rs").is_err());
    assert_eq!(
        scope.read_source("SS/plain.rs").unwrap(),
        original.as_bytes()
    );
    assert_eq!(
        fs::read(root.path().join("SS/plain.rs")).unwrap(),
        original.as_bytes()
    );
}

#[test]
fn absent_and_null_identity_preserve_original_option_bytes_and_invalid_selection_refuses() {
    let original = json!({"onlyDirs":[],"includeDirs":[],"languageOverrides":{}});
    for value in [json!({}), json!({"ignoredDirectoryIdentity":null})] {
        let options: CaptureOptions = serde_json::from_value(value).unwrap();
        assert_eq!(options.validated().unwrap(), CaptureOptions::default());
        assert_eq!(serde_json::to_value(options).unwrap(), original);
    }
    for value in [
        json!({"includeIgnoredDirs":["ß"],"ignoredDirectoryIdentity":true}),
        json!({"includeIgnoredDirs":["ß"],"ignoredDirectoryIdentity":"future-v9"}),
        json!({"includeIgnoredDirs":["ß"],"ignoredDirectoryIdentity":"literal-ascii-v1"}),
    ] {
        assert!(serde_json::from_value::<CaptureOptions>(value).is_err());
    }
    for value in [
        json!({"ignoredDirectoryIdentity":"windows-physical-v1"}),
        json!({"includeGitIgnored":true,"includeIgnoredDirs":["ß"],"ignoredDirectoryIdentity":"windows-physical-v1"}),
        json!({"includeIgnoredDirs":vec!["ß";257],"ignoredDirectoryIdentity":"windows-physical-v1"}),
    ] {
        let options: CaptureOptions = serde_json::from_value(value).unwrap();
        assert!(options.validated().is_err());
    }
    for path in ["..", "/outside", "ß/../SS", ".git", "ß/.git", "ß/.env"] {
        let options: CaptureOptions = serde_json::from_value(json!({
            "includeIgnoredDirs":[path],"ignoredDirectoryIdentity":"windows-physical-v1"
        }))
        .unwrap();
        assert!(options.validated().is_err(), "{path}");
    }
}

#[test]
fn new_identity_changes_only_new_capture_scope_and_preserves_legacy_replay() {
    let root = fixture();
    let mut engine = Engine::open(root.path()).unwrap();
    engine.capture_options = options(&["ß"], false);
    let old = engine.index(&deadline()).unwrap().0;
    let original_proof: serde_json::Value = engine
        .store
        .get_record(capture::IGNORED_RECORD_KIND, &old.id)
        .unwrap();
    let original_options = serde_json::to_vec(&engine.capture_options).unwrap();
    assert!(
        capture::source_scope_policy(&engine.store, &old)
            .unwrap()
            .path_policy("SS/plain.rs")
            .unwrap()
            .read_only
    );
    engine.capture_options = options(&["ß"], true);
    let selected = engine.index(&deadline()).unwrap().0;
    assert_ne!(selected.id, old.id);
    assert_eq!(selected.files, old.files);
    assert_eq!(selected.omissions, old.omissions);
    assert!(
        !capture::source_scope_policy(&engine.store, &selected)
            .unwrap()
            .path_policy("SS/plain.rs")
            .unwrap()
            .read_only
    );
    assert!(
        capture::source_scope_policy(&engine.store, &old)
            .unwrap()
            .path_policy("SS/plain.rs")
            .unwrap()
            .read_only
    );
    let unchanged: serde_json::Value = engine
        .store
        .get_record(capture::IGNORED_RECORD_KIND, &old.id)
        .unwrap();
    assert_eq!(unchanged, original_proof);
    assert_eq!(
        serde_json::to_vec(&capture::options_for_snapshot(&engine.store, &old).unwrap()).unwrap(),
        original_options
    );
    let proof: serde_json::Value = engine
        .store
        .get_record(capture::IGNORED_RECORD_KIND, &selected.id)
        .unwrap();
    assert_eq!(
        proof["options"]["ignoredDirectoryIdentity"],
        "windows-physical-v1"
    );
    assert_ne!(proof["optionsHash"], original_proof["optionsHash"]);
}

#[test]
fn selected_source_remains_immutable_while_unselected_sibling_can_be_planned() {
    let root = fixture();
    let originals =
        ["SS/plain.rs", "ß/hidden.rs"].map(|p| (p, fs::read(root.path().join(p)).unwrap()));
    let mut engine = Engine::open(root.path()).unwrap();
    engine.allow_writes = true;
    engine.capture_options = options(&["ß"], true);
    let (snapshot, graph, _, _) = engine.index(&deadline()).unwrap();
    for edits in [
        replacement(&snapshot, "ß/hidden.rs"),
        json!({"edits":[{"kind":"create","path":"ß/new.rs","text":"fn denied(){}\n"}]}),
    ] {
        let before = engine.store.db.total_changes();
        assert!(
            yoke::changes::prepare(&mut engine.store, &snapshot, &graph, &edits, &deadline())
                .is_err()
        );
        assert_eq!(engine.store.db.total_changes(), before);
    }
    let plan = yoke::changes::prepare(
        &mut engine.store,
        &snapshot,
        &graph,
        &replacement(&snapshot, "SS/plain.rs"),
        &deadline(),
    )
    .unwrap();
    assert_eq!(plan.files.len(), 1);
    assert_eq!(plan.files[0].path, "SS/plain.rs");
    yoke::changes::load_checked_plan(&engine.store, &plan.id, &deadline()).unwrap();
    for (path, bytes) in originals {
        assert_eq!(fs::read(root.path().join(path)).unwrap(), bytes);
    }
    assert!(!root.path().join("ß/new.rs").exists());
}

#[test]
fn editor_overlays_preserve_physical_policy_and_refuse_selected_source_without_revision_changes() {
    let root = fixture();
    let original = fs::read(root.path().join("SS/plain.rs")).unwrap();
    let mut engine = Engine::open(root.path()).unwrap();
    engine.capture_options = options(&["ß"], true);
    let editor = yoke::editor::open(&mut engine, &deadline()).unwrap();
    let before =
        serde_json::to_vec(&yoke::editor::session(&engine.store, &editor.id).unwrap()).unwrap();
    assert!(
        yoke::editor::update(
            &engine,
            &editor.id,
            &json!({"path":"ß/hidden.rs","version":1,"text":"fn denied(){}\n"}),
            &deadline()
        )
        .is_err()
    );
    assert_eq!(
        serde_json::to_vec(&yoke::editor::session(&engine.store, &editor.id).unwrap()).unwrap(),
        before
    );
    yoke::editor::update(
        &engine,
        &editor.id,
        &json!({"path":"SS/plain.rs","version":1,"text":"fn buffer_only(){ let π = 2; }\r\n"}),
        &deadline(),
    )
    .unwrap();
    let overlay = yoke::editor::capture(&mut engine, &editor.id, &deadline()).unwrap();
    let scope = capture::source_scope_policy(&engine.store, &overlay).unwrap();
    assert!(scope.path_policy("ß/hidden.rs").unwrap().read_only);
    // Every virtual overlay retains its independent read-only boundary.
    assert!(scope.path_policy("SS/plain.rs").unwrap().read_only);
    assert_eq!(
        capture::options_for_snapshot(&engine.store, &overlay).unwrap(),
        options(&["ß"], true)
    );
    assert_eq!(
        scope.read_source("SS/plain.rs").unwrap(),
        "fn buffer_only(){ let π = 2; }\r\n".as_bytes()
    );
    assert_eq!(fs::read(root.path().join("SS/plain.rs")).unwrap(), original);
}

#[test]
fn current_physical_selection_blocks_saved_workspace_plan_without_rewriting_history() {
    let root = fixture();
    let original = fs::read(root.path().join("SS/plain.rs")).unwrap();
    let mut engine = Engine::open(root.path()).unwrap();
    engine.allow_writes = true;
    let (snapshot, graph, _, _) = engine.index(&deadline()).unwrap();
    let plan = yoke::changes::prepare(
        &mut engine.store,
        &snapshot,
        &graph,
        &replacement(&snapshot, "SS/plain.rs"),
        &deadline(),
    )
    .unwrap();
    let historical = serde_json::to_vec(&plan).unwrap();
    yoke::changes::approve(&engine.store, &plan.id, false).unwrap();
    engine.capture_options = options(&["SS"], true);
    engine.index(&deadline()).unwrap();
    assert!(
        !capture::source_scope_policy(&engine.store, &snapshot)
            .unwrap()
            .path_policy("SS/plain.rs")
            .unwrap()
            .read_only
    );
    let before_prepare = engine.store.db.total_changes();
    let error = yoke::changes::prepare(
        &mut engine.store,
        &snapshot,
        &graph,
        &replacement(&snapshot, "SS/plain.rs"),
        &deadline(),
    )
    .err()
    .unwrap();
    assert!(
        error
            .to_string()
            .contains("ignored_directory_is_read_only_in_current_host_selection"),
        "{error}"
    );
    assert_eq!(engine.store.db.total_changes(), before_prepare);
    let before = engine.store.db.total_changes();
    let error = yoke::changes::apply(
        &engine.store,
        root.path(),
        &json!({"planId":plan.id,"idempotencyKey":"current-policy-denied","mode":"workspace"}),
        true,
        &deadline(),
    )
    .err()
    .unwrap();
    assert!(
        error
            .to_string()
            .contains("ignored_directory_is_read_only_in_current_host_selection"),
        "{error}"
    );
    assert_eq!(engine.store.db.total_changes(), before);
    assert_eq!(
        serde_json::to_vec(
            &yoke::changes::load_checked_plan(&engine.store, &plan.id, &deadline()).unwrap()
        )
        .unwrap(),
        historical
    );
    assert_eq!(fs::read(root.path().join("SS/plain.rs")).unwrap(), original);
    let receipt = yoke::changes::apply(
        &engine.store,
        root.path(),
        &json!({"planId":plan.id,"idempotencyKey":"historical-isolated","mode":"isolated"}),
        false,
        &deadline(),
    )
    .unwrap();
    assert_eq!(
        fs::read(std::path::Path::new(receipt.sandbox_path.as_ref().unwrap()).join("SS/plain.rs"))
            .unwrap(),
        b"fn reviewed_change(){}\r\n"
    );
    assert_eq!(fs::read(root.path().join("SS/plain.rs")).unwrap(), original);
}

#[test]
fn changing_stored_identity_selection_cannot_reinterpret_an_existing_capture() {
    for physical in [false, true] {
        let root = fixture();
        let mut engine = Engine::open(root.path()).unwrap();
        engine.capture_options = options(&["ß"], physical);
        let snapshot = engine.index(&deadline()).unwrap().0;
        let mut proof: serde_json::Value = engine
            .store
            .get_record(capture::IGNORED_RECORD_KIND, &snapshot.id)
            .unwrap();
        if physical {
            proof["options"]
                .as_object_mut()
                .unwrap()
                .remove("ignoredDirectoryIdentity");
        } else {
            proof["options"]["ignoredDirectoryIdentity"] = json!("windows-physical-v1");
        }
        engine
            .store
            .db
            .execute(
                "UPDATE records SET value=?1 WHERE kind=?2 AND id=?3",
                rusqlite::params![proof.to_string(), capture::IGNORED_RECORD_KIND, snapshot.id],
            )
            .unwrap();
        let before = engine.store.db.total_changes();
        assert!(capture::source_scope_policy(&engine.store, &snapshot).is_err());
        assert_eq!(engine.store.db.total_changes(), before);
    }
}

#[test]
fn physical_capture_retention_and_offline_collection_preserve_exact_source_proof() {
    let root = fixture();
    let archive_root = tempfile::tempdir().unwrap();
    let mut engine = Engine::open(root.path()).unwrap();
    engine.capture_options = options(&["ß"], true);
    let (snapshot, graph, _, _) = engine.index(&deadline()).unwrap();
    let original: serde_json::Value = engine
        .store
        .get_record(capture::IGNORED_RECORD_KIND, &snapshot.id)
        .unwrap();
    let retirement = yoke::retention::plan(
        &engine.store,
        &yoke::retention::Policy::default(),
        &deadline(),
    )
    .unwrap();
    assert_eq!(retirement["unknownDependencyKinds"], json!([]));
    let preserved: serde_json::Value = engine
        .store
        .get_record(capture::IGNORED_RECORD_KIND, &snapshot.id)
        .unwrap();
    assert_eq!(preserved, original);
    let registry = yoke::graph_collections::Registry::open(archive_root.path()).unwrap();
    registry
        .register("physical", &engine.store, &snapshot, &graph, &deadline())
        .unwrap();
    drop(engine);
    root.close().unwrap();
    for name in ["ordinary", "hidden"] {
        let result = registry
            .query(
                &["physical".into()],
                &json!({"operation":"find","query":name,"limit":20}),
                &deadline(),
            )
            .unwrap();
        assert!(
            result["items"]
                .as_array()
                .unwrap()
                .iter()
                .any(|item| item["name"] == name),
            "{result}"
        );
    }
}

#[test]
fn physical_unicode_case_alias_selects_the_same_captured_readonly_directory() {
    let root = fixture();
    fs::create_dir(root.path().join("é")).unwrap();
    fs::write(root.path().join("é/plain.rs"), "fn same_physical(){}\r\n").unwrap();
    assert_eq!(
        root.path().join("é").canonicalize().unwrap(),
        root.path().join("É").canonicalize().unwrap()
    );
    let mut engine = Engine::open(root.path()).unwrap();
    engine.capture_options = options(&["é"], true);
    let exact = engine.index(&deadline()).unwrap().0;
    engine.capture_options = options(&["É"], true);
    let aliased = engine.index(&deadline()).unwrap().0;
    assert_eq!(
        exact.id, aliased.id,
        "One physical selection must have one captured identity"
    );
    assert!(
        capture::source_scope_policy(&engine.store, &aliased)
            .unwrap()
            .path_policy("é/plain.rs")
            .unwrap()
            .read_only
    );
    assert_eq!(
        capture::options_for_snapshot(&engine.store, &aliased)
            .unwrap()
            .include_ignored_dirs,
        vec!["é"]
    );
}

#[test]
fn current_physical_unicode_alias_restricts_a_previously_writable_snapshot_before_publication() {
    let root = fixture();
    fs::create_dir(root.path().join("é")).unwrap();
    fs::write(root.path().join("é/plain.rs"), "fn same_physical(){}\r\n").unwrap();
    let mut engine = Engine::open(root.path()).unwrap();
    engine.allow_writes = true;
    let snapshot = engine.index(&deadline()).unwrap().0;
    let mut input = replacement(&snapshot, "é/plain.rs");
    input["snapshotId"] = json!(snapshot.id);
    engine.capture_options = options(&["É"], true);
    let before = engine.store.db.total_changes();
    let result = engine.invoke("ci_change_prepare", input).unwrap();
    assert_eq!(result["status"], "error", "{result}");
    assert!(
        result
            .to_string()
            .contains("ignored_directory_is_read_only_in_current_host_selection")
    );
    assert_eq!(engine.store.db.total_changes(), before);
    assert_eq!(
        fs::read(root.path().join("é/plain.rs")).unwrap(),
        b"fn same_physical(){}\r\n"
    );
}

fn short_directory(path: &std::path::Path) -> String {
    use std::os::windows::ffi::OsStrExt;
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetShortPathNameW(long: *const u16, short: *mut u16, size: u32) -> u32;
    }
    let wide = path
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect::<Vec<_>>();
    let mut short = [0u16; 32768];
    let count = unsafe { GetShortPathNameW(wide.as_ptr(), short.as_mut_ptr(), short.len() as u32) }
        as usize;
    assert!(
        count > 0 && count < short.len(),
        "Owned Windows short-path prerequisite unavailable"
    );
    let path = std::path::PathBuf::from(String::from_utf16(&short[..count]).unwrap());
    path.file_name().unwrap().to_str().unwrap().to_owned()
}

#[test]
fn real_dos_aliases_select_one_capture_and_cannot_cross_current_readonly_scope() {
    let root = fixture();
    let directory = "LongPhysicalIgnoredDirectoryß";
    fs::create_dir(root.path().join(directory)).unwrap();
    fs::write(
        root.path().join(directory).join("existing.rs"),
        "fn existing(){}\r\n",
    )
    .unwrap();
    let alias = short_directory(&root.path().join(directory));
    assert_ne!(alias, directory);
    let mut engine = Engine::open(root.path()).unwrap();
    engine.allow_writes = true;
    let default = engine.index(&deadline()).unwrap().0;
    engine.capture_options = options(&[directory], true);
    let exact = engine.index(&deadline()).unwrap().0;
    engine.capture_options = options(&[&alias], true);
    let aliased = engine.index(&deadline()).unwrap().0;
    assert_eq!(exact.id, aliased.id);
    assert_eq!(
        capture::options_for_snapshot(&engine.store, &aliased)
            .unwrap()
            .include_ignored_dirs,
        vec![directory]
    );
    let before = engine.store.db.total_changes();
    let result = engine.invoke("ci_change_prepare", json!({"snapshotId":default.id,"edits":[{"kind":"create","path":format!("{alias}/future.rs"),"text":"fn denied(){}\n"}]})).unwrap();
    assert_eq!(result["status"], "error", "{result}");
    assert!(
        result
            .to_string()
            .contains("ignored_directory_is_read_only_in_current_host_selection")
    );
    assert_eq!(engine.store.db.total_changes(), before);
    assert!(!root.path().join(directory).join("future.rs").exists());
}

#[test]
fn physical_aliases_of_hard_exclusions_never_publish_a_capture() {
    let root = fixture();
    fs::create_dir(root.path().join("node_modules")).unwrap();
    fs::write(
        root.path().join("node_modules/private.rs"),
        "fn excluded(){}\n",
    )
    .unwrap();
    let alias = short_directory(&root.path().join("node_modules"));
    assert_ne!(alias, "node_modules");
    let mut engine = Engine::open(root.path()).unwrap();
    engine.capture_options = options(&[&alias], true);
    let before = engine.store.db.total_changes();
    assert!(engine.index(&deadline()).is_err());
    assert_eq!(engine.store.db.total_changes(), before);
}

#[test]
fn physical_capture_checks_binding_and_uses_the_new_request_deadline() {
    let root = fixture();
    let foreign = fixture();
    let mut engine = Engine::open(root.path()).unwrap();
    let before = engine.store.db.total_changes();
    assert!(
        capture::capture_with_options(
            foreign.path(),
            &engine.store,
            &options(&["ß"], true),
            &deadline()
        )
        .is_err()
    );
    assert_eq!(engine.store.db.total_changes(), before);
    engine.store.set_deadline(&Deadline::new(50)).unwrap();
    std::thread::sleep(std::time::Duration::from_millis(60));
    engine.capture_options = options(&["ß"], true);
    engine.index(&deadline()).unwrap();
}

#[test]
fn captured_readonly_scope_survives_a_physical_unicode_case_rename_without_current_policy() {
    let root = fixture();
    fs::create_dir(root.path().join("é")).unwrap();
    fs::write(root.path().join("é/plain.rs"), "fn captured(){}\r\n").unwrap();
    let mut engine = Engine::open(root.path()).unwrap();
    engine.allow_writes = true;
    engine.capture_options = options(&["é"], true);
    let snapshot = engine.index(&deadline()).unwrap().0;
    let canonical_root = root.path().canonicalize().unwrap();
    for path in ["é", "case-stage", "É"] {
        assert_eq!(
            root.path()
                .join(path)
                .parent()
                .unwrap()
                .canonicalize()
                .unwrap(),
            canonical_root
        );
    }
    fs::rename(root.path().join("é"), root.path().join("case-stage")).unwrap();
    fs::rename(root.path().join("case-stage"), root.path().join("É")).unwrap();
    assert_eq!(
        root.path()
            .join("é")
            .canonicalize()
            .unwrap()
            .file_name()
            .unwrap(),
        "É"
    );
    engine.capture_options = CaptureOptions::default();
    let before = engine.store.db.total_changes();
    let result = engine.invoke("ci_change_prepare", json!({"snapshotId":snapshot.id,"edits":[{"kind":"create","path":"É/future.rs","text":"fn denied(){}\n"}]})).unwrap();
    assert_eq!(result["status"], "error", "{result}");
    assert!(
        result
            .to_string()
            .contains("captured_physical_ignored_directory_is_read_only")
    );
    assert_eq!(engine.store.db.total_changes(), before);
    assert!(!root.path().join("É/future.rs").exists());
    assert_eq!(
        capture::source_scope_policy(&engine.store, &snapshot)
            .unwrap()
            .read_source("é/plain.rs")
            .unwrap(),
        b"fn captured(){}\r\n"
    );
}

#[test]
fn missing_or_nondirectory_physical_selectors_refuse_before_capture_publication() {
    let root = fixture();
    let mut engine = Engine::open(root.path()).unwrap();
    for directory in ["absent", "SS/plain.rs"] {
        engine.capture_options = options(&[directory], true);
        let before = engine.store.db.total_changes();
        assert!(engine.index(&deadline()).is_err(), "{directory}");
        assert_eq!(engine.store.db.total_changes(), before);
    }
}
