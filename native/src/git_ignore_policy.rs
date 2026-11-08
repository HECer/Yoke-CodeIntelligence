//! Explicit analysis inclusion never confers write or execution authority.
use crate::{project_session::HostProfile, store::Store};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs,
    io::{Read, Seek, SeekFrom},
    path::Path,
};

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields, default)]
pub struct ProjectPolicy {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub local: Option<bool>,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields, default)]
pub struct HostPolicy {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub global: Option<bool>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub projects: BTreeMap<String, ProjectPolicy>,
}
impl HostPolicy {
    pub fn select(&self, name: &str) -> Option<bool> {
        self.projects
            .get(name)
            .and_then(|p| p.local.or(p.project))
            .or(self.global)
    }
    pub(crate) fn empty(&self) -> bool {
        self.global.is_none()
            && self
                .projects
                .values()
                .all(|p| p.project.is_none() && p.local.is_none())
    }
    pub(crate) fn canonicalize(&mut self) {
        self.projects
            .retain(|_, p| p.project.is_some() || p.local.is_some());
    }
    pub(crate) fn validate(&self, host: &HostProfile) -> Result<()> {
        ensure!(self.projects.len() <= 101, "git_ignore_policy_capacity");
        for name in self.projects.keys() {
            ensure!(
                name == "default" || host.projects.iter().any(|p| p.name == *name),
                "git_ignore_policy_project_not_registered"
            );
        }
        Ok(())
    }
}
pub(crate) fn unselected(policy: &Option<HostPolicy>) -> bool {
    policy.as_ref().is_none_or(HostPolicy::empty)
}
pub(crate) fn disabled(value: &bool) -> bool {
    !*value
}

/// Current host restriction, checked even for a plan prepared under an older
/// capture. The ordinary ignore matcher also handles nonexistent future paths.
pub(crate) fn check_current_write(store: &Store, path: &str) -> Result<()> {
    if !store.current_git_ignored_read_only {
        return Ok(());
    }
    store.check_deadline()?;
    crate::paths::normalized_syntax(path)?;
    let root = store.dir.parent().context("Workspace root missing")?;
    let mut directories: Vec<_> = root.ancestors().map(Path::to_path_buf).collect();
    directories.reverse();
    let mut current = root.to_path_buf();
    let parts: Vec<_> = Path::new(path).components().collect();
    for part in parts.iter().take(parts.len().saturating_sub(1)) {
        current.push(part.as_os_str());
        directories.push(current.clone());
    }
    // Own the paths because prospective nested directories may be absent.
    ensure!(
        directories.len() <= 1000,
        "git_ignore_write_policy_directory_capacity"
    );
    let mut policy_bytes = 0usize;
    let mut pinned = vec![];
    for directory in directories {
        store.check_deadline()?;
        let git = directory.join(".git");
        if let Ok(meta) = fs::symlink_metadata(&git) {
            ensure!(
                meta.is_dir() && !crate::paths::is_link(&meta),
                "git_ignore_write_policy_external_git_refused"
            );
            if let Ok(meta) = fs::symlink_metadata(git.join("info")) {
                ensure!(
                    meta.is_dir() && !crate::paths::is_link(&meta),
                    "git_ignore_write_policy_link_refused"
                );
            }
        }
        for file in [
            directory.join(".gitignore"),
            directory.join(".ignore"),
            git.join("info/exclude"),
        ] {
            store.check_deadline()?;
            match fs::symlink_metadata(&file) {
                Ok(meta) => {
                    ensure!(
                        meta.is_file() && !crate::paths::is_link(&meta) && meta.len() <= 1_000_000,
                        "git_ignore_write_policy_type_or_capacity"
                    );
                    let mut options = fs::OpenOptions::new();
                    options.read(true);
                    #[cfg(windows)]
                    {
                        use std::os::windows::fs::OpenOptionsExt;
                        options.share_mode(1);
                    }
                    let mut handle = options.open(&file)?;
                    let mut bytes = vec![];
                    (&mut handle).take(1_000_001).read_to_end(&mut bytes)?;
                    ensure!(
                        bytes.len() <= 1_000_000 && !crate::paths::is_link(&handle.metadata()?),
                        "git_ignore_write_policy_grew_or_linked"
                    );
                    let text = std::str::from_utf8(&bytes)?;
                    ensure!(
                        text.lines().count() <= 10000
                            && text.lines().all(|line| line.len() <= 4096),
                        "git_ignore_write_policy_rule_capacity"
                    );
                    policy_bytes += bytes.len();
                    ensure!(
                        policy_bytes <= 8_000_000,
                        "git_ignore_write_policy_byte_capacity"
                    );
                    pinned.push((handle, bytes));
                }
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => return Err(e.into()),
            }
        }
    }
    let mut matchers = ignore::WalkBuilder::new(root)
        .hidden(false)
        .follow_links(false)
        .git_ignore(true)
        .git_global(false)
        .git_exclude(true)
        .require_git(false)
        .build_matchers();
    ensure!(matchers.len() == 1, "git_ignore_write_policy_unavailable");
    let (matched, error) = matchers[0].matched_with_errors(path, false);
    if let Some(error) = error {
        return Err(error.into());
    }
    for (mut handle, bytes) in pinned {
        store.check_deadline()?;
        handle.seek(SeekFrom::Start(0))?;
        let mut current = vec![];
        (&mut handle).take(1_000_001).read_to_end(&mut current)?;
        ensure!(current == bytes, "git_ignore_write_policy_changed");
    }
    store.check_deadline()?;
    ensure!(
        !matched.is_ignore(),
        "git_ignored_analysis_source_is_read_only"
    );
    Ok(())
}
