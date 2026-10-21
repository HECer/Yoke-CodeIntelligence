//! Named authored application notes, distinct from central AgentMemory.
use crate::{
    capture::Deadline,
    model::{Snapshot, identity, valid_id},
    paths,
    store::Store,
};
use anyhow::{Context, Result, bail, ensure};
use regex::{Regex, RegexBuilder, RegexSet, RegexSetBuilder};
use rusqlite::{OptionalExtension, Transaction, TransactionBehavior, params};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
pub const REVISION_KIND: &str = "native_named_note_revision_v1";
pub const HEAD_KIND: &str = "native_named_note_head_v1";
const MAX_CONTENT_BYTES: usize = 64_000;
const MAX_REVISION_BYTES: usize = 384_000;
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Scope {
    Project,
    Global,
}
#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Policy {
    #[serde(default)]
    pub read_only_patterns: Vec<String>,
    #[serde(default)]
    pub ignored_patterns: Vec<String>,
    #[serde(default)]
    pub writable: bool,
}
/// Explicit session regex restrictions are separate from the original glob policy.
pub(crate) struct MemoryPatterns {
    readonly: RegexSet,
    ignored: RegexSet,
}
pub(crate) fn memory_patterns(readonly: &[String], ignored: &[String]) -> Result<MemoryPatterns> {
    memory_pattern_limits(readonly, ignored)?;
    let build = |values: &[String]| {
        RegexSetBuilder::new(values)
            .size_limit(1000000)
            .dfa_size_limit(1000000)
            .build()
            .context("Invalid or oversized memory regex policy")
    };
    Ok(MemoryPatterns {
        readonly: build(readonly)?,
        ignored: build(ignored)?,
    })
}
pub(crate) fn memory_pattern_limits(readonly: &[String], ignored: &[String]) -> Result<()> {
    ensure!(
        readonly.len() <= 100
            && ignored.len() <= 100
            && readonly
                .iter()
                .chain(ignored)
                .map(String::len)
                .sum::<usize>()
                <= 32768,
        "Memory regex policy capacity exceeded"
    );
    for values in [readonly, ignored] {
        ensure!(
            values.iter().collect::<BTreeSet<_>>().len() == values.len()
                && values.iter().all(|s| s.len() <= 4096 && !s.contains('\0')),
            "Invalid or duplicate memory regex policy"
        );
    }
    Ok(())
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Anchor {
    path: String,
    content_hash: String,
    snapshot_id: String,
    source_workspace_id: String,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Note {
    id: String,
    name: String,
    scope: Scope,
    storage_workspace_id: String,
    source_workspace_id: String,
    snapshot_id: String,
    content: String,
    anchors: Vec<Anchor>,
    parent_revision: Option<String>,
    created_ms: u64,
    nonce: String,
    deleted: bool,
    trust: String,
}
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Head {
    name: String,
    scope: Scope,
    revision_id: String,
}
struct Rules {
    readonly: Vec<Regex>,
    ignored: Vec<Regex>,
    readonly_regex: Option<RegexSet>,
    ignored_regex: Option<RegexSet>,
    writable: bool,
    scope: Scope,
}
impl Rules {
    fn new(p: &Policy, scope: Scope, patterns: Option<&MemoryPatterns>) -> Result<Self> {
        ensure!(
            p.read_only_patterns.len() <= 100 && p.ignored_patterns.len() <= 100,
            "Too many note policies"
        );
        Ok(Self {
            readonly: p
                .read_only_patterns
                .iter()
                .map(|p| crate::query_features::glob(p, false))
                .collect::<Result<_>>()?,
            ignored: p
                .ignored_patterns
                .iter()
                .map(|p| crate::query_features::glob(p, false))
                .collect::<Result<_>>()?,
            readonly_regex: patterns.map(|p| p.readonly.clone()),
            ignored_regex: patterns.map(|p| p.ignored.clone()),
            writable: p.writable,
            scope,
        })
    }
    fn hidden(&self, n: &str) -> bool {
        self.ignored.iter().any(|r| {
            r.is_match(n) || self.scope == Scope::Global && r.is_match(&format!("global/{n}"))
        }) || self.regex_matches(&self.ignored_regex, n)
    }
    fn readonly(&self, n: &str) -> bool {
        !self.writable
            || self.readonly.iter().any(|r| {
                r.is_match(n) || self.scope == Scope::Global && r.is_match(&format!("global/{n}"))
            })
            || self.regex_matches(&self.readonly_regex, n)
    }
    fn regex_matches(&self, patterns: &Option<RegexSet>, name: &str) -> bool {
        patterns.as_ref().is_some_and(|patterns| {
            if self.scope == Scope::Global {
                patterns.is_match(&format!("global/{name}"))
            } else {
                patterns.is_match(name)
            }
        })
    }
    fn visible(&self, n: &str) -> Result<()> {
        ensure!(!self.hidden(n), "Note unavailable under host policy");
        Ok(())
    }
    fn editable(&self, n: &str) -> Result<()> {
        self.visible(n)?;
        ensure!(!self.readonly(n), "Note is read-only");
        Ok(())
    }
}
fn name(a: &Value, k: &str, scope: Scope) -> Result<String> {
    let n = a[k].as_str().context("Note name required")?;
    ensure!(
        !n.is_empty() && n.len() <= 500 && !n.chars().any(char::is_control),
        "Invalid note name"
    );
    ensure!(
        scope != Scope::Project || !n.starts_with("global/"),
        "The global note namespace requires global scope"
    );
    let normal = paths::normalized(n)?;
    ensure!(
        normal == n && !paths::protected(n),
        "Invalid or protected note name"
    );
    Ok(normal)
}
fn content(s: &str) -> Result<()> {
    ensure!(
        s.len() <= MAX_CONTENT_BYTES
            && !s
                .chars()
                .any(|c| c.is_control() && !matches!(c, '\n' | '\r' | '\t')),
        "Invalid note content size or controls"
    );
    ensure!(!paths::obvious_secret(s), "Obvious secret content refused");
    Ok(())
}
fn key(s: &Store, scope: Scope, n: &str) -> String {
    identity(&(HEAD_KIND, &s.workspace_id, scope, n))
}
fn note_id(n: &Note) -> String {
    let mut n = n.clone();
    n.id.clear();
    identity(&n)
}
pub(crate) fn validate_retention_record(
    store: &Store,
    kind: &str,
    id: &str,
    value: &Value,
) -> Result<()> {
    store.check_deadline()?;
    ensure!(valid_id(id), "Invalid retained note ID");
    if kind == HEAD_KIND {
        let head: Head = serde_json::from_value(value.clone())?;
        ensure!(
            serde_json::to_value(&head)? == *value
                && key(store, head.scope, &head.name) == id
                && valid_id(&head.revision_id),
            "Invalid retained note head"
        );
        name(&json!({"name":head.name}), "name", head.scope)?;
        let revision: Value = store.get_record(REVISION_KIND, &head.revision_id)?;
        validate_retention_record(store, REVISION_KIND, &head.revision_id, &revision)?;
        ensure!(
            revision["name"] == head.name && revision["scope"] == serde_json::to_value(head.scope)?,
            "Retained head revision mismatch"
        );
    } else {
        ensure!(kind == REVISION_KIND, "Unknown retained note kind");
        let note: Note = serde_json::from_value(value.clone())?;
        ensure!(
            serde_json::to_value(&note)? == *value
                && note.id == id
                && note_id(&note) == id
                && note.storage_workspace_id == store.workspace_id
                && valid_id(&note.source_workspace_id)
                && valid_id(&note.snapshot_id)
                && note.parent_revision.as_ref().is_none_or(|id| valid_id(id))
                && note.nonce.len() == 32
                && note.nonce.chars().all(|ch| ch.is_ascii_hexdigit())
                && note.anchors.len() <= 100
                && note.trust == "authored_data_not_execution_authority",
            "Invalid retained note revision"
        );
        name(&json!({"name":note.name}), "name", note.scope)?;
        content(&note.content)?;
        for anchor in &note.anchors {
            store.check_deadline()?;
            paths::normalized(&anchor.path)?;
            ensure!(
                valid_id(&anchor.content_hash)
                    && valid_id(&anchor.snapshot_id)
                    && valid_id(&anchor.source_workspace_id),
                "Invalid retained note anchor"
            );
            if anchor.source_workspace_id == store.workspace_id {
                let snapshot = store.snapshot(&anchor.snapshot_id)?;
                ensure!(
                    snapshot
                        .files
                        .iter()
                        .any(|file| file.path == anchor.path && file.hash == anchor.content_hash),
                    "Retained note source binding mismatch"
                );
            }
        }
    }
    Ok(())
}
fn load(t: &Transaction<'_>, s: &Store, scope: Scope, n: &str) -> Result<Option<Note>> {
    let raw: Option<String> = t
        .query_row(
            "SELECT value FROM records WHERE kind=?1 AND id=?2",
            params![HEAD_KIND, key(s, scope, n)],
            |r| r.get(0),
        )
        .optional()?;
    let Some(raw) = raw else { return Ok(None) };
    ensure!(raw.len() <= 2000, "Note head capacity exceeded");
    let h: Head = serde_json::from_str(&raw)?;
    ensure!(
        h.name == n && h.scope == scope && valid_id(&h.revision_id),
        "Note head mismatch"
    );
    let raw: String = t.query_row(
        "SELECT value FROM records WHERE kind=?1 AND id=?2",
        params![REVISION_KIND, h.revision_id],
        |r| r.get(0),
    )?;
    ensure!(
        raw.len() <= MAX_REVISION_BYTES,
        "Note revision capacity exceeded"
    );
    let note: Note = serde_json::from_str(&raw)?;
    ensure!(
        note.id == h.revision_id
            && note.id == note_id(&note)
            && note.name == n
            && note.scope == scope
            && note.storage_workspace_id == s.workspace_id
            && valid_id(&note.snapshot_id)
            && valid_id(&note.source_workspace_id),
        "Note revision integrity mismatch"
    );
    content(&note.content)?;
    ensure!(
        note.anchors.len() <= 100
            && note.anchors.iter().all(|a| valid_id(&a.content_hash)
                && valid_id(&a.snapshot_id)
                && valid_id(&a.source_workspace_id)
                && paths::normalized(&a.path).is_ok()),
        "Invalid note anchors"
    );
    Ok(Some(note))
}
fn all(
    t: &Transaction<'_>,
    s: &Store,
    scope: Scope,
    d: &Deadline,
) -> Result<BTreeMap<String, Note>> {
    let mut stmt = t.prepare("SELECT value FROM records WHERE kind=?1 ORDER BY id LIMIT 2001")?;
    let heads = stmt
        .query_map([HEAD_KIND], |r| r.get::<_, String>(0))?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    ensure!(heads.len() <= 2000, "Named note capacity exceeded");
    let mut notes = BTreeMap::new();
    let mut bytes = 0usize;
    for raw in heads {
        d.check()?;
        ensure!(raw.len() <= 2000, "Note head capacity exceeded");
        let h: Head = serde_json::from_str(&raw)?;
        if h.scope != scope {
            continue;
        }
        let n = load(t, s, scope, &h.name)?.context("Missing note head")?;
        if n.deleted {
            continue;
        }
        bytes = bytes.saturating_add(n.content.len());
        ensure!(bytes <= 32000000, "Note content capacity exceeded");
        notes.insert(h.name, n);
    }
    Ok(notes)
}
fn current(t: &Transaction<'_>, s: &Store, scope: Scope, n: &str) -> Result<Note> {
    load(t, s, scope, n)?
        .filter(|n| !n.deleted)
        .context("Named note unavailable")
}
fn expected(a: &Value, n: Option<&Note>) -> Result<()> {
    if let Some(id) = a.get("expectedRevision") {
        ensure!(
            id.as_str().is_some_and(valid_id) && n.is_some_and(|n| id == n.id.as_str()),
            "Stale note revision"
        );
    }
    Ok(())
}
fn anchors(s: &Store, snap: &Snapshot, a: &Value, prior: Option<&Note>) -> Result<Vec<Anchor>> {
    let Some(p) = a.get("paths") else {
        return Ok(prior.map_or_else(Vec::new, |n| n.anchors.clone()));
    };
    let p = p.as_array().context("Note paths must be array")?;
    ensure!(p.len() <= 100, "Too many note anchors");
    let mut seen = BTreeSet::new();
    let mut result = Vec::new();
    for p in p {
        let p = p.as_str().context("Invalid note path")?;
        paths::normalized(p)?;
        ensure!(seen.insert(p), "Duplicate note anchor");
        let f = snap
            .files
            .iter()
            .find(|f| f.path == p)
            .context("Note anchor absent from snapshot")?;
        s.read_blob(&f.hash)?;
        result.push(Anchor {
            path: p.into(),
            content_hash: f.hash.clone(),
            snapshot_id: snap.id.clone(),
            source_workspace_id: snap.workspace_id.clone(),
        });
    }
    Ok(result)
}
#[allow(clippy::too_many_arguments)]
fn revision(
    s: &Store,
    snap: &Snapshot,
    scope: Scope,
    name: String,
    text: String,
    anchors: Vec<Anchor>,
    parent: Option<String>,
    deleted: bool,
) -> Result<Note> {
    let mut random = [0u8; 16];
    getrandom::fill(&mut random).map_err(|_| anyhow::anyhow!("Note nonce unavailable"))?;
    let mut n = Note {
        id: String::new(),
        name,
        scope,
        storage_workspace_id: s.workspace_id.clone(),
        source_workspace_id: snap.workspace_id.clone(),
        snapshot_id: snap.id.clone(),
        content: text,
        anchors,
        parent_revision: parent,
        created_ms: u64::try_from(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)?
                .as_millis(),
        )
        .context("Note timestamp overflow")?,
        nonce: hex::encode(random),
        deleted,
        trust: "authored_data_not_execution_authority".into(),
    };
    n.id = note_id(&n);
    Ok(n)
}
fn save(t: &Transaction<'_>, s: &Store, n: &Note) -> Result<()> {
    let head_id = key(s, n.scope, &n.name);
    let existing: bool = t.query_row(
        "SELECT EXISTS(SELECT 1 FROM records WHERE kind=?1 AND id=?2)",
        params![HEAD_KIND, head_id],
        |row| row.get(0),
    )?;
    if !existing {
        let count: usize = t.query_row(
            "SELECT count(*) FROM (SELECT 1 FROM records WHERE kind=?1 LIMIT 2000)",
            [HEAD_KIND],
            |row| row.get(0),
        )?;
        ensure!(count < 2000, "Named note capacity exceeded");
    }
    let raw = serde_json::to_string(n)?;
    ensure!(
        raw.len() <= MAX_REVISION_BYTES,
        "Serialized note revision capacity exceeded"
    );
    let previous: Option<String> = t
        .query_row(
            "SELECT value FROM records WHERE kind=?1 AND id=?2",
            params![REVISION_KIND, n.id],
            |r| r.get(0),
        )
        .optional()?;
    ensure!(
        previous.as_ref().is_none_or(|p| p == &raw),
        "Note revision collision"
    );
    t.execute(
        "INSERT OR IGNORE INTO records(kind,id,value) VALUES(?1,?2,?3)",
        params![REVISION_KIND, n.id, raw],
    )?;
    let h = Head {
        name: n.name.clone(),
        scope: n.scope,
        revision_id: n.id.clone(),
    };
    t.execute("INSERT INTO records(kind,id,value) VALUES(?1,?2,?3) ON CONFLICT(kind,id) DO UPDATE SET value=excluded.value",params![HEAD_KIND,key(s,n.scope,&n.name),serde_json::to_string(&h)?])?;
    Ok(())
}
fn append(output: &mut String, text: &str) -> Result<()> {
    ensure!(
        output
            .len()
            .checked_add(text.len())
            .is_some_and(|bytes| bytes <= MAX_CONTENT_BYTES),
        "Expanded note capacity exceeded"
    );
    output.push_str(text);
    Ok(())
}
fn address(scope: Scope, name: &str) -> String {
    if scope == Scope::Global {
        format!("mem:global/{name}")
    } else {
        format!("mem:{name}")
    }
}
fn reference_continuation(ch: char) -> bool {
    ch.is_alphanumeric()
        || matches!(
            ch,
            '_' | '-' | '+' | '.' | '/' | ':' | '%' | '@' | '=' | '&' | '!' | '~' | '$' | ';' | ','
        )
}
fn reference_ranges<'a>(
    text: &'a str,
    address: &'a str,
) -> impl Iterator<Item = (usize, usize)> + 'a {
    text.match_indices(address).filter_map(move |(start, _)| {
        let end = start + address.len();
        if text[..start]
            .chars()
            .next_back()
            .is_some_and(reference_continuation)
            || text[end..]
                .chars()
                .next()
                .is_some_and(reference_continuation)
        {
            None
        } else {
            Some((start, end))
        }
    })
}
fn reference_count(text: &str, old: &str, scope: Scope, deadline: &Deadline) -> Result<usize> {
    let old = address(scope, old);
    let mut count = 0;
    for _ in reference_ranges(text, &old) {
        deadline.check()?;
        count += 1;
    }
    Ok(count)
}
fn references(
    text: &str,
    old: &str,
    new: &str,
    scope: Scope,
    deadline: &Deadline,
) -> Result<(String, usize)> {
    let old = address(scope, old);
    let new = address(scope, new);
    let mut output = String::new();
    let mut position = 0;
    let mut count = 0;
    for (start, end) in reference_ranges(text, &old) {
        deadline.check()?;
        append(&mut output, &text[position..start])?;
        append(&mut output, &new)?;
        position = end;
        count += 1;
    }
    append(&mut output, &text[position..])?;
    content(&output)?;
    Ok((output, count))
}
/// Stores and policies are host-selected; request arguments never grant path access.
#[allow(clippy::too_many_arguments)]
pub fn invoke(
    source: &Store,
    store: &Store,
    snapshot: &Snapshot,
    scope: Scope,
    operation: &str,
    args: &Value,
    policy: &Policy,
    deadline: &Deadline,
) -> Result<Value> {
    invoke_with_patterns(
        source, store, snapshot, scope, operation, args, policy, None, deadline,
    )
}
#[allow(clippy::too_many_arguments)]
pub(crate) fn invoke_with_patterns(
    source: &Store,
    store: &Store,
    snapshot: &Snapshot,
    scope: Scope,
    operation: &str,
    args: &Value,
    policy: &Policy,
    patterns: Option<&MemoryPatterns>,
    deadline: &Deadline,
) -> Result<Value> {
    source.set_deadline(deadline)?;
    store.set_deadline(deadline)?;
    deadline.check()?;
    ensure!(
        snapshot.workspace_id == source.workspace_id
            && identity(&source.snapshot(&snapshot.id)?) == identity(snapshot),
        "Note source manifest mismatch"
    );
    ensure!(
        scope == Scope::Global || source.workspace_id == store.workspace_id,
        "Project notes require own workspace store"
    );
    let object = args.as_object().context("Note arguments must be object")?;
    ensure!(
        serde_json::to_vec(args)?.len() <= 128000,
        "Note input capacity exceeded"
    );
    let allowed: &[&str] = match operation {
        "list" => &["topic"],
        "read" => &["name"],
        "write" => &["name", "content", "paths", "expectedRevision"],
        "edit" => &[
            "name",
            "needle",
            "replacement",
            "regex",
            "allowMultiple",
            "expectedCount",
            "expectedRevision",
        ],
        "delete" => &["name", "expectedRevision"],
        "rename" => &["oldName", "newName", "expectedRevision"],
        _ => bail!("Unknown note operation"),
    };
    ensure!(
        object.keys().all(|k| allowed.contains(&k.as_str())),
        "Unknown note argument"
    );
    let rules = Rules::new(policy, scope, patterns)?;
    let tx = Transaction::new_unchecked(&store.db, TransactionBehavior::Immediate)?;
    let result = match operation {
        "list" => {
            let topic = args
                .get("topic")
                .map(|v| v.as_str().context("Invalid note topic"))
                .transpose()?
                .unwrap_or("");
            if !topic.is_empty() {
                paths::normalized(topic)?;
            }
            let notes = all(&tx, store, scope, deadline)?;
            let items:Vec<_>=notes.values().filter(|n|!rules.hidden(&n.name) && (topic.is_empty() || n.name==topic || n.name.strip_prefix(topic).is_some_and(|s|s.starts_with('/')))).map(|n|json!({"name":n.name,"scope":n.scope,"revisionId":n.id,"readOnly":rules.readonly(&n.name),"snapshotId":n.snapshot_id,"sourceWorkspaceId":n.source_workspace_id})).collect();
            json!({"items":items,"scope":scope})
        }
        "read" => {
            let n = name(args, "name", scope)?;
            rules.visible(&n)?;
            let note = current(&tx, store, scope, &n)?;
            let currentness:Vec<_>=note.anchors.iter().map(|anchor| {
                let state=if anchor.source_workspace_id!=snapshot.workspace_id {"foreign_workspace_unverified"}
                else {match snapshot.files.iter().find(|file|file.path==anchor.path) {Some(file) if file.hash==anchor.content_hash =>"current",Some(_)=>"changed",None=>"missing"}};
                json!({"path":anchor.path,"snapshotId":anchor.snapshot_id,"sourceWorkspaceId":anchor.source_workspace_id,"state":state})
            }).collect();
            json!({"note":note,"readOnly":rules.readonly(&n),"anchorCurrentness":currentness})
        }
        "write" => {
            let n = name(args, "name", scope)?;
            rules.editable(&n)?;
            let text = args["content"].as_str().context("Note content required")?;
            content(text)?;
            let prior = load(&tx, store, scope, &n)?;
            expected(args, prior.as_ref())?;
            let notes = all(&tx, store, scope, deadline)?;
            ensure!(
                notes.len() < 2000 || notes.contains_key(&n),
                "Named note capacity exceeded"
            );
            let bytes: usize = notes
                .values()
                .filter(|note| note.name != n)
                .map(|n| n.content.len())
                .sum();
            ensure!(
                bytes.saturating_add(text.len()) <= 32000000,
                "Note capacity exceeded"
            );
            let bindings = anchors(source, snapshot, args, prior.as_ref())?;
            let note = revision(
                store,
                snapshot,
                scope,
                n,
                text.into(),
                bindings,
                prior.map(|n| n.id),
                false,
            )?;
            save(&tx, store, &note)?;
            json!({"note":note,"sourceWritten":false})
        }
        "edit" => {
            let n = name(args, "name", scope)?;
            rules.editable(&n)?;
            let prior = current(&tx, store, scope, &n)?;
            expected(args, Some(&prior))?;
            let needle = args["needle"].as_str().context("Needle required")?;
            ensure!(
                !needle.is_empty() && needle.len() <= 16000,
                "Invalid note pattern"
            );
            let repl = args["replacement"]
                .as_str()
                .context("Replacement required")?;
            content(repl)?;
            for flag in ["regex", "allowMultiple"] {
                ensure!(
                    args.get(flag).is_none_or(Value::is_boolean),
                    "Invalid note edit flag"
                );
            }
            let expression = if args["regex"] == true {
                needle.to_owned()
            } else {
                regex::escape(needle)
            };
            let pattern = RegexBuilder::new(&expression)
                .size_limit(1000000)
                .dfa_size_limit(1000000)
                .build()?;
            let mut output = String::new();
            let mut cursor = 0;
            let mut count = 0u64;
            for captures in pattern.captures_iter(&prior.content) {
                deadline.check()?;
                let whole = captures.get(0).context("Missing capture")?;
                append(&mut output, &prior.content[cursor..whole.start()])?;
                if args["regex"] == true {
                    let expanded = crate::replacement::expand(
                        &captures,
                        repl,
                        MAX_CONTENT_BYTES
                            .checked_sub(output.len())
                            .context("Expanded note capacity exceeded")?,
                        deadline,
                    )?;
                    append(&mut output, &expanded)?;
                } else {
                    append(&mut output, repl)?;
                }
                cursor = whole.end();
                count += 1;
                ensure!(
                    output.len() <= MAX_CONTENT_BYTES,
                    "Expanded note capacity exceeded"
                );
            }
            ensure!(
                count > 0 && (args["allowMultiple"] == true || count == 1),
                "Note edit requires exactly one match unless allowMultiple enabled"
            );
            if let Some(expected) = args.get("expectedCount") {
                ensure!(
                    expected.as_u64() == Some(count),
                    "Note expected-count mismatch"
                );
            }
            append(&mut output, &prior.content[cursor..])?;
            content(&output)?;
            let notes = all(&tx, store, scope, deadline)?;
            let bytes: usize = notes
                .values()
                .filter(|note| note.name != n)
                .map(|n| n.content.len())
                .sum();
            ensure!(
                bytes.saturating_add(output.len()) <= 32000000,
                "Note capacity exceeded"
            );
            let note = revision(
                store,
                snapshot,
                scope,
                n,
                output,
                prior.anchors,
                Some(prior.id),
                false,
            )?;
            save(&tx, store, &note)?;
            json!({"note":note,"matchCount":count,"sourceWritten":false})
        }
        "delete" => {
            let n = name(args, "name", scope)?;
            rules.editable(&n)?;
            let prior = current(&tx, store, scope, &n)?;
            expected(args, Some(&prior))?;
            let note = revision(
                store,
                snapshot,
                scope,
                n,
                prior.content,
                prior.anchors,
                Some(prior.id),
                true,
            )?;
            save(&tx, store, &note)?;
            json!({"deleted":true,"revisionId":note.id,"sourceWritten":false})
        }
        "rename" => {
            let old = name(args, "oldName", scope)?;
            let new = name(args, "newName", scope)?;
            ensure!(old != new, "Rename needs a different name");
            rules.editable(&old)?;
            rules.editable(&new)?;
            let notes = all(&tx, store, scope, deadline)?;
            let prior = notes.get(&old).context("Named note unavailable")?;
            expected(args, Some(prior))?;
            ensure!(!notes.contains_key(&new), "Rename destination exists");
            let mut renamed = prior.clone();
            let mut updates = Vec::new();
            let mut unchanged = Vec::new();
            let mut count = 0;
            let mut bytes = 0usize;
            for n in notes.values() {
                deadline.check()?;
                if rules.hidden(&n.name) {
                    bytes = bytes.saturating_add(n.content.len());
                    continue;
                }
                if rules.readonly(&n.name) {
                    let hits = reference_count(&n.content, &old, scope, deadline)?;
                    if hits > 0 {
                        unchanged.push(n.name.clone());
                    }
                    bytes = bytes.saturating_add(n.content.len());
                    continue;
                }
                let (text, hits) = references(&n.content, &old, &new, scope, deadline)?;
                if hits == 0 {
                    bytes = bytes.saturating_add(n.content.len());
                    continue;
                }
                count += hits;
                bytes = bytes.saturating_add(text.len());
                if n.name == old {
                    renamed.content = text;
                } else {
                    updates.push(revision(
                        store,
                        snapshot,
                        scope,
                        n.name.clone(),
                        text,
                        n.anchors.clone(),
                        Some(n.id.clone()),
                        false,
                    )?);
                }
            }
            ensure!(bytes <= 32000000, "Renamed notes capacity exceeded");
            let removed = revision(
                store,
                snapshot,
                scope,
                old,
                prior.content.clone(),
                prior.anchors.clone(),
                Some(prior.id.clone()),
                true,
            )?;
            let renamed = revision(
                store,
                snapshot,
                scope,
                new,
                renamed.content,
                renamed.anchors,
                Some(prior.id.clone()),
                false,
            )?;
            save(&tx, store, &removed)?;
            save(&tx, store, &renamed)?;
            for n in updates {
                deadline.check()?;
                save(&tx, store, &n)?;
            }
            json!({"note":renamed,"referencesUpdated":count,"readOnlyReferencesUnchanged":unchanged,"sourceWritten":false})
        }
        _ => unreachable!(),
    };
    deadline.check()?;
    tx.commit()?;
    Ok(result)
}
