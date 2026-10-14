use anyhow::{Result, bail, ensure};
use clap::{Parser, Subcommand};
use serde_json::{Value, json};
use std::{
    fs,
    io::{self, Read, Write},
    path::PathBuf,
};
use yoke::{Engine, capture::Deadline};

#[derive(Parser)]
#[command(
    name = "yoke",
    version,
    about = "Native local code intelligence; immutable source, evidence graphs and guarded edits"
)]
struct Cli {
    #[arg(long, global = true, default_value = ".")]
    root: PathBuf,
    #[arg(long, global = true)]
    snapshot: Option<String>,
    #[arg(long, global = true, default_value_t = 120000)]
    timeout: u64,
    #[arg(long, global = true, default_value_t = 32000)]
    max_bytes: u64,
    #[arg(long, global = true)]
    allow_writes: bool,
    #[arg(long, global = true)]
    language_server_config: Option<PathBuf>,
    #[arg(long, global = true)]
    trust_servers: bool,
    #[command(subcommand)]
    command: Operation,
}
#[derive(Subcommand)]
enum Operation {
    Index,
    Credentials {
        #[command(subcommand)]
        command: CredentialOperation,
    },
    Runtime {
        entry: String,
        #[arg(long)]
        python: PathBuf,
        #[arg(long)]
        requests: PathBuf,
        #[arg(long)]
        trust_runtime: bool,
        #[arg(long)]
        allow_evaluation: bool,
        #[arg(long, default_value_t = 1_000_000)]
        max_output_bytes: usize,
    },
    FederationService {
        #[arg(long)]
        profile: PathBuf,
        #[arg(long)]
        trust_service: bool,
        #[arg(long)]
        allow_remote_bind: bool,
        #[arg(long, default_value_t = 0)]
        duration_ms: u64,
    },
    FederationRequest {
        #[arg(long)]
        profile: PathBuf,
        #[arg(long)]
        input: PathBuf,
        #[arg(long)]
        trust_transport: bool,
        #[arg(long)]
        allow_plaintext_loopback: bool,
    },
    Dataflow {
        #[arg(long)]
        policy: Option<PathBuf>,
    },
    Compiler {
        #[arg(long)]
        node: PathBuf,
        #[arg(long)]
        typescript: PathBuf,
        #[arg(long)]
        trust_compiler: bool,
        #[arg(long)]
        build: bool,
    },
    TestMap {
        #[command(subcommand)]
        command: TestMapOperation,
    },
    Models {
        #[arg(long)]
        config: PathBuf,
        #[arg(long)]
        trust_provider: bool,
        #[command(subcommand)]
        command: ModelOperation,
    },
    Ledger {
        task: String,
    },
    Retention {
        #[command(subcommand)]
        command: RetentionOperation,
    },
    VerifyContainer {
        #[arg(long)]
        plan: String,
        #[arg(long)]
        profile: PathBuf,
        #[arg(long)]
        trust_project: bool,
    },
    Status,
    Analyze,
    Remember {
        #[arg(long)]
        input: PathBuf,
    },
    Recall {
        query: String,
        #[arg(long, default_value_t = 20)]
        limit: usize,
    },
    Vectors {
        #[command(subcommand)]
        command: VectorOperation,
    },
    Import {
        format: String,
        #[arg(long)]
        input: PathBuf,
    },
    Media {
        #[command(subcommand)]
        command: MediaOperation,
    },
    Federation {
        #[command(subcommand)]
        command: FederationOperation,
    },
    Maintenance {
        #[command(subcommand)]
        command: MaintenanceOperation,
    },
    #[command(hide = true)]
    ModelWorker {
        #[arg(long)]
        endpoint: String,
        #[arg(long)]
        model: String,
        #[arg(long)]
        digest: String,
    },
    #[command(hide = true)]
    MediaPdfWorker {
        #[arg(long)]
        input: PathBuf,
    },
    Editor {
        #[arg(long, default_value_t = 0)]
        port: u16,
        #[arg(long)]
        max_requests: Option<usize>,
    },
    Export {
        #[arg(long, default_value = "json")]
        format: String,
        #[arg(long)]
        output: Option<PathBuf>,
    },
    Find {
        query: String,
        #[arg(long, default_value = "symbol")]
        kind: String,
        #[arg(long)]
        path: Option<String>,
        #[arg(long, default_value_t = 30)]
        limit: u64,
        #[arg(long)]
        regex: bool,
    },
    Read {
        path: String,
        #[arg(long)]
        start_byte: Option<u64>,
        #[arg(long)]
        end_byte: Option<u64>,
        #[arg(long)]
        start_line: Option<u64>,
        #[arg(long)]
        end_line: Option<u64>,
    },
    Outline {
        path: String,
    },
    Trace {
        entity: String,
        #[arg(long, default_value = "out")]
        direction: String,
        #[arg(long, default_value_t = 2)]
        depth: u64,
        #[arg(long)]
        relation: Vec<String>,
    },
    Context {
        query: String,
        #[arg(long, default_value_t = 5)]
        limit: u64,
    },
    Path {
        from: String,
        to: String,
        #[arg(long, default_value = "out")]
        direction: String,
    },
    Invoke {
        tool: String,
        #[arg(long, default_value = "{}")]
        args: String,
    },
    Serve,
    Prepare {
        #[arg(long)]
        input: PathBuf,
    },
    Approve {
        plan: String,
        #[arg(long)]
        allow_test_changes: bool,
    },
    Apply {
        plan: String,
        #[arg(long)]
        key: String,
        #[arg(long, default_value = "isolated")]
        mode: String,
    },
    Verify {
        #[arg(long)]
        plan: String,
        #[arg(long)]
        command: Option<String>,
        #[arg(long = "arg", allow_hyphen_values = true)]
        args: Vec<String>,
        #[arg(long)]
        trust_project: bool,
    },
    Recover {
        journal: String,
    },
    Lsp {
        #[arg(long)]
        command: String,
        #[arg(long = "server-arg", allow_hyphen_values = true)]
        args: Vec<String>,
        #[arg(long)]
        language: String,
        #[arg(long)]
        path: String,
        #[arg(long, default_value_t = 0)]
        offset: usize,
        #[arg(long, default_value = "definition")]
        operation: String,
        #[arg(long)]
        new_name: Option<String>,
        #[arg(long)]
        trust_server: bool,
    },
    Position {
        #[arg(long)]
        text: String,
        #[arg(long)]
        byte: Option<usize>,
        #[arg(long)]
        line: Option<usize>,
        #[arg(long)]
        character: Option<usize>,
    },
    Watch {
        #[arg(long, default_value_t = 1000)]
        interval: u64,
        #[arg(long, default_value_t = 0)]
        iterations: u64,
    },
}
#[derive(Subcommand)]
enum VectorOperation {
    Build,
    Import {
        #[arg(long)]
        input: PathBuf,
    },
    Search {
        #[arg(long)]
        model: String,
        #[arg(long)]
        query: Option<String>,
        #[arg(long)]
        input: Option<PathBuf>,
        #[arg(long, default_value_t = 20)]
        limit: usize,
    },
}
#[derive(Subcommand)]
enum TestMapOperation {
    Import {
        #[arg(long)]
        input: PathBuf,
    },
    Collect {
        #[arg(long)]
        selections: PathBuf,
        #[arg(long)]
        node: String,
        #[arg(long)]
        trust_runtime: bool,
    },
    Select {
        #[arg(long)]
        base: String,
        #[arg(long = "test")]
        tests: Vec<String>,
    },
}
#[derive(Subcommand)]
enum CredentialOperation {
    Tls {
        name: String,
        #[arg(long = "hostname", required = true)]
        hostnames: Vec<String>,
    },
    Token {
        name: String,
    },
}
#[derive(Subcommand)]
enum ModelOperation {
    Embeddings,
    Search {
        query: String,
        #[arg(long, default_value_t = 20)]
        limit: usize,
    },
    Rerank {
        query: String,
        #[arg(long = "entity")]
        entities: Vec<String>,
    },
    Summarize {
        #[arg(long = "entity")]
        entities: Vec<String>,
    },
    Media {
        path: String,
    },
}
#[derive(Subcommand)]
enum RetentionOperation {
    Plan {
        #[arg(long, default_value_t = 86_400_000)]
        minimum_age_ms: u64,
        #[arg(long, default_value_t = 600_000)]
        expires_after_ms: u64,
    },
    Apply {
        plan: String,
        #[arg(long)]
        confirmed_hash: String,
        #[arg(long)]
        allow_state_deletion: bool,
    },
    Recover,
    PurgePlan {
        #[arg(long, default_value_t = 86_400_000)]
        minimum_age_ms: u64,
    },
    Purge {
        plan: String,
        #[arg(long)]
        confirmed_hash: String,
        #[arg(long)]
        allow_state_deletion: bool,
    },
}
#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct HostProvider {
    command: PathBuf,
    #[serde(default)]
    args: Vec<String>,
    model_id: String,
    model_version: String,
    task_id: String,
    currency: String,
    max_cost_micros: u64,
    task_limit_micros: u64,
}
#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct HostContainer {
    docker_command: String,
    daemon_endpoint: String,
    #[serde(default)]
    docker_args: Vec<String>,
    image: String,
    command: Vec<String>,
    isolation: String,
    user: String,
    memory_mb: u32,
    cpus: u32,
    pids: u32,
    tmpfs_mb: u32,
    test_count_pattern: String,
}
#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct HostService {
    bind: std::net::SocketAddr,
    tls_credential_name: Option<String>,
    server_names: Vec<String>,
    request_timeout_ms: u64,
    max_body_bytes: usize,
    max_response_bytes: usize,
    imports: Vec<HostImport>,
    exports: Vec<HostExport>,
    principals: Vec<HostPrincipal>,
}
#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct HostImport {
    id: String,
    grant: yoke::federation::ImportGrant,
}
#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct HostExport {
    id: String,
    key_name: String,
    configuration: yoke::federation::ExportScope,
}
#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct HostPrincipal {
    id: String,
    token_credential_name: String,
    roles: Vec<yoke::federation_http::Role>,
    import_ids: Vec<String>,
    export_ids: Vec<String>,
}
#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct HostClient {
    address: std::net::SocketAddr,
    server_name: String,
    ca_certificates: Vec<String>,
    token_credential_name: String,
    max_body_bytes: usize,
    max_response_bytes: usize,
}
#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct HostRequest {
    method: String,
    path: String,
    body: Option<Value>,
}
#[derive(Subcommand)]
enum MediaOperation {
    Inspect {
        path: String,
    },
    Transcript {
        path: String,
        #[arg(long)]
        input: PathBuf,
    },
}
#[derive(Subcommand)]
enum FederationOperation {
    Keygen {
        name: String,
    },
    Export {
        #[arg(long)]
        scope: PathBuf,
        #[arg(long)]
        key_name: String,
        #[arg(long)]
        output: PathBuf,
    },
    Import {
        #[arg(long)]
        input: PathBuf,
        #[arg(long)]
        grant: PathBuf,
        #[arg(long)]
        trust_grant: bool,
    },
    List,
    Read {
        remote_id: String,
        path: String,
        #[arg(long)]
        grant: PathBuf,
        #[arg(long)]
        trust_grant: bool,
    },
    Find {
        remote_id: String,
        query: String,
        #[arg(long, default_value = "symbol")]
        kind: String,
        #[arg(long, default_value_t = 30)]
        limit: u64,
        #[arg(long)]
        grant: PathBuf,
        #[arg(long)]
        trust_grant: bool,
    },
}
#[derive(Subcommand)]
enum MaintenanceOperation {
    Collect {
        #[arg(long)]
        apply: bool,
        #[arg(long, default_value_t = 86_400_000)]
        minimum_age_ms: u64,
        #[arg(long)]
        purge_quarantine: bool,
    },
    Pin {
        snapshot: String,
        id: String,
        #[arg(long)]
        reason: String,
    },
    Unpin {
        id: String,
    },
}
fn host_grant(path: &std::path::Path, trusted: bool) -> Result<yoke::federation::ImportGrant> {
    ensure!(
        trusted,
        "Source grants require explicit host --trust-grant permission"
    );
    yoke::json::decode(input_json(path, 64000)?)
}
fn print_bounded(engine: &Engine, value: &Value, max: u64) -> Result<()> {
    let bytes = serde_json::to_vec(value)?;
    if bytes.len() as u64 <= max {
        return print(value);
    }
    let id = engine.store.artifact(&bytes)?;
    print(
        &json!({"status":"partial","artifactId":id,"totalBytes":bytes.len(),"readWith":{"tool":"ci_read","artifactId":id},"warning":"Complete host result retained as an immutable artifact."}),
    )
}
fn input_json(path: &std::path::Path, max: u64) -> Result<Value> {
    ensure!(
        fs::metadata(path)?.len() <= max,
        "Input artifact exceeds byte capacity"
    );
    let mut bytes = Vec::new();
    fs::File::open(path)?
        .take(max + 1)
        .read_to_end(&mut bytes)?;
    ensure!(bytes.len() as u64 <= max, "Input artifact changed size");
    yoke::json::parse(&bytes, max as usize)
}
fn print(value: &Value) -> Result<()> {
    let stdout = io::stdout();
    let mut out = stdout.lock();
    serde_json::to_writer(&mut out, value)?;
    writeln!(out)?;
    Ok(())
}
fn entity_id(graph: &yoke::model::Graph, name: &str) -> Result<String> {
    let matched: Vec<_> = graph
        .entities
        .iter()
        .filter(|e| e.id == name || e.qualified_name == name || e.name == name)
        .collect();
    ensure!(
        matched.len() == 1,
        "Entity name missing or ambiguous; use an exact entity ID or qualified name"
    );
    Ok(matched[0].id.clone())
}
fn run() -> Result<bool> {
    let cli = Cli::parse();
    if let Operation::Credentials { ref command } = cli.command {
        let value = match command {
            CredentialOperation::Tls { name, hostnames } => {
                yoke::host_credentials::generate_tls(name, hostnames)?
            }
            CredentialOperation::Token { name } => yoke::host_credentials::generate_token(name)?,
        };
        print(&value)?;
        return Ok(true);
    }
    if let Operation::Federation {
        command: FederationOperation::Keygen { ref name },
    } = cli.command
    {
        print(&yoke::credentials::generate(name)?)?;
        return Ok(true);
    }
    if let Operation::MediaPdfWorker { ref input } = cli.command {
        print(&yoke::media::pdf_worker(input)?)?;
        return Ok(true);
    }
    if let Operation::ModelWorker {
        ref endpoint,
        ref model,
        ref digest,
    } = cli.command
    {
        ensure!(
            (10..=600_000).contains(&cli.timeout),
            "Invalid model worker deadline"
        );
        let deadline = Deadline::new(cli.timeout);
        let (sender, receiver) = std::sync::mpsc::sync_channel(1);
        std::thread::spawn(move || {
            let result = (|| -> io::Result<Vec<u8>> {
                let mut input = Vec::new();
                io::stdin().take(16_000_001).read_to_end(&mut input)?;
                Ok(input)
            })();
            let _ = sender.send(result);
        });
        let input = receiver
            .recv_timeout(deadline.remaining())
            .map_err(|_| anyhow::anyhow!("Model worker input deadline exceeded"))??;
        deadline.check()?;
        ensure!(
            input.len() <= 16_000_000,
            "Model worker request exceeds capacity"
        );
        print(&yoke::ollama::provider_reply(
            &yoke::ollama::OllamaSpec {
                endpoint: endpoint.clone(),
                model: model.clone(),
                model_digest: digest.clone(),
            },
            &input,
            &deadline,
        )?)?;
        return Ok(true);
    }
    ensure!(
        (10..=600000).contains(&cli.timeout) && (1024..=4000000).contains(&cli.max_bytes),
        "Invalid timeout or output budget"
    );
    if let Operation::Position {
        text,
        byte,
        line,
        character,
    } = cli.command
    {
        let value = if let Some(byte) = byte {
            ensure!(
                line.is_none() && character.is_none(),
                "Choose byte or line/character"
            );
            yoke::lsp::byte_to_position(&text, byte)?
        } else {
            let position = json!({"line":line.ok_or_else(||anyhow::anyhow!("line required"))?,"character":character.ok_or_else(||anyhow::anyhow!("character required"))?});
            json!({"byte":yoke::lsp::position_to_byte(&text,&position)?})
        };
        print(&value)?;
        return Ok(true);
    }
    let deadline = Deadline::new(cli.timeout);
    let mut engine = Engine::open_with_timeout(&cli.root, deadline.remaining())?;
    engine.allow_writes = cli.allow_writes;
    if let Some(config) = cli.language_server_config {
        ensure!(
            cli.trust_servers,
            "Host language server configuration requires --trust-servers"
        );
        #[derive(serde::Deserialize)]
        #[serde(deny_unknown_fields)]
        struct ConfiguredServer {
            language: String,
            command: String,
            #[serde(default)]
            args: Vec<String>,
        }
        let servers: Vec<ConfiguredServer> = yoke::json::decode(input_json(&config, 1_000_000)?)?;
        ensure!(servers.len() <= 100, "Too many configured servers");
        for server in servers {
            engine.register_server(yoke::lsp::Server {
                command: server.command,
                args: server.args,
                language: server.language,
                trust: true,
            })?;
        }
    }
    engine.store.set_deadline(&deadline)?;
    let mut arguments = json!({"timeoutMs":cli.timeout,"maxBytes":cli.max_bytes});
    if let Some(snapshot) = cli.snapshot {
        arguments["snapshotId"] = json!(snapshot);
    }
    let tool = match cli.command {
        Operation::FederationRequest {
            profile,
            input,
            trust_transport,
            allow_plaintext_loopback,
        } => {
            use base64::Engine as _;
            ensure!(
                trust_transport,
                "Federation transport requires --trust-transport"
            );
            let profile: HostClient = yoke::json::decode(input_json(&profile, 64_000)?)?;
            ensure!(
                profile.ca_certificates.len() <= 8,
                "Too many trusted TLS certificates"
            );
            let ca_certificates = profile
                .ca_certificates
                .iter()
                .map(|encoded| {
                    base64::prelude::BASE64_STANDARD
                        .decode(encoded)
                        .map_err(anyhow::Error::from)
                })
                .collect::<Result<Vec<_>>>()?;
            let token = yoke::host_credentials::read_token(&profile.token_credential_name)?;
            let config = yoke::federation_http::ClientConfig {
                address: profile.address,
                server_name: profile.server_name,
                ca_certificates,
                token,
                allow_plaintext_loopback,
                max_body_bytes: profile.max_body_bytes,
                max_response_bytes: profile.max_response_bytes,
            };
            let request: HostRequest = yoke::json::decode(input_json(&input, 32_000_000)?)?;
            let response = yoke::federation_http::request(
                &config,
                &request.method,
                &request.path,
                request.body,
                &deadline,
            )?;
            print_bounded(
                &engine,
                &json!({"statusCode":response.status,"body":response.body}),
                cli.max_bytes,
            )?;
            return Ok((200..300).contains(&response.status));
        }
        Operation::FederationService {
            profile,
            trust_service,
            allow_remote_bind,
            duration_ms,
        } => {
            ensure!(trust_service, "Federation service requires --trust-service");
            ensure!(
                duration_ms <= 86_400_000,
                "Service duration exceeds one day; use zero for foreground operation"
            );
            let host: HostService = yoke::json::decode(input_json(&profile, 128_000)?)?;
            ensure!(
                host.principals.len() <= 100
                    && host.imports.len() <= 100
                    && host.exports.len() <= 100,
                "Service grant capacity exceeded"
            );
            let tls = host
                .tls_credential_name
                .as_deref()
                .map(yoke::host_credentials::read_tls)
                .transpose()?
                .map(|identity| yoke::federation_http::TlsMaterial {
                    certificates: vec![identity.certificate],
                    private_key: identity.private_key,
                });
            let imports = host
                .imports
                .into_iter()
                .map(|access| yoke::federation_http::ImportAccess {
                    id: access.id,
                    grant: access.grant,
                })
                .collect();
            let mut exports = Vec::new();
            for access in host.exports {
                exports.push(yoke::federation_http::ExportAccess {
                    id: access.id,
                    configuration: access.configuration,
                    private_key: zeroize::Zeroizing::new(yoke::credentials::read(
                        &access.key_name,
                    )?),
                });
            }
            let mut principals = Vec::new();
            for principal in host.principals {
                let token = yoke::host_credentials::read_token(&principal.token_credential_name)?;
                let token: &[u8; 32] = token
                    .as_slice()
                    .try_into()
                    .map_err(|_| anyhow::anyhow!("Invalid OS service token"))?;
                principals.push(yoke::federation_http::Principal::new(
                    &principal.id,
                    token,
                    &principal.roles,
                    &principal.import_ids,
                    &principal.export_ids,
                )?);
            }
            let config = yoke::federation_http::Config {
                bind: host.bind,
                tls,
                allow_remote_bind,
                server_names: host.server_names,
                request_timeout_ms: host.request_timeout_ms,
                max_body_bytes: host.max_body_bytes,
                max_response_bytes: host.max_response_bytes,
                imports,
                exports,
            };
            let handle = yoke::federation_http::serve(&engine.root, config, principals)?;
            print(
                &json!({"status":"listening","address":handle.local_addr().to_string(),"grantAuthority":"explicit_host_profile","secrets":"os_credential_store_and_ram_only"}),
            )?;
            let started = std::time::Instant::now();
            loop {
                if duration_ms > 0 && started.elapsed().as_millis() >= duration_ms as u128 {
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(100));
            }
            handle.shutdown()?;
            return Ok(true);
        }
        Operation::Runtime {
            entry,
            python,
            requests,
            trust_runtime,
            allow_evaluation,
            max_output_bytes,
        } => {
            ensure!(trust_runtime, "Runtime execution requires --trust-runtime");
            let requests: Vec<yoke::runtime::Request> =
                yoke::json::decode(input_json(&requests, 1_000_000)?)?;
            ensure!(
                requests.len() <= 100,
                "Runtime batch contains too many commands"
            );
            let (snapshot, _) = engine.select(&arguments, &deadline)?;
            let spec = yoke::runtime::RuntimeSpec {
                python,
                trust_runtime,
                allow_evaluation,
                max_output_bytes,
            };
            let mut session =
                yoke::runtime::Session::start(&engine.store, &snapshot, &entry, &spec, &deadline)?;
            let mut events = vec![session.state().clone()];
            for request in requests {
                events.push(session.request(request)?);
            }
            drop(session);
            let completed = !events.iter().any(|event| {
                event["operationCompleted"] == false
                    || (event["status"] == "finished" && event["programCompleted"] == false)
            });
            let data = json!({"snapshotId":snapshot.id,"mode":"persistent_debugger_session","sourceExecuted":true,"hostWroteOriginalSource":false,"programHostEffects":"not_monitored","events":events,"executionAttestation":false});
            print_bounded(&engine, &data, cli.max_bytes)?;
            return Ok(completed);
        }
        Operation::Dataflow { policy } => {
            let policy = policy
                .map(|path| input_json(&path, 64_000).and_then(yoke::json::decode))
                .transpose()?
                .unwrap_or_default();
            let (snapshot, graph) = engine.select(&arguments, &deadline)?;
            let data =
                yoke::dataflow::analyze(&engine.store, &snapshot, &graph, policy, &deadline)?;
            print_bounded(&engine, &data, cli.max_bytes)?;
            return Ok(true);
        }
        Operation::Compiler {
            node,
            typescript,
            trust_compiler,
            build,
        } => {
            ensure!(
                trust_compiler,
                "Compiler execution requires --trust-compiler"
            );
            let (snapshot, _) = engine.select(&arguments, &deadline)?;
            let spec = yoke::projects::CompilerSpec {
                node,
                typescript,
                trusted: trust_compiler,
            };
            let data = if build {
                yoke::projects::build(&engine.store, &snapshot, &spec, &deadline)?
            } else {
                yoke::projects::analyze(&engine.store, &snapshot, &spec, &deadline)?
            };
            let passed = data["status"] == "passed";
            print_bounded(&engine, &data, cli.max_bytes)?;
            return Ok(passed);
        }
        Operation::TestMap { command } => {
            let (snapshot, _) = engine.select(&arguments, &deadline)?;
            let data = match command {
                TestMapOperation::Import { input } => yoke::test_map::import(
                    &engine.store,
                    &snapshot,
                    input_json(&input, 24_000_000)?,
                    &deadline,
                )?,
                TestMapOperation::Collect {
                    selections,
                    node,
                    trust_runtime,
                } => {
                    ensure!(trust_runtime, "Test execution requires --trust-runtime");
                    let selected: Vec<yoke::test_map::Selection> =
                        yoke::json::decode(input_json(&selections, 1_000_000)?)?;
                    yoke::test_map::collect_node(
                        &engine.store,
                        &snapshot,
                        &selected,
                        &node,
                        trust_runtime,
                        &deadline,
                    )?
                }
                TestMapOperation::Select { base, tests } => yoke::test_map::select(
                    &engine.store,
                    &engine.store.snapshot(&base)?,
                    &snapshot,
                    &tests,
                    &deadline,
                )?,
            };
            let passed = data["status"] != "failed"
                && data["jobs"]
                    .as_array()
                    .is_none_or(|jobs| jobs.iter().all(|job| job["passed"] == true));
            print_bounded(&engine, &data, cli.max_bytes)?;
            return Ok(passed);
        }
        Operation::Ledger { task } => {
            let data = serde_json::to_value(
                yoke::ledger::Ledger::new(&engine.store, &deadline).total(&task)?,
            )?;
            print_bounded(&engine, &data, cli.max_bytes)?;
            return Ok(true);
        }
        Operation::Models {
            config,
            trust_provider,
            command,
        } => {
            ensure!(trust_provider, "Model execution requires --trust-provider");
            let config: HostProvider = yoke::json::decode(input_json(&config, 64_000)?)?;
            let provider = yoke::providers::ProviderSpec {
                trusted: trust_provider,
                command: config.command,
                args: config.args,
                model_id: config.model_id,
                model_version: config.model_version,
                task_id: config.task_id,
                currency: config.currency,
                max_cost_micros: config.max_cost_micros,
                task_limit_micros: config.task_limit_micros,
            };
            let data = match command {
                ModelOperation::Media { path } => {
                    yoke::providers::describe_media(&engine, &path, &provider, &deadline)?
                }
                other => {
                    let (snapshot, graph) = engine.select(&arguments, &deadline)?;
                    match other {
                        ModelOperation::Embeddings => yoke::providers::generate_embeddings(
                            &engine.store,
                            &snapshot,
                            &graph,
                            &provider,
                            &deadline,
                        )?,
                        ModelOperation::Search { query, limit } => {
                            yoke::providers::semantic_search(
                                &engine.store,
                                &snapshot,
                                &graph,
                                &query,
                                limit,
                                &provider,
                                &deadline,
                            )?
                        }
                        ModelOperation::Rerank { query, entities } => yoke::providers::rerank(
                            &engine.store,
                            &snapshot,
                            &graph,
                            &query,
                            &entities,
                            &provider,
                            &deadline,
                        )?,
                        ModelOperation::Summarize { entities } => {
                            yoke::providers::summarize_sources(
                                &engine.store,
                                &snapshot,
                                &graph,
                                &entities,
                                &provider,
                                &deadline,
                            )?
                        }
                        ModelOperation::Media { .. } => unreachable!(),
                    }
                }
            };
            print_bounded(&engine, &data, cli.max_bytes)?;
            return Ok(true);
        }
        Operation::Retention { command } => {
            let data = match command {
                RetentionOperation::Plan {
                    minimum_age_ms,
                    expires_after_ms,
                } => yoke::retention::plan(
                    &engine.store,
                    &yoke::retention::Policy {
                        minimum_age_ms,
                        expires_after_ms,
                    },
                    &deadline,
                )?,
                RetentionOperation::Apply {
                    plan,
                    confirmed_hash,
                    allow_state_deletion,
                } => yoke::retention::apply(
                    &engine.store,
                    &plan,
                    &confirmed_hash,
                    allow_state_deletion,
                    &deadline,
                )?,
                RetentionOperation::Recover => yoke::retention::recover(&engine.store, &deadline)?,
                RetentionOperation::PurgePlan { minimum_age_ms } => {
                    yoke::retention::purge_plan(&engine.store, minimum_age_ms, &deadline)?
                }
                RetentionOperation::Purge {
                    plan,
                    confirmed_hash,
                    allow_state_deletion,
                } => yoke::retention::purge(
                    &engine.store,
                    &plan,
                    &confirmed_hash,
                    allow_state_deletion,
                    &deadline,
                )?,
            };
            print_bounded(&engine, &data, cli.max_bytes)?;
            return Ok(true);
        }
        Operation::VerifyContainer {
            plan,
            profile,
            trust_project,
        } => {
            ensure!(
                trust_project,
                "Container execution requires --trust-project"
            );
            let profile: HostContainer = yoke::json::decode(input_json(&profile, 64_000)?)?;
            ensure!(
                profile.isolation == "linux",
                "Container profile requires explicit linux isolation"
            );
            let spec = yoke::container::DockerSpec {
                docker_command: profile.docker_command,
                daemon_endpoint: profile.daemon_endpoint,
                docker_args: profile.docker_args,
                image: profile.image,
                command: profile.command,
                isolation: yoke::container::Isolation::Linux,
                user: profile.user,
                memory_mb: profile.memory_mb,
                cpus: profile.cpus,
                pids: profile.pids,
                tmpfs_mb: profile.tmpfs_mb,
                test_count_pattern: profile.test_count_pattern,
            };
            let data = yoke::container::execute_container(
                &engine,
                &plan,
                &spec,
                trust_project,
                &deadline,
            )?;
            let passed = data["status"] == "passed";
            print_bounded(&engine, &data, cli.max_bytes)?;
            return Ok(passed);
        }
        Operation::Maintenance { command } => {
            let data = match command {
                MaintenanceOperation::Collect {
                    apply,
                    minimum_age_ms,
                    purge_quarantine,
                } => yoke::maintenance::collect(
                    &engine.store,
                    &yoke::maintenance::Policy {
                        apply,
                        minimum_age_ms,
                        purge_quarantine,
                    },
                    &deadline,
                )?,
                MaintenanceOperation::Pin {
                    snapshot,
                    id,
                    reason,
                } => {
                    yoke::maintenance::pin(&engine.store, &snapshot, &id, &reason, &deadline)?;
                    json!({"pinned":id,"snapshotId":snapshot})
                }
                MaintenanceOperation::Unpin { id } => {
                    yoke::maintenance::unpin(&engine.store, &id, &deadline)?;
                    json!({"unpinned":id})
                }
            };
            print_bounded(&engine, &data, cli.max_bytes)?;
            return Ok(true);
        }
        Operation::Federation { command } => {
            let data = match command {
                FederationOperation::List => yoke::federation::list(&engine.store, &deadline)?,
                FederationOperation::Import {
                    input,
                    grant,
                    trust_grant,
                } => yoke::federation::import_snapshot(
                    &engine,
                    input_json(&input, 32_000_000)?,
                    &host_grant(&grant, trust_grant)?,
                    &deadline,
                )?,
                FederationOperation::Read {
                    remote_id,
                    path,
                    grant,
                    trust_grant,
                } => yoke::federation::read(
                    &engine,
                    &remote_id,
                    &path,
                    &host_grant(&grant, trust_grant)?,
                    &deadline,
                )?,
                FederationOperation::Find {
                    remote_id,
                    query,
                    kind,
                    limit,
                    grant,
                    trust_grant,
                } => {
                    ensure!(
                        (1..=1000).contains(&limit),
                        "Federation search limit must be 1..1000"
                    );
                    let grant = host_grant(&grant, trust_grant)?;
                    let snapshot =
                        yoke::federation::snapshot(&engine, &remote_id, &grant, &deadline)?;
                    let graph = yoke::federation::graph(&engine, &remote_id, &grant, &deadline)?;
                    yoke::query::require_kind(&kind)?;
                    let items = yoke::query::find(
                        &engine.store,
                        &snapshot,
                        &graph,
                        &json!({"query":query,"kind":kind,"limit":limit}),
                        &deadline,
                    )?;
                    json!({"remoteId":remote_id,"namespace":"federated","scope":grant.scope,"data":items,"trust":"signed_source_not_execution_authority"})
                }
                FederationOperation::Export {
                    scope,
                    key_name,
                    output,
                } => {
                    let scope: yoke::federation::ExportScope =
                        yoke::json::decode(input_json(&scope, 64000)?)?;
                    let key = zeroize::Zeroizing::new(yoke::credentials::read(&key_name)?);
                    let (snapshot, _) = engine.select(&arguments, &deadline)?;
                    let envelope = yoke::federation::export_snapshot(
                        &engine.store,
                        &snapshot,
                        &scope,
                        &key,
                        &deadline,
                    )?;
                    let bytes = serde_json::to_vec(&envelope)?;
                    deadline.check()?;
                    let parent = output
                        .parent()
                        .filter(|path| !path.as_os_str().is_empty())
                        .unwrap_or_else(|| std::path::Path::new("."));
                    let mut file = tempfile::NamedTempFile::new_in(parent)?;
                    file.write_all(&bytes)?;
                    file.as_file().sync_all()?;
                    deadline.check()?;
                    file.persist_noclobber(&output)?;
                    json!({"output":output,"bytes":bytes.len(),"sha256":yoke::model::hash(&bytes),"snapshotId":snapshot.id,"scope":scope.scope,"privateKeyExported":false})
                }
                FederationOperation::Keygen { .. } => bail!("Key generation dispatch error"),
            };
            print_bounded(&engine, &data, cli.max_bytes)?;
            return Ok(true);
        }
        Operation::Media { command } => {
            let data = match command {
                MediaOperation::Inspect { path } => yoke::media::inspect_with_worker(
                    &engine,
                    &path,
                    &std::env::current_exe()?,
                    &deadline,
                )?,
                MediaOperation::Transcript { path, input } => yoke::media::ingest_transcript(
                    &engine,
                    &path,
                    input_json(&input, 4_000_000)?,
                    &deadline,
                )?,
            };
            print(&data)?;
            return Ok(true);
        }
        Operation::Editor { port, max_requests } => {
            ensure!(
                arguments.get("snapshotId").is_none(),
                "Editor bridge captures saved/unsaved source; do not choose an explicit snapshot"
            );
            engine.allow_writes = false;
            yoke::editor_http::serve(
                &mut engine,
                yoke::editor_http::Config {
                    port,
                    request_timeout_ms: cli.timeout.min(15000),
                    max_requests,
                },
                io::stdout().lock(),
            )?;
            return Ok(true);
        }
        Operation::Remember { input } => {
            let (snapshot, _) = engine.select(&arguments, &deadline)?;
            let data = yoke::knowledge::remember(
                &engine.store,
                &snapshot,
                input_json(&input, 1_000_000)?,
                &deadline,
            )?;
            print(&data)?;
            return Ok(true);
        }
        Operation::Recall { query, limit } => {
            let (snapshot, _) = engine.select(&arguments, &deadline)?;
            let data = yoke::knowledge::recall(&engine.store, &snapshot, &query, limit, &deadline)?;
            print(&data)?;
            return Ok(true);
        }
        Operation::Vectors { command } => {
            let (snapshot, graph) = engine.select(&arguments, &deadline)?;
            let data = match command {
                VectorOperation::Build => yoke::knowledge::build_local_vectors(
                    &engine.store,
                    &snapshot,
                    &graph,
                    &deadline,
                )?,
                VectorOperation::Import { input } => yoke::knowledge::import_vectors(
                    &engine.store,
                    &snapshot,
                    &graph,
                    input_json(&input, 24_000_000)?,
                    &deadline,
                )?,
                VectorOperation::Search {
                    model,
                    query,
                    input,
                    limit,
                } => {
                    ensure!(
                        query.is_some() != input.is_some(),
                        "Choose a lexical query or vector input file"
                    );
                    if let Some(query) = query {
                        yoke::knowledge::search_local_vectors(
                            &engine.store,
                            &snapshot,
                            &graph,
                            &model,
                            &query,
                            limit,
                            &deadline,
                        )?
                    } else {
                        let input = input_json(&input.unwrap(), 1_000_000)?;
                        let vector = input
                            .as_array()
                            .ok_or_else(|| anyhow::anyhow!("Expected a query vector array"))?
                            .iter()
                            .map(|value| {
                                value.as_f64().ok_or_else(|| {
                                    anyhow::anyhow!("Query vector must contain finite numbers")
                                })
                            })
                            .collect::<Result<Vec<_>>>()?;
                        yoke::knowledge::search_vectors(
                            &engine.store,
                            &snapshot,
                            &graph,
                            &model,
                            &vector,
                            limit,
                            &deadline,
                        )?
                    }
                }
            };
            print(&data)?;
            return Ok(true);
        }
        Operation::Import { format, input } => {
            let (snapshot, _) = engine.select(&arguments, &deadline)?;
            let input = input_json(&input, 24_000_000)?;
            let data = match format.as_str() {
                "lcov" => yoke::knowledge::import_lcov(&engine.store, &snapshot, input, &deadline)?,
                "runtime" => {
                    yoke::knowledge::import_runtime(&engine.store, &snapshot, input, &deadline)?
                }
                "sarif" => {
                    yoke::knowledge::import_sarif(&engine.store, &snapshot, input, &deadline)?
                }
                _ => bail!("Import format must be lcov, runtime or sarif"),
            };
            print(&data)?;
            return Ok(true);
        }
        Operation::Serve => {
            yoke::protocol::serve(&mut engine, io::stdin().lock(), io::stdout().lock())?;
            return Ok(true);
        }
        Operation::Index => {
            arguments["mode"] = json!("index");
            "ci_status"
        }
        Operation::Status => "ci_status",
        Operation::Analyze => {
            arguments["mode"] = json!("analyze");
            "ci_status"
        }
        Operation::Export { format, output } => {
            let (snapshot, graph) = engine.select(&arguments, &deadline)?;
            let output = output.unwrap_or_else(|| {
                engine
                    .store
                    .dir
                    .join("exports")
                    .join(format!("{}.{}", snapshot.id, format))
            });
            yoke::export::write(&graph, &format, &output)?;
            print(
                &json!({"snapshotId":snapshot.id,"format":format,"output":output,"entities":graph.entities.len(),"observations":graph.observations.len()}),
            )?;
            return Ok(true);
        }
        Operation::Find {
            query,
            kind,
            path,
            limit,
            regex,
        } => {
            arguments["query"] = json!(query);
            arguments["kind"] = json!(kind);
            arguments["limit"] = json!(limit);
            arguments["regex"] = json!(regex);
            if let Some(path) = path {
                arguments["path"] = json!(path);
            }
            "ci_find"
        }
        Operation::Read {
            path,
            start_byte,
            end_byte,
            start_line,
            end_line,
        } => {
            arguments["path"] = json!(path);
            for (name, value) in [
                ("startByte", start_byte),
                ("endByte", end_byte),
                ("startLine", start_line),
                ("endLine", end_line),
            ] {
                if let Some(value) = value {
                    arguments[name] = json!(value);
                }
            }
            "ci_read"
        }
        Operation::Outline { path } => {
            arguments["path"] = json!(path);
            arguments["kind"] = json!("all");
            arguments["limit"] = json!(200);
            "ci_find"
        }
        Operation::Context { query, limit } => {
            arguments["query"] = json!(query);
            arguments["limit"] = json!(limit);
            "ci_context"
        }
        Operation::Trace {
            entity,
            direction,
            depth,
            relation,
        } => {
            let (s, g) = engine.select(&arguments, &deadline)?;
            arguments["snapshotId"] = json!(s.id);
            arguments["entityIds"] = json!([entity_id(&g, &entity)?]);
            arguments["direction"] = json!(direction);
            arguments["depth"] = json!(depth);
            if !relation.is_empty() {
                arguments["relations"] = json!(relation);
            }
            "ci_trace"
        }
        Operation::Path {
            from,
            to,
            direction,
        } => {
            ensure!(
                ["out", "both"].contains(&direction.as_str()),
                "Path direction must be out or both"
            );
            let (_, g) = engine.select(&arguments, &deadline)?;
            let result = yoke::query::shortest_path(
                &g,
                &entity_id(&g, &from)?,
                &entity_id(&g, &to)?,
                &direction,
                10000,
                &deadline,
            )?;
            print(&result)?;
            return Ok(true);
        }
        Operation::Invoke { tool, args } => {
            let value: Value = serde_json::from_str(&args)?;
            let output = engine.invoke(&tool, value)?;
            print(&output)?;
            return Ok(output["status"] != "error");
        }
        Operation::Prepare { input } => {
            let bytes = fs::read(input)?;
            ensure!(bytes.len() <= 4000000, "Patch input too large");
            let edits: Value = serde_json::from_slice(&bytes)?;
            if edits.is_array() {
                arguments["edits"] = edits;
            } else {
                for (key, value) in edits
                    .as_object()
                    .ok_or_else(|| anyhow::anyhow!("Expected edits array or operation object"))?
                {
                    arguments[key] = value.clone();
                }
            }
            "ci_change_prepare"
        }
        Operation::Approve {
            plan,
            allow_test_changes,
        } => {
            let plan = engine.approve(&plan, allow_test_changes)?;
            print(&json!({"approvedPlanId":plan.id,"expiresAt":plan.expires_at}))?;
            return Ok(true);
        }
        Operation::Apply { plan, key, mode } => {
            arguments["planId"] = json!(plan);
            arguments["idempotencyKey"] = json!(key);
            arguments["mode"] = json!(mode);
            "ci_change_apply"
        }
        Operation::Verify {
            plan,
            command,
            args,
            trust_project,
        } => {
            if let Some(command) = command {
                let result = yoke::verify::execute(
                    &engine,
                    &plan,
                    &command,
                    &args,
                    trust_project,
                    &deadline,
                )?;
                print(&result)?;
                return Ok(result["status"] == "passed");
            }
            arguments["planId"] = json!(plan);
            "ci_verify"
        }
        Operation::Recover { journal } => {
            let result =
                yoke::changes::recover(&engine.store, &engine.root, &journal, cli.allow_writes)?;
            print(&serde_json::to_value(result)?)?;
            return Ok(true);
        }
        Operation::Lsp {
            command,
            args,
            language,
            path,
            offset,
            operation,
            new_name,
            trust_server,
        } => {
            let server = yoke::lsp::Server {
                command,
                args,
                language,
                trust: trust_server,
            };
            let (snapshot, graph) = engine.select(&arguments, &deadline)?;
            let result = yoke::lsp::run_at(
                &mut engine,
                &server,
                &snapshot,
                &graph,
                yoke::lsp::SemanticRequest {
                    path: &path,
                    offset,
                    operation: &operation,
                    new_name: new_name.as_deref(),
                },
                &deadline,
            )?;
            print(&result)?;
            return Ok(true);
        }
        Operation::Watch {
            interval,
            iterations,
        } => {
            ensure!(
                (100..=60000).contains(&interval),
                "Watch interval must be 100..60000ms"
            );
            let mut latest = String::new();
            let mut count = 0;
            loop {
                let (s, g, p, r) = engine.index(&Deadline::new(cli.timeout))?;
                if s.id != latest {
                    print(
                        &json!({"snapshotId":s.id,"entities":g.entities.len(),"parsedFiles":p,"reusedFiles":r}),
                    )?;
                    latest = s.id;
                }
                count += 1;
                if iterations > 0 && count >= iterations {
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(interval));
            }
            return Ok(true);
        }
        Operation::Position { .. }
        | Operation::MediaPdfWorker { .. }
        | Operation::ModelWorker { .. }
        | Operation::Credentials { .. } => {
            bail!("Internal dispatch error")
        }
    };
    let result = engine.invoke(tool, arguments)?;
    let success = result["status"] != "error";
    print(&result)?;
    Ok(success)
}
fn main() {
    match run() {
        Ok(true) => {}
        Ok(false) => std::process::exit(1),
        Err(error) => {
            eprintln!("yoke: {error:#}");
            std::process::exit(1);
        }
    }
}
