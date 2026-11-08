use std::{fs, process::Command};
use tempfile::TempDir;
use yoke::{Engine, capture::Deadline, config_graph, graph_exports, manifest_graph};

#[test]
fn review_launcher_option_values_are_not_persisted_as_packages() {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("mcp.json"), r#"{"mcpServers":{"test":{"command":"npx","args":["--userconfig","private-option-value","@org/server"]}}}"#).unwrap();
    let mut engine = Engine::open(dir.path()).unwrap();
    let (snapshot, graph, _, _) = engine.index(&Deadline::new(10000)).unwrap();
    let derived =
        config_graph::extract(&engine.store, &snapshot, &graph, &Deadline::new(10000)).unwrap();
    assert!(
        !serde_json::to_string(&derived)
            .unwrap()
            .contains("private-option-value")
    );
}

#[test]
fn review_json_pointer_can_address_array_child_keys() {
    let dir = TempDir::new().unwrap();
    fs::write(
        dir.path().join("config.json"),
        r##"{"source":{"$ref":"#/items/0/value"},"items":[{"value":1}]}"##,
    )
    .unwrap();
    let mut engine = Engine::open(dir.path()).unwrap();
    let (snapshot, graph, _, _) = engine.index(&Deadline::new(10000)).unwrap();
    let derived =
        config_graph::extract(&engine.store, &snapshot, &graph, &Deadline::new(10000)).unwrap();
    let target = derived
        .nodes
        .iter()
        .find(|node| node["name"] == "items[0].value")
        .unwrap();
    assert!(derived.observations.iter().any(
        |edge| edge["relation"] == "references_configuration" && edge["target"] == target["id"]
    ));
}

#[test]
fn review_yaml_mcp_config_preserves_server_contract() {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("mcp.yaml"), "mcpServers:\n  test:\n    command: npx\n    args: ['@org/server@2']\n    env:\n      API_TOKEN: private-value\n").unwrap();
    let mut engine = Engine::open(dir.path()).unwrap();
    let (snapshot, graph, _, _) = engine.index(&Deadline::new(10000)).unwrap();
    let derived =
        config_graph::extract(&engine.store, &snapshot, &graph, &Deadline::new(10000)).unwrap();
    assert!(
        derived
            .nodes
            .iter()
            .any(|node| node["kind"] == "mcp_server" && node["name"] == "test")
    );
}

#[test]
fn review_cargo_ordinary_ancestor_does_not_shadow_workspace() {
    let dir = TempDir::new().unwrap();
    fs::create_dir_all(dir.path().join("outer/client")).unwrap();
    fs::write(dir.path().join("Cargo.toml"), "[workspace]\nmembers=['outer','outer/client']\n[workspace.dependencies]\nalias={package='actual',version='1'}\n").unwrap();
    fs::write(
        dir.path().join("outer/Cargo.toml"),
        "[package]\nname='outer'\nversion='1'\n",
    )
    .unwrap();
    fs::write(
        dir.path().join("outer/client/Cargo.toml"),
        "[package]\nname='client'\nversion='1'\n[dependencies]\nalias.workspace=true\n",
    )
    .unwrap();
    let mut engine = Engine::open(dir.path()).unwrap();
    let (snapshot, graph, _, _) = engine.index(&Deadline::new(10000)).unwrap();
    let derived =
        manifest_graph::extract(&engine.store, &snapshot, &graph, &Deadline::new(10000)).unwrap();
    assert!(
        derived
            .observations
            .iter()
            .any(|edge| edge["dependencyAlias"] == "alias" && edge["targetLabel"] == "actual")
    );
}

#[test]
fn review_tree_export_script_is_valid_javascript() {
    let dir = TempDir::new().unwrap();
    fs::write(
        dir.path().join("app.ts"),
        "export function run() { return 1; }\n",
    )
    .unwrap();
    let mut engine = Engine::open(dir.path()).unwrap();
    let (snapshot, graph, _, _) = engine.index(&Deadline::new(10000)).unwrap();
    let bundle = graph_exports::generate(
        &engine.store,
        &snapshot,
        &graph,
        &graph_exports::ExportOptions {
            format: graph_exports::ExportFormat::Tree,
            max_nodes: 5000,
            max_edges: 50000,
        },
        &Deadline::new(10000),
    )
    .unwrap();
    let html = &bundle.files[0].content;
    let script = html
        .rsplit_once("<script>")
        .unwrap()
        .1
        .split_once("</script>")
        .unwrap()
        .0;
    let path = dir.path().join("export-script.cjs");
    fs::write(&path, script).unwrap();
    let output = Command::new("node")
        .arg("--check")
        .arg(path)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}
