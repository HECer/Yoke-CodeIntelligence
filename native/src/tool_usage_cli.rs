//! CLI arguments grant token-count text disclosure independently of policy data.
use anyhow::{Result, ensure};
use yoke::{
    capture::Deadline,
    project_session::HostProfile,
    tool_usage::{Estimator, http::Host},
};

#[derive(Clone, Default, clap::Args)]
#[group(id = "tool_token_count_disclosure")]
pub struct Arguments {
    /// Permit selected tool arguments/results to leave this host for token counting.
    #[arg(long, global = true, requires = "token_count_credential")]
    pub allow_tool_token_disclosure: bool,
    /// Existing native OS model-token credential name; never a raw key or environment lookup.
    #[arg(long, global = true, requires = "allow_tool_token_disclosure")]
    pub token_count_credential: Option<String>,
}
impl Arguments {
    pub fn selected(&self) -> bool {
        self.allow_tool_token_disclosure || self.token_count_credential.is_some()
    }
    pub fn validate(&self) -> Result<()> {
        ensure!(
            !self.selected()
                || (self.allow_tool_token_disclosure && self.token_count_credential.is_some()),
            "tool_usage_network_and_text_disclosure_flags_required"
        );
        if let Some(name) = &self.token_count_credential {
            ensure!(
                !name.is_empty()
                    && name.len() <= 80
                    && name.as_bytes()[0].is_ascii_alphanumeric()
                    && name
                        .bytes()
                        .all(|c| c.is_ascii_alphanumeric() || b"_.-".contains(&c)),
                "tool_usage_credential_reference_invalid"
            );
        }
        Ok(())
    }
    pub fn bind(&self, host: &mut HostProfile, deadline: &Deadline) -> Result<()> {
        deadline.check()?;
        self.validate()?;
        if host.token_count_estimator == Some(Estimator::AnthropicClaudeSonnet4) {
            ensure!(
                self.allow_tool_token_disclosure,
                "tool_usage_independent_text_disclosure_grant_required"
            );
            host.token_count_runtime = Some(Host::anthropic(
                self.token_count_credential
                    .clone()
                    .expect("Validated explicit host grant"),
                true,
            )?);
        }
        Ok(())
    }
}
