use serde_json::{Value, json};
use yoke::Engine;

#[test]
fn large_graphify_resources_preserve_the_complete_original_body_in_paged_storage() {
    use yoke::{
        capture::Deadline,
        project_session::{HostProfile, Session},
    };
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("main.rs"), "fn main() {}\n").unwrap();
    for i in 0..400 {
        std::fs::write(
            root.path().join(format!(
                "invalid_utf8_source_{i:04}_with_a_descriptive_filename.rs"
            )),
            [255, 254],
        )
        .unwrap();
    }
    let deadline = Deadline::new(120000);
    let mut session = Session::open(
        Engine::open(root.path()).unwrap(),
        HostProfile::default(),
        Some("large-resource"),
        &deadline,
    )
    .unwrap();
    let (original_mime, original) =
        yoke::graphify_adapter::resource(&mut session, "graphify://audit", &deadline).unwrap();
    assert!(
        original.len() > 32000,
        "fixture must exceed inline capacity: {}",
        original.len()
    );
    let input = json!({"jsonrpc":"2.0","id":0,"method":"initialize"}).to_string()+"\n"+
        &json!({"jsonrpc":"2.0","id":1,"method":"resources/read","params":{"uri":"graphify://audit"}}).to_string()+"\n";
    let mut output = vec![];
    yoke::protocol::serve_graphify(&mut session, std::io::Cursor::new(input), &mut output).unwrap();
    let rows: Vec<Value> = String::from_utf8(output)
        .unwrap()
        .lines()
        .map(|row| serde_json::from_str(row).unwrap())
        .collect();
    let response = &rows[1];
    assert!(response.get("error").is_none(), "{response}");
    let content = &response["result"]["contents"][0];
    assert_eq!(content["mimeType"], "application/json");
    let descriptor: Value = serde_json::from_str(content["text"].as_str().unwrap()).unwrap();
    assert_eq!(descriptor["originalMimeType"], original_mime);
    assert_eq!(descriptor["totalBytes"], original.len());
    assert_eq!(descriptor["resultTransport"], "immutable_paged_artifact");
    let engine = session.active_engine_at(&deadline).unwrap();
    assert_eq!(
        engine
            .store
            .read_artifact(descriptor["artifactId"].as_str().unwrap())
            .unwrap(),
        original.as_bytes()
    );
    assert!(
        descriptor["resourceUri"]
            .as_str()
            .unwrap()
            .starts_with("ci://artifact/")
    );
}

#[test]
fn project_resource_pages_use_only_registered_roots_without_activation() {
    use yoke::{
        capture::Deadline,
        project_session::{HostProfile, HostProject, Session},
    };
    let primary = tempfile::tempdir().unwrap();
    let reference = tempfile::tempdir().unwrap();
    let foreign = tempfile::tempdir().unwrap();
    let deadline = Deadline::new(20000);
    let reference_engine = Engine::open(reference.path()).unwrap();
    let id = reference_engine
        .store
        .artifact(b"registered reference data")
        .unwrap();
    drop(reference_engine);
    let mut session = Session::open(
        Engine::open(primary.path()).unwrap(),
        HostProfile {
            projects: vec![HostProject {
                name: "reference".into(),
                root: reference.path().to_owned(),
                read_only: true,
                query_only: true,
            }],
            ..Default::default()
        },
        Some("resource-project"),
        &deadline,
    )
    .unwrap();
    let uri = |project: &std::path::Path| {
        let query = url::form_urlencoded::Serializer::new(String::new())
            .append_pair(
                "project",
                &project.canonicalize().unwrap().to_string_lossy(),
            )
            .append_pair("maxBytes", "1024")
            .finish();
        format!("ci://artifact/{id}?{query}")
    };
    let messages = [
        json!({"jsonrpc":"2.0","id":1,"method":"initialize"}),
        json!({"jsonrpc":"2.0","id":2,"method":"resources/read","params":{"uri":uri(reference.path())}}),
        json!({"jsonrpc":"2.0","id":3,"method":"resources/read","params":{"uri":uri(foreign.path())}}),
    ];
    let input = messages
        .iter()
        .map(|value| value.to_string() + "\n")
        .collect::<String>();
    let mut output = vec![];
    yoke::protocol::serve_session(&mut session, std::io::Cursor::new(input), &mut output).unwrap();
    let rows: Vec<Value> = String::from_utf8(output)
        .unwrap()
        .lines()
        .map(|row| serde_json::from_str(row).unwrap())
        .collect();
    assert!(rows[1].get("error").is_none(), "{}", rows[1]);
    let page: Value =
        serde_json::from_str(rows[1]["result"]["contents"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(page["chunk"], "registered reference data");
    assert!(rows[2].get("error").is_some());
    assert_eq!(
        session.active_engine_at(&deadline).unwrap().root,
        primary.path().canonicalize().unwrap()
    );
    assert!(!foreign.path().join(".yoke").exists());
}
#[test]
fn generic_resource_pages_reach_large_artifacts_under_the_original_eight_tool_profile() {
    let root = tempfile::tempdir().unwrap();
    let mut engine = Engine::open(root.path()).unwrap();
    let id = engine.store.artifact(&vec![b'x'; 160000]).unwrap();
    let messages = [
        json!({"jsonrpc":"2.0","id":1,"method":"initialize"}),
        json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}),
        json!({"jsonrpc":"2.0","id":3,"method":"resources/read","params":{"uri":format!("ci://artifact/{id}?cursor=100000&maxBytes=1024")}}),
    ];
    let input = messages
        .iter()
        .map(|row| row.to_string() + "\n")
        .collect::<String>();
    let mut output = Vec::new();
    yoke::protocol::serve(&mut engine, std::io::Cursor::new(input), &mut output).unwrap();
    let rows: Vec<Value> = String::from_utf8(output)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(rows[1]["result"]["tools"].as_array().unwrap().len(), 8);
    assert!(rows[2].get("error").is_none(), "{}", rows[2]);
    let page: Value =
        serde_json::from_str(rows[2]["result"]["contents"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(page["cursor"], "100000");
    assert_eq!(page["totalBytes"], 160000);
    assert_eq!(page["chunk"].as_str().unwrap(), "x".repeat(204));
}
#[test]
fn streaming_pages_verify_bytes_outside_the_page_and_preserve_unicode_boundaries() {
    use std::io::{Seek, SeekFrom, Write};
    let root = tempfile::tempdir().unwrap();
    let engine = Engine::open(root.path()).unwrap();
    let text = format!("{}🌍{}", "x".repeat(63999), "y".repeat(64001));
    let id = engine.store.artifact(text.as_bytes()).unwrap();
    let page = engine
        .store
        .read_artifact_page(&id, 63999, 5, true)
        .unwrap();
    assert_eq!(String::from_utf8(page.bytes).unwrap(), "🌍y");
    assert!(
        engine
            .store
            .read_artifact_page(&id, 64000, 10, true)
            .is_err()
    );
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .open(engine.store.dir.join("blobs").join(&id))
        .unwrap();
    file.seek(SeekFrom::End(-1)).unwrap();
    file.write_all(b"z").unwrap();
    assert!(engine.store.read_artifact_page(&id, 0, 100, true).is_err());
}
