//! Explicit host-authorized settings for already registered catalog backends.
use crate::{lsp, project_session::HostProfile};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Component, Path},
};

pub type Entries = BTreeMap<String, Option<Settings>>;
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct Settings {
    #[serde(
        rename = "initializationOptions",
        skip_serializing_if = "Option::is_none"
    )]
    pub initialization_options: Option<Map<String, Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub settings: Option<Map<String, Value>>,
    #[serde(rename = "resourceSettings", skip_serializing_if = "Option::is_none")]
    pub resource_settings: Option<BTreeMap<String, Map<String, Value>>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ls_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ls_base_cmd: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ls_args: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ls_extra_args: Option<Vec<String>>,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields, default)]
pub struct ProjectSettings {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project: Option<Entries>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub local: Option<Entries>,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields, default)]
pub struct HostSettings {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub global: Option<Entries>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub projects: BTreeMap<String, ProjectSettings>,
}
fn strings(values: &[String]) -> Result<()> {
    ensure!(
        values.len() <= 128 && values.iter().map(String::len).sum::<usize>() <= 64000,
        "server_settings_argument_capacity"
    );
    ensure!(
        values.iter().all(|v| v.len() <= 4096 && !v.contains('\0')),
        "server_settings_argument_invalid"
    );
    Ok(())
}
fn command(value: &str) -> Result<()> {
    ensure!(
        !value.is_empty()
            && value.len() <= 4096
            && !value.contains('\0')
            && Path::new(value).is_absolute(),
        "server_settings_absolute_command_required"
    );
    ensure!(
        !Path::new(value)
            .components()
            .any(|v| matches!(v, Component::ParentDir | Component::CurDir)),
        "server_settings_command_alias_refused"
    );
    if cfg!(windows) {
        ensure!(
            !value.starts_with("\\\\")
                && !value.contains(['\r', '\n'])
                && !value.get(2..).is_some_and(|v| v.contains(':')),
            "server_settings_command_alias_refused"
        );
    }
    Ok(())
}
impl Settings {
    pub(crate) fn validate(&self) -> Result<()> {
        ensure!(
            self.ls_path.is_none() || self.ls_base_cmd.is_none(),
            "server_settings_command_forms_conflict"
        );
        if let Some(path) = &self.ls_path {
            command(path)?;
        }
        if let Some(base) = &self.ls_base_cmd {
            strings(base)?;
            command(
                base.first()
                    .ok_or_else(|| anyhow::anyhow!("server_settings_empty_base_command"))?,
            )?;
        }
        for values in [&self.ls_args, &self.ls_extra_args].into_iter().flatten() {
            strings(values)?;
        }
        for (options, structure_error, capacity_error) in [
            (
                &self.initialization_options,
                "server_settings_initialization_structure_capacity",
                "server_settings_initialization_capacity",
            ),
            (
                &self.settings,
                "server_settings_workspace_structure_capacity",
                "server_settings_workspace_capacity",
            ),
        ] {
            let Some(options) = options else { continue };
            let mut pending: Vec<_> = options.values().map(|v| (v, 1usize)).collect();
            let mut nodes = options.len();
            while let Some((value, depth)) = pending.pop() {
                ensure!(depth <= 32 && nodes <= 4096, "{structure_error}");
                match value {
                    Value::Object(values) => {
                        nodes += values.len();
                        pending.extend(values.values().map(|v| (v, depth + 1)));
                    }
                    Value::Array(values) => {
                        nodes += values.len();
                        pending.extend(values.iter().map(|v| (v, depth + 1)));
                    }
                    _ => {}
                }
            }
            ensure!(
                serde_json::to_vec(options)?.len() <= 64000,
                "{capacity_error}"
            );
        }
        if let Some(resources) = &self.resource_settings {
            lsp::validate_resource_entries(resources)?;
        }
        Ok(())
    }
    fn apply(&self, source: &lsp::ConfiguredServer) -> Result<lsp::ConfiguredServer> {
        self.validate()?;
        ensure!(
            source.server.trust,
            "server_settings_host_runtime_trust_required"
        );
        ensure!(
            source.configuration.tcp_endpoint.is_none()
                || (self.ls_path.is_none()
                    && self.ls_base_cmd.is_none()
                    && self.ls_args.is_none()
                    && self.ls_extra_args.is_none()),
            "server_settings_tcp_launch_options_refused"
        );
        let mut result = source.clone();
        let mut prefix = Vec::new();
        if let Some(base) = &self.ls_base_cmd {
            result.server.command = base[0].clone();
            prefix.extend_from_slice(&base[1..]);
        } else if let Some(path) = &self.ls_path {
            result.server.command = path.clone();
        }
        prefix.extend_from_slice(self.ls_args.as_ref().unwrap_or(&source.server.args));
        if let Some(extra) = &self.ls_extra_args {
            prefix.extend_from_slice(extra);
        }
        strings(&prefix)?;
        result.server.args = prefix;
        ensure!(
            (result.server.command == source.server.command
                && result.server.args == source.server.args)
                || source.configuration.allow_source_execution,
            "server_settings_launch_override_requires_host_source_execution"
        );
        if let Some(options) = &self.initialization_options {
            ensure!(
                result.configuration.initialization_options.is_null()
                    || result.configuration.initialization_options.is_object(),
                "server_settings_original_initialization_object_required"
            );
            let mut merged = result
                .configuration
                .initialization_options
                .as_object()
                .cloned()
                .unwrap_or_default();
            merged.extend(options.clone());
            ensure!(
                serde_json::to_vec(&merged)?.len() <= 64000,
                "server_settings_initialization_capacity"
            );
            result.configuration.initialization_options = Value::Object(merged);
        }
        if let Some(settings) = &self.settings {
            ensure!(
                result.configuration.settings.is_null()
                    || result.configuration.settings.is_object(),
                "server_settings_original_workspace_object_required"
            );
            let mut merged = result
                .configuration
                .settings
                .as_object()
                .cloned()
                .unwrap_or_default();
            merged.extend(settings.clone());
            // Validate the combined object too: two individually bounded maps
            // must not bypass the effective workspace configuration limits.
            Self {
                settings: Some(merged.clone()),
                ..Default::default()
            }
            .validate()?;
            result.configuration.settings = Value::Object(merged);
        }
        if let Some(resources) = &self.resource_settings {
            let merged = result
                .configuration
                .resource_settings
                .get_or_insert_default();
            for (path, settings) in resources {
                merged
                    .entry(path.clone())
                    .or_default()
                    .extend(settings.clone());
            }
        }
        lsp::effective_resource_settings(&result.configuration)?;
        // Runtime/source-execution/passive authority is copied unchanged. The
        // common LSP admission still checks the effective settings before spawn.
        Ok(result)
    }
}
pub(crate) fn validate_entries(entries: &Entries) -> Result<()> {
    ensure!(entries.len() <= 100, "server_settings_entry_capacity");
    for (id, settings) in entries {
        ensure!(
            crate::server_catalog::get(id).is_some(),
            "server_settings_unknown_backend"
        );
        if let Some(settings) = settings {
            settings.validate()?;
        }
    }
    ensure!(
        serde_json::to_vec(entries)?.len() <= 128000,
        "server_settings_entry_capacity"
    );
    Ok(())
}
impl HostSettings {
    pub fn resolve(&self, project: &str) -> BTreeMap<String, Settings> {
        let mut result = BTreeMap::new();
        let selected = self.projects.get(project);
        for entries in [
            self.global.as_ref(),
            selected.and_then(|v| v.project.as_ref()),
            selected.and_then(|v| v.local.as_ref()),
        ]
        .into_iter()
        .flatten()
        {
            for (id, settings) in entries {
                if let Some(settings) = settings {
                    result.insert(id.clone(), settings.clone());
                }
            }
        }
        result
    }
    pub(crate) fn canonicalize(&mut self) {
        for entries in self.global.iter_mut().chain(
            self.projects
                .values_mut()
                .flat_map(|v| v.project.iter_mut().chain(v.local.iter_mut())),
        ) {
            entries.retain(|_, v| v.is_some());
        }
        if self.global.as_ref().is_some_and(BTreeMap::is_empty) {
            self.global = None;
        }
        for p in self.projects.values_mut() {
            if p.project.as_ref().is_some_and(BTreeMap::is_empty) {
                p.project = None;
            }
            if p.local.as_ref().is_some_and(BTreeMap::is_empty) {
                p.local = None;
            }
        }
        self.projects
            .retain(|_, p| p.project.is_some() || p.local.is_some());
    }
    pub(crate) fn empty(&self) -> bool {
        self.global
            .as_ref()
            .is_none_or(|v| v.values().all(Option::is_none))
            && self.projects.values().all(|p| {
                [&p.project, &p.local]
                    .into_iter()
                    .all(|v| v.as_ref().is_none_or(|v| v.values().all(Option::is_none)))
            })
    }
    pub(crate) fn validate(&self, host: &HostProfile) -> Result<()> {
        ensure!(
            self.projects.len() <= 101,
            "server_settings_policy_capacity"
        );
        for name in self.projects.keys() {
            ensure!(
                name == "default" || host.projects.iter().any(|p| p.name == *name),
                "server_settings_project_not_host_registered"
            );
        }
        for entries in self.global.iter().chain(
            self.projects
                .values()
                .flat_map(|v| v.project.iter().chain(v.local.iter())),
        ) {
            validate_entries(entries)?;
        }
        ensure!(
            serde_json::to_vec(self)?.len() <= 128000,
            "server_settings_policy_capacity"
        );
        let mut seen = BTreeSet::new();
        ensure!(
            host.trusted_server_settings_projects.len() <= 101,
            "server_settings_trust_capacity"
        );
        for name in &host.trusted_server_settings_projects {
            ensure!(
                seen.insert(name.as_str())
                    && (name == "default" || host.projects.iter().any(|p| p.name == *name)),
                "server_settings_trust_project_invalid"
            );
        }
        for name in std::iter::once("default").chain(host.projects.iter().map(|p| p.name.as_str()))
        {
            if !self.resolve(name).is_empty() {
                ensure!(
                    seen.contains(name),
                    "server_settings_project_host_trust_required"
                );
                ensure!(
                    host.language_servers
                        .as_ref()
                        .and_then(|p| p.select(name))
                        .is_some(),
                    "server_settings_explicit_backend_selection_required"
                );
            }
        }
        Ok(())
    }
    pub(crate) fn apply(
        &self,
        project: &str,
        registry: &BTreeMap<String, lsp::ConfiguredServer>,
    ) -> Result<BTreeMap<String, lsp::ConfiguredServer>> {
        let mut result = registry.clone();
        for (id, settings) in self.resolve(project) {
            let original = registry
                .get(&id)
                .ok_or_else(|| anyhow::anyhow!("server_settings_backend_not_host_registered"))?;
            result.insert(id, settings.apply(original)?);
        }
        crate::server_selection::bindings(&result)?;
        Ok(result)
    }
}
pub(crate) fn unselected(value: &Option<HostSettings>) -> bool {
    value.as_ref().is_none_or(HostSettings::empty)
}
