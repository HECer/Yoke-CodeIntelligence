use serde_json::{Value, json};
use std::fs;
use yoke::{
    Engine,
    agent_repl::invoke_facade,
    capture::{CaptureOptions, Deadline},
    project_session::{HostProfile, HostProject, Session},
};

fn fixture() -> (tempfile::TempDir, Engine) {
    let root = tempfile::tempdir().unwrap();
    fs::create_dir(root.path().join("a_ignored")).unwrap();
    fs::write(
        root.path().join(".gitignore"),
        "a_ignored/hidden.rs\na_ignored/only_hidden/\n",
    )
    .unwrap();
    fs::write(root.path().join("main.rs"), "fn needle_public() {}\n").unwrap();
    fs::write(
        root.path().join("a_ignored/hidden.rs"),
        "fn needle_hidden() {}\n",
    )
    .unwrap();
    fs::write(
        root.path().join("a_ignored/visible.rs"),
        "fn needle_visible() {}\n",
    )
    .unwrap();
    fs::create_dir(root.path().join("a_ignored/only_hidden")).unwrap();
    fs::write(
        root.path().join("a_ignored/only_hidden/nested.rs"),
        "fn needle_nested() {}\n",
    )
    .unwrap();
    let mut engine = Engine::open(root.path()).unwrap();
    engine.capture_options =
        serde_json::from_value::<CaptureOptions>(json!({"includeIgnoredDirs":["a_ignored"]}))
            .unwrap();
    (root, engine)
}
fn paths(value: &Value, field: &str) -> Vec<String> {
    value[field]
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item["path"].as_str().unwrap().to_owned())
        .collect()
}

#[test]
fn native_ignored_filter_is_strict_and_precedes_file_and_symbol_pagination() {
    let (_root, mut engine) = fixture();
    let deadline = Deadline::new(20000);
    let (snapshot, graph, _, _) = engine.index(&deadline).unwrap();
    for kind in ["file", "symbol"] {
        let query = if kind == "file" { "" } else { "needle" };
        let result = engine.invoke("ci_find", json!({"snapshotId":snapshot.id,"kind":kind,"query":query,"skipIgnored":true,"limit":1,"offset":0})).unwrap();
        assert_eq!(result["status"], "ok", "{result}");
        assert!(
            !paths(&result["data"], "items")
                .iter()
                .any(|path| path.ends_with("hidden.rs") || path.ends_with("nested.rs")),
            "{result}"
        );
        let page = yoke::query_features::symbols_find(
            &engine.store,
            &snapshot,
            &graph,
            &json!({"kind":kind,"query":query,"skipIgnored":true,"limit":100}),
            &deadline,
        )
        .unwrap();
        assert_eq!(
            page["total"],
            if kind == "file" { json!(3) } else { json!(2) },
            "{page}"
        );
    }
    let malformed = engine
        .invoke("ci_find", json!({"kind":"file","skipIgnored":"false"}))
        .unwrap();
    assert_eq!(malformed["status"], "error", "{malformed}");
    let unrelated = engine
        .invoke("ci_find", json!({"kind":"document","skipIgnored":false}))
        .unwrap();
    assert_eq!(unrelated["status"], "error", "{unrelated}");
}

#[test]
fn ignored_text_is_filtered_before_blob_reads_and_selected_code_paths() {
    let (_root, mut engine) = fixture();
    let deadline = Deadline::new(20000);
    let (snapshot, graph, _, _) = engine.index(&deadline).unwrap();
    let hidden = snapshot
        .files
        .iter()
        .find(|file| file.path == "a_ignored/hidden.rs")
        .unwrap();
    fs::write(
        engine.store.dir.join("blobs").join(&hidden.hash),
        "corrupted ignored bytes",
    )
    .unwrap();
    let args = json!({"query":"needle","skipIgnored":true,"limit":100});
    let value = yoke::query_features::text_find(&engine.store, &snapshot, &graph, &args, &deadline)
        .unwrap();
    assert_eq!(value["total"], 2, "{value}");
    let selected = snapshot
        .files
        .iter()
        .map(|file| file.path.clone())
        .collect();
    let value = yoke::query_features::text_find_selected(
        &engine.store,
        &snapshot,
        &graph,
        &args,
        &selected,
        &deadline,
    )
    .unwrap();
    assert_eq!(value["total"], 2, "{value}");
    assert!(
        yoke::query_features::text_find(
            &engine.store,
            &snapshot,
            &graph,
            &json!({"query":"needle","skipIgnored":false}),
            &deadline
        )
        .is_err()
    );
}

#[test]
fn list_filters_observed_ignored_files_before_directory_projection() {
    let (_root, engine) = fixture();
    let deadline = Deadline::new(20000);
    let mut session = Session::open(
        engine,
        HostProfile::default(),
        Some("ignored-list"),
        &deadline,
    )
    .unwrap();
    let filtered = session
        .invoke(
            "fs.list",
            &json!({"relativePath":"a_ignored","recursive":true,"skipIgnored":true}),
            &deadline,
        )
        .unwrap();
    assert_eq!(paths(&filtered, "files"), vec!["a_ignored/visible.rs"]);
    assert_eq!(filtered["directories"], json!([]));
    let default = session
        .invoke(
            "fs.list",
            &json!({"relativePath":"a_ignored","recursive":true}),
            &deadline,
        )
        .unwrap();
    assert_eq!(paths(&default, "files").len(), 3);
    assert!(
        session
            .invoke("fs.list", &json!({"skipIgnored":0}), &deadline)
            .is_err()
    );
}

#[test]
fn pinned_fs_facade_defaults_and_explicit_ignored_options_preserve_host_scope() {
    let (root, engine) = fixture();
    let deadline = Deadline::new(20000);
    let mut session = Session::open(
        engine,
        HostProfile::default(),
        Some("ignored-facades"),
        &deadline,
    )
    .unwrap();
    let list = invoke_facade(
        &mut session,
        "fs.list_dir",
        json!({"relative_path":"a_ignored","recursive":true,"skip_ignored_files":true}),
        None,
        &deadline,
    )
    .unwrap();
    assert_eq!(paths(&list, "files"), vec!["a_ignored/visible.rs"]);
    let files = invoke_facade(
        &mut session,
        "fs.find_file",
        json!({"relative_path":"a_ignored","file_mask":"*.rs","skip_ignored_files":true}),
        None,
        &deadline,
    )
    .unwrap();
    assert_eq!(paths(&files, "items"), vec!["a_ignored/visible.rs"]);
    let all = invoke_facade(
        &mut session,
        "fs.find_file",
        json!({"relative_path":"a_ignored","file_mask":"**/*.rs"}),
        None,
        &deadline,
    )
    .unwrap();
    assert_eq!(paths(&all, "items").len(), 3);
    for code_only in [false, true] {
        let filtered = invoke_facade(&mut session,"fs.search_for_pattern",json!({"substring_pattern":"needle","relative_path":"a_ignored","restrict_search_to_code_files":code_only}),None,&deadline).unwrap();
        assert_eq!(filtered["total"], 1, "{filtered}");
        let all = invoke_facade(&mut session,"fs.search_for_pattern",json!({"substring_pattern":"needle","relative_path":"a_ignored","skip_ignored_files":false,"restrict_search_to_code_files":code_only}),None,&deadline).unwrap();
        assert_eq!(all["total"], 3, "{all}");
    }
    // An argument alone cannot add ignored source bytes to a default capture.
    fs::write(
        root.path().join(".gitignore"),
        "a_ignored/hidden.rs\na_ignored/only_hidden/\n",
    )
    .unwrap();
    let plain_root = tempfile::tempdir().unwrap();
    fs::write(plain_root.path().join(".gitignore"), "hidden.rs\n").unwrap();
    fs::write(plain_root.path().join("hidden.rs"), "fn hidden() {}\n").unwrap();
    let mut plain = Session::open(
        Engine::open(plain_root.path()).unwrap(),
        HostProfile::default(),
        Some("no-implicit-grant"),
        &deadline,
    )
    .unwrap();
    let value = invoke_facade(
        &mut plain,
        "fs.find_file",
        json!({"relative_path":"","file_mask":"**/*.rs","skip_ignored_files":false}),
        None,
        &deadline,
    )
    .unwrap();
    assert_eq!(value["items"], json!([]));
}

#[test]
fn querying_a_pinned_snapshot_keeps_original_ignore_outcomes_after_disk_policy_changes() {
    let (root, mut engine) = fixture();
    let deadline = Deadline::new(20000);
    let (original, graph, _, _) = engine.index(&deadline).unwrap();
    fs::write(root.path().join(".gitignore"), "a_ignored/visible.rs\n").unwrap();
    let current = engine.index(&deadline).unwrap().0;
    assert_ne!(original.id, current.id);
    for (snapshot, expected) in [(&original, 2), (&current, 3)] {
        let value = engine.invoke("ci_find", json!({"snapshotId":snapshot.id,"kind":"text","query":"needle","skipIgnored":true,"limit":100})).unwrap();
        assert_eq!(value["status"], "ok", "{value}");
        assert_eq!(value["data"]["total"], expected, "{value}");
    }
    let original_result = yoke::query_features::symbols_find(
        &engine.store,
        &original,
        &graph,
        &json!({"kind":"symbol","query":"needle","skipIgnored":true,"includeBody":true}),
        &deadline,
    )
    .unwrap();
    assert_eq!(original_result["total"], 2, "{original_result}");
    assert!(
        !paths(&original_result, "items")
            .iter()
            .any(|path| path.contains("hidden"))
    );
}

#[test]
fn public_external_directory_queries_share_ignored_scope_without_activation_and_honor_removal() {
    let (_local, engine) = fixture();
    let (external, external_engine) = fixture();
    drop(external_engine);
    let unregistered = tempfile::tempdir().unwrap();
    fs::write(unregistered.path().join("private.rs"), "fn private() {}\n").unwrap();
    let host = HostProfile {
        projects: vec![HostProject {
            name: "reference".into(),
            root: external.path().into(),
            read_only: true,
            query_only: true,
        }],
        ..HostProfile::default()
    };
    let deadline = Deadline::new(20000);
    let mut session = Session::open(engine, host, Some("external-directory"), &deadline).unwrap();
    let query = |session: &mut Session, project: &str, arguments: Value| {
        yoke::compatibility::invoke_serena(
            session,
            "query_project",
            &json!({"project_name":project,"tool_name":"list_dir","tool_params_json":arguments.to_string()}),
            &deadline,
        )
    };
    let filtered = query(
        &mut session,
        "reference",
        json!({"relative_path":"a_ignored","recursive":true,"skip_ignored_files":true}),
    )
    .unwrap();
    assert_eq!(paths(&filtered, "files"), vec!["a_ignored/visible.rs"]);
    assert_eq!(filtered["directories"], json!([]));
    assert!(
        filtered["snapshotId"]
            .as_str()
            .is_some_and(yoke::model::valid_id)
    );
    let all = query(
        &mut session,
        external.path().to_str().unwrap(),
        json!({"relative_path":"a_ignored","recursive":true}),
    )
    .unwrap();
    assert_eq!(paths(&all, "files").len(), 3);
    assert_eq!(
        session
            .invoke("configuration", &json!({}), &deadline)
            .unwrap()["activeProject"],
        "default"
    );
    assert!(
        query(
            &mut session,
            unregistered.path().to_str().unwrap(),
            json!({"relative_path":"","recursive":true})
        )
        .is_err()
    );
    assert!(
        !unregistered.path().join(".yoke").exists(),
        "An unregistered path must not even open project storage"
    );
    assert!(
        query(
            &mut session,
            "reference",
            json!({"relative_path":"../","recursive":true})
        )
        .is_err()
    );
    assert!(
        query(
            &mut session,
            "reference",
            json!({"relative_path":"","recursive":true,"skip_ignored_files":"false"})
        )
        .is_err()
    );
    assert!(
        query(
            &mut session,
            "reference",
            json!({"relative_path":"","recursive":true,"command":"untrusted"})
        )
        .is_err()
    );
    session
        .invoke("removeProject", &json!({"project":"reference"}), &deadline)
        .unwrap();
    assert!(
        query(
            &mut session,
            "reference",
            json!({"relative_path":"","recursive":true})
        )
        .is_err()
    );
    assert!(
        query(
            &mut session,
            external.path().to_str().unwrap(),
            json!({"relative_path":"","recursive":true})
        )
        .is_err()
    );
    assert_eq!(
        fs::read_to_string(external.path().join("a_ignored/hidden.rs")).unwrap(),
        "fn needle_hidden() {}\n"
    );
}

#[test]
fn malformed_native_directory_arguments_fail_before_capture_publication() {
    let (_root, engine) = fixture();
    let deadline = Deadline::new(20000);
    let mut session = Session::open(
        engine,
        HostProfile::default(),
        Some("strict-directory"),
        &deadline,
    )
    .unwrap();
    for arguments in [
        json!({"relativePath":7}),
        json!({"relativePath":null}),
        json!({"relativePath":"../outside"}),
        json!({"recursive":1}),
        json!({"skipIgnored":1}),
        json!({"root":"arbitrary"}),
    ] {
        assert!(
            session.invoke("fs.list", &arguments, &deadline).is_err(),
            "Malformed directory request accepted: {arguments}"
        );
    }
    let count: i64 = session
        .active_engine()
        .unwrap()
        .store
        .db
        .query_row("SELECT COUNT(*) FROM snapshots", [], |row| row.get(0))
        .unwrap();
    assert_eq!(
        count, 0,
        "Invalid arguments must fail before capture publication"
    );
}

#[test]
fn public_directory_recursion_returns_every_captured_level_and_nonrecursive_keeps_first_level() {
    let (root, engine) = fixture();
    fs::create_dir_all(root.path().join("src/a/b")).unwrap();
    fs::write(root.path().join("src/a/b/deep.rs"), "fn deep() {}\n").unwrap();
    let deadline = Deadline::new(20000);
    let mut session = Session::open(
        engine,
        HostProfile::default(),
        Some("recursive-directory"),
        &deadline,
    )
    .unwrap();
    let recursive = yoke::compatibility::invoke_serena(
        &mut session,
        "list_dir",
        &json!({"relative_path":"src","recursive":true}),
        &deadline,
    )
    .unwrap();
    assert_eq!(
        recursive["directories"],
        json!(["src/a", "src/a/b"]),
        "{recursive}"
    );
    assert_eq!(paths(&recursive, "files"), vec!["src/a/b/deep.rs"]);
    let direct = yoke::compatibility::invoke_serena(
        &mut session,
        "list_dir",
        &json!({"relative_path":"src","recursive":false}),
        &deadline,
    )
    .unwrap();
    assert_eq!(direct["directories"], json!(["src/a"]));
    assert_eq!(direct["files"], json!([]));
}

#[test]
fn captured_directory_membership_rejects_files_and_unknown_empty_scopes_without_live_reads() {
    let (root, mut engine) = fixture();
    let deadline = Deadline::new(20000);
    let (snapshot, _, _, _) = engine.index(&deadline).unwrap();
    fs::create_dir(root.path().join("empty")).unwrap();
    fs::create_dir(root.path().join("late")).unwrap();
    fs::write(root.path().join("late/new.rs"), "fn uncaptured() {}\n").unwrap();
    for path in ["main.rs", "not-present", "empty", "late"] {
        assert!(
            yoke::query_features::files_list(
                &engine.store,
                &snapshot,
                &json!({"relativePath":path,"recursive":true}),
                &deadline
            )
            .is_err(),
            "Unproven directory admitted: {path}"
        );
    }
    fs::remove_file(root.path().join("a_ignored/visible.rs")).unwrap();
    let old = yoke::query_features::files_list(
        &engine.store,
        &snapshot,
        &json!({"relativePath":"a_ignored","skipIgnored":true,"recursive":true}),
        &deadline,
    )
    .unwrap();
    assert_eq!(
        paths(&old, "files"),
        vec!["a_ignored/visible.rs"],
        "Directory projection must retain original manifest membership"
    );
    let empty_after_filter = yoke::query_features::files_list(
        &engine.store,
        &snapshot,
        &json!({"relativePath":"a_ignored/only_hidden","skipIgnored":true}),
        &deadline,
    )
    .unwrap();
    assert_eq!(empty_after_filter["files"], json!([]));
    let mut session = Session::open(
        engine,
        HostProfile::default(),
        Some("directory-membership"),
        &deadline,
    )
    .unwrap();
    for path in ["main.rs", "not-present", "empty"] {
        assert!(
            yoke::compatibility::invoke_serena(
                &mut session,
                "list_dir",
                &json!({"relative_path":path,"recursive":true}),
                &deadline
            )
            .is_err(),
            "Public list accepted unproven directory {path}"
        );
    }
}

#[test]
fn deeply_nested_directory_projection_has_an_explicit_budget_and_can_be_refined() {
    use yoke::model::{ANALYZER, Graph, Snapshot, SourceFile, identity};
    let root = tempfile::tempdir().unwrap();
    let mut engine = Engine::open(root.path()).unwrap();
    let empty = engine.store.blob(b"").unwrap();
    let files: Vec<_> = (0..512)
        .map(|index| SourceFile {
            path: format!("branch{index:04}/{}leaf.rs", "segment/".repeat(200)),
            hash: empty.clone(),
            size: 0,
            language: "rust".into(),
            mode: 0o644,
        })
        .collect();
    let mut snapshot = Snapshot {
        id: String::new(),
        workspace_id: engine.store.workspace_id.clone(),
        kind: "disk".into(),
        files,
        omissions: vec![],
        capture_consistency: "stable-double-scan".into(),
    };
    snapshot.id = identity(&(
        &snapshot.workspace_id,
        "disk",
        ANALYZER,
        &snapshot.files,
        &snapshot.omissions,
    ));
    // This bounded native manifest fixture avoids creating a deep host file tree.
    engine
        .store
        .publish(
            &snapshot,
            &Graph {
                snapshot_id: snapshot.id.clone(),
                ..Graph::default()
            },
        )
        .unwrap();
    let deadline = Deadline::new(20000);
    let shallow = yoke::query_features::files_list(
        &engine.store,
        &snapshot,
        &json!({"recursive":false}),
        &deadline,
    )
    .unwrap();
    assert_eq!(shallow["directories"].as_array().unwrap().len(), 512);
    let error = yoke::query_features::files_list(
        &engine.store,
        &snapshot,
        &json!({"recursive":true}),
        &deadline,
    )
    .unwrap_err();
    assert!(
        error.to_string().contains("capacity exceeded"),
        "Expected explicit projection budget: {error}"
    );
    let refined = yoke::query_features::files_list(
        &engine.store,
        &snapshot,
        &json!({"relativePath":"branch0000","recursive":true}),
        &deadline,
    )
    .unwrap();
    assert_eq!(refined["directories"].as_array().unwrap().len(), 200);
    assert_eq!(refined["files"].as_array().unwrap().len(), 1);
}
