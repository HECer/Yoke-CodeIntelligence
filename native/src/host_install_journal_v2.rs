//! Strict v2 output-allocation lineage. Legacy journal validation is separate.
use super::*;

#[cfg(all(test, windows))]
#[path = "host_install_profile.rs"]
pub(super) mod profile;

pub(super) const MAX_ATTEMPTS: usize = 3;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Attempt {
    pub(super) ordinal: u8,
    pub(super) identity: Option<String>,
    pub(super) security_hash: Option<String>,
    pub(super) abandoned: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Progress {
    pub(super) stage: String,
    pub(super) output_identity: Option<String>,
    pub(super) source_security_hash: Option<String>,
    pub(super) attempts: Vec<Attempt>,
}
impl Progress {
    pub(super) fn latest_identity(&self) -> Option<&str> {
        self.attempts
            .last()
            .filter(|a| !a.abandoned)?
            .identity
            .as_deref()
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Journal {
    pub(super) schema_version: u32,
    pub(super) id: String,
    pub(super) journal_id: String,
    pub(super) plan_id: String,
    pub(super) sequence: u32,
    pub(super) previous_event: Option<String>,
    pub(super) phase: String,
    pub(super) files: Vec<Progress>,
    pub(super) receipt_id: Option<String>,
    pub(super) result: Option<Receipt>,
}

pub(super) fn event_id(journal: &Journal) -> String {
    let mut copy = journal.clone();
    copy.id.clear();
    identity(&copy)
}

// Only a validated, read-only common view. It is never passed to v1 recovery.
pub(super) fn summary(journal: &Journal) -> super::Journal {
    super::Journal {
        schema_version: 2,
        id: journal.id.clone(),
        journal_id: journal.journal_id.clone(),
        plan_id: journal.plan_id.clone(),
        sequence: journal.sequence,
        previous_event: journal.previous_event.clone(),
        phase: journal.phase.clone(),
        files: journal
            .files
            .iter()
            .map(|f| super::Progress {
                stage: f.stage.clone(),
                output_identity: f.output_identity.clone(),
            })
            .collect(),
        receipt_id: journal.receipt_id.clone(),
        result: journal.result.clone(),
    }
}

pub(super) fn version(selection: &Selection, id: &str, d: &Deadline) -> Result<u32> {
    ensure!(valid_id(id), "Invalid host journal identifier");
    for path in names(&state(selection).join("journals").join(id), MAX_EVENTS, d)? {
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .context("Invalid journal filename")?;
        if name.starts_with(".tmp-") {
            continue;
        }
        let file =
            io::read_file(&path, MAX_PLAN, false, d)?.context("Journal event disappeared")?;
        let value = crate::json::parse(&file.bytes, MAX_PLAN)?;
        let version = value
            .get("schemaVersion")
            .and_then(serde_json::Value::as_u64)
            .context("Host journal schema is absent")?;
        ensure!(matches!(version, 1 | 2), "Unsupported host journal schema");
        return Ok(version as u32);
    }
    bail!("Host journal contains no committed events")
}

fn rank(stage: &str) -> Option<usize> {
    [
        "pending",
        "allocating",
        "registered",
        "populating",
        "prepared",
        "backed_up",
        "published",
        "cleaned",
    ]
    .iter()
    .position(|s| *s == stage)
}

fn validate_event(
    selection: &Selection,
    plan: &Plan,
    journal: &Journal,
    d: &Deadline,
) -> Result<()> {
    ensure!(
        journal.schema_version == 2
            && journal.journal_id == journal_id(plan)
            && journal.plan_id == plan.id
            && journal.sequence < MAX_EVENTS as u32
            && journal.previous_event.as_deref().is_none_or(valid_id)
            && journal.receipt_id.as_deref().is_none_or(valid_id)
            && journal.files.len() == plan.files.len()
            && [
                "preparing",
                "applying",
                "cleanup",
                "committed",
                "rolling_back",
                "rolled_back"
            ]
            .contains(&journal.phase.as_str()),
        "Host v2 journal phase/target binding invalid"
    );
    for (file, progress) in plan.files.iter().zip(&journal.files) {
        ensure!(
            rank(&progress.stage).is_some()
                && progress.attempts.len() <= MAX_ATTEMPTS
                && progress
                    .source_security_hash
                    .as_deref()
                    .is_none_or(valid_id)
                && progress.source_security_hash.is_some() == file.before_identity.is_some()
                && progress
                    .output_identity
                    .as_deref()
                    .is_none_or(native_identity)
                && progress
                    .output_identity
                    .as_deref()
                    .is_none_or(|id| progress.latest_identity() == Some(id)),
            "Invalid host v2 progress"
        );
        for (ordinal, attempt) in progress.attempts.iter().enumerate() {
            ensure!(
                attempt.ordinal as usize == ordinal
                    && attempt.identity.as_deref().is_none_or(native_identity)
                    && attempt.security_hash.as_deref().is_none_or(valid_id)
                    && attempt.identity.is_some() == attempt.security_hash.is_some()
                    && attempt
                        .identity
                        .as_ref()
                        .is_none_or(|id| Some(id) != file.before_identity.as_ref())
                    && (ordinal + 1 == progress.attempts.len() || attempt.abandoned),
                "Invalid host v2 attempt lineage"
            );
        }
        let changed = file.before_hash != file.after_hash;
        if !changed || file.after_hash.is_none() {
            ensure!(
                progress.attempts.is_empty()
                    && progress.output_identity.is_none()
                    && !["allocating", "registered", "populating"]
                        .contains(&progress.stage.as_str()),
                "Host v2 output allocation without planned output"
            );
        } else if ["registered", "populating"].contains(&progress.stage.as_str()) {
            ensure!(
                progress.latest_identity().is_some() && progress.output_identity.is_none(),
                "Host v2 registration identity absent"
            );
        } else if progress.stage == "allocating" {
            ensure!(
                progress
                    .attempts
                    .last()
                    .is_some_and(|a| !a.abandoned && a.identity.is_none())
                    && progress.output_identity.is_none(),
                "Host v2 allocation is invalid"
            );
        } else if progress.stage == "pending" {
            ensure!(
                progress.attempts.is_empty() && progress.output_identity.is_none(),
                "Host v2 pending allocation has history"
            );
        } else if !matches!(journal.phase.as_str(), "rolling_back" | "rolled_back") {
            ensure!(
                progress.latest_identity() == progress.output_identity.as_deref()
                    && progress.output_identity.is_some(),
                "Host v2 prepared identity absent"
            );
        }
    }
    ensure!(
        journal.result.is_some() == (journal.phase == "rolled_back")
            && journal.receipt_id.is_some()
                == matches!(journal.phase.as_str(), "cleanup" | "committed"),
        "Host v2 journal result/commit mismatch"
    );
    if journal.phase != "preparing"
        && journal.phase != "rolling_back"
        && journal.phase != "rolled_back"
    {
        ensure!(
            journal
                .files
                .iter()
                .all(|p| rank(&p.stage).is_some_and(|n| n >= 4)),
            "Host v2 publication precedes prepared evidence"
        );
    }
    if journal.phase == "preparing" {
        ensure!(
            journal
                .files
                .iter()
                .all(|p| rank(&p.stage).is_some_and(|n| n <= 4 || n == 7)),
            "Host v2 file publication precedes applying phase"
        );
    }
    if matches!(journal.phase.as_str(), "cleanup" | "committed") {
        ensure!(
            journal
                .files
                .iter()
                .all(|p| rank(&p.stage).is_some_and(|n| n >= 6)),
            "Host v2 receipt cleanup precedes publication"
        );
    }
    if matches!(journal.phase.as_str(), "committed" | "rolled_back") {
        ensure!(
            journal.files.iter().all(|f| f.stage == "cleaned"),
            "Host v2 terminal cleanup incomplete"
        );
    }
    if let Some(result) = &journal.result {
        journal_receipt(selection, plan, result, true, d)?;
    }
    if let Some(id) = &journal.receipt_id {
        let receipt: Receipt = metadata(
            &state(selection).join("receipts").join(format!("{id}.json")),
            d,
        )?;
        ensure!(receipt.id == *id, "Host v2 receipt identifier differs");
        journal_receipt(selection, plan, &receipt, false, d)?;
    }
    Ok(())
}

fn transition(before: &Journal, after: &Journal) -> Result<()> {
    let allowed: &[&str] = match before.phase.as_str() {
        "preparing" => &["preparing", "applying", "rolling_back"],
        "applying" => &["applying", "cleanup", "rolling_back"],
        "cleanup" => &["cleanup", "committed"],
        "rolling_back" => &["rolling_back", "rolled_back"],
        _ => &[],
    };
    ensure!(
        before.plan_id == after.plan_id && allowed.contains(&after.phase.as_str()),
        "Host v2 transaction/direction changed"
    );
    for (old, new) in before.files.iter().zip(&after.files) {
        ensure!(
            old.source_security_hash == new.source_security_hash,
            "Host v2 original security baseline changed"
        );
        ensure!(
            old.output_identity
                .as_ref()
                .is_none_or(|id| new.output_identity.as_ref() == Some(id)),
            "Host v2 prepared identity changed"
        );
        ensure!(
            new.attempts.len() >= old.attempts.len()
                && new.attempts.len() <= old.attempts.len() + 1,
            "Host v2 attempt history changed"
        );
        let retry = new.attempts.len() > old.attempts.len();
        if retry {
            ensure!(
                before.phase == "preparing"
                    && after.phase == "preparing"
                    && ["pending", "allocating", "registered"].contains(&old.stage.as_str())
                    && new.stage == "allocating"
                    && new.output_identity.is_none(),
                "Host v2 retry after population/publication"
            );
        } else {
            let old_rank = rank(&old.stage).context("Invalid prior v2 stage")?;
            let new_rank = rank(&new.stage).context("Invalid next v2 stage")?;
            let removal = old.stage == "pending"
                && new.stage == "prepared"
                && new.attempts.is_empty()
                && new.output_identity.is_none();
            let rollback_cleanup = matches!(after.phase.as_str(), "rolling_back" | "rolled_back")
                && new.stage == "cleaned";
            ensure!(
                new_rank == old_rank || new_rank == old_rank + 1 || removal || rollback_cleanup,
                "Host v2 file stage skipped required durable evidence"
            );
        }
        for (index, prior) in old.attempts.iter().enumerate() {
            let next = &new.attempts[index];
            let may_register = index + 1 == old.attempts.len()
                && !retry
                && old.stage == "allocating"
                && new.stage == "registered"
                && prior.identity.is_none();
            ensure!(
                prior.ordinal == next.ordinal
                    && (prior.identity == next.identity
                        || (may_register && next.identity.is_some()))
                    && (prior.security_hash == next.security_hash
                        || (may_register && next.security_hash.is_some()))
                    && (prior.abandoned == next.abandoned
                        || (retry
                            && index + 1 == old.attempts.len()
                            && !prior.abandoned
                            && next.abandoned)),
                "Host v2 attempt identity/history changed"
            );
        }
    }
    Ok(())
}

pub(super) fn load(selection: &Selection, id: &str, d: &Deadline) -> Result<Journal> {
    #[cfg(all(test, windows))]
    let _profile = profile::span(profile::Phase::Load);
    ensure!(valid_id(id), "Invalid host v2 journal identifier");
    let mut events = BTreeMap::new();
    // One invocation validates one immutable plan. Every event is still read,
    // integrity-checked against that exact plan on every fresh load.
    let mut validated_plan: Option<Plan> = None;
    let (directory, leaves) = {
        #[cfg(all(test, windows))]
        let _profile_names = profile::span(profile::Phase::LoadNames);
        metadata_directory_names(&state(selection).join("journals").join(id), MAX_EVENTS, d)?
    };
    for name in leaves {
        if name.starts_with(".tmp-") {
            continue;
        }
        let event: Journal = {
            #[cfg(all(test, windows))]
            let _profile_read = profile::span(profile::Phase::EventRead);
            metadata_from_directory(&directory, &name, d)?
        };
        ensure!(
            event.schema_version == 2
                && event.journal_id == id
                && valid_id(&event.plan_id)
                && event.id == event_id(&event)
                && name == format!("{:08}-{}.json", event.sequence, event.id),
            "Host v2 event identity/schema differs"
        );
        if validated_plan.is_none() {
            #[cfg(all(test, windows))]
            let _profile_plan = profile::span(profile::Phase::PlanRead);
            validated_plan = Some(load_plan(selection, &event.plan_id, d)?);
        }
        let plan = validated_plan.as_ref().context("Host v2 plan is absent")?;
        ensure!(
            event.plan_id == plan.id,
            "Host v2 journal plan changed within its event chain"
        );
        {
            #[cfg(all(test, windows))]
            let _profile_validate = profile::span(profile::Phase::EventValidate);
            validate_event(selection, plan, &event, d)?;
        }
        ensure!(
            events.insert(event.sequence, event).is_none(),
            "Host v2 event fork"
        );
    }
    #[cfg(all(test, windows))]
    let _profile_chain = profile::span(profile::Phase::Chain);
    let mut previous: Option<Journal> = None;
    for (sequence, event) in events {
        ensure!(
            sequence == previous.as_ref().map_or(0, |p| p.sequence + 1)
                && event.previous_event.as_deref() == previous.as_ref().map(|p| p.id.as_str()),
            "Host v2 chain mismatch"
        );
        if let Some(prior) = &previous {
            transition(prior, &event)?;
        } else {
            ensure!(
                event.phase == "preparing"
                    && event
                        .files
                        .iter()
                        .all(|p| ["pending", "cleaned"].contains(&p.stage.as_str())
                            && p.attempts.is_empty()
                            && p.output_identity.is_none()),
                "Host v2 initial event contains unearned ownership"
            );
        }
        previous = Some(event);
    }
    previous.context("Host v2 journal contains no committed events")
}

pub(super) fn append_event(
    selection: &Selection,
    journal: &mut Journal,
    d: &Deadline,
) -> Result<()> {
    #[cfg(all(test, windows))]
    let _profile = profile::span(profile::Phase::Append);
    let prior = if journal.id.is_empty() {
        None
    } else {
        Some(load(selection, &journal.journal_id, d)?)
    };
    if let Some(prior) = &prior {
        ensure!(
            prior.id == journal.id,
            "Host v2 journal changed before append"
        );
        journal.previous_event = Some(prior.id.clone());
        journal.sequence = prior
            .sequence
            .checked_add(1)
            .context("Host v2 sequence overflow")?;
        transition(prior, journal)?;
    }
    ensure!(
        journal.sequence < MAX_EVENTS as u32,
        "Host v2 event capacity exceeded"
    );
    let plan = load_plan(selection, &journal.plan_id, d)?;
    validate_event(selection, &plan, journal, d)?;
    journal.id = event_id(journal);
    let directory = state(selection).join("journals").join(&journal.journal_id);
    let _guard = if io::entry_info(&directory, d)?.is_none() {
        io::create_directory(&directory, d)?
    } else {
        io::guarded_directory(&directory, d)?
    };
    immutable(
        &directory.join(format!("{:08}-{}.json", journal.sequence, journal.id)),
        journal,
        d,
    )
}

pub(super) fn reserve_suffix(
    selection: &Selection,
    journal: &Journal,
    ordinary_events: usize,
    result: Option<&Receipt>,
    d: &Deadline,
) -> Result<()> {
    let extra = ordinary_events
        .checked_add(usize::from(result.is_some()))
        .context("V2 reservation overflow")?;
    let existing = if journal.id.is_empty() {
        0
    } else {
        journal.sequence as usize + 1
    };
    ensure!(
        existing.checked_add(extra).is_some_and(|n| n <= MAX_EVENTS),
        "Host v2 journal event capacity exhausted"
    );
    let mut bound = journal.clone();
    bound.id = "a".repeat(64);
    bound.previous_event = Some("a".repeat(64));
    bound.sequence = (MAX_EVENTS - 1) as u32;
    bound.phase = "rolling_back".into();
    bound.receipt_id = Some("a".repeat(64));
    bound.result = None;
    for file in &mut bound.files {
        file.stage = "populating".into();
        file.output_identity = Some(format!("{}:{}", "a".repeat(16), "a".repeat(32)));
        file.attempts = (0..MAX_ATTEMPTS)
            .map(|i| Attempt {
                ordinal: i as u8,
                identity: file.output_identity.clone(),
                security_hash: Some("a".repeat(64)),
                abandoned: i + 1 < MAX_ATTEMPTS,
            })
            .collect();
    }
    let row = crate::json::encode_bounded(&bound, MAX_PLAN, d)?.len();
    let mut bytes = row
        .checked_mul(ordinary_events)
        .context("V2 byte reservation overflow")?;
    if let Some(result) = result {
        bound.result = Some(result.clone());
        bound.phase = "rolled_back".into();
        bytes = bytes
            .checked_add(crate::json::encode_bounded(&bound, MAX_PLAN, d)?.len())
            .context("V2 result reservation overflow")?;
    }
    reserve_directory(
        &state(selection).join("journals").join(&journal.journal_id),
        MAX_EVENTS,
        extra,
        bytes,
        d,
    )
}

// A new attempt re-admits the remaining successful suffix before allocation.
// Row byte bounds still include all three immutable allocation attempts.
pub(super) fn remaining_events(plan: &Plan, journal: &Journal) -> usize {
    let mut events = usize::from(journal.id.is_empty());
    events += match journal.phase.as_str() {
        "preparing" => 3,
        "applying" => 2,
        "cleanup" => 1,
        _ => 0,
    };
    for (file, progress) in plan.files.iter().zip(&journal.files) {
        if file.before_hash == file.after_hash || progress.stage == "cleaned" {
            continue;
        }
        let stage = rank(&progress.stage).unwrap_or(0);
        if file.after_hash.is_some() {
            events += match progress.stage.as_str() {
                "pending" | "allocating" | "registered" => 4,
                "populating" => 1,
                _ => 0,
            };
        } else if progress.stage == "pending" {
            events += 1;
        }
        events += usize::from(stage < 5) + usize::from(stage < 6) + usize::from(stage < 7);
    }
    events
}

pub(super) fn commit_receipt(
    selection: &Selection,
    plan: &Plan,
    journal: &Journal,
    d: &Deadline,
) -> Result<Receipt> {
    super::commit_receipt(selection, plan, &summary(journal), d)
}

pub(super) fn registration_token(
    selection: &Selection,
    plan: &Plan,
    journal: &Journal,
    index: usize,
    d: &Deadline,
) -> Result<DurableOutputRegistration> {
    let durable = load(selection, &journal.journal_id, d)?;
    ensure!(
        durable == *journal && journal.plan_id == plan.id,
        "Host output registration is not durable"
    );
    let file = journal
        .files
        .get(index)
        .context("Output registration index invalid")?;
    ensure!(
        ["registered", "populating"].contains(&file.stage.as_str()),
        "Output registration stage invalid"
    );
    let attempt = file.attempts.last().context("Output allocation absent")?;
    Ok(DurableOutputRegistration {
        plan_id: plan.id.clone(),
        journal_id: journal.journal_id.clone(),
        root_index: index,
        attempt: attempt.ordinal,
        path: auxiliary(selection, plan, index, "after")?,
        identity: file
            .latest_identity()
            .context("Output identity absent")?
            .into(),
        after_hash: plan.files[index]
            .after_hash
            .clone()
            .context("Output hash absent")?,
        security_hash: attempt
            .security_hash
            .clone()
            .context("Output security fingerprint absent")?,
    })
}

pub(super) fn register_output(
    selection: &Selection,
    plan: &Plan,
    journal: &mut Journal,
    index: usize,
    id: &str,
    security_hash: &str,
    d: &Deadline,
) -> Result<DurableOutputRegistration> {
    ensure!(
        native_identity(id) && valid_id(security_hash),
        "Invalid allocated output identity/security"
    );
    let file = journal
        .files
        .get_mut(index)
        .context("Allocation index invalid")?;
    ensure!(
        file.stage == "allocating",
        "Output allocation stage differs"
    );
    let attempt = file.attempts.last_mut().context("Output attempt absent")?;
    ensure!(
        attempt.identity.is_none() && !attempt.abandoned,
        "Output attempt already registered"
    );
    attempt.identity = Some(id.into());
    attempt.security_hash = Some(security_hash.into());
    file.stage = "registered".into();
    append_event(selection, journal, d)?;
    registration_token(selection, plan, journal, index, d)
}

pub(super) fn check_source_security(
    progress: &Progress,
    original: Option<&io::HeldFile>,
    d: &Deadline,
) -> Result<()> {
    #[cfg(all(test, windows))]
    let _profile = profile::span(profile::Phase::SourceSecurity);
    ensure!(
        original.is_some() == progress.source_security_hash.is_some(),
        "Host original security proof is absent or unexpected"
    );
    if let Some(original) = original {
        ensure!(
            Some(io::security_fingerprint(original, d)?) == progress.source_security_hash,
            "Host original security changed"
        );
    }
    Ok(())
}

pub(super) fn prepare_output(
    selection: &Selection,
    plan: &Plan,
    journal: &mut Journal,
    index: usize,
    original: Option<&io::HeldFile>,
    canonical: &[u8],
    d: &Deadline,
) -> Result<io::HeldFile> {
    #[cfg(all(test, windows))]
    let _profile = profile::span(profile::Phase::Prepare);
    ensure!(
        journal.phase == "preparing"
            && ["pending", "allocating", "registered"]
                .contains(&journal.files[index].stage.as_str()),
        "Host output cannot start a new attempt after population"
    );
    check_source_security(&journal.files[index], original, d)?;
    let path = auxiliary(selection, plan, index, "after")?;
    ensure_absent(&path, d)?;
    ensure_absent(&auxiliary(selection, plan, index, "before")?, d)?;
    let current = &journal.files[index];
    ensure!(
        current.attempts.len() < MAX_ATTEMPTS,
        "Host output allocation attempts exhausted; Rollback remains available"
    );
    reserve_suffix(selection, journal, remaining_events(plan, journal), None, d)?;
    let progress = &mut journal.files[index];
    if let Some(prior) = progress.attempts.last_mut() {
        prior.abandoned = true;
    }
    progress.attempts.push(Attempt {
        ordinal: progress.attempts.len() as u8,
        identity: None,
        security_hash: None,
        abandoned: false,
    });
    progress.stage = "allocating".into();
    append_event(selection, journal, d)?;
    #[cfg(all(test, windows))]
    output_recovery_tests::barrier("allocating", plan, index, None)?;
    let armed = io::allocate_output(&path, original, d)?;
    let binding = armed.binding();
    #[cfg(all(test, windows))]
    output_recovery_tests::barrier(
        "allocated",
        plan,
        index,
        Some((&binding.identity, binding.byte_length)),
    )?;
    check_source_security(&journal.files[index], original, d)?;
    ensure!(
        journal.files[index]
            .source_security_hash
            .as_ref()
            .is_none_or(|expected| expected == &binding.security_hash),
        "Host allocated output security differs from the original baseline"
    );
    let token = register_output(
        selection,
        plan,
        journal,
        index,
        &binding.identity,
        &binding.security_hash,
        d,
    )?;
    #[cfg(all(test, windows))]
    output_recovery_tests::barrier(
        "registered",
        plan,
        index,
        Some((&binding.identity, binding.byte_length)),
    )?;
    let mut retained = armed.retain(&token, d)?;
    #[cfg(all(test, windows))]
    output_recovery_tests::barrier(
        "disarmed",
        plan,
        index,
        Some((&binding.identity, binding.byte_length)),
    )?;
    journal.files[index].stage = "populating".into();
    append_event(selection, journal, d)?;
    #[cfg(all(test, windows))]
    output_recovery_tests::barrier(
        "populating",
        plan,
        index,
        Some((&binding.identity, binding.byte_length)),
    )?;
    loop {
        check_source_security(&journal.files[index], original, d)?;
        if retained.append_chunk(canonical, d)? == 0 {
            break;
        }
        #[cfg(all(test, windows))]
        {
            let b = retained.binding();
            output_recovery_tests::barrier(
                "partial",
                plan,
                index,
                Some((&b.identity, b.byte_length)),
            )?;
        }
    }
    check_source_security(&journal.files[index], original, d)?;
    let output = retained.finalize(canonical, d)?;
    #[cfg(all(test, windows))]
    output_recovery_tests::barrier(
        "flushed",
        plan,
        index,
        Some((&output.identity, output.byte_length)),
    )?;
    journal.files[index].output_identity = Some(output.identity.clone());
    journal.files[index].stage = "prepared".into();
    append_event(selection, journal, d)?;
    #[cfg(all(test, windows))]
    output_recovery_tests::barrier(
        "prepared",
        plan,
        index,
        Some((&output.identity, output.byte_length)),
    )?;
    Ok(output)
}
