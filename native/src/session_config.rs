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
        let mut value = json!({"schema":"native_session_config_yaml_v1","hostProfile":self.profile,"sources":self.sources,"sourceBytesRewritten":false,"templatesEvaluated":false,"activationCommandsExecuted":false,"projectRegistrationsFromYaml":false,"runtimeTrustFromYaml":false,"storeOpened":false,"networkRequests":0,"modelCalls":0,"sourceExecuted":false,"complete":false});
        if self.profile.language_servers.is_some() {
            value["languageServerRegistrationsValidated"] = json!(false);
            value["languageServerRegistrationsFromYaml"] = json!(false);
        }
        if self.profile.additional_workspace_folders.is_some() {
            value["additionalWorkspaceFoldersValidated"] = json!(false);
            value["additionalWorkspaceAuthority"] = json!("existing_captured_source_only");
        }
        value
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
struct DecodedFields {
    additional_folders: Option<Option<Vec<String>>>,
    native_defaults: Map<String, Value>,
    server_settings: Option<Option<crate::server_settings::Entries>>,
    language_servers: Option<Option<Vec<String>>>,
    git_ignore: Option<Option<bool>>,
    fields: Map<String, Value>,
    base_modes: Option<Value>,
    read_only: Option<bool>,
    declared: Option<String>,
    line_ending: Option<Option<crate::source_line_endings::LineEnding>>,
    ignored_paths: Option<Vec<String>>,
    symbol_info_budget: Option<Option<u64>>,
    workspace_folders: Option<Option<Vec<String>>>,
}
fn rename_fields(
    raw: Map<String, Value>,
    definition: bool,
    global: bool,
    project: bool,
) -> Result<DecodedFields> {
    let mut mapped = Map::new();
    let mut additional_folders = None;
    let mut native_defaults = Map::new();
    let mut server_settings = None;
    let mut language_servers = None;
    let mut git_ignore = None;
    let mut base = None;
    let mut read_only = None;
    let mut declared = None;
    let mut line_ending = None;
    let mut ignored_paths = None;
    let mut symbol_info_budget = None;
    let mut workspace_folders = None;
    for (key, value) in raw {
        let name = match key.as_str() {
            "encoding" if global || project => {
                ensure!(
                    value.is_null()
                        || value
                            .as_str()
                            .is_some_and(|v| v.eq_ignore_ascii_case("utf-8")
                                || v.eq_ignore_ascii_case("utf8")),
                    "session_config_source_encoding_unavailable"
                );
                native_defaults.insert(key, value);
                continue;
            }
            "activation_command" if global || project => {
                ensure!(
                    value.is_null(),
                    "session_config_activation_command_unavailable"
                );
                native_defaults.insert(key, value);
                continue;
            }
            "activation_command_timeout" if global || project => {
                ensure!(
                    value.is_null() || value.as_u64() == Some(180),
                    "session_config_activation_timeout_unavailable"
                );
                native_defaults.insert(key, value);
                continue;
            }
            "language_backend" if global || project => {
                ensure!(
                    value.is_null() || value.as_str() == Some("LSP"),
                    "session_config_language_backend_unavailable"
                );
                native_defaults.insert(key, value);
                continue;
            }
            "ls_additional_workspace_folders" if global || project => {
                let values: Option<Vec<String>> =
                    serde_json::from_value(value.clone()).map_err(|_| {
                        anyhow::anyhow!("session_config_additional_workspace_folders_unavailable")
                    })?;
                let values = values
                    .map(|v| crate::semantic_workspace::folders(&v))
                    .transpose()?;
                if values.as_ref().is_none_or(Vec::is_empty) {
                    native_defaults.insert(key, value);
                }
                additional_folders = Some(values);
                continue;
            }
            "ls_specific_settings" if global || project => {
                let entries: Option<crate::server_settings::Entries> =
                    serde_json::from_value(value)
                        .map_err(|_| anyhow::anyhow!("session_config_server_settings_invalid"))?;
                if let Some(entries) = &entries {
                    crate::server_settings::validate_entries(entries)?;
                }
                server_settings = Some(entries);
                continue;
            }
            "ignore_all_files_in_gitignore" if global || project => {
                git_ignore =
                    Some(serde_json::from_value(value).map_err(|_| {
                        anyhow::anyhow!("session_config_git_ignore_boolean_required")
                    })?);
                continue;
            }
            "language_servers" if global || project => {
                let ids: Option<Vec<String>> = serde_json::from_value(value)
                    .map_err(|_| anyhow::anyhow!("session_config_language_servers_invalid"))?;
                if let Some(ids) = &ids {
                    crate::server_selection::validate_ids(ids)?;
                }
                language_servers = Some(ids);
                continue;
            }
            "ls_workspace_folders" if global || project => {
                let values: Option<Vec<String>> = serde_json::from_value(value)
                    .map_err(|_| anyhow::anyhow!("session_config_workspace_folders_invalid"))?;
                workspace_folders = Some(
                    values
                        .map(|v| crate::symbol_workspace::folders(&v))
                        .transpose()?,
                );
                continue;
            }
            "symbol_info_budget" if global || project => {
                symbol_info_budget = Some(crate::symbol_info_budget::milliseconds(&value)?);
                continue;
            }
            "ignored_paths" if global || project => {
                ignored_paths = serde_json::from_value(value)
                    .map_err(|_| anyhow::anyhow!("session_config_ignored_paths_invalid"))?;
                continue;
            }
            "line_ending" if global || project => {
                line_ending = Some(
                    serde_json::from_value(value)
                        .map_err(|_| anyhow::anyhow!("session_config_line_ending_invalid"))?,
                );
                continue;
            }
            "excluded_tools" => "excludedTools",
            "included_optional_tools" => "includedOptionalTools",
            "fixed_tools" => "fixedTools",
            "excluded_apis" => "excludedApis",
            "included_apis" => "includedApis",
            "read_only_memory_patterns" => "readOnlyMemoryPatterns",
            "ignored_memory_patterns" => "ignoredMemoryPatterns",
            "default_modes" if !definition => "defaultModes",
            "added_modes" if !definition => {
                mapped.insert(
                    "addedModes".into(),
                    if value.is_null() { json!([]) } else { value },
                );
                continue;
            }
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
    Ok(DecodedFields {
        additional_folders,
        native_defaults,
        server_settings,
        language_servers,
        git_ignore,
        fields: mapped,
        base_modes: base,
        read_only,
        declared,
        line_ending,
        ignored_paths,
        symbol_info_budget,
        workspace_folders,
    })
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
        let DecodedFields {
            additional_folders,
            native_defaults,
            server_settings,
            language_servers,
            git_ignore,
            fields,
            base_modes,
            read_only,
            declared,
            line_ending,
            ignored_paths,
            symbol_info_budget,
            workspace_folders,
        } = rename_fields(
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
                if project == "default" {
                    profile.primary_read_only |= read_only;
                } else {
                    let registered = profile
                        .projects
                        .iter_mut()
                        .find(|p| p.name == *project)
                        .ok_or_else(|| {
                            anyhow::anyhow!("session_config_project_not_host_registered")
                        })?;
                    registered.read_only |= read_only;
                }
            }
        }
        if let Target::Context { name } | Target::Mode { name } = source.target {
            label(name)?;
        }
        if let Some(folders) = additional_folders
            && (folders.is_some() || profile.additional_workspace_folders.is_some())
        {
            let policy = profile.additional_workspace_folders.get_or_insert_default();
            match source.target {
                Target::Global => policy.global = folders,
                Target::Project { project } => {
                    policy.projects.entry(project.clone()).or_default().project = folders
                }
                Target::ProjectLocal { project } => {
                    policy.projects.entry(project.clone()).or_default().local = folders
                }
                _ => unreachable!(),
            }
            // Keep empty layer overrides until all explicit files have been
            // applied. A later global file must not erase an earlier clear.
        }
        if let Some(entries) = server_settings
            && (entries.is_some() || profile.language_server_settings.is_some())
        {
            let policy = profile.language_server_settings.get_or_insert_default();
            match source.target {
                Target::Global => policy.global = entries,
                Target::Project { project } => {
                    policy.projects.entry(project.clone()).or_default().project = entries
                }
                Target::ProjectLocal { project } => {
                    policy.projects.entry(project.clone()).or_default().local = entries
                }
                _ => unreachable!(),
            }
            policy.canonicalize();
            if policy.empty() {
                profile.language_server_settings = None;
            }
        }
        if let Some(respect) = git_ignore
            && (respect.is_some() || profile.git_ignore.is_some())
        {
            let policy = profile.git_ignore.get_or_insert_default();
            match source.target {
                Target::Global => policy.global = respect,
                Target::Project { project } => {
                    policy.projects.entry(project.clone()).or_default().project = respect
                }
                Target::ProjectLocal { project } => {
                    policy.projects.entry(project.clone()).or_default().local = respect
                }
                _ => unreachable!(),
            }
            policy.canonicalize();
            if policy.empty() {
                profile.git_ignore = None;
            }
        }
        if let Some(ids) = language_servers
            && (ids.is_some() || profile.language_servers.is_some())
        {
            let selection = profile.language_servers.get_or_insert_default();
            match source.target {
                Target::Global => selection.global = ids,
                Target::Project { project } => {
                    selection
                        .projects
                        .entry(project.clone())
                        .or_default()
                        .project = ids
                }
                Target::ProjectLocal { project } => {
                    selection.projects.entry(project.clone()).or_default().local = ids
                }
                _ => unreachable!(),
            }
            selection.canonicalize();
            if selection.empty() {
                profile.language_servers = None;
            }
        }
        if let Some(folders) = workspace_folders
            && (folders.is_some() || profile.symbol_workspace_folders.is_some())
        {
            let policy = profile.symbol_workspace_folders.get_or_insert_default();
            match source.target {
                Target::Global => policy.global = folders,
                Target::Project { project } => {
                    policy.projects.entry(project.clone()).or_default().project = folders
                }
                Target::ProjectLocal { project } => {
                    policy.projects.entry(project.clone()).or_default().local = folders
                }
                _ => unreachable!(),
            }
            policy.canonicalize()?;
            if policy.empty() {
                profile.symbol_workspace_folders = None;
            }
        }
        if let Some(milliseconds) = symbol_info_budget
            && (milliseconds.is_some() || profile.symbol_info_budgets.is_some())
        {
            let policy = profile.symbol_info_budgets.get_or_insert_default();
            match source.target {
                Target::Global => policy.global_ms = milliseconds,
                Target::Project { project } => {
                    policy
                        .projects
                        .entry(project.clone())
                        .or_default()
                        .project_ms = milliseconds
                }
                Target::ProjectLocal { project } => {
                    policy.projects.entry(project.clone()).or_default().local_ms = milliseconds
                }
                _ => unreachable!(),
            }
            policy.canonicalize();
            if policy.empty() {
                profile.symbol_info_budgets = None;
            }
        }
        if let Some(patterns) = ignored_paths
            && !patterns.is_empty()
        {
            let policy = profile.source_exclusions.get_or_insert_default();
            match source.target {
                Target::Global => policy.global.push(patterns),
                Target::Project { project } => policy
                    .projects
                    .entry(project.clone())
                    .or_default()
                    .project
                    .push(patterns),
                Target::ProjectLocal { project } => policy
                    .projects
                    .entry(project.clone())
                    .or_default()
                    .local
                    .push(patterns),
                _ => unreachable!(),
            }
        }
        if let Some(ending) = line_ending
            && (ending.is_some() || profile.source_line_endings.is_some())
        {
            let policy = profile.source_line_endings.get_or_insert_default();
            match source.target {
                Target::Global => policy.global = ending,
                Target::Project { project } => {
                    policy.projects.entry(project.clone()).or_default().project = ending
                }
                Target::ProjectLocal { project } => {
                    policy.projects.entry(project.clone()).or_default().local = ending
                }
                _ => unreachable!(),
            }
            if policy.empty() {
                profile.source_line_endings = None;
            }
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
        let mut receipt = json!({"target":source.target,"originalBytes":source.bytes.len(),"originalSha256":source.expected_sha256,"declaredProjectName":declared,"declaredProjectNameIsRegistration":false});
        if !native_defaults.is_empty() {
            receipt["nativeEquivalentDefaults"] = Value::Object(native_defaults);
            receipt["nativeDefaultCapabilities"] = json!({"sourceEncodings":["utf-8"],"activationCommands":false,"languageBackends":["LSP"],"additionalWorkspaceFolders":true,"additionalWorkspaceFolderScope":"captured_relative_only"});
        }
        receipts.push(receipt);
    }
    deadline.check()?;
    if let Some(folders) = &profile.additional_workspace_folders {
        folders.validate(&profile)?;
    }
    if let Some(folders) = &mut profile.additional_workspace_folders {
        folders.canonicalize()?;
        if folders.empty() {
            profile.additional_workspace_folders = None;
        }
    }
    if let Some(settings) = &profile.language_server_settings {
        settings.validate(&profile)?;
    }
    if let Some(policy) = &profile.git_ignore {
        policy.validate(&profile)?;
    }
    if let Some(endings) = &profile.source_line_endings {
        endings.validate(&profile)?;
    }
    if let Some(exclusions) = &profile.source_exclusions {
        exclusions.validate(&profile, deadline)?;
    }
    if let Some(selection) = &profile.language_servers {
        selection.validate(&profile)?;
    }
    if let Some(folders) = &profile.symbol_workspace_folders {
        folders.validate(&profile)?;
    }
    if let Some(budgets) = &profile.symbol_info_budgets {
        budgets.validate(&profile)?;
    }
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
