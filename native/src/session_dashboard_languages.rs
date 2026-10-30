//! Project-local metadata selection from a pinned, independently granted host registry.
use super::*;
use crate::session_dashboard::management::{LanguageGrant, LanguageSelection};

fn grant(host: &HostProfile) -> Option<&LanguageGrant> {
    host.dashboard
        .as_ref()?
        .management
        .as_ref()?
        .language_servers
        .as_ref()
}
pub(super) fn validate_retained(overrides: &BTreeMap<String, LanguageSelection>) -> Result<()> {
    ensure!(
        overrides.len() <= 101 && serde_json::to_vec(overrides)?.len() <= 32000,
        "dashboard_language_override_capacity"
    );
    for (project, selection) in overrides {
        label(project)?;
        selection.validate()?;
    }
    Ok(())
}
pub(super) fn validate_overrides(
    host: &HostProfile,
    overrides: &BTreeMap<String, LanguageSelection>,
) -> Result<()> {
    validate_retained(overrides)?;
    for (project, selection) in overrides {
        ensure!(
            project == "default" || host.projects.iter().any(|p| p.name == *project),
            "dashboard_language_project_not_registered"
        );
        grant(host)
            .context("dashboard_language_grant_required")?
            .permits(selection)?;
    }
    Ok(())
}
pub(super) struct Registry {
    servers: BTreeMap<String, crate::lsp::Server>,
    configurations: BTreeMap<String, crate::lsp::HostConfiguration>,
    backends: BTreeMap<String, crate::lsp::ConfiguredServer>,
}
fn legacy_binding(servers: &BTreeMap<String, crate::lsp::Server>) -> Value {
    json!(
        servers
            .iter()
            .map(|(id, s)| (id, &s.language, &s.command, &s.args, s.trust))
            .collect::<Vec<_>>()
    )
}
struct Projection {
    servers: BTreeMap<String, crate::lsp::Server>,
    configurations: BTreeMap<String, crate::lsp::HostConfiguration>,
    selected_backends: Option<Vec<String>>,
    registered_backends: BTreeMap<String, crate::lsp::ConfiguredServer>,
}
impl Projection {
    fn apply(self, engine: &mut Engine) {
        engine.servers = self.servers;
        engine.server_configurations = self.configurations;
        engine.selected_backends = self.selected_backends;
        engine.backend_servers = self.registered_backends;
    }
}
impl Registry {
    pub(super) fn capture(host: &HostProfile, engine: &Engine) -> Result<Option<Self>> {
        let Some(grant) = grant(host) else {
            return Ok(None);
        };
        grant.validate()?;
        ensure!(
            grant
                .backend_ids
                .iter()
                .all(|id| engine.backend_servers.contains_key(id))
                && grant
                    .legacy_languages
                    .iter()
                    .all(|id| engine.servers.contains_key(id)),
            "dashboard_language_not_host_registered"
        );
        let registry = Self {
            servers: engine.servers.clone(),
            configurations: engine.server_configurations.clone(),
            backends: engine.backend_servers.clone(),
        };
        registry.binding()?;
        Ok(Some(registry))
    }
    pub(super) fn binding(&self) -> Result<Value> {
        let value = json!({"legacy":legacy_binding(&self.servers),"configurations":self.configurations,
            "backends":crate::server_selection::bindings(&self.backends)?});
        ensure!(
            serde_json::to_vec(&value)?.len() <= 1_000_000,
            "dashboard_language_registry_capacity"
        );
        Ok(value)
    }
    fn projection(
        &self,
        host: &HostProfile,
        project: &str,
        selection: Option<&LanguageSelection>,
    ) -> Result<Projection> {
        let mut servers = self.servers.clone();
        let mut configurations = self.configurations.clone();
        let mut backends = host
            .language_servers
            .as_ref()
            .and_then(|s| s.select(project))
            .map(<[String]>::to_vec);
        if let Some(selection) = selection {
            grant(host)
                .context("dashboard_language_grant_required")?
                .permits(selection)?;
            match selection {
                LanguageSelection::Backends { ids } => backends = Some(ids.clone()),
                LanguageSelection::Legacy { languages } => {
                    backends = None;
                    servers.retain(|id, _| languages.contains(id));
                    configurations.retain(|id, _| languages.contains(id));
                }
            }
        }
        Ok(Projection {
            servers,
            configurations,
            selected_backends: backends,
            registered_backends: self.configured_backends(host, project)?,
        })
    }
    fn configured_backends(
        &self,
        host: &HostProfile,
        project: &str,
    ) -> Result<BTreeMap<String, crate::lsp::ConfiguredServer>> {
        if let Some(settings) = &host.language_server_settings {
            settings.apply(project, &self.backends)
        } else {
            Ok(self.backends.clone())
        }
    }
    pub(super) fn apply(
        &self,
        engine: &mut Engine,
        host: &HostProfile,
        project: &str,
        selection: Option<&LanguageSelection>,
    ) -> Result<()> {
        self.projection(host, project, selection)?.apply(engine);
        Ok(())
    }
    pub(super) fn check(
        &self,
        engine: &Engine,
        host: &HostProfile,
        project: &str,
        selection: Option<&LanguageSelection>,
    ) -> Result<()> {
        let projected = self.projection(host, project, selection)?;
        ensure!(
            legacy_binding(&engine.servers) == legacy_binding(&projected.servers)
                && serde_json::to_value(&engine.server_configurations)?
                    == serde_json::to_value(&projected.configurations)?
                && engine.selected_backends == projected.selected_backends
                && crate::server_selection::bindings(&engine.backend_servers)?
                    == crate::server_selection::bindings(&projected.registered_backends)?,
            "dashboard_language_registry_changed"
        );
        Ok(())
    }
}
impl Session {
    pub(crate) fn dashboard_languages_granted(&self) -> bool {
        grant(&self.host).is_some()
    }
    pub(super) fn dashboard_language_view(&self) -> Option<Value> {
        let allowed = grant(&self.host)?;
        let registry = self.dashboard_language_registry.as_ref()?;
        let project = &self.state.active_project;
        let selected = self.state.language_server_overrides.get(project);
        let projected = registry.projection(&self.host, project, selected).ok()?;
        Some(
            json!({"backendChoices":allowed.backend_ids,"legacyChoices":allowed.legacy_languages,
            "selectedBackends":projected.selected_backends,"selectedLegacyLanguages":projected.servers.keys().collect::<Vec<_>>(),
            "override":selected,"resetAvailable":selected.is_some(),"providerLifecycle":"per_request",
            "runtimeCertification":false,"sourceWritten":false}),
        )
    }
    pub(super) fn dashboard_select_languages(
        &mut self,
        selection: Option<LanguageSelection>,
        deadline: &Deadline,
    ) -> Result<Value> {
        self.check_tool_state(deadline)?;
        let allowed = grant(&self.host).context("dashboard_language_grant_required")?;
        if let Some(selection) = &selection {
            allowed.permits(selection)?;
        }
        let project = self.state.active_project.clone();
        let mut next = self.state.clone();
        if let Some(selection) = selection {
            next.language_server_overrides
                .insert(project.clone(), selection);
        } else {
            next.language_server_overrides.remove(&project);
        }
        validate_overrides(&self.host, &next.language_server_overrides)?;
        // Validate all fallible projections before the atomic state comparison.
        let registry = self
            .dashboard_language_registry
            .as_ref()
            .context("dashboard_language_grant_required")?;
        let projected = registry.projection(
            &self.host,
            &project,
            next.language_server_overrides.get(&project),
        )?;
        if next.language_server_overrides == self.state.language_server_overrides {
            return Ok(json!({"changed":false}));
        }
        self.update(next, deadline)?;
        let engine = if project == "default" {
            Some(&mut self.engine)
        } else {
            self.others.get_mut(&project)
        };
        if let Some(engine) = engine {
            projected.apply(engine);
        }
        Ok(json!({"changed":true,"selection":self.dashboard_language_view()}))
    }
}
