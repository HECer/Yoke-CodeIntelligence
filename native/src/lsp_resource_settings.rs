//! Explicit settings data bound to captured file membership, never host authority.
use super::{HostConfiguration, bounded_host_json, edit_relative, uri};
use crate::{capture::Deadline, model::Snapshot};
use anyhow::{Result, ensure};
use serde_json::{Map, Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
};

pub(crate) type Entries = BTreeMap<String, Map<String, Value>>;

pub(crate) fn validate_entries(entries: &Entries) -> Result<()> {
    ensure!(entries.len() <= 100, "resource_settings_entry_capacity");
    ensure!(
        serde_json::to_vec(entries)?.len() <= 128000,
        "resource_settings_total_capacity"
    );
    for (path, settings) in entries {
        ensure!(
            crate::paths::normalized(path)? == *path,
            "resource_settings_canonical_path_required"
        );
        crate::server_settings::Settings {
            settings: Some(settings.clone()),
            ..Default::default()
        }
        .validate()?;
    }
    Ok(())
}

pub(crate) fn effective(
    configuration: &HostConfiguration,
) -> Result<Option<BTreeMap<String, Value>>> {
    let Some(entries) = &configuration.resource_settings else {
        return Ok(None);
    };
    validate_entries(entries)?;
    ensure!(
        configuration.settings.is_null() || configuration.settings.is_object(),
        "resource_settings_workspace_object_required"
    );
    let base = configuration
        .settings
        .as_object()
        .cloned()
        .unwrap_or_default();
    let mut result = BTreeMap::new();
    for (path, settings) in entries {
        let mut merged = base.clone();
        merged.extend(settings.clone());
        validate_value(&merged)?;
        result.insert(path.clone(), Value::Object(merged));
    }
    Ok(Some(result))
}

fn validate_value(value: &Map<String, Value>) -> Result<()> {
    crate::server_settings::Settings {
        settings: Some(value.clone()),
        ..Default::default()
    }
    .validate()
}

pub(super) fn enforce_passive_rust(value: &mut Value) -> Result<()> {
    for section in ["", "rust-analyzer"] {
        let value = if section.is_empty() {
            &mut *value
        } else {
            &mut value[section]
        };
        if value.is_null() {
            *value = json!({});
        }
        ensure!(
            value.is_object()
                && (value["cargo"].is_null() || value["cargo"].is_object())
                && (value["cargo"]["buildScripts"].is_null()
                    || value["cargo"]["buildScripts"].is_object())
                && (value["procMacro"].is_null() || value["procMacro"].is_object()),
            "resource_settings_passive_rust_shape_invalid"
        );
        value["cargo"]["buildScripts"]["enable"] = json!(false);
        value["procMacro"]["enable"] = json!(false);
        value["checkOnSave"] = json!(false);
    }
    validate_value(
        value
            .as_object()
            .expect("validated resource settings object"),
    )
}

pub(super) struct Binding {
    root: PathBuf,
    root_uri: String,
    members: BTreeSet<String>,
    settings: BTreeMap<String, Value>,
}
impl Binding {
    pub(super) fn new(
        root: &Path,
        snapshot: &Snapshot,
        exclusions: &BTreeSet<String>,
        settings: BTreeMap<String, Value>,
        deadline: &Deadline,
    ) -> Result<Self> {
        let mut members = BTreeSet::new();
        for file in &snapshot.files {
            deadline.check()?;
            if !exclusions.contains(&file.path) {
                members.insert(file.path.clone());
            }
        }
        ensure!(
            settings.keys().all(|path| members.contains(path)),
            "resource_settings_captured_file_required"
        );
        for value in settings.values() {
            bounded_host_json(value, 64000, deadline)?;
        }
        Ok(Self {
            root: root.to_path_buf(),
            root_uri: uri(root)?,
            members,
            settings,
        })
    }
    pub(super) fn select<'a>(
        &'a self,
        scope: Option<&Value>,
        base: &'a Value,
    ) -> Result<&'a Value> {
        let scope = match scope {
            None | Some(Value::Null) => return Ok(base),
            Some(value) => value
                .as_str()
                .ok_or_else(|| anyhow::anyhow!("Configuration callback scope must be a string"))?,
        };
        ensure!(
            scope.len() <= 8192,
            "Configuration callback scope budget exceeded"
        );
        if scope == self.root_uri {
            return Ok(base);
        }
        let Ok(path) = edit_relative(&self.root, scope) else {
            return Ok(&Value::Null);
        };
        if !self.members.contains(&path) {
            return Ok(&Value::Null);
        }
        Ok(self.settings.get(&path).unwrap_or(base))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn staged_actions_receive_the_same_source_bound_resource_configuration() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("source");
        std::fs::create_dir_all(root.join("unicode")).unwrap();
        let source = "// owned \u{1f30d}\r\nfn visible() {}\r\n";
        std::fs::write(root.join("main.rs"), source).unwrap();
        std::fs::write(
            root.join("unicode/e\u{301}-\u{1f9ed}.rs"),
            "fn alternate() {}\n",
        )
        .unwrap();
        let log = temp.path().join("log");
        let node = std::process::Command::new("node")
            .args(["-p", "process.execPath"])
            .output()
            .unwrap();
        assert!(node.status.success());
        let server = super::super::ConfiguredServer {
            server: super::super::Server {
                command: String::from_utf8(node.stdout).unwrap().trim().into(),
                args: vec![
                    Path::new(env!("CARGO_MANIFEST_DIR"))
                        .join("tests/fixtures/resource-settings-lsp.mjs")
                        .display()
                        .to_string(),
                    log.display().to_string(),
                ],
                language: "rust".into(),
                trust: true,
            },
            configuration: serde_json::from_value(json!({
                "passiveProfile":"contract_fixture","settings":{"base":"original"},
                "resourceSettings":{"main.rs":{"tag":"resource-action"}}
            }))
            .unwrap(),
        };
        let mut engine = crate::Engine::open(&root).unwrap();
        let deadline = Deadline::new(30000);
        let (snapshot, graph, _, _) = engine.index(&deadline).unwrap();
        let staged = super::super::stage_at(
            &mut engine,
            (&server.server, Some(&server.configuration)),
            &snapshot,
            &graph,
            super::super::SemanticRequest {
                path: "main.rs",
                offset: 0,
                operation: "codeActions",
                new_name: None,
            },
            &json!({}),
            &deadline,
        )
        .unwrap();
        assert_eq!(staged.value["items"][0]["title"], "resource-action");
        assert_eq!(staged.edits.len(), 1);
        assert_eq!(staged.value["sourceExecutionAuthorized"], false);
        assert_eq!(
            std::fs::read(root.join("main.rs")).unwrap(),
            source.as_bytes()
        );
    }
}
