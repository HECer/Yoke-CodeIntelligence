use serde_json::{Value, json};
use std::{fs, path::Path};
use yoke::{
    Engine, agent_repl,
    capture::Deadline,
    project_session::{HostProfile, Session},
};

fn session(root: &Path, global: Option<&Path>, deadline: &Deadline) -> Session {
    fs::write(root.join("main.rs"), "fn main() {}\n").unwrap();
    Session::open(
        Engine::open(root).unwrap(),
        HostProfile {
            global_notes_root: global.map(Path::to_path_buf),
            ..Default::default()
        },
        Some("memory-namespaces"),
        deadline,
    )
    .unwrap()
}
fn invoke(
    session: &mut Session,
    method: &str,
    args: Value,
    deadline: &Deadline,
) -> anyhow::Result<Value> {
    agent_repl::invoke_facade(session, method, args, None, deadline)
}
#[test]
fn default_memory_listing_merges_only_host_registered_namespaces_and_keeps_names_unambiguous() {
    let root = tempfile::tempdir().unwrap();
    let global = tempfile::tempdir().unwrap();
    let deadline = Deadline::new(20000);
    let mut session = session(root.path(), Some(global.path()), &deadline);
    for (name, content) in [
        ("topic/shared", "project"),
        ("global/topic/shared", "global"),
        ("global/other", "other"),
    ] {
        invoke(
            &mut session,
            "mem.write_memory",
            json!({"memory_name":name,"content":content}),
            &deadline,
        )
        .unwrap();
    }
    let listed = invoke(&mut session, "mem.list_memories", json!({}), &deadline).unwrap();
    let names: Vec<_> = listed["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item["name"].as_str().unwrap())
        .collect();
    assert_eq!(
        names,
        ["global/other", "global/topic/shared", "topic/shared"]
    );
    for item in listed["items"].as_array().unwrap() {
        assert_eq!(
            item["scope"],
            if item["name"].as_str().unwrap().starts_with("global/") {
                "global"
            } else {
                "project"
            }
        );
        let read = invoke(
            &mut session,
            "mem.read_memory",
            json!({"memory_name":item["name"]}),
            &deadline,
        )
        .unwrap();
        assert_eq!(read["note"]["scope"], item["scope"]);
        assert_eq!(read["note"]["id"], item["revisionId"]);
    }
    let topic = invoke(
        &mut session,
        "mem.list_memories",
        json!({"topic":"topic"}),
        &deadline,
    )
    .unwrap();
    assert_eq!(
        topic["items"]
            .as_array()
            .unwrap()
            .iter()
            .map(|item| item["name"].as_str().unwrap())
            .collect::<Vec<_>>(),
        ["global/topic/shared", "topic/shared"]
    );
    let global_only = invoke(
        &mut session,
        "mem.list_memories",
        json!({"topic":"global/topic"}),
        &deadline,
    )
    .unwrap();
    assert_eq!(global_only["items"].as_array().unwrap().len(), 1);
    assert_eq!(global_only["items"][0]["name"], "global/topic/shared");
}
#[test]
fn unregistered_global_namespace_is_not_created_or_probed_and_registered_errors_propagate() {
    let root = tempfile::tempdir().unwrap();
    let deadline = Deadline::new(20000);
    let mut local = session(root.path(), None, &deadline);
    invoke(
        &mut local,
        "mem.write_memory",
        json!({"memory_name":"local","content":"local"}),
        &deadline,
    )
    .unwrap();
    let listed = invoke(&mut local, "mem.list_memories", json!({}), &deadline).unwrap();
    assert_eq!(listed["items"].as_array().unwrap().len(), 1);
    assert_eq!(listed["items"][0]["name"], "local");
    assert!(
        invoke(
            &mut local,
            "mem.list_memories",
            json!({"topic":"global/topic"}),
            &deadline
        )
        .is_err()
    );
    let other = tempfile::tempdir().unwrap();
    let global = tempfile::tempdir().unwrap();
    let unavailable = global.path().join("registered-file");
    fs::create_dir(&unavailable).unwrap();
    let mut broken = session(other.path(), Some(&unavailable), &deadline);
    fs::remove_dir(&unavailable).unwrap();
    fs::write(&unavailable, "not a directory").unwrap();
    assert!(
        invoke(&mut broken, "mem.list_memories", json!({}), &deadline).is_err(),
        "A registered backend failure must not become a partial project-only success"
    );
}
