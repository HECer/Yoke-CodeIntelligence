//! Independent explicit host admission; stored requests cannot grant execution.
use anyhow::{Context as _, Result, ensure};
use clap::{Args, Subcommand};
use serde::Deserialize;
use std::{io::Write, path::PathBuf};
use yoke::{Engine, capture::Deadline, meaning_identity as api, providers::ProviderSpec};
#[derive(Args)]
pub struct Arguments {
    #[command(subcommand)]
    operation: Operation,
    #[arg(long, global = true, default_value_t = 128)]
    max_concepts: usize,
    #[arg(long, global = true, default_value_t = 8128)]
    max_pairs: usize,
    #[arg(long, global = true, default_value_t = 256)]
    max_candidates: usize,
    #[arg(long, global = true, default_value_t = 64_000_000)]
    max_similarity_work: u64,
    #[arg(long, global = true, default_value_t = 64_000_000)]
    max_input_bytes: usize,
}
#[derive(Subcommand)]
enum Operation {
    Candidates {
        #[arg(long)]
        request: PathBuf,
    },
    Build {
        #[arg(long)]
        request: PathBuf,
        #[arg(long)]
        config: PathBuf,
        #[arg(long)]
        policy: PathBuf,
        #[arg(long)]
        trust_provider: bool,
    },
    Load {
        #[arg(long)]
        job: String,
        #[arg(long)]
        query: Option<String>,
    },
    Status {
        #[arg(long)]
        job: String,
    },
    Jobs {
        #[arg(long)]
        task: String,
        #[arg(long, default_value_t = 0)]
        offset: usize,
        #[arg(long, default_value_t = 25)]
        limit: usize,
    },
}
enum Input {
    Candidates(Box<api::Request>),
    Build(Box<Build>),
    Load {
        job: String,
        query: Option<api::Query>,
    },
    Status(String),
    Jobs {
        task: String,
        offset: usize,
        limit: usize,
    },
}
struct Build {
    request: api::Request,
    provider: ProviderSpec,
    policy: api::Policy,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Policy {
    contract_hash: String,
    #[serde(default)]
    provider_input_files: Vec<PathBuf>,
    #[serde(default = "result_bytes")]
    max_result_bytes: usize,
}
fn result_bytes() -> usize {
    16_000_000
}
fn read<T: serde::de::DeserializeOwned>(
    path: &std::path::Path,
    max: usize,
    d: &Deadline,
) -> Result<T> {
    yoke::json::decode(yoke::json::parse(
        &super::feedback_cli::host_bytes(path, max, d)?,
        max,
    )?)
}
pub struct Prepared {
    input: Input,
    options: api::Options,
}
impl Arguments {
    pub fn check_permission(&self, allow_writes: bool) -> Result<()> {
        if let Operation::Build { trust_provider, .. } = &self.operation {
            ensure!(allow_writes, "meaning_identity_write_permission_required");
            ensure!(
                *trust_provider,
                "meaning_identity_provider_permission_required"
            );
        } else {
            ensure!(!allow_writes, "meaning_identity_input_invalid");
        }
        Ok(())
    }
    pub fn prepare(&self, max: u64, d: &Deadline) -> Result<Prepared> {
        ensure!(
            (1024..=64_000_000).contains(&max),
            "meaning_identity_input_invalid"
        );
        d.check()?;
        let options = api::Options {
            max_concepts: self.max_concepts,
            max_pairs: self.max_pairs,
            max_candidates: self.max_candidates,
            max_similarity_work: self.max_similarity_work,
            max_input_bytes: self.max_input_bytes,
            max_output_bytes: max as usize - 1,
        };
        api::validate_options(&options, d)?;
        let input = (|| -> Result<Input> {
            Ok(match &self.operation {
                Operation::Candidates { request } => {
                    let r = read(request, 128_000, d)?;
                    api::validate_request(&r, d)?;
                    Input::Candidates(Box::new(r))
                }
                Operation::Build {
                    request,
                    config,
                    policy,
                    trust_provider,
                } => {
                    ensure!(
                        *trust_provider,
                        "meaning_identity_provider_permission_required"
                    );
                    let r = read(request, 128_000, d)?;
                    let p: super::HostProvider = read(config, 64_000, d)?;
                    let policy: Policy = read(policy, 128_000, d)?;
                    let provider = ProviderSpec {
                        trusted: true,
                        command: p.command,
                        args: p.args,
                        model_id: p.model_id,
                        model_version: p.model_version,
                        task_id: p.task_id,
                        currency: p.currency,
                        max_cost_micros: p.max_cost_micros,
                        task_limit_micros: p.task_limit_micros,
                    };
                    let policy = api::Policy {
                        contract_hash: policy.contract_hash,
                        provider_input_files: policy.provider_input_files,
                        max_result_bytes: policy.max_result_bytes,
                    };
                    api::validate_request(&r, d)?;
                    api::validate_host_configuration(&provider, &policy, d)?;
                    Input::Build(Box::new(Build {
                        request: r,
                        provider,
                        policy,
                    }))
                }
                Operation::Load { job, query } => {
                    ensure!(yoke::model::valid_id(job), "meaning_identity_input_invalid");
                    let query = query
                        .as_ref()
                        .map(|q| -> Result<api::Query> {
                            let q = yoke::json::decode(yoke::json::parse(q.as_bytes(), 64_000)?)?;
                            api::validate_query_shape(&q, d)?;
                            Ok(q)
                        })
                        .transpose()?;
                    Input::Load {
                        job: job.clone(),
                        query,
                    }
                }
                Operation::Status { job } => {
                    ensure!(yoke::model::valid_id(job), "meaning_identity_input_invalid");
                    Input::Status(job.clone())
                }
                Operation::Jobs {
                    task,
                    offset,
                    limit,
                } => {
                    ensure!(
                        !task.trim().is_empty()
                            && task.len() <= 200
                            && !task.chars().any(char::is_control)
                            && (1..=100).contains(limit),
                        "meaning_identity_input_invalid"
                    );
                    Input::Jobs {
                        task: task.clone(),
                        offset: *offset,
                        limit: *limit,
                    }
                }
            })
        })();
        d.check()?;
        let input = input.map_err(|_| anyhow::anyhow!("meaning_identity_input_invalid"))?;
        Ok(Prepared { input, options })
    }
}
impl Prepared {
    pub fn run(&self, engine: &Engine, d: &Deadline) -> Result<bool> {
        let value = match &self.input {
            Input::Candidates(r) => api::candidates(&engine.store, r, &self.options, d)?
                .export(self.options.max_output_bytes, d)?,
            Input::Build(b) => api::build(
                &engine.store,
                &b.request,
                &b.provider,
                &b.policy,
                &self.options,
                d,
            )?,
            Input::Load { job, query } => {
                let graph = api::load(&engine.store, job, &self.options, d)?;
                if let Some(q) = query {
                    graph.query(q, self.options.max_output_bytes, d)?
                } else {
                    graph.export(self.options.max_output_bytes, d)?
                }
            }
            Input::Status(id) => api::status(&engine.store, id, &self.options, d)?,
            Input::Jobs {
                task,
                offset,
                limit,
            } => api::jobs(&engine.store, task, *offset, *limit, &self.options, d)?,
        };
        let emitted = (|| -> Result<()> {
            let bytes = yoke::json::encode_bounded(&value, self.options.max_output_bytes, d)?;
            d.check()?;
            let mut stdout = std::io::stdout().lock();
            stdout.write_all(&bytes)?;
            stdout.write_all(b"\n")?;
            stdout.flush()?;
            Ok(())
        })();
        if matches!(&self.input, Input::Build(_)) {
            emitted.with_context(||format!("meaning_identity_publication_committed: job {}; inspect meaning-identity status; never retry a send automatically",value["jobId"].as_str().unwrap_or("unknown")))?;
        } else {
            emitted?;
        }
        Ok(true)
    }
}
