//! Explicit host projection of pinned YAML data. No filesystem or execution authority.
use crate::{
    capture::Deadline,
    model,
    project_session::HostProfile,
    session_steering::{self, Definition, Layer},
};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use std::collections::BTreeSet;
#[path = "session_config_yaml.rs"]
mod yaml;
pub const MAX_FILES: usize = 268;
pub const MAX_FILE_BYTES: usize = 65536;
pub const MAX_TOTAL_BYTES: usize = 2_000_000;
pub const MAX_PROFILE_BYTES: usize = 128000;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Target {
    Global,
    Project { project: String },
    ProjectLocal { project: String },
    Context { name: String },
    Mode { name: String },
}
pub struct Source<'a> {
    pub target: &'a Target,
    pub expected_sha256: &'a str,
    pub bytes: &'a [u8],
}
pub struct Projection {
    profile: HostProfile,
    sources: Vec<Value>,
}
impl Projection {
    pub fn profile(&self) -> &HostProfile {
        &self.profile
    }
    pub fn profile_bytes(&self, deadline: &Deadline) -> Result<Vec<u8>> {
        crate::json::encode_bounded(&self.profile, MAX_PROFILE_BYTES, deadline)
    }
    pub fn plan(&self) -> Value {
        json!({"schema":"native_session_config_yaml_v1","hostProfile":self.profile,"sources":self.sources,"sourceBytesRewritten":false,"templatesEvaluated":false,"activationCommandsExecuted":false,"projectRegistrationsFromYaml":false,"runtimeTrustFromYaml":false,"storeOpened":false,"networkRequests":0,"modelCalls":0,"sourceExecuted":false,"complete":false})
    }
}
fn label(value: &str) -> Result<()> {
    ensure!(
        !value.is_empty()
            && value.len() <= 100
            && value
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"_-.".contains(&b)),
        "session_config_target_invalid"
    );
    Ok(())
}
type DecodedFields = (
    Map<String, Value>,
    Option<Value>,
    Option<bool>,
    Option<String>,
);
fn rename_fields(
    raw: Map<String, Value>,
    definition: bool,
    global: bool,
    project: bool,
) -> Result<DecodedFields> {
    let mut mapped = Map::new();
    let mut base = None;
    let mut read_only = None;
    let mut declared = None;
    for (key, value) in raw {
        let name = match key.as_str() {
            "excluded_tools" => "excludedTools",
            "included_optional_tools" => "includedOptionalTools",
            "fixed_tools" => "fixedTools",
            "excluded_apis" => "excludedApis",
            "included_apis" => "includedApis",
            "read_only_memory_patterns" => "readOnlyMemoryPatterns",
            "ignored_memory_patterns" => "ignoredMemoryPatterns",
            "default_modes" if !definition => "defaultModes",
            "added_modes" if !definition => "addedModes",
            "agent_interface" if !definition => "agentInterface",
            "initial_prompt" if project => "instructions",
            "prompt" if definition => "instructions",
            "base_modes" if global => {
                base = Some(value);
                continue;
            }
            "read_only" if project => {
                read_only =
                    Some(value.as_bool().ok_or_else(|| {
                        anyhow::anyhow!("session_config_read_only_boolean_required")
                    })?);
                continue;
            }
            "project_name" if project => {
                let name = value
                    .as_str()
                    .ok_or_else(|| anyhow::anyhow!("session_config_project_name_invalid"))?;
                ensure!(
                    !name.is_empty() && name.len() <= 100 && !name.contains('\0'),
                    "session_config_project_name_invalid"
                );
                declared = Some(name.to_owned());
                continue;
            }
            _ => anyhow::bail!("session_config_setting_unsupported"),
        };
        if name == "instructions" {
            let text = value
                .as_str()
                .ok_or_else(|| anyhow::anyhow!("session_config_prompt_string_required"))?;
            ensure!(
                !["{{", "{%", "{#"].iter().any(|s| text.contains(s)),
                "session_config_template_evaluation_unavailable"
            );
        }
        mapped.insert(name.into(), value);
    }
    Ok((mapped, base, read_only, declared))
}
/// Project only explicitly supplied values. Files cannot supply registrations or grants.
pub fn project(
    base: &HostProfile,
    sources: &[Source<'_>],
    deadline: &Deadline,
) -> Result<Projection> {
    deadline.check()?;
    ensure!(
        sources.len() <= MAX_FILES,
        "session_config_file_capacity_exceeded"
    );
    crate::json::encode_bounded(base, MAX_PROFILE_BYTES, deadline)?;
    let mut profile = base.clone();
    let mut seen = BTreeSet::new();
    let mut receipts = Vec::new();
    let mut total = 0usize;
    for source in sources {
        deadline.check()?;
        total = total
            .checked_add(source.bytes.len())
            .ok_or_else(|| anyhow::anyhow!("session_config_source_capacity_exceeded"))?;
        ensure!(
            source.bytes.len() <= MAX_FILE_BYTES && total <= MAX_TOTAL_BYTES,
            "session_config_source_capacity_exceeded"
        );
        ensure!(
            model::valid_id(source.expected_sha256)
                && model::hash(source.bytes) == source.expected_sha256,
            "session_config_source_changed"
        );
        ensure!(
            seen.insert(serde_json::to_string(source.target)?),
            "session_config_duplicate_target"
        );
        let is_project = matches!(
            source.target,
            Target::Project { .. } | Target::ProjectLocal { .. }
        );
        let definition = matches!(source.target, Target::Context { .. } | Target::Mode { .. });
        let (fields, base_modes, read_only, declared) = rename_fields(
            yaml::decode(source.bytes, deadline)?,
            definition,
            matches!(source.target, Target::Global),
            is_project,
        )?;
        if let Target::Project { project } | Target::ProjectLocal { project } = source.target {
            label(project)?;
            ensure!(
                project == "default" || profile.projects.iter().any(|p| p.name == *project),
                "session_config_project_not_host_registered"
            );
            if let Some(read_only) = read_only {
                let registered = profile
                    .projects
                    .iter_mut()
                    .find(|p| p.name == *project)
                    .ok_or_else(|| {
                        anyhow::anyhow!("session_config_primary_read_only_unavailable")
                    })?;
                registered.read_only |= read_only;
            }
        }
        if let Target::Context { name } | Target::Mode { name } = source.target {
            label(name)?;
        }
        let has_fields = !fields.is_empty();
        let has_base = base_modes.is_some();
        if definition {
            let value: Definition = serde_json::from_value(Value::Object(fields))
                .map_err(|_| anyhow::anyhow!("session_config_definition_schema_invalid"))?;
            let steering = profile.steering.get_or_insert_default();
            match source.target {
                Target::Context { name } => {
                    steering.contexts.insert(name.clone(), value);
                }
                Target::Mode { name } => {
                    steering.modes.insert(name.clone(), value);
                }
                _ => unreachable!(),
            }
        } else if has_fields || has_base {
            let value: Layer = serde_json::from_value(Value::Object(fields))
                .map_err(|_| anyhow::anyhow!("session_config_layer_schema_invalid"))?;
            let steering = profile.steering.get_or_insert_default();
            match source.target {
                Target::Global => {
                    steering.global = value;
                    if let Some(base) = base_modes {
                        steering.base_modes = serde_json::from_value(base)
                            .map_err(|_| anyhow::anyhow!("session_config_base_modes_invalid"))?;
                    }
                }
                Target::Project { project } => {
                    steering
                        .projects
                        .entry(project.clone())
                        .or_default()
                        .project = value;
                }
                Target::ProjectLocal { project } => {
                    steering
                        .projects
                        .entry(project.clone())
                        .or_default()
                        .project_local = value;
                }
                _ => unreachable!(),
            }
        }
        receipts.push(json!({"target":source.target,"originalBytes":source.bytes.len(),"originalSha256":source.expected_sha256,"declaredProjectName":declared,"declaredProjectNameIsRegistration":false}));
    }
    deadline.check()?;
    session_steering::validate(&profile)
        .map_err(|_| anyhow::anyhow!("session_config_steering_invalid"))?;
    let projection = Projection {
        profile,
        sources: receipts,
    };
    projection.profile_bytes(deadline)?;
    Ok(projection)
}
#[cfg(test)]
#[path = "session_config_tests.rs"]
mod tests;
