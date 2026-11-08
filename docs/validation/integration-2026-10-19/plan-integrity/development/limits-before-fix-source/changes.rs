use crate::{
    analysis,
    capture::Deadline,
    model::{Graph, Snapshot, SourceFile, identity},
    paths::{is_link, scoped, utf8_range},
    query::source,
    store::Store,
};
use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use similar::TextDiff;
use std::{
    collections::BTreeMap,
    fs,
    io::Write,
    path::Path,
    time::{SystemTime, UNIX_EPOCH},
};
#[path = "changes_plan.rs"]
mod checked_plan;

pub fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock after epoch")
        .as_secs()
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase", deny_unknown_fields)]
pub enum Edit {
    Replace {
        path: String,
        #[serde(rename = "expectedHash")]
        expected_hash: String,
        #[serde(rename = "startByte")]
        start_byte: usize,
        #[serde(rename = "endByte")]
        end_byte: usize,
        text: String,
    },
    Create {
        path: String,
        text: String,
    },
    Delete {
        path: String,
        #[serde(rename = "expectedHash")]
        expected_hash: String,
    },
}
impl Edit {
    fn path(&self) -> &str {
        match self {
            Self::Replace { path, .. } | Self::Create { path, .. } | Self::Delete { path, .. } => {
                path
            }
        }
    }
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PlannedFile {
    pub path: String,
    pub before_hash: Option<String>,
    pub after_hash: Option<String>,
    pub mode: u32,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Plan {
    pub id: String,
    pub base_snapshot_id: String,
    pub result_snapshot_id: String,
    pub workspace_id: String,
    pub created_at: u64,
    pub expires_at: u64,
    pub files: Vec<PlannedFile>,
    pub diff: String,
    pub warnings: Vec<String>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Receipt {
    pub id: String,
    pub plan_id: String,
    pub idempotency_key: String,
    pub mode: String,
    pub status: String,
    pub snapshot_id: String,
    pub changed_paths: Vec<String>,
    pub applied_paths: Vec<String>,
    pub sandbox_path: Option<String>,
}

#[derive(Debug)]
pub struct RecoveryRequired {
    pub journal_id: String,
    pub reason: String,
    pub emergency_persisted: bool,
}
impl std::fmt::Display for RecoveryRequired {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Journal {} requires recovery: {}",
            self.journal_id, self.reason
        )
    }
}
impl std::error::Error for RecoveryRequired {}

fn recovery_failure(store: &Store, receipt: &mut Receipt, error: anyhow::Error) -> anyhow::Error {
    receipt.status = "recovery_required".into();
    // The durable intent remains authoritative even when the shared deadline or
    // database contention prevents an emergency update after a source write.
    let emergency_persisted = store.record("journal", &receipt.id, receipt).is_ok();
    RecoveryRequired {
        journal_id: receipt.id.clone(),
        reason: error.to_string(),
        emergency_persisted,
    }
    .into()
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Approval {
    pub plan_id: String,
    pub approved_at: u64,
    pub allow_test_changes: bool,
}

pub fn prepare(
    store: &mut Store,
    snapshot: &Snapshot,
    graph: &Graph,
    args: &Value,
    deadline: &Deadline,
) -> Result<Plan> {
    let epoch =
        crate::capture::analyzer_epoch_for_snapshot_with_deadline(store, snapshot, deadline)?;
    prepare_at(epoch, store, snapshot, graph, args, deadline)
}
pub fn prepare_at(
    epoch: analysis::AnalyzerEpoch,
    store: &mut Store,
    snapshot: &Snapshot,
    graph: &Graph,
    args: &Value,
    deadline: &Deadline,
) -> Result<Plan> {
    let mut edits: Vec<Edit> = if let Some(value) = args.get("edits") {
        serde_json::from_value(value.clone())?
    } else {
        Vec::new()
    };
    for (name, operation) in [
        ("replaceSymbol", "replace"),
        ("insertBefore", "before"),
        ("insertAfter", "after"),
        ("deleteSymbol", "delete"),
    ] {
        if let Some(value) = args.get(name) {
            let id = value["entityId"]
                .as_str()
                .ok_or_else(|| anyhow::anyhow!("entityId required"))?;
            let entity = graph
                .entities
                .iter()
                .find(|e| e.id == id)
                .ok_or_else(|| anyhow::anyhow!("Unknown symbol"))?;
            ensure!(
                entity.kind != "file",
                "Use explicit file edits for whole files"
            );
            let file = snapshot
                .files
                .iter()
                .find(|f| f.path == entity.path)
                .unwrap();
            let (start, end) = match operation {
                "before" => (entity.start_byte, entity.start_byte),
                "after" => (entity.end_byte, entity.end_byte),
                _ => (entity.start_byte, entity.end_byte),
            };
            let text = if operation == "delete" {
                ""
            } else {
                value["text"]
                    .as_str()
                    .ok_or_else(|| anyhow::anyhow!("Symbol edit text required"))?
            };
            edits.push(Edit::Replace {
                path: file.path.clone(),
                expected_hash: file.hash.clone(),
                start_byte: start,
                end_byte: end,
                text: text.into(),
            });
        }
    }
    if args.get("rename").is_some() {
        bail!(
            "Semantic rename requires a host-selected LSP server; structural text rename is refused"
        );
    }
    ensure!(
        !edits.is_empty() && edits.len() <= 2000,
        "Expected 1..2000 edits"
    );
    store.set_deadline(deadline)?;
    let source_scope = crate::capture::source_scope_policy(store, snapshot)?;
    let mut grouped: BTreeMap<String, Vec<Edit>> = BTreeMap::new();
    for edit in edits {
        crate::paths::normalized(edit.path())?;
        source_scope.check_host_write_scope(edit.path())?;
        grouped.entry(edit.path().into()).or_default().push(edit);
    }
    drop(source_scope);
    let mut files = snapshot.files.clone();
    let mut changed = Vec::new();
    let mut diff = String::new();
    let mut warnings = Vec::new();
    for (path, mut edits) in grouped {
        deadline.check()?;
        let before = snapshot.files.iter().find(|f| f.path == path);
        let old = if before.is_some() {
            source(store, snapshot, &path)?.0
        } else {
            String::new()
        };
        let mut mode = before.map_or(0o644, |f| f.mode);
        let after = match &edits[0] {
            Edit::Create { text, .. } => {
                ensure!(
                    edits.len() == 1 && before.is_none(),
                    "Create conflicts with an existing file or another edit"
                );
                Some(text.clone())
            }
            Edit::Delete { expected_hash, .. } => {
                ensure!(
                    edits.len() == 1 && before.is_some_and(|f| &f.hash == expected_hash),
                    "Delete precondition failed"
                );
                None
            }
            Edit::Replace { .. } => {
                let before = before.ok_or_else(|| anyhow::anyhow!("Replace source missing"))?;
                mode = before.mode;
                edits.sort_by_key(|e| match e {
                    Edit::Replace {
                        start_byte,
                        end_byte,
                        ..
                    } => (*start_byte, *end_byte),
                    _ => (usize::MAX, usize::MAX),
                });
                let mut previous: Option<(usize, usize)> = None;
                for edit in &edits {
                    let Edit::Replace {
                        expected_hash,
                        start_byte,
                        end_byte,
                        ..
                    } = edit
                    else {
                        bail!("Mixed file operations refused");
                    };
                    ensure!(expected_hash == &before.hash, "Expected hash is stale");
                    utf8_range(&old, *start_byte, *end_byte)?;
                    if let Some((start, end)) = previous {
                        ensure!(
                            *start_byte >= end && !(*start_byte == start && end == start),
                            "Overlapping or ambiguous edits"
                        );
                    }
                    previous = Some((*start_byte, *end_byte));
                }
                let mut result = old.clone();
                for edit in edits.iter().rev() {
                    if let Edit::Replace {
                        start_byte,
                        end_byte,
                        text,
                        ..
                    } = edit
                    {
                        result.replace_range(*start_byte..*end_byte, text);
                    }
                }
                Some(result)
            }
        };
        ensure!(
            after
                .as_ref()
                .is_none_or(|s| s.len() <= 4_000_000 && !s.contains('\0')),
            "Change file budget or NUL violated"
        );
        ensure!(
            after
                .as_ref()
                .is_none_or(|text| !crate::paths::obvious_secret(text)),
            "Edited source contains an obvious credential"
        );
        let after_hash = after
            .as_ref()
            .map(|s| store.blob(s.as_bytes()))
            .transpose()?;
        if before.map(|f| &f.hash) == after_hash.as_ref() {
            continue;
        }
        diff.push_str(
            &TextDiff::from_lines(old.as_str(), after.as_deref().unwrap_or(""))
                .unified_diff()
                .context_radius(3)
                .header(
                    if before.is_some() { &path } else { "/dev/null" },
                    if after.is_some() { &path } else { "/dev/null" },
                )
                .to_string(),
        );
        ensure!(diff.len() <= 16_000_000, "Patch diff budget exceeded");
        let folded_path = path.to_ascii_lowercase();
        if folded_path.contains("test") || folded_path.contains("spec") {
            warnings.push(format!(
                "Test-like file modified: {path}; host approval must include test changes"
            ));
        }
        changed.push(PlannedFile {
            path: path.clone(),
            before_hash: before.map(|f| f.hash.clone()),
            after_hash: after_hash.clone(),
            mode,
        });
        files.retain(|f| f.path != path);
        if let (Some(text), Some(hash)) = (after, after_hash) {
            files.push(SourceFile {
                path: path.clone(),
                hash,
                size: text.len(),
                language: crate::capture::language_for_snapshot(store, snapshot, &path)?,
                mode,
            });
        }
    }
    ensure!(!changed.is_empty(), "Patch contains no changes");
    files.sort_by(|a, b| a.path.cmp(&b.path));
    let result = Snapshot {
        id: epoch.derived_snapshot_id(snapshot, &files),
        workspace_id: snapshot.workspace_id.clone(),
        kind: "derived".into(),
        files,
        omissions: snapshot.omissions.clone(),
        capture_consistency: "derived-from-immutable".into(),
    };
    let mut extractions = Vec::new();
    for file in &result.files {
        let text = String::from_utf8(store.read_blob(&file.hash)?)?;
        extractions.push(analysis::extract_at(epoch, file, &text, deadline)?);
    }
    let result_graph = analysis::assemble_at(epoch, &result, extractions, deadline)?;
    store.publish(&result, &result_graph)?;
    crate::capture::inherit_options(store, snapshot, &result)?;
    let created = now();
    let id = identity(&(&snapshot.id, &result.id, &changed, created));
    let plan = Plan {
        id: id.clone(),
        base_snapshot_id: snapshot.id.clone(),
        result_snapshot_id: result.id,
        workspace_id: snapshot.workspace_id.clone(),
        created_at: created,
        expires_at: created + 86400,
        files: changed,
        diff,
        warnings,
    };
    store.record("plan", &id, &plan)?;
    Ok(plan)
}
pub fn materialize(
    store: &Store,
    snapshot: &Snapshot,
    target: &Path,
    deadline: &Deadline,
) -> Result<()> {
    materialize_selected_sources(
        store,
        snapshot,
        target,
        &std::collections::BTreeSet::new(),
        deadline,
    )
}
/// Host-selected omissions affect only this private copy, never captured source.
pub fn materialize_selected_sources(
    store: &Store,
    snapshot: &Snapshot,
    target: &Path,
    excluded: &std::collections::BTreeSet<String>,
    deadline: &Deadline,
) -> Result<()> {
    let scope = crate::capture::source_scope_policy(store, snapshot)?;
    for path in excluded {
        deadline.check()?;
        scope.file(path)?;
    }
    fs::create_dir_all(target)?;
    ensure!(
        !is_link(&fs::symlink_metadata(target)?),
        "Materialization root is a link"
    );
    for file in &snapshot.files {
        deadline.check()?;
        if excluded.contains(&file.path) {
            continue;
        }
        let file = scope.file(&file.path)?;
        let mut path = target.to_path_buf();
        for component in file.path.split('/') {
            path.push(component);
            match fs::symlink_metadata(&path) {
                Ok(metadata) => ensure!(!is_link(&metadata), "Materialized source path is a link"),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(error.into()),
            }
        }
        let bytes = scope.read_source(&file.path)?;
        fs::create_dir_all(path.parent().unwrap())?;
        let mut output = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)?;
        output.write_all(&bytes)?;
        output.sync_all()?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&path, fs::Permissions::from_mode(file.mode))?;
        }
    }
    Ok(())
}
pub fn load_checked_plan(store: &Store, id: &str, deadline: &Deadline) -> Result<Plan> {
    checked_plan::load(store, id, deadline)
}
pub fn approve(store: &Store, id: &str, allow_tests: bool) -> Result<Plan> {
    approve_at(store, id, allow_tests, &Deadline::new(30_000))
}
pub fn approve_at(store: &Store, id: &str, allow_tests: bool, deadline: &Deadline) -> Result<Plan> {
    let deadline = store.inherited_deadline(deadline)?;
    crate::github_app::worker::publication_policy(store, &deadline, || {
        let plan = load_checked_plan(store, id, &deadline)?;
        ensure!(plan.expires_at > now(), "Plan expired");
        ensure!(
            allow_tests || plan.warnings.is_empty(),
            "Test changes need explicit host approval"
        );
        store.record(
            "approval",
            id,
            &Approval {
                plan_id: id.into(),
                approved_at: now(),
                allow_test_changes: allow_tests,
            },
        )?;
        Ok(plan)
    })
}
struct WriterLock {
    file: fs::File,
}
impl Drop for WriterLock {
    fn drop(&mut self) {
        let _ = fs2::FileExt::unlock(&self.file);
    }
}
fn lock(store: &Store) -> Result<WriterLock> {
    let path = store.dir.join("writer.lock");
    if let Ok(metadata) = fs::symlink_metadata(&path) {
        ensure!(!is_link(&metadata), "Writer lock link refused");
    }
    let file = fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(false)
        .open(&path)?;
    fs2::FileExt::try_lock_exclusive(&file).context("Workspace writer lock is held")?;
    Ok(WriterLock { file })
}
fn matches_file(root: &Path, file: &PlannedFile, after: bool) -> Result<bool> {
    let path = scoped(root, &file.path)?;
    let expected = if after {
        &file.after_hash
    } else {
        &file.before_hash
    };
    match (expected, fs::read(&path)) {
        (Some(hash), Ok(bytes)) => Ok(crate::model::hash(bytes) == *hash),
        (None, Err(e)) if e.kind() == std::io::ErrorKind::NotFound => Ok(true),
        (Some(_), Err(e)) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
        (_, Err(e)) => Err(e.into()),
        _ => Ok(false),
    }
}
fn write_file(store: &Store, root: &Path, file: &PlannedFile, after: bool) -> Result<()> {
    let path = scoped(root, &file.path)?;
    let id = if after {
        &file.after_hash
    } else {
        &file.before_hash
    };
    if let Some(id) = id {
        fs::create_dir_all(path.parent().unwrap())?;
        let mut output = tempfile::NamedTempFile::new_in(path.parent().unwrap())?;
        output.write_all(&store.read_blob(id)?)?;
        output.as_file().sync_all()?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            output
                .as_file()
                .set_permissions(fs::Permissions::from_mode(file.mode))?;
        }
        output.persist(&path).map_err(|e| e.error)?;
    } else {
        fs::remove_file(&path)?;
    }
    Ok(())
}
pub fn apply(
    store: &Store,
    root: &Path,
    args: &Value,
    allow_writes: bool,
    deadline: &Deadline,
) -> Result<Receipt> {
    let id = args["planId"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("planId required"))?;
    let key = args["idempotencyKey"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("idempotencyKey required"))?;
    ensure!(
        !key.is_empty()
            && key.len() <= 150
            && key
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || "_.-".contains(c)),
        "Invalid idempotency key"
    );
    let mode = args["mode"].as_str().unwrap_or("isolated");
    ensure!(
        matches!(mode, "isolated" | "workspace"),
        "Invalid apply mode"
    );
    let receipt_id = identity(&(id, key));
    if let Ok(previous) = store.get_record::<Receipt>("receipt", &receipt_id) {
        ensure!(
            previous.mode == mode,
            "Idempotency key reused with different mode"
        );
        ensure!(previous.status == "applied", "Receipt requires recovery");
        return Ok(previous);
    }
    let plan = load_checked_plan(store, id, deadline)?;
    ensure!(
        plan.workspace_id == store.workspace_id && plan.expires_at > now(),
        "Plan scope or expiry invalid"
    );
    let mut receipt = Receipt {
        id: receipt_id.clone(),
        plan_id: id.into(),
        idempotency_key: key.into(),
        mode: mode.into(),
        status: "intent".into(),
        snapshot_id: plan.result_snapshot_id.clone(),
        changed_paths: plan.files.iter().map(|f| f.path.clone()).collect(),
        applied_paths: Vec::new(),
        sandbox_path: None,
    };
    if mode == "isolated" {
        let target = store.dir.join("trees").join(&receipt_id);
        ensure!(
            !target.exists(),
            "Previous materialization exists without receipt; inspect recovery"
        );
        materialize(
            store,
            &store.snapshot(&plan.result_snapshot_id)?,
            &target,
            deadline,
        )?;
        receipt.sandbox_path = Some(target.display().to_string());
        receipt.status = "applied".into();
        store.record("receipt", &receipt_id, &receipt)?;
        return Ok(receipt);
    }
    ensure!(
        allow_writes,
        "Workspace writes require host --allow-writes permission"
    );
    let approval: Approval = store
        .get_record("approval", id)
        .context("Exact plan has not been approved by the host")?;
    ensure!(
        approval.plan_id == id && (plan.warnings.is_empty() || approval.allow_test_changes),
        "Approval mismatch"
    );
    let verification: Value = store
        .get_record("verification", id)
        .context("Executed verification is required")?;
    ensure!(
        verification["status"] == "passed"
            && verification["level"] == "executed"
            && verification["snapshotId"] == plan.result_snapshot_id,
        "Executed verification did not pass for this plan"
    );
    let _lock = lock(store)?;
    let base = store.snapshot(&plan.base_snapshot_id)?;
    let current = crate::capture::capture_for_snapshot(root, store, &base, deadline)?;
    ensure!(
        current.id == plan.base_snapshot_id,
        "Workspace snapshot is stale"
    );
    for file in &plan.files {
        ensure!(
            matches_file(root, file, false)?,
            "Workspace precondition failed: {}",
            file.path
        );
    }
    store.record("journal", &receipt_id, &receipt)?;
    let result = (|| -> Result<()> {
        for file in &plan.files {
            deadline.check()?;
            ensure!(
                matches_file(root, file, false)?,
                "Concurrent source conflict"
            );
            write_file(store, root, file, true)?;
            receipt.applied_paths.push(file.path.clone());
            store.record("journal", &receipt_id, &receipt)?;
        }
        receipt.status = "applied".into();
        store.record("journal", &receipt_id, &receipt)?;
        store.record("receipt", &receipt_id, &receipt)?;
        Ok(())
    })();
    if let Err(error) = result {
        return Err(recovery_failure(store, &mut receipt, error));
    }
    Ok(receipt)
}
pub fn recover(store: &Store, root: &Path, journal: &str, allow_writes: bool) -> Result<Receipt> {
    ensure!(allow_writes, "Recovery requires host write permission");
    let _lock = lock(store)?;
    let mut receipt: Receipt = store.get_record("journal", journal)?;
    let plan = load_checked_plan(store, &receipt.plan_id, &Deadline::new(30_000))?;
    ensure!(
        receipt.status != "rolled_back",
        "Journal already rolled back"
    );
    let mut conflicts = Vec::new();
    // Check all planned files, including a write that preceded its journal update.
    for file in plan.files.iter().rev() {
        if matches_file(root, file, false)? {
            continue;
        }
        if matches_file(root, file, true)? {
            write_file(store, root, file, false)?;
        } else {
            conflicts.push(file.path.clone());
        }
    }
    receipt.status = if conflicts.is_empty() {
        "rolled_back"
    } else {
        "recovery_required"
    }
    .into();
    store.record("journal", journal, &receipt)?;
    store.record("receipt", journal, &receipt)?;
    ensure!(
        conflicts.is_empty(),
        "Foreign edits preserved; manual recovery required for {conflicts:?}"
    );
    Ok(receipt)
}
pub fn verify_syntax(store: &Store, plan: &Plan) -> Result<Value> {
    let checked = load_checked_plan(store, &plan.id, &Deadline::new(30_000))?;
    ensure!(
        serde_json::to_value(&checked)? == serde_json::to_value(plan)?,
        "changes_plan_invalid"
    );
    let before = store.graph(&plan.base_snapshot_id)?;
    let after = store.graph(&plan.result_snapshot_id)?;
    let mut baseline: BTreeMap<_, usize> = BTreeMap::new();
    for diagnostic in &before.diagnostics {
        *baseline
            .entry((&diagnostic.path, &diagnostic.message, &diagnostic.producer))
            .or_default() += 1;
    }
    let new: Vec<_> = after
        .diagnostics
        .iter()
        .filter(|diagnostic| {
            let key = (&diagnostic.path, &diagnostic.message, &diagnostic.producer);
            if let Some(count) = baseline.get_mut(&key)
                && *count > 0
            {
                *count -= 1;
                return false;
            }
            true
        })
        .collect();
    let value = json!({"status":if new.is_empty(){"passed"}else{"failed"},"level":"syntax-only","snapshotId":plan.result_snapshot_id,"newDiagnostics":new,"limitations":["Syntax checks are not compiler diagnostics or behavioral verification and do not authorize workspace application."]});
    store.record("syntax-verification", &plan.id, &value)?;
    Ok(value)
}

#[cfg(test)]
mod recovery_tests {
    use super::*;
    #[test]
    fn expired_emergency_write_preserves_the_durable_recovery_identifier() {
        let root = tempfile::tempdir().unwrap();
        let store = Store::open(root.path()).unwrap();
        let mut receipt = Receipt {
            id: "a".repeat(64),
            plan_id: "b".repeat(64),
            idempotency_key: "crash".into(),
            mode: "workspace".into(),
            status: "intent".into(),
            snapshot_id: "c".repeat(64),
            changed_paths: vec!["created.rs".into()],
            applied_paths: Vec::new(),
            sandbox_path: None,
        };
        store.record("journal", &receipt.id, &receipt).unwrap();
        fs::write(root.path().join("created.rs"), "fn new_source() {}\n").unwrap();
        receipt.applied_paths.push("created.rs".into());
        let deadline = Deadline::new(10);
        store.set_deadline(&deadline).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(20));
        let error = recovery_failure(
            &store,
            &mut receipt,
            anyhow::anyhow!("Simulated journal update deadline"),
        );
        let recovery = error
            .downcast_ref::<RecoveryRequired>()
            .expect("Structured recovery error required");
        assert_eq!(recovery.journal_id, "a".repeat(64));
        assert!(!recovery.emergency_persisted);
        let durable: Receipt = store.get_record("journal", &receipt.id).unwrap();
        assert_eq!(durable.status, "intent");
        assert!(root.path().join("created.rs").exists());
        assert!(error.to_string().contains(&receipt.id));
    }
}
