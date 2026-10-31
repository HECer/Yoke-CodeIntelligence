//! CLI-only authority construction. Selection files remain inert without the
//! independent execution switch on the actual host process invocation.
use anyhow::{Result, ensure};
use clap::Args;
use std::path::{Path, PathBuf};
use yoke::{
    capture::Deadline,
    project_activation::{Runtime, Selection},
    project_session::HostProfile,
};

#[derive(Args, Default)]
pub(crate) struct Arguments {
    /// Execute exactly selected activation commands; arbitrary host shell effects.
    #[arg(long, global = true)]
    pub allow_project_activation: bool,
    /// Private JSON array of exact project/root/config/command/shell selections.
    #[arg(long, global = true)]
    pub activation_selection: Option<PathBuf>,
}
impl Arguments {
    pub(crate) fn selected(&self) -> bool {
        self.allow_project_activation || self.activation_selection.is_some()
    }
    pub(crate) fn validate(&self) -> Result<()> {
        ensure!(
            self.allow_project_activation == self.activation_selection.is_some(),
            "activation_host_switch_and_selection_required"
        );
        Ok(())
    }
    pub(crate) fn bind(&self, host: &mut HostProfile, root: &Path, d: &Deadline) -> Result<()> {
        self.validate()?;
        host.activation_runtime = None;
        let Some(path) = &self.activation_selection else {
            return Ok(());
        };
        let Some(config) = &host.activation_commands else {
            return Ok(());
        };
        config.validate(host, d)?;
        let selections: Vec<Selection> = yoke::json::decode(crate::input_json(path, 256000)?)?;
        ensure!(
            !selections.is_empty() && selections.len() <= 101,
            "activation_selection_capacity"
        );
        let mut selected = Vec::new();
        for selection in selections {
            d.check()?;
            let expected = if selection.project == "default" {
                Some(root)
            } else {
                host.projects
                    .iter()
                    .find(|p| p.name == selection.project)
                    .map(|p| p.root.as_path())
            };
            if selection.configuration_sha256 == config.fingerprint()
                && let Some(expected) = expected
                && selection.root.is_absolute()
                && selection.root.canonicalize()? == expected.canonicalize()?
            {
                selected.push(selection);
            }
        }
        ensure!(!selected.is_empty(), "activation_selection_not_for_target");
        host.activation_runtime = Some(Runtime::authorize(
            host,
            &root.canonicalize()?,
            &selected,
            d,
        )?);
        Ok(())
    }
}
