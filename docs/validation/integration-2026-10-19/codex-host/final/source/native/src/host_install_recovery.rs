//! Explicit recovery of one host installation transaction.
use super::*;

pub(super) fn recover(
    selection: &Selection,
    authority: &WriteAuthority,
    requested_journal: &str,
    mode: RecoveryMode,
    now_ms: u64,
    d: &Deadline,
) -> Result<Receipt> {
    check_authority(selection, authority)?;
    let _guards = guard_bindings(selection, d)?;
    ensure!(
        existing_state(selection, d)?,
        "Host recovery state is absent"
    );
    let _lock = write_state(selection, d)?;
    let mut journal = load_journal(selection, requested_journal, d)?;
    let plan = load_plan(selection, &journal.plan_id, d)?;
    validate_plan(selection, &plan, now_ms, true)?;
    ensure!(
        journal.journal_id == super::journal_id(&plan) && journal.files.len() == plan.files.len(),
        "Host recovery journal/plan binding differs"
    );
    let current = head(selection, d)?;
    let committed = current.as_ref().filter(|r| r.plan_id == plan.id);
    if let Some(receipt) = committed {
        check_commit(selection, &plan, &journal, receipt)?;
        ensure!(
            mode == RecoveryMode::Resume,
            "Committed host installation permits resume only"
        );
        ensure!(
            !matches!(journal.phase.as_str(), "rolling_back" | "rolled_back"),
            "Host rollback journal conflicts with committed ownership"
        );
    } else {
        ensure!(
            current.as_ref().map(|r| &r.id) == plan.expected_receipt.as_ref()
                && journal.receipt_id.is_none()
                && !matches!(journal.phase.as_str(), "cleanup" | "committed"),
            "Host recovery ownership head changed"
        );
    }
    if mode == RecoveryMode::Resume {
        ensure!(
            !matches!(journal.phase.as_str(), "rolling_back" | "rolled_back"),
            "Host rollback direction cannot be changed"
        );
    }
    let mut runs = preflight(selection, &plan, &journal, mode, committed.is_some(), d)?;
    let parent = if committed.is_some() {
        previous_receipt(selection, &plan, d)?
    } else {
        None
    };
    let previous = if committed.is_some() {
        parent.as_ref()
    } else {
        current.as_ref()
    };
    canonical_outputs(
        selection,
        &plan,
        previous,
        &mut runs,
        committed.is_some(),
        d,
    )?;
    if journal.phase == "committed" {
        ensure!(
            runs.iter().all(|r| r.original.is_none() || !r.changed),
            "Host committed backup remains"
        );
        return committed.cloned().context("Host committed receipt missing");
    }
    if journal.phase == "rolled_back" {
        ensure!(
            mode == RecoveryMode::Rollback,
            "Host rollback direction cannot be changed"
        );
        ensure_restored(&runs)?;
        let result = rollback_receipt(selection, &plan, &journal, current.as_ref())?;
        ensure!(
            journal.result.as_ref() == Some(&result),
            "Host rollback result differs"
        );
        return Ok(result);
    }
    let rollback = if mode == RecoveryMode::Rollback {
        Some(rollback_receipt(
            selection,
            &plan,
            &journal,
            current.as_ref(),
        )?)
    } else {
        None
    };
    if mode == RecoveryMode::Resume && committed.is_none() {
        reserve_receipt_publication(selection, &plan, d)?;
    }
    reserve_journal_suffix(
        selection,
        &journal,
        4 * plan.files.len() + 6,
        rollback.as_ref(),
        d,
    )?;
    let result = if mode == RecoveryMode::Rollback {
        rollback_files(
            selection,
            &plan,
            &mut journal,
            &mut runs,
            rollback.unwrap(),
            d,
        )
    } else {
        resume_files(
            selection,
            &plan,
            &mut journal,
            &mut runs,
            committed.cloned(),
            d,
        )
    };
    result.map_err(|error| anyhow::anyhow!(
        "Host installation recovery required; journal {}. {}. Matching originals and foreign files are preserved",
        journal.journal_id, error
    ))
}

struct RecoveryRun {
    changed: bool,
    original: Option<io::HeldFile>,
    original_at_target: bool,
    output: Option<io::HeldFile>,
    output_at_target: bool,
    bytes: Option<Vec<u8>>,
}

fn matches_file(file: &io::HeldFile, id: Option<&String>, hash: Option<&String>) -> bool {
    id == Some(&file.identity) && hash == Some(&file.hash)
}

fn preflight(
    selection: &Selection,
    plan: &Plan,
    journal: &Journal,
    mode: RecoveryMode,
    committed: bool,
    d: &Deadline,
) -> Result<Vec<RecoveryRun>> {
    let mut runs = Vec::with_capacity(plan.files.len());
    for (index, file) in plan.files.iter().enumerate() {
        d.check()?;
        let progress = &journal.files[index];
        let changed = file.before_hash != file.after_hash;
        ensure!(
            progress
                .output_identity
                .as_ref()
                .is_none_or(|id| Some(id) != file.before_identity.as_ref())
                && (file.after_hash.is_some() || progress.output_identity.is_none()),
            "Host recovery output identity is invalid"
        );
        let actual = io::read_file(&target(selection, index)?, MAX_FILE, true, d)?;
        let before = io::read_file(
            &auxiliary(selection, plan, index, "before")?,
            MAX_FILE,
            true,
            d,
        )?;
        let after = io::read_file(
            &auxiliary(selection, plan, index, "after")?,
            MAX_FILE,
            true,
            d,
        )?;
        let mut run = RecoveryRun {
            changed,
            original: None,
            original_at_target: false,
            output: None,
            output_at_target: false,
            bytes: None,
        };
        for (slot, held) in [(0, actual), (1, before), (2, after)] {
            let Some(held) = held else { continue };
            if slot != 2
                && matches_file(
                    &held,
                    file.before_identity.as_ref(),
                    file.before_hash.as_ref(),
                )
            {
                ensure!(
                    run.original.is_none() && (changed || slot == 0),
                    "Host recovery original is ambiguous"
                );
                run.original_at_target = slot == 0;
                run.original = Some(held);
            } else if changed
                && slot != 1
                && matches_file(
                    &held,
                    progress.output_identity.as_ref(),
                    file.after_hash.as_ref(),
                )
            {
                ensure!(run.output.is_none(), "Host recovery output is ambiguous");
                run.output_at_target = slot == 0;
                run.output = Some(held);
            } else {
                bail!("Host recovery encountered a foreign or unidentified file");
            }
        }
        if !changed {
            ensure!(
                run.original.is_some() == file.before_hash.is_some()
                    && (run.original.is_none() || run.original_at_target)
                    && progress.output_identity.is_none(),
                "Unchanged host recovery target differs"
            );
        } else if committed {
            ensure!(
                !run.original_at_target
                    && run.output.is_some() == file.after_hash.is_some()
                    && (run.output.is_none() || run.output_at_target),
                "Committed host recovery target differs"
            );
        } else {
            ensure!(
                run.original.is_some() == file.before_hash.is_some(),
                "Host recovery original is missing"
            );
            if mode == RecoveryMode::Resume && progress.output_identity.is_some() {
                ensure!(
                    run.output.is_some(),
                    "Recorded host recovery output is missing"
                );
            }
        }
        runs.push(run);
    }
    Ok(runs)
}

fn canonical_outputs(
    selection: &Selection,
    plan: &Plan,
    previous: Option<&Receipt>,
    runs: &mut [RecoveryRun],
    committed: bool,
    d: &Deadline,
) -> Result<()> {
    if plan.operation == Operation::Uninstall {
        ensure!(
            previous.is_some_and(|r| r.status == "installed"),
            "Host uninstall recovery lacks ownership"
        );
    }
    for (index, (file, run)) in plan.files.iter().zip(runs).enumerate() {
        if committed && run.changed && file.before_hash.is_some() && run.original.is_none() {
            continue;
        }
        let bytes = run
            .original
            .as_ref()
            .map_or(&[][..], |f| f.bytes.as_slice());
        let previous = previous
            .filter(|r| r.status == "installed")
            .map(|r| &r.files[index]);
        let patch = adapter::patch(
            selection,
            bytes,
            plan.operation == Operation::Install,
            previous.and_then(|f| f.owned_fragment.as_ref()),
            d,
        )?;
        let output = host_config::apply_patch(bytes, &patch, d)?;
        let created = previous.map_or(run.original.is_none(), |f| f.created_by_installation);
        let after_hash = if plan.operation == Operation::Uninstall && created && output.is_empty() {
            None
        } else {
            Some(hash(&output))
        };
        ensure!(
            patch == file.patch
                && created == file.created_by_installation
                && after_hash == file.after_hash,
            "Host recovery plan is not the canonical owned edit"
        );
        run.bytes = Some(output);
    }
    Ok(())
}

fn previous_receipt(selection: &Selection, plan: &Plan, d: &Deadline) -> Result<Option<Receipt>> {
    let Some(id) = &plan.expected_receipt else {
        return Ok(None);
    };
    ensure!(
        valid_id(id),
        "Host recovery parent receipt identifier is invalid"
    );
    let receipt: Receipt = metadata(
        &state(selection).join("receipts").join(format!("{id}.json")),
        d,
    )?;
    validate_receipt(selection, &receipt)?;
    ensure!(
        receipt.id == *id,
        "Host recovery parent receipt identity differs"
    );
    Ok(Some(receipt))
}

fn check_commit(
    selection: &Selection,
    plan: &Plan,
    journal: &Journal,
    receipt: &Receipt,
) -> Result<()> {
    let files = plan
        .files
        .iter()
        .map(|f| InstalledFile {
            root_index: f.root_index,
            path: f.path.clone(),
            created_by_installation: f.created_by_installation,
            owned_fragment: f.patch.owned_fragment.clone(),
            after_hash: f.after_hash.clone(),
        })
        .collect::<Vec<_>>();
    ensure!(
        receipt.parent_receipt == plan.expected_receipt
            && receipt.journal_id == journal.journal_id
            && receipt.installation_id == plan.installation_id
            && receipt.bindings == plan.bindings
            && receipt.operation == plan.operation
            && receipt.status
                == if plan.operation == Operation::Install {
                    "installed"
                } else {
                    "uninstalled"
                }
            && receipt.files == files
            && receipt.changed_paths == changed_paths(selection, plan)?
            && journal
                .receipt_id
                .as_ref()
                .is_none_or(|id| id == &receipt.id),
        "Host recovery committed receipt differs from plan"
    );
    Ok(())
}

fn changed_paths(selection: &Selection, plan: &Plan) -> Result<Vec<String>> {
    plan.files
        .iter()
        .filter(|f| f.before_hash != f.after_hash)
        .map(|f| target(selection, f.root_index).and_then(|p| path_text(&p)))
        .collect()
}

fn rollback_receipt(
    selection: &Selection,
    plan: &Plan,
    journal: &Journal,
    previous: Option<&Receipt>,
) -> Result<Receipt> {
    let mut result = Receipt {
        schema_version: 1,
        id: String::new(),
        parent_receipt: plan.expected_receipt.clone(),
        generation: previous.map_or(1, |r| r.generation),
        plan_id: plan.id.clone(),
        installation_id: plan.installation_id.clone(),
        profile: adapter::profile(selection).into(),
        bindings: plan.bindings.clone(),
        operation: plan.operation.clone(),
        status: "rolled_back".into(),
        changed_paths: changed_paths(selection, plan)?,
        journal_id: journal.journal_id.clone(),
        files: plan
            .files
            .iter()
            .enumerate()
            .map(|(index, f)| {
                let prior = previous.and_then(|r| r.files.get(index));
                InstalledFile {
                    root_index: f.root_index,
                    path: f.path.clone(),
                    created_by_installation: prior.is_some_and(|f| f.created_by_installation),
                    owned_fragment: prior.and_then(|f| f.owned_fragment.clone()),
                    after_hash: f.before_hash.clone(),
                }
            })
            .collect(),
    };
    result.id = receipt_id(&result);
    Ok(result)
}

fn resume_files(
    selection: &Selection,
    plan: &Plan,
    journal: &mut Journal,
    runs: &mut [RecoveryRun],
    committed: Option<Receipt>,
    d: &Deadline,
) -> Result<Receipt> {
    let receipt = if let Some(receipt) = committed {
        receipt
    } else {
        for (index, run) in runs.iter_mut().enumerate() {
            if !run.changed || plan.files[index].after_hash.is_none() || run.output.is_some() {
                continue;
            }
            let output = io::prepare_file(
                &auxiliary(selection, plan, index, "after")?,
                run.original.as_ref(),
                run.bytes
                    .as_deref()
                    .context("Host recovery canonical output missing")?,
                d,
            )?;
            journal.files[index].output_identity = Some(output.identity.clone());
            run.output = Some(output);
            journal.files[index].stage = "prepared".into();
            append_event(selection, journal, d)?;
        }
        if journal.phase != "applying" {
            journal.phase = "applying".into();
            append_event(selection, journal, d)?;
        }
        for (index, run) in runs.iter_mut().enumerate() {
            if !run.changed {
                continue;
            }
            if run.original_at_target {
                run.original
                    .as_mut()
                    .context("Host recovery original missing")?
                    .rename_no_replace(&auxiliary(selection, plan, index, "before")?, d)?;
                run.original_at_target = false;
            }
            if !matches!(
                journal.files[index].stage.as_str(),
                "backed_up" | "published"
            ) {
                journal.files[index].stage = "backed_up".into();
                append_event(selection, journal, d)?;
            }
            if let Some(output) = run.output.as_mut()
                && !run.output_at_target
            {
                output.rename_no_replace(&target(selection, index)?, d)?;
                run.output_at_target = true;
            }
            if journal.files[index].stage != "published" {
                journal.files[index].stage = "published".into();
                append_event(selection, journal, d)?;
            }
        }
        commit_receipt(selection, plan, journal, d)?
    };
    if journal.phase != "cleanup" || journal.receipt_id.as_ref() != Some(&receipt.id) {
        journal.receipt_id = Some(receipt.id.clone());
        journal.phase = "cleanup".into();
        append_event(selection, journal, d)?;
    }
    for (index, run) in runs.iter_mut().enumerate() {
        if !run.changed {
            continue;
        }
        if let Some(original) = run.original.take() {
            original.delete_owned(d)?;
        }
        if journal.files[index].stage != "cleaned" {
            journal.files[index].stage = "cleaned".into();
            append_event(selection, journal, d)?;
        }
    }
    journal.phase = "committed".into();
    append_event(selection, journal, d)?;
    Ok(receipt)
}

fn ensure_restored(runs: &[RecoveryRun]) -> Result<()> {
    ensure!(
        runs.iter()
            .all(|r| (r.original.is_none() || r.original_at_target) && r.output.is_none()),
        "Host rollback cleanup is incomplete"
    );
    Ok(())
}

fn rollback_files(
    selection: &Selection,
    plan: &Plan,
    journal: &mut Journal,
    runs: &mut [RecoveryRun],
    result: Receipt,
    d: &Deadline,
) -> Result<Receipt> {
    if journal.phase != "rolling_back" {
        journal.phase = "rolling_back".into();
        append_event(selection, journal, d)?;
    }
    for (index, run) in runs.iter_mut().enumerate() {
        if !run.changed {
            continue;
        }
        if run.output_at_target {
            run.output
                .as_mut()
                .context("Host rollback output missing")?
                .rename_no_replace(&auxiliary(selection, plan, index, "after")?, d)?;
            run.output_at_target = false;
            journal.files[index].stage = "backed_up".into();
            append_event(selection, journal, d)?;
        }
        if let Some(original) = run.original.as_mut()
            && !run.original_at_target
        {
            original.rename_no_replace(&target(selection, index)?, d)?;
            run.original_at_target = true;
            journal.files[index].stage = "prepared".into();
            append_event(selection, journal, d)?;
        }
    }
    for (index, run) in runs.iter_mut().enumerate() {
        if !run.changed {
            continue;
        }
        if let Some(output) = run.output.take() {
            output.delete_owned(d)?;
        }
        if journal.files[index].stage != "cleaned" {
            journal.files[index].stage = "cleaned".into();
            append_event(selection, journal, d)?;
        }
    }
    ensure_restored(runs)?;
    journal.phase = "rolled_back".into();
    journal.result = Some(result.clone());
    append_event(selection, journal, d)?;
    Ok(result)
}
