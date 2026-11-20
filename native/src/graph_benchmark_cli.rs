use anyhow::{Result, ensure};
use clap::Args;
use std::{
    io::{self, Write},
    path::Path,
};
use yoke::{capture::Deadline, query, store::Store};

pub(super) const INPUT: &str =
    "graph_benchmark_requires_an_existing_snapshot_without_capture_or_provider_flags";

#[derive(Args)]
pub(super) struct Arguments {
    /// Repeat to measure selected context questions instead of the five defaults.
    #[arg(long = "question")]
    questions: Vec<String>,
    /// Use a supplied corpus word count instead of counting captured text.
    #[arg(long)]
    corpus_words: Option<u64>,
    /// Maximum number of results for each context query.
    #[arg(long, default_value_t = 5, value_parser = clap::value_parser!(u64).range(1..=20))]
    limit: u64,
}

impl Arguments {
    pub(super) fn run(
        &self,
        root: &Path,
        snapshot_id: &str,
        timeout: u64,
        output_bytes: u64,
    ) -> Result<bool> {
        ensure!(
            (1..=600_000).contains(&timeout) && (2..=4_000_000).contains(&output_bytes),
            "Graph benchmark deadline or output budget is out of range"
        );
        let deadline = Deadline::new(timeout);
        let store = Store::open_existing_read_only(root, &deadline)?;
        let report = query::token_benchmark(
            &store,
            snapshot_id,
            &self.questions,
            self.corpus_words,
            self.limit,
            &deadline,
        )?;
        let mut output = yoke::json::encode_bounded(&report, output_bytes as usize - 1, &deadline)?;
        output.push(b'\n');
        deadline.check()?;
        let mut stdout = io::stdout().lock();
        stdout.write_all(&output)?;
        stdout.flush()?;
        Ok(true)
    }
}
