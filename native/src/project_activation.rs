//! Inert activation declarations and a distinct, invocation-owned shell grant.
//! Independently implemented MIT policy; repository data never grants execution.
use crate::{capture::Deadline, model, project_session::HostProfile};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    process::Command,
    sync::Arc,
    time::Duration,
};

#[path = "project_activation_path.rs"]
mod path;
pub const VERSION: &str = "native_project_activation_v1";
pub const MAX_COMMAND_BYTES: usize = 8192;
pub(crate) const ACTIVATION_RESERVE: Duration = Duration::from_millis(250);

/// Missing command inherits; explicit null disables. Missing/null timeout inherits.
#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Layer(BTreeMap<String, Value>);
#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProjectLayers {
    #[serde(default)]
    pub project: Layer,
    #[serde(default)]
    pub local: Layer,
}
#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Config {
    #[serde(default)]
    pub global: Layer,
    #[serde(default)]
    pub projects: BTreeMap<String, ProjectLayers>,
}
pub struct Selected<'a> {
    pub command: Option<&'a str>,
    pub timeout: Duration,
}
pub fn timeout(value: &Value) -> Result<Duration> {
    let seconds = value
        .as_f64()
        .context("session_config_activation_timeout_unavailable")?;
    ensure!(
        seconds.is_finite() && seconds > 0.0,
        "session_config_activation_timeout_unavailable"
    );
    let duration = Duration::try_from_secs_f64(seconds)
        .map_err(|_| anyhow::anyhow!("session_config_activation_timeout_unavailable"))?;
    ensure!(
        !duration.is_zero()
            && u64::try_from(duration.as_nanos()).is_ok()
            && (duration.as_secs_f64() - seconds).abs() <= seconds.abs() * f64::EPSILON * 2.0
            && std::time::Instant::now().checked_add(duration).is_some(),
        "session_config_activation_timeout_unavailable"
    );
    Ok(duration)
}
pub fn command(value: &Value) -> Result<()> {
    ensure!(
        value.is_null()
            || value.as_str().is_some_and(|s| !s.trim().is_empty()
                && s.len() <= MAX_COMMAND_BYTES
                && !s.contains('\0')),
        "session_config_activation_command_unavailable"
    );
    Ok(())
}
impl Layer {
    fn validate(&self) -> Result<()> {
        for (key, value) in &self.0 {
            match key.as_str() {
                "activation_command" => command(value)?,
                "activation_command_timeout" => {
                    if !value.is_null() {
                        timeout(value)?;
                    }
                }
                _ => anyhow::bail!("activation_layer_unknown_field"),
            }
        }
        Ok(())
    }
}
impl Config {
    pub fn fingerprint(&self) -> String {
        model::identity(&(VERSION, self))
    }
    pub fn validate(&self, host: &HostProfile, d: &Deadline) -> Result<()> {
        crate::json::encode_bounded(self, 96000, d)?;
        self.global.validate()?;
        ensure!(self.projects.len() <= 101, "activation_project_capacity");
        for (name, layers) in &self.projects {
            crate::project_session::label(name)?;
            ensure!(
                name == "default" || host.projects.iter().any(|p| p.name == *name),
                "activation_project_not_registered"
            );
            layers.project.validate()?;
            layers.local.validate()?;
        }
        Ok(())
    }
    pub(crate) fn project_yaml(
        &mut self,
        target: &crate::session_config::Target,
        fields: &Map<String, Value>,
    ) {
        let layer = match target {
            crate::session_config::Target::Global => &mut self.global,
            crate::session_config::Target::Project { project } => {
                &mut self.projects.entry(project.clone()).or_default().project
            }
            crate::session_config::Target::ProjectLocal { project } => {
                &mut self.projects.entry(project.clone()).or_default().local
            }
            _ => return,
        };
        for key in ["activation_command", "activation_command_timeout"] {
            if let Some(value) = fields.get(key) {
                layer.0.insert(key.into(), value.clone());
            }
        }
    }
    /// Legacy null/default declarations alone produce no new profile/binding.
    pub(crate) fn selected(&self) -> bool {
        std::iter::once(&self.global)
            .chain(self.projects.values().flat_map(|p| [&p.project, &p.local]))
            .any(|layer| {
                layer.0.iter().any(|(k, v)| match k.as_str() {
                    "activation_command" => !v.is_null(),
                    "activation_command_timeout" => !v.is_null() && v.as_u64() != Some(180),
                    _ => true,
                })
            })
    }
    pub fn resolve(&self, project: &str) -> Result<Selected<'_>> {
        let mut selected = Selected {
            command: None,
            timeout: Duration::from_secs(180),
        };
        for layer in std::iter::once(&self.global).chain(
            self.projects
                .get(project)
                .into_iter()
                .flat_map(|p| [&p.project, &p.local]),
        ) {
            layer.validate()?;
            if let Some(value) = layer.0.get("activation_command") {
                selected.command = value.as_str();
            }
            if let Some(value) = layer
                .0
                .get("activation_command_timeout")
                .filter(|v| !v.is_null())
            {
                selected.timeout = timeout(value)?;
            }
        }
        Ok(selected)
    }
}

/// Data describing the host's exact selection; it is not a runtime grant.
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Selection {
    pub project: String,
    pub root: PathBuf,
    pub command_sha256: String,
    pub configuration_sha256: String,
    pub shell: Shell,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Shell {
    pub executable: PathBuf,
    pub kind: ShellKind,
}
#[derive(Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ShellKind {
    Posix,
    Powershell,
}
struct Grant {
    project: String,
    command: String,
    root: path::Pin,
    shell_pin: path::Pin,
    shell: Shell,
}
struct Authority {
    configuration: String,
    grants: BTreeMap<String, Grant>,
    environment: Vec<(String, Option<String>)>,
    binding: String,
}
/// Deliberately has no Serialize/Deserialize/Default implementation. Only the
/// trusted host calls authorize, independently of decoding any profile/request.
#[derive(Clone)]
pub struct Runtime(Arc<Authority>);
impl Runtime {
    pub fn authorize(
        host: &HostProfile,
        primary_root: &Path,
        selections: &[Selection],
        d: &Deadline,
    ) -> Result<Self> {
        d.check()?;
        let config = host
            .activation_commands
            .as_ref()
            .context("activation_selection_required")?;
        crate::json::encode_bounded(selections, 256000, d)?;
        config.validate(host, d)?;
        ensure!(
            !selections.is_empty() && selections.len() <= 101,
            "activation_selection_capacity"
        );
        let configuration = config.fingerprint();
        let mut grants = BTreeMap::new();
        for selection in selections {
            d.check()?;
            crate::project_session::label(&selection.project)?;
            let expected_root = if selection.project == "default" {
                primary_root
            } else {
                &host
                    .projects
                    .iter()
                    .find(|p| p.name == selection.project && !p.query_only)
                    .context("activation_project_not_available")?
                    .root
            };
            let selected = config.resolve(&selection.project)?;
            let command = selected
                .command
                .context("activation_command_not_selected")?;
            ensure!(
                selection.configuration_sha256 == configuration
                    && model::valid_id(&selection.command_sha256)
                    && selection.command_sha256 == model::hash(command),
                "activation_selection_changed"
            );
            let root = path::Pin::directory(&selection.root, d)?;
            root.check(expected_root, d)?;
            let shell_pin = path::Pin::file(&selection.shell.executable, d)?;
            ensure!(
                grants
                    .insert(
                        selection.project.clone(),
                        Grant {
                            project: selection.project.clone(),
                            command: command.into(),
                            root,
                            shell_pin,
                            shell: selection.shell.clone(),
                        }
                    )
                    .is_none(),
                "activation_duplicate_selection"
            );
        }
        let environment = [
            "PATH",
            "SystemRoot",
            "SystemDrive",
            "WINDIR",
            "USERPROFILE",
            "HOME",
            "TEMP",
            "TMP",
            "LANG",
            "RUSTUP_HOME",
            "CARGO_HOME",
        ]
        .into_iter()
        .map(|key| {
            let value = std::env::var_os(key)
                .map(|v| {
                    v.into_string()
                        .map_err(|_| anyhow::anyhow!("activation_environment_unrepresentable"))
                })
                .transpose()?;
            ensure!(
                value.as_ref().is_none_or(|v| v.len() <= 64000),
                "activation_environment_capacity"
            );
            Ok((key.into(), value))
        })
        .collect::<Result<Vec<_>>>()?;
        let pins = grants
            .iter()
            .map(|(name, g)| (name, g.root.binding(), g.shell_pin.binding(), &g.shell))
            .collect::<Vec<_>>();
        let binding = model::identity(&(VERSION, &configuration, pins, &environment));
        Ok(Self(Arc::new(Authority {
            configuration,
            grants,
            environment,
            binding,
        })))
    }
    pub(crate) fn binding(&self) -> &str {
        &self.0.binding
    }
    pub(crate) fn check(
        &self,
        config: &Config,
        project: &str,
        root: &Path,
        d: &Deadline,
    ) -> Result<()> {
        d.check()?;
        ensure!(
            self.0.configuration == config.fingerprint(),
            "activation_configuration_changed"
        );
        let command = config
            .resolve(project)?
            .command
            .context("activation_command_not_selected")?;
        let grant = self
            .0
            .grants
            .get(project)
            .context("activation_host_permission_required")?;
        ensure!(
            grant.project == project && grant.command == command,
            "activation_command_changed"
        );
        grant.root.check(root, d)?;
        grant.shell_pin.check(&grant.shell.executable, d)?;
        Ok(())
    }
    pub(crate) fn run(
        &self,
        config: &Config,
        project: &str,
        root: &Path,
        enclosing: &Deadline,
    ) -> Result<Diagnostic> {
        self.check(config, project, root, enclosing)?;
        ensure!(
            enclosing.remaining() > ACTIVATION_RESERVE,
            "activation_enclosing_budget_insufficient"
        );
        let selected = config.resolve(project)?;
        let grant = &self.0.grants[project];
        let requested = selected.timeout;
        let command_deadline = Deadline::from_duration(requested).intersection(enclosing)?;
        let effective = command_deadline.remaining();
        let mut command = Command::new(grant.shell_pin.path());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x0800_0000);
        }
        match grant.shell.kind {
            ShellKind::Posix => {
                command.args(["-c", &grant.command]);
            }
            ShellKind::Powershell => {
                command.args([
                    "-NoLogo",
                    "-NoProfile",
                    "-NonInteractive",
                    "-Command",
                    &grant.command,
                ]);
            }
        }
        command.current_dir(grant.root.path());
        for (key, value) in &self.0.environment {
            if let Some(value) = value {
                command.env(key, value);
            } else {
                command.env_remove(key);
            }
        }
        let captured = crate::process::run_capture_observed(
            command,
            &[],
            &command_deadline,
            enclosing,
            crate::process::CaptureLimits {
                stdin_bytes: 0,
                stdout_bytes: 64000,
                stderr_bytes: 16000,
            },
        )?;
        // Only the checked-capture constructor can issue this typed observation.
        // Timeout and nonzero exit never excuse missing EOF, cancellation or caps.
        captured.require_complete()?;
        enclosing.check()?;
        ensure!(
            enclosing.remaining() > ACTIVATION_RESERVE,
            "activation_enclosing_budget_insufficient"
        );
        let timed_out = captured.command_timed_out();
        let output = captured.into_output();
        Ok(Diagnostic {
            exit_code: output.exit_code,
            timed_out,
            stdout: output.stdout,
            stderr: output.stderr,
            requested_timeout_ns: u64::try_from(requested.as_nanos())?,
            effective_timeout_ns: u64::try_from(effective.as_nanos())?,
        })
    }
}

/// Private host diagnostics: never included in public configuration/protocol replies.
pub struct Diagnostic {
    pub exit_code: Option<i32>,
    pub timed_out: bool,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
    pub requested_timeout_ns: u64,
    pub effective_timeout_ns: u64,
}
#[cfg(test)]
#[path = "project_activation_tests.rs"]
mod tests;
