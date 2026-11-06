//! Exact host-confirmed authored notes. Content never becomes execution authority.
use crate::{
    capture::Deadline,
    model,
    notes::Scope,
    project_session::HostProfile,
    session_prompt::memory::{self, Entry, Snapshot},
};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

pub const VERSION: &str = "host_required_prompt_memories_v1";
const MAX_POLICY: usize = 64_000;
#[cfg(test)]
#[path = "required_prompt_memory_tests.rs"]
mod tests;

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Pin {
    pub name: String,
    pub revision_id: String,
    pub content_sha256: String,
    pub storage_workspace_id: String,
    pub source_workspace_id: String,
    pub snapshot_id: String,
}

#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Policy {
    #[serde(default)]
    pub global: Vec<Pin>,
    #[serde(default)]
    pub projects: BTreeMap<String, Vec<Pin>>,
}

impl Policy {
    fn selected<'a>(&'a self, project: &str) -> impl Iterator<Item = &'a Pin> {
        self.global
            .iter()
            .chain(self.projects.get(project).into_iter().flatten())
    }

    pub fn validate(&self, host: &HostProfile, d: &Deadline) -> Result<()> {
        crate::json::encode_bounded(self, MAX_POLICY, d)?;
        let config = host
            .prompt_templates
            .as_ref()
            .context("required_memory_templates_required")?;
        config.validate(d)?;
        let names = config
            .memory_embeddings
            .as_ref()
            .context("required_memory_selection_required")?;
        let mut count = 0usize;
        ensure!(
            self.projects.len() <= 101,
            "required_memory_project_capacity"
        );
        for (project, pins) in std::iter::once((None, &self.global))
            .chain(self.projects.iter().map(|(name, pins)| (Some(name), pins)))
        {
            d.check()?;
            if let Some(project) = project {
                ensure!(
                    project == "default" || host.projects.iter().any(|p| p.name == *project),
                    "required_memory_project_unregistered"
                );
                ensure!(!pins.is_empty(), "required_memory_empty_project_selection");
            }
            let mut seen = BTreeSet::new();
            for pin in pins {
                d.check()?;
                count += 1;
                ensure!(
                    count <= memory::MAX_NAMES,
                    "required_memory_selection_capacity"
                );
                let (scope, _) = crate::notes::prompt_selection(&pin.name)?;
                ensure!(
                    (scope == Scope::Global) == project.is_none()
                        && names.contains(&pin.name)
                        && seen.insert(&pin.name),
                    "required_memory_selection_scope_or_name"
                );
                ensure!(
                    [
                        &pin.revision_id,
                        &pin.content_sha256,
                        &pin.storage_workspace_id,
                        &pin.source_workspace_id,
                        &pin.snapshot_id
                    ]
                    .into_iter()
                    .all(|id| model::valid_id(id)),
                    "required_memory_pin_invalid"
                );
            }
        }
        ensure!(count > 0, "required_memory_empty_selection");
        Ok(())
    }
}

fn binding(host: &HostProfile, d: &Deadline) -> Result<String> {
    let policy = host
        .required_prompt_memories
        .as_ref()
        .context("required_memory_policy_required")?;
    policy.validate(host, d)?;
    let config = host
        .prompt_templates
        .as_ref()
        .context("required_memory_templates_required")?;
    Ok(model::hash(crate::json::encode_bounded(
        &(VERSION, policy, config.fingerprint()?),
        MAX_POLICY,
        d,
    )?))
}

/// Explicit host confirmation, without serde/default constructors. This is not
/// an attestation of a person's identity or transcript authenticity.
#[derive(Clone)]
pub struct Runtime {
    binding: String,
    live: Arc<AtomicBool>,
}
impl Runtime {
    pub fn confirm(host: &HostProfile, d: &Deadline) -> Result<Self> {
        Ok(Self {
            binding: binding(host, d)?,
            live: Arc::new(AtomicBool::new(true)),
        })
    }
    pub fn revoke(&self) {
        self.live.store(false, Ordering::Release);
    }
    pub fn check(&self, host: &HostProfile, d: &Deadline) -> Result<()> {
        d.check()?;
        ensure!(
            self.live.load(Ordering::Acquire),
            "required_memory_confirmation_revoked"
        );
        ensure!(
            self.binding == binding(host, d)?,
            "required_memory_confirmation_changed"
        );
        ensure!(
            self.live.load(Ordering::Acquire),
            "required_memory_confirmation_revoked"
        );
        d.check()
    }
}

pub fn admit_host(host: &HostProfile, d: &Deadline) -> Result<()> {
    match (
        &host.required_prompt_memories,
        &host.required_prompt_memory_runtime,
    ) {
        (None, None) => Ok(()),
        (Some(_), Some(runtime)) => runtime.check(host, d),
        (Some(_), None) => anyhow::bail!("required_memory_host_confirmation_required"),
        (None, Some(_)) => anyhow::bail!("required_memory_policy_required"),
    }
}

pub struct Prepared {
    pub text: String,
    pub evidence: Value,
}

/// Materialize once outside template interpretation. Existing Session policy
/// and read-only note admission must supply the checked snapshot first.
pub fn prepare(
    host: &HostProfile,
    project: &str,
    snapshot: Option<&Snapshot>,
    d: &Deadline,
) -> Result<Option<Prepared>> {
    admit_host(host, d)?;
    let Some(policy) = &host.required_prompt_memories else {
        return Ok(None);
    };
    ensure!(
        project == "default" || host.projects.iter().any(|p| p.name == project),
        "required_memory_project_unregistered"
    );
    let snapshot = snapshot.context("required_memory_snapshot_required")?;
    let names = host
        .prompt_templates
        .as_ref()
        .and_then(|c| c.memory_embeddings.as_ref())
        .context("required_memory_selection_required")?;
    snapshot.check_selection(names, d)?;
    let mut text = String::new();
    let mut selected = Vec::new();
    for pin in policy.selected(project) {
        d.check()?;
        let Entry::Available { content, revision } = snapshot
            .entries
            .get(&pin.name)
            .context("required_memory_unavailable")?
        else {
            anyhow::bail!("required_memory_unavailable")
        };
        ensure!(
            revision.revision_id == pin.revision_id
                && revision.content_sha256 == pin.content_sha256
                && revision.storage_workspace_id == pin.storage_workspace_id
                && revision.source_workspace_id == pin.source_workspace_id
                && revision.snapshot_id == pin.snapshot_id,
            "required_memory_confirmed_revision_changed"
        );
        let name = pin
            .name
            .replace('&', "&amp;")
            .replace('"', "&quot;")
            .replace('<', "&lt;")
            .replace('>', "&gt;");
        let prefix = format!(
            "\n\n<required_memory name=\"{name}\" confirmation=\"explicit_host_selection\">\n"
        );
        let suffix = "\n</required_memory>";
        let size = text
            .len()
            .checked_add(prefix.len())
            .and_then(|n| n.checked_add(content.len()))
            .and_then(|n| n.checked_add(suffix.len()))
            .context("required_memory_output_capacity")?;
        ensure!(size <= 131_072, "required_memory_output_capacity");
        text.push_str(&prefix);
        text.push_str(content);
        text.push_str(suffix);
        selected.push(pin);
    }
    let evidence = json!({"schema":VERSION,"project":project,"selectionSha256":binding(host,d)?,
        "notes":selected,"confirmation":"explicit_host_capability","humanIdentityAttested":false,
        "contentIsAuthoredClaim":true,"sourceAuthority":false,"overridesInstructionHierarchy":false});
    crate::json::encode_bounded(&evidence, MAX_POLICY, d)?;
    d.check()?;
    Ok(Some(Prepared { text, evidence }))
}
