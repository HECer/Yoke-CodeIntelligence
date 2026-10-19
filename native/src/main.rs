use anyhow::{Context, Result, bail, ensure};
use clap::{Parser, Subcommand};
use serde_json::{Value, json};
use std::{
    fs,
    io::{self, IsTerminal, Read, Write},
    path::PathBuf,
};
use yoke::{Engine, capture::Deadline};
mod bedrock_cli;
mod digest_history_capture_cli;
mod digest_history_job_cli;
mod digest_inbox_cli;
mod digest_receiver_cli;
mod feedback_cli;
mod git_affected_cli;
mod git_areas_cli;
mod git_conflicts_cli;
mod git_impact_cli;
mod github_app_cli;
mod github_app_digest_cli;
mod github_app_ingress_cli;
mod github_app_pages_cli;
mod github_app_publication_cli;
mod github_app_worker_cli;
mod github_discussions_cli;
mod host_install_cli;
mod meaning_archive_cli;
mod meaning_cli;
mod mixed_view_cli;
mod view_comparison_cli;
struct PreparedModelProvider {
    provider: yoke::providers::ProviderSpec,
    embedding_batch: Option<yoke::provider_batches::EmbeddingBatchPolicy>,
}

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
    /// Analyzer for new saved-source captures; the default remains epoch 8.
    #[arg(long, global = true, value_parser = ["8", "9"])]
    analyzer_epoch: Option<String>,
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
    #[arg(long, global = true)]
    capture_config: Option<PathBuf>,
    #[arg(long, global = true)]
    include_ignored_dir: Vec<String>,
    #[command(subcommand)]
    command: ParsedOperation,
}
#[derive(Subcommand)]
enum ExtensionOperation {
    ServerCatalog,
    Session {
        #[arg(long)]
        id: Option<String>,
        #[arg(long)]
        host_profile: Option<PathBuf>,
        #[arg(long)]
        requests: PathBuf,
    },
    ServeSession(Box<ServeSessionArguments>),
    ServeHttp(Box<HttpServeArguments>),
    AgentRepl(Box<AgentReplArguments>),
    Git(Box<GitArguments>),
    Document(Box<DocumentArguments>),
    Skills(Box<SkillArguments>),
    Decisions(Box<DecisionArguments>),
    HostInstall(Box<host_install_cli::Arguments>),
    MeaningArchive(Box<meaning_archive_cli::Arguments>),
    Meaning(Box<meaning_cli::Arguments>),
    MixedView(Box<mixed_view_cli::Arguments>),
    Feedback(Box<feedback_cli::Arguments>),
    GithubApp(Box<github_app_cli::Arguments>),
    GithubDiscussions(Box<github_discussions_cli::Arguments>),
    DigestReceiver(Box<digest_receiver_cli::Arguments>),
    DigestInbox(Box<digest_inbox_cli::Arguments>),
    DigestHistoryCapture(Box<digest_history_capture_cli::Arguments>),
    DigestHistoryJob(Box<digest_history_job_cli::Arguments>),
    ModelsHttp(Box<HttpModelArguments>),
    ModelHttpWorker(Box<HttpModelWorkerArguments>),
    ModelsBedrock(Box<bedrock_cli::Arguments>),
    #[command(hide = true)]
    ModelBedrockWorker(Box<bedrock_cli::WorkerArguments>),
    ModelBatchStatus {
        id: String,
    },
    Collections(Box<CollectionArguments>),
    Graph {
        #[command(subcommand)]
        command: GraphOperation,
    },
    Notes {
        operation: String,
        #[arg(long, default_value = "{}")]
        args: String,
        #[arg(long)]
        global_root: Option<PathBuf>,
        #[arg(long)]
        read_only: bool,
        #[arg(long)]
        read_only_pattern: Vec<String>,
        #[arg(long)]
        ignored_pattern: Vec<String>,
    },
    Freshness,
    Retrieve {
        query: String,
        #[arg(long)]
        options: Option<PathBuf>,
    },
}
enum Operation {
    Index,
    Extensions(Box<ExtensionOperation>),
    Credentials {
        command: CredentialOperation,
    },
    Runtime {
        entry: String,
        python: PathBuf,
        requests: PathBuf,
        trust_runtime: bool,
        allow_evaluation: bool,
        max_output_bytes: usize,
    },
    FederationService {
        profile: PathBuf,
        trust_service: bool,
        allow_remote_bind: bool,
        duration_ms: u64,
    },
    FederationRequest {
        profile: PathBuf,
        input: PathBuf,
        trust_transport: bool,
        allow_plaintext_loopback: bool,
    },
    Dataflow {
        policy: Option<PathBuf>,
    },
    Compiler {
        node: PathBuf,
        typescript: PathBuf,
        trust_compiler: bool,
        build: bool,
    },
    TestMap {
        command: TestMapOperation,
    },
    Models {
        config: PathBuf,
        trust_provider: bool,
        command: ModelOperation,
    },
    Ledger {
        task: String,
    },
    Retention {
        command: RetentionOperation,
    },
    VerifyContainer {
        plan: String,
        profile: PathBuf,
        trust_project: bool,
    },
    Status,
    Analyze,
    Remember {
        input: PathBuf,
    },
    Recall {
        query: String,
        limit: usize,
    },
    Vectors {
        command: VectorOperation,
    },
    Import {
        format: String,
        input: PathBuf,
    },
    Media {
        command: MediaOperation,
    },
    Federation {
        command: FederationOperation,
    },
    Maintenance {
        command: MaintenanceOperation,
    },
    ModelWorker {
        endpoint: String,
        model: String,
        digest: String,
    },
    MediaPdfWorker {
        input: PathBuf,
    },
    Editor {
        port: u16,
        max_requests: Option<usize>,
    },
    Export {
        format: String,
        output: Option<PathBuf>,
    },
    Find {
        query: String,
        kind: String,
        path: Option<String>,
        limit: u64,
        regex: bool,
    },
    Read {
        path: String,
        start_byte: Option<u64>,
        end_byte: Option<u64>,
        start_line: Option<u64>,
        end_line: Option<u64>,
    },
    Outline {
        path: String,
    },
    Trace {
        entity: String,
        direction: String,
        depth: u64,
        relation: Vec<String>,
    },
    Context {
        query: String,
        limit: u64,
    },
    Path {
        from: String,
        to: String,
        direction: String,
    },
    Invoke {
        tool: String,
        args: String,
    },
    Serve,
    Prepare {
        input: PathBuf,
    },
    Approve {
        plan: String,
        allow_test_changes: bool,
    },
    Apply {
        plan: String,
        key: String,
        mode: String,
    },
    Verify {
        plan: String,
        command: Option<String>,
        args: Vec<String>,
        trust_project: bool,
    },
    Recover {
        journal: String,
    },
    Lsp {
        command: String,
        args: Vec<String>,
        language: String,
        path: String,
        offset: usize,
        operation: String,
        new_name: Option<String>,
        trust_server: bool,
    },
    Position {
        text: String,
        byte: Option<usize>,
        line: Option<usize>,
        character: Option<usize>,
    },
    Watch {
        interval: u64,
        iterations: u64,
    },
}

#[derive(Subcommand)]
enum ParsedOperation {
    #[command(flatten)]
    Group0(Box<ParsedGroup0>),
    #[command(flatten)]
    Group1(Box<ParsedGroup1>),
    #[command(flatten)]
    Group2(Box<ParsedGroup2>),
    #[command(flatten)]
    Group3(Box<ParsedGroup3>),
    #[command(flatten)]
    Group4(Box<ParsedGroup4>),
    #[command(flatten)]
    Group5(Box<ParsedGroup5>),
    #[command(flatten)]
    Group6(Box<ParsedGroup6>),
}

#[derive(Subcommand)]
enum ParsedGroup0 {
    Index,
    #[command(flatten)]
    Extensions(Box<ExtensionOperation>),
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
}

#[derive(Subcommand)]
enum ParsedGroup1 {
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
}

#[derive(Subcommand)]
enum ParsedGroup2 {
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
}

#[derive(Subcommand)]
enum ParsedGroup3 {
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
}

#[derive(Subcommand)]
enum ParsedGroup4 {
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
}

#[derive(Subcommand)]
enum ParsedGroup5 {
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
}

#[derive(Subcommand)]
enum ParsedGroup6 {
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

impl From<ParsedOperation> for Operation {
    fn from(parsed: ParsedOperation) -> Self {
        match parsed {
            ParsedOperation::Group0(group) => match *group {
                ParsedGroup0::Index => Operation::Index,
                ParsedGroup0::Extensions(value) => Operation::Extensions(value),
                ParsedGroup0::Credentials { command } => Operation::Credentials { command },
                ParsedGroup0::Runtime {
                    entry,
                    python,
                    requests,
                    trust_runtime,
                    allow_evaluation,
                    max_output_bytes,
                } => Operation::Runtime {
                    entry,
                    python,
                    requests,
                    trust_runtime,
                    allow_evaluation,
                    max_output_bytes,
                },
                ParsedGroup0::FederationService {
                    profile,
                    trust_service,
                    allow_remote_bind,
                    duration_ms,
                } => Operation::FederationService {
                    profile,
                    trust_service,
                    allow_remote_bind,
                    duration_ms,
                },
                ParsedGroup0::FederationRequest {
                    profile,
                    input,
                    trust_transport,
                    allow_plaintext_loopback,
                } => Operation::FederationRequest {
                    profile,
                    input,
                    trust_transport,
                    allow_plaintext_loopback,
                },
            },
            ParsedOperation::Group1(group) => match *group {
                ParsedGroup1::Dataflow { policy } => Operation::Dataflow { policy },
                ParsedGroup1::Compiler {
                    node,
                    typescript,
                    trust_compiler,
                    build,
                } => Operation::Compiler {
                    node,
                    typescript,
                    trust_compiler,
                    build,
                },
                ParsedGroup1::TestMap { command } => Operation::TestMap { command },
                ParsedGroup1::Models {
                    config,
                    trust_provider,
                    command,
                } => Operation::Models {
                    config,
                    trust_provider,
                    command,
                },
                ParsedGroup1::Ledger { task } => Operation::Ledger { task },
                ParsedGroup1::Retention { command } => Operation::Retention { command },
            },
            ParsedOperation::Group2(group) => match *group {
                ParsedGroup2::VerifyContainer {
                    plan,
                    profile,
                    trust_project,
                } => Operation::VerifyContainer {
                    plan,
                    profile,
                    trust_project,
                },
                ParsedGroup2::Status => Operation::Status,
                ParsedGroup2::Analyze => Operation::Analyze,
                ParsedGroup2::Remember { input } => Operation::Remember { input },
                ParsedGroup2::Recall { query, limit } => Operation::Recall { query, limit },
                ParsedGroup2::Vectors { command } => Operation::Vectors { command },
            },
            ParsedOperation::Group3(group) => match *group {
                ParsedGroup3::Import { format, input } => Operation::Import { format, input },
                ParsedGroup3::Media { command } => Operation::Media { command },
                ParsedGroup3::Federation { command } => Operation::Federation { command },
                ParsedGroup3::Maintenance { command } => Operation::Maintenance { command },
                ParsedGroup3::ModelWorker {
                    endpoint,
                    model,
                    digest,
                } => Operation::ModelWorker {
                    endpoint,
                    model,
                    digest,
                },
                ParsedGroup3::MediaPdfWorker { input } => Operation::MediaPdfWorker { input },
            },
            ParsedOperation::Group4(group) => match *group {
                ParsedGroup4::Editor { port, max_requests } => {
                    Operation::Editor { port, max_requests }
                }
                ParsedGroup4::Export { format, output } => Operation::Export { format, output },
                ParsedGroup4::Find {
                    query,
                    kind,
                    path,
                    limit,
                    regex,
                } => Operation::Find {
                    query,
                    kind,
                    path,
                    limit,
                    regex,
                },
                ParsedGroup4::Read {
                    path,
                    start_byte,
                    end_byte,
                    start_line,
                    end_line,
                } => Operation::Read {
                    path,
                    start_byte,
                    end_byte,
                    start_line,
                    end_line,
                },
                ParsedGroup4::Outline { path } => Operation::Outline { path },
                ParsedGroup4::Trace {
                    entity,
                    direction,
                    depth,
                    relation,
                } => Operation::Trace {
                    entity,
                    direction,
                    depth,
                    relation,
                },
            },
            ParsedOperation::Group5(group) => match *group {
                ParsedGroup5::Context { query, limit } => Operation::Context { query, limit },
                ParsedGroup5::Path {
                    from,
                    to,
                    direction,
                } => Operation::Path {
                    from,
                    to,
                    direction,
                },
                ParsedGroup5::Invoke { tool, args } => Operation::Invoke { tool, args },
                ParsedGroup5::Serve => Operation::Serve,
                ParsedGroup5::Prepare { input } => Operation::Prepare { input },
                ParsedGroup5::Approve {
                    plan,
                    allow_test_changes,
                } => Operation::Approve {
                    plan,
                    allow_test_changes,
                },
            },
            ParsedOperation::Group6(group) => match *group {
                ParsedGroup6::Apply { plan, key, mode } => Operation::Apply { plan, key, mode },
                ParsedGroup6::Verify {
                    plan,
                    command,
                    args,
                    trust_project,
                } => Operation::Verify {
                    plan,
                    command,
                    args,
                    trust_project,
                },
                ParsedGroup6::Recover { journal } => Operation::Recover { journal },
                ParsedGroup6::Lsp {
                    command,
                    args,
                    language,
                    path,
                    offset,
                    operation,
                    new_name,
                    trust_server,
                } => Operation::Lsp {
                    command,
                    args,
                    language,
                    path,
                    offset,
                    operation,
                    new_name,
                    trust_server,
                },
                ParsedGroup6::Position {
                    text,
                    byte,
                    line,
                    character,
                } => Operation::Position {
                    text,
                    byte,
                    line,
                    character,
                },
                ParsedGroup6::Watch {
                    interval,
                    iterations,
                } => Operation::Watch {
                    interval,
                    iterations,
                },
            },
        }
    }
}
#[derive(Subcommand)]
enum GraphOperation {
    Build {
        #[arg(long)]
        options: Option<PathBuf>,
    },
    View {
        view: String,
        operation: String,
        #[arg(long, default_value = "{}")]
        args: String,
    },
    Analyze {
        #[arg(long)]
        options: Option<PathBuf>,
    },
    Compare {
        before: String,
        after: String,
    },
    /// Compare two explicitly pinned historical Views without capture or publication.
    CompareViews(Box<view_comparison_cli::Arguments>),
    Export {
        #[arg(long)]
        format: String,
        #[arg(long)]
        output: Option<PathBuf>,
        #[arg(long, default_value_t = 1000)]
        max_nodes: usize,
        #[arg(long, default_value_t = 10000)]
        max_edges: usize,
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
    AwsImport {
        name: String,
    },
    ModelImport {
        name: String,
    },
    GithubImport {
        name: String,
    },
    GithubWebhookImport {
        name: String,
    },
    GithubAppKeyImport {
        name: String,
    },
    Tls {
        name: String,
        #[arg(long = "hostname", required = true)]
        hostnames: Vec<String>,
    },
    Token {
        name: String,
    },
}
#[derive(clap::Args)]
struct DecisionArguments {
    #[arg(value_parser = ["write", "read", "list", "check", "export", "import"])]
    operation: String,
    #[arg(long, conflicts_with = "args")]
    input: Option<PathBuf>,
    #[arg(long, conflicts_with = "input")]
    args: Option<String>,
    #[arg(long)]
    allow_record_writes: bool,
    #[arg(long, requires = "host_id")]
    personal_root: Option<PathBuf>,
    #[arg(long, requires = "personal_root")]
    host_id: Option<String>,
    #[arg(skip)]
    prepared: Option<Value>,
}

fn decision_preflight(args: &DecisionArguments, source_root: &std::path::Path) -> Result<Value> {
    if matches!(args.operation.as_str(), "write" | "import") {
        ensure!(
            args.allow_record_writes,
            "Decision record mutation requires --allow-record-writes"
        );
    }
    match (&args.personal_root, &args.host_id) {
        (Some(root), Some(host)) => {
            ensure!(
                !host.trim().is_empty()
                    && host.len() <= 200
                    && !host.chars().any(char::is_control)
                    && !yoke::paths::obvious_secret(host),
                "Decision host ID must be a bounded single-line label"
            );
            ensure!(
                root.is_absolute(),
                "Decision personal root must be absolute"
            );
            ensure!(
                !root
                    .components()
                    .any(|part| matches!(part, std::path::Component::ParentDir)),
                "Decision personal root cannot contain parent traversal"
            );
            for ancestor in root.ancestors() {
                let meta = fs::symlink_metadata(ancestor)?;
                ensure!(
                    meta.is_dir() && !yoke::paths::is_link(&meta),
                    "Decision personal root ancestors must be ordinary existing directories"
                );
            }
            ensure!(
                fs::canonicalize(root)? != fs::canonicalize(source_root)?,
                "Decision personal storage requires a separate host directory"
            );
        }
        (None, None) => {}
        _ => bail!("Decision personal scope requires both --personal-root and --host-id"),
    }
    let input = if let Some(path) = &args.input {
        let meta = fs::symlink_metadata(path)?;
        ensure!(
            meta.is_file() && !yoke::paths::is_link(&meta),
            "Decision input requires an ordinary host file"
        );
        input_json(path, 128000)?
    } else {
        yoke::json::parse(args.args.as_deref().unwrap_or("{}").as_bytes(), 128000)?
    };
    ensure!(input.is_object(), "Decision arguments must be an object");
    Ok(input)
}

fn run_decisions(
    engine: &mut Engine,
    mut args: DecisionArguments,
    arguments: &Value,
    deadline: &Deadline,
) -> Result<bool> {
    use yoke::decision_records::{DecisionStore, Scope};
    let input = args
        .prepared
        .take()
        .ok_or_else(|| anyhow::anyhow!("Decision host preflight missing"))?;
    let (snapshot, _) = engine.select(arguments, deadline)?;
    let personal = args
        .personal_root
        .as_ref()
        .map(|root| yoke::store::Store::open_with_timeout(root, deadline.remaining()))
        .transpose()?;
    let storage = personal.as_ref().unwrap_or(&engine.store);
    storage.set_deadline(deadline)?;
    let scope = match args.host_id {
        Some(host_id) => Scope::Personal { host_id },
        None => Scope::Repository,
    };
    let access = DecisionStore::new(&engine.store, storage, scope, deadline)?;
    let value = access.invoke(&snapshot, &args.operation, &input, deadline)?;
    let bytes = yoke::json::encode_bounded(&value, 64_000_000, deadline)?;
    let max = arguments["maxBytes"]
        .as_u64()
        .ok_or_else(|| anyhow::anyhow!("Decision output budget absent"))?;
    let output = if (bytes.len() as u64).saturating_add(1) <= max {
        bytes
    } else {
        let artifact = storage.artifact(&bytes)?;
        let mut read_with = json!({"tool":"ci_read","artifactId":artifact});
        if let Some(root) = &args.personal_root {
            read_with["root"] = json!(root);
        }
        yoke::json::encode_bounded(
            &json!({
                "status":"partial", "artifactId":artifact, "totalBytes":bytes.len(),
                "readWith":read_with,
                "warning":"Complete decision result retained as an immutable artifact in the selected record store."
            }),
            usize::try_from(max.saturating_sub(1))?,
            deadline,
        )?
    };
    let mut stdout = io::stdout().lock();
    stdout.write_all(&output)?;
    stdout.write_all(b"\n")?;
    stdout.flush()?;
    Ok(true)
}

#[derive(clap::Args)]
struct CollectionArguments {
    #[arg(long)]
    registry: PathBuf,
    #[arg(long)]
    allow_registration: bool,
    /// Permit SQLite coordination sidecars while retaining a read-only database snapshot.
    #[arg(long)]
    allow_registry_coordination: bool,
    #[arg(long)]
    federation_profile: Option<PathBuf>,
    #[arg(long)]
    trust_grant: bool,
    #[command(subcommand)]
    operation: CollectionOperation,
}
#[derive(Subcommand)]
enum CollectionOperation {
    List,
    Describe {
        name: String,
    },
    Query {
        #[arg(long = "member", required = true)]
        members: Vec<String>,
        #[arg(long)]
        args: String,
    },
    Join {
        #[arg(long = "member", required = true)]
        members: Vec<String>,
    },
    Register {
        name: String,
        #[arg(long, conflicts_with = "expect_absent")]
        expected: Option<String>,
        #[arg(long, conflicts_with = "expected")]
        expect_absent: bool,
        #[arg(long)]
        view: Option<String>,
    },
    Remove {
        name: String,
        #[arg(long)]
        expected: String,
    },
    RegisterFederated {
        name: String,
        #[arg(long)]
        grant_id: String,
        #[arg(long)]
        remote_id: String,
        #[arg(long)]
        source_only: bool,
        #[arg(long, conflicts_with = "expect_absent")]
        expected: Option<String>,
        #[arg(long, conflicts_with = "expected")]
        expect_absent: bool,
    },
    ReadSource {
        member: String,
        path: String,
        #[arg(long)]
        start_byte: usize,
        #[arg(long)]
        end_byte: usize,
    },
}
#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CollectionFederationProfile {
    receiver_root: PathBuf,
    #[serde(default)]
    capture_options: yoke::capture::CaptureOptions,
    imports: Vec<HostImport>,
}

fn collection_preflight(args: &CollectionArguments) -> Result<()> {
    use yoke::graph_collections::validate_registration_name;
    ensure!(
        args.registry.is_absolute(),
        "Collection registry requires an absolute host path"
    );
    match &args.operation {
        CollectionOperation::Describe { name }
        | CollectionOperation::Register { name, .. }
        | CollectionOperation::Remove { name, .. }
        | CollectionOperation::RegisterFederated { name, .. } => validate_registration_name(name)?,
        CollectionOperation::Query { members, .. } | CollectionOperation::Join { members } => {
            ensure!(
                !members.is_empty() && members.len() <= 100,
                "Invalid collection member count"
            );
            for name in members {
                validate_registration_name(name)?;
            }
        }
        CollectionOperation::ReadSource {
            member,
            path,
            start_byte,
            end_byte,
        } => {
            validate_registration_name(member)?;
            yoke::paths::normalized_syntax(path)?;
            ensure!(
                start_byte <= end_byte && end_byte - start_byte <= 256000,
                "Invalid collection source range"
            );
        }
        CollectionOperation::List => {}
    }
    if matches!(
        args.operation,
        CollectionOperation::Register { .. }
            | CollectionOperation::Remove { .. }
            | CollectionOperation::RegisterFederated { .. }
    ) {
        ensure!(
            args.allow_registration,
            "Collection mutation requires --allow-registration"
        );
    }
    if args.federation_profile.is_some() {
        ensure!(
            args.trust_grant,
            "Collection federation profile requires --trust-grant"
        );
        ensure!(
            !matches!(
                args.operation,
                CollectionOperation::Register { .. } | CollectionOperation::Remove { .. }
            ),
            "Native registration and binding removal do not select federation grants"
        );
    } else {
        ensure!(
            !args.trust_grant,
            "Collection --trust-grant requires --federation-profile"
        );
    }
    if let CollectionOperation::RegisterFederated {
        expected,
        expect_absent,
        remote_id,
        ..
    } = &args.operation
    {
        ensure!(
            args.federation_profile.is_some(),
            "Foreign registration requires a host federation profile"
        );
        ensure!(
            expected.is_some() != *expect_absent,
            "Registration requires --expected or --expect-absent"
        );
        ensure!(
            expected.as_deref().is_none_or(yoke::model::valid_id)
                && yoke::model::valid_id(remote_id),
            "Invalid foreign collection identity"
        );
    }
    if let CollectionOperation::Register {
        expected,
        expect_absent,
        view,
        ..
    } = &args.operation
    {
        ensure!(
            expected.is_some() != *expect_absent,
            "Registration requires --expected or --expect-absent"
        );
        ensure!(
            expected.as_deref().is_none_or(yoke::model::valid_id)
                && view.as_deref().is_none_or(yoke::model::valid_id),
            "Invalid collection identity"
        );
    }
    if let CollectionOperation::Remove { expected, .. } = &args.operation {
        ensure!(
            yoke::model::valid_id(expected),
            "Invalid expected registration identity"
        );
    }
    Ok(())
}

fn collection_output(value: &Value, max: u64, deadline: &Deadline) -> Result<()> {
    let limit = usize::try_from(max.saturating_sub(1))?;
    let bytes = yoke::json::encode_bounded(value, limit, deadline)?;
    let mut output = io::stdout().lock();
    output.write_all(&bytes)?;
    output.write_all(b"\n")?;
    output.flush()?;
    Ok(())
}

fn standalone_collections(
    args: &CollectionArguments,
    deadline: &Deadline,
    max: u64,
) -> Result<bool> {
    use yoke::graph_collections::Registry;
    let context = args
        .federation_profile
        .as_ref()
        .map(|path| -> Result<_> {
            let profile: CollectionFederationProfile =
                yoke::json::decode(input_json(path, 1_000_000)?)?;
            ensure!(
                profile.receiver_root.is_absolute(),
                "Collection receiver root must be an absolute host path"
            );
            let capture = profile.capture_options.validated()?;
            ensure!(
                profile.imports.len() <= 100,
                "Too many current collection grants"
            );
            let mut grants = std::collections::BTreeMap::new();
            for entry in profile.imports {
                ensure!(
                    !entry.id.is_empty()
                        && entry.id.len() <= 100
                        && entry
                            .id
                            .bytes()
                            .all(|b| b.is_ascii_alphanumeric() || b"_.-".contains(&b)),
                    "Invalid current collection grant label"
                );
                ensure!(
                    grants.insert(entry.id, entry.grant).is_none(),
                    "Duplicate current collection grant label"
                );
            }
            let state = profile.receiver_root.join(".yoke/index.sqlite");
            let meta = fs::symlink_metadata(state)?;
            ensure!(
                meta.is_file() && !yoke::paths::is_link(&meta),
                "Collection receiver requires an existing registered source store"
            );
            let mut receiver =
                Engine::open_with_timeout(&profile.receiver_root, deadline.remaining())?;
            receiver.capture_options = capture;
            receiver.store.set_deadline(deadline)?;
            Ok((receiver, grants))
        })
        .transpose()?;
    let access = context.as_ref().map(|(receiver, grants)| {
        yoke::federation_archive::FederationArchiveAccess::new(receiver, grants)
    });
    let value = match &args.operation {
        CollectionOperation::Register { .. } => bail!("Collection source dispatch error"),
        CollectionOperation::RegisterFederated {
            name,
            grant_id,
            remote_id,
            source_only,
            expected,
            ..
        } => {
            let access = access
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!("Current host collection grants required"))?;
            let registry = Registry::open(&args.registry)?;
            let (id, kind) = if *source_only {
                (
                    registry.register_federated_sources_cas(
                        name,
                        access,
                        grant_id,
                        remote_id,
                        expected.as_deref(),
                        deadline,
                    )?,
                    "federatedSources",
                )
            } else {
                (
                    registry.register_federated_cas(
                        name,
                        access,
                        grant_id,
                        remote_id,
                        expected.as_deref(),
                        deadline,
                    )?,
                    "federatedCanonical",
                )
            };
            json!({"name":name,"registrationId":id,"kind":kind,"executionAuthority":"none"})
        }
        CollectionOperation::Remove { name, expected } => {
            // Refuse missing registries instead of creating one during removal.
            ensure!(
                args.registry.join("yoke-collection.sqlite").is_file(),
                "Collection registry does not exist"
            );
            let registry = Registry::open(&args.registry)?;
            registry.remove(name, expected, deadline)?;
            json!({"removed":name,"expectedRegistrationId":expected,"sourceBlobsDeleted":false})
        }
        other => {
            let registry = Registry::open_existing_for_host(
                &args.registry,
                args.allow_registry_coordination,
                deadline,
            )?;
            match other {
                CollectionOperation::List => {
                    if let Some(access) = &access {
                        registry.list_authorized(access, deadline)?
                    } else {
                        registry.list(deadline)?
                    }
                }
                CollectionOperation::Describe { name } => {
                    if let Some(access) = &access {
                        registry.describe_authorized(name, access, deadline)?
                    } else {
                        registry.describe(name, deadline)?
                    }
                }
                CollectionOperation::Query { members, args } => {
                    let query = yoke::json::parse(args.as_bytes(), 128000)?;
                    if let Some(access) = &access {
                        registry.query_authorized(members, &query, access, deadline)?
                    } else {
                        registry.query(members, &query, deadline)?
                    }
                }
                CollectionOperation::Join { members } => {
                    if let Some(access) = &access {
                        registry.join_authorized(members, access, deadline)?
                    } else {
                        registry.join(members, deadline)?
                    }
                }
                CollectionOperation::ReadSource {
                    member,
                    path,
                    start_byte,
                    end_byte,
                } => {
                    if let Some(access) = &access {
                        registry.read_source_authorized(
                            member,
                            path,
                            *start_byte..*end_byte,
                            access,
                            deadline,
                        )?
                    } else {
                        registry.read_source(member, path, *start_byte..*end_byte, deadline)?
                    }
                }
                _ => unreachable!(),
            }
        }
    };
    collection_output(&value, max, deadline)?;
    Ok(true)
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
#[derive(clap::Args)]
struct HttpModelArguments {
    #[arg(long)]
    config: PathBuf,
    #[arg(long)]
    trust_network: bool,
    #[arg(long)]
    allow_loopback_http: bool,
    #[command(subcommand)]
    operation: ModelOperation,
}
#[derive(clap::Args)]
struct HttpModelWorkerArguments {
    #[arg(long)]
    config: PathBuf,
    #[arg(long)]
    config_hash: String,
    #[arg(long)]
    trust_network: bool,
    #[arg(long)]
    allow_loopback_http: bool,
}
#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct HostHttpModel {
    endpoint: String,
    dialect: HttpModelDialect,
    auth: HttpModelAuth,
    json_mode: HttpModelJson,
    token_limit: HttpModelTokens,
    model_id: String,
    model_version: String,
    expected_remote_model: String,
    credential_name: String,
    max_output_tokens: u32,
    task_id: String,
    currency: String,
    max_cost_micros: u64,
    task_limit_micros: u64,
    #[serde(default)]
    capabilities: HttpModelCapabilities,
    deployment: Option<String>,
    api_version: Option<String>,
    #[serde(default)]
    root_certificates_der_base64: Vec<String>,
}
#[derive(Clone, Copy, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
enum HttpModelDialect {
    OpenAiCompatible,
    AnthropicMessages,
    AzureV1,
    AzureDeployment,
}
#[derive(Clone, Copy, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
enum HttpModelAuth {
    Bearer,
    ApiKey,
    AnthropicApiKey,
}
#[derive(Clone, Copy, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
enum HttpModelJson {
    JsonSchema,
    JsonObject,
    TextJson,
}
#[derive(Clone, Copy, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
enum HttpModelTokens {
    MaxCompletionTokens,
    MaxTokens,
}
#[derive(Default, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct HttpModelCapabilities {
    #[serde(default)]
    embeddings: bool,
    #[serde(default)]
    vision: bool,
}
fn load_http_model(
    path: &std::path::Path,
    trusted: bool,
    loopback: bool,
    expected_hash: Option<&str>,
) -> Result<(
    HostHttpModel,
    yoke::model_http::HttpProviderSpec,
    PathBuf,
    String,
)> {
    use base64::Engine as _;
    use yoke::model_http as http;
    ensure!(trusted, "HTTP model execution requires --trust-network");
    let path = fs::canonicalize(path)?;
    let mut bytes = Vec::new();
    fs::File::open(&path)?
        .take(64_001)
        .read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() <= 64_000,
        "HTTP provider profile exceeds capacity"
    );
    let hash = yoke::model::hash(&bytes);
    if let Some(expected) = expected_hash {
        ensure!(
            yoke::model::valid_id(expected) && expected == hash,
            "HTTP provider profile changed"
        );
    }
    let config: HostHttpModel = yoke::json::decode(yoke::json::parse(&bytes, 64_000)?)?;
    let dialect = match config.dialect {
        HttpModelDialect::AzureDeployment => http::Dialect::AzureDeployment {
            deployment: config
                .deployment
                .clone()
                .ok_or_else(|| anyhow::anyhow!("Azure deployment required"))?,
            api_version: config
                .api_version
                .clone()
                .ok_or_else(|| anyhow::anyhow!("Pinned Azure API version required"))?,
        },
        _ => {
            ensure!(
                config.deployment.is_none() && config.api_version.is_none(),
                "Unexpected Azure deployment configuration"
            );
            match config.dialect {
                HttpModelDialect::OpenAiCompatible => http::Dialect::OpenAiCompatible,
                HttpModelDialect::AnthropicMessages => http::Dialect::AnthropicMessages,
                HttpModelDialect::AzureV1 => http::Dialect::AzureV1,
                HttpModelDialect::AzureDeployment => unreachable!(),
            }
        }
    };
    ensure!(
        config.root_certificates_der_base64.len() <= 8,
        "TLS root count exceeds capacity"
    );
    let mut roots = Vec::new();
    for cert in &config.root_certificates_der_base64 {
        ensure!(cert.len() <= 21_848, "TLS root exceeds capacity");
        roots.push(
            base64::engine::general_purpose::STANDARD
                .decode(cert)
                .map_err(|_| anyhow::anyhow!("Invalid TLS root encoding"))?,
        );
    }
    let spec = http::HttpProviderSpec {
        trusted_network: trusted,
        endpoint: config.endpoint.clone(),
        model: config.model_id.clone(),
        model_version: config.model_version.clone(),
        expected_remote_model: config.expected_remote_model.clone(),
        credential_name: config.credential_name.clone(),
        dialect,
        auth: match config.auth {
            HttpModelAuth::Bearer => http::AuthMode::Bearer,
            HttpModelAuth::ApiKey => http::AuthMode::ApiKey,
            HttpModelAuth::AnthropicApiKey => http::AuthMode::AnthropicApiKey,
        },
        json_mode: match config.json_mode {
            HttpModelJson::JsonSchema => http::JsonMode::JsonSchema,
            HttpModelJson::JsonObject => http::JsonMode::JsonObject,
            HttpModelJson::TextJson => http::JsonMode::TextJson,
        },
        token_limit: match config.token_limit {
            HttpModelTokens::MaxCompletionTokens => http::TokenLimit::MaxCompletionTokens,
            HttpModelTokens::MaxTokens => http::TokenLimit::MaxTokens,
        },
        capabilities: http::Capabilities {
            embeddings: config.capabilities.embeddings,
            vision: config.capabilities.vision,
        },
        max_output_tokens: config.max_output_tokens,
        allow_loopback_http: loopback,
        additional_root_certificates: roots,
    };
    http::validate_spec(&spec)?;
    ensure!(
        !config.task_id.trim().is_empty()
            && config.task_id.len() <= 200
            && !config.task_id.chars().any(char::is_control)
            && !yoke::paths::obvious_secret(&config.task_id),
        "Invalid model task identity"
    );
    ensure!(
        config.currency.len() == 3 && config.currency.bytes().all(|b| b.is_ascii_uppercase()),
        "Invalid model currency"
    );
    yoke::ledger::validate_cost(config.max_cost_micros)?;
    yoke::ledger::validate_cost(config.task_limit_micros)?;
    Ok((config, spec, path, hash))
}

fn http_process_provider(args: &HttpModelArguments) -> Result<yoke::providers::ProviderSpec> {
    let (config, _, path, hash) = load_http_model(
        &args.config,
        args.trust_network,
        args.allow_loopback_http,
        None,
    )?;
    let mut worker_args = vec![
        "model-http-worker".to_owned(),
        "--config".to_owned(),
        path.to_str()
            .ok_or_else(|| anyhow::anyhow!("HTTP profile path must be UTF-8"))?
            .to_owned(),
        "--config-hash".to_owned(),
        hash,
        "--trust-network".to_owned(),
    ];
    if args.allow_loopback_http {
        worker_args.push("--allow-loopback-http".to_owned());
    }
    let provider = yoke::providers::ProviderSpec {
        trusted: true,
        command: std::env::current_exe()?,
        args: worker_args,
        model_id: config.model_id,
        model_version: config.model_version,
        task_id: config.task_id,
        currency: config.currency,
        max_cost_micros: config.max_cost_micros,
        task_limit_micros: config.task_limit_micros,
    };
    provider.model_key()?;
    Ok(provider)
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
fn print_batch_result(value: &Value, max_bytes: u64, deadline: &Deadline) -> Result<()> {
    ensure!(
        (1024..=4_000_000).contains(&max_bytes),
        "Batch output budget must be 1024..4000000 bytes"
    );
    let batch_id = value["batchId"]
        .as_str()
        .filter(|id| yoke::model::valid_id(id))
        .ok_or_else(|| anyhow::anyhow!("Durable batch identity missing"))?;
    let bytes = match yoke::json::encode_bounded(value, max_bytes as usize - 1, deadline) {
        Ok(bytes) => bytes,
        Err(_) => {
            deadline.check()?;
            // The complete typed record is already durable. Do not create a
            // second untyped artifact merely because the transport is smaller.
            let reference = json!({"kind":"model_embedding_batch_reference","status":"partial",
                "batchId":batch_id,"batchStatus":value["status"],"snapshotId":value["snapshotId"],
                "model":value["model"],"dimension":value["dimension"],"count":value["count"],
                "resultTransport":"durable_batch_record",
                "readWith":{"command":"model-batch-status","batchId":batch_id,"maxBytes":4_000_000}});
            yoke::json::encode_bounded(&reference, max_bytes as usize - 1, deadline)?
        }
    };
    let mut stdout = io::stdout().lock();
    stdout
        .write_all(&bytes)
        .and_then(|_| stdout.write_all(b"\n"))
        .map_err(|_| anyhow::anyhow!("Batch {batch_id} is durable; output transport failed"))?;
    Ok(())
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
fn run_parity(
    engine: &mut Engine,
    operation: ExtensionOperation,
    arguments: &Value,
    deadline: &Deadline,
) -> Result<bool> {
    match operation {
        ExtensionOperation::Collections(_) => bail!("Collection host dispatch error"),
        ExtensionOperation::ModelsHttp(_)
        | ExtensionOperation::ModelHttpWorker(_)
        | ExtensionOperation::ModelsBedrock(_)
        | ExtensionOperation::ModelBedrockWorker(_)
        | ExtensionOperation::ModelBatchStatus { .. } => {
            bail!("HTTP model host dispatch error")
        }
        ExtensionOperation::Skills(_) => bail!("Skill host dispatch error"),
        ExtensionOperation::HostInstall(_) => bail!("Host installation dispatch error"),
        ExtensionOperation::MeaningArchive(_) => bail!("Meaning archive host dispatch error"),
        ExtensionOperation::Meaning(_) => bail!("Meaning graph host dispatch error"),
        ExtensionOperation::MixedView(_) => bail!("Mixed views host dispatch error"),
        ExtensionOperation::Feedback(_) => bail!("Feedback host dispatch error"),
        ExtensionOperation::DigestReceiver(_) => bail!("Digest receiver host dispatch error"),
        ExtensionOperation::DigestInbox(_) => bail!("Digest inbox host dispatch error"),
        ExtensionOperation::DigestHistoryCapture(_) => {
            bail!("Digest history capture host dispatch error")
        }
        ExtensionOperation::DigestHistoryJob(_) => bail!("Digest history job host dispatch error"),
        ExtensionOperation::GithubApp(_) => bail!("GitHub App host dispatch error"),
        ExtensionOperation::GithubDiscussions(_) => bail!("GitHub discussions host dispatch error"),
        ExtensionOperation::Decisions(args) => run_decisions(engine, *args, arguments, deadline),
        ExtensionOperation::Document(args) => run_document(engine, *args, arguments, deadline),
        ExtensionOperation::Retrieve { query, options } => {
            let mut options = options
                .map(|path| input_json(&path, 64000))
                .transpose()?
                .unwrap_or_else(|| json!({}));
            ensure!(
                options.is_object() && options.get("query").is_none(),
                "Retrieval options must be an object without a query override"
            );
            options["query"] = json!(query);
            let (snapshot, graph) = engine.select(arguments, deadline)?;
            print(&yoke::retrieval::find(
                &engine.store,
                &snapshot,
                &graph,
                &options,
                deadline,
            )?)?;
            Ok(true)
        }
        ExtensionOperation::ServerCatalog => {
            print(
                &json!({"declaredAdapters":yoke::server_catalog::entries().len(),"liveCertifiedAdapters":null,"adapters":yoke::server_catalog::entries(),"execution":"requires_explicit_host_registration_and_negotiated_capabilities"}),
            )?;
            Ok(true)
        }
        ExtensionOperation::Notes {
            operation,
            args,
            global_root,
            read_only,
            read_only_pattern,
            ignored_pattern,
        } => {
            ensure!(args.len() <= 128000, "Note input exceeds capacity");
            let args: Value = serde_json::from_str(&args)?;
            let (snapshot, _) = engine.select(arguments, deadline)?;
            let global = global_root
                .map(|path| -> Result<Engine> {
                    ensure!(
                        path.is_absolute(),
                        "Global note root must be an absolute host path"
                    );
                    Engine::open_with_timeout(path, deadline.remaining())
                })
                .transpose()?;
            let (store, scope) = global
                .as_ref()
                .map_or((&engine.store, yoke::notes::Scope::Project), |global| {
                    (&global.store, yoke::notes::Scope::Global)
                });
            let policy = yoke::notes::Policy {
                read_only_patterns: read_only_pattern,
                ignored_patterns: ignored_pattern,
                writable: !read_only,
            };
            print(&yoke::notes::invoke(
                &engine.store,
                store,
                &snapshot,
                scope,
                &operation,
                &args,
                &policy,
                deadline,
            )?)?;
            Ok(true)
        }
        ExtensionOperation::Graph { command } => {
            match command {
                GraphOperation::Build { options } => {
                    let options = options
                        .map(|path| -> Result<yoke::graph_view::BuildOptions> {
                            yoke::json::decode(input_json(&path, 128000)?)
                        })
                        .transpose()?
                        .unwrap_or_default();
                    let (snapshot, graph) = engine.select(arguments, deadline)?;
                    let view = yoke::graph_view::build(
                        &engine.store,
                        &snapshot,
                        &graph,
                        &options,
                        deadline,
                    )?;
                    print(
                        &json!({"viewId":view.id,"artifactId":view.artifact_id,"snapshotId":view.snapshot_id,"workspaceId":view.workspace_id,"overlays":view.overlays,"nodes":view.nodes.len(),"observations":view.observations.len(),"limitations":view.limitations,"complete":view.complete}),
                    )?;
                }
                GraphOperation::View {
                    view,
                    operation,
                    args,
                } => {
                    ensure!(
                        yoke::model::valid_id(&view) && args.len() <= 128000,
                        "Invalid graph view request"
                    );
                    let args: Value = yoke::json::parse(args.as_bytes(), 128000)?;
                    let view = yoke::graph_view::load(&engine.store, &view, deadline)?;
                    let result = match operation.as_str() {
                        "find" => yoke::graph_view::find(&engine.store, &view, &args, deadline)?,
                        "trace" => yoke::graph_view::trace(&engine.store, &view, &args, deadline)?,
                        "context" => {
                            yoke::graph_view::context(&engine.store, &view, &args, deadline)?
                        }
                        "export" => {
                            let object = args.as_object().ok_or_else(|| {
                                anyhow::anyhow!("Graph export args must be an object")
                            })?;
                            ensure!(
                                object.len() == 1 && object.contains_key("format"),
                                "Graph view export requires only format"
                            );
                            yoke::graph_view::export(
                                &engine.store,
                                &view,
                                args["format"].as_str().ok_or_else(|| {
                                    anyhow::anyhow!("Graph export format required")
                                })?,
                                deadline,
                            )?
                        }
                        _ => bail!("Unsupported graph view operation"),
                    };
                    print(&result)?;
                }
                GraphOperation::Analyze { options } => {
                    let options = options
                        .map(|path| -> Result<yoke::graph_extensions::AnalysisOptions> {
                            yoke::json::decode(input_json(&path, 64000)?)
                        })
                        .transpose()?
                        .unwrap_or_default();
                    let (snapshot, graph) = engine.select(arguments, deadline)?;
                    print(&yoke::graph_extensions::analyze(
                        &engine.store,
                        &snapshot,
                        &graph,
                        &options,
                        deadline,
                    )?)?;
                }
                GraphOperation::Compare { before, after } => {
                    ensure!(
                        yoke::model::valid_id(&before) && yoke::model::valid_id(&after),
                        "Invalid snapshot ID"
                    );
                    let before_snapshot = engine.store.snapshot(&before)?;
                    let before_graph = engine.store.graph(&before)?;
                    let after_snapshot = engine.store.snapshot(&after)?;
                    let after_graph = engine.store.graph(&after)?;
                    print(&yoke::graph_extensions::compare(
                        &engine.store,
                        &before_snapshot,
                        &before_graph,
                        &after_snapshot,
                        &after_graph,
                        deadline,
                    )?)?;
                }
                GraphOperation::CompareViews(_) => bail!(view_comparison_cli::INPUT),
                GraphOperation::Export {
                    format,
                    output,
                    max_nodes,
                    max_edges,
                } => {
                    let format: yoke::graph_exports::ExportFormat =
                        serde_json::from_value(json!(format))?;
                    let (snapshot, graph) = engine.select(arguments, deadline)?;
                    let bundle = yoke::graph_exports::generate(
                        &engine.store,
                        &snapshot,
                        &graph,
                        &yoke::graph_exports::ExportOptions {
                            format,
                            max_nodes,
                            max_edges,
                        },
                        deadline,
                    )?;
                    let output = output
                        .map(|path| -> Result<PathBuf> {
                            Ok(if path.is_absolute() {
                                path
                            } else {
                                std::env::current_dir()?.join(path)
                            })
                        })
                        .transpose()?;
                    if let Some(output) = &output {
                        yoke::graph_exports::write_new_bundle(&bundle, output, deadline)?;
                    }
                    let files:Vec<_>=bundle.files.iter().map(|file|json!({"path":file.path,"mediaType":file.media_type,"bytes":file.content.len()})).collect();
                    print(
                        &json!({"snapshotId":bundle.snapshot_id,"workspaceId":bundle.workspace_id,"graphHash":bundle.graph_hash,"format":format,"artifactId":bundle.artifact_id,"files":files,"output":output,"omittedNodes":bundle.omitted_nodes,"omittedObservations":bundle.omitted_observations,"complete":bundle.complete}),
                    )?;
                }
            }
            Ok(true)
        }
        _ => bail!("Internal parity dispatch error"),
    }
}

#[derive(clap::Args)]
struct AgentReplArguments {
    #[arg(long)]
    python: PathBuf,
    #[arg(long)]
    trust_python: bool,
    #[arg(long)]
    cells: PathBuf,
    #[arg(long)]
    id: Option<String>,
    #[arg(long)]
    host_profile: Option<PathBuf>,
    #[arg(long, default_value_t = 100000)]
    max_output_bytes: usize,
    #[arg(long)]
    shell_command: Option<PathBuf>,
    #[arg(long)]
    shell_argument: Vec<String>,
}
fn open_session(
    engine: Engine,
    id: Option<String>,
    host_profile: Option<PathBuf>,
    deadline: &Deadline,
) -> Result<yoke::project_session::Session> {
    let host = host_profile
        .map(|path| -> Result<yoke::project_session::HostProfile> {
            yoke::json::decode(input_json(&path, 128000)?)
        })
        .transpose()?
        .unwrap_or_default();
    yoke::project_session::Session::open(engine, host, id.as_deref(), deadline)
}
fn run_session(
    engine: Engine,
    id: Option<String>,
    host_profile: Option<PathBuf>,
    requests: PathBuf,
    deadline: &Deadline,
) -> Result<bool> {
    #[derive(serde::Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Request {
        operation: String,
        args: Value,
    }
    let requests: Vec<Request> = yoke::json::decode(input_json(&requests, 4_000_000)?)?;
    ensure!(requests.len() <= 1000, "Too many session requests");
    let mut session = open_session(engine, id, host_profile, deadline)?;
    let mut results = Vec::new();
    let mut bytes = 0usize;
    for request in requests {
        deadline.check()?;
        let result = session.invoke(&request.operation, &request.args, deadline)?;
        bytes = bytes
            .checked_add(serde_json::to_vec(&result)?.len())
            .ok_or_else(|| anyhow::anyhow!("Session output size overflow"))?;
        ensure!(bytes <= 8_000_000, "Session batch output budget exceeded");
        results.push(result);
    }
    print(&json!({"results":results,"sourceWritten":false,"atomicBatch":false}))?;
    Ok(true)
}
#[derive(clap::Args)]
struct ServeSessionArguments {
    #[arg(long)]
    id: Option<String>,
    #[arg(long)]
    host_profile: Option<PathBuf>,
    #[arg(long,default_value="native",value_parser=["native","serena","trail","graft","graphify"])]
    profile: String,
    #[arg(long)]
    python: Option<PathBuf>,
    #[arg(long)]
    trust_python: bool,
    #[arg(long, default_value_t = 100000)]
    max_output_bytes: usize,
    #[arg(long)]
    shell_command: Option<PathBuf>,
    #[arg(long)]
    shell_argument: Vec<String>,
}
#[derive(clap::Args)]
struct GitArguments {
    #[command(subcommand)]
    operation: GitOperation,
}
#[derive(clap::Args)]
struct SkillArguments {
    #[arg(value_parser = ["learn", "list", "show", "confirm", "fold", "publish", "recover"])]
    operation: String,
    #[arg(long)]
    personal_root: PathBuf,
    #[arg(long)]
    project: String,
    #[arg(long, default_value = "{}")]
    args: String,
    #[arg(long)]
    allow_personal_writes: bool,
}
#[derive(clap::Args)]
struct DocumentArguments {
    #[command(subcommand)]
    operation: DocumentOperation,
}
#[derive(Subcommand)]
enum DocumentOperation {
    Build {
        path: String,
    },
    Load {
        id: String,
    },
    Query {
        id: String,
        #[arg(value_parser = ["find", "read", "trace"])]
        operation: String,
        #[arg(long, default_value = "{}")]
        args: String,
    },
    Explorer {
        id: String,
        #[arg(long)]
        output: PathBuf,
    },
    Archive {
        id: String,
        #[arg(long)]
        output: PathBuf,
    },
    QueryArchive {
        directory: PathBuf,
        #[arg(value_parser = ["find", "read", "trace", "describe"])]
        operation: String,
        #[arg(long, default_value = "{}")]
        args: String,
    },
    ExplorerArchive {
        directory: PathBuf,
        #[arg(long)]
        output: PathBuf,
    },
}
fn document_query(
    graph: &yoke::document_graph::CheckedDocumentGraph,
    operation: &str,
    args: &str,
    deadline: &Deadline,
) -> Result<Value> {
    let args = yoke::json::parse(args.as_bytes(), 64_000)?;
    match operation {
        "find" => yoke::document_graph::find(graph, &args, deadline),
        "read" => yoke::document_graph::read(graph, &args, deadline),
        "trace" => yoke::document_graph::trace(graph, &args, deadline),
        "describe" => {
            ensure!(
                args == json!({}),
                "Document description accepts no query arguments"
            );
            Ok(yoke::document_graph::describe(graph))
        }
        _ => bail!("Unsupported document operation"),
    }
}
fn write_document_explorer(
    graph: &yoke::document_graph::CheckedDocumentGraph,
    output: &std::path::Path,
    deadline: &Deadline,
) -> Result<Value> {
    let bytes = yoke::document_graph::explorer(graph, deadline)?;
    ensure!(
        bytes.len() <= 32_000_000,
        "Document explorer exceeds output capacity"
    );
    let output = std::path::absolute(output)?;
    for parent in output
        .parent()
        .ok_or_else(|| anyhow::anyhow!("Output parent absent"))?
        .ancestors()
    {
        let meta = fs::symlink_metadata(parent)?;
        ensure!(
            meta.is_dir() && !yoke::paths::is_link(&meta),
            "Document output parent must be an existing nonlinked directory"
        );
    }
    deadline.check()?;
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&output)?;
    for chunk in bytes.chunks(64 * 1024) {
        deadline.check()?;
        file.write_all(chunk)?;
    }
    file.sync_all()?;
    Ok(
        json!({"output":output,"bytes":bytes.len(),"contentHash":yoke::model::hash(&bytes),"sourceSnapshotModified":false,"outputCreated":true}),
    )
}
fn run_document(
    engine: &Engine,
    args: DocumentArguments,
    arguments: &Value,
    deadline: &Deadline,
) -> Result<bool> {
    use DocumentOperation::*;
    let result = match args.operation {
        Build { path } => yoke::document_graph::build(engine, &path, deadline)?,
        Load { id } => yoke::document_graph::describe(&yoke::document_graph::load(
            &engine.store,
            &id,
            deadline,
        )?),
        Query {
            id,
            operation,
            args,
        } => document_query(
            &yoke::document_graph::load(&engine.store, &id, deadline)?,
            &operation,
            &args,
            deadline,
        )?,
        Explorer { id, output } => write_document_explorer(
            &yoke::document_graph::load(&engine.store, &id, deadline)?,
            &output,
            deadline,
        )?,
        Archive { id, output } => yoke::document_graph::write_archive(
            &yoke::document_graph::load(&engine.store, &id, deadline)?,
            &output,
            deadline,
        )?,
        QueryArchive { .. } | ExplorerArchive { .. } => {
            bail!("Internal standalone document dispatch error")
        }
    };
    print_bounded(
        engine,
        &result,
        arguments["maxBytes"].as_u64().unwrap_or(32_000),
    )?;
    Ok(true)
}
#[derive(Subcommand)]
enum GitOperation {
    Areas(Box<git_areas_cli::Arguments>),
    Conflicts(Box<git_conflicts_cli::Arguments>),
    Analyze {
        #[arg(long)]
        options: Option<PathBuf>,
        #[arg(long)]
        git: Option<PathBuf>,
        #[arg(long)]
        trust_git: bool,
    },
    Load {
        id: String,
    },
    Affected {
        id: String,
        #[arg(long)]
        options: Option<PathBuf>,
    },
    Render {
        id: String,
        #[arg(long,default_value="markdown",value_parser=["json","text","markdown","mermaid","html"])]
        format: String,
    },
    Digest {
        id: String,
        #[arg(value_parser=["report","history","file","impact-details"])]
        kind: String,
        #[arg(long)]
        path: Option<String>,
        #[arg(long)]
        options: Option<PathBuf>,
        #[arg(long,value_parser=["json","html"])]
        format: Option<String>,
    },
    Seed {
        #[arg(long)]
        parent_root: PathBuf,
        #[arg(long)]
        parent_workspace: String,
        #[arg(long)]
        git: Option<PathBuf>,
        #[arg(long)]
        trust_git: bool,
    },
}
fn selected_git(path: Option<PathBuf>, trusted: bool) -> Result<yoke::git_workflows::HostGit> {
    ensure!(trusted, "Git execution requires --trust-git");
    match path {
        Some(executable) => {
            ensure!(executable.is_absolute(), "Git executable must be absolute");
            Ok(yoke::git_workflows::HostGit { executable })
        }
        None => yoke::git_workflows::HostGit::discover(),
    }
}
fn run_git(mut engine: Engine, operation: GitOperation, deadline: &Deadline) -> Result<bool> {
    match operation {
        GitOperation::Areas(_) => bail!("Git area host dispatch error"),
        GitOperation::Conflicts(_) => bail!("Git conflict host dispatch error"),
        GitOperation::Analyze {
            options,
            git,
            trust_git,
        } => {
            let git = selected_git(git, trust_git)?;
            let options = options
                .map(|path| -> Result<yoke::git_workflows::WorkflowOptions> {
                    yoke::json::decode(input_json(&path, 128000)?)
                })
                .transpose()?
                .unwrap_or_default();
            print(&yoke::git_workflows::analyze(
                &mut engine,
                &git,
                &options,
                deadline,
            )?)?;
        }
        GitOperation::Load { id } => {
            print(&yoke::git_workflows::load(&engine.store, &id, deadline)?)?;
        }
        GitOperation::Affected { .. } => bail!(git_affected_cli::INPUT),
        GitOperation::Render { id, format } => {
            let report = yoke::git_workflows::load(&engine.store, &id, deadline)?;
            let text = yoke::git_workflows::render(&engine.store, &report, &format, deadline)?;
            let mut output = io::stdout().lock();
            output.write_all(text.as_bytes())?;
            output.write_all(b"\n")?;
        }
        GitOperation::Digest {
            id,
            kind,
            path,
            options,
            format,
        } => {
            ensure!(
                kind != "impact-details" && options.is_none() && format.is_none(),
                git_impact_cli::INPUT
            );
            let report = yoke::git_workflows::load(&engine.store, &id, deadline)?;
            print(&yoke::git_workflows::digest(
                &engine.store,
                &report,
                &kind,
                path.as_deref(),
                deadline,
            )?)?;
        }
        GitOperation::Seed {
            parent_root,
            parent_workspace,
            git,
            trust_git,
        } => {
            let git = selected_git(git, trust_git)?;
            print(&yoke::git_workflows::seed_worktree_cache(
                &mut engine,
                &git,
                &parent_root,
                &parent_workspace,
                deadline,
            )?)?;
        }
    }
    Ok(true)
}
fn invoke_model_operation(
    engine: &mut Engine,
    command: ModelOperation,
    provider: &yoke::providers::ProviderSpec,
    arguments: &Value,
    deadline: &Deadline,
) -> Result<Value> {
    let data = match command {
        ModelOperation::Media { path } => {
            yoke::providers::describe_media(engine, &path, provider, deadline)?
        }
        other => {
            let (snapshot, graph) = engine.select(arguments, deadline)?;
            match other {
                ModelOperation::Embeddings => yoke::providers::generate_embeddings(
                    &engine.store,
                    &snapshot,
                    &graph,
                    provider,
                    deadline,
                )?,
                ModelOperation::Search { query, limit } => yoke::providers::semantic_search(
                    &engine.store,
                    &snapshot,
                    &graph,
                    &query,
                    limit,
                    provider,
                    deadline,
                )?,
                ModelOperation::Rerank { query, entities } => yoke::providers::rerank(
                    &engine.store,
                    &snapshot,
                    &graph,
                    &query,
                    &entities,
                    provider,
                    deadline,
                )?,
                ModelOperation::Summarize { entities } => yoke::providers::summarize_sources(
                    &engine.store,
                    &snapshot,
                    &graph,
                    &entities,
                    provider,
                    deadline,
                )?,
                ModelOperation::Media { .. } => unreachable!(),
            }
        }
    };
    Ok(data)
}
fn run_extension(
    mut engine: Engine,
    prepared_http: Option<PreparedModelProvider>,
    operation: ExtensionOperation,
    arguments: &Value,
    deadline: &Deadline,
) -> Result<bool> {
    if let ExtensionOperation::Collections(args) = operation {
        let CollectionOperation::Register {
            name,
            expected,
            view,
            ..
        } = &args.operation
        else {
            bail!("Collection registration dispatch error");
        };
        let (id, kind) = if let Some(id) = view {
            let view = yoke::graph_view::load(&engine.store, id, deadline)?;
            let registry = yoke::graph_collections::Registry::open(&args.registry)?;
            (
                registry.register_view_cas(
                    name,
                    &engine.store,
                    &view,
                    expected.as_deref(),
                    deadline,
                )?,
                "view",
            )
        } else {
            let (snapshot, graph) = engine.select(arguments, deadline)?;
            let registry = yoke::graph_collections::Registry::open(&args.registry)?;
            (
                registry.register_cas(
                    name,
                    &engine.store,
                    &snapshot,
                    &graph,
                    expected.as_deref(),
                    deadline,
                )?,
                "native",
            )
        };
        collection_output(
            &json!({"name":name,"registrationId":id,"kind":kind,"executionAuthority":"none"}),
            arguments["maxBytes"].as_u64().unwrap_or(32000),
            deadline,
        )?;
        return Ok(true);
    }
    if let ExtensionOperation::ModelsBedrock(args) = operation {
        let prepared =
            prepared_http.ok_or_else(|| anyhow::anyhow!("Bedrock model preflight missing"))?;
        let max_bytes = arguments["maxBytes"].as_u64().unwrap_or(32000);
        let data = match (args.operation, prepared.embedding_batch) {
            (ModelOperation::Embeddings, Some(policy)) => {
                ensure!(
                    (1024..=4_000_000).contains(&max_bytes),
                    "Batch output budget must be 1024..4000000 bytes"
                );
                let (snapshot, graph) = engine.select(arguments, deadline)?;
                let result = yoke::provider_batches::generate_embeddings_batched(
                    &engine.store,
                    &snapshot,
                    &graph,
                    &prepared.provider,
                    &policy,
                    deadline,
                )?;
                print_batch_result(&result, max_bytes, deadline)?;
                return Ok(true);
            }
            (command, _) => invoke_model_operation(
                &mut engine,
                command,
                &prepared.provider,
                arguments,
                deadline,
            )?,
        };
        print_bounded(&engine, &data, max_bytes)?;
        return Ok(true);
    }
    if let ExtensionOperation::ModelsHttp(args) = operation {
        let prepared =
            prepared_http.ok_or_else(|| anyhow::anyhow!("HTTP model preflight missing"))?;
        let data = invoke_model_operation(
            &mut engine,
            args.operation,
            &prepared.provider,
            arguments,
            deadline,
        )?;
        print_bounded(
            &engine,
            &data,
            arguments["maxBytes"].as_u64().unwrap_or(32000),
        )?;
        return Ok(true);
    }
    if let ExtensionOperation::Git(git) = operation {
        ensure!(
            arguments.get("snapshotId").is_none(),
            "Git workflows select their own proven snapshots"
        );
        return run_git(engine, git.operation, deadline);
    }
    if let ExtensionOperation::Session {
        id,
        host_profile,
        requests,
    } = operation
    {
        ensure!(
            arguments.get("snapshotId").is_none(),
            "Session operations select their own source snapshots"
        );
        return run_session(engine, id, host_profile, requests, deadline);
    }
    if let ExtensionOperation::ServeSession(server) = operation {
        ensure!(
            arguments.get("snapshotId").is_none(),
            "Session operations select their own source snapshots"
        );
        let ServeSessionArguments {
            id,
            host_profile,
            profile,
            python,
            trust_python,
            max_output_bytes,
            shell_command,
            shell_argument,
        } = *server;
        ensure!(
            python.is_some()
                || (!trust_python && shell_command.is_none() && shell_argument.is_empty()),
            "Python runtime options require an explicit Python executable"
        );
        ensure!(
            python.is_none() || profile == "serena",
            "Persistent Python is available only in the Serena profile"
        );
        ensure!(
            shell_command.is_some() || shell_argument.is_empty(),
            "Shell arguments require an explicit shell command"
        );
        let mut session = open_session(engine, id, host_profile, deadline)?;
        if let Some(python) = python {
            let spec = yoke::agent_repl::ReplSpec {
                python,
                trust_python,
                max_output_bytes,
                shell: shell_command.map(|command| yoke::agent_repl::HostShell {
                    command,
                    arguments: shell_argument,
                }),
            };
            let mut runtime = yoke::agent_repl::AgentRepl::spawn(spec, deadline)?;
            let result = yoke::protocol::serve_serena_with_repl(
                &mut session,
                &mut runtime,
                io::stdin().lock(),
                io::stdout().lock(),
            );
            runtime.close()?;
            result?;
        } else if profile == "serena" {
            yoke::protocol::serve_serena(&mut session, io::stdin().lock(), io::stdout().lock())?;
        } else if profile == "graphify" {
            yoke::protocol::serve_graphify(&mut session, io::stdin().lock(), io::stdout().lock())?;
        } else if matches!(profile.as_str(), "trail" | "graft") {
            yoke::protocol::serve_trail(
                &mut session,
                &profile,
                io::stdin().lock(),
                io::stdout().lock(),
            )?;
        } else {
            yoke::protocol::serve_session(&mut session, io::stdin().lock(), io::stdout().lock())?;
        }
        return Ok(true);
    }
    if let ExtensionOperation::AgentRepl(repl_args) = operation {
        let AgentReplArguments {
            python,
            trust_python,
            cells,
            id,
            host_profile,
            max_output_bytes,
            shell_command,
            shell_argument,
        } = *repl_args;
        ensure!(
            arguments.get("snapshotId").is_none(),
            "Agent REPL selects its own source snapshots"
        );
        ensure!(
            shell_command.is_some() || shell_argument.is_empty(),
            "Shell arguments require an explicit shell command"
        );
        let cells: Vec<String> = yoke::json::decode(input_json(&cells, 4_000_000)?)?;
        ensure!(cells.len() <= 1000, "Too many REPL cells");
        let spec = yoke::agent_repl::ReplSpec {
            python,
            trust_python,
            max_output_bytes,
            shell: shell_command.map(|command| yoke::agent_repl::HostShell {
                command,
                arguments: shell_argument,
            }),
        };
        let mut session = open_session(engine, id, host_profile, deadline)?;
        let mut repl = yoke::agent_repl::AgentRepl::spawn(spec, deadline)?;
        let mut results = Vec::new();
        let mut bytes = 0usize;
        for code in cells {
            let result = repl.execute(&mut session, &code, deadline)?;
            bytes = bytes
                .checked_add(serde_json::to_vec(&result)?.len())
                .ok_or_else(|| anyhow::anyhow!("REPL output size overflow"))?;
            ensure!(bytes <= 8_000_000, "REPL batch output budget exceeded");
            results.push(result);
        }
        repl.close()?;
        print(
            &json!({"cells":results,"execution":"trusted_host_python","sourceEffects":"unmonitored_host_execution","osSandbox":false}),
        )?;
        return Ok(true);
    }
    if let ExtensionOperation::Freshness = operation {
        let mut arguments = arguments.clone();
        arguments["mode"] = json!("freshness");
        let result = engine.invoke("ci_status", arguments)?;
        let success = result["status"] != "error";
        print(&result)?;
        return Ok(success);
    }
    run_parity(&mut engine, operation, arguments, deadline)
}
#[derive(clap::Args)]
struct HttpServeArguments {
    #[arg(long)]
    config: PathBuf,
    #[arg(long)]
    trust_service: bool,
    #[arg(long)]
    allow_remote_bind: bool,
    #[arg(long)]
    trust_python: bool,
    #[arg(long)]
    trust_shell: bool,
    #[arg(long, default_value_t = 0)]
    duration_ms: u64,
}
#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields, default)]
struct HttpHostFile {
    bind: std::net::SocketAddr,
    tls_credential_name: Option<String>,
    server_names: Vec<String>,
    allowed_origins: Vec<String>,
    endpoint: String,
    request_timeout_ms: u64,
    max_body_bytes: usize,
    max_connections: usize,
    workers: usize,
    max_sessions: usize,
    max_sessions_per_principal: usize,
    session_ttl_ms: u64,
    max_requests_per_session: u64,
    principals: Vec<HttpPrincipalFile>,
}
impl Default for HttpHostFile {
    fn default() -> Self {
        let config = yoke::mcp_http::Config::default();
        Self {
            bind: config.bind,
            tls_credential_name: None,
            server_names: config.server_names,
            allowed_origins: config.allowed_origins,
            endpoint: config.endpoint,
            request_timeout_ms: config.request_timeout_ms,
            max_body_bytes: config.max_body_bytes,
            max_connections: config.max_connections,
            workers: config.workers,
            max_sessions: config.max_sessions,
            max_sessions_per_principal: config.max_sessions_per_principal,
            session_ttl_ms: config.session_ttl_ms,
            max_requests_per_session: config.max_requests_per_session,
            principals: vec![],
        }
    }
}
#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct HttpPrincipalFile {
    id: String,
    token_credential_name: String,
    #[serde(default)]
    root: Option<PathBuf>,
    #[serde(default = "http_default_profile")]
    profile: String,
    #[serde(default)]
    host_profile: yoke::project_session::HostProfile,
    #[serde(default)]
    capture_options: yoke::capture::CaptureOptions,
    #[serde(default)]
    servers: Vec<HttpServerFile>,
    #[serde(default)]
    allow_writes: bool,
    #[serde(default)]
    agent_repl: Option<HttpReplFile>,
    #[serde(default)]
    collection_grants: std::collections::BTreeMap<
        String,
        std::collections::BTreeMap<String, yoke::federation::ImportGrant>,
    >,
}
#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct HttpReplFile {
    python: PathBuf,
    #[serde(default = "http_repl_output_limit")]
    max_output_bytes: usize,
    #[serde(default)]
    shell: Option<HttpShellFile>,
}
#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct HttpShellFile {
    command: PathBuf,
    #[serde(default)]
    arguments: Vec<String>,
}
fn http_repl_output_limit() -> usize {
    100_000
}
impl HttpReplFile {
    // Called only after independent host CLI authority checks.
    fn approved_spec(&self) -> yoke::agent_repl::ReplSpec {
        yoke::agent_repl::ReplSpec {
            python: self.python.clone(),
            trust_python: true,
            max_output_bytes: self.max_output_bytes,
            shell: self
                .shell
                .as_ref()
                .map(|shell| yoke::agent_repl::HostShell {
                    command: shell.command.clone(),
                    arguments: shell.arguments.clone(),
                }),
        }
    }
}
#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct HttpServerFile {
    language: String,
    command: String,
    #[serde(default)]
    args: Vec<String>,
    #[serde(default)]
    configuration: yoke::lsp::HostConfiguration,
}
fn http_default_profile() -> String {
    "session".into()
}
fn http_profile(value: &str) -> Result<yoke::protocol::Profile> {
    use yoke::protocol::Profile;
    Ok(match value {
        "native" => Profile::Native,
        "session" => Profile::Session,
        "serena" => Profile::Serena,
        "graphify" => Profile::Graphify,
        "trail" => Profile::Trail,
        "graft" => Profile::Graft,
        _ => bail!("Unsupported host MCP profile"),
    })
}
fn run_http(
    root: &std::path::Path,
    args: &HttpServeArguments,
    trust_servers: bool,
    allow_writes: bool,
) -> Result<bool> {
    ensure!(
        args.trust_service,
        "MCP HTTP service requires --trust-service"
    );
    ensure!(
        args.duration_ms <= 86_400_000,
        "Service duration exceeds one day; use zero for foreground operation"
    );
    let host: HttpHostFile = yoke::json::decode(input_json(&args.config, 1_000_000)?)?;
    ensure!(
        !host.principals.is_empty() && host.principals.len() <= 100,
        "MCP service requires bounded host principals"
    );
    // Admit the complete host schema and authority switches before resolving
    // credentials or opening source state for any principal.
    for principal in &host.principals {
        http_profile(&principal.profile)?;
        ensure!(
            principal
                .root
                .as_ref()
                .is_none_or(|root| root.is_absolute()),
            "Explicit MCP target roots must be absolute"
        );
        ensure!(
            principal.servers.len() <= 64 && (principal.servers.is_empty() || trust_servers),
            "MCP language servers require --trust-servers"
        );
        ensure!(
            !principal.allow_writes || allow_writes,
            "MCP source writes require --allow-writes"
        );
        principal.capture_options.clone().validated()?;
        if let Some(runtime) = &principal.agent_repl {
            ensure!(
                principal.profile == "serena",
                "Persistent Python is available only in the Serena profile"
            );
            ensure!(
                args.trust_python,
                "MCP Python runtime requires --trust-python"
            );
            ensure!(
                runtime.shell.is_none() || args.trust_shell,
                "MCP Python shell requires --trust-shell"
            );
            yoke::agent_repl::AgentRepl::validate_spec(&runtime.approved_spec())?;
        }
    }
    let tls = host
        .tls_credential_name
        .as_deref()
        .map(yoke::host_credentials::read_tls)
        .transpose()?
        .map(|identity| yoke::mcp_http::TlsMaterial {
            certificates: vec![identity.certificate],
            private_key: identity.private_key,
        });
    let mut principals = vec![];
    for principal in host.principals {
        let mut target = yoke::mcp_http::Target::new(
            principal.root.unwrap_or_else(|| root.to_owned()),
            http_profile(&principal.profile)?,
            principal.host_profile,
        );
        target.capture_options = principal.capture_options;
        target.servers = principal
            .servers
            .into_iter()
            .map(|server| yoke::lsp::ConfiguredServer {
                server: yoke::lsp::Server {
                    language: server.language,
                    command: server.command,
                    args: server.args,
                    trust: true,
                },
                configuration: server.configuration,
            })
            .collect();
        target.allow_writes = principal.allow_writes;
        target.collection_grants = principal.collection_grants;
        target.agent_repl = principal
            .agent_repl
            .as_ref()
            .map(HttpReplFile::approved_spec);
        principals.push(yoke::mcp_http::Principal::new(
            &principal.id,
            yoke::host_credentials::read_token(&principal.token_credential_name)?,
            target,
        )?);
    }
    let endpoint = host.endpoint.clone();
    let config = yoke::mcp_http::Config {
        bind: host.bind,
        tls,
        allow_remote_bind: args.allow_remote_bind,
        server_names: host.server_names,
        allowed_origins: host.allowed_origins,
        endpoint: host.endpoint,
        request_timeout_ms: host.request_timeout_ms,
        max_body_bytes: host.max_body_bytes,
        max_connections: host.max_connections,
        workers: host.workers,
        max_sessions: host.max_sessions,
        max_sessions_per_principal: host.max_sessions_per_principal,
        session_ttl_ms: host.session_ttl_ms,
        max_requests_per_session: host.max_requests_per_session,
    };
    let handle = yoke::mcp_http::serve(config, principals)?;
    print(
        &json!({"status":"listening","transport":"mcp_streamable_http_json","address":handle.local_addr().to_string(),"endpoint":endpoint,"grantAuthority":"explicit_host_profile","secrets":"os_credential_store_and_ram_only"}),
    )?;
    let started = std::time::Instant::now();
    while args.duration_ms == 0 || started.elapsed().as_millis() < args.duration_ms as u128 {
        std::thread::sleep(std::time::Duration::from_millis(25));
    }
    handle.shutdown()?;
    print(&json!({"status":"stopped"}))?;
    Ok(true)
}
fn validate_existing_batch_store(root: &std::path::Path, deadline: &Deadline) -> Result<()> {
    let dir = root.join(".yoke");
    for path in [&dir, &dir.join("index.sqlite")] {
        let metadata = fs::symlink_metadata(path)?;
        ensure!(
            !yoke::paths::is_link(&metadata),
            "Batch status store must not be linked"
        );
    }
    for name in [
        "index.sqlite-wal",
        "index.sqlite-shm",
        "index.sqlite-journal",
    ] {
        if let Ok(metadata) = fs::symlink_metadata(dir.join(name)) {
            ensure!(
                !yoke::paths::is_link(&metadata),
                "Batch status SQLite sidecar must not be linked"
            );
        }
    }
    deadline.check()?;
    let db = rusqlite::Connection::open_with_flags(
        dir.join("index.sqlite"),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )?;
    db.busy_timeout(deadline.remaining().min(std::time::Duration::from_secs(10)))?;
    db.set_limit(rusqlite::limits::Limit::SQLITE_LIMIT_LENGTH, 65_536)?;
    validate_existing_batch_schema(&db, deadline)
}
fn validate_existing_batch_schema(db: &rusqlite::Connection, deadline: &Deadline) -> Result<()> {
    let workspace: String = db.query_row(
        "SELECT value FROM meta WHERE key='workspace' AND length(CAST(value AS BLOB))=64",
        [],
        |row| row.get(0),
    )?;
    ensure!(
        yoke::model::valid_id(&workspace),
        "Batch status requires an initialized workspace store"
    );
    // Preparing these reads verifies the existing schema without running the
    // normal constructor's CREATE TABLE/identity initialization path.
    for query in [
        "SELECT id,value FROM snapshots LIMIT 0",
        "SELECT id,value FROM graphs LIMIT 0",
        "SELECT key,value FROM cache LIMIT 0",
        "SELECT kind,id,value FROM records LIMIT 0",
        "SELECT snapshot,id,name,path,signature FROM symbols LIMIT 0",
        "SELECT snapshot,path,text FROM documents LIMIT 0",
    ] {
        deadline.check()?;
        db.prepare(query)?;
    }
    Ok(())
}
fn run(cli: Cli) -> Result<bool> {
    let mut operation: Operation = cli.command.into();
    if let Operation::Extensions(extension) = &operation
        && let ExtensionOperation::Graph {
            command: GraphOperation::CompareViews(args),
        } = extension.as_ref()
    {
        ensure!(
            cli.snapshot.is_none()
                && cli.capture_config.is_none()
                && cli.include_ignored_dir.is_empty()
                && cli.analyzer_epoch.is_none()
                && cli.language_server_config.is_none()
                && !cli.trust_servers
                && !cli.allow_writes,
            view_comparison_cli::INPUT
        );
        return view_comparison_cli::run(args, &cli.root, cli.timeout, cli.max_bytes);
    }
    if let Operation::Extensions(extension) = &operation
        && let ExtensionOperation::Git(git) = extension.as_ref()
        && let GitOperation::Areas(args) = &git.operation
    {
        ensure!(
            cli.snapshot.is_none()
                && cli.capture_config.is_none()
                && cli.include_ignored_dir.is_empty()
                && cli.analyzer_epoch.is_none()
                && cli.language_server_config.is_none()
                && !cli.trust_servers
                && !cli.allow_writes,
            "git_areas_input_invalid"
        );
        return args.run(&cli.root, cli.timeout, cli.max_bytes);
    }
    if let Operation::Extensions(extension) = &operation
        && let ExtensionOperation::Git(git) = extension.as_ref()
        && let GitOperation::Conflicts(args) = &git.operation
    {
        ensure!(
            cli.snapshot.is_none()
                && cli.capture_config.is_none()
                && cli.include_ignored_dir.is_empty()
                && cli.analyzer_epoch.is_none()
                && cli.language_server_config.is_none()
                && !cli.trust_servers
                && !cli.allow_writes,
            "git_conflicts_input_invalid"
        );
        return args.run(&cli.root, cli.timeout, cli.max_bytes);
    }
    if let Operation::Extensions(extension) = &operation
        && let ExtensionOperation::Git(git) = extension.as_ref()
        && let GitOperation::Affected { id, options } = &git.operation
    {
        ensure!(
            cli.snapshot.is_none()
                && cli.capture_config.is_none()
                && cli.include_ignored_dir.is_empty()
                && cli.analyzer_epoch.is_none()
                && cli.language_server_config.is_none()
                && !cli.trust_servers
                && !cli.allow_writes,
            git_affected_cli::INPUT
        );
        return git_affected_cli::run(
            &cli.root,
            id,
            options.as_deref(),
            cli.timeout,
            cli.max_bytes,
        );
    }
    if let Operation::Extensions(extension) = &operation
        && let ExtensionOperation::Git(git) = extension.as_ref()
        && let GitOperation::Digest {
            id,
            kind,
            path,
            options,
            format,
        } = &git.operation
    {
        if kind == "impact-details" {
            ensure!(
                path.is_none()
                    && cli.snapshot.is_none()
                    && cli.capture_config.is_none()
                    && cli.include_ignored_dir.is_empty()
                    && cli.analyzer_epoch.is_none()
                    && cli.language_server_config.is_none()
                    && !cli.trust_servers
                    && !cli.allow_writes,
                git_impact_cli::INPUT
            );
            return git_impact_cli::run(
                &cli.root,
                id,
                options.as_deref(),
                format.as_deref(),
                cli.timeout,
                cli.max_bytes,
            );
        }
        ensure!(options.is_none() && format.is_none(), git_impact_cli::INPUT);
    }
    if let Operation::Extensions(extension) = &operation
        && let ExtensionOperation::MeaningArchive(args) = extension.as_ref()
    {
        args.preflight_permission()?;
        ensure!(
            cli.snapshot.is_none()
                && cli.capture_config.is_none()
                && cli.include_ignored_dir.is_empty()
                && cli.language_server_config.is_none()
                && !cli.trust_servers
                && !cli.allow_writes
                && cli.analyzer_epoch.is_none(),
            "Meaning archives select checked stored or supplied data; source/provider flags are unavailable"
        );
        ensure!(
            args.uses_source_store() || cli.root == std::path::Path::new("."),
            "This meaning archive operation does not select a source workspace"
        );
        return meaning_archive_cli::run(args, &cli.root, cli.timeout, cli.max_bytes);
    }
    if let Operation::Extensions(extension) = &operation
        && let ExtensionOperation::HostInstall(args) = extension.as_ref()
    {
        args.preflight_permission()?;
        ensure!(
            cli.root == std::path::Path::new(".")
                && cli.snapshot.is_none()
                && cli.capture_config.is_none()
                && cli.include_ignored_dir.is_empty()
                && cli.language_server_config.is_none()
                && !cli.trust_servers
                && !cli.allow_writes
                && cli.analyzer_epoch.is_none(),
            "Host installation selects its explicit host files; source/provider flags are unavailable"
        );
        return host_install_cli::run(args, cli.timeout, cli.max_bytes);
    }
    if let Operation::Extensions(extension) = &operation
        && let ExtensionOperation::GithubDiscussions(args) = extension.as_ref()
    {
        ensure!(
            cli.root == std::path::Path::new(".")
                && cli.snapshot.is_none()
                && cli.capture_config.is_none()
                && cli.include_ignored_dir.is_empty()
                && cli.language_server_config.is_none()
                && !cli.trust_servers
                && !cli.allow_writes
                && cli.analyzer_epoch.is_none(),
            "GitHub discussion reads select explicit host inputs; source/provider/write flags are unavailable"
        );
        return args.run(cli.timeout, cli.max_bytes);
    }
    if let Operation::Extensions(extension) = &operation
        && let ExtensionOperation::DigestHistoryJob(args) = extension.as_ref()
    {
        ensure!(
            cli.snapshot.is_none()
                && cli.capture_config.is_none()
                && cli.include_ignored_dir.is_empty()
                && cli.language_server_config.is_none()
                && !cli.trust_servers
                && cli.analyzer_epoch.is_none(),
            "History jobs inherit captured scope; source/provider options are unavailable"
        );
        return args.run(&cli.root, cli.allow_writes, cli.timeout, cli.max_bytes);
    }
    if let Operation::Extensions(extension) = &operation
        && let ExtensionOperation::DigestHistoryCapture(args) = extension.as_ref()
    {
        ensure!(
            cli.snapshot.is_none()
                && cli.capture_config.is_none()
                && cli.include_ignored_dir.is_empty()
                && cli.language_server_config.is_none()
                && !cli.trust_servers
                && cli.analyzer_epoch.is_none(),
            "History acquisition inherits captured scope; source/provider options are unavailable"
        );
        return args.run(&cli.root, cli.allow_writes, cli.timeout, cli.max_bytes);
    }
    if let Operation::Extensions(extension) = &operation
        && let ExtensionOperation::DigestInbox(args) = extension.as_ref()
    {
        ensure!(
            cli.snapshot.is_none()
                && cli.capture_config.is_none()
                && cli.include_ignored_dir.is_empty()
                && cli.language_server_config.is_none()
                && !cli.trust_servers
                && !cli.allow_writes
                && cli.analyzer_epoch.is_none(),
            "Digest inbox reads accept host inputs; source/provider/write flags are unavailable"
        );
        return args.run(&cli.root, cli.timeout, cli.max_bytes);
    }
    if let Operation::Extensions(extension) = &operation
        && let ExtensionOperation::DigestReceiver(args) = extension.as_ref()
    {
        ensure!(
            cli.snapshot.is_none()
                && cli.capture_config.is_none()
                && cli.include_ignored_dir.is_empty()
                && cli.language_server_config.is_none()
                && !cli.trust_servers
                && cli.analyzer_epoch.is_none(),
            "Digest receiver accepts host state/configuration, not source/provider options"
        );
        return args.run(&cli.root, cli.allow_writes, cli.timeout, cli.max_bytes);
    }
    if let Operation::Extensions(extension) = &operation
        && let ExtensionOperation::GithubApp(args) = extension.as_ref()
    {
        args.check_permission(cli.allow_writes)?;
        ensure!(
            (10..=if args.is_serve() { 86_400_000 } else { 600_000 }).contains(&cli.timeout)
                && cli.snapshot.is_none()
                && cli.capture_config.is_none()
                && cli.include_ignored_dir.is_empty()
                && cli.analyzer_epoch.is_none()
                && cli.language_server_config.is_none()
                && !cli.trust_servers,
            "github_app_input_invalid"
        );
        let deadline = Deadline::new(cli.timeout);
        if args.is_digest() {
            let prepared = args.prepare_digest(cli.max_bytes, cli.allow_writes, &deadline)?;
            return prepared.run(&cli.root, &deadline);
        }
        if args.is_pages() {
            let prepared =
                args.prepare_pages(cli.max_bytes, cli.allow_writes, cli.timeout, &deadline)?;
            validate_existing_batch_store(&cli.root, &deadline)
                .map_err(|_| anyhow::anyhow!("github_app_store_unavailable"))?;
            let engine = Engine::open_with_timeout(&cli.root, deadline.remaining())
                .map_err(|_| anyhow::anyhow!("github_app_store_unavailable"))?;
            return prepared.run(engine, &deadline);
        }
        if args.is_publication() {
            let prepared = args.prepare_publication(cli.max_bytes, cli.allow_writes, &deadline)?;
            validate_existing_batch_store(&cli.root, &deadline)
                .map_err(|_| anyhow::anyhow!("github_app_store_unavailable"))?;
            let engine = Engine::open_with_timeout(&cli.root, deadline.remaining())
                .map_err(|_| anyhow::anyhow!("github_app_store_unavailable"))?;
            return prepared.run(&engine, &deadline);
        }
        if args.is_worker() {
            let prepared = args.prepare_worker(cli.max_bytes, cli.allow_writes, &deadline)?;
            validate_existing_batch_store(&cli.root, &deadline)
                .map_err(|_| anyhow::anyhow!("github_app_store_unavailable"))?;
            let engine = Engine::open_with_timeout(&cli.root, deadline.remaining())
                .map_err(|_| anyhow::anyhow!("github_app_store_unavailable"))?;
            return prepared.run(&engine, &deadline);
        }
        if args.is_authorize() {
            ensure!(
                cli.root == std::path::Path::new("."),
                "github_app_auth_input_invalid"
            );
            return args.run_authorize(cli.max_bytes, &deadline);
        }
        if args.is_serve() {
            let prepared =
                args.prepare_service(cli.max_bytes, cli.allow_writes, cli.timeout, &deadline)?;
            validate_existing_batch_store(&cli.root, &deadline)
                .map_err(|_| anyhow::anyhow!("github_app_store_unavailable"))?;
            let engine = Engine::open_with_timeout(&cli.root, deadline.remaining())
                .map_err(|_| anyhow::anyhow!("github_app_store_unavailable"))?;
            return prepared.run(engine, &deadline);
        }
        let prepared = args.prepare(cli.max_bytes, cli.allow_writes, &deadline)?;
        validate_existing_batch_store(&cli.root, &deadline)
            .map_err(|_| anyhow::anyhow!("github_app_store_unavailable"))?;
        let engine = Engine::open_with_timeout(&cli.root, deadline.remaining())
            .map_err(|_| anyhow::anyhow!("github_app_store_unavailable"))?;
        engine.store.set_deadline(&deadline)?;
        return prepared.run(&engine, &deadline);
    }
    if let Operation::Extensions(extension) = &operation
        && let ExtensionOperation::Feedback(args) = extension.as_ref()
    {
        args.check_permission(cli.allow_writes)?;
        ensure!(
            (10..=600_000).contains(&cli.timeout)
                && cli.snapshot.is_none()
                && cli.capture_config.is_none()
                && cli.include_ignored_dir.is_empty()
                && cli.analyzer_epoch.is_none()
                && cli.language_server_config.is_none()
                && !cli.trust_servers,
            "feedback_input_invalid"
        );
        let deadline = Deadline::new(cli.timeout);
        let prepared = args.prepare(cli.max_bytes, &deadline)?;
        validate_existing_batch_store(&cli.root, &deadline)
            .map_err(|_| anyhow::anyhow!("feedback_store_unavailable"))?;
        let engine = Engine::open_with_timeout(&cli.root, deadline.remaining())?;
        engine.store.set_deadline(&deadline)?;
        return prepared.run(&engine, &deadline);
    }
    if let Operation::Extensions(extension) = &operation
        && let ExtensionOperation::MixedView(args) = extension.as_ref()
    {
        ensure!(
            (10..=600_000).contains(&cli.timeout),
            "Mixed views timeout is invalid"
        );
        ensure!(
            cli.snapshot.is_none()
                && cli.capture_config.is_none()
                && cli.include_ignored_dir.is_empty()
                && cli.analyzer_epoch.is_none()
                && cli.language_server_config.is_none()
                && !cli.trust_servers
                && !cli.allow_writes,
            "Mixed views select stored source and document spaces without capture, language servers or source writes"
        );
        let deadline = Deadline::new(cli.timeout);
        let prepared = args.prepare(cli.max_bytes, &deadline)?;
        validate_existing_batch_store(&cli.root, &deadline)
            .context("Mixed views require an existing initialized workspace store")?;
        let engine = Engine::open_with_timeout(&cli.root, deadline.remaining())?;
        engine.store.set_deadline(&deadline)?;
        return prepared.run(&engine, &deadline);
    }
    if let Operation::Extensions(extension) = &operation
        && let ExtensionOperation::Meaning(args) = extension.as_ref()
    {
        ensure!(
            (10..=600_000).contains(&cli.timeout),
            "Invalid meaning graph timeout"
        );
        ensure!(
            cli.language_server_config.is_none() && !cli.trust_servers && !cli.allow_writes,
            "Meaning graph operations do not launch language servers or apply source edits"
        );
        if !args.is_build() {
            ensure!(
                cli.snapshot.is_none()
                    && cli.capture_config.is_none()
                    && cli.include_ignored_dir.is_empty()
                    && cli.analyzer_epoch.is_none(),
                "Meaning graph reads select their stored source and do not capture or execute providers"
            );
        }
        let deadline = Deadline::new(cli.timeout);
        // Admit host permission, schemas and output before opening source state.
        let prepared = args.prepare(cli.max_bytes, &deadline)?;
        let mut capture_options = cli
            .capture_config
            .as_ref()
            .map(|path| {
                let options: yoke::capture::CaptureOptions =
                    yoke::json::decode(input_json(path, 64_000)?)?;
                options.validated()
            })
            .transpose()?
            .unwrap_or_default();
        capture_options
            .include_ignored_dirs
            .extend(cli.include_ignored_dir.iter().cloned());
        capture_options = capture_options.validated()?;
        if !args.is_build() {
            validate_existing_batch_store(&cli.root, &deadline)
                .context("Meaning graph reads require an existing initialized workspace store")?;
        }
        let mut engine = Engine::open_with_timeout(&cli.root, deadline.remaining())?;
        engine.capture_options = capture_options;
        if let Some(epoch) = &cli.analyzer_epoch {
            engine.capture_epoch =
                yoke::analysis::AnalyzerEpoch::parse(&format!("yoke-tree-sitter-{epoch}"))?;
        }
        engine.store.set_deadline(&deadline)?;
        let mut arguments = json!({"timeoutMs":cli.timeout,"maxBytes":cli.max_bytes});
        if let Some(snapshot) = &cli.snapshot {
            arguments["snapshotId"] = json!(snapshot);
        }
        return prepared.run(&mut engine, &arguments, &deadline);
    }
    if cli.analyzer_epoch.is_some() {
        ensure!(
            matches!(
                &operation,
                Operation::Index
                    | Operation::Status
                    | Operation::Find { .. }
                    | Operation::Read { .. }
                    | Operation::Outline { .. }
                    | Operation::Trace { .. }
                    | Operation::Context { .. }
                    | Operation::Path { .. }
                    | Operation::Invoke { .. }
                    | Operation::Serve
                    | Operation::Watch { .. }
                    | Operation::Export { .. }
            ) || matches!(
                &operation,
                Operation::Extensions(extension)
                    if matches!(extension.as_ref(),
                        ExtensionOperation::Git(_)
                            | ExtensionOperation::Graph { .. }
                            | ExtensionOperation::Session { .. }
                            | ExtensionOperation::ServeSession(_)
                            | ExtensionOperation::Freshness)
            ),
            "This command does not select a host analyzer epoch"
        );
    }
    if let Operation::Extensions(extension) = &operation
        && let ExtensionOperation::ModelBatchStatus { id } = extension.as_ref()
    {
        ensure!(
            yoke::model::valid_id(id),
            "Invalid embedding batch identity"
        );
        ensure!(
            cli.snapshot.is_none()
                && cli.capture_config.is_none()
                && cli.include_ignored_dir.is_empty()
                && cli.language_server_config.is_none(),
            "Batch status selects its stored source and never starts providers"
        );
        ensure!(
            (1024..=4_000_000).contains(&cli.max_bytes),
            "Batch output budget must be 1024..4000000 bytes"
        );
        ensure!(
            (10..=600_000).contains(&cli.timeout),
            "Invalid batch status timeout"
        );
        ensure!(
            cli.root.join(".yoke/index.sqlite").is_file(),
            "Batch status requires an existing workspace store"
        );
        let deadline = Deadline::new(cli.timeout);
        validate_existing_batch_store(&cli.root, &deadline)?;
        let engine = Engine::open_with_timeout(&cli.root, deadline.remaining())?;
        let data = yoke::provider_batches::describe_embedding_batch(&engine.store, id, &deadline)?;
        print_batch_result(&data, cli.max_bytes, &deadline)?;
        return Ok(true);
    }
    if let Operation::Extensions(extension) = &operation
        && let ExtensionOperation::Collections(args) = extension.as_ref()
    {
        collection_preflight(args)?;
        ensure!(
            (10..=600000).contains(&cli.timeout) && (1024..=4000000).contains(&cli.max_bytes),
            "Invalid timeout or output budget"
        );
        ensure!(
            cli.language_server_config.is_none(),
            "Collections do not launch language servers"
        );
        if matches!(
            args.operation,
            CollectionOperation::Register { view: Some(_), .. }
        ) {
            ensure!(
                cli.snapshot.is_none()
                    && cli.capture_config.is_none()
                    && cli.include_ignored_dir.is_empty(),
                "Checked collection views select their own immutable source capture"
            );
        }
        if !matches!(args.operation, CollectionOperation::Register { .. }) {
            ensure!(
                cli.snapshot.is_none()
                    && cli.capture_config.is_none()
                    && cli.include_ignored_dir.is_empty(),
                "Archived collection reads do not capture a source project"
            );
            return standalone_collections(args, &Deadline::new(cli.timeout), cli.max_bytes);
        }
    }
    if let Operation::Extensions(extension) = &operation
        && let ExtensionOperation::ModelBedrockWorker(args) = extension.as_ref()
    {
        ensure!(
            cli.snapshot.is_none()
                && cli.capture_config.is_none()
                && cli.include_ignored_dir.is_empty()
                && cli.language_server_config.is_none(),
            "Bedrock workers do not open source state or language servers"
        );
        ensure!(
            (10..=600_000).contains(&cli.timeout),
            "Invalid model worker deadline"
        );
        return bedrock_cli::worker(args, &Deadline::new(cli.timeout));
    }
    if let Operation::Extensions(extension) = &operation
        && let ExtensionOperation::ModelHttpWorker(args) = extension.as_ref()
    {
        ensure!(
            cli.snapshot.is_none()
                && cli.capture_config.is_none()
                && cli.include_ignored_dir.is_empty()
                && cli.language_server_config.is_none(),
            "HTTP model workers do not open source state or language servers"
        );
        ensure!(
            (10..=600_000).contains(&cli.timeout),
            "Invalid model worker deadline"
        );
        let deadline = Deadline::new(cli.timeout);
        let (_, spec, _, _) = load_http_model(
            &args.config,
            args.trust_network,
            args.allow_loopback_http,
            Some(&args.config_hash),
        )?;
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
        print(&yoke::model_http::provider_reply(
            &spec,
            &input,
            &yoke::host_credentials::read_model_token,
            &deadline,
        )?)?;
        return Ok(true);
    }
    if let Operation::Extensions(extension) = &operation
        && let ExtensionOperation::ServeHttp(args) = extension.as_ref()
    {
        ensure!(
            cli.snapshot.is_none()
                && cli.capture_config.is_none()
                && cli.include_ignored_dir.is_empty()
                && cli.language_server_config.is_none(),
            "HTTP targets select snapshots, capture options and servers through their host configuration"
        );
        return run_http(&cli.root, args, cli.trust_servers, cli.allow_writes);
    }
    if let Operation::Credentials { ref command } = operation {
        let value = match command {
            CredentialOperation::AwsImport { name } => {
                ensure!(
                    !io::stdin().is_terminal(),
                    "AWS credentials require a private stdin pipe"
                );
                let mut bytes = zeroize::Zeroizing::new(Vec::new());
                io::stdin().lock().take(70_001).read_to_end(&mut bytes)?;
                ensure!(
                    bytes.len() <= 70_000,
                    "AWS credential input exceeds capacity"
                );
                yoke::host_credentials::import_aws_session_json(
                    name,
                    &bytes,
                    &Deadline::new(cli.timeout),
                )?
            }
            CredentialOperation::Tls { name, hostnames } => {
                yoke::host_credentials::generate_tls(name, hostnames)?
            }
            CredentialOperation::Token { name } => yoke::host_credentials::generate_token(name)?,
            CredentialOperation::ModelImport { name } => {
                ensure!(
                    !io::stdin().is_terminal(),
                    "Model credentials require a private stdin pipe"
                );
                let mut bytes = zeroize::Zeroizing::new(Vec::new());
                io::stdin().lock().take(2563).read_to_end(&mut bytes)?;
                ensure!(
                    bytes.len() <= 2562,
                    "Model credential input exceeds capacity"
                );
                if bytes.last() == Some(&b'\n') {
                    bytes.pop();
                    if bytes.last() == Some(&b'\r') {
                        bytes.pop();
                    }
                }
                yoke::host_credentials::import_model_token(name, &bytes)?
            }
            CredentialOperation::GithubAppKeyImport { name } => {
                ensure!(
                    !io::stdin().is_terminal(),
                    "GitHub App keys require a private stdin pipe"
                );
                let mut bytes = zeroize::Zeroizing::new(Vec::new());
                io::stdin().lock().take(8193).read_to_end(&mut bytes)?;
                ensure!(bytes.len() <= 8192, "github_app_key_invalid");
                yoke::host_credentials::import_github_app_key(name, &bytes)?
            }
            CredentialOperation::GithubWebhookImport { name } => {
                ensure!(
                    !io::stdin().is_terminal(),
                    "GitHub webhook secrets require a private stdin pipe"
                );
                let mut bytes = zeroize::Zeroizing::new(Vec::new());
                io::stdin().lock().take(2561).read_to_end(&mut bytes)?;
                ensure!(bytes.len() <= 2560, "github_app_credential_invalid");
                yoke::host_credentials::import_github_webhook_secret(name, &bytes)?
            }
            CredentialOperation::GithubImport { name } => {
                ensure!(
                    !io::stdin().is_terminal(),
                    "GitHub credentials require a private stdin pipe, never command arguments or an echoed terminal"
                );
                let mut bytes = zeroize::Zeroizing::new(Vec::new());
                io::stdin().lock().take(2563).read_to_end(&mut bytes)?;
                ensure!(
                    bytes.len() <= 2562,
                    "GitHub credential input exceeds capacity"
                );
                if bytes.last() == Some(&b'\n') {
                    bytes.pop();
                    if bytes.last() == Some(&b'\r') {
                        bytes.pop();
                    }
                }
                yoke::host_credentials::import_github_token(name, &bytes)?
            }
        };
        print(&value)?;
        return Ok(true);
    }
    if let Operation::Federation {
        command: FederationOperation::Keygen { ref name },
    } = operation
    {
        print(&yoke::credentials::generate(name)?)?;
        return Ok(true);
    }
    if let Operation::MediaPdfWorker { ref input } = operation {
        print(&yoke::media::pdf_worker(input)?)?;
        return Ok(true);
    }
    if let Operation::ModelWorker {
        ref endpoint,
        ref model,
        ref digest,
    } = operation
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
    if let Operation::Extensions(extension) = &mut operation
        && let ExtensionOperation::Decisions(args) = extension.as_mut()
    {
        ensure!(
            cli.language_server_config.is_none(),
            "Decision record operations do not launch language servers"
        );
        args.prepared = Some(decision_preflight(args, &cli.root)?);
    }
    if let Operation::Extensions(extension) = &operation
        && let ExtensionOperation::Skills(skills) = extension.as_ref()
    {
        ensure!(
            cli.snapshot.is_none()
                && cli.capture_config.is_none()
                && cli.include_ignored_dir.is_empty()
                && cli.language_server_config.is_none(),
            "Authored skills do not select a source snapshot or provider configuration"
        );
        ensure!(
            yoke::model::valid_id(&skills.project),
            "Skill project must be an explicit stable SHA-256 ID"
        );
        let args = yoke::json::parse(skills.args.as_bytes(), 128000)?;
        ensure!(args.is_object(), "Skill arguments must be an object");
        for root in [&cli.root, &skills.personal_root] {
            ensure!(
                root.is_absolute(),
                "Skill host roots must be absolute existing directories"
            );
            ensure!(
                !root
                    .components()
                    .any(|part| matches!(part, std::path::Component::ParentDir)),
                "Skill host roots cannot contain parent traversal"
            );
            for ancestor in root.ancestors() {
                let meta = fs::symlink_metadata(ancestor)?;
                ensure!(
                    meta.is_dir() && !yoke::paths::is_link(&meta),
                    "Skill host root ancestors must be nonlinked directories"
                );
            }
        }
        let deadline = Deadline::new(cli.timeout);
        let store =
            yoke::store::Store::open_with_timeout(&skills.personal_root, deadline.remaining())?;
        store.set_deadline(&deadline)?;
        let access = yoke::skill_takeaways::SkillAccess::new(
            &store,
            &skills.project,
            &cli.root,
            skills.allow_personal_writes,
            cli.allow_writes,
        )?;
        let args =
            yoke::skill_takeaways::prepare_arguments(&access, &skills.operation, &args, &deadline)?;
        let value = yoke::skill_takeaways::invoke(&access, &skills.operation, &args, &deadline)?;
        let bytes = serde_json::to_vec(&value)?;
        if (bytes.len() as u64).saturating_add(1) > cli.max_bytes {
            let artifact = store.artifact(&bytes)?;
            print(&json!({
                "status":"partial", "artifactId":artifact, "totalBytes":bytes.len(),
                "readWith":{"root":skills.personal_root,"tool":"ci_read","args":{"artifactId":artifact}},
                "warning":"Complete skill result retained as an immutable artifact in the explicit personal store."
            }))?;
            return Ok(true);
        }
        print(&value)?;
        return Ok(true);
    }
    if let Operation::Extensions(extension) = &operation
        && let ExtensionOperation::Document(document) = extension.as_ref()
    {
        let standalone = match &document.operation {
            DocumentOperation::QueryArchive {
                directory,
                operation,
                args,
            } => Some((directory, Some((operation, args)), None)),
            DocumentOperation::ExplorerArchive { directory, output } => {
                Some((directory, None, Some(output)))
            }
            _ => None,
        };
        if let Some((directory, query, output)) = standalone {
            ensure!(
                cli.snapshot.is_none()
                    && cli.capture_config.is_none()
                    && cli.include_ignored_dir.is_empty()
                    && cli.language_server_config.is_none(),
                "Standalone document archives do not select a source snapshot or provider configuration"
            );
            let deadline = Deadline::new(cli.timeout);
            let graph = yoke::document_graph::load_archive(directory, &deadline)?;
            let result = if let Some((operation, args)) = query {
                document_query(&graph, operation, args, &deadline)?
            } else {
                write_document_explorer(
                    &graph,
                    output.ok_or_else(|| anyhow::anyhow!("Document output absent"))?,
                    &deadline,
                )?
            };
            ensure!(
                (serde_json::to_vec(&result)?.len() as u64).saturating_add(1) <= cli.max_bytes,
                "Standalone document result exceeds --max-bytes; select a smaller page or source range"
            );
            print(&result)?;
            return Ok(true);
        }
    }
    if let Operation::Position {
        text,
        byte,
        line,
        character,
    } = operation
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
    let prepared_http = match &operation {
        Operation::Extensions(extension) => match extension.as_ref() {
            ExtensionOperation::ModelsHttp(args) => Some(PreparedModelProvider {
                provider: http_process_provider(args)?,
                embedding_batch: None,
            }),
            ExtensionOperation::ModelsBedrock(args) => Some(bedrock_cli::process_provider(args)?),
            _ => None,
        },
        _ => None,
    };
    let deadline = Deadline::new(cli.timeout);
    let mut engine = Engine::open_with_timeout(&cli.root, deadline.remaining())?;
    engine.allow_writes = cli.allow_writes;
    if let Some(epoch) = cli.analyzer_epoch {
        engine.capture_epoch =
            yoke::analysis::AnalyzerEpoch::parse(&format!("yoke-tree-sitter-{epoch}"))?;
    }
    if let Some(config) = cli.capture_config {
        let options: yoke::capture::CaptureOptions =
            yoke::json::decode(input_json(&config, 64000)?)?;
        engine.capture_options = options.validated()?;
    }
    if !cli.include_ignored_dir.is_empty() {
        engine
            .capture_options
            .include_ignored_dirs
            .extend(cli.include_ignored_dir);
        engine.capture_options = engine.capture_options.validated()?;
    }
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
            #[serde(default)]
            configuration: yoke::lsp::HostConfiguration,
        }
        let servers: Vec<ConfiguredServer> = yoke::json::decode(input_json(&config, 1_000_000)?)?;
        ensure!(servers.len() <= 100, "Too many configured servers");
        for server in servers {
            engine.register_configured_server(yoke::lsp::ConfiguredServer {
                server: yoke::lsp::Server {
                    command: server.command,
                    args: server.args,
                    language: server.language,
                    trust: true,
                },
                configuration: server.configuration,
            })?;
        }
    }
    engine.store.set_deadline(&deadline)?;
    let mut arguments = json!({"timeoutMs":cli.timeout,"maxBytes":cli.max_bytes});
    if let Some(snapshot) = cli.snapshot {
        arguments["snapshotId"] = json!(snapshot);
    }
    if let Operation::Extensions(operation) = operation {
        return run_extension(engine, prepared_http, *operation, &arguments, &deadline);
    }
    let tool = match operation {
        Operation::Extensions(_) => bail!("Internal extension dispatch error"),
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
            let data =
                invoke_model_operation(&mut engine, command, &provider, &arguments, &deadline)?;
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
                    let items = yoke::federation::find(
                        &engine,
                        &remote_id,
                        &grant,
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
            print_bounded(&engine, &data, cli.max_bytes)?;
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
            if format == "html" {
                yoke::export::write_explorer(&engine.store, &snapshot, &graph, &output, &deadline)?;
            } else {
                yoke::export::write(&graph, &format, &output)?;
            }
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
            let plan =
                yoke::changes::approve_at(&engine.store, &plan, allow_test_changes, &deadline)?;
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
    match run(Cli::parse()) {
        Ok(true) => {}
        Ok(false) => std::process::exit(1),
        Err(error) => {
            eprintln!("yoke: {error:#}");
            std::process::exit(1);
        }
    }
}
