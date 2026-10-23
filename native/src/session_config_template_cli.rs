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
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct TemplateContext {
    #[serde(default)]
    variables: Map<String, Value>,
    #[serde(default)]
    templates: BTreeMap<String, String>,
}
impl TemplateContext {
    pub(super) fn validate(&self, d: &Deadline) -> Result<()> {
        yoke::json::encode_bounded(self, MAX_CONTEXT, d)?;
        ensure!(
            self.templates.len() <= 32,
            "session_config_template_catalog_capacity"
        );
        for (name, text) in &self.templates {
            ensure!(
                !name.is_empty()
                    && name != "[native prompt]"
                    && name.len() <= 200
                    && name
                        .split('/')
                        .all(|p| !p.is_empty() && p != "." && p != "..")
                    && !name
                        .chars()
                        .any(|c| c.is_control() || matches!(c, '\\' | ':'))
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
        context.validate(d)?;
    }
    Ok(())
}
pub(super) fn project(
    base: &yoke::project_session::HostProfile,
    sources: &[Source<'_>],
    context: Option<&TemplateContext>,
    d: &Deadline,
) -> Result<session_config::Projection> {
    match context {
        None => session_config::project(base, sources, d),
        Some(context) => {
            session_config::project_with_prompt_renderer(base, sources, d, &mut |text, d| {
                render(context, text, d)
            })
        }
    }
}
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Request {
    schema: u8,
    context: TemplateContext,
    prompt: String,
}
fn render(context: &TemplateContext, text: &str, d: &Deadline) -> Result<RenderedPrompt> {
    context.validate(d)?;
    ensure!(
        text.len() <= session_config::MAX_FILE_BYTES && !text.contains('\0'),
        "session_config_template_prompt_invalid"
    );
    let request = Request {
        schema: 1,
        context: context.clone(),
        prompt: text.into(),
    };
    let input = yoke::json::encode_bounded(&request, MAX_REQUEST, d)?;
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
                stdin_bytes: MAX_REQUEST,
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
    ensure!(
        reply["schema"] == "native_session_prompt_jinja_v1"
            && reply["engine"] == ENGINE
            && reply["requestSha256"] == yoke::model::hash(&input)
            && reply["contextSha256"] == context.hash()?
            && reply["originalPromptSha256"] == yoke::model::hash(text)
            && reply["memoryCeilingBytes"] == MEMORY,
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
    let evidence = json!({"schema":"native_session_prompt_jinja_v1","engine":ENGINE,"requestSha256":reply["requestSha256"],"contextSha256":reply["contextSha256"],"memoryCeilingBytes":MEMORY,"memoryLimitKind":reply["memoryLimitKind"],"fuel":FUEL,"recursionLimit":64,"outputByteLimit":MAX_OUTPUT,"filesystemLoader":false,"environmentAccess":false,"modelCalls":0,"analyzedSourceExecuted":false,"templateWorkerExecuted":true});
    Ok(RenderedPrompt {
        text: rendered.into(),
        evidence,
    })
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
        .take(MAX_REQUEST as u64 + 1)
        .read_to_end(&mut input)?;
    let request: Request = yoke::json::parse(&input, MAX_REQUEST).and_then(yoke::json::decode)?;
    ensure!(
        request.schema == 1
            && request.prompt.len() <= session_config::MAX_FILE_BYTES
            && !request.prompt.contains('\0'),
        "session_config_template_request_invalid"
    );
    request.context.validate(deadline)?;
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
    let root_name = "[native prompt]";
    ensure!(
        !request.context.templates.contains_key(root_name),
        "session_config_template_reserved_name"
    );
    env.add_template(root_name, &request.prompt)
        .map_err(|_| anyhow::anyhow!("session_config_template_syntax_invalid"))?;
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
    let value = json!({"schema":"native_session_prompt_jinja_v1","engine":ENGINE,"requestSha256":yoke::model::hash(&input),"contextSha256":request.context.hash()?,"originalPromptSha256":yoke::model::hash(&request.prompt),"renderedSha256":yoke::model::hash(&rendered),"rendered":rendered,"memoryCeilingBytes":MEMORY,"memoryLimitKind":kind});
    let reply = yoke::json::encode_bounded(&value, cap.min(65535), deadline)?;
    let mut stdout = std::io::stdout().lock();
    stdout.write_all(&reply)?;
    stdout.write_all(b"\n")?;
    stdout.flush()?;
    Ok(true)
}
