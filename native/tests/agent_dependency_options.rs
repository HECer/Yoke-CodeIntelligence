use serde_json::json;
use std::fs;
use yoke::{
    Engine, agent_repl,
    capture::Deadline,
    project_session::{HostProfile, Session},
};
fn invoke(session: &mut Session, args: serde_json::Value) -> anyhow::Result<serde_json::Value> {
    agent_repl::invoke_facade(session, "jb.find_symbol", args, None, &Deadline::new(30000))
}
#[test]
fn dependency_search_is_host_captured_read_only_scope_and_filters_before_the_match_threshold() {
    let root = tempfile::tempdir().unwrap();
    fs::create_dir_all(root.path().join("node_modules/pkg")).unwrap();
    fs::write(root.path().join("main.js"), "export function shared() {}\n").unwrap();
    fs::write(
        root.path().join("node_modules/pkg/index.js"),
        "export function shared() {}\n",
    )
    .unwrap();
    let mut engine = Engine::open(root.path()).unwrap();
    engine.capture_options.read_only_dependency_dirs = vec!["node_modules".into()];
    let deadline = Deadline::new(30000);
    let mut session = Session::open(
        engine,
        HostProfile::default(),
        Some("host-captured-dependency-options"),
        &deadline,
    )
    .unwrap();
    let project = invoke(
        &mut session,
        json!({"name_path_pattern":"shared","search_deps":false,"max_matches":1}),
    )
    .unwrap();
    assert_eq!(project["items"].as_array().unwrap().len(), 1);
    assert_eq!(project["items"][0]["path"], "main.js");
    let default_scope = invoke(
        &mut session,
        json!({"name_path_pattern":"shared","max_matches":1}),
    )
    .unwrap();
    assert_eq!(default_scope["items"].as_array().unwrap().len(), 1);
    let both = invoke(
        &mut session,
        json!({"name_path_pattern":"shared","search_deps":true,"max_matches":2}),
    )
    .unwrap();
    assert_eq!(both["items"].as_array().unwrap().len(), 2);
    assert_eq!(both["dependencySearch"]["capturedReadOnlyPaths"], 1);
    assert_eq!(both["snapshotId"], project["snapshotId"]);
    assert_eq!(
        both["dependencySearch"]["scope"],
        "explicit_host_captured_read_only_sources"
    );
    assert!(agent_repl::invoke_facade(&mut session,"edit.replace_symbol_body",json!({"name_path":"shared","relative_path":"node_modules/pkg/index.js","body":"export function changed() {}"}),None,&deadline).is_err());
    assert!(
        fs::read_to_string(root.path().join("node_modules/pkg/index.js"))
            .unwrap()
            .contains("shared")
    );
}
#[test]
fn requesting_dependencies_never_captures_ungranted_paths_or_protected_files() {
    let root = tempfile::tempdir().unwrap();
    fs::create_dir_all(root.path().join("node_modules/pkg")).unwrap();
    fs::write(root.path().join("main.js"), "export function local() {}\n").unwrap();
    fs::write(
        root.path().join("node_modules/pkg/index.js"),
        "export function uncaptured() {}\n",
    )
    .unwrap();
    fs::write(root.path().join("node_modules/pkg/.env"), "protected-data").unwrap();
    let mut session = Session::open(
        Engine::open(root.path()).unwrap(),
        HostProfile::default(),
        Some("ungranted-dependency-options"),
        &Deadline::new(30000),
    )
    .unwrap();
    let value = invoke(
        &mut session,
        json!({"name_path_pattern":"uncaptured","search_deps":true}),
    )
    .unwrap();
    assert!(value["items"].as_array().unwrap().is_empty());
    assert_eq!(value["dependencySearch"]["capturedReadOnlyPaths"], 0);
    assert_eq!(value["complete"], false);
}
