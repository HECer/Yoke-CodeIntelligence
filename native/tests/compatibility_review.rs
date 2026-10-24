use serde_json::json;
use std::fs;
use yoke::{
    Engine,
    capture::Deadline,
    project_session::{HostProfile, Session},
};

#[test]
fn public_reference_kind_filters_cannot_silently_return_unfiltered_results() {
    let root = tempfile::tempdir().unwrap();
    fs::write(
        root.path().join("main.rs"),
        "fn target() {}\nfn caller() { target(); }\n",
    )
    .unwrap();
    let deadline = Deadline::new(20000);
    let mut session = Session::open(
        Engine::open(root.path()).unwrap(),
        HostProfile::default(),
        Some("review"),
        &deadline,
    )
    .unwrap();
    let ordinary = yoke::compatibility::invoke_serena(
        &mut session,
        "find_referencing_symbols",
        &json!({"name_path":"target","relative_path":"main.rs"}),
        &deadline,
    )
    .unwrap();
    assert!(!ordinary.to_string().contains("\"status\":\"error\""));
    let filtered = yoke::compatibility::invoke_serena(
        &mut session,
        "find_referencing_symbols",
        &json!({"name_path":"target","relative_path":"main.rs","include_kinds":[5]}),
        &deadline,
    )
    .unwrap();
    assert!(
        filtered["observations"].as_array().unwrap().is_empty(),
        "Class-only filter returned function callers: {filtered}"
    );
    let functions = yoke::compatibility::invoke_serena(
        &mut session,
        "find_referencing_symbols",
        &json!({"name_path":"target","relative_path":"main.rs","include_kinds":[12]}),
        &deadline,
    )
    .unwrap();
    assert!(
        !functions["observations"].as_array().unwrap().is_empty(),
        "Function callers were dropped: {functions}"
    );
    assert!(
        functions["entities"]
            .as_array()
            .unwrap()
            .iter()
            .any(|entity| entity["name"] == "caller")
    );
}
