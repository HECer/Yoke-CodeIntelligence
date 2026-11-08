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
    /// Replay source/response without a provider; PDF text retains its own permission.
    Load {
        job_id: String,
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
enum Selected {
    Build(Box<HostBuild>),
    Status(String),
    Load {
        id: String,
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
            Selected::Load { id, pdf } => provider::load(&engine.store, id, pdf.as_ref(), d)?,
        };
        let bytes = yoke::json::encode_bounded(&result, self.max_bytes - 1, d)?;
        d.check()?;
        let mut stdout = std::io::stdout().lock();
        stdout.write_all(&bytes)?;
        stdout.write_all(b"\n")?;
        stdout.flush()?;
        Ok(true)
    }
}
