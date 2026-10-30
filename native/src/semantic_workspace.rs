//! Additional semantic context from the same immutable capture, never an index grant.
use crate::{
    Engine, capture::Deadline, lsp::HostConfiguration, model::Snapshot,
    project_session::HostProfile,
};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields, default)]
pub struct HostFolders {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub global: Option<Vec<String>>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub projects: BTreeMap<String, crate::symbol_workspace::ProjectFolders>,
}
pub(crate) fn folders(values: &[String]) -> Result<Vec<String>> {
    if values.is_empty() {
        return Ok(Vec::new());
    }
    crate::symbol_workspace::folders(values)
        .map_err(|_| anyhow::anyhow!("session_config_additional_workspace_folders_unavailable"))
}
impl HostFolders {
    fn lists(&self) -> impl Iterator<Item = &Vec<String>> {
        self.global.iter().chain(
            self.projects
                .values()
                .flat_map(|p| p.project.iter().chain(p.local.iter())),
        )
    }
    pub(crate) fn empty(&self) -> bool {
        self.lists().all(Vec::is_empty)
    }
    pub fn select(&self, project: &str) -> Option<&[String]> {
        self.projects
            .get(project)
            .and_then(|p| p.local.as_deref().or(p.project.as_deref()))
            .or(self.global.as_deref())
    }
    pub(crate) fn validate(&self, host: &HostProfile) -> Result<()> {
        ensure!(
            self.projects.len() <= 101,
            "additional_workspace_project_capacity"
        );
        for name in self.projects.keys() {
            ensure!(
                name == "default" || host.projects.iter().any(|p| p.name == *name),
                "additional_workspace_project_not_host_registered"
            );
        }
        for values in self.lists() {
            folders(values)?;
        }
        ensure!(
            serde_json::to_vec(self)?.len() <= 128000,
            "additional_workspace_policy_capacity"
        );
        Ok(())
    }
    pub(crate) fn canonicalize(&mut self) -> Result<()> {
        for values in self.global.iter_mut().chain(
            self.projects
                .values_mut()
                .flat_map(|p| p.project.iter_mut().chain(p.local.iter_mut())),
        ) {
            *values = folders(values)?;
        }
        self.projects
            .retain(|_, p| p.project.is_some() || p.local.is_some());
        if self.empty() {
            *self = Self::default();
        } else if self.global.as_ref().is_some_and(Vec::is_empty) {
            self.global = None;
        }
        Ok(())
    }
}
pub(crate) fn unselected(value: &Option<HostFolders>) -> bool {
    value.as_ref().is_none_or(HostFolders::empty)
}

/// Called by the common transport path before admission, hashing or startup.
pub(crate) fn extend(
    engine: &Engine,
    snapshot: &Snapshot,
    configuration: &mut Option<HostConfiguration>,
    deadline: &Deadline,
) -> Result<()> {
    let Some(extra) = engine
        .additional_workspace_folders
        .as_deref()
        .filter(|v| !v.is_empty())
    else {
        return Ok(());
    };
    deadline.check()?;
    engine.store.set_deadline(deadline)?;
    let scope = crate::capture::source_scope_policy(&engine.store, snapshot)?;
    crate::symbol_workspace::validate_snapshot(extra, snapshot, &scope, deadline)?;
    let configuration = configuration.get_or_insert_default();
    let primary = if configuration.workspace_folders.is_empty() {
        vec![".".to_owned()]
    } else {
        configuration.workspace_folders.clone()
    };
    crate::symbol_workspace::validate_snapshot(&primary, snapshot, &scope, deadline)?;
    let mut roots = Vec::new();
    let mut seen = BTreeSet::new();
    for value in primary.iter().chain(extra.iter()) {
        deadline.check()?;
        let key = crate::symbol_workspace::folder_identity(value);
        if seen.insert(key) {
            roots.push(value.clone());
        }
    }
    ensure!(roots.len() <= 32, "additional_workspace_combined_capacity");
    configuration.workspace_folders = roots;
    Ok(())
}
