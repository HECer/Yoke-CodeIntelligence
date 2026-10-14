use crate::{
    Engine,
    capture::Deadline,
    changes,
    model::{Graph, Snapshot},
    paths::{scoped, utf8_range},
    query,
};
use anyhow::{Context, Result, bail, ensure};
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    fs,
    io::{BufRead, BufReader, Read, Write},
    path::Path,
    process::{Command, Stdio},
    sync::mpsc::{self, Receiver, Sender, SyncSender},
    thread,
};
use url::Url;
#[derive(Clone)]
pub struct Server {
    pub command: String,
    pub args: Vec<String>,
    pub language: String,
    pub trust: bool,
}
fn document_language(language: &str) -> &str {
    match language {
        "tsx" => "typescriptreact",
        "jsx" => "javascriptreact",
        _ => language,
    }
}
fn provider_language(language: &str) -> &str {
    match language {
        "tsx" => "typescript",
        "jsx" => "javascript",
        _ => language,
    }
}

pub fn byte_to_position(text: &str, byte: usize) -> Result<Value> {
    utf8_range(text, 0, byte)?;
    let starts = crate::paths::line_starts(text);
    let line = starts.partition_point(|start| *start <= byte) - 1;
    let start = starts[line];
    ensure!(
        !text[start..byte].contains('\r'),
        "Position inside line terminator"
    );
    Ok(json!({"line":line,"character":text[start..byte].encode_utf16().count()}))
}
pub fn position_to_byte(text: &str, position: &Value) -> Result<usize> {
    let line = position["line"]
        .as_u64()
        .ok_or_else(|| anyhow::anyhow!("Invalid LSP line"))? as usize;
    let character = position["character"]
        .as_u64()
        .ok_or_else(|| anyhow::anyhow!("Invalid LSP character"))? as usize;
    let starts = crate::paths::line_starts(text);
    let start = *starts
        .get(line)
        .ok_or_else(|| anyhow::anyhow!("LSP line outside source"))?;
    let mut units = 0;
    for (relative, c) in text[start..].char_indices() {
        if units == character {
            return Ok(start + relative);
        }
        ensure!(c != '\n' && c != '\r', "LSP character outside line");
        units += c.len_utf16();
        ensure!(units <= character, "LSP position splits a surrogate pair");
    }
    ensure!(units == character, "LSP character outside source");
    Ok(text.len())
}
pub fn read_frame(input: &mut impl BufRead) -> Result<Value> {
    let mut length = None;
    let mut total = 0;
    loop {
        let mut header = Vec::new();
        let bytes = input.take(8193).read_until(b'\n', &mut header)?;
        ensure!(
            bytes > 0 && bytes <= 8192,
            "LSP header missing or too large"
        );
        total += bytes;
        ensure!(total <= 16384, "LSP header budget exceeded");
        ensure!(header.ends_with(b"\r\n"), "LSP header must use CRLF");
        if header == b"\r\n" {
            break;
        }
        let text = std::str::from_utf8(&header)?;
        let (name, value) = text
            .trim()
            .split_once(':')
            .ok_or_else(|| anyhow::anyhow!("Malformed LSP header"))?;
        if name.eq_ignore_ascii_case("Content-Length") {
            ensure!(length.is_none(), "Duplicate Content-Length");
            length = Some(value.trim().parse::<usize>()?);
        }
    }
    let length = length.ok_or_else(|| anyhow::anyhow!("Missing Content-Length"))?;
    ensure!(
        length > 0 && length <= 4_000_000,
        "LSP message budget exceeded"
    );
    let mut bytes = vec![0; length];
    input.read_exact(&mut bytes)?;
    let value: Value = serde_json::from_slice(&bytes)?;
    ensure!(
        value.is_object() && value["jsonrpc"] == "2.0",
        "Invalid LSP JSON-RPC message"
    );
    Ok(value)
}
struct Session {
    child: crate::process::ManagedChild,
    input: SyncSender<(Vec<u8>, Sender<Result<()>>)>,
    deadline: Deadline,
    messages: Receiver<Result<Value>>,
    next: u64,
    diagnostics: Vec<Value>,
    quiescent: bool,
}
impl Session {
    fn spawn(server: &Server, root: &Path, deadline: &Deadline) -> Result<Self> {
        ensure!(
            server.trust,
            "Starting a language server requires explicit host --trust-server permission"
        );
        ensure!(
            !server.command.is_empty() && server.args.len() <= 100,
            "Invalid host server command"
        );
        let mut command = Command::new(&server.command);
        command
            .args(&server.args)
            .current_dir(root)
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
            "RUSTUP_HOME",
            "CARGO_HOME",
        ] {
            if let Some(value) = std::env::var_os(name) {
                command.env(name, value);
            }
        }
        command.env("DO_NOT_TRACK", "1").env("NO_COLOR", "1");
        let mut child = crate::process::ManagedChild::spawn(command)
            .context("Unable to start host-selected language server")?;
        let mut stdin = child.stdin().take().unwrap();
        let stdout = child.stdout().take().unwrap();
        let stderr = child.stderr().take().unwrap();
        let (input, writes) = mpsc::sync_channel::<(Vec<u8>, Sender<Result<()>>)>(8);
        thread::spawn(move || {
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
        });
        let (sender, messages) = mpsc::sync_channel(8);
        thread::spawn(move || {
            let mut input = BufReader::new(stdout);
            loop {
                let result = read_frame(&mut input);
                let done = result.is_err();
                if sender.send(result).is_err() || done {
                    break;
                }
            }
        });
        thread::spawn(move || {
            let mut stderr = stderr;
            let mut buffer = [0; 4096];
            let mut total = 0usize;
            while let Ok(count) = stderr.read(&mut buffer) {
                if count == 0 {
                    break;
                }
                total = total.saturating_add(count);
                if total > 8_000_000 {
                    break;
                }
            }
        });
        Ok(Self {
            child,
            input,
            deadline: deadline.clone(),
            messages,
            next: 0,
            diagnostics: Vec::new(),
            quiescent: false,
        })
    }
    fn send(&mut self, value: Value) -> Result<()> {
        self.deadline.check()?;
        let bytes = serde_json::to_vec(&value)?;
        ensure!(bytes.len() <= 4_000_000, "Outgoing LSP budget exceeded");
        let mut framed = format!("Content-Length: {}\r\n\r\n", bytes.len()).into_bytes();
        framed.extend_from_slice(&bytes);
        let (done, completed) = mpsc::channel();
        self.input
            .try_send((framed, done))
            .map_err(|_| anyhow::anyhow!("Language server input queue blocked or closed"))?;
        completed
            .recv_timeout(self.deadline.remaining())
            .context("Language server input write exceeded deadline")??;
        Ok(())
    }
    fn notify(&mut self, method: &str, params: Value) -> Result<()> {
        self.send(json!({"jsonrpc":"2.0","method":method,"params":params}))
    }
    fn callback(&mut self, message: &Value) -> Result<()> {
        if message["method"] == "experimental/serverStatus" {
            self.quiescent = message["params"]["quiescent"].as_bool().unwrap_or(false);
            ensure!(
                message["params"]["health"] != "error",
                "Rust analyzer workspace loading failed: {}",
                message["params"]["message"]
            );
        }
        if message.get("method").is_some() && message.get("id").is_some() {
            let result = match message["method"].as_str().unwrap_or("") {
                "window/workDoneProgress/create" => Some(Value::Null),
                "workspace/configuration" => Some(Value::Array(
                    message["params"]["items"]
                        .as_array()
                        .map(|items| items.iter().map(|_| Value::Null).collect())
                        .unwrap_or_default(),
                )),
                "workspace/applyEdit" => Some(
                    json!({"applied":false,"failureReason":"Client prepares immutable plans; server writes are refused"}),
                ),
                _ => None,
            };
            if let Some(result) = result {
                self.send(json!({"jsonrpc":"2.0","id":message["id"],"result":result}))?;
            } else {
                self.send(json!({"jsonrpc":"2.0","id":message["id"],"error":{"code":-32601,"message":"Unsupported server callback"}}))?;
            }
        }
        Ok(())
    }
    fn await_ready(&mut self, deadline: &Deadline) -> Result<()> {
        for _ in 0..4096 {
            if self.quiescent {
                return Ok(());
            }
            deadline.check()?;
            let message = self
                .messages
                .recv_timeout(deadline.remaining())
                .context("Language server did not finish loading the snapshot")??;
            ensure!(
                message.get("method").is_some(),
                "Unexpected response during workspace loading"
            );
            self.callback(&message)?;
        }
        bail!("Workspace loading notification budget exceeded")
    }
    fn request(&mut self, method: &str, params: Value, deadline: &Deadline) -> Result<Value> {
        self.next += 1;
        let id = self.next;
        self.send(json!({"jsonrpc":"2.0","id":id,"method":method,"params":params}))?;
        for _ in 0..4096 {
            deadline.check()?;
            let message = self
                .messages
                .recv_timeout(deadline.remaining())
                .context("LSP response timeout or disconnected process")??;
            self.callback(&message)?;
            if message["id"] == id && message.get("method").is_none() {
                ensure!(
                    message.get("result").is_some() != message.get("error").is_some(),
                    "LSP response must contain exactly one result or error"
                );
                if message.get("error").is_some() {
                    bail!(
                        "Language server returned an error: {}",
                        message["error"]
                            .to_string()
                            .chars()
                            .take(500)
                            .collect::<String>()
                    );
                }
                return Ok(message["result"].clone());
            }
            if message["method"] == "textDocument/publishDiagnostics"
                && self.diagnostics.len() < 200
            {
                self.diagnostics.push(message["params"].clone());
            }
            if message.get("method").is_none() && message.get("id").is_some() {
                bail!("Unexpected LSP response ID");
            }
        }
        bail!("LSP notification budget exceeded")
    }
    fn shutdown(&mut self, deadline: &Deadline) -> Result<()> {
        self.request("shutdown", Value::Null, deadline)?;
        self.notify("exit", Value::Null)?;
        let stop =
            std::time::Instant::now() + deadline.remaining().min(std::time::Duration::from_secs(2));
        while self.child.try_wait()?.is_none() && std::time::Instant::now() < stop {
            deadline.check()?;
            std::thread::sleep(
                deadline
                    .remaining()
                    .min(std::time::Duration::from_millis(10)),
            );
        }
        deadline.check()?;
        Ok(())
    }
}
fn uri(path: &Path) -> Result<String> {
    Ok(Url::from_file_path(path)
        .map_err(|_| anyhow::anyhow!("Cannot encode local file URI"))?
        .to_string())
}
fn relative_uri(root: &Path, value: &str, snapshot: &Snapshot) -> Result<String> {
    let url = Url::parse(value)?;
    ensure!(
        url.scheme() == "file"
            && url.query().is_none()
            && url.fragment().is_none()
            && url
                .host_str()
                .is_none_or(|host| host.eq_ignore_ascii_case("localhost"))
            && url.username().is_empty()
            && url.password().is_none(),
        "Non-local LSP URI refused"
    );
    let path = url
        .to_file_path()
        .map_err(|_| anyhow::anyhow!("Invalid LSP file URI"))?;
    let local_root = Url::from_file_path(root)
        .map_err(|_| anyhow::anyhow!("Invalid captured root URI"))?
        .to_file_path()
        .map_err(|_| anyhow::anyhow!("Invalid captured root path"))?;
    #[cfg(windows)]
    let relative = {
        let root_parts: Vec<_> = local_root.components().collect();
        let path_parts: Vec<_> = path.components().collect();
        ensure!(
            path_parts.len() > root_parts.len()
                && root_parts.iter().zip(&path_parts).all(|(a, b)| a
                    .as_os_str()
                    .to_str()
                    .zip(b.as_os_str().to_str())
                    .is_some_and(|(a, b)| a.eq_ignore_ascii_case(b))),
            "LSP URI escaped captured workspace"
        );
        path_parts[root_parts.len()..]
            .iter()
            .collect::<std::path::PathBuf>()
    };
    #[cfg(not(windows))]
    let relative = path
        .strip_prefix(&local_root)
        .context("LSP URI escaped captured workspace")?
        .to_path_buf();
    let relative = relative
        .to_str()
        .ok_or_else(|| anyhow::anyhow!("Non-UTF-8 LSP path"))?
        .replace('\\', "/");
    // Reject scope and links before any target canonicalization; a provider
    // URI must not cause an outside or UNC filesystem/network lookup.
    let local = scoped(root, &relative)?;
    ensure!(
        snapshot.files.iter().any(|file| file.path == relative),
        "LSP path was not captured"
    );
    let canonical = local.canonicalize()?;
    let canonical_root = root.canonicalize()?;
    let relative = canonical
        .strip_prefix(&canonical_root)
        .context("LSP URI escaped captured workspace")?
        .to_str()
        .ok_or_else(|| anyhow::anyhow!("Non-UTF-8 LSP path"))?
        .replace('\\', "/");
    scoped(&canonical_root, &relative)?;
    ensure!(
        snapshot.files.iter().any(|f| f.path == relative),
        "LSP path was not captured"
    );
    Ok(relative)
}
fn range(text: &str, value: &Value) -> Result<(usize, usize)> {
    let start = position_to_byte(text, &value["start"])?;
    let end = position_to_byte(text, &value["end"])?;
    utf8_range(text, start, end)?;
    Ok((start, end))
}

fn check_copy(snapshot: &Snapshot, root: &Path, deadline: &Deadline) -> Result<()> {
    for file in &snapshot.files {
        deadline.check()?;
        ensure!(
            crate::model::hash(fs::read(scoped(root, &file.path)?)?) == file.hash,
            "Language server modified captured source"
        );
    }
    Ok(())
}
pub fn run(
    engine: &mut Engine,
    server: &Server,
    path: &str,
    offset: usize,
    operation: &str,
    new_name: Option<&str>,
    deadline: &Deadline,
) -> Result<Value> {
    let (snapshot, graph, _, _) = engine.index(deadline)?;
    run_at(
        engine,
        server,
        &snapshot,
        &graph,
        SemanticRequest {
            path,
            offset,
            operation,
            new_name,
        },
        deadline,
    )
}
pub struct SemanticRequest<'a> {
    pub path: &'a str,
    pub offset: usize,
    pub operation: &'a str,
    pub new_name: Option<&'a str>,
}
pub fn run_at(
    engine: &mut Engine,
    server: &Server,
    snapshot: &Snapshot,
    graph: &Graph,
    request: SemanticRequest<'_>,
    deadline: &Deadline,
) -> Result<Value> {
    let SemanticRequest {
        path,
        offset,
        operation,
        new_name,
    } = request;
    ensure!(server.trust, "Language server trust is required");
    let (text, _) = query::source(&engine.store, snapshot, path)?;
    let target_language = &snapshot
        .files
        .iter()
        .find(|file| file.path == path)
        .context("Semantic source file absent from snapshot")?
        .language;
    let position = byte_to_position(&text, offset)?;
    let tree = tempfile::Builder::new().prefix("yoke-lsp-").tempdir()?;
    changes::materialize(&engine.store, snapshot, tree.path(), deadline)?;
    let mut session = Session::spawn(server, tree.path(), deadline)?;
    let root_uri = uri(tree.path())?;
    let document_uri = uri(&tree.path().join(path))?;
    let initialization=session.request("initialize",json!({"processId":std::process::id(),"rootUri":root_uri,"rootPath":tree.path().display().to_string(),"workspaceFolders":[{"uri":root_uri,"name":"immutable-snapshot"}],"capabilities":{"general":{"positionEncodings":["utf-16"]},"window":{"workDoneProgress":true},"experimental":{"serverStatusNotification":true},"workspace":{"configuration":false,"applyEdit":false},"textDocument":{"rename":{"prepareSupport":false},"definition":{"linkSupport":true},"references":{}}},"initializationOptions":{"cargo":{"buildScripts":{"enable":false},"extraEnv":{"CARGO_NET_OFFLINE":"true"}},"procMacro":{"enable":false},"checkOnSave":false}}),deadline)?;
    let capabilities = &initialization["capabilities"];
    ensure!(capabilities.is_object(), "Server capabilities missing");
    ensure!(
        capabilities["positionEncoding"]
            .as_str()
            .unwrap_or("utf-16")
            == "utf-16",
        "Server selected unsupported position encoding"
    );
    session.notify("initialized", json!({}))?;
    for file in &snapshot.files {
        if file.path != path
            && provider_language(&file.language) == provider_language(&server.language)
        {
            deadline.check()?;
            session.notify("textDocument/didOpen", json!({"textDocument":{"uri":uri(&tree.path().join(&file.path))?,"languageId":document_language(&file.language),"version":1,"text":String::from_utf8(engine.store.read_blob(&file.hash)?)?}}))?;
        }
    }
    session.notify("textDocument/didOpen",json!({"textDocument":{"uri":document_uri,"languageId":document_language(target_language),"version":1,"text":text}}))?;
    if Path::new(&server.command)
        .file_stem()
        .is_some_and(|name| name == "rust-analyzer")
    {
        session.await_ready(deadline)?;
    }
    let (method, capability) = match operation {
        "definition" => ("textDocument/definition", "definitionProvider"),
        "declaration" => ("textDocument/declaration", "declarationProvider"),
        "references" => ("textDocument/references", "referencesProvider"),
        "implementation" => ("textDocument/implementation", "implementationProvider"),
        "hover" => ("textDocument/hover", "hoverProvider"),
        "rename" => ("textDocument/rename", "renameProvider"),
        "symbols" => ("textDocument/documentSymbol", "documentSymbolProvider"),
        _ => bail!("Unsupported semantic operation"),
    };
    ensure!(
        !capabilities[capability].is_null() && capabilities[capability] != false,
        "Language server does not advertise {operation}"
    );
    let mut params = json!({"textDocument":{"uri":document_uri},"position":position});
    if operation == "references" {
        params["context"] = json!({"includeDeclaration":true});
    }
    if operation == "rename" {
        let name = new_name.ok_or_else(|| anyhow::anyhow!("Rename name required"))?;
        ensure!(
            !name.is_empty() && name.len() <= 1000 && !name.contains(['\0', '\n', '\r']),
            "Invalid new name"
        );
        params["newName"] = json!(name);
    }
    let response = session.request(method, params, deadline)?;
    session.shutdown(deadline)?;
    drop(session);
    deadline.check()?;
    check_copy(snapshot, tree.path(), deadline)?;
    if operation == "rename" {
        ensure!(response.is_object(), "Rename result unavailable");
        ensure!(
            response.get("changes").is_some() != response.get("documentChanges").is_some(),
            "Ambiguous or absent workspace edit"
        );
        ensure!(
            response.get("changeAnnotations").is_none(),
            "Annotated edits require an unsupported approval contract"
        );
        let mut edits = Vec::new();
        let mut import = |uri: &str, values: &Value, version: Option<&Value>| -> Result<()> {
            deadline.check()?;
            if let Some(version) = version {
                ensure!(
                    version.is_null() || version == 1,
                    "Stale or unsupported document version"
                );
            }
            let relative = relative_uri(tree.path(), uri, snapshot)?;
            let (source, hash) = query::source(&engine.store, snapshot, &relative)?;
            let array = values
                .as_array()
                .ok_or_else(|| anyhow::anyhow!("Invalid text edits"))?;
            ensure!(array.len() <= 2000, "Edit count exceeded");
            for edit in array {
                deadline.check()?;
                ensure!(edit.get("annotationId").is_none(), "Annotated edit refused");
                let (start, end) = range(&source, &edit["range"])?;
                let text = edit["newText"]
                    .as_str()
                    .ok_or_else(|| anyhow::anyhow!("Edit replacement missing"))?;
                edits.push(json!({"kind":"replace","path":relative,"expectedHash":hash,"startByte":start,"endByte":end,"text":text}));
            }
            Ok(())
        };
        if let Some(changes) = response.get("changes") {
            for (uri, values) in changes
                .as_object()
                .ok_or_else(|| anyhow::anyhow!("Invalid changes map"))?
            {
                import(uri, values, None)?;
            }
        }
        if let Some(changes) = response.get("documentChanges") {
            for document in changes
                .as_array()
                .ok_or_else(|| anyhow::anyhow!("Invalid document changes"))?
            {
                ensure!(
                    document.get("kind").is_none(),
                    "LSP resource operations refused"
                );
                let uri = document["textDocument"]["uri"]
                    .as_str()
                    .ok_or_else(|| anyhow::anyhow!("Document URI missing"))?;
                import(
                    uri,
                    &document["edits"],
                    document["textDocument"].get("version"),
                )?;
            }
        }
        ensure!(edits.len() <= 2000, "Total edit count exceeded");
        let plan = changes::prepare(
            &mut engine.store,
            snapshot,
            graph,
            &json!({"edits":edits}),
            deadline,
        )?;
        deadline.check()?;
        return Ok(
            json!({"snapshotId":snapshot.id,"plan":plan,"producer":server.command,"semantic":"language-server-result","complete":false}),
        );
    }
    if matches!(operation, "hover" | "symbols") {
        deadline.check()?;
        return Ok(
            json!({"snapshotId":snapshot.id,"response":response,"producer":server.command,"trust":"source-data","complete":false}),
        );
    }
    let list = if response.is_null() {
        Vec::new()
    } else if let Some(array) = response.as_array() {
        array.clone()
    } else {
        vec![response]
    };
    ensure!(list.len() <= 10000, "Location count exceeded");
    let mut locations = Vec::new();
    let mut sources = HashMap::new();
    let mut cached_bytes = 0usize;
    let mut body_bytes = 0usize;
    for location in list {
        deadline.check()?;
        let uri = location["uri"]
            .as_str()
            .or_else(|| location["targetUri"].as_str())
            .ok_or_else(|| anyhow::anyhow!("Location URI missing"))?;
        let relative = relative_uri(tree.path(), uri, snapshot)?;
        if !sources.contains_key(&relative) {
            let (text, hash) = query::source(&engine.store, snapshot, &relative)?;
            if cached_bytes.saturating_add(text.len()) > 16_000_000 {
                sources.clear();
                cached_bytes = 0;
            }
            cached_bytes += text.len();
            sources.insert(relative.clone(), (text, hash));
        }
        let (text, hash) = &sources[&relative];
        let value = location
            .get("range")
            .or_else(|| location.get("targetSelectionRange"))
            .ok_or_else(|| anyhow::anyhow!("Location range missing"))?;
        let (start, end) = range(text, value)?;
        if location.get("targetUri").is_some() {
            let (target_start, target_end) = range(text, &location["targetRange"])?;
            ensure!(
                start >= target_start && end <= target_end,
                "LocationLink selection escaped target range"
            );
        }
        let mut item = json!({"path":relative,"contentHash":hash,"startByte":start,"endByte":end,"producer":server.command,"resolution":"exact_binding","trust":"source-data"});
        let size = end - start;
        if size <= 6000 && size <= 256_000usize.saturating_sub(body_bytes) {
            item["text"] = json!(&text[start..end]);
            body_bytes += size;
        } else {
            item["bodyOmitted"] = json!(true);
            item["readWith"] = json!({"tool":"ci_read","snapshotId":snapshot.id,"path":relative,"startByte":start,"endByte":end});
        }
        locations.push(item);
    }
    deadline.check()?;
    Ok(
        json!({"snapshotId":snapshot.id,"locations":locations,"producer":server.command,"complete":false,"limitations":["Checked source binding does not establish exhaustive language-server coverage."]}),
    )
}

#[cfg(test)]
mod uri_tests {
    use super::*;
    static PROTOCOL_FIXTURE: std::sync::Mutex<()> = std::sync::Mutex::new(());
    fn fixture(adversary: &str, root: &Path, setup: &Deadline) -> Session {
        let server = Server {
            command: "node".into(),
            args: vec![
                Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("tests/fixtures/lsp.mjs")
                    .display()
                    .to_string(),
                adversary.into(),
            ],
            language: "rust".into(),
            trust: true,
        };
        let mut session = Session::spawn(&server, root, setup).unwrap();
        session
            .request("initialize", json!({"rootUri":uri(root).unwrap()}), setup)
            .unwrap();
        session
    }
    #[test]
    fn blocked_pipe_write_uses_300ms_deadline_after_confirmed_server_pause() {
        let _fixture = PROTOCOL_FIXTURE
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        let directory = tempfile::tempdir().unwrap();
        let setup = Deadline::new(10000);
        let mut session = fixture("blocked", directory.path(), &setup);
        let paused = session
            .messages
            .recv_timeout(setup.remaining())
            .unwrap()
            .unwrap();
        assert_eq!(paused["method"], "fixture/blocked");
        assert_eq!(paused["params"]["paused"], true);
        let started = std::time::Instant::now();
        session.deadline = Deadline::new(300);
        let result = session.notify(
            "textDocument/didOpen",
            json!({"textDocument":{
            "uri":uri(&directory.path().join("main.rs")).unwrap(),"languageId":"rust",
            "version":1,"text":"x".repeat(200000)}}),
        );
        let error = result.unwrap_err();
        assert!(
            error.to_string().contains("input write exceeded deadline"),
            "{error}"
        );
        drop(session);
        assert!(
            started.elapsed() < std::time::Duration::from_secs(2),
            "Blocked protocol write and process cleanup outlived deadline: {:?}",
            started.elapsed()
        );
    }
    #[test]
    fn slow_shutdown_uses_800ms_deadline_after_completed_protocol_response() {
        let _fixture = PROTOCOL_FIXTURE
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        let directory = tempfile::tempdir().unwrap();
        let setup = Deadline::new(10000);
        let mut session = fixture("slow-exit", directory.path(), &setup);
        session
            .notify(
                "textDocument/didOpen",
                json!({"textDocument":{
            "uri":uri(&directory.path().join("main.rs")).unwrap(),"languageId":"rust",
            "version":1,"text":"fn helper() {}"}}),
            )
            .unwrap();
        let response = session
            .request("textDocument/definition", json!({}), &setup)
            .unwrap();
        assert!(response["range"].is_object());
        let started = std::time::Instant::now();
        let deadline = Deadline::new(800);
        session.deadline = deadline.clone();
        let error = session.shutdown(&deadline).unwrap_err();
        assert!(error.to_string().contains("deadline"), "{error}");
        let notified = session.messages.try_recv().unwrap().unwrap();
        assert_eq!(notified["method"], "fixture/slow-exit");
        drop(session);
        assert!(
            started.elapsed() < std::time::Duration::from_secs(2),
            "Independent shutdown grace outlived shared request deadline: {:?}",
            started.elapsed()
        );
    }
    #[test]
    fn react_documents_and_siblings_keep_their_ids_with_base_language_providers() {
        let _fixture = PROTOCOL_FIXTURE
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        for (target, sibling, provider) in [
            ("component.tsx", "support.ts", "typescript"),
            ("component.jsx", "support.js", "javascript"),
        ] {
            let directory = tempfile::tempdir().unwrap();
            fs::write(
                directory.path().join(target),
                "// component\nconst helper = () => <div />;\n",
            )
            .unwrap();
            fs::write(
                directory.path().join(sibling),
                "export function support() {}\n",
            )
            .unwrap();
            fs::write(
                directory.path().join("unrelated.py"),
                "def unrelated(): pass\n",
            )
            .unwrap();
            let mut engine = Engine::open(directory.path()).unwrap();
            let deadline = Deadline::new(10000);
            let (snapshot, graph, _, _) = engine.index(&deadline).unwrap();
            let server = Server {
                command: "node".into(),
                args: vec![
                    Path::new(env!("CARGO_MANIFEST_DIR"))
                        .join("tests/fixtures/lsp.mjs")
                        .display()
                        .to_string(),
                    "react-language-ids".into(),
                ],
                language: provider.into(),
                trust: true,
            };
            let result = run_at(
                &mut engine,
                &server,
                &snapshot,
                &graph,
                SemanticRequest {
                    path: target,
                    offset: 0,
                    operation: "definition",
                    new_name: None,
                },
                &deadline,
            )
            .unwrap();
            assert_eq!(result["locations"][0]["path"], target);
        }
    }
    #[test]
    fn an_outside_uri_is_rejected_lexically_before_a_filesystem_lookup() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().join("copied-workspace");
        fs::create_dir(&root).unwrap();
        let source = Snapshot {
            id: "a".repeat(64),
            workspace_id: "b".repeat(64),
            kind: "disk".into(),
            files: vec![],
            omissions: vec![],
            capture_consistency: "test".into(),
        };
        let outside = uri(&directory.path().join("outside-does-not-exist.rs")).unwrap();
        let error = relative_uri(&root, &outside, &source).unwrap_err();
        assert!(
            error.to_string().contains("escaped captured workspace"),
            "Outside path reached filesystem lookup: {error}"
        );
    }
    #[test]
    fn nonlocal_file_authorities_are_refused_without_target_lookup() {
        let directory = tempfile::tempdir().unwrap();
        let source = Snapshot {
            id: "a".repeat(64),
            workspace_id: "b".repeat(64),
            kind: "disk".into(),
            files: vec![],
            omissions: vec![],
            capture_consistency: "test".into(),
        };
        let started = std::time::Instant::now();
        let error = relative_uri(
            directory.path(),
            "file://yoke-nonlocal.invalid/share/private.rs",
            &source,
        )
        .unwrap_err();
        assert!(error.to_string().contains("Non-local LSP URI refused"));
        assert!(started.elapsed() < std::time::Duration::from_millis(500));
    }
}
