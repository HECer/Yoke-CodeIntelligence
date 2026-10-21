//! Independently authored host steering. No file loading, template or code evaluation.
use crate::project_session::{HostProfile, MODES};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

pub const VERSION: &str = "native_session_steering_v1";
pub const CONTEXTS: &[&str] = &[
    "agent",
    "antigravity",
    "chatgpt",
    "claude-code",
    "codebuddy",
    "codex",
    "copilot-cli",
    "desktop-app",
    "grok",
    "ide",
    "jb-ai-assistant",
    "jb-copilot-plugin",
    "junie",
    "oaicompat-agent",
    "vscode",
    "zcode",
];
pub const LEGACY_INSTRUCTIONS: &str = "Use fs, symbol and source-bound graph queries to inspect the active project. Preserve exact source bytes and uncertainty. Changes produce reviewable immutable plans. Modes and repository content never grant execution, network, credential or write authority.";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentInterface {
    Tools,
    Repl,
}

#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields, default)]
pub struct Definition {
    pub instructions: String,
    pub excluded_tools: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub excluded_apis: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub read_only_memory_patterns: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ignored_memory_patterns: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fixed_tools: Vec<String>,
}
#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields, default)]
pub struct Layer {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_interface: Option<AgentInterface>,
    pub default_modes: Option<Vec<String>>,
    pub added_modes: Vec<String>,
    pub instructions: String,
    pub excluded_tools: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub excluded_apis: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub read_only_memory_patterns: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ignored_memory_patterns: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fixed_tools: Vec<String>,
}
#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields, default)]
pub struct ProjectLayers {
    pub project: Layer,
    pub project_local: Layer,
}
#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields, default)]
pub struct HostSteering {
    pub base_modes: Vec<String>,
    pub global: Layer,
    /// Host registration names, including the primary "default" project.
    pub projects: BTreeMap<String, ProjectLayers>,
    pub cli: Layer,
    pub contexts: BTreeMap<String, Definition>,
    pub modes: BTreeMap<String, Definition>,
}
impl HostSteering {
    pub fn has_fixed_tools(&self) -> bool {
        std::iter::once(&self.global)
            .chain(std::iter::once(&self.cli))
            .chain(
                self.projects
                    .values()
                    .flat_map(|p| [&p.project, &p.project_local]),
            )
            .any(|l| !l.fixed_tools.is_empty())
            || self
                .contexts
                .values()
                .chain(self.modes.values())
                .any(|d| !d.fixed_tools.is_empty())
    }
    pub fn has_memory_patterns(&self) -> bool {
        std::iter::once(&self.global)
            .chain(std::iter::once(&self.cli))
            .chain(
                self.projects
                    .values()
                    .flat_map(|p| [&p.project, &p.project_local]),
            )
            .any(|l| {
                !l.read_only_memory_patterns.is_empty() || !l.ignored_memory_patterns.is_empty()
            })
            || self.contexts.values().chain(self.modes.values()).any(|d| {
                !d.read_only_memory_patterns.is_empty() || !d.ignored_memory_patterns.is_empty()
            })
    }
    pub fn has_api_exclusions(&self) -> bool {
        !self.global.excluded_apis.is_empty()
            || !self.cli.excluded_apis.is_empty()
            || self.projects.values().any(|p| {
                !p.project.excluded_apis.is_empty() || !p.project_local.excluded_apis.is_empty()
            })
            || self
                .contexts
                .values()
                .chain(self.modes.values())
                .any(|d| !d.excluded_apis.is_empty())
    }
}
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Fragment {
    pub source: String,
    pub text: String,
}
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Resolved {
    pub version: &'static str,
    pub context: String,
    pub project: String,
    pub modes: Vec<String>,
    pub selection_source: String,
    pub base_modes: Vec<String>,
    pub added_modes: Vec<String>,
    pub excluded_tools: BTreeSet<String>,
    #[serde(skip_serializing_if = "BTreeSet::is_empty")]
    pub excluded_apis: BTreeSet<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub read_only_memory_patterns: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub ignored_memory_patterns: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub fixed_tools: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fixed_tools_source: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_interface: Option<AgentInterface>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_interface_source: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_interface_project: Option<String>,
    pub fragments: Vec<Fragment>,
    pub authority: &'static str,
}
fn label(s: &str) -> Result<()> {
    ensure!(
        !s.is_empty()
            && s.len() <= 100
            && s.bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"_.-".contains(&b)),
        "Invalid steering label"
    );
    Ok(())
}
pub(crate) fn retained_modes(values: &[String], allow_empty: bool) -> Result<()> {
    ensure!(
        (allow_empty || !values.is_empty())
            && values.len() <= 41
            && values.iter().collect::<BTreeSet<_>>().len() == values.len(),
        "Invalid steering mode selection"
    );
    for value in values {
        label(value)?;
    }
    Ok(())
}
pub fn enabled(host: &HostProfile) -> bool {
    host.agent_interface.is_some()
        || host.steering.is_some()
        || (host.context != "agent" && CONTEXTS.contains(&host.context.as_str()))
}
/// Startup project settings can override the CLI; later activation never changes the interface.
pub(crate) fn agent_interface(host: &HostProfile) -> Option<(AgentInterface, &'static str)> {
    let mut selected = host
        .agent_interface
        .map(|value| (value, "host.agentInterface"));
    if let Some(s) = &host.steering {
        for (source, layer) in [("global", &s.global), ("cli", &s.cli)] {
            if let Some(value) = layer.agent_interface {
                selected = Some((value, source));
            }
        }
        if let Some(project) = s
            .projects
            .get(host.startup_project.as_deref().unwrap_or("default"))
        {
            for (source, layer) in [
                ("project", &project.project),
                ("project-local", &project.project_local),
            ] {
                if let Some(value) = layer.agent_interface {
                    selected = Some((value, source));
                }
            }
        }
    }
    selected
}
/// Host selection only; source configuration cannot select or trust a project.
pub(crate) fn single_project_startup(host: &HostProfile) -> Option<&str> {
    matches!(host.context.as_str(), "ide" | "claude-code" | "grok")
        .then_some(host.startup_project.as_deref())
        .flatten()
}
fn known_mode(host: &HostProfile, name: &str) -> bool {
    MODES.contains(&name)
        || host
            .steering
            .as_ref()
            .is_some_and(|s| s.modes.contains_key(name))
}
fn selection(host: &HostProfile, values: &[String]) -> Result<()> {
    retained_modes(values, true)?;
    ensure!(
        values.iter().all(|s| known_mode(host, s)),
        "Unknown host steering mode"
    );
    Ok(())
}
fn tool_names() -> BTreeSet<String> {
    [
        crate::protocol::tools(),
        crate::compatibility::serena_tools(),
        crate::graphify_adapter::tools(),
        crate::trail_adapter::tools("trail"),
        crate::trail_adapter::tools("graft"),
    ]
    .iter()
    .flat_map(|v| v.as_array().into_iter().flatten())
    .filter_map(|v| v["name"].as_str().map(str::to_owned))
    .chain(
        ["ci_session", "ci_retrieve", "ci_graph"]
            .into_iter()
            .map(str::to_owned),
    )
    .collect()
}
fn definition(instructions: &str, tools: &[String], known: &BTreeSet<String>) -> Result<()> {
    ensure!(
        instructions.len() <= 8192 && !instructions.contains('\0'),
        "Steering instruction capacity exceeded"
    );
    ensure!(
        tools.len() <= known.len()
            && tools.iter().collect::<BTreeSet<_>>().len() == tools.len()
            && tools
                .iter()
                .all(|s| known.contains(s) || crate::trail_adapter::steering_identity(s).is_some()),
        "Unknown or duplicate steering tool"
    );
    Ok(())
}
fn fixed_selection(fixed: &[String], excluded: &[String], known: &BTreeSet<String>) -> Result<()> {
    ensure!(
        fixed.is_empty() || excluded.is_empty(),
        "Fixed tools cannot share a layer with excluded tools"
    );
    definition("", fixed, known)
}
fn api_selection(values: &[String]) -> Result<()> {
    if values.is_empty() {
        return Ok(());
    }
    let contract = crate::agent_repl::facade_contract();
    let methods = contract["methods"]
        .as_object()
        .expect("Owned facade methods");
    let groups: BTreeSet<_> = methods
        .keys()
        .filter_map(|s| s.split_once('.').map(|(group, _)| group))
        .collect();
    ensure!(
        values.len() <= methods.len() + groups.len()
            && values.iter().collect::<BTreeSet<_>>().len() == values.len()
            && values
                .iter()
                .all(|s| methods.contains_key(s) || groups.contains(s.as_str())),
        "Unknown or duplicate excluded REPL API"
    );
    Ok(())
}
pub(crate) fn validate(host: &HostProfile) -> Result<()> {
    if !enabled(host) {
        return Ok(());
    }
    let known = tool_names();
    if let Some(s) = &host.steering {
        ensure!(
            serde_json::to_vec(s)?.len() <= 262144
                && s.contexts.len() <= 32
                && s.modes.len() <= 32
                && s.projects.len() <= 101,
            "Host steering capacity exceeded"
        );
        for (name, d) in &s.contexts {
            label(name)?;
            ensure!(
                !CONTEXTS.contains(&name.as_str()),
                "Built-in context cannot be redefined"
            );
            definition(&d.instructions, &d.excluded_tools, &known)?;
            fixed_selection(&d.fixed_tools, &d.excluded_tools, &known)?;
            api_selection(&d.excluded_apis)?;
            crate::notes::memory_patterns(
                &d.read_only_memory_patterns,
                &d.ignored_memory_patterns,
            )?;
        }
        for (name, d) in &s.modes {
            label(name)?;
            ensure!(
                !MODES.contains(&name.as_str()),
                "Built-in mode cannot be redefined"
            );
            definition(&d.instructions, &d.excluded_tools, &known)?;
            fixed_selection(&d.fixed_tools, &d.excluded_tools, &known)?;
            api_selection(&d.excluded_apis)?;
            crate::notes::memory_patterns(
                &d.read_only_memory_patterns,
                &d.ignored_memory_patterns,
            )?;
        }
        ensure!(
            CONTEXTS.contains(&host.context.as_str()) || s.contexts.contains_key(&host.context),
            "Unknown host steering context"
        );
        selection(host, &s.base_modes)?;
        for (name, p) in &s.projects {
            ensure!(
                name == "default" || host.projects.iter().any(|p| p.name == *name),
                "Steering project is not host-registered"
            );
            label(name)?;
            for l in [&p.project, &p.project_local] {
                validate_layer(host, l, &known)?;
            }
        }
        for l in [&s.global, &s.cli] {
            validate_layer(host, l, &known)?;
        }
    }
    selection(host, &host.modes)?;
    for project in std::iter::once("default").chain(host.projects.iter().map(|p| p.name.as_str())) {
        let resolved = resolve(host, project, None)?;
        crate::notes::memory_patterns(
            &resolved.read_only_memory_patterns,
            &resolved.ignored_memory_patterns,
        )?;
    }
    Ok(())
}
fn validate_layer(host: &HostProfile, l: &Layer, known: &BTreeSet<String>) -> Result<()> {
    if let Some(modes) = &l.default_modes {
        selection(host, modes)?;
    }
    selection(host, &l.added_modes)?;
    definition(&l.instructions, &l.excluded_tools, known)?;
    fixed_selection(&l.fixed_tools, &l.excluded_tools, known)?;
    api_selection(&l.excluded_apis)?;
    crate::notes::memory_patterns(&l.read_only_memory_patterns, &l.ignored_memory_patterns)?;
    Ok(())
}
fn context_definition(name: &str) -> Definition {
    let instructions = match name {
        "desktop-app" => {
            "Use the complete native tool catalog for source inspection and reviewed plans in the desktop client."
        }
        "claude-code" => {
            "Coordinate source intelligence with the host coding agent's file and shell tools. Use semantic queries here; use the host's own file and shell operations when available."
        }
        "ide" => {
            "Coordinate semantic source queries with the editor's existing file and shell facilities."
        }
        "grok" => {
            "Coordinate semantic source queries with the host agent's existing file and shell facilities."
        }
        "chatgpt" | "oaicompat-agent" => {
            "Present source evidence and reviewable changes clearly to the conversational client."
        }
        "agent" => "Use source intelligence to support the host agent's explicitly requested task.",
        _ => {
            "Coordinate captured source evidence and reviewed plans with the selected coding client."
        }
    };
    let instructions = format!("Native context {name}. {instructions}");
    let excluded_tools = if ["claude-code", "ide", "grok"].contains(&name) {
        [
            "execute_shell_command",
            "read_file",
            "create_text_file",
            "list_dir",
            "find_file",
            "search_for_pattern",
            "replace_content",
            "replace_in_files",
            "delete_lines",
            "replace_lines",
            "insert_at_line",
        ]
        .into_iter()
        .map(str::to_owned)
        .collect()
    } else {
        Vec::new()
    };
    Definition {
        instructions,
        excluded_tools,
        excluded_apis: Vec::new(),
        ..Default::default()
    }
}
fn mode_definition(name: &str) -> Definition {
    let instructions = match name {
        "benchmark" => {
            "Keep the requested evaluation reproducible and preserve measured evidence; do not invent results."
        }
        "editing" => {
            "Prepare reviewable source changes when requested. Application still requires the independent host write and approval gates."
        }
        "interactive" => {
            "Ask for missing requirements when needed and keep the user informed during ongoing work."
        }
        "no-memories" => "Do not read or mutate application notes in this session.",
        "no-onboarding" => "Do not perform project onboarding in this session.",
        "onboarding" => {
            "Inspect captured project structure and document evidence before proposing changes."
        }
        "one-shot" => {
            "Complete the specified task with the available evidence and report unresolved requirements explicitly."
        }
        "planning" => {
            "Inspect and explain a concrete plan. Source edit preparation is disabled while planning mode is active."
        }
        "query-projects" => {
            "Use only host-registered project selectors for cross-project read queries."
        }
        _ => unreachable!("Validated built-in mode"),
    };
    let tools: &[&str] = match name {
        "no-memories" => &[
            "write_memory",
            "read_memory",
            "list_memories",
            "delete_memory",
            "rename_memory",
            "edit_memory",
        ],
        "no-onboarding" => &["onboarding"],
        "planning" => &[
            "create_text_file",
            "replace_content",
            "replace_in_files",
            "delete_lines",
            "replace_lines",
            "insert_at_line",
            "replace_symbol_body",
            "insert_after_symbol",
            "insert_before_symbol",
            "rename_symbol",
            "safe_delete_symbol",
            "jet_brains_move",
            "jet_brains_safe_delete",
            "jet_brains_inline_symbol",
            "jet_brains_rename",
        ],
        _ => &[],
    };
    Definition {
        instructions: instructions.into(),
        excluded_tools: tools.iter().map(|s| (*s).into()).collect(),
        excluded_apis: Vec::new(),
        ..Default::default()
    }
}
fn extend_unique(out: &mut Vec<String>, values: &[String]) {
    for value in values {
        if !out.contains(value) {
            out.push(value.clone());
        }
    }
}
impl Resolved {
    pub fn api_allowed(&self, method: &str) -> bool {
        !self.excluded_apis.contains(method)
            && !method
                .split_once('.')
                .is_some_and(|(group, _)| self.excluded_apis.contains(group))
    }
    pub fn allows(&self, name: &str) -> bool {
        let identity = crate::trail_adapter::steering_identity(name);
        let equivalent = |selected: &String| {
            selected == name
                || identity.is_some_and(|identity| {
                    crate::trail_adapter::steering_identity(selected) == Some(identity)
                })
        };
        !self.excluded_tools.iter().any(equivalent)
            && (self.fixed_tools.is_empty() || self.fixed_tools.iter().any(equivalent))
    }
    fn select_fixed(&mut self, source: String, tools: &[String]) {
        if !tools.is_empty() {
            self.fixed_tools = tools.to_vec();
            self.fixed_tools_source = Some(source);
        }
    }
    fn add(&mut self, source: String, instructions: &str, tools: &[String]) {
        if !instructions.is_empty() {
            self.fragments.push(Fragment {
                source,
                text: instructions.into(),
            });
        }
        self.excluded_tools.extend(tools.iter().cloned());
    }
    pub fn instructions(&self) -> String {
        let mut text = LEGACY_INSTRUCTIONS.to_owned();
        for f in &self.fragments {
            text.push_str(&format!("\n\n[host steering: {}]\n{}", f.source, f.text));
        }
        text
    }
}
pub(crate) fn resolve(
    host: &HostProfile,
    project: &str,
    requested: Option<&[String]>,
) -> Result<Resolved> {
    let empty = HostSteering::default();
    let s = host.steering.as_ref().unwrap_or(&empty);
    let p = s.projects.get(project);
    let mut layers = vec![("global", &s.global)];
    if let Some(p) = p {
        layers.extend([("project", &p.project), ("project-local", &p.project_local)]);
    }
    let mut selected = host.modes.as_slice();
    let mut selection_source = "host.modes";
    for (source, l) in layers
        .iter()
        .copied()
        .chain(std::iter::once(("cli", &s.cli)))
    {
        if let Some(modes) = &l.default_modes {
            selected = modes;
            selection_source = source;
        }
    }
    if let Some(modes) = requested {
        selected = modes;
        selection_source = "session.switchModes";
    }
    selection(host, selected)?;
    let mut out = Resolved {
        version: VERSION,
        context: host.context.clone(),
        project: project.into(),
        modes: s.base_modes.clone(),
        selection_source: selection_source.into(),
        base_modes: s.base_modes.clone(),
        added_modes: Vec::new(),
        excluded_tools: BTreeSet::new(),
        excluded_apis: BTreeSet::new(),
        read_only_memory_patterns: Vec::new(),
        ignored_memory_patterns: Vec::new(),
        fixed_tools: Vec::new(),
        fixed_tools_source: None,
        agent_interface: agent_interface(host).map(|(value, _)| value),
        agent_interface_source: agent_interface(host).map(|(_, source)| source),
        agent_interface_project: agent_interface(host)
            .map(|_| host.startup_project.as_deref().unwrap_or("default").into()),
        fragments: Vec::new(),
        authority: "advisory_instructions_and_tool_restrictions_only",
    };
    extend_unique(&mut out.modes, selected);
    for (source, l) in &layers {
        out.excluded_apis.extend(l.excluded_apis.iter().cloned());
        extend_unique(
            &mut out.read_only_memory_patterns,
            &l.read_only_memory_patterns,
        );
        extend_unique(&mut out.ignored_memory_patterns, &l.ignored_memory_patterns);
        extend_unique(&mut out.added_modes, &l.added_modes);
        out.add((*source).into(), &l.instructions, &l.excluded_tools);
        out.select_fixed((*source).into(), &l.fixed_tools);
    }
    extend_unique(&mut out.added_modes, &s.cli.added_modes);
    extend_unique(&mut out.modes, &out.added_modes);
    retained_modes(&out.modes, false)?;
    let context = s
        .contexts
        .get(&host.context)
        .cloned()
        .unwrap_or_else(|| context_definition(&host.context));
    out.excluded_apis
        .extend(context.excluded_apis.iter().cloned());
    extend_unique(
        &mut out.read_only_memory_patterns,
        &context.read_only_memory_patterns,
    );
    extend_unique(
        &mut out.ignored_memory_patterns,
        &context.ignored_memory_patterns,
    );
    out.add(
        format!("context:{}", host.context),
        &context.instructions,
        &context.excluded_tools,
    );
    out.select_fixed(format!("context:{}", host.context), &context.fixed_tools);
    for name in out.modes.clone() {
        let d = s
            .modes
            .get(&name)
            .cloned()
            .unwrap_or_else(|| mode_definition(&name));
        out.excluded_apis.extend(d.excluded_apis.iter().cloned());
        extend_unique(
            &mut out.read_only_memory_patterns,
            &d.read_only_memory_patterns,
        );
        extend_unique(&mut out.ignored_memory_patterns, &d.ignored_memory_patterns);
        out.add(format!("mode:{name}"), &d.instructions, &d.excluded_tools);
        out.select_fixed(format!("mode:{name}"), &d.fixed_tools);
    }
    out.add("cli".into(), &s.cli.instructions, &s.cli.excluded_tools);
    out.select_fixed("cli".into(), &s.cli.fixed_tools);
    out.excluded_apis
        .extend(s.cli.excluded_apis.iter().cloned());
    extend_unique(
        &mut out.read_only_memory_patterns,
        &s.cli.read_only_memory_patterns,
    );
    extend_unique(
        &mut out.ignored_memory_patterns,
        &s.cli.ignored_memory_patterns,
    );
    crate::notes::memory_pattern_limits(
        &out.read_only_memory_patterns,
        &out.ignored_memory_patterns,
    )?;
    if let Some(project) = single_project_startup(host) {
        out.excluded_tools.insert("activate_project".into());
        out.add(
            "host.startupProject".into(),
            &format!(
                "The host fixed this session to registered project {project}. Project activation is disabled; modes grant no additional permissions."
            ),
            &[],
        );
    }
    if let Some((interface, _)) = agent_interface(host) {
        out.add("host.agentInterface".into(), match interface {
            AgentInterface::Tools => "The host selected the Serena-style tools interface for this session. Source changes still require their independent host permissions and approvals.",
            AgentInterface::Repl => "The host selected the Serena-style REPL interface for this session. Python execution still requires a separately configured and explicitly trusted host runtime; API and source permissions remain independent.",
        }, &[]);
    }
    ensure!(
        out.instructions().len() <= 131072,
        "Expanded steering instruction capacity exceeded"
    );
    Ok(out)
}
