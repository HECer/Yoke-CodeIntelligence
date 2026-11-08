use serde_json::json;
use std::fs;
use tempfile::TempDir;
use yoke::{Engine, capture::Deadline};

#[test]
fn explicitly_captured_dependencies_are_queryable_but_never_writable() {
    let root = TempDir::new().unwrap();
    fs::create_dir_all(root.path().join("node_modules/pkg")).unwrap();
    fs::write(
        root.path().join("node_modules/pkg/index.js"),
        "export function dependencyAlpha() { return 7; }\n",
    )
    .unwrap();
    fs::write(root.path().join("main.js"), "const alpha = 1;\n").unwrap();
    let mut engine = Engine::open(root.path()).unwrap();
    engine.capture_options.read_only_dependency_dirs = vec!["node_modules".into()];
    let (snapshot, graph, _, _) = engine.index(&Deadline::new(20000)).unwrap();
    let read = engine
        .invoke(
            "ci_read",
            json!({"snapshotId":snapshot.id,"path":"node_modules/pkg/index.js"}),
        )
        .unwrap();
    assert_eq!(read["status"], "ok", "{read}");
    assert!(
        read["data"]["text"]
            .as_str()
            .unwrap()
            .contains("dependencyAlpha")
    );
    let found=engine.invoke("ci_find",json!({"snapshotId":snapshot.id,"kind":"text","query":"dependencyAlpha","relativePath":"node_modules/pkg"})).unwrap();
    assert_eq!(found["status"], "ok", "{found}");
    assert_eq!(found["data"]["items"].as_array().unwrap().len(), 1);
    let symbol = graph
        .entities
        .iter()
        .find(|e| e.name == "dependencyAlpha")
        .unwrap();
    let edit=engine.invoke("ci_change_prepare",json!({"snapshotId":snapshot.id,"replaceSymbol":{"entityId":symbol.id,"text":"export function compromised() {}"}})).unwrap();
    assert_eq!(edit["status"], "error", "{edit}");
    assert!(
        fs::read_to_string(root.path().join("node_modules/pkg/index.js"))
            .unwrap()
            .contains("dependencyAlpha")
    );
}
