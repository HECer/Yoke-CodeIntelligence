//! Explicit host profiles for protocol fixtures; these are not production providers.
#![allow(dead_code)]
use serde_json::{Value, json};
pub use yoke::lsp::*;
use yoke::{
    Engine,
    capture::Deadline,
    model::{Graph, Snapshot},
};
pub fn configured(server: &Server) -> ConfiguredServer {
    ConfiguredServer {
        server: server.clone(),
        configuration: HostConfiguration {
            passive_profile: Some(PassiveProfile::ContractFixture),
            ..Default::default()
        },
    }
}
pub fn run(
    engine: &mut Engine,
    server: &Server,
    path: &str,
    offset: usize,
    operation: &str,
    new_name: Option<&str>,
    deadline: &Deadline,
) -> anyhow::Result<Value> {
    let (snapshot, graph, _, _) = engine.index(deadline)?;
    run_at(
        engine,
        server,
        &snapshot,
        &graph,
        SemanticRequest {
            path,
            offset,
            operation,
            new_name,
        },
        deadline,
    )
}
pub fn run_at(
    engine: &mut Engine,
    server: &Server,
    snapshot: &Snapshot,
    graph: &Graph,
    request: SemanticRequest<'_>,
    deadline: &Deadline,
) -> anyhow::Result<Value> {
    run_at_with_options(
        engine,
        server,
        snapshot,
        graph,
        request,
        &json!({}),
        deadline,
    )
}
pub fn run_at_with_options(
    engine: &mut Engine,
    server: &Server,
    snapshot: &Snapshot,
    graph: &Graph,
    request: SemanticRequest<'_>,
    options: &Value,
    deadline: &Deadline,
) -> anyhow::Result<Value> {
    yoke::lsp::run_at_configured(
        engine,
        &configured(server),
        snapshot,
        graph,
        request,
        options,
        deadline,
    )
}
