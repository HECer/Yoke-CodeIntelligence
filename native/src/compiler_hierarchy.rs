//! Source-checked observations from an explicitly selected immutable compiler.
use crate::{
    capture::Deadline,
    model::{Graph, Snapshot, identity, valid_id},
    projects::CompilerSpec,
    store::Store,
};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Options {
    pub max_nodes: usize,
    pub max_relations: usize,
    pub max_pair_visits: usize,
    pub max_depth: usize,
    pub max_states: usize,
    pub max_output_bytes: usize,
}
impl Default for Options {
    fn default() -> Self {
        Self {
            max_nodes: 5000,
            max_relations: 20000,
            max_pair_visits: 200000,
            max_depth: 8,
            max_states: 20000,
            max_output_bytes: 4000000,
        }
    }
}
impl Options {
    pub(crate) fn validate(&self) -> Result<()> {
        let max = Self::default();
        ensure!(
            self.max_nodes <= max.max_nodes
                && self.max_relations <= max.max_relations
                && self.max_pair_visits <= max.max_pair_visits
                && self.max_depth <= 32
                && self.max_states <= max.max_states
                && (1024..=max.max_output_bytes).contains(&self.max_output_bytes),
            "Invalid compiler hierarchy bounds"
        );
        Ok(())
    }
}
#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Span {
    path: String,
    start_byte: usize,
    end_byte: usize,
    text: String,
}
impl Span {
    fn check(&self, sources: &BTreeMap<String, String>) -> Result<()> {
        let text = sources
            .get(&self.path)
            .context("Compiler hierarchy span outside source scope")?;
        ensure!(
            self.start_byte <= self.end_byte
                && text.get(self.start_byte..self.end_byte) == Some(self.text.as_str()),
            "Compiler hierarchy original UTF-8 span mismatch"
        );
        Ok(())
    }
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Node {
    kind: String,
    name: String,
    declaration: Span,
    selection: Span,
    #[serde(rename = "static")]
    is_static: bool,
    has_body: bool,
}
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Edge {
    relation: String,
    source: usize,
    target: usize,
    proof: Span,
    owner: Option<usize>,
    resolved_target: Option<usize>,
    candidate_owner: Option<usize>,
    basis: Option<String>,
    conditional: Option<bool>,
}
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Unavailable {
    reason: String,
    anchor: Span,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Hierarchy {
    schema: u32,
    nodes: Vec<Node>,
    relations: Vec<Edge>,
    unavailable: Vec<Unavailable>,
    pair_visits: usize,
    ast_nodes: usize,
    complete: bool,
    runtime_dispatch_established: bool,
}
fn method(node: &Node) -> bool {
    matches!(node.kind.as_str(), "MethodDeclaration" | "MethodSignature")
}
fn contains(outer: &Span, inner: &Span) -> bool {
    outer.path == inner.path
        && outer.start_byte <= inner.start_byte
        && inner.end_byte <= outer.end_byte
}
fn entity_kind(kind: &str) -> Option<&str> {
    match kind {
        "SourceFile" => Some("file"),
        "ClassDeclaration" => Some("class"),
        "InterfaceDeclaration" => Some("interface"),
        "FunctionDeclaration" | "FunctionExpression" | "ArrowFunction" => Some("function"),
        "MethodDeclaration" | "MethodSignature" | "Constructor" => Some("method"),
        _ => None,
    }
}
pub(crate) fn observe(
    store: &Store,
    snapshot: &Snapshot,
    graph: &Graph,
    spec: &CompilerSpec,
    options: &Options,
    deadline: &Deadline,
) -> Result<Value> {
    options.validate()?;
    ensure!(
        spec.trusted,
        "Compiler hierarchy requires explicit host trust"
    );
    ensure!(
        store.db.is_autocommit(),
        "Compiler hierarchy cannot execute inside a caller transaction"
    );
    ensure!(
        snapshot
            .files
            .iter()
            .try_fold(0usize, |n, f| n.checked_add(f.size))
            .is_some_and(|n| n <= 10000000),
        "Compiler hierarchy source capacity exceeded"
    );
    let sources = crate::graph_extensions::sources(store, snapshot, graph, deadline)?;
    let raw = crate::projects::execute(
        store,
        snapshot,
        spec,
        "hierarchy",
        Some(
            json!({"maxNodes":options.max_nodes,"maxRelations":options.max_relations,"maxPairVisits":options.max_pair_visits}),
        ),
        deadline,
    )?;
    ensure!(
        raw["status"] == "passed",
        "Compiler hierarchy requires passed captured compiler contexts; inspect ordinary compiler diagnostics separately"
    );
    let mut contexts = vec![];
    let mut seen_contexts = BTreeSet::new();
    for context in raw["contexts"]
        .as_array()
        .context("Compiler hierarchy contexts absent")?
    {
        deadline.check()?;
        let id = context["contextId"]
            .as_str()
            .context("Compiler hierarchy context identity absent")?;
        ensure!(
            valid_id(id) && seen_contexts.insert(id) && context["status"] == "passed",
            "Invalid compiler hierarchy context"
        );
        let parsed: Hierarchy = crate::json::decode(context["hierarchy"].clone())?;
        ensure!(
            parsed.schema == 1
                && !parsed.complete
                && !parsed.runtime_dispatch_established
                && parsed.nodes.len() <= options.max_nodes
                && parsed.relations.len() <= options.max_relations
                && parsed.unavailable.len() <= options.max_relations
                && parsed.pair_visits <= options.max_pair_visits
                && parsed.ast_nodes <= 500000,
            "Compiler hierarchy schema or capacity mismatch"
        );
        let mut nodes = vec![];
        let mut positions = BTreeSet::new();
        for node in &parsed.nodes {
            deadline.check()?;
            node.declaration.check(&sources)?;
            node.selection.check(&sources)?;
            ensure!(
                node.kind.len() <= 80
                    && node.name.len() <= 4096
                    && node.declaration.path == node.selection.path
                    && node.declaration.start_byte <= node.selection.start_byte
                    && node.selection.end_byte <= node.declaration.end_byte
                    && positions.insert((
                        &node.declaration.path,
                        node.declaration.start_byte,
                        node.declaration.end_byte,
                        &node.kind
                    )),
                "Invalid compiler hierarchy declaration or selection"
            );
            let node_id = identity(&("native_compiler_hierarchy_node_v1", &snapshot.id, id, node));
            let bindings: Vec<_> = graph
                .entities
                .iter()
                .filter(|e| {
                    e.path == node.declaration.path
                        && Some(e.kind.as_str()) == entity_kind(&node.kind)
                        && ((e.name_start_byte == Some(node.selection.start_byte)
                            && e.name_end_byte == Some(node.selection.end_byte)
                            && node.selection.start_byte < node.selection.end_byte)
                            || (e.start_byte == node.declaration.start_byte
                                && e.end_byte == node.declaration.end_byte))
                })
                .map(|e| e.id.clone())
                .collect();
            let file = snapshot
                .files
                .iter()
                .find(|f| f.path == node.declaration.path)
                .context("Compiler declaration file missing")?;
            nodes.push(json!({"id":node_id,"kind":node.kind,"name":node.name,"declaration":node.declaration,"selection":node.selection,"static":node.is_static,"hasBody":node.has_body,"contentHash":file.hash,"baseEntityIds":bindings}));
        }
        let mut relations = vec![];
        for (position, edge) in parsed.relations.iter().enumerate() {
            deadline.check()?;
            edge.proof.check(&sources)?;
            ensure!(
                edge.source < nodes.len()
                    && edge.target < nodes.len()
                    && [edge.owner, edge.resolved_target, edge.candidate_owner]
                        .into_iter()
                        .flatten()
                        .all(|n| n < nodes.len()),
                "Compiler hierarchy relation endpoint outside context"
            );
            let from = &parsed.nodes[edge.source];
            let to = &parsed.nodes[edge.target];
            let no_dispatch = edge.resolved_target.is_none()
                && edge.candidate_owner.is_none()
                && edge.basis.is_none();
            match edge.relation.as_str() {
                "extends" | "implements" => ensure!(
                    matches!(
                        from.kind.as_str(),
                        "ClassDeclaration" | "InterfaceDeclaration"
                    ) && matches!(
                        to.kind.as_str(),
                        "ClassDeclaration" | "InterfaceDeclaration"
                    ) && edge.owner.is_none()
                        && no_dispatch
                        && edge.conditional.is_none(),
                    "Invalid compiler heritage relation"
                ),
                "declaresMember" => ensure!(
                    matches!(
                        from.kind.as_str(),
                        "ClassDeclaration" | "InterfaceDeclaration"
                    ) && method(to)
                        && edge.owner.is_none()
                        && no_dispatch
                        && edge.conditional.is_none()
                        && from.declaration.path == to.declaration.path
                        && from.declaration.start_byte <= to.declaration.start_byte
                        && to.declaration.end_byte <= from.declaration.end_byte
                        && edge.proof == to.selection,
                    "Invalid compiler member ownership"
                ),
                "overrides" | "implementsMember" => ensure!(
                    method(from)
                        && method(to)
                        && from.is_static == to.is_static
                        && edge.owner.is_some_and(|n| matches!(
                            parsed.nodes[n].kind.as_str(),
                            "ClassDeclaration" | "InterfaceDeclaration"
                        ))
                        && no_dispatch
                        && edge.conditional.is_none(),
                    "Invalid compiler member binding"
                ),
                "signatureImplementation" => ensure!(
                    !from.has_body
                        && to.has_body
                        && from.declaration.path == to.declaration.path
                        && from.kind == to.kind
                        && edge.owner.is_none()
                        && no_dispatch
                        && edge.conditional.is_none(),
                    "Invalid compiler signature implementation"
                ),
                "calls" => ensure!(
                    edge.owner.is_none() && no_dispatch && edge.conditional.is_some(),
                    "Invalid compiler call binding"
                ),
                "possibleDispatch" => ensure!(
                    method(to)
                        && !to.is_static
                        && to.has_body
                        && edge.owner.is_none()
                        && edge
                            .resolved_target
                            .is_some_and(|n| method(&parsed.nodes[n]))
                        && edge
                            .candidate_owner
                            .is_some_and(|n| parsed.nodes[n].kind == "ClassDeclaration")
                        && edge.basis.as_deref() == Some("compiler_type_assignability")
                        && edge.conditional.is_some(),
                    "Invalid compiler possible dispatch"
                ),
                _ => anyhow::bail!("Unknown compiler hierarchy relation"),
            }
            match edge.relation.as_str() {
                "extends" | "implements" => ensure!(
                    contains(&from.declaration, &edge.proof),
                    "Compiler heritage proof outside source declaration"
                ),
                "overrides" | "implementsMember" => {
                    let owner = &parsed.nodes[edge.owner.context("Compiler owner missing")?];
                    ensure!(
                        contains(&owner.declaration, &from.declaration)
                            && contains(&owner.declaration, &edge.proof),
                        "Compiler member binding owner/proof mismatch"
                    );
                }
                "calls" | "possibleDispatch" => ensure!(
                    contains(&from.declaration, &edge.proof),
                    "Compiler call proof outside its caller"
                ),
                "signatureImplementation" => ensure!(
                    contains(&from.declaration, &edge.proof),
                    "Compiler overload proof outside its signature"
                ),
                _ => {}
            }
            let mut value = serde_json::to_value(edge)?;
            value["source"] = nodes[edge.source]["id"].clone();
            value["target"] = nodes[edge.target]["id"].clone();
            for (key, index) in [
                ("owner", edge.owner),
                ("resolvedTarget", edge.resolved_target),
                ("candidateOwner", edge.candidate_owner),
            ] {
                value[key] = index.map_or(Value::Null, |n| nodes[n]["id"].clone());
            }
            value["id"] = json!(identity(&(
                "native_compiler_hierarchy_relation_v1",
                &snapshot.id,
                id,
                position,
                &value
            )));
            value["runtimeDispatchEstablished"] = json!(false);
            relations.push(value);
        }
        for unavailable in &parsed.unavailable {
            deadline.check()?;
            unavailable.anchor.check(&sources)?;
            ensure!(
                unavailable.reason.len() <= 128,
                "Compiler hierarchy coverage reason too long"
            );
        }
        contexts.push(json!({"contextId":id,"config":context["config"],"options":context["options"],"files":context["files"],"standardLibraryHashes":context["standardLibraryHashes"],"nodes":nodes,"relations":relations,"unavailable":parsed.unavailable,"pairVisits":parsed.pair_visits,"astNodes":parsed.ast_nodes,"complete":false}));
    }
    ensure!(
        !contexts.is_empty(),
        "Compiler hierarchy has no captured context"
    );
    Ok(
        json!({"schema":"native_compiler_hierarchy_v1","snapshotId":snapshot.id,"graphHash":identity(graph),"sourceBindings":raw["sourceBindings"],"captureOmissions":snapshot.omissions,"captureConsistency":snapshot.capture_consistency,"compiler":raw["compiler"],"compilerHash":raw["compilerHash"],"inputHash":raw["inputHash"],"standardLibraryHashes":raw["standardLibraryHashes"],"compilerLimitations":raw["limitations"],"contexts":contexts,"compilerBindingEstablished":true,"runtimeDispatchEstablished":false,"complete":false,"limitations":["Trusted host compiler observations are not an independent compiler correctness attestation.","Possible dispatch enumerates captured non-generic class types assignable to the static receiver; it is not execution or a closed-world guarantee.","Unsupported and external declarations remain explicit coverage entries; baseline source graphs are unchanged."]}),
    )
}
