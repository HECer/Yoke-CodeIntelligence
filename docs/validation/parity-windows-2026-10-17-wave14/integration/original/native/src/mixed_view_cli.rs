//! Explicit host selection; ordinary mixed reads never capture or publish.
use anyhow::{Result, ensure};
use clap::Args;
use std::{io::Write, path::PathBuf};
use yoke::{
    Engine,
    capture::Deadline,
    mixed_view::{self, Options, Query, Selection},
};
#[derive(Args)]
pub struct Arguments {
    #[arg(long)]
    selection: PathBuf,
    #[arg(long)]
    query: Option<String>,
    #[arg(long, default_value_t = 4)]
    max_documents: usize,
    #[arg(long, default_value_t = 64_000_000)]
    max_metadata_bytes: usize,
    #[arg(long, default_value_t = 64_000_000)]
    max_source_bytes: usize,
    #[arg(long, default_value_t = 100_000)]
    max_matches: usize,
}
pub struct Prepared {
    selection: Selection,
    query: Query,
    options: Options,
}
impl Arguments {
    pub fn prepare(&self, max_bytes: u64, d: &Deadline) -> Result<Prepared> {
        ensure!(
            (1024..=4_000_000).contains(&max_bytes),
            "Mixed views output budget must be 1024..4000000 bytes"
        );
        d.check()?;
        let options = Options {
            max_documents: self.max_documents,
            max_metadata_bytes: self.max_metadata_bytes,
            max_source_bytes: self.max_source_bytes,
            max_matches: self.max_matches,
            max_output_bytes: max_bytes as usize - 1,
        };
        mixed_view::validate_options(&options)?;
        let selection: Selection = yoke::json::decode(
            super::input_json(&self.selection, mixed_view::MAX_SELECTION_BYTES as u64)
                .map_err(|_| anyhow::anyhow!("mixed_view_input_invalid"))?,
        )
        .map_err(|_| anyhow::anyhow!("mixed_view_input_invalid"))?;
        mixed_view::validate_selection(&selection, &options, d)?;
        let query = match &self.query {
            None => Query::Describe {},
            Some(raw) => yoke::json::decode(
                yoke::json::parse(raw.as_bytes(), 16_000)
                    .map_err(|_| anyhow::anyhow!("mixed_view_input_invalid"))?,
            )
            .map_err(|_| anyhow::anyhow!("mixed_view_input_invalid"))?,
        };
        mixed_view::validate_query(&query, &selection, d)?;
        d.check()?;
        Ok(Prepared {
            selection,
            query,
            options,
        })
    }
}
impl Prepared {
    pub fn run(&self, engine: &Engine, d: &Deadline) -> Result<bool> {
        let view = mixed_view::load(&engine.store, &self.selection, &self.options, d)?;
        let result = view.query(&self.query, self.options.max_output_bytes, d)?;
        let bytes = yoke::json::encode_bounded(&result, self.options.max_output_bytes, d)
            .map_err(|_| anyhow::anyhow!("mixed_view_capacity_exceeded"))?;
        d.check()?;
        let mut stdout = std::io::stdout().lock();
        stdout.write_all(&bytes)?;
        stdout.write_all(b"\n")?;
        stdout.flush()?;
        Ok(true)
    }
}
