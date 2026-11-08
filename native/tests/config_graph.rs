use std::fs;
use tempfile::TempDir;
use yoke::{Engine, capture::Deadline, config_graph};
#[test]
fn literal_dotted_keys_cannot_collapse_into_nested_key_paths() {
    let d = TempDir::new().unwrap();
    fs::write(d.path().join("config.json"), r##"{"a.b":1,"a":{"b":2}}"##).unwrap();
    let mut e = Engine::open(d.path()).unwrap();
    let (s, g, _, _) = e.index(&Deadline::new(10000)).unwrap();
    let out = config_graph::extract(&e.store, &s, &g, &Deadline::new(10000)).unwrap();
    let keys: Vec<_> = out
        .nodes
        .iter()
        .filter(|n| n["kind"] == "configuration_key")
        .collect();
    assert_eq!(keys.len(), 3);
    assert!(keys.iter().any(|n| n["name"] == "a~1b"));
    assert!(keys.iter().any(|n| n["name"] == "a.b"));
}
#[test]
fn local_json_references_link_declared_keys_and_duplicate_fields_are_rejected() {
    let d = TempDir::new().unwrap();
    fs::write(
        d.path().join("config.json"),
        r##"{"service":{"$ref":"#/definitions/User"},"definitions":{"User":{"type":"object"}}}"##,
    )
    .unwrap();
    let mut e = Engine::open(d.path()).unwrap();
    let (s, g, _, _) = e.index(&Deadline::new(10000)).unwrap();
    let out = config_graph::extract(&e.store, &s, &g, &Deadline::new(10000)).unwrap();
    let target = out
        .nodes
        .iter()
        .find(|n| n["name"] == "definitions.User")
        .unwrap();
    assert!(
        out.observations
            .iter()
            .any(|o| o["relation"] == "references_configuration"
                && o["target"] == target["id"]
                && o["resolution"] == "authored_local_pointer")
    );
    fs::write(
        d.path().join("config.json"),
        "{\"service\":1,\"service\":2}",
    )
    .unwrap();
    let (s, g, _, _) = e.index(&Deadline::new(10000)).unwrap();
    assert!(config_graph::extract(&e.store, &s, &g, &Deadline::new(10000)).is_err());
}
#[test]
fn dotnet_projects_forms_and_byond_assets_keep_authored_dependency_paths() {
    let d = TempDir::new().unwrap();
    for (path, text) in [
        (
            "app.csproj",
            "<Project><ItemGroup><ProjectReference Include='lib.csproj'/><Compile Include='Payment.cs'/><PackageReference Include='Newtonsoft.Json' Version='13'/></ItemGroup></Project>",
        ),
        (
            "solution.sln",
            "Project(\"{A}\") = \"Library\", \"lib.csproj\", \"{B}\"\nEndProject\n",
        ),
        (
            "form.dfm",
            "object PaymentForm: TForm\n  Caption = 'Payments'\n  object PayButton: TButton\n    OnClick = PaymentClick\n  end\nend\n",
        ),
        (
            "world.dmf",
            "window \"main\"\n elem \"view\"\n  type = MAIN\n  icon = 'assets/world.dmi'\n",
        ),
    ] {
        fs::write(d.path().join(path), text).unwrap();
    }
    let mut e = Engine::open(d.path()).unwrap();
    let (s, g, _, _) = e.index(&Deadline::new(10000)).unwrap();
    let out = config_graph::extract(&e.store, &s, &g, &Deadline::new(10000)).unwrap();
    for (label, relation) in [
        ("lib.csproj", "references_project"),
        ("Payment.cs", "includes_source"),
        ("Newtonsoft.Json", "references_package"),
        ("PaymentClick", "references_handler"),
        ("assets/world.dmi", "references_asset"),
    ] {
        assert!(
            out.observations
                .iter()
                .any(|o| o["targetLabel"] == label && o["relation"] == relation),
            "{relation}:{label}"
        );
    }
    assert!(
        out.nodes
            .iter()
            .any(|n| n["kind"] == "form_component" && n["name"] == "PayButton")
    );
}
#[test]
fn yaml_alias_expansion_budget_fails_without_publishing() {
    let d = TempDir::new().unwrap();
    let mut text = String::from("a: &a [word, word]\n");
    let mut prior = String::from("a");
    for i in 0..7 {
        let name = format!("level{i}");
        text.push_str(&format!(
            "{name}: &{name} [*{prior}, *{prior}, *{prior}, *{prior}]\n"
        ));
        prior = name;
    }
    fs::write(d.path().join("config.yaml"), text).unwrap();
    let mut e = Engine::open(d.path()).unwrap();
    let (s, g, _, _) = e.index(&Deadline::new(10000)).unwrap();
    assert!(config_graph::extract(&e.store, &s, &g, &Deadline::new(10000)).is_err());
    let count: i64 = e
        .store
        .db
        .query_row(
            "SELECT COUNT(*) FROM records WHERE kind='native_derived_source_graph_v1'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(count, 0);
}
#[test]
fn mcp_launcher_packages_are_described_without_argument_or_environment_values() {
    let d = TempDir::new().unwrap();
    fs::write(d.path().join("mcp.json"),r##"{"mcpServers":{"x":{"command":"npx","args":["-y","@org/server@2","--token","private-argument-value"],"env":{"API_TOKEN":"private-env-value"}}}}"##).unwrap();
    let mut e = Engine::open(d.path()).unwrap();
    let (s, g, _, _) = e.index(&Deadline::new(10000)).unwrap();
    let out = config_graph::extract(&e.store, &s, &g, &Deadline::new(10000)).unwrap();
    assert!(
        out.observations
            .iter()
            .any(|o| o["relation"] == "declares_package" && o["targetLabel"] == "@org/server")
    );
    let raw = serde_json::to_string(&out).unwrap();
    assert!(!raw.contains("private-argument-value") && !raw.contains("private-env-value"));
}
#[test]
fn xml_configuration_repeated_names_keep_distinct_parent_paths() {
    let d = TempDir::new().unwrap();
    fs::write(
        d.path().join("config.xml"),
        "<settings><left><value/></left><right><value/></right></settings>",
    )
    .unwrap();
    let mut e = Engine::open(d.path()).unwrap();
    let (s, g, _, _) = e.index(&Deadline::new(10000)).unwrap();
    let out = config_graph::extract(&e.store, &s, &g, &Deadline::new(10000)).unwrap();
    assert!(out.nodes.iter().any(|n| n["name"] == "settings.left.value"));
    assert!(
        out.nodes
            .iter()
            .any(|n| n["name"] == "settings.right.value")
    );
}
#[test]
fn mcp_config_environment_values_are_never_persisted_or_executed() {
    let d = TempDir::new().unwrap();
    fs::write(d.path().join("mcp.json"),r##"{"mcpServers":{"database":{"command":"node","args":["server.mjs"],"env":{"DATABASE_URL":"private-test-content-never-store"}}}}"##).unwrap();
    let mut e = Engine::open(d.path()).unwrap();
    let (s, g, _, _) = e.index(&Deadline::new(10000)).unwrap();
    let out = config_graph::extract(&e.store, &s, &g, &Deadline::new(10000)).unwrap();
    assert!(
        out.nodes
            .iter()
            .any(|n| n["kind"] == "mcp_server" && n["name"] == "database")
    );
    assert!(
        out.nodes
            .iter()
            .any(|n| n["kind"] == "environment_name" && n["name"] == "DATABASE_URL")
    );
    assert!(
        !serde_json::to_string(&out)
            .unwrap()
            .contains("private-test-content")
    );
    assert!(
        out.observations
            .iter()
            .any(|o| o["relation"] == "declares_command" && o["targetLabel"] == "node")
    );
}
#[test]
fn config_keys_and_references_parse_json_toml_yaml_and_xml() {
    let d = TempDir::new().unwrap();
    for (p, t) in [
        (
            "a.json",
            r##"{"service":{"$ref":"#/definitions/User"},"definitions":{"User":{"type":"object"}}}"##,
        ),
        ("b.toml", "[service]\nurl='${API_HOST}'\n"),
        ("c.yaml", "service:\n  reference: '${API_HOST}'\n"),
        ("d.xml", "<settings><service ref='User'/></settings>"),
    ] {
        fs::write(d.path().join(p), t).unwrap();
    }
    let mut e = Engine::open(d.path()).unwrap();
    let (s, g, _, _) = e.index(&Deadline::new(10000)).unwrap();
    let out = config_graph::extract(&e.store, &s, &g, &Deadline::new(10000)).unwrap();
    for p in ["a.json", "b.toml", "c.yaml", "d.xml"] {
        assert!(
            out.nodes
                .iter()
                .any(|n| n["path"] == p && n["kind"] == "configuration_key"),
            "{p}"
        );
    }
    assert!(
        out.observations
            .iter()
            .any(|o| o["targetLabel"] == "API_HOST" && o["relation"] == "references_environment")
    );
}
