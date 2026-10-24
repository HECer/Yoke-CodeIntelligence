//! Conditional host-document publication and explicit, direction-preserving recovery.
//! These capabilities are constructed by the host, never deserialized from data.
use super::*;
#[path = "host_documents_binding_migration.rs"]
mod binding_migration;
pub use binding_migration::{
    BindingMigrationApproval, BindingMigrationAuthority, BindingMigrationPlan,
    BindingMigrationReceipt, BindingMigrationReview, apply_binding_migration,
    inspect_binding_migration, plan_binding_migration, recover_binding_migration,
};

const MAX_EVENTS: u32 = 32;
const MAX_EVENT: usize = 8192;
const MAX_ENTRIES: usize = 2048;
const MAX_STATE_BYTES: u64 = 16_000_000;
const RESERVED_BYTES: u64 = (2 * MAX_PLAN + 2 * MAX_EVENTS as usize * MAX_EVENT) as u64;
#[cfg(test)]
std::thread_local! {
    static STOP_AFTER: std::cell::RefCell<Option<&'static str>> = const { std::cell::RefCell::new(None) };
}
fn checkpoint(name: &str) -> Result<()> {
    #[cfg(test)]
    if STOP_AFTER.with(|slot| *slot.borrow() == Some(name)) {
        STOP_AFTER.with(|slot| *slot.borrow_mut() = None);
        anyhow::bail!("Owned interruption after {name}");
    }
    let _ = name;
    Ok(())
}

pub struct WriteAuthority {
    binding: String,
    documents: BTreeSet<String>,
}
pub struct Approval {
    binding: String,
    plan_id: String,
    document_id: String,
}
pub struct RecoveryApproval {
    binding: String,
    plan_id: String,
    event_id: String,
    direction: RecoveryDirection,
}
#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecoveryDirection {
    Resume,
    Rollback,
}
#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Phase {
    Planned,
    Allocating,
    Registered,
    Prepared,
    BackedUp,
    Published,
    Committed,
    RolledBack,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Event {
    schema_version: u32,
    id: String,
    plan_id: String,
    access_binding: String,
    sequence: u32,
    previous: Option<String>,
    phase: Phase,
    direction: Option<RecoveryDirection>,
    attempt: u8,
    output_identity: Option<String>,
    output_security_hash: Option<String>,
}
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Header {
    schema_version: u32,
    plan: Plan,
    initial: Event,
}
struct Transaction<'a> {
    access: &'a Access,
    plan: &'a Plan,
    selected: &'a BoundDocument,
    root: &'a Path,
    name: &'a str,
    d: &'a Deadline,
}
/// Restrictive, persistent ownership of the journal namespace for one target.
/// It grants no write authority and is never silently migrated or deleted.
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct OwnerBinding {
    schema_version: u32,
    id: String,
    target: String,
    access_binding: String,
    journal_identity: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    generation: Option<String>,
}
fn target_key(selected: &BoundDocument) -> Result<String> {
    let mut leaf = selected
        .spec
        .path
        .file_name()
        .and_then(|v| v.to_str())
        .context("Host document leaf must be Unicode")?
        .to_owned();
    if cfg!(windows) {
        leaf.make_ascii_lowercase();
    }
    Ok(identity(&(
        "host_document_target_v1",
        &selected.parent_identity,
        leaf,
    )))
}
fn owner_path(selected: &BoundDocument) -> Result<PathBuf> {
    Ok(selected
        .parent_path
        .join(format!(".yoke-doc-owner-{}.json", target_key(selected)?)))
}
fn owner_id(owner: &OwnerBinding) -> String {
    let mut copy = owner.clone();
    copy.id.clear();
    identity(&("host_document_owner_v1", copy))
}
fn expected_owner(access: &Access, selected: &BoundDocument) -> Result<OwnerBinding> {
    let mut owner = OwnerBinding {
        schema_version: 1,
        id: String::new(),
        target: target_key(selected)?,
        access_binding: access.binding.clone(),
        journal_identity: access
            .journal
            .as_ref()
            .context("Host document journal root required")?
            .1
            .clone(),
        generation: None,
    };
    owner.id = owner_id(&owner);
    Ok(owner)
}
fn read_owner(selected: &BoundDocument, d: &Deadline) -> Result<Option<OwnerBinding>> {
    let path = owner_path(selected)?;
    let Some(file) = io::read_file(&path, 2048, false, d)? else {
        return Ok(None);
    };
    let owner: OwnerBinding = serde_json::from_slice(&file.bytes)?;
    ensure!(
        serde_json::to_vec(&owner)? == file.bytes
            && ((owner.schema_version == 1 && owner.generation.is_none())
                || (owner.schema_version == 2
                    && owner.generation.as_deref().is_some_and(valid_id)))
            && owner.id == owner_id(&owner)
            && owner.target == target_key(selected)?
            && valid_id(&owner.access_binding)
            && !owner.journal_identity.is_empty()
            && owner.journal_identity.len() <= 128,
        "Invalid host document owner binding; no automatic adoption or cleanup"
    );
    Ok(Some(owner))
}
fn require_owner(access: &Access, selected: &BoundDocument, d: &Deadline) -> Result<()> {
    binding_migration::require_no_pending(selected, d)?;
    let owner = read_owner(selected, d)?
        .context("Host document owner binding absent; recovery requires its original authority")?;
    ensure!(
        binding_migration::matches_selection(&owner, &expected_owner(access, selected)?),
        "Host document belongs to a different write selection; restore its original authority and journal root"
    );
    Ok(())
}
fn bind_owner(access: &Access, selected: &BoundDocument, d: &Deadline) -> Result<()> {
    binding_migration::require_no_pending(selected, d)?;
    if read_owner(selected, d)?.is_some() {
        return require_owner(access, selected, d);
    }
    let expected = expected_owner(access, selected)?;
    write_record(
        &selected.parent_path,
        &owner_path(selected)?,
        &serde_json::to_vec(&expected)?,
        d,
    )?;
    require_owner(access, selected, d)
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Receipt {
    pub schema_version: u32,
    pub document_id: String,
    pub plan_id: String,
    pub journal_id: Option<String>,
    pub outcome: String,
    pub document_sha256: String,
    pub document_identity: String,
    pub file_changed: bool,
    pub metadata_changed: bool,
    pub source_written: bool,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JournalView {
    pub plan: Plan,
    pub event_id: String,
    pub sequence: u32,
    pub phase: String,
    pub direction: Option<RecoveryDirection>,
    pub pending: bool,
}
struct StateGuard {
    root: io::GuardedDirectory,
    _lock: Option<std::fs::File>,
}
struct Loaded {
    plan: Plan,
    event: Event,
}
fn journal_view(loaded: Loaded) -> Result<JournalView> {
    Ok(JournalView {
        plan: loaded.plan,
        event_id: loaded.event.id,
        sequence: loaded.event.sequence,
        phase: serde_json::to_value(loaded.event.phase)?
            .as_str()
            .context("Owned journal phase")?
            .into(),
        direction: loaded.event.direction,
        pending: !matches!(loaded.event.phase, Phase::Committed | Phase::RolledBack),
    })
}

fn event_id(event: &Event) -> String {
    let mut copy = event.clone();
    copy.id.clear();
    identity(&("host_document_event_v1", copy))
}
fn initial(access: &Access, plan: &Plan) -> Event {
    let mut event = Event {
        schema_version: 1,
        id: String::new(),
        plan_id: plan.id.clone(),
        access_binding: access.binding.clone(),
        sequence: 0,
        previous: None,
        phase: Phase::Planned,
        direction: None,
        attempt: 0,
        output_identity: None,
        output_security_hash: None,
    };
    event.id = event_id(&event);
    event
}
fn valid_step(before: &Event, after: &Event) -> bool {
    if before
        .direction
        .is_some_and(|mode| after.direction != Some(mode))
    {
        return false;
    }
    let same_output = before.output_identity == after.output_identity
        && before.output_security_hash == after.output_security_hash;
    if before.phase == after.phase
        && before.attempt == after.attempt
        && same_output
        && before.direction.is_none()
        && after.direction.is_some()
    {
        return true;
    }
    if before.direction != after.direction {
        return false;
    }
    if after.attempt == before.attempt + 1 {
        return before.direction == Some(RecoveryDirection::Resume)
            && matches!(before.phase, Phase::Allocating | Phase::Registered)
            && after.phase == Phase::Allocating
            && after.output_identity.is_none()
            && after.output_security_hash.is_none();
    }
    if before.attempt != after.attempt {
        return false;
    }
    if after.phase == Phase::RolledBack {
        return after.direction == Some(RecoveryDirection::Rollback) && same_output;
    }
    if before.phase == Phase::Allocating && after.phase == Phase::Registered {
        return before.output_identity.is_none() && after.output_identity.is_some();
    }
    same_output
        && matches!(
            (before.phase, after.phase),
            (Phase::Planned, Phase::Allocating)
                | (Phase::Registered, Phase::Prepared)
                | (Phase::Prepared, Phase::BackedUp)
                | (Phase::Prepared, Phase::Published)
                | (Phase::BackedUp, Phase::Published)
                | (Phase::Published, Phase::Committed)
        )
}
fn nonce() -> Result<String> {
    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes)
        .map_err(|_| anyhow::anyhow!("Host document journal identity unavailable"))?;
    Ok(hash(bytes))
}
fn stem(access: &Access, id: &str) -> Result<String> {
    ensure!(valid_id(id), "Invalid host document journal identifier");
    Ok(format!("yoke-doc-{}-{id}", access.binding))
}
fn plan_path(root: &Path, stem: &str) -> PathBuf {
    root.join(format!("{stem}.plan.json"))
}
fn event_path(root: &Path, stem: &str, seq: u32) -> PathBuf {
    root.join(format!("{stem}.e{seq:04}.json"))
}
fn output_path(selected: &BoundDocument, id: &str, attempt: u8) -> PathBuf {
    selected
        .parent_path
        .join(format!(".yoke-doc-{id}-{attempt}.new"))
}
fn backup_path(selected: &BoundDocument, id: &str) -> PathBuf {
    selected
        .parent_path
        .join(format!(".yoke-doc-{id}.original"))
}
fn exists(path: &Path, d: &Deadline) -> Result<bool> {
    Ok(io::entry_info(path, d)?.is_some())
}
fn budget(root: &Path, d: &Deadline) -> Result<()> {
    let directory = io::open_metadata_directory(root, d)?;
    let names = directory.names(MAX_ENTRIES, d)?;
    ensure!(
        names.len() + 2 * MAX_EVENTS as usize + 4 <= MAX_ENTRIES,
        "Host document journal entry capacity exceeded"
    );
    let mut bytes = 0u64;
    for name in names {
        d.check()?;
        if let Some(info) = directory.entry_info(&name, d)? {
            bytes = bytes
                .checked_add(info.length)
                .context("Host document journal byte count overflow")?;
            ensure!(
                bytes.saturating_add(RESERVED_BYTES) <= MAX_STATE_BYTES,
                "Host document journal byte capacity exceeded"
            );
        }
    }
    Ok(())
}
fn state(access: &Access, write: bool, d: &Deadline) -> Result<StateGuard> {
    let (path, id) = access
        .journal
        .as_ref()
        .context("Explicit host document journal root required")?;
    let root = io::guarded_directory(path, d)?;
    ensure!(&root.identity == id, "Host document journal root changed");
    let lock = if write {
        Some(io::open_state_lock(
            &root.path.join(".yoke-documents.lock"),
            d,
        )?)
    } else {
        None
    };
    Ok(StateGuard { root, _lock: lock })
}
fn write_record(root: &Path, path: &Path, bytes: &[u8], d: &Deadline) -> Result<()> {
    ensure!(
        path.parent() == Some(root) && bytes.len() <= MAX_PLAN,
        "Host document journal record capacity or path invalid"
    );
    let temporary = root.join(format!(".tmp-doc-{}", nonce()?));
    let mut file = io::prepare_file(&temporary, None, bytes, d)?;
    file.sync_all(d)?;
    let expected = file.identity.clone();
    file.rename_no_replace(path, d)?;
    drop(file);
    let read = io::read_owned_file(path, &[&expected], MAX_PLAN, false, d)?
        .context("Host document journal readback absent")?;
    ensure!(
        read.bytes == bytes,
        "Host document journal durable readback differs"
    );
    Ok(())
}
fn append(root: &Path, stem: &str, event: &mut Event, d: &Deadline) -> Result<()> {
    let previous: Event = if event.sequence == 0 {
        let header: Header = canonical(&plan_path(root, stem), MAX_PLAN, d)?;
        header.initial
    } else {
        canonical(&event_path(root, stem, event.sequence), MAX_EVENT, d)?
    };
    ensure!(
        previous.id == event.id && previous.sequence == event.sequence,
        "Host document event head changed"
    );
    let mut next = event.clone();
    next.sequence = next
        .sequence
        .checked_add(1)
        .context("Host document journal overflow")?;
    next.previous = Some(previous.id.clone());
    ensure!(
        next.sequence < MAX_EVENTS && valid_step(&previous, &next),
        "Host document journal transition invalid"
    );
    next.id = event_id(&next);
    let bytes = serde_json::to_vec(&next)?;
    ensure!(
        bytes.len() <= MAX_EVENT,
        "Host document event byte capacity exceeded"
    );
    write_record(root, &event_path(root, stem, next.sequence), &bytes, d)?;
    *event = next;
    Ok(())
}

fn canonical<T: serde::de::DeserializeOwned + Serialize>(
    path: &Path,
    cap: usize,
    d: &Deadline,
) -> Result<T> {
    let file =
        io::read_file(path, cap, false, d)?.context("Host document journal record absent")?;
    let value: T = serde_json::from_slice(&file.bytes)?;
    ensure!(
        serde_json::to_vec(&value)? == file.bytes,
        "Noncanonical host document journal record"
    );
    Ok(value)
}
fn load(access: &Access, root: &Path, id: &str, d: &Deadline) -> Result<Loaded> {
    let name = stem(access, id)?;
    let header: Header = canonical(&plan_path(root, &name), MAX_PLAN, d)?;
    let plan = header.plan;
    access.check_plan(&plan, d)?;
    let genesis = initial(access, &plan);
    ensure!(
        header.schema_version == 1
            && plan.id == id
            && plan.before_hash != plan.after_hash
            && serde_json::to_vec(&header.initial)? == serde_json::to_vec(&genesis)?,
        "Host document journal header differs"
    );
    let directory = io::open_metadata_directory(root, d)?;
    let prefix = format!("{name}.e");
    let mut paths = directory
        .names(MAX_ENTRIES, d)?
        .into_iter()
        .filter(|n| n.starts_with(&prefix))
        .collect::<Vec<_>>();
    paths.sort();
    ensure!(
        paths.len() < MAX_EVENTS as usize,
        "Host document journal event sequence oversized"
    );
    let mut last = genesis;
    for (index, path) in paths.into_iter().enumerate() {
        let sequence = index as u32 + 1;
        ensure!(
            root.join(&path) == event_path(root, &name, sequence),
            "Host document journal event gap or foreign name"
        );
        let event: Event = canonical(&root.join(path), MAX_EVENT, d)?;
        ensure!(
            event.schema_version == 1
                && event.id == event_id(&event)
                && event.plan_id == id
                && event.access_binding == access.binding
                && event.sequence == sequence
                && event.attempt < 3
                && event
                    .output_identity
                    .as_ref()
                    .is_none_or(|v| !v.is_empty() && v.len() <= 128 && *v != plan.before_identity)
                && event
                    .output_security_hash
                    .as_ref()
                    .is_none_or(|v| valid_id(v) && *v == plan.before_security_hash)
                && event.output_identity.is_some() == event.output_security_hash.is_some()
                && (!matches!(
                    event.phase,
                    Phase::Registered
                        | Phase::Prepared
                        | Phase::BackedUp
                        | Phase::Published
                        | Phase::Committed
                ) || event.output_identity.is_some())
                && (!matches!(event.phase, Phase::Planned | Phase::Allocating)
                    || event.output_identity.is_none()),
            "Host document journal event binding invalid"
        );
        ensure!(
            event.previous.as_ref() == Some(&last.id)
                && !matches!(last.phase, Phase::Committed | Phase::RolledBack)
                && valid_step(&last, &event),
            "Host document journal lineage or direction changed"
        );
        last = event;
    }
    Ok(Loaded { plan, event: last })
}
fn pending_document(access: &Access, root: &Path, document: &str, d: &Deadline) -> Result<bool> {
    let directory = io::open_metadata_directory(root, d)?;
    let prefix = format!("yoke-doc-{}-", access.binding);
    let suffix = ".plan.json";
    for name in directory.names(MAX_ENTRIES, d)? {
        d.check()?;
        if !name.starts_with(&prefix) || !name.ends_with(suffix) {
            continue;
        }
        let id = &name[prefix.len()..name.len() - suffix.len()];
        ensure!(valid_id(id), "Invalid host document plan filename");
        let loaded = load(access, root, id, d)?;
        if loaded.plan.document_id == document
            && !matches!(loaded.event.phase, Phase::Committed | Phase::RolledBack)
        {
            return Ok(true);
        }
    }
    Ok(false)
}

fn original(
    access: &Access,
    plan: &Plan,
    selected: &BoundDocument,
    file: &io::HeldFile,
    d: &Deadline,
) -> Result<()> {
    let security = io::security_fingerprint(file, d)?;
    ensure!(
        file.identity == plan.before_identity
            && file.hash == plan.before_hash
            && security == plan.before_security_hash
            && access.revision(selected, file, &security) == plan.before_revision,
        "Stale host document or security revision"
    );
    Ok(())
}
fn receipt(
    plan: &Plan,
    file: &io::HeldFile,
    outcome: &str,
    changed: bool,
    journal: bool,
    metadata_changed: bool,
) -> Receipt {
    Receipt {
        schema_version: 1,
        document_id: plan.document_id.clone(),
        plan_id: plan.id.clone(),
        journal_id: journal.then(|| plan.id.clone()),
        outcome: outcome.into(),
        document_sha256: file.hash.clone(),
        document_identity: file.identity.clone(),
        file_changed: changed,
        metadata_changed,
        source_written: false,
    }
}
impl WriteAuthority {
    pub fn approve_recovery_at(
        &self,
        access: &Access,
        id: &str,
        expected_event: &str,
        direction: RecoveryDirection,
        d: &Deadline,
    ) -> Result<RecoveryApproval> {
        ensure!(
            valid_id(expected_event),
            "Invalid host document recovery revision"
        );
        let approval = self.approve_recovery(access, id, direction, d)?;
        ensure!(
            approval.event_id == expected_event,
            "Stale host document recovery review"
        );
        Ok(approval)
    }
    /// Host-only opt-in, to be constructed only after the caller's independent
    /// host-file write gate. Project files additionally need workspace approval
    /// and are deliberately excluded from this host-file authority.
    pub fn new(access: &Access) -> Result<Self> {
        ensure!(
            access.journal.is_some(),
            "Explicit host document journal root required"
        );
        let documents = access
            .documents
            .iter()
            .filter(|(_, v)| v.spec.writable && v.source_project.is_none())
            .map(|(id, _)| id.clone())
            .collect::<BTreeSet<_>>();
        ensure!(
            !documents.is_empty(),
            "No writable global host documents selected; project files require workspace approval"
        );
        Ok(Self {
            binding: access.binding.clone(),
            documents,
        })
    }
    fn check(&self, access: &Access, plan: &Plan, d: &Deadline) -> Result<()> {
        let selected = access.check_plan(plan, d)?;
        ensure!(
            self.binding == access.binding
                && self.documents.contains(&plan.document_id)
                && selected.spec.writable
                && selected.source_project.is_none(),
            "Independent host document write authority required"
        );
        Ok(())
    }
    /// The host calls this after reviewing the concrete proposal. Deserializing
    /// a plan, a flag or a confirmation string cannot construct this capability.
    pub fn approve(&self, access: &Access, plan: &Plan, d: &Deadline) -> Result<Approval> {
        self.check(access, plan, d)?;
        let (selected, file) = access.current(&plan.document_id, d)?;
        original(access, plan, selected, &file, d)?;
        Ok(Approval {
            binding: self.binding.clone(),
            plan_id: plan.id.clone(),
            document_id: plan.document_id.clone(),
        })
    }
    pub fn approve_recovery(
        &self,
        access: &Access,
        id: &str,
        direction: RecoveryDirection,
        d: &Deadline,
    ) -> Result<RecoveryApproval> {
        let guard = state(access, false, d)?;
        let loaded = load(access, &guard.root.path, id, d)?;
        self.check(access, &loaded.plan, d)?;
        ensure!(
            loaded.event.direction.is_none_or(|mode| mode == direction),
            "Host document recovery direction is already fixed"
        );
        ensure!(
            !matches!(
                (loaded.event.phase, direction),
                (Phase::Committed, RecoveryDirection::Rollback)
                    | (Phase::RolledBack, RecoveryDirection::Resume)
            ),
            "Completed host document recovery direction differs"
        );
        Ok(RecoveryApproval {
            binding: self.binding.clone(),
            plan_id: id.into(),
            event_id: loaded.event.id,
            direction,
        })
    }
}
impl Access {
    /// Presence is a blocker, not proof that a serialized record grants rights.
    pub fn binding_migration_blocked(&self, document_id: &str, d: &Deadline) -> Result<bool> {
        let _guards = self.hold_bindings(document_id, d)?;
        let selected = self
            .documents
            .get(document_id)
            .context("Host document not selected")?;
        binding_migration::pending_metadata_present(selected, d)
    }
    /// The opaque identity of the original host selection. This is restrictive
    /// metadata only; it cannot authorize a path, journal relocation or writes.
    pub fn owner_binding(&self, document_id: &str, d: &Deadline) -> Result<Option<String>> {
        let _guards = self.hold_bindings(document_id, d)?;
        let selected = self
            .documents
            .get(document_id)
            .context("Host document not selected")?;
        binding_migration::require_no_pending(selected, d)?;
        Ok(read_owner(selected, d)?.map(|owner| owner.access_binding))
    }
    pub fn pending_journals(&self, document_id: &str, d: &Deadline) -> Result<Vec<JournalView>> {
        self.selection(document_id)?;
        self.owner_binding(document_id, d)?;
        let guard = state(self, false, d)?;
        let directory = io::open_metadata_directory(&guard.root.path, d)?;
        let prefix = format!("yoke-doc-{}-", self.binding);
        let mut result = Vec::new();
        for name in directory.names(MAX_ENTRIES, d)? {
            if !name.starts_with(&prefix) || !name.ends_with(".plan.json") {
                continue;
            }
            let id = &name[prefix.len()..name.len() - ".plan.json".len()];
            let loaded = load(self, &guard.root.path, id, d)?;
            if loaded.plan.document_id == document_id
                && !matches!(loaded.event.phase, Phase::Committed | Phase::RolledBack)
            {
                ensure!(
                    result.len() < 32,
                    "Pending host document journal capacity exceeded"
                );
                result.push(journal_view(loaded)?);
            }
        }
        Ok(result)
    }
    pub fn inspect_journal(&self, id: &str, d: &Deadline) -> Result<JournalView> {
        let guard = state(self, false, d)?;
        let loaded = load(self, &guard.root.path, id, d)?;
        journal_view(loaded)
    }
    pub fn apply(
        &self,
        authority: &WriteAuthority,
        approval: &Approval,
        plan: &Plan,
        d: &Deadline,
    ) -> Result<Receipt> {
        authority.check(self, plan, d)?;
        let _bindings = self.hold_bindings(&plan.document_id, d)?;
        ensure!(
            approval.binding == self.binding
                && approval.plan_id == plan.id
                && approval.document_id == plan.document_id,
            "Concrete host document approval required"
        );
        let (selected, mut before) = self.hold_current(&plan.document_id, true, d)?;
        original(self, plan, selected, &before, d)?;
        if before.hash == plan.after_hash {
            return Ok(receipt(plan, &before, "unchanged", false, false, false));
        }
        let guard = state(self, false, d)?;
        let name = stem(self, &plan.id)?;
        let output = output_path(selected, &plan.id, 0);
        let backup = backup_path(selected, &plan.id);
        ensure!(
            !exists(&plan_path(&guard.root.path, &name), d)?
                && !exists(&output, d)?
                && !exists(&backup, d)?,
            "Host document plan already exists or temporary name is occupied; explicit recovery required"
        );
        budget(&guard.root.path, d)?;
        drop(guard);
        let guard = state(self, true, d)?;
        budget(&guard.root.path, d)?;
        ensure!(
            !pending_document(self, &guard.root.path, &plan.document_id, d)?,
            "Pending host document journal requires explicit recovery"
        );
        bind_owner(self, selected, d)?;
        checkpoint("owner_bound")?;
        let mut event = initial(self, plan);
        let header = Header {
            schema_version: 1,
            plan: plan.clone(),
            initial: event.clone(),
        };
        write_record(
            &guard.root.path,
            &plan_path(&guard.root.path, &name),
            &serde_json::to_vec(&header)?,
            d,
        )?;
        checkpoint("planned")?;
        let transaction = Transaction {
            access: self,
            plan,
            selected,
            root: &guard.root.path,
            name: &name,
            d,
        };
        let mut prepared = prepare(&transaction, &before, &mut event)?;
        checkpoint("prepared")?;
        original(self, plan, selected, &before, d)?;
        before.rename_no_replace(&backup, d)?;
        checkpoint("original_moved")?;
        event.phase = Phase::BackedUp;
        append(&guard.root.path, &name, &mut event, d)?;
        prepared.rename_no_replace(&selected.spec.path, d)?;
        checkpoint("output_promoted")?;
        event.phase = Phase::Published;
        append(&guard.root.path, &name, &mut event, d)?;
        checkpoint("published")?;
        ensure!(
            io::security_fingerprint(&prepared, d)? == plan.before_security_hash,
            "Published host document security changed"
        );
        before.delete_owned(d)?;
        checkpoint("original_cleaned")?;
        event.phase = Phase::Committed;
        append(&guard.root.path, &name, &mut event, d)?;
        Ok(receipt(plan, &prepared, "committed", true, true, true))
    }
}
fn token(
    access: &Access,
    plan: &Plan,
    selected: &BoundDocument,
    event: &Event,
) -> Result<crate::host_install::DurableOutputRegistration> {
    ensure!(
        matches!(
            event.phase,
            Phase::Registered
                | Phase::Prepared
                | Phase::BackedUp
                | Phase::Published
                | Phase::Committed
                | Phase::RolledBack
        ) && event.output_security_hash.as_deref() == Some(plan.before_security_hash.as_str()),
        "Host document output registration absent"
    );
    Ok(crate::host_install::DurableOutputRegistration {
        plan_id: plan.id.clone(),
        journal_id: plan.id.clone(),
        root_index: access
            .documents
            .keys()
            .position(|id| id == &plan.document_id)
            .context("Owned document index")?,
        attempt: event.attempt,
        path: output_path(selected, &plan.id, event.attempt),
        identity: event
            .output_identity
            .clone()
            .context("Owned document output identity absent")?,
        after_hash: plan.after_hash.clone(),
        security_hash: plan.before_security_hash.clone(),
    })
}
fn prepare(
    transaction: &Transaction<'_>,
    before: &io::HeldFile,
    event: &mut Event,
) -> Result<io::HeldFile> {
    let &Transaction {
        access,
        plan,
        selected,
        root,
        name,
        d,
    } = transaction;
    original(access, plan, selected, before, d)?;
    event.phase = Phase::Allocating;
    append(root, name, event, d)?;
    let path = output_path(selected, &plan.id, event.attempt);
    let armed = io::allocate_output(&path, Some(before), d)?;
    let binding = armed.binding();
    ensure!(
        binding.security_hash == plan.before_security_hash,
        "Allocated host document security differs"
    );
    event.phase = Phase::Registered;
    event.output_identity = Some(binding.identity);
    event.output_security_hash = Some(binding.security_hash);
    append(root, name, event, d)?;
    checkpoint("registered_before_retain")?;
    let registration = token(access, plan, selected, event)?;
    let mut registered = armed.retain(&registration, d)?;
    while registered.byte_length(d)? < plan.content.len() {
        registered.append_chunk(plan.content.as_bytes(), d)?;
    }
    let prepared = registered.finalize(plan.content.as_bytes(), d)?;
    event.phase = Phase::Prepared;
    append(root, name, event, d)?;
    Ok(prepared)
}

struct Roles {
    target: Option<io::HeldFile>,
    backup: Option<io::HeldFile>,
    stage: Option<io::RegisteredOutput>,
    target_before: bool,
}
fn roles(
    access: &Access,
    plan: &Plan,
    selected: &BoundDocument,
    event: &Event,
    d: &Deadline,
) -> Result<Roles> {
    let mut ids = vec![plan.before_identity.as_str()];
    if let Some(id) = &event.output_identity {
        ids.push(id);
    }
    let target = io::read_owned_file(&selected.spec.path, &ids, access.max_bytes, true, d)?;
    let target_before = target
        .as_ref()
        .is_some_and(|f| f.identity == plan.before_identity);
    if let Some(file) = &target {
        if target_before {
            original(access, plan, selected, file, d)?;
        } else {
            ensure!(
                file.hash == plan.after_hash
                    && io::security_fingerprint(file, d)? == plan.before_security_hash,
                "Published host document changed"
            );
        }
    }
    let backup = io::read_owned_file(
        &backup_path(selected, &plan.id),
        &[&plan.before_identity],
        access.max_bytes,
        true,
        d,
    )?;
    if let Some(file) = &backup {
        original(access, plan, selected, file, d)?;
    }
    ensure!(
        !(target_before && backup.is_some()),
        "Original host document appears at both roles"
    );
    let path = output_path(selected, &plan.id, event.attempt);
    let stage = if let Some(id) = &event.output_identity {
        let registration = token(access, plan, selected, event)?;
        let output = io::open_registered_output(
            &path,
            id,
            if target_before {
                target.as_ref()
            } else {
                backup.as_ref()
            },
            plan.content.as_bytes(),
            false,
            &registration,
            d,
        )?;
        if let Some(output) = &output
            && matches!(
                event.phase,
                Phase::Prepared | Phase::BackedUp | Phase::Published | Phase::Committed
            )
        {
            ensure!(
                output.byte_length(d)? == plan.content.len(),
                "Prepared host document became incomplete"
            );
        }
        output
    } else {
        ensure!(
            !exists(&path, d)?,
            "Unregistered host document output occupant refused"
        );
        None
    };
    ensure!(
        !(target
            .as_ref()
            .is_some_and(|f| f.identity != plan.before_identity)
            && stage.is_some()),
        "Owned host output appears at both roles"
    );
    Ok(Roles {
        target,
        backup,
        stage,
        target_before,
    })
}
fn choose_direction(
    root: &Path,
    name: &str,
    event: &mut Event,
    direction: RecoveryDirection,
    d: &Deadline,
) -> Result<()> {
    ensure!(
        event.direction.is_none_or(|current| current == direction),
        "Host document recovery direction is already fixed"
    );
    if event.direction.is_none() {
        event.direction = Some(direction);
        append(root, name, event, d)?;
    }
    checkpoint("recovery_direction")?;
    Ok(())
}
fn resume(transaction: &Transaction<'_>, event: &mut Event, mut roles: Roles) -> Result<Receipt> {
    let &Transaction {
        access,
        plan,
        selected,
        root,
        name,
        d,
    } = transaction;
    ensure!(
        !matches!(event.phase, Phase::RolledBack),
        "Rolled-back host document cannot resume"
    );
    if event.phase == Phase::Committed {
        ensure!(
            !roles.target_before && roles.backup.is_none() && roles.stage.is_none(),
            "Committed host document roles changed"
        );
        return Ok(receipt(
            plan,
            roles
                .target
                .as_ref()
                .context("Committed host document absent")?,
            "committed",
            false,
            true,
            false,
        ));
    }
    ensure!(
        roles.target_before || roles.backup.is_some() || roles.target.is_some(),
        "Original host document and published output are both absent"
    );
    let needs_publication = roles.target.is_none() || roles.target_before;
    if needs_publication && roles.stage.is_none() {
        ensure!(
            roles.target_before
                && roles.backup.is_none()
                && matches!(
                    event.phase,
                    Phase::Planned | Phase::Allocating | Phase::Registered
                ),
            "Owned prepared output disappeared after publication preconditions"
        );
        if event.phase != Phase::Planned {
            ensure!(
                event.attempt < 2,
                "Host document allocation attempts exhausted"
            );
        }
    }
    choose_direction(root, name, event, RecoveryDirection::Resume, d)?;
    if !needs_publication {
        ensure!(
            roles.stage.is_none(),
            "Published host document has an extra output occupant"
        );
        if event.phase != Phase::Published {
            event.phase = Phase::Published;
            append(root, name, event, d)?;
        }
        if let Some(backup) = roles.backup.take() {
            backup.delete_owned(d)?;
        }
        event.phase = Phase::Committed;
        append(root, name, event, d)?;
        return Ok(receipt(
            plan,
            roles
                .target
                .as_ref()
                .context("Published host document absent")?,
            "committed",
            false,
            true,
            true,
        ));
    }
    let mut prepared = if let Some(mut stage) = roles.stage.take() {
        while stage.byte_length(d)? < plan.content.len() {
            stage.append_chunk(plan.content.as_bytes(), d)?;
        }
        let prepared = stage.finalize(plan.content.as_bytes(), d)?;
        if event.phase == Phase::Registered {
            event.phase = Phase::Prepared;
            append(root, name, event, d)?;
        }
        prepared
    } else {
        if event.phase != Phase::Planned {
            event.attempt += 1;
            event.output_identity = None;
            event.output_security_hash = None;
        }
        prepare(
            transaction,
            roles
                .target
                .as_ref()
                .context("Original host document absent before allocation")?,
            event,
        )?
    };
    if roles.target_before {
        let mut before = roles
            .target
            .take()
            .context("Original host document absent")?;
        original(access, plan, selected, &before, d)?;
        before.rename_no_replace(&backup_path(selected, &plan.id), d)?;
        roles.backup = Some(before);
    }
    ensure!(
        roles.backup.is_some(),
        "Host document original backup absent"
    );
    if event.phase != Phase::BackedUp {
        event.phase = Phase::BackedUp;
        append(root, name, event, d)?;
    }
    prepared.rename_no_replace(&selected.spec.path, d)?;
    event.phase = Phase::Published;
    append(root, name, event, d)?;
    ensure!(
        io::security_fingerprint(&prepared, d)? == plan.before_security_hash,
        "Published host document security changed"
    );
    roles
        .backup
        .take()
        .context("Original backup absent")?
        .delete_owned(d)?;
    event.phase = Phase::Committed;
    append(root, name, event, d)?;
    Ok(receipt(plan, &prepared, "committed", true, true, true))
}
fn rollback(transaction: &Transaction<'_>, event: &mut Event, mut roles: Roles) -> Result<Receipt> {
    let &Transaction {
        access,
        plan,
        selected,
        root,
        name,
        d,
    } = transaction;
    ensure!(
        !matches!(event.phase, Phase::Committed),
        "Committed host document cannot roll back"
    );
    if event.phase == Phase::RolledBack {
        ensure!(
            roles.target_before && roles.backup.is_none() && roles.stage.is_none(),
            "Rolled-back host document roles changed"
        );
        return Ok(receipt(
            plan,
            roles
                .target
                .as_ref()
                .context("Rolled-back host document absent")?,
            "rolled_back",
            false,
            true,
            false,
        ));
    }
    ensure!(
        roles.target_before || roles.backup.is_some(),
        "Original host document backup absent; rollback refused"
    );
    let changed = !roles.target_before;
    choose_direction(root, name, event, RecoveryDirection::Rollback, d)?;
    if let Some(mut output) = roles.target.take() {
        if roles.target_before {
            roles.target = Some(output);
        } else {
            ensure!(roles.stage.is_none(), "Rollback stage occupant refused");
            let path = output_path(selected, &plan.id, event.attempt);
            output.rename_no_replace(&path, d)?;
            drop(output);
            let registration = token(access, plan, selected, event)?;
            roles.stage = io::open_registered_output(
                &path,
                event
                    .output_identity
                    .as_deref()
                    .context("Owned output absent")?,
                roles.backup.as_ref(),
                plan.content.as_bytes(),
                false,
                &registration,
                d,
            )?;
        }
    }
    if !roles.target_before {
        let mut before = roles
            .backup
            .take()
            .context("Original host document backup absent")?;
        original(access, plan, selected, &before, d)?;
        before.rename_no_replace(&selected.spec.path, d)?;
        roles.target = Some(before);
    }
    if let Some(stage) = roles.stage.take() {
        stage.discard(plan.content.as_bytes(), d)?;
    }
    ensure!(
        roles.backup.is_none(),
        "Rollback retained an original backup"
    );
    event.phase = Phase::RolledBack;
    append(root, name, event, d)?;
    Ok(receipt(
        plan,
        roles
            .target
            .as_ref()
            .context("Restored host document absent")?,
        "rolled_back",
        changed,
        true,
        true,
    ))
}
impl Access {
    pub fn recover(
        &self,
        authority: &WriteAuthority,
        approval: &RecoveryApproval,
        d: &Deadline,
    ) -> Result<Receipt> {
        ensure!(
            approval.binding == self.binding && valid_id(&approval.plan_id),
            "Explicit host document recovery approval required"
        );
        let guard = state(self, true, d)?;
        let mut loaded = load(self, &guard.root.path, &approval.plan_id, d)?;
        authority.check(self, &loaded.plan, d)?;
        ensure!(
            loaded.event.id == approval.event_id,
            "Stale host document recovery approval"
        );
        ensure!(
            loaded
                .event
                .direction
                .is_none_or(|mode| mode == approval.direction),
            "Host document recovery direction is already fixed"
        );
        let selected = self.check_plan(&loaded.plan, d)?;
        let _bindings = self.hold_bindings(&loaded.plan.document_id, d)?;
        require_owner(self, selected, d)?;
        let observed = roles(self, &loaded.plan, selected, &loaded.event, d)?;
        let name = stem(self, &loaded.plan.id)?;
        let transaction = Transaction {
            access: self,
            plan: &loaded.plan,
            selected,
            root: &guard.root.path,
            name: &name,
            d,
        };
        match approval.direction {
            RecoveryDirection::Resume => resume(&transaction, &mut loaded.event, observed),
            RecoveryDirection::Rollback => rollback(&transaction, &mut loaded.event, observed),
        }
    }
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    use std::fs;
    const ORIGINAL: &str = "\u{feff}# original e\u{301}\r\nvalue: 🦀\r\n";
    const PROPOSED: &str = "\u{feff}# explicit proposal\r\ncommand: <script>inert</script>\r\n";
    fn d() -> Deadline {
        Deadline::new(20_000)
    }
    struct Fixture {
        _temp: tempfile::TempDir,
        source: PathBuf,
        target: PathBuf,
        journal: PathBuf,
        config: Config,
        roots: Vec<ProjectRoot>,
        access: Access,
        authority: WriteAuthority,
        plan: Plan,
        approval: Approval,
    }
    fn fixture() -> Fixture {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("source");
        let journal = temp.path().join("private-journals");
        fs::create_dir(&source).unwrap();
        fs::create_dir(&journal).unwrap();
        fs::write(source.join("main.rs"), b"fn untouched() {}\r\n").unwrap();
        let target = temp.path().join("host-π.yaml");
        fs::write(&target, ORIGINAL).unwrap();
        let config = Config {
            documents: vec![DocumentSpec {
                id: "settings".into(),
                label: "Owned global settings".into(),
                path: target.clone(),
                scope: Scope::Global {},
                writable: true,
            }],
            journal_root: Some(journal.clone()),
            max_document_bytes: MAX_BYTES,
        };
        let roots = vec![ProjectRoot {
            name: "default".into(),
            path: source.clone(),
        }];
        let access = Access::open(&config, &roots, &d()).unwrap();
        let loaded = access.read("settings", &d()).unwrap();
        let plan = access
            .plan("settings", &loaded.revision, PROPOSED, &d())
            .unwrap();
        let authority = WriteAuthority::new(&access).unwrap();
        let approval = authority.approve(&access, &plan, &d()).unwrap();
        Fixture {
            _temp: temp,
            source,
            target,
            journal,
            config,
            roots,
            access,
            authority,
            plan,
            approval,
        }
    }
    fn verify_source(f: &Fixture) {
        assert_eq!(
            fs::read(f.source.join("main.rs")).unwrap(),
            b"fn untouched() {}\r\n"
        );
    }
    #[test]
    fn explicit_host_approval_commits_exact_bytes_and_keeps_source_and_security() {
        let f = fixture();
        let before = f.access.read("settings", &d()).unwrap();
        let receipt = f
            .access
            .apply(&f.authority, &f.approval, &f.plan, &d())
            .unwrap();
        assert_eq!(receipt.outcome, "committed");
        assert!(receipt.file_changed && receipt.metadata_changed);
        assert!(!receipt.source_written);
        assert_eq!(fs::read(&f.target).unwrap(), PROPOSED.as_bytes());
        let after = f.access.read("settings", &d()).unwrap();
        assert_eq!(after.security_sha256, before.security_sha256);
        assert_ne!(after.revision, before.revision);
        assert!(
            !f.target
                .parent()
                .unwrap()
                .join(format!(".yoke-doc-{}.original", f.plan.id))
                .exists()
        );
        assert!(!f.access.inspect_journal(&f.plan.id, &d()).unwrap().pending);
        verify_source(&f);
    }
    #[test]
    fn stale_or_mutated_plan_and_foreign_preparation_names_refuse_without_configuration_writes() {
        let f = fixture();
        let mut changed = f.plan.clone();
        changed.content = "unreviewed".into();
        changed.after_hash = hash(changed.content.as_bytes());
        assert!(
            f.access
                .apply(&f.authority, &f.approval, &changed, &d())
                .is_err()
        );
        assert_eq!(fs::read(&f.target).unwrap(), ORIGINAL.as_bytes());
        fs::write(&f.target, b"concurrent\r\n").unwrap();
        assert!(
            f.access
                .apply(&f.authority, &f.approval, &f.plan, &d())
                .is_err()
        );
        assert_eq!(fs::read(&f.target).unwrap(), b"concurrent\r\n");
        let f = fixture();
        let selected = &f.access.documents["settings"];
        let foreign = backup_path(selected, &f.plan.id);
        fs::write(&foreign, b"foreign backup occupant").unwrap();
        assert!(
            f.access
                .apply(&f.authority, &f.approval, &f.plan, &d())
                .is_err()
        );
        assert_eq!(fs::read(foreign).unwrap(), b"foreign backup occupant");
        assert_eq!(fs::read(&f.target).unwrap(), ORIGINAL.as_bytes());
        assert_eq!(fs::read_dir(&f.journal).unwrap().count(), 0);
        verify_source(&f);
    }
    #[test]
    fn interrupted_publication_reopens_missing_targets_and_resumes_each_owned_boundary() {
        for point in [
            "planned",
            "registered_before_retain",
            "prepared",
            "original_moved",
            "output_promoted",
            "published",
            "original_cleaned",
        ] {
            let f = fixture();
            STOP_AFTER.with(|slot| *slot.borrow_mut() = Some(point));
            assert!(
                f.access
                    .apply(&f.authority, &f.approval, &f.plan, &d())
                    .is_err(),
                "{point}"
            );
            let access = Access::open(&f.config, &f.roots, &d()).unwrap();
            let authority = WriteAuthority::new(&access).unwrap();
            let approval = authority
                .approve_recovery(&access, &f.plan.id, RecoveryDirection::Resume, &d())
                .unwrap();
            let receipt = access
                .recover(&authority, &approval, &d())
                .unwrap_or_else(|error| panic!("{point}: {error:#}"));
            assert_eq!(receipt.outcome, "committed");
            assert_eq!(fs::read(&f.target).unwrap(), PROPOSED.as_bytes());
            assert!(!access.inspect_journal(&f.plan.id, &d()).unwrap().pending);
            let again = authority
                .approve_recovery(&access, &f.plan.id, RecoveryDirection::Resume, &d())
                .unwrap();
            let replay = access.recover(&authority, &again, &d()).unwrap();
            assert!(!replay.file_changed && !replay.metadata_changed);
            assert_eq!(replay.journal_id.as_deref(), Some(f.plan.id.as_str()));
            verify_source(&f);
        }
    }
    #[test]
    fn explicit_rollback_restores_original_identity_and_never_uses_missing_backup_bytes() {
        for point in [
            "planned",
            "registered_before_retain",
            "prepared",
            "original_moved",
            "output_promoted",
            "published",
        ] {
            let f = fixture();
            STOP_AFTER.with(|slot| *slot.borrow_mut() = Some(point));
            assert!(
                f.access
                    .apply(&f.authority, &f.approval, &f.plan, &d())
                    .is_err()
            );
            let approval = f
                .authority
                .approve_recovery(&f.access, &f.plan.id, RecoveryDirection::Rollback, &d())
                .unwrap();
            let receipt = f
                .access
                .recover(&f.authority, &approval, &d())
                .unwrap_or_else(|error| panic!("{point}: {error:#}"));
            assert_eq!(receipt.outcome, "rolled_back");
            assert_eq!(receipt.document_identity, f.plan.before_identity);
            assert_eq!(fs::read(&f.target).unwrap(), ORIGINAL.as_bytes());
            let again = f
                .authority
                .approve_recovery(&f.access, &f.plan.id, RecoveryDirection::Rollback, &d())
                .unwrap();
            let replay = f.access.recover(&f.authority, &again, &d()).unwrap();
            assert!(!replay.file_changed && !replay.metadata_changed);
            assert!(
                f.access
                    .pending_journals("settings", &d())
                    .unwrap()
                    .is_empty()
            );
            verify_source(&f);
        }
        let f = fixture();
        STOP_AFTER.with(|slot| *slot.borrow_mut() = Some("original_cleaned"));
        assert!(
            f.access
                .apply(&f.authority, &f.approval, &f.plan, &d())
                .is_err()
        );
        let approval = f
            .authority
            .approve_recovery(&f.access, &f.plan.id, RecoveryDirection::Rollback, &d())
            .unwrap();
        assert!(f.access.recover(&f.authority, &approval, &d()).is_err());
        assert_eq!(fs::read(&f.target).unwrap(), PROPOSED.as_bytes());
    }
    #[test]
    fn recovery_direction_and_current_event_approval_cannot_be_replayed_or_reversed() {
        let f = fixture();
        STOP_AFTER.with(|slot| *slot.borrow_mut() = Some("prepared"));
        assert!(
            f.access
                .apply(&f.authority, &f.approval, &f.plan, &d())
                .is_err()
        );
        let old = f
            .authority
            .approve_recovery(&f.access, &f.plan.id, RecoveryDirection::Resume, &d())
            .unwrap();
        STOP_AFTER.with(|slot| *slot.borrow_mut() = Some("recovery_direction"));
        assert!(f.access.recover(&f.authority, &old, &d()).is_err());
        assert!(
            f.authority
                .approve_recovery(&f.access, &f.plan.id, RecoveryDirection::Rollback, &d())
                .is_err()
        );
        assert!(f.access.recover(&f.authority, &old, &d()).is_err());
        let current = f
            .authority
            .approve_recovery(&f.access, &f.plan.id, RecoveryDirection::Resume, &d())
            .unwrap();
        f.access.recover(&f.authority, &current, &d()).unwrap();
        assert_eq!(fs::read(&f.target).unwrap(), PROPOSED.as_bytes());
    }
    #[test]
    fn pending_journal_blocks_a_fresh_plan_until_explicit_recovery() {
        let f = fixture();
        STOP_AFTER.with(|slot| *slot.borrow_mut() = Some("planned"));
        assert!(
            f.access
                .apply(&f.authority, &f.approval, &f.plan, &d())
                .is_err()
        );
        let view = f.access.inspect_journal(&f.plan.id, &d()).unwrap();
        assert_eq!(view.phase, "planned");
        assert_eq!(view.sequence, 0);
        assert!(view.pending);
        let listed = f.access.pending_journals("settings", &d()).unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].event_id, view.event_id);
        assert!(
            f.authority
                .approve_recovery_at(
                    &f.access,
                    &f.plan.id,
                    &"0".repeat(64),
                    RecoveryDirection::Resume,
                    &d()
                )
                .is_err()
        );
        let snapshot = f.access.read("settings", &d()).unwrap();
        let fresh = f
            .access
            .plan("settings", &snapshot.revision, "second proposal\r\n", &d())
            .unwrap();
        let approval = f.authority.approve(&f.access, &fresh, &d()).unwrap();
        assert!(
            f.access
                .apply(&f.authority, &approval, &fresh, &d())
                .is_err()
        );
        assert_eq!(fs::read(&f.target).unwrap(), ORIGINAL.as_bytes());
        assert!(!plan_path(&f.journal, &stem(&f.access, &fresh.id).unwrap()).exists());
        let recovery = f
            .authority
            .approve_recovery(&f.access, &f.plan.id, RecoveryDirection::Rollback, &d())
            .unwrap();
        f.access.recover(&f.authority, &recovery, &d()).unwrap();
        f.access
            .apply(&f.authority, &approval, &fresh, &d())
            .unwrap();
        assert_eq!(fs::read(&f.target).unwrap(), b"second proposal\r\n");
        verify_source(&f);
    }
    fn interrupt(f: &Fixture, point: &'static str) {
        STOP_AFTER.with(|slot| *slot.borrow_mut() = Some(point));
        assert!(
            f.access
                .apply(&f.authority, &f.approval, &f.plan, &d())
                .is_err()
        );
    }
    #[test]
    fn foreign_target_stage_or_backup_preserves_foreign_bytes_and_event_head() {
        for role in ["target", "stage", "backup"] {
            let f = fixture();
            interrupt(&f, "prepared");
            let selected = &f.access.documents["settings"];
            let path = match role {
                "target" => f.target.clone(),
                "stage" => output_path(selected, &f.plan.id, 0),
                _ => backup_path(selected, &f.plan.id),
            };
            if path.exists() {
                fs::rename(&path, path.with_extension("held-original")).unwrap();
            }
            fs::write(&path, b"foreign occupant must survive").unwrap();
            let head = f.access.inspect_journal(&f.plan.id, &d()).unwrap().event_id;
            for mode in [RecoveryDirection::Resume, RecoveryDirection::Rollback] {
                let approval = f
                    .authority
                    .approve_recovery(&f.access, &f.plan.id, mode, &d())
                    .unwrap();
                assert!(
                    f.access.recover(&f.authority, &approval, &d()).is_err(),
                    "{role}"
                );
                assert_eq!(fs::read(&path).unwrap(), b"foreign occupant must survive");
                assert_eq!(
                    f.access.inspect_journal(&f.plan.id, &d()).unwrap().event_id,
                    head
                );
            }
            verify_source(&f);
        }
    }
    #[test]
    fn journal_lineage_direction_and_header_tampering_refuse_without_target_changes() {
        for change in [
            "unknown_header",
            "gap",
            "previous",
            "phase",
            "direction",
            "security",
        ] {
            let f = fixture();
            interrupt(&f, "prepared");
            let name = stem(&f.access, &f.plan.id).unwrap();
            if change == "unknown_header" {
                let path = plan_path(&f.journal, &name);
                let mut value: serde_json::Value =
                    serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
                value["authority"] = serde_json::json!(true);
                fs::write(path, serde_json::to_vec(&value).unwrap()).unwrap();
            } else if change == "gap" {
                fs::rename(
                    event_path(&f.journal, &name, 1),
                    event_path(&f.journal, &name, 20),
                )
                .unwrap();
            } else {
                let path = event_path(&f.journal, &name, 2);
                let mut event: Event = canonical(&path, MAX_EVENT, &d()).unwrap();
                match change {
                    "previous" => event.previous = Some("0".repeat(64)),
                    "phase" => event.phase = Phase::Committed,
                    "direction" => event.direction = Some(RecoveryDirection::Rollback),
                    _ => event.output_security_hash = Some("0".repeat(64)),
                }
                event.id = event_id(&event);
                fs::write(path, serde_json::to_vec(&event).unwrap()).unwrap();
            }
            assert!(
                f.access.inspect_journal(&f.plan.id, &d()).is_err(),
                "{change}"
            );
            assert!(
                f.authority
                    .approve_recovery(&f.access, &f.plan.id, RecoveryDirection::Resume, &d())
                    .is_err()
            );
            assert_eq!(fs::read(&f.target).unwrap(), ORIGINAL.as_bytes());
            verify_source(&f);
        }
    }
    #[test]
    fn no_op_and_exhausted_budget_never_change_configuration_or_create_a_transaction() {
        let f = fixture();
        let snapshot = f.access.read("settings", &d()).unwrap();
        let plan = f
            .access
            .plan("settings", &snapshot.revision, ORIGINAL, &d())
            .unwrap();
        let approval = f.authority.approve(&f.access, &plan, &d()).unwrap();
        let result = f
            .access
            .apply(&f.authority, &approval, &plan, &d())
            .unwrap();
        assert!(!result.file_changed && !result.metadata_changed);
        assert_eq!(fs::read_dir(&f.journal).unwrap().count(), 0);
        let unrelated = f.journal.join("unrelated-reserved-file");
        fs::File::create(&unrelated)
            .unwrap()
            .set_len(MAX_STATE_BYTES)
            .unwrap();
        assert!(
            f.access
                .apply(&f.authority, &f.approval, &f.plan, &d())
                .is_err()
        );
        assert_eq!(fs::read(&f.target).unwrap(), ORIGINAL.as_bytes());
        assert_eq!(fs::metadata(unrelated).unwrap().len(), MAX_STATE_BYTES);
        assert_eq!(fs::read_dir(&f.journal).unwrap().count(), 1);
        verify_source(&f);
    }
    #[test]
    fn recovery_rejects_replaced_analyzed_roots_before_event_or_configuration_mutation() {
        let f = fixture();
        interrupt(&f, "prepared");
        let head = f.access.inspect_journal(&f.plan.id, &d()).unwrap().event_id;
        fs::rename(&f.source, f.source.with_extension("original")).unwrap();
        fs::create_dir(&f.source).unwrap();
        fs::write(f.source.join("main.rs"), b"foreign source").unwrap();
        let approval = f
            .authority
            .approve_recovery(&f.access, &f.plan.id, RecoveryDirection::Resume, &d())
            .unwrap();
        assert!(f.access.recover(&f.authority, &approval, &d()).is_err());
        assert_eq!(
            f.access.inspect_journal(&f.plan.id, &d()).unwrap().event_id,
            head
        );
        assert_eq!(fs::read(&f.target).unwrap(), ORIGINAL.as_bytes());
        assert_eq!(
            fs::read(f.source.join("main.rs")).unwrap(),
            b"foreign source"
        );
    }
    #[test]
    fn changed_host_selection_or_journal_root_cannot_bypass_a_targets_original_owner() {
        for change in ["label", "id", "maximum", "journal"] {
            let f = fixture();
            interrupt(&f, "prepared");
            let mut config = f.config.clone();
            match change {
                "label" => config.documents[0].label = "Changed label".into(),
                "id" => config.documents[0].id = "renamed".into(),
                "maximum" => config.max_document_bytes = 32000,
                _ => {
                    let other = f._temp.path().join("different-journal");
                    fs::create_dir(&other).unwrap();
                    config.journal_root = Some(other)
                }
            }
            let id = config.documents[0].id.clone();
            let access = Access::open(&config, &f.roots, &d()).unwrap();
            assert_ne!(access.binding(), f.access.binding());
            assert_eq!(
                access.owner_binding(&id, &d()).unwrap().as_deref(),
                Some(f.access.binding())
            );
            let snapshot = access.read(&id, &d()).unwrap();
            let plan = access
                .plan(
                    &id,
                    &snapshot.revision,
                    "must not bypass recovery\r\n",
                    &d(),
                )
                .unwrap();
            let authority = WriteAuthority::new(&access).unwrap();
            let approval = authority.approve(&access, &plan, &d()).unwrap();
            assert!(
                access.apply(&authority, &approval, &plan, &d()).is_err(),
                "{change}"
            );
            assert_eq!(fs::read(&f.target).unwrap(), ORIGINAL.as_bytes());
            let approval = f
                .authority
                .approve_recovery(&f.access, &f.plan.id, RecoveryDirection::Resume, &d())
                .unwrap();
            f.access.recover(&f.authority, &approval, &d()).unwrap();
            assert_eq!(fs::read(&f.target).unwrap(), PROPOSED.as_bytes());
            // Completion does not silently migrate a pinned namespace either.
            let snapshot = access.read(&id, &d()).unwrap();
            let plan = access
                .plan(&id, &snapshot.revision, "new authority\r\n", &d())
                .unwrap();
            let approval = authority.approve(&access, &plan, &d()).unwrap();
            assert!(access.apply(&authority, &approval, &plan, &d()).is_err());
            verify_source(&f);
        }
    }
    #[test]
    fn interruption_after_owner_binding_and_before_plan_allows_only_the_same_authority_to_retry() {
        let f = fixture();
        interrupt(&f, "owner_bound");
        assert_eq!(fs::read(&f.target).unwrap(), ORIGINAL.as_bytes());
        assert_eq!(
            f.access.owner_binding("settings", &d()).unwrap().as_deref(),
            Some(f.access.binding())
        );
        assert!(!plan_path(&f.journal, &stem(&f.access, &f.plan.id).unwrap()).exists());
        f.access
            .apply(&f.authority, &f.approval, &f.plan, &d())
            .unwrap();
        assert_eq!(fs::read(&f.target).unwrap(), PROPOSED.as_bytes());
        verify_source(&f);
    }
    #[test]
    fn foreign_owner_metadata_is_never_overwritten_or_cleaned_up() {
        let f = fixture();
        let selected = &f.access.documents["settings"];
        let path = owner_path(selected).unwrap();
        fs::write(&path, b"foreign metadata occupant").unwrap();
        assert!(
            f.access
                .apply(&f.authority, &f.approval, &f.plan, &d())
                .is_err()
        );
        assert_eq!(fs::read(path).unwrap(), b"foreign metadata occupant");
        assert_eq!(fs::read(&f.target).unwrap(), ORIGINAL.as_bytes());
        verify_source(&f);
    }
    #[test]
    fn project_files_do_not_obtain_workspace_write_approval_from_host_document_json() {
        let f = fixture();
        let project = f.source.join("project.yaml");
        fs::write(&project, b"project: original\n").unwrap();
        let config = Config {
            documents: vec![DocumentSpec {
                id: "project".into(),
                label: "Project configuration".into(),
                path: project.clone(),
                scope: Scope::Project {
                    project: "default".into(),
                },
                writable: true,
            }],
            journal_root: Some(f.journal.clone()),
            max_document_bytes: MAX_BYTES,
        };
        let access = Access::open(&config, &f.roots, &d()).unwrap();
        assert!(WriteAuthority::new(&access).is_err());
        assert_eq!(fs::read(project).unwrap(), b"project: original\n");
        assert_eq!(fs::read_dir(&f.journal).unwrap().count(), 0);
        verify_source(&f);
    }
}
