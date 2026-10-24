//! Explicit trusted-host dispatch for source-bound inferred meaning.
use anyhow::{Context, Result, ensure};
use clap::{Args, Subcommand};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{io::Write, path::PathBuf};
use yoke::{
    Engine,
    capture::Deadline,
    meaning_build::{self, MeaningPolicy, MeaningSelection},
    providers::ProviderSpec,
};

#[derive(Args)]
pub struct Arguments {
    #[command(subcommand)]
    operation: Operation,
}

#[derive(Subcommand)]
enum Operation {
    /// Extract one completely admitted host selection; no automatic retry.
    Build {
        #[arg(long)]
        config: PathBuf,
        #[arg(long)]
        policy: PathBuf,
        #[arg(long)]
        trust_provider: bool,
    },
    /// Replay a published graph without starting or resolving a provider.
    Load { graph_id: String },
    /// Find literal concept text and preserve each matching packet's evidence.
    Query { graph_id: String, term: String },
    /// Propose lexical aliases from a replayed graph without merging concepts.
    Aliases {
        graph_id: String,
        #[arg(long, default_value_t = 9000)]
        min_score_bps: u16,
        #[arg(long, default_value_t = 512)]
        max_concepts: usize,
        #[arg(long, default_value_t = 65_536)]
        max_pairs: usize,
        #[arg(long, default_value_t = 4096)]
        max_candidates: usize,
        #[arg(long, default_value_t = 16_000_000)]
        max_similarity_work: u64,
    },
    /// Project explicit host-selected groups without changing the parent graph.
    Group {
        #[arg(long)]
        review: PathBuf,
        #[arg(long, default_value_t = 256)]
        max_groups: usize,
        #[arg(long, default_value_t = 4096)]
        max_members: usize,
        #[arg(long, default_value_t = 1024)]
        max_concepts: usize,
        #[arg(long, default_value_t = 8192)]
        max_relations: usize,
        #[arg(long, default_value_t = 2048)]
        max_hyperedges: usize,
        #[arg(long, default_value_t = 16384)]
        max_participants: usize,
        #[arg(long, default_value_t = 16384)]
        max_evidence: usize,
    },
    /// Inspect a stored job; comparison never captures current source.
    Status {
        job_id: String,
        #[arg(long)]
        compare_snapshot: Option<String>,
    },
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct HostPolicy {
    contract_hash: String,
    generation: String,
    selection: MeaningSelection,
    #[serde(default)]
    context_groups: Vec<Vec<String>>,
    #[serde(default)]
    provider_input_files: Vec<PathBuf>,
    #[serde(default = "default_packets")]
    max_packets: usize,
    #[serde(default = "default_source_bytes")]
    max_total_source_bytes: usize,
    #[serde(default = "default_graph_bytes")]
    max_graph_bytes: usize,
}
fn default_packets() -> usize {
    64
}
fn default_source_bytes() -> usize {
    4_000_000
}
fn default_graph_bytes() -> usize {
    64_000_000
}

enum PreparedOperation {
    Build {
        provider: Box<ProviderSpec>,
        policy: MeaningPolicy,
    },
    Load(String),
    Query {
        graph_id: String,
        term: String,
    },
    Aliases {
        graph_id: String,
        options: yoke::meaning_aliases::Options,
    },
    Group {
        review: Box<yoke::meaning_groups::Review>,
        options: yoke::meaning_groups::Options,
    },
    Status {
        job_id: String,
        compare_snapshot: Option<String>,
    },
}

pub struct Prepared {
    operation: PreparedOperation,
    max_bytes: usize,
}

fn valid_identity(id: &str) -> Result<()> {
    ensure!(yoke::model::valid_id(id), "Invalid meaning record identity");
    Ok(())
}

fn build_reference(job_id: &str, graph_id: &str, snapshot_id: &str) -> Value {
    json!({
        "kind":"meaning_build_reference", "status":"partial", "buildStatus":"ready",
        "jobId":job_id, "graphId":graph_id, "snapshotId":snapshot_id,
        "resultTransport":"durable_meaning_records",
        "readWith":{"command":"meaning status", "jobId":job_id, "maxBytes":4_000_000},
        "resolution":"inferred_concept", "providerAttestation":"unverified"
    })
}

impl Arguments {
    pub fn is_build(&self) -> bool {
        matches!(self.operation, Operation::Build { .. })
    }

    /// Pure host admission plus bounded host-file reads, before opening a Store.
    /// Executable resolution and all source/cost admission remain in the builder.
    pub fn prepare(&self, max_bytes: u64, deadline: &Deadline) -> Result<Prepared> {
        ensure!(
            (1024..=4_000_000).contains(&max_bytes),
            "Meaning output budget must be 1024..4000000 bytes"
        );
        deadline.check()?;
        let max_bytes = max_bytes as usize;
        let operation = match &self.operation {
            Operation::Build {
                config,
                policy,
                trust_provider,
            } => {
                ensure!(
                    *trust_provider,
                    "Meaning extraction requires --trust-provider"
                );
                let config: super::HostProvider =
                    yoke::json::decode(super::input_json(config, 64_000)?)?;
                let selected: HostPolicy = yoke::json::decode(super::input_json(policy, 128_000)?)?;
                deadline.check()?;
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
                let policy = MeaningPolicy {
                    contract_hash: selected.contract_hash,
                    generation: selected.generation,
                    selection: selected.selection,
                    context_groups: selected.context_groups,
                    provider_input_files: selected.provider_input_files,
                    max_packets: selected.max_packets,
                    max_total_source_bytes: selected.max_total_source_bytes,
                    max_graph_bytes: selected.max_graph_bytes,
                };
                meaning_build::validate_host_configuration(&provider, &policy)?;
                // Every emitted build reference has exactly these bounded fields.
                // Admit it before a worker can start or a graph can be published.
                let placeholder = "f".repeat(64);
                let reference = build_reference(&placeholder, &placeholder, &placeholder);
                yoke::json::encode_bounded(&reference, max_bytes - 1, deadline)
                    .context("Meaning output budget cannot carry a durable build reference")?;
                PreparedOperation::Build {
                    provider: Box::new(provider),
                    policy,
                }
            }
            Operation::Load { graph_id } => {
                valid_identity(graph_id)?;
                PreparedOperation::Load(graph_id.clone())
            }
            Operation::Query { graph_id, term } => {
                valid_identity(graph_id)?;
                ensure!(
                    !term.trim().is_empty()
                        && term.len() <= 1000
                        && !term.chars().any(char::is_control)
                        && !yoke::paths::obvious_secret(term),
                    "Invalid meaning query text"
                );
                PreparedOperation::Query {
                    graph_id: graph_id.clone(),
                    term: term.clone(),
                }
            }
            Operation::Aliases {
                graph_id,
                min_score_bps,
                max_concepts,
                max_pairs,
                max_candidates,
                max_similarity_work,
            } => {
                valid_identity(graph_id)?;
                let options = yoke::meaning_aliases::Options {
                    min_score_bps: *min_score_bps,
                    max_concepts: *max_concepts,
                    max_pairs: *max_pairs,
                    max_candidates: *max_candidates,
                    max_similarity_work: *max_similarity_work,
                    max_output_bytes: max_bytes - 1,
                };
                yoke::meaning_aliases::validate_options(&options)?;
                PreparedOperation::Aliases {
                    graph_id: graph_id.clone(),
                    options,
                }
            }
            Operation::Group {
                review,
                max_groups,
                max_members,
                max_concepts,
                max_relations,
                max_hyperedges,
                max_participants,
                max_evidence,
            } => {
                let options = yoke::meaning_groups::Options {
                    max_groups: *max_groups,
                    max_members: *max_members,
                    max_concepts: *max_concepts,
                    max_relations: *max_relations,
                    max_hyperedges: *max_hyperedges,
                    max_participants: *max_participants,
                    max_evidence: *max_evidence,
                    max_output_bytes: max_bytes - 1,
                };
                yoke::meaning_groups::validate_options(&options)?;
                let review: yoke::meaning_groups::Review = yoke::json::decode(super::input_json(
                    review,
                    yoke::meaning_groups::MAX_REVIEW_BYTES as u64,
                )?)?;
                yoke::meaning_groups::validate_review(&review, &options, deadline)?;
                PreparedOperation::Group {
                    review: Box::new(review),
                    options,
                }
            }
            Operation::Status {
                job_id,
                compare_snapshot,
            } => {
                valid_identity(job_id)?;
                if let Some(id) = compare_snapshot {
                    valid_identity(id)?;
                }
                PreparedOperation::Status {
                    job_id: job_id.clone(),
                    compare_snapshot: compare_snapshot.clone(),
                }
            }
        };
        deadline.check()?;
        Ok(Prepared {
            operation,
            max_bytes,
        })
    }
}

fn write(bytes: &[u8], deadline: &Deadline) -> Result<()> {
    deadline.check()?;
    let mut stdout = std::io::stdout().lock();
    stdout.write_all(bytes)?;
    stdout.write_all(b"\n")?;
    stdout.flush()?;
    Ok(())
}

impl Prepared {
    /// The caller enforces existing-store reads and rejects incompatible global
    /// options before Engine construction. Only Build calls Engine::select.
    pub fn run(
        &self,
        engine: &mut Engine,
        source_arguments: &Value,
        deadline: &Deadline,
    ) -> Result<bool> {
        match &self.operation {
            PreparedOperation::Build { provider, policy } => {
                let (snapshot, graph) = engine.select(source_arguments, deadline)?;
                let result = meaning_build::build(
                    &engine.store,
                    &snapshot,
                    &graph,
                    provider,
                    policy,
                    deadline,
                )?;
                let job = result["jobId"]
                    .as_str()
                    .context("Durable meaning job identity missing")?;
                let graph = result["graphId"]
                    .as_str()
                    .context("Durable meaning graph identity missing")?;
                let snapshot = result["snapshotId"]
                    .as_str()
                    .context("Durable meaning snapshot identity missing")?;
                let deliver = || -> Result<()> {
                    valid_identity(job)?;
                    valid_identity(graph)?;
                    valid_identity(snapshot)?;
                    let bytes =
                        match yoke::json::encode_bounded(&result, self.max_bytes - 1, deadline) {
                            Ok(bytes) => bytes,
                            Err(_) => {
                                deadline.check()?;
                                yoke::json::encode_bounded(
                                    &build_reference(job, graph, snapshot),
                                    self.max_bytes - 1,
                                    deadline,
                                )?
                            }
                        };
                    write(&bytes, deadline)
                };
                deliver().with_context(|| {
                    format!("Meaning job {job} graph {graph} is durable; output transport failed")
                })?;
            }
            operation => {
                let result = match operation {
                    PreparedOperation::Load(id) => {
                        meaning_build::load(&engine.store, id, deadline)?
                    }
                    PreparedOperation::Query { graph_id, term } => {
                        meaning_build::query(&engine.store, graph_id, term, deadline)?
                    }
                    PreparedOperation::Aliases { graph_id, options } => {
                        yoke::meaning_aliases::discover(&engine.store, graph_id, options, deadline)?
                    }
                    PreparedOperation::Group { review, options } => {
                        yoke::meaning_groups::project(&engine.store, review, options, deadline)?
                    }
                    PreparedOperation::Status {
                        job_id,
                        compare_snapshot,
                    } => meaning_build::status(
                        &engine.store,
                        job_id,
                        compare_snapshot.as_deref(),
                        deadline,
                    )?,
                    PreparedOperation::Build { .. } => unreachable!(),
                };
                // Read commands never create an overflow artifact or a new record.
                let bytes = yoke::json::encode_bounded(&result, self.max_bytes - 1, deadline)
                    .context("Meaning result exceeds output budget; increase --max-bytes or narrow the query")?;
                write(&bytes, deadline)?;
            }
        }
        Ok(true)
    }
}
