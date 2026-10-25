//! Explicit dual-authority migration of restrictive metadata, never file content.
use super::*;
#[path = "host_documents_history_archive.rs"]
mod history_archive;
pub use history_archive::{
    HistoryArchiveAccess, HistoryArchiveApproval, HistoryArchivePlan, HistoryArchivePurgeAccess,
    HistoryArchivePurgeApproval, HistoryArchivePurgePermission, HistoryArchivePurgePlan,
    HistoryArchivePurgeReview, HistoryArchiveRecord, HistoryArchiveReview, HistoryPurgeFileWitness,
    HistoryPurgeRetainedWitness, apply_history_archive, apply_history_archive_purge,
    inspect_history_archive, inspect_history_archive_purge, plan_history_archive,
    plan_history_archive_purge, read_history_archive_record, recover_history_archive,
    recover_history_archive_purge,
};

const MAX_MIGRATION: usize = 32_000;
const MAX_MIGRATION_EVENTS: u32 = 16;

pub(super) fn matches_selection(a: &OwnerBinding, b: &OwnerBinding) -> bool {
    a.target == b.target
        && a.access_binding == b.access_binding
        && a.journal_identity == b.journal_identity
}
fn marker_path(selected: &BoundDocument) -> Result<PathBuf> {
    Ok(selected.parent_path.join(format!(
        ".yoke-doc-migration-{}.pending.json",
        target_key(selected)?
    )))
}
pub(super) fn require_no_pending(selected: &BoundDocument, d: &Deadline) -> Result<()> {
    ensure!(
        !exists(&marker_path(selected)?, d)?,
        "Host document binding migration is pending; use its explicit host recovery workflow"
    );
    history_archive::require_no_pending(selected, d)?;
    Ok(())
}
pub(super) fn history_pending(selected: &BoundDocument, d: &Deadline) -> Result<bool> {
    history_archive::pending(selected, d)
}
pub(super) fn pending_metadata_present(selected: &BoundDocument, d: &Deadline) -> Result<bool> {
    exists(&marker_path(selected)?, d)
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BindingMigrationPlan {
    pub schema_version: u32,
    pub id: String,
    pub from_document_id: String,
    pub to_document_id: String,
    pub from_access: String,
    pub to_access: String,
    pub target_path: String,
    pub target: String,
    pub source_identity: String,
    pub source_hash: String,
    pub source_security_hash: String,
    pub source_revision: String,
    pub destination_revision: String,
    pub owner_identity: String,
    pub owner_security_hash: String,
    before_owner: OwnerBinding,
    after_owner: OwnerBinding,
    pub from_journal_state: String,
    pub to_journal_state: String,
    pub nonce: String,
}
fn plan_id(plan: &BindingMigrationPlan) -> String {
    let mut copy = plan.clone();
    copy.id.clear();
    identity(&("host_document_binding_migration_v1", copy))
}
fn selected_pair<'a>(
    from: &'a Access,
    to: &'a Access,
    old: &str,
    new: &str,
) -> Result<(&'a BoundDocument, &'a BoundDocument)> {
    let a = from
        .documents
        .get(old)
        .context("Original host document selection required")?;
    let b = to
        .documents
        .get(new)
        .context("Destination host document selection required")?;
    ensure!(
        from.binding != to.binding
            && a.spec.writable
            && b.spec.writable
            && matches!(a.spec.scope, Scope::Global {})
            && matches!(b.spec.scope, Scope::Global {})
            && a.source_project.is_none()
            && b.source_project.is_none()
            && target_key(a)? == target_key(b)?,
        "Migration requires two distinct writable selections of the same global target"
    );
    ensure!(
        from.journal.is_some() && to.journal.is_some(),
        "Both explicit host journal roots are required"
    );
    Ok((a, b))
}
fn journal_state(access: &Access, document: &str, root: &Path, d: &Deadline) -> Result<String> {
    let directory = io::open_metadata_directory(root, d)?;
    let prefix = format!("yoke-doc-{}-", access.binding);
    let mut rows = Vec::new();
    for name in directory.names(MAX_ENTRIES, d)? {
        if !name.starts_with(&prefix) || !name.ends_with(".plan.json") {
            continue;
        }
        let id = &name[prefix.len()..name.len() - ".plan.json".len()];
        let loaded = load(access, root, id, d)?;
        if loaded.plan.document_id == document {
            ensure!(
                matches!(loaded.event.phase, Phase::Committed | Phase::RolledBack),
                "Unfinished host document journal prevents binding migration"
            );
            let selected = &access.documents[document];
            ensure!(
                !exists(&backup_path(selected, id), d)?,
                "Terminal document journal has an occupied original-backup name"
            );
            for attempt in 0..=loaded.event.attempt {
                ensure!(
                    !exists(&output_path(selected, id, attempt), d)?,
                    "Terminal document journal has an occupied prepared-output name"
                );
            }
            rows.push((id.to_owned(), loaded.event.id));
        }
    }
    rows.sort();
    Ok(identity(&("host_document_terminal_journals_v1", rows)))
}
fn locks(from: &Access, to: &Access, d: &Deadline) -> Result<Vec<StateGuard>> {
    let a = &from
        .journal
        .as_ref()
        .context("Original journal root required")?
        .1;
    let b = &to
        .journal
        .as_ref()
        .context("Destination journal root required")?
        .1;
    if a == b {
        return Ok(vec![state(from, true, d)?]);
    }
    if a < b {
        Ok(vec![state(from, true, d)?, state(to, true, d)?])
    } else {
        Ok(vec![state(to, true, d)?, state(from, true, d)?])
    }
}
fn roots(
    from: &Access,
    to: &Access,
    d: &Deadline,
) -> Result<(io::GuardedDirectory, io::GuardedDirectory)> {
    let a = state(from, false, d)?.root;
    let b = state(to, false, d)?.root;
    Ok((a, b))
}
fn check_plan(from: &Access, to: &Access, p: &BindingMigrationPlan, d: &Deadline) -> Result<()> {
    d.check()?;
    let (a, b) = selected_pair(from, to, &p.from_document_id, &p.to_document_id)?;
    ensure!(
        p.schema_version == 1
            && valid_id(&p.id)
            && p.id == plan_id(p)
            && p.from_access == from.binding
            && p.to_access == to.binding
            && p.target == target_key(a)?
            && p.before_owner.id == owner_id(&p.before_owner)
            && ((p.before_owner.schema_version == 1 && p.before_owner.generation.is_none())
                || (p.before_owner.schema_version == 2
                    && p.before_owner.generation.as_deref().is_some_and(valid_id)))
            && matches_selection(&p.before_owner, &expected_owner(from, a)?)
            && p.after_owner.schema_version == 2
            && p.after_owner.id == owner_id(&p.after_owner)
            && p.after_owner.generation.as_ref() == Some(&p.nonce)
            && matches_selection(&p.after_owner, &expected_owner(to, b)?)
            && [
                &p.source_hash,
                &p.source_security_hash,
                &p.source_revision,
                &p.destination_revision,
                &p.owner_security_hash,
                &p.from_journal_state,
                &p.to_journal_state,
                &p.nonce
            ]
            .into_iter()
            .all(|v| valid_id(v))
            && !p.owner_identity.is_empty()
            && p.owner_identity.len() <= 128
            && !p.source_identity.is_empty()
            && p.source_identity.len() <= 128
            && serde_json::to_vec(p)?.len() <= MAX_MIGRATION,
        "Invalid reviewed host document binding migration"
    );
    Ok(())
}
fn check_source(
    from: &Access,
    to: &Access,
    p: &BindingMigrationPlan,
    file: &io::HeldFile,
    d: &Deadline,
) -> Result<()> {
    let (a, b) = selected_pair(from, to, &p.from_document_id, &p.to_document_id)?;
    let security = io::security_fingerprint(file, d)?;
    std::str::from_utf8(&file.bytes).context("Selected global document is not UTF-8")?;
    ensure!(
        file.identity == p.source_identity
            && file.hash == p.source_hash
            && security == p.source_security_hash
            && file.path.to_str() == Some(p.target_path.as_str())
            && file.byte_length <= from.max_bytes.min(to.max_bytes)
            && from.revision(a, file, &security) == p.source_revision
            && to.revision(b, file, &security) == p.destination_revision,
        "Source or security changed since the binding migration review"
    );
    Ok(())
}
pub fn plan_binding_migration(
    from: &Access,
    to: &Access,
    old: &str,
    new: &str,
    d: &Deadline,
) -> Result<BindingMigrationPlan> {
    let (a, b) = selected_pair(from, to, old, new)?;
    let _old_guards = from.hold_bindings(old, d)?;
    let _new_guards = to.hold_bindings(new, d)?;
    require_no_pending(a, d)?;
    require_owner(from, a, d)?;
    let (_, source) = from.current(old, d)?;
    let owner =
        io::read_file(&owner_path(a)?, 2048, false, d)?.context("Original owner binding absent")?;
    let before: OwnerBinding = serde_json::from_slice(&owner.bytes)?;
    ensure!(
        serde_json::to_vec(&before)? == owner.bytes,
        "Noncanonical original owner metadata"
    );
    let generation = nonce()?;
    let mut after = expected_owner(to, b)?;
    after.schema_version = 2;
    after.generation = Some(generation.clone());
    after.id = owner_id(&after);
    let security = io::security_fingerprint(&source, d)?;
    let (ar, br) = roots(from, to, d)?;
    let mut plan = BindingMigrationPlan {
        schema_version: 1,
        id: String::new(),
        from_document_id: old.into(),
        to_document_id: new.into(),
        from_access: from.binding.clone(),
        to_access: to.binding.clone(),
        target_path: source
            .path
            .to_str()
            .context("Unicode target required")?
            .into(),
        target: target_key(a)?,
        source_identity: source.identity.clone(),
        source_hash: source.hash.clone(),
        source_security_hash: security.clone(),
        source_revision: from.revision(a, &source, &security),
        destination_revision: to.revision(b, &source, &security),
        owner_identity: owner.identity.clone(),
        owner_security_hash: io::security_fingerprint(&owner, d)?,
        before_owner: before,
        after_owner: after,
        from_journal_state: journal_state(from, old, &ar.path, d)?,
        to_journal_state: journal_state(to, new, &br.path, d)?,
        nonce: generation,
    };
    plan.id = plan_id(&plan);
    check_plan(from, to, &plan, d)?;
    check_source(from, to, &plan, &source, d)?;
    Ok(plan)
}

pub struct BindingMigrationAuthority {
    from: String,
    to: String,
    old: String,
    new: String,
}
pub struct BindingMigrationApproval {
    plan_id: String,
    from: String,
    to: String,
}
impl BindingMigrationAuthority {
    pub fn new(
        from: &Access,
        to: &Access,
        old: &str,
        new: &str,
        a: &WriteAuthority,
        b: &WriteAuthority,
    ) -> Result<Self> {
        selected_pair(from, to, old, new)?;
        ensure!(
            a.binding == from.binding
                && b.binding == to.binding
                && a.documents.contains(old)
                && b.documents.contains(new),
            "Both independent host document write authorities are required"
        );
        Ok(Self {
            from: from.binding.clone(),
            to: to.binding.clone(),
            old: old.into(),
            new: new.into(),
        })
    }
    fn check(
        &self,
        from: &Access,
        to: &Access,
        p: &BindingMigrationPlan,
        d: &Deadline,
    ) -> Result<()> {
        check_plan(from, to, p, d)?;
        ensure!(
            self.from == from.binding
                && self.to == to.binding
                && self.old == p.from_document_id
                && self.new == p.to_document_id,
            "Migration host authority changed"
        );
        Ok(())
    }
    pub fn approve(
        &self,
        from: &Access,
        to: &Access,
        p: &BindingMigrationPlan,
        d: &Deadline,
    ) -> Result<BindingMigrationApproval> {
        self.check(from, to, p, d)?;
        Ok(BindingMigrationApproval {
            plan_id: p.id.clone(),
            from: self.from.clone(),
            to: self.to.clone(),
        })
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum MigrationPhase {
    Prepared,
    Writing,
    Committed,
    RolledBack,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct MigrationEvent {
    schema_version: u32,
    id: String,
    plan_id: String,
    sequence: u32,
    previous: Option<String>,
    phase: MigrationPhase,
    direction: Option<RecoveryDirection>,
}
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct MigrationHeader {
    schema_version: u32,
    plan: BindingMigrationPlan,
    initial: MigrationEvent,
}
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Marker {
    schema_version: u32,
    plan_id: String,
    target: String,
    owner_identity: String,
}
fn marker(p: &BindingMigrationPlan) -> Marker {
    Marker {
        schema_version: 1,
        plan_id: p.id.clone(),
        target: p.target.clone(),
        owner_identity: p.owner_identity.clone(),
    }
}
fn migration_stem(p: &BindingMigrationPlan) -> String {
    format!("yoke-binding-migration-{}", p.id)
}
fn event_id(e: &MigrationEvent) -> String {
    let mut c = e.clone();
    c.id.clear();
    identity(&("host_document_binding_migration_event_v1", c))
}
fn initial(p: &BindingMigrationPlan) -> MigrationEvent {
    let mut e = MigrationEvent {
        schema_version: 1,
        id: String::new(),
        plan_id: p.id.clone(),
        sequence: 0,
        previous: None,
        phase: MigrationPhase::Prepared,
        direction: None,
    };
    e.id = event_id(&e);
    e
}
fn valid_transition(before: &MigrationEvent, after: &MigrationEvent) -> bool {
    if before
        .direction
        .is_some_and(|fixed| after.direction != Some(fixed))
    {
        return false;
    }
    match (before.phase, after.phase) {
        (MigrationPhase::Prepared, MigrationPhase::Writing) => true,
        (MigrationPhase::Writing, MigrationPhase::Writing) => after.direction.is_some(),
        (MigrationPhase::Writing, MigrationPhase::Committed) => {
            after.direction != Some(RecoveryDirection::Rollback)
        }
        (MigrationPhase::Writing, MigrationPhase::RolledBack) => {
            after.direction == Some(RecoveryDirection::Rollback)
        }
        _ => false,
    }
}
fn next(
    root: &Path,
    p: &BindingMigrationPlan,
    e: &mut MigrationEvent,
    phase: MigrationPhase,
    direction: Option<RecoveryDirection>,
    d: &Deadline,
) -> Result<()> {
    ensure!(
        e.sequence + 1 < MAX_MIGRATION_EVENTS
            && e.direction.is_none_or(|fixed| direction == Some(fixed)),
        "Migration direction or event capacity refused"
    );
    let mut n = e.clone();
    n.sequence += 1;
    n.previous = Some(e.id.clone());
    n.phase = phase;
    n.direction = direction;
    ensure!(
        valid_transition(e, &n),
        "Invalid binding migration transition"
    );
    n.id = event_id(&n);
    write_record(
        root,
        &event_path(root, &migration_stem(p), n.sequence),
        &serde_json::to_vec(&n)?,
        d,
    )?;
    *e = n;
    Ok(())
}
fn load_event(
    from: &Access,
    to: &Access,
    p: &BindingMigrationPlan,
    root: &Path,
    d: &Deadline,
) -> Result<MigrationEvent> {
    check_plan(from, to, p, d)?;
    let stem = migration_stem(p);
    let directory = io::open_metadata_directory(root, d)?;
    let namespace = format!("{stem}.");
    for name in directory.names(MAX_ENTRIES, d)? {
        if !name.starts_with(&namespace) || name == format!("{stem}.plan.json") {
            continue;
        }
        let sequence = name
            .strip_prefix(&format!("{stem}.e"))
            .and_then(|name| name.strip_suffix(".json"))
            .and_then(|digits| {
                if digits.len() == 4 {
                    digits.parse::<u32>().ok()
                } else {
                    None
                }
            });
        ensure!(
            sequence.is_some_and(|n| (1..MAX_MIGRATION_EVENTS).contains(&n)
                && name == format!("{stem}.e{n:04}.json")),
            "Unsupported or out-of-capacity binding migration record"
        );
    }
    let header: MigrationHeader = canonical(&plan_path(root, &stem), MAX_MIGRATION, d)?;
    let mut e = initial(p);
    ensure!(
        header.schema_version == 1
            && serde_json::to_vec(&header.plan)? == serde_json::to_vec(p)?
            && serde_json::to_vec(&header.initial)? == serde_json::to_vec(&e)?,
        "Migration journal header differs"
    );
    let mut ended = false;
    for sequence in 1..MAX_MIGRATION_EVENTS {
        let path = event_path(root, &stem, sequence);
        if !exists(&path, d)? {
            ended = true;
            continue;
        }
        ensure!(!ended, "Migration event sequence has a gap");
        let n: MigrationEvent = canonical(&path, MAX_EVENT, d)?;
        let terminal = matches!(
            e.phase,
            MigrationPhase::Committed | MigrationPhase::RolledBack
        );
        ensure!(
            !terminal
                && n.schema_version == 1
                && n.plan_id == p.id
                && n.sequence == sequence
                && n.previous.as_ref() == Some(&e.id)
                && n.id == event_id(&n)
                && e.direction.is_none_or(|fixed| n.direction == Some(fixed))
                && valid_transition(&e, &n)
                && (n.phase != MigrationPhase::RolledBack
                    || n.direction == Some(RecoveryDirection::Rollback))
                && (n.phase != MigrationPhase::Committed
                    || n.direction != Some(RecoveryDirection::Rollback)),
            "Invalid migration event transition"
        );
        e = n;
    }
    Ok(e)
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BindingMigrationReview {
    pub plan_id: String,
    pub event_id: String,
    pub phase: String,
    pub direction: Option<RecoveryDirection>,
    pub marker_present: bool,
    pub file_changed: bool,
    pub source_written: bool,
}
pub fn inspect_binding_migration(
    from: &Access,
    to: &Access,
    p: &BindingMigrationPlan,
    d: &Deadline,
) -> Result<BindingMigrationReview> {
    let (a, _) = selected_pair(from, to, &p.from_document_id, &p.to_document_id)?;
    let root = state(from, false, d)?;
    let e = load_event(from, to, p, &root.root.path, d)?;
    let phase = serde_json::to_value(e.phase)?
        .as_str()
        .context("Migration phase")?
        .to_owned();
    Ok(BindingMigrationReview {
        plan_id: p.id.clone(),
        event_id: e.id,
        phase,
        direction: e.direction,
        marker_present: exists(&marker_path(a)?, d)?,
        file_changed: false,
        source_written: false,
    })
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BindingMigrationReceipt {
    pub plan_id: String,
    pub event_id: String,
    pub outcome: String,
    pub metadata_changed: bool,
    pub file_changed: bool,
    pub source_written: bool,
}
fn receipt(
    p: &BindingMigrationPlan,
    e: &MigrationEvent,
    changed: bool,
) -> Result<BindingMigrationReceipt> {
    Ok(BindingMigrationReceipt {
        plan_id: p.id.clone(),
        event_id: e.id.clone(),
        outcome: serde_json::to_value(e.phase)?
            .as_str()
            .context("Migration phase")?
            .into(),
        metadata_changed: changed,
        file_changed: false,
        source_written: false,
    })
}
fn archive_marker(a: &BoundDocument, p: &BindingMigrationPlan, d: &Deadline) -> Result<bool> {
    let path = marker_path(a)?;
    let archive = a
        .parent_path
        .join(format!(".yoke-doc-migration-{}.complete.json", p.id));
    let expected = serde_json::to_vec(&marker(p))?;
    if let Some(mut held) = io::read_file(&path, 2048, true, d)? {
        ensure!(held.bytes == expected, "Foreign migration marker preserved");
        held.rename_no_replace(&archive, d)?;
        return Ok(true);
    }
    let held = io::read_file(&archive, 2048, false, d)?
        .context("Migration marker and terminal history are absent")?;
    ensure!(
        held.bytes == expected,
        "Foreign migration history preserved"
    );
    Ok(false)
}
fn held_owner(
    from: &Access,
    to: &Access,
    p: &BindingMigrationPlan,
    d: &Deadline,
) -> Result<(io::HeldFile, io::DocumentOwnerMetadata)> {
    let (a, _) = selected_pair(from, to, &p.from_document_id, &p.to_document_id)?;
    let (_, source) = from.hold_current(&p.from_document_id, true, d)?;
    check_source(from, to, p, &source, d)?;
    let owner = io::open_document_owner_metadata(&owner_path(a)?, d)?;
    ensure!(
        owner.file.identity == p.owner_identity
            && io::security_fingerprint(&owner.file, d)? == p.owner_security_hash,
        "Original migration metadata identity or security changed"
    );
    Ok((source, owner))
}
pub fn apply_binding_migration(
    from: &Access,
    to: &Access,
    authority: &BindingMigrationAuthority,
    approval: &BindingMigrationApproval,
    p: &BindingMigrationPlan,
    d: &Deadline,
) -> Result<BindingMigrationReceipt> {
    authority.check(from, to, p, d)?;
    ensure!(
        approval.plan_id == p.id && approval.from == from.binding && approval.to == to.binding,
        "Concrete dual-host migration approval required"
    );
    let (a, _) = selected_pair(from, to, &p.from_document_id, &p.to_document_id)?;
    let _old = from.hold_bindings(&p.from_document_id, d)?;
    let _new = to.hold_bindings(&p.to_document_id, d)?;
    let _locks = locks(from, to, d)?;
    let (ar, br) = roots(from, to, d)?;
    require_no_pending(a, d)?;
    require_owner(from, a, d)?;
    let (_source, mut owner) = held_owner(from, to, p, d)?;
    ensure!(
        owner.file.bytes == serde_json::to_vec(&p.before_owner)?
            && journal_state(from, &p.from_document_id, &ar.path, d)? == p.from_journal_state
            && journal_state(to, &p.to_document_id, &br.path, d)? == p.to_journal_state,
        "Migration owner or terminal journal review changed"
    );
    budget(&ar.path, d)?;
    budget(&br.path, d)?;
    let mut e = initial(p);
    let header = MigrationHeader {
        schema_version: 1,
        plan: p.clone(),
        initial: e.clone(),
    };
    write_record(
        &ar.path,
        &plan_path(&ar.path, &migration_stem(p)),
        &serde_json::to_vec(&header)?,
        d,
    )?;
    write_record(
        &a.parent_path,
        &marker_path(a)?,
        &serde_json::to_vec(&marker(p))?,
        d,
    )?;
    checkpoint("binding_migration_marked")?;
    next(&ar.path, p, &mut e, MigrationPhase::Writing, None, d)?;
    owner.rewrite(&serde_json::to_vec(&p.after_owner)?, d)?;
    checkpoint("binding_migration_rewritten")?;
    next(&ar.path, p, &mut e, MigrationPhase::Committed, None, d)?;
    archive_marker(a, p, d)?;
    receipt(p, &e, true)
}
pub fn recover_binding_migration(
    from: &Access,
    to: &Access,
    authority: &BindingMigrationAuthority,
    p: &BindingMigrationPlan,
    expected_event: &str,
    direction: RecoveryDirection,
    d: &Deadline,
) -> Result<BindingMigrationReceipt> {
    authority.check(from, to, p, d)?;
    ensure!(valid_id(expected_event), "Migration event review required");
    let (a, _) = selected_pair(from, to, &p.from_document_id, &p.to_document_id)?;
    let _old = from.hold_bindings(&p.from_document_id, d)?;
    let _new = to.hold_bindings(&p.to_document_id, d)?;
    let _locks = locks(from, to, d)?;
    let (ar, br) = roots(from, to, d)?;
    let mut e = load_event(from, to, p, &ar.path, d)?;
    ensure!(
        e.id == expected_event && e.direction.is_none_or(|fixed| fixed == direction),
        "Stale migration review or fixed recovery direction"
    );
    let terminal = matches!(
        e.phase,
        MigrationPhase::Committed | MigrationPhase::RolledBack
    );
    if terminal {
        ensure!(
            (e.phase == MigrationPhase::Committed) == (direction == RecoveryDirection::Resume),
            "Terminal migration direction cannot reverse"
        );
    }
    let (_source, mut owner) = held_owner(from, to, p, d)?;
    ensure!(
        journal_state(from, &p.from_document_id, &ar.path, d)? == p.from_journal_state
            && journal_state(to, &p.to_document_id, &br.path, d)? == p.to_journal_state,
        "Migration terminal journal review changed"
    );
    let before = serde_json::to_vec(&p.before_owner)?;
    let after = serde_json::to_vec(&p.after_owner)?;
    let bytes = &owner.file.bytes;
    let partial = e.phase == MigrationPhase::Writing
        && (before.starts_with(bytes) || after.starts_with(bytes));
    ensure!(
        *bytes == before || *bytes == after || partial,
        "Foreign owner metadata preserved"
    );
    let desired = if direction == RecoveryDirection::Resume {
        &after
    } else {
        &before
    };
    if terminal {
        ensure!(
            owner.file.bytes == *desired,
            "Terminal owner metadata changed"
        );
        let changed = archive_marker(a, p, d)?;
        return receipt(p, &e, changed);
    }
    let fixes_direction = e.direction.is_none();
    let required_events = if fixes_direction { 2 } else { 1 };
    ensure!(
        e.sequence + required_events < MAX_MIGRATION_EVENTS,
        "Migration must reserve its terminal event before metadata mutation"
    );
    budget(&ar.path, d)?;
    if e.phase == MigrationPhase::Prepared && !exists(&marker_path(a)?, d)? {
        ensure!(
            owner.file.bytes == before,
            "Unmarked migration metadata changed"
        );
        write_record(
            &a.parent_path,
            &marker_path(a)?,
            &serde_json::to_vec(&marker(p))?,
            d,
        )?;
    }
    let marker_file = io::read_file(&marker_path(a)?, 2048, false, d)?
        .context("Pending migration marker absent; metadata is preserved")?;
    ensure!(
        marker_file.bytes == serde_json::to_vec(&marker(p))?,
        "Foreign pending marker preserved"
    );
    drop(marker_file);
    if fixes_direction {
        next(
            &ar.path,
            p,
            &mut e,
            MigrationPhase::Writing,
            Some(direction),
            d,
        )?;
    }
    owner.rewrite(desired, d)?;
    next(
        &ar.path,
        p,
        &mut e,
        if direction == RecoveryDirection::Resume {
            MigrationPhase::Committed
        } else {
            MigrationPhase::RolledBack
        },
        Some(direction),
        d,
    )?;
    archive_marker(a, p, d)?;
    receipt(p, &e, true)
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    use std::fs;
    const ORIGINAL: &[u8] = "\u{feff}# owned e\u{301}\r\nvalue: 🦀\r\n".as_bytes();
    fn d() -> Deadline {
        Deadline::new(30_000)
    }
    struct Fixture {
        _root: tempfile::TempDir,
        target: PathBuf,
        from: Access,
        to: Access,
        a: WriteAuthority,
        b: WriteAuthority,
    }
    impl Fixture {
        fn new() -> Self {
            let root = tempfile::tempdir().unwrap();
            let source = root.path().join("source");
            let target = root.path().join("settings-π.yaml");
            let old = root.path().join("old-journals");
            let new = root.path().join("new-journals");
            for path in [&source, &old, &new] {
                fs::create_dir(path).unwrap();
            }
            fs::write(source.join("untouched.rs"), b"fn untouched() {}\r\n").unwrap();
            fs::write(&target, ORIGINAL).unwrap();
            let roots = [ProjectRoot {
                name: "default".into(),
                path: source,
            }];
            let config = |id: &str, journal: PathBuf| Config {
                documents: vec![DocumentSpec {
                    id: id.into(),
                    label: format!("Owned {id}"),
                    path: target.clone(),
                    scope: Scope::Global {},
                    writable: true,
                }],
                journal_root: Some(journal),
                max_document_bytes: 64000,
            };
            let from = Access::open(&config("old", old), &roots, &d()).unwrap();
            let to = Access::open(&config("new", new), &roots, &d()).unwrap();
            let a = WriteAuthority::new(&from).unwrap();
            let b = WriteAuthority::new(&to).unwrap();
            bind_owner(&from, &from.documents["old"], &d()).unwrap();
            Self {
                _root: root,
                target,
                from,
                to,
                a,
                b,
            }
        }
        fn plan(&self) -> BindingMigrationPlan {
            plan_binding_migration(&self.from, &self.to, "old", "new", &d()).unwrap()
        }
        fn authority(&self) -> BindingMigrationAuthority {
            BindingMigrationAuthority::new(&self.from, &self.to, "old", "new", &self.a, &self.b)
                .unwrap()
        }
        fn apply(&self, p: &BindingMigrationPlan) -> Result<BindingMigrationReceipt> {
            let a = self.authority();
            let approved = a.approve(&self.from, &self.to, p, &d())?;
            apply_binding_migration(&self.from, &self.to, &a, &approved, p, &d())
        }
        fn owner_path(&self) -> PathBuf {
            owner_path(&self.from.documents["old"]).unwrap()
        }
        fn recover(
            &self,
            p: &BindingMigrationPlan,
            event: &str,
            direction: RecoveryDirection,
        ) -> Result<BindingMigrationReceipt> {
            recover_binding_migration(
                &self.from,
                &self.to,
                &self.authority(),
                p,
                event,
                direction,
                &d(),
            )
        }
    }
    #[test]
    fn plan_is_inert_and_default_owner_bytes_remain_legacy_canonical() {
        let f = Fixture::new();
        let before = fs::read(f.owner_path()).unwrap();
        let p = f.plan();
        assert_eq!(fs::read(&f.target).unwrap(), ORIGINAL);
        assert_eq!(fs::read(f.owner_path()).unwrap(), before);
        let value: serde_json::Value = serde_json::from_slice(&before).unwrap();
        assert!(value.get("generation").is_none());
        assert_eq!(value["schemaVersion"], 1);
        assert!(!marker_path(&f.from.documents["old"]).unwrap().exists());
        assert_eq!(p.before_owner.schema_version, 1);
        assert_eq!(p.after_owner.schema_version, 2);
        assert_eq!(p.after_owner.generation.as_ref(), Some(&p.nonce));
    }
    #[test]
    fn migration_changes_only_binding_then_new_host_can_write_and_old_host_cannot() {
        let f = Fixture::new();
        let p = f.plan();
        let r = f.apply(&p).unwrap();
        assert!(!r.file_changed && !r.source_written && r.metadata_changed);
        assert_eq!(r.outcome, "committed");
        assert_eq!(fs::read(&f.target).unwrap(), ORIGINAL);
        require_owner(&f.to, &f.to.documents["new"], &d()).unwrap();
        assert!(require_owner(&f.from, &f.from.documents["old"], &d()).is_err());
        let old = f.from.read("old", &d()).unwrap();
        let old_plan = f.from.plan("old", &old.revision, "refused", &d()).unwrap();
        let old_approval = f.a.approve(&f.from, &old_plan, &d()).unwrap();
        assert!(f.from.apply(&f.a, &old_approval, &old_plan, &d()).is_err());
        assert_eq!(fs::read(&f.target).unwrap(), ORIGINAL);
        let new = f.to.read("new", &d()).unwrap();
        let new_plan =
            f.to.plan("new", &new.revision, "explicit new host bytes\r\n", &d())
                .unwrap();
        let approved = f.b.approve(&f.to, &new_plan, &d()).unwrap();
        f.to.apply(&f.b, &approved, &new_plan, &d()).unwrap();
        assert_eq!(fs::read(&f.target).unwrap(), b"explicit new host bytes\r\n");
    }
    #[test]
    fn marked_and_rewritten_interruptions_block_normal_actions_until_explicit_recovery() {
        for checkpoint_name in ["binding_migration_marked", "binding_migration_rewritten"] {
            for direction in [RecoveryDirection::Resume, RecoveryDirection::Rollback] {
                let f = Fixture::new();
                let p = f.plan();
                super::super::STOP_AFTER.with(|slot| *slot.borrow_mut() = Some(checkpoint_name));
                assert!(f.apply(&p).is_err());
                assert!(f.from.owner_binding("old", &d()).is_err());
                assert!(f.to.owner_binding("new", &d()).is_err());
                assert!(f.from.pending_journals("old", &d()).is_err());
                let review = inspect_binding_migration(&f.from, &f.to, &p, &d()).unwrap();
                assert!(review.marker_present);
                assert_eq!(fs::read(&f.target).unwrap(), ORIGINAL);
                let r = f.recover(&p, &review.event_id, direction).unwrap();
                assert!(r.metadata_changed);
                assert_eq!(fs::read(&f.target).unwrap(), ORIGINAL);
                let expected = if direction == RecoveryDirection::Resume {
                    &p.after_owner
                } else {
                    &p.before_owner
                };
                assert_eq!(
                    fs::read(f.owner_path()).unwrap(),
                    serde_json::to_vec(expected).unwrap()
                );
                let done = inspect_binding_migration(&f.from, &f.to, &p, &d()).unwrap();
                assert!(!done.marker_present);
                assert!(
                    !f.recover(&p, &done.event_id, direction)
                        .unwrap()
                        .metadata_changed
                );
            }
        }
    }
    #[test]
    fn a_document_proposal_cannot_revive_after_returning_to_its_old_selection() {
        let f = Fixture::new();
        let loaded = f.from.read("old", &d()).unwrap();
        let stale = f
            .from
            .plan("old", &loaded.revision, "stale reviewed proposal\r\n", &d())
            .unwrap();
        let old_approval = f.a.approve(&f.from, &stale, &d()).unwrap();
        f.apply(&f.plan()).unwrap();
        let back = plan_binding_migration(&f.to, &f.from, "new", "old", &d()).unwrap();
        let authority =
            BindingMigrationAuthority::new(&f.to, &f.from, "new", "old", &f.b, &f.a).unwrap();
        let approval = authority.approve(&f.to, &f.from, &back, &d()).unwrap();
        apply_binding_migration(&f.to, &f.from, &authority, &approval, &back, &d()).unwrap();
        assert_eq!(fs::read(&f.target).unwrap(), ORIGINAL);
        let result = f.from.apply(&f.a, &old_approval, &stale, &d());
        assert!(
            result.is_err(),
            "A stale document proposal revived after binding migration and wrote {:?}",
            fs::read(&f.target).unwrap()
        );
        assert!(f.a.approve(&f.from, &stale, &d()).is_err());
        assert_eq!(fs::read(&f.target).unwrap(), ORIGINAL);
        let loaded = f.from.read("old", &d()).unwrap();
        let fresh = f
            .from
            .plan("old", &loaded.revision, "fresh reviewed proposal\r\n", &d())
            .unwrap();
        let approved = f.a.approve(&f.from, &fresh, &d()).unwrap();
        f.from.apply(&f.a, &approved, &fresh, &d()).unwrap();
        assert_eq!(fs::read(&f.target).unwrap(), b"fresh reviewed proposal\r\n");
    }
    #[test]
    fn a_generation_bound_document_proposal_cannot_revive_on_a_later_return() {
        let f = Fixture::new();
        f.apply(&f.plan()).unwrap();
        let loaded = f.to.read("new", &d()).unwrap();
        let stale =
            f.to.plan("new", &loaded.revision, "stale second epoch", &d())
                .unwrap();
        assert!(stale.owner_generation.is_some());
        let approved = f.b.approve(&f.to, &stale, &d()).unwrap();
        let back = plan_binding_migration(&f.to, &f.from, "new", "old", &d()).unwrap();
        let authority =
            BindingMigrationAuthority::new(&f.to, &f.from, "new", "old", &f.b, &f.a).unwrap();
        let approval = authority.approve(&f.to, &f.from, &back, &d()).unwrap();
        apply_binding_migration(&f.to, &f.from, &authority, &approval, &back, &d()).unwrap();
        f.apply(&f.plan()).unwrap();
        assert!(f.to.apply(&f.b, &approved, &stale, &d()).is_err());
        assert!(f.b.approve(&f.to, &stale, &d()).is_err());
        assert_eq!(fs::read(&f.target).unwrap(), ORIGINAL);
        let loaded = f.to.read("new", &d()).unwrap();
        let fresh =
            f.to.plan("new", &loaded.revision, "new epoch approval", &d())
                .unwrap();
        assert_ne!(fresh.owner_generation, stale.owner_generation);
        let approved = f.b.approve(&f.to, &fresh, &d()).unwrap();
        f.to.apply(&f.b, &approved, &fresh, &d()).unwrap();
        assert_eq!(fs::read(&f.target).unwrap(), b"new epoch approval");
    }
    #[test]
    fn legacy_generationless_journals_keep_explicit_recovery_but_not_proposal_approval() {
        let f = Fixture::new();
        f.apply(&f.plan()).unwrap();
        let loaded = f.to.read("new", &d()).unwrap();
        let proposed =
            f.to.plan(
                "new",
                &loaded.revision,
                "legacy explicit recovery\r\n",
                &d(),
            )
            .unwrap();
        assert!(proposed.owner_generation.is_some());
        let mut encoded = serde_json::to_value(&proposed).unwrap();
        encoded.as_object_mut().unwrap().remove("ownerGeneration");
        let mut legacy: Plan = serde_json::from_value(encoded).unwrap();
        legacy.id.clear();
        legacy.id = identity(&("dashboard_host_document_plan_v1", &legacy));
        assert!(legacy.owner_generation.is_none());
        assert!(
            serde_json::to_value(&legacy)
                .unwrap()
                .get("ownerGeneration")
                .is_none()
        );
        let genesis = super::super::initial(&f.to, &legacy);
        let header = Header {
            schema_version: 1,
            plan: legacy.clone(),
            initial: genesis.clone(),
        };
        let root = state(&f.to, true, &d()).unwrap();
        let name = stem(&f.to, &legacy.id).unwrap();
        write_record(
            &root.root.path,
            &plan_path(&root.root.path, &name),
            &serde_json::to_vec(&header).unwrap(),
            &d(),
        )
        .unwrap();
        drop(root);
        assert!(f.b.approve(&f.to, &legacy, &d()).is_err());
        let approval =
            f.b.approve_recovery_at(
                &f.to,
                &legacy.id,
                &genesis.id,
                RecoveryDirection::Resume,
                &d(),
            )
            .unwrap();
        let receipt = f.to.recover(&f.b, &approval, &d()).unwrap();
        assert_eq!(receipt.outcome, "committed");
        assert_eq!(
            fs::read(&f.target).unwrap(),
            b"legacy explicit recovery\r\n"
        );
        let journal = f.to.inspect_journal(&legacy.id, &d()).unwrap();
        assert!(!journal.pending && journal.plan.owner_generation.is_none());
    }
    #[test]
    fn generation_bound_recovery_refuses_changed_owner_metadata_and_preserves_the_journal() {
        let f = Fixture::new();
        f.apply(&f.plan()).unwrap();
        let loaded = f.to.read("new", &d()).unwrap();
        let plan =
            f.to.plan(
                "new",
                &loaded.revision,
                "must not recover a stale generation",
                &d(),
            )
            .unwrap();
        assert!(plan.owner_generation.is_some());
        let genesis = super::super::initial(&f.to, &plan);
        let header = Header {
            schema_version: 1,
            plan: plan.clone(),
            initial: genesis.clone(),
        };
        let root = state(&f.to, true, &d()).unwrap();
        let name = stem(&f.to, &plan.id).unwrap();
        let path = plan_path(&root.root.path, &name);
        let bytes = serde_json::to_vec(&header).unwrap();
        write_record(&root.root.path, &path, &bytes, &d()).unwrap();
        drop(root);
        let approval =
            f.b.approve_recovery_at(
                &f.to,
                &plan.id,
                &genesis.id,
                RecoveryDirection::Resume,
                &d(),
            )
            .unwrap();
        let mut foreign = read_owner(&f.to.documents["new"], &d()).unwrap().unwrap();
        foreign.generation = Some(hash(b"owned foreign generation fixture"));
        foreign.id = owner_id(&foreign);
        let metadata = serde_json::to_vec(&foreign).unwrap();
        fs::write(f.owner_path(), &metadata).unwrap();
        assert!(f.to.recover(&f.b, &approval, &d()).is_err());
        assert!(
            f.b.approve_recovery_at(
                &f.to,
                &plan.id,
                &genesis.id,
                RecoveryDirection::Resume,
                &d()
            )
            .is_err()
        );
        assert_eq!(fs::read(&path).unwrap(), bytes);
        assert_eq!(fs::read(f.owner_path()).unwrap(), metadata);
        assert_eq!(fs::read(&f.target).unwrap(), ORIGINAL);
        let inspected = f.to.inspect_journal(&plan.id, &d()).unwrap();
        assert!(inspected.pending);
        assert_eq!(inspected.event_id, genesis.id);
    }
    #[test]
    fn partial_owned_metadata_recovers_in_either_direction_without_changing_source() {
        for count in [0, 7] {
            for direction in [RecoveryDirection::Resume, RecoveryDirection::Rollback] {
                let f = Fixture::new();
                let p = f.plan();
                io::set_document_owner_write_stop(Some(count));
                assert!(f.apply(&p).is_err());
                assert!(f.owner_path().is_file());
                assert_eq!(fs::read(f.owner_path()).unwrap().len(), count);
                let review = inspect_binding_migration(&f.from, &f.to, &p, &d()).unwrap();
                assert_eq!(review.phase, "writing");
                f.recover(&p, &review.event_id, direction).unwrap();
                assert_eq!(fs::read(&f.target).unwrap(), ORIGINAL);
            }
        }
    }
    #[test]
    fn stale_events_fixed_directions_and_foreign_metadata_are_preserved() {
        let f = Fixture::new();
        let p = f.plan();
        super::super::STOP_AFTER.with(|slot| *slot.borrow_mut() = Some("binding_migration_marked"));
        assert!(f.apply(&p).is_err());
        let review = inspect_binding_migration(&f.from, &f.to, &p, &d()).unwrap();
        assert!(
            f.recover(&p, &"0".repeat(64), RecoveryDirection::Resume)
                .is_err()
        );
        let own = fs::read(f.owner_path()).unwrap();
        fs::write(f.owner_path(), b"foreign metadata").unwrap();
        assert!(
            f.recover(&p, &review.event_id, RecoveryDirection::Resume)
                .is_err()
        );
        assert_eq!(fs::read(f.owner_path()).unwrap(), b"foreign metadata");
        fs::write(f.owner_path(), own).unwrap();
        io::set_document_owner_write_stop(Some(5));
        assert!(
            f.recover(&p, &review.event_id, RecoveryDirection::Rollback)
                .is_err()
        );
        let fixed = inspect_binding_migration(&f.from, &f.to, &p, &d()).unwrap();
        assert!(
            f.recover(&p, &fixed.event_id, RecoveryDirection::Resume)
                .is_err()
        );
        f.recover(&p, &fixed.event_id, RecoveryDirection::Rollback)
            .unwrap();
        assert_eq!(fs::read(&f.target).unwrap(), ORIGINAL);
    }
    #[test]
    fn source_changes_and_missing_dual_authority_refuse_before_owner_mutation() {
        let f = Fixture::new();
        let p = f.plan();
        let before = fs::read(f.owner_path()).unwrap();
        assert!(BindingMigrationAuthority::new(&f.from, &f.to, "old", "new", &f.a, &f.a).is_err());
        fs::write(&f.target, b"foreign config").unwrap();
        assert!(f.apply(&p).is_err());
        assert_eq!(fs::read(f.owner_path()).unwrap(), before);
        assert_eq!(fs::read(&f.target).unwrap(), b"foreign config");
        let mut serialized = serde_json::to_value(&p).unwrap();
        serialized["command"] = serde_json::json!("untrusted");
        assert!(serde_json::from_value::<BindingMigrationPlan>(serialized).is_err());
    }
    #[test]
    fn returning_to_an_old_selection_has_new_generation_and_refuses_the_old_plan() {
        let f = Fixture::new();
        let p = f.plan();
        f.apply(&p).unwrap();
        let back = plan_binding_migration(&f.to, &f.from, "new", "old", &d()).unwrap();
        let a = BindingMigrationAuthority::new(&f.to, &f.from, "new", "old", &f.b, &f.a).unwrap();
        let approval = a.approve(&f.to, &f.from, &back, &d()).unwrap();
        apply_binding_migration(&f.to, &f.from, &a, &approval, &back, &d()).unwrap();
        assert_ne!(back.after_owner.generation, p.before_owner.generation);
        let before = fs::read(f.owner_path()).unwrap();
        assert!(f.apply(&p).is_err());
        assert_eq!(fs::read(f.owner_path()).unwrap(), before);
        assert_eq!(fs::read(&f.target).unwrap(), ORIGINAL);
    }
    #[test]
    fn rehashed_event_cannot_skip_writing_or_hide_a_gap() {
        for sequence in [1, 2, MAX_MIGRATION_EVENTS] {
            let f = Fixture::new();
            let p = f.plan();
            super::super::STOP_AFTER
                .with(|slot| *slot.borrow_mut() = Some("binding_migration_marked"));
            assert!(f.apply(&p).is_err());
            let mut e = initial(&p);
            e.sequence = sequence;
            e.previous = Some(e.id.clone());
            e.phase = if sequence == 1 {
                MigrationPhase::Committed
            } else {
                MigrationPhase::Writing
            };
            e.id = event_id(&e);
            let root = &f.from.journal.as_ref().unwrap().0;
            write_record(
                root,
                &event_path(root, &migration_stem(&p), sequence),
                &serde_json::to_vec(&e).unwrap(),
                &d(),
            )
            .unwrap();
            assert!(inspect_binding_migration(&f.from, &f.to, &p, &d()).is_err());
            assert_eq!(fs::read(&f.target).unwrap(), ORIGINAL);
        }
    }
    #[test]
    fn unfinished_original_journal_blocks_planning_and_an_already_reviewed_migration() {
        let f = Fixture::new();
        let p = f.plan();
        let before = fs::read(f.owner_path()).unwrap();
        let document = f.from.read("old", &d()).unwrap();
        let write = f
            .from
            .plan("old", &document.revision, "pending owned fixture", &d())
            .unwrap();
        let approved = f.a.approve(&f.from, &write, &d()).unwrap();
        super::super::STOP_AFTER.with(|slot| *slot.borrow_mut() = Some("planned"));
        assert!(f.from.apply(&f.a, &approved, &write, &d()).is_err());
        assert!(plan_binding_migration(&f.from, &f.to, "old", "new", &d()).is_err());
        assert!(f.apply(&p).is_err());
        assert_eq!(fs::read(f.owner_path()).unwrap(), before);
        assert_eq!(fs::read(&f.target).unwrap(), ORIGINAL);
    }
    #[test]
    fn foreign_pending_marker_and_held_destination_lock_are_never_overwritten() {
        let f = Fixture::new();
        let p = f.plan();
        let before = fs::read(f.owner_path()).unwrap();
        {
            let _lock = state(&f.to, true, &d()).unwrap();
            assert!(f.apply(&p).is_err());
        }
        let marker = marker_path(&f.from.documents["old"]).unwrap();
        fs::write(&marker, b"foreign marker").unwrap();
        assert!(f.apply(&p).is_err());
        assert!(plan_binding_migration(&f.from, &f.to, "old", "new", &d()).is_err());
        assert_eq!(fs::read(&marker).unwrap(), b"foreign marker");
        assert_eq!(fs::read(f.owner_path()).unwrap(), before);
        assert_eq!(fs::read(&f.target).unwrap(), ORIGINAL);
    }
    #[test]
    fn rehashed_foreign_target_and_expired_request_fail_before_intent_or_owner_write() {
        let f = Fixture::new();
        let mut p = f.plan();
        let before = fs::read(f.owner_path()).unwrap();
        p.target_path.push_str(".foreign");
        p.id = plan_id(&p);
        assert!(f.apply(&p).is_err());
        let p = f.plan();
        let authority = f.authority();
        let approved = authority.approve(&f.from, &f.to, &p, &d()).unwrap();
        assert!(
            apply_binding_migration(&f.from, &f.to, &authority, &approved, &p, &Deadline::new(0))
                .is_err()
        );
        assert!(!marker_path(&f.from.documents["old"]).unwrap().exists());
        assert_eq!(fs::read(f.owner_path()).unwrap(), before);
        assert_eq!(fs::read(&f.target).unwrap(), ORIGINAL);
    }
    #[test]
    fn terminal_journal_with_a_foreign_stage_cannot_be_retired_by_migration() {
        let f = Fixture::new();
        let original = f.from.read("old", &d()).unwrap();
        let change = f
            .from
            .plan("old", &original.revision, "first global result\r\n", &d())
            .unwrap();
        let approval = f.a.approve(&f.from, &change, &d()).unwrap();
        f.from.apply(&f.a, &approval, &change, &d()).unwrap();
        let plan = f.plan();
        let before = fs::read(f.owner_path()).unwrap();
        let stage = output_path(&f.from.documents["old"], &change.id, 0);
        fs::write(&stage, b"owned foreign-stage fixture").unwrap();
        assert!(plan_binding_migration(&f.from, &f.to, "old", "new", &d()).is_err());
        assert!(f.apply(&plan).is_err());
        assert_eq!(fs::read(&stage).unwrap(), b"owned foreign-stage fixture");
        assert_eq!(fs::read(f.owner_path()).unwrap(), before);
        assert_eq!(fs::read(&f.target).unwrap(), b"first global result\r\n");
    }
    #[test]
    fn a_fixed_direction_retry_uses_the_reserved_last_terminal_event() {
        let f = Fixture::new();
        let p = f.plan();
        super::super::STOP_AFTER.with(|slot| *slot.borrow_mut() = Some("binding_migration_marked"));
        assert!(f.apply(&p).is_err());
        let root = &f.from.journal.as_ref().unwrap().0;
        let mut event = initial(&p);
        // Authored historical retry fixtures exercise admission at the event
        // ceiling; they do not claim that additional owner writes happened.
        for _ in 1..MAX_MIGRATION_EVENTS - 1 {
            next(
                root,
                &p,
                &mut event,
                MigrationPhase::Writing,
                Some(RecoveryDirection::Rollback),
                &d(),
            )
            .unwrap();
        }
        assert_eq!(event.sequence, MAX_MIGRATION_EVENTS - 2);
        let result = f
            .recover(&p, &event.id, RecoveryDirection::Rollback)
            .unwrap();
        assert_eq!(result.outcome, "rolled_back");
        let head = load_event(&f.from, &f.to, &p, root, &d()).unwrap();
        assert_eq!(head.sequence, MAX_MIGRATION_EVENTS - 1);
        assert_eq!(fs::read(&f.target).unwrap(), ORIGINAL);
    }
}
