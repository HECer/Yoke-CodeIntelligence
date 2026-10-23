//! Immutable exact host selections. Stored validation performs no live Git/IO.
use super::Deadline;
use crate::{git_workflows::repository_scope, paths};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeSet, path::Path};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Binding {
    pub observed_path: String,
    pub captured_path: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RepositoryScope {
    pub policy: String,
    pub observation: serde_json::Value,
    pub file_bindings: Vec<Binding>,
    pub directory_bindings: Vec<Binding>,
    pub files: Vec<String>,
    pub read_only_roots: Vec<String>,
}
fn prefix(roots: &[String], path: &str) -> bool {
    roots
        .iter()
        .any(|root| path == root || path.strip_prefix(root).is_some_and(|s| s.starts_with('/')))
}
impl RepositoryScope {
    pub(super) fn omissions(&self) -> Result<Vec<crate::model::Omission>> {
        let data = &self.observation["observation"];
        let mut omissions = Vec::new();
        for row in data["omissions"]
            .as_array()
            .context("repository_capture_scope_invalid")?
        {
            omissions.push(crate::model::Omission {
                path: row["path"]
                    .as_str()
                    .context("repository_capture_scope_invalid")?
                    .into(),
                reason: format!(
                    "git-index-{}",
                    row["reason"]
                        .as_str()
                        .context("repository_capture_scope_invalid")?
                ),
            });
        }
        for row in data["boundaries"]
            .as_array()
            .context("repository_capture_scope_invalid")?
        {
            let state = row["state"]
                .as_str()
                .context("repository_capture_scope_invalid")?;
            if state != "followed" {
                omissions.push(crate::model::Omission {
                    path: row["path"]
                        .as_str()
                        .context("repository_capture_scope_invalid")?
                        .into(),
                    reason: format!(
                        "repository-{}-{state}",
                        row["kind"]
                            .as_str()
                            .context("repository_capture_scope_invalid")?
                    ),
                });
            }
        }
        Ok(omissions)
    }
    pub fn validated(&self) -> Result<()> {
        self.validated_with(&|| Ok(()))
    }
    fn validated_with(&self, check: &impl Fn() -> Result<()>) -> Result<()> {
        check()?;
        ensure!(
            self.policy == "exact-host-git-working-files-v1"
                && self.file_bindings.len() <= 20_000
                && self.directory_bindings.len() <= 64
                && self.files.len() <= 20_000
                && self.read_only_roots.len() <= 64,
            "repository_capture_scope_invalid"
        );
        ensure!(
            serde_json::to_vec(self)?.len() <= 8_000_000,
            "repository_capture_capacity_exceeded"
        );
        // No independent deadline is invented for a stored source proof. The
        // enclosing Store/reader retains its own request deadline checks.
        let (files, roots, _) = repository_scope::capture_parts(&self.observation, check)?;
        let validate =
            |bindings: &[Binding], expected: &[String], selected: &[String]| -> Result<()> {
                ensure!(
                    bindings
                        .iter()
                        .map(|b| b.observed_path.as_str())
                        .eq(expected.iter().map(String::as_str)),
                    "repository_capture_binding_invalid"
                );
                let mut actual = BTreeSet::new();
                for binding in bindings {
                    check()?;
                    paths::normalized(&binding.observed_path)?;
                    paths::normalized(&binding.captured_path)?;
                    actual.insert(binding.captured_path.clone());
                }
                ensure!(
                    actual.into_iter().eq(selected.iter().cloned()),
                    "repository_capture_binding_invalid"
                );
                Ok(())
            };
        validate(&self.file_bindings, &files, &self.files)?;
        validate(&self.directory_bindings, &roots, &self.read_only_roots)?;
        let owners = self.observation["observation"]["files"]
            .as_array()
            .context("repository_capture_binding_invalid")?
            .iter()
            .map(|row| -> Result<(&str, &str)> {
                Ok((
                    row["path"]
                        .as_str()
                        .context("repository_capture_binding_invalid")?,
                    row["ownerRepository"]
                        .as_str()
                        .context("repository_capture_binding_invalid")?,
                ))
            })
            .collect::<Result<std::collections::BTreeMap<_, _>>>()?;
        for binding in &self.file_bindings {
            check()?;
            let owner = *owners
                .get(binding.observed_path.as_str())
                .context("repository_capture_binding_invalid")?;
            if owner != "." {
                let root = self
                    .directory_bindings
                    .iter()
                    .find(|b| b.observed_path == owner)
                    .context("repository_capture_binding_invalid")?;
                ensure!(
                    binding
                        .captured_path
                        .strip_prefix(&root.captured_path)
                        .is_some_and(|tail| tail.starts_with('/')),
                    "repository_capture_binding_invalid"
                );
            }
        }
        Ok(())
    }
    pub(super) fn includes(&self, path: &str) -> bool {
        self.files
            .binary_search_by(|p| p.as_str().cmp(path))
            .is_ok()
    }
    pub(super) fn read_only(&self, path: &str) -> bool {
        prefix(&self.read_only_roots, path)
    }
    pub(crate) fn physical(
        root: &Path,
        observation: &repository_scope::Observation,
        d: &Deadline,
    ) -> Result<Self> {
        let (files, roots, observed_root) =
            repository_scope::capture_parts(observation.value(), &|| d.check())?;
        let root = root.canonicalize()?;
        ensure!(
            Path::new(&observed_root) == root,
            "repository_capture_workspace_changed"
        );
        let bindings = |selected: Vec<String>, directory: bool| -> Result<Vec<Binding>> {
            selected
                .into_iter()
                .map(|observed_path| {
                    d.check()?;
                    let path = paths::scoped(&root, &observed_path)?;
                    let metadata = std::fs::symlink_metadata(&path)?;
                    ensure!(
                        !paths::is_link(&metadata)
                            && if directory {
                                metadata.is_dir()
                            } else {
                                metadata.is_file()
                            },
                        "repository_capture_member_changed"
                    );
                    let actual = path.canonicalize()?;
                    let captured_path = actual
                        .strip_prefix(&root)
                        .context("repository_capture_member_escaped")?
                        .to_str()
                        .context("repository_capture_path_invalid")?
                        .replace('\\', "/");
                    paths::normalized(&captured_path)?;
                    Ok(Binding {
                        observed_path,
                        captured_path,
                    })
                })
                .collect()
        };
        let file_bindings = bindings(files, false)?;
        let directory_bindings = bindings(roots, true)?;
        let unique = |bindings: &[Binding]| {
            bindings
                .iter()
                .map(|b| b.captured_path.clone())
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect::<Vec<_>>()
        };
        let selected = Self {
            policy: "exact-host-git-working-files-v1".into(),
            observation: observation.value().clone(),
            files: unique(&file_bindings),
            read_only_roots: unique(&directory_bindings),
            file_bindings,
            directory_bindings,
        };
        selected.validated_with(&|| d.check())?;
        d.check()?;
        Ok(selected)
    }
}

pub(crate) struct WritePolicy {
    files: Vec<String>,
    read_only_roots: Vec<String>,
}
impl WritePolicy {
    pub(crate) fn from_scope(scope: &RepositoryScope) -> Result<Self> {
        scope.validated()?;
        Ok(Self {
            files: scope.files.clone(),
            read_only_roots: scope.read_only_roots.clone(),
        })
    }
    fn permits(&self, path: &str) -> bool {
        self.files
            .binary_search_by(|p| p.as_str().cmp(path))
            .is_ok()
            && !prefix(&self.read_only_roots, path)
    }
}
pub(crate) fn check_current_write(
    store: &crate::store::Store,
    path: &str,
    d: &Deadline,
) -> Result<()> {
    let Some(scope) = &store.current_repository_scope else {
        return Ok(());
    };
    d.check()?;
    paths::normalized(path)?;
    ensure!(
        scope.permits(path),
        "repository_capture_current_host_scope_excludes_write"
    );
    #[cfg(windows)]
    ensure!(
        scope.permits(&super::canonical_write_path(store, path)?),
        "repository_capture_current_host_scope_excludes_write"
    );
    d.check()?;
    Ok(())
}
