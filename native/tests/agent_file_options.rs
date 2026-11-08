use serde_json::json;
use std::{collections::BTreeSet, fs};
use yoke::{
    Engine, agent_repl,
    capture::Deadline,
    project_session::{HostProfile, Session},
};

#[test]
fn code_only_search_selects_captured_languages_before_matching_and_retains_exact_context() {
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("a-readme.md"), "needle\n".repeat(100)).unwrap();
    fs::write(
        root.path().join("settings.json"),
        "{\"value\":\"needle\"}\n",
    )
    .unwrap();
    fs::write(
        root.path().join("main.rs"),
        "// before\nfn needle() {}\n// after\n",
    )
    .unwrap();
    let deadline = Deadline::new(20000);
    let engine = Engine::open(root.path()).unwrap();
    let mut session = Session::open(
        engine,
        HostProfile::default(),
        Some("code-only-search"),
        &deadline,
    )
    .unwrap();
    let result = agent_repl::invoke_facade(&mut session,"fs.search_for_pattern",json!({"substring_pattern":"needle","restrict_search_to_code_files":true,"context_lines_before":1,"context_lines_after":1}),None,&deadline).unwrap();
    assert_eq!(result["items"].as_array().unwrap().len(), 1, "{result}");
    assert_eq!(result["items"][0]["path"], "main.rs");
    assert_eq!(result["total"], 1);
    assert!(
        result["snapshotId"]
            .as_str()
            .is_some_and(yoke::model::valid_id)
    );
    assert_eq!(
        result["items"][0]["context"],
        "// before\nfn needle() {}\n// after"
    );
    assert_eq!(
        result["items"][0]["contentHash"],
        yoke::model::hash("// before\nfn needle() {}\n// after\n")
    );
    let excluded=agent_repl::invoke_facade(&mut session,"fs.search_for_pattern",json!({"substring_pattern":"needle","restrict_search_to_code_files":true,"paths_exclude_glob":"*.rs"}),None,&deadline).unwrap();
    assert_eq!(excluded["total"], 0);
}

#[test]
fn code_only_classification_preserves_host_language_overrides_without_capturing_new_files() {
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("program.custom"), "fn needle() {}\n").unwrap();
    fs::write(root.path().join(".env"), "needle secret").unwrap();
    let deadline = Deadline::new(20000);
    let mut engine = Engine::open(root.path()).unwrap();
    engine
        .capture_options
        .language_overrides
        .insert("custom".into(), "rust".into());
    let mut session = Session::open(
        engine,
        HostProfile::default(),
        Some("code-only-override"),
        &deadline,
    )
    .unwrap();
    let result = agent_repl::invoke_facade(
        &mut session,
        "fs.search_for_pattern",
        json!({"substring_pattern":"needle","restrict_search_to_code_files":true}),
        None,
        &deadline,
    )
    .unwrap();
    assert_eq!(result["items"].as_array().unwrap().len(), 1);
    assert_eq!(result["items"][0]["path"], "program.custom");
    assert_eq!(result["codeScope"]["captureExpanded"], false);
}

#[test]
fn selected_search_paths_reject_uncaptured_aliases_before_reads_and_do_not_scan_unselected_blobs() {
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("main.rs"), "fn needle() {}\n").unwrap();
    fs::write(root.path().join("readme.md"), "needle\n").unwrap();
    let deadline = Deadline::new(20000);
    let mut engine = Engine::open(root.path()).unwrap();
    let (snapshot, graph, _, _) = engine.index(&deadline).unwrap();
    for path in ["missing.rs", "./main.rs", ".env", "../main.rs"] {
        assert!(
            yoke::query_features::text_find_selected(
                &engine.store,
                &snapshot,
                &graph,
                &json!({"query":"needle"}),
                &BTreeSet::from([path.to_owned()]),
                &deadline
            )
            .is_err(),
            "Uncaptured selection accepted: {path}"
        );
    }
    let readme = snapshot
        .files
        .iter()
        .find(|file| file.path == "readme.md")
        .unwrap();
    let missing = engine.store.dir.join("blobs").join(&readme.hash);
    fs::remove_file(&missing).unwrap();
    let selected = yoke::query_features::text_find_selected(
        &engine.store,
        &snapshot,
        &graph,
        &json!({"query":"needle"}),
        &BTreeSet::from(["main.rs".to_owned()]),
        &deadline,
    )
    .unwrap();
    assert_eq!(selected["total"], 1);
    assert_eq!(selected["items"][0]["path"], "main.rs");
}

#[test]
fn project_symbol_access_filter_describes_native_query_capability_without_probing_providers() {
    let root = tempfile::tempdir().unwrap();
    let registered = tempfile::tempdir().unwrap();
    fs::write(root.path().join("main.rs"), "fn local() {}\n").unwrap();
    fs::write(registered.path().join("main.py"), "def external(): pass\n").unwrap();
    let deadline = Deadline::new(20000);
    let host = HostProfile {
        projects: vec![yoke::project_session::HostProject {
            name: "registered".into(),
            root: registered.path().into(),
            read_only: true,
            query_only: true,
        }],
        ..Default::default()
    };
    let mut session = Session::open(
        Engine::open(root.path()).unwrap(),
        host,
        Some("project-symbol-access"),
        &deadline,
    )
    .unwrap();
    assert!(
        agent_repl::invoke_facade(
            &mut session,
            "ext.list_projects",
            json!({"symbol_access":"yes"}),
            None,
            &deadline
        )
        .is_err()
    );
    for filter in [true, false] {
        let listed = agent_repl::invoke_facade(
            &mut session,
            "ext.list_projects",
            json!({"symbol_access":filter}),
            None,
            &deadline,
        )
        .unwrap();
        assert_eq!(listed["symbolAccessRequested"], filter);
        assert_eq!(listed["items"].as_array().unwrap().len(), 2);
        assert!(
            listed["items"]
                .as_array()
                .unwrap()
                .iter()
                .all(|item| item["nativeSymbolQueries"] == true
                    && item["semanticProviderReadiness"] == "not_probed")
        );
    }
}
