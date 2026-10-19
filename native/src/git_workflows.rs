//! Source-bound, read-only Git workflow analysis and explicit cache seeding.
use crate::{
    Engine,
    capture::Deadline,
    model::{self, Graph, Omission, Snapshot, SourceFile, hash, identity},
    paths::{is_link, normalized},
    process::{CaptureLimits, run_capture},
};
use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::Digest;
use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    io::Read,
    path::{Path, PathBuf},
    process::Command,
    time::Duration,
};
pub const SOURCE_KIND: &str = "native_git_source_v1";
pub const REPORT_KIND: &str = "native_git_workflow_v1";
#[path = "git_affected.rs"]
pub mod affected_details;
#[path = "git_areas.rs"]
pub mod areas;
#[path = "git_conflicts.rs"]
pub mod conflicts;
#[path = "git_history_read.rs"]
pub(crate) mod history_read;
pub use history_read::Options as HistoryOptions;
#[path = "git_impact_details.rs"]
pub mod impact_details;
#[path = "git_source_read.rs"]
pub(crate) mod source_read;
pub(crate) fn validate_retention_record(
    store: &crate::store::Store,
    kind: &str,
    id: &str,
    value: &Value,
    deadline: &Deadline,
) -> Result<Option<Vec<String>>> {
    if ![SOURCE_KIND, REPORT_KIND].contains(&kind) {
        return Ok(None);
    }
    let maximum = if kind == SOURCE_KIND {
        16_000_000
    } else {
        64000
    };
    ensure!(
        record_value(store, kind, id, maximum, deadline)? == *value,
        "Git retention record changed"
    );
    if kind == SOURCE_KIND {
        let proof = validate_source(store, id, deadline)?;
        ensure!(proof == *value, "Git source retention receipt mismatch");
        return Ok(Some(vec![
            proof["commitArtifactId"]
                .as_str()
                .context("Git commit artifact missing")?
                .to_owned(),
        ]));
    }
    let report = load(store, id, deadline)?;
    let mut artifacts = BTreeSet::from([id.to_owned()]);
    for key in ["baseSnapshotId", "headSnapshotId"] {
        if key == "headSnapshotId" && report["headMode"] != "commit" {
            continue;
        }
        let snapshot = report[key].as_str().context("Git report source missing")?;
        let proof = validate_source(store, snapshot, deadline)?;
        artifacts.insert(
            proof["commitArtifactId"]
                .as_str()
                .context("Git commit artifact missing")?
                .to_owned(),
        );
    }
    Ok(Some(artifacts.into_iter().collect()))
}
pub fn seed_worktree_cache(
    engine: &mut Engine,
    git: &HostGit,
    parent_root: &Path,
    parent_workspace_id: &str,
    deadline: &Deadline,
) -> Result<Value> {
    use rusqlite::{OpenFlags, OptionalExtension};
    deadline.check()?;
    ensure!(
        parent_root.is_absolute() && model::valid_id(parent_workspace_id),
        "Explicit absolute parent root and workspace identity required"
    );
    ensure!(
        !is_link(&std::fs::symlink_metadata(parent_root)?),
        "Parent worktree cannot be a link"
    );
    let parent_root = parent_root.canonicalize()?;
    ensure!(
        parent_root != engine.root.canonicalize()?,
        "Parent and target worktrees must differ"
    );
    let common = |root: &Path| -> Result<PathBuf> {
        let bytes = git.run(
            root,
            &["rev-parse", "--path-format=absolute", "--git-common-dir"],
            &[],
            4096,
            deadline,
        )?;
        let text = std::str::from_utf8(&bytes)?.trim();
        let path = PathBuf::from(text);
        ensure!(path.is_absolute(), "Git common directory must be absolute");
        Ok(path.canonicalize()?)
    };
    ensure!(
        common(&parent_root)? == common(&engine.root)?,
        "Parent worktree belongs to a different Git repository"
    );
    let state = parent_root.join(".yoke");
    ensure!(
        !is_link(&std::fs::symlink_metadata(&state)?) && state.is_dir(),
        "Parent state must be a local directory"
    );
    for name in [
        "index.sqlite",
        "index.sqlite-wal",
        "index.sqlite-shm",
        "index.sqlite-journal",
    ] {
        if let Ok(meta) = std::fs::symlink_metadata(state.join(name)) {
            ensure!(
                !is_link(&meta) && meta.is_file(),
                "Parent database or sidecar cannot be a link"
            );
        }
    }
    let parent = rusqlite::Connection::open_with_flags(
        state.join("index.sqlite"),
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )?;
    // SQLite can materialize an entire cell even for length(CAST(value AS BLOB)).
    // This private connection is dropped after seeding; cap VDBE cells first.
    let parent_limit = parent
        .limit(rusqlite::limits::Limit::SQLITE_LIMIT_LENGTH)?
        .min(8_000_000);
    parent.set_limit(rusqlite::limits::Limit::SQLITE_LIMIT_LENGTH, parent_limit)?;
    parent.busy_timeout(deadline.remaining().min(Duration::from_secs(1)))?;
    let sql_end = std::time::Instant::now()
        .checked_add(deadline.remaining())
        .context("Git cache SQL deadline overflow")?;
    parent.progress_handler(1000, Some(move || std::time::Instant::now() >= sql_end));
    parent.pragma_update(None, "query_only", true)?;
    let workspace: String =
        parent.query_row("SELECT value FROM meta WHERE key='workspace'", [], |row| {
            row.get(0)
        })?;
    ensure!(
        workspace == parent_workspace_id,
        "Parent workspace identity mismatch"
    );
    engine.store.set_deadline(deadline)?;
    let epoch = engine.capture_epoch;
    let snapshot = crate::capture::capture_with_options_at(
        epoch,
        &engine.root,
        &engine.store,
        &engine.capture_options,
        deadline,
    )?;
    let mut entries = vec![];
    let mut total = 0usize;
    for file in &snapshot.files {
        deadline.check()?;
        let key = epoch.cache_key(file);
        let length: Option<usize> = parent
            .query_row(
                "SELECT length(CAST(value AS BLOB)) FROM cache WHERE key=?1",
                [&key],
                |row| row.get(0),
            )
            .optional()?;
        let Some(length) = length else { continue };
        ensure!(
            length <= 8_000_000,
            "Parent parser entry byte capacity exceeded"
        );
        total = total
            .checked_add(length)
            .context("Parent parser byte overflow")?;
        ensure!(
            total <= 32_000_000,
            "Parent parser aggregate capacity exceeded"
        );
        let raw: String =
            parent.query_row("SELECT value FROM cache WHERE key=?1", [&key], |row| {
                row.get(0)
            })?;
        ensure!(
            raw.len() == length,
            "Parent parser entry changed during read"
        );
        let candidate: model::Extraction =
            crate::json::decode(crate::json::parse(raw.as_bytes(), 8_000_000)?)?;
        let source = cas_bytes(&engine.store, &file.hash, file.size, 4_000_000, deadline)?;
        let verified =
            crate::analysis::extract_at(epoch, file, std::str::from_utf8(&source)?, deadline)?;
        ensure!(
            serde_json::to_value(&candidate)? == serde_json::to_value(&verified)?,
            "Parent parser entry is not the current source-derived extraction"
        );
        entries.push((key, verified));
    }
    engine.store.put_caches(&entries, deadline)?;
    let (snapshot, _, parsed, reused) = engine.analyze_snapshot_at(epoch, snapshot, deadline)?;
    Ok(
        json!({"schemaVersion":1,"parentWorkspaceId":workspace,"targetWorkspaceId":engine.store.workspace_id,"targetSnapshotId":snapshot.id,"analyzer":epoch.producer(),"copiedParserEntries":entries.len(),"verifiedParserBytes":total,"parsedFiles":parsed,"reusedFiles":reused,"copiedStateKinds":["source_verified_parser_cache"],"trust":"reconstructed_from_target_source","limitations":["Parser verification reparses matching inputs; this receipt is not a performance benchmark.","No parent snapshots, graphs, records, leases, provider state or mutable session state are copied."]}),
    )
}
#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct WorkflowRecord {
    artifact_id: String,
    workspace_id: String,
    base_snapshot_id: String,
    head_snapshot_id: String,
    origin_snapshot_id: String,
    base_graph_hash: String,
    head_graph_hash: String,
    producer: String,
}
#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ReportData {
    schema_version: u8,
    workspace_id: String,
    base_snapshot_id: String,
    head_snapshot_id: String,
    origin_snapshot_id: String,
    base_graph_hash: String,
    head_graph_hash: String,
    base_commit: String,
    head_commit: String,
    head_mode: String,
    options: WorkflowOptions,
    changes: Vec<Value>,
    changed_symbols: Vec<Value>,
    impact: Vec<Value>,
    impact_observations: Vec<Value>,
    owner_suggestions: Vec<Value>,
    branches: Vec<Value>,
    history: Vec<Value>,
    complete: bool,
    limitations: Vec<String>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct HistoricalMetadata {
    commit_id: String,
    author: String,
    email: String,
    date: String,
    subject: String,
    paths: Vec<String>,
    trust: String,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct BranchMetadata {
    #[serde(rename = "ref")]
    reference: String,
    tip_commit: String,
    merge_base_commit: Option<String>,
    #[serde(default)]
    relationship: Option<String>,
    author_candidate: String,
    tip_date: String,
    changed_paths: Vec<String>,
    overlapping_paths: Vec<String>,
    source: String,
    remote_freshness: String,
    trust: String,
}
fn bounded_metadata(text: &str) -> Result<()> {
    ensure!(
        text.len() <= 16000
            && !text.chars().any(char::is_control)
            && !crate::paths::obvious_secret(text),
        "Unsafe or oversized Git metadata field"
    );
    Ok(())
}
fn validate_metadata(
    report: &ReportData,
    changed: &[String],
    scope: &crate::capture::CapturedSourceScope<'_>,
    deadline: &Deadline,
) -> Result<()> {
    ensure!(
        report.options.impact_depth <= 8
            && (1..=20).contains(&report.options.owner_limit)
            && (1..=1000).contains(&report.options.history_limit)
            && report.options.branch_limit <= 100
            && report.history.len() <= report.options.history_limit
            && report.branches.len() <= report.options.branch_limit,
        "Invalid Git report option capacities"
    );
    let mut commits = BTreeSet::new();
    for value in &report.history {
        deadline.check()?;
        let row: HistoricalMetadata = crate::json::decode(value.clone())?;
        ensure!(
            oid_valid(&row.commit_id)
                && commits.insert(row.commit_id)
                && row.trust == "observed_commit_metadata_not_verified_authorship"
                && row.paths.len() <= 10000,
            "Invalid historical Git metadata claim"
        );
        for text in [&row.author, &row.email, &row.date, &row.subject] {
            bounded_metadata(text)?;
        }
        let mut paths = BTreeSet::new();
        for path in row.paths {
            scope.prospective_path_policy(&path)?;
            ensure!(paths.insert(path), "Duplicate historical Git path");
        }
    }
    let mut refs = BTreeSet::new();
    for value in &report.branches {
        deadline.check()?;
        let row: BranchMetadata = crate::json::decode(value.clone())?;
        ensure!(
            (row.reference.starts_with("refs/heads/")
                || row.reference.starts_with("refs/remotes/"))
                && refs.insert(row.reference.clone())
                && oid_valid(&row.tip_commit)
                && row.source == "observed_local_refs_only"
                && row.remote_freshness == "unverified_no_fetch"
                && row.changed_paths.len() <= 10000,
            "Invalid branch metadata claim"
        );
        for text in [&row.reference, &row.author_candidate, &row.tip_date] {
            bounded_metadata(text)?;
        }
        let mut paths = BTreeSet::new();
        for path in &row.changed_paths {
            scope.prospective_path_policy(path)?;
            ensure!(paths.insert(path), "Duplicate branch path");
        }
        let expected: Vec<_> = changed
            .iter()
            .filter(|path| paths.contains(path))
            .cloned()
            .collect();
        ensure!(
            row.overlapping_paths == expected,
            "Branch overlap does not match captured changed paths"
        );
        match row.merge_base_commit {
            Some(oid) => ensure!(
                oid_valid(&oid)
                    && row.relationship.is_none()
                    && row.trust == "shared_path_candidate_not_verified_collaboration",
                "Invalid related branch claim"
            ),
            None => ensure!(
                row.relationship.as_deref() == Some("unrelated_history")
                    && row.changed_paths.is_empty()
                    && row.trust == "unrelated_history_no_overlap_proof",
                "Invalid unrelated branch claim"
            ),
        }
    }
    Ok(())
}
pub fn load(store: &crate::store::Store, id: &str, deadline: &Deadline) -> Result<Value> {
    deadline.check()?;
    store.set_deadline(deadline)?;
    ensure!(model::valid_id(id), "Invalid Git report artifact ID");
    let record: WorkflowRecord =
        crate::json::decode(record_value(store, REPORT_KIND, id, 64000, deadline)?)?;
    ensure!(
        record.artifact_id == id
            && record.workspace_id == store.workspace_id
            && record.producer == "yoke-native-git-workflow-1",
        "Git report publication scope mismatch"
    );
    let bytes = artifact_bytes(store, id, 16_000_000, deadline)?;
    let report: ReportData = crate::json::decode(crate::json::parse(&bytes, 16_000_000)?)?;
    ensure!(
        report.schema_version == 1
            && !report.complete
            && report.workspace_id == record.workspace_id
            && report.base_snapshot_id == record.base_snapshot_id
            && report.head_snapshot_id == record.head_snapshot_id
            && report.origin_snapshot_id == record.origin_snapshot_id
            && report.base_graph_hash == record.base_graph_hash
            && report.head_graph_hash == record.head_graph_hash
            && oid_valid(&report.base_commit)
            && oid_valid(&report.head_commit),
        "Git report artifact binding mismatch"
    );
    let base = store.snapshot(&report.base_snapshot_id)?;
    let head = store.snapshot(&report.head_snapshot_id)?;
    let base_graph = store.graph(&base.id)?;
    let head_graph = store.graph(&head.id)?;
    crate::graph_extensions::validate_scope(store, &base, &base_graph, deadline)?;
    crate::graph_extensions::validate_scope(store, &head, &head_graph, deadline)?;
    ensure!(
        identity(&base_graph) == report.base_graph_hash
            && identity(&head_graph) == report.head_graph_hash,
        "Git report graph binding mismatch"
    );
    let proof = validate_source(store, &base.id, deadline)?;
    ensure!(
        proof["commitId"] == report.base_commit
            && proof["originSnapshotId"] == report.origin_snapshot_id,
        "Git report base source proof mismatch"
    );
    match report.head_mode.as_str() {
        "commit" => {
            let proof = validate_source(store, &head.id, deadline)?;
            ensure!(
                proof["commitId"] == report.head_commit
                    && proof["originSnapshotId"] == report.origin_snapshot_id,
                "Git report head source proof mismatch"
            );
        }
        "captured_working_tree" => ensure!(
            head.id == report.origin_snapshot_id,
            "Git report working capture mismatch"
        ),
        _ => bail!("Unsupported Git report head mode"),
    }
    ensure!(
        report.changes.len() <= 10000
            && report.changed_symbols.len() <= 40000
            && report.impact.len() <= 40000
            && report.impact_observations.len() <= 100000
            && report.branches.len() <= 100
            && report.history.len() <= 1000
            && report.limitations.len() <= 100,
        "Git report collection capacity exceeded"
    );
    for node in report.changed_symbols.iter().chain(&report.impact) {
        deadline.check()?;
        let (snapshot, graph) = match node["side"].as_str() {
            Some("base") => (&base, &base_graph),
            Some("head") => (&head, &head_graph),
            _ => bail!("Invalid Git impact source side"),
        };
        let entity = graph
            .entities
            .iter()
            .find(|entity| node["id"] == entity.id)
            .context("Git report source entity absent")?;
        let mut normalized = node.clone();
        if let Some(fields) = normalized.as_object_mut() {
            fields.remove("distance");
            fields.remove("observationId");
        }
        ensure!(
            normalized == node_claim(entity, snapshot, node["side"].as_str().unwrap()),
            "Git report source entity claim mismatch"
        );
    }
    for arc in &report.impact_observations {
        let graph = match arc["side"].as_str() {
            Some("base") => &base_graph,
            Some("head") => &head_graph,
            _ => bail!("Invalid Git observation side"),
        };
        let original = graph
            .observations
            .iter()
            .find(|edge| arc["id"] == edge.id)
            .context("Git impact observation absent")?;
        let mut expected = serde_json::to_value(original)?;
        expected["side"] = arc["side"].clone();
        expected["evidence"] = json!(
            graph
                .evidence
                .iter()
                .filter(|anchor| original.evidence_ids.contains(&anchor.id))
                .collect::<Vec<_>>()
        );
        ensure!(&expected == arc, "Git impact observation/evidence mismatch");
    }
    let base_scope = crate::capture::source_scope_policy(store, &base)?;
    let head_scope = crate::capture::source_scope_policy(store, &head)?;
    for change in &report.changes {
        for (side, snapshot, scope) in [("base", &base, &base_scope), ("head", &head, &head_scope)]
        {
            let key = if side == "base" {
                "basePath"
            } else {
                "headPath"
            };
            let path = change[key].as_str();
            if let Some(path) = path {
                scope.path_policy(path)?;
                ensure!(
                    snapshot.files.iter().any(|file| file.path == path),
                    "Git changed path outside captured source"
                );
            }
            let hunks = change["hunks"]
                .as_array()
                .context("Git change hunks missing")?;
            ensure!(hunks.len() <= 100000, "Git hunk capacity exceeded");
            for hunk in hunks {
                let anchor = &hunk[side];
                if path.is_none() {
                    ensure!(anchor.is_null(), "Absent Git source cannot carry an anchor");
                    continue;
                }
                let path = path.unwrap();
                let (text, hash) = crate::query::source(store, snapshot, path)?;
                let start = anchor["startByte"]
                    .as_u64()
                    .context("Git hunk start missing")? as usize;
                let end = anchor["endByte"].as_u64().context("Git hunk end missing")? as usize;
                crate::paths::utf8_range(&text, start, end)?;
                ensure!(
                    anchor["snapshotId"] == snapshot.id
                        && anchor["path"] == path
                        && anchor["contentHash"] == hash
                        && anchor["startLine"]
                            == text[..start].bytes().filter(|b| *b == b'\n').count() + 1
                        && anchor["anchor"] == "exact_source_range",
                    "Git hunk source binding mismatch"
                );
            }
        }
    }
    let rebuilt = native_impact(
        store,
        &base,
        &base_graph,
        &head,
        &head_graph,
        &report.options,
        deadline,
    )?;
    validate_metadata(&report, &rebuilt.changed_paths, &head_scope, deadline)?;
    let mut owners = owner_rules(
        store,
        &head,
        &rebuilt.changed_paths,
        report.options.owner_limit,
        deadline,
    )?;
    owners.extend(history_owners(
        &report.history,
        &rebuilt.changed_paths,
        report.options.owner_limit,
    ));
    ensure!(
        owners == report.owner_suggestions,
        "Git owner suggestions do not match source rules/history candidates"
    );
    let observations: Vec<_> = rebuilt.impact_observations.into_values().collect();
    ensure!(
        report.changes == rebuilt.changes
            && report.changed_symbols == rebuilt.changed
            && report.impact == rebuilt.impacts
            && report.impact_observations == observations,
        "Git report native diff/impact reconstruction mismatch"
    );
    let mut value = serde_json::to_value(report)?;
    value["artifactId"] = json!(id);
    deadline.check()?;
    Ok(value)
}
fn checked_report(store: &crate::store::Store, report: &Value, deadline: &Deadline) -> Result<()> {
    let id = report["artifactId"]
        .as_str()
        .context("Git report identity missing")?;
    ensure!(
        load(store, id, deadline)? == *report,
        "Git report differs from immutable publication"
    );
    Ok(())
}
fn escape_markup(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}
pub fn render(
    store: &crate::store::Store,
    report: &Value,
    format: &str,
    deadline: &Deadline,
) -> Result<String> {
    checked_report(store, report, deadline)?;
    let source = serde_json::to_string_pretty(report)?;
    let result = match format {
        "json" => source,
        "text" => format!(
            "Git structural impact report {}\nBase {}\nHead {}\n\n{}",
            report["artifactId"].as_str().unwrap(),
            report["baseCommit"],
            report["headCommit"],
            source
        ),
        "markdown" => format!(
            "# Git structural impact\n\nReport `{}`\n\nSource-bound candidates; runtime and verified ownership remain unproven.\n\n{}",
            report["artifactId"].as_str().unwrap(),
            source
                .lines()
                .map(|line| format!("    {line}\n"))
                .collect::<String>()
        ),
        "html" => format!(
            "<!doctype html><html><head><meta charset=\"utf-8\"><title>Git structural impact</title></head><body><h1>Git structural impact</h1><pre>{}</pre></body></html>",
            escape_markup(&source)
        ),
        "mermaid" => {
            let mut text = String::from("flowchart LR\n");
            let mut seen = BTreeSet::new();
            for node in report["changedSymbols"]
                .as_array()
                .unwrap()
                .iter()
                .chain(report["impact"].as_array().unwrap())
            {
                deadline.check()?;
                let id = node["id"].as_str().unwrap();
                if seen.insert(id) {
                    text.push_str(&format!(
                        "  n{id}[\"{}: {}\"]\n",
                        escape_markup(node["side"].as_str().unwrap()),
                        escape_markup(node["name"].as_str().unwrap()).replace('"', "#quot;")
                    ));
                }
            }
            for arc in report["impactObservations"].as_array().unwrap() {
                text.push_str(&format!(
                    "  n{} --> n{}\n",
                    arc["source"].as_str().unwrap(),
                    arc["target"]
                        .as_str()
                        .context("Git impact target missing")?
                ));
            }
            text
        }
        _ => bail!("Unsupported Git report rendering format"),
    };
    ensure!(
        result.len() <= 32_000_000,
        "Git rendering byte capacity exceeded"
    );
    deadline.check()?;
    Ok(result)
}
pub fn digest(
    store: &crate::store::Store,
    report: &Value,
    kind: &str,
    path: Option<&str>,
    deadline: &Deadline,
) -> Result<Value> {
    checked_report(store, report, deadline)?;
    let data = match kind {
        "report" => {
            json!({"baseCommit":report["baseCommit"],"headCommit":report["headCommit"],"changedFiles":report["changes"].as_array().unwrap().len(),"changedSymbols":report["changedSymbols"],"impact":report["impact"],"ownerSuggestions":report["ownerSuggestions"],"branches":report["branches"]})
        }
        "history" => {
            json!({"commits":report["history"],"source":"bounded_observed_local_history","authorTrust":"unverified"})
        }
        "file" => {
            let path = crate::paths::normalized_syntax(path.context("File digest path required")?)?;
            let base = store.snapshot(report["baseSnapshotId"].as_str().unwrap())?;
            let head = store.snapshot(report["headSnapshotId"].as_str().unwrap())?;
            let before = base.files.iter().find(|file| file.path == path);
            let after = head.files.iter().find(|file| file.path == path);
            ensure!(
                before.is_some() || after.is_some(),
                "File digest path outside report snapshots"
            );
            if before.is_some() {
                crate::capture::source_path_policy(store, &base, &path)?;
            }
            if after.is_some() {
                crate::capture::source_path_policy(store, &head, &path)?;
            }
            json!({"path":path,"base":before,"head":after,"changes":report["changes"].as_array().unwrap().iter().filter(|change|change["basePath"]==path||change["headPath"]==path).collect::<Vec<_>>(),"history":report["history"].as_array().unwrap().iter().filter(|commit|commit["paths"].as_array().is_some_and(|paths|paths.iter().any(|item|item==&path))).collect::<Vec<_>>()})
        }
        _ => bail!("Unsupported Git digest kind"),
    };
    let mut value = json!({"kind":kind,"reportArtifactId":report["artifactId"],"workspaceId":report["workspaceId"],"baseSnapshotId":report["baseSnapshotId"],"headSnapshotId":report["headSnapshotId"],"data":data,"complete":false});
    value["id"] = json!(identity(&value));
    deadline.check()?;
    Ok(value)
}
#[derive(Clone, Debug)]
pub struct HostGit {
    pub executable: PathBuf,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkflowOptions {
    pub base_ref: String,
    #[serde(default)]
    pub head_ref: Option<String>,
    #[serde(default)]
    pub merge_base: bool,
    #[serde(default = "depth")]
    pub impact_depth: usize,
    #[serde(default = "owners")]
    pub owner_limit: usize,
    #[serde(default = "default_history")]
    pub history_limit: usize,
    #[serde(default = "branches")]
    pub branch_limit: usize,
}
fn depth() -> usize {
    2
}
fn owners() -> usize {
    3
}
fn default_history() -> usize {
    100
}
fn branches() -> usize {
    20
}
impl Default for WorkflowOptions {
    fn default() -> Self {
        Self {
            base_ref: "HEAD".into(),
            head_ref: None,
            merge_base: false,
            impact_depth: depth(),
            owner_limit: owners(),
            history_limit: default_history(),
            branch_limit: branches(),
        }
    }
}
impl HostGit {
    /// Resolve host PATH entries only; never search the repository's current directory.
    pub fn discover() -> Result<Self> {
        for dir in std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default()) {
            if !dir.is_absolute() {
                continue;
            }
            let path = dir.join(if cfg!(windows) { "git.exe" } else { "git" });
            if path.is_file() && !is_link(&std::fs::symlink_metadata(&path)?) {
                return Ok(Self {
                    executable: path.canonicalize()?,
                });
            }
        }
        bail!("Host Git executable unavailable")
    }
    fn run(
        &self,
        root: &Path,
        args: &[&str],
        input: &[u8],
        limit: usize,
        deadline: &Deadline,
    ) -> Result<Vec<u8>> {
        deadline.check()?;
        ensure!(
            self.executable.is_absolute()
                && self.executable.is_file()
                && !is_link(&std::fs::symlink_metadata(&self.executable)?)
                && !self.executable.canonicalize()?.starts_with(root),
            "Git executable must be host-selected outside repository"
        );
        let mut command = Command::new(&self.executable);
        command.current_dir(root).args([
            "--no-replace-objects",
            "--no-lazy-fetch",
            "--no-pager",
            "-c",
            "core.fsmonitor=false",
            "-c",
            "core.hooksPath=",
            "-c",
            "diff.external=",
            "-c",
            "log.showSignature=false",
            "-c",
            "submodule.recurse=false",
            "-c",
            "core.quotePath=false",
            "-c",
            "protocol.allow=never",
        ]);
        let null = if cfg!(windows) { "NUL" } else { "/dev/null" };
        command
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_SYSTEM", null)
            .env("GIT_CONFIG_GLOBAL", null)
            .env("GIT_ATTR_NOSYSTEM", "1")
            .env("GIT_OPTIONAL_LOCKS", "0")
            .env("GIT_NO_REPLACE_OBJECTS", "1")
            .env("GIT_NO_LAZY_FETCH", "1")
            .env("GIT_TERMINAL_PROMPT", "0")
            .args(args);
        let result = run_capture(
            command,
            input,
            deadline,
            CaptureLimits {
                stdin_bytes: 1_000_000,
                stdout_bytes: limit,
                stderr_bytes: 64000,
            },
        )?;
        ensure!(
            !result.timed_out
                && !result.output_exceeded
                && result.output_complete
                && result.stdin_complete
                && !result.io_error,
            "Git metadata process exceeded deadline/output capacity or failed IO"
        );
        let unrelated_history = args.first() == Some(&"merge-base")
            && result.exit_code == Some(1)
            && result.stdout.is_empty()
            && result.stderr.is_empty();
        ensure!(
            result.exit_code == Some(0) || unrelated_history,
            "Git metadata command failed (exit {:?})",
            result.exit_code
        );
        deadline.check()?;
        Ok(result.stdout)
    }
    fn oid(&self, root: &Path, reference: &str, deadline: &Deadline) -> Result<String> {
        ensure!(
            !reference.is_empty()
                && reference.len() <= 256
                && !reference.starts_with('-')
                && reference
                    .chars()
                    .all(|ch| ch.is_alphanumeric() || "_/.-~^".contains(ch))
                && !reference.contains(".."),
            "Invalid Git ref expression"
        );
        let expression = format!("{reference}^{{commit}}");
        let value = String::from_utf8(self.run(
            root,
            &["rev-parse", "--verify", "--end-of-options", &expression],
            &[],
            200,
            deadline,
        )?)?
        .trim()
        .to_owned();
        ensure!(
            oid_valid(&value),
            "Git did not resolve a canonical commit OID"
        );
        Ok(value)
    }
}
fn oid_valid(value: &str) -> bool {
    matches!(value.len(), 40 | 64)
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
fn object_id(kind: &str, bytes: &[u8], length: usize) -> String {
    let header = format!("{kind} {}\0", bytes.len());
    if length == 40 {
        let mut digest = sha1::Sha1::new();
        digest.update(header.as_bytes());
        digest.update(bytes);
        hex::encode(digest.finalize())
    } else {
        let mut digest = sha2::Sha256::new();
        digest.update(header.as_bytes());
        digest.update(bytes);
        hex::encode(digest.finalize())
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct BlobProof {
    path: String,
    git_object_id: String,
    content_hash: String,
    mode: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct TreeEntry {
    path: String,
    mode: String,
    object_id: String,
    object_type: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SourceReceipt {
    snapshot_id: String,
    workspace_id: String,
    origin_snapshot_id: String,
    snapshot_hash: String,
    commit_id: String,
    tree_id: String,
    commit_artifact_id: String,
    tree_entries: Vec<TreeEntry>,
    blobs: Vec<BlobProof>,
    analyzer: String,
    producer: String,
}
fn verify_tree(entries: &[TreeEntry], root_oid: &str, deadline: &Deadline) -> Result<()> {
    ensure!(
        oid_valid(root_oid) && entries.len() <= 10000,
        "Invalid Git tree proof capacity/identity"
    );
    let mut directories = BTreeMap::<String, Vec<&TreeEntry>>::from([(String::new(), vec![])]);
    let mut ids = BTreeMap::from([(String::new(), root_oid.to_owned())]);
    let mut paths = BTreeSet::new();
    for entry in entries {
        deadline.check()?;
        ensure!(
            !entry.path.is_empty()
                && entry.path.len() <= 4096
                && !entry.path.contains('\0')
                && !entry.path.starts_with('/')
                && entry.path.split('/').all(|part| !part.is_empty())
                && oid_valid(&entry.object_id)
                && entry.object_id.len() == root_oid.len()
                && paths.insert(entry.path.as_str()),
            "Invalid or duplicate Git tree proof entry"
        );
        ensure!(
            matches!(
                (entry.mode.as_str(), entry.object_type.as_str()),
                ("040000", "tree")
                    | ("160000", "commit")
                    | ("100644" | "100755" | "120000", "blob")
            ),
            "Unsupported Git tree proof mode/type"
        );
        if entry.object_type == "tree" {
            directories.entry(entry.path.clone()).or_default();
            ids.insert(entry.path.clone(), entry.object_id.clone());
        }
        let parent = entry.path.rsplit_once('/').map_or("", |(parent, _)| parent);
        directories.entry(parent.into()).or_default().push(entry);
    }
    for (path, children) in &mut directories {
        deadline.check()?;
        let expected = ids
            .get(path)
            .context("Git tree proof parent not declared")?;
        children.sort_by_cached_key(|entry| {
            let name = entry.path.rsplit('/').next().unwrap();
            if entry.object_type == "tree" {
                format!("{name}/").into_bytes()
            } else {
                name.as_bytes().to_vec()
            }
        });
        let mut body = Vec::new();
        for entry in children {
            body.extend_from_slice(entry.mode.trim_start_matches('0').as_bytes());
            body.push(b' ');
            body.extend_from_slice(entry.path.rsplit('/').next().unwrap().as_bytes());
            body.push(0);
            body.extend_from_slice(&hex::decode(&entry.object_id)?);
        }
        ensure!(
            body.len() <= 4_000_000 && object_id("tree", &body, root_oid.len()) == *expected,
            "Git tree proof object/membership hash mismatch"
        );
    }
    Ok(())
}
#[derive(Clone)]
struct TreeFile {
    path: String,
    language: String,
    policy_only: bool,
    oid: String,
    mode: String,
    size: usize,
}
fn committed_ignored(
    root: &Path,
    texts: &BTreeMap<String, String>,
    path: &str,
    cache: &mut BTreeMap<String, ignore::gitignore::Gitignore>,
    deadline: &Deadline,
) -> Result<bool> {
    fn matcher(
        root: &Path,
        texts: &BTreeMap<String, String>,
        directory: &str,
        cache: &mut BTreeMap<String, ignore::gitignore::Gitignore>,
        deadline: &Deadline,
    ) -> Result<()> {
        if cache.contains_key(directory) {
            return Ok(());
        }
        let mut builder = ignore::gitignore::GitignoreBuilder::new(root);
        let mut ancestors = vec![String::new()];
        let mut current = String::new();
        for part in directory.split('/').filter(|part| !part.is_empty()) {
            if !current.is_empty() {
                current.push('/');
            }
            current.push_str(part);
            ancestors.push(current.clone());
        }
        for ancestor in ancestors {
            for name in [".gitignore", ".ignore"] {
                let policy = if ancestor.is_empty() {
                    name.to_owned()
                } else {
                    format!("{ancestor}/{name}")
                };
                if let Some(text) = texts.get(&policy) {
                    for line in text.lines() {
                        deadline.check()?;
                        builder.add_line(Some(root.join(&policy)), line)?;
                    }
                }
            }
        }
        cache.insert(directory.to_owned(), builder.build()?);
        Ok(())
    }
    let parts: Vec<_> = path.split('/').collect();
    let mut directory = String::new();
    matcher(root, texts, &directory, cache, deadline)?;
    for part in parts.iter().take(parts.len().saturating_sub(1)) {
        deadline.check()?;
        let next = if directory.is_empty() {
            (*part).to_owned()
        } else {
            format!("{directory}/{part}")
        };
        if cache[&directory]
            .matched(root.join(&next), true)
            .is_ignore()
        {
            return Ok(true);
        }
        directory = next;
        matcher(root, texts, &directory, cache, deadline)?;
    }
    Ok(cache[&directory]
        .matched(root.join(path), false)
        .is_ignore())
}
fn committed(
    engine: &mut Engine,
    host: &HostGit,
    origin: &Snapshot,
    commit: &str,
    deadline: &Deadline,
) -> Result<(Snapshot, Graph)> {
    capture_commit_at(engine.capture_epoch, engine, host, origin, commit, deadline)
}
/// Host-selected historical capture. The exact immutable origin supplies scope,
/// while the explicit epoch supplies extraction and source classification.
pub fn capture_commit_at(
    epoch: crate::analysis::AnalyzerEpoch,
    engine: &mut Engine,
    host: &HostGit,
    origin: &Snapshot,
    commit: &str,
    deadline: &Deadline,
) -> Result<(Snapshot, Graph)> {
    ensure!(
        engine.store.db.is_autocommit(),
        "Git capture caller transaction refused"
    );
    crate::github_app::worker::publication_policy(&engine.store, deadline, || {
        let tx = rusqlite::Transaction::new_unchecked(
            &engine.store.db,
            rusqlite::TransactionBehavior::Immediate,
        )?;
        let mut budget = CaptureBudget {
            files: usize::MAX,
            source_bytes: 64_000_000,
            commit_bytes: 1_000_000,
            tree_bytes: 4_000_000,
            tree_entries: usize::MAX,
        };
        let result =
            capture_commit_pending(epoch, engine, host, origin, commit, &mut budget, deadline)?;
        deadline.check()?;
        tx.commit()?;
        Ok(result)
    })
}
/// Shared acquisition allowances, debited before object bodies or entries are
/// retained. The owned caller admits every bound before opening a transaction.
pub(crate) struct CaptureBudget {
    pub(crate) files: usize,
    pub(crate) source_bytes: usize,
    pub(crate) commit_bytes: usize,
    pub(crate) tree_bytes: usize,
    pub(crate) tree_entries: usize,
}
#[allow(clippy::too_many_arguments)] // Exact source origin, epoch and aggregate acquisition budget.
pub(crate) fn capture_commit_pending(
    epoch: crate::analysis::AnalyzerEpoch,
    engine: &Engine,
    host: &HostGit,
    origin: &Snapshot,
    commit: &str,
    budget: &mut CaptureBudget,
    deadline: &Deadline,
) -> Result<(Snapshot, Graph)> {
    ensure!(
        !engine.store.db.is_autocommit(),
        "Git capture requires owned transaction"
    );
    deadline.check()?;
    engine.store.set_deadline(deadline)?;
    ensure!(
        identity(&engine.store.snapshot(&origin.id)?) == identity(origin),
        "Git origin differs from immutable capture"
    );
    ensure!(
        oid_valid(commit),
        "Git capture requires an exact commit OID"
    );
    let commit_bytes = host.run(
        &engine.root,
        &["cat-file", "commit", commit],
        &[],
        budget.commit_bytes.min(1_000_000),
        deadline,
    )?;
    budget.commit_bytes = budget
        .commit_bytes
        .checked_sub(commit_bytes.len())
        .context("Git acquisition capacity exceeded")?;
    ensure!(
        object_id("commit", &commit_bytes, commit.len()) == commit,
        "Git commit object hash mismatch"
    );
    let commit_text = std::str::from_utf8(&commit_bytes)?;
    ensure!(
        !crate::paths::obvious_secret(commit_text),
        "Git commit metadata contains an obvious secret; cannot publish proof"
    );
    let tree = commit_text
        .lines()
        .next()
        .and_then(|line| line.strip_prefix("tree "))
        .context("Git commit tree binding missing")?
        .to_owned();
    ensure!(
        oid_valid(&tree) && tree.len() == commit.len(),
        "Invalid Git tree object identity"
    );
    let listing = host.run(
        &engine.root,
        &["ls-tree", "-r", "-t", "-l", "-z", "--full-tree", &tree],
        &[],
        budget.tree_bytes.min(4_000_000),
        deadline,
    )?;
    budget.tree_bytes = budget
        .tree_bytes
        .checked_sub(listing.len())
        .context("Git acquisition capacity exceeded")?;
    let origin_scope = crate::capture::source_scope_policy(&engine.store, origin)?;
    let mut entries = vec![];
    let mut tree_entries = vec![];
    let mut omissions = vec![];
    let mut total = 0usize;
    for raw in listing.split(|b| *b == 0).filter(|part| !part.is_empty()) {
        deadline.check()?;
        budget.tree_entries = budget
            .tree_entries
            .checked_sub(1)
            .context("Git acquisition capacity exceeded")?;
        ensure!(
            entries.len() + omissions.len() < 10000,
            "Git tree entry capacity exceeded"
        );
        let line = std::str::from_utf8(raw).context("Git tree path is not UTF-8")?;
        let (info, path) = line.split_once('\t').context("Malformed Git tree record")?;
        let parts: Vec<_> = info.split_ascii_whitespace().collect();
        ensure!(
            parts.len() == 4 && oid_valid(parts[2]),
            "Malformed Git tree object metadata"
        );
        tree_entries.push(TreeEntry {
            path: path.into(),
            mode: parts[0].into(),
            object_id: parts[2].into(),
            object_type: parts[1].into(),
        });
        if parts[1] == "tree" {
            continue;
        }
        let admission = origin_scope.prospective_path_policy_at(epoch, path);
        let is_policy = matches!(path.rsplit('/').next(), Some(".gitignore" | ".ignore"))
            && normalized(path).is_ok();
        let policy_only = admission.is_err() && is_policy;
        let reason = if parts[0] == "120000" {
            Some("symbolic-link")
        } else if parts[0] == "160000" || parts[1] != "blob" {
            Some("submodule-or-nonblob")
        } else if admission.is_err() && !is_policy {
            Some("outside-capture-scope")
        } else {
            None
        };
        if let Some(reason) = reason {
            omissions.push(Omission {
                path: path.into(),
                reason: reason.into(),
            });
            continue;
        }
        let size = parts[3].parse::<usize>()?;
        ensure!(
            !is_policy || size <= 4_000_000,
            "Committed ignore policy byte capacity exceeded"
        );
        if size > 4_000_000 {
            omissions.push(Omission {
                path: path.into(),
                reason: "file-size-limit".into(),
            });
            continue;
        }
        total = total
            .checked_add(size)
            .context("Git source byte overflow")?;
        ensure!(
            total <= 64_000_000,
            "Git committed source byte capacity exceeded"
        );
        budget.source_bytes = budget
            .source_bytes
            .checked_sub(size)
            .context("Git acquisition capacity exceeded")?;
        budget.files = budget
            .files
            .checked_sub(1)
            .context("Git acquisition capacity exceeded")?;
        entries.push(TreeFile {
            path: path.into(),
            language: if policy_only {
                "text".into()
            } else {
                admission?.0
            },
            policy_only,
            oid: parts[2].into(),
            mode: parts[0].into(),
            size,
        });
    }
    verify_tree(&tree_entries, &tree, deadline)?;
    let mut texts = BTreeMap::new();
    let mut remaining = entries.as_slice();
    while !remaining.is_empty() {
        let mut bytes = 0usize;
        let mut count = 0;
        for entry in remaining.iter().take(64) {
            if bytes.saturating_add(entry.size + 100) > 32_000_000 {
                break;
            }
            bytes += entry.size + 100;
            count += 1;
        }
        ensure!(count > 0, "Git object cannot fit a bounded batch");
        let (batch, rest) = remaining.split_at(count);
        remaining = rest;
        deadline.check()?;
        let requested = batch
            .iter()
            .map(|entry| format!("{}\n", entry.oid))
            .collect::<String>();
        let expected = batch.iter().map(|entry| entry.size + 100).sum::<usize>();
        ensure!(
            expected <= 32_000_000,
            "Git object batch byte capacity exceeded"
        );
        let bytes = host.run(
            &engine.root,
            &["cat-file", "--batch"],
            requested.as_bytes(),
            expected.max(1),
            deadline,
        )?;
        let mut cursor = 0;
        for entry in batch {
            let offset = bytes[cursor..]
                .iter()
                .position(|b| *b == b'\n')
                .context("Git batch header truncated")?;
            let header = std::str::from_utf8(&bytes[cursor..cursor + offset])?;
            ensure!(
                header == format!("{} blob {}", entry.oid, entry.size),
                "Git batch object binding mismatch"
            );
            cursor += offset + 1;
            let end = cursor
                .checked_add(entry.size)
                .context("Git batch size overflow")?;
            ensure!(
                end < bytes.len() && bytes[end] == b'\n',
                "Git batch object bytes truncated"
            );
            let body = &bytes[cursor..end];
            cursor = end + 1;
            ensure!(
                object_id("blob", body, entry.oid.len()) == entry.oid,
                "Git blob object hash mismatch"
            );
            if body.contains(&0) || std::str::from_utf8(body).is_err() {
                ensure!(
                    !matches!(
                        entry.path.rsplit('/').next(),
                        Some(".gitignore" | ".ignore")
                    ),
                    "Committed ignore policy is not UTF-8 text"
                );
                omissions.push(Omission {
                    path: entry.path.clone(),
                    reason: "binary-or-invalid-utf8".into(),
                });
                continue;
            }
            let text = std::str::from_utf8(body)?;
            if crate::paths::obvious_secret(text)
                && !matches!(
                    entry.path.rsplit('/').next(),
                    Some(".gitignore" | ".ignore")
                )
            {
                omissions.push(Omission {
                    path: entry.path.clone(),
                    reason: "obvious-secret".into(),
                });
                continue;
            }
            texts.insert(entry.path.clone(), text.to_owned());
        }
        ensure!(
            cursor == bytes.len(),
            "Unexpected extra Git batch object bytes"
        );
    }
    let mut ignore_cache = BTreeMap::new();
    let mut files = vec![];
    let mut proofs = vec![];
    let mut parsed = vec![];
    for entry in &entries {
        deadline.check()?;
        if entry.policy_only {
            omissions.push(Omission {
                path: entry.path.clone(),
                reason: "outside-capture-scope-policy-only".into(),
            });
            continue;
        }
        let Some(text) = texts.get(&entry.path) else {
            continue;
        };
        if crate::paths::obvious_secret(text) {
            omissions.push(Omission {
                path: entry.path.clone(),
                reason: "obvious-secret".into(),
            });
            continue;
        }
        if committed_ignored(
            &engine.root,
            &texts,
            &entry.path,
            &mut ignore_cache,
            deadline,
        )? {
            omissions.push(Omission {
                path: entry.path.clone(),
                reason: "committed-ignore-rule".into(),
            });
            continue;
        }
        let content_hash = engine.store.blob(text.as_bytes())?;
        let file = SourceFile {
            path: entry.path.clone(),
            hash: content_hash.clone(),
            size: text.len(),
            language: entry.language.clone(),
            mode: if entry.mode == "100755" { 0o755 } else { 0o644 },
        };
        parsed.push(crate::analysis::extract_at(epoch, &file, text, deadline)?);
        proofs.push(BlobProof {
            path: entry.path.clone(),
            git_object_id: entry.oid.clone(),
            content_hash,
            mode: entry.mode.clone(),
        });
        files.push(file);
    }
    files.sort_by(|a, b| a.path.cmp(&b.path));
    omissions.sort_by(|a, b| a.path.cmp(&b.path));
    proofs.sort_by(|a, b| a.path.cmp(&b.path));
    let id = identity(&(
        "native-git-source-1",
        &engine.store.workspace_id,
        commit,
        &origin.id,
        &files,
        &omissions,
        epoch.producer(),
    ));
    let snapshot = Snapshot {
        id,
        workspace_id: engine.store.workspace_id.clone(),
        kind: "derived".into(),
        files,
        omissions,
        capture_consistency: "immutable_git_objects_verified".into(),
    };
    let graph = crate::analysis::assemble_at(epoch, &snapshot, parsed, deadline)?;
    engine.store.publish_historical_pending(&snapshot, &graph)?;
    crate::capture::inherit_options(&engine.store, origin, &snapshot)?;
    let commit_artifact_id = engine.store.artifact(&commit_bytes)?;
    let receipt = SourceReceipt {
        snapshot_id: snapshot.id.clone(),
        workspace_id: snapshot.workspace_id.clone(),
        origin_snapshot_id: origin.id.clone(),
        snapshot_hash: identity(&snapshot),
        commit_id: commit.into(),
        tree_id: tree,
        commit_artifact_id,
        tree_entries,
        blobs: proofs,
        analyzer: epoch.producer().into(),
        producer: "yoke-native-git-1".into(),
    };
    immutable_record(
        &engine.store,
        SOURCE_KIND,
        &snapshot.id,
        &serde_json::to_value(receipt)?,
        deadline,
    )?;
    Ok((snapshot, graph))
}
fn immutable_record(
    store: &crate::store::Store,
    kind: &str,
    id: &str,
    value: &Value,
    deadline: &Deadline,
) -> Result<()> {
    deadline.check()?;
    store.db.execute(
        "INSERT INTO records(kind,id,value) VALUES(?1,?2,?3) ON CONFLICT(kind,id) DO NOTHING",
        rusqlite::params![kind, id, value.to_string()],
    )?;
    ensure!(
        record_value(
            store,
            kind,
            id,
            if kind == SOURCE_KIND {
                16_000_000
            } else {
                64000
            },
            deadline
        )? == *value,
        "Immutable Git record collision"
    );
    Ok(())
}
fn record_value(
    store: &crate::store::Store,
    kind: &str,
    id: &str,
    maximum: usize,
    deadline: &Deadline,
) -> Result<Value> {
    deadline.check()?;
    let limit = rusqlite::limits::Limit::SQLITE_LIMIT_LENGTH;
    let cap = store
        .db
        .limit(limit)?
        .min(i32::try_from(maximum.saturating_add(4096))?);
    let prior = store.db.set_limit(limit, cap)?;
    let result = (|| -> Result<Value> {
        let size: usize = store.db.query_row(
            "SELECT length(CAST(value AS BLOB)) FROM records WHERE kind=?1 AND id=?2",
            rusqlite::params![kind, id],
            |row| row.get(0),
        )?;
        ensure!(size <= maximum, "Git record byte capacity exceeded");
        let raw: String = store.db.query_row(
            "SELECT value FROM records WHERE kind=?1 AND id=?2",
            rusqlite::params![kind, id],
            |row| row.get(0),
        )?;
        ensure!(raw.len() == size, "Git record changed during read");
        deadline.check()?;
        crate::json::parse(raw.as_bytes(), maximum)
    })();
    store.db.set_limit(limit, prior)?;
    result
}
fn cas_bytes(
    store: &crate::store::Store,
    id: &str,
    size: usize,
    maximum: usize,
    deadline: &Deadline,
) -> Result<Vec<u8>> {
    ensure!(
        model::valid_id(id) && size <= maximum,
        "Git CAS identity/capacity exceeded"
    );
    let path = store.dir.join("blobs").join(id);
    let metadata = std::fs::symlink_metadata(&path)?;
    ensure!(
        metadata.is_file() && !is_link(&metadata) && metadata.len() == size as u64,
        "Git CAS file type/length mismatch"
    );
    let mut input = std::fs::File::open(path)?;
    let mut bytes = Vec::with_capacity(size);
    let mut buffer = [0u8; 65536];
    loop {
        deadline.check()?;
        let count = input.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        ensure!(
            bytes.len().saturating_add(count) <= size,
            "Git CAS grew while reading"
        );
        bytes.extend_from_slice(&buffer[..count]);
    }
    ensure!(
        bytes.len() == size && hash(&bytes) == id,
        "Git CAS content hash mismatch"
    );
    Ok(bytes)
}
fn artifact_bytes(
    store: &crate::store::Store,
    id: &str,
    maximum: usize,
    deadline: &Deadline,
) -> Result<Vec<u8>> {
    ensure!(model::valid_id(id), "Invalid Git artifact ID");
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase", deny_unknown_fields)]
    struct Registration {
        artifact_id: String,
        total_bytes: usize,
    }
    let registration: Registration =
        crate::json::decode(record_value(store, "artifact", id, 64000, deadline)?)?;
    ensure!(
        registration.artifact_id == id,
        "Git artifact registration mismatch"
    );
    cas_bytes(store, id, registration.total_bytes, maximum, deadline)
}
pub fn validate_source(
    store: &crate::store::Store,
    id: &str,
    deadline: &Deadline,
) -> Result<Value> {
    deadline.check()?;
    store.set_deadline(deadline)?;
    ensure!(model::valid_id(id), "Invalid Git source snapshot ID");
    let snapshot = store.snapshot(id)?;
    let header = source_read::header(store, &snapshot, deadline)?;
    validate_source_objects(
        &snapshot,
        &header.receipt,
        &header.commit,
        &mut |file| cas_bytes(store, &file.hash, file.size, 4_000_000, deadline),
        deadline,
    )?;
    Ok(serde_json::to_value(header.receipt)?)
}
fn validate_source_objects(
    snapshot: &Snapshot,
    receipt: &SourceReceipt,
    commit: &[u8],
    read: &mut dyn FnMut(&SourceFile) -> Result<Vec<u8>>,
    deadline: &Deadline,
) -> Result<()> {
    validate_source_objects_retained(
        snapshot,
        receipt,
        commit,
        read,
        &mut |_, _| Ok(()),
        deadline,
    )
}
fn validate_source_objects_retained(
    snapshot: &Snapshot,
    receipt: &SourceReceipt,
    commit: &[u8],
    read: &mut dyn FnMut(&SourceFile) -> Result<Vec<u8>>,
    retain: &mut dyn FnMut(&SourceFile, String) -> Result<()>,
    deadline: &Deadline,
) -> Result<()> {
    ensure!(
        snapshot.kind == "derived"
            && snapshot.capture_consistency == "immutable_git_objects_verified"
            && [
                "yoke-tree-sitter-4",
                "yoke-tree-sitter-6",
                "yoke-tree-sitter-8",
                "yoke-tree-sitter-9"
            ]
            .contains(&receipt.analyzer.as_str())
            && receipt.snapshot_id == snapshot.id
            && receipt.workspace_id == snapshot.workspace_id
            && receipt.snapshot_hash == identity(snapshot)
            && receipt.producer == "yoke-native-git-1"
            && receipt.blobs.len() == snapshot.files.len()
            && snapshot.files.len() <= 10000
            && hash(commit) == receipt.commit_artifact_id,
        "Git source object scope mismatch"
    );
    ensure!(
        commit.len() <= 1_000_000
            && oid_valid(&receipt.commit_id)
            && receipt.commit_id.len() == receipt.tree_id.len()
            && object_id("commit", commit, receipt.commit_id.len()) == receipt.commit_id,
        "Git source commit proof hash mismatch"
    );
    let text = std::str::from_utf8(commit)?;
    ensure!(
        !crate::paths::obvious_secret(text)
            && text.lines().next() == Some(&format!("tree {}", receipt.tree_id)),
        "Git commit proof tree binding mismatch"
    );
    verify_tree(&receipt.tree_entries, &receipt.tree_id, deadline)?;
    let tree: BTreeMap<_, _> = receipt
        .tree_entries
        .iter()
        .map(|entry| (entry.path.as_str(), entry))
        .collect();
    let mut paths = BTreeSet::new();
    let mut files = BTreeMap::new();
    for file in &snapshot.files {
        deadline.check()?;
        ensure!(
            files.insert(file.path.as_str(), file).is_none(),
            "Duplicate Git source manifest path"
        );
    }
    let mut total = 0usize;
    for proof in &receipt.blobs {
        deadline.check()?;
        ensure!(
            paths.insert(proof.path.as_str()),
            "Duplicate Git file proof"
        );
        let file = files
            .get(proof.path.as_str())
            .copied()
            .context("Git proof file outside source manifest")?;
        let entry = tree
            .get(proof.path.as_str())
            .context("Git source file lacks committed tree membership")?;
        ensure!(
            entry.object_type == "blob"
                && entry.object_id == proof.git_object_id
                && entry.mode == proof.mode
                && matches!(proof.mode.as_str(), "100644" | "100755")
                && file.hash == proof.content_hash
                && file.mode == if proof.mode == "100755" { 0o755 } else { 0o644 },
            "Git source leaf/tree binding mismatch"
        );
        let text = String::from_utf8(read(file)?)?;
        let content_hash = &file.hash;
        total = total
            .checked_add(text.len())
            .context("Git proof source byte overflow")?;
        ensure!(
            text.len() == file.size
                && hash(text.as_bytes()) == file.hash
                && text.len() <= 4_000_000
                && total <= 64_000_000
                && content_hash == &proof.content_hash
                && object_id("blob", text.as_bytes(), proof.git_object_id.len())
                    == proof.git_object_id,
            "Git source blob proof hash/size mismatch"
        );
        retain(file, text)?;
    }
    ensure!(
        snapshot.id
            == identity(&(
                "native-git-source-1",
                &snapshot.workspace_id,
                &receipt.commit_id,
                &receipt.origin_snapshot_id,
                &snapshot.files,
                &snapshot.omissions,
                &receipt.analyzer
            )),
        "Git source snapshot identity mismatch"
    );
    Ok(())
}
pub(crate) fn has_source_proof(store: &crate::store::Store, id: &str) -> Result<bool> {
    store.check_deadline()?;
    Ok(store.db.query_row(
        "SELECT EXISTS(SELECT 1 FROM records WHERE kind=?1 AND id=?2)",
        rusqlite::params![SOURCE_KIND, id],
        |row| row.get(0),
    )?)
}
/// Select only installed extraction policies after verifying the complete original Git proof.
/// Known historical receipts remain readable through validate_source, never retagged for replay.
pub fn validated_source_epoch(
    store: &crate::store::Store,
    snapshot: &Snapshot,
    deadline: &Deadline,
) -> Result<crate::analysis::AnalyzerEpoch> {
    deadline.check()?;
    store.set_deadline(deadline)?;
    ensure!(
        identity(&store.snapshot(&snapshot.id)?) == identity(snapshot),
        "Git epoch requires exact immutable manifest"
    );
    let receipt: SourceReceipt =
        crate::json::decode(validate_source(store, &snapshot.id, deadline)?)?;
    crate::analysis::AnalyzerEpoch::parse(&receipt.analyzer)
}

/// Offline producer proof; source bytes are included because an ancestor Git
/// capture can differ from the current derived graph's source set.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct ArchivedGitSource {
    receipt: SourceReceipt,
    inheritance: crate::capture::DerivedCaptureOptionsRecord,
    commit_text: String,
    sources: BTreeMap<String, String>,
}
impl ArchivedGitSource {
    pub(crate) fn origin_id(&self) -> &str {
        &self.receipt.origin_snapshot_id
    }
    pub(crate) fn epoch(&self) -> Option<crate::analysis::AnalyzerEpoch> {
        // The full proof validator admits only explicitly known producer labels.
        // An unavailable historical extractor remains unclaimed.
        crate::analysis::AnalyzerEpoch::parse(&self.receipt.analyzer).ok()
    }
}
pub(crate) fn archive_source(
    store: &crate::store::Store,
    snapshot: &Snapshot,
    deadline: &Deadline,
) -> Result<ArchivedGitSource> {
    let (header, sources) = source_read::retained(store, snapshot, 64_000_000, deadline)?;
    Ok(ArchivedGitSource {
        receipt: header.receipt,
        inheritance: header.inheritance,
        commit_text: String::from_utf8(header.commit)?,
        sources,
    })
}
pub(crate) fn validate_archived_source(
    snapshot: &Snapshot,
    archive: &ArchivedGitSource,
    origin: &Snapshot,
    options: &crate::capture::CaptureOptions,
    deadline: &Deadline,
) -> Result<()> {
    deadline.check()?;
    let inherited = &archive.inheritance;
    ensure!(
        inherited.snapshot_id == snapshot.id
            && inherited.workspace_id == snapshot.workspace_id
            && inherited.origin_snapshot_id == origin.id
            && inherited.origin_snapshot_hash == identity(origin)
            && inherited.derived_snapshot_hash == identity(snapshot)
            && inherited.options_hash == identity(options)
            && archive.receipt.origin_snapshot_id == origin.id
            && archive.sources.len() == snapshot.files.len(),
        "Archived Git capture origin binding mismatch"
    );
    validate_source_objects(
        snapshot,
        &archive.receipt,
        archive.commit_text.as_bytes(),
        &mut |file| {
            let text = archive
                .sources
                .get(&file.path)
                .context("Archived Git source missing")?;
            ensure!(
                text.len() == file.size && text.len() <= 4_000_000,
                "Archived Git source byte mismatch"
            );
            Ok(text.as_bytes().to_vec())
        },
        deadline,
    )
}
fn texts(store: &crate::store::Store, snapshot: &Snapshot) -> Result<BTreeMap<String, String>> {
    snapshot
        .files
        .iter()
        .map(|file| {
            crate::query::source(store, snapshot, &file.path)
                .map(|(text, _)| (file.path.clone(), text))
        })
        .collect()
}
fn node_claim(node: &model::Entity, snapshot: &Snapshot, side: &str) -> Value {
    let file = snapshot
        .files
        .iter()
        .find(|file| file.path == node.path)
        .unwrap();
    json!({"id":node.id,"snapshotId":snapshot.id,"path":node.path,"name":node.name,"kind":node.kind,"startByte":node.start_byte,"endByte":node.end_byte,"startLine":node.start_line,"endLine":node.end_line,"contentHash":file.hash,"side":side,"producer":node.producer,"trust":"source_bound_structural_candidate"})
}
fn owner_rules(
    store: &crate::store::Store,
    snapshot: &Snapshot,
    paths: &[String],
    limit: usize,
    deadline: &Deadline,
) -> Result<Vec<Value>> {
    let chosen = [".github/CODEOWNERS", "CODEOWNERS", "docs/CODEOWNERS"]
        .into_iter()
        .find(|path| snapshot.files.iter().any(|file| file.path == *path));
    let Some(path) = chosen else {
        return Ok(vec![]);
    };
    let (text, hash) = crate::query::source(store, snapshot, path)?;
    let mut rules = vec![];
    for (index, line) in text.lines().enumerate() {
        deadline.check()?;
        let mut parts = line.split_ascii_whitespace();
        let Some(pattern) = parts.next() else {
            continue;
        };
        if pattern.starts_with(['#', '!']) || pattern.contains(['[', ']', '\\']) {
            continue;
        }
        let owners: Vec<_> = parts
            .take_while(|part| !part.starts_with('#'))
            .filter(|owner| owner.starts_with('@') || owner.contains('@'))
            .take(50)
            .map(str::to_owned)
            .collect();
        if owners.is_empty() {
            continue;
        }
        let mut matcher = ignore::gitignore::GitignoreBuilder::new("");
        matcher.add_line(None, pattern)?;
        rules.push((matcher.build()?, owners, index + 1, pattern.to_owned()));
    }
    let mut result = BTreeMap::<String, (BTreeSet<String>, usize, String)>::new();
    for changed in paths {
        if let Some((_, owners, line, pattern)) = rules.iter().rev().find(|(matcher, _, _, _)| {
            matcher
                .matched_path_or_any_parents(changed, false)
                .is_ignore()
        }) {
            for owner in owners {
                let item = result.entry(owner.clone()).or_insert((
                    BTreeSet::new(),
                    *line,
                    pattern.clone(),
                ));
                item.0.insert(changed.clone());
            }
        }
    }
    Ok(result.into_iter().take(limit).map(|(owner,(paths,line,pattern))|json!({"owner":owner,"paths":paths,"source":"CODEOWNERS","snapshotId":snapshot.id,"sourcePath":path,"contentHash":hash,"line":line,"pattern":pattern,"trust":"authored_owner_candidate_not_verified_team_membership"})).collect())
}
fn changed_commit_paths(
    root: &Path,
    git: &HostGit,
    base: &str,
    head: &str,
    scope: &crate::capture::CapturedSourceScope<'_>,
    deadline: &Deadline,
) -> Result<BTreeSet<String>> {
    let bytes = git.run(
        root,
        &[
            "diff",
            "--name-only",
            "-z",
            "--no-ext-diff",
            "--no-textconv",
            "--ignore-submodules=all",
            "--no-renames",
            base,
            head,
            "--",
        ],
        &[],
        2_000_000,
        deadline,
    )?;
    let mut paths = BTreeSet::new();
    for raw in bytes.split(|b| *b == 0).filter(|part| !part.is_empty()) {
        let path = std::str::from_utf8(raw)?;
        if scope.prospective_path_policy(path).is_ok() {
            paths.insert(path.into());
        }
        ensure!(paths.len() <= 10000, "Git changed path capacity exceeded");
    }
    Ok(paths)
}
fn branch_overlap(
    root: &Path,
    git: &HostGit,
    head: &str,
    changed: &[String],
    maximum: usize,
    scope: &crate::capture::CapturedSourceScope<'_>,
    deadline: &Deadline,
) -> Result<Vec<Value>> {
    if maximum == 0 {
        return Ok(vec![]);
    }
    let count = format!("--count={maximum}");
    let bytes = git.run(
        root,
        &[
            "for-each-ref",
            "--sort=-committerdate",
            &count,
            "--format=%(refname)%00%(objectname)%00%(authorname)%00%(committerdate:iso-strict)",
            "refs/heads",
            "refs/remotes",
        ],
        &[],
        1_000_000,
        deadline,
    )?;
    let text = std::str::from_utf8(&bytes)?;
    let mut result = vec![];
    for line in text.lines() {
        deadline.check()?;
        let fields: Vec<_> = line.split('\0').collect();
        ensure!(
            fields.len() == 4 && oid_valid(fields[1]),
            "Malformed Git branch metadata"
        );
        if fields[1] == head {
            continue;
        }
        let merge = String::from_utf8(git.run(
            root,
            &["merge-base", head, fields[1]],
            &[],
            200,
            deadline,
        )?)?
        .trim()
        .to_owned();
        if merge.is_empty() {
            result.push(json!({"ref":fields[0],"tipCommit":fields[1],"mergeBaseCommit":null,"relationship":"unrelated_history","authorCandidate":fields[2],"tipDate":fields[3],"changedPaths":[],"overlappingPaths":[],"source":"observed_local_refs_only","remoteFreshness":"unverified_no_fetch","trust":"unrelated_history_no_overlap_proof"}));
            continue;
        }
        ensure!(oid_valid(&merge), "Invalid branch merge base");
        let paths = changed_commit_paths(root, git, &merge, fields[1], scope, deadline)?;
        let overlap: Vec<_> = changed
            .iter()
            .filter(|path| paths.contains(*path))
            .cloned()
            .collect();
        result.push(json!({"ref":fields[0],"tipCommit":fields[1],"mergeBaseCommit":merge,"authorCandidate":fields[2],"tipDate":fields[3],"changedPaths":paths,"overlappingPaths":overlap,"source":"observed_local_refs_only","remoteFreshness":"unverified_no_fetch","trust":"shared_path_candidate_not_verified_collaboration"}));
    }
    Ok(result)
}
fn history(
    root: &Path,
    git: &HostGit,
    head: &str,
    maximum: usize,
    scope: &crate::capture::CapturedSourceScope<'_>,
    deadline: &Deadline,
) -> Result<Vec<Value>> {
    let count = format!("--max-count={maximum}");
    let bytes = git.run(
        root,
        &[
            "log",
            "--no-show-signature",
            "--no-ext-diff",
            "--no-textconv",
            &count,
            "--format=%H%x00%an%x00%ae%x00%aI%x00%s%x00",
            head,
            "--",
        ],
        &[],
        2_000_000,
        deadline,
    )?;
    let mut result = vec![];
    for line in std::str::from_utf8(&bytes)?
        .lines()
        .filter(|line| !line.is_empty())
    {
        deadline.check()?;
        let fields: Vec<_> = line.split('\0').collect();
        ensure!(
            fields.len() == 6 && fields[5].is_empty() && oid_valid(fields[0]),
            "Malformed Git history metadata"
        );
        ensure!(
            fields
                .iter()
                .all(|value| value.len() <= 16000 && !value.chars().any(|ch| ch.is_control())),
            "Git history metadata field capacity/control exceeded"
        );
        let raw = git.run(
            root,
            &[
                "diff-tree",
                "--root",
                "--no-commit-id",
                "--name-only",
                "-r",
                "-z",
                "--no-ext-diff",
                "--no-textconv",
                fields[0],
                "--",
            ],
            &[],
            2_000_000,
            deadline,
        )?;
        let paths: Vec<_> = raw
            .split(|b| *b == 0)
            .filter(|part| !part.is_empty())
            .filter_map(|part| std::str::from_utf8(part).ok())
            .filter(|path| scope.prospective_path_policy(path).is_ok())
            .take(10000)
            .map(str::to_owned)
            .collect();
        result.push(json!({"commitId":fields[0],"author":fields[1],"email":fields[2],"date":fields[3],"subject":if crate::paths::obvious_secret(fields[4]){"[obvious secret omitted]"}else{fields[4]},"paths":paths,"trust":"observed_commit_metadata_not_verified_authorship"}));
    }
    Ok(result)
}
fn history_owners(commits: &[Value], paths: &[String], limit: usize) -> Vec<Value> {
    let mut owners = BTreeMap::<String, (String, BTreeSet<String>, BTreeSet<String>)>::new();
    for commit in commits {
        let touched: Vec<_> = commit["paths"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(Value::as_str)
            .filter(|path| paths.iter().any(|changed| changed == path))
            .collect();
        if touched.is_empty() {
            continue;
        }
        let owner = commit["email"].as_str().unwrap_or("");
        if owner.is_empty() {
            continue;
        }
        let entry = owners.entry(owner.into()).or_insert((
            commit["author"].as_str().unwrap_or("").into(),
            BTreeSet::new(),
            BTreeSet::new(),
        ));
        entry.1.insert(commit["commitId"].as_str().unwrap().into());
        entry.2.extend(touched.into_iter().map(str::to_owned));
    }
    let mut entries: Vec<_> = owners.into_iter().collect();
    entries.sort_by(|a, b| b.1.1.len().cmp(&a.1.1.len()).then_with(|| a.0.cmp(&b.0)));
    entries.into_iter().take(limit).map(|(owner,(name,commits,paths))|json!({"owner":owner,"name":name,"source":"git_history","commits":commits,"paths":paths,"trust":"historical_author_candidate_not_verified_ownership"})).collect()
}
struct NativeImpact {
    changes: Vec<Value>,
    changed: Vec<Value>,
    impacts: Vec<Value>,
    impact_observations: BTreeMap<(String, String), Value>,
    changed_paths: Vec<String>,
}
fn native_impact(
    store: &crate::store::Store,
    base: &Snapshot,
    base_graph: &Graph,
    head: &Snapshot,
    head_graph: &Graph,
    options: &WorkflowOptions,
    deadline: &Deadline,
) -> Result<NativeImpact> {
    let base_text = texts(store, base)?;
    let head_text = texts(store, head)?;
    let paths: BTreeSet<_> = base_text.keys().chain(head_text.keys()).cloned().collect();
    let mut changes = vec![];
    let mut changed = vec![];
    let mut impacts = vec![];
    let mut impact_observations = BTreeMap::new();
    let mut changed_paths = vec![];
    for path in paths {
        deadline.check()?;
        let before = base_text.get(&path);
        let after = head_text.get(&path);
        if before == after {
            continue;
        }
        changed_paths.push(path.clone());
        let before = before.map(String::as_str).unwrap_or("");
        let after = after.map(String::as_str).unwrap_or("");
        ensure!(
            before.lines().count() + after.lines().count() <= 200000,
            "Native diff line capacity exceeded"
        );
        let diff = similar::TextDiff::configure()
            .timeout(deadline.remaining().min(Duration::from_secs(1)))
            .diff_lines(before, after);
        deadline.check()?;
        let base_lines = crate::paths::line_starts(before);
        let head_lines = crate::paths::line_starts(after);
        let mut hunks = vec![];
        let mut selections = [BTreeSet::new(), BTreeSet::new()];
        for op in diff.ops() {
            if op.tag() == similar::DiffTag::Equal {
                continue;
            }
            let ranges = [op.old_range(), op.new_range()];
            let mut anchors = vec![];
            for (side, ((text, lines), (snapshot, graph))) in
                [(before, &base_lines), (after, &head_lines)]
                    .into_iter()
                    .zip([(&base, &base_graph), (&head, &head_graph)])
                    .enumerate()
            {
                let range = &ranges[side];
                if !snapshot.files.iter().any(|file| file.path == path) {
                    anchors.push(Value::Null);
                    continue;
                }
                let start = lines.get(range.start).copied().unwrap_or(text.len());
                let end = lines.get(range.end).copied().unwrap_or(text.len());
                anchors.push(json!({"snapshotId":snapshot.id,"path":path,"contentHash":hash(text.as_bytes()),"startByte":start,"endByte":end,"startLine":range.start+1,"lineCount":range.len(),"anchor":"exact_source_range"}));
                if range.is_empty() {
                    continue;
                }
                let nodes: Vec<_> = graph
                    .entities
                    .iter()
                    .filter(|node| {
                        node.path == path
                            && node.kind != "file"
                            && node.start_byte < end
                            && node.end_byte > start
                    })
                    .collect();
                if nodes.is_empty() {
                    if let Some(file) = graph
                        .entities
                        .iter()
                        .find(|node| node.path == path && node.kind == "file")
                    {
                        selections[side].insert(file.id.clone());
                    }
                } else {
                    for node in nodes {
                        selections[side].insert(node.id.clone());
                    }
                }
            }
            hunks.push(json!({"base":anchors[0],"head":anchors[1]}));
        }
        changes.push(json!({"basePath":if base_text.contains_key(&path){Some(&path)}else{None},"headPath":if head_text.contains_key(&path){Some(&path)}else{None},"status":if !base_text.contains_key(&path){"added"}else if !head_text.contains_key(&path){"deleted"}else{"modified"},"hunks":hunks}));
        for (side, (snapshot, graph)) in [(&base, &base_graph), (&head, &head_graph)]
            .into_iter()
            .enumerate()
        {
            let label = if side == 0 { "base" } else { "head" };
            for node in graph
                .entities
                .iter()
                .filter(|node| selections[side].contains(&node.id))
            {
                changed.push(node_claim(node, snapshot, label));
            }
            let mut visited = selections[side].clone();
            let mut queue: VecDeque<_> =
                selections[side].iter().map(|id| (id.clone(), 0)).collect();
            while let Some((id, depth)) = queue.pop_front() {
                deadline.check()?;
                if depth == options.impact_depth {
                    continue;
                }
                for edge in graph
                    .observations
                    .iter()
                    .filter(|edge| edge.target.as_deref() == Some(&id))
                {
                    let mut claim = serde_json::to_value(edge)?;
                    claim["side"] = json!(label);
                    claim["evidence"] = json!(
                        graph
                            .evidence
                            .iter()
                            .filter(|anchor| edge.evidence_ids.contains(&anchor.id))
                            .collect::<Vec<_>>()
                    );
                    impact_observations.insert((label.to_owned(), edge.id.clone()), claim);
                    if visited.insert(edge.source.clone()) {
                        ensure!(visited.len() <= 20000, "Git impact node capacity exceeded");
                        if let Some(node) =
                            graph.entities.iter().find(|node| node.id == edge.source)
                        {
                            let mut value = node_claim(node, snapshot, label);
                            value["distance"] = json!(depth + 1);
                            value["observationId"] = json!(edge.id);
                            impacts.push(value);
                            queue.push_back((edge.source.clone(), depth + 1));
                        }
                    }
                }
            }
        }
    }
    Ok(NativeImpact {
        changes,
        changed,
        impacts,
        impact_observations,
        changed_paths,
    })
}
pub fn analyze(
    engine: &mut Engine,
    git: &HostGit,
    options: &WorkflowOptions,
    deadline: &Deadline,
) -> Result<Value> {
    ensure!(
        engine.capture_options.include_ignored_dirs.is_empty(),
        "Historical Git workflows with includeIgnoredDirs require an independent committed ignore proof; disk ignore observations cannot authorize Git history"
    );
    ensure!(
        options.impact_depth <= 8
            && (1..=20).contains(&options.owner_limit)
            && (1..=1000).contains(&options.history_limit)
            && options.branch_limit <= 100,
        "Invalid Git workflow capacities"
    );
    let (working, working_graph, _, _) = engine.index(deadline)?;
    let resolved_base = git.oid(&engine.root, &options.base_ref, deadline)?;
    let head_commit = git.oid(
        &engine.root,
        options.head_ref.as_deref().unwrap_or("HEAD"),
        deadline,
    )?;
    let base_commit = if options.merge_base {
        let value = String::from_utf8(git.run(
            &engine.root,
            &["merge-base", &resolved_base, &head_commit],
            &[],
            200,
            deadline,
        )?)?
        .trim()
        .to_owned();
        ensure!(oid_valid(&value), "Invalid merge base OID");
        value
    } else {
        resolved_base
    };
    let (base, base_graph) = committed(engine, git, &working, &base_commit, deadline)?;
    let (head, head_graph) = if options.head_ref.is_some() {
        committed(engine, git, &working, &head_commit, deadline)?
    } else {
        (working.clone(), working_graph)
    };
    let NativeImpact {
        changes,
        changed,
        impacts,
        impact_observations,
        changed_paths,
    } = native_impact(
        &engine.store,
        &base,
        &base_graph,
        &head,
        &head_graph,
        options,
        deadline,
    )?;
    let working_scope = crate::capture::source_scope_policy(&engine.store, &working)?;
    let head_scope = crate::capture::source_scope_policy(&engine.store, &head)?;
    let mut owners = owner_rules(
        &engine.store,
        &head,
        &changed_paths,
        options.owner_limit,
        deadline,
    )?;
    let history = history(
        &engine.root,
        git,
        &head_commit,
        options.history_limit,
        &working_scope,
        deadline,
    )?;
    owners.extend(history_owners(
        &history,
        &changed_paths,
        options.owner_limit,
    ));
    let branches = branch_overlap(
        &engine.root,
        git,
        &head_commit,
        &changed_paths,
        options.branch_limit,
        &working_scope,
        deadline,
    )?;
    let impact_observations: Vec<_> = impact_observations.into_values().collect();
    let mut report = json!({"schemaVersion":1,"workspaceId":engine.store.workspace_id,"baseSnapshotId":base.id,"headSnapshotId":head.id,"originSnapshotId":working.id,"baseGraphHash":identity(&base_graph),"headGraphHash":identity(&head_graph),"baseCommit":base_commit,"headCommit":head_commit,"headMode":if options.head_ref.is_some(){"commit"}else{"captured_working_tree"},"options":options,"changes":changes,"changedSymbols":changed,"impact":impacts,"ownerSuggestions":owners,"branches":branches,"history":history,"complete":false,"limitations":["Impact is incoming structural candidate reachability, not compiler/runtime proof.","Working source diffs compare captured bytes directly; no Git working-tree status/diff/filter/hook is executed.","CODEOWNERS suggestions are authored candidates; unsupported patterns and verified team membership are not inferred.","Tracked historical source uses committed ignore rules; private Git info/global excludes are not imported.","Branch overlap uses current local refs; remote push/freshness and team identity are unverified."]});
    report["impactObservations"] = json!(impact_observations);
    let typed: ReportData = crate::json::decode(report.clone())?;
    validate_metadata(&typed, &changed_paths, &head_scope, deadline)?;
    let bytes = serde_json::to_vec(&report)?;
    ensure!(
        bytes.len() <= 16_000_000,
        "Git workflow report capacity exceeded"
    );
    let id = engine.store.artifact(&bytes)?;
    immutable_record(
        &engine.store,
        REPORT_KIND,
        &id,
        &json!({"artifactId":id,"workspaceId":engine.store.workspace_id,"baseSnapshotId":base.id,"headSnapshotId":head.id,"originSnapshotId":working.id,"baseGraphHash":identity(&base_graph),"headGraphHash":identity(&head_graph),"producer":"yoke-native-git-workflow-1"}),
        deadline,
    )?;
    report["artifactId"] = json!(id);
    Ok(report)
}
