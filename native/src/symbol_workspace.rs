//! A host-selected symbol index projection, separate from captured source authority.
use crate::{
    capture::Deadline,
    model::{Graph, Snapshot, identity},
    project_session::HostProfile,
};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields, default)]
pub struct ProjectFolders {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub local: Option<Vec<String>>,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields, default)]
pub struct HostFolders {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub global: Option<Vec<String>>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub projects: BTreeMap<String, ProjectFolders>,
}
pub(crate) fn folder_identity(value: &str) -> String {
    let mut key = value.to_owned();
    if cfg!(windows) {
        // Unicode lowercasing can expand a distinct filesystem name (U+0130
        // becomes ASCII i plus U+0307). Keep opaque Unicode components exact.
        key.make_ascii_lowercase();
    }
    key
}
pub fn folders(values: &[String]) -> Result<Vec<String>> {
    ensure!(
        !values.is_empty() && values.len() <= 32,
        "symbol_workspace_folder_capacity"
    );
    ensure!(
        values.iter().map(String::len).sum::<usize>() <= 65536,
        "symbol_workspace_path_byte_capacity"
    );
    let mut seen = BTreeSet::new();
    values
        .iter()
        .map(|value| {
            let value = value.strip_prefix("./").unwrap_or(value);
            let value = if value == "." {
                value.to_owned()
            } else {
                crate::paths::normalized(value)?
            };
            let key = folder_identity(&value);
            ensure!(seen.insert(key), "symbol_workspace_duplicate_folder");
            Ok(value)
        })
        .collect()
}
impl HostFolders {
    pub(crate) fn empty(&self) -> bool {
        self.global.is_none()
            && self
                .projects
                .values()
                .all(|p| p.project.is_none() && p.local.is_none())
    }
    pub fn select(&self, project: &str) -> Option<&[String]> {
        self.projects
            .get(project)
            .and_then(|p| p.local.as_deref().or(p.project.as_deref()))
            .or(self.global.as_deref())
    }
    pub(crate) fn validate(&self, host: &HostProfile) -> Result<()> {
        ensure!(
            serde_json::to_vec(self)?.len() <= 128000,
            "symbol_workspace_policy_byte_capacity"
        );
        ensure!(
            self.projects.len() <= 101,
            "symbol_workspace_project_capacity"
        );
        for (name, p) in &self.projects {
            ensure!(
                name == "default" || host.projects.iter().any(|p| p.name == *name),
                "symbol_workspace_project_not_host_registered"
            );
            for values in [&p.project, &p.local].into_iter().flatten() {
                folders(values)?;
            }
        }
        if let Some(values) = &self.global {
            folders(values)?;
        }
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
        Ok(())
    }
}
pub(crate) fn unselected(value: &Option<HostFolders>) -> bool {
    value.as_ref().is_none_or(HostFolders::empty)
}
pub(crate) fn contains(roots: &[String], path: &str) -> bool {
    roots.iter().any(|root| {
        root == "."
            || path
                .strip_prefix(root)
                .is_some_and(|suffix| suffix.starts_with('/'))
    })
}
/// Existence is proved from the frozen source manifest, never the live filesystem.
pub(crate) fn validate_snapshot(
    roots: &[String],
    snapshot: &Snapshot,
    scope: &crate::capture::CapturedSourceScope<'_>,
    deadline: &Deadline,
) -> Result<()> {
    ensure!(
        folders(roots)? == roots,
        "symbol_workspace_folders_not_canonical"
    );
    for root in roots {
        deadline.check()?;
        let mut captured = root == ".";
        for file in &snapshot.files {
            deadline.check()?;
            if contains(std::slice::from_ref(root), &file.path)
                && (scope.index_uses_git_ignored() || !scope.ignored_at_capture(&file.path)?)
            {
                captured = true;
                break;
            }
        }
        ensure!(captured, "symbol_workspace_folder_not_captured");
    }
    Ok(())
}
pub(crate) fn qualify(
    value: &mut Value,
    roots: &[String],
    snapshot: &Snapshot,
    graph: &Graph,
    scope: &crate::capture::CapturedSourceScope<'_>,
    deadline: &Deadline,
) -> Result<()> {
    let mut sources = 0usize;
    for file in &snapshot.files {
        deadline.check()?;
        if contains(roots, &file.path)
            && (scope.index_uses_git_ignored() || !scope.ignored_at_capture(&file.path)?)
        {
            sources += 1;
        }
    }
    let graph_hash = identity(graph);
    value["symbolIndexScope"] = json!({"schema":"native_symbol_workspace_index_v1", "id":identity(&("native_symbol_workspace_index_v1", &snapshot.id, &graph_hash, roots)),
        "folders":roots,"snapshotId":snapshot.id,"baseGraphHash":graph_hash,"scope":"selected_structural_symbol_projection",
        "sourcesInScope":sources,"baseSources":snapshot.files.len(),
        "sourceCaptureNarrowed":false,"baseGraphMutated":false,"complete":false});
    deadline.check()
}
