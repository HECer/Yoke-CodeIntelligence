//! Explicit recovery of journal-v2 host output ownership and publication.
use super::journal_v2::{self, Journal};
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
    let mut journal = journal_v2::load(selection, requested_journal, d)?;
    let plan = load_plan(selection, &journal.plan_id, d)?;
    validate_plan(selection, &plan, now_ms, true)?;
    ensure!(
        journal.journal_id == journal_id(&plan) && journal.files.len() == plan.files.len(),
        "Host v2 recovery journal/plan binding differs"
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
    let mut runs = original_roles(selection, &plan, &journal, committed.is_some(), d)?;
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
    output_roles(
        selection,
        &plan,
        &journal,
        mode,
        committed.is_some(),
        &mut runs,
        d,
    )?;
    if committed.is_none() {
        precommit_security(&runs, &journal, d)?;
    }
    if journal.phase == "committed" {
        ensure!(
            runs.iter().all(|r| !r.changed || r.original.is_none()),
            "Host committed backup remains"
        );
        return committed.cloned().context("Host committed receipt missing");
    }
    if journal.phase == "rolled_back" {
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
    journal_v2::reserve_suffix(
        selection,
        &journal,
        remaining_events(&plan, &journal, &runs, mode, committed.is_some()),
        rollback.as_ref(),
        d,
    )?;
    let result = if let Some(rollback) = rollback {
        rollback_files(selection, &plan, &mut journal, &mut runs, rollback, d)
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
    partial: Option<io::RegisteredOutput>,
    allocate: bool,
    bytes: Option<Vec<u8>>,
}

fn matches_file(file: &io::HeldFile, id: Option<&str>, hash: Option<&str>) -> bool {
    id == Some(file.identity.as_str()) && hash == Some(file.hash.as_str())
}

fn original_roles(
    selection: &Selection,
    plan: &Plan,
    journal: &Journal,
    committed: bool,
    d: &Deadline,
) -> Result<Vec<RecoveryRun>> {
    let mut runs = Vec::with_capacity(plan.files.len());
    for (index, file) in plan.files.iter().enumerate() {
        let progress = &journal.files[index];
        let changed = file.before_hash != file.after_hash;
        let actual = io::read_file(&target(selection, index)?, MAX_FILE, true, d)?;
        let before = io::read_file(
            &auxiliary(selection, plan, index, "before")?,
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
            partial: None,
            allocate: false,
            bytes: None,
        };
        for (slot, held) in [(0, actual), (1, before)] {
            let Some(held) = held else { continue };
            if matches_file(
                &held,
                file.before_identity.as_deref(),
                file.before_hash.as_deref(),
            ) {
                ensure!(
                    run.original.is_none() && (changed || slot == 0),
                    "Host recovery original is ambiguous"
                );
                run.original_at_target = slot == 0;
                run.original = Some(held);
            } else if changed
                && slot == 0
                && matches_file(
                    &held,
                    progress.output_identity.as_deref(),
                    file.after_hash.as_deref(),
                )
            {
                run.output = Some(held);
                run.output_at_target = true;
            } else {
                bail!("Host recovery encountered a foreign or unprepared target");
            }
        }
        if !changed {
            ensure!(
                run.original.is_some() == file.before_hash.is_some()
                    && (run.original.is_none() || run.original_at_target),
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
        }
        runs.push(run);
    }
    Ok(runs)
}

fn output_roles(
    selection: &Selection,
    plan: &Plan,
    journal: &Journal,
    mode: RecoveryMode,
    committed: bool,
    runs: &mut [RecoveryRun],
    d: &Deadline,
) -> Result<()> {
    for (index, (file, run)) in plan.files.iter().zip(runs.iter_mut()).enumerate() {
        let path = auxiliary(selection, plan, index, "after")?;
        let progress = &journal.files[index];
        if !run.changed || committed || file.after_hash.is_none() {
            ensure_absent(&path, d)?;
            continue;
        }
        match progress.stage.as_str() {
            "pending" | "allocating" => {
                ensure_absent(&path, d)?;
                run.allocate = mode == RecoveryMode::Resume;
            }
            "registered" | "populating" => {
                let token = journal_v2::registration_token(selection, plan, journal, index, d)?;
                let canonical = run
                    .bytes
                    .as_deref()
                    .context("Host canonical output missing")?;
                run.partial = io::open_registered_output(
                    &path,
                    progress
                        .latest_identity()
                        .context("Host output identity absent")?,
                    run.original.as_ref(),
                    canonical,
                    progress.stage == "registered",
                    &token,
                    d,
                )?;
                if run.partial.is_none() && mode == RecoveryMode::Resume {
                    ensure!(
                        progress.stage == "registered",
                        "Populating host output is missing"
                    );
                    run.allocate = true;
                }
            }
            "prepared" | "backed_up" | "published" | "cleaned" => {
                if let Some(output) = io::read_file(&path, MAX_FILE, true, d)? {
                    ensure!(
                        run.output.is_none()
                            && matches_file(
                                &output,
                                progress.output_identity.as_deref(),
                                file.after_hash.as_deref()
                            ),
                        "Host recovery output is foreign or ambiguous"
                    );
                    run.output = Some(output);
                }
                if mode == RecoveryMode::Resume {
                    ensure!(run.output.is_some(), "Prepared host output is missing");
                }
            }
            _ => bail!("Unsupported host recovery output stage"),
        }
        if run.allocate {
            ensure!(
                progress.attempts.len() < journal_v2::MAX_ATTEMPTS,
                "Host output allocation attempts exhausted; Rollback remains available"
            );
        }
    }
    if mode == RecoveryMode::Resume && runs.iter().any(|r| r.allocate || r.partial.is_some()) {
        ensure!(
            journal.phase == "preparing"
                && runs
                    .iter()
                    .all(|r| !r.output_at_target && (r.original.is_none() || r.original_at_target)),
            "Host output preparation requires intact original targets"
        );
    }
    Ok(())
}

fn remaining_events(
    plan: &Plan,
    journal: &Journal,
    runs: &[RecoveryRun],
    mode: RecoveryMode,
    committed: bool,
) -> usize {
    if mode == RecoveryMode::Rollback {
        return 3 * plan.files.len() + 3;
    }
    if committed {
        return plan.files.len() + 3;
    }
    let mut events = 3;
    for (index, run) in runs.iter().enumerate() {
        if !run.changed {
            continue;
        }
        events += 3;
        if run.allocate {
            events += 4;
        } else if run.partial.is_some() {
            events += if journal.files[index].stage == "registered" {
                2
            } else {
                1
            };
        } else if plan.files[index].after_hash.is_none() && journal.files[index].stage == "pending"
        {
            events += 1;
        }
    }
    events
}

fn precommit_file_security(
    run: &RecoveryRun,
    progress: &journal_v2::Progress,
    d: &Deadline,
) -> Result<()> {
    journal_v2::check_source_security(progress, run.original.as_ref(), d)?;
    let Some(output) = &run.output else {
        return Ok(());
    };
    let attempt = progress
        .attempts
        .last()
        .filter(|attempt| !attempt.abandoned)
        .context("Host prepared output has no active security proof")?;
    ensure!(
        attempt.identity.as_deref() == Some(output.identity.as_str()),
        "Host prepared output identity differs from its security proof"
    );
    let expected = attempt
        .security_hash
        .as_deref()
        .context("Host prepared output security proof is absent")?;
    ensure!(
        io::security_fingerprint(output, d)? == expected,
        "Host prepared output security changed"
    );
    Ok(())
}

fn precommit_security(runs: &[RecoveryRun], journal: &Journal, d: &Deadline) -> Result<()> {
    for (run, progress) in runs.iter().zip(&journal.files) {
        precommit_file_security(run, progress, d)?;
    }
    Ok(())
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
            if !run.changed {
                continue;
            }
            precommit_file_security(run, &journal.files[index], d)?;
            if run.allocate {
                run.output = Some(journal_v2::prepare_output(
                    selection,
                    plan,
                    journal,
                    index,
                    run.original.as_ref(),
                    run.bytes
                        .as_deref()
                        .context("Host canonical output missing")?,
                    d,
                )?);
                run.allocate = false;
                precommit_file_security(run, &journal.files[index], d)?;
            } else if let Some(mut partial) = run.partial.take() {
                if journal.files[index].stage == "registered" {
                    journal.files[index].stage = "populating".into();
                    journal_v2::append_event(selection, journal, d)?;
                }
                let canonical = run
                    .bytes
                    .as_deref()
                    .context("Host canonical output missing")?;
                loop {
                    journal_v2::check_source_security(
                        &journal.files[index],
                        run.original.as_ref(),
                        d,
                    )?;
                    if partial.append_chunk(canonical, d)? == 0 {
                        break;
                    }
                }
                journal_v2::check_source_security(&journal.files[index], run.original.as_ref(), d)?;
                let output = partial.finalize(canonical, d)?;
                journal.files[index].output_identity = Some(output.identity.clone());
                run.output = Some(output);
                journal.files[index].stage = "prepared".into();
                precommit_file_security(run, &journal.files[index], d)?;
                journal_v2::append_event(selection, journal, d)?;
            } else if plan.files[index].after_hash.is_none()
                && journal.files[index].stage == "pending"
            {
                journal.files[index].stage = "prepared".into();
                journal_v2::append_event(selection, journal, d)?;
            }
        }
        if journal.phase != "applying" {
            precommit_security(runs, journal, d)?;
            journal.phase = "applying".into();
            journal_v2::append_event(selection, journal, d)?;
        }
        for (index, run) in runs.iter_mut().enumerate() {
            if !run.changed {
                continue;
            }
            precommit_file_security(run, &journal.files[index], d)?;
            if run.original_at_target {
                run.original
                    .as_mut()
                    .context("Host original missing")?
                    .rename_no_replace(&auxiliary(selection, plan, index, "before")?, d)?;
                run.original_at_target = false;
            }
            if !matches!(
                journal.files[index].stage.as_str(),
                "backed_up" | "published"
            ) {
                precommit_file_security(run, &journal.files[index], d)?;
                journal.files[index].stage = "backed_up".into();
                journal_v2::append_event(selection, journal, d)?;
            }
            precommit_file_security(run, &journal.files[index], d)?;
            if let Some(output) = run.output.as_mut()
                && !run.output_at_target
            {
                output.rename_no_replace(&target(selection, index)?, d)?;
                run.output_at_target = true;
            }
            if journal.files[index].stage != "published" {
                precommit_file_security(run, &journal.files[index], d)?;
                journal.files[index].stage = "published".into();
                journal_v2::append_event(selection, journal, d)?;
            }
        }
        precommit_security(runs, journal, d)?;
        journal_v2::commit_receipt(selection, plan, journal, d)?
    };
    if journal.phase != "cleanup" || journal.receipt_id.as_ref() != Some(&receipt.id) {
        journal.receipt_id = Some(receipt.id.clone());
        journal.phase = "cleanup".into();
        journal_v2::append_event(selection, journal, d)?;
    }
    for (index, run) in runs.iter_mut().enumerate() {
        if !run.changed {
            continue;
        }
        if let Some(original) = run.original.take() {
            ensure!(
                !run.original_at_target,
                "Host cleanup original remains at target"
            );
            original.delete_owned(d)?;
        }
        if journal.files[index].stage != "cleaned" {
            journal.files[index].stage = "cleaned".into();
            journal_v2::append_event(selection, journal, d)?;
        }
    }
    journal.phase = "committed".into();
    journal_v2::append_event(selection, journal, d)?;
    Ok(receipt)
}

fn ensure_restored(runs: &[RecoveryRun]) -> Result<()> {
    ensure!(
        runs.iter()
            .all(|r| (r.original.is_none() || r.original_at_target)
                && r.output.is_none()
                && r.partial.is_none()),
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
    precommit_security(runs, journal, d)?;
    if journal.phase != "rolling_back" {
        journal.phase = "rolling_back".into();
        journal_v2::append_event(selection, journal, d)?;
    }
    for (index, run) in runs.iter_mut().enumerate() {
        if !run.changed {
            continue;
        }
        precommit_file_security(run, &journal.files[index], d)?;
        if run.output_at_target {
            run.output
                .as_mut()
                .context("Host rollback output missing")?
                .rename_no_replace(&auxiliary(selection, plan, index, "after")?, d)?;
            run.output_at_target = false;
            precommit_file_security(run, &journal.files[index], d)?;
            journal_v2::append_event(selection, journal, d)?;
        }
        precommit_file_security(run, &journal.files[index], d)?;
        if let Some(original) = run.original.as_mut()
            && !run.original_at_target
        {
            original.rename_no_replace(&target(selection, index)?, d)?;
            run.original_at_target = true;
            precommit_file_security(run, &journal.files[index], d)?;
            journal_v2::append_event(selection, journal, d)?;
        }
    }
    for (index, run) in runs.iter_mut().enumerate() {
        if !run.changed {
            continue;
        }
        precommit_file_security(run, &journal.files[index], d)?;
        if let Some(partial) = run.partial.take() {
            journal_v2::check_source_security(&journal.files[index], run.original.as_ref(), d)?;
            partial.discard(
                run.bytes
                    .as_deref()
                    .context("Host canonical output missing")?,
                d,
            )?;
        }
        if let Some(output) = run.output.take() {
            output.delete_owned(d)?;
        }
        if journal.files[index].stage != "cleaned" {
            precommit_file_security(run, &journal.files[index], d)?;
            journal.files[index].stage = "cleaned".into();
            journal_v2::append_event(selection, journal, d)?;
        }
    }
    ensure_restored(runs)?;
    precommit_security(runs, journal, d)?;
    journal.phase = "rolled_back".into();
    journal.result = Some(result.clone());
    journal_v2::append_event(selection, journal, d)?;
    Ok(result)
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
    let authored = body(selection)?;
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
        let patch = host_config::agents_patch(
            bytes,
            (plan.operation == Operation::Install).then_some(authored.as_slice()),
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
