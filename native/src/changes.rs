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
    io::{Read, Write},
    path::Path,
    time::{SystemTime, UNIX_EPOCH},
};
#[path = "changes_plan.rs"]
mod checked_plan;
#[path = "changes_dashboard.rs"]
mod dashboard;
pub(crate) use dashboard::DocumentBinding;
pub(crate) use dashboard::{
    ReviewedDocumentJournal, document_journal_page,
    retention_projection as journal_retention_projection, review_document_journal,
};
#[path = "changes_directory.rs"]
mod directory;
#[path = "changes_symbol_delete.rs"]
mod symbol_delete;
pub use directory::DirectoryMove;
pub(crate) use directory::SelectedDirectoryMove;

pub fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock after epoch")
        .as_secs()
}
pub(crate) fn validate_symbol_delete_policies(args: &Value) -> Result<()> {
    for name in ["rename", "replaceSymbol", "insertBefore", "insertAfter"] {
        ensure!(
            args[name].get("emptyPythonBody").is_none()
                && args[name].get("siblingDeclarators").is_none(),
            "Deletion policy belongs to deleteSymbol"
        );
    }
    ensure!(
        args["deleteSymbol"].get("emptyPythonBody").is_none()
            || args["deleteSymbol"].get("siblingDeclarators").is_none(),
        "Symbol deletion policies cannot be combined"
    );
    for (name, choices) in [
        ("emptyPythonBody", ["reject", "pass"]),
        ("siblingDeclarators", ["reject", "preserve"]),
    ] {
        if let Some(policy) = args["deleteSymbol"].get(name) {
            ensure!(
                policy.as_str().is_some_and(|p| choices.contains(&p)),
                "Unknown symbol deletion policy"
            );
            ensure!(
                [
                    "edits",
                    "rename",
                    "replaceSymbol",
                    "insertBefore",
                    "insertAfter",
                    "bulkReplace"
                ]
                .iter()
                .all(|key| args.get(key).is_none()),
                "Symbol deletion policy cannot be mixed with other changes"
            );
        }
    }
    Ok(())
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub directory_move: Option<DirectoryMove>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_line_ending: Option<crate::source_line_endings::ResolvedLineEnding>,
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
    let emergency_persisted = dashboard::record_journal(store, receipt, None).is_ok();
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
    prepare_inner(epoch, store, snapshot, graph, args, None, deadline)
}
pub(crate) fn select_directory_move(
    root: &Path,
    store: &Store,
    snapshot: &Snapshot,
    source: &str,
    destination: &str,
    deadline: &Deadline,
) -> Result<SelectedDirectoryMove> {
    DirectoryMove::select(root, store, snapshot, source, destination, deadline)
}
pub(crate) fn prepare_directory(
    store: &mut Store,
    snapshot: &Snapshot,
    graph: &Graph,
    args: &Value,
    selected: &SelectedDirectoryMove,
    root: &Path,
    deadline: &Deadline,
) -> Result<Plan> {
    selected.record().validate(snapshot, deadline)?;
    selected.record().check_before(root, deadline)?;
    let epoch =
        crate::capture::analyzer_epoch_for_snapshot_with_deadline(store, snapshot, deadline)?;
    prepare_inner(
        epoch,
        store,
        snapshot,
        graph,
        args,
        Some(selected.record()),
        deadline,
    )
}
#[allow(clippy::too_many_arguments)]
fn prepare_inner(
    epoch: analysis::AnalyzerEpoch,
    store: &mut Store,
    snapshot: &Snapshot,
    graph: &Graph,
    args: &Value,
    directory_move: Option<&DirectoryMove>,
    deadline: &Deadline,
) -> Result<Plan> {
    validate_symbol_delete_policies(args)?;
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
                .context("Symbol source absent from snapshot")?;
            let mut delete_text = "";
            let mut separator = None;
            if value.get("emptyPythonBody").is_some() {
                ensure!(
                    file.language == "python",
                    "Empty Python body policy requires Python source"
                );
            }
            if value.get("siblingDeclarators").is_some() {
                ensure!(
                    matches!(
                        file.language.as_str(),
                        "typescript" | "tsx" | "javascript" | "jsx"
                    ),
                    "Sibling declarators policy requires TypeScript or JavaScript source"
                );
            }
            let (start, end) = match operation {
                "before" => (entity.start_byte, entity.start_byte),
                "after" => (entity.end_byte, entity.end_byte),
                "delete"
                    if matches!(
                        file.language.as_str(),
                        "typescript" | "tsx" | "javascript" | "jsx" | "rust" | "python"
                    ) =>
                {
                    let (text, _) = source(store, snapshot, &file.path)?;
                    if value["emptyPythonBody"] == "pass" {
                        let selected = symbol_delete::python_with_empty_body(
                            epoch, file, entity, &text, deadline,
                        )?;
                        delete_text = selected.replacement;
                        (selected.start, selected.end)
                    } else if value["siblingDeclarators"] == "preserve" {
                        let selected = symbol_delete::preserving_declarators(
                            epoch, file, entity, &text, deadline,
                        )?;
                        separator = selected.separator;
                        (selected.start, selected.end)
                    } else {
                        symbol_delete::range(epoch, file, entity, &text, deadline)?
                    }
                }
                _ => (entity.start_byte, entity.end_byte),
            };
            let text = if operation == "delete" {
                delete_text
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
            if let Some((start_byte, end_byte)) = separator {
                edits.push(Edit::Replace {
                    path: file.path.clone(),
                    expected_hash: file.hash.clone(),
                    start_byte,
                    end_byte,
                    text: String::new(),
                });
            }
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
        crate::source_exclusions::check_current_write_scope(store, edit.path(), deadline)?;
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
        let moved_source =
            directory_move.and_then(|directory| directory.source_for_destination(&path));
        let mut mode = before.or(moved_source).map_or(0o644, |f| f.mode);
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
        let after = match (after, store.source_line_ending) {
            (Some(text), Some(ending)) => Some(ending.normalize(text, deadline)?),
            (after, _) => after,
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
                language: if let Some(original) = moved_source {
                    original.language.clone()
                } else {
                    crate::capture::language_for_snapshot(store, snapshot, &path)?
                },
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
    let created = now();
    let mut plan = Plan {
        id: String::new(),
        base_snapshot_id: snapshot.id.clone(),
        result_snapshot_id: result.id.clone(),
        workspace_id: snapshot.workspace_id.clone(),
        created_at: created,
        expires_at: created + 86400,
        files: changed,
        diff,
        warnings,
        directory_move: directory_move.cloned(),
        source_line_ending: store.source_line_ending,
    };
    plan.id = checked_plan::plan_identity(&plan);
    let id = plan.id.clone();
    if let Some(directory) = &plan.directory_move {
        directory.validate_result(snapshot, &result, deadline)?;
    }
    checked_plan::admit_prepared(&plan, snapshot, &result, deadline)?;
    let mut extractions = Vec::new();
    for file in &result.files {
        let text = String::from_utf8(store.read_blob(&file.hash)?)?;
        extractions.push(analysis::extract_at(epoch, file, &text, deadline)?);
    }
    let result_graph = analysis::assemble_at(epoch, &result, extractions, deadline)?;
    store.publish(&result, &result_graph)?;
    crate::capture::inherit_options(store, snapshot, &result)?;
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
pub(crate) fn materialize_plan(
    store: &Store,
    plan: &Plan,
    target: &Path,
    deadline: &Deadline,
) -> Result<()> {
    let snapshot = store.snapshot(&plan.result_snapshot_id)?;
    materialize(store, &snapshot, target, deadline)?;
    if let Some(directory) = &plan.directory_move {
        directory.materialize_directories(target, deadline)?;
        directory.check_materialized(target, &snapshot, deadline)?;
    }
    Ok(())
}
pub(crate) fn plan_directory_integrity(
    plan: &Plan,
    snapshot: &Snapshot,
    root: &Path,
    deadline: &Deadline,
) -> Result<()> {
    if let Some(directory) = &plan.directory_move {
        directory.check_materialized(root, snapshot, deadline)?;
    }
    Ok(())
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
    apply_inner(store, root, args, allow_writes, None, deadline)
}
pub(crate) fn apply_document(
    store: &Store,
    root: &Path,
    args: &Value,
    allow_writes: bool,
    binding: &DocumentBinding,
    deadline: &Deadline,
) -> Result<Receipt> {
    ensure!(
        allow_writes,
        "Dashboard project writes require host permission"
    );
    binding.validate(store)?;
    apply_inner(store, root, args, allow_writes, Some(binding), deadline)
}
fn apply_inner(
    store: &Store,
    root: &Path,
    args: &Value,
    allow_writes: bool,
    binding: Option<&DocumentBinding>,
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
    if let Some(binding) = binding {
        ensure!(
            mode == "workspace",
            "Dashboard document application requires workspace mode"
        );
        let plan = load_checked_plan(store, id, deadline)?;
        binding.check_plan(store, &plan)?;
        dashboard::check_saved_binding(store, &receipt_id, binding)?;
    }
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
        materialize_plan(store, &plan, &target, deadline)?;
        receipt.sandbox_path = Some(target.display().to_string());
        receipt.status = "applied".into();
        store.record("receipt", &receipt_id, &receipt)?;
        return Ok(receipt);
    }
    ensure!(
        allow_writes,
        "Workspace writes require host --allow-writes permission"
    );
    for file in &plan.files {
        crate::source_exclusions::check_current_write_scope(store, &file.path, deadline)?;
    }
    // Never format an already reviewed effect at application time. A newly
    // selected host convention may only admit its exact stored after-bytes.
    if let Some(ending) = store.source_line_ending {
        for file in &plan.files {
            if let Some(hash) = &file.after_hash {
                ensure!(
                    ending.canonical(&store.read_blob(hash)?, deadline)?,
                    "changes_plan_line_ending_mismatch; prepare and review a new plan"
                );
            }
        }
    }
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
    if let Some(directory) = &plan.directory_move {
        directory.check_before(root, deadline)?;
    }
    dashboard::record_journal(store, &receipt, binding)?;
    let result = (|| -> Result<()> {
        if let Some(directory) = &plan.directory_move {
            directory.rename(root, deadline)?;
        }
        for file in &plan.files {
            deadline.check()?;
            let handled = if let Some(directory) = &plan.directory_move {
                directory.apply_file(store, root, file, deadline)?
            } else {
                false
            };
            if !handled {
                ensure!(
                    matches_file(root, file, false)?,
                    "Concurrent source conflict"
                );
                write_file(store, root, file, true)?;
            }
            receipt.applied_paths.push(file.path.clone());
            dashboard::record_journal(store, &receipt, binding)?;
        }
        if let Some(directory) = &plan.directory_move {
            directory.check_after(root, &store.snapshot(&plan.result_snapshot_id)?, deadline)?;
        }
        receipt.status = "applied".into();
        dashboard::record_journal(store, &receipt, binding)?;
        store.record("receipt", &receipt_id, &receipt)?;
        Ok(())
    })();
    if let Err(error) = result {
        return Err(recovery_failure(store, &mut receipt, error));
    }
    Ok(receipt)
}
pub fn recover(store: &Store, root: &Path, journal: &str, allow_writes: bool) -> Result<Receipt> {
    let mut started = false;
    recover_inner(
        store,
        root,
        journal,
        allow_writes,
        None,
        &Deadline::new(30_000),
        &mut started,
    )
    .map(|(receipt, _)| receipt)
}
pub(crate) fn recover_document(
    store: &Store,
    root: &Path,
    review: &ReviewedDocumentJournal,
    allow_writes: bool,
    deadline: &Deadline,
    started: &mut bool,
) -> Result<(Receipt, bool)> {
    let bound = store.inherited_deadline(deadline)?;
    bound.check()?;
    let prior = store.replace_deadline(Some(bound.clone()));
    let result = recover_inner(
        store,
        root,
        &review.receipt().id,
        allow_writes,
        Some(review),
        &bound,
        started,
    );
    store.replace_deadline(prior);
    result
}
fn recovery_matches(
    root: &Path,
    file: &PlannedFile,
    after: bool,
    maximum: Option<usize>,
) -> Result<bool> {
    let Some(maximum) = maximum else {
        return matches_file(root, file, after);
    };
    let expected = if after {
        &file.after_hash
    } else {
        &file.before_hash
    };
    let path = scoped(root, &file.path)?;
    let input = match fs::File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(expected.is_none()),
        Err(error) => return Err(error.into()),
    };
    ensure!(
        input.metadata()?.len() <= maximum as u64,
        "Selected recovery file capacity exceeded"
    );
    let mut bytes = Vec::new();
    input.take(maximum as u64 + 1).read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() <= maximum,
        "Selected recovery file grew beyond capacity"
    );
    Ok(expected
        .as_ref()
        .is_some_and(|hash| crate::model::hash(&bytes) == *hash))
}
fn recover_inner(
    store: &Store,
    root: &Path,
    journal: &str,
    allow_writes: bool,
    review: Option<&ReviewedDocumentJournal>,
    deadline: &Deadline,
    started: &mut bool,
) -> Result<(Receipt, bool)> {
    ensure!(allow_writes, "Recovery requires host write permission");
    deadline.check()?;
    let _lock = lock(store)?;
    let (mut receipt, plan) = if let Some(review) = review {
        let fresh = dashboard::review_document_journal(store, journal, review.binding(), deadline)?;
        ensure!(
            fresh.event_id() == review.event_id()
                && serde_json::to_value(fresh.plan())? == serde_json::to_value(review.plan())?,
            "Workspace recovery review is stale"
        );
        for file in &fresh.plan().files {
            crate::source_exclusions::check_current_write_scope(store, &file.path, deadline)?;
            let maximum = Some(review.binding().max_document_bytes);
            ensure!(
                recovery_matches(root, file, false, maximum)?
                    || recovery_matches(root, file, true, maximum)?,
                "Foreign selected-file changes preserved"
            );
        }
        (fresh.receipt().clone(), fresh.plan().clone())
    } else {
        let receipt: Receipt = store.get_record("journal", journal)?;
        let plan = load_checked_plan(store, &receipt.plan_id, &Deadline::new(30_000))?;
        (receipt, plan)
    };
    ensure!(
        receipt.status != "rolled_back",
        "Journal already rolled back"
    );
    let mut conflicts = Vec::new();
    let mut source_written = false;
    if let Some(directory) = &plan.directory_move {
        let deadline = store.inherited_deadline(&Deadline::new(30_000))?;
        if let Err(error) = directory.recover(store, root, &plan, &deadline) {
            let failure = recovery_failure(store, &mut receipt, error);
            let _ = store.record("receipt", journal, &receipt);
            return Err(failure);
        }
    }
    // Check all planned files, including a write that preceded its journal update.
    for file in plan.files.iter().rev() {
        deadline.check()?;
        if plan
            .directory_move
            .as_ref()
            .is_some_and(|directory| directory.handles(&file.path))
        {
            continue;
        }
        let maximum = review.map(|r| r.binding().max_document_bytes);
        if recovery_matches(root, file, false, maximum)? {
            continue;
        }
        if recovery_matches(root, file, true, maximum)? {
            *started = true;
            write_file(store, root, file, false)?;
            source_written = true;
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
    deadline.check()?;
    *started = true;
    dashboard::record_journal(store, &receipt, None)?;
    store.record("receipt", journal, &receipt)?;
    ensure!(
        conflicts.is_empty(),
        "Foreign edits preserved; manual recovery required for {conflicts:?}"
    );
    Ok((receipt, source_written))
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
