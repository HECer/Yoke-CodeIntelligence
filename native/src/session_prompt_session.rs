//! Explicit existing authored-note stores; never capture sources while rendering.
use super::{Session, notes};
use crate::{
    capture::Deadline,
    session_prompt::{Config, memory::Snapshot},
    store::Store,
};
use anyhow::{Context, Result, ensure};
use std::sync::Arc;

impl Session {
    pub(super) fn prompt_memories(
        &self,
        config: &Config,
        d: &Deadline,
    ) -> Result<Option<Arc<Snapshot>>> {
        let Some(names) = &config.memory_embeddings else {
            return Ok(None);
        };
        self.check_tool_state(d)?;
        ensure!(
            !self.state.modes.iter().any(|m| m == "no-memories"),
            "Notes disabled by session mode"
        );
        self.require_tool("read_memory", d)?;
        self.require_repl_api("mem.read_memory")?;
        let patterns = self
            .steering()?
            .map(|s| {
                notes::memory_patterns(&s.read_only_memory_patterns, &s.ignored_memory_patterns)
            })
            .transpose()?;
        // Admit every selected name and policy before opening either payload store.
        let selected = names
            .iter()
            .map(|name| {
                d.check()?;
                notes::admit_prompt(name, &self.host.notes_policy, patterns.as_ref())
            })
            .collect::<Result<Vec<_>>>()?;
        let project = if selected
            .iter()
            .any(|(scope, _)| *scope == notes::Scope::Project)
        {
            ensure!(
                !self
                    .state
                    .removed_projects
                    .contains(&self.state.active_project),
                "Project removed from session"
            );
            let root = if self.state.active_project == "default" {
                &self.engine.root
            } else {
                &self
                    .host
                    .projects
                    .iter()
                    .find(|p| p.name == self.state.active_project)
                    .context("Project is not host-registered")?
                    .root
            };
            Some(Store::open_existing_read_only(root, d)?)
        } else {
            None
        };
        let global = if selected
            .iter()
            .any(|(scope, _)| *scope == notes::Scope::Global)
        {
            let root = self
                .host
                .global_notes_root
                .as_ref()
                .context("No global notes store registered by host")?;
            Some(Store::open_existing_read_only(root, d)?)
        } else {
            None
        };
        let mut snapshot = Snapshot::default();
        for (raw, (scope, _)) in names.iter().zip(selected) {
            d.check()?;
            let store = if scope == notes::Scope::Global {
                &global
            } else {
                &project
            }
            .as_ref()
            .context("Selected note store unavailable")?;
            snapshot.entries.insert(
                raw.clone(),
                notes::read_prompt(store, raw, &self.host.notes_policy, patterns.as_ref(), d)?,
            );
        }
        snapshot.check_selection(names, d)?;
        self.check_tool_state(d)?;
        Ok(Some(Arc::new(snapshot)))
    }
}
