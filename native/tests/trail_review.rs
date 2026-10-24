use serde_json::json;
use std::fs;
use yoke::{
    Engine,
    capture::Deadline,
    project_session::{HostProfile, Session},
    trail_adapter,
};

#[test]
fn trail_dependency_trace_excludes_source_containment_edges() {
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
    let result = trail_adapter::invoke(
        &mut session,
        "graft_trace_calls",
        &json!({"symbol":"target","depth":"all"}),
        &deadline,
    )
    .unwrap();
    let edges = result["observations"].as_array().unwrap();
    assert!(edges.iter().any(|edge| edge["relation"] == "calls"));
    assert!(
        !edges.iter().any(|edge| edge["relation"] == "contains"),
        "File containment was exposed as dependency wiring"
    );
}

#[test]
fn trail_file_trace_seeds_the_files_symbols_for_incoming_dependency_closure() {
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("lib.rs"), "mod api; mod caller;\n").unwrap();
    fs::write(root.path().join("api.rs"), "pub fn target() {}\n").unwrap();
    fs::write(
        root.path().join("caller.rs"),
        "use crate::api::target; pub fn caller() { target(); }\n",
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
    let result = trail_adapter::invoke(
        &mut session,
        "graft_trace_calls",
        &json!({"symbol":"api.rs","depth":"all"}),
        &deadline,
    )
    .unwrap();
    assert!(
        result["entities"]
            .as_array()
            .unwrap()
            .iter()
            .any(|entity| entity["name"] == "caller"),
        "File-only seed missed a caller of its defined symbol"
    );
}

#[test]
fn trail_incoming_coupling_does_not_count_the_containing_file_as_a_dependency() {
    let root = tempfile::tempdir().unwrap();
    fs::write(
        root.path().join("main.rs"),
        "fn lonely() { /* marker */ }\n",
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
    let result = trail_adapter::invoke(
        &mut session,
        "graft_find_all",
        &json!({"pattern":"marker","fixed":true}),
        &deadline,
    )
    .unwrap();
    assert_eq!(
        result["items"][0]["incomingCoupling"], 0,
        "Parent containment inflated coupling for an isolated function"
    );
}

#[test]
fn trail_qualified_symbol_selection_matches_the_declared_container_suffix() {
    let root = tempfile::tempdir().unwrap();
    fs::write(
        root.path().join("Sample.java"),
        "class Sample { void target() {} void caller() { target(); } }\n",
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
    let result = trail_adapter::invoke(
        &mut session,
        "graft_trace_calls",
        &json!({"symbol":"Sample.target"}),
        &deadline,
    );
    assert!(
        result.is_ok(),
        "Qualified container.method selector did not resolve: {result:?}"
    );
}
