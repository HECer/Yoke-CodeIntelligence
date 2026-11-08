//! Explicit host prompt data and caller-owned renderer capability; no file I/O.
use crate::{
    capture::Deadline,
    model,
    session_config::{self, RenderedPrompt},
};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::{collections::BTreeMap, path::PathBuf, sync::Arc};
#[path = "session_prompt_catalog.rs"]
mod catalog;
pub(crate) use catalog::resolve as resolve_catalog;
#[path = "session_prompt_memory.rs"]
pub mod memory;
pub struct Instructions {
    pub text: String,
    pub evidence: Option<Value>,
}
pub const VERSION: &str = "native_session_prompt_runtime_v1";
pub const MAX_CONTEXT: usize = 128000;
#[derive(Clone, Copy, Default, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Profile {
    #[default]
    Native,
    Serena,
    Graphify,
    Trail,
    Graft,
}
pub const RUNTIME_VARIABLES: &[&str] = &["available_tools", "available_markers", "tool_names"];
#[derive(Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CollisionPolicy {
    #[default]
    Error,
    LastWins,
}
impl CollisionPolicy {
    fn is_error(&self) -> bool {
        *self == Self::Error
    }
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CatalogFile {
    pub path: PathBuf,
    pub sha256: String,
}
#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TemplateContext {
    #[serde(default)]
    pub variables: Map<String, Value>,
    #[serde(default)]
    pub templates: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub catalogs: Vec<CatalogFile>,
    #[serde(default, skip_serializing_if = "CollisionPolicy::is_error")]
    pub collision_policy: CollisionPolicy,
}
impl TemplateContext {
    /// Pure data admission. The host separately owns path/read and render grants.
    pub fn validate_data(&self, d: &Deadline) -> Result<()> {
        crate::json::encode_bounded(self, MAX_CONTEXT, d)?;
        ensure!(
            self.templates.len() <= 32,
            "session_config_template_catalog_capacity"
        );
        ensure!(
            self.catalogs.len() <= session_config::MAX_CATALOG_FILES,
            "session_config_catalog_file_capacity"
        );
        for (name, text) in &self.templates {
            ensure!(
                session_config::prompt_name(name).is_ok()
                    && text.len() <= session_config::MAX_FILE_BYTES
                    && !text.contains('\0'),
                "session_config_template_catalog_invalid"
            );
        }
        for file in &self.catalogs {
            ensure!(
                model::valid_id(&file.sha256),
                "session_config_catalog_source_changed"
            );
        }
        Ok(())
    }
}
#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Config {
    #[serde(default)]
    pub context: TemplateContext,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub system_prompt: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub memory_embeddings: Option<Vec<String>>,
}
impl Config {
    pub fn validate(&self, d: &Deadline) -> Result<()> {
        crate::json::encode_bounded(self, MAX_CONTEXT, d)?;
        self.context.validate_data(d)?;
        ensure!(
            RUNTIME_VARIABLES
                .iter()
                .all(|name| !self.context.variables.contains_key(*name)),
            "session_prompt_reserved_runtime_variable"
        );
        if let Some(name) = &self.system_prompt {
            session_config::prompt_name(name)?;
        }
        if let Some(names) = &self.memory_embeddings {
            memory::selection(names)?;
            ensure!(
                !self.context.variables.contains_key("embed_memory"),
                "session_prompt_reserved_memory_function"
            );
        }
        Ok(())
    }
    pub fn fingerprint(&self) -> Result<String> {
        let original = model::hash(serde_json::to_vec(&(
            VERSION,
            model::hash(include_bytes!("session_prompt_markers.json")),
            self,
        ))?);
        Ok(if self.memory_embeddings.is_some() {
            model::hash(serde_json::to_vec(&(original, memory::VERSION))?)
        } else {
            original
        })
    }
}
/// Values derived by the current transport/session, never copied from prompt data.
pub struct RenderInput {
    pub prompt: String,
    pub template_name: Option<String>,
    pub source: String,
    pub variables: Map<String, Value>,
    pub binding: Value,
    pub memories: Option<Arc<memory::Snapshot>>,
}
pub type Renderer = dyn Fn(&RenderInput, &Deadline) -> Result<RenderedPrompt> + Send + Sync;
/// Non-serializable explicit trusted-host authority. The callback owns its limits
/// and evidence. JSON cannot construct a callback or select an executable.
#[derive(Clone)]
pub struct Runtime {
    selection: String,
    render: Arc<Renderer>,
}
impl Runtime {
    pub fn trusted(config: &Config, render: Arc<Renderer>, d: &Deadline) -> Result<Self> {
        config.validate(d)?;
        Ok(Self {
            selection: config.fingerprint()?,
            render,
        })
    }
    pub fn check(&self, config: &Config, d: &Deadline) -> Result<()> {
        d.check()?;
        config.validate(d)?;
        ensure!(
            self.selection == config.fingerprint()?,
            "session_prompt_runtime_binding_changed"
        );
        Ok(())
    }
    pub fn render(
        &self,
        config: &Config,
        input: &RenderInput,
        d: &Deadline,
    ) -> Result<RenderedPrompt> {
        self.check(config, d)?;
        ensure!(
            input.prompt.len() <= session_config::MAX_FILE_BYTES && !input.prompt.contains('\0'),
            "session_prompt_input_invalid"
        );
        ensure!(
            input.source.len() <= 512 && !input.source.chars().any(char::is_control),
            "session_prompt_source_invalid"
        );
        crate::json::encode_bounded(&input.variables, MAX_CONTEXT, d)?;
        crate::json::encode_bounded(&input.binding, MAX_CONTEXT, d)?;
        match (&config.memory_embeddings, &input.memories) {
            (Some(names), Some(snapshot)) => {
                snapshot.check_selection(names, d)?;
                ensure!(
                    input.binding["memorySnapshotSha256"] == snapshot.hash()?,
                    "session_prompt_memory_binding_changed"
                );
            }
            (None, None) => {}
            _ => anyhow::bail!("session_prompt_memory_selection_changed"),
        }
        let result = (self.render)(input, d)?;
        d.check()?;
        ensure!(
            result.text.len() <= 8192 && !result.text.contains('\0'),
            "session_prompt_rendered_capacity_or_encoding"
        );
        crate::json::encode_bounded(&result.evidence, 65536, d)?;
        Ok(result)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn shared_context_preserves_preceding_inline_canonical_bytes_and_catalog_defaults() {
        let context: TemplateContext =
            serde_json::from_str(r#"{"variables":{"x":"owned"},"templates":{"entry":"{{ x }}"}}"#)
                .unwrap();
        assert_eq!(
            serde_json::to_string(&context).unwrap(),
            r#"{"variables":{"x":"owned"},"templates":{"entry":"{{ x }}"}}"#
        );
        context.validate_data(&Deadline::new(10000)).unwrap();
        let value = serde_json::to_value(TemplateContext::default()).unwrap();
        assert_eq!(value, serde_json::json!({"variables":{},"templates":{}}));
    }
    #[test]
    fn runtime_selection_cannot_spoof_derived_variables_or_rebind_an_existing_callback() {
        let d = Deadline::new(10000);
        let config = Config::default();
        let runtime = Runtime::trusted(
            &config,
            Arc::new(|_, _| {
                Ok(RenderedPrompt {
                    text: "owned".into(),
                    evidence: Value::Null,
                })
            }),
            &d,
        )
        .unwrap();
        let mut changed = config.clone();
        changed
            .context
            .variables
            .insert("custom".into(), Value::Bool(true));
        assert!(runtime.check(&changed, &d).is_err());
        for name in RUNTIME_VARIABLES {
            let mut spoof = config.clone();
            spoof.context.variables.insert((*name).into(), Value::Null);
            assert!(
                spoof
                    .validate(&d)
                    .unwrap_err()
                    .to_string()
                    .contains("session_prompt_reserved_runtime_variable")
            );
            spoof.context.validate_data(&d).unwrap();
        }
    }
}
