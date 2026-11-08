//! Separate explicit trusted operation; ordinary mixed-view queries stay model-free.
use anyhow::{Result, ensure};
use clap::{Args, Subcommand};
use serde::Deserialize;
use std::{io::Write, path::PathBuf};
use yoke::{
    Engine,
    capture::Deadline,
    mixed_view::{
        self, MeaningInputRequest, NativePdfWorker, Options, Selection, SelectionV2,
        provider::{self, Policy, SourceSelection},
    },
    providers::ProviderSpec,
};

#[derive(Args)]
pub struct Arguments {
    #[command(subcommand)]
    operation: Operation,
}
#[derive(Subcommand)]
enum Operation {
    /// Run one fixed fully admitted trusted-host request; never retries.
    Build {
        #[arg(long)]
        selection: PathBuf,
        #[arg(long)]
        inputs: PathBuf,
        #[arg(long)]
        config: PathBuf,
        #[arg(long)]
        policy: PathBuf,
        #[arg(long)]
        trust_provider: bool,
        #[arg(long)]
        native_pdf_text: bool,
    },
    /// Inspect original response/accounting metadata without any process.
    Status { job_id: String },
    /// Recover identities from bounded recorded metadata after lost output.
    Jobs {
        #[arg(long)]
        task: String,
        #[arg(long, default_value_t = 0)]
        offset: usize,
        #[arg(long, default_value_t = 20)]
        limit: usize,
    },
    /// Replay source/response without a provider; PDF text retains its own permission.
    Load {
        job_id: String,
        #[arg(long)]
        native_pdf_text: bool,
    },
    /// Query one source-replayed graph without a provider or inferred hyperedge flow.
    Query {
        job_id: String,
        #[arg(long)]
        query: PathBuf,
        #[arg(long)]
        native_pdf_text: bool,
    },
    /// Compose explicitly selected checked jobs; host links remain unverified.
    Compose {
        #[arg(long = "job-id", required = true)]
        job_ids: Vec<String>,
        #[arg(long)]
        review: PathBuf,
        #[arg(long)]
        query: Option<PathBuf>,
        #[arg(long, requires = "query")]
        include_host_links: bool,
        #[arg(long, default_value_t = 64_000_000)]
        max_input_bytes: usize,
        #[arg(long)]
        native_pdf_text: bool,
    },
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct HostPolicy {
    contract_hash: String,
    #[serde(default)]
    provider_input_files: Vec<PathBuf>,
    #[serde(default = "default_graph")]
    max_graph_bytes: usize,
}
fn default_graph() -> usize {
    16_000_000
}
struct HostBuild {
    selection: SourceSelection,
    inputs: MeaningInputRequest,
    provider: ProviderSpec,
    policy: Policy,
    pdf: Option<NativePdfWorker>,
}
struct HostComposition {
    job_ids: Vec<String>,
    review: provider::composition::Review,
    query: Option<provider::query::Query>,
    include_host_links: bool,
    max_input_bytes: usize,
    pdf: Option<NativePdfWorker>,
}
enum Selected {
    Build(Box<HostBuild>),
    Compose(Box<HostComposition>),
    Status(String),
    Jobs {
        task: String,
        offset: usize,
        limit: usize,
    },
    Load {
        id: String,
        pdf: Option<NativePdfWorker>,
    },
    Query {
        id: String,
        query: provider::query::Query,
        pdf: Option<NativePdfWorker>,
    },
}
pub struct Prepared {
    selected: Selected,
    max_bytes: usize,
}
fn host_file<T: serde::de::DeserializeOwned>(
    path: &std::path::Path,
    code: &'static str,
) -> Result<T> {
    yoke::json::decode(super::input_json(path, 64_000).map_err(|_| anyhow::anyhow!(code))?)
        .map_err(|_| anyhow::anyhow!(code))
}
fn pdf_worker(enabled: bool, d: &Deadline) -> Result<Option<NativePdfWorker>> {
    enabled
        .then(|| NativePdfWorker::new(&std::env::current_exe()?, d))
        .transpose()
}
impl Arguments {
    pub fn prepare(&self, max_bytes: u64, d: &Deadline) -> Result<Prepared> {
        ensure!(
            (1024..=64_000_000).contains(&max_bytes),
            "Mixed meaning output budget must be 1024..64000000 bytes"
        );
        d.check()?;
        let selected = match &self.operation {
            Operation::Compose {
                job_ids,
                review,
                query,
                include_host_links,
                max_input_bytes,
                native_pdf_text,
            } => {
                ensure!(
                    (2..=4).contains(&job_ids.len())
                        && job_ids.iter().all(|id| yoke::model::valid_id(id))
                        && job_ids
                            .iter()
                            .collect::<std::collections::BTreeSet<_>>()
                            .len()
                            == job_ids.len()
                        && (1..=64_000_000).contains(max_input_bytes),
                    "mixed_meaning_composition_invalid"
                );
                let review: provider::composition::Review =
                    host_file(review, "mixed_meaning_composition_invalid")?;
                provider::composition::validate_review(&review, d)?;
                ensure!(
                    review.graph_ids.len() == job_ids.len(),
                    "mixed_meaning_composition_invalid"
                );
                let query = query
                    .as_ref()
                    .map(|p| host_file::<provider::query::Query>(p, "mixed_meaning_query_invalid"))
                    .transpose()?;
                if let Some(q) = &query {
                    provider::query::validate(q, d)?;
                }
                Selected::Compose(Box::new(HostComposition {
                    job_ids: job_ids.clone(),
                    review,
                    query,
                    include_host_links: *include_host_links,
                    max_input_bytes: *max_input_bytes,
                    pdf: pdf_worker(*native_pdf_text, d)?,
                }))
            }
            Operation::Build {
                selection,
                inputs,
                config,
                policy,
                trust_provider,
                native_pdf_text,
            } => {
                ensure!(
                    *trust_provider,
                    "Mixed meaning extraction requires --trust-provider"
                );
                let config: super::HostProvider =
                    host_file(config, "mixed_meaning_host_config_invalid")?;
                let policy: HostPolicy = host_file(policy, "mixed_meaning_policy_invalid")?;
                let provider = ProviderSpec {
                    trusted: true,
                    command: config.command,
                    args: config.args,
                    model_id: config.model_id,
                    model_version: config.model_version,
                    task_id: config.task_id,
                    currency: config.currency,
                    max_cost_micros: config.max_cost_micros,
                    task_limit_micros: config.task_limit_micros,
                };
                let policy = Policy {
                    contract_hash: policy.contract_hash,
                    provider_input_files: policy.provider_input_files,
                    max_graph_bytes: policy.max_graph_bytes,
                };
                provider::validate_host_configuration(&provider, &policy)?;
                let value = super::input_json(selection, 64_000)
                    .map_err(|_| anyhow::anyhow!("mixed_meaning_selection_invalid"))?;
                let inputs: MeaningInputRequest =
                    host_file(inputs, "mixed_meaning_inputs_invalid")?;
                let selection = if value["schemaVersion"] == 2 {
                    let selection: SelectionV2 = yoke::json::decode(value)
                        .map_err(|_| anyhow::anyhow!("mixed_meaning_selection_invalid"))?;
                    mixed_view::validate_pdf_selection(&selection, &Options::default(), d)?;
                    mixed_view::validate_pdf_meaning_inputs(&inputs, &selection, d)?;
                    ensure!(*native_pdf_text, "mixed_view_pdf_permission_required");
                    SourceSelection::Pdf { selection }
                } else {
                    ensure!(!native_pdf_text, "mixed_meaning_selection_invalid");
                    let selection: Selection = yoke::json::decode(value)
                        .map_err(|_| anyhow::anyhow!("mixed_meaning_selection_invalid"))?;
                    mixed_view::validate_selection(&selection, &Options::default(), d)?;
                    mixed_view::validate_meaning_inputs(&inputs, &selection, d)?;
                    SourceSelection::Office { selection }
                };
                let placeholder = "f".repeat(64);
                let reference = serde_json::json!({"kind":"mixed_meaning_build_reference_v1","jobId":placeholder,"state":"complete",
                    "graphArtifactId":placeholder,"preparedInputHash":placeholder,"resultTransport":"durable_mixed_meaning_records",
                    "readWith":{"command":"mixed-meaning status","jobId":placeholder},"resolution":"inferred_concept",
                    "providerAttestation":"unverified","complete":false,"sourceWritten":false});
                yoke::json::encode_bounded(&reference, max_bytes as usize - 1, d)?;
                Selected::Build(Box::new(HostBuild {
                    selection,
                    inputs,
                    provider,
                    policy,
                    pdf: pdf_worker(*native_pdf_text, d)?,
                }))
            }
            Operation::Status { job_id } => {
                ensure!(
                    yoke::model::valid_id(job_id),
                    "mixed_meaning_job_id_invalid"
                );
                Selected::Status(job_id.clone())
            }
            Operation::Jobs {
                task,
                offset,
                limit,
            } => {
                ensure!(
                    !task.is_empty()
                        && task.len() <= 200
                        && !task.chars().any(char::is_control)
                        && *offset <= 4096
                        && (1..=100).contains(limit),
                    "mixed_meaning_job_query_invalid"
                );
                Selected::Jobs {
                    task: task.clone(),
                    offset: *offset,
                    limit: *limit,
                }
            }
            Operation::Query {
                job_id,
                query,
                native_pdf_text,
            } => {
                ensure!(
                    yoke::model::valid_id(job_id),
                    "mixed_meaning_job_id_invalid"
                );
                let query: provider::query::Query =
                    host_file(query, "mixed_meaning_query_invalid")?;
                provider::query::validate(&query, d)?;
                Selected::Query {
                    id: job_id.clone(),
                    query,
                    pdf: pdf_worker(*native_pdf_text, d)?,
                }
            }
            Operation::Load {
                job_id,
                native_pdf_text,
            } => {
                ensure!(
                    yoke::model::valid_id(job_id),
                    "mixed_meaning_job_id_invalid"
                );
                Selected::Load {
                    id: job_id.clone(),
                    pdf: pdf_worker(*native_pdf_text, d)?,
                }
            }
        };
        d.check()?;
        Ok(Prepared {
            selected,
            max_bytes: max_bytes as usize,
        })
    }
}
impl Prepared {
    pub fn run(&self, engine: &Engine, d: &Deadline) -> Result<bool> {
        let result = match &self.selected {
            Selected::Compose(selected) => {
                let mut graphs = Vec::new();
                for id in &selected.job_ids {
                    d.check()?;
                    let status = provider::status(&engine.store, id, d)?;
                    let needs_pdf = status["requiresNativePdfText"]
                        .as_bool()
                        .ok_or_else(|| anyhow::anyhow!("mixed_meaning_record_invalid"))?;
                    graphs.push(provider::load_checked(
                        &engine.store,
                        id,
                        if needs_pdf {
                            selected.pdf.as_ref()
                        } else {
                            None
                        },
                        d,
                    )?);
                }
                let composition = provider::composition::compose(
                    graphs,
                    &selected.review,
                    selected.max_input_bytes,
                    d,
                )?;
                if let Some(q) = &selected.query {
                    composition.query(q, selected.include_host_links, self.max_bytes - 1, d)?
                } else {
                    composition.export(self.max_bytes - 1, d)?
                }
            }
            Selected::Build(build) => provider::build(
                &engine.store,
                &build.selection,
                &build.inputs,
                build.pdf.as_ref(),
                &build.provider,
                &build.policy,
                d,
            )?,
            Selected::Status(id) => provider::status(&engine.store, id, d)?,
            Selected::Jobs {
                task,
                offset,
                limit,
            } => provider::jobs(&engine.store, task, *offset, *limit, d)?,
            Selected::Load { id, pdf } => provider::load(&engine.store, id, pdf.as_ref(), d)?,
            Selected::Query { id, query, pdf } => {
                provider::load_checked(&engine.store, id, pdf.as_ref(), d)?.query(
                    query,
                    self.max_bytes - 1,
                    d,
                )?
            }
        };
        let emitted = (|| -> Result<bool> {
            let bytes = yoke::json::encode_bounded(&result, self.max_bytes - 1, d)?;
            d.check()?;
            let mut stdout = std::io::stdout().lock();
            stdout.write_all(&bytes)?;
            stdout.write_all(b"\n")?;
            stdout.flush()?;
            Ok(true)
        })();
        emitted.map_err(|error| {
            if matches!(&self.selected,Selected::Build(_)) {
                anyhow::anyhow!("mixed_meaning_publication_committed; job={}; output unavailable; inspect mixed-meaning status/jobs",
                    result["jobId"].as_str().unwrap_or("unknown"))
            } else {error}
        })
    }
}
