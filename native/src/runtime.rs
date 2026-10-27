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

fn receive_startup<T>(
    started: &mpsc::Receiver<std::result::Result<T, String>>,
    stopped: &mpsc::Receiver<Result<()>>,
    cancel: &AtomicBool,
    deadline: &Deadline,
) -> Result<T> {
    let failure = match started.recv_timeout(deadline.remaining()) {
        Ok(Ok(pipes)) => return Ok(pipes),
        Ok(Err(error)) => anyhow::anyhow!(error),
        Err(_) => anyhow::anyhow!("Runtime startup deadline exceeded"),
    };
    cancel.store(true, Ordering::SeqCst);
    // No Session exists yet to request checked shutdown. Failure delivery still
    // requires the owner's receipt, using the existing separate cleanup budget.
    match stopped.recv_timeout(Duration::from_secs(2)) {
        Ok(Ok(())) => Err(failure),
        Ok(Err(owner_error)) => Err(owner_error.context(failure)),
        Err(_) => Err(anyhow::Error::new(crate::process::CleanupUnproven::new(
            "Runtime startup owner cleanup receipt unavailable",
        ))
        .context(failure)),
    }
}

#[cfg(test)]
#[path = "runtime_startup_tests.rs"]
mod startup_tests;

#[cfg(test)]
type ResponseCheck = Box<dyn FnOnce(&mut Deadline) + Send>;

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
    stopped: mpsc::Receiver<Result<()>>,
    output_limit: usize,
    output_bytes: usize,
    _directory: tempfile::TempDir,
    // Unit-only delivery-boundary control; no production host/protocol surface.
    #[cfg(test)]
    before_response_check: Option<ResponseCheck>,
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
            "SystemDrive",
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
            let result = (|| -> Result<()> {
                owner_deadline.check()?;
                ensure!(
                    !owner_cancel.load(Ordering::SeqCst),
                    "Runtime startup cancelled by host"
                );
                let mut child =
                    ManagedChild::spawn(command).context("Python runtime unavailable")?;
                let observation = (|| -> Result<()> {
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
                        return Ok(());
                    }
                    loop {
                        if owner_cancel.load(Ordering::SeqCst) || owner_deadline.check().is_err() {
                            break Ok(());
                        }
                        match child.inner_mut().try_wait() {
                            Ok(Some(_)) => {
                                let _ = owner_events.try_send(Event::Exited);
                                break Ok(());
                            }
                            Ok(None) => {}
                            Err(error) => break Err(error.into()),
                        }
                        thread::sleep(owner_deadline.remaining().min(Duration::from_millis(10)));
                    }
                })();
                // Every post-spawn path, including pipe setup failure, reaches
                // exactly one checked cleanup before the owner sends a receipt.
                match (child.terminate_checked(Duration::from_secs(2)), observation) {
                    (Err(cleanup), Err(error)) => Err(cleanup.context(error)),
                    (Err(cleanup), Ok(())) => Err(cleanup),
                    (Ok(_), result) => result,
                }
            })();
            if let Err(error) = &result {
                let _ = startup.try_send(Err(error.to_string()));
                let _ = owner_events.try_send(Event::Failure(error.to_string()));
            }
            let _ = stopped_tx.send(result);
        });
        let (mut stdin, mut stdout, mut stderr) =
            receive_startup(&started, &stopped, &cancel, deadline)?;
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
            #[cfg(test)]
            before_response_check: None,
        };
        let result = session.exchange(initial, 0);
        if let Err(error) = result {
            return Err(session.refuse_after_stop(error));
        }
        session.state = result?;
        Ok(session)
    }
    pub fn state(&self) -> &Value {
        &self.state
    }
    fn stop(&mut self) -> Result<()> {
        self.active = false;
        self.cancel.store(true, Ordering::SeqCst);
        self.stopped
            .recv_timeout(Duration::from_secs(2))
            .map_err(|_| {
                crate::process::CleanupUnproven::new("Runtime owner cleanup receipt unavailable")
            })?
    }
    fn refuse_after_stop(&mut self, error: anyhow::Error) -> anyhow::Error {
        if self.active
            && let Err(cleanup) = self.stop()
        {
            return cleanup.context(error);
        }
        error
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
            return Err(self.refuse_after_stop(error));
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
            Err(error) => Err(self.refuse_after_stop(error)),
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
        ensure!(
            value["command"] != "close" || status == "closed",
            "Runtime Close response did not confirm closed state"
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
            self.stop()
                .context("Runtime process group cleanup was not confirmed")?;
        }
        #[cfg(test)]
        if let Some(before_check) = self.before_response_check.take() {
            before_check(&mut self.deadline);
        }
        // Cleanup has its own bounded budget, but cannot renew the lifetime
        // deadline or authorize delivery after cancellation or expiry.
        self.deadline.check()?;
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
            let _ = self.stop();
        }
    }
}

#[cfg(test)]
mod response_boundary_tests {
    use super::*;

    // Scripted transport/cleanup receipts isolate response publication. These
    // tests do not replace actual Python or OS process-ownership acceptance.
    struct Fixture {
        engine: crate::Engine,
        snapshot: Snapshot,
        // Drop the Store's Windows file handles before removing its directory.
        _workspace: tempfile::TempDir,
    }

    fn fixture() -> Fixture {
        let workspace = tempfile::tempdir().unwrap();
        fs::write(workspace.path().join("main.py"), "value = 1\n").unwrap();
        let mut engine = crate::Engine::open(workspace.path()).unwrap();
        let (snapshot, _, _, _) = engine.index(&Deadline::new(10_000)).unwrap();
        Fixture {
            _workspace: workspace,
            engine,
            snapshot,
        }
    }

    fn session(
        fixture: &Fixture,
        deadline: Deadline,
        receipt: Option<Result<()>>,
    ) -> (Session<'_>, mpsc::Receiver<(u64, Vec<u8>)>) {
        session_with_response(
            fixture,
            deadline,
            receipt,
            json!({"status": "closed", "frames": [], "programCompleted": false}),
        )
    }

    fn session_with_response(
        fixture: &Fixture,
        deadline: Deadline,
        receipt: Option<Result<()>>,
        response: Value,
    ) -> (Session<'_>, mpsc::Receiver<(u64, Vec<u8>)>) {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().join("tree");
        crate::changes::materialize(
            &fixture.engine.store,
            &fixture.snapshot,
            &root,
            &Deadline::new(10_000),
        )
        .unwrap();
        let id = "owned-response-boundary".to_owned();
        let (writer, queue) = mpsc::sync_channel(1);
        let (event_tx, events) = mpsc::sync_channel(4);
        event_tx.send(Event::Written(1)).unwrap();
        event_tx
            .send(Event::Frame(
                serde_json::to_vec(&json!({
                    "schema": 1,
                    "sessionId": id,
                    "requestId": 1,
                    "event": "closed",
                    "data": response
                }))
                .unwrap(),
            ))
            .unwrap();
        drop(event_tx);
        let (stopped_tx, stopped) = mpsc::sync_channel(1);
        if let Some(result) = receipt {
            stopped_tx.send(result).unwrap();
        }
        drop(stopped_tx);
        (
            Session {
                _store: &fixture.engine.store,
                snapshot: fixture.snapshot.clone(),
                sources: BTreeMap::new(),
                root,
                state: json!({"status": "paused", "frames": []}),
                id,
                sequence: 0,
                allow_evaluation: false,
                active: true,
                deadline,
                writer,
                events,
                cancel: Arc::new(AtomicBool::new(false)),
                stopped,
                output_limit: 1024,
                output_bytes: 0,
                _directory: directory,
                before_response_check: None,
            },
            queue,
        )
    }

    #[test]
    fn nonclosed_close_response_requires_cleanup_and_preserves_original_state() {
        for status in ["paused", "finished"] {
            for cleanup_refused in [false, true] {
                let fixture = fixture();
                let receipt = if cleanup_refused {
                    Err(crate::process::CleanupUnproven::new("owned close receipt refusal").into())
                } else {
                    Ok(())
                };
                let (mut session, queue) = session_with_response(
                    &fixture,
                    Deadline::new(10_000),
                    Some(receipt),
                    json!({"status": status, "frames": []}),
                );
                let original = session.state().clone();
                let result = session.request(Request::Close);
                let active = session.active;
                let cancelled = session.cancel.load(Ordering::SeqCst);
                let state = session.state().clone();
                drop(session);
                drop(queue);
                let error = result.unwrap_err();
                assert!(!active);
                assert!(cancelled);
                assert_eq!(state, original);
                assert_eq!(crate::process::cleanup_unproven(&error), cleanup_refused);
                assert!(
                    format!("{error:#}").contains("Close response did not confirm closed state")
                );
            }
        }
    }

    #[test]
    fn expiry_after_confirmed_cleanup_refuses_terminal_delivery() {
        let fixture = fixture();
        let (mut session, queue) = session(&fixture, Deadline::new(10_000), Some(Ok(())));
        let original = session.state().clone();
        // A scoped logical expiry at the actual post-cleanup boundary avoids a
        // scheduler/sleep assumption. It is not real timing acceptance.
        session.before_response_check = Some(Box::new(|deadline| {
            *deadline = Deadline::new(0);
        }));
        let result = session.request(Request::Close);
        let active = session.active;
        let cancelled = session.cancel.load(Ordering::SeqCst);
        let state = session.state().clone();
        drop(session);
        drop(queue);
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("deadline exceeded")
        );
        assert!(!active);
        assert!(cancelled);
        assert_eq!(state, original);
    }

    #[test]
    fn cancellation_after_confirmed_cleanup_preserves_the_original_deadline() {
        let fixture = fixture();
        let host_cancel = Arc::new(AtomicBool::new(false));
        let deadline = Deadline::run_bounded(&Deadline::new(10_000), host_cancel.clone(), || {
            Ok(Deadline::new(10_000))
        })
        .unwrap();
        let (mut session, queue) = session(&fixture, deadline, Some(Ok(())));
        let original = session.state().clone();
        session.before_response_check = Some(Box::new(move |_| {
            host_cancel.store(true, Ordering::SeqCst);
        }));
        let result = session.request(Request::Close);
        let active = session.active;
        let state = session.state().clone();
        drop(session);
        drop(queue);
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("cancelled by host")
        );
        assert!(!active);
        assert_eq!(state, original);
    }

    #[test]
    fn timely_confirmed_cleanup_publishes_the_bound_terminal_response() {
        let fixture = fixture();
        let (mut session, queue) = session(&fixture, Deadline::new(10_000), Some(Ok(())));
        let checked = Arc::new(AtomicBool::new(false));
        let observed = checked.clone();
        session.before_response_check = Some(Box::new(move |deadline| {
            observed.store(deadline.check().is_ok(), Ordering::SeqCst);
        }));
        let result = session.request(Request::Close);
        let active = session.active;
        let state = session.state().clone();
        drop(session);
        drop(queue);
        let value = result.unwrap();
        assert!(checked.load(Ordering::SeqCst));
        assert!(!active);
        assert_eq!(value["status"], "closed");
        assert_eq!(value["snapshotId"], fixture.snapshot.id);
        assert_eq!(value["workspaceId"], fixture.snapshot.workspace_id);
        assert_eq!(value["requestId"], 1);
        assert_eq!(state, value);
    }

    #[test]
    fn missing_and_refused_cleanup_receipts_keep_typed_error_precedence() {
        for (missing, expired) in [(true, false), (false, false), (true, true), (false, true)] {
            let fixture = fixture();
            let receipt = if missing {
                None
            } else {
                Some(Err(crate::process::CleanupUnproven::new(
                    "owned receipt refusal",
                )
                .into()))
            };
            let deadline = Deadline::new(if expired { 0 } else { 10_000 });
            let (mut session, queue) = session(&fixture, deadline, receipt);
            let original = session.state().clone();
            let checked = Arc::new(AtomicBool::new(false));
            let observed = checked.clone();
            session.before_response_check = Some(Box::new(move |deadline| {
                observed.store(true, Ordering::SeqCst);
                *deadline = Deadline::new(0);
            }));
            let result = session.request(Request::Close);
            let active = session.active;
            let cancelled = session.cancel.load(Ordering::SeqCst);
            let state = session.state().clone();
            drop(session);
            drop(queue);
            let error = result.unwrap_err();
            assert!(crate::process::cleanup_unproven(&error));
            assert!(!checked.load(Ordering::SeqCst));
            assert!(!active);
            assert!(cancelled);
            assert_eq!(state, original);
        }
    }
}
