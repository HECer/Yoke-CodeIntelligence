//! One source-bound folder operation using one explicitly registered provider.
use crate::{
    Engine,
    capture::Deadline,
    changes, lsp,
    model::{Graph, Snapshot},
};
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use std::collections::BTreeSet;

pub(super) fn prepare(
    engine: &mut Engine,
    snapshot: &Snapshot,
    graph: &Graph,
    source: &str,
    destination: &str,
    deadline: &Deadline,
) -> Result<Value> {
    let selected = changes::select_directory_move(
        &engine.root,
        &engine.store,
        snapshot,
        source,
        destination,
        deadline,
    )?;
    let languages: BTreeSet<_> = selected
        .record()
        .files()
        .iter()
        .map(|file| file.language.clone())
        .collect();
    let (server, original_configuration) = if engine.selected_backends.is_some() {
        let mut ids = BTreeSet::new();
        for file in selected.record().files() {
            deadline.check()?;
            ids.insert(
                engine
                    .selected_backend_id(&file.path)?
                    .context("Directory move provider unavailable")?
                    .to_owned(),
            );
        }
        ensure!(
            ids.len() == 1,
            "Directory move spans multiple selected backends; multi-provider repair is unavailable"
        );
        let file = selected
            .record()
            .files()
            .first()
            .context("Directory move source absent")?;
        engine
            .semantic_host(&file.path, &file.language)?
            .context("Directory move provider unavailable")?
    } else {
        let registered: Vec<_> = languages
            .iter()
            .filter(|language| engine.servers.contains_key(language.as_str()))
            .collect();
        ensure!(
            registered.len() == 1,
            "Directory move requires exactly one registered provider language; multi-provider repair is unavailable"
        );
        let language = registered[0];
        let server = engine
            .servers
            .get(language)
            .cloned()
            .context("Directory move provider unavailable")?;
        (server, engine.server_configurations.get(language).cloned())
    };
    let configuration = lsp::effective_host_configuration(
        engine,
        snapshot,
        original_configuration.as_ref(),
        deadline,
    )?;
    if let Some(configuration) = &configuration {
        ensure!(
            !configuration
                .workspace_folders
                .iter()
                .any(|folder| folder == source
                    || folder
                        .strip_prefix(source)
                        .is_some_and(|suffix| suffix.starts_with('/'))),
            "Directory move would relocate a host-registered workspace root"
        );
    }
    let mut staged = lsp::stage_directory_move_at(
        engine,
        (&server, configuration.as_ref()),
        snapshot,
        graph,
        &selected,
        deadline,
    )?;
    ensure!(
        staged.edits.len() == 1,
        "Directory move requires one unpublished edit batch"
    );
    let checked = staged.edits.pop().context("Directory move edit absent")?;
    let plan = checked.prepare(engine, snapshot, graph, deadline)?;
    let other: Vec<_> = languages
        .iter()
        .filter(|other| other.as_str() != server.language)
        .collect();
    Ok(
        json!({"snapshotId":snapshot.id,"plan":plan,"serverProfileHash":staged.value["serverProfileHash"],"providerSourceOmissions":staged.value["providerSourceOmissions"],"provider":"host_selected_lsp","equivalent":"independent_captured_directory_move","referenceRepairCoverage":{"providerLanguage":server.language,"otherCapturedLanguages":other,"exhaustiveCompilerProof":false},"complete":false,"commandsExecuted":false}),
    )
}
