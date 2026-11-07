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
        self.prompt_memories_for_state(config, &self.state, d)
    }

    fn prompt_memories_for_state(
        &self,
        config: &Config,
        state: &super::State,
        d: &Deadline,
    ) -> Result<Option<Arc<Snapshot>>> {
        let Some(names) = &config.memory_embeddings else {
            return Ok(None);
        };
        self.check_tool_state(d)?;
        ensure!(
            !state.modes.iter().any(|m| m == "no-memories"),
            "Notes disabled by session mode"
        );
        let steering = self
            .steering_enabled()
            .then(|| {
                crate::session_steering::resolve(
                    &self.host,
                    &state.active_project,
                    state.mode_override.as_deref(),
                )
            })
            .transpose()?;
        self.check_tool_state(d)?;
        ensure!(
            steering.as_ref().is_none_or(|s| s.allows("read_memory")),
            "Tool excluded by session steering"
        );
        ensure!(
            steering
                .as_ref()
                .is_none_or(|s| s.api_allowed("mem.read_memory")),
            "API excluded by session steering"
        );
        let patterns = steering
            .as_ref()
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
                !self.state.removed_projects.contains(&state.active_project),
                "Project removed from session"
            );
            let root = if state.active_project == "default" {
                &self.engine.root
            } else {
                &self
                    .host
                    .projects
                    .iter()
                    .find(|p| p.name == state.active_project)
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

    pub(super) fn admit_required_memories_for_state(
        &self,
        state: &super::State,
        d: &Deadline,
    ) -> Result<()> {
        if self.host.required_prompt_memories.is_none() {
            return Ok(());
        }
        crate::required_prompt_memories::admit_host(&self.host, d)?;
        let config = self
            .host
            .prompt_templates
            .as_ref()
            .context("required_memory_templates_required")?;
        let snapshot = self.prompt_memories_for_state(config, state, d)?;
        crate::required_prompt_memories::prepare(
            &self.host,
            &state.active_project,
            snapshot.as_deref(),
            d,
        )?;
        Ok(())
    }
}
