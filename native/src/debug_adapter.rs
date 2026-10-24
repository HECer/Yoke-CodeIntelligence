//! Host-authorized independent Debug Adapter Protocol client.
use crate::{
    capture::Deadline,
    model::{Snapshot, identity},
    process::ManagedChild,
    store::Store,
};
use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::{BufReader, Read, Write},
    path::PathBuf,
    process::{Command, Stdio},
    sync::mpsc::{self, Receiver, Sender, SyncSender},
    thread,
    time::{Duration, Instant},
};
const MAX_FRAME: usize = 4_000_000;
const MAX_LIFETIME: usize = 16_000_000;
type Input = SyncSender<(Vec<u8>, Sender<Result<()>>)>;

fn receive_cooperatively<T>(receiver: &Receiver<T>, deadline: &Deadline) -> Result<T> {
    loop {
        deadline.check()?;
        match receiver.recv_timeout(deadline.remaining().min(Duration::from_millis(25))) {
            Ok(value) => {
                deadline.check()?;
                return Ok(value);
            }
            Err(mpsc::RecvTimeoutError::Timeout) => continue,
            Err(mpsc::RecvTimeoutError::Disconnected) => bail!("Debugger channel closed"),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct HostDebugProfile {
    pub key: String,
    pub adapter_id: String,
    pub command: PathBuf,
    #[serde(default)]
    pub arguments: Vec<String>,
    pub trust_adapter: bool,
    pub allow_target_execution: bool,
    pub allow_evaluation: bool,
    pub request: String,
    pub configuration: Value,
    pub entry: Option<String>,
    #[serde(default)]
    pub breakpoints: Vec<DebugBreakpoint>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DebugBreakpoint {
    pub path: String,
    pub line: usize,
}
impl HostDebugProfile {
    pub fn validate(&self) -> Result<()> {
        for label in [&self.key, &self.adapter_id] {
            ensure!(
                !label.is_empty()
                    && label.len() <= 100
                    && label
                        .chars()
                        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.')),
                "Invalid debugger profile label"
            );
        }
        ensure!(
            self.command.is_absolute()
                && self.command.is_file()
                && !crate::paths::is_link(&fs::symlink_metadata(&self.command)?),
            "Debugger adapter requires an existing absolute regular host executable"
        );
        ensure!(
            self.arguments.len() <= 100
                && self.arguments.iter().map(String::len).sum::<usize>() <= 128000,
            "Debugger argument capacity exceeded"
        );
        ensure!(
            matches!(self.request.as_str(), "launch" | "attach")
                && self.configuration.is_object()
                && serde_json::to_vec(&self.configuration)?.len() <= 64000,
            "Invalid host debugger target configuration"
        );
        if let Some(entry) = &self.entry {
            ensure!(
                crate::paths::normalized(entry)? == *entry,
                "Invalid captured debugger entry"
            );
        }
        ensure!(
            self.breakpoints.len() <= 100,
            "Debugger breakpoint capacity exceeded"
        );
        for breakpoint in &self.breakpoints {
            ensure!(
                crate::paths::normalized(&breakpoint.path)? == breakpoint.path
                    && breakpoint.line > 0
                    && breakpoint.line <= i32::MAX as usize,
                "Invalid source debugger breakpoint"
            );
        }
        Ok(())
    }
}
pub struct DebugAdapter {
    child: Option<ManagedChild>,
    input: Option<Input>,
    messages: Option<Receiver<Result<Value>>>,
    workers: Vec<thread::JoinHandle<()>>,
    cleanup_unproven: bool,
    directory: tempfile::TempDir,
    snapshot: Snapshot,
    profile_hash: String,
    sequence: u64,
    last_adapter_sequence: u64,
    pending: BTreeMap<u64, String>,
    replies: BTreeMap<u64, Value>,
    initialized: bool,
    stopped: Option<u64>,
    frame: Value,
    frame_source: Option<PathBuf>,
    alive: bool,
    output_bytes: usize,
}
impl DebugAdapter {
    pub fn start(
        store: &Store,
        snapshot: &Snapshot,
        profile: &HostDebugProfile,
        deadline: &Deadline,
    ) -> Result<Self> {
        deadline.check()?;
        profile.validate()?;
        ensure!(
            profile.trust_adapter && profile.allow_target_execution && profile.allow_evaluation,
            "Debugger startup requires separate explicit adapter, target execution and evaluation host authority"
        );
        store.set_deadline(deadline)?;
        ensure!(
            snapshot.workspace_id == store.workspace_id
                && snapshot.kind != "federated"
                && identity(&store.snapshot(&snapshot.id)?) == identity(snapshot),
            "Debugger source manifest mismatch"
        );
        let scope = crate::capture::source_scope_policy(store, snapshot)?;
        if let Some(entry) = &profile.entry {
            scope.file(entry)?;
        }
        let directory = tempfile::Builder::new().prefix("yoke-dap-").tempdir()?;
        crate::changes::materialize(store, snapshot, directory.path(), deadline)?;
        let configuration = expand_configuration(
            &profile.configuration,
            directory.path(),
            profile.entry.as_deref(),
            deadline,
        )?;
        // Selected executable bytes and declared host context are bound; this is
        // not transitive plugin or runtime-loaded-code attestation.
        let mut executable = fs::File::open(&profile.command)?;
        ensure!(
            executable.metadata()?.len() <= 256_000_000,
            "Debugger executable capacity exceeded"
        );
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        let mut chunk = [0u8; 65536];
        let mut total = 0usize;
        loop {
            deadline.check()?;
            let read = executable.read(&mut chunk)?;
            if read == 0 {
                break;
            }
            total = total
                .checked_add(read)
                .context("Debugger executable size overflow")?;
            ensure!(
                total <= 256_000_000,
                "Debugger executable grew beyond capacity"
            );
            hasher.update(&chunk[..read]);
        }
        let profile_hash = identity(&(profile, hex::encode(hasher.finalize())));
        let mut command = Command::new(&profile.command);
        command
            .args(&profile.arguments)
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
        let mut child = ManagedChild::spawn(command)?;
        let mut stdin = child.stdin().take().context("Debugger stdin missing")?;
        let stdout = child.stdout().take().context("Debugger stdout missing")?;
        let mut stderr = child.stderr().take().context("Debugger stderr missing")?;
        let (input, queue) = mpsc::sync_channel::<(Vec<u8>, Sender<Result<()>>)>(1);
        let mut workers = Vec::new();
        workers.push(thread::spawn(move || {
            while let Ok((bytes, done)) = queue.recv() {
                let result = stdin
                    .write_all(&bytes)
                    .and_then(|()| stdin.flush())
                    .map_err(Into::into);
                let failed = result.is_err();
                let _ = done.send(result);
                if failed {
                    break;
                }
            }
        }));
        let (frames, messages) = mpsc::sync_channel(8);
        let raw_errors = frames.clone();
        workers.push(thread::spawn(move || {
            let mut stdout = BufReader::new(stdout);
            let mut bytes = 0usize;
            loop {
                let value = crate::lsp::read_content_frame(&mut stdout).and_then(|value| {
                    bytes = bytes.saturating_add(serde_json::to_vec(&value)?.len());
                    ensure!(
                        bytes <= MAX_LIFETIME,
                        "DAP lifetime protocol budget exceeded"
                    );
                    Ok(value)
                });
                let failed = value.is_err();
                if frames.send(value).is_err() || failed {
                    break;
                }
            }
        }));
        workers.push(thread::spawn(move || {
            let mut discarded = [0u8; 4096];
            let mut total = 0usize;
            loop {
                match stderr.read(&mut discarded) {
                    Ok(0) => break,
                    Ok(n) => {
                        total += n;
                        if total > 32000 {
                            let _ = raw_errors
                                .send(Err(anyhow::anyhow!("DAP stderr capacity exceeded")));
                            break;
                        }
                    }
                    Err(_) => {
                        let _ =
                            raw_errors.send(Err(anyhow::anyhow!("DAP stderr transport failed")));
                        break;
                    }
                }
            }
        }));
        let mut adapter = Self {
            child: Some(child),
            input: Some(input),
            messages: Some(messages),
            workers,
            cleanup_unproven: false,
            directory,
            snapshot: snapshot.clone(),
            profile_hash,
            sequence: 0,
            last_adapter_sequence: 0,
            pending: BTreeMap::new(),
            replies: BTreeMap::new(),
            initialized: false,
            stopped: None,
            frame: Value::Null,
            frame_source: None,
            alive: true,
            output_bytes: 0,
        };
        let setup = (|| -> Result<()> {
            let initialize=adapter.request("initialize",json!({"adapterID":profile.adapter_id,"clientID":"yoke","clientName":"Yoke independent DAP client","linesStartAt1":true,"columnsStartAt1":true,"pathFormat":"path","supportsVariableType":true,"supportsRunInTerminalRequest":false,"supportsStartDebuggingRequest":false}),deadline)?;
            let launch = adapter.send_request(&profile.request, configuration, deadline)?;
            if initialize["supportsConfigurationDoneRequest"] == true
                || !profile.breakpoints.is_empty()
            {
                while !adapter.initialized {
                    adapter.receive(deadline)?;
                }
            }
            let mut paths = BTreeSet::new();
            for breakpoint in &profile.breakpoints {
                paths.insert(&breakpoint.path);
            }
            for path in paths {
                let file = scope.file(path)?;
                let bytes = scope.read_source(path)?;
                let text = std::str::from_utf8(&bytes)?;
                let lines = crate::paths::line_starts(text).len();
                let mut selected = Vec::new();
                for breakpoint in &profile.breakpoints {
                    if breakpoint.path == *path {
                        ensure!(
                            breakpoint.line <= lines,
                            "Debugger breakpoint outside captured source"
                        );
                        selected.push(json!({"line":breakpoint.line}));
                    }
                }
                let reply=adapter.request("setBreakpoints",json!({"source":{"path":adapter.directory.path().join(path).display().to_string()},"breakpoints":selected,"sourceModified":false}),deadline)?;
                let received = reply["breakpoints"]
                    .as_array()
                    .context("Debugger breakpoint response missing")?;
                ensure!(
                    received.len() == selected.len()
                        && received.iter().all(|item| item["verified"] == true),
                    "Debugger did not verify selected source breakpoints"
                );
                ensure!(
                    file.hash == crate::model::hash(bytes),
                    "Debugger breakpoint source mismatch"
                );
            }
            if initialize["supportsConfigurationDoneRequest"] == true {
                adapter.request("configurationDone", json!({}), deadline)?;
            }
            adapter.await_response(launch, deadline)?;
            while adapter.stopped.is_none() {
                adapter.receive(deadline)?;
            }
            adapter.select_frame(deadline)?;
            Ok(())
        })();
        if let Err(error) = setup {
            adapter.kill()?;
            return Err(error);
        }
        Ok(adapter)
    }
    pub fn info(&self) -> Value {
        json!({"schemaVersion":1,"snapshotId":self.snapshot.id,"workspaceId":self.snapshot.workspace_id,"profileHash":self.profile_hash,"alive":self.alive,"stopped":self.stopped.is_some(),"frame":self.frame,"provider":"independent_dap","runtimeLoadedCodeVerified":false,"execution":"explicit_host_authorized_runtime","osSandbox":false,"ownedProcessCleanupProven":self.child.is_none() && self.workers.is_empty() && !self.cleanup_unproven,"ownedIoWorkers":self.workers.len()})
    }
    pub fn snapshot_id(&self) -> &str {
        &self.snapshot.id
    }
    pub fn evaluate(&mut self, expression: &str, deadline: &Deadline) -> Result<Value> {
        ensure!(
            self.alive && self.stopped.is_some(),
            "Debugger evaluation requires a live stopped target"
        );
        ensure!(
            !expression.is_empty() && expression.len() <= 64000 && !expression.contains('\0'),
            "Invalid debugger expression"
        );
        self.check_frame(deadline)?;
        let frame_id = self.frame["id"]
            .as_u64()
            .context("Stopped frame ID missing")?;
        let body = self.request(
            "evaluate",
            json!({"expression":expression,"frameId":frame_id,"context":"repl"}),
            deadline,
        )?;
        ensure!(
            self.stopped.is_some(),
            "Target resumed during debugger evaluation"
        );
        self.check_frame(deadline)?;
        let result = body["result"]
            .as_str()
            .context("DAP evaluation result must be text")?;
        ensure!(
            result.len() <= 1_000_000
                && body["variablesReference"]
                    .as_u64()
                    .is_some_and(|n| n <= i32::MAX as u64),
            "Invalid or excessive DAP evaluation result"
        );
        ensure!(
            body.get("type")
                .is_none_or(|value| value.as_str().is_some_and(|text| text.len() <= 1000)),
            "Invalid DAP result type"
        );
        Ok(
            json!({"result":result,"type":body["type"],"variablesReference":body["variablesReference"],"snapshotId":self.snapshot.id,"frame":self.frame,"profileHash":self.profile_hash,"provider":"independent_dap","runtimeEffects":"authorized_unmonitored_host_effects","runtimeLoadedCodeVerified":false,"osSandbox":false}),
        )
    }
    pub fn close(&mut self, deadline: &Deadline) -> Result<()> {
        if !self.alive {
            return self.kill();
        }
        let result = self
            .request(
                "disconnect",
                json!({"restart":false,"terminateDebuggee":true}),
                deadline,
            )
            .map(|_| ());
        self.kill()?;
        result
    }
    pub(crate) fn terminate(&mut self) -> Result<()> {
        self.kill()
    }
    pub(crate) fn cleanup_unproven(&self) -> bool {
        self.cleanup_unproven
    }
    fn kill(&mut self) -> Result<()> {
        let end = Instant::now() + Duration::from_secs(2);
        self.alive = false;
        self.stopped = None;
        self.input.take();
        self.messages.take();
        if let Some(child) = self.child.take()
            && child.terminate_checked(Duration::from_secs(1)).is_err()
        {
            self.cleanup_unproven = true;
        }
        while !self.workers.is_empty() && Instant::now() < end {
            for index in (0..self.workers.len()).rev() {
                if self.workers[index].is_finished()
                    && self.workers.swap_remove(index).join().is_err()
                {
                    self.cleanup_unproven = true;
                }
            }
            if !self.workers.is_empty() {
                thread::sleep(Duration::from_millis(10));
            }
        }
        self.cleanup_unproven |= !self.workers.is_empty();
        if self.cleanup_unproven {
            return Err(crate::process::CleanupUnproven::new(
                "Debugger owned process or I/O worker cleanup could not be proven",
            )
            .into());
        }
        Ok(())
    }
    fn write(&mut self, message: Value, deadline: &Deadline) -> Result<()> {
        let result = self.write_inner(message, deadline);
        if result.is_err() {
            self.kill()?;
        }
        result
    }
    fn write_inner(&mut self, message: Value, deadline: &Deadline) -> Result<()> {
        deadline.check()?;
        let bytes = serde_json::to_vec(&message)?;
        ensure!(bytes.len() <= MAX_FRAME, "DAP request capacity exceeded");
        let mut framed = format!("Content-Length: {}\r\n\r\n", bytes.len()).into_bytes();
        framed.extend(bytes);
        let (done, ack) = mpsc::channel();
        self.input
            .as_ref()
            .context("Debugger input closed")?
            .try_send((framed, done))
            .map_err(|_| anyhow::anyhow!("Debugger input queue unavailable"))?;
        receive_cooperatively(&ack, deadline).context("Debugger write deadline exceeded")??;
        deadline.check()
    }
    fn next_sequence(&mut self) -> Result<u64> {
        self.sequence = self
            .sequence
            .checked_add(1)
            .context("DAP sequence overflow")?;
        ensure!(
            self.sequence <= i32::MAX as u64,
            "DAP sequence capacity exceeded"
        );
        Ok(self.sequence)
    }
    fn send_request(
        &mut self,
        command: &str,
        arguments: Value,
        deadline: &Deadline,
    ) -> Result<u64> {
        ensure!(
            self.pending.len() < 32,
            "DAP pending request capacity exceeded"
        );
        let seq = self.next_sequence()?;
        self.write(
            json!({"seq":seq,"type":"request","command":command,"arguments":arguments}),
            deadline,
        )?;
        self.pending.insert(seq, command.into());
        Ok(seq)
    }
    fn request(&mut self, command: &str, arguments: Value, deadline: &Deadline) -> Result<Value> {
        let seq = self.send_request(command, arguments, deadline)?;
        self.await_response(seq, deadline)
    }
    fn await_response(&mut self, seq: u64, deadline: &Deadline) -> Result<Value> {
        while !self.replies.contains_key(&seq) {
            self.receive(deadline)?;
        }
        let reply = self.replies.remove(&seq).context("DAP response missing")?;
        self.pending.remove(&seq);
        ensure!(
            reply["success"] == true,
            "Debugger request failed: {}",
            reply["command"].as_str().unwrap_or("unknown")
        );
        Ok(reply.get("body").cloned().unwrap_or(json!({})))
    }
    fn receive(&mut self, deadline: &Deadline) -> Result<()> {
        let result = self.receive_inner(deadline);
        if result.is_err() {
            self.kill()?;
        }
        result
    }
    fn receive_inner(&mut self, deadline: &Deadline) -> Result<()> {
        deadline.check()?;
        let message = receive_cooperatively(
            self.messages.as_ref().context("Debugger channel closed")?,
            deadline,
        )
        .context("DAP response deadline exceeded")??;
        let seq = message["seq"]
            .as_u64()
            .filter(|seq| *seq > self.last_adapter_sequence && *seq <= i32::MAX as u64)
            .context("DAP adapter sequence is invalid or repeated")?;
        self.last_adapter_sequence = seq;
        match message["type"].as_str() {
            Some("response") => {
                let request = message["request_seq"]
                    .as_u64()
                    .context("DAP response request ID missing")?;
                let command = self
                    .pending
                    .get(&request)
                    .context("DAP response has unknown request ID")?;
                ensure!(
                    message["command"] == *command
                        && message["success"].is_boolean()
                        && !self.replies.contains_key(&request),
                    "Invalid or repeated DAP response"
                );
                ensure!(
                    self.replies.len() < 32,
                    "DAP retained response capacity exceeded"
                );
                self.replies.insert(request, message);
            }
            Some("event") => match message["event"].as_str() {
                Some("initialized") => {
                    ensure!(!self.initialized, "Repeated DAP initialized event");
                    self.initialized = true;
                }
                Some("stopped") => {
                    ensure!(
                        message["body"]["reason"]
                            .as_str()
                            .is_some_and(|reason| !reason.is_empty() && reason.len() <= 200),
                        "Stopped DAP reason missing"
                    );
                    self.stopped = Some(if message["body"].get("threadId").is_some() {
                        message["body"]["threadId"]
                            .as_u64()
                            .filter(|n| *n > 0 && *n <= i32::MAX as u64)
                            .context("Invalid stopped DAP thread ID")?
                    } else {
                        ensure!(
                            message["body"]["allThreadsStopped"] == true,
                            "No stopped DAP thread is identified"
                        );
                        0
                    });
                }
                Some("continued") => {
                    self.stopped = None;
                    self.frame = Value::Null;
                    self.frame_source = None;
                }
                Some("terminated" | "exited") => {
                    ensure!(
                        self.pending.values().any(|command| command == "disconnect"),
                        "Debugger target terminated"
                    );
                    self.stopped = None;
                }
                Some("output") => {
                    let text = message["body"]["output"]
                        .as_str()
                        .context("Invalid DAP output event")?;
                    self.output_bytes = self
                        .output_bytes
                        .checked_add(text.len())
                        .context("Debugger output overflow")?;
                    ensure!(
                        self.output_bytes <= 1_000_000,
                        "Debugger target output capacity exceeded"
                    );
                }
                Some(event) => ensure!(event.len() <= 200, "Invalid DAP event name"),
                None => bail!("DAP event name missing"),
            },
            Some("request") => {
                let command = message["command"]
                    .as_str()
                    .filter(|s| s.len() <= 200)
                    .context("DAP reverse request command missing")?
                    .to_owned();
                let sequence = self.next_sequence()?;
                self.write(json!({"seq":sequence,"type":"response","request_seq":seq,"success":false,"command":command,"message":"Reverse adapter execution requests are not authorized"}),deadline)?;
                bail!(
                    "DAP reverse request is unavailable without independent host callback authority"
                );
            }
            _ => bail!("Unsupported DAP message type"),
        }
        deadline.check()
    }
    fn select_frame(&mut self, deadline: &Deadline) -> Result<()> {
        let mut thread = self.stopped.context("Stopped target required")?;
        if thread == 0 {
            let body = self.request("threads", json!({}), deadline)?;
            let threads = body["threads"]
                .as_array()
                .filter(|threads| !threads.is_empty() && threads.len() <= 100)
                .context("Invalid DAP stopped thread inventory")?;
            thread = threads[0]["id"]
                .as_u64()
                .filter(|n| *n > 0 && *n <= i32::MAX as u64)
                .context("Invalid selected stopped thread ID")?;
            self.stopped = Some(thread);
        }
        let body = self.request(
            "stackTrace",
            json!({"threadId":thread,"startFrame":0,"levels":1}),
            deadline,
        )?;
        let frames = body["stackFrames"]
            .as_array()
            .filter(|frames| frames.len() == 1)
            .context("Debugger did not return one stopped frame")?;
        let frame = &frames[0];
        ensure!(
            frame["id"].as_u64().is_some_and(|n| n <= i32::MAX as u64),
            "Invalid DAP frame ID"
        );
        self.frame_source = Some(PathBuf::from(
            frame["source"]["path"]
                .as_str()
                .context("Debugger frame source absent")?,
        ));
        self.frame = json!({"id":frame["id"],"line":frame["line"],"column":frame["column"]});
        self.check_frame(deadline)
    }
    fn check_frame(&mut self, deadline: &Deadline) -> Result<()> {
        deadline.check()?;
        let path = self
            .frame_source
            .as_ref()
            .context("Debugger source path absent")?;
        ensure!(path.is_absolute(), "Debugger frame source must be absolute");
        let relative = path
            .strip_prefix(self.directory.path())
            .context("Debugger frame outside captured source")?
            .to_str()
            .context("Debugger path is not UTF-8")?
            .replace('\\', "/");
        ensure!(
            crate::paths::normalized(&relative)? == relative,
            "Debugger frame has unsafe source path"
        );
        let source = self
            .snapshot
            .files
            .iter()
            .find(|file| file.path == relative)
            .context("Debugger frame source is not captured")?;
        let mut component = self.directory.path().to_path_buf();
        for part in relative.split('/') {
            component.push(part);
            ensure!(
                !crate::paths::is_link(&fs::symlink_metadata(&component)?),
                "Debugger source link refused"
            );
        }
        let metadata = fs::symlink_metadata(path)?;
        ensure!(
            metadata.is_file() && metadata.len() == source.size as u64,
            "Debugger source size changed"
        );
        let mut input = fs::File::open(path)?;
        ensure!(
            input.metadata()?.len() == source.size as u64,
            "Opened debugger source size changed"
        );
        let mut bytes = Vec::with_capacity(source.size);
        Read::by_ref(&mut input)
            .take(source.size as u64 + 1)
            .read_to_end(&mut bytes)?;
        ensure!(
            bytes.len() == source.size && crate::model::hash(&bytes) == source.hash,
            "Debugger source bytes changed"
        );
        let text = std::str::from_utf8(&bytes)?;
        let starts = crate::paths::line_starts(text);
        let line = self.frame["line"]
            .as_u64()
            .filter(|line| *line > 0 && *line <= starts.len() as u64)
            .context("Debugger frame line outside captured source")? as usize;
        let column = self.frame["column"].as_u64().unwrap_or(1);
        ensure!(
            column > 0 && column <= i32::MAX as u64,
            "Invalid debugger frame column"
        );
        let at = crate::lsp::position_to_byte(text, &json!({"line":line-1,"character":column-1}))?;
        self.frame["path"] = json!(relative);
        self.frame["contentHash"] = json!(source.hash);
        self.frame["startByte"] = json!(at);
        deadline.check()
    }
}
impl Drop for DebugAdapter {
    fn drop(&mut self) {
        if self.kill().is_err() {
            eprintln!("Debugger cleanup could not be proven; host recovery required");
        }
    }
}

fn expand_configuration(
    value: &Value,
    root: &std::path::Path,
    entry: Option<&str>,
    deadline: &Deadline,
) -> Result<Value> {
    fn expand(
        value: &Value,
        root: &str,
        entry: Option<&str>,
        depth: usize,
        nodes: &mut usize,
        deadline: &Deadline,
    ) -> Result<Value> {
        deadline.check()?;
        *nodes += 1;
        ensure!(
            depth <= 16 && *nodes <= 2000,
            "Debugger target configuration nesting exceeded"
        );
        Ok(match value {
            Value::String(text) => {
                let text = text.replace("${capturedRoot}", root);
                let text = if text.contains("${capturedEntry}") {
                    text.replace(
                        "${capturedEntry}",
                        entry.context("Captured debugger entry required")?,
                    )
                } else {
                    text
                };
                ensure!(
                    text.len() <= 64000,
                    "Expanded debugger setting exceeded capacity"
                );
                json!(text)
            }
            Value::Array(values) => Value::Array(
                values
                    .iter()
                    .map(|value| expand(value, root, entry, depth + 1, nodes, deadline))
                    .collect::<Result<_>>()?,
            ),
            Value::Object(values) => Value::Object(
                values
                    .iter()
                    .map(|(key, value)| {
                        Ok((
                            key.clone(),
                            expand(value, root, entry, depth + 1, nodes, deadline)?,
                        ))
                    })
                    .collect::<Result<_>>()?,
            ),
            _ => value.clone(),
        })
    }
    let root = root
        .to_str()
        .context("Captured debugger root is not UTF-8")?;
    let entry = entry.map(|entry| PathBuf::from(root).join(entry).display().to_string());
    let expanded = expand(value, root, entry.as_deref(), 0, &mut 0, deadline)?;
    ensure!(
        serde_json::to_vec(&expanded)?.len() <= 128000,
        "Expanded debugger configuration capacity exceeded"
    );
    Ok(expanded)
}

#[cfg(test)]
mod cleanup_tests {
    use super::*;
    #[test]
    fn unjoined_worker_cleanup_failure_remains_sticky_after_worker_eventually_exits() {
        let (release, wait) = mpsc::channel::<()>();
        let worker = thread::spawn(move || {
            let _ = wait.recv();
        });
        let mut adapter = DebugAdapter {
            child: None,
            input: None,
            messages: None,
            workers: vec![worker],
            cleanup_unproven: false,
            directory: tempfile::tempdir().unwrap(),
            snapshot: Snapshot {
                id: "a".repeat(64),
                workspace_id: "b".repeat(64),
                kind: "disk".into(),
                files: vec![],
                omissions: vec![],
                capture_consistency: "test".into(),
            },
            profile_hash: "c".repeat(64),
            sequence: 0,
            last_adapter_sequence: 0,
            pending: BTreeMap::new(),
            replies: BTreeMap::new(),
            initialized: false,
            stopped: None,
            frame: Value::Null,
            frame_source: None,
            alive: false,
            output_bytes: 0,
        };
        let error = adapter.terminate().unwrap_err();
        let sticky = adapter.cleanup_unproven();
        release.send(()).unwrap();
        for worker in adapter.workers.drain(..) {
            worker.join().unwrap();
        }
        assert!(crate::process::cleanup_unproven(&error));
        assert!(
            sticky,
            "A timed-out I/O worker cleanup disappeared from retained session state"
        );
        assert!(
            adapter.terminate().is_err(),
            "A later worker exit must not silently erase a prior unproven cleanup"
        );
    }
}
