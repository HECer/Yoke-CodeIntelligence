//! Host-selected portable JSON, without a source workspace or Store.
use anyhow::{Result, ensure};
use clap::Args;
use std::{io::Write, path::PathBuf};
use yoke::{
    capture::Deadline,
    meaning_projection::{self, Options, Projection, Query},
};

#[derive(Args)]
pub struct Arguments {
    #[arg(long)]
    input: PathBuf,
    /// Strict node, degree or directed trace query; omitted means inspect all.
    #[arg(long)]
    query: Option<String>,
    #[arg(long, default_value_t = 4_000_000)]
    max_input_bytes: usize,
    #[arg(long, default_value_t = 4096)]
    max_concepts: usize,
    #[arg(long, default_value_t = 32768)]
    max_relations: usize,
    #[arg(long, default_value_t = 8192)]
    max_hyperedges: usize,
    #[arg(long, default_value_t = 65536)]
    max_participants: usize,
    #[arg(long, default_value_t = 65536)]
    max_evidence: usize,
}
impl Arguments {
    pub fn run(&self, timeout: u64, max_bytes: u64) -> Result<bool> {
        ensure!(
            (10..=600_000).contains(&timeout),
            "Invalid portable projection timeout"
        );
        ensure!(
            (1024..=meaning_projection::MAX_BYTES as u64).contains(&max_bytes),
            "Invalid portable projection output budget"
        );
        let d = Deadline::new(timeout);
        let options = Options {
            max_input_bytes: self.max_input_bytes,
            max_concepts: self.max_concepts,
            max_relations: self.max_relations,
            max_hyperedges: self.max_hyperedges,
            max_participants: self.max_participants,
            max_evidence: self.max_evidence,
        };
        meaning_projection::validate_options(&options)?;
        let query: Option<Query> = self
            .query
            .as_ref()
            .map(|q| yoke::json::parse(q.as_bytes(), 16_384).and_then(yoke::json::decode))
            .transpose()?;
        let bytes =
            super::meaning_archive_cli::read_input(&self.input, options.max_input_bytes, &d)?;
        let projection = Projection::parse(&bytes, &options, &d)?;
        let limit = max_bytes as usize - 1;
        let value = if let Some(query) = &query {
            projection.query(query, limit, &d)?
        } else {
            projection.inspect(limit, &d)?
        };
        let mut output = yoke::json::encode_bounded(&value, limit, &d)?;
        output.push(b'\n');
        d.check()?;
        std::io::stdout().lock().write_all(&output)?;
        Ok(true)
    }
}
