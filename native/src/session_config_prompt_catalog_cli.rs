//! Guarded host catalog resolution, preserving every original declaration.
use super::super::{Loaded, loaded_identity, read};
use super::ResolvedContext;
use anyhow::{Result, ensure};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
pub(in crate::session_config_cli) use yoke::session_prompt::{
    CatalogFile, CollisionPolicy, TemplateContext,
};
use yoke::{capture::Deadline, session_config};
pub(super) fn validate_context(context: &TemplateContext, d: &Deadline) -> Result<()> {
    context.validate_data(d)?;
    for file in &context.catalogs {
        super::super::super::meaning_archive_cli::host_path(&file.path)?;
    }
    Ok(())
}
pub(in crate::session_config_cli) struct PreparedContext {
    pub(super) data: ResolvedContext,
    pub(super) schema: u8,
    metadata: Option<Value>,
    loaded: Vec<Loaded>,
}
impl PreparedContext {
    pub(super) fn runtime(data: ResolvedContext, metadata: Option<Value>) -> Self {
        Self {
            data,
            schema: 3,
            metadata,
            loaded: Vec::new(),
        }
    }
    pub(super) fn recheck(&self, d: &Deadline) -> Result<()> {
        super::super::recheck(&self.loaded, d)
    }
    pub(in crate::session_config_cli) fn metadata(&self) -> Option<&Value> {
        self.metadata.as_ref()
    }
    pub(in crate::session_config_cli) fn into_loaded(self) -> Vec<Loaded> {
        self.loaded
    }
    pub(super) fn resolution_hash(&self) -> Result<Option<String>> {
        self.metadata
            .as_ref()
            .map(|v| serde_json::to_vec(v).map(yoke::model::hash))
            .transpose()
            .map_err(Into::into)
    }
}
fn declaration(name: &str, text: &str, origin: Value, previous: Option<usize>) -> Value {
    let mut value = json!({"name":name,"decodedPromptSha256":yoke::model::hash(text),"decodedPromptBytes":text.len(),"origin":origin,"effective":false,"sourceByteSpansAvailable":false});
    if let Some(previous) = previous {
        value["replacesDeclaration"] = json!(previous);
    }
    value
}
fn add(
    data: &mut ResolvedContext,
    origins: &mut BTreeMap<String, usize>,
    declarations: &mut Vec<Value>,
    name: String,
    text: String,
    origin: Value,
    policy: &CollisionPolicy,
) -> Result<()> {
    ensure!(
        declarations.len() < session_config::MAX_PROMPT_DECLARATIONS,
        "session_config_prompt_declaration_capacity"
    );
    let previous = origins.get(&name).copied();
    ensure!(
        previous.is_none() || *policy == CollisionPolicy::LastWins,
        "session_config_prompt_name_collision"
    );
    if previous.is_none() {
        ensure!(
            data.templates.len() < session_config::MAX_EFFECTIVE_PROMPTS,
            "session_config_effective_prompt_capacity"
        );
    }
    let index = declarations.len();
    if let Some(previous) = previous {
        declarations[previous]["replacedByDeclaration"] = json!(index);
    }
    declarations.push(declaration(&name, &text, origin, previous));
    origins.insert(name.clone(), index);
    data.templates.insert(name, text);
    Ok(())
}
pub(in crate::session_config_cli) fn prepare(
    context: &TemplateContext,
    policy_bytes: usize,
    d: &Deadline,
) -> Result<PreparedContext> {
    prepare_loaded(context, policy_bytes, d, None)
}
pub(in crate::session_config_cli) fn prepare_loaded(
    context: &TemplateContext,
    policy_bytes: usize,
    d: &Deadline,
    preloaded: Option<Vec<Loaded>>,
) -> Result<PreparedContext> {
    validate_context(context, d)?;
    ensure!(
        policy_bytes <= session_config::MAX_TOTAL_BYTES,
        "session_config_source_capacity_exceeded"
    );
    let mut data = ResolvedContext {
        variables: context.variables.clone(),
        templates: BTreeMap::new(),
    };
    let mut origins = BTreeMap::new();
    let mut declarations = Vec::new();
    let mut sources = Vec::new();
    let mut loaded = Vec::new();
    let mut identities = BTreeSet::new();
    let mut total = policy_bytes;
    for (name, text) in &context.templates {
        add(
            &mut data,
            &mut origins,
            &mut declarations,
            name.clone(),
            text.clone(),
            json!({"kind":"inline_context"}),
            &context.collision_policy,
        )?;
    }
    let mut preloaded = preloaded.map(Vec::into_iter);
    for (index, file) in context.catalogs.iter().enumerate() {
        d.check()?;
        let source = match &mut preloaded {
            Some(values) => values
                .next()
                .ok_or_else(|| anyhow::anyhow!("session_config_catalog_loaded_mismatch"))?,
            None => read(&file.path, session_config::MAX_FILE_BYTES, d)?,
        };
        ensure!(
            source.path == file.path,
            "session_config_catalog_loaded_mismatch"
        );
        total = total
            .checked_add(source.bytes.len())
            .ok_or_else(|| anyhow::anyhow!("session_config_source_capacity_exceeded"))?;
        ensure!(
            total <= session_config::MAX_TOTAL_BYTES,
            "session_config_source_capacity_exceeded"
        );
        ensure!(
            identities.insert(loaded_identity(&source)?),
            "session_config_catalog_source_alias"
        );
        let prompts = session_config::decode_prompt_catalog(&source.bytes, &file.sha256, d)?;
        sources.push(json!({"path":file.path,"originalBytes":source.bytes.len(),"originalSha256":file.sha256,"promptNames":prompts.keys().collect::<Vec<_>>(),"promptCount":prompts.len(),"sourceByteSpansAvailable":false}));
        for (name, text) in prompts {
            add(
                &mut data,
                &mut origins,
                &mut declarations,
                name,
                text,
                json!({"kind":"catalog_file","sourceIndex":index}),
                &context.collision_policy,
            )?;
        }
        loaded.push(source);
    }
    ensure!(
        preloaded
            .as_mut()
            .is_none_or(|values| values.next().is_none()),
        "session_config_catalog_loaded_mismatch"
    );
    for index in origins.values() {
        declarations[*index]["effective"] = json!(true);
    }
    let catalog_mode = !context.catalogs.is_empty();
    let schema = if catalog_mode { 2 } else { 1 };
    data.validate(schema, d)?;
    let metadata = if catalog_mode || context.collision_policy != CollisionPolicy::Error {
        Some(
            json!({"schema":"native_prompt_catalog_resolution_v1","selectionSha256":yoke::model::hash(serde_json::to_vec(context)?),"collisionPolicy":context.collision_policy,"sources":sources,"declarations":declarations,"effectivePromptCount":data.templates.len(),"declarationCount":declarations.len(),"sourceBytesIncludingPolicy":total,"definitionOrder":"inline_then_selected_files_each_name_sorted","complete":false}),
        )
    } else {
        None
    };
    Ok(PreparedContext {
        data,
        schema,
        metadata,
        loaded,
    })
}
