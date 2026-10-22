use crate::{
    capture::Deadline,
    process::{CleanupUnproven, ManagedChild},
    project_session::Session,
};
use anyhow::{Context, Result, bail, ensure};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    fs,
    io::{BufRead, BufReader, Read, Write},
    path::PathBuf,
    process::{Command, Stdio},
    sync::mpsc::{self, Receiver, Sender, SyncSender},
    thread,
    time::{Duration, Instant},
};
const MAX_FRAME: usize = 4_000_000;
type WorkerInput = SyncSender<(Vec<u8>, Sender<Result<()>>)>;

#[cfg(windows)]
fn admit_windows_python_image(executable: &std::path::Path) -> Result<()> {
    use std::os::windows::ffi::OsStrExt;
    // Explicit batch launchers are handled by the existing owned process path.
    if executable
        .extension()
        .and_then(|s| s.to_str())
        .is_some_and(|s| s.eq_ignore_ascii_case("cmd") || s.eq_ignore_ascii_case("bat"))
    {
        return Ok(());
    }
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetBinaryTypeW(application: *const u16, binary_type: *mut u32) -> i32;
    }
    let path = executable
        .canonicalize()
        .context("Selected Python image is unavailable")?;
    let path: Vec<u16> = path
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    ensure!(
        path.len() <= 32768 && !path[..path.len() - 1].contains(&0),
        "Invalid selected Python image path"
    );
    let mut kind = 0u32;
    // Classification reads executable metadata; it does not launch selected bytes.
    // SAFETY: path is live, NUL-terminated UTF-16; kind is writable for the call.
    let classified = unsafe { GetBinaryTypeW(path.as_ptr(), &mut kind) };
    ensure!(
        classified != 0 && matches!(kind, 0 | 6),
        "Selected Python host requires a Windows PE executable or an explicit batch launcher"
    );
    Ok(())
}

#[derive(Clone, Debug)]
pub struct HostShell {
    pub command: PathBuf,
    pub arguments: Vec<String>,
}
#[derive(Clone, Debug)]
pub struct ReplSpec {
    pub python: PathBuf,
    pub trust_python: bool,
    pub max_output_bytes: usize,
    pub shell: Option<HostShell>,
}
pub struct AgentRepl {
    spec: ReplSpec,
    child: Option<ManagedChild>,
    input: Option<WorkerInput>,
    messages: Option<Receiver<Result<Value>>>,
    _directory: tempfile::TempDir,
    next: u64,
    workers: Vec<thread::JoinHandle<()>>,
    process_cleanup_unproven: Option<String>,
}

fn receive_cooperatively<T>(receiver: &Receiver<T>, deadline: &Deadline) -> Result<T> {
    loop {
        deadline.check()?;
        match receiver.recv_timeout(deadline.remaining().min(Duration::from_millis(25))) {
            Ok(value) => {
                deadline.check()?;
                return Ok(value);
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => bail!("Agent REPL channel closed"),
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Callback {
    event: String,
    request_id: u64,
    call_id: u64,
    method: String,
    arguments: Value,
    project: Option<String>,
}

pub fn facade_contract() -> Value {
    serde_json::from_str(include_str!("agent_repl_contract.json"))
        .expect("Owned facade contract must be valid JSON")
}

/// Invoke a declared source/session facade without Python execution authority.
/// Shell execution and worker-only Python regex/context-manager operations are
/// explicit unavailable boundaries of this host-independent adapter.
pub fn invoke_facade(
    session: &mut Session,
    method: &str,
    arguments: Value,
    project: Option<&str>,
    deadline: &Deadline,
) -> Result<Value> {
    strict_facade(method, &arguments)?;
    ensure!(
        !matches!(
            method,
            "lsp.find_declaration" | "jb.find_declaration" | "ext.project_context"
        ),
        "This facade operation requires the explicitly authorized persistent Python worker"
    );
    let spec = ReplSpec {
        python: PathBuf::new(),
        trust_python: false,
        max_output_bytes: MAX_FRAME,
        shell: None,
    };
    dispatch(session, &spec, method, &arguments, project, deadline)
}
fn read_frame(input: &mut impl BufRead) -> Result<Value> {
    let mut bytes = Vec::new();
    loop {
        let available = input.fill_buf()?;
        ensure!(
            !available.is_empty(),
            "Agent REPL worker closed its protocol"
        );
        let end = available
            .iter()
            .position(|byte| *byte == b'\n')
            .map(|at| at + 1);
        let take = end.unwrap_or(available.len());
        ensure!(
            bytes.len().saturating_add(take) <= MAX_FRAME,
            "Agent REPL frame budget exceeded"
        );
        bytes.extend_from_slice(&available[..take]);
        input.consume(take);
        if end.is_some() {
            break;
        }
    }
    crate::json::parse(&bytes, MAX_FRAME)
}
impl AgentRepl {
    /// Pure host preflight. This does not start Python or execute shell code.
    pub fn validate_spec(spec: &ReplSpec) -> Result<()> {
        ensure!(
            spec.trust_python,
            "Persistent agent REPL requires explicit trusted Python host execution"
        );
        ensure!(
            spec.python.is_absolute() && spec.python.is_file(),
            "Agent REPL requires an existing absolute host-selected Python executable"
        );
        ensure!(
            (100..=MAX_FRAME).contains(&spec.max_output_bytes),
            "Invalid agent REPL output budget"
        );
        if let Some(shell) = &spec.shell {
            ensure!(
                shell.command.is_absolute()
                    && shell.command.is_file()
                    && shell.arguments.len() <= 100
                    && shell.arguments.iter().map(String::len).sum::<usize>() <= 128000,
                "Invalid explicit host shell profile"
            );
        }
        Ok(())
    }
    pub fn is_alive(&self) -> bool {
        self.child.is_some()
    }
    pub fn spawn(spec: ReplSpec, deadline: &Deadline) -> Result<Self> {
        deadline.check()?;
        Self::validate_spec(&spec)?;
        #[cfg(windows)]
        admit_windows_python_image(&spec.python)?;
        deadline.check()?;
        let directory = tempfile::Builder::new()
            .prefix("yoke-agent-repl-")
            .tempdir()?;
        let worker = directory.path().join("worker.py");
        fs::write(&worker, include_str!("agent_repl_worker.py"))?;
        let mut command = Command::new(&spec.python);
        command
            .args(["-I", "-B", "-u"])
            .arg(&worker)
            .current_dir(directory.path())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .env_clear();
        for name in [
            "PATH",
            "SystemRoot",
            "SystemDrive",
            "WINDIR",
            "TEMP",
            "TMP",
            "LANG",
        ] {
            if let Some(value) = std::env::var_os(name) {
                command.env(name, value);
            }
        }
        command.env("DO_NOT_TRACK", "1").env("NO_COLOR", "1");
        let mut child = ManagedChild::spawn(command)
            .context("Unable to start the selected agent REPL worker")?;
        let mut stdin = child.stdin().take().context("Agent REPL input missing")?;
        let stdout = child.stdout().take().context("Agent REPL output missing")?;
        let mut stderr = child.stderr().take().context("Agent REPL stderr missing")?;
        let (input, writes) = mpsc::sync_channel::<(Vec<u8>, Sender<Result<()>>)>(8);
        let mut workers = Vec::new();
        workers.push(thread::spawn(move || {
            while let Ok((bytes, done)) = writes.recv() {
                let result = (|| -> Result<()> {
                    stdin.write_all(&bytes)?;
                    stdin.flush()?;
                    Ok(())
                })();
                let failed = result.is_err();
                let _ = done.send(result);
                if failed {
                    break;
                }
            }
        }));
        let (sender, messages) = mpsc::sync_channel(8);
        let errors = sender.clone();
        workers.push(thread::spawn(move || {
            let mut reader = BufReader::new(stdout);
            loop {
                let value = read_frame(&mut reader);
                let failed = value.is_err();
                if sender.send(value).is_err() || failed {
                    break;
                }
            }
        }));
        workers.push(thread::spawn(move || {
            let mut buffer = [0u8; 4096];
            let mut bytes = 0usize;
            while let Ok(count) = stderr.read(&mut buffer) {
                if count == 0 {
                    break;
                }
                bytes = bytes.saturating_add(count);
                if bytes > 1_000_000 {
                    let _ = errors.send(Err(anyhow::anyhow!(
                        "Agent REPL unmanaged stderr budget exceeded"
                    )));
                    break;
                }
            }
        }));
        let mut repl = Self {
            spec,
            child: Some(child),
            input: Some(input),
            messages: Some(messages),
            _directory: directory,
            next: 0,
            workers,
            process_cleanup_unproven: None,
        };
        let result = (|| -> Result<()> {
            repl.send(&json!({"event":"boot","methods":facade_contract()["methods"],"maxOutputBytes":repl.spec.max_output_bytes}),deadline)?;
            let ready = repl.receive(deadline)?;
            ensure!(
                ready["event"] == "ready"
                    && ready["schemaVersion"] == 1
                    && ready["facadeMethods"] == 50,
                "Agent REPL handshake mismatch"
            );
            Ok(())
        })();
        if let Err(error) = result {
            if let Err(cleanup) = repl.close() {
                return Err(cleanup.context(format!(
                    "Agent REPL initialization failed ({error}); cleanup also failed"
                )));
            }
            return Err(error);
        }
        Ok(repl)
    }
    fn send(&mut self, value: &Value, deadline: &Deadline) -> Result<()> {
        deadline.check()?;
        let mut bytes = serde_json::to_vec(value)?;
        bytes.push(b'\n');
        ensure!(
            bytes.len() <= MAX_FRAME,
            "Agent REPL outgoing frame budget exceeded"
        );
        let (done, completed) = mpsc::channel();
        self.input
            .as_ref()
            .context("Agent REPL state was lost")?
            .try_send((bytes, done))
            .map_err(|_| anyhow::anyhow!("Agent REPL input queue is blocked or closed"))?;
        receive_cooperatively(&completed, deadline)
            .context("Agent REPL input exceeded shared deadline")??;
        Ok(())
    }
    fn receive(&mut self, deadline: &Deadline) -> Result<Value> {
        deadline.check()?;
        receive_cooperatively(
            self.messages
                .as_ref()
                .context("Agent REPL state was lost")?,
            deadline,
        )
        .context("Agent REPL worker exceeded shared deadline")?
    }
    pub fn execute(
        &mut self,
        session: &mut Session,
        code: &str,
        deadline: &Deadline,
    ) -> Result<Value> {
        self.execute_owned(session, code, deadline, true)
    }
    /// Only independently validated, host-generated tool-adapter cells use this path.
    pub(crate) fn execute_tool(
        &mut self,
        session: &mut Session,
        code: &str,
        deadline: &Deadline,
    ) -> Result<Value> {
        self.execute_owned(session, code, deadline, false)
    }
    fn execute_owned(
        &mut self,
        session: &mut Session,
        code: &str,
        deadline: &Deadline,
        apply_api_selection: bool,
    ) -> Result<Value> {
        ensure!(
            self.child.is_some(),
            "Agent REPL state was lost; open a new worker"
        );
        ensure!(
            !code.is_empty() && code.len() <= 128000 && !code.contains('\0'),
            "Invalid agent REPL code budget"
        );
        let result = self.execute_inner(session, code, deadline, apply_api_selection);
        if let Err(error) = result {
            if let Err(cleanup) = self.close() {
                return Err(cleanup.context(format!(
                    "Agent REPL operation failed ({error}); cleanup also failed"
                )));
            }
            return Err(error);
        }
        result
    }
    fn execute_inner(
        &mut self,
        session: &mut Session,
        code: &str,
        deadline: &Deadline,
        apply_api_selection: bool,
    ) -> Result<Value> {
        self.next += 1;
        let request_id = self.next;
        let mut callbacks = 0usize;
        let mut last_call_id = 0u64;
        let mut request = json!({"event":"execute","requestId":request_id,"code":code});
        if apply_api_selection {
            session.check_repl_state(deadline)?;
            if let Some(methods) = session.selected_repl_methods()? {
                request["selectedMethods"] = json!(methods);
            }
        }
        self.send(&request, deadline)?;
        loop {
            let message = self.receive(deadline)?;
            ensure!(
                message["requestId"] == request_id,
                "Agent REPL response request identity mismatch"
            );
            match message["event"].as_str() {
                Some("call") => {
                    let call: Callback = serde_json::from_value(message)?;
                    callbacks += 1;
                    ensure!(
                        callbacks <= 2000
                            && call.event == "call"
                            && call.request_id == request_id
                            && call.call_id > last_call_id,
                        "Agent REPL callback identity/count budget exceeded"
                    );
                    last_call_id = call.call_id;
                    let response = if apply_api_selection {
                        dispatch_repl(
                            session,
                            &self.spec,
                            &call.method,
                            &call.arguments,
                            call.project.as_deref(),
                            deadline,
                        )
                    } else {
                        dispatch(
                            session,
                            &self.spec,
                            &call.method,
                            &call.arguments,
                            call.project.as_deref(),
                            deadline,
                        )
                    };
                    let reply = match response {
                        Ok(value) => {
                            json!({"event":"reply","requestId":request_id,"callId":call.call_id,"result":value,"error":null})
                        }
                        Err(error) => {
                            json!({"event":"reply","requestId":request_id,"callId":call.call_id,"result":null,"error":error.to_string().chars().take(2000).collect::<String>()})
                        }
                    };
                    self.send(&reply, deadline)?;
                }
                Some("result") => {
                    ensure!(
                        message
                            .as_object()
                            .is_some_and(|object| object.keys().all(|key| [
                                "event",
                                "requestId",
                                "result",
                                "stdout",
                                "stderr",
                                "error"
                            ]
                            .contains(&key.as_str()))),
                        "Unknown agent REPL result field"
                    );
                    let stdout = message["stdout"]
                        .as_str()
                        .context("Agent REPL stdout missing")?;
                    let stderr = message["stderr"]
                        .as_str()
                        .context("Agent REPL stderr missing")?;
                    ensure!(
                        stdout.len().saturating_add(stderr.len()) <= self.spec.max_output_bytes
                            && serde_json::to_vec(&message["result"])?.len()
                                <= self.spec.max_output_bytes,
                        "Agent REPL result/output budget exceeded"
                    );
                    deadline.check()?;
                    return Ok(
                        json!({"requestId":request_id,"result":message["result"],"stdout":stdout,"stderr":stderr,"error":message["error"],"callbackCount":callbacks,"stateLost":false,"runtime":"persistent_trusted_python","programHostEffects":"not_monitored","facadeEdits":"immutable_guarded_plans","osSandbox":false}),
                    );
                }
                Some("fatal") => bail!(
                    "Agent REPL worker rejected the cell: {}",
                    message["message"]
                ),
                _ => bail!("Unexpected agent REPL worker event"),
            }
        }
    }
    pub fn close(&mut self) -> Result<()> {
        self.close_with_budget(Duration::from_secs(1))
    }
    fn close_with_budget(&mut self, process_budget: Duration) -> Result<()> {
        let end = Instant::now() + Duration::from_secs(2);
        self.input.take();
        self.messages.take();
        let termination = self
            .child
            .take()
            .map(|child| child.terminate_checked(process_budget))
            .transpose();
        let mut worker_failed = false;
        while !self.workers.is_empty() && Instant::now() < end {
            for index in (0..self.workers.len()).rev() {
                if self.workers[index].is_finished() {
                    worker_failed |= self.workers.swap_remove(index).join().is_err();
                }
            }
            if !self.workers.is_empty() {
                thread::sleep(Duration::from_millis(10));
            }
        }
        if let Err(error) = termination {
            self.process_cleanup_unproven
                .get_or_insert_with(|| error.to_string().chars().take(2000).collect());
        }
        if let Some(detail) = &self.process_cleanup_unproven {
            return Err(CleanupUnproven::new(detail.clone()).into());
        }
        ensure!(
            self.workers.is_empty() && !worker_failed,
            "Agent REPL worker cleanup could not be proven"
        );
        Ok(())
    }
}
impl Drop for AgentRepl {
    fn drop(&mut self) {
        if (self.child.is_some() || !self.workers.is_empty()) && self.close().is_err() {
            eprintln!("Agent REPL cleanup could not be proven; host recovery required");
        }
    }
}

#[cfg(all(test, windows))]
mod cleanup_proof_tests {
    use super::*;
    #[test]
    fn lost_process_cleanup_proof_stays_typed_across_repeated_close() {
        let output = Command::new("python")
            .args(["-I", "-c", "import sys;print(sys.executable)"])
            .output()
            .unwrap();
        assert!(output.status.success());
        let spec = ReplSpec {
            python: String::from_utf8(output.stdout).unwrap().trim().into(),
            trust_python: true,
            max_output_bytes: 1000,
            shell: None,
        };
        let mut runtime = AgentRepl::spawn(spec, &Deadline::new(10_000)).unwrap();
        // A zero cleanup budget requests termination but deliberately refuses
        // to certify exit. Only this private unit gate selects that budget.
        let first = runtime.close_with_budget(Duration::ZERO).unwrap_err();
        let repeated = runtime.close();
        assert!(
            repeated.is_err(),
            "Lost consuming-child proof must not become successful cleanup on retry; first error: {first}"
        );
        assert!(crate::process::cleanup_unproven(&first));
        assert!(crate::process::cleanup_unproven(
            &repeated.unwrap_err().context("HTTP initialization failed")
        ));
        assert!(!runtime.is_alive());
    }
}

fn unwrap_native(value: Value) -> Result<Value> {
    if value.get("status").is_some() {
        ensure!(
            value["status"] != "error",
            "Native facade operation failed: {}",
            value["error"]
        );
        if value["status"] == "partial" {
            return Ok(value);
        }
        let mut data = value["data"].clone();
        if data.is_object() && data.get("snapshotId").is_none() && value["snapshotId"].is_string() {
            data["snapshotId"] = value["snapshotId"].clone();
        }
        return Ok(data);
    }
    Ok(value)
}
fn call(
    session: &mut Session,
    operation: &str,
    args: Value,
    project: Option<&str>,
    deadline: &Deadline,
) -> Result<Value> {
    let value = if let Some(project) = project {
        if operation == "fs.list" {
            crate::query_features::validate_files_list_args(&args)?;
            let engine = session.query_engine_at(Some(project), deadline)?;
            let (snapshot, _, _, _) = engine.index(deadline)?;
            return crate::query_features::files_list(&engine.store, &snapshot, &args, deadline);
        }
        let tool = match operation {
            "fs.read" => "ci_read",
            "fs.search" | "fs.glob" | "symbols.find" => "ci_find",
            "symbols.references" => "ci_trace",
            _ => bail!("External project contexts permit source queries only"),
        };
        let mut args = args;
        match operation {
            "fs.search" => args["kind"] = json!("text"),
            "symbols.find" => args["kind"] = json!("symbol"),
            "fs.glob" => {
                args["kind"] = json!("file");
                args["pathGlob"] = args["pattern"].clone();
                args.as_object_mut()
                    .context("Arguments must be object")?
                    .remove("pattern");
            }
            "symbols.references" => {
                args["direction"] = json!("in");
                args["relations"] = json!(["references", "calls"]);
            }
            _ => {}
        }
        session.invoke(
            "queryProject",
            &json!({"project":project,"tool":tool,"arguments":args}),
            deadline,
        )?
    } else {
        session.invoke(operation, &args, deadline)?
    };
    unwrap_native(value)
}
fn text<'a>(args: &'a Value, key: &str) -> Result<&'a str> {
    args[key]
        .as_str()
        .with_context(|| format!("Missing string argument: {key}"))
}
fn move_field(source: &Value, target: &mut Value, from: &str, to: &str) {
    if let Some(value) = source.get(from) {
        target[to] = value.clone();
    }
}
fn prepare_captured_file_relocation(
    session: &mut Session,
    path: &str,
    destination: &str,
    directory_move: bool,
    deadline: &Deadline,
) -> Result<Value> {
    let path = crate::paths::normalized_syntax(path)?;
    let destination = crate::paths::normalized(destination)?;
    let engine = session.active_engine_at(deadline)?;
    let (snapshot, _, _, _) = engine.index(deadline)?;
    let source_host = crate::paths::scoped(&engine.root, &path)?;
    match std::fs::symlink_metadata(source_host) {
        Ok(metadata) if metadata.is_dir() && !crate::paths::is_link(&metadata) => {
            ensure!(
                directory_move,
                "File rename requires a captured regular file"
            );
            return crate::semantic_refactor::invoke(
                engine,
                "move",
                &json!({"snapshotId":snapshot.id,"relativePath":path,"targetRelativePath":destination}),
                deadline,
            );
        }
        Err(error) if error.kind() != std::io::ErrorKind::NotFound => return Err(error.into()),
        _ => {}
    }
    {
        let scope = crate::capture::source_scope_policy(&engine.store, &snapshot)?;
        scope
            .file(&path)
            .context("Relocation source must be a captured file or supported complete directory")?;
        scope.check_host_write_scope(&path)?;
        scope.check_host_write_scope(&destination)?;
    }
    ensure!(
        !path.eq_ignore_ascii_case(&destination)
            && !snapshot
                .files
                .iter()
                .any(|file| file.path.eq_ignore_ascii_case(&destination)),
        "File destination aliases its source or is occupied"
    );
    deadline.check()?;
    let destination_path = crate::paths::scoped(&engine.root, &destination)?;
    match std::fs::symlink_metadata(destination_path) {
        Ok(_) => bail!("File destination is occupied on the host"),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    let parent_path = match destination.rsplit_once('/') {
        Some((parent, _)) => crate::paths::scoped(&engine.root, parent)?,
        None => engine.root.clone(),
    };
    let parent_metadata = std::fs::symlink_metadata(parent_path)?;
    ensure!(
        parent_metadata.is_dir() && !crate::paths::is_link(&parent_metadata),
        "File destination must have an existing parent directory"
    );
    // Both facade translations use the existing native final-filename contract.
    // Bind its fresh acquisition to the source policy checked before the provider.
    crate::semantic_refactor::invoke(
        engine,
        "move",
        &json!({"snapshotId":snapshot.id,"relativePath":path,"targetRelativePath":destination}),
        deadline,
    )
}
fn strict_facade(method: &str, args: &Value) -> Result<()> {
    let object = args
        .as_object()
        .context("Facade arguments must be object")?;
    ensure!(
        serde_json::to_vec(args)?.len() <= MAX_FRAME,
        "Facade input budget exceeded"
    );
    let contract = facade_contract();
    let fields = contract["methods"][method]
        .as_array()
        .context("Unsupported facade method")?;
    ensure!(
        object
            .keys()
            .all(|key| fields.iter().any(|field| field == key)),
        "Unknown facade argument"
    );
    for key in [
        "include_info",
        "include_body",
        "include_file_documentation",
        "search_deps",
        "substring_matching",
        "symbol_access",
        "check_symbol_references",
        "recursive",
        "skip_ignored_files",
        "restrict_search_to_code_files",
        "multiline",
        "capture_stderr",
        "allow_multiple_occurrences",
        "dry_run",
        "delete_even_if_used",
        "propagate",
        "keep_definition",
        "rename_in_comments",
        "rename_in_text_occurrences",
    ] {
        ensure!(
            args.get(key)
                .is_none_or(|value| value.is_null() || value.is_boolean()),
            "Facade option {key} must be boolean"
        );
    }
    Ok(())
}
fn one_symbol(
    session: &mut Session,
    name: &str,
    path: &str,
    project: Option<&str>,
    deadline: &Deadline,
) -> Result<Value> {
    let found = find_symbols_facade(
        session,
        "lsp.find_symbol",
        &json!({"name_path_pattern":name,"relative_path":path,"max_matches":1}),
        project,
        deadline,
    )?;
    let items = found["items"]
        .as_array()
        .context("Symbol search result missing")?;
    ensure!(
        items.len() == 1 && found["truncated"] != true,
        "Symbol name path must resolve uniquely within captured source"
    );
    Ok(items[0].clone())
}
fn repl_callback_identity(method: &str) -> Option<(&'static str, &'static str)> {
    match method {
        "bridge.lsp.find_declaration.source" => {
            Some(("lsp.find_declaration", "bridge.declaration_source"))
        }
        "bridge.lsp.find_declaration.at" => Some(("lsp.find_declaration", "bridge.declaration_at")),
        "bridge.jb.find_declaration.source" => {
            Some(("jb.find_declaration", "bridge.declaration_source"))
        }
        "bridge.jb.find_declaration.at" => Some(("jb.find_declaration", "bridge.declaration_at")),
        "bridge.ext.project_context.enter" => {
            Some(("ext.project_context", "ext.read_project_context"))
        }
        _ => None,
    }
}
fn dispatch_repl(
    session: &mut Session,
    spec: &ReplSpec,
    method: &str,
    args: &Value,
    project: Option<&str>,
    deadline: &Deadline,
) -> Result<Value> {
    session.check_repl_state(deadline)?;
    if let Some((api, _)) = repl_callback_identity(method) {
        session.require_repl_api(api)?;
        if api == "ext.project_context" {
            session.require_repl_api("ext.read_project_context")?;
        }
        return dispatch(session, spec, method, args, project, deadline);
    }
    if session.selected_repl_methods()?.is_some() {
        ensure!(
            !method.starts_with("native."),
            "Generic native callbacks are unavailable under REPL API restrictions; use selected declared APIs"
        );
        session.require_repl_api(method)?;
    }
    dispatch(session, spec, method, args, project, deadline)
}

fn dispatch(
    session: &mut Session,
    spec: &ReplSpec,
    method: &str,
    args: &Value,
    project: Option<&str>,
    deadline: &Deadline,
) -> Result<Value> {
    let limit = args
        .get("max_answer_chars")
        .map(|value| {
            value
                .as_i64()
                .filter(|n| (-1..=4_000_000).contains(n))
                .context("Invalid max_answer_chars bound")
        })
        .transpose()?;
    let value = dispatch_unbounded(session, spec, method, args, project, deadline)?;
    if let Some(limit) = limit.filter(|n| *n >= 0) {
        let characters = serde_json::to_string(&value)?.chars().count();
        ensure!(
            characters <= limit as usize,
            "Result contains {characters} serialized Unicode characters and exceeds max_answer_chars={limit}; narrow the requested result"
        );
    }
    deadline.check()?;
    Ok(value)
}
fn dispatch_unbounded(
    session: &mut Session,
    spec: &ReplSpec,
    method: &str,
    args: &Value,
    project: Option<&str>,
    deadline: &Deadline,
) -> Result<Value> {
    let lsp_declaration = method != "bridge.jb.find_declaration.at";
    let method = repl_callback_identity(method).map_or(method, |(_, callback)| callback);
    deadline.check()?;
    if method == "bridge.declaration_source" {
        strict_facade("lsp.find_declaration", args)?;
        let path = text(args, "relative_path")?;
        ensure!(
            text(args, "regex")?.len() <= 10000,
            "Declaration regex budget exceeded"
        );
        let mut source = call(
            session,
            "fs.read",
            json!({"path":path,"maxBytes":MAX_FRAME}),
            project,
            deadline,
        )?;
        let bytes = source["text"]
            .as_str()
            .context("Full captured source is unavailable within the facade frame budget")?
            .len();
        ensure!(
            source["startByte"] == 0 && source["endByte"] == bytes,
            "Declaration regex requires the complete captured file"
        );
        if let Some(name) = args["containing_symbol_name_path"].as_str() {
            let symbol = one_symbol(session, name, path, project, deadline)?;
            ensure!(
                symbol["snapshotId"] == source["snapshotId"],
                "Source changed before declaration scope selection"
            );
            source["startByte"] = symbol["startByte"].clone();
            source["endByte"] = symbol["endByte"].clone();
        }
        return Ok(source);
    }
    if method == "bridge.declaration_at" {
        let fields = [
            "path",
            "snapshotId",
            "contentHash",
            "offset",
            "includeBody",
            "includeInfo",
        ];
        ensure!(
            args.as_object()
                .is_some_and(|object| object.keys().all(|key| fields.contains(&key.as_str()))),
            "Unknown declaration anchor field"
        );
        ensure!(
            args["snapshotId"]
                .as_str()
                .is_some_and(crate::model::valid_id)
                && args["contentHash"]
                    .as_str()
                    .is_some_and(crate::model::valid_id),
            "Invalid declaration source binding"
        );
        let path = text(args, "path")?;
        let offset = args["offset"]
            .as_u64()
            .context("Declaration byte offset missing")?;
        let checked = call(
            session,
            "fs.read",
            json!({"path":path,"snapshotId":args["snapshotId"],"startByte":0,"endByte":0}),
            project,
            deadline,
        )?;
        ensure!(
            checked["contentHash"] == args["contentHash"]
                && checked["snapshotId"] == args["snapshotId"],
            "Declaration source binding changed"
        );
        ensure!(
            ["includeBody", "includeInfo"]
                .iter()
                .all(|key| args.get(*key).is_none_or(Value::is_boolean)),
            "Declaration enrichment flags must be boolean"
        );
        let call_args = json!({"snapshotId":args["snapshotId"],"languageServer":{"path":path,"offset":offset,"operation":"declaration","options":{"includeBody":args["includeBody"]}},"timeoutMs":deadline.remaining().as_millis().min(600000)});
        let value = if let Some(project) = project {
            session.invoke(
                "queryProject",
                &json!({"project":project,"tool":"ci_find","arguments":call_args}),
                deadline,
            )?
        } else {
            session
                .active_engine_at(deadline)?
                .invoke("ci_find", call_args)?
        };
        let mut value = unwrap_native(value)?;
        ensure!(
            value["snapshotId"] == args["snapshotId"]
                && value["contentHash"] == args["contentHash"],
            "Declaration provider source binding changed"
        );
        if args["includeBody"] == true {
            let snapshot_id = text(args, "snapshotId")?;
            let locations = value["locations"]
                .as_array()
                .filter(|locations| locations.len() <= 2000)
                .context("Declaration location budget exceeded")?;
            let mut symbols = Vec::with_capacity(locations.len());
            for location in locations {
                deadline.check()?;
                symbols.push(
                    location_symbol(session, location, snapshot_id, project, deadline)?
                        .context("Provider declaration cannot map to one captured symbol body")?,
                );
            }
            let mut bodies = json!({"snapshotId":snapshot_id,"items":symbols});
            attach_symbol_bodies(session, &mut bodies, project, deadline)?;
            for (location, symbol) in value["locations"]
                .as_array_mut()
                .unwrap()
                .iter_mut()
                .zip(bodies["items"].as_array().unwrap())
            {
                location["symbol"] = symbol.clone();
            }
        }
        if args["includeInfo"] == true {
            let anchor = json!({"path":path,"snapshotId":args["snapshotId"],"contentHash":args["contentHash"],"nameStartByte":offset});
            let mut budget =
                session.symbol_information_budget(project, lsp_declaration, deadline)?;
            attach_optional_symbol_info(
                session,
                &anchor,
                &mut value,
                &mut budget,
                project,
                deadline,
            )?;
            budget.qualify(&mut value);
        }
        value["declarationEngine"] = json!("independent_host_selected_lsp");
        return Ok(value);
    }
    if let Some(operation) = method.strip_prefix("native.") {
        return call(session, operation, args.clone(), project, deadline);
    }
    strict_facade(method, args)?;
    if method == "mem.write_memory"
        && let Some(max) = args.get("max_chars")
    {
        let max = max.as_i64().context("max_chars must be an integer")?;
        ensure!(max >= -1, "Invalid max_chars bound");
        ensure!(
            max < 0 || text(args, "content")?.chars().count() <= max as usize,
            "Named note content exceeds requested max_chars"
        );
    }
    if method.starts_with("mem.") && method != "mem.onboarding" {
        if method == "mem.list_memories" {
            return list_memories(session, args, project, deadline);
        }
        let namespace = |name: &str| -> (bool, String) {
            if let Some(name) = name.strip_prefix("global/") {
                (true, name.into())
            } else {
                (false, name.into())
            }
        };
        let (operation, global, mut target) = match method {
            "mem.read_memory" | "mem.delete_memory" => {
                let (global, name) = namespace(text(args, "memory_name")?);
                (
                    if method == "mem.read_memory" {
                        "read"
                    } else {
                        "delete"
                    },
                    global,
                    json!({"name":name}),
                )
            }
            "mem.write_memory" => {
                let (global, name) = namespace(text(args, "memory_name")?);
                (
                    "write",
                    global,
                    json!({"name":name,"content":text(args,"content")?}),
                )
            }
            "mem.edit_memory" => {
                let (global, name) = namespace(text(args, "memory_name")?);
                let mode = text(args, "mode")?;
                ensure!(
                    matches!(mode, "literal" | "regex"),
                    "Invalid memory replacement mode"
                );
                (
                    "edit",
                    global,
                    json!({"name":name,"needle":text(args,"needle")?,"replacement":text(args,"repl")?,"regex":mode=="regex","allowMultiple":args["allow_multiple_occurrences"]==true}),
                )
            }
            "mem.rename_memory" => {
                let (global, old) = namespace(text(args, "old_name")?);
                let (new_global, new) = namespace(text(args, "new_name")?);
                ensure!(
                    global == new_global,
                    "Cross-namespace memory rename requires a separate explicit storage operation"
                );
                ("rename", global, json!({"oldName":old,"newName":new}))
            }
            _ => bail!("Unsupported memory facade"),
        };
        if global {
            target["scope"] = json!("global");
        }
        return call(
            session,
            &format!("notes.{operation}"),
            target,
            project,
            deadline,
        );
    }
    if matches!(method, "jb.run_inspections" | "jb.list_inspections") {
        let mut target = json!({});
        let operation = if method == "jb.run_inspections" {
            "inspections"
        } else {
            "listInspections"
        };
        for (from, to) in [
            ("relative_path", "relativePath"),
            ("inspection_names", "inspectionNames"),
            ("start_line", "startLine"),
            ("end_line", "endLine"),
            ("language", "language"),
            ("group_path_contains", "groupContains"),
        ] {
            if args.get(from).is_some_and(|value| !value.is_null()) {
                move_field(args, &mut target, from, to);
            }
        }
        if let Some(severity) = args.get("min_severity").filter(|value| !value.is_null()) {
            let severity = severity
                .as_str()
                .context("Inspection severity must be text")?
                .to_ascii_lowercase();
            target["minSeverity"] = json!(match severity.as_str() {
                "error" => 1,
                "warning" => 2,
                "weak_warning" | "information" => 3,
                "hint" => 4,
                _ => bail!("Unsupported inspection severity"),
            });
        }
        return crate::semantic_refactor::invoke(
            session.query_engine_at(project, deadline)?,
            operation,
            &target,
            deadline,
        );
    }
    if matches!(
        method,
        "lsp.safe_delete_symbol" | "jb.safe_delete" | "jb.inline_symbol" | "jb.move"
    ) {
        ensure!(
            project.is_none(),
            "External project contexts cannot prepare semantic refactoring plans"
        );
        session.require_edit_permission()?;
        let operation = match method {
            "jb.inline_symbol" => "inline",
            "jb.move" => "move",
            _ => "safeDelete",
        };
        if method == "jb.safe_delete" {
            let file_selection = match args.get("name_path") {
                None | Some(Value::Null) => true,
                Some(Value::String(name)) => {
                    ensure!(
                        !name.is_empty(),
                        "Empty safe-delete name_path does not select a file; omit it or use null"
                    );
                    false
                }
                _ => bail!("Safe-delete name_path must be text or null"),
            };
            if file_selection {
                match args.get("propagate") {
                    None | Some(Value::Bool(false)) => {}
                    Some(Value::Bool(true)) => {
                        bail!("File deletion propagation is unavailable")
                    }
                    _ => bail!("File deletion propagate must be boolean"),
                }
                let forced = match args.get("delete_even_if_used") {
                    Some(Value::Bool(true)) => true,
                    None | Some(Value::Bool(false)) => false,
                    _ => bail!("File deletion delete_even_if_used must be boolean"),
                };
                let path = crate::paths::normalized_syntax(text(args, "relative_path")?)?;
                let engine = session.active_engine_at(deadline)?;
                if !forced {
                    return crate::semantic_refactor::invoke(
                        engine,
                        "safeDelete",
                        &json!({"relativePath":path,"deleteEvenIfUsed":false,"propagate":false}),
                        deadline,
                    );
                }
                let (snapshot, graph, _, _) = engine.index(deadline)?;
                let source_hash = {
                    let scope = crate::capture::source_scope_policy(&engine.store, &snapshot)?;
                    let file = scope.file(&path).context(
                        "Only captured file deletion is supported; directory tree deletion is unavailable",
                    )?;
                    scope.check_host_write_scope(&path)?;
                    file.hash.clone()
                };
                deadline.check()?;
                let host_path = crate::paths::scoped(&engine.root, &path)?;
                let metadata = std::fs::symlink_metadata(host_path)?;
                ensure!(
                    metadata.is_file() && !crate::paths::is_link(&metadata),
                    "File deletion source must be an existing regular file"
                );
                let plan = crate::changes::prepare(
                    &mut engine.store,
                    &snapshot,
                    &graph,
                    &json!({"edits":[{"kind":"delete","path":path,"expectedHash":source_hash}]}),
                    deadline,
                )?;
                return Ok(json!({
                    "snapshotId":snapshot.id,
                    "plan":plan,
                    "sourceBinding":{"snapshotId":snapshot.id,"path":path,"contentHash":source_hash},
                    "provider":"native_source_policy",
                    "equivalent":"independent_source_bound_forced_file_delete",
                    "forced":true,
                    "propagate":false,
                    "referencesRewritten":false,
                    "remainingUsagesMayBreak":true,
                    "usageCheck":{"performed":false,"reason":"explicit_file_force","exhaustiveCompilerProof":false},
                    "warnings":["Explicit file force skips usage checks and does not rewrite usages; references may remain."],
                    "complete":false,
                    "commandsExecuted":false
                }));
            }
        }
        let mut target = json!({});
        for (from, to) in [
            ("relative_path", "relativePath"),
            ("name_path", "namePath"),
            ("name_path_pattern", "namePath"),
            ("target_relative_path", "targetRelativePath"),
            ("delete_even_if_used", "deleteEvenIfUsed"),
            ("propagate", "propagate"),
            ("keep_definition", "keepDefinition"),
        ] {
            if args.get(from).is_some_and(|value| !value.is_null()) {
                move_field(args, &mut target, from, to);
            }
        }
        if operation == "move" {
            let file_selection = match args.get("name_path") {
                None | Some(Value::Null) => true,
                Some(Value::String(name)) => name.is_empty(),
                _ => bail!("Move name_path must be text or null"),
            };
            if file_selection {
                ensure!(
                    args.get("target_parent_name_path")
                        .is_none_or(Value::is_null),
                    "File move does not accept a symbol-parent destination"
                );
                let path = crate::paths::normalized_syntax(text(args, "relative_path")?)?;
                let parent = crate::paths::normalized(text(args, "target_relative_path")?)?;
                let basename = path
                    .rsplit('/')
                    .next()
                    .context("Move source filename missing")?;
                let destination = crate::paths::normalized(&format!("{parent}/{basename}"))?;
                // The pinned JB facade uses a parent directory. The direct
                // native move API continues to take the final destination path.
                return prepare_captured_file_relocation(
                    session,
                    &path,
                    &destination,
                    true,
                    deadline,
                );
            }
            match args.get("target_parent_name_path") {
                None | Some(Value::Null) => {}
                Some(Value::String(parent)) => {
                    ensure!(
                        !parent.is_empty(),
                        "Move symbol parent selector cannot be empty"
                    );
                    target["targetParentNamePath"] = json!(parent);
                }
                _ => bail!("Move target_parent_name_path must be text or null"),
            }
        } else {
            ensure!(
                args.get("target_parent_name_path")
                    .is_none_or(Value::is_null),
                "Symbol-parent move requires an explicit provider destination refactoring capability"
            );
        }
        return session.invoke(
            "refactor.prepare",
            &json!({"operation":operation,"arguments":target}),
            deadline,
        );
    }
    match method {
        "cfg.get_current_config" => call(session, "configuration", json!({}), None, deadline),
        "jb.debug_eval_info" => {
            ensure!(
                project.is_none(),
                "External source query contexts have no debugger authority"
            );
            Ok(session.debug_info())
        }
        "jb.debug_eval" => {
            ensure!(
                project.is_none(),
                "External source query contexts have no debugger authority"
            );
            ensure!(
                args.get("repl_key")
                    .is_none_or(|value| value.is_null() || value.is_string()),
                "Debugger key must be text"
            );
            session.debug_evaluate(
                args["repl_key"].as_str(),
                text(args, "expression")?,
                deadline,
            )
        }
        "lsp.restart_language_server" => {
            ensure!(
                project.is_none(),
                "External query contexts cannot refresh provider lifecycle state"
            );
            crate::lsp::restart_registered(session.query_engine_at(None, deadline)?, deadline)
        }
        "ext.list_projects" => {
            let requested = args["symbol_access"].as_bool().unwrap_or(true);
            let mut value = call(session, "listProjects", json!({}), None, deadline)?;
            // Every registered project is backed by the native source-query API.
            // Provider process health is a separate explicit host operation.
            for item in value["items"]
                .as_array_mut()
                .context("Project registry items missing")?
            {
                deadline.check()?;
                item["nativeSymbolQueries"] = json!(true);
                item["semanticProviderReadiness"] = json!("not_probed");
            }
            value["symbolAccessRequested"] = json!(requested);
            value["symbolAccessSemantics"] =
                json!("registered_native_symbol_query_api_not_live_provider_readiness");
            Ok(value)
        }
        "ext.read_project_context" => {
            let project = text(args, "project_name")?;
            unwrap_native(session.invoke(
                "queryProject",
                &json!({"project":project,"tool":"ci_status","arguments":{}}),
                deadline,
            )?)
        }
        "mem.onboarding" => call(session, "onboarding", json!({}), project, deadline),
        "fs.list_dir" => call(
            session,
            "fs.list",
            json!({"relativePath":text(args,"relative_path")?,"recursive":args.get("recursive").cloned().unwrap_or(json!(false)),"skipIgnored":args.get("skip_ignored_files").cloned().unwrap_or(json!(false))}),
            project,
            deadline,
        ),
        "fs.find_file" => {
            let mask = text(args, "file_mask")?;
            let pattern = if mask.contains('/') {
                mask.to_owned()
            } else {
                format!("**/{mask}")
            };
            call(
                session,
                "fs.glob",
                json!({"pattern":pattern,"relativePath":text(args,"relative_path")?,"skipIgnored":args.get("skip_ignored_files").cloned().unwrap_or(json!(false))}),
                project,
                deadline,
            )
        }
        "fs.read_file" => {
            let mut target = json!({"path":text(args,"relative_path")?});
            for (from, to) in [("start_line", "startLine"), ("end_line", "endLine")] {
                if let Some(value) = args.get(from).filter(|value| !value.is_null()) {
                    let line = value.as_u64().context("Invalid zero-based file line")?;
                    target[to] = json!(line + 1);
                }
            }
            call(session, "fs.read", target, project, deadline)
        }
        "fs.search_for_pattern" => {
            let mut target = json!({"query":text(args,"substring_pattern")?,"regex":true,"multiline":args.get("multiline").cloned().unwrap_or(json!(true)),"skipIgnored":args.get("skip_ignored_files").cloned().unwrap_or(json!(true))});
            for (from, to) in [
                ("relative_path", "relativePath"),
                ("paths_include_glob", "pathGlob"),
                ("paths_exclude_glob", "excludeGlob"),
                ("context_lines_before", "beforeContextLines"),
                ("context_lines_after", "afterContextLines"),
            ] {
                if matches!(from, "paths_include_glob" | "paths_exclude_glob")
                    && args
                        .get(from)
                        .and_then(Value::as_str)
                        .is_some_and(str::is_empty)
                {
                    continue;
                }
                move_field(args, &mut target, from, to);
            }
            if args.get("skip_ignored_files").is_none() && session.has_git_ignore_selection() {
                target["skipIgnored"] = json!(
                    !session
                        .query_engine_at(project, deadline)?
                        .capture_options
                        .include_git_ignored
                );
            }
            if args["restrict_search_to_code_files"] == true {
                let engine = session.query_engine_at(project, deadline)?;
                let (snapshot, graph, _, _) = engine.index(deadline)?;
                let scope = crate::capture::source_scope_policy(&engine.store, &snapshot)?;
                let mut selected = std::collections::BTreeSet::new();
                for file in &snapshot.files {
                    deadline.check()?;
                    let file = scope.file(&file.path)?;
                    if !matches!(file.language.as_str(), "text" | "config" | "markdown") {
                        selected.insert(file.path.clone());
                    }
                }
                let mut value = crate::query_features::text_find_selected(
                    &engine.store,
                    &snapshot,
                    &graph,
                    &target,
                    &selected,
                    deadline,
                )?;
                value["snapshotId"] = json!(snapshot.id);
                value["codeScope"] = json!({"classification":"proven_captured_language","selectedPaths":selected.len(),"excludedClasses":["text","config","markdown"],"providerCertification":false,"captureExpanded":false});
                return Ok(value);
            }
            call(session, "fs.search", target, project, deadline)
        }
        "lsp.find_symbol" | "jb.find_symbol" => {
            find_symbols_facade(session, method, args, project, deadline)
        }
        "lsp.get_symbols_overview" | "jb.get_symbols_overview" => {
            let mut value = symbols_overview(session, args, project, deadline)?;
            if args["include_file_documentation"] == true {
                value["fileDocumentation"] = file_documentation(
                    session,
                    text(args, "relative_path")?,
                    text(&value, "snapshotId")?,
                    project,
                    deadline,
                )?;
            }
            Ok(value)
        }
        "lsp.find_referencing_symbols" | "jb.find_referencing_symbols" => {
            let include = symbol_kind_filter(args, "include_kinds")?.unwrap_or_default();
            let exclude = symbol_kind_filter(args, "exclude_kinds")?.unwrap_or_default();
            let symbol = one_symbol(
                session,
                text(args, "name_path")?,
                text(args, "relative_path")?,
                project,
                deadline,
            )?;
            let mut value = call(
                session,
                "symbols.references",
                json!({"snapshotId":symbol["snapshotId"],"entityIds":[symbol["id"]],"depth":1}),
                project,
                deadline,
            )?;
            let entities: std::collections::BTreeMap<_, _> = value["entities"]
                .as_array()
                .context("Reference entities missing")?
                .iter()
                .map(|entity| {
                    (
                        entity["id"].as_str().unwrap_or("").to_owned(),
                        entity.clone(),
                    )
                })
                .collect();
            let mut selected = std::collections::BTreeSet::new();
            let mut evidence = std::collections::BTreeSet::new();
            value["observations"]
                .as_array_mut()
                .context("Reference observations missing")?
                .retain(|observation| {
                    let Some(entity) = observation["source"]
                        .as_str()
                        .and_then(|id| entities.get(id))
                    else {
                        return false;
                    };
                    let kind = entity["kind"].as_str().unwrap_or("");
                    let keep =
                        (include.is_empty() || include.contains(&kind)) && !exclude.contains(&kind);
                    if keep {
                        selected.insert(entity["id"].as_str().unwrap_or("").to_owned());
                        if let Some(ids) = observation["evidenceIds"].as_array() {
                            evidence
                                .extend(ids.iter().filter_map(Value::as_str).map(str::to_owned));
                        }
                    }
                    keep
                });
            value["evidence"]
                .as_array_mut()
                .context("Reference evidence missing")?
                .retain(|item| item["id"].as_str().is_some_and(|id| evidence.contains(id)));
            let mut callers = vec![];
            for id in selected {
                deadline.check()?;
                let mut entity = entities
                    .get(id.as_str())
                    .context("Selected reference container missing")?
                    .clone();
                let source = call(
                    session,
                    "fs.read",
                    json!({"path":entity["path"],"snapshotId":symbol["snapshotId"],"startByte":0,"endByte":0}),
                    project,
                    deadline,
                )?;
                ensure!(
                    source["snapshotId"] == symbol["snapshotId"]
                        && entity["snapshotId"] == symbol["snapshotId"],
                    "Referencing container source binding changed"
                );
                entity["contentHash"] = source["contentHash"].clone();
                callers.push(entity);
            }
            value["referencingSymbols"] = json!(callers);
            value["kindFilterSemantics"] = json!(
                "containing_referencing_symbol_native_kind_mapped_from_standard_lsp_symbolkind"
            );
            value["unmappedKindPolicy"] =
                json!("included_only_when_no_include_filter_and_not_explicitly_excluded");
            Ok(value)
        }
        "lsp.find_implementations" | "jb.find_implementations" => {
            let include = symbol_kind_filter(args, "include_kinds")?.unwrap_or_default();
            let exclude = symbol_kind_filter(args, "exclude_kinds")?.unwrap_or_default();
            let symbol = one_symbol(
                session,
                text(args, "name_path")?,
                text(args, "relative_path")?,
                project,
                deadline,
            )?;
            let mut value = semantic_from_symbol(
                session,
                &symbol,
                "implementation",
                json!({}),
                project,
                deadline,
            )?;
            let locations = value["locations"]
                .as_array()
                .context("Implementation locations missing")?
                .clone();
            let mut selected = vec![];
            let mut unmapped = 0usize;
            let mut info_budget = if args["include_info"] == true {
                session.symbol_information_budget(
                    project,
                    method == "lsp.find_implementations",
                    deadline,
                )?
            } else {
                crate::symbol_info_budget::Retrieval::default()
            };
            for mut location in locations {
                deadline.check()?;
                let metadata = location_symbol(
                    session,
                    &location,
                    text(&value, "snapshotId")?,
                    project,
                    deadline,
                )?;
                if let Some(metadata) = metadata {
                    let kind = text(&metadata, "kind")?;
                    if !include.is_empty() && !include.contains(&kind) || exclude.contains(&kind) {
                        continue;
                    }
                    if args["include_info"] == true {
                        attach_optional_symbol_info(
                            session,
                            &metadata,
                            &mut location,
                            &mut info_budget,
                            project,
                            deadline,
                        )?;
                    }
                    location["symbol"] = metadata;
                } else {
                    unmapped += 1;
                    ensure!(
                        include.is_empty() && exclude.is_empty(),
                        "Implementation kind filter cannot classify an unmatched source location"
                    );
                    if args["include_info"] == true {
                        let anchor = json!({"path":location["path"],"contentHash":location["contentHash"],"snapshotId":value["snapshotId"],"nameStartByte":location["startByte"]});
                        attach_optional_symbol_info(
                            session,
                            &anchor,
                            &mut location,
                            &mut info_budget,
                            project,
                            deadline,
                        )?;
                    }
                    location["symbol"] = Value::Null;
                }
                selected.push(location);
            }
            value["locations"] = json!(selected);
            value["unmappedSourceSymbols"] = json!(unmapped);
            info_budget.qualify(&mut value);
            value["kindFilterSemantics"] =
                json!("source_bound_implemented_symbol_native_kind_from_standard_lsp_symbolkind");
            Ok(value)
        }
        "lsp.rename_symbol" | "jb.rename" => {
            ensure!(
                project.is_none(),
                "External project contexts cannot prepare rename plans"
            );
            session.require_edit_permission()?;
            if method == "jb.rename" {
                match args.get("name_path") {
                    None | Some(Value::Null) => {
                        for key in ["rename_in_comments", "rename_in_text_occurrences"] {
                            match args.get(key) {
                                None | Some(Value::Bool(false)) => {}
                                Some(Value::Bool(true)) => bail!(
                                    "File rename comment/text occurrence options are unavailable"
                                ),
                                _ => bail!("File rename option {key} must be boolean"),
                            }
                        }
                        let path = crate::paths::normalized_syntax(text(args, "relative_path")?)?;
                        let basename = crate::paths::normalized(text(args, "new_name")?)?;
                        ensure!(
                            !basename.contains('/'),
                            "File rename new_name must be a single portable basename"
                        );
                        let destination = match path.rsplit_once('/') {
                            Some((parent, _)) => format!("{parent}/{basename}"),
                            None => basename,
                        };
                        return prepare_captured_file_relocation(
                            session,
                            &path,
                            &destination,
                            false,
                            deadline,
                        );
                    }
                    Some(Value::String(name)) => ensure!(
                        !name.is_empty(),
                        "Empty rename name_path does not select a file; omit it or use null"
                    ),
                    _ => bail!("Rename name_path must be text or null"),
                }
            }
            if args["rename_in_comments"] == true || args["rename_in_text_occurrences"] == true {
                return crate::semantic_refactor::invoke(
                    session.active_engine_at(deadline)?,
                    "rename",
                    &json!({"relativePath":text(args,"relative_path")?,"namePath":text(args,"name_path")?,"newName":text(args,"new_name")?,
                        "renameInComments":args.get("rename_in_comments").cloned().unwrap_or(json!(false)),
                        "renameInTextOccurrences":args.get("rename_in_text_occurrences").cloned().unwrap_or(json!(false))}),
                    deadline,
                );
            }
            let symbol = one_symbol(
                session,
                text(args, "name_path")?,
                text(args, "relative_path")?,
                None,
                deadline,
            )?;
            call(
                session,
                "edit.prepare",
                json!({"snapshotId":symbol["snapshotId"],"rename":{"entityId":symbol["id"],"newName":text(args,"new_name")?}}),
                None,
                deadline,
            )
        }
        "lsp.get_diagnostics_for_file" => {
            let path = text(args, "relative_path")?;
            let source = call(
                session,
                "fs.read",
                json!({"path":path,"maxBytes":MAX_FRAME}),
                project,
                deadline,
            )?;
            let text = text(&source, "text")?;
            ensure!(
                source["startByte"] == 0 && source["endByte"] == text.len(),
                "Diagnostic line scope requires complete captured source"
            );
            let starts = crate::paths::line_starts(text);
            let first = args
                .get("start_line")
                .map(|value| value.as_u64().context("Invalid diagnostic start line"))
                .transpose()?
                .unwrap_or(0) as usize;
            let last = args
                .get("end_line")
                .map(|value| {
                    value
                        .as_i64()
                        .filter(|n| *n >= -1)
                        .context("Invalid diagnostic end line")
                })
                .transpose()?
                .unwrap_or(-1);
            let last = if last < 0 {
                starts.len() - 1
            } else {
                last as usize
            };
            ensure!(
                first <= last && last < starts.len(),
                "Diagnostic line range outside captured source"
            );
            let begin = starts[first];
            let end = starts.get(last + 1).copied().unwrap_or(text.len());
            let anchor = json!({"path":path,"snapshotId":source["snapshotId"],"contentHash":source["contentHash"]});
            let mut value = semantic_from_symbol(
                session,
                &anchor,
                "diagnostics",
                json!({"minSeverity":args.get("min_severity").cloned().unwrap_or(json!(4))}),
                project,
                deadline,
            )?;
            value["items"]
                .as_array_mut()
                .context("Diagnostics missing")?
                .retain(|item| {
                    item["startByte"].as_u64().is_some_and(|start| {
                        start as usize >= begin
                            && ((start as usize) < end
                                || last + 1 == starts.len()
                                    && start as usize == end
                                    && item["endByte"].as_u64() == Some(start))
                    })
                });
            value["sourceRange"] =
                json!({"startByte":begin,"endByte":end,"startLine":first,"endLine":last});
            Ok(value)
        }
        "jb.get_type_hierarchy" => {
            let symbol = one_symbol(
                session,
                text(args, "name_path")?,
                text(args, "relative_path")?,
                project,
                deadline,
            )?;
            let direction = args["hierarchy_type"].as_str().unwrap_or("both");
            ensure!(
                matches!(direction, "super" | "sub" | "both"),
                "Invalid hierarchy direction"
            );
            let options = json!({"depth":args.get("depth").filter(|value|!value.is_null()).cloned().unwrap_or(json!(1))});
            let mut value = json!({"complete":false,"provider":"host_selected_lsp"});
            if direction != "sub" {
                value["supertypes"] = semantic_from_symbol(
                    session,
                    &symbol,
                    "supertypes",
                    options.clone(),
                    project,
                    deadline,
                )?;
            }
            if direction != "super" {
                value["subtypes"] =
                    semantic_from_symbol(session, &symbol, "subtypes", options, project, deadline)?;
            }
            Ok(value)
        }
        "lsp.get_diagnostics_for_symbol" => {
            let symbol = one_symbol(
                session,
                text(args, "name_path")?,
                args["reference_file"].as_str().unwrap_or(""),
                project,
                deadline,
            )?;
            let mut ranges = vec![symbol.clone()];
            if args["check_symbol_references"] == true {
                let refs = dispatch_unbounded(
                    session,
                    spec,
                    "lsp.find_referencing_symbols",
                    &json!({"name_path":args["name_path"],"relative_path":symbol["path"]}),
                    project,
                    deadline,
                )?;
                ensure!(
                    refs["snapshotId"] == symbol["snapshotId"],
                    "Diagnostic reference source binding changed"
                );
                ranges.extend(diagnostic_referencing_symbols(
                    session, &refs, &symbol, project, deadline,
                )?);
            }
            let mut files = std::collections::BTreeMap::new();
            for range in &ranges {
                deadline.check()?;
                let path = text(range, "path")?;
                if !files.contains_key(path) {
                    files.insert(path.to_owned(),semantic_from_symbol(session,range,"diagnostics",json!({"minSeverity":args.get("min_severity").cloned().unwrap_or(json!(4))}),project,deadline)?);
                }
            }
            let mut value = files
                .get(text(&symbol, "path")?)
                .context("Selected symbol diagnostics absent")?
                .clone();
            let mut items = vec![];
            for (path, diagnostics) in &files {
                for item in diagnostics["items"]
                    .as_array()
                    .context("Diagnostics missing")?
                {
                    deadline.check()?;
                    if ranges.iter().any(|range| {
                        range["path"] == *path
                            && item["startByte"]
                                .as_u64()
                                .zip(range["startByte"].as_u64())
                                .is_some_and(|(start, first)| start >= first)
                            && item["startByte"]
                                .as_u64()
                                .zip(range["endByte"].as_u64())
                                .is_some_and(|(start, end)| start < end)
                    }) {
                        items.push(item.clone());
                    }
                }
            }
            value["items"] = json!(items);
            value["referenceScope"] = json!(if args["check_symbol_references"] == true {
                "captured_referencing_symbols"
            } else {
                "selected_symbol_only"
            });
            value["checkedFiles"] = json!(files.keys().collect::<Vec<_>>());
            value["symbol"] = symbol;
            Ok(value)
        }
        "edit.replace_lines" | "edit.delete_lines" | "edit.insert_at_line" => {
            ensure!(
                project.is_none(),
                "External project contexts cannot prepare edits"
            );
            let path = text(args, "relative_path")?;
            let engine = session.active_engine_at(deadline)?;
            let (snapshot, _, _, _) = engine.index(deadline)?;
            let (source, hash) = crate::query::source(&engine.store, &snapshot, path)?;
            let starts = crate::paths::line_starts(&source);
            let insertion = method == "edit.insert_at_line";
            let first = args[if insertion { "line" } else { "start_line" }]
                .as_u64()
                .context("Zero-based line required")? as usize;
            let last = if insertion {
                first
            } else {
                args["end_line"]
                    .as_u64()
                    .context("Zero-based end line required")? as usize
            };
            ensure!(
                first <= last && last < starts.len(),
                "Line edit range outside captured source"
            );
            let start = starts[first];
            let end = if insertion {
                start
            } else {
                starts.get(last + 1).copied().unwrap_or(source.len())
            };
            let replacement = if method == "edit.delete_lines" {
                ""
            } else {
                text(args, "content")?
            };
            call(
                session,
                "edit.prepare",
                json!({"snapshotId":snapshot.id,"edits":[{"kind":"replace","path":path,"expectedHash":hash,"startByte":start,"endByte":end,"text":replacement}]}),
                None,
                deadline,
            )
        }
        "edit.replace_content" | "edit.replace_in_files" => {
            ensure!(
                project.is_none(),
                "External project contexts cannot prepare edits"
            );
            let mode = text(args, "mode")?;
            ensure!(
                matches!(mode, "literal" | "regex"),
                "Replacement mode must be literal or regex"
            );
            let mut target = json!({"pattern":text(args,"needle")?,"replacement":text(args,"repl")?,"literal":mode=="literal","multiline":true});
            for (from, to) in [
                ("relative_path", "relativePath"),
                ("paths_include_glob", "pathGlob"),
                ("paths_exclude_glob", "excludeGlob"),
                ("dry_run", "dryRun"),
                ("occurrence_ids", "occurrenceIds"),
            ] {
                move_field(args, &mut target, from, to);
            }
            if method == "edit.replace_content" && args["allow_multiple_occurrences"] != true {
                target["expectedCount"] = json!(1);
            } else if args["expected_count"]
                .as_i64()
                .is_some_and(|count| count >= 0)
            {
                target["expectedCount"] = args["expected_count"].clone();
            }
            call(
                session,
                "edit.prepare",
                json!({"bulkReplace":target}),
                None,
                deadline,
            )
        }
        "edit.replace_symbol_body" | "edit.insert_before_symbol" | "edit.insert_after_symbol" => {
            ensure!(
                project.is_none(),
                "External project contexts cannot prepare edits"
            );
            let symbol = one_symbol(
                session,
                text(args, "name_path")?,
                text(args, "relative_path")?,
                None,
                deadline,
            )?;
            let operation = match method {
                "edit.replace_symbol_body" => "replaceSymbol",
                "edit.insert_before_symbol" => "insertBefore",
                _ => "insertAfter",
            };
            let mut target = json!({});
            target["snapshotId"] = symbol["snapshotId"].clone();
            target[operation] = json!({"entityId":symbol["id"],"text":text(args,"body")?});
            call(session, "edit.prepare", target, None, deadline)
        }
        "fs.create_text_file" => {
            ensure!(
                project.is_none(),
                "External project contexts cannot prepare edits"
            );
            let path = text(args, "relative_path")?;
            let content = text(args, "content")?;
            let engine = session.active_engine_at(deadline)?;
            let (snapshot, _, _, _) = engine.index(deadline)?;
            let edit = if let Some(file) = snapshot.files.iter().find(|file| file.path == path) {
                json!({"kind":"replace","path":path,"expectedHash":file.hash,"startByte":0,"endByte":file.size,"text":content})
            } else {
                json!({"kind":"create","path":path,"text":content})
            };
            call(
                session,
                "edit.prepare",
                json!({"snapshotId":snapshot.id,"edits":[edit]}),
                None,
                deadline,
            )
        }
        "shell.execute_shell_command" => {
            ensure!(
                project.is_none(),
                "External project contexts cannot execute shell commands"
            );
            let shell = spec
                .shell
                .as_ref()
                .context("No independent host shell profile was authorized")?;
            let command = text(args, "command")?;
            ensure!(
                command.len() <= 128000 && !command.contains('\0'),
                "Invalid shell command capacity"
            );
            let root = session.active_engine_at(deadline)?.root.clone();
            let cwd = args["cwd"]
                .as_str()
                .map(|path| crate::paths::scoped(&root, path))
                .transpose()?
                .unwrap_or(root);
            ensure!(
                cwd.is_dir(),
                "Shell cwd must be an existing active-project directory"
            );
            let mut process = Command::new(&shell.command);
            process.args(&shell.arguments).arg(command).current_dir(cwd);
            let output = crate::process::run_capture(
                process,
                &[],
                deadline,
                crate::process::CaptureLimits {
                    stdin_bytes: 0,
                    stdout_bytes: spec.max_output_bytes,
                    stderr_bytes: spec.max_output_bytes.min(8_000_000),
                },
            )?;
            Ok(
                json!({"exitCode":output.exit_code,"stdout":String::from_utf8_lossy(&output.stdout),"stderr":if args["capture_stderr"]==false{String::new()}else{String::from_utf8_lossy(&output.stderr).into_owned()},"timedOut":output.timed_out,"outputComplete":output.output_complete,"hostExecution":true,"osSandbox":false}),
            )
        }
        _ => bail!("Facade method requires an unavailable provider capability: {method}"),
    }
}

fn diagnostic_referencing_symbols(
    session: &mut Session,
    references: &Value,
    symbol: &Value,
    project: Option<&str>,
    deadline: &Deadline,
) -> Result<Vec<Value>> {
    let snapshot_id = text(symbol, "snapshotId")?;
    ensure!(
        references["snapshotId"] == snapshot_id && references["truncated"] != true,
        "Diagnostic reference discovery is incomplete or changed snapshot"
    );
    let containers = references["referencingSymbols"]
        .as_array()
        .filter(|items| items.len() <= 200)
        .context("Diagnostic referencing symbol budget exceeded")?;
    let engine = session.query_engine_at(project, deadline)?;
    let snapshot = engine.store.snapshot(snapshot_id)?;
    let scope = crate::capture::source_scope_policy(&engine.store, &snapshot)?;
    let mut sources = std::collections::BTreeMap::new();
    let mut source_bytes = 0usize;
    let mut ranges = vec![];
    let mut seen = std::collections::BTreeSet::new();
    for item in containers {
        deadline.check()?;
        let id = text(item, "id")?;
        ensure!(
            crate::model::valid_id(id) && seen.insert(id),
            "Invalid or duplicate diagnostic referencing symbol"
        );
        ensure!(
            item["snapshotId"] == snapshot_id,
            "Diagnostic referencing symbol snapshot mismatch"
        );
        let path = text(item, "path")?;
        let file = scope.file(path)?;
        ensure!(
            item["contentHash"] == file.hash,
            "Diagnostic reference hash mismatch"
        );
        if !sources.contains_key(path) {
            source_bytes = source_bytes
                .checked_add(file.size)
                .context("Diagnostic reference source capacity overflow")?;
            ensure!(
                sources.len() < 200 && source_bytes <= 16_000_000,
                "Diagnostic reference source budget exceeded"
            );
            sources.insert(
                path.to_owned(),
                String::from_utf8(scope.read_source(path)?)?,
            );
        }
        let start = item["startByte"]
            .as_u64()
            .context("Diagnostic reference start missing")?;
        let end = item["endByte"]
            .as_u64()
            .context("Diagnostic reference end missing")?;
        crate::paths::utf8_range(&sources[path], start as usize, end as usize)?;
        ranges.push(item.clone());
    }
    Ok(ranges)
}

fn attach_optional_symbol_info(
    session: &mut Session,
    anchor: &Value,
    target: &mut Value,
    budget: &mut crate::symbol_info_budget::Retrieval,
    project: Option<&str>,
    deadline: &Deadline,
) -> Result<()> {
    if !budget.permit(deadline)? {
        target["infoOmitted"] = json!({"reason":"symbol_info_budget_elapsed"});
        return Ok(());
    }
    let info = semantic_from_symbol(session, anchor, "hover", json!({}), project, deadline)?;
    deadline.check()?;
    target["info"] = info;
    budget.received();
    Ok(())
}

fn semantic_from_symbol(
    session: &mut Session,
    symbol: &Value,
    operation: &str,
    options: Value,
    project: Option<&str>,
    deadline: &Deadline,
) -> Result<Value> {
    let path = symbol["path"]
        .as_str()
        .context("Symbol source path missing")?;
    let offset = if operation == "diagnostics" {
        0
    } else {
        symbol["nameStartByte"]
            .as_u64()
            .context("Symbol selection missing")? as usize
    };
    ensure!(
        symbol["snapshotId"]
            .as_str()
            .is_some_and(crate::model::valid_id)
            && symbol["contentHash"]
                .as_str()
                .is_some_and(crate::model::valid_id),
        "Invalid symbol source binding"
    );
    let args = json!({"snapshotId":symbol["snapshotId"],"languageServer":{"path":path,"offset":offset,"operation":operation,"options":options},"timeoutMs":deadline.remaining().as_millis().min(600000)});
    let value = if let Some(project) = project {
        session.invoke(
            "queryProject",
            &json!({"project":project,"tool":"ci_find","arguments":args}),
            deadline,
        )?
    } else {
        session
            .active_engine_at(deadline)?
            .invoke("ci_find", args)?
    };
    let value = unwrap_native(value)?;
    ensure!(
        value["snapshotId"] == symbol["snapshotId"]
            && value["contentHash"] == symbol["contentHash"],
        "Provider result changed symbol source binding"
    );
    Ok(value)
}
fn symbol_kind_filter(args: &Value, key: &str) -> Result<Option<Vec<&'static str>>> {
    let Some(value) = args.get(key).filter(|value| !value.is_null()) else {
        return Ok(None);
    };
    let kinds = value
        .as_array()
        .filter(|kinds| kinds.len() <= 26)
        .context("Symbol kinds must be an array of standard integer SymbolKind values")?;
    let names = [
        "file",
        "module",
        "namespace",
        "package",
        "class",
        "method",
        "property",
        "field",
        "constructor",
        "enum",
        "interface",
        "function",
        "variable",
        "constant",
        "string",
        "number",
        "boolean",
        "array",
        "object",
        "key",
        "null",
        "enum_member",
        "struct",
        "event",
        "operator",
        "type_parameter",
    ];
    kinds
        .iter()
        .map(|value| {
            value
                .as_u64()
                .filter(|kind| (1..=26).contains(kind))
                .map(|kind| names[kind as usize - 1])
                .context("Invalid standard LSP SymbolKind integer")
        })
        .collect::<Result<Vec<_>>>()
        .map(Some)
}
fn symbol_pages(
    session: &mut Session,
    mut args: Value,
    max_matches: Option<usize>,
    project: Option<&str>,
    deadline: &Deadline,
) -> Result<Value> {
    args["limit"] = json!(200);
    args["offset"] = json!(0);
    let mut value = call(session, "symbols.find", args.clone(), project, deadline)?;
    args["snapshotId"] = value["snapshotId"].clone();
    let total = value["total"]
        .as_u64()
        .context("Symbol match count unavailable")? as usize;
    if let Some(max) = max_matches {
        let preview: Vec<_> = value["items"]
            .as_array()
            .context("Symbol items missing")?
            .iter()
            .take(3)
            .map(|item| json!({"path":item["path"],"namePath":item["namePath"]}))
            .collect();
        ensure!(
            total <= max,
            "Found {total} root matches, exceeding max_matches={max}; refine the search. Shortened result: {}",
            serde_json::to_string(&preview)?
        );
    }
    ensure!(
        total <= 10000,
        "Symbol result exceeds native 10000-item capacity; refine the search"
    );
    let mut items = std::mem::take(
        value["items"]
            .as_array_mut()
            .context("Symbol items missing")?,
    );
    let mut bytes = serde_json::to_vec(&items)?.len();
    ensure!(
        bytes <= MAX_FRAME,
        "Symbol result byte capacity exceeded; refine the search"
    );
    while items.len() < total {
        deadline.check()?;
        args["offset"] = json!(items.len());
        let mut page = call(session, "symbols.find", args.clone(), project, deadline)?;
        ensure!(
            page["snapshotId"] == value["snapshotId"] && page["total"] == total,
            "Symbol pagination source or match count changed"
        );
        let next = page["items"]
            .as_array_mut()
            .filter(|items| !items.is_empty())
            .context("Symbol page made no progress")?;
        for item in std::mem::take(next) {
            bytes = bytes
                .checked_add(serde_json::to_vec(&item)?.len() + 1)
                .context("Symbol result capacity overflow")?;
            ensure!(
                bytes <= MAX_FRAME,
                "Symbol result byte capacity exceeded; refine the search"
            );
            items.push(item);
        }
    }
    ensure!(
        items.len() == total,
        "Symbol result count differs from its source query"
    );
    value["items"] = json!(items);
    value["truncated"] = json!(
        value["omittedPaths"]
            .as_array()
            .is_some_and(|paths| !paths.is_empty())
    );
    Ok(value)
}

fn find_symbols_facade(
    session: &mut Session,
    method: &str,
    args: &Value,
    project: Option<&str>,
    deadline: &Deadline,
) -> Result<Value> {
    let max = args
        .get("max_matches")
        .filter(|value| !value.is_null())
        .map(|value| {
            value
                .as_i64()
                .filter(|max| *max >= -1)
                .context("Invalid max_matches threshold")
        })
        .transpose()?
        .unwrap_or(-1);
    let body = args["include_body"] == true;
    let depth = args
        .get("depth")
        .filter(|value| !value.is_null())
        .map(|value| {
            value
                .as_u64()
                .filter(|depth| body || *depth <= 8)
                .context("Invalid symbol descendant depth")
        })
        .transpose()?
        .unwrap_or(0);
    let depth = if body { 0 } else { depth };
    let name = text(args, "name_path_pattern")?;
    ensure!(
        method != "jb.find_symbol" || name.chars().any(|ch| ch != '*' && ch != '/'),
        "Wildcard-only symbol patterns require get_symbols_overview"
    );
    let pattern = SymbolNamePattern::new(name, args["substring_matching"] == true)?;
    let mut target = json!({"namePath":pattern.backend,"depth":0});
    for (from, to) in [("relative_path", "relativePath")] {
        if !args[from].is_null() {
            move_field(args, &mut target, from, to);
        }
    }
    for (from, to) in [
        ("include_kinds", "includeKinds"),
        ("exclude_kinds", "excludeKinds"),
    ] {
        if let Some(kinds) = symbol_kind_filter(args, from)? {
            target[to] = json!(kinds);
        }
    }
    let mut roots = symbol_pages(session, target.clone(), None, project, deadline)?;
    format_symbol_paths(session, &mut roots, Some(&pattern), project, deadline)?;
    // The public dependency switch belongs to the jb facade. It selects only
    // already proven host captures and never widens capture or provider authority.
    let dependency_scope = (method == "jb.find_symbol").then_some(args["search_deps"] == true);
    if let Some(include) = dependency_scope {
        filter_dependency_symbols(session, &mut roots, include, project, deadline)?;
    }
    if max >= 0 {
        check_symbol_threshold(&roots, max as usize)?;
    }
    let root_ids: std::collections::BTreeSet<_> = roots["items"]
        .as_array()
        .context("Root symbol items missing")?
        .iter()
        .map(|item| text(item, "id").map(str::to_owned))
        .collect::<Result<_>>()?;
    target["snapshotId"] = roots["snapshotId"].clone();
    target["depth"] = json!(depth);
    let mut value = if depth == 0 || root_ids.is_empty() {
        roots
    } else {
        symbol_pages(session, target, None, project, deadline)?
    };
    if depth > 0 && !root_ids.is_empty() {
        retain_permitted_descendants(
            session,
            &mut value,
            &root_ids,
            depth as usize,
            project,
            deadline,
        )?;
        format_symbol_paths(session, &mut value, None, project, deadline)?;
        if let Some(include) = dependency_scope {
            filter_dependency_symbols(session, &mut value, include, project, deadline)?;
        }
    }
    if body {
        attach_symbol_bodies(session, &mut value, project, deadline)?;
    } else if args["include_info"] == true {
        let mut budget =
            session.symbol_information_budget(project, method == "lsp.find_symbol", deadline)?;
        for item in value["items"]
            .as_array_mut()
            .context("Symbol items missing")?
        {
            deadline.check()?;
            if item["id"].as_str().is_some_and(|id| root_ids.contains(id)) {
                let anchor = item.clone();
                attach_optional_symbol_info(
                    session,
                    &anchor,
                    item,
                    &mut budget,
                    project,
                    deadline,
                )?;
            }
        }
        budget.qualify(&mut value);
    }
    value["rootMatchCount"] = json!(root_ids.len());
    value["matchLimitSemantics"] = json!("maximum_root_matches_before_descendant_expansion");
    Ok(value)
}

struct SymbolNamePattern {
    absolute: bool,
    components: Vec<(regex::Regex, Option<usize>)>,
    backend: String,
}
impl SymbolNamePattern {
    fn new(name: &str, substring: bool) -> Result<Self> {
        ensure!(
            !name.is_empty() && name.len() <= 4096 && !name.contains('\0'),
            "Invalid symbol name path"
        );
        let absolute = name.starts_with('/');
        let parts: Vec<_> = name.strip_prefix('/').unwrap_or(name).split('/').collect();
        ensure!(
            parts.len() <= 64 && parts.iter().all(|part| !part.is_empty()),
            "Invalid symbol name path components"
        );
        let mut components = vec![];
        let mut backend = vec![];
        for (position, part) in parts.iter().enumerate() {
            let (base, ordinal) = overload_component(part)?;
            let base = if substring && position + 1 == parts.len() {
                format!("*{base}*")
            } else {
                base.to_owned()
            };
            let expression = base
                .split('*')
                .map(regex::escape)
                .collect::<Vec<_>>()
                .join(".*");
            components.push((
                regex::RegexBuilder::new(&format!("^(?:{expression})$"))
                    .size_limit(1_000_000)
                    .dfa_size_limit(1_000_000)
                    .build()?,
                ordinal,
            ));
            backend.push(base);
        }
        Ok(Self {
            absolute,
            components,
            backend: format!("{}{}", if absolute { "/" } else { "" }, backend.join("/")),
        })
    }
    fn matches(&self, path: &str) -> Result<bool> {
        let parts: Vec<_> = path.split('/').collect();
        if parts.len() < self.components.len()
            || self.absolute && parts.len() != self.components.len()
        {
            return Ok(false);
        }
        for ((matcher, ordinal), part) in self
            .components
            .iter()
            .zip(&parts[parts.len() - self.components.len()..])
        {
            let (base, actual) = overload_component(part)?;
            if !matcher.is_match(base) || ordinal.is_some_and(|ordinal| actual != Some(ordinal)) {
                return Ok(false);
            }
        }
        Ok(true)
    }
}
fn overload_component(component: &str) -> Result<(&str, Option<usize>)> {
    if let Some((base, suffix)) = component
        .strip_suffix(']')
        .and_then(|part| part.rsplit_once('['))
        && !suffix.is_empty()
        && suffix.bytes().all(|byte| byte.is_ascii_digit())
    {
        let ordinal = suffix
            .parse::<usize>()
            .context("Invalid symbol overload index")?;
        ensure!(
            !base.is_empty() && ordinal < 100_000,
            "Invalid symbol overload index"
        );
        return Ok((base, Some(ordinal)));
    }
    Ok((component, None))
}

struct SourceSymbolPaths<'a> {
    by_id: std::collections::BTreeMap<&'a str, &'a crate::model::Entity>,
    by_key: std::collections::BTreeMap<&'a str, &'a crate::model::Entity>,
    ordinals: std::collections::BTreeMap<&'a str, usize>,
}
impl<'a> SourceSymbolPaths<'a> {
    fn new(graph: &'a crate::model::Graph, deadline: &Deadline) -> Result<Self> {
        ensure!(
            graph.entities.len() <= 100_000,
            "Symbol path graph capacity exceeded"
        );
        let mut by_id = std::collections::BTreeMap::new();
        let mut by_key = std::collections::BTreeMap::new();
        let mut groups = std::collections::BTreeMap::<_, Vec<&crate::model::Entity>>::new();
        for entity in &graph.entities {
            deadline.check()?;
            ensure!(
                by_id.insert(entity.id.as_str(), entity).is_none()
                    && by_key.insert(entity.key.as_str(), entity).is_none(),
                "Duplicate canonical symbol identity"
            );
            if entity.kind != "file" {
                groups
                    .entry((
                        entity.path.as_str(),
                        entity.parent_key.as_deref(),
                        entity.name.as_str(),
                    ))
                    .or_default()
                    .push(entity);
            }
        }
        let mut ordinals = std::collections::BTreeMap::new();
        for group in groups.values_mut().filter(|group| group.len() > 1) {
            deadline.check()?;
            group.sort_by_key(|entity| (entity.start_byte, entity.end_byte, &entity.id));
            for (ordinal, entity) in group.iter().enumerate() {
                ordinals.insert(entity.id.as_str(), ordinal);
            }
        }
        Ok(Self {
            by_id,
            by_key,
            ordinals,
        })
    }
    fn path(&self, entity: &'a crate::model::Entity, deadline: &Deadline) -> Result<String> {
        let mut current = Some(entity);
        let mut names = vec![];
        let mut visited = std::collections::BTreeSet::new();
        let mut bytes = 0usize;
        while let Some(entity) = current {
            deadline.check()?;
            ensure!(
                names.len() < 64 && visited.insert(entity.id.as_str()),
                "Cyclic or excessive symbol ancestry"
            );
            if entity.kind != "file" {
                let name = if let Some(ordinal) = self.ordinals.get(entity.id.as_str()) {
                    format!("{}[{ordinal}]", entity.name)
                } else {
                    entity.name.clone()
                };
                bytes = bytes
                    .checked_add(name.len() + 1)
                    .context("Symbol path size overflow")?;
                ensure!(bytes <= 4096, "Symbol name path capacity exceeded");
                names.push(name);
            }
            current = entity
                .parent_key
                .as_deref()
                .map(|key| {
                    self.by_key
                        .get(key)
                        .copied()
                        .context("Canonical symbol parent missing")
                })
                .transpose()?;
        }
        names.reverse();
        Ok(names.join("/"))
    }
}

/// Derive a canonical source path from the exact graph entity, never its selector.
pub(crate) fn canonical_source_symbol_path(
    graph: &crate::model::Graph,
    entity: &crate::model::Entity,
    deadline: &Deadline,
) -> Result<String> {
    let paths = SourceSymbolPaths::new(graph, deadline)?;
    let captured = paths
        .by_id
        .get(entity.id.as_str())
        .copied()
        .context("Canonical source symbol missing")?;
    deadline.check()?;
    ensure!(
        captured.kind != "file" && serde_json::to_value(captured)? == serde_json::to_value(entity)?,
        "Canonical source symbol differs from the captured graph entity"
    );
    paths.path(captured, deadline)
}

fn format_symbol_paths(
    session: &mut Session,
    value: &mut Value,
    pattern: Option<&SymbolNamePattern>,
    project: Option<&str>,
    deadline: &Deadline,
) -> Result<()> {
    let engine = session.query_engine_at(project, deadline)?;
    let snapshot = engine.store.snapshot(text(value, "snapshotId")?)?;
    let graph = engine.store.graph(&snapshot.id)?;
    crate::query_features::source_scope(&engine.store, &snapshot, &graph, deadline)?;
    let paths = SourceSymbolPaths::new(&graph, deadline)?;
    let mut items = vec![];
    for mut item in std::mem::take(
        value["items"]
            .as_array_mut()
            .context("Symbol items missing")?,
    ) {
        deadline.check()?;
        let entity = paths
            .by_id
            .get(text(&item, "id")?)
            .context("Canonical symbol missing")?;
        let path = paths.path(entity, deadline)?;
        if pattern
            .map(|pattern| pattern.matches(&path))
            .transpose()?
            .unwrap_or(true)
        {
            item["namePath"] = json!(path);
            items.push(item);
        }
    }
    value["total"] = json!(items.len());
    value["items"] = json!(items);
    value["namePathSemantics"] =
        json!("source_order_zero_based_overloads_and_last_segment_substring");
    Ok(())
}

/// Resolve facade overload paths against the same immutable canonical source tree.
pub(crate) fn source_symbol(
    store: &crate::store::Store,
    snapshot: &crate::model::Snapshot,
    graph: &crate::model::Graph,
    name: &str,
    path: &str,
    deadline: &Deadline,
) -> Result<crate::model::Entity> {
    crate::query_features::source_scope(store, snapshot, graph, deadline)?;
    let filter = crate::query_features::PathFilter::new(&json!({"relativePath":path}))?;
    let pattern = SymbolNamePattern::new(name, false)?;
    let paths = SourceSymbolPaths::new(graph, deadline)?;
    let mut selected = None;
    for entity in &graph.entities {
        deadline.check()?;
        if entity.kind != "file"
            && filter.matches(&entity.path)
            && pattern.matches(&paths.path(entity, deadline)?)?
        {
            ensure!(
                selected.is_none(),
                "Semantic refactoring requires an unambiguous captured symbol"
            );
            selected = Some(entity);
        }
    }
    selected
        .cloned()
        .context("Semantic refactoring requires an unambiguous captured symbol")
}

fn check_symbol_threshold(value: &Value, max: usize) -> Result<()> {
    let items = value["items"]
        .as_array()
        .context("Root symbol items missing")?;
    let preview:Vec<_>=items.iter().take(3).map(|item|json!({"path":item["path"],"namePath":item["namePath"].as_str().unwrap_or("").chars().take(160).collect::<String>()})).collect();
    ensure!(
        items.len() <= max,
        "Found {} root matches, exceeding max_matches={max}; refine the search. Shortened result: {}",
        items.len(),
        serde_json::to_string(&preview)?
    );
    Ok(())
}

fn list_memories(
    session: &mut Session,
    args: &Value,
    project: Option<&str>,
    deadline: &Deadline,
) -> Result<Value> {
    let topic = args
        .get("topic")
        .filter(|value| !value.is_null())
        .map(|value| value.as_str().context("Memory topic must be text"))
        .transpose()?
        .unwrap_or("");
    let explicit_global = topic.strip_prefix("global/");
    let include_global = explicit_global.is_some() || session.global_notes_available();
    let mut items = vec![];
    let mut bytes = 1024usize;
    let mut namespaces = vec![];
    for global in [false, true] {
        if (!global && explicit_global.is_some()) || (global && !include_global) {
            continue;
        }
        deadline.check()?;
        let scope = if global { "global" } else { "project" };
        let value = call(
            session,
            "notes.list",
            json!({"scope":scope,"topic":explicit_global.unwrap_or(topic)}),
            project,
            deadline,
        )?;
        ensure!(value["scope"] == scope, "Memory listing namespace mismatch");
        let listed = value["items"]
            .as_array()
            .context("Memory listing missing")?;
        ensure!(
            listed.len() <= 2000,
            "Memory listing item capacity exceeded"
        );
        for item in listed {
            deadline.check()?;
            ensure!(item["scope"] == scope, "Memory item namespace mismatch");
            let name = text(item, "name")?;
            let mut item = item.clone();
            if global {
                item["name"] = json!(format!("global/{name}"));
            }
            bytes = bytes
                .checked_add(serde_json::to_vec(&item)?.len() + 1)
                .context("Memory listing size overflow")?;
            ensure!(
                bytes <= MAX_FRAME,
                "Memory listing result capacity exceeded"
            );
            items.push(item);
        }
        namespaces.push(scope);
    }
    items.sort_by(|left, right| left["name"].as_str().cmp(&right["name"].as_str()));
    deadline.check()?;
    Ok(
        json!({"items":items,"scope":if explicit_global.is_some(){"global"}else if include_global{"project_and_global"}else{"project"},"namespaces":namespaces}),
    )
}

fn filter_dependency_symbols(
    session: &mut Session,
    value: &mut Value,
    include: bool,
    project: Option<&str>,
    deadline: &Deadline,
) -> Result<()> {
    let engine = session.query_engine_at(project, deadline)?;
    let snapshot = engine.store.snapshot(text(value, "snapshotId")?)?;
    let scope = crate::capture::source_scope_policy(&engine.store, &snapshot)?;
    let mut dependencies = std::collections::BTreeSet::new();
    for file in &snapshot.files {
        deadline.check()?;
        if scope.path_policy(&file.path)?.read_only {
            dependencies.insert(file.path.as_str());
        }
    }
    let items = value["items"]
        .as_array_mut()
        .context("Symbol items missing")?;
    for item in items.iter() {
        deadline.check()?;
        let file = scope.file(text(item, "path")?)?;
        ensure!(
            item["snapshotId"] == snapshot.id && item["contentHash"] == file.hash,
            "Dependency symbol source binding mismatch"
        );
    }
    if !include {
        items.retain(|item| !dependencies.contains(item["path"].as_str().unwrap()));
    }
    value["total"] = json!(value["items"].as_array().unwrap().len());
    value["dependencySearch"] = json!({"requested":include,"capturedReadOnlyPaths":dependencies.len(),"scope":"explicit_host_captured_read_only_sources","uncapturedExternalLibraries":"unavailable_without_independent_host_capture","captureExpanded":false});
    Ok(())
}

fn retain_permitted_descendants(
    session: &mut Session,
    value: &mut Value,
    roots: &std::collections::BTreeSet<String>,
    depth: usize,
    project: Option<&str>,
    deadline: &Deadline,
) -> Result<()> {
    let engine = session.query_engine_at(project, deadline)?;
    let snapshot = engine.store.snapshot(text(value, "snapshotId")?)?;
    let graph = engine.store.graph(&snapshot.id)?;
    crate::query_features::source_scope(&engine.store, &snapshot, &graph, deadline)?;
    let mut by_id = std::collections::BTreeMap::new();
    let mut parents = std::collections::BTreeMap::new();
    for entity in &graph.entities {
        deadline.check()?;
        by_id.insert(entity.id.as_str(), entity);
        parents.insert(entity.key.as_str(), entity);
    }
    let items = value["items"]
        .as_array_mut()
        .context("Symbol descendants missing")?;
    let mut kept = vec![];
    for item in std::mem::take(items) {
        deadline.check()?;
        let entity = by_id
            .get(text(&item, "id")?)
            .context("Symbol descendant canonical entity missing")?;
        let mut allowed = roots.contains(&entity.id);
        let mut parent = entity.parent_key.as_deref();
        for _ in 0..depth {
            let Some(owner) = parent.and_then(|key| parents.get(key)) else {
                break;
            };
            if roots.contains(&owner.id) {
                allowed = true;
                break;
            }
            parent = owner.parent_key.as_deref();
        }
        if allowed {
            kept.push(item);
        }
    }
    *items = kept;
    value["total"] = json!(value["items"].as_array().unwrap().len());
    Ok(())
}

fn attach_symbol_bodies(
    session: &mut Session,
    value: &mut Value,
    project: Option<&str>,
    deadline: &Deadline,
) -> Result<()> {
    let engine = session.query_engine_at(project, deadline)?;
    let snapshot = engine.store.snapshot(text(value, "snapshotId")?)?;
    let scope = crate::capture::source_scope_policy(&engine.store, &snapshot)?;
    let mut sources = std::collections::BTreeMap::new();
    let (mut source_bytes, mut body_bytes) = (0usize, 0usize);
    for item in value["items"]
        .as_array_mut()
        .context("Symbol items missing")?
    {
        deadline.check()?;
        let path = text(item, "path")?;
        let file = scope.file(path)?;
        ensure!(
            item["contentHash"] == file.hash && item["snapshotId"] == snapshot.id,
            "Symbol body source binding mismatch"
        );
        if !sources.contains_key(path) {
            source_bytes = source_bytes
                .checked_add(file.size)
                .context("Symbol body source size overflow")?;
            ensure!(
                source_bytes <= 16_000_000,
                "Symbol body source capacity exceeded"
            );
            sources.insert(
                path.to_owned(),
                String::from_utf8(scope.read_source(path)?)?,
            );
        }
        let start = item["startByte"]
            .as_u64()
            .context("Symbol body start missing")? as usize;
        let end = item["endByte"]
            .as_u64()
            .context("Symbol body end missing")? as usize;
        crate::paths::utf8_range(&sources[path], start, end)?;
        let source = &sources[path][start..end];
        body_bytes = body_bytes
            .checked_add(source.len())
            .context("Symbol body result size overflow")?;
        ensure!(
            body_bytes <= MAX_FRAME,
            "Full symbol bodies exceed result capacity; refine the search"
        );
        item["body"] = json!(source);
        item["bodyEndByte"] = json!(end);
        item["bodyTruncated"] = json!(false);
    }
    Ok(())
}

fn symbols_overview(
    session: &mut Session,
    args: &Value,
    project: Option<&str>,
    deadline: &Deadline,
) -> Result<Value> {
    let path = text(args, "relative_path")?;
    let requested = args
        .get("depth")
        .filter(|value| !value.is_null())
        .map(|value| {
            value
                .as_i64()
                .filter(|depth| (-1..=64).contains(depth))
                .context("Invalid overview descendant depth")
        })
        .transpose()?
        .unwrap_or(-1);
    let mut value = symbol_pages(
        session,
        json!({"relativePath":path,"depth":0}),
        None,
        project,
        deadline,
    )?;
    let engine = session.query_engine_at(project, deadline)?;
    let snapshot = engine.store.snapshot(text(&value, "snapshotId")?)?;
    let language = crate::capture::validate_language(&engine.store, &snapshot, path)?;
    let depth = if requested < 0 {
        usize::from(matches!(language.as_str(), "java" | "kotlin"))
    } else {
        requested as usize
    };
    let items = value["items"]
        .as_array()
        .context("Overview items missing")?;
    let parents: std::collections::BTreeMap<_, _> = items
        .iter()
        .map(|item| {
            Ok((
                text(item, "key")?.to_owned(),
                item["parentKey"].as_str().map(str::to_owned),
            ))
        })
        .collect::<Result<_>>()?;
    value["items"].as_array_mut().unwrap().retain(|item| {
        let mut level = 0usize;
        let mut parent = item["parentKey"].as_str();
        while let Some(key) = parent {
            let Some(next) = parents.get(key) else { break };
            level += 1;
            if level > depth {
                return false;
            }
            parent = next.as_deref();
        }
        true
    });
    value["total"] = json!(value["items"].as_array().unwrap().len());
    value["overviewDepth"] = json!(depth);
    format_symbol_paths(session, &mut value, None, project, deadline)?;
    Ok(value)
}

fn location_symbol(
    session: &mut Session,
    location: &Value,
    snapshot_id: &str,
    project: Option<&str>,
    deadline: &Deadline,
) -> Result<Option<Value>> {
    let path = text(location, "path")?;
    let start = location["startByte"]
        .as_u64()
        .context("Implementation selection missing")? as usize;
    let end = location["endByte"]
        .as_u64()
        .context("Implementation end missing")? as usize;
    let engine = session.query_engine_at(project, deadline)?;
    let snapshot = engine.store.snapshot(snapshot_id)?;
    let (source, hash) = crate::query::source(&engine.store, &snapshot, path)?;
    ensure!(
        location["contentHash"] == hash,
        "Implementation source hash mismatch"
    );
    crate::paths::utf8_range(&source, start, end)?;
    let graph = engine.store.graph(snapshot_id)?;
    ensure!(
        graph.snapshot_id == snapshot_id,
        "Implementation graph snapshot mismatch"
    );
    let mut candidates = vec![];
    for entity in &graph.entities {
        deadline.check()?;
        if entity.kind != "file"
            && entity.path == path
            && entity.start_byte <= start
            && end <= entity.end_byte
        {
            let selection =
                entity.name_start_byte == Some(start) && entity.name_end_byte == Some(end);
            candidates.push((!selection, entity.end_byte - entity.start_byte, entity));
        }
    }
    candidates.sort_by_key(|(not_selection, size, entity)| (*not_selection, *size, &entity.id));
    let Some((not_selection, size, entity)) = candidates.first() else {
        return Ok(None);
    };
    if candidates
        .get(1)
        .is_some_and(|(other_selection, other_size, _)| {
            other_selection == not_selection && other_size == size
        })
    {
        return Ok(None);
    }
    let mut value = serde_json::to_value(entity)?;
    value["contentHash"] = json!(hash);
    Ok(Some(value))
}
fn python_docstring(node: tree_sitter::Node<'_>, text: &str, depth: usize) -> bool {
    if depth > 64 {
        return false;
    }
    if node.kind() == "string" {
        let Ok(value) = node.utf8_text(text.as_bytes()) else {
            return false;
        };
        let prefix = value.split(['\'', '"']).next().unwrap_or("");
        return prefix
            .chars()
            .all(|character| matches!(character, 'r' | 'R' | 'u' | 'U'));
    }
    if node.kind() == "parenthesized_expression" && node.named_child_count() == 1 {
        return node
            .named_child(0)
            .is_some_and(|child| python_docstring(child, text, depth + 1));
    }
    if node.kind() == "concatenated_string" {
        let mut cursor = node.walk();
        let children: Vec<_> = node.named_children(&mut cursor).collect();
        return !children.is_empty()
            && children
                .iter()
                .all(|child| python_docstring(*child, text, depth + 1));
    }
    false
}
fn file_documentation(
    session: &mut Session,
    path: &str,
    snapshot_id: &str,
    project: Option<&str>,
    deadline: &Deadline,
) -> Result<Value> {
    let source = call(
        session,
        "fs.read",
        json!({"path":path,"snapshotId":snapshot_id,"maxBytes":MAX_FRAME}),
        project,
        deadline,
    )?;
    let text = source["text"]
        .as_str()
        .context("File documentation requires complete captured source within the frame budget")?;
    ensure!(
        source["snapshotId"] == snapshot_id
            && source["startByte"] == 0
            && source["endByte"] == text.len(),
        "File documentation source incomplete or rebound"
    );
    let engine = session.query_engine_at(project, deadline)?;
    let snapshot = engine.store.snapshot(snapshot_id)?;
    let language = crate::capture::validate_language(&engine.store, &snapshot, path)?;
    let Some(grammar) = crate::languages::grammar(&language) else {
        return Ok(Value::Null);
    };
    let mut parser = tree_sitter::Parser::new();
    parser.set_language(&grammar)?;
    let mut cancelled = |_: &tree_sitter::ParseState| deadline.check().is_err();
    let tree = parser
        .parse_with_options(
            &mut |offset, _| &text.as_bytes()[offset.min(text.len())..],
            None,
            Some(tree_sitter::ParseOptions::new().progress_callback(&mut cancelled)),
        )
        .context("File documentation parser cancelled")?;
    let mut cursor = tree.root_node().walk();
    let mut range = None;
    for node in tree.root_node().named_children(&mut cursor) {
        deadline.check()?;
        let module_docstring = language == "python"
            && node.kind() == "expression_statement"
            && node
                .named_child(0)
                .is_some_and(|child| python_docstring(child, text, 0));
        if !node.kind().contains("comment") && !module_docstring {
            break;
        }
        let (start, _) = range.unwrap_or((node.start_byte(), node.end_byte()));
        range = Some((start, node.end_byte()));
        if module_docstring {
            break;
        }
    }
    let Some((start, end)) = range else {
        return Ok(Value::Null);
    };
    let documentation = text
        .get(start..end)
        .context("File documentation source range invalid")?;
    ensure!(
        documentation.len() <= 128000,
        "File documentation exceeds source byte budget"
    );
    Ok(
        json!({"snapshotId":snapshot_id,"path":path,"contentHash":source["contentHash"],"startByte":start,"endByte":end,"text":documentation,"producer":"native_leading_source_comments_or_python_module_docstring","sourceBound":true}),
    )
}

#[cfg(test)]
mod source_binding_tests {
    use super::*;
    #[test]
    fn provider_queries_keep_the_symbol_snapshot_after_original_source_changes() {
        let root = tempfile::tempdir().unwrap();
        let source = "// 🌍\nfn helper() {}\nfn main() { helper(); }\n";
        fs::write(root.path().join("main.rs"), source).unwrap();
        let setup = Deadline::new(60000);
        let mut engine = crate::Engine::open(root.path()).unwrap();
        engine
            .register_configured_server(crate::lsp::ConfiguredServer {
                server: crate::lsp::Server {
                    command: "node".into(),
                    args: vec![
                        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                            .join("tests/fixtures/advanced-lsp.mjs")
                            .display()
                            .to_string(),
                        "".into(),
                    ],
                    language: "rust".into(),
                    trust: true,
                },
                configuration: crate::lsp::HostConfiguration {
                    passive_profile: Some(crate::lsp::PassiveProfile::ContractFixture),
                    ..Default::default()
                },
            })
            .unwrap();
        let mut session = Session::open(
            engine,
            crate::project_session::HostProfile::default(),
            Some("source-bound-repl"),
            &setup,
        )
        .unwrap();
        let symbol = one_symbol(&mut session, "helper", "main.rs", None, &setup).unwrap();
        fs::write(
            root.path().join("main.rs"),
            source.replace("helper", "holder"),
        )
        .unwrap();
        // Query the already captured symbol under its own unchanged 20s
        // operation budget; host/store setup is not part of this assertion.
        let deadline = Deadline::new(20000);
        let value = semantic_from_symbol(
            &mut session,
            &symbol,
            "implementation",
            json!({}),
            None,
            &deadline,
        )
        .unwrap();
        assert_eq!(
            value["snapshotId"], symbol["snapshotId"],
            "Semantic lookup recaptured source at an old symbol offset"
        );
        assert_eq!(value["contentHash"], symbol["contentHash"]);
    }
}
