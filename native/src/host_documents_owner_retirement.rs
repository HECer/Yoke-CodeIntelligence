//! Explicit, single-authority logical retirement. The fixed owner entry is never removed.
//! A retirement is a host write-eligibility decision, not evidence that all hosts exited.
use super::*;

const MAX_RETIREMENT: usize = 32_000;
const MAX_RETIREMENT_EVENTS: u32 = 16;

/// Format 3 is deliberately unreadable by ordinary format-1/2 owner readers,
/// including older binaries. Retain the original optional generation exactly.
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RetiredOwner {
    schema_version: u32,
    id: String,
    target: String,
    access_binding: String,
    journal_identity: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    generation: Option<String>,
    original_owner_id: String,
    retirement_nonce: String,
}
fn retired_id(owner: &RetiredOwner) -> String {
    let mut copy = owner.clone();
    copy.id.clear();
    identity(&("host_document_retired_owner_v1", copy))
}
fn retired_owner(before: &OwnerBinding, nonce: &str) -> RetiredOwner {
    let mut after = RetiredOwner {
        schema_version: 3,
        id: String::new(),
        target: before.target.clone(),
        access_binding: before.access_binding.clone(),
        journal_identity: before.journal_identity.clone(),
        generation: before.generation.clone(),
        original_owner_id: before.id.clone(),
        retirement_nonce: nonce.into(),
    };
    after.id = retired_id(&after);
    after
}
fn check_retired_owner(owner: &RetiredOwner, target: &str) -> Result<()> {
    let mut before = OwnerBinding {
        schema_version: if owner.generation.is_some() { 2 } else { 1 },
        id: String::new(),
        target: owner.target.clone(),
        access_binding: owner.access_binding.clone(),
        journal_identity: owner.journal_identity.clone(),
        generation: owner.generation.clone(),
    };
    before.id = owner_id(&before);
    ensure!(
        owner.schema_version == 3
            && owner.id == retired_id(owner)
            && owner.target == target
            && valid_id(&owner.access_binding)
            && !owner.journal_identity.is_empty()
            && owner.journal_identity.len() <= 128
            && owner.generation.as_deref().is_none_or(valid_id)
            && owner.original_owner_id == before.id
            && valid_id(&owner.retirement_nonce),
        "Invalid retired host document owner metadata; no adoption or cleanup"
    );
    Ok(())
}
impl Access {
    /// Presence blocks writes; it never grants a serialized record authority.
    /// Without a marker, unknown/partial metadata is an error, not an unblocked result.
    pub fn owner_retirement_blocked(&self, document: &str, d: &Deadline) -> Result<bool> {
        let _bindings = self.hold_bindings(document, d)?;
        let selected = &self.documents[document];
        if selected.source_project.is_some() {
            return Ok(false);
        }
        if exists(&marker_path(selected)?, d)? {
            return Ok(true);
        }
        let Some(file) = io::read_file(&owner_path(selected)?, 2048, false, d)? else {
            return Ok(false);
        };
        if let Ok(retired) = serde_json::from_slice::<RetiredOwner>(&file.bytes) {
            ensure!(
                serde_json::to_vec(&retired)? == file.bytes && file.path == owner_path(selected)?,
                "Noncanonical retired host document owner metadata"
            );
            check_retired_owner(&retired, &target_key(selected)?)?;
            return Ok(true);
        }
        // Preserve the ordinary reader's complete schema/identity checks.
        read_owner(selected, d)?;
        Ok(false)
    }
}
fn marker_path(selected: &BoundDocument) -> Result<PathBuf> {
    Ok(selected.parent_path.join(format!(
        ".yoke-doc-owner-retirement-{}.pending.json",
        target_key(selected)?
    )))
}
pub(super) fn require_no_pending(selected: &BoundDocument, d: &Deadline) -> Result<()> {
    ensure!(
        !exists(&marker_path(selected)?, d)?,
        "Host document owner retirement is pending; use explicit host retirement recovery"
    );
    Ok(())
}
fn require_no_other_pending(selected: &BoundDocument, d: &Deadline) -> Result<()> {
    ensure!(
        !binding_migration::pending_metadata_present(selected, d)?
            && !binding_migration::history_pending(selected, d)?,
        "Binding migration or history archival is pending; preserve its original recovery workflow"
    );
    Ok(())
}
fn selected<'a>(access: &'a Access, document: &str) -> Result<&'a BoundDocument> {
    let selected = access
        .documents
        .get(document)
        .context("Original host selection required")?;
    ensure!(
        selected.spec.writable
            && selected.source_project.is_none()
            && matches!(selected.spec.scope, Scope::Global {})
            && access.journal.is_some(),
        "Owner retirement requires an original writable global selection and journal root"
    );
    Ok(selected)
}
fn hold_selection(
    access: &Access,
    document: &str,
    d: &Deadline,
) -> Result<Vec<io::GuardedDirectory>> {
    let mut held = access.hold_bindings(document, d)?;
    let mut identities = held
        .iter()
        .map(|directory| directory.identity.clone())
        .collect::<BTreeSet<_>>();
    for selected in access.documents.values() {
        let parent = io::guarded_directory(&selected.parent_path, d)?;
        ensure!(
            parent.identity == selected.parent_identity && parent.path == selected.parent_path,
            "Complete original host document selection parent changed"
        );
        if identities.insert(parent.identity.clone()) {
            held.push(parent);
        }
    }
    Ok(held)
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OwnerRetirementPlan {
    pub schema_version: u32,
    pub id: String,
    pub document_id: String,
    pub access_binding: String,
    pub target: String,
    pub target_path: String,
    pub source_identity: String,
    pub source_hash: String,
    pub source_security_hash: String,
    pub source_revision: String,
    pub owner_identity: String,
    pub owner_security_hash: String,
    before_owner: OwnerBinding,
    after_owner: RetiredOwner,
    pub journal_state: String,
    pub nonce: String,
}
fn plan_id(plan: &OwnerRetirementPlan) -> String {
    let mut copy = plan.clone();
    copy.id.clear();
    identity(&("host_document_owner_retirement_v1", copy))
}
fn check_plan(access: &Access, p: &OwnerRetirementPlan, d: &Deadline) -> Result<()> {
    check_record_plan(p, d)?;
    let a = selected(access, &p.document_id)?;
    ensure!(
        p.access_binding == access.binding
            && p.target == target_key(a)?
            && binding_migration::matches_selection(&p.before_owner, &expected_owner(access, a)?),
        "Original owner retirement selection, target or journal binding differs"
    );
    Ok(())
}
fn check_record_plan(p: &OwnerRetirementPlan, d: &Deadline) -> Result<()> {
    d.check()?;
    crate::project_session::label(&p.document_id)?;
    ensure!(
        p.schema_version == 1
            && p.id == plan_id(p)
            && valid_id(&p.access_binding)
            && valid_id(&p.target)
            && p.before_owner.target == p.target
            && p.before_owner.access_binding == p.access_binding
            && !p.before_owner.journal_identity.is_empty()
            && p.before_owner.journal_identity.len() <= 128
            && p.before_owner.id == owner_id(&p.before_owner)
            && ((p.before_owner.schema_version == 1 && p.before_owner.generation.is_none())
                || (p.before_owner.schema_version == 2
                    && p.before_owner.generation.as_deref().is_some_and(valid_id)))
            && serde_json::to_vec(&p.after_owner)?
                == serde_json::to_vec(&retired_owner(&p.before_owner, &p.nonce))?
            && [
                &p.id,
                &p.source_hash,
                &p.source_security_hash,
                &p.source_revision,
                &p.owner_security_hash,
                &p.journal_state,
                &p.nonce,
            ]
            .into_iter()
            .all(|v| valid_id(v))
            && !p.source_identity.is_empty()
            && p.source_identity.len() <= 128
            && !p.owner_identity.is_empty()
            && p.owner_identity.len() <= 128
            && p.target_path.len() <= MAX_RETIREMENT
            && Path::new(&p.target_path).is_absolute()
            && !p.target_path.chars().any(char::is_control)
            && serde_json::to_vec(p)?.len() <= MAX_RETIREMENT
            && serde_json::to_vec(&RetirementHeader {
                schema_version: 1,
                plan: p.clone(),
                initial: initial(p)
            })?
            .len()
                <= MAX_RETIREMENT
            && serde_json::to_vec(&p.after_owner)?.len() <= 2048,
        "Invalid reviewed host document owner retirement"
    );
    Ok(())
}
fn check_authority(
    authority: &WriteAuthority,
    access: &Access,
    p: &OwnerRetirementPlan,
    d: &Deadline,
) -> Result<()> {
    check_plan(access, p, d)?;
    ensure!(
        authority.binding == access.binding && authority.documents.contains(&p.document_id),
        "Original independent host document write authority required for retirement"
    );
    Ok(())
}
fn check_source(
    access: &Access,
    p: &OwnerRetirementPlan,
    file: &io::HeldFile,
    d: &Deadline,
) -> Result<()> {
    let a = selected(access, &p.document_id)?;
    let security = io::security_fingerprint(file, d)?;
    std::str::from_utf8(&file.bytes).context("Selected global document is not UTF-8")?;
    ensure!(
        file.identity == p.source_identity
            && file.hash == p.source_hash
            && security == p.source_security_hash
            && file.path.to_str() == Some(p.target_path.as_str())
            && file.byte_length <= access.max_bytes
            && access.revision(a, file, &security) == p.source_revision,
        "Source identity, bytes or security changed since the retirement review"
    );
    Ok(())
}
fn retirement_stem(p: &OwnerRetirementPlan) -> String {
    format!("yoke-owner-retirement-{}", p.id)
}
fn complete_path(a: &BoundDocument, p: &OwnerRetirementPlan) -> PathBuf {
    a.parent_path
        .join(format!(".yoke-doc-owner-retirement-{}.complete.json", p.id))
}
fn check_journal_name(name: &str) -> Result<()> {
    let (rest, cap) = if let Some(rest) = name.strip_prefix("yoke-doc-") {
        let (binding, rest) = rest
            .split_once('-')
            .context("Unknown document journal namespace")?;
        ensure!(
            valid_id(binding),
            "Unknown document journal selection namespace"
        );
        (rest, MAX_EVENTS)
    } else if let Some(rest) = name.strip_prefix("yoke-binding-migration-") {
        (rest, MAX_RETIREMENT_EVENTS)
    } else {
        (
            name.strip_prefix("yoke-owner-retirement-")
                .context("Unknown retirement journal evidence namespace")?,
            MAX_RETIREMENT_EVENTS,
        )
    };
    let (id, suffix) = rest
        .split_once('.')
        .context("Unknown retirement journal evidence role")?;
    ensure!(
        valid_id(id)
            && (suffix == "plan.json" || (1..cap).any(|n| suffix == format!("e{n:04}.json"))),
        "Unknown or out-of-capacity retirement journal evidence role"
    );
    Ok(())
}

/// Replay every record in this selection's namespace, including orphan events.
/// Other retained evidence is observed by exact bytes/identity/security, never
/// interpreted as authority or as proof that every live host released a target.
fn journal_state(
    access: &Access,
    document: &str,
    root: &Path,
    ignore: Option<&OwnerRetirementPlan>,
    d: &Deadline,
) -> Result<String> {
    let directory = io::open_metadata_directory(root, d)?;
    let names = directory.names(MAX_ENTRIES, d)?;
    let prefix = format!("yoke-doc-{}-", access.binding);
    let ignored = ignore.map(|p| format!("{}.", retirement_stem(p)));
    let mut ids = BTreeSet::new();
    let mut retirements = BTreeSet::new();
    let mut rows = Vec::new();
    let mut bytes = 0usize;
    for name in names {
        d.check()?;
        if name == ".yoke-documents.lock"
            || ignored
                .as_ref()
                .is_some_and(|prefix| name.starts_with(prefix))
        {
            continue;
        }
        // Private journal roots contain evidence only. Unknown entries cannot
        // silently become an empty complete inventory, and temporary occupants
        // are preserved for their original workflow rather than cleaned here.
        ensure!(
            name.is_ascii()
                && !name.bytes().any(|b| b.is_ascii_uppercase())
                && name.starts_with("yoke-")
                && name.ends_with(".json"),
            "Unavailable or unknown retirement journal evidence"
        );
        check_journal_name(&name)?;
        let file = io::read_file(&root.join(&name), MAX_PLAN, false, d)?
            .context("Retirement journal evidence disappeared")?;
        let envelope = crate::json::parse(&file.bytes, MAX_PLAN)?;
        ensure!(
            envelope
                .get("schemaVersion")
                .and_then(serde_json::Value::as_u64)
                == Some(1),
            "Unsupported retained journal evidence schema"
        );
        d.check()?;
        bytes = bytes
            .checked_add(file.byte_length)
            .context("Retirement evidence byte overflow")?;
        ensure!(
            bytes as u64 <= MAX_STATE_BYTES,
            "Retirement journal evidence byte capacity exceeded"
        );
        rows.push((
            name.clone(),
            file.identity.clone(),
            file.hash.clone(),
            io::security_fingerprint(&file, d)?,
        ));
        if let Some(rest) = name.strip_prefix(&prefix) {
            let (id, suffix) = rest
                .split_once('.')
                .context("Invalid retirement journal record name")?;
            ensure!(
                valid_id(id)
                    && (suffix == "plan.json"
                        || (1..MAX_EVENTS).any(|n| suffix == format!("e{n:04}.json"))),
                "Unknown or out-of-capacity document journal role"
            );
            ids.insert(id.to_owned());
        }
        if let Some(rest) = name.strip_prefix("yoke-owner-retirement-") {
            let (id, suffix) = rest
                .split_once('.')
                .context("Invalid owner retirement record name")?;
            ensure!(
                valid_id(id)
                    && (suffix == "plan.json"
                        || (1..MAX_RETIREMENT_EVENTS).any(|n| suffix == format!("e{n:04}.json"))),
                "Unknown owner retirement journal role"
            );
            retirements.insert(id.to_owned());
        }
    }
    let a = selected(access, document)?;
    for id in ids {
        let loaded = load(access, root, &id, d)?;
        if loaded.plan.document_id == document {
            ensure!(
                matches!(loaded.event.phase, Phase::Committed | Phase::RolledBack),
                "Unfinished host document journal prevents owner retirement"
            );
            ensure!(
                !exists(&backup_path(a, &id), d)?,
                "Occupied original-backup role prevents owner retirement"
            );
            for attempt in 0..3 {
                ensure!(
                    !exists(&output_path(a, &id, attempt), d)?,
                    "Occupied prepared-output role prevents owner retirement"
                );
            }
        }
    }
    for id in retirements {
        let h: RetirementHeader = canonical(
            &plan_path(root, &format!("yoke-owner-retirement-{id}")),
            MAX_RETIREMENT,
            d,
        )?;
        ensure!(
            h.plan.id == id && h.plan.id == plan_id(&h.plan),
            "Foreign owner retirement header binding"
        );
        let e = load_record_event(&h.plan, root, d)?;
        if h.plan.target == target_key(a)? {
            ensure!(
                terminal(&e),
                "Unfinished owner retirement intent prevents a second retirement"
            );
        }
    }
    // An orphan stage has no trustworthy target association. Refuse rather than
    // adopt/delete it or infer safety from an absent header.
    let parent = io::open_metadata_directory(&a.parent_path, d)?;
    let parent_names = parent.names(MAX_ENTRIES, d)?;
    ensure!(
        parent_names.len() + 2 <= MAX_ENTRIES,
        "Owner retirement parent entry reserve exceeded"
    );
    for name in parent_names {
        let name = if cfg!(windows) {
            name.to_ascii_lowercase()
        } else {
            name
        };
        ensure!(
            !(name.starts_with(".yoke-doc-")
                && (name.ends_with(".new") || name.ends_with(".original"))),
            "Unattributed host document output or backup prevents owner retirement"
        );
    }
    rows.sort();
    Ok(identity(&(
        "host_document_retirement_journal_state_v1",
        rows,
    )))
}
pub fn plan_owner_retirement(
    access: &Access,
    document: &str,
    d: &Deadline,
) -> Result<OwnerRetirementPlan> {
    let a = selected(access, document)?;
    let _bindings = hold_selection(access, document, d)?;
    binding_migration::require_no_pending(a, d)?;
    require_owner(access, a, d)?;
    let (_, source) = access.current(document, d)?;
    let file =
        io::read_file(&owner_path(a)?, 2048, false, d)?.context("Original owner binding absent")?;
    ensure!(
        file.path == owner_path(a)?,
        "Owner retirement requires its exact fixed metadata name"
    );
    let before: OwnerBinding = canonical_bytes(&file.bytes, 2048, d)?;
    let security = io::security_fingerprint(&source, d)?;
    let root = state(access, false, d)?;
    budget(&root.root.path, d)?;
    let nonce = nonce()?;
    let mut p = OwnerRetirementPlan {
        schema_version: 1,
        id: String::new(),
        document_id: document.into(),
        access_binding: access.binding.clone(),
        target: target_key(a)?,
        target_path: source
            .path
            .to_str()
            .context("Unicode target required")?
            .into(),
        source_identity: source.identity.clone(),
        source_hash: source.hash.clone(),
        source_security_hash: security.clone(),
        source_revision: access.revision(a, &source, &security),
        owner_identity: file.identity.clone(),
        owner_security_hash: io::security_fingerprint(&file, d)?,
        after_owner: retired_owner(&before, &nonce),
        before_owner: before,
        journal_state: journal_state(access, document, &root.root.path, None, d)?,
        nonce,
    };
    p.id = plan_id(&p);
    check_plan(access, &p, d)?;
    check_source(access, &p, &source, d)?;
    Ok(p)
}

/// A concrete host review, never deserialized from input or a confirmation flag.
pub struct OwnerRetirementApproval {
    plan_id: String,
    binding: String,
}
impl WriteAuthority {
    pub fn approve_owner_retirement(
        &self,
        access: &Access,
        p: &OwnerRetirementPlan,
        d: &Deadline,
    ) -> Result<OwnerRetirementApproval> {
        check_authority(self, access, p, d)?;
        Ok(OwnerRetirementApproval {
            plan_id: p.id.clone(),
            binding: access.binding.clone(),
        })
    }
}
#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum RetirementPhase {
    Prepared,
    Writing,
    Committed,
    RolledBack,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RetirementEvent {
    schema_version: u32,
    id: String,
    plan_id: String,
    sequence: u32,
    previous: Option<String>,
    phase: RetirementPhase,
    direction: Option<RecoveryDirection>,
}
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RetirementHeader {
    schema_version: u32,
    plan: OwnerRetirementPlan,
    initial: RetirementEvent,
}
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Marker {
    schema_version: u32,
    plan_id: String,
    target: String,
    owner_identity: String,
}
fn marker(p: &OwnerRetirementPlan) -> Marker {
    Marker {
        schema_version: 1,
        plan_id: p.id.clone(),
        target: p.target.clone(),
        owner_identity: p.owner_identity.clone(),
    }
}
fn event_id(e: &RetirementEvent) -> String {
    let mut copy = e.clone();
    copy.id.clear();
    identity(&("host_document_owner_retirement_event_v1", copy))
}
fn initial(p: &OwnerRetirementPlan) -> RetirementEvent {
    let mut e = RetirementEvent {
        schema_version: 1,
        id: String::new(),
        plan_id: p.id.clone(),
        sequence: 0,
        previous: None,
        phase: RetirementPhase::Prepared,
        direction: None,
    };
    e.id = event_id(&e);
    e
}
fn valid_transition(before: &RetirementEvent, after: &RetirementEvent) -> bool {
    if before
        .direction
        .is_some_and(|fixed| after.direction != Some(fixed))
    {
        return false;
    }
    match (before.phase, after.phase) {
        (RetirementPhase::Prepared, RetirementPhase::Writing) => true,
        (RetirementPhase::Writing, RetirementPhase::Writing) => after.direction.is_some(),
        (RetirementPhase::Writing, RetirementPhase::Committed) => {
            after.direction != Some(RecoveryDirection::Rollback)
        }
        (RetirementPhase::Writing, RetirementPhase::RolledBack) => {
            after.direction == Some(RecoveryDirection::Rollback)
        }
        _ => false,
    }
}
fn next(
    root: &Path,
    p: &OwnerRetirementPlan,
    e: &mut RetirementEvent,
    phase: RetirementPhase,
    direction: Option<RecoveryDirection>,
    d: &Deadline,
) -> Result<()> {
    ensure!(
        e.sequence + 1 < MAX_RETIREMENT_EVENTS,
        "Owner retirement event capacity refused"
    );
    let mut n = e.clone();
    n.sequence += 1;
    n.previous = Some(e.id.clone());
    n.phase = phase;
    n.direction = direction;
    ensure!(
        valid_transition(e, &n),
        "Invalid owner retirement transition or fixed direction"
    );
    n.id = event_id(&n);
    write_record(
        root,
        &event_path(root, &retirement_stem(p), n.sequence),
        &serde_json::to_vec(&n)?,
        d,
    )?;
    *e = n;
    Ok(())
}
fn load_event(
    access: &Access,
    p: &OwnerRetirementPlan,
    root: &Path,
    d: &Deadline,
) -> Result<RetirementEvent> {
    check_plan(access, p, d)?;
    load_record_event(p, root, d)
}
fn load_record_event(
    p: &OwnerRetirementPlan,
    root: &Path,
    d: &Deadline,
) -> Result<RetirementEvent> {
    check_record_plan(p, d)?;
    let stem = retirement_stem(p);
    let directory = io::open_metadata_directory(root, d)?;
    for name in directory.names(MAX_ENTRIES, d)? {
        if name.starts_with(&format!("{stem}.")) && name != format!("{stem}.plan.json") {
            ensure!(
                (1..MAX_RETIREMENT_EVENTS).any(|n| name == format!("{stem}.e{n:04}.json")),
                "Unknown or out-of-capacity owner retirement event"
            );
        }
    }
    let h: RetirementHeader = canonical(&plan_path(root, &stem), MAX_RETIREMENT, d)?;
    let mut e = initial(p);
    ensure!(
        h.schema_version == 1
            && serde_json::to_vec(&h.plan)? == serde_json::to_vec(p)?
            && serde_json::to_vec(&h.initial)? == serde_json::to_vec(&e)?,
        "Owner retirement header differs"
    );
    let mut ended = false;
    for sequence in 1..MAX_RETIREMENT_EVENTS {
        let path = event_path(root, &stem, sequence);
        if !exists(&path, d)? {
            ended = true;
            continue;
        }
        ensure!(!ended, "Owner retirement event sequence has a gap");
        let n: RetirementEvent = canonical(&path, MAX_EVENT, d)?;
        ensure!(
            n.schema_version == 1
                && n.plan_id == p.id
                && n.sequence == sequence
                && n.previous.as_ref() == Some(&e.id)
                && n.id == event_id(&n)
                && valid_transition(&e, &n),
            "Invalid owner retirement event chain"
        );
        e = n;
    }
    Ok(e)
}
fn terminal(e: &RetirementEvent) -> bool {
    matches!(
        e.phase,
        RetirementPhase::Committed | RetirementPhase::RolledBack
    )
}
fn phase(e: &RetirementEvent) -> Result<String> {
    Ok(serde_json::to_value(e.phase)?
        .as_str()
        .context("Owner retirement phase")?
        .into())
}
fn held_owner(
    access: &Access,
    p: &OwnerRetirementPlan,
    d: &Deadline,
) -> Result<(io::HeldFile, io::DocumentOwnerMetadata)> {
    let a = selected(access, &p.document_id)?;
    let (_, source) = access.hold_current(&p.document_id, false, d)?;
    check_source(access, p, &source, d)?;
    let owner = io::open_document_owner_metadata(&owner_path(a)?, d)?;
    ensure!(
        owner.file.path == owner_path(a)?
            && owner.file.identity == p.owner_identity
            && io::security_fingerprint(&owner.file, d)? == p.owner_security_hash,
        "Original retirement owner identity or security changed"
    );
    Ok((source, owner))
}
fn admitted_owner(bytes: &[u8], p: &OwnerRetirementPlan, e: &RetirementEvent) -> Result<String> {
    let before = serde_json::to_vec(&p.before_owner)?;
    let after = serde_json::to_vec(&p.after_owner)?;
    let state = if bytes == before {
        "original"
    } else if bytes == after {
        "retired"
    } else if e.phase == RetirementPhase::Writing
        && (before.starts_with(bytes) || after.starts_with(bytes))
    {
        // Fixing a recovery direction can itself be interrupted before the
        // next rewrite. The preceding recorded rewrite's prefix still exists.
        "owned_partial"
    } else {
        anyhow::bail!("Foreign or phase-ineligible owner metadata preserved");
    };
    if e.phase == RetirementPhase::Prepared {
        ensure!(bytes == before, "Unmarked retirement owner changed");
    }
    if terminal(e) {
        ensure!(
            (e.phase == RetirementPhase::Committed && bytes == after)
                || (e.phase == RetirementPhase::RolledBack && bytes == before),
            "Terminal retirement owner changed"
        );
    }
    Ok(state.into())
}
fn marker_present(
    a: &BoundDocument,
    p: &OwnerRetirementPlan,
    e: &RetirementEvent,
    d: &Deadline,
) -> Result<(bool, bool)> {
    let expected = serde_json::to_vec(&marker(p))?;
    let pending = io::read_file(&marker_path(a)?, 2048, false, d)?;
    let complete = io::read_file(&complete_path(a, p), 2048, false, d)?;
    ensure!(
        pending.as_ref().is_none_or(|file| file.bytes == expected)
            && complete.as_ref().is_none_or(|file| file.bytes == expected),
        "Foreign owner retirement marker or terminal history preserved"
    );
    ensure!(
        !(pending.is_some() && complete.is_some()),
        "Conflicting owner retirement marker roles preserved"
    );
    if terminal(e) {
        ensure!(
            pending.is_some() || complete.is_some(),
            "Terminal retirement marker evidence absent"
        );
    } else {
        ensure!(
            complete.is_none() && (e.phase == RetirementPhase::Prepared || pending.is_some()),
            "Owner retirement pending marker absent or premature terminal evidence"
        );
    }
    Ok((pending.is_some(), complete.is_some()))
}
fn retain_marker(a: &BoundDocument, p: &OwnerRetirementPlan, d: &Deadline) -> Result<bool> {
    let expected = serde_json::to_vec(&marker(p))?;
    if let Some(mut file) = io::read_file(&marker_path(a)?, 2048, true, d)? {
        ensure!(
            file.bytes == expected,
            "Foreign retirement marker preserved"
        );
        file.rename_no_replace(&complete_path(a, p), d)?;
        return Ok(true);
    }
    let file = io::read_file(&complete_path(a, p), 2048, false, d)?
        .context("Owner retirement terminal evidence absent")?;
    ensure!(
        file.bytes == expected,
        "Foreign retirement terminal evidence preserved"
    );
    Ok(false)
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OwnerRetirementReview {
    pub plan_id: String,
    pub event_id: String,
    pub phase: String,
    pub direction: Option<RecoveryDirection>,
    pub owner_state: String,
    pub marker_present: bool,
    pub terminal_evidence_present: bool,
    pub complete: bool,
    pub file_changed: bool,
    pub source_written: bool,
}
pub fn inspect_owner_retirement(
    access: &Access,
    p: &OwnerRetirementPlan,
    d: &Deadline,
) -> Result<OwnerRetirementReview> {
    check_plan(access, p, d)?;
    let a = selected(access, &p.document_id)?;
    let _bindings = hold_selection(access, &p.document_id, d)?;
    let root = state(access, false, d)?;
    let e = load_event(access, p, &root.root.path, d)?;
    require_no_other_pending(a, d)?;
    ensure!(
        journal_state(access, &p.document_id, &root.root.path, Some(p), d)? == p.journal_state,
        "Owner retirement journal evidence changed"
    );
    let (_, source) = access.current(&p.document_id, d)?;
    check_source(access, p, &source, d)?;
    let owner = io::read_file(&owner_path(a)?, 2048, false, d)?
        .context("Retirement owner evidence absent")?;
    ensure!(
        owner.identity == p.owner_identity
            && io::security_fingerprint(&owner, d)? == p.owner_security_hash,
        "Retirement owner identity or security changed"
    );
    let owner_state = admitted_owner(&owner.bytes, p, &e)?;
    let (pending, complete) = marker_present(a, p, &e, d)?;
    Ok(OwnerRetirementReview {
        plan_id: p.id.clone(),
        event_id: e.id.clone(),
        phase: phase(&e)?,
        direction: e.direction,
        owner_state,
        marker_present: pending,
        terminal_evidence_present: complete,
        complete: terminal(&e) && complete,
        file_changed: false,
        source_written: false,
    })
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OwnerRetirementReceipt {
    pub plan_id: String,
    pub event_id: String,
    pub outcome: String,
    pub metadata_changed: bool,
    pub file_changed: bool,
    pub source_written: bool,
}
fn receipt(
    p: &OwnerRetirementPlan,
    e: &RetirementEvent,
    changed: bool,
) -> Result<OwnerRetirementReceipt> {
    Ok(OwnerRetirementReceipt {
        plan_id: p.id.clone(),
        event_id: e.id.clone(),
        outcome: phase(e)?,
        metadata_changed: changed,
        file_changed: false,
        source_written: false,
    })
}
pub fn apply_owner_retirement(
    access: &Access,
    authority: &WriteAuthority,
    approval: &OwnerRetirementApproval,
    p: &OwnerRetirementPlan,
    d: &Deadline,
) -> Result<OwnerRetirementReceipt> {
    check_authority(authority, access, p, d)?;
    ensure!(
        approval.plan_id == p.id && approval.binding == access.binding,
        "Concrete host owner retirement approval required"
    );
    let a = selected(access, &p.document_id)?;
    let _bindings = hold_selection(access, &p.document_id, d)?;
    let root = state(access, true, d)?;
    budget(&root.root.path, d)?;
    binding_migration::require_no_pending(a, d)?;
    require_owner(access, a, d)?;
    let (_source, mut owner) = held_owner(access, p, d)?;
    ensure!(
        owner.file.bytes == serde_json::to_vec(&p.before_owner)?
            && journal_state(access, &p.document_id, &root.root.path, None, d)? == p.journal_state,
        "Owner retirement review changed"
    );
    ensure!(
        !exists(&complete_path(a, p), d)?
            && !exists(&plan_path(&root.root.path, &retirement_stem(p)), d)?,
        "Occupied owner retirement intent or terminal role; inspect explicit recovery"
    );
    budget(&root.root.path, d)?;
    let mut e = initial(p);
    let header = RetirementHeader {
        schema_version: 1,
        plan: p.clone(),
        initial: e.clone(),
    };
    ensure!(
        serde_json::to_vec(&header)?.len() <= MAX_RETIREMENT,
        "Owner retirement header byte capacity exceeded"
    );
    write_record(
        &root.root.path,
        &plan_path(&root.root.path, &retirement_stem(p)),
        &serde_json::to_vec(&header)?,
        d,
    )?;
    checkpoint("owner_retirement_intent")?;
    write_record(
        &a.parent_path,
        &marker_path(a)?,
        &serde_json::to_vec(&marker(p))?,
        d,
    )?;
    checkpoint("owner_retirement_marked")?;
    next(
        &root.root.path,
        p,
        &mut e,
        RetirementPhase::Writing,
        None,
        d,
    )?;
    checkpoint("owner_retirement_writing")?;
    owner.rewrite(&serde_json::to_vec(&p.after_owner)?, d)?;
    checkpoint("owner_retirement_rewritten")?;
    next(
        &root.root.path,
        p,
        &mut e,
        RetirementPhase::Committed,
        None,
        d,
    )?;
    checkpoint("owner_retirement_committed")?;
    retain_marker(a, p, d)?;
    checkpoint("owner_retirement_response")?;
    receipt(p, &e, true)
}
pub fn recover_owner_retirement(
    access: &Access,
    authority: &WriteAuthority,
    p: &OwnerRetirementPlan,
    expected_event: &str,
    direction: RecoveryDirection,
    d: &Deadline,
) -> Result<OwnerRetirementReceipt> {
    check_authority(authority, access, p, d)?;
    ensure!(
        valid_id(expected_event),
        "Explicit owner retirement event review required"
    );
    let a = selected(access, &p.document_id)?;
    let _bindings = hold_selection(access, &p.document_id, d)?;
    let root = state(access, true, d)?;
    require_no_other_pending(a, d)?;
    let mut e = load_event(access, p, &root.root.path, d)?;
    ensure!(
        e.id == expected_event && e.direction.is_none_or(|fixed| fixed == direction),
        "Stale owner retirement review or fixed recovery direction"
    );
    if terminal(&e) {
        ensure!(
            (e.phase == RetirementPhase::Committed) == (direction == RecoveryDirection::Resume),
            "Terminal owner retirement direction cannot reverse"
        );
    }
    let (_source, mut owner) = held_owner(access, p, d)?;
    admitted_owner(&owner.file.bytes, p, &e)?;
    let (pending, _) = marker_present(a, p, &e, d)?;
    ensure!(
        journal_state(access, &p.document_id, &root.root.path, Some(p), d)? == p.journal_state,
        "Owner retirement journal evidence changed"
    );
    if terminal(&e) {
        return receipt(p, &e, retain_marker(a, p, d)?);
    }
    let fixes_direction = e.direction.is_none();
    let required_events = if fixes_direction { 2 } else { 1 };
    ensure!(
        e.sequence + required_events < MAX_RETIREMENT_EVENTS,
        "Owner retirement must reserve its terminal event before mutation"
    );
    budget(&root.root.path, d)?;
    if !pending {
        write_record(
            &a.parent_path,
            &marker_path(a)?,
            &serde_json::to_vec(&marker(p))?,
            d,
        )?;
    }
    if fixes_direction {
        next(
            &root.root.path,
            p,
            &mut e,
            RetirementPhase::Writing,
            Some(direction),
            d,
        )?;
    }
    checkpoint("owner_retirement_direction")?;
    let desired = if direction == RecoveryDirection::Resume {
        serde_json::to_vec(&p.after_owner)?
    } else {
        serde_json::to_vec(&p.before_owner)?
    };
    if owner.file.bytes != desired {
        owner.rewrite(&desired, d)?;
    }
    checkpoint("owner_retirement_recovered")?;
    next(
        &root.root.path,
        p,
        &mut e,
        if direction == RecoveryDirection::Resume {
            RetirementPhase::Committed
        } else {
            RetirementPhase::RolledBack
        },
        Some(direction),
        d,
    )?;
    retain_marker(a, p, d)?;
    receipt(p, &e, true)
}

#[cfg(test)]
mod protocol_tests {
    use super::*;

    fn owner(generation: Option<String>) -> OwnerBinding {
        let mut owner = OwnerBinding {
            schema_version: if generation.is_some() { 2 } else { 1 },
            id: String::new(),
            target: hash("target"),
            access_binding: hash("selection"),
            journal_identity: "physical-journal".into(),
            generation,
        };
        owner.id = owner_id(&owner);
        owner
    }
    #[test]
    fn tombstones_keep_optional_generation_and_old_owner_readers_refuse_them() {
        for generation in [None, Some(hash("generation"))] {
            let before = owner(generation.clone());
            let after = retired_owner(&before, &hash("retirement"));
            assert_eq!(after.schema_version, 3);
            assert_eq!(after.generation, generation);
            assert_eq!(after.original_owner_id, before.id);
            let bytes = serde_json::to_vec(&after).unwrap();
            assert!(serde_json::from_slice::<OwnerBinding>(&bytes).is_err());
            let decoded: RetiredOwner = serde_json::from_slice(&bytes).unwrap();
            assert_eq!(decoded.id, retired_id(&decoded));
            check_retired_owner(&decoded, &before.target).unwrap();
            assert_ne!(
                after.id,
                retired_owner(&before, &hash("other retirement")).id
            );
        }
    }
    #[test]
    fn retirement_protocol_rejects_unknown_duplicate_truncated_and_oversized_records() {
        let e = RetirementEvent {
            schema_version: 1,
            id: hash("id"),
            plan_id: hash("plan"),
            sequence: 0,
            previous: None,
            phase: RetirementPhase::Prepared,
            direction: None,
        };
        let bytes = serde_json::to_vec(&e).unwrap();
        let text = String::from_utf8(bytes.clone()).unwrap();
        for altered in [
            text.replacen('{', "{\"unknown\":true,", 1),
            text.replacen('{', "{\"schemaVersion\":1,", 1),
            text[..text.len() - 1].into(),
            format!(" {text}"),
        ] {
            assert!(
                canonical_bytes::<RetirementEvent>(
                    altered.as_bytes(),
                    MAX_EVENT,
                    &Deadline::new(1000)
                )
                .is_err()
            );
        }
        assert!(
            canonical_bytes::<RetirementEvent>(&bytes, bytes.len() - 1, &Deadline::new(1000))
                .is_err()
        );
        assert!(canonical_bytes::<RetirementEvent>(&bytes, MAX_EVENT, &Deadline::new(0)).is_err());
    }
    #[test]
    fn closed_events_never_reopen_and_fixed_recovery_directions_never_reverse() {
        let mut e = RetirementEvent {
            schema_version: 1,
            id: hash("id"),
            plan_id: hash("plan"),
            sequence: 0,
            previous: None,
            phase: RetirementPhase::Prepared,
            direction: None,
        };
        let mut next = e.clone();
        next.phase = RetirementPhase::Committed;
        assert!(!valid_transition(&e, &next));
        e.phase = RetirementPhase::Writing;
        e.direction = Some(RecoveryDirection::Rollback);
        next.phase = RetirementPhase::Writing;
        next.direction = Some(RecoveryDirection::Resume);
        assert!(!valid_transition(&e, &next));
        next.direction = e.direction;
        next.phase = RetirementPhase::Committed;
        assert!(!valid_transition(&e, &next));
        e.phase = RetirementPhase::RolledBack;
        next.phase = RetirementPhase::Writing;
        assert!(!valid_transition(&e, &next));
    }
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    use std::fs;

    const ORIGINAL: &str = "\u{feff}# original e\u{301}\r\nvalue: \u{1f980}\r\n";
    const SOURCE: &[u8] = b"fn untouched() {}\r\n";
    fn d() -> Deadline {
        Deadline::new(30_000)
    }
    struct Fixture {
        _root: tempfile::TempDir,
        target: PathBuf,
        source: PathBuf,
        journal: PathBuf,
        access: Access,
        alternate: Access,
        authority: WriteAuthority,
    }
    impl Fixture {
        fn new() -> Self {
            let root = tempfile::tempdir().unwrap();
            let source = root.path().join("source");
            let journal = root.path().join("journals");
            let other = root.path().join("other-journals");
            for path in [&source, &journal, &other] {
                fs::create_dir(path).unwrap();
            }
            fs::write(source.join("untouched.rs"), SOURCE).unwrap();
            let target = root.path().join("settings-\u{130}-e\u{301}.yaml");
            fs::write(&target, ORIGINAL).unwrap();
            let roots = [ProjectRoot {
                name: "default".into(),
                path: source.clone(),
            }];
            let config = |id: &str, journal| Config {
                documents: vec![DocumentSpec {
                    id: id.into(),
                    label: id.into(),
                    path: target.clone(),
                    scope: Scope::Global {},
                    writable: true,
                }],
                journal_root: Some(journal),
                max_document_bytes: MAX_BYTES,
            };
            let access = Access::open(&config("owned", journal.clone()), &roots, &d()).unwrap();
            let alternate = Access::open(&config("alternate", other), &roots, &d()).unwrap();
            let authority = WriteAuthority::new(&access).unwrap();
            bind_owner(&access, &access.documents["owned"], &d()).unwrap();
            Self {
                _root: root,
                target,
                source,
                journal,
                access,
                alternate,
                authority,
            }
        }
        fn make_generated(&self) {
            let other = WriteAuthority::new(&self.alternate).unwrap();
            for (from, to, old, new, a, b) in [
                (
                    &self.access,
                    &self.alternate,
                    "owned",
                    "alternate",
                    &self.authority,
                    &other,
                ),
                (
                    &self.alternate,
                    &self.access,
                    "alternate",
                    "owned",
                    &other,
                    &self.authority,
                ),
            ] {
                let p = plan_binding_migration(from, to, old, new, &d()).unwrap();
                let authority = BindingMigrationAuthority::new(from, to, old, new, a, b).unwrap();
                let approval = authority.approve(from, to, &p, &d()).unwrap();
                apply_binding_migration(from, to, &authority, &approval, &p, &d()).unwrap();
            }
        }
        fn plan(&self) -> OwnerRetirementPlan {
            plan_owner_retirement(&self.access, "owned", &d()).unwrap()
        }
        fn apply(&self, p: &OwnerRetirementPlan) -> Result<OwnerRetirementReceipt> {
            let approval = self
                .authority
                .approve_owner_retirement(&self.access, p, &d())?;
            apply_owner_retirement(&self.access, &self.authority, &approval, p, &d())
        }
        fn inspect(&self, p: &OwnerRetirementPlan) -> OwnerRetirementReview {
            inspect_owner_retirement(&self.access, p, &d()).unwrap()
        }
        fn recover(
            &self,
            p: &OwnerRetirementPlan,
            event: &str,
            direction: RecoveryDirection,
        ) -> Result<OwnerRetirementReceipt> {
            recover_owner_retirement(&self.access, &self.authority, p, event, direction, &d())
        }
        fn owner_path(&self) -> PathBuf {
            owner_path(&self.access.documents["owned"]).unwrap()
        }
        fn marker_path(&self) -> PathBuf {
            marker_path(&self.access.documents["owned"]).unwrap()
        }
        fn preserve(&self) {
            assert_eq!(fs::read(&self.target).unwrap(), ORIGINAL.as_bytes());
            assert_eq!(fs::read(self.source.join("untouched.rs")).unwrap(), SOURCE);
            assert_eq!(self.access.read("owned", &d()).unwrap().content, ORIGINAL);
            assert_eq!(
                self.alternate.read("alternate", &d()).unwrap().content,
                ORIGINAL
            );
        }
        fn interrupt(&self, p: &OwnerRetirementPlan, point: &'static str) {
            super::super::STOP_AFTER.with(|slot| *slot.borrow_mut() = Some(point));
            assert!(self.apply(p).is_err());
            self.preserve();
        }
        fn ordinary(&self) -> Plan {
            let current = self.access.read("owned", &d()).unwrap();
            self.access
                .plan(
                    "owned",
                    &current.revision,
                    "explicit proposed bytes\r\n",
                    &d(),
                )
                .unwrap()
        }
        fn closed_journal(&self) -> Plan {
            let p = self.ordinary();
            let approved = self.authority.approve(&self.access, &p, &d()).unwrap();
            super::super::STOP_AFTER.with(|slot| *slot.borrow_mut() = Some("planned"));
            assert!(
                self.access
                    .apply(&self.authority, &approved, &p, &d())
                    .is_err()
            );
            let view = self.access.inspect_journal(&p.id, &d()).unwrap();
            let approved = self
                .authority
                .approve_recovery_at(
                    &self.access,
                    &p.id,
                    &view.event_id,
                    RecoveryDirection::Rollback,
                    &d(),
                )
                .unwrap();
            self.access
                .recover(&self.authority, &approved, &d())
                .unwrap();
            p
        }
    }
    #[test]
    fn original_authority_can_retire_an_owner_created_before_any_journal_header() {
        let f = Fixture::new();
        let before = fs::read(f.owner_path()).unwrap();
        let proposal = f.ordinary();
        let approved = f.authority.approve(&f.access, &proposal, &d()).unwrap();
        super::super::STOP_AFTER.with(|slot| *slot.borrow_mut() = Some("owner_bound"));
        assert!(
            f.access
                .apply(&f.authority, &approved, &proposal, &d())
                .is_err()
        );
        assert!(!plan_path(&f.journal, &stem(&f.access, &proposal.id).unwrap()).exists());
        let source = f.access.read("owned", &d()).unwrap();
        let owner = io::read_file(&f.owner_path(), 2048, false, &d())
            .unwrap()
            .unwrap();
        let owner_identity = owner.identity.clone();
        let owner_security = io::security_fingerprint(&owner, &d()).unwrap();
        drop(owner);
        let p = f.plan();
        assert!(!f.access.owner_retirement_blocked("owned", &d()).unwrap());
        assert_eq!(fs::read(f.owner_path()).unwrap(), before);
        assert_eq!(fs::read_dir(&f.journal).unwrap().count(), 1); // Original lock only; no header.
        assert!(!f.marker_path().exists());
        let result = f.apply(&p).unwrap();
        assert_eq!(result.outcome, "committed");
        assert!(result.metadata_changed && !result.file_changed && !result.source_written);
        assert_eq!(
            fs::read(f.owner_path()).unwrap(),
            serde_json::to_vec(&p.after_owner).unwrap()
        );
        let owner = io::read_file(&f.owner_path(), 2048, false, &d())
            .unwrap()
            .unwrap();
        assert_eq!(owner.identity, owner_identity);
        assert_eq!(
            io::security_fingerprint(&owner, &d()).unwrap(),
            owner_security
        );
        assert_eq!(
            f.access.read("owned", &d()).unwrap().revision,
            source.revision
        );
        assert!(f.inspect(&p).complete);
        assert!(f.access.owner_retirement_blocked("owned", &d()).unwrap());
        f.preserve();
    }
    #[test]
    fn cached_generationless_and_generated_proposals_recovery_and_rebinding_cannot_resurrect() {
        for generated in [false, true] {
            let f = Fixture::new();
            if generated {
                f.make_generated();
            }
            let proposal = f.ordinary();
            assert_eq!(proposal.owner_generation.is_some(), generated);
            let approval = f.authority.approve(&f.access, &proposal, &d()).unwrap();
            let closed = f.closed_journal();
            let recovery = f
                .authority
                .approve_recovery(&f.access, &closed.id, RecoveryDirection::Rollback, &d())
                .unwrap();
            let p = f.plan();
            assert_eq!(p.after_owner.generation, p.before_owner.generation);
            f.apply(&p).unwrap();
            assert!(f.authority.approve(&f.access, &proposal, &d()).is_err());
            assert!(
                f.access
                    .apply(&f.authority, &approval, &proposal, &d())
                    .is_err()
            );
            assert!(f.access.recover(&f.authority, &recovery, &d()).is_err());
            assert!(
                f.access
                    .plan(
                        "owned",
                        &f.access.read("owned", &d()).unwrap().revision,
                        "fresh",
                        &d()
                    )
                    .is_err()
            );
            assert!(bind_owner(&f.access, &f.access.documents["owned"], &d()).is_err());
            assert!(bind_owner(&f.alternate, &f.alternate.documents["alternate"], &d()).is_err());
            assert!(
                plan_binding_migration(&f.access, &f.alternate, "owned", "alternate", &d())
                    .is_err()
            );
            assert!(plan_owner_retirement(&f.access, "owned", &d()).is_err());
            assert_eq!(
                fs::read(f.owner_path()).unwrap(),
                serde_json::to_vec(&p.after_owner).unwrap()
            );
            f.preserve();
        }
    }
    #[test]
    fn completed_archive_inspection_and_exact_retained_reads_survive_live_owner_retirement() {
        let f = Fixture::new();
        let journal = f.closed_journal();
        let record = format!("{}.plan.json", stem(&f.access, &journal.id).unwrap());
        let original_record = fs::read(f.journal.join(&record)).unwrap();
        let other = WriteAuthority::new(&f.alternate).unwrap();
        let migration =
            plan_binding_migration(&f.access, &f.alternate, "owned", "alternate", &d()).unwrap();
        let migration_authority = BindingMigrationAuthority::new(
            &f.access,
            &f.alternate,
            "owned",
            "alternate",
            &f.authority,
            &other,
        )
        .unwrap();
        let approved = migration_authority
            .approve(&f.access, &f.alternate, &migration, &d())
            .unwrap();
        apply_binding_migration(
            &f.access,
            &f.alternate,
            &migration_authority,
            &approved,
            &migration,
            &d(),
        )
        .unwrap();
        let archive_root = f._root.path().join("archive");
        fs::create_dir(&archive_root).unwrap();
        let archive_access = HistoryArchiveAccess::new(
            &f.access,
            &f.alternate,
            &migration,
            &archive_root,
            &f.authority,
            &other,
            &d(),
        )
        .unwrap();
        let archive =
            plan_history_archive(&f.access, &f.alternate, &archive_access, &migration, &d())
                .unwrap();
        let approved = archive_access
            .approve(&f.access, &f.alternate, &archive, &d())
            .unwrap();
        assert!(
            apply_history_archive(
                &f.access,
                &f.alternate,
                &archive_access,
                &approved,
                &archive,
                &d()
            )
            .unwrap()
            .complete
        );
        let baseline = read_history_archive_record(
            &f.access,
            &f.alternate,
            &archive_access,
            &archive,
            &record,
            &d(),
        )
        .unwrap();
        assert_eq!(baseline.sha256, hash(&original_record));
        assert!(!f.journal.join(&record).exists());
        let p = plan_owner_retirement(&f.alternate, "alternate", &d()).unwrap();
        let approved = other
            .approve_owner_retirement(&f.alternate, &p, &d())
            .unwrap();
        super::super::STOP_AFTER.with(|slot| *slot.borrow_mut() = Some("owner_retirement_marked"));
        assert!(apply_owner_retirement(&f.alternate, &other, &approved, &p, &d()).is_err());
        assert!(
            archive_access
                .approve(&f.access, &f.alternate, &archive, &d())
                .is_err()
        );
        assert!(
            plan_history_archive(&f.access, &f.alternate, &archive_access, &migration, &d())
                .is_err()
        );
        assert_eq!(
            serde_json::to_vec(
                &read_history_archive_record(
                    &f.access,
                    &f.alternate,
                    &archive_access,
                    &archive,
                    &record,
                    &d()
                )
                .unwrap()
            )
            .unwrap(),
            serde_json::to_vec(&baseline).unwrap()
        );
        let review = inspect_owner_retirement(&f.alternate, &p, &d()).unwrap();
        recover_owner_retirement(
            &f.alternate,
            &other,
            &p,
            &review.event_id,
            RecoveryDirection::Resume,
            &d(),
        )
        .unwrap();
        assert!(
            inspect_history_archive(&f.access, &f.alternate, &archive_access, &archive, &d())
                .unwrap()
                .complete
        );
        let retained = read_history_archive_record(
            &f.access,
            &f.alternate,
            &archive_access,
            &archive,
            &record,
            &d(),
        )
        .unwrap();
        assert_eq!(
            serde_json::to_vec(&retained).unwrap(),
            serde_json::to_vec(&baseline).unwrap()
        );
        assert!(
            archive_access
                .approve(&f.access, &f.alternate, &archive, &d())
                .is_err()
        );
        assert!(
            plan_history_archive(&f.access, &f.alternate, &archive_access, &migration, &d())
                .is_err()
        );
        f.preserve();
    }
    #[test]
    fn rehashed_tombstone_generation_schema_and_selection_forgery_cannot_authorize_metadata() {
        for field in ["generation", "schema", "selection", "target", "identity"] {
            let f = Fixture::new();
            let mut p = f.plan();
            let before = fs::read(f.owner_path()).unwrap();
            match field {
                "generation" => p.after_owner.generation = Some(hash("foreign generation")),
                "schema" => p.after_owner.schema_version = 2,
                "selection" => {
                    p.before_owner.access_binding = f.alternate.binding.clone();
                    p.before_owner.id = owner_id(&p.before_owner);
                    p.after_owner = retired_owner(&p.before_owner, &p.nonce);
                }
                "target" => p.target = hash("foreign target"),
                _ => p.owner_identity.push_str("-foreign"),
            }
            p.id = plan_id(&p);
            assert!(f.apply(&p).is_err());
            assert_eq!(fs::read(f.owner_path()).unwrap(), before);
            assert!(!f.marker_path().exists());
            assert!(!plan_path(&f.journal, &retirement_stem(&p)).exists());
            f.preserve();
        }
    }
    #[test]
    fn every_owned_interruption_recovers_in_reviewed_direction_and_keeps_source_bytes() {
        for point in [
            "owner_retirement_intent",
            "owner_retirement_marked",
            "owner_retirement_writing",
            "owner_retirement_rewritten",
        ] {
            for direction in [RecoveryDirection::Resume, RecoveryDirection::Rollback] {
                let f = Fixture::new();
                let p = f.plan();
                f.interrupt(&p, point);
                let review = f.inspect(&p);
                assert!(!review.complete && !review.file_changed && !review.source_written);
                if point != "owner_retirement_intent" {
                    assert!(f.access.owner_binding("owned", &d()).is_err());
                }
                let result = f.recover(&p, &review.event_id, direction).unwrap();
                assert!(result.metadata_changed && !result.file_changed && !result.source_written);
                let expected = if direction == RecoveryDirection::Resume {
                    serde_json::to_vec(&p.after_owner).unwrap()
                } else {
                    serde_json::to_vec(&p.before_owner).unwrap()
                };
                assert_eq!(fs::read(f.owner_path()).unwrap(), expected);
                assert_eq!(
                    f.access.owner_retirement_blocked("owned", &d()).unwrap(),
                    direction == RecoveryDirection::Resume
                );
                let terminal = f.inspect(&p);
                assert!(
                    terminal.complete
                        && !terminal.marker_present
                        && terminal.terminal_evidence_present
                );
                assert!(
                    !f.recover(&p, &terminal.event_id, direction)
                        .unwrap()
                        .metadata_changed
                );
                let opposite = if direction == RecoveryDirection::Resume {
                    RecoveryDirection::Rollback
                } else {
                    RecoveryDirection::Resume
                };
                assert!(f.recover(&p, &terminal.event_id, opposite).is_err());
                f.preserve();
            }
        }
    }
    #[test]
    fn terminal_event_and_lost_response_are_inspectable_without_repeating_owner_writes() {
        for point in ["owner_retirement_committed", "owner_retirement_response"] {
            let f = Fixture::new();
            let p = f.plan();
            f.interrupt(&p, point);
            let review = f.inspect(&p);
            assert_eq!(review.phase, "committed");
            assert_eq!(review.complete, point == "owner_retirement_response");
            let bytes = fs::read(f.owner_path()).unwrap();
            let r = f
                .recover(&p, &review.event_id, RecoveryDirection::Resume)
                .unwrap();
            assert_eq!(r.metadata_changed, point == "owner_retirement_committed");
            assert_eq!(fs::read(f.owner_path()).unwrap(), bytes);
            assert!(f.inspect(&p).complete);
            f.preserve();
        }
    }
    #[test]
    fn empty_and_partial_owned_rewrites_resume_or_restore_without_unlinking_owner() {
        for count in [0, 1, 73, 199] {
            for direction in [RecoveryDirection::Resume, RecoveryDirection::Rollback] {
                let f = Fixture::new();
                let p = f.plan();
                io::set_document_owner_write_stop(Some(count));
                assert!(f.apply(&p).is_err());
                assert!(f.owner_path().exists());
                let file = io::read_file(&f.owner_path(), 2048, false, &d())
                    .unwrap()
                    .unwrap();
                assert_eq!(file.identity, p.owner_identity);
                assert_eq!(file.byte_length, count);
                drop(file);
                assert!(f.access.owner_binding("owned", &d()).is_err());
                let review = f.inspect(&p);
                assert_eq!(review.owner_state, "owned_partial");
                f.recover(&p, &review.event_id, direction).unwrap();
                assert!(f.inspect(&p).complete);
                f.preserve();
            }
        }
    }
    #[test]
    fn stale_review_and_fixed_direction_refuse_before_mutating_partial_metadata() {
        let f = Fixture::new();
        let p = f.plan();
        f.interrupt(&p, "owner_retirement_rewritten");
        let review = f.inspect(&p);
        super::super::STOP_AFTER
            .with(|slot| *slot.borrow_mut() = Some("owner_retirement_direction"));
        assert!(
            f.recover(&p, &review.event_id, RecoveryDirection::Rollback)
                .is_err()
        );
        let fixed = f.inspect(&p);
        assert_ne!(review.event_id, fixed.event_id);
        assert!(fixed.direction == Some(RecoveryDirection::Rollback));
        let before = fs::read(f.owner_path()).unwrap();
        assert!(
            f.recover(&p, &review.event_id, RecoveryDirection::Rollback)
                .is_err()
        );
        assert!(
            f.recover(&p, &fixed.event_id, RecoveryDirection::Resume)
                .is_err()
        );
        assert_eq!(fs::read(f.owner_path()).unwrap(), before);
        io::set_document_owner_write_stop(Some(73));
        assert!(
            f.recover(&p, &fixed.event_id, RecoveryDirection::Rollback)
                .is_err()
        );
        let partial = f.inspect(&p);
        assert_eq!(partial.owner_state, "owned_partial");
        f.recover(&p, &partial.event_id, RecoveryDirection::Rollback)
            .unwrap();
        assert_eq!(
            fs::read(f.owner_path()).unwrap(),
            serde_json::to_vec(&p.before_owner).unwrap()
        );
        f.preserve();
    }
    #[test]
    fn interrupted_direction_fix_preserves_the_preceding_partial_rewrite_for_rollback() {
        let f = Fixture::new();
        let p = f.plan();
        io::set_document_owner_write_stop(Some(73));
        assert!(f.apply(&p).is_err());
        let review = f.inspect(&p);
        let partial = fs::read(f.owner_path()).unwrap();
        super::super::STOP_AFTER
            .with(|slot| *slot.borrow_mut() = Some("owner_retirement_direction"));
        assert!(
            f.recover(&p, &review.event_id, RecoveryDirection::Rollback)
                .is_err()
        );
        assert_eq!(fs::read(f.owner_path()).unwrap(), partial);
        let fixed = f.inspect(&p);
        assert!(fixed.direction == Some(RecoveryDirection::Rollback));
        assert_eq!(fixed.owner_state, "owned_partial");
        assert!(
            f.recover(&p, &fixed.event_id, RecoveryDirection::Resume)
                .is_err()
        );
        f.recover(&p, &fixed.event_id, RecoveryDirection::Rollback)
            .unwrap();
        assert_eq!(
            fs::read(f.owner_path()).unwrap(),
            serde_json::to_vec(&p.before_owner).unwrap()
        );
        f.preserve();
    }
    #[test]
    fn missing_marker_evidence_after_writing_or_commit_stays_uncertain_and_preserved() {
        for point in ["owner_retirement_writing", "owner_retirement_response"] {
            let f = Fixture::new();
            let p = f.plan();
            f.interrupt(&p, point);
            let review = f.inspect(&p);
            let path = if point == "owner_retirement_response" {
                complete_path(&f.access.documents["owned"], &p)
            } else {
                f.marker_path()
            };
            let original = fs::read(&path).unwrap();
            let retained = path.with_extension("missing-fixture");
            fs::rename(&path, &retained).unwrap();
            let before = fs::read(f.owner_path()).unwrap();
            assert!(inspect_owner_retirement(&f.access, &p, &d()).is_err());
            assert!(
                f.recover(&p, &review.event_id, RecoveryDirection::Resume)
                    .is_err()
            );
            assert_eq!(fs::read(&retained).unwrap(), original);
            assert_eq!(fs::read(f.owner_path()).unwrap(), before);
            assert!(!path.exists());
            f.preserve();
        }
    }
    #[test]
    fn foreign_bytes_replaced_identity_and_foreign_markers_are_never_adopted_or_removed() {
        for role in ["owner_bytes", "owner_identity", "pending", "complete"] {
            let f = Fixture::new();
            let p = f.plan();
            f.interrupt(&p, "owner_retirement_marked");
            let review = f.inspect(&p);
            let path = match role {
                "owner_bytes" | "owner_identity" => f.owner_path(),
                "pending" => f.marker_path(),
                _ => complete_path(&f.access.documents["owned"], &p),
            };
            if role == "owner_identity" {
                fs::rename(&path, path.with_extension("original-fixture")).unwrap();
            }
            fs::write(&path, b"foreign fixture occupant").unwrap();
            let event = fs::read(plan_path(&f.journal, &retirement_stem(&p))).unwrap();
            assert!(
                f.recover(&p, &review.event_id, RecoveryDirection::Resume)
                    .is_err()
            );
            assert!(inspect_owner_retirement(&f.access, &p, &d()).is_err());
            assert_eq!(fs::read(&path).unwrap(), b"foreign fixture occupant");
            assert_eq!(
                fs::read(plan_path(&f.journal, &retirement_stem(&p))).unwrap(),
                event
            );
            f.preserve();
        }
    }
    #[test]
    fn pending_journals_and_changed_closed_evidence_refuse_a_previously_reviewed_retirement() {
        let f = Fixture::new();
        let p = f.plan();
        let proposal = f.ordinary();
        let approval = f.authority.approve(&f.access, &proposal, &d()).unwrap();
        super::super::STOP_AFTER.with(|slot| *slot.borrow_mut() = Some("planned"));
        assert!(
            f.access
                .apply(&f.authority, &approval, &proposal, &d())
                .is_err()
        );
        let before = fs::read(f.owner_path()).unwrap();
        assert!(plan_owner_retirement(&f.access, "owned", &d()).is_err());
        assert!(f.apply(&p).is_err());
        let view = f.access.inspect_journal(&proposal.id, &d()).unwrap();
        let approval = f
            .authority
            .approve_recovery_at(
                &f.access,
                &proposal.id,
                &view.event_id,
                RecoveryDirection::Rollback,
                &d(),
            )
            .unwrap();
        f.access.recover(&f.authority, &approval, &d()).unwrap();
        assert!(f.apply(&p).is_err());
        let p = f.plan();
        let before_header = fs::read(plan_path(
            &f.journal,
            &stem(&f.access, &proposal.id).unwrap(),
        ))
        .unwrap();
        f.apply(&p).unwrap();
        assert_eq!(
            fs::read(plan_path(
                &f.journal,
                &stem(&f.access, &proposal.id).unwrap()
            ))
            .unwrap(),
            before_header
        );
        assert_ne!(fs::read(f.owner_path()).unwrap(), before);
        f.preserve();
    }
    #[test]
    fn orphan_journal_events_unknown_roles_and_unattributed_parent_outputs_refuse() {
        for role in [
            "orphan_event",
            "unknown_event",
            "backup",
            "output",
            "case_output",
            "case_event",
            "unknown_namespace",
            "foreign_schema",
            "foreign_duplicate",
            "unknown_root",
        ] {
            let f = Fixture::new();
            let p = f.plan();
            let before = fs::read(f.owner_path()).unwrap();
            let proposal = f.ordinary();
            let id = proposal.id.clone();
            let name = stem(&f.access, &id).unwrap();
            let mut event = super::super::initial(&f.access, &proposal);
            event.sequence = 1;
            event.previous = Some(event.id.clone());
            event.phase = Phase::Allocating;
            event.id = super::super::event_id(&event);
            let bytes = match role {
                "orphan_event" | "unknown_event" | "case_event" => {
                    serde_json::to_vec(&event).unwrap()
                }
                "foreign_schema" => b"{\"schemaVersion\":99}".to_vec(),
                "foreign_duplicate" => b"{\"schemaVersion\":1,\"schemaVersion\":1}".to_vec(),
                _ => b"foreign role fixture".to_vec(),
            };
            let path = match role {
                "orphan_event" => event_path(&f.journal, &name, 1),
                "unknown_event" => f.journal.join(format!("{name}.e9999.json")),
                "backup" => backup_path(&f.access.documents["owned"], &id),
                "output" => output_path(&f.access.documents["owned"], &id, 0),
                "case_output" => f
                    .target
                    .parent()
                    .unwrap()
                    .join(format!(".YOKE-DOC-{id}-0.NEW")),
                "case_event" => f
                    .journal
                    .join(format!("{}.plan.json", name.to_ascii_uppercase())),
                "unknown_namespace" => f.journal.join("yoke-unknown.json"),
                "foreign_schema" | "foreign_duplicate" => f
                    .journal
                    .join(format!("yoke-binding-migration-{id}.e0001.json")),
                _ => f.journal.join("foreign-unknown.bin"),
            };
            fs::write(&path, &bytes).unwrap();
            assert!(plan_owner_retirement(&f.access, "owned", &d()).is_err());
            assert!(f.apply(&p).is_err());
            assert_eq!(fs::read(&path).unwrap(), bytes);
            assert_eq!(fs::read(f.owner_path()).unwrap(), before);
            assert!(!f.marker_path().exists());
            f.preserve();
        }
    }
    #[test]
    fn an_unmarked_retirement_intent_cannot_be_silently_superseded() {
        let f = Fixture::new();
        let p = f.plan();
        f.interrupt(&p, "owner_retirement_intent");
        assert!(plan_owner_retirement(&f.access, "owned", &d()).is_err());
        let review = f.inspect(&p);
        f.recover(&p, &review.event_id, RecoveryDirection::Rollback)
            .unwrap();
        let fresh = f.plan();
        assert_ne!(fresh.id, p.id);
        f.apply(&fresh).unwrap();
        f.preserve();
    }
    #[test]
    fn migration_archive_and_retirement_markers_and_held_journal_lock_refuse() {
        for kind in ["migration", "history", "owner-retirement"] {
            let f = Fixture::new();
            let p = f.plan();
            let before = fs::read(f.owner_path()).unwrap();
            let marker = f
                .target
                .parent()
                .unwrap()
                .join(format!(".yoke-doc-{kind}-{}.pending.json", p.target));
            fs::write(&marker, b"foreign pending fixture").unwrap();
            assert!(f.apply(&p).is_err());
            assert!(plan_owner_retirement(&f.access, "owned", &d()).is_err());
            assert_eq!(fs::read(&marker).unwrap(), b"foreign pending fixture");
            assert_eq!(fs::read(f.owner_path()).unwrap(), before);
            f.preserve();
        }
        let f = Fixture::new();
        let p = f.plan();
        let lock = state(&f.access, true, &d()).unwrap();
        assert!(f.apply(&p).is_err());
        drop(lock);
        f.apply(&p).unwrap();
        f.preserve();
    }
    #[test]
    fn source_changes_replaced_roots_and_alien_authority_refuse_before_intent() {
        for role in [
            "source_bytes",
            "source_identity",
            "source_root",
            "journal_root",
            "authority",
            "plan_path",
        ] {
            let f = Fixture::new();
            let mut p = f.plan();
            let before = fs::read(f.owner_path()).unwrap();
            match role {
                "source_bytes" => fs::write(&f.target, b"foreign source\r\n").unwrap(),
                "source_identity" => {
                    fs::rename(&f.target, f.target.with_extension("original-fixture")).unwrap();
                    fs::write(&f.target, ORIGINAL).unwrap();
                }
                "source_root" => {
                    fs::rename(&f.source, f.source.with_extension("original-fixture")).unwrap();
                    fs::create_dir(&f.source).unwrap();
                }
                "journal_root" => {
                    fs::rename(&f.journal, f.journal.with_extension("original-fixture")).unwrap();
                    fs::create_dir(&f.journal).unwrap();
                }
                "plan_path" => {
                    p.target_path.push_str("/../foreign");
                    p.id = plan_id(&p);
                }
                _ => {}
            }
            if role == "authority" {
                let other = WriteAuthority::new(&f.alternate).unwrap();
                assert!(other.approve_owner_retirement(&f.access, &p, &d()).is_err());
            } else {
                assert!(f.apply(&p).is_err());
            }
            assert_eq!(fs::read(f.owner_path()).unwrap(), before);
            assert!(!f.marker_path().exists());
            assert!(!plan_path(&f.journal, &retirement_stem(&p)).exists());
            if role == "source_bytes" {
                assert_eq!(fs::read(&f.target).unwrap(), b"foreign source\r\n");
            }
        }
    }
    #[test]
    fn a_replaced_other_selected_parent_invalidates_the_complete_original_selection() {
        let f = Fixture::new();
        let other_parent = f._root.path().join("other-documents");
        fs::create_dir(&other_parent).unwrap();
        let other_file = other_parent.join("other.yaml");
        fs::write(&other_file, b"independent selected document\r\n").unwrap();
        let config = Config {
            documents: vec![
                f.access.documents["owned"].spec.clone(),
                DocumentSpec {
                    id: "other".into(),
                    label: "Other selected document".into(),
                    path: other_file,
                    scope: Scope::Global {},
                    writable: true,
                },
            ],
            journal_root: Some(f.journal.clone()),
            max_document_bytes: MAX_BYTES,
        };
        let access = Access::open(
            &config,
            &[ProjectRoot {
                name: "default".into(),
                path: f.source.clone(),
            }],
            &d(),
        )
        .unwrap();
        let authority = WriteAuthority::new(&access).unwrap();
        let migration = plan_binding_migration(&f.access, &access, "owned", "owned", &d()).unwrap();
        let grant = BindingMigrationAuthority::new(
            &f.access,
            &access,
            "owned",
            "owned",
            &f.authority,
            &authority,
        )
        .unwrap();
        let approved = grant.approve(&f.access, &access, &migration, &d()).unwrap();
        apply_binding_migration(&f.access, &access, &grant, &approved, &migration, &d()).unwrap();
        let p = plan_owner_retirement(&access, "owned", &d()).unwrap();
        let before = fs::read(f.owner_path()).unwrap();
        let approved = authority
            .approve_owner_retirement(&access, &p, &d())
            .unwrap();
        let retained = other_parent.with_extension("original-fixture");
        fs::rename(&other_parent, &retained).unwrap();
        fs::create_dir(&other_parent).unwrap();
        assert!(plan_owner_retirement(&access, "owned", &d()).is_err());
        assert!(apply_owner_retirement(&access, &authority, &approved, &p, &d()).is_err());
        assert_eq!(
            fs::read(retained.join("other.yaml")).unwrap(),
            b"independent selected document\r\n"
        );
        assert_eq!(fs::read(f.owner_path()).unwrap(), before);
        assert!(!f.marker_path().exists());
        f.preserve();
    }
    #[test]
    fn read_only_and_project_document_selections_never_obtain_retirement_authority() {
        let f = Fixture::new();
        let before = fs::read(f.owner_path()).unwrap();
        for project in [false, true] {
            let config = Config {
                documents: vec![DocumentSpec {
                    id: "selection".into(),
                    label: "Selection".into(),
                    path: if project {
                        f.source.join("untouched.rs")
                    } else {
                        f.target.clone()
                    },
                    scope: if project {
                        Scope::Project {
                            project: "default".into(),
                        }
                    } else {
                        Scope::Global {}
                    },
                    writable: project,
                }],
                journal_root: Some(f.journal.clone()),
                max_document_bytes: MAX_BYTES,
            };
            let access = Access::open(
                &config,
                &[ProjectRoot {
                    name: "default".into(),
                    path: f.source.clone(),
                }],
                &d(),
            )
            .unwrap();
            assert!(WriteAuthority::new(&access).is_err());
            assert!(plan_owner_retirement(&access, "selection", &d()).is_err());
        }
        assert!(plan_owner_retirement(&f.access, "../foreign", &d()).is_err());
        assert_eq!(fs::read(f.owner_path()).unwrap(), before);
        f.preserve();
    }
    #[test]
    fn hardlinked_targets_or_owner_metadata_refuse_without_mutating_any_alias() {
        for owner in [false, true] {
            let f = Fixture::new();
            let p = f.plan();
            let path = if owner {
                f.owner_path()
            } else {
                f.target.clone()
            };
            let bytes = fs::read(&path).unwrap();
            let alias = path.with_extension("hardlink-fixture");
            fs::hard_link(&path, &alias).unwrap();
            assert!(f.apply(&p).is_err());
            assert!(plan_owner_retirement(&f.access, "owned", &d()).is_err());
            assert_eq!(fs::read(&path).unwrap(), bytes);
            assert_eq!(fs::read(&alias).unwrap(), bytes);
            assert!(!f.marker_path().exists());
        }
    }
    #[test]
    fn retirement_preserves_distinct_unicode_targets_and_exact_bom_crlf_bytes() {
        let f = Fixture::new();
        let second = f
            .target
            .parent()
            .unwrap()
            .join("settings-i\u{307}-e\u{301}.yaml");
        fs::write(&second, b"# independent\r\n").unwrap();
        let config = Config {
            documents: vec![
                DocumentSpec {
                    id: "first".into(),
                    label: "first".into(),
                    path: f.target.clone(),
                    scope: Scope::Global {},
                    writable: true,
                },
                DocumentSpec {
                    id: "second".into(),
                    label: "second".into(),
                    path: second.clone(),
                    scope: Scope::Global {},
                    writable: true,
                },
            ],
            journal_root: Some(f.journal.clone()),
            max_document_bytes: MAX_BYTES,
        };
        let access = Access::open(
            &config,
            &[ProjectRoot {
                name: "default".into(),
                path: f.source.clone(),
            }],
            &d(),
        )
        .unwrap();
        assert_ne!(
            target_key(&access.documents["first"]).unwrap(),
            target_key(&access.documents["second"]).unwrap()
        );
        // The first fixture owner belongs to a different complete selection;
        // migrate explicitly rather than adopting it through the new config.
        let authority = WriteAuthority::new(&access).unwrap();
        let migration = plan_binding_migration(&f.access, &access, "owned", "first", &d()).unwrap();
        let grant = BindingMigrationAuthority::new(
            &f.access,
            &access,
            "owned",
            "first",
            &f.authority,
            &authority,
        )
        .unwrap();
        let approved = grant.approve(&f.access, &access, &migration, &d()).unwrap();
        apply_binding_migration(&f.access, &access, &grant, &approved, &migration, &d()).unwrap();
        bind_owner(&access, &access.documents["second"], &d()).unwrap();
        let second_owner = owner_path(&access.documents["second"]).unwrap();
        let before = fs::read(&second_owner).unwrap();
        let p = plan_owner_retirement(&access, "first", &d()).unwrap();
        let approved = authority
            .approve_owner_retirement(&access, &p, &d())
            .unwrap();
        apply_owner_retirement(&access, &authority, &approved, &p, &d()).unwrap();
        assert_eq!(fs::read(&f.target).unwrap(), ORIGINAL.as_bytes());
        assert_eq!(fs::read(&second).unwrap(), b"# independent\r\n");
        assert_eq!(fs::read(&second_owner).unwrap(), before);
        require_owner(&access, &access.documents["second"], &d()).unwrap();
    }
    #[test]
    fn deadline_entry_and_byte_budgets_fail_before_intent_or_owner_mutation() {
        for budget_kind in ["deadline", "entries", "bytes"] {
            let f = Fixture::new();
            let p = f.plan();
            let before = fs::read(f.owner_path()).unwrap();
            match budget_kind {
                "entries" => {
                    for n in 0..MAX_ENTRIES - (2 * MAX_EVENTS as usize + 4) + 1 {
                        fs::write(f.journal.join(format!("yoke-padding-{n}.json")), b"").unwrap();
                    }
                }
                "bytes" => {
                    let file = fs::File::create(f.journal.join("yoke-padding.json")).unwrap();
                    file.set_len(MAX_STATE_BYTES - RESERVED_BYTES + 1).unwrap();
                }
                _ => {}
            }
            let approval = f
                .authority
                .approve_owner_retirement(&f.access, &p, &d())
                .unwrap();
            let deadline = if budget_kind == "deadline" {
                Deadline::new(0)
            } else {
                d()
            };
            assert!(
                apply_owner_retirement(&f.access, &f.authority, &approval, &p, &deadline).is_err()
            );
            assert_eq!(fs::read(f.owner_path()).unwrap(), before);
            assert!(!f.marker_path().exists());
            assert!(!plan_path(&f.journal, &retirement_stem(&p)).exists());
            f.preserve();
        }
    }
    #[test]
    fn rehashed_skipped_or_gapped_out_of_capacity_and_noncanonical_events_refuse() {
        for kind in ["skip_writing", "gap", "capacity", "unknown", "duplicate"] {
            let f = Fixture::new();
            let p = f.plan();
            f.interrupt(&p, "owner_retirement_marked");
            let review = f.inspect(&p);
            let mut e = initial(&p);
            e.sequence = if kind == "gap" {
                2
            } else if kind == "capacity" {
                MAX_RETIREMENT_EVENTS
            } else {
                1
            };
            e.previous = Some(e.id.clone());
            e.phase = if kind == "skip_writing" {
                RetirementPhase::Committed
            } else {
                RetirementPhase::Writing
            };
            e.id = event_id(&e);
            let text = serde_json::to_string(&e).unwrap();
            let text = match kind {
                "unknown" => text.replacen('{', "{\"foreign\":true,", 1),
                "duplicate" => text.replacen('{', "{\"schemaVersion\":1,", 1),
                _ => text,
            };
            let path = event_path(&f.journal, &retirement_stem(&p), e.sequence);
            fs::write(&path, text.as_bytes()).unwrap();
            let before = fs::read(f.owner_path()).unwrap();
            assert!(inspect_owner_retirement(&f.access, &p, &d()).is_err());
            assert!(
                f.recover(&p, &review.event_id, RecoveryDirection::Resume)
                    .is_err()
            );
            assert_eq!(fs::read(&path).unwrap(), text.as_bytes());
            assert_eq!(fs::read(f.owner_path()).unwrap(), before);
            f.preserve();
        }
    }
    #[test]
    fn fixed_retry_reserves_last_terminal_event_and_exhaustion_never_rewrites_owner() {
        for exhaust in [false, true] {
            let f = Fixture::new();
            let p = f.plan();
            f.interrupt(&p, "owner_retirement_marked");
            let mut e = initial(&p);
            let last = if exhaust {
                MAX_RETIREMENT_EVENTS - 1
            } else {
                MAX_RETIREMENT_EVENTS - 2
            };
            for _ in 0..last {
                next(
                    &f.journal,
                    &p,
                    &mut e,
                    RetirementPhase::Writing,
                    Some(RecoveryDirection::Resume),
                    &d(),
                )
                .unwrap();
            }
            let before = fs::read(f.owner_path()).unwrap();
            if exhaust {
                assert!(f.recover(&p, &e.id, RecoveryDirection::Resume).is_err());
                assert_eq!(fs::read(f.owner_path()).unwrap(), before);
            } else {
                io::set_document_owner_write_stop(Some(0));
                assert!(f.recover(&p, &e.id, RecoveryDirection::Resume).is_err());
                assert_eq!(f.inspect(&p).event_id, e.id);
                f.recover(&p, &e.id, RecoveryDirection::Resume).unwrap();
                assert_eq!(
                    load_event(&f.access, &p, &f.journal, &d())
                        .unwrap()
                        .sequence,
                    MAX_RETIREMENT_EVENTS - 1
                );
            }
            f.preserve();
        }
    }
}
