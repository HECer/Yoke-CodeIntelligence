//! Explicit, reviewed Neo4j graph publication; no query-time network acquisition.
use crate::{
    capture::Deadline,
    model::{Graph, Snapshot},
    store::Store,
};
use anyhow::Result;
use serde::{Deserialize, Serialize};
use serde_json::Value;
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Limits {
    pub max_nodes: usize,
    pub max_observations: usize,
    pub max_evidence: usize,
    pub max_bytes: usize,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            max_nodes: 5000,
            max_observations: 50000,
            max_evidence: 50000,
            max_bytes: 16_000_000,
        }
    }
}
pub fn build_payload(
    _store: &Store,
    _snapshot: &Snapshot,
    _graph: &Graph,
    _limits: &Limits,
    _deadline: &Deadline,
) -> Result<Value> {
    anyhow::bail!("neo4j_publish_unavailable")
}
