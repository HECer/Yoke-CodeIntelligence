//! Explicit compiler-bound incoming impact; original captured candidates stay separate.
pub use crate::compiler_hierarchy::Options;
use crate::{
    capture::Deadline,
    model::{Graph, Snapshot, identity, valid_id},
    projects::CompilerSpec,
    store::Store,
};
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet, VecDeque};

pub fn derive(
    store: &Store,
    report_id: &str,
    spec: &CompilerSpec,
    options: &Options,
    deadline: &Deadline,
) -> Result<Value> {
    deadline.check()?;
    options.validate()?;
    ensure!(
        spec.trusted,
        "Compiler-affected analysis requires explicit host trust"
    );
    ensure!(
        store.db.is_autocommit(),
        "Compiler-affected analysis cannot run inside a caller transaction"
    );
    ensure!(
        valid_id(report_id),
        "Invalid compiler-affected report identity"
    );
    // Clone already checked immutable pins, then release the read view BEFORE
    // any compiler process starts. The original report is checked again below.
    let (base, head, changed): ((Snapshot, Graph), (Snapshot, Graph), Vec<Value>) =
        super::impact_details::with_checked_report_inputs(
            store,
            report_id,
            super::impact_details::GraphInputLimits::new(5000, 50000, 100000)?,
            deadline,
            |input| {
                Ok((
                    (
                        input.base().snapshot().clone(),
                        input.base().graph().clone(),
                    ),
                    (
                        input.head().snapshot().clone(),
                        input.head().graph().clone(),
                    ),
                    input.changed_symbols().to_vec(),
                ))
            },
        )?;
    ensure!(
        changed.len() <= options.max_states,
        "Compiler-affected original seed capacity exceeded"
    );
    let node = crate::compiler_inline::host_file(&spec.node, 128000000, deadline)?;
    let compiler = crate::compiler_inline::host_file(&spec.typescript, 16000000, deadline)?;
    let profile = identity(&(
        "host_git_compiler_hierarchy_v1",
        &node,
        &compiler,
        crate::model::hash(crate::projects::worker_source()),
    ));
    let before =
        crate::compiler_hierarchy::observe(store, &base.0, &base.1, spec, options, deadline)?;
    let after = if base.0.id == head.0.id {
        before.clone()
    } else {
        crate::compiler_hierarchy::observe(store, &head.0, &head.1, spec, options, deadline)?
    };
    ensure!(
        before["compilerHash"] == compiler.1
            && after["compilerHash"] == compiler.1
            && crate::compiler_inline::host_file(&spec.node, 128000000, deadline)? == node
            && crate::compiler_inline::host_file(&spec.typescript, 16000000, deadline)? == compiler,
        "Compiler host files changed during hierarchy analysis"
    );
    super::impact_details::with_checked_report_inputs(
        store,
        report_id,
        super::impact_details::GraphInputLimits::new(5000, 50000, 100000)?,
        deadline,
        |input| {
            ensure!(
                identity(input.base().snapshot()) == identity(&base.0)
                    && identity(input.head().snapshot()) == identity(&head.0)
                    && identity(input.base().graph()) == identity(&base.1)
                    && identity(input.head().graph()) == identity(&head.1)
                    && input.changed_symbols() == changed,
                "Compiler-affected original report changed"
            );
            finish(
                report_id, &base, &head, &changed, &before, &after, &profile, options, deadline,
            )
        },
    )
}

#[allow(clippy::too_many_arguments)]
fn finish(
    report: &str,
    base: &(Snapshot, Graph),
    head: &(Snapshot, Graph),
    changed: &[Value],
    before: &Value,
    after: &Value,
    profile: &str,
    options: &Options,
    deadline: &Deadline,
) -> Result<Value> {
    let mut states = vec![];
    let mut proofs = vec![];
    let mut contract_expansions = vec![];
    let mut seed_mappings = vec![];
    let mut unmapped = vec![];
    let mut visits = 0usize;
    for (side, snapshot, graph, observation) in [
        ("base", &base.0, &base.1, before),
        ("head", &head.0, &head.1, after),
    ] {
        for context in observation["contexts"]
            .as_array()
            .context("Compiler contexts absent")?
        {
            let nodes = context["nodes"]
                .as_array()
                .context("Compiler nodes absent")?;
            let mut incoming = BTreeMap::<&str, Vec<&Value>>::new();
            let mut contracts = BTreeMap::<&str, Vec<&Value>>::new();
            for edge in context["relations"]
                .as_array()
                .context("Compiler relations absent")?
            {
                deadline.check()?;
                if matches!(
                    edge["relation"].as_str(),
                    Some(
                        "calls"
                            | "possibleDispatch"
                            | "possibleConstruct"
                            | "possibleConstructValue"
                            | "constructResult"
                            | "signatureImplementation"
                            | "accessesProperty"
                            | "possiblePropertyAccess"
                            | "gets"
                            | "sets"
                            | "possibleGets"
                            | "possibleSets"
                            | "propertyInitializer"
                    )
                ) {
                    incoming
                        .entry(edge["target"].as_str().context("Compiler target missing")?)
                        .or_default()
                        .push(edge);
                }
                if matches!(
                    edge["relation"].as_str(),
                    Some(
                        "overrides"
                            | "implementsMember"
                            | "overridesProperty"
                            | "implementsProperty"
                    )
                ) {
                    contracts
                        .entry(
                            edge["source"]
                                .as_str()
                                .context("Compiler contract source missing")?,
                        )
                        .or_default()
                        .push(edge);
                }
            }
            for (row_index, row) in changed
                .iter()
                .enumerate()
                .filter(|(_, r)| r["side"] == side)
            {
                deadline.check()?;
                let entity = graph
                    .entities
                    .iter()
                    .find(|e| row["id"] == e.id)
                    .context("Original changed symbol outside graph")?;
                let mut queue = VecDeque::new();
                let mut original_seeds = BTreeSet::new();
                let mut distances = BTreeMap::new();
                for node in nodes {
                    deadline.check()?;
                    visits = visits
                        .checked_add(1)
                        .context("Compiler walk visit overflow")?;
                    ensure!(
                        visits <= options.max_pair_visits,
                        "Compiler-affected visit capacity exceeded"
                    );
                    let exact = node["baseEntityIds"]
                        .as_array()
                        .context("Compiler entity bindings absent")?
                        .iter()
                        .any(|id| id == &row["id"]);
                    let span = &node["declaration"];
                    let contained = matches!(
                        entity.kind.as_str(),
                        "file" | "class" | "interface" | "function" | "method"
                    ) && span["path"] == entity.path
                        && span["startByte"]
                            .as_u64()
                            .is_some_and(|n| n >= entity.start_byte as u64)
                        && span["endByte"]
                            .as_u64()
                            .is_some_and(|n| n <= entity.end_byte as u64);
                    if !exact && !contained {
                        continue;
                    }
                    let id = node["id"]
                        .as_str()
                        .context("Compiler node identity absent")?;
                    if original_seeds.insert(id) {
                        distances.insert(id, 0usize);
                        queue.push_back((id, 0usize));
                        seed_mappings.push(json!({"changedRow":row_index,"side":side,"contextId":context["contextId"],"nodeId":id,"basis":if exact{"exact_native_declaration_binding"}else{"original_source_contains_compiler_declaration"}}));
                    }
                }
                if queue.is_empty() {
                    unmapped.push(json!({"changedRow":row_index,"side":side,"contextId":context["contextId"],"reason":"no_captured_compiler_declaration_mapping"}));
                }
                while let Some((node, distance)) = queue.pop_front() {
                    deadline.check()?;
                    // A later zero-cost contract step can improve an enqueued
                    // incoming path. Discard its older queue entry, not the proof.
                    if distances.get(node) != Some(&distance) {
                        continue;
                    }
                    ensure!(
                        states.len() < options.max_states,
                        "Compiler-affected state capacity exceeded"
                    );
                    let edges = incoming.get(node).map(Vec::as_slice).unwrap_or(&[]);
                    let bindings = contracts.get(node).map(Vec::as_slice).unwrap_or(&[]);
                    states.push(json!({"changedRow":row_index,"side":side,"snapshotId":snapshot.id,"contextId":context["contextId"],"nodeId":node,"distance":distance,"originalSeed":original_seeds.contains(node),"boundary":distance==options.max_depth,"unexaminedIncoming":if distance==options.max_depth{edges.len()}else{0},"unexaminedContractBindings":if distance==options.max_depth{bindings.len()}else{0}}));
                    if distance == options.max_depth {
                        continue;
                    }
                    for binding in bindings {
                        deadline.check()?;
                        visits = visits
                            .checked_add(1)
                            .context("Compiler walk visit overflow")?;
                        ensure!(
                            visits <= options.max_pair_visits
                                && proofs.len() + contract_expansions.len() < options.max_relations,
                            "Compiler-affected contract proof capacity exceeded"
                        );
                        let target = binding["target"]
                            .as_str()
                            .context("Compiler contract target missing")?;
                        let basis = if matches!(
                            binding["relation"].as_str(),
                            Some("overridesProperty" | "implementsProperty")
                        ) {
                            "compiler_property_contract_dependency"
                        } else {
                            "compiler_override_contract_dependency"
                        };
                        contract_expansions.push(json!({"changedRow":row_index,"side":side,"contextId":context["contextId"],"relationId":binding["id"],"sourceNodeId":node,"targetNodeId":target,"distance":distance,"direction":"outgoing_compiler_binding","basis":basis,"runtimeDispatchEstablished":false}));
                        if distances
                            .get(target)
                            .is_none_or(|previous| distance < *previous)
                        {
                            distances.insert(target, distance);
                            queue.push_front((target, distance));
                        }
                    }
                    for edge in edges {
                        deadline.check()?;
                        visits = visits
                            .checked_add(1)
                            .context("Compiler walk visit overflow")?;
                        ensure!(
                            visits <= options.max_pair_visits
                                && proofs.len() + contract_expansions.len() < options.max_relations,
                            "Compiler-affected incoming proof capacity exceeded"
                        );
                        proofs.push(json!({"changedRow":row_index,"side":side,"contextId":context["contextId"],"relationId":edge["id"],"sourceNodeId":edge["source"],"targetNodeId":node,"distance":distance+1}));
                        let source = edge["source"]
                            .as_str()
                            .context("Compiler source endpoint missing")?;
                        if distances
                            .get(source)
                            .is_none_or(|previous| distance + 1 < *previous)
                        {
                            distances.insert(source, distance + 1);
                            queue.push_back((source, distance + 1));
                        }
                    }
                }
            }
        }
    }
    let mut result = json!({"schema":"git_compiler_affected_v1","reportArtifactId":report,"workspaceId":base.0.workspace_id,"hostCompilerProfile":profile,"compiler": "5.8.3","options":options,"originalChangedSymbols":changed,"base":before,"head":after,"seedMappings":seed_mappings,"unmappedOriginals":unmapped,"states":states,"incomingProofs":proofs,"contractExpansions":contract_expansions,"edgeVisits":visits,"compilerBindingEstablished":true,"runtimeDispatchEstablished":false,"complete":false,"limitations":["Incoming walks use actual compiler call bindings and distinct type-assignable possible targets, not observed execution.","Zero-distance override/implementation steps preserve exact compiler contract dependencies; they are not actual dispatch, compatible generic instantiations or changed supertype declarations.","Source containment seed mappings are original-byte ownership, not an additional semantic identity claim.","Captured classes and selected configured contexts are not a closed-world runtime or exhaustive dispatch enumeration.","The original captured_hierarchy_v1 report and canonical source graphs remain unchanged."]});
    result["workerHash"] = json!(crate::model::hash(crate::projects::worker_source()));
    result["resultId"] = json!(identity(&("git_compiler_affected_v1", &result)));
    let bytes = serde_json::to_vec(&result)?;
    ensure!(
        bytes
            .len()
            .checked_add(1)
            .is_some_and(|n| n <= options.max_output_bytes),
        "Compiler-affected output byte capacity exceeded"
    );
    deadline.check()?;
    Ok(result)
}
