//! Explicit, reviewed Neo4j graph publication; no query-time network acquisition.
#[path = "neo4j_publish_checks.rs"]
mod checks;
#[path = "neo4j_publish_client.rs"]
mod client;
#[path = "neo4j_publish_journal.rs"]
mod journal;
#[path = "neo4j_publish_plan.rs"]
pub(crate) mod plan;
#[path = "neo4j_publish_program.rs"]
mod program;
#[path = "neo4j_publish_transport.rs"]
mod transport;
use crate::{
    capture::Deadline,
    model::{Graph, Snapshot},
    store::Store,
};
use anyhow::{Result, ensure};
pub use client::{apply, approve, inspect_journal, inspect_server, recover};
pub use plan::{Mode, Plan, Target};
pub use program::{Operation, operations, request};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
pub use transport::Permissions;
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
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
    store: &Store,
    snapshot: &Snapshot,
    graph: &Graph,
    limits: &Limits,
    deadline: &Deadline,
) -> Result<Value> {
    deadline.check()?;
    let hard = Limits::default();
    ensure!(
        limits.max_nodes > 0
            && limits.max_nodes <= hard.max_nodes
            && limits.max_observations <= hard.max_observations
            && limits.max_evidence <= hard.max_evidence
            && limits.max_bytes > 0
            && limits.max_bytes <= hard.max_bytes,
        "neo4j_publish_capacity_exceeded"
    );
    ensure!(
        graph.entities.len() <= limits.max_nodes
            && graph.observations.len() <= limits.max_observations
            && graph.evidence.len() <= limits.max_evidence
            && snapshot.files.len() <= 5000
            && graph.diagnostics.len() <= 50000
            && snapshot.id.len() <= 128
            && snapshot.workspace_id.len() <= 128,
        "neo4j_publish_capacity_exceeded"
    );
    // Admission precedes hashing/decoding stored graph rows. These fields are
    // subsequently retained in full; a quota never silently truncates a push.
    crate::json::encode_bounded(&(graph, &snapshot.files), limits.max_bytes, deadline)
        .map_err(|_| anyhow::anyhow!("neo4j_publish_capacity_exceeded"))?;
    crate::graph_extensions::validate_scope(store, snapshot, graph, deadline)?;
    deadline.check()?;
    let mut value = json!({"schemaVersion":1,"kind":"native_neo4j_graph_payload_v1",
        "snapshotId":snapshot.id,"workspaceId":snapshot.workspace_id,
        "graphHash":crate::model::identity(graph),"sourceFiles":snapshot.files,
        "entities":graph.entities,"observations":graph.observations,
        "evidence":graph.evidence,"diagnostics":graph.diagnostics,"coverage":graph.coverage,
        "sourceValidation":"native_published_base_graph_verified_before_export",
        "sourceAuthority":false,"canonicalCodeSnapshot":false,"complete":false,
        "networkConnections":0,"modelCalls":0,"projectCodeExecutionAuthorized":false,
        "limitations":["This is serialized export data. Importing it does not establish source, server, compiler, execution, ownership or permission authority.",
        "Directions, parallel observations, unresolved targets, producer/resolution labels and original evidence byte ranges are retained. No relation is upgraded to a call, compiler proof or observed execution.",
        "Admission quotas refuse the whole payload; no entities, observations or evidence are silently omitted. Preparing this data performs no network or database publication."]});
    value["payloadHash"] = json!(crate::model::identity(&value));
    crate::json::encode_bounded(&value, limits.max_bytes, deadline)
        .map_err(|_| anyhow::anyhow!("neo4j_publish_capacity_exceeded"))?;
    deadline.check()?;
    Ok(value)
}
