//! Explicit host-authorized Python debugging. Source never grants runtime/eval authority.
use crate::{
    capture::Deadline,
    model::{Snapshot, identity},
    paths::{line_starts, normalized},
    process::ManagedChild,
    store::Store,
};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    fs,
    io::{Read, Write},
    path::PathBuf,
    process::{Command, Stdio},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread,
    time::Duration,
};
const MAX_FRAME: usize = 262_144;
const MAX_PROTOCOL: usize = 8_000_000;
const MAX_INPUT: usize = 8_000_000;
#[derive(Clone, Debug)]
pub struct RuntimeSpec {
    pub python: PathBuf,
    pub trust_runtime: bool,
    pub allow_evaluation: bool,
    pub max_output_bytes: usize,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Breakpoint {
    pub path: String,
    pub content_hash: String,
    pub line: usize,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "command", rename_all = "snake_case", deny_unknown_fields)]
pub enum Request {
    Stack,
    Variables { frame: usize },
    Breakpoints { items: Vec<Breakpoint> },
    Step,
    Next,
    Continue,
    Evaluate { expression: String, frame: usize },
    Repl { code: String, frame: usize },
    Close,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Reply {
    schema: u8,
    session_id: String,
    request_id: u64,
    event: String,
    data: Value,
}
enum Event {
    Frame(Vec<u8>),
    Stderr(Vec<u8>),
    Written(u64),
    Failure(String),
    Eof,
    Exited,
}
pub struct Session<'a> {
    _store: &'a Store,
    snapshot: Snapshot,
    sources: BTreeMap<String, String>,
    root: PathBuf,
    state: Value,
    id: String,
    sequence: u64,
    allow_evaluation: bool,
    active: bool,
    deadline: Deadline,
    writer: mpsc::SyncSender<(u64, Vec<u8>)>,
    events: mpsc::Receiver<Event>,
    cancel: Arc<AtomicBool>,
    stopped: mpsc::Receiver<bool>,
    output_limit: usize,
    output_bytes: usize,
    _directory: tempfile::TempDir,
}
impl<'a> Session<'a> {
    pub fn start(
        store: &'a Store,
        snapshot: &Snapshot,
        entry: &str,
        spec: &RuntimeSpec,
        deadline: &Deadline,
    ) -> Result<Self> {
        ensure!(
            spec.trust_runtime,
            "Explicit host runtime authority is required"
        );
        ensure!(
            spec.python.is_absolute(),
            "Python must be an absolute host interpreter path"
        );
        ensure!(
            (1..=4_000_000).contains(&spec.max_output_bytes),
            "Invalid runtime output bound"
        );
        deadline.check()?;
        store.set_deadline(deadline)?;
        ensure!(
            snapshot.workspace_id == store.workspace_id
                && snapshot.kind != "federated"
                && identity(&store.snapshot(&snapshot.id)?) == identity(snapshot),
            "Runtime snapshot scope mismatch"
        );
        normalized(entry)?;
        let selected = snapshot
            .files
            .iter()
            .find(|f| f.path == entry)
            .context("Runtime entry absent from snapshot")?;
        ensure!(
            selected.language == "python" && entry.ends_with(".py"),
            "Runtime supports captured Python scripts only"
        );
        let interpreter = spec
            .python
            .canonicalize()
            .context("Host Python interpreter unavailable")?;
        ensure!(
            interpreter.is_file()
                && !interpreter.starts_with(
                    store
                        .dir
                        .parent()
                        .context("Workspace root absent")?
                        .canonicalize()?
                ),
            "Host interpreter must be outside analyzed source"
        );
        ensure!(
            snapshot.files.len() <= 2000
                && snapshot
                    .files
                    .iter()
                    .try_fold(0usize, |a, f| a.checked_add(f.size))
                    .is_some_and(|n| n <= 16_000_000),
            "Runtime snapshot capacity exceeded"
        );
        let mut sources = BTreeMap::new();
        for file in &snapshot.files {
            deadline.check()?;
            if file.language == "python" {
                let text = String::from_utf8(store.read_blob(&file.hash)?)?;
                ensure!(
                    text.len() == file.size,
                    "Runtime captured source size mismatch"
                );
                sources.insert(file.path.clone(), text);
            }
        }
        let directory = tempfile::Builder::new().prefix("yoke-runtime-").tempdir()?;
        let root = directory.path().join("tree");
        crate::changes::materialize(store, snapshot, &root, deadline)?;
        ensure!(
            crate::verify::snapshot_integrity(snapshot, &root, deadline)?,
            "Runtime source copy integrity failure"
        );
        let worker = directory.path().join("worker.py");
        fs::write(&worker, include_bytes!("runtime_worker.py"))?;
        let mut random = [0; 32];
        getrandom::fill(&mut random)?;
        let id = hex::encode(random);
        let files: BTreeMap<_, _> = snapshot
            .files
            .iter()
            .map(|f| (&f.path, json!({"hash":f.hash,"size":f.size})))
            .collect();
        let initial = json!({"schema":1,"command":"init","sessionId":id,"requestId":0,"root":root,"entry":entry,"allowEvaluation":spec.allow_evaluation,"maxOutputBytes":spec.max_output_bytes,"files":files});
        let mut command = Command::new(interpreter);
        command
            .args(["-I", "-B", "-u"])
            .arg(worker)
            .current_dir(&root)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .env_clear();
        for name in [
            "PATH",
            "SystemRoot",
            "WINDIR",
            "USERPROFILE",
            "HOME",
            "TEMP",
            "TMP",
            "LANG",
        ] {
            if let Some(value) = std::env::var_os(name) {
                command.env(name, value);
            }
        }
        command.env("CI", "1").env("DO_NOT_TRACK", "1");
        let cancel = Arc::new(AtomicBool::new(false));
        let owner_cancel = cancel.clone();
        let owner_deadline = deadline.clone();
        let (startup, started) = mpsc::sync_channel(1);
        let (event_tx, events) = mpsc::sync_channel(256);
        let owner_events = event_tx.clone();
        let (stopped_tx, stopped) = mpsc::sync_channel(1);
        // The process owner never crosses a thread boundary: process-wrap's child
        // wrapper is not Send. This thread also enforces an idle session deadline.
        thread::spawn(move || {
            let result = (|| -> Result<bool> {
                let mut child =
                    ManagedChild::spawn(command).context("Python runtime unavailable")?;
                let pipes = (
                    child.stdin().take().context("Runtime stdin unavailable")?,
                    child
                        .stdout()
                        .take()
                        .context("Runtime stdout unavailable")?,
                    child
                        .stderr()
                        .take()
                        .context("Runtime stderr unavailable")?,
                );
                if startup.send(Ok(pipes)).is_err() {
                    return Ok(false);
                }
                loop {
                    if owner_cancel.load(Ordering::SeqCst) || owner_deadline.check().is_err() {
                        break;
                    }
                    if child.inner_mut().try_wait()?.is_some() {
                        let _ = owner_events.try_send(Event::Exited);
                        break;
                    }
                    thread::sleep(owner_deadline.remaining().min(Duration::from_millis(10)));
                }
                let killed = child.start_kill();
                if let Err(error) = killed {
                    #[cfg(unix)]
                    if error.raw_os_error() == Some(3) {
                        return Ok(true);
                    }
                    return Err(error.into());
                }
                child.inner_mut().wait()?;
                Ok(true)
            })();
            if let Err(error) = &result {
                let _ = startup.try_send(Err(error.to_string()));
                let _ = owner_events.try_send(Event::Failure(error.to_string()));
            }
            let _ = stopped_tx.send(result.unwrap_or(false));
        });
        let (mut stdin, mut stdout, mut stderr) = match started.recv_timeout(deadline.remaining()) {
            Ok(Ok(pipes)) => pipes,
            Ok(Err(error)) => anyhow::bail!(error),
            Err(_) => {
                cancel.store(true, Ordering::SeqCst);
                anyhow::bail!("Runtime startup deadline exceeded")
            }
        };
        let (writer, queue) = mpsc::sync_channel::<(u64, Vec<u8>)>(1);
        let writes = event_tx.clone();
        thread::spawn(move || {
            while let Ok((id, bytes)) = queue.recv() {
                if let Err(error) = stdin.write_all(&bytes).and_then(|()| stdin.flush()) {
                    let _ = writes.send(Event::Failure(format!("Runtime stdin failed: {error}")));
                    break;
                }
                if writes.send(Event::Written(id)).is_err() {
                    break;
                }
            }
        });
        let frames = event_tx.clone();
        thread::spawn(move || {
            let result = (|| -> Result<()> {
                let mut total = 0usize;
                loop {
                    let mut header = [0; 4];
                    match stdout.read_exact(&mut header) {
                        Ok(()) => {}
                        Err(error) if error.kind() == std::io::ErrorKind::UnexpectedEof => {
                            let _ = frames.send(Event::Eof);
                            return Ok(());
                        }
                        Err(error) => return Err(error.into()),
                    }
                    let length = u32::from_be_bytes(header) as usize;
                    ensure!(length <= MAX_FRAME, "Runtime response frame exceeded bound");
                    total = total
                        .checked_add(length + 4)
                        .context("Runtime protocol size overflow")?;
                    ensure!(
                        total <= MAX_PROTOCOL,
                        "Runtime lifetime protocol output exceeded bound"
                    );
                    let mut bytes = vec![0; length];
                    stdout.read_exact(&mut bytes)?;
                    if frames.send(Event::Frame(bytes)).is_err() {
                        return Ok(());
                    }
                }
            })();
            if let Err(error) = result {
                let _ = frames.send(Event::Failure(error.to_string()));
            }
        });
        let stderr_events = event_tx;
        thread::spawn(move || {
            let result = (|| -> Result<()> {
                let mut total = 0usize;
                let mut buffer = [0; 4096];
                loop {
                    let count = stderr.read(&mut buffer)?;
                    if count == 0 {
                        return Ok(());
                    }
                    total += count;
                    ensure!(total <= 32_000, "Runtime raw stderr exceeded bound");
                    if stderr_events
                        .send(Event::Stderr(buffer[..count].to_vec()))
                        .is_err()
                    {
                        return Ok(());
                    }
                }
            })();
            if let Err(error) = result {
                let _ = stderr_events.send(Event::Failure(error.to_string()));
            }
        });
        let mut session = Self {
            _store: store,
            snapshot: snapshot.clone(),
            sources,
            root,
            state: json!({}),
            id,
            sequence: 0,
            allow_evaluation: spec.allow_evaluation,
            active: true,
            deadline: deadline.clone(),
            writer,
            events,
            cancel,
            stopped,
            output_limit: spec.max_output_bytes,
            output_bytes: 0,
            _directory: directory,
        };
        let result = session.exchange(initial, 0);
        if let Err(error) = result {
            session.stop();
            return Err(error);
        }
        session.state = result?;
        Ok(session)
    }
    pub fn state(&self) -> &Value {
        &self.state
    }
    fn stop(&mut self) -> bool {
        self.active = false;
        self.cancel.store(true, Ordering::SeqCst);
        self.stopped
            .recv_timeout(Duration::from_secs(2))
            .unwrap_or(false)
    }
    fn validate(&self, request: &Request) -> Result<()> {
        let frames = self.state["frames"].as_array().map_or(0, Vec::len);
        let frame = match request {
            Request::Variables { frame }
            | Request::Evaluate { frame, .. }
            | Request::Repl { frame, .. } => Some(*frame),
            _ => None,
        };
        if let Some(frame) = frame {
            ensure!(frame < frames, "Requested frame absent from current pause");
        }
        match request {
            Request::Evaluate { expression, .. } => {
                ensure!(
                    self.allow_evaluation,
                    "Separate host evaluation authority is required"
                );
                code(expression)?;
            }
            Request::Repl { code: text, .. } => {
                ensure!(
                    self.allow_evaluation,
                    "Separate host evaluation authority is required"
                );
                code(text)?;
            }
            Request::Breakpoints { items } => {
                ensure!(items.len() <= 100, "Breakpoint capacity exceeded");
                let mut seen = std::collections::BTreeSet::new();
                for b in items {
                    normalized(&b.path)?;
                    let file = self
                        .snapshot
                        .files
                        .iter()
                        .find(|f| f.path == b.path)
                        .context("Breakpoint path absent from snapshot")?;
                    ensure!(
                        file.language == "python"
                            && file.hash == b.content_hash
                            && seen.insert((&b.path, b.line)),
                        "Breakpoint hash/language/identity mismatch"
                    );
                    let text = self
                        .sources
                        .get(&b.path)
                        .context("Breakpoint source unavailable")?;
                    ensure!(
                        b.line >= 1 && b.line <= line_starts(text).len(),
                        "Breakpoint line outside source"
                    );
                }
            }
            _ => {}
        }
        Ok(())
    }
    pub fn request(&mut self, request: Request) -> Result<Value> {
        ensure!(self.active, "Runtime session is closed");
        if let Err(error) = self.deadline.check() {
            self.stop();
            return Err(error);
        }
        self.validate(&request)?;
        self.sequence = self
            .sequence
            .checked_add(1)
            .context("Runtime request sequence exhausted")?;
        let mut value = serde_json::to_value(request)?;
        value["schema"] = json!(1);
        value["sessionId"] = json!(self.id);
        value["requestId"] = json!(self.sequence);
        ensure!(
            serde_json::to_vec(&value)?.len() <= 65_536,
            "Runtime command exceeded bound"
        );
        let result = self.exchange(value, self.sequence);
        match result {
            Ok(value) => {
                self.state = value.clone();
                Ok(value)
            }
            Err(error) => {
                self.stop();
                Err(error)
            }
        }
    }
    fn exchange(&mut self, value: Value, id: u64) -> Result<Value> {
        self.deadline.check()?;
        ensure!(
            crate::verify::snapshot_integrity(&self.snapshot, &self.root, &self.deadline)?,
            "Runtime copied source changed before request"
        );
        let payload = serde_json::to_vec(&value)?;
        ensure!(
            payload.len() <= MAX_INPUT,
            "Runtime input exceeded capacity"
        );
        let mut bytes = Vec::with_capacity(payload.len() + 4);
        bytes.extend_from_slice(&(payload.len() as u32).to_be_bytes());
        bytes.extend_from_slice(&payload);
        self.writer
            .try_send((id, bytes))
            .context("Runtime stdin queue unavailable")?;
        let mut written = false;
        let mut response = None;
        let mut output = Vec::new();
        let mut output_frames = 0usize;
        loop {
            self.deadline.check()?;
            if written && response.is_some() {
                break;
            }
            match self
                .events
                .recv_timeout(self.deadline.remaining().min(Duration::from_millis(20)))
            {
                Ok(Event::Written(sequence)) => {
                    ensure!(sequence == id, "Runtime stdin request mismatch");
                    written = true;
                }
                Ok(Event::Frame(bytes)) => {
                    let reply: Reply =
                        serde_json::from_slice(&bytes).context("Malformed runtime frame")?;
                    ensure!(
                        reply.schema == 1 && reply.session_id == self.id && reply.request_id == id,
                        "Runtime frame session/request scope mismatch"
                    );
                    match reply.event.as_str() {
                        "output" => {
                            let text = reply.data["text"]
                                .as_str()
                                .context("Runtime output text missing")?;
                            let stream = reply.data["stream"]
                                .as_str()
                                .context("Runtime output stream missing")?;
                            ensure!(
                                matches!(stream, "stdout" | "stderr"),
                                "Unknown runtime output stream"
                            );
                            self.output_bytes = self
                                .output_bytes
                                .checked_add(text.len())
                                .context("Runtime output count overflow")?;
                            ensure!(
                                self.output_bytes <= self.output_limit,
                                "Runtime session output exceeded bound"
                            );
                            output_frames += 1;
                            ensure!(
                                output_frames <= 10_000,
                                "Runtime output event count exceeded bound"
                            );
                            output.push(reply.data);
                        }
                        "paused" | "result" | "finished" | "closed" => {
                            ensure!(response.is_none(), "Duplicate runtime response");
                            response = Some(reply.data)
                        }
                        "error" => anyhow::bail!(
                            "Runtime worker failure: {}",
                            reply.data["errorKind"].as_str().unwrap_or("unknown")
                        ),
                        _ => anyhow::bail!("Unsupported runtime event"),
                    }
                }
                Ok(Event::Stderr(bytes)) => {
                    self.output_bytes += bytes.len();
                    ensure!(
                        self.output_bytes <= self.output_limit,
                        "Runtime session stderr exceeded bound"
                    );
                    output.push(json!({"stream":"raw_stderr","text":String::from_utf8_lossy(&bytes),"encoding":"utf8_lossy"}));
                }
                Ok(Event::Failure(error)) => anyhow::bail!(error),
                Ok(Event::Eof) => {
                    ensure!(
                        response.is_some(),
                        "Runtime ended without a complete response"
                    )
                }
                Ok(Event::Exited) => {}
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(_) => anyhow::bail!("Runtime event stream closed"),
            }
        }
        ensure!(
            crate::verify::snapshot_integrity(&self.snapshot, &self.root, &self.deadline)?,
            "Runtime copied source changed; source evidence invalidated"
        );
        let mut result = response.context("Runtime response absent")?;
        ensure!(result.is_object(), "Runtime result must be an object");
        let status = result["status"]
            .as_str()
            .context("Runtime status missing")?;
        ensure!(
            matches!(status, "paused" | "finished" | "closed"),
            "Unknown runtime status"
        );
        let terminal = status != "paused";
        self.bind_frames(&mut result)?;
        result["snapshotId"] = json!(self.snapshot.id);
        result["workspaceId"] = json!(self.snapshot.workspace_id);
        result["sessionId"] = json!(self.id);
        result["requestId"] = json!(id);
        result["output"] = json!(output);
        result["sourceIntegrity"] = json!(true);
        result["evaluationAllowed"] = json!(self.allow_evaluation);
        result["resolution"] = json!("source_bound_runtime_observation");
        result["verificationAttestation"] = json!(false);
        result["networkIsolation"] = json!("not_enforced_trusted_host_runtime");
        ensure!(
            serde_json::to_vec(&result)?.len() <= MAX_PROTOCOL,
            "Runtime result exceeded capacity"
        );
        if terminal {
            ensure!(
                self.stop(),
                "Runtime process group cleanup was not confirmed"
            );
        }
        Ok(result)
    }
    fn bind_frames(&self, result: &mut Value) -> Result<()> {
        let frames = result["frames"]
            .as_array_mut()
            .context("Runtime frame stack missing")?;
        ensure!(frames.len() <= 20, "Runtime stack capacity exceeded");
        for (index, frame) in frames.iter_mut().enumerate() {
            let path = frame["path"]
                .as_str()
                .context("Runtime frame path missing")?
                .to_string();
            normalized(&path)?;
            ensure!(
                frame["frame"].as_u64() == Some(index as u64),
                "Runtime stack frame identity mismatch"
            );
            let source = self
                .sources
                .get(&path)
                .context("Runtime frame outside captured Python source")?;
            let file = self
                .snapshot
                .files
                .iter()
                .find(|f| f.path == path)
                .context("Runtime frame file absent")?;
            let line = usize::try_from(
                frame["line"]
                    .as_u64()
                    .context("Runtime frame line missing")?,
            )?;
            let starts = line_starts(source);
            ensure!(
                line >= 1 && line <= starts.len(),
                "Runtime frame line outside captured source"
            );
            let start = starts[line - 1];
            let end = starts.get(line).copied().unwrap_or(source.len());
            frame["snapshotId"] = json!(self.snapshot.id);
            frame["contentHash"] = json!(file.hash);
            frame["startByte"] = json!(start);
            frame["endByte"] = json!(end);
            frame["anchor"] = json!("reported_python_line");
        }
        Ok(())
    }
}
fn code(text: &str) -> Result<()> {
    ensure!(
        !text.trim().is_empty()
            && text.len() <= 10_000
            && !text
                .chars()
                .any(|c| c.is_control() && !matches!(c, '\n' | '\r' | '\t')),
        "Invalid bounded runtime expression/statements"
    );
    Ok(())
}
impl Drop for Session<'_> {
    fn drop(&mut self) {
        if self.active {
            self.stop();
        }
    }
}
