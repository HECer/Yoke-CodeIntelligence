//! Ordered selection from current host registrations. Catalog names grant no execution.
use crate::{lsp, project_session::HostProfile, server_catalog};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
};

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields, default)]
pub struct ProjectSelection {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub local: Option<Vec<String>>,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields, default)]
pub struct HostSelection {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub global: Option<Vec<String>>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub projects: BTreeMap<String, ProjectSelection>,
}
pub(crate) fn validate_ids(values: &[String]) -> Result<()> {
    ensure!(
        !values.is_empty() && values.len() <= 32,
        "language_server_selection_capacity"
    );
    let mut seen = BTreeSet::new();
    for id in values {
        ensure!(
            server_catalog::get(id).is_some(),
            "language_server_selection_unknown_backend"
        );
        ensure!(
            seen.insert(id),
            "language_server_selection_duplicate_backend"
        );
    }
    Ok(())
}
impl HostSelection {
    pub(crate) fn empty(&self) -> bool {
        self.global.is_none()
            && self
                .projects
                .values()
                .all(|v| v.project.is_none() && v.local.is_none())
    }
    pub(crate) fn canonicalize(&mut self) {
        self.projects
            .retain(|_, v| v.project.is_some() || v.local.is_some());
    }
    pub fn select(&self, project: &str) -> Option<&[String]> {
        self.projects
            .get(project)
            .and_then(|p| p.local.as_deref().or(p.project.as_deref()))
            .or(self.global.as_deref())
    }
    fn lists(&self) -> impl Iterator<Item = &Vec<String>> {
        self.global.iter().chain(
            self.projects
                .values()
                .flat_map(|p| p.project.iter().chain(p.local.iter())),
        )
    }
    pub(crate) fn validate(&self, host: &HostProfile) -> Result<()> {
        ensure!(
            self.projects.len() <= 101 && serde_json::to_vec(self)?.len() <= 128000,
            "language_server_selection_policy_capacity"
        );
        for name in self.projects.keys() {
            ensure!(
                name == "default" || host.projects.iter().any(|p| p.name == *name),
                "language_server_selection_project_not_host_registered"
            );
        }
        for ids in self.lists() {
            validate_ids(ids)?;
        }
        Ok(())
    }
    pub(crate) fn validate_registered(
        &self,
        registry: &BTreeMap<String, lsp::ConfiguredServer>,
    ) -> Result<()> {
        for ids in self.lists() {
            for id in ids {
                ensure!(
                    registry.contains_key(id),
                    "language_server_not_host_registered"
                );
            }
        }
        Ok(())
    }
}
pub(crate) fn unselected(value: &Option<HostSelection>) -> bool {
    value.as_ref().is_none_or(HostSelection::empty)
}
pub(crate) fn choose<'a>(ids: &'a [String], path: &str) -> Result<&'a str> {
    validate_ids(ids)?;
    Ok(ids
        .iter()
        .find(|id| {
            server_catalog::get(id).is_some_and(|entry| entry.supports_path(Path::new(path)))
        })
        .unwrap_or(&ids[0]))
}

/// Closed host-only forms accepted by CLI and principal configuration. Neither
/// form can be constructed by project YAML or an MCP request.
#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LegacySpec {
    pub language: String,
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub configuration: lsp::HostConfiguration,
}
#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BackendSpec {
    pub backend_id: String,
    #[serde(default)]
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub configuration: lsp::HostConfiguration,
}
#[derive(Clone, Deserialize)]
#[serde(untagged)]
pub enum HostServerSpec {
    Legacy(LegacySpec),
    Backend(BackendSpec),
}
impl HostServerSpec {
    pub fn register(self, engine: &mut crate::Engine, trust: bool) -> Result<()> {
        ensure!(
            trust,
            "Host language server configuration requires explicit trust"
        );
        match self {
            Self::Legacy(s) => engine.register_configured_server(lsp::ConfiguredServer {
                server: lsp::Server {
                    language: s.language,
                    command: s.command,
                    args: s.args,
                    trust: true,
                },
                configuration: s.configuration,
            }),
            Self::Backend(s) => {
                let language = server_catalog::get(&s.backend_id)
                    .context("Unknown language-server backend")?
                    .languages[0]
                    .to_owned();
                engine.register_backend(
                    &s.backend_id,
                    lsp::ConfiguredServer {
                        server: lsp::Server {
                            language,
                            command: s.command,
                            args: s.args,
                            trust: true,
                        },
                        configuration: s.configuration,
                    },
                )
            }
        }
    }
}
/// Validate an explicitly trusted backend registration without starting a process.
pub(crate) fn checked(id: &str, server: lsp::ConfiguredServer) -> Result<lsp::ConfiguredServer> {
    let registration = server_catalog::HostRegistration {
        backend_id: id.into(),
        command: PathBuf::from(&server.server.command),
        tcp_endpoint: server.configuration.tcp_endpoint,
        args: server.server.args,
        trust: server.server.trust,
        allow_source_execution: server.configuration.allow_source_execution,
        passive_profile: server.configuration.passive_profile,
        initialization_options: server.configuration.initialization_options,
        settings: server.configuration.settings,
        resource_settings: server.configuration.resource_settings,
        workspace_folders: server.configuration.workspace_folders,
        language_ids: server.configuration.language_ids,
        open_languages: server.configuration.open_languages,
    };
    let mut configured = registration.configured()?;
    ensure!(
        server.server.language == configured.server.language,
        "language_server_backend_language_mismatch"
    );
    configured.configuration.refactor_intent_profile = server.configuration.refactor_intent_profile;
    Ok(configured)
}
pub(crate) fn bindings(registry: &BTreeMap<String, lsp::ConfiguredServer>) -> Result<Value> {
    ensure!(
        registry.len() <= 100,
        "language_server_registration_capacity"
    );
    let value=Value::Array(registry.iter().map(|(id,p)|json!({"id":id,"language":p.server.language,"command":p.server.command,"args":p.server.args,"trust":p.server.trust,"configuration":p.configuration})).collect());
    ensure!(
        serde_json::to_vec(&value)?.len() <= 1_000_000,
        "language_server_registration_byte_capacity"
    );
    Ok(value)
}
