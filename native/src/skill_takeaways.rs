//! Authored application skills; never execution authority or AgentMemory state.
use crate::{
    capture::Deadline,
    model::{hash, identity, valid_id},
    store::Store,
};
use anyhow::{Context, Result, bail, ensure};
use rusqlite::OptionalExtension;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
};
use unicode_normalization::UnicodeNormalization;

pub const REVISION_KIND: &str = "native_skill_revision_v1";
pub const HEAD_KIND: &str = "native_skill_head_v1";
pub const OPERATION_KIND: &str = "native_skill_operation_v1";
pub const RECEIPT_KIND: &str = "native_skill_operation_receipt_v1";
const MAX_STATE: usize = 32_000_000;
pub struct SkillAccess<'a> {
    store: &'a Store,
    project: String,
    repository: PathBuf,
    personal: PathBuf,
    personal_writes: bool,
    repository_writes: bool,
}
fn directory(path: &Path) -> Result<()> {
    ensure!(path.is_absolute(), "Explicit absolute skill root required");
    for component in path.ancestors() {
        let metadata = fs::symlink_metadata(component)?;
        ensure!(
            metadata.is_dir() && !crate::paths::is_link(&metadata),
            "Skill root link refused"
        );
    }
    Ok(())
}
impl<'a> SkillAccess<'a> {
    pub fn new(
        store: &'a Store,
        project_id: &str,
        repository_root: &Path,
        personal_writes: bool,
        repository_writes: bool,
    ) -> Result<Self> {
        ensure!(
            valid_id(project_id),
            "Explicit stable skill project identity required"
        );
        directory(repository_root)?;
        directory(&store.dir)?;
        Ok(Self {
            store,
            project: project_id.into(),
            repository: repository_root.canonicalize()?,
            personal: store.dir.join("skill-library").join(project_id),
            personal_writes,
            repository_writes,
        })
    }
    fn key(&self, name: &str) -> String {
        identity(&(&self.project, name))
    }
    fn file(&self, location: &str, relative: &str) -> Result<PathBuf> {
        crate::paths::normalized_syntax(relative)?;
        let base = match location {
            "personal" => self.personal.clone(),
            "repo" => self.repository.join(".claude/skills"),
            _ => bail!("Unknown skill file authority"),
        };
        let components: Vec<_> = relative.split('/').collect();
        ensure!(
            components.len() == 2 && components[1] == "SKILL.md"
                || components.len() == 3
                    && components[1] == "takeaways"
                    && components[2].ends_with(".md"),
            "Invalid skill presentation path"
        );
        ensure!(
            name(components[0])? == components[0],
            "Invalid skill directory name"
        );
        let path = base.join(relative);
        for ancestor in path.ancestors() {
            match fs::symlink_metadata(ancestor) {
                Ok(metadata) => {
                    ensure!(!crate::paths::is_link(&metadata), "Skill file link refused")
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => (),
                Err(error) => return Err(error.into()),
            }
        }
        Ok(path)
    }
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Takeaway {
    id: String,
    file: String,
    taught_by: String,
    date: String,
    from: String,
    confirmed_by: Vec<String>,
    folded_in: Option<u64>,
    section: Option<String>,
    text: String,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct State {
    name: String,
    description: String,
    version: u64,
    markdown: String,
    location: String,
    takeaways: Vec<Takeaway>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Revision {
    schema_version: u8,
    project_id: String,
    name: String,
    parent_revision: Option<String>,
    state: State,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Head {
    schema_version: u8,
    project_id: String,
    name: String,
    revision: String,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct FileChange {
    location: String,
    relative: String,
    before: Option<String>,
    after: Option<String>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Operation {
    schema_version: u8,
    project_id: String,
    repository_hash: String,
    name: String,
    before: Option<String>,
    after: String,
    files: Vec<FileChange>,
}
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Receipt {
    schema_version: u8,
    operation_id: String,
    status: String,
}
struct Observed {
    state: State,
    head: Option<String>,
    token: String,
    content_hash: String,
}

fn name(raw: &str) -> Result<String> {
    ensure!(raw.len() <= 4096, "Skill name capacity exceeded");
    let normalized: String = raw
        .to_lowercase()
        .nfkd()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c.is_whitespace() || c == '-' {
                c
            } else {
                ' '
            }
        })
        .collect();
    let value = normalized
        .split(|c: char| c.is_whitespace() || c == '-')
        .filter(|s| !s.is_empty())
        .take(8)
        .collect::<Vec<_>>()
        .join("-");
    ensure!(
        !value.is_empty() && value.len() <= 200 && value != "trail" && value != "graft",
        "Invalid or reserved skill name"
    );
    Ok(value)
}
fn text(value: &str, maximum: usize) -> Result<()> {
    ensure!(
        !value.trim().is_empty()
            && value.len() <= maximum
            && !crate::paths::obvious_secret(value)
            && !value
                .chars()
                .any(|c| c.is_control() && !matches!(c, '\n' | '\r' | '\t')),
        "Invalid skill text or secret content"
    );
    Ok(())
}
fn serialized<T: Serialize>(value: &T, max: usize) -> Result<Vec<u8>> {
    struct Count(usize, usize);
    impl Write for Count {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            if bytes.len() > self.1.saturating_sub(self.0) {
                return Err(std::io::Error::other(
                    "Skill serialization capacity exceeded",
                ));
            }
            self.0 += bytes.len();
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut count = Count(0, max);
    serde_json::to_writer(&mut count, value)?;
    let mut bytes = Vec::with_capacity(count.0);
    serde_json::to_writer(&mut bytes, value)?;
    Ok(bytes)
}
fn record<T: serde::de::DeserializeOwned>(
    store: &Store,
    kind: &str,
    id: &str,
    d: &Deadline,
) -> Result<Option<T>> {
    store.set_deadline(d)?;
    let limit = rusqlite::limits::Limit::SQLITE_LIMIT_LENGTH;
    let prior = store
        .db
        .set_limit(limit, store.db.limit(limit)?.min((MAX_STATE + 4096) as i32))?;
    let value: rusqlite::Result<Option<String>> = store
        .db
        .query_row(
            "SELECT value FROM records WHERE kind=?1 AND id=?2",
            rusqlite::params![kind, id],
            |r| r.get(0),
        )
        .optional();
    store.db.set_limit(limit, prior)?;
    value?
        .map(|value| crate::json::decode(crate::json::parse(value.as_bytes(), MAX_STATE)?))
        .transpose()
}
fn immutable(
    store: &Store,
    kind: &str,
    id: &str,
    value: &impl Serialize,
    d: &Deadline,
) -> Result<()> {
    let bytes = serialized(value, MAX_STATE)?;
    let raw = std::str::from_utf8(&bytes)?;
    d.check()?;
    store.db.execute(
        "INSERT INTO records(kind,id,value) VALUES(?1,?2,?3) ON CONFLICT(kind,id) DO NOTHING",
        rusqlite::params![kind, id, raw],
    )?;
    let actual: Value = record(store, kind, id, d)?.context("Skill immutable record missing")?;
    ensure!(
        serialized(&actual, MAX_STATE)? == serialized(&serde_json::to_value(value)?, MAX_STATE)?,
        "Immutable skill identity collision"
    );
    Ok(())
}
fn read_file(path: &Path, d: &Deadline) -> Result<Option<Vec<u8>>> {
    d.check()?;
    let metadata = match fs::symlink_metadata(path) {
        Ok(value) => value,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    ensure!(
        metadata.is_file() && !crate::paths::is_link(&metadata) && metadata.len() <= 1_000_000,
        "Skill file type/size refused"
    );
    let mut bytes = Vec::new();
    let mut file = fs::File::open(path)?;
    let mut chunk = [0; 65536];
    loop {
        d.check()?;
        let n = file.read(&mut chunk)?;
        if n == 0 {
            break;
        }
        ensure!(
            bytes.len() + n <= 1_000_000,
            "Skill file grew beyond capacity"
        );
        bytes.extend_from_slice(&chunk[..n]);
    }
    ensure!(
        bytes.len() as u64 == metadata.len(),
        "Skill file changed during read"
    );
    text(std::str::from_utf8(&bytes)?, 1_000_000)?;
    Ok(Some(bytes))
}
fn parse_markdown(markdown: &str, d: &Deadline) -> Result<(String, u64, String)> {
    text(markdown, 1_000_000)?;
    let lines: Vec<_> = markdown.lines().collect();
    if lines.first() != Some(&"---") {
        return Ok((String::new(), 1, markdown.trim().into()));
    }
    let close = lines
        .iter()
        .skip(1)
        .position(|line| *line == "---")
        .context("Unclosed skill frontmatter")?
        + 1;
    let yaml = lines[1..close].join("\n");
    ensure!(
        lines[1..close]
            .iter()
            .filter(|line| line.starts_with("version:"))
            .count()
            <= 1,
        "Duplicate skill version field"
    );
    let values = crate::config_graph::bounded_yaml(&yaml, d)?;
    ensure!(
        values.len() == 1 && values[0].as_hash().is_some(),
        "Skill frontmatter must be one mapping"
    );
    let description = values[0]["description"]
        .as_str()
        .unwrap_or("")
        .trim()
        .to_string();
    let version = match &values[0]["version"] {
        yaml_rust2::Yaml::BadValue => 1,
        value => u64::try_from(value.as_i64().context("Skill version must be an integer")?)?,
    };
    ensure!(
        (1..=1_000_000).contains(&version),
        "Skill version outside capacity"
    );
    Ok((
        description,
        version,
        lines[close + 1..].join("\n").trim().into(),
    ))
}
fn observed(a: &SkillAccess<'_>, n: &str, d: &Deadline) -> Result<Option<Observed>> {
    let head: Option<Head> = record(a.store, HEAD_KIND, &a.key(n), d)?;
    let mut state = if let Some(head) = &head {
        ensure!(
            head.schema_version == 1
                && head.project_id == a.project
                && head.name == n
                && valid_id(&head.revision),
            "Skill head binding mismatch"
        );
        let revision = revision(a.store, &head.revision, d)?;
        ensure!(
            hash(serialized(&revision, MAX_STATE)?) == head.revision
                && revision.schema_version == 1
                && revision.project_id == a.project
                && revision.name == n
                && revision.state.name == n,
            "Skill revision identity mismatch"
        );
        revision.state
    } else {
        State {
            name: n.into(),
            description: String::new(),
            version: 1,
            markdown: String::new(),
            location: "personal".into(),
            takeaways: vec![],
        }
    };
    let repo = read_file(&a.file("repo", &format!("{n}/SKILL.md"))?, d)?;
    let (location, bytes) = if let Some(repo) = repo {
        ("repo", Some(repo))
    } else {
        (
            "personal",
            read_file(&a.file("personal", &format!("{n}/SKILL.md"))?, d)?,
        )
    };
    let Some(bytes) = bytes else {
        ensure!(head.is_none(), "Registered skill presentation missing");
        return Ok(None);
    };
    let markdown = String::from_utf8(bytes)?;
    let (description, version, _) = parse_markdown(&markdown, d)?;
    state.location = location.into();
    state.description = description;
    state.version = version;
    state.markdown = markdown;
    ensure!(
        state.takeaways.len() <= 100_000,
        "Skill takeaway capacity exceeded"
    );
    let content_hash = hash(state.markdown.as_bytes());
    let head = head.map(|head| head.revision);
    let token = identity(&(&a.project, n, &head, &content_hash));
    Ok(Some(Observed {
        state,
        head,
        token,
        content_hash,
    }))
}
fn public(a: &SkillAccess<'_>, o: &Observed, d: &Deadline) -> Result<Value> {
    let (_, _, body) = parse_markdown(&o.state.markdown, d)?;
    let takeaways: Vec<_> = o
        .state
        .takeaways
        .iter()
        .map(|t| takeaway_public(a, &o.state.name, t))
        .collect::<Result<_>>()?;
    Ok(
        json!({"name":o.state.name,"description":o.state.description,"version":o.state.version,"body":body,"where":if o.state.location=="personal"{"home"}else{"repo"},"file":a.file(&o.state.location,&format!("{}/SKILL.md",o.state.name))?,"takeaways":takeaways,"revisionId":o.token,"contentHash":o.content_hash,"identityTrust":"declared_authors_not_verified_human_identity","executionAuthority":false}),
    )
}
fn takeaway_public(a: &SkillAccess<'_>, n: &str, t: &Takeaway) -> Result<Value> {
    let mut value = serde_json::to_value(t)?;
    value["skill"] = json!(n);
    value["path"] = json!(a.file("personal", &format!("{n}/takeaways/{}", t.file))?);
    Ok(value)
}
fn result(a: &SkillAccess<'_>, n: &str, d: &Deadline) -> Result<Value> {
    let o = observed(a, n, d)?.context("Skill absent")?;
    Ok(json!({"skill":public(a,&o,d)?,"revisionId":o.token,"contentHash":o.content_hash}))
}
fn names(a: &SkillAccess<'_>, d: &Deadline) -> Result<Vec<String>> {
    let mut names = BTreeSet::new();
    for base in [a.repository.join(".claude/skills"), a.personal.clone()] {
        if !base.exists() {
            continue;
        }
        directory(&base)?;
        for entry in fs::read_dir(base)? {
            d.check()?;
            let entry = entry?;
            let metadata = fs::symlink_metadata(entry.path())?;
            ensure!(
                !crate::paths::is_link(&metadata),
                "Skill directory link refused"
            );
            if !metadata.is_dir() || !entry.path().join("SKILL.md").exists() {
                continue;
            }
            ensure!(names.len() < 10000, "Skill count capacity exceeded");
            let raw = entry
                .file_name()
                .into_string()
                .map_err(|_| anyhow::anyhow!("NonUTF8 skill name"))?;
            if matches!(raw.as_str(), "trail" | "graft") {
                continue;
            }
            let n = name(&raw)?;
            ensure!(n == raw, "Noncanonical or ambiguous skill directory name");
            if entry.path().join("SKILL.md").exists() {
                names.insert(n);
            }
        }
    }
    Ok(names.into_iter().collect())
}
fn fields(args: &Value, allowed: &[&str]) -> Result<()> {
    ensure!(
        args.as_object()
            .is_some_and(|a| a.keys().all(|key| allowed.contains(&key.as_str()))),
        "Unknown skill argument"
    );
    Ok(())
}
fn required<'a>(args: &'a Value, key: &str, max: usize) -> Result<&'a str> {
    let value = args[key]
        .as_str()
        .with_context(|| format!("Skill {key} required"))?;
    text(value, max)?;
    Ok(value)
}
fn expected(args: &Value, o: Option<&Observed>) -> Result<()> {
    ensure!(
        args.get("expectedRevision").is_some(),
        "Expected skill revision required"
    );
    ensure!(
        match o {
            Some(o) => args["expectedRevision"].as_str() == Some(&o.token),
            None => args["expectedRevision"].is_null(),
        },
        "Skill revision conflict"
    );
    Ok(())
}
fn takeaway_markdown(n: &str, t: &Takeaway) -> String {
    format!(
        "---\nskill: {n}\nnative_id: {}\ntaught_by: {}\ndate: {}\nfrom: correction\nconfirmed_by: {}\nfolded_in: {}\nsection: {}\n---\n{}\n",
        t.id,
        json!(t.taught_by),
        t.date,
        json!(t.confirmed_by),
        json!(t.folded_in),
        json!(t.section),
        t.text.trim()
    )
}
fn stage_change(
    a: &SkillAccess<'_>,
    location: &str,
    relative: String,
    expected: Option<&str>,
    after: Option<&[u8]>,
    d: &Deadline,
) -> Result<FileChange> {
    let bytes = read_file(&a.file(location, &relative)?, d)?;
    ensure!(
        bytes.as_ref().map(hash).as_deref() == expected,
        "Skill source changed after observation"
    );
    let before = bytes.map(|bytes| a.store.artifact(&bytes)).transpose()?;
    let after = after.map(|bytes| a.store.artifact(bytes)).transpose()?;
    Ok(FileChange {
        location: location.into(),
        relative,
        before,
        after,
    })
}
fn head_id(a: &SkillAccess<'_>, n: &str, d: &Deadline) -> Result<Option<String>> {
    let h: Option<Head> = record(a.store, HEAD_KIND, &a.key(n), d)?;
    Ok(h.map(|h| h.revision))
}
fn revision(store: &Store, id: &str, d: &Deadline) -> Result<Revision> {
    ensure!(valid_id(id), "Invalid skill revision identity");
    let r: Revision = record(store, REVISION_KIND, id, d)?.context("Skill revision absent")?;
    ensure!(
        r.schema_version == 1
            && valid_id(&r.project_id)
            && name(&r.name)? == r.name
            && r.state.name == r.name
            && hash(serialized(&r, MAX_STATE)?) == id,
        "Skill revision binding mismatch"
    );
    ensure!(
        r.parent_revision
            .as_ref()
            .is_none_or(|p| valid_id(p) && p != id),
        "Invalid skill revision parent"
    );
    let (_, version, _) = parse_markdown(&r.state.markdown, d)?;
    ensure!(
        version == r.state.version
            && ["personal", "repo"].contains(&r.state.location.as_str())
            && r.state.takeaways.len() <= 100_000,
        "Invalid skill revision state"
    );
    let mut ids = BTreeSet::new();
    let mut files = BTreeSet::new();
    for t in &r.state.takeaways {
        d.check()?;
        ensure!(
            valid_id(&t.id)
                && ids.insert(&t.id)
                && files.insert(&t.file)
                && !t.file.contains(['/', '\\'])
                && !t.file.contains("..")
                && t.file.ends_with(".md")
                && t.from == "correction",
            "Invalid skill takeaway identity"
        );
        text(&t.text, 64000)?;
        text(&t.taught_by, 256)?;
        validate_date(&t.date)?;
        text(&takeaway_markdown(&r.name, t), 1_000_000)?;
        ensure!(
            t.folded_in
                .is_none_or(|v| (1..=r.state.version).contains(&v)),
            "Invalid folded version"
        );
        let mut confirmed = BTreeSet::new();
        for who in &t.confirmed_by {
            text(who, 256)?;
            ensure!(
                who.to_lowercase() != t.taught_by.to_lowercase()
                    && confirmed.insert(who.to_lowercase()),
                "Invalid skill confirmation"
            );
        }
        if let Some(section) = &t.section {
            text(section, 256)?;
            ensure!(!section.contains(['\n', '\r']), "Invalid skill section");
        }
    }
    if let Some(parent) = &r.parent_revision {
        let prior: Revision =
            record(store, REVISION_KIND, parent, d)?.context("Skill parent revision absent")?;
        ensure!(
            hash(serialized(&prior, MAX_STATE)?) == *parent
                && prior.schema_version == 1
                && prior.project_id == r.project_id
                && prior.name == r.name,
            "Skill parent lineage mismatch"
        );
        let next_by_id: BTreeMap<_, _> = r
            .state
            .takeaways
            .iter()
            .map(|t| (t.id.as_str(), t))
            .collect();
        for old in &prior.state.takeaways {
            d.check()?;
            let next = next_by_id
                .get(old.id.as_str())
                .context("Skill history was removed")?;
            ensure!(
                old.file == next.file
                    && old.taught_by == next.taught_by
                    && old.date == next.date
                    && old.from == next.from
                    && old.text == next.text
                    && old.section == next.section
                    && old
                        .confirmed_by
                        .iter()
                        .all(|who| next.confirmed_by.contains(who))
                    && (old.folded_in.is_none() || old.folded_in == next.folded_in),
                "Immutable skill takeaway history changed"
            );
        }
    }
    Ok(r)
}
fn projections(state: &State) -> Vec<(String, String, String)> {
    let mut files = vec![(
        state.location.clone(),
        format!("{}/SKILL.md", state.name),
        hash(state.markdown.as_bytes()),
    )];
    files.extend(state.takeaways.iter().map(|t| {
        (
            "personal".into(),
            format!("{}/takeaways/{}", state.name, t.file),
            hash(takeaway_markdown(&state.name, t).as_bytes()),
        )
    }));
    files
}
fn validate_operation(store: &Store, id: &str, op: &Operation, d: &Deadline) -> Result<Revision> {
    ensure!(
        valid_id(id)
            && hash(serialized(op, MAX_STATE)?) == id
            && op.schema_version == 1
            && valid_id(&op.project_id)
            && valid_id(&op.repository_hash)
            && name(&op.name)? == op.name
            && op.files.len() <= 100_001,
        "Skill operation identity mismatch"
    );
    let after = revision(store, &op.after, d)?;
    ensure!(
        after.project_id == op.project_id
            && after.name == op.name
            && after.parent_revision == op.before,
        "Skill operation revision mismatch"
    );
    let prior = op
        .before
        .as_ref()
        .map(|id| revision(store, id, d))
        .transpose()?;
    let expected = projections(&after.state);
    let expected_by_path: BTreeMap<_, _> = expected
        .iter()
        .map(|(l, p, h)| ((l.as_str(), p.as_str()), h.as_str()))
        .collect();
    let previous = prior
        .as_ref()
        .map(|p| projections(&p.state))
        .unwrap_or_default();
    let previous_by_path: BTreeMap<_, _> = previous
        .iter()
        .map(|(l, p, h)| ((l.as_str(), p.as_str()), h.as_str()))
        .collect();
    let mut paths = BTreeSet::new();
    for file in &op.files {
        d.check()?;
        ensure!(
            paths.insert((&file.location, &file.relative)) && file.before != file.after,
            "Duplicate or empty skill file change"
        );
        if let Some(value) = &file.after {
            ensure!(
                expected_by_path
                    .get(&(file.location.as_str(), file.relative.as_str()))
                    .copied()
                    == Some(value.as_str()),
                "Skill operation file differs from bound revision projection"
            );
        } else {
            ensure!(
                file.location == "personal"
                    && file.relative == format!("{}/SKILL.md", op.name)
                    && after.state.location == "repo"
                    && prior
                        .as_ref()
                        .is_some_and(|p| p.state.location == "personal"),
                "Invalid skill source removal"
            );
        }
        for value in [&file.before, &file.after].into_iter().flatten() {
            ensure!(valid_id(value), "Invalid skill file artifact identity");
            let bytes = store.read_artifact_limited(value, 1_000_000)?;
            text(std::str::from_utf8(&bytes)?, 1_000_000)?;
        }
        if file.relative.contains("/takeaways/") {
            let old = previous_by_path
                .get(&(file.location.as_str(), file.relative.as_str()))
                .copied();
            ensure!(
                file.before.as_deref() == old,
                "Skill takeaway previous projection differs"
            );
        }
    }
    Ok(after)
}
fn apply(a: &SkillAccess<'_>, id: &str, op: &Operation, d: &Deadline) -> Result<()> {
    let next = validate_operation(a.store, id, op, d)?;
    ensure!(
        op.schema_version == 1
            && op.project_id == a.project
            && op.repository_hash == identity(&a.repository)
            && valid_id(&op.after),
        "Skill operation binding mismatch"
    );
    ensure!(
        a.personal_writes
            && op
                .files
                .iter()
                .all(|f| f.location != "repo" || a.repository_writes),
        "Skill operation write authority unavailable"
    );
    ensure!(
        head_id(a, &op.name, d)? == op.before,
        "Skill operation head conflict"
    );
    for (location, path, expected) in projections(&next.state) {
        if !op
            .files
            .iter()
            .any(|f| f.location == location && f.relative == path)
        {
            ensure!(
                read_file(&a.file(&location, &path)?, d)?.map(hash).as_ref() == Some(&expected),
                "Unchanged skill projection is missing or differs"
            );
        }
    }
    // All target bytes must still be either the exact previous or already applied version.
    for f in &op.files {
        let current = read_file(&a.file(&f.location, &f.relative)?, d)?.map(hash);
        ensure!(
            current == f.before || current == f.after,
            "Skill operation target changed; recovery refused"
        );
    }
    for f in &op.files {
        d.check()?;
        let path = a.file(&f.location, &f.relative)?;
        let current = read_file(&path, d)?.map(hash);
        if current == f.after {
            continue;
        }
        if let Some(after) = &f.after {
            let bytes = a.store.read_artifact_limited(after, 1_000_000)?;
            let parent = path.parent().context("Skill target parent absent")?;
            fs::create_dir_all(parent)?;
            let _ = a.file(&f.location, &f.relative)?;
            let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
            temporary.write_all(&bytes)?;
            temporary.as_file().sync_all()?;
            ensure!(
                read_file(&path, d)?.map(hash) == f.before,
                "Skill target changed before publication"
            );
            temporary.persist(&path).map_err(|e| e.error)?;
        } else {
            ensure!(current == f.before, "Skill removal conflict");
            fs::remove_file(path)?;
        }
    }
    let tx = rusqlite::Transaction::new_unchecked(
        &a.store.db,
        rusqlite::TransactionBehavior::Immediate,
    )?;
    ensure!(
        head_id(a, &op.name, d)? == op.before,
        "Skill head changed during publication"
    );
    a.store.record(
        HEAD_KIND,
        &a.key(&op.name),
        &Head {
            schema_version: 1,
            project_id: a.project.clone(),
            name: op.name.clone(),
            revision: op.after.clone(),
        },
    )?;
    a.store.record(
        RECEIPT_KIND,
        id,
        &Receipt {
            schema_version: 1,
            operation_id: id.into(),
            status: "applied".into(),
        },
    )?;
    d.check()?;
    tx.commit()?;
    Ok(())
}
fn pending(a: &SkillAccess<'_>, d: &Deadline) -> Result<Vec<(String, Operation)>> {
    let count: i64 = a.store.db.query_row(
        "SELECT count(*) FROM records WHERE kind=?1",
        [RECEIPT_KIND],
        |r| r.get(0),
    )?;
    ensure!(
        count <= 10000,
        "Skill receipt history capacity exceeded; incomplete recovery scans are refused"
    );
    let mut stmt = a
        .store
        .db
        .prepare("SELECT id FROM records WHERE kind=?1 ORDER BY id LIMIT 10001")?;
    let rows = stmt.query_map([RECEIPT_KIND], |r| r.get::<_, String>(0))?;
    let mut output = vec![];
    for id in rows {
        d.check()?;
        let id = id?;
        let receipt: Receipt =
            record(a.store, RECEIPT_KIND, &id, d)?.context("Skill receipt absent")?;
        ensure!(
            receipt.schema_version == 1
                && receipt.operation_id == id
                && ["prepared", "applied"].contains(&receipt.status.as_str()),
            "Invalid skill receipt"
        );
        if receipt.status == "prepared" {
            let op: Operation =
                record(a.store, OPERATION_KIND, &id, d)?.context("Skill operation absent")?;
            if op.project_id == a.project {
                ensure!(
                    hash(serialized(&op, MAX_STATE)?) == id,
                    "Skill operation identity mismatch"
                );
                output.push((id, op));
            }
        }
        ensure!(output.len() <= 10000, "Skill recovery capacity exceeded");
    }
    Ok(output)
}
fn save(
    a: &SkillAccess<'_>,
    before: Option<&Observed>,
    state: State,
    extra: Vec<FileChange>,
    d: &Deadline,
) -> Result<()> {
    // Every presentation must pass its read contract before creating a journal.
    let count: i64 = a.store.db.query_row(
        "SELECT count(*) FROM records WHERE kind=?1",
        [RECEIPT_KIND],
        |r| r.get(0),
    )?;
    ensure!(count < 10000, "Skill receipt history capacity exceeded");
    parse_markdown(&state.markdown, d)?;
    for takeaway in &state.takeaways {
        d.check()?;
        text(&takeaway_markdown(&state.name, takeaway), 1_000_000)?;
    }
    let revision = Revision {
        schema_version: 1,
        project_id: a.project.clone(),
        name: state.name.clone(),
        parent_revision: before.and_then(|o| o.head.clone()),
        state,
    };
    let bytes = serialized(&revision, MAX_STATE)?;
    let revision_id = hash(&bytes);
    let mut files = extra;
    let n = &revision.name;
    files.push(stage_change(
        a,
        &revision.state.location,
        format!("{n}/SKILL.md"),
        before
            .filter(|o| o.state.location == revision.state.location)
            .map(|o| o.content_hash.as_str()),
        Some(revision.state.markdown.as_bytes()),
        d,
    )?);
    for t in &revision.state.takeaways {
        d.check()?;
        let expected = before
            .and_then(|o| o.state.takeaways.iter().find(|old| old.file == t.file))
            .map(|old| hash(takeaway_markdown(n, old).as_bytes()));
        files.push(stage_change(
            a,
            "personal",
            format!("{n}/takeaways/{}", t.file),
            expected.as_deref(),
            Some(takeaway_markdown(n, t).as_bytes()),
            d,
        )?);
    }
    files.retain(|f| f.before != f.after);
    let op = Operation {
        schema_version: 1,
        project_id: a.project.clone(),
        repository_hash: identity(&a.repository),
        name: n.clone(),
        before: before.and_then(|o| o.head.clone()),
        after: revision_id.clone(),
        files,
    };
    let id = hash(serialized(&op, MAX_STATE)?);
    immutable(a.store, REVISION_KIND, &revision_id, &revision, d)?;
    immutable(a.store, OPERATION_KIND, &id, &op, d)?;
    a.store.record(
        RECEIPT_KIND,
        &id,
        &Receipt {
            schema_version: 1,
            operation_id: id.clone(),
            status: "prepared".into(),
        },
    )?;
    apply(a, &id, &op, d)
}
pub fn invoke(a: &SkillAccess<'_>, operation: &str, args: &Value, d: &Deadline) -> Result<Value> {
    d.check()?;
    validate_invocation(operation, args)?;
    a.store.set_deadline(d)?;
    ensure!(
        [
            "list", "show", "learn", "confirm", "fold", "publish", "recover"
        ]
        .contains(&operation),
        "Unknown skill operation"
    );
    if ["list", "show"].contains(&operation) {
        ensure!(
            pending(a, d)?.is_empty(),
            "Pending skill publication requires explicit recovery"
        );
        return query(a, operation, args, d);
    }
    ensure!(
        a.personal_writes,
        "Host personal skill write authority required"
    );
    a.store.with_exclusive_client(d, || {
        for (id, op) in pending(a, d)? {
            apply(a, &id, &op, d)?;
        }
        if operation == "recover" {
            fields(args, &[])?;
            return Ok(json!({"recovered":true}));
        }
        mutate(a, operation, args, d, false)
    })
}
pub fn prepare_arguments(
    a: &SkillAccess<'_>,
    operation: &str,
    args: &Value,
    d: &Deadline,
) -> Result<Value> {
    d.check()?;
    a.store.set_deadline(d)?;
    serialized(args, MAX_STATE)?;
    let mut prepared = args.clone();
    ensure!(prepared.is_object(), "Skill arguments must be an object");
    let all = operation == "fold" && args.get("skill").is_none();
    if all {
        for key in ["expectedRevisions", "expectedFileHashes"] {
            if prepared.get(key).is_none() {
                let other = if key == "expectedRevisions" {
                    "expectedFileHashes"
                } else {
                    "expectedRevisions"
                };
                prepared[key] = args.get(other).cloned().unwrap_or_else(|| json!({}));
            }
        }
    } else if matches!(operation, "learn" | "confirm" | "fold" | "publish") {
        if prepared.get("expectedRevision").is_none() {
            prepared["expectedRevision"] = Value::Null;
        }
        if matches!(operation, "fold" | "publish") && prepared.get("expectedFileHash").is_none() {
            prepared["expectedFileHash"] = Value::Null;
        }
    }
    validate_invocation(operation, &prepared)?;
    if matches!(operation, "list" | "show" | "recover") {
        return Ok(prepared);
    }
    ensure!(
        pending(a, d)?.is_empty(),
        "Pending skill publication requires recovery before host argument preflight"
    );
    if all {
        let mut revisions = serde_json::Map::new();
        let mut hashes = serde_json::Map::new();
        for n in names(a, d)? {
            let o = observed(a, &n, d)?.context("Skill disappeared during host preflight")?;
            revisions.insert(n.clone(), json!(o.token));
            hashes.insert(n, json!(o.content_hash));
        }
        if args.get("expectedRevisions").is_none() {
            prepared["expectedRevisions"] = json!(revisions);
        }
        if args.get("expectedFileHashes").is_none() {
            prepared["expectedFileHashes"] = json!(hashes);
        }
    } else {
        let n = name(required(args, "skill", 4096)?)?;
        let o = observed(a, &n, d)?;
        if args.get("expectedRevision").is_none() {
            prepared["expectedRevision"] = o.as_ref().map_or(Value::Null, |o| json!(o.token));
        }
        if matches!(operation, "fold" | "publish") && args.get("expectedFileHash").is_none() {
            prepared["expectedFileHash"] =
                o.as_ref().map_or(Value::Null, |o| json!(o.content_hash));
        }
    }
    validate_invocation(operation, &prepared)?;
    Ok(prepared)
}
fn validate_invocation(operation: &str, args: &Value) -> Result<()> {
    if operation == "fold" && args.get("skill").is_none() {
        fields(
            args,
            &["confirmed", "expectedRevisions", "expectedFileHashes"],
        )?;
        ensure!(
            args.get("confirmed").is_none_or(Value::is_boolean),
            "Confirmed must be boolean"
        );
        let revisions = args["expectedRevisions"]
            .as_object()
            .context("Expected skill revision map required")?;
        let hashes = args["expectedFileHashes"]
            .as_object()
            .context("Expected skill file hash map required")?;
        ensure!(
            revisions.len() <= 10000 && revisions.keys().eq(hashes.keys()),
            "Skill selection maps differ or exceed capacity"
        );
        for (n, revision) in revisions {
            ensure!(
                name(n)? == *n
                    && revision.as_str().is_some_and(valid_id)
                    && hashes[n].as_str().is_some_and(valid_id),
                "Invalid checked skill selection"
            );
        }
        return Ok(());
    }
    let allowed: &[&str] = match operation {
        "list" | "recover" => &[],
        "show" => &["skill"],
        "learn" => &[
            "skill",
            "text",
            "author",
            "date",
            "description",
            "section",
            "expectedRevision",
        ],
        "confirm" => &["skill", "author", "takeaway", "expectedRevision"],
        "fold" => &["skill", "confirmed", "expectedRevision", "expectedFileHash"],
        "publish" => &["skill", "expectedRevision", "expectedFileHash"],
        _ => bail!("Unknown skill operation"),
    };
    fields(args, allowed)?;
    if !matches!(operation, "list" | "recover") {
        name(required(args, "skill", 4096)?)?;
    }
    if matches!(operation, "learn" | "confirm") {
        required(args, "author", 256)?;
    }
    if operation == "learn" {
        required(args, "text", 64000)?;
    }
    for (key, max) in [("description", 4096), ("section", 256), ("takeaway", 4096)] {
        if let Some(value) = args.get(key) {
            let value = value
                .as_str()
                .with_context(|| format!("Skill {key} must be string"))?;
            text(value, max)?;
            if key == "section" {
                ensure!(!value.contains(['\n', '\r']), "Section must be one line");
            }
        }
    }
    if let Some(value) = args.get("date") {
        validate_date(value.as_str().context("Skill date must be string")?)?;
    }
    if let Some(value) = args.get("confirmed") {
        ensure!(value.is_boolean(), "Confirmed must be boolean");
    }
    if matches!(operation, "learn" | "confirm" | "fold" | "publish") {
        ensure!(
            args.get("expectedRevision")
                .is_some_and(|v| v.is_null() || v.as_str().is_some_and(valid_id)),
            "Expected skill revision must be null or SHA256 identity"
        );
    }
    if matches!(operation, "fold" | "publish") {
        ensure!(
            args.get("expectedFileHash")
                .is_some_and(|v| v.is_null() || v.as_str().is_some_and(valid_id)),
            "Expected skill file hash required"
        );
    }
    Ok(())
}
fn query(a: &SkillAccess<'_>, operation: &str, args: &Value, d: &Deadline) -> Result<Value> {
    if operation == "list" {
        fields(args, &[])?;
        let mut skills = vec![];
        for n in names(a, d)? {
            if let Some(o) = observed(a, &n, d)? {
                skills.push(public(a, &o, d)?);
            }
        }
        let value = json!({"count":skills.len(),"skills":skills});
        serialized(&value, 4_000_000)?;
        return Ok(value);
    }
    fields(args, &["skill"])?;
    let n = name(required(args, "skill", 4096)?)?;
    let o = observed(a, &n, d)?.context("Unknown skill")?;
    show_result(a, &o, d)
}
fn show_result(a: &SkillAccess<'_>, o: &Observed, d: &Deadline) -> Result<Value> {
    let n = &o.state.name;
    let (_, _, body) = parse_markdown(&o.state.markdown, d)?;
    let mut output = format!(
        "# {} (v{}, {})\n\n{}\n",
        n,
        o.state.version,
        if o.state.location == "repo" {
            "the team's"
        } else {
            "kept on this machine"
        },
        body
    );
    let mut waiting: Vec<_> = o
        .state
        .takeaways
        .iter()
        .filter(|t| t.folded_in.is_none())
        .collect();
    waiting.sort_by(|x, y| y.date.cmp(&x.date).then(y.file.cmp(&x.file)));
    if !waiting.is_empty() {
        output.push_str(&format!("\n## Corrections since v{} (newest first; these win where they disagree with the above)\n",o.state.version));
        for t in waiting {
            d.check()?;
            let line = format!(
                "- {} ({}, {})\n",
                t.text.lines().map(str::trim).collect::<Vec<_>>().join(" "),
                t.taught_by,
                short_date(&t.date)
            );
            ensure!(
                line.len() <= 4_000_000usize.saturating_sub(output.len()),
                "Skill view exceeds output capacity"
            );
            output.push_str(&line);
        }
    }
    let mut value =
        json!({"skill":public(a,o,d)?,"revisionId":o.token,"contentHash":o.content_hash});
    value["text"] = json!(output);
    serialized(&value, 4_000_000)?;
    Ok(value)
}
fn mutate(
    a: &SkillAccess<'_>,
    operation: &str,
    args: &Value,
    d: &Deadline,
    preview_only: bool,
) -> Result<Value> {
    if operation == "fold" && args.get("skill").is_none() {
        let revisions = args["expectedRevisions"]
            .as_object()
            .context("Expected revision map absent")?;
        let hashes = args["expectedFileHashes"]
            .as_object()
            .context("Expected hash map absent")?;
        let discovered = names(a, d)?;
        ensure!(
            discovered.iter().eq(revisions.keys()),
            "Checked skill selection differs from current library"
        );
        let mut selected = Vec::new();
        let mut preflight = Vec::new();
        for n in discovered {
            let o = observed(a, &n, d)?.context("Selected skill absent")?;
            let one = json!({"skill":n,"confirmed":args["confirmed"]==true,"expectedRevision":revisions[&n],"expectedFileHash":hashes[&n]});
            expected(&one, Some(&o))?;
            file_expected(&one, &o)?;
            mutate(a, "fold", &one, d, true)?;
            ensure!(
                o.state.location != "repo" || a.repository_writes,
                "Repository skill fold authority unavailable"
            );
            let ready: Vec<_> = o
                .state
                .takeaways
                .iter()
                .filter(|t| {
                    t.folded_in.is_none()
                        && (args["confirmed"] != true
                            || t.confirmed_by
                                .iter()
                                .any(|who| who.to_lowercase() != t.taught_by.to_lowercase()))
                })
                .map(|t| t.id.clone())
                .collect();
            preflight.push(json!({"name":n,"from":o.state.version,"to":o.state.version+u64::from(!ready.is_empty()),"folded":ready,"revisionId":"0".repeat(64),"contentHash":"0".repeat(64)}));
            selected.push(one);
        }
        serialized(
            &json!({"results":preflight,"count":selected.len(),"atomicAcrossSkills":false}),
            4_000_000,
        )?;
        let mut results = Vec::new();
        for one in selected {
            d.check()?;
            let output = mutate(a, "fold", &one, d, false)?;
            let ids: Vec<_> = output["folded"]
                .as_array()
                .context("Folded results absent")?
                .iter()
                .map(|t| t["id"].clone())
                .collect();
            results.push(json!({"name":one["skill"],"from":output["from"],"to":output["to"],"folded":ids,"revisionId":output["revisionId"],"contentHash":output["contentHash"]}));
        }
        return Ok(json!({"count":results.len(),"results":results,"atomicAcrossSkills":false}));
    }
    let allowed = match operation {
        "learn" => vec![
            "skill",
            "text",
            "author",
            "date",
            "description",
            "section",
            "expectedRevision",
        ],
        "confirm" => vec!["skill", "author", "takeaway", "expectedRevision"],
        "fold" => vec!["skill", "confirmed", "expectedRevision", "expectedFileHash"],
        "publish" => vec!["skill", "expectedRevision", "expectedFileHash"],
        _ => bail!("Unsupported skill mutation"),
    };
    fields(args, &allowed)?;
    let n = name(required(args, "skill", 4096)?)?;
    let before = observed(a, &n, d)?;
    expected(args, before.as_ref())?;
    if operation != "learn" && before.is_none() {
        return Ok(match operation {
            "confirm" => json!({"skill":null,"confirmed":[],"reason":"unknown"}),
            "fold" => json!({"skill":null,"folded":[],"sections":[],"reason":"unknown"}),
            "publish" => json!({"ok":false,"reason":"unknown"}),
            _ => bail!("Unknown skill"),
        });
    }
    let mut state = before
        .as_ref()
        .map(|o| o.state.clone())
        .unwrap_or_else(|| State {
            name: n.clone(),
            description: String::new(),
            version: 1,
            markdown: String::new(),
            location: "personal".into(),
            takeaways: vec![],
        });
    let mut extras = vec![];
    let mut detail = json!({});
    match operation {
        "learn" => {
            let author = required(args, "author", 256)?;
            let value = required(args, "text", 64000)?;
            let today = time::OffsetDateTime::now_utc();
            let date = args
                .get("date")
                .map(|v| v.as_str().context("Skill date must be string"))
                .transpose()?
                .map(str::to_string)
                .unwrap_or_else(|| {
                    format!(
                        "{:04}-{:02}-{:02}",
                        today.year(),
                        u8::from(today.month()),
                        today.day()
                    )
                });
            validate_date(&date)?;
            let section = args
                .get("section")
                .map(|v| v.as_str().context("Section must be string"))
                .transpose()?
                .map(str::trim)
                .map(str::to_string);
            if let Some(section) = &section {
                text(section, 256)?;
                ensure!(!section.contains(['\n', '\r']), "Section must be one line");
            }
            if before.is_none() {
                let description = args
                    .get("description")
                    .map(|v| v.as_str().context("Description must be string"))
                    .transpose()?
                    .map(str::trim)
                    .map(str::to_string)
                    .unwrap_or_else(|| {
                        format!(
                            "What has been learned about {} in this repo",
                            n.replace('-', " ")
                        )
                    });
                text(&description, 4096)?;
                state.markdown = format!(
                    "---\nname: {n}\ndescription: {}\nversion: 1\n---\n\n# {n}\n\n{description}.\n",
                    json!(description)
                );
            }
            ensure!(
                state.takeaways.len() < 100_000,
                "Takeaway count capacity exceeded"
            );
            let mut index = 1;
            let author_slug = name(author)
                .unwrap_or_else(|_| "someone".into())
                .split('-')
                .next()
                .unwrap_or("someone")
                .to_string();
            while state
                .takeaways
                .iter()
                .any(|t| t.file == format!("{date}-{author_slug}-{index}.md"))
            {
                index += 1;
            }
            let id = identity(&(
                &a.project,
                &n,
                &before.as_ref().map(|o| &o.token),
                value,
                author,
                &date,
                index,
            ));
            let takeaway = Takeaway {
                id,
                file: format!("{date}-{author_slug}-{index}.md"),
                taught_by: author.into(),
                date,
                from: "correction".into(),
                confirmed_by: vec![],
                folded_in: None,
                section,
                text: value.into(),
            };
            detail =
                json!({"created":before.is_none(),"takeaway":takeaway_public(a,&n,&takeaway)?});
            state.takeaways.push(takeaway);
        }
        "confirm" => {
            let who = required(args, "author", 256)?;
            let only = args
                .get("takeaway")
                .map(|v| v.as_str().context("Takeaway selector must be string"))
                .transpose()?;
            let mut confirmed = vec![];
            for t in &mut state.takeaways {
                d.check()?;
                let known = a
                    .file("personal", &format!("{n}/takeaways/{}", t.file))?
                    .to_string_lossy()
                    .replace('\\', "/");
                if t.folded_in.is_none()
                    && only.is_none_or(|only| {
                        let only = only.replace('\\', "/");
                        t.id == only
                            || known.ends_with(&only)
                            || known.ends_with(&format!("{only}.md"))
                    })
                    && t.taught_by.to_lowercase() != who.to_lowercase()
                    && !t
                        .confirmed_by
                        .iter()
                        .any(|v| v.to_lowercase() == who.to_lowercase())
                {
                    t.confirmed_by.push(who.into());
                    confirmed.push(t.clone());
                }
            }
            if confirmed.is_empty() {
                let mut output = result(a, &n, d)?;
                output["confirmed"] = json!([]);
                return Ok(output);
            }
            let confirmed: Vec<_> = confirmed
                .iter()
                .map(|t| takeaway_public(a, &n, t))
                .collect::<Result<_>>()?;
            detail = json!({"confirmed":confirmed});
        }
        "fold" => {
            ensure!(
                args.get("confirmed").is_none_or(Value::is_boolean),
                "Confirmed must be boolean"
            );
            let confirmed = args["confirmed"] == true;
            let ready: Vec<_> = state
                .takeaways
                .iter()
                .filter(|t| {
                    t.folded_in.is_none()
                        && (!confirmed
                            || t.confirmed_by
                                .iter()
                                .any(|c| c.to_lowercase() != t.taught_by.to_lowercase()))
                })
                .cloned()
                .collect();
            let still = state
                .takeaways
                .iter()
                .filter(|t| t.folded_in.is_none())
                .count()
                - ready.len();
            let from = state.version;
            if ready.is_empty() {
                let mut output = result(a, &n, d)?;
                output["folded"] = json!([]);
                output["from"] = json!(from);
                output["to"] = json!(from);
                output["stillWaiting"] = json!(still);
                output["sections"] = json!([]);
                return Ok(output);
            }
            file_expected(args, before.as_ref().unwrap())?;
            ensure!(
                state.location != "repo" || a.repository_writes,
                "Host repository skill write authority required"
            );
            let (_, _, body) = parse_markdown(&state.markdown, d)?;
            let mut lines: Vec<String> = body.lines().map(str::to_string).collect();
            let mut sections = vec![];
            for t in &ready {
                let heading = t.section.as_deref().unwrap_or("Takeaways");
                let at = if let Some(at) = lines.iter().position(|l| {
                    l.trim().to_lowercase() == format!("## {}", heading.to_lowercase())
                }) {
                    at
                } else {
                    while lines.last().is_some_and(|l| l.trim().is_empty()) {
                        lines.pop();
                    }
                    lines.push(String::new());
                    lines.push(format!("## {heading}"));
                    lines.len() - 1
                };
                let end = lines
                    .iter()
                    .enumerate()
                    .skip(at + 1)
                    .find(|(_, l)| l.starts_with("## "))
                    .map_or(lines.len(), |(i, _)| i);
                let mut insert = end;
                while insert > at + 1 && lines[insert - 1].trim().is_empty() {
                    insert -= 1;
                }
                lines.insert(
                    insert,
                    format!(
                        "- {} ({}, {})",
                        t.text.lines().map(str::trim).collect::<Vec<_>>().join(" "),
                        t.taught_by,
                        short_date(&t.date)
                    ),
                );
                if !sections.contains(&heading.to_string()) {
                    sections.push(heading.to_string());
                }
            }
            state.version += 1;
            state.markdown = rewrite(&state.markdown, state.version, &lines.join("\n"))?;
            for t in &mut state.takeaways {
                if ready.iter().any(|r| r.id == t.id) {
                    t.folded_in = Some(state.version);
                }
            }
            let ready_ids: BTreeSet<_> = ready.iter().map(|t| t.id.as_str()).collect();
            let folded: Vec<_> = state
                .takeaways
                .iter()
                .filter(|t| ready_ids.contains(t.id.as_str()))
                .map(|t| takeaway_public(a, &n, t))
                .collect::<Result<_>>()?;
            detail = json!({"from":from,"to":state.version,"folded":folded,"stillWaiting":still,"sections":sections});
        }
        "publish" => {
            if state.location == "repo" {
                let mut output = result(a, &n, d)?;
                output["ok"] = json!(false);
                output["reason"] = json!("in-repo");
                return Ok(output);
            }
            ensure!(
                a.repository_writes,
                "Host repository skill write authority required"
            );
            file_expected(args, before.as_ref().unwrap())?;
            ensure!(
                read_file(&a.file("repo", &format!("{n}/SKILL.md"))?, d)?.is_none(),
                "Repository skill target exists"
            );
            extras.push(stage_change(
                a,
                "personal",
                format!("{n}/SKILL.md"),
                Some(
                    &before
                        .as_ref()
                        .context("Missing observed skill")?
                        .content_hash,
                ),
                None,
                d,
            )?);
            state.location = "repo".into();
            detail = json!({"ok":true});
        }
        _ => bail!("Unknown skill mutation"),
    }
    let preview = Observed {
        content_hash: hash(state.markdown.as_bytes()),
        state: state.clone(),
        head: None,
        token: "0".repeat(64),
    };
    let mut preview_result = json!({"skill":public(a,&preview,d)?,"revisionId":preview.token,"contentHash":preview.content_hash});
    for (key, value) in detail.as_object().context("Invalid skill result")? {
        preview_result[key] = value.clone();
    }
    serialized(&preview_result, 4_000_000)?;
    show_result(a, &preview, d)?;
    parse_markdown(&preview.state.markdown, d)?;
    for t in &preview.state.takeaways {
        d.check()?;
        text(&takeaway_markdown(&preview.state.name, t), 1_000_000)?;
    }
    if preview_only {
        return Ok(preview_result);
    }
    save(a, before.as_ref(), state, extras, d)?;
    let mut output = result(a, &n, d)?;
    for (key, value) in detail.as_object().context("Invalid skill result")? {
        output[key] = value.clone();
    }
    serialized(&output, 4_000_000)?;
    Ok(output)
}
fn file_expected(args: &Value, o: &Observed) -> Result<()> {
    ensure!(
        args["expectedFileHash"].as_str() == Some(&o.content_hash),
        "Expected skill file hash conflict"
    );
    Ok(())
}
fn validate_date(value: &str) -> Result<()> {
    let pieces: Vec<_> = value.split('-').collect();
    ensure!(
        pieces.len() == 3 && value.len() == 10,
        "Skill date must be YYYY-MM-DD"
    );
    let year = pieces[0].parse::<i32>()?;
    let month = time::Month::try_from(pieces[1].parse::<u8>()?)?;
    time::Date::from_calendar_date(year, month, pieces[2].parse()?)?;
    Ok(())
}
fn short_date(value: &str) -> String {
    const MONTHS: [&str; 12] = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    let month = value.get(5..7).and_then(|s| s.parse::<usize>().ok());
    let day = value.get(8..10).and_then(|s| s.parse::<u8>().ok());
    match (
        month
            .and_then(|m| m.checked_sub(1))
            .and_then(|m| MONTHS.get(m)),
        day,
    ) {
        (Some(month), Some(day)) => format!("{month} {day}"),
        _ => value.into(),
    }
}
fn rewrite(markdown: &str, version: u64, body: &str) -> Result<String> {
    let lines: Vec<_> = markdown.lines().collect();
    let mut front = if lines.first() == Some(&"---") {
        let close = lines
            .iter()
            .skip(1)
            .position(|line| *line == "---")
            .context("Unclosed skill frontmatter")?
            + 1;
        lines[1..close]
            .iter()
            .map(|s| s.to_string())
            .collect::<Vec<_>>()
    } else {
        vec![]
    };
    if let Some(line) = front.iter_mut().find(|line| line.starts_with("version:")) {
        *line = format!("version: {version}");
    } else {
        front.push(format!("version: {version}"));
    }
    Ok(format!(
        "---\n{}\n---\n\n{}\n",
        front.join("\n"),
        body.trim()
    ))
}

pub(crate) fn validate_retention_record(
    store: &Store,
    kind: &str,
    id: &str,
    value: &Value,
    d: &Deadline,
) -> Result<Option<Vec<String>>> {
    if ![REVISION_KIND, HEAD_KIND, OPERATION_KIND, RECEIPT_KIND].contains(&kind) {
        return Ok(None);
    }
    let stored: Value = record(store, kind, id, d)?.context("Skill retention record missing")?;
    ensure!(stored == *value, "Skill retention record changed");
    let mut artifacts = BTreeSet::new();
    let r = match kind {
        REVISION_KIND => revision(store, id, d)?,
        HEAD_KIND => {
            let head: Head = crate::json::decode(value.clone())?;
            ensure!(
                head.schema_version == 1
                    && valid_id(&head.project_id)
                    && name(&head.name)? == head.name
                    && identity(&(&head.project_id, &head.name)) == id,
                "Skill retention head binding mismatch"
            );
            let r = revision(store, &head.revision, d)?;
            ensure!(
                r.project_id == head.project_id && r.name == head.name,
                "Skill head revision mismatch"
            );
            r
        }
        OPERATION_KIND => {
            let op: Operation = crate::json::decode(value.clone())?;
            let r = validate_operation(store, id, &op, d)?;
            for file in &op.files {
                artifacts.extend(file.before.iter().cloned());
                artifacts.extend(file.after.iter().cloned());
            }
            r
        }
        RECEIPT_KIND => {
            let receipt: Receipt = crate::json::decode(value.clone())?;
            ensure!(
                receipt.schema_version == 1
                    && receipt.operation_id == id
                    && ["prepared", "applied"].contains(&receipt.status.as_str()),
                "Invalid skill retention receipt"
            );
            let op: Operation =
                record(store, OPERATION_KIND, id, d)?.context("Skill receipt operation absent")?;
            validate_operation(store, id, &op, d)?
        }
        _ => bail!("Unknown skill record"),
    };
    let mut bytes = vec![r.state.markdown.into_bytes()];
    bytes.extend(
        r.state
            .takeaways
            .iter()
            .map(|t| takeaway_markdown(&r.name, t).into_bytes()),
    );
    for bytes in bytes {
        d.check()?;
        let id = hash(&bytes);
        ensure!(
            store.read_artifact_limited(&id, 1_000_000)? == bytes,
            "Skill projection artifact differs"
        );
        artifacts.insert(id);
    }
    Ok(Some(artifacts.into_iter().collect()))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn a_large_applied_receipt_prefix_cannot_hide_pending_journals() {
        let personal = tempfile::tempdir().unwrap();
        let repo = tempfile::tempdir().unwrap();
        let store = Store::open(personal.path()).unwrap();
        let project = hash(b"receipt capacity");
        let a = SkillAccess::new(&store, &project, repo.path(), true, true).unwrap();
        let d = Deadline::new(60000);
        store.db.execute_batch("WITH RECURSIVE n(i) AS (VALUES(1) UNION ALL SELECT i+1 FROM n WHERE i<10002) INSERT INTO records(kind,id,value) SELECT 'native_skill_operation_receipt_v1',printf('%064x',i),json_object('schemaVersion',1,'operationId',printf('%064x',i),'status','applied') FROM n;").unwrap();
        assert!(
            pending(&a, &d).is_err(),
            "A bounded scan silently accepted an incomplete receipt history"
        );
    }
    #[test]
    fn expanded_yaml_frontmatter_is_rejected_before_alias_materialization() {
        let mut yaml = String::from("---\nname: bounded\nversion: 1\na0: &a0 [word,word]\n");
        for index in 1..15 {
            yaml.push_str(&format!(
                "a{index}: &a{index} [*a{},*a{}]\n",
                index - 1,
                index - 1
            ));
        }
        yaml.push_str("---\n# bounded\n");
        assert!(
            parse_markdown(&yaml, &Deadline::new(60000)).is_err(),
            "Expanded YAML exceeded the shared 20000-node admission contract"
        );
    }
    #[test]
    fn manual_edit_after_observation_is_never_adopted_as_a_write_precondition() {
        let personal = tempfile::tempdir().unwrap();
        let repository = tempfile::tempdir().unwrap();
        let store = Store::open(personal.path()).unwrap();
        let project = hash(b"race proof");
        let access = SkillAccess::new(&store, &project, repository.path(), true, true).unwrap();
        let d = Deadline::new(60000);
        invoke(&access,"learn",&json!({"skill":"race","text":"Original takeaway.","author":"Ada","expectedRevision":null}),&d).unwrap();
        let before = observed(&access, "race", &d).unwrap().unwrap();
        let mut next = before.state.clone();
        next.version += 1;
        next.markdown = rewrite(&next.markdown, next.version, "New folded body").unwrap();
        let path = access.file("personal", "race/SKILL.md").unwrap();
        fs::write(&path, "# Concurrent human edit\n").unwrap();
        assert!(
            save(&access, Some(&before), next, vec![], &d).is_err(),
            "A newly observed foreign edit was adopted as operation.before"
        );
        assert_eq!(
            fs::read_to_string(path).unwrap(),
            "# Concurrent human edit\n"
        );
    }
}
