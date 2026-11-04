use crate::{
    Engine,
    capture::Deadline,
    changes,
    model::{Graph, Snapshot, identity},
    paths::{scoped, utf8_range},
    query,
};
use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
#[cfg(test)]
use std::fs;
use std::{
    collections::{BTreeMap, HashMap, HashSet, VecDeque},
    io::{BufRead, BufReader, Read, Write},
    net::{Shutdown, SocketAddr, TcpStream},
    path::Path,
    process::{Command, Stdio},
    sync::mpsc::{self, Receiver, Sender, SyncSender},
    thread,
};
use url::Url;

#[path = "lsp_refactor_intents.rs"]
mod refactor_intents;
pub(crate) use refactor_intents::stage_move_intent_at;
#[path = "lsp_resource_settings.rs"]
mod resource_settings;
#[cfg(test)]
#[path = "source_encoding_lsp_tests.rs"]
mod source_encoding_tests;
pub(crate) use resource_settings::{
    effective as effective_resource_settings, validate_entries as validate_resource_entries,
};

fn receive_cooperatively<T>(channel: &Receiver<T>, deadline: &Deadline) -> Result<T> {
    loop {
        deadline.check()?;
        match channel.recv_timeout(
            deadline
                .remaining()
                .min(std::time::Duration::from_millis(25)),
        ) {
            Ok(value) => {
                deadline.check()?;
                return Ok(value);
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(error @ mpsc::RecvTimeoutError::Disconnected) => return Err(error.into()),
        }
    }
}
#[derive(Clone)]
pub struct Server {
    pub command: String,
    pub args: Vec<String>,
    pub language: String,
    pub trust: bool,
}
#[derive(Clone, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct HostConfiguration {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub passive_profile: Option<PassiveProfile>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub allow_source_execution: bool,
    #[serde(default)]
    pub tcp_endpoint: Option<SocketAddr>,
    #[serde(default)]
    pub initialization_options: Value,
    #[serde(default)]
    pub settings: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resource_settings: Option<BTreeMap<String, serde_json::Map<String, Value>>>,
    #[serde(default)]
    pub workspace_folders: Vec<String>,
    #[serde(default)]
    pub language_ids: BTreeMap<String, String>,
    #[serde(default)]
    pub open_languages: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub refactor_intent_profile: Option<RefactorIntentProfile>,
}
#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq)]
pub enum RefactorIntentProfile {
    #[serde(rename = "own_v1")]
    OwnV1,
}
impl<'de> Deserialize<'de> for RefactorIntentProfile {
    fn deserialize<D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> std::result::Result<Self, D::Error> {
        match String::deserialize(deserializer)?.as_str() {
            "own_v1" => Ok(Self::OwnV1),
            _ => Err(serde::de::Error::custom(
                "Unsupported owned refactor intent profile",
            )),
        }
    }
}
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PassiveProfile {
    RustAnalyzer,
    TypescriptLanguageServer,
    JediLanguageServer,
    ContractFixture,
}
fn is_false(value: &bool) -> bool {
    !*value
}
#[derive(Clone)]
pub struct ConfiguredServer {
    pub server: Server,
    pub configuration: HostConfiguration,
}
fn default_items() -> usize {
    200
}
fn default_depth() -> usize {
    1
}
fn default_severity() -> u64 {
    4
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct QueryOptions {
    #[serde(default = "default_items")]
    max_items: usize,
    #[serde(default = "default_depth")]
    depth: usize,
    #[serde(default)]
    include_body: bool,
    #[serde(default)]
    include_declaration: Option<bool>,
    #[serde(default = "default_severity")]
    min_severity: u64,
    #[serde(default)]
    start_byte: Option<usize>,
    #[serde(default)]
    end_byte: Option<usize>,
    #[serde(default)]
    only: Vec<String>,
    #[serde(default)]
    include_kinds: Vec<u64>,
    #[serde(default)]
    exclude_kinds: Vec<u64>,
    #[serde(default)]
    name_path_pattern: Option<String>,
    #[serde(default)]
    rename_path: Option<String>,
}
impl QueryOptions {
    fn parse(value: &Value) -> Result<Self> {
        ensure!(
            serde_json::to_vec(value)?.len() <= 64000,
            "Semantic option budget exceeded"
        );
        let options: Self = crate::json::decode(value.clone())?;
        ensure!(
            options.max_items > 0 && options.max_items <= 2000 && options.depth <= 8,
            "Semantic result or depth budget invalid"
        );
        ensure!(
            (1..=4).contains(&options.min_severity)
                && options.only.len() <= 20
                && options.only.iter().all(|kind| kind.len() <= 200),
            "Invalid semantic filters"
        );
        ensure!(
            options.include_kinds.len() <= 26
                && options.exclude_kinds.len() <= 26
                && options
                    .include_kinds
                    .iter()
                    .chain(&options.exclude_kinds)
                    .all(|kind| (1..=26).contains(kind)),
            "Invalid symbol-kind filters"
        );
        ensure!(
            options
                .name_path_pattern
                .as_ref()
                .is_none_or(|pattern| !pattern.is_empty() && pattern.len() <= 1000),
            "Invalid name path filter"
        );
        if let Some(path) = &options.rename_path {
            ensure!(
                path.len() <= 4096
                    && crate::paths::normalized(path)? == *path
                    && !crate::paths::protected(path),
                "Invalid source-bound rename path"
            );
        }
        Ok(options)
    }
}
pub const READ_OPERATIONS: &[&str] = &[
    "definition",
    "declaration",
    "references",
    "implementation",
    "hover",
    "symbols",
    "typeDefinition",
    "incomingCalls",
    "outgoingCalls",
    "supertypes",
    "subtypes",
    "completion",
    "signatureHelp",
    "diagnostics",
    "symbolInfo",
    "prepareRename",
    "codeActions",
    "willRenameFiles",
    "capabilities",
];
pub fn validate_query_options(value: &Value) -> Result<()> {
    QueryOptions::parse(value).map(|_| ())
}

/// Refresh the actual host-registered providers used by this captured project.
/// Providers currently have a per-request lifecycle. This negotiates new
/// sessions and shuts down only owned processes; it never claims to restart an
/// external TCP peer or invalidates published immutable semantic evidence.
pub fn restart_registered(engine: &mut Engine, deadline: &Deadline) -> Result<Value> {
    use rusqlite::{OptionalExtension, params};
    deadline.check()?;
    ensure!(
        engine
            .selected_backends
            .as_ref()
            .is_some_and(|ids| !ids.is_empty() && ids.len() <= 32)
            || (!engine.automatic_backend_selection
                && !engine.servers.is_empty()
                && engine.servers.len() <= 100),
        "Restart requires bounded host-registered providers"
    );
    let (snapshot, graph, _, _) = engine.index(deadline)?;
    let registrations: Vec<_> = if let Some(ids) = &engine.selected_backends {
        ids.iter()
            .map(|id| {
                let provider = engine
                    .backend_servers
                    .get(id)
                    .context("language_server_not_host_registered")?;
                Ok((
                    provider.server.language.clone(),
                    provider.server.clone(),
                    Some(provider.configuration.clone()),
                    Some(id.clone()),
                ))
            })
            .collect::<Result<Vec<_>>>()?
    } else {
        engine
            .servers
            .iter()
            .map(|(language, server)| {
                (
                    language.clone(),
                    server.clone(),
                    engine.server_configurations.get(language).cloned(),
                    None,
                )
            })
            .collect()
    };
    let mut providers = Vec::new();
    let mut absent = Vec::new();
    for (language, server, configuration, backend_id) in registrations {
        deadline.check()?;
        let mut selected_file = None;
        for file in &snapshot.files {
            deadline.check()?;
            let matches = if let Some(id) = &backend_id {
                engine.selected_backend_id(&file.path)? == Some(id.as_str())
            } else {
                provider_language(&file.language) == provider_language(&language)
            };
            if matches {
                selected_file = Some(file);
                break;
            }
        }
        let Some(file) = selected_file else {
            let mut value = json!({"language":language,"reason":"no_captured_source"});
            if let Some(id) = &backend_id {
                value["backendId"] = json!(id);
                value["reason"] = json!("no_captured_source_selected_for_backend");
            }
            absent.push(value);
            continue;
        };
        crate::capture::validate_language(&engine.store, &snapshot, &file.path)?;
        let request = SemanticRequest {
            path: &file.path,
            offset: 0,
            operation: "capabilities",
            new_name: None,
        };
        let tcp = configuration
            .as_ref()
            .is_some_and(|config| config.tcp_endpoint.is_some());
        let health = if let Some(configuration) = configuration {
            run_at_configured(
                engine,
                &ConfiguredServer {
                    server,
                    configuration,
                },
                &snapshot,
                &graph,
                request,
                &json!({}),
                deadline,
            )?
        } else {
            run_at_with_options(
                engine,
                &server,
                &snapshot,
                &graph,
                request,
                &json!({}),
                deadline,
            )?
        };
        ensure!(
            health["snapshotId"] == snapshot.id
                && health["contentHash"] == file.hash
                && health["serverProfileHash"]
                    .as_str()
                    .is_some_and(crate::model::valid_id),
            "Provider health source/profile binding mismatch"
        );
        let mut provider = json!({
            "language":language,"path":file.path,"contentHash":file.hash,
            "serverProfileHash":health["serverProfileHash"],
            "providerSourceOmissions":health["providerSourceOmissions"],
            "providerSourcePolicy":health["providerSourcePolicy"],
            "sourceExecutionAuthorized":health["sourceExecutionAuthorized"],
            "codeActionKinds":health["codeActionKinds"],
            "transport":if tcp{"existing_tcp_endpoint"}else{"owned_stdio_process"},
            "initialized":true,"closed":true,"peerProcessRestarted":false,
        });
        if let Some(id) = &backend_id {
            provider["backendId"] = json!(id);
            provider["backendSelection"] = health["backendSelection"].clone();
        }
        providers.push(provider);
    }
    ensure!(
        !providers.is_empty(),
        "No registered provider has captured project source"
    );
    deadline.check()?;
    // Lifecycle status is intentionally mutable operational state, separate
    // from immutable artifacts and their retention record kinds.
    let transaction = engine.store.db.transaction()?;
    transaction.execute_batch("CREATE TABLE IF NOT EXISTS native_lsp_lifecycle(workspace_id TEXT PRIMARY KEY NOT NULL,generation INTEGER NOT NULL,status TEXT NOT NULL)")?;
    let prior: Option<(i64, String)> = transaction.query_row(
        "SELECT generation,status FROM native_lsp_lifecycle WHERE workspace_id=?1 AND length(status)<=512000",
        [&snapshot.workspace_id], |row| Ok((row.get(0)?, row.get(1)?)),
    ).optional()?;
    let generation = if let Some((generation, status)) = prior {
        let prior = crate::json::parse(status.as_bytes(), 512_000)?;
        ensure!(
            generation > 0
                && generation < i64::MAX
                && prior["schemaVersion"] == 1
                && prior["generation"] == generation
                && prior["workspaceId"] == snapshot.workspace_id,
            "Invalid prior provider lifecycle status"
        );
        generation + 1
    } else {
        let exists: bool = transaction.query_row(
            "SELECT EXISTS(SELECT 1 FROM native_lsp_lifecycle WHERE workspace_id=?1)",
            [&snapshot.workspace_id],
            |row| row.get(0),
        )?;
        ensure!(!exists, "Prior provider lifecycle status exceeds capacity");
        1
    };
    let status = json!({
        "schemaVersion":1,"workspaceId":snapshot.workspace_id,"snapshotId":snapshot.id,
        "generation":generation,"lifecycle":"per_request_reinitialization",
        "providers":providers,"notInitialized":absent,"persistentProcessesRestarted":0,
        "unpublishedOverlayCount":0,"publishedArtifactsPreserved":true,
    });
    let serialized = serde_json::to_string(&status)?;
    ensure!(
        serialized.len() <= 512_000,
        "Provider lifecycle status exceeds capacity"
    );
    transaction.execute(
        "INSERT INTO native_lsp_lifecycle(workspace_id,generation,status) VALUES(?1,?2,?3) ON CONFLICT(workspace_id) DO UPDATE SET generation=excluded.generation,status=excluded.status",
        params![snapshot.workspace_id, generation, serialized],
    )?;
    deadline.check()?;
    transaction.commit()?;
    Ok(status)
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
    let value = read_content_frame(input)?;
    ensure!(
        value.is_object() && value["jsonrpc"] == "2.0",
        "Invalid LSP JSON-RPC message"
    );
    Ok(value)
}
pub(crate) fn read_content_frame(input: &mut impl BufRead) -> Result<Value> {
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
    crate::json::parse(&bytes, 4_000_000)
}
struct Session {
    child: Option<crate::process::ManagedChild>,
    socket: Option<TcpStream>,
    input: SyncSender<(Vec<u8>, Sender<Result<()>>)>,
    deadline: Deadline,
    messages: Receiver<Result<Value>>,
    next: u64,
    diagnostics: Vec<Value>,
    diagnostic_bytes: usize,
    quiescent: bool,
    settings: Value,
    resource_settings: Option<resource_settings::Binding>,
    workspace_folders: Option<Value>,
    _cargo_home: Option<tempfile::TempDir>,
}
impl Session {
    #[cfg(test)]
    fn spawn(server: &Server, root: &Path, deadline: &Deadline) -> Result<Self> {
        Self::spawn_scoped(server, root, false, deadline)
    }
    fn spawn_scoped(
        server: &Server,
        root: &Path,
        passive_rust: bool,
        deadline: &Deadline,
    ) -> Result<Self> {
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
            "SystemDrive",
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
        let cargo_home = if passive_rust {
            let directory = tempfile::Builder::new()
                .prefix("yoke-passive-cargo-")
                .tempdir()?;
            command
                .env("CARGO_HOME", directory.path())
                .env("CARGO_NET_OFFLINE", "true")
                .env("RUSTC_WRAPPER", "")
                .env("RUSTC_WORKSPACE_WRAPPER", "")
                .env("CARGO_BUILD_RUSTC_WRAPPER", "")
                .env("CARGO_BUILD_RUSTC_WORKSPACE_WRAPPER", "");
            Some(directory)
        } else {
            None
        };
        let mut child = crate::process::ManagedChild::spawn(command)
            .context("Unable to start host-selected language server")?;
        let stdin = child.stdin().take().unwrap();
        let stdout = child.stdout().take().unwrap();
        let stderr = child.stderr().take().unwrap();
        let mut session = Self::from_io(
            (Some(child), None),
            (Box::new(stdin), Box::new(stdout)),
            Some(Box::new(stderr)),
            deadline,
        )?;
        session._cargo_home = cargo_home;
        Ok(session)
    }
    fn connect(server: &Server, endpoint: SocketAddr, deadline: &Deadline) -> Result<Self> {
        ensure!(
            server.trust,
            "Connecting a language server requires explicit host trust"
        );
        ensure!(
            endpoint.ip().is_loopback() && endpoint.port() != 0,
            "LSP TCP requires an explicit numeric loopback endpoint"
        );
        deadline.check()?;
        let socket = TcpStream::connect_timeout(&endpoint, deadline.remaining())
            .context("Unable to connect to the host-selected LSP endpoint")?;
        socket.set_read_timeout(Some(deadline.remaining()))?;
        socket.set_write_timeout(Some(deadline.remaining()))?;
        let input = socket.try_clone()?;
        let output = socket.try_clone()?;
        Self::from_io(
            (None, Some(socket)),
            (Box::new(input), Box::new(output)),
            None,
            deadline,
        )
    }
    fn from_io(
        owned: (Option<crate::process::ManagedChild>, Option<TcpStream>),
        streams: (Box<dyn Write + Send>, Box<dyn Read + Send>),
        stderr: Option<Box<dyn Read + Send>>,
        deadline: &Deadline,
    ) -> Result<Self> {
        let (child, socket) = owned;
        let (mut stdin, stdout) = streams;
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
        if let Some(mut stderr) = stderr {
            thread::spawn(move || {
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
        }
        Ok(Self {
            child,
            socket,
            input,
            deadline: deadline.clone(),
            messages,
            next: 0,
            diagnostics: Vec::new(),
            diagnostic_bytes: 0,
            quiescent: false,
            settings: Value::Null,
            resource_settings: None,
            workspace_folders: None,
            _cargo_home: None,
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
        receive_cooperatively(&completed, &self.deadline)
            .context("Language server input write exceeded deadline")??;
        Ok(())
    }
    fn notify(&mut self, method: &str, params: Value) -> Result<()> {
        self.send(json!({"jsonrpc":"2.0","method":method,"params":params}))
    }
    fn callback(&mut self, message: &Value) -> Result<()> {
        if message["method"] == "textDocument/publishDiagnostics" {
            ensure!(
                self.diagnostics.len() < 200,
                "Diagnostic notification budget exceeded"
            );
            let bytes = serde_json::to_vec(&message["params"])?.len();
            ensure!(
                self.diagnostic_bytes.saturating_add(bytes) <= 8_000_000,
                "Diagnostic notification byte budget exceeded"
            );
            self.diagnostic_bytes += bytes;
            self.diagnostics.push(message["params"].clone());
        }
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
                "workspace/workspaceFolders" if self.workspace_folders.is_some() => {
                    ensure!(
                        message["params"].is_null()
                            || message["params"]
                                .as_object()
                                .is_some_and(serde_json::Map::is_empty),
                        "Invalid workspace folders callback parameters"
                    );
                    self.workspace_folders.clone()
                }
                "workspace/configuration" => {
                    let items = message["params"]["items"]
                        .as_array()
                        .context("Configuration callback items missing")?;
                    ensure!(
                        items.len() <= 64,
                        "Configuration callback item budget exceeded"
                    );
                    let mut selected = Vec::new();
                    let mut bytes =
                        1024usize.saturating_add(serde_json::to_vec(&message["id"])?.len());
                    for item in items {
                        ensure!(
                            item.is_object(),
                            "Configuration callback item must be an object"
                        );
                        let mut value = match &self.resource_settings {
                            Some(binding) => {
                                binding.select(item.get("scopeUri"), &self.settings)?
                            }
                            None => &self.settings,
                        };
                        if let Some(section) = item.get("section") {
                            let section = section
                                .as_str()
                                .context("Configuration callback section must be a string")?;
                            ensure!(
                                section.len() <= 1000,
                                "Configuration callback section budget exceeded"
                            );
                            for part in section.split('.') {
                                value = &value[part];
                            }
                        }
                        bytes = bytes.saturating_add(serde_json::to_vec(value)?.len() + 1);
                        ensure!(
                            bytes <= 4_000_000,
                            "Configuration callback output budget exceeded"
                        );
                        selected.push(value);
                    }
                    Some(Value::Array(selected.into_iter().cloned().collect()))
                }
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
            let message = receive_cooperatively(&self.messages, deadline)
                .context("Language server did not finish loading the snapshot")??;
            ensure!(
                message.get("method").is_some(),
                "Unexpected response during workspace loading"
            );
            self.callback(&message)?;
        }
        bail!("Workspace loading notification budget exceeded")
    }
    fn await_diagnostics(
        &mut self,
        root: &Path,
        snapshot: &Snapshot,
        scope: &crate::capture::CapturedSourceScope<'_>,
        path: &str,
        deadline: &Deadline,
    ) -> Result<Value> {
        for _ in 0..4096 {
            deadline.check()?;
            if let Some(publication) = self.diagnostics.iter().rev().find(|publication| {
                publication["uri"]
                    .as_str()
                    .and_then(|value| relative_uri_scoped(root, value, snapshot, Some(scope)).ok())
                    .is_some_and(|relative| relative == path)
            }) {
                return Ok(publication.clone());
            }
            let message = receive_cooperatively(&self.messages, deadline)
                .context("Provider did not publish diagnostics for the captured document")??;
            ensure!(
                message.get("method").is_some(),
                "Unexpected response while waiting for diagnostics"
            );
            self.callback(&message)?;
        }
        bail!("Diagnostic publication notification budget exceeded")
    }
    fn request(&mut self, method: &str, params: Value, deadline: &Deadline) -> Result<Value> {
        self.request_message(method, Some(params), deadline)
    }
    fn request_message(
        &mut self,
        method: &str,
        params: Option<Value>,
        deadline: &Deadline,
    ) -> Result<Value> {
        self.next += 1;
        let id = self.next;
        let mut message = json!({"jsonrpc":"2.0","id":id,"method":method});
        if let Some(params) = params {
            message["params"] = params;
        }
        self.send(message)?;
        for _ in 0..4096 {
            deadline.check()?;
            let message = receive_cooperatively(&self.messages, deadline)
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
            if message.get("method").is_none() && message.get("id").is_some() {
                bail!("Unexpected LSP response ID");
            }
        }
        bail!("LSP notification budget exceeded")
    }
    fn shutdown(&mut self, deadline: &Deadline) -> Result<()> {
        if self.socket.is_some() {
            deadline.check()?;
            return Ok(());
        }
        // LSP lifecycle messages have no parameters. Explicit JSON null is a
        // different envelope and strict servers (including TSGo) reject it.
        self.request_message("shutdown", None, deadline)?;
        self.send(json!({"jsonrpc":"2.0","method":"exit"}))?;
        let stop =
            std::time::Instant::now() + deadline.remaining().min(std::time::Duration::from_secs(2));
        while self
            .child
            .as_mut()
            .context("Owned server process missing")?
            .try_wait()?
            .is_none()
            && std::time::Instant::now() < stop
        {
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
impl Drop for Session {
    fn drop(&mut self) {
        if let Some(socket) = &self.socket {
            let _ = socket.shutdown(Shutdown::Both);
        }
    }
}
fn uri(path: &Path) -> Result<String> {
    let value = Url::from_file_path(path)
        .map_err(|_| anyhow::anyhow!("Cannot encode local file URI"))?
        .to_string();
    #[cfg(windows)]
    let value = {
        let mut value = value;
        // Python/Jedi canonicalizes Windows drive letters. Open and request
        // documents under that same URI so versions remain attached to them.
        if value.starts_with("file:///")
            && value.as_bytes().get(8).is_some_and(u8::is_ascii_alphabetic)
            && value.as_bytes().get(9) == Some(&b':')
        {
            let drive = value[8..9].to_ascii_lowercase();
            value.replace_range(8..9, &drive);
        }
        value
    };
    Ok(value)
}
#[cfg(test)]
fn relative_uri(root: &Path, value: &str, snapshot: &Snapshot) -> Result<String> {
    relative_uri_scoped(root, value, snapshot, None)
}
fn read_source_path(
    root: &Path,
    relative: &str,
    scope: Option<&crate::capture::CapturedSourceScope<'_>>,
) -> Result<std::path::PathBuf> {
    if let Some(scope) = scope {
        scope.file(relative)?;
        crate::paths::normalized_syntax(relative)?;
        let mut current = root.to_path_buf();
        ensure!(
            !crate::paths::is_link(&std::fs::symlink_metadata(&current)?),
            "Source root link refused"
        );
        for part in relative.split('/') {
            current.push(part);
            ensure!(
                !crate::paths::is_link(&std::fs::symlink_metadata(&current)?),
                "Source link or reparse point refused"
            );
        }
        Ok(current)
    } else {
        scoped(root, relative)
    }
}
fn relative_uri_scoped(
    root: &Path,
    value: &str,
    snapshot: &Snapshot,
    scope: Option<&crate::capture::CapturedSourceScope<'_>>,
) -> Result<String> {
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
    let local = read_source_path(root, &relative, scope)?;
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
    read_source_path(&canonical_root, &relative, scope)?;
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

struct Normalizer<'a> {
    snapshot: &'a Snapshot,
    scope: crate::capture::CapturedSourceScope<'a>,
    root: &'a Path,
    producer: &'a str,
    deadline: &'a Deadline,
    sources: HashMap<String, (crate::source_encoding::View, String)>,
    cached_bytes: usize,
    body_bytes: usize,
    records: usize,
}
impl<'a> Normalizer<'a> {
    fn new(
        store: &'a crate::store::Store,
        snapshot: &'a Snapshot,
        root: &'a Path,
        producer: &'a str,
        deadline: &'a Deadline,
    ) -> Result<Self> {
        Ok(Self {
            snapshot,
            scope: crate::capture::source_scope_policy(store, snapshot)?,
            root,
            producer,
            deadline,
            sources: HashMap::new(),
            cached_bytes: 0,
            body_bytes: 0,
            records: 0,
        })
    }
    fn source(&mut self, path: &str) -> Result<(&crate::source_encoding::View, &str)> {
        self.deadline.check()?;
        if !self.sources.contains_key(path) {
            let text = self.scope.source_view(path)?;
            let hash = self.scope.file(path)?.hash.clone();
            let size = if text.policy().is_some() {
                text.retained_bytes()
            } else {
                text.text().len()
            };
            ensure!(
                size <= 16_000_000,
                "source_encoding_capability_unavailable: semantic mapping cache capacity"
            );
            if self.cached_bytes.saturating_add(size) > 16_000_000 {
                self.sources.clear();
                self.cached_bytes = 0;
            }
            self.cached_bytes += size;
            self.sources.insert(path.into(), (text, hash));
        }
        let (text, hash) = &self.sources[path];
        Ok((text, hash))
    }
    fn anchor(&mut self, path: &str, span: &Value, body: bool) -> Result<Value> {
        self.records += 1;
        ensure!(
            self.records <= 20000,
            "Semantic source-anchor budget exceeded"
        );
        let body_available = 256_000usize.saturating_sub(self.body_bytes);
        let (view, hash) = self.source(path)?;
        let text = view.text();
        let (start, end) = range(text, span)?;
        let excerpt = if body && end - start <= 6000 && end - start <= body_available {
            Some(text[start..end].to_owned())
        } else {
            None
        };
        let start = view.to_raw(start)?;
        let end = view.to_raw(end)?;
        let mut value = json!({"path":path,"contentHash":hash,"startByte":start,"endByte":end});
        view.annotate(&mut value, hash);
        value["snapshotId"] = json!(self.snapshot.id);
        value["producer"] = json!(self.producer);
        value["resolution"] = json!("exact_binding");
        if let Some(text) = excerpt {
            self.body_bytes += text.len();
            value["text"] = json!(text);
        } else if body {
            value["bodyOmitted"] = json!(true);
            value["readWith"] = json!({"tool":"ci_read","snapshotId":self.snapshot.id,"path":path,"startByte":start,"endByte":end});
        }
        Ok(value)
    }
    fn item(&mut self, item: &Value, body: bool) -> Result<Value> {
        let path = relative_uri_scoped(
            self.root,
            item["uri"].as_str().context("Hierarchy item URI missing")?,
            self.snapshot,
            Some(&self.scope),
        )?;
        let mut value = self.anchor(&path, &item["range"], body)?;
        let selection = self.anchor(&path, &item["selectionRange"], false)?;
        ensure!(
            selection["startByte"].as_u64() >= value["startByte"].as_u64()
                && selection["endByte"].as_u64() <= value["endByte"].as_u64(),
            "Symbol selection escaped its range"
        );
        let name = item["name"].as_str().context("Symbol name missing")?;
        ensure!(
            !name.is_empty() && name.len() <= 1000,
            "Symbol name exceeds budget"
        );
        let kind = item["kind"].as_u64().context("Symbol kind missing")?;
        ensure!((1..=26).contains(&kind), "Invalid symbol kind");
        value["name"] = json!(name);
        value["kind"] = json!(kind);
        value["selectionStartByte"] = selection["startByte"].clone();
        value["selectionEndByte"] = selection["endByte"].clone();
        if let Some(detail) = item.get("detail") {
            ensure!(
                detail.as_str().is_some_and(|text| text.len() <= 6000),
                "Symbol detail exceeds budget"
            );
            value["detail"] = detail.clone();
        }
        value["id"] = json!(identity(&(
            self.snapshot.id.as_str(),
            self.producer,
            &value
        )));
        Ok(value)
    }
    fn symbols(
        &mut self,
        path: &str,
        values: &[Value],
        parent_context: (&str, Option<(u64, u64)>, usize),
        options: &QueryOptions,
        output: &mut Vec<Value>,
    ) -> Result<()> {
        let (parent, parent_range, depth) = parent_context;
        ensure!(
            depth <= 64 && values.len() <= 10000,
            "Symbol hierarchy budget exceeded"
        );
        let mut totals = HashMap::<String, usize>::new();
        for value in values {
            if let Some(name) = value["name"].as_str() {
                *totals.entry(name.into()).or_default() += 1;
            }
        }
        let mut indexes = HashMap::<String, usize>::new();
        for item in values {
            self.deadline.check()?;
            let mut item = item.clone();
            if item.get("location").is_some() {
                item["uri"] = item["location"]["uri"].clone();
                item["range"] = item["location"]["range"].clone();
                item["selectionRange"] = item["range"].clone();
            } else {
                item["uri"] = json!(uri(&self.root.join(path))?);
            }
            let mut normalized = self.item(&item, options.include_body)?;
            let start = normalized["startByte"].as_u64().unwrap();
            let end = normalized["endByte"].as_u64().unwrap();
            ensure!(
                parent_range.is_none_or(|(a, b)| start >= a && end <= b),
                "Child symbol escaped parent range"
            );
            let name = normalized["name"].as_str().unwrap().to_owned();
            let component = if totals.get(&name).copied().unwrap_or(0) > 1 {
                let index = indexes.entry(name.clone()).or_default();
                let value = format!("{name}[{index}]");
                *index += 1;
                value
            } else {
                name
            };
            let name_path = if parent.is_empty() {
                component
            } else {
                format!("{parent}/{component}")
            };
            normalized["namePath"] = json!(name_path);
            normalized["depth"] = json!(depth);
            let kind = normalized["kind"].as_u64().unwrap();
            let selected =
                options.include_kinds.is_empty() || options.include_kinds.contains(&kind);
            let named = options.name_path_pattern.as_ref().is_none_or(|pattern| {
                name_path == *pattern
                    || name_path
                        .strip_suffix(pattern)
                        .is_some_and(|prefix| prefix.ends_with('/'))
            });
            if selected && !options.exclude_kinds.contains(&kind) && named {
                output.push(normalized);
            }
            if let Some(children) = item.get("children") {
                self.symbols(
                    path,
                    children.as_array().context("Invalid symbol children")?,
                    (&name_path, Some((start, end)), depth + 1),
                    options,
                    output,
                )?;
            }
        }
        Ok(())
    }
}

fn requested_code_action(action: &Value, kinds: &[String]) -> bool {
    kinds.is_empty()
        || action["kind"].as_str().is_some_and(|kind| {
            kinds.iter().any(|wanted| {
                kind == wanted
                    || kind
                        .strip_prefix(wanted)
                        .is_some_and(|suffix| suffix.starts_with('.'))
            })
        })
}

fn bounded_items(value: &Value) -> Result<Vec<Value>> {
    if value.is_null() {
        return Ok(Vec::new());
    }
    let values = value.as_array().context("Expected semantic item array")?;
    ensure!(values.len() <= 10000, "Semantic item count exceeded");
    Ok(values.clone())
}

fn edit_relative(root: &Path, value: &str) -> Result<String> {
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
        "Non-local resource URI refused"
    );
    let path = url
        .to_file_path()
        .map_err(|_| anyhow::anyhow!("Invalid resource URI"))?;
    let local_root = Url::from_file_path(root)
        .map_err(|_| anyhow::anyhow!("Invalid resource root"))?
        .to_file_path()
        .map_err(|_| anyhow::anyhow!("Invalid resource root"))?;
    #[cfg(windows)]
    let relative = {
        let roots: Vec<_> = local_root.components().collect();
        let paths: Vec<_> = path.components().collect();
        ensure!(
            paths.len() > roots.len()
                && roots.iter().zip(&paths).all(|(a, b)| a
                    .as_os_str()
                    .to_str()
                    .zip(b.as_os_str().to_str())
                    .is_some_and(|(a, b)| a.eq_ignore_ascii_case(b))),
            "Resource URI escaped copied workspace"
        );
        paths[roots.len()..].iter().collect::<std::path::PathBuf>()
    };
    #[cfg(not(windows))]
    let relative = path
        .strip_prefix(&local_root)
        .context("Resource URI escaped copied workspace")?
        .to_path_buf();
    let relative = relative
        .to_str()
        .context("Non-UTF-8 resource path")?
        .replace('\\', "/");
    scoped(root, &relative)?;
    Ok(relative)
}
fn virtual_file(
    store: &crate::store::Store,
    snapshot: &Snapshot,
    files: &mut BTreeMap<String, Option<String>>,
    path: &str,
) -> Result<()> {
    if !files.contains_key(path) {
        files.insert(
            path.into(),
            if snapshot.files.iter().any(|file| file.path == path) {
                Some(query::source(store, snapshot, path)?.0)
            } else {
                None
            },
        );
    }
    Ok(())
}
#[derive(Clone, Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CheckedTextEdit {
    pub start: usize,
    pub end: usize,
    pub text: String,
}
#[derive(Clone, Debug, serde::Serialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub(crate) enum CheckedEditOperation {
    TextDocument {
        path: String,
        source_hash: String,
        original_source: bool,
        version: Option<i64>,
        edits: Vec<CheckedTextEdit>,
    },
    Create {
        path: String,
        applied: bool,
    },
    Rename {
        old: String,
        new: String,
        applied: bool,
    },
    Delete {
        path: String,
        applied: bool,
    },
}
/// A checked, unpublished edit batch. Source coordinates describe each exact
/// virtual document input, before edits are flattened into final file contents.
pub(crate) struct CheckedWorkspaceEdit {
    snapshot_id: String,
    workspace_id: String,
    manifest_hash: String,
    pub operations: Vec<CheckedEditOperation>,
    pub files: BTreeMap<String, Option<String>>,
    directory_move: Option<changes::SelectedDirectoryMove>,
}
impl CheckedWorkspaceEdit {
    fn retained_bytes(&self) -> usize {
        retained_edit_bytes(&self.files, &self.operations)
    }
    pub(crate) fn prepare(
        &self,
        engine: &mut Engine,
        snapshot: &Snapshot,
        graph: &Graph,
        deadline: &Deadline,
    ) -> Result<changes::Plan> {
        deadline.check()?;
        ensure!(
            self.snapshot_id == snapshot.id
                && self.workspace_id == snapshot.workspace_id
                && self.workspace_id == engine.store.workspace_id
                && self.manifest_hash == identity(snapshot),
            "Checked WorkspaceEdit source binding changed"
        );
        let edits = self.edits(engine, snapshot, deadline)?;
        if let Some(directory) = &self.directory_move {
            return changes::prepare_directory(
                &mut engine.store,
                snapshot,
                graph,
                &json!({"edits":edits}),
                directory,
                &engine.root,
                deadline,
            );
        }
        changes::prepare(
            &mut engine.store,
            snapshot,
            graph,
            &json!({"edits":edits}),
            deadline,
        )
    }
    fn edits(
        &self,
        engine: &Engine,
        snapshot: &Snapshot,
        deadline: &Deadline,
    ) -> Result<Vec<Value>> {
        let scope = crate::capture::source_scope_policy(&engine.store, snapshot)?;
        let mut edits = Vec::new();
        let mut diff_bytes = 0usize;
        for (path, after) in &self.files {
            deadline.check()?;
            scope.check_host_write_scope(path)?;
            let before = snapshot.files.iter().find(|file| file.path == *path);
            ensure!(
                after
                    .as_ref()
                    .is_none_or(|text| text.len() <= 4_000_000 && !text.contains('\0')),
                "Change file budget or NUL violated"
            );
            ensure!(
                after
                    .as_ref()
                    .is_none_or(|text| !crate::paths::obvious_secret(text)),
                "Edited source contains an obvious credential"
            );
            let old = if before.is_some() {
                query::source(&engine.store, snapshot, path)?.0
            } else {
                String::new()
            };
            let changed = match (before, after) {
                (None, None) => false,
                (Some(_), Some(text)) => old != *text,
                _ => true,
            };
            if changed {
                let diff =
                    similar::TextDiff::from_lines(old.as_str(), after.as_deref().unwrap_or(""))
                        .unified_diff()
                        .context_radius(3)
                        .header(
                            if before.is_some() { path } else { "/dev/null" },
                            if after.is_some() { path } else { "/dev/null" },
                        )
                        .to_string();
                diff_bytes = diff_bytes.saturating_add(diff.len());
                ensure!(diff_bytes <= 16_000_000, "Patch diff budget exceeded");
            }
            match (before, after) {
                (Some(file), None) => {
                    edits.push(json!({"kind":"delete","path":path,"expectedHash":file.hash}))
                }
                (None, Some(text)) => edits.push(json!({"kind":"create","path":path,"text":text})),
                (Some(file), Some(text)) => {
                    if old != *text {
                        edits.push(json!({"kind":"replace","path":path,"expectedHash":file.hash,"startByte":0,"endByte":old.len(),"text":text}));
                    }
                }
                (None, None) => {}
            }
        }
        ensure!(
            !edits.is_empty() && edits.len() <= 2000,
            "Expected 1..2000 effective WorkspaceEdit changes"
        );
        Ok(edits)
    }
}
fn retained_edit_bytes(
    files: &BTreeMap<String, Option<String>>,
    operations: &[CheckedEditOperation],
) -> usize {
    let file_bytes = files.iter().fold(0usize, |bytes, (path, text)| {
        bytes
            .saturating_add(path.len())
            .saturating_add(text.as_ref().map_or(0, String::len))
    });
    operations.iter().fold(file_bytes, |bytes, operation| {
        let payload = match operation {
            CheckedEditOperation::TextDocument { path, edits, .. } => {
                edits.iter().fold(path.len(), |bytes, edit| {
                    bytes.saturating_add(edit.text.len()).saturating_add(32)
                })
            }
            CheckedEditOperation::Create { path, .. }
            | CheckedEditOperation::Delete { path, .. } => path.len(),
            CheckedEditOperation::Rename { old, new, .. } => old.len().saturating_add(new.len()),
        };
        bytes.saturating_add(payload).saturating_add(256)
    })
}
pub(crate) struct StagedSemantic {
    pub value: Value,
    pub edits: Vec<CheckedWorkspaceEdit>,
}
fn virtual_text_edits(
    store: &crate::store::Store,
    snapshot: &Snapshot,
    files: &mut BTreeMap<String, Option<String>>,
    path: &str,
    edits: &Value,
    version: Option<&Value>,
    deadline: &Deadline,
) -> Result<CheckedEditOperation> {
    ensure!(
        version.is_none_or(|version| version.is_null() || version == 1),
        "Stale resource document version"
    );
    virtual_file(store, snapshot, files, path)?;
    let source = files[path]
        .as_ref()
        .context("Text edits target a deleted or absent file")?;
    let source_hash = crate::model::hash(source.as_bytes());
    let original_source = snapshot
        .files
        .iter()
        .any(|file| file.path == path && file.hash == source_hash);
    let edits = edits.as_array().context("Invalid resource text edits")?;
    ensure!(edits.len() <= 2000, "Resource text edit count exceeded");
    let mut replacements = Vec::new();
    for edit in edits {
        deadline.check()?;
        ensure!(
            edit.get("annotationId").is_none(),
            "Annotated text edit refused"
        );
        let (start, end) = range(source, &edit["range"])?;
        let text = edit["newText"].as_str().context("Edit text missing")?;
        replacements.push((start, end, text));
    }
    replacements.sort_by_key(|&(start, end, _)| (start, end));
    for pair in replacements.windows(2) {
        ensure!(
            pair[0].1 <= pair[1].0 && pair[0].0 != pair[1].0,
            "Overlapping or ambiguous text edits"
        );
    }
    let normalized = replacements
        .iter()
        .map(|&(start, end, text)| CheckedTextEdit {
            start,
            end,
            text: text.into(),
        })
        .collect();
    let mut next = source.clone();
    for (start, end, text) in replacements.into_iter().rev() {
        next.replace_range(start..end, text);
        ensure!(
            next.len() <= 4_000_000,
            "Resource output file exceeds budget"
        );
    }
    files.insert(path.into(), Some(next));
    Ok(CheckedEditOperation::TextDocument {
        path: path.into(),
        source_hash,
        original_source,
        version: version.and_then(Value::as_i64),
        edits: normalized,
    })
}
fn check_workspace_edit(
    engine: &Engine,
    snapshot: &Snapshot,
    _graph: &Graph,
    root: &Path,
    edit: &Value,
    deadline: &Deadline,
) -> Result<CheckedWorkspaceEdit> {
    check_workspace_edit_directory(engine, snapshot, _graph, root, edit, None, deadline)
}
#[allow(clippy::too_many_arguments)]
fn check_workspace_edit_directory(
    engine: &Engine,
    snapshot: &Snapshot,
    _graph: &Graph,
    root: &Path,
    edit: &Value,
    directory: Option<&changes::SelectedDirectoryMove>,
    deadline: &Deadline,
) -> Result<CheckedWorkspaceEdit> {
    ensure!(
        edit.is_object() && edit.get("changes").is_some() != edit.get("documentChanges").is_some(),
        "Ambiguous or missing WorkspaceEdit"
    );
    ensure!(
        edit.get("changeAnnotations").is_none(),
        "Annotated edits require host review"
    );
    let mut files = BTreeMap::<String, Option<String>>::new();
    let mut operations = Vec::new();
    if let Some(changes) = edit.get("changes") {
        let changes = changes.as_object().context("Invalid changes map")?;
        ensure!(changes.len() <= 2000, "WorkspaceEdit file budget exceeded");
        for (uri, values) in changes {
            operations.push(virtual_text_edits(
                &engine.store,
                snapshot,
                &mut files,
                &edit_relative(root, uri)?,
                values,
                None,
                deadline,
            )?);
            ensure!(
                retained_edit_bytes(&files, &operations) <= 32_000_000,
                "Virtual WorkspaceEdit capacity exceeded"
            );
        }
    }
    if let Some(documents) = edit.get("documentChanges") {
        let documents = documents.as_array().context("Invalid document changes")?;
        ensure!(
            documents.len() <= 2000,
            "WorkspaceEdit operation budget exceeded"
        );
        for document in documents {
            deadline.check()?;
            if let Some(kind) = document.get("kind") {
                ensure!(
                    document.get("annotationId").is_none(),
                    "Annotated resource operation refused"
                );
                match kind.as_str().context("Invalid resource operation")? {
                    "create" => {
                        let path = edit_relative(
                            root,
                            document["uri"].as_str().context("Create URI missing")?,
                        )?;
                        virtual_file(&engine.store, snapshot, &mut files, &path)?;
                        let exists = files[&path].is_some();
                        if exists && document["options"]["ignoreIfExists"] == true {
                            operations.push(CheckedEditOperation::Create {
                                path,
                                applied: false,
                            });
                            continue;
                        }
                        ensure!(
                            !exists || document["options"]["overwrite"] == true,
                            "Create resource already exists"
                        );
                        operations.push(CheckedEditOperation::Create {
                            path: path.clone(),
                            applied: true,
                        });
                        files.insert(path, Some(String::new()));
                    }
                    "rename" => {
                        let old = edit_relative(
                            root,
                            document["oldUri"]
                                .as_str()
                                .context("Rename source URI missing")?,
                        )?;
                        let new = edit_relative(
                            root,
                            document["newUri"]
                                .as_str()
                                .context("Rename target URI missing")?,
                        )?;
                        if let Some(directory) = directory {
                            let selected = directory.record();
                            ensure!(
                                old == selected.source()
                                    && new == selected.destination()
                                    && document.get("options").is_none()
                                    && operations.iter().all(|op| matches!(
                                        op,
                                        CheckedEditOperation::TextDocument { .. }
                                    )),
                                "Unsupported directory rename resource sequence"
                            );
                            for source in selected.files() {
                                let destination = selected.destination_path(&source.path)?;
                                virtual_file(&engine.store, snapshot, &mut files, &source.path)?;
                                virtual_file(&engine.store, snapshot, &mut files, &destination)?;
                                ensure!(
                                    files[&destination].is_none(),
                                    "Directory resource destination exists"
                                );
                                let text = files
                                    .get_mut(&source.path)
                                    .unwrap()
                                    .take()
                                    .context("Directory resource source missing")?;
                                files.insert(destination, Some(text));
                            }
                            operations.push(CheckedEditOperation::Rename {
                                old,
                                new,
                                applied: true,
                            });
                            continue;
                        }
                        ensure!(
                            old != new && !old.eq_ignore_ascii_case(&new),
                            "Ambiguous resource rename"
                        );
                        virtual_file(&engine.store, snapshot, &mut files, &old)?;
                        virtual_file(&engine.store, snapshot, &mut files, &new)?;
                        if files[&new].is_some() && document["options"]["ignoreIfExists"] == true {
                            operations.push(CheckedEditOperation::Rename {
                                old,
                                new,
                                applied: false,
                            });
                            continue;
                        }
                        ensure!(
                            files[&new].is_none() || document["options"]["overwrite"] == true,
                            "Rename resource target exists"
                        );
                        let source = files
                            .get_mut(&old)
                            .unwrap()
                            .take()
                            .context("Rename resource source absent")?;
                        operations.push(CheckedEditOperation::Rename {
                            old,
                            new: new.clone(),
                            applied: true,
                        });
                        files.insert(new, Some(source));
                    }
                    "delete" => {
                        let path = edit_relative(
                            root,
                            document["uri"].as_str().context("Delete URI missing")?,
                        )?;
                        virtual_file(&engine.store, snapshot, &mut files, &path)?;
                        ensure!(
                            files[&path].is_some()
                                || document["options"]["ignoreIfNotExists"] == true,
                            "Delete resource absent"
                        );
                        let applied = files[&path].is_some();
                        operations.push(CheckedEditOperation::Delete {
                            path: path.clone(),
                            applied,
                        });
                        files.insert(path, None);
                    }
                    _ => bail!("Unsupported resource operation"),
                }
            } else {
                let path = edit_relative(
                    root,
                    document["textDocument"]["uri"]
                        .as_str()
                        .context("Text document URI missing")?,
                )?;
                operations.push(virtual_text_edits(
                    &engine.store,
                    snapshot,
                    &mut files,
                    &path,
                    &document["edits"],
                    document["textDocument"].get("version"),
                    deadline,
                )?);
            }
            ensure!(
                files.len() <= 2000
                    && files.values().flatten().map(String::len).sum::<usize>() <= 32_000_000,
                "Virtual WorkspaceEdit capacity exceeded"
            );
        }
    }
    let checked = CheckedWorkspaceEdit {
        snapshot_id: snapshot.id.clone(),
        workspace_id: snapshot.workspace_id.clone(),
        manifest_hash: identity(snapshot),
        operations,
        files,
        directory_move: directory.cloned(),
    };
    ensure!(
        checked.files.len() <= 2000 && checked.retained_bytes() <= 32_000_000,
        "Virtual WorkspaceEdit capacity exceeded"
    );
    // Include source/write-scope admission in staging, before any sibling action
    // can publish a plan. This has no graph, plan, approval or receipt writes.
    checked.edits(engine, snapshot, deadline)?;
    Ok(checked)
}
#[cfg(test)]
fn check_copy(snapshot: &Snapshot, root: &Path, deadline: &Deadline) -> Result<()> {
    check_copy_scoped(snapshot, root, None, deadline)
}
fn check_copy_verified(
    store: &crate::store::Store,
    snapshot: &Snapshot,
    root: &Path,
    excluded: &std::collections::BTreeSet<String>,
    deadline: &Deadline,
) -> Result<()> {
    let scope = crate::capture::source_scope_policy(store, snapshot)?;
    for path in excluded {
        scope.file(path)?;
        ensure!(
            matches!(std::fs::symlink_metadata(root.join(path)),Err(error) if error.kind() == std::io::ErrorKind::NotFound),
            "Provider recreated an excluded source selector"
        );
    }
    for file in &snapshot.files {
        deadline.check()?;
        if !excluded.contains(&file.path) {
            let mut expected = file.clone();
            if scope.source_encoding().is_some() {
                let view = scope.source_view(&file.path)?;
                expected.hash = crate::model::hash(view.text().as_bytes());
                expected.size = view.text().len();
            }
            ensure!(
                crate::verify::file_integrity(
                    &expected,
                    &read_source_path(root, &file.path, Some(&scope))?,
                    deadline
                )?,
                "Language server modified captured source"
            );
        }
    }
    Ok(())
}
#[cfg(test)]
fn check_copy_scoped(
    snapshot: &Snapshot,
    root: &Path,
    scope: Option<&crate::capture::CapturedSourceScope<'_>>,
    deadline: &Deadline,
) -> Result<()> {
    for file in &snapshot.files {
        deadline.check()?;
        ensure!(
            crate::verify::file_integrity(
                file,
                &read_source_path(root, &file.path, scope)?,
                deadline
            )?,
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
    run_at_with_options(
        engine,
        server,
        snapshot,
        graph,
        request,
        &json!({}),
        deadline,
    )
}
pub fn run_at_with_options(
    engine: &mut Engine,
    server: &Server,
    snapshot: &Snapshot,
    graph: &Graph,
    request: SemanticRequest<'_>,
    options: &Value,
    deadline: &Deadline,
) -> Result<Value> {
    run_at_internal(
        engine,
        (server, None),
        snapshot,
        graph,
        request,
        options,
        None,
        None,
        deadline,
    )
}
pub fn run_at_configured(
    engine: &mut Engine,
    server: &ConfiguredServer,
    snapshot: &Snapshot,
    graph: &Graph,
    request: SemanticRequest<'_>,
    options: &Value,
    deadline: &Deadline,
) -> Result<Value> {
    run_at_internal(
        engine,
        (&server.server, Some(&server.configuration)),
        snapshot,
        graph,
        request,
        options,
        None,
        None,
        deadline,
    )
}
/// Crate-owned refactoring consumers validate their intent against these
/// unpublished batches before creating any guarded plan.
pub(crate) fn stage_at(
    engine: &mut Engine,
    host: (&Server, Option<&HostConfiguration>),
    snapshot: &Snapshot,
    graph: &Graph,
    request: SemanticRequest<'_>,
    options: &Value,
    deadline: &Deadline,
) -> Result<StagedSemantic> {
    let mut edits = Vec::new();
    let value = run_at_internal(
        engine,
        host,
        snapshot,
        graph,
        request,
        options,
        Some(&mut edits),
        None,
        deadline,
    )?;
    Ok(StagedSemantic { value, edits })
}
pub(crate) fn stage_directory_move_at(
    engine: &mut Engine,
    host: (&Server, Option<&HostConfiguration>),
    snapshot: &Snapshot,
    graph: &Graph,
    selected: &changes::SelectedDirectoryMove,
    deadline: &Deadline,
) -> Result<StagedSemantic> {
    let file = selected
        .record()
        .files()
        .iter()
        .find(|file| engine.selected_backends.is_some() || file.language == host.0.language)
        .context("Directory move provider has no selected source file")?;
    let mut edits = Vec::new();
    let value = run_at_internal(
        engine,
        host,
        snapshot,
        graph,
        SemanticRequest {
            path: &file.path,
            offset: 0,
            operation: "willRenameFiles",
            new_name: None,
        },
        &json!({"renamePath":selected.record().destination()}),
        Some(&mut edits),
        Some(selected),
        deadline,
    )?;
    Ok(StagedSemantic { value, edits })
}
fn check_semantic_manifests(
    engine: &Engine,
    snapshot: &Snapshot,
    graph: &Graph,
    deadline: &Deadline,
) -> Result<()> {
    engine.store.set_deadline(deadline)?;
    deadline.check()?;
    ensure!(
        snapshot.workspace_id == engine.store.workspace_id
            && identity(&engine.store.snapshot(&snapshot.id)?) == identity(snapshot),
        "Semantic source manifest mismatch"
    );
    ensure!(
        graph.snapshot_id == snapshot.id
            && identity(&engine.store.graph(&snapshot.id)?) == identity(graph),
        "Semantic graph manifest mismatch"
    );
    deadline.check()
}
#[allow(clippy::too_many_arguments)]
fn run_at_internal(
    engine: &mut Engine,
    host: (&Server, Option<&HostConfiguration>),
    snapshot: &Snapshot,
    graph: &Graph,
    request: SemanticRequest<'_>,
    options: &Value,
    staged: Option<&mut Vec<CheckedWorkspaceEdit>>,
    directory: Option<&changes::SelectedDirectoryMove>,
    deadline: &Deadline,
) -> Result<Value> {
    check_semantic_manifests(engine, snapshot, graph, deadline)?;
    let selected = engine.selected_provider(request.path)?;
    ensure!(
        !engine.automatic_backend_selection || selected.is_some(),
        "language_auto_no_selected_provider_for_path"
    );
    let host = selected
        .as_ref()
        .map(|(_, p)| (&p.server, Some(&p.configuration)))
        .unwrap_or(host);
    ensure!(host.0.trust, "Language server trust is required");
    let configuration = effective_host_configuration(engine, snapshot, host.1, deadline)?;
    let host = (host.0, configuration.as_ref());
    validate_passive_host(host.0, host.1, deadline)?;
    let profile = server_profile(host.0, host.1, deadline)?;
    let profile_hash =
        selected_profile_hash(&profile, selected.as_ref().map(|(id, _)| id.as_str()));
    let encoding = crate::source_encoding::selected(&engine.store, snapshot)?;
    let profile_hash = if let Some(policy) = encoding {
        identity(&(
            profile_hash,
            crate::source_encoding::VERSION,
            policy,
            "private_utf8_v1",
        ))
    } else {
        profile_hash
    };
    let path = request.path;
    let mut value = run_at_unprofiled(
        engine, host, snapshot, graph, request, options, staged, directory, deadline,
    )?;
    value["serverProfileHash"] = json!(profile_hash);
    if let Some(policy) = encoding {
        value["sourceEncodingPolicy"] = json!(policy);
        value["privateSourceView"] = json!({"encoding":"utf8","offsetsReturned":"original_source_bytes","hashesReturned":"original_source_bytes","copyIntegrity":"decoded_utf8_content_checked","fullBackendCertification":false});
    }
    if let Some(extra) = engine
        .additional_workspace_folders
        .as_ref()
        .filter(|v| !v.is_empty())
    {
        value["additionalWorkspaceFolders"] = json!(extra);
        value["additionalWorkspaceScope"] = json!({"scope":"same_captured_source","symbolIndexPolicyUnchanged":true,"sourceCaptureWidened":false});
    }
    value["serverProfileBinding"] = profile["commandBinding"].clone();
    let omissions = passive_source_exclusions(host.0, host.1, snapshot);
    value["providerSourceOmissions"] = json!(snapshot.files.iter().filter(|file|omissions.contains(&file.path)).map(|file|json!({"path":file.path,"contentHash":file.hash,"reason":"workspace_execution_selector_omitted_from_private_provider_tree"})).collect::<Vec<_>>());
    value["sourceExecutionAuthorized"] = json!(
        host.1
            .is_some_and(|configuration| configuration.allow_source_execution)
    );
    value["providerSourcePolicy"] = json!(provider_source_policy(host.0, host.1, snapshot));
    if let Some((id, _)) = &selected {
        qualify_selection(&mut value, id, path);
    }
    Ok(value)
}
fn selected_profile_hash(profile: &Value, id: Option<&str>) -> String {
    id.map_or_else(
        || identity(profile),
        |id| identity(&(profile, "host_catalog_backend_v1", id)),
    )
}
fn qualify_selection(value: &mut Value, id: &str, path: &str) {
    let matched =
        crate::server_catalog::get(id).is_some_and(|entry| entry.supports_path(Path::new(path)));
    value["selectedBackendId"] = json!(id);
    value["backendSelection"] = json!({"policy":"first_supporting_file_then_first_fallback","matchedDeclaredExtension":matched,"fallback":!matched,"runtimeCertification":false});
}
/// All transport paths, including staged refactors, bind the same effective roots
/// before passive-policy checks, provider-profile hashing or process startup.
pub(crate) fn effective_host_configuration(
    engine: &Engine,
    snapshot: &Snapshot,
    configuration: Option<&HostConfiguration>,
    deadline: &Deadline,
) -> Result<Option<HostConfiguration>> {
    let mut configuration = configuration.cloned();
    if let Some(roots) = &engine.symbol_workspace_folders {
        deadline.check()?;
        engine.store.set_deadline(deadline)?;
        let scope = crate::capture::source_scope_policy(&engine.store, snapshot)?;
        crate::symbol_workspace::validate_snapshot(roots, snapshot, &scope, deadline)?;
        configuration.get_or_insert_default().workspace_folders = roots.clone();
    }
    crate::semantic_workspace::extend(engine, snapshot, &mut configuration, deadline)?;
    Ok(configuration)
}
fn validate_passive_host(
    server: &Server,
    configuration: Option<&HostConfiguration>,
    deadline: &Deadline,
) -> Result<()> {
    let configuration = configuration.context("Passive provider unavailable: explicit measured host passiveProfile or allowSourceExecution required")?;
    ensure!(
        configuration.workspace_folders.len() <= 32
            && configuration.language_ids.len() <= 100
            && configuration.open_languages.len() <= 100,
        "Host provider configuration capacity exceeded"
    );
    for value in [
        &configuration.initialization_options,
        &configuration.settings,
    ] {
        bounded_host_json(value, 64000, deadline)?;
    }
    let mut resources = resource_settings::effective(configuration)?;
    for value in resources.iter().flat_map(|values| values.values()) {
        bounded_host_json(value, 64000, deadline)?;
    }
    if configuration.allow_source_execution {
        return Ok(());
    }
    let profile = configuration.passive_profile.context(
        "Passive provider unavailable: backend trust does not certify source execution behavior",
    )?;
    ensure!(
        match profile {
            PassiveProfile::RustAnalyzer => server.language == "rust",
            PassiveProfile::TypescriptLanguageServer => matches!(
                server.language.as_str(),
                "typescript" | "javascript" | "tsx" | "jsx"
            ),
            PassiveProfile::JediLanguageServer => server.language == "python",
            PassiveProfile::ContractFixture => true,
        },
        "Selected passive profile does not cover this language"
    );
    for folder in &configuration.workspace_folders {
        ensure!(
            folder == "." || crate::paths::normalized(folder)? == *folder,
            "Passive provider workspace folder must stay in the captured private tree"
        );
    }
    for root in [
        &configuration.initialization_options,
        &configuration.settings,
    ]
    .into_iter()
    .chain(resources.iter().flat_map(|values| values.values()))
    {
        if profile == PassiveProfile::TypescriptLanguageServer {
            ensure!(
                root.get("plugins")
                    .is_none_or(|plugins| plugins.is_null()
                        || plugins.as_array().is_some_and(Vec::is_empty))
                    && root["disableAutomaticTypingAcquisition"] != false
                    && root["tsserver"]["path"].is_null()
                    && !server
                        .args
                        .iter()
                        .any(|arg| arg.contains("allowLocalPluginLoads")),
                "TypeScript plugin/tool overrides require explicit host allowSourceExecution"
            );
        }
        if profile == PassiveProfile::JediLanguageServer {
            ensure!(
                root["jediSettings"]["autoImportModules"].is_null()
                    || root["jediSettings"]["autoImportModules"]
                        .as_array()
                        .is_some_and(Vec::is_empty),
                "Jedi additional auto-imports require explicit host allowSourceExecution"
            );
            ensure!(
                root["workspace"]["environmentPath"].is_null()
                    && (root["workspace"]["extraPaths"].is_null()
                        || root["workspace"]["extraPaths"]
                            .as_array()
                            .is_some_and(Vec::is_empty)),
                "Jedi environment/import redirects require explicit host allowSourceExecution"
            );
        }
        if !passive_rust(server, Some(configuration)) {
            continue;
        }
        for value in std::iter::once(root).chain(root.get("rust-analyzer")) {
            ensure!(
                value
                    .get("linkedProjects")
                    .is_none_or(|projects| projects.is_null()
                        || projects.as_array().is_some_and(Vec::is_empty))
                    && value["workspace"]["discoverConfig"]["command"].is_null(),
                "Passive Rust linked/discovered project redirects require source-bound private mappings"
            );
            ensure!(
                value["cargo"]["buildScripts"]["enable"] != true
                    && value["procMacro"]["enable"] != true
                    && value["checkOnSave"] != true,
                "Rust execution settings require explicit host allowSourceExecution"
            );
            for selector in [
                &value["cargo"]["extraArgs"],
                &value["cargo"]["buildScripts"]["overrideCommand"],
                &value["check"]["overrideCommand"],
                &value["check"]["extraArgs"],
            ] {
                ensure!(
                    selector.is_null() || selector.as_array().is_some_and(Vec::is_empty),
                    "Rust tool arguments/commands require explicit host allowSourceExecution"
                );
            }
            for key in [
                "PATH",
                "RUSTUP_HOME",
                "RUSTFLAGS",
                "CARGO_ENCODED_RUSTFLAGS",
                "RUSTC_BOOTSTRAP",
                "CARGO_TARGET_DIR",
                "LD_PRELOAD",
                "LD_LIBRARY_PATH",
                "DYLD_INSERT_LIBRARIES",
                "DYLD_LIBRARY_PATH",
                "CARGO_HOME",
                "RUSTC",
                "RUSTC_WRAPPER",
                "RUSTC_WORKSPACE_WRAPPER",
                "CARGO_BUILD_RUSTC",
                "CARGO_BUILD_RUSTC_WRAPPER",
                "CARGO_BUILD_RUSTC_WORKSPACE_WRAPPER",
                "RUSTUP_TOOLCHAIN",
            ] {
                ensure!(
                    value["cargo"]["extraEnv"]
                        .as_object()
                        .is_none_or(|environment| environment
                            .keys()
                            .all(|name| !name.eq_ignore_ascii_case(key))),
                    "Rust compiler override requires explicit host allowSourceExecution"
                );
            }
        }
    }
    if passive_rust(server, Some(configuration)) {
        for value in resources.iter_mut().flat_map(|values| values.values_mut()) {
            resource_settings::enforce_passive_rust(value)?;
        }
    }
    Ok(())
}
fn bounded_host_json(value: &Value, limit: usize, deadline: &Deadline) -> Result<()> {
    struct Counter<'a> {
        bytes: usize,
        limit: usize,
        deadline: &'a Deadline,
    }
    impl Write for Counter<'_> {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            if self.deadline.check().is_err() || self.bytes.saturating_add(bytes.len()) > self.limit
            {
                return Err(std::io::Error::other("Host JSON budget exceeded"));
            }
            self.bytes += bytes.len();
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut pending = vec![(value, 0)];
    let mut nodes = 0usize;
    while let Some((value, depth)) = pending.pop() {
        deadline.check()?;
        nodes += 1;
        ensure!(
            depth <= 64 && nodes <= 64000,
            "Host JSON shape budget exceeded"
        );
        match value {
            Value::Array(values) => {
                ensure!(
                    pending.len().saturating_add(values.len()) <= 64000,
                    "Host JSON queue budget exceeded"
                );
                pending.extend(values.iter().map(|value| (value, depth + 1)));
            }
            Value::Object(values) => {
                ensure!(
                    pending.len().saturating_add(values.len()) <= 64000,
                    "Host JSON queue budget exceeded"
                );
                pending.extend(values.values().map(|value| (value, depth + 1)));
            }
            _ => {}
        }
    }
    serde_json::to_writer(
        Counter {
            bytes: 0,
            limit,
            deadline,
        },
        value,
    )
    .context("Host JSON byte budget exceeded")?;
    deadline.check()
}
fn validate_passive_manifest_sources(
    engine: &Engine,
    snapshot: &Snapshot,
    deadline: &Deadline,
) -> Result<()> {
    let scope = crate::capture::source_scope_policy(&engine.store, snapshot)?;
    let mut bytes = 0usize;
    for file in snapshot.files.iter().filter(|file| {
        file.path
            .rsplit('/')
            .next()
            .is_some_and(|name| name.eq_ignore_ascii_case("Cargo.toml"))
    }) {
        deadline.check()?;
        bytes = bytes
            .checked_add(file.size)
            .context("Cargo manifest source size overflow")?;
        ensure!(
            bytes <= 16_000_000,
            "Passive Cargo manifest source byte budget exceeded"
        );
        let view = scope.source_view(&file.path)?;
        let text = view.text();
        let manifest: toml::Value =
            toml::from_str(text).context("Passive Cargo manifest source could not be parsed")?;
        let mut pending = vec![&manifest];
        let mut nodes = 0usize;
        while let Some(value) = pending.pop() {
            nodes += 1;
            deadline.check()?;
            ensure!(
                nodes <= 100000,
                "Passive Cargo manifest node budget exceeded"
            );
            match value {
                toml::Value::Table(table) => {
                    for (key, value) in table {
                        if key == "metadata" {
                            continue;
                        }
                        if matches!(key.as_str(), "path" | "workspace" | "build")
                            && let Some(selector) = value.as_str()
                        {
                            private_manifest_selector(&file.path, selector, false, snapshot)?;
                        }
                        if matches!(key.as_str(), "members" | "default-members" | "exclude")
                            && let Some(selectors) = value.as_array()
                        {
                            ensure!(
                                selectors.len() <= 4000,
                                "Cargo manifest member capacity exceeded"
                            );
                            for selector in selectors {
                                deadline.check()?;
                                private_manifest_selector(
                                    &file.path,
                                    selector
                                        .as_str()
                                        .context("Cargo manifest member selector must be text")?,
                                    true,
                                    snapshot,
                                )?;
                            }
                        }
                        pending.push(value);
                    }
                }
                toml::Value::Array(values) => {
                    ensure!(
                        pending.len().saturating_add(values.len()) <= 100000,
                        "Passive Cargo manifest queue budget exceeded"
                    );
                    pending.extend(values);
                }
                _ => {}
            }
        }
    }
    deadline.check()
}
fn private_manifest_selector(
    manifest: &str,
    selector: &str,
    glob: bool,
    snapshot: &Snapshot,
) -> Result<()> {
    ensure!(
        !selector.is_empty()
            && selector.len() <= 4096
            && !selector.starts_with(['/', '\\'])
            && !selector.contains(':')
            && !selector.chars().any(char::is_control),
        "Passive Cargo manifest selector is not a private relative source path"
    );
    let selector = selector.replace('\\', "/");
    let mut parts: Vec<_> = manifest
        .rsplit_once('/')
        .map(|(parent, _)| parent.split('/').collect())
        .unwrap_or_default();
    for part in selector.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                ensure!(
                    parts.pop().is_some(),
                    "Passive Cargo manifest selector escapes captured private tree"
                );
            }
            _ => parts.push(part),
        }
    }
    let path = parts.join("/");
    if glob {
        crate::query_features::glob(&path, false)?;
    } else {
        if !path.is_empty() {
            ensure!(
                crate::paths::normalized_syntax(&path)? == path,
                "Passive Cargo selector path invalid"
            );
        }
        let manifest = if path.is_empty() {
            "Cargo.toml".into()
        } else {
            format!("{path}/Cargo.toml")
        };
        ensure!(
            snapshot
                .files
                .iter()
                .any(|file| file.path == path || file.path == manifest),
            "Passive Cargo selector has no exact captured source member"
        );
    }
    Ok(())
}
fn passive_rust(server: &Server, configuration: Option<&HostConfiguration>) -> bool {
    configuration.is_some_and(|configuration| {
        !configuration.allow_source_execution
            && (configuration.passive_profile == Some(PassiveProfile::RustAnalyzer)
                || (configuration.passive_profile == Some(PassiveProfile::ContractFixture)
                    && server.language == "rust"))
    })
}
fn provider_source_policy(
    server: &Server,
    configuration: Option<&HostConfiguration>,
    snapshot: &Snapshot,
) -> &'static str {
    let Some(configuration) =
        configuration.filter(|configuration| !configuration.allow_source_execution)
    else {
        return "host_selected_provider_unattested_transitive_behavior";
    };
    let has_json_project = snapshot.files.iter().any(|file| {
        file.path
            .rsplit('/')
            .next()
            .is_some_and(|name| name.eq_ignore_ascii_case("rust-project.json"))
    });
    match configuration.passive_profile {
        Some(PassiveProfile::RustAnalyzer) if has_json_project => {
            "passive_rust_json_project_selectors_excluded_v2"
        }
        Some(PassiveProfile::RustAnalyzer) => {
            "passive_rust_source_selectors_and_inherited_cargo_config_excluded"
        }
        Some(PassiveProfile::TypescriptLanguageServer) => {
            "passive_typescript_automatic_typing_acquisition_and_local_plugins_disabled"
        }
        Some(PassiveProfile::JediLanguageServer) => {
            "passive_jedi_project_import_paths_and_environment_overrides_disabled"
        }
        Some(PassiveProfile::ContractFixture) if server.language == "rust" && has_json_project => {
            "contract_fixture_rust_json_project_selectors_excluded_v2"
        }
        Some(PassiveProfile::ContractFixture) if server.language == "rust" => {
            "contract_fixture_rust_selector_broker_only"
        }
        Some(PassiveProfile::ContractFixture) => "contract_fixture_protocol_only",
        None => "host_selected_provider_unattested_transitive_behavior",
    }
}
fn passive_source_exclusions(
    server: &Server,
    configuration: Option<&HostConfiguration>,
    snapshot: &Snapshot,
) -> std::collections::BTreeSet<String> {
    if !passive_rust(server, configuration) {
        return Default::default();
    }
    snapshot
        .files
        .iter()
        .filter(|file| is_rust_source_selector(&file.path))
        .map(|file| file.path.clone())
        .collect()
}
pub(crate) fn is_rust_source_selector(path: &str) -> bool {
    is_legacy_rust_source_selector(path)
        || path
            .rsplit('/')
            .next()
            .is_some_and(|name| name.eq_ignore_ascii_case("rust-project.json"))
}
// Archived policy strings retain their original exact omission semantics.
pub(crate) fn is_legacy_rust_source_selector(path: &str) -> bool {
    let path = path.to_ascii_lowercase();
    matches!(
        path.rsplit('/').next(),
        Some(
            "rust-toolchain" | "rust-toolchain.toml" | "rust-analyzer.toml" | ".rust-analyzer.toml"
        )
    ) || path
        .split('/')
        .collect::<Vec<_>>()
        .windows(2)
        .any(|parts| parts[0] == ".cargo" && matches!(parts[1], "config" | "config.toml"))
}

/// Bind declared launch/configuration context and selected executable bytes.
/// This is not an attestation of transitive plugins, TCP peer code, or provider correctness.
fn server_profile(
    server: &Server,
    configuration: Option<&HostConfiguration>,
    deadline: &Deadline,
) -> Result<Value> {
    use sha2::{Digest, Sha256};
    deadline.check()?;
    ensure!(
        server.command.len() <= 4096
            && server.args.len() <= 100
            && server.args.iter().map(String::len).sum::<usize>() <= 128_000,
        "Server profile launch capacity exceeded"
    );
    let defaults = HostConfiguration::default();
    let configuration = configuration.unwrap_or(&defaults);
    let mut binding = json!({"kind":"existing_tcp_endpoint","executionAttestation":"unverified"});
    if configuration.tcp_endpoint.is_none() {
        let command = Path::new(&server.command);
        let selected = if command.is_absolute() {
            Some(command.to_path_buf())
        } else {
            let extensions = if cfg!(windows) {
                vec!["", ".exe", ".cmd", ".bat"]
            } else {
                vec![""]
            };
            std::env::var_os("PATH").and_then(|search| {
                std::env::split_paths(&search)
                    .flat_map(|dir| {
                        extensions.iter().map(move |extension| {
                            dir.join(format!("{}{extension}", server.command))
                        })
                    })
                    .find(|candidate| candidate.is_file())
            })
        };
        if let Some(selected) = selected {
            let selected = selected.canonicalize()?;
            let mut input = std::fs::File::open(&selected)?;
            let size = input.metadata()?.len();
            ensure!(
                size <= 512_000_000,
                "Selected provider command hash budget exceeded"
            );
            let mut digest = Sha256::new();
            let mut buffer = [0u8; 65_536];
            let mut bytes = 0u64;
            loop {
                deadline.check()?;
                let count = input.read(&mut buffer)?;
                if count == 0 {
                    break;
                }
                bytes = bytes.saturating_add(count as u64);
                ensure!(
                    bytes <= size,
                    "Selected provider command changed while hashing"
                );
                digest.update(&buffer[..count]);
            }
            ensure!(
                bytes == size && input.metadata()?.len() == size,
                "Selected provider command changed while hashing"
            );
            binding = json!({"kind":"selected_command_bytes","path":selected,"contentHash":format!("{:x}",digest.finalize()),"executionAttestation":"unverified","transitiveCode":"not_attested"});
        } else {
            binding =
                json!({"kind":"unresolved_command_selection","executionAttestation":"unverified"});
        }
    }
    Ok(
        json!({"schemaVersion":1,"command":server.command,"arguments":server.args,"language":server.language,"configuration":configuration,"commandBinding":binding,"nativeProviderPolicy":"typed_passive_provider_profiles_and_private_manifest_paths_v2"}),
    )
}

struct InitializedProvider {
    tree: tempfile::TempDir,
    session: Session,
    capabilities: Value,
    document_uri: String,
    exclusions: std::collections::BTreeSet<String>,
}
#[allow(clippy::too_many_arguments)]
fn initialize_provider_at(
    engine: &Engine,
    host: (&Server, Option<&HostConfiguration>),
    snapshot: &Snapshot,
    path: &str,
    text: &str,
    target_language: &str,
    deadline: &Deadline,
) -> Result<InitializedProvider> {
    let (server, configuration) = host;
    let tree = tempfile::Builder::new().prefix("yoke-lsp-").tempdir()?;
    let exclusions = passive_source_exclusions(server, configuration, snapshot);
    ensure!(
        exclusions.len() <= 1000 && !exclusions.contains(path),
        "Passive provider selector omission capacity or target invalid"
    );
    let passive_rust = passive_rust(server, configuration);
    let mut resources = configuration
        .map(resource_settings::effective)
        .transpose()?
        .flatten();
    if passive_rust {
        for value in resources.iter_mut().flat_map(|values| values.values_mut()) {
            resource_settings::enforce_passive_rust(value)?;
        }
    }
    let resource_binding = resources
        .map(|settings| {
            resource_settings::Binding::new(tree.path(), snapshot, &exclusions, settings, deadline)
        })
        .transpose()?;
    if passive_rust {
        validate_passive_manifest_sources(engine, snapshot, deadline)?;
    }
    changes::materialize_source_views(&engine.store, snapshot, tree.path(), &exclusions, deadline)?;
    let mut session = if let Some(endpoint) =
        configuration.and_then(|configuration| configuration.tcp_endpoint)
    {
        Session::connect(server, endpoint, deadline)?
    } else {
        Session::spawn_scoped(server, tree.path(), passive_rust, deadline)?
    };
    let root_uri = uri(tree.path())?;
    session.resource_settings = resource_binding;
    let document_uri = uri(&tree.path().join(path))?;
    let mut initialize = json!({"processId":std::process::id(),"rootUri":root_uri,"rootPath":tree.path().display().to_string(),"workspaceFolders":[{"uri":root_uri,"name":"immutable-snapshot"}],"capabilities":{"general":{"positionEncodings":["utf-16"]},"window":{"workDoneProgress":true},"experimental":{"serverStatusNotification":true},"workspace":{"configuration":true,"applyEdit":false,"workspaceEdit":{"documentChanges":true,"resourceOperations":["create","rename","delete"]}},"textDocument":{"rename":{"prepareSupport":true},"definition":{"linkSupport":true},"declaration":{"linkSupport":true},"typeDefinition":{"linkSupport":true},"implementation":{"linkSupport":true},"references":{},"documentSymbol":{"hierarchicalDocumentSymbolSupport":true},"callHierarchy":{},"typeHierarchy":{},"completion":{"completionItem":{"snippetSupport":false,"insertReplaceSupport":true}},"signatureHelp":{},"diagnostic":{},"publishDiagnostics":{"versionSupport":true},"codeAction":{"codeActionLiteralSupport":{"codeActionKind":{"valueSet":["quickfix","refactor","source"]}}}}},"initializationOptions":{"cargo":{"buildScripts":{"enable":false},"extraEnv":{"CARGO_NET_OFFLINE":"true"}},"procMacro":{"enable":false},"checkOnSave":false}});
    initialize["capabilities"]["workspace"]["fileOperations"] =
        json!({"dynamicRegistration":false,"willRename":true});
    initialize["capabilities"]["textDocument"]["codeAction"]["dataSupport"] = json!(true);
    initialize["capabilities"]["textDocument"]["codeAction"]["disabledSupport"] = json!(true);
    initialize["capabilities"]["textDocument"]["codeAction"]["resolveSupport"] =
        json!({"properties":["edit"]});
    if let Some(configuration) = configuration {
        ensure!(
            serde_json::to_vec(&configuration.initialization_options)?.len() <= 64000
                && serde_json::to_vec(&configuration.settings)?.len() <= 64000
                && configuration.workspace_folders.len() <= 32
                && configuration.language_ids.len() <= 100
                && configuration.open_languages.len() <= 100,
            "Host language-server configuration exceeds budget"
        );
        if !configuration.initialization_options.is_null() {
            initialize["initializationOptions"] = configuration.initialization_options.clone();
        }
        session.settings = configuration.settings.clone();
        if !configuration.workspace_folders.is_empty() {
            let mut folders = Vec::new();
            for folder in &configuration.workspace_folders {
                let folder = if folder == "." {
                    tree.path().to_path_buf()
                } else {
                    scoped(tree.path(), folder)?
                };
                ensure!(
                    folder.is_dir(),
                    "Configured workspace folder missing from captured tree"
                );
                folders.push(json!({"uri":uri(&folder)?,"name":"captured-project"}));
            }
            initialize["workspaceFolders"] = json!(folders);
            initialize["capabilities"]["workspace"]["workspaceFolders"] = json!(true);
            session.workspace_folders = Some(initialize["workspaceFolders"].clone());
        }
    }
    if configuration.is_some_and(|config| {
        !config.allow_source_execution
            && config.passive_profile == Some(PassiveProfile::TypescriptLanguageServer)
    }) {
        ensure!(
            initialize["initializationOptions"].is_null()
                || initialize["initializationOptions"].is_object(),
            "Invalid passive TypeScript initialization shape"
        );
        if initialize["initializationOptions"].is_null() {
            initialize["initializationOptions"] = json!({});
        }
        initialize["initializationOptions"]["disableAutomaticTypingAcquisition"] = json!(true);
    }
    if passive_rust {
        for value in [&initialize["initializationOptions"], &session.settings] {
            let value = value.get("rust-analyzer").unwrap_or(value);
            ensure!(
                value["cargo"]["buildScripts"]["enable"] != true
                    && value["procMacro"]["enable"] != true
                    && value["checkOnSave"] != true
                    && value["workspace"]["discoverConfig"]["command"].is_null(),
                "Rust source execution settings require explicit host allowSourceExecution"
            );
            for key in [
                "CARGO_HOME",
                "RUSTC",
                "RUSTC_WRAPPER",
                "RUSTC_WORKSPACE_WRAPPER",
                "CARGO_BUILD_RUSTC",
                "CARGO_BUILD_RUSTC_WRAPPER",
                "CARGO_BUILD_RUSTC_WORKSPACE_WRAPPER",
                "RUSTUP_TOOLCHAIN",
            ] {
                ensure!(
                    value["cargo"]["extraEnv"].get(key).is_none(),
                    "Rust compiler override requires explicit host allowSourceExecution"
                );
            }
        }
        for value in [&initialize["initializationOptions"], &session.settings] {
            let value = value.get("rust-analyzer").unwrap_or(value);
            ensure!(
                (value.is_null() || value.is_object())
                    && (value["cargo"].is_null() || value["cargo"].is_object())
                    && (value["cargo"]["buildScripts"].is_null()
                        || value["cargo"]["buildScripts"].is_object())
                    && (value["procMacro"].is_null() || value["procMacro"].is_object()),
                "Invalid passive Rust configuration shape"
            );
        }
        if initialize["initializationOptions"].is_null() {
            initialize["initializationOptions"] = json!({});
        }
        initialize["initializationOptions"]["cargo"]["buildScripts"]["enable"] = json!(false);
        initialize["initializationOptions"]["procMacro"]["enable"] = json!(false);
        initialize["initializationOptions"]["checkOnSave"] = json!(false);
        if session.settings.is_null() {
            session.settings = json!({});
        }
        for section in ["", "rust-analyzer"] {
            let value = if section.is_empty() {
                &mut session.settings
            } else {
                &mut session.settings[section]
            };
            if value.is_null() {
                *value = json!({});
            }
            ensure!(
                value.is_object()
                    && (value["cargo"].is_null() || value["cargo"].is_object())
                    && (value["cargo"]["buildScripts"].is_null()
                        || value["cargo"]["buildScripts"].is_object())
                    && (value["procMacro"].is_null() || value["procMacro"].is_object()),
                "Invalid passive Rust settings shape"
            );
            value["cargo"]["buildScripts"]["enable"] = json!(false);
            value["procMacro"]["enable"] = json!(false);
            value["checkOnSave"] = json!(false);
        }
    }
    let initialization = session.request("initialize", initialize, deadline)?;
    let capabilities = initialization["capabilities"].clone();
    ensure!(capabilities.is_object(), "Server capabilities missing");
    ensure!(
        capabilities["positionEncoding"]
            .as_str()
            .unwrap_or("utf-16")
            == "utf-16",
        "Server selected unsupported position encoding"
    );
    session.notify("initialized", json!({}))?;
    if !session.settings.is_null() {
        session.notify(
            "workspace/didChangeConfiguration",
            json!({"settings":session.settings}),
        )?;
    }
    for file in &snapshot.files {
        if file.path != path
            && !exclusions.contains(&file.path)
            && (provider_language(&file.language) == provider_language(&server.language)
                || configuration.is_some_and(|config| {
                    config
                        .open_languages
                        .iter()
                        .any(|language| language == &file.language)
                }))
        {
            deadline.check()?;
            let language = configuration
                .and_then(|config| config.language_ids.get(&file.language))
                .map(String::as_str)
                .unwrap_or_else(|| document_language(&file.language));
            let view = query::source_view(&engine.store, snapshot, &file.path)?;
            let source = view.text();
            session.notify("textDocument/didOpen", json!({"textDocument":{"uri":uri(&tree.path().join(&file.path))?,"languageId":language,"version":1,"text":source}}))?;
        }
    }
    let language = configuration
        .and_then(|config| config.language_ids.get(target_language))
        .map(String::as_str)
        .unwrap_or_else(|| document_language(target_language));
    session.notify(
        "textDocument/didOpen",
        json!({"textDocument":{"uri":document_uri,"languageId":language,"version":1,"text":text}}),
    )?;
    if Path::new(&server.command)
        .file_stem()
        .is_some_and(|name| name == "rust-analyzer")
    {
        session.await_ready(deadline)?;
    }
    Ok(InitializedProvider {
        tree,
        session,
        capabilities,
        document_uri,
        exclusions,
    })
}

#[allow(clippy::too_many_arguments)]
fn run_at_unprofiled(
    engine: &mut Engine,
    host: (&Server, Option<&HostConfiguration>),
    snapshot: &Snapshot,
    graph: &Graph,
    request: SemanticRequest<'_>,
    options: &Value,
    mut staged: Option<&mut Vec<CheckedWorkspaceEdit>>,
    directory: Option<&changes::SelectedDirectoryMove>,
    deadline: &Deadline,
) -> Result<Value> {
    let (server, configuration) = host;
    check_semantic_manifests(engine, snapshot, graph, deadline)?;
    let encoding = crate::source_encoding::selected(&engine.store, snapshot)?;
    ensure!(
        encoding.is_none()
            || !matches!(
                request.operation,
                "rename" | "codeActions" | "willRenameFiles"
            ),
        "source_encoding_capability_unavailable: semantic workspace-edit/resource proof requires legacy UTF-8; native raw-range source changes remain available"
    );
    let options = QueryOptions::parse(options)?;
    if let Some(directory) = directory {
        ensure!(
            request.operation == "willRenameFiles"
                && options.rename_path.as_deref() == Some(directory.record().destination()),
            "Directory move request binding mismatch"
        );
        directory.record().validate(snapshot, deadline)?;
        directory.record().check_before(&engine.root, deadline)?;
    }
    let SemanticRequest {
        path,
        offset,
        operation,
        new_name,
    } = request;
    ensure!(server.trust, "Language server trust is required");
    let source_view = query::source_view(&engine.store, snapshot, path)?;
    let text = source_view.text();
    let content_hash = &snapshot
        .files
        .iter()
        .find(|f| f.path == path)
        .context("Semantic source missing")?
        .hash;
    let target_language = &snapshot
        .files
        .iter()
        .find(|file| file.path == path)
        .context("Semantic source file absent from snapshot")?
        .language;
    let position = byte_to_position(text, source_view.to_decoded(offset)?)?;
    let InitializedProvider {
        tree,
        mut session,
        capabilities,
        document_uri,
        exclusions,
    } = initialize_provider_at(
        engine,
        host,
        snapshot,
        path,
        text,
        target_language,
        deadline,
    )?;
    if operation == "capabilities" {
        let declared = capabilities["codeActionProvider"].get("codeActionKinds");
        let kinds = declared
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        ensure!(
            declared.is_none_or(Value::is_array)
                && kinds.len() <= 100
                && kinds
                    .iter()
                    .all(|kind| kind.as_str().is_some_and(|kind| kind.len() <= 200)),
            "Invalid declared code-action kind registry"
        );
        session.shutdown(deadline)?;
        drop(session);
        check_copy_verified(&engine.store, snapshot, tree.path(), &exclusions, deadline)?;
        return Ok(
            json!({"snapshotId":snapshot.id,"path":path,"contentHash":content_hash,"codeActionKinds":kinds,"codeActionKindsDeclared":declared.is_some(),"diagnostics":{"pull":capabilities["diagnosticProvider"].is_object(),"push":"provider_publication_required"},"producer":server.command,"complete":false,"registrySemantics":"declared_standard_lsp_capabilities_not_global_ide_inventory"}),
        );
    }
    let (method, capability) = match operation {
        "definition" => ("textDocument/definition", "definitionProvider"),
        "declaration" => ("textDocument/declaration", "declarationProvider"),
        "references" => ("textDocument/references", "referencesProvider"),
        "implementation" => ("textDocument/implementation", "implementationProvider"),
        "typeDefinition" => ("textDocument/typeDefinition", "typeDefinitionProvider"),
        "hover" => ("textDocument/hover", "hoverProvider"),
        "rename" => ("textDocument/rename", "renameProvider"),
        "symbols" | "symbolInfo" => ("textDocument/documentSymbol", "documentSymbolProvider"),
        "incomingCalls" | "outgoingCalls" => {
            ("textDocument/prepareCallHierarchy", "callHierarchyProvider")
        }
        "supertypes" | "subtypes" => ("textDocument/prepareTypeHierarchy", "typeHierarchyProvider"),
        "completion" => ("textDocument/completion", "completionProvider"),
        "signatureHelp" => ("textDocument/signatureHelp", "signatureHelpProvider"),
        "prepareRename" => ("textDocument/prepareRename", "renameProvider"),
        "diagnostics" => ("textDocument/diagnostic", "diagnosticProvider"),
        "codeActions" => ("textDocument/codeAction", "codeActionProvider"),
        "willRenameFiles" => ("workspace/willRenameFiles", "workspace"),
        _ => bail!("Unsupported semantic operation"),
    };
    let push_diagnostics = operation == "diagnostics"
        && (capabilities["diagnosticProvider"].is_null()
            || capabilities["diagnosticProvider"] == false);
    ensure!(
        push_diagnostics
            || (!capabilities[capability].is_null() && capabilities[capability] != false),
        "Language server does not advertise {operation}"
    );
    if operation == "prepareRename" {
        ensure!(
            capabilities["renameProvider"]["prepareProvider"] == true,
            "Server does not advertise rename preparation"
        );
    }
    let mut params = json!({"textDocument":{"uri":document_uri},"position":position});
    ensure!(
        options.rename_path.is_none() || operation == "willRenameFiles",
        "renamePath is only valid for willRenameFiles"
    );
    let mut rename_resource = None;
    if operation == "willRenameFiles" {
        let operation_source = directory.map_or(path, |directory| directory.record().source());
        let destination = options
            .rename_path
            .as_deref()
            .context("Source-bound renamePath required")?;
        ensure!(
            !operation_source.eq_ignore_ascii_case(destination)
                && !snapshot
                    .files
                    .iter()
                    .any(|file| file.path.eq_ignore_ascii_case(destination)),
            "Rename destination is occupied or aliases its source"
        );
        let registration = &capabilities["workspace"]["fileOperations"]["willRename"];
        let filters = registration["filters"]
            .as_array()
            .filter(|filters| !filters.is_empty() && filters.len() <= 100)
            .context("Server does not advertise bounded willRenameFiles filters")?;
        let mut selected = false;
        for filter in filters {
            deadline.check()?;
            ensure!(
                filter
                    .get("scheme")
                    .is_none_or(|scheme| scheme.as_str() == Some("file")),
                "Unsupported file-operation URI scheme"
            );
            let pattern = &filter["pattern"];
            ensure!(
                pattern
                    .get("matches")
                    .is_none_or(|kind| matches!(kind.as_str(), Some("file" | "folder"))),
                "Invalid file-operation match type"
            );
            ensure!(
                pattern["options"]
                    .get("ignoreCase")
                    .is_none_or(Value::is_boolean),
                "Invalid file-operation case flag"
            );
            let glob = pattern["glob"]
                .as_str()
                .context("File-operation glob missing")?;
            let matcher =
                crate::query_features::glob(glob, pattern["options"]["ignoreCase"] == true)?;
            let applies = pattern.get("matches").is_none_or(|kind| {
                kind.as_str()
                    == Some(if directory.is_some() {
                        "folder"
                    } else {
                        "file"
                    })
            });
            selected |= applies && matcher.is_match(operation_source);
        }
        ensure!(
            selected,
            "Selected source file is outside server willRenameFiles registration filters"
        );
        let target_uri = uri(&tree.path().join(destination))?;
        let source_uri = if directory.is_some() {
            uri(&tree.path().join(operation_source))?
        } else {
            document_uri.clone()
        };
        params = json!({"files":[{"oldUri":source_uri,"newUri":target_uri}]});
        rename_resource = Some(json!({"kind":"rename","oldUri":source_uri,"newUri":target_uri}));
    }
    if operation == "references" {
        params["context"] =
            json!({"includeDeclaration":options.include_declaration.unwrap_or(true)});
    }
    if operation == "rename" {
        let name = new_name.ok_or_else(|| anyhow::anyhow!("Rename name required"))?;
        ensure!(
            !name.is_empty() && name.len() <= 1000 && !name.contains(['\0', '\n', '\r']),
            "Invalid new name"
        );
        params["newName"] = json!(name);
    }
    let mut action_diagnostic_context = json!({"source":"not_advertised_or_observed","count":0});
    if operation == "codeActions" {
        let start = options.start_byte.unwrap_or(offset);
        let end = options.end_byte.unwrap_or(start);
        utf8_range(text, start, end)?;
        params["range"] =
            json!({"start":byte_to_position(text,start)?,"end":byte_to_position(text,end)?});
        let report = if capabilities["diagnosticProvider"].is_object() {
            let report = session.request(
                "textDocument/diagnostic",
                json!({"textDocument":{"uri":document_uri}}),
                deadline,
            )?;
            ensure!(
                report["kind"] == "full",
                "Code action context requires an initial full diagnostic report"
            );
            action_diagnostic_context["source"] = json!("pull");
            Some(report["items"].clone())
        } else if configuration.is_some_and(|config| {
            config.passive_profile == Some(PassiveProfile::TypescriptLanguageServer)
        }) {
            // This installed provider family publishes the diagnostics required by
            // its quick-fix resolver. Other unmeasured families are not inferred.
            let scope = crate::capture::source_scope_policy(&engine.store, snapshot)?;
            let publication =
                session.await_diagnostics(tree.path(), snapshot, &scope, path, deadline)?;
            ensure!(
                publication
                    .get("version")
                    .is_none_or(|version| version == 1),
                "Stale code action diagnostic version"
            );
            action_diagnostic_context["source"] = json!("published");
            Some(publication["diagnostics"].clone())
        } else {
            None
        };
        let mut diagnostics = vec![];
        if let Some(report) = report {
            let items = report
                .as_array()
                .filter(|items| items.len() <= 2000)
                .context("Code action diagnostic context budget exceeded")?;
            for item in items {
                deadline.check()?;
                let (first, last) = range(text, &item["range"])?;
                ensure!(
                    item["message"]
                        .as_str()
                        .is_some_and(|message| message.len() <= 16000),
                    "Invalid code action diagnostic message"
                );
                ensure!(
                    item.get("severity").is_none_or(|severity| severity
                        .as_u64()
                        .is_some_and(|severity| (1..=4).contains(&severity))),
                    "Invalid code action diagnostic severity"
                );
                let overlaps = if start == end {
                    first <= start && start <= last
                } else {
                    first < end && start < last
                        || first == last
                            && first >= start
                            && (first < end || first == text.len() && end == text.len())
                };
                if overlaps {
                    diagnostics.push(item.clone());
                }
            }
        }
        action_diagnostic_context["count"] = json!(diagnostics.len());
        params["context"] = json!({"diagnostics":diagnostics,"triggerKind":1});
        if !options.only.is_empty() {
            params["context"]["only"] = json!(options.only);
        }
    }
    if operation == "completion" {
        params["context"] = json!({"triggerKind":1});
    }
    let mut response = if push_diagnostics {
        let scope = crate::capture::source_scope_policy(&engine.store, snapshot)?;
        let publication =
            session.await_diagnostics(tree.path(), snapshot, &scope, path, deadline)?;
        ensure!(
            publication
                .get("version")
                .is_none_or(|version| version == 1),
            "Stale published diagnostic version"
        );
        json!({"kind":"full","items":publication["diagnostics"],"version":publication["version"]})
    } else {
        session.request(method, params, deadline)?
    };
    if matches!(
        operation,
        "incomingCalls" | "outgoingCalls" | "supertypes" | "subtypes"
    ) {
        let roots = bounded_items(&response)?;
        ensure!(roots.len() <= 100, "Hierarchy root budget exceeded");
        let hierarchy_method = match operation {
            "incomingCalls" => "callHierarchy/incomingCalls",
            "outgoingCalls" => "callHierarchy/outgoingCalls",
            "supertypes" => "typeHierarchy/supertypes",
            _ => "typeHierarchy/subtypes",
        };
        let mut pending: VecDeque<_> = roots.iter().cloned().map(|item| (item, 1usize)).collect();
        let mut visited = HashSet::new();
        let mut result = Vec::new();
        let mut result_bytes = 0usize;
        while let Some((root, depth)) = pending.pop_front() {
            deadline.check()?;
            if depth > options.depth
                || !visited.insert(identity(&(
                    root["uri"].clone(),
                    root["range"].clone(),
                    root["name"].clone(),
                )))
            {
                continue;
            }
            ensure!(visited.len() <= 2000, "Hierarchy traversal budget exceeded");
            let values = bounded_items(&session.request(
                hierarchy_method,
                json!({"item":root}),
                deadline,
            )?)?;
            for item in values {
                if depth < options.depth {
                    let next = match operation {
                        "incomingCalls" => item["from"].clone(),
                        "outgoingCalls" => item["to"].clone(),
                        _ => item.clone(),
                    };
                    pending.push_back((next, depth + 1));
                }
                let entry = json!({"root":root,"item":item,"depth":depth});
                result_bytes = result_bytes.saturating_add(serde_json::to_vec(&entry)?.len());
                result.push(entry);
                ensure!(
                    result.len() <= 10000 && result_bytes <= 4_000_000,
                    "Hierarchy result budget exceeded"
                );
            }
        }
        response = json!({"roots":roots,"entries":result});
    }
    let mut resolved_actions = HashSet::new();
    if operation == "codeActions" {
        let mut actions = bounded_items(&response)?;
        let mut bytes = serde_json::to_vec(&actions)?.len();
        for (index, action) in actions.iter_mut().take(options.max_items).enumerate() {
            deadline.check()?;
            if !requested_code_action(action, &options.only)
                || action.get("disabled").is_some()
                || action.get("edit").is_some()
                || action["command"].is_string()
                || capabilities["codeActionProvider"]["resolveProvider"] != true
            {
                continue;
            }
            let original = action.as_object().context("CodeAction must be an object")?;
            ensure!(
                original
                    .get("title")
                    .and_then(Value::as_str)
                    .is_some_and(|title| title.len() <= 2000),
                "Code action title missing or exceeds budget"
            );
            let resolved = session.request("codeAction/resolve", action.clone(), deadline)?;
            let object = resolved
                .as_object()
                .context("Resolved CodeAction must be an object")?;
            for (key, value) in original {
                ensure!(
                    object.get(key) == Some(value),
                    "Resolved code action changed existing attribute: {key}"
                );
            }
            bytes = bytes
                .checked_sub(serde_json::to_vec(action)?.len())
                .and_then(|bytes| bytes.checked_add(serde_json::to_vec(&resolved).ok()?.len()))
                .context("Resolved code action capacity overflow")?;
            ensure!(
                bytes <= 4_000_000,
                "Resolved code action result budget exceeded"
            );
            *action = resolved;
            resolved_actions.insert(index);
        }
        response = json!(actions);
    }
    session.shutdown(deadline)?;
    drop(session);
    deadline.check()?;
    check_copy_verified(&engine.store, snapshot, tree.path(), &exclusions, deadline)?;
    if let Some(rename) = rename_resource {
        let mut documents = Vec::new();
        if !response.is_null() {
            let object = response
                .as_object()
                .context("willRenameFiles WorkspaceEdit must be object or null")?;
            ensure!(
                object
                    .keys()
                    .all(|key| ["changes", "documentChanges"].contains(&key.as_str()))
                    && !(object.contains_key("changes") && object.contains_key("documentChanges")),
                "Unsupported or ambiguous willRenameFiles WorkspaceEdit"
            );
            if let Some(changes) = response.get("changes") {
                let changes = changes
                    .as_object()
                    .filter(|changes| changes.len() <= 2000)
                    .context("Invalid willRenameFiles changes")?;
                for (uri, edits) in changes {
                    documents
                        .push(json!({"textDocument":{"uri":uri,"version":null},"edits":edits}));
                }
            } else if let Some(changes) = response.get("documentChanges") {
                documents = changes
                    .as_array()
                    .filter(|changes| changes.len() < 2000)
                    .context("Invalid willRenameFiles document changes")?
                    .clone();
            }
        }
        if directory.is_some() {
            ensure!(
                documents
                    .iter()
                    .all(|document| document.get("kind").is_none()),
                "Directory move provider may return only original-coordinate text edits"
            );
        }
        documents.push(rename);
        let checked = check_workspace_edit_directory(
            engine,
            snapshot,
            graph,
            tree.path(),
            &json!({"documentChanges":documents}),
            directory,
            deadline,
        )?;
        if let Some(staged) = staged.as_deref_mut() {
            let index = staged.len();
            staged.push(checked);
            return Ok(
                json!({"snapshotId":snapshot.id,"stagedEditIndex":index,"producer":server.command,"semantic":"will_rename_files_before_guarded_resource_plan","complete":false,"commandsExecuted":false}),
            );
        }
        let plan = checked.prepare(engine, snapshot, graph, deadline)?;
        return Ok(
            json!({"snapshotId":snapshot.id,"plan":plan,"producer":server.command,"semantic":"will_rename_files_before_guarded_resource_plan","complete":false,"commandsExecuted":false}),
        );
    }
    if operation == "codeActions" {
        let mut items = Vec::new();
        let mut pending = Vec::new();
        let mut pending_bytes = 0usize;
        let all = bounded_items(&response)?;
        let total = all.len();
        for (index, action) in all.into_iter().take(options.max_items).enumerate() {
            deadline.check()?;
            if !requested_code_action(&action, &options.only) {
                continue;
            }
            let title = action["title"]
                .as_str()
                .context("Code action title missing")?;
            ensure!(title.len() <= 2000, "Code action title exceeds budget");
            let mut item = json!({"title":title,"kind":action["kind"],"usable":false,"hasWorkspaceEdit":action.get("edit").is_some(),"commandExecution":"refused"});
            item["resolution"] = json!(if resolved_actions.contains(&index) {
                "resolved"
            } else if action.get("edit").is_some() {
                "explicit_edit"
            } else {
                "unresolved"
            });
            if action.get("disabled").is_some() {
                item["disabled"] = action["disabled"].clone();
            } else if let Some(edit) = action.get("edit") {
                if action.get("command").is_some() {
                    item["limitation"] = json!(
                        "Combined edit/command action requires unsupported command execution"
                    );
                } else {
                    let checked =
                        check_workspace_edit(engine, snapshot, graph, tree.path(), edit, deadline)?;
                    pending_bytes = pending_bytes.saturating_add(checked.retained_bytes());
                    ensure!(
                        pending_bytes <= 32_000_000,
                        "Checked action candidate capacity exceeded"
                    );
                    pending.push((items.len(), checked));
                    item["usable"] = json!(true);
                }
            } else {
                item["limitation"] = json!(
                    "No explicit WorkspaceEdit; command execution and unresolved actions are unavailable"
                );
            }
            items.push(item);
        }
        // Complete every requested action's admission before publishing any
        // sibling. A later malformed action cannot leave an earlier hidden plan.
        for (index, checked) in pending {
            deadline.check()?;
            if let Some(staged) = staged.as_deref_mut() {
                items[index]["stagedEditIndex"] = json!(staged.len());
                staged.push(checked);
            } else {
                items[index]["plan"] = json!(checked.prepare(engine, snapshot, graph, deadline)?);
            }
        }
        return Ok(
            json!({"snapshotId":snapshot.id,"items":items,"itemsOmitted":total.saturating_sub(options.max_items),"producer":server.command,"diagnosticContext":action_diagnostic_context,"complete":false}),
        );
    }
    let mut normalize = Normalizer::new(
        &engine.store,
        snapshot,
        tree.path(),
        &server.command,
        deadline,
    )?;
    if matches!(
        operation,
        "incomingCalls" | "outgoingCalls" | "supertypes" | "subtypes"
    ) {
        let mut roots = Vec::new();
        for item in bounded_items(&response["roots"])? {
            roots.push(normalize.item(&item, options.include_body)?);
        }
        let entries = bounded_items(&response["entries"])?;
        let total = entries.len();
        let mut items = Vec::new();
        for entry in entries.into_iter().take(options.max_items) {
            let root = normalize.item(&entry["root"], false)?;
            let item = &entry["item"];
            if matches!(operation, "incomingCalls" | "outgoingCalls") {
                let key = if operation == "incomingCalls" {
                    "from"
                } else {
                    "to"
                };
                let target = normalize.item(&item[key], options.include_body)?;
                let source = if operation == "incomingCalls" {
                    &target
                } else {
                    &root
                };
                let path = source["path"].as_str().unwrap();
                let mut ranges = Vec::new();
                for span in bounded_items(&item["fromRanges"])? {
                    let anchor = normalize.anchor(path, &span, true)?;
                    ensure!(
                        anchor["startByte"].as_u64() >= source["startByte"].as_u64()
                            && anchor["endByte"].as_u64() <= source["endByte"].as_u64(),
                        "Call occurrence escaped its referring symbol"
                    );
                    ranges.push(anchor);
                }
                let mut value = json!({"root":root,"fromRanges":ranges,"depth":entry["depth"]});
                value[key] = target;
                items.push(value);
            } else {
                let mut value = normalize.item(item, options.include_body)?;
                value["root"] = root;
                value["depth"] = entry["depth"].clone();
                items.push(value);
            }
        }
        return Ok(
            json!({"snapshotId":snapshot.id,"operation":operation,"roots":roots,"items":items,"itemsOmitted":total.saturating_sub(options.max_items),"producer":server.command,"complete":false}),
        );
    }
    if matches!(operation, "symbolInfo" | "symbols") {
        let mut items = Vec::new();
        normalize.symbols(
            path,
            &bounded_items(&response)?,
            ("", None, 0),
            &options,
            &mut items,
        )?;
        let total = items.len();
        items.truncate(options.max_items);
        return Ok(
            json!({"snapshotId":snapshot.id,"items":items,"itemsOmitted":total.saturating_sub(options.max_items),"producer":server.command,"complete":false}),
        );
    }
    if operation == "prepareRename" {
        ensure!(
            !response.is_null(),
            "Provider cannot prepare rename at this position"
        );
        ensure!(
            response.get("defaultBehavior").is_none(),
            "Default rename behavior has no exact provider anchor"
        );
        let span = response.get("range").unwrap_or(&response);
        let anchor = normalize.anchor(path, span, true)?;
        return Ok(
            json!({"snapshotId":snapshot.id,"range":anchor,"placeholder":response["placeholder"],"producer":server.command,"complete":false}),
        );
    }
    if operation == "diagnostics" {
        ensure!(
            response["kind"] == "full",
            "Initial diagnostics require a full report"
        );
        let mut items = Vec::new();
        let source_items = bounded_items(&response["items"])?;
        for item in source_items {
            let severity = item["severity"].as_u64().unwrap_or(4);
            ensure!((1..=4).contains(&severity), "Invalid diagnostic severity");
            let mut anchor = normalize.anchor(path, &item["range"], false)?;
            let message = item["message"]
                .as_str()
                .context("Diagnostic message missing")?;
            ensure!(message.len() <= 16000, "Diagnostic message exceeds budget");
            anchor["message"] = json!(message);
            anchor["severity"] = json!(severity);
            anchor["code"] = item["code"].clone();
            anchor["source"] = item["source"].clone();
            if severity <= options.min_severity {
                items.push(anchor);
            }
        }
        let total = items.len();
        items.truncate(options.max_items);
        return Ok(
            json!({"snapshotId":snapshot.id,"items":items,"itemsOmitted":total.saturating_sub(options.max_items),"producer":server.command,"complete":false,"diagnosticMode":if push_diagnostics{"push"}else{"pull"},"reportedDocumentVersion":response["version"]}),
        );
    }
    if operation == "completion" {
        let list = if response.is_array() {
            response.clone()
        } else {
            response["items"].clone()
        };
        let all = bounded_items(&list)?;
        let total = all.len();
        let mut items = Vec::new();
        for mut item in all.into_iter().take(options.max_items) {
            ensure!(
                item["label"]
                    .as_str()
                    .is_some_and(|label| label.len() <= 2000),
                "Completion label missing or oversized"
            );
            if let Some(edit) = item.get("textEdit").cloned() {
                let mut edit = edit;
                let replacement = edit["newText"]
                    .as_str()
                    .context("Completion edit text missing")?
                    .to_owned();
                let span = if edit.get("range").is_some() {
                    &edit["range"]
                } else {
                    &edit["replace"]
                };
                let mut anchor = normalize.anchor(path, span, false)?;
                anchor["newText"] = json!(replacement);
                if edit.get("insert").is_some() {
                    edit["insert"] = normalize.anchor(path, &edit["insert"], false)?;
                    ensure!(
                        edit["insert"]["startByte"] == anchor["startByte"]
                            && edit["insert"]["endByte"].as_u64() <= anchor["endByte"].as_u64(),
                        "Completion insert range must share replace start and fit inside replace range"
                    );
                    anchor["insert"] = edit["insert"].clone();
                }
                item["textEdit"] = anchor;
            }
            if let Some(additional) = item.get("additionalTextEdits").cloned() {
                let mut edits = Vec::new();
                for edit in bounded_items(&additional)? {
                    let mut anchor = normalize.anchor(path, &edit["range"], false)?;
                    let text = edit["newText"]
                        .as_str()
                        .context("Completion additional edit text must be a string")?;
                    ensure!(
                        text.len() <= 4_000_000,
                        "Completion additional edit text budget exceeded"
                    );
                    anchor["newText"] = json!(text);
                    edits.push(anchor);
                }
                item["additionalTextEdits"] = json!(edits);
            }
            if item.get("command").is_some() {
                item.as_object_mut().unwrap().remove("command");
                item["commandExecution"] = json!("refused");
            }
            items.push(item);
        }
        return Ok(
            json!({"snapshotId":snapshot.id,"items":items,"isIncomplete":response["isIncomplete"].as_bool().unwrap_or(false),"itemsOmitted":total.saturating_sub(options.max_items),"producer":server.command,"complete":false}),
        );
    }
    if operation == "signatureHelp" {
        return Ok(
            json!({"snapshotId":snapshot.id,"response":response,"producer":server.command,"trust":"source-data","complete":false}),
        );
    }
    if operation == "rename" {
        let checked =
            check_workspace_edit(engine, snapshot, graph, tree.path(), &response, deadline)?;
        if let Some(staged) = staged {
            let index = staged.len();
            staged.push(checked);
            return Ok(
                json!({"snapshotId":snapshot.id,"stagedEditIndex":index,"producer":server.command,"complete":false}),
            );
        }
        let plan = checked.prepare(engine, snapshot, graph, deadline)?;
        deadline.check()?;
        return Ok(
            json!({"snapshotId":snapshot.id,"plan":plan,"producer":server.command,"semantic":"language-server-result","complete":false}),
        );
    }
    if operation == "hover" {
        deadline.check()?;
        if encoding.is_some()
            && let Some(span) = response.get("range").cloned()
        {
            response["range"] = normalize.anchor(path, &span, false)?;
        }
        let mut result = json!({"snapshotId":snapshot.id,"response":response,"producer":server.command,"trust":"source-data","complete":false});
        source_view.annotate(&mut result, content_hash);
        return Ok(result);
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
        let relative = relative_uri_scoped(tree.path(), uri, snapshot, Some(&normalize.scope))?;
        if !sources.contains_key(&relative) {
            let text = normalize.scope.source_view(&relative)?;
            let hash = normalize.scope.file(&relative)?.hash.clone();
            let size = if text.policy().is_some() {
                text.retained_bytes()
            } else {
                text.text().len()
            };
            ensure!(
                size <= 16_000_000,
                "source_encoding_capability_unavailable: semantic mapping cache capacity"
            );
            if cached_bytes.saturating_add(size) > 16_000_000 {
                sources.clear();
                cached_bytes = 0;
            }
            cached_bytes += size;
            sources.insert(relative.clone(), (text, hash));
        }
        let (view, hash) = &sources[&relative];
        let text = view.text();
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
        let mut item = json!({"path":relative,"contentHash":hash,"startByte":view.to_raw(start)?,"endByte":view.to_raw(end)?,"producer":server.command,"resolution":"exact_binding","trust":"source-data"});
        view.annotate(&mut item, hash);
        let size = end - start;
        if size <= 6000 && size <= 256_000usize.saturating_sub(body_bytes) {
            item["text"] = json!(&text[start..end]);
            body_bytes += size;
        } else {
            item["bodyOmitted"] = json!(true);
            item["readWith"] = json!({"tool":"ci_read","snapshotId":snapshot.id,"path":relative,"startByte":view.to_raw(start)?,"endByte":view.to_raw(end)?});
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
    #[test]
    fn grown_provider_copy_is_rejected_without_reading_unbounded_source() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("main.rs");
        let text = "fn main() {}\n";
        fs::write(&path, text).unwrap();
        let file = crate::model::SourceFile {
            path: "main.rs".into(),
            hash: crate::model::hash(text),
            size: text.len(),
            language: "rust".into(),
            mode: 0o644,
        };
        fs::OpenOptions::new()
            .write(true)
            .open(&path)
            .unwrap()
            .set_len(128_000_000)
            .unwrap();
        let snapshot = Snapshot {
            id: "a".repeat(64),
            workspace_id: "b".repeat(64),
            kind: "disk".into(),
            files: vec![file],
            omissions: vec![],
            capture_consistency: "test".into(),
        };
        let started = std::time::Instant::now();
        assert!(check_copy(&snapshot, dir.path(), &Deadline::new(300)).is_err());
        assert!(
            started.elapsed() < std::time::Duration::from_secs(1),
            "An altered provider file must be size-checked before hashing"
        );
    }
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
    fn strict_lifecycle_omits_params_and_exits_naturally_after_shutdown_response() {
        let _fixture = PROTOCOL_FIXTURE
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        let directory = tempfile::tempdir().unwrap();
        let deadline = Deadline::new(10000);
        let mut session = fixture("strict-lifecycle", directory.path(), &deadline);
        session.shutdown(&deadline).unwrap();
        let report = receive_cooperatively(&session.messages, &deadline)
            .unwrap()
            .unwrap();
        assert_eq!(report["method"], "fixture/lifecycle");
        assert_eq!(report["params"]["valid"], true);
        let status = session.child.as_mut().unwrap().try_wait().unwrap();
        assert!(status.is_some_and(|status| status.success()));
    }
    #[test]
    fn explicit_null_request_and_notification_params_remain_present() {
        let _fixture = PROTOCOL_FIXTURE
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        let directory = tempfile::tempdir().unwrap();
        let deadline = Deadline::new(10000);
        let mut session = fixture("strict-lifecycle", directory.path(), &deadline);
        let result = session
            .request("fixture/echoParams", Value::Null, &deadline)
            .unwrap();
        assert_eq!(result, json!({"hasParams":true,"params":null}));
        session.notify("fixture/echoParams", Value::Null).unwrap();
        let notification = receive_cooperatively(&session.messages, &deadline)
            .unwrap()
            .unwrap();
        assert_eq!(notification["method"], "fixture/echoed");
        assert_eq!(notification["params"], result);
        let error = session
            .request("shutdown", Value::Null, &deadline)
            .unwrap_err();
        assert!(error.to_string().contains("InvalidParams"), "{error}");
        session.shutdown(&deadline).unwrap();
        assert!(
            session
                .child
                .as_mut()
                .unwrap()
                .try_wait()
                .unwrap()
                .unwrap()
                .success()
        );
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
            let result = run_at_configured(
                &mut engine,
                &ConfiguredServer {
                    server,
                    configuration: HostConfiguration {
                        passive_profile: Some(PassiveProfile::ContractFixture),
                        ..Default::default()
                    },
                },
                &snapshot,
                &graph,
                SemanticRequest {
                    path: target,
                    offset: 0,
                    operation: "definition",
                    new_name: None,
                },
                &json!({}),
                &deadline,
            )
            .unwrap();
            assert_eq!(result["locations"][0]["path"], target);
        }
    }
    #[test]
    fn react_fixture_uses_requested_document_and_equivalent_tilde_uri_spellings() {
        let _fixture = PROTOCOL_FIXTURE
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().join("copied~workspace");
        fs::create_dir(&root).unwrap();
        let setup = Deadline::new(10000);
        let mut session = fixture("react-language-ids", &root, &setup);
        let target = uri(&root.join("component.tsx")).unwrap();
        assert!(target.contains('~'));
        let encoded = target.replace('~', "%7E");
        for (document, language) in [
            (target, "typescriptreact"),
            (
                uri(&root.join("support.ts")).unwrap().replace('~', "%7E"),
                "typescript",
            ),
        ] {
            session.notify("textDocument/didOpen", json!({"textDocument":{
                "uri":document,"languageId":language,"version":1,"text":"// fixture\nconst helper = 1;"}
            })).unwrap();
        }
        let response = session
            .request(
                "textDocument/definition",
                json!({"textDocument":{"uri":encoded},"position":{"line":0,"character":0}}),
                &setup,
            )
            .unwrap();
        assert_eq!(response["uri"], encoded);
        session.shutdown(&setup).unwrap();
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
