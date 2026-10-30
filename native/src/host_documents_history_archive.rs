//! Explicit immutable evidence archival followed by identity-checked retirement.
//! Host authority and the archive selection are never deserialized from a plan.
use super::*;
use base64::Engine as _;
#[path = "host_documents_history_purge.rs"]
mod history_purge;
pub use history_purge::{
    HistoryArchivePurgeAccess, HistoryArchivePurgeApproval, HistoryArchivePurgePermission,
    HistoryArchivePurgePlan, HistoryArchivePurgeReview, HistoryPurgeFileWitness,
    HistoryPurgeRetainedWitness, apply_history_archive_purge, inspect_history_archive_purge,
    plan_history_archive_purge, recover_history_archive_purge,
};

const MAX_ARCHIVE: usize = 16_000_000;
const MAX_RECORDS: usize = 512;
const MAX_ARCHIVE_ENTRIES: usize = 256;
const MAX_ARCHIVE_ROOT_BYTES: u64 = 64_000_000;
const MAGIC: &[u8] = b"YOKE-DOCUMENT-HISTORY\0\x01";

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Record {
    name: String,
    identity: String,
    security_hash: String,
    hash: String,
    bytes: usize,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct HistoryArchivePlan {
    pub schema_version: u32,
    pub id: String,
    pub migration: BindingMigrationPlan,
    pub archive_root: String,
    pub archive_identity: String,
    pub source_identity: String,
    pub source_hash: String,
    pub source_security_hash: String,
    pub source_revision: String,
    pub owner_identity: String,
    pub owner_security_hash: String,
    pub owner_hash: String,
    pub nonce: String,
    pub record_budget: usize,
    pub unselected_closed_journals_at_review: usize,
    records: Vec<Record>,
}
fn archive_plan_id(p: &HistoryArchivePlan) -> String {
    let mut copy = p.clone();
    copy.id.clear();
    identity(&("host_document_history_archive_v1", copy))
}
fn history_marker(selected: &BoundDocument) -> Result<PathBuf> {
    Ok(selected.parent_path.join(format!(
        ".yoke-doc-history-{}.pending.json",
        target_key(selected)?
    )))
}
pub(super) fn pending(selected: &BoundDocument, d: &Deadline) -> Result<bool> {
    Ok(exists(&history_marker(selected)?, d)? || history_purge::pending(selected, d)?)
}
pub(super) fn require_no_pending(selected: &BoundDocument, d: &Deadline) -> Result<()> {
    ensure!(
        !pending(selected, d)?,
        "Host document history archival is pending; use explicit host archive recovery"
    );
    Ok(())
}
fn history_stem(p: &HistoryArchivePlan) -> String {
    format!("yoke-doc-history-{}", p.id)
}
fn archive_path(root: &Path, p: &HistoryArchivePlan) -> PathBuf {
    root.join(format!("{}.bin", history_stem(p)))
}

/// Separately selected private host storage plus both original host grants.
pub struct HistoryArchiveAccess {
    migration: BindingMigrationAuthority,
    migration_id: String,
    root: PathBuf,
    root_identity: String,
    record_budget: usize,
}
pub struct HistoryArchiveApproval {
    plan_id: String,
    archive_identity: String,
}
fn archive_directory(
    from: &Access,
    to: &Access,
    path: &Path,
    d: &Deadline,
) -> Result<io::GuardedDirectory> {
    ensure!(
        path.is_absolute(),
        "Explicit absolute archive root required"
    );
    let held = io::guarded_directory(path, d)?;
    // Canonical paths and actual ancestor identities both constrain aliases.
    let mut excluded = Vec::new();
    for access in [from, to] {
        for (_, path, id) in &access.project_roots {
            excluded.push((path.clone(), id.clone()));
        }
        if let Some((path, id)) = &access.journal {
            excluded.push((path.clone(), id.clone()));
        }
        for selected in access.documents.values() {
            // A parent can contain a separate archive child; the selected file
            // itself must never lie inside the archive namespace.
            let target = selected.parent_path.join(
                selected
                    .spec
                    .path
                    .file_name()
                    .context("Selected document leaf absent")?,
            );
            ensure!(
                !inside(&target, &held.path)?,
                "Archive namespace contains a selected host document"
            );
        }
    }
    for (path, id) in excluded {
        let other = io::guarded_directory(&path, d)?;
        ensure!(
            other.identity == id
                && !inside(&held.path, &other.path)?
                && !inside(&other.path, &held.path)?
                && !io::has_ancestor_identity(&held, &id, d)?
                && !io::has_ancestor_identity(&other, &held.identity, d)?,
            "Archive root overlaps analyzed or active journal namespaces"
        );
    }
    Ok(held)
}
impl HistoryArchiveAccess {
    pub fn new(
        from: &Access,
        to: &Access,
        migration: &BindingMigrationPlan,
        archive_root: &Path,
        a: &WriteAuthority,
        b: &WriteAuthority,
        d: &Deadline,
    ) -> Result<Self> {
        let authority = BindingMigrationAuthority::new(
            from,
            to,
            &migration.from_document_id,
            &migration.to_document_id,
            a,
            b,
        )?;
        authority.check(from, to, migration, d)?;
        let held = archive_directory(from, to, archive_root, d)?;
        Ok(Self {
            migration: authority,
            migration_id: migration.id.clone(),
            root: held.path.clone(),
            root_identity: held.identity.clone(),
            record_budget: MAX_RECORDS,
        })
    }
    pub fn with_record_budget(mut self, records: usize) -> Result<Self> {
        ensure!(
            (MAX_EVENTS as usize..=MAX_RECORDS).contains(&records),
            "History archive record budget refused"
        );
        self.record_budget = records;
        Ok(self)
    }
    fn root(&self, from: &Access, to: &Access, d: &Deadline) -> Result<io::GuardedDirectory> {
        let held = archive_directory(from, to, &self.root, d)?;
        ensure!(held.identity == self.root_identity, "Archive root changed");
        Ok(held)
    }
    fn check(
        &self,
        from: &Access,
        to: &Access,
        p: &HistoryArchivePlan,
        d: &Deadline,
    ) -> Result<()> {
        self.migration.check(from, to, &p.migration, d)?;
        ensure!(
            p.schema_version == 1
                && p.id == archive_plan_id(p)
                && valid_id(&p.id)
                && p.migration.id == self.migration_id
                && self.root.to_str() == Some(p.archive_root.as_str())
                && p.archive_identity == self.root_identity
                && p.record_budget == self.record_budget
                && p.owner_identity == p.migration.owner_identity
                && p.owner_security_hash == p.migration.owner_security_hash
                && valid_id(&p.nonce)
                && [
                    &p.source_hash,
                    &p.source_security_hash,
                    &p.source_revision,
                    &p.owner_security_hash,
                    &p.owner_hash,
                ]
                .iter()
                .all(|v| valid_id(v))
                && [&p.source_identity, &p.owner_identity, &p.archive_identity]
                    .iter()
                    .all(|v| !v.is_empty() && v.len() <= 128)
                && !p.records.is_empty()
                && p.records.len() <= self.record_budget
                && p.unselected_closed_journals_at_review <= MAX_ENTRIES
                && serde_json::to_vec(p)?.len() <= MAX_PLAN - MAX_EVENT,
            "Invalid or foreign history archive plan"
        );
        let mut previous: Option<&str> = None;
        let mut total = 0usize;
        for record in &p.records {
            ensure!(
                previous.is_none_or(|name| name < record.name.as_str())
                    && record.name.is_ascii()
                    && !record.name.contains(['/', '\\', ':'])
                    && record
                        .name
                        .starts_with(&format!("yoke-doc-{}-", from.binding))
                    && (record.name.ends_with(".plan.json") || record.name.ends_with(".json"))
                    && !record.identity.is_empty()
                    && record.identity.len() <= 128
                    && valid_id(&record.hash)
                    && valid_id(&record.security_hash)
                    && record.bytes > 0
                    && record.bytes <= MAX_PLAN,
                "Invalid history record witness"
            );
            total = total
                .checked_add(record.bytes)
                .context("Archive size overflow")?;
            ensure!(
                total <= MAX_ARCHIVE - MAX_PLAN,
                "History archive byte capacity exceeded"
            );
            previous = Some(&record.name);
        }
        Ok(())
    }
    pub fn approve(
        &self,
        from: &Access,
        to: &Access,
        p: &HistoryArchivePlan,
        d: &Deadline,
    ) -> Result<HistoryArchiveApproval> {
        self.check(from, to, p, d)?;
        let _archive = self.root(from, to, d)?;
        let _old = from.hold_bindings(&p.migration.from_document_id, d)?;
        let _new = to.hold_bindings(&p.migration.to_document_id, d)?;
        let _locks = locks(from, to, d)?;
        let (_source, _owner) = current(from, to, p, d)?;
        let (selected, _) = selected_pair(
            from,
            to,
            &p.migration.from_document_id,
            &p.migration.to_document_id,
        )?;
        require_no_pending(selected, d)?;
        let batch = collect(from, to, &p.migration, p.record_budget, d)?;
        require_batch(p, &batch)?;
        let _checked = frame(p, &batch.bytes, d)?;
        Ok(HistoryArchiveApproval {
            plan_id: p.id.clone(),
            archive_identity: self.root_identity.clone(),
        })
    }
}

fn completed_migration(
    from: &Access,
    to: &Access,
    m: &BindingMigrationPlan,
    d: &Deadline,
) -> Result<()> {
    check_plan(from, to, m, d)?;
    let (a, b) = selected_pair(from, to, &m.from_document_id, &m.to_document_id)?;
    ensure!(
        !exists(&marker_path(a)?, d)?,
        "Binding migration remains pending"
    );
    let root = state(from, false, d)?;
    let e = load_event(from, to, m, &root.root.path, d)?;
    ensure!(
        e.phase == MigrationPhase::Committed,
        "Completed forward migration required before archival"
    );
    let complete = a
        .parent_path
        .join(format!(".yoke-doc-migration-{}.complete.json", m.id));
    let held = io::read_file(&complete, 2048, false, d)?
        .context("Retained migration completion absent")?;
    ensure!(
        held.bytes == serde_json::to_vec(&marker(m))?,
        "Migration completion differs"
    );
    let owner = read_owner(b, d)?.context("Current destination owner absent")?;
    ensure!(
        serde_json::to_vec(&owner)? == serde_json::to_vec(&m.after_owner)?,
        "Destination owner generation changed"
    );
    Ok(())
}
fn current(
    from: &Access,
    to: &Access,
    p: &HistoryArchivePlan,
    d: &Deadline,
) -> Result<(io::HeldFile, io::HeldFile)> {
    completed_migration(from, to, &p.migration, d)?;
    let (selected, source) = to.current(&p.migration.to_document_id, d)?;
    let owner =
        io::read_file(&owner_path(selected)?, 2048, false, d)?.context("Current owner absent")?;
    ensure!(
        source.identity == p.source_identity
            && source.hash == p.source_hash
            && io::security_fingerprint(&source, d)? == p.source_security_hash
            && to.revision(selected, &source, &p.source_security_hash) == p.source_revision
            && owner.identity == p.owner_identity
            && owner.hash == p.owner_hash
            && io::security_fingerprint(&owner, d)? == p.owner_security_hash,
        "Source or owner changed since history review"
    );
    Ok((source, owner))
}
struct Collected {
    records: Vec<Record>,
    bytes: BTreeMap<String, Vec<u8>>,
    unselected: usize,
}
fn require_batch(p: &HistoryArchivePlan, batch: &Collected) -> Result<()> {
    ensure!(
        serde_json::to_vec(&batch.records)? == serde_json::to_vec(&p.records)?
            && batch.unselected == p.unselected_closed_journals_at_review,
        "Reviewed history batch changed"
    );
    Ok(())
}
fn collect(
    from: &Access,
    to: &Access,
    m: &BindingMigrationPlan,
    record_budget: usize,
    d: &Deadline,
) -> Result<Collected> {
    collect_bounded(from, to, m, record_budget, d)
}
fn collect_bounded(
    from: &Access,
    to: &Access,
    m: &BindingMigrationPlan,
    max_records: usize,
    d: &Deadline,
) -> Result<Collected> {
    ensure!(
        (MAX_EVENTS as usize..=MAX_RECORDS).contains(&max_records),
        "Archive batch record budget invalid"
    );
    let (selected, _) = selected_pair(from, to, &m.from_document_id, &m.to_document_id)?;
    let root = state(from, false, d)?;
    let directory = io::open_metadata_directory(&root.root.path, d)?;
    let mut names = directory.names(MAX_ENTRIES, d)?;
    names.sort();
    let mut namespace_bytes = 0u64;
    for name in &names {
        if let Some(info) = directory.entry_info(name, d)? {
            namespace_bytes = namespace_bytes
                .checked_add(info.length)
                .context("Journal namespace byte overflow")?;
            ensure!(
                namespace_bytes <= MAX_STATE_BYTES,
                "Active journal byte capacity exceeded before archive admission"
            );
        }
    }
    let prefix = format!("yoke-doc-{}-", from.binding);
    let mut records = Vec::new();
    let mut bytes = BTreeMap::new();
    let mut total = 0usize;
    let mut unselected = 0usize;
    let mut accepting = true;
    for name in &names {
        if !name.starts_with(&prefix) || !name.ends_with(".plan.json") {
            continue;
        }
        let id = &name[prefix.len()..name.len() - ".plan.json".len()];
        let loaded = load(from, &root.root.path, id, d)?;
        if loaded.plan.document_id != m.from_document_id {
            continue;
        }
        ensure!(
            matches!(loaded.event.phase, Phase::Committed | Phase::RolledBack),
            "Unfinished journal cannot be archived"
        );
        ensure!(
            !exists(&backup_path(selected, id), d)?,
            "Terminal journal has an occupied backup"
        );
        for attempt in 0..=loaded.event.attempt {
            ensure!(
                !exists(&output_path(selected, id, attempt), d)?,
                "Terminal journal has an occupied output"
            );
        }
        let stem = stem(from, id)?;
        let expected = std::iter::once(format!("{stem}.plan.json"))
            .chain((1..=loaded.event.sequence).map(|n| format!("{stem}.e{n:04}.json")))
            .collect::<BTreeSet<_>>();
        for name in names
            .iter()
            .filter(|name| name.starts_with(&format!("{stem}.")))
        {
            ensure!(
                expected.contains(name),
                "Unknown selected journal record preserved"
            );
        }
        let mut journal_bytes = 0usize;
        for name in &expected {
            let info = directory
                .entry_info(name, d)?
                .context("Journal record absent during archive sizing")?;
            let length = usize::try_from(info.length)?;
            ensure!(length <= MAX_PLAN, "Oversized journal record preserved");
            journal_bytes = journal_bytes
                .checked_add(length)
                .context("Journal size overflow")?;
        }
        if !accepting
            || records.len() + expected.len() > max_records
            || total
                .checked_add(journal_bytes)
                .is_none_or(|n| n > MAX_ARCHIVE - MAX_PLAN)
        {
            accepting = false;
            unselected += 1;
            continue;
        }
        for name in expected {
            d.check()?;
            ensure!(
                records.len() < MAX_RECORDS,
                "History record capacity exceeded"
            );
            let held = io::read_file(&root.root.path.join(&name), MAX_PLAN, false, d)?
                .context("Reviewed journal record absent")?;
            total = total
                .checked_add(held.bytes.len())
                .context("History byte overflow")?;
            ensure!(
                total <= MAX_ARCHIVE - MAX_PLAN,
                "History archive byte capacity exceeded"
            );
            records.push(Record {
                name: name.clone(),
                identity: held.identity.clone(),
                security_hash: io::security_fingerprint(&held, d)?,
                hash: held.hash.clone(),
                bytes: held.bytes.len(),
            });
            bytes.insert(name, held.bytes.clone());
        }
    }
    records.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(Collected {
        records,
        bytes,
        unselected,
    })
}
pub fn plan_history_archive(
    from: &Access,
    to: &Access,
    access: &HistoryArchiveAccess,
    m: &BindingMigrationPlan,
    d: &Deadline,
) -> Result<HistoryArchivePlan> {
    access.migration.check(from, to, m, d)?;
    ensure!(
        m.id == access.migration_id,
        "Archive migration selection differs"
    );
    let _old = from.hold_bindings(&m.from_document_id, d)?;
    let _new = to.hold_bindings(&m.to_document_id, d)?;
    let _locks = locks(from, to, d)?;
    let root = access.root(from, to, d)?;
    let (a, b) = selected_pair(from, to, &m.from_document_id, &m.to_document_id)?;
    require_no_pending(a, d)?;
    completed_migration(from, to, m, d)?;
    let (_, source) = to.current(&m.to_document_id, d)?;
    let owner = io::read_file(&owner_path(b)?, 2048, false, d)?.context("Current owner absent")?;
    let security = io::security_fingerprint(&source, d)?;
    let batch = collect(from, to, m, access.record_budget, d)?;
    let mut p = HistoryArchivePlan {
        schema_version: 1,
        id: String::new(),
        migration: m.clone(),
        archive_root: root
            .path
            .to_str()
            .context("Unicode archive root required")?
            .into(),
        archive_identity: root.identity.clone(),
        source_identity: source.identity.clone(),
        source_hash: source.hash.clone(),
        source_security_hash: security.clone(),
        source_revision: to.revision(b, &source, &security),
        owner_identity: owner.identity.clone(),
        owner_security_hash: io::security_fingerprint(&owner, d)?,
        owner_hash: owner.hash.clone(),
        nonce: nonce()?,
        record_budget: access.record_budget,
        unselected_closed_journals_at_review: batch.unselected,
        records: batch.records,
    };
    p.id = archive_plan_id(&p);
    access.check(from, to, &p, d)?;
    let _checked = frame(&p, &batch.bytes, d)?;
    Ok(p)
}

fn push_length(out: &mut Vec<u8>, n: usize) -> Result<()> {
    out.extend_from_slice(&u32::try_from(n)?.to_le_bytes());
    Ok(())
}
fn frame(
    p: &HistoryArchivePlan,
    records: &BTreeMap<String, Vec<u8>>,
    d: &Deadline,
) -> Result<Vec<u8>> {
    let plan = serde_json::to_vec(p)?;
    ensure!(
        plan.len() <= MAX_PLAN && records.len() == p.records.len(),
        "Archive manifest capacity or cardinality differs"
    );
    let mut out = MAGIC.to_vec();
    push_length(&mut out, plan.len())?;
    out.extend_from_slice(&plan);
    for record in &p.records {
        d.check()?;
        let bytes = records.get(&record.name).context("Archive record absent")?;
        ensure!(
            bytes.len() == record.bytes && hash(bytes) == record.hash,
            "Archive record bytes differ"
        );
        ensure!(
            out.len()
                .checked_add(4 + bytes.len())
                .is_some_and(|n| n <= MAX_ARCHIVE),
            "Archive frame capacity exceeded"
        );
        push_length(&mut out, bytes.len())?;
        out.extend_from_slice(bytes);
    }
    Ok(out)
}
fn take<'a>(bytes: &'a [u8], offset: &mut usize, length: usize) -> Result<&'a [u8]> {
    let end = offset
        .checked_add(length)
        .context("Archive length overflow")?;
    let result = bytes.get(*offset..end).context("Truncated archive frame")?;
    *offset = end;
    Ok(result)
}
fn length(bytes: &[u8], offset: &mut usize) -> Result<usize> {
    Ok(u32::from_le_bytes(take(bytes, offset, 4)?.try_into()?) as usize)
}
fn admit_frame<'a>(
    from: &Access,
    p: &HistoryArchivePlan,
    bytes: &'a [u8],
    d: &Deadline,
) -> Result<BTreeMap<String, &'a [u8]>> {
    ensure!(
        bytes.len() <= MAX_ARCHIVE && bytes.starts_with(MAGIC),
        "Unsupported or oversized history frame"
    );
    let mut at = MAGIC.len();
    let size = length(bytes, &mut at)?;
    ensure!(size <= MAX_PLAN, "Archive manifest capacity exceeded");
    let manifest: HistoryArchivePlan = canonical_bytes(take(bytes, &mut at, size)?, MAX_PLAN, d)?;
    ensure!(
        serde_json::to_vec(&manifest)? == serde_json::to_vec(p)?,
        "Archive plan differs"
    );
    let mut records = BTreeMap::new();
    for record in &p.records {
        d.check()?;
        let size = length(bytes, &mut at)?;
        ensure!(
            size == record.bytes && size <= MAX_PLAN,
            "Archive record length differs"
        );
        let data = take(bytes, &mut at, size)?;
        ensure!(
            hash(data) == record.hash && records.insert(record.name.clone(), data).is_none(),
            "Archive record hash or uniqueness differs"
        );
    }
    ensure!(at == bytes.len(), "Trailing history frame data");
    let root = from
        .journal
        .as_ref()
        .context("Original journal required")?
        .0
        .as_path();
    let mut admitted = BTreeSet::new();
    for (name, data) in &records {
        if !name.ends_with(".plan.json") {
            continue;
        }
        let prefix = format!("yoke-doc-{}-", from.binding);
        let id = name
            .strip_prefix(&prefix)
            .context("Foreign archive header")?
            .strip_suffix(".plan.json")
            .context("Invalid archive header")?;
        let header: Header = canonical_bytes(data, MAX_PLAN, d)?;
        let checked = admit_header(from, id, header, d)?;
        let stem = stem(from, id)?;
        let events = records
            .keys()
            .filter(|n| n.starts_with(&format!("{stem}.e")))
            .cloned()
            .collect::<Vec<_>>();
        let loaded = admit_events(
            from,
            root,
            id,
            checked,
            events.clone(),
            |path| {
                let name = path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .context("Archive record name required")?;
                canonical_bytes(
                    records.get(name).context("Archived event absent")?,
                    MAX_EVENT,
                    d,
                )
            },
            d,
        )?;
        ensure!(
            loaded.plan.document_id == p.migration.from_document_id
                && matches!(loaded.event.phase, Phase::Committed | Phase::RolledBack),
            "Foreign or unfinished archived journal"
        );
        admitted.insert(name.clone());
        admitted.extend(events);
    }
    ensure!(
        admitted.len() == records.len(),
        "Unrecognized or orphan archive record"
    );
    Ok(records)
}

#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum ArchivePhase {
    Prepared,
    Archived,
    Removing,
    Committed,
    Cancelled,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ArchiveEvent {
    schema_version: u32,
    id: String,
    plan_id: String,
    sequence: u32,
    previous: Option<String>,
    phase: ArchivePhase,
}
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ArchiveHeader {
    schema_version: u32,
    plan: HistoryArchivePlan,
    initial: ArchiveEvent,
}
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ArchiveMarker {
    schema_version: u32,
    plan_id: String,
    archive_identity: String,
}
fn archive_marker(p: &HistoryArchivePlan) -> ArchiveMarker {
    ArchiveMarker {
        schema_version: 1,
        plan_id: p.id.clone(),
        archive_identity: p.archive_identity.clone(),
    }
}
fn archive_event_id(e: &ArchiveEvent) -> String {
    let mut copy = e.clone();
    copy.id.clear();
    identity(&("host_document_history_event_v1", copy))
}
fn archive_initial(p: &HistoryArchivePlan) -> ArchiveEvent {
    let mut e = ArchiveEvent {
        schema_version: 1,
        id: String::new(),
        plan_id: p.id.clone(),
        sequence: 0,
        previous: None,
        phase: ArchivePhase::Prepared,
    };
    e.id = archive_event_id(&e);
    e
}
fn archive_step(a: ArchivePhase, b: ArchivePhase) -> bool {
    matches!(
        (a, b),
        (ArchivePhase::Prepared, ArchivePhase::Archived)
            | (
                ArchivePhase::Prepared | ArchivePhase::Archived,
                ArchivePhase::Cancelled
            )
            | (ArchivePhase::Archived, ArchivePhase::Removing)
            | (ArchivePhase::Removing, ArchivePhase::Committed)
    )
}
fn load_archive_event(root: &Path, p: &HistoryArchivePlan, d: &Deadline) -> Result<ArchiveEvent> {
    let stem = history_stem(p);
    let header: ArchiveHeader = canonical(&plan_path(root, &stem), MAX_PLAN, d)?;
    let mut e = archive_initial(p);
    ensure!(
        header.schema_version == 1
            && serde_json::to_vec(&header.plan)? == serde_json::to_vec(p)?
            && serde_json::to_vec(&header.initial)? == serde_json::to_vec(&e)?,
        "Archive operation header differs"
    );
    let directory = io::open_metadata_directory(root, d)?;
    let mut paths = Vec::new();
    for name in directory.names(MAX_ARCHIVE_ENTRIES, d)? {
        if name.starts_with(&format!("{stem}."))
            && name != format!("{stem}.plan.json")
            && name != format!("{stem}.bin")
        {
            paths.push(name);
        }
    }
    paths.sort();
    ensure!(paths.len() <= 3, "Archive event capacity exceeded");
    for (index, name) in paths.into_iter().enumerate() {
        let sequence = index as u32 + 1;
        ensure!(
            root.join(name) == event_path(root, &stem, sequence),
            "Archive event gap or foreign entry"
        );
        let n: ArchiveEvent = canonical(&event_path(root, &stem, sequence), MAX_EVENT, d)?;
        ensure!(
            n.schema_version == 1
                && n.id == archive_event_id(&n)
                && n.plan_id == p.id
                && n.sequence == sequence
                && n.previous.as_ref() == Some(&e.id)
                && archive_step(e.phase, n.phase),
            "Archive event lineage or direction differs"
        );
        e = n;
    }
    Ok(e)
}
fn next_archive(
    root: &Path,
    p: &HistoryArchivePlan,
    e: &mut ArchiveEvent,
    phase: ArchivePhase,
    d: &Deadline,
) -> Result<()> {
    let previous = load_archive_event(root, p, d)?;
    ensure!(
        previous.id == e.id && archive_step(e.phase, phase),
        "Stale archive event or invalid direction"
    );
    let mut n = e.clone();
    n.sequence += 1;
    n.previous = Some(e.id.clone());
    n.phase = phase;
    n.id = archive_event_id(&n);
    ensure!(n.sequence <= 3, "Archive event capacity exceeded");
    write_record(
        root,
        &event_path(root, &history_stem(p), n.sequence),
        &serde_json::to_vec(&n)?,
        d,
    )?;
    *e = n;
    Ok(())
}
fn archive_budget(root: &Path, reserve: usize, d: &Deadline) -> Result<()> {
    let directory = io::open_metadata_directory(root, d)?;
    let names = directory.names(MAX_ARCHIVE_ENTRIES, d)?;
    ensure!(
        names.len() + 6 <= MAX_ARCHIVE_ENTRIES,
        "Archive directory entry capacity exceeded"
    );
    let mut total = reserve as u64;
    for name in names {
        if let Some(info) = directory.entry_info(&name, d)? {
            total = total
                .checked_add(info.length)
                .context("Archive directory byte overflow")?;
        }
    }
    ensure!(
        total <= MAX_ARCHIVE_ROOT_BYTES,
        "Archive directory byte capacity exceeded"
    );
    Ok(())
}
fn validate_archive(
    from: &Access,
    root: &Path,
    p: &HistoryArchivePlan,
    d: &Deadline,
) -> Result<io::HeldFile> {
    let held = io::read_file(&archive_path(root, p), MAX_ARCHIVE, false, d)?
        .context("Immutable archive absent; preserve original records")?;
    admit_frame(from, p, &held.bytes, d)?;
    Ok(held)
}
fn original_records(
    from: &Access,
    p: &HistoryArchivePlan,
    allow_absent: bool,
    d: &Deadline,
) -> Result<usize> {
    let root = state(from, false, d)?;
    let directory = io::open_metadata_directory(&root.root.path, d)?;
    let names = directory.names(MAX_ENTRIES, d)?;
    let selected = p
        .records
        .iter()
        .map(|r| r.name.as_str())
        .collect::<BTreeSet<_>>();
    for record in p.records.iter().filter(|r| r.name.ends_with(".plan.json")) {
        let stem = record
            .name
            .strip_suffix(".plan.json")
            .context("Archive header name")?;
        for name in names.iter().filter(|n| n.starts_with(&format!("{stem}."))) {
            ensure!(
                selected.contains(name.as_str()),
                "Unknown original journal record preserved"
            );
        }
    }
    let mut present = 0;
    for record in &p.records {
        let held = io::read_owned_file(
            &root.root.path.join(&record.name),
            &[&record.identity],
            record.bytes,
            false,
            d,
        )?;
        if let Some(held) = held {
            ensure!(
                held.byte_length == record.bytes
                    && held.hash == record.hash
                    && io::security_fingerprint(&held, d)? == record.security_hash,
                "Original archive record changed"
            );
            present += 1;
        } else {
            ensure!(
                allow_absent,
                "Original archive record absent before removal intent"
            );
        }
    }
    Ok(present)
}
fn finish_marker(selected: &BoundDocument, p: &HistoryArchivePlan, d: &Deadline) -> Result<()> {
    let pending = history_marker(selected)?;
    let terminal = selected
        .parent_path
        .join(format!(".yoke-doc-history-{}.complete.json", p.id));
    let expected = serde_json::to_vec(&archive_marker(p))?;
    if let Some(mut held) = io::read_file(&pending, 2048, true, d)? {
        ensure!(held.bytes == expected, "Foreign history marker preserved");
        held.rename_no_replace(&terminal, d)?;
    } else {
        require_completion(selected, p, d)?;
    }
    Ok(())
}
fn require_completion(
    selected: &BoundDocument,
    p: &HistoryArchivePlan,
    d: &Deadline,
) -> Result<()> {
    let path = selected
        .parent_path
        .join(format!(".yoke-doc-history-{}.complete.json", p.id));
    let held = io::read_file(&path, 2048, false, d)?
        .context("Retained archive completion absent; outcome remains unconfirmed")?;
    ensure!(
        held.bytes == serde_json::to_vec(&archive_marker(p))?,
        "Archive completion differs"
    );
    Ok(())
}
fn check_marker(selected: &BoundDocument, p: &HistoryArchivePlan, d: &Deadline) -> Result<()> {
    let held = io::read_file(&history_marker(selected)?, 2048, false, d)?
        .context("Pending archive marker absent")?;
    ensure!(
        held.bytes == serde_json::to_vec(&archive_marker(p))?,
        "Foreign history marker preserved"
    );
    Ok(())
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryArchiveReview {
    pub plan_id: String,
    pub event_id: String,
    pub phase: String,
    pub original_records: usize,
    pub present_original_records: usize,
    pub absent_original_records: usize,
    pub immutable_archive_verified: bool,
    pub archived_record_evidence_available: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub purge: Option<HistoryArchivePurgeReview>,
    pub marker_present: bool,
    pub complete: bool,
    pub unselected_closed_journals_at_review: usize,
    pub all_closed_journals_selected_at_review: bool,
    pub original_absence_proves_this_process_deleted_it: bool,
    pub file_changed: bool,
    pub source_written: bool,
}
fn review(
    from: &Access,
    to: &Access,
    access: &HistoryArchiveAccess,
    p: &HistoryArchivePlan,
    e: &ArchiveEvent,
    d: &Deadline,
) -> Result<HistoryArchiveReview> {
    let root = access.root(from, to, d)?;
    let terminal = matches!(e.phase, ArchivePhase::Committed | ArchivePhase::Cancelled);
    let purge = history_purge::archived_frame_status(from, to, access, p, d)?;
    let has_archive = exists(&archive_path(&root.path, p), d)?;
    if has_archive {
        let _held = validate_archive(from, &root.path, p, d)?;
    }
    ensure!(
        has_archive
            || matches!(e.phase, ArchivePhase::Prepared | ArchivePhase::Cancelled)
            || (e.phase == ArchivePhase::Committed
                && purge.as_ref().is_some_and(|r| r.evidence_unavailable)),
        "Durable archive required after publication"
    );
    let present = original_records(
        from,
        p,
        matches!(e.phase, ArchivePhase::Removing | ArchivePhase::Committed),
        d,
    )?;
    ensure!(
        e.phase != ArchivePhase::Committed || present == 0,
        "Committed archival has an occupied original record"
    );
    let selected = &from.documents[&p.migration.from_document_id];
    let marker_present = pending(selected, d)?;
    if terminal && !marker_present {
        require_completion(selected, p, d)?;
    }
    let purge_complete = purge.as_ref().is_none_or(|r| r.complete);
    Ok(HistoryArchiveReview {
        plan_id: p.id.clone(),
        event_id: e.id.clone(),
        phase: serde_json::to_value(e.phase)?
            .as_str()
            .context("Archive phase")?
            .into(),
        original_records: p.records.len(),
        present_original_records: present,
        absent_original_records: p.records.len() - present,
        immutable_archive_verified: has_archive,
        archived_record_evidence_available: has_archive,
        purge,
        marker_present,
        complete: terminal && !marker_present && purge_complete,
        unselected_closed_journals_at_review: p.unselected_closed_journals_at_review,
        all_closed_journals_selected_at_review: p.unselected_closed_journals_at_review == 0,
        original_absence_proves_this_process_deleted_it: false,
        file_changed: false,
        source_written: false,
    })
}
pub fn inspect_history_archive(
    from: &Access,
    to: &Access,
    access: &HistoryArchiveAccess,
    p: &HistoryArchivePlan,
    d: &Deadline,
) -> Result<HistoryArchiveReview> {
    access.check(from, to, p, d)?;
    let _old = from.hold_bindings(&p.migration.from_document_id, d)?;
    let _new = to.hold_bindings(&p.migration.to_document_id, d)?;
    let root = access.root(from, to, d)?;
    let e = load_archive_event(&root.path, p, d)?;
    review(from, to, access, p, &e, d)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryArchiveRecord {
    pub plan_id: String,
    pub record_name: String,
    pub original_identity: String,
    pub original_security_sha256: String,
    pub sha256: String,
    pub byte_length: usize,
    pub bytes_base64: String,
    pub immutable_record_evidence_only: bool,
    pub file_changed: bool,
    pub source_written: bool,
}
/// Reads only the admitted immutable frame, never a reconstructed/live journal.
pub fn read_history_archive_record(
    from: &Access,
    to: &Access,
    access: &HistoryArchiveAccess,
    p: &HistoryArchivePlan,
    name: &str,
    d: &Deadline,
) -> Result<HistoryArchiveRecord> {
    access.check(from, to, p, d)?;
    let _old = from.hold_bindings(&p.migration.from_document_id, d)?;
    let _new = to.hold_bindings(&p.migration.to_document_id, d)?;
    let root = access.root(from, to, d)?;
    let _event = load_archive_event(&root.path, p, d)?;
    let record = p
        .records
        .iter()
        .find(|r| r.name == name)
        .context("Archive record is not selected")?;
    if history_purge::archived_frame_status(from, to, access, p, d)?
        .is_some_and(|r| r.evidence_unavailable)
    {
        anyhow::bail!(
            "Exact archived record evidence unavailable after recorded physical purge; retained receipts do not contain record bytes"
        );
    }
    let held = validate_archive(from, &root.path, p, d)?;
    let records = admit_frame(from, p, &held.bytes, d)?;
    let bytes = records
        .get(name)
        .context("Selected archive record absent")?;
    d.check()?;
    Ok(HistoryArchiveRecord {
        plan_id: p.id.clone(),
        record_name: name.into(),
        original_identity: record.identity.clone(),
        original_security_sha256: record.security_hash.clone(),
        sha256: record.hash.clone(),
        byte_length: record.bytes,
        bytes_base64: base64::engine::general_purpose::STANDARD.encode(bytes),
        immutable_record_evidence_only: true,
        file_changed: false,
        source_written: false,
    })
}
fn remove_records(from: &Access, p: &HistoryArchivePlan, d: &Deadline) -> Result<()> {
    original_records(from, p, true, d)?;
    let root = state(from, false, d)?;
    // Remove headers last. The immutable checked frame retains every original
    // record, and the pending marker keeps partial live histories unavailable.
    for record in p
        .records
        .iter()
        .filter(|r| !r.name.ends_with(".plan.json"))
        .chain(p.records.iter().filter(|r| r.name.ends_with(".plan.json")))
    {
        d.check()?;
        if let Some(held) = io::read_owned_file(
            &root.root.path.join(&record.name),
            &[&record.identity],
            record.bytes,
            true,
            d,
        )? {
            ensure!(
                held.byte_length == record.bytes
                    && held.hash == record.hash
                    && io::security_fingerprint(&held, d)? == record.security_hash,
                "Original record changed before removal; preserved"
            );
            held.delete_owned(d)?;
            checkpoint("history_archive_record_removed")?;
        }
    }
    Ok(())
}
fn resume(
    from: &Access,
    to: &Access,
    access: &HistoryArchiveAccess,
    p: &HistoryArchivePlan,
    e: &mut ArchiveEvent,
    root: &Path,
    d: &Deadline,
) -> Result<HistoryArchiveReview> {
    let selected = &from.documents[&p.migration.from_document_id];
    if matches!(e.phase, ArchivePhase::Committed | ArchivePhase::Cancelled) {
        review(from, to, access, p, e, d)?;
        finish_marker(selected, p, d)?;
        return review(from, to, access, p, e, d);
    }
    check_marker(selected, p, d)?;
    let (_source, _owner) = current(from, to, p, d)?;
    if e.phase == ArchivePhase::Prepared {
        let batch = collect(from, to, &p.migration, p.record_budget, d)?;
        require_batch(p, &batch)?;
        let data = frame(p, &batch.bytes, d)?;
        admit_frame(from, p, &data, d)?;
        if !exists(&archive_path(root, p), d)? {
            let temporary = root.join(format!(".tmp-history-{}", nonce()?));
            let mut held = io::prepare_file(&temporary, None, &data, d)?;
            held.sync_all(d)?;
            held.rename_no_replace(&archive_path(root, p), d)?;
        }
        let _archive = validate_archive(from, root, p, d)?;
        checkpoint("history_archive_published")?;
        next_archive(root, p, e, ArchivePhase::Archived, d)?;
        checkpoint("history_archive_recorded")?;
    }
    if e.phase == ArchivePhase::Archived {
        original_records(from, p, false, d)?;
        let _archive = validate_archive(from, root, p, d)?;
        next_archive(root, p, e, ArchivePhase::Removing, d)?;
        checkpoint("history_archive_removal_intent")?;
    }
    if e.phase == ArchivePhase::Removing {
        // Hold the admitted immutable archive open across all destructive work.
        let _archive = validate_archive(from, root, p, d)?;
        remove_records(from, p, d)?;
        next_archive(root, p, e, ArchivePhase::Committed, d)?;
        checkpoint("history_archive_committed")?;
        finish_marker(selected, p, d)?;
    }
    review(from, to, access, p, e, d)
}
pub fn apply_history_archive(
    from: &Access,
    to: &Access,
    access: &HistoryArchiveAccess,
    approval: &HistoryArchiveApproval,
    p: &HistoryArchivePlan,
    d: &Deadline,
) -> Result<HistoryArchiveReview> {
    access.check(from, to, p, d)?;
    ensure!(
        approval.plan_id == p.id && approval.archive_identity == access.root_identity,
        "Concrete history archive approval required"
    );
    let _old = from.hold_bindings(&p.migration.from_document_id, d)?;
    let _new = to.hold_bindings(&p.migration.to_document_id, d)?;
    let _locks = locks(from, to, d)?;
    let root = access.root(from, to, d)?;
    let _archive_lock = io::open_state_lock(&root.path.join(".yoke-history.lock"), d)?;
    let selected = &from.documents[&p.migration.from_document_id];
    require_no_pending(selected, d)?;
    let (_source, _owner) = current(from, to, p, d)?;
    let batch = collect(from, to, &p.migration, p.record_budget, d)?;
    require_batch(p, &batch)?;
    let data = frame(p, &batch.bytes, d)?;
    admit_frame(from, p, &data, d)?;
    archive_budget(&root.path, data.len() + MAX_PLAN + 3 * MAX_EVENT, d)?;
    ensure!(
        !exists(&archive_path(&root.path, p), d)?,
        "Occupied archive output preserved"
    );
    let mut e = archive_initial(p);
    let header = ArchiveHeader {
        schema_version: 1,
        plan: p.clone(),
        initial: e.clone(),
    };
    write_record(
        &root.path,
        &plan_path(&root.path, &history_stem(p)),
        &serde_json::to_vec(&header)?,
        d,
    )?;
    write_record(
        &selected.parent_path,
        &history_marker(selected)?,
        &serde_json::to_vec(&archive_marker(p))?,
        d,
    )?;
    checkpoint("history_archive_marked")?;
    resume(from, to, access, p, &mut e, &root.path, d)
}
pub fn recover_history_archive(
    from: &Access,
    to: &Access,
    access: &HistoryArchiveAccess,
    p: &HistoryArchivePlan,
    expected_event: &str,
    direction: RecoveryDirection,
    d: &Deadline,
) -> Result<HistoryArchiveReview> {
    access.check(from, to, p, d)?;
    ensure!(
        valid_id(expected_event),
        "Explicit history event review required"
    );
    let _old = from.hold_bindings(&p.migration.from_document_id, d)?;
    let _new = to.hold_bindings(&p.migration.to_document_id, d)?;
    let _locks = locks(from, to, d)?;
    let root = access.root(from, to, d)?;
    let _archive_lock = io::open_state_lock(&root.path.join(".yoke-history.lock"), d)?;
    let mut e = load_archive_event(&root.path, p, d)?;
    ensure!(e.id == expected_event, "Stale history archive event review");
    let selected = &from.documents[&p.migration.from_document_id];
    let terminal = matches!(e.phase, ArchivePhase::Committed | ArchivePhase::Cancelled);
    ensure!(
        !matches!(
            (e.phase, direction),
            (
                ArchivePhase::Removing | ArchivePhase::Committed,
                RecoveryDirection::Rollback
            ) | (ArchivePhase::Cancelled, RecoveryDirection::Resume)
        ),
        "Archive removal direction is already fixed"
    );
    if !terminal {
        let (_source, _owner) = current(from, to, p, d)?;
        if !pending(selected, d)? {
            ensure!(
                e.phase == ArchivePhase::Prepared,
                "Pending archive marker absent after prepared state"
            );
            original_records(from, p, false, d)?;
            write_record(
                &selected.parent_path,
                &history_marker(selected)?,
                &serde_json::to_vec(&archive_marker(p))?,
                d,
            )?;
        }
        check_marker(selected, p, d)?;
    }
    if direction == RecoveryDirection::Rollback && !terminal {
        original_records(from, p, false, d)?;
        if exists(&archive_path(&root.path, p), d)? {
            let _archive = validate_archive(from, &root.path, p, d)?;
        }
        next_archive(&root.path, p, &mut e, ArchivePhase::Cancelled, d)?;
    }
    resume(from, to, access, p, &mut e, &root.path, d)
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    use std::fs;
    fn d() -> Deadline {
        Deadline::new(30_000)
    }
    pub(super) struct Fixture {
        pub(super) _temp: tempfile::TempDir,
        pub(super) target: PathBuf,
        pub(super) source: PathBuf,
        pub(super) from: Access,
        pub(super) to: Access,
        pub(super) a: WriteAuthority,
        pub(super) b: WriteAuthority,
        pub(super) migration: BindingMigrationPlan,
        pub(super) archive: HistoryArchiveAccess,
    }
    impl Fixture {
        pub(super) fn new() -> Self {
            Self::with_saves(1)
        }
        pub(super) fn with_saves(saves: usize) -> Self {
            let temp = tempfile::tempdir().unwrap();
            let source = temp.path().join("source-İ");
            let old = temp.path().join("old-journal");
            let new = temp.path().join("new-journal");
            let archive_root = temp.path().join("archive-π-🦀");
            for path in [&source, &old, &new, &archive_root] {
                fs::create_dir(path).unwrap();
            }
            fs::write(source.join("untouched.rs"), b"fn untouched() {}\r\n").unwrap();
            let host = temp.path().join("host-settings");
            fs::create_dir(&host).unwrap();
            let target = host.join("settings-e\u{301}.yaml");
            fs::write(&target, "\u{feff}# owned π\r\nvalue: initial\r\n").unwrap();
            let roots = [ProjectRoot {
                name: "default".into(),
                path: source.clone(),
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
            for saved in 0..saves {
                let snapshot = from.read("old", &d()).unwrap();
                let p = from
                    .plan(
                        "old",
                        &snapshot.revision,
                        &format!("\u{feff}# owned π\r\nvalue: saved 🦀 {saved}\r\n"),
                        &d(),
                    )
                    .unwrap();
                let approval = a.approve(&from, &p, &d()).unwrap();
                from.apply(&a, &approval, &p, &d()).unwrap();
            }
            let migration = plan_binding_migration(&from, &to, "old", "new", &d()).unwrap();
            let authority =
                BindingMigrationAuthority::new(&from, &to, "old", "new", &a, &b).unwrap();
            let approval = authority.approve(&from, &to, &migration, &d()).unwrap();
            apply_binding_migration(&from, &to, &authority, &approval, &migration, &d()).unwrap();
            let archive =
                HistoryArchiveAccess::new(&from, &to, &migration, &archive_root, &a, &b, &d())
                    .unwrap();
            Self {
                _temp: temp,
                target,
                source,
                from,
                to,
                a,
                b,
                migration,
                archive,
            }
        }
        pub(super) fn plan(&self) -> HistoryArchivePlan {
            plan_history_archive(&self.from, &self.to, &self.archive, &self.migration, &d())
                .unwrap()
        }
        pub(super) fn apply(&self, p: &HistoryArchivePlan) -> Result<HistoryArchiveReview> {
            let approved = self.archive.approve(&self.from, &self.to, p, &d())?;
            apply_history_archive(&self.from, &self.to, &self.archive, &approved, p, &d())
        }
        fn inspect(&self, p: &HistoryArchivePlan) -> HistoryArchiveReview {
            inspect_history_archive(&self.from, &self.to, &self.archive, p, &d()).unwrap()
        }
        fn recover(
            &self,
            p: &HistoryArchivePlan,
            event: &str,
            direction: RecoveryDirection,
        ) -> Result<HistoryArchiveReview> {
            recover_history_archive(
                &self.from,
                &self.to,
                &self.archive,
                p,
                event,
                direction,
                &d(),
            )
        }
        fn originals(&self, p: &HistoryArchivePlan) -> BTreeMap<String, Vec<u8>> {
            let root = &self.from.journal.as_ref().unwrap().0;
            p.records
                .iter()
                .map(|r| (r.name.clone(), fs::read(root.join(&r.name)).unwrap()))
                .collect()
        }
        fn interrupt(&self, p: &HistoryArchivePlan, checkpoint: &'static str) {
            STOP_AFTER.with(|s| *s.borrow_mut() = Some(checkpoint));
            assert!(self.apply(p).is_err());
        }
    }
    #[test]
    fn archives_exact_closed_records_before_removal_and_preserves_source_owner_and_foreign_data() {
        let f = Fixture::new();
        let p = f.plan();
        let originals = f.originals(&p);
        let source = fs::read(&f.target).unwrap();
        let untouched = fs::read(f.source.join("untouched.rs")).unwrap();
        let owner = owner_path(&f.to.documents["new"]).unwrap();
        let owner_bytes = fs::read(&owner).unwrap();
        let root = &f.from.journal.as_ref().unwrap().0;
        fs::write(root.join("foreign.txt"), b"keep").unwrap();
        let result = f.apply(&p).unwrap();
        assert!(result.complete && result.immutable_archive_verified && !result.marker_present);
        assert_eq!(result.absent_original_records, p.records.len());
        assert!(!result.original_absence_proves_this_process_deleted_it);
        assert_eq!(fs::read(&f.target).unwrap(), source);
        assert_eq!(fs::read(&owner).unwrap(), owner_bytes);
        assert_eq!(fs::read(f.source.join("untouched.rs")).unwrap(), untouched);
        assert_eq!(fs::read(root.join("foreign.txt")).unwrap(), b"keep");
        assert!(plan_path(root, &migration_stem(&f.migration)).exists());
        let data = fs::read(archive_path(&f.archive.root, &p)).unwrap();
        admit_frame(&f.from, &p, &data, &d()).unwrap();
        for bytes in originals.values() {
            assert!(data.windows(bytes.len()).any(|w| w == bytes));
        }
        let snapshot = f.to.read("new", &d()).unwrap();
        let next =
            f.to.plan(
                "new",
                &snapshot.revision,
                "# explicitly reviewed after archival\n",
                &d(),
            )
            .unwrap();
        let approved = f.b.approve(&f.to, &next, &d()).unwrap();
        f.to.apply(&f.b, &approved, &next, &d()).unwrap();
        assert!(f.a.approve(&f.from, &next, &d()).is_err());
        let repeated = f
            .recover(&p, &result.event_id, RecoveryDirection::Resume)
            .unwrap();
        assert!(repeated.complete);
    }
    #[test]
    fn interrupted_stages_keep_both_authorities_blocked_until_reviewed_resume() {
        for stop in [
            "history_archive_marked",
            "history_archive_published",
            "history_archive_removal_intent",
            "history_archive_record_removed",
            "history_archive_committed",
        ] {
            let f = Fixture::new();
            let p = f.plan();
            let source = fs::read(&f.target).unwrap();
            f.interrupt(&p, stop);
            let inspected = f.inspect(&p);
            assert!(inspected.marker_present);
            assert!(!inspected.complete);
            assert!(f.from.history_archive_blocked("old", &d()).unwrap());
            assert!(f.to.history_archive_blocked("new", &d()).unwrap());
            assert!(f.to.owner_binding("new", &d()).is_err());
            assert!(plan_binding_migration(&f.to, &f.from, "new", "old", &d()).is_err());
            let wrong = "a".repeat(64);
            assert!(f.recover(&p, &wrong, RecoveryDirection::Resume).is_err());
            let result = f
                .recover(&p, &inspected.event_id, RecoveryDirection::Resume)
                .unwrap();
            assert_eq!(result.phase, "committed");
            assert!(!result.marker_present);
            assert_eq!(fs::read(&f.target).unwrap(), source);
        }
    }
    #[test]
    fn cancellation_before_removal_preserves_all_originals_and_any_immutable_archive() {
        for stop in [
            "history_archive_marked",
            "history_archive_published",
            "history_archive_recorded",
        ] {
            let f = Fixture::new();
            let p = f.plan();
            let originals = f.originals(&p);
            f.interrupt(&p, stop);
            let inspected = f.inspect(&p);
            let bin = archive_path(&f.archive.root, &p);
            let archived = fs::read(&bin).ok();
            let result = f
                .recover(&p, &inspected.event_id, RecoveryDirection::Rollback)
                .unwrap();
            assert_eq!(result.phase, "cancelled");
            assert!(!result.marker_present);
            assert_eq!(f.originals(&p), originals);
            assert_eq!(fs::read(&bin).ok(), archived);
            assert!(
                f.recover(&p, &result.event_id, RecoveryDirection::Resume)
                    .is_err()
            );
            assert!(
                f.recover(&p, &result.event_id, RecoveryDirection::Rollback)
                    .unwrap()
                    .complete
            );
        }
    }
    #[test]
    fn removal_intent_fixes_resume_and_never_reconstructs_an_absent_original() {
        let f = Fixture::new();
        let p = f.plan();
        f.interrupt(&p, "history_archive_removal_intent");
        let inspected = f.inspect(&p);
        assert!(
            f.recover(&p, &inspected.event_id, RecoveryDirection::Rollback)
                .is_err()
        );
        // An explicit removal intent permits an absent original. Its absence is
        // evidence of current state, never proof of who removed it.
        let path = f.from.journal.as_ref().unwrap().0.join(&p.records[0].name);
        fs::remove_file(&path).unwrap();
        let inspected = f.inspect(&p);
        assert_eq!(inspected.absent_original_records, 1);
        assert!(!inspected.original_absence_proves_this_process_deleted_it);
        let result = f
            .recover(&p, &inspected.event_id, RecoveryDirection::Resume)
            .unwrap();
        assert!(result.complete && !path.exists());
    }
    #[test]
    fn missing_or_foreign_records_before_removal_intent_preserve_uncertainty() {
        for replacement in [false, true] {
            let f = Fixture::new();
            let p = f.plan();
            f.interrupt(&p, "history_archive_marked");
            let inspected = f.inspect(&p);
            let path = f.from.journal.as_ref().unwrap().0.join(&p.records[0].name);
            fs::remove_file(&path).unwrap();
            if replacement {
                fs::write(&path, b"foreign record").unwrap();
            }
            assert!(
                f.recover(&p, &inspected.event_id, RecoveryDirection::Resume)
                    .is_err()
            );
            assert!(
                f.recover(&p, &inspected.event_id, RecoveryDirection::Rollback)
                    .is_err()
            );
            assert!(f.to.history_archive_blocked("new", &d()).unwrap());
            assert_eq!(
                fs::read(&path).ok(),
                replacement.then(|| b"foreign record".to_vec())
            );
        }
    }
    #[test]
    fn replaced_original_after_partial_removal_is_preserved_with_immutable_evidence() {
        let f = Fixture::new();
        let p = f.plan();
        f.interrupt(&p, "history_archive_record_removed");
        let inspected = f.inspect(&p);
        let root = &f.from.journal.as_ref().unwrap().0;
        let r = p
            .records
            .iter()
            .find(|r| root.join(&r.name).exists())
            .unwrap();
        let path = root.join(&r.name);
        let retained = path.with_extension("owned-retained");
        fs::rename(&path, &retained).unwrap();
        fs::write(&path, b"foreign occupancy").unwrap();
        let archive = fs::read(archive_path(&f.archive.root, &p)).unwrap();
        assert!(
            f.recover(&p, &inspected.event_id, RecoveryDirection::Resume)
                .is_err()
        );
        assert_eq!(fs::read(&path).unwrap(), b"foreign occupancy");
        assert_eq!(
            fs::read(archive_path(&f.archive.root, &p)).unwrap(),
            archive
        );
    }
    #[test]
    fn changed_source_owner_or_review_refuses_before_archive_intent() {
        let f = Fixture::new();
        let mut p = f.plan();
        let approved = f.archive.approve(&f.from, &f.to, &p, &d()).unwrap();
        p.source_hash = "b".repeat(64);
        p.id = archive_plan_id(&p);
        assert!(apply_history_archive(&f.from, &f.to, &f.archive, &approved, &p, &d()).is_err());
        let p = f.plan();
        fs::write(&f.target, b"external authored edit\n").unwrap();
        assert!(f.apply(&p).is_err());
        assert!(!f.to.history_archive_blocked("new", &d()).unwrap());
        assert!(!plan_path(&f.archive.root, &history_stem(&p)).exists());
    }
    #[test]
    fn archive_and_active_namespace_overlap_or_missing_independent_authority_refuse() {
        let f = Fixture::new();
        for path in [
            &f.source,
            &f.from.journal.as_ref().unwrap().0,
            &f.to.journal.as_ref().unwrap().0,
            f._temp.path(),
            f.target.parent().unwrap(),
        ] {
            assert!(
                HistoryArchiveAccess::new(&f.from, &f.to, &f.migration, path, &f.a, &f.b, &d())
                    .is_err()
            );
        }
        assert!(
            HistoryArchiveAccess::new(
                &f.from,
                &f.to,
                &f.migration,
                &f.archive.root,
                &f.b,
                &f.a,
                &d()
            )
            .is_err()
        );
        assert!(
            HistoryArchiveAccess::new(
                &f.from,
                &f.to,
                &f.migration,
                Path::new("relative"),
                &f.a,
                &f.b,
                &d()
            )
            .is_err()
        );
        let alias = f.archive.root.join(".. ");
        assert!(
            HistoryArchiveAccess::new(&f.from, &f.to, &f.migration, &alias, &f.a, &f.b, &d())
                .is_err()
        );
    }
    #[test]
    fn immutable_frame_rejects_truncation_lengths_trailing_bytes_and_rehashed_orphan_records() {
        let f = Fixture::new();
        let p = f.plan();
        let records = collect(&f.from, &f.to, &f.migration, p.record_budget, &d())
            .unwrap()
            .bytes;
        let data = frame(&p, &records, &d()).unwrap();
        admit_frame(&f.from, &p, &data, &d()).unwrap();
        for length in [0, MAGIC.len(), MAGIC.len() + 3, data.len() - 1] {
            assert!(admit_frame(&f.from, &p, &data[..length], &d()).is_err());
        }
        let mut bad = data.clone();
        bad[MAGIC.len()..MAGIC.len() + 4].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(admit_frame(&f.from, &p, &bad, &d()).is_err());
        let mut bad = data.clone();
        bad.push(0);
        assert!(admit_frame(&f.from, &p, &bad, &d()).is_err());
        let mut bad = data.clone();
        *bad.last_mut().unwrap() ^= 1;
        assert!(admit_frame(&f.from, &p, &bad, &d()).is_err());
        let mut forged = p.clone();
        let mut records = records;
        let index = forged
            .records
            .iter()
            .position(|r| r.name.ends_with(".plan.json"))
            .unwrap();
        let mut r = forged.records[index].clone();
        let data = records.remove(&r.name).unwrap();
        r.name = r.name.replace(".plan.json", ".e0031.json");
        records.insert(r.name.clone(), data);
        forged.records[index] = r;
        forged.records.sort_by(|a, b| a.name.cmp(&b.name));
        forged.id = archive_plan_id(&forged);
        let bad = frame(&forged, &records, &d()).unwrap();
        assert!(admit_frame(&f.from, &forged, &bad, &d()).is_err());
    }
    #[test]
    fn foreign_outputs_markers_and_completion_entries_are_never_overwritten() {
        let f = Fixture::new();
        let p = f.plan();
        let bin = archive_path(&f.archive.root, &p);
        fs::write(&bin, b"foreign archive").unwrap();
        assert!(f.apply(&p).is_err());
        assert_eq!(fs::read(&bin).unwrap(), b"foreign archive");
        let f = Fixture::new();
        let p = f.plan();
        let marker = history_marker(&f.from.documents["old"]).unwrap();
        fs::write(&marker, b"foreign marker").unwrap();
        assert!(f.apply(&p).is_err());
        assert_eq!(fs::read(&marker).unwrap(), b"foreign marker");
        let f = Fixture::new();
        let p = f.plan();
        let complete = f.from.documents["old"]
            .parent_path
            .join(format!(".yoke-doc-history-{}.complete.json", p.id));
        fs::write(&complete, b"foreign completion").unwrap();
        assert!(f.apply(&p).is_err());
        let inspected = f.inspect(&p);
        assert_eq!(inspected.phase, "committed");
        assert!(inspected.marker_present);
        assert!(!inspected.complete);
        assert!(
            f.recover(&p, &inspected.event_id, RecoveryDirection::Resume)
                .is_err()
        );
        assert_eq!(fs::read(&complete).unwrap(), b"foreign completion");
        let f = Fixture::new();
        let p = f.plan();
        f.interrupt(&p, "history_archive_committed");
        let marker = history_marker(&f.from.documents["old"]).unwrap();
        fs::remove_file(marker).unwrap();
        assert!(inspect_history_archive(&f.from, &f.to, &f.archive, &p, &d()).is_err());
        let e = load_archive_event(&f.archive.root, &p, &d()).unwrap();
        assert!(f.recover(&p, &e.id, RecoveryDirection::Resume).is_err());
    }
    #[test]
    fn finite_archive_entry_and_record_caps_refuse_without_mutation() {
        let f = Fixture::new();
        let p = f.plan();
        let originals = f.originals(&p);
        for n in 0..MAX_ARCHIVE_ENTRIES - 5 {
            fs::write(f.archive.root.join(format!("foreign-{n}")), b"keep").unwrap();
        }
        assert!(f.apply(&p).is_err());
        assert_eq!(f.originals(&p), originals);
        assert!(!f.to.history_archive_blocked("new", &d()).unwrap());
        let mut bad = p.clone();
        bad.records = vec![p.records[0].clone(); MAX_RECORDS + 1];
        bad.id = archive_plan_id(&bad);
        assert!(f.archive.approve(&f.from, &f.to, &bad, &d()).is_err());
    }
    #[test]
    fn journal_event_gap_and_unknown_selected_record_refuse_planning() {
        let f = Fixture::new();
        let p = f.plan();
        let root = &f.from.journal.as_ref().unwrap().0;
        let header = p
            .records
            .iter()
            .find(|r| r.name.ends_with(".plan.json"))
            .unwrap();
        let stem = header.name.strip_suffix(".plan.json").unwrap();
        let unknown = root.join(format!("{stem}.foreign.json"));
        fs::write(&unknown, b"retain").unwrap();
        assert!(plan_history_archive(&f.from, &f.to, &f.archive, &f.migration, &d()).is_err());
        assert_eq!(fs::read(&unknown).unwrap(), b"retain");
        fs::remove_file(&unknown).unwrap();
        let first = root.join(format!("{stem}.e0001.json"));
        fs::remove_file(&first).unwrap();
        assert!(plan_history_archive(&f.from, &f.to, &f.archive, &f.migration, &d()).is_err());
    }
    #[test]
    fn rehashed_archive_event_cannot_skip_durable_publication_and_removal_intent() {
        let f = Fixture::new();
        let p = f.plan();
        f.interrupt(&p, "history_archive_marked");
        let initial = archive_initial(&p);
        let mut forged = ArchiveEvent {
            schema_version: 1,
            id: String::new(),
            plan_id: p.id.clone(),
            sequence: 1,
            previous: Some(initial.id),
            phase: ArchivePhase::Committed,
        };
        forged.id = archive_event_id(&forged);
        let path = event_path(&f.archive.root, &history_stem(&p), 1);
        fs::write(path, serde_json::to_vec(&forged).unwrap()).unwrap();
        let original = f.originals(&p);
        assert!(inspect_history_archive(&f.from, &f.to, &f.archive, &p, &d()).is_err());
        assert!(
            f.recover(&p, &forged.id, RecoveryDirection::Resume)
                .is_err()
        );
        assert_eq!(f.originals(&p), original);
        assert!(!archive_path(&f.archive.root, &p).exists());
    }
    #[test]
    fn expired_deadline_and_replaced_archive_directory_refuse_before_intent() {
        let f = Fixture::new();
        let p = f.plan();
        assert!(
            plan_history_archive(&f.from, &f.to, &f.archive, &f.migration, &Deadline::new(0))
                .is_err()
        );
        let original = f.originals(&p);
        let retained = f._temp.path().join("retained-owned-archive-directory");
        fs::rename(&f.archive.root, &retained).unwrap();
        fs::create_dir(&f.archive.root).unwrap();
        assert!(f.apply(&p).is_err());
        assert_eq!(f.originals(&p), original);
        assert!(!f.to.history_archive_blocked("new", &d()).unwrap());
    }
    #[test]
    fn explicit_archive_reads_reproduce_exact_original_bytes_without_live_fallback() {
        let f = Fixture::new();
        let p = f.plan();
        let originals = f.originals(&p);
        assert!(
            read_history_archive_record(&f.from, &f.to, &f.archive, &p, &p.records[0].name, &d())
                .is_err()
        );
        f.apply(&p).unwrap();
        for (name, bytes) in &originals {
            let record =
                read_history_archive_record(&f.from, &f.to, &f.archive, &p, name, &d()).unwrap();
            assert_eq!(
                base64::engine::general_purpose::STANDARD
                    .decode(record.bytes_base64)
                    .unwrap(),
                *bytes
            );
            assert_eq!(record.sha256, hash(bytes));
            assert!(record.immutable_record_evidence_only);
        }
        for name in ["../foreign", "yoke-doc-unknown.plan.json", "", "é.json"] {
            assert!(
                read_history_archive_record(&f.from, &f.to, &f.archive, &p, name, &d()).is_err()
            );
        }
        let path = archive_path(&f.archive.root, &p);
        let retained = f.archive.root.join("retained-owned-frame.bin");
        fs::rename(&path, retained).unwrap();
        assert!(
            read_history_archive_record(&f.from, &f.to, &f.archive, &p, &p.records[0].name, &d())
                .is_err()
        );
    }
    #[test]
    fn bounded_batches_keep_whole_journals_and_require_a_fresh_review_for_the_remainder() {
        let mut f = Fixture::with_saves(6);
        f.archive = f.archive.with_record_budget(MAX_EVENTS as usize).unwrap();
        let first = f.plan();
        assert!(first.unselected_closed_journals_at_review > 0);
        assert!(first.records.len() <= MAX_EVENTS as usize);
        let original_namespace = collect(&f.from, &f.to, &f.migration, MAX_RECORDS, &d()).unwrap();
        let all_names = original_namespace
            .records
            .iter()
            .map(|r| r.name.clone())
            .collect::<BTreeSet<_>>();
        let result = f.apply(&first).unwrap();
        assert!(!result.all_closed_journals_selected_at_review);
        assert_eq!(
            result.unselected_closed_journals_at_review,
            first.unselected_closed_journals_at_review
        );
        let second = f.plan();
        assert_eq!(second.unselected_closed_journals_at_review, 0);
        assert_ne!(first.id, second.id);
        let first_names = first
            .records
            .iter()
            .map(|r| r.name.clone())
            .collect::<BTreeSet<_>>();
        let second_names = second
            .records
            .iter()
            .map(|r| r.name.clone())
            .collect::<BTreeSet<_>>();
        assert!(first_names.is_disjoint(&second_names));
        assert_eq!(
            first_names
                .union(&second_names)
                .cloned()
                .collect::<BTreeSet<_>>(),
            all_names
        );
        assert!(
            f.apply(&second)
                .unwrap()
                .all_closed_journals_selected_at_review
        );
        for p in [&first, &second] {
            for record in &p.records {
                assert!(
                    read_history_archive_record(&f.from, &f.to, &f.archive, p, &record.name, &d())
                        .is_ok()
                );
            }
        }
        assert!(plan_history_archive(&f.from, &f.to, &f.archive, &f.migration, &d()).is_err());
    }
}
