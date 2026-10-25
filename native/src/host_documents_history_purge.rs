//! Explicit single-frame physical purge. Historical archive receipts remain intact.
//! Serialized review data cannot construct either host authority or deletion permission.
use super::*;

const PURGE_EVENTS: u32 = 3;
const PURGE_REPLY_RESERVE: usize = 8192;

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct HistoryPurgeFileWitness {
    pub identity: String,
    pub security_sha256: String,
    pub sha256: String,
    pub byte_length: usize,
}
impl HistoryPurgeFileWitness {
    fn read(file: &io::HeldFile, d: &Deadline) -> Result<Self> {
        Ok(Self {
            identity: file.identity.clone(),
            security_sha256: io::security_fingerprint(file, d)?,
            sha256: file.hash.clone(),
            byte_length: file.byte_length,
        })
    }
    fn valid(&self, maximum: usize) -> bool {
        !self.identity.is_empty()
            && self.identity.len() <= 128
            && valid_id(&self.security_sha256)
            && valid_id(&self.sha256)
            && self.byte_length <= maximum
    }
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct HistoryPurgeRetainedWitness {
    pub role: String,
    pub name: String,
    pub file: HistoryPurgeFileWitness,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct HistoryArchivePurgePlan {
    pub schema_version: u32,
    pub id: String,
    pub archive: HistoryArchivePlan,
    pub archive_event_id: String,
    pub frame: HistoryPurgeFileWitness,
    pub source: HistoryPurgeFileWitness,
    pub source_revision: String,
    pub owner: HistoryPurgeFileWitness,
    pub from_journal_state: String,
    pub to_journal_state: String,
    pub retained: Vec<HistoryPurgeRetainedWitness>,
    pub nonce: String,
}
fn purge_plan_id(p: &HistoryArchivePurgePlan) -> String {
    let mut copy = p.clone();
    copy.id.clear();
    identity(&("host_document_history_purge_v1", copy))
}
fn purge_prefix(archive: &HistoryArchivePlan) -> String {
    format!("yoke-doc-history-purge-{}-", archive.id)
}
fn purge_stem(p: &HistoryArchivePurgePlan) -> String {
    format!("{}{}", purge_prefix(&p.archive), p.id)
}
fn purge_marker_path(selected: &BoundDocument) -> Result<PathBuf> {
    Ok(selected.parent_path.join(format!(
        ".yoke-doc-history-purge-{}.pending.json",
        target_key(selected)?
    )))
}
pub(super) fn pending(selected: &BoundDocument, d: &Deadline) -> Result<bool> {
    exists(&purge_marker_path(selected)?, d)
}

/// Both independently selected host write grants and an explicit archive root.
pub struct HistoryArchivePurgeAccess {
    archive: HistoryArchiveAccess,
}
pub struct HistoryArchivePurgeApproval {
    plan_id: String,
    archive_identity: String,
}
/// Separate explicit host permission; never inferred from archive approval.
pub struct HistoryArchivePurgePermission {
    _private: (),
}
impl HistoryArchivePurgePermission {
    pub fn new(allow_state_deletion: bool) -> Result<Self> {
        ensure!(
            allow_state_deletion,
            "Archive purge requires explicit host state deletion permission"
        );
        Ok(Self { _private: () })
    }
}
impl HistoryArchivePurgeAccess {
    pub fn new(
        from: &Access,
        to: &Access,
        archive: &HistoryArchivePlan,
        root: &Path,
        a: &WriteAuthority,
        b: &WriteAuthority,
        d: &Deadline,
    ) -> Result<Self> {
        let access = HistoryArchiveAccess::new(from, to, &archive.migration, root, a, b, d)?
            .with_record_budget(archive.record_budget)?;
        access.check(from, to, archive, d)?;
        Ok(Self { archive: access })
    }
    fn check(
        &self,
        from: &Access,
        to: &Access,
        p: &HistoryArchivePurgePlan,
        d: &Deadline,
    ) -> Result<()> {
        check_purge_plan(&self.archive, from, to, p, d)
    }
    pub fn approve(
        &self,
        from: &Access,
        to: &Access,
        p: &HistoryArchivePurgePlan,
        d: &Deadline,
    ) -> Result<HistoryArchivePurgeApproval> {
        self.check(from, to, p, d)?;
        let _old = from.hold_bindings(&p.archive.migration.from_document_id, d)?;
        let _new = to.hold_bindings(&p.archive.migration.to_document_id, d)?;
        let _locks = locks(from, to, d)?;
        let root = self.archive.root(from, to, d)?;
        let _lock = io::open_state_lock(&root.path.join(".yoke-history.lock"), d)?;
        no_pending(from, to, &p.archive, None, d)?;
        ensure!(
            evidence(from, to, &self.archive, &p.archive, d)?.is_none(),
            "Archive already has an active or completed purge"
        );
        let _current = current_purge(from, to, &self.archive, p, d)?;
        let _frame = held_frame(from, &root.path, p, false, d)?
            .context("Selected archive frame unavailable")?;
        Ok(HistoryArchivePurgeApproval {
            plan_id: p.id.clone(),
            archive_identity: root.identity.clone(),
        })
    }
}
fn check_purge_plan(
    access: &HistoryArchiveAccess,
    from: &Access,
    to: &Access,
    p: &HistoryArchivePurgePlan,
    d: &Deadline,
) -> Result<()> {
    access.check(from, to, &p.archive, d)?;
    ensure!(
        p.schema_version == 1
            && valid_id(&p.id)
            && p.id == purge_plan_id(p)
            && p.frame.valid(MAX_ARCHIVE)
            && p.frame.byte_length > 0
            && p.source.valid(MAX_BYTES)
            && p.owner.valid(2048)
            && p.owner.byte_length > 0
            && [
                &p.archive_event_id,
                &p.source_revision,
                &p.from_journal_state,
                &p.to_journal_state,
                &p.nonce
            ]
            .iter()
            .all(|v| valid_id(v))
            && !p.retained.is_empty()
            && p.retained.len() <= 2 * MAX_MIGRATION_EVENTS as usize + 12
            && p.retained.iter().all(|r| r.file.valid(MAX_PLAN)
                && r.name.is_ascii()
                && !r.name.contains(['/', '\\', ':'])
                && r.name.len() <= 240
                && matches!(r.role.as_str(), "archive" | "migration" | "target"))
            && serde_json::to_vec(p)?.len() <= MAX_PLAN - MAX_EVENT,
        "Invalid or foreign history purge plan"
    );
    Ok(())
}

fn no_pending(
    from: &Access,
    to: &Access,
    archive: &HistoryArchivePlan,
    own: Option<&HistoryArchivePurgePlan>,
    d: &Deadline,
) -> Result<()> {
    let (a, b) = selected_pair(
        from,
        to,
        &archive.migration.from_document_id,
        &archive.migration.to_document_id,
    )?;
    for selected in [a, b] {
        ensure!(
            !exists(&marker_path(selected)?, d)? && !exists(&history_marker(selected)?, d)?,
            "Unfinished binding migration or history archive blocks purge"
        );
        if pending(selected, d)? {
            let p = own.context("Host document history purge remains pending")?;
            check_purge_marker(selected, p, d)?;
        }
    }
    Ok(())
}

// Admit the complete selected namespace, including orphan names, before a digest.
// Other selected documents remain independent; their canonical records are admitted
// so an unknown same-binding orphan is not silently called unrelated.
fn dependency_state(access: &Access, document: &str, d: &Deadline) -> Result<String> {
    let root = state(access, false, d)?;
    let directory = io::open_metadata_directory(&root.root.path, d)?;
    let names = directory.names(MAX_ENTRIES, d)?;
    let prefix = format!("yoke-doc-{}-", access.binding);
    let mut expected = BTreeSet::new();
    let mut rows = Vec::new();
    let mut total = 0u64;
    for name in &names {
        d.check()?;
        if let Some(info) = directory.entry_info(name, d)? {
            total = total
                .checked_add(info.length)
                .context("Journal namespace byte overflow")?;
            ensure!(
                total <= MAX_STATE_BYTES,
                "Journal namespace byte capacity exceeded"
            );
        }
        if !name.starts_with(&prefix) || !name.ends_with(".plan.json") {
            continue;
        }
        let id = &name[prefix.len()..name.len() - ".plan.json".len()];
        let loaded = load(access, &root.root.path, id, d)?;
        let stem = stem(access, id)?;
        expected.insert(format!("{stem}.plan.json"));
        expected.extend((1..=loaded.event.sequence).map(|n| format!("{stem}.e{n:04}.json")));
        if loaded.plan.document_id == document {
            ensure!(
                matches!(loaded.event.phase, Phase::Committed | Phase::RolledBack),
                "Unfinished selected journal blocks purge"
            );
            let selected = &access.documents[document];
            ensure!(
                !exists(&backup_path(selected, id), d)?,
                "Occupied selected journal backup blocks purge"
            );
            for attempt in 0..=loaded.event.attempt {
                ensure!(
                    !exists(&output_path(selected, id, attempt), d)?,
                    "Occupied selected journal output blocks purge"
                );
            }
            rows.push((id.to_owned(), loaded.event.id));
        }
    }
    for name in names.iter().filter(|n| n.starts_with(&prefix)) {
        ensure!(
            expected.contains(name),
            "Unknown selected journal namespace record blocks purge"
        );
    }
    rows.sort();
    Ok(identity(&("host_document_purge_dependencies_v1", rows)))
}

struct CurrentPurge {
    source: io::HeldFile,
    owner: io::HeldFile,
    retained: Vec<io::HeldFile>,
    archive_event: String,
    from_state: String,
    to_state: String,
}
fn retained_files(
    from: &Access,
    to: &Access,
    access: &HistoryArchiveAccess,
    archive: &HistoryArchivePlan,
    d: &Deadline,
) -> Result<(String, Vec<io::HeldFile>)> {
    let root = access.root(from, to, d)?;
    let event = load_archive_event(&root.path, archive, d)?;
    ensure!(
        event.phase == ArchivePhase::Committed,
        "Committed archive receipt required for purge evidence"
    );
    let a = &from.documents[&archive.migration.from_document_id];
    require_completion(a, archive, d)?;
    let old = state(from, false, d)?;
    let migration_event = load_event(from, to, &archive.migration, &old.root.path, d)?;
    ensure!(
        migration_event.phase == MigrationPhase::Committed,
        "Committed migration receipt required for purge evidence"
    );
    let migration_complete = a.parent_path.join(format!(
        ".yoke-doc-migration-{}.complete.json",
        archive.migration.id
    ));
    let complete = io::read_file(&migration_complete, 2048, false, d)?
        .context("Retained migration completion absent")?;
    ensure!(
        complete.bytes == serde_json::to_vec(&marker(&archive.migration))?,
        "Migration completion differs"
    );
    drop(complete);
    let mut paths = vec![plan_path(&root.path, &history_stem(archive))];
    paths.extend((1..=event.sequence).map(|n| event_path(&root.path, &history_stem(archive), n)));
    paths.push(plan_path(
        &old.root.path,
        &migration_stem(&archive.migration),
    ));
    paths.extend(
        (1..=migration_event.sequence)
            .map(|n| event_path(&old.root.path, &migration_stem(&archive.migration), n)),
    );
    paths.push(
        a.parent_path
            .join(format!(".yoke-doc-history-{}.complete.json", archive.id)),
    );
    paths.push(migration_complete);
    let mut retained = Vec::new();
    for path in paths {
        retained.push(
            io::read_file(&path, MAX_PLAN, false, d)?
                .context("Retained maintenance evidence absent")?,
        );
    }
    Ok((event.id, retained))
}
fn observe_current(
    from: &Access,
    to: &Access,
    access: &HistoryArchiveAccess,
    archive: &HistoryArchivePlan,
    d: &Deadline,
) -> Result<CurrentPurge> {
    completed_migration(from, to, &archive.migration, d)?;
    let root = access.root(from, to, d)?;
    let event = load_archive_event(&root.path, archive, d)?;
    ensure!(
        event.phase == ArchivePhase::Committed,
        "Only a committed history archive frame is eligible for physical purge"
    );
    let (a, b) = selected_pair(
        from,
        to,
        &archive.migration.from_document_id,
        &archive.migration.to_document_id,
    )?;
    require_completion(a, archive, d)?;
    ensure!(
        original_records(from, archive, true, d)? == 0,
        "Original journal records reappeared; purge refused"
    );
    // Even absent archived journals retain occupied-role exclusions. Attempt
    // space is finite in the original journal schema, independent of live files.
    for record in archive
        .records
        .iter()
        .filter(|r| r.name.ends_with(".plan.json"))
    {
        let prefix = format!("yoke-doc-{}-", from.binding);
        let id = record
            .name
            .strip_prefix(&prefix)
            .and_then(|n| n.strip_suffix(".plan.json"))
            .context("Invalid archived journal name")?;
        ensure!(valid_id(id), "Invalid archived journal identity");
        ensure!(
            !exists(&backup_path(a, id), d)?,
            "Archived journal backup is occupied"
        );
        for attempt in 0..3 {
            d.check()?;
            ensure!(
                !exists(&output_path(a, id, attempt), d)?,
                "Archived journal output is occupied"
            );
        }
    }
    let (_, source) = to.current(&archive.migration.to_document_id, d)?;
    std::str::from_utf8(&source.bytes).context("Current selected host document is not UTF-8")?;
    ensure!(
        source.byte_length <= from.max_bytes.min(to.max_bytes),
        "Current source exceeds the complete host selections' document budgets"
    );
    let owner = io::read_file(&owner_path(b)?, 2048, false, d)?.context("Current owner absent")?;
    ensure!(
        owner.bytes == serde_json::to_vec(&archive.migration.after_owner)?,
        "Held current owner differs from the active migration destination generation"
    );
    let from_state = dependency_state(from, &archive.migration.from_document_id, d)?;
    let to_state = dependency_state(to, &archive.migration.to_document_id, d)?;
    let (_, retained) = retained_files(from, to, access, archive, d)?;
    Ok(CurrentPurge {
        source,
        owner,
        retained,
        archive_event: event.id,
        from_state,
        to_state,
    })
}
fn retained_witnesses(
    current: &CurrentPurge,
    archive_root: &Path,
    journal_root: &Path,
    d: &Deadline,
) -> Result<Vec<HistoryPurgeRetainedWitness>> {
    file_witnesses(&current.retained, archive_root, journal_root, d)
}
fn file_witnesses(
    files: &[io::HeldFile],
    archive_root: &Path,
    journal_root: &Path,
    d: &Deadline,
) -> Result<Vec<HistoryPurgeRetainedWitness>> {
    files
        .iter()
        .map(|file| {
            Ok(HistoryPurgeRetainedWitness {
                role: if file.path.parent() == Some(archive_root) {
                    "archive"
                } else if file.path.parent() == Some(journal_root) {
                    "migration"
                } else {
                    "target"
                }
                .into(),
                name: file
                    .path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .context("Retained record name must be Unicode")?
                    .into(),
                file: HistoryPurgeFileWitness::read(file, d)?,
            })
        })
        .collect()
}
fn current_purge(
    from: &Access,
    to: &Access,
    access: &HistoryArchiveAccess,
    p: &HistoryArchivePurgePlan,
    d: &Deadline,
) -> Result<CurrentPurge> {
    let current = observe_current(from, to, access, &p.archive, d)?;
    let selected = &to.documents[&p.archive.migration.to_document_id];
    ensure!(
        current.archive_event == p.archive_event_id
            && HistoryPurgeFileWitness::read(&current.source, d)? == p.source
            && to.revision(selected, &current.source, &p.source.security_sha256)
                == p.source_revision
            && HistoryPurgeFileWitness::read(&current.owner, d)? == p.owner
            && current.from_state == p.from_journal_state
            && current.to_state == p.to_journal_state
            && retained_witnesses(
                &current,
                &access.root,
                &from.journal.as_ref().context("Original journal absent")?.0,
                d
            )? == p.retained,
        "Source, owner, generation, dependency or retained archive evidence changed since purge review"
    );
    Ok(current)
}
fn held_frame(
    from: &Access,
    root: &Path,
    p: &HistoryArchivePurgePlan,
    delete: bool,
    d: &Deadline,
) -> Result<Option<io::HeldFile>> {
    let held = io::read_owned_file(
        &archive_path(root, &p.archive),
        &[&p.frame.identity],
        p.frame.byte_length,
        delete,
        d,
    )?;
    if let Some(file) = &held {
        ensure!(
            HistoryPurgeFileWitness::read(file, d)? == p.frame,
            "Selected archive frame identity, bytes or security changed"
        );
        admit_frame(from, &p.archive, &file.bytes, d)?;
    }
    Ok(held)
}
pub fn plan_history_archive_purge(
    from: &Access,
    to: &Access,
    access: &HistoryArchivePurgeAccess,
    archive: &HistoryArchivePlan,
    d: &Deadline,
) -> Result<HistoryArchivePurgePlan> {
    access.archive.check(from, to, archive, d)?;
    let _old = from.hold_bindings(&archive.migration.from_document_id, d)?;
    let _new = to.hold_bindings(&archive.migration.to_document_id, d)?;
    let _locks = locks(from, to, d)?;
    let root = access.archive.root(from, to, d)?;
    let _lock = io::open_state_lock(&root.path.join(".yoke-history.lock"), d)?;
    no_pending(from, to, archive, None, d)?;
    ensure!(
        evidence(from, to, &access.archive, archive, d)?.is_none(),
        "Archive already has an active or completed purge"
    );
    let current = observe_current(from, to, &access.archive, archive, d)?;
    let frame = validate_archive(from, &root.path, archive, d)?;
    let source = HistoryPurgeFileWitness::read(&current.source, d)?;
    let mut p = HistoryArchivePurgePlan {
        schema_version: 1,
        id: String::new(),
        archive: archive.clone(),
        archive_event_id: current.archive_event.clone(),
        frame: HistoryPurgeFileWitness::read(&frame, d)?,
        source_revision: to.revision(
            &to.documents[&archive.migration.to_document_id],
            &current.source,
            &source.security_sha256,
        ),
        source,
        owner: HistoryPurgeFileWitness::read(&current.owner, d)?,
        from_journal_state: current.from_state.clone(),
        to_journal_state: current.to_state.clone(),
        retained: retained_witnesses(
            &current,
            &root.path,
            &from.journal.as_ref().context("Original journal absent")?.0,
            d,
        )?,
        nonce: nonce()?,
    };
    p.id = purge_plan_id(&p);
    access.check(from, to, &p, d)?;
    Ok(p)
}

#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum PurgePhase {
    Prepared,
    Deleting,
    Removed,
    Committed,
    Cancelled,
}
#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Removal {
    None,
    Confirmed,
    Unverified,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct PurgeEvent {
    schema_version: u32,
    id: String,
    plan_id: String,
    sequence: u32,
    previous: Option<String>,
    phase: PurgePhase,
    removal: Removal,
}
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct PurgeHeader {
    schema_version: u32,
    plan: HistoryArchivePurgePlan,
    initial: PurgeEvent,
}
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct PurgeMarker {
    schema_version: u32,
    plan_id: String,
    archive_plan_id: String,
    archive_identity: String,
}
fn purge_marker(p: &HistoryArchivePurgePlan) -> PurgeMarker {
    PurgeMarker {
        schema_version: 1,
        plan_id: p.id.clone(),
        archive_plan_id: p.archive.id.clone(),
        archive_identity: p.archive.archive_identity.clone(),
    }
}
fn purge_event_id(e: &PurgeEvent) -> String {
    let mut copy = e.clone();
    copy.id.clear();
    identity(&("host_document_history_purge_event_v1", copy))
}
fn purge_initial(p: &HistoryArchivePurgePlan) -> PurgeEvent {
    let mut e = PurgeEvent {
        schema_version: 1,
        id: String::new(),
        plan_id: p.id.clone(),
        sequence: 0,
        previous: None,
        phase: PurgePhase::Prepared,
        removal: Removal::None,
    };
    e.id = purge_event_id(&e);
    e
}
fn purge_step(a: &PurgeEvent, b: &PurgeEvent) -> bool {
    match (a.phase, b.phase) {
        (PurgePhase::Prepared, PurgePhase::Deleting | PurgePhase::Cancelled) => {
            a.removal == Removal::None && b.removal == Removal::None
        }
        (PurgePhase::Deleting, PurgePhase::Removed) => {
            a.removal == Removal::None
                && matches!(b.removal, Removal::Confirmed | Removal::Unverified)
        }
        (PurgePhase::Removed, PurgePhase::Committed) => {
            a.removal == b.removal && a.removal != Removal::None
        }
        _ => false,
    }
}
fn load_purge_event(root: &Path, p: &HistoryArchivePurgePlan, d: &Deadline) -> Result<PurgeEvent> {
    let stem = purge_stem(p);
    let header: PurgeHeader = canonical(&plan_path(root, &stem), MAX_PLAN, d)?;
    let mut e = purge_initial(p);
    ensure!(
        header.schema_version == 1
            && serde_json::to_vec(&header.plan)? == serde_json::to_vec(p)?
            && serde_json::to_vec(&header.initial)? == serde_json::to_vec(&e)?,
        "Purge header differs"
    );
    let directory = io::open_metadata_directory(root, d)?;
    let mut names = directory
        .names(MAX_ARCHIVE_ENTRIES, d)?
        .into_iter()
        .filter(|n| n.starts_with(&format!("{stem}.")) && n != &format!("{stem}.plan.json"))
        .collect::<Vec<_>>();
    names.sort();
    ensure!(
        names.len() <= PURGE_EVENTS as usize,
        "Purge event capacity exceeded"
    );
    for (index, name) in names.into_iter().enumerate() {
        let sequence = index as u32 + 1;
        ensure!(
            root.join(name) == event_path(root, &stem, sequence),
            "Purge event gap or foreign entry"
        );
        let next: PurgeEvent = canonical(&event_path(root, &stem, sequence), MAX_EVENT, d)?;
        ensure!(
            next.schema_version == 1
                && next.id == purge_event_id(&next)
                && next.plan_id == p.id
                && next.sequence == sequence
                && next.previous.as_ref() == Some(&e.id)
                && purge_step(&e, &next),
            "Purge event lineage, removal attribution or direction differs"
        );
        e = next;
    }
    Ok(e)
}
fn next_purge(
    root: &Path,
    p: &HistoryArchivePurgePlan,
    e: &mut PurgeEvent,
    phase: PurgePhase,
    removal: Removal,
    d: &Deadline,
) -> Result<()> {
    ensure!(
        load_purge_event(root, p, d)?.id == e.id,
        "Stale purge event"
    );
    let mut next = e.clone();
    next.sequence += 1;
    next.previous = Some(e.id.clone());
    next.phase = phase;
    next.removal = removal;
    ensure!(
        next.sequence <= PURGE_EVENTS && purge_step(e, &next),
        "Purge transition or event capacity refused"
    );
    next.id = purge_event_id(&next);
    write_record(
        root,
        &event_path(root, &purge_stem(p), next.sequence),
        &serde_json::to_vec(&next)?,
        d,
    )?;
    *e = next;
    Ok(())
}
fn check_purge_marker(
    selected: &BoundDocument,
    p: &HistoryArchivePurgePlan,
    d: &Deadline,
) -> Result<()> {
    let held = io::read_file(&purge_marker_path(selected)?, 2048, false, d)?
        .context("Pending purge marker absent")?;
    ensure!(
        held.bytes == serde_json::to_vec(&purge_marker(p))?,
        "Foreign purge marker preserved"
    );
    Ok(())
}
fn purge_completion_path(selected: &BoundDocument, p: &HistoryArchivePurgePlan) -> PathBuf {
    selected
        .parent_path
        .join(format!(".yoke-doc-history-purge-{}.complete.json", p.id))
}
fn purge_completion(
    selected: &BoundDocument,
    p: &HistoryArchivePurgePlan,
    d: &Deadline,
) -> Result<()> {
    let held = io::read_file(&purge_completion_path(selected, p), 2048, false, d)?
        .context("Retained purge completion absent; outcome unconfirmed")?;
    ensure!(
        held.bytes == serde_json::to_vec(&purge_marker(p))?,
        "Purge completion differs"
    );
    Ok(())
}
fn finish_purge_marker(
    selected: &BoundDocument,
    p: &HistoryArchivePurgePlan,
    d: &Deadline,
) -> Result<()> {
    let path = purge_marker_path(selected)?;
    if let Some(mut held) = io::read_file(&path, 2048, true, d)? {
        ensure!(
            held.bytes == serde_json::to_vec(&purge_marker(p))?,
            "Foreign purge marker preserved"
        );
        held.rename_no_replace(&purge_completion_path(selected, p), d)?;
    } else {
        purge_completion(selected, p, d)?;
    }
    Ok(())
}

// A selected archive can have retained cancellations, but only one noncancelled
// purge. Unknown relevant names and orphan events cannot hide behind frame loss.
fn evidence(
    from: &Access,
    to: &Access,
    access: &HistoryArchiveAccess,
    archive: &HistoryArchivePlan,
    d: &Deadline,
) -> Result<Option<(HistoryArchivePurgePlan, PurgeEvent)>> {
    let root = access.root(from, to, d)?;
    let directory = io::open_metadata_directory(&root.path, d)?;
    let names = directory.names(MAX_ARCHIVE_ENTRIES, d)?;
    let prefix = purge_prefix(archive);
    let mut expected = BTreeSet::new();
    let mut active = None;
    for name in names
        .iter()
        .filter(|n| n.starts_with(&prefix) && n.ends_with(".plan.json"))
    {
        let header: PurgeHeader = canonical(&root.path.join(name), MAX_PLAN, d)?;
        let p = header.plan;
        check_purge_plan(access, from, to, &p, d)?;
        ensure!(
            serde_json::to_vec(&p.archive)? == serde_json::to_vec(archive)?
                && *name == format!("{}.plan.json", purge_stem(&p)),
            "Foreign purge selection or header name"
        );
        let e = load_purge_event(&root.path, &p, d)?;
        expected.insert(name.clone());
        expected.extend((1..=e.sequence).map(|n| format!("{}.e{n:04}.json", purge_stem(&p))));
        if e.phase == PurgePhase::Cancelled {
            let selected = &from.documents[&archive.migration.from_document_id];
            if exists(&purge_completion_path(selected, &p), d)? {
                purge_completion(selected, &p, d)?;
            } else {
                check_purge_marker(selected, &p, d)?;
            }
        } else {
            ensure!(
                active.is_none(),
                "Multiple noncancelled purges for selected archive"
            );
            active = Some((p, e));
        }
    }
    for name in names.iter().filter(|n| n.starts_with(&prefix)) {
        ensure!(
            expected.contains(name),
            "Unknown or orphan selected purge record"
        );
    }
    Ok(active)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryArchivePurgeReview {
    pub plan_id: String,
    pub archive_plan_id: String,
    pub event_id: String,
    pub phase: String,
    pub frame_present: bool,
    pub frame_verified: bool,
    pub evidence_unavailable: bool,
    pub marker_present: bool,
    pub complete: bool,
    pub confirmed_removed_content_bytes: usize,
    pub unverified_removal_bytes: usize,
    pub measured_filesystem_space_bytes: Option<u64>,
    pub secure_erasure_claimed: bool,
    pub file_changed: bool,
    pub source_written: bool,
}
fn purge_review(
    from: &Access,
    to: &Access,
    access: &HistoryArchiveAccess,
    p: &HistoryArchivePurgePlan,
    e: &PurgeEvent,
    d: &Deadline,
) -> Result<HistoryArchivePurgeReview> {
    let root = access.root(from, to, d)?;
    verify_retained(from, to, access, p, d)?;
    let present = held_frame(from, &root.path, p, false, d)?.is_some();
    ensure!(
        present
            || matches!(
                e.phase,
                PurgePhase::Deleting | PurgePhase::Removed | PurgePhase::Committed
            ),
        "Archive frame absent before deletion intent"
    );
    ensure!(
        !present || !matches!(e.phase, PurgePhase::Removed | PurgePhase::Committed),
        "Purged frame path has a replacement occupant"
    );
    let selected = &from.documents[&p.archive.migration.from_document_id];
    let marked = pending(selected, d)?;
    if marked {
        check_purge_marker(selected, p, d)?;
    }
    let terminal = matches!(e.phase, PurgePhase::Committed | PurgePhase::Cancelled);
    if terminal && !marked {
        purge_completion(selected, p, d)?;
    }
    let unverified =
        e.removal == Removal::Unverified || (e.phase == PurgePhase::Deleting && !present);
    Ok(HistoryArchivePurgeReview {
        plan_id: p.id.clone(),
        archive_plan_id: p.archive.id.clone(),
        event_id: e.id.clone(),
        phase: serde_json::to_value(e.phase)?
            .as_str()
            .context("Purge phase")?
            .into(),
        frame_present: present,
        frame_verified: present,
        evidence_unavailable: !present,
        marker_present: marked,
        complete: terminal && !marked,
        confirmed_removed_content_bytes: if e.removal == Removal::Confirmed {
            p.frame.byte_length
        } else {
            0
        },
        unverified_removal_bytes: if unverified { p.frame.byte_length } else { 0 },
        measured_filesystem_space_bytes: None,
        secure_erasure_claimed: false,
        file_changed: false,
        source_written: false,
    })
}
pub fn inspect_history_archive_purge(
    from: &Access,
    to: &Access,
    access: &HistoryArchivePurgeAccess,
    p: &HistoryArchivePurgePlan,
    d: &Deadline,
) -> Result<HistoryArchivePurgeReview> {
    access.check(from, to, p, d)?;
    let _old = from.hold_bindings(&p.archive.migration.from_document_id, d)?;
    let _new = to.hold_bindings(&p.archive.migration.to_document_id, d)?;
    let root = access.archive.root(from, to, d)?;
    let e = load_purge_event(&root.path, p, d)?;
    require_recorded_purge(from, to, &access.archive, p, &e, d)?;
    purge_review(from, to, &access.archive, p, &e, d)
}

fn reserve_purge(
    root: &Path,
    selected: &BoundDocument,
    p: &HistoryArchivePurgePlan,
    e: &PurgeEvent,
    new: bool,
    d: &Deadline,
) -> Result<()> {
    let directory = io::open_metadata_directory(root, d)?;
    let names = directory.names(MAX_ARCHIVE_ENTRIES, d)?;
    let remaining = (PURGE_EVENTS - e.sequence) as usize;
    // Header and one temporary coexist at publication; recovery needs its own
    // remaining events plus a temporary even if the frame has disappeared.
    let entries = remaining + usize::from(new) + 1;
    ensure!(
        names
            .len()
            .checked_add(entries)
            .is_some_and(|n| n <= MAX_ARCHIVE_ENTRIES),
        "Purge journal entry capacity exceeded"
    );
    let mut total = ((remaining + 1) * MAX_EVENT + if new { MAX_PLAN } else { 0 }) as u64;
    for name in names {
        d.check()?;
        if let Some(info) = directory.entry_info(&name, d)? {
            total = total
                .checked_add(info.length)
                .context("Purge archive byte overflow")?;
        }
    }
    ensure!(
        total <= MAX_ARCHIVE_ROOT_BYTES,
        "Purge journal byte capacity exceeded"
    );
    for sequence in e.sequence + 1..=PURGE_EVENTS {
        ensure!(
            !exists(&event_path(root, &purge_stem(p), sequence), d)?,
            "Occupied purge event output preserved"
        );
    }
    if new {
        ensure!(
            !exists(&plan_path(root, &purge_stem(p)), d)?,
            "Occupied purge header preserved"
        );
    }
    let completion = purge_completion_path(selected, p);
    ensure!(
        !exists(&completion, d)?,
        "Occupied purge completion preserved"
    );
    Ok(())
}
fn resume_purge(
    from: &Access,
    to: &Access,
    access: &HistoryArchivePurgeAccess,
    p: &HistoryArchivePurgePlan,
    e: &mut PurgeEvent,
    root: &Path,
    d: &Deadline,
) -> Result<HistoryArchivePurgeReview> {
    let selected = &from.documents[&p.archive.migration.from_document_id];
    if matches!(e.phase, PurgePhase::Committed | PurgePhase::Cancelled) {
        finish_purge_marker(selected, p, d)?;
        return purge_review(from, to, &access.archive, p, e, d);
    }
    reserve_purge(root, selected, p, e, false, d)?;
    require_recorded_purge(from, to, &access.archive, p, e, d)?;
    no_pending(from, to, &p.archive, Some(p), d)?;
    check_purge_marker(selected, p, d)?;
    let _current = current_purge(from, to, &access.archive, p, d)?;
    if e.phase == PurgePhase::Prepared {
        let _frame = held_frame(from, root, p, true, d)?
            .context("Archive frame absent before purge intent")?;
        next_purge(root, p, e, PurgePhase::Deleting, Removal::None, d)?;
        checkpoint("history_purge_intent")?;
    }
    if e.phase == PurgePhase::Deleting {
        let frame = held_frame(from, root, p, true, d)?;
        let removal = if let Some(frame) = frame {
            // Exact owned deletion is the only content mutation in this module.
            checkpoint("history_purge_before_unlink")?;
            frame.delete_owned(d)?;
            checkpoint("history_purge_unlinked")?;
            Removal::Confirmed
        } else {
            Removal::Unverified
        };
        ensure!(
            !exists(&archive_path(root, &p.archive), d)?,
            "Frame name remains occupied after unlink; preserve uncertainty"
        );
        next_purge(root, p, e, PurgePhase::Removed, removal, d)?;
        checkpoint("history_purge_progress")?;
    }
    if e.phase == PurgePhase::Removed {
        ensure!(
            !exists(&archive_path(root, &p.archive), d)?,
            "Replacement frame preserved after purge progress"
        );
        let removal = e.removal;
        next_purge(root, p, e, PurgePhase::Committed, removal, d)?;
        checkpoint("history_purge_committed")?;
    }
    finish_purge_marker(selected, p, d)?;
    checkpoint("history_purge_completed")?;
    purge_review(from, to, &access.archive, p, e, d)
}
#[allow(clippy::too_many_arguments)]
pub fn apply_history_archive_purge(
    from: &Access,
    to: &Access,
    access: &HistoryArchivePurgeAccess,
    approval: &HistoryArchivePurgeApproval,
    _permission: &HistoryArchivePurgePermission,
    p: &HistoryArchivePurgePlan,
    maximum_output_bytes: usize,
    d: &Deadline,
) -> Result<HistoryArchivePurgeReview> {
    access.check(from, to, p, d)?;
    ensure!(
        maximum_output_bytes >= PURGE_REPLY_RESERVE,
        "Purge result output budget refused before mutation"
    );
    ensure!(
        approval.plan_id == p.id && approval.archive_identity == access.archive.root_identity,
        "Concrete reviewed purge approval required"
    );
    let _old = from.hold_bindings(&p.archive.migration.from_document_id, d)?;
    let _new = to.hold_bindings(&p.archive.migration.to_document_id, d)?;
    let _locks = locks(from, to, d)?;
    let root = access.archive.root(from, to, d)?;
    let _lock = io::open_state_lock(&root.path.join(".yoke-history.lock"), d)?;
    no_pending(from, to, &p.archive, None, d)?;
    ensure!(
        evidence(from, to, &access.archive, &p.archive, d)?.is_none(),
        "Existing purge requires explicit event-reviewed recovery"
    );
    let _current = current_purge(from, to, &access.archive, p, d)?;
    // Acquire DELETE before publishing any intent. A reader therefore refuses
    // cleanly without creating a blocker or a misleading removal claim.
    let frame =
        held_frame(from, &root.path, p, true, d)?.context("Selected archive frame unavailable")?;
    let mut e = purge_initial(p);
    let selected = &from.documents[&p.archive.migration.from_document_id];
    reserve_purge(&root.path, selected, p, &e, true, d)?;
    let header = PurgeHeader {
        schema_version: 1,
        plan: p.clone(),
        initial: e.clone(),
    };
    write_record(
        &root.path,
        &plan_path(&root.path, &purge_stem(p)),
        &serde_json::to_vec(&header)?,
        d,
    )?;
    checkpoint("history_purge_prepared")?;
    write_record(
        &selected.parent_path,
        &purge_marker_path(selected)?,
        &serde_json::to_vec(&purge_marker(p))?,
        d,
    )?;
    checkpoint("history_purge_marked")?;
    // Release our DELETE guard before the resumed path acquires its own. The
    // locks and fresh owned admission still precede unlink; read sharing stays strict.
    drop(frame);
    resume_purge(from, to, access, p, &mut e, &root.path, d)
}
#[allow(clippy::too_many_arguments)]
pub fn recover_history_archive_purge(
    from: &Access,
    to: &Access,
    access: &HistoryArchivePurgeAccess,
    _permission: &HistoryArchivePurgePermission,
    p: &HistoryArchivePurgePlan,
    expected_event: &str,
    direction: RecoveryDirection,
    maximum_output_bytes: usize,
    d: &Deadline,
) -> Result<HistoryArchivePurgeReview> {
    access.check(from, to, p, d)?;
    ensure!(
        maximum_output_bytes >= PURGE_REPLY_RESERVE,
        "Purge result output budget refused before recovery mutation"
    );
    ensure!(
        valid_id(expected_event),
        "Explicit current purge event review required"
    );
    let _old = from.hold_bindings(&p.archive.migration.from_document_id, d)?;
    let _new = to.hold_bindings(&p.archive.migration.to_document_id, d)?;
    let _locks = locks(from, to, d)?;
    let root = access.archive.root(from, to, d)?;
    let _lock = io::open_state_lock(&root.path.join(".yoke-history.lock"), d)?;
    let mut e = load_purge_event(&root.path, p, d)?;
    ensure!(e.id == expected_event, "Stale history purge event review");
    require_recorded_purge(from, to, &access.archive, p, &e, d)?;
    ensure!(
        !matches!(
            (e.phase, direction),
            (
                PurgePhase::Deleting | PurgePhase::Removed | PurgePhase::Committed,
                RecoveryDirection::Rollback
            ) | (PurgePhase::Cancelled, RecoveryDirection::Resume)
        ),
        "Purge direction is already fixed; deletion intent permits resume only"
    );
    if !matches!(e.phase, PurgePhase::Committed | PurgePhase::Cancelled) {
        no_pending(from, to, &p.archive, Some(p), d)?;
        let _current = current_purge(from, to, &access.archive, p, d)?;
        let selected = &from.documents[&p.archive.migration.from_document_id];
        reserve_purge(&root.path, selected, p, &e, false, d)?;
        if !pending(selected, d)? {
            ensure!(
                e.phase == PurgePhase::Prepared,
                "Purge blocker absent after deletion intent"
            );
            let _frame = held_frame(from, &root.path, p, false, d)?
                .context("Archive frame absent before intent")?;
            write_record(
                &selected.parent_path,
                &purge_marker_path(selected)?,
                &serde_json::to_vec(&purge_marker(p))?,
                d,
            )?;
        }
        if direction == RecoveryDirection::Rollback {
            let _frame = held_frame(from, &root.path, p, false, d)?
                .context("Archive frame absent before cancellation")?;
            next_purge(
                &root.path,
                p,
                &mut e,
                PurgePhase::Cancelled,
                Removal::None,
                d,
            )?;
        }
    }
    resume_purge(from, to, access, p, &mut e, &root.path, d)
}

/// Read-only retained purge proof for archive inspection and explicit read refusal.
pub(super) fn archived_frame_status(
    from: &Access,
    to: &Access,
    access: &HistoryArchiveAccess,
    archive: &HistoryArchivePlan,
    d: &Deadline,
) -> Result<Option<HistoryArchivePurgeReview>> {
    let Some((p, e)) = evidence(from, to, access, archive, d)? else {
        return Ok(None);
    };
    purge_review(from, to, access, &p, &e, d).map(Some)
}
fn require_recorded_purge(
    from: &Access,
    to: &Access,
    access: &HistoryArchiveAccess,
    p: &HistoryArchivePurgePlan,
    e: &PurgeEvent,
    d: &Deadline,
) -> Result<()> {
    if e.phase == PurgePhase::Cancelled {
        return Ok(());
    }
    let (selected, event) = evidence(from, to, access, &p.archive, d)?
        .context("Selected purge operation evidence absent")?;
    ensure!(
        selected.id == p.id && event.id == e.id,
        "Selected purge evidence changed or has conflicting dependencies"
    );
    Ok(())
}
fn verify_retained(
    from: &Access,
    to: &Access,
    access: &HistoryArchiveAccess,
    p: &HistoryArchivePurgePlan,
    d: &Deadline,
) -> Result<()> {
    let (event, files) = retained_files(from, to, access, &p.archive, d)?;
    ensure!(
        original_records(from, &p.archive, true, d)? == 0,
        "Archived original record namespace is occupied"
    );
    let old_root = &from.journal.as_ref().context("Original journal absent")?.0;
    ensure!(
        event == p.archive_event_id
            && file_witnesses(&files, &access.root, old_root, d)? == p.retained,
        "Retained archive, migration or completion evidence changed since purge review"
    );
    Ok(())
}

#[cfg(all(test, windows))]
#[path = "host_documents_history_purge_tests.rs"]
mod tests;

#[cfg(test)]
mod protocol_tests {
    use super::*;
    #[test]
    fn purge_protocol_refuses_unknown_duplicate_and_noncanonical_json() {
        let event = PurgeEvent {
            schema_version: 1,
            id: "a".repeat(64),
            plan_id: "b".repeat(64),
            sequence: 0,
            previous: None,
            phase: PurgePhase::Prepared,
            removal: Removal::None,
        };
        let bytes = serde_json::to_vec(&event).unwrap();
        let d = Deadline::new(10_000);
        assert!(canonical_bytes::<PurgeEvent>(&bytes, MAX_EVENT, &d).is_ok());
        let mut spaced = bytes.clone();
        spaced.push(b' ');
        assert!(canonical_bytes::<PurgeEvent>(&spaced, MAX_EVENT, &d).is_err());
        let text = std::str::from_utf8(&bytes).unwrap();
        for prefix in ["{\"extra\":0,", "{\"schemaVersion\":1,"] {
            let altered = format!("{prefix}{}", &text[1..]);
            assert!(canonical_bytes::<PurgeEvent>(altered.as_bytes(), MAX_EVENT, &d).is_err());
        }
        assert!(canonical_bytes::<PurgeEvent>(&bytes, bytes.len() - 1, &d).is_err());
        assert!(canonical_bytes::<PurgeEvent>(&bytes, MAX_EVENT, &Deadline::new(0)).is_err());
    }
    #[test]
    fn purge_protocol_cannot_relabel_unverified_removal_or_cancel_after_intent() {
        let mut a = PurgeEvent {
            schema_version: 1,
            id: String::new(),
            plan_id: String::new(),
            sequence: 0,
            previous: None,
            phase: PurgePhase::Prepared,
            removal: Removal::None,
        };
        let mut b = a.clone();
        b.phase = PurgePhase::Committed;
        b.removal = Removal::Confirmed;
        assert!(!purge_step(&a, &b));
        a.phase = PurgePhase::Deleting;
        b.phase = PurgePhase::Cancelled;
        b.removal = Removal::None;
        assert!(!purge_step(&a, &b));
        b.phase = PurgePhase::Removed;
        b.removal = Removal::Unverified;
        assert!(purge_step(&a, &b));
        a = b.clone();
        b.phase = PurgePhase::Committed;
        b.removal = Removal::Confirmed;
        assert!(!purge_step(&a, &b));
        b.removal = Removal::Unverified;
        assert!(purge_step(&a, &b));
    }
}
