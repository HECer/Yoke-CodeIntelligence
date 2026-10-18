//! Read-only, bounded native entity replay over one captured source scope.
use super::{CAPACITY, Options, PreviewRequest, SOURCE, rust};
use crate::{
    capture::Deadline,
    model::{Graph, Snapshot, SourceFile, identity},
    store::Store,
};
use anyhow::{Context as _, Result, ensure};
use rusqlite::limits::Limit;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use tree_sitter::{ParseOptions, Parser, Tree};

pub(super) struct Context<'a> {
    pub(super) store: &'a Store,
    pub(super) options: &'a Options,
}

pub(super) struct ReadPolicy<'a> {
    store: &'a Store,
    deadline: Option<Deadline>,
    length: i32,
    busy: u64,
    deadline_active: bool,
}
impl<'a> ReadPolicy<'a> {
    pub(super) fn enter(store: &'a Store, d: &Deadline, max: usize) -> Result<Self> {
        let length = store.db.limit(Limit::SQLITE_LIMIT_LENGTH)?;
        let busy = store
            .db
            .query_row("PRAGMA busy_timeout", [], |row| row.get(0))?;
        let policy = Self {
            store,
            deadline: store.replace_deadline(Some(d.clone())),
            length,
            busy,
            deadline_active: true,
        };
        store
            .db
            .set_limit(Limit::SQLITE_LIMIT_LENGTH, length.min((max + 4096) as i32))?;
        store.db.busy_timeout(std::time::Duration::ZERO)?;
        d.check()?;
        Ok(policy)
    }
    pub(super) fn restore(&mut self) -> Result<()> {
        let length = self
            .store
            .db
            .set_limit(Limit::SQLITE_LIMIT_LENGTH, self.length);
        if self.deadline_active {
            self.store.replace_deadline(self.deadline.take());
            self.deadline_active = false;
        }
        let busy = self
            .store
            .db
            .busy_timeout(std::time::Duration::from_millis(self.busy));
        ensure!(
            length.is_ok()
                && busy.is_ok()
                && self.store.db.limit(Limit::SQLITE_LIMIT_LENGTH).ok() == Some(self.length),
            "github_app_digest_sql_policy_restore_failed"
        );
        Ok(())
    }
}
impl Drop for ReadPolicy<'_> {
    fn drop(&mut self) {
        let _ = self.restore();
    }
}

pub(super) struct Projection {
    pub symbols: Vec<Value>,
    pub sources: Vec<Value>,
    pub candidates: usize,
    pub omissions: usize,
    pub unsupported_files: usize,
    pub visibility_unresolved: usize,
    pub analyzer: String,
    pub entities: usize,
    pub ast_nodes: usize,
    pub commit_id: Option<String>,
    pub tree_id: Option<String>,
}
fn metadata<T: DeserializeOwned>(
    c: &Context<'_>,
    table: &str,
    id: &str,
    remaining: &mut usize,
    d: &Deadline,
) -> Result<T> {
    d.check()?;
    // Both table names are private constants. The substring prevents a large
    // stored value from becoming an unbounded Rust allocation before checking.
    let raw: Vec<u8> = loop {
        d.check()?;
        let result = c.store.db.query_row(
            &format!("SELECT substr(CAST(value AS BLOB),1,?2) FROM {table} WHERE id=?1"),
            rusqlite::params![id, (*remaining + 1) as i64],
            |row| row.get(0),
        );
        d.check()?;
        match result {
            Ok(raw) => break raw,
            Err(rusqlite::Error::SqliteFailure(error, _))
                if matches!(
                    error.code,
                    rusqlite::ErrorCode::DatabaseBusy | rusqlite::ErrorCode::DatabaseLocked
                ) =>
            {
                // SQLite's native busy sleep can overshoot short Windows
                // deadlines substantially. Keep lock polling host-owned.
                std::thread::sleep(d.remaining().min(std::time::Duration::from_millis(2)));
            }
            Err(_) => return Err(anyhow::anyhow!(SOURCE)),
        }
    };
    ensure!(raw.len() <= *remaining, CAPACITY);
    *remaining -= raw.len();
    let value = serde_json::from_slice(&raw).context(SOURCE)?;
    d.check()?;
    Ok(value)
}
struct Work<'a> {
    options: &'a Options,
    epoch: crate::analysis::AnalyzerEpoch,
    nodes: usize,
    name_bytes: usize,
    entities: usize,
}
impl Work<'_> {
    fn names(&mut self, bytes: usize) -> Result<()> {
        ensure!(
            bytes
                <= self
                    .options
                    .max_replay_name_bytes
                    .saturating_sub(self.name_bytes),
            CAPACITY
        );
        self.name_bytes += bytes;
        Ok(())
    }
    fn parsed(&mut self, file: &SourceFile, text: &str, d: &Deadline) -> Result<Tree> {
        let grammar = crate::analysis::grammar("rust").context(SOURCE)?;
        d.check()?;
        let mut parser = Parser::new();
        parser.set_language(&grammar)?;
        let mut cancelled = |_: &tree_sitter::ParseState| d.check().is_err();
        let tree = parser
            .parse_with_options(
                &mut |offset, _| &text.as_bytes()[offset.min(text.len())..],
                None,
                Some(ParseOptions::new().progress_callback(&mut cancelled)),
            )
            .context(SOURCE)?;
        d.check()?;
        // A cursor bounds traversal memory even for wide trees. All nodes, including
        // private declarations and function bodies, consume the shared work budget.
        let mut cursor = tree.walk();
        let mut owners = vec![file.path.len()];
        ensure!(self.entities < self.options.max_entities, CAPACITY);
        self.entities += 1;
        self.names(file.path.len())?;
        loop {
            d.check()?;
            ensure!(
                self.nodes < self.options.max_ast_nodes
                    && owners.len() <= self.options.max_ast_depth,
                CAPACITY
            );
            self.nodes += 1;
            let node = cursor.node();
            let mut owner = *owners.last().context(SOURCE)?;
            if let Some((_, name)) = crate::analysis::symbol(self.epoch, node, text, "rust") {
                let size = name.end_byte() - name.start_byte();
                if (1..=1000).contains(&size) {
                    ensure!(self.entities < self.options.max_entities, CAPACITY);
                    self.entities += 1;
                    owner += 1 + size;
                    *owners.last_mut().context(SOURCE)? = owner;
                }
            }
            // Extraction clones its owner while visiting nodes, including private
            // nodes. Charge those qualified-name copies before invoking extraction.
            self.names(owner)?;
            // Native call targets, receiver expressions and pattern labels may
            // copy overlapping source. Charge every AST span conservatively.
            self.names(node.end_byte() - node.start_byte())?;
            if node.kind().contains("comment") {
                let comment = node.utf8_text(text.as_bytes()).context(SOURCE)?;
                if ["WHY:", "NOTE:", "ADR-", "RFC-"]
                    .iter()
                    .any(|mark| comment.contains(mark))
                {
                    self.names(owner + 1 + comment.trim().len())?;
                }
            }
            if cursor.goto_first_child() {
                owners.push(owner);
                continue;
            }
            loop {
                if cursor.goto_next_sibling() {
                    let parent = owners
                        .get(owners.len().saturating_sub(2))
                        .copied()
                        .unwrap_or(file.path.len());
                    *owners.last_mut().context(SOURCE)? = parent;
                    break;
                }
                if !cursor.goto_parent() {
                    drop(cursor);
                    return Ok(tree);
                }
                owners.pop();
            }
        }
    }
}
pub(super) fn project(
    c: &Context<'_>,
    request: &PreviewRequest,
    d: &Deadline,
) -> Result<Projection> {
    let mut remaining = c.options.max_metadata_bytes;
    let snapshot: Snapshot = metadata(c, "snapshots", &request.snapshot_id, &mut remaining, d)?;
    ensure!(
        snapshot.id == request.snapshot_id
            && snapshot.workspace_id == c.store.workspace_id
            && matches!(snapshot.kind.as_str(), "disk" | "derived"),
        SOURCE
    );
    ensure!(snapshot.files.len() <= c.options.max_files, CAPACITY);
    let mut bytes = 0usize;
    for file in &snapshot.files {
        d.check()?;
        ensure!(
            file.size <= c.options.max_source_bytes.saturating_sub(bytes),
            CAPACITY
        );
        bytes += file.size;
    }
    let has_git = crate::git_workflows::has_source_proof(c.store, &snapshot.id).context(SOURCE)?;
    let (epoch, commit_id, tree_id, mut retained) = if snapshot.kind == "derived" {
        ensure!(has_git, SOURCE);
        let (epoch, commit, tree, sources) = crate::git_workflows::source_read::verified(
            c.store,
            &snapshot,
            c.options.max_source_bytes,
            c.options.max_lineage_metadata_bytes,
            d,
        )
        .map_err(lineage_error)?
        .into_parts();
        (epoch, Some(commit), Some(tree), Some(sources))
    } else {
        ensure!(!has_git, SOURCE);
        crate::git_workflows::source_read::disk_preflight(
            c.store,
            &snapshot,
            c.options.max_lineage_metadata_bytes,
            d,
        )
        .map_err(lineage_error)?;
        let epoch =
            crate::capture::analyzer_epoch_for_snapshot_with_deadline(c.store, &snapshot, d)
                .context(SOURCE)?;
        (epoch, None, None, None)
    };
    let scope =
        crate::capture::source_scope_policy_at(c.store, &snapshot, epoch).context(SOURCE)?;
    let graph: Graph = metadata(c, "graphs", &snapshot.id, &mut remaining, d)?;
    ensure!(graph.snapshot_id == snapshot.id, SOURCE);
    ensure!(
        graph.entities.len() <= c.options.max_entities
            && graph.observations.len() <= c.options.max_ast_nodes
            && graph.evidence.len() <= c.options.max_ast_nodes,
        CAPACITY
    );
    let rust_paths: BTreeSet<_> = snapshot
        .files
        .iter()
        .filter(|file| rust_path(file))
        .map(|file| file.path.as_str())
        .collect();
    let mut ids = BTreeSet::new();
    let mut stored = BTreeMap::new();
    for entity in &graph.entities {
        d.check()?;
        ensure!(
            entity.snapshot_id == snapshot.id && ids.insert(entity.id.as_str()),
            SOURCE
        );
        if rust_paths.contains(entity.path.as_str()) {
            stored.insert(entity.id.as_str(), entity);
        }
    }
    let mut seen = BTreeSet::new();
    let mut output = Projection {
        symbols: Vec::new(),
        sources: Vec::new(),
        candidates: 0,
        omissions: 0,
        unsupported_files: 0,
        visibility_unresolved: 0,
        analyzer: epoch.producer().into(),
        entities: 0,
        ast_nodes: 0,
        commit_id,
        tree_id,
    };
    let mut work = Work {
        options: c.options,
        epoch,
        nodes: 0,
        name_bytes: 0,
        entities: 0,
    };
    for file in &snapshot.files {
        d.check()?;
        scope.path_policy(&file.path).context(SOURCE)?;
        let text = if let Some(sources) = &mut retained {
            sources.remove(&file.path).context(SOURCE)?
        } else {
            String::from_utf8(scope.read_source(&file.path).context(SOURCE)?).context(SOURCE)?
        };
        output.sources.push(json!({"path":file.path,"contentHash":file.hash,"bytes":file.size,
            "language":file.language,"capturedBytesVerified":true,"exportPolicyAvailable":rust_path(file)}));
        if !rust_path(file) {
            output.unsupported_files += 1;
            continue;
        }
        let tree = work.parsed(file, &text, d)?;
        let declarations =
            crate::analysis::declarations::rust_at(epoch, file, &text, &tree, d).context(SOURCE)?;
        ensure!(
            declarations.len() <= c.options.max_entities.saturating_sub(output.entities),
            CAPACITY
        );
        let mut spans = BTreeMap::new();
        for mut entity in declarations {
            d.check()?;
            entity.snapshot_id.clone_from(&snapshot.id);
            entity.id = identity(&(&snapshot.id, &entity.key));
            ensure!(
                stored
                    .get(entity.id.as_str())
                    .is_some_and(|saved| identity(*saved) == identity(&entity))
                    && seen.insert(entity.id.clone()),
                SOURCE
            );
            output.entities += 1;
            if entity.producer == epoch.producer() && entity.kind != "file" {
                spans.insert((entity.start_byte, entity.end_byte), entity);
            }
        }
        rust::project(
            &tree,
            &text,
            file,
            &snapshot.id,
            &spans,
            c.options,
            &mut output,
            d,
        )?;
    }
    ensure!(
        stored
            .values()
            .all(|entity| entity.producer != epoch.producer()
                || entity.kind == "rationale"
                || seen.contains(&entity.id)),
        SOURCE
    );
    output.omissions = output.candidates - output.symbols.len();
    output.ast_nodes = work.nodes;
    ensure!(retained.is_none_or(|sources| sources.is_empty()), SOURCE);
    d.check()?;
    Ok(output)
}

fn lineage_error(error: anyhow::Error) -> anyhow::Error {
    if error.is::<crate::git_workflows::source_read::LineageCapacity>() {
        anyhow::anyhow!(CAPACITY)
    } else {
        error.context(SOURCE)
    }
}

fn rust_path(file: &SourceFile) -> bool {
    file.language == "rust"
        && file
            .path
            .rsplit('.')
            .next()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("rs"))
}
