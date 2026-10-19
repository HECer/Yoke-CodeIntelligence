//! Host-selected spellings remain selectors for the same registered root only.
use serde_json::{Value, json};
use std::{fs, path::Path};
use yoke::{
    Engine,
    capture::Deadline,
    project_session::{HostProfile, HostProject, Session},
};

fn host(root: &Path) -> HostProfile {
    HostProfile {
        projects: vec![HostProject {
            name: "reference".into(),
            root: root.into(),
            read_only: true,
            query_only: true,
        }],
        ..Default::default()
    }
}
fn query(session: &mut Session, root: &str, d: &Deadline) -> anyhow::Result<Value> {
    yoke::compatibility::invoke_serena(
        session,
        "query_project",
        &json!({"project_name":root,"tool_name":"list_dir",
            "tool_params_json":json!({"relative_path":"","recursive":true}).to_string()}),
        d,
    )
}
fn file_paths(value: &Value) -> Vec<&str> {
    value["files"]
        .as_array()
        .unwrap()
        .iter()
        .map(|file| file["path"].as_str().unwrap())
        .collect()
}
#[test]
fn selected_root_alias_and_canonical_root_share_only_the_registered_query_scope() {
    let local = tempfile::tempdir().unwrap();
    let external = tempfile::tempdir().unwrap();
    let root = external.path().join("registered");
    fs::create_dir(&root).unwrap();
    fs::write(root.join("source.rs"), "fn registered_source() {}\n").unwrap();
    let intermediate = external.path().join("host-spelling");
    fs::create_dir(&intermediate).unwrap();
    let selected = intermediate.join("..").join("registered");
    let d = Deadline::new(30_000);
    let mut session = Session::open(
        Engine::open(local.path()).unwrap(),
        host(&selected),
        Some("root-alias"),
        &d,
    )
    .unwrap();
    let canonical = root.canonicalize().unwrap();
    for selector in [
        "reference",
        selected.to_str().unwrap(),
        canonical.to_str().unwrap(),
    ] {
        let result = query(&mut session, selector, &d).unwrap();
        assert_eq!(file_paths(&result), vec!["source.rs"]);
        assert_eq!(
            session.invoke("configuration", &json!({}), &d).unwrap()["activeProject"],
            "default"
        );
    }
    // A different, unregistered spelling is not resolved against the filesystem.
    let other = external.path().join("not-a-host-selector");
    fs::create_dir(&other).unwrap();
    let unregistered = other.join("..").join("registered");
    assert!(
        query(&mut session, unregistered.to_str().unwrap(), &d)
            .unwrap_err()
            .to_string()
            .contains("not host-registered")
    );
    session
        .invoke("removeProject", &json!({"project":"reference"}), &d)
        .unwrap();
    assert!(query(&mut session, selected.to_str().unwrap(), &d).is_err());
    assert_eq!(
        fs::read_to_string(root.join("source.rs")).unwrap(),
        "fn registered_source() {}\n"
    );
}

#[test]
fn aliases_are_rebuilt_from_current_host_input_and_never_restored_from_session_records() {
    let local = tempfile::tempdir().unwrap();
    let external = tempfile::tempdir().unwrap();
    let root = external.path().join("registered");
    fs::create_dir(&root).unwrap();
    fs::write(root.join("source.rs"), "fn source() {}\n").unwrap();
    let other = external.path().join("spelling");
    fs::create_dir(&other).unwrap();
    let selected = other.join("..").join("registered");
    let d = Deadline::new(30_000);
    let first = Session::open(
        Engine::open(local.path()).unwrap(),
        host(&selected),
        Some("resume-alias"),
        &d,
    )
    .unwrap();
    let row: String = first
        .engine
        .store
        .db
        .query_row(
            "SELECT value FROM records WHERE kind=?1",
            [yoke::project_session::RECORD_KIND],
            |r| r.get(0),
        )
        .unwrap();
    assert!(
        serde_json::from_str::<Value>(&row)
            .unwrap()
            .get("projectRootAliases")
            .is_none()
    );
    drop(first);
    let canonical = root.canonicalize().unwrap();
    let mut resumed = Session::open(
        Engine::open(local.path()).unwrap(),
        host(&canonical),
        Some("resume-alias"),
        &d,
    )
    .unwrap();
    assert!(query(&mut resumed, selected.to_str().unwrap(), &d).is_err());
    assert_eq!(
        file_paths(&query(&mut resumed, "reference", &d).unwrap()),
        vec!["source.rs"]
    );
}

#[cfg(windows)]
#[test]
fn unicode_lowercase_expansion_cannot_turn_an_unregistered_root_into_a_selector() {
    let local = tempfile::tempdir().unwrap();
    let external = tempfile::tempdir().unwrap();
    let root = external.path().join("İ-source");
    fs::create_dir(&root).unwrap();
    fs::write(root.join("source.rs"), "fn source() {}\n").unwrap();
    let unrelated = external.path().join("i\u{307}-source");
    let d = Deadline::new(30_000);
    let mut session = Session::open(
        Engine::open(local.path()).unwrap(),
        host(&root),
        Some("unicode-alias"),
        &d,
    )
    .unwrap();
    assert!(query(&mut session, unrelated.to_str().unwrap(), &d).is_err());
    assert_eq!(
        file_paths(&query(&mut session, "reference", &d).unwrap()),
        vec!["source.rs"]
    );
}
