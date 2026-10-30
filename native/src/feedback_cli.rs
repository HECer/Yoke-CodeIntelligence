//! Explicit feedback input admission precedes source Store access and publication.
use anyhow::{Context as _, Result, ensure};
use clap::{Args, Subcommand};
use serde::Deserialize;
use std::{
    fs,
    io::{Read, Write},
    path::{Component, Path, PathBuf},
};
use yoke::{
    Engine,
    capture::Deadline,
    knowledge::feedback::{self, Draft, Options, RankInput, semantic_transcript as semantic},
    providers::ProviderSpec,
};

#[derive(Args)]
pub struct Arguments {
    #[command(subcommand)]
    operation: Operation,
    #[arg(long, global = true, default_value_t = 10_000)]
    max_events: usize,
    #[arg(long, global = true, default_value_t = 64_000_000)]
    max_input_bytes: usize,
    #[arg(long, global = true, default_value_t = 64_000_000)]
    max_source_bytes: usize,
    #[arg(long, global = true, default_value_t = 32)]
    max_pairs: usize,
    #[arg(long, global = true, default_value_t = 10_000)]
    max_candidates: usize,
}
#[derive(Subcommand)]
enum Operation {
    /// Explicit host model attempt over selected imported UTF-8 bytes.
    SemanticMine {
        #[arg(long)]
        request: PathBuf,
        #[arg(long)]
        config: PathBuf,
        #[arg(long)]
        policy: PathBuf,
        #[arg(long)]
        trust_provider: bool,
    },
    /// Replay saved candidates without provider configuration or execution.
    SemanticLoad {
        #[arg(long)]
        job: String,
    },
    SemanticStatus {
        #[arg(long)]
        job: String,
    },
    SemanticJobs {
        #[arg(long)]
        task: String,
        #[arg(long, default_value_t = 0)]
        offset: usize,
        #[arg(long, default_value_t = 25)]
        limit: usize,
    },
    MeaningRecord {
        #[arg(long)]
        input: PathBuf,
    },
    MeaningRank {
        #[arg(long)]
        input: PathBuf,
    },
    PlanRecord {
        #[arg(long)]
        input: PathBuf,
    },
    PlanRank {
        #[arg(long)]
        input: PathBuf,
    },
    Record {
        #[arg(long)]
        input: PathBuf,
    },
    Rank {
        #[arg(long)]
        input: PathBuf,
    },
    ImportTranscript {
        #[arg(long)]
        input: PathBuf,
        #[arg(long)]
        label: String,
        #[arg(long)]
        author: String,
    },
    Mine {
        #[arg(long)]
        transcript: String,
    },
}
enum Input {
    SemanticMine(Box<SemanticBuild>),
    SemanticLoad(String),
    SemanticStatus(String),
    SemanticJobs {
        task: String,
        offset: usize,
        limit: usize,
    },
    MeaningRecord(Box<feedback::meaning::Draft>),
    MeaningRank(Box<feedback::meaning::RankInput>),
    PlanRecord(Box<feedback::plans::Draft>),
    PlanRank(Box<feedback::plans::RankInput>),
    Record(Box<Draft>),
    Rank(Box<RankInput>),
    Transcript {
        bytes: Vec<u8>,
        label: String,
        author: String,
    },
    Mine(String),
}
struct SemanticBuild {
    request: semantic::Request,
    provider: ProviderSpec,
    policy: semantic::Policy,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SemanticPolicy {
    contract_hash: String,
    #[serde(default)]
    provider_input_files: Vec<PathBuf>,
    #[serde(default = "semantic_result_bytes")]
    max_result_bytes: usize,
}
fn semantic_result_bytes() -> usize {
    4_000_000
}
pub struct Prepared {
    input: Input,
    options: Options,
}
pub(crate) fn host_bytes(path: &Path, maximum: usize, d: &Deadline) -> Result<Vec<u8>> {
    d.check()?;
    ensure!(
        !path.components().any(|c| matches!(c, Component::ParentDir)),
        "feedback_input_invalid"
    );
    let absolute = if path.is_absolute() {
        path.to_owned()
    } else {
        std::env::current_dir()?.join(path)
    };
    #[cfg(windows)]
    ensure!(
        absolute.components().all(|component| match component {
            Component::Prefix(prefix) => matches!(prefix.kind(), std::path::Prefix::Disk(_)),
            _ => true,
        }),
        "feedback_input_invalid"
    );
    for component in absolute.components() {
        if let Component::Normal(name) = component {
            let name = name
                .to_str()
                .ok_or_else(|| anyhow::anyhow!("feedback_input_invalid"))?;
            ensure!(
                yoke::paths::normalized(name)? == name,
                "feedback_input_invalid"
            );
        }
    }
    for ancestor in absolute.ancestors() {
        d.check()?;
        ensure!(
            !yoke::paths::is_link(&fs::symlink_metadata(ancestor)?),
            "feedback_input_invalid"
        );
    }
    let canonical = fs::canonicalize(&absolute)?;
    for component in canonical.components() {
        if let Component::Normal(name) = component {
            let name = name
                .to_str()
                .ok_or_else(|| anyhow::anyhow!("feedback_input_invalid"))?;
            ensure!(
                yoke::paths::normalized(name)? == name,
                "feedback_input_invalid"
            );
        }
    }
    let metadata = fs::symlink_metadata(&canonical)?;
    ensure!(
        metadata.is_file() && metadata.len() <= maximum as u64,
        "feedback_input_invalid"
    );
    let mut file = fs::File::open(&canonical)?;
    let opened = file.metadata()?;
    ensure!(
        opened.is_file() && opened.len() == metadata.len(),
        "feedback_input_invalid"
    );
    let mut bytes = Vec::new();
    (&mut file)
        .take(maximum as u64 + 1)
        .read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() <= maximum
            && bytes.len() as u64 == opened.len()
            && file.metadata()?.len() == opened.len(),
        "feedback_input_invalid"
    );
    d.check()?;
    Ok(bytes)
}
impl Arguments {
    pub fn check_permission(&self, allow_writes: bool) -> Result<()> {
        let write = matches!(
            self.operation,
            Operation::Record { .. }
                | Operation::MeaningRecord { .. }
                | Operation::PlanRecord { .. }
                | Operation::ImportTranscript { .. }
                | Operation::SemanticMine { .. }
        );
        if write {
            ensure!(allow_writes, "feedback_write_permission_required");
        } else {
            ensure!(!allow_writes, "feedback_input_invalid");
        }
        if let Operation::SemanticMine { trust_provider, .. } = &self.operation {
            ensure!(
                *trust_provider,
                "transcript_semantic_provider_permission_required"
            );
        }
        Ok(())
    }
    pub fn prepare(&self, max_bytes: u64, d: &Deadline) -> Result<Prepared> {
        ensure!(
            (1024..=4_000_000).contains(&max_bytes),
            "feedback_input_invalid"
        );
        d.check()?;
        let options = Options {
            max_events: self.max_events,
            max_input_bytes: self.max_input_bytes,
            max_source_bytes: self.max_source_bytes,
            max_pairs: self.max_pairs,
            max_candidates: self.max_candidates,
            max_output_bytes: max_bytes as usize - 1,
        };
        feedback::validate_options(&options)?;
        let input = (|| -> Result<Input> {
            Ok(match &self.operation {
                Operation::SemanticMine {
                    request,
                    config,
                    policy,
                    trust_provider,
                } => {
                    ensure!(
                        *trust_provider,
                        "transcript_semantic_provider_permission_required"
                    );
                    let r: semantic::Request = yoke::json::decode(yoke::json::parse(
                        &host_bytes(request, 64_000, d)?,
                        64_000,
                    )?)?;
                    let p: super::HostProvider = yoke::json::decode(yoke::json::parse(
                        &host_bytes(config, 64_000, d)?,
                        64_000,
                    )?)?;
                    let policy: SemanticPolicy = yoke::json::decode(yoke::json::parse(
                        &host_bytes(policy, 128_000, d)?,
                        128_000,
                    )?)?;
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
                    let policy = semantic::Policy {
                        contract_hash: policy.contract_hash,
                        provider_input_files: policy.provider_input_files,
                        max_result_bytes: policy.max_result_bytes,
                    };
                    semantic::validate_request(&r, d)?;
                    semantic::validate_host_configuration(&provider, &policy, d)?;
                    Input::SemanticMine(Box::new(SemanticBuild {
                        request: r,
                        provider,
                        policy,
                    }))
                }
                Operation::SemanticLoad { job } | Operation::SemanticStatus { job } => {
                    ensure!(yoke::model::valid_id(job), "feedback_input_invalid");
                    if matches!(&self.operation, Operation::SemanticLoad { .. }) {
                        Input::SemanticLoad(job.clone())
                    } else {
                        Input::SemanticStatus(job.clone())
                    }
                }
                Operation::SemanticJobs {
                    task,
                    offset,
                    limit,
                } => {
                    ensure!(
                        !task.trim().is_empty()
                            && task.len() <= 200
                            && !task.chars().any(char::is_control)
                            && (1..=100).contains(limit),
                        "feedback_input_invalid"
                    );
                    Input::SemanticJobs {
                        task: task.clone(),
                        offset: *offset,
                        limit: *limit,
                    }
                }
                Operation::MeaningRecord { input } => {
                    let bytes = host_bytes(input, feedback::MAX_DRAFT_BYTES, d)?;
                    let draft =
                        yoke::json::decode(yoke::json::parse(&bytes, feedback::MAX_DRAFT_BYTES)?)?;
                    feedback::meaning::validate_draft(&draft, d)?;
                    Input::MeaningRecord(Box::new(draft))
                }
                Operation::MeaningRank { input } => {
                    let bytes = host_bytes(input, feedback::MAX_DRAFT_BYTES, d)?;
                    let query =
                        yoke::json::decode(yoke::json::parse(&bytes, feedback::MAX_DRAFT_BYTES)?)?;
                    feedback::meaning::validate_rank(&query, d)?;
                    Input::MeaningRank(Box::new(query))
                }
                Operation::PlanRecord { input } => {
                    let bytes = host_bytes(input, feedback::MAX_DRAFT_BYTES, d)?;
                    let draft =
                        yoke::json::decode(yoke::json::parse(&bytes, feedback::MAX_DRAFT_BYTES)?)?;
                    feedback::plans::validate_draft(&draft, d)?;
                    Input::PlanRecord(Box::new(draft))
                }
                Operation::PlanRank { input } => {
                    let bytes = host_bytes(input, feedback::MAX_DRAFT_BYTES, d)?;
                    let query =
                        yoke::json::decode(yoke::json::parse(&bytes, feedback::MAX_DRAFT_BYTES)?)?;
                    feedback::plans::validate_rank(&query, d)?;
                    Input::PlanRank(Box::new(query))
                }
                Operation::Record { input } => {
                    let bytes = host_bytes(input, feedback::MAX_DRAFT_BYTES, d)?;
                    let draft: Draft =
                        yoke::json::decode(yoke::json::parse(&bytes, feedback::MAX_DRAFT_BYTES)?)?;
                    feedback::validate_draft(&draft, d)?;
                    Input::Record(Box::new(draft))
                }
                Operation::Rank { input } => {
                    let bytes = host_bytes(input, feedback::MAX_DRAFT_BYTES, d)?;
                    let query: RankInput =
                        yoke::json::decode(yoke::json::parse(&bytes, feedback::MAX_DRAFT_BYTES)?)?;
                    feedback::validate_rank(&query, d)?;
                    Input::Rank(Box::new(query))
                }
                Operation::ImportTranscript {
                    input,
                    label,
                    author,
                } => {
                    let bytes = host_bytes(input, feedback::MAX_TRANSCRIPT_BYTES, d)?;
                    feedback::validate_transcript(&bytes, label, author, &options, d)?;
                    Input::Transcript {
                        bytes,
                        label: label.clone(),
                        author: author.clone(),
                    }
                }
                Operation::Mine { transcript } => {
                    ensure!(yoke::model::valid_id(transcript), "feedback_input_invalid");
                    Input::Mine(transcript.clone())
                }
            })
        })();
        d.check()?;
        let input = input.map_err(|_| anyhow::anyhow!("feedback_input_invalid"))?;
        Ok(Prepared { input, options })
    }
}
impl Prepared {
    pub fn run(&self, engine: &Engine, d: &Deadline) -> Result<bool> {
        let result = match &self.input {
            Input::SemanticMine(b) => semantic::build(
                &engine.store,
                &b.request,
                &b.provider,
                &b.policy,
                &self.options,
                d,
            )?,
            Input::SemanticLoad(id) => semantic::load(&engine.store, id, &self.options, d)?
                .export(self.options.max_output_bytes, d)?,
            Input::SemanticStatus(id) => semantic::status(&engine.store, id, &self.options, d)?,
            Input::SemanticJobs {
                task,
                offset,
                limit,
            } => semantic::jobs(&engine.store, task, *offset, *limit, &self.options, d)?,
            Input::MeaningRecord(input) => {
                feedback::meaning::record(&engine.store, input, &self.options, d)?
            }
            Input::MeaningRank(input) => {
                feedback::meaning::rank(&engine.store, input, &self.options, d)?
            }
            Input::PlanRecord(input) => {
                feedback::plans::record(&engine.store, input, &self.options, d)?
            }
            Input::PlanRank(input) => {
                feedback::plans::rank(&engine.store, input, &self.options, d)?
            }
            Input::Record(input) => feedback::record(&engine.store, input, &self.options, d)?,
            Input::Rank(input) => feedback::rank(&engine.store, input, &self.options, d)?,
            Input::Transcript {
                bytes,
                label,
                author,
            } => {
                feedback::import_transcript(&engine.store, bytes, label, author, &self.options, d)?
            }
            Input::Mine(id) => feedback::mine(&engine.store, id, &self.options, d)?,
        };
        let emitted = (|| -> Result<()> {
            let bytes = yoke::json::encode_bounded(&result, self.options.max_output_bytes, d)
                .map_err(|_| anyhow::anyhow!("feedback_capacity_exceeded"))?;
            d.check()?;
            let mut stdout = std::io::stdout().lock();
            stdout.write_all(&bytes)?;
            stdout.write_all(b"\n")?;
            stdout.flush()?;
            Ok(())
        })();
        if matches!(&self.input, Input::SemanticMine(_)) {
            emitted.with_context(|| format!("transcript_semantic_publication_committed: job {}; inspect feedback semantic-status; do not retry the build automatically", result["jobId"].as_str().unwrap_or("unknown")))?;
        } else {
            emitted?;
        }
        Ok(true)
    }
}
