//! Host-file selection data. Execution/index authority stays in independent CLI flags.
use anyhow::{Result, ensure};
use serde::Deserialize;
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};
use yoke::{
    capture::Deadline,
    git_workflows::{
        repository_host::Host,
        repository_preferences::Resolved,
        repository_projects::Hosts,
        repository_scope::{Limits, Options},
    },
};

#[derive(Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields, default)]
pub struct Selection {
    program: Option<PathBuf>,
    follow_submodules: Option<bool>,
    follow_nested_repositories: Option<bool>,
    limits: Limits,
}
pub fn admit_projects(
    selections: &BTreeMap<String, Selection>,
    host: &yoke::project_session::HostProfile,
    allow_index_read: bool,
    trust_git: bool,
) -> Result<()> {
    Hosts::validate_selectors(host, &selections.keys().cloned().collect())?;
    for selection in selections.values() {
        selection.admit(allow_index_read, trust_git)?;
    }
    Ok(())
}
pub fn prepare_projects(
    root: &Path,
    host: &yoke::project_session::HostProfile,
    selections: &BTreeMap<String, Selection>,
    allow_index_read: bool,
    trust_git: bool,
    d: &Deadline,
) -> Result<Hosts> {
    admit_projects(selections, host, allow_index_read, trust_git)?;
    let mut drivers = BTreeMap::new();
    for (name, selection) in selections {
        d.check()?;
        let selected_root = if name == "default" {
            root
        } else {
            &host
                .projects
                .iter()
                .find(|p| &p.name == name)
                .ok_or_else(|| anyhow::anyhow!("Git project is not host-registered"))?
                .root
        };
        drivers.insert(
            name.clone(),
            selection.prepare(selected_root, allow_index_read, trust_git, d)?,
        );
    }
    Hosts::new(root, host, drivers, d)
}
impl Selection {
    pub fn admit(&self, allow_index_read: bool, trust_git: bool) -> Result<()> {
        ensure!(
            allow_index_read && trust_git,
            "Git capture requires --allow-git-index-read and --trust-git"
        );
        self.limits.validate()?;
        if let Some(program) = &self.program {
            let name = program
                .to_str()
                .ok_or_else(|| anyhow::anyhow!("repository_host_program_encoding_invalid"))?;
            ensure!(
                program.is_absolute() && name.len() <= 16_384 && !name.contains('\0'),
                "repository_host_program_invalid"
            );
        }
        Ok(())
    }
    pub fn prepare(
        &self,
        root: &Path,
        allow_index_read: bool,
        trust_git: bool,
        d: &Deadline,
    ) -> Result<Host> {
        self.admit(allow_index_read, trust_git)?;
        d.check()?;
        let git = crate::selected_git(self.program.clone(), trust_git)?;
        let preference = Resolved::current(
            root,
            self.follow_submodules,
            self.follow_nested_repositories,
            d,
        )?;
        Host::new(
            root,
            git,
            Options {
                allow_index_read,
                limits: self.limits.clone(),
                ..Default::default()
            },
            preference,
            d,
        )
    }
}
