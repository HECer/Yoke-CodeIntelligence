//! Actual unmerged index observations. Git-stage bytes are a distinct source
//! space, never a captured graph/working-file proof or permission to resolve it.
use super::{HostGit, object_id, oid_valid};
use crate::{
    analysis::AnalyzerEpoch,
    capture::{self, Deadline},
    model::{Snapshot, hash, identity, valid_id},
    store::Store,
};
use anyhow::{Context, Result, ensure};
use rusqlite::{OptionalExtension, Transaction, TransactionBehavior, limits::Limit};
use serde::{Deserialize, Serialize, Serializer};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Limits {
    pub max_paths: usize,
    pub max_stage_rows: usize,
    pub max_index_bytes: usize,
    pub max_blob_objects: usize,
    pub max_blob_bytes: usize,
    pub max_source_bytes: usize,
    pub max_quote_bytes: usize,
    pub max_total_quote_bytes: usize,
    pub max_scope_files: usize,
    pub max_scope_metadata_bytes: usize,
    pub max_output_bytes: usize,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            max_paths: 2000,
            max_stage_rows: 6000,
            max_index_bytes: 2_000_000,
            max_blob_objects: 6000,
            max_blob_bytes: 4_000_000,
            max_source_bytes: 32_000_000,
            max_quote_bytes: 16_384,
            max_total_quote_bytes: 256_000,
            max_scope_files: 20_000,
            max_scope_metadata_bytes: 16_000_000,
            max_output_bytes: 4_000_000,
        }
    }
}
impl Limits {
    pub fn validate(&self) -> Result<()> {
        let hard = Self::default();
        for (v, max) in [
            (self.max_paths, hard.max_paths),
            (self.max_stage_rows, hard.max_stage_rows),
            (self.max_index_bytes, hard.max_index_bytes),
            (self.max_blob_objects, hard.max_blob_objects),
            (self.max_blob_bytes, hard.max_blob_bytes),
            (self.max_source_bytes, hard.max_source_bytes),
            (self.max_quote_bytes, hard.max_quote_bytes),
            (self.max_total_quote_bytes, hard.max_total_quote_bytes),
            (self.max_scope_files, hard.max_scope_files),
            (self.max_scope_metadata_bytes, hard.max_scope_metadata_bytes),
            (self.max_output_bytes, hard.max_output_bytes),
        ] {
            ensure!(v <= max, Failure::Capacity);
        }
        Ok(())
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReadRequest {
    pub schema_version: u32,
    pub expected_index_hash: String,
    pub path: String,
    pub stage: u8,
    pub start_byte: usize,
    pub max_bytes: usize,
}
impl ReadRequest {
    pub fn parse(raw: &[u8]) -> Result<Self> {
        crate::json::parse(raw, 8192)
            .and_then(crate::json::decode)
            .map_err(|_| anyhow::anyhow!(Failure::Input))
    }
    fn validate(&self, limits: &Limits) -> Result<()> {
        ensure!(
            self.schema_version == 1
                && valid_id(&self.expected_index_hash)
                && (1..=3).contains(&self.stage)
                && self.start_byte <= limits.max_blob_bytes
                && self.max_bytes <= limits.max_quote_bytes,
            Failure::Input
        );
        crate::paths::normalized_syntax(&self.path).map_err(|_| anyhow::anyhow!(Failure::Input))?;
        Ok(())
    }
}
/// These grants are host inputs; JSON limits and page requests cannot enable them.
#[derive(Default)]
pub struct Options {
    pub allow_index_read: bool,
    pub allow_git_sources: bool,
    pub limits: Limits,
    pub read_request: Option<ReadRequest>,
}
impl Options {
    pub fn admit(&self, scopes: &[String], d: &Deadline) -> Result<()> {
        ensure!(self.allow_index_read, Failure::Permission);
        d.check()?;
        self.limits.validate()?;
        ensure!(
            scopes.len() <= 4
                && scopes.iter().all(|id| valid_id(id))
                && scopes.iter().collect::<BTreeSet<_>>().len() == scopes.len()
                && (self.allow_git_sources || scopes.is_empty())
                && (!self.allow_git_sources || !scopes.is_empty()),
            Failure::Input
        );
        if let Some(r) = &self.read_request {
            ensure!(self.allow_git_sources, Failure::Permission);
            r.validate(&self.limits)?;
        }
        Ok(())
    }
}
#[derive(Clone, Debug)]
pub struct ConflictReport {
    value: Value,
    bytes: Vec<u8>,
}
impl ConflictReport {
    pub fn value(&self) -> &Value {
        &self.value
    }
    pub fn canonical_bytes(&self) -> &[u8] {
        &self.bytes
    }
}
impl Serialize for ConflictReport {
    fn serialize<S: Serializer>(&self, s: S) -> std::result::Result<S::Ok, S::Error> {
        self.value.serialize(s)
    }
}
#[derive(Debug)]
enum Failure {
    Input,
    Permission,
    Capacity,
    Source,
    Changed,
    Unavailable,
}
impl std::fmt::Display for Failure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Input => "git_conflicts_input_invalid",
            Self::Permission => "git_conflicts_permission_required",
            Self::Capacity => "git_conflicts_capacity_exceeded",
            Self::Source => "git_conflicts_source_unavailable",
            Self::Changed => "git_conflicts_index_changed",
            Self::Unavailable => "git_conflicts_unavailable",
        })
    }
}
impl std::error::Error for Failure {}
fn public(error: anyhow::Error, d: &Deadline) -> anyhow::Error {
    if crate::process::cleanup_unproven(&error) {
        return error.context("git_conflicts_cleanup_unproven");
    }
    if let Some(f) = d.failure() {
        return anyhow::anyhow!(match f {
            capture::DeadlineFailure::Cancelled => "git_conflicts_cancelled",
            capture::DeadlineFailure::Expired => "git_conflicts_deadline_exceeded",
        });
    }
    if error.is::<Failure>() {
        error
    } else {
        anyhow::anyhow!(Failure::Unavailable)
    }
}
struct ScopeBudget {
    remaining: usize,
    files: usize,
}
fn scope_record(
    store: &Store,
    kind: Option<&str>,
    id: &str,
    b: &mut ScopeBudget,
    d: &Deadline,
) -> Result<Option<Value>> {
    d.check()?;
    let raw: Option<Vec<u8>> = if let Some(kind) = kind {
        store
            .db
            .query_row(
                "SELECT substr(CAST(value AS BLOB),1,?3) FROM records WHERE kind=?1 AND id=?2",
                rusqlite::params![kind, id, b.remaining + 1],
                |r| r.get(0),
            )
            .optional()?
    } else {
        store
            .db
            .query_row(
                "SELECT substr(CAST(value AS BLOB),1,?2) FROM snapshots WHERE id=?1",
                rusqlite::params![id, b.remaining + 1],
                |r| r.get(0),
            )
            .optional()?
    };
    raw.map(|raw| {
        ensure!(raw.len() <= b.remaining, Failure::Capacity);
        b.remaining -= raw.len();
        crate::json::parse(&raw, raw.len()).map_err(|_| anyhow::anyhow!(Failure::Source))
    })
    .transpose()
}
/// Admit the complete known capture-scope closure before its shared validator
/// reads typed rows. This counts preflight bytes, not inherited resolver I/O.
fn preflight_scope(
    store: &Store,
    id: &str,
    b: &mut ScopeBudget,
    seen: &mut BTreeSet<String>,
    d: &Deadline,
) -> Result<Snapshot> {
    d.check()?;
    ensure!(
        valid_id(id) && seen.len() < 16 && seen.insert(id.into()),
        Failure::Source
    );
    let raw = scope_record(store, None, id, b, d)?.context(Failure::Source)?;
    let snapshot: Snapshot =
        crate::json::decode(raw).map_err(|_| anyhow::anyhow!(Failure::Source))?;
    ensure!(
        snapshot.id == id && snapshot.workspace_id == store.workspace_id,
        Failure::Source
    );
    b.files = b
        .files
        .checked_sub(snapshot.files.len())
        .context(Failure::Capacity)?;
    let mut parents = BTreeSet::new();
    let mut derived = false;
    let mut overlay = false;
    for kind in [
        capture::OPTIONS_RECORD_KIND,
        capture::IGNORED_RECORD_KIND,
        capture::DERIVED_OPTIONS_RECORD_KIND,
        "overlay-snapshot",
    ] {
        if let Some(v) = scope_record(store, Some(kind), id, b, d)? {
            if kind == capture::DERIVED_OPTIONS_RECORD_KIND {
                derived = true;
                parents.insert(
                    v["originSnapshotId"]
                        .as_str()
                        .context(Failure::Source)?
                        .to_owned(),
                );
            }
            if kind == "overlay-snapshot" {
                overlay = true;
                parents.insert(
                    v["baseSnapshotId"]
                        .as_str()
                        .context(Failure::Source)?
                        .to_owned(),
                );
            }
        }
    }
    // Imported/default virtual scope is not a native host acquisition policy
    // for reading this worktree's current Git objects. Validate every ancestor.
    ensure!(
        match snapshot.kind.as_str() {
            "disk" => !derived && !overlay,
            "derived" => derived && !overlay,
            "overlay" => overlay && !derived,
            _ => false,
        },
        Failure::Source
    );
    for parent in parents {
        preflight_scope(store, &parent, b, seen, d)?;
    }
    Ok(snapshot)
}
#[derive(Clone)]
struct Row {
    path: String,
    mode: String,
    oid: String,
    stage: u8,
    ordinal: usize,
}
fn parse(raw: &[u8], width: usize, limits: &Limits, d: &Deadline) -> Result<Vec<Row>> {
    d.check()?;
    ensure!(raw.len() <= limits.max_index_bytes, Failure::Capacity);
    ensure!(
        raw.is_empty() || raw.last() == Some(&0),
        Failure::Unavailable
    );
    let mut rows = vec![];
    let mut unique = BTreeSet::new();
    let mut paths = BTreeSet::new();
    if raw.is_empty() {
        return Ok(rows);
    }
    for bytes in raw[..raw.len() - 1].split(|b| *b == 0) {
        d.check()?;
        ensure!(rows.len() < limits.max_stage_rows, Failure::Capacity);
        let text = std::str::from_utf8(bytes).map_err(|_| anyhow::anyhow!(Failure::Unavailable))?;
        let (fields, path) = text.split_once('\t').context(Failure::Unavailable)?;
        ensure!(!path.is_empty() && path.len() <= 4096, Failure::Unavailable);
        let fields: Vec<_> = fields.split(' ').collect();
        ensure!(
            fields.len() == 3
                && ["100644", "100755", "120000", "160000", "040000"].contains(&fields[0])
                && oid_valid(fields[1])
                && fields[1].len() == width,
            Failure::Unavailable
        );
        let stage = match fields[2] {
            "1" => 1,
            "2" => 2,
            "3" => 3,
            _ => return Err(anyhow::anyhow!(Failure::Unavailable)),
        };
        ensure!(
            unique.insert((path.to_owned(), stage)),
            Failure::Unavailable
        );
        paths.insert(path);
        ensure!(paths.len() <= limits.max_paths, Failure::Capacity);
        rows.push(Row {
            path: path.into(),
            mode: fields[0].into(),
            oid: fields[1].into(),
            stage,
            ordinal: rows.len(),
        });
    }
    Ok(rows)
}
trait Objects {
    fn format(&mut self, d: &Deadline) -> Result<usize>;
    fn index(&mut self, max: usize, d: &Deadline) -> Result<Vec<u8>>;
    fn size(&mut self, id: &str, d: &Deadline) -> Result<usize>;
    fn bytes(&mut self, id: &str, size: usize, d: &Deadline) -> Result<Vec<u8>>;
}
struct Git<'a> {
    git: &'a HostGit,
    root: &'a Path,
    calls: usize,
}
impl Git<'_> {
    fn run(&mut self, args: &[&str], input: &[u8], max: usize, d: &Deadline) -> Result<Vec<u8>> {
        self.calls += 1;
        self.git.run(self.root, args, input, max, d)
    }
}
impl Objects for Git<'_> {
    fn format(&mut self, d: &Deadline) -> Result<usize> {
        match self
            .run(&["rev-parse", "--show-object-format"], &[], 100, d)?
            .as_slice()
        {
            b"sha1\n" => Ok(40),
            b"sha256\n" => Ok(64),
            _ => Err(anyhow::anyhow!(Failure::Unavailable)),
        }
    }
    fn index(&mut self, max: usize, d: &Deadline) -> Result<Vec<u8>> {
        let raw = self.run(
            &["ls-files", "--unmerged", "--full-name", "-z"],
            &[],
            max + 1,
            d,
        )?;
        ensure!(raw.len() <= max, Failure::Capacity);
        Ok(raw)
    }
    fn size(&mut self, id: &str, d: &Deadline) -> Result<usize> {
        let raw = self.run(
            &["cat-file", "--batch-check"],
            format!("{id}\n").as_bytes(),
            200,
            d,
        )?;
        let prefix = format!("{id} blob ");
        let text = std::str::from_utf8(&raw)?
            .strip_prefix(&prefix)
            .and_then(|s| s.strip_suffix('\n'))
            .context(Failure::Unavailable)?;
        ensure!(
            !text.is_empty()
                && text.bytes().all(|b| b.is_ascii_digit())
                && (text == "0" || !text.starts_with('0')),
            Failure::Unavailable
        );
        text.parse().map_err(|_| anyhow::anyhow!(Failure::Capacity))
    }
    fn bytes(&mut self, id: &str, size: usize, d: &Deadline) -> Result<Vec<u8>> {
        let raw = self.run(
            &["cat-file", "--batch"],
            format!("{id}\n").as_bytes(),
            size + 100,
            d,
        )?;
        let prefix = format!("{id} blob {size}\n");
        ensure!(
            raw.starts_with(prefix.as_bytes())
                && raw.len() == prefix.len() + size + 1
                && raw.last() == Some(&b'\n'),
            Failure::Unavailable
        );
        Ok(raw[prefix.len()..raw.len() - 1].to_vec())
    }
}
struct Source {
    hash: String,
    size: usize,
    text: Option<String>,
    status: &'static str,
}
fn quote(
    source: &Source,
    start: usize,
    max: usize,
    remaining: &mut usize,
    d: &Deadline,
) -> Result<Value> {
    d.check()?;
    let text = source.text.as_ref().context(Failure::Source)?;
    ensure!(
        start <= text.len() && text.is_char_boundary(start),
        Failure::Input
    );
    let mut end = start + max.min(*remaining).min(text.len() - start);
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    if max > 0 && start < text.len() {
        ensure!(end > start, Failure::Capacity);
    }
    *remaining -= end - start;
    Ok(
        json!({"coordinateSpace":"git_blob_utf8","startByte":start,"endByte":end,"text":&text[start..end],
        "completeSource":start==0 && end==text.len(),"remainingSourceBytes":text.len()-end}),
    )
}
fn observe(
    objects: &mut impl Objects,
    allowed: &BTreeMap<String, Vec<String>>,
    options: &Options,
    d: &Deadline,
) -> Result<Value> {
    let l = &options.limits;
    let width = objects.format(d)?;
    ensure!(matches!(width, 40 | 64), Failure::Unavailable);
    let raw = objects.index(l.max_index_bytes, d)?;
    let index_hash = hash(&raw);
    if let Some(r) = &options.read_request {
        ensure!(r.expected_index_hash == index_hash, Failure::Changed);
    }
    let rows = parse(&raw, width, l, d)?;
    if let Some(r) = &options.read_request {
        ensure!(
            rows.iter()
                .any(|row| row.path == r.path && row.stage == r.stage),
            Failure::Source
        );
    }
    let mut cache = BTreeMap::<String, Source>::new();
    let mut source_bytes = 0usize;
    let mut quote_remaining = l.max_total_quote_bytes;
    let mut output_rows = vec![];
    let mut groups: Vec<(String, Vec<usize>)> = vec![];
    let mut group_index = BTreeMap::new();
    let mut selected_page = Value::Null;
    for row in &rows {
        d.check()?;
        let syntax = crate::paths::normalized_syntax(&row.path).is_ok()
            && !crate::paths::obvious_secret(&row.path);
        let scopes = allowed.get(&row.path).filter(|_| syntax);
        let safe = syntax && (!crate::paths::protected(&row.path) || scopes.is_some());
        let selected = options
            .read_request
            .as_ref()
            .is_none_or(|r| r.path == row.path && r.stage == row.stage);
        let mut item = json!({"rowOrdinal":row.ordinal,"path":if safe {Some(&row.path)}else{None},"pathSha256":hash(row.path.as_bytes()),
            "mode":row.mode,"gitObjectId":row.oid,"stage":row.stage,"role":match row.stage {1=>"base",2=>"index_ours",_=>"index_theirs"},
            "scopeSnapshotIds":scopes.cloned().unwrap_or_default(),"sourceSpace":"git_index_stage_blob","sourceStatus":"unavailable","quote":null});
        let reason = if !safe {
            Some("protected_or_noncanonical_path")
        } else if !options.allow_git_sources {
            Some("host_source_permission_required")
        } else if scopes.is_none() {
            Some("outside_selected_captured_paths")
        } else if !["100644", "100755"].contains(&row.mode.as_str()) {
            Some("unsupported_index_mode")
        } else if !selected {
            Some("not_selected")
        } else {
            None
        };
        if let Some(reason) = reason {
            item["sourceUnavailableReason"] = json!(reason);
            if selected && options.read_request.is_some() {
                return Err(anyhow::anyhow!(Failure::Source));
            }
        } else {
            if !cache.contains_key(&row.oid) {
                ensure!(cache.len() < l.max_blob_objects, Failure::Capacity);
                let size = objects.size(&row.oid, d)?;
                ensure!(
                    size <= l.max_blob_bytes
                        && size <= l.max_source_bytes.saturating_sub(source_bytes),
                    Failure::Capacity
                );
                source_bytes += size;
                let bytes = objects.bytes(&row.oid, size, d)?;
                ensure!(
                    bytes.len() == size && object_id("blob", &bytes, width) == row.oid,
                    Failure::Source
                );
                let hash = hash(&bytes);
                let (text, status) = match String::from_utf8(bytes) {
                    Ok(text) if crate::paths::obvious_secret(&text) => {
                        (None, "verified_obvious_secret_omitted")
                    }
                    Ok(text)
                        if text
                            .chars()
                            .any(|c| c.is_control() && !matches!(c, '\n' | '\r' | '\t')) =>
                    {
                        (None, "verified_unsupported_control_text")
                    }
                    Ok(text) => (Some(text), "verified_utf8"),
                    Err(_) => (None, "verified_non_utf8"),
                };
                cache.insert(
                    row.oid.clone(),
                    Source {
                        hash,
                        size,
                        text,
                        status,
                    },
                );
            }
            let source = &cache[&row.oid];
            item["sourceStatus"] = json!(source.status);
            item["contentSha256"] = json!(source.hash);
            item["objectBytes"] = json!(source.size);
            item["gitBlobHashVerified"] = json!(true);
            if let Some(r) = &options.read_request {
                let page = quote(source, r.start_byte, r.max_bytes, &mut quote_remaining, d)?;
                item["quote"] = page.clone();
                selected_page = json!({"request":r,"rowOrdinal":row.ordinal,"gitObjectId":row.oid,"contentSha256":source.hash,"objectBytes":source.size,"range":page});
            } else if source.text.is_some() {
                if source.size == 0 || (l.max_quote_bytes > 0 && quote_remaining > 0) {
                    item["quote"] = quote(source, 0, l.max_quote_bytes, &mut quote_remaining, d)?;
                } else {
                    item["quoteUnavailableReason"] = json!("quote_budget");
                }
            }
        }
        let g = *group_index.entry(row.path.clone()).or_insert_with(|| {
            groups.push((row.path.clone(), vec![]));
            groups.len() - 1
        });
        groups[g].1.push(row.ordinal);
        output_rows.push(item);
    }
    let mut conflicts = vec![];
    for (path, indices) in groups {
        d.check()?;
        let mut stages = BTreeMap::new();
        for i in &indices {
            stages.insert(rows[*i].stage, &rows[*i]);
        }
        let mask = stages
            .keys()
            .fold(0u8, |mask, stage| mask | 1 << (stage - 1));
        let same = |a, b| -> Option<bool> {
            Some(
                stages.get(&a)?.oid == stages.get(&b)?.oid
                    && stages.get(&a)?.mode == stages.get(&b)?.mode,
            )
        };
        let safe = crate::paths::normalized_syntax(&path).is_ok()
            && !crate::paths::obvious_secret(&path)
            && (!crate::paths::protected(&path) || allowed.contains_key(&path));
        conflicts.push(json!({"path":if safe {Some(&path)}else{None},"pathSha256":hash(path.as_bytes()),"stageRowOrdinals":indices,
            "stagePresence":stages.keys().collect::<Vec<_>>(),"absentStages":(1..=3).filter(|s|!stages.contains_key(s)).collect::<Vec<_>>(),
            "indexPattern":match mask {1=>"base_only_unmerged",2=>"ours_only_unmerged",4=>"theirs_only_unmerged",3=>"theirs_stage_absent",5=>"ours_stage_absent",6=>"base_stage_absent",7=>"three_way_unmerged",_=>return Err(anyhow::anyhow!(Failure::Unavailable))},
            "baseOursObjectAndModeIdentical":same(1,2),"baseTheirsObjectAndModeIdentical":same(1,3),"oursTheirsObjectAndModeIdentical":same(2,3),
            "indexResolved":false,"causeInferred":false}));
    }
    d.check()?;
    ensure!(
        objects.index(l.max_index_bytes, d)? == raw && objects.format(d)? == width,
        Failure::Changed
    );
    Ok(
        json!({"schemaVersion":1,"kind":"native_git_index_conflicts","objectFormat":if width==40 {"sha1"}else{"sha256"},
        "indexHash":index_hash,"matchingIndexObservations":2,"indexLocked":false,"indexObservationComplete":true,
        "stageRows":output_rows,"conflicts":conflicts,"selectedSourcePage":selected_page,
        "consumed":{"unmergedPaths":group_index.len(),"stageRows":rows.len(),"indexBytesPerObservation":raw.len(),"uniqueBlobObjects":cache.len(),"verifiedGitBlobBytes":source_bytes,"quotedBytes":l.max_total_quote_bytes-quote_remaining},
        "limits":l,"canonicalSnapshotSourcesRead":false,"workingFilesRead":false,"workingTreeResolutionVerified":false,
        "stateWritten":false,"sourceWritten":false,"latestChanged":false,"networkRequests":0,"modelCalls":0,"complete":false,
        "limitations":["This is the selected host worktree's observed unmerged index, not a predicted branch-overlap conflict or a resolution. Equal pre/post observations neither lock the index nor attest future state or intervening ABA changes.",
        "Index stage roles are not authenticated branch/author identities. Missing stages and identical object/mode pairs do not establish a conflict's cause or an automatic resolution.",
        "Host-permitted Git blob bytes have exact stage-qualified coordinates. Selected captured-path scope is a read restriction, not proof that these current Git bytes equal a canonical snapshot, graph or working file. Unsupported, protected, unsafe or unrequested sources remain explicit omissions. No other worktree path is followed."]}),
    )
}
pub fn inspect(
    store: &Store,
    root: &Path,
    scope_snapshot_ids: &[String],
    git: &HostGit,
    options: &Options,
    d: &Deadline,
) -> Result<ConflictReport> {
    let run = || -> Result<ConflictReport> {
        options.admit(scope_snapshot_ids, d)?;
        ensure!(
            root.is_absolute() && store.db.is_autocommit(),
            Failure::Input
        );
        for ancestor in root.ancestors() {
            d.check()?;
            ensure!(
                !crate::paths::is_link(&std::fs::symlink_metadata(ancestor)?),
                Failure::Input
            );
        }
        let root = root.canonicalize()?;
        ensure!(
            root.join(".yoke").canonicalize()? == store.dir.canonicalize()?,
            Failure::Source
        );
        crate::github_app::worker::publication_policy(store, d, |d| {
            let prior = store.db.limit(Limit::SQLITE_LIMIT_LENGTH)?;
            store
                .db
                .set_limit(Limit::SQLITE_LIMIT_LENGTH, prior.min(16_004_096))?;
            let tx = Transaction::new_unchecked(&store.db, TransactionBehavior::Deferred)?;
            let mut b = ScopeBudget {
                remaining: options.limits.max_scope_metadata_bytes,
                files: options.limits.max_scope_files,
            };
            let mut snapshots = vec![];
            for id in scope_snapshot_ids.iter().collect::<BTreeSet<_>>() {
                snapshots.push(preflight_scope(store, id, &mut b, &mut BTreeSet::new(), d)?);
            }
            let scopes = snapshots
                .iter()
                .map(|s| capture::source_scope_policy_at(store, s, AnalyzerEpoch::V9))
                .collect::<Result<Vec<_>>>()?;
            let mut objects = Git {
                git,
                root: &root,
                calls: 0,
            };
            ensure!(
                objects.run(&["rev-parse", "--is-inside-work-tree"], &[], 100, d)? == b"true\n"
                    && objects.run(&["rev-parse", "--show-prefix"], &[], 4096, d)? == b"\n",
                Failure::Input
            );
            // Only observed paths which pass a selected immutable scope can
            // authorize a separate host-permitted Git-stage body read.
            let mut allowed = BTreeMap::<String, Vec<String>>::new();
            for (snapshot, scope) in snapshots.iter().zip(&scopes) {
                for file in &snapshot.files {
                    d.check()?;
                    scope.file(&file.path)?;
                    allowed
                        .entry(file.path.clone())
                        .or_default()
                        .push(snapshot.id.clone());
                }
            }
            let mut value = observe(&mut objects, &allowed, options, d)?;
            value["workspaceId"] = json!(store.workspace_id);
            value["scopeSnapshots"] = json!(
                snapshots
                    .iter()
                    .map(|s| json!({"snapshotId":s.id,"snapshotHash":identity(s),"kind":s.kind}))
                    .collect::<Vec<_>>()
            );
            value["worktreeSelection"] = json!("explicit_host_root");
            value["gitSubprocesses"] = json!(objects.calls);
            value["consumed"]["scopePreflightMetadataBytes"] =
                json!(options.limits.max_scope_metadata_bytes - b.remaining);
            value["consumed"]["scopeFiles"] = json!(options.limits.max_scope_files - b.files);
            let id = identity(&value);
            value["resultId"] = json!(id);
            let bytes = crate::json::encode_bounded(&value, options.limits.max_output_bytes, d)
                .context(Failure::Capacity)?;
            d.check()?;
            tx.commit()?;
            Ok(ConflictReport { value, bytes })
        })
    };
    run().map_err(|error| public(error, d))
}
#[cfg(test)]
#[path = "git_conflicts_tests.rs"]
mod tests;
