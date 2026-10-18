use std::fs;
use tempfile::TempDir;
use yoke::{Engine, capture::Deadline, manifest_graph};
#[test]
fn derived_publication_refuses_an_existing_record_with_different_scope() {
    let d = TempDir::new().unwrap();
    fs::write(d.path().join("package.json"), "{\"name\":\"app\"}").unwrap();
    let mut e = Engine::open(d.path()).unwrap();
    let (s, g, _, _) = e.index(&Deadline::new(10000)).unwrap();
    let out = manifest_graph::extract(&e.store, &s, &g, &Deadline::new(10000)).unwrap();
    e.store
        .db
        .execute(
            "UPDATE records SET value='{}' WHERE kind=?1 AND id=?2",
            rusqlite::params![manifest_graph::RECORD_KIND, out.artifact_id],
        )
        .unwrap();
    assert!(manifest_graph::extract(&e.store, &s, &g, &Deadline::new(10000)).is_err());
    let value: String = e
        .store
        .db
        .query_row(
            "SELECT value FROM records WHERE kind=?1 AND id=?2",
            rusqlite::params![manifest_graph::RECORD_KIND, out.artifact_id],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(value, "{}");
}
#[test]
fn cargo_inheritance_uses_the_nearest_declared_workspace() {
    let d = TempDir::new().unwrap();
    fs::create_dir_all(d.path().join("nested/client")).unwrap();
    fs::write(d.path().join("Cargo.toml"),"[workspace]\nmembers=[]\nexclude=['nested']\n[workspace.dependencies]\nalias={package='wrong',version='1'}\n").unwrap();
    fs::write(d.path().join("nested/Cargo.toml"),"[workspace]\nmembers=['client']\n[workspace.dependencies]\nalias={package='correct',version='1'}\n").unwrap();
    fs::write(
        d.path().join("nested/client/Cargo.toml"),
        "[package]\nname='client'\nversion='1'\n[dependencies]\nalias.workspace=true\n",
    )
    .unwrap();
    let mut e = Engine::open(d.path()).unwrap();
    let (s, g, _, _) = e.index(&Deadline::new(10000)).unwrap();
    let out = manifest_graph::extract(&e.store, &s, &g, &Deadline::new(10000)).unwrap();
    assert!(
        out.observations
            .iter()
            .any(|o| o["dependencyAlias"] == "alias" && o["targetLabel"] == "correct")
    );
    assert!(!out.observations.iter().any(|o| o["targetLabel"] == "wrong"));
}
#[test]
fn apm_yaml_manifest_dependencies_are_not_treated_as_npm() {
    let d = TempDir::new().unwrap();
    fs::write(
        d.path().join("apm.yml"),
        "name: agent-project\ndependencies:\n  - org/tool\n  - org/library\n",
    )
    .unwrap();
    let mut e = Engine::open(d.path()).unwrap();
    let (s, g, _, _) = e.index(&Deadline::new(10000)).unwrap();
    let out = manifest_graph::extract(&e.store, &s, &g, &Deadline::new(10000)).unwrap();
    assert!(out.nodes.iter().any(|n| n["kind"] == "package"
        && n["name"] == "agent-project"
        && n["ecosystem"] == "apm"));
    assert!(
        out.observations
            .iter()
            .any(|o| o["targetLabel"] == "org/tool" && o["ecosystem"] == "apm")
    );
}
#[test]
fn cargo_path_link_must_match_declared_package_identity() {
    let d = TempDir::new().unwrap();
    fs::create_dir(d.path().join("other")).unwrap();
    fs::write(
        d.path().join("Cargo.toml"),
        "[package]\nname='app'\nversion='1'\n[dependencies]\nexpected={path='other'}\n",
    )
    .unwrap();
    fs::write(
        d.path().join("other/Cargo.toml"),
        "[package]\nname='different'\nversion='1'\n",
    )
    .unwrap();
    let mut e = Engine::open(d.path()).unwrap();
    let (s, g, _, _) = e.index(&Deadline::new(10000)).unwrap();
    let out = manifest_graph::extract(&e.store, &s, &g, &Deadline::new(10000)).unwrap();
    let edge = out
        .observations
        .iter()
        .find(|o| o["dependencyAlias"] == "expected")
        .unwrap();
    assert!(
        edge["target"].is_null(),
        "A path with another package name is not a valid Cargo binding"
    );
    assert_eq!(edge["resolution"], "manifest_package_mismatch");
}
#[test]
fn invalid_xml_or_expired_capture_cannot_publish_derived_graph() {
    let d = TempDir::new().unwrap();
    fs::write(d.path().join("pom.xml"),"<!DOCTYPE project [<!ENTITY secret SYSTEM 'file:///private'>]><project><artifactId>x</artifactId></project>").unwrap();
    let mut e = Engine::open(d.path()).unwrap();
    let (s, g, _, _) = e.index(&Deadline::new(10000)).unwrap();
    assert!(manifest_graph::extract(&e.store, &s, &g, &Deadline::new(10000)).is_err());
    assert!(manifest_graph::extract(&e.store, &s, &g, &Deadline::new(0)).is_err());
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
fn package_json_graph_preserves_aliases_and_never_stores_credentials() {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("package.json"),r#"{"name":"payments","dependencies":{"alias":"npm:real-package@^2","vue":"^3"},"devDependencies":{"vitest":"^2"},"scripts":{"unsafe":"echo never-execute"}}"#).unwrap();
    let mut engine = Engine::open(dir.path()).unwrap();
    let (s, g, _, _) = engine.index(&Deadline::new(10000)).unwrap();
    let out = manifest_graph::extract(&engine.store, &s, &g, &Deadline::new(10000)).unwrap();
    assert!(
        out.nodes
            .iter()
            .any(|n| n["name"] == "payments" && n["kind"] == "package")
    );
    assert!(
        out.observations
            .iter()
            .any(|e| e["dependencyAlias"] == "alias"
                && e["targetLabel"] == "real-package"
                && e["relation"] == "depends_on")
    );
    assert!(
        out.observations
            .iter()
            .any(|e| e["targetLabel"] == "vitest" && e["scope"] == "development")
    );
    assert!(
        !serde_json::to_string(&out)
            .unwrap()
            .contains("never-execute")
    );
    assert_eq!(out.snapshot_id, s.id);
    assert_eq!(
        engine.store.graph(&s.id).unwrap().observations.len(),
        g.observations.len()
    );
    for e in &out.evidence {
        let f = s.files.iter().find(|f| f.path == e["path"]).unwrap();
        assert_eq!(e["contentHash"], f.hash);
        let text = fs::read_to_string(dir.path().join(&f.path)).unwrap();
        let a = e["startByte"].as_u64().unwrap() as usize;
        let b = e["endByte"].as_u64().unwrap() as usize;
        assert!(text.is_char_boundary(a) && text.is_char_boundary(b));
    }
}
#[test]
fn cargo_workspace_inherits_renamed_path_dependencies_without_merging_packages() {
    let dir = TempDir::new().unwrap();
    fs::create_dir_all(dir.path().join("crates/client")).unwrap();
    fs::create_dir_all(dir.path().join("crates/core")).unwrap();
    fs::write(dir.path().join("Cargo.toml"),"[workspace]\nmembers=['crates/*']\n[workspace.dependencies]\nrenamed = { package='actual-core', path='crates/core' }\n").unwrap();
    fs::write(
        dir.path().join("crates/core/Cargo.toml"),
        "[package]\nname='actual-core'\nversion='1.0.0'\n",
    )
    .unwrap();
    fs::write(
        dir.path().join("crates/client/Cargo.toml"),
        "[package]\nname='client'\nversion='1.0.0'\n[dependencies]\nrenamed.workspace=true\n",
    )
    .unwrap();
    let mut engine = Engine::open(dir.path()).unwrap();
    let (s, g, _, _) = engine.index(&Deadline::new(10000)).unwrap();
    let out = manifest_graph::extract(&engine.store, &s, &g, &Deadline::new(10000)).unwrap();
    let client = out.nodes.iter().find(|n| n["name"] == "client").unwrap();
    let core = out
        .nodes
        .iter()
        .find(|n| n["name"] == "actual-core" && n["kind"] == "package")
        .unwrap();
    assert!(out.observations.iter().any(|e| e["source"] == client["id"]
        && e["target"] == core["id"]
        && e["dependencyAlias"] == "renamed"
        && e["resolution"] == "manifest_path"));
}
#[test]
fn pyproject_go_and_pom_declarations_have_distinct_ecosystems() {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("pyproject.toml"),"[project]\nname='python-app'\ndependencies=['requests>=2', 'rich[all]>=13; python_version >= \"3.10\"']\n").unwrap();
    fs::write(
        dir.path().join("go.mod"),
        "module example.test/payments\nrequire (\n example.test/library v1.2.3\n)\n",
    )
    .unwrap();
    fs::write(dir.path().join("pom.xml"),"<project><groupId>org.example</groupId><artifactId>app</artifactId><dependencies><dependency><groupId>org.library</groupId><artifactId>api</artifactId><version>2</version></dependency></dependencies></project>").unwrap();
    let mut engine = Engine::open(dir.path()).unwrap();
    let (s, g, _, _) = engine.index(&Deadline::new(10000)).unwrap();
    let out = manifest_graph::extract(&engine.store, &s, &g, &Deadline::new(10000)).unwrap();
    for (ecosystem, name) in [
        ("pypi", "requests"),
        ("go", "example.test/library"),
        ("maven", "org.library:api"),
    ] {
        assert!(
            out.observations
                .iter()
                .any(|e| e["ecosystem"] == ecosystem && e["targetLabel"] == name),
            "{ecosystem}:{name}"
        );
    }
}
