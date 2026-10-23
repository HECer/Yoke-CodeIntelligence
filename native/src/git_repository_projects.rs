//! Explicit Git authorities for a complete, existing host project registration.
use super::repository_host::Host;
use crate::{capture::Deadline, project_session::HostProfile};
use anyhow::{Context, Result, ensure};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
};

/// Invocation-owned typed authorities. Serialized project selections cannot create this set.
#[derive(Clone)]
pub struct Hosts {
    root: PathBuf,
    hosts: BTreeMap<String, Host>,
}
impl Hosts {
    /// Prepare primary metadata before creating state; the caller then applies
    /// capture/server policy and opens the matching Session before source capture.
    pub fn open_primary(&self, d: &Deadline) -> Result<crate::Engine> {
        self.check_current(d)?;
        let prepared = self.primary()?.clone().prepare(d)?;
        let mut engine = crate::Engine::open_with_timeout(&self.root, d.remaining())?;
        prepared.attach_prepared(&mut engine, d)?;
        Ok(engine)
    }
    /// Admit the complete name selection before resolving any registered root.
    pub fn validate_selectors(host: &HostProfile, selected: &BTreeSet<String>) -> Result<()> {
        ensure!(
            host.projects.len() <= 100 && host.collections.is_empty(),
            "Git project hosts require bounded projects without collections"
        );
        let mut expected = BTreeSet::from(["default".to_owned()]);
        for project in &host.projects {
            crate::project_session::label(&project.name)?;
            ensure!(
                project.name != "default"
                    && project.root.is_absolute()
                    && expected.insert(project.name.clone()),
                "Invalid Git host project registration"
            );
        }
        ensure!(
            *selected == expected,
            "Git project hosts require exactly default and every registered project"
        );
        Ok(())
    }
    pub fn new(
        root: &Path,
        host: &HostProfile,
        hosts: BTreeMap<String, Host>,
        d: &Deadline,
    ) -> Result<Self> {
        Self::validate_selectors(host, &hosts.keys().cloned().collect())?;
        let root = root.canonicalize()?;
        let result = Self { root, hosts };
        result.validate(host, &result.root, d)?;
        Ok(result)
    }
    pub(crate) fn validate(&self, host: &HostProfile, root: &Path, d: &Deadline) -> Result<()> {
        Self::validate_selectors(host, &self.hosts.keys().cloned().collect())?;
        ensure!(
            self.root == root && self.primary()?.root() == root,
            "Git project host primary root changed"
        );
        let mut roots = vec![self.root.as_path()];
        for project in &host.projects {
            d.check()?;
            let driver = self
                .hosts
                .get(&project.name)
                .context("Git host project selection missing")?;
            ensure!(
                project.root.canonicalize()? == driver.root(),
                "Git host project root changed"
            );
            ensure!(
                roots
                    .iter()
                    .all(|root| !crate::project_session::roots_overlap(root, driver.root())),
                "Git project host roots must not overlap"
            );
            roots.push(driver.root());
        }
        self.binding()?;
        self.check_current(d)
    }
    pub(crate) fn primary(&self) -> Result<&Host> {
        self.hosts
            .get("default")
            .context("Primary Git host missing")
    }
    pub(crate) fn project(&self, name: &str) -> Option<&Host> {
        self.hosts.get(name)
    }
    pub(crate) fn binding(&self) -> Result<String> {
        let bindings = self
            .hosts
            .iter()
            .map(|(name, host)| Ok((name, host.binding()?)))
            .collect::<Result<BTreeMap<_, _>>>()?;
        Ok(crate::model::identity(&(
            "host_git_registered_projects_v1",
            &self.root,
            bindings,
        )))
    }
    pub(crate) fn check_current(&self, d: &Deadline) -> Result<()> {
        for host in self.hosts.values() {
            d.check()?;
            host.check_current(d)?;
        }
        Ok(())
    }
}
