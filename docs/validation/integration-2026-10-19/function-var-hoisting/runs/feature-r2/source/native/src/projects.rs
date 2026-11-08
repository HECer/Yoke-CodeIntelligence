//! Host-selected compiler on an immutable virtual filesystem. This optional
//! semantic backend requires Node and TypeScript; native indexing does not.
use crate::{
    capture::Deadline,
    model::{Snapshot, hash},
    process::{CaptureLimits, run_capture},
    store::Store,
};
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use std::{collections::BTreeMap, fs, path::PathBuf, process::Command};
#[derive(Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CompilerSpec {
    pub node: PathBuf,
    pub typescript: PathBuf,
    pub trusted: bool,
}
pub fn analyze(
    store: &Store,
    snapshot: &Snapshot,
    spec: &CompilerSpec,
    deadline: &Deadline,
) -> Result<Value> {
    execute(store, snapshot, spec, "analyze", None, deadline)
}
pub fn build(
    store: &Store,
    snapshot: &Snapshot,
    spec: &CompilerSpec,
    deadline: &Deadline,
) -> Result<Value> {
    execute(store, snapshot, spec, "build", None, deadline)
}
pub(crate) fn execute(
    store: &Store,
    snapshot: &Snapshot,
    spec: &CompilerSpec,
    operation: &str,
    intent: Option<Value>,
    deadline: &Deadline,
) -> Result<Value> {
    ensure!(
        spec.trusted,
        "Compiler execution requires explicit host trust"
    );
    ensure!(
        spec.node.is_absolute() && spec.typescript.is_absolute(),
        "Compiler executable and module must be absolute host paths"
    );
    ensure!(
        snapshot.workspace_id == store.workspace_id && snapshot.kind != "federated",
        "Compiler snapshot scope mismatch"
    );
    store.set_deadline(deadline)?;
    let persisted = store.snapshot(&snapshot.id)?;
    ensure!(
        serde_json::to_vec(&persisted)? == serde_json::to_vec(snapshot)?,
        "Compiler source manifest mismatch"
    );
    let node = spec
        .node
        .canonicalize()
        .context("Host Node executable unavailable")?;
    let compiler = spec
        .typescript
        .canonicalize()
        .context("Host TypeScript compiler unavailable")?;
    ensure!(
        node.is_file() && compiler.is_file(),
        "Compiler host paths must be files"
    );
    let compiler = compiler
        .to_str()
        .context("Compiler path must be UTF-8")?
        .to_owned();
    #[cfg(windows)]
    let compiler = if let Some(unc) = compiler.strip_prefix(r"\\?\UNC\") {
        format!(r"\\{unc}")
    } else {
        compiler
            .strip_prefix(r"\\?\")
            .unwrap_or(&compiler)
            .to_owned()
    };
    let root = store
        .dir
        .parent()
        .context("Workspace root unavailable")?
        .canonicalize()?;
    ensure!(
        !node.starts_with(&root),
        "Compiler executable must be outside the analyzed workspace"
    );
    let mut contents = BTreeMap::new();
    let mut bindings = BTreeMap::new();
    let mut bytes = 0usize;
    ensure!(
        snapshot.files.len() <= 20_000,
        "Compiler input file capacity exceeded"
    );
    for file in &snapshot.files {
        deadline.check()?;
        bytes = bytes
            .checked_add(file.size)
            .context("Compiler input size overflow")?;
        ensure!(
            bytes <= 10_000_000,
            "Compiler immutable input exceeds 10 MB"
        );
        let text = String::from_utf8(store.read_blob_exact(&file.hash, file.size, 10_000_000)?)?;
        contents.insert(file.path.clone(), text);
        bindings.insert(file.path.clone(), file.hash.clone());
    }
    let mut input = json!({"schema":1,"operation":operation,"snapshotId":snapshot.id,"compiler":compiler,"contents":contents,"sourceBindings":bindings});
    if let Some(intent) = intent {
        input["intent"] = intent;
    }
    let payload = serde_json::to_vec(&input)?;
    let input_hash = hash(&payload);
    let dir = tempfile::Builder::new()
        .prefix("yoke-compiler-")
        .tempdir()?;
    let worker = dir.path().join("compiler.cjs");
    fs::write(
        &worker,
        [
            include_str!("compiler_worker.cjs"),
            include_str!("compiler_inline_worker.cjs"),
            include_str!("compiler_inline_body_worker.cjs"),
            include_str!("compiler_inline_body_vars_worker.cjs"),
            include_str!("compiler_move_worker.cjs"),
            include_str!("compiler_move_repairs_worker.cjs"),
        ]
        .join("\n"),
    )?;
    let mut command = Command::new(node);
    command
        .arg("--max-old-space-size=512")
        .arg(&worker)
        .arg(&input_hash)
        .current_dir(dir.path());
    let capture = run_capture(
        command,
        &payload,
        deadline,
        CaptureLimits {
            stdin_bytes: 16_000_000,
            stdout_bytes: 32_000_000,
            stderr_bytes: 1_000_000,
        },
    )?;
    ensure!(
        capture.exit_code == Some(0) && capture.output_complete && capture.stdin_complete,
        "Compiler worker failed, exceeded bounds or timed out: {}",
        String::from_utf8_lossy(&capture.stderr)
            .chars()
            .take(2000)
            .collect::<String>()
    );
    let mut report: Value = crate::json::parse(&capture.stdout, 32_000_000)
        .context("Invalid compiler worker response")?;
    ensure!(
        report["schema"] == 1
            && report["operation"] == operation
            && report["snapshotId"] == snapshot.id
            && report["inputHash"] == input_hash
            && report["compiler"] == "5.8.3",
        "Compiler response identity mismatch"
    );
    ensure!(
        report["sourceBindings"] == serde_json::to_value(&bindings)?,
        "Compiler response source binding mismatch"
    );
    ensure!(
        report["sourceExecuted"] == false && report["sourceTreeWritten"] == false,
        "Compiler response authority mismatch"
    );
    for context in report["contexts"]
        .as_array()
        .context("Compiler contexts missing")?
    {
        for binding in context["bindings"]
            .as_array()
            .context("Compiler bindings missing")?
        {
            validate_span(binding, &contents)?;
            for definition in binding["definitions"]
                .as_array()
                .context("Compiler definitions missing")?
            {
                validate_span(definition, &contents)?;
            }
        }
    }
    for diagnostic in report["diagnostics"]
        .as_array()
        .context("Compiler diagnostics missing")?
    {
        if diagnostic["text"].is_string() {
            validate_span(diagnostic, &contents)?;
        }
    }
    for output in report["outputs"]
        .as_array()
        .context("Compiler outputs missing")?
    {
        let path = output["path"]
            .as_str()
            .context("Compiler output path missing")?;
        crate::paths::normalized(path)?;
        ensure!(
            !contents.contains_key(path),
            "Compiler output conflicts with an immutable input"
        );
        let text = output["content"]
            .as_str()
            .context("Compiler output content missing")?;
        ensure!(
            output["hash"] == hash(text) && output["bytes"] == text.len(),
            "Compiler output integrity mismatch"
        );
    }
    deadline.check()?;
    if matches!(operation, "inline" | "move" | "inlineBody") {
        // No artifact, derived source or plan is published before independent
        // effect admission by the compiler-inline adapter.
        return Ok(report);
    }
    let artifact = store.artifact(&serde_json::to_vec(&report)?)?;
    if let Some(outputs) = report["outputs"].as_array_mut() {
        for output in outputs {
            output
                .as_object_mut()
                .context("Compiler output must be an object")?
                .remove("content");
        }
    }
    report["artifactId"] = json!(artifact);
    report["captureOmissions"] = json!(snapshot.omissions);
    report["exhaustive"] = json!(false);
    store.record("native_compiler_report_v1",&artifact,&json!({"snapshotId":snapshot.id,"artifactId":artifact,"operation":operation,"inputHash":input_hash}))?;
    Ok(report)
}
fn validate_span(span: &Value, contents: &BTreeMap<String, String>) -> Result<()> {
    let path = span["path"]
        .as_str()
        .context("Compiler span path missing")?;
    let text = contents
        .get(path)
        .context("Compiler span outside snapshot")?;
    let start = span["startByte"]
        .as_u64()
        .context("Compiler span start missing")? as usize;
    let end = span["endByte"]
        .as_u64()
        .context("Compiler span end missing")? as usize;
    ensure!(
        start <= end
            && end <= text.len()
            && text.is_char_boundary(start)
            && text.is_char_boundary(end)
            && span["text"] == text[start..end],
        "Compiler span does not match immutable source"
    );
    Ok(())
}
