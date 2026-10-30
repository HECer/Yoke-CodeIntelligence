//! Host-file admission and managed one-attempt Bedrock worker entry points.
use super::{ModelOperation, PreparedModelProvider, print};
use anyhow::{Context, Result, ensure};
use base64::Engine as _;
use std::{
    fs,
    io::{self, Read},
    path::PathBuf,
};
use yoke::{
    capture::Deadline,
    model_bedrock::{self as bedrock, BedrockSpec},
};

#[derive(clap::Args)]
pub(super) struct Arguments {
    #[arg(long)]
    pub config: PathBuf,
    #[arg(long)]
    pub trust_network: bool,
    #[arg(long)]
    pub allow_loopback_http: bool,
    #[command(subcommand)]
    pub operation: ModelOperation,
}
#[derive(clap::Args)]
pub(super) struct WorkerArguments {
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
struct HostModel {
    endpoint: String,
    region: String,
    remote_model_id: String,
    model_id: String,
    model_version: String,
    credential_name: String,
    dialect: Dialect,
    task_id: String,
    currency: String,
    max_cost_micros: u64,
    task_limit_micros: u64,
    #[serde(default)]
    root_certificates_der_base64: Vec<String>,
    #[serde(default)]
    embedding_batch: Option<HostEmbeddingBatch>,
}
#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct HostEmbeddingBatch {
    max_calls: usize,
    max_entities: usize,
    max_total_source_bytes: usize,
}
#[derive(serde::Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
enum Dialect {
    Converse {
        output: Output,
        max_output_tokens: u32,
        #[serde(default)]
        vision: bool,
    },
    TitanV1,
    TitanV2 {
        dimensions: usize,
        normalize: bool,
    },
    CohereV3,
    CohereV4 {
        dimensions: usize,
    },
}
#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
enum Output {
    JsonSchema,
    TextJson,
}

fn load(
    path: &std::path::Path,
    trusted: bool,
    loopback: bool,
    expected: Option<&str>,
) -> Result<(HostModel, BedrockSpec, PathBuf, String)> {
    ensure!(trusted, "Bedrock model execution requires --trust-network");
    let path = fs::canonicalize(path)?;
    let mut bytes = Vec::new();
    fs::File::open(&path)?
        .take(64_001)
        .read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() <= 64_000,
        "Bedrock provider profile exceeds capacity"
    );
    let hash = yoke::model::hash(&bytes);
    if let Some(expected) = expected {
        ensure!(
            yoke::model::valid_id(expected) && expected == hash,
            "Bedrock provider profile changed"
        );
    }
    let config: HostModel = yoke::json::decode(yoke::json::parse(&bytes, 64_000)?)?;
    let dialect = match config.dialect {
        Dialect::Converse {
            ref output,
            max_output_tokens,
            vision,
        } => bedrock::Dialect::Converse {
            output: match output {
                Output::JsonSchema => bedrock::Output::JsonSchema,
                Output::TextJson => bedrock::Output::TextJson,
            },
            max_output_tokens,
            vision,
        },
        Dialect::TitanV1 => bedrock::Dialect::TitanV1,
        Dialect::TitanV2 {
            dimensions,
            normalize,
        } => bedrock::Dialect::TitanV2 {
            dimensions,
            normalize,
        },
        Dialect::CohereV3 => bedrock::Dialect::CohereV3,
        Dialect::CohereV4 { dimensions } => bedrock::Dialect::CohereV4 { dimensions },
    };
    ensure!(
        config.root_certificates_der_base64.len() <= 8,
        "TLS root count exceeds capacity"
    );
    let mut roots = Vec::new();
    for root in &config.root_certificates_der_base64 {
        ensure!(root.len() <= 21_848, "TLS root exceeds capacity");
        roots.push(
            base64::prelude::BASE64_STANDARD
                .decode(root)
                .map_err(|_| anyhow::anyhow!("Invalid TLS root encoding"))?,
        );
    }
    let spec = BedrockSpec {
        trusted_network: trusted,
        endpoint: config.endpoint.clone(),
        region: config.region.clone(),
        remote_model_id: config.remote_model_id.clone(),
        model: config.model_id.clone(),
        model_version: config.model_version.clone(),
        credential_name: config.credential_name.clone(),
        dialect,
        allow_loopback_http: loopback,
        additional_root_certificates: roots,
    };
    bedrock::validate_spec(&spec)?;
    if let Some(batch) = &config.embedding_batch {
        yoke::provider_batches::EmbeddingBatchPolicy::for_bedrock(
            &spec,
            batch.max_calls,
            batch.max_entities,
            batch.max_total_source_bytes,
        )?;
    }
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

pub(super) fn process_provider(args: &Arguments) -> Result<PreparedModelProvider> {
    let (config, spec, path, hash) = load(
        &args.config,
        args.trust_network,
        args.allow_loopback_http,
        None,
    )?;
    let supported = match spec.dialect {
        bedrock::Dialect::Converse { .. } => matches!(
            args.operation,
            ModelOperation::Summarize { .. }
                | ModelOperation::Rerank { .. }
                | ModelOperation::Media { .. }
        ),
        _ => matches!(
            args.operation,
            ModelOperation::Embeddings | ModelOperation::Search { .. }
        ),
    };
    ensure!(supported, "Bedrock dialect does not support this operation");
    let embedding_batch = config
        .embedding_batch
        .as_ref()
        .map(|batch| {
            yoke::provider_batches::EmbeddingBatchPolicy::for_bedrock(
                &spec,
                batch.max_calls,
                batch.max_entities,
                batch.max_total_source_bytes,
            )
        })
        .transpose()?;
    let mut worker_args = vec![
        "model-bedrock-worker".into(),
        "--config".into(),
        path.to_str()
            .context("Bedrock profile path must be UTF-8")?
            .into(),
        "--config-hash".into(),
        hash,
        "--trust-network".into(),
    ];
    if args.allow_loopback_http {
        worker_args.push("--allow-loopback-http".into());
    }
    let spec = yoke::providers::ProviderSpec {
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
    spec.model_key()?;
    Ok(PreparedModelProvider {
        provider: spec,
        embedding_batch,
    })
}

pub(super) fn worker(args: &WorkerArguments, deadline: &Deadline) -> Result<bool> {
    let (_, spec, _, _) = load(
        &args.config,
        args.trust_network,
        args.allow_loopback_http,
        Some(&args.config_hash),
    )?;
    let (sender, receiver) = std::sync::mpsc::sync_channel(1);
    std::thread::spawn(move || {
        let result = (|| -> io::Result<Vec<u8>> {
            let mut bytes = Vec::new();
            io::stdin().take(16_000_001).read_to_end(&mut bytes)?;
            Ok(bytes)
        })();
        let _ = sender.send(result);
    });
    let input = receiver
        .recv_timeout(deadline.remaining())
        .map_err(|_| anyhow::anyhow!("Bedrock worker input deadline exceeded"))??;
    deadline.check()?;
    ensure!(
        input.len() <= 16_000_000,
        "Bedrock worker request exceeds capacity"
    );
    print(&bedrock::provider_reply(
        &spec,
        &input,
        &yoke::host_credentials::read_aws_session,
        deadline,
    )?)?;
    Ok(true)
}
