//! Explicit arXiv acquisition and received metadata; never source authority.
use crate::{capture::Deadline, source_acquisition::ReceivedSource};
use anyhow::Result;
use serde_json::Value;
pub fn inspect(_source: &ReceivedSource, _max_bytes: usize, _deadline: &Deadline) -> Result<Value> {
    anyhow::bail!("arxiv_source_unavailable")
}
