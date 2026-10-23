//! Pure prompt templates in a dedicated, OS-memory-bounded native worker.
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use std::{
    collections::BTreeMap,
    io::{Read, Write},
    process::Command,
    sync::{Arc, atomic::AtomicBool},
};
use yoke::{
    capture::Deadline,
    session_config::{self, RenderedPrompt, Source},
};
const MEMORY: usize = 256 * 1024 * 1024;
const FUEL: u64 = 100_000;
const MAX_CONTEXT: usize = 128_000;
const MAX_REQUEST: usize = 256_000;
const MAX_OUTPUT: usize = 8192;
const ENGINE: &str = "minijinja-3.0.3";
#[path = "session_config_prompt_catalog_cli.rs"]
mod catalogs;
pub(super) use catalogs::{
    CatalogFile, PreparedContext, TemplateContext, prepare as prepare_context,
    prepare_loaded as prepare_loaded_context,
};
const MAX_CATALOG_REQUEST: usize = 8_000_000;
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ResolvedContext {
    #[serde(default)]
    variables: Map<String, Value>,
    #[serde(default)]
    templates: BTreeMap<String, String>,
}
impl ResolvedContext {
    fn validate(&self, schema: u8, d: &Deadline) -> Result<()> {
        ensure!(
            matches!(schema, 1..=3),
            "session_config_template_request_invalid"
        );
        if schema == 1 {
            yoke::json::encode_bounded(self, MAX_CONTEXT, d)?;
        } else {
            yoke::json::encode_bounded(&self.variables, MAX_CONTEXT, d)?;
            yoke::json::encode_bounded(self, MAX_CATALOG_REQUEST, d)?;
        }
        ensure!(
            self.templates.len()
                <= if schema == 1 {
                    32
                } else {
                    session_config::MAX_EFFECTIVE_PROMPTS
                },
            "session_config_template_catalog_capacity"
        );
        for (name, text) in &self.templates {
            ensure!(
                session_config::prompt_name(name).is_ok()
                    && text.len() <= session_config::MAX_FILE_BYTES
                    && !text.contains('\0'),
                "session_config_template_catalog_invalid"
            );
        }
        Ok(())
    }
    fn hash(&self) -> Result<String> {
        Ok(yoke::model::hash(serde_json::to_vec(self)?))
    }
}
pub(super) fn plan(
    projection: &session_config::Projection,
    context: Option<&PreparedContext>,
) -> Value {
    let mut value = projection.plan();
    if let Some(metadata) = context.and_then(PreparedContext::metadata) {
        value["promptCatalogResolution"] = metadata.clone();
    }
    value
}
pub(super) fn admit(context: Option<&TemplateContext>, allowed: bool, d: &Deadline) -> Result<()> {
    ensure!(
        context.is_none() || allowed,
        "session_config_template_permission_required"
    );
    ensure!(
        !allowed || context.is_some(),
        "session_config_template_context_required"
    );
    if let Some(context) = context {
        catalogs::validate_context(context, d)?;
    }
    Ok(())
}
pub(super) fn deferred_context(
    context: Option<&TemplateContext>,
    system_prompt: Option<&str>,
    defer: bool,
    allowed: bool,
    d: &Deadline,
) -> Result<Option<yoke::session_prompt::Config>> {
    ensure!(
        system_prompt.is_none() || defer,
        "session_config_system_prompt_requires_deferred_templates"
    );
    ensure!(
        !defer || allowed,
        "session_config_template_permission_required"
    );
    if !defer {
        return Ok(None);
    }
    let context =
        context.ok_or_else(|| anyhow::anyhow!("session_config_template_context_required"))?;
    let config = yoke::session_prompt::Config {
        context: context.clone(),
        system_prompt: system_prompt.map(str::to_owned),
    };
    config.validate(d)?;
    Ok(Some(config))
}
pub(super) fn project(
    base: &yoke::project_session::HostProfile,
    sources: &[Source<'_>],
    context: Option<&PreparedContext>,
    deferred: Option<&yoke::session_prompt::Config>,
    d: &Deadline,
) -> Result<session_config::Projection> {
    if let Some(config) = deferred {
        let context =
            context.ok_or_else(|| anyhow::anyhow!("session_config_template_context_required"))?;
        if let Some(name) = &config.system_prompt {
            ensure!(
                context.data.templates.contains_key(name),
                "session_config_prompt_name_unavailable"
            );
        }
        return session_config::project_for_session_templates(base, sources, config, d);
    }
    ensure!(
        context.is_none() || base.prompt_templates.is_none(),
        "session_config_template_mode_ambiguous"
    );
    match context {
        None => session_config::project(base, sources, d),
        Some(context) => {
            session_config::project_with_prompt_renderer(base, sources, d, &mut |text, d| {
                render(context, text, None, None, d)
            })
        }
    }
}
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Request {
    schema: u8,
    context: ResolvedContext,
    prompt: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    template_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    runtime_binding: Option<Value>,
}
pub(super) fn render_named(
    context: &PreparedContext,
    name: &str,
    d: &Deadline,
) -> Result<RenderedPrompt> {
    session_config::prompt_name(name)?;
    let text = context
        .data
        .templates
        .get(name)
        .ok_or_else(|| anyhow::anyhow!("session_config_prompt_name_unavailable"))?;
    render(context, text, Some(name), None, d)
}
fn render(
    context: &PreparedContext,
    text: &str,
    name: Option<&str>,
    binding: Option<&Value>,
    d: &Deadline,
) -> Result<RenderedPrompt> {
    let schema = if binding.is_some() {
        3
    } else if name.is_some() {
        2
    } else {
        context.schema
    };
    context.data.validate(schema, d)?;
    ensure!(
        text.len() <= session_config::MAX_FILE_BYTES && !text.contains('\0'),
        "session_config_template_prompt_invalid"
    );
    let request = Request {
        schema,
        context: context.data.clone(),
        prompt: text.into(),
        template_name: name.map(str::to_owned),
        runtime_binding: binding.cloned(),
    };
    let request_cap = if schema == 1 {
        MAX_REQUEST
    } else {
        MAX_CATALOG_REQUEST
    };
    let input = yoke::json::encode_bounded(&request, request_cap, d)?;
    let private = tempfile::tempdir()?;
    let mut command = Command::new(std::env::current_exe()?);
    command.current_dir(private.path()).args([
        "--timeout",
        "10000",
        "--max-bytes",
        "65536",
        "session-config",
        "render-worker",
    ]);
    for name in ["HOME", "USERPROFILE", "TEMP", "TMP"] {
        command.env(name, private.path());
    }
    for name in ["PATH", "RUSTUP_HOME", "CARGO_HOME"] {
        command.env_remove(name);
    }
    let output = Deadline::run_bounded(d, Arc::new(AtomicBool::new(false)), || {
        yoke::process::run_capture_with_memory_limit(
            command,
            &input,
            &Deadline::new(10_000),
            yoke::process::CaptureLimits {
                stdin_bytes: request_cap,
                stdout_bytes: 65536,
                stderr_bytes: 8192,
            },
            MEMORY,
        )
    })?;
    ensure!(
        output.exit_code == Some(0)
            && output.stdin_complete
            && output.output_complete
            && !output.timed_out
            && !output.output_exceeded
            && !output.io_error,
        "session_config_template_worker_failed"
    );
    let reply = yoke::json::parse(&output.stdout, 65536)?;
    let reply_schema = reply_schema(schema);
    ensure!(
        reply["schema"] == reply_schema
            && reply["engine"] == ENGINE
            && reply["requestSha256"] == yoke::model::hash(&input)
            && reply["contextSha256"] == context.data.hash()?
            && reply["originalPromptSha256"] == yoke::model::hash(text)
            && reply["memoryCeilingBytes"] == MEMORY,
        "session_config_template_worker_binding_invalid"
    );
    ensure!(
        reply.get("entryTemplate").and_then(Value::as_str) == name,
        "session_config_template_worker_binding_invalid"
    );
    let binding_hash = binding
        .map(|v| serde_json::to_vec(v).map(yoke::model::hash))
        .transpose()?;
    ensure!(
        reply.get("runtimeBindingSha256").and_then(Value::as_str) == binding_hash.as_deref(),
        "session_config_template_worker_binding_invalid"
    );
    let rendered = reply["rendered"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("session_config_template_worker_output_invalid"))?;
    ensure!(
        rendered.len() <= MAX_OUTPUT
            && !rendered.contains('\0')
            && reply["renderedSha256"] == yoke::model::hash(rendered),
        "session_config_template_worker_output_invalid"
    );
    let mut evidence = json!({"schema":reply_schema,"engine":ENGINE,"requestSha256":reply["requestSha256"],"contextSha256":reply["contextSha256"],"memoryCeilingBytes":MEMORY,"memoryLimitKind":reply["memoryLimitKind"],"fuel":FUEL,"recursionLimit":64,"outputByteLimit":MAX_OUTPUT,"filesystemLoader":false,"environmentAccess":false,"modelCalls":0,"analyzedSourceExecuted":false,"templateWorkerExecuted":true});
    if let Some(hash) = binding_hash {
        evidence["runtimeBindingSha256"] = json!(hash);
    }
    if let Some(hash) = context.resolution_hash()? {
        evidence["promptCatalogResolutionSha256"] = json!(hash);
    }
    if let Some(name) = name {
        evidence["entryTemplate"] = json!(name);
    }
    Ok(RenderedPrompt {
        text: rendered.into(),
        evidence,
    })
}
fn reply_schema(schema: u8) -> &'static str {
    match schema {
        1 => "native_session_prompt_jinja_v1",
        2 => "native_session_prompt_jinja_v2",
        3 => "native_session_prompt_jinja_v3",
        _ => unreachable!("Validated own worker schema"),
    }
}
pub(super) fn bind_runtime(
    host: &mut yoke::project_session::HostProfile,
    allowed: bool,
    d: &Deadline,
) -> Result<()> {
    let Some(config) = host.prompt_templates.as_ref() else {
        return Ok(());
    };
    ensure!(allowed, "session_prompt_host_permission_required");
    config.validate(d)?;
    let prepared = Arc::new(prepare_context(&config.context, 0, d)?);
    if let Some(name) = &config.system_prompt {
        ensure!(
            prepared.data.templates.contains_key(name),
            "session_config_prompt_name_unavailable"
        );
    }
    host.prompt_runtime = Some(yoke::session_prompt::Runtime::trusted(
        config,
        Arc::new(move |input, d| {
            prepared.recheck(d)?;
            let mut data = prepared.data.clone();
            for (name, value) in &input.variables {
                ensure!(
                    yoke::session_prompt::RUNTIME_VARIABLES.contains(&name.as_str()),
                    "session_prompt_runtime_variable_invalid"
                );
                data.variables.insert(name.clone(), value.clone());
            }
            ensure!(
                input.binding.is_object(),
                "session_prompt_runtime_binding_invalid"
            );
            let mut binding = input.binding.clone();
            binding["source"] = json!(input.source);
            let text = if let Some(name) = &input.template_name {
                prepared
                    .data
                    .templates
                    .get(name)
                    .ok_or_else(|| anyhow::anyhow!("session_config_prompt_name_unavailable"))?
                    .as_str()
            } else {
                input.prompt.as_str()
            };
            let merged = PreparedContext::runtime(data, prepared.metadata().cloned());
            let rendered = render(
                &merged,
                text,
                input.template_name.as_deref(),
                Some(&binding),
                d,
            )?;
            prepared.recheck(d)?;
            Ok(rendered)
        }),
        d,
    )?);
    Ok(())
}
fn data(value: &Value) -> minijinja::Value {
    match value {
        Value::Null => minijinja::Value::from(()),
        Value::Bool(v) => (*v).into(),
        Value::String(v) => v.clone().into(),
        Value::Number(v) => {
            if let Some(v) = v.as_i64() {
                v.into()
            } else if let Some(v) = v.as_u64() {
                v.into()
            } else {
                v.as_f64().unwrap().into()
            }
        }
        Value::Array(v) => v.iter().map(data).collect(),
        Value::Object(v) => {
            minijinja::Value::from_pairs(v.iter().map(|(k, v)| (k.as_str(), data(v))))
        }
    }
}
struct BoundedOutput<'a> {
    bytes: Vec<u8>,
    deadline: &'a Deadline,
}
impl Write for BoundedOutput<'_> {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.deadline
            .check()
            .map_err(|_| std::io::Error::other("template deadline"))?;
        if self.bytes.len().saturating_add(bytes.len()) > MAX_OUTPUT {
            return Err(std::io::Error::other("template output capacity"));
        }
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
pub(super) fn worker(host: &Deadline, cap: usize) -> Result<bool> {
    Deadline::run_bounded(host, Arc::new(AtomicBool::new(false)), || {
        worker_inner(&Deadline::new(10_000), cap)
    })
}
fn worker_inner(deadline: &Deadline, cap: usize) -> Result<bool> {
    #[cfg(windows)]
    {
        #[link(name = "kernel32")]
        unsafe extern "system" {
            fn SetErrorMode(mode: u32) -> u32;
        }
        // Dedicated worker resource failures must not open an interactive dialog.
        unsafe {
            SetErrorMode(0x8003);
        }
    }
    // This runs before reading even the bounded input. Parent capture also sets
    // its own ceiling before exec. A direct worker call cannot omit this limit.
    let _memory = yoke::process::limit_current_memory(MEMORY)?;
    ensure!(
        yoke::process::current_memory_limit()? == Some(MEMORY),
        "session_config_template_memory_limit_unproven"
    );
    deadline.check()?;
    let mut input = Vec::new();
    std::io::stdin()
        .lock()
        .take(MAX_CATALOG_REQUEST as u64 + 1)
        .read_to_end(&mut input)?;
    let frame = yoke::json::parse(&input, MAX_CATALOG_REQUEST)?;
    let runtime_binding_present = frame.get("runtimeBinding").is_some();
    let request: Request = yoke::json::decode(frame)?;
    ensure!(
        matches!(request.schema, 1..=3)
            && input.len()
                <= if request.schema == 1 {
                    MAX_REQUEST
                } else {
                    MAX_CATALOG_REQUEST
                }
            && request.prompt.len() <= session_config::MAX_FILE_BYTES
            && !request.prompt.contains('\0'),
        "session_config_template_request_invalid"
    );
    request.context.validate(request.schema, deadline)?;
    ensure!(
        request.template_name.is_none()
            || (request.schema >= 2
                && request.template_name.as_ref().is_some_and(|name| request
                    .context
                    .templates
                    .get(name)
                    == Some(&request.prompt))),
        "session_config_template_entry_binding_invalid"
    );
    ensure!(
        runtime_binding_present == (request.schema == 3),
        "session_config_template_runtime_protocol_invalid"
    );
    if request.schema == 3 {
        let binding = request
            .runtime_binding
            .as_ref()
            .filter(|v| v.is_object())
            .ok_or_else(|| anyhow::anyhow!("session_config_template_runtime_binding_invalid"))?;
        yoke::json::encode_bounded(binding, MAX_CONTEXT, deadline)?;
    }
    let mut env = minijinja::Environment::new();
    env.set_auto_escape_callback(|_| minijinja::AutoEscape::None);
    env.set_undefined_behavior(minijinja::UndefinedBehavior::Strict);
    env.set_fuel(Some(FUEL));
    env.set_recursion_limit(64);
    env.set_syntax(
        minijinja::syntax::SyntaxConfig::builder()
            .keep_trailing_newline(true)
            .build()?,
    );
    for (name, text) in &request.context.templates {
        env.add_template(name, text)
            .map_err(|_| anyhow::anyhow!("session_config_template_catalog_syntax_invalid"))?;
    }
    // A unique name cannot shadow or import a selected catalog entry.
    let root_name = request
        .template_name
        .as_deref()
        .unwrap_or("[native prompt]");
    ensure!(
        !request.context.templates.contains_key("[native prompt]"),
        "session_config_template_reserved_name"
    );
    if request.template_name.is_none() {
        env.add_template(root_name, &request.prompt)
            .map_err(|_| anyhow::anyhow!("session_config_template_syntax_invalid"))?;
    }
    let mut output = BoundedOutput {
        bytes: Vec::new(),
        deadline,
    };
    env.get_template(root_name)?
        .render_captured_to(
            data(&Value::Object(request.context.variables.clone())),
            &mut output,
        )
        .map_err(|_| anyhow::anyhow!("session_config_template_render_failed"))?;
    deadline.check()?;
    let rendered = String::from_utf8(output.bytes)?;
    ensure!(
        !rendered.contains('\0'),
        "session_config_template_rendered_nul"
    );
    let kind = if cfg!(windows) {
        "windows_job_committed_bytes"
    } else {
        "unix_address_space_bytes"
    };
    let reply_schema = reply_schema(request.schema);
    let mut value = json!({"schema":reply_schema,"engine":ENGINE,"requestSha256":yoke::model::hash(&input),"contextSha256":request.context.hash()?,"originalPromptSha256":yoke::model::hash(&request.prompt),"renderedSha256":yoke::model::hash(&rendered),"rendered":rendered,"memoryCeilingBytes":MEMORY,"memoryLimitKind":kind});
    if let Some(binding) = &request.runtime_binding {
        value["runtimeBindingSha256"] = json!(yoke::model::hash(serde_json::to_vec(binding)?));
    }
    if let Some(name) = request.template_name {
        value["entryTemplate"] = json!(name);
    }
    let reply = yoke::json::encode_bounded(&value, cap.min(65535), deadline)?;
    let mut stdout = std::io::stdout().lock();
    stdout.write_all(&reply)?;
    stdout.write_all(b"\n")?;
    stdout.flush()?;
    Ok(true)
}
